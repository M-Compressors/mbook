//! Text a view keeps, made again when the language changes.

use std::borrow::Cow;

use gpui_kit::component::input::InputState;
use gpui_kit::{Context, Entity, Subscription, Window};

use crate::shared::i18n::InterfaceLanguage;

/// Gives `input` the placeholder `text` makes, now and whenever the language
/// of the interface changes. The field keeps its placeholder, which a view
/// gives it once; the subscription, held by the view, keeps it in the current
/// language.
pub(crate) fn follow_language<P: 'static>(
    input: &Entity<InputState>,
    text: fn() -> Cow<'static, str>,
    window: &mut Window,
    cx: &mut Context<'_, P>,
) -> Subscription {
    input.update(cx, |input, cx| input.set_placeholder(text(), window, cx));
    let input = input.downgrade();
    cx.observe_global_in::<InterfaceLanguage>(window, move |_, window, cx| {
        // A field that is gone needs no placeholder.
        input
            .update(cx, |input, cx| input.set_placeholder(text(), window, cx))
            .ok();
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use gpui_kit::{AppContext as _, IntoElement, Render, TestAppContext};

    use super::*;
    use crate::shared::i18n::Language;
    use crate::testing;

    /// How often [`text`] was asked for the placeholder.
    static ASKED: AtomicUsize = AtomicUsize::new(0);

    fn text() -> Cow<'static, str> {
        ASKED.fetch_add(1, Ordering::SeqCst);
        "Search".into()
    }

    /// A view with one field that follows the language.
    struct Field {
        _input: Entity<InputState>,
        _language: Subscription,
    }

    impl Render for Field {
        fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
            gpui_kit::div()
        }
    }

    #[gpui_kit::test]
    fn a_placeholder_is_made_at_once_and_again_when_the_language_changes(cx: &mut TestAppContext) {
        let (_window, _field) = testing::open(cx, 400.0, 300.0, |window, cx| {
            let input = cx.new(|cx| InputState::new(window, cx));
            let language = follow_language(&input, text, window, cx);
            Field {
                _input: input,
                _language: language,
            }
        });
        let before = ASKED.load(Ordering::SeqCst);
        assert!(before >= 1, "the placeholder is given at once");
        cx.update(|cx| cx.set_global(InterfaceLanguage(Language::Russian)));
        cx.run_until_parked();
        assert!(ASKED.load(Ordering::SeqCst) > before, "made again");
    }
}
