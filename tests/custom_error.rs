use hyper::{Body, Request};
use hyperdrive::{body::Json, FromBody, FromRequest, FromRequestError, Guard, NoContext};
use serde::Deserialize;
use std::{fmt, sync::Arc};
// 1. test explicitly using default custom error (`NoCustomError`)
// 2. test with different custom error
//    - and Guard's (with different custom error)
//    - and #[body] (with different custom error)
//    - and #[fallback] (with different custom error)

fn invoke_custom_err<T>(request: Result<Request<Body>, hyper::http::Error>) -> T::Error
where
    T: FromRequest<Context = NoContext> + fmt::Debug,
{
    let request = request.unwrap();
    match T::from_request_sync(request, NoContext) {
        Ok(route) => panic!(
            "unexpectedly route creation did not fail, route: {:?}",
            route
        ),
        Err(FromRequestError::BuildIn(err)) => panic!("unexpected build in error: {:?}", err),
        Err(FromRequestError::Custom(err)) => err,
    }
}

#[derive(Debug)]
enum MyOuterError {
    Inner(MyInnerError),
    // Errors in guards/bodies do not need to implement `StdError` etc.
    StringError(String),
    Hyper(hyper::Error),
}

impl std::error::Error for MyOuterError {}
impl fmt::Display for MyOuterError {
    fn fmt(&self, fter: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, fter)
    }
}

impl From<hyper::Error> for MyOuterError {
    fn from(e: hyper::Error) -> Self {
        MyOuterError::Hyper(e)
    }
}

impl From<String> for MyOuterError {
    fn from(e: String) -> Self {
        MyOuterError::StringError(e)
    }
}

impl From<&'static str> for MyOuterError {
    fn from(e: &'static str) -> Self {
        MyOuterError::StringError(e.into())
    }
}

impl From<MyInnerError> for MyOuterError {
    fn from(e: MyInnerError) -> Self {
        MyOuterError::Inner(e)
    }
}

#[derive(Debug)]
enum MyInnerError {
    IntError(u32),
    Hyper(hyper::Error),
}

impl std::error::Error for MyInnerError {}
impl fmt::Display for MyInnerError {
    fn fmt(&self, fter: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self, fter)
    }
}

impl From<hyper::Error> for MyInnerError {
    fn from(e: hyper::Error) -> Self {
        MyInnerError::Hyper(e)
    }
}

impl From<u32> for MyInnerError {
    fn from(e: u32) -> Self {
        MyInnerError::IntError(e)
    }
}

#[derive(Debug)]
struct FailOuter;

impl Guard for FailOuter {
    type Context = NoContext;
    type Error = String;
    type Result = Result<Self, Self::Error>;

    fn from_request(_: &Arc<http::Request<()>>, _: &Self::Context) -> Self::Result {
        Err("failed outer".to_owned())
    }
}

#[derive(Debug)]
struct FailInner;

impl Guard for FailInner {
    type Context = NoContext;
    type Error = u32;
    type Result = Result<Self, Self::Error>;

    fn from_request(_: &Arc<http::Request<()>>, _: &Self::Context) -> Self::Result {
        Err(12)
    }
}

#[derive(Debug)]
struct FailBody;

impl FromBody for FailBody {
    type Context = NoContext;
    type Error = &'static str;
    type Result = Result<Self, FromRequestError<Self::Error>>;

    fn from_body(
        _request: &Arc<http::Request<()>>,
        _body: hyper::Body,
        _context: &Self::Context,
    ) -> Self::Result {
        Err("failed body".into())
    }
}

#[derive(Deserialize, Debug)]
struct Data {
    name: String,
}

#[test]
fn use_custom_errors() {
    #[derive(FromRequest, Debug)]
    #[error(MyOuterError)]
    enum Route {
        #[get("/a")]
        A {
            g: FailOuter,
            #[body]
            b: Json<Data>,
        },

        #[get("/b")]
        B {
            #[body]
            b: FailBody,
        },

        Fallback {
            #[forward]
            f: Inner,
        },
    }

    #[derive(FromRequest, Debug)]
    #[error(MyInnerError)]
    enum Inner {
        #[get("/c")]
        Fun { f: FailInner },
    }

    let err = invoke_custom_err::<Route>(Request::get("/a").body(Body::empty()));
    if let MyOuterError::StringError(err) = err {
        assert_eq!(err, "failed outer");
    } else {
        panic!("unexpected error {:?}", err);
    }

    let err = invoke_custom_err::<Route>(Request::get("/b").body(Body::empty()));
    if let MyOuterError::StringError(err) = err {
        assert_eq!(err, "failed body");
    } else {
        panic!("unexpected error {:?}", err);
    }

    let err = invoke_custom_err::<Route>(Request::get("/c").body(Body::empty()));
    if let MyOuterError::Inner(MyInnerError::IntError(err)) = err {
        assert_eq!(err, 12);
    } else {
        panic!("unexpected error {:?}", err);
    }
}

#[test]
fn generic_use_custom_errors() {
    #[derive(FromRequest, Debug)]
    #[error(MyOuterError)]
    enum Route<FO, FI, BO> {
        #[get("/a")]
        A {
            g: FO,
            #[body]
            b: Json<Data>,
        },

        #[get("/b")]
        B {
            #[body]
            b: BO,
        },

        Fallback {
            #[forward]
            f: Inner<FI>,
        },
    }

    #[derive(FromRequest, Debug)]
    #[error(MyInnerError)]
    enum Inner<FI> {
        #[get("/c")]
        Fun { f: FI },
    }

    let err = invoke_custom_err::<Route<FailOuter, FailInner, FailBody>>(
        Request::get("/a").body(Body::empty()),
    );
    if let MyOuterError::StringError(err) = err {
        assert_eq!(err, "failed outer");
    } else {
        panic!("unexpected error {:?}", err);
    }

    let err = invoke_custom_err::<Route<FailOuter, FailInner, FailBody>>(
        Request::get("/b").body(Body::empty()),
    );
    if let MyOuterError::StringError(err) = err {
        assert_eq!(err, "failed body");
    } else {
        panic!("unexpected error {:?}", err);
    }

    let err = invoke_custom_err::<Route<FailOuter, FailInner, FailBody>>(
        Request::get("/c").body(Body::empty()),
    );
    if let MyOuterError::Inner(MyInnerError::IntError(err)) = err {
        assert_eq!(err, 12);
    } else {
        panic!("unexpected error {:?}", err);
    }
}
