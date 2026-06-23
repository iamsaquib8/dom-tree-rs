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

/// Elements whose content is "escapable raw text": like raw text (no nested
/// tags) but character references *are* decoded.
pub(crate) fn is_escapable_raw_text(name: &str) -> bool {
    matches!(name, "title" | "textarea")
}

/// Either flavour of raw text — the tokenizer scans both the same way; the
/// difference (entity decoding) is applied later at accessor time.
pub(crate) fn is_rawish(name: &str) -> bool {
    is_raw_text(name) || is_escapable_raw_text(name)
}

/// Block-level start tags that implicitly close an open `<p>`.
fn closes_p(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "details"
            | "div"
            | "dl"
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
            | "main"
            | "menu"
            | "nav"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "table"
            | "ul"
    )
}

/// Given a start tag `opening` and the tag name `open` currently on top of the
/// open-element stack, decide whether `open` should be implicitly closed first.
///
/// This is the deliberately small, high-value subset of the WHATWG "implied end
/// tags" rules — enough to handle the overwhelmingly common real-world cases of
/// omitted end tags (lists, paragraphs, table cells, definition lists, option
/// groups) without the full insertion-mode state machine.
pub(crate) fn implicitly_closes(opening: &str, open: &str) -> bool {
    match open {
        "p" => closes_p(opening),
        "li" => opening == "li",
        "dt" | "dd" => matches!(opening, "dt" | "dd"),
        "option" => matches!(opening, "option" | "optgroup"),
        "optgroup" => opening == "optgroup",
        "td" | "th" => matches!(opening, "td" | "th" | "tr" | "tbody" | "thead" | "tfoot"),
        "tr" => matches!(opening, "tr" | "tbody" | "thead" | "tfoot"),
        "thead" | "tbody" | "tfoot" => matches!(opening, "tbody" | "thead" | "tfoot"),
        _ => false,
    }
}
