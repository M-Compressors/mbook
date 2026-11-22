//! Comic archives contain the paginated edition as numbered PNG files.

use std::io::{Cursor, Write};

use mbook::Book;
use unpdf::PdfParser;
use unpdf::parser::raster::RasterOptions;
use xmlwriter::{Options, XmlWriter};
use zip::{ZipWriter, write::SimpleFileOptions};

use super::{Format, Result, element, export_error, pdf};

pub(super) fn write(book: &Book, sink: &mut impl Write) -> Result<()> {
    let edition = pdf::render(book, Format::Cbz)?;
    let render_error = |error| export_error(Format::Cbz, error);
    let parser = PdfParser::from_bytes(&edition).map_err(render_error)?;
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    let zip_error = |error| export_error(Format::Cbz, error);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for page in 1..=parser.page_count() {
        let image = parser
            .render_page(page, &RasterOptions::default())
            .map_err(render_error)?;
        archive
            .start_file(format!("{page:010}.png"), options)
            .map_err(zip_error)?;
        archive.write_all(&image.to_png())?;
    }
    archive
        .start_file("ComicInfo.xml", options)
        .map_err(zip_error)?;
    archive.write_all(comic_info(book).as_bytes())?;
    let bytes = archive.finish().map_err(zip_error)?.into_inner();
    sink.write_all(&bytes)?;
    Ok(())
}

fn comic_info(book: &Book) -> String {
    let mut xml = XmlWriter::new(Options::default());
    xml.start_element("ComicInfo");
    let meta = book.meta();
    element(&mut xml, "Title", meta.title());
    element(&mut xml, "LanguageISO", meta.language());
    let authors = meta.authors().collect::<Vec<_>>().join(", ");
    if !authors.is_empty() {
        element(&mut xml, "Writer", &authors);
    }
    if let Some(annotation) = meta.annotation() {
        element(&mut xml, "Summary", annotation);
    }
    xml.end_document()
}
