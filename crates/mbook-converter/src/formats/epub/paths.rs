//! Addresses inside an EPUB: how the `href` of a link, a picture or an entry
//! of the contents leads to a file of the archive.

use percent_encoding::percent_decode_str;

/// Where an address leads: a file of the archive and, perhaps, an element.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Reference {
    /// The path of the file inside the archive, without a leading `/`.
    pub(super) path: String,
    /// The `id` of an element of the file, percent-decoded.
    pub(super) fragment: Option<String>,
}

/// Tells whether an address starts with a URI scheme such as `https:`.
pub(super) fn has_scheme(address: &str) -> bool {
    let Some((scheme, _)) = address.split_once(':') else {
        return false;
    };
    let mut bytes = scheme.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || b"+.-".contains(&byte))
}

/// Resolves an address written in the file `document` of the archive.
///
/// A relative address is taken from the folder of `document`, an address
/// that starts with `/` from the root of the archive. `.` and `..` are
/// applied, the path is percent-decoded, and the query is dropped. An
/// address without a path leads to `document` itself. `None` for an address
/// with a scheme, for one that starts with `//`, for one that leaves the
/// archive and for a path that is not UTF-8.
pub(super) fn resolve(document: &str, address: &str) -> Option<Reference> {
    let address = address.trim();
    if has_scheme(address) || address.starts_with("//") {
        return None;
    }
    let (path, fragment) = match address.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (address, None),
    };
    let path = path.split_once('?').map_or(path, |(path, _)| path);
    let path = percent_decode_str(path).decode_utf8().ok()?;
    let fragment = fragment
        .map(|fragment| {
            percent_decode_str(fragment)
                .decode_utf8_lossy()
                .into_owned()
        })
        .filter(|fragment| !fragment.is_empty());
    if path.is_empty() {
        return Some(Reference {
            path: document.to_owned(),
            fragment,
        });
    }
    let folder = if path.starts_with('/') {
        ""
    } else {
        document.rsplit_once('/').map_or("", |(folder, _)| folder)
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in folder.split('/').chain(path.split('/')) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    (!parts.is_empty()).then(|| Reference {
        path: parts.join("/"),
        fragment,
    })
}

/// The name of a section for a file of the archive: its path, without the
/// characters that a name must not have.
pub(super) fn section_name(path: &str) -> String {
    path.replace(['#', ':'], "_")
}

#[cfg(test)]
mod tests {
    use super::{Reference, has_scheme, resolve, section_name};

    fn reference(path: &str, fragment: Option<&str>) -> Reference {
        Reference {
            path: path.to_owned(),
            fragment: fragment.map(str::to_owned),
        }
    }

    #[test]
    fn a_scheme_starts_with_a_letter_and_ends_at_a_colon() {
        assert!(has_scheme("https://example.org"));
        assert!(has_scheme("mailto:a@b.c"));
        assert!(has_scheme("x-y+z.1:rest"));
        assert!(!has_scheme("ch1.xhtml"));
        assert!(!has_scheme("1http://x"));
        assert!(!has_scheme("dir/a:b.xhtml"));
        assert!(!has_scheme(":x"));
    }

    #[test]
    fn a_relative_address_is_taken_from_the_folder_of_its_document() {
        assert_eq!(
            resolve("OEBPS/text/ch1.xhtml", "ch2.xhtml#p3"),
            Some(reference("OEBPS/text/ch2.xhtml", Some("p3")))
        );
        assert_eq!(
            resolve("OEBPS/text/ch1.xhtml", "../images/a%20b.png"),
            Some(reference("OEBPS/images/a b.png", None))
        );
        assert_eq!(
            resolve("OEBPS/text/ch1.xhtml", "./sub/../ch2.xhtml?x=1"),
            Some(reference("OEBPS/text/ch2.xhtml", None))
        );
        assert_eq!(
            resolve("ch1.xhtml", "ch2.xhtml"),
            Some(reference("ch2.xhtml", None))
        );
    }

    #[test]
    fn an_absolute_address_is_taken_from_the_root_of_the_archive() {
        assert_eq!(
            resolve("OEBPS/ch1.xhtml", "/OEBPS/ch2.xhtml#x"),
            Some(reference("OEBPS/ch2.xhtml", Some("x")))
        );
    }

    #[test]
    fn an_address_without_a_path_leads_to_its_document() {
        assert_eq!(
            resolve("OEBPS/ch1.xhtml", "#note%201"),
            Some(reference("OEBPS/ch1.xhtml", Some("note 1")))
        );
        assert_eq!(
            resolve("OEBPS/ch1.xhtml", "#"),
            Some(reference("OEBPS/ch1.xhtml", None))
        );
    }

    #[test]
    fn an_address_outside_the_archive_leads_nowhere() {
        assert_eq!(resolve("ch1.xhtml", "../x.xhtml"), None);
        assert_eq!(resolve("ch1.xhtml", "https://example.org/a"), None);
        assert_eq!(resolve("ch1.xhtml", "//example.org/a"), None);
        assert_eq!(resolve("ch1.xhtml", "%ff.xhtml"), None);
        assert_eq!(resolve("ch1.xhtml", "/"), None);
    }

    #[test]
    fn a_section_name_has_no_hash_and_no_colon() {
        assert_eq!(section_name("a/b#c:d.xhtml"), "a/b_c_d.xhtml");
        assert_eq!(section_name("a/b.xhtml"), "a/b.xhtml");
    }
}
