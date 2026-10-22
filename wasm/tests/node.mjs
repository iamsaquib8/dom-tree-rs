// Smoke test of the *built npm package* in Node.js, imported by its name the
// way users do (so the "exports" map and the Node entry point are exercised).
// `npm/smoke-test.sh` packs pkg/, installs the tarball and runs this file:
//
//   wasm-pack build --target web --release && node npm/finish.mjs && sh npm/smoke-test.sh
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

// 1. Plain ESM import: ready to use, no init call.
import init, {
  Document,
  errors,
  parse,
  parseJson,
  select,
  selectFirst,
  toHtml,
  toMarkdown,
  toText,
} from "domq-wasm";

const html = `<ul id=list><li class=item>One<li>Two &amp; <b>more</b></ul>`;

assert.equal(parse(html).children[0].tag, "ul");
assert.equal(parse(html).type, "document");
assert.equal(typeof parseJson(html), "string");
assert.deepEqual(JSON.parse(parseJson(html)), parse(html));

assert.deepEqual(select(html, "li").map((m) => m.text), ["One", "Two & more"]);
assert.deepEqual(selectFirst(html, ".item"), {
  tag: "li",
  attrs: { class: "item" },
  text: "One",
  html: `<li class="item">One</li>`,
  nodeIndex: 1,
});
assert.equal(selectFirst(html, "table"), undefined);
assert.equal(toText(html), "One\nTwo & more");
assert.equal(toText(html, "li"), "One\n\nTwo & more");
assert.equal(toMarkdown(html), "- One\n- Two & **more**");
assert.equal(toMarkdown(html, "b"), "**more**");
assert.equal(
  toHtml(html),
  `<ul id="list"><li class="item">One</li><li>Two &amp; <b>more</b></li></ul>`,
);
assert.deepEqual(errors("<p>ok</p>"), []);
assert.deepEqual(errors("<b><i>x</b></i>")[0], {
  position: 7,
  message: "end tag closed mis-nested elements",
});

// 2. Invalid selectors throw a real Error with a position.
assert.throws(() => select(html, "li:hover"), (e) => {
  assert.ok(e instanceof Error);
  assert.equal(e.name, "SelectorError");
  assert.equal(e.message, "unsupported pseudo-class ':hover' at byte 2");
  assert.equal(e.position, 2);
  return true;
});
assert.throws(() => toText(html, "ul >"), { name: "SelectorError", position: 4 });

// 3. The Document class: parse once, query many times.
const doc = new Document(html);
assert.equal(doc.select("li").length, 2);
assert.equal(doc.selectFirst("b").text, "more");
assert.equal(doc.toText("b"), "more");
assert.equal(doc.toMarkdown(), toMarkdown(html));
assert.equal(doc.toHtml(), toHtml(html));
assert.deepEqual(doc.tree(), parse(html));
assert.equal(JSON.stringify(doc), parseJson(html)); // toJSON()
assert.deepEqual(doc.errors(), errors(html));
doc.free();
const doc2 = Document.parse(html);
assert.equal(doc2.select("#list > li").length, 2);
doc2.free();

// 4. Browser-style code still works: init() is a no-op here.
await init();
assert.equal(toText("<p>after init</p>"), "after init");

// 5. CommonJS: require() works where Node can require ES modules (20.19+,
//    22.12+); import() works everywhere.
const require = createRequire(import.meta.url);
const viaImport = await import("domq-wasm");
assert.equal(viaImport.toText("<i>import()</i>"), "import()");
let viaRequire = "require() unsupported on this Node";
try {
  viaRequire = require("domq-wasm").toText("<i>require() works</i>");
} catch (e) {
  assert.equal(e.code, "ERR_REQUIRE_ESM");
}

// 6. The plain web build, initialised by hand (what node.js does for you).
//    The query string gives a fresh, not yet initialised module instance.
const webPath = require.resolve("domq-wasm/domq_wasm.js");
const web = await import(`${pathToFileURL(webPath)}?fresh`);
assert.throws(() => web.toText("<p>x</p>")); // not initialised yet
const wasmPath = require.resolve("domq-wasm/domq_wasm_bg.wasm");
await web.default({ module_or_path: readFileSync(wasmPath) });
assert.equal(web.toText("<p>web build</p>"), "web build");

console.log(`domq-wasm OK on Node ${process.version} (${viaRequire})`);
console.log(select(html, "li").map((m) => m.html).join("\n"));
