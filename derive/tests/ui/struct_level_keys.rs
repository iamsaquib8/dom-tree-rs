use domtree::FromHtml;

#[derive(FromHtml)]
#[html(select = ".card", attr = "data-id")]
struct Card {
    #[html(select = "h2")]
    title: String,
}

fn main() {}
