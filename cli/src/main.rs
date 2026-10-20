//! `domtree` — query HTML from the command line with CSS selectors.
//!
//! A thin, zero-dependency wrapper over the `dom-tree-rs` library. Flags mirror
//! `htmlq` where they overlap; see `domtree --help`.

mod args;
mod pretty;
mod url;

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
        Ok(Command::Help) => return emit(None, args::HELP),
        Ok(Command::Version) => {
            return emit(None, &format!("domtree {}\n", env!("CARGO_PKG_VERSION")));
        }
        Err(e) => {
            eprintln!("domtree: {e}\nTry 'domtree --help' for more information.");
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
    let html = read_input(opts.input.as_deref())?;
    let mut dom = domtree::parse(&html);

    for remove in &opts.remove {
        let sel = compile(remove)?;
        let ids: Vec<_> = dom.select(&sel).map(|n| n.id()).collect();
        for id in ids {
            dom.remove(id);
        }
    }

    if let Some(base) = base_url(&dom, opts) {
        absolutize(&mut dom, &base);
    }

    let selector = match (opts.selector.as_str(), &opts.attribute, opts.mode) {
        ("", Some(_), _) | ("", None, Mode::Count) => Some(compile("*")?),
        ("", None, _) => None,
        (s, _, _) => Some(compile(s)?),
    };

    let Some(selector) = selector else {
        return Ok(document_output(&dom, opts));
    };
    let mut matches: Vec<NodeRef<'_>> = dom.select(&selector).collect();
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
        Mode::Json => dom.to_json(),
        Mode::Count => unreachable!("count always has a selector"),
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
            Mode::Json => {
                let items: Vec<String> = values.iter().map(|v| json_string(v)).collect();
                format!("[{}]\n", items.join(","))
            }
            _ => lines(values.iter().map(|v| v.to_string()), "\n"),
        };
    }
    match opts.mode {
        Mode::Count => format!("{}\n", matches.len()),
        Mode::Json => {
            let items: Vec<String> = matches.iter().map(|m| m.to_json()).collect();
            format!("[{}]\n", items.join(","))
        }
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
        Mode::Html => lines(matches.iter().map(|m| m.to_html()), "\n"),
    }
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
    let detected = opts
        .detect_base
        .then(|| dom.select_first("base[href]").and_then(|b| b.attr("href")))
        .flatten();
    match (detected, &opts.base) {
        (Some(d), Some(b)) => Some(url::resolve(b, d)),
        (Some(d), None) => Some(d.to_string()),
        (None, b) => b.clone(),
    }
}

/// Rewrite every relative URL attribute in the document to an absolute one.
fn absolutize(dom: &mut Dom, base: &str) {
    let mut edits = Vec::new();
    for node in dom.nodes() {
        for (name, value) in node.attributes() {
            let resolved = if URL_ATTRS.contains(&name) {
                url::resolve(base, value)
            } else if name == "srcset" {
                url::resolve_srcset(base, value)
            } else {
                continue;
            };
            if resolved != value {
                edits.push((node.id(), name.to_string(), resolved));
            }
        }
    }
    for (id, name, value) in edits {
        dom.set_attr(id, &name, &value);
    }
}

fn read_input(path: Option<&str>) -> Result<String, String> {
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
    let text = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
    };
    Ok(match text.strip_prefix('\u{feff}') {
        Some(rest) => rest.to_string(),
        None => text,
    })
}

/// Write `out` to a file or stdout. A closed pipe (`domtree … | head`) is not
/// an error.
fn emit(path: Option<&str>, out: &str) -> ExitCode {
    let result = match path {
        Some(p) if p != "-" => fs::write(p, out).map_err(|e| format!("writing {p}: {e}")),
        _ => {
            let mut stdout = io::stdout().lock();
            match stdout.write_all(out.as_bytes()).and_then(|()| stdout.flush()) {
                Err(e) if e.kind() != io::ErrorKind::BrokenPipe => Err(format!("writing: {e}")),
                _ => Ok(()),
            }
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("domtree: {e}");
            ExitCode::FAILURE
        }
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
