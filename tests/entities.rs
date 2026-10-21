//! Numeric character references: WHATWG Windows-1252 remapping and the
//! U+FFFD replacement rules.

use domtree::parse;

fn decode(html: &str) -> String {
    parse(html)
        .root()
        .map(|n| n.text_content())
        .unwrap_or_default()
}

#[test]
fn c1_range_is_remapped_through_windows_1252() {
    assert_eq!(decode("<p>&#150;</p>"), "\u{2013}"); // en dash
    assert_eq!(decode("<p>&#151;</p>"), "\u{2014}"); // em dash
    assert_eq!(decode("<p>&#128;</p>"), "\u{20ac}"); // euro
    assert_eq!(decode("<p>&#x99;</p>"), "\u{2122}"); // trade mark
    assert_eq!(decode("<p>&#X92;s</p>"), "\u{2019}s"); // right single quote
    assert_eq!(
        decode("<p>&#145;&#146; &#147;&#148;</p>"),
        "\u{2018}\u{2019} \u{201c}\u{201d}"
    );
    assert_eq!(
        decode("<p>&#133;&#149;&#159;</p>"),
        "\u{2026}\u{2022}\u{178}"
    );
}

#[test]
fn unmapped_c1_slots_decode_to_themselves() {
    for code in [0x81u32, 0x8D, 0x8F, 0x90, 0x9D] {
        let html = format!("<p>&#{code};</p>");
        assert_eq!(decode(&html), char::from_u32(code).unwrap().to_string());
    }
}

#[test]
fn neighbours_of_the_c1_range_are_untouched() {
    assert_eq!(decode("<p>&#127;</p>"), "\u{7f}");
    assert_eq!(decode("<p>&#160;</p>"), "\u{a0}");
    assert_eq!(decode("<p>&#xA9;</p>"), "\u{a9}");
}

#[test]
fn invalid_code_points_become_replacement_characters() {
    assert_eq!(decode("<p>&#0;</p>"), "\u{fffd}");
    assert_eq!(decode("<p>&#x0;</p>"), "\u{fffd}");
    assert_eq!(decode("<p>&#xD800;</p>"), "\u{fffd}"); // surrogate
    assert_eq!(decode("<p>&#xDFFF;</p>"), "\u{fffd}");
    assert_eq!(decode("<p>&#x110000;</p>"), "\u{fffd}"); // above U+10FFFF
    assert_eq!(decode("<p>&#1114112;</p>"), "\u{fffd}");
    // Overflowing u32 still decodes (to U+FFFD) rather than staying verbatim.
    assert_eq!(decode("<p>&#99999999999999;</p>"), "\u{fffd}");
    assert_eq!(decode("<p>&#xFFFFFFFFFFFF;</p>"), "\u{fffd}");
    // The largest valid scalar value is fine.
    assert_eq!(decode("<p>&#x10FFFF;</p>"), "\u{10ffff}");
}

#[test]
fn malformed_numeric_references_stay_verbatim() {
    assert_eq!(decode("<p>&#;</p>"), "&#;");
    assert_eq!(decode("<p>&#x;</p>"), "&#x;");
    assert_eq!(decode("<p>&#xZZ;</p>"), "&#xZZ;");
    assert_eq!(decode("<p>&#+65;</p>"), "&#+65;");
    assert_eq!(decode("<p>&#65;</p>"), "A");
}

#[test]
fn remapping_applies_in_attributes_too() {
    let dom = parse("<a title='a&#150;b'>x</a>");
    assert_eq!(dom.root().unwrap().attr("title"), Some("a\u{2013}b"));
}

#[test]
fn entity_lookahead_is_bounded_with_multibyte_text() {
    // Byte 32 after each `&` falls inside a multi-byte character. The window
    // used to fall back to the whole rest of the input, so every `&` scanned
    // to EOF looking for a `;` (quadratic).
    let unit = format!("&{}é", "a".repeat(31));
    let html = format!("<p>{}</p>", unit.repeat(80_000));
    let start = std::time::Instant::now();
    let dom = parse(&html);
    assert!(
        start.elapsed().as_millis() < 1000,
        "took {:?}",
        start.elapsed()
    );
    assert_eq!(dom.root().unwrap().text_content(), unit.repeat(80_000));
}

#[test]
fn far_semicolon_is_not_part_of_a_reference() {
    // A `;` beyond the 32-byte lookahead never turns the run into a reference.
    assert_eq!(
        decode("<p>&ampéééééééééééééééééé;</p>"),
        "&ampéééééééééééééééééé;"
    );
    assert_eq!(decode("<p>&amp;é</p>"), "&é");
}

#[test]
fn named_references_need_their_semicolon() {
    // With or without `entities-full`: the legacy no-semicolon spellings stay
    // verbatim and are reported.
    assert_eq!(decode("<p>&amp &copy 2026 &lt</p>"), "&amp &copy 2026 &lt");
    let dom = parse("<p>AT&ampT</p>");
    assert!(
        dom.errors()
            .iter()
            .any(|e| e.kind == domtree::ErrorKind::UnknownEntity)
    );
    assert_eq!(decode("<p>&amp;&copy;&lt;</p>"), "&©<");
}

#[cfg(not(feature = "entities-full"))]
#[test]
fn curated_table_leaves_rare_names_verbatim() {
    assert_eq!(
        decode("<p>&fjlig; &NotNestedGreaterGreater;</p>"),
        "&fjlig; &NotNestedGreaterGreater;"
    );
}

#[cfg(feature = "entities-full")]
mod full_table {
    use super::*;

    #[test]
    fn multi_code_point_and_long_names() {
        assert_eq!(
            decode("<p>&NotNestedGreaterGreater;</p>"),
            "\u{2aa2}\u{338}"
        );
        assert_eq!(decode("<p>&fjlig;</p>"), "fj");
        assert_eq!(decode("<p>&ThickSpace;</p>"), "\u{205f}\u{200a}");
        // The longest name in the table (31 bytes) still fits the lookahead.
        assert_eq!(
            decode("<p>&CounterClockwiseContourIntegral;</p>"),
            "\u{2233}"
        );
        assert_eq!(decode("<p>&bsol;&quot;&Tab;&NewLine;</p>"), "\\\"\t\n");
    }

    #[test]
    fn names_are_case_sensitive() {
        assert_eq!(decode("<p>&AMP; &amp; &Amp;</p>"), "& & &Amp;");
        assert_eq!(decode("<p>&Aacute;&aacute;</p>"), "\u{c1}\u{e1}");
    }

    #[test]
    fn unknown_names_stay_verbatim_and_are_reported() {
        let dom = parse("<p>&notanentity; &Notanentity;</p>");
        assert_eq!(
            dom.root().unwrap().text_content(),
            "&notanentity; &Notanentity;"
        );
        assert!(
            dom.errors()
                .iter()
                .any(|e| e.kind == domtree::ErrorKind::UnknownEntity)
        );
    }

    #[test]
    fn full_names_decode_in_attributes() {
        let dom = parse("<a title='&fjlig;&ShortRightArrow;'>x</a>");
        assert_eq!(dom.root().unwrap().attr("title"), Some("fj\u{2192}"));
    }
}
