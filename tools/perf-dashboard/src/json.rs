//! Just enough JSON to write a report.
//!
//! The document has a fixed shape and every string in it is a program name, a
//! section name, or an error message, so a serializer is one escaping function and
//! a number formatter. That keeps this tool free of dependencies, which is what the
//! rest of the repository is.

/// Escapes a string's contents for a JSON string literal.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if (control as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", control as u32)),
            other => out.push(other),
        }
    }
    out
}

/// A JSON string literal, quotes included.
pub fn quote(text: &str) -> String {
    format!("\"{}\"", escape(text))
}

/// A JSON number with a fixed number of decimal places.
///
/// A value that is not finite becomes `null` rather than `Infinity`, which is not
/// JSON: a ratio against a zero CPython measurement is not a number, and the
/// dashboard draws that as "no measurement" instead of refusing the document.
pub fn number(value: f64, places: usize) -> String {
    if value.is_finite() {
        format!("{value:.places$}")
    } else {
        "null".to_string()
    }
}

/// A JSON boolean.
pub fn boolean(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        "false"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_the_characters_json_reserves() {
        assert_eq!(quote("a\"b\\c"), "\"a\\\"b\\\\c\"");
        assert_eq!(quote("one\ntwo\tthree"), "\"one\\ntwo\\tthree\"");
        assert_eq!(quote("\u{1}"), "\"\\u0001\"");
        // Non-ASCII is left alone, which is what JSON allows.
        assert_eq!(quote("naïve"), "\"naïve\"");
    }

    #[test]
    fn a_non_finite_number_is_null() {
        assert_eq!(number(1.5, 3), "1.500");
        assert_eq!(number(f64::INFINITY, 2), "null");
        assert_eq!(number(f64::NAN, 2), "null");
    }
}
