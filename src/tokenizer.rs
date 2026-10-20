//! The streaming HTML tokenizer.
//!
//! Byte-driven and pull-based: it implements [`Iterator`] and yields borrowed
//! [`Token`]s without allocating anything except each tag's attribute list. All
//! cursor access goes through `get()`/iterators, so it never panics on
//! malformed or truncated input — the whole point of the crate.

use alloc::vec::Vec;

use crate::error::{ErrorKind, ParseError};
use crate::token::{AttrValue, Token};

/// A streaming HTML tokenizer. Construct with [`Tokenizer::new`] (or the
/// top-level [`crate::tokenize`]) and iterate to get [`Token`]s.
pub struct Tokenizer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    /// When set, the next token is the raw-text content up to this element's
    /// matching end tag (used for `<script>`/`<style>`/`<title>`/`<textarea>`).
    raw_text_end: Option<&'a str>,
    pub(crate) errors: Vec<ParseError>,
}

impl<'a> Tokenizer<'a> {
    /// Create a tokenizer over `src`.
    pub fn new(src: &'a str) -> Self {
        Tokenizer {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            raw_text_end: None,
            errors: Vec::new(),
        }
    }

    // --- cursor helpers (all panic-free) ---------------------------------

    #[inline]
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    #[inline]
    fn peek_at(&self, off: usize) -> Option<u8> {
        self.bytes.get(self.pos + off).copied()
    }

    #[inline]
    fn slice(&self, start: usize, end: usize) -> &'a str {
        self.src.get(start..end).unwrap_or("")
    }

    #[inline]
    fn rest(&self) -> &'a [u8] {
        self.bytes.get(self.pos..).unwrap_or(&[])
    }

    fn starts_with(&self, s: &[u8]) -> bool {
        self.rest().starts_with(s)
    }

    fn starts_with_ci(&self, s: &[u8]) -> bool {
        self.rest()
            .get(..s.len())
            .is_some_and(|p| p.eq_ignore_ascii_case(s))
    }

    fn skip_whitespace(&mut self) {
        while let Some(b) = self.peek() {
            if b.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn error(&mut self, pos: usize, kind: ErrorKind) {
        self.errors.push(ParseError::new(pos, kind));
    }

    /// The byte offset just past the last token returned (its end).
    #[inline]
    pub(crate) fn offset(&self) -> usize {
        self.pos
    }

    // --- token production ------------------------------------------------

    /// Like `Iterator::next`, but also returns the byte offset the token
    /// started at (used internally by the tree builder for diagnostics).
    pub(crate) fn next_token(&mut self) -> Option<(Token<'a>, usize)> {
        if self.pos >= self.bytes.len() {
            return None;
        }
        let start = self.pos;
        if let Some(end_name) = self.raw_text_end.take() {
            return Some((self.read_raw_text(end_name), start));
        }
        let tok = match self.peek() {
            Some(b'<') => self.read_markup(),
            _ => self.read_text(),
        };
        Some((tok, start))
    }

    fn read_text(&mut self) -> Token<'a> {
        let start = self.pos;
        // Scan to the next '<' in one shot — the iterator `position` loop
        // auto-vectorizes far better than a byte-at-a-time `peek` loop.
        match self.rest().iter().position(|&b| b == b'<') {
            Some(rel) => self.pos += rel,
            None => self.pos = self.bytes.len(),
        }
        Token::Text(self.slice(start, self.pos))
    }

    fn read_markup(&mut self) -> Token<'a> {
        match self.peek_at(1) {
            Some(b'/') => self.read_end_tag(),
            Some(b'!') => self.read_bang(),
            Some(b'?') => self.read_bogus_comment(),
            Some(c) if c.is_ascii_alphabetic() => self.read_start_tag(),
            _ => {
                // A lone '<' that doesn't begin markup is literal text.
                let s = self.slice(self.pos, self.pos + 1);
                self.pos += 1;
                Token::Text(s)
            }
        }
    }

    fn read_tag_name(&mut self) -> &'a str {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b':' | b'_' | b'.') {
                self.pos += 1;
            } else {
                break;
            }
        }
        self.slice(start, self.pos)
    }

    fn read_start_tag(&mut self) -> Token<'a> {
        let open = self.pos;
        self.pos += 1; // consume '<'
        let name = self.read_tag_name();
        let mut attrs = Vec::new();
        let mut self_closing = false;

        loop {
            self.skip_whitespace();
            match self.peek() {
                None => {
                    self.error(open, ErrorKind::UnterminatedTag);
                    break;
                }
                Some(b'>') => {
                    self.pos += 1;
                    break;
                }
                Some(b'/') => {
                    if self.peek_at(1) == Some(b'>') {
                        self_closing = true;
                        self.pos += 2;
                        break;
                    }
                    self.pos += 1; // stray slash
                }
                Some(_) => match self.read_attribute() {
                    Some(attr) => attrs.push(attr),
                    None => self.pos += 1, // couldn't form an attr; make progress
                },
            }
        }

        if !self_closing && is_rawish_name(name) {
            self.raw_text_end = Some(name);
        }

        Token::StartTag {
            name,
            attrs,
            self_closing,
        }
    }

    fn read_attribute(&mut self) -> Option<(&'a str, AttrValue<'a>)> {
        let nstart = self.pos;
        while let Some(b) = self.peek() {
            if b.is_ascii_whitespace() || matches!(b, b'=' | b'>' | b'/') {
                break;
            }
            self.pos += 1;
        }
        let name = self.slice(nstart, self.pos);
        if name.is_empty() {
            return None;
        }
        self.skip_whitespace();
        if self.peek() == Some(b'=') {
            self.pos += 1;
            self.skip_whitespace();
            Some((name, self.read_attr_value()))
        } else {
            Some((name, AttrValue::Empty))
        }
    }

    fn read_attr_value(&mut self) -> AttrValue<'a> {
        match self.peek() {
            Some(q @ (b'"' | b'\'')) => {
                self.pos += 1; // opening quote
                let start = self.pos;
                while let Some(b) = self.peek() {
                    if b == q {
                        break;
                    }
                    self.pos += 1;
                }
                let val = self.slice(start, self.pos);
                if self.peek() == Some(q) {
                    self.pos += 1; // closing quote
                }
                AttrValue::Quoted(val)
            }
            _ => {
                let start = self.pos;
                while let Some(b) = self.peek() {
                    if b.is_ascii_whitespace() || b == b'>' {
                        break;
                    }
                    self.pos += 1;
                }
                AttrValue::Unquoted(self.slice(start, self.pos))
            }
        }
    }

    fn read_end_tag(&mut self) -> Token<'a> {
        let open = self.pos;
        self.pos += 2; // consume '</'
        let name = self.read_tag_name();
        loop {
            match self.peek() {
                None => {
                    self.error(open, ErrorKind::UnterminatedTag);
                    break;
                }
                Some(b'>') => {
                    self.pos += 1;
                    break;
                }
                Some(_) => self.pos += 1,
            }
        }
        Token::EndTag { name }
    }

    fn read_bang(&mut self) -> Token<'a> {
        if self.starts_with(b"<!--") {
            self.read_comment()
        } else if self.starts_with(b"<![CDATA[") {
            self.read_cdata()
        } else if self.starts_with_ci(b"<!doctype") {
            self.read_doctype()
        } else {
            self.read_bogus_comment()
        }
    }

    fn read_comment(&mut self) -> Token<'a> {
        let open = self.pos;
        self.pos += 4; // consume '<!--'
        let start = self.pos;
        while !self.rest().is_empty() {
            if self.starts_with(b"-->") {
                let s = self.slice(start, self.pos);
                self.pos += 3;
                return Token::Comment(s);
            }
            self.pos += 1;
        }
        self.error(open, ErrorKind::UnterminatedComment);
        Token::Comment(self.slice(start, self.pos))
    }

    fn read_cdata(&mut self) -> Token<'a> {
        let open = self.pos;
        self.pos += 9; // consume '<![CDATA['
        let start = self.pos;
        while !self.rest().is_empty() {
            if self.starts_with(b"]]>") {
                let s = self.slice(start, self.pos);
                self.pos += 3;
                return Token::Cdata(s);
            }
            self.pos += 1;
        }
        self.error(open, ErrorKind::UnterminatedCdata);
        Token::Cdata(self.slice(start, self.pos))
    }

    fn read_doctype(&mut self) -> Token<'a> {
        let open = self.pos;
        self.pos += 9; // consume '<!doctype'
        self.skip_whitespace();
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b == b'>' {
                break;
            }
            self.pos += 1;
        }
        let s = self.slice(start, self.pos);
        if self.peek() == Some(b'>') {
            self.pos += 1;
        } else {
            self.error(open, ErrorKind::UnterminatedTag);
        }
        Token::Doctype(s)
    }

    /// `<! ... >` (not comment/doctype/cdata) and `<? ... >` are treated as
    /// bogus comments, matching browser recovery.
    fn read_bogus_comment(&mut self) -> Token<'a> {
        let open = self.pos;
        self.pos += 1; // consume '<'
        if matches!(self.peek(), Some(b'!') | Some(b'?')) {
            self.pos += 1;
        }
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b == b'>' {
                break;
            }
            self.pos += 1;
        }
        let s = self.slice(start, self.pos);
        if self.peek() == Some(b'>') {
            self.pos += 1;
        }
        self.error(open, ErrorKind::BogusComment);
        Token::Comment(s)
    }

    /// Scan raw-text content up to (but not consuming) the matching end tag.
    fn read_raw_text(&mut self, end_name: &str) -> Token<'a> {
        let start = self.pos;
        loop {
            match self.rest().iter().position(|&b| b == b'<') {
                None => {
                    self.pos = self.bytes.len();
                    break;
                }
                Some(rel) => {
                    let lt = self.pos + rel;
                    if self.is_close_tag_at(lt, end_name) {
                        self.pos = lt;
                        break;
                    }
                    self.pos = lt + 1;
                }
            }
        }
        Token::Text(self.slice(start, self.pos))
    }

    fn is_close_tag_at(&self, lt: usize, name: &str) -> bool {
        if self.bytes.get(lt) != Some(&b'<') || self.bytes.get(lt + 1) != Some(&b'/') {
            return false;
        }
        let after = lt + 2;
        let nm = name.as_bytes();
        match self.bytes.get(after..after + nm.len()) {
            Some(slice) if slice.eq_ignore_ascii_case(nm) => {
                match self.bytes.get(after + nm.len()) {
                    None => true,
                    Some(b) => b.is_ascii_whitespace() || matches!(b, b'>' | b'/'),
                }
            }
            _ => false,
        }
    }
}

impl<'a> Iterator for Tokenizer<'a> {
    type Item = Token<'a>;

    fn next(&mut self) -> Option<Token<'a>> {
        self.next_token().map(|(tok, _)| tok)
    }
}

/// Whether an element's content is treated as raw text by the tokenizer
/// (no nested tags). Entity-decoding of that text is decided later, per element.
fn is_rawish_name(name: &str) -> bool {
    ["script", "style", "title", "textarea"]
        .iter()
        .any(|t| name.eq_ignore_ascii_case(t))
}
