# `from-request`

[![crates.io](https://img.shields.io/crates/v/from-request.svg)](https://crates.io/crates/from-request)
[![docs.rs](https://docs.rs/from-request/badge.svg)](https://docs.rs/from-request/)
[![Build Status](https://travis-ci.org/1aim/from-request.svg?branch=master)](https://travis-ci.org/1aim/from-request)

This crate provides Rocket-style declarative HTTP request routing and guarding.
It's fully async (using hyper's support for futures 0.1) and works on stable
Rust.

You can declare the endpoints of your web application using attributes like
`#[post("/user/{id}/posts")]`, and this crate will generate code that dispatches
incoming requests depending on the method and path.

Please refer to the [changelog](CHANGELOG.md) to see what changed in the last
releases.

## Example

TODO
