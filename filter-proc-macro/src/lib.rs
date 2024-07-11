use proc_macro::{Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{
    punctuated::Punctuated, Data, DeriveInput, GenericParam, Lifetime, LifetimeParam, Pat, PatIdent,
};

#[proc_macro_derive(Args)]
pub fn derive_args(input: TokenStream) -> TokenStream {
    let ast = syn::parse(input).expect("token stream should be valid");

    derive_args_impl(&ast)
}

fn derive_args_impl(ast: &DeriveInput) -> TokenStream {
    let name = &ast.ident;

    // add 'doc if it is not already present
    let generics = &ast.generics;
    let (_, ty_generics, where_clause) = generics.split_for_impl();
    let mut added_generics = generics.clone();
    if !generics.params.iter().any(|x| match x {
        GenericParam::Lifetime(lt) => lt.lifetime.ident == "doc",
        _ => false,
    }) {
        added_generics
            .params
            .push(GenericParam::Lifetime(LifetimeParam {
                attrs: vec![],
                bounds: Punctuated::new(),
                colon_token: None,
                lifetime: Lifetime::new("'doc", Span::call_site().into()),
            }));
    }

    let (impl_generics, _, _) = added_generics.split_for_impl();

    let Data::Struct(s) = &ast.data else {
        return quote! {
            compile_error!("#[derive(Args)] on a non-struct is not supported.");
        }
        .into();
    };

    let field = s.fields.iter().map(|x| {
        if let Some(id) = &x.ident {
            id
        } else {
            panic!("#[derive(Args)] not supported on a tuple struct")
        }
    });

    let field_extract = field
        .clone()
        .filter(|x| !x.to_string().starts_with("_marker"));

    let field_assign = field.clone().map(|x| {
        if x.to_string().starts_with("_marker") {
            quote! { #x: Default::default() }
        } else {
            quote! { #x }
        }
    });

    quote! {
        impl #impl_generics crate::interpreter::filter::Args<'doc> for #name #ty_generics #where_clause {
            fn try_deserialize<'ast>(
                mut args: ::std::collections::BTreeMap<&'ast str, crate::interpreter::Value<'doc>>
            ) -> anyhow::Result<Self> {
                #(
                    let #field_extract = crate::interpreter::TryFromValue::try_from_option_value(args.remove(stringify!(#field_extract)))?;
                )*

                if !args.is_empty() {
                    anyhow::bail!("Found unexpected arguments {args:?}");
                }

                Ok(Self {
                    #(#field_assign),*
                })
            }
        }
    }
    .into()
}

#[proc_macro_attribute]
pub fn filter_fn(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let func: syn::ItemFn = syn::parse(item).expect("token stream should be valid");
    let inner = func.clone();
    let name = func.sig.ident;

    let (value, args) = func
        .sig
        .inputs
        .into_iter()
        .map(|arg| match arg {
            syn::FnArg::Receiver(_) => panic!("Calling #[filter_fn] on a method"),
            syn::FnArg::Typed(x) => match *x.pat {
                Pat::Ident(PatIdent {
                    ident,
                    subpat: None,
                    ..
                }) => (ident, x.ty),
                other => panic!("I don't know what to do with pattern {other:?}"),
            },
        })
        .partition::<Vec<_>, _>(|(ident, _)| ident == "value");
    let (ctx, args) = args
        .into_iter()
        .partition::<Vec<_>, _>(|(ident, _)| ident == "ctx");

    let [(value, vty)]: [_; 1] = value.try_into().expect("expected exactly 1 value arg");

    let arg = args.iter().map(|(id, _)| id);
    let ty = args.iter().map(|(_, ty)| ty);

    let (ctx, _cty) = if let Some(x) = ctx.into_iter().next() {
        (Some(x.0), Some(x.1))
    } else {
        (None, None)
    };

    let call_args = std::iter::once(value.clone().into_token_stream())
        .chain(arg.clone().map(|arg| quote! {args.#arg}))
        .chain(ctx.clone().into_iter().map(|x| x.into_token_stream()));

    quote! {
        mod #name {
            use crate::interpreter::filter::prelude::*;

            #[derive(Debug, crate::interpreter::filter::Args)]
            pub struct Args<'doc> {
                _marker: core::marker::PhantomData<&'doc ()>,
                #(#arg: #ty),*
            }

            #[derive(Debug)]
            pub struct Filter;

            impl crate::interpreter::filter::Filter for Filter {
                type Args<'doc> = Args<'doc>;
                type Value<'doc> = #vty;

                fn apply<'ctx>(#value: Self::Value<'ctx>,
                    args: Self::Args<'ctx>,
                    #[allow(unused)]
                    ctx: &mut crate::interpreter::ElementContext<'_, 'ctx>
                ) -> anyhow::Result<crate::interpreter::Value<'ctx>> {
                    // we can't elide the 'doc lifetime here because it needs to
                    // also be in the struct, unless we make a smarter macro
                    // (i.e., lifetime-aware)
                    #[allow(clippy::needless_lifetimes, clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
                    #inner

                    #name (#(#call_args),*)
                }
            }
        }
    }
    .into()
}
