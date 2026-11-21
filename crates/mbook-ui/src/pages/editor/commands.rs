//! What the controls do: send commands to the task that owns the book, and
//! put its answers where they belong.

use std::time::Duration;

use gpui_kit::component::RopeExt as _;
use gpui_kit::component::input::InputEvent;
use gpui_kit::{Context, Window};

use super::session::{
    Control, MetaField, Phase, Section, edit_error_words, line_and_column, meta_change,
    unsaved_announcement,
};
use super::{EditorEvent, EditorPage};
use crate::features::edit_book::command::Command;
use crate::features::edit_book::error::EditError;
use crate::features::edit_book::outline::Outline;
use crate::shared::ui::announce;

/// How long typing must pause before the text is sent.
pub(super) const TYPING_PAUSE: Duration = Duration::from_millis(400);
/// How long a field of the Details form waits before it is sent.
pub(super) const FIELD_PAUSE: Duration = Duration::from_millis(800);

/// What a chapter command was for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ChapterOp {
    /// A chapter was added and is to be opened.
    Add(String),
    /// A chapter moved in the reading order.
    Move,
    /// A chapter got another path.
    Rename {
        /// The path it had.
        from: String,
        /// The path it has.
        to: String,
    },
    /// A chapter was removed.
    Remove(String),
}

/// What a command was for, so that its answer is put where it belongs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Origin {
    /// The first command of the session.
    Load,
    /// A field of the Details form.
    Meta(MetaField),
    /// The text of a chapter.
    Text {
        /// The chapter.
        path: String,
        /// The text that was sent.
        text: String,
    },
    /// A change of the list of chapters.
    Chapter(ChapterOp),
    /// A picture was added.
    AddPicture,
    /// A picture was removed.
    RemovePicture,
    /// The cover was set.
    SetCover,
    /// The contents were rebuilt.
    Contents,
    /// A check.
    Check,
    /// A save.
    Save,
}

impl EditorPage {
    /// Sends a command. Commands are applied in the order they were sent, so
    /// a save always includes what was sent before it.
    pub(super) fn send(
        &mut self,
        command: Command,
        origin: Origin,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.sent += 1;
        let seq = self.sent;
        let reply = self.handle.send(command);
        let task = cx.spawn_in(window, async move |this, cx| {
            let result = reply.await;
            this.update_in(cx, |this, window, cx| {
                this.requests.remove(&seq);
                this.answered(seq, origin, result, window, cx);
            })
            .ok();
        });
        self.requests.insert(seq, task);
    }

    /// Puts the answer to a command where it belongs.
    fn answered(
        &mut self,
        seq: u64,
        origin: Origin,
        result: Result<Outline, EditError>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match (origin, result) {
            (Origin::Load, Err(error)) => {
                self.session.fail(edit_error_words(&error));
                cx.notify();
            }
            (Origin::Load | Origin::Check, Ok(outline)) => self.take(seq, outline, window, cx),
            (Origin::Meta(field), Ok(outline)) => {
                self.session.clear_error(&Control::Meta(field));
                self.take(seq, outline, window, cx);
            }
            (Origin::Meta(field), Err(error)) => {
                self.fail(Control::Meta(field), &error, cx);
            }
            (Origin::Text { path, .. }, Ok(outline)) => {
                self.session.text_accepted(&path);
                self.take(seq, outline, window, cx);
            }
            (Origin::Text { path, text }, Err(error)) => {
                self.text_refused(&path, text, &error, window, cx);
            }
            (Origin::Chapter(op), Ok(outline)) => self.chapter_done(seq, &op, outline, window, cx),
            (Origin::Chapter(_), Err(error)) => self.fail(Control::Chapters, &error, cx),
            (Origin::AddPicture | Origin::RemovePicture, Ok(outline)) => {
                self.session.clear_error(&Control::Pictures);
                self.take(seq, outline, window, cx);
            }
            (Origin::AddPicture | Origin::RemovePicture, Err(error)) => {
                self.fail(Control::Pictures, &error, cx);
            }
            (Origin::SetCover, Ok(outline)) => {
                self.session.clear_error(&Control::Cover);
                self.take(seq, outline, window, cx);
            }
            (Origin::SetCover, Err(error)) => self.fail(Control::Cover, &error, cx),
            (Origin::Contents, Ok(outline)) => {
                self.session.clear_error(&Control::Contents);
                self.take(seq, outline, window, cx);
            }
            (Origin::Contents, Err(error)) => self.fail(Control::Contents, &error, cx),
            (Origin::Check, Err(error)) => self.fail(Control::Save, &error, cx),
            (Origin::Save, result) => self.saved(seq, result, window, cx),
        }
    }

    /// Shows what went wrong at a control.
    fn fail(&mut self, control: Control, error: &EditError, cx: &mut Context<'_, Self>) {
        self.session.set_error(control, edit_error_words(error));
        cx.notify();
    }

    /// Takes the outline the book answered with.
    pub(super) fn take(
        &mut self,
        seq: u64,
        outline: Outline,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let applied = self.session.apply_outline(seq, outline);
        if !applied.taken {
            return;
        }
        if applied.first {
            self.fill_fields(window, cx);
        }
        if applied.open_changed {
            self.loaded = None;
        }
        self.load_open_text(window, cx);
        self.refresh_cover(cx);
        self.refresh_places(cx);
        cx.notify();
    }

    /// The answer to a command on the list of chapters.
    fn chapter_done(
        &mut self,
        seq: u64,
        op: &ChapterOp,
        outline: Outline,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.session.clear_error(&Control::Chapters);
        match op {
            ChapterOp::Rename { from, to } => {
                self.session.rename(from, to);
                if self.loaded.as_deref() == Some(from) {
                    self.loaded = Some(to.clone());
                }
            }
            ChapterOp::Remove(path) => {
                self.session.forget(path);
                if self.loaded.as_deref() == Some(path) {
                    self.loaded = None;
                }
            }
            ChapterOp::Add(_) | ChapterOp::Move => {}
        }
        self.take(seq, outline, window, cx);
        if let ChapterOp::Add(path) = op {
            self.open_chapter(path, window, cx);
        }
    }

    /// The book refused the text of a chapter: it is kept, the place is told
    /// and the caret goes there.
    fn text_refused(
        &mut self,
        path: &str,
        text: String,
        error: &EditError,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let mut message = edit_error_words(error);
        let offset = error.text_offset();
        if let Some(offset) = offset {
            let (line, column) = line_and_column(&text, offset);
            message = format!("{message} It is at line {line}, column {column}.");
        }
        self.session.text_refused(path, text, message);
        if let Some(offset) = offset
            && self.loaded.as_deref() == Some(path)
        {
            self.move_caret(offset, window, cx);
        }
        cx.notify();
    }

    /// Puts the caret of the Markdown editor at a byte offset.
    pub(super) fn move_caret(
        &mut self,
        offset: usize,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.text.update(cx, |state, cx| {
            let at = offset.min(state.text().len());
            let position = state.text().offset_to_position(at);
            state.set_cursor_position(position, window, cx);
        });
    }

    /// What the Markdown editor reports.
    pub(super) fn on_text_event(
        &mut self,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match event {
            InputEvent::Change => self.text_typed(window, cx),
            InputEvent::Focus | InputEvent::Blur => {
                if self.escaped {
                    self.escaped = false;
                    cx.notify();
                }
            }
            InputEvent::PressEnter { .. } => {}
        }
    }

    /// The text of the open chapter was typed into: the preview follows at
    /// once, and the book is sent the text when typing pauses.
    fn text_typed(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(path) = self.loaded.clone() else {
            return;
        };
        self.session.text_typed(&path);
        let text = self.text.read(cx).value();
        self.preview.update(cx, |view, cx| view.set_text(&text, cx));
        self.escaped = false;
        self.flush_timer = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(TYPING_PAUSE).await;
            this.update_in(cx, EditorPage::flush_text).ok();
        }));
        cx.notify();
    }

    /// Sends the text that was typed and not sent.
    pub(super) fn flush_text(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.flush_timer = None;
        let Some(path) = self.session.take_pending() else {
            return;
        };
        if self.loaded.as_deref() != Some(path.as_str()) {
            return;
        }
        let text = self.text.read(cx).value().to_string();
        self.send(
            Command::SetChapterText {
                path: path.clone(),
                text: text.clone(),
            },
            Origin::Text { path, text },
            window,
            cx,
        );
    }

    /// Opens a chapter in the editor, after sending the text of the one that
    /// was open.
    pub(super) fn open_chapter(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.flush_text(window, cx);
        self.session.show_chapter(path);
        self.load_open_text(window, cx);
        cx.notify();
    }

    /// Loads the text of the open chapter into the editor, unless it is there.
    pub(super) fn load_open_text(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(path) = self.session.open.clone() else {
            self.loaded = None;
            self.load_task = None;
            self.set_editor_text("", false, window, cx);
            return;
        };
        if self.loaded.as_deref() == Some(path.as_str()) {
            return;
        }
        self.load_generation += 1;
        let generation = self.load_generation;
        self.loaded = None;
        self.text
            .update(cx, |state, cx| state.set_disabled(true, cx));
        self.visual_loading(cx);
        if let Some(draft) = self.session.draft(&path).map(str::to_owned) {
            self.set_editor_text(&draft, true, window, cx);
            self.loaded = Some(path);
            self.text_typed(window, cx);
            return;
        }
        let reply = self.handle.chapter_text(&path);
        self.load_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = reply.await;
            this.update_in(cx, |this, window, cx| {
                this.text_loaded(generation, &path, result, window, cx);
            })
            .ok();
        }));
    }

    /// The text of a chapter arrived.
    fn text_loaded(
        &mut self,
        generation: u64,
        path: &str,
        result: Result<String, EditError>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if generation != self.load_generation {
            return;
        }
        match result {
            Ok(text) => {
                self.set_editor_text(&text, true, window, cx);
                self.loaded = Some(path.to_owned());
                if let Some(offset) = self.caret.take() {
                    self.move_caret(offset, window, cx);
                }
            }
            Err(error) => self.fail(Control::ChapterText, &error, cx),
        }
        cx.notify();
    }

    /// Replaces the text of the editor without counting it as typing.
    fn set_editor_text(
        &mut self,
        text: &str,
        editable: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.text.update(cx, |state, cx| {
            state.set_value(text, window, cx);
            state.set_disabled(!editable, cx);
        });
        self.visual_loaded(editable, window, cx);
        self.preview.update(cx, |view, cx| view.set_text(text, cx));
    }

    /// Moves the chapter `path` one place up or down.
    pub(super) fn move_chapter(
        &mut self,
        path: &str,
        step: isize,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(index) = self.session.chapter_index(path) else {
            return;
        };
        let Some(target) = index.checked_add_signed(step) else {
            return;
        };
        if target >= self.session.chapters().len() {
            return;
        }
        self.flush_text(window, cx);
        self.send(
            Command::MoveChapter {
                path: path.to_owned(),
                index: target,
            },
            Origin::Chapter(ChapterOp::Move),
            window,
            cx,
        );
    }

    /// Sends the field of the Details form when it differs from the book.
    pub(super) fn commit_meta(
        &mut self,
        field: MetaField,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.meta_timers.remove(&field);
        let Some(meta) = self.session.meta() else {
            return;
        };
        let values = self.meta_values(cx);
        if let Some(change) = meta_change(field, &values, meta) {
            self.send(Command::SetMeta(change), Origin::Meta(field), window, cx);
        } else {
            self.session.clear_error(&Control::Meta(field));
            cx.notify();
        }
    }

    /// What a field of the Details form reports.
    pub(super) fn on_field_event(
        &mut self,
        field: MetaField,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match event {
            InputEvent::Change => {
                self.meta_timers.insert(
                    field,
                    cx.spawn_in(window, async move |this, cx| {
                        cx.background_executor().timer(FIELD_PAUSE).await;
                        this.update_in(cx, |this, window, cx| {
                            this.commit_meta(field, window, cx);
                        })
                        .ok();
                    }),
                );
            }
            InputEvent::Blur => self.commit_meta(field, window, cx),
            InputEvent::Focus | InputEvent::PressEnter { .. } => {}
        }
    }

    /// Runs the check; the problems come in the outline.
    pub(super) fn check(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.flush_text(window, cx);
        self.send(Command::Check, Origin::Check, window, cx);
    }

    /// Rebuilds the contents from the headings and checks the book again.
    pub(super) fn rebuild_contents(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.flush_text(window, cx);
        self.send(Command::ContentsFromHeadings, Origin::Contents, window, cx);
        self.send(Command::Check, Origin::Check, window, cx);
    }

    /// Asks to save: the text is sent, the pictures that are still being
    /// encoded are waited for, and then the book is saved.
    pub(super) fn save(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.session.phase != Phase::Ready || self.saving.active() {
            return;
        }
        self.saving.requested = true;
        self.session.clear_error(&Control::Save);
        self.continue_save(window, cx);
        cx.notify();
    }

    /// Goes on with a save that was asked for, when nothing is left to wait
    /// for.
    pub(super) fn continue_save(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if !self.saving.requested || self.session.encoding > 0 {
            return;
        }
        self.saving.requested = false;
        self.flush_text(window, cx);
        for field in MetaField::ALL {
            if self.meta_timers.contains_key(&field) {
                self.commit_meta(field, window, cx);
            }
        }
        if let Some(path) = self.session.first_refused().map(str::to_owned) {
            let message = format!(
                "The book was not saved: it refused the text of {path}. Correct it, then save."
            );
            self.session.set_error(Control::Save, message.clone());
            announce(message, window, cx);
            self.saving.leave_after = false;
            return;
        }
        self.saving.running = true;
        self.send(Command::Save, Origin::Save, window, cx);
    }

    /// The answer to a save.
    fn saved(
        &mut self,
        seq: u64,
        result: Result<Outline, EditError>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.saving.running = false;
        match result {
            Err(error) => {
                self.saving.leave_after = false;
                let message = edit_error_words(&error);
                self.session.set_error(Control::Save, message.clone());
                announce(message, window, cx);
            }
            Ok(outline) if !outline.problems.is_empty() => {
                self.saving.leave_after = false;
                let count = outline.problems.len();
                self.take(seq, outline, window, cx);
                self.session.show_section(Section::Problems);
                announce(unsaved_announcement(count), window, cx);
            }
            Ok(outline) => {
                self.take(seq, outline, window, cx);
                self.after_save(window, cx);
            }
        }
        cx.notify();
    }

    /// Asks to leave: with nothing unsaved the editor closes, otherwise a
    /// dialog asks what to do with the changes.
    pub(super) fn request_leave(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.flush_text(window, cx);
        if self.session.has_unsaved() {
            super::prompts::ask_leave(window, cx);
        } else {
            cx.emit(EditorEvent::Close);
        }
    }

    /// The header's Back control: a compact window steps back through its
    /// screens first.
    pub(super) fn back(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let compact = Self::class(window, cx) == crate::shared::layout::SizeClass::Compact;
        if compact && self.session.step_back() {
            cx.notify();
        } else {
            self.request_leave(window, cx);
        }
    }

    /// Saves and then leaves.
    pub(super) fn save_and_leave(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.saving.leave_after = true;
        self.save(window, cx);
    }
}
