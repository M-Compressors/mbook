//! The menu of a book, and the question that comes before removing one.

use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::ButtonVariant;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{App, Context, SharedString, WeakEntity, Window};
use mbook_converter::Format;
use rust_i18n::t;

use super::{LibraryEvent, LibraryPage};
use super::{export, share};
use crate::entities::catalogue::{Library, Store};
use crate::shared::model::BookKey;

/// Whether the platform has a file manager that can show a book of the
/// library.
const SHOWS_FILES: bool = !cfg!(target_os = "android");

/// The question put before a book is removed.
pub(super) fn remove_question(title: &str) -> String {
    t!("library.remove_question", title = title).into_owned()
}

/// Fills `menu` with what can be done to the book `key`, called `title`:
/// open, edit, show the file, export, share, remove.
pub(super) fn book_menu(
    menu: PopupMenu,
    key: BookKey,
    title: &str,
    page: &WeakEntity<LibraryPage>,
    window: &mut Window,
    cx: &mut Context<'_, PopupMenu>,
) -> PopupMenu {
    let emit = |event: LibraryEvent| {
        let page = page.clone();
        move |_: &_, _: &mut Window, cx: &mut App| {
            page.update(cx, |_, cx| cx.emit(event)).ok();
        }
    };
    let title = title.to_owned();
    let export_title = title.clone();
    let share_title = title.clone();
    let page_for_remove = page.clone();
    menu.item(
        PopupMenuItem::new(t!("library.open"))
            .icon(IconName::BookOpen)
            .on_click(emit(LibraryEvent::Open(key))),
    )
    .item(
        PopupMenuItem::new(t!("library.edit"))
            .icon(IconName::Pencil)
            .on_click(emit(LibraryEvent::Edit(key))),
    )
    // The library of Android is in the private storage of the application,
    // which no file manager shows; its file leaves through Share and Export.
    .when(SHOWS_FILES, |menu| {
        menu.item(
            PopupMenuItem::new(t!("library.show_file"))
                .icon(IconName::FolderOpen)
                .on_click(move |_, _, cx| {
                    let path = Store::global(cx).roots.book(key);
                    cx.reveal_path(&path);
                }),
        )
    })
    .submenu_with_icon(
        Some(Icon::new(IconName::Download)),
        t!("library.export"),
        window,
        cx,
        move |menu, _, _| {
            Format::ALL.iter().copied().fold(menu, |menu, format| {
                let title = export_title.clone();
                menu.item(PopupMenuItem::new(export::label(format)).on_click(
                    move |_, window, cx| {
                        #[cfg(not(target_os = "android"))]
                        export::choose(key, &title, format, window, cx, export::pick);
                        #[cfg(target_os = "android")]
                        export::share(key, &title, format, window, cx);
                    },
                ))
            })
        },
    )
    .item(
        PopupMenuItem::new(t!("library.share"))
            .icon(IconName::Share)
            .on_click(move |_, window, cx| {
                share::choose(key, &share_title, window, cx, share::present);
            }),
    )
    .separator()
    .item(
        PopupMenuItem::new(t!("library.remove"))
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
            .title(SharedString::from(t!("library.remove_title")))
            .description(question.clone())
            .confirm()
            .ok_text(t!("library.remove"))
            .ok_variant(ButtonVariant::Danger)
            .cancel_text(t!("library.cancel"))
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
