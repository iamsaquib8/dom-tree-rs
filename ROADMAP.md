# Roadmap

This is a direction, not a promise. It is organised around **the people who
actually reach for a tiny HTML parser** and what each of them needs to get done,
rather than around parser internals. Issues and PRs that fit the philosophy
below are very welcome.

## Who it's for

1. **Rust developers pulling data out of HTML** — scrapers, crawlers, feed
   readers, link checkers, test assertions on rendered pages. They want
   `select(...)` + text in one line, fast compiles, and no 40-crate dependency
   tree for what is a two-line job.
2. **Shell users** — `curl … | domtree 'a.title' -a href`. The popular tools
   here ([`htmlq`](https://github.com/mgdm/htmlq), last release 2022;
   [`pup`](https://github.com/ericchiang/pup), last release 2016) are
   unmaintained. A small, maintained, drop-in replacement is missing.
3. **LLM / RAG pipelines** — turn a web page into clean text or Markdown to put
   in a prompt or an embedding index, on a server or at the edge, without a
   headless browser or a Python dependency.
4. **JavaScript on the edge** — Cloudflare Workers, Deno, Bun, the browser:
   places where a ~38 KB wasm module beats a 200 KB parser and there is no DOM.

Every item below should make at least one of these people's day better. If it
doesn't, it probably doesn't belong.

## Philosophy

Every addition is weighed against four things that define the crate:

1. **Tiny** — small code size, especially in WASM.
2. **Zero-dependency by default** — new deps must be optional and behind a feature.
3. **Forgiving** — never panic; recover from real-world mess.
4. **Honest** — no over-claiming; document what we don't do.

If a feature can't hold all four, it's either feature-gated or a non-goal.

## Budgets

Numbers we hold ourselves to (CI-enforced where marked):

| budget | limit |
|---|---|
| runtime dependencies, default build | **0** (CI: `cargo tree`) |
| gzipped wasm, all bindings (selectors, text, Markdown) | **< 40 KB** (CI size gate; ~38 KB today) |
| panics on any input — `parse`, `select`, `to_text`, `to_markdown` | **none** (proptest + `cargo-fuzz`) |
| stack usage on deeply nested input | **bounded** (no recursion proportional to input depth) |
| MSRV | Rust 1.85 (CI) |

## Non-goals

These are deliberately *out of scope* — reach for the right tool instead:

- **Full WHATWG tree construction** (adoption-agency algorithm, automatic
  `<html>`/`<head>`/`<body>` insertion, foster parenting, active-formatting
  reconstruction). If you need byte-exact browser parsing, use
  [`html5ever`](https://crates.io/crates/html5ever).
- **Security sanitization.** Do not use this crate to strip XSS from untrusted
  HTML. A forgiving, non-spec parser will disagree with browsers on edge cases,
  and those disagreements are exactly what mutation-XSS attacks exploit. Use
  [`ammonia`](https://crates.io/crates/ammonia).
- **Rendering.** No layout, no CSS cascade, no `:hover`/`:visited`, no script
  execution. If a page builds its content with JavaScript, you need a browser.
- **Networking in the library.** The crate never fetches URLs. The CLI reads
  stdin or files — pipe from `curl`.
- **The raw-throughput crown.** We aim to stay in the same ballpark as
  [`tl`](https://crates.io/crates/tl), not to beat it; if speed is your only
  metric, use `tl`.
- **A heavy dependency graph.** The default build stays at zero external
  runtime dependencies.

## Shipped — 0.2

- Never-panics, error-tolerant parser with browser-like recovery.
- Arena tree with O(1) traversal, `find_by_*`, and a CSS-subset selector engine.
- Zero-dependency JSON output (+ optional byte-identical `serde`).
- `to_html()` re-serialization; `#![forbid(unsafe_code)]`.
- `no_std + alloc`; ~19.5 KB-gzipped WASM bindings.
- Property tests, snapshot corpus, `cargo-fuzz` harness, criterion benches, CI.

## 0.3 — "One-line scraping" (shipped)

Make the everyday job — find things, read their text — trivial in every
environment.

**Selectors** (now on by default)

- [x] `+` / `~` sibling combinators.
- [x] Attribute operators `^=` `$=` `*=` `~=` `|=` and the `i` flag.
- [x] Structural pseudo-classes: `:first-child`, `:last-child`, `:only-child`,
      `:nth-child(an+b)`, `:nth-last-child`, `:first-of-type`, `:last-of-type`,
      `:only-of-type`, `:nth-of-type`, `:nth-last-of-type`, `:empty`, `:root`.
- [x] Logical pseudo-classes: `:not()`, `:is()`, `:where()`, `:has()`.
- [x] `:contains("text")` — non-standard, but the single most-requested
      scraping helper (jQuery, cheerio, pup all have it). Clearly labelled.
- [x] `Selector::parse` — compile once, reuse in loops, get a positioned error
      instead of silently matching nothing.
- [x] Scoped queries: `NodeRef::select`, `select_first`, `matches`, `closest`;
      `Dom::select_first`.

**Getting content out**

- [x] `to_text()` — readable plain text: block elements become line breaks,
      whitespace collapsed, `<script>`/`<style>` dropped. Not `textContent`.
- [x] `to_markdown()` — zero-dependency HTML → CommonMark/GFM (headings, lists,
      links, images, code blocks, tables, quotes). For LLM prompts, docs, notes.
- [x] `inner_html()` / `outer_html()`; element-only sibling helpers.

**Correctness**

- [x] Windows-1252 remapping for numeric references in `0x80–0x9F`
      (browser behaviour, e.g. `&#150;` → `–`).

**Tools**

- [x] **`domtree` CLI** (`cargo install domtree-cli`) — htmlq-compatible flags
      (`-a`, `-t`, `-r`, `-p`, `-w`, `-B`) plus `--markdown`, `--json`,
      `--count`. Zero dependencies.
- [x] **WASM / npm 0.3** — selectors, text, Markdown and HTML output, a
      parse-once `Document`, typed `.d.ts`, zero-setup Node support.
- [x] **Live playground** on GitHub Pages — paste HTML, try selectors, see the
      tree, JSON, text and Markdown, all in a ~38 KB wasm module.

## 0.4 — "Edit and point"

From read-only to a small HTML rewriter, and from "a tree" to "a tree that knows
where it came from".

- [x] **Editing, part 1** — `Dom::remove`, `set_attr`, `remove_attr` (shipped
      early in 0.3; powers the CLI's `--remove-nodes` and `--base`).
- [ ] **Editing, part 2** — replace text, insert a parsed fragment
      (append/prepend/before/after), wrap/unwrap, move nodes, rename elements.
      Turns the crate into a small HTML rewriter.
- [ ] **Source spans** — `NodeRef::source_range()` mapping each node back to its
      byte range in the input, plus a line/column helper (linters, editors,
      precise error reporting).
- [ ] **Main-content extraction** — a small, documented heuristic
      (`Dom::main_content()`) that drops nav/header/footer/aside and picks the
      densest text block. The "reader mode" step most LLM pipelines need.
- [ ] **More implicit-close rules** — `<colgroup>`/implicit `<tbody>`,
      `<rb>`/`<rt>`/`<rp>`, `<optgroup>`, and other common omitted-tag cases
      (still short of full insertion modes).
- [ ] **`entities-full` feature** — the complete WHATWG named-entity set
      (default stays the curated ~130).

## 0.5 — "Fast and lean"

- [ ] **Inline small-attribute storage** — drop the per-element attribute `Vec`
      allocation (closes more of the `tl` gap, no dependency).
- [ ] **Optional `simd` feature** — `memchr`-backed scanning for text-heavy
      documents (off by default; the default build stays zero-dependency).
- [ ] **Streaming events** — a SAX-style API that applies the same recovery as
      the tree builder, for documents you don't want to materialize.
- [ ] **Benchmark depth** — add `html5ever`/`scraper` to the harness; track
      throughput, wasm size and compile time over time in CI.
- [ ] **Differential fuzzing vs `html5ever`** on the subset we claim to support.
- [ ] **Borrowed-tree mode** — *evaluate first*: an opt-in `Dom<'a>` that
      borrows from the source. Ship only if profiling shows a real win;
      otherwise record the numbers here and close it.

## 0.6 — "Structure and data"

Most scraping code is the same five helpers rewritten by hand. Ship them.

- [ ] **Links** — `Dom::links(base)`: every `href`/`src` resolved against a base
      URL (RFC 3986, zero-dependency), with the link text.
- [ ] **Page metadata** — `Dom::metadata()`: title, description, canonical URL,
      language, OpenGraph and Twitter-card fields, and raw JSON-LD blocks.
- [ ] **Tables as data** — `NodeRef::table_rows()` → rows of cell text, with
      `colspan`/`rowspan` expanded; CLI `--csv`.
- [ ] **RAG chunking** — `to_markdown_chunks(max_chars)`: split at headings,
      keep each chunk under a size budget, and carry the heading path
      (`# Guide > ## Install`) so every chunk stands on its own.

## 0.7 — "Tools"

- [ ] **`domtree lint`** — fast HTML checks with `file:line:col` output: images
      without `alt`, empty links and buttons, duplicate ids, skipped heading
      levels, and every recovered parse error. CI-friendly exit codes.
- [ ] **Structured extraction from the shell** — `domtree -e title=h1
      -e price=.price` → one JSON object; `--jsonl` for one object per match.
- [ ] **`--main`** — main-content extraction in the CLI (`curl … | domtree
      --main --markdown`).
- [ ] **Encoding detection in the CLI** — BOM, `<meta charset>`, and
      Windows-1252 fallback for non-UTF-8 pages.

## Ecosystem & distribution

Being easy to *get* matters as much as features.

- [x] Prebuilt CLI binaries on GitHub Releases (Linux/macOS/Windows), also
      installable with `cargo binstall domtree-cli`.
- [x] Homebrew formula (`brew install iamsaquib8/tap/domtree`) — filled in by the
      release workflow (`packaging/homebrew/`).
- [ ] npm package published from CI in lock-step with the crate version.
- [x] **Python package** (`pip install dom-tree-rs`, `import domtree`; PyO3 +
      maturin abi3 wheels, typed stubs, GIL released on large inputs). Python is
      where most scraping and RAG code lives.
- [x] WASI build of the CLI (`domtree.wasm` on releases; runs under `wasmtime`).

## Exploring

Ideas we like but haven't committed to. Comments and prototypes welcome.

- **MCP server mode** — `domtree mcp`, exposing "HTML → Markdown/text/select"
  as tools for LLM agents (the agent fetches; we only parse).
- **HTML diff** — a structural diff of two documents (`domtree diff a.html b.html`),
  useful for snapshot tests of rendered pages.
- **Language-server-lite** — reuse source spans and `lint` for editor
  diagnostics.

## Have an idea?

Open an issue describing the use case — ideally naming which of the four people
above it helps. The bar is "does it keep the crate tiny, zero-dep, forgiving,
and honest?" — if yes, let's talk.
