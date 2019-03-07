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

TODO
