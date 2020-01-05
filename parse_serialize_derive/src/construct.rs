/**
 * Creates a construct expression for an enum variant,
 * and returns types & expected identifiers of arguments.
 */
pub fn construct_from_enum_variant<N : Iterator<Item = syn::Ident>> (
    enum_name: &syn::Ident,
    variant: &syn::Variant,
    namegen: N,
) -> (syn::Expr, Vec<syn::Type>, Vec<syn::Ident>) {
    let variant_name = &variant.ident;
    construct_from_fields(
        &syn::parse_quote! { #enum_name :: #variant_name },
        &variant.fields,
        namegen,
    )
}

pub fn construct_from_fields<N : Iterator<Item = syn::Ident>>(
    constructor_path: &syn::Path,
    fields: &syn::Fields,
    namegen: N,
) -> (syn::Expr, Vec<syn::Type>, Vec<syn::Ident>) {
    match fields {
        syn::Fields::Named(f) => construct_from_named_fields(constructor_path, &f, namegen),
        syn::Fields::Unnamed(f) => construct_from_unnamed_fields(constructor_path, &f, namegen),
        syn::Fields::Unit => (
            syn::parse_quote! { #constructor_path },
            Vec::new(),
            Vec::new(),
        ),
    }
}

pub fn construct_from_named_fields<N : Iterator<Item = syn::Ident>>(
    constructor_path: &syn::Path,
    fields: &syn::FieldsNamed,
    namegen: N,
) -> (syn::Expr, Vec<syn::Type>, Vec<syn::Ident>) {
    let struct_names = fields.named.iter().map(|f| f.ident.as_ref().unwrap());
    let new_names: Vec<_> = namegen.take(fields.named.len()).collect();
    let types: Vec<_> = fields.named.iter().map(|f| f.ty.clone()).collect();
    (
        syn::parse_quote! { #constructor_path { #( #struct_names: #new_names ),* } },
        types,
        new_names,
    )
}

pub fn construct_from_unnamed_fields<N : Iterator<Item = syn::Ident>>(
    constructor_path: &syn::Path,
    fields: &syn::FieldsUnnamed,
    namegen: N,
) -> (syn::Expr, Vec<syn::Type>, Vec<syn::Ident>) {
    let new_names: Vec<_> = namegen.take(fields.unnamed.len()).collect();
    let types: Vec<_> = fields.unnamed.iter().map(|f| f.ty.clone()).collect();
    (
        syn::parse_quote! { #constructor_path ( #( #new_names ),* ) },
        types,
        new_names,
    )
}
