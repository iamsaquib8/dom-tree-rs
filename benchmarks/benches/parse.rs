//! Parse-throughput benchmarks: `dom-tree-rs` against `tl`, `scraper` and
//! `html5ever` (into its reference `RcDom`).
//!
//! Run with: `cargo bench` (from the `benchmarks/` directory); add
//! `--features simd` to measure the optional memchr-backed scanning.
//!
//! We do NOT claim to beat `tl` — it is a speed specialist that does far less
//! work (no entity decoding, no implicit closes). `html5ever` and `scraper`
//! are the spec-compliant reference points. These numbers exist to be honest
//! about where we land, and to catch regressions. See `README.md` for results.

use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use html5ever::tendril::TendrilSink;

fn bench_parse(c: &mut Criterion) {
    for (name, input) in dom_tree_rs_benchmarks::inputs() {
        let mut group = c.benchmark_group(format!("parse/{name}"));
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.warm_up_time(Duration::from_secs(1));
        group.measurement_time(Duration::from_secs(4));
        let input = input.as_str();

        group.bench_function("dom-tree-rs", |b| {
            b.iter(|| black_box(domtree::parse(black_box(input))).len())
        });

        // The streaming API: the same recovery, no tree.
        group.bench_function("dom-tree-rs/events", |b| {
            b.iter(|| domtree::events(black_box(input)).count())
        });

        group.bench_function("tl", |b| {
            b.iter(|| {
                let dom = tl::parse(black_box(input), tl::ParserOptions::default()).unwrap();
                black_box(dom.nodes().len())
            })
        });

        group.bench_function("scraper", |b| {
            b.iter(|| black_box(scraper::Html::parse_document(black_box(input))))
        });

        group.bench_function(BenchmarkId::new("html5ever", "rcdom"), |b| {
            b.iter(|| {
                let dom = html5ever::parse_document(
                    markup5ever_rcdom::RcDom::default(),
                    Default::default(),
                )
                .one(black_box(input));
                black_box(dom)
            })
        });

        group.finish();
    }
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
