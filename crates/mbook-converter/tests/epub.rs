//! EPUB books, through the public API.
#![cfg(feature = "epub")]

use image::{ImageFormat, Rgb, RgbImage};
use mbook_converter::mbook_core::{Book, Chapter, Toc, TocItem};
use mbook_converter::{ConvertOptions, Converted, Converter, Error, Format, WarningKind};
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

type Outcome = Result<(), Box<dyn std::error::Error>>;

/// A converter that reads its source as an EPUB.
fn epub_converter() -> Converter {
    converter_with(|_| {})
}

/// A converter for EPUB books with changed options.
fn converter_with(change: impl FnOnce(&mut ConvertOptions)) -> Converter {
    let mut options = ConvertOptions::default();
    options.format = Some(Format::Epub);
    change(&mut options);
    Converter::with_options(options)
}

// ---------------------------------------------------------------------------
// A builder of EPUB files in memory.
// ---------------------------------------------------------------------------

/// The identifier every sample package names as its unique identifier.
const UID: &str =
    "<dc:identifier id=\"uid\">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>";

/// The metadata every sample has: an identifier, a title and a language.
const BASIC: &str = "<dc:identifier id=\"uid\">urn:uuid:12345678-1234-1234-1234-123456789abc</dc:identifier>\
<dc:title>The Example</dc:title><dc:language>en</dc:language>";

/// A manifest entry.
fn item(id: &str, href: &str, media_type: &str, properties: &str) -> String {
    let properties = if properties.is_empty() {
        String::new()
    } else {
        format!(" properties=\"{properties}\"")
    };
    format!("<item id=\"{id}\" href=\"{href}\" media-type=\"{media_type}\"{properties}/>")
}

/// A manifest entry of an XHTML file.
fn page_item(id: &str, href: &str) -> String {
    item(id, href, "application/xhtml+xml", "")
}

/// The spine of the pages with these identifiers.
fn spine(ids: &[&str]) -> String {
    ids.iter()
        .map(|id| ["<itemref idref=\"", id, "\"/>"].concat())
        .collect()
}

/// An OPF package.
fn package(version: &str, metadata: &str, manifest: &str, spine: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"{version}\" unique-identifier=\"uid\">\
<metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:opf=\"http://www.idpf.org/2007/opf\">{metadata}</metadata>\
<manifest>{manifest}</manifest><spine>{spine}</spine></package>"
    )
}

/// An EPUB 3 package with the basic metadata.
fn package3(manifest: &str, spine: &str) -> String {
    package("3.0", BASIC, manifest, spine)
}

/// An XHTML document with this body.
fn page(body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">\
<head><title>Page</title></head><body>{body}</body></html>"
    )
}

/// The `META-INF/container.xml` that names the package `opf`.
fn container(opf: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?>\
<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">\
<rootfiles><rootfile full-path=\"{opf}\" media-type=\"application/oebps-package+xml\"/></rootfiles>\
</container>"
    )
}

/// A navigation document: `entries` is the markup of the `ol` of the table
/// of contents.
fn nav(entries: &str) -> String {
    page(&format!(
        "<nav epub:type=\"toc\"><h1>Contents</h1><ol>{entries}</ol></nav>"
    ))
}

/// A ZIP archive with the `mimetype` entry first and stored, then the files.
fn zip(files: &[(&str, &[u8])]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    writer.start_file("mimetype", stored)?;
    writer.write_all(b"application/epub+zip")?;
    for (name, content) in files {
        writer.start_file(*name, SimpleFileOptions::default())?;
        writer.write_all(content)?;
    }
    Ok(writer.finish()?.into_inner())
}

/// An EPUB whose package is `OEBPS/content.opf`, with these files besides
/// the container and the package. Names are relative to `OEBPS/`.
fn epub(opf: &str, files: &[(&str, &[u8])]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let container = container("OEBPS/content.opf");
    let mut all: Vec<(String, &[u8])> = vec![
        ("META-INF/container.xml".to_owned(), container.as_bytes()),
        ("OEBPS/content.opf".to_owned(), opf.as_bytes()),
    ];
    all.extend(
        files
            .iter()
            .map(|(name, content)| (format!("OEBPS/{name}"), *content)),
    );
    let borrowed: Vec<(&str, &[u8])> = all.iter().map(|(n, c)| (n.as_str(), *c)).collect();
    zip(&borrowed)
}

/// A book of XHTML pages `ch1.xhtml`, `ch2.xhtml`, … with these bodies.
fn book_of(bodies: &[&str]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let ids: Vec<String> = (1..=bodies.len()).map(|n| format!("c{n}")).collect();
    let manifest: String = ids
        .iter()
        .map(|id| page_item(id, &format!("{id}.xhtml")))
        .collect();
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let opf = package3(&manifest, &spine(&refs));
    let pages: Vec<(String, String)> = ids
        .iter()
        .zip(bodies)
        .map(|(id, body)| (format!("{id}.xhtml"), page(body)))
        .collect();
    let files: Vec<(&str, &[u8])> = pages
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_bytes()))
        .collect();
    epub(&opf, &files)
}

/// A picture of a given size in `format`.
fn picture(
    width: u32,
    height: u32,
    format: ImageFormat,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let image = RgbImage::from_pixel(width, height, Rgb([200, 40, 40]));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, format)?;
    Ok(bytes.into_inner())
}

/// A PNG of a given size.
fn png(width: u32, height: u32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    picture(width, height, ImageFormat::Png)
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

// ---------------------------------------------------------------------------
// What the tests look at.
// ---------------------------------------------------------------------------

/// The texts of the chapters of a book.
fn texts(book: &Book) -> Vec<&str> {
    book.chapters().iter().map(Chapter::text).collect()
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

/// The kinds of the warnings of a conversion.
fn kinds(converted: &Converted) -> Vec<&WarningKind> {
    converted.report.warnings.iter().map(|w| &w.kind).collect()
}

/// Asserts that no markup was dropped, which no sample of these tests
/// causes: the XHTML they hold is all expressible.
fn assert_no_markup_dropped(converted: &Converted) {
    let dropped: Vec<_> = converted
        .report
        .warnings
        .iter()
        .filter(|warning| matches!(warning.kind, WarningKind::MarkupDropped(_)))
        .collect();
    assert!(dropped.is_empty(), "unexpected warnings: {dropped:?}");
}

/// An item of the contents: level, title, chapter and anchor.
type TocRow<'a> = (u8, &'a str, &'a str, Option<&'a str>);

/// The contents of a book as `(level, title, chapter, anchor)`.
fn toc_items(book: &Book) -> Result<Vec<TocRow<'_>>, String> {
    let Toc::Explicit(items) = book.toc() else {
        return Err(format!("explicit contents expected, got {:?}", book.toc()));
    };
    Ok(items
        .iter()
        .map(|item: &TocItem| {
            (
                item.level,
                item.title.as_str(),
                item.chapter.as_str(),
                item.anchor.as_deref(),
            )
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Metadata.
// ---------------------------------------------------------------------------

/// The metadata of a full EPUB 3 package.
fn full_metadata() -> String {
    format!(
        "{UID}\
<dc:identifier>urn:isbn:9780306406157</dc:identifier>\
<dc:title id=\"t1\">The Example</dc:title>\
<meta refines=\"#t1\" property=\"title-type\">main</meta>\
<dc:title id=\"t2\">A Subtitle</dc:title>\
<meta refines=\"#t2\" property=\"title-type\">subtitle</meta>\
<dc:language>en</dc:language><dc:language>de</dc:language>\
<dc:creator id=\"a1\">Ann Author</dc:creator>\
<meta refines=\"#a1\" property=\"role\" scheme=\"marc:relators\">aut</meta>\
<dc:creator>Bob Writer</dc:creator>\
<dc:contributor id=\"tr\">Carl Translator</dc:contributor>\
<meta refines=\"#tr\" property=\"role\" scheme=\"marc:relators\">trl</meta>\
<dc:contributor id=\"ed\">Dora Editor</dc:contributor>\
<meta refines=\"#ed\" property=\"role\" scheme=\"marc:relators\">edt</meta>\
<dc:publisher>Example Press</dc:publisher>\
<dc:date>2020-03-07T00:00:00Z</dc:date>\
<dc:subject>Fiction</dc:subject><dc:subject>Adventure</dc:subject>\
<dc:description>&lt;p&gt;First &amp;amp; &lt;b&gt;last&lt;/b&gt;.&lt;/p&gt;&lt;p&gt;Second paragraph.&lt;/p&gt;</dc:description>\
<meta id=\"s1\" property=\"belongs-to-collection\">Examples</meta>\
<meta refines=\"#s1\" property=\"collection-type\">series</meta>\
<meta refines=\"#s1\" property=\"group-position\">2</meta>"
    )
}

#[test]
fn the_package_metadata_fills_the_metadata_of_the_book() -> Outcome {
    let opf = package(
        "3.0",
        &full_metadata(),
        &page_item("c1", "c1.xhtml"),
        &spine(&["c1"]),
    );
    let source = epub(&opf, &[("c1.xhtml", page("<p>Text.</p>").as_bytes())])?;
    let converted = epub_converter().load_slice(&source)?;
    let meta = converted.book.meta();
    assert_eq!(meta.title(), "The Example");
    assert_eq!(meta.languages().collect::<Vec<_>>(), ["en", "de"]);
    assert_eq!(meta.id(), "urn:uuid:12345678-1234-1234-1234-123456789abc");
    assert_eq!(
        meta.authors().collect::<Vec<_>>(),
        ["Ann Author", "Bob Writer"]
    );
    assert_eq!(meta.translators().collect::<Vec<_>>(), ["Carl Translator"]);
    assert_eq!(meta.publisher(), Some("Example Press"));
    assert_eq!(meta.date(), Some("2020-03-07"));
    assert_eq!(meta.genres().collect::<Vec<_>>(), ["Fiction", "Adventure"]);
    assert_eq!(
        meta.annotation(),
        Some("First & last.\n\nSecond paragraph.")
    );
    let series = meta.series().map(|series| (series.name, series.index));
    assert_eq!(series, Some(("Examples", Some("2"))));
    assert_eq!(meta.isbns().collect::<Vec<_>>(), ["9780306406157"]);
    assert_eq!(kinds(&converted), [] as [&WarningKind; 0]);
    assert_eq!(converted.report.format, Format::Epub);
    Ok(())
}

#[test]
fn a_collection_of_another_type_is_a_series_when_no_series_is_named() -> Outcome {
    let metadata = format!(
        "{BASIC}<meta id=\"s1\" property=\"belongs-to-collection\">Anthology</meta>\
<meta refines=\"#s1\" property=\"group-position\">3.5</meta>"
    );
    let opf = package(
        "3.0",
        &metadata,
        &page_item("c1", "c1.xhtml"),
        &spine(&["c1"]),
    );
    let source = epub(&opf, &[("c1.xhtml", page("<p>Text.</p>").as_bytes())])?;
    let converted = epub_converter().load_slice(&source)?;
    let series = converted
        .book
        .meta()
        .series()
        .map(|series| (series.name, series.index));
    assert_eq!(series, Some(("Anthology", Some("3.5"))));
    Ok(())
}

#[test]
fn the_series_of_calibre_is_a_series() -> Outcome {
    let metadata = format!(
        "{BASIC}<meta name=\"calibre:series\" content=\"The Cycle\"/>\
<meta name=\"calibre:series_index\" content=\"4.0\"/>"
    );
    let opf = package(
        "2.0",
        &metadata,
        &page_item("c1", "c1.xhtml"),
        &spine(&["c1"]),
    );
    let source = epub(&opf, &[("c1.xhtml", page("<p>Text.</p>").as_bytes())])?;
    let converted = epub_converter().load_slice(&source)?;
    let series = converted
        .book
        .meta()
        .series()
        .map(|series| (series.name, series.index));
    assert_eq!(series, Some(("The Cycle", Some("4.0"))));
    Ok(())
}

#[test]
fn a_series_without_a_position_has_none() -> Outcome {
    let metadata = format!("{BASIC}<meta name=\"calibre:series\" content=\"The Cycle\"/>");
    let opf = package(
        "2.0",
        &metadata,
        &page_item("c1", "c1.xhtml"),
        &spine(&["c1"]),
    );
    let source = epub(&opf, &[("c1.xhtml", page("<p>Text.</p>").as_bytes())])?;
    let converted = epub_converter().load_slice(&source)?;
    let series = converted
        .book
        .meta()
        .series()
        .map(|series| (series.name, series.index));
    assert_eq!(series, Some(("The Cycle", None)));
    Ok(())
}

#[test]
fn an_epub_2_package_gives_roles_dates_and_isbns_in_its_own_attributes() -> Outcome {
    let metadata = "<dc:identifier id=\"uid\" opf:scheme=\"ISBN\">978-0-306-40615-7</dc:identifier>\
<dc:title>Old Book</dc:title><dc:language>fr</dc:language>\
<dc:creator opf:role=\"aut\">Anne Auteur</dc:creator>\
<dc:contributor opf:role=\"trl\">Marc Traducteur</dc:contributor>\
<dc:contributor opf:role=\"edt\">Edith Editrice</dc:contributor>\
<dc:date opf:event=\"modification\">2021-01-01</dc:date>\
<dc:date opf:event=\"publication\">1999-12</dc:date>\
<dc:description>One.\r\nStill one.\n\nTwo.</dc:description>";
    let opf = package(
        "2.0",
        metadata,
        &page_item("c1", "c1.xhtml"),
        &spine(&["c1"]),
    );
    let source = epub(&opf, &[("c1.xhtml", page("<p>Texte.</p>").as_bytes())])?;
    let converted = epub_converter().load_slice(&source)?;
    let meta = converted.book.meta();
    assert_eq!(meta.title(), "Old Book");
    assert_eq!(meta.language(), "fr");
    assert_eq!(meta.id(), "978-0-306-40615-7");
    assert_eq!(meta.authors().collect::<Vec<_>>(), ["Anne Auteur"]);
    assert_eq!(meta.translators().collect::<Vec<_>>(), ["Marc Traducteur"]);
    assert_eq!(meta.date(), Some("1999-12"));
    assert_eq!(meta.isbns().collect::<Vec<_>>(), ["9780306406157"]);
    // `rbook` joins the lines of a text into one.
    assert_eq!(meta.annotation(), Some("One. Still one. Two."));
    Ok(())
}

#[test]
fn an_identifier_with_an_isbn_scheme_of_epub_3_is_an_isbn() -> Outcome {
    let metadata = format!(
        "{BASIC}<dc:identifier id=\"i2\">9780306406157</dc:identifier>\
<meta refines=\"#i2\" property=\"identifier-type\" scheme=\"onix:codelist5\">15</meta>\
<dc:identifier id=\"i3\">0-306-40615-2</dc:identifier>\
<meta refines=\"#i3\" property=\"identifier-type\">ISBN</meta>\
<dc:identifier>urn:doi:10.1/x</dc:identifier>"
    );
    let opf = package(
        "3.0",
        &metadata,
        &page_item("c1", "c1.xhtml"),
        &spine(&["c1"]),
    );
    let source = epub(&opf, &[("c1.xhtml", page("<p>Text.</p>").as_bytes())])?;
    let converted = epub_converter().load_slice(&source)?;
    let isbns: Vec<_> = converted.book.meta().isbns().collect();
    assert_eq!(isbns, ["9780306406157", "0306406152"]);
    Ok(())
}

#[test]
fn a_package_without_metadata_gets_the_stand_ins() -> Outcome {
    let opf = package("3.0", "", &page_item("c1", "c1.xhtml"), &spine(&["c1"]));
    let source = epub(
        &opf,
        &[("c1.xhtml", page("<h1>Heading</h1><p>Text.</p>").as_bytes())],
    )?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(converted.book.meta().title(), "Heading");
    assert_eq!(converted.book.meta().language(), "und");
    assert!(kinds(&converted).contains(&&WarningKind::MetadataMissing("lang".to_owned())));
    Ok(())
}

// ---------------------------------------------------------------------------
// Sections and text.
// ---------------------------------------------------------------------------

#[test]
fn every_spine_item_is_a_chapter_in_spine_order_including_non_linear_ones() -> Outcome {
    let manifest = [
        page_item("a", "a.xhtml"),
        page_item("b", "b.xhtml"),
        page_item("c", "c.xhtml"),
        page_item("unused", "unused.xhtml"),
    ]
    .concat();
    let spine = "<itemref idref=\"c\"/><itemref idref=\"a\" linear=\"no\"/><itemref idref=\"b\"/>";
    let opf = package3(&manifest, spine);
    let source = epub(
        &opf,
        &[
            ("a.xhtml", page("<p>Alpha.</p>").as_bytes()),
            ("b.xhtml", page("<p>Beta.</p>").as_bytes()),
            ("c.xhtml", page("<p>Gamma.</p>").as_bytes()),
            ("unused.xhtml", page("<p>Not in the spine.</p>").as_bytes()),
        ],
    )?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(texts(&converted.book), ["Gamma.\n", "Alpha.\n", "Beta.\n"]);
    assert_no_markup_dropped(&converted);
    Ok(())
}

#[test]
fn the_text_of_xhtml_is_markdown_and_a_heading_does_not_cut_a_chapter() -> Outcome {
    let source = book_of(&[
        "<h1>One</h1><p>First <em>emphasis</em> and <strong>strong</strong> *stars*.</p>\
<h1>Two</h1><ul><li>item</li></ul>",
    ])?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(
        texts(&converted.book),
        ["# One\n\nFirst *emphasis* and **strong** \\*stars\\*.\n\n# Two\n\n- item\n"]
    );
    assert_no_markup_dropped(&converted);
    Ok(())
}

#[test]
fn a_utf_16_chapter_is_decoded() -> Outcome {
    let text = page("<p>Привет, мир.</p>");
    let utf16: Vec<u8> = [0xff, 0xfe]
        .into_iter()
        .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    let opf = package3(&page_item("c1", "c1.xhtml"), &spine(&["c1"]));
    let source = epub(&opf, &[("c1.xhtml", &utf16)])?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(texts(&converted.book), ["Привет, мир.\n"]);
    Ok(())
}

#[test]
fn bytes_that_are_not_text_become_replacement_characters() -> Outcome {
    let mut text = page("<p>Bad</p>").into_bytes();
    let at = text.windows(3).position(|w| w == b"Bad").ok_or("marker")?;
    text[at + 1] = 0xff;
    let opf = package3(&page_item("c1", "c1.xhtml"), &spine(&["c1"]));
    let source = epub(&opf, &[("c1.xhtml", &text)])?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(texts(&converted.book), ["B\u{fffd}d\n"]);
    Ok(())
}

#[test]
fn files_whose_section_names_become_equal_are_both_kept_and_linkable() -> Outcome {
    // `a#b.xhtml` and `a_b.xhtml` get the same name before it is made unique.
    let manifest = [page_item("a", "a%23b.xhtml"), page_item("b", "a_b.xhtml")].concat();
    let opf = package3(&manifest, &spine(&["a", "b"]));
    let first = page("<p><a href=\"a_b.xhtml\">to the second</a></p>");
    let second = page("<p><a href=\"a%23b.xhtml\">to the first</a></p>");
    let container = container("OEBPS/content.opf");
    let source = zip(&[
        ("META-INF/container.xml", container.as_bytes()),
        ("OEBPS/content.opf", opf.as_bytes()),
        ("OEBPS/a#b.xhtml", first.as_bytes()),
        ("OEBPS/a_b.xhtml", second.as_bytes()),
    ])?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(
        texts(&converted.book),
        [
            "[to the second](ch0002.md)\n",
            "[to the first](ch0001.md)\n"
        ]
    );
    assert_eq!(kinds(&converted), [] as [&WarningKind; 0]);
    Ok(())
}

#[test]
fn an_item_that_is_missing_or_unknown_or_repeated_is_left_out() -> Outcome {
    let manifest = [
        page_item("a", "a.xhtml"),
        page_item("gone", "gone.xhtml"),
        item("pdf", "book.pdf", "application/pdf", ""),
    ]
    .concat();
    let spine = "<itemref idref=\"a\"/><itemref idref=\"gone\"/><itemref idref=\"nowhere\"/>\
<itemref idref=\"pdf\"/><itemref idref=\"a\"/>";
    let opf = package3(&manifest, spine);
    let source = epub(
        &opf,
        &[
            ("a.xhtml", page("<p>Only once.</p>").as_bytes()),
            ("book.pdf", b"%PDF-1.4"),
        ],
    )?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(texts(&converted.book), ["Only once.\n"]);
    Ok(())
}

#[test]
fn a_spine_without_text_is_empty() -> Outcome {
    let source = book_of(&["", "  "])?;
    let result = epub_converter().load_slice(&source);
    assert!(matches!(result, Err(Error::Empty)), "{result:?}");
    Ok(())
}

#[test]
fn a_package_without_a_spine_is_empty() -> Outcome {
    let opf = package3(&page_item("c1", "c1.xhtml"), "");
    let source = epub(&opf, &[("c1.xhtml", page("<p>Text.</p>").as_bytes())])?;
    let result = epub_converter().load_slice(&source);
    assert!(matches!(result, Err(Error::Empty)), "{result:?}");
    Ok(())
}

#[test]
fn markup_that_mbook_cannot_express_is_reported_with_its_section() -> Outcome {
    let source = book_of(&["<p>H<sub>2</sub>O</p>"])?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(texts(&converted.book), ["H2O\n"]);
    let warning = converted.report.warnings.first().ok_or("a warning")?;
    assert_eq!(warning.kind, WarningKind::MarkupDropped("sub".to_owned()));
    assert_eq!(warning.location.as_deref(), Some("OEBPS/c1.xhtml"));
    Ok(())
}

#[test]
fn footnotes_inside_one_file_become_footnotes() -> Outcome {
    let source = book_of(
        &["<p>A claim<a epub:type=\"noteref\" href=\"#n1\">1</a>.</p>\
<aside epub:type=\"footnote\" id=\"n1\"><p>The proof.</p></aside>"],
    )?;
    let converted = epub_converter().load_slice(&source)?;
    let text = texts(&converted.book);
    assert_eq!(text.len(), 1);
    let text = text.first().copied().unwrap_or_default();
    assert!(text.starts_with("A claim[^n1]."), "{text}");
    assert!(text.contains("[^n1]: The proof."), "{text}");
    assert!(!text.contains("aside"), "{text}");
    assert_no_markup_dropped(&converted);
    Ok(())
}

// ---------------------------------------------------------------------------
// Links.
// ---------------------------------------------------------------------------

#[test]
fn a_link_to_an_element_of_another_chapter_leads_to_its_anchor() -> Outcome {
    let source = book_of(&[
        "<p>See <a href=\"c2.xhtml#target\">the target</a> and <a href=\"c2.xhtml\">chapter two</a>.</p>",
        "<p>Before.</p><p id=\"target\">The target paragraph.</p>",
    ])?;
    let converted = epub_converter().load_slice(&source)?;
    let book = &converted.book;
    assert_eq!(
        texts(book),
        [
            "See [the target](ch0002.md#target) and [chapter two](ch0002.md).\n",
            "Before.\n\n[target]: #target\n\nThe target paragraph.\n"
        ]
    );
    let anchors: Vec<_> = book.chapters()[1]
        .extra_anchors()
        .iter()
        .map(|anchor| anchor.name.as_str())
        .collect();
    assert_eq!(anchors, ["target"]);
    assert_eq!(kinds(&converted), [] as [&WarningKind; 0]);
    Ok(())
}

#[test]
fn a_link_to_a_heading_leads_to_the_anchor_of_the_heading() -> Outcome {
    let source = book_of(&[
        "<p><a href=\"c2.xhtml#sec\">Go</a></p>",
        "<h2 id=\"sec\">Section</h2><p>Text.</p>",
    ])?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(texts(&converted.book)[0], "[Go](ch0002.md#sec)\n");
    let anchors: Vec<_> = converted.book.chapters()[1]
        .extra_anchors()
        .iter()
        .map(|anchor| anchor.name.as_str())
        .collect();
    assert_eq!(anchors, ["sec"]);
    Ok(())
}

#[test]
fn a_link_within_a_file_leads_to_its_own_chapter() -> Outcome {
    let source = book_of(&[
        "<p><a href=\"#end\">Jump</a> and <a href=\"#missing\">nowhere</a> and <a href=\"#\">top</a></p>\
<p>Middle.</p><p id=\"end\">The end.</p>",
    ])?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(
        texts(&converted.book),
        [
            "[Jump](ch0001.md#end) and [nowhere](ch0001.md) and [top](ch0001.md)\n\nMiddle.\n\n[end]: #end\n\nThe end.\n"
        ]
    );
    Ok(())
}

#[test]
fn a_link_to_a_missing_element_leads_to_the_chapter() -> Outcome {
    let source = book_of(&["<p><a href=\"c2.xhtml#gone\">Go</a></p>", "<p>Two.</p>"])?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(texts(&converted.book)[0], "[Go](ch0002.md)\n");
    Ok(())
}

#[test]
fn links_are_resolved_against_the_folder_of_their_document() -> Outcome {
    let manifest = [
        page_item("a", "text/part1/a.xhtml"),
        page_item("b", "text/b.xhtml"),
        page_item("c", "c%20d.xhtml"),
    ]
    .concat();
    let opf = package3(&manifest, &spine(&["a", "b", "c"]));
    let source = epub(
        &opf,
        &[
            (
                "text/part1/a.xhtml",
                page(
                    "<p><a href=\"../b.xhtml#x\">up</a> <a href=\"./../../c%20d.xhtml\">far</a> \
<a href=\"/OEBPS/text/b.xhtml\">absolute</a></p>",
                )
                .as_bytes(),
            ),
            (
                "text/b.xhtml",
                page("<p id=\"x\">B.</p><p><a href=\"part1/a.xhtml\">down</a></p>").as_bytes(),
            ),
            ("c d.xhtml", page("<p>C.</p>").as_bytes()),
        ],
    )?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(
        texts(&converted.book),
        [
            "[up](ch0002.md#x) [far](ch0003.md) [absolute](ch0002.md)\n",
            "[x]: #x\n\nB.\n\n[down](ch0001.md)\n",
            "C.\n"
        ]
    );
    assert_eq!(kinds(&converted), [] as [&WarningKind; 0]);
    Ok(())
}

#[test]
fn external_links_are_written_as_they_are() -> Outcome {
    let source = book_of(&[
        "<p><a href=\"https://example.org/a?b=c\">web</a> <a href=\"mailto:a@example.org\">mail</a></p>",
    ])?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(
        texts(&converted.book),
        ["[web](https://example.org/a?b=c) [mail](mailto:a@example.org)\n"]
    );
    assert_eq!(kinds(&converted), [] as [&WarningKind; 0]);
    Ok(())
}

#[test]
fn a_dead_link_becomes_its_text_with_a_warning() -> Outcome {
    let source = book_of(&[
        "<p><a href=\"missing.xhtml\">gone</a> <a href=\"../../outside.xhtml\">out</a> \
<a href=\"javascript:void(0)\">script</a></p>",
    ])?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(texts(&converted.book), ["gone out script\n"]);
    assert_eq!(
        kinds(&converted),
        [
            &WarningKind::LinkDropped("missing.xhtml".to_owned()),
            &WarningKind::LinkDropped("../../outside.xhtml".to_owned()),
            &WarningKind::LinkDropped("javascript:void(0)".to_owned()),
        ]
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// Pictures and the cover.
// ---------------------------------------------------------------------------

/// A book with a chapter in `text/` that shows pictures of `img/`.
fn book_with_pictures(
    body: &str,
    extra_manifest: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let manifest = [
        page_item("c1", "text/c1.xhtml"),
        item("p1", "img/a.png", "image/png", ""),
        item("p2", "img/b.jpg", "image/jpeg", ""),
        item("p3", "img/c d.png", "image/png", ""),
        extra_manifest.to_owned(),
    ]
    .concat();
    let opf = package3(&manifest, &spine(&["c1"]));
    epub(
        &opf,
        &[
            ("text/c1.xhtml", page(body).as_bytes()),
            ("img/a.png", &png(12, 8)?),
            ("img/b.jpg", &picture(20, 10, ImageFormat::Jpeg)?),
            ("img/c d.png", &png(6, 6)?),
        ],
    )
}

#[test]
fn pictures_in_a_subfolder_are_found_by_relative_paths() -> Outcome {
    let source = book_with_pictures(
        "<p>Before.</p><p><img src=\"../img/a.png\" alt=\"First\"/></p>\
<figure><img src=\"../img/b.jpg\" alt=\"Second\"/><figcaption>A caption.</figcaption></figure>\
<p>Inline <img src=\"../img/c%20d.png\" alt=\"third\"/> picture.</p>",
        "",
    )?;
    let converted = epub_converter().load_slice(&source)?;
    let book = &converted.book;
    assert_eq!(
        pictures(book),
        [
            ("img/0001.avif".to_owned(), (12, 8)),
            ("img/0002.avif".to_owned(), (20, 10)),
            ("img/0003.avif".to_owned(), (6, 6)),
        ]
    );
    assert_eq!(
        texts(book),
        [
            "Before.\n\n![First](img/0001.avif)\n\n![Second](img/0002.avif \"A caption.\")\n\nInline ![third](img/0003.avif) picture.\n"
        ]
    );
    assert_eq!(kinds(&converted), [] as [&WarningKind; 0]);
    Ok(())
}

#[test]
fn a_picture_that_is_not_in_the_book_is_replaced_by_its_alternative_text() -> Outcome {
    let source = book_with_pictures(
        "<p><img src=\"../img/missing.png\" alt=\"Lost\"/> <img src=\"https://example.org/x.png\" alt=\"Far\"/></p>",
        "",
    )?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(texts(&converted.book), ["Lost Far\n"]);
    assert_eq!(converted.book.pictures().len(), 0);
    assert_eq!(
        kinds(&converted),
        [
            &WarningKind::PictureDropped("../img/missing.png".to_owned()),
            &WarningKind::PictureDropped("https://example.org/x.png".to_owned()),
        ]
    );
    Ok(())
}

#[test]
fn a_picture_wrapped_in_svg_is_a_figure() -> Outcome {
    let source = book_with_pictures(
        "<div><svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" \
viewBox=\"0 0 12 8\"><image width=\"12\" height=\"8\" xlink:href=\"../img/a.png\"/></svg></div>",
        "",
    )?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(texts(&converted.book), ["![](img/0001.avif)\n"]);
    assert_eq!(
        pictures(&converted.book),
        [("img/0001.avif".to_owned(), (12, 8))]
    );
    assert_eq!(kinds(&converted), [] as [&WarningKind; 0]);
    Ok(())
}

#[test]
fn an_image_in_the_spine_is_a_section_with_a_figure() -> Outcome {
    let manifest = [
        page_item("c1", "c1.xhtml"),
        item("p1", "plate.png", "image/png", ""),
    ]
    .concat();
    let opf = package3(&manifest, &spine(&["c1", "p1"]));
    let source = epub(
        &opf,
        &[
            ("c1.xhtml", page("<p>Text.</p>").as_bytes()),
            ("plate.png", &png(9, 7)?),
        ],
    )?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(texts(&converted.book), ["Text.\n", "![](img/0001.avif)\n"]);
    assert_eq!(
        pictures(&converted.book),
        [("img/0001.avif".to_owned(), (9, 7))]
    );
    Ok(())
}

#[test]
fn an_svg_in_the_spine_is_converted_like_xhtml() -> Outcome {
    let manifest = [
        page_item("c1", "c1.xhtml"),
        item("s1", "plate.svg", "image/svg+xml", ""),
        item("p1", "plate.png", "image/png", ""),
    ]
    .concat();
    let opf = package3(&manifest, &spine(&["c1", "s1"]));
    let svg = "<?xml version=\"1.0\"?><svg xmlns=\"http://www.w3.org/2000/svg\" \
xmlns:xlink=\"http://www.w3.org/1999/xlink\" viewBox=\"0 0 9 7\">\
<image width=\"9\" height=\"7\" xlink:href=\"plate.png\"/></svg>";
    let source = epub(
        &opf,
        &[
            ("c1.xhtml", page("<p>Text.</p>").as_bytes()),
            ("plate.svg", svg.as_bytes()),
            ("plate.png", &png(9, 7)?),
        ],
    )?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(texts(&converted.book), ["Text.\n", "![](img/0001.avif)\n"]);
    Ok(())
}

/// A book with a cover page in front of a chapter; `meta` and `properties`
/// say how the cover is named.
fn book_with_cover(
    version: &str,
    meta: &str,
    properties: &str,
    cover_page: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let metadata = format!("{BASIC}{meta}");
    let manifest = [
        page_item("cover", "cover.xhtml"),
        page_item("c1", "c1.xhtml"),
        item("cover-img", "images/cover.png", "image/png", properties),
    ]
    .concat();
    let opf = package(version, &metadata, &manifest, &spine(&["cover", "c1"]));
    epub(
        &opf,
        &[
            ("cover.xhtml", page(cover_page).as_bytes()),
            ("c1.xhtml", page("<p>Text.</p>").as_bytes()),
            ("images/cover.png", &png(30, 40)?),
        ],
    )
}

const COVER_PAGE: &str = "<div><img src=\"images/cover.png\" alt=\"Cover\"/></div>";

#[test]
fn the_cover_image_of_epub_3_is_the_cover_and_its_page_is_skipped() -> Outcome {
    let source = book_with_cover("3.0", "", "cover-image", COVER_PAGE)?;
    let converted = epub_converter().load_slice(&source)?;
    let book = &converted.book;
    assert_eq!(book.meta().cover(), Some("img/cover.avif"));
    assert_eq!(pictures(book), [("img/cover.avif".to_owned(), (30, 40))]);
    assert_eq!(texts(book), ["Text.\n"]);
    assert_eq!(kinds(&converted), [] as [&WarningKind; 0]);
    Ok(())
}

#[test]
fn the_cover_of_epub_2_is_named_by_a_meta_element() -> Outcome {
    let source = book_with_cover(
        "2.0",
        "<meta name=\"cover\" content=\"cover-img\"/>",
        "",
        COVER_PAGE,
    )?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(converted.book.meta().cover(), Some("img/cover.avif"));
    assert_eq!(texts(&converted.book), ["Text.\n"]);
    Ok(())
}

#[test]
fn a_cover_page_with_text_is_kept_and_shows_the_cover_once_more() -> Outcome {
    let source = book_with_cover(
        "3.0",
        "",
        "cover-image",
        "<h1>The Example</h1><img src=\"images/cover.png\" alt=\"Cover\"/>",
    )?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(converted.book.meta().cover(), Some("img/cover.avif"));
    assert_eq!(
        texts(&converted.book),
        ["# The Example\n\n![Cover](img/cover.avif)\n", "Text.\n"]
    );
    Ok(())
}

#[test]
fn a_cover_page_in_an_svg_wrapper_is_skipped() -> Outcome {
    let source = book_with_cover(
        "3.0",
        "",
        "cover-image",
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" viewBox=\"0 0 30 40\">\
<image width=\"30\" height=\"40\" xlink:href=\"images/cover.png\"/></svg>",
    )?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(texts(&converted.book), ["Text.\n"]);
    Ok(())
}

#[test]
fn the_cover_image_in_the_spine_is_not_shown_twice() -> Outcome {
    let manifest = [
        page_item("c1", "c1.xhtml"),
        item("cover-img", "cover.png", "image/png", "cover-image"),
    ]
    .concat();
    let opf = package3(&manifest, &spine(&["cover-img", "c1"]));
    let source = epub(
        &opf,
        &[
            ("c1.xhtml", page("<p>Text.</p>").as_bytes()),
            ("cover.png", &png(30, 40)?),
        ],
    )?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(converted.book.meta().cover(), Some("img/cover.avif"));
    assert_eq!(texts(&converted.book), ["Text.\n"]);
    Ok(())
}

#[test]
fn a_book_with_a_cover_and_no_text_is_a_book_of_its_cover() -> Outcome {
    let manifest = [
        page_item("cover", "cover.xhtml"),
        item("cover-img", "cover.png", "image/png", "cover-image"),
    ]
    .concat();
    let opf = package3(&manifest, &spine(&["cover"]));
    let source = epub(
        &opf,
        &[
            (
                "cover.xhtml",
                page("<p><img src=\"cover.png\" alt=\"\"/></p>").as_bytes(),
            ),
            ("cover.png", &png(30, 40)?),
        ],
    )?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(converted.book.chapters().len(), 0);
    assert_eq!(converted.book.meta().cover(), Some("img/cover.avif"));
    Ok(())
}

// ---------------------------------------------------------------------------
// Contents.
// ---------------------------------------------------------------------------

/// A book with a navigation document, two chapters and these `ol` entries.
fn book_with_nav(entries: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let manifest = [
        item("nav", "nav.xhtml", "application/xhtml+xml", "nav"),
        page_item("c1", "text/c1.xhtml"),
        page_item("c2", "text/c2.xhtml"),
    ]
    .concat();
    let opf = package3(&manifest, &spine(&["nav", "c1", "c2"]));
    epub(
        &opf,
        &[
            ("nav.xhtml", nav(entries).as_bytes()),
            (
                "text/c1.xhtml",
                page("<h1 id=\"one\">One</h1><p>Text.</p><h2 id=\"one-a\">One A</h2><p id=\"para\">Para.</p>")
                    .as_bytes(),
            ),
            (
                "text/c2.xhtml",
                page("<h1>Two</h1><p>More.</p>").as_bytes(),
            ),
        ],
    )
}

#[test]
fn a_navigation_document_with_nested_entries_gives_explicit_contents() -> Outcome {
    let source = book_with_nav(
        "<li><a href=\"text/c1.xhtml\">First</a><ol>\
<li><a href=\"text/c1.xhtml#one-a\">First A</a></li></ol></li>\
<li><a href=\"text/c2.xhtml\">Second</a></li>",
    )?;
    let converted = epub_converter().load_slice(&source)?;
    let book = &converted.book;
    // The navigation document is a chapter like any other.
    assert_eq!(book.chapters().len(), 3);
    assert_eq!(
        toc_items(book)?,
        [
            (1, "First", "ch0002.md", None),
            (2, "First A", "ch0002.md", Some("one-a")),
            (1, "Second", "ch0003.md", None),
        ]
    );
    assert_eq!(kinds(&converted), [] as [&WarningKind; 0]);
    Ok(())
}

#[test]
fn the_navigation_document_is_converted_like_any_chapter() -> Outcome {
    let source = book_with_nav("<li><a href=\"text/c1.xhtml#one\">First</a></li>")?;
    let converted = epub_converter().load_slice(&source)?;
    let first = texts(&converted.book)[0];
    assert!(
        first.starts_with("# Contents\n\n1. [First](ch0002.md#one-2)"),
        "{first}"
    );
    Ok(())
}

#[test]
fn an_entry_that_leads_to_a_paragraph_leads_to_the_heading_above_it() -> Outcome {
    let source = book_with_nav("<li><a href=\"text/c1.xhtml#para\">Paragraph</a></li>")?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(
        toc_items(&converted.book)?,
        [(1, "Paragraph", "ch0002.md", Some("one-a"))]
    );
    assert_eq!(
        kinds(&converted),
        [&WarningKind::LinkRetargeted(
            "OEBPS/text/c1.xhtml#para".to_owned()
        )]
    );
    Ok(())
}

#[test]
fn an_entry_without_an_address_or_to_a_missing_file_is_handled() -> Outcome {
    let source = book_with_nav(
        "<li><span>Group</span><ol><li><a href=\"text/c2.xhtml\">Second</a></li></ol></li>\
<li><a href=\"text/nowhere.xhtml\">Nowhere</a></li>\
<li><a href=\"https://example.org/\">Outside</a></li>",
    )?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(
        toc_items(&converted.book)?,
        [(1, "Second", "ch0003.md", None)]
    );
    assert_eq!(
        kinds(&converted),
        [
            // The link of the navigation document itself, then its two
            // entries that lead nowhere.
            &WarningKind::LinkDropped("text/nowhere.xhtml".to_owned()),
            &WarningKind::LinkDropped("/OEBPS/text/nowhere.xhtml".to_owned()),
            &WarningKind::LinkDropped("https://example.org/".to_owned()),
        ]
    );
    Ok(())
}

#[test]
fn an_entry_that_leads_to_the_skipped_cover_page_is_left_out_without_a_warning() -> Outcome {
    let manifest = [
        item("nav", "nav.xhtml", "application/xhtml+xml", "nav"),
        page_item("cover", "cover.xhtml"),
        page_item("c1", "c1.xhtml"),
        item("cover-img", "cover.png", "image/png", "cover-image"),
    ]
    .concat();
    let opf = package3(&manifest, &spine(&["cover", "c1"]));
    let source = epub(
        &opf,
        &[
            (
                "nav.xhtml",
                nav("<li><a href=\"cover.xhtml\">Cover</a></li><li><a href=\"c1.xhtml\">Text</a></li>")
                    .as_bytes(),
            ),
            (
                "cover.xhtml",
                page("<p><img src=\"cover.png\" alt=\"\"/></p>").as_bytes(),
            ),
            ("c1.xhtml", page("<h1>Text</h1>").as_bytes()),
            ("cover.png", &png(30, 40)?),
        ],
    )?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(
        toc_items(&converted.book)?,
        [(1, "Text", "ch0001.md", None)]
    );
    assert_eq!(kinds(&converted), [] as [&WarningKind; 0]);
    Ok(())
}

/// An EPUB 2 NCX with these `navPoint` elements.
fn ncx(points: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?><ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" version=\"2005-1\">\
<head/><docTitle><text>The Example</text></docTitle><navMap>{points}</navMap></ncx>"
    )
}

#[test]
fn an_epub_2_book_has_its_contents_from_the_ncx() -> Outcome {
    let manifest = [
        item("ncx", "toc.ncx", "application/x-dtbncx+xml", ""),
        page_item("c1", "text/c1.xhtml"),
        page_item("c2", "text/c2.xhtml"),
    ]
    .concat();
    let opf = package("2.0", BASIC, &manifest, &spine(&["c1", "c2"]))
        .replace("<spine>", "<spine toc=\"ncx\">");
    let points = "<navPoint id=\"n1\" playOrder=\"1\"><navLabel><text>First</text></navLabel>\
<content src=\"text/c1.xhtml\"/>\
<navPoint id=\"n2\" playOrder=\"2\"><navLabel><text>First A</text></navLabel>\
<content src=\"text/c1.xhtml#one-a\"/></navPoint></navPoint>\
<navPoint id=\"n3\" playOrder=\"3\"><navLabel><text>Second</text></navLabel>\
<content src=\"text/c2.xhtml\"/></navPoint>";
    let source = epub(
        &opf,
        &[
            ("toc.ncx", ncx(points).as_bytes()),
            (
                "text/c1.xhtml",
                page("<h1>One</h1><p>Text.</p><h2 id=\"one-a\">One A</h2>").as_bytes(),
            ),
            ("text/c2.xhtml", page("<h1>Two</h1>").as_bytes()),
        ],
    )?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(
        toc_items(&converted.book)?,
        [
            (1, "First", "ch0001.md", None),
            (2, "First A", "ch0001.md", Some("one-a")),
            (1, "Second", "ch0002.md", None),
        ]
    );
    assert_eq!(kinds(&converted), [] as [&WarningKind; 0]);
    Ok(())
}

#[test]
fn a_book_with_both_contents_uses_the_navigation_document() -> Outcome {
    let manifest = [
        item("nav", "nav.xhtml", "application/xhtml+xml", "nav"),
        item("ncx", "toc.ncx", "application/x-dtbncx+xml", ""),
        page_item("c1", "c1.xhtml"),
        page_item("c2", "c2.xhtml"),
    ]
    .concat();
    let opf = package3(&manifest, &spine(&["c1", "c2"])).replace("<spine>", "<spine toc=\"ncx\">");
    let points = "<navPoint id=\"n1\"><navLabel><text>From the NCX</text></navLabel>\
<content src=\"c1.xhtml\"/></navPoint>";
    let source = epub(
        &opf,
        &[
            (
                "nav.xhtml",
                nav("<li><a href=\"c2.xhtml\">From the nav</a></li>").as_bytes(),
            ),
            ("toc.ncx", ncx(points).as_bytes()),
            ("c1.xhtml", page("<h1>One</h1>").as_bytes()),
            ("c2.xhtml", page("<h1>Two</h1>").as_bytes()),
        ],
    )?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(
        toc_items(&converted.book)?,
        [(1, "From the nav", "ch0002.md", None)]
    );
    Ok(())
}

#[test]
fn a_book_without_contents_lists_its_headings() -> Outcome {
    let source = book_of(&["<h1>One</h1><p>Text.</p>", "<h1>Two</h1>"])?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(converted.book.toc(), &Toc::FromHeadings { max_level: 6 });
    Ok(())
}

// ---------------------------------------------------------------------------
// Protection, errors and limits.
// ---------------------------------------------------------------------------

/// A book with an `encryption.xml` and a font of the given path.
fn book_with_encryption(algorithm: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let manifest = [
        page_item("c1", "c1.xhtml"),
        item("font", "fonts/a.otf", "font/otf", ""),
    ]
    .concat();
    let opf = package3(&manifest, &spine(&["c1"]));
    let encryption = format!(
        "<?xml version=\"1.0\"?><encryption xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\" \
xmlns:enc=\"http://www.w3.org/2001/04/xmlenc#\"><enc:EncryptedData>\
<enc:EncryptionMethod Algorithm=\"{algorithm}\"/>\
<enc:CipherData><enc:CipherReference URI=\"OEBPS/fonts/a.otf\"/></enc:CipherData></enc:EncryptedData></encryption>"
    );
    let container = container("OEBPS/content.opf");
    zip(&[
        ("META-INF/container.xml", container.as_bytes()),
        ("META-INF/encryption.xml", encryption.as_bytes()),
        ("OEBPS/content.opf", opf.as_bytes()),
        ("OEBPS/c1.xhtml", page("<p>Text.</p>").as_bytes()),
        ("OEBPS/fonts/a.otf", b"not really a font"),
    ])
}

#[test]
fn a_book_with_encrypted_content_is_protected() -> Outcome {
    let source = book_with_encryption("http://www.w3.org/2001/04/xmlenc#aes128-cbc")?;
    let result = epub_converter().load_slice(&source);
    assert!(
        matches!(result, Err(Error::Protected(Format::Epub))),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn obfuscated_fonts_alone_do_not_make_a_book_protected() -> Outcome {
    for algorithm in [
        "http://www.idpf.org/2008/embedding",
        "http://ns.adobe.com/pdf/enc#RC",
    ] {
        let source = book_with_encryption(algorithm)?;
        let converted = epub_converter().load_slice(&source)?;
        assert_eq!(texts(&converted.book), ["Text.\n"]);
    }
    Ok(())
}

#[test]
fn a_book_with_a_rights_file_is_protected() -> Outcome {
    let opf = package3(&page_item("c1", "c1.xhtml"), &spine(&["c1"]));
    let container = container("OEBPS/content.opf");
    let source = zip(&[
        ("META-INF/container.xml", container.as_bytes()),
        ("META-INF/rights.xml", b"<rights/>"),
        ("OEBPS/content.opf", opf.as_bytes()),
        ("OEBPS/c1.xhtml", page("<p>Text.</p>").as_bytes()),
    ])?;
    let result = epub_converter().load_slice(&source);
    assert!(
        matches!(result, Err(Error::Protected(Format::Epub))),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn an_archive_that_is_not_an_epub_is_a_source_error() -> Outcome {
    let source = zip(&[("readme.txt", b"hello".as_slice())])?;
    let error = epub_converter()
        .load_slice(&source)
        .err()
        .ok_or("an error")?;
    assert!(
        matches!(
            error,
            Error::Source {
                format: Format::Epub,
                ..
            }
        ),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn bytes_that_are_not_a_zip_archive_are_a_source_error() {
    let result = epub_converter().load_slice(b"definitely not a zip archive");
    assert!(
        matches!(
            result,
            Err(Error::Source {
                format: Format::Epub,
                ..
            })
        ),
        "{result:?}"
    );
}

#[test]
fn a_broken_package_is_a_source_error() -> Outcome {
    let container = container("OEBPS/content.opf");
    let source = zip(&[
        ("META-INF/container.xml", container.as_bytes()),
        ("OEBPS/content.opf", b"this is not a package".as_slice()),
    ])?;
    let result = epub_converter().load_slice(&source);
    assert!(
        matches!(
            result,
            Err(Error::Source {
                format: Format::Epub,
                ..
            })
        ),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn a_container_without_a_package_is_a_source_error() -> Outcome {
    let source = zip(&[(
        "META-INF/container.xml",
        b"<container><rootfiles/></container>".as_slice(),
    )])?;
    let result = epub_converter().load_slice(&source);
    assert!(
        matches!(
            result,
            Err(Error::Source {
                format: Format::Epub,
                ..
            })
        ),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn an_epub_is_detected_from_its_content() -> Outcome {
    let source = book_of(&["<p>Detected.</p>"])?;
    let converted = Converter::new().load_slice(&source)?;
    assert_eq!(converted.report.format, Format::Epub);
    assert_eq!(texts(&converted.book), ["Detected.\n"]);
    Ok(())
}

#[test]
fn a_chapter_over_the_resource_limit_exceeds_a_limit() -> Outcome {
    let source = book_of(&[&"<p>A long chapter of text.</p>".repeat(100)])?;
    let converter = converter_with(|options| options.limits.book.max_resource_bytes = 1000);
    let result = converter.load_slice(&source);
    assert!(matches!(result, Err(Error::Limit(_))), "{result:?}");
    Ok(())
}

#[test]
fn a_picture_over_the_resource_limit_exceeds_a_limit() -> Outcome {
    let source = book_with_pictures("<p>Text.</p>", "")?;
    let size = png(12, 8)?.len() as u64;
    let converter = converter_with(|options| options.limits.book.max_resource_bytes = size - 1);
    let result = converter.load_slice(&source);
    assert!(matches!(result, Err(Error::Limit(_))), "{result:?}");
    Ok(())
}

#[test]
fn everything_unpacked_together_is_bounded_by_the_file_limit() -> Outcome {
    let body = "<p>Some text for a chapter.</p>".repeat(20);
    let source = book_of(&[&body, &body, &body])?;
    // Each chapter fits; the three of them together do not.
    let chapter = page(&body).len() as u64;
    let converter = converter_with(|options| {
        options.limits.book.max_resource_bytes = chapter + 100;
        options.limits.book.max_file_bytes = 2 * chapter + 50;
    });
    let result = converter.load_slice(&source);
    assert!(matches!(result, Err(Error::Limit(_))), "{result:?}");
    // With room for all three the book converts.
    let roomy = converter_with(|options| {
        options.limits.book.max_resource_bytes = chapter + 100;
        options.limits.book.max_file_bytes = 3 * chapter + 50;
    });
    assert_eq!(roomy.load_slice(&source)?.book.chapters().len(), 3);
    Ok(())
}

#[test]
fn too_many_entries_in_the_archive_exceed_a_limit() -> Outcome {
    let source = book_of(&["<p>1</p>", "<p>2</p>", "<p>3</p>"])?;
    // The archive has six entries: the mimetype, the container, the package
    // and three chapters.
    let converter = converter_with(|options| options.limits.book.max_resources = 6);
    assert_eq!(converter.load_slice(&source)?.book.chapters().len(), 3);
    let converter = converter_with(|options| options.limits.book.max_resources = 5);
    let result = converter.load_slice(&source);
    assert!(matches!(result, Err(Error::Limit(_))), "{result:?}");
    Ok(())
}

#[test]
fn too_many_spine_items_exceed_a_limit() -> Outcome {
    // One file that the spine names ten times: the archive is small, the
    // spine is not.
    let opf = package3(&page_item("c1", "c1.xhtml"), &spine(&["c1"; 10]));
    let source = epub(&opf, &[("c1.xhtml", page("<p>Text.</p>").as_bytes())])?;
    let converter = converter_with(|options| options.limits.book.max_resources = 10);
    assert_eq!(converter.load_slice(&source)?.book.chapters().len(), 1);
    let converter = converter_with(|options| options.limits.book.max_resources = 9);
    let result = converter.load_slice(&source);
    assert!(matches!(result, Err(Error::Limit(_))), "{result:?}");
    Ok(())
}

#[test]
fn an_entry_that_declares_less_than_it_holds_is_refused() -> Outcome {
    let mut source = book_of(&["<p>A chapter that is longer than ten bytes.</p>"])?;
    patch(&mut source, b"PK\x03\x04", 22, [10, 0, 0, 0]);
    patch(&mut source, b"PK\x01\x02", 24, [10, 0, 0, 0]);
    let result = epub_converter().load_slice(&source);
    assert!(
        matches!(
            result,
            Err(Error::Source {
                format: Format::Epub,
                ..
            })
        ),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn an_entry_that_declares_more_than_the_limit_is_refused_before_it_is_read() -> Outcome {
    let mut source = book_of(&["<p>Short.</p>"])?;
    patch(&mut source, b"PK\x03\x04", 22, [0, 0, 0, 0x7f]);
    patch(&mut source, b"PK\x01\x02", 24, [0, 0, 0, 0x7f]);
    let result = epub_converter().load_slice(&source);
    assert!(matches!(result, Err(Error::Limit(_))), "{result:?}");
    Ok(())
}

#[test]
fn manifest_entries_that_cannot_be_unpacked_are_left_out() -> Outcome {
    let manifest = [
        page_item("c1", "c1.xhtml"),
        page_item("remote", "https://example.org/page.xhtml"),
        item("p1", "a.png", "image/png", ""),
        item("p1-again", "a.png", "image/png", ""),
        item("p2", "missing.png", "image/png", ""),
        item("p3", "https://example.org/x.png", "image/png", ""),
    ]
    .concat();
    let opf = package3(&manifest, &spine(&["c1", "remote", "p2", "gone"]));
    let source = epub(
        &opf,
        &[
            (
                "c1.xhtml",
                page("<p><img src=\"a.png\" alt=\"A\"/> <img src=\"missing.png\" alt=\"M\"/></p>")
                    .as_bytes(),
            ),
            ("a.png", &png(5, 4)?),
        ],
    )?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(texts(&converted.book), ["![A](img/0001.avif) M\n"]);
    assert_eq!(
        pictures(&converted.book),
        [("img/0001.avif".to_owned(), (5, 4))]
    );
    assert_eq!(
        kinds(&converted),
        [&WarningKind::PictureDropped("missing.png".to_owned())]
    );
    Ok(())
}

#[test]
fn an_encrypted_entry_of_the_archive_makes_the_book_protected() -> Outcome {
    let mut source = book_of(&["<p>Text.</p>"])?;
    // The general purpose flag of every entry says "encrypted".
    patch(&mut source, b"PK\x03\x04", 6, [1, 0, 0, 0]);
    patch(&mut source, b"PK\x01\x02", 8, [1, 0, 0, 0]);
    let result = epub_converter().load_slice(&source);
    assert!(
        matches!(result, Err(Error::Protected(Format::Epub))),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn a_table_without_a_header_row_has_its_first_row_as_the_header() -> Outcome {
    let source =
        book_of(&["<table><tr><td>a</td><td>b</td></tr><tr><td>1</td><td>2</td></tr></table>"])?;
    let converted = epub_converter().load_slice(&source)?;
    assert_eq!(
        texts(&converted.book),
        ["| a | b |\n| - | - |\n| 1 | 2 |\n"]
    );
    Ok(())
}

#[test]
fn a_rich_book_loses_no_markup() -> Outcome {
    let source = book_of(&[
        "<h1>Title</h1><p>Plain, <i>italic</i>, <b>bold</b>, <code>code</code>, <a href=\"c2.xhtml\">link</a>.</p>\
<blockquote><p>Quote.</p></blockquote><ol><li>One</li><li>Two</li></ol><hr/>\
<table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></table><pre>fn main() {}</pre>",
        "<div><span>Spans</span> and <br/> breaks.</div>",
    ])?;
    let converted = epub_converter().load_slice(&source)?;
    assert_no_markup_dropped(&converted);
    assert_eq!(kinds(&converted), [] as [&WarningKind; 0]);
    Ok(())
}
