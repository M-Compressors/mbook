//! The find field, feedback and navigation shared by the reader and editor.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, h_flex, v_flex};
use std::borrow::Cow;

use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, Styled as _, Subscription, Task, Window, div,
};

use rust_i18n::t;

use super::{follow_language, icon_button, status_line};

/// The key context of the field and match navigation controls.
pub(crate) const KEY_CONTEXT: &str = "FindBar";

/// The pause between changing a query and searching.
pub(crate) const TYPING_PAUSE: std::time::Duration = std::time::Duration::from_millis(180);

/// A find bar and the current search for one page.
pub(crate) struct SearchBar<T> {
    /// The field that emits query and Enter events.
    pub(crate) input: Entity<InputState>,
    /// Whether the bar is open.
    pub(crate) open: bool,
    /// The last match, used as the cursor for the next move.
    pub(crate) current: Option<T>,
    /// Dropping the task cancels an obsolete request.
    pub(crate) task: Option<Task<()>>,
    /// The name of the field and its placeholder, in the current language.
    label: fn() -> Cow<'static, str>,
    feedback: Option<SharedString>,
    _language: Subscription,
}

type Step<P> = fn(&mut P, bool, &mut Context<'_, P>);
type Close<P> = fn(&mut P, &mut Window, &mut Context<'_, P>);

impl<T> SearchBar<T> {
    /// Creates a closed find bar for the given scope, named by what `label`
    /// says in the current language.
    pub(crate) fn new<P: 'static>(
        label: fn() -> Cow<'static, str>,
        window: &mut Window,
        cx: &mut Context<'_, P>,
    ) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx));
        let language = follow_language(&input, label, window, cx);
        Self {
            input,
            open: false,
            current: None,
            task: None,
            label,
            feedback: None,
            _language: language,
        }
    }

    /// Brings the bar forward and selects any previous query.
    pub(crate) fn show(&mut self, window: &mut Window, cx: &mut App) {
        self.open = true;
        self.input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
    }

    /// Clears the previous request and result.
    pub(crate) fn reset(&mut self) {
        self.task = None;
        self.current = None;
        self.feedback = None;
    }

    /// Closes the bar and cancels the search.
    pub(crate) fn hide(&mut self) {
        self.open = false;
        self.reset();
    }

    /// Stores a completed search's result and the words for the status line.
    pub(crate) fn finish(&mut self, result: Option<T>, feedback: impl Into<SharedString>) {
        self.task = None;
        self.current = result;
        self.feedback = Some(feedback.into());
    }

    /// Draws the find controls while open.
    pub(crate) fn render<P: 'static>(
        &self,
        step: Step<P>,
        close: Close<P>,
        cx: &Context<'_, P>,
    ) -> Option<AnyElement> {
        self.open.then(|| {
            let pending = self.task.is_some();
            let disabled = pending || self.current.is_none();
            let feedback = if pending {
                Some(t!("ui.searching").into())
            } else {
                self.feedback.clone()
            };
            v_flex()
                .key_context(KEY_CONTEXT)
                .w_full()
                .flex_none()
                .bg(cx.theme().background)
                .text_color(cx.theme().foreground)
                .border_b_1()
                .border_color(cx.theme().border)
                .px_3()
                .py_1()
                .child(
                    h_flex()
                        .gap_1()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(Input::new(&self.input).aria_label((self.label)())),
                        )
                        .child(
                            icon_button(
                                "find-previous",
                                IconName::ChevronUp,
                                t!("ui.previous_match"),
                            )
                            .disabled(disabled)
                            .on_click(cx.listener(move |this, _, _, cx| step(this, true, cx))),
                        )
                        .child(
                            icon_button("find-next", IconName::ChevronDown, t!("ui.next_match"))
                                .disabled(disabled)
                                .on_click(cx.listener(move |this, _, _, cx| step(this, false, cx))),
                        )
                        .child(
                            icon_button("find-close", IconName::X, t!("ui.close_search")).on_click(
                                cx.listener(move |this, _, window, cx| close(this, window, cx)),
                            ),
                        ),
                )
                .children(
                    feedback.map(|text| status_line("find-status", text).truncate().text_sm()),
                )
                .into_any_element()
        })
    }
}
