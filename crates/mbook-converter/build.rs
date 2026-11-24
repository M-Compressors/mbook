//! Tells the compiler what the enabled formats have in common.
//!
//! Every format but plain text carries pictures: the picture codecs and the
//! code that uses them are compiled under `cfg(pictures)`. Some formats have
//! a table of contents of their own: the code that converts one is compiled
//! under `cfg(outlines)`. The lists of formats are written once, here.
//! With the `rayon` feature as well, the pictures are encoded on all cores at
//! once: that code is compiled under `cfg(parallel_pictures)`.

use std::env;
use std::io::{self, Write};

/// The features of the formats that can contain pictures.
const WITH_PICTURES: [&str; 6] = ["EPUB", "FB2", "PDF", "CBZ", "DOCX", "HTML"];
/// The features of the formats that have a table of contents of their own.
const WITH_OUTLINES: [&str; 2] = ["EPUB", "PDF"];

fn main() -> io::Result<()> {
    let mut cargo = io::stdout().lock();
    writeln!(cargo, "cargo::rustc-check-cfg=cfg(pictures)")?;
    writeln!(cargo, "cargo::rustc-check-cfg=cfg(outlines)")?;
    writeln!(cargo, "cargo::rustc-check-cfg=cfg(parallel_pictures)")?;
    let enabled = |feature: &&str| env::var_os(format!("CARGO_FEATURE_{feature}")).is_some();
    if WITH_PICTURES.iter().any(enabled) {
        writeln!(cargo, "cargo::rustc-cfg=pictures")?;
        if enabled(&"RAYON") {
            writeln!(cargo, "cargo::rustc-cfg=parallel_pictures")?;
        }
    }
    if WITH_OUTLINES.iter().any(enabled) {
        writeln!(cargo, "cargo::rustc-cfg=outlines")?;
    }
    writeln!(cargo, "cargo::rerun-if-changed=build.rs")
}
