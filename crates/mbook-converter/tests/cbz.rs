//! Comic book archives, through the public API.
#![cfg(feature = "cbz")]

use image::{ImageFormat, Rgb, RgbImage};
use mbook_converter::mbook::Book;
use mbook_converter::{ConvertOptions, Converted, Converter, Error, Format, WarningKind};
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

type Outcome = Result<(), Box<dyn std::error::Error>>;

/// A converter that reads its source as a comic book archive.
fn cbz_converter() -> Converter {
    converter_with(|_| {})
}

/// A converter for comic book archives with changed options.
fn converter_with(change: impl FnOnce(&mut ConvertOptions)) -> Converter {
    let mut options = ConvertOptions::default();
    options.format = Some(Format::Cbz);
    change(&mut options);
    Converter::with_options(options)
}

/// The pixel size of page `n`; every page of a test has its own.
fn size(n: u32) -> (u32, u32) {
    (10 + n, 6 + n)
}

/// The picture of page `n` in `format`.
fn picture(n: u32, format: ImageFormat) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let (width, height) = size(n);
    let image = RgbImage::from_pixel(width, height, Rgb([20 * (n % 10) as u8, 90, 160]));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, format)?;
    Ok(bytes.into_inner())
}

/// The PNG of page `n`.
fn page(n: u32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    picture(n, ImageFormat::Png)
}

/// A ZIP archive with these entries; a name that ends with `/` is a folder.
fn archive(entries: &[(&str, &[u8])]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    archive_with(entries, SimpleFileOptions::default())
}

/// A ZIP archive written with these options.
fn archive_with(
    entries: &[(&str, &[u8])],
    options: SimpleFileOptions,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in entries {
        if name.ends_with('/') {
            writer.add_directory(*name, options)?;
        } else {
            writer.start_file(*name, options)?;
            writer.write_all(content)?;
        }
    }
    Ok(writer.finish()?.into_inner())
}

/// Replaces the four bytes at `offset` after every `signature` in `bytes`.
fn patch(bytes: &mut [u8], signature: &[u8], offset: usize, value: [u8; 4]) {
    let starts: Vec<usize> = (0..bytes.len())
        .filter(|&start| bytes[start..].starts_with(signature))
        .collect();
    for start in starts {
        if let Some(field) = bytes.get_mut(start + offset..start + offset + 4) {
            field.copy_from_slice(&value);
        }
    }
}

/// The paths and pixel sizes of the pictures of a book.
fn pictures(book: &Book) -> Vec<(String, (u32, u32))> {
    book.pictures()
        .iter()
        .map(|picture| {
            let size = picture.size();
            (picture.path().to_owned(), (size.width, size.height))
        })
        .collect()
}

/// The texts of the chapters of a book.
fn texts(book: &Book) -> Vec<&str> {
    book.chapters()
        .iter()
        .map(mbook_converter::mbook::Chapter::text)
        .collect()
}

/// The kinds of the warnings of a conversion, except the stand-ins for
/// metadata that a comic without `ComicInfo.xml` lacks.
fn kinds(converted: &Converted) -> Vec<&WarningKind> {
    let missing = |kind: &&WarningKind| !matches!(kind, WarningKind::MetadataMissing(_));
    let all = converted.report.warnings.iter().map(|w| &w.kind);
    all.filter(missing).collect()
}

const COMIC_INFO: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
<ComicInfo>\n\
  <Title>The Example</Title>\n\
  <Series>Examples</Series>\n\
  <Number>2.5</Number>\n\
  <Writer>Ann Author, Bob Writer</Writer>\n\
  <Translator>Carl Translator</Translator>\n\
  <Publisher>Example Press</Publisher>\n\
  <Year>2020</Year>\n\
  <Month>3</Month>\n\
  <Day>7</Day>\n\
  <LanguageISO>ja</LanguageISO>\n\
  <Genre>Action, Drama</Genre>\n\
  <Summary>First &amp; last.\n\nSecond paragraph.</Summary>\n\
  <Pages><Page Image=\"0\" /></Pages>\n\
</ComicInfo>";

#[test]
fn an_archive_without_folders_is_one_chapter_in_natural_order() -> Outcome {
    let source = archive(&[
        ("10.png", &page(10)?),
        ("2.png", &page(2)?),
        ("1.png", &page(1)?),
    ])?;
    let converted = cbz_converter().load_slice(&source)?;
    assert_eq!(
        texts(&converted.book),
        ["![](img/cover.avif)\n\n![](img/0001.avif)\n\n![](img/0002.avif)\n"]
    );
    assert_eq!(converted.report.format, Format::Cbz);
    Ok(())
}

#[test]
fn stored_pictures_keep_their_pixel_sizes_in_reading_order() -> Outcome {
    let source = archive(&[
        ("10.png", &page(10)?),
        ("2.jpg", &picture(2, ImageFormat::Jpeg)?),
        ("1.png", &page(1)?),
    ])?;
    let converted = cbz_converter().load_slice(&source)?;
    assert_eq!(
        pictures(&converted.book),
        [
            ("img/cover.avif".to_owned(), size(1)),
            ("img/0001.avif".to_owned(), size(2)),
            ("img/0002.avif".to_owned(), size(10)),
        ]
    );
    assert_eq!(kinds(&converted), [] as [&WarningKind; 0]);
    Ok(())
}

#[test]
fn the_first_page_is_the_cover_and_the_first_figure() -> Outcome {
    let source = archive(&[("img/b.png", &page(2)?), ("img/a.png", &page(1)?)])?;
    let converted = cbz_converter().load_slice(&source)?;
    assert_eq!(converted.book.meta().cover(), Some("img/cover.avif"));
    assert_eq!(
        texts(&converted.book),
        ["# img\n\n![](img/cover.avif)\n\n![](img/0001.avif)\n"]
    );
    Ok(())
}

#[test]
fn every_folder_with_pages_is_a_chapter_with_a_heading() -> Outcome {
    let source = archive(&[
        ("vol10/1.png", &page(4)?),
        ("vol2/2.png", &page(2)?),
        ("vol2/1.png", &page(1)?),
        ("vol2/", &[]),
    ])?;
    let converted = cbz_converter().load_slice(&source)?;
    assert_eq!(
        texts(&converted.book),
        [
            "# vol2\n\n![](img/cover.avif)\n\n![](img/0001.avif)\n",
            "# vol10\n\n![](img/0002.avif)\n"
        ]
    );
    Ok(())
}

#[test]
fn a_nested_folder_is_headed_by_its_last_component() -> Outcome {
    let source = archive(&[
        ("a/b/c/3.png", &page(3)?),
        ("a/b/2.png", &page(2)?),
        ("a/1.png", &page(1)?),
    ])?;
    let converted = cbz_converter().load_slice(&source)?;
    assert_eq!(
        texts(&converted.book),
        [
            "# a\n\n![](img/cover.avif)\n",
            "# b\n\n![](img/0001.avif)\n",
            "# c\n\n![](img/0002.avif)\n"
        ]
    );
    Ok(())
}

#[test]
fn pages_in_the_root_form_a_first_chapter_without_a_heading() -> Outcome {
    let source = archive(&[
        ("vol1/1.png", &page(3)?),
        ("cover.png", &page(1)?),
        ("credits.png", &page(2)?),
    ])?;
    let converted = cbz_converter().load_slice(&source)?;
    assert_eq!(
        texts(&converted.book),
        [
            "![](img/cover.avif)\n\n![](img/0001.avif)\n",
            "# vol1\n\n![](img/0002.avif)\n"
        ]
    );
    Ok(())
}

#[test]
fn a_folder_called_pages_does_not_collide_with_the_root() -> Outcome {
    let source = archive(&[("pages/1.png", &page(2)?), ("1.png", &page(1)?)])?;
    let converted = cbz_converter().load_slice(&source)?;
    assert_eq!(
        texts(&converted.book),
        ["![](img/cover.avif)\n", "# pages\n\n![](img/0001.avif)\n"]
    );
    Ok(())
}

#[test]
fn a_folder_name_with_reserved_characters_is_converted() -> Outcome {
    let source = archive(&[("a#b:c/1.png", &page(1)?), ("a_b_c/1.png", &page(2)?)])?;
    let converted = cbz_converter().load_slice(&source)?;
    assert_eq!(
        texts(&converted.book),
        [
            "# a#b:c\n\n![](img/cover.avif)\n",
            "# a\\_b\\_c\n\n![](img/0001.avif)\n"
        ]
    );
    Ok(())
}

#[test]
fn markdown_in_a_folder_name_is_escaped() -> Outcome {
    let source = archive(&[("# *Vol* [1]/1.png", &page(1)?)])?;
    let converted = cbz_converter().load_slice(&source)?;
    assert_eq!(
        texts(&converted.book),
        ["# \\# \\*Vol\\* \\[1\\]\n\n![](img/cover.avif)\n"]
    );
    Ok(())
}

#[test]
fn a_page_name_with_spaces_is_written_as_an_encoded_destination() -> Outcome {
    let source = archive(&[("My Book/page 1.png", &page(1)?)])?;
    let converted = cbz_converter().load_slice(&source)?;
    assert_eq!(
        pictures(&converted.book),
        [("img/cover.avif".to_owned(), size(1))]
    );
    assert_eq!(kinds(&converted), [] as [&WarningKind; 0]);
    Ok(())
}

#[test]
fn extensions_are_matched_in_any_letter_case() -> Outcome {
    let source = archive(&[
        ("1.PNG", &page(1)?),
        ("2.Jpg", &picture(2, ImageFormat::Jpeg)?),
    ])?;
    let converted = cbz_converter().load_slice(&source)?;
    assert_eq!(converted.book.pictures().len(), 2);
    Ok(())
}

#[test]
fn comic_info_fills_the_metadata() -> Outcome {
    let source = archive(&[
        ("1.png", &page(1)?),
        ("ComicInfo.xml", COMIC_INFO.as_bytes()),
    ])?;
    let converted = cbz_converter().load_slice(&source)?;
    let meta = converted.book.meta();
    assert_eq!(meta.title(), "The Example");
    let series = meta.series().map(|series| (series.name, series.index));
    assert_eq!(series, Some(("Examples", Some("2.5"))));
    assert_eq!(
        meta.authors().collect::<Vec<_>>(),
        ["Ann Author", "Bob Writer"]
    );
    assert_eq!(meta.translators().collect::<Vec<_>>(), ["Carl Translator"]);
    assert_eq!(meta.publisher(), Some("Example Press"));
    assert_eq!(meta.date(), Some("2020-03-07"));
    assert_eq!(meta.language(), "ja");
    assert_eq!(meta.genres().collect::<Vec<_>>(), ["Action", "Drama"]);
    assert_eq!(
        meta.annotation(),
        Some("First & last.\n\nSecond paragraph.")
    );
    assert_eq!(kinds(&converted), [] as [&WarningKind; 0]);
    Ok(())
}

#[test]
fn the_date_is_as_precise_as_the_fields_that_are_there() -> Outcome {
    let cases = [
        (
            "<Year>987</Year><Month>1</Month><Day>2</Day>",
            Some("0987-01-02"),
        ),
        ("<Year>2020</Year><Month>12</Month>", Some("2020-12")),
        ("<Year>2020</Year><Day>5</Day>", Some("2020")),
        (
            "<Year>2020</Year><Month>13</Month><Day>5</Day>",
            Some("2020"),
        ),
        (
            "<Year>2020</Year><Month>2</Month><Day>32</Day>",
            Some("2020-02"),
        ),
        ("<Year>-1</Year><Month>2</Month>", None),
        ("<Month>2</Month>", None),
    ];
    for (fields, expected) in cases {
        let xml = format!("<ComicInfo>{fields}</ComicInfo>");
        let source = archive(&[("1.png", &page(1)?), ("ComicInfo.xml", xml.as_bytes())])?;
        let converted = cbz_converter().load_slice(&source)?;
        assert_eq!(converted.book.meta().date(), expected, "{fields}");
    }
    Ok(())
}

#[test]
fn comic_info_is_found_in_any_letter_case_and_only_in_the_root() -> Outcome {
    let found = archive(&[
        ("1.png", &page(1)?),
        (
            "comicinfo.XML",
            b"<ComicInfo><Title>Root</Title></ComicInfo>",
        ),
    ])?;
    let missed = archive(&[
        ("1.png", &page(1)?),
        (
            "vol/ComicInfo.xml",
            b"<ComicInfo><Title>Deep</Title></ComicInfo>",
        ),
    ])?;
    assert_eq!(
        cbz_converter().load_slice(&found)?.book.meta().title(),
        "Root"
    );
    assert_eq!(
        cbz_converter().load_slice(&missed)?.book.meta().title(),
        "Untitled"
    );
    Ok(())
}

#[test]
fn a_comic_info_that_is_not_well_formed_is_dropped_with_a_warning() -> Outcome {
    for broken in [
        &b"<ComicInfo><Title>Oops</ComicInfo>"[..],
        b"not xml",
        b"\xff\xfe",
    ] {
        let source = archive(&[("1.png", &page(1)?), ("ComicInfo.xml", broken)])?;
        let converted = cbz_converter().load_slice(&source)?;
        let warning = converted.report.warnings.first().ok_or("a warning")?;
        assert_eq!(
            (&warning.kind, &warning.location),
            (&WarningKind::MetadataDropped("ComicInfo.xml".into()), &None)
        );
        assert_eq!(converted.book.meta().title(), "Untitled");
        assert_eq!(converted.book.chapters().len(), 1);
    }
    Ok(())
}

#[test]
fn entries_that_are_not_pages_are_ignored() -> Outcome {
    let source = archive(&[
        ("__MACOSX/", &[]),
        ("__MACOSX/1.png", b"resource fork"),
        ("__MACOSX/vol/._2.png", b"resource fork"),
        (".hidden.png", b"hidden"),
        ("vol/.hidden.png", b"hidden"),
        ("readme.txt", b"Read me."),
        ("vol/notes.txt", b"Notes."),
        ("1.png", &page(1)?),
    ])?;
    let converted = cbz_converter().load_slice(&source)?;
    assert_eq!(texts(&converted.book), ["![](img/cover.avif)\n"]);
    assert_eq!(converted.book.pictures().len(), 1);
    assert_eq!(kinds(&converted), [] as [&WarningKind; 0]);
    Ok(())
}

#[test]
fn a_page_that_is_not_a_picture_is_dropped_with_a_warning() -> Outcome {
    let source = archive(&[("1.png", &page(1)?), ("2.png", b"this is not a picture")])?;
    let converted = cbz_converter().load_slice(&source)?;
    assert_eq!(converted.book.pictures().len(), 1);
    let warning = converted.report.warnings.first().ok_or("a warning")?;
    assert_eq!(warning.kind, WarningKind::PictureDropped("2.png".into()));
    assert_eq!(warning.location.as_deref(), Some("pages"));
    assert_eq!(kinds(&converted).len(), 1);
    Ok(())
}

#[test]
fn a_cover_that_is_not_a_picture_is_reported_too() -> Outcome {
    let source = archive(&[("1.png", b"this is not a picture"), ("2.png", &page(2)?)])?;
    let converted = cbz_converter().load_slice(&source)?;
    assert_eq!(converted.book.meta().cover(), None);
    assert_eq!(
        kinds(&converted),
        [
            &WarningKind::PictureDropped("1.png".into()),
            &WarningKind::PictureDropped("1.png".into())
        ]
    );
    Ok(())
}

#[test]
fn a_comic_is_detected_from_its_content() -> Outcome {
    let source = archive(&[
        ("vol1/1.png", &page(1)?),
        ("vol1/2.png", &page(2)?),
        ("ComicInfo.xml", COMIC_INFO.as_bytes()),
    ])?;
    let converted = Converter::new().load_slice(&source)?;
    assert_eq!(converted.report.format, Format::Cbz);
    assert_eq!(converted.book.meta().title(), "The Example");
    Ok(())
}

#[test]
fn a_conversion_writes_an_mbook_file() -> Outcome {
    let source = archive(&[("1.png", &page(1)?)])?;
    let mut file = Vec::new();
    let report = cbz_converter().convert_slice(&source, &mut file)?;
    assert_eq!(report.format, Format::Cbz);
    assert_eq!(Book::read(file.as_slice())?.chapters().len(), 1);
    Ok(())
}

#[test]
fn a_source_that_is_not_a_zip_archive_is_a_source_error() {
    let result = cbz_converter().load_slice(b"definitely not a ZIP archive");
    assert!(matches!(
        result,
        Err(Error::Source {
            format: Format::Cbz,
            ..
        })
    ));
}

#[test]
fn an_archive_without_pages_is_empty() -> Outcome {
    let source = archive(&[
        ("readme.txt", b"Read me."),
        ("ComicInfo.xml", COMIC_INFO.as_bytes()),
        ("vol/", &[]),
    ])?;
    let result = cbz_converter().load_slice(&source);
    assert!(matches!(result, Err(Error::Empty)));
    Ok(())
}

#[test]
fn an_archive_without_entries_is_empty() -> Outcome {
    let result = cbz_converter().load_slice(&archive(&[])?);
    assert!(matches!(result, Err(Error::Empty)));
    Ok(())
}

#[test]
fn an_encrypted_entry_makes_the_source_protected() -> Outcome {
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let mut source = archive_with(&[("1.png", &page(1)?)], stored)?;
    // Bit 0 of the general purpose flags marks an encrypted entry.
    patch(&mut source, b"PK\x03\x04", 6, [1, 0, 0, 0]);
    patch(&mut source, b"PK\x01\x02", 8, [1, 0, 0, 0]);
    let result = cbz_converter().load_slice(&source);
    assert!(matches!(result, Err(Error::Protected(Format::Cbz))));
    Ok(())
}

#[test]
fn a_damaged_entry_is_a_source_error() -> Outcome {
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let mut source = archive_with(&[("1.png", &page(1)?)], stored)?;
    // The checksum of the entry in the local and the central header.
    patch(&mut source, b"PK\x03\x04", 14, [1, 2, 3, 4]);
    patch(&mut source, b"PK\x01\x02", 16, [1, 2, 3, 4]);
    let result = cbz_converter().load_slice(&source);
    assert!(matches!(
        result,
        Err(Error::Source {
            format: Format::Cbz,
            ..
        })
    ));
    Ok(())
}

#[test]
fn an_entry_with_an_unknown_compression_is_a_source_error() -> Outcome {
    let mut source = archive(&[("1.png", &page(1)?)])?;
    // The compression method (and the time, which does not matter).
    patch(&mut source, b"PK\x03\x04", 8, [99, 0, 0, 0]);
    patch(&mut source, b"PK\x01\x02", 10, [99, 0, 0, 0]);
    let result = cbz_converter().load_slice(&source);
    assert!(matches!(
        result,
        Err(Error::Source {
            format: Format::Cbz,
            ..
        })
    ));
    Ok(())
}

#[test]
fn an_entry_over_the_resource_limit_is_refused() -> Outcome {
    let big = page(1)?;
    let source = archive(&[("1.png", &big)])?;
    let limit = u64::try_from(big.len())?;
    let refused = converter_with(|options| options.limits.book.max_resource_bytes = limit - 1)
        .load_slice(&source);
    assert!(matches!(refused, Err(Error::Limit(_))));
    let accepted = converter_with(|options| options.limits.book.max_resource_bytes = limit)
        .load_slice(&source);
    assert!(accepted.is_ok());
    Ok(())
}

#[test]
fn comic_info_is_bound_by_the_resource_limit_too() -> Outcome {
    let source = archive(&[
        ("1.png", &page(1)?),
        ("ComicInfo.xml", COMIC_INFO.as_bytes()),
    ])?;
    let refused =
        converter_with(|options| options.limits.book.max_resource_bytes = 100).load_slice(&source);
    assert!(matches!(refused, Err(Error::Limit(_))));
    Ok(())
}

#[test]
fn a_highly_compressed_entry_is_refused_at_the_limit() -> Outcome {
    let zeros = vec![0_u8; 1 << 20];
    let source = archive(&[("1.png", &zeros)])?;
    assert!(source.len() < 10_000);
    let refused =
        converter_with(|options| options.limits.book.max_resource_bytes = 1000).load_slice(&source);
    assert!(matches!(refused, Err(Error::Limit(_))));
    Ok(())
}

#[test]
fn an_entry_that_declares_less_than_it_holds_is_a_source_error() -> Outcome {
    let zeros = vec![0_u8; 1 << 20];
    let mut source = archive(&[("1.png", &zeros)])?;
    // Both headers claim that the entry unpacks to ten bytes.
    patch(&mut source, b"PK\x03\x04", 22, [10, 0, 0, 0]);
    patch(&mut source, b"PK\x01\x02", 24, [10, 0, 0, 0]);
    let result =
        converter_with(|options| options.limits.book.max_resource_bytes = 1000).load_slice(&source);
    assert!(matches!(
        result,
        Err(Error::Source {
            format: Format::Cbz,
            ..
        })
    ));
    Ok(())
}

#[test]
fn pages_that_together_exceed_the_file_limit_are_refused() -> Outcome {
    let (first, second) = (page(1)?, page(2)?);
    let source = archive(&[("1.png", &first), ("2.png", &second)])?;
    let total = u64::try_from(first.len() + second.len())?;
    let refused = converter_with(|options| options.limits.book.max_file_bytes = total - 1)
        .load_slice(&source);
    assert!(matches!(refused, Err(Error::Limit(_))));
    let accepted =
        converter_with(|options| options.limits.book.max_file_bytes = total).load_slice(&source);
    assert!(accepted.is_ok());
    Ok(())
}

#[test]
fn an_archive_with_too_many_entries_is_refused() -> Outcome {
    let source = archive(&[
        ("1.png", &page(1)?),
        ("2.png", &page(2)?),
        ("readme.txt", b"Read me."),
    ])?;
    let refused =
        converter_with(|options| options.limits.book.max_resources = 2).load_slice(&source);
    assert!(matches!(refused, Err(Error::Limit(_))));
    let accepted =
        converter_with(|options| options.limits.book.max_resources = 3).load_slice(&source);
    assert!(accepted.is_ok());
    Ok(())
}
