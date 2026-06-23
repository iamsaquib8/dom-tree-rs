//! The tree builder: turns a token stream into the [`Dom`] arena.
//!
//! Parsing is infallible. The builder keeps an "open elements" stack and
//! recovers from the common real-world mistakes — omitted end tags, mis-nested
//! tags, stray end tags — recording each as a [`ParseError`] rather than
//! bailing out. See the crate docs for the exact recovery rules we implement.

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::entities;
use crate::error::{ErrorKind, ParseError};
use crate::node::{Element, NodeId, NodeKind};
use crate::tags;
use crate::token::Token;
use crate::tokenizer::Tokenizer;
use crate::tree::Dom;

/// Parse `src` into a best-effort [`Dom`]. Never panics; diagnostics land in
/// [`Dom::errors`].
pub(crate) fn parse_document(src: &str) -> Dom {
    let mut tk = Tokenizer::new(src);
    let mut dom = Dom::with_capacity(src.len() / 8 + 4);
    // The open-elements stack holds arena ids; names are resolved from the arena
    // on demand, so we never allocate a separate per-element name string.
    let mut open: Vec<NodeId> = Vec::new();

    while let Some((tok, pos)) = tk.next_token() {
        match tok {
            Token::StartTag {
                name,
                attrs,
                self_closing,
            } => {
                let lname = lower(name);

                // Implicit close: pop open elements that this start tag closes.
                while let Some(&top) = open.last() {
                    let should_close = tags::implicitly_closes(&lname, open_name(&dom, top));
                    if should_close {
                        open.pop();
                        dom.errors
                            .push(ParseError::new(pos, ErrorKind::ImplicitlyClosed));
                    } else {
                        break;
                    }
                }

                let parent = open.last().copied();

                let mut built: Vec<(Box<str>, Box<str>)> = Vec::with_capacity(attrs.len());
                for (k, v) in attrs {
                    let key = box_lower(k);
                    let (decoded, unknown) = entities::decode(v.as_str());
                    if unknown {
                        dom.errors.push(ParseError::new(pos, ErrorKind::UnknownEntity));
                    }
                    built.push((key, into_box(decoded)));
                }

                let element = Element {
                    name: Box::from(lname.as_ref()),
                    attrs: built,
                };
                let id = dom.alloc(NodeKind::Element(element));
                dom.append(parent, id);

                if !self_closing && !tags::is_void(&lname) {
                    open.push(id);
                }
            }

            Token::EndTag { name } => {
                let lname = lower(name);
                if tags::is_void(&lname) {
                    continue; // void elements never have end tags
                }
                match open.iter().rposition(|&id| open_name(&dom, id) == lname.as_ref()) {
                    Some(idx) => {
                        if idx != open.len() - 1 {
                            dom.errors
                                .push(ParseError::new(pos, ErrorKind::MismatchedEndTag));
                        }
                        open.truncate(idx);
                    }
                    None => dom.errors.push(ParseError::new(pos, ErrorKind::StrayEndTag)),
                }
            }

            Token::Text(t) => {
                if t.is_empty() {
                    continue;
                }
                let parent = open.last().copied();
                let text: Box<str> = if parent_is_raw(&dom, &open) {
                    Box::from(t)
                } else {
                    let (decoded, unknown) = entities::decode(t);
                    if unknown {
                        dom.errors.push(ParseError::new(pos, ErrorKind::UnknownEntity));
                    }
                    into_box(decoded)
                };
                let id = dom.alloc(NodeKind::Text(text));
                dom.append(parent, id);
            }

            Token::Cdata(c) => {
                if c.is_empty() {
                    continue;
                }
                let parent = open.last().copied();
                let id = dom.alloc(NodeKind::Text(Box::from(c)));
                dom.append(parent, id);
            }

            Token::Comment(c) => {
                let parent = open.last().copied();
                let id = dom.alloc(NodeKind::Comment(Box::from(c)));
                dom.append(parent, id);
            }

            Token::Doctype(d) => {
                let parent = open.last().copied();
                let id = dom.alloc(NodeKind::Doctype(Box::from(d.trim())));
                dom.append(parent, id);
            }
        }
    }

    if !open.is_empty() {
        dom.errors
            .push(ParseError::new(src.len(), ErrorKind::ImplicitlyClosed));
    }

    // Fold in the tokenizer's lexical diagnostics, then order by position.
    dom.errors.append(&mut tk.errors);
    dom.errors.sort_by_key(|e| e.position);
    dom
}

/// The lower-cased tag name of an open element, looked up in the arena.
fn open_name(dom: &Dom, id: NodeId) -> &str {
    match dom.node(id).map(|n| &n.kind) {
        Some(NodeKind::Element(e)) => e.name(),
        _ => "",
    }
}

/// Whether the innermost open element keeps its text raw (no entity decoding).
fn parent_is_raw(dom: &Dom, open: &[NodeId]) -> bool {
    open.last()
        .is_some_and(|&id| tags::is_raw_text(open_name(dom, id)))
}

/// Lower-case `name`, but borrow it unchanged when it's already lower-case
/// (the common case) — avoiding an allocation + copy per tag/end-tag.
fn lower(name: &str) -> Cow<'_, str> {
    if name.bytes().any(|b| b.is_ascii_uppercase()) {
        Cow::Owned(name.to_ascii_lowercase())
    } else {
        Cow::Borrowed(name)
    }
}

/// Box a lower-cased copy of `name` in a single allocation, skipping the
/// lower-casing work when it's already lower-case.
fn box_lower(name: &str) -> Box<str> {
    if name.bytes().any(|b| b.is_ascii_uppercase()) {
        name.to_ascii_lowercase().into_boxed_str()
    } else {
        Box::from(name)
    }
}

/// Turn a `Cow<str>` into a `Box<str>` with at most one allocation.
fn into_box(c: Cow<'_, str>) -> Box<str> {
    match c {
        Cow::Borrowed(s) => Box::from(s),
        Cow::Owned(s) => s.into_boxed_str(),
    }
}
