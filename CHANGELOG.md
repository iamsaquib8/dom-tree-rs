# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- **Editing, part 2:** `Dom::set_text`, `rename`, `create_element`,
  `create_text`, `append_child`, `insert_before`, `wrap`, `unwrap`, and
  parsed-fragment insertion (`append_html`, `prepend_html`,
  `insert_html_before`, `insert_html_after`, returning the inserted ids). Moves
  never create cycles; `nodes()`, `select` and `find_by_*` stay in document
  order after any edit. `NodeRef::is_detached()`.
- **Source spans:** `NodeRef::source_range()` (byte range in the input) and
  `domtree::line_col(src, byte)`.
- **`entities-full` feature:** all 2,125 WHATWG named character references
  (off by default; the default table has the 178 most common).
- `Dom::main_content()` — a deterministic "reader mode" heuristic (semantic
  markup first, then Readability-style paragraph scoring) that picks the
  article and drops nav, sidebars, comments, ads and footers.
- `Dom::to_markdown_chunks(max_chars)` / `NodeRef::to_markdown_chunks` —
  heading-aware Markdown chunks for RAG (`Chunk { heading_path, markdown }`,
  `Chunk::context()`), never splitting inside code blocks or tables unless a
  single block exceeds the budget.
- `Dom::links(base)` (readable link text, resolved URLs, `<base href>`,
  `javascript:` skipped), `Dom::resources(base)` (every embedded URL incl.
  `srcset` candidates), `Dom::base_url(base)`.
- `Dom::metadata()` — title, description, canonical, lang, site name, image,
  author, published date, keywords, OpenGraph and Twitter fields, raw JSON-LD
  and feed links (`serde::Serialize` under `serde`).
- `NodeRef::table_rows()` (colspan/rowspan expanded, bounded on hostile input)
  and `domtree::to_csv` (RFC 4180).
- `domtree::resolve_url` / `resolve_srcset` — zero-dependency RFC 3986 and
  spec-compliant `srcset` resolution, moved into the library from the CLI.
- **Python package** `dom-tree-rs` on PyPI (import `domtree`): `parse`,
  `Document.select`/`select_first`/`to_text`/`to_markdown`/`to_html`/`to_json`,
  `Element` with attributes, text, HTML and traversal; typed stubs; abi3 wheels
  for Linux, macOS and Windows; the GIL is released while parsing large inputs.
- **Homebrew formula** for the `domtree` CLI (`packaging/homebrew/`), filled in
  with release checksums by the release workflow.
- **WASI build** of the CLI (`domtree.wasm`, attached to releases, smoke-tested
  under wasmtime), including quiet exit on a closed stdout pipe.
- **CLI structured extraction:** `domtree -e NAME=SELECTOR[@attr][[]]` builds a
  JSON object of named fields, or one object per match of a scope selector
  (JSON array, or `--jsonl`).
- **CLI output modes:** `--jsonl` (JSON Lines), `--csv` (tables as RFC 4180
  CSV), `--main` (reader-mode narrowing before selecting), `--links`
  (url/text, TSV or JSON), `--meta` (page metadata as JSON).
- **`domtree lint`:** images without `alt`, empty links and buttons, duplicate
  ids, skipped heading levels, and every recovered parse error, as
  `file:line:col: level: message [rule]` or `--format json`; exit 0/1/2 for CI.
  Omitted optional end tags are notes, missing required ones warnings.
- **CLI encoding detection:** BOM, `<meta charset>`, UTF-8, then Windows-1252;
  `--encoding` override.
- Prebuilt CLI binaries for Linux arm64.

### Changed
- More implicit-close rules (WHATWG optional tags): the full list of elements
  that close `<p>`; a heading closes an open heading; an unclosed `<head>` ends
  at the first body content (previously such pages lost their body in
  `to_text`); ruby `rb`/`rt`/`rtc`/`rp`; `<hr>` closes an open
  `option`/`optgroup`; captions, `colgroup` and `col` close open cells, rows
  and row groups.
- Nodes are 64 bytes (were 80); `NodeId` is non-zero, so `Option<NodeId>` is
  4 bytes.

- CLI: undeclared non-UTF-8 input now decodes as Windows-1252 (as browsers
  do) instead of lossy UTF-8, e.g. `caf\xe9` → `café`.

### Fixed
- Quadratic parsing of many unmatched end tags over a deep open-element stack,
  and quadratic entity decoding when the lookahead ended inside a multi-byte
  character.
- CLI `--base` keeps commas inside `srcset` URLs, drops empty candidates, and
  rewrites the first of a repeated attribute (the one that counts).

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

- **`dom-tree-wasm` 0.3.0** (npm): `parse` now returns a typed object
  (`parseJson` keeps the string); new `select`, `selectFirst`, `toText`,
  `toMarkdown`, `toHtml`, `errors`, and a parse-once `Document` class. Invalid
  selectors throw a `SelectorError` with a position. Full TypeScript types,
  works in Node with no setup, ~38 KB gzipped. **Breaking:** `parseErrorCount`
  and `query` are removed, and `dom-node.d.ts` is replaced by the generated
  types.
- **Live playground** on GitHub Pages: paste HTML, try selectors, and see the
  tree, text, Markdown, JSON and repaired HTML, all running locally in wasm.
- `CODE_OF_CONDUCT.md`, `SECURITY.md`, `docs/ARCHITECTURE.md`, issue and PR
  templates.

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
