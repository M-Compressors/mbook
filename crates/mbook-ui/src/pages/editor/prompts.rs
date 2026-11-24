//! Dialogs: asking for a path, and asking before something cannot be undone.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    AppContext as _, Context, Entity, Focusable as _, ParentElement as _, SharedString,
    Styled as _, Window, div, px,
};

use rust_i18n::t;

use super::{EditorEvent, EditorPage};
use crate::shared::ui::{status_line, target_size};

/// What a dialog that asks for one line of words says.
#[derive(Debug, Clone)]
pub(super) struct PathRequest {
    /// The title of the dialog.
    pub(super) title: SharedString,
    /// The label of the field, which is also its name.
    pub(super) label: SharedString,
    /// What the field holds when the dialog opens.
    pub(super) initial: String,
    /// The name of the button that goes on.
    pub(super) ok: SharedString,
    /// What the dialog says when the field is empty; `None` lets it be
    /// empty.
    pub(super) required: Option<SharedString>,
}

/// Asks for one line in a dialog. `on_ok` gets what was entered, without its
/// edges; `on_cancel` is called when the dialog is dismissed. A field that
/// must not be empty is refused in the dialog, which stays open and says
/// what to enter.
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
    let (title, label, ok) = (
        request.title.clone(),
        request.label.clone(),
        request.ok.clone(),
    );
    let missing = request.required.clone();
    window.open_alert_dialog(cx, move |alert, window, cx| {
        if first.replace(false) {
            input.focus_handle(cx).focus(window, cx);
        }
        let message = error.read(cx).clone();
        let danger = cx.theme().danger;
        let field = v_flex()
            .gap_2()
            .child(div().child(label.clone()))
            .child(Input::new(&input).aria_label(label.clone()))
            .children(message.map(|message| status_line("path-error", message).text_color(danger)));
        let (page_ok, page_cancel) = (page.clone(), page.clone());
        let (input, error) = (input.clone(), error.clone());
        let (on_ok, on_cancel) = (on_ok.clone(), on_cancel.clone());
        let missing = missing.clone();
        alert
            .title(title.clone())
            .description(field)
            .confirm()
            .ok_text(ok.clone())
            .cancel_text(t!("prompt.cancel"))
            .on_ok(
                move |_, window, cx| match entered(&input.read(cx).value(), missing.as_ref()) {
                    Ok(path) => {
                        page_ok
                            .update(cx, |this, cx| on_ok(this, path, window, cx))
                            .ok();
                        true
                    }
                    Err(words) => {
                        error.update(cx, |error, cx| {
                            *error = Some(words);
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

/// What was entered, without its edges, or the words that say what to
/// enter when the field must not be empty and is.
fn entered(value: &str, missing: Option<&SharedString>) -> Result<String, SharedString> {
    let value = value.trim();
    match missing {
        Some(missing) if value.is_empty() => Err(missing.clone()),
        _ => Ok(value.to_owned()),
    }
}

/// Asks whether to save, discard or keep editing, because the book has
/// changes that were not saved.
pub(super) fn ask_leave(window: &mut Window, cx: &mut Context<'_, EditorPage>) {
    let page = cx.weak_entity();
    window.open_alert_dialog(cx, move |alert, _, _| {
        let (discard, save) = (page.clone(), page.clone());
        alert
            .title(t!("prompt.leave.title"))
            .description(t!("prompt.leave.detail"))
            .footer(
                h_flex()
                    .w_full()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("leave-cancel")
                            .label(t!("prompt.cancel"))
                            .min_h(px(target_size()))
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("leave-discard")
                            .label(t!("prompt.leave.discard"))
                            .min_h(px(target_size()))
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                                discard.update(cx, |_, cx| cx.emit(EditorEvent::Close)).ok();
                            }),
                    )
                    .child(
                        Button::new("leave-save")
                            .primary()
                            .label(t!("prompt.leave.save"))
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

/// What a dialog asks before a change that cannot be undone.
#[derive(Debug, Clone)]
pub(super) struct Question {
    /// The question, as the title of the dialog.
    pub(super) title: String,
    /// What the change does.
    pub(super) detail: String,
    /// The name of the button that goes on.
    pub(super) ok: SharedString,
}

/// Asks before a change that cannot be undone; `act` makes the change.
pub(super) fn confirm(
    question: Question,
    window: &mut Window,
    cx: &mut Context<'_, EditorPage>,
    act: impl Fn(&mut EditorPage, &mut Window, &mut Context<'_, EditorPage>) + 'static,
) {
    let page = cx.weak_entity();
    let act = Rc::new(act);
    window.open_alert_dialog(cx, move |alert, _, _| {
        let (page, act) = (page.clone(), act.clone());
        alert
            .title(question.title.clone())
            .description(question.detail.clone())
            .confirm()
            .ok_text(question.ok.clone())
            .ok_variant(ButtonVariant::Danger)
            .cancel_text(t!("prompt.cancel"))
            .on_ok(move |_, window, cx| {
                page.update(cx, |this, cx| act(this, window, cx)).ok();
                true
            })
    });
}

/// Asks before something is removed, because it cannot be undone.
pub(super) fn confirm_remove(
    what: &str,
    window: &mut Window,
    cx: &mut Context<'_, EditorPage>,
    remove: impl Fn(&mut EditorPage, &mut Window, &mut Context<'_, EditorPage>) + 'static,
) {
    confirm(
        Question {
            title: t!("prompt.remove.title", what = what).into_owned(),
            detail: t!("prompt.remove.detail").into_owned(),
            ok: t!("prompt.remove.ok").into(),
        },
        window,
        cx,
        remove,
    );
}

#[cfg(test)]
mod tests {
    use gpui_kit::SharedString;

    use super::entered;

    #[test]
    fn what_is_entered_loses_its_edges() {
        assert_eq!(entered("  ch01.md \n", None), Ok("ch01.md".to_owned()));
    }

    #[test]
    fn an_empty_field_may_be_left_empty_unless_it_is_required() {
        let missing = SharedString::from("Enter a name.");
        assert_eq!(entered("  ", None), Ok(String::new()));
        assert_eq!(entered("  ", Some(&missing)), Err(missing.clone()));
        assert_eq!(entered(" a ", Some(&missing)), Ok("a".to_owned()));
    }
}
