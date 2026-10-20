# domtree — query HTML from your shell

`domtree` reads HTML on stdin and prints what your CSS selector matches — as HTML,
attribute values, readable text, **Markdown**, or JSON. It's a tiny,
zero-dependency, never-panicking CLI built on
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

# A page as Markdown, without the chrome — ready for an LLM prompt
curl -sL https://blog.rust-lang.org | domtree -r 'nav, footer, script' --markdown body

# Count images that are missing alt text
domtree -c 'img:not([alt])' < page.html

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
| `-c, --count` | number of matches |
| `-p, --pretty` | indent HTML output |
| `-1, --first` | only the first match |
| `-w, --ignore-whitespace` | skip blank results |
| `-r, --remove-nodes <SEL>` | remove matching nodes first (repeatable) |
| `-b, --base <URL>` | make relative `href`/`src`/`srcset`/… URLs absolute |
| `-B, --detect-base` | use the document's `<base href>` |
| `-f, --filename <FILE>` / `-o, --output <FILE>` | read / write a file instead of stdin / stdout |

Several selector arguments are joined with spaces (`domtree ul '>' li`). With no
selector, the whole document is used.

Exit codes: `0` success (including "no matches"), `1` invalid selector or I/O
error, `2` bad command-line usage.

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
