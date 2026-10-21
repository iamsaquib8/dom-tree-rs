//! Benchmark inputs, shared by `benches/` and `examples/memory.rs` so every
//! number in `README.md` comes from exactly the same documents.
//!
//! All inputs are generated deterministically (no files, no randomness), so a
//! run on another machine measures the same bytes.

use std::fmt::Write;

/// A realistic-looking blog article: headings, links, inline formatting,
/// entities, an implicitly closed list, images and comments — repeated
/// `sections` times. Markup-dense (about one node per 13 bytes).
pub fn article(sections: usize) -> String {
    let mut s = String::from(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
         <title>Bench &amp; Co</title></head><body>",
    );
    for i in 0..sections {
        let _ = write!(
            s,
            "<section class=\"post\" id=\"s{i}\">\
               <h2 class=\"title\">Section &mdash; {i}</h2>\
               <p>Some <a href=\"/p/{i}\" rel=\"bookmark\">linked</a> text with <strong>bold</strong> \
                  and <em>emphasis</em> and an entity &copy; plus a &lt;tag-like&gt; bit.\
               <ul class=\"tags\"><li>one<li>two<li>three</ul>\
               <img src=\"/img/{i}.png\" alt=\"image {i}\" loading=\"lazy\">\
               <!-- comment {i} -->\
             </section>"
        );
    }
    s.push_str("</body></html>");
    s
}

/// Long-form prose: big paragraphs of plain text with only the occasional
/// inline element or entity — a book chapter, documentation, a news story.
/// Throughput here is dominated by scanning text for the next `<` / `&`.
pub fn text_heavy(paragraphs: usize) -> String {
    const WORDS: &[&str] = &[
        "the",
        "quick",
        "brown",
        "fox",
        "jumps",
        "over",
        "lazy",
        "dog",
        "while",
        "parsers",
        "scan",
        "long",
        "runs",
        "of",
        "text",
        "looking",
        "for",
        "markup",
        "and",
        "rarely",
        "find",
        "any",
        "because",
        "prose",
        "is",
        "mostly",
        "letters",
        "spaces",
        "commas,",
        "periods.",
        "Lorem",
        "ipsum",
        "dolor",
        "sit",
        "amet,",
        "consectetur",
        "adipiscing",
    ];
    let mut s = String::from(
        "<!doctype html><html><head><title>A long read</title></head><body><article>\n",
    );
    let mut w = 0usize;
    for p in 0..paragraphs {
        if p % 12 == 0 {
            let _ = write!(s, "<h2>Chapter {}</h2>\n", p / 12 + 1);
        }
        s.push_str("<p>");
        for i in 0..380 {
            s.push_str(WORDS[w % WORDS.len()]);
            w = w.wrapping_mul(31).wrapping_add(7) % 9973;
            s.push(' ');
            // One inline element and one entity per ~190 words.
            match i {
                120 => s.push_str("<em>emphasised words</em> "),
                250 => s.push_str("&mdash; "),
                _ => {}
            }
        }
        s.push_str("</p>\n");
    }
    s.push_str("</article></body></html>");
    s
}

/// A large data table: a header row and `rows` rows of six cells with classes,
/// numbers, a link per row, and the omitted `</td>`/`</tr>` that real-world
/// generated tables are full of. Attribute- and element-dense.
pub fn table_heavy(rows: usize) -> String {
    let mut s = String::from(
        "<!doctype html><html><head><title>Report</title></head><body>\
         <table class=\"data\" id=\"report\"><thead><tr><th>ID<th>Name<th>Region\
         <th class=\"num\">Units<th class=\"num\">Price<th>Link</thead><tbody>\n",
    );
    for r in 0..rows {
        let _ = write!(
            s,
            "<tr class=\"{}\" data-row=\"{r}\"><td>{r}<td class=\"name\">Item #{r}\
             <td>{}<td class=\"num\">{}<td class=\"num\">{}.{:02}\
             <td><a href=\"/items/{r}\" title=\"Item {r} &amp; details\">view</a>\n",
            if r % 2 == 0 { "even" } else { "odd" },
            ["North", "South", "East", "West"][r % 4],
            (r * 37) % 1000,
            (r * 13) % 500,
            r % 100,
        );
    }
    s.push_str("</tbody></table></body></html>");
    s
}

/// Every benchmark input as `(name, html)`, smallest first.
pub fn inputs() -> Vec<(&'static str, String)> {
    vec![
        ("small", article(1)),
        ("medium", article(60)),
        ("large", article(1200)),
        ("text-heavy", text_heavy(240)),
        ("table-heavy", table_heavy(4000)),
    ]
}

#[cfg(test)]
mod tests {
    #[test]
    fn inputs_have_the_advertised_shape() {
        for (name, html) in super::inputs() {
            let dom = domtree::parse(&html);
            assert!(dom.len() > 5, "{name}");
            let bytes_per_node = html.len() / dom.len();
            match name {
                "text-heavy" => assert!(bytes_per_node > 200, "{name}: {bytes_per_node}"),
                "table-heavy" | "large" => assert!(bytes_per_node < 40, "{name}: {bytes_per_node}"),
                _ => {}
            }
        }
    }
}
