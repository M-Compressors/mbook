//! Puts the icon of the application into its Windows executable.
//!
//! GPUI takes the icon of a window, of the taskbar and of the task switcher
//! from the resource with the identifier 1. The icon named under
//! `[package.metadata.bundle]` reaches the installer only.

use std::error::Error;
use std::io::{self, Write};

fn main() -> Result<(), Box<dyn Error>> {
    writeln!(io::stdout().lock(), "cargo::rerun-if-changed=build.rs")?;

    // The resource compiler exists on a Windows host only, so a build for
    // Windows from another system gets no icon.
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows::embed_icon()?;
    }
    Ok(())
}

#[cfg(windows)]
mod windows {
    use std::error::Error;
    use std::fs::File;
    use std::io::{self, BufWriter, Write};
    use std::path::{Path, PathBuf};

    use image::ExtendedColorType;
    use image::codecs::ico::{IcoEncoder, IcoFrame};
    use image::imageops::FilterType;

    /// The picture the icon is made from; the same as the icon of the bundle.
    const ICON_PNG: &str = "assets/icon.png";

    /// The sizes Windows asks for: title bar, taskbar, Explorer.
    const SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];

    /// Makes the `.ico` and compiles it into the executable.
    pub(crate) fn embed_icon() -> Result<(), Box<dyn Error>> {
        writeln!(io::stdout().lock(), "cargo::rerun-if-changed={ICON_PNG}")?;

        let ico = PathBuf::from(std::env::var("OUT_DIR")?).join("app.ico");
        write_ico(Path::new(ICON_PNG), &ico)?;

        let mut resource = winresource::WindowsResource::new();
        // `set_icon` stores the icon under the identifier 1, which GPUI loads.
        resource.set_icon(ico.to_str().ok_or("OUT_DIR is not UTF-8")?);
        // Shown by the task manager in place of the name of the file.
        resource.set("FileDescription", "mbook");
        resource.set("ProductName", "mbook");
        resource.compile()?;
        Ok(())
    }

    /// The `.ico` is not kept in the repository: it is made from the PNG.
    fn write_ico(png: &Path, ico: &Path) -> Result<(), Box<dyn Error>> {
        let picture = image::open(png)?;
        let frames = SIZES
            .iter()
            .map(|&side| {
                let frame = picture
                    .resize_exact(side, side, FilterType::Lanczos3)
                    .into_rgba8();
                IcoFrame::as_png(frame.as_raw(), side, side, ExtendedColorType::Rgba8)
            })
            .collect::<Result<Vec<_>, _>>()?;
        IcoEncoder::new(BufWriter::new(File::create(ico)?)).encode_images(&frames)?;
        Ok(())
    }
}
