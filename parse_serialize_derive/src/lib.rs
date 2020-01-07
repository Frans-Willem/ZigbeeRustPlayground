extern crate proc_macro;
extern crate proc_macro2;

mod construct;
mod deconstruct;

use crate::proc_macro2::TokenStream;
use construct::*;
use deconstruct::*;
use quote::quote;
use quote::ToTokens;
use std::collections::HashSet;
use syn;
use syn::parse_macro_input;

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
            return segment.ident == name;
        }
    }
    false
}

fn find_simple_attribute<T: syn::parse::Parse>(
    attrs: &[syn::Attribute],
    name: &str,
) -> syn::Result<T> {
    let found = attrs
        .iter()
        .find(|attr| match_path(&attr.path, name))
        .ok_or_else(|| {
            syn::Error::new(
                proc_macro2::Span::call_site(),
                format!("Attribute '{}' not found", name),
            )
        })?;
    found.parse_args()
}

fn get_tag_type(attributes: &[syn::Attribute]) -> syn::Result<syn::Type> {
    find_simple_attribute(attributes, "tag_type")
}

fn get_tag_expr(variant: &syn::Variant) -> syn::Result<syn::Expr> {
    if let Some((_, discr)) = &variant.discriminant {
        syn::parse2(discr.to_token_stream())
    } else {
        find_simple_attribute(&variant.attrs, "tag")
    }
}

fn get_tag_pat(variant: &syn::Variant) -> syn::Result<syn::Pat> {
    if let Some((_, discr)) = &variant.discriminant {
        syn::parse2(discr.to_token_stream())
    } else {
        find_simple_attribute(&variant.attrs, "tag")
    }
}

#[proc_macro_derive(Pack, attributes(tag, tag_type))]
pub fn pack_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    impl_pack(&ast).into()
}

#[proc_macro_derive(PackTagged, attributes(tag, tag_type))]
pub fn pack_tagged_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    impl_pack_tagged(&ast).into()
}

fn impl_pack(ast: &syn::DeriveInput) -> TokenStream {
    match &ast.data {
        syn::Data::Struct(s) => impl_pack_for_struct(&ast.ident, &s.fields),
        syn::Data::Enum(e) => impl_pack_for_enum(&ast.ident, &ast.attrs, &e.variants),
        _ => panic!("derive(Pack) not (yet) implemented for this type"),
    }
}

fn impl_pack_tagged(ast: &syn::DeriveInput) -> TokenStream {
    match &ast.data {
        syn::Data::Enum(e) => impl_pack_tagged_for_enum(&ast.ident, &ast.attrs, &e.variants),
        _ => panic!("derive(Pack) not (yet) implemented for this type"),
    }
}

fn impl_pack_for_struct(name: &syn::Ident, fields: &syn::Fields) -> TokenStream {
    let (construct_expr, construct_types, construct_names) =
        construct_from_fields(&syn::parse_quote! { #name }, fields, gen_temporary_names());
    let (deconstruct_pat, deconstruct_names) =
        deconstruct_from_fields(&syn::parse_quote! { #name }, fields, gen_temporary_names());
    let subtypes = &construct_types;
    quote! {
        impl crate::pack::Pack for #name
            where
                #( #subtypes : crate::pack::Pack, )*
        {
            fn unpack(data: &[u8]) -> core::result::Result<(Self, &[u8]), crate::pack::UnpackError> {
                #( let ( #construct_names, data) = #construct_types::unpack(data)?; )*
                Ok((#construct_expr, data))
            }
            fn pack<T: crate::pack::PackTarget>(&self, target: T) -> core::result::Result<T, crate::pack::PackError<T::Error>>
            {
                let #deconstruct_pat = self;
                #( let target = #deconstruct_names.pack(target)?; )*
                Ok(target)
            }
        }
    }
}

fn impl_pack_for_enum(
    name: &syn::Ident,
    attributes: &[syn::Attribute],
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::token::Comma>,
) -> TokenStream {
    let tag_type = get_tag_type(attributes).unwrap();
    let mut contained_types: HashSet<syn::Type> = HashSet::new();
    contained_types.insert(tag_type.clone());
    let pack_arms: Vec<syn::Arm> = variants
        .iter()
        .map(|variant| {
            let tag = get_tag_expr(variant).unwrap();
            let (deconstruct_pat, names) =
                deconstruct_from_enum_variant(name, variant, gen_temporary_names());
            syn::parse_quote! {
                #deconstruct_pat => {
                    let tag : #tag_type = #tag;
                    let target = tag.pack(target)?;
                    #( let target = #names.pack(target)?; )*
                    Ok(target)
                }
            }
        })
        .collect();
    let unpack_arms: Vec<syn::Arm> = variants
        .iter()
        .map(|variant| {
            let tag = get_tag_pat(variant).unwrap();
            let (construct_expr, construct_types, construct_names) =
                construct_from_enum_variant(name, variant, gen_temporary_names());
            for construct_type in construct_types.iter() {
                contained_types.insert(construct_type.clone());
            }
            syn::parse_quote! {
                #tag => {
                    #( let (#construct_names, data) = #construct_types::unpack(data)?; )*
                    Ok((#construct_expr, data))
                }
            }
        })
        .collect();
    let contained_types = contained_types.into_iter();
    quote! {
        impl crate::pack::Pack for #name
            where
                #( #contained_types : crate::pack::Pack, )*
        {
            fn unpack(data: &[u8]) -> core::result::Result<(Self, &[u8]), crate::pack::UnpackError> {
                let (tag, data) = #tag_type::unpack(data)?;
                match tag {
                    #( #unpack_arms, )*
                    _ => Err(crate::pack::UnpackError::InvalidEnumTag),
                }
            }
            fn pack<T: crate::pack::PackTarget>(&self, target: T) -> core::result::Result<T, crate::pack::PackError<T::Error>>
            {
                match self {
                    #( #pack_arms, )*
                }
            }
        }
    }
}

fn impl_pack_tagged_for_enum(
    name: &syn::Ident,
    attributes: &[syn::Attribute],
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::token::Comma>,
) -> TokenStream {
    let tag_type = get_tag_type(attributes).unwrap();
    let mut contained_types: HashSet<syn::Type> = HashSet::new();
    let tag_arms: Vec<syn::Arm> = variants
        .iter()
        .map(|variant| {
            let tag = get_tag_expr(variant).unwrap();
            let (deconstruct_pat, _) =
                deconstruct_from_enum_variant(name, variant, gen_ignored_names());
            syn::parse_quote! {
                #deconstruct_pat => {
                    let tag : #tag_type = #tag;
                    tag
                }
            }
        })
        .collect();
    let pack_arms: Vec<syn::Arm> = variants
        .iter()
        .map(|variant| {
            let (deconstruct_pat, names) =
                deconstruct_from_enum_variant(name, variant, gen_temporary_names());
            syn::parse_quote! {
                #deconstruct_pat => {
                    #( let target = #names.pack(target)?; )*
                    Ok(target)
                }
            }
        })
        .collect();
    let unpack_arms: Vec<syn::Arm> = variants
        .iter()
        .map(|variant| {
            let tag = get_tag_pat(variant).unwrap();
            let (construct_expr, construct_types, construct_names) =
                construct_from_enum_variant(name, variant, gen_temporary_names());
            for construct_type in construct_types.iter() {
                contained_types.insert(construct_type.clone());
            }
            syn::parse_quote! {
                #tag => {
                    #( let (#construct_names, data) = #construct_types::unpack(data)?; )*
                    Ok((#construct_expr, data))
                }
            }
        })
        .collect();
    let contained_types = contained_types.into_iter();
    quote! {
        impl crate::pack::PackTagged for #name
            where
                #( #contained_types : crate::pack::Pack, )*
        {
            type Tag = #tag_type;
            fn get_tag(&self) -> Self::Tag {
                match self {
                    #( #tag_arms, )*
                }
            }
            fn unpack_data(tag: Self::Tag, data: &[u8]) -> core::result::Result<(Self, &[u8]), crate::pack::UnpackError> {
                match tag {
                    #( #unpack_arms, )*
                    _ => Err(crate::pack::UnpackError::InvalidEnumTag),
                }
            }
            fn pack_data<T: crate::pack::PackTarget>(&self, target: T) -> core::result::Result<T, crate::pack::PackError<T::Error>>
            {
                match self {
                    #( #pack_arms, )*
                }
            }
        }
    }
}
