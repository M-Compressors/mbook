//! What the main thread knows of the book being edited.

use super::contents::{Places, faults};
use markdown::ParseOptions;
use markdown::mdast::Node;
use mbook::{Book, Chapter, Landmark, Problems, Toc};

/// The state of the book after a command: its metadata, the paths and sizes of
/// its parts and the problems last reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Outline {
    /// The metadata the Details form shows.
    pub(crate) meta: MetaOutline,
    /// The chapters in reading order.
    pub(crate) chapters: Vec<ChapterOutline>,
    /// The contents a reader of the book is shown.
    pub(crate) contents: ContentsOutline,
    /// The landmarks in their stored order.
    pub(crate) landmarks: Vec<Landmark>,
    /// The pictures in the order they were added.
    pub(crate) pictures: Vec<PictureOutline>,
    /// The problems of the last `Check` or `Save`; empty after any change.
    pub(crate) problems: Vec<ProblemOutline>,
    /// A change was made since the book was opened or last saved.
    pub(crate) unsaved: bool,
}

/// The metadata the Details form shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MetaOutline {
    /// The title.
    pub(crate) title: String,
    /// The authors, in the order they are credited.
    pub(crate) authors: Vec<String>,
    /// The main language.
    pub(crate) language: String,
    /// The series, when the book belongs to one.
    pub(crate) series: Option<SeriesOutline>,
    /// The publisher.
    pub(crate) publisher: Option<String>,
    /// The publication date.
    pub(crate) date: Option<String>,
    /// The annotation.
    pub(crate) annotation: Option<String>,
    /// The path of the cover picture.
    pub(crate) cover: Option<String>,
}

/// The series a book belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SeriesOutline {
    /// The name of the series.
    pub(crate) name: String,
    /// The position in the series.
    pub(crate) index: Option<String>,
}

/// One chapter of the book.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChapterOutline {
    /// The path of the chapter.
    pub(crate) path: String,
    /// The length of its text in bytes.
    pub(crate) len: usize,
    /// The text of its first heading that has an anchor, without its markup;
    /// `None` when it has none.
    pub(crate) heading: Option<String>,
    /// The places of the chapter that a link or a landmark can name, in the
    /// order of the text: its headings and the lines it gives a name to.
    pub(crate) places: Vec<PlaceOutline>,
}

/// A place inside a chapter that has a name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlaceOutline {
    /// The name: what a link writes after `#`.
    pub(crate) anchor: String,
    /// The text of the heading without its markup; empty for a named line.
    pub(crate) title: String,
    /// The level of the heading, 1 to 6; `None` for a line that an anchor
    /// definition names.
    pub(crate) level: Option<u8>,
}

/// The contents of the book as a reader is shown them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ContentsOutline {
    /// The contents follow the headings of the chapters. Otherwise the book
    /// keeps a list of its own, which an edit of a heading does not change.
    pub(crate) automatic: bool,
    /// The entries in reading order.
    pub(crate) entries: Vec<ContentsEntry>,
}

/// One entry of the contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ContentsEntry {
    /// The depth in the tree, starting at 1.
    pub(crate) level: u8,
    /// The title.
    pub(crate) title: String,
    /// The path of the chapter the entry leads to.
    pub(crate) chapter: String,
    /// The name of the place inside that chapter; `None` is its start.
    pub(crate) anchor: Option<String>,
    /// What keeps the book from being saved with this entry; `None` for an
    /// entry that leads somewhere and is in the order of the book.
    pub(crate) fault: Option<Fault>,
}

/// What is wrong with where an entry of a stored contents list leads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fault {
    /// The chapter is gone, or it has no heading with the name.
    Nowhere,
    /// The entry leads to a place before the place of the entry above it.
    OutOfOrder,
}

/// One picture of the book.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PictureOutline {
    /// The path of the picture.
    pub(crate) path: String,
    /// The width in pixels.
    pub(crate) width: u32,
    /// The height in pixels.
    pub(crate) height: u32,
    /// The length of the AVIF file in bytes.
    pub(crate) len: usize,
}

/// One problem that keeps the book from being saved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProblemOutline {
    /// The chapter the problem is in, when it is in one.
    pub(crate) path: Option<String>,
    /// Where in that chapter, in bytes from the start of its text.
    pub(crate) offset: Option<u32>,
    /// What is wrong: the `Display` of the `mbook` problem's kind as a
    /// sentence. The place is in `path` and `offset`.
    pub(crate) message: String,
}

impl Outline {
    /// Describes a book with the problems last found in it.
    pub(crate) fn of(book: &Book, problems: &[ProblemOutline], unsaved: bool) -> Self {
        let meta = book.meta();
        let chapters: Vec<ChapterOutline> = book.chapters().iter().map(chapter_outline).collect();
        let contents = match book.toc() {
            Toc::FromHeadings { max_level } => ContentsOutline {
                automatic: true,
                entries: contents_from_headings(&chapters, *max_level),
            },
            Toc::Explicit(items) => ContentsOutline {
                automatic: false,
                entries: items
                    .iter()
                    .zip(faults(items, &Places::of(book)))
                    .map(|(item, fault)| ContentsEntry {
                        level: item.level,
                        title: item.title.clone(),
                        chapter: item.chapter.clone(),
                        anchor: item.anchor.clone(),
                        fault,
                    })
                    .collect(),
            },
        };
        Self {
            meta: MetaOutline {
                title: meta.title().to_owned(),
                authors: meta.authors().map(str::to_owned).collect(),
                language: meta.language().to_owned(),
                series: meta.series().map(|series| SeriesOutline {
                    name: series.name.to_owned(),
                    index: series.index.map(str::to_owned),
                }),
                publisher: meta.publisher().map(str::to_owned),
                date: meta.date().map(str::to_owned),
                annotation: meta.annotation().map(str::to_owned),
                cover: meta.cover().map(str::to_owned),
            },
            chapters,
            contents,
            landmarks: book.landmarks().to_vec(),
            pictures: book
                .pictures()
                .iter()
                .map(|picture| PictureOutline {
                    path: picture.path().to_owned(),
                    width: picture.size().width,
                    height: picture.size().height,
                    len: picture.data().len(),
                })
                .collect(),
            problems: problems.to_vec(),
            unsaved,
        }
    }
}

impl ProblemOutline {
    /// Lists the problems of a failed check.
    pub(crate) fn all(problems: &Problems) -> Vec<Self> {
        problems
            .iter()
            .map(|problem| Self {
                path: problem.path.clone(),
                offset: problem.offset,
                message: sentence(&problem.kind.to_string()),
            })
            .collect()
    }
}

/// Starts a message with a capital letter and ends it with a full stop.
fn sentence(message: &str) -> String {
    let mut chars = message.chars();
    let mut sentence: String = chars
        .next()
        .map_or_default(|first| first.to_uppercase().chain(chars).collect());
    if !sentence.is_empty() && !sentence.ends_with('.') {
        sentence.push('.');
    }
    sentence
}

/// Describes one chapter: its size and the places in it that have a name.
fn chapter_outline(chapter: &Chapter) -> ChapterOutline {
    let mut places: Vec<(u32, PlaceOutline)> = chapter
        .anchors()
        .iter()
        .filter_map(|anchor| {
            let (level, title) = heading_at(chapter.text(), anchor.offset)?;
            Some((
                anchor.offset,
                PlaceOutline {
                    anchor: anchor.name.clone(),
                    title,
                    level: Some(level),
                },
            ))
        })
        .chain(chapter.extra_anchors().iter().map(|anchor| {
            (
                anchor.offset,
                PlaceOutline {
                    anchor: anchor.name.clone(),
                    title: String::new(),
                    level: None,
                },
            )
        }))
        .collect();
    places.sort_by_key(|(offset, _)| *offset);
    let places: Vec<PlaceOutline> = places.into_iter().map(|(_, place)| place).collect();
    ChapterOutline {
        path: chapter.path().to_owned(),
        len: chapter.text().len(),
        heading: places
            .iter()
            .find(|place| place.level.is_some() && !place.title.is_empty())
            .map(|place| place.title.clone()),
        places,
    }
}

/// The level and the text of the heading that starts on the line at `offset`.
///
/// The parser is the one `mbook` reads a chapter with, so that the title is
/// the one the book writes into its contents. Only the block of the heading
/// is parsed, which keeps a book of many chapters cheap to describe after
/// every change. A block with a bracket may refer to a definition elsewhere
/// in the chapter, so for it the whole chapter is parsed.
fn heading_at(text: &str, offset: u32) -> Option<(u8, String)> {
    let at = usize::try_from(offset).ok()?;
    let rest = text.get(at..)?;
    let block = rest.find("\n\n").map_or(rest, |end| &rest[..end]);
    let (source, line) = if block.contains('[') {
        (text, at)
    } else {
        (block, 0)
    };
    let root = markdown::to_mdast(source, &ParseOptions::gfm()).ok()?;
    let heading = heading_on_line(&root, source, line)?;
    let mut title = String::new();
    heading
        .children
        .iter()
        .for_each(|child| plain_text(child, &mut title));
    Some((heading.depth, title.trim().to_owned()))
}

/// The heading under a node that starts on the line beginning at `line`.
fn heading_on_line<'a>(
    node: &'a Node,
    text: &str,
    line: usize,
) -> Option<&'a markdown::mdast::Heading> {
    if let Node::Heading(heading) = node {
        let start = heading.position.as_ref()?.start.offset;
        let line_start = text
            .get(..start)?
            .rfind('\n')
            .map_or(0, |newline| newline + 1);
        return (line_start == line).then_some(heading);
    }
    node.children()?
        .iter()
        .find_map(|child| heading_on_line(child, text, line))
}

/// Appends the text of a node without its markup, as `mbook` does for the
/// title of a heading.
fn plain_text(node: &Node, out: &mut String) {
    match node {
        Node::Text(text) => out.push_str(&text.value),
        Node::InlineCode(code) => out.push_str(&code.value),
        Node::Image(image) => out.push_str(&image.alt),
        Node::ImageReference(image) => out.push_str(&image.alt),
        Node::Break(_) => out.push(' '),
        other => {
            for child in other.children().map_or_default(Vec::as_slice) {
                plain_text(child, out);
            }
        }
    }
}

/// The contents a book with automatic contents is written with: every
/// heading down to `max_level` that has a title, at its depth among the
/// headings that enclose it (RFC 0001, "Contents").
fn contents_from_headings(chapters: &[ChapterOutline], max_level: u8) -> Vec<ContentsEntry> {
    let mut entries = Vec::new();
    let mut enclosing: Vec<u8> = Vec::new();
    for chapter in chapters {
        let headings = chapter.places.iter().filter_map(|place| {
            let level = place.level.filter(|level| *level <= max_level)?;
            (!place.title.is_empty()).then_some((level, place))
        });
        for (level, place) in headings {
            while enclosing.last().is_some_and(|depth| *depth >= level) {
                enclosing.pop();
            }
            enclosing.push(level);
            entries.push(ContentsEntry {
                level: u8::try_from(enclosing.len()).unwrap_or(u8::MAX),
                title: place.title.clone(),
                chapter: chapter.path.clone(),
                anchor: Some(place.anchor.clone()),
                fault: None,
            });
        }
    }
    entries
}

#[cfg(test)]
mod tests {
    use super::{ContentsEntry, Fault, Outline, ProblemOutline, chapter_outline, sentence};
    use mbook::Book;
    use mbook::{Toc, TocItem};

    type Result<T = ()> = std::result::Result<T, mbook::Error>;

    fn heading_of(text: &str) -> Result<Option<String>> {
        let mut book = Book::new("T", "en")?;
        let chapter = book.add_chapter("a.md", text)?;
        Ok(chapter_outline(chapter).heading)
    }

    #[test]
    fn a_sentence_gets_a_capital_and_a_full_stop() {
        assert_eq!(
            sentence("raw HTML is not allowed"),
            "Raw HTML is not allowed."
        );
    }

    #[test]
    fn a_sentence_that_ends_with_a_full_stop_keeps_one() {
        assert_eq!(sentence("done."), "Done.");
    }

    #[test]
    fn an_empty_message_stays_empty() {
        assert_eq!(sentence(""), "");
    }

    #[test]
    fn the_heading_of_a_chapter_is_the_text_after_the_marks() -> Result {
        assert_eq!(
            heading_of("intro\n\n## The station\n")?.as_deref(),
            Some("The station")
        );
        Ok(())
    }

    #[test]
    fn a_closing_sequence_is_not_part_of_the_heading() -> Result {
        assert_eq!(heading_of("# One ##\n")?.as_deref(), Some("One"));
        Ok(())
    }

    #[test]
    fn a_hash_inside_the_heading_stays() -> Result {
        assert_eq!(heading_of("# C#\n")?.as_deref(), Some("C#"));
        Ok(())
    }

    #[test]
    fn an_underlined_heading_is_its_line() -> Result {
        assert_eq!(heading_of("Title\n=====\n")?.as_deref(), Some("Title"));
        Ok(())
    }

    #[test]
    fn a_chapter_without_a_heading_has_none() -> Result {
        assert_eq!(heading_of("just text\n")?, None);
        Ok(())
    }

    #[test]
    fn an_empty_chapter_has_no_heading() -> Result {
        assert_eq!(heading_of("")?, None);
        Ok(())
    }

    #[test]
    fn the_heading_of_a_chapter_drops_its_markup() -> Result {
        assert_eq!(
            heading_of("# The *late* `harvest` \\[1\\]\n")?.as_deref(),
            Some("The late harvest [1]")
        );
        Ok(())
    }

    #[test]
    fn a_picture_in_a_heading_gives_its_description_and_a_break_a_space() -> Result {
        assert_eq!(
            heading_of("# The ![old gate](gate.avif) at dawn\n")?.as_deref(),
            Some("The old gate at dawn")
        );
        assert_eq!(
            heading_of("# The ![old gate][g]\n\n[g]: gate.avif\n")?.as_deref(),
            Some("The old gate")
        );
        assert_eq!(heading_of("One\\\nTwo\n===\n")?.as_deref(), Some("One Two"));
        Ok(())
    }

    #[test]
    fn a_heading_inside_a_quote_is_found() -> Result {
        assert_eq!(heading_of("> ## Quoted\n")?.as_deref(), Some("Quoted"));
        Ok(())
    }

    fn places_of(text: &str) -> Result<Vec<(String, String, Option<u8>)>> {
        let mut book = Book::new("T", "en")?;
        let chapter = book.add_chapter("a.md", text)?;
        Ok(chapter_outline(chapter)
            .places
            .into_iter()
            .map(|place| (place.anchor, place.title, place.level))
            .collect())
    }

    #[test]
    fn the_places_of_a_chapter_are_its_headings_and_named_lines_in_text_order() -> Result {
        let places = places_of("# One\n\n[remark]: #remark\nA remark.\n\n## Two words\n")?;
        assert_eq!(
            places,
            [
                ("one".to_owned(), "One".to_owned(), Some(1)),
                ("remark".to_owned(), String::new(), None),
                ("two-words".to_owned(), "Two words".to_owned(), Some(2)),
            ]
        );
        Ok(())
    }

    #[test]
    fn a_chapter_without_names_has_no_places() -> Result {
        assert_eq!(places_of("just text\n")?, []);
        Ok(())
    }

    fn entry(level: u8, title: &str, chapter: &str, anchor: Option<&str>) -> ContentsEntry {
        ContentsEntry {
            level,
            title: title.to_owned(),
            chapter: chapter.to_owned(),
            anchor: anchor.map(str::to_owned),
            fault: None,
        }
    }

    #[test]
    fn automatic_contents_list_the_headings_at_their_depth_among_each_other() -> Result {
        let mut book = Book::new("T", "en")?;
        book.add_chapter("a.md", "# A\n\n### Deep\n\n## Mid\n")?;
        book.add_chapter("b.md", "text\n\n# B\n")?;
        let contents = Outline::of(&book, &[], false).contents;
        assert!(contents.automatic);
        assert_eq!(
            contents.entries,
            [
                entry(1, "A", "a.md", Some("a")),
                entry(2, "Deep", "a.md", Some("deep")),
                entry(2, "Mid", "a.md", Some("mid")),
                entry(1, "B", "b.md", Some("b")),
            ]
        );
        Ok(())
    }

    #[test]
    fn automatic_contents_stop_at_the_deepest_level_asked_for() -> Result {
        let mut book = Book::new("T", "en")?;
        book.add_chapter("a.md", "# A\n\n## Below\n")?;
        book.set_toc(Toc::FromHeadings { max_level: 1 });
        let contents = Outline::of(&book, &[], false).contents;
        assert_eq!(contents.entries, [entry(1, "A", "a.md", Some("a"))]);
        Ok(())
    }

    #[test]
    fn stored_contents_are_listed_as_they_are() -> Result {
        let mut book = Book::new("T", "en")?;
        book.add_chapter("a.md", "# A\n\n## Below\n")?;
        book.set_toc(Toc::Explicit(vec![
            TocItem::new(1, "Opening", "a.md"),
            TocItem::new(2, "Further", "a.md#below"),
        ]));
        let contents = Outline::of(&book, &[], false).contents;
        assert!(!contents.automatic);
        assert_eq!(
            contents.entries,
            [
                entry(1, "Opening", "a.md", None),
                entry(2, "Further", "a.md", Some("below")),
            ]
        );
        Ok(())
    }

    #[test]
    fn a_stored_entry_that_cannot_be_saved_says_what_is_wrong_with_it() -> Result {
        let mut book = Book::new("T", "en")?;
        book.add_chapter("a.md", "# A\n")?;
        book.add_chapter("b.md", "# B\n")?;
        book.set_toc(Toc::Explicit(vec![
            TocItem::new(1, "B", "b.md"),
            TocItem::new(1, "A", "a.md"),
            TocItem::new(1, "Lost", "a.md#gone"),
        ]));
        let faults: Vec<Option<Fault>> = Outline::of(&book, &[], false)
            .contents
            .entries
            .iter()
            .map(|entry| entry.fault)
            .collect();
        assert_eq!(
            faults,
            [None, Some(Fault::OutOfOrder), Some(Fault::Nowhere)]
        );
        Ok(())
    }

    #[test]
    fn the_outline_of_a_new_book_holds_its_metadata_and_nothing_else() -> Result {
        let book = Book::new("The Example", "en")?;
        let outline = Outline::of(&book, &[], true);
        assert_eq!(outline.meta.title, "The Example");
        assert_eq!(outline.meta.language, "en");
        assert!(outline.meta.authors.is_empty() && outline.meta.series.is_none());
        assert!(outline.chapters.is_empty() && outline.pictures.is_empty());
        assert!(outline.contents.automatic && outline.contents.entries.is_empty());
        assert!(outline.unsaved);
        Ok(())
    }

    #[test]
    fn the_problems_of_a_book_are_described_with_their_place() -> Result {
        let mut book = Book::new("T", "en")?;
        book.add_chapter("a.md", "# A\n\n[x](missing.md)\n")?;
        let problems = book.check().expect_err("a dangling link");
        let listed = ProblemOutline::all(&problems);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path.as_deref(), Some("a.md"));
        assert_eq!(listed[0].offset, Some(5));
        assert_eq!(
            listed[0].message,
            "The link to \"missing.md\" leads nowhere."
        );
        Ok(())
    }
}
