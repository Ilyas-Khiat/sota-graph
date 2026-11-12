//! How words of a paper's text meet registry spellings. Papers wrap words in
//! every kind of quote, bracket and emphasis, and their HTML carries invisible
//! characters; none of it may hide a setting value from the evidence check.
//! The tests run over every spelling of the shipped registry, not examples.

use sota_core::{Normalizer, Registry, clean_word, mentions_value};

const NORMALIZATION: &str = include_str!("../../../config/normalization.toml");
const KGC: &str = include_str!("../../../config/registry/kgc.toml");

fn registry() -> Registry {
    let normalizer = Normalizer::from_toml_str("normalization.toml", NORMALIZATION).unwrap();
    Registry::build(normalizer, &[("kgc.toml", KGC)]).unwrap()
}

/// Opening and closing marks seen around words in papers and their HTML:
/// ASCII and typographic quotes in several languages' conventions, brackets,
/// guillemets, CJK corner brackets, Markdown/LaTeX-ish emphasis.
const WRAPPERS: &[(&str, &str)] = &[
    ("", ""),
    ("\"", "\""),
    ("'", "'"),
    ("`", "'"),
    ("``", "''"),
    ("\u{201C}", "\u{201D}"),                 // “ ”
    ("\u{2018}", "\u{2019}"),                 // ‘ ’
    ("\u{201E}", "\u{201C}"),                 // „ “
    ("\u{201A}", "\u{2018}"),                 // ‚ ‘
    ("\u{00AB}", "\u{00BB}"),                 // « »
    ("\u{00AB}\u{00A0}", "\u{00A0}\u{00BB}"), // « » with no-break spaces
    ("\u{2039}", "\u{203A}"),                 // ‹ ›
    ("\u{300C}", "\u{300D}"),                 // 「 」
    ("\u{300E}", "\u{300F}"),                 // 『 』
    ("\u{2033}", "\u{2033}"),                 // ″ (double prime, a common OCR stand-in)
    ("\u{FF02}", "\u{FF02}"),                 // ＂ fullwidth
    ("(", ")"),
    ("[", "]"),
    ("{", "}"),
    ("\u{27E8}", "\u{27E9}"), // ⟨ ⟩
    ("*", "*"),
    ("_", "_"),
    ("\u{201C}(", ")\u{201D}"),
];

/// What can follow the closing mark.
const TRAILING: &[&str] = &[
    "",
    ",",
    ".",
    ";",
    ":",
    "!",
    "?",
    "\u{2026}",
    ").",
    "\u{201D},",
];

/// Every way the registry spells every value of every setting.
fn setting_spellings(r: &Registry) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for s in r.settings() {
        for v in &s.values {
            for spelling in std::iter::once(&v.id).chain(&v.aliases) {
                out.push((s.id.clone(), v.id.clone(), spelling.clone()));
            }
        }
    }
    out
}

#[test]
fn every_setting_spelling_is_found_whatever_wraps_it() {
    let r = registry();
    let mut missed = Vec::new();
    for (setting, value, spelling) in setting_spellings(&r) {
        for (open, close) in WRAPPERS {
            for trail in TRAILING {
                let text = format!("We use the {open}{spelling}{close}{trail} setting as usual.");
                if !mentions_value(&text, &setting, &value, &r) {
                    missed.push(text);
                }
            }
        }
    }
    assert!(
        missed.is_empty(),
        "{} missed, e.g. {:?}",
        missed.len(),
        &missed[..missed.len().min(5)]
    );
}

#[test]
fn a_word_at_the_start_or_end_of_the_text_is_found() {
    let r = registry();
    assert!(mentions_value(
        "\u{201C}Filtered\u{201D} ranking.",
        "protocol",
        "filtered",
        &r
    ));
    assert!(mentions_value(
        "ranking is \u{00AB}filtered\u{00BB}",
        "protocol",
        "filtered",
        &r
    ));
}

#[test]
fn invisible_characters_inside_a_word_do_not_hide_it() {
    let r = registry();
    for invisible in [
        '\u{00AD}', '\u{200B}', '\u{200C}', '\u{200D}', '\u{2060}', '\u{FEFF}',
    ] {
        let text = format!("the fil{invisible}tered setting");
        assert!(
            mentions_value(&text, "protocol", "filtered", &r),
            "U+{:04X} hides the word",
            invisible as u32
        );
    }
}

#[test]
fn stripping_marks_never_creates_a_match_that_is_not_there() {
    let r = registry();
    for text in [
        "the \u{201C}unfiltered\u{201D} candidates",
        "\u{201C}filtering\u{201D} is slow",
        "a \u{00AB}rawness\u{00BB} measure",
        "tail\u{2019}s embedding",
    ] {
        for (setting, value, _) in setting_spellings(&r) {
            assert!(
                !mentions_value(text, &setting, &value, &r),
                "{text:?} wrongly mentions {setting} = {value}"
            );
        }
    }
}

#[test]
fn clean_word_strips_marks_at_the_edges_only() {
    assert_eq!(clean_word("\u{201C}Filtered\u{201D},"), "Filtered");
    assert_eq!(clean_word("\u{00AB}\u{00A0}raw\u{00A0}\u{00BB}"), "raw");
    assert_eq!(clean_word("(1-to-n)."), "1-to-n");
    assert_eq!(clean_word("head+tail"), "head+tail");
    assert_eq!(clean_word("H@10\u{2191}"), "H@10");
    assert_eq!(clean_word("0-shot,"), "0-shot");
    assert_eq!(clean_word("\u{201C}\u{201D}"), "");
}

/// No spelling in the registry may depend on a mark at its edge, or cleaning
/// a mention would lose it.
#[test]
fn every_registry_spelling_survives_cleaning() {
    let r = registry();
    for (setting, value, spelling) in setting_spellings(&r) {
        let cleaned: Vec<&str> = spelling.split_whitespace().map(clean_word).collect();
        let got = r.resolve_setting_value(&setting, &cleaned.join(" "));
        assert_eq!(
            got.map(|v| v.id.as_str()),
            Some(value.as_str()),
            "{spelling:?}"
        );
    }
    for d in r.datasets() {
        for spelling in std::iter::once(&d.name).chain(&d.aliases) {
            let got = r.resolve_dataset(clean_word(spelling));
            assert_eq!(
                got.map(|x| x.id.as_str()),
                Some(d.id.as_str()),
                "{spelling:?}"
            );
        }
    }
    for m in r.metrics() {
        for spelling in std::iter::once(&m.name).chain(&m.aliases) {
            let got = r.resolve_metric(clean_word(spelling));
            assert_eq!(
                got.map(|x| x.id.as_str()),
                Some(m.id.as_str()),
                "{spelling:?}"
            );
        }
    }
}
