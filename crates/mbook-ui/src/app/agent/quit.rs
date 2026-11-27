//! What the window shows of agents: a badge while one is at work, and a
//! question when it closes over changes an agent did not save.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex};
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, px,
};
use rust_i18n::t;

use crate::features::agents::AgentStatus;
use crate::features::edit_book::agent::Op;
use crate::features::edit_book::sessions::Sessions;
use crate::shared::ui::target_size;

/// The badge that says an agent is at work, while one is.
pub(crate) fn badge(cx: &App) -> Option<AnyElement> {
    let busy = AgentStatus::try_global(cx).is_some_and(|status| status.read(cx).busy());
    busy.then(|| {
        let words = t!("agent.working");
        div()
            .id("agent-working")
            .role(Role::Status)
            .aria_label(words.clone())
            .test_support()
            .absolute()
            .top(px(8.0))
            .right(px(16.0))
            .px_2()
            .py_1()
            .rounded_md()
            .text_xs()
            .bg(cx.theme().secondary)
            .text_color(cx.theme().secondary_foreground)
            .child(words)
            .into_any_element()
    })
}

/// Whether the window may close now. With books only agents edit that have
/// unsaved changes, it asks first, and closes once answered.
pub(crate) fn may_close(allowed: &Rc<Cell<bool>>, window: &mut Window, cx: &mut App) -> bool {
    if allowed.get() {
        return true;
    }
    let unsaved = Sessions::unsaved_of_agents(cx);
    if unsaved.is_empty() {
        return true;
    }
    let count = unsaved.len();
    let (discard, save) = (Rc::clone(allowed), Rc::clone(allowed));
    window.open_alert_dialog(cx, move |alert, _, _| {
        let (discard, save) = (Rc::clone(&discard), Rc::clone(&save));
        let books = unsaved.clone();
        alert
            .title(t!("agent.quit.title"))
            .description(t!("agent.quit.detail", count = count))
            .footer(
                h_flex()
                    .w_full()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("quit-cancel")
                            .label(t!("prompt.cancel"))
                            .min_h(px(target_size()))
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("quit-discard")
                            .label(t!("agent.quit.discard"))
                            .min_h(px(target_size()))
                            .on_click(move |_, window, cx| {
                                discard.set(true);
                                window.close_dialog(cx);
                                window.remove_window();
                            }),
                    )
                    .child(
                        Button::new("quit-save")
                            .primary()
                            .label(t!("agent.quit.save"))
                            .min_h(px(target_size()))
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                                let saves: Vec<_> = books
                                    .iter()
                                    .map(|(_, handle)| handle.agent(Op::Save))
                                    .collect();
                                let save = Rc::clone(&save);
                                let handle = window.window_handle();
                                cx.spawn(async move |cx| {
                                    for saving in saves {
                                        // A book with problems is not written; the
                                        // person chose to leave all the same.
                                        drop(saving.await);
                                    }
                                    save.set(true);
                                    drop(handle.update(cx, |_, window, _| window.remove_window()));
                                })
                                .detach();
                            }),
                    ),
            )
    });
    false
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{AnyWindowHandle, AppContext as _, TestAppContext};

    use super::{badge, may_close};
    use crate::features::agents::AgentStatus;
    use crate::features::edit_book::agent::Op;
    use crate::features::edit_book::agent::types::MetaPatch;
    use crate::features::edit_book::sessions::Sessions;
    use crate::shared::model::BookKey;
    use crate::testing::{self, Blank, Fixture};

    /// A window, and a book an agent changed and did not save.
    fn unsaved(cx: &mut TestAppContext) -> (Fixture, BookKey, AnyWindowHandle) {
        let fixture = Fixture::new();
        fixture.install(cx);
        let key = BookKey::generate();
        let mut book = mbook::Book::new("T", "en").unwrap();
        book.add_chapter("ch01.md", "# One\n").unwrap();
        book.save(fixture.roots.book(key)).unwrap();
        let (window, _) = testing::open(cx, 800.0, 600.0, |_, _| Blank);
        let agent = cx.update(|cx| Sessions::agent(key, cx));
        let patch = MetaPatch {
            title: Some("Changed".into()),
            ..MetaPatch::default()
        };
        let answer = cx
            .foreground_executor()
            .block_test(agent.agent(Op::SetMeta(patch)))
            .unwrap();
        cx.update(|cx| Sessions::note(key, answer.unsaved, cx));
        (fixture, key, window)
    }

    fn ask(allowed: &Rc<Cell<bool>>, window: AnyWindowHandle, cx: &mut TestAppContext) -> bool {
        let allowed = Rc::clone(allowed);
        let answer = cx
            .update_window(window, move |_, window, cx| may_close(&allowed, window, cx))
            .unwrap();
        cx.run_until_parked();
        answer
    }

    fn press(window: AnyWindowHandle, id: &str, cx: &mut TestAppContext) {
        let id = id.to_owned();
        testing::frame(cx, window, move |window, cx| window.click(id, cx));
        cx.run_until_parked();
    }

    #[gpui_kit::test]
    fn a_window_without_unsaved_changes_of_agents_closes_at_once(cx: &mut TestAppContext) {
        let (window, _) = testing::open(cx, 800.0, 600.0, |_, _| Blank);
        let allowed = Rc::new(Cell::new(false));
        assert!(ask(&allowed, window, cx));
    }

    #[gpui_kit::test]
    fn closing_over_unsaved_changes_of_an_agent_asks_and_can_go_without_them(
        cx: &mut TestAppContext,
    ) {
        let (_fixture, _key, window) = unsaved(cx);
        let allowed = Rc::new(Cell::new(false));
        assert!(!ask(&allowed, window, cx));
        press(window, "quit-discard", cx);
        assert!(allowed.get());
        assert!(cx.update(|cx| cx.windows().is_empty()), "the window closed");
    }

    #[gpui_kit::test]
    fn closing_over_unsaved_changes_of_an_agent_can_save_them(cx: &mut TestAppContext) {
        let (fixture, key, window) = unsaved(cx);
        let allowed = Rc::new(Cell::new(false));
        assert!(!ask(&allowed, window, cx));
        press(window, "quit-save", cx);
        for _ in 0..200 {
            if allowed.get() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            cx.run_until_parked();
        }
        assert!(allowed.get());
        let saved = mbook::Book::open(fixture.roots.book(key)).unwrap();
        assert_eq!(saved.meta().title(), "Changed");
    }

    #[gpui_kit::test]
    fn closing_can_be_cancelled(cx: &mut TestAppContext) {
        let (_fixture, _key, window) = unsaved(cx);
        let allowed = Rc::new(Cell::new(false));
        assert!(!ask(&allowed, window, cx));
        press(window, "quit-cancel", cx);
        assert!(!allowed.get());
    }

    #[gpui_kit::test]
    fn the_badge_shows_while_a_call_is_answered(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        assert!(cx.update(|cx| badge(cx)).is_none());
        let status = cx.update(AgentStatus::global);
        status.update(cx, |status, cx| status.working(true, cx));
        assert!(cx.update(|cx| badge(cx)).is_some());
        status.update(cx, |status, cx| status.working(false, cx));
        assert!(cx.update(|cx| badge(cx)).is_none());
    }
}
