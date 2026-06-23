//! The token type produced by the streaming [`Tokenizer`](crate::Tokenizer).
//!
//! Tokens borrow directly from the source string — tokenizing allocates nothing
//! except the per-tag attribute list. Text and attribute values are returned
//! **raw** (HTML entities are *not* decoded here); decoding is deferred until a
//! tree node is built or [`text`](crate::NodeRef::text) is requested, which
//! keeps the token stream zero-copy.

use alloc::vec::Vec;

/// A single lexical token of HTML.
#[derive(Clone, Debug, PartialEq)]
pub enum Token<'a> {
    /// A start tag such as `<div class="x">` or a self-closing `<br/>`.
    StartTag {
        /// Tag name, exactly as written (not lower-cased).
        name: &'a str,
        /// Attributes in source order. Duplicate names are kept as-is.
        attrs: Vec<(&'a str, AttrValue<'a>)>,
        /// Whether the tag used XML-style self-closing syntax (`/>`).
        self_closing: bool,
    },
    /// An end tag such as `</div>`.
    EndTag {
        /// Tag name, exactly as written.
        name: &'a str,
    },
    /// A run of character data. Raw (undecoded) slice of the source.
    Text(&'a str),
    /// The inner text of an `<!-- ... -->` comment (or a bogus comment).
    Comment(&'a str),
    /// The inner text of a `<!doctype ...>` declaration.
    Doctype(&'a str),
    /// The inner text of a `<![CDATA[ ... ]]>` section.
    Cdata(&'a str),
}

/// The value side of an attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttrValue<'a> {
    /// `name="value"` or `name='value'` — slice excludes the quotes.
    Quoted(&'a str),
    /// `name=value` with no quotes.
    Unquoted(&'a str),
    /// A boolean attribute with no value, e.g. `disabled`.
    Empty,
}

impl<'a> AttrValue<'a> {
    /// The raw value as a string slice (`""` for [`Empty`](AttrValue::Empty)).
    pub fn as_str(&self) -> &'a str {
        match self {
            AttrValue::Quoted(s) | AttrValue::Unquoted(s) => s,
            AttrValue::Empty => "",
        }
    }
}
