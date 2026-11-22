//! `FictionBook` sections, metadata and inline or standalone pictures.

use std::collections::HashMap;
use std::io::Write;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use mbook::{Book, Meta};
use xmlwriter::{Indent, Options, XmlWriter};

use super::blocks::{self, Block, Inline};
use super::{Format, Result, element, picture};

pub(super) fn write(book: &Book, sink: &mut impl Write) -> Result<()> {
    let mut xml = XmlWriter::new(Options {
        indent: Indent::None,
        ..Options::default()
    });
    xml.start_element("FictionBook");
    xml.write_attribute("xmlns", "http://www.gribuser.ru/xml/fictionbook/2.0");
    xml.write_attribute("xmlns:xlink", "http://www.w3.org/1999/xlink");
    let pictures: HashMap<_, _> = book
        .pictures()
        .iter()
        .enumerate()
        .map(|(index, picture)| (picture.path(), format!("picture-{}.png", index + 1)))
        .collect();
    let cover = book.meta().cover().and_then(|path| pictures.get(path));
    description(&mut xml, book.meta(), cover.map(String::as_str));
    xml.start_element("body");
    if book.chapters().is_empty() {
        xml.start_element("section");
        element(&mut xml, "p", book.meta().title());
        xml.end_element();
    }
    for chapter in book.chapters() {
        xml.start_element("section");
        let mut blocks = blocks::chapter(book, chapter).into_iter().peekable();
        if blocks.peek().is_some_and(|block| block.heading.is_some()) {
            xml.start_element("title");
            if let Some(block) = blocks.next() {
                paragraph(&mut xml, block, &pictures);
            }
            xml.end_element();
        }
        let mut body_written = false;
        for block in blocks {
            paragraph(&mut xml, block, &pictures);
            body_written = true;
        }
        if !body_written {
            xml.start_element("empty-line");
            xml.end_element();
        }
        xml.end_element();
    }
    xml.end_element();
    for source in book.pictures() {
        let png = picture::png(source, Format::Fb2)?;
        xml.start_element("binary");
        xml.write_attribute("id", &pictures[source.path()]);
        xml.write_attribute("content-type", "image/png");
        xml.write_text(&STANDARD.encode(png.bytes));
        xml.end_element();
    }
    sink.write_all(xml.end_document().as_bytes())?;
    Ok(())
}

fn description(xml: &mut XmlWriter, meta: &Meta, cover: Option<&str>) {
    xml.start_element("description");
    xml.start_element("title-info");
    element(xml, "genre", "unrecognised");
    let mut authors = meta.authors().peekable();
    if authors.peek().is_none() {
        person(xml, "author", "Unknown");
    }
    for author in authors {
        person(xml, "author", author);
    }
    element(xml, "book-title", meta.title());
    if let Some(annotation) = meta.annotation() {
        xml.start_element("annotation");
        for paragraph in annotation.split("\n\n") {
            element(xml, "p", paragraph);
        }
        xml.end_element();
    }
    let genres = meta.genres().collect::<Vec<_>>().join(", ");
    if !genres.is_empty() {
        element(xml, "keywords", &genres);
    }
    if let Some(date) = meta.date() {
        element(xml, "date", date);
    }
    if let Some(id) = cover {
        xml.start_element("coverpage");
        image(xml, id, "");
        xml.end_element();
    }
    element(xml, "lang", meta.language());
    for translator in meta.translators() {
        person(xml, "translator", translator);
    }
    xml.end_element();
    xml.start_element("document-info");
    person(xml, "author", "mbook-converter");
    element(xml, "date", "");
    element(xml, "id", meta.id());
    element(xml, "version", "1.0");
    xml.end_element();
    xml.end_element();
}

fn person(xml: &mut XmlWriter, role: &str, name: &str) {
    xml.start_element(role);
    element(xml, "nickname", name);
    xml.end_element();
}

fn paragraph(xml: &mut XmlWriter, block: Block<'_>, pictures: &HashMap<&str, String>) {
    xml.start_element("p");
    for inline in block.content {
        match inline {
            Inline::Text(text, style) => {
                let tags = [
                    ("strong", style.bold || block.heading.is_some()),
                    ("emphasis", style.italic),
                    ("strikethrough", style.strike),
                    ("code", style.code == blocks::Code::Inline),
                ];
                let mut parents = 0;
                for (tag, active) in tags {
                    if active {
                        xml.start_element(tag);
                        parents += 1;
                    }
                }
                xml.write_text(&quick_xml::escape::escape(text));
                for _ in 0..parents {
                    xml.end_element();
                }
            }
            Inline::Picture(picture, alt) => image(xml, &pictures[picture.path()], &alt),
        }
    }
    xml.end_element();
}

fn image(xml: &mut XmlWriter, id: &str, alt: &str) {
    xml.start_element("image");
    xml.write_attribute("xlink:href", &format!("#{id}"));
    if !alt.is_empty() {
        xml.write_attribute("alt", &*quick_xml::escape::escape(alt));
    }
    xml.end_element();
}
