//! This module prototypes the output of the `FromRequest` custom derive.
//!
//! This should ideally be kept in sync with the actual derive output.

#![allow(dead_code)]

use crate::{
    body::{HtmlForm, Json},
    BoxedError, DefaultFuture, Error, ErrorKind, FromBody, FromRequest, Guard, NoContext,
};
use futures::{Future, IntoFuture};
use http::Method;
use lazy_static::lazy_static;
use regex::{Regex, RegexSet};
use serde::Deserialize;
use std::str::FromStr;

//#[derive(FromRequest)]
enum Routes {
    //#[get("/")]
    Index,

    //#[get("/users/:id")]
    User {
        id: u32,
        //#[query_params]
        params: UserParams,
        guard1: MyGuard,
        guard2: MyGuard,
    },

    //#[patch("/users/:id")]
    EditUser {
        id: u32,
        //#[body]
        data: HtmlForm<UserPatch>,
    },

    //#[post("/test")]
    Post {
        //#[body]
        body: Json<UserParams>,
        guard: MyGuard,
    },
}

#[derive(Deserialize)]
struct UserParams {
    /// `true`: Show user edit form (if we can edit this user)
    /// `false`: Show user info
    edit: bool,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum UserPatch {
    Email {
        email: String,
    },
    Password {
        old_password: String,
        new_password: String,
    },
}

struct MyGuard {}

impl Guard for MyGuard {
    type Context = NoContext;
    type Result = Result<Self, BoxedError>;

    fn from_request(_request: &http::Request<()>, _context: &Self::Context) -> Self::Result {
        Ok(MyGuard {})
    }
}

// Output of custom derive:
#[allow(unused_variables)]
impl FromRequest for Routes {
    type Context = NoContext;
    type Result = DefaultFuture<Self, BoxedError>;

    fn from_request(request: http::Request<hyper::Body>, context: Self::Context) -> Self::Result {
        // Step 0: `Variant` is `Routes` without the payload.
        enum Variant {
            Index,
            User,
            EditUser,
            Post,
        }

        // Step 1: Match against the generated regex set and inspect the HTTP
        // method in order to find the route that matches.
        lazy_static! {
            static ref ROUTES: RegexSet = RegexSet::new(&[
                "^/$",  // GET => Index
                "^/users/([^/]+)$", // GET => User, PATCH => EditUser
                "^/test$", // POST => Post
            ][..]).expect("internal error: invalid regex from FromRequest derive");

            static ref REGEXES: Vec<Option<Regex>> = {
                vec![
                    None,
                    Some(Regex::new("^/users/([^/]+)$").unwrap()),
                    None,
                ]
            };
        }

        let matches = ROUTES.matches(request.uri().path());
        assert!(
            matches.len() <= 1,
            "internal error: FromRequest derive produced overlapping regexes"
        );

        let method = request.method();
        let index = match matches.iter().next() {
            Some(index) => index,
            None => return Error::from_kind(ErrorKind::NoMatchingRoute).into_future(),
        };

        let variant = match (index, method) {
            // "/"
            (0, &Method::GET) => Variant::Index,
            (0, _) => return Error::wrong_method(&[&Method::GET]).into_future(),
            // "/users/:id"
            (1, &Method::GET) => Variant::User,
            (1, &Method::PATCH) => Variant::EditUser,
            (1, _) => return Error::wrong_method(&[&Method::GET, &Method::PATCH]).into_future(),
            // "/test"
            (2, &Method::POST) => Variant::Post,
            (2, _) => return Error::wrong_method(&[&Method::POST]).into_future(),
            _ => unimplemented!(),
        };

        // Now we have the route (`variant`) which tells us which fields we need to construct that
        // variant.
        // Step 2: For the variant in question, do the following:
        // * If it has any path segment placeholders:
        //   * Re-match the path with the specific regex for this route (must be a match)
        //   * Call `FromStr` on all captured segments
        // * If it has `query_params`
        //   * Deserialize from ?these&query=parameters
        // * For each guard (= field that isn't mentioned in any attribute)
        //   * Chain all calls to the `from_request` methods
        // * If it has a `body`
        //   * Chain the call to its `from_body` method

        match variant {
            Variant::Index => {
                //#[get("/")]
                let into_future = Ok(Routes::Index);
                Box::new(into_future.into_future()) as DefaultFuture<Self, BoxedError>
            }
            Variant::User => {
                //#[get("/users/:id", query_params = "<params>")]
                // Guards: guard1, guard2

                let caps = REGEXES[index]
                    .as_ref()
                    .expect("internal error: no regex for route with placeholders")
                    .captures(request.uri().path())
                    .expect("internal error: regex first matched but now didn't?");
                // Extract captures starting at 1 (0 = entire match)
                let id_str = caps
                    .get(1)
                    .expect("internal error: capture group did not match anything")
                    .as_str();
                // Parse captured path segments
                let fld_id = match <u32 as FromStr>::from_str(id_str) {
                    Ok(v) => v,
                    Err(e) => return Error::with_source(ErrorKind::PathSegment, e).into_future(),
                };

                // Parse query params
                let raw_query = request.uri().query().unwrap_or("");
                let fld_params = match serde_urlencoded::from_str::<UserParams>(raw_query) {
                    Ok(val) => val,
                    Err(e) => return Error::with_source(ErrorKind::QueryParam, e).into_future(),
                };

                // Prepare for guard/body/async stuff
                let (parts, body) = request.into_parts();
                let headers = http::Request::from_parts(parts, ());

                // guard1
                let future = MyGuard::from_request(&headers, &context)
                    .into_future()
                    .and_then(move |fld_guard1| {
                        // guard2
                        MyGuard::from_request(&headers, &context)
                            .into_future()
                            .and_then(move |fld_guard2| {
                                // No body, so we're done, assemble type
                                Ok(Routes::User {
                                    id: fld_id,
                                    params: fld_params,
                                    guard1: fld_guard1,
                                    guard2: fld_guard2,
                                })
                                .into_future()
                            })
                    });

                Box::new(future)
            }
            Variant::EditUser => {
                //#[patch("/users/:id", body = "<data>")]
                // no guards

                let caps = REGEXES[index]
                    .as_ref()
                    .expect("internal error: no regex for route with placeholders")
                    .captures(request.uri().path())
                    .expect("internal error: regex first matched but now didn't?");

                // Extract captures starting at 1 (0 = entire match)
                let id_str = caps
                    .get(1)
                    .expect("internal error: capture group did not match anything")
                    .as_str();
                // Parse captured path segments
                let fld_id = match <u32 as FromStr>::from_str(id_str) {
                    Ok(v) => v,
                    Err(e) => return Error::with_source(ErrorKind::PathSegment, e).into_future(),
                };

                // No query params

                // Prepare for guard/body/async stuff
                let (parts, body) = request.into_parts();
                let headers = http::Request::from_parts(parts, ());

                // body
                let future = <HtmlForm<UserPatch> as FromBody>::from_body(&headers, body, &context)
                    .and_then(move |fld_data| {
                        Ok(Routes::EditUser {
                            id: fld_id,
                            data: fld_data,
                        })
                    });

                Box::new(future)
            }
            Variant::Post => {
                //#[post("/test", body = "<body>")]
                // Guards: guard

                // No captures

                // No query params

                // Prepare for guard/body/async stuff
                let (parts, body) = request.into_parts();
                let headers = http::Request::from_parts(parts, ());

                let future = <MyGuard as Guard>::from_request(
                    &headers,
                    <Self::Context as AsRef<_>>::as_ref(&context),
                )
                .into_future()
                .and_then(move |fld_guard| {
                    // body
                    <Json<UserParams> as FromBody>::from_body(&headers, body, &context)
                        .into_future()
                        .and_then(move |fld_body| {
                            // assemble result
                            Ok(Routes::Post {
                                body: fld_body,
                                guard: fld_guard,
                            })
                        })
                });

                Box::new(future)
            }
        }
    }
}
