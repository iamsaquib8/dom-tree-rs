# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

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
