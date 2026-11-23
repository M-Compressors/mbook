//! The desktop shell of the mbook application.
//!
//! It chooses the allocator, asks the platform where files are kept, and
//! hands over to [`mbook_ui::run`]. Everything else is in the library.
//!
//! On Android the application is not an executable: its activity loads the
//! library of `mbook-android`, which does the same there. This `main` is
//! empty on Android.

#[cfg(not(target_os = "android"))]
mod desktop {
    use std::path::PathBuf;
    use std::process::ExitCode;

    use mbook_ui::Roots;

    #[global_allocator]
    static GLOBAL: mimalloc3::MiMalloc = mimalloc3::MiMalloc;

    /// The folder of the application inside the directories of the platform.
    const FOLDER: &str = "mbook";

    pub(crate) fn main() -> ExitCode {
        let Some(roots) = roots() else {
            // Without a place for the library there is nothing to start.
            return ExitCode::FAILURE;
        };
        mbook_ui::run(gpui_kit::application(), roots, files());
        ExitCode::SUCCESS
    }

    /// The files named on the command line: a file manager starts the
    /// application this way to open a file on Windows and Linux. An option,
    /// such as the `-psn_…` an old macOS passes, is not a file.
    fn files() -> Vec<PathBuf> {
        std::env::args_os()
            .skip(1)
            .filter(|argument| !argument.to_string_lossy().starts_with('-'))
            .map(PathBuf::from)
            .collect()
    }

    /// The data and cache directories of the platform, from `dirs`.
    fn roots() -> Option<Roots> {
        let data: PathBuf = dirs::data_dir()?.join(FOLDER);
        let cache: PathBuf = dirs::cache_dir()?.join(FOLDER);
        Some(Roots::new(data, cache))
    }
}

#[cfg(not(target_os = "android"))]
fn main() -> std::process::ExitCode {
    desktop::main()
}

#[cfg(target_os = "android")]
fn main() {}
