extern crate proc_macro;
extern crate proc_macro2;

use crate::proc_macro2::{Ident, Span, TokenStream};
use quote::{quote, ToTokens};
use syn;
use syn::parse_macro_input;

#[proc_macro_derive(Serialize, attributes(enum_tag, enum_tag_type))]
pub fn serialize_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    impl_serialize_macro(&ast).into()
}

#[proc_macro_derive(Deserialize, attributes(enum_tag, enum_tag_type))]
pub fn deserialize_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    impl_deserialize_macro(&ast).into()
}

#[proc_macro_derive(SerializeTagged, attributes(enum_tag, enum_tag_type))]
pub fn serializetagged_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    impl_serializetagged_macro(&ast).into()
}

#[proc_macro_derive(DeserializeTagged, attributes(enum_tag, enum_tag_type))]
pub fn deserializetagged_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    impl_deserializetagged_macro(&ast).into()
}

fn impl_serialize_macro(ast: &syn::DeriveInput) -> TokenStream {
    match &ast.data {
        syn::Data::Struct(d) => impl_serialize_struct_macro(&ast.ident, d),
        syn::Data::Enum(d) => impl_serialize_enum_macro(&ast.ident, &ast.attrs, d),
        _ => panic!("derive(Serialize) not implemented for this datatype"),
    }
}

fn impl_serializetagged_macro(ast: &syn::DeriveInput) -> TokenStream {
    match &ast.data {
        syn::Data::Enum(d) => impl_serializetagged_enum_macro(&ast.ident, &ast.attrs, d),
        _ => panic!("derive(SerializeTagged) not implemented for this datatype"),
    }
}

fn impl_deserializetagged_macro(ast: &syn::DeriveInput) -> TokenStream {
    match &ast.data {
        syn::Data::Enum(d) => impl_deserializetagged_enum_macro(&ast.ident, &ast.attrs, d),
        _ => panic!("derive(DeserializeTagged) not implemented for this datatype"),
    }
}

/** Finds outer attribute by name */
fn find_outer_attr_by_name(attrs: &Vec<syn::Attribute>, name: &str) -> Option<TokenStream> {
    let mut outer_attrs = attrs.iter().filter(|attr| {
        if let syn::AttrStyle::Outer = attr.style {
            true
        } else {
            false
        }
    });
    let result = outer_attrs.find(|attr| attr.path.is_ident(name))?;
    Some(result.tokens.clone())
}

fn get_enum_tag_type(attrs: &Vec<syn::Attribute>) -> Option<syn::Type> {
    if let Some(repr_attr) = find_outer_attr_by_name(attrs, "repr") {
        Some(syn::parse2::<syn::Type>(repr_attr).unwrap())
    } else if let Some(enum_tag_type_attr) = find_outer_attr_by_name(attrs, "enum_tag_type") {
        Some(syn::parse2::<syn::Type>(enum_tag_type_attr).unwrap())
    } else {
        panic!("derive Serialize or Deserialize on an enum requires either a #[repr(ty)] or #[enum_tag_type(ty)] attribute");
    }
}

fn create_match_pattern_for_fields(
    constructor_name: TokenStream,
    fields: &syn::Fields,
    varprefix: &str,
) -> (TokenStream, Vec<(syn::Type, Ident)>) {
    let varnames =
        (0..).map(|index| Ident::new(&format!("{}{}", varprefix, index), Span::call_site()));
    match fields {
        syn::Fields::Unit => ((quote! { #constructor_name }, vec![])),
        syn::Fields::Named(f) => {
            let stored: Vec<(syn::Type, Ident)> = f
                .named
                .iter()
                .map(|field| field.ty.clone())
                .zip(varnames)
                .collect();
            let storenames = stored.iter().map(|(_, name)| name);
            let fieldnames = f.named.iter().map(|f| &f.ident);
            (
                quote! {
                    #constructor_name { #(#fieldnames : #storenames, )* }
                },
                stored,
            )
        }
        syn::Fields::Unnamed(f) => {
            let stored: Vec<(syn::Type, Ident)> = f
                .unnamed
                .iter()
                .map(|field| field.ty.clone())
                .zip(varnames)
                .collect();
            let storenames = stored.iter().map(|(_, name)| name);
            (
                quote! {
                    #constructor_name ( #( #storenames ),* )
                },
                stored,
            )
        }
    }
}

fn create_match_pattern_for_variant(
    owner: &syn::Ident,
    variant: &syn::Variant,
    varprefix: &str,
) -> (TokenStream, Vec<(syn::Type, Ident)>) {
    let variant_name = &variant.ident;
    create_match_pattern_for_fields(quote! { #owner::#variant_name }, &variant.fields, varprefix)
}

fn create_build_expression_for_fields(
    constructor_name: TokenStream,
    fields: &syn::Fields,
    varprefix: &str,
) -> (TokenStream, Vec<(syn::Type, Ident)>) {
    let varnames =
        (0..).map(|index| Ident::new(&format!("{}{}", varprefix, index), Span::call_site()));
    match fields {
        syn::Fields::Unit => (quote! { #constructor_name }, vec![]),
        syn::Fields::Named(f) => {
            let stored: Vec<(syn::Type, Ident)> = f
                .named
                .iter()
                .map(|field| field.ty.clone())
                .zip(varnames)
                .collect();
            let storenames = stored.iter().map(|(_, name)| name);
            let fieldnames = f.named.iter().map(|f| &f.ident);
            (
                quote! { #constructor_name { #( #fieldnames : #storenames, )* } },
                stored,
            )
        }
        syn::Fields::Unnamed(f) => {
            let stored: Vec<(syn::Type, syn::Ident)> = f
                .unnamed
                .iter()
                .map(|field| field.ty.clone())
                .zip(varnames)
                .collect();
            let storenames = stored.iter().map(|(_, name)| name);
            (quote! { #constructor_name ( #(#storenames, )* ) }, stored)
        }
    }
}

fn impl_serialize_struct_macro(name: &syn::Ident, data: &syn::DataStruct) -> TokenStream {
    let (match_pat, stored) =
        create_match_pattern_for_fields(quote! { #name }, &data.fields, "tmp");
    let destructure: TokenStream = quote! {
        let #match_pat = self
    };
    let serialize: Vec<TokenStream> = stored
        .iter()
        .map(|(_, name)| {
            quote! {
                #name.serialize_to(target)?
            }
        })
        .collect();
    let gen = quote! {
        impl crate::parse_serialize::Serialize for #name {
            fn serialize_to(&self, target: &mut Vec<u8>) -> crate::parse_serialize::SerializeResult<()> {
               #destructure;
               #(#serialize;)*
               Ok(())
            }
        }
    };
    gen.into()
}

fn get_variant_discriminant(variant: &syn::Variant) -> TokenStream {
    if let Some((_, discr)) = variant.discriminant.as_ref() {
        discr.to_token_stream()
    } else if let Some(tokens) = find_outer_attr_by_name(&variant.attrs, "enum_tag") {
        let parsed: syn::ExprParen = syn::parse2(tokens).unwrap();
        parsed.expr.to_token_stream()
    } else {
        panic!("All enum variants should have a discriminant or a #[enum_tag(expr)] attribute.");
    }
}

fn impl_serialize_enum_macro(
    name: &syn::Ident,
    attrs: &Vec<syn::Attribute>,
    data: &syn::DataEnum,
) -> TokenStream {
    let repr_type = get_enum_tag_type(attrs).unwrap();
    let match_arms = data.variants.iter().map(|variant| {
        let (pattern, data) = create_match_pattern_for_variant(name, &variant, "var");
        let discriminant = get_variant_discriminant(&variant);
        let serialize_data = data
            .iter()
            .map(|(_ty, name)| quote! { #name.serialize_to(target)? });
        quote! {
            #pattern => {
                ((#discriminant) as #repr_type).serialize_to(target)?;
                #(#serialize_data ;)*
                Ok(())
            }
        }
    });
    quote! {
        impl crate::parse_serialize::Serialize for #name {
            fn serialize_to(&self, target: &mut Vec<u8>) -> crate::parse_serialize::SerializeResult<()> {
                match self {
                    #(#match_arms,)*
                }
            }
        }
    }
}

fn impl_serializetagged_enum_macro(
    name: &syn::Ident,
    attrs: &Vec<syn::Attribute>,
    data: &syn::DataEnum,
) -> TokenStream {
    let repr_type = get_enum_tag_type(attrs).unwrap();
    let tag_match_arms = data.variants.iter().map(|variant| {
        let discriminant = get_variant_discriminant(&variant);
        let (pattern, _) = create_match_pattern_for_variant(name, &variant, "_var");
        quote! {
            #pattern => Ok(#discriminant)
        }
    });
    let data_match_arms = data.variants.iter().map(|variant| {
        let (pattern, to_serialize) = create_match_pattern_for_variant(name, &variant, "var");
        let to_serialize = to_serialize.iter().map(|(_ty, name)| name);
        quote! {
            #pattern => {
                #(#to_serialize.serialize_to(target)?;)*
                Ok(())
            }
        }
    });
    quote! {
        impl crate::parse_serialize::SerializeTagged for #name {
            type TagType = #repr_type;
            fn serialize_tag(&self) -> crate::parse_serialize::SerializeResult<#repr_type> {
                match self {
                    #(#tag_match_arms,)*
                }
            }
            fn serialize_data_to(&self, target: &mut Vec<u8>) -> crate::parse_serialize::SerializeResult<()> {
                match self {
                    #(#data_match_arms,)*
                }
            }
        }
    }
}

fn impl_deserialize_macro(ast: &syn::DeriveInput) -> TokenStream {
    match &ast.data {
        syn::Data::Struct(d) => impl_deserialize_struct_macro(&ast.ident, d),
        syn::Data::Enum(d) => impl_deserialize_enum_macro(&ast.ident, &ast.attrs, d),
        _ => unimplemented!(),
    }
}
fn impl_deserialize_struct_macro(name: &syn::Ident, data: &syn::DataStruct) -> TokenStream {
    let (build_expr, vars) =
        create_build_expression_for_fields(quote! { #name }, &data.fields, "tmp");
    let deserialize_stmts = vars.iter().map(|(ty, name)| {
        quote! {
            let (input, #name) = <#ty>::deserialize(input)?
        }
    });
    let gen = quote! {
        impl crate::parse_serialize::Deserialize for #name {
            fn deserialize(input: &[u8]) -> crate::parse_serialize::DeserializeResult<Self> {
                #(#deserialize_stmts;)*
                Ok((input, #build_expr))
            }
        }
    };
    gen.into()
}

fn impl_deserialize_enum_macro(
    name: &syn::Ident,
    attrs: &Vec<syn::Attribute>,
    data: &syn::DataEnum,
) -> TokenStream {
    let repr_type = get_enum_tag_type(attrs).unwrap();
    let match_arms = data.variants.iter().map(|variant| {
        let discriminant = get_variant_discriminant(variant);
        let variant_ident = &variant.ident;
        let (build_expr, to_deserialize) = create_build_expression_for_fields(
            quote! { #name::#variant_ident },
            &variant.fields,
            "tmp",
        );
        let deserialize_stmts = to_deserialize.iter().map(|(ty, name)| {
            quote! {
                let (input, #name) = <#ty>::deserialize(input)?
            }
        });
        quote! {
            #discriminant => {
                #(#deserialize_stmts ;)*
                Ok((input, #build_expr))
            }
        }
    });
    let ret = quote! {
        impl crate::parse_serialize::Deserialize for #name {
            fn deserialize(input: &[u8]) -> crate::parse_serialize::DeserializeResult<Self> {
                let (input, tag) = <#repr_type>::deserialize(input)?;
                match tag {
                    #(#match_arms, )*
                    _ => crate::parse_serialize::DeserializeError::unexpected_data(input).into(),
                }
            }
        }
    };
    ret
}

fn impl_deserializetagged_enum_macro(
    name: &syn::Ident,
    attrs: &Vec<syn::Attribute>,
    data: &syn::DataEnum,
) -> TokenStream {
    let repr_type = get_enum_tag_type(attrs).unwrap();
    let match_arms = data.variants.iter().map(|variant| {
        let discriminant = get_variant_discriminant(variant);
        let variant_ident = &variant.ident;
        let (build_expr, to_deserialize) = create_build_expression_for_fields(
            quote! { #name::#variant_ident },
            &variant.fields,
            "tmp",
        );
        let deserialize_stmts = to_deserialize.iter().map(|(ty, name)| {
            quote! {
                let (input, #name) = <#ty>::deserialize(input)?
            }
        });
        quote! {
            #discriminant => {
                #(#deserialize_stmts ;)*
                Ok((input, #build_expr))
            }
        }
    });
    let ret = quote! {
        impl crate::parse_serialize::DeserializeTagged for #name {
            type TagType = #repr_type;
            fn deserialize_data(discriminant: #repr_type, input: &[u8]) -> crate::parse_serialize::DeserializeResult<Self> {
                match discriminant {
                    #(#match_arms, )*
                    _ => crate::parse_serialize::DeserializeError::unexpected_data(input).into(),
                }
            }
        }
    };
    ret
}
