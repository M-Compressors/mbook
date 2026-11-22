//! Export a book through the writer of the requested format.

mod blocks;
mod cbz;
mod docx;
mod epub;
mod fb2;
mod markup;
mod pdf;
mod picture;

use std::io::Write;

use mbook::{Book, Chapter, Picture};
use percent_encoding::percent_decode_str;
use pulldown_cmark::{Options, Parser};

use crate::{Error, Format, Result};

pub(super) fn write(book: &Book, format: Format, mut sink: impl Write) -> Result<()> {
    match format {
        Format::Epub => epub::write(book, &mut sink),
        Format::Fb2 => fb2::write(book, &mut sink),
        Format::Pdf => sink
            .write_all(&pdf::render(book, format)?)
            .map_err(Error::Io),
        Format::Cbz => cbz::write(book, &mut sink),
        Format::Docx => docx::write(book, &mut sink),
        Format::Html => markup::write_html(book, &mut sink),
        Format::Markdown => markup::write_markdown(book, &mut sink),
        Format::Text => blocks::write_text(book, &mut sink),
    }
}

fn parser(chapter: &Chapter) -> Parser<'_> {
    Parser::new_ext(chapter.text(), parser_options())
}

fn parser_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
}

/// Resolve a resource path according to RFC 0001.
fn destination<'a>(from: &str, url: &'a str) -> Option<(String, Option<&'a str>)> {
    if url.starts_with('/') || url.contains('?') {
        return None;
    }
    let (path, anchor) = url
        .split_once('#')
        .map_or((url, None), |(path, anchor)| (path, Some(anchor)));
    let path = percent_decode_str(path).decode_utf8().ok()?;
    if path.is_empty() {
        return Some((from.to_owned(), anchor));
    }
    let mut segments: Vec<_> = from.split('/').collect();
    segments.pop();
    for part in path.split('/') {
        match part {
            "" => return None,
            "." => {}
            ".." => {
                segments.pop()?;
            }
            name => segments.push(name),
        }
    }
    Some((segments.join("/"), anchor))
}

fn linked_picture<'a>(book: &'a Book, chapter: &Chapter, url: &str) -> Option<&'a Picture> {
    let (path, None) = destination(chapter.path(), url)? else {
        return None;
    };
    book.picture(&path)
}

fn export_error(
    format: Format,
    error: impl Into<Box<dyn std::error::Error + Send + Sync>>,
) -> Error {
    Error::Export {
        format,
        source: error.into(),
    }
}

fn writer_error(
    format: Format,
    message: impl Into<Box<dyn std::error::Error + Send + Sync>>,
) -> Error {
    export_error(format, std::io::Error::other(message))
}

fn element(xml: &mut xmlwriter::XmlWriter, name: &str, text: &str) {
    xml.start_element(name);
    xml.set_preserve_whitespaces(true);
    xml.write_text(&quick_xml::escape::escape(text));
    xml.end_element();
    xml.set_preserve_whitespaces(false);
}
