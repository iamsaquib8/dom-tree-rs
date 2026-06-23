//! Parse-throughput benchmarks: `dom-tree-rs` vs `tl`.
//!
//! Run with: `cargo bench` (from the `benchmarks/` directory).
//!
//! We do NOT claim to beat `tl` — it's a SIMD-accelerated speed specialist.
//! These numbers exist to be honest about where we land (and to catch
//! regressions). `tl` is included as the reference fast baseline.

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};

/// Build a realistic-looking HTML article, repeated `sections` times.
fn make_article(sections: usize) -> String {
    let mut s = String::from("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Bench &amp; Co</title></head><body>");
    for i in 0..sections {
        s.push_str(&format!(
            "<section class=\"post\" id=\"s{i}\">\
               <h2 class=\"title\">Section &mdash; {i}</h2>\
               <p>Some <a href=\"/p/{i}\" rel=\"bookmark\">linked</a> text with <strong>bold</strong> \
                  and <em>emphasis</em> and an entity &copy; plus a &lt;tag-like&gt; bit.\
               <ul class=\"tags\"><li>one<li>two<li>three</ul>\
               <img src=\"/img/{i}.png\" alt=\"image {i}\" loading=\"lazy\">\
               <!-- comment {i} -->\
             </section>"
        ));
    }
    s.push_str("</body></html>");
    s
}

fn bench_parse(c: &mut Criterion) {
    let inputs = [
        ("small", make_article(1)),
        ("medium", make_article(60)),
        ("large", make_article(1200)),
    ];

    for (name, input) in &inputs {
        let mut group = c.benchmark_group(format!("parse/{name}"));
        group.throughput(Throughput::Bytes(input.len() as u64));

        group.bench_function("dom-tree-rs", |b| {
            b.iter(|| {
                let dom = domtree::parse(black_box(input));
                black_box(dom.len())
            })
        });

        group.bench_function("tl", |b| {
            b.iter(|| {
                let dom = tl::parse(black_box(input), tl::ParserOptions::default()).unwrap();
                black_box(dom.nodes().len())
            })
        });

        group.finish();
    }
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
