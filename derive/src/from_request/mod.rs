//! `FromRequest` derive.
//!
//! ```ignore
//! #[derive(FromRequest)]
//! enum Routes {
//!     #[get("/")]
//!     Index,
//!
//!     #[post("/login")]
//!     Login {
//!         #[body]
//!         data: LoginData,
//!         guard: MyGuard,
//!     },
//!
//!     #[get("/users/{user}/posts/{post}")]
//!     #[head("/users/{user}/posts/{post}")]
//!     Post { user: u32, post: u32 },
//!
//!     #[get("/users/{id}")]
//!     User {
//!         #[query_params]
//!         query: QueryParams,
//!     },
//! }
//! ```
//!
//! * Request path is matched completely (there is no regex support, although we
//!   use regex internally)
//! * Path segments either match a literal (`/user/`) or a placeholder using
//!   `FromStr` (`/:id`). The placeholder must not contain `/`, of course.
//! * Query params are ignored (but can be deserialized)
//!
//! Idea: A `#[sync]` on the type could use `Result<Self, Box<Error>>` as the
//! assoc. `Result` type instead of a future and generate a different
//! `from_request` body which makes everything work in a sync context.
//!
//! # Existing syntaxes
//!
//! ## rocket
//!
//! `"/my/<path>/<bla..>"`
//!
//! Placeholders must implement `FromParam` (for `<path>`) or `FromSegments`
//! (for `<bla..>`).
//!
//! ## rouille
//!
//! `/{id: u32}/bla`
//!
//! or
//!
//! `/{id}/bla`
//!
//! or
//!
//! `"/{id}/bla", id: u32`
//!
//! Placeholders must implement `FromStr`.
//!
//! ## tower-web
//!
//! `"/hello/:var"`
//!
//! Placeholders must implement `Extract`.

mod parse;

use self::parse::{ItemData, PathMap, VariantData};
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;
use std::iter;
use syn::Variant;

pub fn derive_from_request(s: synstructure::Structure) -> TokenStream {
    let en = match &s.ast().data {
        syn::Data::Struct(_) | syn::Data::Union(_) => {
            panic!("#[derive(FromRequest)] is only allowed on enums")
        }
        syn::Data::Enum(en) => en,
    };

    if !s.ast().generics.params.is_empty() {
        panic!("#[derive(FromRequest)] does not support generic types");
    }

    let item_data = ItemData::parse(&s.ast().attrs);

    let context = item_data.context().cloned().unwrap_or_else(|| {
        syn::parse_str("NoContext").expect("internal error: couldn't parse type")
    });

    let variant_data = s
        .variants()
        .iter()
        .map(|variant| {
            let data = VariantData::parse(&variant.ast());
            if !data.routes().is_empty() {
                // can be created by us
                match &variant.ast().fields {
                    syn::Fields::Unnamed(_) => panic!("tuple variants are not supported"),
                    _ => {}
                }
            }
            data
        })
        .collect::<Vec<_>>();
    let pathmap = PathMap::build(&variant_data);
    let all_regexes = pathmap
        .paths()
        .map(|p| p.regex().as_str().to_string())
        .collect::<Vec<_>>();
    let all_regexes = &all_regexes;

    if pathmap.paths().next().is_none() {
        // No route attributes. This situation would lead to "cannot infer type
        // for `T`" errors.
        panic!("at least one route attribute must be used");
    }

    let capturing_regexes = pathmap
        .paths()
        .map(|path| {
            let regex = path.regex();
            if regex.captures_len() > 0 {
                // Captures something, so we need to store it separately
                let r = regex.as_str();
                quote!(Some(Regex::new(#r).expect("internal error: generated invalid regex")))
            } else {
                quote!(None)
            }
        })
        .collect::<Vec<_>>();

    let (variants, variant_matches_path): (Vec<_>, Vec<_>) = variant_data
        .iter()
        .zip(s.variants())
        .filter_map(|(data, variant)| {
            if let Some(route) = data.routes().first() {
                let matches_path = if route.placeholders().is_empty() {
                    // If there's no placeholders, there's no FromStr impls we have to check
                    quote!(true)
                } else {
                    let tys = route
                        .placeholders()
                        .iter()
                        .map(|name| {
                            variant
                                .ast()
                                .fields
                                .iter()
                                .find(|field| field.ident.as_ref() == Some(name))
                                .expect("internal error: couldn't find field by name")
                                .ty
                                .clone()
                        })
                        .collect::<Vec<_>>();
                    let indices = tys
                        .iter()
                        .enumerate()
                        .map(|(i, _)| i + 1)
                        .collect::<Vec<_>>();

                    quote! {
                        let caps = regex
                            .captures(path)
                            .expect("internal error: regex first matched but now didn't?");

                        #( <#tys as FromStr>::from_str(
                            caps
                                .get(#indices)
                                .expect("internal error: capture group did not match anything")
                                .as_str()
                        ).is_ok() )&&*
                    }
                };
                Some((data.variant_name().clone(), matches_path))
            } else {
                // We only care about variants with at least one `#[method]`-style attr
                None
            }
        })
        .unzip();
    let variants = &variants;

    let regex_match_arms = pathmap
        .paths()
        .enumerate()
        .flat_map(|(i, pathinfo)| {
            pathinfo
                .method_map()
                .map(move |(method, variant)| {
                    let variant = &variant.variant_name();
                    quote! {
                        (#i, &http::Method::#method) => Variants::#variant,
                    }
                })
                .chain(iter::once({
                    // This arm matches when the path matches, but an incorrect method is used.
                    if pathinfo.regex().captures_len() == 0 {
                        // No captures, no FromStr: We have a fixed list of allowed methods
                        let methods = pathinfo.method_map().map(|(m, _)| m).collect::<Vec<_>>();

                        quote! {
                            (#i, _) => return Error::wrong_method(&[
                                #( Method::#methods, )*
                            ]).into_future(),
                        }
                    } else {
                        // FIXME determine the allowed routes
                        let (variants, methods): (Vec<_>, Vec<_>) = pathinfo
                            .method_map()
                            .map(|(method, variant)| (variant.variant_name(), method))
                            .unzip();

                        quote! {
                            (#i, _) => {
                                let path = request.uri().path();
                                let regex = REGEXES[#i].as_ref().unwrap();
                                let mut methods = Vec::new();

                                #(
                                    if Variants::#variants.matches_path(regex, path) {
                                        methods.push(&http::Method::#methods);
                                    }
                                )*

                                return Error::wrong_method(methods).into_future();
                            }
                        }
                    }
                }))
        })
        .collect::<Vec<_>>();

    let variant_arms = en
        .variants
        .iter()
        .zip(&variant_data)
        .filter_map(|(variant, data)| {
            if data.routes().is_empty() {
                None
            } else {
                Some(construct_variant(&s.ast().ident, variant, data))
            }
        })
        .collect::<Vec<_>>();

    s.gen_impl(quote!(
        extern crate from_request;
        use from_request::{
            FromBody, FromRequest, Guard, DefaultFuture, NoContext,
            ErrorKind, BoxedError, Error,
            http, hyper, lazy_static, regex::{RegexSet, Regex},
            futures::{IntoFuture, Future},
        };
        // Make sure `.as_ref()` always refers to the `AsRef` trait in libstd.
        // Otherwise the calling crate could override this.
        use core::convert::AsRef;
        use core::str::FromStr;

        gen impl FromRequest for @Self {
            type Future = DefaultFuture<Self, BoxedError>;
            type Context = #context;

            fn from_request(request: http::Request<hyper::Body>, context: Self::Context) -> Self::Future {
                // Step 0: `Variants` has all variants of the input enum that have a route attribute
                // but without any data.
                enum Variants {
                    #(#variants,)*
                }

                impl Variants {
                    fn matches_path(&self, regex: &Regex, path: &str) -> bool {
                        match self {
                            #( Variants::#variants => { #variant_matches_path } )*
                        }
                    }
                }

                // Step 1: Match against the generated regex set and inspect the HTTP
                // method in order to find the route that matches.
                lazy_static! {
                    static ref ROUTES: RegexSet = RegexSet::new(&[
                        #(#all_regexes,)*
                    ][..]).expect("invalid regex from FromRequest derive");

                    static ref REGEXES: Vec<Option<Regex>> = vec![
                        #(#capturing_regexes,)*
                    ];
                }

                let method = request.method();
                let path = request.uri().path();
                let matches = ROUTES.matches(path);
                debug_assert!(
                    matches.iter().count() <= 1,
                    "internal error: FromRequest derive produced overlapping regexes (path={},method={},regexes={:?})",
                    path, method, &[ #(#all_regexes),* ]
                );
                let index = match matches.iter().next() {
                    Some(index) => index,
                    None => return Error::from_kind(ErrorKind::NoMatchingRoute).into_future(),
                };

                let variant = match (index, method) {
                    #(#regex_match_arms)*
                    _ => unreachable!("FromRequest derive generated bad match"),
                };

                match variant {
                    #( Variants::#variants => #variant_arms, )*
                }
            }
        }
    ))
}

/// Generates all the code needed to build an enum variant from a matching
/// request.
///
/// Returns an expression.
///
/// The generated code will do the following:
/// * If the path has any segment placeholders:
///   * Obtain the captures with the specific regex for this route
///   * Call `FromStr` on all captured segments
/// * If it has `query_params`
///   * Deserialize from ?these&query=parameters
/// * For each guard (= field that isn't mentioned in any attribute)
///   * Chain all calls to the `from_request` methods
/// * If it has a `body`
///   * Chain the call to its `from_body` method
fn construct_variant(type_name: &Ident, variant: &Variant, data: &VariantData) -> TokenStream {
    // Must have at least 1 route, otherwise we wouldn't be here
    let route = data
        .routes()
        .first()
        .expect("internal error: no routes on variant");

    let field_by_name = |name: &Ident| -> &syn::Field {
        variant
            .fields
            .iter()
            .find(|field| field.ident.as_ref() == Some(name))
            .expect("internal error: couldn't find field by name")
    };

    let placeholders = if route.placeholders().is_empty() {
        quote!() // nothing to do
    } else {
        // For each placeholder, get its captured string and parse it
        let parse = route
            .placeholders()
            .iter()
            .enumerate()
            .map(|(i, field_name)| {
                let variable = Ident::new(&format!("fld_{}", field_name), Span::call_site());
                let capture = i + 1;
                let ty = &field_by_name(field_name).ty;
                quote! {
                    let #variable = captures
                        .get(#capture)
                        .expect("internal error: capture group did not match anything")
                        .as_str();
                    let #variable = match <#ty as FromStr>::from_str(#variable) {
                        Ok(v) => v,
                        Err(e) => return Error::with_source(ErrorKind::PathSegment, e).into_future(),
                    };
                }
            })
            .collect::<Vec<_>>();

        quote! {
            // Re-match the path with the right regex and get the captures
            let captures = REGEXES[index]
                .as_ref()
                .expect("internal error: no regex for route with placeholders")
                .captures(request.uri().path())
                .expect("internal error: regex first matched but now didn't?");

            #(#parse)*
        }
    };

    let query = if let Some(query_params_field) = data.query_params_field() {
        let ty = &field_by_name(&query_params_field).ty;
        let variable = Ident::new(&format!("fld_{}", query_params_field), Span::call_site());
        quote! {
            // Parse query params
            let raw_query = request.uri().query().unwrap_or("");
            let #variable = match serde_urlencoded::from_str::<#ty>(raw_query) {
                Ok(val) => val,
                Err(e) => return Error::with_source(ErrorKind::QueryParam, e).into_future(),
            };
        }
    } else {
        quote!()
    };

    // Last step, chain all the asynchronous operations (guards and body).
    // Reverse order because we have to chain everything with `.and_then`.

    // Construct the final value
    let variant_name = &variant.ident;
    let (fields, field_variables): (Vec<_>, Vec<_>) = variant
        .fields
        .iter()
        .filter_map(|field| field.ident.as_ref())
        .map(|field| {
            (
                field,
                Ident::new(&format!("fld_{}", field), Span::call_site()),
            )
        })
        .unzip();
    let mut future = quote! {
        Ok(#type_name::#variant_name {
            #(#fields: #field_variables,)*
        })
        .into_future()
    };

    // Read the body
    if let Some(body) = data.body_field() {
        let ty = &field_by_name(body).ty;
        let var = Ident::new(&format!("fld_{}", body), Span::call_site());
        future = quote! {
            <#ty as FromBody>::from_body(&headers, body, context.as_ref())
                .and_then(move |#var| #future)
        };
    };

    // Check all guards
    // Reverse order so guards are evaluated top to bottom in declaration order.
    for guard in data.guard_fields().iter().rev() {
        let ty = &field_by_name(guard).ty;
        let var = Ident::new(&format!("fld_{}", guard), Span::call_site());
        future = quote! {
            <#ty as Guard>::from_request(&headers, context.as_ref())
                .into_future()
                .and_then(move |#var| #future)
        };
    }

    quote! {{
        use std::str::FromStr;

        #placeholders

        #query

        // Before the async operations, split the incoming `Request<Body>` into
        // the headers (etc.) as a `Request<()>` and the `Body` itself.
        let (parts, body) = request.into_parts();
        let headers = http::Request::from_parts(parts, ());

        let future = #future;

        Box::new(future) as DefaultFuture<Self, BoxedError>
    }}
}

#[cfg(test)]
mod tests {
    use super::derive_from_request;
    use synstructure::test_derive;

    /// Expands the given item by putting a `#[derive(FromRequest)]` on it.
    macro_rules! expand {
        (
            $i:item
        ) => {
            test_derive! {
                derive_from_request {
                    $i
                }
                expands to {} no_build
            }
        };
    }

    #[test]
    #[should_panic(expected = "#[derive(FromRequest)] is only allowed on enums")]
    fn on_struct() {
        expand! {
            struct MyStruct {}
        }
    }

    #[test]
    #[should_panic(expected = "synstructure does not handle untagged unions")]
    // FIXME bad error message
    fn on_union() {
        expand! {
            union MyStruct {}
        }
    }

    #[test]
    #[should_panic(expected = "#[derive(FromRequest)] does not support generic types")]
    fn generics() {
        expand! {
            enum Routes<T> {
                #[get("/{ph}")]
                Variant {
                    ph: u32,
                    #[body]
                    body: T,
                }
            }
        }
    }

    #[test]
    #[should_panic(expected = "#[context] is not valid on enum variants")]
    fn context_attr_on_variant() {
        expand! {
            enum Routes {
                #[context(MyContext)]
                Variant,
            }
        }
    }

    #[test]
    #[should_panic(expected = "at least one route attribute must be used")]
    fn no_route() {
        expand! {
            enum Routes {
                Variant,
            }
        }
    }

    #[test]
    #[should_panic(expected = "different placeholders used")]
    fn wrong_routes() {
        expand! {
            enum Routes {
                #[get("/{ph}")]
                #[post("/{pl}")]
                Variant,
            }
        }
    }

    #[test]
    #[should_panic(
        expected = r#"duplicate route: `#[get("/{ph}")]` on `Variant` matches the same requests as `#[get("/{pl}")]` on `Var`"#
    )]
    fn dup_routes() {
        expand! {
            enum Routes {
                #[get("/{ph}")]
                Variant {
                    ph: u32,
                },

                #[get("/{pl}")]
                Var {
                    pl: u32,
                },
            }
        }
    }

    #[test]
    #[should_panic(expected = "duplicate placeholders")]
    fn dup_placeholder() {
        expand! {
            enum Routes {
                #[get("/{ph}/{ph}")]
                Variant {
                    #[allow(unused)]
                    ph: u32,
                },
            }
        }
    }

    #[test]
    #[should_panic(expected = "...-placeholders must not be followed by anything")]
    fn any_placeholder1() {
        expand! {
            enum Routes {
                #[get("/{ph}/{rest...}/")]
                Variant {
                    #[allow(unused)]
                    ph: u32,
                    #[allow(unused)]
                    rest: String,
                },
            }
        }
    }

    #[test]
    #[should_panic(expected = "...-placeholders must not be followed by anything")]
    fn any_placeholder2() {
        expand! {
            enum Routes {
                #[get("/{rest...}/more/{stuff}")]
                Variant {
                    #[allow(unused)]
                    rest: String,
                    #[allow(unused)]
                    stuff: String,
                },
            }
        }
    }

    #[test]
    #[should_panic(expected = "...-placeholders must not be followed by anything")]
    fn any_placeholder3() {
        expand! {
            enum Routes {
                #[get("/{rest...}/more/{stuff...}")]
                Variant {
                    #[allow(unused)]
                    rest: String,
                    #[allow(unused)]
                    stuff: String,
                },
            }
        }
    }

    #[test]
    #[should_panic(expected = "cannot mark a field with #[body]")]
    fn unrouted() {
        expand! {
            enum Routes {
                #[get("/")]
                Index,

                NoRoute {
                    #[body]
                    body: (),
                },
            }
        }
    }

    #[test]
    #[should_panic(
        expected = r#"route `#[get("/{ph}")]` overlaps with previously defined route `#[get("/0")]`"#
    )]
    fn overlap() {
        expand! {
            enum Routes {
                #[get("/0")]
                Var {},

                #[get("/{ph}")]
                Variant {
                    #[allow(unused)]
                    ph: u32,
                },
            }
        }
    }

    // TODO write lots more tests
}
