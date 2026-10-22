use domq::FromHtml;

#[derive(FromHtml)]
struct Product {
    #[html(select = "h1")]
    name: String,
    price: f64,
}

fn main() {}
