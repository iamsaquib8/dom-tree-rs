//! Tests for HTML → Markdown (`to_markdown`).
//!
//! Page snapshots live in `tests/snapshots/`. Regenerate with
//! `INSTA_UPDATE=always cargo test --test markdown_output` and review the diff.
#![cfg(feature = "markdown")]

use domtree::parse;

fn md(html: &str) -> String {
    parse(html).to_markdown()
}

// --- blocks ------------------------------------------------------------------

#[test]
fn headings() {
    assert_eq!(
        md("<h1>One</h1><h2>Two</h2><h6>Six</h6>"),
        "# One\n\n## Two\n\n###### Six"
    );
    // Headings are a single line: breaks and nested blocks become spaces.
    assert_eq!(
        md("<h2>Head<br>line <p>block</p></h2>"),
        "## Head line block"
    );
    assert_eq!(md("<h1> </h1><p>x</p>"), "x");
}

#[test]
fn paragraphs_and_blocks() {
    assert_eq!(md("<p>a</p><p>b</p>"), "a\n\nb");
    assert_eq!(
        md("<div>a</div><div>b</div>text<span>c</span>"),
        "a\n\nb\n\ntextc"
    );
    assert_eq!(
        md("<section><div><p>deep</p></div></section>after"),
        "deep\n\nafter"
    );
    assert_eq!(md("<p>a</p><hr><p>b</p>"), "a\n\n---\n\nb");
}

#[test]
fn hard_breaks_use_a_backslash() {
    assert_eq!(md("<p>one<br>two</p>"), "one\\\ntwo");
    assert_eq!(md("<p>one <br> two</p>"), "one\\\ntwo");
    // A break at the edge of a paragraph is dropped.
    assert_eq!(md("<p><br>one<br></p><p>two</p>"), "one\n\ntwo");
    assert_eq!(md("<ul><li>a<br>b</li></ul>"), "- a\\\n  b");
}

#[test]
fn whitespace_collapses() {
    assert_eq!(md("<p>  a \n\t b   <b> c </b>  d </p>"), "a b **c** d");
    assert_eq!(md("<p>a&nbsp;&nbsp;b</p>"), "a b");
    assert_eq!(md("\n\n<p>x</p>\n\n"), "x");
}

// --- inline ------------------------------------------------------------------

#[test]
fn emphasis() {
    assert_eq!(md("<b>b</b> <strong>s</strong>"), "**b** **s**");
    assert_eq!(md("<i>i</i> <em>e</em>"), "*i* *e*");
    assert_eq!(
        md("<del>d</del> <s>s</s> <strike>k</strike>"),
        "~~d~~ ~~s~~ ~~k~~"
    );
    assert_eq!(md("<b><i>both</i></b>"), "***both***");
    // Whitespace moves outside the markers; empty spans vanish.
    assert_eq!(
        md("a<b> bold </b>b <i></i>c <em> </em>d"),
        "a **bold** b c d"
    );
    // Nested duplicates collapse.
    assert_eq!(md("<b>a <b>b</b> c</b>"), "**a b c**");
    // Spanning blocks: closed and reopened around each block.
    assert_eq!(md("<b><p>one</p><p>two</p></b>"), "**one**\n\n**two**");
}

#[test]
fn inline_code() {
    assert_eq!(
        md("<p>Run <code>cargo test</code> now</p>"),
        "Run `cargo test` now"
    );
    assert_eq!(md("<code>a`b</code>"), "``a`b``");
    assert_eq!(md("<code>`tick`</code>"), "`` `tick` ``");
    assert_eq!(md("<code>  a \n  b  </code>"), "`a b`");
    assert_eq!(md("<code>*not* [md]</code>"), "`*not* [md]`");
    assert_eq!(md("<p>x<code> </code>y</p>"), "xy");
    assert_eq!(md("<kbd>Ctrl</kbd>+<kbd>C</kbd>"), "`Ctrl`+`C`");
    assert_eq!(md("<p><code>a</code> b</p>"), "`a` b");
}

#[test]
fn adjacent_identical_spans_merge() {
    // `**a****b**` / `*a**b*` would misparse (rule of 3).
    assert_eq!(md("<b>a</b><b>b</b>"), "**ab**");
    assert_eq!(md("<em>a</em><em>b</em>"), "*ab*");
    assert_eq!(md("<strong>a</strong><b>b</b>c"), "**ab**c");
    assert_eq!(md("<del>a</del><s>b</s>"), "~~ab~~");
    assert_eq!(md("<b><i>a</i></b><b><i>b</i></b>"), "***ab***");
    assert_eq!(md("<b><i>a</i></b><b>b</b>"), "***a*b**");
    assert_eq!(md("<p><b>a</b><br><b>b</b></p>"), "**a**\\\n**b**");
    // Whitespace already keeps them apart.
    assert_eq!(md("<b>a</b> <b>b</b>"), "**a** **b**");
    // Links never merge.
    assert_eq!(md("<a href=x>a</a><a href=x>b</a>"), "[a](x)[b](x)");
}

#[test]
fn adjacent_code_spans_merge() {
    // Two spans side by side would read as one span with literal backticks.
    assert_eq!(md("<code>a</code><code>b</code>"), "`ab`");
    assert_eq!(md("<code>a`</code><code>b</code>"), "``a`b``");
    assert_eq!(md("<b><code>a</code></b><b><code>b</code></b>"), "**`ab`**");
    assert_eq!(md("<code>a</code> <code>b</code>"), "`a` `b`");
    assert_eq!(md("<p><code>a</code><br><code>b</code></p>"), "`a`\\\n`b`");
    assert_eq!(md("<code>a</code><b><code>b</code></b>"), "`a`**`b`**");
}

#[test]
fn known_limitation_emphasis_flanking() {
    // CommonMark won't close `**` between punctuation and a letter, so this
    // renders without bold; the text itself is exact. Documented in the module.
    assert_eq!(md("<b>Note:</b>Text"), "**Note:**Text");
}

#[test]
fn links() {
    assert_eq!(md("<a href='/x'>text</a>"), "[text](/x)");
    assert_eq!(md("<a href='/x' title='Title'>t</a>"), "[t](/x \"Title\")");
    assert_eq!(
        md(r#"<a href="/x" title='say "hi"'>t</a>"#),
        r#"[t](/x "say \"hi\"")"#
    );
    // No href: just the text.
    assert_eq!(md("<a name='top'>anchor</a>"), "anchor");
    assert_eq!(md("<a href=''>empty</a>"), "empty");
    // No text: the URL is the text.
    assert_eq!(
        md("<a href='https://e.com'></a>"),
        "[https://e.com](https://e.com)"
    );
    assert_eq!(
        md("<p>Mail: <a href='mailto:a@b.c'></a>!</p>"),
        "Mail: [mailto:a@b.c](mailto:a@b.c)!"
    );
    assert_eq!(md("<p><a href='/x'> </a>y</p>"), "[/x](/x) y");
    // Destinations with spaces or parentheses are wrapped in <…>.
    assert_eq!(md("<a href='/a b'>x</a>"), "[x](</a b>)");
    assert_eq!(
        md("<a href='https://en.wikipedia.org/wiki/Rust_(language)'>w</a>"),
        "[w](<https://en.wikipedia.org/wiki/Rust_(language)>)"
    );
    assert_eq!(md("<a href='/a<b>'>x</a>"), "[x](</a\\<b\\>>)");
    // Inline markup inside the text, and text escaping.
    assert_eq!(
        md("<a href='/x'>a <b>b</b> [c]</a>"),
        "[a **b** \\[c\\]](/x)"
    );
    // A link wrapping blocks links its first block only.
    assert_eq!(
        md("<a href='/card'><h3>Title</h3><p>Body</p></a>"),
        "### [Title](/card)\n\nBody"
    );
}

#[test]
fn images() {
    assert_eq!(md("<img src='a.png' alt='An image'>"), "![An image](a.png)");
    assert_eq!(
        md("<img src='a.png' alt='A' title='T'>"),
        "![A](a.png \"T\")"
    );
    assert_eq!(md("<img src='a b.png'>"), "![](<a b.png>)");
    assert_eq!(
        md("<img src='a.png' alt='[x] *y*'>"),
        "![\\[x\\] \\*y\\*](a.png)"
    );
    // No src: the alt text stands in.
    assert_eq!(md("<p>see <img alt='diagram'></p>"), "see diagram");
    assert_eq!(
        md("<a href='/'><img src='logo.png' alt='Home'></a>"),
        "[![Home](logo.png)](/)"
    );
}

// --- code blocks -------------------------------------------------------------

#[test]
fn fenced_code_blocks() {
    assert_eq!(
        md("<pre><code class='language-rust'>fn main() {\n    go();\n}\n</code></pre>"),
        "```rust\nfn main() {\n    go();\n}\n```"
    );
    assert_eq!(
        md("<pre class='lang-py'>print(1)</pre>"),
        "```py\nprint(1)\n```"
    );
    assert_eq!(md("<pre>\nplain</pre>"), "```\nplain\n```");
    // The fence outgrows any backtick run inside.
    assert_eq!(md("<pre>a ``` b</pre>"), "````\na ``` b\n````");
    // Markdown is not escaped inside code; HTML entities are decoded.
    assert_eq!(md("<pre>*a* &lt;b&gt; [c]</pre>"), "```\n*a* <b> [c]\n```");
    // Markup inside: text kept, `<br>` and blocks become line breaks.
    assert_eq!(
        md("<pre><span>a</span><br><b>b</b><div>c</div></pre>"),
        "```\na\nb\nc\n```"
    );
    assert_eq!(md("<pre>  </pre><p>x</p>"), "x");
    assert_eq!(md("<p>x</p><pre>y</pre><p>z</p>"), "x\n\n```\ny\n```\n\nz");
}

// --- lists -------------------------------------------------------------------

#[test]
fn unordered_lists() {
    assert_eq!(md("<ul><li>a</li><li>b</li></ul>"), "- a\n- b");
    assert_eq!(md("<ul><li>a<li>b</ul>"), "- a\n- b");
    assert_eq!(md("<p>x</p><ul><li>a</li></ul><p>y</p>"), "x\n\n- a\n\ny");
    // Empty items are dropped.
    assert_eq!(md("<ul><li></li><li> </li><li>b</li></ul>"), "- b");
}

#[test]
fn ordered_lists() {
    assert_eq!(md("<ol><li>a</li><li>b</li></ol>"), "1. a\n2. b");
    assert_eq!(md("<ol start='9'><li>a</li><li>b</li></ol>"), "9. a\n10. b");
    assert_eq!(md("<ol start='0'><li>z</li></ol>"), "0. z");
    assert_eq!(md("<ol start='junk'><li>a</li></ol>"), "1. a");
    assert_eq!(md("<ol><li></li><li>a</li></ol>"), "1. a");
    // CommonMark has no negative numbers (`-3.` would be a bullet): clamp to 0.
    assert_eq!(md("<ol start='-3'><li>a</li></ol>"), "0. a");
    assert_eq!(
        md("<ol start='99999999999'><li>a</li></ol>"),
        "999999999. a"
    );
}

#[test]
fn list_item_values_and_reversed_lists() {
    // Numbers are what a browser shows (renderers only honour the first).
    assert_eq!(md("<ol><li>a<li value=7>b<li>c</ol>"), "1. a\n7. b\n8. c");
    assert_eq!(md("<ol><li value=4>a<li>b</ol>"), "4. a\n5. b");
    assert_eq!(md("<ol reversed><li>a<li>b<li>c</ol>"), "3. a\n2. b\n1. c");
    assert_eq!(md("<ol reversed start=10><li>a<li>b</ol>"), "10. a\n9. b");
    assert_eq!(md("<ol reversed start=1><li>a<li>b</ol>"), "1. a\n0. b");
    // `reversed` means nothing on a bullet list.
    assert_eq!(md("<ul reversed><li>a<li>b</ul>"), "- a\n- b");
}

#[test]
fn adjacent_lists_stay_separate() {
    // Same marker after a blank line would continue the first list.
    assert_eq!(md("<ul><li>a</li></ul><ul><li>b</li></ul>"), "- a\n\n* b");
    assert_eq!(
        md("<ul><li>a</li></ul><ul><li>b</li></ul><ul><li>c</li></ul>"),
        "- a\n\n* b\n\n- c"
    );
    assert_eq!(
        md("<ol><li>a</li></ol><ol start=5><li>b</li></ol>"),
        "1. a\n\n5) b"
    );
    assert_eq!(
        md("<ul><li>x<ul><li>a</li></ul><ul><li>b</li></ul></li></ul>"),
        "- x\n  - a\n\n  * b"
    );
    // Wrappers that leave no trace in Markdown don't separate them either.
    assert_eq!(
        md("<div><ul><li>a</li></ul></div><!-- c --><div><ul><li>b</li></ul></div>"),
        "- a\n\n* b"
    );
    assert_eq!(
        md("<ul><li>a<ul><li>x</li></ul></li></ul><ul><li>b</li></ul>"),
        "- a\n  - x\n\n* b"
    );
    // Anything in between, or a different kind, already separates them.
    assert_eq!(
        md("<ul><li>a</li></ul><p>p</p><ul><li>b</li></ul>"),
        "- a\n\np\n\n- b"
    );
    assert_eq!(md("<ul><li>a</li></ul><ol><li>b</li></ol>"), "- a\n\n1. b");
    assert_eq!(md("<ul><li>*</li></ul><ul><li>b</li></ul>"), "- \\*\n\n* b");
    // Lists in different items are apart already: no switch needed.
    assert_eq!(
        md("<ul><li><ul><li>a</li></ul></li><li><ul><li>b</li></ul></li></ul>"),
        "- - a\n- - b"
    );
}

#[test]
fn nested_lists() {
    assert_eq!(
        md("<ul><li>a<ul><li>b</li><li>c<ol><li>d</li></ol></li></ul></li><li>e</li></ul>"),
        "- a\n  - b\n  - c\n    1. d\n- e"
    );
    // Continuation indent follows the marker width.
    assert_eq!(
        md("<ol start='10'><li>ten<ul><li>x</li></ul></li></ol>"),
        "10. ten\n    - x"
    );
    // A list as an item's first content shares its line.
    assert_eq!(md("<ul><li><ul><li>x</li></ul></li></ul>"), "- - x");
    // Text after a nested list needs a blank line, or it would join the list.
    assert_eq!(
        md("<ul><li>a<ul><li>b</li></ul>tail</li><li>c</li></ul>"),
        "- a\n  - b\n\n  tail\n- c"
    );
}

#[test]
fn list_items_with_blocks() {
    assert_eq!(
        md("<ul><li><p>one</p><p>two</p></li><li>next</li></ul>"),
        "- one\n\n  two\n- next"
    );
    assert_eq!(
        md("<ol><li>run:<pre>cargo test</pre></li></ol>"),
        "1. run:\n\n   ```\n   cargo test\n   ```"
    );
    assert_eq!(
        md("<ul><li>quote<blockquote><p>q1</p><p>q2</p></blockquote></li></ul>"),
        "- quote\n\n  > q1\n  >\n  > q2"
    );
    assert_eq!(
        md("<ul><li><h3>Title</h3>text</li></ul>"),
        "- ### Title\n\n  text"
    );
}

#[test]
fn li_outside_a_list() {
    assert_eq!(md("<li>orphan</li>"), "- orphan");
    assert_eq!(md("<div><li>a</li></div>"), "- a");
}

// --- quotes ------------------------------------------------------------------

#[test]
fn blockquotes() {
    assert_eq!(md("<blockquote>quoted</blockquote>"), "> quoted");
    assert_eq!(
        md("<blockquote><p>a</p><p>b</p></blockquote>"),
        "> a\n>\n> b"
    );
    assert_eq!(
        md("<blockquote><p>a</p><blockquote><p>b</p></blockquote></blockquote>"),
        "> a\n>\n> > b"
    );
    assert_eq!(
        md("<blockquote><ul><li>x</li><li>y</li></ul></blockquote>"),
        "> - x\n> - y"
    );
    assert_eq!(
        md("<p>before</p><blockquote>q</blockquote><p>after</p>"),
        "before\n\n> q\n\nafter"
    );
    assert_eq!(md("<blockquote>a<br>b</blockquote>"), "> a\\\n> b");
}

// --- tables ------------------------------------------------------------------

#[test]
fn tables() {
    assert_eq!(
        md("<table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></table>"),
        "| A | B |\n| --- | --- |\n| 1 | 2 |"
    );
    // No header cells: the first row becomes the header.
    assert_eq!(
        md("<table><tr><td>a</td><td>b</td></tr><tr><td>c</td><td>d</td></tr></table>"),
        "| a | b |\n| --- | --- |\n| c | d |"
    );
    // Rows are never reordered: a later `<thead>`/`<th>` row stays where it is
    // and the first row is the header.
    assert_eq!(
        md("<table><tbody><tr><td>1</td></tr></tbody><thead><tr><td>H</td></tr></thead></table>"),
        "| 1 |\n| --- |\n| H |"
    );
    assert_eq!(
        md("<table><tr><td>a</td></tr><tr><th>H</th></tr></table>"),
        "| a |\n| --- |\n| H |"
    );
    assert_eq!(
        md(
            "<table><thead><tr><th>H</th></tr></thead><tr><td>a</td></tr><tr><th>H2</th></tr></table>"
        ),
        "| H |\n| --- |\n| a |\n| H2 |"
    );
}

#[test]
fn table_cells_are_inline() {
    assert_eq!(
        md(
            "<table><tr><td>a<br>b</td><td><p>p1</p><p>p2</p></td><td><ul><li>x</li><li>y</li></ul></td></tr></table>"
        ),
        "| a b | p1 p2 | x y |\n| --- | --- | --- |"
    );
    assert_eq!(
        md("<table><tr><td>a|b</td><td><code>c|d</code></td><td><b>e</b></td></tr></table>"),
        "| a\\|b | `c\\|d` | **e** |\n| --- | --- | --- |"
    );
    // A nested table is flattened into its cell.
    assert_eq!(
        md(
            "<table><tr><td><table><tr><td>in1</td><td>in2</td></tr></table></td><td>o</td></tr></table>"
        ),
        "| in1 in2 | o |\n| --- | --- |"
    );
}

#[test]
fn ragged_tables_are_padded() {
    assert_eq!(
        md(
            "<table><tr><th>A</th></tr><tr><td>1</td><td>2</td><td>3</td></tr><tr><td></td><td>x</td></tr></table>"
        ),
        "| A | | |\n| --- | --- | --- |\n| 1 | 2 | 3 |\n| | x | |"
    );
}

#[test]
fn table_extras() {
    // Captions and stray text inside the table come before it — where a
    // browser draws them, even when the source puts them after the rows.
    assert_eq!(
        md("<table><caption>Cap</caption><tr><td>a</td></tr></table>"),
        "Cap\n\n| a |\n| --- |"
    );
    assert_eq!(
        md("<table><tr><td>a</td></tr><caption>Cap</caption></table>"),
        "Cap\n\n| a |\n| --- |"
    );
    assert_eq!(md("<table></table><p>x</p>"), "x");
    assert_eq!(
        md("<ul><li>item<table><tr><td>v</td></tr></table></li></ul>"),
        "- item\n\n  | v |\n  | --- |"
    );
    // Implicitly closed rows and cells.
    assert_eq!(
        md("<table><tr><th>A<th>B<tr><td>1<td>2</table>"),
        "| A | B |\n| --- | --- |\n| 1 | 2 |"
    );
}

#[test]
fn definition_lists() {
    assert_eq!(
        md("<dl><dt>Term</dt><dd>Definition</dd><dt>T2</dt><dd>D2</dd></dl>"),
        "**Term**\n\nDefinition\n\n**T2**\n\nD2"
    );
}

// --- skipping and escaping ---------------------------------------------------

#[test]
fn hidden_elements_are_skipped() {
    assert_eq!(
        md("<head><title>T</title></head><p>a<script>x</script><style>s</style>b</p>"),
        "ab"
    );
    // `<title>` is metadata even without a `<head>` around it.
    assert_eq!(md("<!doctype html><title>T</title><p>x</p>"), "x");
    assert_eq!(
        md("<p>a<datalist><option>o</option></datalist><noembed>n</noembed>b</p>"),
        "ab"
    );
    assert_eq!(
        md("<template><p>t</p></template><noscript>n</noscript><svg><text>s</text></svg><p>x</p>"),
        "x"
    );
    assert_eq!(md("<!doctype html><!-- c --><p>x<!-- y -->z</p>"), "xz");
}

#[test]
fn escapes_inline_syntax() {
    assert_eq!(
        md(r"<p>a\b *c* `d` [e] ~f~</p>"),
        r"a\\b \*c\* \`d\` \[e\] \~f\~"
    );
    assert_eq!(
        md("<p>_em_ and snake_case_name</p>"),
        r"\_em\_ and snake_case_name"
    );
    assert_eq!(md("<p>&lt;div&gt; and 1 &lt; 2</p>"), r"\<div> and 1 < 2");
    assert_eq!(
        md("<p>&amp;copy; vs AT&amp;T &amp; co</p>"),
        r"\&copy; vs AT\&T & co"
    );
}

#[test]
fn escapes_line_start_syntax() {
    assert_eq!(md("<p># not a heading</p>"), r"\# not a heading");
    assert_eq!(md("<p>&gt; not a quote</p>"), r"\> not a quote");
    assert_eq!(md("<p>- not a list</p>"), r"\- not a list");
    assert_eq!(md("<p>+ not a list</p>"), r"\+ not a list");
    assert_eq!(md("<p>1. not a list</p>"), r"1\. not a list");
    assert_eq!(md("<p>2) not a list</p>"), r"2\) not a list");
    assert_eq!(md("<p>---</p>"), r"\---");
    assert_eq!(md("<p>===</p>"), r"\===");
    // After a hard break is a line start too.
    assert_eq!(md("<p>a<br>- b</p>"), "a\\\n\\- b");
    // Item text starts a line.
    assert_eq!(md("<ul><li># x</li></ul>"), r"- \# x");
}

#[test]
fn does_not_over_escape_prose() {
    let prose = "Hello, world. It costs $3.50 - or 3.5 - and a-b is fine (really!) #hashtag C# 100% #1: ok?";
    assert_eq!(md(&format!("<p>{prose}</p>")), prose);
    assert_eq!(md("<p>Version 1.2.3 is out.</p>"), "Version 1.2.3 is out.");
    assert_eq!(md("<p>-5 degrees, +3 points</p>"), "-5 degrees, +3 points");
    assert_eq!(md("<p>x</p><p>2024. A year.</p>"), "x\n\n2024\\. A year.");
}

#[test]
fn trailing_heading_hashes_are_escaped() {
    assert_eq!(md("<h2>C #</h2>"), r"## C \#");
}

#[test]
fn output_has_no_stray_whitespace() {
    let out = md(
        "<div> <p> a </p> </div> <ul> <li> b </li> </ul> <blockquote> <p> c </p> <p> d </p> </blockquote>",
    );
    assert_eq!(out, "a\n\n- b\n\n> c\n>\n> d");
    assert!(out.lines().all(|l| l == l.trim_end()), "{out:?}");
    assert_eq!(md(""), "");
    assert_eq!(md("<script>x</script>"), "");
}

#[test]
fn node_to_markdown() {
    let dom = parse("<div><h2>T</h2><p>a <b>b</b></p></div><p>other</p>");
    let div = dom.find_by_tag("div").next().unwrap();
    assert_eq!(div.to_markdown(), "## T\n\na **b**");
    let b = dom.find_by_tag("b").next().unwrap();
    assert_eq!(b.to_markdown(), "**b**");
    let li = parse("<ol><li>x</li></ol>");
    assert_eq!(li.find_by_tag("li").next().unwrap().to_markdown(), "- x");
}

macro_rules! page_snapshot {
    ($name:ident, $file:literal) => {
        #[test]
        fn $name() {
            let html = include_str!(concat!("pages/", $file));
            insta::assert_snapshot!(parse(html).to_markdown());
        }
    };
}

page_snapshot!(page_blog, "blog.html");
page_snapshot!(page_docs, "docs.html");
page_snapshot!(page_messy, "messy.html");
