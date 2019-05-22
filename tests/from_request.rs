use hyperdrive::{
    body::Json,
    http::{Method, Request, StatusCode},
    hyper::Body,
    BoxedError, Error, ErrorKind, FromRequest, Guard, NoContext, RequestContext,
};
use serde::Deserialize;

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

#[derive(Debug)]
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
    assert_eq!(error.allowed_methods(), &[&Method::POST]);

    let post_user = invoke::<Routes>(Request::post("/users/0").body(Body::empty()).unwrap());
    let error: Box<Error> = post_user.unwrap_err().downcast().unwrap();
    assert_eq!(error.kind(), ErrorKind::WrongMethod);
    assert_eq!(
        error.allowed_methods(),
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
    #[derive(FromRequest, Debug)]
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
    match route {
        Routes::Variant { ph, rest } => {
            assert_eq!(ph, 1234);
            assert_eq!(rest, "bla/bli");
        }
    }
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
    struct NoGuard;

    impl Guard for NoGuard {
        type Context = NoContext;
        type Result = Result<Self, BoxedError>;

        fn from_request(_request: &Request<()>, _context: &Self::Context) -> Self::Result {
            Ok(NoGuard)
        }
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
    let route: Routes<String, Pagination, Json<LoginData>, NoGuard> =
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
            guard: NoGuard,
        }
    );

    // Make sure the `SpecialContext` is turned into whatever context is needed by the fields, and
    // that we have the right where-clauses for it
    let route: Struct<String, Pagination, Json<LoginData>, NoGuard> =
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
            guard: NoGuard,
        }
    );

    // A guard that needs a `SpecialContext` must also work:
    let _route: Struct<String, Pagination, Json<LoginData>, SpecialGuard> =
        invoke_with(Request::get(url).body(body.into()).unwrap(), SpecialContext).unwrap();
}
