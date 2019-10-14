extern crate proc_macro;
extern crate proc_macro2;

use crate::proc_macro2::TokenStream;
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

fn impl_serialize_macro(ast: &syn::DeriveInput) -> TokenStream {
    match &ast.data {
        syn::Data::Struct(d) => impl_serialize_struct_macro(&ast.ident, d),
        syn::Data::Enum(d) => impl_serialize_enum_macro(&ast.ident, &ast.attrs, d),
        _ => unimplemented!(),
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

fn create_path<'lt, I: IntoIterator<Item = &'lt syn::Ident>>(
    leading_colon: bool,
    iter: I,
) -> syn::Path {
    //  where I::Item : Into<syn::PathSegment> {
    let iter = iter
        .into_iter()
        .map(|x| Into::<syn::PathSegment>::into(x.clone())); //x.into::<syn::PathSegment>());
    syn::Path {
        leading_colon: if leading_colon {
            Some(syn::token::Colon2::default())
        } else {
            None
        },
        segments: iter.collect(),
    }
}

fn create_match_pattern_for_fields(
    name: syn::Path,
    fields: &syn::Fields,
    varprefix: &str,
) -> (syn::Pat, Vec<(syn::Type, syn::Ident)>) {
    let varnames = (0..).map(|index| {
        syn::Ident::new(
            &format!("{}{}", varprefix, index),
            proc_macro2::Span::call_site(),
        )
    });
    match fields {
        syn::Fields::Unit => (
            syn::Pat::Path(syn::PatPath {
                attrs: vec![],
                qself: None,
                path: name,
            }),
            vec![],
        ),
        syn::Fields::Named(f) => {
            let stored: Vec<(syn::Type, syn::Ident)> = f
                .named
                .iter()
                .map(|field| field.ty.clone())
                .zip(varnames)
                .collect();
            let pattern: syn::punctuated::Punctuated<syn::FieldPat, syn::token::Comma> = stored
                .iter()
                .zip(f.named.iter())
                .map(|((_, store_to), field)| syn::FieldPat {
                    attrs: vec![],
                    member: syn::Member::Named(field.ident.as_ref().unwrap().clone()),
                    colon_token: Some(syn::token::Colon::default()),
                    pat: Box::new(syn::Pat::Verbatim(store_to.to_token_stream())),
                })
                .collect();
            (
                syn::Pat::Struct(syn::PatStruct {
                    attrs: vec![],
                    path: name,
                    brace_token: syn::token::Brace::default(),
                    fields: pattern,
                    dot2_token: None,
                }),
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
            let pattern: syn::punctuated::Punctuated<syn::Pat, syn::token::Comma> = stored
                .iter()
                .map(|(_ty, name)| syn::Pat::Verbatim(name.to_token_stream()))
                .collect();
            (
                syn::Pat::TupleStruct(syn::PatTupleStruct {
                    attrs: vec![],
                    path: name,
                    pat: syn::PatTuple {
                        attrs: vec![],
                        paren_token: syn::token::Paren::default(),
                        elems: pattern,
                    },
                }),
                stored,
            )
        }
    }
}

fn create_build_expression_for_fields(
    name: syn::Path,
    fields: &syn::Fields,
    varprefix: &str,
) -> (syn::Expr, Vec<(syn::Type, syn::Ident)>) {
    let varnames = (0..).map(|index| {
        syn::Ident::new(
            &format!("{}{}", varprefix, index),
            proc_macro2::Span::call_site(),
        )
    });
    match fields {
        syn::Fields::Unit => (
            syn::Expr::Path(syn::ExprPath {
                attrs: vec![],
                qself: None,
                path: name,
            }),
            vec![],
        ),
        syn::Fields::Named(f) => {
            let stored: Vec<(syn::Type, syn::Ident)> = f
                .named
                .iter()
                .map(|field| field.ty.clone())
                .zip(varnames)
                .collect();
            let fields: syn::punctuated::Punctuated<syn::FieldValue, syn::token::Comma> = stored
                .iter()
                .zip(f.named.iter())
                .map(|((_, store_to), field)| syn::FieldValue {
                    attrs: vec![],
                    member: syn::Member::Named(field.ident.as_ref().unwrap().clone()),
                    colon_token: Some(syn::token::Colon::default()),
                    expr: syn::Expr::Path(syn::ExprPath {
                        attrs: vec![],
                        qself: None,
                        path: create_path(false, &[store_to.clone()]),
                    }),
                })
                .collect();
            (
                syn::Expr::Struct(syn::ExprStruct {
                    attrs: vec![],
                    path: name,
                    brace_token: syn::token::Brace::default(),
                    fields: fields,
                    dot2_token: None,
                    rest: None,
                }),
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
            let fields: syn::punctuated::Punctuated<syn::Expr, syn::token::Comma> = stored
                .iter()
                .map(|(_ty, name)| {
                    syn::Expr::Path(syn::ExprPath {
                        attrs: vec![],
                        qself: None,
                        path: create_path(false, &[name.clone()]),
                    })
                })
                .collect();
            (
                syn::Expr::Call(syn::ExprCall {
                    attrs: vec![],
                    func: Box::new(syn::Expr::Path(syn::ExprPath {
                        attrs: vec![],
                        qself: None,
                        path: name,
                    })),
                    paren_token: Default::default(),
                    args: fields,
                }),
                stored,
            )
        }
    }
}

fn impl_serialize_struct_macro(name: &syn::Ident, data: &syn::DataStruct) -> TokenStream {
    let (match_pat, stored) =
        create_match_pattern_for_fields(create_path(false, &[name.clone()]), &data.fields, "tmp");
    let destructure = syn::ExprLet {
        attrs: vec![],
        let_token: Default::default(),
        pat: match_pat,
        eq_token: Default::default(),
        expr: Box::new(syn::Expr::Path(syn::ExprPath {
            attrs: vec![],
            qself: None,
            path: create_path(
                false,
                &[syn::Ident::new("self", proc_macro2::Span::call_site())],
            ),
        })),
    };
    let serialize: TokenStream = stored
        .iter()
        .map(|(_, name)| {
            quote! {
                #name.serialize_to(target)?;
            }
        })
        .collect();
    let gen = quote! {
        impl crate::parse_serialize::Serialize for #name {
            fn serialize_to(&self, target: &mut Vec<u8>) -> crate::parse_serialize::SerializeResult<()> {
               #destructure;
               #serialize
               Ok(())
            }
        }
    };
    gen.into()
}

fn create_match_pattern_for_variant(
    owner: &syn::Ident,
    variant: &syn::Variant,
    varprefix: &str,
) -> (syn::Pat, Vec<(syn::Type, syn::Ident)>) {
    let path: syn::Path = create_path(false, &[owner.clone(), variant.ident.clone()]);
    create_match_pattern_for_fields(path, &variant.fields, varprefix)
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
    let mut match_arms = TokenStream::new();
    for variant in &data.variants {
        let (pattern, data) = create_match_pattern_for_variant(name, &variant, "var");
        let discriminator = get_variant_discriminant(&variant);
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
    let match_arms: Vec<syn::Arm> = data
        .variants
        .iter()
        .map(|variant| {
            let discriminant = get_variant_discriminant(variant);
            let (build, to_deserialize) = create_build_expression_for_fields(
                create_path(false, &[name.clone(), variant.ident.clone()]),
                &variant.fields,
                "tmp",
            );
            let deserialize_statements: Vec<syn::Expr> = to_deserialize
                .iter()
                .map(|(ty, name)| {
                    syn::parse_quote! {
                        let (input, #name) = <#ty>::deserialize(input)?
                    }
                })
                .map(syn::Expr::Let)
                .collect();
            let arm: syn::Arm = syn::parse_quote! { #discriminant => {
                #(#deserialize_statements ;)*
                Ok((input, #build))
            }};
            arm
        })
        .collect();
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
