//! Plain text, through the public API.
#![cfg(feature = "txt")]

use mbook_converter::mbook::{Book, BookReader};
use mbook_converter::{ConvertOptions, Converter, Error, Format, WarningKind};
use std::io::Cursor;

type Outcome = Result<(), Box<dyn std::error::Error>>;

fn text_converter() -> Converter {
    let mut options = ConvertOptions::default();
    options.format = Some(Format::Text);
    Converter::with_options(options)
}

const TEXT: &str = "Chapter 1\r\n\r\n# not a heading\r\n* not *a list*\r\n\r\nSecond paragraph, with <b>no html</b>.\r\n";

#[test]
fn text_becomes_one_chapter_of_escaped_paragraphs() -> Outcome {
    let converted = text_converter().load_slice(TEXT.as_bytes())?;
    let chapters = converted.book.chapters();
    assert_eq!(chapters.len(), 1);
    assert_eq!(
        chapters[0].text(),
        "Chapter 1\n\n\\# not a heading\n\\* not \\*a list\\*\n\nSecond paragraph, with \\<b\\>no html\\</b\\>.\n"
    );
    assert_eq!(chapters[0].anchors(), []);
    Ok(())
}

#[test]
fn text_has_no_metadata_of_its_own() -> Outcome {
    let converted = text_converter().load_slice(TEXT.as_bytes())?;
    let meta = converted.book.meta();
    assert_eq!((meta.title(), meta.language()), ("Untitled", "und"));
    assert!(meta.id().starts_with("urn:uuid:"));
    let kinds: Vec<_> = converted.report.warnings.iter().map(|w| &w.kind).collect();
    assert_eq!(
        kinds,
        [
            &WarningKind::MetadataMissing("title".into()),
            &WarningKind::MetadataMissing("lang".into())
        ]
    );
    assert_eq!(converted.report.format, Format::Text);
    Ok(())
}

#[test]
fn a_reader_is_converted_like_a_slice() -> Outcome {
    let from_slice = text_converter().load_slice(TEXT.as_bytes())?;
    let from_reader = text_converter().load(Cursor::new(TEXT))?;
    assert_eq!(from_reader, from_slice);
    Ok(())
}

#[test]
fn a_conversion_writes_an_mbook_file() -> Outcome {
    let mut file = Vec::new();
    let report = text_converter().convert(TEXT.as_bytes(), &mut file)?;
    let mut sliced = Vec::new();
    text_converter().convert_slice(TEXT.as_bytes(), &mut sliced)?;
    assert!(file == sliced);

    let mut reader = BookReader::new(Cursor::new(file))?;
    let first = reader.chapters().next().map(|chapter| chapter.id);
    let text = match first {
        Some(id) => reader.read_chapter(id)?,
        None => String::new(),
    };
    assert!(text.starts_with("Chapter 1\n"));
    assert_eq!(report.warnings.len(), 2);
    Ok(())
}

#[test]
fn converting_twice_gives_the_same_file() -> Outcome {
    let convert = || {
        let mut file = Vec::new();
        text_converter()
            .convert_slice(TEXT.as_bytes(), &mut file)
            .map(|_| file)
    };
    assert!(convert()? == convert()?);
    Ok(())
}

#[test]
fn a_converted_book_can_be_corrected_before_it_is_written() -> Outcome {
    let mut converted = text_converter().load_slice(TEXT.as_bytes())?;
    converted.book.meta_mut().set_title("The Example")?;
    let mut file = Vec::new();
    converted.book.write(&mut file)?;
    assert_eq!(Book::read(file.as_slice())?.meta().title(), "The Example");
    Ok(())
}

#[test]
fn text_is_not_detected() {
    let result = Converter::new().load_slice(TEXT.as_bytes());
    assert!(matches!(result, Err(Error::UnknownFormat)));
}

#[test]
fn empty_text_holds_nothing_to_convert() {
    let result = text_converter().load_slice(b"  \n\n");
    assert!(matches!(result, Err(Error::Empty)));
}

#[test]
fn a_source_above_the_limit_is_refused() {
    let mut options = ConvertOptions::default();
    options.format = Some(Format::Text);
    options.limits.max_source_bytes = 10;
    let converter = Converter::with_options(options);
    assert!(matches!(
        converter.load_slice(TEXT.as_bytes()),
        Err(Error::Limit(_))
    ));
    assert!(matches!(
        converter.load(Cursor::new(TEXT)),
        Err(Error::Limit(_))
    ));
    assert!(converter.load_slice(b"Short.").is_ok());
}

#[test]
fn a_failing_sink_is_an_io_error() {
    struct Closed;
    impl std::io::Write for Closed {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("closed"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let result = text_converter().convert_slice(TEXT.as_bytes(), Closed);
    assert!(matches!(result, Err(Error::Io(_))));
}

#[test]
fn a_failing_source_is_an_io_error() {
    struct Broken;
    impl std::io::Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("broken"))
        }
    }
    assert!(matches!(text_converter().load(Broken), Err(Error::Io(_))));
}

#[test]
fn a_format_whose_feature_is_off_is_named() {
    let disabled = Format::ALL.iter().find(|format| !format.is_enabled());
    let Some(&format) = disabled else {
        return;
    };
    let mut options = ConvertOptions::default();
    options.format = Some(format);
    let result = Converter::with_options(options).load_slice(b"anything");
    assert!(matches!(result, Err(Error::FormatDisabled(named)) if named == format));
}
