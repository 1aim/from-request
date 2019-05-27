use hyperdrive::{
    body::Json,
    http::{Method, Request, StatusCode},
    hyper::Body,
    BoxedError, Error, ErrorKind, FromRequest, Guard, NoContext, RequestContext,
};
use serde::Deserialize;
use std::str::FromStr;

/// Simulates receiving `request`, and decodes a `FromRequest` implementor `T`.
///
/// `T` has to take a `NoContext`.
fn invoke<T>(request: Request<Body>) -> Result<T, BoxedError>
where
    T: FromRequest<Context = NoContext>,
{
    T::from_request_sync(request, NoContext)
}

fn invoke_with<T>(request: Request<Body>, context: T::Context) -> Result<T, BoxedError>
where
    T: FromRequest,
{
    T::from_request_sync(request, context)
}

#[derive(Debug, PartialEq, Eq)]
struct MyGuard;

impl Guard for MyGuard {
    type Context = NoContext;

    type Result = Result<Self, BoxedError>;

    fn from_request(_request: &http::Request<()>, _context: &Self::Context) -> Self::Result {
        Ok(MyGuard)
    }
}

/// A few demo routes for user management (login, user info, user edit).
#[test]
fn user_app() {
    #[derive(FromRequest, Debug)]
    #[allow(dead_code)]
    enum Routes {
        #[post("/login")]
        Login {
            #[body]
            data: Json<LoginData>,
            #[query_params]
            params: (),

            gourd: MyGuard,
        },

        #[get("/users/{id}")]
        User { id: u32 },

        #[patch("/users/{id}")]
        PatchUser {
            id: u32,

            #[body]
            data: Json<PatchUser>,
        },
    }

    #[derive(Deserialize, Debug)]
    #[allow(dead_code)]
    struct LoginData {
        email: String,
        password: String,
    }

    #[derive(Deserialize, Debug)]
    #[serde(untagged)]
    #[allow(dead_code)]
    enum PatchUser {
        General {
            display_name: String,
        },
        ChangePassword {
            old_password: String,
            new_password: String,
        },
    }

    let login = invoke::<Routes>(
        Request::post("/login")
            .body(
                r#"
                {
                    "email": "test@example.com",
                    "password": "hunter2"
                }
                "#
                .into(),
            )
            .unwrap(),
    )
    .expect("/login not routed properly");
    match login {
        Routes::Login {
            params: (),
            gourd: MyGuard,
            data: Json(body),
        } => {
            assert_eq!(body.email, "test@example.com");
            assert_eq!(body.password, "hunter2");
        }
        _ => panic!("unexpected result: {:?}", login),
    }

    let get_login = invoke::<Routes>(Request::get("/login").body(Body::empty()).unwrap());
    let error: Box<Error> = get_login.unwrap_err().downcast().unwrap();
    assert_eq!(error.kind(), ErrorKind::WrongMethod);
    assert_eq!(
        error.allowed_methods().expect("allowed_methods()"),
        &[&Method::POST]
    );

    let post_user = invoke::<Routes>(Request::post("/users/0").body(Body::empty()).unwrap());
    let error: Box<Error> = post_user.unwrap_err().downcast().unwrap();
    assert_eq!(error.kind(), ErrorKind::WrongMethod);
    assert_eq!(
        error.allowed_methods().expect("allowed_methods()"),
        &[&Method::GET, &Method::PATCH, &Method::HEAD]
    );

    let user = invoke::<Routes>(Request::get("/users/wrong").body(Body::empty()).unwrap());
    let error: Box<Error> = user.unwrap_err().downcast().unwrap();
    assert_eq!(error.kind(), ErrorKind::PathSegment);
    assert_eq!(error.http_status(), StatusCode::NOT_FOUND);
}

/// Tests that `#[context]` can be used to change the context accepted by the
/// `FromRequest` impl. It should still be possible to use guards that take a
/// `NoContext` instead.
#[test]
fn context() {
    #[derive(FromRequest, Debug)]
    #[context(SpecialContext)]
    enum Routes {
        #[get("/")]
        Variant {
            /// Takes a `SpecialContext`.
            special: SpecialGuard,
            /// Takes a `NoContext`.
            normal: MyGuard,
        },
    }

    #[derive(RequestContext, Debug)]
    struct SpecialContext;

    #[derive(Debug)]
    struct SpecialGuard;

    impl Guard for SpecialGuard {
        type Context = SpecialContext;

        type Result = Result<Self, BoxedError>;

        fn from_request(_request: &http::Request<()>, _context: &Self::Context) -> Self::Result {
            Ok(SpecialGuard)
        }
    }

    invoke_with::<Routes>(
        Request::get("/").body(Body::empty()).unwrap(),
        SpecialContext,
    )
    .unwrap();
    invoke_with::<Routes>(
        Request::get("/bla").body(Body::empty()).unwrap(),
        SpecialContext,
    )
    .unwrap_err();
}

#[test]
fn struct_context() {
    #[derive(FromRequest, Debug)]
    #[context(SpecialContext)]
    #[get("/")]
    struct Route {
        /// Takes a `SpecialContext`.
        special: SpecialGuard,
        /// Takes a `NoContext`.
        normal: MyGuard,
    }

    #[derive(RequestContext, Debug)]
    struct SpecialContext;

    #[derive(Debug)]
    struct SpecialGuard;

    impl Guard for SpecialGuard {
        type Context = SpecialContext;

        type Result = Result<Self, BoxedError>;

        fn from_request(_request: &http::Request<()>, _context: &Self::Context) -> Self::Result {
            Ok(SpecialGuard)
        }
    }

    invoke_with::<Route>(
        Request::get("/").body(Body::empty()).unwrap(),
        SpecialContext,
    )
    .unwrap();
    invoke_with::<Route>(
        Request::get("/bla").body(Body::empty()).unwrap(),
        SpecialContext,
    )
    .unwrap_err();
}

#[test]
fn any_placeholder() {
    #[derive(FromRequest, Debug, PartialEq, Eq)]
    enum Routes {
        #[get("/{ph}/{rest...}")]
        Variant { ph: u32, rest: String },
    }

    let route = invoke::<Routes>(
        Request::get("/1234/bla/bli?param=123")
            .body(Body::empty())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        route,
        Routes::Variant {
            ph: 1234,
            rest: "bla/bli".to_string()
        }
    );

    let route = invoke::<Routes>(Request::get("/1234/").body(Body::empty()).unwrap()).unwrap();
    assert_eq!(
        route,
        Routes::Variant {
            ph: 1234,
            rest: "".to_string()
        }
    );

    invoke::<Routes>(Request::get("/1234").body(Body::empty()).unwrap()).unwrap_err();
}

#[test]
fn asterisk() {
    #[derive(FromRequest, Debug)]
    enum Routes {
        #[options("*")]
        ServerOptions,
    }

    invoke::<Routes>(Request::options("*").body(Body::empty()).unwrap()).unwrap();
    invoke::<Routes>(Request::options("/").body(Body::empty()).unwrap()).unwrap_err();
    invoke::<Routes>(Request::head("/").body(Body::empty()).unwrap()).unwrap_err();

    #[derive(FromRequest, Debug)]
    #[options("*")]
    struct Options;

    invoke::<Options>(Request::options("*").body(Body::empty()).unwrap()).unwrap();
    invoke::<Options>(Request::options("/").body(Body::empty()).unwrap()).unwrap_err();
    invoke::<Options>(Request::head("/").body(Body::empty()).unwrap()).unwrap_err();
}

#[test]
fn implicit_head_route() {
    #[derive(FromRequest, Debug, PartialEq, Eq)]
    enum Routes {
        #[get("/")]
        Index,

        #[get("/2/other")]
        Other,

        // We should still be able to define our own HEAD route instead
        #[head("/2/other")]
        OtherHead,
    }

    let head = invoke::<Routes>(Request::head("/").body(Body::empty()).unwrap()).unwrap();
    assert_eq!(head, Routes::Index);

    let anyhead = invoke::<Routes>(Request::head("/2/other").body(Body::empty()).unwrap()).unwrap();
    assert_eq!(anyhead, Routes::OtherHead);

    let anyhead = invoke::<Routes>(Request::get("/2/other").body(Body::empty()).unwrap()).unwrap();
    assert_eq!(anyhead, Routes::Other);
}

#[test]
fn query_params() {
    #[derive(FromRequest, PartialEq, Eq, Debug)]
    enum Routes {
        #[get("/users")]
        UserList {
            #[query_params]
            pagination: Pagination,
        },
    }

    #[derive(Deserialize, PartialEq, Eq, Debug)]
    struct Pagination {
        #[serde(default)]
        start: u32,
        #[serde(default = "default_count")]
        count: u32,
    }

    fn default_count() -> u32 {
        10
    }

    let route = invoke::<Routes>(Request::get("/users").body(Body::empty()).unwrap()).unwrap();
    assert_eq!(
        route,
        Routes::UserList {
            pagination: Pagination {
                start: 0,
                count: 10,
            }
        }
    );

    let route =
        invoke::<Routes>(Request::get("/users?count=30").body(Body::empty()).unwrap()).unwrap();
    assert_eq!(
        route,
        Routes::UserList {
            pagination: Pagination {
                start: 0,
                count: 30,
            }
        }
    );

    let route = invoke::<Routes>(
        Request::get("/users?start=543")
            .body(Body::empty())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        route,
        Routes::UserList {
            pagination: Pagination {
                start: 543,
                count: 10,
            }
        }
    );

    let route = invoke::<Routes>(
        Request::get("/users?start=123&count=30")
            .body(Body::empty())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        route,
        Routes::UserList {
            pagination: Pagination {
                start: 123,
                count: 30,
            }
        }
    );
}

/// Tests that the derive works on generic enums and structs.
#[test]
fn generic() {
    #[derive(FromRequest, Debug, PartialEq, Eq)]
    enum Routes<U, Q, B, G> {
        #[get("/{path}")]
        OmniRoute {
            path: U,

            #[query_params]
            qp: Q,

            #[body]
            body: B,

            guard: G,
        },
    }

    #[derive(RequestContext, Debug)]
    struct SpecialContext;

    #[derive(FromRequest, Debug, PartialEq, Eq)]
    #[get("/{path}")]
    #[context(SpecialContext)]
    struct Struct<U, Q, B, G> {
        path: U,

        #[query_params]
        qp: Q,

        #[body]
        body: B,

        guard: G,
    }

    #[derive(PartialEq, Eq, Debug)]
    struct SpecialGuard;

    impl Guard for SpecialGuard {
        type Context = SpecialContext;
        type Result = Result<Self, BoxedError>;

        fn from_request(_request: &Request<()>, _context: &Self::Context) -> Self::Result {
            Ok(SpecialGuard)
        }
    }

    #[derive(Deserialize, PartialEq, Eq, Debug)]
    struct Pagination {
        start: u32,
        count: u32,
    }

    #[derive(Deserialize, PartialEq, Eq, Debug)]
    struct LoginData {
        email: String,
        password: String,
    }

    let url = "/users?start=123&count=30";
    let body = r#"
        {
            "email": "test@example.com",
            "password": "hunter2"
        }
        "#;
    let route: Routes<String, Pagination, Json<LoginData>, MyGuard> =
        invoke(Request::get(url).body(body.into()).unwrap()).unwrap();

    assert_eq!(
        route,
        Routes::OmniRoute {
            path: "users".to_string(),
            qp: Pagination {
                start: 123,
                count: 30
            },
            body: Json(LoginData {
                email: "test@example.com".to_string(),
                password: "hunter2".to_string()
            }),
            guard: MyGuard,
        }
    );

    // Make sure the `SpecialContext` is turned into whatever context is needed by the fields, and
    // that we have the right where-clauses for it
    let route: Struct<String, Pagination, Json<LoginData>, MyGuard> =
        invoke_with(Request::get(url).body(body.into()).unwrap(), SpecialContext).unwrap();

    assert_eq!(
        route,
        Struct {
            path: "users".to_string(),
            qp: Pagination {
                start: 123,
                count: 30
            },
            body: Json(LoginData {
                email: "test@example.com".to_string(),
                password: "hunter2".to_string()
            }),
            guard: MyGuard,
        }
    );

    // A guard that needs a `SpecialContext` must also work:
    let _route: Struct<String, Pagination, Json<LoginData>, SpecialGuard> =
        invoke_with(Request::get(url).body(body.into()).unwrap(), SpecialContext).unwrap();
}

#[test]
fn forward() {
    #[derive(FromRequest, PartialEq, Eq, Debug)]
    enum Inner {
        #[get("/")]
        Index,

        #[get("/flabberghast")]
        Flabberghast,

        #[post("/")]
        Post,
    }

    #[derive(FromRequest, PartialEq, Eq, Debug)]
    #[get("/")] // FIXME: forbid this?
    struct Req {
        #[forward]
        _inner: Inner,
    }

    #[derive(FromRequest, PartialEq, Eq, Debug)]
    enum Enum {
        #[get("/")]
        First {
            #[forward]
            _inner: Inner,
        },

        Second {
            #[forward]
            _inner: Inner,
        },
    }

    invoke::<Req>(Request::get("/").body(Body::empty()).unwrap()).unwrap();

    let route = invoke::<Enum>(Request::get("/").body(Body::empty()).unwrap()).unwrap();
    assert_eq!(
        route,
        Enum::First {
            _inner: Inner::Index
        },
        "GET /"
    );

    let route = invoke::<Enum>(Request::head("/").body(Body::empty()).unwrap()).unwrap();
    assert_eq!(
        route,
        Enum::First {
            _inner: Inner::Index
        },
        "HEAD /"
    );

    let route = invoke::<Enum>(Request::get("/flabberghast").body(Body::empty()).unwrap()).unwrap();
    assert_eq!(
        route,
        Enum::Second {
            _inner: Inner::Flabberghast
        },
        "GET /flabberghast"
    );

    let route = invoke::<Enum>(Request::post("/").body(Body::empty()).unwrap()).unwrap();
    assert_eq!(
        route,
        Enum::Second {
            _inner: Inner::Post
        },
        "POST /"
    );
}

/// Tests that invalid methods return the right set of allowed methods, even in the presence of
/// `#[forward]`.
#[test]
fn forward_allowed_methods() {
    #[derive(FromRequest, PartialEq, Eq, Debug)]
    enum Inner {
        #[get("/")]
        #[post("/")]
        Index,

        #[get("/customhead")]
        GetCustomHead,

        #[head("/customhead")]
        HeadCustomHead,

        #[post("/post")]
        Post,

        #[post("/shared")]
        Shared,

        #[post("/shared/{s}")]
        Shared2 { s: u32 },
    }

    #[derive(FromRequest, PartialEq, Eq, Debug)]
    enum Wrapper {
        #[get("/shared")]
        Shared,

        #[get("/shared/{s}")]
        Shared2 { s: u8 },

        Fallback {
            #[forward]
            inner: Inner,
        },
    }

    #[derive(PartialEq, Eq, Debug)]
    struct AlwaysErr;

    impl FromStr for AlwaysErr {
        type Err = BoxedError;

        fn from_str(_: &str) -> Result<Self, BoxedError> {
            Err(String::new().into())
        }
    }

    let err: Box<Error> = invoke::<Wrapper>(Request::get("/post").body(Body::empty()).unwrap())
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err.kind(), ErrorKind::WrongMethod);
    assert_eq!(
        err.allowed_methods().expect("allowed_methods()"),
        &[&Method::POST]
    );

    let err: Box<Error> =
        invoke::<Wrapper>(Request::post("/customhead").body(Body::empty()).unwrap())
            .unwrap_err()
            .downcast()
            .unwrap();
    assert_eq!(err.kind(), ErrorKind::WrongMethod);
    assert_eq!(
        err.allowed_methods().expect("allowed_methods()"),
        &[&Method::GET, &Method::HEAD]
    );

    // `/shared` is defined in both. Outer takes precedence over inner, if it matches.

    let route = invoke::<Wrapper>(Request::post("/shared").body(Body::empty()).unwrap()).unwrap();
    assert_eq!(
        route,
        Wrapper::Fallback {
            inner: Inner::Shared
        }
    );

    let route = invoke::<Wrapper>(Request::get("/shared").body(Body::empty()).unwrap()).unwrap();
    assert_eq!(route, Wrapper::Shared);

    // Methods not accepted by either result in `allowed_methods()` being merged together.
    let err: Box<Error> = invoke::<Wrapper>(Request::put("/shared").body(Body::empty()).unwrap())
        .unwrap_err()
        .downcast()
        .unwrap();
    assert_eq!(err.kind(), ErrorKind::WrongMethod);
    assert_eq!(
        err.allowed_methods().expect("allowed_methods()"),
        &[&Method::GET, &Method::HEAD, &Method::POST]
    );

    // Also with FromStr segments
    let route =
        invoke::<Wrapper>(Request::post("/shared/123").body(Body::empty()).unwrap()).unwrap();
    assert_eq!(
        route,
        Wrapper::Fallback {
            inner: Inner::Shared2 { s: 123 }
        }
    );

    let route =
        invoke::<Wrapper>(Request::get("/shared/123").body(Body::empty()).unwrap()).unwrap();
    assert_eq!(route, Wrapper::Shared2 { s: 123 });
}

#[test]
fn generic_forward() {
    #[derive(FromRequest, Debug, PartialEq, Eq)]
    enum Generic<G, I> {
        #[get("/unused")]
        Unused,
        Fallback {
            guard: G,

            #[forward]
            inner: I,
        },
    }

    #[derive(FromRequest, Debug, PartialEq, Eq)]
    enum Inner {
        #[get("/")]
        Index,
    }

    let route: Generic<MyGuard, Inner> =
        invoke(Request::get("/").body(Body::empty()).unwrap()).unwrap();
    assert_eq!(
        route,
        Generic::Fallback {
            guard: MyGuard,
            inner: Inner::Index
        }
    );
}
