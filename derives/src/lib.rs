extern crate proc_macro;
extern crate proc_macro2;

use crate::proc_macro2::{Literal, TokenStream, TokenTree};
use quote::ToTokens;
use quote::quote;
use syn;
use syn::parse_macro_input;

fn match_path(path: &syn::Path, name: &str) -> bool {
    if path.leading_colon.is_none() && path.segments.len() == 1 {
        if let Some(segment) = path.segments.first() {
            return segment.ident.to_string() == name
        }
    }
    false
}

fn find_simple_attribute<'a, T: syn::parse::Parse>(attrs: &'a Vec<syn::Attribute>, name: &str) -> syn::Result<T> {
    let found = attrs.iter().find(|attr| {
        match_path(&attr.path, name)
    }).ok_or(syn::Error::new(proc_macro2::Span::call_site(), "Attribute not found"))?;
    found.parse_args()
}

#[proc_macro_attribute]
pub fn serialize_tag_type(_attr: proc_macro::TokenStream, item: proc_macro::TokenStream) -> proc_macro::TokenStream {
    item
}

#[proc_macro_derive(Serialize, attributes(serialize_tag, serialize_tag_type))]
pub fn serialize_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    impl_serialize_macro(&ast).into()
}

#[proc_macro_derive(Deserialize, attributes(serialize_tag, serialize_tag_type))]
pub fn deserialize_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    impl_deserialize_macro(&ast).into()
}

fn impl_serialize_macro(ast: &syn::DeriveInput) -> TokenStream {
    match &ast.data {
        syn::Data::Struct(s) => impl_serialize_struct_macro(&ast.ident, &s.fields),
        syn::Data::Enum(e) => impl_serialize_enum_macro(&ast.ident, &e.variants),
        _ => panic!("derive(Serialize) not yet implemented for this type"),
    }
}
fn impl_deserialize_macro(ast: &syn::DeriveInput) -> TokenStream {
    match &ast.data {
        syn::Data::Struct(s) => impl_deserialize_struct_macro(&ast.ident, &s.fields),
        syn::Data::Enum(e) => impl_deserialize_enum_macro(&ast.ident, &ast.attrs, &e.variants),
        _ => panic!("derive(Deserialize) not yet implemented for this type"),
    }
}

fn impl_serialize_struct_macro(name: &syn::Ident, fields: &syn::Fields) -> TokenStream {
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
            let ctx = self.#field_name.serialize(ctx)?;
        }
        .into();
        serialize_stmts.extend(serialize_stmt);
    }
    let gen = quote! {
        impl crate::parse_serialize::Serialize for #name {
            fn serialize<W: std::io::Write>(&self, ctx: cookie_factory::WriteContext<W>) -> cookie_factory::GenResult<W> {
               #serialize_stmts
               Ok(ctx)
            }
        }
    };
    gen.into()
}
fn impl_deserialize_struct_macro(name: &syn::Ident, fields: &syn::Fields) -> TokenStream {
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

fn impl_serialize_enum_macro(name: &syn::Ident, variants: &syn::punctuated::Punctuated<syn::Variant, syn::token::Comma>) -> TokenStream {
    quote! {
        impl crate::parse_serialize::Serialize for #name {
            fn serialize<W: std::io::Write>(&self, ctx: cookie_factory::WriteContext<W>) -> cookie_factory::GenResult<W> {
                Ok(ctx)
            }
        }
    }
}

fn impl_deserialize_enum_macro(name: &syn::Ident, attributes: &Vec<syn::Attribute>,variants: &syn::punctuated::Punctuated<syn::Variant, syn::token::Comma>) -> TokenStream {
    let tag_type : syn::Type = find_simple_attribute(attributes, "serialize_tag_type").unwrap();
    let tags : Vec<syn::Pat> = variants.iter().map(|variant| {
        if let Some((_, discr)) = &variant.discriminant {
            syn::parse2(discr.to_token_stream()).unwrap()
        } else {
            find_simple_attribute(&variant.attrs, "serialize_tag").unwrap()
        }
            }).collect();
    quote! {
        impl crate::parse_serialize::Deserialize for #name {
            fn deserialize(input: &[u8]) -> crate::parse_serialize::DeserializeResult<Self> {
                let (input, tag) = #tag_type ::deserialize(input)?;
                match tag {
                    #( #tags => panic!("uh-oh! parsed {:?}", #tags), )*
                    _ => std::convert::Into::into(crate::parse_serialize::DeserializeError::unexpected_data(input))
                }
            }
        }
    }
}
