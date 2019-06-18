# Hyperdrive

[![crates.io](https://img.shields.io/crates/v/hyperdrive.svg)](https://crates.io/crates/hyperdrive)
[![docs.rs](https://docs.rs/hyperdrive/badge.svg)](https://docs.rs/hyperdrive/)
[![Build Status](https://travis-ci.org/1aim/hyperdrive.svg?branch=master)](https://travis-ci.org/1aim/hyperdrive)

This crate provides Rocket-style declarative HTTP request routing and guarding.
It can be used in both synchronous and fully async apps (using hyper's support
for futures 0.1) and works on stable Rust.

You can declare the endpoints of your web application using attributes like
`#[post("/user/{id}/posts")]`, and this crate will generate code that dispatches
incoming requests depending on the method and path.

Please refer to the [changelog](CHANGELOG.md) to see what changed in the last
releases.

## Example

This example shows how to use Hyperdrive to define routes for a simple
webservice and how to spin up a hyper server that will serve these routes with a
user-provided sync handler:

```rust
use hyperdrive::{FromRequest, body::Json, service::SyncService};
use hyper::{Server, Body};
use http::{Response, StatusCode};
use futures::prelude::*;
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
    let srv = Server::bind(&"127.0.0.1:0".parse().unwrap())
        .serve(SyncService::new(|route: Route, _| {
            match route {
                Route::Index => {
                    Response::new(Body::from("Hello World!"))
                }
                Route::User { id } => {
                    Response::new(Body::from(format!("User #{}", id)))
                }
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
            }
        }));

    let port = srv.local_addr().port();

    std::thread::spawn(move || tokio::run(srv.map_err(|e| {
        panic!("unexpected error: {}", e);
    })));

    // Let's make a login request to it
    let response = reqwest::Client::new()
        .post(&format!("http://127.0.0.1:{}/login", port))
        .body(r#"{ "email": "oof@example.com", "password": "hunter2" }"#)
        .send()
        .unwrap();

    // This login request should succeed
    assert_eq!(response.status(), StatusCode::OK);
}
```
