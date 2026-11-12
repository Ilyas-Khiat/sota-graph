use sota_core::{Direction, Metric, Scale, ValueError, is_missing, parse_number, to_canonical};

fn value(text: &str) -> f64 {
    parse_number(text)
        .unwrap_or_else(|e| panic!("{text:?}: {e}"))
        .value
}

#[test]
fn plain_numbers_as_tables_print_them() {
    assert_eq!(value(".338"), 0.338);
    assert_eq!(value("0.338"), 0.338);
    assert_eq!(value("357"), 357.0);
    assert_eq!(value("22438"), 22438.0);
    assert_eq!(value(" 0.43 "), 0.43);
    assert_eq!(value("\u{2212}0.5"), -0.5); // unicode minus
}

#[test]
fn decimals_are_remembered() {
    assert_eq!(parse_number(".338").unwrap().decimals, 3);
    assert_eq!(parse_number("357").unwrap().decimals, 0);
    assert_eq!(parse_number("33.80").unwrap().decimals, 2);
}

#[test]
fn thousands_separators_only_in_their_exact_shape() {
    assert_eq!(value("3,384"), 3384.0);
    assert_eq!(value("1,234,567"), 1_234_567.0);
    assert!(parse_number("1,23").is_err()); // a decimal comma is not guessed
}

#[test]
fn latex_wrappers_and_markers_are_ignored() {
    assert_eq!(value("\\mathbf{.338}"), 0.338);
    assert_eq!(value("\\textbf{0.5}"), 0.5);
    assert_eq!(value("\\underline{0.476}"), 0.476);
    assert_eq!(value(".338\u{2020}"), 0.338); // dagger
    assert_eq!(value("0.338*"), 0.338);
    assert_eq!(value("0.338^{\\dagger}"), 0.338);
    assert_eq!(value("0.338^{1}"), 0.338); // footnote number in a superscript
}

#[test]
fn standard_deviations_are_split_off() {
    let v = parse_number("0.338\\pm 0.002").unwrap();
    assert_eq!((v.value, v.std), (0.338, Some(0.002)));
    let v = parse_number("33.8 \u{00B1} 0.4").unwrap();
    assert_eq!((v.value, v.std), (33.8, Some(0.4)));
    let v = parse_number("0.338_{\\pm.002}").unwrap();
    assert_eq!((v.value, v.std), (0.338, Some(0.002)));
    assert_eq!(parse_number("0.338").unwrap().std, None);
}

#[test]
fn several_numbers_are_ambiguous() {
    for text in ["0.338 / 0.401", "0.338 (0.401)", "12 34"] {
        assert!(
            matches!(parse_number(text), Err(ValueError::Ambiguous(_))),
            "{text:?}"
        );
    }
}

#[test]
fn missing_values() {
    for text in [
        "", "-", "\u{2013}", "\u{2014}", "\u{2212}", "n/a", "N/A", "--", " - ",
    ] {
        assert!(is_missing(text), "{text:?}");
        assert!(
            matches!(parse_number(text), Err(ValueError::Missing)),
            "{text:?}"
        );
    }
    assert!(!is_missing("0"));
    assert!(matches!(parse_number("OOM"), Err(ValueError::NoNumber(_))));
}

fn metric(
    id: &str,
    canonical: Scale,
    accepted: &[Scale],
    min: Option<f64>,
    max: Option<f64>,
) -> Metric {
    Metric {
        id: id.into(),
        name: id.to_uppercase(),
        aliases: Vec::new(),
        direction: Direction::HigherIsBetter,
        canonical_scale: canonical,
        accepted_scales: accepted.to_vec(),
        min,
        max,
        notes: String::new(),
    }
}

#[test]
fn conversion_to_the_canonical_scale() {
    let mrr = metric(
        "mrr",
        Scale::Fraction,
        &[Scale::Fraction, Scale::Percent],
        Some(0.0),
        Some(1.0),
    );
    let pct = |t: &str| to_canonical(&parse_number(t).unwrap(), Scale::Percent, &mrr).unwrap();
    assert_eq!(pct("33.8"), 0.338); // exact, not 0.33799999999999997
    assert_eq!(pct("47.6"), 0.476);
    assert_eq!(pct("53.30"), 0.533);
    let frac = to_canonical(&parse_number(".338").unwrap(), Scale::Fraction, &mrr).unwrap();
    assert_eq!(frac, 0.338);
}

#[test]
fn a_wrong_scale_shows_up_as_out_of_bounds() {
    let mrr = metric(
        "mrr",
        Scale::Fraction,
        &[Scale::Fraction, Scale::Percent],
        Some(0.0),
        Some(1.0),
    );
    let err = to_canonical(&parse_number("33.8").unwrap(), Scale::Fraction, &mrr).unwrap_err();
    assert!(matches!(err, ValueError::OutOfBounds { .. }), "{err:?}");
}

#[test]
fn unaccepted_scales_are_refused() {
    let mr = metric("mr", Scale::Raw, &[Scale::Raw], Some(1.0), None);
    assert_eq!(
        to_canonical(&parse_number("357").unwrap(), Scale::Raw, &mr).unwrap(),
        357.0
    );
    let err = to_canonical(&parse_number("35.7").unwrap(), Scale::Percent, &mr).unwrap_err();
    assert!(
        matches!(err, ValueError::ScaleNotAccepted { .. }),
        "{err:?}"
    );
    let err = to_canonical(&parse_number("0.5").unwrap(), Scale::Raw, &mr).unwrap_err();
    assert!(matches!(err, ValueError::OutOfBounds { .. }), "{err:?}"); // MR is at least 1
}
