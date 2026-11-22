//! Reflowable documents and their rewritten links.

use mbook::{Book, Chapter, Picture};
use percent_encoding::percent_decode_str;
use pulldown_cmark::{Event, LinkType, Tag, TagEnd};

use super::{destination, linked_picture, parser};
use crate::Result;

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
    mut image_url: impl FnMut(&'a Picture) -> Result<String>,
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
    let footnote = |name| format!("chapter-{}-note-{name}", index + 1).into();

    for (event, range) in parser(chapter).into_offset_iter() {
        while let Some(&(_, anchor)) = offsets.peek().filter(|(offset, _)| *offset <= range.start) {
            result.push(anchor_event(Place {
                chapter: index,
                anchor: Some(anchor),
            }));
            offsets.next();
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
                let picture = linked_picture(book, chapter, &dest_url);
                images.push(picture.is_some());
                let Some(picture) = picture else { continue };
                Event::Start(Tag::Image {
                    link_type: LinkType::Inline,
                    dest_url: image_url(picture)?.into(),
                    title,
                    id: "".into(),
                })
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
    image_url: impl FnMut(&'a Picture) -> Result<String>,
) -> Result<String> {
    let mut output = String::new();
    pulldown_cmark::html::push_html(
        &mut output,
        events(book, index, layout, image_url)?.into_iter(),
    );
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
    use std::collections::HashMap;

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
    let mut pictures = HashMap::new();
    let mut image_url =
        |picture: &Picture| png_data_url(pictures.entry(picture.path().to_owned()), picture);
    if let Some(cover) = book.meta().cover().and_then(|path| book.picture(path)) {
        writeln!(
            sink,
            "<p><img src=\"{}\" alt=\"Cover\"></p>",
            image_url(cover)?
        )?;
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
            chapter_html(book, index, &Layout::SingleFile, &mut image_url)?
        )?;
    }
    writeln!(sink, "</body></html>")?;
    Ok(())
}

fn png_data_url(
    entry: std::collections::hash_map::Entry<'_, String, String>,
    picture: &Picture,
) -> Result<String> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
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
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use pulldown_cmark_to_cmark::cmark;

    writeln!(sink, "# {}", crate::markdown::escape(book.meta().title()))?;
    let image_url = |picture: &Picture| {
        Ok(format!(
            "data:image/avif;base64,{}",
            STANDARD.encode(picture.data())
        ))
    };
    if let Some(cover) = book.meta().cover().and_then(|path| book.picture(path)) {
        writeln!(sink, "\n![Cover]({})", image_url(cover)?)?;
    }
    for index in 0..book.chapters().len() {
        let mut body = String::new();
        cmark(
            events(book, index, &Layout::SingleFile, image_url)?,
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
