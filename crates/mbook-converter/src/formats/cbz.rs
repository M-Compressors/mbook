//! Comic book archives.
//!
//! A comic book archive is a ZIP archive of pictures. The pages are the
//! entries whose name ends in `avif`, `bmp`, `gif`, `jpeg`, `jpg`, `png` or
//! `webp` in any letter case, in the natural order of their names: the
//! folder first, then the file, so that `page2` comes before `page10`.
//! Folders, entries below `__MACOSX/`, entries whose file name starts with a
//! dot and everything else that is not a page are ignored.
//!
//! - Each folder of the archive that holds pages directly is a section,
//!   headed by the last component of the folder path. The pages in the root
//!   of the archive form a section without a heading; it comes first.
//! - Each page is a figure: a paragraph with nothing but the picture. The
//!   bytes of a page are handed over as they are stored.
//! - The first page in reading order is the cover.
//! - `ComicInfo.xml` in the root fills the metadata. A file that is not
//!   well-formed XML (or not UTF-8) is left out with a warning.
//!
//! An encrypted entry makes the source protected. Entries are read through
//! the limits of the options whatever size their headers declare.

use crate::draft::{Draft, DraftMeta, PictureData, Section, SourcePicture};
use crate::error::{Error, Result};
use crate::format::Format;
use crate::markdown;
use crate::options::ConvertOptions;
use crate::report::{Warning, WarningKind};
use quick_xml::escape::resolve_predefined_entity;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};
use zip::ZipArchive;
use zip::result::ZipError;

/// What a file name ends with when an entry is a page.
const PAGE_EXTENSIONS: [&str; 7] = ["avif", "bmp", "gif", "jpeg", "jpg", "png", "webp"];

/// The name of the section of the pages in the root of the archive.
const ROOT_SECTION: &str = "pages";

/// What an entry of the archive is.
#[derive(Debug, PartialEq, Eq)]
enum Kind {
    /// A picture of the book.
    Page,
    /// `ComicInfo.xml` in the root.
    Info,
    /// Anything else.
    Ignored,
}

/// An entry that is a page: where it is in the archive and what it is called.
#[derive(Debug)]
struct Page {
    index: usize,
    name: String,
}

impl Page {
    /// The folder of the page; empty for the root.
    fn folder(&self) -> &str {
        split(&self.name).0
    }
}

/// Reads a comic book archive into a draft with one section per folder.
///
/// # Errors
///
/// The source is not a ZIP archive or an entry cannot be unpacked
/// ([`Error::Source`]), an entry is encrypted ([`Error::Protected`]), or a
/// limit is exceeded ([`Error::Limit`]).
pub(crate) fn read(source: &[u8], options: &ConvertOptions) -> Result<Draft> {
    let limits = &options.limits.book;
    let mut archive =
        ZipArchive::new(Cursor::new(source)).map_err(|error| Error::source(Format::Cbz, error))?;
    if archive.len() > limits.max_resources {
        return Err(Error::Limit("the archive has too many entries"));
    }

    let mut pages = Vec::new();
    let mut info = None;
    for (index, name) in archive.file_names().enumerate() {
        let name = name
            .map_err(|error| Error::source(Format::Cbz, error))?
            .into_owned();
        match classify(&name) {
            Kind::Page => pages.push(Page { index, name }),
            Kind::Info if info.is_none() => info = Some(index),
            Kind::Info | Kind::Ignored => {}
        }
    }
    pages.sort_by(|a, b| reading_order(&a.name, &b.name));

    let mut draft = Draft::default();
    let mut total = 0;
    if let Some(index) = info {
        let bytes = unpack(&mut archive, index, options, &mut total)?;
        match comic_info(&bytes) {
            Some(meta) => draft.meta = meta,
            None => draft.warnings.push(Warning::new(
                WarningKind::MetadataDropped("ComicInfo.xml".to_owned()),
                "",
            )),
        }
    }
    for page in &pages {
        let bytes = unpack(&mut archive, page.index, options, &mut total)?;
        draft.pictures.push(SourcePicture {
            name: page.name.clone(),
            data: PictureData::Encoded(bytes),
        });
    }
    draft.cover = pages.first().map(|page| page.name.clone());

    let mut used = HashSet::new();
    for group in pages.chunk_by(|a, b| a.folder() == b.folder()) {
        let Some(first) = group.first() else { continue };
        let folder = first.folder();
        draft.sections.push(Section {
            name: unique_name(folder, &mut used),
            markdown: section_text(folder, group),
        });
    }
    Ok(draft)
}

/// Unpacks one entry and counts it against the limits.
fn unpack(
    archive: &mut ZipArchive<Cursor<&[u8]>>,
    index: usize,
    options: &ConvertOptions,
    total: &mut u64,
) -> Result<Vec<u8>> {
    let limits = &options.limits.book;
    let entry = archive.by_index(index).map_err(|error| match error {
        ZipError::UnsupportedArchive(ZipError::PASSWORD_REQUIRED) => Error::Protected(Format::Cbz),
        other => Error::source(Format::Cbz, other),
    })?;
    // The size in the header is not trusted: one byte more than allowed
    // shows that the entry is too large.
    let mut bytes = Vec::new();
    entry
        .take(limits.max_resource_bytes.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| Error::source(Format::Cbz, error))?;
    let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if length > limits.max_resource_bytes {
        return Err(Error::Limit("an entry of the archive is too large"));
    }
    *total = total.saturating_add(length);
    if *total > limits.max_file_bytes {
        return Err(Error::Limit("the archive unpacks to too many bytes"));
    }
    Ok(bytes)
}

/// Tells what an entry is from its name.
fn classify(name: &str) -> Kind {
    if name.ends_with('/') || name.starts_with("__MACOSX/") {
        return Kind::Ignored;
    }
    let file = split(name).1;
    if file.starts_with('.') {
        return Kind::Ignored;
    }
    if !name.contains('/') && file.eq_ignore_ascii_case("ComicInfo.xml") {
        return Kind::Info;
    }
    let is_page = file.rsplit_once('.').is_some_and(|(_, extension)| {
        PAGE_EXTENSIONS
            .iter()
            .any(|known| extension.eq_ignore_ascii_case(known))
    });
    if is_page { Kind::Page } else { Kind::Ignored }
}

/// A name as its folder and its file name; the folder is empty for the root.
fn split(name: &str) -> (&str, &str) {
    name.rsplit_once('/').unwrap_or(("", name))
}

/// Compares two names the way a person reads them: a run of digits as a
/// number, and without regard to letter case.
fn natural(a: &str, b: &str) -> Ordering {
    alphanumeric_sort::compare_str(a.to_lowercase(), b.to_lowercase())
}

/// The natural order of two entry names: by folder, then by file name. Letter
/// case does not matter unless nothing else tells two names apart.
fn reading_order(a: &str, b: &str) -> Ordering {
    let ((folder_a, file_a), (folder_b, file_b)) = (split(a), split(b));
    natural(folder_a, folder_b)
        .then_with(|| folder_a.cmp(folder_b))
        .then_with(|| natural(file_a, file_b))
        .then_with(|| file_a.cmp(file_b))
}

/// A section name for a folder: the path of the folder without the
/// characters a name cannot hold, and different from the name of every
/// section before it.
fn unique_name(folder: &str, used: &mut HashSet<String>) -> String {
    let base = if folder.is_empty() {
        ROOT_SECTION.to_owned()
    } else {
        folder.replace(['#', ':'], "_")
    };
    let mut name = base.clone();
    let mut number = 1_u32;
    while !used.insert(name.clone()) {
        number += 1;
        name = format!("{base}~{number}");
    }
    name
}

/// The text of the section of a folder: its heading, when it is not the
/// root, and a figure for each page.
fn section_text(folder: &str, pages: &[Page]) -> String {
    let mut text = String::new();
    let title = split_last(folder);
    let title = markdown::escape(title).replace(['\n', '\r'], " ");
    if !folder.is_empty() && !title.trim().is_empty() {
        text.push_str("# ");
        text.push_str(title.trim());
        text.push_str("\n\n");
    }
    let figures: Vec<String> = pages
        .iter()
        .map(|page| markdown::image("", &markdown::encoded_path(&page.name), None))
        .collect();
    text.push_str(&figures.join("\n\n"));
    text.push('\n');
    text
}

/// The last component of a folder path.
fn split_last(folder: &str) -> &str {
    folder.rsplit_once('/').map_or(folder, |(_, last)| last)
}

/// Reads `ComicInfo.xml`. `None` when it is not well-formed XML in UTF-8.
fn comic_info(bytes: &[u8]) -> Option<DraftMeta> {
    let text = std::str::from_utf8(bytes).ok()?;
    let fields = comic_fields(text.strip_prefix('\u{feff}').unwrap_or(text))?;
    let get = |key: &str| {
        fields
            .get(key)
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
    };
    let own = |value: Option<&str>| value.map(str::to_owned);
    Some(DraftMeta {
        title: own(get("Title")),
        languages: get("LanguageISO").map(str::to_owned).into_iter().collect(),
        authors: comma_list(get("Writer")),
        translators: comma_list(get("Translator")),
        series: own(get("Series")),
        series_index: own(get("Number")),
        publisher: own(get("Publisher")),
        date: date(get),
        genres: comma_list(get("Genre")),
        annotation: own(get("Summary")),
        ..DraftMeta::default()
    })
}

/// The text of every child of the root element, by element name; the first
/// of a name wins. `None` when the document is not well-formed.
fn comic_fields(xml: &str) -> Option<HashMap<String, String>> {
    let mut reader = Reader::from_str(xml);
    let mut fields = HashMap::new();
    let mut field: Option<(String, String)> = None;
    let mut depth = 0_usize;
    let mut has_root = false;
    loop {
        match reader.read_event().ok()? {
            Event::Start(start) => {
                depth += 1;
                has_root = true;
                if depth == 2 {
                    let name = start.local_name().as_ref().to_owned();
                    field = Some((name, String::new()));
                }
            }
            Event::End(_) => {
                if depth == 2
                    && let Some((name, value)) = field.take()
                {
                    fields.entry(name).or_insert(value);
                }
                depth = depth.checked_sub(1)?;
            }
            Event::Empty(_) => has_root = true,
            Event::Text(text) => push_value(&mut field, depth, &text.xml10_content()),
            Event::CData(data) => push_value(&mut field, depth, &data.xml10_content()),
            Event::GeneralRef(reference) => {
                let mut buffer = [0; 4];
                let value = match reference.resolve_char_ref().ok()? {
                    Some(character) => &*character.encode_utf8(&mut buffer),
                    None => resolve_predefined_entity(&reference)?,
                };
                push_value(&mut field, depth, value);
            }
            Event::Eof => break,
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {}
        }
    }
    (depth == 0 && has_root).then_some(fields)
}

/// Adds text to the element being read, when it is a child of the root.
fn push_value(field: &mut Option<(String, String)>, depth: usize, text: &str) {
    if depth == 2
        && let Some((_, value)) = field
    {
        value.push_str(text);
    }
}

/// The items of a list separated by commas.
fn comma_list(value: Option<&str>) -> Vec<String> {
    value
        .into_iter()
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The date of `Year`, `Month` and `Day` as `YYYY`, `YYYY-MM` or
/// `YYYY-MM-DD`. A month or a day outside its range ends the date.
fn date<'a>(get: impl Fn(&str) -> Option<&'a str>) -> Option<String> {
    let number = |key: &str, last: u32| {
        get(key)?
            .parse::<u32>()
            .ok()
            .filter(|value| (1..=last).contains(value))
    };
    let year = number("Year", 9999)?;
    let Some(month) = number("Month", 12) else {
        return Some(format!("{year:04}"));
    };
    Some(match number("Day", 31) {
        Some(day) => format!("{year:04}-{month:02}-{day:02}"),
        None => format!("{year:04}-{month:02}"),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        Kind, classify, comic_fields, comic_info, comma_list, reading_order, split, unique_name,
    };
    use std::cmp::Ordering;
    use std::collections::HashSet;

    #[test]
    fn pages_are_told_by_the_extension_in_any_letter_case() {
        for name in [
            "a.png", "d/a.JPG", "a.JpEg", "a.webp", "a.avif", "a.gif", "a.bmp",
        ] {
            assert_eq!(classify(name), Kind::Page, "{name}");
        }
        for name in ["a.txt", "a", "png", "d/", "a.png.bak", "a.tiff"] {
            assert_eq!(classify(name), Kind::Ignored, "{name}");
        }
    }

    #[test]
    fn metadata_is_found_by_its_name_in_the_root_only() {
        assert_eq!(classify("ComicInfo.xml"), Kind::Info);
        assert_eq!(classify("comicinfo.XML"), Kind::Info);
        assert_eq!(classify("vol1/ComicInfo.xml"), Kind::Ignored);
    }

    #[test]
    fn debris_of_other_systems_is_ignored() {
        for name in [
            "__MACOSX/1.png",
            ".hidden.png",
            "vol1/.1.png",
            "__MACOSX/a/b.jpg",
        ] {
            assert_eq!(classify(name), Kind::Ignored, "{name}");
        }
    }

    #[test]
    fn a_name_is_split_into_folder_and_file() {
        assert_eq!(split("a/b/c.png"), ("a/b", "c.png"));
        assert_eq!(split("c.png"), ("", "c.png"));
    }

    #[test]
    fn numbers_in_names_are_ordered_by_value() {
        assert_eq!(reading_order("2.png", "10.png"), Ordering::Less);
        assert_eq!(
            reading_order("vol10/1.png", "vol2/9.png"),
            Ordering::Greater
        );
    }

    #[test]
    fn the_folder_decides_before_the_file() {
        // As whole names `a b/1.png` would come before `a/1.png`.
        assert_eq!(reading_order("a/1.png", "a b/1.png"), Ordering::Less);
        assert_eq!(reading_order("z.png", "a/1.png"), Ordering::Less);
    }

    #[test]
    fn letter_case_only_breaks_a_tie() {
        assert_eq!(reading_order("B.png", "a.png"), Ordering::Greater);
        assert_eq!(reading_order("a.png", "A.png"), Ordering::Greater);
        assert_eq!(reading_order("a.png", "a.png"), Ordering::Equal);
    }

    #[test]
    fn section_names_are_unique_and_free_of_reserved_characters() {
        let mut used = HashSet::new();
        let names: Vec<String> = ["", "pages", "a#b", "a:b", "pages"]
            .iter()
            .map(|folder| unique_name(folder, &mut used))
            .collect();
        assert_eq!(names, ["pages", "pages~2", "a_b", "a_b~2", "pages~3"]);
    }

    #[test]
    fn a_comma_list_drops_empty_items() {
        assert_eq!(comma_list(Some(" A , ,B,")), ["A", "B"]);
        assert_eq!(comma_list(None), Vec::<String>::new());
    }

    #[test]
    fn fields_are_the_children_of_the_root() -> Result<(), String> {
        let xml = "<ComicInfo><Title>A &amp; B &#x41;<![CDATA[ <c>]]></Title><Title>2</Title>\
                   <Empty/><Pages><Page Image=\"0\">x</Page></Pages></ComicInfo>";
        let fields = comic_fields(xml).ok_or("well-formed")?;
        assert_eq!(fields.get("Title").map(String::as_str), Some("A & B A <c>"));
        assert_eq!(fields.get("Pages").map(String::as_str), Some(""));
        assert!(!fields.contains_key("Empty"));
        Ok(())
    }

    #[test]
    fn documents_that_are_not_well_formed_have_no_fields() {
        for xml in [
            "",
            "plain text",
            "<a><b></a>",
            "<a>",
            "</a>",
            "<a>&nope;</a>",
            "<a>&#0;</a>",
            "<a",
        ] {
            assert!(comic_fields(xml).is_none(), "{xml:?}");
        }
    }

    #[test]
    fn a_byte_order_mark_does_not_matter_and_other_encodings_are_refused() {
        assert!(comic_info(b"\xef\xbb\xbf<ComicInfo><Title>T</Title></ComicInfo>").is_some());
        assert!(comic_info(b"<ComicInfo>\xff</ComicInfo>").is_none());
    }
}
