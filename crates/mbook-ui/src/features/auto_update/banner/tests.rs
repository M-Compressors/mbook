//! The banner, with a scripted stand-in for the releases.

use std::cell::Cell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{
    AnyWindowHandle, Context, Entity, IntoElement, Render, Role, TestAppContext, Window, div,
};

use super::*;
use crate::testing::{self, Fixture};

/// What the stand-in answers, in order, and how often it was asked.
#[derive(Default)]
struct Script {
    checks: Mutex<VecDeque<Result<Option<Update>, UpdateError>>>,
    installs: Mutex<VecDeque<Result<Installed, UpdateError>>>,
    check_calls: AtomicUsize,
    install_calls: AtomicUsize,
}

impl Script {
    fn answering_checks(answers: Vec<Result<Option<Update>, UpdateError>>) -> Arc<Self> {
        let script = Self::default();
        script.checks.lock().unwrap().extend(answers);
        Arc::new(script)
    }

    fn then_installs(self: &Arc<Self>, answers: Vec<Result<Installed, UpdateError>>) {
        self.installs.lock().unwrap().extend(answers);
    }

    fn backend(self: &Arc<Self>, enabled: bool) -> Backend {
        let (checks, installs) = (self.clone(), self.clone());
        Backend {
            enabled,
            check: Arc::new(move || {
                checks.check_calls.fetch_add(1, Ordering::SeqCst);
                checks
                    .checks
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or(Ok(None))
            }),
            install: Arc::new(move |_, _| {
                installs.install_calls.fetch_add(1, Ordering::SeqCst);
                installs
                    .installs
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap_or(Ok(Installed::Restart))
            }),
        }
    }

    fn checked(&self) -> usize {
        self.check_calls.load(Ordering::SeqCst)
    }

    fn installed(&self) -> usize {
        self.install_calls.load(Ordering::SeqCst)
    }
}

fn update(version: &str) -> Update {
    Update {
        version: version.to_owned(),
        archive: format!("mbook-v{version}.tar.gz"),
        url: format!("https://example.com/mbook-v{version}.tar.gz"),
    }
}

fn offline() -> UpdateError {
    UpdateError::Releases("connection refused".into())
}

struct Host(Entity<UpdateBanner>);

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<'_, Self>) -> impl IntoElement {
        div().w_full().child(self.0.clone())
    }
}

struct Started {
    window: AnyWindowHandle,
    banner: Entity<UpdateBanner>,
    busy: Rc<Cell<bool>>,
}

fn start(cx: &mut TestAppContext, backend: Backend) -> Started {
    let busy = Rc::new(Cell::new(false));
    let asked = busy.clone();
    let (window, host) = testing::open(cx, 600.0, 480.0, |window, cx| {
        let banner = cx.new(|cx| {
            UpdateBanner::with_backend(
                backend,
                Roots::new("/nowhere/data", "/nowhere/cache"),
                Box::new(move |_| asked.get()),
                window,
                cx,
            )
        });
        Host(banner)
    });
    let banner = host.read_with(cx, |host, _| host.0.clone());
    Started {
        window,
        banner,
        busy,
    }
}

fn state(started: &Started, cx: &TestAppContext) -> UpdateState {
    started
        .banner
        .read_with(cx, |banner, _| banner.state().clone())
}

fn click(started: &Started, id: &str, cx: &mut TestAppContext) {
    let id = id.to_owned();
    testing::frame(cx, started.window, move |window, cx| window.click(id, cx));
    cx.run_until_parked();
}

fn notifications(started: &Started, cx: &mut TestAppContext) -> usize {
    testing::frame(cx, started.window, |window, cx| {
        window.notifications(cx).len()
    })
}

#[test]
fn nothing_is_said_before_anything_is_asked() {
    assert_eq!(UpdateState::Idle.summary(), "");
    assert!(!UpdateState::Idle.is_shown());
}

#[test]
fn each_state_says_where_the_update_is() {
    let found = update("0.2.0");
    assert_eq!(UpdateState::Checking.summary(), "Checking for updates…");
    assert_eq!(UpdateState::UpToDate.summary(), "mbook is up to date");
    assert_eq!(
        UpdateState::Available(found.clone()).summary(),
        "Version 0.2.0 is available"
    );
    assert_eq!(
        UpdateState::Waiting(found.clone()).summary(),
        "The update will be installed when the import or the edit is finished"
    );
    assert_eq!(
        UpdateState::Installing(found.clone()).summary(),
        "Downloading and verifying…"
    );
    assert_eq!(
        UpdateState::Installed(found.clone()).summary(),
        "Restart to finish"
    );
    assert_eq!(
        UpdateState::Failed {
            reason: "No network".into(),
            retry: Retry::Check
        }
        .summary(),
        "Could not check for updates: No network"
    );
    assert_eq!(
        UpdateState::Failed {
            reason: "Bad signature".into(),
            retry: Retry::Install(found)
        }
        .summary(),
        "The update failed: Bad signature"
    );
}

#[test]
fn a_reason_names_every_cause_and_starts_with_a_capital() {
    assert_eq!(
        reason(&offline()),
        "The releases of the repository could not be asked: connection refused"
    );
    assert_eq!(
        reason(&UpdateError::Unsupported),
        "Updates cannot be installed on this platform"
    );
}

#[gpui_kit::test]
fn a_build_that_does_not_update_itself_never_asks(cx: &mut TestAppContext) {
    let script = Script::answering_checks(vec![Ok(Some(update("0.2.0")))]);
    let started = start(cx, script.backend(false));
    started.banner.update(cx, UpdateBanner::check_now);
    cx.run_until_parked();
    assert_eq!(script.checked(), 0);
    assert_eq!(state(&started, cx), UpdateState::Idle);
    assert!(
        !started
            .banner
            .read_with(cx, |banner, _| banner.is_enabled())
    );
}

#[gpui_kit::test]
fn the_start_asks_once_and_offers_a_newer_version(cx: &mut TestAppContext) {
    let script = Script::answering_checks(vec![Ok(Some(update("0.2.0")))]);
    let started = start(cx, script.backend(true));
    cx.run_until_parked();
    assert_eq!(script.checked(), 1);
    assert_eq!(state(&started, cx), UpdateState::Available(update("0.2.0")));
    testing::frame(cx, started.window, |window, _| {
        let bar = window.find("update-banner");
        assert_eq!(bar.role(), Some(Role::Region));
        assert_eq!(bar.label(), Some("Application update"));
        assert_eq!(
            window.find("update-message").label(),
            Some("Version 0.2.0 is available")
        );
        assert_eq!(window.find_by_label("Update").role(), Some(Role::Button));
        assert_eq!(window.find_by_label("Later").role(), Some(Role::Button));
    });
}

#[gpui_kit::test]
fn an_update_that_became_available_is_announced(cx: &mut TestAppContext) {
    let script = Script::answering_checks(vec![Ok(Some(update("0.2.0")))]);
    let started = start(cx, script.backend(true));
    cx.run_until_parked();
    assert_eq!(notifications(&started, cx), 1);
}

#[gpui_kit::test]
fn the_same_version_is_not_announced_again_by_the_next_day(cx: &mut TestAppContext) {
    let script =
        Script::answering_checks(vec![Ok(Some(update("0.2.0"))), Ok(Some(update("0.2.0")))]);
    let started = start(cx, script.backend(true));
    cx.run_until_parked();
    // An answer to a check on the next day finds the update still offered.
    started.banner.update(cx, |banner, cx| {
        banner.state = UpdateState::UpToDate;
        banner.begin_check(false, cx);
    });
    cx.run_until_parked();
    assert_eq!(script.checked(), 2);
    assert_eq!(notifications(&started, cx), 1);
}

#[gpui_kit::test]
fn a_check_is_made_again_a_day_later(cx: &mut TestAppContext) {
    let script = Script::answering_checks(vec![Ok(None), Ok(Some(update("0.3.0")))]);
    let started = start(cx, script.backend(true));
    cx.run_until_parked();
    assert_eq!(state(&started, cx), UpdateState::UpToDate);
    cx.executor().advance_clock(CHECK_EVERY);
    cx.run_until_parked();
    assert_eq!(script.checked(), 2);
    assert_eq!(state(&started, cx), UpdateState::Available(update("0.3.0")));
}

#[gpui_kit::test]
fn nothing_newer_shows_no_bar(cx: &mut TestAppContext) {
    let script = Script::answering_checks(vec![Ok(None)]);
    let started = start(cx, script.backend(true));
    cx.run_until_parked();
    assert_eq!(state(&started, cx), UpdateState::UpToDate);
    assert!(
        !started
            .banner
            .read_with(cx, |banner, _| banner.is_visible())
    );
    testing::frame(cx, started.window, |window, _| {
        assert!(window.try_find("update-banner").is_none());
    });
}

#[gpui_kit::test]
fn a_check_that_fails_on_its_own_is_not_shown_in_the_bar(cx: &mut TestAppContext) {
    let script = Script::answering_checks(vec![Err(offline())]);
    let started = start(cx, script.backend(true));
    cx.run_until_parked();
    assert_eq!(
        state(&started, cx).summary(),
        "Could not check for updates: The releases of the repository could not be asked: connection refused"
    );
    assert!(
        !started
            .banner
            .read_with(cx, |banner, _| banner.is_visible())
    );
}

#[gpui_kit::test]
fn a_check_does_not_start_while_another_is_under_way(cx: &mut TestAppContext) {
    let script = Script::answering_checks(vec![Ok(None), Ok(None)]);
    let started = start(cx, script.backend(true));
    started.banner.update(cx, |banner, cx| {
        banner.check_now(cx);
        banner.check_now(cx);
    });
    cx.run_until_parked();
    assert_eq!(script.checked(), 1);
}

#[gpui_kit::test]
fn the_user_can_check_again_after_a_check_ended(cx: &mut TestAppContext) {
    let script = Script::answering_checks(vec![Ok(None), Ok(Some(update("0.2.0")))]);
    let started = start(cx, script.backend(true));
    cx.run_until_parked();
    started.banner.update(cx, UpdateBanner::check_now);
    cx.run_until_parked();
    assert_eq!(script.checked(), 2);
    assert_eq!(state(&started, cx), UpdateState::Available(update("0.2.0")));
}

#[gpui_kit::test]
fn the_answer_of_a_check_that_was_replaced_is_dropped(cx: &mut TestAppContext) {
    let script = Script::answering_checks(vec![Ok(None)]);
    let started = start(cx, script.backend(true));
    cx.run_until_parked();
    started.banner.update(cx, |banner, cx| {
        let stale = banner.generation - 1;
        banner.checked(stale, false, Ok(Some(update("9.9.9"))), cx);
    });
    assert_eq!(state(&started, cx), UpdateState::UpToDate);
}

#[gpui_kit::test]
fn later_hides_the_bar_until_the_next_check(cx: &mut TestAppContext) {
    let script =
        Script::answering_checks(vec![Ok(Some(update("0.2.0"))), Ok(Some(update("0.2.0")))]);
    let started = start(cx, script.backend(true));
    cx.run_until_parked();
    click(&started, "update-later", cx);
    assert!(
        !started
            .banner
            .read_with(cx, |banner, _| banner.is_visible())
    );
    cx.executor().advance_clock(CHECK_EVERY);
    cx.run_until_parked();
    assert!(
        started
            .banner
            .read_with(cx, |banner, _| banner.is_visible())
    );
}

#[gpui_kit::test]
fn updating_installs_and_offers_the_restart(cx: &mut TestAppContext) {
    let script = Script::answering_checks(vec![Ok(Some(update("0.2.0")))]);
    script.then_installs(vec![Ok(Installed::Restart)]);
    let started = start(cx, script.backend(true));
    cx.run_until_parked();
    click(&started, "update-install", cx);
    assert_eq!(script.installed(), 1);
    assert_eq!(state(&started, cx), UpdateState::Installed(update("0.2.0")));
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(
            window.find("update-message").label(),
            Some("Restart to finish")
        );
        assert_eq!(window.find_by_label("Restart").role(), Some(Role::Button));
        assert!(window.try_find("update-install").is_none());
    });
    assert_eq!(
        notifications(&started, cx),
        2,
        "the offer and the end of the install"
    );
}

#[gpui_kit::test]
fn the_bar_says_that_it_downloads_while_it_does(cx: &mut TestAppContext) {
    let script = Script::answering_checks(vec![Ok(Some(update("0.2.0")))]);
    let started = start(cx, script.backend(true));
    cx.run_until_parked();
    started.banner.update(cx, UpdateBanner::begin_install);
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(
            window.find("update-message").label(),
            Some("Downloading and verifying…")
        );
        assert!(window.try_find("update-install").is_none());
    });
}

#[gpui_kit::test]
fn an_installation_that_installer_quits_leaves_the_application(cx: &mut TestAppContext) {
    let script = Script::answering_checks(vec![Ok(Some(update("0.2.0")))]);
    script.then_installs(vec![Ok(Installed::Quit)]);
    let started = start(cx, script.backend(true));
    cx.run_until_parked();
    click(&started, "update-install", cx);
    assert_eq!(script.installed(), 1);
}

#[gpui_kit::test]
fn an_installation_waits_while_an_import_is_running(cx: &mut TestAppContext) {
    let script = Script::answering_checks(vec![Ok(Some(update("0.2.0")))]);
    let started = start(cx, script.backend(true));
    cx.run_until_parked();
    started.busy.set(true);
    click(&started, "update-install", cx);
    assert_eq!(script.installed(), 0);
    assert_eq!(state(&started, cx), UpdateState::Waiting(update("0.2.0")));
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(
            window.find("update-message").label(),
            Some("The update will be installed when the import or the edit is finished")
        );
    });

    cx.executor().advance_clock(BUSY_RETRY);
    cx.run_until_parked();
    assert_eq!(script.installed(), 0, "still busy");

    started.busy.set(false);
    cx.executor().advance_clock(BUSY_RETRY);
    cx.run_until_parked();
    assert_eq!(script.installed(), 1);
    assert_eq!(state(&started, cx), UpdateState::Installed(update("0.2.0")));
}

#[gpui_kit::test]
fn a_restart_waits_while_the_application_is_busy(cx: &mut TestAppContext) {
    let script = Script::answering_checks(vec![Ok(Some(update("0.2.0")))]);
    let started = start(cx, script.backend(true));
    cx.run_until_parked();
    click(&started, "update-install", cx);
    assert_eq!(state(&started, cx), UpdateState::Installed(update("0.2.0")));
    let before = notifications(&started, cx);

    // An import started, or a book was opened in the editor, meanwhile.
    started.busy.set(true);
    click(&started, "update-restart", cx);

    // The application is still here, was told why, and can be asked again.
    assert_eq!(state(&started, cx), UpdateState::Installed(update("0.2.0")));
    assert_eq!(notifications(&started, cx), before + 1);
    assert!(RESTART_WAITS.contains("close the editor"));
}

#[gpui_kit::test]
fn the_restart_button_restarts_an_idle_application(cx: &mut TestAppContext) {
    let script = Script::answering_checks(vec![Ok(Some(update("0.2.0")))]);
    let started = start(cx, script.backend(true));
    cx.run_until_parked();
    click(&started, "update-install", cx);

    let mut restart = cx.expect_restart();
    click(&started, "update-restart", cx);

    assert!(matches!(restart.try_recv(), Ok(Some(_))));
}

#[gpui_kit::test]
fn a_failed_installation_shows_its_reason_and_can_be_tried_again(cx: &mut TestAppContext) {
    let script = Script::answering_checks(vec![Ok(Some(update("0.2.0")))]);
    script.then_installs(vec![
        Err(UpdateError::Signature("wrong key".into())),
        Ok(Installed::Restart),
    ]);
    let started = start(cx, script.backend(true));
    cx.run_until_parked();
    click(&started, "update-install", cx);
    testing::frame(cx, started.window, |window, _| {
        assert_eq!(
            window.find("update-message").label(),
            Some("The update failed: The signature of the update is not valid: wrong key")
        );
        assert_eq!(window.find_by_label("Try again").role(), Some(Role::Button));
    });

    click(&started, "update-retry", cx);
    assert_eq!(script.installed(), 2);
    assert_eq!(state(&started, cx), UpdateState::Installed(update("0.2.0")));
}

#[gpui_kit::test]
fn try_again_after_a_failed_check_asks_the_releases_again(cx: &mut TestAppContext) {
    let script = Script::answering_checks(vec![Err(offline()), Ok(None)]);
    let started = start(cx, script.backend(true));
    cx.run_until_parked();
    started.banner.update(cx, UpdateBanner::try_again);
    cx.run_until_parked();
    assert_eq!(script.checked(), 2);
    assert_eq!(state(&started, cx), UpdateState::UpToDate);
}

#[gpui_kit::test]
fn try_again_with_nothing_failed_does_nothing(cx: &mut TestAppContext) {
    let script = Script::answering_checks(vec![Ok(None)]);
    let started = start(cx, script.backend(true));
    cx.run_until_parked();
    started.banner.update(cx, UpdateBanner::try_again);
    cx.run_until_parked();
    assert_eq!(script.checked(), 1);
}

#[gpui_kit::test]
fn installing_with_nothing_on_offer_does_nothing(cx: &mut TestAppContext) {
    let script = Script::answering_checks(vec![Ok(None)]);
    let started = start(cx, script.backend(true));
    cx.run_until_parked();
    started.banner.update(cx, UpdateBanner::begin_install);
    cx.run_until_parked();
    assert_eq!(script.installed(), 0);
}

#[gpui_kit::test]
fn every_control_of_the_bar_has_a_role_and_a_name(cx: &mut TestAppContext) {
    let script = Script::answering_checks(vec![Ok(Some(update("0.2.0")))]);
    let started = start(cx, script.backend(true));
    cx.run_until_parked();
    testing::frame(cx, started.window, |window, _| {
        for element in window.elements() {
            if format!("{:?}", element.path()).contains("notification") {
                // The stack of notifications is the Kit's own.
                continue;
            }
            assert!(element.role().is_some(), "no role: {element:?}");
            if element.role() == Some(Role::Button) {
                assert!(
                    element.label().is_some_and(|label| !label.is_empty()),
                    "a button without a name: {element:?}"
                );
            }
        }
    });
}

/// A catalogue and the settings over it, for the tests of the setting.
fn with_settings(cx: &mut TestAppContext) -> (Fixture, Entity<Appearance>) {
    let fixture = Fixture::new();
    cx.update(gpui_kit::init);
    fixture.install(cx);
    let appearance = cx.update(|cx| {
        let appearance = cx.new(Appearance::new);
        Appearance::install(appearance.clone(), cx);
        appearance
    });
    cx.run_until_parked();
    (fixture, appearance)
}

#[gpui_kit::test]
fn the_setting_off_stops_the_checks_of_the_day(cx: &mut TestAppContext) {
    let (_fixture, appearance) = with_settings(cx);
    appearance.update(cx, |appearance, cx| {
        appearance.update(|settings| settings.check_updates = false, cx);
    });
    let script = Script::answering_checks(vec![Ok(None)]);
    let started = start(cx, script.backend(true));
    cx.run_until_parked();
    assert_eq!(script.checked(), 0);
    cx.executor().advance_clock(CHECK_EVERY);
    cx.run_until_parked();
    assert_eq!(script.checked(), 0);
    started.banner.update(cx, UpdateBanner::check_now);
    cx.run_until_parked();
    assert_eq!(script.checked(), 1, "a check the user asks for is made");
}

#[gpui_kit::test]
fn turning_the_setting_on_checks_at_once(cx: &mut TestAppContext) {
    let (_fixture, appearance) = with_settings(cx);
    appearance.update(cx, |appearance, cx| {
        appearance.update(|settings| settings.check_updates = false, cx);
    });
    let script = Script::answering_checks(vec![Ok(None)]);
    let _started = start(cx, script.backend(true));
    cx.run_until_parked();
    assert_eq!(script.checked(), 0);
    appearance.update(cx, |appearance, cx| {
        appearance.update(|settings| settings.check_updates = true, cx);
    });
    cx.run_until_parked();
    assert_eq!(script.checked(), 1);
}

#[gpui_kit::test]
fn the_banner_is_reached_through_its_global(cx: &mut TestAppContext) {
    let script = Script::answering_checks(vec![Ok(None)]);
    let started = start(cx, script.backend(true));
    cx.update(|cx| {
        assert!(UpdateBanner::try_global(cx).is_none());
        UpdateBanner::install(started.banner.clone(), cx);
        assert_eq!(
            UpdateBanner::try_global(cx).map(|banner| banner.entity_id()),
            Some(started.banner.entity_id())
        );
    });
}
