//! The task that owns the book, and the handle that talks to it.

use super::command::{Command, ContentsChange, MetaChange, Reply, Request};
use super::contents::{self, Places};
use super::error::{ContentsError, EditError, OpenFailure};
use super::outline::{Outline, ProblemOutline};
use crate::entities::book::chapter::{image_path, shown};
use crate::entities::figure::{FigureKind, FigureSource, FigureSources};
use futures::StreamExt;
use futures::channel::{mpsc, oneshot};
use mbook::{Book, Meta, Toc};
use std::future::Future;
use std::path::PathBuf;

const CONTENTS_MODE: &str = "x-mbook-ui-contents";
const HEADINGS: &str = "headings";

/// Starts a session for the book at `path`.
///
/// Returns the handle the main thread keeps and the future that owns the
/// book. The caller spawns the future on a background thread; it opens the
/// book when first polled and ends when every handle is dropped.
pub(crate) fn session(path: PathBuf) -> (EditHandle, impl Future<Output = ()> + Send + 'static) {
    let (requests, incoming) = mpsc::unbounded();
    (EditHandle { requests }, serve(path, incoming))
}

/// Opens the book and answers requests until every handle is gone.
async fn serve(path: PathBuf, mut incoming: mpsc::UnboundedReceiver<Request>) {
    let mut state = State::open(path);
    while let Some(request) = incoming.next().await {
        state.answer(request);
    }
}

/// The side of a session the main thread keeps.
#[derive(Clone, Debug)]
pub(crate) struct EditHandle {
    requests: mpsc::UnboundedSender<Request>,
}

impl EditHandle {
    /// Sends a command. Commands are applied in the order they were sent,
    /// whatever the order in which their futures are awaited. The future gives
    /// the outline after the command.
    ///
    /// # Errors
    ///
    /// The future fails with [`EditError::Ended`] when the session is gone,
    /// with [`EditError::Open`] when the book could not be opened, and
    /// otherwise as the command says; a failed command leaves the book as it
    /// was.
    pub(crate) fn send(
        &self,
        command: Command,
    ) -> impl Future<Output = Result<Outline, EditError>> + Send + 'static {
        self.ask(|reply| Request::Command(command, reply))
    }

    /// The current text of a chapter.
    ///
    /// # Errors
    ///
    /// [`EditError::NoSuchPath`] when no chapter has this path, and the errors
    /// of [`send`](Self::send).
    pub(crate) fn chapter_text(
        &self,
        path: &str,
    ) -> impl Future<Output = Result<String, EditError>> + Send + 'static {
        let path = path.to_owned();
        self.ask(|reply| Request::ChapterText(path, reply))
    }

    /// The AVIF bytes of a picture, for the preview.
    ///
    /// # Errors
    ///
    /// [`EditError::NoSuchPath`] when no picture has this path, and the errors
    /// of [`send`](Self::send).
    pub(crate) fn picture(
        &self,
        path: &str,
    ) -> impl Future<Output = Result<Vec<u8>, EditError>> + Send + 'static {
        let path = path.to_owned();
        self.ask(|reply| Request::Picture(path, reply))
    }

    /// Asks for the editable source of a figure.
    ///
    /// # Errors
    ///
    /// Returns an error when the path does not name a figure of this kind
    /// or the session cannot answer.
    pub(crate) fn figure(
        &self,
        kind: FigureKind,
        path: &str,
    ) -> impl Future<Output = Result<String, EditError>> + Send + 'static {
        let path = path.to_owned();
        self.ask(move |reply| Request::Figure(kind, path, reply))
    }

    /// Gets the figures in the supplied draft, including reference-style images.
    pub(crate) fn chapter_figures(
        &self,
        chapter: String,
        text: String,
    ) -> impl Future<Output = Result<FigureSources, EditError>> + Send + 'static {
        self.ask(|reply| Request::ChapterFigures(chapter, text, reply))
    }

    /// Queues a request now, so that the order of calls is the order of
    /// application, and waits for the answer later.
    fn ask<T: Send + 'static>(
        &self,
        request: impl FnOnce(Reply<T>) -> Request,
    ) -> impl Future<Output = Result<T, EditError>> + Send + 'static {
        let (reply, answer) = oneshot::channel();
        // A closed channel drops the request and with it the reply, which the
        // answer reports as the end of the session.
        drop(self.requests.unbounded_send(request(reply)));
        async move { answer.await.unwrap_or(Err(EditError::Ended)) }
    }
}

/// What the task owns.
enum State {
    /// The book is open.
    Open(Box<Session>),
    /// The book could not be opened; every request gets this failure.
    Failed(OpenFailure),
}

impl State {
    /// Opens the book at `path` and restores any automatic contents.
    fn open(path: PathBuf) -> Self {
        let book = Book::open(&path).and_then(|mut book| {
            if book.meta().get(CONTENTS_MODE) == Some(HEADINGS)
                || matches!(book.toc(), Toc::Explicit(items) if items.is_empty())
            {
                contents_from_headings(&mut book)?;
            }
            Ok(book)
        });
        match book {
            Ok(book) => Self::Open(Box::new(Session {
                book,
                path,
                problems: Vec::new(),
                unsaved: false,
            })),
            Err(error) => Self::Failed(OpenFailure::new(&error)),
        }
    }

    /// Serves one request. A request whose caller stopped waiting is still
    /// served: the change is made.
    fn answer(&mut self, request: Request) {
        match request {
            Request::Command(command, reply) => {
                let outcome = match self {
                    Self::Open(session) => session.command(command),
                    Self::Failed(failure) => Err(EditError::Open(failure.clone())),
                };
                drop(reply.send(outcome));
            }
            Request::ChapterText(path, reply) => {
                let outcome = self.read(|book| {
                    book.chapter(&path)
                        .map(|chapter| chapter.text().to_owned())
                        .ok_or_else(|| EditError::NoSuchPath(path.clone()))
                });
                drop(reply.send(outcome));
            }
            Request::Picture(path, reply) => {
                let outcome = self.read(|book| {
                    book.picture(&path)
                        .map(|picture| picture.data().to_vec())
                        .ok_or_else(|| EditError::NoSuchPath(path.clone()))
                });
                drop(reply.send(outcome));
            }
            Request::Figure(kind, path, reply) => {
                let outcome = self.read(|book| {
                    match kind {
                        FigureKind::Diagram => book.diagram(&path).map(mbook::Diagram::source),
                        FigureKind::Formula => book.formula(&path).map(mbook::Formula::source),
                    }
                    .map(str::to_owned)
                    .ok_or(EditError::NoSuchPath(path))
                });
                drop(reply.send(outcome));
            }
            Request::ChapterFigures(chapter, text, reply) => {
                let outcome = self.read(|book| Ok(chapter_figures(book, &chapter, &text)));
                drop(reply.send(outcome));
            }
        }
    }

    /// Reads something from the book, or fails as the opening did.
    fn read<T>(&self, read: impl FnOnce(&Book) -> Result<T, EditError>) -> Result<T, EditError> {
        match self {
            Self::Open(session) => read(&session.book),
            Self::Failed(failure) => Err(EditError::Open(failure.clone())),
        }
    }
}

/// Resolves the resources used as images in the draft against the editing book.
fn chapter_figures(book: &Book, chapter: &str, text: &str) -> FigureSources {
    let (destinations, mut references) = shown(text);
    let by_destination: std::collections::HashMap<_, _> = destinations
        .into_iter()
        .filter_map(|destination| {
            let path = image_path(chapter, &destination)?;
            let (kind, source) = if let Some(diagram) = book.diagram(&path) {
                (FigureKind::Diagram, diagram.source())
            } else {
                (FigureKind::Formula, book.formula(&path)?.source())
            };
            Some((
                destination,
                FigureSource {
                    kind,
                    source: source.into(),
                },
            ))
        })
        .collect();
    references.retain(|_, reference| by_destination.contains_key(&reference.destination));
    FigureSources {
        by_destination,
        by_reference: references,
    }
}

/// An open book and what is known about it.
struct Session {
    book: Book,
    /// Where `Save` writes.
    path: PathBuf,
    /// The problems of the last `Check` or `Save`, until the next change.
    problems: Vec<ProblemOutline>,
    /// A change was made since the book was opened or last saved.
    unsaved: bool,
}

impl Session {
    /// Applies a command and describes the book afterwards.
    ///
    /// A command that changes the book sets `unsaved` and drops the problems,
    /// which are stale then; this holds also for a change that leaves the
    /// book equal to what it was, such as the title it already has. A failed
    /// command changes nothing.
    fn command(&mut self, command: Command) -> Result<Outline, EditError> {
        match command {
            Command::Load => {}
            Command::Check => self.problems = self.found(),
            Command::Save => self.save()?,
            change => {
                self.apply(change)?;
                self.problems.clear();
                self.unsaved = true;
            }
        }
        Ok(Outline::of(&self.book, &self.problems, self.unsaved))
    }

    /// Runs `Book::check`.
    fn found(&self) -> Vec<ProblemOutline> {
        self.book
            .check()
            .err()
            .map_or_default(|problems| ProblemOutline::all(&problems))
    }

    /// Writes the book when it has no problems.
    fn save(&mut self) -> Result<(), EditError> {
        self.problems = self.found();
        if self.problems.is_empty() {
            self.book.save(&self.path).map_err(EditError::Save)?;
            self.unsaved = false;
        }
        Ok(())
    }

    /// Makes one change to the book. On error the book is as it was.
    fn apply(&mut self, change: Command) -> Result<(), EditError> {
        let book = &mut self.book;
        match change {
            Command::SetMeta(change) => set_meta(book.meta_mut(), change)?,
            Command::SetLandmarks(landmarks) => book.set_landmarks(landmarks)?,
            Command::SetChapterText { path, text } => {
                book.chapter_mut(&path)
                    .ok_or(EditError::NoSuchPath(path))?
                    .set_text(text)?;
            }
            Command::AddChapter { path, text } => {
                book.add_chapter(path, text)?;
            }
            Command::MoveChapter { path, index } => move_chapter(book, &path, index)?,
            Command::MoveChapterPath { from, to } => move_chapter_path(book, &from, to)?,
            Command::RemoveChapter { path } => remove_chapter(book, &path)?,
            Command::AddPicture { path, avif } => {
                book.add_picture(path, avif)?;
            }
            Command::RemovePicture { path } => {
                book.remove_picture(&path)
                    .ok_or(EditError::NoSuchPath(path))?;
            }
            Command::AddFigure { kind, path, source } => match kind {
                FigureKind::Diagram => {
                    book.add_diagram(path, source)?;
                }
                FigureKind::Formula => {
                    book.add_formula(path, source)?;
                }
            },
            Command::SetFigureSource { kind, path, source } => match kind {
                FigureKind::Diagram => book
                    .diagram_mut(&path)
                    .ok_or(EditError::NoSuchPath(path))?
                    .set_source(source)?,
                FigureKind::Formula => book
                    .formula_mut(&path)
                    .ok_or(EditError::NoSuchPath(path))?
                    .set_source(source)?,
            },
            Command::RemoveFigure { kind, path } => {
                match kind {
                    FigureKind::Diagram => book.remove_diagram(&path).is_some(),
                    FigureKind::Formula => book.remove_formula(&path).is_some(),
                }
                .then_some(())
                .ok_or(EditError::NoSuchPath(path))?;
            }
            Command::SetCover { path, avif } => {
                book.set_cover(path, avif)?;
            }
            Command::ContentsFromHeadings => contents_from_headings(book)?,
            Command::EditContents(change) => edit_contents(book, change)?,
            Command::Load | Command::Check | Command::Save => {}
        }
        Ok(())
    }
}

/// Selects automatic contents and records the choice for the next session.
fn contents_from_headings(book: &mut Book) -> mbook::Result<()> {
    if book.meta().get(CONTENTS_MODE) != Some(HEADINGS) {
        book.meta_mut().set(CONTENTS_MODE, HEADINGS)?;
    }
    book.set_toc(Toc::default());
    Ok(())
}

/// Changes the list of contents the book keeps. A list left without entries
/// gives way to the headings, as an empty list does when a book is opened.
fn edit_contents(book: &mut Book, change: ContentsChange) -> Result<(), EditError> {
    let Toc::Explicit(items) = book.toc() else {
        return Err(ContentsError::Automatic.into());
    };
    let mut items = items.clone();
    contents::change(&mut items, change, &Places::of(book))?;
    Ok(keep_list(book, items)?)
}

/// Stores a contents list, or selects automatic contents for an empty one.
fn keep_list(book: &mut Book, items: Vec<mbook::TocItem>) -> mbook::Result<()> {
    if items.is_empty() {
        contents_from_headings(book)
    } else {
        book.set_toc(Toc::Explicit(items));
        Ok(())
    }
}

/// Removes a chapter, and with it what led into it and would lead nowhere:
/// the landmarks, of every kind, and the entries of a contents list the book
/// keeps.
fn remove_chapter(book: &mut Book, path: &str) -> Result<(), EditError> {
    if book.chapter(path).is_none() {
        return Err(EditError::NoSuchPath(path.to_owned()));
    }
    let mut landmarks = book.landmarks().to_vec();
    landmarks.retain(|landmark| landmark.chapter != path);
    book.set_landmarks(landmarks)?;
    if let Toc::Explicit(items) = book.toc() {
        let mut items = items.clone();
        if contents::forget_chapter(&mut items, path) > 0 {
            keep_list(book, items)?;
        }
    }
    drop(book.remove_chapter(path));
    Ok(())
}

/// Changes the metadata; on error it is as it was, which matters for the
/// changes that take several calls.
fn set_meta(meta: &mut Meta, change: MetaChange) -> Result<(), EditError> {
    let before = meta.clone();
    let result = change_meta(meta, change);
    if result.is_err() {
        *meta = before;
    }
    Ok(result?)
}

/// Makes the calls of one change of the metadata.
fn change_meta(meta: &mut Meta, change: MetaChange) -> mbook::Result<()> {
    match change {
        MetaChange::Title(title) => meta.set_title(title),
        MetaChange::Language(tag) => meta.set_language(tag),
        MetaChange::Authors(authors) => {
            meta.remove("author")?;
            authors
                .into_iter()
                .try_for_each(|author| meta.add_author(author))
        }
        MetaChange::Series(Some((name, index))) => meta.set_series(name, index.as_deref()),
        MetaChange::Series(None) => {
            meta.remove("series")?;
            meta.remove("series-index")
        }
        MetaChange::Publisher(value) => set_or_remove(meta, "publisher", value),
        MetaChange::Date(value) => set_or_remove(meta, "date", value),
        MetaChange::Annotation(value) => set_or_remove(meta, "annotation", value),
    }
}

/// Sets a key, or removes it for `None`.
fn set_or_remove(meta: &mut Meta, key: &str, value: Option<String>) -> mbook::Result<()> {
    match value {
        Some(value) => meta.set(key, value),
        None => meta.remove(key),
    }
}

/// Moves a chapter in the reading order. A contents list the book keeps is
/// sorted again when it was in the order of the book before, so that the
/// entries of the chapter move with it; a list that was out of order already
/// is left for the author to sort.
fn move_chapter(book: &mut Book, path: &str, index: usize) -> Result<(), EditError> {
    let followed = match book.toc() {
        Toc::Explicit(items) if contents::in_order(items, &Places::of(book)) => Some(items.clone()),
        _ => None,
    };
    book.move_chapter(path, index)?;
    if let Some(mut items) = followed {
        contents::put_in_order(&mut items, &Places::of(book));
        book.set_toc(Toc::Explicit(items));
    }
    Ok(())
}

/// Gives a chapter another path at the same position with the same text.
///
/// The chapter is taken out and put back under the new path; when the book
/// refuses that, it is put back under the old one. The landmarks that led
/// into the chapter, and the entries of a contents list the book keeps,
/// follow it to its new path.
fn move_chapter_path(book: &mut Book, from: &str, to: String) -> Result<(), EditError> {
    let index = book
        .chapters()
        .iter()
        .position(|chapter| chapter.path() == from)
        .ok_or_else(|| EditError::NoSuchPath(from.to_owned()))?;
    let text = book
        .remove_chapter(from)
        .map(|chapter| chapter.text().to_owned())
        .ok_or_else(|| EditError::NoSuchPath(from.to_owned()))?;
    match book.insert_chapter(index, to, text.clone()) {
        Ok(chapter) => {
            let to = chapter.path().to_owned();
            if let Toc::Explicit(items) = book.toc() {
                let mut items = items.clone();
                contents::follow_path(&mut items, from, &to);
                book.set_toc(Toc::Explicit(items));
            }
            let mut landmarks = book.landmarks().to_vec();
            for landmark in landmarks.iter_mut().filter(|l| l.chapter == from) {
                landmark.chapter.clone_from(&to);
            }
            // The book took these kinds before, and a path is not checked
            // here, so this cannot fail.
            drop(book.set_landmarks(landmarks));
            Ok(())
        }
        Err(refusal) => {
            // The old path and text were accepted before, so this cannot fail;
            // the refusal is the answer either way.
            drop(book.insert_chapter(index, from, text));
            Err(refusal.into())
        }
    }
}
