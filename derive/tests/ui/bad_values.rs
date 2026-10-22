use domtree::FromHtml;

#[derive(FromHtml)]
struct Page {
    #[html(select = "div", html = "yes")]
    body: String,
    #[html(select = "b", with = "not a path!")]
    price: f64,
    #[html(select = "")]
    empty: String,
    #[html(select = "a", select = "b")]
    twice: String,
    #[html(select = "a", attr = "")]
    nameless: String,
}

fn main() {}
