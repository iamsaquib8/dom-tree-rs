//! `Dom::links`, `Dom::resources` and `Dom::base_url`.

use domtree::parse;

fn urls(html: &str, base: Option<&str>) -> Vec<(String, String)> {
    parse(html)
        .links(base)
        .into_iter()
        .map(|l| (l.url, l.text))
        .collect()
}

fn pair(url: &str, text: &str) -> (String, String) {
    (url.to_string(), text.to_string())
}

#[test]
fn resolves_against_the_given_base() {
    assert_eq!(
        urls(
            r#"<a href="a.html">A</a><a href="/b">B</a><a href="https://o.dev/c">C</a><a href="?q=1">Q</a>"#,
            Some("https://x.dev/dir/page.html")
        ),
        [
            pair("https://x.dev/dir/a.html", "A"),
            pair("https://x.dev/b", "B"),
            pair("https://o.dev/c", "C"),
            pair("https://x.dev/dir/page.html?q=1", "Q"),
        ]
    );
}

#[test]
fn without_a_base_urls_are_trimmed_hrefs() {
    let dom = parse("<a href='  ../x.html\n'>x</a>");
    let links = dom.links(None);
    assert_eq!(links[0].href, "  ../x.html\n");
    assert_eq!(links[0].url, "../x.html");
}

#[test]
fn base_element_is_honoured() {
    let html = r#"<head><base href="/docs/v2/"></head><a href="intro.html">Intro</a>"#;
    // Relative <base>, resolved against the page URL.
    assert_eq!(
        urls(html, Some("https://x.dev/index.html")),
        [pair("https://x.dev/docs/v2/intro.html", "Intro")]
    );
    // Relative <base> and no page URL: nothing to anchor to.
    assert_eq!(urls(html, None), [pair("intro.html", "Intro")]);
    // Absolute <base> needs no page URL.
    let html = r#"<base href="https://cdn.dev/a/"><a href="b">B</a>"#;
    assert_eq!(urls(html, None), [pair("https://cdn.dev/a/b", "B")]);
    assert_eq!(
        urls(html, Some("https://x.dev/")),
        [pair("https://cdn.dev/a/b", "B")]
    );
}

#[test]
fn base_url_details() {
    // The first <base> with an href counts.
    let dom = parse("<base target=_blank><base href='/one/'><base href='/two/'>");
    assert_eq!(
        dom.base_url(Some("https://x.dev/p")).as_deref(),
        Some("https://x.dev/one/")
    );
    assert_eq!(dom.base_url(None).as_deref(), Some("/one/"));
    // An empty <base href> is the page URL itself.
    let dom = parse("<base href=''>");
    assert_eq!(
        dom.base_url(Some("https://x.dev/p#f")).as_deref(),
        Some("https://x.dev/p")
    );
    assert_eq!(dom.base_url(None), None);
    // No <base>: the given base, cleaned.
    let dom = parse("<p>x</p>");
    assert_eq!(
        dom.base_url(Some(" https://x.dev/ ")).as_deref(),
        Some("https://x.dev/")
    );
}

#[test]
fn javascript_links_are_skipped() {
    let html = "<a href='javascript:void(0)'>a</a><a href=' JavaScript:alert(1)'>b</a>\
                <a href='java\tscript:x()'>c</a><a href='/ok'>ok</a><a href='javascript-guide.html'>d</a>";
    let dom = parse(html);
    let links = dom.links(None);
    let texts: Vec<_> = links.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, ["ok", "d"]);
}

#[test]
fn which_elements_are_links() {
    let html = "<a name=anchor>no href</a><a href>empty</a><area href=/map alt='Map area'>\
                <link rel=next href=/2><img src=/i.png><A HREF=/upper>Upper</A>";
    assert_eq!(
        urls(html, Some("https://x.dev/")),
        [
            pair("https://x.dev/", "empty"),
            pair("https://x.dev/map", "Map area"),
            pair("https://x.dev/upper", "Upper"),
        ]
    );
}

#[test]
fn link_text_is_readable() {
    let html = "<a href=/1>  Multi\n  line\t text </a>\
                <a href=/2><div><h3>Card title</h3><p>Card summary.</p></div></a>\
                <a href=/3>a<b>b</b>c<br>d</a>\
                <a href=/4>text<script>ignored()</script><style>.x{}</style></a>\
                <a href=/5>caf&eacute; &amp; more</a>";
    let texts: Vec<_> = parse(html)
        .links(None)
        .into_iter()
        .map(|l| l.text)
        .collect();
    assert_eq!(
        texts,
        [
            "Multi line text",
            "Card title Card summary.",
            "abc d",
            "text",
            "café & more"
        ]
    );
}

#[test]
fn text_fallbacks_for_icon_links() {
    let html = r#"<a href=/1 aria-label="Close menu"><svg><title>x</title></svg></a>
                  <a href=/2><img src=logo.png alt="  Home  page "></a>
                  <a href=/3 title="Settings"><i class=icon></i></a>
                  <a href=/4><img src=a.png alt=""><img src=b.png alt="Second"></a>
                  <a href=/5></a>"#;
    let texts: Vec<_> = parse(html)
        .links(None)
        .into_iter()
        .map(|l| l.text)
        .collect();
    assert_eq!(texts, ["Close menu", "Home page", "Settings", "Second", ""]);
}

#[test]
fn nested_links_split_their_text() {
    // Invalid HTML this parser keeps nested: the inner text belongs to the
    // inner link only, and both are reported in document order.
    let html = "<a href=/outer>before <a href=/inner>inside</a> after</a>";
    assert_eq!(
        urls(html, None),
        [pair("/outer", "before after"), pair("/inner", "inside")]
    );
}

#[test]
fn first_of_repeated_attributes_wins() {
    let dom = parse("<a href=/first href=/second>x</a>");
    let links = dom.links(None);
    assert_eq!(links[0].href, "/first");
}

#[test]
fn links_carry_their_element() {
    let dom = parse("<p><a href=/x rel=nofollow class=c>x</a></p>");
    let links = dom.links(None);
    assert_eq!(links[0].node.attr("rel"), Some("nofollow"));
    assert_eq!(links[0].node.parent().and_then(|p| p.tag_name()), Some("p"));
}

#[test]
fn removed_links_are_skipped() {
    let mut dom = parse("<nav><a href=/n>nav</a></nav><a href=/k>keep</a>");
    let nav = dom.find_by_tag("nav").next().unwrap().id();
    dom.remove(nav);
    let links = dom.links(None);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].text, "keep");
}

#[test]
fn page_fixture_links() {
    let dom = parse(include_str!("pages/blog.html"));
    let links = dom.links(Some("https://blog.example/posts/tiny-parser"));
    assert_eq!(links.len(), 11);
    assert_eq!(links[0].url, "https://blog.example/");
    assert_eq!(links[0].text, "The Blog"); // logo image alt
    assert!(
        links
            .iter()
            .any(|l| l.url == "mailto:hi@example.com" && l.text.is_empty())
    );
    assert!(
        links
            .iter()
            .any(|l| l.url == "https://blog.example/posts/arena-trees"
                && l.text == "Arena trees in Rust")
    );
}

// --- resources ---------------------------------------------------------------

fn resources(html: &str, base: Option<&str>) -> Vec<(String, &'static str, String)> {
    parse(html)
        .resources(base)
        .into_iter()
        .map(|r| (r.node.tag_name().unwrap_or("").to_string(), r.attr, r.url))
        .collect()
}

fn res(tag: &str, attr: &'static str, url: &str) -> (String, &'static str, String) {
    (tag.to_string(), attr, url.to_string())
}

#[test]
fn resources_in_document_order() {
    let html = r#"<head><link rel=stylesheet href=s.css><link rel=icon href=/f.ico>
        <script src=app.js></script></head>
        <body><img src=a.png srcset="a1.png 1x, a2.png 2x" alt=a>
        <picture><source srcset="https://c.io/c_fill,w_300/p.webp 300w, https://c.io/c_fill,w_600/p.webp 600w"><img src=p.jpg></picture>
        <video src=v.mp4 poster=poster.jpg><track src=subs.vtt></video>
        <iframe src="//embed.example/x"></iframe><object data=movie.swf></object>
        <a href=not-a-resource.html>x</a><input type=image src=btn.png></body>"#;
    assert_eq!(
        resources(html, Some("https://x.dev/d/")),
        [
            res("link", "href", "https://x.dev/d/s.css"),
            res("link", "href", "https://x.dev/f.ico"),
            res("script", "src", "https://x.dev/d/app.js"),
            res("img", "src", "https://x.dev/d/a.png"),
            res("img", "srcset", "https://x.dev/d/a1.png"),
            res("img", "srcset", "https://x.dev/d/a2.png"),
            res("source", "srcset", "https://c.io/c_fill,w_300/p.webp"),
            res("source", "srcset", "https://c.io/c_fill,w_600/p.webp"),
            res("img", "src", "https://x.dev/d/p.jpg"),
            res("video", "src", "https://x.dev/d/v.mp4"),
            res("video", "poster", "https://x.dev/d/poster.jpg"),
            res("track", "src", "https://x.dev/d/subs.vtt"),
            res("iframe", "src", "https://embed.example/x"),
            res("object", "data", "https://x.dev/d/movie.swf"),
            res("input", "src", "https://x.dev/d/btn.png"),
        ]
    );
}

#[test]
fn resource_values_are_raw() {
    let dom = parse(r#"<img srcset=" a.png 1x,b.png 2x,"><img src=" c.png ">"#);
    let values: Vec<_> = dom.resources(None).iter().map(|r| r.value).collect();
    assert_eq!(values, ["a.png", "b.png", " c.png "]);
}

#[test]
fn empty_and_script_resources_are_skipped() {
    let html = "<img src=''><img src='  '><iframe src='javascript:alert(1)'></iframe>\
                <object data=x.svg></object><div data=not-object></div><img srcset=', ,'>";
    assert_eq!(resources(html, None), [res("object", "data", "x.svg")]);
}

#[test]
fn resources_honour_base_element() {
    let html = "<base href='https://static.dev/v1/'><img src=i.png>";
    assert_eq!(
        resources(html, None),
        [res("img", "src", "https://static.dev/v1/i.png")]
    );
}

// --- robustness ----------------------------------------------------------------

#[test]
fn deeply_nested_links_stay_linear() {
    let depth = 50_000;
    let html = "<a href=/x>t ".repeat(depth);
    let dom = parse(&html);
    let links = dom.links(Some("https://x.dev/"));
    assert_eq!(links.len(), depth);
    assert!(links.iter().all(|l| l.text == "t"));
}

#[test]
fn many_links_and_resources() {
    let html = "<a href=/x>x</a><img src=/i srcset='a 1x, b 2x'>".repeat(20_000);
    let dom = parse(&html);
    assert_eq!(dom.links(None).len(), 20_000);
    assert_eq!(dom.resources(None).len(), 60_000);
}
