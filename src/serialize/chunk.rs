//! RAG chunking of the Markdown output: [`Dom::to_markdown_chunks`] and
//! [`NodeRef::to_markdown_chunks`], behind the `markdown` feature.
//!
//! Works on the finished Markdown: it is cut into blocks at blank lines
//! (never inside a fenced code block), sections start at top-level ATX
//! headings, and blocks are packed greedily into chunks under the budget.
//! Linear in the size of the Markdown.

use alloc::string::String;
use alloc::vec::Vec;
use core::mem;

use super::is_space;
use crate::tree::{Dom, NodeRef};

/// A piece of a document's Markdown, small enough for an embedding model or a
/// prompt, with the headings it sits under — so every chunk stands on its
/// own. Built by [`Dom::to_markdown_chunks`].
///
/// With the `serde` feature, `Chunk` implements `serde::Serialize`
/// (`{"heading_path": [...], "markdown": "..."}`).
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Chunk {
    /// The plain-text titles of the headings this chunk sits under, outermost
    /// first (`["Guide", "Install"]`). Empty before the first heading.
    pub heading_path: Vec<String>,
    /// The chunk's Markdown. It starts with its section's heading line when
    /// it is the first chunk of that section.
    pub markdown: String,
}

impl Chunk {
    /// The heading path as one line of context, joined with `" > "` — e.g.
    /// `"Guide > Install"` — to prepend to the chunk when embedding it. Empty
    /// when the chunk comes before any heading.
    ///
    /// ```
    /// let dom = domtree::parse("<h1>Guide</h1><h2>Install</h2><p>Run it.</p>");
    /// let chunks = dom.to_markdown_chunks(1000);
    /// assert_eq!(chunks[0].context(), "Guide > Install");
    /// ```
    pub fn context(&self) -> String {
        self.heading_path.join(" > ")
    }
}

impl Dom {
    /// The document's Markdown ([`Dom::to_markdown`]) split into chunks of at
    /// most `max_chars` characters (Unicode scalar values) for retrieval
    /// (RAG), each carrying its heading path.
    ///
    /// - **Sections.** A chunk never spans two sections: each top-level
    ///   heading (`#` … `######`) starts a new chunk, and its plain-text title
    ///   joins the [`Chunk::heading_path`] (replacing any heading of the same
    ///   or a deeper level). A heading directly followed by a deeper one stays
    ///   in the same chunk, so `# Guide` / `## Install` / text is one chunk.
    /// - **Packing.** Within a section, whole blocks (paragraphs, lists,
    ///   quotes, tables, fenced code) are packed greedily, separated by a
    ///   blank line, while the chunk stays within `max_chars`. Small sections
    ///   are not merged with their neighbours.
    /// - **Oversized blocks.** A block that alone exceeds `max_chars` is split
    ///   at line boundaries. A split code block is re-fenced and a split table
    ///   repeats its header rows in every piece, so each piece is valid
    ///   Markdown on its own (when the fence or header leaves room for a
    ///   line; otherwise lines are split plainly).
    ///
    /// Every chunk is at most `max_chars` long, except one made of a single
    /// line that is itself longer — lines are never cut (a long paragraph is
    /// one line). A section heading that cannot share a chunk with the first
    /// block under it becomes a chunk of its own. No content is lost or
    /// reordered: concatenating the chunks gives back the Markdown, plus any
    /// repeated fences and table headers. `max_chars == 0` is allowed (every
    /// line becomes its own chunk). Blank chunks are never returned.
    ///
    /// ```
    /// let dom = domtree::parse(
    ///     "<h1>Guide</h1><p>Welcome.</p>\
    ///      <h2>Install</h2><p>Run the installer.</p><pre>cargo add dom-tree-rs</pre>\
    ///      <h2>Usage</h2><p>Parse, then query.</p>",
    /// );
    /// let chunks = dom.to_markdown_chunks(500);
    /// let parts: Vec<_> = chunks.iter().map(|c| (c.context(), c.markdown.as_str())).collect();
    /// assert_eq!(parts, [
    ///     ("Guide".to_string(), "# Guide\n\nWelcome."),
    ///     ("Guide > Install".to_string(), "## Install\n\nRun the installer.\n\n```\ncargo add dom-tree-rs\n```"),
    ///     ("Guide > Usage".to_string(), "## Usage\n\nParse, then query."),
    /// ]);
    /// ```
    pub fn to_markdown_chunks(&self, max_chars: usize) -> Vec<Chunk> {
        chunk_markdown(&self.to_markdown(), max_chars)
    }
}

impl NodeRef<'_> {
    /// This node's subtree as Markdown chunks; see [`Dom::to_markdown_chunks`].
    ///
    /// ```
    /// let dom = domtree::parse(
    ///     "<nav>Home</nav><main><h2>Notes</h2><p>One.</p><p>Two.</p></main>",
    /// );
    /// let main = dom.find_by_tag("main").next().unwrap();
    /// let chunks = main.to_markdown_chunks(14);
    /// assert_eq!(chunks.len(), 2);
    /// assert_eq!(chunks[0].markdown, "## Notes\n\nOne.");
    /// assert_eq!(chunks[1].markdown, "Two.");
    /// assert_eq!(chunks[1].heading_path, ["Notes"]);
    /// ```
    pub fn to_markdown_chunks(&self, max_chars: usize) -> Vec<Chunk> {
        chunk_markdown(&self.to_markdown(), max_chars)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Heading(u8),
    /// A fenced code block at the top level: first and last lines are fences.
    Fence,
    /// A GFM table: a header row and a delimiter row, then body rows.
    Table,
    Text,
}

struct Block<'a> {
    lines: Vec<&'a str>,
    kind: Kind,
}

fn chunk_markdown(md: &str, max: usize) -> Vec<Chunk> {
    let mut packer = Packer {
        max,
        ..Packer::default()
    };
    for block in blocks(md) {
        let text = block.lines.join("\n");
        let len = text.chars().count();
        match block.kind {
            Kind::Heading(level) => packer.heading(level, text, len),
            _ => packer.block(&block, text, len),
        }
    }
    packer.flush();
    packer.chunks
}

#[derive(Default)]
struct Packer {
    max: usize,
    chunks: Vec<Chunk>,
    /// The open headings: level and plain-text title.
    path: Vec<(u8, String)>,
    cur: String,
    /// `cur`'s length in characters.
    len: usize,
    /// `cur` holds something besides headings.
    body: bool,
    /// The level of the last heading in `cur`.
    level: Option<u8>,
}

impl Packer {
    fn fits(&self, len: usize) -> bool {
        if self.cur.is_empty() {
            len <= self.max
        } else {
            self.len.saturating_add(2).saturating_add(len) <= self.max
        }
    }

    fn append(&mut self, text: &str, len: usize) {
        if !self.cur.is_empty() {
            self.cur.push_str("\n\n");
            self.len = self.len.saturating_add(2);
        }
        self.cur.push_str(text);
        self.len = self.len.saturating_add(len);
    }

    fn flush(&mut self) {
        let markdown = mem::take(&mut self.cur);
        self.len = 0;
        self.body = false;
        self.level = None;
        if markdown.chars().any(|c| !c.is_whitespace()) {
            self.chunks.push(Chunk {
                heading_path: self.path.iter().map(|(_, t)| t.clone()).collect(),
                markdown,
            });
        }
    }

    fn heading(&mut self, level: u8, line: String, len: usize) {
        if self.body || self.level.is_some_and(|l| level <= l) || !self.fits(len) {
            self.flush();
        }
        while self.path.last().is_some_and(|&(l, _)| l >= level) {
            self.path.pop();
        }
        self.path.push((level, heading_text(&line)));
        self.append(&line, len);
        self.level = Some(level);
    }

    fn block(&mut self, block: &Block<'_>, text: String, len: usize) {
        if self.fits(len) {
            self.append(&text, len);
            self.body = true;
            return;
        }
        if len <= self.max {
            self.flush();
            self.append(&text, len);
            self.body = true;
            return;
        }
        // Too big for any chunk: split it, keeping a pending heading with the
        // first piece when that piece fits next to it.
        let room = if self.cur.is_empty() || self.body {
            0
        } else {
            self.max.saturating_sub(self.len.saturating_add(2))
        };
        let mut pieces = Vec::new();
        if room > 0 {
            pieces = split(block, room, self.max);
            if pieces.first().is_some_and(|&(_, l)| l > room) {
                pieces.clear();
            }
        }
        if pieces.is_empty() {
            self.flush();
            pieces = split(block, self.max, self.max);
        }
        for (i, (piece, piece_len)) in pieces.into_iter().enumerate() {
            if i > 0 {
                self.flush();
            }
            self.append(&piece, piece_len);
            self.body = true;
        }
    }
}

/// Split an oversized block at line boundaries into pieces of at most
/// `first` (the first piece) and `max` characters, except where a single
/// line is longer. Fenced code is re-fenced and tables repeat their header
/// in each piece when that leaves room for a line.
fn split(block: &Block<'_>, first: usize, max: usize) -> Vec<(String, usize)> {
    let chars = |s: &str| s.chars().count();
    // Characters of lines joined with `\n`.
    let size =
        |ls: &[&str]| ls.iter().map(|l| chars(l)).sum::<usize>() + ls.len().saturating_sub(1);
    let lines = block.lines.as_slice();
    // Repeated in every piece: a code block's fence lines, a table's header
    // and delimiter rows.
    let (head, body, tail): (&[&str], &[&str], &[&str]) = match block.kind {
        Kind::Fence if lines.len() > 2 => {
            let (open, rest) = lines.split_at_checked(1).unwrap_or((&[], lines));
            let last = rest.len().saturating_sub(1);
            let (body, close) = rest.split_at_checked(last).unwrap_or((rest, &[]));
            (open, body, close)
        }
        Kind::Table if lines.len() > 2 => {
            let (head, body) = lines.split_at_checked(2).unwrap_or((&[], lines));
            (head, body, &[])
        }
        _ => (&[], lines, &[]),
    };
    let longest = body.iter().map(|l| chars(l)).max().unwrap_or(0);
    let tail_len = |tail: &[&str]| if tail.is_empty() { 0 } else { 1 + size(tail) };
    // Repeating only pays if every piece still has room for a line.
    let wrap = !head.is_empty() && size(head) + 1 + longest + tail_len(tail) <= max;
    let (head, body, tail) = if wrap {
        (head, body, tail)
    } else {
        (&[][..], lines, &[][..])
    };
    let (head_text, head_len) = (head.join("\n"), size(head));
    let (tail_text, tail_len) = (tail.join("\n"), tail_len(tail));

    let mut pieces = Vec::new();
    let mut limit = first;
    let mut piece = head_text.clone();
    let mut len = head_len;
    let mut count = head.len(); // lines in `piece`
    let mut has_body = false;
    for line in body {
        let line_len = chars(line);
        let sep = usize::from(count > 0);
        if has_body && len + sep + line_len + tail_len > limit {
            if !tail.is_empty() {
                piece.push('\n');
                piece.push_str(&tail_text);
                len += tail_len;
            }
            pieces.push((mem::replace(&mut piece, head_text.clone()), len));
            len = head_len;
            count = head.len();
            limit = max;
        }
        if count > 0 {
            piece.push('\n');
            len += 1;
        }
        piece.push_str(line);
        len += line_len;
        count += 1;
        has_body = true;
    }
    if !tail.is_empty() {
        piece.push('\n');
        piece.push_str(&tail_text);
        len += tail_len;
    }
    pieces.push((piece, len));
    pieces
}

/// Cut Markdown into blocks at blank lines, keeping fenced code blocks whole;
/// each top-level ATX heading is a block of its own.
fn blocks(md: &str) -> Vec<Block<'_>> {
    let mut out = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    let mut fence: Option<usize> = None;
    for line in md.split('\n') {
        if let Some(open) = fence {
            cur.push(line);
            if closes_fence(line, open) {
                fence = None;
            }
            continue;
        }
        if line.trim().is_empty() {
            push_block(&mut out, &mut cur);
            continue;
        }
        if let Some(level) = heading_level(line) {
            push_block(&mut out, &mut cur);
            out.push(Block {
                lines: alloc::vec![line],
                kind: Kind::Heading(level),
            });
            continue;
        }
        fence = opens_fence(line);
        cur.push(line);
    }
    push_block(&mut out, &mut cur);
    out
}

fn push_block<'a>(out: &mut Vec<Block<'a>>, cur: &mut Vec<&'a str>) {
    if cur.is_empty() {
        return;
    }
    let lines = mem::take(cur);
    let first = lines.first().copied().unwrap_or("");
    let last = lines.last().copied().unwrap_or("");
    let fence = (lines.len() >= 2 && first.starts_with("```"))
        .then(|| opens_fence(first))
        .flatten()
        .is_some_and(|n| !last.starts_with(' ') && closes_fence(last, n));
    let table = first.starts_with('|')
        && lines.get(1).is_some_and(|l| {
            l.starts_with('|') && l.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
        });
    let kind = if fence {
        Kind::Fence
    } else if table {
        Kind::Table
    } else {
        Kind::Text
    };
    out.push(Block { lines, kind });
}

/// The level of an ATX heading line (`## Title`), if `line` is one.
fn heading_level(line: &str) -> Option<u8> {
    let hashes = line.bytes().take_while(|&b| b == b'#').count();
    let after = line.get(hashes..)?;
    ((1..=6).contains(&hashes) && after.starts_with(' ')).then_some(hashes as u8)
}

/// Strip the container prefixes a line can carry inside block quotes and
/// list items (`> `, `- `, `1. `, indentation).
fn content(line: &str) -> &str {
    let mut rest = line;
    loop {
        let trimmed = rest.trim_start_matches(' ');
        if let Some(r) = trimmed.strip_prefix('>') {
            rest = r.strip_prefix(' ').unwrap_or(r);
            continue;
        }
        if let Some(r) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
            .or_else(|| trimmed.strip_prefix("+ "))
        {
            rest = r;
            continue;
        }
        let digits = trimmed.bytes().take_while(u8::is_ascii_digit).count();
        if (1..=9).contains(&digits) {
            if let Some(r) = trimmed
                .get(digits..)
                .and_then(|r| r.strip_prefix(". ").or_else(|| r.strip_prefix(") ")))
            {
                rest = r;
                continue;
            }
        }
        return trimmed;
    }
}

/// A fence opening line: three or more backticks and an info string without
/// backticks. Returns the fence length.
fn opens_fence(line: &str) -> Option<usize> {
    let line = content(line);
    let n = line.bytes().take_while(|&b| b == b'`').count();
    (n >= 3 && !line.get(n..).is_some_and(|info| info.contains('`'))).then_some(n)
}

/// A line closing a fence of `open` backticks.
fn closes_fence(line: &str, open: usize) -> bool {
    let line = content(line).trim_end();
    line.len() >= open && line.bytes().all(|b| b == b'`')
}

/// The plain text of an ATX heading line: markers, emphasis, link
/// destinations and escapes removed, whitespace collapsed.
fn heading_text(line: &str) -> String {
    let mut rest = line.trim_start_matches('#');
    let mut out = String::with_capacity(rest.len());
    while let Some(c) = rest.chars().next() {
        let after = rest.get(c.len_utf8()..).unwrap_or("");
        match c {
            '\\' => match after.chars().next() {
                Some(n) if n.is_ascii_punctuation() => {
                    out.push(n);
                    rest = after.get(1..).unwrap_or("");
                    continue;
                }
                _ => out.push('\\'),
            },
            '`' => {
                let n = rest.bytes().take_while(|&b| b == b'`').count();
                let body = rest.get(n..).unwrap_or("");
                match code_span(body, n) {
                    Some((code, after_span)) => {
                        out.push_str(code);
                        rest = after_span;
                    }
                    None => {
                        out.push_str(rest.get(..n).unwrap_or(""));
                        rest = body;
                    }
                }
                continue;
            }
            '*' | '~' | '[' => {}
            '!' if after.starts_with('[') => {}
            ']' if after.starts_with('(') => {
                rest = skip_destination(after);
                continue;
            }
            ']' => {}
            c => out.push(c),
        }
        rest = after;
    }
    let mut text = String::with_capacity(out.len());
    for word in out.split(is_space).filter(|w| !w.is_empty()) {
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(word);
    }
    text
}

/// The content of a code span whose opening run of `n` backticks preceded
/// `body`, and the text after its closing run.
fn code_span(body: &str, n: usize) -> Option<(&str, &str)> {
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes.get(i) != Some(&b'`') {
            i += 1;
            continue;
        }
        let start = i;
        while bytes.get(i) == Some(&b'`') {
            i += 1;
        }
        if i - start == n {
            let code = body.get(..start)?;
            let code = match code.strip_prefix(' ').and_then(|c| c.strip_suffix(' ')) {
                Some(inner) if !inner.trim().is_empty() => inner,
                _ => code,
            };
            return Some((code, body.get(i..)?));
        }
    }
    None
}

/// Skip a link destination and title, `(…)`, returning what follows.
fn skip_destination(rest: &str) -> &str {
    let mut angle = false;
    let mut quote = false;
    let mut escaped = false;
    for (i, c) in rest.char_indices().skip(1) {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '<' if !quote && i == 1 => angle = true,
            '>' if angle => angle = false,
            '"' if !angle => quote = !quote,
            ')' if !angle && !quote => return rest.get(i + 1..).unwrap_or(""),
            _ => {}
        }
    }
    ""
}

#[cfg(feature = "serde")]
impl serde::Serialize for Chunk {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("Chunk", 2)?;
        st.serialize_field("heading_path", &self.heading_path)?;
        st.serialize_field("markdown", &self.markdown)?;
        st.end()
    }
}
