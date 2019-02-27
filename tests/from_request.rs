use from_request::{body::Json, BoxedError, FromRequest, Guard, NoContext, RequestContext};
use serde::Deserialize;

fn assert_impls<T: FromRequest>() {}

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
    enum Request {
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

    assert_impls::<Request>();
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

/// Overlapping paths are accepted and the variant that was declared first is
/// preferred.
#[test]
fn overlap() {
    #[derive(FromRequest)]
    enum Routes {
        #[get("/0")]
        Var {},

        #[get("/{ph}")]
        Variant {
            #[allow(unused)]
            ph: u32,
        },
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
