//! The actions and keys of the editor.

use std::rc::Rc;

use gpui_kit::{App, Global, KeyBinding, KeyBindingContextPredicate, actions};

actions!(
    mbook_editor,
    [
        /// Saves the book.
        Save,
        /// Moves the focus on from the Markdown editor, after Escape.
        LeaveEditor,
        /// Moves the focus back from the Markdown editor, after Escape.
        LeaveEditorBackward,
    ]
);

/// The key context of the page.
pub(super) const PAGE_CONTEXT: &str = "EditorPage";
/// The key context of the editor's frame.
pub(super) const SOURCE_CONTEXT: &str = "EditorSource";
/// The key context of the editor's frame while Escape was pressed in it.
pub(super) const ESCAPED_CONTEXT: &str = "EditorEscaped";
/// Where Tab leaves the editor: inside the frame that Escape marked.
const ESCAPED_INPUT: &str = "EditorEscaped > Input";

/// Remembers that the keys are bound, so that a second editor does not bind
/// them again.
struct Bound;

impl Global for Bound {}

/// Binds the keys of the editor once for the application.
///
/// Save is the platform's Save key. Tab and Shift+Tab leave the Markdown
/// editor only after Escape, as RFC 0004 ("Accessibility") says; before it,
/// Tab indents.
pub(super) fn bind(cx: &mut App) {
    if cx.has_global::<Bound>() {
        return;
    }
    cx.set_global(Bound);
    let bindings = [
        (
            "secondary-s",
            Box::new(Save) as Box<dyn gpui_kit::Action>,
            PAGE_CONTEXT,
        ),
        ("tab", Box::new(LeaveEditor), ESCAPED_INPUT),
        ("shift-tab", Box::new(LeaveEditorBackward), ESCAPED_INPUT),
    ];
    let bindings: Vec<KeyBinding> = bindings
        .into_iter()
        .filter_map(|(keys, action, context)| {
            let predicate = KeyBindingContextPredicate::parse(context).ok()?;
            KeyBinding::load(keys, action, Some(Rc::new(predicate)), false, None, cx).ok()
        })
        .collect();
    cx.bind_keys(bindings);
}
