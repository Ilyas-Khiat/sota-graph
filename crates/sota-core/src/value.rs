//! Reading a number out of a table cell, and putting it on a metric's
//! canonical scale. Deliberately strict: anything it cannot read with
//! certainty is an error, and errors go to review instead of the graph.

use crate::{Metric, Scale};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParsedValue {
    pub value: f64,
    /// A standard deviation printed with the value (`0.338 ± 0.002`).
    pub std: Option<f64>,
    /// Digits after the decimal point, as printed.
    pub decimals: u32,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ValueError {
    #[error("missing value")]
    Missing,
    #[error("no number in {0:?}")]
    NoNumber(String),
    #[error("{0:?} holds several numbers")]
    Ambiguous(String),
    #[error("{0:?} is written in a way this parser does not guess at")]
    Unreadable(String),
    #[error("metric {metric} does not accept the {scale:?} scale")]
    ScaleNotAccepted { metric: String, scale: Scale },
    #[error("{value} is outside the bounds of {metric}")]
    OutOfBounds { metric: String, value: f64 },
}

/// Cell contents that mean "no result here".
const MISSING: &[&str] = &[
    "", "-", "--", "\u{2013}", "\u{2014}", "\u{2212}", "n/a", "na", "/", "?",
];

pub fn is_missing(text: &str) -> bool {
    MISSING.contains(&text.trim().to_lowercase().as_str())
}

pub fn parse_number(text: &str) -> Result<ParsedValue, ValueError> {
    if is_missing(text) {
        return Err(ValueError::Missing);
    }
    let cleaned = strip_superscripts(&text.replace('\u{2212}', "-").replace("\\pm", " \u{00B1} "));
    let chars: Vec<char> = cleaned.chars().collect();

    let mut tokens: Vec<(usize, Token)> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match read_token(&chars, i) {
            Some(Ok((token, end))) => {
                tokens.push((i, token));
                i = end;
            }
            Some(Err(())) => return Err(ValueError::Unreadable(text.to_owned())),
            None => i += 1,
        }
    }

    match tokens.as_slice() {
        [] => Err(ValueError::NoNumber(text.to_owned())),
        [(_, v)] => Ok(ParsedValue {
            value: v.value,
            std: None,
            decimals: v.decimals,
        }),
        [(_, v), (start, s)] if chars[..*start].contains(&'\u{00B1}') => Ok(ParsedValue {
            value: v.value,
            std: Some(s.value),
            decimals: v.decimals,
        }),
        _ => Err(ValueError::Ambiguous(text.to_owned())),
    }
}

/// The value on `metric`'s canonical scale, given the scale it was printed in.
/// Rounded to the printed precision, so 33.8 % is exactly 0.338.
pub fn to_canonical(
    parsed: &ParsedValue,
    scale: Scale,
    metric: &Metric,
) -> Result<f64, ValueError> {
    let refused = || ValueError::ScaleNotAccepted {
        metric: metric.id.clone(),
        scale,
    };
    if !metric.accepted_scales.contains(&scale) {
        return Err(refused());
    }
    let value = match (scale, metric.canonical_scale) {
        (from, to) if from == to => parsed.value,
        (Scale::Percent, Scale::Fraction) => round(parsed.value / 100.0, parsed.decimals + 2),
        (Scale::Fraction, Scale::Percent) => {
            round(parsed.value * 100.0, parsed.decimals.saturating_sub(2))
        }
        _ => return Err(refused()),
    };
    let below = metric.min.is_some_and(|min| value < min);
    let above = metric.max.is_some_and(|max| value > max);
    if below || above {
        return Err(ValueError::OutOfBounds {
            metric: metric.id.clone(),
            value,
        });
    }
    Ok(value)
}

struct Token {
    value: f64,
    decimals: u32,
}

/// A number starting at `i`: `-`? digits (`,ddd`)* (`.` digits)? or `.` digits.
/// `None` when no number starts here; `Err` for a shape we refuse to guess at
/// (a decimal comma such as `0,338` or `1,23`).
fn read_token(chars: &[char], i: usize) -> Option<Result<(Token, usize), ()>> {
    let digit = |k: usize| chars.get(k).is_some_and(char::is_ascii_digit);
    let starts_number = |k: usize| digit(k) || (chars.get(k) == Some(&'.') && digit(k + 1));
    let after_letter = i > 0 && chars[i - 1].is_ascii_alphabetic();

    let mut k = i;
    let mut text = String::new();
    if chars[i] == '-' && starts_number(i + 1) && !after_letter {
        text.push('-');
        k += 1;
    } else if !starts_number(i) || (digit(i) && after_letter) {
        return None;
    }

    let int_start = k;
    while digit(k) {
        text.push(chars[k]);
        k += 1;
    }
    let int_digits = k - int_start;
    while chars.get(k) == Some(&',') && digit(k + 1) {
        let group = (k + 1..).take_while(|&j| digit(j)).count();
        let leading_ok = (1..=3).contains(&int_digits) && chars[int_start] != '0';
        if group != 3 || !leading_ok {
            return Some(Err(()));
        }
        text.extend(&chars[k + 1..k + 4]);
        k += 4;
    }
    let mut decimals = 0;
    if chars.get(k) == Some(&'.') && digit(k + 1) {
        text.push('.');
        k += 1;
        while digit(k) {
            text.push(chars[k]);
            k += 1;
            decimals += 1;
        }
    }
    let value = text.parse::<f64>().ok()?;
    Some(Ok((Token { value, decimals }, k)))
}

/// Drops `^{...}` (balanced) and `^x`: footnote marks, never values.
fn strip_superscripts(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '^' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        if chars.get(i + 1) == Some(&'{') {
            let mut depth = 0;
            i += 1;
            while i < chars.len() {
                match chars[i] {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
        } else {
            i += 2;
        }
    }
    out
}

fn round(x: f64, decimals: u32) -> f64 {
    let factor = 10f64.powi(decimals as i32);
    (x * factor).round() / factor
}
