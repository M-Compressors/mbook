//! Dialogs: asking for a path, and asking before something cannot be undone.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    AppContext as _, Context, Entity, Focusable as _, ParentElement as _, SharedString,
    Styled as _, Window, div, px,
};

use super::session::clean_path;
use super::{EditorEvent, EditorPage};
use crate::shared::ui::{status_line, target_size};

/// What a dialog that asks for a path says.
#[derive(Debug, Clone)]
pub(super) struct PathRequest {
    /// The title of the dialog.
    pub(super) title: &'static str,
    /// The label of the field, which is also its name.
    pub(super) label: &'static str,
    /// What the field holds when the dialog opens.
    pub(super) initial: String,
    /// The name of the button that goes on.
    pub(super) ok: &'static str,
    /// Chapter names and link destinations to copy into the field.
    pub(super) chapters: Vec<(String, String)>,
}

/// Asks for a path in a dialog. `on_ok` gets the path that was entered;
/// `on_cancel` is called when the dialog is dismissed. An empty path is
/// refused in the dialog, which stays open and says what to enter.
pub(super) fn ask_path(
    request: &PathRequest,
    window: &mut Window,
    cx: &mut Context<'_, EditorPage>,
    on_ok: impl Fn(&mut EditorPage, String, &mut Window, &mut Context<'_, EditorPage>) + 'static,
    on_cancel: impl Fn(&mut EditorPage, &mut Window, &mut Context<'_, EditorPage>) + 'static,
) {
    let page = cx.weak_entity();
    let input = cx.new(|cx| InputState::new(window, cx));
    input.update(cx, |state, cx| {
        state.set_value(request.initial.as_str(), window, cx);
    });
    let error: Entity<Option<SharedString>> = cx.new(|_| None);
    let (on_ok, on_cancel) = (Rc::new(on_ok), Rc::new(on_cancel));
    let first = Cell::new(true);
    let (title, label, ok) = (request.title, request.label, request.ok);
    let chapters = Rc::new(request.chapters.clone());
    window.open_alert_dialog(cx, move |alert, window, cx| {
        if first.replace(false) {
            input.focus_handle(cx).focus(window, cx);
        }
        let message = error.read(cx).clone();
        let danger = cx.theme().danger;
        let field = v_flex()
            .gap_2()
            .children((!chapters.is_empty()).then(|| {
                let chapters = chapters.clone();
                let input = input.clone();
                Button::new("link-choose-chapter")
                    .label("Choose chapter")
                    .dropdown_caret(true)
                    .dropdown_menu(move |menu, _, _| {
                        chapters.iter().fold(menu, |menu, (label, destination)| {
                            let input = input.clone();
                            let destination = destination.clone();
                            menu.item(PopupMenuItem::new(label.clone()).on_click(
                                move |_, window, cx| {
                                    input.update(cx, |field, cx| {
                                        field.set_value(destination.clone(), window, cx);
                                    });
                                },
                            ))
                        })
                    })
            }))
            .child(div().child(label))
            .child(Input::new(&input).aria_label(label))
            .children(message.map(|message| status_line("path-error", message).text_color(danger)));
        let (page_ok, page_cancel) = (page.clone(), page.clone());
        let (input, error) = (input.clone(), error.clone());
        let (on_ok, on_cancel) = (on_ok.clone(), on_cancel.clone());
        alert
            .title(title)
            .description(field)
            .confirm()
            .ok_text(ok)
            .cancel_text("Cancel")
            .on_ok(
                move |_, window, cx| match clean_path(&input.read(cx).value()) {
                    Ok(path) => {
                        page_ok
                            .update(cx, |this, cx| on_ok(this, path, window, cx))
                            .ok();
                        true
                    }
                    Err(words) => {
                        error.update(cx, |error, cx| {
                            *error = Some(words.into());
                            cx.notify();
                        });
                        window.refresh();
                        false
                    }
                },
            )
            .on_cancel(move |_, window, cx| {
                page_cancel
                    .update(cx, |this, cx| on_cancel(this, window, cx))
                    .ok();
                true
            })
    });
}

/// Asks whether to save, discard or keep editing, because the book has
/// changes that were not saved.
pub(super) fn ask_leave(window: &mut Window, cx: &mut Context<'_, EditorPage>) {
    let page = cx.weak_entity();
    window.open_alert_dialog(cx, move |alert, _, _| {
        let (discard, save) = (page.clone(), page.clone());
        alert
            .title("Save the changes?")
            .description("The book has changes that were not saved.")
            .footer(
                h_flex()
                    .w_full()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("leave-cancel")
                            .label("Cancel")
                            .min_h(px(target_size()))
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("leave-discard")
                            .label("Discard")
                            .min_h(px(target_size()))
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                                discard.update(cx, |_, cx| cx.emit(EditorEvent::Close)).ok();
                            }),
                    )
                    .child(
                        Button::new("leave-save")
                            .primary()
                            .label("Save")
                            .min_h(px(target_size()))
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                                save.update(cx, |this, cx| this.save_and_leave(window, cx))
                                    .ok();
                            }),
                    ),
            )
    });
}

/// Asks before something is removed, because it cannot be undone.
pub(super) fn confirm_remove(
    what: &str,
    window: &mut Window,
    cx: &mut Context<'_, EditorPage>,
    remove: impl Fn(&mut EditorPage, &mut Window, &mut Context<'_, EditorPage>) + 'static,
) {
    let page = cx.weak_entity();
    let remove = Rc::new(remove);
    let title = format!("Remove {what}?");
    window.open_alert_dialog(cx, move |alert, _, _| {
        let (page, remove) = (page.clone(), remove.clone());
        alert
            .title(title.clone())
            .description("This cannot be undone.")
            .confirm()
            .ok_text("Remove")
            .ok_variant(ButtonVariant::Danger)
            .cancel_text("Cancel")
            .on_ok(move |_, window, cx| {
                page.update(cx, |this, cx| remove(this, window, cx)).ok();
                true
            })
    });
}
