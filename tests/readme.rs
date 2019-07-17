use futures::prelude::*;
use http::{Response, StatusCode};
use hyper::{Body, Server};
use hyperdrive::{body::Json, service::SyncService, FromRequest};
use serde::Deserialize;

#[derive(FromRequest)]
enum Route {
    #[get("/")]
    Index,

    #[get("/users/{id}")]
    User {
        /// Taken from request path
        id: u32,
    },

    #[post("/login")]
    Login {
        #[body]
        data: Json<Login>,
    },
}

#[derive(Deserialize)]
struct Login {
    email: String,
    password: String,
}

fn main() {
    // Prepare a hyper server using Hyperdrive's `SyncService` adapter.
    // If you want to write an async handler, you could use `AsyncService` instead.
    let srv =
        Server::bind(&"127.0.0.1:0".parse().unwrap()).serve(SyncService::new(|route: Route, _| {
            let response = match route {
                Route::Index => Response::new(Body::from("Hello World!")),
                Route::User { id } => Response::new(Body::from(format!("User #{}", id))),
                Route::Login { data } => {
                    if data.password == "hunter2" {
                        Response::new(Body::from(format!("Welcome, {}!", data.email)))
                    } else {
                        Response::builder()
                            .status(StatusCode::UNAUTHORIZED)
                            .body(Body::from("Invalid username or password"))
                            .expect("building response failed")
                    }
                }
            };

            Ok(response)
        }));

    let port = srv.local_addr().port();

    std::thread::spawn(move || {
        tokio::run(srv.map_err(|e| {
            panic!("unexpected error: {}", e);
        }))
    });

    // Let's make a login request to it
    let response = reqwest::Client::new()
        .post(&format!("http://127.0.0.1:{}/login", port))
        .body(r#"{ "email": "oof@example.com", "password": "hunter2" }"#)
        .send()
        .unwrap();

    // This login request should succeed
    assert_eq!(response.status(), StatusCode::OK);
}
