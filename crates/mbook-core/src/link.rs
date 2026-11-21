//! Link and picture destinations: the rules of "Resolving destinations" in
//! RFC 0001 that need no table.

use crate::error::ResolveError;
use percent_encoding::percent_decode_str;

/// A destination, taken apart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Parsed {
    /// A destination with a URI scheme the format allows for links.
    External,
    /// A destination inside the book.
    Internal {
        /// The path of the target, resolved against the folder of the chapter
        /// that holds the reference. It is that chapter's own path when the
        /// destination has no path part.
        path: String,
        /// What follows the first `#`, percent-decoded.
        fragment: Option<String>,
    },
}

/// Takes a destination apart.
///
/// `from` is the path of the chapter that holds the reference, and
/// `destination` is taken after the Markdown parser has processed escapes and
/// entities. Whether the resulting path names a resource is for the caller to
/// find out.
pub(crate) fn parse(from: &str, destination: &str) -> Result<Parsed, ResolveError> {
    if let Some(scheme) = scheme(destination) {
        let allowed = ["http", "https", "mailto"]
            .iter()
            .any(|name| scheme.eq_ignore_ascii_case(name));
        return if allowed {
            Ok(Parsed::External)
        } else {
            Err(ResolveError::ForbiddenScheme)
        };
    }
    if destination.contains('?') || destination.starts_with('/') {
        return Err(ResolveError::Malformed);
    }
    let (path, fragment) = match destination.split_once('#') {
        Some((path, fragment)) => (path, Some(percent_decode(fragment)?)),
        None => (destination, None),
    };
    let path = percent_decode(path)?;
    let path = if path.is_empty() {
        from.to_owned()
    } else {
        join(from, &path)?
    };
    Ok(Parsed::Internal { path, fragment })
}

/// The URI scheme a destination starts with: `[A-Za-z][A-Za-z0-9+.-]*` before
/// a colon.
fn scheme(destination: &str) -> Option<&str> {
    let (scheme, _) = destination.split_once(':')?;
    let mut bytes = scheme.bytes();
    let starts_with_a_letter = bytes.next()?.is_ascii_alphabetic();
    let continues = bytes.all(|byte| byte.is_ascii_alphanumeric() || b"+.-".contains(&byte));
    (starts_with_a_letter && continues).then_some(scheme)
}

/// Replaces every `%XX` with the byte it names. A `%` that is not followed by
/// two hexadecimal digits stays as it is.
fn percent_decode(text: &str) -> Result<String, ResolveError> {
    percent_decode_str(text)
        .decode_utf8()
        .map(std::borrow::Cow::into_owned)
        .map_err(|_| ResolveError::Malformed)
}

/// Resolves a relative path against the folder of `from`, applying `.` and
/// `..` segments.
fn join(from: &str, relative: &str) -> Result<String, ResolveError> {
    let mut segments: Vec<&str> = from.split('/').collect();
    // The last segment of `from` is the chapter itself, not a folder.
    segments.pop();
    for segment in relative.split('/') {
        match segment {
            // An empty segment can never be part of a stored path.
            "" => return Err(ResolveError::NotFound),
            "." => {}
            ".." => {
                segments.pop().ok_or(ResolveError::OutsideRoot)?;
            }
            name => segments.push(name),
        }
    }
    Ok(segments.join("/"))
}

#[cfg(test)]
mod tests {
    use super::{Parsed, parse, scheme};
    use crate::error::ResolveError;

    fn internal(path: &str, fragment: Option<&str>) -> Parsed {
        Parsed::Internal {
            path: path.to_owned(),
            fragment: fragment.map(str::to_owned),
        }
    }

    #[test]
    fn a_web_link_is_external() {
        assert_eq!(
            parse("ch01.md", "https://example.org/a?b#c"),
            Ok(Parsed::External)
        );
    }

    #[test]
    fn schemes_are_compared_without_case() {
        assert_eq!(
            parse("ch01.md", "MAILTO:a@example.org"),
            Ok(Parsed::External)
        );
    }

    #[test]
    fn another_scheme_is_forbidden() {
        assert_eq!(
            parse("ch01.md", "javascript:alert(1)"),
            Err(ResolveError::ForbiddenScheme)
        );
    }

    #[test]
    fn a_colon_after_a_slash_is_not_a_scheme() {
        assert_eq!(scheme("img/a:b.avif"), None);
    }

    #[test]
    fn a_scheme_starts_with_a_letter() {
        assert_eq!(scheme("1x:rest"), None);
    }

    #[test]
    fn a_sibling_is_found_next_to_the_chapter() {
        assert_eq!(parse("ch01.md", "ch02.md"), Ok(internal("ch02.md", None)));
    }

    #[test]
    fn a_path_is_resolved_against_the_folder_of_the_chapter() {
        assert_eq!(
            parse("part1/ch01.md", "img/fig.avif"),
            Ok(internal("part1/img/fig.avif", None))
        );
    }

    #[test]
    fn a_parent_segment_climbs_one_folder() {
        assert_eq!(
            parse("part1/ch01.md", "../img/fig.avif"),
            Ok(internal("img/fig.avif", None))
        );
    }

    #[test]
    fn a_current_segment_changes_nothing() {
        assert_eq!(
            parse("part1/ch01.md", "./ch02.md"),
            Ok(internal("part1/ch02.md", None))
        );
    }

    #[test]
    fn climbing_above_the_root_is_an_error() {
        assert_eq!(
            parse("ch01.md", "../ch02.md"),
            Err(ResolveError::OutsideRoot)
        );
    }

    #[test]
    fn a_fragment_alone_stays_in_the_chapter() {
        assert_eq!(
            parse("part1/ch01.md", "#the-station"),
            Ok(internal("part1/ch01.md", Some("the-station")))
        );
    }

    #[test]
    fn the_destination_is_split_at_the_first_hash() {
        assert_eq!(parse("a.md", "b.md#x#y"), Ok(internal("b.md", Some("x#y"))));
    }

    #[test]
    fn an_empty_fragment_is_kept_as_empty() {
        assert_eq!(parse("a.md", "b.md#"), Ok(internal("b.md", Some(""))));
    }

    #[test]
    fn path_and_fragment_are_percent_decoded() {
        assert_eq!(
            parse("a.md", "my%20chapter.md#caf%C3%A9"),
            Ok(internal("my chapter.md", Some("café")))
        );
    }

    #[test]
    fn an_escaped_question_mark_is_part_of_the_path() {
        assert_eq!(parse("a.md", "why%3F.md"), Ok(internal("why?.md", None)));
    }

    #[test]
    fn a_percent_sign_without_two_digits_is_kept() {
        assert_eq!(parse("a.md", "100%.md"), Ok(internal("100%.md", None)));
    }

    #[test]
    fn bytes_that_are_not_utf8_are_refused() {
        assert_eq!(parse("a.md", "%FF.md"), Err(ResolveError::Malformed));
    }

    #[test]
    fn a_query_is_refused() {
        assert_eq!(parse("a.md", "b.md?x=1"), Err(ResolveError::Malformed));
    }

    #[test]
    fn an_absolute_path_is_refused() {
        assert_eq!(parse("a.md", "/b.md"), Err(ResolveError::Malformed));
    }

    #[test]
    fn a_doubled_slash_names_nothing() {
        assert_eq!(parse("a.md", "img//b.avif"), Err(ResolveError::NotFound));
    }
}
