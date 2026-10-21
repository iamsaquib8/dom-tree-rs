//! The selector engine's memo stays bounded by the size of the document plus
//! the length of the selector, never their product. Measured with a counting
//! allocator, so this file holds a single test (nothing else allocates while
//! it measures).
#![cfg(feature = "query")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

use domtree::{Selector, parse};

/// Tracks the high-water mark of live heap bytes.
struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn grew(size: usize) {
    let live = LIVE.fetch_add(size, Relaxed) + size;
    PEAK.fetch_max(live, Relaxed);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        grew(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
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

/// What `query` returns, and the most heap it used on top of what was live.
fn peak(query: impl FnOnce() -> usize) -> (usize, usize) {
    let before = LIVE.load(Relaxed);
    PEAK.store(before, Relaxed);
    let found = query();
    (found, PEAK.load(Relaxed) - before)
}

/// A selector list of `n` entries.
fn list(n: usize, entry: &str) -> Selector {
    let entries: Vec<_> = (1..=n)
        .map(|k| entry.replace('K', &k.to_string()))
        .collect();
    Selector::parse(&entries.join(", ")).unwrap()
}

#[test]
fn memo_memory_is_bounded_by_document_plus_selector() {
    // Hundreds of `:nth-child()`s share one table of positions: about 4
    // bytes a node. (Each used to get its own: 150 tables, 2.6 MB here.)
    let p = "<p>x</p>".repeat(2_000);
    let dom = parse(&format!(
        "<div>{p}<ul>{}</ul></div>",
        "<li>y</li>".repeat(150)
    ));
    let nth = list(300, "li:nth-child(K)");
    let root = dom.root().unwrap();
    for (scoped, (found, bytes)) in [
        (false, peak(|| dom.select(&nth).count())),
        (true, peak(|| root.select(&nth).count())),
    ] {
        assert_eq!(found, 150);
        let limit = 16 * dom.len();
        assert!(
            bytes < limit,
            "scoped: {scoped}: {bytes} bytes, limit {limit}"
        );
    }

    // Each `of S` and each combinator needs entries of its own, but past a
    // size proportional to the document the oldest are dropped, so four
    // times the selector is nowhere near four times the memory (as it was).
    let mut rows = String::from("<table>");
    for i in 0..500 {
        rows.push_str(&format!("<tr><td>{i}</td><td>x</td></tr>"));
    }
    rows.push_str("</table>");
    let rows = parse(&rows);
    let deep = parse(&format!(
        "{}{}",
        "<div>".repeat(1_000),
        "</div>".repeat(1_000)
    ));
    for (dom, entry) in [(&rows, "td:nth-child(2n of .cK)"), (&deep, ".aK div")] {
        let (short, long) = (list(100, entry), list(400, entry));
        let (_, small) = peak(|| dom.select(&short).count());
        let (_, big) = peak(|| dom.select(&long).count());
        assert!(
            big < 2 * small,
            "{entry}: {small} bytes for 100, {big} for 400"
        );
    }
}
