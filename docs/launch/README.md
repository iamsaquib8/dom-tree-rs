# 0.3 launch kit

Drafts and a checklist for announcing 0.3. Nothing here is posted
automatically — edit the drafts in your own voice before using them.
(`docs/launch/` is excluded from the published crate.)

Every number and command below was re-checked against the **published**
0.3.0 on 2026-10-22: clean debug build of a crate depending only on the parser
(Apple M4 Pro) — dom-tree-rs **0.55 s, 0 dependencies**; scraper **3.9 s, 42
crates**. Throughput figures are from [`benchmarks/README.md`](../../benchmarks/README.md).

## Checklist

**Ship** — done

- [x] crates.io: `dom-tree-rs` 0.3.0 and `domtree-cli` 0.3.0 (docs.rs built)
- [x] GitHub release `v0.3.0`: CLI binaries for Linux x86_64/arm64, macOS
      Intel/Apple Silicon, Windows, `domtree.wasm`, `SHA256SUMS`
- [x] npm: `dom-tree-wasm` 0.3.0 (trusted publishing, with provenance)
- [x] PyPI: `dom-tree-rs` 0.3.0 (abi3 wheels for 5 platforms + sdist)
- [x] Homebrew: `brew install iamsaquib8/tap/domtree` (`brew test` passes)
- [x] Playground: <https://iamsaquib8.github.io/dom-tree-rs/>

**Repo page**

- [x] Website, description, topics
- [ ] Social preview image: Settings → General → Social preview → upload
      `docs/assets/social-preview.png`
- [ ] Optional: pin the repo on your profile; enable Discussions (Settings →
      Features) so launch traffic has somewhere to ask questions
- [ ] Optional: open 3–5 `good first issue`s from the roadmap's light (**S**) items
      so visitors see a way in

**Announce** — space these out over a week. Tuesday–Thursday mornings (US
Eastern) get the most eyes. Reply to every comment in the first few hours;
that matters more than the post itself.

- [ ] **Day 1:** r/rust (draft below) — a text post, not a bare link
- [ ] **Day 1:** users.rust-lang.org → *announcements* (reuse the r/rust text)
- [ ] **Day 2–3:** Show HN (draft below)
- [ ] **Same week:** This Week in Rust — PR adding the line below under
      "Project/Tooling Updates" in the current draft in
      <https://github.com/rust-lang/this-week-in-rust/tree/master/draft>
- [ ] **Same week:** X / Bluesky / Mastodon (drafts below), with the demo GIF
      or a playground link
- [ ] **Later:** awesome-rust (they ask for ≥50 stars or ≥2,000 downloads
      first) — the library under *Parsing*, the CLI under *Applications →
      Utilities*

## Draft: r/rust

**Title:** dom-tree-rs 0.3: a zero-dependency HTML parser with real CSS
selectors, HTML→Markdown, and an htmlq-compatible CLI

> I've been working on `dom-tree-rs`, a small HTML parser for the "I just need
> to get stuff out of this HTML" job — scrapers, link checkers, feeding pages to
> an LLM. 0.3 is the release where it grew up, and I'd love feedback.
>
> ```rust
> let dom = domtree::parse(html);
> let title = dom.select_first("h1").map(|h| h.to_text());
> let links: Vec<_> = dom.select("a[href^='https://']").filter_map(|a| a.attr("href")).collect();
> let article = dom.main_content().map(|n| n.to_markdown()); // reader mode → Markdown
> ```
>
> **What's in it**
>
> - **Zero dependencies**, `no_std + alloc`, `#![forbid(unsafe_code)]`. A clean
>   build takes ~0.6 s (scraper: 42 crates, ~4 s).
> - **Fast for an owned tree:** ~500 MB/s on markup — about 8× scraper/html5ever,
>   ahead of `tl` on large documents, ~20 allocations per parse.
> - **Never panics** on any input. Proptested and fuzzed (including selectors,
>   Markdown and editing), differential-fuzzed against html5ever on the subset it
>   claims to handle, and 100k-deep nesting can't blow the stack.
> - **Real CSS selectors:** `+`/`~`, every attribute operator, `:nth-child(An+B of
>   S)`, `:not`/`:is`/`:has`, and the scraper's favourite `:contains()`.
>   `Selector::parse` gives positioned errors.
> - **Content out:** `to_text()`, GFM `to_markdown()`, heading-aware
>   `to_markdown_chunks()` for RAG, `main_content()` reader mode, `links()`,
>   `metadata()` (OpenGraph, JSON-LD…), `table_rows()` → CSV.
> - **Editing + source spans:** remove/wrap/rename/insert HTML, then `to_html()`;
>   every node maps back to its byte range.
> - **Streaming:** `domtree::events()` with the same error recovery as the tree.
>
> **The `domtree` CLI** is a maintained, htmlq-compatible replacement (htmlq's last
> release was 2022) with Markdown/JSONL/CSV output, field extraction and a linter:
>
> ```sh
> curl -sL https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/ | domtree --main --markdown
> curl -sL https://blog.rust-lang.org | domtree --jsonl -e title=a -e url=a@href 'tr:has(a)'
> domtree lint public/**/*.html   # file:line:col, exit 1 on findings
> ```
>
> **Also on npm and PyPI:** a ~41 KB (gzipped) wasm module for the browser,
> Workers, Deno, Bun and Node, and Python wheels. Playground that runs it in your
> browser: https://iamsaquib8.github.io/dom-tree-rs/
>
> ```sh
> cargo add dom-tree-rs
> cargo install domtree-cli       # or: brew install iamsaquib8/tap/domtree
> npm install dom-tree-wasm
> pip install dom-tree-rs
> ```
>
> **What it deliberately isn't:** a WHATWG-compliant tree builder (use
> html5ever/scraper when you need browser-identical trees) or an HTML sanitizer
> (use ammonia). The roadmap lists the non-goals and what's next.
>
> Repo: https://github.com/iamsaquib8/dom-tree-rs — bug reports with a small
> HTML input are the most useful thing you could send me.

## Draft: Show HN

**Title:** Show HN: Domtree – query HTML with CSS selectors, get Markdown, JSON
or CSV

**URL:** https://github.com/iamsaquib8/dom-tree-rs

> domtree reads HTML on stdin and prints what a CSS selector matches — as HTML,
> attribute values, readable text, Markdown, JSON Lines or CSV. It started as a
> maintained replacement for htmlq (flags are compatible) and grew a few things I
> kept writing by hand:
>
>     # an article as Markdown, without nav/sidebars/footer (for an LLM prompt)
>     curl -sL https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/ | domtree --main --markdown
>
>     # structured extraction: one JSON object per post
>     curl -sL https://blog.rust-lang.org | domtree --jsonl -e title=a -e url=a@href 'tr:has(a)'
>
>     # headlines
>     curl -s https://news.ycombinator.com | domtree -t '.titleline > a'
>
>     # lint pages in CI: missing alt text, empty links, duplicate ids, unclosed tags
>     domtree lint public/**/*.html
>
> Under it is a small Rust library (zero dependencies, ~0.6 s clean build, never
> panics on any input) that's also on npm as a ~41 KB wasm module and on PyPI.
> The parser is forgiving rather than spec-exact: it's built for extraction, not
> for browser-identical trees, and it is not a sanitizer.
>
> Playground (the wasm build, running in your browser):
> https://iamsaquib8.github.io/dom-tree-rs/
>
> Install: `brew install iamsaquib8/tap/domtree`, `cargo install domtree-cli`,
> or a binary from the release page.

## Draft: This Week in Rust line

```markdown
* [dom-tree-rs 0.3: zero-dependency HTML parser with CSS selectors, HTML→Markdown, and an htmlq-compatible CLI](https://github.com/iamsaquib8/dom-tree-rs/releases/tag/v0.3.0)
```

## Draft: short social posts

**X (≤280):**

> dom-tree-rs 0.3 🦀 messy HTML in → clean tree, text, Markdown, JSON out.
> Zero deps, never panics, real CSS selectors, ~8× scraper's speed, an
> htmlq-compatible CLI with a linter — plus npm (41 KB wasm) and PyPI.
> Try it in your browser: https://iamsaquib8.github.io/dom-tree-rs/

**Bluesky / Mastodon (longer):**

> Released dom-tree-rs 0.3 🦀 — a tiny HTML parser for getting stuff *out* of
> HTML: CSS selectors, readable text, GFM Markdown (with RAG chunking and reader
> mode), links/metadata/tables, editing.
>
> Zero dependencies, never panics, ~500 MB/s. Same engine as an htmlq-compatible
> CLI (`brew install iamsaquib8/tap/domtree`), a 41 KB wasm module on npm, and
> Python wheels on PyPI.
>
> Playground: https://iamsaquib8.github.io/dom-tree-rs/
> Code: https://github.com/iamsaquib8/dom-tree-rs
>
> #rustlang #webscraping #llm
