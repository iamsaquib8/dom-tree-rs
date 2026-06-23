//! Serialization of the tree to JSON.
//!
//! The default [`json`](crate#features) feature provides a hand-rolled,
//! zero-dependency [`Dom::to_json`](crate::Dom::to_json). The optional `serde`
//! feature adds `serde::Serialize` impls that produce the **same** JSON shape
//! (pinned by a roundtrip test).

#[cfg(feature = "json")]
mod json;

#[cfg(feature = "serde")]
mod serde_impl;
