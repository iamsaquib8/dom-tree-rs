//! `domtree` — query HTML from the command line with CSS selectors.
//!
//! A thin, zero-dependency wrapper over the `dom-tree-rs` library. Flags mirror
//! `htmlq` where they overlap; see `domtree --help`.

mod args;
mod encoding;
mod extract;
mod json;
mod lint;
mod pretty;

use std::fs;
use std::io::{self, Read, Write};
use std::process::ExitCode;

use domtree::{Dom, NodeRef, Selector};

use args::{Command, Mode, Options};

/// Attributes holding a single URL, rewritten by `--base`.
const URL_ATTRS: &[&str] = &["href", "src", "action", "cite", "poster", "formaction"];

fn main() -> ExitCode {
    let opts = match args::parse(std::env::args_os().skip(1)) {
        Ok(Command::Run(opts)) => opts,
        Ok(Command::Lint(opts)) => {
            let (report, code) = lint::run(&opts);
            return match write_output(None, &report) {
                Ok(()) => ExitCode::from(code),
                Err(e) => {
                    eprintln!("domtree: {e}");
                    ExitCode::from(2)
                }
            };
        }
        Ok(Command::Help) => return emit(None, args::HELP),
        Ok(Command::LintHelp) => return emit(None, args::LINT_HELP),
        Ok(Command::Version) => {
            return emit(None, &format!("domtree {}\n", env!("CARGO_PKG_VERSION")));
        }
        Err(e) => {
            eprintln!("domtree: {e}\nTry '{}' for more information.", e.help);
            return ExitCode::from(2);
        }
    };
    match run(&opts) {
        Ok(out) => emit(opts.output.as_deref(), &out),
        Err(e) => {
            eprintln!("domtree: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(opts: &Options) -> Result<String, String> {
    let html = read_input(opts.input.as_deref(), opts.encoding.as_deref())?;
    let mut dom = domtree::parse(&html);

    for remove in &opts.remove {
        let sel = compile(remove)?;
        let ids: Vec<_> = dom.select(&sel).map(|n| n.id()).collect();
        for id in ids {
            dom.remove(id);
        }
    }

    let base = base_url(&dom, opts);
    if let Some(base) = &base {
        absolutize(&mut dom, base, opts.base.as_deref());
    }

    if opts.meta {
        // Page-level, so read before `--main` narrows the document.
        return Ok(meta_output(&dom, base.as_deref()));
    }
    if opts.main {
        if let Some(main) = dom.main_content().map(|m| m.to_html()) {
            dom = domtree::parse(&main);
        }
    }

    let scope = match opts.selector.as_str() {
        "" => None,
        s => Some(compile(s)?),
    };
    if !opts.extract.is_empty() {
        return extract_output(&dom, scope.as_ref(), opts);
    }
    if opts.links {
        return Ok(links_output(&dom, scope.as_ref(), base.as_deref(), opts));
    }

    let selector = match (scope, &opts.attribute, opts.mode) {
        (Some(sel), _, _) => Some(sel),
        (None, _, Mode::Csv) => Some(compile("table")?),
        (None, Some(_), _) | (None, None, Mode::Count) => Some(compile("*")?),
        (None, None, _) => None,
    };

    let Some(selector) = selector else {
        return Ok(document_output(&dom, opts));
    };
    let mut matches: Vec<NodeRef<'_>> = dom.select(&selector).collect();
    if opts.mode == Mode::Csv {
        return Ok(csv_output(&matches, opts.first));
    }
    if opts.first {
        matches.truncate(1);
    }
    Ok(match_output(&matches, opts))
}

/// Output for the whole document (no selector given).
fn document_output(dom: &Dom, opts: &Options) -> String {
    let out = match opts.mode {
        Mode::Html if opts.pretty => {
            let mut out = String::new();
            for root in dom.roots() {
                pretty::write(root, &mut out);
            }
            return out;
        }
        Mode::Html => dom.to_html(),
        Mode::Text => dom.to_text(),
        Mode::Markdown => dom.to_markdown(),
        Mode::Json | Mode::Jsonl => dom.to_json(),
        // Both always have a selector (see `run`).
        Mode::Count | Mode::Csv => String::new(),
    };
    with_newline(out)
}

/// Output for a list of selected elements.
fn match_output(matches: &[NodeRef<'_>], opts: &Options) -> String {
    if let Some(name) = &opts.attribute {
        let values: Vec<&str> = matches
            .iter()
            .filter_map(|m| m.attr(name))
            .filter(|v| !(opts.ignore_whitespace && v.trim().is_empty()))
            .collect();
        return match opts.mode {
            Mode::Count => format!("{}\n", values.len()),
            Mode::Json => format!("{}\n", json::string_array(values)),
            Mode::Jsonl => lines(values.iter().map(|v| json::string(v)), "\n"),
            _ => lines(values.iter().map(|v| v.to_string()), "\n"),
        };
    }
    match opts.mode {
        Mode::Count => format!("{}\n", matches.len()),
        Mode::Json => format!("{}\n", json::array(matches.iter().map(|m| m.to_json()))),
        Mode::Jsonl => lines(matches.iter().map(|m| m.to_json()), "\n"),
        Mode::Text => lines(non_blank(matches.iter().map(|m| m.to_text()), opts), "\n"),
        Mode::Markdown => lines(
            non_blank(matches.iter().map(|m| m.to_markdown()), opts),
            "\n\n",
        ),
        Mode::Html if opts.pretty => {
            let mut out = String::new();
            for m in matches {
                pretty::write(*m, &mut out);
            }
            out
        }
        Mode::Html | Mode::Csv => lines(matches.iter().map(|m| m.to_html()), "\n"),
    }
}

/// `--csv`: each matched `<table>` with rows as RFC 4180 CSV, separated by a
/// blank line. Other matches and tables without rows are skipped.
fn csv_output(matches: &[NodeRef<'_>], first: bool) -> String {
    let tables = matches
        .iter()
        .filter(|m| m.tag_name() == Some("table"))
        .map(|t| t.table_rows())
        .filter(|rows| !rows.is_empty())
        .take(if first { 1 } else { usize::MAX })
        .map(domtree::to_csv);
    tables.collect::<Vec<_>>().join("\r\n")
}

/// `-e`: one object, or one per match of the scope selector.
fn extract_output(dom: &Dom, scope: Option<&Selector>, opts: &Options) -> Result<String, String> {
    let fields = extract::compile(&opts.extract)?;
    let ws = opts.ignore_whitespace;
    let Some(scope) = scope else {
        return Ok(format!("{}\n", extract::object(dom, &fields, ws)));
    };
    let mut scopes: Vec<NodeRef<'_>> = dom.select(scope).collect();
    if opts.first {
        scopes.truncate(1);
    }
    let objects = extract::scoped_objects(dom, &scopes, &fields, ws).into_iter();
    Ok(if opts.mode == Mode::Jsonl {
        lines(objects, "\n")
    } else {
        format!("{}\n", json::array(objects))
    })
}

/// `--links`: every link (inside a match of `scope`, if given), its URL
/// resolved as `--base`/`-B` say — so with neither, URLs print as written.
/// With `-w`, links without text are skipped.
fn links_output(dom: &Dom, scope: Option<&Selector>, base: Option<&str>, opts: &Options) -> String {
    let inside = scope.map(|sel| inside_matches(dom, sel));
    let in_scope = |node: NodeRef<'_>| {
        inside
            .as_ref()
            .is_none_or(|marks| marks.get(node.id().index()).copied().unwrap_or(false))
    };
    let mut links: Vec<(String, String)> = dom
        .links(None)
        .into_iter()
        .filter(|l| in_scope(l.node))
        .filter(|l| !(opts.ignore_whitespace && l.text.trim().is_empty()))
        .map(|l| (domtree::resolve_url(base.unwrap_or(""), l.href), l.text))
        .collect();
    if opts.first {
        links.truncate(1);
    }
    let object = |(url, text): &(String, String)| {
        let mut obj = json::Object::default();
        obj.field("url", &json::string(url))
            .field("text", &json::string(text));
        obj.finish()
    };
    match opts.mode {
        Mode::Count => format!("{}\n", links.len()),
        Mode::Json => format!("{}\n", json::array(links.iter().map(object))),
        Mode::Jsonl => lines(links.iter().map(object), "\n"),
        _ => lines(
            links
                .iter()
                .map(|(url, text)| format!("{}\t{}", one_field(url), one_field(text))),
            "\n",
        ),
    }
}

/// Which nodes are a match of `sel` or inside one, indexed by
/// `NodeId::index()`: one pass in document order, where parents come before
/// their children (rather than a walk up the ancestors of every node, which
/// is quadratic in deep documents).
fn inside_matches(dom: &Dom, sel: &Selector) -> Vec<bool> {
    let mut marks = vec![false; dom.len()];
    for m in dom.select(sel) {
        if let Some(mark) = marks.get_mut(m.id().index()) {
            *mark = true;
        }
    }
    for node in dom.nodes() {
        let parent_inside = node
            .parent()
            .is_some_and(|p| marks.get(p.id().index()).copied().unwrap_or(false));
        if let Some(mark) = marks.get_mut(node.id().index()) {
            *mark |= parent_inside;
        }
    }
    marks
}

/// `s` with tabs and line breaks turned into spaces, for one TSV field.
fn one_field(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

/// `--meta`: [`Dom::metadata`] as a JSON object, keys as in the library's
/// `serde` form. With a base URL, `canonical`, `image` and `feeds` are made
/// absolute.
fn meta_output(dom: &Dom, base: Option<&str>) -> String {
    let m = dom.metadata();
    let url = |u: &str| match base {
        Some(base) => domtree::resolve_url(base, u),
        None => u.to_string(),
    };
    let opt = |v: &Option<String>| json::opt_string(v.as_deref());
    let opt_url = |v: &Option<String>| json::opt_string(v.as_deref().map(url).as_deref());
    let pairs = |v: &[(String, String)]| {
        json::array(
            v.iter()
                .map(|(k, v)| json::string_array([k.as_str(), v.as_str()])),
        )
    };
    let feeds: Vec<String> = m.feeds.iter().map(|f| url(f)).collect();
    let mut obj = json::Object::default();
    obj.field("title", &opt(&m.title))
        .field("description", &opt(&m.description))
        .field("canonical", &opt_url(&m.canonical))
        .field("lang", &opt(&m.lang))
        .field("site_name", &opt(&m.site_name))
        .field("image", &opt_url(&m.image))
        .field("author", &opt(&m.author))
        .field("published", &opt(&m.published))
        .field(
            "keywords",
            &json::string_array(m.keywords.iter().map(String::as_str)),
        )
        .field("og", &pairs(&m.og))
        .field("twitter", &pairs(&m.twitter))
        .field(
            "json_ld",
            &json::string_array(m.json_ld.iter().map(String::as_str)),
        )
        .field(
            "feeds",
            &json::string_array(feeds.iter().map(String::as_str)),
        );
    format!("{}\n", obj.finish())
}

/// Drop blank outputs when `--ignore-whitespace` is set.
fn non_blank<'o>(
    items: impl Iterator<Item = String> + 'o,
    opts: &'o Options,
) -> impl Iterator<Item = String> + 'o {
    items.filter(move |s| !(opts.ignore_whitespace && s.trim().is_empty()))
}

fn lines(items: impl Iterator<Item = String>, sep: &str) -> String {
    with_newline(items.collect::<Vec<_>>().join(sep))
}

fn with_newline(mut s: String) -> String {
    if !s.is_empty() && !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// Parse a selector, turning errors into a message that points at the problem.
fn compile(selector: &str) -> Result<Selector, String> {
    Selector::parse(selector).map_err(|e| {
        let col = selector
            .get(..e.position())
            .map_or(0, |s| s.chars().count());
        format!(
            "invalid selector: {}\n  {selector}\n  {}^",
            e.message(),
            " ".repeat(col)
        )
    })
}

/// The base URL to resolve links against: `<base href>` (with `-B`), resolved
/// against `--base` when both are present.
fn base_url(dom: &Dom, opts: &Options) -> Option<String> {
    if opts.detect_base {
        dom.base_url(opts.base.as_deref())
    } else {
        opts.base.clone()
    }
}

/// Rewrite every relative URL attribute in the document to an absolute one,
/// resolved against `base`.
///
/// A `<base href>` is the exception: it is relative to the document's own
/// URL (`--base`, if given), not to the base URL it defines — so with `-B`
/// it is resolved once, as in [`Dom::base_url`], instead of against itself.
///
/// A repeated attribute (`<a href=a href=b>`) is read and rewritten through
/// its first occurrence only — the one `NodeRef::attr` and `Dom::set_attr`
/// use — so the value that counts is the one resolved.
fn absolutize(dom: &mut Dom, base: &str, document_url: Option<&str>) {
    let mut edits = Vec::new();
    for node in dom.nodes() {
        let base = match node.tag_name() {
            Some("base") => match document_url {
                Some(url) => url,
                None => continue,
            },
            _ => base,
        };
        for &name in URL_ATTRS.iter().chain(&["srcset"]) {
            let Some(value) = node.attr(name) else {
                continue;
            };
            let resolved = if name == "srcset" {
                domtree::resolve_srcset(base, value)
            } else {
                domtree::resolve_url(base, value)
            };
            if resolved != value {
                edits.push((node.id(), name, resolved));
            }
        }
    }
    for (id, name, value) in edits {
        dom.set_attr(id, name, &value);
    }
}

/// Read the input and decode it (see `encoding.rs`).
fn read_input(path: Option<&str>, encoding: Option<&str>) -> Result<String, String> {
    let bytes = match path {
        None | Some("-") => {
            let mut buf = Vec::new();
            io::stdin()
                .read_to_end(&mut buf)
                .map_err(|e| format!("reading stdin: {e}"))?;
            buf
        }
        Some(p) => fs::read(p).map_err(|e| format!("reading {p}: {e}"))?,
    };
    let decoded = encoding::decode(bytes, encoding);
    if let Some(warning) = &decoded.warning {
        let name = match path {
            None | Some("-") => "<stdin>",
            Some(p) => p,
        };
        eprintln!("domtree: warning: {name}: {warning}");
    }
    Ok(decoded.text)
}

/// Write `out` to a file or stdout. A closed pipe (`domtree … | head`) is not
/// an error.
fn write_output(path: Option<&str>, out: &str) -> Result<(), String> {
    match path {
        Some(p) if p != "-" => fs::write(p, out).map_err(|e| format!("writing {p}: {e}")),
        _ => {
            let mut stdout = io::stdout().lock();
            match stdout
                .write_all(out.as_bytes())
                .and_then(|()| stdout.flush())
            {
                Err(e) if !is_closed_pipe(&e) => Err(format!("writing: {e}")),
                _ => Ok(()),
            }
        }
    }
}

/// [`write_output`], as the exit code.
fn emit(path: Option<&str>, out: &str) -> ExitCode {
    match write_output(path, out) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("domtree: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Whether a stdout write failed because the reader went away.
#[cfg(not(target_os = "wasi"))]
fn is_closed_pipe(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::BrokenPipe
}

/// Whether a stdout write failed because the reader went away. wasmtime
/// reports a closed pipe as `EIO` (WASI errno 29) rather than `EPIPE`.
#[cfg(target_os = "wasi")]
fn is_closed_pipe(e: &io::Error) -> bool {
    const WASI_EIO: i32 = 29;
    e.kind() == io::ErrorKind::BrokenPipe || e.raw_os_error() == Some(WASI_EIO)
}
