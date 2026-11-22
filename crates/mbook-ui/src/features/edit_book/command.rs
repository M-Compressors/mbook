//! The commands of an editing session and the requests that carry them.

use super::error::EditError;
use super::outline::Outline;
use futures::channel::oneshot;

/// One step of editing. The session applies commands in the order they were
/// sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    /// Changes nothing; answers with the outline of the book as opened. It is
    /// the first command of a session.
    Load,
    /// Changes one key of the metadata.
    SetMeta(MetaChange),
    /// Replaces the book's list of landmarks.
    SetLandmarks(Vec<mbook::Landmark>),
    /// Replaces the text of a chapter.
    SetChapterText {
        /// The path of the chapter.
        path: String,
        /// The new Markdown text.
        text: String,
    },
    /// Adds a chapter after the last one.
    AddChapter {
        /// The path of the new chapter.
        path: String,
        /// Its Markdown text.
        text: String,
    },
    /// Moves a chapter to another position of the reading order.
    MoveChapter {
        /// The path of the chapter.
        path: String,
        /// Its new position; less than the number of chapters.
        index: usize,
    },
    /// Gives a chapter another path and keeps its position and text. `Book`
    /// has no rename, so the chapter is removed and inserted at the same
    /// position; when the new path is refused, the chapter stays as it was.
    MoveChapterPath {
        /// The path the chapter has.
        from: String,
        /// The path it gets.
        to: String,
    },
    /// Removes a chapter.
    RemoveChapter {
        /// The path of the chapter.
        path: String,
    },
    /// Adds a picture.
    AddPicture {
        /// The path of the new picture.
        path: String,
        /// The AVIF file.
        avif: Vec<u8>,
    },
    /// Removes a picture.
    RemovePicture {
        /// The path of the picture.
        path: String,
    },
    /// Adds a picture and makes it the cover.
    SetCover {
        /// The path of the new picture.
        path: String,
        /// The AVIF file.
        avif: Vec<u8>,
    },
    /// Switches the book to automatic contents from chapter headings.
    ///
    /// A book with a stored, nonempty list keeps its entries. When edits
    /// invalidate those entries, this command replaces the list and records
    /// the choice for future sessions.
    ContentsFromHeadings,
    /// Runs `Book::check`; the problems are in the outline. Nothing else
    /// changes.
    Check,
    /// Runs `Book::check`, and when it finds nothing writes the book onto the
    /// path of the session. With problems nothing is written and the outline
    /// holds them.
    Save,
}

/// One change of the metadata: the keys the Details form shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MetaChange {
    /// The title; it must not be empty.
    Title(String),
    /// The whole list of authors; an empty list removes them.
    Authors(Vec<String>),
    /// The main language, replacing every language the book has.
    Language(String),
    /// The series: its name and the optional index in it; `None` removes both.
    Series(Option<(String, Option<String>)>),
    /// The publisher; `None` removes it.
    Publisher(Option<String>),
    /// The publication date; `None` removes it.
    Date(Option<String>),
    /// The annotation; `None` removes it.
    Annotation(Option<String>),
}

/// The answer to a request: the value, or why there is none.
pub(crate) type Reply<T> = oneshot::Sender<Result<T, EditError>>;

/// What the handle sends to the task that owns the book.
#[derive(Debug)]
pub(crate) enum Request {
    /// A command and where its outline goes.
    Command(Command, Reply<Outline>),
    /// The text of the chapter with this path.
    ChapterText(String, Reply<String>),
    /// The AVIF bytes of the picture with this path.
    Picture(String, Reply<Vec<u8>>),
}
