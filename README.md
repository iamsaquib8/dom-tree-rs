<div align="center">

<img src="docs/assets/logo.svg" width="96" height="96" alt="">

# dom-tree-rs

**Messy HTML in. Clean tree, text, Markdown, or JSON out.**

A tiny, zero-dependency, never-panicking HTML parser with CSS selectors —
for Rust, your shell, and JavaScript (via a ~20 KB WebAssembly module).

[![crates.io](https://img.shields.io/crates/v/dom-tree-rs.svg)](https://crates.io/crates/dom-tree-rs)
[![docs.rs](https://img.shields.io/docsrs/dom-tree-rs)](https://docs.rs/dom-tree-rs)
[![CI](https://github.com/iamsaquib8/dom-tree-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/iamsaquib8/dom-tree-rs/actions/workflows/ci.yml)
![dependencies](https://img.shields.io/badge/dependencies-0-success)
![no_std](https://img.shields.io/badge/no__std-yes-success)
[![MSRV](https://img.shields.io/badge/MSRV-1.85-blue)](#msrv)
[![license](https://img.shields.io/github/license/iamsaquib8/dom-tree-rs)](./LICENSE)

[**▶ Try it in your browser**](https://iamsaquib8.github.io/dom-tree-rs/) ·
[Rust docs](https://docs.rs/dom-tree-rs) ·
[CLI](./cli) ·
[JavaScript](./dom-tree-wasm) ·
[Roadmap](./ROADMAP.md)

<img src="docs/assets/demo.svg" width="760" alt="Terminal demo: domtree prints a page's heading as text, its links as URLs, a fragment as Markdown, and counts images missing alt text.">

</div>

---

```rust
let dom = domtree::parse(r#"<ul id=links><li><a href="/a">One</a><li><a href="/b">Two</a></ul>"#);

for a in dom.select("#links a[href]") {
    println!("{} -> {}", a.text_content(), a.attr("href").unwrap_or_default());
}
// One -> /a
// Two -> /b        (the missing </li>s were recovered)

let md = domtree::parse("<h2>Notes</h2><p>Some <b>bold</b> text</p>").to_markdown();
assert_eq!(md, "## Notes\n\nSome **bold** text");
```

## Pick your entry point

<table>
<tr>
<th width="33%">🦀 Rust library</th>
<th width="33%">🖥️ Command line</th>
<th width="33%">🌐 JavaScript / WASM</th>
</tr>
<tr valign="top">
<td>

```toml
[dependencies]
dom-tree-rs = "0.3"
```

```rust
let dom = domtree::parse(html);
dom.select_first("h1")
   .map(|h| h.to_text());
```

</td>
<td>

```sh
cargo install domtree-cli
```

```sh
curl -s example.com \
  | domtree -t h1
```

</td>
<td>

```sh
npm install dom-tree-wasm
```

```js
import init, { select } from "dom-tree-wasm";
await init();
select(html, "a[href]");
```

</td>
</tr>
</table>

## Why dom-tree-rs?

- **Zero dependencies.** One crate in your `cargo tree`. A clean build takes
  **0.3 s** — `scraper` pulls in 42 crates and takes 5.2 s.¹
- **Never panics.** Any byte sequence gives you a best-effort tree. Proptested and
  fuzzed: `parse`, `select`, `to_text`, `to_markdown`. Deep nesting can't blow the
  stack.
- **Real CSS selectors.** Combinators (` `, `>`, `+`, `~`), every attribute
  operator, `:nth-child()`, `:not()`, `:is()`, `:has()`, and the scraper's
  favourite, `:contains("text")`.
- **Content out, not just a tree.** Readable text (`to_text`), CommonMark/GFM
  Markdown (`to_markdown`) for LLM prompts and RAG pipelines, JSON, and HTML.
- **Runs everywhere.** `no_std + alloc`, `#![forbid(unsafe_code)]`, a ~20 KB
  gzipped wasm module for browsers, Cloudflare Workers, Deno, Bun and Node, and a
  single static binary for your shell.
- **Fast enough.** ~250 MiB/s — within 5–15 % of [`tl`] while building an owned
  tree (see [Performance](#performance)).

<sub>¹ Clean `cargo build` of a crate depending only on the parser, Apple M4 Pro.
dom-tree-rs 0.28 s · tl 0.36 s · html5ever 2.7 s · scraper 5.2 s.</sub>

## Cookbook

<details open>
<summary><b>Scrape links, titles, and attributes</b></summary>

```rust
let dom = domtree::parse(html);

let title = dom.select_first("title").map(|t| t.to_text());
let links: Vec<&str> = dom.select("a[href^='https://']").filter_map(|a| a.attr("href")).collect();
let price = dom.select_first(".product:has(.in-stock) .price").map(|p| p.to_text());
```
</details>

<details>
<summary><b>Turn a web page into Markdown for an LLM</b></summary>

```rust
let dom = domtree::parse(html);
let article = dom.select_first("article, main").map(|n| n.to_markdown());
```

Or from the shell, stripping the chrome first:

```sh
curl -s https://example.com/post | domtree -r 'nav, footer, script' --markdown article
```
</details>

<details>
<summary><b>Read a table</b></summary>

```rust
for row in dom.select("table#prices tr:has(td)") {
    let cells: Vec<String> = row.select("td").map(|td| td.to_text()).collect();
    println!("{}", cells.join(" | "));
}
```
</details>

<details>
<summary><b>Compile a selector once, reuse it, and get real errors</b></summary>

```rust
use domtree::Selector;

let sel = Selector::parse("li:nth-child(odd) > a").expect("valid selector");
for page in pages {
    let dom = domtree::parse(page);
    let count = dom.select(&sel).count();
}

let err = Selector::parse("a:hover").unwrap_err();
println!("{err}"); // explains what's unsupported, and where
```
</details>

<details>
<summary><b>Walk the tree</b></summary>

```rust
let link = dom.select_first("a").unwrap();
let section = link.closest("section");          // nearest matching ancestor
let parent = link.parent();                     // O(1) — arena-backed
let next = link.next_element_sibling();
for node in link.ancestors() { /* … */ }
for node in dom.root().unwrap().descendants() { /* … */ }
```
</details>

<details>
<summary><b>Edit and re-serialize</b></summary>

```rust
let mut dom = domtree::parse("<p>Hi <script>track()</script><a href=/x>x</a></p>");
let scripts: Vec<_> = dom.select("script").map(|n| n.id()).collect();
for id in scripts {
    dom.remove(id);
}
assert_eq!(dom.to_html(), r#"<p>Hi <a href="/x">x</a></p>"#);
```
</details>

## Command line

`domtree` is a maintained, zero-dependency, drop-in replacement for
[`htmlq`](https://github.com/mgdm/htmlq) — same flags, plus `--markdown`,
`--json`, `--count` and modern selectors. [Full CLI docs →](./cli)

```sh
curl -s https://news.ycombinator.com | domtree -t '.titleline > a'     # headlines
curl -s https://www.rust-lang.org | domtree -B -b https://www.rust-lang.org -a href a
domtree -c 'img:not([alt])' < page.html                                 # accessibility check
echo '<ul><li>a<li>b</ul>' | domtree -p                                 # pretty-print
```

## JavaScript & WebAssembly

The same parser in a ~20 KB (gzipped) wasm module with zero JS dependencies —
works in the browser, Cloudflare Workers, Deno, Bun, and Node.
[Package docs →](./dom-tree-wasm) · [**Playground →**](https://iamsaquib8.github.io/dom-tree-rs/)

```js
import init, { parse, select, toMarkdown } from "dom-tree-wasm";
await init();

const doc = JSON.parse(parse("<ul><li>a<li>b</ul>"));  // typed tree (see dom-node.d.ts)
const links = select(html, "a[href]");                 // [{ tag, attrs, text, html }, …]
const md = toMarkdown(html);
```

## How it compares

| | dom-tree-rs | [tl] | [scraper] | [html5ever] | [lol_html] |
|---|:---:|:---:|:---:|:---:|:---:|
| Crates in a clean build | **1** | 2 | 43 | 20 | 11+ |
| CSS selectors | **✅** | limited | ✅ | ❌ | ✅ |
| Text / Markdown output | **✅** | ❌ | ❌ | ❌ | ❌ |
| JSON output | **✅** | ❌ | ❌ | ❌ | ❌ |
| WASM + npm package | **✅** | ✅ | ❌ | ❌ | ❌ |
| CLI | **✅** | ❌ | ❌ | ❌ | ❌ |
| `no_std` | **✅** | ❌ | ❌ | ❌ | ❌ |
| Edit + re-serialize | basic | ❌ | ❌ | ✅ | ✅ (streaming) |
| Never panics on input | ✅ | ✅ | ✅ | ✅ | ✅ |
| WHATWG spec-compliant tree | ❌ | ❌ | ✅ | ✅ | ❌ |

**When to pick something else:** if you need byte-exact browser parsing, use
[`html5ever`]/[`scraper`]. If raw throughput is all that matters, use [`tl`]. If
you're sanitizing untrusted HTML, use [`ammonia`](https://crates.io/crates/ammonia)
— never a forgiving, non-spec parser.

[tl]: https://crates.io/crates/tl
[scraper]: https://crates.io/crates/scraper
[html5ever]: https://crates.io/crates/html5ever
[lol_html]: https://crates.io/crates/lol_html

## Performance

Parsing is fast *for an owned tree*. Synthetic article workload, Apple M-series,
single core, `--release`:

| input | dom-tree-rs | tl |
|---|---:|---:|
| ~1 KB   | ~256 MiB/s | ~308 MiB/s |
| ~50 KB  | ~250 MiB/s | ~297 MiB/s |
| ~1 MB   | ~248 MiB/s | ~257 MiB/s |

html5ever benches around ~50 MB/s in the community
[rust-html-parser-benchmark]. Run ours with `cd benchmarks && cargo bench`.

[rust-html-parser-benchmark]: https://github.com/y21/rust-html-parser-benchmark

## What it handles

Void elements, self-closing tags, comments, doctypes, CDATA, raw text
(`<script>`/`<style>`), escapable raw text (`<title>`/`<textarea>`), boolean and
unquoted attributes, hyphenated/namespaced/custom tags, named + numeric entities
(with browser-style Windows-1252 remapping), and implicit closing for the everyday
cases (`<li>`, `<p>`, table cells/rows, `<dt>`/`<dd>`, `<option>`). Everything it
recovers from is recorded in
[`Dom::errors`](https://docs.rs/dom-tree-rs/latest/domtree/struct.Dom.html#method.errors).

**Deliberately not done:** full WHATWG tree construction (adoption agency, automatic
`<html>`/`<head>`/`<body>`, foster parenting). See the [non-goals](./ROADMAP.md#non-goals).

## Cargo features

| feature | default | what it adds |
|---|:---:|---|
| `json` | ✅ | `to_json()` — zero-dependency JSON |
| `query` | ✅ | CSS selectors: `select`, `select_first`, `Selector`, `closest`, `matches` |
| `markdown` | ✅ | `to_markdown()` — zero-dependency HTML → Markdown |
| `serde` | | `serde::Serialize` for the tree (same JSON shape; adds `serde`) |
| `std` | | `std::error::Error` impls; the crate is `no_std + alloc` otherwise |

## MSRV

Rust **1.85** (edition 2024), enforced in CI.

## Quality

Property-tested ("never panics" + tree invariants), snapshot-tested against a
corpus of messy HTML, fuzzed with `cargo-fuzz`, and CI-checked for fmt, clippy,
`no_std`, MSRV, zero dependencies and a wasm size budget.

## Contributing

Issues and PRs are welcome — see [CONTRIBUTING.md](./CONTRIBUTING.md) and the
[roadmap](./ROADMAP.md). If dom-tree-rs saved you some time, a ⭐ helps other
people find it.

## License

[MIT](./LICENSE)
