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
   places where a ~20 KB wasm module beats a 200 KB parser and there is no DOM.

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
| gzipped wasm, all bindings | **< 30 KB** (CI size gate) |
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

## 0.3 — "One-line scraping"

Make the everyday job — find things, read their text — trivial in every
environment.

**Selectors** (now on by default)

- [ ] `+` / `~` sibling combinators.
- [ ] Attribute operators `^=` `$=` `*=` `~=` `|=` and the `i` flag.
- [ ] Structural pseudo-classes: `:first-child`, `:last-child`, `:only-child`,
      `:nth-child(an+b)`, `:nth-last-child`, `:first-of-type`, `:last-of-type`,
      `:only-of-type`, `:nth-of-type`, `:nth-last-of-type`, `:empty`, `:root`.
- [ ] Logical pseudo-classes: `:not()`, `:is()`, `:where()`, `:has()`.
- [ ] `:contains("text")` — non-standard, but the single most-requested
      scraping helper (jQuery, cheerio, pup all have it). Clearly labelled.
- [ ] `Selector::parse` — compile once, reuse in loops, get a positioned error
      instead of silently matching nothing.
- [ ] Scoped queries: `NodeRef::select`, `select_first`, `matches`, `closest`;
      `Dom::select_first`.

**Getting content out**

- [ ] `to_text()` — readable plain text: block elements become line breaks,
      whitespace collapsed, `<script>`/`<style>` dropped. Not `textContent`.
- [ ] `to_markdown()` — zero-dependency HTML → CommonMark/GFM (headings, lists,
      links, images, code blocks, tables, quotes). For LLM prompts, docs, notes.
- [ ] `inner_html()` / `outer_html()`; element-only sibling helpers.

**Correctness**

- [ ] Windows-1252 remapping for numeric references in `0x80–0x9F`
      (browser behaviour, e.g. `&#150;` → `–`).

**Tools**

- [ ] **`domtree` CLI** (`cargo install domtree-cli`) — htmlq-compatible flags
      (`-a`, `-t`, `-r`, `-p`, `-w`, `-B`) plus `--markdown`, `--json`,
      `--count`. Zero dependencies.
- [ ] **WASM / npm 0.3** — expose selectors, text, Markdown and HTML output;
      typed `.d.ts`; publish to npm.
- [ ] **Live playground** on GitHub Pages — paste HTML, try selectors, see the
      tree, JSON, text and Markdown, all in a ~20 KB wasm module.

## 0.4 — "Edit and point"

From read-only to a small HTML rewriter, and from "a tree" to "a tree that knows
where it came from".

- [ ] **Mutation API** — detach/remove nodes, set/remove attributes, replace
      text, insert a parsed fragment; then `to_html()`. Unlocks CLI `--remove`
      for boilerplate stripping before `--markdown`.
- [ ] **Source spans** — `NodeRef::source_range()` mapping each node back to its
      byte range in the input (linters, editors, precise error reporting).
- [ ] **Main-content extraction** — a small, documented heuristic
      (`Dom::main_content()`) that drops nav/header/footer/aside and picks the
      densest text block. The "reader mode" step most LLM pipelines need.
- [ ] **More implicit-close rules** — `<colgroup>`/implicit `<tbody>`,
      `<rb>`/`<rt>`/`<rp>`, and other common omitted-tag cases (still short of
      full insertion modes).
- [ ] **`entities-full` feature** — the complete named-entity set (default stays
      the curated ~130).

## 0.5 — "Fast and lean"

- [ ] **Inline small-attribute storage** — drop the per-element attribute `Vec`
      allocation (closes more of the `tl` gap, no dependency).
- [ ] **Optional `simd` feature** — SIMD/`memchr`-backed scanning for
      text-heavy documents (off by default).
- [ ] **SAX-style streaming events** over the tokenizer for documents you don't
      want to materialize.
- [ ] **Borrowed-tree mode** — an opt-in `Dom<'a>` that borrows from the source
      for near-zero-allocation parsing.
- [ ] **Benchmark depth** — add `html5ever`/`scraper` to the harness; track
      throughput, wasm size and compile time over time in CI.
- [ ] **Differential fuzzing vs `html5ever`** on the subset we claim to support.

## Distribution

Being easy to *get* matters as much as features.

- [ ] Prebuilt CLI binaries on GitHub Releases (Linux/macOS/Windows) and a
      Homebrew tap.
- [ ] npm package kept in lock-step with the crate version.
- [ ] Python bindings (PyO3 wheel) — *exploring*. Python is where most scraping
      and RAG code lives; a tiny, fast, never-panicking parser with selectors and
      Markdown would be useful there.
- [ ] Encoding detection — best-effort charset sniffing for non-UTF-8 input
      (CLI first). *Exploring.*

## Have an idea?

Open an issue describing the use case — ideally naming which of the four people
above it helps. The bar is "does it keep the crate tiny, zero-dep, forgiving,
and honest?" — if yes, let's talk.
