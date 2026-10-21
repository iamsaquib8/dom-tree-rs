//! Tests for readable plain-text output (`to_text`).
//!
//! Page snapshots live in `tests/snapshots/`. Regenerate with
//! `INSTA_UPDATE=always cargo test --test text_output` and review the diff.

use domtree::parse;

fn text(html: &str) -> String {
    parse(html).to_text()
}

#[test]
fn hidden_elements_are_skipped() {
    assert_eq!(
        text("<head><title>T</title><style>p{}</style></head><p>Body</p>"),
        "Body"
    );
    // `<title>` is metadata even without a `<head>` around it.
    assert_eq!(text("<!doctype html><title>T</title><p>x</p>"), "x");
    assert_eq!(
        text("<p>a<datalist><option>o</option></datalist><noembed>n</noembed>b</p>"),
        "ab"
    );
    assert_eq!(
        text(
            "<p>a<script>x()</script><noscript>no js</noscript><template><p>t</p></template>b</p>"
        ),
        "ab"
    );
    assert_eq!(
        text("<p>x<iframe>frame</iframe><object>obj</object><svg><text>s</text></svg>y</p>"),
        "xy"
    );
    assert_eq!(text("<!doctype html><!-- comment --><p>x</p>"), "x");
}

#[test]
fn whitespace_collapses() {
    assert_eq!(text("<p>  a \n\t b   <b> c </b>  d </p>"), "a b c d");
    assert_eq!(text("<span>a</span><span>b</span>"), "ab");
    assert_eq!(text("<span>a</span> <span>b</span>"), "a b");
    assert_eq!(text("a&nbsp;&nbsp;b"), "a b");
}

#[test]
fn block_elements_start_new_lines() {
    assert_eq!(text("<div>a</div><div>b</div>"), "a\nb");
    assert_eq!(text("<section>a<article>b</article>c</section>"), "a\nb\nc");
    assert_eq!(text("<ul><li>one</li><li>two</li></ul>"), "one\ntwo");
    assert_eq!(
        text("x<nav>n</nav><header>h</header><footer>f</footer><main>m</main>"),
        "x\nn\nh\nf\nm"
    );
    assert_eq!(text("<dl><dt>term</dt><dd>def</dd></dl>"), "term\ndef");
    assert_eq!(
        text("<div><div><div>deep</div></div></div>after"),
        "deep\nafter"
    );
}

#[test]
fn paragraphs_are_separated_by_one_blank_line() {
    assert_eq!(text("<p>a</p><p>b</p>"), "a\n\nb");
    assert_eq!(
        text("<h1>T</h1>text<ul><li>i</li></ul><blockquote>q</blockquote><h6>six</h6>"),
        "T\n\ntext\n\ni\n\nq\n\nsix"
    );
    assert_eq!(text("<p>a</p><hr><p>b</p>"), "a\n\nb");
    assert_eq!(text("<div><p>a</p></div><div><p>b</p></div>"), "a\n\nb");
    assert_eq!(text("<p>a</p>\n\n\n<p></p><p> </p><p>b</p>"), "a\n\nb");
}

#[test]
fn nested_lists_have_no_blank_lines() {
    assert_eq!(
        text("<ul><li>a<ul><li>b</li><li>c</li></ul></li><li>d</li></ul><p>e</p>"),
        "a\nb\nc\nd\n\ne"
    );
}

#[test]
fn br_is_a_newline() {
    assert_eq!(text("<p>a<br>b <br> c</p>"), "a\nb\nc");
    assert_eq!(text("a<br><br>b"), "a\n\nb");
    assert_eq!(text("a<br><br><br><br>b"), "a\n\nb"); // blank-line runs collapse
    assert_eq!(text("<p>a<br></p><p>b</p>"), "a\n\nb");
}

#[test]
fn pre_and_textarea_are_verbatim() {
    assert_eq!(
        text("<p>x</p><pre>\n  fn main() {\n      go();\n  }\n</pre><p>y</p>"),
        "x\n\n  fn main() {\n      go();\n  }\n\ny"
    );
    assert_eq!(text("<pre><code>a  b\n\n\nc</code></pre>"), "a  b\n\n\nc");
    assert_eq!(
        text("<textarea>  keep\n   this</textarea>"),
        "  keep\n   this"
    );
    assert_eq!(text("<pre>a<br>b</pre>"), "a\nb");
    assert_eq!(text("<pre>a\r\nb\rc</pre>"), "a\nb\nc");
    // Whitespace collapses again after the `<pre>`.
    assert_eq!(text("<pre> a </pre><p>  b  c </p>"), " a\n\nb c");
}

#[test]
fn table_cells_are_tab_separated() {
    assert_eq!(
        text("<table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></table>"),
        "A\tB\n1\t2"
    );
    assert_eq!(
        text("<table><tr><td>a</td><td></td><td>c</td></tr></table>"),
        "a\t\tc"
    );
    assert_eq!(
        text("<table>\n<tr>\n<td> a </td>\n<td> b </td>\n</tr>\n</table>"),
        "a\tb"
    );
    // Implicitly closed cells and rows.
    assert_eq!(
        text("<table><tr><td>a<td>b<tr><td>c<td>d</table>"),
        "a\tb\nc\td"
    );
}

#[test]
fn blocks_inside_cells_keep_the_row_together() {
    assert_eq!(
        text("<table><tr><td>a</td><td><div>b</div></td></tr></table>"),
        "a\tb"
    );
    assert_eq!(
        text(
            "<table><tr><td><p>a</p></td><td><p>b</p></td></tr>\
             <tr><td><div>c</div></td><td>d<br></td><td>e</td></tr></table>"
        ),
        "a\tb\nc\td\te"
    );
    // Breaks within a cell still break.
    assert_eq!(
        text("<table><tr><td><p>a</p><p>b</p></td><td>c</td></tr></table>"),
        "a\n\nb\tc"
    );
    // A row break is not swallowed by an empty cell.
    assert_eq!(
        text("<table><tr><td>a</td><td></td></tr><tr><td>b</td></tr></table>"),
        "a\nb"
    );
}

#[test]
fn output_is_trimmed() {
    assert_eq!(text("\n\n  <p>  hi  </p>  \n\n"), "hi");
    assert_eq!(text("<br><br><div></div><p>x</p><br><br>"), "x");
    assert_eq!(text(""), "");
    assert_eq!(text("<script>only</script>"), "");
    assert_eq!(text("<pre>\n\n\ncode\n\n</pre>"), "code");
}

#[test]
fn no_trailing_spaces_on_lines() {
    let t = text("<div>a <span> </span></div><div> b </div><p>c <br> d</p>");
    assert!(t.lines().all(|l| l == l.trim_end()), "{t:?}");
    assert_eq!(t, "a\nb\n\nc\nd");
}

#[test]
fn entities_are_decoded() {
    assert_eq!(
        text("<p>Tom &amp; Jerry &#150; &copy; &lt;3</p>"),
        "Tom & Jerry \u{2013} \u{a9} <3"
    );
}

#[test]
fn node_to_text() {
    let dom = parse("<div><p>one</p><p>two <b>2</b></p></div><p>other</p>");
    let div = dom.find_by_tag("div").next().unwrap();
    assert_eq!(div.to_text(), "one\n\ntwo 2");
    let b = dom.find_by_tag("b").next().unwrap();
    assert_eq!(b.to_text(), "2");
    let text_node = b.first_child().unwrap();
    assert_eq!(text_node.to_text(), "2");
}

#[test]
fn not_text_content() {
    let dom = parse("<div><style>.x{}</style><p>a</p><p>b</p></div>");
    let div = dom.root().unwrap();
    assert_eq!(div.text_content(), ".x{}ab");
    assert_eq!(div.to_text(), "a\n\nb");
}

macro_rules! page_snapshot {
    ($name:ident, $file:literal) => {
        #[test]
        fn $name() {
            let html = include_str!(concat!("pages/", $file));
            insta::assert_snapshot!(parse(html).to_text());
        }
    };
}

page_snapshot!(page_blog, "blog.html");
page_snapshot!(page_docs, "docs.html");
page_snapshot!(page_messy, "messy.html");
