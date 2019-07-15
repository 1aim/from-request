//! The custom derive powering the [`hyperdrive`] crate.
//!
//! You should never use this crate directly. It does not expose a stable API
//! and may break at any time. Use [`hyperdrive`] directly instead.
//!
//! [`hyperdrive`]: https://docs.rs/hyperdrive

#![recursion_limit = "256"]
#![warn(rust_2018_idioms)]

use synstructure::decl_derive;

mod from_request;
mod request_context;
mod utils;

use from_request::derive_from_request;
use request_context::derive_request_context;

decl_derive!([FromRequest, attributes(
    // Attributes need to be kept in sync with from_request/parse.rs

    context, error, body, forward, query_params,

    // We support all HTTP verbs from RFC 7231 as well as PATCH
    get, head, post, put, delete, connect, options, trace, patch

    // FIXME support arbitrary HTTP verbs (eg. for WebDAV)
)] => derive_from_request);

decl_derive!([RequestContext, attributes(
    as_ref
)] => derive_request_context);
