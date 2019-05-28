//! Provides wrappers that deserialize a request body.
//!
//! All wrappers provided here implement [`FromBody`].
//!
//! Note that the wrapper types will not inspect the `Content-Type` header and
//! instead assume that the body has the right format. You can add a [`Guard`]
//! if you want to reject requests that don't specify the right type.
//!
//! The wrappers will also ignore the `Content-Length` header. If you want to
//! limit the maximum request size, you can do that in a [`Guard`] as well.
//!
//! [`FromBody`]: ../trait.FromBody.html
//! [`Guard`]: ../trait.Guard.html

// TODO: Add many more types here and make them optional

use crate::{BoxedError, DefaultFuture, FromBody, NoContext};
use futures::{Future, Stream};
use serde::de::DeserializeOwned;

/// Decodes an `x-www-form-urlencoded` request body (eg. sent by an HTML form).
///
/// This uses [`serde_urlencoded`] to deserialize the request body.
/// The `Content-Type` and `Content-Length` headers are ignored.
///
/// [`serde_urlencoded`]: https://github.com/nox/serde_urlencoded
///
/// # Examples
///
/// Here's an example decoding the following HTML form:
///
/// ```html
/// <form method="POST" action="/login">
///     <input name="id" value="12345" type="hidden" />
///     <input name="user" />
///     <input name="password" type="password" />
///     <input type="submit" value="Log In" />
/// </form>
/// ```
///
/// ```
/// # use hyperdrive::{FromRequest, body::HtmlForm, serde::Deserialize, http, NoContext};
/// #[derive(Deserialize)]
/// struct LoginData {
///     id: u32,
///     user: String,
///     password: String,
/// }
///
/// #[derive(FromRequest)]
/// enum Route {
///     #[post("/login")]
///     LogIn {
///         #[body]
///         data: HtmlForm<LoginData>,
///     },
/// }
///
/// let data = "id=12345&user=myuser&password=hunter2";
///
/// let Route::LogIn { data: HtmlForm(form) } = Route::from_request_sync(
///     http::Request::post("/login").body(data.into()).unwrap(),
///     NoContext,
/// ).unwrap();
///
/// assert_eq!(form.id, 12345);
/// assert_eq!(form.user, "myuser");
/// assert_eq!(form.password, "hunter2");
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct HtmlForm<T: DeserializeOwned + Send + 'static>(pub T);

// Note that `serde_qs` offers more functionality than `serde_urlencoded`, but
// uses error-chain, so its error type isn't `Sync`, which unfortunately is
// rather annoying here.

impl<T: DeserializeOwned + Send + 'static> FromBody for HtmlForm<T> {
    type Context = NoContext;

    type Result = DefaultFuture<Self, BoxedError>;

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

/// Decodes a JSON-encoded request body.
///
/// The [`FromBody`] implementation of this type will retrieve the request body
/// and decode it as JSON using `serde_json`. The `Content-Type` and
/// `Content-Length` headers are ignored.
///
/// # Examples
///
/// ```
/// # use hyperdrive::{FromRequest, serde::Deserialize, body::Json, NoContext};
/// #[derive(Deserialize)]
/// struct BodyData {
///     id: u32,
///     names: Vec<String>,
/// }
///
/// #[derive(FromRequest)]
/// enum Route {
///     #[post("/json")]
///     Index {
///         #[body]
///         data: Json<BodyData>,
///     },
/// }
///
/// let data = r#"
/// {
///     "id": 123,
///     "names": [
///         "Joachim",
///         "Johannes",
///         "Jonathan"
///     ]
/// }
/// "#;
///
/// let Route::Index { data: Json(body) } = Route::from_request_sync(
///     http::Request::post("/json").body(data.into()).unwrap(),
///     NoContext,
/// ).unwrap();
///
/// assert_eq!(body.id, 123);
/// assert_eq!(body.names, vec![
///     "Joachim".to_string(),
///     "Johannes".to_string(),
///     "Jonathan".to_string(),
/// ]);
/// ```
///
/// [`FromBody`]: ../trait.FromBody.html
#[derive(Debug, PartialEq, Eq)]
pub struct Json<T: DeserializeOwned + Send + 'static>(pub T);

impl<T: DeserializeOwned + Send + 'static> FromBody for Json<T> {
    type Context = NoContext;

    type Result = DefaultFuture<Self, BoxedError>;

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
