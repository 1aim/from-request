//! Implements hyper `Service` adapters that reduce boilerplate.
//!
//! This module contains adapters for hyper's `Service` trait that make common
//! operations easier and require less boilerplate:
//! * [`AsyncService`] and [`SyncService`] can be directly passed to a hyper
//!   server and will decode incoming requests for you and invoke a handler
//!   closure. They make it very easy to use any type implementing
//!   [`FromRequest`] as the main entry point of your app.
//! * [`ServiceExt`] provides adapter methods on Hyper `Service`s that simplify
//!   common patterns like catching panics.
//!
//! [`AsyncService`]: struct.AsyncService.html
//! [`SyncService`]: struct.SyncService.html
//! [`ServiceExt`]: trait.ServiceExt.html
//! [`FromRequest`]: ../trait.FromRequest.html

use crate::{BoxedError, DefaultFuture, Error, FromRequest, NoContext};
use futures::{future::FutureResult, Future, IntoFuture};
use hyper::{
    service::{MakeService, Service},
    Body, Method, Request, Response,
};
use std::any::Any;
use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

/// Asynchronous hyper service adapter.
///
/// This implements `hyper::service::Service`, decodes incoming requests using
/// [`FromRequest`] and passes the result to a provided handler closure.
///
/// Using this type takes a bit of boilerplate away from your app. Specifically,
/// it handles:
///
/// * Suppressing the body of the response when the request used `HEAD`.
/// * Turning any [`hyperdrive::Error`] into a proper HTTP response.
///
/// This type stores an async request handler `H` and the context needed by the
/// [`FromRequest`] implementation. The context is cloned for every request.
///
/// # Type Parameters
///
/// * **`H`**: The handler closure. Takes a [`FromRequest`] implementor `R`, and
///   the original request. Returns a future resolving to the response to return
///   to the client. Shared via `Arc`.
/// * **`R`**: The request type expected by the handler `H`. Implements
///   [`FromRequest`].
/// * **`F`**: The `Future` returned by the handler closure `H`.
///
/// # Examples
///
/// ```
/// use hyperdrive::{FromRequest, service::AsyncService};
/// use hyper::{Server, Response, Request, Body};
/// use futures::prelude::*;
/// use std::sync::Arc;
///
/// #[derive(FromRequest)]
/// enum Route {
///     #[get("/")]
///     Index,
/// }
///
/// let service = AsyncService::new(|route: Route, orig: Arc<Request<()>>| {
///     // The closure is called with the `FromRequest`-implementing type and
///     // the original request. It has to return any type implementing
///     // `Future`.
///     match route {
///         Route::Index => {
///             Ok(Response::new(Body::from("Hello World!"))).into_future()
///         }
///     }
/// });
///
/// // Create the server future:
/// let srv = Server::bind(&"127.0.0.1:0".parse().unwrap())
///    .serve(service);
/// ```
///
/// [`FromRequest`]: ../trait.FromRequest.html
/// [`hyperdrive::Error`]: ../struct.Error.html
pub struct AsyncService<H, R, F>
where
    H: Fn(R, Arc<Request<()>>) -> F + Send + Sync + 'static,
    R: FromRequest,
    R::Context: Clone,
    R::Future: 'static,
    F: Future<Item = Response<Body>, Error = BoxedError> + Send + 'static,
{
    handler: Arc<H>,
    context: R::Context,
}

impl<H, R, F> AsyncService<H, R, F>
where
    H: Fn(R, Arc<Request<()>>) -> F + Send + Sync + 'static,
    R: FromRequest<Context = NoContext>,
    R::Future: 'static,
    F: Future<Item = Response<Body>, Error = BoxedError> + Send + 'static,
{
    /// Creates an `AsyncService` from a handler closure.
    ///
    /// This will pass a [`NoContext`] to the [`FromRequest`] implementation,
    /// which will work fine as long as your type doesn't require a custom
    /// context. If you need to pass a custom context, refer to
    /// [`with_context`].
    ///
    /// [`NoContext`]: ../struct.NoContext.html
    /// [`FromRequest`]: ../trait.FromRequest.html
    /// [`with_context`]: #method.with_context
    pub fn new(handler: H) -> Self {
        Self::with_context(handler, NoContext)
    }
}

impl<H, R, F> AsyncService<H, R, F>
where
    H: Fn(R, Arc<Request<()>>) -> F + Send + Sync + 'static,
    R: FromRequest,
    R::Context: Clone,
    R::Future: 'static,
    F: Future<Item = Response<Body>, Error = BoxedError> + Send + 'static,
{
    /// Creates an `AsyncService` that will call `handler` to process incoming
    /// requests.
    ///
    /// # Parameters
    ///
    /// * **`handler`**: The handler closure. This is stored in an `Arc` and is
    ///   passed every decoded request `R`. Returns a future `F` resolving to
    ///   the response to return.
    /// * **`context`**: The context to pass to your [`FromRequest`]
    ///   implementor.
    ///
    /// [`FromRequest`]: ../trait.FromRequest.html
    pub fn with_context(handler: H, context: R::Context) -> Self {
        Self {
            handler: Arc::new(handler),
            context,
        }
    }
}

impl<H, R, F> Clone for AsyncService<H, R, F>
where
    H: Fn(R, Arc<Request<()>>) -> F + Send + Sync + 'static,
    R: FromRequest,
    R::Context: Clone,
    R::Future: 'static,
    F: Future<Item = Response<Body>, Error = BoxedError> + Send + 'static,
{
    fn clone(&self) -> Self {
        Self {
            handler: self.handler.clone(),
            context: self.context.clone(),
        }
    }
}

impl<C, H, R, F> MakeService<C> for AsyncService<H, R, F>
where
    H: Fn(R, Arc<Request<()>>) -> F + Send + Sync + 'static,
    R: FromRequest,
    R::Context: Clone,
    R::Future: 'static,
    F: Future<Item = Response<Body>, Error = BoxedError> + Send + 'static,
{
    type ReqBody = Body;
    type ResBody = Body;
    type Error = BoxedError;
    type Service = Self;
    type Future = FutureResult<Self, BoxedError>;
    type MakeError = BoxedError;

    fn make_service(&mut self, _ctx: C) -> Self::Future {
        Ok(self.clone()).into_future()
    }
}

impl<H, R, F> Service for AsyncService<H, R, F>
where
    H: Fn(R, Arc<Request<()>>) -> F + Send + Sync + 'static,
    R: FromRequest,
    R::Context: Clone,
    R::Future: 'static,
    F: Future<Item = Response<Body>, Error = BoxedError> + Send + 'static,
{
    type ReqBody = Body;
    type ResBody = Body;
    type Error = BoxedError;
    type Future = DefaultFuture<Response<Body>, BoxedError>;

    fn call(&mut self, req: Request<Self::ReqBody>) -> Self::Future {
        let is_head = req.method() == Method::HEAD;
        let handler = self.handler.clone();
        let (parts, body) = req.into_parts();
        let req = Arc::new(Request::from_parts(parts, ()));
        let fut = R::from_request_and_body(&req, body, self.context.clone())
            .and_then(move |r| handler(r, req))
            .map(move |response| {
                if is_head {
                    // Responses to HEAD requests must have an empty body
                    response.map(|_| Body::empty())
                } else {
                    response
                }
            })
            .or_else(|err| {
                if let Some(our_error) = err.downcast_ref::<Error>() {
                    Ok(our_error.response().map(|()| Body::empty()))
                } else {
                    Err(err)
                }
            });

        Box::new(fut)
    }
}

impl<H, R, F> fmt::Debug for AsyncService<H, R, F>
where
    H: Fn(R, Arc<Request<()>>) -> F + Send + Sync + 'static,
    R: FromRequest,
    R::Context: Clone + fmt::Debug,
    R::Future: 'static,
    F: Future<Item = Response<Body>, Error = BoxedError> + Send + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Closures aren't debug-printable, so we print a few Arc stats instead

        #[derive(Debug)]
        struct HandlerRef {
            strong_count: usize,
            weak_count: usize,
        }

        f.debug_struct("AsyncService")
            .field(
                "handler",
                &HandlerRef {
                    strong_count: Arc::strong_count(&self.handler),
                    weak_count: Arc::weak_count(&self.handler),
                },
            )
            .field("context", &self.context)
            .finish()
    }
}

/// A hyper `Service` that dispatches requests to a blocking handler.
///
/// Just like [`AsyncService`], using this type takes a bit of boilerplate away
/// from your app. Specifically, it handles:
///
/// * Suppressing the body of the response when the request used `HEAD`.
/// * Turning any [`hyperdrive::Error`] into a proper HTTP response.
///
/// This is effectively a bridge between async hyper and a synchronous,
/// blocking app. Writing sync code is much simpler than writing async code
/// (even with async/await syntax), so depending on your app this might be a
/// good tradeoff.
///
/// # Type Parameters
///
/// * **`H`**: The handler closure. It is called with the request type `R` and
///   the original request. It has to return the `Response<Body>` to send to the
///   client.
/// * **`R`**: The request type implementing `FromRequest`.
///
/// # Examples
///
/// ```
/// use hyperdrive::{FromRequest, service::SyncService};
/// use hyper::{Request, Response, Body, Server};
/// use std::sync::Arc;
///
/// #[derive(FromRequest)]
/// enum Route {
///     #[get("/")]
///     Index,
/// }
///
/// let service = SyncService::new(|route: Route, orig: Arc<Request<()>>| {
///     match route {
///         Route::Index => Response::new(Body::from("Hello world!")),
///     }
/// });
///
/// // Create the server future:
/// let srv = Server::bind(&"127.0.0.1:0".parse().unwrap())
///    .serve(service);
/// ```
///
/// [`AsyncService`]: struct.AsyncService.html
/// [`hyperdrive::Error`]: ../struct.Error.html
pub struct SyncService<H, R>
where
    H: Fn(R, Arc<Request<()>>) -> Response<Body> + Send + Sync + 'static,
    R: FromRequest + Send + 'static,
    R::Context: Clone,
{
    handler: Arc<H>,
    context: R::Context,
}

impl<H, R> SyncService<H, R>
where
    H: Fn(R, Arc<Request<()>>) -> Response<Body> + Send + Sync + 'static,
    R: FromRequest<Context = NoContext> + Send + 'static,
{
    /// Creates a `SyncService` that will call `handler` to process incoming
    /// requests.
    pub fn new(handler: H) -> Self {
        Self::with_context(handler, NoContext)
    }
}

impl<H, R> SyncService<H, R>
where
    H: Fn(R, Arc<Request<()>>) -> Response<Body> + Send + Sync + 'static,
    R: FromRequest + Send + 'static,
    R::Context: Clone,
{
    /// Creates a `SyncService` that will call `handler` to process incoming
    /// requests.
    ///
    /// # Parameters
    ///
    /// * **`handler`**: The handler closure. This is stored in an `Arc` and is
    ///   called with every decoded request `R`. Returns the response to return
    ///   to the client.
    /// * **`context`**: The context to pass to your [`FromRequest`]
    ///   implementor. If you don't need a special context, [`new()`] should be
    ///   called instead.
    ///
    /// [`new()`]: #method.new
    /// [`FromRequest`]: ../trait.FromRequest.html
    pub fn with_context(handler: H, context: R::Context) -> Self {
        Self {
            handler: Arc::new(handler),
            context,
        }
    }
}

impl<H, R> Clone for SyncService<H, R>
where
    H: Fn(R, Arc<Request<()>>) -> Response<Body> + Send + Sync + 'static,
    R: FromRequest + Send + 'static,
    R::Context: Clone,
{
    fn clone(&self) -> Self {
        Self {
            handler: self.handler.clone(),
            context: self.context.clone(),
        }
    }
}

impl<C, H, R> MakeService<C> for SyncService<H, R>
where
    H: Fn(R, Arc<Request<()>>) -> Response<Body> + Send + Sync + 'static,
    R: FromRequest + Send + 'static,
    R::Context: Clone,
{
    type ReqBody = Body;
    type ResBody = Body;
    type Error = BoxedError;
    type Service = Self;
    type Future = FutureResult<Self, BoxedError>;
    type MakeError = BoxedError;

    fn make_service(&mut self, _ctx: C) -> Self::Future {
        Ok(self.clone()).into_future()
    }
}

impl<H, R> Service for SyncService<H, R>
where
    H: Fn(R, Arc<Request<()>>) -> Response<Body> + Send + Sync + 'static,
    R: FromRequest + Send + 'static,
    R::Context: Clone,
{
    type ReqBody = Body;
    type ResBody = Body;
    type Error = BoxedError;
    type Future = DefaultFuture<Response<Body>, BoxedError>;

    fn call(&mut self, req: Request<Self::ReqBody>) -> Self::Future {
        let is_head = req.method() == Method::HEAD;
        let handler = self.handler.clone();

        let (parts, body) = req.into_parts();
        let req = Arc::new(Request::from_parts(parts, ()));

        let fut = R::from_request_and_body(&req, body, self.context.clone())
            .and_then(move |route| {
                // Run the sync handler on the blocking thread pool.
                crate::blocking(move || Ok(handler(route, req)))
            })
            .map(move |response| {
                if is_head {
                    // Responses to HEAD requests must have an empty body
                    response.map(|_| Body::empty())
                } else {
                    response
                }
            })
            .or_else(|err| {
                if let Some(our_error) = err.downcast_ref::<Error>() {
                    Ok(our_error.response().map(|()| Body::empty()))
                } else {
                    Err(err)
                }
            });

        Box::new(fut)
    }
}

impl<H, R> fmt::Debug for SyncService<H, R>
where
    H: Fn(R, Arc<Request<()>>) -> Response<Body> + Send + Sync + 'static,
    R: FromRequest + Send + 'static,
    R::Context: Clone + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Closures aren't debug-printable, so we print a few Arc stats instead

        #[derive(Debug)]
        struct HandlerRef {
            strong_count: usize,
            weak_count: usize,
        }

        f.debug_struct("SyncService")
            .field(
                "handler",
                &HandlerRef {
                    strong_count: Arc::strong_count(&self.handler),
                    weak_count: Arc::weak_count(&self.handler),
                },
            )
            .field("context", &self.context)
            .finish()
    }
}

/// Extension trait for types implementing Hyper's `Service` trait.
///
/// This adds a number of convenience methods that can be used to build robust
/// applications with Hyper and Hyperdrive.
pub trait ServiceExt: Service + Sized {
    /// Catches any panics that occur in the service `self`, and calls an
    /// asynchronous panic handler with the panic payload.
    ///
    /// The `handler` can decide if and how the request should be answered. A
    /// common option is to return a `500 Internal Server Error` response to the
    /// client. If the handler returns an error, the connection will be dropped
    /// and no response will be sent, which mirrors the behavior of Hyper.
    ///
    /// **Note**: Panics occurring inside of `handler` will not be caught again.
    /// The behavior in this case depends on the futures executor in use. When
    /// using tokio, it will catch the panic in the worker thread and recover.
    /// The connection to the client will be dropped.
    ///
    /// **Note**: Like `std::panic::catch_unwind`, this only works when the
    /// final binary is compiled with `panic = unwind` (the default). Using
    /// `panic = abort` will *always* abort the whole process on any panic and
    /// cannot be caught.
    ///
    /// **Note**: This mechanism is not very suitable for *logging* panics,
    /// since no useful backtrace can be constructed and no location information
    /// is available. The panic hook mechanism in the standard library is better
    /// suited for that (see `std::panic::set_hook`).
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use hyperdrive::{FromRequest, service::*};
    /// use hyper::{Body, Server, Response};
    /// use futures::Future;
    /// use http::StatusCode;
    ///
    /// #[derive(FromRequest)]
    /// enum Routes {
    ///     #[get("/")]
    ///     Panic,
    /// }
    ///
    /// let service = SyncService::new(|route: Routes, orig_request| {
    ///     match route {
    ///         Routes::Panic => panic!("Oops, something went wrong!"),
    ///     }
    /// }).catch_unwind(|panic_payload| {
    ///     // We ignore the payload here. We could also downcast it to `String`/`&'static str`
    ///     // and include it in the response.
    ///     let _ = panic_payload;
    ///
    ///     let message = r#"
    ///         <!DOCTYPE html>
    ///         <html>
    ///         <body>
    ///             <h1>Internal Server Error</h1>
    ///             <p>
    ///                 The server has encountered an internal error and can not process
    ///                 your request at this time. Please try again later or contact us
    ///                 at <pre>help@example.com</pre>.
    ///             </p>
    ///         </body>
    ///         </html>
    ///     "#;
    ///
    ///     Ok(Response::builder()
    ///         .status(StatusCode::INTERNAL_SERVER_ERROR)
    ///         .header("Content-Type", "text/html")
    ///         .body(Body::from(message))
    ///         .expect("couldn't build response"))
    /// }).make_service_by_cloning();
    ///
    /// let server = Server::bind(&"127.0.0.1:0".parse().unwrap())
    ///     .serve(service);
    ///
    /// tokio::run(server.map_err(|e| {
    ///     panic!("unexpected error: {}", e);
    /// }));
    /// ```
    fn catch_unwind<H, R>(self, handler: H) -> CatchUnwind<Self, R, H>
    where
        Self: Service<ResBody = Body, Error = BoxedError> + Sync,
        Self::Future: Send,
        H: Fn(Box<dyn Any + Send>) -> R + Send + Sync + 'static,
        R: IntoFuture<Item = Response<Body>, Error = BoxedError>,
        R::Future: Send + 'static;

    /// Creates a type implementing `MakeService` by cloning `self` for every
    /// incoming connection.
    ///
    /// The result of this can be directly passed to Hyper's `Builder::serve`.
    fn make_service_by_cloning(self) -> MakeServiceByCloning<Self>
    where
        Self: Clone;
}

impl<T: Service> ServiceExt for T {
    fn catch_unwind<H, R>(self, handler: H) -> CatchUnwind<Self, R, H>
    where
        Self: Service<ResBody = Body, Error = BoxedError> + Sync,
        Self::Future: Send,
        H: Fn(Box<dyn Any + Send>) -> R + Send + Sync + 'static,
        R: IntoFuture<Item = Response<Body>, Error = BoxedError>,
        R::Future: Send + 'static,
    {
        CatchUnwind {
            inner: self,
            handler: Arc::new(handler),
        }
    }

    fn make_service_by_cloning(self) -> MakeServiceByCloning<Self>
    where
        Self: Clone,
    {
        MakeServiceByCloning { service: self }
    }
}

/// A `Service` adapter that catches unwinding panics.
///
/// Returned by [`ServiceExt::catch_unwind`].
///
/// [`ServiceExt::catch_unwind`]: trait.ServiceExt.html#tymethod.catch_unwind
#[derive(Debug)]
pub struct CatchUnwind<S, R, H>
where
    S: Service<ResBody = Body, Error = BoxedError> + Sync,
    S::Future: Send + 'static,
    R: IntoFuture<Item = Response<Body>, Error = BoxedError>,
    R::Future: Send + 'static,
    H: Fn(Box<dyn Any + Send>) -> R + Send + Sync + 'static,
{
    inner: S,
    handler: Arc<H>,
}

impl<S, R, H> Service for CatchUnwind<S, R, H>
where
    S: Service<ResBody = Body, Error = BoxedError> + Sync,
    S::Future: Send + 'static,
    R: IntoFuture<Item = Response<Body>, Error = BoxedError>,
    R::Future: Send + 'static,
    H: Fn(Box<dyn Any + Send>) -> R + Send + Sync + 'static,
{
    type ReqBody = S::ReqBody;
    type ResBody = Body;
    type Error = BoxedError;
    type Future = DefaultFuture<Response<Body>, BoxedError>;

    fn call(&mut self, req: Request<Self::ReqBody>) -> Self::Future {
        // We need to make sure that we don't just catch panics that happen while *polling* the
        // inner service's `Future`, but also those that happen when the inner `Future`s are
        // constructed, which basically means anything happening inside `self.inner.call(..)`.

        let handler = self.handler.clone();
        let inner_future = match catch_unwind(AssertUnwindSafe(move || self.inner.call(req))) {
            Ok(future) => future,
            Err(panic_payload) => return Box::new(handler(panic_payload).into_future()),
        };

        Box::new(AssertUnwindSafe(inner_future)
            .catch_unwind()
            .then(move |panic_result| -> Box<dyn Future<Item=Response<Body>, Error = BoxedError>
            + Send> {
                match panic_result {
                    // FIXME avoid boxing so much here
                    Ok(result) => Box::new(result.into_future()),
                    Err(panic_payload) => Box::new(handler(panic_payload).into_future()),
                }
            }),
        )
    }
}

impl<S, R, H> Clone for CatchUnwind<S, R, H>
where
    S: Service<ResBody = Body, Error = BoxedError> + Clone + Sync,
    S::Future: Send + 'static,
    R: IntoFuture<Item = Response<Body>, Error = BoxedError>,
    R::Future: Send + 'static,
    H: Fn(Box<dyn Any + Send>) -> R + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        CatchUnwind {
            inner: self.inner.clone(),
            handler: self.handler.clone(),
        }
    }
}

/// Implements Hyper's `MakeService` trait by cloning a service `S` for every
/// incoming connection.
///
/// Both [`SyncService`] and [`AsyncService`] already implement `MakeService`
/// using the same implementation (cloning themselves), so you don't need this
/// if you are using either of those directly.
///
/// This type is returned by [`ServiceExt::make_service_by_cloning`].
///
/// [`SyncService`]: struct.SyncService.html
/// [`AsyncService`]: struct.AsyncService.html
/// [`ServiceExt::make_service_by_cloning`]: trait.ServiceExt.html#tymethod.make_service_by_cloning
#[derive(Debug, Copy, Clone)]
pub struct MakeServiceByCloning<S: Service + Clone> {
    service: S,
}

impl<Ctx, S: Service + Clone> MakeService<Ctx> for MakeServiceByCloning<S> {
    type ReqBody = S::ReqBody;
    type ResBody = S::ResBody;
    type Error = S::Error;
    type Service = S;
    type Future = FutureResult<S, Self::MakeError>;
    type MakeError = BoxedError;

    fn make_service(&mut self, _ctx: Ctx) -> Self::Future {
        Ok(self.service.clone()).into_future()
    }
}
