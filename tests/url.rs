//! URL reference resolution (`resolve_url`, RFC 3986 §5) and `srcset`
//! handling (`resolve_srcset`).

use domtree::{resolve_srcset, resolve_url};

/// The normal and abnormal examples from RFC 3986 §5.4.
#[test]
fn rfc3986_examples() {
    let base = "http://a/b/c/d;p?q";
    let cases = [
        // §5.4.1 normal examples
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
        ("g;x?y#s", "http://a/b/c/g;x?y#s"),
        ("", "http://a/b/c/d;p?q"),
        (".", "http://a/b/c/"),
        ("./", "http://a/b/c/"),
        ("..", "http://a/b/"),
        ("../", "http://a/b/"),
        ("../g", "http://a/b/g"),
        ("../..", "http://a/"),
        ("../../", "http://a/"),
        ("../../g", "http://a/g"),
        // §5.4.2 abnormal examples
        ("../../../g", "http://a/g"),
        ("../../../../g", "http://a/g"),
        ("/./g", "http://a/g"),
        ("/../g", "http://a/g"),
        ("g.", "http://a/b/c/g."),
        (".g", "http://a/b/c/.g"),
        ("g..", "http://a/b/c/g.."),
        ("..g", "http://a/b/c/..g"),
        ("./../g", "http://a/b/g"),
        ("./g/.", "http://a/b/c/g/"),
        ("g/./h", "http://a/b/c/g/h"),
        ("g/../h", "http://a/b/c/h"),
        ("g;x=1/./y", "http://a/b/c/g;x=1/y"),
        ("g;x=1/../y", "http://a/b/c/y"),
        ("g?y/./x", "http://a/b/c/g?y/./x"),
        ("g?y/../x", "http://a/b/c/g?y/../x"),
        ("g#s/./x", "http://a/b/c/g#s/./x"),
        ("g#s/../x", "http://a/b/c/g#s/../x"),
        ("http:g", "http:g"), // strict parsers
    ];
    for (reference, want) in cases {
        assert_eq!(
            resolve_url(base, reference),
            want,
            "resolving {reference:?}"
        );
    }
}

#[test]
fn host_only_base() {
    assert_eq!(resolve_url("https://x.dev", "a/b"), "https://x.dev/a/b");
    assert_eq!(resolve_url("https://x.dev", "/a"), "https://x.dev/a");
    assert_eq!(resolve_url("https://x.dev", "?q"), "https://x.dev?q");
}

#[test]
fn schemeless_base_leaves_reference_alone() {
    assert_eq!(resolve_url("/docs/", "a.html"), "a.html");
    assert_eq!(resolve_url("", " ../x "), "../x");
}

#[test]
fn whitespace_is_cleaned_like_a_browser() {
    let base = "https://x.dev/a/";
    assert_eq!(resolve_url(base, "  b.html\n"), "https://x.dev/a/b.html");
    // Tabs and line breaks inside a URL are dropped.
    assert_eq!(resolve_url(base, "b\n.ht\tml"), "https://x.dev/a/b.html");
    assert_eq!(resolve_url(" https://x.dev/a/ ", "c"), "https://x.dev/a/c");
}

#[test]
fn base_fragment_is_ignored() {
    assert_eq!(resolve_url("https://x.dev/p#frag", "q"), "https://x.dev/q");
    assert_eq!(
        resolve_url("https://x.dev/p?a#frag", ""),
        "https://x.dev/p?a"
    );
}

#[test]
fn other_schemes_pass_through() {
    let base = "https://x.dev/";
    assert_eq!(resolve_url(base, "mailto:a@b.c"), "mailto:a@b.c");
    assert_eq!(
        resolve_url(base, "javascript:void(0)"),
        "javascript:void(0)"
    );
    assert_eq!(
        resolve_url(base, "data:text/plain,hi"),
        "data:text/plain,hi"
    );
    // Not a scheme: `/` before the colon.
    assert_eq!(resolve_url(base, "a/b:c"), "https://x.dev/a/b:c");
}

#[test]
fn never_panics_on_odd_input() {
    for s in [
        "",
        ":",
        "::",
        "//",
        "///",
        "?",
        "#",
        "a:",
        ":a",
        "..",
        "/..",
        "🦀:x",
        "x:🦀",
        "http://",
        "http:///x",
        "\u{0}",
        "%",
        "\\\\x",
    ] {
        let _ = resolve_url(s, s);
        let _ = resolve_url("http://a/b", s);
        let _ = resolve_url(s, "http://a/b");
        let _ = resolve_srcset(s, s);
    }
}

#[test]
fn srcset() {
    assert_eq!(
        resolve_srcset("https://x.dev/p/", "a.png 1x, /b.png 2x"),
        "https://x.dev/p/a.png 1x, https://x.dev/b.png 2x"
    );
}

#[test]
fn srcset_urls_may_contain_commas() {
    // Cloudinary-style URLs: commas are part of the URL, not separators.
    let srcset = "https://c.io/up/c_fill,w_300/a.jpg 300w, https://c.io/up/c_fill,w_600/a.jpg 600w";
    assert_eq!(resolve_srcset("https://x.dev/", srcset), srcset);
    assert_eq!(
        resolve_srcset("https://x.dev/", "/c_fill,w_300/a.jpg 300w,/b.jpg 600w"),
        "https://x.dev/c_fill,w_300/a.jpg 300w, https://x.dev/b.jpg 600w"
    );
    assert_eq!(
        resolve_srcset("https://x.dev/", "data:image/png;base64,AAAA 1x"),
        "data:image/png;base64,AAAA 1x"
    );
}

#[test]
fn srcset_trailing_and_empty_candidates() {
    // A trailing comma ends the URL; it is not an empty candidate that would
    // resolve to the base.
    assert_eq!(
        resolve_srcset("https://x.dev/", "a.png 1x,"),
        "https://x.dev/a.png 1x"
    );
    assert_eq!(
        resolve_srcset("https://x.dev/", "a.png,"),
        "https://x.dev/a.png"
    );
    assert_eq!(
        resolve_srcset("https://x.dev/", " , ,a.png,, b.png 2x ,"),
        "https://x.dev/a.png, https://x.dev/b.png 2x"
    );
    assert_eq!(resolve_srcset("https://x.dev/", " , "), "");
    assert_eq!(resolve_srcset("https://x.dev/", ""), "");
}

#[test]
fn srcset_descriptors_keep_parentheses() {
    // Descriptors run to the next comma outside parentheses.
    assert_eq!(
        resolve_srcset("https://x.dev/", "a.png (x, y) 1x, b.png"),
        "https://x.dev/a.png (x, y) 1x, https://x.dev/b.png"
    );
    assert_eq!(
        resolve_srcset("https://x.dev/", "a.png\t\n 2x"),
        "https://x.dev/a.png 2x"
    );
}
