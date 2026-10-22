use domq::FromHtml;

#[derive(FromHtml)]
enum Page {
    Article,
    Listing,
}

#[derive(FromHtml)]
struct Pair(String, String);

#[derive(FromHtml)]
union Bits {
    a: u32,
    b: f32,
}

fn main() {}
