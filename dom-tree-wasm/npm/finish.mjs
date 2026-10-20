// Turn wasm-pack's `--target web` output into the published npm package:
// add the Node.js entry point and an "exports" map that picks it for Node/Bun
// and the plain ES module (call `await init()`) everywhere else.
//
//   wasm-pack build --target web --release && node npm/finish.mjs [pkg-dir]
import { copyFileSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = new URL(".", import.meta.url);
const pkgDir = resolve(process.argv[2] ?? fileURLToPath(new URL("../pkg", here)));
const manifestPath = resolve(pkgDir, "package.json");
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));

copyFileSync(new URL("node.js", here), resolve(pkgDir, "node.js"));

const types = "./dom_tree_wasm.d.ts";
const ordered = {};
for (const [key, value] of Object.entries(manifest)) {
  ordered[key] = value;
  if (key === "types") {
    ordered.exports = {
      ".": { types, node: "./node.js", default: "./dom_tree_wasm.js" },
      "./dom_tree_wasm.js": { types, default: "./dom_tree_wasm.js" },
      "./dom_tree_wasm_bg.wasm": "./dom_tree_wasm_bg.wasm",
      "./package.json": "./package.json",
    };
  }
}
ordered.files = [...new Set([...(manifest.files ?? []), "node.js"])];
// node.js instantiates the module when imported: never tree-shake it away.
ordered.sideEffects = [...new Set([...(manifest.sideEffects ?? []), "./node.js"])];

writeFileSync(manifestPath, JSON.stringify(ordered, null, 2) + "\n");
console.log(`finished ${manifestPath}`);
