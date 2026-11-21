//! Pictures of the screens, for looking at them.
//!
//! [`render_screens`] puts the application in a headless window, drives it
//! into each of its states and writes what the window shows to a PNG file.
//! Nothing here is part of the application: the module exists with the
//! `visual` feature, for `tests/visual.rs` and for a person who wants to see
//! the screens without a window on a desktop.

mod scenes;
mod stage;
mod world;

use std::fs;
use std::path::{Path, PathBuf};

use self::scenes::Library;
use self::stage::{Look, Stage};
use self::world::World;
use crate::error::{Error, Result};

/// The windows each screen is drawn in: a phone, a tablet and a desktop.
const SIZES: [(f32, f32); 3] = [(390.0, 844.0), (800.0, 780.0), (1280.0, 780.0)];

/// Whether this system can draw a window to a picture.
fn can_render() -> bool {
    matches!(gpui_kit::platform::current_headless_renderer(), Ok(Some(_)))
}

/// Draws every screen of the application to a PNG file in `out`, and returns
/// the files it wrote.
///
/// Each screen is drawn 390, 800 and 1280 pixels wide in the light
/// appearance, and 1280 pixels wide in the dark one. A file is named by the
/// number of the screen, what it shows, the width and the appearance:
/// `02-library-books-1280-light.png`. The library is made of books that are
/// written to a temporary folder; nothing outside it is read or changed, and
/// nothing is fetched.
///
/// A system without a headless renderer draws nothing: the result is empty.
///
/// This must be called on the main thread of the process, which the Metal
/// renderer requires.
///
/// # Errors
///
/// When a folder or a file cannot be written, when a book cannot be made, or
/// when a screen cannot be put in its state. The screens that could be drawn
/// are written before the first of them is reported.
pub fn render_screens(out: &Path) -> Result<Vec<PathBuf>> {
    if !can_render() {
        return Ok(Vec::new());
    }
    fs::create_dir_all(out)?;
    let empty = World::empty()?;
    let books = World::with_books()?;
    let mut written = Vec::new();
    let mut failures = Vec::new();
    for shot in scenes::all() {
        let world = match shot.library {
            Library::Empty => &empty,
            Library::Books => &books,
        };
        for (width, height) in SIZES {
            for dark in [false, true] {
                // The dark appearance is drawn at the width of a desktop only.
                if dark && width < 1000.0 {
                    continue;
                }
                let look = Look {
                    width,
                    height,
                    dark,
                    calm: shot.calm,
                };
                let path = out.join(format!("{}-{}.png", shot.name, look.suffix()));
                match one(world, look, &shot, &path) {
                    Ok(path) => written.push(path),
                    Err(error) => failures.push(format!("{}: {error}", path.display())),
                }
            }
        }
    }
    if failures.is_empty() {
        Ok(written)
    } else {
        Err(Error::Render(failures.join("; ")))
    }
}

/// Draws one screen in one window.
fn one(world: &World, look: Look, shot: &scenes::Shot, path: &Path) -> Result<PathBuf> {
    let mut stage = Stage::open(world, look, shot.settings)?;
    (shot.run)(&mut stage)?;
    stage.capture(path)
}
