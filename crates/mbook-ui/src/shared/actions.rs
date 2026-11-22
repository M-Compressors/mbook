//! The actions of the application.
//!
//! Keys, menus and controls all name an action, and the key map of GPUI binds
//! them in one place (`app::keys`), so that a menu lists the key and a user can
//! change it. A view handles an action with `on_action`; one that dispatches an
//! action calls `window.dispatch_action`.

use gpui_kit::actions;

actions!(
    mbook,
    [
        /// Quits the application.
        Quit,
        /// Asks for files to add to the library.
        OpenFile,
        /// Saves the book open in the editor.
        Save,
        /// Looks for text on the current screen.
        Find,
        /// Goes to the settings.
        OpenSettings,
        /// Goes back to the previous screen. The root handles it unless the
        /// page in front handles it first, as the editor does to ask about
        /// unsaved changes.
        GoBack,
        /// Shows what the application is.
        About,
    ]
);
