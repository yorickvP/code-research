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

## What the high-level `nix-bindings` crate already provides

The companion `nix-bindings` crate (same repo, `nix-bindings/src/lib.rs`) wraps the sys
crate and provides:

- **`Context`** — RAII wrapper around `nix_c_context`; `Drop` calls `nix_c_context_free`.
  Also calls `nix_libutil_init`, `nix_libstore_init`, `nix_libexpr_init` in `Context::new()`.
- **`EvalStateBuilder` / `EvalState`** — builder pattern, RAII drops.
- **`Store` / `StorePath`** — wraps store operations.
- **`Value<'a>`** — struct with `value_type() -> ValueType`, and `as_int()`, `as_float()`,
  `as_bool()`, `as_string()`, `force()`, `force_deep()` methods returning `Result<T>`.
  `Drop` calls `nix_value_decref`. `as_string()` uses `nix_string_realise` (not the awkward
  callback approach).
- **`ValueType`** enum — `Int`, `Float`, `Bool`, `String`, `Path`, `Null`, `Attrs`, `List`,
  `Function`, `External`, `Thunk`.
- **`Error`** enum and `Result<T>` alias — covers `Unknown`, `Overflow`, `KeyNotFound`,
  `EvalError`, `InvalidType`, `NullPointer`, `StringConversion`.
- **`check_err()`** internal helper converting `nix_err` codes to `Result<()>`.

It does NOT expose anything for primop/plugin authoring (`nix_alloc_primop`,
`nix_register_primop` are absent from the high-level API). The `sys` module is re-exported
as `pub mod sys { pub use nix_bindings_sys::*; }` (doc-hidden) as an escape hatch.

## Improvements to nix-bindings for Plugin Authoring

After writing a plugin using the raw `nix-bindings-sys` crate, studying the full generated
bindings (~1200 lines), and reading the high-level crate source, here are the remaining gaps.

### 1. Safe context wrapper that propagates errors as `Result` — MOSTLY DONE

The high-level crate has `Context` (RAII, Drop), `check_err()` converting `nix_err` to
`Result`, and a typed `Error` enum. However, `check_err` loses the human-readable error
message — it maps error codes to generic strings like `"Unknown error"` or
`"Evaluation error"` rather than calling `nix_err_msg(context)` to retrieve the actual
message Nix stored in the context. So `EvalError("Evaluation error")` is returned instead
of e.g. `EvalError("undefined variable 'foo'")`. Fixing this would require threading the
context into `check_err` or storing errors directly on the context struct.

### 2. Primop registration macro or builder — MISSING ENTIRELY

The high-level crate has no primop support at all. `nix_alloc_primop` and
`nix_register_primop` are not wrapped. Plugin authors must drop to `nix_bindings::sys`
(the re-exported `nix-bindings-sys`) and write the full 20-line unsafe registration dance
themselves, including:
- manual CString conversions for name, doc, and each arg name
- building a null-terminated `*mut *const c_char` array with a non-obvious cast
- arity specified as a separate integer redundant with the arg names array
- null-checking the returned `PrimOp *`
- explicit `#[ctor::ctor]` for plugin `.init_array` placement

This is the biggest gap for plugin authoring. A `PrimOpBuilder` or `#[nix_builtin]`
proc-macro would be the highest-leverage addition to the high-level crate:

```rust
#[nix_builtin(name = "helloWorld", args = ["_"], doc = "Returns Hello, World!")]
fn hello_world(_state: &EvalState, _args: &[Value]) -> Result<String> {
    Ok("Hello, World!".into())
}
```

### 3. Typed `NixValue` enum for reading arguments — PARTIAL

The high-level crate has `Value<'a>` with `value_type() -> ValueType` and typed accessors
(`as_int()`, `as_string()`, etc.). `as_string()` uses `nix_string_realise` which is cleaner
than the raw callback approach. `Drop` calls `nix_value_decref`. `ValueType` is a full enum.

What's still missing:
- **No enum-based matching** — you can't `match value.into_typed() { NixValue::Int(n) => ... }`.
  You must call `value_type()` and then the appropriate accessor separately, with two
  potential failure points.
- **No `Attrs` or `List` traversal methods** — `ValueType::Attrs` and `ValueType::List`
  exist as variants but there are no corresponding `as_attrs()`, `get_attr()`, `iter_list()`
  etc. methods on `Value`. Accessing structured values requires dropping to `sys`.
- **No `Clone`** — `Value` cannot be cloned (no `nix_value_incref` wrapping), which
  limits how values can be stored or passed around.

### 4. Smart pointer for GC-managed values — HALF DONE

`Value::Drop` already calls `nix_value_decref`, which is the important half. But `Value`
does not implement `Clone` (which would call `nix_value_incref`), so values cannot be
duplicated or stored independently of the eval state they borrow. The lifetime `Value<'a>`
tied to `&'a EvalState` prevents storing values in structs or returning them from closures.
An `Arc`-like `OwnedValue` that calls incref on clone and decref on drop would decouple
values from the state's lifetime.

### 5. Suppress libc types from the generated bindings — NOT DONE (sys-level issue)

The generated `bindings.rs` still contains ~700 lines of `__u_char`, `__int8_t`, `__dev_t`,
etc. that leak from transitive `#include`s. The high-level crate re-exports `sys` as
doc-hidden, which hides these from docs, but they still bloat the `sys` module and clutter
IDE auto-complete when someone needs to reach into `sys`. The bindgen config in
`nix-bindings-sys/build.rs` should add `allowlist_type("nix_.*")` and
`allowlist_function("nix_.*")` to restrict output to the Nix API surface.

### 6. Fix the `*mut *const c_char` mutability quirk — NOT DONE (upstream C API issue)

`nix_alloc_primop`'s `args` parameter is declared as `const char **` in the C header
(meaning: mutable pointer to const char pointers), but the intent is clearly
`const char * const *` (pointer to an immutable array of const char pointers). Bindgen
faithfully emits `*mut *const c_char`, so callers must cast a `&[*const c_char]` slice's
`.as_ptr()` with `as *mut *const c_char` — an unsafe cast that Rust rightly flags as
suspicious. The fix is a one-line change to `nix_api_value.h` in the Nix repo upstream.

### 7. Version compatibility documentation and enforcement — NOT DONE

`nix-bindings-sys` version 2.32.4 implies Nix 2.32 headers, but does not enforce this.
The bindings are regenerated from whatever Nix dev headers are present at build time; if
the build environment provides Nix 2.30 headers, the crate compiles silently with a
different API surface. Additionally, `nix_2_32` is absent from nixos-25.05 (only goes up
to nix_2_30); users must use nixos-unstable. Neither the crate README nor flake.nix
document this. A build.rs check against `pkg-config --modversion nix-expr-c` and a clearer
README section on nixpkgs requirements would prevent silent mismatch.

## nix-bindings-macros: proc-macro implementation

Created `plugin/nix-bindings-macros/` — a proc-macro crate that generates all
registration boilerplate from a single attribute.

### Design decisions

**Two modes: manual vs auto-register**

`#[ctor]`-based auto-registration only makes sense for `cdylib` plugins. A library
that embeds Nix (e.g. a custom Nix-based tool) would want to call registration at a
specific point in startup code. So the macro always generates a safe
`register_<fn_name>() -> Result<(), String>` function, and auto-registers via `#[ctor]`
only when `auto_register = true` is set.

**User writes 4 params, macro prepends `_nix_user_data`**

The user writes an `unsafe fn` with the 4 meaningful params (`ctx`, `state`, `args`, `ret`).
The macro converts it to `unsafe extern "C" fn` with `_nix_user_data: *mut c_void` prepended
to match the `PrimOpFun` C typedef. This is explicit and the function is valid Rust on its own.

**Generated code (for `auto_register = true`)**

```rust
// 1. The extern "C" primop callback (user body unchanged, _nix_user_data prepended)
unsafe extern "C" fn hello_world(_nix_user_data: *mut c_void, ctx: ..., ...) { ... }

// 2. Safe registration function (always generated)
fn register_hello_world() -> Result<(), String> {
    // creates context, calls nix_alloc_primop + nix_register_primop, frees context
}

// 3. ctor (only when auto_register = true; requires `ctor` in [dependencies])
#[ctor::ctor]
fn __nix_ctor_hello_world() {
    if let Err(e) = register_hello_world() { eprintln!("nix plugin: {e}"); }
}
```

### Verified working

Running `nix eval --plugin-files ./libnix_hello_world_plugin.so --expr 'builtins.helloWorld null'`
produces `"Hello, World!"` with the macro-rewritten plugin.

Symbol inspection confirms `.init_array` has 3 entries (Rust runtime init + our ctor)
and `hello_world` is compiled with the correct `extern "C"` ABI for use as a function pointer.
