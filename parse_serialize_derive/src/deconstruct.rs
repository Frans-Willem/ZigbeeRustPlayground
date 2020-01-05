pub fn deconstruct_from_enum_variant<N: Iterator<Item = syn::Ident>>(
    enum_name: &syn::Ident,
    variant: &syn::Variant,
    namegen: N,
) -> (syn::Pat, Vec<syn::Ident>) {
    let variant_name = &variant.ident;
    deconstruct_from_fields(
        &syn::parse_quote! { #enum_name :: #variant_name },
        &variant.fields,
        namegen,
    )
}

pub fn deconstruct_from_fields<N: Iterator<Item = syn::Ident>>(
    constructor_path: &syn::Path,
    fields: &syn::Fields,
    namegen: N,
) -> (syn::Pat, Vec<syn::Ident>) {
    match fields {
        syn::Fields::Named(f) => deconstruct_from_named_fields(constructor_path, &f, namegen),
        syn::Fields::Unnamed(f) => deconstruct_from_unnamed_fields(constructor_path, &f, namegen),
        syn::Fields::Unit => (syn::parse_quote! { #constructor_path }, Vec::new()),
    }
}

pub fn deconstruct_from_named_fields<N: Iterator<Item = syn::Ident>>(
    constructor_path: &syn::Path,
    fields: &syn::FieldsNamed,
    namegen: N,
) -> (syn::Pat, Vec<syn::Ident>) {
    let struct_names = fields.named.iter().map(|f| f.ident.as_ref().unwrap());
    let new_names: Vec<_> = namegen.take(fields.named.len()).collect();
    (
        syn::parse_quote! { #constructor_path { #( #struct_names: #new_names ),* } },
        new_names,
    )
}

pub fn deconstruct_from_unnamed_fields<N: Iterator<Item = syn::Ident>>(
    constructor_path: &syn::Path,
    fields: &syn::FieldsUnnamed,
    namegen: N,
) -> (syn::Pat, Vec<syn::Ident>) {
    let new_names: Vec<_> = namegen.take(fields.unnamed.len()).collect();
    (
        syn::parse_quote! { #constructor_path ( #( #new_names ),* ) },
        new_names,
    )
}
