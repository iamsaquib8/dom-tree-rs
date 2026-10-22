use domq::FromHtml;

#[derive(FromHtml)]
struct Page {
    #[html(skip, select = "h1")]
    title: String,
}

fn main() {}
