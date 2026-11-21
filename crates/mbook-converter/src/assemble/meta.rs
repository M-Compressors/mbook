//! The metadata of a draft, brought into the form the format wants.

use crate::draft::DraftMeta;
use crate::report::{Warning, WarningKind};
use mbook_core::Book;
use uuid::Uuid;

/// The namespace of the identifiers computed from the bytes of a source.
const NAMESPACE: Uuid = Uuid::from_u128(0x6f0c_7d3a_5b1e_4c58_9f3d_2a8e_4b7c_1d90);

/// What a book without a title is called.
pub(crate) const UNTITLED: &str = "Untitled";
/// The language tag of a book whose language is not known.
pub(crate) const UNDETERMINED: &str = "und";

/// Fills the metadata of a book from a draft.
///
/// `heading` is the first top-level heading of the text, which names a book
/// whose source has no title. A value that `mbook-core` refuses is left out
/// with a warning; nothing here fails a conversion.
pub(crate) fn apply(
    book: &mut Book,
    meta: DraftMeta,
    heading: Option<String>,
    source: &[u8],
    warnings: &mut Vec<Warning>,
) {
    let mut missing = |key: &str| {
        warnings.push(Warning::whole_book(WarningKind::MetadataMissing(
            key.to_owned(),
        )));
    };
    let title = present(meta.title).or(heading).unwrap_or_else(|| {
        missing("title");
        UNTITLED.to_owned()
    });
    let mut languages = meta
        .languages
        .into_iter()
        .filter_map(|tag| present(Some(tag)));
    let language = languages.next().unwrap_or_else(|| {
        missing("lang");
        UNDETERMINED.to_owned()
    });
    let id = present(meta.id);
    let isbn_in_id = id.as_deref().and_then(isbn);
    let id = id.map_or_else(|| computed_id(source), |id| stable_id(&id));

    let mut dropped = Vec::new();
    let mut keep = |key: &str, kept: bool| {
        if !kept {
            dropped.push(key.to_owned());
        }
    };
    let meta_out = book.meta_mut();
    keep("title", meta_out.set_title(title).is_ok());
    keep("lang", meta_out.set_language(language).is_ok());
    for tag in languages {
        keep("lang", meta_out.add("lang", tag).is_ok());
    }
    keep("id", meta_out.set_id(id).is_ok());
    for name in meta
        .authors
        .into_iter()
        .filter_map(|name| present(Some(name)))
    {
        keep("author", meta_out.add_author(name).is_ok());
    }
    for name in meta
        .translators
        .into_iter()
        .filter_map(|name| present(Some(name)))
    {
        keep("translator", meta_out.add_translator(name).is_ok());
    }
    if let Some(series) = present(meta.series) {
        let index = present(meta.series_index);
        keep(
            "series",
            meta_out.set_series(series, index.as_deref()).is_ok(),
        );
    }
    if let Some(publisher) = present(meta.publisher) {
        keep("publisher", meta_out.set_publisher(publisher).is_ok());
    }
    if let Some(written) = present(meta.date) {
        match date(&written) {
            Some(date) => keep("date", meta_out.set_date(date).is_ok()),
            None => keep("date", false),
        }
    }
    let mut isbns: Vec<String> = Vec::new();
    for written in meta.isbns.iter().map(String::as_str) {
        match isbn(written) {
            Some(digits) => isbns.push(digits),
            None => keep("isbn", false),
        }
    }
    isbns.extend(isbn_in_id);
    isbns.dedup();
    for digits in isbns {
        keep("isbn", meta_out.add_isbn(digits).is_ok());
    }
    for genre in meta
        .genres
        .into_iter()
        .filter_map(|genre| present(Some(genre)))
    {
        keep("genre", meta_out.add_genre(genre).is_ok());
    }
    if let Some(annotation) = present(meta.annotation) {
        keep("annotation", meta_out.set_annotation(annotation).is_ok());
    }
    warnings.extend(
        dropped
            .into_iter()
            .map(|key| Warning::whole_book(WarningKind::MetadataDropped(key))),
    );
}

/// A value without the white space around it; `None` when nothing is left.
fn present(value: Option<String>) -> Option<String> {
    let value = value?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else if trimmed.len() == value.len() {
        Some(value)
    } else {
        Some(trimmed.to_owned())
    }
}

/// The identifier of a source: a UUID in the form the format recommends,
/// anything else as it is.
fn stable_id(id: &str) -> String {
    let bare = id
        .get(..9)
        .filter(|prefix| prefix.eq_ignore_ascii_case("urn:uuid:"))
        .map_or(id, |_| &id[9..]);
    match Uuid::try_parse(bare) {
        Ok(uuid) => format!("urn:uuid:{}", uuid.as_hyphenated()),
        Err(_) => id.to_owned(),
    }
}

/// An identifier for a source that has none: the same for the same bytes.
fn computed_id(source: &[u8]) -> String {
    format!(
        "urn:uuid:{}",
        Uuid::new_v5(&NAMESPACE, source).as_hyphenated()
    )
}

/// A date as `YYYY`, `YYYY-MM` or `YYYY-MM-DD`, cut from text that starts
/// with one. `None` when it does not start with a year.
fn date(written: &str) -> Option<String> {
    let bytes = written.as_bytes();
    let digits = |range: std::ops::Range<usize>| {
        bytes
            .get(range)
            .is_some_and(|part| part.iter().all(u8::is_ascii_digit))
    };
    let part = |at: usize, max: u8| {
        let value = written.get(at + 1..at + 3)?.parse::<u8>().ok()?;
        (bytes.get(at) == Some(&b'-') && digits(at + 1..at + 3) && (1..=max).contains(&value))
            .then_some(at + 3)
    };
    if !digits(0..4) || digits(4..5) {
        return None;
    }
    let end = match part(4, 12) {
        Some(month_end) => part(month_end, 31).unwrap_or(month_end),
        None => 4,
    };
    written.get(..end).map(str::to_owned)
}

/// An ISBN as its digits: ten or thirteen characters, the last of which may
/// be `X` in the short form. `None` for anything else.
fn isbn(written: &str) -> Option<String> {
    let bare = written
        .get(..9)
        .filter(|prefix| prefix.eq_ignore_ascii_case("urn:isbn:"))
        .map_or(written, |_| &written[9..]);
    let bare = bare
        .get(..4)
        .filter(|prefix| prefix.eq_ignore_ascii_case("isbn"))
        .map_or(bare, |_| bare[4..].trim_start_matches([':', ' ']));
    if !bare
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '-' | ' ' | 'x' | 'X'))
    {
        return None;
    }
    let digits: String = bare
        .chars()
        .filter(|c| !matches!(c, '-' | ' '))
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let numeric = |text: &str| text.bytes().all(|byte| byte.is_ascii_digit());
    let valid = match digits.len() {
        13 => numeric(&digits),
        10 => numeric(&digits[..9]),
        _ => false,
    };
    valid.then_some(digits)
}

#[cfg(test)]
mod tests {
    use super::{apply, computed_id, date, isbn, stable_id};
    use crate::draft::DraftMeta;
    use crate::report::{Warning, WarningKind};
    use mbook_core::Book;

    fn applied(meta: DraftMeta, heading: Option<&str>) -> (Book, Vec<WarningKind>) {
        let mut book = Book::new("x", "x").unwrap();
        let mut warnings: Vec<Warning> = Vec::new();
        apply(
            &mut book,
            meta,
            heading.map(str::to_owned),
            b"source",
            &mut warnings,
        );
        (book, warnings.into_iter().map(|w| w.kind).collect())
    }

    #[test]
    fn every_value_of_a_draft_reaches_the_book() {
        let meta = DraftMeta {
            title: Some(" The Example ".to_owned()),
            languages: vec!["en".into(), "fr".into()],
            id: Some("3F1C2A7E-8A54-4D0B-9C1E-6B2F0D9A7E11".to_owned()),
            authors: vec!["A. Writer".into(), "  ".into()],
            translators: vec!["T. Ranslator".into()],
            series: Some("The Series".to_owned()),
            series_index: Some("2".to_owned()),
            publisher: Some("Press".to_owned()),
            date: Some("2001-05-17T10:00:00Z".to_owned()),
            isbns: vec!["978-3-16-148410-0".into()],
            genres: vec!["prose".into()],
            annotation: Some("About.".to_owned()),
        };
        let (book, warnings) = applied(meta, None);
        let meta = book.meta();
        assert_eq!(warnings, []);
        assert_eq!((meta.title(), meta.language()), ("The Example", "en"));
        assert_eq!(meta.languages().collect::<Vec<_>>(), ["en", "fr"]);
        assert_eq!(meta.id(), "urn:uuid:3f1c2a7e-8a54-4d0b-9c1e-6b2f0d9a7e11");
        assert_eq!(meta.authors().collect::<Vec<_>>(), ["A. Writer"]);
        assert_eq!(meta.translators().collect::<Vec<_>>(), ["T. Ranslator"]);
        assert_eq!(
            meta.series().map(|s| (s.name, s.index)),
            Some(("The Series", Some("2")))
        );
        assert_eq!(
            (meta.publisher(), meta.date(), meta.annotation()),
            (Some("Press"), Some("2001-05-17"), Some("About."))
        );
        assert_eq!(meta.isbns().collect::<Vec<_>>(), ["9783161484100"]);
        assert_eq!(meta.genres().collect::<Vec<_>>(), ["prose"]);
    }

    #[test]
    fn a_source_without_a_title_is_named_by_its_first_heading() {
        let (book, warnings) = applied(DraftMeta::default(), Some("Chapter One"));
        assert_eq!(book.meta().title(), "Chapter One");
        assert_eq!(warnings, [WarningKind::MetadataMissing("lang".into())]);
    }

    #[test]
    fn a_source_without_title_and_language_gets_stand_ins() {
        let (book, warnings) = applied(DraftMeta::default(), None);
        assert_eq!(
            (book.meta().title(), book.meta().language()),
            ("Untitled", "und")
        );
        assert_eq!(
            warnings,
            [
                WarningKind::MetadataMissing("title".into()),
                WarningKind::MetadataMissing("lang".into())
            ]
        );
    }

    #[test]
    fn a_source_without_an_identifier_gets_one_from_its_bytes() {
        let (book, _) = applied(DraftMeta::default(), None);
        assert_eq!(book.meta().id(), computed_id(b"source"));
        assert_ne!(computed_id(b"source"), computed_id(b"another"));
        assert!(computed_id(b"").starts_with("urn:uuid:"));
    }

    #[test]
    fn an_identifier_that_is_a_uuid_is_written_as_a_urn() {
        assert_eq!(
            stable_id("URN:UUID:3F1C2A7E-8A54-4D0B-9C1E-6B2F0D9A7E11"),
            "urn:uuid:3f1c2a7e-8a54-4d0b-9c1e-6b2f0d9a7e11"
        );
        assert_eq!(
            stable_id("http://example.org/book"),
            "http://example.org/book"
        );
        assert_eq!(stable_id("urn:uuid:not-one"), "urn:uuid:not-one");
    }

    #[test]
    fn an_identifier_that_is_an_isbn_also_fills_the_isbn() {
        let meta = DraftMeta {
            id: Some("urn:isbn:9783161484100".to_owned()),
            isbns: vec!["9783161484100".into()],
            ..DraftMeta::default()
        };
        let (book, _) = applied(meta, None);
        assert_eq!(book.meta().id(), "urn:isbn:9783161484100");
        assert_eq!(book.meta().isbns().collect::<Vec<_>>(), ["9783161484100"]);
    }

    #[test]
    fn values_the_format_does_not_allow_are_dropped_with_a_warning() {
        let meta = DraftMeta {
            date: Some("spring of 1999".to_owned()),
            isbns: vec!["12345".into()],
            ..DraftMeta::default()
        };
        let (book, warnings) = applied(meta, Some("T"));
        assert_eq!((book.meta().date(), book.meta().isbns().count()), (None, 0));
        assert!(warnings.contains(&WarningKind::MetadataDropped("date".into())));
        assert!(warnings.contains(&WarningKind::MetadataDropped("isbn".into())));
    }

    #[test]
    fn dates_are_cut_to_the_precision_they_have() {
        let dates = [
            "1999",
            "1999-05",
            "1999-05-17",
            "1999-05-17T10:00",
            "1999-13",
            "1999-05-40",
        ]
        .map(date);
        assert_eq!(
            dates,
            [
                Some("1999".to_owned()),
                Some("1999-05".to_owned()),
                Some("1999-05-17".to_owned()),
                Some("1999-05-17".to_owned()),
                Some("1999".to_owned()),
                Some("1999-05".to_owned())
            ]
        );
    }

    #[test]
    fn text_that_does_not_start_with_a_year_is_not_a_date() {
        assert_eq!(
            ["", "99", "May 1999", "19990517"].map(date),
            [None, None, None, None]
        );
    }

    #[test]
    fn an_isbn_is_reduced_to_its_digits() {
        assert_eq!(isbn("ISBN 0-306-40615-X").as_deref(), Some("030640615X"));
        assert_eq!(
            isbn("isbn:978 3 16 148410 0").as_deref(),
            Some("9783161484100")
        );
        assert_eq!(
            isbn("urn:isbn:9783161484100").as_deref(),
            Some("9783161484100")
        );
    }

    #[test]
    fn text_that_is_not_an_isbn_is_refused() {
        assert_eq!(
            [
                "",
                "12345",
                "97831614841000",
                "X306406152",
                "doi:10.1000/182"
            ]
            .map(isbn),
            [None, None, None, None, None]
        );
    }
}
