//! Page metadata ([`Dom::metadata`]): title, description, canonical URL,
//! language, OpenGraph / Twitter-card fields, JSON-LD and feeds.

use alloc::string::String;
use alloc::vec::Vec;

use super::{collapse, has_token, non_empty};
use crate::serialize::walk;
use crate::tree::{Dom, NodeRef};

/// Metadata about a page, from its `<title>`, `<meta>` and `<link>` tags,
/// JSON-LD blocks and a few in-page conventions. Built by [`Dom::metadata`].
///
/// Every field is owned, so the struct outlives the [`Dom`]. Scalar fields
/// take the first non-empty value from a list of sources, in the order given
/// on each field; text fields have their whitespace collapsed, URL fields are
/// trimmed but **not** resolved (use [`resolve_url`](crate::resolve_url) with
/// the page URL or [`Dom::base_url`]).
///
/// With the `serde` feature, `Metadata` implements `serde::Serialize` (a map
/// with one key per field).
#[non_exhaustive]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metadata {
    /// `<title>` (not one inside an `<svg>`), else `og:title`, else
    /// `twitter:title`.
    pub title: Option<String>,
    /// `<meta name=description>`, else `og:description`, else
    /// `twitter:description`.
    pub description: Option<String>,
    /// `<link rel=canonical href>`, else `og:url`.
    pub canonical: Option<String>,
    /// The `lang` of the `<html>` element, else `<meta
    /// http-equiv=content-language>`.
    pub lang: Option<String>,
    /// `og:site_name`, else `<meta name=application-name>`.
    pub site_name: Option<String>,
    /// `og:image` (or `og:image:url`, `og:image:secure_url`), else
    /// `twitter:image` (or `twitter:image:src`), else `<link rel=image_src>`.
    pub image: Option<String>,
    /// `<meta name=author>`, else `article:author` unless it is a URL, else
    /// the text of the first `<a rel=author>`, else the `article:author` URL.
    pub author: Option<String>,
    /// The publication date as written (usually ISO 8601):
    /// `article:published_time`, else `<meta itemprop=datePublished>`, else a
    /// `date`-style `<meta name>` (`date`, `pubdate`, `publish-date`,
    /// `dc.date`, `dc.date.issued`, `dcterms.date`, `dcterms.created`, …), else
    /// the `datetime` of the first `<time>` that is inside an `<article>` or
    /// carries `pubdate` / `itemprop=datePublished`.
    pub published: Option<String>,
    /// `<meta name=keywords>` (else `news_keywords`) split on commas, else the
    /// `article:tag` values.
    pub keywords: Vec<String>,
    /// Every OpenGraph `<meta property=… content=…>` pair — properties in the
    /// `og:`, `article:`, `book:`, `profile:`, `music:` and `video:`
    /// namespaces — in document order, with the full, lower-cased property
    /// name as the key (`"og:title"`, `"article:published_time"`). Repeated
    /// properties (several `og:image`s) are all kept.
    pub og: Vec<(String, String)>,
    /// Every Twitter-card `<meta name=twitter:… content=…>` pair (also when
    /// written with `property=`), in document order, keys lower-cased
    /// (`"twitter:card"`).
    pub twitter: Vec<(String, String)>,
    /// The raw, trimmed contents of each non-empty `<script
    /// type="application/ld+json">`, in document order. Not parsed: hand them
    /// to a JSON parser.
    pub json_ld: Vec<String>,
    /// The `href` of each `<link rel=alternate>` whose `type` is RSS
    /// (`application/rss+xml`), Atom (`application/atom+xml`) or JSON Feed
    /// (`application/feed+json`), in document order.
    pub feeds: Vec<String>,
}

/// `<meta name>` values that hold a publication date.
const DATE_NAMES: &[&str] = &[
    "date",
    "pubdate",
    "publishdate",
    "publish-date",
    "publish_date",
    "article.published",
    "dc.date",
    "dc.date.issued",
    "dcterms.date",
    "dcterms.created",
    "dcterms.issued",
];

const OG_PREFIXES: &[&str] = &["og:", "article:", "book:", "profile:", "music:", "video:"];

const FEED_TYPES: &[&str] = &[
    "application/rss+xml",
    "application/atom+xml",
    "application/feed+json",
];

impl Dom {
    /// The page's [`Metadata`]: title, description, canonical URL, language,
    /// OpenGraph and Twitter-card fields, raw JSON-LD blocks and feed links.
    ///
    /// The whole document is searched (not just `<head>`: this parser does not
    /// move misplaced tags there), in one pass. Names and properties match
    /// ASCII case-insensitively; empty values are ignored.
    ///
    /// ```
    /// let dom = domtree::parse(r#"<html lang="en"><head>
    ///   <title>  Shipping a parser
    ///     | Blog </title>
    ///   <meta name="description" content="What we learned.">
    ///   <meta property="og:image" content="https://example.com/cover.png">
    ///   <meta name="twitter:card" content="summary_large_image">
    ///   <link rel="canonical" href="https://example.com/p/1">
    ///   <link rel="alternate" type="application/rss+xml" href="/feed.xml">
    ///   <script type="application/ld+json">{"@type": "BlogPosting"}</script>
    /// </head></html>"#);
    ///
    /// let meta = dom.metadata();
    /// assert_eq!(meta.title.as_deref(), Some("Shipping a parser | Blog"));
    /// assert_eq!(meta.description.as_deref(), Some("What we learned."));
    /// assert_eq!(meta.lang.as_deref(), Some("en"));
    /// assert_eq!(meta.canonical.as_deref(), Some("https://example.com/p/1"));
    /// assert_eq!(meta.image.as_deref(), Some("https://example.com/cover.png"));
    /// assert_eq!(meta.twitter, [("twitter:card".to_string(), "summary_large_image".to_string())]);
    /// assert_eq!(meta.json_ld, [r#"{"@type": "BlogPosting"}"#]);
    /// assert_eq!(meta.feeds, ["/feed.xml"]);
    /// ```
    pub fn metadata(&self) -> Metadata {
        let mut scan = Scan::default();
        for root in self.roots() {
            walk(root, |node, enter| scan.visit(node, enter));
        }
        scan.finish()
    }
}

/// What one pass over the document collects; [`Scan::finish`] applies the
/// fallbacks.
#[derive(Default)]
struct Scan<'a> {
    meta: Metadata,
    /// `<meta name=… content=…>` pairs (names lower-cased), first one wins.
    names: Vec<(String, &'a str)>,
    title: Option<String>,
    html_lang: Option<&'a str>,
    seen_html: bool,
    canonical: Option<&'a str>,
    image_src: Option<&'a str>,
    author_link: Option<String>,
    item_published: Option<&'a str>,
    time: Option<&'a str>,
    /// Depth of open `<svg>` and `<article>` elements.
    svg: u32,
    article: u32,
}

impl<'a> Scan<'a> {
    fn visit(&mut self, node: NodeRef<'a>, enter: bool) -> bool {
        let Some(tag) = node.tag_name() else {
            return false;
        };
        if !enter {
            match tag {
                "svg" => self.svg = self.svg.saturating_sub(1),
                "article" => self.article = self.article.saturating_sub(1),
                _ => {}
            }
            return false;
        }
        match tag {
            "svg" => self.svg = self.svg.saturating_add(1),
            "article" => self.article = self.article.saturating_add(1),
            "html" if !self.seen_html => {
                self.seen_html = true;
                self.html_lang = node.attr("lang").and_then(non_empty);
            }
            "title" if self.svg == 0 && self.title.is_none() => {
                self.title = Some(collapse(&node.text_content())).filter(|t| !t.is_empty());
                return false;
            }
            "meta" => self.meta_tag(node),
            "link" => self.link_tag(node),
            "a" if self.author_link.is_none() => {
                if node.attr("rel").is_some_and(|r| has_token(r, "author")) {
                    self.author_link = Some(collapse(&node.to_text())).filter(|t| !t.is_empty());
                }
            }
            "time" if self.time.is_none() => {
                let marked = self.article > 0
                    || node.has_attr("pubdate")
                    || node
                        .attr("itemprop")
                        .is_some_and(|p| has_token(p, "datePublished"));
                if marked {
                    self.time = node.attr("datetime").and_then(non_empty);
                }
            }
            "script" => {
                let ld = node.attr("type").is_some_and(|t| {
                    let mime = t.split(';').next().unwrap_or("").trim();
                    mime.eq_ignore_ascii_case("application/ld+json")
                });
                if ld {
                    if let Some(json) = non_empty(&node.text_content()) {
                        self.meta.json_ld.push(String::from(json));
                    }
                }
                return false;
            }
            "style" | "template" => return false,
            _ => {}
        }
        true
    }

    fn meta_tag(&mut self, node: NodeRef<'a>) {
        let Some(content) = node.attr("content").and_then(non_empty) else {
            return;
        };
        if let Some(property) = node.attr("property").and_then(non_empty) {
            let key = property.to_ascii_lowercase();
            if OG_PREFIXES.iter().any(|p| key.starts_with(p)) {
                self.meta.og.push((key, String::from(content)));
            } else if key.starts_with("twitter:") {
                self.meta.twitter.push((key, String::from(content)));
            }
        }
        if let Some(name) = node.attr("name").and_then(non_empty) {
            let key = name.to_ascii_lowercase();
            if key.starts_with("twitter:") {
                self.meta.twitter.push((key.clone(), String::from(content)));
            } else if key.starts_with("og:") {
                // A common mistake for `property="og:…"`.
                self.meta.og.push((key.clone(), String::from(content)));
            }
            self.names.push((key, content));
        }
        if let Some(http) = node.attr("http-equiv") {
            if http.trim().eq_ignore_ascii_case("content-language") {
                self.names.push((String::from("content-language"), content));
            }
        }
        if self.item_published.is_none()
            && node
                .attr("itemprop")
                .is_some_and(|p| has_token(p, "datePublished"))
        {
            self.item_published = Some(content);
        }
    }

    fn link_tag(&mut self, node: NodeRef<'a>) {
        let (Some(rel), Some(href)) = (node.attr("rel"), node.attr("href").and_then(non_empty))
        else {
            return;
        };
        if self.canonical.is_none() && has_token(rel, "canonical") {
            self.canonical = Some(href);
        }
        if self.image_src.is_none() && has_token(rel, "image_src") {
            self.image_src = Some(href);
        }
        if has_token(rel, "alternate") {
            let feed = node.attr("type").is_some_and(|t| {
                let mime = t.split(';').next().unwrap_or("").trim();
                FEED_TYPES.iter().any(|f| mime.eq_ignore_ascii_case(f))
            });
            if feed {
                self.meta.feeds.push(String::from(href));
            }
        }
    }

    fn name(&self, key: &str) -> Option<&'a str> {
        self.names
            .iter()
            .find(|(k, _)| k == key)
            .map(|&(_, value)| value)
    }

    fn og(&self, key: &str) -> Option<&str> {
        find(&self.meta.og, key)
    }

    fn twitter(&self, key: &str) -> Option<&str> {
        find(&self.meta.twitter, key)
    }

    fn finish(mut self) -> Metadata {
        let text = |s: Option<&str>| s.map(collapse).filter(|s| !s.is_empty());
        let url = |s: Option<&str>| s.map(String::from);

        let title = self.title.take().or_else(|| {
            text(
                self.og("og:title")
                    .or_else(|| self.twitter("twitter:title")),
            )
        });
        let description = text(
            self.name("description")
                .or_else(|| self.og("og:description"))
                .or_else(|| self.twitter("twitter:description")),
        );
        let canonical = url(self.canonical.or_else(|| self.og("og:url")));
        let lang = url(self.html_lang.or_else(|| self.name("content-language")));
        let site_name = text(
            self.og("og:site_name")
                .or_else(|| self.name("application-name")),
        );
        let image = url(self
            .og("og:image")
            .or_else(|| self.og("og:image:url"))
            .or_else(|| self.og("og:image:secure_url"))
            .or_else(|| self.twitter("twitter:image"))
            .or_else(|| self.twitter("twitter:image:src"))
            .or(self.image_src));
        // `article:author` is often a profile URL: prefer a name.
        let og_author = self.og("article:author").map(String::from);
        let is_url = |a: &String| a.contains("://");
        let author = text(self.name("author"))
            .or_else(|| text(og_author.as_deref().filter(|a| !a.contains("://"))))
            .or_else(|| self.author_link.take())
            .or_else(|| og_author.filter(is_url));
        let published = url(self
            .og("article:published_time")
            .or(self.item_published)
            .or_else(|| DATE_NAMES.iter().find_map(|k| self.name(k)))
            .or(self.time));
        let mut keywords: Vec<String> = self
            .name("keywords")
            .or_else(|| self.name("news_keywords"))
            .map(|k| {
                k.split(',')
                    .filter_map(non_empty)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        if keywords.is_empty() {
            keywords = self
                .meta
                .og
                .iter()
                .filter(|(k, _)| k == "article:tag")
                .map(|(_, v)| v.clone())
                .collect();
        }

        Metadata {
            title,
            description,
            canonical,
            lang,
            site_name,
            image,
            author,
            published,
            keywords,
            ..self.meta
        }
    }
}

fn find<'v>(pairs: &'v [(String, String)], key: &str) -> Option<&'v str> {
    pairs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

#[cfg(feature = "serde")]
impl serde::Serialize for Metadata {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("Metadata", 13)?;
        st.serialize_field("title", &self.title)?;
        st.serialize_field("description", &self.description)?;
        st.serialize_field("canonical", &self.canonical)?;
        st.serialize_field("lang", &self.lang)?;
        st.serialize_field("site_name", &self.site_name)?;
        st.serialize_field("image", &self.image)?;
        st.serialize_field("author", &self.author)?;
        st.serialize_field("published", &self.published)?;
        st.serialize_field("keywords", &self.keywords)?;
        st.serialize_field("og", &self.og)?;
        st.serialize_field("twitter", &self.twitter)?;
        st.serialize_field("json_ld", &self.json_ld)?;
        st.serialize_field("feeds", &self.feeds)?;
        st.end()
    }
}
