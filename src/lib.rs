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
use tokio::runtime::current_thread::Runtime;

/// A default boxed future that may be returned from [`FromRequest`]
/// implementations.
///
/// The future is required to be `Send` to allow running it on a multi-threaded
/// executor.
///
/// [`FromRequest`]: trait.FromRequest.html
pub type DefaultFuture<T, E> = Box<dyn Future<Item = T, Error = E> + Send>;

/// A boxed `std::error::Error` that can be used when the actual error type is
/// unknown.
pub type BoxedError = Box<dyn std::error::Error + Send + Sync>;

/// Trait for asynchronous conversion from HTTP requests.
///
/// # `#[derive(FromRequest)]`
///
/// This trait can be derived for enums to generate a request router and
/// decoder. Here's a simple example:
///
/// ```
/// use from_request::{FromRequest, body::Json};
/// # use serde::Deserialize;
///
/// #[derive(FromRequest)]
/// enum Routes {
///     #[get("/")]
///     Index,
///
///     #[get("/users/{id}")]
///     User { id: u32 },
///
///     #[post("/login")]
///     Login {
///         #[body]
///         data: Json<Login>,
///     },
/// }
///
/// #[derive(Deserialize)]
/// struct Login {
///     email: String,
///     password: String,
/// }
/// ```
///
/// Calling `Routes::from_request` will result in `Routes::Index` for a `GET /`
/// request, and in `Routes::User` for a `GET /users/123` request, for example.
/// A `POST /login` request will end up as `Routes::Login`, decoding the POSTed
/// JSON body.
///
/// The generated `FromRequest` implementation will always use
/// [`DefaultFuture<Self, BoxedError>`][`DefaultFuture`] as the associated
/// `Result` type.
///
/// Note that the generated implementation will make use of `.and_then()` to
/// chain asynchronous operations instead of running them in parallel using
/// `join_all`. This is because it simplifies the code and doesn't require
/// making use of boxed futures everywhere in the generated code. Multiple
/// requests will still be handled in parallel, so this should not negatively
/// affect performance.
///
/// ## Extracting Path Segments (`{field}` syntax)
///
/// In a route attribute, the `{field}` placeholder syntax will match a path
/// segment and convert it to the type of the named field using `FromStr`:
///
/// ```ignore
/// #[get("/users/{id}")]
/// ```
///
/// To extract multiple path segments this way, the `{field...}` syntax can be
/// used at the end of the path, which will consume the rest of the path:
///
/// ```ignore
/// #[get("/static/{path...}")]
/// ```
///
/// If the `FromStr` conversion fails, the generated `FromRequest`
/// implementation will bail out with an error (in other words, this feature
/// cannot be used to try multiple routes in sequence until one matches).
///
/// ## Extracting the request body (`#[body]` attribute)
///
/// Putting `#[body]` on a field of a variant will deserialize the request body
/// using the [`FromBody`] trait and store the result in the annotated field:
///
/// ```ignore
/// #[post("/login")]
/// Login {
///     #[body]
///     data: Json<Login>,
/// },
/// ```
///
/// The type of the field must implement [`FromBody`]. The [`body` module]
/// contains predefined types implementing that trait.
///
/// ## Extracting query parameters (`#[query_params]` attribute)
///
/// The route attribute cannot match or extract query parameters (`?name=val`).
/// Instead, query parameters can be extracted by marking a field in the struct
/// with the `#[query_params]` attribute:
///
/// ```
/// use from_request::{FromRequest, body::Json};
/// # use serde::Deserialize;
///
/// #[derive(FromRequest)]
/// enum Routes {
///     #[get("/users")]
///     UserList {
///         #[query_params]
///         pagination: Option<Pagination>,
///     },
/// }
///
/// #[derive(Deserialize)]
/// struct Pagination {
///     start_id: u32,
///     count: u32,
/// }
/// ```
///
/// A request like `GET /users?start_id=42&count=10` would thus end up with a
/// corresponding `Pagination` object, while `GET /users` would store `None` in
/// the `pagination` field.
///
/// The type of the `#[query_params]` field must implement serde's `Deserialize`
/// trait and the conversion will be performed using the `serde_urlencoded`
/// crate.
///
/// # Changing the `Context` type
///
/// By default, the derive will use [`NoContext`] as the associated `Context`
/// type. You can change this by putting a `#[context = MyContext]` attribute on
/// the type:
///
/// TODO document everything about the derive
///
/// [`FromBody`]: trait.FromBody.html
/// [`NoContext`]: struct.NoContext.html
/// [`DefaultFuture`]: type.DefaultFuture.html
/// [`body` module]: body/index.html
pub trait FromRequest: Sized {
    /// A context parameter passed to [`from_request`].
    ///
    /// This can be used to pass application-specific data like a logger or a
    /// database connection around.
    ///
    /// If no context is needed, this should be set to [`NoContext`], which is a
    /// context type that can be obtained from any [`RequestContext`] via
    /// `AsRef`.
    ///
    /// [`from_request`]: #tymethod.from_request
    /// [`NoContext`]: struct.NoContext.html
    /// [`RequestContext`]: trait.RequestContext.html
    type Context: RequestContext;

    /// The result returned by [`from_request`].
    ///
    /// Because `impl Trait` cannot be used inside traits (and named
    /// existentential types aren't yet stable), the type here might not be
    /// nameable. In that case, you can set it to
    /// [`DefaultFuture<Self, Error>`][`DefaultFuture`] and box the returned
    /// future.
    ///
    /// If your `FromRequest` implementation doesn't need to return a future
    /// (eg. because it's just a parsing step), you can set this to
    /// `Result<Self, ...>` and immediately return the result of the conversion.
    ///
    /// [`DefaultFuture`]: type.DefaultFuture.html
    /// [`from_request`]: #tymethod.from_request
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

    /// Create a `Self` from an HTTP request, synchronously.
    ///
    /// This is a blocking version of [`from_request`]. The provided default
    /// implementation will internally create a single-threaded tokio runtime to
    /// perform the conversion and receive the request body.
    ///
    /// [`from_request`]: #tymethod.from_request
    fn from_request_sync(
        request: http::Request<hyper::Body>,
        context: Self::Context,
    ) -> Result<<Self::Result as IntoFuture>::Item, <Self::Result as IntoFuture>::Error> {
        let mut rt = Runtime::new().expect("couldn't start single-threaded tokio runtime");
        rt.block_on(Self::from_request(request, context).into_future())
    }
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
/// Types implementing this trait are provided in the [`body` module]. They
/// allow easy deserialization from a variety of data formats.
///
/// # Examples
///
/// TODO: Example that extracts a `Json<T>`
///
/// [`body` module]: body/index.html
pub trait FromBody: Sized {
    /// A context parameter passed to `from_body`.
    ///
    /// This can be used to pass application-specific data like a logger or a
    /// database connection around.
    ///
    /// If no context is needed, this should be set to [`NoContext`].
    ///
    /// [`NoContext`]: struct.NoContext.html
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

/// Default context used by [`FromRequest`] implementations.
///
/// This context type should be used whenever no application-specific context is
/// needed. It can be created from any parent context via `AsRef`.
///
/// [`FromRequest`]: trait.FromRequest.html
#[derive(Debug, Copy, Clone, Default)]
pub struct NoContext;

/// Trait for context types passed to [`FromRequest`].
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
