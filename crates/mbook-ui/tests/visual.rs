//! Draws every screen of the application to a PNG file in `target/visual/` of
//! the workspace, with a headless renderer, for a person to look at.
//!
//! Run it with `cargo test -p mbook-ui --features visual --test visual`.
//!
//! The renderer of macOS needs the main thread, which the harness of tests
//! does not give to a test: this test has a `main` of its own.

use std::error::Error;
use std::io::{self, Write as _};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/visual");
    let files = mbook_ui::visual::render_screens(&out)?;
    let mut stdout = io::stdout().lock();
    if files.is_empty() {
        writeln!(
            stdout,
            "visual: skipped, this system has no headless renderer"
        )?;
    }
    for file in files {
        writeln!(stdout, "visual: {}", file.display())?;
    }
    Ok(())
}
