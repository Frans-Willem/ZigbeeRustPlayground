extern crate proc_macro;
extern crate proc_macro2;

mod construct;
mod deconstruct;

use crate::proc_macro2::TokenStream;
use quote::quote;
use quote::ToTokens;
use syn;
use syn::parse_macro_input;
use construct::*;
use deconstruct::*;

fn gen_temporary_names() -> impl Iterator<Item = syn::Ident> {
    (0..).map(|num| {
        let name = format!("tmp{}", num);
        syn::Ident::new(&name, proc_macro2::Span::call_site())
    })
}

fn gen_ignored_names() -> impl Iterator<Item = syn::Ident> {
    (0..).map(|num| {
        let name = format!("_ignore{}", num);
        syn::Ident::new(&name, proc_macro2::Span::call_site())
    })
}

fn match_path(path: &syn::Path, name: &str) -> bool {
    if path.leading_colon.is_none() && path.segments.len() == 1 {
        if let Some(segment) = path.segments.first() {
            return segment.ident.to_string() == name;
        }
    }
    false
}

fn find_simple_attribute<'a, T: syn::parse::Parse>(
    attrs: &'a Vec<syn::Attribute>,
    name: &str,
) -> syn::Result<T> {
    let found = attrs
        .iter()
        .find(|attr| match_path(&attr.path, name))
        .ok_or(syn::Error::new(
            proc_macro2::Span::call_site(),
            format!("Attribute '{}' not found", name),
        ))?;
    found.parse_args()
}

#[proc_macro_attribute]
pub fn serialize_tag_type(
    _attr: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
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

#[proc_macro_derive(Tagged, attributes(serialize_tag, serialize_tag_type))]
pub fn tagged_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    impl_tagged_macro(&ast).into()
}

fn impl_serialize_macro(ast: &syn::DeriveInput) -> TokenStream {
    match &ast.data {
        syn::Data::Struct(s) => impl_serialize_struct_macro(&ast.ident, &s.fields),
        syn::Data::Enum(e) => impl_serialize_enum_macro(&ast.ident, &ast.attrs, &e.variants),
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

fn impl_tagged_macro(ast: &syn::DeriveInput) -> TokenStream {
    match &ast.data {
        syn::Data::Enum(e) => impl_tagged_enum_macro(&ast.ident, &ast.attrs, &e.variants),
        _ => panic!("derive(Tagged) not (yet) implemented for this type"),
    }
}

fn impl_serialize_struct_macro(name: &syn::Ident, fields: &syn::Fields) -> TokenStream {
    let (deconstruct, names) = deconstruct_from_fields(&syn::parse_quote! { #name }, fields, gen_temporary_names());
    quote! {
        impl crate::parse_serialize::Serialize for #name {
            fn serialize<W: std::io::Write>(&self, ctx: cookie_factory::WriteContext<W>) -> cookie_factory::GenResult<W> {
                            let #deconstruct = self;
                            #( let ctx = #names.serialize(ctx)?; )*
               Ok(ctx)
            }
        }
    }
}

fn impl_deserialize_struct_macro(name: &syn::Ident, fields: &syn::Fields) -> TokenStream {
    let (construct_expr, field_types, field_names) =
        construct_from_fields(&syn::parse_quote! { #name }, fields, gen_temporary_names());
    quote! {
        impl crate::parse_serialize::Deserialize for #name {
            fn deserialize(input: &[u8]) -> crate::parse_serialize::DeserializeResult<Self> {
                                #( let (input, #field_names ) = #field_types::deserialize(input)?; )*
                Ok((input, #construct_expr))
            }
        }

    }
}

fn impl_serialize_enum_macro(
    name: &syn::Ident,
    attributes: &Vec<syn::Attribute>,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::token::Comma>,
) -> TokenStream {
    let tag_type: syn::Type = find_simple_attribute(attributes, "serialize_tag_type").unwrap();
    let arms: Vec<syn::Arm> = variants
        .iter()
        .map(|variant| {
            let tag: syn::Expr = if let Some((_, discr)) = &variant.discriminant {
                syn::parse2(discr.to_token_stream()).unwrap()
            } else {
                find_simple_attribute(&variant.attrs, "serialize_tag").unwrap()
            };
            let (deconstruct, names) = deconstruct_from_enum_variant(name, variant, gen_temporary_names());
            let ret: syn::Arm = syn::parse_quote! {
                #deconstruct => {
                                        let tag : #tag_type = #tag;
                                        let ctx = tag.serialize(ctx)?;
                                        #( let ctx = #names.serialize(ctx)?; )*
                    Ok(ctx)
                }
            };
            return ret;
        })
        .collect();

    quote! {
        impl crate::parse_serialize::Serialize for #name {
            fn serialize<W: std::io::Write>(&self, ctx: cookie_factory::WriteContext<W>) -> cookie_factory::GenResult<W> {
                                match self {
                                    #( #arms ),*
                                }
            }
        }
    }
}

fn impl_deserialize_enum_macro(
    name: &syn::Ident,
    attributes: &Vec<syn::Attribute>,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::token::Comma>,
) -> TokenStream {
    let tag_type: syn::Type = find_simple_attribute(attributes, "serialize_tag_type").unwrap();
    let arms: Vec<syn::Arm> = variants
        .iter()
        .map(|variant| {
            let tag: syn::Pat = if let Some((_, discr)) = &variant.discriminant {
                syn::parse2(discr.to_token_stream()).unwrap()
            } else {
                find_simple_attribute(&variant.attrs, "serialize_tag").unwrap()
            };
            let (construct, types, names) = construct_from_enum_variant(name, variant, gen_temporary_names());
            let ret: syn::Arm = syn::parse_quote! {
                #tag => {
                    #( let (input, #names) = #types::deserialize(input)?; )*
                    Ok((input, #construct ))
                }
            };
            return ret;
        })
        .collect();
    quote! {
        impl crate::parse_serialize::Deserialize for #name {
            fn deserialize(input: &[u8]) -> crate::parse_serialize::DeserializeResult<Self> {
                let (input, tag) = #tag_type ::deserialize(input)?;
                match tag {
                                        #( #arms , )*
                    _ => std::convert::Into::into(crate::parse_serialize::DeserializeError::unexpected_data(input))
                }
            }
        }
    }
}

fn impl_tagged_enum_macro(
    name: &syn::Ident,
    attributes: &Vec<syn::Attribute>,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::token::Comma>,
) -> TokenStream {
    let tag_type: syn::Type = find_simple_attribute(attributes, "serialize_tag_type").unwrap();
    let arms: Vec<syn::Arm> = variants
        .iter()
        .map(|variant| {
            let tag: syn::Expr = if let Some((_, discr)) = &variant.discriminant {
                syn::parse2(discr.to_token_stream()).unwrap()
            } else {
                find_simple_attribute(&variant.attrs, "serialize_tag").unwrap()
            };
            let (deconstruct, _) = deconstruct_from_enum_variant(name, variant, gen_ignored_names());
            let ret: syn::Arm = syn::parse_quote! {
                #deconstruct => {
                    let tag : #tag_type = #tag;
                    Ok(tag)
                }
            };
            return ret;
        })
        .collect();

    quote! {
        impl crate::parse_serialize::Tagged for #name {
            type TagType = #tag_type;

            fn get_tag(&self) -> crate::parse_serialize::SerializeResult<Self::TagType> {
                                match self {
                                    #( #arms ),*
                                }
            }
        }
    }
}
