//! The source formats and how one is recognised from its content.

use std::fmt;
use std::io::{Cursor, Read};
use std::str::FromStr;
use zip::ZipArchive;

/// A supported book format.
///
/// Every variant exists whatever features are enabled, so that a `match`
/// does not depend on the feature set. [`Format::is_enabled`] tells whether
/// this build can import it.
///
/// # Examples
///
/// ```
/// use mbook_converter::Format;
///
/// assert_eq!("epub".parse(), Ok(Format::Epub));
/// assert_eq!(Format::Fb2.to_string(), "fb2");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Format {
    /// EPUB 2 or 3.
    Epub,
    /// `FictionBook` 2, plain or as the only entry of a ZIP archive.
    Fb2,
    /// PDF.
    Pdf,
    /// A comic book archive: a ZIP of pictures.
    Cbz,
    /// A Word document.
    Docx,
    /// One HTML document.
    Html,
    /// One Markdown document. Never detected; it must be named.
    Markdown,
    /// Plain text. Never detected; it must be named.
    Text,
}

/// What a file name ends with when an entry of an archive is a picture.
const PICTURE_EXTENSIONS: [&str; 7] = ["avif", "bmp", "gif", "jpeg", "jpg", "png", "webp"];

impl Format {
    /// Every format, in the order of the table of RFC 0003.
    pub const ALL: &'static [Self] = &[
        Self::Epub,
        Self::Fb2,
        Self::Pdf,
        Self::Cbz,
        Self::Docx,
        Self::Html,
        Self::Markdown,
        Self::Text,
    ];

    /// Tells whether this format’s import feature was compiled in.
    #[must_use]
    pub const fn is_enabled(self) -> bool {
        match self {
            Self::Epub => cfg!(feature = "epub"),
            Self::Fb2 => cfg!(feature = "fb2"),
            Self::Pdf => cfg!(feature = "pdf"),
            Self::Cbz => cfg!(feature = "cbz"),
            Self::Docx => cfg!(feature = "docx"),
            Self::Html => cfg!(feature = "html"),
            Self::Markdown => cfg!(feature = "md"),
            Self::Text => cfg!(feature = "txt"),
        }
    }

    /// The standard extension for an exported file, without a leading dot.
    #[cfg(feature = "export")]
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Text => "txt",
            other => other.name(),
        }
    }

    /// The name of the format, which is also the name of its feature except
    /// for Markdown and plain text.
    const fn name(self) -> &'static str {
        match self {
            Self::Epub => "epub",
            Self::Fb2 => "fb2",
            Self::Pdf => "pdf",
            Self::Cbz => "cbz",
            Self::Docx => "docx",
            Self::Html => "html",
            Self::Markdown => "markdown",
            Self::Text => "text",
        }
    }

    /// Recognises a format from the content of a source, by the rules of
    /// "Detection" in RFC 0003. Markdown and plain text are never
    /// recognised.
    pub(crate) fn detect(source: &[u8]) -> Option<Self> {
        let head = source.get(..source.len().min(1024)).unwrap_or_default();
        if head.windows(5).any(|window| window == b"%PDF-") {
            return Some(Self::Pdf);
        }
        if source.starts_with(b"PK\x03\x04") {
            return detect_archive(source);
        }
        let text = leading_text(source);
        if text.contains("<fictionbook") {
            Some(Self::Fb2)
        } else if after_prolog(&text).starts_with("<!doctype html")
            || after_prolog(&text).starts_with("<html")
        {
            Some(Self::Html)
        } else {
            None
        }
    }
}

/// Tells which format a ZIP archive holds, from the names of its entries.
fn detect_archive(source: &[u8]) -> Option<Format> {
    let mut archive = ZipArchive::new(Cursor::new(source)).ok()?;
    let names: Vec<String> = archive
        .file_names()
        .filter_map(std::result::Result::ok)
        .filter(|name| !name.ends_with('/'))
        .map(std::borrow::Cow::into_owned)
        .collect();
    let has = |wanted: &str| names.iter().any(|name| name == wanted);
    if has("META-INF/container.xml") || declares_epub(&mut archive) {
        return Some(Format::Epub);
    }
    if has("word/document.xml") {
        return Some(Format::Docx);
    }
    let extension_of = |name: &str| {
        name.rsplit_once('.')
            .map(|(_, extension)| extension.to_ascii_lowercase())
    };
    if let [only] = names.as_slice()
        && extension_of(only).as_deref() == Some("fb2")
    {
        return Some(Format::Fb2);
    }
    let is_picture = |name: &String| {
        extension_of(name).is_some_and(|extension| PICTURE_EXTENSIONS.contains(&extension.as_str()))
    };
    let is_page_or_info =
        |name: &String| is_picture(name) || name.rsplit('/').next() == Some("ComicInfo.xml");
    (names.iter().any(is_picture) && names.iter().all(is_page_or_info)).then_some(Format::Cbz)
}

/// Tells whether an archive has the `mimetype` entry of an EPUB.
fn declares_epub(archive: &mut ZipArchive<Cursor<&[u8]>>) -> bool {
    const MEDIA_TYPE: &[u8] = b"application/epub+zip";
    let Ok(entry) = archive.by_name("mimetype") else {
        return false;
    };
    let mut declared = Vec::new();
    // Nothing longer than the expected value needs to be read.
    let enough = u64::try_from(MEDIA_TYPE.len() + 1).unwrap_or(u64::MAX);
    entry.take(enough).read_to_end(&mut declared).is_ok() && declared.trim_ascii() == MEDIA_TYPE
}

/// The first kilobytes of a source as lower-case text, whatever its
/// encoding: enough to find the first element of an XML or HTML document.
fn leading_text(source: &[u8]) -> String {
    let head = source.get(..source.len().min(4096)).unwrap_or_default();
    // UTF-16 text has a zero byte next to every ASCII character.
    let ascii: Vec<u8> = head.iter().copied().filter(|&byte| byte != 0).collect();
    String::from_utf8_lossy(&ascii).to_ascii_lowercase()
}

/// A document without what may stand in front of its first element: a byte
/// order mark, white space, an XML declaration and comments.
fn after_prolog(text: &str) -> &str {
    let mut rest =
        text.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}' || c == '\u{fffd}');
    loop {
        let skipped = [("<?xml", "?>"), ("<!--", "-->")]
            .iter()
            .find_map(|(open, close)| rest.strip_prefix(open)?.split_once(close))
            .map(|(_, after)| after.trim_start());
        match skipped {
            Some(after) => rest = after,
            None => return rest,
        }
    }
}

impl fmt::Display for Format {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The text is not the name of a format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("not the name of a source format")]
pub struct UnknownFormatName;

impl FromStr for Format {
    type Err = UnknownFormatName;

    /// Reads the name of a format or a common file extension, in any letter
    /// case and with or without a leading dot.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let name = text.strip_prefix('.').unwrap_or(text).to_ascii_lowercase();
        Ok(match name.as_str() {
            "epub" => Self::Epub,
            "fb2" | "fbz" | "fb2.zip" => Self::Fb2,
            "pdf" => Self::Pdf,
            "cbz" => Self::Cbz,
            "docx" => Self::Docx,
            "html" | "htm" | "xhtml" => Self::Html,
            "markdown" | "md" | "mkd" => Self::Markdown,
            "text" | "txt" => Self::Text,
            _ => return Err(UnknownFormatName),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Format, UnknownFormatName};
    use std::io::{Cursor, Write};
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    /// A ZIP archive with these entries.
    fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, content) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(content).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    #[test]
    fn every_format_has_a_name_that_reads_back() {
        for format in Format::ALL {
            assert_eq!(format.to_string().parse(), Ok(*format));
        }
    }

    #[test]
    fn file_extensions_name_formats() {
        let formats = [".EPUB", "fbz", "htm", "txt", "MD"].map(str::parse::<Format>);
        assert_eq!(
            formats,
            [
                Ok(Format::Epub),
                Ok(Format::Fb2),
                Ok(Format::Html),
                Ok(Format::Text),
                Ok(Format::Markdown)
            ]
        );
    }

    #[test]
    fn an_unknown_name_is_refused_with_a_message() {
        let refused = "djvu".parse::<Format>();
        assert_eq!(refused, Err(UnknownFormatName));
        assert_eq!(
            UnknownFormatName.to_string(),
            "not the name of a source format"
        );
    }

    #[test]
    fn a_format_is_enabled_when_its_feature_is() {
        assert_eq!(Format::Text.is_enabled(), cfg!(feature = "txt"));
        assert_eq!(Format::Pdf.is_enabled(), cfg!(feature = "pdf"));
    }

    #[test]
    fn a_pdf_is_recognised_by_its_header() {
        assert_eq!(Format::detect(b"%PDF-1.7\n..."), Some(Format::Pdf));
        assert_eq!(Format::detect(b"\n\n junk %PDF-1.4"), Some(Format::Pdf));
    }

    #[test]
    fn a_format_this_crate_does_not_read_is_not_recognised() {
        // Mobipocket: a Palm database with `BOOKMOBI` at offset 60.
        let mut source = vec![0; 60];
        source.extend_from_slice(b"BOOKMOBI");
        assert_eq!(Format::detect(&source), None);
        assert!("mobi".parse::<Format>().is_err());
    }

    #[test]
    fn an_epub_is_recognised_by_its_media_type_entry() {
        let source = archive(&[("mimetype", b"application/epub+zip\n")]);
        assert_eq!(Format::detect(&source), Some(Format::Epub));
    }

    #[test]
    fn an_epub_is_recognised_by_its_container_file() {
        let source = archive(&[("META-INF/container.xml", b"<container/>")]);
        assert_eq!(Format::detect(&source), Some(Format::Epub));
    }

    #[test]
    fn another_media_type_is_not_an_epub() {
        let source = archive(&[("mimetype", b"application/zip"), ("a.txt", b"x")]);
        assert_eq!(Format::detect(&source), None);
    }

    #[test]
    fn a_word_document_is_recognised_by_its_main_part() {
        let source = archive(&[("[Content_Types].xml", b""), ("word/document.xml", b"")]);
        assert_eq!(Format::detect(&source), Some(Format::Docx));
    }

    #[test]
    fn an_archive_with_one_fb2_entry_is_fictionbook() {
        let source = archive(&[("Book.FB2", b"<FictionBook/>")]);
        assert_eq!(Format::detect(&source), Some(Format::Fb2));
    }

    #[test]
    fn an_archive_of_pictures_is_a_comic() {
        let source = archive(&[
            ("vol1/001.jpg", b""),
            ("vol1/002.PNG", b""),
            ("ComicInfo.xml", b""),
        ]);
        assert_eq!(Format::detect(&source), Some(Format::Cbz));
    }

    #[test]
    fn an_archive_with_anything_else_is_not_recognised() {
        let with_text = archive(&[("001.jpg", b""), ("readme.txt", b"")]);
        let without_pictures = archive(&[("ComicInfo.xml", b"")]);
        let two_books = archive(&[("a.fb2", b""), ("b.fb2", b"")]);
        for source in [with_text, without_pictures, two_books] {
            assert_eq!(Format::detect(&source), None);
        }
    }

    #[test]
    fn a_damaged_archive_is_not_recognised() {
        assert_eq!(Format::detect(b"PK\x03\x04 not really"), None);
    }

    #[test]
    fn fictionbook_is_recognised_by_its_root_element() {
        let source = "<?xml version=\"1.0\" encoding=\"windows-1251\"?>\n<FictionBook xmlns=\"x\">";
        assert_eq!(Format::detect(source.as_bytes()), Some(Format::Fb2));
    }

    #[test]
    fn fictionbook_in_utf_16_is_recognised() {
        let mut source = vec![0xff, 0xfe];
        for unit in "<?xml version=\"1.0\"?><FictionBook>".encode_utf16() {
            source.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(Format::detect(&source), Some(Format::Fb2));
    }

    #[test]
    fn html_is_recognised_by_its_first_element() {
        let sources = [
            "<!DOCTYPE html><html>",
            "\u{feff}  <HTML lang=\"en\">",
            "<?xml version=\"1.0\"?>\n<!-- a comment -->\n<!DOCTYPE html PUBLIC \"x\">",
        ];
        for source in sources {
            assert_eq!(Format::detect(source.as_bytes()), Some(Format::Html));
        }
    }

    #[test]
    fn markdown_and_plain_text_are_never_recognised() {
        assert_eq!(Format::detect(b"# A heading\n\nA paragraph.\n"), None);
        assert_eq!(Format::detect(b"Call me Ishmael."), None);
        assert_eq!(Format::detect(b""), None);
        assert_eq!(Format::detect(b"<p>not a document</p>"), None);
    }
}
