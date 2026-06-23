# dom-tree-rs

[![CI](https://github.com/iamsaquib8/dom-tree-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/iamsaquib8/dom-tree-rs/actions/workflows/ci.yml)
[![license](https://img.shields.io/github/license/iamsaquib8/dom-tree-rs)](./LICENSE)
[![MSRV](https://img.shields.io/badge/MSRV-1.70-blue)](#msrv)
![no_std](https://img.shields.io/badge/no__std-yes-success)
![dependencies](https://img.shields.io/badge/dependencies-0-success)

<!-- Re-enable these after `cargo publish` (see docs/PUBLISHING.md) — until then
     they render as "not found":
[![crates.io](https://img.shields.io/crates/v/dom-tree-rs.svg)](https://crates.io/crates/dom-tree-rs)
[![docs.rs](https://img.shields.io/docsrs/dom-tree-rs)](https://docs.rs/dom-tree-rs)
-->


**A tiny, zero-dependency, forgiving HTML parser.** Turn messy real-world HTML
into a clean tree (and JSON) — in the browser, on the edge, or anywhere you
don't want to pull in a 100 KB+ parser. It **never panics** on bad input.

```rust
let dom = domtree::parse(r#"<ul id="list"><li class="item">Item 1<li>Item 2"#);

let items: Vec<_> = dom.find_by_tag("li").map(|li| li.text_content()).collect();
assert_eq!(items, ["Item 1", "Item 2"]); // note the missing </li> — recovered

println!("{}", dom.to_json()); // self-describing JSON tree, zero deps
```

> **What it is not:** a spec-compliant WHATWG parser (use
> [`html5ever`](https://crates.io/crates/html5ever)) or a raw-throughput record
> (use [`tl`](https://crates.io/crates/tl)). It's the *smallest* way to turn
> arbitrary HTML into a usable tree.

## Why

Most Rust HTML parsers are either spec-grade and heavy (`html5ever`, `scraper`,
`lol_html` — 5–11 dependencies, hundreds of KB of WASM) or fast and low-level
(`tl`). `dom-tree-rs` fills a specific gap:

- **Zero dependencies** in the default build. Your `cargo tree` stays clean.
- **WASM-first**: a ~19 KB-gzipped module that parses HTML to JSON in any JS
  runtime (browser / Deno / Cloudflare Workers / Node).
- **Built-in JSON** output — no `serde` required (it's an optional feature).
- **Forgiving**: missing end tags, void elements, mis-nesting, stray tags,
  unquoted/boolean attributes, comments, doctypes, `<script>`/`<style>`,
  entities — all handled, and it never panics.
- **`no_std` + `alloc`** compatible.
- **Ergonomic tree**: `parent`/`children`/`descendants`/`ancestors`,
  `find_by_tag/id/class`, and optional CSS-selector querying.

## How it compares

| | dom-tree-rs | [tl] | [scraper] | [html5ever] | [lol_html] |
|---|:---:|:---:|:---:|:---:|:---:|
| Default dependencies | **0** | 0 | 5+ | 10+ | 11+ |
| WASM + npm package | **✅** | ✅ | ❌ | ❌ | ❌ |
| Built-in JSON output | **✅** | ❌ | ❌ | ❌ | ❌ |
| CSS-selector querying | subset¹ | limited | ✅ | ❌ | ✅ |
| Streaming tokenizer | ✅ | ❌ | ❌ | partial | ✅ |
| `no_std` | **✅** | ❌ | ❌ | ❌ | ❌ |
| Never panics on input | ✅ | ✅ | ✅ | ✅ | ✅ |
| WHATWG spec-compliant | ❌ | ❌ | ✅ | ✅ | ❌ |

¹ behind the `query` feature.

[tl]: https://crates.io/crates/tl
[scraper]: https://crates.io/crates/scraper
[html5ever]: https://crates.io/crates/html5ever
[lol_html]: https://crates.io/crates/lol_html

## Performance

Parsing is fast *for an owned tree*. On a synthetic article workload (Apple
M-series, single core, `--release`):

| input | dom-tree-rs | tl | html5ever† |
|---|---:|---:|---:|
| ~1 KB   | ~256 MiB/s | ~308 MiB/s | — |
| ~50 KB  | ~250 MiB/s | ~297 MiB/s | — |
| ~1 MB   | ~248 MiB/s | ~257 MiB/s | — |

We land within ~5–15 % of `tl` (a SIMD speed specialist) and roughly on par on
large documents, while staying zero-dependency and producing an *owned* tree.
`tl` will usually win on raw throughput — **if speed is your only goal, use
`tl`.** † html5ever benches around ~50 MB/s in the community
[rust-html-parser-benchmark]; we don't include it in our harness to keep the
benchmark crate dependency-light. Run the numbers yourself:

```sh
cd benchmarks && cargo bench
```

[rust-html-parser-benchmark]: https://github.com/y21/rust-html-parser-benchmark

## Install

```toml
[dependencies]
dom-tree-rs = "0.2"
```

The library is imported as `domtree`:

```rust
use domtree::parse;
```

## Usage

### Traverse

```rust
let dom = domtree::parse("<article><h1>Title</h1><p>Body <a href=\"/x\">link</a></p></article>");

let h1 = dom.find_by_tag("h1").next().unwrap();
assert_eq!(h1.text_content(), "Title");

for a in dom.find_by_tag("a") {
    println!("{} -> {}", a.text_content(), a.attr("href").unwrap_or(""));
}

// arena-backed: parent / siblings / ancestors are O(1) per step
let link = dom.find_by_tag("a").next().unwrap();
assert_eq!(link.parent().unwrap().tag_name(), Some("p"));
```

### Query with CSS selectors (`query` feature)

```toml
dom-tree-rs = { version = "0.2", features = ["query"] }
```

```rust
# #[cfg(feature = "query")] {
let dom = domtree::parse("<ul class=links><li><a href=/a>a</a><li><a href=/b rel=next>b</a></ul>");
assert_eq!(dom.select("ul.links a[href]").count(), 2);
assert_eq!(dom.select("a[rel=next]").next().unwrap().text_content(), "b");
# }
```

Supported: type / `*` / `#id` / `.class` / `[attr]` / `[attr=value]`,
descendant (` `) and child (`>`) combinators, and `,` selector lists.

### JSON

```rust
let dom = domtree::parse("<p>hi</p>");
let json = dom.to_json();
// {"type":"document","errors":0,"children":[
//   {"type":"element","tag":"p","attrs":{},"children":[
//     {"type":"text","value":"hi"}]}]}
```

With the `serde` feature, the tree also implements `serde::Serialize` (same JSON
shape).

### WebAssembly

See [`dom-tree-wasm/`](./dom-tree-wasm) — parse HTML to JSON from JavaScript with
a ~19 KB-gzipped module and zero JS dependencies.

```js
import init, { parse } from "dom-tree-wasm";
await init();
const doc = JSON.parse(parse("<ul><li>a<li>b</ul>"));
```

## What it handles

Void elements (`<br>`, `<img>`, …), self-closing tags, comments, doctypes,
CDATA, raw text (`<script>`/`<style>`), escapable raw text
(`<title>`/`<textarea>`), boolean and unquoted attributes,
hyphenated/namespaced/custom tags, the common HTML entities (named + numeric),
and implicit closing for the everyday cases (`<li>`, `<p>`, table cells/rows,
`<dt>`/`<dd>`, `<option>`).

## What it deliberately does not do

It is **not** a full WHATWG tree builder: no adoption-agency algorithm, no
automatic `<html>`/`<head>`/`<body>` insertion, no foster parenting, and only a
curated subset (~130) of named entities. Everything it recovers from is recorded
in [`Dom::errors`](https://docs.rs/dom-tree-rs/latest/domtree/struct.Dom.html#method.errors).
If you need byte-exact browser parsing, use `html5ever`.

## Features

| feature | default | what it adds |
|---|:---:|---|
| `json` | ✅ | the zero-dependency `Dom::to_json` serializer |
| `serde` | | `serde::Serialize` for the tree (adds the `serde` dep) |
| `query` | | the CSS-subset selector engine, `Dom::select` |
| `std` | | `std`-only conveniences; the crate is `no_std + alloc` otherwise |

## MSRV

Rust **1.70**.

## Quality

Property-tested for "never panics" + tree invariants (proptest, thousands of
cases per run), snapshot-tested against a corpus of messy HTML, and CI runs
fmt / clippy / a `no_std` build / a wasm size gate / `cargo-fuzz`.

## License

[MIT](./LICENSE).
