//! The path rules of the resource table.

use crate::error::PathError;

/// Checks a path against the rules RFC 0001 sets for a text or picture
/// resource: segments joined by `/`, none of them empty, `.` or `..`, and no
/// backslash or control character anywhere.
#[cfg_attr(
    not(any(feature = "write", test)),
    expect(dead_code, reason = "the reader calls it once it decodes tables")
)]
pub(crate) fn validate(path: &str) -> Result<(), PathError> {
    if path.is_empty() {
        return Err(PathError::Empty);
    }
    if path.contains('\\') {
        return Err(PathError::Backslash);
    }
    if path.chars().any(|c| c < '\u{20}') {
        return Err(PathError::ControlCharacter);
    }
    for segment in path.split('/') {
        match segment {
            "" => return Err(PathError::EmptySegment),
            "." | ".." => return Err(PathError::DotSegment),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate;
    use crate::error::PathError;

    #[test]
    fn validate_accepts_a_nested_path() {
        assert_eq!(validate("img/part-1/fig_3.avif"), Ok(()));
    }

    #[test]
    fn validate_accepts_names_outside_ascii() {
        assert_eq!(validate("главы/первая.md"), Ok(()));
    }

    #[test]
    fn validate_rejects_an_empty_path() {
        assert_eq!(validate(""), Err(PathError::Empty));
    }

    #[test]
    fn validate_rejects_a_leading_slash() {
        assert_eq!(validate("/ch01.md"), Err(PathError::EmptySegment));
    }

    #[test]
    fn validate_rejects_a_trailing_slash() {
        assert_eq!(validate("img/"), Err(PathError::EmptySegment));
    }

    #[test]
    fn validate_rejects_a_doubled_slash() {
        assert_eq!(validate("img//a.avif"), Err(PathError::EmptySegment));
    }

    #[test]
    fn validate_rejects_a_parent_segment() {
        assert_eq!(validate("img/../ch01.md"), Err(PathError::DotSegment));
    }

    #[test]
    fn validate_rejects_a_current_segment() {
        assert_eq!(validate("./ch01.md"), Err(PathError::DotSegment));
    }

    #[test]
    fn validate_accepts_dots_inside_a_segment() {
        assert_eq!(validate("..hidden/a..b.md"), Ok(()));
    }

    #[test]
    fn validate_rejects_a_backslash() {
        assert_eq!(validate("img\\a.avif"), Err(PathError::Backslash));
    }

    #[test]
    fn validate_rejects_a_control_character() {
        assert_eq!(validate("ch\t01.md"), Err(PathError::ControlCharacter));
    }
}
