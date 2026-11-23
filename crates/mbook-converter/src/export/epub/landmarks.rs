//! The landmarks of an exported EPUB.
//!
//! `epub-builder` gives a landmark only to a whole document of the spine and
//! only by a type of its own, which would lose the anchor of a landmark, a
//! second landmark of the same chapter and several kinds. So the writer lists
//! the landmarks itself: it replaces the `landmarks` navigation that
//! `epub-builder` renders and adds to the guide of the package, in the
//! archive `epub-builder` generated.

use std::fmt::Write as _;
use std::io::{Cursor, Read as _, Write as _};

use mbook::Book;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use super::super::markup::epub_href;
use super::super::{Format, Result, export_error, writer_error};

/// The navigation document `epub-builder` writes.
const NAV: &str = "OEBPS/nav.xhtml";
/// The package document `epub-builder` writes.
const PACKAGE: &str = "OEBPS/content.opf";
/// The page that shows the cover picture.
pub(super) const COVER_PAGE: &str = "cover.xhtml";

/// A landmark as the EPUB lists it.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Mark {
    /// The type of the EPUB structural semantics vocabulary, which the
    /// `landmarks` navigation of EPUB 3 uses.
    semantic: &'static str,
    /// The type of the guide of EPUB 2, when it has one for the kind.
    guide: Option<&'static str>,
    title: String,
    href: String,
}

/// What EPUB calls a kind of RFC 0001: the type of the structural semantics
/// vocabulary, the type of the guide of EPUB 2 if it has one, and the title
/// of a landmark that has none of its own, since a link of the navigation
/// must have text. `None` for `about-the-author` and the kinds for private
/// use, which EPUB has no type for.
fn vocabulary(kind: &str) -> Option<(&'static str, Option<&'static str>, &'static str)> {
    Some(match kind {
        "start" => ("bodymatter", Some("text"), "Start"),
        "title-page" => ("titlepage", Some("title-page"), "Title page"),
        "dedication" => ("dedication", Some("dedication"), "Dedication"),
        "epigraph" => ("epigraph", Some("epigraph"), "Epigraph"),
        "foreword" => ("foreword", Some("foreword"), "Foreword"),
        "preface" => ("preface", Some("preface"), "Preface"),
        "introduction" => ("introduction", None, "Introduction"),
        "prologue" => ("prologue", None, "Prologue"),
        "epilogue" => ("epilogue", None, "Epilogue"),
        "conclusion" => ("conclusion", None, "Conclusion"),
        "afterword" => ("afterword", None, "Afterword"),
        "acknowledgements" => (
            "acknowledgments",
            Some("acknowledgements"),
            "Acknowledgements",
        ),
        "appendix" => ("appendix", None, "Appendix"),
        "notes" => ("endnotes", Some("notes"), "Notes"),
        "glossary" => ("glossary", Some("glossary"), "Glossary"),
        "bibliography" => ("bibliography", Some("bibliography"), "Bibliography"),
        "index" => ("index", Some("index"), "Index"),
        "list-of-figures" => ("loi", Some("loi"), "List of figures"),
        "list-of-tables" => ("lot", Some("lot"), "List of tables"),
        "copyright" => ("copyright-page", Some("copyright-page"), "Copyright page"),
        "colophon" => ("colophon", Some("colophon"), "Colophon"),
        _ => return None,
    })
}

/// The landmarks of the EPUB, in order: the cover page when `cover`, then
/// the landmarks of the book that EPUB has a type for and that lead to a
/// chapter and an anchor of the book.
pub(super) fn marks(book: &Book, cover: bool) -> Vec<Mark> {
    let cover = cover.then(|| Mark {
        semantic: "cover",
        guide: None,
        title: "Cover".to_owned(),
        href: COVER_PAGE.to_owned(),
    });
    let landmarks = book.landmarks().iter().filter_map(|landmark| {
        let (semantic, guide, name) = vocabulary(&landmark.kind)?;
        let href = epub_href(book, &landmark.chapter, landmark.anchor.as_deref())?;
        let title = match landmark.title.trim() {
            "" => name,
            title => title,
        };
        Some(Mark {
            semantic,
            guide,
            title: title.to_owned(),
            href,
        })
    });
    cover.into_iter().chain(landmarks).collect()
}

/// The `landmarks` navigation of `marks`; empty when there are none, since
/// the navigation must hold a list with at least one entry.
fn navigation(marks: &[Mark]) -> String {
    if marks.is_empty() {
        return String::new();
    }
    let mut entries = String::new();
    for mark in marks {
        // Writing to a `String` does not fail.
        let _ = write!(
            entries,
            "<li><a epub:type=\"{}\" href=\"{}\">{}</a></li>",
            mark.semantic,
            html_escape::encode_double_quoted_attribute(&mark.href),
            html_escape::encode_text(&mark.title),
        );
    }
    format!("<nav epub:type=\"landmarks\"><ol>{entries}</ol></nav>")
}

/// The references of the guide for the marks that have a guide type. The
/// cover page has its reference from `epub-builder` already.
fn references(marks: &[Mark]) -> String {
    marks
        .iter()
        .filter_map(|mark| {
            Some(format!(
                "<reference type=\"{}\" title=\"{}\" href=\"{}\"/>",
                mark.guide?,
                html_escape::encode_double_quoted_attribute(&mark.title),
                html_escape::encode_double_quoted_attribute(&mark.href),
            ))
        })
        .collect()
}

/// The navigation document `nav` with `landmarks` in place of its own
/// `landmarks` navigation, or at the end of its body when it has none.
/// `None` when it has neither.
fn with_navigation(nav: &str, landmarks: &str) -> Option<String> {
    // Text and attribute values are escaped, so `<nav` only opens an
    // element.
    let own = nav.match_indices("<nav").find_map(|(start, _)| {
        let tag = nav[start..].split('>').next()?;
        tag.contains("landmarks").then_some(start)
    });
    let (start, end) = if let Some(start) = own {
        (start, start + nav[start..].find("</nav>")? + "</nav>".len())
    } else {
        let end = nav.rfind("</body>")?;
        (end, end)
    };
    Some(format!("{}{landmarks}{}", &nav[..start], &nav[end..]))
}

/// The package document `package` with `references` at the end of its
/// guide. `None` when there are references and no guide to hold them.
fn with_guide(package: &str, references: &str) -> Option<String> {
    if references.is_empty() {
        return Some(package.to_owned());
    }
    let end = package.rfind("</guide>")?;
    Some(format!(
        "{}{references}{}",
        &package[..end],
        &package[end..]
    ))
}

/// How a document of the archive is given its landmarks.
type Rewrite = fn(&str, &str) -> Option<String>;

/// An error of the archive, reported as one of the EPUB writer.
fn failed(error: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> crate::Error {
    export_error(Format::Epub, error)
}

/// The archive `epub` with `marks` as its landmarks. Every other entry is
/// copied as it is, in its place, so the `mimetype` stays first and stored.
///
/// # Errors
///
/// The archive cannot be read or written, or a document has no place for
/// the landmarks.
pub(super) fn with_marks(epub: Vec<u8>, marks: &[Mark]) -> Result<Vec<u8>> {
    let navigation = navigation(marks);
    let references = references(marks);
    let mut archive = ZipArchive::new(Cursor::new(epub)).map_err(failed)?;
    let mut out = ZipWriter::new(Cursor::new(Vec::new()));
    for index in 0..archive.len() {
        let name = archive
            .by_index_raw(index)
            .and_then(|file| Ok(file.name()?.into_owned()))
            .map_err(failed)?;
        let (rewritten, addition): (Rewrite, &str) = match name.as_str() {
            NAV => (with_navigation, &navigation),
            PACKAGE => (with_guide, &references),
            _ => {
                let file = archive.by_index_raw(index).map_err(failed)?;
                out.raw_copy_file(file).map_err(failed)?;
                continue;
            }
        };
        let mut text = String::new();
        archive
            .by_index(index)
            .map_err(failed)?
            .read_to_string(&mut text)
            .map_err(failed)?;
        let text = rewritten(&text, addition).ok_or_else(|| {
            writer_error(Format::Epub, format!("{name} has no place for landmarks"))
        })?;
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        out.start_file(name, options).map_err(failed)?;
        out.write_all(text.as_bytes()).map_err(failed)?;
    }
    Ok(out.finish().map_err(failed)?.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_but_about_the_author_has_a_type_that_is_read_back() {
        let kinds = [
            "start",
            "title-page",
            "dedication",
            "epigraph",
            "foreword",
            "preface",
            "introduction",
            "prologue",
            "epilogue",
            "conclusion",
            "afterword",
            "acknowledgements",
            "appendix",
            "notes",
            "glossary",
            "bibliography",
            "index",
            "list-of-figures",
            "list-of-tables",
            "copyright",
            "colophon",
        ];
        for kind in kinds {
            let (semantic, _, name) = vocabulary(kind).unwrap_or_default();
            assert!(!semantic.is_empty() && !name.is_empty(), "{kind}");
        }
        assert_eq!(vocabulary("about-the-author"), None);
        assert_eq!(vocabulary("x-private"), None);
    }

    #[test]
    fn a_navigation_document_gets_the_landmarks_in_place_of_its_own() {
        let nav = "<body><nav epub:type=\"toc\">A</nav>\
                   <nav epub:type = \"landmarks\">\n</nav></body>";
        assert_eq!(
            with_navigation(nav, "<nav>L</nav>").as_deref(),
            Some("<body><nav epub:type=\"toc\">A</nav><nav>L</nav></body>")
        );
    }

    #[test]
    fn a_navigation_document_without_landmarks_gets_them_at_the_end_of_its_body() {
        assert_eq!(
            with_navigation("<body><nav>A</nav></body>", "<nav>L</nav>").as_deref(),
            Some("<body><nav>A</nav><nav>L</nav></body>")
        );
    }

    #[test]
    fn a_navigation_document_without_a_body_or_an_end_of_its_landmarks_has_no_place() {
        assert_eq!(with_navigation("<nav>A</nav>", "L"), None);
        assert_eq!(with_navigation("<nav id=\"landmarks\">", "L"), None);
    }

    #[test]
    fn references_go_at_the_end_of_the_guide() {
        assert_eq!(
            with_guide("<guide><a/></guide></package>", "<b/>").as_deref(),
            Some("<guide><a/><b/></guide></package>")
        );
    }

    #[test]
    fn a_package_without_a_guide_has_room_for_no_references_only() {
        assert_eq!(with_guide("<package/>", "").as_deref(), Some("<package/>"));
        assert_eq!(with_guide("<package/>", "<b/>"), None);
    }

    #[test]
    fn an_archive_that_cannot_be_read_is_an_export_error() {
        let error = with_marks(b"not a zip".to_vec(), &[]);
        assert!(matches!(
            error,
            Err(crate::Error::Export {
                format: Format::Epub,
                ..
            })
        ));
    }
}
