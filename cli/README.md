# domtree — query HTML from your shell

`domtree` reads HTML on stdin and prints what your CSS selector matches — as HTML,
attribute values, readable text, **Markdown**, JSON or CSV. It also pulls named
fields out of a page as JSON, and **lints** HTML for common accessibility and
markup problems. It's a tiny, zero-dependency, never-panicking CLI built on
[`dom-tree-rs`](https://github.com/iamsaquib8/dom-tree-rs), and its flags are
compatible with [`htmlq`](https://github.com/mgdm/htmlq), so it drops into existing
scripts.

```sh
cargo install domtree-cli            # from source
cargo binstall domtree-cli           # prebuilt binary
brew install iamsaquib8/tap/domtree  # Homebrew (macOS / Linux)
```

Prebuilt binaries for Linux (x86_64, arm64), macOS (Intel, Apple Silicon) and
Windows, plus a WASI build (`wasmtime domtree.wasm …`), are attached to every
[GitHub release](https://github.com/iamsaquib8/dom-tree-rs/releases).

## Examples

```sh
# Every navigation link on a page, as absolute URLs
curl -sL https://www.rust-lang.org | domtree -b https://www.rust-lang.org -a href 'nav a'

# Headlines as plain text
curl -s https://news.ycombinator.com | domtree -t '.titleline > a'

# An article as Markdown, without the chrome — ready for an LLM prompt
curl -sL https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/ | domtree --main --markdown

# Every link on a page, absolute, as url<TAB>text
curl -sL https://www.rust-lang.org | domtree -b https://www.rust-lang.org --links

# A table as CSV
domtree --csv -1 '#results' < report.html > results.csv

# Count images that are missing alt text
domtree -c 'img:not([alt])' < page.html

# Lint pages in CI: file:line:col findings, exit 1 on errors or warnings
domtree lint index.html docs/*.html

# Pretty-print a messy fragment
echo '<ul><li>a<li>b</ul>' | domtree -p

# Third row of every table, as JSON
domtree -j 'table tr:nth-child(3)' -f report.html
```

## Options

| flag | what it does |
|---|---|
| `-a, --attribute <NAME>` | print this attribute of each match (no selector = all elements) |
| `-t, --text` | readable text of each match (block elements → line breaks, scripts dropped) |
| `-m, --markdown` | each match as Markdown (CommonMark + GFM tables) |
| `-j, --json` | matches as a JSON array (whole document as JSON with no selector) |
| `--jsonl` | one JSON value per line ([JSON Lines](https://jsonlines.org)): elements, `-a` values, `-e` objects or `--links` |
| `--csv` | matched `<table>`s as RFC 4180 CSV (default selector `table`), separated by a blank line |
| `-c, --count` | number of matches |
| `-p, --pretty` | indent HTML output |
| `-1, --first` | only the first match |
| `-w, --ignore-whitespace` | skip blank results |
| `-e, --extract <NAME=SPEC>` | add a field to a JSON object (repeatable; see below) |
| `--links` | every link as `url<TAB>text` (JSON objects with `-j`/`--jsonl`) |
| `--meta` | page metadata (title, description, canonical, OpenGraph, JSON-LD, …) as JSON |
| `-r, --remove-nodes <SEL>` | remove matching nodes first (repeatable) |
| `--main` | keep only the main content (the article, without navigation, sidebars, footer) |
| `-b, --base <URL>` | make relative `href`/`src`/`srcset`/… URLs absolute |
| `-B, --detect-base` | use the document's `<base href>` |
| `--encoding <LABEL>` | decode the input as `utf-8`, `utf-16le`, `utf-16be` or `windows-1252` |
| `-f, --filename <FILE>` / `-o, --output <FILE>` | read / write a file instead of stdin / stdout |

Several selector arguments are joined with spaces (`domtree ul '>' li`). With no
selector, the whole document is used.

The document is prepared in this order: decode, parse, `-r` removals, `-b`/`-B`
URL resolution, `--main`; then the selector runs. `--meta` always describes the
whole page, so `--main` does not narrow it.

Exit codes: `0` success (including "no matches"), `1` invalid selector or I/O
error, `2` bad command-line usage.

## Extract structured data

`-e NAME=SPEC` adds `NAME` to a JSON object. `SPEC` is:

| SPEC | value |
|---|---|
| `SELECTOR` | readable text of the first match, or `null` |
| `SELECTOR@ATTR` | `ATTR` of the first match that has it, or `null` (URL attributes resolved with `-b`/`-B`) |
| `SELECTOR[]` / `SELECTOR@ATTR[]` | a JSON array over all matches (`[]` when none) |
| `@ATTR`, empty | the scope element itself (see below) |

```sh
$ domtree -e title=h1 -e price=.price -e 'tags=.tag[]' -e img=img@src < product.html
{"title":"Desk lamp","price":"$49","tags":["home","lighting"],"img":"/lamp.jpg"}
```

With a positional `SELECTOR`, it is the **scope**: you get one object per match,
with every `-e` evaluated inside it — a JSON array, or one object per line with
`--jsonl`.

```sh
$ domtree '.card' -e name=h2 -e url=a@href -e id=@data-id --jsonl < list.html
{"name":"Ann","url":"/u/ann","id":"1"}
{"name":"Bob","url":null,"id":"2"}
```

Only the **last** `@NAME` and a trailing `[]` are special, so selectors with an
`@` inside an attribute value work as written (`a[href^="mailto:me@x.org"]@href`).
Write `\@` for a literal `@` before a name-like suffix (`.w-1\@sm`) — it is also
the CSS escape, so the selector engine reads it as `@`. `-w` skips blank values;
`-1` keeps only the first scope.

`--links` prints every `<a href>`/`<area href>` (except `javascript:`) with its
readable text — the visible text, else `aria-label`, an image's `alt`, or
`title`. A selector keeps only the links inside its matches (`domtree --links
nav`). URLs are resolved like everything else: as written by default, against
`-b URL`, or against the page's `<base href>` with `-B`.

```sh
$ curl -sL https://www.rust-lang.org | domtree -b https://www.rust-lang.org --links nav
https://www.rust-lang.org/	Rust Logo
https://www.rust-lang.org/tools/install	Install
https://www.rust-lang.org/learn	Learn
…
$ curl -sL https://www.rust-lang.org | domtree --links --jsonl header
{"url":"/learn/get-started","text":"Get Started"}
{"url":"https://blog.rust-lang.org/releases/latest","text":"Version 1.99.1"}
```

`--meta` prints the page's metadata as one object: `title`, `description`,
`canonical`, `lang`, `site_name`, `image`, `author`, `published`, `keywords`,
`og` and `twitter` (arrays of `[property, value]` pairs, repeats kept),
`json_ld` (raw blocks) and `feeds`. Missing values are `null` or `[]`; with
`-b`/`-B`, `canonical`, `image` and `feeds` are made absolute.

## Encodings

Input is decoded the way a browser would, minus HTTP headers: a byte-order mark
(UTF-8, UTF-16LE/BE) wins; then `--encoding`; then a `<meta charset>` (or
`<meta http-equiv=content-type>`) in the first 1024 bytes; then UTF-8 if the
bytes are valid UTF-8 (a character cut off at the very end still counts);
otherwise Windows-1252. `iso-8859-1`, `latin1` and `ascii` mean Windows-1252,
as in browsers. Other encodings are not supported: an unknown `--encoding`
label prints a warning and decodes as UTF-8, with invalid bytes as U+FFFD (a
page declaring an unsupported charset is decoded as UTF-8 if it is valid UTF-8,
else as Windows-1252 with a warning).

## Lint

`domtree lint [FILE]...` checks HTML (stdin when no file is given, or for `-`)
and prints one line per finding, in document order — the format editors and CI
problem matchers understand:

```console
$ domtree lint post.html
post.html:96:34: error: <a> link has no text or accessible name [empty-link]
post.html:290:13: warning: heading level skipped: <h4> after <h2> [heading-order]
$ domtree lint tests/pages/messy.html
tests/pages/messy.html:5:1: note: <p> implicitly closed by <ul> [implicitly-closed]
tests/pages/messy.html:7:44: warning: missing </b> before </i> (<b> opened at 7:37) [mismatched-end-tag]
tests/pages/messy.html:7:54: warning: stray end tag with no matching open element: </b> [stray-end-tag]
```

| rule | level | finds |
|---|---|---|
| `img-alt` | error | `<img>` without `alt` (`alt=""` marks a decorative image; `aria-label`, `title`, `role=presentation` also count) |
| `empty-link` | error | `<a href>` / `<area href>` with no text, `aria-label`, `title` or image `alt` (hidden content does not count) |
| `empty-button` | error | `<button>` / `<input type=button>` with no text or accessible name |
| `duplicate-id` | error | an `id` already used — reported where it repeats, naming the first use |
| `heading-order` | warning | a skipped heading level (`<h4>` right after `<h2>`) |
| `unclosed-element` | warning | an element whose end tag HTML requires, closed by another tag or never closed |
| `mismatched-end-tag` | warning | an end tag that also closed such elements (`<b><i>x</b>`) |
| `stray-end-tag`, `unterminated-tag`, `unterminated-comment`, `unterminated-cdata`, `unexpected-eof`, `bogus-comment` | warning | other markup the parser recovered from |
| `implicitly-closed` | note | an omitted *optional* end tag (`</li>`, `</p>`, `</td>`, …) — valid HTML |
| `unknown-entity` | note | an `&` that starts no known character reference (also a bare `&` in a URL) |

Every parse error the parser recovers from is reported; the source positions of
the elements it closed decide whether it was valid HTML (a note) or a real
problem (a warning). Notes never fail a run, and `-q`/`--quiet` hides them.

`--format json` prints one JSON array of
`{"file", "line", "column", "level", "rule", "message"}` objects instead.
Columns count characters, and lines end at `\n`, `\r\n` or `\r`.
`--encoding` works as above.

Exit codes: `0` no errors or warnings, `1` errors or warnings found, `2` bad
usage or an unreadable file (the other files are still checked). To query an
element named `<lint>`, write `domtree -- lint`.

## Selectors

Type, `*`, `.class`, `#id`, `[attr]`, `[attr=v]`, `^=`, `$=`, `*=`, `~=`, `|=`
(with the `i` flag), the ` `, `>`, `+`, `~` combinators, `:first-child`,
`:last-child`, `:nth-child(2n+1)`, `:nth-of-type()`, `:not()`, `:is()`, `:has()`,
`:empty`, `:root`, and the scraping-friendly `:contains("text")`.

## Coming from htmlq?

`-a`, `-t`, `-p`, `-w`, `-r`, `-b`, `-B`, `-f`, `-o` work the same way. Two
differences: `--text` prints *readable* text (paragraphs on their own lines,
`<script>`/`<style>` dropped) rather than raw text-node contents, and with no
selector `domtree` uses the whole document instead of `html` (so fragments work).

## License

MIT
