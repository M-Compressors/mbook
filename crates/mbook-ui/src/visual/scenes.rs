//! The screens, one shot each: the state a screen is put in, and the name of
//! the file it is written to.

use std::time::Duration;

use gpui_kit::Action as _;
use gpui_kit::component::WindowExt;

use super::stage::Stage;
use crate::app::show_about;
use crate::entities::catalogue::Settings;
use crate::error::{Error, Result};
use crate::features::import_book::{ImportQueue, ImportStatus};
use crate::pages::library::{FindBooks, LibraryEvent};
use crate::shared::actions::About;

/// Which library a shot is taken over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Library {
    /// No book.
    Empty,
    /// Nine books.
    Books,
}

/// One screen in one state.
pub(super) struct Shot {
    /// The start of the name of the file: the number and what it shows.
    pub(super) name: &'static str,
    /// The library under it.
    pub(super) library: Library,
    /// Whether the screen is drawn with motion off. The settings are shown
    /// with it on.
    pub(super) calm: bool,
    /// Changes the settings the shot starts with.
    pub(super) settings: fn(&mut Settings),
    /// Puts the screen in its state.
    pub(super) run: fn(&mut Stage) -> Result<()>,
}

fn same(_: &mut Settings) {}

fn keep_controls(settings: &mut Settings) {
    settings.keep_controls = true;
}

const fn shot(name: &'static str, library: Library, run: fn(&mut Stage) -> Result<()>) -> Shot {
    Shot {
        name,
        library,
        calm: true,
        settings: same,
        run,
    }
}

/// Every shot, in the order of their files.
pub(super) fn all() -> Vec<Shot> {
    vec![
        shot("01-library-empty", Library::Empty, |_| Ok(())),
        shot("02-library-books", Library::Books, seed_imports),
        shot("03a-library-search", Library::Books, |stage| {
            type_in_search(stage, "mo")
        }),
        shot("03b-library-no-match", Library::Books, |stage| {
            type_in_search(stage, "zzz")
        }),
        shot("04a-library-menu", Library::Books, menu),
        shot("04b-library-remove", Library::Books, remove),
        Shot {
            settings: keep_controls,
            ..shot("05-reader-chapter", Library::Books, open_reader)
        },
        shot("06a-reader-hidden", Library::Books, reader_hidden),
        Shot {
            settings: keep_controls,
            ..shot("06b-reader-contents", Library::Books, |stage| {
                open_reader(stage)?;
                stage.click("contents")
            })
        },
        Shot {
            settings: keep_controls,
            ..shot("06c-reader-appearance", Library::Books, |stage| {
                open_reader(stage)?;
                stage.click("appearance")
            })
        },
        shot("07-reader-error", Library::Books, reader_error),
        shot("08a-editor-details", Library::Books, |stage| {
            open_editor(stage)?;
            open_section(stage, "section-details")
        }),
        shot("08b-editor-split", Library::Books, |stage| {
            open_editor(stage)?;
            open_chapter(stage)
        }),
        shot("08c-editor-source", Library::Books, |stage| {
            open_editor(stage)?;
            open_chapter(stage)?;
            stage.click_label("Source")
        }),
        shot("08d-editor-preview", Library::Books, |stage| {
            open_editor(stage)?;
            open_chapter(stage)?;
            stage.click_label("Preview")
        }),
        shot("08e-editor-pictures", Library::Books, |stage| {
            open_editor(stage)?;
            open_section(stage, "section-pictures")
        }),
        shot("08f-editor-problems", Library::Books, editor_problems),
        shot("08g-editor-unsaved", Library::Books, editor_unsaved),
        Shot {
            calm: false,
            ..shot("09a-settings-top", Library::Books, open_settings)
        },
        Shot {
            calm: false,
            ..shot("09b-settings-bottom", Library::Books, |stage| {
                open_settings(stage)?;
                stage.scroll("settings-page", -400.0)
            })
        },
        shot("11a-library-error", Library::Books, |stage| {
            stage.with_app(|cx| {
                crate::entities::catalogue::Library::global(cx).update(cx, |library, cx| {
                    library.fail(disk_gone(), cx);
                });
            });
            stage.settle()
        }),
        Shot {
            calm: false,
            ..shot("11b-settings-error", Library::Books, |stage| {
                open_settings(stage)?;
                stage.with_app(|cx| {
                    crate::features::appearance::Appearance::global(cx)
                        .update(cx, |appearance, cx| appearance.fail(disk_gone(), cx));
                });
                stage.settle()
            })
        },
        #[cfg(feature = "self-update")]
        shot("10a-update-banner", Library::Books, update_banner),
        shot("10b-about", Library::Books, |stage| {
            stage.with_window(|window, cx| show_about(&About, window, cx))?;
            stage.settle()
        }),
    ]
}

/// The failure the catalogue is made to report in the shots of errors.
fn disk_gone() -> crate::entities::catalogue::CatalogueError {
    crate::entities::catalogue::CatalogueError::storage(std::io::Error::other(
        "the disk is not there",
    ))
}

/// Puts a running import and a failed one in the list of imports.
fn seed_imports(stage: &mut Stage) -> Result<()> {
    stage.with_app(|cx| {
        if let Some(queue) = ImportQueue::try_global(cx) {
            queue.update(cx, |queue, cx| {
                queue.seed(vec![
                    (
                        "A Study in Scarlet.epub".to_owned(),
                        Some("A Study in Scarlet".to_owned()),
                        ImportStatus::Converting,
                    ),
                    (
                        "broken.fb2".to_owned(),
                        None,
                        ImportStatus::Failed {
                            message: "The file is not a book that mbook can read".to_owned(),
                        },
                    ),
                ]);
                cx.notify();
            });
        }
    });
    stage.settle()
}

/// Types words in the search field.
fn type_in_search(stage: &mut Stage, words: &str) -> Result<()> {
    stage.with_window(|window, cx| window.dispatch_action(FindBooks.boxed_clone(), cx))?;
    stage.settle()?;
    stage.type_text(words)
}

/// Opens the menu of the book that stands first on the shelf.
fn menu(stage: &mut Stage) -> Result<()> {
    let key = book(stage, 8)?;
    stage.click_named(&format!("more-{key}"))
}

fn remove(stage: &mut Stage) -> Result<()> {
    menu(stage)?;
    stage.click_label("Remove")
}

fn book(stage: &Stage, at: usize) -> Result<crate::shared::model::BookKey> {
    stage
        .keys()
        .get(at)
        .copied()
        .ok_or_else(|| Error::Render(format!("the library has no book number {at}")))
}

/// Asks the root for a page the way the library page does.
fn ask(stage: &mut Stage, event: LibraryEvent) -> Result<()> {
    let root = stage.root()?;
    stage.with_app(|cx| {
        if let Some(page) = root.read(cx).library_page().cloned() {
            page.update(cx, |_, cx| cx.emit(event));
        }
    });
    stage.settle()?;
    // The book is read, its chapter laid out and its pictures decoded.
    for _ in 0..4 {
        stage.settle()?;
    }
    Ok(())
}

fn open_reader(stage: &mut Stage) -> Result<()> {
    let key = book(stage, 0)?;
    ask(stage, LibraryEvent::Open(key))
}

fn reader_hidden(stage: &mut Stage) -> Result<()> {
    open_reader(stage)?;
    // The pointer rests over the text, away from the controls.
    stage.hover("reader-page")?;
    stage.wait(Duration::from_secs(6))
}

fn reader_error(stage: &mut Stage) -> Result<()> {
    let key = book(stage, 0)?;
    std::fs::remove_file(stage.roots().book(key))?;
    ask(stage, LibraryEvent::Open(key))
}

fn open_editor(stage: &mut Stage) -> Result<()> {
    let key = book(stage, 0)?;
    ask(stage, LibraryEvent::Edit(key))
}

/// Goes to a section of the editor, whatever the size of the window.
fn open_section(stage: &mut Stage, id: &'static str) -> Result<()> {
    // A compact window may be inside a chapter: back steps out to the sections.
    for _ in 0..3 {
        if stage.has(id)? {
            break;
        }
        if stage.has("open-sections")? {
            stage.click("open-sections")?;
        } else if stage.has("back")? {
            stage.click("back")?;
        }
    }
    stage.click(id)
}

/// Shows the text of the second chapter, whatever the size of the window.
fn open_chapter(stage: &mut Stage) -> Result<()> {
    if stage.has("chapter-source")? || stage.has("chapter-text")? {
        return Ok(());
    }
    if !stage.has("chapter-ch02.md")? {
        open_section(stage, "section-chapters")?;
    }
    stage.click("chapter-ch02.md")
}

/// Types in a chapter and asks to leave the editor.
fn editor_unsaved(stage: &mut Stage) -> Result<()> {
    open_editor(stage)?;
    open_chapter(stage)?;
    if stage.has("chapter-source")? {
        stage.click("chapter-source")?;
    }
    stage.type_text(" More words. ")?;
    for _ in 0..3 {
        stage.click("back")?;
        if stage.with_window(WindowExt::has_active_dialog)? {
            break;
        }
    }
    Ok(())
}

/// Breaks a link in a chapter and asks the editor to check the book.
fn editor_problems(stage: &mut Stage) -> Result<()> {
    open_editor(stage)?;
    open_chapter(stage)?;
    if stage.has("chapter-source")? {
        stage.click("chapter-source")?;
    }
    stage.type_text("\n\nSee [the gone page](nowhere.md).\n")?;
    open_section(stage, "section-problems")?;
    stage.click("check")
}

fn open_settings(stage: &mut Stage) -> Result<()> {
    ask(stage, LibraryEvent::Settings)
}

/// Shows the bar that offers a new version.
#[cfg(feature = "self-update")]
fn update_banner(stage: &mut Stage) -> Result<()> {
    use gpui_kit::AppContext as _;

    use crate::features::auto_update::UpdateBanner;

    let banner = stage.with_window(|window, cx| {
        let banner = cx.new(|cx| UpdateBanner::scripted(true, Ok(Some("0.2.0")), window, cx));
        UpdateBanner::install(banner.clone(), cx);
        banner
    })?;
    stage.with_app(|cx| banner.update(cx, UpdateBanner::check_now));
    let root = stage.root()?;
    stage.with_app(|cx| root.update(cx, |_, cx| cx.notify()));
    stage.settle()
}
