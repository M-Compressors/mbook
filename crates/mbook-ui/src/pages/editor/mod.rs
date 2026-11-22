//! The editor: the metadata, chapters, landmarks and pictures of a book.
//!
//! The whole `mbook::Book` is owned by one background task, the actor of
//! [`crate::features::edit_book`]. The page holds an
//! [`Outline`](crate::features::edit_book::outline::Outline) of the book,
//! sends commands and shows what comes back; it never holds a `Book`
//! (mbook-ui docs, "Editing"). The state that has no window in it is
//! [`session::Session`], and the views render it:
//!
//! - `view` composes the page for each size class,
//! - `sections` is the list of sections and chapters,
//! - `details`, `chapter`, `contents`, `landmarks`, `panels` are the sections,
//! - `preview` makes the reader's chapter view that shows the text being
//!   edited,
//! - `commands`, `files` and `prompts` are what the controls do,
//! - `link_prompt`, `picture_prompt`, `entry_prompt` and `place_picker` are
//!   the dialogs that put a link or a picture into a chapter, change an entry
//!   of the contents and choose a place of the book,
//! - `markup` writes the Markdown the editor inserts itself.

mod chapter;
mod commands;
mod contents;
mod details;
mod encode;
mod entry_prompt;
mod figures;
mod files;
mod keys;
mod landmarks;
mod link_prompt;
mod markup;
mod panels;
mod picture_prompt;
mod place_picker;
mod preview;
mod prompts;
mod search;
mod sections;
mod session;
#[cfg(test)]
mod tests;
mod view;
mod visual;
mod visual_formatting;
mod visual_pictures;
mod visual_source;

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use gpui_kit::component::input::{EditorState, InputEvent};
use gpui_kit::{
    App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable, RenderImage,
    Subscription, Task, Window,
};

use crate::entities::catalogue::Store;
use crate::features::appearance::Appearance;
use crate::features::edit_book::actor::{self, EditHandle};
use crate::features::edit_book::command::Command;
use crate::shared::model::BookKey;
use crate::shared::ui::search_bar::SearchBar;
use crate::widgets::chapter_view::ChapterView;

use self::details::Fields;
use self::encode::{Job, Target};
use self::search::ChapterMatch;
use self::session::{MetaField, Session, ViewMode};
use self::visual::VisualEditor;

/// What the editor asks the root to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorEvent {
    /// Go back to the previous screen.
    Close,
    /// The book was saved over its file in the library: the library reads its
    /// row again.
    Saved(BookKey),
}

/// The cover as the page shows it.
#[derive(Default)]
struct CoverView {
    /// The path of the picture the image is of.
    path: Option<String>,
    /// The picture decoded for the screen.
    image: Option<Arc<RenderImage>>,
    /// The picture could not be decoded.
    failed: bool,
    /// The request the answer must belong to.
    generation: u64,
    /// The decoding in flight.
    task: Option<Task<()>>,
}

/// The pictures that are being turned into AVIF, one at a time.
#[derive(Default)]
struct Encoder {
    /// Files waiting for their output settings.
    queue: VecDeque<(std::path::PathBuf, Target)>,
    /// The job in flight and the task that does it.
    running: Option<(Job, Task<()>)>,
    /// A dialog asks for the settings of one picture.
    prompting: bool,
    /// The encoded picture has been sent to the book.
    waiting_for_book: bool,
    /// The chapter the picture that was sent goes into, and its Markdown.
    placing: Option<(String, String)>,
}

impl Encoder {
    /// The names of the files that are being encoded or wait for it.
    fn names(&self, target: Target) -> Vec<String> {
        self.running
            .iter()
            .map(|(job, _)| job)
            .filter(|job| job.target == target)
            .map(Job::name)
            .collect()
    }
}

/// Where a save is.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Saving {
    /// Save was asked for and waits for the pictures that are encoded.
    requested: bool,
    /// The save is on its way.
    running: bool,
    /// Leave the editor when the save has worked.
    leave_after: bool,
}

impl Saving {
    /// Whether the header says the book is being saved.
    const fn active(self) -> bool {
        self.requested || self.running
    }
}

/// The page that edits one book.
pub(crate) struct EditorPage {
    key: BookKey,
    focus_handle: FocusHandle,
    handle: EditHandle,
    /// The task that owns the book; dropping the page ends it.
    _actor: Task<()>,
    session: Session,
    /// The number of the last command sent.
    sent: u64,
    /// The commands whose answer has not come.
    requests: HashMap<u64, Task<()>>,
    /// The Markdown editor, which holds the text of the open chapter.
    text: Entity<EditorState>,
    visual: VisualEditor,
    search: SearchBar<ChapterMatch>,
    search_button: FocusHandle,
    /// The preview of the text of the editor.
    preview: Entity<ChapterView>,
    /// The preview request for the current chapter text.
    preview_figures: Option<Task<()>>,
    /// Loads the source for the current figure dialog.
    figure_edit: Option<Task<()>>,
    /// The chapter whose text is in the editor.
    loaded: Option<String>,
    /// The request the text that is loading must belong to.
    load_generation: u64,
    load_task: Option<Task<()>>,
    /// Escape was pressed in the editor: Tab leaves it.
    escaped: bool,
    /// The pause after typing, at the end of which the text is sent.
    flush_timer: Option<Task<()>>,
    fields: Fields,
    meta_timers: HashMap<MetaField, Task<()>>,
    cover: CoverView,
    encoder: Encoder,
    saving: Saving,
    /// The texts that give the problems their places.
    places_task: Option<Task<()>>,
    /// The files that a prompt of the platform is choosing.
    chooser: Option<Task<()>>,
    /// The caret goes here when the text that is loading has arrived.
    caret: Option<usize>,
    /// The chapter that is loading was just added: when its text has
    /// arrived, the editor takes the focus with the caret at its end.
    write_next: bool,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<EditorEvent> for EditorPage {}

impl EditorPage {
    /// An editor for the book `key` of the library.
    ///
    /// The book is opened on the background; until its outline arrives the
    /// page says that it is opening.
    pub(crate) fn new(key: BookKey, window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
        let path = Store::global(cx).roots.book(key);
        let (handle, serving) = actor::session(path);
        let actor = cx.background_spawn(serving);
        let text = cx.new(|cx| {
            let mut state = EditorState::new(window, cx)
                .language("markdown")
                .folding(false)
                .line_number(false)
                .soft_wrap(true);
            state.set_disabled(true, cx);
            state
        });
        let visual = VisualEditor::new(window, cx);
        // Our Escape + Tab bindings take precedence over the block editor's Tab.
        keys::bind(cx);
        let fields = Fields::new(window, cx);
        let search = SearchBar::new("Find in chapter", window, cx);
        let mut subscriptions = vec![
            cx.subscribe(&search.input, Self::on_search_input),
            cx.subscribe_in(&text, window, |this, _, event: &InputEvent, window, cx| {
                this.on_text_event(event, window, cx);
            }),
            cx.subscribe_in(
                &visual.input,
                window,
                |this, _, event: &markdown_editor::BlockEditorEvent, window, cx| {
                    this.on_visual_event(event, window, cx);
                },
            ),
            cx.observe(&Appearance::global(cx), |_, _, cx| cx.notify()),
        ];
        subscriptions.extend(fields.subscribe(window, cx));
        let mut page = Self {
            key,
            focus_handle: cx.focus_handle(),
            handle,
            _actor: actor,
            session: Session::new(),
            sent: 0,
            requests: HashMap::new(),
            text,
            visual,
            search,
            search_button: cx.focus_handle(),
            preview: preview::new_preview(window, cx),
            preview_figures: None,
            figure_edit: None,
            loaded: None,
            load_generation: 0,
            load_task: None,
            escaped: false,
            flush_timer: None,
            fields,
            meta_timers: HashMap::new(),
            cover: CoverView::default(),
            encoder: Encoder::default(),
            saving: Saving::default(),
            places_task: None,
            chooser: None,
            caret: None,
            write_next: false,
            _subscriptions: subscriptions,
        };
        page.send(Command::Load, commands::Origin::Load, window, cx);
        page
    }

    /// The focus handle of the Markdown editor.
    fn text_focus(&self, cx: &App) -> FocusHandle {
        match self.session.mode {
            ViewMode::Visual => self.visual.input.focus_handle(cx),
            _ => self.text.focus_handle(cx),
        }
    }
}

impl Focusable for EditorPage {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
