//! This example shows how to return a `500 Internal Server Error` response when
//! any part of the request panics.

use futures::Future;
use http::{Response, StatusCode};
use hyper::{Body, Server};
use hyperdrive::service::{ServiceExt, SyncService};
use hyperdrive::{BoxedError, FromBody, FromRequest, Guard, NoContext};
use std::sync::Arc;

#[derive(FromRequest)]
enum Route {
    /// Accessing this route will panic in a `Guard` implementation.
    #[get("/panic-guard")]
    PanicGuard { _guard: PanicGuard },

    /// Accessing this route will panic in a `FromBody` implementation.
    #[get("/panic-body")]
    PanicBody {
        #[body]
        _body: PanicBody,
    },

    /// Accessing this route will panic in the *request handler*.
    #[get("/panic-handler")]
    PanicHandler,
}

enum PanicGuard {}

impl Guard for PanicGuard {
    type Context = NoContext;
    type Result = Result<Self, BoxedError>;

    fn from_request(_request: &Arc<http::Request<()>>, _context: &Self::Context) -> Self::Result {
        panic!("panic inside PanicGuard");
    }
}

enum PanicBody {}

impl FromBody for PanicBody {
    type Context = NoContext;
    type Result = Result<Self, BoxedError>;

    fn from_body(
        _request: &Arc<http::Request<()>>,
        _body: hyper::Body,
        _context: &Self::Context,
    ) -> Self::Result {
        panic!("panic inside PanicBody");
    }
}

#[test]
fn main() {
    // Prepare a hyper server using Hyperdrive's `SyncService` adapter.
    // If you want to write an async handler, you could use `AsyncService` instead.
    let srv = Server::bind(&"127.0.0.1:0".parse().unwrap()).serve(
        SyncService::new(|route: Route, _| match route {
            Route::PanicGuard { .. } => unreachable!(),
            Route::PanicBody { .. } => unreachable!(),
            Route::PanicHandler => {
                panic!("panic inside the request handler");
            }
        })
        .catch_unwind(|_panic_payload| {
            Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header("Content-Type", "text/html")
                .body(Body::from(format!(
                    r#"
                    <!DOCTYPE html>
                    <html>
                    <body>
                        <h1>OOPSIE WOOPSIE!!</h1>
                        <p>
                            UwU we made a fucky wucky!! A wittle fucko boingo! The code monkeys at
                            our headquarters are working VEWY HAWD to fix this!
                        </p>
                    </body>
                    </html>
                "#
                )))
                .expect("couldn't build response"))
        })
        .make_service_by_cloning(),
    );

    let port = srv.local_addr().port();

    std::thread::spawn(move || {
        tokio::run(srv.map_err(|e| {
            panic!("unexpected error: {}", e);
        }))
    });

    let assert_500 = |route: &str| {
        let mut response = reqwest::Client::new()
            .get(&format!("http://127.0.0.1:{}/{}", port, route))
            .send()
            .expect("request failed");

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(
            response.text().unwrap().contains("UwU"),
            "route /{} did not send expected response",
            route
        );
    };

    assert_500("panic-handler");
    assert_500("panic-guard");
    assert_500("panic-body");
}
