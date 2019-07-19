# Changelog

## Unreleased

### Breaking Changes

* The signature of `from_request` was changed to pass the request differently.

### New Features

* Add a `hyperdrive::blocking` helper function to simplify sync/async interop.
* Pass the request as a `&Arc<Request<_>>` ([#19]).
* The original request is now passed to the service closure when using
  `SyncService` or `AsyncService` ([#20]).
* Add a `ServiceExt` trait, which offers a convenient way of catching panics in
  the App ([#25](https://github.com/dac-gmbh/hyperdrive/pull/25)).

### Other Changes

* Hyperdrive is now licensed under the [0BSD] license

[#19]: https://github.com/dac-gmbh/hyperdrive/issues/19
[#20]: https://github.com/dac-gmbh/hyperdrive/issues/20
[0BSD]: https://github.com/dac-gmbh/hyperdrive/blob/master/LICENSE

## 0.1.1 - 2019-06-06

### Bug Fixes

* Fix an issue where `Request::new` was being used unqualified in the generated
  code ([#17]).

[#17]: https://github.com/dac-gmbh/hyperdrive/issues/17

## 0.1.0 - 2019-05-31

Initial release.
