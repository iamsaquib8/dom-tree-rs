//! `Dom::metadata`: sources, fallbacks and robustness.

use domtree::{Metadata, parse};

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn blog_fixture() {
    let meta = parse(include_str!("pages/reader/blog-comments.html")).metadata();
    assert_eq!(
        meta.title,
        s("Why we rewrote the importer in Rust – Field Notes")
    );
    assert_eq!(
        meta.description,
        s("Three months, one importer, and what changed when the hot path stopped allocating.")
    );
    assert_eq!(
        meta.canonical,
        s("https://fieldnotes.example/2026/03/why-we-rewrote-the-importer/")
    );
    assert_eq!(meta.lang, s("en-GB"));
    assert_eq!(meta.site_name, s("Field Notes"));
    assert_eq!(
        meta.image,
        s("https://fieldnotes.example/wp-content/uploads/2026/03/flame.png")
    );
    // `article:author` is a URL here, so the byline link's text wins.
    assert_eq!(meta.author, s("Maya Lindqvist"));
    assert_eq!(meta.published, s("2026-03-14T09:30:00+00:00"));
    assert_eq!(meta.keywords, ["rust", "performance"]); // from article:tag
    assert_eq!(meta.og.len(), 8);
    assert_eq!(meta.og[0], ("og:type".to_string(), "article".to_string()));
    assert_eq!(
        meta.twitter,
        pairs(&[
            ("twitter:card", "summary_large_image"),
            ("twitter:creator", "@maya")
        ])
    );
    assert_eq!(meta.json_ld.len(), 1);
    assert!(
        meta.json_ld[0].starts_with(r#"{"@context":"https://schema.org","@type":"BlogPosting""#)
    );
    // Two RSS feeds; the `application/json` REST link is not a feed.
    assert_eq!(
        meta.feeds,
        [
            "https://fieldnotes.example/feed/",
            "https://fieldnotes.example/comments/feed/"
        ]
    );
}

#[test]
fn news_fixture() {
    let meta = parse(include_str!("pages/reader/news.html")).metadata();
    assert_eq!(
        meta.title,
        s("City council approves night-bus pilot | The Harbour Tribune")
    );
    assert_eq!(meta.author, s("Lena Okafor"));
    assert_eq!(meta.keywords, ["transport", "night bus", "city council"]);
    // No canonical link: og:url.
    assert_eq!(
        meta.canonical,
        s("https://tribune.example/local/2026/04/02/night-bus-pilot")
    );
    // No article:published_time: the <time> in the <article>.
    assert_eq!(meta.published, s("2026-04-02T18:15:00Z"));
    assert_eq!(meta.json_ld.len(), 1);
    assert!(meta.json_ld[0].starts_with('{') && meta.json_ld[0].ends_with('}'));
    assert!(meta.feeds.is_empty());
}

#[test]
fn empty_documents() {
    assert_eq!(parse("").metadata(), Metadata::default());
    assert_eq!(parse("<p>just text</p>").metadata(), Metadata::default());
}

#[test]
fn title_fallbacks() {
    let og = r#"<meta property="og:title" content="OG title"><meta name="twitter:title" content="TW title">"#;
    assert_eq!(parse(og).metadata().title, s("OG title"));
    assert_eq!(
        parse(r#"<meta name="twitter:title" content="TW title">"#)
            .metadata()
            .title,
        s("TW title")
    );
    // An empty <title> falls through to the next one / og:title.
    assert_eq!(
        parse(&format!("<title>  </title>{og}")).metadata().title,
        s("OG title")
    );
    // <title> inside <svg> is an SVG tooltip, not the page title.
    assert_eq!(
        parse("<body><svg><title>Icon</title></svg><title>Page</title></body>")
            .metadata()
            .title,
        s("Page")
    );
    assert_eq!(
        parse("<title>First</title><title>Second</title>")
            .metadata()
            .title,
        s("First")
    );
    assert_eq!(
        parse("<title>\n  Spread \t over\n lines &amp; entities </title>")
            .metadata()
            .title,
        s("Spread over lines & entities")
    );
}

#[test]
fn description_lang_site_image_fallbacks() {
    let meta = parse(
        r#"<meta property="og:description" content="From OG">
           <meta http-equiv="Content-Language" content="de">
           <meta name="application-name" content="My App">
           <meta name="twitter:image:src" content="/tw.png">"#,
    )
    .metadata();
    assert_eq!(meta.description, s("From OG"));
    assert_eq!(meta.lang, s("de"));
    assert_eq!(meta.site_name, s("My App"));
    assert_eq!(meta.image, s("/tw.png"));

    let meta = parse(r#"<link rel="image_src" href="/thumb.jpg"><html lang=" fr-CA ">"#).metadata();
    assert_eq!(meta.image, s("/thumb.jpg"));
    assert_eq!(meta.lang, s("fr-CA"));

    let meta =
        parse(r#"<meta property="og:image:secure_url" content="https://x/s.png">"#).metadata();
    assert_eq!(meta.image, s("https://x/s.png"));
}

#[test]
fn names_and_properties_are_case_insensitive() {
    let meta = parse(
        r#"<META NAME="Description" CONTENT="Upper">
           <meta property="OG:Site_Name" content="Site">
           <meta name="Twitter:Card" content="summary">
           <LINK REL="Canonical Alternate" HREF="/c">"#,
    )
    .metadata();
    assert_eq!(meta.description, s("Upper"));
    assert_eq!(meta.site_name, s("Site"));
    assert_eq!(meta.og, pairs(&[("og:site_name", "Site")]));
    assert_eq!(meta.twitter, pairs(&[("twitter:card", "summary")]));
    assert_eq!(meta.canonical, s("/c"));
}

#[test]
fn first_value_wins_and_empty_values_are_ignored() {
    let meta = parse(
        r#"<meta name="description" content="  ">
           <meta name="description" content="Second">
           <meta name="description" content="Third">
           <link rel=canonical href=""><link rel=canonical href="/real">"#,
    )
    .metadata();
    assert_eq!(meta.description, s("Second"));
    assert_eq!(meta.canonical, s("/real"));
}

#[test]
fn og_and_twitter_collect_every_pair() {
    let meta = parse(
        r#"<meta property="og:image" content="/1.png">
           <meta property="og:image" content="/2.png">
           <meta property="article:section" content="Tech">
           <meta property="book:isbn" content="123">
           <meta property="fb:app_id" content="999">
           <meta name="og:locale" content="en_US">
           <meta property="twitter:site" content="@a">
           <meta name="twitter:site" content="@b">"#,
    )
    .metadata();
    assert_eq!(
        meta.og,
        pairs(&[
            ("og:image", "/1.png"),
            ("og:image", "/2.png"),
            ("article:section", "Tech"),
            ("book:isbn", "123"),
            ("og:locale", "en_US"),
        ])
    );
    assert_eq!(
        meta.twitter,
        pairs(&[("twitter:site", "@a"), ("twitter:site", "@b")])
    );
    assert_eq!(meta.image, s("/1.png"));
}

#[test]
fn author_sources() {
    assert_eq!(
        parse(r#"<meta name=author content="Ann"><a rel=author>Bob</a>"#)
            .metadata()
            .author,
        s("Ann")
    );
    assert_eq!(
        parse(r#"<meta property="article:author" content="Cy Young">"#)
            .metadata()
            .author,
        s("Cy Young")
    );
    assert_eq!(
        parse(r#"<meta property="article:author" content="https://x/cy">"#)
            .metadata()
            .author,
        s("https://x/cy")
    );
    assert_eq!(
        parse(r#"<p>By <a href=/d rel="author external"> Dee  Dee </a></p>"#)
            .metadata()
            .author,
        s("Dee Dee")
    );
}

#[test]
fn published_sources() {
    let date = |html: &str| parse(html).metadata().published;
    assert_eq!(
        date(
            r#"<meta property="article:published_time" content="2026-01-01"><meta name=date content="2020">"#
        ),
        s("2026-01-01")
    );
    assert_eq!(
        date(r#"<meta itemprop="datePublished" content="2026-02-02">"#),
        s("2026-02-02")
    );
    assert_eq!(
        date(r#"<meta name="DC.date.issued" content="2026-03-03">"#),
        s("2026-03-03")
    );
    assert_eq!(
        date(r#"<meta name="pubdate" content="20260404">"#),
        s("20260404")
    );
    // <time>: only inside an <article>, or marked as the publication date.
    assert_eq!(
        date(r#"<footer><time datetime="1999">x</time></footer>"#),
        None
    );
    assert_eq!(
        date(
            r#"<time datetime="1999">x</time><article><time datetime="2026-05-05">May</time></article>"#
        ),
        s("2026-05-05")
    );
    assert_eq!(
        date(r#"<time pubdate datetime="2026-06-06">x</time>"#),
        s("2026-06-06")
    );
    assert_eq!(
        date(r#"<span><time itemprop="datePublished" datetime="2026-07-07">x</time></span>"#),
        s("2026-07-07")
    );
}

#[test]
fn keywords() {
    assert_eq!(
        parse(r#"<meta name=keywords content=" a, b ,,c , ">"#)
            .metadata()
            .keywords,
        ["a", "b", "c"]
    );
    assert_eq!(
        parse(r#"<meta name=news_keywords content="x,y">"#)
            .metadata()
            .keywords,
        ["x", "y"]
    );
}

#[test]
fn json_ld_blocks() {
    let meta = parse(
        r#"<script type="application/ld+json">
             {"@type": "Person"}
           </script>
           <script type="Application/LD+JSON; charset=utf-8">[1]</script>
           <script type="application/ld+json">   </script>
           <script type="application/json">{"not": "ld"}</script>
           <script>var x = 1;</script>"#,
    )
    .metadata();
    assert_eq!(meta.json_ld, [r#"{"@type": "Person"}"#, "[1]"]);
}

#[test]
fn feeds() {
    let meta = parse(
        r#"<link rel=alternate type="application/atom+xml" href="/atom.xml">
           <link rel="alternate" type="application/feed+json" href="/feed.json">
           <link rel="alternate" type="APPLICATION/RSS+XML; charset=utf-8" href="/rss">
           <link rel="alternate" hreflang="de" href="/de/">
           <link rel="stylesheet" type="application/rss+xml" href="/odd">"#,
    )
    .metadata();
    assert_eq!(meta.feeds, ["/atom.xml", "/feed.json", "/rss"]);
}

#[test]
fn metadata_outside_head_and_in_templates() {
    // This parser keeps misplaced tags where they are: they still count. But
    // <template> content is inert.
    let meta = parse(
        r#"<body><meta name=description content="In body">
           <template><title>Template</title><meta name=author content="T"></template></body>"#,
    )
    .metadata();
    assert_eq!(meta.description, s("In body"));
    assert_eq!(meta.title, None);
    assert_eq!(meta.author, None);
}

#[test]
fn metadata_is_owned() {
    let meta = {
        let dom = parse("<title>Owned</title>");
        dom.metadata()
    };
    assert_eq!(meta.title, s("Owned"));
}

#[test]
fn many_meta_tags_stay_linear() {
    let html = r#"<meta name="x-custom" content="v"><meta property="og:image" content="/i">"#
        .repeat(50_000);
    let meta = parse(&html).metadata();
    assert_eq!(meta.og.len(), 50_000);
    assert_eq!(meta.description, None);
}

#[test]
fn deep_svg_and_articles() {
    let depth = 100_000;
    let html = format!(
        "{}<title>t</title>{}",
        "<svg>".repeat(depth),
        "<article><time datetime=d>".repeat(depth)
    );
    let meta = parse(&html).metadata();
    assert_eq!(meta.title, None);
}

#[cfg(feature = "serde")]
#[test]
fn serializes_with_serde() {
    let meta =
        parse(r#"<html lang=en><title>T</title><meta property="og:type" content="website">"#)
            .metadata();
    let json = serde_json::to_value(&meta).unwrap();
    assert_eq!(json["title"], "T");
    assert_eq!(json["lang"], "en");
    assert_eq!(json["description"], serde_json::Value::Null);
    assert_eq!(json["og"][0][0], "og:type");
    assert_eq!(json["og"][0][1], "website");
    assert_eq!(json["feeds"], serde_json::json!([]));
    assert_eq!(json.as_object().unwrap().len(), 13);
}
