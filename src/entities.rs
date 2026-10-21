//! HTML character-reference (entity) decoding.
//!
//! By default we decode the ~180 named entities that actually appear in
//! real-world content plus full numeric references (`&#169;`, `&#xA9;`, with
//! the browser's Windows-1252 remapping of `&#128;`–`&#159;`). Unknown named
//! references are left **verbatim** (browser-ish behaviour) and reported via
//! [`ErrorKind::UnknownEntity`](crate::ErrorKind::UnknownEntity). Shipping the
//! full WHATWG table by default would conflict with the crate's "tiny" goal, so
//! it is opt-in: the `entities-full` feature swaps the curated table for all
//! 2,125 `;`-terminated names (generated into `src/entities_full.rs` by
//! `scripts/gen_entities.py`; looked up by binary search).
//!
//! Either way a named reference needs its `;`: the legacy no-semicolon
//! spellings browsers also accept (`&amp`, `&copy`) stay verbatim.

use alloc::borrow::Cow;
use alloc::string::String;

use crate::scan;

/// Maximum length we will scan looking for a closing `;`. Real entity names are
/// short; capping avoids treating a far-away `;` (as in `&nbsp ; foo`) as part
/// of a reference and keeps decoding linear.
const MAX_ENTITY_LEN: usize = 32;

/// The Unicode replacement character, used for out-of-range / null numeric refs.
const REPLACEMENT: char = '\u{FFFD}';

/// Decode HTML entities in `input`.
///
/// Returns the decoded text (borrowed unchanged when there is nothing to do)
/// and a flag that is `true` if at least one reference-shaped `&…` was left
/// verbatim. `in_attr` selects the attribute-value reporting rule (see
/// [`decode_into`]).
pub(crate) fn decode(input: &str, in_attr: bool) -> (Cow<'_, str>, bool) {
    if scan::find_byte(b'&', input.as_bytes()).is_none() {
        return (Cow::Borrowed(input), false);
    }
    let mut out = String::with_capacity(input.len());
    let unknown = decode_into(input, &mut out, in_attr);
    (Cow::Owned(out), unknown)
}

/// Append `input` to `out` with its HTML entities decoded (the tree builder
/// decodes straight into its string arena, with no temporary). Returns `true`
/// if at least one reference-shaped `&…` was left verbatim.
///
/// Reporting follows WHATWG: an unknown `&name;` or a malformed `&#…` is always
/// reported; in text, so is a known legacy name missing its `;` (`AT&ampT`,
/// `&copy 2026`); in attribute values that is valid when an alphanumeric or `=`
/// follows (`?a=1&region=eu`), so only an exact known name is reported there.
pub(crate) fn decode_into(input: &str, out: &mut String, in_attr: bool) -> bool {
    let mut unknown = false;
    let mut rest = input;

    while let Some(amp) = scan::find_byte(b'&', rest.as_bytes()) {
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

        match decode_one(after, out) {
            Some(consumed) => rest = after.get(consumed..).unwrap_or(""),
            None => {
                // Not a valid reference: keep the '&' verbatim and continue.
                // Only a reference-shaped `&…` is reported; a bare `&`
                // (`a & b`, `?a=1&b=2`) is valid HTML.
                unknown |= looks_like_reference(after, in_attr);
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    unknown
}

/// Whether the text after an undecodable `&` was meant as a reference, by the
/// rules described on [`decode_into`].
fn looks_like_reference(after: &str, in_attr: bool) -> bool {
    let bytes = after.as_bytes();
    if bytes.first() == Some(&b'#') {
        return true;
    }
    let n = bytes
        .iter()
        .take(MAX_ENTITY_LEN)
        .take_while(|b| b.is_ascii_alphanumeric())
        .count();
    let Some(run) = after.get(..n).filter(|r| !r.is_empty()) else {
        return false;
    };
    if bytes.get(n) == Some(&b';') {
        return true;
    }
    if in_attr {
        // `&amp` followed by `=` or an alphanumeric is valid in attributes.
        return named(run).is_some() && !matches!(bytes.get(n), Some(b'='));
    }
    // In text, any known name at the start of the run is a missing `;`.
    (2..=n).any(|len| run.get(..len).and_then(named).is_some())
}

/// Try to decode a single reference whose text begins right after the `&`.
/// On success appends what it decodes to to `out` and returns how many bytes
/// of `after` were consumed (including the trailing `;`).
fn decode_one(after: &str, out: &mut String) -> Option<usize> {
    // Search bytes, not a `&str` window: a window cut at a fixed byte count can
    // land inside a multi-byte character. `;` is ASCII, so `semi` is always a
    // char boundary.
    let semi = after
        .as_bytes()
        .iter()
        .take(MAX_ENTITY_LEN)
        .position(|&b| b == b';')?;
    let name = after.get(..semi)?;
    if name.is_empty() {
        return None;
    }

    if let Some(stripped) = name.strip_prefix('#') {
        out.push(numeric(stripped)?);
    } else {
        out.push_str(named(name)?);
    }
    Some(semi + 1)
}

/// What numeric references `&#x80;`–`&#x9F;` decode to (WHATWG): the C1
/// control range is remapped through Windows-1252, because in real content it
/// almost always is mislabelled cp1252. Five slots are unmapped and decode to
/// themselves.
const WINDOWS_1252: [u16; 32] = [
    0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, // 80–87
    0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x008D, 0x017D, 0x008F, // 88–8F
    0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, // 90–97
    0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178, // 98–9F
];

/// Decode a numeric reference body (the part after `&#`): `123` or `x1F600`.
///
/// `0`, surrogates and anything above U+10FFFF (however many digits) decode to
/// U+FFFD; `0x80..=0x9F` go through [`WINDOWS_1252`].
fn numeric(digits: &str) -> Option<char> {
    let (body, radix) = match digits.strip_prefix(['x', 'X']) {
        Some(hex) => (hex, 16),
        None => (digits, 10),
    };
    if body.is_empty() {
        return None;
    }
    // Saturate rather than overflow: every out-of-range value is U+FFFD.
    let mut code: u32 = 0;
    for c in body.chars() {
        code = code
            .saturating_mul(radix)
            .saturating_add(c.to_digit(radix)?)
            .min(0x11_0000);
    }
    if let Some(&mapped) = code
        .checked_sub(0x80)
        .and_then(|i| WINDOWS_1252.get(i as usize))
    {
        code = u32::from(mapped);
    }
    Some(
        char::from_u32(code)
            .filter(|&c| c != '\0')
            .unwrap_or(REPLACEMENT),
    )
}

/// Look up a named reference (the part between `&` and `;`): the curated
/// table by default, the complete WHATWG one with the `entities-full`
/// feature. `None` means unknown (the caller leaves it verbatim).
#[inline]
fn named(name: &str) -> Option<&'static str> {
    #[cfg(feature = "entities-full")]
    {
        full(name)
    }
    #[cfg(not(feature = "entities-full"))]
    {
        curated(name)
    }
}

/// Binary search of the generated, name-sorted WHATWG table.
#[cfg(feature = "entities-full")]
fn full(name: &str) -> Option<&'static str> {
    use crate::entities_full::ENTITIES;
    let i = ENTITIES.binary_search_by(|&(k, _)| k.cmp(name)).ok()?;
    ENTITIES.get(i).map(|&(_, v)| v)
}

/// Curated named-entity table. Returns the UTF-8 replacement, or `None` if the
/// name is not recognised (caller leaves it verbatim).
#[cfg(any(not(feature = "entities-full"), test))]
fn curated(name: &str) -> Option<&'static str> {
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

#[cfg(all(test, feature = "entities-full"))]
mod tests {
    use crate::entities_full::ENTITIES;

    #[test]
    fn full_table_is_sorted_for_binary_search() {
        assert!(ENTITIES.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(
            ENTITIES
                .iter()
                .all(|(k, _)| k.len() < super::MAX_ENTITY_LEN)
        );
    }

    #[test]
    fn curated_table_is_a_subset_of_the_full_one() {
        // Turning the feature on must never change how a curated name decodes.
        let mut found = 0;
        for &(name, value) in &ENTITIES {
            if let Some(curated) = super::curated(name) {
                assert_eq!(curated, value, "&{name};");
                found += 1;
            }
        }
        assert_eq!(found, 178, "every curated name is a WHATWG name");
    }
}
