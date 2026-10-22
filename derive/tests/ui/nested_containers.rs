use domtree::FromHtml;

#[derive(FromHtml)]
struct Page {
    #[html(select = "a", attr = "href")]
    links: Vec<Option<String>>,
}

fn main() {}
