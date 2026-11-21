//! The library's New Book dialog and its background work.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, v_flex};
use gpui_kit::{AppContext as _, Context, Focusable as _, ParentElement as _, Styled as _, Window};

use super::{LibraryEvent, LibraryPage};
use crate::entities::catalogue::{Library, Store};
use crate::features::import_book::{create_book, error_chain, now_ms};
use crate::shared::ui::{announce, status_line};

impl LibraryPage {
    /// Asks for the name of a book before making it in the library.
    pub(super) fn ask_to_create(&self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.creating.is_some() {
            return;
        }
        let page = cx.weak_entity();
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Book title"));
        let first = Cell::new(true);
        let missing_title = Rc::new(Cell::new(false));
        window.open_alert_dialog(cx, move |alert, window, cx| {
            if first.replace(false) {
                input.focus_handle(cx).focus(window, cx);
            }
            let field = v_flex()
                .gap_2()
                .child(Input::new(&input).aria_label("Book title"))
                .children(missing_title.get().then(|| {
                    status_line("new-book-error", "Enter a title.").text_color(cx.theme().danger)
                }));
            let page = page.clone();
            let input = input.clone();
            let report_missing_title = missing_title.clone();
            alert
                .title("New book")
                .description(field)
                .confirm()
                .ok_text("Create")
                .cancel_text("Cancel")
                .on_ok(move |_, window, cx| {
                    let value = input.read(cx).value();
                    let title = value.trim();
                    if title.is_empty() {
                        report_missing_title.set(true);
                        input.focus_handle(cx).focus(window, cx);
                        window.refresh();
                        return false;
                    }
                    page.update(cx, |this, cx| this.create(title.to_owned(), window, cx))
                        .ok();
                    true
                })
        });
    }

    /// Makes and indexes a book before opening it in the editor.
    fn create(&mut self, title: String, window: &mut Window, cx: &mut Context<'_, Self>) {
        let store = Store::global(cx);
        let roots = store.roots.clone();
        let catalogue = store.catalogue.clone();
        let work =
            cx.background_spawn(async move { create_book(&roots, &catalogue, &title, now_ms()) });
        self.creating = Some(cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            this.update_in(cx, |this, window, cx| {
                this.creating = None;
                cx.notify();
                match result {
                    Ok(key) => {
                        Library::global(cx).update(cx, Library::reload);
                        cx.emit(LibraryEvent::Write(key));
                    }
                    Err(error) => {
                        let reason = error_chain(&error);
                        announce(
                            format!("The book could not be created. {reason}."),
                            window,
                            cx,
                        );
                    }
                }
            })
            .ok();
        }));
        cx.notify();
    }
}
