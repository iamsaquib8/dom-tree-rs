//! End-to-end tests: run the real `domtree` binary.

use std::io::Write;
use std::process::{Command, Stdio};

const PAGE: &str = r#"<!doctype html>
<html><head><title>T</title><base href="https://example.com/blog/"></head>
<body>
<nav><a href="/">Home</a></nav>
<article>
  <h1 class=title>Hello &amp; welcome</h1>
  <p>First <a href="post-1.html">post</a>.</p>
  <ul><li>one<li>two</ul>
  <img src="img/a.png" alt="">
</article>
<footer><a href="https://other.dev/x">x</a></footer>
</body></html>"#;

fn domtree(args: &[&str], stdin: &str) -> (String, String, i32) {
    domtree_bytes(args, stdin.as_bytes())
}

fn domtree_bytes(args: &[&str], stdin: &[u8]) -> (String, String, i32) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_domtree"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn domtree");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin)
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait");
    (
        String::from_utf8(out.stdout).expect("utf8 stdout"),
        String::from_utf8(out.stderr).expect("utf8 stderr"),
        out.status.code().unwrap_or(-1),
    )
}

fn ok(args: &[&str], stdin: &str) -> String {
    let (out, err, code) = domtree(args, stdin);
    assert_eq!(code, 0, "stderr: {err}");
    out
}

#[test]
fn selects_elements_as_html() {
    assert_eq!(ok(&["li"], PAGE), "<li>one</li>\n<li>two</li>\n");
}

#[test]
fn selector_words_are_joined() {
    assert_eq!(ok(&["ul", ">", "li:last-child"], PAGE), "<li>two</li>\n");
}

#[test]
fn attribute_output() {
    assert_eq!(
        ok(&["-a", "href", "a"], PAGE),
        "/\npost-1.html\nhttps://other.dev/x\n"
    );
}

#[test]
fn attribute_without_selector_searches_everything() {
    assert_eq!(ok(&["-a", "src"], PAGE), "img/a.png\n");
}

#[test]
fn base_resolves_urls() {
    assert_eq!(
        ok(&["-b", "https://site.dev/a/b", "-a", "href", "a"], PAGE),
        "https://site.dev/\nhttps://site.dev/a/post-1.html\nhttps://other.dev/x\n"
    );
}

#[test]
fn detect_base_uses_base_tag() {
    assert_eq!(
        ok(&["-B", "-a", "href", "article a"], PAGE),
        "https://example.com/blog/post-1.html\n"
    );
}

#[test]
fn base_keeps_commas_inside_srcset_urls() {
    let html = r#"<img srcset="https://c.io/up/c_fill,w_300/a.jpg 300w, https://c.io/up/c_fill,w_600/a.jpg 600w">"#;
    assert_eq!(
        ok(&["-b", "https://x.dev/", "-a", "srcset", "img"], html),
        "https://c.io/up/c_fill,w_300/a.jpg 300w, https://c.io/up/c_fill,w_600/a.jpg 600w\n"
    );
    // A trailing comma is not an extra (empty) candidate.
    assert_eq!(
        ok(
            &["-b", "https://x.dev/", "-a", "srcset", "img"],
            r#"<img srcset="a.png 1x,">"#
        ),
        "https://x.dev/a.png 1x\n"
    );
}

#[test]
fn base_rewrites_the_attribute_that_counts() {
    // With a repeated attribute the first one wins, as in `NodeRef::attr`.
    assert_eq!(
        ok(
            &["-b", "https://x.dev/d/", "-a", "href", "a"],
            "<a href=a href=b>x</a>"
        ),
        "https://x.dev/d/a\n"
    );
}

#[test]
fn remove_nodes_before_selecting() {
    assert_eq!(
        ok(&["-r", "nav", "-r", "footer", "-a", "href", "a"], PAGE),
        "post-1.html\n"
    );
}

#[test]
fn text_output() {
    assert_eq!(ok(&["-t", "h1"], PAGE), "Hello & welcome\n");
}

#[test]
fn markdown_output() {
    let md = ok(&["--markdown", "article"], PAGE);
    assert!(md.starts_with("# Hello & welcome\n"), "{md}");
    assert!(md.contains("[post](post-1.html)"), "{md}");
}

#[test]
fn count_and_first() {
    assert_eq!(ok(&["-c", "a"], PAGE), "3\n");
    assert_eq!(ok(&["-1", "-t", "li"], PAGE), "one\n");
}

#[test]
fn json_output() {
    assert_eq!(
        ok(&["-j", "li"], PAGE),
        concat!(
            r#"[{"type":"element","tag":"li","attrs":{},"children":[{"type":"text","value":"one"}]},"#,
            r#"{"type":"element","tag":"li","attrs":{},"children":[{"type":"text","value":"two"}]}]"#,
            "\n"
        )
    );
    assert_eq!(ok(&["-j", "-a", "href", "nav a"], PAGE), "[\"/\"]\n");
}

#[test]
fn no_selector_uses_whole_document() {
    assert_eq!(ok(&[], "<p>a<p>b"), "<p>a</p><p>b</p>\n");
    assert_eq!(ok(&["-t"], "<p>a<p>b"), "a\n\nb\n");
}

#[test]
fn pretty_output() {
    assert_eq!(
        ok(&["-p", "ul"], PAGE),
        "<ul>\n  <li>one</li>\n  <li>two</li>\n</ul>\n"
    );
}

#[test]
fn no_match_is_empty_success() {
    assert_eq!(ok(&["table"], PAGE), "");
}

#[test]
fn invalid_selector_points_at_problem() {
    let (out, err, code) = domtree(&["a:hover"], PAGE);
    assert_eq!(code, 1);
    assert_eq!(out, "");
    assert!(err.contains("invalid selector"), "{err}");
    assert!(err.contains("a:hover"), "{err}");
}

#[test]
fn usage_errors_exit_2() {
    let (_, err, code) = domtree(&["--bogus"], "");
    assert_eq!(code, 2);
    assert!(err.contains("--bogus"), "{err}");
}

#[test]
fn reads_file_and_writes_output() {
    let dir = std::env::temp_dir().join(format!("domtree-cli-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let input = dir.join("in.html");
    let output = dir.join("out.txt");
    std::fs::write(&input, PAGE).expect("write input");
    ok(
        &[
            "-f",
            input.to_str().expect("path"),
            "-o",
            output.to_str().expect("path"),
            "-t",
            "h1",
        ],
        "",
    );
    assert_eq!(
        std::fs::read_to_string(&output).expect("read output"),
        "Hello & welcome\n"
    );
    std::fs::remove_dir_all(&dir).ok();
}

fn usage_error(args: &[&str], stdin: &str) -> String {
    let (out, err, code) = domtree(args, stdin);
    assert_eq!((code, out.as_str()), (2, ""), "{args:?}: {err}");
    err
}

// --- --extract ---------------------------------------------------------------

#[test]
fn extract_one_object() {
    assert_eq!(
        ok(
            &[
                "-e",
                "title=h1",
                "-e",
                "first=li",
                "-e",
                "items=li[]",
                "-e",
                "img=img@src",
                "-e",
                "missing=table",
                "-e",
                "none=table[]",
            ],
            PAGE
        ),
        concat!(
            r#"{"title":"Hello & welcome","first":"one","items":["one","two"],"#,
            r#""img":"img/a.png","missing":null,"none":[]}"#,
            "\n"
        )
    );
}

#[test]
fn extract_resolves_url_attributes_with_base() {
    assert_eq!(
        ok(
            &["-B", "-e", "post=article a@href", "-e", "img=img@src"],
            PAGE
        ),
        concat!(
            r#"{"post":"https://example.com/blog/post-1.html","#,
            r#""img":"https://example.com/blog/img/a.png"}"#,
            "\n"
        )
    );
}

const CARDS: &str = r#"
<div class=card data-id=1><h2>Ann</h2><a href=/a>Profile</a>
  <span class=tag>rust</span> <span class=tag>wasm</span></div>
<div class=card data-id=2><h2> Bob </h2><span class=tag> </span></div>"#;

#[test]
fn extract_per_scope_match() {
    let args = [
        ".card",
        "-e",
        "name=h2",
        "-e",
        "url=a@href",
        "-e",
        "id=@data-id",
        "-e",
        "tags=.tag[]",
    ];
    assert_eq!(
        ok(&args, CARDS),
        concat!(
            r#"[{"name":"Ann","url":"/a","id":"1","tags":["rust","wasm"]},"#,
            r#"{"name":"Bob","url":null,"id":"2","tags":[""]}]"#,
            "\n"
        )
    );
    assert_eq!(
        ok(&[&args[..], &["--jsonl"]].concat(), CARDS),
        concat!(
            r#"{"name":"Ann","url":"/a","id":"1","tags":["rust","wasm"]}"#,
            "\n",
            r#"{"name":"Bob","url":null,"id":"2","tags":[""]}"#,
            "\n"
        )
    );
    // `-1`: the first scope only; `-w`: blank values are skipped.
    assert_eq!(
        ok(&[".card", "-1", "-e", "id=@data-id"], CARDS),
        "[{\"id\":\"1\"}]\n"
    );
    assert_eq!(
        ok(&[".card:last-child", "-w", "-e", "tags=.tag[]"], CARDS),
        "[{\"tags\":[]}]\n"
    );
}

#[test]
fn extract_scope_without_matches() {
    assert_eq!(ok(&["table", "-e", "x=td"], PAGE), "[]\n");
    assert_eq!(ok(&["table", "-e", "x=td", "--jsonl"], PAGE), "");
}

#[test]
fn extract_attribute_skips_matches_without_it() {
    assert_eq!(
        ok(&["-e", "src=img@src"], "<img alt=x><img src=b.png>"),
        "{\"src\":\"b.png\"}\n"
    );
}

#[test]
fn extract_spec_escaping() {
    let html = r#"<a href="mailto:me@x.org" title="a@b">mail</a><i class="w@sm">i</i>"#;
    assert_eq!(
        ok(
            &[
                "-e",
                r#"mail=a[href^="mailto:me@x"]@href"#,
                "-e",
                r#"text=[title="a@b"]"#,
                "-e",
                r"i=.w\@sm",
            ],
            html
        ),
        "{\"mail\":\"mailto:me@x.org\",\"text\":\"mail\",\"i\":\"i\"}\n"
    );
}

#[test]
fn extract_errors() {
    assert!(usage_error(&["-e", "title"], PAGE).contains("NAME=SELECTOR"));
    assert!(usage_error(&["-e", "a=h1", "-e", "a=h2"], PAGE).contains("twice"));
    assert!(usage_error(&["-e", "a=h1", "--links"], PAGE).contains("--links"));
    assert!(usage_error(&["-e", "a=h1", "-t"], PAGE).contains("--text"));
    let (out, err, code) = domtree(&["-e", "x=a:hover"], PAGE);
    assert_eq!((code, out.as_str()), (1, ""));
    assert!(err.contains("--extract x: invalid selector"), "{err}");
}

// --- --jsonl -----------------------------------------------------------------

#[test]
fn jsonl_output() {
    assert_eq!(
        ok(&["--jsonl", "li"], PAGE),
        concat!(
            r#"{"type":"element","tag":"li","attrs":{},"children":[{"type":"text","value":"one"}]}"#,
            "\n",
            r#"{"type":"element","tag":"li","attrs":{},"children":[{"type":"text","value":"two"}]}"#,
            "\n"
        )
    );
    assert_eq!(
        ok(&["--jsonl", "-a", "href", "a"], PAGE),
        "\"/\"\n\"post-1.html\"\n\"https://other.dev/x\"\n"
    );
    assert_eq!(ok(&["--jsonl"], "<b>x</b>").lines().count(), 1);
    assert_eq!(ok(&["--jsonl", "table"], PAGE), "");
}

// --- --csv -------------------------------------------------------------------

const TABLES: &str = r#"
<table id=a><thead><tr><th>Name<th>Note</thead>
  <tr><td>Ann<td>says "hi", twice
  <tr><td colspan=2>merged</table>
<p>between</p>
<table id=empty></table>
<table id=b><tr><td>1<td>2</table>"#;

#[test]
fn csv_output() {
    let first = "Name,Note\r\nAnn,\"says \"\"hi\"\", twice\"\r\nmerged,merged\r\n";
    assert_eq!(ok(&["--csv"], TABLES), format!("{first}\r\n1,2\r\n"));
    assert_eq!(ok(&["--csv", "-1"], TABLES), first);
    assert_eq!(ok(&["--csv", "#b"], TABLES), "1,2\r\n");
    // Non-table matches and tables without rows are skipped.
    assert_eq!(ok(&["--csv", "p, #empty"], TABLES), "");
    assert_eq!(ok(&["--csv"], PAGE), "");
    assert!(usage_error(&["--csv", "-a", "href"], TABLES).contains("--csv"));
}

// --- --main ------------------------------------------------------------------

const ARTICLE: &str = r#"<html><head><title>Arena trees</title></head><body>
<nav><a href="/">Home</a> <a href="/blog">Blog</a></nav>
<div class="post">
  <h1>Arena trees</h1>
  <p>An arena stores every node in one vector, and links them by index.</p>
  <p>Traversal is cheap, removal is a flag, and <a href="rc">nothing is reference-counted</a>.</p>
</div>
<div id="comments"><p>Great post, thanks for writing it up in so much detail!</p></div>
<footer><a href="/about">About</a></footer>
</body></html>"#;

#[test]
fn main_narrows_the_document() {
    assert_eq!(
        ok(&["--main", "--markdown"], ARTICLE),
        "# Arena trees\n\nAn arena stores every node in one vector, and links them by index.\n\n\
         Traversal is cheap, removal is a flag, and [nothing is reference-counted](rc).\n"
    );
    assert_eq!(ok(&["--main", "-c", "a"], ARTICLE), "1\n");
    assert_eq!(ok(&["--main", "-1", "-a", "class"], ARTICLE), "post\n");
    assert_eq!(
        ok(&["--main", "-e", "title=h1", "-e", "n=p[]"], ARTICLE),
        "{\"title\":\"Arena trees\",\"n\":[\"An arena stores every node in one vector, and links \
         them by index.\",\"Traversal is cheap, removal is a flag, and nothing is \
         reference-counted.\"]}\n"
    );
}

#[test]
fn main_falls_back_to_the_whole_document() {
    assert_eq!(
        ok(&["--main", "-t"], "<nav><a href=/>Home</a></nav>"),
        "Home\n"
    );
}

#[test]
fn main_applies_after_base_and_remove() {
    assert_eq!(
        ok(&["--main", "-b", "https://x.dev/a/", "--links"], ARTICLE),
        "https://x.dev/a/rc\tnothing is reference-counted\n"
    );
    assert_eq!(ok(&["-r", "a", "--main", "-c", "a"], ARTICLE), "0\n");
}

// --- --links / --meta --------------------------------------------------------

#[test]
fn links_output() {
    let html = format!(
        "{PAGE}<a href='javascript:go()'>js</a><a href=/i aria-label='Icon'><svg></svg></a>"
    );
    assert_eq!(
        ok(&["--links"], &html),
        "/\tHome\npost-1.html\tpost\nhttps://other.dev/x\tx\n/i\tIcon\n"
    );
    assert_eq!(
        ok(&["--links", "-B", "-j", "article"], &html),
        "[{\"url\":\"https://example.com/blog/post-1.html\",\"text\":\"post\"}]\n"
    );
    assert_eq!(
        ok(&["--links", "--jsonl", "nav, footer"], &html),
        "{\"url\":\"/\",\"text\":\"Home\"}\n{\"url\":\"https://other.dev/x\",\"text\":\"x\"}\n"
    );
    assert_eq!(ok(&["--links", "-c"], &html), "4\n");
    assert_eq!(ok(&["--links", "-1"], &html), "/\tHome\n");
    // A selector that matches a link itself keeps that link.
    assert_eq!(
        ok(&["--links", "a[href^=http]"], &html),
        "https://other.dev/x\tx\n"
    );
    assert!(usage_error(&["--links", "--meta"], PAGE).contains("cannot be combined"));
    assert!(usage_error(&["--links", "-m"], PAGE).contains("--markdown"));
}

#[test]
fn meta_output() {
    let html = r#"<html lang=en><head><title> A  page </title>
      <meta name=description content="About it">
      <meta name=keywords content="a, b">
      <link rel=canonical href="/p/1">
      <meta property=og:title content="OG">
      <meta property=og:image content="cover.png">
      <meta name=twitter:card content=summary>
      <link rel=alternate type=application/rss+xml href=feed.xml>
      <script type=application/ld+json>{"@type":"Article"}</script>
    </head><body><p>x</p></body></html>"#;
    assert_eq!(
        ok(&["--meta"], html),
        concat!(
            r#"{"title":"A page","description":"About it","canonical":"/p/1","lang":"en","#,
            r#""site_name":null,"image":"cover.png","author":null,"published":null,"#,
            r#""keywords":["a","b"],"og":[["og:title","OG"],["og:image","cover.png"]],"#,
            r#""twitter":[["twitter:card","summary"]],"json_ld":["{\"@type\":\"Article\"}"],"#,
            r#""feeds":["feed.xml"]}"#,
            "\n"
        )
    );
    let based = ok(&["--meta", "-b", "https://x.dev/blog/"], html);
    assert!(
        based.contains(r#""canonical":"https://x.dev/p/1""#),
        "{based}"
    );
    assert!(
        based.contains(r#""image":"https://x.dev/blog/cover.png""#),
        "{based}"
    );
    assert!(
        based.contains(r#""feeds":["https://x.dev/blog/feed.xml"]"#),
        "{based}"
    );
    // Page-level: `--main` does not narrow it.
    assert_eq!(ok(&["--meta", "--main"], html), ok(&["--meta"], html));
    assert!(usage_error(&["--meta", "p"], html).contains("SELECTOR"));
}

// --- encoding ----------------------------------------------------------------

#[test]
fn non_utf8_input_falls_back_to_windows_1252() {
    let (out, _, code) = domtree_bytes(&["-t", "p"], b"<p>caf\xe9 \x93q\x94</p>");
    assert_eq!((out.as_str(), code), ("caf\u{e9} \u{201c}q\u{201d}\n", 0));
}

#[test]
fn meta_charset_is_honoured() {
    let (out, _, _) = domtree_bytes(&["-t", "p"], b"<meta charset=iso-8859-1><p>\xc3\xa9</p>");
    assert_eq!(out, "\u{c3}\u{a9}\n");
}

#[test]
fn utf16_input() {
    let mut le = vec![0xFF, 0xFE];
    le.extend("<p>h\u{e9}</p>".encode_utf16().flat_map(u16::to_le_bytes));
    assert_eq!(domtree_bytes(&["-t"], &le).0, "h\u{e9}\n");
    let be: Vec<u8> = "<p>hi</p>"
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect();
    assert_eq!(
        domtree_bytes(&["--encoding", "utf-16be", "-t"], &be).0,
        "hi\n"
    );
}

#[test]
fn unknown_encoding_warns() {
    let (out, err, code) = domtree_bytes(&["--encoding=klingon", "-t"], b"<p>caf\xe9</p>");
    assert_eq!((out.as_str(), code), ("caf\u{fffd}\n", 0));
    assert!(err.contains("warning") && err.contains("klingon"), "{err}");
}

// --- misc --------------------------------------------------------------------

#[test]
fn help_lists_every_flag() {
    let help = ok(&["--help"], "");
    for flag in [
        "--extract",
        "--jsonl",
        "--csv",
        "--main",
        "--links",
        "--meta",
        "--encoding",
    ] {
        assert!(help.contains(flag), "{flag}");
    }
}

#[test]
fn closed_stdout_pipe_is_not_an_error() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_domtree"))
        .args(["p"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    drop(child.stdout.take());
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all("<p>x</p>".repeat(100_000).as_bytes())
        .expect("write");
    let out = child.wait_with_output().expect("wait");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&out.stderr), "");
}
