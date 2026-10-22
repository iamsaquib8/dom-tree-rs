//! `#[derive(FromHtml)]` and the extraction runtime: a realistic product page,
//! listings through `extract_all`, nested structs, every container, source and
//! modifier, and the exact text of every error.

use domq::{
    Dom, ExtractError, ExtractErrorKind, FromHtml, FromHtmlValue, Scope, ValueSource, parse,
};

const PRODUCT_PAGE: &str = r#"<!doctype html>
<html lang="en">
<head>
  <title>Acme Anvil | Acme Store</title>
  <meta name="description" content="A very heavy anvil.">
</head>
<body>
  <nav><a href="/">Home</a> &rsaquo; <a href="/tools">Tools</a></nav>
  <main>
    <article class="product" data-sku="ANV-1000" data-in-stock="TRUE">
      <h1 class="title">  Acme   Anvil </h1>
      <div class="price" data-value="149.99" data-currency="USD">$149.99</div>
      <div class="rating" data-stars="4.5">&#9733;&#9733;&#9733;&#9733;&#189;</div>
      <ul class="tags"><li>tools</li><li> heavy </li><li>classic</li></ul>
      <div class="description"><p>Forged from <b>solid</b> steel.</p><p>Drop-tested.</p></div>
      <div class="seller" data-id="77">
        <span class="name">Wile E. Supplies</span>
        <a class="profile" href="/sellers/77">profile</a>
      </div>
      <img class="hero" src="/img/anvil.jpg" alt="An anvil">
      <section class="reviews">
        <div class="review"><span class="author">Road Runner</span>
          <span class="stars" data-value="5">5/5</span><p class="body">Beep beep!</p></div>
        <div class="review"><span class="author">Coyote</span>
          <span class="stars" data-value="1">1/5</span><p class="body">It fell on me.</p></div>
        <div class="review"><span class="author">Bugs</span>
          <span class="stars" data-value="4">4/5</span></div>
      </section>
    </article>
  </main>
</body>
</html>"#;

#[derive(FromHtml, Debug, PartialEq)]
struct Product {
    #[html(select = "h1.title")]
    name: String,
    #[html(select = ".product", attr = "data-sku")]
    sku: String,
    #[html(select = ".price", attr = "data-value")]
    price: f64,
    #[html(select = ".price", attr = "data-currency")]
    currency: String,
    #[html(select = ".product", attr = "data-in-stock")]
    in_stock: bool,
    #[html(select = ".rating", attr = "data-stars")]
    rating: Option<f32>,
    #[html(select = ".tags li")]
    tags: Vec<String>,
    #[html(select = ".description", markdown)]
    description: String,
    #[html(select = ".seller")]
    seller: Seller,
    #[html(select = ".review")]
    reviews: Vec<Review>,
    #[html(select = "img.hero", attr = "src")]
    image: Option<String>,
    #[html(select = ".discount")]
    discount: Option<String>,
    #[html(skip)]
    fetched_at: u64,
}

#[derive(FromHtml, Debug, PartialEq)]
struct Seller {
    #[html(attr = "data-id")]
    id: u32,
    #[html(select = ".name")]
    name: String,
    #[html(select = "a.profile", attr = "href")]
    profile: String,
}

#[derive(FromHtml, Debug, PartialEq)]
struct Review {
    #[html(select = ".author")]
    author: String,
    #[html(select = ".stars", attr = "data-value")]
    stars: u8,
    #[html(select = ".body")]
    body: Option<String>,
}

fn review(author: &str, stars: u8, body: Option<&str>) -> Review {
    Review {
        author: author.into(),
        stars,
        body: body.map(Into::into),
    }
}

#[test]
fn realistic_product_page() {
    let product: Product = parse(PRODUCT_PAGE).extract().unwrap();
    assert_eq!(
        product,
        Product {
            name: "Acme Anvil".into(),
            sku: "ANV-1000".into(),
            price: 149.99,
            currency: "USD".into(),
            in_stock: true,
            rating: Some(4.5),
            tags: vec!["tools".into(), "heavy".into(), "classic".into()],
            description: "Forged from **solid** steel.\n\nDrop-tested.".into(),
            seller: Seller {
                id: 77,
                name: "Wile E. Supplies".into(),
                profile: "/sellers/77".into(),
            },
            reviews: vec![
                review("Road Runner", 5, Some("Beep beep!")),
                review("Coyote", 1, Some("It fell on me.")),
                review("Bugs", 4, None),
            ],
            image: Some("/img/anvil.jpg".into()),
            discount: None,
            fetched_at: 0,
        }
    );
}

// --- listings ------------------------------------------------------------------

const LISTING: &str = r#"
<ul class="results">
  <li class="card" data-id="1"><h2><a href="/p/1">Pen</a></h2><span class="price">1.50</span></li>
  <li class="card" data-id="2"><h2><a href="/p/2">Ink</a></h2><span class="price">4</span>
      <span class="badge">new</span></li>
  <li class="card" data-id="3"><h2><a href="/p/3">Pad</a></h2><span class="price">2.25</span></li>
</ul>"#;

#[derive(FromHtml, Debug, PartialEq)]
#[html(select = "li.card")]
struct Card {
    #[html(attr = "data-id")]
    id: u32,
    #[html(select = "h2")]
    title: String,
    #[html(select = "h2 a", attr = "href")]
    url: String,
    #[html(select = ".price")]
    price: f64,
    #[html(select = ".badge")]
    badge: Option<String>,
}

#[test]
fn listing_with_extract_all() {
    let dom = parse(LISTING);
    let cards: Vec<Card> = dom.extract_all().unwrap();
    assert_eq!(cards.len(), 3);
    assert_eq!(
        cards[1],
        Card {
            id: 2,
            title: "Ink".into(),
            url: "/p/2".into(),
            price: 4.0,
            badge: Some("new".into()),
        }
    );
    let titles: Vec<_> = cards.iter().map(|c| c.title.as_str()).collect();
    assert_eq!(titles, ["Pen", "Ink", "Pad"]);
    assert_eq!(Card::SELECTOR, Some("li.card"));

    // No match is an empty list, not an error.
    assert_eq!(parse("<p>nothing</p>").extract_all::<Card>(), Ok(vec![]));
}

#[test]
fn type_selector_in_extract_and_node_extract() {
    let dom = parse(LISTING);
    // `Dom::extract` reads the first match of the type's selector…
    let first: Card = dom.extract().unwrap();
    assert_eq!(first.id, 1);
    // …and `NodeRef::extract` reads the node it is called on.
    let third = dom.select_first("li:nth-child(3)").unwrap();
    assert_eq!(third.extract::<Card>().unwrap().title, "Pad");

    let err = parse("<p>no cards</p>").extract::<Card>().unwrap_err();
    assert_eq!(err.to_string(), r#"Card ("li.card"): no element matches"#);
    assert_eq!(err.kind(), &ExtractErrorKind::NoMatch);
    assert_eq!(err.path(), "Card");
}

#[test]
fn listing_inside_a_page() {
    #[derive(FromHtml)]
    struct Results {
        #[html(select = "li.card")]
        cards: Vec<Card>,
        #[html(select = "li.card h2")]
        titles: Vec<String>,
        #[html(select = "li.card", attr = "data-id")]
        ids: Vec<u32>,
        #[html(select = "table tr")]
        rows: Vec<String>,
    }
    let results: Results = parse(LISTING).extract().unwrap();
    assert_eq!(results.cards.len(), 3);
    assert_eq!(results.titles, ["Pen", "Ink", "Pad"]);
    assert_eq!(results.ids, [1, 2, 3]);
    assert!(results.rows.is_empty());
}

// --- containers and modifiers ------------------------------------------------------

fn parse_cents(value: &str) -> Result<u64, String> {
    let digits: String = value.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return Err(format!("no digits in {value:?}"));
    }
    digits
        .parse()
        .map_err(|e: core::num::ParseIntError| e.to_string())
}

fn upper(value: &str) -> Result<String, core::convert::Infallible> {
    Ok(value.to_uppercase())
}

mod util {
    pub fn words(value: &str) -> Result<usize, &'static str> {
        Ok(value.split_whitespace().count())
    }
}

#[derive(FromHtml, Debug, PartialEq)]
struct Modifiers {
    #[html(select = ".price", with = "parse_cents")]
    cents: u64,
    #[html(select = ".price", attr = "data-was", with = "parse_cents")]
    was: Option<u64>,
    #[html(select = ".tag", with = "upper")]
    tags: Vec<String>,
    #[html(select = "p", with = "util::words")]
    words: usize,
    #[html(select = ".missing", default)]
    missing_text: String,
    #[html(select = ".price", attr = "data-missing", default)]
    missing_attr: u32,
    #[html(select = ".missing", with = "parse_cents", default)]
    missing_with: u64,
    #[html(select = ".count", default)]
    count: u32,
    #[html(skip)]
    skipped: Vec<String>,
    #[html(select = ".price", attr = "data-was")]
    was_raw: Option<String>,
}

#[test]
fn option_default_with_skip() {
    let dom = parse(
        r#"<b class=price data-was="$12.00">$9.99</b><i class=tag>a</i><i class=tag>b c</i>
           <p>three short words</p><span class=count>7</span>"#,
    );
    let m: Modifiers = dom.extract().unwrap();
    assert_eq!(
        m,
        Modifiers {
            cents: 999,
            was: Some(1200),
            tags: vec!["A".into(), "B C".into()],
            words: 3,
            missing_text: String::new(),
            missing_attr: 0,
            missing_with: 0,
            count: 7,
            skipped: vec![],
            was_raw: Some("$12.00".into()),
        }
    );
}

#[test]
fn default_still_reports_parse_errors() {
    #[derive(FromHtml, Debug)]
    struct D {
        #[html(select = ".count", default)]
        _count: u32,
    }
    let err = parse("<span class=count>lots</span>")
        .extract::<D>()
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"D._count (".count"): cannot parse "lots" as u32: invalid digit found in string"#
    );
    assert!(parse("<p>none</p>").extract::<D>().is_ok());
}

#[test]
fn option_is_none_without_match_or_attribute_but_parse_errors_stay() {
    #[derive(FromHtml, Debug, PartialEq)]
    struct O {
        #[html(select = "a", attr = "data-n")]
        n: Option<i32>,
        #[html(select = ".seller")]
        seller: Option<Seller>,
    }
    let none = O {
        n: None,
        seller: None,
    };
    assert_eq!(parse("").extract::<O>(), Ok(none));
    assert_eq!(parse("<a>no attribute</a>").extract::<O>().unwrap().n, None);
    assert_eq!(
        parse("<a data-n=-3>x</a>").extract::<O>().unwrap().n,
        Some(-3)
    );

    let err = parse("<a data-n=x>x</a>").extract::<O>().unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"O.n ("a" @data-n): cannot parse "x" as i32: invalid digit found in string"#
    );
    // An optional nested struct that is present must still extract fully.
    let err = parse("<div class=seller data-id=1><span class=name>N</span></div>")
        .extract::<O>()
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"O.seller.profile ("a.profile"): no element matches"#
    );
}

// --- sources -------------------------------------------------------------------------

#[test]
fn every_source() {
    #[derive(FromHtml, Debug, PartialEq)]
    struct Sources {
        #[html(select = "div")]
        text: String,
        #[html(select = "div", attr = "id")]
        attr: String,
        #[html(select = "div", html)]
        html: String,
        #[html(select = "div", outer_html)]
        outer: String,
        #[html(select = "div", markdown)]
        markdown: String,
        #[html(select = "div", attr = "DATA-Upper")]
        lower_cased_name: String,
        #[html()]
        whole_text: String,
        #[html]
        bare: String,
    }
    let dom =
        parse(r#"<div id=d data-upper=yes><p>Hello <em>there</em></p><ul><li>x</li></ul></div>"#);
    let s: Sources = dom.extract().unwrap();
    assert_eq!(
        s,
        Sources {
            text: "Hello there\n\nx".into(),
            attr: "d".into(),
            html: "<p>Hello <em>there</em></p><ul><li>x</li></ul>".into(),
            outer: r#"<div id="d" data-upper="yes"><p>Hello <em>there</em></p><ul><li>x</li></ul></div>"#
                .into(),
            markdown: "Hello *there*\n\n- x".into(),
            lower_cased_name: "yes".into(),
            whole_text: "Hello there\n\nx".into(),
            bare: "Hello there\n\nx".into(),
        }
    );
}

#[test]
fn attribute_values_are_not_trimmed_for_strings_but_are_for_numbers() {
    #[derive(FromHtml, Debug, PartialEq)]
    struct T {
        #[html(select = "i", attr = "title")]
        raw: String,
        #[html(select = "i", attr = "data-n")]
        n: u8,
        #[html(select = "i", attr = "data-b")]
        b: bool,
        #[html(select = "i", attr = "data-c")]
        c: char,
    }
    let dom = parse(r#"<i title=" padded " data-n=" 8 " data-b=" False " data-c=" é ">x</i>"#);
    assert_eq!(
        dom.extract::<T>(),
        Ok(T {
            raw: " padded ".into(),
            n: 8,
            b: false,
            c: 'é',
        })
    );
}

// --- scope ----------------------------------------------------------------------------

#[test]
fn document_and_element_scope() {
    #[derive(FromHtml, Debug, PartialEq)]
    struct Block {
        #[html(select = "h2")]
        heading: String,
        #[html(select = "p")]
        paragraphs: Vec<String>,
    }
    let dom = parse("<h2>Top</h2><p>a</p><section><h2>Inner</h2><p>b</p><p>c</p></section>");
    // The document: every match in the whole page.
    let page: Block = dom.extract().unwrap();
    assert_eq!(page.heading, "Top");
    assert_eq!(page.paragraphs, ["a", "b", "c"]);
    // An element: only its descendants.
    let section = dom.select_first("section").unwrap();
    let inner: Block = section.extract().unwrap();
    assert_eq!(inner.heading, "Inner");
    assert_eq!(inner.paragraphs, ["b", "c"]);
    assert_eq!(Block::from_html(Scope::Element(section)), Ok(inner));
    assert_eq!(Block::from_html(Scope::from(&dom)), Ok(page));

    // A document has no attributes.
    #[derive(FromHtml, Debug)]
    struct Lang {
        #[html(attr = "lang")]
        _lang: String,
    }
    let err = dom.extract::<Lang>().unwrap_err();
    assert_eq!(
        err.to_string(),
        "Lang._lang (@lang): a document has no attributes; give the field a selector or \
         extract from an element"
    );
    assert_eq!(
        err.kind(),
        &ExtractErrorKind::MissingAttribute {
            name: "lang".into(),
            element: None,
        }
    );
    let html = parse("<html lang=en></html>");
    assert!(html.root().unwrap().extract::<Lang>().is_ok());
}

#[test]
fn leaf_types_read_readable_text() {
    let dom = parse("<p> 12 </p><script>99</script>");
    assert_eq!(dom.extract::<u16>(), Ok(12));
    assert_eq!(dom.extract::<String>(), Ok("12".into()));
    assert_eq!(dom.extract::<f32>(), Ok(12.0));
    let err = parse("<p>twelve</p>").extract::<i8>().unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"cannot parse "twelve" as i8: invalid digit found in string"#
    );
    assert_eq!(err.path(), "");
    assert_eq!(err.selector(), None);
    assert_eq!(err.value_source(), Some(&ValueSource::Text));
    assert_eq!(parse("<b>x</b>").extract::<char>(), Ok('x'));
    assert_eq!(
        parse("<b>yes</b>")
            .extract::<bool>()
            .unwrap_err()
            .to_string(),
        r#"cannot parse "yes" as bool: expected "true" or "false""#
    );
}

// --- nesting, generics, hand-written impls ------------------------------------------------

#[derive(Debug, PartialEq)]
struct Price {
    cents: u64,
    currency: String,
}

impl FromHtmlValue for Price {
    fn from_html_value(value: &str) -> Result<Self, String> {
        let (currency, amount) = value
            .trim()
            .split_once(' ')
            .ok_or_else(|| "expected \"CUR 1.23\"".to_string())?;
        let cents = parse_cents(amount)?;
        Ok(Price {
            cents,
            currency: currency.into(),
        })
    }
}

/// Hand-written: the number of `<li>` in the scope.
#[derive(Debug, PartialEq)]
struct ItemCount(usize);

impl FromHtml for ItemCount {
    fn from_html(scope: Scope<'_>) -> Result<Self, ExtractError> {
        let li = domq::Selector::parse("li").unwrap();
        Ok(ItemCount(scope.select(&li).count()))
    }
}

#[test]
fn custom_value_types_and_hand_written_nested_impls() {
    #[derive(FromHtml, Debug, PartialEq)]
    struct Offer {
        #[html(select = "data", attr = "value")]
        price: Price,
        #[html(select = "ul")]
        items: ItemCount,
        #[html(select = "ul")]
        lists: Vec<ItemCount>,
    }
    let dom = parse(r#"<data value="EUR 3.50">€3.50</data><ul><li>a<li>b</ul><ul></ul>"#);
    assert_eq!(
        dom.extract::<Offer>(),
        Ok(Offer {
            price: Price {
                cents: 350,
                currency: "EUR".into()
            },
            items: ItemCount(2),
            lists: vec![ItemCount(2), ItemCount(0)],
        })
    );
    let err = parse(r#"<data value="3.50">x</data><ul></ul>"#)
        .extract::<Offer>()
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"Offer.price ("data" @value): cannot parse "3.50" as Price: expected "CUR 1.23""#
    );
}

#[test]
fn generic_structs() {
    #[derive(FromHtml, Debug, PartialEq)]
    struct Labeled<T, U = String> {
        #[html(select = "dt")]
        label: U,
        #[html(select = "dd")]
        value: T,
        #[html(select = "dd", attr = "data-alt")]
        alt: Option<T>,
        #[html(select = "dd")]
        all: Vec<T>,
        #[html(skip)]
        _marker: core::marker::PhantomData<U>,
    }
    let dom = parse("<dl><dt>Weight</dt><dd data-alt=2>12</dd><dd>13</dd></dl>");
    let l: Labeled<u32> = dom.extract().unwrap();
    assert_eq!(
        (l.label.as_str(), l.value, l.alt, l.all),
        ("Weight", 12, Some(2), vec![12, 13])
    );
    let l: Labeled<String, String> = dom.extract().unwrap();
    assert_eq!(l.value, "12");
}

#[test]
fn unit_and_empty_structs_and_raw_identifiers() {
    #[derive(FromHtml, Debug, PartialEq)]
    struct Unit;
    #[derive(FromHtml, Debug, PartialEq)]
    struct Empty {}
    #[derive(FromHtml, Debug, PartialEq)]
    #[html(select = "input")]
    struct Input {
        #[html(attr = "type")]
        r#type: String,
    }
    let dom = parse("<input type=email>");
    assert_eq!(dom.extract::<Unit>(), Ok(Unit));
    assert_eq!(dom.extract::<Empty>(), Ok(Empty {}));
    assert_eq!(dom.extract::<Input>().unwrap().r#type, "email");
    let err = parse("<input>").extract::<Input>().unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"Input.type (@type): <input> has no "type" attribute"#
    );
}

// --- errors -----------------------------------------------------------------------------

#[test]
fn no_match_error() {
    let err = parse("<p>no product here</p>")
        .extract::<Product>()
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"Product.name ("h1.title"): no element matches"#
    );
    assert_eq!(err.kind(), &ExtractErrorKind::NoMatch);
    assert_eq!(err.path(), "Product.name");
    assert_eq!(err.selector(), Some("h1.title"));
    assert_eq!(err.value_source(), None);
}

#[test]
fn fields_are_read_in_declaration_order() {
    // Both `name` and `price` would fail: the first field is reported.
    let html = PRODUCT_PAGE.replace("h1", "h3").replace("149.99", "cheap");
    let err = parse(&html).extract::<Product>().unwrap_err();
    assert_eq!(err.path(), "Product.name");
}

#[test]
fn parse_error_on_an_attribute() {
    let html = PRODUCT_PAGE.replace(r#"data-value="149.99""#, r#"data-value="$5""#);
    let err = parse(&html).extract::<Product>().unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"Product.price (".price" @data-value): cannot parse "$5" as f64: invalid float literal"#
    );
    assert_eq!(
        err.kind(),
        &ExtractErrorKind::Parse {
            value: "$5".into(),
            type_name: "f64".into(),
            message: "invalid float literal".into(),
        }
    );
    assert_eq!(
        err.value_source(),
        Some(&ValueSource::Attr("data-value".into()))
    );
}

#[test]
fn errors_in_nested_structs_name_the_outer_field() {
    let html = PRODUCT_PAGE.replace(r#"class="name""#, r#"class="nom""#);
    let err = parse(&html).extract::<Product>().unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"Product.seller.name (".name"): no element matches"#
    );

    // The nested field reads its own scope: the outer selector is reported.
    let html = PRODUCT_PAGE.replace(r#"data-id="77""#, r#"data-id="seventy""#);
    let err = parse(&html).extract::<Product>().unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"Product.seller.id (".seller" @data-id): cannot parse "seventy" as u32: invalid digit found in string"#
    );

    let html = PRODUCT_PAGE.replace(r#" data-id="77""#, "");
    let err = parse(&html).extract::<Product>().unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"Product.seller.id (".seller" @data-id): <div> has no "data-id" attribute"#
    );
    assert_eq!(
        err.kind(),
        &ExtractErrorKind::MissingAttribute {
            name: "data-id".into(),
            element: Some("div".into()),
        }
    );
}

#[test]
fn errors_in_vec_items_name_the_index() {
    let html = PRODUCT_PAGE.replace(r#"data-value="4""#, r#"data-value="four""#);
    let err = parse(&html).extract::<Product>().unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"Product.reviews[2].stars (".stars" @data-value): cannot parse "four" as u8: invalid digit found in string"#
    );
    assert_eq!(err.path(), "Product.reviews[2].stars");

    let html = PRODUCT_PAGE.replace(r#"<span class="author">Coyote</span>"#, "");
    let err = parse(&html).extract::<Product>().unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"Product.reviews[1].author (".author"): no element matches"#
    );

    #[derive(FromHtml, Debug)]
    struct Gallery {
        #[html(select = "img", attr = "src")]
        _images: Vec<String>,
        #[html(select = "b")]
        _numbers: Vec<f64>,
    }
    let err = parse("<img src=a.png><img alt=b>")
        .extract::<Gallery>()
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"Gallery._images[1] ("img" @src): <img> has no "src" attribute"#
    );
    let err = parse("<b>1</b><b>2</b><b>x</b>")
        .extract::<Gallery>()
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"Gallery._numbers[2] ("b"): cannot parse "x" as f64: invalid float literal"#
    );
}

#[test]
fn errors_in_extract_all_name_the_index() {
    let html = LISTING.replace(r#"data-id="3""#, r#"data-id="three""#);
    let err = parse(&html).extract_all::<Card>().unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"Card[2].id (@data-id): cannot parse "three" as u32: invalid digit found in string"#
    );
    assert_eq!(err.path(), "Card[2].id");
}

#[test]
fn with_errors() {
    let err = parse("<b class=price>free</b>")
        .extract::<Modifiers>()
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"Modifiers.cents (".price"): cannot convert "free" with parse_cents: no digits in "free""#
    );
    let err = parse(r#"<b class=price data-was="n/a">1</b><p>x</p>"#)
        .extract::<Modifiers>()
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"Modifiers.was (".price" @data-was): cannot convert "n/a" with parse_cents: no digits in "n/a""#
    );
}

#[test]
fn invalid_selectors() {
    #[derive(FromHtml, Debug)]
    struct BadField {
        #[html(select = "h1[")]
        _title: Option<String>,
    }
    let err = parse("<h1>x</h1>").extract::<BadField>().unwrap_err();
    let selector_error = domq::Selector::parse("h1[").unwrap_err();
    assert_eq!(
        err.to_string(),
        format!(r#"BadField._title ("h1["): invalid selector: {selector_error}"#)
    );
    assert_eq!(
        err.to_string(),
        r#"BadField._title ("h1["): invalid selector: unexpected end of selector at byte 3"#
    );
    assert_eq!(
        err.kind(),
        &ExtractErrorKind::InvalidSelector(selector_error)
    );

    #[derive(FromHtml, Debug)]
    #[html(select = "li:hover")]
    struct BadType {}
    for err in [
        parse("<li>").extract::<BadType>().unwrap_err(),
        parse("<li>").extract_all::<BadType>().unwrap_err(),
    ] {
        assert_eq!(
            err.to_string(),
            r#"BadType ("li:hover"): invalid selector: unsupported pseudo-class ':hover' at byte 2"#
        );
    }
}

#[test]
fn missing_selector_for_extract_all() {
    let err = parse(PRODUCT_PAGE).extract_all::<Product>().unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"Product: extract_all needs a selector on the type: #[html(select = "…")]"#
    );
    assert_eq!(err.kind(), &ExtractErrorKind::MissingSelector);
    assert_eq!(
        parse("").extract_all::<u32>().unwrap_err().to_string(),
        r#"u32: extract_all needs a selector on the type: #[html(select = "…")]"#
    );
}

#[test]
fn long_values_are_shortened_in_messages() {
    #[derive(FromHtml, Debug)]
    struct Long {
        #[html(select = "p")]
        _n: u8,
    }
    let text = "word ".repeat(30);
    let err = parse(&format!("<p>{text}</p>"))
        .extract::<Long>()
        .unwrap_err();
    let shown: String = text.trim().chars().take(60).collect();
    assert_eq!(
        err.to_string(),
        format!(r#"Long._n ("p"): cannot parse "{shown}"… as u8: invalid digit found in string"#)
    );
}

#[test]
fn hand_built_errors() {
    let err = ExtractError::new(ExtractErrorKind::MissingAttribute {
        name: "href".into(),
        element: Some("a".into()),
    })
    .with_source(ValueSource::Attr("href".into()))
    .with_selector("a")
    .with_selector("ignored: the innermost selector wins")
    .at_index(4)
    .in_field("Nav", "links");
    assert_eq!(
        err.to_string(),
        r#"Nav.links[4] ("a" @href): <a> has no "href" attribute"#
    );
    let err = ExtractError::new(ExtractErrorKind::NoMatch).with_source(ValueSource::InnerHtml);
    assert_eq!(err.to_string(), "(html): no element matches");
    let err = ExtractError::new(ExtractErrorKind::NoMatch)
        .with_source(ValueSource::OuterHtml)
        .in_field("A", "b");
    assert_eq!(err.to_string(), "A.b (outer_html): no element matches");
    let err = ExtractError::new(ExtractErrorKind::NoMatch).with_source(ValueSource::Markdown);
    assert_eq!(err.to_string(), "(markdown): no element matches");
    assert_eq!(
        ExtractError::new(ExtractErrorKind::NoMatch).to_string(),
        "no element matches"
    );
}

#[cfg(feature = "std")]
#[test]
fn is_a_std_error() {
    fn boxed(e: ExtractError) -> Box<dyn std::error::Error + Send + Sync> {
        Box::new(e)
    }
    let err = parse("").extract::<Card>().unwrap_err();
    assert_eq!(
        boxed(err).to_string(),
        r#"Card ("li.card"): no element matches"#
    );
}

#[test]
fn extraction_follows_edits() {
    let mut dom: Dom = parse(LISTING);
    let first = dom.select_first("li.card").unwrap().id();
    dom.remove(first);
    let pad = dom.select_first("li.card:last-child").unwrap().id();
    dom.insert_html_after(pad, r#"<li class=card data-id=4><h2><a href="/p/4">Cap</a></h2><span class=price>1</span></li>"#);
    let ids: Vec<u32> = dom
        .extract_all::<Card>()
        .unwrap()
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(ids, [2, 3, 4]);
}

// --- macro hygiene ------------------------------------------------------------------------

macro_rules! define_reader {
    ($name:ident, $ty:ty, $sel:literal, $with:literal) => {
        #[derive(FromHtml, Debug, PartialEq)]
        struct $name {
            #[html(select = $sel)]
            value: $ty,
            #[html(select = $sel, with = $with)]
            converted: u64,
            #[html(select = $sel)]
            all: Vec<$ty>,
            #[html(select = ".none", default)]
            fallback: $ty,
            #[html(select = $sel)]
            first: Option<$ty>,
        }
    };
}

define_reader!(FromMacro, u32, "b", "parse_cents");

#[test]
fn structs_defined_by_macros() {
    let dom = parse("<b>12</b><b>3</b>");
    assert_eq!(
        dom.extract::<FromMacro>(),
        Ok(FromMacro {
            value: 12,
            converted: 12,
            all: vec![12, 3],
            fallback: 0,
            first: Some(12),
        })
    );
}

// --- hidden elements -----------------------------------------------------------------------

#[test]
fn title_and_json_ld_are_readable() {
    #[derive(FromHtml, Debug, PartialEq)]
    struct Meta {
        #[html(select = "title")]
        title: String,
        #[html(select = r#"script[type="application/ld+json"]"#)]
        json_ld: Option<String>,
        #[html(select = "body")]
        body: String,
    }
    let dom = parse(
        "<html><head><title> Acme  Anvil | Store </title>\
         <script type=application/ld+json>\n{\"@type\": \"Product\",\n \"name\": \"Anvil\"}\n</script>\
         </head><body><p>Hi<script>track()</script></p></body></html>",
    );
    assert_eq!(
        dom.extract::<Meta>(),
        Ok(Meta {
            title: "Acme Anvil | Store".into(),
            json_ld: Some("{\"@type\": \"Product\",\n \"name\": \"Anvil\"}".into()),
            body: "Hi".into(),
        })
    );
}

// --- the messages shared with the Python binding --------------------------------------------

/// The exact `Display` of every error kind, kept identical to the Python
/// binding's `ExtractError`.
#[test]
fn error_messages_match_the_python_binding() {
    fn message<T: FromHtml + core::fmt::Debug>(html: &str) -> String {
        parse(html).extract::<T>().unwrap_err().to_string()
    }

    #[derive(FromHtml, Debug)]
    struct Product {
        #[html(select = "h1")]
        _title: String,
        #[html(select = ".price", attr = "data-value")]
        _price: f64,
        #[html(attr = "data-sku")]
        _sku: Option<String>,
    }
    assert_eq!(
        message::<Product>("<p>x</p>"),
        r#"Product._title ("h1"): no element matches"#
    );
    assert_eq!(
        message::<Product>(r#"<h1>T</h1><span class=price data-value=" $5 ">$5</span>"#),
        r#"Product._price (".price" @data-value): cannot parse "$5" as f64: invalid float literal"#
    );
    assert_eq!(
        message::<Product>("<h1>T</h1><span class=price>$5</span>"),
        r#"Product._price (".price" @data-value): <span> has no "data-value" attribute"#
    );

    #[derive(FromHtml, Debug)]
    struct Sku {
        #[html(attr = "data-sku")]
        _sku: String,
    }
    assert_eq!(
        message::<Sku>("<p data-sku=1>x</p>"),
        "Sku._sku (@data-sku): a document has no attributes; give the field a selector or \
         extract from an element"
    );

    #[derive(FromHtml, Debug)]
    struct Cents {
        #[html(select = ".price", with = "parse_cents")]
        _cents: u64,
        #[html(select = ".n", with = "str::parse::<u8>")]
        _n: Option<u8>,
        #[html(select = ".w", with = "util::words")]
        _w: Option<usize>,
    }
    assert_eq!(
        message::<Cents>("<b class=price>$x</b>"),
        r#"Cents._cents (".price"): cannot convert "$x" with parse_cents: no digits in "$x""#
    );
    assert_eq!(
        message::<Cents>("<b class=price>1</b><i class=n> 300 </i>"),
        r#"Cents._n (".n"): cannot convert "300" with parse: number too large to fit in target type"#
    );

    #[derive(FromHtml, Debug)]
    struct Flags {
        #[html(select = "input", attr = "data-on")]
        _on: bool,
    }
    assert_eq!(
        message::<Flags>("<input data-on=yes>"),
        r#"Flags._on ("input" @data-on): cannot parse "yes" as bool: expected "true" or "false""#
    );

    #[derive(FromHtml, Debug)]
    struct Typo {
        #[html(select = "h1:hover")]
        _title: String,
    }
    assert_eq!(
        message::<Typo>("<h1>x</h1>"),
        r#"Typo._title ("h1:hover"): invalid selector: unsupported pseudo-class ':hover' at byte 2"#
    );

    // Paths: nested structs, `Vec` items, and `extract_all` items.
    #[derive(FromHtml, Debug)]
    #[html(select = ".product")]
    struct Item {
        #[html(select = ".seller")]
        _seller: Seller,
        #[html(select = ".review")]
        _reviews: Vec<Review>,
    }
    let page = |name: &str, stars: &str| {
        format!(
            r#"<div class=product><div class=seller data-id=1><span class={name}>S</span>
               <a class=profile href=/s>s</a></div>
               <div class=review><span class=author>A</span><span class=stars data-value=5>5</span></div>
               <div class=review><span class=author>B</span><span class=stars data-value={stars}>?</span></div>
               </div>"#
        )
    };
    assert_eq!(
        message::<Item>(&page("nom", "4")),
        r#"Item._seller.name (".name"): no element matches"#
    );
    assert_eq!(
        message::<Item>(&page("name", "x")),
        r#"Item._reviews[1].stars (".stars" @data-value): cannot parse "x" as u8: invalid digit found in string"#
    );
    let two = format!("{}{}", page("name", "4"), page("name", "x"));
    assert_eq!(
        parse(&two).extract_all::<Item>().unwrap_err().to_string(),
        r#"Item[1]._reviews[1].stars (".stars" @data-value): cannot parse "x" as u8: invalid digit found in string"#
    );
}
