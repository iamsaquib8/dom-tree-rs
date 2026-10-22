//! The macro's diagnostics (`tests/ui/*.rs` must fail to compile with exactly
//! the `.stderr` next to them) and code that must compile and run
//! (`tests/pass/*.rs`, e.g. a `#![no_std]` crate).
//!
//! The cases only cover errors the macro itself reports, whose text does not
//! change between compiler versions. After an intentional change to a
//! message, regenerate the `.stderr` files with
//! `TRYBUILD=overwrite cargo test -p dom-tree-derive` and review the diff.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
    t.pass("tests/pass/*.rs");
}
