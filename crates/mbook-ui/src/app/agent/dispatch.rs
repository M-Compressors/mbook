//! The main thread's side of a call: the session of the book, the library,
//! the dialog that asks the person, and the window (mbook-ui docs, "Agents
//! (MCP)", "How the server works").

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::str::FromStr as _;

use futures::StreamExt as _;
use futures::channel::{mpsc, oneshot};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::{WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, AsyncApp, Entity, Global, ParentElement as _,
    SharedString, Styled as _, Task, px,
};
use mbook_converter::Format;
use rust_i18n::t;

use super::bridge::{Call, Caller, CreateBook, ExportBook, FileArgs, ListBooks, Request, Show};
use super::pictures::{self, Source};
use super::server::{BookSummary, Books, Created, Written};
use crate::app::shell::Shell;
use crate::entities::catalogue::{Library, Store};
use crate::features::agents::AgentStatus;
use crate::features::edit_book::agent::error::{AgentError, chain};
use crate::features::edit_book::agent::types::{ChapterArgs, Done, MetaPatch, PathArgs};
use crate::features::edit_book::agent::{Answer, Op};
use crate::features::edit_book::sessions::Sessions;
use crate::features::import_book::{create_book, import, now_ms};
use crate::pages::editor::record_save;
use crate::pages::library::export_to;
use crate::shared::compute;
use crate::shared::model::BookKey;
use crate::shared::ui::target_size;

/// The window of the application, where dialogs are shown.
#[derive(Clone)]
pub(crate) struct MainWindow {
    pub(crate) handle: AnyWindowHandle,
    /// The shell that holds the pages; `None` in a window without one.
    pub(crate) shell: Option<Entity<Shell>>,
}

impl Global for MainWindow {}

/// What a call asks the person for, by kind. Allowing a kind can be
/// remembered for a client until the application closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Action {
    RemoveChapter,
    RemovePicture,
    RemoveFigure,
    Discard,
    DeleteBook,
    /// Reading a book file of the computer into the library.
    Import,
    /// Reading a picture file of the computer into a book.
    ReadPicture,
    /// Writing a new file on the computer.
    Export,
    /// Writing over a file of the computer.
    Overwrite,
}

impl Action {
    /// The words of the box that remembers the choice: "Allow … until mbook
    /// closes".
    pub(crate) fn always(self) -> String {
        match self {
            Self::RemoveChapter => t!("agent.always.remove_chapter"),
            Self::RemovePicture => t!("agent.always.remove_picture"),
            Self::RemoveFigure => t!("agent.always.remove_figure"),
            Self::Discard => t!("agent.always.discard"),
            Self::DeleteBook => t!("agent.always.delete_book"),
            Self::Import => t!("agent.always.import"),
            Self::ReadPicture => t!("agent.always.read_picture"),
            Self::Export => t!("agent.always.export"),
            Self::Overwrite => t!("agent.always.overwrite"),
        }
        .into_owned()
    }
}

/// The kinds of action each session of the server was allowed. A session is
/// numbered by the server, so a client cannot take another's permission by
/// giving its name.
#[derive(Debug, Default)]
pub(crate) struct Allowed(HashSet<(u64, Action)>);

impl Global for Allowed {}

impl Allowed {
    /// Whether the session of `client` may do `action` without asking.
    pub(crate) fn has(client: &Caller, action: Action, cx: &App) -> bool {
        cx.try_global::<Self>()
            .is_some_and(|allowed| allowed.0.contains(&(client.session, action)))
    }

    /// Lets the session of `client` do `action` without asking for as long
    /// as it lasts.
    pub(crate) fn remember(client: &Caller, action: Action, cx: &mut App) {
        cx.default_global::<Self>()
            .0
            .insert((client.session, action));
    }
}

/// Answers the calls that cross the bridge, for as long as the task lives.
pub(crate) fn listen(mut calls: mpsc::UnboundedReceiver<Call>, cx: &mut App) -> Task<()> {
    cx.spawn(async move |cx| {
        while let Some(call) = calls.next().await {
            let Call {
                request,
                client,
                reply,
            } = call;
            let cx = cx.clone();
            cx.spawn(async move |cx| {
                working(true, cx);
                let outcome = answer(request, &client, cx).await;
                working(false, cx);
                drop(reply.send(outcome));
            })
            .detach();
        }
    })
}

/// Tells the title bar that a call began or ended.
fn working(started: bool, cx: &mut AsyncApp) {
    cx.update(|cx| {
        let status = AgentStatus::global(cx);
        status.update(cx, |status, cx| status.working(started, cx));
    });
}

/// A key of the library.
fn key(book: &str, cx: &AsyncApp) -> Result<BookKey, AgentError> {
    let key = BookKey::from_str(book).map_err(|_| AgentError::NoSuchBook(book.to_owned()))?;
    let known = cx.update(|cx| Library::global(cx).read(cx).book(key).is_some());
    if known {
        Ok(key)
    } else {
        Err(AgentError::NoSuchBook(book.to_owned()))
    }
}

/// Runs one call.
async fn answer(
    request: Request,
    client: &Caller,
    cx: &mut AsyncApp,
) -> Result<Answer, AgentError> {
    match request {
        Request::ListBooks(args) => Ok(Answer::of(&list_books(&args, cx))),
        Request::CreateBook(args) => create(args, cx).await,
        Request::ImportBook(args) => import_file(args, client, cx).await,
        Request::ExportBook(args) => export(args, client, cx).await,
        Request::DeleteBook { book } => delete(&book, client, cx).await,
        Request::Show(args) => show(&args, cx),
        Request::Book { book, op } => on_book(&book, op, client, cx).await,
        Request::AddPicture {
            book,
            source,
            path,
            encoding,
            cover,
        } => {
            let key = key(&book, cx)?;
            if let Source::File { file } = &source {
                let question = t!(
                    "agent.confirm.read_picture",
                    path = file.display().to_string(),
                    book = title(key, cx)
                );
                confirm(client, Action::ReadPicture, question.into_owned(), cx).await?;
            }
            let name = source.name();
            let avif = compute::spawn(move || pictures::encode(&source, encoding)).await?;
            let op = Op::AddPicture {
                path,
                name,
                avif,
                cover,
            };
            run(key, op, cx).await
        }
        Request::ViewPicture {
            book,
            path,
            max_size,
        } => {
            let key = key(&book, cx)?;
            let mut answer = run(key, Op::Picture(PathArgs { path }), cx).await?;
            let avif = answer.avif.take().unwrap_or_default();
            answer.png = Some(compute::spawn(move || pictures::png(&avif, max_size)).await?);
            Ok(answer)
        }
        Request::Discard { book } => {
            let key = key(&book, cx)?;
            let title = title(key, cx);
            confirm(
                client,
                Action::Discard,
                t!("agent.confirm.discard", book = title).into_owned(),
                cx,
            )
            .await?;
            let handle = cx.update(|cx| Sessions::agent(key, cx));
            handle.discard().await.map_err(AgentError::from)?;
            Ok(Answer::of(&Done {}))
        }
    }
}

/// The title of a book of the library.
fn title(key: BookKey, cx: &AsyncApp) -> String {
    cx.update(|cx| {
        Library::global(cx)
            .read(cx)
            .book(key)
            .map_or_else(String::new, |book| book.row.title.clone())
    })
}

/// A call on one book, through its session.
async fn on_book(
    book: &str,
    op: Op,
    client: &Caller,
    cx: &mut AsyncApp,
) -> Result<Answer, AgentError> {
    let key = key(book, cx)?;
    let ask = match &op {
        Op::RemoveChapter(ChapterArgs { chapter }) => Some((
            Action::RemoveChapter,
            t!(
                "agent.confirm.remove_chapter",
                chapter = chapter,
                book = title(key, cx)
            ),
        )),
        Op::RemovePicture(PathArgs { path }) => Some((
            Action::RemovePicture,
            t!(
                "agent.confirm.remove_picture",
                path = path,
                book = title(key, cx)
            ),
        )),
        Op::RemoveFigure(PathArgs { path }) => Some((
            Action::RemoveFigure,
            t!(
                "agent.confirm.remove_figure",
                path = path,
                book = title(key, cx)
            ),
        )),
        _ => None,
    };
    if let Some((action, question)) = ask {
        confirm(client, action, question.into_owned(), cx).await?;
    }
    let op = match op {
        Op::GetBook { .. } => Op::GetBook {
            open_in_editor: cx.update(|cx| Sessions::in_editor(key, cx)),
        },
        other => other,
    };
    run(key, op, cx).await
}

/// Runs a call in the session of the book, and records a save in the
/// library.
async fn run(key: BookKey, op: Op, cx: &mut AsyncApp) -> Result<Answer, AgentError> {
    let saving = matches!(op, Op::Save);
    let handle = cx.update(|cx| Sessions::agent(key, cx));
    let answer = handle.agent(op).await?;
    cx.update(|cx| Sessions::note(key, answer.unsaved, cx));
    if saving && answer.value["saved"] == true {
        let (roots, catalogue) = cx.update(|cx| {
            let store = Store::global(cx);
            (store.roots.clone(), store.catalogue.clone())
        });
        let recorded = cx
            .background_spawn(async move { record_save(&roots, &catalogue, key).await })
            .await;
        // The book is saved; a row that could not be made again is made
        // when the library is rebuilt.
        drop(recorded);
        cx.update(|cx| Library::global(cx).update(cx, Library::reload));
    }
    Ok(answer)
}

/// The books of the library that match the query.
fn list_books(args: &ListBooks, cx: &AsyncApp) -> Books {
    let query = args.query.as_deref().unwrap_or_default().to_lowercase();
    let limit = args.limit.unwrap_or(50).max(1);
    cx.update(|cx| {
        let books = Library::global(cx)
            .read(cx)
            .books()
            .iter()
            .filter(|book| {
                let row = &book.row;
                query.is_empty()
                    || row.title.to_lowercase().contains(&query)
                    || row
                        .authors
                        .iter()
                        .any(|a| a.to_lowercase().contains(&query))
                    || row
                        .series
                        .as_deref()
                        .is_some_and(|s| s.to_lowercase().contains(&query))
            })
            .take(limit)
            .map(|book| BookSummary {
                book: book.row.key.to_string(),
                title: book.row.title.clone(),
                authors: book.row.authors.clone(),
                language: book.row.language.clone(),
                series: book.row.series.clone(),
                added: book.row.added_ms,
                open_in_editor: Sessions::in_editor(book.row.key, cx),
            })
            .collect();
        Books { books }
    })
}

/// Makes a book with an empty first chapter, in the language asked for.
async fn create(args: CreateBook, cx: &mut AsyncApp) -> Result<Answer, AgentError> {
    if args.title.trim().is_empty() {
        return Err(AgentError::InvalidMeta {
            field: "title".into(),
            message: "the title is empty".into(),
        });
    }
    let (roots, catalogue) = cx.update(|cx| {
        let store = Store::global(cx);
        (store.roots.clone(), store.catalogue.clone())
    });
    let title = args.title.clone();
    let key = cx
        .background_spawn(async move { create_book(&roots, &catalogue, &title, now_ms()) })
        .await
        .map_err(|error| AgentError::Io(chain(&error)))?;
    cx.update(|cx| Library::global(cx).update(cx, Library::reload));
    reload_library(cx).await;
    let patch = MetaPatch {
        language: Some(args.language),
        ..MetaPatch::default()
    };
    run(key, Op::SetMeta(patch), cx).await?;
    run(key, Op::Save, cx).await?;
    Ok(Answer::of(&Created {
        book: key.to_string(),
        warnings: Vec::new(),
    }))
}

/// Waits until the library has read its rows again.
async fn reload_library(cx: &mut AsyncApp) {
    loop {
        let loading = cx.update(|cx| Library::global(cx).read(cx).is_loading());
        if !loading {
            return;
        }
        cx.background_executor()
            .timer(std::time::Duration::from_millis(20))
            .await;
    }
}

/// Imports a file of this computer.
async fn import_file(
    args: FileArgs,
    client: &Caller,
    cx: &mut AsyncApp,
) -> Result<Answer, AgentError> {
    if !args.path.is_absolute() {
        return Err(AgentError::InvalidArguments(format!(
            "{} is not an absolute path",
            args.path.display()
        )));
    }
    let question = t!(
        "agent.confirm.import",
        path = args.path.display().to_string()
    );
    confirm(client, Action::Import, question.into_owned(), cx).await?;
    let (roots, catalogue) = cx.update(|cx| {
        let store = Store::global(cx);
        (store.roots.clone(), store.catalogue.clone())
    });
    let path = args.path.clone();
    let imported = cx
        .background_spawn(async move { import(&roots, &catalogue, &path, now_ms(), &|_| {}).await })
        .await
        .map_err(|error| AgentError::Io(chain(&error)))?;
    cx.update(|cx| Library::global(cx).update(cx, Library::reload));
    reload_library(cx).await;
    Ok(Answer::of(&Created {
        book: imported.key.to_string(),
        warnings: imported.warnings,
    }))
}

/// Writes the saved book in another format.
async fn export(
    args: ExportBook,
    client: &Caller,
    cx: &mut AsyncApp,
) -> Result<Answer, AgentError> {
    let key = key(&args.book, cx)?;
    let format = Format::from_str(&args.format)
        .map_err(|_| AgentError::InvalidArguments(format!("no format {:?}", args.format)))?;
    if !args.path.is_absolute() {
        return Err(AgentError::InvalidArguments(format!(
            "{} is not an absolute path",
            args.path.display()
        )));
    }
    let destination = args.path.clone();
    let exists = cx
        .background_spawn(async move { destination.exists() })
        .await;
    let shown = args.path.display().to_string();
    let (action, question) = if exists {
        (
            Action::Overwrite,
            t!("agent.confirm.overwrite", path = shown),
        )
    } else {
        (Action::Export, t!("agent.confirm.export", path = shown))
    };
    confirm(client, action, question.into_owned(), cx).await?;
    let source = cx.update(|cx| Store::global(cx).roots.book(key));
    let destination = args.path.clone();
    let bytes = compute::spawn(move || {
        export_to(&source, &destination, format, exists)
            .map_err(|error| AgentError::Io(chain(&error)))?;
        std::fs::metadata(&destination)
            .map(|metadata| metadata.len())
            .map_err(|error| AgentError::Io(error.to_string()))
    })
    .await?;
    Ok(Answer::of(&Written {
        path: args.path.display().to_string(),
        bytes,
    }))
}

/// Removes a book from the library.
async fn delete(book: &str, client: &Caller, cx: &mut AsyncApp) -> Result<Answer, AgentError> {
    let key = key(book, cx)?;
    if cx.update(|cx| Sessions::in_editor(key, cx)) {
        return Err(AgentError::InvalidArguments(
            "the book is open in the editor of the application".into(),
        ));
    }
    confirm(
        client,
        Action::DeleteBook,
        t!("agent.confirm.delete", book = title(key, cx)).into_owned(),
        cx,
    )
    .await?;
    cx.update(|cx| {
        Sessions::forget(key, cx);
        Library::global(cx).update(cx, |library, cx| library.remove(key, cx));
    });
    Ok(Answer::of(&Done {}))
}

/// Opens a book in the window.
fn show(args: &Show, cx: &mut AsyncApp) -> Result<Answer, AgentError> {
    let key = key(&args.book, cx)?;
    let edit = match args.mode.as_deref() {
        None | Some("read") => false,
        Some("edit") => true,
        Some(other) => {
            return Err(AgentError::InvalidArguments(format!(
                "no mode {other:?}; it is `read` or `edit`"
            )));
        }
    };
    let chapter = args
        .href
        .as_deref()
        .map(|href| href.split('#').next().unwrap_or(href).to_owned());
    let Some(main) = cx.update(|cx| cx.try_global::<MainWindow>().cloned()) else {
        return Err(AgentError::OpenFailed(
            "the application has no window".into(),
        ));
    };
    main.handle
        .update(cx, |_, window, cx| {
            let Some(root) = main
                .shell
                .as_ref()
                .and_then(|shell| shell.read(cx).root().cloned())
            else {
                return;
            };
            root.update(cx, |root, cx| root.show(key, chapter, edit, window, cx));
            window.activate_window();
        })
        .map_err(|_| AgentError::OpenFailed("the window is gone".into()))?;
    Ok(Answer::of(&Done {}))
}

/// Opens the dialog an agent's request to add a picture from a file opens,
/// for the pictures of the visual test.
#[cfg(feature = "visual")]
pub(crate) fn show_confirmation(cx: &mut App) {
    cx.spawn(async move |cx| {
        drop(
            confirm(
                &Caller {
                    name: "Claude Code".into(),
                    session: 0,
                },
                Action::ReadPicture,
                t!(
                    "agent.confirm.read_picture",
                    path = "/Users/me/Pictures/pond.jpg",
                    book = "Dune"
                )
                .into_owned(),
                cx,
            )
            .await,
        );
    })
    .detach();
}

/// Asks the person in a dialog whether `client` may do what `question`
/// says, unless they allowed this kind of action for it already. A box under
/// the question allows the kind until the application closes. A dialog
/// closed without an answer is a refusal.
async fn confirm(
    client: &Caller,
    action: Action,
    question: String,
    cx: &mut AsyncApp,
) -> Result<(), AgentError> {
    if cx.update(|cx| Allowed::has(client, action, cx)) {
        return Ok(());
    }
    let Some(window) = cx.update(|cx| cx.try_global::<MainWindow>().cloned()) else {
        return Err(AgentError::Declined);
    };
    let (answer, answered) = oneshot::channel::<bool>();
    let answer = Rc::new(RefCell::new(Some(answer)));
    let always = Rc::new(Cell::new(false));
    let title = t!("agent.confirm.title", client = client.name).into_owned();
    let remembered = action.always();
    let who = client.clone();
    let opened = window.handle.update(cx, move |_, window, cx| {
        window.activate_window();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (allow, decline) = (Rc::clone(&answer), Rc::clone(&answer));
            let (checked, toggled, kept) = (always.get(), Rc::clone(&always), Rc::clone(&always));
            let who = who.clone();
            alert
                .title(title.clone())
                .description(question.clone())
                .footer(
                    v_flex()
                        .w_full()
                        .gap_3()
                        .child(
                            Checkbox::new("agent-always")
                                .label(SharedString::from(remembered.clone()))
                                .checked(checked)
                                .on_click(move |checked, window, _| {
                                    toggled.set(*checked);
                                    window.refresh();
                                }),
                        )
                        .child(
                            h_flex()
                                .w_full()
                                .justify_end()
                                .gap_2()
                                .child(
                                    Button::new("agent-decline")
                                        .label(t!("agent.confirm.decline"))
                                        .min_h(px(target_size()))
                                        .on_click(move |_, window, cx| {
                                            if let Some(answer) = decline.borrow_mut().take() {
                                                answer.send(false).ok();
                                            }
                                            window.close_dialog(cx);
                                        }),
                                )
                                .child(
                                    Button::new("agent-allow")
                                        .primary()
                                        .label(t!("agent.confirm.allow"))
                                        .min_h(px(target_size()))
                                        .on_click(move |_, window, cx| {
                                            if kept.get() {
                                                Allowed::remember(&who, action, cx);
                                            }
                                            if let Some(answer) = allow.borrow_mut().take() {
                                                answer.send(true).ok();
                                            }
                                            window.close_dialog(cx);
                                        }),
                                ),
                        ),
                )
        });
    });
    if opened.is_err() {
        return Err(AgentError::Declined);
    }
    if answered.await == Ok(true) {
        Ok(())
    } else {
        Err(AgentError::Declined)
    }
}

#[cfg(test)]
mod tests;
