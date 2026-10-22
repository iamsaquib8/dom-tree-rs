# Roadmap

This is a direction, not a promise: order and scope will move with what people
actually report and ask for. It is organised around **the people who reach for
a tiny HTML parser** and the job each of them is trying to finish, not around
parser internals. Issues and PRs that fit the principles below are very welcome.

**Where we are:** 0.3.0 shipped on 2026-10-21. It covers selectors, readable text
and Markdown, editing, source spans, streaming events, reader mode, RAG
chunking, data helpers, a CLI with a linter, and packages for crates.io, npm,
PyPI and Homebrew. See [what shipped](#shipped) and the
[changelog](./CHANGELOG.md). Everything below is what comes next.

## Who it's for

1. **Rust developers pulling data out of HTML** — scrapers, crawlers, feed
   readers, link checkers, test assertions on rendered pages. They want the
   answer in one line, fast compiles, and no 40-crate tree for a two-line job.
2. **Shell users** — `curl … | domtree 'a.title' -a href`. `htmlq` (last release
   2022) and `pup` (2016) are unmaintained; `domtree` is the maintained,
   flag-compatible replacement.
3. **LLM, RAG and agent pipelines** — turn a page into clean Markdown or
   chunks for a prompt or an index, at the edge or on a server, without a
   headless browser.
4. **JavaScript on the edge** — Workers, Deno, Bun, the browser: no DOM, and a
   ~41 KB wasm module beats a 200 KB parser.
5. **Python scraping and data code** — where most scraping and RAG code lives;
   a fast, never-panicking parser with selectors and Markdown.
6. **People who ship HTML** — docs sites, static-site generators, email
   templates: `domtree lint` in CI catches missing alt text, empty links,
   duplicate ids and broken markup with `file:line:col`.

Every item below should make at least one of these people's day better. If it
doesn't, it probably doesn't belong.

## Principles

Every addition is weighed against what defines the project:

1. **Tiny** — small code, especially in WASM.
2. **Zero-dependency by default** — new dependencies must be optional, behind a
   feature or in a separate crate.
3. **Forgiving** — never panic, never hang; recover from real-world mess.
4. **Honest** — no over-claiming. Numbers in the README are reproducible, and
   what we don't do is written down.

If a feature can't keep all four, it goes behind a feature flag, into a
companion crate, or onto the non-goals list.

## Budgets

Numbers we hold ourselves to (✅ = enforced in CI):

| budget | limit | today |
|---|---|---|
| runtime dependencies, default build | **0** ✅ | 0 |
| gzipped wasm, all bindings | **< 45 KiB** ✅ | ~40.8 KB |
| panics or hangs on any input (HTML, selectors, edits) | **none** ✅ proptest + fuzz | none known |
| stack use on deeply nested input | **bounded** ✅ 100k-deep tests | bounded |
| super-linear time on adversarial input | **none** — every fix gets a timing test | none known |
| clean debug build of the library | **< 1 s** (M-series) | 0.55 s |
| allocations per parse | **O(1)**, not O(nodes) | ~20 |
| MSRV | **Rust 1.85** ✅ | 1.85 |

## Non-goals

Deliberately out of scope. Reach for the right tool instead:

- **Full WHATWG tree construction** (adoption agency, automatic
  `<html>`/`<head>`/`<body>`, foster parenting). For browser-identical trees,
  use [`html5ever`](https://crates.io/crates/html5ever). We will *measure* how
  far from the spec we are (see 0.6) rather than pretend.
- **Security sanitization.** A forgiving, non-spec parser disagrees with
  browsers on edge cases, which is exactly what mutation-XSS exploits. Use
  [`ammonia`](https://crates.io/crates/ammonia).
- **Rendering.** No layout, no CSS cascade, no `:hover`, no JavaScript. If a
  page builds its content with script, you need a browser.
- **Networking.** Neither the library nor the CLI fetches URLs; pipe from
  `curl`. The MCP server (0.7) only parses HTML it is given.
- **Speed at any cost.** We stay among the fastest owned-tree parsers, but we
  won't trade safety, correctness or size for a benchmark. If raw speed over a
  borrowed tree is your only metric, use [`tl`](https://crates.io/crates/tl).

---

## How releases are ordered

**Light work ships first, heavy work later.** Each release finishes what it
starts, so the next few releases stay small and frequent, and the big projects
get their own releases once the basics are solid. Every item is tagged:

- **S**: small, a contained change in one area.
- **M**: medium, touches several areas or a binding.
- **L**: large, new infrastructure, a new crate, or a research-style project.

## 0.3.x — Launch feedback (now)

The first weeks after the announcement decide whether this becomes a tool
people rely on.

- [ ] **S** Triage every report within 48 hours; small HTML repros become corpus
      fixtures and regression tests.
- [ ] **S** Patch releases (0.3.x) for any panic, hang, wrong match or wrong
      output, in lock-step across crates.io, npm, PyPI and Homebrew.
- [ ] **S** Turn recurring questions into README cookbook entries and doc
      examples.
- [ ] **S** Label 5–10 `good first issue`s from 0.4 below.

## 0.4 — "Quick wins" (light)

Small, high-value changes that make the existing features nicer to use and the
project cheaper to release.

- [ ] **S** `domtree lint` for teams: `--allow` / `--deny` per rule, a summary
      line, `--format sarif` for GitHub code scanning.
- [ ] **S** `Selection` helpers, chainable and allocation-light:
      `dom.select("a").attrs("href")`, `.texts()`, `.first()`, `.len()`.
- [ ] **S** Reusable parser: `Dom::parse_into(&mut self, html)` reuses the arenas,
      so a crawler parsing millions of pages stops allocating per page.
- [ ] **S** Token-aware chunking: `to_markdown_chunks_by(max, |s| tokens(s))` with
      your own tokenizer, optional overlap, and stable chunk ids for
      incremental re-indexing.
- [ ] **S** LLM-friendly text: links as numbered footnotes and image alt text
      inline, as `to_text` options.
- [ ] **S** Release automation: one command bumps every version in lock-step
      (crate, CLI, wasm, Python) and drafts the changelog; the release workflow
      updates the Homebrew tap itself (token scoped to the tap repo).
- [ ] **S** Supply chain: signed build provenance (GitHub artifact attestations)
      and an SBOM attached to every release.
- [ ] **S** A pre-commit hook for `domtree lint`.
- [ ] **M** Playground: reader-mode, chunks, metadata and lint tabs, within the
      wasm size budget.

## 0.5 — "Parity and reach" (medium)

Bring the bindings and integrations up to the Rust crate, and meet people where
they install software.

- [ ] **M** Python parity: editing API, `events()`, reusable `Selector` objects.
- [ ] **M** JS parity: editing on `Document` and `events()` in the wasm package,
      inside the size budget.
- [ ] **M** Optional `<tbody>`/`<colgroup>` insertion: a builder option for
      browser-identical table structure.
- [ ] **M** Context-aware fragments: `append_html` and `parse_fragment_in("tr",
      …)` parse in the parent's context (a `<td>` inside a `<tr>`).
- [ ] **M** A reusable GitHub Action for `domtree lint` that annotates pull
      requests.
- [ ] **M** Continuous fuzzing on every PR and nightly (ClusterFuzzLite).
- [ ] **M** More package managers: Scoop / winget (Windows), Nix, AUR, and a
      tiny Docker image.
- [ ] **M** Playground "copy as Rust / JS / Python / CLI" for the current
      selector, so it doubles as a code generator.

## 0.6 — "Trust, measured" (heavy)

Earn trust with published numbers, not adjectives. This needs new test
infrastructure, so it gets its own release.

- [ ] **L** Conformance report: run the
      [html5lib-tests](https://github.com/html5lib/html5lib-tests)
      tree-construction suite, publish pass rates per category (marking the
      non-goal categories honestly), and track them in CI.
- [ ] **L** Spec-aligned recovery where it's cheap, chosen by what the
      conformance report says matters (e.g. common formatting-element
      mis-nesting like `<b><i></b></i>`), and only within the size budget.
- [ ] **L** Reader-mode benchmark: score `main_content()` against a labelled
      corpus (for example Mozilla Readability's test pages, if the licence
      allows), publish precision and recall, and tune the heuristic with it.
- [ ] **M** Apply to OSS-Fuzz once eligible; crashers become regression tests.
- [ ] **L** A docs site next to the playground: cookbook, CLI reference, and
      migration guides from htmlq, scraper, BeautifulSoup and cheerio.

## 0.7 — "Declarative scraping and agents" (heavy)

New crates and new protocols: the biggest API additions.

- [ ] **L** `#[derive(FromHtml)]` in an optional companion crate
      (`dom-tree-derive`; the core stays zero-dependency). Declarative
      extraction into structs: `#[html(select = "h1")] title: String`,
      `#[html(select = ".price", attr = "data-value")] price: f64`, `Vec<T>`
      for repeated items, nested structs, `Option<T>`, and errors that name the
      selector that failed. Plus a matching Python dataclass helper.
- [ ] **L** `domtree mcp`: an MCP server over stdio exposing select, text,
      Markdown, chunks, links, metadata and lint on HTML the agent provides
      (still no fetching).

## 0.8 — "Huge documents" (heavy)

- [ ] **L** Streaming selectors: run a `Selector` over `domtree::events` for
      multi-gigabyte dumps without building a tree; CLI `--stream`.
- [ ] **L** `domtree diff a.html b.html`: a structural diff (node-level,
      whitespace-insensitive) for snapshot tests of rendered pages.

## 1.0 — "Stable"

1.0 ships when all of these hold, not on a date:

- [ ] The public API is reviewed against the Rust API Guidelines, with
      `#[non_exhaustive]` where growth is expected and no known breaking
      changes pending.
- [ ] A written MSRV policy, e.g. raised only in minor releases and never past
      the stable release from six months earlier.
- [ ] Conformance and reader-mode numbers are published and tracked.
- [ ] Months of continuous fuzzing with no open crashers.
- [ ] Used by real projects (a "used by" list in the README).
- [ ] SemVer covers everything people script against: the crate API, CLI
      flags, output formats and exit codes, and the JS and Python APIs.

---

## Distribution

Being easy to *get* matters as much as features.

**Done in 0.3.0:** crates.io (`dom-tree-rs`, `domtree-cli`), `cargo binstall`,
release binaries (Linux x86_64/arm64, macOS, Windows) with checksums, WASI build,
Homebrew tap, npm (trusted publishing, provenance), PyPI (trusted publishing,
abi3 wheels), and the playground.

**Planned:** release automation and the Homebrew tap auto-update (0.4); Scoop /
winget / Nix / AUR / Docker (0.5); the docs site (0.6); homebrew-core once the
project meets its notability bar.

## Community

- [ ] `good first issue`s kept stocked; first response on issues within 48
      hours.
- [ ] A "used by" section and credit for every contributor in release notes.
- [ ] GitHub Discussions for questions and show-and-tell, so issues stay
      actionable.

## Exploring

Ideas we like but haven't committed to. Prototypes and use-cases welcome.

- **WASI component (WIT)** so any component runtime can call the parser.
- **More bindings** (C ABI, Ruby, PHP, Go via wasm) if there's demand.
- **Language-server-lite** — lint diagnostics in editors, using source spans.
- **`domtree fmt`** — an HTML formatter built on the pretty-printer and spans.
- **XPath subset** — only with strong demand; CSS covers most real queries.
- **Incremental re-parse** for editors.

## Considered and declined

- **Borrowed-tree mode** — measured in 0.3: string copying is 10–15% of parse
  time on dense markup and nothing on prose, and it would need a lifetime on
  `Dom` or a second API. Numbers in [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md).
- **Fetching URLs**, **sanitization**, **full WHATWG** — see non-goals.

---

## Shipped

### 0.3.0 — 2026-10-21 · "One-line scraping"

Everything below shipped in 0.3.0 (what was planned as phases 0.3–0.7, plus
distribution and two rounds of review fixes). Details in the
[changelog](./CHANGELOG.md).

**Selectors** (the `query` feature, now on by default)

- [x] `+` / `~` sibling combinators.
- [x] Attribute operators `^=` `$=` `*=` `~=` `|=` with the `i` / `s` flags;
      HTML's case-insensitive enumerated attributes (`type`, `method`, …).
- [x] Structural pseudo-classes: `:first-child`, `:last-child`, `:only-child`,
      `:nth-child(An+B [of S])`, `:nth-last-child`, `:first-of-type`,
      `:last-of-type`, `:only-of-type`, `:nth-of-type`, `:nth-last-of-type`,
      `:empty`, `:root`, `:scope`.
- [x] Logical pseudo-classes: `:not()`, `:is()`, `:where()`, `:has()` with
      relative selectors.
- [x] `:checked`, `:disabled`, `:enabled`, CSS escapes (`.md\:flex`), and the
      non-standard `:contains("text")`, clearly labelled.
- [x] `Selector::parse` / `FromStr` with positioned errors; `select` accepts
      `&str` or a compiled `&Selector`.
- [x] Scoped queries: `NodeRef::select`, `select_first`, `matches`, `closest`;
      `Dom::select_first`.
- [x] Linear-time matching: per-query memo for `:nth-*`, sibling and ancestor
      searches; bounded memo memory; scoped queries pay only for what they walk.

**Getting content out**

- [x] `to_text()`: readable text (blocks become lines, whitespace collapsed,
      `<head>`/`<script>`/`<style>`/`<title>` dropped).
- [x] `to_markdown()`: zero-dependency CommonMark + GFM (headings, emphasis,
      links, images, nested lists, `<li value>`/`<ol reversed>`, code blocks
      with languages, quotes, tables).
- [x] `inner_html()`, `outer_html()`, `next_element_sibling()`,
      `prev_element_sibling()`.
- [x] Windows-1252 remapping for numeric references `&#128;`–`&#159;`.

**Edit and point**

- [x] Editing: `remove`, `set_attr`, `remove_attr`, `set_text`, `rename`,
      `create_element` / `create_text`, `append_child`, `insert_before`,
      `wrap` / `unwrap`, `append_html` / `prepend_html` /
      `insert_html_before` / `insert_html_after`. No cycles; document order is
      kept after every edit; `is_detached()`.
- [x] Source spans: `NodeRef::source_range()` and `domtree::line_col()`.
- [x] Top-level nodes linked as siblings (`li + li` works on fragments).
- [x] More WHATWG optional-tag rules: the full `<p>`-closing list, headings,
      unclosed `<head>`, ruby, `<hr>` in `<select>`, captions / `colgroup` / `col`.
- [x] `entities-full` feature: all 2,125 WHATWG named references (default: the
      178 most common).

**Fast and lean**

- [x] One attribute arena per document; 52-byte nodes; no per-tag allocations
      (~20 allocations per parse); unused arena capacity released after parsing.
- [x] Word-at-a-time byte scanning; optional `simd` feature (`memchr`).
- [x] Parsing 25–37% faster on markup and 69% on prose, ahead of `tl` on large
      documents.
- [x] `domtree::events()`: streaming parse events with the same recovery as
      the tree builder.
- [x] Benchmarks against `tl`, `scraper` and `html5ever`, a memory report, and a
      weekly benchmark workflow.
- [x] Differential fuzzing against html5ever on the supported subset.
- [x] Borrowed-tree mode evaluated and declined (see above).

**Structure and data**

- [x] `Dom::main_content()`: deterministic reader-mode heuristic.
- [x] `to_markdown_chunks()`: heading-aware RAG chunks with heading paths.
- [x] `Dom::links()` / `resources()` / `base_url()`, honouring `<base href>`
      (but not inside `<template>`).
- [x] `Dom::metadata()`: title, description, canonical, lang, OpenGraph,
      Twitter cards, JSON-LD and feeds.
- [x] `NodeRef::table_rows()` with colspan/rowspan expanded; `domtree::to_csv`.
- [x] `domtree::resolve_url` / `resolve_srcset`: RFC 3986, HTML `srcset` rules,
      and browsers' backslash handling.

**CLI: `domtree`** (htmlq-compatible: `-a -t -p -w -r -b -B -f -o`)

- [x] Output: `--markdown`, `--json`, `--jsonl`, `--csv`, `--count`, `--first`,
      pretty HTML; conflicting formats are a usage error.
- [x] `-e NAME=SELECTOR[@attr][[]]` structured extraction, scoped per match.
- [x] `--main` reader mode, `--links`, `--meta`, `--remove-nodes`, `--base`.
- [x] Encoding detection (BOM, `<meta charset>`, UTF-8, Windows-1252) and
      `--encoding`.
- [x] `domtree lint`: alt text, empty links and buttons, duplicate ids, heading
      order and parse errors, as `file:line:col` or JSON, with CI exit codes.

**JavaScript, Python and the browser**

- [x] `dom-tree-wasm` on npm: `parse`, `select`, `selectFirst`, `toText`,
      `toMarkdown`, `toHtml`, `errors`, and a parse-once `Document`; full
      TypeScript types; works in Node with no setup; ~41 KB gzipped.
- [x] Playground: <https://iamsaquib8.github.io/dom-tree-rs/>.
- [x] `dom-tree-rs` on PyPI (`import domtree`): abi3 wheels for 5 platforms,
      typed stubs, the GIL released on large inputs.

**Distribution**

- [x] crates.io: `dom-tree-rs` and `domtree-cli`.
- [x] Release binaries for Linux x86_64/arm64, macOS Intel/Apple Silicon and
      Windows, with `SHA256SUMS`; `cargo binstall domtree-cli`.
- [x] WASI build (`domtree.wasm`, runs under wasmtime).
- [x] Homebrew: `brew install iamsaquib8/tap/domtree`.
- [x] npm and PyPI publishing from CI with trusted publishing (no stored
      tokens; npm with provenance).

**Quality and hardening**

- [x] `Dom::errors()` no longer flags valid HTML (a bare `&`, optional end
      tags); positions point at the problem.
- [x] Two independent reviews. All findings fixed: quadratic paths (tag-name
      hash collisions, scoped queries, `append_html`, chunk paths, nested tables,
      CLI `--links`/`-e`), markup injection through edited content (raw-text
      escaping, name validation), and URL/`<base>` edge cases.
- [x] Never-panic proptests for selectors, output, editing and source spans;
      fuzz targets for `parse`, `tokenize`, `select`, `markdown` and
      `differential`; 100k-deep nesting tests.
- [x] CI: tests (stable/beta), MSRV 1.85, `no_std`, clippy, fuzz smoke, wasm
      size gate, zero-dependency gate, Python, WASI, Pages, release.
- [x] Project: README redesign, `CODE_OF_CONDUCT.md`, `SECURITY.md`,
      `docs/ARCHITECTURE.md`, issue/PR templates, dependabot, launch kit.

### 0.2 — 2026-06

Never-panicking, error-tolerant parser; arena tree with O(1) traversal; a CSS
subset; zero-dependency JSON; `to_html()`; `no_std + alloc`; WASM bindings;
property tests, snapshot corpus, fuzzing and CI.

## Have an idea?

Open an issue describing the use case, ideally naming which of the six groups
above it helps. The bar is "does it keep the project tiny, zero-dependency,
forgiving and honest?" If yes, let's talk.
