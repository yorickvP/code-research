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
