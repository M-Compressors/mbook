//! The reading order and common inline content used by non-HTML writers.

use mbook::{Book, Chapter};
use pulldown_cmark::{Event, Tag, TagEnd};

use super::graphic::Graphic;
use super::{alone, linked_graphic, parser, setting};

#[derive(Default)]
pub(super) struct Block<'a> {
    pub(super) heading: Option<usize>,
    /// The item of a list the block belongs to.
    pub(super) item: Option<Item>,
    pub(super) content: Vec<Inline<'a>>,
}

/// Where a block stands in a list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Item {
    /// The list, counted from 0 in the order the lists of the chapter open.
    pub(super) list: usize,
    /// How many lists hold the list: 0 for a list inside no other.
    pub(super) level: usize,
    /// The marker of the item, on its first block only.
    pub(super) marker: Option<Marker>,
}

/// The marker of an item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Marker {
    Bullet,
    Number(u64),
}

impl Marker {
    /// The marker as the text in front of the item, for a writer that has
    /// no lists.
    pub(super) fn text(self) -> String {
        match self {
            Self::Bullet => "• ".to_owned(),
            Self::Number(number) => format!("{number}. "),
        }
    }
}

/// A list being read.
struct List {
    /// The list, counted from 0 in the order the lists of the chapter open.
    id: usize,
    /// The number of its next item; `None` for bullets.
    next: Option<u64>,
    /// The marker of the item whose first block is still to come.
    marker: Option<Marker>,
}

/// The lists that hold the block being read.
#[derive(Default)]
struct Lists {
    /// The open lists, the innermost last.
    open: Vec<List>,
    /// How many lists of the chapter have opened.
    opened: usize,
}

impl Lists {
    /// Opens a list whose first item has this number, or bullets.
    fn open(&mut self, next: Option<u64>) {
        self.open.push(List {
            id: self.opened,
            next,
            marker: None,
        });
        self.opened += 1;
    }

    /// Starts the next item of the innermost list.
    fn start_item(&mut self) {
        if let Some(list) = self.open.last_mut() {
            list.marker = Some(match &mut list.next {
                Some(number) => {
                    let marker = Marker::Number(*number);
                    *number = number.saturating_add(1);
                    marker
                }
                None => Marker::Bullet,
            });
        }
    }
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
    /// A picture, a diagram or a formula, and its alternative text.
    Graphic(Graphic<'a>, String),
}

impl Block<'_> {
    fn text(&mut self, text: impl Into<String>, style: Style) {
        self.content.push(Inline::Text(text.into(), style));
    }
}

/// Lower rich Markdown blocks to paragraphs while keeping their reading order.
/// Every block of an item of a list is a paragraph of its own that knows its
/// item; tables and footnotes retain readable markers in a paragraph.
pub(super) fn chapter<'a>(book: &'a Book, chapter: &'a Chapter) -> Vec<Block<'a>> {
    let mut blocks = Vec::new();
    let mut block = Block::default();
    let mut style = Style::default();
    let mut lists = Lists::default();
    let mut image: Option<(Option<Graphic<'a>>, String)> = None;
    let mut table_cell = false;
    let alone = alone(chapter);
    for (event, range) in parser(chapter).into_offset_iter() {
        if let Some((graphic, alt)) = &mut image {
            match event {
                Event::End(TagEnd::Image) => {
                    let alt = std::mem::take(alt);
                    if let Some(graphic) = *graphic {
                        block.content.push(Inline::Graphic(graphic, alt));
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
                let setting = setting(&alone, range.start);
                image = Some((
                    linked_graphic(book, chapter, &dest_url, setting),
                    String::new(),
                ));
            }
            Event::Start(Tag::List(next)) => {
                // A tight item holds its text and then its nested list.
                finish(&mut blocks, &mut block, &mut lists);
                lists.open(next);
            }
            Event::End(TagEnd::List(_)) => {
                finish(&mut blocks, &mut block, &mut lists);
                lists.open.pop();
            }
            Event::Start(Tag::Item) => {
                finish(&mut blocks, &mut block, &mut lists);
                lists.start_item();
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
                finish(&mut blocks, &mut block, &mut lists);
            }
            Event::End(
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::Item
                | TagEnd::CodeBlock
                | TagEnd::FootnoteDefinition,
            ) => {
                finish(&mut blocks, &mut block, &mut lists);
            }
            other => update_style(&mut style, &other),
        }
    }
    finish(&mut blocks, &mut block, &mut lists);
    blocks
}

/// Ends the block being read. A block inside a list belongs to the item
/// being read; the first block of an item takes its marker, and an item
/// without text is still a block of its own, so that it keeps its marker.
fn finish<'a>(blocks: &mut Vec<Block<'a>>, block: &mut Block<'a>, lists: &mut Lists) {
    let level = lists.open.len().saturating_sub(1);
    let marker = lists.open.last_mut().and_then(|list| list.marker.take());
    if block.content.is_empty() && marker.is_none() {
        return;
    }
    block.item = lists.open.last().map(|list| Item {
        list: list.id,
        level,
        marker,
    });
    blocks.push(std::mem::take(block));
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
            if let Some(item) = block.item {
                sink.write_all("  ".repeat(item.level).as_bytes())?;
                if let Some(marker) = item.marker {
                    sink.write_all(marker.text().as_bytes())?;
                }
            }
            for inline in block.content {
                let text = match inline {
                    Inline::Text(text, _) | Inline::Graphic(_, text) => text,
                };
                sink.write_all(text.as_bytes())?;
            }
            writeln!(sink)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Lists, Marker};

    #[test]
    fn the_items_of_a_numbered_list_count_up_from_its_start() {
        let mut lists = Lists::default();
        lists.open(Some(3));
        lists.start_item();
        let first = lists.open[0].marker.take();
        lists.start_item();
        assert_eq!(
            [first, lists.open[0].marker],
            [Some(Marker::Number(3)), Some(Marker::Number(4))]
        );
    }

    #[test]
    fn an_item_outside_a_list_has_no_marker() {
        let mut lists = Lists::default();
        lists.start_item();
        assert!(lists.open.is_empty());
    }
}
