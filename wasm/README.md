# domq for JavaScript

**Messy HTML in. Clean tree, text, Markdown, or JSON out.** A forgiving HTML
parser with CSS selectors for JavaScript: browsers, Node, Deno, Bun and edge
workers. It is the Rust crate [`domq`](https://github.com/iamsaquib8/domq)
compiled to one **~41 KB (gzipped)** WebAssembly module, with zero JS
dependencies.

**[Try it in the playground](https://iamsaquib8.github.io/domq/)**: paste
any HTML and query it live.

```js
import { select, toMarkdown } from "domq-wasm";

const html = `<ul id=links><li><a href="/a">One</a><li><a href="/b">Two</a></ul>`;

select(html, "#links a[href]").map((a) => a.attrs.href); // ["/a", "/b"]
toMarkdown(html); // "- [One](/a)\n- [Two](/b)"
```

- **Never throws on bad HTML.** Unclosed tags, mis-nesting and stray end tags
  are repaired, and `errors()` tells you where.
- **Real CSS selectors.** Combinators, attribute operators, `:nth-child()`,
  `:not()`, `:is()`, `:has()`, plus jQuery-style `:contains("text")`.
- **Content out.** Readable text (no scripts or styles), CommonMark/GFM Markdown
  for LLM prompts and RAG, a JSON-ready tree, or repaired HTML.
- **Small.** One `.wasm` (~41 KB gzipped) plus ~4 KB of generated glue JS.

It is not a spec-compliant WHATWG parser (no implicit `<html>`/`<body>`, no
adoption agency) and not a sanitizer. Use DOMPurify for untrusted HTML that you
render.

## Install

```sh
npm install domq-wasm
```

## Use it

The package ships one ES module build. How it loads the `.wasm` depends on the
runtime:

| Runtime | Setup | Status |
|---|---|---|
| Node.js 22+ (supported LTS lines) | nothing: `import { select } from "domq-wasm"` | tested in CI on the current LTS |
| Browsers, bundlers (Vite, webpack 5, …) | `await init()` once | tested (the playground uses this build) |
| Bun | nothing (uses the Node entry point) | expected to work, not tested |
| Deno | nothing with `npm:domq-wasm` (Node entry point) | expected to work, not tested |
| Cloudflare Workers | `initSync({ module })` with the imported `.wasm` | expected to work, not tested |

### Node.js

Import and call. The Node entry point loads the module synchronously at import
time, so there is no `init()` to await (calling it anyway is harmless).

```js
import { select, toText } from "domq-wasm";

const res = await fetch("https://news.ycombinator.com");
const html = await res.text();
for (const link of select(html, ".titleline > a")) {
  console.log(link.text, link.attrs.href);
}
```

From CommonJS, `require("domq-wasm")` works on Node versions that can
`require()` ES modules (20.19+ and 22.12+). On older versions, use
`const { select } = await import("domq-wasm")`.

### Browsers and bundlers

Call the default export once before anything else. It fetches
`domq_wasm_bg.wasm` from next to the JS file, which bundlers such as Vite
and webpack 5 handle as an asset.

```js
import init, { select, toMarkdown } from "domq-wasm";

await init();
const md = toMarkdown(document.documentElement.outerHTML, "article");
```

Without a bundler, import it from a CDN:

```html
<script type="module">
  import init, { toText } from "https://cdn.jsdelivr.net/npm/domq-wasm@0.4/domq_wasm.js";
  await init();
  console.log(toText("<p>Hello <b>world</b>"));
</script>
```

### Deno and Bun

```js
import { select } from "npm:domq-wasm@0.4"; // Deno; in Bun: "domq-wasm"
```

Both resolve the package's Node entry point, which reads the `.wasm` with
`node:fs`. Deno may ask for read permission (`--allow-read`).

### Cloudflare Workers

Wrangler imports `.wasm` files as compiled `WebAssembly.Module`s; hand it to
`initSync`:

```js
import { initSync, toMarkdown } from "domq-wasm";
import wasm from "domq-wasm/domq_wasm_bg.wasm";

initSync({ module: wasm });

export default {
  async fetch(request) {
    const page = await fetch("https://blog.rust-lang.org");
    return new Response(toMarkdown(await page.text(), "main"));
  },
};
```

## API

Every function takes the HTML as a string. Functions that take a `selector`
throw a `SelectorError` if it is invalid; nothing else throws.

| Function | Returns |
|---|---|
| `parse(html)` | `DomDocument`: the tree as plain objects |
| `parseJson(html)` | `string`: the same tree as JSON text |
| `select(html, selector)` | `Match[]`: every matching element, in document order |
| `selectFirst(html, selector)` | `Match \| undefined`: the first match |
| `toText(html, selector?)` | `string`: readable text of the document, or of every match separated by a blank line |
| `toMarkdown(html, selector?)` | `string`: Markdown of the document, or of every match separated by a blank line |
| `toHtml(html)` | `string`: the repaired HTML (every element closed and properly nested) |
| `errors(html)` | `ParseError[]`: what the parser had to repair, and where |
| `init(input?)`, `initSync({ module })` | load the `.wasm` (browsers and Workers only; see above) |

### Parse once, query many times

Each function above parses the HTML again. To run several queries on one
document, parse it once:

```js
import { Document } from "domq-wasm";

const doc = new Document(html); // or Document.parse(html)
const title = doc.selectFirst("h1")?.text;
const prices = doc.select("tr:has(td.in-stock) .price").map((m) => m.text);
const body = doc.toMarkdown("article");
doc.free(); // release its wasm memory now instead of waiting for the GC
```

`Document` has the same operations as methods: `select(selector)`,
`selectFirst(selector)`, `toText(selector?)`, `toMarkdown(selector?)`,
`toHtml()`, `tree()`, `errors()` and `free()`. `toJSON()` returns the same as
`tree()`, so `JSON.stringify(doc)` gives the JSON tree. Where explicit resource
management is available, `using doc = new Document(html)` frees it
automatically.

If you also use the DOM's `Document` type in the same file, rename the import:
`import { Document as HtmlDocument } from "domq-wasm"`.

### Types

The package includes TypeScript declarations; none of the API is typed `any`.

```ts
interface Match {
  tag: string;                   // lower-cased tag name
  attrs: Record<string, string>; // attributes; boolean attributes are ""
  text: string;                  // readable text, as toText() gives it
  html: string;                  // outer HTML, re-serialized
  nodeIndex: number;             // position in a depth-first walk of tree()
}

interface DomDocument {
  type: "document";
  errors: number;                // how many repairs (0 = clean input)
  children: DomNode[];
}

type DomNode =
  | { type: "element"; tag: string; attrs: Record<string, string>; children: DomNode[] }
  | { type: "text"; value: string }
  | { type: "comment"; value: string }
  | { type: "doctype"; value: string };

interface ParseError {
  position: number;              // index into the html string
  message: string;               // e.g. "element implicitly closed"
}

interface SelectorError extends Error {
  name: "SelectorError";
  position: number;              // index into the selector string
}
```

### Invalid selectors

```js
try {
  select(html, "a:hover");
} catch (e) {
  e.name;     // "SelectorError"
  e.message;  // "unsupported pseudo-class ':hover' at byte 1"
  e.position; // 1
}
```

Unsupported selectors are errors rather than silently matching nothing:
pseudo-elements, `:hover` and other interactive states, namespaces, and
`:has()` inside `:has()`. The full grammar is in the
[`Selector` docs](https://docs.rs/domq/latest/domq/struct.Selector.html).

## Size

| File | Raw | Gzipped |
|---|---:|---:|
| `domq_wasm_bg.wasm` | ~88 KB | ~41 KB |
| `domq_wasm.js` (glue) | ~22 KB | ~4 KB |

The module contains the parser, the selector engine, and the text, Markdown,
JSON and HTML serializers. CI fails if the gzipped `.wasm` grows past 45 KiB.

## Upgrading from 0.1 or 0.2

0.3 is a new API. `parse()` now returns objects, so drop the `JSON.parse`, or
call `parseJson()` for the string. `parseErrorCount(html)` is now
`errors(html).length`. `query(html, selector)` is now `select(html, selector)`,
which returns `Match` objects instead of a JSON string of subtrees.

## Building from source

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack

cd wasm
wasm-pack build --target web --release   # -> pkg/
node npm/finish.mjs                      # add the Node entry point + "exports"
wasm-pack test --node                    # Rust-side tests of the bindings
sh npm/smoke-test.sh                     # pack pkg/ and run tests/node.mjs against it
```

For the playground: `wasm-pack build --target web --release --no-pack --out-dir ../playground/pkg`,
then serve `playground/` over HTTP (for example `python3 -m http.server -d ../playground`).

## License

MIT
