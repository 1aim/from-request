# Changelog

## Unreleased

### Breaking Changes

* The signature of `from_request` was changed to pass the request differently.
* Support for dynamic extensions (a feature of the `http` crate) in requests
  was removed. `Request::extensions` will now return an empty map.

### New Features

* Add a `hyperdrive::blocking` helper function to simplify sync/async interop.
* Pass the request as a `&Arc<Request<_>>` ([#19]).

[#19]: https://github.com/1aim/hyperdrive/issues/19

## 0.1.1 - 2019-06-06

### Bug Fixes

* Fix an issue where `Request::new` was being used unqualified in the generated
  code ([#17]).

[#17]: https://github.com/1aim/hyperdrive/issues/17

## 0.1.0 - 2019-05-31

Initial release.
