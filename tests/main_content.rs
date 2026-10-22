//! Main-content extraction (`Dom::main_content`) on realistic pages
//! (`tests/pages/reader/`) and on focused edge cases.

use domq::{Dom, NodeRef, parse};

/// The chosen element's readable output: Markdown when available, else text.
fn render(node: NodeRef<'_>) -> String {
    #[cfg(feature = "markdown")]
    return node.to_markdown();
    #[cfg(not(feature = "markdown"))]
    return node.to_text();
}

/// Assert the chosen element contains every `keep` and none of `drop`.
fn check(dom: &Dom, keep: &[&str], drop: &[&str]) -> String {
    let node = dom.main_content().expect("main content");
    let out = render(node);
    for k in keep {
        assert!(out.contains(k), "missing {k:?} in:\n{out}");
    }
    for d in drop {
        assert!(!out.contains(d), "chrome {d:?} leaked into:\n{out}");
    }
    out
}

// --- fixture pages -----------------------------------------------------------

#[test]
fn blog_with_sidebar_and_comments() {
    // WordPress-style markup with divs only: no <article>/<main> shortcut, so
    // this exercises paragraph scoring. `.entry-footer` (boilerplate) next to
    // the body stops the widening at `.entry-content`.
    let dom = parse(include_str!("pages/reader/blog-comments.html"));
    let main = dom.main_content().unwrap();
    assert_eq!(main.attr("class"), Some("entry-content"));
    check(
        &dom,
        &[
            "For four years the nightly import job",
            "Measuring before cutting",
            "fn normalise",
            "Peak memory went from 14 GB to 900 MB.",
            "keep the old system running in parallel",
        ],
        &[
            "Archive",                 // site navigation
            "Skip to content",         // skip link
            "On-call without heroics", // post navigation
            "Great write-up",          // a comment
            "Leave a Reply",           // the comment form
            "Recent Posts",            // sidebar widget
            "Field Notes is written",  // sidebar text widget
            "Proudly powered",         // footer
            "Posted in",               // entry footer
        ],
    );
}

#[test]
fn news_article_with_related_links() {
    let dom = parse(include_str!("pages/reader/news.html"));
    let main = dom.main_content().unwrap();
    assert_eq!(main.tag_name(), Some("article"));
    assert_eq!(main.attr("class"), Some("story"));
    check(
        &dom,
        &[
            "City council approves night-bus pilot",
            "voted 31 to 9",
            "What happens next",
            "before deciding whether to make the service permanent",
        ],
        &[
            "We use cookies",          // cookie banner
            "Politics",                // section navigation
            "Advertisement",           // ad slot
            "Harbour ferry timetable", // related stories
            "Central library extends", // "more from" cards
            "All rights reserved",     // footer
            "Search the Tribune",      // search form
        ],
    );
}

#[test]
fn docs_page_with_nav_tree() {
    let dom = parse(include_str!("pages/reader/docs.html"));
    let main = dom.main_content().unwrap();
    assert_eq!(main.tag_name(), Some("main"));
    let out = check(
        &dom,
        &[
            "Connection pooling",
            "Configuring the pool",
            "max_size = 16",
            "acquire_timeout",
            "Health checks",
        ],
        &[
            "Writing queries", // sidebar nav tree
            "Testing with a real database",
            "On this page", // table of contents
            "Apache-2.0",   // footer
            "v4.2",         // top bar
        ],
    );
    #[cfg(feature = "markdown")]
    assert!(out.starts_with("# Connection pooling\n"), "{out}");
    let _ = out;
}

#[test]
fn product_page() {
    let dom = parse(include_str!("pages/reader/product.html"));
    let main = dom.main_content().unwrap();
    assert_eq!(main.attr("id"), Some("product-4417"));
    check(
        &dom,
        &[
            "Trailhead 28L Daypack",
            "$129.00",
            "ventilated, suspended mesh back panel",
            "Hydration sleeve",
            "Recycled nylon ripstop",
        ],
        &[
            "Free shipping",       // announcement bar
            "Tents",               // menu
            "You might also like", // related products
            "Summit 40L",
            "Careers",                     // footer
            "trail reports",               // newsletter
            "Carried it on a twelve-hour", // reviews are a separate section
        ],
    );
}

#[test]
fn page_of_bare_paragraphs() {
    let dom = parse(include_str!("pages/reader/paragraphs.html"));
    let main = dom.main_content().unwrap();
    assert_eq!(main.tag_name(), Some("body"));
    let out = check(
        &dom,
        &["The keeper's day began", "Automation arrived slowly"],
        &["Notes on lighthouse keeping"], // the <title>
    );
    assert_eq!(out.matches("keeper").count(), 4, "{out}");
}

#[test]
fn garbage_page_has_no_main_content() {
    let dom = parse(include_str!("pages/reader/garbage.html"));
    assert!(dom.main_content().is_none());
}

#[test]
fn existing_blog_page() {
    let dom = parse(include_str!("pages/blog.html"));
    let main = dom.main_content().unwrap();
    assert_eq!(main.attr("class"), Some("post"));
    check(
        &dom,
        &["Shipping a", "The core loop", "Kent Beck"],
        &["Archive", "Arena trees in Rust", "RSS"],
    );
}

// --- focused cases -----------------------------------------------------------

const PARA: &str =
    "This paragraph is comfortably longer than the minimum, with a comma, and then some.";

fn paras(n: usize) -> String {
    format!("<p>{PARA}</p>").repeat(n)
}

#[test]
fn empty_and_tiny_documents() {
    for html in [
        "",
        "   ",
        "plain text with no elements at all, quite long but still no element to hold it",
        "<p>short</p>",
        "<!-- only a comment -->",
        "<!doctype html>",
        "<nav><a href=/>Home</a> <a href=/a>About us and our long history</a></nav>",
    ] {
        assert!(parse(html).main_content().is_none(), "{html:?}");
    }
}

#[test]
fn top_level_paragraphs_have_no_single_container() {
    // No element holds all of them: the document itself is the content.
    let dom = parse(&format!(
        "<div class=nav><a href=/>Home</a></div>{}",
        paras(4)
    ));
    assert!(dom.main_content().is_none());
}

#[test]
fn single_article_wins() {
    let html = format!(
        "<div class=wrap><div class=side>{}</div><article id=a>{}</article></div>",
        "<p>Some unrelated but long sidebar text that is not linked anywhere.</p>",
        paras(5)
    );
    let dom = parse(&html);
    assert_eq!(dom.main_content().unwrap().attr("id"), Some("a"));
}

#[test]
fn several_articles_fall_back_to_main() {
    let html = format!(
        "<nav>menu</nav><main id=m><article>{0}</article><article>{0}</article></main>",
        paras(4)
    );
    let dom = parse(&html);
    assert_eq!(dom.main_content().unwrap().attr("id"), Some("m"));
}

#[test]
fn nested_comment_articles_do_not_count() {
    // HTML5 comments are often <article>s inside the post's <article>.
    let html = format!(
        "<article id=post>{}<section class=comments><article>{}</article><article>{}</article></section></article>",
        paras(8),
        paras(1),
        paras(1)
    );
    let dom = parse(&html);
    assert_eq!(dom.main_content().unwrap().attr("id"), Some("post"));
}

#[test]
fn role_main_counts_as_main() {
    let html = format!(
        "<div role=navigation>{}</div><div role=main id=r>{}</div>",
        "<a href=/x>x</a>",
        paras(4)
    );
    let dom = parse(&html);
    assert_eq!(dom.main_content().unwrap().attr("id"), Some("r"));
}

#[test]
fn article_inside_boilerplate_is_ignored() {
    let html = format!(
        "<body><aside><article id=promo>{}</article></aside><div id=story>{}</div></body>",
        paras(3),
        paras(6)
    );
    let dom = parse(&html);
    assert_eq!(dom.main_content().unwrap().attr("id"), Some("story"));
}

#[test]
fn link_heavy_blocks_lose() {
    let links = r#"<li><a href="/a/very/long/link/target">A very long link title to another story</a></li>"#;
    let html = format!(
        "<body><div id=links><ul>{}</ul></div><div id=text>{}</div></body>",
        links.repeat(20),
        paras(3)
    );
    let dom = parse(&html);
    assert_eq!(dom.main_content().unwrap().attr("id"), Some("text"));
}

#[test]
fn class_hints_steer_the_choice() {
    // Same amount of text; the boilerplate class loses, the content class wins.
    let html = format!(
        "<body><div class=comments id=c>{0}</div><div class=post-body id=p>{0}</div></body>",
        paras(4)
    );
    let dom = parse(&html);
    assert_eq!(dom.main_content().unwrap().attr("id"), Some("p"));
}

#[test]
fn hidden_text_is_ignored() {
    let html = format!(
        "<body><div id=h1 hidden>{0}{0}{0}</div><div id=h2 aria-hidden=true>{0}{0}{0}</div>\
         <div id=h3 style='color: red; DISPLAY : none'>{0}{0}{0}</div>\
         <noscript>{0}{0}{0}</noscript><div id=v>{0}</div></body>",
        format!("<p>{PARA}</p>")
    );
    let dom = parse(&html);
    assert_eq!(dom.main_content().unwrap().attr("id"), Some("v"));
}

#[test]
fn boilerplate_only_pages_still_get_an_answer() {
    // Nothing outside boilerplate: the second pass allows it (the class
    // penalty still makes `<body>`, with the same text, the winner here).
    let html = format!(
        "<body><div class=sidebar-layout id=s>{}</div></body>",
        paras(3)
    );
    let dom = parse(&html);
    let main = dom.main_content().unwrap();
    assert_eq!(main.tag_name(), Some("body"));
    assert!(main.to_text().contains("comfortably longer"));

    let html = format!("<body><nav id=n><div>{}</div></nav></body>", paras(3));
    let dom = parse(&html);
    assert!(dom.main_content().is_some());
}

#[test]
fn widens_to_include_the_title() {
    let html = format!(
        "<body><div id=post><h1>Title</h1><p class=byline>By Someone Important, yesterday</p>\
         <div class=body>{}</div></div><div id=comments>{}</div></body>",
        paras(5),
        paras(2)
    );
    let dom = parse(&html);
    let main = dom.main_content().unwrap();
    assert_eq!(main.attr("id"), Some("post"));
    assert!(main.to_text().starts_with("Title"));
}

#[test]
fn removed_nodes_are_ignored() {
    let mut dom = parse(&format!(
        "<body><div id=a>{}</div><div id=b>{}</div></body>",
        paras(6),
        paras(3)
    ));
    assert_eq!(dom.main_content().unwrap().attr("id"), Some("a"));
    let a = dom.find_by_id("a").unwrap().id();
    dom.remove(a);
    assert_eq!(dom.main_content().unwrap().attr("id"), Some("b"));
}

#[test]
fn is_deterministic() {
    let html = include_str!("pages/reader/blog-comments.html");
    let first = parse(html).main_content().map(|n| n.id());
    for _ in 0..5 {
        assert_eq!(parse(html).main_content().map(|n| n.id()), first);
    }
}

#[test]
fn deep_nesting_is_fine() {
    let depth = 100_000;
    let mut html = "<div>".repeat(depth);
    html.push_str(&paras(3));
    let dom = parse(&html);
    let main = dom.main_content().unwrap();
    assert!(main.to_text().contains("comfortably longer"));

    // Deeply nested paragraph-like elements and articles.
    let dom = parse(&"<article><li><p>x, y, z and more words to pass the minimum".repeat(10_000));
    let _ = dom.main_content();
}

#[test]
fn wide_documents_are_fine() {
    let html = format!("<body>{}</body>", "<div><p>a, b</p></div>".repeat(50_000));
    let _ = parse(&html).main_content();
}
