//! The custom derive powering the [`from-request`] crate.
//!
//! You should never use this crate directly. It does not expose a stable API
//! and may break at any time. Use `from-request` directly instead.
//!
//! [`from-request`]: https://docs.rs/from-request

#![recursion_limit = "128"]

use synstructure::decl_derive;

mod from_request;
mod request_context;
mod utils;

use from_request::derive_from_request;
use request_context::derive_request_context;

decl_derive!([FromRequest, attributes(
    context, body, query_params,

    // We support all HTTP verbs from RFC 7231 as well as PATCH
    get, head, post, put, delete, connect, options, trace, patch

    // FIXME support arbitrary HTTP verbs (eg. for WebDAV)
)] => derive_from_request);

decl_derive!([RequestContext, attributes(
    as_ref
)] => derive_request_context);
