//! HTML character-reference (entity) decoding.
//!
//! We decode the ~130 named entities that actually appear in real-world content
//! plus full numeric references (`&#169;`, `&#xA9;`). Unknown named references
//! are left **verbatim** (browser-ish behaviour) and reported via
//! [`ErrorKind::UnknownEntity`](crate::ErrorKind::UnknownEntity). Shipping the
//! full ~2200-entry WHATWG table would conflict with the crate's "tiny" goal;
//! the curated subset is documented and covers the common cases.

use alloc::borrow::Cow;
use alloc::string::String;

/// Maximum length we will scan looking for a closing `;`. Real entity names are
/// short; capping avoids treating a far-away `;` (as in `&nbsp ; foo`) as part
/// of a reference and keeps decoding linear.
const MAX_ENTITY_LEN: usize = 32;

/// The Unicode replacement character, used for out-of-range / null numeric refs.
const REPLACEMENT: char = '\u{FFFD}';

/// Decode HTML entities in `input`.
///
/// Returns the decoded text (borrowed unchanged when there is nothing to do)
/// and a flag that is `true` if at least one unknown named reference was left
/// verbatim.
pub(crate) fn decode(input: &str) -> (Cow<'_, str>, bool) {
    if !input.as_bytes().contains(&b'&') {
        return (Cow::Borrowed(input), false);
    }

    let mut out = String::with_capacity(input.len());
    let mut unknown = false;
    let mut rest = input;

    while let Some(amp) = rest.find('&') {
        // Everything before the '&' is literal.
        if let Some(literal) = rest.get(..amp) {
            out.push_str(literal);
        }
        // `after` starts just past the '&'.
        let after = match rest.get(amp + 1..) {
            Some(s) => s,
            None => {
                out.push('&');
                rest = "";
                break;
            }
        };

        match decode_one(after) {
            Some((decoded, consumed)) => {
                out.push_str(&decoded);
                rest = after.get(consumed..).unwrap_or("");
            }
            None => {
                // Not a valid reference: keep the '&' verbatim and continue.
                unknown = true;
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    (Cow::Owned(out), unknown)
}

/// Try to decode a single reference whose text begins right after the `&`.
/// On success returns the decoded string plus how many bytes of `after` were
/// consumed (including the trailing `;`).
fn decode_one(after: &str) -> Option<(Cow<'static, str>, usize)> {
    let window = after.get(..after.len().min(MAX_ENTITY_LEN)).unwrap_or(after);
    let semi = window.find(';')?;
    let name = window.get(..semi)?;
    if name.is_empty() {
        return None;
    }

    if let Some(stripped) = name.strip_prefix('#') {
        let ch = numeric(stripped)?;
        let mut s = String::new();
        s.push(ch);
        return Some((Cow::Owned(s), semi + 1));
    }

    named(name).map(|s| (Cow::Borrowed(s), semi + 1))
}

/// Decode a numeric reference body (the part after `&#`): `123` or `x1F600`.
fn numeric(digits: &str) -> Option<char> {
    let hex_body = digits
        .strip_prefix('x')
        .or_else(|| digits.strip_prefix('X'));
    let code = if let Some(hex) = hex_body {
        if hex.is_empty() {
            return None;
        }
        u32::from_str_radix(hex, 16).ok()?
    } else {
        if digits.is_empty() {
            return None;
        }
        digits.parse::<u32>().ok()?
    };
    if code == 0 {
        return Some(REPLACEMENT);
    }
    Some(char::from_u32(code).unwrap_or(REPLACEMENT))
}

/// Curated named-entity table. Returns the UTF-8 replacement, or `None` if the
/// name is not recognised (caller leaves it verbatim).
fn named(name: &str) -> Option<&'static str> {
    Some(match name {
        // Core / markup-significant
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => "\u{a0}",
        // Spaces & invisibles
        "ensp" => "\u{2002}",
        "emsp" => "\u{2003}",
        "thinsp" => "\u{2009}",
        "zwnj" => "\u{200c}",
        "zwj" => "\u{200d}",
        "lrm" => "\u{200e}",
        "rlm" => "\u{200f}",
        "shy" => "\u{ad}",
        // Punctuation & typography
        "hellip" => "\u{2026}",
        "mdash" => "\u{2014}",
        "ndash" => "\u{2013}",
        "lsquo" => "\u{2018}",
        "rsquo" => "\u{2019}",
        "sbquo" => "\u{201a}",
        "ldquo" => "\u{201c}",
        "rdquo" => "\u{201d}",
        "bdquo" => "\u{201e}",
        "laquo" => "\u{ab}",
        "raquo" => "\u{bb}",
        "lsaquo" => "\u{2039}",
        "rsaquo" => "\u{203a}",
        "bull" => "\u{2022}",
        "middot" => "\u{b7}",
        "dagger" => "\u{2020}",
        "Dagger" => "\u{2021}",
        "permil" => "\u{2030}",
        "prime" => "\u{2032}",
        "Prime" => "\u{2033}",
        "oline" => "\u{203e}",
        "frasl" => "\u{2044}",
        // Currency & legal
        "cent" => "\u{a2}",
        "pound" => "\u{a3}",
        "curren" => "\u{a4}",
        "yen" => "\u{a5}",
        "euro" => "\u{20ac}",
        "copy" => "\u{a9}",
        "reg" => "\u{ae}",
        "trade" => "\u{2122}",
        // Symbols & misc latin-1
        "sect" => "\u{a7}",
        "para" => "\u{b6}",
        "deg" => "\u{b0}",
        "plusmn" => "\u{b1}",
        "times" => "\u{d7}",
        "divide" => "\u{f7}",
        "frac14" => "\u{bc}",
        "frac12" => "\u{bd}",
        "frac34" => "\u{be}",
        "sup1" => "\u{b9}",
        "sup2" => "\u{b2}",
        "sup3" => "\u{b3}",
        "micro" => "\u{b5}",
        "iexcl" => "\u{a1}",
        "iquest" => "\u{bf}",
        "brvbar" => "\u{a6}",
        "uml" => "\u{a8}",
        "ordf" => "\u{aa}",
        "ordm" => "\u{ba}",
        "not" => "\u{ac}",
        "macr" => "\u{af}",
        "acute" => "\u{b4}",
        "cedil" => "\u{b8}",
        // Arrows & math
        "larr" => "\u{2190}",
        "uarr" => "\u{2191}",
        "rarr" => "\u{2192}",
        "darr" => "\u{2193}",
        "harr" => "\u{2194}",
        "lArr" => "\u{21d0}",
        "rArr" => "\u{21d2}",
        "hArr" => "\u{21d4}",
        "forall" => "\u{2200}",
        "part" => "\u{2202}",
        "exist" => "\u{2203}",
        "empty" => "\u{2205}",
        "nabla" => "\u{2207}",
        "isin" => "\u{2208}",
        "notin" => "\u{2209}",
        "ni" => "\u{220b}",
        "prod" => "\u{220f}",
        "sum" => "\u{2211}",
        "minus" => "\u{2212}",
        "lowast" => "\u{2217}",
        "radic" => "\u{221a}",
        "prop" => "\u{221d}",
        "infin" => "\u{221e}",
        "ang" => "\u{2220}",
        "and" => "\u{2227}",
        "or" => "\u{2228}",
        "cap" => "\u{2229}",
        "cup" => "\u{222a}",
        "int" => "\u{222b}",
        "there4" => "\u{2234}",
        "sim" => "\u{223c}",
        "cong" => "\u{2245}",
        "asymp" => "\u{2248}",
        "ne" => "\u{2260}",
        "equiv" => "\u{2261}",
        "le" => "\u{2264}",
        "ge" => "\u{2265}",
        "sub" => "\u{2282}",
        "sup" => "\u{2283}",
        "sube" => "\u{2286}",
        "supe" => "\u{2287}",
        "oplus" => "\u{2295}",
        "otimes" => "\u{2297}",
        "perp" => "\u{22a5}",
        "sdot" => "\u{22c5}",
        "loz" => "\u{25ca}",
        "spades" => "\u{2660}",
        "clubs" => "\u{2663}",
        "hearts" => "\u{2665}",
        "diams" => "\u{2666}",
        // Common accented latin (lower + upper for the frequent ones)
        "agrave" => "\u{e0}",
        "aacute" => "\u{e1}",
        "acirc" => "\u{e2}",
        "atilde" => "\u{e3}",
        "auml" => "\u{e4}",
        "aring" => "\u{e5}",
        "aelig" => "\u{e6}",
        "ccedil" => "\u{e7}",
        "egrave" => "\u{e8}",
        "eacute" => "\u{e9}",
        "ecirc" => "\u{ea}",
        "euml" => "\u{eb}",
        "igrave" => "\u{ec}",
        "iacute" => "\u{ed}",
        "icirc" => "\u{ee}",
        "iuml" => "\u{ef}",
        "ntilde" => "\u{f1}",
        "ograve" => "\u{f2}",
        "oacute" => "\u{f3}",
        "ocirc" => "\u{f4}",
        "otilde" => "\u{f5}",
        "ouml" => "\u{f6}",
        "oslash" => "\u{f8}",
        "ugrave" => "\u{f9}",
        "uacute" => "\u{fa}",
        "ucirc" => "\u{fb}",
        "uuml" => "\u{fc}",
        "yacute" => "\u{fd}",
        "yuml" => "\u{ff}",
        "szlig" => "\u{df}",
        "eth" => "\u{f0}",
        "thorn" => "\u{fe}",
        "Agrave" => "\u{c0}",
        "Aacute" => "\u{c1}",
        "Acirc" => "\u{c2}",
        "Auml" => "\u{c4}",
        "Aring" => "\u{c5}",
        "AElig" => "\u{c6}",
        "Ccedil" => "\u{c7}",
        "Egrave" => "\u{c8}",
        "Eacute" => "\u{c9}",
        "Ntilde" => "\u{d1}",
        "Ouml" => "\u{d6}",
        "Uuml" => "\u{dc}",
        "Oslash" => "\u{d8}",
        // Greek (the commonly seen ones)
        "alpha" => "\u{3b1}",
        "beta" => "\u{3b2}",
        "gamma" => "\u{3b3}",
        "delta" => "\u{3b4}",
        "epsilon" => "\u{3b5}",
        "theta" => "\u{3b8}",
        "lambda" => "\u{3bb}",
        "mu" => "\u{3bc}",
        "pi" => "\u{3c0}",
        "sigma" => "\u{3c3}",
        "phi" => "\u{3c6}",
        "omega" => "\u{3c9}",
        "Delta" => "\u{394}",
        "Sigma" => "\u{3a3}",
        "Omega" => "\u{3a9}",
        "Pi" => "\u{3a0}",
        _ => return None,
    })
}
