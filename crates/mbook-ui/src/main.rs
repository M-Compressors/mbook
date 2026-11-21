//! The desktop shell of the mbook application.
//!
//! It chooses the allocator, asks the platform where files are kept, and
//! hands over to [`mbook_ui::run`]. Everything else is in the library.

use std::path::PathBuf;
use std::process::ExitCode;

use mbook_ui::Roots;

#[global_allocator]
static GLOBAL: mimalloc3::MiMalloc = mimalloc3::MiMalloc;

/// The folder of the application inside the directories of the platform.
const FOLDER: &str = "mbook";

fn main() -> ExitCode {
    let Some(roots) = roots() else {
        // Without a place for the library there is nothing to start.
        return ExitCode::FAILURE;
    };
    mbook_ui::run(gpui_kit::application(), roots);
    ExitCode::SUCCESS
}

/// The data and cache directories of the platform, from `dirs`.
fn roots() -> Option<Roots> {
    let data: PathBuf = dirs::data_dir()?.join(FOLDER);
    let cache: PathBuf = dirs::cache_dir()?.join(FOLDER);
    Some(Roots::new(data, cache))
}
