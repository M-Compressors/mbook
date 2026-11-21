//! Downloading, verifying and installing an update.

#[cfg(any(target_os = "macos", test))]
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use self_update::Download;

use super::check::Update;
use super::error::UpdateError;
use super::verify;
use crate::shared::roots::Roots;

/// What is replaced on macOS: the bundle in the archive.
#[cfg(target_os = "macos")]
const APP: &str = "mbook.app";
/// What is installed on Windows: the installer in the archive.
#[cfg(target_os = "windows")]
const MSI: &str = "mbook.msi";
/// More than any archive of a release: a server cannot fill the disk.
const MAX_DOWNLOAD: u64 = 512 * 1024 * 1024;
/// How long a download may take.
const DOWNLOAD_TIME: Duration = Duration::from_mins(15);

/// What the application does after an update was installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Installed {
    /// The files are replaced: start again (macOS).
    Restart,
    /// The installer of the system was started and brings the new version
    /// up: just leave (Windows).
    #[cfg_attr(
        not(any(target_os = "windows", test)),
        expect(
            dead_code,
            reason = "only the Windows installer asks the application to quit"
        )
    )]
    Quit,
}

/// A downloaded file that is deleted when it goes out of scope, whether the
/// update was installed, refused or failed half way.
struct Downloaded(PathBuf);

impl Drop for Downloaded {
    fn drop(&mut self) {
        // The file may not exist (the download did not start) or is already
        // gone: there is nothing to do about a file that cannot be removed.
        drop(std::fs::remove_file(&self.0));
    }
}

/// Downloads the archive into `roots.update()`, verifies its signature,
/// installs it.
///
/// Nothing is extracted or replaced before the signature is verified; when it
/// is not valid the installed application stays as it is and the downloaded
/// file is deleted.
///
/// Blocks on the network and the file system: call it on the background
/// executor.
///
/// # Errors
///
/// [`UpdateError::NotInBundle`] when the executable is not inside an
/// application bundle (macOS); [`UpdateError::Download`] when the archive
/// cannot be downloaded; [`UpdateError::Signature`] when it is not signed with
/// the key of the application; [`UpdateError::Unpack`],
/// [`UpdateError::MissingInArchive`] and [`UpdateError::Io`] when it cannot be
/// installed; [`UpdateError::Unsupported`] on a system without an installer.
pub(crate) fn install(update: &Update, roots: &Roots) -> Result<Installed, UpdateError> {
    install_here(update, roots)
}

#[cfg(target_os = "macos")]
fn install_here(update: &Update, roots: &Roots) -> Result<Installed, UpdateError> {
    // Before the download: a run that cannot install does not fetch.
    let exe = std::env::current_exe()?;
    let bundle = bundle_of(&exe).ok_or(UpdateError::NotInBundle { path: exe })?;
    let archive = fetch(update, roots)?;
    replace_bundle(&archive.0, &bundle)?;
    Ok(Installed::Restart)
}

#[cfg(target_os = "windows")]
fn install_here(update: &Update, roots: &Roots) -> Result<Installed, UpdateError> {
    use std::os::windows::process::CommandExt as _;

    /// `CREATE_NO_WINDOW`: the console of `cmd` is not shown.
    const NO_WINDOW: u32 = 0x0800_0000;

    let exe = std::env::current_exe()?;
    let archive = fetch(update, roots)?;
    let dir = roots.update();
    self_update::Extract::from_source(&archive.0)
        .extract_file(&dir, MSI)
        .map_err(|error| UpdateError::Unpack(Box::new(error)))?;
    let msi = dir.join(MSI);
    if !msi.is_file() {
        return Err(UpdateError::MissingInArchive(MSI));
    }
    // The installer is `passive` and shows no final dialog with a "launch"
    // box, so `cmd` starts the application when it has finished.
    let child = std::process::Command::new("cmd")
        .raw_arg(msiexec_command_line(&msi, &exe))
        .creation_flags(NO_WINDOW)
        .spawn()?;
    drop(child);
    Ok(Installed::Quit)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn install_here(_update: &Update, _roots: &Roots) -> Result<Installed, UpdateError> {
    Err(UpdateError::Unsupported)
}

/// Downloads the archive of `update` into `roots.update()` and verifies it
/// against the key built into the application.
///
/// The returned file is deleted when it is dropped; a failed download or a
/// refused signature deletes it before the error is returned.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn fetch(update: &Update, roots: &Roots) -> Result<Downloaded, UpdateError> {
    let dir = roots.update();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(&update.archive);
    let file = Downloaded(path);
    download(&update.url, &file.0)?;
    verify::verify(&file.0, &verify::KEY)?;
    Ok(file)
}

/// Downloads `url` into `path`.
///
/// The GitHub API gives a file away only to a request that accepts binary
/// content.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn download(url: &str, path: &Path) -> Result<(), UpdateError> {
    let mut file = std::fs::File::create(path)?;
    Download::from_url(url)
        .request_header("Accept", "application/octet-stream")
        .max_download_size(MAX_DOWNLOAD)
        .timeout(DOWNLOAD_TIME)
        .download_to(&mut file)
        .map_err(|error| UpdateError::Download(Box::new(error)))?;
    Ok(file.sync_all()?)
}

/// The bundle an executable runs from: `mbook.app` for
/// `mbook.app/Contents/MacOS/mbook`. `None` for any other place.
#[cfg(any(target_os = "macos", test))]
fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let in_bundle = macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension()? == "app";
    in_bundle.then(|| bundle.to_path_buf())
}

/// Unpacks the verified `archive` next to `bundle` and puts the new bundle in
/// its place, whole, so that its code signature stays valid.
#[cfg(target_os = "macos")]
fn replace_bundle(archive: &Path, bundle: &Path) -> Result<(), UpdateError> {
    let parent = bundle.parent().ok_or_else(|| UpdateError::NotInBundle {
        path: bundle.to_path_buf(),
    })?;
    // On the volume of the bundle, so that the move is a rename.
    let staging = tempfile::tempdir_in(parent)?;
    self_update::Extract::from_source(archive)
        .extract_into(staging.path())
        .map_err(|error| UpdateError::Unpack(Box::new(error)))?;
    let fresh = staging.path().join(APP);
    if !fresh.is_dir() {
        return Err(UpdateError::MissingInArchive(APP));
    }
    let stash = tempfile::tempdir_in(parent)?;
    swap(&fresh, bundle, stash.path())
}

/// Moves `bundle` into `stash` and `fresh` to the place of `bundle`. When the
/// second move fails the first is undone, so the old bundle is not lost.
#[cfg(any(target_os = "macos", test))]
fn swap(fresh: &Path, bundle: &Path, stash: &Path) -> Result<(), UpdateError> {
    let old = stash.join("old");
    fs::rename(bundle, &old)?;
    if let Err(error) = fs::rename(fresh, bundle) {
        // Best effort: the error to report is the one that made it necessary.
        drop(fs::rename(&old, bundle));
        return Err(error.into());
    }
    Ok(())
}

/// The command line `cmd` is given: installs `msi` without asking, waits for
/// the installer and then starts `exe`.
#[cfg(any(target_os = "windows", test))]
fn msiexec_command_line(msi: &Path, exe: &Path) -> String {
    format!(
        r#"/C start "" /wait msiexec /i "{}" /passive /norestart && start "" "{}""#,
        msi.display(),
        exe.display()
    )
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    #[test]
    fn the_bundle_is_found_from_the_executable() {
        let exe = Path::new("/Applications/mbook.app/Contents/MacOS/mbook");
        assert_eq!(
            bundle_of(exe),
            Some(PathBuf::from("/Applications/mbook.app"))
        );
    }

    #[test]
    fn a_bundle_with_another_name_is_found_too() {
        let exe = Path::new("/Users/me/Apps/mbook beta.app/Contents/MacOS/mbook");
        assert_eq!(
            bundle_of(exe),
            Some(PathBuf::from("/Users/me/Apps/mbook beta.app"))
        );
    }

    #[test]
    fn a_bare_binary_is_in_no_bundle() {
        assert_eq!(bundle_of(Path::new("/work/target/release/mbook")), None);
        assert_eq!(bundle_of(Path::new("/work/target/debug/mbook")), None);
    }

    #[test]
    fn a_folder_called_app_without_the_layout_is_no_bundle() {
        assert_eq!(bundle_of(Path::new("/x/mbook.app/mbook")), None);
        assert_eq!(bundle_of(Path::new("/x/mbook.app/Contents/mbook")), None);
        assert_eq!(
            bundle_of(Path::new("/x/mbook.app/Resources/MacOS/mbook")),
            None
        );
    }

    #[test]
    fn the_layout_in_a_folder_that_is_no_app_is_no_bundle() {
        assert_eq!(bundle_of(Path::new("/x/mbook/Contents/MacOS/mbook")), None);
    }

    #[test]
    fn a_path_that_is_too_short_is_in_no_bundle() {
        assert_eq!(bundle_of(Path::new("mbook")), None);
        assert_eq!(bundle_of(Path::new("/")), None);
        assert_eq!(bundle_of(Path::new("")), None);
    }

    fn make_bundle(root: &Path, name: &str, marker: &str) -> std::io::Result<PathBuf> {
        let bundle = root.join(name);
        fs::create_dir_all(bundle.join("Contents"))?;
        fs::write(bundle.join("Contents/marker"), marker)?;
        Ok(bundle)
    }

    #[test]
    fn swapping_puts_the_new_bundle_in_place_of_the_old() -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let bundle = make_bundle(dir.path(), "mbook.app", "old")?;
        let fresh = make_bundle(dir.path(), "fresh.app", "new")?;
        let stash = tempfile::tempdir_in(dir.path())?;
        swap(&fresh, &bundle, stash.path())?;
        assert_eq!(fs::read_to_string(bundle.join("Contents/marker"))?, "new");
        assert!(!fresh.exists());
        assert_eq!(
            fs::read_to_string(stash.path().join("old/Contents/marker"))?,
            "old"
        );
        Ok(())
    }

    #[test]
    fn a_failed_swap_gives_the_old_bundle_back() -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let bundle = make_bundle(dir.path(), "mbook.app", "old")?;
        let stash = tempfile::tempdir_in(dir.path())?;
        let result = swap(&dir.path().join("missing.app"), &bundle, stash.path());
        assert!(matches!(result, Err(UpdateError::Io(_))));
        assert_eq!(fs::read_to_string(bundle.join("Contents/marker"))?, "old");
        Ok(())
    }

    #[test]
    fn swapping_a_bundle_that_is_gone_fails_without_touching_the_new_one()
    -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let fresh = make_bundle(dir.path(), "fresh.app", "new")?;
        let stash = tempfile::tempdir_in(dir.path())?;
        let result = swap(&fresh, &dir.path().join("mbook.app"), stash.path());
        assert!(matches!(result, Err(UpdateError::Io(_))));
        assert!(fresh.is_dir());
        Ok(())
    }

    #[test]
    fn the_downloaded_file_is_deleted_when_it_goes_out_of_scope() -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("archive.tar.gz");
        fs::write(&path, b"bytes")?;
        drop(Downloaded(path.clone()));
        assert!(!path.exists());
        Ok(())
    }

    #[test]
    fn a_downloaded_file_that_was_never_written_is_no_error_to_drop() {
        drop(Downloaded(PathBuf::from("/nonexistent/archive.tar.gz")));
    }

    #[test]
    fn the_installer_is_started_without_questions_and_the_application_after_it() {
        let line = msiexec_command_line(
            Path::new(r"C:\cache\update\mbook.msi"),
            Path::new(r"C:\Program Files\mbook\mbook.exe"),
        );
        assert_eq!(
            line,
            r#"/C start "" /wait msiexec /i "C:\cache\update\mbook.msi" /passive /norestart && start "" "C:\Program Files\mbook\mbook.exe""#
        );
    }

    #[cfg(target_os = "macos")]
    mod bundle {
        use std::os::unix::fs::PermissionsExt as _;

        use super::super::super::fixtures::{Entry, tar_gz};
        use super::*;

        fn archive(dir: &Path) -> std::io::Result<PathBuf> {
            let path = dir.join("mbook-v0.2.0-aarch64-apple-darwin.tar.gz");
            fs::write(
                &path,
                tar_gz(&[
                    Entry {
                        path: "mbook.app/Contents/MacOS/mbook",
                        mode: 0o755,
                        data: b"new program",
                    },
                    Entry {
                        path: "mbook.app/Contents/Info.plist",
                        mode: 0o644,
                        data: b"new plist",
                    },
                ]),
            )?;
            Ok(path)
        }

        #[test]
        fn the_whole_bundle_is_replaced_and_the_program_stays_executable()
        -> Result<(), Box<dyn Error>> {
            let dir = tempfile::tempdir()?;
            let bundle = make_bundle(dir.path(), "mbook.app", "old")?;
            let archive = archive(dir.path())?;
            replace_bundle(&archive, &bundle)?;
            assert!(!bundle.join("Contents/marker").exists());
            assert_eq!(
                fs::read_to_string(bundle.join("Contents/Info.plist"))?,
                "new plist"
            );
            let mode = fs::metadata(bundle.join("Contents/MacOS/mbook"))?
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111);
            Ok(())
        }

        #[test]
        fn nothing_stays_beside_the_bundle_after_it_is_replaced() -> Result<(), Box<dyn Error>> {
            let dir = tempfile::tempdir()?;
            let apps = dir.path().join("Applications");
            let bundle = make_bundle(&apps, "mbook.app", "old")?;
            let archive = archive(dir.path())?;
            replace_bundle(&archive, &bundle)?;
            let names: Vec<_> = fs::read_dir(&apps)?
                .map(|entry| entry.map(|entry| entry.file_name()))
                .collect::<Result<_, _>>()?;
            assert_eq!(names, ["mbook.app"]);
            Ok(())
        }

        #[test]
        fn an_archive_without_the_bundle_leaves_the_old_one() -> Result<(), Box<dyn Error>> {
            let dir = tempfile::tempdir()?;
            let bundle = make_bundle(dir.path(), "mbook.app", "old")?;
            let path = dir.path().join("other.tar.gz");
            fs::write(
                &path,
                tar_gz(&[Entry {
                    path: "other.app/Contents/Info.plist",
                    mode: 0o644,
                    data: b"x",
                }]),
            )?;
            let result = replace_bundle(&path, &bundle);
            assert!(matches!(
                result,
                Err(UpdateError::MissingInArchive("mbook.app"))
            ));
            assert_eq!(fs::read_to_string(bundle.join("Contents/marker"))?, "old");
            Ok(())
        }

        #[test]
        fn a_file_that_is_no_archive_leaves_the_old_bundle() -> Result<(), Box<dyn Error>> {
            let dir = tempfile::tempdir()?;
            let bundle = make_bundle(dir.path(), "mbook.app", "old")?;
            let path = dir.path().join("broken.tar.gz");
            fs::write(&path, b"not an archive")?;
            let result = replace_bundle(&path, &bundle);
            assert!(matches!(
                result,
                Err(UpdateError::Unpack(_) | UpdateError::Io(_))
            ));
            assert_eq!(fs::read_to_string(bundle.join("Contents/marker"))?, "old");
            Ok(())
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    #[test]
    fn installing_is_not_supported_on_this_platform() -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let update = Update {
            version: "0.2.0".to_owned(),
            archive: "a.tar.gz".to_owned(),
            url: "https://example.test/a".to_owned(),
        };
        let roots = Roots::new(dir.path().join("data"), dir.path().join("cache"));
        assert!(matches!(
            install(&update, &roots),
            Err(UpdateError::Unsupported)
        ));
        Ok(())
    }
}
