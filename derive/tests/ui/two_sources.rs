use domtree::FromHtml;

#[derive(FromHtml)]
struct Link {
    #[html(select = "a", attr = "href", html)]
    url: String,
}

fn main() {}
