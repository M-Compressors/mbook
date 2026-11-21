//! The text encoding rules of a chapter.

use crate::error::TextError;

const BYTE_ORDER_MARK: char = '\u{feff}';

/// Brings chapter text to its stored form, or says why it cannot be stored.
///
/// A leading byte order mark is dropped, and CRLF and lone CR become a line
/// feed. Any other character below U+0020, apart from line feed and tab, is
/// an error. Text that is already in stored form is returned without copying.
pub(crate) fn normalize(text: String) -> Result<String, TextError> {
    let text = if text.starts_with(BYTE_ORDER_MARK) || text.contains('\r') {
        rewrite(&text)
    } else {
        text
    };
    let offence = text
        .bytes()
        .position(|byte| byte < 0x20 && byte != b'\n' && byte != b'\t');
    match offence {
        Some(offset) => Err(TextError {
            offset,
            character: char::from(text.as_bytes()[offset]),
        }),
        None => Ok(text),
    }
}

fn rewrite(text: &str) -> String {
    let mut rest = text.strip_prefix(BYTE_ORDER_MARK).unwrap_or(text);
    let mut out = String::with_capacity(rest.len());
    while let Some(at) = rest.find('\r') {
        out.push_str(&rest[..at]);
        out.push('\n');
        rest = &rest[at + 1..];
        rest = rest.strip_prefix('\n').unwrap_or(rest);
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::normalize;
    use crate::error::TextError;

    fn normalized(text: &str) -> Result<String, TextError> {
        normalize(text.to_owned())
    }

    #[test]
    fn normalize_keeps_stored_form_unchanged() {
        assert_eq!(
            normalized("# One\n\n\ttext\n").as_deref(),
            Ok("# One\n\n\ttext\n")
        );
    }

    #[test]
    fn normalize_turns_crlf_into_a_line_feed() {
        assert_eq!(normalized("a\r\nb\r\n").as_deref(), Ok("a\nb\n"));
    }

    #[test]
    fn normalize_turns_a_lone_cr_into_a_line_feed() {
        assert_eq!(normalized("a\rb").as_deref(), Ok("a\nb"));
    }

    #[test]
    fn normalize_keeps_two_line_ends_for_cr_cr_lf() {
        assert_eq!(normalized("a\r\r\nb").as_deref(), Ok("a\n\nb"));
    }

    #[test]
    fn normalize_drops_a_leading_byte_order_mark() {
        assert_eq!(normalized("\u{feff}# One\n").as_deref(), Ok("# One\n"));
    }

    #[test]
    fn normalize_keeps_a_byte_order_mark_inside_the_text() {
        assert_eq!(normalized("a\u{feff}b").as_deref(), Ok("a\u{feff}b"));
    }

    #[test]
    fn normalize_rejects_a_control_character() {
        let error = normalized("ab\u{1b}c").unwrap_err();
        assert_eq!((error.offset, error.character), (2, '\u{1b}'));
    }

    #[test]
    fn normalize_reports_the_offset_in_the_stored_form() {
        let error = normalized("\u{feff}a\r\n\0").unwrap_err();
        assert_eq!((error.offset, error.character), (2, '\0'));
    }
}
