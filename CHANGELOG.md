# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

## [0.3.0] — Unreleased

"One-line scraping": find things and get their content out, in Rust, in the
shell, and in JavaScript.

### Added
- **CSS selector engine v2** (`query` feature, now **on by default**):
  `+`/`~` combinators; every attribute operator (`^=` `$=` `*=` `~=` `|=`) with
  the `i`/`s` flags; `:first-child`, `:last-child`, `:only-child`,
  `:nth-child(An+B [of S])`, `:nth-last-child`, the `*-of-type` family,
  `:empty`, `:root`, `:scope`, `:not()`, `:is()`, `:where()`, `:has()` (with
  relative selectors), `:checked`/`:disabled`/`:enabled`, CSS escapes, and the
  non-standard `:contains("text")`. Matching never backtracks exponentially and
  never recurses on document depth.
- `Selector::parse` (and `FromStr`) — compile once, reuse, and get a
  `SelectorError` with a byte position instead of silently matching nothing.
  `Dom::select` accepts `&str`, `&String`, or `&Selector`.
- Scoped queries: `Dom::select_first`, `NodeRef::select`, `select_first`,
  `matches`, `closest`.
- `to_text()` on `Dom`/`NodeRef` — readable plain text (block elements become
  line breaks, whitespace collapsed, `<script>`/`<style>`/`<head>` dropped).
- `to_markdown()` (new `markdown` feature, on by default) — zero-dependency HTML
  → CommonMark + GFM: headings, emphasis, links, images, nested lists, code
  blocks with languages, blockquotes, tables.
- `NodeRef::inner_html`, `outer_html`, `next_element_sibling`,
  `prev_element_sibling`.
- Editing: `Dom::remove` (detach a subtree), `Dom::set_attr`, `Dom::remove_attr`.
- **`domtree` CLI** (`cargo install domtree-cli`) — htmlq-compatible flags
  (`-a`, `-t`, `-p`, `-w`, `-r`, `-b`, `-B`, `-f`, `-o`) plus `--markdown`,
  `--json`, `--count`, `--first`; zero dependencies. Prebuilt binaries are
  attached to GitHub releases.

### Changed
- Top-level nodes are now linked as siblings (`next_sibling`/`prev_sibling`),
  so sibling combinators and `:first-child` work on multi-root fragments.
- Numeric character references in `0x80–0x9F` are remapped per the WHATWG
  Windows-1252 table (`&#150;` → `–`), as browsers do; `&#0;`, surrogates and
  out-of-range values become U+FFFD.
- Default features are now `json`, `query`, `markdown` (still zero
  dependencies). Use `default-features = false` for the smallest build.

## [0.2.1] — 2026-06-24

### Added
- Zero-dependency HTML re-serialization (`Dom::to_html`, `NodeRef::to_html`) —
  round-trips a parsed tree back to HTML with correct escaping and void/raw-text
  handling. Always available (no feature required).
- `#![forbid(unsafe_code)]` — the crate contains no `unsafe`.

### Changed
- Moved to **edition 2024**; minimum supported Rust is now **1.85**
  (developed and tested on the latest stable).

## [0.2.0] — 2026-06-23

A full rewrite. The 0.1 line was a learning-project parser that panicked on
almost any real-world HTML; 0.2 is a robust, tiny, zero-dependency library.

### Added
- Error-tolerant parser that **never panics**; recovery diagnostics via
  `Dom::errors()`.
- Streaming, allocation-light `Tokenizer` (`domtree::tokenize`) implementing
  `Iterator<Item = Token>`.
- Arena-based tree with O(1) `parent`/`children`/`descendants`/`ancestors`/
  sibling traversal through a `Copy` `NodeRef` handle.
- `find_by_tag` / `find_by_id` / `find_by_class` helpers.
- Built-in, zero-dependency JSON serialization (`Dom::to_json`, `NodeRef::to_json`)
  behind the default `json` feature.
- Optional `serde::Serialize` (`serde` feature) producing the identical JSON shape.
- Optional CSS-subset selector engine (`Dom::select`, `query` feature):
  type/`*`/`#id`/`.class`/`[attr]`/`[attr=value]`, descendant and child
  combinators, selector lists.
- `no_std` + `alloc` support; new `std` feature for std-only conveniences.
- Coverage for void elements, self-closing tags, comments, doctype, CDATA,
  raw-text `<script>`/`<style>`, escapable raw text, boolean/unquoted attributes,
  hyphenated/namespaced/custom tags, HTML entities (named + numeric), and
  implicit closing for the common cases.
- Rebuilt WebAssembly bindings (`dom-tree-wasm`): ~19 KB gzipped, zero JS deps.
- `cargo-fuzz` harness, criterion benchmarks, and a full GitHub Actions CI.

### Changed
- **Breaking:** the public API is redesigned. The crate is now used as `domtree`;
  `parse` returns an owned `Dom`; `NodeRef::kind()` returns a borrowed `Copy`
  `NodeKind<'a>`. The old owned `Node`/`Props`/`NodeData` types are gone.
- `serde`/`serde_json` are no longer dependencies of the default build.

### Performance
- ~250 MiB/s on a synthetic article workload — within ~5–15 % of `tl` and
  roughly on par on large documents — while remaining zero-dependency.

## [0.1.1] — 2020
- Initial learning-project parser (HTML → debug tree / `serde_json` string).
