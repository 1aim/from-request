//! Regression test for https://github.com/1aim/hyperdrive/issues/17
//!
//! Previously, `Request` was used unqualified in the generated code when
//! `#[forward]` was used, but another variant is also present. This didn't blow
//! up before, since all tests are in `from_request.rs`, which imports
//! `http::Request`.

#![allow(unused)]

use hyperdrive::FromRequest;

#[derive(FromRequest)]
enum Outer {
    #[get("/outer")]
    Mine,

    Fallback {
        #[forward]
        inner: Inner,
    }
}

#[derive(FromRequest)]
#[get("/")]
struct Inner;
