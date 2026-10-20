// dom-tree-rs playground. No framework, no build step: this module, the
// presets, and the wasm package that `wasm-pack build --target web` writes to
// ./pkg/.
import init, { Document } from "./pkg/dom_tree_wasm.js";
import { presets } from "./presets.js";

const $ = (id) => document.getElementById(id);

const ui = {
  selector: $("selector"),
  caret: $("selector-caret"),
  status: $("selector-status"),
  presets: $("presets"),
  editor: $("editor"),
  paint: $("editor-paint"),
  html: $("html"),
  time: $("stat-time"),
  nodes: $("stat-nodes"),
  repairs: $("repairs"),
  repairCount: $("stat-repairs"),
  repairList: $("repairs-list"),
  size: $("stat-size"),
  tabs: $("tabs"),
  matchCount: $("match-count"),
  panel: $("panel"),
  caption: $("panel-caption"),
  scope: $("scope"),
  treeTools: $("tree-tools"),
  showSpace: $("show-space"),
  copyOutput: $("copy-output"),
  copyLink: $("copy-link"),
  shareNote: $("share-note"),
  boot: $("boot"),
};

const TABS = ["matches", "tree", "text", "markdown", "json", "html"];
const MAX_PAINT = 150_000; // characters; bigger inputs are edited unhighlighted
const MAX_TREE_NODES = 5_000;
const MAX_MATCHES = 300;
const MAX_REPAIRS = 500;

const state = {
  html: "",
  selector: "",
  tab: "matches",
  scope: "matches",
  showSpace: false,
};

let doc = null; // the parsed Document, reused for every selector change
let tree = null; // doc.tree(), with depth-first indices added
let nodeCount = 0;
let repairs = [];
let matches = [];
let matched = new Set(); // nodeIndex of every match
let matchedSorted = []; // the same, ascending
let selectorError = null;
let parseMs = 0;
let selectMs = 0;
let copyText = "";

// --- boot --------------------------------------------------------------------

async function boot() {
  const wasmUrl = new URL("./pkg/dom_tree_wasm_bg.wasm", import.meta.url);
  const response = await fetch(wasmUrl);
  if (!response.ok) throw new Error(`${response.status} fetching ${wasmUrl.pathname}`);
  const bytes = await response.clone().arrayBuffer();
  await init({ module_or_path: response });
  reportSize(bytes);

  renderPresets();
  wireEvents();
  const restored = await readHash();
  if (!restored) applyPreset(presets[0]);
  ui.boot.hidden = true;
}

async function reportSize(bytes) {
  const raw = bytes.byteLength;
  let text = `${kb(raw)} raw`;
  let gz = null;
  try {
    if (typeof CompressionStream === "function") {
      const stream = new Blob([bytes]).stream().pipeThrough(new CompressionStream("gzip"));
      gz = (await new Response(stream).arrayBuffer()).byteLength;
    }
  } catch {
    gz = null;
  }
  if (gz) text = kb(gz);
  ui.size.textContent = text;
  ui.size.parentElement.title = `${raw.toLocaleString()} bytes uncompressed${gz ? `, ${gz.toLocaleString()} gzipped` : ""}`;
  if (!gz) ui.size.nextElementSibling.textContent = "wasm";
  for (const el of document.querySelectorAll("[data-wasm-size]")) el.textContent = gz ? `${kb(gz)} (gzipped)` : kb(raw);
}

const kb = (n) => `${(n / 1024).toFixed(1)} KB`;

// --- parsing -----------------------------------------------------------------

function parseNow() {
  doc?.free();
  // Average over several runs so tiny inputs get a meaningful number despite
  // the browser's coarse timer.
  let runs = 0;
  const start = performance.now();
  let elapsed = 0;
  do {
    new Document(state.html).free();
    runs++;
    elapsed = performance.now() - start;
  } while (elapsed < 6 && runs < 500);
  parseMs = elapsed / runs;

  doc = new Document(state.html);
  tree = doc.tree();
  nodeCount = indexTree(tree);
  repairs = doc.errors();
  selectNow();
  renderStats();
  paintMarks();
}

/** Give every node its depth-first index (`i`) and subtree size (`n`), the
 *  order `Match.nodeIndex` refers to. Iterative: documents can be very deep. */
function indexTree(root) {
  let i = 0;
  const stack = [];
  for (let k = root.children.length - 1; k >= 0; k--) stack.push(root.children[k]);
  const order = [];
  while (stack.length) {
    const node = stack.pop();
    node.i = i++;
    order.push(node);
    if (node.children) for (let k = node.children.length - 1; k >= 0; k--) stack.push(node.children[k]);
  }
  for (let k = order.length - 1; k >= 0; k--) {
    const node = order[k];
    node.n = 1 + (node.children ? node.children.reduce((sum, c) => sum + c.n, 0) : 0);
  }
  return i;
}

function selectNow() {
  selectorError = null;
  matches = [];
  if (state.selector.trim() && doc) {
    try {
      matches = doc.select(state.selector);
      // Repeat (cheaply) to get a stable timing for fast selectors.
      let runs = 0;
      const start = performance.now();
      do {
        doc.select(state.selector);
        runs++;
      } while (performance.now() - start < 4 && runs < 200);
      selectMs = (performance.now() - start) / runs;
    } catch (e) {
      if (e?.name !== "SelectorError") throw e;
      selectorError = { message: e.message, position: e.position };
    }
  }
  matched = new Set(matches.map((m) => m.nodeIndex));
  matchedSorted = [...matched].sort((a, b) => a - b);
  renderQuery();
  renderPanel();
  markPresets();
  saveHash();
}

// --- rendering: query line ---------------------------------------------------

function renderQuery() {
  const input = ui.selector;
  ui.status.classList.toggle("error", Boolean(selectorError));
  input.setAttribute("aria-invalid", selectorError ? "true" : "false");
  if (selectorError) {
    const { message, position } = selectorError;
    const reason = message.replace(/ at byte \d+$/, "");
    ui.status.textContent = `${capitalize(reason)}, at character ${position + 1}.`;
    placeCaret(position);
  } else {
    ui.caret.hidden = true;
    if (!state.selector.trim()) {
      ui.status.textContent = "Matches show up in the Matches and Tree tabs as you type.";
    } else {
      const n = matches.length;
      ui.status.textContent = n
        ? `${n.toLocaleString()} ${n === 1 ? "element matches" : "elements match"}, found in ${ms(selectMs)}.`
        : "Valid selector, but nothing in the HTML matches it.";
    }
  }
  ui.matchCount.textContent = state.selector.trim() && !selectorError ? String(matches.length) : "";
}

function placeCaret(position) {
  const style = getComputedStyle(ui.selector);
  const left = parseFloat(style.paddingLeft) + parseFloat(style.borderLeftWidth) - ui.selector.scrollLeft;
  ui.caret.style.left = `calc(${left}px + ${position}ch)`;
  ui.caret.hidden = false;
}

// --- rendering: stats ----------------------------------------------------------

function renderStats() {
  ui.time.textContent = ms(parseMs);
  ui.nodes.textContent = nodeCount.toLocaleString();
  ui.repairCount.textContent = repairs.length.toLocaleString();

  const lines = lineStarts(state.html);
  const items = repairs.slice(0, MAX_REPAIRS).map((r, k) => {
    const { line, column } = lineColumn(lines, r.position);
    const li = document.createElement("li");
    const button = document.createElement("button");
    button.type = "button";
    button.dataset.repair = String(k);
    const where = document.createElement("span");
    where.className = "where";
    where.textContent = `Line ${line}:${column}`;
    const what = document.createElement("span");
    what.textContent = capitalize(r.message);
    button.append(where, what);
    li.append(button);
    return li;
  });
  if (!repairs.length) {
    const li = document.createElement("li");
    li.className = "repairs-clean";
    li.textContent = "None: this HTML is well-formed.";
    items.push(li);
  } else if (repairs.length > MAX_REPAIRS) {
    const li = document.createElement("li");
    li.textContent = `And ${(repairs.length - MAX_REPAIRS).toLocaleString()} more.`;
    items.push(li);
  }
  ui.repairList.replaceChildren(...items);
}

function jumpToRepair(k) {
  const r = repairs[k];
  if (!r) return;
  const text = state.html;
  const end = Math.min(text.length, r.position + Math.max(1, (text.codePointAt(r.position) ?? 0) > 0xffff ? 2 : 1));
  ui.editor.scrollIntoView({ block: "nearest" });
  ui.html.focus({ preventScroll: true });
  ui.html.setSelectionRange(r.position, end);
  const lines = lineStarts(text);
  const { line } = lineColumn(lines, r.position);
  const lineHeight = parseFloat(getComputedStyle(ui.html).lineHeight) || 20;
  ui.html.scrollTop = Math.max(0, (line - 4) * lineHeight);
  syncScroll();
  for (const m of ui.paint.querySelectorAll("mark.current")) m.classList.remove("current");
  ui.paint.querySelector(`mark[data-repair="${k}"]`)?.classList.add("current");
}

// --- rendering: the editor's painted layer --------------------------------------

function paintText() {
  const text = state.html;
  const plain = text.length > MAX_PAINT;
  ui.editor.classList.toggle("plain", plain);
  if (plain) {
    ui.paint.textContent = "";
    return;
  }
  ui.paint.innerHTML = highlight(text, markedPositions()) + (text.endsWith("\n") || !text ? " " : "");
  syncScroll();
}

const paintMarks = () => paintText();

/** Repair positions keyed for the painter: index -> repair number. */
function markedPositions() {
  const map = new Map();
  repairs.slice(0, MAX_REPAIRS).forEach((r, k) => {
    if (!map.has(r.position)) map.set(r.position, k);
  });
  return map;
}

function syncScroll() {
  ui.paint.scrollTop = ui.html.scrollTop;
  ui.paint.scrollLeft = ui.html.scrollLeft;
}

// --- HTML syntax highlighting (escape-safe, linear-time) -------------------------

const ESC = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" };
const esc = (s) => s.replace(/[&<>"]/g, (c) => ESC[c]);

const TOKEN =
  /<!--[\s\S]*?(?:-->|$)|<![^>]*>?|<\/?[A-Za-z][^\s/>]*(?:[^>"']|"[^"]*"?|'[^']*'?)*>?|&#?[A-Za-z0-9]+;/g;
const ATTR = /([^\s=/>"']+)(\s*=\s*)?("[^"]*"?|'[^']*'?|[^\s>]+)?/g;

/**
 * Highlight `src` as HTML. `marks` (index -> repair number) wraps the
 * character at each index in a <mark>, for the editor's repair markers.
 */
function highlight(src, marks = new Map()) {
  const positions = [...marks.keys()].sort((a, b) => a - b);
  let p = 0; // next mark to place
  let out = "";

  // Emit src[from, to) escaped, wrapped in `cls`, splitting at marks.
  const emit = (from, to, cls) => {
    let at = from;
    while (p < positions.length && positions[p] < from) p++;
    while (at < to) {
      const next = p < positions.length && positions[p] < to ? positions[p] : to;
      if (next > at) out += wrap(src.slice(at, next), cls);
      if (next < to) {
        const width = (src.codePointAt(next) ?? 0) > 0xffff ? 2 : 1;
        const piece = src.slice(next, next + width);
        out += `<mark class="fix" data-repair="${marks.get(next)}">${wrap(piece, cls)}</mark>`;
        at = next + width;
        p++;
      } else {
        at = to;
      }
    }
  };
  const wrap = (s, cls) => (cls ? `<span class="${cls}">${esc(s)}</span>` : esc(s));

  let last = 0;
  TOKEN.lastIndex = 0;
  for (let m; (m = TOKEN.exec(src)); ) {
    const start = m.index;
    const tok = m[0];
    if (!tok) {
      TOKEN.lastIndex++;
      continue;
    }
    emit(last, start, "");
    if (tok.startsWith("<!")) {
      emit(start, start + tok.length, "hl-com");
    } else if (tok[0] === "&") {
      emit(start, start + tok.length, "hl-ent");
    } else {
      const close = tok[1] === "/";
      const nameStart = start + (close ? 2 : 1);
      const name = /^[^\s/>]*/.exec(tok.slice(nameStart - start))[0];
      const nameEnd = nameStart + name.length;
      const end = start + tok.length;
      const bodyEnd = tok.endsWith(">") ? end - 1 : end;
      emit(start, nameStart, "hl-punct");
      emit(nameStart, nameEnd, "hl-tag");
      const body = src.slice(nameEnd, bodyEnd);
      let b = 0;
      ATTR.lastIndex = 0;
      for (let a; (a = ATTR.exec(body)); ) {
        if (!a[0]) {
          ATTR.lastIndex++;
          continue;
        }
        const s = nameEnd + a.index;
        emit(nameEnd + b, s, "");
        emit(s, s + a[1].length, "hl-attr");
        let q = s + a[1].length;
        if (a[2]) {
          emit(q, q + a[2].length, "hl-punct");
          q += a[2].length;
        }
        if (a[3]) {
          emit(q, q + a[3].length, "hl-val");
          q += a[3].length;
        }
        b = q - nameEnd;
      }
      emit(nameEnd + b, bodyEnd, "");
      emit(bodyEnd, end, "hl-punct");
    }
    last = start + tok.length;
  }
  emit(last, src.length, "");
  return out;
}

// --- rendering: output panel --------------------------------------------------------

function renderPanel(focusIndex) {
  for (const tab of ui.tabs.querySelectorAll("[role=tab]")) {
    const on = tab.dataset.tab === state.tab;
    tab.setAttribute("aria-selected", String(on));
    tab.tabIndex = on ? 0 : -1;
  }
  ui.panel.setAttribute("aria-labelledby", `tab-${state.tab}`);
  ui.scope.hidden = !(state.tab === "text" || state.tab === "markdown");
  ui.treeTools.hidden = state.tab !== "tree";
  if (!doc) return;
  const render = { matches: renderMatches, tree: renderTree, text: renderText, markdown: renderMarkdown, json: renderJson, html: renderHtml };
  render[state.tab](focusIndex);
}

function setCaption(...parts) {
  ui.caption.replaceChildren(...parts.map((p) => (typeof p === "string" ? document.createTextNode(p) : p)));
}

function codeEl(text) {
  const c = document.createElement("code");
  c.textContent = text;
  return c;
}

function preEl(text, html = false) {
  const pre = document.createElement("pre");
  pre.className = "code";
  if (html && text.length <= MAX_PAINT) pre.innerHTML = highlight(text);
  else pre.textContent = text;
  return pre;
}

function renderMatches() {
  const sel = state.selector.trim();
  if (!sel || selectorError) {
    setCaption(sel ? "Fix the selector to see matches." : "Elements matching your selector, as outer HTML.");
    copyText = "";
    ui.panel.replaceChildren(sel ? emptyState("This selector can't be parsed.", ui.status.textContent) : suggestionsState());
    return;
  }
  if (!matches.length) {
    setCaption("No matches.");
    copyText = "";
    ui.panel.replaceChildren(emptyState("Nothing matches this selector.", "Check the Tree tab to see how the HTML was parsed.", true));
    return;
  }
  setCaption(`${matches.length.toLocaleString()} ${matches.length === 1 ? "match" : "matches"} for `, codeEl(state.selector), `, in document order.`);
  copyText = matches.map((m) => m.html).join("\n");
  const list = document.createElement("ol");
  list.className = "matches";
  for (const m of matches.slice(0, MAX_MATCHES)) {
    const li = document.createElement("li");
    li.className = "match";
    const head = document.createElement("div");
    head.className = "match-head";
    const name = document.createElement("span");
    name.className = "match-name";
    name.textContent = describe(m.tag, m.attrs);
    const show = document.createElement("button");
    show.type = "button";
    show.className = "quiet";
    show.dataset.node = String(m.nodeIndex);
    show.textContent = "Show in tree";
    head.append(name, show);
    li.append(head, preEl(m.html, true));
    if (m.text) {
      const t = document.createElement("p");
      t.className = "match-text";
      t.textContent = clip(m.text.replace(/\s+/g, " "), 240);
      li.append(t);
    }
    list.append(li);
  }
  const parts = [list];
  if (matches.length > MAX_MATCHES) {
    const more = document.createElement("p");
    more.className = "more";
    more.textContent = `Showing the first ${MAX_MATCHES} of ${matches.length.toLocaleString()} matches. Copy gets them all.`;
    parts.push(more);
  }
  ui.panel.replaceChildren(...parts);
}

function suggestionsState() {
  const box = emptyState("Type a CSS selector above.", "Matching elements show up here and light up in the Tree tab. Some to try:");
  const row = document.createElement("div");
  row.className = "suggestions";
  for (const s of ["a[href]", "li:nth-child(odd)", ":is(h1, h2, h3)", "p:has(> img)", "td:contains(\"$\")", "img:not([alt])"]) {
    const b = document.createElement("button");
    b.type = "button";
    b.dataset.suggest = s;
    b.textContent = s;
    row.append(b);
  }
  box.append(row);
  return box;
}

function emptyState(title, body, treeLink = false) {
  const box = document.createElement("div");
  box.className = "empty";
  const strong = document.createElement("strong");
  strong.textContent = title;
  const p = document.createElement("p");
  p.style.margin = "0";
  p.textContent = body;
  box.append(strong, p);
  if (treeLink) {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "quiet";
    b.dataset.goto = "tree";
    b.textContent = "Open the Tree tab";
    b.style.marginTop = "0.8rem";
    box.append(b);
  }
  return box;
}

function renderTree(focusIndex) {
  const total = nodeCount;
  const openAll = total <= 600;
  const isSpace = (node) => node.type === "text" && !/\S/.test(node.value);
  // Is any match strictly inside `node`'s subtree [i + 1, i + n)?
  const hasMatchInside = (node) => {
    let lo = 0;
    let hi = matchedSorted.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (matchedSorted[mid] <= node.i) lo = mid + 1;
      else hi = mid;
    }
    return lo < matchedSorted.length && matchedSorted[lo] < node.i + node.n;
  };

  let rendered = 0;
  let hiddenSpace = 0;
  const lines = [];
  const rootList = document.createElement("ul");
  // Iterative build: [node, parent <ul>, depth].
  const stack = [];
  for (let k = tree.children.length - 1; k >= 0; k--) stack.push([tree.children[k], rootList, 0]);
  while (stack.length && rendered < MAX_TREE_NODES) {
    const [node, parentList, depth] = stack.pop();
    if (!state.showSpace && isSpace(node)) {
      hiddenSpace++;
      continue;
    }
    rendered++;
    const li = document.createElement("li");
    li.dataset.i = String(node.i);
    if (matched.has(node.i)) li.classList.add("matched");
    const indent = "  ".repeat(depth);

    if (node.type === "element") {
      const label = describe(node.tag, node.attrs);
      lines.push(indent + label);
      const kids = node.children.filter((c) => state.showSpace || !isSpace(c));
      if (kids.length) {
        const details = document.createElement("details");
        details.open = openAll || depth < 3 || hasMatchInside(node);
        const summary = document.createElement("summary");
        summary.className = "row";
        summary.append(dot(), ...elementLabel(node));
        const folded = document.createElement("span");
        folded.className = "t-folded";
        folded.textContent = `${kids.length} ${kids.length === 1 ? "child" : "children"}`;
        summary.append(folded);
        const ul = document.createElement("ul");
        details.append(summary, ul);
        li.append(details);
        for (let k = node.children.length - 1; k >= 0; k--) stack.push([node.children[k], ul, depth + 1]);
      } else {
        const row = document.createElement("div");
        row.className = "row";
        row.tabIndex = -1;
        row.append(dot(), ...elementLabel(node));
        li.append(row);
      }
    } else {
      const row = document.createElement("div");
      row.className = "row";
      row.tabIndex = -1;
      const span = document.createElement("span");
      if (node.type === "text") {
        span.className = "t-text";
        const shown = isSpace(node) ? "(whitespace)" : `"${clip(node.value.replace(/\s+/g, " "), 160)}"`;
        span.textContent = shown;
        lines.push(indent + shown);
      } else {
        span.className = "t-other";
        span.textContent = node.type === "comment" ? `<!--${clip(node.value, 120)}-->` : `<!doctype ${node.value}>`;
        lines.push(indent + span.textContent);
      }
      row.append(dot(), span);
      li.append(row);
    }
    parentList.append(li);
  }

  const container = document.createElement("div");
  container.className = "tree";
  container.append(rootList);
  const parts = [container];
  if (rendered >= MAX_TREE_NODES && stack.length > 0) {
    const more = document.createElement("p");
    more.className = "more";
    more.textContent = `Showing the first ${MAX_TREE_NODES.toLocaleString()} nodes of ${total.toLocaleString()}.`;
    parts.push(more);
  }
  ui.panel.replaceChildren(...parts);
  copyText = lines.join("\n");

  const counts = [`${total.toLocaleString()} nodes`];
  if (matched.size) counts.push(`${matched.size.toLocaleString()} matched`);
  const truncated = rendered >= MAX_TREE_NODES && stack.length > 0;
  if (hiddenSpace && !state.showSpace) {
    counts.push(truncated ? "whitespace-only text hidden" : `${hiddenSpace.toLocaleString()} whitespace-only text nodes hidden`);
  }
  setCaption(counts.join(", ") + ".");

  if (focusIndex !== undefined) revealNode(focusIndex);
}

/** `div#main.card` as one unit, then any other attributes. */
function elementLabel(node) {
  const name = document.createElement("span");
  name.className = "t-name";
  const tag = document.createElement("span");
  tag.className = "t-tag";
  tag.textContent = node.tag;
  name.append(tag);
  const { id, class: cls, ...rest } = node.attrs;
  if (id !== undefined) {
    const s = document.createElement("span");
    s.className = "t-id";
    s.textContent = `#${id}`;
    name.append(s);
  }
  if (cls && classes(cls)) {
    const s = document.createElement("span");
    s.className = "t-class";
    s.textContent = classes(cls);
    name.append(s);
  }
  const parts = [name];
  const attrs = Object.entries(rest);
  if (attrs.length) {
    const s = document.createElement("span");
    s.className = "t-attrs";
    s.textContent = clip(attrs.map(([k, v]) => (v === "" ? k : `${k}="${v}"`)).join(" "), 120);
    parts.push(s);
  }
  return parts;
}

function dot() {
  const d = document.createElement("span");
  d.className = "dot";
  d.setAttribute("aria-hidden", "true");
  return d;
}

/** `div#main.card.wide` */
function describe(tag, attrs) {
  let s = tag;
  if (attrs.id !== undefined) s += `#${attrs.id}`;
  if (attrs.class) s += classes(attrs.class);
  return s;
}

const classes = (value) =>
  value
    .split(/\s+/)
    .filter(Boolean)
    .map((c) => `.${c}`)
    .join("");

function revealNode(index) {
  const li = ui.panel.querySelector(`li[data-i="${index}"]`);
  if (!li) return;
  for (let el = li.parentElement; el && el !== ui.panel; el = el.parentElement) {
    if (el.tagName === "DETAILS") el.open = true;
  }
  const row = li.querySelector(":scope > .row, :scope > details > summary");
  row?.scrollIntoView({ block: "center", behavior: "instant" });
  row?.focus({ preventScroll: true });
  li.classList.remove("flash");
  void li.offsetWidth;
  li.classList.add("flash");
}

function scopedSelector() {
  const usable = state.selector.trim() && !selectorError;
  return state.scope === "matches" && usable ? state.selector : undefined;
}

function scopeCaption(what) {
  const sel = scopedSelector();
  if (sel !== undefined) {
    const which = matches.length === 1 ? "the match" : `the ${matches.length.toLocaleString()} matches`;
    return setCaption(`${what} of ${which} for `, codeEl(sel), ".");
  }
  const why = state.scope === "matches" ? " (no valid selector)" : "";
  setCaption(`${what} of the whole document${why}.`);
}

function renderText() {
  copyText = doc.toText(scopedSelector());
  scopeCaption("Readable text");
  ui.panel.replaceChildren(copyText ? preEl(copyText) : emptyState("No text.", "Scripts, styles and the head are left out."));
}

function renderMarkdown() {
  copyText = doc.toMarkdown(scopedSelector());
  scopeCaption("Markdown (CommonMark with GFM tables)");
  ui.panel.replaceChildren(copyText ? preEl(copyText) : emptyState("No Markdown.", "Nothing visible to convert."));
}

function renderJson() {
  copyText = JSON.stringify(doc.tree(), null, 2);
  setCaption("The tree as JSON: what ", codeEl("parse(html)"), " returns.");
  ui.panel.replaceChildren(preEl(copyText));
}

function renderHtml() {
  copyText = doc.toHtml();
  setCaption("Re-serialized: every element closed and properly nested, attributes quoted.");
  ui.panel.replaceChildren(preEl(copyText, true));
}

// --- presets -------------------------------------------------------------------------

function renderPresets() {
  ui.presets.replaceChildren(
    ...presets.map((p) => {
      const b = document.createElement("button");
      b.type = "button";
      b.dataset.preset = p.id;
      b.textContent = p.label;
      b.setAttribute("aria-pressed", "false");
      return b;
    }),
  );
}

function markPresets() {
  for (const b of ui.presets.querySelectorAll("button")) {
    const p = presets.find((x) => x.id === b.dataset.preset);
    b.setAttribute("aria-pressed", String(p.html === state.html));
  }
}

function applyPreset(p) {
  state.html = p.html;
  state.selector = p.selector;
  state.tab = p.tab;
  ui.html.value = p.html;
  ui.selector.value = p.selector;
  ui.html.scrollTop = 0;
  parseNow();
}

// --- shareable URL ---------------------------------------------------------------------

const b64url = {
  encode(bytes) {
    let s = "";
    for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
    return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
  },
  decode(text) {
    const s = atob(text.replace(/-/g, "+").replace(/_/g, "/"));
    return Uint8Array.from(s, (c) => c.charCodeAt(0));
  },
};

async function pipe(bytes, transform) {
  const stream = new Blob([bytes]).stream().pipeThrough(transform);
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

async function buildHash() {
  const params = new URLSearchParams();
  const preset = presets.find((p) => p.html === state.html);
  if (preset) {
    params.set("p", preset.id);
  } else {
    const utf8 = new TextEncoder().encode(state.html);
    if (typeof CompressionStream === "function") {
      params.set("z", b64url.encode(await pipe(utf8, new CompressionStream("deflate-raw"))));
    } else {
      params.set("r", b64url.encode(utf8));
    }
  }
  params.set("s", state.selector);
  params.set("t", state.tab);
  if (state.scope !== "matches") params.set("scope", state.scope);
  return `#${params.toString()}`;
}

let hashTimer = 0;
function saveHash() {
  clearTimeout(hashTimer);
  hashTimer = setTimeout(async () => {
    try {
      history.replaceState(null, "", await buildHash());
    } catch {
      /* the URL is a convenience; never break the page over it */
    }
  }, 400);
}

async function readHash() {
  const params = new URLSearchParams(location.hash.slice(1));
  let html;
  try {
    if (params.has("p")) html = presets.find((p) => p.id === params.get("p"))?.html;
    else if (params.has("z")) html = new TextDecoder().decode(await pipe(b64url.decode(params.get("z")), new DecompressionStream("deflate-raw")));
    else if (params.has("r")) html = new TextDecoder().decode(b64url.decode(params.get("r")));
  } catch {
    html = undefined;
  }
  if (html === undefined) return false;
  state.html = html;
  state.selector = params.get("s") ?? "";
  state.tab = TABS.includes(params.get("t")) ? params.get("t") : "matches";
  state.scope = params.get("scope") === "document" ? "document" : "matches";
  ui.html.value = state.html;
  ui.selector.value = state.selector;
  ui.scope.querySelector(`input[value="${state.scope}"]`).checked = true;
  parseNow();
  return true;
}

// --- clipboard ---------------------------------------------------------------------------

async function copy(text, button, done = "Copied") {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const area = document.createElement("textarea");
    area.value = text;
    document.body.append(area);
    area.select();
    document.execCommand("copy");
    area.remove();
  }
  if (button) {
    const label = button.dataset.label ?? button.textContent;
    button.dataset.label = label;
    button.textContent = done;
    setTimeout(() => (button.textContent = label), 1500);
  }
}

// --- events ------------------------------------------------------------------------------

function debounce(fn, wait) {
  let t = 0;
  return (...args) => {
    clearTimeout(t);
    t = setTimeout(() => fn(...args), wait);
  };
}

function wireEvents() {
  const reparse = debounce(parseNow, 140);
  ui.html.addEventListener("input", () => {
    state.html = ui.html.value;
    repairs = []; // stale until the reparse lands
    paintText();
    reparse();
  });
  ui.html.addEventListener("scroll", syncScroll);
  // Keep the painted layer's wrapping in step when the editor is resized.
  new ResizeObserver(syncScroll).observe(ui.html);

  const reselect = debounce(selectNow, 60);
  ui.selector.addEventListener("input", () => {
    state.selector = ui.selector.value;
    reselect();
  });
  ui.selector.addEventListener("scroll", () => selectorError && placeCaret(selectorError.position));

  ui.presets.addEventListener("click", (e) => {
    const b = e.target.closest("button[data-preset]");
    if (b) applyPreset(presets.find((p) => p.id === b.dataset.preset));
  });

  ui.repairList.addEventListener("click", (e) => {
    const b = e.target.closest("button[data-repair]");
    if (b) jumpToRepair(Number(b.dataset.repair));
  });

  ui.tabs.addEventListener("click", (e) => {
    const tab = e.target.closest("[role=tab]");
    if (tab) selectTab(tab.dataset.tab);
  });
  ui.tabs.addEventListener("keydown", (e) => {
    const k = TABS.indexOf(state.tab);
    const next =
      e.key === "ArrowRight" ? (k + 1) % TABS.length
      : e.key === "ArrowLeft" ? (k - 1 + TABS.length) % TABS.length
      : e.key === "Home" ? 0
      : e.key === "End" ? TABS.length - 1
      : -1;
    if (next < 0) return;
    e.preventDefault();
    selectTab(TABS[next]);
    $(`tab-${TABS[next]}`).focus();
  });

  ui.panel.addEventListener("click", (e) => {
    const show = e.target.closest("button[data-node]");
    if (show) {
      state.tab = "tree";
      saveHash();
      renderPanel(Number(show.dataset.node));
      return;
    }
    const suggest = e.target.closest("button[data-suggest]");
    if (suggest) {
      ui.selector.value = state.selector = suggest.dataset.suggest;
      selectNow();
      ui.selector.focus();
      return;
    }
    const goto = e.target.closest("button[data-goto]");
    if (goto) selectTab(goto.dataset.goto);
  });

  ui.scope.addEventListener("change", (e) => {
    state.scope = e.target.value;
    renderPanel();
    saveHash();
  });
  ui.showSpace.addEventListener("change", () => {
    state.showSpace = ui.showSpace.checked;
    renderPanel();
  });
  $("expand-all").addEventListener("click", () => {
    for (const d of ui.panel.querySelectorAll("details")) d.open = true;
  });
  $("collapse-all").addEventListener("click", () => {
    for (const d of ui.panel.querySelectorAll("details")) d.open = false;
  });

  ui.copyOutput.addEventListener("click", () => copy(copyText, ui.copyOutput));
  for (const b of document.querySelectorAll("[data-copy]")) {
    b.addEventListener("click", () => copy($(b.dataset.copy).textContent, b));
  }
  ui.copyLink.addEventListener("click", async () => {
    const hash = await buildHash();
    history.replaceState(null, "", hash);
    await copy(location.href, ui.copyLink, "Link copied");
    ui.shareNote.textContent = `Copied a ${location.href.length.toLocaleString()}-character link with your HTML, selector and tab.`;
  });

  window.addEventListener("hashchange", () => readHash());
}

function selectTab(tab) {
  state.tab = tab;
  renderPanel();
  saveHash();
}

// --- small helpers -----------------------------------------------------------------------

function ms(t) {
  return `${t < 10 ? t.toPrecision(2) : t.toFixed(0)} ms`;
}

const capitalize = (s) => s.charAt(0).toUpperCase() + s.slice(1);
const clip = (s, n) => (s.length > n ? `${s.slice(0, n - 1)}…` : s);

function lineStarts(text) {
  const starts = [0];
  for (let i = text.indexOf("\n"); i >= 0; i = text.indexOf("\n", i + 1)) starts.push(i + 1);
  return starts;
}

function lineColumn(starts, index) {
  let lo = 0;
  let hi = starts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (starts[mid] <= index) lo = mid;
    else hi = mid - 1;
  }
  return { line: lo + 1, column: index - starts[lo] + 1 };
}

boot().catch((e) => {
  console.error(e);
  ui.boot.classList.add("failed");
  ui.boot.textContent =
    location.protocol === "file:"
      ? "The parser can't load from a file:// page. Serve this folder over HTTP, for example with: python3 -m http.server"
      : `Couldn't load the parser (${e.message}). Build it with: wasm-pack build --target web --out-dir ../playground/pkg`;
});
