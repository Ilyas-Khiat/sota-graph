//! Shapes allowed for names that end up in Cypher or in ids.
//!
//! Labels, relationship types and property names cannot be Cypher parameters,
//! so they are interpolated into queries. Restricting them to plain ASCII
//! identifiers here is what makes that safe.

fn shaped(s: &str, first: impl Fn(char) -> bool, rest: impl Fn(char) -> bool) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(first) && chars.all(rest)
}

/// `Paper`, `Dataset`: PascalCase ASCII.
pub(crate) fn is_label(s: &str) -> bool {
    shaped(s, |c| c.is_ascii_uppercase(), |c| c.is_ascii_alphanumeric())
}

/// `BUILDS_ON`: upper snake case.
pub(crate) fn is_relationship_type(s: &str) -> bool {
    shaped(
        s,
        |c| c.is_ascii_uppercase(),
        |c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_',
    )
}

/// `raw_text`: lower snake case.
pub(crate) fn is_property_name(s: &str) -> bool {
    shaped(
        s,
        |c| c.is_ascii_lowercase(),
        |c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_',
    )
}

/// `fb15k-237`, `hits@10`: lowercase slug, stable across renames of the display name.
pub(crate) fn is_registry_id(s: &str) -> bool {
    let body = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    shaped(s, body, |c| body(c) || "@._-".contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes() {
        assert!(is_label("Paper") && !is_label("paper") && !is_label("Pa per"));
        assert!(!is_label("Method`) DETACH DELETE n //"));
        assert!(is_relationship_type("BUILDS_ON") && !is_relationship_type("builds_on"));
        assert!(is_property_name("raw_text") && !is_property_name("rawText"));
        assert!(is_registry_id("hits@10") && is_registry_id("fb15k-237"));
        assert!(!is_registry_id("FB15k") && !is_registry_id("-x") && !is_registry_id(""));
    }
}
