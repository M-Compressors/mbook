//! Keys and menus.
//!
//! Every key goes through the key map of GPUI, so that a menu shows it and it
//! can be changed in one place. `secondary-` is the platform's modifier:
//! Command on macOS, Control elsewhere.

use gpui_kit::component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
use gpui_kit::{
    Action, App, KeyBinding, Menu, MenuItem, OperatingSystem, OsAction, SystemMenuType,
};

use crate::shared::actions::{About, Find, GoBack, OpenFile, OpenSettings, Quit, Save};

/// The name of the application in its menus.
const NAME: &str = "mbook";

/// Binds the keys, makes the menus and handles the actions the application
/// owns whatever window is in front.
pub(super) fn install(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
    bind(cx);
    if cx.platform_info().os == OperatingSystem::Mac {
        cx.set_menus(menus());
    }
}

/// What a key does.
fn table(os: OperatingSystem) -> Vec<(&'static str, Box<dyn Action>)> {
    let (back, save) = if os == OperatingSystem::Mac {
        ("cmd-[", "cmd-s")
    } else {
        ("alt-left", "ctrl-s")
    };
    vec![
        ("secondary-q", Box::new(Quit)),
        ("secondary-o", Box::new(OpenFile)),
        (save, Box::new(Save)),
        (
            if os == OperatingSystem::Mac {
                "cmd-f"
            } else {
                "ctrl-f"
            },
            Box::new(Find),
        ),
        ("secondary-,", Box::new(OpenSettings)),
        ("escape", Box::new(GoBack)),
        (back, Box::new(GoBack)),
    ]
}

/// Binds the keys of the application.
pub(crate) fn bind(cx: &mut App) {
    cx.bind_keys(bindings(cx));
}

/// The bindings for this platform. A key the platform cannot parse is left
/// out; the controls and menus still reach its action.
fn bindings(cx: &App) -> Vec<KeyBinding> {
    table(cx.platform_info().os)
        .into_iter()
        .filter_map(|(keys, action)| KeyBinding::load(keys, action, None, false, None, cx).ok())
        .collect()
}

/// The menu bar of macOS: the application, File and Edit.
fn menus() -> Vec<Menu> {
    vec![
        Menu::new(NAME).items([
            MenuItem::action(format!("About {NAME}"), About),
            MenuItem::separator(),
            MenuItem::action("Settings…", OpenSettings),
            MenuItem::separator(),
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action(format!("Quit {NAME}"), Quit),
        ]),
        Menu::new("File").items([
            MenuItem::action("Add a book…", OpenFile),
            MenuItem::separator(),
            MenuItem::action("Save", Save),
        ]),
        Menu::new("Edit").items([
            MenuItem::os_action("Undo", Undo, OsAction::Undo),
            MenuItem::os_action("Redo", Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action("Cut", Cut, OsAction::Cut),
            MenuItem::os_action("Copy", Copy, OsAction::Copy),
            MenuItem::os_action("Paste", Paste, OsAction::Paste),
            MenuItem::os_action("Select all", SelectAll, OsAction::SelectAll),
            MenuItem::separator(),
            MenuItem::action("Find…", Find),
        ]),
    ]
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Keystroke, TestAppContext};

    use super::*;

    #[test]
    fn every_platform_goes_back_with_escape() {
        for os in [
            OperatingSystem::Mac,
            OperatingSystem::Windows,
            OperatingSystem::Linux,
        ] {
            assert!(
                table(os).iter().any(|(keys, _)| *keys == "escape"),
                "{os:?}"
            );
        }
    }

    #[test]
    fn the_back_key_of_the_platform_differs() {
        let keys = |os| {
            table(os)
                .into_iter()
                .filter(|(_, action)| action.partial_eq(&GoBack))
                .map(|(keys, _)| keys)
                .collect::<Vec<_>>()
        };
        assert!(keys(OperatingSystem::Mac).contains(&"cmd-["));
        assert!(keys(OperatingSystem::Windows).contains(&"alt-left"));
        assert!(keys(OperatingSystem::Linux).contains(&"alt-left"));
    }

    #[test]
    fn save_uses_command_on_mac_and_control_on_windows_and_linux() {
        for (os, expected) in [
            (OperatingSystem::Mac, "cmd-s"),
            (OperatingSystem::Windows, "ctrl-s"),
            (OperatingSystem::Linux, "ctrl-s"),
        ] {
            let binding = table(os)
                .into_iter()
                .find(|(_, action)| action.partial_eq(&Save))
                .map(|(key, _)| key);
            assert_eq!(binding, Some(expected), "{os:?}");
        }
    }

    #[test]
    fn find_uses_command_on_mac_and_control_on_windows_and_linux() {
        for (os, expected) in [
            (OperatingSystem::Mac, "cmd-f"),
            (OperatingSystem::Windows, "ctrl-f"),
            (OperatingSystem::Linux, "ctrl-f"),
        ] {
            let binding = table(os)
                .into_iter()
                .find(|(_, action)| action.partial_eq(&Find))
                .map(|(key, _)| key);
            assert_eq!(binding, Some(expected), "{os:?}");
        }
    }

    #[test]
    fn the_menu_bar_has_the_application_file_and_edit() {
        let names: Vec<_> = menus().into_iter().map(|menu| menu.name).collect();
        assert_eq!(names, ["mbook", "File", "Edit"]);
    }

    #[test]
    fn the_file_menu_uses_the_save_action() {
        let file = menus()
            .into_iter()
            .find(|menu| menu.name == "File")
            .unwrap();
        assert!(file.items.iter().any(|item| matches!(
            item,
            MenuItem::Action { name, action, .. } if name == "Save" && action.partial_eq(&Save)
        )));
    }

    #[gpui_kit::test]
    fn every_key_of_the_table_is_bound(cx: &mut TestAppContext) {
        let (bound, wanted) = cx.update(|cx| {
            let wanted = table(cx.platform_info().os).len();
            (bindings(cx).len(), wanted)
        });
        assert_eq!(bound, wanted);
    }

    #[gpui_kit::test]
    fn the_keys_reach_their_actions(cx: &mut TestAppContext) {
        cx.update(|cx| {
            bind(cx);
            let find = |keystroke: Keystroke, action: &dyn Action, cx: &App| {
                cx.all_bindings_for_input(&[keystroke])
                    .iter()
                    .any(|binding| binding.action().partial_eq(action))
            };
            let mac = cx.platform_info().os == OperatingSystem::Mac;
            let key = |mac_keys: &'static str, other: &'static str| {
                Keystroke::parse(if mac { mac_keys } else { other }).unwrap()
            };
            assert!(find(Keystroke::parse("escape").unwrap(), &GoBack, cx));
            assert!(find(key("cmd-o", "ctrl-o"), &OpenFile, cx));
            assert!(find(key("cmd-s", "ctrl-s"), &Save, cx));
            assert!(find(key("cmd-f", "ctrl-f"), &Find, cx));
            assert!(find(key("cmd-,", "ctrl-,"), &OpenSettings, cx));
            assert!(find(key("cmd-q", "ctrl-q"), &Quit, cx));
        });
    }
}
