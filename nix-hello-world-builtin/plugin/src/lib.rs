//! Nix plugin: adds `builtins.helloWorld` using the Nix C API.
//!
//! When Nix loads this shared library (via `--plugin-files` or the `plugin-files`
//! setting), the `#[ctor]` function runs during library initialization (.init_array),
//! which registers a new primitive operation `helloWorld` into Nix's builtin set.
//!
//! Usage:
//!   nix eval --plugin-files ./libnix_hello_world_plugin.so --expr 'builtins.helloWorld null'
//!   # => "Hello, World!"

use std::ffi::{c_char, c_void, CString};

// Import raw FFI types from the generated nix-bindings-sys crate.
// These match the C types in nix_api_value.h and nix_api_util.h.
use nix_bindings_sys::{
    nix_alloc_primop, nix_c_context, nix_c_context_create, nix_c_context_free,
    nix_init_string, nix_register_primop, EvalState, nix_value,
};

/// The primop implementation: ignores its single argument and returns "Hello, World!".
///
/// Signature must match `PrimOpFun`:
///   void (*PrimOpFun)(void *user_data, nix_c_context *, EvalState *, nix_value **args, nix_value *ret)
unsafe extern "C" fn hello_world_primop(
    _user_data: *mut c_void,
    ctx: *mut nix_c_context,
    _state: *mut EvalState,
    _args: *mut *mut nix_value,
    ret: *mut nix_value,
) {
    let message = CString::new("Hello, World!").expect("CString::new failed");
    nix_init_string(ctx, ret, message.as_ptr());
}

/// Registers the `helloWorld` primop when the shared library is loaded.
///
/// The `#[ctor::ctor]` attribute places this function in the `.init_array` ELF
/// section, so it runs automatically when Nix calls `dlopen()` on the plugin.
/// Primops must be registered before `EvalState` is created, and Nix guarantees
/// this ordering for plugin-files.
#[ctor::ctor]
fn register_hello_world_builtin() {
    unsafe {
        let ctx = nix_c_context_create();
        if ctx.is_null() {
            eprintln!("nix-hello-world-plugin: failed to create nix context");
            return;
        }

        let name = CString::new("helloWorld").expect("CString::new failed");
        let doc = CString::new(
            "builtins.helloWorld\n\n\
             Returns the string \"Hello, World!\". \
             This builtin is provided by the nix-hello-world-plugin.",
        )
        .expect("CString::new failed");

        // NULL-terminated array of argument names (one ignored argument).
        let arg_name = CString::new("_").expect("CString::new failed");
        let arg_names: [*const c_char; 2] = [arg_name.as_ptr(), std::ptr::null()];

        let primop = nix_alloc_primop(
            ctx,
            Some(hello_world_primop),
            1,                          // arity: takes one (ignored) argument
            name.as_ptr(),
            arg_names.as_ptr() as *mut *const c_char,
            doc.as_ptr(),
            std::ptr::null_mut(),       // no user_data needed
        );

        if primop.is_null() {
            eprintln!("nix-hello-world-plugin: nix_alloc_primop returned null");
            nix_c_context_free(ctx);
            return;
        }

        let rc = nix_register_primop(ctx, primop);
        if rc != 0 {
            eprintln!("nix-hello-world-plugin: nix_register_primop failed (rc={})", rc);
        }

        nix_c_context_free(ctx);
    }
}
