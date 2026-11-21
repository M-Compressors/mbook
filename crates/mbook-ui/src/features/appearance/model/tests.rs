use std::cell::Cell;
use std::error::Error as _;
use std::rc::Rc;

use gpui_kit::{Entity, Hsla, TestAppContext, WindowAppearance};

use super::*;
use crate::testing::{self, Fixture};

/// An appearance over a library in a temporary root, with the Kit
/// initialised.
fn appearance(cx: &mut TestAppContext, fixture: &Fixture) -> Entity<Appearance> {
    cx.update(gpui_kit::init);
    fixture.install(cx);
    cx.new(Appearance::new)
}

/// An appearance beside a window `width` pixels wide.
fn in_window(
    cx: &mut TestAppContext,
    width: f32,
    height: f32,
) -> (gpui_kit::AnyWindowHandle, Entity<Appearance>) {
    let (window, _) = testing::open(cx, width, height, |_, _| testing::Blank);
    (window, cx.new(Appearance::new))
}

fn settings(appearance: &Entity<Appearance>, cx: &TestAppContext) -> Settings {
    appearance.read_with(cx, |appearance, _| appearance.settings().clone())
}

fn change(
    appearance: &Entity<Appearance>,
    cx: &mut TestAppContext,
    change: impl FnOnce(&mut Settings),
) {
    appearance.update(cx, |appearance, cx| appearance.update(change, cx));
}

#[gpui_kit::test]
fn the_settings_are_the_defaults_until_the_catalogue_answers(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture
        .catalogue
        .put_settings(&Settings {
            text_size: 22.0,
            ..Settings::default()
        })
        .unwrap();
    let appearance = appearance(cx, &fixture);
    assert!(!appearance.read_with(cx, |appearance, _| appearance.is_loaded()));
    assert_eq!(settings(&appearance, cx), Settings::default());
    cx.run_until_parked();
    assert!(appearance.read_with(cx, |appearance, _| appearance.is_loaded()));
    assert!((settings(&appearance, cx).text_size - 22.0).abs() < f32::EPSILON);
}

#[gpui_kit::test]
fn a_change_is_stored_in_the_catalogue_in_the_background(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let appearance = appearance(cx, &fixture);
    cx.run_until_parked();
    change(&appearance, cx, |settings| {
        settings.page_tone = Some(PageTone::Sepia);
        settings.text_size = 24.0;
    });
    assert_eq!(settings(&appearance, cx).page_tone, Some(PageTone::Sepia));
    assert_eq!(
        fixture.catalogue.settings().unwrap().page_tone,
        None,
        "the write waits for the background"
    );
    cx.run_until_parked();
    let stored = fixture.catalogue.settings().unwrap();
    assert_eq!(stored.page_tone, Some(PageTone::Sepia));
    assert!((stored.text_size - 24.0).abs() < f32::EPSILON);
}

#[gpui_kit::test]
fn values_out_of_range_are_brought_into_it(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let appearance = appearance(cx, &fixture);
    cx.run_until_parked();
    change(&appearance, cx, |settings| {
        settings.text_size = 99.0;
        settings.line_height = 0.5;
        settings.interface_scale = 5.0;
    });
    let now = settings(&appearance, cx);
    assert!((now.text_size - 32.0).abs() < f32::EPSILON);
    assert!((now.line_height - 1.2).abs() < f32::EPSILON);
    assert!((now.interface_scale - 2.0).abs() < f32::EPSILON);
    change(&appearance, cx, |settings| {
        settings.text_size = 1.0;
        settings.interface_scale = 0.1;
    });
    let now = settings(&appearance, cx);
    assert!((now.text_size - 12.0).abs() < f32::EPSILON);
    assert!((now.interface_scale - 1.0).abs() < f32::EPSILON);
}

#[gpui_kit::test]
fn a_value_that_is_no_number_keeps_what_it_was(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let appearance = appearance(cx, &fixture);
    cx.run_until_parked();
    change(&appearance, cx, |settings| {
        settings.text_size = f32::NAN;
        settings.line_height = f32::INFINITY;
    });
    let now = settings(&appearance, cx);
    assert!((now.text_size - 18.0).abs() < f32::EPSILON);
    assert!((now.line_height - 1.6).abs() < f32::EPSILON);
}

#[gpui_kit::test]
fn a_blank_typeface_keeps_the_previous_one(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let appearance = appearance(cx, &fixture);
    cx.run_until_parked();
    change(&appearance, cx, |settings| {
        settings.typeface = "  ".to_owned();
    });
    assert_eq!(settings(&appearance, cx).typeface, "Literata");
}

#[gpui_kit::test]
fn a_change_that_changes_nothing_tells_nobody(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let appearance = appearance(cx, &fixture);
    cx.run_until_parked();
    let told = Rc::new(Cell::new(0));
    let counter = told.clone();
    let _subscription =
        cx.update(|cx| cx.observe(&appearance, move |_, _| counter.set(counter.get() + 1)));
    change(&appearance, cx, |settings| settings.text_size = 18.0);
    assert_eq!(told.get(), 0);
    change(&appearance, cx, |settings| settings.text_size = 20.0);
    assert_eq!(told.get(), 1);
}

#[gpui_kit::test]
fn a_change_made_before_the_catalogue_answers_is_not_overwritten(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture
        .catalogue
        .put_settings(&Settings {
            text_size: 22.0,
            ..Settings::default()
        })
        .unwrap();
    let appearance = appearance(cx, &fixture);
    change(&appearance, cx, |settings| settings.text_size = 30.0);
    cx.run_until_parked();
    assert!((settings(&appearance, cx).text_size - 30.0).abs() < f32::EPSILON);
}

#[test]
fn a_chosen_page_tone_is_the_one_drawn() {
    for tone in PageTone::ALL {
        assert_eq!(resolve(Some(tone), WindowAppearance::Dark), tone);
        assert_eq!(resolve(Some(tone), WindowAppearance::Light), tone);
    }
}

#[test]
fn a_page_tone_that_follows_the_system_follows_the_window() {
    assert_eq!(resolve(None, WindowAppearance::Light), PageTone::Paper);
    assert_eq!(
        resolve(None, WindowAppearance::VibrantLight),
        PageTone::Paper
    );
    assert_eq!(resolve(None, WindowAppearance::Dark), PageTone::Night);
    assert_eq!(
        resolve(None, WindowAppearance::VibrantDark),
        PageTone::Night
    );
}

#[gpui_kit::test]
fn the_page_tone_and_the_chrome_are_those_of_the_settings(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture.install(cx);
    let (window, appearance) = in_window(cx, 800.0, 600.0);
    cx.run_until_parked();
    testing::frame(cx, window, |window, cx| {
        appearance.update(cx, |appearance, cx| {
            assert_eq!(
                appearance.page_tone(window),
                PageTone::Paper,
                "a light window"
            );
            appearance.update(|settings| settings.page_tone = Some(PageTone::Dusk), cx);
            assert_eq!(appearance.page_tone(window), PageTone::Dusk);
            assert_eq!(appearance.chrome(window), Chrome::DARK);
            appearance.update(|settings| settings.page_tone = Some(PageTone::Sepia), cx);
            assert_eq!(appearance.chrome(window), Chrome::LIGHT);
        });
    });
}

#[gpui_kit::test]
fn the_size_class_follows_the_width_of_the_window(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture.install(cx);
    for (width, class) in [
        (390.0, SizeClass::Compact),
        (800.0, SizeClass::Medium),
        (1280.0, SizeClass::Expanded),
    ] {
        let (window, appearance) = in_window(cx, width, 700.0);
        cx.run_until_parked();
        let found = testing::frame(cx, window, |window, cx| {
            appearance.read(cx).size_class(window)
        });
        assert_eq!(found, class, "{width} px");
    }
}

#[gpui_kit::test]
fn an_enlarged_interface_gives_a_narrower_size_class(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture.install(cx);
    let (window, appearance) = in_window(cx, 1100.0, 700.0);
    cx.run_until_parked();
    let class = |cx: &mut TestAppContext| {
        testing::frame(cx, window, |window, cx| {
            appearance.read(cx).size_class(window)
        })
    };
    assert_eq!(class(cx), SizeClass::Expanded);
    change(&appearance, cx, |settings| settings.interface_scale = 1.5);
    assert_eq!(class(cx), SizeClass::Medium);
    change(&appearance, cx, |settings| settings.interface_scale = 2.0);
    assert_eq!(class(cx), SizeClass::Compact);
}

#[gpui_kit::test]
fn observers_hear_of_a_resize_only_when_the_class_changes(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture.install(cx);
    let (window, appearance) = in_window(cx, 1100.0, 700.0);
    cx.run_until_parked();
    let told = Rc::new(Cell::new(0));
    let counter = told.clone();
    let _subscription =
        cx.update(|cx| cx.observe(&appearance, move |_, _| counter.set(counter.get() + 1)));
    let resize = |cx: &mut TestAppContext, width: f32| {
        cx.simulate_window_resize(
            window,
            gpui_kit::size(gpui_kit::px(width), gpui_kit::px(700.0)),
        );
        testing::frame(cx, window, |window, cx| {
            appearance.update(cx, |appearance, cx| appearance.observe_viewport(window, cx));
        });
    };
    resize(cx, 1000.0);
    assert_eq!(told.get(), 0, "expanded stays expanded");
    resize(cx, 500.0);
    assert_eq!(told.get(), 1, "expanded becomes compact");
}

#[gpui_kit::test]
fn the_theme_takes_the_colours_of_the_page_tone(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let appearance = appearance(cx, &fixture);
    cx.run_until_parked();
    let (dark, canvas) = cx.update(|cx| (cx.theme().is_dark(), cx.theme().background));
    assert!(!dark);
    assert_eq!(canvas, Hsla::from(Chrome::LIGHT.canvas));
    change(&appearance, cx, |settings| {
        settings.page_tone = Some(PageTone::Night);
    });
    let (dark, canvas, ring) =
        cx.update(|cx| (cx.theme().is_dark(), cx.theme().background, cx.theme().ring));
    assert!(dark);
    assert_eq!(canvas, Hsla::from(Chrome::DARK.canvas));
    assert_eq!(ring, Hsla::from(Chrome::DARK.accent));
}

#[gpui_kit::test]
fn a_page_tone_that_follows_the_system_follows_its_changes(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let appearance = appearance(cx, &fixture);
    cx.run_until_parked();
    appearance.update(cx, |appearance, cx| {
        appearance.set_system(WindowAppearance::Dark, cx);
    });
    assert!(cx.update(|cx| cx.theme().is_dark()));
    appearance.update(cx, |appearance, cx| {
        appearance.set_system(WindowAppearance::Light, cx);
    });
    assert!(!cx.update(|cx| cx.theme().is_dark()));
}

#[gpui_kit::test]
fn a_chosen_tone_ignores_the_system(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let appearance = appearance(cx, &fixture);
    cx.run_until_parked();
    change(&appearance, cx, |settings| {
        settings.page_tone = Some(PageTone::Paper);
    });
    appearance.update(cx, |appearance, cx| {
        appearance.set_system(WindowAppearance::Dark, cx);
    });
    assert!(!cx.update(|cx| cx.theme().is_dark()));
}

#[gpui_kit::test]
fn the_size_of_the_interface_is_the_rem_size_of_the_window(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture.install(cx);
    let (window, appearance) = in_window(cx, 800.0, 600.0);
    cx.run_until_parked();
    change(&appearance, cx, |settings| settings.interface_scale = 1.5);
    testing::frame(cx, window, |window, cx| {
        appearance.read(cx).sync_window(window);
        assert_eq!(window.rem_size(), gpui_kit::px(24.0));
    });
}

#[gpui_kit::test]
fn reduced_motion_is_set_when_the_setting_asks(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let appearance = appearance(cx, &fixture);
    cx.run_until_parked();
    assert!(!cx.update(|cx| cx.reduce_motion()));
    change(&appearance, cx, |settings| settings.reduce_motion = true);
    assert!(cx.update(|cx| cx.reduce_motion()));
}

#[gpui_kit::test]
fn the_stored_typeface_is_used_when_the_device_has_it(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let appearance = appearance(cx, &fixture);
    cx.run_until_parked();
    appearance.update(cx, |appearance, _| {
        appearance.families = Some(vec!["Georgia".to_owned(), "Literata".to_owned()]);
    });
    let font = cx.update(|cx| appearance.read(cx).reading_font(cx));
    assert_eq!(font, "Literata");
}

#[gpui_kit::test]
fn a_typeface_the_device_lacks_falls_back_to_the_font_of_the_interface(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let appearance = appearance(cx, &fixture);
    cx.run_until_parked();
    appearance.update(cx, |appearance, _| {
        appearance.families = Some(vec!["Georgia".to_owned()]);
    });
    let (font, interface) = cx.update(|cx| {
        (
            appearance.read(cx).reading_font(cx),
            cx.theme().font_family.clone(),
        )
    });
    assert_eq!(font, interface);
    assert_eq!(
        settings(&appearance, cx).typeface,
        "Literata",
        "the setting is kept"
    );
}

#[gpui_kit::test]
fn before_the_families_are_read_the_font_of_the_interface_is_used(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let appearance = appearance(cx, &fixture);
    appearance.update(cx, |appearance, _| appearance.families = None);
    let (font, interface) = cx.update(|cx| {
        (
            appearance.read(cx).reading_font(cx),
            cx.theme().font_family.clone(),
        )
    });
    assert_eq!(font, interface);
}

#[gpui_kit::test]
fn the_families_are_offered_sorted_and_without_repeats(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let appearance = appearance(cx, &fixture);
    cx.run_until_parked();
    let families = appearance.read_with(cx, |appearance, _| appearance.families().to_vec());
    let mut expected = families.clone();
    expected.sort_unstable();
    expected.dedup();
    assert_eq!(families, expected);
}

/// The contrast of two Kit colours, by the formula of WCAG 2.
fn contrast(a: Hsla, b: Hsla) -> f32 {
    let rgb = |colour: Hsla| {
        let colour = colour.to_rgb();
        // A channel of a colour is between 0 and 1; the rounded value fits a byte.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a channel is between 0 and 1, so the byte is in range"
        )]
        let byte = |channel: f32| (channel * 255.0).round() as u8;
        crate::shared::theme::Rgb(byte(colour.r), byte(colour.g), byte(colour.b))
    };
    rgb(a).contrast(rgb(b))
}

#[gpui_kit::test]
fn the_text_and_controls_of_the_theme_keep_their_contrast_in_both_appearances(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    let appearance = appearance(cx, &fixture);
    cx.run_until_parked();
    for tone in [PageTone::Paper, PageTone::Night] {
        change(&appearance, cx, |settings| settings.page_tone = Some(tone));
        cx.update(|cx| {
            let theme = cx.theme();
            let text = [
                (theme.foreground, theme.background),
                (theme.muted_foreground, theme.background),
                (theme.primary_foreground, theme.primary),
                (theme.primary_foreground, theme.primary_hover),
                (theme.primary_foreground, theme.primary_active),
                (theme.foreground, theme.popover),
                (theme.foreground, theme.accent),
                (theme.button_foreground, theme.button),
                (theme.link, theme.background),
            ];
            for (foreground, background) in text {
                assert!(contrast(foreground, background) >= 4.5, "{tone:?}");
            }
            let controls = [
                (theme.input, theme.background),
                (theme.ring, theme.background),
            ];
            for (edge, background) in controls {
                assert!(contrast(edge, background) >= 3.0, "{tone:?}");
            }
        });
    }
}

#[gpui_kit::test]
fn a_failure_of_the_catalogue_is_kept_until_a_write_succeeds(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let appearance = appearance(cx, &fixture);
    cx.run_until_parked();
    assert!(appearance.read_with(cx, |appearance, _| appearance.error().is_none()));
    appearance.update(cx, |appearance, cx| {
        appearance.fail(
            CatalogueError::storage(std::io::Error::other("disk gone")),
            cx,
        );
    });
    appearance.read_with(cx, |appearance, _| {
        assert!(appearance.error().is_some());
        assert!(appearance.error().unwrap().source().is_some());
    });
    change(&appearance, cx, |settings| settings.text_size = 20.0);
    cx.run_until_parked();
    assert!(
        appearance.read_with(cx, |appearance, _| appearance.error().is_none()),
        "the settings were saved after all"
    );
}
