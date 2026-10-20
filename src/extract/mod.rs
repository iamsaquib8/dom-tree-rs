//! Structured data out of a page: links and resources.
//!
//! Everything here is built on the public tree API ([`NodeRef`] traversal)
//! plus the shared, iterative [`walk`](crate::serialize::walk), so it inherits
//! the crate's guarantees: no panics, no recursion on document depth, and work
//! linear in the size of the document.

use alloc::string::String;

use crate::serialize::is_space;
use crate::tree::{Dom, NodeRef};

mod links;

pub use links::{Link, Resource};

/// Every node of the document in document (pre-order) order, following the
/// tree links (so it stays correct after edits, unlike arena order).
pub(crate) fn document_order(dom: &Dom) -> impl Iterator<Item = NodeRef<'_>> {
    dom.roots()
        .flat_map(|root| core::iter::once(root).chain(root.descendants()))
}

/// `text` with runs of whitespace collapsed to one space and trimmed.
pub(crate) fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for word in text.split(is_space).filter(|w| !w.is_empty()) {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

/// `Some(trimmed)` unless the value is empty after trimming.
pub(crate) fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim_matches(is_space);
    (!value.is_empty()).then_some(value)
}
