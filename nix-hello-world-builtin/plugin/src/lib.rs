//! Nix plugin: adds `builtins.helloWorld` using the Nix C API.
//!
//! When Nix loads this shared library (via `--plugin-files` or the `plugin-files`
//! setting), the `#[ctor]` function runs during library initialization (.init_array),
//! which registers a new primitive operation `helloWorld` into Nix's builtin set.
//!
//! Usage:
//!   nix eval --plugin-files ./libnix_hello_world_plugin.so --expr 'builtins.helloWorld null'
//!   # => "Hello, World!"

use std::ffi::CString;

use nix_bindings_sys::{nix_c_context, EvalState, nix_init_string, nix_value};
use nix_bindings_macros::nix_builtin;

/// Returns `"Hello, World!"` for any input.
///
/// The `#[nix_builtin]` macro generates:
///   - An `unsafe extern "C" fn hello_world` with the `PrimOpFun` ABI
///     (`_nix_user_data` is prepended automatically).
///   - A safe `fn register_hello_world() -> Result<(), String>` you can call
///     at any time before an `EvalState` is created.
///   - A `#[ctor::ctor]` constructor (because `auto_register = true`) that
///     calls `register_hello_world()` when Nix `dlopen`s the plugin.
#[nix_builtin(
    name         = "helloWorld",
    args         = ["_"],
    doc          = "builtins.helloWorld\n\n\
                    Returns the string \"Hello, World!\". \
                    This builtin is provided by the nix-hello-world-plugin.",
    auto_register = true
)]
unsafe fn hello_world(
    ctx:    *mut nix_c_context,
    _state: *mut EvalState,
    _args:  *mut *mut nix_value,
    ret:    *mut nix_value,
) {
    let msg = CString::new("Hello, World!").expect("CString::new failed");
    nix_init_string(ctx, ret, msg.as_ptr());
}
