//! Zero-dependency URL reference resolution (RFC 3986 §5.2) and `srcset`
//! parsing — enough to turn the relative links of a page into absolute ones
//! without pulling in a URL crate.
//!
//! This is *reference resolution*, not a URL parser: it does not validate,
//! percent-encode or normalize hosts. Like a browser, it ignores leading and
//! trailing whitespace (and control characters) and drops tabs and line breaks
//! inside a URL.

use alloc::string::String;
use alloc::vec::Vec;

/// Resolve `reference` against `base`, following RFC 3986 §5.2 (the algorithm
/// browsers use for `href` and `src`).
///
/// Leading and trailing whitespace is ignored and tabs and line breaks inside
/// either URL are dropped, as in a browser. If `base` has no scheme (it is
/// itself relative, or empty) there is nothing to anchor to, and the cleaned-up
/// `reference` is returned unchanged. An absolute `reference` (one with a
/// scheme, such as `mailto:` or `javascript:`) is returned as is, apart from
/// removing `.`/`..` path segments.
///
/// ```
/// use domtree::resolve_url;
///
/// let base = "https://example.com/blog/post.html?x=1";
/// assert_eq!(resolve_url(base, "../img/a.png"), "https://example.com/img/a.png");
/// assert_eq!(resolve_url(base, "/about"), "https://example.com/about");
/// assert_eq!(resolve_url(base, "#top"), "https://example.com/blog/post.html?x=1#top");
/// assert_eq!(resolve_url(base, "//cdn.example.com/x.js"), "https://cdn.example.com/x.js");
/// assert_eq!(resolve_url(base, " mailto:hi@example.com "), "mailto:hi@example.com");
/// // A relative base cannot anchor anything:
/// assert_eq!(resolve_url("/docs/", "a.html"), "a.html");
/// ```
pub fn resolve_url(base: &str, reference: &str) -> String {
    let base = clean(base);
    let reference = clean(reference);
    let b = Parts::split(&base);
    if b.scheme.is_none() {
        return reference;
    }
    let r = Parts::split(&reference);

    let mut t = Parts {
        scheme: b.scheme,
        authority: b.authority,
        path: String::new(),
        query: r.query,
        fragment: r.fragment,
    };
    if r.scheme.is_some() {
        t.scheme = r.scheme;
        t.authority = r.authority;
        t.path = remove_dot_segments(&r.path);
    } else if r.authority.is_some() {
        t.authority = r.authority;
        t.path = remove_dot_segments(&r.path);
    } else if r.path.is_empty() {
        t.path = b.path.clone();
        t.query = r.query.or(b.query);
    } else if r.path.starts_with('/') {
        t.path = remove_dot_segments(&r.path);
    } else {
        let mut merged = String::new();
        if b.authority.is_some() && b.path.is_empty() {
            merged.push('/');
        } else if let Some(dir) = b.path.rfind('/').and_then(|i| b.path.get(..=i)) {
            merged.push_str(dir);
        }
        merged.push_str(&r.path);
        t.path = remove_dot_segments(&merged);
    }
    t.join()
}

/// Resolve every URL in a `srcset` attribute value (`"a.png 1x, b.png 2x"`)
/// against `base` with [`resolve_url`], keeping each candidate's descriptor.
///
/// Candidates are split as the HTML standard's `srcset` parser does: a URL is
/// a run of non-whitespace (so URLs containing commas survive), trailing commas
/// end a candidate, descriptors run to the next comma outside parentheses, and
/// empty candidates are dropped. The result is normalized to
/// `"url descriptor, url descriptor"`.
///
/// ```
/// use domtree::resolve_srcset;
///
/// assert_eq!(
///     resolve_srcset("https://x.dev/p/", "a.png 1x, /b.png 2x"),
///     "https://x.dev/p/a.png 1x, https://x.dev/b.png 2x",
/// );
/// // Commas inside URLs are not separators:
/// assert_eq!(
///     resolve_srcset("https://x.dev/", "/c_fill,w_300/a.jpg 300w,/c_fill,w_600/a.jpg 600w"),
///     "https://x.dev/c_fill,w_300/a.jpg 300w, https://x.dev/c_fill,w_600/a.jpg 600w",
/// );
/// ```
pub fn resolve_srcset(base: &str, srcset: &str) -> String {
    let mut out = String::new();
    for (url, descriptor) in srcset_candidates(srcset) {
        if !out.is_empty() {
            out.push_str(", ");
        }
        out.push_str(&resolve_url(base, url));
        if !descriptor.is_empty() {
            out.push(' ');
            out.push_str(descriptor);
        }
    }
    out
}

/// The `(url, descriptor)` candidates of a `srcset` value, per the HTML
/// standard's parsing algorithm (see [`resolve_srcset`]). The descriptor is
/// trimmed and may be empty.
pub(crate) fn srcset_candidates(srcset: &str) -> Srcset<'_> {
    Srcset { rest: srcset }
}

/// Iterator over `srcset` candidates; see [`srcset_candidates`].
pub(crate) struct Srcset<'a> {
    rest: &'a str,
}

impl<'a> Iterator for Srcset<'a> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<(&'a str, &'a str)> {
        let rest = self
            .rest
            .trim_start_matches(|c: char| is_ascii_space(c) || c == ',');
        if rest.is_empty() {
            self.rest = "";
            return None;
        }
        // Non-empty and not starting with whitespace or a comma, so the URL is
        // at least one character long: every call makes progress.
        let end = rest.find(is_ascii_space).unwrap_or(rest.len());
        let (url, after) = rest.split_at_checked(end).unwrap_or((rest, ""));
        if url.ends_with(',') {
            self.rest = after;
            return Some((url.trim_end_matches(','), ""));
        }
        let mut depth = 0u32;
        let mut stop = after.len();
        for (i, c) in after.char_indices() {
            match c {
                '(' => depth = depth.saturating_add(1),
                ')' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => {
                    stop = i;
                    break;
                }
                _ => {}
            }
        }
        let (descriptor, tail) = after.split_at_checked(stop).unwrap_or((after, ""));
        self.rest = tail.strip_prefix(',').unwrap_or(tail);
        Some((url, descriptor.trim_matches(is_ascii_space)))
    }
}

fn is_ascii_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0C')
}

/// Whether a URL is a `javascript:` URL (scheme compared ASCII
/// case-insensitively, after the clean-up browsers apply).
pub(crate) fn is_javascript(url: &str) -> bool {
    let url = clean(url);
    url.get(..11)
        .is_some_and(|s| s.eq_ignore_ascii_case("javascript:"))
}

/// Strip leading/trailing C0 controls and spaces, and remove ASCII tabs and
/// newlines anywhere (the WHATWG URL parser's pre-processing).
pub(crate) fn clean(url: &str) -> String {
    url.trim_matches(|c: char| c <= ' ')
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect()
}

struct Parts<'a> {
    scheme: Option<&'a str>,
    authority: Option<&'a str>,
    path: String,
    query: Option<&'a str>,
    fragment: Option<&'a str>,
}

impl<'a> Parts<'a> {
    fn split(s: &'a str) -> Parts<'a> {
        let (s, fragment) = match s.split_once('#') {
            Some((a, f)) => (a, Some(f)),
            None => (s, None),
        };
        let (s, query) = match s.split_once('?') {
            Some((a, q)) => (a, Some(q)),
            None => (s, None),
        };
        let (scheme, rest) = match s.split_once(':') {
            Some((scheme, rest)) if is_scheme(scheme) => (Some(scheme), rest),
            _ => (None, s),
        };
        let (authority, path) = match rest.strip_prefix("//") {
            Some(r) => {
                let end = r.find('/').unwrap_or(r.len());
                let (authority, path) = r.split_at_checked(end).unwrap_or((r, ""));
                (Some(authority), path)
            }
            None => (None, rest),
        };
        Parts {
            scheme,
            authority,
            path: String::from(path),
            query,
            fragment,
        }
    }

    fn join(&self) -> String {
        let mut out = String::new();
        if let Some(s) = self.scheme {
            out.push_str(s);
            out.push(':');
        }
        if let Some(a) = self.authority {
            out.push_str("//");
            out.push_str(a);
        }
        out.push_str(&self.path);
        if let Some(q) = self.query {
            out.push('?');
            out.push_str(q);
        }
        if let Some(f) = self.fragment {
            out.push('#');
            out.push_str(f);
        }
        out
    }
}

/// `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`
fn is_scheme(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// RFC 3986 §5.2.4, segment-wise.
fn remove_dot_segments(path: &str) -> String {
    let (absolute, body) = match path.strip_prefix('/') {
        Some(body) => (true, body),
        None => (false, path),
    };
    let mut out: Vec<&str> = Vec::new();
    let mut segments = body.split('/').peekable();
    while let Some(seg) = segments.next() {
        match seg {
            "." => {}
            ".." => {
                out.pop();
            }
            s => {
                out.push(s);
                continue;
            }
        }
        if segments.peek().is_none() {
            out.push(""); // keep the trailing slash: "a/b/.." -> "a/"
        }
    }
    let mut joined = String::with_capacity(path.len());
    if absolute {
        joined.push('/');
    }
    for (i, seg) in out.iter().enumerate() {
        if i > 0 {
            joined.push('/');
        }
        joined.push_str(seg);
    }
    joined
}
