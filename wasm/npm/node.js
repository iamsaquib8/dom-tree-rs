// Node.js entry point of the `domq` npm package (selected through the
// "node" export condition; Bun picks it too).
//
// It loads the .wasm from disk synchronously, so the API is ready as soon as
// the import resolves: no `await init()` needed. Calling `init()` anyway (as
// code shared with the browser does) is harmless: it returns immediately.
import { readFileSync } from "node:fs";
import init, { initSync } from "./domq_wasm.js";

initSync({ module: readFileSync(new URL("./domq_wasm_bg.wasm", import.meta.url)) });

export * from "./domq_wasm.js";
export default init;
