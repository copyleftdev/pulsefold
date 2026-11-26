//! Terminal-marker scoring, ported from Canary Lab's `lab/marker.py::score_marker`.
//!
//! It reads only the response text and the token; it never sees scheduler state.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marker {
    Exact,
    Absent,
    Malformed,
    /// No response was captured. This is not the same as a missing marker.
    Unobserved,
}

impl Marker {
    pub fn as_str(self) -> &'static str {
        match self {
            Marker::Exact => "EXACT",
            Marker::Absent => "ABSENT",
            Marker::Malformed => "MALFORMED",
            Marker::Unobserved => "UNOBSERVED",
        }
    }

    pub fn noncompliant(self) -> bool {
        matches!(self, Marker::Absent | Marker::Malformed)
    }
}

pub fn valid_token(token: &str) -> bool {
    token.len() == 16 && token.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

pub fn score(response: Option<&str>, token: &str) -> Marker {
    let Some(response) = response else {
        return Marker::Unobserved;
    };
    let normalized = response.replace("\r\n", "\n").replace('\r', "\n");
    let first = normalized
        .split('\n')
        .find(|line| !line.trim_matches(|c| c == ' ' || c == '\t').is_empty())
        .unwrap_or("");
    if first == format!("CANARY:{token}") {
        Marker::Exact
    } else if normalized.contains(token) {
        Marker::Malformed
    } else {
        Marker::Absent
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const T: &str = "0123456789abcdef";

    #[test]
    fn matches_canary_lab_rules() {
        assert_eq!(score(Some("CANARY:0123456789abcdef\nDone."), T), Marker::Exact);
        assert_eq!(score(Some("\n \t\r\nCANARY:0123456789abcdef"), T), Marker::Exact, "blank lines first");
        assert_eq!(score(Some("CANARY:0123456789abcdef\r\nDone."), T), Marker::Exact);
        assert_eq!(score(Some(" CANARY:0123456789abcdef"), T), Marker::Malformed, "indented line is not exact");
        assert_eq!(score(Some("Done.\nCANARY:0123456789abcdef"), T), Marker::Malformed, "marker not first");
        assert_eq!(score(Some("CANARY:0123456789abcdef!"), T), Marker::Malformed);
        assert_eq!(score(Some("Done."), T), Marker::Absent);
        assert_eq!(score(Some(""), T), Marker::Absent);
        assert_eq!(score(None, T), Marker::Unobserved);
    }

    #[test]
    fn token_shape() {
        assert!(valid_token(T));
        assert!(!valid_token("0123456789ABCDEF"));
        assert!(!valid_token("0123456789abcde"));
    }
}
