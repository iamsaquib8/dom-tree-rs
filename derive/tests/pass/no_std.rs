//! The generated code names nothing from `std` (or from `alloc`, which this
//! crate doesn't declare): only `::domtree::__private` re-exports.
#![no_std]

// A host `std` for the panic handler and `main`, under another name so that
// `::std` paths would not resolve.
extern crate std as host;

use domtree::FromHtml;
use host::string::String;
use host::vec::Vec;
use host::{assert_eq, format};

fn cents(value: &str) -> Result<u32, String> {
    let digits = value.trim_start_matches('$').replace('.', "");
    digits.parse().map_err(|e| format!("{e}"))
}

#[derive(FromHtml)]
#[html(select = ".item")]
struct Item {
    #[html(select = "h2")]
    name: String,
    #[html(select = ".price", with = "cents")]
    cents: u32,
    #[html(attr = "data-sku")]
    sku: Option<String>,
    #[html(select = ".tag")]
    tags: Vec<String>,
    #[html(select = ".stock", default)]
    stock: u32,
    #[html(select = "p", html)]
    html: Option<String>,
    #[html(select = "p", markdown)]
    markdown: Option<String>,
    #[html(skip)]
    seen: bool,
}

#[derive(FromHtml)]
struct Shop<T> {
    #[html(select = "h1")]
    title: T,
    #[html(select = ".item")]
    items: Vec<Item>,
}

fn main() {
    let dom = domtree::parse(
        "<h1>Shop</h1><div class=item data-sku=a1><h2>Pen</h2><b class=price>$1.50</b>\
         <i class=tag>x</i><p>A <em>fine</em> pen</p></div>",
    );
    let shop: Shop<String> = dom.extract().ok().unwrap();
    assert_eq!(shop.title, "Shop");
    let item = &shop.items[0];
    assert_eq!((item.name.as_str(), item.cents, item.stock, item.seen), ("Pen", 150, 0, false));
    assert_eq!(item.sku.as_deref(), Some("a1"));
    assert_eq!(item.tags, ["x"]);
    assert_eq!(item.html.as_deref(), Some("A <em>fine</em> pen"));
    assert_eq!(item.markdown.as_deref(), Some("A *fine* pen"));
    assert_eq!(dom.extract_all::<Item>().ok().map(|v| v.len()), Some(1));
}
