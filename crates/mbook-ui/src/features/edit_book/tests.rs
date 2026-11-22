//! The session end to end: a book on disk, commands, and the file after a save.

use super::actor::{EditHandle, session};
use super::command::{Command, ContentsChange, EntryDraft, MetaChange};
use super::error::{ContentsError, EditError};
use super::outline::{Fault, Outline, ProblemOutline, SeriesOutline};
use futures::FutureExt;
use futures::executor::block_on;
use futures::future::join;
use std::error::Error as _;
use std::future::Future;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const PICTURE: &[u8] = include_bytes!("../../../../mbook/tests/fixtures/plain.avif");

/// A book with two chapters and a picture, saved in a new folder.
fn sample() -> Result<(TempDir, PathBuf)> {
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("book.mbook");
    let mut book = mbook::Book::new("The Example", "en")?;
    book.add_chapter("ch01.md", "# Arrival\n\nThe train came in.\n")?;
    book.add_chapter("ch02.md", "# Departure\n")?;
    book.add_picture("img/a.avif", PICTURE)?;
    book.save(&path)?;
    Ok((folder, path))
}

/// The first chapter of [`sections`] with its two inner headings swapped.
const SWAPPED: &str = "# Arrival\n\n## Hall\n\n## Platform\n";

/// A saved book whose first chapter has two headings under its title. Its
/// file keeps four contents entries: Arrival, Platform, Hall, Departure.
fn sections() -> Result<(TempDir, PathBuf)> {
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("book.mbook");
    let mut book = mbook::Book::new("The Example", "en")?;
    book.add_chapter("ch01.md", "# Arrival\n\n## Platform\n\n## Hall\n")?;
    book.add_chapter("ch02.md", "# Departure\n")?;
    book.save(&path)?;
    Ok((folder, path))
}

/// Runs the actor of a session on the book at `path` next to a script that
/// owns the handle; the actor ends when the script does.
fn run<T, F, Fut>(path: &Path, script: F) -> T
where
    F: FnOnce(EditHandle) -> Fut,
    Fut: Future<Output = T>,
{
    let (handle, actor) = session(path.to_owned());
    block_on(async { join(actor, script(handle)).await.1 })
}

/// Sends one command and waits for the outline.
async fn send(handle: &EditHandle, command: Command) -> Result<Outline> {
    Ok(handle.send(command).await?)
}

/// The book as saved in the file.
fn reopened(path: &Path) -> Result<mbook::Book> {
    Ok(mbook::Book::open(path)?)
}

fn paths(outline: &Outline) -> Vec<&str> {
    outline.chapters.iter().map(|c| c.path.as_str()).collect()
}

fn add(path: &str, text: &str) -> Command {
    Command::AddChapter {
        path: path.to_owned(),
        text: text.to_owned(),
    }
}

fn set_text(path: &str, text: &str) -> Command {
    Command::SetChapterText {
        path: path.to_owned(),
        text: text.to_owned(),
    }
}

fn is_send<T: Send>(_: &T) {}

// Loading

#[test]
fn load_gives_the_outline_of_the_opened_book() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move { send(&h, Command::Load).await })?;
    assert_eq!(outline.meta.title, "The Example");
    assert_eq!(outline.meta.language, "en");
    assert_eq!(paths(&outline), ["ch01.md", "ch02.md"]);
    assert_eq!(outline.chapters[0].heading.as_deref(), Some("Arrival"));
    assert_eq!(
        outline.chapters[0].len,
        "# Arrival\n\nThe train came in.\n".len()
    );
    assert_eq!(outline.pictures.len(), 1);
    assert_eq!(outline.pictures[0].path, "img/a.avif");
    assert_eq!(outline.pictures[0].len, PICTURE.len());
    assert!(outline.pictures[0].width > 0 && outline.pictures[0].height > 0);
    assert_eq!(outline.problems.len(), 0);
    assert!(!outline.unsaved);
    Ok(())
}

#[test]
fn the_actor_and_its_futures_can_run_on_another_thread() {
    let (handle, actor) = session(PathBuf::from("none.mbook"));
    is_send(&actor);
    is_send(&handle.send(Command::Load));
    is_send(&handle.chapter_text("a"));
    is_send(&handle.picture("a"));
}

// Metadata

#[test]
fn the_title_is_changed_and_saved() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(&h, Command::SetMeta(MetaChange::Title("New".into()))).await?;
        send(&h, Command::Save).await
    })?;
    assert_eq!(outline.meta.title, "New");
    assert_eq!(reopened(&path)?.meta().title(), "New");
    Ok(())
}

#[test]
fn an_empty_title_is_refused_and_the_old_one_stays() -> Result {
    let (_folder, path) = sample()?;
    let (error, outline) = run(&path, |h| async move {
        let error = h
            .send(Command::SetMeta(MetaChange::Title(String::new())))
            .await
            .err();
        (error, send(&h, Command::Load).await)
    });
    assert!(matches!(
        error,
        Some(EditError::Book(mbook::Error::InvalidMeta(_)))
    ));
    let outline = outline?;
    assert_eq!(outline.meta.title, "The Example");
    assert!(!outline.unsaved);
    Ok(())
}

#[test]
fn authors_replace_the_whole_list() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        let set = |names: &[&str]| {
            Command::SetMeta(MetaChange::Authors(
                names.iter().map(|n| (*n).to_owned()).collect(),
            ))
        };
        send(&h, set(&["A", "B"])).await?;
        send(&h, set(&["C"])).await?;
        send(&h, Command::Save).await
    })?;
    assert_eq!(outline.meta.authors, ["C"]);
    assert_eq!(reopened(&path)?.meta().authors().collect::<Vec<_>>(), ["C"]);
    Ok(())
}

#[test]
fn an_empty_list_removes_the_authors() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(&h, Command::SetMeta(MetaChange::Authors(vec!["A".into()]))).await?;
        send(&h, Command::SetMeta(MetaChange::Authors(Vec::new()))).await
    })?;
    assert_eq!(outline.meta.authors.len(), 0);
    Ok(())
}

#[test]
fn the_language_is_changed() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(&h, Command::SetMeta(MetaChange::Language("ru".into()))).await?;
        send(&h, Command::Save).await
    })?;
    assert_eq!(outline.meta.language, "ru");
    assert_eq!(reopened(&path)?.meta().language(), "ru");
    Ok(())
}

#[test]
fn a_series_with_an_index_is_set_and_then_removed() -> Result {
    let (_folder, path) = sample()?;
    let (with, without) = run(&path, |h| async move {
        let series = Some(("Cycle".to_owned(), Some("2.5".to_owned())));
        let with = send(&h, Command::SetMeta(MetaChange::Series(series))).await?;
        let without = send(&h, Command::SetMeta(MetaChange::Series(None))).await?;
        Result::Ok((with, without))
    })?;
    assert_eq!(
        with.meta.series,
        Some(SeriesOutline {
            name: "Cycle".into(),
            index: Some("2.5".into())
        })
    );
    assert_eq!(without.meta.series, None);
    Ok(())
}

#[test]
fn a_series_without_an_index_drops_the_old_index() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        let first = Some(("Cycle".to_owned(), Some("1".to_owned())));
        send(&h, Command::SetMeta(MetaChange::Series(first))).await?;
        let second = Some(("Cycle".to_owned(), None));
        send(&h, Command::SetMeta(MetaChange::Series(second))).await
    })?;
    assert_eq!(outline.meta.series.map(|s| s.index), Some(None));
    Ok(())
}

#[test]
fn publisher_date_and_annotation_are_set_saved_and_removed() -> Result {
    let (_folder, path) = sample()?;
    let set = run(&path, |h| async move {
        send(
            &h,
            Command::SetMeta(MetaChange::Publisher(Some("Press".into()))),
        )
        .await?;
        send(
            &h,
            Command::SetMeta(MetaChange::Date(Some("2024-05".into()))),
        )
        .await?;
        send(
            &h,
            Command::SetMeta(MetaChange::Annotation(Some("About".into()))),
        )
        .await?;
        send(&h, Command::Save).await
    })?;
    assert_eq!(set.meta.publisher.as_deref(), Some("Press"));
    assert_eq!(set.meta.date.as_deref(), Some("2024-05"));
    assert_eq!(set.meta.annotation.as_deref(), Some("About"));
    let saved = reopened(&path)?;
    assert_eq!(saved.meta().publisher(), Some("Press"));
    assert_eq!(saved.meta().date(), Some("2024-05"));
    assert_eq!(saved.meta().annotation(), Some("About"));

    let cleared = run(&path, |h| async move {
        send(&h, Command::SetMeta(MetaChange::Publisher(None))).await?;
        send(&h, Command::SetMeta(MetaChange::Date(None))).await?;
        send(&h, Command::SetMeta(MetaChange::Annotation(None))).await
    })?;
    assert_eq!(
        (
            cleared.meta.publisher,
            cleared.meta.date,
            cleared.meta.annotation
        ),
        (None, None, None)
    );
    Ok(())
}

#[test]
fn removing_a_key_the_book_does_not_have_succeeds() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(&h, Command::SetMeta(MetaChange::Publisher(None))).await
    })?;
    assert_eq!(outline.meta.publisher, None);
    Ok(())
}

// Chapters

#[test]
fn a_new_chapter_is_added_to_automatic_contents_after_reopening() -> Result {
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("book.mbook");
    let mut book = mbook::Book::new("The Example", "en")?;
    book.add_chapter("ch01.md", "")?;
    book.save(&path)?;
    run(&path, |h| async move {
        send(&h, set_text("ch01.md", "# First\n")).await?;
        send(&h, Command::Save).await
    })?;
    run(&path, |h| async move {
        send(&h, add("ch02.md", "# Second\n")).await?;
        send(&h, Command::Save).await
    })?;

    let reader = mbook::BookReader::open(&path)?;
    let titles: Vec<_> = reader
        .toc()
        .iter()
        .map(|entry| entry.title.as_str())
        .collect();
    assert_eq!(titles, ["First", "Second"]);
    Ok(())
}

#[test]
fn a_chapter_text_change_is_visible_in_the_outline_and_in_the_file() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(&h, set_text("ch01.md", "# Changed\n")).await?;
        send(&h, Command::Save).await
    })?;
    assert_eq!(outline.chapters[0].heading.as_deref(), Some("Changed"));
    assert_eq!(outline.chapters[0].len, "# Changed\n".len());
    let saved = reopened(&path)?;
    assert_eq!(
        saved.chapter("ch01.md").map(mbook::Chapter::text),
        Some("# Changed\n")
    );
    Ok(())
}

#[test]
fn a_text_with_a_control_character_is_refused_with_its_offset() -> Result {
    let (_folder, path) = sample()?;
    let (error, text) = run(&path, |h| async move {
        let error = h.send(set_text("ch01.md", "ab\u{1}c")).await.err();
        (error, h.chapter_text("ch01.md").await)
    });
    let error = error.ok_or("the text was accepted")?;
    assert_eq!(error.text_offset(), Some(2));
    assert_eq!(error.to_string(), "the book refused the change");
    let source = error.source().ok_or("no source")?;
    assert_eq!(source.to_string(), "invalid chapter text");
    assert_eq!(text?, "# Arrival\n\nThe train came in.\n");
    Ok(())
}

#[test]
fn a_text_change_of_an_unknown_chapter_names_the_path() -> Result {
    let (_folder, path) = sample()?;
    let error = run(&path, |h| async move {
        h.send(set_text("nope.md", "x")).await.err()
    });
    assert!(matches!(error, Some(EditError::NoSuchPath(p)) if p == "nope.md"));
    Ok(())
}

#[test]
fn a_chapter_is_added_at_the_end() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(&h, add("ch03.md", "# Return\n")).await?;
        send(&h, Command::Save).await
    })?;
    assert_eq!(paths(&outline), ["ch01.md", "ch02.md", "ch03.md"]);
    let saved = reopened(&path)?;
    assert_eq!(saved.chapters().len(), 3);
    assert_eq!(saved.chapters()[2].path(), "ch03.md");
    Ok(())
}

#[test]
fn a_chapter_with_a_used_path_is_refused_and_the_book_stays() -> Result {
    let (_folder, path) = sample()?;
    let (error, outline) = run(&path, |h| async move {
        let error = h.send(add("ch01.md", "# Other\n")).await.err();
        (error, send(&h, Command::Load).await)
    });
    assert!(matches!(
        error,
        Some(EditError::Book(mbook::Error::DuplicatePath(p))) if p == "ch01.md"
    ));
    let outline = outline?;
    assert_eq!(outline.chapters[0].heading.as_deref(), Some("Arrival"));
    assert!(!outline.unsaved);
    Ok(())
}

#[test]
fn a_chapter_with_an_invalid_path_is_refused() -> Result {
    let (_folder, path) = sample()?;
    let error = run(
        &path,
        |h| async move { h.send(add("../x.md", "")).await.err() },
    );
    assert!(matches!(
        error,
        Some(EditError::Book(mbook::Error::InvalidPath { .. }))
    ));
    Ok(())
}

#[test]
fn a_chapter_moves_in_the_reading_order() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(&h, add("notes.md", "Just notes.\n")).await?;
        send(
            &h,
            Command::MoveChapter {
                path: "notes.md".into(),
                index: 0,
            },
        )
        .await?;
        send(&h, Command::Save).await
    })?;
    assert_eq!(paths(&outline), ["notes.md", "ch01.md", "ch02.md"]);
    assert_eq!(reopened(&path)?.chapters()[0].path(), "notes.md");
    Ok(())
}

#[test]
fn the_contents_entries_of_a_moved_chapter_move_with_it() -> Result {
    // A loaded book keeps the explicit contents of its file.
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(
            &h,
            Command::MoveChapter {
                path: "ch02.md".into(),
                index: 0,
            },
        )
        .await?;
        send(&h, Command::Save).await
    })?;
    assert_eq!(paths(&outline), ["ch02.md", "ch01.md"]);
    assert_eq!(entries(&outline), ["1 Departure", "1 Arrival"]);
    assert_eq!(outline.problems, Vec::new());
    assert!(!outline.unsaved);
    let saved = reopened(&path)?;
    assert_eq!(saved.chapters()[0].path(), "ch02.md");
    assert_eq!(stored(&saved), ["1 Departure", "1 Arrival"]);
    Ok(())
}

#[test]
fn a_list_that_was_out_of_order_is_not_sorted_by_moving_a_chapter() -> Result {
    let (_folder, path) = sections()?;
    let outline = run(&path, |h| async move {
        send(&h, set_text("ch01.md", SWAPPED)).await?;
        send(&h, add("ch03.md", "# Return\n")).await?;
        send(
            &h,
            Command::MoveChapter {
                path: "ch03.md".into(),
                index: 0,
            },
        )
        .await
    })?;
    assert_eq!(paths(&outline), ["ch03.md", "ch01.md", "ch02.md"]);
    assert_eq!(
        entries(&outline),
        ["1 Arrival", "2 Platform", "2 Hall", "1 Departure"]
    );
    Ok(())
}

#[test]
fn the_contents_entries_of_a_chapter_follow_it_to_another_path() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        let rename = Command::MoveChapterPath {
            from: "ch02.md".into(),
            to: "leaving.md".into(),
        };
        send(&h, rename).await?;
        send(&h, Command::Save).await
    })?;
    let leads: Vec<&str> = outline
        .contents
        .entries
        .iter()
        .map(|entry| entry.chapter.as_str())
        .collect();
    assert_eq!(leads, ["ch01.md", "leaving.md"]);
    assert!(outline.contents.entries.iter().all(|e| e.fault.is_none()));
    assert_eq!(outline.problems, Vec::new());
    assert_eq!(stored(&reopened(&path)?), ["1 Arrival", "1 Departure"]);
    Ok(())
}

#[test]
fn a_chapter_moved_past_the_end_is_refused_and_stays() -> Result {
    let (_folder, path) = sample()?;
    let (error, outline) = run(&path, |h| async move {
        let error = h
            .send(Command::MoveChapter {
                path: "ch01.md".into(),
                index: 2,
            })
            .await
            .err();
        (error, send(&h, Command::Load).await)
    });
    assert!(matches!(
        error,
        Some(EditError::Book(mbook::Error::IndexOutOfRange {
            index: 2,
            ..
        }))
    ));
    assert_eq!(paths(&outline?), ["ch01.md", "ch02.md"]);
    Ok(())
}

#[test]
fn moving_an_unknown_chapter_names_the_path() -> Result {
    let (_folder, path) = sample()?;
    let error = run(&path, |h| async move {
        h.send(Command::MoveChapter {
            path: "nope.md".into(),
            index: 0,
        })
        .await
        .err()
    });
    assert!(matches!(error, Some(EditError::NoSuchPath(p)) if p == "nope.md"));
    Ok(())
}

#[test]
fn a_chapter_keeps_its_place_and_text_when_its_path_changes() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(&h, add("notes.md", "Just notes.\n")).await?;
        let rename = Command::MoveChapterPath {
            from: "notes.md".into(),
            to: "extra.md".into(),
        };
        send(&h, rename).await?;
        send(&h, Command::Save).await
    })?;
    assert_eq!(paths(&outline), ["ch01.md", "ch02.md", "extra.md"]);
    let saved = reopened(&path)?;
    assert_eq!(
        saved.chapter("extra.md").map(mbook::Chapter::text),
        Some("Just notes.\n")
    );
    assert!(saved.chapter("notes.md").is_none());
    Ok(())
}

#[test]
fn contents_that_follow_the_headings_stay_so_when_a_chapter_gets_another_path() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(&h, Command::ContentsFromHeadings).await?;
        let rename = Command::MoveChapterPath {
            from: "ch02.md".into(),
            to: "leaving.md".into(),
        };
        send(&h, rename).await
    })?;
    assert!(outline.contents.automatic);
    assert_eq!(outline.contents.entries[1].chapter, "leaving.md");
    Ok(())
}

#[test]
fn a_path_change_to_a_used_path_leaves_the_chapter_where_and_what_it_was() -> Result {
    let (_folder, path) = sample()?;
    let (error, outline, text) = run(&path, |h| async move {
        let rename = Command::MoveChapterPath {
            from: "ch01.md".into(),
            to: "ch02.md".into(),
        };
        let error = h.send(rename).await.err();
        (
            error,
            send(&h, Command::Load).await,
            h.chapter_text("ch01.md").await,
        )
    });
    assert!(matches!(
        error,
        Some(EditError::Book(mbook::Error::DuplicatePath(_)))
    ));
    let outline = outline?;
    assert_eq!(paths(&outline), ["ch01.md", "ch02.md"]);
    assert!(!outline.unsaved);
    assert_eq!(text?, "# Arrival\n\nThe train came in.\n");
    Ok(())
}

#[test]
fn a_path_change_to_an_invalid_path_in_the_middle_keeps_the_position() -> Result {
    let (_folder, path) = sample()?;
    let (error, outline) = run(&path, |h| async move {
        send(&h, add("ch03.md", "# Three\n")).await?;
        let rename = Command::MoveChapterPath {
            from: "ch02.md".into(),
            to: "a\\b.md".into(),
        };
        let error = h.send(rename).await.err();
        Result::Ok((error, send(&h, Command::Load).await?))
    })?;
    assert!(matches!(
        error,
        Some(EditError::Book(mbook::Error::InvalidPath { .. }))
    ));
    assert_eq!(paths(&outline), ["ch01.md", "ch02.md", "ch03.md"]);
    assert_eq!(outline.chapters[1].heading.as_deref(), Some("Departure"));
    Ok(())
}

#[test]
fn a_path_change_of_an_unknown_chapter_names_the_path() -> Result {
    let (_folder, path) = sample()?;
    let error = run(&path, |h| async move {
        let rename = Command::MoveChapterPath {
            from: "nope.md".into(),
            to: "x.md".into(),
        };
        h.send(rename).await.err()
    });
    assert!(matches!(error, Some(EditError::NoSuchPath(p)) if p == "nope.md"));
    Ok(())
}

#[test]
fn a_chapter_is_removed() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(&h, add("notes.md", "Just notes.\n")).await?;
        send(
            &h,
            Command::RemoveChapter {
                path: "notes.md".into(),
            },
        )
        .await?;
        send(&h, Command::Save).await
    })?;
    assert_eq!(paths(&outline), ["ch01.md", "ch02.md"]);
    assert_eq!(reopened(&path)?.chapters().len(), 2);
    Ok(())
}

#[test]
fn the_contents_entries_of_a_removed_chapter_are_removed_with_it() -> Result {
    // A loaded book keeps the explicit contents of its file.
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(
            &h,
            Command::RemoveChapter {
                path: "ch02.md".into(),
            },
        )
        .await?;
        send(&h, Command::Save).await
    })?;
    assert_eq!(paths(&outline), ["ch01.md"]);
    assert!(!outline.contents.automatic);
    assert_eq!(entries(&outline), ["1 Arrival"]);
    assert_eq!(outline.problems, Vec::new());
    assert!(!outline.unsaved);
    let saved = reopened(&path)?;
    assert_eq!(saved.chapters().len(), 1);
    assert_eq!(stored(&saved), ["1 Arrival"]);
    Ok(())
}

#[test]
fn a_list_whose_last_entries_go_with_their_chapter_follows_the_headings() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(&h, add("ch03.md", "# Return\n")).await?;
        for gone in ["ch01.md", "ch02.md"] {
            send(&h, Command::RemoveChapter { path: gone.into() }).await?;
        }
        send(&h, Command::Load).await
    })?;
    assert!(outline.contents.automatic);
    assert_eq!(entries(&outline), ["1 Return"]);
    Ok(())
}

#[test]
fn contents_that_follow_the_headings_lose_the_headings_of_a_removed_chapter() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(&h, Command::ContentsFromHeadings).await?;
        send(
            &h,
            Command::RemoveChapter {
                path: "ch02.md".into(),
            },
        )
        .await
    })?;
    assert!(outline.contents.automatic);
    assert_eq!(entries(&outline), ["1 Arrival"]);
    Ok(())
}

/// A start landmark at the first chapter and notes at the second.
fn two_landmarks() -> Command {
    Command::SetLandmarks(vec![
        mbook::Landmark::new("start", "", "ch01.md"),
        mbook::Landmark::new("notes", "Endnotes", "ch02.md"),
    ])
}

#[test]
fn the_landmarks_of_a_removed_chapter_are_removed_with_it() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(&h, two_landmarks()).await?;
        send(
            &h,
            Command::RemoveChapter {
                path: "ch02.md".into(),
            },
        )
        .await?;
        send(&h, Command::Save).await
    })?;
    assert_eq!(
        outline.landmarks,
        [mbook::Landmark::new("start", "", "ch01.md")]
    );
    assert_eq!(outline.problems, Vec::new());
    assert_eq!(reopened(&path)?.landmarks().len(), 1);
    Ok(())
}

#[test]
fn the_landmarks_of_a_chapter_follow_it_to_another_path() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(&h, two_landmarks()).await?;
        let rename = Command::MoveChapterPath {
            from: "ch02.md".into(),
            to: "leaving.md".into(),
        };
        send(&h, rename).await?;
        send(&h, Command::Save).await
    })?;
    assert_eq!(
        outline.landmarks,
        [
            mbook::Landmark::new("start", "", "ch01.md"),
            mbook::Landmark::new("notes", "Endnotes", "leaving.md"),
        ]
    );
    assert_eq!(outline.problems, Vec::new());
    assert_eq!(reopened(&path)?.landmarks()[1].chapter, "leaving.md");
    Ok(())
}

#[test]
fn a_refused_path_leaves_the_landmarks_of_the_chapter_as_they_were() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(&h, two_landmarks()).await?;
        let rename = Command::MoveChapterPath {
            from: "ch02.md".into(),
            to: "ch01.md".into(),
        };
        assert!(h.send(rename).await.is_err());
        send(&h, Command::Load).await
    })?;
    assert_eq!(outline.landmarks[1].chapter, "ch02.md");
    Ok(())
}

#[test]
fn removing_a_chapter_without_entries_leaves_the_list_as_it_is() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(&h, add("ch03.md", "# Return\n")).await?;
        send(
            &h,
            Command::RemoveChapter {
                path: "ch03.md".into(),
            },
        )
        .await
    })?;
    assert!(!outline.contents.automatic);
    assert_eq!(entries(&outline), ["1 Arrival", "1 Departure"]);
    Ok(())
}

#[test]
fn contents_built_from_the_headings_let_a_book_without_a_chapter_be_saved() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(
            &h,
            Command::RemoveChapter {
                path: "ch02.md".into(),
            },
        )
        .await?;
        send(&h, Command::ContentsFromHeadings).await?;
        send(&h, Command::Save).await
    })?;
    assert_eq!(outline.problems, Vec::new());
    assert!(!outline.unsaved);
    let book = reopened(&path)?;
    assert_eq!(book.chapters().len(), 1);
    Ok(())
}

#[test]
fn building_the_contents_from_the_headings_is_a_change_to_save() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(&h, Command::ContentsFromHeadings).await
    })?;
    assert!(outline.unsaved);
    Ok(())
}

// The list of contents a book keeps

fn entry(title: &str, chapter: &str, anchor: Option<&str>) -> EntryDraft {
    EntryDraft {
        title: title.to_owned(),
        chapter: chapter.to_owned(),
        anchor: anchor.map(str::to_owned),
    }
}

/// The entries of the outline as `level title`.
fn entries(outline: &Outline) -> Vec<String> {
    let entries = outline.contents.entries.iter();
    entries
        .map(|entry| format!("{} {}", entry.level, entry.title))
        .collect()
}

/// The contents of a book as `level title`, when it keeps a list.
fn stored(book: &mbook::Book) -> Vec<String> {
    match book.toc() {
        mbook::Toc::Explicit(items) => items
            .iter()
            .map(|item| format!("{} {}", item.level, item.title))
            .collect(),
        mbook::Toc::FromHeadings { .. } => Vec::new(),
    }
}

#[test]
fn an_entry_of_the_stored_list_gets_another_title_and_is_saved() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        let change = ContentsChange::Set {
            index: 1,
            entry: entry("Leaving", "ch02.md", None),
        };
        send(&h, Command::EditContents(change)).await?;
        send(&h, Command::Save).await
    })?;
    assert!(!outline.contents.automatic);
    assert_eq!(outline.problems, Vec::new());
    assert_eq!(stored(&reopened(&path)?), ["1 Arrival", "1 Leaving"]);
    Ok(())
}

#[test]
fn an_entry_is_added_where_its_place_is_and_moved_in_under_the_entry_above() -> Result {
    let (_folder, path) = sample()?;
    let (added, outline) = run(&path, |h| async move {
        send(&h, set_text("ch01.md", "# Arrival\n\n## Platform\n")).await?;
        let add = ContentsChange::Add(entry("The platform", "ch01.md", Some("platform")));
        let added = send(&h, Command::EditContents(add)).await?;
        let shift = ContentsChange::Shift {
            index: 1,
            deeper: true,
        };
        send(&h, Command::EditContents(shift)).await?;
        Ok::<_, Box<dyn std::error::Error>>((added, send(&h, Command::Save).await?))
    })?;
    assert_eq!(
        entries(&added),
        ["1 Arrival", "1 The platform", "1 Departure"]
    );
    assert!(added.unsaved);
    assert_eq!(outline.problems, Vec::new());
    assert_eq!(
        stored(&reopened(&path)?),
        ["1 Arrival", "2 The platform", "1 Departure"]
    );
    Ok(())
}

#[test]
fn contents_that_follow_the_headings_have_no_list_to_change() -> Result {
    let (_folder, path) = sample()?;
    let (error, outline) = run(&path, |h| async move {
        send(&h, Command::ContentsFromHeadings).await?;
        let remove = ContentsChange::Remove { index: 0 };
        let error = h.send(Command::EditContents(remove)).await.err();
        Ok::<_, Box<dyn std::error::Error>>((error, send(&h, Command::Load).await?))
    })?;
    assert!(matches!(
        error,
        Some(EditError::Contents(ContentsError::Automatic))
    ));
    assert_eq!(entries(&outline), ["1 Arrival", "1 Departure"]);
    Ok(())
}

#[test]
fn a_list_that_loses_its_last_entry_follows_the_headings() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        let remove = || Command::EditContents(ContentsChange::Remove { index: 0 });
        let one_left = send(&h, remove()).await?;
        assert_eq!(entries(&one_left), ["1 Departure"]);
        assert!(!one_left.contents.automatic);
        send(&h, remove()).await
    })?;
    assert!(outline.contents.automatic);
    assert_eq!(entries(&outline), ["1 Arrival", "1 Departure"]);
    Ok(())
}

#[test]
fn entries_that_headings_moved_in_the_text_left_out_of_order_are_sorted() -> Result {
    let (_folder, path) = sections()?;
    let (swapped, outline) = run(&path, |h| async move {
        let swapped = send(&h, set_text("ch01.md", SWAPPED)).await?;
        send(&h, Command::EditContents(ContentsChange::InBookOrder)).await?;
        Ok::<_, Box<dyn std::error::Error>>((swapped, send(&h, Command::Save).await?))
    })?;
    let faults: Vec<_> = swapped.contents.entries.iter().map(|e| e.fault).collect();
    assert_eq!(faults, [None, None, Some(Fault::OutOfOrder), None]);
    assert_eq!(
        entries(&outline),
        ["1 Arrival", "2 Hall", "2 Platform", "1 Departure"]
    );
    assert!(outline.contents.entries.iter().all(|e| e.fault.is_none()));
    assert_eq!(outline.problems, Vec::new());
    Ok(())
}

#[test]
fn a_refused_change_of_the_list_leaves_the_book_as_it_was() -> Result {
    let (_folder, path) = sample()?;
    let (error, outline) = run(&path, |h| async move {
        let change = ContentsChange::Set {
            index: 9,
            entry: entry("Nine", "ch01.md", None),
        };
        let error = h.send(Command::EditContents(change)).await.err();
        (error, send(&h, Command::Load).await)
    });
    assert!(matches!(
        error,
        Some(EditError::Contents(ContentsError::NoSuchEntry(9)))
    ));
    let outline = outline?;
    assert!(!outline.unsaved);
    assert_eq!(entries(&outline), ["1 Arrival", "1 Departure"]);
    Ok(())
}

#[test]
fn removing_an_unknown_chapter_names_the_path() -> Result {
    let (_folder, path) = sample()?;
    let (error, outline) = run(&path, |h| async move {
        let error = h
            .send(Command::RemoveChapter {
                path: "nope.md".into(),
            })
            .await
            .err();
        (error, send(&h, Command::Load).await)
    });
    assert!(matches!(error, Some(EditError::NoSuchPath(p)) if p == "nope.md"));
    assert!(!outline?.unsaved);
    Ok(())
}

#[test]
fn removing_a_picture_path_as_a_chapter_is_refused() -> Result {
    let (_folder, path) = sample()?;
    let (error, outline) = run(&path, |h| async move {
        let error = h
            .send(Command::RemoveChapter {
                path: "img/a.avif".into(),
            })
            .await
            .err();
        (error, send(&h, Command::Load).await)
    });
    assert!(matches!(error, Some(EditError::NoSuchPath(_))));
    assert_eq!(outline?.pictures.len(), 1);
    Ok(())
}

// Pictures

#[test]
fn a_picture_is_added_and_saved() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(
            &h,
            Command::AddPicture {
                path: "img/b.avif".into(),
                avif: PICTURE.to_vec(),
            },
        )
        .await?;
        send(&h, Command::Save).await
    })?;
    assert_eq!(outline.pictures.len(), 2);
    assert_eq!(outline.pictures[1].path, "img/b.avif");
    assert!(reopened(&path)?.picture("img/b.avif").is_some());
    Ok(())
}

#[test]
fn bytes_that_are_not_avif_are_refused_and_no_picture_is_added() -> Result {
    let (_folder, path) = sample()?;
    let (error, outline) = run(&path, |h| async move {
        let error = h
            .send(Command::AddPicture {
                path: "img/b.avif".into(),
                avif: b"not avif".to_vec(),
            })
            .await
            .err();
        (error, send(&h, Command::Load).await)
    });
    assert!(matches!(
        error,
        Some(EditError::Book(mbook::Error::InvalidPicture(_)))
    ));
    let outline = outline?;
    assert_eq!(outline.pictures.len(), 1);
    assert!(!outline.unsaved);
    Ok(())
}

#[test]
fn a_picture_with_a_used_path_is_refused() -> Result {
    let (_folder, path) = sample()?;
    let error = run(&path, |h| async move {
        h.send(Command::AddPicture {
            path: "img/a.avif".into(),
            avif: PICTURE.to_vec(),
        })
        .await
        .err()
    });
    assert!(matches!(
        error,
        Some(EditError::Book(mbook::Error::DuplicatePath(_)))
    ));
    Ok(())
}

#[test]
fn a_picture_is_removed() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(
            &h,
            Command::RemovePicture {
                path: "img/a.avif".into(),
            },
        )
        .await?;
        send(&h, Command::Save).await
    })?;
    assert_eq!(outline.pictures.len(), 0);
    assert_eq!(reopened(&path)?.pictures().len(), 0);
    Ok(())
}

#[test]
fn removing_an_unknown_picture_names_the_path() -> Result {
    let (_folder, path) = sample()?;
    let error = run(&path, |h| async move {
        h.send(Command::RemovePicture {
            path: "img/none.avif".into(),
        })
        .await
        .err()
    });
    assert!(matches!(error, Some(EditError::NoSuchPath(p)) if p == "img/none.avif"));
    Ok(())
}

#[test]
fn a_cover_is_added_and_named_by_the_metadata() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(
            &h,
            Command::SetCover {
                path: "cover.avif".into(),
                avif: PICTURE.to_vec(),
            },
        )
        .await?;
        send(&h, Command::Save).await
    })?;
    assert_eq!(outline.meta.cover.as_deref(), Some("cover.avif"));
    assert_eq!(outline.pictures.len(), 2);
    assert_eq!(reopened(&path)?.meta().cover(), Some("cover.avif"));
    Ok(())
}

#[test]
fn a_refused_cover_leaves_the_cover_key_as_it_was() -> Result {
    let (_folder, path) = sample()?;
    let (error, outline) = run(&path, |h| async move {
        let error = h
            .send(Command::SetCover {
                path: "cover.avif".into(),
                avif: b"no".to_vec(),
            })
            .await
            .err();
        (error, send(&h, Command::Load).await)
    });
    assert!(error.is_some());
    let outline = outline?;
    assert_eq!(outline.meta.cover, None);
    assert_eq!(outline.pictures.len(), 1);
    Ok(())
}

// Check and save

/// A chapter with a link to a chapter that is not in the book.
const DANGLING: &str = "# One\n\n[x](missing.md)\n";

#[test]
fn a_problem_is_reported_with_its_path_and_cleared_by_the_fix() -> Result {
    let (_folder, path) = sample()?;
    let (found, fixed) = run(&path, |h| async move {
        send(&h, set_text("ch01.md", DANGLING)).await?;
        let found = send(&h, Command::Check).await?;
        send(&h, set_text("ch01.md", "# One\n")).await?;
        let fixed = send(&h, Command::Check).await?;
        Result::Ok((found, fixed))
    })?;
    assert_eq!(
        found.problems,
        [ProblemOutline {
            path: Some("ch01.md".into()),
            offset: Some(7),
            message: "The link to \"missing.md\" leads nowhere.".into(),
        }]
    );
    assert_eq!(fixed.problems.len(), 0);
    Ok(())
}

#[test]
fn check_changes_nothing_but_the_problems() -> Result {
    let (_folder, path) = sample()?;
    let (before, after) = run(&path, |h| async move {
        let before = send(&h, Command::Load).await?;
        let after = send(&h, Command::Check).await?;
        Result::Ok((before, after))
    })?;
    assert_eq!(before, after);
    Ok(())
}

#[test]
fn check_after_a_change_keeps_the_book_unsaved() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(&h, add("ch03.md", "")).await?;
        send(&h, Command::Check).await
    })?;
    assert!(outline.unsaved);
    Ok(())
}

#[test]
fn a_save_with_a_problem_writes_nothing_and_reports_it() -> Result {
    let (_folder, path) = sample()?;
    let before = std::fs::read(&path)?;
    let outline = run(&path, |h| async move {
        send(&h, set_text("ch01.md", DANGLING)).await?;
        send(&h, Command::Save).await
    })?;
    assert_eq!(std::fs::read(&path)?, before);
    assert!(outline.unsaved);
    assert_eq!(outline.problems.len(), 1);
    assert_eq!(outline.problems[0].path.as_deref(), Some("ch01.md"));
    Ok(())
}

#[test]
fn a_change_drops_the_stale_problems() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(&h, set_text("ch01.md", DANGLING)).await?;
        send(&h, Command::Check).await?;
        send(&h, add("ch03.md", "")).await
    })?;
    assert_eq!(outline.problems.len(), 0);
    Ok(())
}

#[test]
fn a_failed_command_keeps_the_problems() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(&h, set_text("ch01.md", DANGLING)).await?;
        send(&h, Command::Check).await?;
        drop(h.send(add("ch01.md", "")).await);
        send(&h, Command::Load).await
    })?;
    assert_eq!(outline.problems.len(), 1);
    Ok(())
}

#[test]
fn the_book_is_unsaved_after_a_change_and_saved_after_a_save() -> Result {
    let (_folder, path) = sample()?;
    let (changed, saved) = run(&path, |h| async move {
        let changed = send(&h, add("ch03.md", "")).await?;
        let saved = send(&h, Command::Save).await?;
        Result::Ok((changed, saved))
    })?;
    assert!(changed.unsaved);
    assert!(!saved.unsaved);
    assert_eq!(saved.problems.len(), 0);
    Ok(())
}

#[test]
fn setting_the_title_the_book_already_has_still_counts_as_a_change() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        send(
            &h,
            Command::SetMeta(MetaChange::Title("The Example".into())),
        )
        .await
    })?;
    assert!(outline.unsaved);
    Ok(())
}

#[test]
fn a_save_that_cannot_write_is_an_error_and_the_book_stays_unsaved() -> Result {
    let (folder, path) = sample()?;
    let (error, outline) = run(&path, |h| async move {
        let _ = send(&h, add("ch03.md", "")).await;
        // The folder of the book disappears under the session.
        let gone = std::fs::remove_dir_all(folder.path());
        let error = h.send(Command::Save).await.err();
        (gone.map(|()| error), send(&h, Command::Check).await)
    });
    let error = error?.ok_or("the save succeeded")?;
    assert!(matches!(error, EditError::Save(_)));
    assert_eq!(error.to_string(), "the book could not be saved");
    assert!(error.source().is_some());
    assert!(outline?.unsaved);
    Ok(())
}

// Order

#[test]
fn commands_are_applied_in_the_order_they_were_sent() -> Result {
    let (_folder, path) = sample()?;
    run(&path, |h| async move {
        let change = h.send(set_text("ch01.md", "# Late\n"));
        let save = h.send(Command::Save);
        // The save is awaited first and still includes the change.
        let saved = save.await;
        let changed = change.await;
        (saved.map(|_| ()), changed.map(|_| ()))
    })
    .0?;
    assert_eq!(
        reopened(&path)?
            .chapter("ch01.md")
            .map(mbook::Chapter::text),
        Some("# Late\n")
    );
    Ok(())
}

#[test]
fn requests_that_are_never_awaited_are_still_applied() -> Result {
    let (_folder, path) = sample()?;
    let outline = run(&path, |h| async move {
        drop(h.send(add("ch03.md", "")));
        send(&h, Command::Load).await
    })?;
    assert_eq!(outline.chapters.len(), 3);
    Ok(())
}

#[test]
fn a_read_after_a_command_sees_the_command() -> Result {
    let (_folder, path) = sample()?;
    let text = run(&path, |h| async move {
        let change = h.send(set_text("ch02.md", "# New\n"));
        let read = h.chapter_text("ch02.md");
        let text = read.await;
        drop(change.await);
        text
    })?;
    assert_eq!(text, "# New\n");
    Ok(())
}

// Reading a chapter and a picture

#[test]
fn the_text_of_a_known_chapter_is_given() -> Result {
    let (_folder, path) = sample()?;
    let text = run(&path, |h| async move { h.chapter_text("ch02.md").await })?;
    assert_eq!(text, "# Departure\n");
    Ok(())
}

#[test]
fn the_text_of_an_unknown_chapter_names_the_path() -> Result {
    let (_folder, path) = sample()?;
    let error = run(
        &path,
        |h| async move { h.chapter_text("nope.md").await.err() },
    );
    assert!(matches!(error, Some(EditError::NoSuchPath(p)) if p == "nope.md"));
    Ok(())
}

#[test]
fn the_text_of_a_picture_path_is_not_given() -> Result {
    let (_folder, path) = sample()?;
    let error = run(&path, |h| async move {
        h.chapter_text("img/a.avif").await.err()
    });
    assert!(matches!(error, Some(EditError::NoSuchPath(_))));
    Ok(())
}

#[test]
fn the_bytes_of_a_known_picture_are_given() -> Result {
    let (_folder, path) = sample()?;
    let bytes = run(&path, |h| async move { h.picture("img/a.avif").await })?;
    assert_eq!(bytes, PICTURE);
    Ok(())
}

#[test]
fn the_bytes_of_an_unknown_picture_name_the_path() -> Result {
    let (_folder, path) = sample()?;
    let error = run(
        &path,
        |h| async move { h.picture("img/none.avif").await.err() },
    );
    assert!(matches!(error, Some(EditError::NoSuchPath(p)) if p == "img/none.avif"));
    Ok(())
}

#[test]
fn the_bytes_of_a_chapter_path_are_not_given() -> Result {
    let (_folder, path) = sample()?;
    let error = run(&path, |h| async move { h.picture("ch01.md").await.err() });
    assert!(matches!(error, Some(EditError::NoSuchPath(_))));
    Ok(())
}

// A session that cannot start, and one that ends

#[test]
fn a_missing_file_answers_every_request_with_the_opening_error() -> Result {
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("missing.mbook");
    let errors = run(&path, |h| async move {
        vec![
            h.send(Command::Load).await.err(),
            h.send(add("a.md", "")).await.err(),
            h.send(Command::Save).await.err(),
            h.chapter_text("a.md").await.err(),
            h.picture("a").await.err(),
        ]
    });
    assert_eq!(errors.len(), 5);
    for error in errors {
        let Some(EditError::Open(failure)) = &error else {
            return Err(format!("not an opening error: {error:?}").into());
        };
        assert!(failure.to_string().starts_with("I/O failed: "));
        let error = error.as_ref().ok_or("no error")?;
        assert_eq!(error.to_string(), "the book could not be opened");
        let source = error.source().ok_or("no source")?;
        assert!(source.to_string().starts_with("I/O failed: "));
    }
    assert!(!path.exists());
    Ok(())
}

#[test]
fn a_file_that_is_not_a_book_answers_with_the_opening_error() -> Result {
    let folder = tempfile::tempdir()?;
    let path = folder.path().join("text.mbook");
    std::fs::write(&path, b"this is not a book")?;
    let error = run(&path, |h| async move { h.send(Command::Load).await.err() });
    assert!(matches!(error, Some(EditError::Open(_))));
    Ok(())
}

#[test]
fn a_handle_used_after_the_actor_was_dropped_says_the_session_has_ended() -> Result {
    let (_folder, path) = sample()?;
    let (handle, actor) = session(path);
    drop(actor);
    let error = block_on(handle.send(Command::Load)).err();
    assert!(matches!(error, Some(EditError::Ended)));
    assert_eq!(
        block_on(handle.chapter_text("ch01.md"))
            .err()
            .map(|e| e.to_string())
            .as_deref(),
        Some("the editing session has ended")
    );
    assert!(matches!(
        block_on(handle.picture("a")),
        Err(EditError::Ended)
    ));
    Ok(())
}

#[test]
fn a_request_in_flight_when_the_actor_is_dropped_says_the_session_has_ended() -> Result {
    let (_folder, path) = sample()?;
    let (handle, actor) = session(path);
    let pending = handle.send(Command::Load);
    drop(actor);
    assert!(matches!(block_on(pending), Err(EditError::Ended)));
    Ok(())
}

#[test]
fn the_actor_ends_when_every_handle_is_dropped() -> Result {
    let (_folder, path) = sample()?;
    let (handle, actor) = session(path);
    let second = handle.clone();
    let mut actor = Box::pin(actor);
    drop(handle);
    // A handle is left, so the actor waits for requests.
    assert!((&mut actor).now_or_never().is_none());
    drop(second);
    assert_eq!(actor.now_or_never(), Some(()));
    Ok(())
}

#[test]
fn figure_edits_are_saved_at_the_original_path() -> Result {
    use crate::entities::figure::FigureKind;

    let (_folder, path) = sample()?;
    for (kind, figure, initial, updated) in [
        (
            FigureKind::Diagram,
            "dia/line.mmd",
            "flowchart LR\n  A --> B",
            "flowchart LR\n  B --> C",
        ),
        (FigureKind::Formula, "math/x.tex", "x^2", "x^3"),
    ] {
        run(&path, |handle| async move {
            let outline = send(
                &handle,
                Command::AddFigure {
                    kind,
                    path: figure.to_owned(),
                    source: initial.to_owned(),
                },
            )
            .await?;
            assert!(outline.unsaved);
            assert!(
                outline
                    .figures
                    .iter()
                    .any(|entry| entry.kind == kind && entry.path == figure)
            );
            assert_eq!(handle.figure(kind, figure).await?, initial);
            send(
                &handle,
                Command::SetFigureSource {
                    kind,
                    path: figure.to_owned(),
                    source: updated.to_owned(),
                },
            )
            .await?;
            assert_eq!(handle.figure(kind, figure).await?, updated);
            assert!(!send(&handle, Command::Save).await?.unsaved);
            Ok::<_, Box<dyn std::error::Error>>(())
        })?;
    }
    let book = reopened(&path)?;
    assert_eq!(
        book.diagram("dia/line.mmd").map(mbook::Diagram::source),
        Some("flowchart LR\n  B --> C")
    );
    assert_eq!(
        book.formula("math/x.tex").map(mbook::Formula::source),
        Some("x^3")
    );
    Ok(())
}

#[test]
fn removing_each_kind_of_figure_updates_the_outline_and_saved_book() -> Result {
    use crate::entities::figure::FigureKind;

    let (_folder, path) = sample()?;
    let mut book = reopened(&path)?;
    book.add_diagram("d", "flowchart LR\n  A --> B")?;
    book.add_formula("f", "x")?;
    book.save(&path)?;
    run(&path, |handle| async move {
        for (kind, figure) in [(FigureKind::Diagram, "d"), (FigureKind::Formula, "f")] {
            let outline = send(
                &handle,
                Command::RemoveFigure {
                    kind,
                    path: figure.to_owned(),
                },
            )
            .await?;
            assert!(!outline.figures.iter().any(|entry| entry.path == figure));
            assert!(
                matches!(handle.figure(kind, figure).await, Err(EditError::NoSuchPath(path)) if path == figure)
            );
        }
        assert!(!send(&handle, Command::Save).await?.unsaved);
        Ok::<_, Box<dyn std::error::Error>>(())
    })?;
    let book = reopened(&path)?;
    assert_eq!(book.diagrams(), []);
    assert_eq!(book.formulas(), []);
    Ok(())
}

#[test]
fn a_draft_resolves_its_figures_without_changing_the_chapter() -> Result {
    use crate::entities::figure::FigureKind;

    let (_folder, path) = sample()?;
    let mut book = reopened(&path)?;
    book.add_diagram("dia/road map", "flowchart LR\n  A --> B")?;
    book.add_formula("math/x", "x^2")?;
    book.save(&path)?;
    run(&path, |handle| async move {
        let draft = "![Road](../dia/road%20map) ![Squared][formula] ![Photo][photo] ![Missing](gone)\n\n[formula]: ../math/x\n[photo]: ../img/a.avif";
        let sources = handle
            .chapter_figures("part/ch01.md".into(), draft.into())
            .await?;
        assert_eq!(sources.by_destination.len(), 2);
        assert_eq!(
            sources.by_destination["../dia/road%20map"].kind,
            FigureKind::Diagram
        );
        assert_eq!(sources.by_destination["../math/x"].source.as_ref(), "x^2");
        assert_eq!(sources.by_reference.len(), 1);
        assert_eq!(sources.by_reference["formula"].destination, "../math/x");
        assert!(!send(&handle, Command::Load).await?.unsaved);
        Ok::<_, Box<dyn std::error::Error>>(())
    })
}

#[test]
fn a_figure_command_refuses_the_wrong_kind_or_invalid_source() -> Result {
    use crate::entities::figure::FigureKind;

    let (_folder, path) = sample()?;
    let mut book = reopened(&path)?;
    book.add_formula("f", "x")?;
    book.save(&path)?;
    run(&path, |handle| async move {
        assert!(matches!(
            handle
                .send(Command::RemoveFigure {
                    kind: FigureKind::Diagram,
                    path: "f".into(),
                })
                .await,
            Err(EditError::NoSuchPath(_))
        ));
        assert!(matches!(
            handle
                .send(Command::SetFigureSource {
                    kind: FigureKind::Diagram,
                    path: "f".into(),
                    source: "flowchart LR".into(),
                })
                .await,
            Err(EditError::NoSuchPath(_))
        ));
        assert!(matches!(
            handle
                .send(Command::SetFigureSource {
                    kind: FigureKind::Formula,
                    path: "f".into(),
                    source: " ".into(),
                })
                .await,
            Err(EditError::Book(mbook::Error::InvalidFormula(_)))
        ));
        assert_eq!(handle.figure(FigureKind::Formula, "f").await?, "x");
        assert!(!send(&handle, Command::Load).await?.unsaved);
        Ok::<_, Box<dyn std::error::Error>>(())
    })
}
