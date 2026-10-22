# domq-derive

`#[derive(FromHtml)]` for [domq](https://crates.io/crates/domq):
declarative extraction of Rust structs from HTML with CSS selectors.

Don't depend on this crate directly. Turn on the core crate's `derive`
feature, which re-exports the macro next to the `FromHtml` trait it
implements (like `serde` and `serde_derive`):

```toml
[dependencies]
domq = { version = "0.4", features = ["derive"] }
```

```rust
use domq::FromHtml;

#[derive(FromHtml)]
struct Product {
    #[html(select = "h1")]
    name: String,
    #[html(select = ".price", attr = "data-value")]
    price: f64,
    #[html(select = ".tags li")]
    tags: Vec<String>,          // every match
    #[html(select = ".discount")]
    discount: Option<String>,   // None without a match
    #[html(select = ".seller")]
    seller: Seller,             // a nested FromHtml struct
    #[html(select = ".review")]
    reviews: Vec<Review>,
}

#[derive(FromHtml)]
#[html(select = ".card")]       // for dom.extract_all::<Card>()
struct Card {
    #[html(select = "h2")]
    title: String,
    #[html(select = "a", attr = "href")]
    url: String,
}

let product: Product = domq::parse(html).extract()?;
let cards: Vec<Card> = domq::parse(listing).extract_all()?;
```

## Attributes

On a field (every field needs one):

| key | meaning |
|---|---|
| `select = "css"` | the elements to read; without it, the scope itself |
| *(no source key)* | the readable text, or a nested `FromHtml` type read from the match |
| `attr = "name"` | an attribute's value |
| `html` / `outer_html` / `markdown` | the inner HTML / outer HTML / Markdown |
| `with = "path::to::fn"` | convert the value with `fn(&str) -> Result<T, E>`, `E: Display` |
| `default` | no match or no attribute → `Default::default()` |
| `skip` | read nothing: `Default::default()` |

On the struct: `select = "css"`, the elements `extract_all` reads one value
from (and `extract` reads the first of).

The field type decides how many matches are read: `Option<T>` (first or
`None`), `Vec<T>` (all, in document order), anything else (the first, an
error without one).

## Errors

At run time, `ExtractError` names the field path, the selector, the source
and the reason:

```text
Product.price (".price" @data-value): cannot parse "$5" as f64: invalid float literal
Product.reviews[1].author (".author"): no element matches
Product.seller.id (".seller" @data-id): <div> has no "data-id" attribute
Card[2].url ("a" @href): cannot convert "x" with parse_url: relative URL without a base
```

At compile time, mistakes point at the attribute that caused them:

```text
error: unknown key `selector`: did you mean `select`?
 --> src/main.rs:5:12
  |
5 |     #[html(selector = "h1")]
  |            ^^^^^^^^
```

Also reported: two sources on one field, `default` on an `Option` or `Vec`,
a `Vec` without `select`, `skip` combined with other keys, a field without
`#[html(...)]`, enums, unions and tuple structs. A field type that implements
neither `FromHtml` nor `FromHtmlValue`, a `with` function of the wrong type,
and `markdown` without the core's `markdown` feature are reported at the
field.

The generated code refers only to `::domq::__private` re-exports (never
`std`), so it works in `no_std` crates. Full documentation:
[Declarative extraction](https://docs.rs/domq/latest/domq/#declarative-extraction).

## License

MIT
