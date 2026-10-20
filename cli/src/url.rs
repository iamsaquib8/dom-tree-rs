//! Minimal RFC 3986 reference resolution, for `--base` (no `url` crate).

/// Resolve `reference` against `base` (RFC 3986 §5.2). If `base` has no
/// scheme, `reference` is returned unchanged — there is nothing to anchor it to.
pub fn resolve(base: &str, reference: &str) -> String {
    let reference = reference.trim();
    let b = Parts::split(base);
    if b.scheme.is_none() {
        return reference.to_string();
    }
    let r = Parts::split(reference);

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
        let merged = if b.authority.is_some() && b.path.is_empty() {
            format!("/{}", r.path)
        } else {
            let dir = b.path.rfind('/').map_or("", |i| &b.path[..=i]);
            format!("{dir}{}", r.path)
        };
        t.path = remove_dot_segments(&merged);
    }
    t.join()
}

/// Resolve every URL in a `srcset` value (`"a.png 1x, b.png 2x"`), following
/// the HTML candidate-parsing rules: a URL is a run of non-whitespace (commas
/// inside it are kept, trailing ones stripped), descriptors run to the next
/// top-level comma, and empty candidates are dropped.
pub fn resolve_srcset(base: &str, srcset: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut rest = srcset;
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
        if rest.is_empty() {
            break;
        }
        let url_end = rest
            .find(|c: char| c.is_ascii_whitespace())
            .unwrap_or(rest.len());
        let (raw_url, after) = rest.split_at(url_end);
        let url = raw_url.trim_end_matches(',');
        // A URL that ended in a comma ends its candidate: no descriptors.
        let (descriptors, next) = if url.len() < raw_url.len() {
            ("", after)
        } else {
            split_descriptors(after)
        };
        let resolved = resolve(base, url);
        let descriptors = descriptors.split_whitespace().collect::<Vec<_>>().join(" ");
        out.push(if descriptors.is_empty() {
            resolved
        } else {
            format!("{resolved} {descriptors}")
        });
        rest = next;
    }
    out.join(", ")
}

/// Split descriptors off at the next comma outside parentheses.
fn split_descriptors(s: &str) -> (&str, &str) {
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return (&s[..i], &s[i + 1..]),
            _ => {}
        }
    }
    (s, "")
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
        let (scheme, rest) = match scheme_len(s) {
            Some(n) => (Some(&s[..n]), &s[n + 1..]),
            None => (None, s),
        };
        let (authority, path) = match rest.strip_prefix("//") {
            Some(r) => {
                let end = r.find('/').unwrap_or(r.len());
                (Some(&r[..end]), &r[end..])
            }
            None => (None, rest),
        };
        Parts {
            scheme,
            authority,
            path: path.to_string(),
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

/// Length of a leading `scheme` (before its `:`), if `s` starts with one.
fn scheme_len(s: &str) -> Option<usize> {
    let colon = s.find(':')?;
    let scheme = &s[..colon];
    let mut chars = scheme.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    valid.then_some(colon)
}

fn remove_dot_segments(path: &str) -> String {
    let absolute = path.starts_with('/');
    let body = if absolute { &path[1..] } else { path };
    let segments: Vec<&str> = body.split('/').collect();
    let last = segments.len().saturating_sub(1);
    let mut out: Vec<&str> = Vec::new();
    for (i, seg) in segments.iter().enumerate() {
        match *seg {
            "." => {}
            ".." => {
                out.pop();
            }
            s => {
                out.push(s);
                continue;
            }
        }
        if i == last {
            out.push(""); // keep the trailing slash: "a/b/.." -> "a/"
        }
    }
    let joined = out.join("/");
    if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The normal and abnormal examples from RFC 3986 §5.4.
    #[test]
    fn rfc3986_examples() {
        let base = "http://a/b/c/d;p?q";
        let cases = [
            ("g:h", "g:h"),
            ("g", "http://a/b/c/g"),
            ("./g", "http://a/b/c/g"),
            ("g/", "http://a/b/c/g/"),
            ("/g", "http://a/g"),
            ("//g", "http://g"),
            ("?y", "http://a/b/c/d;p?y"),
            ("g?y", "http://a/b/c/g?y"),
            ("#s", "http://a/b/c/d;p?q#s"),
            ("g#s", "http://a/b/c/g#s"),
            ("g?y#s", "http://a/b/c/g?y#s"),
            (";x", "http://a/b/c/;x"),
            ("g;x", "http://a/b/c/g;x"),
            ("", "http://a/b/c/d;p?q"),
            (".", "http://a/b/c/"),
            ("./", "http://a/b/c/"),
            ("..", "http://a/b/"),
            ("../", "http://a/b/"),
            ("../g", "http://a/b/g"),
            ("../..", "http://a/"),
            ("../../", "http://a/"),
            ("../../g", "http://a/g"),
            ("../../../g", "http://a/g"),
            ("/./g", "http://a/g"),
            ("/../g", "http://a/g"),
            ("g.", "http://a/b/c/g."),
            (".g", "http://a/b/c/.g"),
            ("./../g", "http://a/b/g"),
            ("g/./h", "http://a/b/c/g/h"),
            ("g/../h", "http://a/b/c/h"),
            ("g;x=1/./y", "http://a/b/c/g;x=1/y"),
            ("g;x=1/../y", "http://a/b/c/y"),
        ];
        for (reference, want) in cases {
            assert_eq!(resolve(base, reference), want, "resolving {reference:?}");
        }
    }

    #[test]
    fn host_only_base() {
        assert_eq!(resolve("https://x.dev", "a/b"), "https://x.dev/a/b");
        assert_eq!(resolve("https://x.dev", "/a"), "https://x.dev/a");
    }

    #[test]
    fn schemeless_base_leaves_reference_alone() {
        assert_eq!(resolve("/docs/", "a.html"), "a.html");
    }

    #[test]
    fn srcset() {
        assert_eq!(
            resolve_srcset("https://x.dev/p/", "a.png 1x, /b.png 2x"),
            "https://x.dev/p/a.png 1x, https://x.dev/b.png 2x"
        );
    }

    #[test]
    fn srcset_keeps_commas_inside_urls() {
        assert_eq!(
            resolve_srcset(
                "https://x.dev/",
                "https://c.io/up/c_fill,w_300/a.jpg 300w, https://c.io/up/c_fill,w_600/a.jpg 600w"
            ),
            "https://c.io/up/c_fill,w_300/a.jpg 300w, https://c.io/up/c_fill,w_600/a.jpg 600w"
        );
    }

    #[test]
    fn srcset_edge_cases() {
        let base = "https://x.dev/p/";
        assert_eq!(
            resolve_srcset(base, "a.png 1x,"),
            "https://x.dev/p/a.png 1x"
        );
        assert_eq!(
            resolve_srcset(base, " ,, a.png, b.png 2x"),
            "https://x.dev/p/a.png, https://x.dev/p/b.png 2x"
        );
        // Per the HTML spec a URL is a run of non-whitespace, so this is one URL.
        assert_eq!(
            resolve_srcset(base, "a.png,b.png 2x"),
            "https://x.dev/p/a.png,b.png 2x"
        );
        assert_eq!(resolve_srcset(base, ""), "");
        assert_eq!(
            resolve_srcset(base, "a.png  100w ,b.png"),
            "https://x.dev/p/a.png 100w, https://x.dev/p/b.png"
        );
    }
}
