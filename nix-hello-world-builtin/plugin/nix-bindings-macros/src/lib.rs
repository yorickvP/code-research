//! Proc-macro for registering Nix builtins (primops) from Rust plugins.
//!
//! # Overview
//!
//! Writing a Nix plugin builtin in raw `nix-bindings-sys` requires ~40 lines of
//! boilerplate: creating a context, building null-terminated C string arrays,
//! calling `nix_alloc_primop` and `nix_register_primop`, error-checking every
//! step, and (for plugins) placing the whole thing in a `#[ctor]` constructor.
//! This crate reduces that to a single attribute.
//!
//! # Usage — library (manual registration)
//!
//! For a Rust binary or library that embeds Nix and calls the registration
//! function explicitly at startup, omit `auto_register`:
//!
//! ```ignore
//! use nix_bindings_macros::nix_builtin;
//! use nix_bindings_sys::{nix_c_context, EvalState, nix_value, nix_init_string};
//! use std::ffi::CString;
//!
//! #[nix_builtin(
//!     name = "helloWorld",
//!     args = ["_"],
//!     doc  = "builtins.helloWorld\n\nReturns \"Hello, World!\"."
//! )]
//! unsafe fn hello_world(
//!     ctx:    *mut nix_c_context,
//!     _state: *mut EvalState,
//!     _args:  *mut *mut nix_value,
//!     ret:    *mut nix_value,
//! ) {
//!     let msg = CString::new("Hello, World!").unwrap();
//!     nix_init_string(ctx, ret, msg.as_ptr());
//! }
//!
//! fn main() {
//!     // Register before creating any EvalState:
//!     register_hello_world().expect("failed to register helloWorld builtin");
//! }
//! ```
//!
//! # Usage — plugin (automatic registration)
//!
//! For a `cdylib` plugin loaded by Nix via `--plugin-files`, set
//! `auto_register = true`. A `#[ctor::ctor]` constructor is added that calls
//! the registration function when `dlopen` loads the `.so`, before any
//! `EvalState` is created. The `ctor` crate must be in your `[dependencies]`.
//!
//! ```ignore
//! #[nix_builtin(
//!     name         = "helloWorld",
//!     args         = ["_"],
//!     doc          = "builtins.helloWorld\n\nReturns \"Hello, World!\".",
//!     auto_register = true
//! )]
//! unsafe fn hello_world( /* ... */ ) { /* ... */ }
//! ```
//!
//! # Expansion
//!
//! The macro always generates:
//!
//! 1. `unsafe extern "C" fn <your_fn_name>(_user_data, ctx, state, args, ret)` —
//!    the function body unchanged, with `_nix_user_data` prepended to match
//!    the C `PrimOpFun` typedef.
//! 2. `fn register_<your_fn_name>() -> Result<(), String>` — a safe Rust
//!    function you can call explicitly to register the primop.
//!
//! With `auto_register = true` it additionally generates:
//!
//! 3. `#[ctor::ctor] fn __nix_register_<your_fn_name>()` — calls
//!    `register_<your_fn_name>()` on library load.
//!
//! # Attribute arguments
//!
//! | Key             | Type            | Required | Description |
//! |-----------------|-----------------|----------|-------------|
//! | `name`          | string literal  | yes      | The Nix builtin name (e.g. `"helloWorld"`). |
//! | `args`          | array of strings | yes     | Argument names. Length determines arity. |
//! | `doc`           | string literal  | yes      | Docstring shown by `:doc builtins.helloWorld`. |
//! | `auto_register` | bool literal    | no       | Defaults to `false`. Set to `true` for plugins. |
//!
//! # Function signature
//!
//! The annotated function must be `unsafe fn` with exactly four parameters
//! matching the last four fields of `PrimOpFun`:
//!
//! ```text
//! unsafe fn my_builtin(
//!     ctx:   *mut nix_c_context,    // use for nix_init_* / nix_get_* calls
//!     state: *mut EvalState,         // evaluator state (prefix _ to suppress warnings)
//!     args:  *mut *mut nix_value,    // input arguments, index 0..arity-1
//!     ret:   *mut nix_value,         // write your result here
//! )
//! ```

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{
    Expr, ExprArray, ExprLit, ItemFn, Lit, LitBool, LitStr,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    Token,
};

// ---------------------------------------------------------------------------
// Attribute argument parsing
// ---------------------------------------------------------------------------

struct NixBuiltinArgs {
    name:          LitStr,
    args:          Vec<LitStr>,
    doc:           LitStr,
    auto_register: bool,
}

struct KvPair {
    key:   syn::Ident,
    _eq:   Token![=],
    value: Expr,
}

impl Parse for KvPair {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        Ok(KvPair {
            key:   input.parse()?,
            _eq:   input.parse()?,
            value: input.parse()?,
        })
    }
}

impl Parse for NixBuiltinArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let pairs = Punctuated::<KvPair, Token![,]>::parse_terminated(input)?;

        let mut name:          Option<LitStr> = None;
        let mut args:          Option<Vec<LitStr>> = None;
        let mut doc:           Option<LitStr> = None;
        let mut auto_register: bool = false;

        for pair in pairs {
            let span = pair.key.span();
            match pair.key.to_string().as_str() {
                "name" => {
                    name = Some(require_str_lit(pair.value, "`name`", span)?);
                }
                "doc" => {
                    doc = Some(require_str_lit(pair.value, "`doc`", span)?);
                }
                "args" => {
                    if let Expr::Array(ExprArray { elems, .. }) = pair.value {
                        let mut v = Vec::new();
                        for elem in elems {
                            v.push(require_str_lit(elem, "`args` element", span)?);
                        }
                        args = Some(v);
                    } else {
                        return Err(syn::Error::new(
                            span,
                            "`args` must be an array of string literals, e.g. [\"x\", \"y\"]",
                        ));
                    }
                }
                "auto_register" => {
                    if let Expr::Lit(ExprLit { lit: Lit::Bool(LitBool { value, .. }), .. }) =
                        pair.value
                    {
                        auto_register = value;
                    } else {
                        return Err(syn::Error::new(
                            span,
                            "`auto_register` must be a bool literal (`true` or `false`)",
                        ));
                    }
                }
                other => {
                    return Err(syn::Error::new(
                        span,
                        format!("unknown key `{other}`; expected `name`, `args`, `doc`, or `auto_register`"),
                    ));
                }
            }
        }

        Ok(NixBuiltinArgs {
            name: name.ok_or_else(|| {
                syn::Error::new(Span::call_site(), "missing required attribute `name`")
            })?,
            args: args.ok_or_else(|| {
                syn::Error::new(Span::call_site(), "missing required attribute `args`")
            })?,
            doc: doc.ok_or_else(|| {
                syn::Error::new(Span::call_site(), "missing required attribute `doc`")
            })?,
            auto_register,
        })
    }
}

fn require_str_lit(expr: Expr, ctx: &str, span: Span) -> syn::Result<LitStr> {
    match expr {
        Expr::Lit(ExprLit { lit: Lit::Str(s), .. }) => Ok(s),
        _ => Err(syn::Error::new(span, format!("{ctx} must be a string literal"))),
    }
}

// ---------------------------------------------------------------------------
// The macro itself
// ---------------------------------------------------------------------------

/// Register a Nix builtin (primop) from Rust.
///
/// See the [crate documentation](crate) for full usage and examples.
#[proc_macro_attribute]
pub fn nix_builtin(attr: TokenStream, item: TokenStream) -> TokenStream {
    let macro_args = parse_macro_input!(attr as NixBuiltinArgs);
    let func       = parse_macro_input!(item as ItemFn);

    if func.sig.unsafety.is_none() {
        return syn::Error::new_spanned(
            &func.sig.fn_token,
            "#[nix_builtin] requires an `unsafe fn`",
        )
        .to_compile_error()
        .into();
    }

    let name_lit      = &macro_args.name;
    let doc_lit       = &macro_args.doc;
    let arg_lits      = &macro_args.args;
    let arity         = arg_lits.len();
    let array_len     = arity + 1; // +1 for the null terminator
    let auto_register = macro_args.auto_register;

    let func_name   = &func.sig.ident;
    let func_vis    = &func.vis;
    let func_inputs = &func.sig.inputs;
    let func_body   = &func.block;
    let func_attrs: Vec<_> = func.attrs.iter().collect();

    // `register_hello_world` — safe public registration function.
    let register_fn_name = syn::Ident::new(
        &format!("register_{}", func_name),
        func_name.span(),
    );
    // `__nix_ctor_hello_world` — ctor shim (only when auto_register = true).
    let ctor_name = syn::Ident::new(
        &format!("__nix_ctor_{}", func_name),
        func_name.span(),
    );

    // CString bindings: `let __nix_arg_0 = CString::new("x").unwrap();`
    let arg_idents: Vec<_> = (0..arity)
        .map(|i| syn::Ident::new(&format!("__nix_arg_{i}"), Span::call_site()))
        .collect();

    let arg_cstring_bindings = arg_idents.iter().zip(arg_lits.iter()).map(|(id, lit)| {
        quote! {
            let #id = ::std::ffi::CString::new(#lit)
                .map_err(|_| ::std::format!(
                    "nix_builtin: arg name {:?} contains a nul byte", #lit
                ))?;
        }
    });

    let arg_ptr_exprs = arg_idents.iter().map(|id| quote! { #id.as_ptr() });

    // The optional auto-register ctor block.
    let ctor_block = if auto_register {
        quote! {
            /// Auto-registration constructor generated by `#[nix_builtin(auto_register = true)]`.
            /// Runs when the plugin `.so` is loaded via `dlopen` / `--plugin-files`.
            /// Requires the `ctor` crate in your `[dependencies]`.
            #[::ctor::ctor]
            #[allow(non_snake_case)]
            fn #ctor_name() {
                if let Err(e) = #register_fn_name() {
                    ::std::eprintln!("nix plugin: {e}");
                }
            }
        }
    } else {
        quote! {}
    };

    let expanded = quote! {
        // ── 1. The primop callback ──────────────────────────────────────────
        //
        // `_nix_user_data` is prepended to the user's four parameters so the
        // function matches the C `PrimOpFun` typedef:
        //   void (*PrimOpFun)(void*, nix_c_context*, EvalState*, nix_value**, nix_value*)
        #(#func_attrs)*
        #[allow(non_snake_case)]
        #func_vis unsafe extern "C" fn #func_name(
            _nix_user_data: *mut ::std::os::raw::c_void,
            #func_inputs
        ) #func_body

        // ── 2. Safe registration function ──────────────────────────────────
        //
        // Call this before creating any `EvalState`. In a library/binary you
        // call it explicitly from your startup code. Plugins can instead set
        // `auto_register = true` to have it called automatically on dlopen.
        #[allow(non_snake_case)]
        #func_vis fn #register_fn_name() -> ::std::result::Result<(), ::std::string::String> {
            unsafe {
                let __ctx = ::nix_bindings_sys::nix_c_context_create();
                if __ctx.is_null() {
                    return Err(::std::format!(
                        "nix plugin: failed to create context while registering '{}'",
                        #name_lit
                    ));
                }

                let __name = ::std::ffi::CString::new(#name_lit)
                    .map_err(|_| ::std::format!(
                        "nix_builtin: `name` '{}' contains a nul byte", #name_lit
                    ))?;
                let __doc = ::std::ffi::CString::new(#doc_lit)
                    .map_err(|_| ::std::format!(
                        "nix_builtin: `doc` for '{}' contains a nul byte", #name_lit
                    ))?;

                // Build a null-terminated array of arg-name pointers on the stack.
                // All CStrings must remain alive until after nix_alloc_primop returns.
                #(#arg_cstring_bindings)*
                let __arg_ptrs: [*const ::std::os::raw::c_char; #array_len] = [
                    #(#arg_ptr_exprs,)*
                    ::std::ptr::null(),
                ];

                let __primop = ::nix_bindings_sys::nix_alloc_primop(
                    __ctx,
                    Some(#func_name),
                    #arity as ::std::os::raw::c_int,
                    __name.as_ptr(),
                    // The C param is `const char **` (not `const char * const *`),
                    // so a mut cast is needed even though we never mutate through it.
                    __arg_ptrs.as_ptr() as *mut *const ::std::os::raw::c_char,
                    __doc.as_ptr(),
                    ::std::ptr::null_mut(), // user_data — unused
                );

                if __primop.is_null() {
                    ::nix_bindings_sys::nix_c_context_free(__ctx);
                    return Err(::std::format!(
                        "nix plugin: nix_alloc_primop returned null for '{}'",
                        #name_lit
                    ));
                }

                let __rc = ::nix_bindings_sys::nix_register_primop(__ctx, __primop);
                ::nix_bindings_sys::nix_c_context_free(__ctx);

                if __rc != ::nix_bindings_sys::nix_err_NIX_OK {
                    return Err(::std::format!(
                        "nix plugin: nix_register_primop failed for '{}' (rc={__rc})",
                        #name_lit
                    ));
                }

                Ok(())
            }
        }

        // ── 3. Optional .init_array constructor (auto_register = true) ─────
        #ctor_block
    };

    expanded.into()
}
