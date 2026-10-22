//! Turning byte offsets into human positions.

/// The 1-based `(line, column)` of byte offset `byte` in `src`, for error
/// messages, linters and editors — pair it with
/// [`NodeRef::source_range`](crate::NodeRef::source_range) or
/// [`ParseError::position`](crate::ParseError::position).
///
/// Lines end at `\n`, `\r\n` (one break) or a lone `\r`. Columns count
/// `char`s (Unicode scalar values), not bytes. Never panics: an offset past
/// the end is clamped to the end, and one inside a multi-byte character to
/// that character's start. Runs in O(`byte`); for many lookups into one large
/// input, precompute line starts instead.
///
/// ```
/// let src = "<ul>\r\n  <li>café\n  <li>x</ul>";
/// let dom = domq::parse(src);
/// let second = dom.find_by_tag("li").nth(1).unwrap();
/// let start = second.source_range().unwrap().start;
/// assert_eq!(domq::line_col(src, start), (3, 3));
/// assert_eq!(domq::line_col(src, 0), (1, 1));
///
/// let after_e = src.find('é').unwrap() + 'é'.len_utf8();
/// assert_eq!(domq::line_col(src, after_e), (2, 11)); // `é`: 2 bytes, 1 column
/// assert_eq!(domq::line_col(src, after_e - 1), (2, 10)); // mid-char: its start
/// assert_eq!(domq::line_col(src, usize::MAX), (3, 13)); // clamped to the end
/// ```
pub fn line_col(src: &str, byte: usize) -> (usize, usize) {
    let mut end = byte.min(src.len());
    while !src.is_char_boundary(end) {
        end -= 1;
    }
    let bytes = src.as_bytes();
    let mut line = 1;
    let mut line_start = 0;
    for (i, &b) in bytes.iter().take(end).enumerate() {
        // A `\r` directly followed by `\n` is part of one `\r\n` break.
        let breaks = b == b'\n' || (b == b'\r' && bytes.get(i + 1) != Some(&b'\n'));
        if breaks {
            line += 1;
            line_start = i + 1;
        }
    }
    let column = src.get(line_start..end).map_or(0, |s| s.chars().count()) + 1;
    (line, column)
}
