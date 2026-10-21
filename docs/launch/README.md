# 0.3 launch kit

Drafts and a checklist for announcing 0.3. Nothing here is posted
automatically — edit the drafts in your own voice before using them.
(`docs/launch/` is excluded from the published crate.)

## Checklist

**Ship**

1. Merge the release PR into `main`.
2. Publish the crates, core first:
   ```sh
   cargo publish
   cargo publish -p domtree-cli
   ```
3. Tag and push: `git tag v0.3.0 && git push origin v0.3.0`. The `Release`
   workflow attaches prebuilt `domtree` binaries (Linux, macOS, Windows) and
   `SHA256SUMS` to the GitHub release.
4. npm: one-time *trusted publishing* setup for `dom-tree-wasm` (see
   `docs/PUBLISHING.md`); the tag push then publishes it.
5. Python: add a PyPI *pending trusted publisher* (project `dom-tree-rs`, owner
   `iamsaquib8`, repo `dom-tree-rs`, workflow `python.yml`, environment `pypi`);
   the tag push then publishes wheels.
6. Homebrew: create the public repo `iamsaquib8/homebrew-tap`, and copy the
   formula from the release run's `homebrew-formula` artifact to
   `Formula/domtree.rb` (see `packaging/homebrew/README.md`).
7. Enable GitHub Pages: **Settings → Pages → Source: GitHub Actions**. The
   `Pages` workflow deploys the playground on the next push to `main` (or run
   it by hand from the Actions tab).

**Polish the repo page** (this is what people see when they land)

- **About → Website:** `https://iamsaquib8.github.io/dom-tree-rs/`
- **About → Description:** "Tiny, zero-dependency HTML parser with CSS
  selectors and HTML→Markdown — for Rust, the shell (htmlq-compatible CLI), and
  JS/WASM. Never panics."
- **Topics:** add `cli`, `css-selectors`, `markdown`, `web-scraping`, `htmlq`,
  `llm`, `rag` to the existing ones.
- **Social preview image** (Settings → General): a 1280×640 screenshot of the
  playground or `docs/assets/demo.svg` — links shared on Reddit/HN/X show it.
- Pin a few `good first issue`s from the roadmap (e.g. Homebrew tap,
  `entities-full`, implicit `<tbody>`), so visitors see a way in.

**Announce** — space these out over a few days, and reply to every comment in the
first hours; that matters more than the post itself.

- [ ] r/rust (draft below)
- [ ] Show HN (draft below) — weekday morning, US Eastern time
- [ ] This Week in Rust — PR adding a line under "Project/Tooling Updates" in
      <https://github.com/rust-lang/this-week-in-rust>
- [ ] users.rust-lang.org "announcements" category
- [ ] X / Bluesky / Mastodon (draft below), with the demo GIF or a playground link
- [ ] awesome-rust — PR under "Libraries → Parsing" (and the CLI under
      "Applications → Utilities") once there's some usage

## Draft: r/rust

**Title:** dom-tree-rs 0.3 — a zero-dependency HTML parser with real CSS
selectors, HTML→Markdown, a CLI, and a tiny wasm build

> I've been working on `dom-tree-rs`, a small HTML parser for the "I just need to
> get stuff out of this HTML" job. 0.3 is the release where it became genuinely
> useful, so I'd love feedback.
>
> - **Zero dependencies**, `no_std + alloc`, `forbid(unsafe_code)`. A clean
>   build is ~0.6 s (scraper: 42 crates, ~5 s).
> - **Never panics** on any input — proptested and fuzzed, including selectors
>   and Markdown output; deep nesting can't overflow the stack.
> - **CSS selectors** with `+`/`~`, all attribute operators, `:nth-child()`,
>   `:not()`/`:is()`/`:has()`, and `:contains()`; `Selector::parse` gives
>   positioned errors.
> - **`to_text()` / `to_markdown()`** — readable text and GFM Markdown, handy
>   for feeding pages to LLMs.
> - **`domtree` CLI** — an htmlq-compatible replacement (htmlq hasn't had a
>   release since 2022) with `--markdown`, `--json`, `--count`.
> - **WASM** — the same thing in the browser / Workers / Deno, and a playground:
>   https://iamsaquib8.github.io/dom-tree-rs/
>
> What it deliberately isn't: a WHATWG-compliant parser (use html5ever/scraper)
> or an HTML sanitizer (use ammonia). The roadmap lists the non-goals.
>
> https://github.com/iamsaquib8/dom-tree-rs

## Draft: Show HN

**Title:** Show HN: Domtree – query HTML from the shell with CSS selectors,
output Markdown

> domtree is a tiny CLI (and Rust library, and wasm module) that reads HTML on
> stdin and prints what a CSS selector matches — as HTML, attribute values,
> readable text, Markdown, or JSON:
>
>     curl -sL https://blog.rust-lang.org | domtree -r 'nav, footer' --markdown body
>     curl -s https://news.ycombinator.com | domtree -t '.titleline > a'
>
> It's flag-compatible with htmlq, has zero dependencies, and never panics on
> malformed input. The parser is forgiving rather than spec-exact — it's built
> for extraction, not for browser-identical trees.
>
> Playground (runs the wasm build in your browser):
> https://iamsaquib8.github.io/dom-tree-rs/

## Draft: short social post

> dom-tree-rs 0.3 🦀 — messy HTML in, clean tree / text / Markdown / JSON out.
> Zero deps, never panics, real CSS selectors, an htmlq-compatible CLI, and a
> tiny wasm build. Try it in the browser: https://iamsaquib8.github.io/dom-tree-rs/
