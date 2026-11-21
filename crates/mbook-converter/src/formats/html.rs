//! One HTML or XHTML document.
//!
//! - **Encoding.** A byte order mark decides. Otherwise the first kilobytes
//!   are searched for `<meta charset>`, `<meta http-equiv="Content-Type">`
//!   or the `encoding` of an XML declaration; without one the encoding is
//!   guessed from the bytes.
//! - **Text.** The document is converted by [`crate::xhtml`], which writes
//!   Markdown without raw HTML, and cut into sections at its top-level
//!   headings; the text in front of the first one is a section of its own.
//!   Sections are named `s1`, `s2`, ….
//! - **Links.** A link to an `id` of the document leads to the section that
//!   holds the element; an `id` that does not exist is written as `#id`,
//!   which the assembler drops. Any other address is written as it is: the
//!   assembler keeps `http`, `https` and `mailto` and drops the rest, such
//!   as the relative file names of a document that has no folder.
//! - **Pictures.** Only a `data:` address brings its picture with it. The
//!   pictures are named `data-1`, `data-2`, …; one address used twice is one
//!   picture. Any other address is kept, and the assembler replaces the
//!   picture by its alternative text.
//! - **Metadata.** The title is `<title>`, the language the `lang` of
//!   `<html>`, the authors the `author` metas, the annotation the
//!   `description` and the genres the comma separated `keywords`.
//!
//! A document with nothing in it gives an empty draft.

use crate::draft::{Draft, DraftMeta, PictureData, Section, SourcePicture};
use crate::error::{Error, Result};
use crate::format::Format;
use crate::markdown;
use crate::options::ConvertOptions;
use crate::report::Warning;
use crate::xhtml;
use base64::Engine;
use base64::alphabet;
use base64::engine::DecodePaddingMode;
use base64::engine::general_purpose::{GeneralPurpose, GeneralPurposeConfig};
use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE};
use percent_encoding::{percent_decode, percent_decode_str};
use std::collections::HashMap;

/// How much of the start of a document is searched for its encoding.
const SNIFF_BYTES: usize = 4096;

/// Base64 as it is found in the wild: with or without padding.
const LENIENT: GeneralPurpose = GeneralPurpose::new(
    &alphabet::STANDARD,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

/// Reads one HTML document into a draft.
pub(crate) fn read(source: &[u8], options: &ConvertOptions) -> Result<Draft> {
    let text = decode(source);
    // The first pass finds out which section holds every element; the links
    // of the second say so. Cutting does not depend on the links.
    let outline = xhtml::convert(Format::Html, &text, |_| None, |_| Ok(None))?;
    if outline.markdown.trim().is_empty() {
        return Ok(Draft::default());
    }
    let mut owners: HashMap<String, usize> = HashMap::new();
    for (index, part) in outline.parts().into_iter().enumerate() {
        for id in part.ids {
            owners.entry(id).or_insert(index);
        }
    }

    let mut store = Store::new(options);
    let document = xhtml::convert(
        Format::Html,
        &text,
        |href| {
            let id = percent_decode_str(href.strip_prefix('#')?).decode_utf8_lossy();
            if id.is_empty() {
                return None;
            }
            // An element that is not there leads to the section of the link.
            let section = owners.get(id.as_ref()).map(|&index| section_name(index));
            Some(markdown::destination(
                section.as_deref().unwrap_or_default(),
                Some(&id),
            ))
        },
        |source| store.add(source),
    )?;

    let sections = document
        .parts()
        .into_iter()
        .enumerate()
        .map(|(index, part)| Section {
            name: section_name(index),
            markdown: part.markdown,
        })
        .collect();
    let genres = document
        .keywords
        .iter()
        .flat_map(|keywords| keywords.split(','))
        .map(str::trim)
        .filter(|keyword| !keyword.is_empty())
        .map(str::to_owned)
        .collect();
    Ok(Draft {
        meta: DraftMeta {
            title: document.title,
            languages: document.lang.into_iter().collect(),
            authors: document.authors,
            genres,
            annotation: document.description,
            ..DraftMeta::default()
        },
        sections,
        pictures: store.pictures,
        warnings: document
            .warnings
            .into_iter()
            .map(Warning::whole_book)
            .collect(),
        ..Draft::default()
    })
}

/// The name of the section at an index.
fn section_name(index: usize) -> String {
    format!("s{}", index + 1)
}

/// The pictures that `data:` addresses brought with them.
struct Store {
    pictures: Vec<SourcePicture>,
    /// The bytes of every picture so far.
    total: u64,
    one_limit: u64,
    all_limit: u64,
}

impl Store {
    fn new(options: &ConvertOptions) -> Self {
        Self {
            pictures: Vec::new(),
            total: 0,
            one_limit: options.limits.book.max_resource_bytes,
            all_limit: options.limits.book.max_file_bytes,
        }
    }

    /// Stores the picture of a `data:` address and gives its name. `None`
    /// for any other address and for data that cannot be read.
    fn add(&mut self, source: &str) -> Result<Option<String>> {
        let Some((header, payload)) = data_address(source) else {
            return Ok(None);
        };
        let base64 = header
            .split(';')
            .any(|parameter| parameter.trim().eq_ignore_ascii_case("base64"));
        let bytes = if base64 {
            // Whole groups of four characters are three bytes each.
            let upper = u64::try_from(payload.len() / 4 * 3).unwrap_or(u64::MAX);
            if upper > self.one_limit {
                return Err(Error::Limit("a picture is larger than the resource limit"));
            }
            let text: Vec<u8> = percent_decode(payload.as_bytes())
                .filter(|byte| !byte.is_ascii_whitespace())
                .collect();
            match LENIENT.decode(text) {
                Ok(bytes) => bytes,
                Err(_) => return Ok(None),
            }
        } else {
            let upper = u64::try_from(payload.len()).unwrap_or(u64::MAX);
            if upper > self.one_limit {
                return Err(Error::Limit("a picture is larger than the resource limit"));
            }
            percent_decode(payload.as_bytes()).collect()
        };
        let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if length > self.one_limit {
            return Err(Error::Limit("a picture is larger than the resource limit"));
        }
        if bytes.is_empty() {
            return Ok(None);
        }
        let same = self.pictures.iter().find(
            |picture| matches!(&picture.data, PictureData::Encoded(known) if *known == bytes),
        );
        if let Some(known) = same {
            return Ok(Some(known.name.clone()));
        }
        self.total = self.total.saturating_add(length);
        if self.total > self.all_limit {
            return Err(Error::Limit("the pictures are larger than the file limit"));
        }
        let name = format!("data-{}", self.pictures.len() + 1);
        self.pictures.push(SourcePicture {
            name: name.clone(),
            data: PictureData::Encoded(bytes),
        });
        Ok(Some(name))
    }
}

/// The parameters and the data of a `data:` address.
fn data_address(address: &str) -> Option<(&str, &str)> {
    let address = address.trim_start();
    let scheme = address.get(..5)?;
    if !scheme.eq_ignore_ascii_case("data:") {
        return None;
    }
    address.get(5..)?.split_once(',')
}

/// The text of a document in any encoding. A byte order mark decides; then
/// the document says so itself; otherwise the encoding is guessed. Bytes that
/// are not text in the encoding become U+FFFD.
fn decode(source: &[u8]) -> String {
    if let Some((encoding, length)) = Encoding::for_bom(source) {
        let (text, _) =
            encoding.decode_without_bom_handling(source.get(length..).unwrap_or_default());
        return text.into_owned();
    }
    let encoding = declared_encoding(source).unwrap_or_else(|| {
        let mut detector = EncodingDetector::new(Iso2022JpDetection::Deny);
        detector.feed(source, true);
        detector.guess(None, Utf8Detection::Allow)
    });
    let (text, _) = encoding.decode_without_bom_handling(source);
    text.into_owned()
}

/// The encoding that the start of a document names: in a `<meta>` element or
/// in an XML declaration.
fn declared_encoding(source: &[u8]) -> Option<&'static Encoding> {
    let head = source.get(..source.len().min(SNIFF_BYTES))?;
    let lower = String::from_utf8_lossy(head).to_ascii_lowercase();
    let label = lower
        .match_indices("<meta")
        .find_map(|(start, _)| {
            let tag = lower.get(start..)?;
            let tag = tag.get(..tag.find('>').unwrap_or(tag.len()))?;
            label_after(tag, "charset")
        })
        .or_else(|| {
            let declaration = lower.trim_start().strip_prefix("<?xml")?;
            let declaration = declaration.get(..declaration.find("?>")?)?;
            label_after(declaration, "encoding")
        })?;
    let encoding = Encoding::for_label(label.as_bytes())?;
    // A document that can be read as ASCII is not UTF-16.
    Some(if encoding == UTF_16LE || encoding == UTF_16BE {
        UTF_8
    } else {
        encoding
    })
}

/// The word after `name`, an equals sign and an optional quote.
fn label_after<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let after = text.get(text.find(name)? + name.len()..)?;
    let value = after.trim_start().strip_prefix('=')?.trim_start();
    let value = value.trim_start_matches(['"', '\'']);
    let end = value
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ':' | '.')))
        .unwrap_or(value.len());
    value.get(..end).filter(|label| !label.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{Store, data_address, declared_encoding, decode, label_after};
    use crate::draft::PictureData;
    use crate::error::Error;
    use crate::options::ConvertOptions;
    use encoding_rs::{Encoding, UTF_8, WINDOWS_1251};

    fn name(result: crate::error::Result<Option<String>>) -> Option<String> {
        result.unwrap()
    }

    fn holds(store: &Store, index: usize, expected: &[u8]) -> bool {
        matches!(
            store.pictures.get(index).map(|picture| &picture.data),
            Some(PictureData::Encoded(bytes)) if bytes == expected
        )
    }

    #[test]
    fn utf_8_is_read_as_it_is() {
        assert_eq!(
            decode("<p>Привет — «да»</p>".as_bytes()),
            "<p>Привет — «да»</p>"
        );
    }

    #[test]
    fn a_byte_order_mark_decides_and_is_dropped() {
        assert_eq!(decode(b"\xef\xbb\xbf<p>Hi</p>"), "<p>Hi</p>");
        let mut utf16 = vec![0xff, 0xfe];
        for unit in "<p>Hi</p>".encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(decode(&utf16), "<p>Hi</p>");
    }

    #[test]
    fn a_meta_charset_decides_over_the_bytes() {
        let (bytes, _, _) =
            WINDOWS_1251.encode("<html><head><meta charset=\"windows-1251\"></head><p>Я</p>");
        assert!(decode(&bytes).ends_with("<p>Я</p>"));
    }

    #[test]
    fn a_content_type_meta_names_the_encoding_too() {
        let html = b"<META HTTP-EQUIV='Content-Type' CONTENT='text/html; charset=ISO-8859-2'>";
        assert_eq!(declared_encoding(html), Encoding::for_label(b"iso-8859-2"));
    }

    #[test]
    fn an_xml_declaration_names_the_encoding() {
        let xml = b"\n<?xml version=\"1.0\" encoding=\"windows-1251\"?><html/>";
        assert_eq!(declared_encoding(xml), Some(WINDOWS_1251));
    }

    #[test]
    fn a_declared_utf_16_is_read_as_utf_8() {
        assert_eq!(declared_encoding(b"<meta charset=\"utf-16\">"), Some(UTF_8));
    }

    #[test]
    fn an_unknown_or_missing_declaration_is_no_encoding() {
        assert_eq!(
            declared_encoding(b"<meta charset=\"no-such\"><p>x</p>"),
            None
        );
        assert_eq!(declared_encoding(b"<p>charset=windows-1251</p>"), None);
        assert_eq!(declared_encoding(b""), None);
        assert_eq!(declared_encoding(b"<meta name=\"x\">"), None);
    }

    #[test]
    fn a_document_without_a_declaration_is_guessed() {
        let text = "<p>Привет, мир. Это текст в старой кодировке.</p>";
        let (bytes, _, _) = WINDOWS_1251.encode(text);
        assert_eq!(decode(&bytes), text);
    }

    #[test]
    fn a_label_is_the_word_after_the_name_and_an_equals_sign() {
        assert_eq!(
            label_after("charset = \"koi8-r\" x", "charset"),
            Some("koi8-r")
        );
        assert_eq!(label_after("charset=", "charset"), None);
        assert_eq!(label_after("charset", "charset"), None);
        assert_eq!(label_after("x", "charset"), None);
    }

    #[test]
    fn a_data_address_is_split_into_its_parameters_and_its_data() {
        assert_eq!(
            data_address(" DATA:image/png;base64,AAAA"),
            Some(("image/png;base64", "AAAA"))
        );
        assert_eq!(data_address("data:image/png;base64"), None);
        assert_eq!(data_address("image.png"), None);
        assert_eq!(data_address("da"), None);
    }

    fn store(one: u64, all: u64) -> Store {
        let mut options = ConvertOptions::default();
        options.limits.book.max_resource_bytes = one;
        options.limits.book.max_file_bytes = all;
        Store::new(&options)
    }

    #[test]
    fn base64_data_is_stored_under_a_numbered_name() {
        let mut store = store(100, 100);
        let first = name(store.add("data:image/png;base64,AQID"));
        let second = name(store.add("data:image/png;base64, BA\nU="));
        assert_eq!(first.as_deref(), Some("data-1"));
        assert_eq!(second.as_deref(), Some("data-2"));
        assert!(holds(&store, 1, &[4, 5]));
    }

    #[test]
    fn base64_without_padding_is_read() {
        let mut store = store(100, 100);
        assert_eq!(
            name(store.add("data:;base64,BAU")).as_deref(),
            Some("data-1")
        );
        assert!(holds(&store, 0, &[4, 5]));
    }

    #[test]
    fn percent_encoded_data_is_stored() {
        let mut store = store(100, 100);
        assert_eq!(
            name(store.add("data:image/gif,%01%02a")).as_deref(),
            Some("data-1")
        );
        assert!(holds(&store, 0, &[1, 2, b'a']));
    }

    #[test]
    fn the_same_data_is_one_picture() {
        let mut store = store(100, 100);
        assert_eq!(
            name(store.add("data:;base64,AQID")).as_deref(),
            Some("data-1")
        );
        assert_eq!(
            name(store.add("data:image/png;base64,AQID")).as_deref(),
            Some("data-1")
        );
        assert_eq!(store.pictures.len(), 1);
    }

    #[test]
    fn other_addresses_and_unreadable_data_are_not_stored() {
        let mut store = store(100, 100);
        assert_eq!(name(store.add("https://x.org/a.png")), None);
        assert_eq!(name(store.add("data:;base64,@@@@")), None);
        assert_eq!(name(store.add("data:;base64,")), None);
        assert!(store.pictures.is_empty());
    }

    #[test]
    fn a_picture_over_the_resource_limit_is_refused() {
        let mut store = store(3, 100);
        assert_eq!(
            name(store.add("data:;base64,AQID")).as_deref(),
            Some("data-1")
        );
        assert!(matches!(
            store.add("data:;base64,AQIDBA=="),
            Err(Error::Limit(_))
        ));
        assert!(matches!(
            store.add("data:;base64,AQIDBAUGBwgJCgsM"),
            Err(Error::Limit(_))
        ));
        // Four bytes without padding: the text is too short to tell.
        assert!(matches!(
            store.add("data:;base64,AQIDBA"),
            Err(Error::Limit(_))
        ));
        assert!(matches!(store.add("data:,abcd"), Err(Error::Limit(_))));
        assert!(matches!(
            store.add("data:,%01%02%03%04"),
            Err(Error::Limit(_))
        ));
    }

    #[test]
    fn pictures_over_the_file_limit_together_are_refused() {
        let mut store = store(10, 5);
        assert_eq!(
            name(store.add("data:;base64,AQID")).as_deref(),
            Some("data-1")
        );
        assert!(matches!(
            store.add("data:;base64,BAUG"),
            Err(Error::Limit(_))
        ));
    }
}
