//! The tree builder: turns a token stream into the [`Dom`] arena.
//!
//! Parsing is infallible. The builder keeps an "open elements" stack and
//! recovers from the common real-world mistakes — omitted end tags, mis-nested
//! tags, stray end tags — recording each as a [`ParseError`] rather than
//! bailing out. See the crate docs for the exact recovery rules we implement.

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use crate::entities;
use crate::error::{ErrorKind, ParseError};
use crate::node::{Element, NodeId, NodeKind};
use crate::tags;
use crate::token::Token;
use crate::tokenizer::Tokenizer;
use crate::tree::Dom;

/// One entry on the open-elements stack: its arena id and lower-cased name.
struct Open {
    id: NodeId,
    name: String,
}

/// Parse `src` into a best-effort [`Dom`]. Never panics; diagnostics land in
/// [`Dom::errors`].
pub(crate) fn parse_document(src: &str) -> Dom {
    let mut tk = Tokenizer::new(src);
    let mut dom = Dom::with_capacity(src.len() / 8 + 4);
    let mut open: Vec<Open> = Vec::new();

    while let Some((tok, pos)) = tk.next_token() {
        match tok {
            Token::StartTag {
                name,
                attrs,
                self_closing,
            } => {
                let lname = name.to_ascii_lowercase();

                // Implicit close: pop open elements that this start tag closes.
                while let Some(top) = open.last() {
                    if tags::implicitly_closes(&lname, &top.name) {
                        open.pop();
                        dom.errors.push(ParseError::new(pos, ErrorKind::ImplicitlyClosed));
                    } else {
                        break;
                    }
                }

                let parent = open.last().map(|o| o.id);

                let mut built: Vec<(Box<str>, Box<str>)> = Vec::with_capacity(attrs.len());
                for (k, v) in attrs {
                    let key: Box<str> = k.to_ascii_lowercase().into_boxed_str();
                    let (decoded, unknown) = entities::decode(v.as_str());
                    if unknown {
                        dom.errors.push(ParseError::new(pos, ErrorKind::UnknownEntity));
                    }
                    built.push((key, into_box(decoded)));
                }

                let element = Element {
                    name: lname.as_str().into(),
                    attrs: built,
                };
                let id = dom.alloc(NodeKind::Element(element));
                dom.append(parent, id);

                if !self_closing && !tags::is_void(&lname) {
                    open.push(Open { id, name: lname });
                }
            }

            Token::EndTag { name } => {
                let lname = name.to_ascii_lowercase();
                if tags::is_void(&lname) {
                    continue; // void elements never have end tags
                }
                match open.iter().rposition(|o| o.name == lname) {
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
                let parent = open.last().map(|o| o.id);
                let text: Box<str> = if parent_is_raw(&open) {
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
                let parent = open.last().map(|o| o.id);
                let id = dom.alloc(NodeKind::Text(Box::from(c)));
                dom.append(parent, id);
            }

            Token::Comment(c) => {
                let parent = open.last().map(|o| o.id);
                let id = dom.alloc(NodeKind::Comment(Box::from(c)));
                dom.append(parent, id);
            }

            Token::Doctype(d) => {
                let parent = open.last().map(|o| o.id);
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

/// Whether the innermost open element keeps its text raw (no entity decoding).
fn parent_is_raw(open: &[Open]) -> bool {
    open.last().is_some_and(|o| tags::is_raw_text(&o.name))
}

/// Turn a `Cow<str>` into a `Box<str>` with at most one allocation.
fn into_box(c: Cow<'_, str>) -> Box<str> {
    match c {
        Cow::Borrowed(s) => Box::from(s),
        Cow::Owned(s) => s.into_boxed_str(),
    }
}
