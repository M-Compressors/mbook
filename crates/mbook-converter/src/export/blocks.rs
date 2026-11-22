//! The reading order and common inline content used by non-HTML writers.

use mbook::{Book, Chapter, Picture};
use pulldown_cmark::{Event, Tag, TagEnd};

use super::{linked_picture, parser};

#[derive(Default)]
pub(super) struct Block<'a> {
    pub(super) heading: Option<usize>,
    pub(super) content: Vec<Inline<'a>>,
}

#[derive(Clone, Copy, Default)]
pub(super) struct Style {
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) strike: bool,
    pub(super) code: Code,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Code {
    #[default]
    Prose,
    Inline,
}

pub(super) enum Inline<'a> {
    Text(String, Style),
    Picture(&'a Picture, String),
}

impl Block<'_> {
    fn text(&mut self, text: impl Into<String>, style: Style) {
        self.content.push(Inline::Text(text.into(), style));
    }
}

/// Lower rich Markdown blocks to paragraphs while keeping their reading order.
/// Lists, tables and footnotes retain readable markers in a paragraph.
pub(super) fn chapter<'a>(book: &'a Book, chapter: &'a Chapter) -> Vec<Block<'a>> {
    let mut blocks = Vec::new();
    let mut block = Block::default();
    let mut style = Style::default();
    let mut lists = Vec::new();
    let mut image: Option<(Option<&Picture>, String)> = None;
    let mut table_cell = false;
    for event in parser(chapter) {
        if let Some((picture, alt)) = &mut image {
            match event {
                Event::End(TagEnd::Image) => {
                    let alt = std::mem::take(alt);
                    if let Some(picture) = *picture {
                        block.content.push(Inline::Picture(picture, alt));
                    } else {
                        block.text(alt, style);
                    }
                    image = None;
                }
                Event::Text(text) | Event::Code(text) => alt.push_str(&text),
                Event::SoftBreak | Event::HardBreak => alt.push(' '),
                _ => {}
            }
            continue;
        }
        match event {
            Event::Start(Tag::Heading { level, .. }) => block.heading = Some(level as usize),
            Event::Start(Tag::Image { dest_url, .. }) => {
                image = Some((linked_picture(book, chapter, &dest_url), String::new()));
            }
            Event::Start(Tag::List(number)) => lists.push(number),
            Event::End(TagEnd::List(_)) => {
                lists.pop();
            }
            Event::Start(Tag::Item) => {
                let marker = match lists.last_mut() {
                    Some(Some(number)) => {
                        let marker = format!("{number}. ");
                        *number = number.saturating_add(1);
                        marker
                    }
                    _ => "• ".to_owned(),
                };
                block.text(marker, style);
            }
            Event::Start(Tag::TableCell) => {
                if table_cell {
                    block.text(" | ", style);
                }
                table_cell = true;
            }
            Event::Start(Tag::FootnoteDefinition(name)) => block.text(format!("[{name}] "), style),
            Event::FootnoteReference(name) => block.text(format!("[{name}]"), style),
            Event::TaskListMarker(checked) => {
                block.text(if checked { "[x] " } else { "[ ] " }, style);
            }
            Event::Text(text) => block.text(text.as_ref(), style),
            Event::Code(text) => {
                let style = Style {
                    code: Code::Inline,
                    ..style
                };
                block.text(text.as_ref(), style);
            }
            Event::SoftBreak => block.text(" ", style),
            Event::HardBreak => block.text("\n", style),
            Event::Rule => block.text("———", style),
            Event::End(TagEnd::TableRow | TagEnd::TableHead) => {
                table_cell = false;
                finish(&mut blocks, &mut block);
            }
            Event::End(
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::Item
                | TagEnd::CodeBlock
                | TagEnd::FootnoteDefinition,
            ) => {
                finish(&mut blocks, &mut block);
            }
            other => update_style(&mut style, &other),
        }
    }
    finish(&mut blocks, &mut block);
    blocks
}

fn finish<'a>(blocks: &mut Vec<Block<'a>>, block: &mut Block<'a>) {
    if !block.content.is_empty() {
        blocks.push(std::mem::take(block));
    }
}

fn update_style(style: &mut Style, event: &Event<'_>) {
    match event {
        Event::Start(Tag::Strong) => style.bold = true,
        Event::End(TagEnd::Strong) => style.bold = false,
        Event::Start(Tag::Emphasis) => style.italic = true,
        Event::End(TagEnd::Emphasis) => style.italic = false,
        Event::Start(Tag::Strikethrough) => style.strike = true,
        Event::End(TagEnd::Strikethrough) => style.strike = false,
        _ => {}
    }
}

pub(super) fn write_text(book: &Book, sink: &mut impl std::io::Write) -> crate::Result<()> {
    writeln!(sink, "{}", book.meta().title())?;
    for chapter_source in book.chapters() {
        for block in chapter(book, chapter_source) {
            writeln!(sink)?;
            for inline in block.content {
                let text = match inline {
                    Inline::Text(text, _) | Inline::Picture(_, text) => text,
                };
                sink.write_all(text.as_bytes())?;
            }
            writeln!(sink)?;
        }
    }
    Ok(())
}
