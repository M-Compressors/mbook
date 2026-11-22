//! Reflowable documents and their rewritten links.

use std::collections::HashMap;
use std::rc::Rc;

use base64::{Engine as _, engine::general_purpose::STANDARD};
use mbook::{Book, Chapter};
use percent_encoding::percent_decode_str;
use pulldown_cmark::{CowStr, Event, LinkType, Tag, TagEnd};

use super::graphic::{Drawings, Drawn, Graphic};
use super::{alone, destination, linked_graphic, parser, setting};
use crate::figure::Setting;
use crate::{Format, Result};

/// The size of the body text of a page, in CSS pixels.
pub(super) const TEXT: f32 = 16.0;
/// The pixels of a figure for each CSS pixel: sharp on a dense screen.
pub(super) const DENSITY: f32 = 2.0;

/// How a writer embeds a graphic.
pub(super) enum Embedded {
    /// A picture at its own size, written as a Markdown picture.
    Picture(String),
    /// A diagram or a formula drawn as PNG, written as an `img` element with
    /// the size it takes in the text.
    Figure { url: String, drawn: Rc<Drawn> },
}

/// A figure whose alternative text is being read.
struct Pending<'a> {
    url: String,
    drawn: Rc<Drawn>,
    title: CowStr<'a>,
    alt: String,
    /// The pictures open inside the alternative text, this one included.
    depth: usize,
}

impl Pending<'_> {
    /// Reads an event of the alternative text; `true` once the picture
    /// closes.
    fn read(&mut self, event: &Event<'_>) -> bool {
        match event {
            Event::Start(Tag::Image { .. }) => self.depth += 1,
            Event::End(TagEnd::Image) => self.depth -= 1,
            Event::Text(text) | Event::Code(text) => self.alt.push_str(text),
            Event::SoftBreak | Event::HardBreak => self.alt.push(' '),
            _ => {}
        }
        self.depth == 0
    }

    /// The `img` element. A figure alone in its paragraph is centred at its
    /// width, and the page may make it narrower; one inside a line takes the
    /// height of its drawing and sits on the baseline of the line.
    fn element(&self) -> String {
        let drawn = &self.drawn;
        let style = match drawn.figure {
            Some(Setting::Line) => format!(
                "height: {}px; vertical-align: -{}px",
                number(drawn.height),
                number(drawn.height - drawn.baseline)
            ),
            _ => format!(
                "display: block; margin: 0 auto; width: {}px",
                number(drawn.width)
            ),
        };
        let title = if self.title.is_empty() {
            String::new()
        } else {
            format!(
                " title=\"{}\"",
                html_escape::encode_double_quoted_attribute(&self.title)
            )
        };
        format!(
            "<img src=\"{}\" alt=\"{}\"{title} style=\"{style}\" />",
            html_escape::encode_double_quoted_attribute(&self.url),
            html_escape::encode_double_quoted_attribute(&self.alt)
        )
    }
}

/// A length for CSS, to a hundredth.
fn number(length: f32) -> String {
    let written = format!("{:.2}", length.max(0.0));
    written
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

/// A diagram or a formula as a PNG data address, each encoded once. `None`
/// for a figure that cannot be drawn.
pub(super) fn figure_data_url<'a>(
    drawings: &mut Drawings<'a>,
    urls: &mut HashMap<(&'a str, Option<Setting>), String>,
    figure: Graphic<'a>,
) -> Result<Option<Embedded>> {
    let Some(drawn) = drawings.draw(figure)? else {
        return Ok(None);
    };
    let url = urls
        .entry(figure.key())
        .or_insert_with(|| {
            format!(
                "data:image/png;base64,{}",
                STANDARD.encode(&drawn.png.bytes)
            )
        })
        .clone();
    Ok(Some(Embedded::Figure { url, drawn }))
}

pub(super) enum Layout {
    Epub,
    SingleFile,
}

#[derive(Clone, Copy)]
struct Place {
    chapter: usize,
    anchor: Option<usize>,
}

impl Place {
    fn id(&self) -> String {
        self.anchor.map_or_else(
            || format!("chapter-{}", self.chapter + 1),
            |anchor| format!("chapter-{}-anchor-{}", self.chapter + 1, anchor + 1),
        )
    }

    fn href(&self, layout: &Layout) -> String {
        let file = match layout {
            Layout::Epub => chapter_file(self.chapter),
            Layout::SingleFile => String::new(),
        };
        format!("{file}#{}", self.id())
    }
}

pub(super) fn chapter_file(chapter: usize) -> String {
    format!("chapter-{}.xhtml", chapter + 1)
}

fn anchors(chapter: &Chapter) -> impl Iterator<Item = &mbook::Anchor> {
    chapter.anchors().iter().chain(chapter.extra_anchors())
}

fn target(book: &Book, from: &Chapter, url: &str, layout: &Layout) -> Option<String> {
    if url.split_once(':').is_some_and(|(scheme, _)| {
        ["http", "https", "mailto"]
            .iter()
            .any(|name| scheme.eq_ignore_ascii_case(name))
    }) {
        return Some(url.to_owned());
    }
    let (path, fragment) = destination(from.path(), url)?;
    let fragment = fragment
        .map(|name| percent_decode_str(name).decode_utf8())
        .transpose()
        .ok()?;
    let chapter = book
        .chapters()
        .iter()
        .position(|chapter| chapter.path() == path)?;
    let anchor = match fragment {
        Some(name) => {
            Some(anchors(&book.chapters()[chapter]).position(|anchor| anchor.name == name)?)
        }
        None => None,
    };
    Some(Place { chapter, anchor }.href(layout))
}

/// Rewrite resource destinations before rendering. Generated anchors are
/// namespaced by chapter, as are footnote labels in a merged document.
fn events<'a>(
    book: &'a Book,
    index: usize,
    layout: &Layout,
    mut embed: impl FnMut(Graphic<'a>) -> Result<Option<Embedded>>,
) -> Result<Vec<Event<'a>>> {
    let chapter = &book.chapters()[index];
    let mut result = vec![anchor_event(Place {
        chapter: index,
        anchor: None,
    })];
    let mut offsets: Vec<_> = anchors(chapter)
        .enumerate()
        .map(|(i, anchor)| (anchor.offset as usize, i))
        .collect();
    offsets.sort_unstable();
    let mut offsets = offsets.into_iter().peekable();
    let mut links = Vec::new();
    let mut images = Vec::new();
    let mut figure: Option<Pending<'a>> = None;
    let alone = alone(chapter);
    let footnote = |name| format!("chapter-{}-note-{name}", index + 1).into();

    for (event, range) in parser(chapter).into_offset_iter() {
        while let Some(&(_, anchor)) = offsets.peek().filter(|(offset, _)| *offset <= range.start) {
            result.push(anchor_event(Place {
                chapter: index,
                anchor: Some(anchor),
            }));
            offsets.next();
        }
        if let Some(pending) = &mut figure {
            if pending.read(&event) {
                result.push(Event::InlineHtml(pending.element().into()));
                figure = None;
            }
            continue;
        }
        let event = match event {
            Event::Html(_) | Event::InlineHtml(_) => continue,
            Event::Start(Tag::Link {
                dest_url, title, ..
            }) => {
                let url = target(book, chapter, &dest_url, layout);
                links.push(url.is_some());
                let Some(url) = url else { continue };
                Event::Start(Tag::Link {
                    link_type: LinkType::Inline,
                    dest_url: url.into(),
                    title,
                    id: "".into(),
                })
            }
            Event::Start(Tag::Image {
                dest_url, title, ..
            }) => {
                let setting = setting(&alone, range.start);
                let embedded = match linked_graphic(book, chapter, &dest_url, setting) {
                    Some(graphic) => embed(graphic)?,
                    None => None,
                };
                match embedded {
                    Some(Embedded::Picture(url)) => {
                        images.push(true);
                        Event::Start(Tag::Image {
                            link_type: LinkType::Inline,
                            dest_url: url.into(),
                            title,
                            id: "".into(),
                        })
                    }
                    Some(Embedded::Figure { url, drawn }) => {
                        figure = Some(Pending {
                            url,
                            drawn,
                            title,
                            alt: String::new(),
                            depth: 1,
                        });
                        continue;
                    }
                    // The alternative text stands in its place.
                    None => {
                        images.push(false);
                        continue;
                    }
                }
            }
            Event::End(TagEnd::Link) if links.pop() == Some(false) => continue,
            Event::End(TagEnd::Image) if images.pop() == Some(false) => continue,
            Event::Start(Tag::FootnoteDefinition(name)) => {
                Event::Start(Tag::FootnoteDefinition(footnote(name)))
            }
            Event::FootnoteReference(name) => Event::FootnoteReference(footnote(name)),
            other => other,
        };
        result.push(event);
    }
    for (_, anchor) in offsets {
        result.push(anchor_event(Place {
            chapter: index,
            anchor: Some(anchor),
        }));
    }
    Ok(result)
}

fn anchor_event(place: Place) -> Event<'static> {
    Event::InlineHtml(format!("<span id=\"{}\"></span>", place.id()).into())
}

pub(super) fn chapter_html<'a>(
    book: &'a Book,
    index: usize,
    layout: &Layout,
    embed: impl FnMut(Graphic<'a>) -> Result<Option<Embedded>>,
) -> Result<String> {
    let mut output = String::new();
    pulldown_cmark::html::push_html(&mut output, events(book, index, layout, embed)?.into_iter());
    Ok(output)
}

pub(super) const CSS: &str = "body { margin: 1em auto; max-width: 44em; padding: 0 1em; } img { max-width: 100%; height: auto; } pre { white-space: pre-wrap; }";

pub(super) fn title_and_language(
    book: &Book,
) -> (std::borrow::Cow<'_, str>, std::borrow::Cow<'_, str>) {
    (
        html_escape::encode_text(book.meta().title()),
        html_escape::encode_double_quoted_attribute(book.meta().language()),
    )
}

pub(super) fn contents(book: &Book, index: usize, layout: &Layout) -> Vec<(usize, String, String)> {
    let chapter = &book.chapters()[index];
    let href = |anchor| {
        Place {
            chapter: index,
            anchor,
        }
        .href(layout)
    };
    match book.toc() {
        mbook::Toc::Explicit(items) => items
            .iter()
            .filter(|item| item.chapter == chapter.path())
            .filter_map(|item| {
                let anchor = match &item.anchor {
                    Some(name) => Some(anchors(chapter).position(|anchor| &anchor.name == name)?),
                    None => None,
                };
                Some((usize::from(item.level), item.title.clone(), href(anchor)))
            })
            .collect(),
        mbook::Toc::FromHeadings { max_level } => {
            let mut items = Vec::new();
            let mut active = None;
            for (event, range) in parser(chapter).into_offset_iter() {
                match event {
                    Event::Start(Tag::Heading { level, .. }) if level as u8 <= *max_level => {
                        if let Some(anchor) = anchors(chapter)
                            .position(|anchor| anchor.offset as usize == range.start)
                        {
                            active = Some((level as usize, String::new(), href(Some(anchor))));
                        }
                    }
                    Event::Text(text) | Event::Code(text) => {
                        if let Some((_, title, _)) = &mut active {
                            title.push_str(&text);
                        }
                    }
                    Event::End(TagEnd::Heading(_)) => items.extend(active.take()),
                    _ => {}
                }
            }
            items
        }
    }
}

pub(super) fn write_html(book: &Book, sink: &mut impl std::io::Write) -> Result<()> {
    let (title, language) = title_and_language(book);
    writeln!(
        sink,
        "<!DOCTYPE html><html lang=\"{language}\"><head><meta charset=\"utf-8\"><title>{title}</title><style>{CSS}</style>"
    )?;
    for author in book.meta().authors() {
        let author = html_escape::encode_double_quoted_attribute(author);
        writeln!(sink, "<meta name=\"author\" content=\"{author}\">")?;
    }
    writeln!(sink, "</head><body>")?;
    let mut drawings = Drawings::new(Format::Html, TEXT, DENSITY);
    let mut pictures = HashMap::new();
    let mut figures = HashMap::new();
    let mut embed = |graphic| match graphic {
        Graphic::Picture(picture) => {
            png_data_url(pictures.entry(picture.path().to_owned()), picture)
                .map(|url| Some(Embedded::Picture(url)))
        }
        figure => figure_data_url(&mut drawings, &mut figures, figure),
    };
    if let Some(cover) = book.meta().cover().and_then(|path| book.picture(path))
        && let Some(Embedded::Picture(url)) = embed(Graphic::Picture(cover))?
    {
        writeln!(sink, "<p><img src=\"{url}\" alt=\"Cover\"></p>")?;
    }
    writeln!(sink, "<h1>{title}</h1>")?;
    if !book.chapters().is_empty() {
        writeln!(sink, "<nav aria-label=\"Contents\"><ul>")?;
        for index in 0..book.chapters().len() {
            let entries = contents(book, index, &Layout::SingleFile);
            if entries.is_empty() {
                writeln!(
                    sink,
                    "<li><a href=\"#chapter-{}\">Chapter {}</a></li>",
                    index + 1,
                    index + 1
                )?;
            }
            for (_, label, link) in entries {
                writeln!(
                    sink,
                    "<li><a href=\"{link}\">{}</a></li>",
                    html_escape::encode_text(&label)
                )?;
            }
        }
        writeln!(sink, "</ul></nav>")?;
    }
    for index in 0..book.chapters().len() {
        writeln!(
            sink,
            "<article>{}</article>",
            chapter_html(book, index, &Layout::SingleFile, &mut embed)?
        )?;
    }
    writeln!(sink, "</body></html>")?;
    Ok(())
}

fn png_data_url(
    entry: std::collections::hash_map::Entry<'_, String, String>,
    picture: &mbook::Picture,
) -> Result<String> {
    use std::collections::hash_map::Entry;

    Ok(match entry {
        Entry::Occupied(entry) => entry.get().clone(),
        Entry::Vacant(entry) => {
            let png = super::picture::png(picture, crate::Format::Html)?;
            entry
                .insert(format!(
                    "data:image/png;base64,{}",
                    STANDARD.encode(png.bytes)
                ))
                .clone()
        }
    })
}

pub(super) fn write_markdown(book: &Book, sink: &mut impl std::io::Write) -> Result<()> {
    use pulldown_cmark_to_cmark::cmark;

    writeln!(sink, "# {}", crate::markdown::escape(book.meta().title()))?;
    let avif_data_url = |picture: &mbook::Picture| {
        format!("data:image/avif;base64,{}", STANDARD.encode(picture.data()))
    };
    if let Some(cover) = book.meta().cover().and_then(|path| book.picture(path)) {
        writeln!(sink, "\n![Cover]({})", avif_data_url(cover))?;
    }
    let mut drawings = Drawings::new(Format::Markdown, TEXT, DENSITY);
    let mut figures = HashMap::new();
    for index in 0..book.chapters().len() {
        let embed = |graphic| match graphic {
            Graphic::Picture(picture) => Ok(Some(Embedded::Picture(avif_data_url(picture)))),
            figure => figure_data_url(&mut drawings, &mut figures, figure),
        };
        let mut body = String::new();
        cmark(
            events(book, index, &Layout::SingleFile, embed)?,
            &mut body,
            super::parser_options(),
        )
        .map_err(|source| crate::Error::Export {
            format: crate::Format::Markdown,
            source: Box::new(source),
        })?;
        writeln!(sink, "\n{body}")?;
    }
    Ok(())
}
