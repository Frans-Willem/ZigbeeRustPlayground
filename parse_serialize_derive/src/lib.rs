extern crate proc_macro;
extern crate proc_macro2;

use crate::proc_macro2::{Literal, TokenStream, TokenTree};
use quote::quote;
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

fn impl_serialize_macro(ast: &syn::DeriveInput) -> TokenStream {
    match &ast.data {
        syn::Data::Struct(d) => impl_serialize_struct_macro(&ast.ident, d),
        syn::Data::Enum(d) => impl_serialize_enum_macro(&ast.ident, &ast.attrs, d),
        _ => unimplemented!(),
    }
}

fn impl_serialize_struct_macro(name: &syn::Ident, data: &syn::DataStruct) -> TokenStream {
    let fields = &data.fields;
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

use quote::ToTokens;

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

fn create_match_pattern(
    owner: &syn::Ident,
    variant: &syn::Variant,
    varprefix: &str,
) -> (syn::Pat, Vec<(syn::Type, syn::Ident)>) {
    let variant_name = &variant.ident;
    match &variant.fields {
        syn::Fields::Unit => (syn::parse_quote! { #owner::#variant_name }, vec![]),
        syn::Fields::Named(f) => panic!("Named fields not yet implemented"),
        syn::Fields::Unnamed(f) => {
            let stored: Vec<(syn::Type, syn::Ident)> = f
                .unnamed
                .iter()
                .zip(0..)
                .map(|(field, index)| {
                    (
                        field.ty.clone(),
                        syn::Ident::new(
                            &format!("{}{}", varprefix, index),
                            proc_macro2::Span::call_site(),
                        ),
                    )
                })
                .collect();
            let pattern: syn::punctuated::Punctuated<syn::Ident, syn::token::Comma> =
                stored.iter().map(|(_ty, name)| name.clone()).collect();
            (
                syn::parse_quote! { #owner::#variant_name(#pattern) },
                stored,
            )
        }
    }
}

fn get_variant_discriminator(variant: &syn::Variant) -> TokenStream {
    if let Some((_, discr)) = variant.discriminant.as_ref() {
        discr.to_token_stream()
    } else if let Some(tokens) = find_outer_attr_by_name(&variant.attrs, "enum_tag") {
        tokens
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
    let mut match_arms = TokenStream::new();
    for variant in &data.variants {
        let (pattern, data) = create_match_pattern(name, &variant, "var");
        let discriminator = get_variant_discriminator(&variant);
        let serialize_data: TokenStream = data
            .iter()
            .map(|(_ty, name)| quote! { #name.serialize_to(target)?; })
            .collect();
        let match_arm = quote! {
            #pattern => {
                ((#discriminator) as #repr_type).serialize_to(target)?;
                #serialize_data
                Ok(())
            }
        };
        match_arms.extend(match_arm);
    }
    quote! {
        impl crate::parse_serialize::Serialize for #name {
            fn serialize_to(&self, target: &mut Vec<u8>) -> crate::parse_serialize::SerializeResult<()> {
                match self {
                    #match_arms
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
    let fields = &data.fields;
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

fn impl_deserialize_enum_macro(
    name: &syn::Ident,
    attrs: &Vec<syn::Attribute>,
    data: &syn::DataEnum,
) -> TokenStream {
    let repr_type = get_enum_tag_type(attrs).unwrap();
    let mut case_stmts = TokenStream::new();
    for variant in &data.variants {
        let variant_discriminant = &variant.discriminant.as_ref().unwrap().1;
        let variant_ident = &variant.ident;
        let case_stmt = quote! { #variant_discriminant => std::result::Result::Ok((input, #name::#variant_ident)), };
        case_stmts.extend(case_stmt);
    }
    quote! {
        impl crate::parse_serialize::Deserialize for #name {
            fn deserialize(input: &[u8]) -> crate::parse_serialize::DeserializeResult<Self> {
                let (input, value) = <#repr_type>::deserialize(input)?;
                match value {
                    #case_stmts
                    _ => crate::parse_serialize::DeserializeError::unexpected_data(input).into(),
                }
            }
        }
    }
}
