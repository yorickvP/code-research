# Notes: Nix Hello World Builtin Plugin

## Goal
Create a Nix plugin using https://github.com/notashelf/nix-bindings that adds a
`builtins.helloWorld` function returning `"Hello, World!"`.

## Research

### nix-bindings library
- Two-crate workspace: `nix-bindings-sys` (raw FFI via bindgen) + `nix-bindings` (safe wrappers)
- Generates bindings from Nix's C API headers: nix_api_value.h, nix_api_expr.h, etc.
- Version 2.32.4 targets Nix 2.32.x

### How Nix Plugins Work
- Nix loads shared libraries (.so) listed in `plugin-files` setting (or `--plugin-files` CLI flag)
- Uses `dlopen()` - so global C constructors (`.init_array`) run at load time
- Primops must be registered BEFORE `EvalState` is created
- Plugin init runs via global constructor before eval state creation

### Key C API functions (from nix_api_value.h)
```c
// Create a primop
PrimOp * nix_alloc_primop(
    nix_c_context * context,
    PrimOpFun fun,       // callback
    int arity,           // number of arguments
    const char * name,   // name (appears under builtins.*)
    const char ** args,  // NULL-terminated array of arg names
    const char * doc,    // docstring
    void * user_data);

// Register a primop as a builtin
nix_err nix_register_primop(nix_c_context * context, PrimOp * primOp);

// Primop callback signature
typedef void (*PrimOpFun)(
    void *user_data,
    nix_c_context *context,
    EvalState *state,
    nix_value **args,
    nix_value *ret);

// Initialize a string value (used in the callback to set return value)
nix_err nix_init_string(nix_c_context * context, nix_value * value, const char * str);
```

### Arity Consideration
- Primops registered via C API must have arity >= 1 (they are functions)
- Chose arity=1 so the builtin is called as `builtins.helloWorld null` → "Hello, World!"
- The single argument is named `_` (ignored)

### Plugin Initialization in Rust
- Used the `ctor` crate which places a function in `.init_array` (same as C `__attribute__((constructor))`)
- The constructor runs when Nix dlopen()s the plugin .so file
- Calls `nix_c_context_create()`, `nix_alloc_primop()`, `nix_register_primop()`

## Build System Design
- Rust `cdylib` crate (produces .so)
- Depends on `nix-bindings-sys = "2.32.4"` for generated FFI types
- `flake.nix` provides build environment with Nix 2.32 dev headers + Rust
- Using `naersk` for Rust builds inside Nix

## Testing (requires Nix)
```bash
nix build
nix eval --plugin-files ./result/lib/libnix_hello_world_plugin.so --expr 'builtins.helloWorld null'
# Expected: "Hello, World!"
```

## Issues / Learnings
- `nix-bindings-sys` auto-generates bindings at build time via bindgen, so Nix dev
  headers must be present during `cargo build`
- The `ctor` crate injects code into `.init_array` section - works on Linux/macOS
- Plugin registration happens at library load, before any eval state is created
- Nix's `nix_api_value.h` contains all the primop-related C API
- `nix-bindings` (high-level) doesn't yet expose primop registration as safe Rust,
  so we use `nix-bindings-sys` (raw FFI) directly

## Build & Test Results

### Fixes Required During Build
- nixpkgs 25.05 does not have `nix_2_32`; switched to `nixpkgs-unstable` which has nix 2.32.6
- `nix_alloc_primop`'s `args` parameter is `*mut *const i8` in generated bindings (not `*const`);
  fixed with `arg_names.as_ptr() as *mut *const c_char` cast

### Successful Build
```
nix develop --command bash -c 'cd plugin && cargo build --release'
# => Finished `release` profile [optimized]
```

### Successful Test
```
nix eval --plugin-files plugin/target/release/libnix_hello_world_plugin.so \
         --expr 'builtins.helloWorld null'
# => "Hello, World!"

nix eval --plugin-files plugin/target/release/libnix_hello_world_plugin.so \
         --expr 'builtins ? helloWorld'
# => true
```

## Improvements to nix-bindings for Plugin Authoring

After writing a plugin using the raw `nix-bindings-sys` crate and studying the full
generated bindings (~1200 lines across the Nix C API surface), here are concrete areas
where a higher-level `nix-bindings` crate could make plugin authoring significantly easier.

### 1. Safe context wrapper that propagates errors as `Result`

Currently every API call takes `*mut nix_c_context` and returns `nix_err` (a raw `c_int`).
Authors must check each return code manually and separately query the context for an error
message. A `NixContext` struct with methods that return `Result<T, NixError>` would make
error handling both safer and more idiomatic:

```rust
// Current state: manual checking
let rc = nix_register_primop(ctx, primop);
if rc != 0 {
    eprintln!("failed: {}", get_error_msg(ctx));
}

// Ideal API
ctx.register_primop(&primop)?;
```

The context should also implement RAII (`Drop`) so it's freed automatically.

### 2. Primop registration macro or builder

The full sequence to register one builtin is:
- Create a context
- Build a null-terminated `*mut *const c_char` array of arg name pointers (with non-obvious
  mutability cast for a parameter that C treats as `const char **` but bindgen emits as `*mut *const c_char`)
- Call `nix_alloc_primop` with arity, name, args, docstring, user_data
- Null-check the returned `PrimOp *`
- Call `nix_register_primop`
- Free the context

A `register_primop!` macro or a `PrimOpBuilder` that encodes the arg names in the type
system (so arity is inferred rather than specified as a separate integer) would cut this
to a few lines and eliminate the unsafe CString juggling. The `#[ctor]` placement requirement
could also be bundled into such a macro.

Ideal form:
```rust
#[nix_builtin(name = "helloWorld", args = ["_"], doc = "Returns Hello, World!")]
fn hello_world(_state: &EvalState, _args: &[NixValue]) -> NixResult {
    NixValue::string("Hello, World!")
}
```

### 3. Typed `NixValue` enum for reading arguments

Primop callbacks receive `*mut *mut nix_value` — an array of opaque pointers. To read
an argument the author must call `nix_get_type` and then the matching `nix_get_*` function
(`nix_get_string` with a callback, `nix_get_int`, `nix_get_bool`, etc.). `nix_get_string`
is particularly awkward: it uses a C callback pattern instead of returning a pointer, because
strings can carry store-path context.

A `NixValue` enum that matches on type and returns safe Rust types would eliminate all of
this:

```rust
pub enum NixValue<'a> {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(Cow<'a, str>),
    Path(PathBuf),
    List(NixList<'a>),
    Attrs(NixAttrs<'a>),
    // ...
}

impl NixValue<'_> {
    pub fn from_raw(ctx: &NixContext, state: &EvalState, v: *mut nix_value) -> Result<Self, NixError>;
}
```

### 4. Smart pointer for GC-managed values

`nix_value` objects are managed by Boehm GC. The API exposes `nix_value_incref` /
`nix_value_decref` and `nix_gc_incref` / `nix_gc_decref`. Plugin authors who allocate
values with `nix_alloc_value` must remember to balance these calls. A `GcValue` smart
pointer (like `Rc` but backed by Nix's GC) with `Clone`/`Drop` impl would make ownership
automatic.

### 5. Suppress libc types from the generated bindings

The generated `bindings.rs` contains ~700 lines of `__u_char`, `__int8_t`, `__dev_t`, etc.
libc typedefs that leak from `#include <stdint.h>` in the Nix headers. These pollute
auto-complete and add noise. The bindgen config should use `allowlist_type` /
`allowlist_function` to restrict output to only `nix_*` names, and replace C primitive
types with Rust equivalents from the `libc` crate.

### 6. Fix the `*mut *const c_char` mutability quirk

`nix_alloc_primop`'s `args` parameter is declared as `const char **` in the C header
(meaning: pointer to array of const char pointers), but bindgen emits it as
`*mut *const c_char` — so a stack-allocated `[*const c_char; N]` array requires an
explicit `as *mut *const c_char` cast that has no safety justification. The C signature
should probably be `const char * const *` (pointer to immutable array), which bindgen would
emit as `*const *const c_char` and eliminate the cast. A bug report or PR upstream is
warranted.

### 7. Version compatibility matrix

`nix-bindings-sys` currently uses its own crate version (2.32.4) to signal which Nix
version's headers it was generated from. But the crate regenerates bindings from whatever
headers are in the build environment, so the version number is aspirational rather than
enforced. A Cargo feature or build.rs check that validates `nix --version` matches the
expected range would catch version skew earlier. The nixpkgs pin (`nixos-unstable` is
required because `nixos-25.05` only goes up to `nix_2_30`) should be documented prominently
in the README.
