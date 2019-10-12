extern crate proc_macro;
extern crate proc_macro2;

use crate::proc_macro2::{Literal, TokenStream, TokenTree};
use quote::quote;
use syn;
use syn::parse_macro_input;

#[proc_macro_derive(Serialize)]
pub fn serialize_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    impl_serialize_macro(&ast).into()
}

#[proc_macro_derive(Deserialize)]
pub fn deserialize_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    impl_deserialize_macro(&ast).into()
}

fn impl_serialize_macro(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let fields = if let syn::Data::Struct(s) = &ast.data {
        &s.fields
    } else {
        panic!("Serialize can only be derived on structures");
    };
    let mut serialize_stmts = TokenStream::new();
    let mut next_index = 0;
    for field in fields {
        let field_name: TokenTree = if let Some(ident) = &field.ident {
            ident.clone().into()
        } else {
            let current_index = next_index;
            next_index += 1;
            Literal::usize_unsuffixed(current_index).into()
        };
        let serialize_stmt: TokenStream = quote! {
            self.#field_name.serialize_to(target)?;
        }
        .into();
        serialize_stmts.extend(serialize_stmt);
    }
    let gen = quote! {
        impl crate::parse_serialize::Serialize for #name {
            fn serialize_to(&self, target: &mut Vec<u8>) -> crate::parse_serialize::SerializeResult<()> {
               #serialize_stmts
               Ok(())
            }
        }
    };
    gen.into()
}
fn impl_deserialize_macro(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let fields = if let syn::Data::Struct(s) = &ast.data {
        &s.fields
    } else {
        panic!("Deserialize can only be derived on structures");
    };
    let mut deserialize_stmts = TokenStream::new();
    let mut build_stmts = TokenStream::new();
    let mut next_index = 0;
    for field in fields {
        let temp_field_name = syn::Ident::new(
            &format!("tmp{}", next_index),
            proc_macro2::Span::call_site(),
        );
        let field_type = &field.ty;
        next_index += 1;
        let deserialize_stmt: TokenStream = quote! {
            let (input, #temp_field_name) = <#field_type>::deserialize(input)?;
        };
        deserialize_stmts.extend(deserialize_stmt);
        let build_stmt: TokenStream = if let Some(ident) = &field.ident {
            quote! { #ident : #temp_field_name , }
        } else {
            quote! { #temp_field_name , }
        };
        build_stmts.extend(build_stmt);
    }
    match fields {
        syn::Fields::Unnamed(syn::FieldsUnnamed { .. }) => {
            build_stmts = quote! {
                #name ( #build_stmts )
            };
        }
        syn::Fields::Named(syn::FieldsNamed { .. }) => {
            build_stmts = quote! {
                #name { #build_stmts }
            };
        }
        syn::Fields::Unit => {
            build_stmts = quote! { #name };
        }
    }
    let gen = quote! {
        impl crate::parse_serialize::Deserialize for #name {
            fn deserialize(input: &[u8]) -> crate::parse_serialize::DeserializeResult<Self> {
                #deserialize_stmts
                Ok((input, #build_stmts))
            }
        }
    };
    gen.into()
}
