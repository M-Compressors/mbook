//! The commands of an editing session and the requests that carry them.

use super::agent::error::AgentError;
use super::agent::{Answer, Op};
use super::error::EditError;
use super::outline::Outline;
use crate::entities::figure::{FigureKind, FigureSources};
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
    /// Moves a chapter to another position of the reading order. The entries
    /// of a contents list the book keeps move with their chapter, when the
    /// list was in the order of the book.
    MoveChapter {
        /// The path of the chapter.
        path: String,
        /// Its new position; less than the number of chapters.
        index: usize,
    },
    /// Gives a chapter another path and keeps its position and text. `Book`
    /// has no rename, so the chapter is removed and inserted at the same
    /// position; when the new path is refused, the chapter stays as it was.
    /// The landmarks that led into it, and the entries of a contents list
    /// the book keeps, follow it to the new path.
    MoveChapterPath {
        /// The path the chapter has.
        from: String,
        /// The path it gets.
        to: String,
    },
    /// Removes a chapter, and with it the landmarks and the entries of a
    /// contents list the book keeps that led into it. A list left without
    /// entries follows the headings from then on.
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
    /// Adds a diagram or formula.
    AddFigure {
        kind: FigureKind,
        path: String,
        source: String,
    },
    /// Replaces the source while keeping existing references valid.
    SetFigureSource {
        kind: FigureKind,
        path: String,
        source: String,
    },
    /// Removes a diagram or formula.
    RemoveFigure { kind: FigureKind, path: String },
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
    /// Changes the list of contents the book keeps.
    ///
    /// Contents that follow the headings have no list and refuse the change.
    /// A list that loses its last entry follows the headings from then on,
    /// as an empty list does when a book is opened.
    EditContents(ContentsChange),
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

/// One change of a contents list the book keeps. An entry is named by its
/// position in the list of the last outline, counted from 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ContentsChange {
    /// Adds an entry where its place is in the book, at the level of the
    /// entry above it.
    Add(EntryDraft),
    /// Gives an entry another title or place. An entry that leads elsewhere
    /// moves to where that place is in the book.
    Set {
        /// The position of the entry.
        index: usize,
        /// What it says and where it leads from now on.
        entry: EntryDraft,
    },
    /// Moves an entry, and the entries under it, one level in or out.
    Shift {
        /// The position of the entry.
        index: usize,
        /// One level in, under the entry above; otherwise one level out.
        deeper: bool,
    },
    /// Removes an entry. The entries under it move one level up.
    Remove {
        /// The position of the entry.
        index: usize,
    },
    /// Sorts the entries by where they lead in the book.
    InBookOrder,
}

/// What an entry of the contents says and where it leads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct EntryDraft {
    /// The title; it must not be empty.
    pub(crate) title: String,
    /// The path of the chapter.
    pub(crate) chapter: String,
    /// The name of a heading of that chapter; `None` is its start.
    pub(crate) anchor: Option<String>,
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
    /// The source of a diagram or formula.
    Figure(FigureKind, String, Reply<String>),
    /// The sources referenced by a draft chapter, which might not be saved yet.
    ChapterFigures(String, String, Reply<FigureSources>),
    /// The text of a chapter and the last change an agent made to it.
    EditorText(String, Reply<(String, u64)>),
    /// A call of an agent.
    Agent(Op, oneshot::Sender<Result<Answer, AgentError>>),
    /// Reads the book again from its file.
    Discard(Reply<()>),
    /// The chapter the editor holds unsent text of, and the last change by
    /// an agent it has seen of it.
    Hold(Option<(String, u64)>),
    /// Where to tell the changes agents make.
    Subscribe(futures::channel::mpsc::UnboundedSender<Change>),
}

/// Chapters an agent changed, each with the change it is at now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Change {
    /// The path of each chapter and the last change of it by an agent.
    pub(crate) chapters: Vec<(String, u64)>,
}
