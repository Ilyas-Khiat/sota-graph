use std::fmt;
use std::str::FromStr;

/// An arXiv identifier, optionally pinned to a version.
///
/// New style `2106.06935v4` (YYMM.NNNN or YYMM.NNNNN) and old style
/// `cs/0112017v1` or `math.GT/0309136`. URLs and `abs/` prefixes are refused:
/// a list of papers should hold ids, nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArxivId {
    pub id: String,
    pub version: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("not an arXiv id: {0:?}")]
pub struct ArxivIdError(pub String);

impl ArxivId {
    /// `2106.06935v4`, or `cs_0112017v1`: usable as a file name on every OS.
    pub fn file_stem(&self) -> String {
        self.to_string().replace('/', "_")
    }
}

impl fmt::Display for ArxivId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.version {
            Some(v) => write!(f, "{}v{v}", self.id),
            None => f.write_str(&self.id),
        }
    }
}

impl FromStr for ArxivId {
    type Err = ArxivIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ArxivIdError(s.to_owned());
        let text = s.trim();
        let (base, version) = split_version(text).ok_or_else(err)?;
        if !(is_new_style(base) || is_old_style(base)) {
            return Err(err());
        }
        Ok(Self {
            id: base.to_owned(),
            version,
        })
    }
}

/// `("2106.06935", Some(4))` for `2106.06935v4`. `None` for a malformed suffix
/// such as `v`, `v0` or `v4x`.
fn split_version(text: &str) -> Option<(&str, Option<u32>)> {
    let Some(pos) = text.rfind('v') else {
        return Some((text, None));
    };
    let (base, suffix) = (&text[..pos], &text[pos + 1..]);
    // A 'v' inside an old-style archive name ("solv-int/...") is not a version.
    if !base.ends_with(|c: char| c.is_ascii_digit()) {
        return Some((text, None));
    }
    if suffix.is_empty() || !suffix.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    match suffix.parse::<u32>() {
        Ok(0) | Err(_) => None,
        Ok(v) => Some((base, Some(v))),
    }
}

fn all_digits(s: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit())
}

/// `YYMM.NNNN` from April 2007, `YYMM.NNNNN` from January 2015 on.
fn is_new_style(s: &str) -> bool {
    let Some((yymm, num)) = s.split_once('.') else {
        return false;
    };
    if !all_digits(yymm, 4, 4) {
        return false;
    }
    let (year_month, month) = (yymm.parse::<u32>().unwrap_or(0), &yymm[2..]);
    let month_ok = matches!(month.parse::<u32>(), Ok(1..=12));
    let digits = if year_month >= 1501 { 5 } else { 4 };
    month_ok && year_month >= 704 && all_digits(num, digits, digits)
}

fn is_old_style(s: &str) -> bool {
    let Some((archive, num)) = s.split_once('/') else {
        return false;
    };
    let (name, subject) = archive.split_once('.').unwrap_or((archive, ""));
    let name_ok = !name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b == b'-');
    let subject_ok = subject.is_empty()
        || (subject.len() == 2 && subject.bytes().all(|b| b.is_ascii_uppercase()));
    name_ok && subject_ok && all_digits(num, 7, 7)
}
