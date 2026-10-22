use domq::FromHtml;

#[derive(FromHtml)]
struct Page {
    #[html(select = ".subtitle", default)]
    subtitle: Option<String>,
}

#[derive(FromHtml)]
struct List {
    #[html(select = "li", default)]
    items: Vec<String>,
}

fn main() {}
