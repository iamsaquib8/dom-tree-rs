# Benchmarks

Parse throughput and memory of `domq` next to
[`tl`](https://crates.io/crates/tl), [`scraper`](https://crates.io/crates/scraper)
and [`html5ever`](https://crates.io/crates/html5ever) (into its reference
`RcDom`), on the same inputs, in the same process.

```sh
cd benchmarks
cargo bench                                    # throughput (criterion)
cargo bench --features simd -- 'domq$'  # with memchr-backed scanning
cargo run --release --example memory           # allocations, peak and retained heap
```

This is a separate crate (outside the workspace), so its dependencies never
reach the published crate. The `Benchmarks` workflow
(`.github/workflows/bench.yml`) runs all of the above weekly and on demand and
uploads the results — plus the wasm size and the library's compile time — as
an artifact. It is not part of PR CI: timings depend on the runner.

## What is measured

Every parser gets the same `&str` and builds its whole document; dropping it
is included. They do different amounts of work, so read the numbers with that
in mind:

- **domq** — tokenizes, applies its recovery rules (implicit closes,
  void elements, mis-nesting), decodes entities, lower-cases names, records
  source ranges and diagnostics, into an arena of owned strings.
- **domq/events** — the same recovery streamed by `domq::events`,
  without a tree.
- **tl** — a speed specialist: no entity decoding, no implicit closes; nodes
  borrow from the input.
- **scraper**, **html5ever** — the full WHATWG algorithm (the browser's
  answer), into scraper's `ego-tree` and html5ever's `RcDom` respectively.

Inputs (`src/lib.rs`; generated, deterministic):

| input | size | shape |
|---|---:|---|
| small | 0.5 KiB | one article section — mostly per-document overhead |
| medium | 20.7 KiB | 60 sections: heading, links, inline formatting, entities, an implicitly closed list, an image and a comment each |
| large | 420 KiB | 1,200 such sections (about one node per 16 bytes) |
| text-heavy | 547 KiB | long-form prose: 240 paragraphs of ~380 words, one inline element and one entity each (one node per ~370 bytes) |
| table-heavy | 712 KiB | a 4,000-row, 6-column data table with classes, links and omitted `</td>`/`</tr>` (one node per ~12 bytes) |

## Results

Machine: Apple M4 Pro (10 performance + 4 efficiency cores), 24 GB, macOS
26.6, rustc 1.93.0, default `bench` profile. The machine was shared with
other jobs (load average ~4–5 of 14 cores), so base and new runs were
alternated, twice each; the competitors moved by at most 2.5% between runs.
Throughput is input bytes per second over criterion's median time; the
lower of the two runs is shown.

### Throughput

| input | domq | domq/events | tl | scraper | html5ever (rcdom) |
|---|---:|---:|---:|---:|---:|
| small | 400 MB/s (1.2 µs) | 341 MB/s (1.4 µs) | 395 MB/s (1.2 µs) | 53 MB/s (8.8 µs) | 48 MB/s (9.6 µs) |
| medium | 491 MB/s (43.1 µs) | 411 MB/s (51.5 µs) | 462 MB/s (45.8 µs) | 60 MB/s (354 µs) | 53 MB/s (401 µs) |
| large | 512 MB/s (0.84 ms) | 424 MB/s (1.01 ms) | 358 MB/s (1.20 ms) | 61 MB/s (7.01 ms) | 54 MB/s (8.01 ms) |
| text-heavy | 7.7 GB/s (72.8 µs) | 6.9 GB/s (81.7 µs) | 8.5 GB/s (66.3 µs) | 1.9 GB/s (302 µs) | 1.6 GB/s (355 µs) |
| table-heavy | 422 MB/s (1.73 ms) | 329 MB/s (2.22 ms) | 326 MB/s (2.23 ms) | 58 MB/s (12.7 ms) | 51 MB/s (14.2 ms) |

On markup, `domq` is 7–8× faster than `scraper` and 8–10× faster than
`html5ever`'s `RcDom` (4–5× on prose); it is level with `tl` on small
documents, ahead of it on large markup (+30–40%), and ~10% behind it on prose.
These are generated documents — real pages differ — and `tl` does less work.
The streaming API takes 12–28% longer than building the tree: it hands out an
owned attribute list per start tag where the tree shares one arena. It exists
for memory, not speed.

### Before / after the 0.5 work (`domq` only)

| input | 0.4 (3aaba5e) | now | change |
|---|---:|---:|---:|
| small | 1.5 µs | 1.2 µs | −25% |
| medium | 62.9 µs | 43.1 µs | −32% |
| large | 1.26 ms | 0.84 ms | −33% |
| text-heavy | 232 µs | 72.8 µs | −69% |
| table-heavy | 2.75 ms | 1.73 ms | −37% |

Where it came from: one attribute arena per document instead of a `Vec` per
element (and 52- instead of 64-byte nodes), no per-tag allocations at all
(attribute buffer reused by the tokenizer, names lower-cased and entities
decoded straight into the string arena), and a word-at-a-time byte search for
every scan — the text-heavy gain is almost all the last one.

### Memory

Heap per parse, from `examples/memory.rs` (a counting global allocator):
**allocations** and bytes **allocated** during the parse, the **peak** of
live heap, and what the result still **retains** once parsing is done.

| input | parser | allocations | allocated | peak | retained (× input) |
|---|---|---:|---:|---:|---:|
| large (420 KiB) | domq | 20 | 2.79 MiB | 2.52 MiB | 2.45 MiB (6.0×) |
| | domq/events | 8,420 | 1.50 MiB | 204 KiB | 0 |
| | tl | 7,226 | 10.9 MiB | 5.52 MiB | 5.52 MiB (13.5×) |
| | scraper | 22,726 | 10.2 MiB | 5.06 MiB | 5.06 MiB (12.3×) |
| | html5ever (rcdom) | 64,733 | 6.37 MiB | 5.09 MiB | 5.09 MiB (12.4×) |
| text-heavy (547 KiB) | domq | 6 | 2.92 MiB | 2.85 MiB | 624 KiB (1.1×) |
| | domq/events | 243 | 369 KiB | 2 KiB | 0 |
| | tl | 262 | 680 KiB | 342 KiB | 342 KiB (0.6×) |
| | scraper | 737 | 1.93 MiB | 1.26 MiB | 1.26 MiB (2.4×) |
| | html5ever (rcdom) | 2,749 | 1.66 MiB | 1.23 MiB | 1.23 MiB (2.3×) |
| table-heavy (712 KiB) | domq | 23 | 11.2 MiB | 7.65 MiB | 7.22 MiB (10.4×) |
| | domq/events | 24,022 | 5.18 MiB | 950 KiB | 0 |
| | tl | 4,029 | 21.3 MiB | 10.7 MiB | 10.7 MiB (15.3×) |
| | scraper | 60,012 | 21.9 MiB | 10.1 MiB | 10.1 MiB (14.6×) |
| | html5ever (rcdom) | 156,045 | 14.9 MiB | 11.0 MiB | 11.0 MiB (15.8×) |

The tree is about half the size of the other parsers' on markup. Its peak
includes arena capacity reserved up front from the input length (sized for
dense markup); on prose most of that is never touched and is given back after
parsing, hence the low retained figure. The event stream's peak is the
nesting depth plus the collected diagnostics (16 bytes each — table-heavy has
~28,000 omitted `</td>`/`</tr>`).

`domq` before / after the 0.5 work:

| input | allocations | peak | retained |
|---|---:|---:|---:|
| medium | 736 → 14 | 136 → 122 KiB (−10%) | 135 → 122 KiB (−10%) |
| large | 14,422 → 20 | 2.79 → 2.52 MiB (−10%) | 2.72 → 2.45 MiB (−10%) |
| text-heavy | 244 → 6 | 3.17 → 2.85 MiB (−10%) | 3.16 MiB → 624 KiB (−81%) |
| table-heavy | 44,026 → 23 | 8.90 → 7.65 MiB (−14%) | 8.47 → 7.22 MiB (−15%) |

### The `simd` feature

`--features simd` swaps the built-in word-at-a-time search for `memchr`
(NEON here; SSE2/AVX2 on x86-64). Alternated runs, two each, lower shown:

| input | default | `--features simd` | change |
|---|---:|---:|---:|
| small | 1.14 µs | 1.12 µs | −2% |
| medium | 42.3 µs | 41.2 µs | −3% |
| large | 834 µs | 812 µs | −3% |
| text-heavy | 72.3 µs | 54.8 µs | **−24%** (7.7 → 10.2 GB/s) |
| table-heavy | 1.72 ms | 1.70 ms | −1% |

Honest summary: worth it for long runs of text (prose, big `<pre>` or
`<script>` blocks), noise-level on markup, where scans are a few bytes long.
It costs nothing when off — the default build has no dependency and the same
code as before — and both searches are unit-tested to agree, so the tokens
are identical.

### WebAssembly

The npm package's module (json + query + markdown, `wasm-opt -Oz`) is 40,137
bytes gzipped after this work (`gzip -9`), against 39,707 before — the
arena-wide attribute storage, the SWAR search, the sink-generic builder and
the post-parse shrink cost ~430 bytes — and a CI budget of 40,960. The
streaming API is not in the bindings, so it costs nothing there, and the
`simd` feature is not enabled.
