//! The optional `serde` Serialize path must produce byte-for-byte the same JSON
//! as the zero-dependency `to_json` writer.
#![cfg(all(feature = "serde", feature = "json"))]

use domq::parse;

#[test]
fn zero_dep_json_matches_serde_json() {
    let samples = [
        "",
        "<br>",
        "<p>hello</p>",
        "<ul id=list><li class=item>Item 1</li><li>2</li></ul>",
        "<!doctype html><!-- c --><div data-x='1' disabled>a<span>b</span>c</div>",
        "<p>Tom &amp; Jerry &lt;3 &#169;</p>",
        "<script>if (a < b) x('<p>');</script>",
        "<p>line1\nline2\t\"q\" \\z \u{1}</p>",
        "<b><i>x</b>y</i>",
        "<a href=\"/x?y=1&z=2\">l</a>",
    ];
    for s in samples {
        let dom = parse(s);
        let zero = dom.to_json();
        let via_serde = serde_json::to_string(&dom).unwrap();
        assert_eq!(zero, via_serde, "JSON mismatch for input {s:?}");
    }
}
