use crate::utils::ByProxy;
use indexmap::{map::Entry, IndexMap};
use proc_macro2::{Ident, Span};
use regex::Regex;
use std::{fmt, slice};
use syn::{Attribute, Lit, Meta, NestedMeta};
use synstructure::VariantAst;

// Attributes need to be kept in sync with lib.rs

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

/// Returns whether `name` names an HTTP method attribute (lowercase only).
fn is_method(name: &Ident) -> bool {
    let name = name.to_string().to_lowercase();
    METHOD_ATTRS.iter().cloned().find(|a| name == *a).is_some()
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
    /// The parsed HTTP routes. There's one for each `#[method]`-style attribute
    /// on the variant.
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
        // They also have to be in the same order because we want to access the
        // captures by index.
        if let Some((first, rest)) = routes.split_first() {
            for route in rest {
                if first.placeholders() != route.placeholders() {
                    let first = first
                        .placeholders()
                        .iter()
                        .map(|ident| ident.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    let other = route
                        .placeholders()
                        .iter()
                        .map(|ident| ident.to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    panic!(
                        "different placeholders used on variant `{}`: `{}` vs. `{}` (they have to be in the same order)",
                        ast.ident, first, other
                    );
                }
            }
        }

        let placeholders = routes
            .first()
            .map(|route| route.placeholders())
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
                        panic!("#[{}] is not valid on fields", meta.name());
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

        // If there's no route, deny all attributes on fields as well
        if routes.is_empty() {
            if body_field.is_some() {
                panic!("cannot mark a field with #[body] when the variant doesn't have a route attribute");
            }

            if query_params_field.is_some() {
                panic!("cannot mark a field with #[query_params] when the variant doesn't have a route attribute");
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

    /// Returns the parsed route attributes attached to this variant.
    ///
    /// This might be empty, in which case the custom derive should just ignore
    /// the variant.
    pub fn routes(&self) -> &[Route] {
        &self.routes
    }

    /// Returns the name of the field marked with `#[body]`.
    ///
    /// If this is `None`, the body is ignored.
    pub fn body_field(&self) -> Option<&Ident> {
        self.body_field.as_ref()
    }

    /// Returns the name of the field marked with `#[query_params]`.
    ///
    /// If this is `None`, the query parameters are ignored.
    pub fn query_params_field(&self) -> Option<&Ident> {
        self.query_params_field.as_ref()
    }

    /// Returns the list of fields that store guard objects.
    pub fn guard_fields(&self) -> &[Ident] {
        &self.guard_fields
    }
}

/// A parsed HTTP route attribute (eg. `#[get("/path/{placeholder}/bla/{rest...}")]`).
#[derive(Clone)]
pub struct Route {
    /// Name of the associated constant on `http::Method`.
    method: Ident,
    path: RoutePath,
}

impl Route {
    fn parse(method: Ident, args: &[&NestedMeta]) -> Self {
        match args {
            [NestedMeta::Literal(Lit::Str(path))] => {
                let path = path.value();

                Self {
                    method: Ident::new(&method.to_string().to_uppercase(), Span::call_site()),
                    path: RoutePath::parse(path),
                }
            }
            _ => {
                panic!("route attributes must be of the form `#[method(\"/path/to/match\")]`");
            }
        }
    }

    pub fn placeholders(&self) -> &[Ident] {
        &self.path.placeholders
    }
}

impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let method = self.method.to_string().to_lowercase();
        if is_method(&self.method) {
            write!(f, "#[{}(\"{}\")]", method, self.path.raw)
        } else {
            // XXX this isn't yet implemented
            write!(f, "#[route({}, \"{}\")]", method, self.path.raw)
        }
    }
}

/// A parsed path of an HTTP route.
#[derive(Clone)]
pub struct RoutePath {
    /// The original path specified in the attribute.
    raw: String,
    /// The regular expression matching the path pattern. Captures all path
    /// segments that correspond to placeholders (`{thing}`).
    regex: Regex,
    /// The segments making up the path.
    ///
    /// If empty, this is the asterisk path `*`, which is different from all
    /// regular paths.
    segments: Vec<PathSegment>,
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

impl RoutePath {
    fn parse(path: String) -> Self {
        if path == "*" {
            return Self {
                raw: path,
                regex: Regex::new("\\*").unwrap(),
                segments: Vec::new(),
                placeholders: Vec::new(),
                placeholders_sorted: Vec::new(),
            };
        }

        // Require paths to start with `/` to make them unambiguous.
        // They may or may not end with `/` - both ways refer to
        // different resources.
        if !path.starts_with("/") {
            panic!("paths of route attributes must start with `/`");
        }

        let segments = path
            .split('/')
            .skip(1)
            .map(|s| PathSegment::parse(s.into()))
            .collect::<Vec<_>>();

        let mut regex = String::new();
        let mut placeholders = Vec::new();
        for (i, segment) in segments.iter().enumerate() {
            match segment {
                PathSegment::Rest(ident) => {
                    // "Rest" placeholder capturing *everything*. Only valid at the end.
                    if i != segments.len() - 1 {
                        panic!("...-placeholders must not be followed by anything");
                    }

                    placeholders.push(ident.clone());
                    regex.push_str("/(.*)");
                }
                PathSegment::Placeholder(ident) => {
                    placeholders.push(ident.clone());
                    regex.push_str("/([^/]+)");
                }
                PathSegment::Literal(literal) => {
                    regex.push('/');
                    regex_syntax::escape_into(literal, &mut regex);
                }
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
            raw: path,
            regex: Regex::new(&format!("^{}$", regex))
                .expect("FromRequest derive created invalid regex"),
            segments,
            placeholders,
            placeholders_sorted,
        }
    }

    /// Returns `true` if `self` and `other` match the exact same set of paths.
    fn matches_same_paths(&self, other: &Self) -> bool {
        self.regex.as_str() == other.regex.as_str()
    }

    /// Tries to find a route that can be matched by both `self` and `other`.
    pub fn find_overlap(&self, other: &Self) -> Option<String> {
        use self::PathSegment::*;

        if self.segments.is_empty() {
            // self is "*"
            if other.segments.is_empty() {
                return Some("*".into());
            } else {
                return None;
            }
        }

        let mut overlap = String::new();
        let mut saw_rest = false;
        for (a, b) in self.segments_fused().zip(other.segments_fused()) {
            match (a, b) {
                // If we reach any `Rest` placeholder there *must* be overlap
                (Rest(_), Rest(_)) => {
                    // Here we want to bail early to prevent an infinite loop (also we want the
                    // shortest counterexample)
                    overlap.push('/');
                    overlap.push_str(&a.matching_string());
                    return Some(overlap);
                }
                (Rest(_), other) | (other, Rest(_)) => {
                    overlap.push('/');
                    overlap.push_str(&other.matching_string());
                    saw_rest = true;
                }

                (Placeholder(a), Placeholder(_)) => {
                    overlap.push('/');
                    overlap.push_str(&a.to_string());
                }

                (Placeholder(_), Literal(lit)) | (Literal(lit), Placeholder(_)) => {
                    overlap.push('/');
                    overlap.push_str(&lit);
                }

                (Literal(a), Literal(b)) => {
                    if a == b {
                        overlap.push('/');
                        overlap.push_str(&a);
                    } else {
                        return None;
                    }
                }
            }
        }

        if self.segments.len() == other.segments.len() || saw_rest {
            Some(overlap)
        } else {
            // Different segment count can only overlap with "rest" placeholders, which is handled
            // above already
            None
        }
    }

    /// Returns an iterator over the path segments, fusing any "rest" placeholder (`{rest...}`).
    ///
    /// If the last placeholder is a "rest" placeholder, it will be yielded indefinitely.
    fn segments_fused(&self) -> impl Iterator<Item = &PathSegment> {
        assert!(
            !self.segments.is_empty(),
            "`*` path has no segments to iterate over"
        );
        SegmentsFused::Unfused(self.segments.iter())
    }
}

enum SegmentsFused<'a> {
    Unfused(slice::Iter<'a, PathSegment>),
    Fused(&'a PathSegment),
}

impl<'a> Iterator for SegmentsFused<'a> {
    type Item = &'a PathSegment;

    fn next(&mut self) -> Option<&'a PathSegment> {
        match self {
            SegmentsFused::Unfused(iter) => match iter.next() {
                None => None,
                Some(segment @ PathSegment::Rest(_)) => {
                    *self = SegmentsFused::Fused(segment);
                    Some(segment)
                }
                Some(other) => Some(other),
            },
            SegmentsFused::Fused(segment) => Some(segment),
        }
    }
}

/// Segment of a request path pattern.
#[derive(Clone)]
pub enum PathSegment {
    /// `{ident}`
    Placeholder(Ident),
    /// `{ident...}`
    Rest(Ident),
    /// `anything else`
    Literal(String),
}

impl PathSegment {
    fn parse(segment: String) -> Self {
        if segment.starts_with('{') && segment.ends_with('}') {
            let inner = &segment[1..segment.len() - 1];
            if inner.ends_with("...") {
                let ident = &inner[..inner.len() - 3];
                if !valid_ident(ident) {
                    panic!("placeholder `{}` must be a valid identifier", inner);
                }

                PathSegment::Rest(Ident::new(ident, Span::call_site()))
            } else {
                // Else the placeholder must be a valid ident that will store a segment
                if !valid_ident(inner) {
                    panic!("placeholder `{}` must be a valid identifier", inner);
                }

                PathSegment::Placeholder(Ident::new(inner, Span::call_site()))
            }
        } else {
            // literal
            PathSegment::Literal(segment)
        }
    }

    /// Creates an example path segment that would match `self`.
    fn matching_string(&self) -> String {
        match self {
            PathSegment::Placeholder(ident) => ident.to_string(),
            PathSegment::Rest(ident) => format!("{}...", ident),
            PathSegment::Literal(lit) => lit.clone(),
        }
    }
}

/// Maps generated path regexes to method->variant maps.
pub struct PathMap {
    regex_map: IndexMap<ByProxy<Regex, str>, IndexMap<Ident, (VariantData, Route)>>,
}

impl PathMap {
    pub fn build(variants: &[VariantData]) -> Self {
        let mut this = Self {
            regex_map: IndexMap::new(),
        };

        for variant in variants {
            for route in &variant.routes {
                // Check for overlap with all previously registered routes
                for prev_route in this
                    .regex_map
                    .values()
                    .flat_map(|m| m.values().map(|(_, r)| r))
                    .filter(|r| !r.path.matches_same_paths(&route.path))
                {
                    if let Some(overlap) = prev_route.path.find_overlap(&route.path) {
                        panic!(
                            "route `{}` overlaps with previously defined route `{}` (both would match path `{}`)",
                            route, prev_route, overlap
                        );
                    }
                }

                this.add_route(variant.clone(), route.clone());
            }
        }

        // For each GET route, register a matching HEAD route if none exists
        let any_head_overlaps_with = |new_route: &Route| {
            this.regex_map
                .values()
                .flat_map(|map| {
                    map.iter().filter_map(|(method, (_, route))| {
                        if method.to_string() == "HEAD" {
                            Some(route)
                        } else {
                            None
                        }
                    })
                })
                .any(|route| route.path.find_overlap(&new_route.path).is_some())
        };
        let mut implied_head_routes = Vec::new();
        for route_map in this.regex_map.values() {
            for (method, (variant, route)) in route_map.iter() {
                if method.to_string() == "GET" {
                    let head = Route {
                        method: Ident::new("HEAD", Span::call_site()),
                        path: route.path.clone(),
                    };
                    if !any_head_overlaps_with(&head) {
                        implied_head_routes.push((variant.clone(), head));
                    }
                }
            }
        }

        for (variant, route) in implied_head_routes {
            this.add_route(variant, route);
        }

        this
    }

    fn add_route(&mut self, variant: VariantData, route: Route) {
        let reg = ByProxy::new(route.path.regex.clone(), Regex::as_str);
        let entry = self.regex_map.entry(reg);
        let route_map = entry.or_insert_with(IndexMap::new);
        match route_map.entry(route.method.clone()) {
            Entry::Vacant(v) => {
                // Map this path regex and method to the variant it was placed on:
                v.insert((variant, route));
            }
            Entry::Occupied(old) => {
                // duplicate path declaration
                let old = old.get();
                panic!(
                    "duplicate route: `{}` on `{}` matches the same requests as `{}` on `{}`",
                    old.1, old.0.name, route, variant.name
                );
            }
        }
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

    /// Returns an iterator over the `Method => Variant` mappings for this path.
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

    #[test]
    fn overlap() {
        macro_rules! intersect {
            ($a:literal, $b:literal) => {{
                RoutePath::parse($a.to_string())
                    .find_overlap(&RoutePath::parse($b.to_string()))
                    .as_ref()
                    .map(|s| s.as_str())
            }};
        }

        assert_eq!(intersect!("/", "/literal"), None);
        assert_eq!(intersect!("/", "/"), Some("/"));
        assert_eq!(intersect!("/a", "/b"), None);
        assert_eq!(intersect!("/abc", "/abc"), Some("/abc"));
        assert_eq!(intersect!("/{a}", "/b"), Some("/b"));
        assert_eq!(intersect!("/a", "/{b}"), Some("/a"));
        assert_eq!(intersect!("/{a}", "/{b}"), Some("/a"));
        assert_eq!(intersect!("/{a}/", "/{b}"), None);
        assert_eq!(intersect!("/{a}/", "/{b}/lit"), None);
        assert_eq!(intersect!("/{a}/{x}", "/{b}/{y}"), Some("/a/x"));
        assert_eq!(intersect!("/{a}/{x...}", "/{b}"), None);
        assert_eq!(intersect!("/{a}/{x...}", "/{b...}"), Some("/a/x..."));
        assert_eq!(intersect!("/lit/{x...}", "/{b...}"), Some("/lit/x..."));
        assert_eq!(intersect!("/lit/bla", "/{b...}"), Some("/lit/bla"));
        assert_eq!(intersect!("/lit/bla", "/lit/{b...}"), Some("/lit/bla"));
        assert_eq!(intersect!("/lit/bla", "/blit/{b...}"), None);
        assert_eq!(intersect!("*", "/{b...}"), None);
        assert_eq!(intersect!("*", "/"), None);
        assert_eq!(intersect!("*", "*"), Some("*"));
    }
}
