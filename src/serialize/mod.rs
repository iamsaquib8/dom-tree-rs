//! Serialization of the tree.
//!
//! - [`Dom::to_html`](crate::Dom::to_html) / [`NodeRef::to_html`](crate::NodeRef::to_html)
//!   (plus [`inner_html`](crate::NodeRef::inner_html) /
//!   [`outer_html`](crate::NodeRef::outer_html)) re-serialize the tree back to
//!   HTML — always available, zero-dependency.
//! - [`Dom::to_text`](crate::Dom::to_text) / [`NodeRef::to_text`](crate::NodeRef::to_text)
//!   produce readable plain text — always available, zero-dependency.
//! - The default [`markdown`](crate#features) feature adds the zero-dependency
//!   `Dom::to_markdown` / `NodeRef::to_markdown` (CommonMark + GFM tables),
//!   and `to_markdown_chunks` (`chunk.rs`), which splits that Markdown into
//!   heading-aware chunks for retrieval.
//! - The default [`json`](crate#features) feature adds the hand-rolled,
//!   zero-dependency [`Dom::to_json`](crate::Dom::to_json).
//! - The optional `serde` feature adds `serde::Serialize` impls that produce the
//!   **same** JSON shape (pinned by a roundtrip test).
//!
//! Every serializer is iterative (an explicit stack, or the link-following
//! `walk` below), so arbitrarily deep trees cannot overflow the call stack.

use alloc::string::String;

use crate::tree::NodeRef;

mod html;
mod text;

#[cfg(feature = "markdown")]
mod markdown;

#[cfg(feature = "markdown")]
mod chunk;

#[cfg(feature = "markdown")]
pub use chunk::Chunk;

#[cfg(feature = "json")]
mod json;

#[cfg(feature = "serde")]
mod serde_impl;

/// Visit `root`'s subtree in document order without recursion, using the
/// tree's parent/sibling links. `f(node, true)` is called on entry and returns
/// whether to descend into the node's children; `f(node, false)` is called on
/// exit, only for nodes that were descended into.
pub(crate) fn walk<'a>(root: NodeRef<'a>, mut f: impl FnMut(NodeRef<'a>, bool) -> bool) {
    let mut cur = root;
    loop {
        if f(cur, true) {
            if let Some(child) = cur.first_child() {
                cur = child;
                continue;
            }
            f(cur, false);
        }
        // `cur` is done: move to its next sibling, closing parents on the way up.
        loop {
            if cur.id() == root.id() {
                return;
            }
            if let Some(next) = cur.next_sibling() {
                cur = next;
                break;
            }
            match cur.parent() {
                Some(parent) => {
                    cur = parent;
                    f(cur, false);
                }
                None => return,
            }
        }
    }
}

/// Whitespace that collapses in readable output: HTML's ASCII whitespace plus
/// the no-break space, which reads the same as a space.
pub(crate) fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0C' | '\u{a0}')
}

/// Drop the one newline that HTML ignores right after a `<pre>` start tag.
pub(crate) fn skip_newline(text: &str) -> &str {
    text.strip_prefix("\r\n")
        .or_else(|| text.strip_prefix(['\n', '\r']))
        .unwrap_or(text)
}

/// Append preformatted text, normalizing `\r\n` and lone `\r` to `\n`.
pub(crate) fn push_verbatim(out: &mut String, text: &str) {
    let mut cr = false;
    for c in text.chars() {
        if c == '\n' && cr {
            cr = false;
            continue;
        }
        cr = c == '\r';
        out.push(if cr { '\n' } else { c });
    }
}
