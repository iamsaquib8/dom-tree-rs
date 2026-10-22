//! Main-content extraction ("reader mode"): [`Dom::main_content`].
//!
//! A small, deterministic cousin of Mozilla's Readability. Four linear passes
//! over the document (pre-order flags, post-order text statistics, paragraph
//! scoring, the semantic check) plus one walk up from the winner; no
//! recursion, no allocation beyond a few per-node arrays.

use alloc::vec;
use alloc::vec::Vec;

use super::{document_order, has_token};
use crate::node::NodeKind;
use crate::serialize::is_space;
use crate::tags::{self, Display};
use crate::tree::{Dom, NodeRef};

/// Paragraphs shorter than this (non-whitespace characters) are not scored.
const MIN_PARAGRAPH: usize = 25;

/// An `<article>`/`<main>` needs this much text (or half the page's).
const MIN_SEMANTIC: usize = 200;

/// A paragraph's score goes to its parent in full, its grandparent halved, and
/// three more ancestors with these divisors (as in Readability).
const DIVIDERS: [f64; 5] = [1.0, 2.0, 6.0, 9.0, 12.0];

/// Class/id words marking boilerplate. Matched against the words of `class`
/// and `id` (split at anything not ASCII alphanumeric, case-insensitive) as a
/// prefix, so `comment` also catches `comments` and `commentlist`.
const NEGATIVE: &[&str] = &[
    "nav",
    "menu",
    "header",
    "masthead",
    "footer",
    "sidebar",
    "aside",
    "widget",
    "comment",
    "disqus",
    "respond",
    "share",
    "sharing",
    "social",
    "related",
    "advert",
    "sponsor",
    "promo",
    "cookie",
    "consent",
    "gdpr",
    "banner",
    "breadcrumb",
    "newsletter",
    "subscribe",
    "popup",
    "modal",
    "pagination",
    "pager",
    "skip",
];

/// Boilerplate words matched only as whole words (`ad` must not catch
/// `address` or `admin`).
const NEGATIVE_EXACT: &[&str] = &["ad", "ads", "toc"];

/// Class/id words marking content (prefix match, like [`NEGATIVE`]).
const POSITIVE: &[&str] = &[
    "article", "content", "post", "entry", "main", "story", "body", "blog", "prose",
];

/// Content words matched only as whole words.
const POSITIVE_EXACT: &[&str] = &["text"];

/// Content words strong enough to keep an element with a boilerplate word
/// (`comment-body`, `sidebar-content`) from being dropped before scoring.
const KEEP: &[&str] = &["article", "content", "main", "body"];

/// ARIA roles of boilerplate landmarks.
const UNLIKELY_ROLES: &[&str] = &[
    "navigation",
    "complementary",
    "contentinfo",
    "banner",
    "menu",
    "menubar",
    "search",
    "dialog",
    "alertdialog",
];

// Per-node flags.
/// The node or an ancestor is not rendered.
const HIDDEN: u8 = 1;
/// The node or an ancestor is boilerplate.
const UNLIKELY: u8 = 2;
/// The node itself is boilerplate.
const SELF_UNLIKELY: u8 = 4;
/// Inside an `<article>`/`<main>` (where `<header>`/`<footer>` belong to it).
const IN_SECTION: u8 = 8;
/// A good `<article>` / `<main>` encloses (or is) this node.
const IN_GOOD_ARTICLE: u8 = 1;
const IN_GOOD_MAIN: u8 = 2;

/// Text statistics of one subtree, in non-whitespace characters of visible
/// text.
#[derive(Clone, Copy, Default)]
struct Stats {
    text: usize,
    /// Text inside links.
    links: usize,
    commas: usize,
    /// Text in the element's own (direct) text children.
    own: usize,
    /// Text inside boilerplate elements.
    unlikely: usize,
    /// Some descendant is paragraph-like.
    para_below: bool,
}

impl Dom {
    /// The element holding the page's main content — the article, post or
    /// documentation body without the navigation, sidebars, comments and
    /// footer around it. The "reader mode" step before
    /// `NodeRef::to_markdown` in an LLM or RAG pipeline.
    ///
    /// A small, deterministic heuristic in the spirit of Mozilla's
    /// Readability. Text is measured in non-whitespace characters of visible
    /// text (no `<script>`, `<style>`, `hidden`, `aria-hidden="true"` or
    /// `display:none`). *Boilerplate* means `<nav>`, `<aside>`, a
    /// `<header>`/`<footer>` that is not inside an `<article>`/`<main>`, an
    /// element with a landmark `role` such as `navigation`, `complementary`
    /// or `contentinfo`, or one whose `class`/`id` has a boilerplate word
    /// (`nav`, `menu`, `header`, `footer`, `sidebar`, `comment`, `share`,
    /// `social`, `related`, `ad`/`ads`/`advert`, `promo`, `cookie`,
    /// `banner`, `breadcrumb`, …) and no strong content word (`article`,
    /// `content`, `main`, `body`).
    ///
    /// 1. **Semantic markup first.** If exactly one outermost `<article>` has
    ///    enough text (200+ characters and at least a fifth of the page, or
    ///    half the page), at most half of it in links and at most a quarter in
    ///    boilerplate, and it is not itself inside boilerplate, it is the
    ///    answer. Otherwise the same test runs on `<main>` / `[role=main]`.
    /// 2. **Score paragraphs.** Each innermost paragraph-like element
    ///    (`<p>`, `<pre>`, `<td>`, `<li>`, `<dd>`, or a block with 25+
    ///    characters of its own text) with 25+ characters, at most half of
    ///    them links, and not inside boilerplate scores `1 + commas +
    ///    min(chars / 100, 3)`. The score goes to its parent in full, to its
    ///    grandparent halved, and to three more ancestors divided by 6, 9 and
    ///    12.
    /// 3. **Pick the best container.** Each scored ancestor starts from a
    ///    base by tag (`div`/`article`/`main` +5, `pre`/`td`/`blockquote` +3,
    ///    lists and forms −3, headings −5) plus ±25 each for content /
    ///    boilerplate words in its `class` and in its `id`, adds its paragraph
    ///    scores, and is scaled by `1 − link density`. The highest final score
    ///    wins; ties go to the earlier element. If no paragraph qualified,
    ///    step 2 is retried with boilerplate allowed.
    /// 4. **Widen to the whole article.** While the winner's parent adds no
    ///    boilerplate, at most as much text as the winner has, and mostly
    ///    non-link text (a title, a byline, the rest of a split body), the
    ///    parent becomes the winner — stopping below `<body>`.
    ///
    /// Returns `None` when no paragraph qualifies even with boilerplate allowed
    /// (an empty, links-only or garbage page), or when the best "container" is the
    /// document itself (paragraphs at the top level with no common parent
    /// element). Use the whole document then. The chosen element is returned
    /// as is: boilerplate *inside* it (a share bar in an article) is not
    /// removed.
    ///
    /// ```
    /// let dom = domq::parse(r#"<body>
    ///   <nav><a href="/">Home</a> <a href="/blog">Blog</a></nav>
    ///   <div class="post">
    ///     <h1>Arena trees</h1>
    ///     <div class="entry">
    ///       <p>An arena stores every node in one vector, and links them by index.</p>
    ///       <p>Traversal is cheap, removal is a flag, and nothing is reference-counted.</p>
    ///     </div>
    ///   </div>
    ///   <div id="comments"><p>Great post, thanks for writing it up in so much detail!</p></div>
    /// </body>"#);
    ///
    /// let main = dom.main_content().unwrap();
    /// assert_eq!(main.attr("class"), Some("post"));
    /// # #[cfg(feature = "markdown")]
    /// assert!(main.to_markdown().starts_with("# Arena trees"));
    /// assert!(!main.to_text().contains("Great post"));
    /// assert!(domq::parse("<nav><a href=/>Home</a></nav>").main_content().is_none());
    /// ```
    pub fn main_content(&self) -> Option<NodeRef<'_>> {
        let order: Vec<NodeRef<'_>> = document_order(self).collect();
        let size = self.len();
        let mut flags = vec![0u8; size];
        let mut stats = vec![Stats::default(); size];

        // Pass 1 (pre-order): inherited flags.
        for node in &order {
            let inherited = node
                .parent()
                .and_then(|p| flags.get(p.id().index()))
                .map_or(0, |f| f & (HIDDEN | UNLIKELY | IN_SECTION));
            let mut f = inherited;
            if let Some(tag) = node.tag_name() {
                if is_hidden(*node, tag) {
                    f |= HIDDEN;
                }
                if is_unlikely(*node, tag, inherited & IN_SECTION != 0) {
                    f |= UNLIKELY | SELF_UNLIKELY;
                }
                if is_main(*node, tag) || tag == "article" {
                    f |= IN_SECTION;
                }
            }
            if let Some(slot) = flags.get_mut(node.id().index()) {
                *slot = f;
            }
        }

        // Pass 2 (reverse pre-order, so children come before parents): text
        // statistics.
        let mut total = 0usize;
        for node in order.iter().rev() {
            let index = node.id().index();
            let f = flags.get(index).copied().unwrap_or(HIDDEN);
            if f & HIDDEN != 0 {
                continue;
            }
            let (s, paragraph) = match node.kind() {
                NodeKind::Text(t) => {
                    let (chars, commas) = measure(t);
                    let s = Stats {
                        text: chars,
                        commas,
                        ..Stats::default()
                    };
                    (s, false)
                }
                NodeKind::Element { tag } => {
                    let Some(s) = stats.get_mut(index) else {
                        continue;
                    };
                    if tag == "a" {
                        s.links = s.text;
                    }
                    (*s, paragraph_like(tag, s.own))
                }
                NodeKind::Comment(_) | NodeKind::Doctype(_) => continue,
            };
            let Some(parent) = node.parent() else {
                total = total.saturating_add(s.text);
                continue;
            };
            let Some(p) = stats.get_mut(parent.id().index()) else {
                continue;
            };
            p.text = p.text.saturating_add(s.text);
            p.links = p.links.saturating_add(s.links);
            p.commas = p.commas.saturating_add(s.commas);
            if node.is_text() {
                p.own = p.own.saturating_add(s.text);
            }
            let unlikely = if f & SELF_UNLIKELY != 0 {
                s.text
            } else {
                s.unlikely
            };
            p.unlikely = p.unlikely.saturating_add(unlikely);
            p.para_below |= s.para_below || paragraph;
        }

        let stat = |n: NodeRef<'_>| stats.get(n.id().index()).copied().unwrap_or_default();
        let flag = |n: NodeRef<'_>| flags.get(n.id().index()).copied().unwrap_or(HIDDEN);

        // Step 1: a single good <article>, else a single good <main>.
        let good = |n: NodeRef<'_>| {
            let s = stat(n);
            let enough = (s.text >= MIN_SEMANTIC && s.text.saturating_mul(5) >= total)
                || s.text.saturating_mul(2) >= total;
            s.text >= MIN_PARAGRAPH
                && enough
                && s.links.saturating_mul(2) <= s.text
                && s.unlikely.saturating_mul(4) <= s.text
                && flag(n) & (HIDDEN | UNLIKELY) == 0
        };
        let mut articles = Vec::new();
        let mut mains = Vec::new();
        let mut enclosing = vec![0u8; size];
        for node in &order {
            let inherited = node
                .parent()
                .and_then(|p| enclosing.get(p.id().index()))
                .copied()
                .unwrap_or(0);
            let mut f = inherited;
            if let Some(tag) = node.tag_name() {
                if tag == "article" && good(*node) {
                    if inherited & IN_GOOD_ARTICLE == 0 {
                        articles.push(*node);
                    }
                    f |= IN_GOOD_ARTICLE;
                }
                if is_main(*node, tag) && good(*node) {
                    if inherited & IN_GOOD_MAIN == 0 {
                        mains.push(*node);
                    }
                    f |= IN_GOOD_MAIN;
                }
            }
            if let Some(slot) = enclosing.get_mut(node.id().index()) {
                *slot = f;
            }
        }
        if let [only] = articles.as_slice() {
            return Some(*only);
        }
        if let [only] = mains.as_slice() {
            return Some(*only);
        }

        // Steps 2 and 3: score paragraphs into their ancestors; the document
        // itself (paragraphs without a parent element) is the extra slot.
        for skip_unlikely in [true, false] {
            let mut scores: Vec<Option<f64>> = vec![None; size + 1];
            let mut any = false;
            for node in &order {
                let Some(tag) = node.tag_name() else {
                    continue;
                };
                let s = stat(*node);
                let f = flag(*node);
                let blocked = f & HIDDEN != 0 || (skip_unlikely && f & UNLIKELY != 0);
                if blocked
                    || s.para_below
                    || s.text < MIN_PARAGRAPH
                    || s.links.saturating_mul(2) > s.text
                    || !paragraph_like(tag, s.own)
                {
                    continue;
                }
                any = true;
                let score = 1.0 + s.commas as f64 + (s.text / 100).min(3) as f64;
                let mut ancestor = node.parent();
                for divider in DIVIDERS {
                    let slot = ancestor.map_or(size, |a| a.id().index());
                    if let Some(entry) = scores.get_mut(slot) {
                        let start = match (*entry, ancestor) {
                            (Some(sum), _) => sum,
                            (None, Some(a)) => base_score(a, a.tag_name().unwrap_or("")),
                            (None, None) => 0.0,
                        };
                        *entry = Some(start + score / divider);
                    }
                    match ancestor {
                        Some(a) => ancestor = a.parent(),
                        None => break,
                    }
                }
            }
            if !any {
                continue;
            }

            let density = |s: Stats| {
                if s.text == 0 {
                    0.0
                } else {
                    s.links as f64 / s.text as f64
                }
            };
            // The document slot first: a real element must beat it.
            let document = Stats {
                text: total,
                links: order
                    .iter()
                    .filter(|n| n.parent().is_none())
                    .map(|n| stat(*n).links)
                    .sum(),
                ..Stats::default()
            };
            let mut best: Option<(Option<NodeRef<'_>>, f64)> = scores
                .get(size)
                .copied()
                .flatten()
                .map(|score| (None, score * (1.0 - density(document))));
            for node in &order {
                if let Some(Some(score)) = scores.get(node.id().index()) {
                    let fin = score * (1.0 - density(stat(*node)));
                    if best.is_none_or(|(_, b)| fin > b) {
                        best = Some((Some(*node), fin));
                    }
                }
            }
            let mut winner = best?.0?;

            // Step 4: widen to the whole article.
            while let Some(parent) = winner.parent() {
                if matches!(parent.tag_name(), Some("html" | "body") | None)
                    || flag(parent) & (HIDDEN | SELF_UNLIKELY) != 0
                {
                    break;
                }
                let (c, p) = (stat(winner), stat(parent));
                let own_unlikely = if flag(winner) & SELF_UNLIKELY != 0 {
                    c.text
                } else {
                    c.unlikely
                };
                let extra = p.text.saturating_sub(c.text);
                let extra_links = p.links.saturating_sub(c.links);
                if p.unlikely > own_unlikely
                    || extra > c.text
                    || extra_links.saturating_mul(2) > extra
                {
                    break;
                }
                winner = parent;
            }
            return Some(winner);
        }
        None
    }
}

/// Non-whitespace characters and commas in a text node.
fn measure(text: &str) -> (usize, usize) {
    let mut chars = 0;
    let mut commas = 0;
    for c in text.chars() {
        if !is_space(c) && !c.is_whitespace() {
            chars += 1;
            if matches!(c, ',' | '，' | '、') {
                commas += 1;
            }
        }
    }
    (chars, commas)
}

/// Elements that hold a paragraph of text.
fn paragraph_like(tag: &str, own: usize) -> bool {
    match tag {
        "p" | "pre" | "td" | "li" | "dd" => true,
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "th" | "caption" | "summary" | "legend" => false,
        _ => {
            matches!(tags::display(tag), Display::Block | Display::Paragraph)
                && own >= MIN_PARAGRAPH
        }
    }
}

fn is_main(node: NodeRef<'_>, tag: &str) -> bool {
    tag == "main" || node.attr("role").is_some_and(|r| has_token(r, "main"))
}

fn is_hidden(node: NodeRef<'_>, tag: &str) -> bool {
    if tags::display(tag) == Display::Hidden
        || node.has_attr("hidden")
        || node
            .attr("aria-hidden")
            .is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
    {
        return true;
    }
    node.attr("style").is_some_and(|style| {
        let style: alloc::string::String = style
            .chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| c.to_ascii_lowercase())
            .collect();
        style.contains("display:none") || style.contains("visibility:hidden")
    })
}

fn is_unlikely(node: NodeRef<'_>, tag: &str, in_section: bool) -> bool {
    match tag {
        "html" | "body" | "article" | "main" => return false,
        "nav" | "aside" => return true,
        "header" | "footer" if !in_section => return true,
        _ => {}
    }
    if let Some(role) = node.attr("role") {
        if has_token(role, "main") {
            return false;
        }
        if UNLIKELY_ROLES.iter().any(|r| has_token(role, r)) {
            return true;
        }
    }
    let words = || {
        node.attr("class")
            .into_iter()
            .chain(node.attr("id"))
            .flat_map(words)
    };
    words().any(is_negative) && !words().any(|w| matches_any(w, KEEP, &[]))
}

/// The starting score of a candidate: by tag, plus ±25 for content /
/// boilerplate words in its `class` and in its `id`.
fn base_score(node: NodeRef<'_>, tag: &str) -> f64 {
    let mut score = match tag {
        "div" | "article" | "main" => 5.0,
        "pre" | "td" | "blockquote" => 3.0,
        "address" | "ol" | "ul" | "dl" | "dd" | "dt" | "li" | "form" => -3.0,
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "th" => -5.0,
        _ => 0.0,
    };
    for value in [node.attr("class"), node.attr("id")].into_iter().flatten() {
        if words(value).any(is_negative) {
            score -= 25.0;
        }
        if words(value).any(|w| matches_any(w, POSITIVE, POSITIVE_EXACT)) {
            score += 25.0;
        }
    }
    score
}

/// The ASCII-alphanumeric words of a `class` or `id` value.
fn words(value: &str) -> impl Iterator<Item = &str> {
    value
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
}

fn is_negative(word: &str) -> bool {
    matches_any(word, NEGATIVE, NEGATIVE_EXACT)
}

/// Whether `word` starts with one of `prefixes` or equals one of `exact`
/// (ASCII case-insensitive).
fn matches_any(word: &str, prefixes: &[&str], exact: &[&str]) -> bool {
    prefixes.iter().any(|p| {
        word.get(..p.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(p))
    }) || exact.iter().any(|e| word.eq_ignore_ascii_case(e))
}
