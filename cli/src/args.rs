//! Hand-rolled argument parsing (no `clap`: the CLI stays zero-dependency).
//!
//! Flags mirror `htmlq` where they overlap, so `domtree` works as a drop-in
//! replacement in existing scripts.

use std::ffi::OsString;
use std::fmt;

pub const HELP: &str = "\
domtree — query HTML with CSS selectors

USAGE:
    domtree [OPTIONS] [SELECTOR]...

    Reads HTML from stdin (or --filename) and prints every element matching
    SELECTOR. Several SELECTOR arguments are joined with spaces. With no
    selector, the whole document is used.

EXAMPLES:
    curl -s https://example.com | domtree 'a' -a href
    domtree -f page.html -t 'article h2'
    curl -s https://blog.rust-lang.org | domtree -r 'nav, footer' --markdown main
    domtree -c 'img:not([alt])' < page.html

OUTPUT (default: matching elements as HTML, one per line):
    -a, --attribute <NAME>      Print this attribute of each match (skips matches
                                without it). With no selector, searches all elements.
    -t, --text                  Print readable text of each match
    -m, --markdown              Print each match as Markdown
    -j, --json                  Print matches as a JSON array (document JSON with
                                no selector)
    -c, --count                 Print the number of matches
    -p, --pretty                Pretty-print HTML output
    -1, --first                 Only use the first match
    -w, --ignore-whitespace     Skip matches whose output is empty or whitespace

DOCUMENT:
    -r, --remove-nodes <SEL>    Remove nodes matching SEL before selecting
                                (repeatable)
    -b, --base <URL>            Resolve relative href/src/... URLs against URL
    -B, --detect-base           Use the document's <base href> (falls back to --base)

INPUT / OUTPUT:
    -f, --filename <FILE>       Read from FILE instead of stdin ('-' = stdin)
    -o, --output <FILE>         Write to FILE instead of stdout

    -h, --help                  Print this help
    -V, --version               Print the version

Selectors: type, .class, #id, [attr op value], descendant/child/sibling
combinators, :nth-child(), :not(), :is(), :has(), :contains(\"text\"), and more.
Docs: https://github.com/iamsaquib8/dom-tree-rs
";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Html,
    Text,
    Markdown,
    Json,
    Count,
}

#[derive(Debug, Default)]
pub struct Options {
    pub selector: String,
    pub mode: Mode,
    pub attribute: Option<String>,
    pub pretty: bool,
    pub first: bool,
    pub ignore_whitespace: bool,
    pub remove: Vec<String>,
    pub base: Option<String>,
    pub detect_base: bool,
    pub input: Option<String>,
    pub output: Option<String>,
}

#[derive(Debug)]
pub enum Command {
    Run(Options),
    Help,
    Version,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ArgError(String);

impl fmt::Display for ArgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Parse command-line arguments (without the program name).
pub fn parse<I: IntoIterator<Item = OsString>>(args: I) -> Result<Command, ArgError> {
    let mut opts = Options::default();
    let mut selector: Vec<String> = Vec::new();
    let mut args = args.into_iter().map(|a| a.to_string_lossy().into_owned());

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
                .ok_or_else(|| ArgError(format!("unknown option '--{name}'")))?;
            if flag.takes_value() {
                let value = match inline {
                    Some(v) => v,
                    None => args
                        .next()
                        .ok_or_else(|| ArgError(format!("option '--{name}' needs a value")))?,
                };
                apply_value(&mut opts, flag, value);
            } else if inline.is_some() {
                return Err(ArgError(format!("option '--{name}' takes no value")));
            } else if let Some(cmd) = apply_switch(&mut opts, flag) {
                return Ok(cmd);
            }
        } else if arg.len() > 1 && arg.starts_with('-') {
            // One or more bundled short flags: `-tw`, `-ahref`, `-a href`.
            let mut chars = arg.char_indices().skip(1);
            while let Some((i, c)) = chars.next() {
                let flag =
                    Flag::from_short(c).ok_or_else(|| ArgError(format!("unknown option '-{c}'")))?;
                if flag.takes_value() {
                    let rest = arg.get(i + c.len_utf8()..).unwrap_or("");
                    let value = if rest.is_empty() {
                        args.next()
                            .ok_or_else(|| ArgError(format!("option '-{c}' needs a value")))?
                    } else {
                        rest.to_string()
                    };
                    apply_value(&mut opts, flag, value);
                    break;
                } else if let Some(cmd) = apply_switch(&mut opts, flag) {
                    return Ok(cmd);
                }
            }
        } else {
            selector.push(arg);
        }
    }

    opts.selector = selector.join(" ").trim().to_string();
    Ok(Command::Run(opts))
}

#[derive(Clone, Copy)]
enum Flag {
    Attribute,
    Text,
    Markdown,
    Json,
    Count,
    Pretty,
    First,
    IgnoreWhitespace,
    Remove,
    Base,
    DetectBase,
    Filename,
    Output,
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
            "count" => Flag::Count,
            "pretty" => Flag::Pretty,
            "first" => Flag::First,
            "ignore-whitespace" => Flag::IgnoreWhitespace,
            "remove-nodes" | "remove" => Flag::Remove,
            "base" => Flag::Base,
            "detect-base" => Flag::DetectBase,
            "filename" | "file" => Flag::Filename,
            "output" => Flag::Output,
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
            Flag::Attribute | Flag::Remove | Flag::Base | Flag::Filename | Flag::Output
        )
    }
}

fn apply_value(opts: &mut Options, flag: Flag, value: String) {
    match flag {
        Flag::Attribute => opts.attribute = Some(value),
        Flag::Remove => opts.remove.push(value),
        Flag::Base => opts.base = Some(value),
        Flag::Filename => opts.input = Some(value),
        Flag::Output => opts.output = Some(value),
        _ => {}
    }
}

/// Apply a value-less flag; returns a command that short-circuits parsing.
fn apply_switch(opts: &mut Options, flag: Flag) -> Option<Command> {
    match flag {
        Flag::Text => opts.mode = Mode::Text,
        Flag::Markdown => opts.mode = Mode::Markdown,
        Flag::Json => opts.mode = Mode::Json,
        Flag::Count => opts.mode = Mode::Count,
        Flag::Pretty => opts.pretty = true,
        Flag::First => opts.first = true,
        Flag::IgnoreWhitespace => opts.ignore_whitespace = true,
        Flag::DetectBase => opts.detect_base = true,
        Flag::Help => return Some(Command::Help),
        Flag::Version => return Some(Command::Version),
        _ => {}
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Options {
        match parse(args.iter().map(OsString::from)) {
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
        assert_eq!(run(&["--attribute=href"]).attribute.as_deref(), Some("href"));
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
        assert_eq!(run(&["-r", "nav", "-r", "footer"]).remove, ["nav", "footer"]);
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
}
