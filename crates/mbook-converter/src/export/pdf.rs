//! A paginated, selectable-text edition used by PDF and CBZ export.

use krilla::Document;
use krilla::geom::{Point, Size, Transform};
use krilla::image::Image;
use krilla::metadata::Metadata;
use krilla::page::PageSettings;
use krilla::text::{Font, TextDirection};
use mbook::Book;

use super::blocks::{self, Inline};
use super::{Format, Result, picture, writer_error};

const PAGE_WIDTH: f32 = 595.0;
const PAGE_HEIGHT: f32 = 842.0;
const MARGIN: f32 = 48.0;
const WIDTH: f32 = PAGE_WIDTH - 2.0 * MARGIN;
const HEIGHT: f32 = PAGE_HEIGHT - 2.0 * MARGIN;
const FONT: &[u8] = include_bytes!("../../assets/NotoSans-Regular.ttf");

pub(super) fn render(book: &Book, format: Format) -> Result<Vec<u8>> {
    let page = PageSettings::from_wh(PAGE_WIDTH, PAGE_HEIGHT)
        .ok_or_else(|| writer_error(format, "invalid page size"))?;
    let font = Font::new(FONT.into(), 0)
        .ok_or_else(|| writer_error(format, "the bundled font cannot be read"))?;
    let mut document = Document::new();
    let meta = book.meta();
    document.set_metadata(
        Metadata::new()
            .title(meta.title().to_owned())
            .language(meta.language().to_owned())
            .authors(meta.authors().map(str::to_owned).collect()),
    );
    let mut layout = Layout {
        document,
        page,
        font,
        commands: Vec::new(),
        y: MARGIN,
        format,
    };
    layout.text(meta.title(), 22.0, 30);
    let authors = meta.authors().collect::<Vec<_>>().join(", ");
    layout.text(&authors, 12.0, 56);
    if let Some(cover) = meta.cover().and_then(|path| book.picture(path)) {
        layout.picture(cover)?;
        layout.finish_page();
    }
    for chapter in book.chapters() {
        for block in blocks::chapter(book, chapter) {
            let (size, columns) = match block.heading {
                Some(1) => (19.0, 36),
                Some(_) => (15.0, 46),
                None => (11.0, 64),
            };
            let mut text = String::new();
            for inline in block.content {
                match inline {
                    Inline::Text(span, _) => text.push_str(&span),
                    Inline::Picture(source, _) => {
                        layout.text(&text, size, columns);
                        text.clear();
                        layout.picture(source)?;
                    }
                }
            }
            layout.text(&text, size, columns);
            layout.y += 7.0;
        }
    }
    layout.finish_page();
    layout
        .document
        .finish()
        .map_err(|error| writer_error(format, format!("{error:?}")))
}

enum Line {
    Text { text: String, size: f32 },
    Picture { image: Image, size: Size },
}

struct Layout {
    document: Document,
    page: PageSettings,
    font: Font,
    commands: Vec<(f32, Line)>,
    y: f32,
    format: Format,
}

impl Layout {
    fn text(&mut self, text: &str, size: f32, columns: usize) {
        if text.is_empty() {
            return;
        }
        for line in text.split('\n') {
            for line in textwrap::wrap(line, columns) {
                let line_height = size * 1.45;
                self.place(
                    Line::Text {
                        text: line.into_owned(),
                        size,
                    },
                    line_height,
                );
            }
        }
    }

    fn picture(&mut self, source: &mbook::Picture) -> Result<()> {
        let png = picture::png(source, self.format)?;
        let image = Image::from_png(png.bytes.into(), true)
            .map_err(|error| writer_error(self.format, error))?;
        // The codec limit keeps decoded dimensions well below f32's exact
        // integer range for normal book illustrations.
        let width = f32::from(u16::try_from(png.width).unwrap_or(u16::MAX));
        let height = f32::from(u16::try_from(png.height).unwrap_or(u16::MAX));
        let scale = (WIDTH / width).min((HEIGHT - 8.0) / height).min(1.0);
        let size = Size::from_wh(width * scale, height * scale)
            .ok_or_else(|| writer_error(self.format, "invalid picture size"))?;
        self.place(Line::Picture { image, size }, height * scale + 8.0);
        Ok(())
    }

    fn place(&mut self, line: Line, height: f32) {
        if self.y + height > PAGE_HEIGHT - MARGIN {
            self.finish_page();
        }
        self.commands.push((self.y, line));
        self.y += height;
    }

    fn finish_page(&mut self) {
        if self.commands.is_empty() {
            return;
        }
        let mut page = self.document.start_page_with(self.page.clone());
        let mut surface = page.surface();
        for (y, line) in self.commands.drain(..) {
            match line {
                Line::Text { text, size } => surface.draw_text(
                    Point::from_xy(MARGIN, y + size),
                    self.font.clone(),
                    size,
                    &text,
                    false,
                    TextDirection::Auto,
                ),
                Line::Picture { image, size } => {
                    surface.push_transform(&Transform::from_translate(MARGIN, y));
                    surface.draw_image(image, size);
                    surface.pop();
                }
            }
        }
        surface.finish();
        page.finish();
        self.y = MARGIN;
    }
}
