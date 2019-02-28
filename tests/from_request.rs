use from_request::{
    body::Json, futures::IntoFuture, http::Request, hyper::Body, BoxedError, FromRequest, Guard,
    NoContext, RequestContext,
};
use serde::Deserialize;
use tokio::runtime::current_thread::Runtime;

/// Simulates receiving `request`, and decodes a `FromRequest` implementor `T`.
///
/// `T` has to take a `NoContext`.
fn invoke<T>(
    request: Request<Body>,
) -> Result<<T::Result as IntoFuture>::Item, <T::Result as IntoFuture>::Error>
where
    T: FromRequest<Context = NoContext>,
{
    invoke_context::<T>(request, NoContext)
}

/// Simulates receiving `request`, and decodes a `FromRequest` implementor `T`.
///
/// Passes the given context object to `T`'s `FromRequest` implementation.
fn invoke_context<T: FromRequest>(
    request: Request<Body>,
    context: T::Context,
) -> Result<<T::Result as IntoFuture>::Item, <T::Result as IntoFuture>::Error> {
    let future = T::from_request(request, context).into_future();
    Runtime::new()
        .expect("couldn't create tokio runtime")
        .block_on(future)
}

struct MyGuard;

impl Guard for MyGuard {
    type Context = NoContext;

    type Result = Result<Self, BoxedError>;

    fn from_request(_request: &http::Request<()>, _context: &Self::Context) -> Self::Result {
        Ok(MyGuard)
    }
}

#[test]
fn test() {
    #[derive(FromRequest)]
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

    #[derive(Deserialize)]
    #[allow(dead_code)]
    struct LoginData {
        email: String,
        password: String,
    }

    #[derive(Deserialize)]
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

    invoke::<Routes>(
        Request::post("https://example.com/login")
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
}

/// Tests that `#[context]` can be used to change the context accepted by the
/// `FromRequest` impl. It should still be possible to use guards that take a
/// `NoContext` instead.
#[test]
fn context() {
    #[derive(FromRequest)]
    #[context(SpecialContext)]
    #[allow(dead_code)]
    enum Routes {
        #[get("/")]
        Variant {
            /// Takes a `SpecialContext`.
            special: SpecialGuard,
            /// Takes a `NoContext`.
            normal: MyGuard,
        },
    }

    #[derive(RequestContext)]
    #[allow(dead_code)]
    struct SpecialContext;

    struct SpecialGuard;

    impl Guard for SpecialGuard {
        type Context = SpecialContext;

        type Result = Result<Self, BoxedError>;

        fn from_request(_request: &http::Request<()>, _context: &Self::Context) -> Self::Result {
            Ok(SpecialGuard)
        }
    }
}

#[test]
fn any_placeholder() {
    #[derive(FromRequest)]
    enum Routes {
        #[get("/{ph}/{rest...}")]
        Variant {
            #[allow(unused)]
            ph: u32,
            #[allow(unused)]
            rest: String,
        },
    }
}

#[test]
fn placeholder_escape() {
    #[derive(FromRequest)]
    enum Routes {
        #[get("/\\{ph}/\\{rest...}")]
        Variant,
    }
}
