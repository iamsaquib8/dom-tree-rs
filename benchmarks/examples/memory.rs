//! Memory use per parse: heap allocations, bytes requested, and the peak of
//! live heap bytes while the parsed document exists — for `domq` and
//! the parsers it is benchmarked against. Prints a Markdown table.
//!
//! Run with: `cargo run --release --example memory` (from `benchmarks/`).

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use html5ever::tendril::TendrilSink;

/// Counts every allocation and tracks the high-water mark of live bytes.
struct Counting;

static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn grew(size: usize) {
    let live = LIVE.fetch_add(size, Relaxed) + size;
    PEAK.fetch_max(live, Relaxed);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(layout.size(), Relaxed);
        grew(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(new_size, Relaxed);
        if new_size >= layout.size() {
            grew(new_size - layout.size());
        } else {
            LIVE.fetch_sub(layout.size() - new_size, Relaxed);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

struct Usage {
    allocs: usize,
    bytes: usize,
    peak: usize,
    retained: usize,
}

/// Measure one parse. `retained` is what the result still holds once parsing
/// is done (for a streaming parse: nothing).
fn measure<T>(f: impl FnOnce() -> T) -> Usage {
    let base = LIVE.load(Relaxed);
    PEAK.store(base, Relaxed);
    let (a0, b0) = (ALLOCS.load(Relaxed), BYTES.load(Relaxed));
    let kept = black_box(f());
    let usage = Usage {
        allocs: ALLOCS.load(Relaxed) - a0,
        bytes: BYTES.load(Relaxed) - b0,
        peak: PEAK.load(Relaxed) - base,
        retained: LIVE.load(Relaxed) - base,
    };
    drop(kept);
    usage
}

fn kib(n: usize) -> String {
    format!("{:.1}", n as f64 / 1024.0)
}

fn main() {
    println!(
        "| input | size (KiB) | parser | allocations | allocated (KiB) | peak live (KiB) | retained (KiB) | retained / input |"
    );
    println!("|---|---:|---|---:|---:|---:|---:|---:|");
    for (name, html) in domq_benchmarks::inputs() {
        let input = html.as_str();
        let rows: Vec<(&str, Usage)> = vec![
            ("domq", measure(|| domq::parse(input))),
            (
                "domq/events",
                measure(|| domq::events(input).count()),
            ),
            (
                "tl",
                measure(|| tl::parse(input, tl::ParserOptions::default())),
            ),
            ("scraper", measure(|| scraper::Html::parse_document(input))),
            (
                "html5ever (rcdom)",
                measure(|| {
                    html5ever::parse_document(
                        markup5ever_rcdom::RcDom::default(),
                        Default::default(),
                    )
                    .one(input)
                }),
            ),
        ];
        for (parser, u) in rows {
            println!(
                "| {name} | {} | {parser} | {} | {} | {} | {} | {:.2}× |",
                kib(input.len()),
                u.allocs,
                kib(u.bytes),
                kib(u.peak),
                kib(u.retained),
                u.retained as f64 / input.len() as f64,
            );
        }
    }
}
