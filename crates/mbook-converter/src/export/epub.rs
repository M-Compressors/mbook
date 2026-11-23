//! EPUB 3 with one XHTML document per chapter.

mod landmarks;

use std::collections::HashMap;
use std::io::Write;
use std::rc::Rc;

use epub_builder::{EpubBuilder, EpubContent, EpubVersion, ReferenceType, TocElement, ZipLibrary};
use mbook::Book;

use super::graphic::{Drawings, Graphic};
use super::markup::{
    CSS, DENSITY, Embedded, Layout, TEXT, chapter_file, chapter_html, contents, title_and_language,
};
use super::{Format, Result, export_error, picture};
use landmarks::COVER_PAGE;

pub(super) fn write(book: &Book, sink: &mut impl Write) -> Result<()> {
    let epub_error = |error| export_error(Format::Epub, error);
    let zip = ZipLibrary::new().map_err(epub_error)?;
    let mut epub = EpubBuilder::new(zip).map_err(epub_error)?;
    epub.epub_version(EpubVersion::V30)
        .stylesheet(CSS.as_bytes())
        .map_err(epub_error)?;
    let meta = book.meta();
    epub.set_title(meta.title());
    epub.set_languages(meta.languages().map(str::to_owned).collect());
    epub.set_authors(meta.authors().map(str::to_owned).collect());
    epub.set_subjects(meta.genres().map(str::to_owned).collect());
    if let Some(description) = meta.annotation() {
        epub.add_description(description);
    }
    if let Ok(uuid) = uuid::Uuid::parse_str(meta.id().trim_start_matches("urn:uuid:")) {
        epub.set_uuid(uuid);
    }

    let (title, lang) = title_and_language(book);
    let document = |body: &str| {
        format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xml:lang=\"{lang}\"><head><title>{title}</title><link rel=\"stylesheet\" href=\"stylesheet.css\" /></head><body>{body}</body></html>"
        )
    };
    let mut pictures = HashMap::new();
    let mut cover = false;
    for (index, source) in book.pictures().iter().enumerate() {
        let name = format!("image-{}.png", index + 1);
        let png = picture::png(source, Format::Epub)?;
        if meta.cover() == Some(source.path()) {
            epub.add_cover_image(&name, png.bytes.as_slice(), "image/png")
                .map_err(epub_error)?;
            let page = document(&format!("<p><img src=\"{name}\" alt=\"Cover\" /></p>"));
            epub.add_content(
                EpubContent::new(COVER_PAGE, page.as_bytes())
                    .title("Cover")
                    .reftype(ReferenceType::Cover),
            )
            .map_err(epub_error)?;
            cover = true;
        } else {
            epub.add_resource(&name, png.bytes.as_slice(), "image/png")
                .map_err(epub_error)?;
        }
        pictures.insert(source.path(), name);
    }
    if book.chapters().is_empty() {
        let body = document(&format!("<h1>{title}</h1>"));
        epub.add_content(EpubContent::new(chapter_file(0), body.as_bytes()).title(meta.title()))
            .map_err(epub_error)?;
    }
    let mut drawings = Drawings::new(Format::Epub, TEXT, DENSITY);
    let mut figures = HashMap::new();
    let mut resources = Vec::new();
    for (index, _) in book.chapters().iter().enumerate() {
        let body = chapter_html(book, index, &Layout::Epub, |graphic| {
            if let Graphic::Picture(source) = graphic {
                return Ok(pictures.get(source.path()).cloned().map(Embedded::Picture));
            }
            let Some(drawn) = drawings.draw(graphic)? else {
                return Ok(None);
            };
            let url = figures
                .entry(graphic.key())
                .or_insert_with(|| {
                    let name = format!("figure-{}.png", resources.len() + 1);
                    resources.push((name.clone(), Rc::clone(&drawn)));
                    name
                })
                .clone();
            Ok(Some(Embedded::Figure { url, drawn }))
        })?;
        let body = document(&body);
        let entries = contents(book, index, &Layout::Epub);
        let label = entries.first().map_or_else(
            || format!("Chapter {}", index + 1),
            |(_, title, _)| title.clone(),
        );
        let mut content = EpubContent::new(chapter_file(index), body.as_bytes()).title(label);
        for (level, title, href) in entries {
            let level = i32::try_from(level).unwrap_or(1).max(2);
            content.toc.add(TocElement::new(href, title).level(level));
        }
        epub.add_content(content).map_err(epub_error)?;
    }
    for (name, drawn) in &resources {
        epub.add_resource(name, drawn.png.bytes.as_slice(), "image/png")
            .map_err(epub_error)?;
    }
    let mut generated = Vec::new();
    epub.generate(&mut generated).map_err(epub_error)?;
    let generated = landmarks::with_marks(generated, &landmarks::marks(book, cover))?;
    sink.write_all(&generated).map_err(Into::into)
}
