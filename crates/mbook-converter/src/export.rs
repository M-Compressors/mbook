//! Export a book through the writer of the requested format.

mod blocks;
mod cbz;
mod docx;
mod epub;
mod fb2;
mod graphic;
mod markup;
mod pdf;
mod picture;

use std::collections::HashSet;
use std::io::Write;

use mbook::{Book, Chapter};
use percent_encoding::percent_decode_str;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::figure::Setting;
use crate::{Error, Format, Result};
use graphic::Graphic;

/// The font of the paginated edition and the fallback for the labels of a
/// diagram.
const FONT: &[u8] = include_bytes!("../assets/NotoSans-Regular.ttf");

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

/// The picture, diagram or formula a reference written as a picture leads
/// to, set as `setting` if it is a figure.
fn linked_graphic<'a>(
    book: &'a Book,
    chapter: &Chapter,
    url: &str,
    setting: Setting,
) -> Option<Graphic<'a>> {
    let (path, None) = destination(chapter.path(), url)? else {
        return None;
    };
    book.picture(&path)
        .map(Graphic::Picture)
        .or_else(|| {
            book.diagram(&path)
                .map(|diagram| Graphic::Diagram(diagram, setting))
        })
        .or_else(|| {
            book.formula(&path)
                .map(|formula| Graphic::Formula(formula, setting))
        })
}

/// Where the references written as pictures that stand alone in their
/// paragraph start. Such a formula is a display formula (RFC 0001,
/// "Rendering"); every other one is set in its line.
fn alone(chapter: &Chapter) -> HashSet<usize> {
    let mut alone = HashSet::new();
    // The start of the only picture of the paragraph so far, and whether
    // the paragraph holds anything else.
    let mut paragraph: Option<(Option<usize>, bool)> = None;
    let mut depth = 0_usize;
    for (event, range) in parser(chapter).into_offset_iter() {
        let Some((only, crowded)) = &mut paragraph else {
            if matches!(event, Event::Start(Tag::Paragraph)) {
                paragraph = Some((None, false));
            }
            continue;
        };
        match event {
            Event::End(TagEnd::Paragraph) => {
                if let (Some(start), false) = (*only, *crowded) {
                    alone.insert(start);
                }
                paragraph = None;
            }
            Event::Start(Tag::Image { .. }) => {
                if depth == 0 {
                    *crowded |= only.replace(range.start).is_some();
                }
                depth += 1;
            }
            Event::End(TagEnd::Image) => depth = depth.saturating_sub(1),
            // The alternative text of the picture.
            _ if depth > 0 => {}
            Event::Text(text) if text.trim().is_empty() => {}
            Event::SoftBreak => {}
            _ => *crowded = true,
        }
    }
    alone
}

/// How the reference that starts at `start` stands in its text.
fn setting(alone: &HashSet<usize>, start: usize) -> Setting {
    if alone.contains(&start) {
        Setting::Block
    } else {
        Setting::Line
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn starts(text: &str) -> Result<Vec<usize>, Box<dyn std::error::Error>> {
        let mut book = Book::new("Settings", "en")?;
        book.add_chapter("one.md", text)?;
        let mut starts: Vec<_> = alone(&book.chapters()[0]).into_iter().collect();
        starts.sort_unstable();
        Ok(starts)
    }

    #[test]
    fn a_picture_alone_in_its_paragraph_stands_alone() -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(starts("![a](a.tex)\n\n  ![b](b.tex)  \n")?, [0, 15]);
        Ok(())
    }

    #[test]
    fn a_picture_with_text_or_another_picture_stands_in_its_line()
    -> Result<(), Box<dyn std::error::Error>> {
        for text in [
            "Text ![a](a.tex)\n",
            "![a](a.tex) ![b](b.tex)\n",
            "![a](a.tex)\nand more\n",
            "*![a](a.tex)*\n",
            "[![a](a.tex)](b.md)\n",
            "- ![a](a.tex)\n",
        ] {
            assert!(starts(text)?.is_empty(), "{text}");
        }
        Ok(())
    }

    #[test]
    fn the_alternative_text_of_a_picture_does_not_crowd_its_paragraph()
    -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(starts("![a *b* `c`](a.tex)\n")?, [0]);
        assert_eq!(starts("![a\nb](a.tex)\n")?, [0]);
        Ok(())
    }

    #[test]
    fn a_setting_follows_where_the_reference_stands() {
        let alone = HashSet::from([4]);
        assert_eq!(setting(&alone, 4), Setting::Block);
        assert_eq!(setting(&alone, 5), Setting::Line);
    }
}
