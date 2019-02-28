use crate::utils::ByProxy;
use indexmap::{map::Entry, IndexMap};
use proc_macro2::{Ident, Span};
use regex::Regex;
use syn::{Attribute, Lit, Meta, NestedMeta};
use synstructure::VariantAst;

const METHOD_ATTRS: &[&str] = &[
    "get", "head", "post", "put", "delete", "connect", "options", "trace", "patch",
];

/// All attributes used by this custom derive.
fn our_attrs() -> impl Iterator<Item = &'static str> {
    METHOD_ATTRS
        .iter()
        .chain(&["context", "body", "query_params"])
        .cloned()
}

/// Returns whether `name` is an attribute used by this custom derive.
fn known_attr(name: &Ident) -> bool {
    our_attrs().find(|s| name == s).is_some()
}

/// Returns whether `name` names an HTTP method (lowercase only).
fn is_method(name: &Ident) -> bool {
    METHOD_ATTRS.iter().cloned().find(|a| name == a).is_some()
}

/// Parsed attributes attached to the item that does `#[derive(FromRequest)]`.
pub struct ItemData {
    context: Option<syn::Type>,
}

impl ItemData {
    pub fn parse(attrs: &[Attribute]) -> Self {
        let mut context = None;

        for attr in attrs {
            let name = attr.parse_meta().unwrap().name();
            if name == "context" {
                let ty = syn::parse2(attr.tts.clone()).expect("#[context] must be given a type");
                insert("#[context]", &mut context, ty);
            } else if known_attr(&name) {
                panic!("#[{}] is not valid on items", name);
            }
        }

        Self { context }
    }

    pub fn context(&self) -> Option<&syn::Type> {
        self.context.as_ref()
    }
}

/// Attribute data attached to an enum variant or struct.
#[derive(Clone)]
pub struct VariantData {
    /// Name of the variant.
    name: Ident,
    routes: Vec<Route>,
    body_field: Option<Ident>,
    query_params_field: Option<Ident>,
    guard_fields: Vec<Ident>,
}

/// Describes where a field is decoded from.
#[derive(PartialEq)]
enum FieldKind {
    PathSegment,
    QueryParams,
    Body,
    Guard,
}

impl VariantData {
    pub fn parse(ast: &VariantAst) -> Self {
        // Collect all the route attributes on the variant
        let mut routes = Vec::new();
        for attr in ast.attrs {
            let meta = attr.parse_meta().unwrap();
            match &meta {
                Meta::List(list) if is_method(&meta.name()) => {
                    routes.push(Route::parse(
                        meta.name(),
                        &list.nested.iter().collect::<Vec<_>>(),
                    ));
                }
                _ if known_attr(&meta.name()) => {
                    panic!("#[{}] is not valid on enum variants", meta.name())
                }
                _ => {}
            }
        }

        // Since you're allowed to put multiple routes on a variant, they all
        // must have the same placeholders.
        if let Some((first, rest)) = routes.split_first() {
            for route in rest {
                // order doesn't matter, though
                if first.placeholders_sorted != route.placeholders_sorted {
                    let first = first
                        .placeholders_sorted
                        .iter()
                        .map(|ident| ident.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    let other = route
                        .placeholders_sorted
                        .iter()
                        .map(|ident| ident.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    panic!(
                        "different placeholders used on variant `{}`: `{}` vs. `{}`",
                        ast.ident, first, other
                    );
                }
            }
        }

        let placeholders = routes
            .first()
            .map(|route| &route.placeholders[..])
            .unwrap_or(&[]);

        // All placeholders must have fields with that name in the variant
        for placeholder in placeholders {
            if ast
                .fields
                .iter()
                .find(|field| field.ident.as_ref() == Some(placeholder))
                .is_none()
            {
                panic!(
                    "placeholder `:{}` does not refer to an existing field on variant `{}`",
                    placeholder, ast.ident,
                );
            }
        }

        // Now check all attributes on the variant's fields
        let mut body_field = None;
        let mut query_params_field = None;
        let mut guard_fields = Vec::new();
        for field in ast.fields.iter() {
            // Every field must have a role
            let mut field_kind = match &field.ident {
                Some(ident) if placeholders.contains(ident) => Some(FieldKind::PathSegment),
                _ => None,
            };

            for attr in &field.attrs {
                let meta = attr.parse_meta().unwrap();
                match &meta {
                    Meta::Word(ident) if ident == "body" => {
                        if let Some(ident) = &field.ident {
                            insert("#[body]", &mut body_field, ident.clone());
                        } else {
                            panic!("#[body] is not supported on unnamed fields");
                        }

                        insert("#[body]/#[query_params]", &mut field_kind, FieldKind::Body);
                    }
                    Meta::Word(ident) if ident == "query_params" => {
                        if let Some(ident) = &field.ident {
                            insert("#[query_params]", &mut query_params_field, ident.clone());
                        } else {
                            panic!("#[query_params] is not supported on unnamed fields");
                        }

                        insert(
                            "#[body]/#[query_params]",
                            &mut field_kind,
                            FieldKind::QueryParams,
                        );
                    }
                    _ if known_attr(&meta.name()) => {
                        panic!("#[{}] is not valid on enum variants", meta.name());
                    }
                    _ => {}
                }
            }

            // If there's no #[body]/#[query_params] on the field and it doesn't appear as a path
            // segment placeholder, it's a guard.
            let field_kind = field_kind.unwrap_or(FieldKind::Guard);

            if field_kind == FieldKind::Guard {
                guard_fields.push(
                    field
                        .ident
                        .clone()
                        .expect("#[derive(FromRequest)] requires named fields"),
                );
            }
        }

        Self {
            name: ast.ident.clone(),
            routes,
            body_field,
            query_params_field,
            guard_fields,
        }
    }

    pub fn variant_name(&self) -> &Ident {
        &self.name
    }

    pub fn routes(&self) -> &[Route] {
        &self.routes
    }

    pub fn body_field(&self) -> Option<&Ident> {
        self.body_field.as_ref()
    }

    pub fn query_params_field(&self) -> Option<&Ident> {
        self.query_params_field.as_ref()
    }

    pub fn guard_fields(&self) -> &[Ident] {
        &self.guard_fields
    }
}

/// A parsed HTTP route attribute (eg. `#[get("/path/{placeholder}/bla/{rest...}")]`).
#[derive(Clone)]
pub struct Route {
    /// Name of the associated constant on `http::Method`.
    method: Ident,
    /// The original path specified in the attribute.
    raw_path: String,
    /// The regular expression matching the path pattern. Captures all path
    /// segments that correspond to placeholders (`:thing`).
    regex: Regex,
    /// Placeholder field names.
    ///
    /// These must exist in the variant that carries this attribute, and their
    /// type must implement `FromStr` to perform the conversion.
    ///
    /// Sorted by order of appearance (this is important for associating the
    /// regex captures with the right field).
    placeholders: Vec<Ident>,
    /// Placeholder field names, sorted by name.
    placeholders_sorted: Vec<Ident>,
}

impl Route {
    fn parse(method: Ident, args: &[&NestedMeta]) -> Self {
        match args {
            [NestedMeta::Literal(Lit::Str(path))] => {
                let path = path.value();

                // Require paths to start with `/` to make them unambiguous.
                // They may or may not end with `/` - both ways refer to
                // different resources.
                if !path.starts_with("/") {
                    panic!("paths of route attributes must start with `/`");
                }

                let segments = path.split('/').skip(1).collect::<Vec<_>>();

                let mut regex = String::new();
                let mut placeholders = Vec::new();
                for (i, segment) in segments.iter().enumerate() {
                    if segment.starts_with('{') && segment.ends_with('}') {
                        let inner = &segment[1..segment.len() - 1];
                        if inner.ends_with("...") {
                            // "Rest" placeholder capturing *everything*. Only valid at the end.
                            if i != segments.len() - 1 {
                                panic!("...-placeholders must not be followed by anything");
                            }

                            let ident = &inner[..inner.len() - 3];
                            if !valid_ident(ident) {
                                panic!("placeholder `{}` must be a valid identifier", inner);
                            }

                            placeholders.push(Ident::new(ident, Span::call_site()));
                            regex.push_str("/(.*)");
                        } else {
                            // Else the placeholder must be a valid ident that will store a segment
                            if !valid_ident(inner) {
                                panic!("placeholder `{}` must be a valid identifier", inner);
                            }

                            placeholders.push(Ident::new(inner, Span::call_site()));
                            regex.push_str("/([^/]+)");
                        }
                    } else if segment.starts_with("\\{") {
                        // escaped `{`
                        regex.push('/');
                        // remove `\`, keep literal `:`
                        regex_syntax::escape_into(&segment[1..], &mut regex);
                    } else {
                        // literal
                        regex.push('/');
                        regex_syntax::escape_into(segment, &mut regex);
                    }
                }

                // Need to check that no duplicate placeholders were used
                let mut placeholders_sorted = placeholders.clone();
                placeholders_sorted.sort();
                let before = placeholders_sorted.len();
                placeholders_sorted.dedup();
                if placeholders_sorted.len() != before {
                    panic!("duplicate placeholders in route path `{}`", path);
                }

                Self {
                    method: Ident::new(&method.to_string().to_uppercase(), Span::call_site()),
                    raw_path: path,
                    regex: Regex::new(&regex).expect("FromRequest derive created invalid regex"),
                    placeholders,
                    placeholders_sorted,
                }
            }
            _ => {
                panic!("route attributes must be of the form `#[method(\"/path/to/match\")]`");
            }
        }
    }

    pub fn placeholders(&self) -> &[Ident] {
        &self.placeholders
    }
}

/// Maps generated path regexes to method->variant maps.
pub struct PathMap {
    regex_map: IndexMap<ByProxy<Regex, str>, IndexMap<Ident, (VariantData, Route)>>,
}

impl PathMap {
    pub fn build(variants: &[VariantData]) -> Self {
        let mut regexes = Vec::new();
        let mut regex_map = IndexMap::new();

        for variant in variants {
            for route in &variant.routes {
                let reg = ByProxy::new(route.regex.clone(), Regex::as_str);
                let entry = regex_map.entry(reg);
                let regex_index = entry.index();
                let route_map = entry.or_insert_with(IndexMap::new);
                match route_map.entry(route.method.clone()) {
                    Entry::Vacant(v) => {
                        // Map this path regex and method to the variant it was placed on:
                        let route = route.clone();

                        if regexes.len() == regex_index {
                            regexes.push((route.regex.clone(), !route.placeholders.is_empty()));
                        }

                        v.insert((variant.clone(), route));
                    }
                    Entry::Occupied(old) => {
                        // duplicate path declaration
                        let old = old.get();
                        panic!(
                            "duplicate route: `{}` on `{}` collides with `{}` on `{}`",
                            old.1.raw_path, old.0.name, route.raw_path, variant.name
                        );
                    }
                }
            }
        }

        Self { regex_map }
    }

    /// Returns an iterator over all unique paths in this map.
    pub fn paths(&self) -> impl Iterator<Item = PathInfo> {
        self.regex_map.iter().map(|(regex, method_map)| PathInfo {
            regex: regex.as_ref(),
            method_map,
        })
    }
}

pub struct PathInfo<'a> {
    regex: &'a Regex,
    method_map: &'a IndexMap<Ident, (VariantData, Route)>,
}

impl<'a> PathInfo<'a> {
    /// Returns the regex used to match this path.
    pub fn regex(&self) -> &'a Regex {
        &self.regex
    }

    /// Returns an iterator over all valid methods at this path.
    ///
    /// Note that since paths can overlap, this isn't necessarily the full set
    /// of idents (FIXME).
    pub fn methods(&self) -> impl Iterator<Item = &'a Ident> {
        self.method_map.keys()
    }

    /// Returns an iterator over the `Method => Variant` mappings.
    pub fn method_map(&self) -> impl Iterator<Item = (&'a Ident, &'a VariantData)> {
        self.method_map.iter().map(|(k, v)| (k, &v.0))
    }
}

fn insert<T>(name: &str, slot: &mut Option<T>, value: T) {
    if slot.is_some() {
        panic!("{} must only be specified once", name);
    }

    *slot = Some(value);
}

fn valid_ident(s: &str) -> bool {
    if s.is_empty() || s == "_" {
        return false;
    }

    match s.chars().next().unwrap() {
        'a'..='z' | 'A'..='Z' | '_' => {}
        _ => return false,
    }

    s.chars().skip(1).all(|c| match c {
        'a'..='z' | 'A'..='Z' | '0'..='9' | '_' => true,
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ident() {
        assert!(valid_ident("__"));
        assert!(valid_ident("_0"));
        assert!(valid_ident("a"));
        assert!(valid_ident("abc0"));
        assert!(valid_ident("abc0_"));
        assert!(!valid_ident("_"));
        assert!(!valid_ident(" "));
        assert!(!valid_ident(""));
        assert!(!valid_ident("0abc"));
    }
}
