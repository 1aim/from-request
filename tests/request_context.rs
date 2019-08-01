//! Contains simple successful `#[derive(RequestContext)]` tests.
//!
//! Negative tests are in the derive implementation.

use hyperdrive::RequestContext;

fn assert_impls<T: RequestContext>() {}

#[test]
fn unit() {
    #[derive(RequestContext)]
    struct Unit;

    assert_impls::<Unit>();
}

#[test]
fn empty() {
    #[derive(RequestContext)]
    struct Empty {}

    assert_impls::<Empty>();
}

#[test]
fn simple() {
    #[derive(RequestContext)]
    struct Simple {
        _field: u8,
    }

    assert_impls::<Simple>();
}

#[test]
fn as_ref() {
    #[derive(RequestContext)]
    struct Refs {
        #[as_ref]
        _field: u8,
    }

    assert_impls::<Refs>();

    // Additional impl added:
    let _ = <Refs as AsRef<u8>>::as_ref;
}

#[test]
fn as_ref_tuple() {
    #[derive(RequestContext)]
    struct Refs(#[as_ref] u16);

    assert_impls::<Refs>();

    // Additional impl added:
    let _ = <Refs as AsRef<u16>>::as_ref;
}

#[test]
fn on_enum() {
    #[derive(RequestContext)]
    enum Ctx {}
    assert_impls::<Ctx>();

    #[derive(RequestContext)]
    #[allow(unused)]
    enum Ctx2 {
        Variant,
        Variant2 { inner: u32 },
    }
    assert_impls::<Ctx2>();
}
