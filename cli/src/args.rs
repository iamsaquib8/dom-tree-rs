//! Hand-rolled argument parsing (no `clap`: the CLI stays zero-dependency).
//!
//! Flags mirror `htmlq` where they overlap, so `domtree` works as a drop-in
//! replacement in existing scripts.

use std::ffi::OsString;
use std::fmt;

use crate::extract::Field;

pub const HELP: &str = "\
domtree — query HTML with CSS selectors

USAGE:
    domtree [OPTIONS] [SELECTOR]...
    domtree lint [OPTIONS] [FILE]...    (see 'domtree lint --help')

    Reads HTML from stdin (or --filename) and prints every element matching
    SELECTOR. Several SELECTOR arguments are joined with spaces. With no
    selector, the whole document is used.

EXAMPLES:
    curl -s https://example.com | domtree 'a' -a href
    domtree -f page.html -t 'article h2'
    curl -sL https://example.com/post | domtree --main --markdown
    domtree -e title=h1 -e price=.price -e 'tags=.tag[]' < product.html
    domtree '.card' -e name=h2 -e url=a@href --jsonl < list.html
    domtree --csv -1 < report.html > report.csv
    domtree -c 'img:not([alt])' < page.html
    domtree lint index.html docs/*.html

OUTPUT (default: matching elements as HTML, one per line; -t, -m, -j, --jsonl,
--csv and -c each pick the format, so give at most one):
    -a, --attribute <NAME>      Print this attribute of each match (skips matches
                                without it). With no selector, searches all elements.
    -t, --text                  Print readable text of each match
    -m, --markdown              Print each match as Markdown
    -j, --json                  Print matches as a JSON array (document JSON with
                                no selector)
        --jsonl                 Print one JSON value per line (JSON Lines)
        --csv                   Print matched <table>s as CSV (default selector:
                                table), separated by a blank line
    -c, --count                 Print the number of matches
    -p, --pretty                Pretty-print HTML output
    -1, --first                 Only use the first match
    -w, --ignore-whitespace     Skip matches whose output is empty or whitespace
                                (with --links, links without text)

EXTRACT:
    -e, --extract <NAME=SPEC>   Print a JSON object with a NAME key (repeat it);
                                SPEC is SELECTOR (text of the first match) or
                                SELECTOR@ATTR (its attribute); end it with []
                                for an array of all matches. An empty SELECTOR
                                is the scope element itself. With a SELECTOR
                                argument, prints one object per match of it, as
                                a JSON array (or JSON Lines with --jsonl).
        --links                 Print every link as URL<TAB>text (JSON objects
                                with --json / --jsonl); a SELECTOR keeps the
                                links inside its matches
        --meta                  Print the page metadata (title, description,
                                canonical, OpenGraph, JSON-LD, ...) as JSON

DOCUMENT:
    -r, --remove-nodes <SEL>    Remove nodes matching SEL before selecting
                                (repeatable)
        --main                  Keep only the main content (the article, without
                                navigation, sidebars and footer)
    -b, --base <URL>            Resolve relative href/src/... URLs against URL
    -B, --detect-base           Use the document's <base href>, itself resolved
                                against --base (falls back to --base)

INPUT / OUTPUT:
    -f, --filename <FILE>       Read from FILE instead of stdin ('-' = stdin)
    -o, --output <FILE>         Write to FILE instead of stdout
        --encoding <LABEL>      Decode the input as LABEL: utf-8, utf-16le,
                                utf-16be or windows-1252 (default: detected from
                                a BOM, then <meta charset>, then UTF-8 validity,
                                falling back to windows-1252)

    -h, --help                  Print this help
    -V, --version               Print the version

Selectors: type, .class, #id, [attr op value], descendant/child/sibling
combinators, :nth-child(), :not(), :is(), :has(), :contains(\"text\"), and more.
In --extract SPECs only the last @NAME and a trailing [] are special; write \\@
for a literal @ in a selector.

Exit codes: 0 success (also when nothing matches), 1 invalid selector or I/O
error, 2 bad usage.
Docs: https://github.com/iamsaquib8/dom-tree-rs
";

pub const LINT_HELP: &str = "\
domtree lint — check HTML for common accessibility and markup problems

USAGE:
    domtree lint [OPTIONS] [FILE]...

    Checks each FILE (stdin when none are given, or for '-') and prints one
    line per finding, in document order:

        FILE:LINE:COLUMN: LEVEL: MESSAGE [RULE]

RULES:
    img-alt             error    <img> without alt (alt=\"\" marks a decorative
                                 image; aria-label, title or role=presentation
                                 also count)
    empty-link          error    <a href> / <area href> without text,
                                 aria-label, title or image alt
    empty-button        error    <button> / <input type=button> without a name
    duplicate-id        error    an id that is already used (reported where it
                                 repeats, naming the first use)
    heading-order       warning  a skipped heading level (<h4> after <h2>)
    unclosed-element    warning  an element whose end tag HTML requires was
                                 closed by another tag, or never closed
    mismatched-end-tag  warning  an end tag that also closed such elements
                                 (<b><i>x</b>)
    stray-end-tag, unterminated-tag, unterminated-comment, unterminated-cdata,
    unexpected-eof, bogus-comment
                        warning  other markup the parser had to recover from
    implicitly-closed   note     an omitted optional end tag (</li>, </p>,
                                 </td>, ...): valid HTML
    unknown-entity      note     an & that starts no known character reference
                                 (a bare & in a URL, too)

OPTIONS:
        --format <FORMAT>       text (default) or json: one JSON array of
                                {file, line, column, level, rule, message}
    -q, --quiet                 Do not print notes
        --encoding <LABEL>      Decode the input as LABEL (see 'domtree --help')
    -h, --help                  Print this help

EXIT CODES:
    0  no errors or warnings (notes do not count)
    1  errors or warnings found
    2  bad usage, or a FILE could not be read
";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Html,
    Text,
    Markdown,
    Json,
    Jsonl,
    Csv,
    Count,
}

impl Mode {
    fn flag(self) -> &'static str {
        match self {
            Mode::Html => "HTML output",
            Mode::Text => "--text",
            Mode::Markdown => "--markdown",
            Mode::Json => "--json",
            Mode::Jsonl => "--jsonl",
            Mode::Csv => "--csv",
            Mode::Count => "--count",
        }
    }
}

#[derive(Debug, Default)]
pub struct Options {
    pub selector: String,
    pub mode: Mode,
    pub attribute: Option<String>,
    pub pretty: bool,
    pub first: bool,
    pub ignore_whitespace: bool,
    pub extract: Vec<Field>,
    pub links: bool,
    pub meta: bool,
    pub remove: Vec<String>,
    pub main: bool,
    pub base: Option<String>,
    pub detect_base: bool,
    pub input: Option<String>,
    pub output: Option<String>,
    pub encoding: Option<String>,
}

/// `domtree lint` options.
#[derive(Debug, Default)]
pub struct LintOptions {
    /// Files to check; empty = stdin.
    pub files: Vec<String>,
    pub json: bool,
    /// Leave out notes.
    pub quiet: bool,
    pub encoding: Option<String>,
}

#[derive(Debug)]
pub enum Command {
    Run(Options),
    Lint(LintOptions),
    Help,
    LintHelp,
    Version,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ArgError {
    message: String,
    /// The help command to point at.
    pub help: &'static str,
}

impl ArgError {
    fn new(message: String) -> ArgError {
        ArgError {
            message,
            help: "domtree --help",
        }
    }

    fn lint(message: String) -> ArgError {
        ArgError {
            message,
            help: "domtree lint --help",
        }
    }
}

impl fmt::Display for ArgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Parse command-line arguments (without the program name).
pub fn parse<I: IntoIterator<Item = OsString>>(args: I) -> Result<Command, ArgError> {
    let args: Vec<String> = args
        .into_iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    if args.first().is_some_and(|a| a == "lint") {
        return parse_lint(args.into_iter().skip(1));
    }

    let mut opts = Options::default();
    let mut selector: Vec<String> = Vec::new();
    let mut args = args.into_iter();

    while let Some(arg) = args.next() {
        if arg == "--" {
            selector.extend(args.by_ref());
            break;
        }
        if let Some(long) = arg.strip_prefix("--") {
            let (name, inline) = match long.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (long, None),
            };
            let flag = Flag::from_long(name)
                .ok_or_else(|| ArgError::new(format!("unknown option '--{name}'")))?;
            if flag.takes_value() {
                let value = match inline {
                    Some(v) => v,
                    None => args
                        .next()
                        .ok_or_else(|| ArgError::new(format!("option '--{name}' needs a value")))?,
                };
                apply_value(&mut opts, flag, value)?;
            } else if inline.is_some() {
                return Err(ArgError::new(format!("option '--{name}' takes no value")));
            } else if let Some(cmd) = apply_switch(&mut opts, flag)? {
                return Ok(cmd);
            }
        } else if arg.len() > 1 && arg.starts_with('-') {
            // One or more bundled short flags: `-tw`, `-ahref`, `-a href`.
            let chars = arg.char_indices().skip(1);
            for (i, c) in chars {
                let flag = Flag::from_short(c)
                    .ok_or_else(|| ArgError::new(format!("unknown option '-{c}'")))?;
                if flag.takes_value() {
                    let rest = arg.get(i + c.len_utf8()..).unwrap_or("");
                    let value = if rest.is_empty() {
                        args.next()
                            .ok_or_else(|| ArgError::new(format!("option '-{c}' needs a value")))?
                    } else {
                        rest.to_string()
                    };
                    apply_value(&mut opts, flag, value)?;
                    break;
                } else if let Some(cmd) = apply_switch(&mut opts, flag)? {
                    return Ok(cmd);
                }
            }
        } else {
            selector.push(arg);
        }
    }

    opts.selector = selector.join(" ").trim().to_string();
    validate(&opts)?;
    Ok(Command::Run(opts))
}

/// Parse the arguments after `lint`.
fn parse_lint(mut args: impl Iterator<Item = String>) -> Result<Command, ArgError> {
    let mut opts = LintOptions::default();
    while let Some(arg) = args.next() {
        if arg == "--" {
            opts.files.extend(args.by_ref());
            break;
        }
        let (name, inline) = match arg.split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n, Some(v.to_string())),
            _ => (arg.as_str(), None),
        };
        let mut value = |inline: Option<String>| {
            inline
                .or_else(|| args.next())
                .ok_or_else(|| ArgError::lint(format!("option '{name}' needs a value")))
        };
        match name {
            "-h" | "--help" => return Ok(Command::LintHelp),
            "-q" | "--quiet" if inline.is_none() => opts.quiet = true,
            "--format" => match value(inline)?.as_str() {
                "text" => opts.json = false,
                "json" => opts.json = true,
                other => {
                    return Err(ArgError::lint(format!(
                        "--format must be 'text' or 'json', not '{other}'"
                    )));
                }
            },
            "--encoding" => opts.encoding = Some(value(inline)?),
            "-" => opts.files.push(arg),
            _ if name.starts_with('-') => {
                return Err(ArgError::lint(format!("unknown lint option '{arg}'")));
            }
            _ => opts.files.push(arg),
        }
    }
    Ok(Command::Lint(opts))
}

/// Reject flag combinations that have no sensible meaning.
fn validate(opts: &Options) -> Result<(), ArgError> {
    let err = |msg: String| Err(ArgError::new(msg));
    let tasks: Vec<&str> = [
        (!opts.extract.is_empty(), "--extract"),
        (opts.links, "--links"),
        (opts.meta, "--meta"),
    ]
    .into_iter()
    .filter_map(|(on, name)| on.then_some(name))
    .collect();
    if let [a, b, ..] = tasks.as_slice() {
        return err(format!("{a} and {b} cannot be combined"));
    }
    if let Some(&task) = tasks.first() {
        if opts.attribute.is_some() {
            return err(format!("{task} cannot be combined with --attribute"));
        }
        let fits = match opts.mode {
            Mode::Html | Mode::Json | Mode::Jsonl => true,
            // Links are text lines (`-t` is the default) and countable.
            Mode::Text | Mode::Count => opts.links,
            Mode::Markdown | Mode::Csv => false,
        };
        if !fits {
            return err(format!(
                "{task} cannot be combined with {}",
                opts.mode.flag()
            ));
        }
        if opts.meta && !opts.selector.is_empty() {
            return err("--meta takes no SELECTOR (it describes the whole page)".into());
        }
    }
    if matches!(opts.mode, Mode::Csv | Mode::Markdown) && opts.attribute.is_some() {
        return err(format!(
            "{} cannot be combined with --attribute",
            opts.mode.flag()
        ));
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Flag {
    Attribute,
    Text,
    Markdown,
    Json,
    Jsonl,
    Csv,
    Count,
    Pretty,
    First,
    IgnoreWhitespace,
    Extract,
    Links,
    Meta,
    Remove,
    Main,
    Base,
    DetectBase,
    Filename,
    Output,
    Encoding,
    Help,
    Version,
}

impl Flag {
    fn from_long(name: &str) -> Option<Flag> {
        Some(match name {
            "attribute" => Flag::Attribute,
            "text" => Flag::Text,
            "markdown" => Flag::Markdown,
            "json" => Flag::Json,
            "jsonl" => Flag::Jsonl,
            "csv" => Flag::Csv,
            "count" => Flag::Count,
            "pretty" => Flag::Pretty,
            "first" => Flag::First,
            "ignore-whitespace" => Flag::IgnoreWhitespace,
            "extract" => Flag::Extract,
            "links" => Flag::Links,
            "meta" => Flag::Meta,
            "remove-nodes" | "remove" => Flag::Remove,
            "main" => Flag::Main,
            "base" => Flag::Base,
            "detect-base" => Flag::DetectBase,
            "filename" | "file" => Flag::Filename,
            "output" => Flag::Output,
            "encoding" => Flag::Encoding,
            "help" => Flag::Help,
            "version" => Flag::Version,
            _ => return None,
        })
    }

    fn from_short(c: char) -> Option<Flag> {
        Some(match c {
            'a' => Flag::Attribute,
            't' => Flag::Text,
            'm' => Flag::Markdown,
            'j' => Flag::Json,
            'c' => Flag::Count,
            'p' => Flag::Pretty,
            '1' => Flag::First,
            'w' => Flag::IgnoreWhitespace,
            'e' => Flag::Extract,
            'r' => Flag::Remove,
            'b' => Flag::Base,
            'B' => Flag::DetectBase,
            'f' => Flag::Filename,
            'o' => Flag::Output,
            'h' => Flag::Help,
            'V' => Flag::Version,
            _ => return None,
        })
    }

    fn takes_value(self) -> bool {
        matches!(
            self,
            Flag::Attribute
                | Flag::Extract
                | Flag::Remove
                | Flag::Base
                | Flag::Filename
                | Flag::Output
                | Flag::Encoding
        )
    }
}

fn apply_value(opts: &mut Options, flag: Flag, value: String) -> Result<(), ArgError> {
    match flag {
        Flag::Attribute => opts.attribute = Some(value),
        Flag::Extract => {
            let field = Field::parse(&value).map_err(ArgError::new)?;
            if opts.extract.iter().any(|f| f.name == field.name) {
                return Err(ArgError::new(format!(
                    "--extract name '{}' is used twice",
                    field.name
                )));
            }
            opts.extract.push(field);
        }
        Flag::Remove => opts.remove.push(value),
        Flag::Base => opts.base = Some(value),
        Flag::Filename => opts.input = Some(value),
        Flag::Output => opts.output = Some(value),
        Flag::Encoding => opts.encoding = Some(value),
        _ => {}
    }
    Ok(())
}

/// Apply a value-less flag; returns a command that short-circuits parsing.
fn apply_switch(opts: &mut Options, flag: Flag) -> Result<Option<Command>, ArgError> {
    let mode = match flag {
        Flag::Text => Mode::Text,
        Flag::Markdown => Mode::Markdown,
        Flag::Json => Mode::Json,
        Flag::Jsonl => Mode::Jsonl,
        Flag::Csv => Mode::Csv,
        Flag::Count => Mode::Count,
        Flag::Pretty => {
            opts.pretty = true;
            return Ok(None);
        }
        Flag::First => {
            opts.first = true;
            return Ok(None);
        }
        Flag::IgnoreWhitespace => {
            opts.ignore_whitespace = true;
            return Ok(None);
        }
        Flag::Links => {
            opts.links = true;
            return Ok(None);
        }
        Flag::Meta => {
            opts.meta = true;
            return Ok(None);
        }
        Flag::Main => {
            opts.main = true;
            return Ok(None);
        }
        Flag::DetectBase => {
            opts.detect_base = true;
            return Ok(None);
        }
        Flag::Help => return Ok(Some(Command::Help)),
        Flag::Version => return Ok(Some(Command::Version)),
        _ => return Ok(None),
    };
    // One output format: a second, different one is a mistake, not an
    // override (repeating the same flag is fine).
    if opts.mode != Mode::Html && opts.mode != mode {
        return Err(ArgError::new(format!(
            "{} and {} cannot be combined (pick one output format)",
            opts.mode.flag(),
            mode.flag()
        )));
    }
    opts.mode = mode;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn try_run(args: &[&str]) -> Result<Command, ArgError> {
        parse(args.iter().map(OsString::from))
    }

    fn run(args: &[&str]) -> Options {
        match try_run(args) {
            Ok(Command::Run(o)) => o,
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn joins_selector_words() {
        assert_eq!(run(&["ul", ">", "li"]).selector, "ul > li");
    }

    #[test]
    fn short_value_forms() {
        assert_eq!(run(&["-a", "href", "a"]).attribute.as_deref(), Some("href"));
        assert_eq!(run(&["-ahref", "a"]).attribute.as_deref(), Some("href"));
        assert_eq!(
            run(&["--attribute=href"]).attribute.as_deref(),
            Some("href")
        );
    }

    #[test]
    fn bundled_switches() {
        let o = run(&["-tw1", "p"]);
        assert_eq!(o.mode, Mode::Text);
        assert!(o.ignore_whitespace && o.first);
        assert_eq!(o.selector, "p");
    }

    #[test]
    fn repeatable_remove() {
        assert_eq!(
            run(&["-r", "nav", "-r", "footer"]).remove,
            ["nav", "footer"]
        );
    }

    #[test]
    fn double_dash_ends_options() {
        assert_eq!(run(&["--", "-weird"]).selector, "-weird");
    }

    #[test]
    fn errors() {
        assert!(parse([OsString::from("--nope")]).is_err());
        assert!(parse([OsString::from("-a")]).is_err());
        assert!(parse([OsString::from("--text=1")]).is_err());
    }

    #[test]
    fn help_and_version() {
        assert!(matches!(parse([OsString::from("-h")]), Ok(Command::Help)));
        assert!(matches!(
            parse([OsString::from("--version")]),
            Ok(Command::Version)
        ));
    }

    #[test]
    fn extract_fields() {
        let o = run(&["-e", "title=h1", "--extract=tags=.tag[]", "-eurl=a@href"]);
        let names: Vec<_> = o.extract.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["title", "tags", "url"]);
        assert!(try_run(&["-e", "title"]).is_err());
        assert!(try_run(&["-e", "a=h1", "-e", "a=h2"]).is_err());
    }

    #[test]
    fn new_switches() {
        let o = run(&["--main", "--links", "--jsonl", "--encoding", "latin1"]);
        assert!(o.main && o.links);
        assert_eq!(o.mode, Mode::Jsonl);
        assert_eq!(o.encoding.as_deref(), Some("latin1"));
        assert_eq!(run(&["--csv"]).mode, Mode::Csv);
        assert!(run(&["--meta", "-j"]).meta);
        assert_eq!(run(&["--links", "-c"]).mode, Mode::Count);
    }

    #[test]
    fn lint_options() {
        let Ok(Command::Lint(o)) = try_run(&[
            "lint",
            "-q",
            "--format=json",
            "a.html",
            "-",
            "--encoding",
            "latin1",
            "--",
            "-b.html",
        ]) else {
            panic!("not lint");
        };
        assert!(o.quiet && o.json);
        assert_eq!(o.files, ["a.html", "-", "-b.html"]);
        assert_eq!(o.encoding.as_deref(), Some("latin1"));
        assert!(matches!(try_run(&["lint", "-h"]), Ok(Command::LintHelp)));
        for bad in [
            &["lint", "--format", "xml"][..],
            &["lint", "-x"],
            &["lint", "--encoding"],
        ] {
            assert_eq!(
                try_run(bad).map(|_| ()).map_err(|e| e.help),
                Err("domtree lint --help")
            );
        }
        // Only the first argument names the subcommand.
        assert_eq!(run(&["-c", "lint"]).selector, "lint");
    }

    #[test]
    fn conflicting_flags() {
        for args in [
            &["--links", "--meta"][..],
            &["-e", "a=h1", "--links"],
            &["-e", "a=h1", "-t"],
            &["-e", "a=h1", "-a", "href"],
            &["--meta", "--csv"],
            &["--meta", "p"],
            &["--links", "-m"],
            &["--csv", "-a", "href"],
            &["-m", "-a", "href"],
            // Two output formats: no silent "last one wins".
            &["-t", "-j"],
            &["-tj"],
            &["-c", "--json"],
            &["--jsonl", "-j"],
            &["--csv", "-c"],
            &["-m", "-t"],
            &["--links", "-t", "--jsonl"],
            &["--meta", "-j", "--jsonl"],
            &["-e", "a=h1", "-j", "--jsonl"],
        ] {
            assert!(try_run(args).is_err(), "{args:?}");
        }
        assert_eq!(
            try_run(&["-c", "-j"])
                .map(|_| ())
                .map_err(|e| e.to_string()),
            Err("--count and --json cannot be combined (pick one output format)".into())
        );
    }

    #[test]
    fn compatible_flags() {
        for args in [
            &["-c", "-a", "href"][..],
            &["-j", "-a", "href"],
            &["--jsonl", "-a", "href"],
            &["-t", "-a", "href"],
            &["-p", "-a", "href"],
            &["-j", "-e", "a=h1"],
            &["--jsonl", "-e", "a=h1", "p"],
            &["--links", "-t"],
            &["--links", "-c", "-w", "-1"],
            &["--links", "-j"],
            &["--links", "--jsonl", "nav"],
            &["--meta", "-j"],
            &["--meta", "--jsonl"],
            &["--csv", "-1", "table"],
            &["-1", "-t", "-w", "p"],
            &["-c", "-1"],
            &["-t", "--text"],
            &["-tt"],
            &["-p", "-1"],
        ] {
            assert!(try_run(args).is_ok(), "{args:?}");
        }
    }
}
