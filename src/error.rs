//! Non-fatal parse diagnostics.
//!
//! Parsing never fails: [`crate::parse`] always returns a best-effort tree.
//! Anything the parser had to recover from (a stray end tag, an unterminated
//! comment, an unknown entity, …) is recorded as a [`ParseError`] and exposed
//! via [`crate::Dom::errors`]. Linters and validators can inspect them; the
//! common "just give me the tree" path can ignore them entirely.

use core::fmt;

/// A single recoverable problem encountered while parsing, with the byte
/// offset into the source where it occurred.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// Byte offset into the original source string.
    pub position: usize,
    /// What kind of problem this is.
    pub kind: ErrorKind,
}

impl ParseError {
    pub(crate) fn new(position: usize, kind: ErrorKind) -> Self {
        ParseError { position, kind }
    }

    /// A short, human-readable description of the problem.
    pub fn message(&self) -> &'static str {
        self.kind.message()
    }
}

/// The category of a [`ParseError`].
///
/// Marked `#[non_exhaustive]`: new recovery cases may be added without it being
/// a breaking change, so always include a `_ =>` arm when matching.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// Input ended in the middle of a construct (tag, attribute, …).
    UnexpectedEof,
    /// A `<!-- comment` was never closed with `-->`.
    UnterminatedComment,
    /// A `<![CDATA[` section was never closed with `]]>`.
    UnterminatedCdata,
    /// A tag was opened with `<` but never closed with `>`.
    UnterminatedTag,
    /// An end tag (`</x>`) had no matching open element in reach and was
    /// ignored. An end tag reaches the 256 innermost open elements and the 8
    /// outermost (so `</body>` still closes a deep pile of unclosed tags);
    /// one whose element is open only in between is stray too, and that
    /// element stays open — closed by a later end tag in reach or at the end
    /// of the input. This keeps every end tag O(1) on any input.
    StrayEndTag,
    /// An end tag closed elements other than the current one (mis-nesting).
    /// Closing elements whose end tag is optional (`</ul>` over an open
    /// `<li>`) is reported as [`ImplicitlyClosed`](Self::ImplicitlyClosed)
    /// instead.
    MismatchedEndTag,
    /// An element was closed implicitly by another tag or end-of-input.
    ImplicitlyClosed,
    /// A reference-shaped `&…` was not recognised and was left verbatim: an
    /// unknown `&name;`, a malformed `&#…`, or a known name missing its `;`.
    /// A bare `&` (as in `a & b` or `?a=1&b=2`) is valid HTML and not reported.
    UnknownEntity,
    /// A `<! ... >` or `<? ... >` construct was treated as a bogus comment.
    BogusComment,
}

impl ErrorKind {
    pub(crate) fn message(&self) -> &'static str {
        match self {
            ErrorKind::UnexpectedEof => "unexpected end of input",
            ErrorKind::UnterminatedComment => "unterminated comment",
            ErrorKind::UnterminatedCdata => "unterminated CDATA section",
            ErrorKind::UnterminatedTag => "unterminated tag",
            ErrorKind::StrayEndTag => "stray end tag with no matching open element",
            ErrorKind::MismatchedEndTag => "end tag closed mis-nested elements",
            ErrorKind::ImplicitlyClosed => "element implicitly closed",
            ErrorKind::UnknownEntity => "unknown character reference left verbatim",
            ErrorKind::BogusComment => "bogus comment",
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.kind.message(), self.position)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ParseError {}
