use http::Request;
use hyperdrive::*;
use std::sync::Arc;

struct MyGuard;

impl Guard for MyGuard {
    type Context = NoContext;

    type Result = Result<Self, BoxedError>;

    fn from_request(_request: &Arc<Request<()>>, _context: &Self::Context) -> Self::Result {
        Ok(MyGuard)
    }
}

/// This used to ICE when NLL is enabled, but now there is a workaround in place that generates
/// trait bounds differently.
///
/// Upstream issues:
/// * https://github.com/rust-lang/rust/issues/61311
/// * https://github.com/rust-lang/rust/issues/61320
#[derive(FromRequest)]
struct Generic<I> {
    _guard: MyGuard,

    #[forward]
    _inner: I,
}
