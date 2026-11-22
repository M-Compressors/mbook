//! Building a book from a draft: the rules of "Assembling the book" in the
//! conversion rules of the docs, the same for every source format.

mod meta;
#[cfg(pictures)]
mod pictures;
pub(crate) mod rewrite;
mod split;

#[cfg(feature = "epub")]
use crate::draft::DraftLandmark;
#[cfg(outlines)]
use crate::draft::OutlineEntry;
use crate::draft::{Draft, Section};
use crate::error::{Error, Result};
use crate::markdown as md;
use crate::options::ConvertOptions;
use crate::report::Warning;
#[cfg(any(pictures, outlines))]
use crate::report::WarningKind;
#[cfg(feature = "epub")]
use mbook::Landmark;
use mbook::{Anchor, Book};
#[cfg(outlines)]
use mbook::{Toc, TocItem};
use percent_encoding::percent_decode_str;
use rewrite::{Fate, Rules, Scan};
use std::collections::{HashMap, HashSet};
use unicode_normalization::UnicodeNormalization;

/// One chapter of the book: a section of the draft, or a piece of one.
struct Part {
    /// The index of its section in the draft.
    section: usize,
    path: String,
    text: String,
}

/// A place a link of the source leads to.
#[derive(Debug, PartialEq, Eq)]
struct Target {
    section: usize,
    /// The name of a mark in that section; `None` is its start.
    mark: Option<String>,
}

/// What a destination of the source is.
#[derive(Debug, PartialEq, Eq)]
enum Destination {
    /// An address the format allows a link to have.
    External,
    /// An address with a scheme the format does not allow.
    Forbidden,
    /// A place in the source.
    Internal(Target),
    /// Nothing the draft has.
    Unknown,
}

/// A mark of the draft that some link leads to.
struct Mark {
    /// The chapter the mark is in.
    part: usize,
    /// Its name in the book.
    name: String,
    /// Whether the mark stands directly in front of a heading. The heading
    /// has an anchor of its own, which is the name: the mark is not kept.
    at_heading: bool,
}

/// Where the sections of a draft ended up.
struct Layout<'a> {
    /// The index of a section by its name.
    sections: HashMap<&'a str, usize>,
    /// The first chapter of each section; `None` for one without text.
    first_part: Vec<Option<usize>>,
    parts: Vec<Part>,
    /// The marks that are kept, by section and by the name in the draft.
    marks: HashMap<(usize, String), Mark>,
}

impl Layout<'_> {
    /// Tells what a destination written in a section of the draft is.
    fn classify(&self, from: usize, destination: &str) -> Destination {
        if let Some(scheme) = scheme(destination) {
            let allowed = ["http", "https", "mailto"]
                .iter()
                .any(|name| scheme.eq_ignore_ascii_case(name));
            return if allowed {
                Destination::External
            } else {
                Destination::Forbidden
            };
        }
        let (name, mark) = match destination.split_once('#') {
            Some((name, mark)) => (name, Some(mark)),
            None => (destination, None),
        };
        let decode = |text: &str| {
            percent_decode_str(text)
                .decode_utf8()
                .ok()
                .map(std::borrow::Cow::into_owned)
        };
        let Some(name) = decode(name) else {
            return Destination::Unknown;
        };
        let section = if name.is_empty() {
            Some(from)
        } else {
            self.sections.get(name.as_str()).copied()
        };
        match (section, mark.map(decode)) {
            (Some(section), None) => Destination::Internal(Target {
                section,
                mark: None,
            }),
            (Some(section), Some(Some(mark))) => Destination::Internal(Target {
                section,
                mark: Some(mark),
            }),
            _ => Destination::Unknown,
        }
    }

    /// The destination a link to a place of the source has in the book.
    fn destination(&self, target: &Target) -> Option<String> {
        match &target.mark {
            None => {
                let part = (*self.first_part.get(target.section)?)?;
                Some(md::encoded_path(&self.parts.get(part)?.path))
            }
            Some(name) => {
                let mark = self.marks.get(&(target.section, name.clone()))?;
                let path = md::encoded_path(&self.parts.get(mark.part)?.path);
                Some(format!("{path}#{}", md::encoded_name(&mark.name)))
            }
        }
    }

    /// What becomes of a link written in a section of the draft.
    fn fate(&self, from: usize, destination: &str) -> Fate {
        match self.classify(from, destination) {
            Destination::External => Fate::Keep,
            Destination::Internal(target) => {
                self.destination(&target).map_or(Fate::Drop, Fate::Replace)
            }
            Destination::Forbidden | Destination::Unknown => Fate::Drop,
        }
    }
}

/// The URI scheme a destination starts with.
fn scheme(destination: &str) -> Option<&str> {
    let (scheme, _) = destination.split_once(':')?;
    let mut bytes = scheme.bytes();
    let starts_with_a_letter = bytes.next()?.is_ascii_alphabetic();
    let continues = bytes.all(|byte| byte.is_ascii_alphanumeric() || b"+.-".contains(&byte));
    (starts_with_a_letter && continues).then_some(scheme)
}

/// Text as a chapter may hold it: Unicode NFC, line feeds as line ends, and
/// no other character below U+0020 than the tab.
fn clean(text: &str) -> String {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut out = String::with_capacity(text.len());
    let mut characters = text.nfc().peekable();
    while let Some(character) = characters.next() {
        match character {
            '\r' => {
                if characters.peek() != Some(&'\n') {
                    out.push('\n');
                }
            }
            '\n' | '\t' => out.push(character),
            other if other >= ' ' => out.push(other),
            _ => {}
        }
    }
    out
}

/// Lays the sections of a draft out as chapters: one per section, more for
/// a section that is too long, none for a section without text.
fn chapters(sections: &[Section]) -> Layout<'_> {
    let mut layout = Layout {
        sections: HashMap::new(),
        first_part: Vec::with_capacity(sections.len()),
        parts: Vec::new(),
        marks: HashMap::new(),
    };
    for (index, section) in sections.iter().enumerate() {
        layout
            .sections
            .entry(section.name.as_str())
            .or_insert(index);
        let text = clean(&section.markdown);
        if text.trim().is_empty() {
            layout.first_part.push(None);
            continue;
        }
        layout.first_part.push(Some(layout.parts.len()));
        for text in split::split(text, split::CHAPTER_LIMIT) {
            let path = format!("ch{:04}.md", layout.parts.len() + 1);
            layout.parts.push(Part {
                section: index,
                path,
                text,
            });
        }
    }
    layout
}

/// Chooses the marks to keep: those that a link, or an entry of the
/// contents, leads to. A mark directly in front of a heading stands for the
/// anchor of that heading. Any other gets a name that is free in its
/// chapter: `headings` holds, for every chapter, the anchors `mbook`
/// gave to its headings.
fn choose_marks(
    layout: &Layout<'_>,
    scans: &[Scan],
    headings: &[Vec<Anchor>],
    outline_targets: &[&str],
) -> HashMap<(usize, String), Mark> {
    let mut used: HashSet<(usize, String)> = HashSet::new();
    let mut use_target = |destination: Destination| {
        if let Destination::Internal(Target {
            section,
            mark: Some(mark),
        }) = destination
        {
            used.insert((section, mark));
        }
    };
    for (part, scan) in layout.parts.iter().zip(scans) {
        for link in &scan.links {
            use_target(layout.classify(part.section, link));
        }
    }
    for target in outline_targets {
        // An entry is not written in any section, so it names its own.
        use_target(layout.classify(usize::MAX, target));
    }
    let mut marks = HashMap::new();
    for (index, (part, scan)) in layout.parts.iter().zip(scans).enumerate() {
        let mut taken: HashSet<String> = HashSet::new();
        let anchors = headings.get(index).map(Vec::as_slice).unwrap_or_default();
        for (mark, &end) in scan.marks.iter().zip(&scan.mark_ends) {
            let key = (part.section, mark.clone());
            if !used.contains(&key) || marks.contains_key(&key) {
                continue;
            }
            // The heading the mark stands in front of, with nothing a reader
            // would see in between.
            let heading = anchors.iter().find(|anchor| {
                usize::try_from(anchor.offset).is_ok_and(|start| {
                    start >= end && part.text.get(end..start).is_some_and(only_marks)
                })
            });
            let is_free = |name: &String| {
                !anchors.iter().any(|anchor| anchor.name == *name) && !taken.contains(name)
            };
            let name = match heading {
                Some(heading) => heading.name.clone(),
                None => std::iter::once(mark.clone())
                    .chain((2..u32::MAX).map(|number| format!("{mark}-{number}")))
                    .find(is_free)
                    .unwrap_or_else(|| mark.clone()),
            };
            if heading.is_none() {
                taken.insert(name.clone());
            }
            marks.insert(
                key,
                Mark {
                    part: index,
                    name,
                    at_heading: heading.is_some(),
                },
            );
        }
    }
    marks
}

/// Builds the book of a draft. `source` is the source the draft was read
/// from; a book without an identifier gets one from its bytes.
pub(crate) fn assemble(
    draft: Draft,
    source: &[u8],
    options: &ConvertOptions,
) -> Result<(Book, Vec<Warning>)> {
    let Draft {
        meta,
        sections,
        #[cfg(pictures)]
        pictures,
        #[cfg(pictures)]
        cover,
        #[cfg(outlines)]
        outline,
        #[cfg(feature = "epub")]
        landmarks,
        mut warnings,
    } = draft;
    #[cfg(not(pictures))]
    let _ = options;

    let mut layout = chapters(&sections);
    // The stand-ins are replaced when the metadata is applied.
    let mut book = Book::new(meta::UNTITLED, meta::UNDETERMINED)?;
    let mut headings: Vec<Vec<Anchor>> = Vec::with_capacity(layout.parts.len());
    for part in &layout.parts {
        // `mbook` names the headings; a mark must not take such a name.
        let chapter = book.add_chapter(part.path.as_str(), part.text.as_str())?;
        headings.push(chapter.anchors().to_vec());
    }
    let trees: Vec<_> = layout
        .parts
        .iter()
        .map(|part| rewrite::parse(&part.text))
        .collect();
    let scans: Vec<Scan> = trees
        .iter()
        .map(|tree| tree.as_ref().map(rewrite::scan).unwrap_or_default())
        .collect();
    // What is not written in any section but leads into one: the entries of
    // the contents and the landmarks.
    #[cfg_attr(
        not(outlines),
        expect(unused_mut, reason = "plain formats have no contents")
    )]
    let mut listed: Vec<&str> = Vec::new();
    #[cfg(outlines)]
    listed.extend(outline.iter().map(|entry| entry.target.as_str()));
    #[cfg(feature = "epub")]
    listed.extend(landmarks.iter().map(|landmark| landmark.target.as_str()));
    layout.marks = choose_marks(&layout, &scans, &headings, &listed);

    #[cfg(pictures)]
    let mut store = pictures::Pictures::new(&pictures, options);
    #[cfg(pictures)]
    if let Some(name) = &cover
        && !store.store_cover(&mut book, name)
    {
        let kind = WarningKind::PictureDropped(name.clone());
        warnings.push(Warning::whole_book(kind));
    }

    // Every chapter gets its final text.
    for (index, ((part, tree), scan)) in layout.parts.iter().zip(&trees).zip(&scans).enumerate() {
        let Some(tree) = tree else { continue };
        let section = part.section;
        let link = |destination: &str| layout.fate(section, destination);
        let mark = |name: &str| {
            layout
                .marks
                .get(&(section, name.to_owned()))
                .filter(|mark| mark.part == index && !mark.at_heading)
                .map(|mark| mark.name.clone())
        };
        #[cfg(pictures)]
        let mut picture = |destination: &str| {
            let name = percent_decode_str(destination).decode_utf8().ok()?;
            store.path(&mut book, &name)
        };
        #[cfg(not(pictures))]
        let mut picture = |_: &str| None;
        let rules = Rules {
            location: sections.get(section).map_or("", |section| &section.name),
            link: &link,
            picture: &mut picture,
            mark: &mark,
            warnings: &mut warnings,
        };
        let text = rewrite::rewrite(&part.text, tree, scan, rules);
        if let Some(chapter) = book.chapter_mut(&part.path) {
            chapter.set_text(text)?;
        }
    }

    let heading = scans.iter().find_map(|scan| scan.heading.clone());
    meta::apply(&mut book, meta, heading, source, &mut warnings);
    #[cfg(outlines)]
    if let Some(toc) = contents(&book, &layout, &outline, &mut warnings) {
        book.set_toc(toc);
    }
    #[cfg(feature = "epub")]
    book.set_landmarks(placed(&layout, landmarks, &mut warnings))?;

    if book.chapters().is_empty() && book.pictures().is_empty() {
        return Err(Error::Empty);
    }
    // A failure here is a defect of this crate, not of the source.
    book.check().map_err(mbook::Error::from)?;
    Ok((book, warnings))
}

/// The landmarks of the source as those of the book.
///
/// A landmark leads to a chapter or to a mark, which is an extra anchor of
/// its chapter. One that leads nowhere is left out with a warning, and so is
/// a second `start` or a kind the format has no form for.
#[cfg(feature = "epub")]
fn placed(
    layout: &Layout<'_>,
    landmarks: Vec<DraftLandmark>,
    warnings: &mut Vec<Warning>,
) -> Vec<Landmark> {
    let mut kept: Vec<Landmark> = Vec::with_capacity(landmarks.len());
    for landmark in landmarks {
        let place = match layout.classify(usize::MAX, &landmark.target) {
            Destination::Internal(target) => match &target.mark {
                None => layout
                    .first_part
                    .get(target.section)
                    .copied()
                    .flatten()
                    .map(|part| (part, None)),
                Some(name) => layout
                    .marks
                    .get(&(target.section, name.clone()))
                    .map(|mark| (mark.part, Some(mark.name.clone()))),
            },
            _ => None,
        };
        let chapter = place
            .as_ref()
            .and_then(|(part, _)| layout.parts.get(*part))
            .map(|part| part.path.clone());
        let again = landmark.kind == "start" && kept.iter().any(|kept| kept.kind == "start");
        let is_kind = !landmark.kind.is_empty()
            && landmark.kind.split('-').all(|part| {
                !part.is_empty()
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            });
        match (chapter, place) {
            (Some(chapter), Some((_, anchor))) if is_kind && !again => kept.push(Landmark {
                kind: landmark.kind,
                title: landmark.title.trim().to_owned(),
                chapter,
                anchor,
            }),
            _ => warnings.push(Warning::whole_book(WarningKind::LinkDropped(
                landmark.target,
            ))),
        }
    }
    kept
}

/// The table of contents of the source as one of the book.
///
/// An entry leads to a chapter or to a heading. An entry to another marked
/// place leads to the heading right after its mark, otherwise to the nearest
/// heading above it or to the start of its chapter, with a warning. `None`
/// when the source has no contents that could be kept.
#[cfg(outlines)]
fn contents(
    book: &Book,
    layout: &Layout<'_>,
    outline: &[OutlineEntry],
    warnings: &mut Vec<Warning>,
) -> Option<Toc> {
    let mut items: Vec<TocItem> = Vec::with_capacity(outline.len());
    let mut previous_level = 0u8;
    let mut previous_place = (0usize, 0u32);
    for entry in outline {
        let mut warn = |kind: fn(String) -> WarningKind| {
            warnings.push(Warning::whole_book(kind(entry.target.clone())));
        };
        let Destination::Internal(target) = layout.classify(usize::MAX, &entry.target) else {
            warn(WarningKind::LinkDropped);
            continue;
        };
        let title = entry.title.trim();
        let Some((part, heading)) = heading_of(book, layout, &target) else {
            warn(WarningKind::LinkDropped);
            continue;
        };
        let place = (part, heading.as_ref().map_or(0, |(offset, _, _)| *offset));
        if title.is_empty() || place < previous_place {
            warn(WarningKind::LinkDropped);
            continue;
        }
        if heading.as_ref().is_some_and(|(_, _, exact)| !exact)
            || (heading.is_none()
                && target.mark.is_some()
                && !at_chapter_start(book, layout, &target))
        {
            warn(WarningKind::LinkRetargeted);
        }
        let level = entry.level.clamp(1, previous_level.saturating_add(1));
        items.push(TocItem {
            level,
            title: title.to_owned(),
            chapter: layout.parts[part].path.clone(),
            anchor: heading.map(|(_, name, _)| name),
        });
        previous_level = level;
        previous_place = place;
    }
    (!items.is_empty()).then_some(Toc::Explicit(items))
}

/// The offset of a mark in its chapter.
#[cfg(outlines)]
fn mark_offset(book: &Book, layout: &Layout<'_>, target: &Target) -> Option<(usize, u32)> {
    let mark = layout.marks.get(&(target.section, target.mark.clone()?))?;
    let chapter = book.chapter(&layout.parts.get(mark.part)?.path)?;
    let anchor = chapter
        .extra_anchors()
        .iter()
        .find(|anchor| anchor.name == mark.name)?;
    Some((mark.part, anchor.offset))
}

/// Tells whether nothing but marks stands between the start of a chapter
/// and the mark an entry leads to.
#[cfg(outlines)]
fn at_chapter_start(book: &Book, layout: &Layout<'_>, target: &Target) -> bool {
    mark_offset(book, layout, target).is_some_and(|(part, offset)| {
        book.chapter(&layout.parts[part].path)
            .and_then(|chapter| chapter.text().get(..usize::try_from(offset).ok()?))
            .is_some_and(only_marks)
    })
}

/// Tells whether a piece of a chapter holds nothing a reader would see.
fn only_marks(text: &str) -> bool {
    text.lines()
        .all(|line| line.trim().is_empty() || (line.starts_with('[') && line.contains("]: #")))
}

/// The chapter an entry of the contents leads to, and the heading in it:
/// its offset, its anchor, and whether it is exactly what the entry names.
/// No heading means the start of the chapter. `None` when the target was
/// not converted.
#[cfg(outlines)]
#[expect(
    clippy::type_complexity,
    reason = "a private pair of a chapter and an optional heading"
)]
fn heading_of(
    book: &Book,
    layout: &Layout<'_>,
    target: &Target,
) -> Option<(usize, Option<(u32, String, bool)>)> {
    let Some(name) = &target.mark else {
        return Some(((*layout.first_part.get(target.section)?)?, None));
    };
    let mark = layout.marks.get(&(target.section, name.clone()))?;
    if mark.at_heading {
        let chapter = book.chapter(&layout.parts.get(mark.part)?.path)?;
        let heading = chapter
            .anchors()
            .iter()
            .find(|anchor| anchor.name == mark.name)?;
        return Some((
            mark.part,
            Some((heading.offset, heading.name.clone(), true)),
        ));
    }
    // Any other mark names a place below a heading, or above all of them.
    let (part, offset) = mark_offset(book, layout, target)?;
    let chapter = book.chapter(&layout.parts.get(part)?.path)?;
    let above = chapter
        .anchors()
        .iter()
        .rev()
        .find(|anchor| anchor.offset < offset);
    let heading = above.map(|anchor| (anchor.offset, anchor.name.clone(), false));
    Some((part, heading))
}

#[cfg(test)]
mod tests;
