//! Serialization of the tree.
//!
//! - [`Dom::to_html`](crate::Dom::to_html) / [`NodeRef::to_html`](crate::NodeRef::to_html)
//!   re-serialize the tree back to HTML — always available, zero-dependency.
//! - The default [`json`](crate#features) feature adds the hand-rolled,
//!   zero-dependency [`Dom::to_json`](crate::Dom::to_json).
//! - The optional `serde` feature adds `serde::Serialize` impls that produce the
//!   **same** JSON shape (pinned by a roundtrip test).

mod html;

#[cfg(feature = "json")]
mod json;

#[cfg(feature = "serde")]
mod serde_impl;
