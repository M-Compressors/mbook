//! Word paragraphs, headings and embedded pictures.

use std::io::{Cursor, Write};

use docx_rs::{BreakType, Docx, Paragraph, Pic, Run, RunFonts, Style, StyleType};
use mbook::{Book, Meta, Picture};
use xmlwriter::{Options, XmlWriter};

use super::blocks::{self, Inline};
use super::{Format, Result, export_error, picture};

pub(super) fn write(book: &Book, sink: &mut impl Write) -> Result<()> {
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
    if let Some(cover) = book.meta().cover().and_then(|path| book.picture(path)) {
        document = document.add_paragraph(Paragraph::new().add_run(picture_run(cover)?));
    }
    for chapter in book.chapters() {
        for block in blocks::chapter(book, chapter) {
            let mut paragraph = Paragraph::new();
            if let Some(level) = block.heading {
                paragraph = paragraph.style(&format!("Heading{level}"));
            }
            for span in block.content {
                let run = match span {
                    Inline::Text(text, style) => text_run(&text, style),
                    Inline::Picture(picture, _) => picture_run(picture)?,
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

fn picture_run(picture: &Picture) -> Result<Run> {
    let png = picture::png(picture, Format::Docx)?;
    let picture = Pic::new_with_dimensions(png.bytes, png.width, png.height);
    let (width, height) = picture.size;
    let limit = 5_650_000;
    let pic = if width.max(height) > limit {
        let scale = |edge| {
            u32::try_from(u64::from(edge) * u64::from(limit) / u64::from(width.max(height)))
                .unwrap_or(limit)
                .max(1)
        };
        picture.size(scale(width), scale(height))
    } else {
        picture
    };
    Ok(Run::new().add_image(pic))
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
