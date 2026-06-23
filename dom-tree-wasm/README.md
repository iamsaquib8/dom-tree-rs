# dom-tree-wasm

WebAssembly bindings for [`dom-tree-rs`](https://github.com/iamsaquib8/dom-tree-rs) —
parse messy HTML into a JSON tree from JavaScript, in the browser / on the edge /
in any wasm runtime, **without** pulling in a 100 KB+ parser.

## Build

```sh
# browser / Deno / edge (ES module)
wasm-pack build --target web --release

# Node / bundler
wasm-pack build --target nodejs --release
```

The default build has **zero JS dependencies** and a tiny `.wasm`.

## Use

```js
import init, { parse, parseErrorCount } from "./pkg/dom_tree_wasm.js";

await init();

const doc = JSON.parse(parse("<ul id=list><li class=item>Item 1<li>Item 2</ul>"));
// doc.type === "document"
// doc.children[0].tag === "ul"
console.log(parseErrorCount("<b><i>x</b>")); // > 0 (recovered)
```

With the `query` feature (`wasm-pack build --target web -- --features query`):

```js
import init, { query } from "./pkg/dom_tree_wasm.js";
await init();
const links = JSON.parse(query(html, "ul.links a[href]")); // array of subtrees
```

## Types

The JSON shape returned by `parse`/`query` is described in
[`dom-node.d.ts`](./dom-node.d.ts) (`DomDocument` / `DomNode`).

## Demo

After `wasm-pack build --target web`, serve this folder and open
[`examples/demo.html`](./examples/demo.html).
