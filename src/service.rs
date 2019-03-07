//! Implements hyper `Service` adapters that reduce boilerplate.
//!
//! This module contains two implementations of hyper's `Service` trait:
//! [`AsyncService`] and [`SyncService`]. Both will decode the request for you
//! and invoke a handler closure.
//!
//! If your app doesn't need to be asynchronous, you can use [`SyncService`],
//! which is an adapter that lets you write blocking and synchronous code that
//! is run in a separate thread pool.
//!
//! [`AsyncService`]: struct.AsyncService.html
//! [`SyncService`]: struct.SyncService.html

use crate::{BoxedError, DefaultFuture, Error, FromRequest, NoContext};
use futures::{future::FutureResult, Future, IntoFuture};
use hyper::{
    service::{MakeService, Service},
    Body, Method, Request, Response,
};
use std::fmt;
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
/// * Turning any [`from_request::Error`] into a proper HTTP response.
///
/// This type stores an async request handler `H` and the context needed by the
/// [`FromRequest`] implementation. The context is cloned for every request.
///
/// ## Type Parameters
///
/// * **`H`**: The handler closure. Takes a [`FromRequest`] implementor `R` and
///   returns a future resolving to the response to return to the client. Shared
///   via `Arc`.
/// * **`R`**: The request type expected by the handler `H`. Implements
///   `FromRequest`.
/// * **`F`**: The `Future` returned by the handler closure `H`.
///
/// [`FromRequest`]: ../trait.FromRequest.html
/// [`from_request::Error`]: ../struct.Error.html
pub struct AsyncService<H, R, F>
where
    H: Fn(R) -> F + Send + Sync + 'static,
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
    H: Fn(R) -> F + Send + Sync + 'static,
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
    H: Fn(R) -> F + Send + Sync + 'static,
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
    H: Fn(R) -> F + Send + Sync + 'static,
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
    H: Fn(R) -> F + Send + Sync + 'static,
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
    H: Fn(R) -> F + Send + Sync + 'static,
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
        let fut = R::from_request(req, self.context.clone())
            .and_then(move |r| handler(r))
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
    H: Fn(R) -> F + Send + Sync + 'static,
    R: FromRequest,
    R::Context: Clone + fmt::Debug,
    R::Future: 'static,
    F: Future<Item = Response<Body>, Error = BoxedError> + Send + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
/// Just like `AsyncService`, using this type takes a bit of boilerplate away
/// from your app. Specifically, it handles:
///
/// * Suppressing the body of the response when the request used `HEAD`.
/// * Turning any [`from_request::Error`] into a proper HTTP response.
///
/// [`from_request::Error`]: ../struct.Error.html
///
/// This is effectively a bridge between async hyper and a synchronous,
/// blocking app. Writing sync code is much simpler than writing async code
/// (even with async/await syntax), so depending on your app this might be a
/// good tradeoff.
pub struct SyncService<H, R>
where
    H: Fn(R) -> Response<Body> + Send + Sync + 'static,
    R: FromRequest + Send + 'static,
    R::Context: Clone,
{
    handler: Arc<H>,
    context: R::Context,
}

impl<H, R> SyncService<H, R>
where
    H: Fn(R) -> Response<Body> + Send + Sync + 'static,
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
    H: Fn(R) -> Response<Body> + Send + Sync + 'static,
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
    H: Fn(R) -> Response<Body> + Send + Sync + 'static,
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
    H: Fn(R) -> Response<Body> + Send + Sync + 'static,
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
    H: Fn(R) -> Response<Body> + Send + Sync + 'static,
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
        let fut = R::from_request(req, self.context.clone())
            .and_then(move |req| {
                // Run the handler on the blocking thread pool. The `blocking` call might fail and
                // is retried when the pool is currently full, so we do a little `Option` dance.
                let mut req = Some(req);
                futures::future::poll_fn(move || {
                    tokio_threadpool::blocking(|| handler(req.take().unwrap()))
                })
                .map_err(|e| BoxedError::from(e))
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
    H: Fn(R) -> Response<Body> + Send + Sync + 'static,
    R: FromRequest + Send + 'static,
    R::Context: Clone + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
