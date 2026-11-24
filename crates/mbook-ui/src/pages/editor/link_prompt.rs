//! The dialog of a link: its words when none are selected, and where it
//! leads, typed as an address or chosen as a place of the book.

use std::borrow::Cow;
use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, v_flex};
use gpui_kit::{
    App, AppContext as _, Context, Entity, Focusable as _, FontWeight, IntoElement,
    ParentElement as _, SharedString, Styled as _, Window, div, px,
};
use rust_i18n::t;

use super::EditorPage;
use super::markup::place_destination;
use super::place_picker::{ChapterChoice, OnPlace, Place, place_picker};
use crate::shared::ui::{status_line, target_size};

/// The label of the field that holds the words of a new link.
pub(super) fn text_label() -> Cow<'static, str> {
    t!("prompt.link.text_label")
}

/// The label of the field that holds where the link leads.
pub(super) fn address_label() -> Cow<'static, str> {
    t!("prompt.link.address_label")
}

/// What the dialog starts from.
pub(super) struct LinkRequest {
    /// Where the link at the caret leads; `None` for a new link.
    pub(super) current: Option<String>,
    /// Nothing is selected, so the dialog asks for the words of the link too.
    pub(super) ask_text: bool,
    /// The chapter being edited: places of the book are written as the way
    /// from it.
    pub(super) from: String,
    /// The chapters of the book and the places inside them.
    pub(super) choices: Rc<[ChapterChoice]>,
}

/// What the dialog was closed with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LinkAnswer {
    /// Make the link.
    Set {
        /// The words to insert and link; `None` links what is selected.
        text: Option<String>,
        /// Where it leads.
        destination: String,
    },
    /// Take the link away and keep its words.
    Remove,
}

/// The answer the fields give, or what is still to be entered.
///
/// # Errors
///
/// The words that say which field is empty.
pub(super) fn link_answer(
    text: Option<&str>,
    destination: &str,
) -> Result<LinkAnswer, Cow<'static, str>> {
    let text = match text.map(str::trim) {
        Some("") => return Err(t!("prompt.link.no_text")),
        text => text.map(|text| text.replace(['\n', '\r', '\t'], " ")),
    };
    let destination = destination.trim();
    if destination.is_empty() {
        return Err(t!("prompt.link.no_address"));
    }
    Ok(LinkAnswer::Set {
        text,
        destination: destination.to_owned(),
    })
}

/// What the dialog holds while it is open.
struct Fields {
    text: Entity<InputState>,
    address: Entity<InputState>,
    place: Entity<Place>,
    error: Entity<Option<SharedString>>,
    choices: Rc<[ChapterChoice]>,
    fill: OnPlace,
}

impl Fields {
    /// The form: the words of a new link, where it leads, the places of the
    /// book, what is still missing, and the control that removes the link.
    fn form(&self, ask_text: bool, remove: Option<Button>, cx: &App) -> impl IntoElement + use<> {
        let theme = cx.theme().clone();
        let label = |words: Cow<'static, str>| {
            div()
                .font_weight(FontWeight::MEDIUM)
                .child(SharedString::from(words))
        };
        v_flex()
            .gap_2()
            .children(ask_text.then(|| {
                v_flex()
                    .gap_1()
                    .child(label(text_label()))
                    .child(Input::new(&self.text).aria_label(text_label()))
            }))
            .child(
                v_flex()
                    .gap_1()
                    .child(label(address_label()))
                    .child(Input::new(&self.address).aria_label(address_label())),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(SharedString::from(t!("prompt.link.choose_hint"))),
                    )
                    .child(place_picker(
                        "link",
                        &self.place,
                        &self.choices,
                        Some(self.fill.clone()),
                        cx,
                    )),
            )
            .children(
                self.error
                    .read(cx)
                    .clone()
                    .map(|message| status_line("link-error", message).text_color(theme.danger)),
            )
            .children(remove.map(|remove| div().child(remove)))
    }
}

/// Asks where a link leads. `answer` gets what was entered; a dialog that is
/// dismissed calls nothing.
pub(super) fn ask_link(
    request: LinkRequest,
    window: &mut Window,
    cx: &mut Context<'_, EditorPage>,
    answer: impl Fn(&mut EditorPage, LinkAnswer, &mut Window, &mut Context<'_, EditorPage>) + 'static,
) {
    let page = cx.weak_entity();
    let text = cx.new(|cx| InputState::new(window, cx));
    let address = cx.new(|cx| InputState::new(window, cx).placeholder("https://"));
    if let Some(current) = &request.current {
        address.update(cx, |field, cx| {
            field.set_value(current.as_str(), window, cx);
        });
    }
    let place = cx.new(|_| Place::default());
    let error = cx.new(|_| None::<SharedString>);
    let answer = Rc::new(answer);
    let first = Cell::new(true);
    let LinkRequest {
        current,
        ask_text,
        from,
        choices,
    } = request;
    let editing = current.is_some();
    // Choosing a place writes the way to it into the address, which stays
    // editable.
    let fill: OnPlace = {
        let address = address.clone();
        Rc::new(move |place: &Place, window: &mut Window, cx: &mut App| {
            if let Some(chapter) = &place.chapter {
                let destination = place_destination(&from, chapter, place.anchor.as_deref());
                address.update(cx, |field, cx| field.set_value(destination, window, cx));
            }
        })
    };
    let fields = Fields {
        text: text.clone(),
        address: address.clone(),
        place,
        error: error.clone(),
        choices,
        fill,
    };
    window.open_alert_dialog(cx, move |alert, window, cx| {
        if first.replace(false) {
            let field = if ask_text { &text } else { &address };
            field.focus_handle(cx).focus(window, cx);
        }
        let remove = editing.then(|| {
            let (page, answer) = (page.clone(), answer.clone());
            Button::new("link-remove")
                .label(t!("prompt.link.remove"))
                .ghost()
                .min_h(px(target_size()))
                .on_click(move |_, window, cx| {
                    window.close_dialog(cx);
                    page.update(cx, |this, cx| {
                        answer(this, LinkAnswer::Remove, window, cx);
                    })
                    .ok();
                })
        });
        let form = fields.form(ask_text, remove, cx);
        let (page, answer) = (page.clone(), answer.clone());
        let (text, address, error) = (text.clone(), address.clone(), error.clone());
        alert
            .title(if editing {
                t!("prompt.link.edit_title")
            } else {
                t!("prompt.link.add_title")
            })
            .description(form)
            .confirm()
            .ok_text(if editing {
                t!("prompt.apply")
            } else {
                t!("prompt.link.add")
            })
            .cancel_text(t!("prompt.cancel"))
            .on_ok(move |_, window, cx| {
                let words = ask_text.then(|| text.read(cx).value().to_string());
                match link_answer(words.as_deref(), &address.read(cx).value()) {
                    Ok(link) => {
                        page.update(cx, |this, cx| answer(this, link, window, cx))
                            .ok();
                        true
                    }
                    Err(message) => {
                        error.update(cx, |error, cx| {
                            *error = Some(message.into());
                            cx.notify();
                        });
                        window.refresh();
                        false
                    }
                }
            })
    });
}

#[cfg(test)]
mod tests {
    use super::{LinkAnswer, link_answer};

    #[test]
    fn selected_words_need_only_a_destination() {
        assert_eq!(
            link_answer(None, " https://example.com "),
            Ok(LinkAnswer::Set {
                text: None,
                destination: "https://example.com".to_owned(),
            })
        );
    }

    #[test]
    fn a_new_link_keeps_its_words_on_one_line() {
        assert_eq!(
            link_answer(Some(" the\nbook "), "ch02.md"),
            Ok(LinkAnswer::Set {
                text: Some("the book".to_owned()),
                destination: "ch02.md".to_owned(),
            })
        );
    }

    #[test]
    fn an_empty_field_is_named() {
        assert_eq!(
            link_answer(Some("  "), "ch02.md"),
            Err("Enter the text of the link.".into())
        );
        assert_eq!(
            link_answer(None, " "),
            Err("Enter a web address, or choose a place in this book.".into())
        );
    }
}
