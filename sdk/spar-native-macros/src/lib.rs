//! `#[spar_native::module]` and `#[spar_native::function]`.
//!
//! ```ignore
//! #[spar_native::module(name = "fastArray", version = "0.1.0")]
//! mod fast_array {
//!     #[spar_native::function]
//!     fn sum(values: &[f64]) -> f64 { values.iter().sum() }
//! }
//! ```
//! The module attribute rewrites the inline module: every function marked `#[function]` keeps its
//! body and gains an `extern "C"` trampoline that validates arity, converts arguments
//! (`FromSpar`), contains panics, converts the result (`IntoSpar`) and reports errors. It also
//! emits the single exported `spar_native_module_v1` symbol.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    parse::Parser, parse_macro_input, punctuated::Punctuated, spanned::Spanned, Attribute, FnArg,
    Item, ItemMod, Lit, Meta, Pat, ReturnType, Token,
};

fn snake_to_camel(name: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for (i, c) in name.chars().enumerate() {
        if c == '_' {
            upper = i != 0;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn is_function_attr(attr: &Attribute) -> bool {
    let path = attr.path();
    path.segments
        .last()
        .map(|s| s.ident == "function")
        .unwrap_or(false)
        && (path.segments.len() == 1
            || path
                .segments
                .iter()
                .any(|s| s.ident == "spar_native" || s.ident == "spar"))
}

fn lit_str_arg(meta: &Punctuated<Meta, Token![,]>, key: &str) -> Option<String> {
    meta.iter().find_map(|m| match m {
        Meta::NameValue(nv) if nv.path.is_ident(key) => match &nv.value {
            syn::Expr::Lit(syn::ExprLit {
                lit: Lit::Str(s), ..
            }) => Some(s.value()),
            _ => None,
        },
        _ => None,
    })
}

fn parse_args(tokens: proc_macro2::TokenStream) -> syn::Result<Punctuated<Meta, Token![,]>> {
    Punctuated::<Meta, Token![,]>::parse_terminated.parse2(tokens)
}

/// Marker; the enclosing `#[module]` consumes it. Using it outside a module is an error.
#[proc_macro_attribute]
pub fn function(_args: TokenStream, item: TokenStream) -> TokenStream {
    let f = parse_macro_input!(item as syn::ItemFn);
    syn::Error::new(
        f.sig.ident.span(),
        "#[spar_native::function] must be inside a #[spar_native::module] module",
    )
    .to_compile_error()
    .into()
}

#[proc_macro_attribute]
pub fn module(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = match parse_args(args.into()) {
        Ok(a) => a,
        Err(e) => return e.to_compile_error().into(),
    };
    let mut module = parse_macro_input!(item as ItemMod);
    match expand_module(&args, &mut module) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

struct FnInfo {
    ident: syn::Ident,
    spar_name: String,
    tramp: syn::Ident,
    arg_pats: Vec<syn::Ident>,
    arg_names: Vec<String>,
    arg_tys: Vec<syn::Type>,
    ret: Option<syn::Type>,
    asynchronous: bool,
}

fn expand_module(
    args: &Punctuated<Meta, Token![,]>,
    module: &mut ItemMod,
) -> syn::Result<proc_macro2::TokenStream> {
    let spar_module_name =
        lit_str_arg(args, "name").unwrap_or_else(|| snake_to_camel(&module.ident.to_string()));
    let version = lit_str_arg(args, "version").unwrap_or_else(|| "0.0.0".into());
    let mut ver = version.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
    let (v_major, v_minor, v_patch) = (
        ver.next().unwrap_or(0),
        ver.next().unwrap_or(0),
        ver.next().unwrap_or(0),
    );
    let module_span = module.span();
    let module_ident_span = module.ident.span();
    let (_, items) = module.content.as_mut().ok_or_else(|| {
        syn::Error::new(
            module_span,
            "#[module] needs an inline module: `mod name { ... }`",
        )
    })?;

    let mut infos = Vec::new();
    for item in items.iter_mut() {
        let Item::Fn(f) = item else { continue };
        let Some(pos) = f.attrs.iter().position(is_function_attr) else {
            continue;
        };
        let attr = f.attrs.remove(pos);
        let fn_args = match &attr.meta {
            Meta::List(l) => parse_args(l.tokens.clone())?,
            _ => Punctuated::new(),
        };
        let asynchronous = fn_args
            .iter()
            .any(|meta| matches!(meta, Meta::Path(path) if path.is_ident("asynchronous")));
        let ident = f.sig.ident.clone();
        let spar_name =
            lit_str_arg(&fn_args, "name").unwrap_or_else(|| snake_to_camel(&ident.to_string()));
        let mut arg_pats = Vec::new();
        let mut arg_names = Vec::new();
        let mut arg_tys = Vec::new();
        for (i, input) in f.sig.inputs.iter().enumerate() {
            let FnArg::Typed(pt) = input else {
                return Err(syn::Error::new(
                    input.span(),
                    "native functions cannot take `self`",
                ));
            };
            let name = match &*pt.pat {
                Pat::Ident(pi) => snake_to_camel(&pi.ident.to_string()),
                _ => format!("arg{i}"),
            };
            arg_pats.push(format_ident!("__a{}", i));
            arg_names.push(name);
            arg_tys.push((*pt.ty).clone());
        }
        let ret = match &f.sig.output {
            ReturnType::Default => None,
            ReturnType::Type(_, t) => Some((**t).clone()),
        };
        infos.push(FnInfo {
            tramp: format_ident!("__spar_tramp_{}", ident),
            ident,
            spar_name,
            arg_pats,
            arg_names,
            arg_tys,
            ret,
            asynchronous,
        });
    }
    if infos.is_empty() {
        return Err(syn::Error::new(
            module_ident_span,
            "module has no #[spar_native::function] items",
        ));
    }

    let mut generated = proc_macro2::TokenStream::new();
    for info in &infos {
        let FnInfo {
            ident,
            tramp,
            arg_pats,
            arg_tys,
            spar_name,
            ..
        } = info;
        let argc = arg_pats.len();
        let idx = (0..argc).collect::<Vec<_>>();
        let fname_lit = format!("{spar_module_name}::{spar_name}");
        generated.extend(quote! {
            #[allow(non_snake_case)]
            unsafe extern "C" fn #tramp(
                __env: *mut ::spar_native::sys::SparEnv,
                _ud: *mut ::core::ffi::c_void,
                __argv: *const ::spar_native::sys::SparValue,
                __argc: u64,
                __out: *mut ::spar_native::sys::SparValue,
            ) -> i32 {
                ::spar_native::__private::run(#fname_lit, __env, __argv, __argc, #argc, __out, |__cx, __args| {
                    #( let #arg_pats: #arg_tys = ::spar_native::FromSpar::from_spar(&__cx, __args[#idx])?; )*
                    let __r = #ident(#(#arg_pats),*);
                    ::spar_native::IntoSpar::into_spar(__r, &__cx)
                })
            }
        });
    }

    let registrations = infos.iter().map(|i| {
        let FnInfo {
            tramp,
            spar_name,
            arg_names,
            arg_tys,
            ret,
            asynchronous,
            ..
        } = i;
        let ret_ty = match ret {
            None => quote! { <() as ::spar_native::IntoSpar<'static>>::SPAR_TYPE },
            Some(t) => quote! { <#t as ::spar_native::IntoSpar<'static>>::SPAR_TYPE },
        };
        quote! {
            ::spar_native::__private::register(
                __api, __module, #spar_name,
                &[ #( (#arg_names, <#arg_tys as ::spar_native::FromSpar<'static>>::SPAR_TYPE) ),* ],
                #ret_ty,
                #tramp,
                if #asynchronous { ::spar_native::sys::SPAR_FN_ASYNC } else { 0 },
            )?;
        }
    });

    let uses_async = infos.iter().any(|function| function.asynchronous);
    let name_len = spar_module_name.len() as u64;
    generated.extend(quote! {
        unsafe extern "C" fn __spar_module_init(
            __api: *const ::spar_native::sys::SparApiV0,
            __module: *mut ::spar_native::sys::SparModule,
            __state: *mut *mut ::core::ffi::c_void,
        ) -> i32 {
            ::spar_native::__private::init(__api, __state, || {
                #(#registrations)*
                Ok(())
            })
        }

        static __SPAR_DESCRIPTOR: ::spar_native::sys::SparModuleDescriptor = ::spar_native::sys::SparModuleDescriptor {
            struct_size: ::core::mem::size_of::<::spar_native::sys::SparModuleDescriptor>() as u32,
            abi_major: ::spar_native::sys::SPAR_NATIVE_ABI_MAJOR,
            min_abi_minor: ::spar_native::sys::SPAR_NATIVE_ABI_MINOR,
            required_capabilities: ::spar_native::DEFAULT_REQUIRED_CAPABILITIES
                | if #uses_async { ::spar_native::sys::SPAR_CAP_ASYNC } else { 0 },
            optional_capabilities: 0,
            module_name: #spar_module_name.as_ptr(),
            module_name_len: #name_len,
            target: ::core::ptr::null(),
            target_len: 0,
            version_major: #v_major,
            version_minor: #v_minor,
            version_patch: #v_patch,
            reserved0: 0,
            init: Some(__spar_module_init),
            quiesce: None,
            destroy: None,
            reserved: [0; 4],
        };

        #[unsafe(no_mangle)]
        pub extern "C" fn spar_native_module_v1() -> *const ::spar_native::sys::SparModuleDescriptor {
            &__SPAR_DESCRIPTOR
        }
    });

    let file: syn::File = syn::parse2(generated)?;
    let (_, items) = module.content.as_mut().unwrap();
    items.extend(file.items);
    Ok(quote! { #module })
}
