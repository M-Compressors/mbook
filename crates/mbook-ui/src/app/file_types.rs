//! The kinds of file the application tells the system it opens
//! (mbook-ui docs, "Opening files from the system").
//!
//! macOS reads them from the `Info.plist` of the bundle
//! (`assets/macos/document-types.plist`), and Android from the manifest of
//! the APK. Windows has neither: the installer cargo-bundle makes registers
//! no file type, so the application registers itself for the user who runs
//! it, each time it starts. It owns `.mbook`; for every other format it is
//! added to "Open with" and the program the user chose stays the default.

#[cfg(any(windows, test))]
use crate::features::import_book::BOOK_FILE_EXTENSIONS;

/// The programmatic identifier of a book for Windows.
#[cfg(any(windows, test))]
const PROG_ID: &str = "mbook.Book";

/// The folder of the classes of the current user, under which Windows looks
/// for file types and programs.
#[cfg(any(windows, test))]
const CLASSES: &str = r"Software\Classes";

/// One value to write under the classes of the current user: the path of
/// its key, the name of the value (empty for the default value) and the
/// value.
#[cfg(any(windows, test))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    key: String,
    name: String,
    value: String,
}

#[cfg(any(windows, test))]
impl Entry {
    fn new(key: impl Into<String>, name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: format!(r"{CLASSES}\{}", key.into()),
            name: name.into(),
            value: value.into(),
        }
    }
}

/// What registers the executable at `exe` for the book formats.
#[cfg(any(windows, test))]
fn entries(exe: &str) -> Vec<Entry> {
    let open = format!("\"{exe}\" \"%1\"");
    let mut entries = vec![
        Entry::new(PROG_ID, "", "mbook book"),
        Entry::new(
            format!(r"{PROG_ID}\DefaultIcon"),
            "",
            format!("\"{exe}\",0"),
        ),
        Entry::new(format!(r"{PROG_ID}\shell\open\command"), "", open.clone()),
        Entry::new(r"Applications\mbook.exe", "FriendlyAppName", "mbook"),
        Entry::new(r"Applications\mbook.exe\shell\open\command", "", open),
        // The application owns its own format.
        Entry::new(".mbook", "", PROG_ID),
    ];
    for extension in BOOK_FILE_EXTENSIONS {
        entries.push(Entry::new(
            format!(r".{extension}\OpenWithProgids"),
            PROG_ID,
            "",
        ));
        entries.push(Entry::new(
            r"Applications\mbook.exe\SupportedTypes",
            format!(".{extension}"),
            "",
        ));
    }
    entries
}

/// Registers the running executable for the book formats, for the current
/// user. A key that cannot be written is left out: the application works
/// without it, and the next start tries again.
///
/// Touches the registry: call it on the background executor.
#[cfg(windows)]
pub(super) fn register() {
    let Some(exe) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.to_str().map(ToOwned::to_owned))
    else {
        return;
    };
    for entry in entries(&exe) {
        if let Ok(key) = windows_registry::CURRENT_USER.create(&entry.key) {
            // A value that cannot be written is tried again at the next start.
            key.set_string(&entry.name, &entry.value).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXE: &str = r"C:\Program Files\mbook\mbook.exe";

    fn value(key: &str, name: &str) -> Option<String> {
        entries(EXE)
            .into_iter()
            .find(|entry| entry.key == format!(r"{CLASSES}\{key}") && entry.name == name)
            .map(|entry| entry.value)
    }

    #[test]
    fn mbook_files_open_with_the_application() {
        assert_eq!(value(".mbook", "").as_deref(), Some(PROG_ID));
        assert_eq!(
            value(r"mbook.Book\shell\open\command", "").as_deref(),
            Some(r#""C:\Program Files\mbook\mbook.exe" "%1""#)
        );
        assert_eq!(
            value(r"mbook.Book\DefaultIcon", "").as_deref(),
            Some(r#""C:\Program Files\mbook\mbook.exe",0"#)
        );
    }

    #[test]
    fn every_book_format_offers_the_application_without_taking_its_default() {
        for extension in BOOK_FILE_EXTENSIONS {
            assert_eq!(
                value(&format!(r".{extension}\OpenWithProgids"), PROG_ID).as_deref(),
                Some(""),
                "{extension}"
            );
            assert_eq!(
                value(
                    r"Applications\mbook.exe\SupportedTypes",
                    &format!(".{extension}")
                )
                .as_deref(),
                Some(""),
                "{extension}"
            );
            if *extension != "mbook" {
                assert_eq!(value(&format!(".{extension}"), ""), None, "{extension}");
            }
        }
    }

    #[test]
    fn every_key_is_under_the_classes_of_the_current_user() {
        assert!(
            entries(EXE)
                .iter()
                .all(|entry| entry.key.starts_with(r"Software\Classes\"))
        );
    }

    #[test]
    fn the_bundle_of_macos_declares_every_book_format() {
        let plist = include_str!("../../assets/macos/document-types.plist");
        assert!(plist.contains("<string>io.github.m-compressors.mbook.book</string>"));
        for extension in BOOK_FILE_EXTENSIONS {
            assert!(
                plist.contains(&format!("<string>{extension}</string>")),
                "{extension}"
            );
        }
        // cargo-bundle writes the fragment as it is, after escaping `&`.
        assert!(!plist.contains('&'));
    }

    #[test]
    fn the_manifest_of_android_opens_the_formats_it_names() {
        let manifest = include_str!("../../../mbook-android/android/AndroidManifest.xml");
        for mime in [
            "application/epub+zip",
            "application/pdf",
            "application/x-fictionbook+xml",
            "application/vnd.comicbook+zip",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "text/html",
            "text/markdown",
            "text/plain",
            // An mbook file has no registered type: providers send it as data.
            "application/octet-stream",
        ] {
            assert!(
                manifest.contains(&format!("android:mimeType=\"{mime}\"")),
                "{mime}"
            );
        }
    }
}
