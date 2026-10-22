use domtree::FromHtml;

#[derive(FromHtml)]
struct Links {
    #[html(attr = "href")]
    urls: Vec<String>,
}

fn main() {}
