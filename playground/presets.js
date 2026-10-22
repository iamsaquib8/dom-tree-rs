// Example inputs. Each one sets the HTML, a selector worth trying on it, and
// the output tab that shows it off best.

const messy = `<h1>Bob&#146;s Hardware <small>est. 1999</h1>
<p>Welcome!  This week only:
<ul>
  <li>Hammers &#128;9.99
  <li>Nails (100 pack) <i>now <b>half</i> price</b>
  <li class=hot>Saws &trade; <span>brand
</ul>
<p>Stray closing tags are dropped</div></span>
<table border=1>
  <tr><th>Item<th>Price
  <tr><td>Drill<td>$49
  <tr><td>Glue<td>$3<td>(limit 2)
</table>
<p>Call <a href=tel:555-0100>555-0100</a><br>or visit <a href=map.html><img src=map.gif alt="Map"></a>
<!-- comments are kept; unknown entities are left as they are -->
<p>&copy; Bob &amp; Sons &bogus;`;

const article = `<!doctype html>
<html lang="en">
<head>
  <title>Why we parse HTML at the edge</title>
  <style>article { max-width: 40em }</style>
  <script>analytics.track("view")</script>
</head>
<body>
  <nav><a href="/">Home</a> <a href="/archive">Archive</a></nav>
  <article>
    <h1>Why we parse HTML at the <em>edge</em></h1>
    <p>Our link previews used to call a headless browser. Now a
       <strong>36 KB</strong> wasm module turns the page into Markdown before
       it reaches the model, and the p95 dropped from 900&nbsp;ms to 40&nbsp;ms.</p>
    <h2>What we keep</h2>
    <ul>
      <li>Headings, paragraphs and <a href="https://commonmark.org">CommonMark</a> lists
      <li>Code, with its language:
        <pre><code class="language-js">const md = toMarkdown(html, "article");</code></pre>
      <li>Tables, as GFM tables
    </ul>
    <table>
      <tr><th>Approach</th><th>p95</th></tr>
      <tr><td>Headless browser</td><td>900 ms</td></tr>
      <tr><td>domq</td><td>40 ms</td></tr>
    </table>
    <blockquote><p>Delete the browser, keep the content.</p></blockquote>
    <p>Images keep their alt text: <img src="/p95.png" alt="p95 latency chart"></p>
  </article>
  <footer>&copy; 2026 Example Corp. <a href="/privacy">Privacy</a></footer>
</body>
</html>`;

const table = `<table id="products">
  <thead>
    <tr><th>Product<th>Stock<th>Price
  </thead>
  <tbody>
    <tr><td>Mechanical keyboard<td class="in-stock">12 left<td class="price">$129
    <tr><td>Trackball<td class="sold-out">Sold out<td class="price">$89
    <tr><td>USB-C dock<td class="in-stock">3 left<td class="price">$199
    <tr><td>Monitor arm<td class="in-stock">40 left<td class="price">$59
    <tr><td>Webcam<td class="sold-out">Sold out<td class="price">$74
  </tbody>
</table>`;

const page = `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Gearbox &ndash; Tools for makers</title>
<link rel="stylesheet" href="/assets/site.css?v=42">
<script async src="https://www.googletagmanager.com/gtag/js?id=G-XXXX"></script>
<script>
  window.dataLayer = window.dataLayer || [];
  function gtag(){dataLayer.push(arguments)}
  if (window.innerWidth < 600 && document.cookie.indexOf("seen=1") < 0) { gtag("event", "mobile") }
</script>
<style>.hero{background:#222;color:#eee} .sr-only{position:absolute;width:1px}</style>
</head>
<body class="home page-template-default">
<a class="sr-only" href="#main">Skip to content</a>
<header class="site-header">
  <a href="/" class="logo"><img src="/logo.svg" alt="Gearbox"></a>
  <nav aria-label="Primary">
    <ul class="menu">
      <li class="menu-item current"><a href="/">Home</a>
      <li class="menu-item"><a href="/shop">Shop</a>
      <li class="menu-item"><a href="/blog">Blog</a>
      <li class="menu-item"><a href="https://github.com/gearbox" rel="noopener">GitHub</a>
    </ul>
  </nav>
</header>
<main id="main">
  <section class="hero">
    <h1>Tools for people who make things</h1>
    <p>Hand-picked gear, shipped from a real workshop.<br>Free returns for 60&nbsp;days.
    <p><a class="button" href="/shop?utm_source=home">Browse the shop</a>
  </section>
  <section class="featured">
    <h2>This week</h2>
    <div class="grid">
      <div class="card" data-sku="KB-01">
        <img src="/img/kb.jpg">
        <h3><a href="/p/keyboard">Mechanical keyboard</a></h3>
        <p class="price">$129 <s>$159</s>
      </div>
      <div class="card" data-sku="TB-07">
        <img src="/img/trackball.jpg" alt="Trackball">
        <h3><a href="/p/trackball">Trackball</a></h3>
        <p class="price">$89
      </div>
      <div class="card sold-out" data-sku="DK-02">
        <img src="/img/dock.jpg" alt="">
        <h3><a href="/p/dock">USB-C dock</a></h3>
        <p class="price">$199 <span class="badge">Sold out</span>
      </div>
    </div>
  </section>
  <!-- TODO: newsletter signup goes here -->
  <section class="posts">
    <h2>From the blog</h2>
    <article><h3><a href="/blog/oiling-chisels">How to oil a chisel</a></h3><p>Three drops, not ten.</article>
    <article><h3><a href="/blog/benches">Workbench heights, measured</a></h3><p>Spoiler: it depends on your elbows.</article>
  </section>
</main>
<footer class="site-footer">
  <p>&copy; 2026 Gearbox Ltd. &middot; <a href="/privacy">Privacy</a> &middot; <a href="https://twitter.com/gearbox">Twitter</a>
  <form action="/subscribe" method="post"><input type="email" name="email" placeholder="you@example.com" required><button>Subscribe</button></form>
</footer>
<script src="/assets/app.js" defer></script>
</body>
</html>`;

export const presets = [
  {
    id: "table",
    label: "Product table",
    html: table,
    selector: "tr:has(td.in-stock) .price",
    tab: "matches",
  },
  {
    id: "messy",
    label: "Messy fragment",
    html: messy,
    selector: "li:has(b)",
    tab: "tree",
  },
  {
    id: "article",
    label: "Blog article",
    html: article,
    selector: "article",
    tab: "markdown",
  },
  {
    id: "page",
    label: "Real page",
    html: page,
    selector: "img:not([alt]), a[href^='https://']",
    tab: "matches",
  },
];
