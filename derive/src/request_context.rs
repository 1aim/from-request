use proc_macro2::{TokenStream, Literal};
use quote::{quote, ToTokens};
use syn::{Attribute, Data, Meta};

pub fn derive_request_context(s: synstructure::Structure) -> TokenStream {
    deny_attr("as_ref", &s.ast().attrs);
    let additional_impls = match &s.ast().data {
        Data::Struct(st) => {
            let mut impls = Vec::new();
            for (index, field) in st.fields.iter().enumerate() {
                let as_ref_count = field
                    .attrs
                    .iter()
                    .filter(|attr| match attr.parse_meta() {
                        Ok(ref meta) if meta.name() == "as_ref" => {
                            if let Meta::Word(_) = meta {
                                true
                            } else {
                                if let Some(field) = &field.ident {
                                    panic!(
                                        "invalid syntax for #[as_ref] attribute on field `{}`",
                                        field
                                    );
                                } else {
                                    panic!(
                                        "invalid syntax for #[as_ref] attribute on field of type `{}`",
                                        field.ty.clone().into_token_stream()
                                    );
                                }
                            }
                        }
                        _ => false,
                    })
                    .count();

                match as_ref_count {
                    0 => {} // no AsRef impl generated
                    1 => {
                        let ty = &field.ty;
                        let field_name = if let Some(name) = &field.ident {
                            quote!(#name)
                        } else {
                            let index = Literal::usize_unsuffixed(index);
                            quote!(#index)
                        };
                        impls.push(s.gen_impl(quote! {
                            gen impl AsRef<#ty> for @Self {
                                fn as_ref(&self) -> &#ty { &self.#field_name }
                            }
                        }));
                    }
                    _ => {
                        let name = if let Some(name) = &field.ident {
                            name.into_token_stream()
                        } else {
                            field.ty.clone().into_token_stream()
                        };
                        panic!(
                            "too many #[as_ref] attributes on `{}` (only one is permitted)",
                            name
                        )
                    }
                }
            }
            impls
        }
        Data::Enum(e) => {
            for variant in &e.variants {
                deny_attr("as_ref", &variant.attrs);

                for field in &variant.fields {
                    deny_attr("as_ref", &field.attrs);
                }
            }
            Vec::new()
        }
        Data::Union(u) => {
            for field in &u.fields.named {
                deny_attr("as_ref", &field.attrs);
            }
            Vec::new()
        }
    };

    let asref_nocontext = s.gen_impl(quote!(
        extern crate hyperdrive;
        use hyperdrive::NoContext;

        gen impl AsRef<NoContext> for @Self {
            fn as_ref(&self) -> &NoContext { &NoContext }
        }
    ));
    let asref_self = s.gen_impl(quote!(
        gen impl AsRef<Self> for @Self {
            fn as_ref(&self) -> &Self { self }
        }
    ));
    let request_context = s.gen_impl(quote!(
        extern crate hyperdrive;
        use hyperdrive::RequestContext;

        gen impl RequestContext for @Self {}
    ));

    quote!(
        #asref_nocontext

        #asref_self

        #(#additional_impls)*

        #request_context
    )
}

fn deny_attr<'a, I>(name: &str, attrs: I)
where
    I: IntoIterator<Item = &'a Attribute>,
{
    for attr in attrs {
        if let Ok(meta) = attr.parse_meta() {
            if meta.name() == name {
                panic!("#[{}] attribute is only allowed on struct fields", name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::derive_request_context;
    use synstructure::test_derive;

    /// Expands the given item by putting a `#[derive(RequestContext)]` on it.
    macro_rules! expand {
        (
            $i:item
        ) => {
            test_derive! {
                derive_request_context {
                    $i
                }
                expands to {} no_build
            }
        };
    }

    #[test]
    #[should_panic(expected = "#[as_ref] attribute is only allowed on struct fields")]
    fn asref_on_struct() {
        expand! {
            #[as_ref]
            struct MyStruct {
                field: u8,
            }
        }
    }

    #[test]
    #[should_panic(expected = "#[as_ref] attribute is only allowed on struct fields")]
    fn asref_enum_field() {
        expand! {
            enum MyEnum {
                Variant {
                    #[as_ref]
                    field: u8,
                }
            }
        }
    }

    #[test]
    #[should_panic(expected = "#[as_ref] attribute is only allowed on struct fields")]
    fn asref_enum_variant() {
        expand! {
            enum MyEnum {
                #[as_ref]
                Variant {
                    field: u8,
                }
            }
        }
    }
}
