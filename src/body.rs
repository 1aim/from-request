//! Provides wrappers that implement `FromRequest` by deserializing a request
//! body.

// TODO: Should these check `Content-Type`?
// TODO: Who should enforce a Content-Length limit? document that these structs don't.
// TODO: Add many more types here and make them optional

use crate::{DefaultFuture, FromBody, NoContext};
use futures::{Future, Stream};
use serde::de::DeserializeOwned;
use std::error::Error;

/// Decodes an `x-www-form-urlencoded` request body (eg. sent by an HTML form).
///
/// This uses [`serde_urlencoded`] to deserialize the request body.
/// `Content-Type` is ignored.
///
/// [`serde_urlencoded`]: https://github.com/nox/serde_urlencoded
///
/// # Examples
///
/// TODO: Example that includes an HTML form definition
#[derive(Debug)]
pub struct HtmlForm<T: DeserializeOwned + Send + 'static>(pub T);

// Note that `serde_qs` offers more functionality than `serde_urlencoded`, but
// uses error-chain, so its error type isn't `Sync`, which unfortunately is
// rather annoying here.

impl<T: DeserializeOwned + Send + 'static> FromBody for HtmlForm<T> {
    type Context = NoContext;

    // TODO use our error type
    type Result = DefaultFuture<Self, Box<dyn Error + Send + Sync>>;

    fn from_body(
        _request: &http::Request<()>,
        body: hyper::Body,
        _context: &Self::Context,
    ) -> Self::Result {
        Box::new(body.concat2().map_err(Into::into).and_then(|body| {
            match serde_urlencoded::from_bytes(&body) {
                Ok(t) => Ok(HtmlForm(t)),
                Err(e) => Err(e.into()),
            }
        }))
    }
}

/// JSON-encoded request body that will decode to a `T`.
///
/// The `FromBody` implementation of this type will retrieve the request body
/// and decode it as JSON using `serde_json`. `Content-Type` is ignored.
#[derive(Debug)]
pub struct Json<T: DeserializeOwned + Send + 'static>(pub T);

impl<T: DeserializeOwned + Send + 'static> FromBody for Json<T> {
    type Context = NoContext;

    type Result = DefaultFuture<Self, Box<dyn Error + Send + Sync>>;

    fn from_body(
        _request: &http::Request<()>,
        body: hyper::Body,
        _context: &Self::Context,
    ) -> Self::Result {
        Box::new(body.concat2().map_err(Into::into).and_then(|body| {
            match serde_json::from_slice(&body) {
                Ok(t) => Ok(Json(t)),
                Err(e) => Err(e.into()),
            }
        }))
    }
}
