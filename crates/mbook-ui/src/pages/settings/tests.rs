//! The settings page, in headless windows.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::component::select::SelectEvent;
use gpui_kit::test::{TestEventExt as _, TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{
    Action as _, AnyWindowHandle, Entity, MouseButton, Role, Subscription, TestAppContext, px,
};

use super::*;
use crate::testing::{self, Fixture};

/// A page over a library in a temporary root, with its events recorded.
struct Started {
    _fixture: Fixture,
    window: AnyWindowHandle,
    page: Entity<SettingsPage>,
    appearance: Entity<Appearance>,
    events: Rc<RefCell<Vec<SettingsEvent>>>,
    _subscription: Subscription,
}

/// The tallest a test window is: the whole form is on the screen, so that
/// every control can be reached by the pointer.
const TALL: f32 = 3000.0;

fn start_with(
    cx: &mut TestAppContext,
    width: f32,
    prepare: impl FnOnce(&Entity<Appearance>, &mut TestAppContext),
) -> Started {
    let fixture = Fixture::new();
    cx.update(gpui_kit::init);
    fixture.install(cx);
    let appearance = cx.update(|cx| {
        let appearance = cx.new(Appearance::new);
        Appearance::install(appearance.clone(), cx);
        appearance
    });
    cx.run_until_parked();
    prepare(&appearance, cx);
    let (window, page) = testing::open(cx, width, TALL, SettingsPage::new);
    let events = Rc::new(RefCell::new(Vec::new()));
    let recorded = events.clone();
    let subscription = cx.update(|cx| {
        cx.subscribe(&page, move |_, event: &SettingsEvent, _| {
            recorded.borrow_mut().push(*event);
        })
    });
    // The root gives the page the focus when it comes to the front.
    let focus = page.clone();
    testing::frame(cx, window, move |window, cx| {
        let handle = focus.focus_handle(cx);
        window.focus(&handle, cx);
    });
    cx.run_until_parked();
    Started {
        _fixture: fixture,
        window,
        page,
        appearance,
        events,
        _subscription: subscription,
    }
}

fn start(cx: &mut TestAppContext, width: f32) -> Started {
    start_with(cx, width, |_, _| {})
}

fn settings(started: &Started, cx: &TestAppContext) -> Settings {
    started
        .appearance
        .read_with(cx, |appearance, _| appearance.settings().clone())
}

fn set(started: &Started, cx: &mut TestAppContext, change: impl FnOnce(&mut Settings)) {
    started
        .appearance
        .update(cx, |appearance, cx| appearance.update(change, cx));
    cx.run_until_parked();
}

fn click_label(started: &Started, label: &str, cx: &mut TestAppContext) {
    let label = label.to_owned();
    testing::frame(cx, started.window, move |window, cx| {
        let at = window.find_by_label(&label).bounds().center();
        window.pointer_down(at, MouseButton::Left, cx);
        window.pointer_up(at, MouseButton::Left, cx);
    });
    cx.run_until_parked();
}

fn press(started: &Started, key: &str, cx: &mut TestAppContext) {
    let key = key.to_owned();
    testing::frame(cx, started.window, move |window, cx| window.press(&key, cx));
    cx.run_until_parked();
}

/// Moves the focus along with Tab until the control called `label` has it.
fn tab_to(started: &Started, label: &str, cx: &mut TestAppContext) {
    for _ in 0..60 {
        let focused = testing::frame(cx, started.window, |window, _| {
            window
                .try_find_by_label(label)
                .and_then(|element| element.focused())
        });
        if focused == Some(true) {
            return;
        }
        press(started, "tab", cx);
    }
    panic!("{label} did not get the focus");
}

/// Whether a control is disabled; an enabled one reports nothing.
fn is_disabled(started: &Started, label: &str, cx: &mut TestAppContext) -> bool {
    let label = label.to_owned();
    testing::frame(cx, started.window, move |window, _| {
        window.find_by_label(&label).disabled() == Some(true)
    })
}

/// The search field inside the select and the track of a slider belong to
/// the Kit; the select and the slider themselves, which are the controls, are
/// checked.
fn is_inside_select(element: &gpui_kit::test::ElementSnapshot) -> bool {
    let path = format!("{:?}", element.path());
    path.contains("Name(\"typeface\"), View") || path.contains("slider::SliderTrack")
}

fn label_of(started: &Started, id: &'static str, cx: &mut TestAppContext) -> Option<String> {
    testing::frame(cx, started.window, move |window, _| {
        window
            .try_find(id)
            .and_then(|element| element.label().map(str::to_owned))
    })
}

#[gpui_kit::test]
fn the_page_is_drawn_at_390_800_and_1280_pixels(cx: &mut TestAppContext) {
    for width in [390.0, 800.0, 1280.0] {
        let started = start(cx, width);
        testing::frame(cx, started.window, |window, _| {
            let page = window.find("settings-page");
            assert_eq!(page.role(), Some(Role::Main), "{width} px");
            assert_eq!(page.label(), Some("Settings"));
            assert!(window.find("settings-sample").visible(), "{width} px");
        });
    }
}

#[gpui_kit::test]
fn on_a_compact_window_the_form_takes_the_full_width(cx: &mut TestAppContext) {
    let started = start(cx, 390.0);
    testing::frame(cx, started.window, |window, _| {
        let sample = window.find("settings-sample").bounds();
        assert!(
            sample.size.width >= px(390.0 - 2.0 * 16.0 - 1.0),
            "{sample:?}"
        );
    });
}

#[gpui_kit::test]
fn on_a_larger_window_the_form_is_a_column_of_at_most_640_pixels(cx: &mut TestAppContext) {
    for width in [800.0, 1280.0] {
        let started = start(cx, width);
        testing::frame(cx, started.window, |window, _| {
            let sample = window.find("settings-sample").bounds();
            assert!(sample.size.width <= px(640.0), "{width} px: {sample:?}");
            assert!(sample.size.width >= px(400.0), "{width} px: {sample:?}");
            let left = sample.origin.x;
            let right = px(width) - (sample.origin.x + sample.size.width);
            assert!(
                (left - right).abs() < px(2.0),
                "{width} px: the column is centred: {left:?} {right:?}"
            );
        });
    }
}

#[gpui_kit::test]
fn the_back_control_closes_the_page(cx: &mut TestAppContext) {
    let started = start(cx, 800.0);
    testing::frame(cx, started.window, |window, cx| window.click("back", cx));
    assert_eq!(*started.events.borrow(), [SettingsEvent::Close]);
}

#[gpui_kit::test]
fn the_go_back_action_closes_the_page(cx: &mut TestAppContext) {
    let started = start(cx, 800.0);
    let page = started.page.clone();
    testing::frame(cx, started.window, move |window, cx| {
        let focus = page.focus_handle(cx);
        window.focus(&focus, cx);
        window.dispatch_action(GoBack.boxed_clone(), cx);
    });
    assert_eq!(*started.events.borrow(), [SettingsEvent::Close]);
}

#[gpui_kit::test]
fn every_setting_has_a_control_named_by_its_visible_label(cx: &mut TestAppContext) {
    let started = start(cx, 800.0);
    testing::frame(cx, started.window, |window, _| {
        for (label, role) in [
            ("Typeface of books", Role::ComboBox),
            ("Size of the text", Role::Slider),
            ("Smaller text", Role::Button),
            ("Larger text", Role::Button),
            ("Line height", Role::Slider),
            ("Page tone", Role::Group),
            ("Follow the system", Role::RadioButton),
            ("Paper", Role::RadioButton),
            ("Sepia", Role::RadioButton),
            ("Neutral", Role::RadioButton),
            ("Night", Role::RadioButton),
            ("Size of the interface", Role::Slider),
            ("Controls of the reader", Role::Group),
            ("Hide while reading", Role::RadioButton),
            ("Always shown", Role::RadioButton),
            ("Motion", Role::Group),
            ("As the system says", Role::RadioButton),
            ("Reduced", Role::RadioButton),
            ("Check for updates", Role::Switch),
            ("Check now", Role::Button),
            ("Back", Role::Button),
        ] {
            let element = window
                .try_find_by_label(label)
                .unwrap_or_else(|| panic!("no control named {label}"));
            assert_eq!(element.role(), Some(role), "{label}");
        }
    });
}

#[gpui_kit::test]
fn every_reported_element_has_a_role_and_every_control_a_name(cx: &mut TestAppContext) {
    const CONTROLS: [Role; 8] = [
        Role::Button,
        Role::Link,
        Role::CheckBox,
        Role::RadioButton,
        Role::Switch,
        Role::Slider,
        Role::TextInput,
        Role::ComboBox,
    ];
    for width in [390.0, 800.0, 1280.0] {
        let started = start(cx, width);
        testing::frame(cx, started.window, |window, _| {
            for element in window.elements() {
                if is_inside_select(&element)
                    || testing::is_kit_decoration(&format!("{:?}", element.path()))
                {
                    continue;
                }
                assert!(element.role().is_some(), "{width} px: no role: {element:?}");
                if element.role().is_some_and(|role| CONTROLS.contains(&role)) {
                    assert!(
                        element
                            .label()
                            .is_some_and(|label| !label.trim().is_empty()),
                        "{width} px: a control without a name: {element:?}"
                    );
                }
            }
        });
    }
}

#[gpui_kit::test]
fn every_control_is_at_least_a_pointer_target_in_both_directions(cx: &mut TestAppContext) {
    let started = start(cx, 800.0);
    testing::frame(cx, started.window, |window, _| {
        for label in ["Smaller text", "Larger text", "Check now", "Back"] {
            let bounds = window.find_by_label(label).bounds();
            assert!(bounds.size.width >= px(24.0), "{label}: {bounds:?}");
            assert!(bounds.size.height >= px(24.0), "{label}: {bounds:?}");
        }
    });
}

#[gpui_kit::test]
fn the_form_holds_the_stored_values(cx: &mut TestAppContext) {
    let started = start_with(cx, 800.0, |appearance, cx| {
        appearance.update(cx, |appearance, cx| {
            appearance.update(
                |settings| {
                    settings.text_size = 22.0;
                    settings.line_height = 1.8;
                    settings.interface_scale = 1.5;
                    settings.page_tone = Some(PageTone::Neutral);
                    settings.keep_controls = true;
                    settings.reduce_motion = true;
                },
                cx,
            );
        });
    });
    assert_eq!(
        label_of(&started, "text-size-value", cx).as_deref(),
        Some("22 px")
    );
    assert_eq!(
        label_of(&started, "line-height-value", cx).as_deref(),
        Some("1.80")
    );
    assert_eq!(
        label_of(&started, "interface-size-value", cx).as_deref(),
        Some("150%")
    );
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(window.find_by_label("Neutral").checked(), Some(true));
        assert_eq!(window.find_by_label("Paper").checked(), Some(false));
        assert_eq!(window.find_by_label("Always shown").checked(), Some(true));
        assert_eq!(window.find_by_label("Reduced").checked(), Some(true));
    });
}

#[gpui_kit::test]
fn a_page_tone_is_chosen_by_its_name_and_applies_at_once(cx: &mut TestAppContext) {
    let started = start(cx, 800.0);
    for (label, expected) in [
        ("Sepia", Some(PageTone::Sepia)),
        ("Night", Some(PageTone::Night)),
        ("Paper", Some(PageTone::Paper)),
        ("Neutral", Some(PageTone::Neutral)),
        ("Follow the system", None),
    ] {
        click_label(&started, label, cx);
        assert_eq!(settings(&started, cx).page_tone, expected, "{label}");
        testing::frame(cx, started.window, |window, _| {
            assert_eq!(window.find_by_label(label).checked(), Some(true), "{label}");
        });
    }
}

#[gpui_kit::test]
fn the_controls_of_the_reader_are_chosen_by_name(cx: &mut TestAppContext) {
    let started = start(cx, 800.0);
    assert!(!settings(&started, cx).keep_controls);
    click_label(&started, "Always shown", cx);
    assert!(settings(&started, cx).keep_controls);
    click_label(&started, "Hide while reading", cx);
    assert!(!settings(&started, cx).keep_controls);
}

#[gpui_kit::test]
fn motion_is_reduced_by_choice_and_follows_the_system_by_default(cx: &mut TestAppContext) {
    let started = start(cx, 800.0);
    assert!(!settings(&started, cx).reduce_motion);
    click_label(&started, "Reduced", cx);
    assert!(settings(&started, cx).reduce_motion);
    click_label(&started, "As the system says", cx);
    assert!(!settings(&started, cx).reduce_motion);
}

#[gpui_kit::test]
fn the_text_is_made_larger_and_smaller_by_buttons(cx: &mut TestAppContext) {
    let started = start(cx, 800.0);
    click_label(&started, "Larger text", cx);
    assert!((settings(&started, cx).text_size - 21.0).abs() < 0.001);
    assert_eq!(
        label_of(&started, "text-size-value", cx).as_deref(),
        Some("21 px")
    );
    click_label(&started, "Smaller text", cx);
    click_label(&started, "Smaller text", cx);
    assert!((settings(&started, cx).text_size - 19.0).abs() < 0.001);
    assert_eq!(
        label_of(&started, "text-size-value", cx).as_deref(),
        Some("19 px")
    );
}

#[gpui_kit::test]
fn the_buttons_stop_at_the_ends_of_the_range(cx: &mut TestAppContext) {
    let started = start(cx, 800.0);
    set(&started, cx, |settings| settings.text_size = 12.0);
    assert!(is_disabled(&started, "Smaller text", cx));
    assert!(!is_disabled(&started, "Larger text", cx));
    set(&started, cx, |settings| settings.text_size = 80.0);
    assert!(is_disabled(&started, "Larger text", cx));
    assert!(!is_disabled(&started, "Smaller text", cx));
}

#[gpui_kit::test]
fn the_slider_of_the_text_size_is_set_with_the_keys_alone(cx: &mut TestAppContext) {
    let started = start(cx, 800.0);
    tab_to(&started, "Size of the text", cx);
    press(&started, "right", cx);
    assert!((settings(&started, cx).text_size - 21.0).abs() < 0.001);
    press(&started, "left", cx);
    press(&started, "left", cx);
    assert!((settings(&started, cx).text_size - 19.0).abs() < 0.001);
    assert_eq!(
        label_of(&started, "text-size-value", cx).as_deref(),
        Some("19 px")
    );
}

#[gpui_kit::test]
fn the_slider_of_the_line_height_moves_in_twentieths(cx: &mut TestAppContext) {
    let started = start(cx, 800.0);
    tab_to(&started, "Line height", cx);
    press(&started, "right", cx);
    assert!((settings(&started, cx).line_height - 1.65).abs() < 0.001);
    assert_eq!(
        label_of(&started, "line-height-value", cx).as_deref(),
        Some("1.65")
    );
}

#[gpui_kit::test]
fn the_interface_is_resized_in_steps_of_ten_percent(cx: &mut TestAppContext) {
    let started = start(cx, 1280.0);
    tab_to(&started, "Size of the interface", cx);
    press(&started, "right", cx);
    assert!((settings(&started, cx).interface_scale - 1.1).abs() < 0.001);
    assert_eq!(
        label_of(&started, "interface-size-value", cx).as_deref(),
        Some("110%")
    );
}

#[gpui_kit::test]
fn a_page_tone_is_chosen_with_the_keys_alone(cx: &mut TestAppContext) {
    let started = start(cx, 800.0);
    tab_to(&started, "Sepia", cx);
    press(&started, "space", cx);
    assert_eq!(settings(&started, cx).page_tone, Some(PageTone::Sepia));
}

#[gpui_kit::test]
fn a_change_made_elsewhere_moves_the_controls(cx: &mut TestAppContext) {
    let started = start(cx, 800.0);
    set(&started, cx, |settings| {
        settings.text_size = 26.0;
        settings.line_height = 1.4;
        settings.interface_scale = 1.3;
    });
    assert_eq!(
        label_of(&started, "text-size-value", cx).as_deref(),
        Some("26 px")
    );
    assert_eq!(
        label_of(&started, "line-height-value", cx).as_deref(),
        Some("1.40")
    );
    assert_eq!(
        label_of(&started, "interface-size-value", cx).as_deref(),
        Some("130%")
    );
    let slider = started
        .page
        .read_with(cx, |page, cx| page.text_size.read(cx).value().end());
    assert!((slider - 26.0).abs() < 0.001);
}

#[gpui_kit::test]
fn the_typeface_list_starts_with_literata_and_holds_the_stored_one(cx: &mut TestAppContext) {
    let started = start(cx, 800.0);
    let (selected, listed) = started.page.read_with(cx, |page, cx| {
        let select = page.typeface.read(cx);
        (select.selected_value().cloned(), page.listed.clone())
    });
    assert_eq!(selected.as_deref(), Some("Literata"));
    assert_eq!(listed.0, "Literata");
    testing::frame(cx, started.window, |window, _| {
        let select = window.find_by_label("Typeface of books");
        assert_eq!(select.value(), Some("Literata"));
    });
}

#[gpui_kit::test]
fn a_stored_typeface_the_device_lacks_is_shown_as_not_installed(cx: &mut TestAppContext) {
    let started = start_with(cx, 800.0, |appearance, cx| {
        appearance.update(cx, |appearance, cx| {
            appearance.update(
                |settings| settings.typeface = "Nonesuch Serif".to_owned(),
                cx,
            );
        });
    });
    let families = started
        .appearance
        .read_with(cx, |appearance, _| appearance.families().len());
    testing::frame(cx, started.window, |window, _| {
        let value = window
            .find_by_label("Typeface of books")
            .value()
            .map(str::to_owned);
        let value = value.unwrap_or_default();
        assert!(value.starts_with("Nonesuch Serif"), "{value}");
        if families > 0 {
            assert!(value.ends_with("(not installed)"), "{value}");
        }
    });
    assert_eq!(
        settings(&started, cx).typeface,
        "Nonesuch Serif",
        "the stored name is kept"
    );
}

#[gpui_kit::test]
fn a_typeface_that_was_chosen_is_stored(cx: &mut TestAppContext) {
    let started = start(cx, 800.0);
    let select = started.page.read_with(cx, |page, _| page.typeface.clone());
    select.update(cx, |_, cx| {
        cx.emit(SelectEvent::Confirm(Some("Georgia".to_owned())));
    });
    cx.run_until_parked();
    assert_eq!(settings(&started, cx).typeface, "Georgia");
}

#[gpui_kit::test]
fn a_choice_that_is_cleared_leaves_the_typeface_as_it_was(cx: &mut TestAppContext) {
    let started = start(cx, 800.0);
    let select = started.page.read_with(cx, |page, _| page.typeface.clone());
    select.update(cx, |_, cx| cx.emit(SelectEvent::Confirm(None)));
    cx.run_until_parked();
    assert_eq!(settings(&started, cx).typeface, "Literata");
}

#[gpui_kit::test]
fn the_list_is_made_again_when_the_typeface_changes_elsewhere(cx: &mut TestAppContext) {
    let started = start(cx, 800.0);
    set(&started, cx, |settings| {
        settings.typeface = "Georgia".to_owned();
    });
    let listed = started.page.read_with(cx, |page, _| page.listed.0.clone());
    assert_eq!(listed, "Georgia");
    let selected = started.page.read_with(cx, |page, cx| {
        page.typeface.read(cx).selected_value().cloned()
    });
    assert_eq!(selected.as_deref(), Some("Georgia"));
}

#[gpui_kit::test]
fn the_sample_grows_with_the_text_and_with_the_line_height(cx: &mut TestAppContext) {
    let started = start(cx, 800.0);
    let height = |started: &Started, cx: &mut TestAppContext| {
        testing::frame(cx, started.window, |window, _| {
            window.find("settings-sample").bounds().size.height
        })
    };
    set(&started, cx, |settings| {
        settings.text_size = 12.0;
        settings.line_height = 1.3;
    });
    let small = height(&started, cx);
    set(&started, cx, |settings| settings.text_size = 80.0);
    let large = height(&started, cx);
    assert!(large > small, "{small:?} {large:?}");
    set(&started, cx, |settings| settings.line_height = 2.0);
    let airy = height(&started, cx);
    assert!(airy > large, "{large:?} {airy:?}");
}

#[gpui_kit::test]
fn the_sample_is_a_named_group_in_latin_and_cyrillic(cx: &mut TestAppContext) {
    assert!(SAMPLE_LATIN.is_ascii());
    assert!(
        SAMPLE_CYRILLIC
            .chars()
            .any(|letter| ('а'..='я').contains(&letter))
    );
    let started = start(cx, 800.0);
    testing::frame(cx, started.window, |window, _| {
        let sample = window.find("settings-sample");
        assert_eq!(sample.role(), Some(Role::Group));
        assert_eq!(sample.label(), Some("Sample of the text"));
    });
}

#[gpui_kit::test]
fn the_version_of_the_application_is_shown(cx: &mut TestAppContext) {
    let started = start(cx, 800.0);
    assert_eq!(
        label_of(&started, "app-version", cx),
        Some(format!("Version {}", env!("CARGO_PKG_VERSION")))
    );
}

#[gpui_kit::test]
fn a_debug_build_shows_the_updates_disabled_with_the_reason(cx: &mut TestAppContext) {
    let started = start(cx, 800.0);
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(
            window.find_by_label("Check for updates").disabled(),
            Some(true)
        );
        assert_eq!(window.find_by_label("Check now").disabled(), Some(true));
        let status = window.find("update-status");
        assert_eq!(status.role(), Some(Role::Status));
        assert_eq!(status.label(), Some("This build does not update itself"));
    });
}

#[gpui_kit::test]
fn the_setting_of_the_check_is_kept_even_when_the_switch_is_disabled(cx: &mut TestAppContext) {
    let started = start(cx, 800.0);
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(
            window.find_by_label("Check for updates").checked(),
            Some(true)
        );
    });
    set(&started, cx, |settings| settings.check_updates = false);
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(
            window.find_by_label("Check for updates").checked(),
            Some(false)
        );
    });
}

#[cfg(feature = "self-update")]
mod updates {
    use super::*;
    use crate::features::auto_update::UpdateBanner;

    type Answer = Result<Option<&'static str>, &'static str>;

    /// A page over a banner whose releases answer as scripted.
    fn with_banner(cx: &mut TestAppContext, enabled: bool, answer: Answer) -> Started {
        let fixture = Fixture::new();
        cx.update(gpui_kit::init);
        fixture.install(cx);
        let appearance = cx.update(|cx| {
            let appearance = cx.new(Appearance::new);
            Appearance::install(appearance.clone(), cx);
            appearance
        });
        cx.run_until_parked();
        let (window, page) = testing::open(cx, 800.0, TALL, |window, cx| {
            let banner = cx.new(|cx| UpdateBanner::scripted(enabled, answer, window, cx));
            UpdateBanner::install(banner, cx);
            SettingsPage::new(window, cx)
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let recorded = events.clone();
        let subscription = cx.update(|cx| {
            cx.subscribe(&page, move |_, event: &SettingsEvent, _| {
                recorded.borrow_mut().push(*event);
            })
        });
        Started {
            _fixture: fixture,
            window,
            page,
            appearance,
            events,
            _subscription: subscription,
        }
    }

    fn status(started: &Started, cx: &mut TestAppContext) -> Option<String> {
        label_of(started, "update-status", cx)
    }

    #[gpui_kit::test]
    fn a_build_that_updates_itself_has_the_switch_and_the_button_enabled(cx: &mut TestAppContext) {
        let started = with_banner(cx, true, Ok(None));
        assert!(!is_disabled(&started, "Check for updates", cx));
        assert!(!is_disabled(&started, "Check now", cx));
    }

    #[gpui_kit::test]
    fn the_switch_turns_the_check_off_and_on(cx: &mut TestAppContext) {
        let started = with_banner(cx, true, Ok(None));
        click_label(&started, "Check for updates", cx);
        assert!(!settings(&started, cx).check_updates);
        click_label(&started, "Check for updates", cx);
        assert!(settings(&started, cx).check_updates);
    }

    #[gpui_kit::test]
    fn checking_now_says_that_mbook_is_up_to_date(cx: &mut TestAppContext) {
        let started = with_banner(cx, true, Ok(None));
        click_label(&started, "Check now", cx);
        assert_eq!(status(&started, cx).as_deref(), Some("mbook is up to date"));
    }

    #[gpui_kit::test]
    fn checking_now_says_which_version_is_available(cx: &mut TestAppContext) {
        let started = with_banner(cx, true, Ok(Some("0.2.0")));
        click_label(&started, "Check now", cx);
        assert_eq!(
            status(&started, cx).as_deref(),
            Some("Version 0.2.0 is available")
        );
    }

    #[gpui_kit::test]
    fn checking_now_says_why_it_failed(cx: &mut TestAppContext) {
        let started = with_banner(cx, true, Err("connection refused"));
        click_label(&started, "Check now", cx);
        assert_eq!(
            status(&started, cx).as_deref(),
            Some(
                "Could not check for updates: The releases of the repository could not be asked: connection refused"
            )
        );
    }

    #[gpui_kit::test]
    fn the_button_waits_while_a_check_is_under_way(cx: &mut TestAppContext) {
        let started = with_banner(cx, true, Ok(None));
        let banner = cx.update(|cx| UpdateBanner::try_global(cx).expect("the banner is installed"));
        banner.update(cx, UpdateBanner::check_now);
        assert!(is_disabled(&started, "Check now", cx));
        assert_eq!(
            label_of(&started, "update-status", cx).as_deref(),
            Some("Checking for updates…")
        );
        cx.run_until_parked();
        assert!(!is_disabled(&started, "Check now", cx));
    }

    #[gpui_kit::test]
    fn a_build_that_does_not_update_itself_keeps_its_banner_silent(cx: &mut TestAppContext) {
        let started = with_banner(cx, false, Ok(Some("0.2.0")));
        testing::frame(cx, started.window, |window, _| {
            assert_eq!(window.find_by_label("Check now").disabled(), Some(true));
            assert_eq!(
                window.find("update-status").label(),
                Some("This build does not update itself")
            );
        });
    }
}

#[gpui_kit::test]
fn a_failure_to_save_the_settings_is_said_on_the_page(cx: &mut TestAppContext) {
    let started = start(cx, 1280.0);
    testing::frame(cx, started.window, |window, _| {
        assert!(window.try_find("settings-error").is_none());
    });
    started.appearance.update(cx, |appearance, cx| {
        appearance.fail(
            crate::entities::catalogue::CatalogueError::storage(std::io::Error::other("disk gone")),
            cx,
        );
    });
    cx.run_until_parked();
    testing::frame(cx, started.window, |window, _| {
        let line = window.find("settings-error");
        assert_eq!(line.role(), Some(Role::Status));
        let label = line.label().unwrap();
        assert!(
            label.starts_with(
                "Settings could not be saved: the catalogue database failed: disk gone."
            ),
            "{label}"
        );
    });
    // A change that is saved takes the words away.
    set(&started, cx, |settings| settings.text_size = 21.0);
    testing::frame(cx, started.window, |window, _| {
        assert!(window.try_find("settings-error").is_none());
    });
}
