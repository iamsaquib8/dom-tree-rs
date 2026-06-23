# Roadmap

This is a direction, not a promise — order and scope will shift with feedback.
Issues and PRs that fit the philosophy below are very welcome.

## Philosophy

Every addition is weighed against four things that define the crate:

1. **Tiny** — small code size, especially in WASM.
2. **Zero-dependency by default** — new deps must be optional and behind a feature.
3. **Forgiving** — never panic; recover from real-world mess.
4. **Honest** — no over-claiming; document what we don't do.

If a feature can't hold all four, it's either feature-gated or a non-goal.

## Non-goals

These are deliberately *out of scope* — reach for the right tool instead:

- **Full WHATWG tree construction** (adoption-agency algorithm, automatic
  `<html>`/`<head>`/`<body>` insertion, foster parenting, active-formatting
  reconstruction). If you need byte-exact browser parsing, use
  [`html5ever`](https://crates.io/crates/html5ever).
- **The raw-throughput crown.** We aim to stay in the same ballpark as
  [`tl`](https://crates.io/crates/tl), not to beat it; if speed is your only
  metric, use `tl`.
- **A heavy dependency graph.** The default build stays at zero external
  runtime dependencies.

## Shipped — 0.2

- Never-panics, error-tolerant parser with browser-like recovery.
- Arena tree with O(1) traversal, `find_by_*`, and a CSS-subset selector engine.
- Zero-dependency JSON output (+ optional byte-identical `serde`).
- `no_std + alloc`; ~19.5 KB-gzipped WASM bindings.
- Property tests, snapshot corpus, `cargo-fuzz` harness, criterion benches, CI.

## 0.3 — coverage & ergonomics

- [x] **`to_html()`** — re-serialize a tree (or subtree) back to HTML, with
      correct escaping, void elements, and raw-text handling. _(shipped)_
- [x] **`#![forbid(unsafe_code)]`** — the crate contains no `unsafe`. _(shipped)_
- [ ] **Wider entity coverage** — Windows-1252 remapping for numeric refs in
      `0x80–0x9F` (browser behaviour, e.g. `&#150;` → `–`), and an optional
      `entities-full` feature for the complete named-entity set (default stays
      the curated ~130).
- [ ] **More implicit-close rules** — `<colgroup>`/implicit `<tbody>`, `<rb>`/
      `<rt>`/`<rp>`, and other common omitted-tag cases (still short of full
      insertion modes).
- [ ] **Richer selectors** (`query` feature) — `~`/`+` combinators,
      `[attr^=]`/`[attr$=]`/`[attr*=]`/`[attr~=]`, `:not(...)`, and
      `select_first` / `closest` helpers.
- [ ] **Source spans** — `NodeRef::source_range()` mapping each node back to its
      byte range in the input (useful for linters, editors, and error reporting).

## 0.4 — performance & streaming

- [ ] **Inline small-attribute storage** — a hand-rolled `SmallVec`-style store
      to drop the per-element attribute `Vec` allocation (closes more of the
      `tl` gap, no dependency).
- [ ] **Optional `simd` feature** — SIMD/`memchr`-backed scanning for
      text-heavy documents (off by default, keeps the default build zero-dep).
- [ ] **SAX-style streaming events** — an event callback API over the tokenizer
      for huge documents you don't want to materialize into a tree.
- [ ] **Explore a borrowed-tree mode** — an opt-in `Dom<'a>` that borrows from
      the source for near-zero-allocation parsing, for users who don't need the
      tree to outlive the input.
- [ ] **Benchmark depth** — add `html5ever` to the comparison harness and track
      WASM size + throughput over time in CI.
- [ ] **Differential fuzzing vs `html5ever`** on the subset we claim to support.

## 0.5+ — exploring

- [ ] **Mutation API** — insert / remove / replace nodes and edit attributes,
      then re-serialize (`to_html`). Turns the crate from read-only into a small
      HTML rewriter.
- [ ] **Richer WASM/JS surface** — expose `query` and the error list; an
      optional typed-object return (behind a feature) alongside the default
      string return; a published npm package and a hosted live demo (GitHub
      Pages) for the README.
- [ ] **Encoding detection** — best-effort charset sniffing for non-UTF-8 input.

## Have an idea?

Open an issue describing the use case. The bar is "does it keep the crate tiny,
zero-dep, forgiving, and honest?" — if yes, let's talk.
