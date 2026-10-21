//! Static, allocation-free tag classification tables.
//!
//! Everything here is a `const`/`match` over ASCII-lowercased tag names. These
//! tables drive the tokenizer's raw-text handling and the tree builder's
//! void-element and implicit-close behaviour. Names are compared
//! case-insensitively by the callers, which lower-case before lookup.

/// Elements that never have children and never take an end tag.
/// (The HTML "void elements".)
pub(crate) fn is_void(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

/// Elements whose content is raw text: `<` does not start a tag inside them,
/// and entities are *not* decoded. Scanning runs until the matching end tag.
pub(crate) fn is_raw_text(name: &str) -> bool {
    matches!(name, "script" | "style")
}

/// Elements the tokenizer reads as text up to their end tag: the raw-text
/// ones plus `title` and `textarea` (whose text is still entity-decoded).
/// Whatever is serialized inside one must not contain that end tag.
pub(crate) fn ends_only_at_end_tag(name: &str) -> bool {
    is_raw_text(name) || matches!(name, "title" | "textarea")
}

/// Whether `name` can be written out as an attribute name as it is: not
/// empty, and none of ASCII whitespace, `"`, `'`, `<`, `>`, `/`, `=` or NUL,
/// which would end the name early or start new markup.
pub(crate) fn is_valid_attr_name(name: &str) -> bool {
    !name.is_empty()
        && !name.bytes().any(|b| {
            b.is_ascii_whitespace() || matches!(b, b'"' | b'\'' | b'<' | b'>' | b'/' | b'=' | 0)
        })
}

/// Whether `name` can be written out as a tag name as it is: a valid
/// attribute name that starts with an ASCII letter. (After `<` anything else
/// is text, and the element's attribute values would be read as markup.)
pub(crate) fn is_valid_tag_name(name: &str) -> bool {
    name.as_bytes().first().is_some_and(u8::is_ascii_alphabetic) && is_valid_attr_name(name)
}

/// Start tags that implicitly close an open `<p>`: every element the WHATWG
/// "optional tags" rules let a `</p>` be omitted before, plus the other start
/// tags tree construction closes a `<p>` for (`center`, `dir`, `li`, `dd`,
/// `dt`, `summary`, `listing`, `plaintext`, `xmp`).
fn closes_p(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "center"
            | "dd"
            | "details"
            | "dialog"
            | "dir"
            | "div"
            | "dl"
            | "dt"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "hgroup"
            | "hr"
            | "li"
            | "listing"
            | "main"
            | "menu"
            | "nav"
            | "ol"
            | "p"
            | "plaintext"
            | "pre"
            | "search"
            | "section"
            | "summary"
            | "table"
            | "ul"
            | "xmp"
    )
}

fn is_heading(name: &str) -> bool {
    matches!(name, "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
}

/// Metadata start tags that stay inside an open `<head>`; any other start tag
/// (`<body>`, `<div>`, …) means the `</head>` was omitted.
fn in_head(name: &str) -> bool {
    matches!(
        name,
        "base"
            | "basefont"
            | "bgsound"
            | "link"
            | "meta"
            | "noframes"
            | "noscript"
            | "script"
            | "style"
            | "template"
            | "title"
    )
}

/// Table-structure start tags that end an open cell, row, row group or
/// caption (the WHATWG "in cell" / "in row" / "in table body" / "in caption"
/// rules, minus the scope checks).
fn table_structure(name: &str) -> bool {
    matches!(
        name,
        "caption" | "col" | "colgroup" | "tbody" | "tfoot" | "thead"
    )
}

/// Given a start tag `opening` and the tag name `open` currently on top of the
/// open-element stack, decide whether `open` should be implicitly closed first.
///
/// These are the WHATWG "optional tags" rules (13.1.2.4) — an end tag may be
/// omitted when the element is immediately followed by one of these start
/// tags — together with the tree-construction cases that act on the current
/// node: `<li>`/`<dt>`/`<dd>`/headings closing `<p>`, a heading closing an
/// open heading, `<colgroup>` ending at the first non-`<col>` content, and
/// table sections, rows, cells and captions closing each other. They only ever
/// look at the *top* of the stack, so the builder needs no insertion modes,
/// scopes or active-formatting list.
pub(crate) fn implicitly_closes(opening: &str, open: &str) -> bool {
    match open {
        "p" => closes_p(opening),
        "li" => opening == "li",
        "dt" | "dd" => matches!(opening, "dt" | "dd"),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => is_heading(opening),
        "head" => !in_head(opening),
        "option" => matches!(opening, "option" | "optgroup" | "hr"),
        "optgroup" => matches!(opening, "optgroup" | "hr"),
        "rb" | "rt" | "rp" => matches!(opening, "rb" | "rt" | "rtc" | "rp"),
        "rtc" => matches!(opening, "rb" | "rtc"),
        "td" | "th" => matches!(opening, "td" | "th" | "tr") || table_structure(opening),
        "tr" => opening == "tr" || table_structure(opening),
        "thead" | "tbody" | "tfoot" => table_structure(opening),
        "caption" => matches!(opening, "td" | "th" | "tr") || table_structure(opening),
        "colgroup" => !matches!(opening, "col" | "template"),
        _ => false,
    }
}

/// Whether non-whitespace text implicitly closes the open element `open`:
/// text never belongs in a `<head>` or a `<colgroup>`.
/// Elements an ancestor's end tag may close without a parse error, because
/// their own end tag is optional (WHATWG "generate implied end tags
/// thoroughly", plus `html`/`head`/`body`): `</ul>` closing an open `<li>` is
/// valid HTML.
pub(crate) fn end_tag_optional(name: &str) -> bool {
    matches!(
        name,
        "body"
            | "caption"
            | "colgroup"
            | "dd"
            | "dt"
            | "head"
            | "html"
            | "li"
            | "optgroup"
            | "option"
            | "p"
            | "rb"
            | "rp"
            | "rt"
            | "rtc"
            | "tbody"
            | "td"
            | "tfoot"
            | "th"
            | "thead"
            | "tr"
    )
}

pub(crate) fn text_closes(open: &str) -> bool {
    matches!(open, "head" | "colgroup")
}

/// How the readable-output serializers (`to_text`, `to_markdown`) lay an
/// element out: a rough stand-in for the CSS `display` of the browser's default
/// stylesheet. Unknown and custom elements are inline.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Display {
    /// Never rendered: metadata, scripts, templates, embedded objects.
    Hidden,
    /// Flows with the surrounding text.
    Inline,
    /// Starts on a new line.
    Block,
    /// Separated from its neighbours by a blank line.
    Paragraph,
}

/// The [`Display`] of an element, by lower-cased tag name.
pub(crate) fn display(name: &str) -> Display {
    match name {
        "head" | "title" | "script" | "style" | "template" | "noscript" | "noembed" | "iframe"
        | "object" | "svg" | "datalist" => Display::Hidden,
        "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "blockquote" | "pre" | "ul" | "ol"
        | "menu" | "dl" | "table" | "hr" => Display::Paragraph,
        "address" | "article" | "aside" | "body" | "caption" | "center" | "dd" | "details"
        | "dialog" | "div" | "dt" | "fieldset" | "figcaption" | "figure" | "footer" | "form"
        | "header" | "hgroup" | "html" | "legend" | "li" | "main" | "nav" | "optgroup"
        | "option" | "search" | "section" | "summary" | "tbody" | "textarea" | "tfoot"
        | "thead" | "tr" => Display::Block,
        _ => Display::Inline,
    }
}
