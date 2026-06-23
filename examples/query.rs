//! Query a document with CSS selectors.
//!
//! Run with: `cargo run --example query --features query`

#[cfg(feature = "query")]
fn main() {
    let html = r#"
        <article>
            <h1 class="title">Hello</h1>
            <ul class="links">
                <li><a href="/a">first</a></li>
                <li><a href="/b" rel="next">second</a></li>
            </ul>
        </article>
    "#;

    let dom = domtree::parse(html);

    println!(
        "title: {:?}",
        dom.select("article > h1.title")
            .next()
            .map(|n| n.text_content())
    );

    println!("links:");
    for a in dom.select("ul.links a[href]") {
        println!("  {} -> {}", a.text_content(), a.attr("href").unwrap_or(""));
    }

    println!(
        "the 'next' link: {:?}",
        dom.select("a[rel=next]").next().map(|n| n.text_content())
    );
}

#[cfg(not(feature = "query"))]
fn main() {
    eprintln!("re-run with: cargo run --example query --features query");
}
