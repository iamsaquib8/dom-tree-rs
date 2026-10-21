//! Turn an HTML page into Markdown — e.g. to put a web page into an LLM prompt.
//!
//! ```sh
//! curl -sL https://blog.rust-lang.org | cargo run --example to_markdown -- main
//! ```
//!
//! The optional argument is a CSS selector for the part of the page you want
//! (default: the whole document). `nav`, `footer` and `script` are removed first.

use std::io::Read;

fn main() {
    let mut html = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut html) {
        eprintln!("reading stdin: {e}");
        std::process::exit(1);
    }

    let mut dom = domtree::parse(&html);
    let chrome: Vec<_> = dom.select("nav, footer, script").map(|n| n.id()).collect();
    for id in chrome {
        dom.remove(id);
    }

    let markdown = match std::env::args().nth(1) {
        Some(selector) => dom
            .select(&selector)
            .map(|n| n.to_markdown())
            .collect::<Vec<_>>()
            .join("\n\n"),
        None => dom.to_markdown(),
    };
    println!("{markdown}");
}
