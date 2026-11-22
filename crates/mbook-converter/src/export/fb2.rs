//! `FictionBook` sections, metadata and inline or standalone pictures.

use std::collections::HashMap;
use std::io::Write;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use mbook::{Book, Meta};
use xmlwriter::{Indent, Options, XmlWriter};

use super::blocks::{self, Block, Inline};
use super::graphic::{Drawings, Graphic};
use super::{Format, Result, element, picture};
use crate::figure::Setting;

/// The size of the body text of a reader, in pixels. `FictionBook` has no
/// size for a picture, so a figure is drawn at the size it is shown.
const TEXT: f32 = 16.0;

/// The binaries of the document: every picture of the book, and the figures
/// as they are met.
struct Binaries<'a> {
    pictures: HashMap<&'a str, String>,
    drawings: Drawings<'a>,
    figures: HashMap<(&'a str, Option<Setting>), String>,
    drawn: Vec<(String, Vec<u8>)>,
}

impl<'a> Binaries<'a> {
    /// The binary of a graphic, or `None` for a figure that cannot be drawn.
    fn id(&mut self, graphic: Graphic<'a>) -> Result<Option<String>> {
        if let Graphic::Picture(picture) = graphic {
            return Ok(self.pictures.get(picture.path()).cloned());
        }
        let key = graphic.key();
        if let Some(id) = self.figures.get(&key) {
            return Ok(Some(id.clone()));
        }
        let Some(drawn) = self.drawings.draw(graphic)? else {
            return Ok(None);
        };
        let id = format!("figure-{}.png", self.drawn.len() + 1);
        self.drawn.push((id.clone(), drawn.png.bytes.clone()));
        self.figures.insert(key, id.clone());
        Ok(Some(id))
    }
}

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
    let mut binaries = Binaries {
        pictures,
        drawings: Drawings::new(Format::Fb2, TEXT, 1.0),
        figures: HashMap::new(),
        drawn: Vec::new(),
    };
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
                paragraph(&mut xml, block, &mut binaries)?;
            }
            xml.end_element();
        }
        let mut body_written = false;
        for block in blocks {
            paragraph(&mut xml, block, &mut binaries)?;
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
        binary(&mut xml, &binaries.pictures[source.path()], &png.bytes);
    }
    for (id, png) in &binaries.drawn {
        binary(&mut xml, id, png);
    }
    sink.write_all(xml.end_document().as_bytes())?;
    Ok(())
}

fn binary(xml: &mut XmlWriter, id: &str, png: &[u8]) {
    xml.start_element("binary");
    xml.write_attribute("id", id);
    xml.write_attribute("content-type", "image/png");
    xml.write_text(&STANDARD.encode(png));
    xml.end_element();
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

fn paragraph<'a>(xml: &mut XmlWriter, block: Block<'a>, binaries: &mut Binaries<'a>) -> Result<()> {
    xml.start_element("p");
    for inline in block.content {
        let (text, style) = match inline {
            Inline::Text(text, style) => (text, style),
            Inline::Graphic(graphic, alt) => match binaries.id(graphic)? {
                Some(id) => {
                    image(xml, &id, &alt);
                    continue;
                }
                None => (alt, blocks::Style::default()),
            },
        };
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
        xml.write_text(&quick_xml::escape::escape(&text));
        for _ in 0..parents {
            xml.end_element();
        }
    }
    xml.end_element();
    Ok(())
}

fn image(xml: &mut XmlWriter, id: &str, alt: &str) {
    xml.start_element("image");
    xml.write_attribute("xlink:href", &format!("#{id}"));
    if !alt.is_empty() {
        xml.write_attribute("alt", &*quick_xml::escape::escape(alt));
    }
    xml.end_element();
}
