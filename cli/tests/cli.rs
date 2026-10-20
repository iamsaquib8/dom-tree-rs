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
        .write_all(stdin.as_bytes())
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

#[test]
fn invalid_utf8_is_tolerated() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_domtree"))
        .args(["-t", "p"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(b"<p>caf\xe9</p>")
        .expect("write");
    let out = child.wait_with_output().expect("wait");
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "caf\u{fffd}\n");
}
