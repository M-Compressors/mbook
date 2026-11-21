//! What the main thread knows of the book being edited.

use mbook::{Book, Chapter, Problems};

/// The state of the book after a command: its metadata, the paths and sizes of
/// its parts and the problems last reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Outline {
    /// The metadata the Details form shows.
    pub(crate) meta: MetaOutline,
    /// The chapters in reading order.
    pub(crate) chapters: Vec<ChapterOutline>,
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
    /// The text of its first heading that has an anchor, without the `#`
    /// marks; `None` when it has none.
    pub(crate) heading: Option<String>,
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
            chapters: book
                .chapters()
                .iter()
                .map(|chapter| ChapterOutline {
                    path: chapter.path().to_owned(),
                    len: chapter.text().len(),
                    heading: first_heading(chapter),
                })
                .collect(),
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

/// The text of the first heading that has an anchor.
fn first_heading(chapter: &Chapter) -> Option<String> {
    let offset = usize::try_from(chapter.anchors().first()?.offset).ok()?;
    let line = chapter.text().get(offset..)?.lines().next()?.trim();
    let title = if line.starts_with('#') {
        let line = line.trim_start_matches('#').trim();
        // A closing sequence of `#` marks needs a space before it.
        let open = line.trim_end_matches('#');
        if open.is_empty() || open.ends_with(' ') {
            open.trim()
        } else {
            line
        }
    } else {
        line
    };
    (!title.is_empty()).then(|| title.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{Outline, ProblemOutline, first_heading, sentence};
    use mbook::Book;

    type Result<T = ()> = std::result::Result<T, mbook::Error>;

    fn heading_of(text: &str) -> Result<Option<String>> {
        let mut book = Book::new("T", "en")?;
        let chapter = book.add_chapter("a.md", text)?;
        Ok(first_heading(chapter))
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
    fn the_outline_of_a_new_book_holds_its_metadata_and_nothing_else() -> Result {
        let book = Book::new("The Example", "en")?;
        let outline = Outline::of(&book, &[], true);
        assert_eq!(outline.meta.title, "The Example");
        assert_eq!(outline.meta.language, "en");
        assert!(outline.meta.authors.is_empty() && outline.meta.series.is_none());
        assert!(outline.chapters.is_empty() && outline.pictures.is_empty());
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
