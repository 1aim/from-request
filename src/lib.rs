//! Composable asynchronous HTTP request routing, guarding and decoding.
//!
//! The most interesting thing in this crate is probably [`FromRequest`], so
//! refer to that for more info.
//!
//! [`FromRequest`]: trait.FromRequest.html

/*

TODO:
* How to handle 2015/2018 compat with the proc-macro?
* Good example that fetches a session from a DB

* Deserialize body with `serde_urlencoded`
*      "      ?query with `serde_qs`

Forward (by-move) to inner implementation to allow nesting of FromRequest types
(= better, more granular app structure).

we should call this "von Request" instead :D

*/

#![doc(html_root_url = "https://docs.rs/from-request/0.0.0")]
#![warn(missing_debug_implementations)]
#![warn(missing_docs)]
#![warn(bare_trait_objects)]

pub mod body;
mod error;
mod gen;

pub use error::*;
pub use from_request_derive::*;

// Reexport public deps for use by the custom derive
pub use {futures, http, hyper};

// These are hidden because the user never actually interacts with them. They're
// only used by the generated code internally.
#[doc(hidden)]
pub use {lazy_static::lazy_static, regex};

use futures::{Future, IntoFuture};

/// A default boxed future that may be returned from `FromRequest`
/// implementations.
///
/// The future is required to be `Send` to allow running it on a multi-threaded
/// executor.
pub type DefaultFuture<T, E> = Box<dyn Future<Item = T, Error = E> + Send>;

/// A boxed `std::error::Error` that can be used when the actual error type is
/// unknown.
pub type BoxedError = Box<dyn std::error::Error + Send + Sync>;

/// Trait for asynchronous conversion from HTTP requests.
///
/// # `#[derive(FromRequest)]`
///
/// This trait can be derived for enums to generate a request router and
/// decoder. Here's an example:
///
/// ```
/// # use from_request::FromRequest;
///
/// #[derive(FromRequest)]
/// enum Routes {
///     #[get("/")]
///     Index,
///
///     #[get("/users/{id}")]
///     User { id: u32 },
/// }
/// ```
///
/// TODO document everything about the derive
pub trait FromRequest: Sized {
    /// A context parameter passed to `from_request`.
    ///
    /// This can be used to pass application-specific data like a logger or a
    /// database connection around.
    ///
    /// If no context is needed, this should be set to `NoContext`, which is a
    /// context type that can be obtained from any `RequestContext` via `AsRef`.
    type Context: RequestContext;

    /// The result returned by `from_request`.
    ///
    /// Because `impl Trait` cannot be used inside traits (and named
    /// existentential types aren't yet stable), the type here might not be
    /// nameable. In that case, you can set it to `DefaultFuture<Self, Error>`
    /// and box the returned future.
    ///
    /// If your `FromRequest` implementation doesn't need to return a future
    /// (eg. because it's just a parsing step), you can set this to
    /// `Result<Self, ...>` and immediately return the result of the conversion.
    type Result: IntoFuture<Item = Self>;

    /// Create a `Self` from an HTTP request.
    ///
    /// This consumes the request *and* the context. You can set the context
    /// type to something like `Arc<Data>` to avoid expensive clones.
    ///
    /// # Parameters
    ///
    /// * **`request`**: An HTTP request from the `http` crate, containing a
    ///   `hyper::Body`.
    /// * **`context`**: User-defined context.
    fn from_request(request: http::Request<hyper::Body>, context: Self::Context) -> Self::Result;
}

/// A request guard that asynchronously checks a condition of an incoming
/// request.
///
/// For example, this could be used to extract an `Authorization` header and
/// verify user credentials, or to look up a session token in a database.
///
/// TODO: Better docs and examples
pub trait Guard: Sized {
    /// A context parameter passed to `from_request`.
    ///
    /// This can be used to pass application-specific data like a logger or a
    /// database connection around.
    ///
    /// If no context is needed, this should be set to `NoContext`.
    type Context: RequestContext;

    /// The result returned by `from_request`.
    ///
    /// Because `impl Trait` cannot be used inside traits (and named
    /// existentential types aren't stable), the type here might not be
    /// nameable. In that case, you can set it to `DefaultFuture<Self, Error>`
    /// and box the returned future.
    ///
    /// If your `FromRequest` implementation doesn't need to return a future
    /// (eg. because it's just a parsing step), you can set this to
    /// `Result<Self, ...>` and immediately return the result of the conversion.
    type Result: IntoFuture<Item = Self>;

    /// Create a `Self` from HTTP request data.
    ///
    /// This can inspect HTTP headers and other data provided by
    /// `http::Request`, but can not access the body of the request. If access
    /// to the body is needed, `FromBody` must be implemented instead.
    ///
    /// # Parameters
    ///
    /// * **`request`**: An HTTP request (without body) from the `http` crate.
    /// * **`context`**: User-defined context needed by the guard.
    fn from_request(request: &http::Request<()>, context: &Self::Context) -> Self::Result;
}

/// Asynchronous conversion from an HTTP request body.
///
/// Types implementing this trait are provided in the `body` module. They allow
/// easy deserialization from a variety of data formats.
///
/// # Examples
///
/// TODO: Example that extracts a `Json<T>`
pub trait FromBody: Sized {
    /// A context parameter passed to `from_body`.
    ///
    /// This can be used to pass application-specific data like a logger or a
    /// database connection around.
    ///
    /// If no context is needed, this should be set to `NoContext`.
    type Context: RequestContext;

    /// The result returned by `from_body`.
    ///
    /// Because `impl Trait` cannot be used inside traits (and named
    /// existentential types aren't stable), the type here might not be
    /// nameable. In that case, you can set it to `DefaultFuture<Self, Error>`
    /// and box the returned future.
    ///
    /// If your `FromRequest` implementation doesn't need to return a future
    /// (eg. because it's just a parsing step), you can set this to
    /// `Result<Self, ...>` and immediately return the result of the conversion.
    type Result: IntoFuture<Item = Self>;

    /// Create a `Self` from an HTTP request body.
    ///
    /// This will consume the body, so only one `FromBody` type can be used for
    /// every processed request.
    ///
    /// # Parameters
    ///
    /// * **`request`**: An HTTP request (without body) from the `http` crate.
    /// * **`body`**: The body stream. Implements `futures::Stream`.
    /// * **`context`**: User-defined context.
    fn from_body(
        request: &http::Request<()>,
        body: hyper::Body,
        context: &Self::Context,
    ) -> Self::Result;
}

/*

With this setup, #[derive(FromRequest)] essentially has 2 choices:
* Run all member fields `FromRequest` futures in series using `and_then`
* Run all futures in parallel using `join_all`

For the second option, all futures must have the same type.

The derive also has to set the assoc. `Future` to the right thing.


Also:

Don't want to box all futures, since many of them are gonna be `FutureResult`s
that can be evaluated immediately (eg. if it's just a parsing step).

=> The `and_then` solution seems much better - incoming requests can still be
   handled in parallel.

Problem: All web frameworks seem to define their own request type, with no hyper
interop.

*/

/// Default context used by `FromRequest` implementations.
///
/// This context type should be used whenever no application-specific context is
/// needed. It can be created from any parent context via `AsRef`.
#[derive(Debug, Copy, Clone, Default)]
pub struct NoContext;

/// Trait for context types passed to `FromRequest`.
///
/// # `#[derive(RequestContext)]`
///
/// This trait can be derived automatically. This will automatically implement
/// `AsRef<Self>` and `AsRef<NoContext>`.
///
/// On structs, fields can also be annotated using `#[as_ref]`, which generates
/// an additional implementation of `AsRef` for that field (note that all
/// `#[as_ref]` fields must have distinct types). This will automatically use
/// the field's type as a context when required by a `FromRequest` impl.
///
/// # Examples
///
/// Create your own context that allows running database queries in [`Guard`]s
/// and elsewhere:
/// ```
/// # use from_request::RequestContext;
/// # struct ConnectionPool {}
/// #
/// #[derive(RequestContext)]
/// struct MyContext {
///     db: ConnectionPool,
/// }
/// ```
///
/// Create a context that contains the above context and additional data:
/// ```
/// # use from_request::RequestContext;
/// # struct Logger {}
/// #
/// # #[derive(RequestContext)]
/// # struct MyContext {}
/// #
/// #[derive(RequestContext)]
/// struct BigContext {
///     #[as_ref]
///     inner: MyContext,
///     logger: Logger,
/// }
/// ```
/// This context can be used in the same places where `MyContext` is accepted,
/// but provides additional data that may be used only by some [`Guard`],
/// [`FromRequest`] or [`FromBody`] implementations.
///
/// [`Guard`]: trait.Guard.html
/// [`FromRequest`]: trait.FromRequest.html
/// [`FromBody`]: trait.FromBody.html
pub trait RequestContext: AsRef<Self> + AsRef<NoContext> {}

impl RequestContext for NoContext {}

impl AsRef<NoContext> for NoContext {
    fn as_ref(&self) -> &Self {
        &NoContext
    }
}
