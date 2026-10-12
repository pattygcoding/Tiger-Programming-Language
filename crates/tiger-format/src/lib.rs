//! Go-compatible value formatting for the Tiger runtime.
//!
//! The Go implementation prints numbers with `strconv.FormatFloat(x, 'f', -1, 64)`
//! (shortest round-tripping digits in *fixed* notation) and quotes strings with
//! `strconv.Quote`. Rust's default float formatting switches to exponent notation
//! and its `{:?}` string escaping differs, so this crate reimplements the Go
//! behaviour exactly. Every function here is checked against captured Go output
//! in the unit tests.

/// Formats a finite float exactly like Go's `strconv.FormatFloat(x, 'f', -1, 64)`.
pub fn number(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 { "+Inf" } else { "-Inf" }.to_string();
    }
    let mut out = String::new();
    let mut magnitude = value;
    if magnitude.is_sign_negative() {
        out.push('-');
        magnitude = -magnitude;
    }
    if magnitude == 0.0 {
        out.push('0');
        return out;
    }
    // Integral values below 2^53 are exactly representable, so print them
    // directly and skip the scientific-notation round trip below.
    if magnitude.fract() == 0.0 && magnitude < 9_007_199_254_740_992.0 {
        out.push_str(&(magnitude as i64).to_string());
        return out;
    }
    // Rust's `{:e}` produces the shortest round-tripping mantissa, matching Go's
    // shortest-mode digit selection. Re-expand it into fixed notation.
    let scientific = format!("{:e}", magnitude);
    let (mantissa, exponent) = scientific.split_once('e').expect("exponent marker");
    let exponent: i32 = exponent.parse().expect("integer exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let length = digits.len() as i32;
    let point = exponent + 1;
    if point <= 0 {
        out.push_str("0.");
        for _ in 0..(-point) {
            out.push('0');
        }
        out.push_str(&digits);
    } else if point >= length {
        out.push_str(&digits);
        for _ in 0..(point - length) {
            out.push('0');
        }
    } else {
        out.push_str(&digits[..point as usize]);
        out.push('.');
        out.push_str(&digits[point as usize..]);
    }
    out
}

/// Formats a float exactly like Go's `strconv.FormatFloat(x, 'f', precision, 64)`.
pub fn number_precision(value: f64, precision: usize) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 { "+Inf" } else { "-Inf" }.to_string();
    }
    format!("{:.*}", precision, value)
}

/// Formats a float like Go's `%v`/`%g` shortest formatting (used only in runtime
/// diagnostics). Switches to exponent notation for exponents below -4 or at or
/// above 6, matching `fmt`'s default float verb.
pub fn number_g(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 { "+Inf" } else { "-Inf" }.to_string();
    }
    if value == 0.0 {
        return if value.is_sign_negative() { "-0".to_string() } else { "0".to_string() };
    }
    let negative = value.is_sign_negative();
    let magnitude = value.abs();
    let scientific = format!("{:e}", magnitude);
    let (mantissa, exponent) = scientific.split_once('e').expect("exponent marker");
    let exponent: i32 = exponent.parse().expect("integer exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let mut body;
    if exponent < -4 || exponent >= 6 {
        body = String::new();
        body.push_str(&digits[..1]);
        if digits.len() > 1 {
            body.push('.');
            body.push_str(&digits[1..]);
        }
        body.push('e');
        body.push(if exponent < 0 { '-' } else { '+' });
        let absolute = exponent.abs();
        if absolute < 10 {
            body.push('0');
        }
        body.push_str(&absolute.to_string());
    } else {
        body = number(magnitude);
    }
    if negative {
        body.insert(0, '-');
    }
    body
}

/// Reports whether Go's `unicode.IsPrint` would consider `ch` printable: the
/// letter, mark, number, punctuation, symbol, and space-separator categories.
pub fn is_print(ch: char) -> bool {
    // Cc (control), including C1 controls.
    if ch.is_control() {
        return false;
    }
    // Zl and Zp line/paragraph separators.
    if ch == '\u{2028}' || ch == '\u{2029}' {
        return false;
    }
    // Cf format characters.
    if is_format(ch) {
        return false;
    }
    // Co private-use areas.
    if matches!(ch, '\u{E000}'..='\u{F8FF}') || matches!(ch, '\u{F0000}'..='\u{FFFFD}') || matches!(ch, '\u{100000}'..='\u{10FFFD}') {
        return false;
    }
    true
}

fn is_format(ch: char) -> bool {
    matches!(ch,
        '\u{00AD}'
        | '\u{0600}'..='\u{0605}'
        | '\u{061C}'
        | '\u{06DD}'
        | '\u{070F}'
        | '\u{0890}'..='\u{0891}'
        | '\u{08E2}'
        | '\u{180E}'
        | '\u{200B}'..='\u{200F}'
        | '\u{202A}'..='\u{202E}'
        | '\u{2060}'..='\u{2064}'
        | '\u{2066}'..='\u{206F}'
        | '\u{FEFF}'
        | '\u{FFF9}'..='\u{FFFB}'
        | '\u{110BD}'
        | '\u{110CD}'
        | '\u{13430}'..='\u{1343F}'
        | '\u{1BCA0}'..='\u{1BCA3}'
        | '\u{1D173}'..='\u{1D17A}'
        | '\u{E0001}'
        | '\u{E0020}'..='\u{E007F}'
    )
}

fn push_escaped_rune(out: &mut String, ch: char) {
    match ch {
        '\u{7}' => out.push_str("\\a"),
        '\u{8}' => out.push_str("\\b"),
        '\u{C}' => out.push_str("\\f"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        '\u{B}' => out.push_str("\\v"),
        _ => {
            let code = ch as u32;
            if code < 0x20 || code == 0x7F {
                out.push_str(&format!("\\x{:02x}", code));
            } else if code < 0x10000 {
                out.push_str(&format!("\\u{:04x}", code));
            } else {
                out.push_str(&format!("\\U{:08x}", code));
            }
        }
    }
}

/// Quotes a string exactly like Go's `strconv.Quote`.
pub fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            _ if is_print(ch) => out.push(ch),
            _ => push_escaped_rune(&mut out, ch),
        }
    }
    out.push('"');
    out
}

/// Quotes a single character exactly like Go's `strconv.QuoteRune`.
pub fn quote_rune(ch: char) -> String {
    let mut out = String::new();
    out.push('\'');
    match ch {
        '\'' => out.push_str("\\'"),
        '\\' => out.push_str("\\\\"),
        _ if is_print(ch) => out.push(ch),
        _ => push_escaped_rune(&mut out, ch),
    }
    out.push('\'');
    out
}

/// One argument to [`sprintf`], mirroring the Go values the runtime formats.
#[derive(Debug, Clone, Copy)]
pub enum Arg<'a> {
    Str(&'a str),
    Int(i64),
    Float(f64),
    Bool(bool),
    Rune(char),
    Error(&'a str),
}

/// A minimal `Sprintf` covering the verbs the runtime uses: `%s %d %q %c %v %t %%`.
pub fn sprintf(template: &str, args: &[Arg]) -> String {
    let mut out = String::new();
    let mut index = 0usize;
    let bytes = template.as_bytes();
    let mut position = 0usize;
    while position < bytes.len() {
        if bytes[position] != b'%' {
            let ch = template[position..].chars().next().unwrap();
            out.push(ch);
            position += ch.len_utf8();
            continue;
        }
        if position + 1 >= bytes.len() {
            out.push_str("%!(NOVERB)");
            break;
        }
        position += 1;
        let verb = bytes[position] as char;
        position += 1;
        if verb == '%' {
            out.push('%');
            continue;
        }
        if index >= args.len() {
            out.push_str(&format!("%!{}(MISSING)", verb));
            continue;
        }
        let argument = args[index];
        index += 1;
        out.push_str(&verb_text(verb, argument));
    }
    out
}

fn verb_text(verb: char, argument: Arg) -> String {
    match verb {
        's' | 'v' => text(argument),
        'q' => quoted(argument),
        'd' => decimal(argument),
        'c' => character(argument),
        't' => boolean(argument),
        _ => format!("%!{}({})", verb, default_text(argument)),
    }
}

fn text(argument: Arg) -> String {
    match argument {
        Arg::Str(value) | Arg::Error(value) => value.to_string(),
        other => default_text(other),
    }
}

fn quoted(argument: Arg) -> String {
    match argument {
        Arg::Str(value) | Arg::Error(value) => quote(value),
        Arg::Rune(value) => quote_rune(value),
        other => quote(&default_text(other)),
    }
}

fn decimal(argument: Arg) -> String {
    match argument {
        Arg::Int(value) => value.to_string(),
        Arg::Bool(value) => value.to_string(),
        other => default_text(other),
    }
}

fn character(argument: Arg) -> String {
    match argument {
        Arg::Rune(value) => value.to_string(),
        Arg::Int(value) => char::from_u32(value as u32).map(String::from).unwrap_or_else(|| format!("%!c({})", value)),
        other => format!("%!c({})", default_text(other)),
    }
}

fn boolean(argument: Arg) -> String {
    match argument {
        Arg::Bool(value) => value.to_string(),
        other => format!("%!t({})", default_text(other)),
    }
}

fn default_text(argument: Arg) -> String {
    match argument {
        Arg::Str(value) | Arg::Error(value) => value.to_string(),
        Arg::Int(value) => value.to_string(),
        Arg::Bool(value) => value.to_string(),
        Arg::Float(value) => number_g(value),
        Arg::Rune(value) => (value as u32).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_matches_go() {
        let cases: &[(f64, &str)] = &[
            (0.0, "0"),
            (-0.0, "-0"),
            (1.0, "1"),
            (-1.0, "-1"),
            (5.0, "5"),
            (3.5, "3.5"),
            (0.1, "0.1"),
            (0.5, "0.5"),
            (-0.5, "-0.5"),
            (1.0 / 3.0, "0.3333333333333333"),
            (1234.5, "1234.5"),
            (1234.5678, "1234.5678"),
            (2.675, "2.675"),
            (0.125, "0.125"),
            (1e6, "1000000"),
            (1e21, "1000000000000000000000"),
            (1e-7, "0.0000001"),
            (2.0 / 3.0, "0.6666666666666666"),
            (9007199254740991.0, "9007199254740991"),
            (1e15, "1000000000000000"),
            (1e16, "10000000000000000"),
            (1e17, "100000000000000000"),
            (1e20, "100000000000000000000"),
            (1e22, "10000000000000000000000"),
            (77.97, "77.97"),
            (19.99, "19.99"),
            (12.5, "12.5"),
            (100.0, "100"),
            (0.30000000000000004, "0.30000000000000004"),
            (1e-5, "0.00001"),
            (1e-4, "0.0001"),
        ];
        for (value, want) in cases {
            assert_eq!(number(*value), *want, "number({value})");
        }
        assert_eq!(number(f64::NAN), "NaN");
        assert_eq!(number(f64::INFINITY), "+Inf");
        assert_eq!(number(f64::NEG_INFINITY), "-Inf");
    }

    #[test]
    fn precision_matches_go() {
        let cases: &[(f64, usize, &str)] = &[
            (3.5, 0, "4"),
            (-7.0, 0, "-7"),
            (255.0, 0, "255"),
            (1.0 / 3.0, 0, "0"),
            (1234.5678, 0, "1235"),
            (0.5, 0, "0"),
            (-0.5, 0, "-0"),
            (2.675, 0, "3"),
            (0.125, 0, "0"),
            (2.5, 0, "2"),
            (3.5, 0, "4"),
            (-2.5, 0, "-2"),
            (3.5, 1, "3.5"),
            (-7.0, 1, "-7.0"),
            (255.0, 1, "255.0"),
            (1.0 / 3.0, 1, "0.3"),
            (1234.5678, 1, "1234.6"),
            (0.5, 1, "0.5"),
            (-0.5, 1, "-0.5"),
            (2.675, 1, "2.7"),
            (0.125, 1, "0.1"),
            (2.5, 1, "2.5"),
            (-2.5, 1, "-2.5"),
            (3.5, 2, "3.50"),
            (-7.0, 2, "-7.00"),
            (255.0, 2, "255.00"),
            (1.0 / 3.0, 2, "0.33"),
            (1234.5678, 2, "1234.57"),
            (2.675, 2, "2.67"),
            (0.125, 2, "0.12"),
            (1e6, 2, "1000000.00"),
            (3.5, 4, "3.5000"),
            (1.0 / 3.0, 4, "0.3333"),
            (1234.5678, 4, "1234.5678"),
            (2.675, 4, "2.6750"),
        ];
        for (value, precision, want) in cases {
            assert_eq!(number_precision(*value, *precision), *want, "number_precision({value}, {precision})");
        }
    }

    #[test]
    fn g_matches_go() {
        let cases: &[(f64, &str)] = &[
            (3.5, "3.5"),
            (123456.0, "123456"),
            (1234567.0, "1.234567e+06"),
            (1e6, "1e+06"),
            (1e20, "1e+20"),
            (1e21, "1e+21"),
            (1e-4, "0.0001"),
            (1e-5, "1e-05"),
            (1.5, "1.5"),
            (100.0, "100"),
        ];
        for (value, want) in cases {
            assert_eq!(number_g(*value), *want, "number_g({value})");
        }
    }

    #[test]
    fn quote_matches_go() {
        let cases: &[(&str, &str)] = &[
            ("hi\n", "\"hi\\n\""),
            ("a\"b", "\"a\\\"b\""),
            ("back\\slash", "\"back\\\\slash\""),
            ("tab\there", "\"tab\\there\""),
            ("\x00\x01\x7f", "\"\\x00\\x01\\x7f\""),
            ("é中😀!", "\"é中😀!\""),
            ("\u{200d}", "\"\\u200d\""),
            ("a\u{0301}", "\"a\u{0301}\""),
            ("\u{00ad}", "\"\\u00ad\""),
        ];
        for (input, want) in cases {
            assert_eq!(quote(input), *want, "quote({input:?})");
        }
    }

    #[test]
    fn quote_rune_matches_go() {
        let cases: &[(char, &str)] = &[
            ('A', "'A'"),
            ('x', "'x'"),
            ('é', "'é'"),
            ('\u{1F600}', "'😀'"),
            ('\n', "'\\n'"),
            ('\'', "'\\''"),
            ('\\', "'\\\\'"),
            ('\u{200d}', "'\\u200d'"),
        ];
        for (input, want) in cases {
            assert_eq!(quote_rune(*input), *want, "quote_rune({input:?})");
        }
    }

    #[test]
    fn sprintf_matches_go() {
        assert_eq!(sprintf("%s-%d-%q", &[Arg::Str("a"), Arg::Int(2), Arg::Str("b")]), "a-2-\"b\"");
        assert_eq!(sprintf("100%%", &[]), "100%");
        assert_eq!(sprintf("%d%%", &[Arg::Int(5)]), "5%");
        assert_eq!(sprintf("%q", &[Arg::Rune('A')]), "'A'");
        assert_eq!(sprintf("%q", &[Arg::Rune('\u{1F600}')]), "'😀'");
        assert_eq!(sprintf("%c", &[Arg::Rune('x')]), "x");
        assert_eq!(sprintf("%c", &[Arg::Int(65)]), "A");
        assert_eq!(sprintf("%v", &[Arg::Float(3.5)]), "3.5");
        assert_eq!(sprintf("%v", &[Arg::Bool(true)]), "true");
        assert_eq!(sprintf("%d", &[]), "%!d(MISSING)");
    }
}
