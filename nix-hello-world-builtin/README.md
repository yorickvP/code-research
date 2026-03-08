# Nix Hello World Builtin Plugin

A Nix plugin that adds `builtins.helloWorld` to the Nix evaluator, built using
[notashelf/nix-bindings](https://github.com/notashelf/nix-bindings).

## What It Does

After loading the plugin, `builtins.helloWorld null` evaluates to `"Hello, World!"`:

```
$ nix eval --plugin-files ./result/lib/libnix_hello_world_plugin.so \
           --expr 'builtins.helloWorld null'
"Hello, World!"
```

## How It Works

### Nix's Plugin System

Nix supports loading shared libraries (`.so` / `.dylib`) at startup via the
`plugin-files` setting. When a plugin is loaded, Nix calls `dlopen()` on the
shared library, which causes the library's global constructors to run. Crucially,
this happens *before* any `EvalState` is created, so plugins can register new
builtins.

### nix-bindings-sys

The plugin depends on
[notashelf/nix-bindings](https://github.com/notashelf/nix-bindings) — specifically
the `nix-bindings-sys` crate, which contains raw, auto-generated Rust FFI bindings
to Nix's C API. The bindings are generated at build time by `bindgen` from Nix's
public C headers (`nix_api_value.h`, `nix_api_expr.h`, etc.).

The key C API functions used are:

| Function | Purpose |
|---|---|
| `nix_c_context_create()` | Create a Nix error context |
| `nix_alloc_primop(ctx, fn, arity, name, args, doc, data)` | Allocate a primitive operation |
| `nix_register_primop(ctx, primop)` | Register the primop as a builtin |
| `nix_init_string(ctx, value, str)` | Write a string into a Nix value |
| `nix_c_context_free(ctx)` | Free the error context |

### Plugin Initialization

Rust doesn't have a built-in way to run code at library load time, but the
[`ctor`](https://crates.io/crates/ctor) crate places a function in the ELF
`.init_array` section — the same mechanism as C's `__attribute__((constructor))`.

```rust
#[ctor::ctor]
fn register_hello_world_builtin() {
    unsafe {
        let ctx = nix_c_context_create();
        // allocate and register the primop...
        nix_c_context_free(ctx);
    }
}
```

### The Primop Callback

The primop has arity 1 (takes one ignored argument — Nix primops must be functions):

```rust
unsafe extern "C" fn hello_world_primop(
    _user_data: *mut c_void,
    ctx: *mut nix_c_context,
    _state: *mut EvalState,
    _args: *mut *mut nix_value,
    ret: *mut nix_value,
) {
    let message = CString::new("Hello, World!").unwrap();
    nix_init_string(ctx, ret, message.as_ptr());
}
```

The `PrimOpFun` signature comes directly from `nix_api_value.h`:
```c
typedef void (*PrimOpFun)(void *user_data, nix_c_context *context,
                          EvalState *state, nix_value **args, nix_value *ret);
```

## Building

Requires Nix 2.32 (the `nix-bindings-sys` crate version matches the Nix C API version).

### With the Nix Flake (recommended)

```bash
# Build the plugin .so
nix build

# The plugin .so is at:
ls result/lib/libnix_hello_world_plugin.so
```

### In the Dev Shell

```bash
# Enter the dev shell (provides Nix 2.32 headers, Rust, pkg-config)
nix develop

# Build
cargo build --release

# The plugin .so is at:
ls target/release/libnix_hello_world_plugin.so
```

### Build Requirements

| Requirement | Reason |
|---|---|
| Nix 2.32 dev headers | `nix-bindings-sys` calls `pkg-config` for `nix-main-c`, `nix-expr-c`, etc. |
| `libclang` | `bindgen` (used by `nix-bindings-sys`) needs clang to parse C headers |
| `pkg-config` | Locates Nix library headers |
| Rust 1.85+ (edition 2024) | Required by the `nix-bindings-sys` crate |

## Testing

```bash
# Build first (in dev shell or via nix build)
nix eval \
  --plugin-files ./result/lib/libnix_hello_world_plugin.so \
  --expr 'builtins.helloWorld null'
# => "Hello, World!"

# Also works through the flake check
nix flake check
```

## Project Structure

```
plugin/
├── Cargo.toml          # cdylib crate, depends on nix-bindings-sys + ctor
├── Cargo.lock          # Pinned dependency tree
└── src/
    └── lib.rs          # Plugin implementation
flake.nix               # Nix build system (naersk + nixpkgs + nix 2.32)
notes.md                # Research notes
README.md               # This file
```

## Key Design Decisions

**Why `nix-bindings-sys` and not `nix-bindings`?**
The high-level `nix-bindings` crate provides safe wrappers for evaluating
expressions and working with the store, but doesn't yet expose the primop
registration API. Since `nix_alloc_primop` and `nix_register_primop` are only
available at the raw FFI level, `nix-bindings-sys` is the right choice.

**Why arity=1?**
Nix primops registered via `nix_alloc_primop` are functions — they must accept at
least one argument. Arity 0 would need the value to be set directly in the
evaluator state, which isn't exposed via the C API. The convention for
"constant-like" builtins is arity=1 with the argument ignored.

**Why `ctor` crate?**
Nix loads plugin .so files and expects global constructors to register builtins.
The `ctor` crate is the idiomatic Rust way to write `.init_array` entries without
unsafe linker tricks.
