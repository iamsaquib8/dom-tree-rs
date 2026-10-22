//! `#[derive(FromHtml)]` on arbitrary markup: extraction never panics, keeps
//! its container promises (`Vec` = every match, `Option` = the first match if
//! any), and stays iterative on very deep documents.

use domq::{Dom, ExtractError, FromHtml, parse};
use proptest::prelude::*;

fn price(value: &str) -> Result<f64, &'static str> {
    value
        .trim()
        .trim_start_matches('$')
        .parse()
        .map_err(|_| "not a price")
}

#[derive(FromHtml, Debug)]
#[allow(dead_code)]
struct Page {
    #[html(select = "h1")]
    title: Option<String>,
    #[html(select = "a", attr = "href")]
    first_link: Option<String>,
    #[html(select = "a[href]", attr = "href")]
    links: Vec<String>,
    #[html(select = "li")]
    items: Vec<String>,
    #[html(select = ".n", attr = "data-n", default)]
    n: i64,
    #[html(select = "b", with = "price")]
    price: Option<f64>,
    #[html(select = "div", html)]
    div_html: Option<String>,
    #[html(select = "div", outer_html)]
    div_outer: Vec<String>,
    #[html(select = ".card")]
    cards: Vec<Card>,
    #[html(skip)]
    skipped: bool,
}

#[derive(FromHtml, Debug)]
#[html(select = ".card")]
#[allow(dead_code)]
struct Card {
    #[html(select = "h2")]
    title: String,
    #[html(attr = "data-id")]
    id: u32,
    #[html(select = "p")]
    body: Option<String>,
    #[html(select = "li")]
    rows: Vec<u8>,
    #[html()]
    text: String,
}

#[derive(FromHtml, Debug)]
#[allow(dead_code)]
struct Texts {
    #[html(select = "h1")]
    first_h1: Option<String>,
    #[html(select = "li")]
    items: Vec<String>,
}

/// Markup soup biased towards what the structs above select.
fn html_soup() -> impl Strategy<Value = String> {
    let pieces = vec![
        "<h1>",
        "</h1>",
        "<h2>",
        "</h2>",
        "<a href=/x>",
        "<a>",
        "</a>",
        "<ul>",
        "<li>",
        "</li>",
        "<b>",
        "$1.50",
        "</b>",
        "<div>",
        "</div>",
        "<div class=card data-id=7>",
        "<div class=card data-id=x>",
        "<section class=card>",
        "<p>",
        "</p>",
        "<span class=n data-n=-4>",
        "<span class=n data-n=nine>",
        "<span class=n>",
        "<script>",
        "</script>",
        "<table><tr><td>",
        "12",
        "300",
        "text",
        " ",
        "\n",
        "&amp;",
        "&#0;",
        "🦀",
        "<",
        ">",
        "\"",
    ];
    proptest::collection::vec(proptest::sample::select(pieces), 0..100).prop_map(|v| v.concat())
}

/// Raw characters, as in `tests/properties.rs`.
fn char_soup() -> impl Strategy<Value = String> {
    let alphabet = vec![
        '<', '>', '/', '=', '"', '\'', ' ', '\n', '!', '-', '&', ';', '#', 'a', 'b', 'h', '1', 'l',
        'i', 'p', '\u{0}', '\u{a0}', '🦀',
    ];
    proptest::collection::vec(proptest::sample::select(alphabet), 0..300)
        .prop_map(|v| v.into_iter().collect())
}

/// Every way of reading a document: from the document, every element, and
/// every `.card`. Results are discarded; only panics matter.
fn exercise(dom: &Dom) {
    let check = |r: Result<(), ExtractError>| {
        if let Err(e) = r {
            // Errors always render, and say what went wrong.
            assert!(!e.to_string().is_empty());
        }
    };
    check(dom.extract::<Page>().map(drop));
    check(dom.extract::<Card>().map(drop));
    check(dom.extract_all::<Card>().map(drop));
    check(dom.extract::<u8>().map(drop));
    for node in dom.nodes() {
        check(node.extract::<Page>().map(drop));
        check(node.extract::<Card>().map(drop));
        check(node.extract::<f32>().map(drop));
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    #[test]
    fn never_panics_on_markup_soup(html in html_soup()) {
        exercise(&parse(&html));
    }

    #[test]
    fn never_panics_on_char_soup(html in char_soup()) {
        exercise(&parse(&html));
    }

    #[test]
    fn containers_keep_their_promises(html in html_soup()) {
        let dom = parse(&html);
        // Text fields cannot fail, so this always succeeds.
        let texts: Texts = dom.extract().unwrap();
        let li: Vec<String> = dom.select("li").map(|n| n.to_text()).collect();
        prop_assert_eq!(texts.items, li);
        prop_assert_eq!(texts.first_h1, dom.select_first("h1").map(|n| n.to_text()));

        // `extract_all` reads exactly one item per match, or fails.
        if let Ok(cards) = dom.extract_all::<Card>() {
            prop_assert_eq!(cards.len(), dom.select(".card").count());
        }
    }
}

#[test]
fn deep_documents_do_not_overflow() {
    let depth = 100_000;
    let html = format!(
        "{}<h1>deep</h1><li>1</li>{}",
        "<div class=card data-id=1>".repeat(depth),
        "</div>".repeat(depth)
    );
    let dom = parse(&html);
    let texts: Texts = dom.extract().unwrap();
    assert_eq!(texts.first_h1.as_deref(), Some("deep"));
    assert_eq!(texts.items, ["1"]);
    // Every card fails on its missing <h2>, after reading the deep text.
    let err = dom.extract_all::<Card>().unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"Card[0].title ("h2"): no element matches"#
    );
}
