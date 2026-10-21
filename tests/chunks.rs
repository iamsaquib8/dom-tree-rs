//! RAG chunking: `Dom::to_markdown_chunks` / `NodeRef::to_markdown_chunks`.
#![cfg(feature = "markdown")]

use domtree::{Chunk, parse};

fn chunks(html: &str, max: usize) -> Vec<Chunk> {
    parse(html).to_markdown_chunks(max)
}

/// `(context, markdown)` pairs, for compact assertions.
fn view(chunks: &[Chunk]) -> Vec<(String, String)> {
    chunks
        .iter()
        .map(|c| (c.context(), c.markdown.clone()))
        .collect()
}

fn pair(context: &str, markdown: &str) -> (String, String) {
    (context.to_string(), markdown.to_string())
}

fn chars(s: &str) -> usize {
    s.chars().count()
}

/// The size promise: within budget, unless the chunk is one long line.
fn assert_budget(chunks: &[Chunk], max: usize) {
    for c in chunks {
        assert!(
            chars(&c.markdown) <= max || !c.markdown.contains('\n'),
            "chunk of {} chars over {max}: {:?}",
            chars(&c.markdown),
            c.markdown
        );
        assert!(!c.markdown.trim().is_empty());
    }
}

#[test]
fn empty_documents_have_no_chunks() {
    assert!(chunks("", 100).is_empty());
    assert!(chunks("<script>x</script><!-- c -->", 100).is_empty());
    assert!(chunks("<p>   </p>", 0).is_empty());
}

#[test]
fn sections_split_at_headings() {
    let html = "<p>Intro before any heading.</p>\
                <h1>Guide</h1><p>Welcome.</p>\
                <h2>Install</h2><p>Run it.</p>\
                <h3>From source</h3><p>Build it.</p>\
                <h2>Usage</h2><p>Use it.</p>\
                <h1>Appendix</h1><p>More.</p>";
    assert_eq!(
        view(&chunks(html, 1000)),
        [
            pair("", "Intro before any heading."),
            pair("Guide", "# Guide\n\nWelcome."),
            pair("Guide > Install", "## Install\n\nRun it."),
            pair(
                "Guide > Install > From source",
                "### From source\n\nBuild it."
            ),
            pair("Guide > Usage", "## Usage\n\nUse it."),
            pair("Appendix", "# Appendix\n\nMore."),
        ]
    );
}

#[test]
fn skipped_levels_and_shallower_headings() {
    let html = "<h3>Deep</h3><p>a</p><h1>Top</h1><p>b</p><h4>Skip</h4><p>c</p><h2>Mid</h2><p>d</p>";
    let paths: Vec<_> = chunks(html, 1000)
        .into_iter()
        .map(|c| c.heading_path)
        .collect();
    assert_eq!(
        paths,
        [
            vec!["Deep"],
            vec!["Top"],
            vec!["Top", "Skip"],
            vec!["Top", "Mid"],
        ]
    );
}

#[test]
fn a_heading_stays_with_its_subheading() {
    assert_eq!(
        view(&chunks(
            "<h1>Guide</h1><h2>Install</h2><p>Run it.</p>",
            1000
        )),
        [pair("Guide > Install", "# Guide\n\n## Install\n\nRun it.")]
    );
    // …but not with a sibling heading.
    assert_eq!(
        view(&chunks("<h2>Empty</h2><h2>Full</h2><p>x</p>", 1000)),
        [pair("Empty", "## Empty"), pair("Full", "## Full\n\nx")]
    );
}

#[test]
fn heading_paths_are_plain_text() {
    let html = r#"<h1>The <em>tiny</em> <a href="/p">parser</a></h1><p>x</p>
                  <h2><code>to_markdown()</code> &amp; <b>friends</b> [beta] #1</h2><p>y</p>
                  <h2>Ticks: <code>a`b</code> and 2 * 3 &lt;tag&gt;</h2><p>z</p>
                  <h2><img src=i.png alt="Logo"> Brand</h2><p>w</p>"#;
    let paths: Vec<_> = chunks(html, 1000)
        .into_iter()
        .map(|c| c.heading_path.last().cloned().unwrap_or_default())
        .collect();
    assert_eq!(
        paths,
        [
            "The tiny parser",
            "to_markdown() & friends [beta] #1",
            "Ticks: a`b and 2 * 3 <tag>",
            "Logo Brand",
        ]
    );
}

#[test]
fn long_heading_titles_are_capped_in_the_path() {
    // Cut at 256 characters, then `…`; the heading line itself stays whole.
    let long = "word ".repeat(100);
    let all = chunks(&format!("<h1>{long}</h1><p>x</p>"), 10_000);
    let title = &all[0].heading_path[0];
    assert_eq!(title.chars().count(), 257);
    assert!(title.ends_with("word w…"), "{title:?}");
    assert!(
        all[0]
            .markdown
            .starts_with(&format!("# {}\n\nx", long.trim_end()))
    );
    // No space is left dangling before the `…`.
    let spaced = format!("{} tail", "x".repeat(255));
    let all = chunks(&format!("<h2>{spaced}</h2><p>y</p>"), 10_000);
    assert_eq!(all[0].heading_path, [format!("{}…", "x".repeat(255))]);
    // Cut on a character boundary.
    let all = chunks(&format!("<h2>{}</h2><p>y</p>", "é🦀".repeat(200)), 10_000);
    assert_eq!(all[0].heading_path, [format!("{}…", "é🦀".repeat(128))]);
    // Exactly 256 characters: kept as is.
    let exact = "z".repeat(256);
    let all = chunks(&format!("<h3>{exact}</h3><p>y</p>"), 10_000);
    assert_eq!(all[0].heading_path, [exact]);
}

#[test]
fn output_stays_linear_with_huge_headings() {
    // Six ~8 KB nested headings over many small chunks: every chunk carries
    // the whole path, so uncapped titles made the output ~45x the input.
    let mut html = String::new();
    for level in 1..=6 {
        html.push_str(&format!("<h{level}>{}</h{level}>", "x".repeat(8_000)));
    }
    for _ in 0..500 {
        html.push_str(&format!("<p>{}</p>", "y".repeat(999)));
    }
    let all = chunks(&html, 1000);
    assert!(all.len() > 500);
    let mut out = 0;
    for c in &all {
        let path: usize = c.heading_path.iter().map(String::len).sum();
        assert!(path <= 6 * (256 + '…'.len_utf8()), "{path}");
        out += path + c.markdown.len();
    }
    assert!(out < 3 * html.len(), "{out} bytes out of {}", html.len());
}

#[test]
fn blocks_are_packed_under_the_budget() {
    let html = "<h2>S</h2><p>aaaa</p><p>bbbb</p><p>cccc</p><p>dddd</p>";
    // "## S\n\naaaa\n\nbbbb" is 16 characters.
    assert_eq!(
        view(&chunks(html, 16)),
        [pair("S", "## S\n\naaaa\n\nbbbb"), pair("S", "cccc\n\ndddd"),]
    );
}

#[test]
fn small_sections_are_not_merged() {
    let html = "<h2>A</h2><p>a</p><h2>B</h2><p>b</p>";
    assert_eq!(chunks(html, 10_000).len(), 2);
}

#[test]
fn code_blocks_are_kept_whole_when_they_fit() {
    let code = "fn a() {}\n\nfn b() {}\n\n# not a heading\n";
    let html = format!("<p>Before the code.</p><pre><code>{code}</code></pre><p>After.</p>");
    let all = chunks(&html, 50);
    assert_budget(&all, 50);
    assert!(
        all.iter()
            .any(|c| c.markdown == "```\nfn a() {}\n\nfn b() {}\n\n# not a heading\n```"),
        "{all:#?}"
    );
    // The `#` line inside the code is not a section break.
    assert!(all.iter().all(|c| c.heading_path.is_empty()));
}

#[test]
fn oversized_code_blocks_are_refenced() {
    let lines: Vec<String> = (0..20).map(|i| format!("let x{i} = {i};")).collect();
    let html = format!(
        "<h2>Code</h2><pre><code class=language-rust>{}</code></pre>",
        lines.join("\n")
    );
    let all = chunks(&html, 80);
    assert_budget(&all, 80);
    assert!(all.len() > 2);
    // The heading travels with the first piece; every piece is a fenced block.
    assert!(
        all[0]
            .markdown
            .starts_with("## Code\n\n```rust\nlet x0 = 0;"),
        "{all:#?}"
    );
    for c in &all {
        let body = c.markdown.trim_start_matches("## Code\n\n");
        assert!(
            body.starts_with("```rust\n") && body.ends_with("\n```"),
            "{body:?}"
        );
        assert_eq!(c.heading_path, ["Code"]);
    }
    let joined: String = all.iter().map(|c| c.markdown.as_str()).collect();
    for line in &lines {
        assert_eq!(joined.matches(line.as_str()).count(), 1, "{line}");
    }
}

#[test]
fn oversized_tables_repeat_their_header() {
    let rows: String = (0..30)
        .map(|i| format!("<tr><td>row {i}</td><td>{}</td></tr>", i * i))
        .collect();
    let html = format!("<table><thead><tr><th>Name</th><th>Square</th></tr></thead>{rows}</table>");
    let all = chunks(&html, 100);
    assert_budget(&all, 100);
    assert!(all.len() > 3);
    for c in &all {
        assert!(
            c.markdown
                .starts_with("| Name | Square |\n| --- | --- |\n| row "),
            "{:?}",
            c.markdown
        );
    }
    let joined: String = all.iter().map(|c| c.markdown.as_str()).collect();
    for i in 0..30 {
        assert_eq!(joined.matches(&format!("| row {i} |")).count(), 1);
    }
}

#[test]
fn long_lines_are_never_cut() {
    let long = "word ".repeat(100);
    let html = format!("<h2>Long</h2><p>{long}</p><p>short</p>");
    let all = chunks(&html, 50);
    assert_budget(&all, 50);
    assert_eq!(
        view(&all),
        [
            pair("Long", "## Long"),
            pair("Long", long.trim_end()),
            pair("Long", "short"),
        ]
    );
}

#[test]
fn lists_split_between_items() {
    let items: String = (0..10)
        .map(|i| format!("<li>item number {i}</li>"))
        .collect();
    let all = chunks(&format!("<ul>{items}</ul>"), 40);
    assert_budget(&all, 40);
    assert_eq!(all.len(), 5);
    assert_eq!(all[0].markdown, "- item number 0\n- item number 1");
}

#[test]
fn zero_budget_puts_every_line_alone() {
    let html = "<h1>T</h1><p>a</p><pre>x\n\ny</pre><ul><li>1</li><li>2</li></ul>";
    let all = chunks(html, 0);
    let md: Vec<_> = all.iter().map(|c| c.markdown.as_str()).collect();
    assert_eq!(md, ["# T", "a", "```", "x", "y", "```", "- 1", "- 2"]);
    assert!(all.iter().all(|c| c.heading_path == ["T"]));
}

#[test]
fn headings_inside_containers_do_not_split() {
    let html = "<blockquote><h2>Quoted</h2><p>q</p></blockquote><ul><li><h3>In list</h3></li></ul>";
    let all = chunks(html, 1000);
    assert_eq!(all.len(), 1);
    assert!(all[0].heading_path.is_empty());
}

#[test]
fn chunks_of_a_subtree() {
    let dom = parse(include_str!("pages/reader/docs.html"));
    let main = dom.main_content().unwrap();
    let all = main.to_markdown_chunks(400);
    assert_budget(&all, 400);
    let contexts: Vec<_> = all.iter().map(Chunk::context).collect();
    assert_eq!(
        contexts.first().map(String::as_str),
        Some("Connection pooling")
    );
    assert!(contexts.contains(&"Connection pooling > Configuring the pool".to_string()));
    assert!(contexts.contains(&"Connection pooling > Timeouts".to_string()));
    assert!(contexts.contains(&"Connection pooling > Health checks".to_string()));
    // Nothing from outside <main>.
    assert!(all.iter().all(|c| !c.markdown.contains("Writing queries")));
}

#[test]
fn whole_page_round_trip() {
    for html in [
        include_str!("pages/blog.html"),
        include_str!("pages/docs.html"),
        include_str!("pages/messy.html"),
        include_str!("pages/reader/blog-comments.html"),
        include_str!("pages/reader/news.html"),
        include_str!("pages/reader/product.html"),
    ] {
        let dom = parse(html);
        let md = dom.to_markdown();
        for max in [0, 1, 20, 80, 300, 1000, 100_000] {
            let all = dom.to_markdown_chunks(max);
            assert_budget(&all, max);
            // Without splits inside blocks, the chunks are exactly the
            // Markdown's blocks, regrouped.
            if max >= 100_000 {
                let joined: Vec<_> = all.iter().map(|c| c.markdown.as_str()).collect();
                assert_eq!(joined.join("\n\n"), md);
            }
            let flat = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
            let joined: String = all.iter().map(|c| flat(&c.markdown)).collect();
            assert!(joined.len() >= flat(&md).len());
        }
    }
}

#[cfg(feature = "serde")]
#[test]
fn serializes_with_serde() {
    let all = chunks("<h1>A</h1><p>b</p>", 100);
    let json = serde_json::to_string(&all).unwrap();
    assert_eq!(json, r##"[{"heading_path":["A"],"markdown":"# A\n\nb"}]"##);
}
