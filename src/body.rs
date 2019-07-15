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

use crate::{DefaultFuture, FromBody, FromRequestError, NoContext, NoCustomError};
use futures::{Future, Stream};
use serde::de::DeserializeOwned;
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

macro_rules! deref {
    ($t:ty) => {
        impl<T: DeserializeOwned + Send + 'static> Deref for $t {
            type Target = T;

            fn deref(&self) -> &T {
                &self.0
            }
        }

        impl<T: DeserializeOwned + Send + 'static> DerefMut for $t {
            fn deref_mut(&mut self) -> &mut T {
                &mut self.0
            }
        }
    };
}

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
/// # use hyperdrive::{FromRequest, body::HtmlForm, serde::Deserialize, NoContext};
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

impl<T> FromBody for HtmlForm<T>
where
    T: DeserializeOwned + Send + 'static,
{
    type Context = NoContext;
    type Error = NoCustomError;
    type Result = DefaultFuture<Self, FromRequestError<Self::Error>>;

    fn from_body(
        _request: &Arc<http::Request<()>>,
        body: hyper::Body,
        _context: &Self::Context,
    ) -> Self::Result {
        let fut = body
            .concat2()
            .map_err(FromRequestError::hyper_error)
            .and_then(|body| match serde_urlencoded::from_bytes(&body) {
                Ok(t) => Ok(HtmlForm(t)),
                Err(err) => Err(FromRequestError::malformed_body(err)),
            });

        Box::new(fut)
    }
}

deref!(HtmlForm<T>);

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

impl<T> FromBody for Json<T>
where
    T: DeserializeOwned + Send + 'static,
{
    type Context = NoContext;
    type Error = NoCustomError;
    type Result = DefaultFuture<Self, FromRequestError<Self::Error>>;

    fn from_body(
        _request: &Arc<http::Request<()>>,
        body: hyper::Body,
        _context: &Self::Context,
    ) -> Self::Result {
        let fut = body
            .concat2()
            .map_err(FromRequestError::hyper_error)
            .and_then(|body| match serde_json::from_slice(&body) {
                Ok(t) => Ok(Json(t)),
                Err(err) => Err(FromRequestError::malformed_body(err)),
            });

        Box::new(fut)
    }
}

deref!(Json<T>);
