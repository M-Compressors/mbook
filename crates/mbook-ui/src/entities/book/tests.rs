//! Tests of opening a book and reading from it, on books written to a
//! temporary folder.

use std::error::Error as _;
use std::path::PathBuf;

use mbook::Book;
use tempfile::TempDir;

use super::chapter::{PictureRef, PreparedChapter, Target};
use super::error::BookError;
use super::snapshot::BookSnapshot;
use super::source::{BookSource, open};
use crate::entities::figure::FigureKind;
use crate::shared::model::Place;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const CH01: &str = "# One

See [two](ch02.md#second), [the web](https://example.org), [the note](#note) and [the top](#one).

![A figure](img/fig.avif)

[note]: #note
A named paragraph.
";

const CH02: &str = "# Two

Intro.

## Second

More text.

- a
- b

```
code
```

| a | b |
| - | - |
| 1 | 2 |
";

fn fixture(name: &str) -> TestResult<Vec<u8>> {
    let path = format!(
        "{}/../mbook/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    Ok(std::fs::read(path)?)
}

fn offset_of(text: &str, needle: &str) -> TestResult<u32> {
    let at = text.find(needle).ok_or("needle not in text")?;
    Ok(u32::try_from(at)?)
}

/// A book of two chapters, a figure and a cover, saved in `dir`.
fn write_sample(dir: &TempDir) -> TestResult<PathBuf> {
    let mut book = Book::new("The Reader", "en")?;
    book.meta_mut().add_author("A. Writer")?;
    book.meta_mut().add_author("B. Author")?;
    book.set_cover("img/cover.avif", fixture("plain.avif")?)?;
    book.add_chapter("ch01.md", CH01)?;
    book.add_chapter("ch02.md", CH02)?;
    book.add_picture("img/fig.avif", fixture("rotated.avif")?)?;
    let path = dir.path().join("sample.mbook");
    book.save(&path)?;
    Ok(path)
}

fn open_sample() -> TestResult<(TempDir, BookSnapshot, BookSource)> {
    let dir = tempfile::tempdir()?;
    let path = write_sample(&dir)?;
    let (snapshot, source) = open(&path)?;
    Ok((dir, snapshot, source))
}

#[test]
fn the_source_can_be_shared_between_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<BookSource>();
    assert_send_sync::<BookSnapshot>();
    assert_send_sync::<PreparedChapter>();
}

#[test]
fn the_snapshot_names_the_book() -> TestResult {
    let (_dir, snapshot, _source) = open_sample()?;
    assert_eq!(snapshot.title, "The Reader");
    assert_eq!(snapshot.language, "en");
    assert_eq!(snapshot.authors, ["A. Writer", "B. Author"]);
    Ok(())
}

#[test]
fn the_snapshot_lists_the_chapters_in_reading_order() -> TestResult {
    let (_dir, snapshot, _source) = open_sample()?;
    let listed: Vec<(&str, u64)> = snapshot
        .chapters
        .iter()
        .map(|chapter| (chapter.path.as_str(), chapter.decoded_len))
        .collect();
    assert_eq!(
        listed,
        [
            ("ch01.md", u64::try_from(CH01.len())?),
            ("ch02.md", u64::try_from(CH02.len())?)
        ]
    );
    Ok(())
}

#[test]
fn the_contents_lead_to_the_chapter_index_and_offset_of_each_heading() -> TestResult {
    let (_dir, snapshot, _source) = open_sample()?;
    let listed: Vec<(u8, &str, usize, u32)> = snapshot
        .contents
        .iter()
        .map(|entry| {
            (
                entry.level,
                entry.title.as_str(),
                entry.chapter,
                entry.offset,
            )
        })
        .collect();
    assert_eq!(
        listed,
        [
            (1, "One", 0, 0),
            (1, "Two", 1, 0),
            (2, "Second", 1, offset_of(CH02, "## Second")?),
        ]
    );
    Ok(())
}

#[test]
fn the_snapshot_has_the_size_of_the_cover() -> TestResult {
    let (_dir, snapshot, _source) = open_sample()?;
    assert_eq!(snapshot.cover, Some((24, 16)));
    Ok(())
}

#[test]
fn a_book_without_a_cover_has_no_cover_size_and_no_cover_bytes() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut book = Book::new("Bare", "en")?;
    book.add_chapter("a.md", "# A\n")?;
    let path = dir.path().join("bare.mbook");
    book.save(&path)?;
    let (snapshot, source) = open(&path)?;
    assert_eq!(snapshot.cover, None);
    assert_eq!(source.read_cover()?, None);
    Ok(())
}

#[test]
fn a_book_without_chapters_opens_empty() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("empty.mbook");
    Book::new("Empty", "en")?.save(&path)?;
    let (snapshot, _source) = open(&path)?;
    assert_eq!(snapshot.chapters, []);
    assert_eq!(snapshot.contents, []);
    assert_eq!(snapshot.progress(0, 0), 0);
    assert_eq!(
        snapshot.locate(&Place {
            chapter: "a.md".to_owned(),
            offset: 0
        }),
        None
    );
    Ok(())
}

#[test]
fn a_file_that_does_not_exist_is_an_open_error_with_an_io_source() -> TestResult {
    let dir = tempfile::tempdir()?;
    let error = open(&dir.path().join("missing.mbook")).unwrap_err();
    assert!(matches!(error, BookError::Open(mbook::Error::Io(_))));
    assert_eq!(error.to_string(), "the book could not be opened");
    assert_eq!(
        error.source().map(ToString::to_string).as_deref(),
        Some("I/O failed")
    );
    Ok(())
}

#[test]
fn a_file_that_is_not_a_book_is_an_open_error_with_its_cause() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("text.mbook");
    std::fs::write(
        &path,
        "just some text, long enough to have a signature to compare",
    )?;
    let error = open(&path).unwrap_err();
    assert!(matches!(error, BookError::Open(mbook::Error::NotMbook)));
    assert_eq!(
        error.source().map(ToString::to_string).as_deref(),
        Some("not an mbook file")
    );
    Ok(())
}

#[test]
fn a_chapter_is_prepared_with_its_text() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    let chapter = source.prepare_chapter(0)?;
    assert_eq!(chapter.index, 0);
    assert_eq!(chapter.text, CH01);
    Ok(())
}

#[test]
fn a_link_to_a_heading_of_another_chapter_is_a_place() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    let chapter = source.prepare_chapter(0)?;
    assert_eq!(
        chapter.links.get("ch02.md#second"),
        Some(&Target::Place {
            chapter: 1,
            offset: offset_of(CH02, "## Second")?
        })
    );
    Ok(())
}

#[test]
fn a_link_to_a_heading_of_the_same_chapter_is_a_place() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    let chapter = source.prepare_chapter(0)?;
    assert_eq!(
        chapter.links.get("#one"),
        Some(&Target::Place {
            chapter: 0,
            offset: 0
        })
    );
    Ok(())
}

#[test]
fn a_link_to_the_web_is_external_and_keeps_its_address() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    let chapter = source.prepare_chapter(0)?;
    assert_eq!(
        chapter.links.get("https://example.org"),
        Some(&Target::External("https://example.org".to_owned()))
    );
    Ok(())
}

#[test]
fn a_destination_that_resolved_to_a_picture_is_a_picture_with_its_size() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    let reader = source.lock()?;
    let id = reader.find_picture("img/fig.avif").ok_or("no picture")?.id;
    let target = source.target(&reader, "img/fig.avif", Ok(mbook::Destination::Picture(id)));
    let Target::Picture(picture) = target else {
        return Err("not a picture".into());
    };
    assert_eq!((picture.width, picture.height), (16, 24));
    Ok(())
}

#[test]
fn a_destination_that_failed_to_resolve_leads_nowhere() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    let reader = source.lock()?;
    let target = source.target(&reader, "x.md", Err(mbook::ResolveError::NotFound));
    assert_eq!(target, Target::Unresolved);
    Ok(())
}

#[test]
fn a_link_to_an_extra_anchor_is_deferred() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    let chapter = source.prepare_chapter(0)?;
    assert_eq!(chapter.links.get("#note"), Some(&Target::Deferred));
    Ok(())
}

#[test]
fn the_links_of_a_chapter_are_only_its_links() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    let chapter = source.prepare_chapter(0)?;
    let mut keys: Vec<&str> = chapter.links.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["#note", "#one", "ch02.md#second", "https://example.org"]
    );
    Ok(())
}

#[test]
fn a_picture_of_the_text_is_resolved_with_its_size() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    let chapter = source.prepare_chapter(0)?;
    assert_eq!(chapter.pictures.len(), 1);
    let picture = chapter.pictures.get("img/fig.avif").ok_or("no picture")?;
    assert_eq!((picture.width, picture.height), (16, 24));
    Ok(())
}

#[test]
fn a_chapter_without_links_has_none() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    let chapter = source.prepare_chapter(1)?;
    assert!(chapter.links.is_empty());
    assert!(chapter.pictures.is_empty());
    Ok(())
}

#[test]
fn the_blocks_of_a_chapter_start_at_its_headings_paragraphs_list_code_and_table() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    let chapter = source.prepare_chapter(1)?;
    let expected: Vec<u32> = [
        "# Two",
        "Intro.",
        "## Second",
        "More text.",
        "- a",
        "```",
        "| a | b |",
    ]
    .iter()
    .map(|needle| offset_of(CH02, needle))
    .collect::<TestResult<_>>()?;
    assert_eq!(chapter.blocks, expected);
    Ok(())
}

#[test]
fn the_block_at_an_offset_of_a_prepared_chapter_is_found_before_at_and_after_starts() -> TestResult
{
    let (_dir, _snapshot, source) = open_sample()?;
    let chapter = source.prepare_chapter(1)?;
    let second = offset_of(CH02, "## Second")?;
    assert_eq!(chapter.block_at(0), 0);
    assert_eq!(chapter.block_at(second - 1), 1);
    assert_eq!(chapter.block_at(second), 2);
    assert_eq!(chapter.block_at(second + 1), 2);
    assert_eq!(chapter.block_at(u32::MAX), chapter.blocks.len() - 1);
    Ok(())
}

#[test]
fn a_chapter_index_out_of_range_is_an_error_without_a_source() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    let error = source.prepare_chapter(2).unwrap_err();
    assert!(matches!(error, BookError::NoSuchChapter(2)));
    assert_eq!(error.to_string(), "the book has no chapter 2");
    assert!(error.source().is_none());
    Ok(())
}

#[test]
fn following_an_extra_anchor_gives_the_place_of_its_line() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    assert_eq!(
        source.follow(0, "#note")?,
        Target::Place {
            chapter: 0,
            offset: offset_of(CH01, "[note]: #note")?
        }
    );
    Ok(())
}

#[test]
fn following_a_link_to_another_chapter_resolves_it_like_a_prepared_one() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    assert_eq!(
        source.follow(0, "ch02.md#second")?,
        Target::Place {
            chapter: 1,
            offset: offset_of(CH02, "## Second")?
        }
    );
    Ok(())
}

#[test]
fn following_a_web_address_gives_it_back() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    assert_eq!(
        source.follow(1, "https://example.org")?,
        Target::External("https://example.org".to_owned())
    );
    Ok(())
}

#[test]
fn following_a_name_the_chapter_does_not_have_leads_nowhere() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    assert_eq!(source.follow(0, "#nothing")?, Target::Unresolved);
    Ok(())
}

#[test]
fn following_a_missing_chapter_leads_nowhere() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    assert_eq!(source.follow(0, "missing.md")?, Target::Unresolved);
    Ok(())
}

#[test]
fn following_from_a_chapter_out_of_range_is_an_error() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    let error = source.follow(5, "#note").unwrap_err();
    assert!(matches!(error, BookError::NoSuchChapter(5)));
    Ok(())
}

#[test]
fn a_picture_is_read_as_the_avif_bytes_it_was_stored_with() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    let chapter = source.prepare_chapter(0)?;
    let picture = *chapter.pictures.get("img/fig.avif").ok_or("no picture")?;
    assert_eq!(source.read_picture(picture)?, fixture("rotated.avif")?);
    Ok(())
}

#[test]
fn a_reference_that_is_not_a_picture_of_the_book_is_refused() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    // Resource 0 is a service resource of every book.
    let forged = PictureRef {
        number: 0,
        width: 1,
        height: 1,
    };
    let error = source.read_picture(forged).unwrap_err();
    assert!(matches!(error, BookError::NoSuchPicture));
    assert_eq!(error.to_string(), "the book has no such picture");
    Ok(())
}

#[test]
fn the_cover_is_read_as_the_avif_bytes_it_was_stored_with() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    assert_eq!(source.read_cover()?, Some(fixture("plain.avif")?));
    Ok(())
}

#[test]
fn a_poisoned_source_is_an_error_value_for_every_call() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    source.poison();
    let picture = PictureRef {
        number: 0,
        width: 1,
        height: 1,
    };
    assert!(matches!(
        source.prepare_chapter(0),
        Err(BookError::Unusable)
    ));
    assert!(matches!(
        source.read_picture(picture),
        Err(BookError::Unusable)
    ));
    assert!(matches!(source.read_cover(), Err(BookError::Unusable)));
    let error = source.follow(0, "#note").unwrap_err();
    assert!(matches!(error, BookError::Unusable));
    assert!(error.source().is_none());
    Ok(())
}

#[test]
fn progress_and_places_of_an_opened_book_follow_its_chapters() -> TestResult {
    let (_dir, snapshot, _source) = open_sample()?;
    let first = u64::try_from(CH01.len())?;
    let total = first + u64::try_from(CH02.len())?;
    assert_eq!(snapshot.progress(0, 0), 0);
    assert_eq!(u64::from(snapshot.progress(1, 0)), first * 1000 / total);
    assert_eq!(snapshot.progress(1, u32::MAX), 1000);
    let place = snapshot.place(1, 5).ok_or("no place")?;
    assert_eq!(
        place,
        Place {
            chapter: "ch02.md".to_owned(),
            offset: 5
        }
    );
    assert_eq!(snapshot.locate(&place), Some((1, 5)));
    Ok(())
}

#[test]
fn the_current_entry_of_an_opened_book_follows_its_headings() -> TestResult {
    let (_dir, snapshot, _source) = open_sample()?;
    let second = offset_of(CH02, "## Second")?;
    assert_eq!(snapshot.current_entry(0, 10), Some(0));
    assert_eq!(snapshot.current_entry(1, second - 1), Some(1));
    assert_eq!(snapshot.current_entry(1, second), Some(2));
    Ok(())
}

// Reading is lazy: mbook-ui docs, "Reading". A reader that decoded the whole
// book would meet the damage below when it opens the file.

/// The sample book with one byte of its figure changed on disk.
fn write_sample_with_a_damaged_figure(dir: &TempDir) -> TestResult<PathBuf> {
    let path = write_sample(dir)?;
    let figure = fixture("rotated.avif")?;
    let mut file = std::fs::read(&path)?;
    // Pictures are stored as they are, so the figure is found in the file.
    let at = file
        .windows(figure.len())
        .position(|window| window == figure)
        .ok_or("the figure is stored as it is")?;
    file[at + figure.len() / 2] ^= 0xff;
    std::fs::write(&path, file)?;
    Ok(path)
}

#[test]
fn a_book_opens_without_its_pictures_being_read() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = write_sample_with_a_damaged_figure(&dir)?;

    let (snapshot, _source) = open(&path)?;

    assert_eq!(snapshot.title, "The Reader");
    assert_eq!(snapshot.chapters.len(), 2);
    Ok(())
}

#[test]
fn a_chapter_is_prepared_without_its_pictures_being_read() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = write_sample_with_a_damaged_figure(&dir)?;
    let (_snapshot, source) = open(&path)?;

    let chapter = source.prepare_chapter(0)?;

    assert_eq!(chapter.text, CH01);
    // The size of the figure comes from the tables, not from its bytes.
    let figure = chapter
        .pictures
        .get("img/fig.avif")
        .copied()
        .ok_or("the figure of the chapter is resolved")?;
    assert_eq!((figure.width, figure.height), (16, 24));
    Ok(())
}

#[test]
fn a_damaged_picture_fails_only_when_it_is_read() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = write_sample_with_a_damaged_figure(&dir)?;
    let (_snapshot, source) = open(&path)?;
    let chapter = source.prepare_chapter(0)?;
    let figure = chapter
        .pictures
        .get("img/fig.avif")
        .copied()
        .ok_or("the figure of the chapter is resolved")?;

    assert!(matches!(
        source.read_picture(figure),
        Err(BookError::Read(_))
    ));
    // The other chapter and the cover are still read.
    assert_eq!(source.prepare_chapter(1)?.text, CH02);
    assert!(source.read_cover()?.is_some());
    Ok(())
}

// Diagrams and formulas: mbook-ui docs, "Diagrams and formulas".

const FLOW: &str = "flowchart LR\n  A --> B\n";
const SPEED: &str = r"v = \frac{s}{t}";
const SHOWN: &str = "# Three

![The way](dia/way.mmd)

Speed is ![v = s / t](math/speed.tex), and again ![v][speed].

![A figure](img/fig.avif)

[speed]: math/speed.tex
";

/// The sample book with a diagram, a formula and a chapter that shows both.
fn write_illustrated(dir: &TempDir) -> TestResult<PathBuf> {
    let path = write_sample(dir)?;
    let mut book = Book::open(&path)?;
    book.add_diagram("dia/way.mmd", FLOW)?;
    book.add_formula("math/speed.tex", SPEED)?;
    book.add_chapter("ch03.md", SHOWN)?;
    book.save(&path)?;
    Ok(path)
}

#[test]
fn a_chapter_is_prepared_with_the_sources_of_its_diagrams_and_formulas() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (_snapshot, source) = open(&write_illustrated(&dir)?)?;

    let chapter = source.prepare_chapter(2)?;

    let figures = &chapter.figures;
    let way = figures
        .by_destination
        .get("dia/way.mmd")
        .ok_or("no diagram")?;
    assert_eq!((way.kind, &*way.source), (FigureKind::Diagram, FLOW));
    let speed = figures
        .by_destination
        .get("math/speed.tex")
        .ok_or("no formula")?;
    assert_eq!((speed.kind, &*speed.source), (FigureKind::Formula, SPEED));
    assert_eq!(figures.by_destination.len(), 2);
    assert_eq!(
        figures
            .by_reference
            .get("speed")
            .map(|reference| reference.destination.as_str()),
        Some("math/speed.tex")
    );
    // A picture stays a picture.
    assert!(chapter.pictures.contains_key("img/fig.avif"));
    assert!(!chapter.pictures.contains_key("dia/way.mmd"));
    Ok(())
}

#[test]
fn a_chapter_without_figures_has_none() -> TestResult {
    let (_dir, _snapshot, source) = open_sample()?;
    let chapter = source.prepare_chapter(0)?;
    assert!(chapter.figures.is_empty());
    assert!(chapter.figures.by_reference.is_empty());
    Ok(())
}

#[test]
fn a_link_to_a_diagram_or_to_a_formula_leads_nowhere() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (_snapshot, source) = open(&write_illustrated(&dir)?)?;
    let reader = source.lock()?;
    let diagram = reader.find_diagram("dia/way.mmd").ok_or("no diagram")?.id;
    let formula = reader
        .find_formula("math/speed.tex")
        .ok_or("no formula")?
        .id;
    let to_diagram = source.target(
        &reader,
        "dia/way.mmd",
        Ok(mbook::Destination::Diagram(diagram)),
    );
    let to_formula = source.target(
        &reader,
        "math/speed.tex",
        Ok(mbook::Destination::Formula(formula)),
    );
    assert_eq!(
        (to_diagram, to_formula),
        (Target::Unresolved, Target::Unresolved)
    );
    Ok(())
}

#[test]
fn a_damaged_formula_has_no_source_and_does_not_fail_its_chapter() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = write_illustrated(&dir)?;
    let mut file = std::fs::read(&path)?;
    // The formula is the last resource: its frame ends where the directory
    // of the container starts.
    let directory = file
        .windows(4)
        .rposition(|window| window == b"INDX")
        .ok_or("a container has a directory")?;
    file[directory - 2] ^= 0xff;
    std::fs::write(&path, file)?;
    let (_snapshot, source) = open(&path)?;

    let chapter = source.prepare_chapter(2)?;

    let speed = chapter
        .figures
        .by_destination
        .get("math/speed.tex")
        .ok_or("the formula is still a figure of the chapter")?;
    assert_eq!((speed.kind, &*speed.source), (FigureKind::Formula, ""));
    let way = chapter
        .figures
        .by_destination
        .get("dia/way.mmd")
        .ok_or("no diagram")?;
    assert_eq!(&*way.source, FLOW);
    Ok(())
}
