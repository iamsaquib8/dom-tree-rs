//! Parse some HTML and print the tree + JSON.
//!
//! Run with: `cargo run --example parse`

fn main() {
    let html = r#"
        <!doctype html>
        <ul id="list">
            <li class="item">Item 1</li>
            <li class="item">Item 2 &amp; a half</li>
            <li>Item 3<br>(wrapped)
        </ul>
    "#;

    let dom = domq::parse(html);

    println!("--- tree ---");
    println!("{dom:#?}");

    println!("\n--- text of each <li> ---");
    for li in dom.find_by_tag("li") {
        println!("{:?}", li.text_content());
    }

    #[cfg(feature = "json")]
    {
        println!("\n--- json ---");
        println!("{}", dom.to_json());
    }

    if !dom.errors().is_empty() {
        println!("\n--- recovered from {} issue(s) ---", dom.errors().len());
        for e in dom.errors() {
            println!("  {e}");
        }
    }
}
