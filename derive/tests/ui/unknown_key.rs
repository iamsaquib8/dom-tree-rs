use domtree::FromHtml;

#[derive(FromHtml)]
struct Typo {
    #[html(selector = "h1")]
    title: String,
}

#[derive(FromHtml)]
struct Unknown {
    #[html(select = "h1", trim)]
    title: String,
}

fn main() {}
