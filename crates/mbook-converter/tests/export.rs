//! Export through the public API, including read-back through format importers.

#![cfg(feature = "export")]

use mbook_converter::mbook::{Book, Toc, TocItem};
use mbook_converter::{ConvertOptions, Converter, Error, Format};

type Outcome = Result<(), Box<dyn std::error::Error>>;

fn avif() -> Result<Vec<u8>, image::ImageError> {
    use image::{ExtendedColorType, ImageEncoder as _, codecs::avif::AvifEncoder};

    let mut avif = Vec::new();
    AvifEncoder::new_with_speed_quality(&mut avif, 10, 80).write_image(
        &[192; 24 * 16 * 3],
        24,
        16,
        ExtendedColorType::Rgb8,
    )?;
    Ok(avif)
}

fn book() -> Result<Book, Box<dyn std::error::Error>> {
    let mut book = Book::new("Winter & Spring", "fr")?;
    book.meta_mut().add_author("Éva Rousseau")?;
    book.meta_mut().set_annotation("A short example.")?;
    book.set_cover("img/cover.avif", avif()?)?;
    book.add_chapter(
        "parts/first.md",
        "# Première\n\nBonjour **à tous**. [Go to the drawing](../second%20page.md#figure).\n\n- [x] A task\n\n| A | B |\n| - | - |\n| C | D |\n\nA note[^a].\n\n[^a]: First note.\n",
    )?;
    book.add_chapter(
        "second page.md",
        "# Deuxième\n\n[figure]: #figure\n\n![The drawing](img/cover.avif)\n\n\\<unsafe\\> & bye[^a].\n\n[^a]: Second note.\n",
    )?;
    book.set_toc(Toc::Explicit(vec![
        TocItem::new(1, "The beginning", "parts/first.md"),
        TocItem::new(2, "The drawing", "second page.md#figure"),
    ]));
    Ok(book)
}

fn export(book: &Book, format: Format) -> Result<Vec<u8>, Error> {
    let mut bytes = Vec::new();
    Converter::export(book, format, &mut bytes)?;
    Ok(bytes)
}

#[test]
fn every_format_exports_even_before_a_chapter_is_written() -> Outcome {
    let empty = Book::new("An unwritten book", "en")?;
    for &format in Format::ALL {
        assert!(!export(&empty, format)?.is_empty(), "{format}");
        assert_eq!(format.extension().parse::<Format>(), Ok(format));
    }
    Ok(())
}

#[test]
fn every_writer_propagates_a_sink_failure() -> Outcome {
    use std::io::{self, Write};

    struct Closed;
    impl Write for Closed {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "the sink is closed",
            ))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let book = Book::new("Example", "en")?;
    for &format in Format::ALL {
        let result = Converter::export(&book, format, Closed);
        assert!(
            matches!(result, Err(Error::Io(error)) if error.kind() == io::ErrorKind::BrokenPipe),
            "{format}"
        );
    }
    Ok(())
}

fn check_read_back(format: Format) -> Outcome {
    let bytes = export(&book()?, format)?;
    let mut options = ConvertOptions::default();
    options.format = Some(format);
    // The source fixture has one small picture. Use the fastest AVIF encoder
    // while checking that the output is usable by the format importer.
    options.pictures.speed = 10;
    let converted = Converter::with_options(options).load_slice(&bytes);
    if !format.is_enabled() {
        assert!(matches!(converted, Err(Error::FormatDisabled(disabled)) if disabled == format));
        return Ok(());
    }
    let back = converted?.book;
    assert_eq!(back.meta().title(), "Winter & Spring");
    let text: String = back
        .chapters()
        .iter()
        .map(mbook_converter::mbook::Chapter::text)
        .collect();
    assert!(text.contains("Bonjour"), "{format}: {text}");
    assert!(text.contains("Deuxième"), "{format}: {text}");
    // The PDF importer filters pictures under 64 pixels. The PDF test
    // handles that case by inspecting the exported file directly.
    if format != Format::Pdf {
        assert!(
            !back.pictures().is_empty(),
            "{format}: the picture was lost"
        );
    }
    Ok(())
}

#[test]
fn an_epub_keeps_metadata_chapter_links_and_pictures() -> Outcome {
    use std::io::{Cursor, Read as _};
    use zip::{CompressionMethod, ZipArchive};

    let bytes = export(&book()?, Format::Epub)?;
    let mut zip = ZipArchive::new(Cursor::new(bytes))?;
    assert_eq!(zip.by_index(0)?.name()?, "mimetype");
    assert_eq!(zip.by_index(0)?.compression(), CompressionMethod::Stored);
    let mut chapter = String::new();
    zip.by_name("OEBPS/chapter-1.xhtml")?
        .read_to_string(&mut chapter)?;
    assert!(chapter.contains("href=\"chapter-2.xhtml#chapter-2-anchor-2\""));
    let mut next = String::new();
    zip.by_name("OEBPS/chapter-2.xhtml")?
        .read_to_string(&mut next)?;
    assert!(next.contains("id=\"chapter-2-anchor-2\""));
    check_read_back(Format::Epub)
}

#[test]
fn a_word_document_keeps_metadata_text_styles_and_pictures() -> Outcome {
    use std::io::{Cursor, Read as _};
    use zip::ZipArchive;

    let bytes = export(&book()?, Format::Docx)?;
    let mut zip = ZipArchive::new(Cursor::new(bytes))?;
    let mut document = String::new();
    zip.by_name("word/document.xml")?
        .read_to_string(&mut document)?;
    assert!(document.contains("w:val=\"Heading1\""));
    assert!(document.contains("<w:b"));
    check_read_back(Format::Docx)
}

#[test]
fn a_fictionbook_document_can_be_imported_with_its_pictures() -> Outcome {
    check_read_back(Format::Fb2)
}

#[test]
fn an_html_document_embeds_pictures_and_resolves_chapter_links() -> Outcome {
    let data = export(&book()?, Format::Html)?;
    let html = std::str::from_utf8(&data)?;
    assert!(html.contains("href=\"#chapter-2-anchor-2\""));
    assert!(html.contains("id=\"chapter-2-note-a\""));
    assert!(html.contains("data:image/png;base64,"));
    assert!(html.contains("&lt;unsafe&gt; &amp; bye"));
    check_read_back(Format::Html)
}

#[test]
fn a_pdf_has_selectable_text_and_pictures() -> Outcome {
    use unpdf::{ParseOptions, PdfParser};

    let bytes = export(&book()?, Format::Pdf)?;
    let options = ParseOptions::new()
        .with_resources(true)
        .with_min_image_dimension(0);
    let document = PdfParser::from_bytes_with_options(&bytes, options)?.parse()?;
    assert!(
        document
            .resources()
            .any(|(_, resource)| resource.is_image())
    );
    check_read_back(Format::Pdf)
}

#[test]
fn a_comic_archive_has_ordered_png_pages_and_metadata() -> Outcome {
    use std::io::{Cursor, Read as _};
    use zip::ZipArchive;

    let bytes = export(&book()?, Format::Cbz)?;
    let mut zip = ZipArchive::new(Cursor::new(bytes))?;
    let mut metadata = String::new();
    zip.by_name("ComicInfo.xml")?
        .read_to_string(&mut metadata)?;
    assert!(metadata.contains("<Title>Winter &amp; Spring</Title>"));
    for name in ["0000000001.png", "0000000002.png"] {
        let mut png = Vec::new();
        zip.by_name(name)?.read_to_end(&mut png)?;
        let page = image::load_from_memory_with_format(&png, image::ImageFormat::Png)?.into_rgb8();
        assert!(page.pixels().any(|pixel| pixel.0 != [255, 255, 255]));
    }
    Ok(())
}

#[test]
fn a_markdown_document_keeps_blocks_and_distinct_footnotes() -> Outcome {
    let data = export(&book()?, Format::Markdown)?;
    let source = std::str::from_utf8(&data)?;
    assert!(source.contains("data:image/avif;base64,"));
    assert!(source.contains("](#chapter-2-anchor-2)"));
    assert!(source.contains("[^chapter-1-note-a]:"));
    assert!(source.contains("[^chapter-2-note-a]:"));
    let rendered = markdown::to_html_with_options(source, &markdown::Options::gfm())
        .map_err(|error| error.to_string())?;
    assert!(rendered.contains("<h1>Première</h1>"), "{source}");
    assert!(rendered.contains("<strong>à tous</strong>"), "{source}");
    Ok(())
}

#[test]
fn a_text_document_reads_in_chapter_order_with_alternative_text() -> Outcome {
    let data = export(&book()?, Format::Text)?;
    let text = std::str::from_utf8(&data)?;
    assert!(text.contains("Bonjour à tous. Go to the drawing."));
    assert!(text.contains("• [x] A task"));
    assert!(text.contains("C | D"));
    assert!(text.contains("The drawing\n"));
    assert!(text.find("Première") < text.find("Deuxième"));
    Ok(())
}

#[test]
fn a_picture_that_cannot_be_decoded_keeps_the_codec_error() -> Outcome {
    use std::error::Error as _;

    let avif = avif()?;
    let end = avif
        .windows(4)
        .position(|chunk| chunk == b"mdat")
        .and_then(|at| at.checked_sub(4))
        .ok_or("no media data in source picture")?;
    let mut book = Book::new("Corrupt picture", "en")?;
    book.set_cover("broken.avif", &avif[..end])?;
    let Err(
        error @ Error::Export {
            format: Format::Epub,
            ..
        },
    ) = export(&book, Format::Epub)
    else {
        return Err("expected a typed export error".into());
    };
    assert!(error.source().is_some());
    Ok(())
}

#[test]
fn an_html_contents_list_follows_the_headings_of_a_new_book() -> Outcome {
    let mut book = Book::new("A short book", "en")?;
    book.add_chapter("one.md", "# A heading\n\nA paragraph.\n")?;
    let bytes = export(&book, Format::Html)?;
    let html = std::str::from_utf8(&bytes)?;
    assert!(html.contains("<a href=\"#chapter-1-anchor-1\">A heading</a>"));
    Ok(())
}

#[test]
fn a_pdf_creates_more_pages_when_the_text_exceeds_one_page() -> Outcome {
    let mut book = Book::new("A long chapter", "en")?;
    book.add_chapter("one.md", "Another paragraph.\n\n".repeat(100))?;
    let bytes = export(&book, Format::Pdf)?;
    assert!(unpdf::PdfParser::from_bytes(&bytes)?.page_count() > 1);
    Ok(())
}

/// A book with a diagram and a formula alone in their paragraphs, a formula
/// inside a line, and a formula that cannot be set.
fn figures() -> Result<Book, Box<dyn std::error::Error>> {
    let mut book = Book::new("Figures", "en")?;
    book.add_diagram(
        "dia/route.mmd",
        "flowchart LR\n  A[Station] --> B[Letter]\n",
    )?;
    book.add_formula("math/speed.tex", r"v = \frac{s}{t}")?;
    book.add_formula("math/broken.tex", r"\frac{")?;
    book.add_chapter(
        "one.md",
        "# Figures\n\n![The route](dia/route.mmd \"The way\")\n\n![speed is distance over time](math/speed.tex)\n\nThe train kept ![v = s / t](math/speed.tex) all the way.\n\n![an unfinished fraction](math/broken.tex)\n",
    )?;
    Ok(book)
}

fn zip_entries(bytes: Vec<u8>) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let names = zip
        .file_names()
        .map(|name| name.map(std::borrow::Cow::into_owned))
        .collect::<Result<_, _>>()?;
    Ok(names)
}

fn is_png(name: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
}

fn zip_text(bytes: &[u8], name: &str) -> Result<String, Box<dyn std::error::Error>> {
    use std::io::Read as _;

    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    let mut text = String::new();
    zip.by_name(name)?.read_to_string(&mut text)?;
    Ok(text)
}

#[test]
fn an_html_document_shows_diagrams_and_formulas_as_pictures() -> Outcome {
    let data = export(&figures()?, Format::Html)?;
    let html = std::str::from_utf8(&data)?;
    let figures = html.matches("<img src=\"data:image/png;base64,").count();
    assert_eq!(figures, 3, "{html}");
    assert!(html.contains("alt=\"The route\""), "{html}");
    assert!(html.contains("title=\"The way\""), "{html}");
    assert!(html.contains("alt=\"v = s / t\""), "{html}");
    Ok(())
}

#[test]
fn a_formula_alone_in_a_paragraph_is_set_apart_and_one_in_a_line_on_its_baseline() -> Outcome {
    let data = export(&figures()?, Format::Html)?;
    let html = std::str::from_utf8(&data)?;
    let tag = |alt: &str| -> Result<String, Box<dyn std::error::Error>> {
        let at = html.find(&format!("alt=\"{alt}\"")).ok_or(alt.to_owned())?;
        let start = html[..at].rfind("<img").ok_or("no tag")?;
        let end = at + html[at..].find("/>").ok_or("no end")?;
        Ok(html[start..end].to_owned())
    };
    let display = tag("speed is distance over time")?;
    let inline = tag("v = s / t")?;
    assert!(display.contains("display: block"), "{display}");
    assert!(inline.contains("vertical-align: -"), "{inline}");
    assert_ne!(display, inline);
    Ok(())
}

#[test]
fn a_formula_that_cannot_be_set_is_exported_as_its_alternative_text() -> Outcome {
    let data = export(&figures()?, Format::Html)?;
    let html = std::str::from_utf8(&data)?;
    assert!(html.contains("<p>an unfinished fraction</p>"), "{html}");
    Ok(())
}

#[test]
fn an_epub_carries_diagrams_and_formulas_as_png_resources() -> Outcome {
    let bytes = export(&figures()?, Format::Epub)?;
    let names = zip_entries(bytes.clone())?;
    let figures: Vec<_> = names
        .iter()
        .filter(|name| name.starts_with("OEBPS/figure-") && is_png(name))
        .collect();
    // The diagram, the display formula and the inline formula.
    assert_eq!(figures.len(), 3, "{names:?}");
    let chapter = zip_text(&bytes, "OEBPS/chapter-1.xhtml")?;
    assert!(chapter.contains("<img src=\"figure-1.png\""), "{chapter}");
    let package = zip_text(&bytes, "OEBPS/content.opf")?;
    assert!(package.contains("figure-1.png"), "{package}");
    Ok(())
}

#[test]
fn a_fictionbook_document_carries_diagrams_and_formulas_as_binaries() -> Outcome {
    let data = export(&figures()?, Format::Fb2)?;
    let xml = std::str::from_utf8(&data)?;
    assert_eq!(xml.matches("<binary ").count(), 3, "{xml}");
    assert!(xml.contains("<image xlink:href=\"#figure-1.png\" alt=\"The route\"/>"));
    assert!(xml.contains("an unfinished fraction"));
    Ok(())
}

#[test]
fn a_word_document_embeds_diagrams_and_formulas() -> Outcome {
    let bytes = export(&figures()?, Format::Docx)?;
    let media: Vec<_> = zip_entries(bytes)?
        .into_iter()
        .filter(|name| name.starts_with("word/media/") && is_png(name))
        .collect();
    assert_eq!(media.len(), 3, "{media:?}");
    Ok(())
}

#[test]
fn a_pdf_draws_diagrams_and_formulas() -> Outcome {
    use unpdf::{ParseOptions, PdfParser};

    let bytes = export(&figures()?, Format::Pdf)?;
    let options = ParseOptions::new()
        .with_resources(true)
        .with_min_image_dimension(0);
    let document = PdfParser::from_bytes_with_options(&bytes, options)?.parse()?;
    let images = document
        .resources()
        .filter(|(_, resource)| resource.is_image())
        .count();
    assert_eq!(images, 3);
    Ok(())
}

#[test]
fn a_markdown_document_shows_diagrams_and_formulas_as_png_pictures() -> Outcome {
    let data = export(&figures()?, Format::Markdown)?;
    let source = std::str::from_utf8(&data)?;
    assert_eq!(
        source.matches("<img src=\"data:image/png;base64,").count(),
        3,
        "{source}"
    );
    assert!(source.contains("an unfinished fraction"));
    Ok(())
}

#[test]
fn a_text_document_keeps_the_alternative_text_of_figures() -> Outcome {
    let data = export(&figures()?, Format::Text)?;
    let text = std::str::from_utf8(&data)?;
    assert!(text.contains("The route\n"));
    assert!(text.contains("The train kept v = s / t all the way."));
    Ok(())
}

fn repeated_formula() -> Result<Book, Box<dyn std::error::Error>> {
    let mut book = Book::new("Repeated", "en")?;
    book.add_formula("math/speed.tex", r"v = \frac{s}{t}")?;
    book.add_chapter(
        "one.md",
        "First ![v](math/speed.tex) and again ![v](math/speed.tex).\n",
    )?;
    Ok(book)
}

#[test]
fn an_epub_stores_a_figure_shown_twice_once() -> Outcome {
    let bytes = export(&repeated_formula()?, Format::Epub)?;
    let figures = zip_entries(bytes.clone())?
        .into_iter()
        .filter(|name| name.starts_with("OEBPS/figure-") && is_png(name))
        .count();
    assert_eq!(figures, 1);
    let chapter = zip_text(&bytes, "OEBPS/chapter-1.xhtml")?;
    assert_eq!(chapter.matches("src=\"figure-1.png\"").count(), 2);
    Ok(())
}

#[test]
fn a_fictionbook_document_stores_a_figure_shown_twice_once() -> Outcome {
    let data = export(&repeated_formula()?, Format::Fb2)?;
    let xml = std::str::from_utf8(&data)?;
    assert_eq!(xml.matches("<binary ").count(), 1);
    assert_eq!(xml.matches("xlink:href=\"#figure-1.png\"").count(), 2);
    Ok(())
}

#[test]
fn the_alternative_text_of_a_figure_is_its_words_on_one_line() -> Outcome {
    let mut book = Book::new("Alternative text", "en")?;
    book.add_formula("math/speed.tex", r"v = \frac{s}{t}")?;
    book.add_chapter(
        "one.md",
        "See ![speed is\ndistance over `time` ![v](math/speed.tex)](math/speed.tex) here.\n",
    )?;
    let data = export(&book, Format::Html)?;
    let html = std::str::from_utf8(&data)?;
    assert!(
        html.contains("alt=\"speed is distance over time v\""),
        "{html}"
    );
    assert_eq!(html.matches("<img ").count(), 1, "{html}");
    assert!(html.contains("\" /> here."), "{html}");
    Ok(())
}

#[test]
fn a_word_document_fits_a_wide_diagram_to_the_page() -> Outcome {
    let mut book = Book::new("Wide", "en")?;
    let chain: Vec<_> = (0..16)
        .map(|step| format!("S{step}[Step number {step}]"))
        .collect();
    book.add_diagram(
        "dia/wide.mmd",
        format!("flowchart LR\n  {}\n", chain.join(" --> ")),
    )?;
    book.add_chapter("one.md", "![A long chain](dia/wide.mmd)\n")?;
    let bytes = export(&book, Format::Docx)?;
    let document = zip_text(&bytes, "word/document.xml")?;
    let width: u64 = document
        .split("cx=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .ok_or("no picture extent")?
        .parse()?;
    assert_eq!(width, 5_650_000, "{document}");
    Ok(())
}
