//! HTML → Markdown (CommonMark, plus GFM tables and strikethrough), behind the
//! `markdown` feature.
//!
//! One iterative pass over the tree (`walk`). Inline content collects in a line
//! buffer — whitespace collapsed, Markdown punctuation escaped, emphasis and
//! link markers opened lazily so empty spans vanish — and is written out at each
//! block boundary behind the prefixes of the open containers (`> ` for quotes,
//! a list marker or its continuation indent). Containers nested deeper than
//! [`MAX_NEST`] are flattened rather than indented further, which bounds the
//! per-line work and the output size; their content is still emitted.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write as _;
use core::mem;

use super::{is_space, push_verbatim, skip_newline, walk};
use crate::node::NodeKind;
use crate::tags::{self, Display};
use crate::tree::{Dom, NodeRef};

/// Quotes and list items nested deeper than this are not indented further.
const MAX_NEST: usize = 32;

/// Tables wider than this are not padded to a rectangle (GFM fills in the
/// missing cells), so a ragged table cannot blow up the output quadratically.
const MAX_PAD: usize = 64;

impl Dom {
    /// Convert the document to Markdown (CommonMark + GFM tables and
    /// strikethrough) — handy for LLM prompts, RAG indexes and notes.
    ///
    /// Headings, paragraphs, emphasis, links, images, code spans, fenced code
    /// blocks (language from `class="language-…"`), nested lists, block quotes
    /// and tables map to their Markdown forms, and text is escaped so it renders
    /// as itself. Hidden elements (`<head>`, `<script>`, …) are skipped as in
    /// [`Dom::to_text`]. Blocks are separated by one blank line, `<br>` becomes
    /// a backslash hard break, and the output has no leading or trailing
    /// newline.
    ///
    /// ```
    /// let dom = domtree::parse(
    ///     "<h1>Title</h1><p>Some <b>bold</b> and a <a href='/x'>link</a>.</p>\
    ///      <ul><li>one</li><li>two</li></ul>",
    /// );
    /// assert_eq!(
    ///     dom.to_markdown(),
    ///     "# Title\n\nSome **bold** and a [link](/x).\n\n- one\n- two",
    /// );
    /// ```
    pub fn to_markdown(&self) -> String {
        let mut md = Markdown::default();
        for root in self.roots() {
            walk(root, |node, enter| md.visit(node, enter));
        }
        md.finish()
    }
}

impl NodeRef<'_> {
    /// This node's subtree as Markdown; see [`Dom::to_markdown`].
    ///
    /// ```
    /// let dom = domtree::parse(
    ///     "<div><pre><code class='language-rust'>fn main() {}\n</code></pre></div>",
    /// );
    /// let pre = dom.find_by_tag("pre").next().unwrap();
    /// assert_eq!(pre.to_markdown(), "```rust\nfn main() {}\n```");
    /// ```
    pub fn to_markdown(&self) -> String {
        let mut md = Markdown::default();
        walk(*self, |node, enter| md.visit(node, enter));
        md.finish()
    }
}

/// An inline element whose content is wrapped in markers.
#[derive(Clone, Copy)]
enum Mark<'a> {
    Strong,
    Emphasis,
    Strike,
    Link(NodeRef<'a>),
}

/// An open [`Mark`]: `open` while its opening marker is in the current line,
/// `used` once it has wrapped any content.
#[derive(Clone, Copy)]
struct Wrap<'a> {
    mark: Mark<'a>,
    open: bool,
    used: bool,
    /// Whitespace was pending when the element opened.
    spaced: bool,
}

/// A container that prefixes each line written inside it.
enum Prefix {
    /// `> `; `true` once a line has been written through it.
    Quote(bool),
    /// A list item: its list (index into `lists`), and the width of its marker
    /// — 0 until the marker is written on the item's first line.
    Item { list: Option<usize>, width: usize },
}

struct List {
    ordered: bool,
    next: u32,
    /// Whether an item has been written (later items then follow tightly).
    written: bool,
}

/// Text captured verbatim for a code span or a fenced code block.
struct Code<'a> {
    buf: String,
    block: bool,
    /// Nothing captured yet: a leading newline is dropped (as after `<pre>`).
    fresh: bool,
    lang: &'a str,
}

/// A GFM table being collected; it is written when the `<table>` closes.
#[derive(Default)]
struct Table {
    rows: Vec<(bool, Vec<String>)>,
    cells: Vec<String>,
    head: bool,
}

/// What an open element changed, undone when it closes.
#[derive(Clone, Copy, Default)]
struct Frame {
    block: bool,
    space: bool,
    mark: bool,
    prefix: bool,
    list: bool,
    inline: bool,
    heading: bool,
    code: bool,
    role: Role,
}

#[derive(Clone, Copy, Default)]
enum Role {
    #[default]
    None,
    Table,
    Row,
    Cell,
}

#[derive(Default)]
struct Markdown<'a> {
    out: String,
    /// Separation owed before the next block: 1 = new line, 2 = blank line.
    sep: u8,
    prefixes: Vec<Prefix>,
    lists: Vec<List>,
    frames: Vec<Frame>,
    /// The current paragraph's inline Markdown (hard breaks are `\` + `\n`).
    line: String,
    space: bool,
    breaks: u8,
    wraps: Vec<Wrap<'a>>,
    heading: u8,
    /// Depth of contexts that only take inline content (headings, table cells).
    inline_only: u32,
    code: Option<Code<'a>>,
    table: Option<Table>,
}

impl<'a> Markdown<'a> {
    fn visit(&mut self, node: NodeRef<'a>, enter: bool) -> bool {
        match node.kind() {
            NodeKind::Text(t) => self.text(t),
            NodeKind::Element { tag } if enter => return self.open(node, tag),
            NodeKind::Element { .. } => self.close(),
            NodeKind::Comment(_) | NodeKind::Doctype(_) => {}
        }
        false
    }

    fn open(&mut self, node: NodeRef<'a>, tag: &'a str) -> bool {
        let display = tags::display(tag);
        if display == Display::Hidden {
            return false;
        }
        let mut f = Frame::default();
        if let Some(code) = &mut self.code {
            // Inside code only the line structure matters.
            if tag == "br" {
                code.br();
            } else if display != Display::Inline {
                code.newline();
                f.block = true;
            }
            self.frames.push(f);
            return true;
        }
        let top = self.inline_only == 0;
        match tag {
            "br" if !top => self.space = true,
            "br" if !self.line.is_empty() => self.breaks = (self.breaks + 1).min(2),
            "hr" if top => {
                self.block(&mut f);
                self.begin_block();
                self.out.push_str("---");
            }
            "img" => self.image(node),
            "code" | "kbd" | "samp" | "tt" => self.start_code(&mut f, false, ""),
            "pre" if top => {
                self.block(&mut f);
                self.start_code(&mut f, true, code_lang(node));
            }
            "pre" => {
                self.block(&mut f); // only inline content allowed: a space
                self.start_code(&mut f, false, "");
            }
            "strong" | "b" => f.mark = self.push_mark(Mark::Strong),
            "em" | "i" => f.mark = self.push_mark(Mark::Emphasis),
            "del" | "s" | "strike" => f.mark = self.push_mark(Mark::Strike),
            "a" if node.attr("href").is_some_and(|h| !h.trim().is_empty()) => {
                f.mark = self.push_mark(Mark::Link(node));
            }
            "dt" => {
                self.block(&mut f);
                f.mark = self.push_mark(Mark::Strong);
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" if top => {
                self.block(&mut f);
                self.heading = tag.as_bytes().get(1).map_or(1, |b| b.saturating_sub(b'0'));
                self.inline_only += 1;
                f.heading = true;
                f.inline = true;
            }
            "ul" | "ol" | "menu" if top => {
                self.flush();
                // A list nested in an item follows the item's text tightly.
                let nested = matches!(self.prefixes.last(), Some(Prefix::Item { .. }));
                self.sep(if nested { 1 } else { 2 });
                f.block = true;
                if self.prefixes.len() < MAX_NEST {
                    // CommonMark allows at most nine digits.
                    let start = node
                        .attr("start")
                        .and_then(|s| s.trim().parse::<i64>().ok());
                    self.lists.push(List {
                        ordered: tag == "ol",
                        next: start.map_or(1, |n| n.clamp(0, 999_999_999) as u32),
                        written: false,
                    });
                    f.list = true;
                }
            }
            "li" if top => {
                self.flush();
                let list = self.frames.last().filter(|p| p.list);
                let list = list.and_then(|_| self.lists.len().checked_sub(1));
                // Items of one list are tight: a single newline between them.
                if list
                    .and_then(|i| self.lists.get(i))
                    .is_some_and(|l| l.written)
                {
                    self.force_sep(1);
                } else {
                    self.sep(1);
                }
                f.block = true;
                if self.prefixes.len() < MAX_NEST {
                    self.prefixes.push(Prefix::Item { list, width: 0 });
                    f.prefix = true;
                } else {
                    self.sep(2);
                }
            }
            "blockquote" if top => {
                self.block(&mut f);
                if self.prefixes.len() < MAX_NEST {
                    self.prefixes.push(Prefix::Quote(false));
                    f.prefix = true;
                }
            }
            "table" if top && self.table.is_none() => {
                self.block(&mut f);
                self.table = Some(Table::default());
                f.role = Role::Table;
            }
            "tr" | "td" | "th" if top && self.table.is_some() => {
                self.flush(); // stray text inside a table goes before it
                if let Some(t) = &mut self.table {
                    if tag == "tr" {
                        t.end_row();
                        t.head = node.parent().and_then(|p| p.tag_name()) == Some("thead");
                        f.role = Role::Row;
                    } else {
                        t.head |= tag == "th";
                        self.inline_only += 1;
                        f.role = Role::Cell;
                        f.inline = true;
                    }
                }
            }
            "td" | "th" => {
                self.space = true;
                f.space = true;
            }
            _ if display != Display::Inline => self.block(&mut f),
            _ => {}
        }
        self.frames.push(f);
        true
    }

    fn close(&mut self) {
        let Some(f) = self.frames.pop() else {
            return;
        };
        if f.code {
            self.end_code();
        } else if let Some(code) = &mut self.code {
            if f.block {
                code.newline();
            }
            return;
        }
        if f.mark {
            self.pop_mark();
        }
        match f.role {
            Role::Cell => {
                let cell = self.take_cell();
                if let Some(t) = &mut self.table {
                    t.cells.push(cell);
                }
            }
            Role::Row => {
                if let Some(t) = &mut self.table {
                    t.end_row();
                }
            }
            Role::Table => {
                self.flush();
                if let Some(t) = self.table.take() {
                    self.emit_table(t);
                }
            }
            Role::None => {}
        }
        if f.block {
            self.flush();
        }
        if f.heading {
            self.heading = 0;
        }
        if f.inline {
            self.inline_only = self.inline_only.saturating_sub(1);
        }
        if f.prefix {
            self.prefixes.pop();
        }
        if f.list {
            self.lists.pop();
        }
        if f.block {
            self.sep(2);
        }
        if f.space {
            self.space = true;
        }
    }

    /// A block-level element: its content becomes its own paragraph, or is just
    /// set off by a space where only inline content is allowed.
    fn block(&mut self, f: &mut Frame) {
        if self.inline_only > 0 {
            self.space = true;
            f.space = true;
        } else {
            self.flush();
            self.sep(2);
            f.block = true;
        }
    }

    // --- inline content ---------------------------------------------------

    fn text(&mut self, text: &str) {
        if let Some(code) = &mut self.code {
            code.push(text);
            return;
        }
        for (i, word) in text.split(is_space).enumerate() {
            if i > 0 {
                self.space = true;
            }
            if !word.is_empty() {
                self.inline();
                let line_start =
                    self.inline_only == 0 && (self.line.is_empty() || self.line.ends_with('\n'));
                escape(&mut self.line, word, line_start);
            }
        }
    }

    /// Settle pending whitespace, hard breaks and opening markers before
    /// inline content.
    fn inline(&mut self) {
        if !self.line.is_empty() {
            if self.breaks > 0 {
                for _ in 0..self.breaks {
                    self.line.push_str("\\\n");
                }
            } else if self.space {
                self.line.push(' ');
            }
        }
        self.breaks = 0;
        self.space = false;
        for w in self.wraps.iter_mut().filter(|w| !w.open) {
            // A link already written in an earlier block is not repeated.
            if w.used && matches!(w.mark, Mark::Link(_)) {
                continue;
            }
            w.open = true;
            w.used = true;
            self.line.push_str(match w.mark {
                Mark::Strong => "**",
                Mark::Emphasis => "*",
                Mark::Strike => "~~",
                Mark::Link(_) => "[",
            });
        }
    }

    fn push_mark(&mut self, mark: Mark<'a>) -> bool {
        // A nested duplicate (`<b><b>`, a link in a link) would only add noise.
        let kind = mem::discriminant(&mark);
        if self
            .wraps
            .iter()
            .any(|w| mem::discriminant(&w.mark) == kind)
        {
            return false;
        }
        self.wraps.push(Wrap {
            mark,
            open: false,
            used: false,
            spaced: self.space,
        });
        true
    }

    fn pop_mark(&mut self) {
        if let Some(Wrap {
            mark: Mark::Link(a),
            used: false,
            spaced,
            ..
        }) = self.wraps.last().copied()
        {
            // A link with no visible text: use its URL as the text. Whitespace
            // that was inside it goes after it.
            let after = !spaced && mem::take(&mut self.space);
            self.text(a.attr("href").unwrap_or("").trim());
            self.space |= after;
        }
        if let Some(w) = self.wraps.pop() {
            if w.open {
                push_close(&mut self.line, w.mark);
            }
        }
    }

    fn image(&mut self, img: NodeRef<'_>) {
        let alt = img.attr("alt").unwrap_or("");
        let src = img.attr("src").unwrap_or("").trim();
        if src.is_empty() {
            self.text(alt);
            return;
        }
        self.inline();
        self.line.push_str("![");
        for (i, word) in alt.split(is_space).filter(|w| !w.is_empty()).enumerate() {
            if i > 0 {
                self.line.push(' ');
            }
            escape(&mut self.line, word, false);
        }
        self.line.push_str("](");
        push_destination(&mut self.line, src);
        push_title(&mut self.line, img.attr("title"));
        self.line.push(')');
    }

    fn start_code(&mut self, f: &mut Frame, block: bool, lang: &'a str) {
        self.code = Some(Code {
            buf: String::new(),
            block,
            fresh: block,
            lang,
        });
        f.code = true;
    }

    fn end_code(&mut self) {
        let Some(code) = self.code.take() else {
            return;
        };
        if code.block {
            self.code_block(&code.buf, code.lang);
            return;
        }
        let mut text = String::new();
        for word in code.buf.split(is_space).filter(|w| !w.is_empty()) {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(word);
        }
        if text.is_empty() {
            return;
        }
        self.inline();
        // The fence must be longer than any backtick run inside.
        let fence = longest_run(&text, '`') + 1;
        let pad = text.starts_with('`') || text.ends_with('`');
        push_n(&mut self.line, '`', fence);
        if pad {
            self.line.push(' ');
        }
        self.line.push_str(&text);
        if pad {
            self.line.push(' ');
        }
        push_n(&mut self.line, '`', fence);
    }

    // --- blocks -----------------------------------------------------------

    /// End the current paragraph: close its open markers and write it out.
    fn flush(&mut self) {
        self.finish_line();
        if self.line.is_empty() {
            return;
        }
        let line = mem::take(&mut self.line);
        self.begin_block();
        if self.heading > 0 {
            push_n(&mut self.out, '#', usize::from(self.heading));
            self.out.push(' ');
        }
        for (i, part) in line.split('\n').enumerate() {
            if i > 0 {
                self.next_line(false);
            }
            self.out.push_str(part);
        }
        self.line = line;
        self.line.clear();
    }

    /// Close the markers open in the current line (they reopen lazily if the
    /// element continues into the next block) and drop pending whitespace.
    fn finish_line(&mut self) {
        for w in self.wraps.iter_mut().rev().filter(|w| w.open) {
            w.open = false;
            push_close(&mut self.line, w.mark);
        }
        self.space = false;
        self.breaks = 0;
    }

    /// Start a block's first line: the owed separation, then the prefixes.
    fn begin_block(&mut self) {
        if !self.out.is_empty() {
            if self.sep >= 2 {
                self.next_line(true);
            }
            self.out.push('\n');
        }
        self.sep = 0;
        self.write_prefix(false);
    }

    fn next_line(&mut self, blank: bool) {
        self.out.push('\n');
        self.write_prefix(blank);
    }

    /// Write the container prefixes for a new line. A blank line stops at the
    /// first container that has not written a line yet (the blank line comes
    /// before it), and carries no trailing spaces.
    fn write_prefix(&mut self, blank: bool) {
        let Markdown {
            out,
            prefixes,
            lists,
            ..
        } = self;
        for prefix in prefixes.iter_mut() {
            match prefix {
                Prefix::Quote(started) => {
                    if !*started {
                        if blank {
                            break;
                        }
                        *started = true;
                    }
                    out.push_str("> ");
                }
                Prefix::Item { width, .. } if *width > 0 => push_n(out, ' ', *width),
                Prefix::Item { .. } if blank => break,
                Prefix::Item { list, width } => {
                    let at = out.len();
                    match list.and_then(|i| lists.get_mut(i)) {
                        Some(l) if l.ordered => {
                            let _ = write!(out, "{}. ", l.next);
                            l.next = l.next.saturating_add(1).min(999_999_999);
                            l.written = true;
                        }
                        Some(l) => {
                            out.push_str("- ");
                            l.written = true;
                        }
                        None => out.push_str("- "),
                    }
                    *width = out.len() - at;
                }
            }
        }
        if blank {
            let end = out.trim_end_matches(' ').len();
            out.truncate(end);
        }
    }

    /// Whether the innermost container has written a line. Until it has, the
    /// separation requested when it opened stands; requests from inside it are
    /// ignored (they would land before its marker).
    fn started(&self) -> bool {
        match self.prefixes.last() {
            Some(Prefix::Quote(started)) => *started,
            Some(Prefix::Item { width, .. }) => *width > 0,
            None => true,
        }
    }

    fn sep(&mut self, n: u8) {
        if self.started() {
            self.sep = self.sep.max(n);
        }
    }

    fn force_sep(&mut self, n: u8) {
        if self.started() {
            self.sep = n;
        }
    }

    fn code_block(&mut self, raw: &str, lang: &str) {
        let body = raw.trim_end();
        if body.trim_start().is_empty() {
            return;
        }
        let fence = longest_run(body, '`').max(2) + 1;
        self.begin_block();
        push_n(&mut self.out, '`', fence);
        self.out.push_str(lang);
        for line in body.split('\n') {
            self.next_line(line.is_empty());
            self.out.push_str(line);
        }
        self.next_line(false);
        push_n(&mut self.out, '`', fence);
    }

    fn take_cell(&mut self) -> String {
        self.finish_line();
        let cell = self.line.trim().replace('|', "\\|");
        self.line.clear();
        cell
    }

    fn emit_table(&mut self, mut table: Table) {
        table.end_row();
        let cols = table.rows.iter().map(|(_, r)| r.len()).max().unwrap_or(0);
        if cols == 0 {
            return;
        }
        // GFM needs a header: the first `<th>`/`<thead>` row, else the first row.
        let head = table.rows.iter().position(|(h, _)| *h).unwrap_or(0);
        let pad = if cols <= MAX_PAD { cols } else { 0 };
        self.sep(2);
        self.begin_block();
        if let Some((_, cells)) = table.rows.get(head) {
            push_row(&mut self.out, cells, cols);
        }
        self.next_line(false);
        self.out.push('|');
        for _ in 0..cols {
            self.out.push_str(" --- |");
        }
        for (i, (_, cells)) in table.rows.iter().enumerate() {
            if i != head {
                self.next_line(false);
                push_row(&mut self.out, cells, pad);
            }
        }
    }

    fn finish(mut self) -> String {
        self.flush();
        let end = self.out.trim_end().len();
        self.out.truncate(end);
        self.out
    }
}

impl Code<'_> {
    fn push(&mut self, text: &str) {
        let text = if mem::take(&mut self.fresh) {
            skip_newline(text)
        } else {
            text
        };
        push_verbatim(&mut self.buf, text);
    }

    fn br(&mut self) {
        self.fresh = false;
        self.buf.push(if self.block { '\n' } else { ' ' });
    }

    /// A block boundary inside code: make sure the next text starts a line.
    fn newline(&mut self) {
        if !self.block {
            self.buf.push(' ');
        } else if !self.buf.is_empty() && !self.buf.ends_with('\n') {
            self.buf.push('\n');
        }
    }
}

impl Table {
    fn end_row(&mut self) {
        if !self.cells.is_empty() {
            self.rows.push((self.head, mem::take(&mut self.cells)));
        }
        self.head = false;
    }
}

/// The language of a code block, from `class="language-x"` (or `lang-x`) on
/// the `<pre>` or on its `<code>` child.
fn code_lang<'a>(pre: NodeRef<'a>) -> &'a str {
    let lang = |n: NodeRef<'a>| {
        n.classes().find_map(|c| {
            c.strip_prefix("language-")
                .or_else(|| c.strip_prefix("lang-"))
        })
    };
    lang(pre)
        .or_else(|| {
            pre.child_elements()
                .find(|c| c.tag_name() == Some("code"))
                .and_then(lang)
        })
        .filter(|l| !l.contains('`'))
        .unwrap_or("")
}

/// Append one word (no whitespace) of text, escaping what Markdown would
/// otherwise interpret. `line_start` adds the checks for block syntax (`#`,
/// `>`, list markers, …) that only matters at the start of a line.
fn escape(out: &mut String, word: &str, line_start: bool) {
    let bytes = word.as_bytes();
    let first = bytes.first().copied().unwrap_or(0);
    let all = |b: u8| bytes.iter().all(|&x| x == b);
    // Byte offset of a character that needs escaping because of where it is.
    let at = if all(b'#') {
        Some(0) // an ATX heading marker or closing sequence
    } else if !line_start {
        None
    } else if first == b'>' || (matches!(first, b'-' | b'+' | b'=') && all(first)) {
        Some(0) // block quote, bullet, thematic break or setext underline
    } else {
        // An ordered-list marker: up to nine digits, then `.` or `)`.
        let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
        let marker = (1..=9).contains(&digits)
            && bytes.len() == digits + 1
            && matches!(bytes.get(digits), Some(b'.' | b')'));
        marker.then_some(digits)
    };
    let mut prev = None;
    let mut chars = word.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let next = chars.peek().map(|&(_, n)| n);
        let escaped = match c {
            '\\' | '*' | '`' | '[' | ']' | '~' => true,
            // `_` inside a word cannot open or close emphasis.
            '_' => {
                !(prev.is_some_and(char::is_alphanumeric)
                    && next.is_some_and(char::is_alphanumeric))
            }
            // Could open an HTML tag, comment or autolink.
            '<' => next.is_some_and(|n| n.is_ascii_alphabetic() || matches!(n, '/' | '!' | '?')),
            // Could open a character reference.
            '&' => next.is_some_and(|n| n.is_ascii_alphanumeric() || n == '#'),
            _ => Some(i) == at,
        };
        if escaped {
            out.push('\\');
        }
        out.push(c);
        prev = Some(c);
    }
}

fn push_close(out: &mut String, mark: Mark<'_>) {
    match mark {
        Mark::Strong => out.push_str("**"),
        Mark::Emphasis => out.push('*'),
        Mark::Strike => out.push_str("~~"),
        Mark::Link(a) => {
            out.push_str("](");
            push_destination(out, a.attr("href").unwrap_or("").trim());
            push_title(out, a.attr("title"));
            out.push(')');
        }
    }
}

/// A link destination, wrapped in `<…>` when it holds spaces, parentheses or
/// angle brackets. Line breaks and tabs (which browsers strip from URLs) are
/// dropped.
fn push_destination(out: &mut String, url: &str) {
    let pointy = url.chars().any(|c| {
        matches!(c, ' ' | '(' | ')' | '<' | '>')
            || (c.is_ascii_control() && !matches!(c, '\n' | '\r' | '\t'))
    });
    if pointy {
        out.push('<');
    }
    for c in url.chars() {
        match c {
            '\n' | '\r' | '\t' => {}
            '\\' => out.push_str("\\\\"),
            '<' | '>' => {
                out.push('\\');
                out.push(c);
            }
            c => out.push(c),
        }
    }
    if pointy {
        out.push('>');
    }
}

fn push_title(out: &mut String, title: Option<&str>) {
    let Some(title) = title.map(str::trim).filter(|t| !t.is_empty()) else {
        return;
    };
    out.push_str(" \"");
    for c in title.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            c if is_space(c) => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// A GFM table row with at least `width` cells (missing ones left empty).
fn push_row(out: &mut String, cells: &[String], width: usize) {
    out.push('|');
    for i in 0..width.max(cells.len()) {
        match cells.get(i).filter(|c| !c.is_empty()) {
            Some(cell) => {
                out.push(' ');
                out.push_str(cell);
                out.push_str(" |");
            }
            None => out.push_str(" |"),
        }
    }
}

fn longest_run(s: &str, ch: char) -> usize {
    let (mut best, mut run) = (0, 0);
    for c in s.chars() {
        run = if c == ch { run + 1 } else { 0 };
        best = best.max(run);
    }
    best
}

fn push_n(out: &mut String, ch: char, n: usize) {
    for _ in 0..n {
        out.push(ch);
    }
}
