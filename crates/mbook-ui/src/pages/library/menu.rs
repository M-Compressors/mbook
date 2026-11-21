//! The menu of a book, and the question that comes before removing one.

use gpui_kit::assets::IconName;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{App, WeakEntity, Window};

use super::{LibraryEvent, LibraryPage};
use crate::entities::catalogue::{Library, Store};
use crate::shared::model::BookKey;

/// The words of the entries of a book's menu, in the order they stand.
pub(super) const OPEN: &str = "Open";
/// Edit the book.
pub(super) const EDIT: &str = "Edit";
/// Show the file of the book in the file manager of the platform.
pub(super) const SHOW_FILE: &str = "Show the file";
/// Remove the book from the library.
pub(super) const REMOVE: &str = "Remove";

/// The question put before a book is removed.
pub(super) fn remove_question(title: &str) -> String {
    format!("Remove {title} from the library? The file you added it from is not touched.")
}

/// Fills `menu` with what can be done to the book `key`, called `title`:
/// open, edit, show the file, remove.
pub(super) fn book_menu(
    menu: PopupMenu,
    key: BookKey,
    title: &str,
    page: &WeakEntity<LibraryPage>,
) -> PopupMenu {
    let emit = |event: LibraryEvent| {
        let page = page.clone();
        move |_: &_, _: &mut Window, cx: &mut App| {
            page.update(cx, |_, cx| cx.emit(event)).ok();
        }
    };
    let title = title.to_owned();
    let page_for_remove = page.clone();
    menu.item(
        PopupMenuItem::new(OPEN)
            .icon(IconName::BookOpen)
            .on_click(emit(LibraryEvent::Open(key))),
    )
    .item(
        PopupMenuItem::new(EDIT)
            .icon(IconName::Pencil)
            .on_click(emit(LibraryEvent::Edit(key))),
    )
    .item(
        PopupMenuItem::new(SHOW_FILE)
            .icon(IconName::FolderOpen)
            .on_click(move |_, _, cx| {
                let path = Store::global(cx).roots.book(key);
                cx.reveal_path(&path);
            }),
    )
    .separator()
    .item(
        PopupMenuItem::new(REMOVE)
            .icon(IconName::Trash)
            .on_click(move |_, window, cx| {
                confirm_remove(key, &title, page_for_remove.clone(), window, cx);
            }),
    )
}

/// Asks whether to remove the book, and removes it when the answer is yes.
/// The focus returns to the page: the control that led here is gone with the
/// book.
pub(super) fn confirm_remove(
    key: BookKey,
    title: &str,
    page: WeakEntity<LibraryPage>,
    window: &mut Window,
    cx: &mut App,
) {
    let question = remove_question(title);
    window.open_alert_dialog(cx, move |alert, _, _| {
        let page = page.clone();
        alert
            .title("Remove this book?")
            .description(question.clone())
            .confirm()
            .ok_text(REMOVE)
            .ok_variant(ButtonVariant::Danger)
            .cancel_text("Cancel")
            .on_ok(move |_, window, cx| {
                Library::global(cx).update(cx, |library, cx| library.remove(key, cx));
                if let Some(page) = page.upgrade() {
                    let focus = page.read(cx).focus_handle.clone();
                    window.focus(&focus, cx);
                }
                true
            })
    });
}
