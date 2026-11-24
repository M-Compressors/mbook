//! Word paragraphs, headings, lists and embedded pictures.

use std::collections::HashMap;
use std::io::{Cursor, Write};

use docx_rs::{
    AbstractNumbering, BreakType, Docx, IndentLevel, Level, LevelJc, LevelText, NumberFormat,
    Numbering, NumberingId, Paragraph, Pic, Run, RunFonts, SpecialIndentType, Start, Style,
    StyleType,
};
use mbook::{Book, Meta};
use xmlwriter::{Options, XmlWriter};

use super::blocks::{self, Inline, Item, Marker};
use super::graphic::{Drawings, Drawn, Graphic};
use super::{Format, Result, export_error};

/// The size of the body text of Word, 11 points, in the pixels of 1/96 inch
/// that `docx-rs` measures pictures in.
const TEXT: f32 = 11.0 * 96.0 / 72.0;
/// The pixels of a figure for each pixel it takes on the page.
const DENSITY: f32 = 2.0;
/// The English Metric Units of a pixel of 1/96 inch.
const EMU_PER_PIXEL: f32 = 9525.0;
/// The longest edge of a picture: about the width of a page between its
/// margins.
const LONGEST_EDGE: u32 = 5_650_000;
/// The levels a numbering of Word has.
const LEVELS: usize = 9;
/// The indent of each level of a list, in twentieths of a point: half an
/// inch.
const LEVEL_INDENT: i32 = 720;
/// How far the marker of an item hangs left of its text.
const MARKER_HANG: i32 = 360;
/// The bullets of the levels of a bulleted list, in turn.
const BULLETS: [&str; 3] = ["•", "◦", "▪"];
/// The first numbering of the document that is not the one `docx-rs` always
/// writes, which has the identifier 1.
const FIRST_NUMBERING: usize = 2;

pub(super) fn write(book: &Book, sink: &mut impl Write) -> Result<()> {
    let mut drawings = Drawings::new(Format::Docx, TEXT, DENSITY);
    let mut document = Docx::new().add_paragraph(
        Paragraph::new().add_run(Run::new().add_text(book.meta().title()).size(44).bold()),
    );
    for level in 1..=6 {
        let name = format!("Heading{level}");
        document = document.add_style(
            Style::new(name, StyleType::Paragraph)
                .name(format!("heading {level}"))
                .outline_lvl(level - 1)
                .size(34 - 2 * level)
                .bold(),
        );
    }
    if let Some(cover) = book.meta().cover().and_then(|path| book.picture(path))
        && let Some(drawn) = drawings.draw(Graphic::Picture(cover))?
    {
        document = document.add_paragraph(Paragraph::new().add_run(picture_run(&drawn)));
    }
    let mut next_numbering = FIRST_NUMBERING;
    for chapter in book.chapters() {
        // The numbering of each list of the chapter, by the list.
        let mut numberings = HashMap::new();
        for block in blocks::chapter(book, chapter) {
            let mut paragraph = Paragraph::new();
            if let Some(level) = block.heading {
                paragraph = paragraph.style(&format!("Heading{level}"));
            }
            if let Some(item) = block.item {
                let level = item.level.min(LEVELS - 1);
                paragraph = match item.marker {
                    Some(marker) => {
                        let id = if let Some(&id) = numberings.get(&item.list) {
                            id
                        } else {
                            let id = next_numbering;
                            next_numbering += 1;
                            numberings.insert(item.list, id);
                            document = document
                                .add_abstract_numbering(numbering(id, item, marker))
                                .add_numbering(Numbering::new(id, id));
                            id
                        };
                        paragraph.numbering(NumberingId::new(id), IndentLevel::new(level))
                    }
                    None => paragraph.indent(Some(indent(level)), None, None, None),
                };
            }
            for span in block.content {
                let run = match span {
                    Inline::Text(text, style) => text_run(&text, style),
                    Inline::Graphic(graphic, alt) => match drawings.draw(graphic)? {
                        Some(drawn) => picture_run(&drawn),
                        None => text_run(&alt, blocks::Style::default()),
                    },
                };
                paragraph = paragraph.add_run(run);
            }
            document = document.add_paragraph(paragraph);
        }
    }
    let mut archive = Cursor::new(Vec::new());
    let mut parts = document.build();
    parts.doc_props.core = metadata(book.meta());
    parts
        .pack(&mut archive)
        .map_err(|error| export_error(Format::Docx, error))?;
    sink.write_all(archive.get_ref())?;
    Ok(())
}

/// The numbering of one list, whose first item has this marker: every level
/// in the format of the list, the level of the list starting at the number
/// of that item. Each list has a numbering of its own, so that it counts its
/// items alone and from its own start, which `undoc` reads in `w:start`
/// only, not in an override of the level.
fn numbering(id: usize, item: Item, marker: Marker) -> AbstractNumbering {
    (0..LEVELS).fold(AbstractNumbering::new(id), |numbering, level| {
        let (start, format, text) = match marker {
            Marker::Bullet => (1, "bullet", BULLETS[level % BULLETS.len()].to_owned()),
            Marker::Number(first) => {
                let start = if level == item.level.min(LEVELS - 1) {
                    usize::try_from(first).unwrap_or(usize::MAX)
                } else {
                    1
                };
                (start, "decimal", format!("%{}.", level + 1))
            }
        };
        numbering.add_level(
            Level::new(
                level,
                Start::new(start),
                NumberFormat::new(format),
                LevelText::new(text),
                LevelJc::new("left"),
            )
            .indent(
                Some(indent(level)),
                Some(SpecialIndentType::Hanging(MARKER_HANG)),
                None,
                None,
            ),
        )
    })
}

/// The indent of the text of an item at this level.
fn indent(level: usize) -> i32 {
    // The level is below `LEVELS`.
    i32::try_from(level + 1).unwrap_or(1) * LEVEL_INDENT
}

fn text_run(text: &str, style: blocks::Style) -> Run {
    let mut run = Run::new();
    for (index, part) in text.split('\n').enumerate() {
        if index > 0 {
            run = run.add_break(BreakType::TextWrapping);
        }
        run = run.add_text(part);
    }
    if style.bold {
        run = run.bold();
    }
    if style.italic {
        run = run.italic();
    }
    if style.strike {
        run = run.strike();
    }
    if style.code == blocks::Code::Inline {
        run = run.fonts(RunFonts::new().ascii("Courier New").hi_ansi("Courier New"));
    }
    run
}

fn picture_run(drawn: &Drawn) -> Run {
    let picture =
        Pic::new_with_dimensions(drawn.png.bytes.clone(), drawn.png.width, drawn.png.height);
    let (width, height) = (emu(drawn.width), emu(drawn.height));
    let longest = width.max(height).max(1);
    let scale = |edge: u32| {
        if longest > LONGEST_EDGE {
            u32::try_from(u64::from(edge) * u64::from(LONGEST_EDGE) / u64::from(longest))
                .unwrap_or(LONGEST_EDGE)
                .max(1)
        } else {
            edge.max(1)
        }
    };
    Run::new().add_image(picture.size(scale(width), scale(height)))
}

/// A length in pixels as English Metric Units.
fn emu(pixels: f32) -> u32 {
    let emu = (pixels * EMU_PER_PIXEL).round().clamp(0.0, 4_294_967_040.0);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the value was clamped to the range of `u32`"
    )]
    let emu = emu as u32;
    emu
}

fn metadata(meta: &Meta) -> Vec<u8> {
    let mut xml = XmlWriter::new(Options::default());
    xml.start_element("cp:coreProperties");
    xml.write_attribute(
        "xmlns:cp",
        "http://schemas.openxmlformats.org/package/2006/metadata/core-properties",
    );
    xml.write_attribute("xmlns:dc", "http://purl.org/dc/elements/1.1/");
    let authors = meta.authors().collect::<Vec<_>>().join("; ");
    for (field, value) in [
        ("dc:title", meta.title()),
        ("dc:creator", &authors),
        ("dc:language", meta.language()),
        ("dc:description", meta.annotation().unwrap_or_default()),
    ] {
        if !value.is_empty() {
            super::element(&mut xml, field, value);
        }
    }
    xml.end_document().into_bytes()
}
