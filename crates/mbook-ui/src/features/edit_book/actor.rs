//! The task that owns the book, and the handle that talks to it.

use super::command::{Command, MetaChange, Reply, Request};
use super::error::{EditError, OpenFailure};
use super::outline::{Outline, ProblemOutline};
use futures::StreamExt;
use futures::channel::{mpsc, oneshot};
use mbook::{Book, Meta};
use std::future::Future;
use std::path::PathBuf;

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
    /// Opens the book at `path`.
    fn open(path: PathBuf) -> Self {
        match Book::open(&path) {
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
            Command::SetChapterText { path, text } => {
                book.chapter_mut(&path)
                    .ok_or(EditError::NoSuchPath(path))?
                    .set_text(text)?;
            }
            Command::AddChapter { path, text } => {
                book.add_chapter(path, text)?;
            }
            Command::MoveChapter { path, index } => book.move_chapter(&path, index)?,
            Command::MoveChapterPath { from, to } => move_chapter_path(book, &from, to)?,
            Command::RemoveChapter { path } => {
                book.remove_chapter(&path)
                    .ok_or(EditError::NoSuchPath(path))?;
            }
            Command::AddPicture { path, avif } => {
                book.add_picture(path, avif)?;
            }
            Command::RemovePicture { path } => {
                book.remove_picture(&path)
                    .ok_or(EditError::NoSuchPath(path))?;
            }
            Command::SetCover { path, avif } => {
                book.set_cover(path, avif)?;
            }
            Command::ContentsFromHeadings => book.set_toc(mbook::Toc::default()),
            Command::Load | Command::Check | Command::Save => {}
        }
        Ok(())
    }
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

/// Gives a chapter another path at the same position with the same text.
///
/// The chapter is taken out and put back under the new path; when the book
/// refuses that, it is put back under the old one.
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
        Ok(_) => Ok(()),
        Err(refusal) => {
            // The old path and text were accepted before, so this cannot fail;
            // the refusal is the answer either way.
            drop(book.insert_chapter(index, from, text));
            Err(refusal.into())
        }
    }
}
