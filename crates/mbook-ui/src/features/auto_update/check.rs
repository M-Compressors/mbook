//! Asking the releases of the repository.

use self_update::backends::github::ReleaseList;
use self_update::version::cmp_versions;

use super::error::UpdateError;

/// The owner of the repository the releases are read from.
const OWNER: &str = "M-Compressors";
/// The name of the repository.
const REPOSITORY: &str = "mbook";
/// The version of the running application.
const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// A release newer than the running version, with the archive for this
/// platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Update {
    /// The version, without the `v` of the tag: `0.2.0`.
    pub(crate) version: String,
    /// The file name of the archive, which is also what its signature is
    /// salted with.
    pub(super) archive: String,
    /// Where the archive is downloaded from.
    pub(super) url: String,
}

/// A release as far as the choice of an update needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Candidate {
    /// The version or the tag of the release.
    pub(super) version: String,
    /// The `(name, address)` of each file attached to the release.
    pub(super) assets: Vec<(String, String)>,
}

/// Whether this build checks for updates at all: not a debug build, which does
/// not run from an installed bundle and whose version is not that of a tag.
#[must_use]
pub(crate) fn enabled() -> bool {
    !cfg!(debug_assertions)
}

/// The name of the archive of a release: `mbook-v0.1.0-aarch64-apple-darwin.tar.gz`.
///
/// `tag` is the tag with its `v`.
#[must_use]
pub(super) fn archive_name(tag: &str, target: &str) -> String {
    format!("mbook-{tag}-{target}.tar.gz")
}

/// The name, among `names`, of the archive of `tag` for `target`.
#[must_use]
pub(super) fn pick_archive<'a>(names: &[&'a str], tag: &str, target: &str) -> Option<&'a str> {
    let wanted = archive_name(tag, target);
    names.iter().copied().find(|name| *name == wanted)
}

/// The version a tag stands for: `v0.2.0` and `0.2.0` are `0.2.0`.
fn version_of(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

/// Whether a release is offered over the running version: its tag is a
/// version, a stable one, and greater. A pre-release is never offered.
#[must_use]
pub(super) fn is_newer(tag: &str, current: &str) -> bool {
    let version = version_of(tag);
    let core = version.split_once('+').map_or(version, |(core, _)| core);
    if core.contains('-') {
        return false;
    }
    matches!(
        cmp_versions(version, current),
        Ok(std::cmp::Ordering::Greater)
    )
}

/// The update to offer: the newest release that is newer than `current`,
/// with its archive for `target`.
///
/// When the newest release has no archive for the target (its binaries are
/// not built yet), there is nothing to offer: an older release is not
/// installed instead.
#[must_use]
pub(super) fn select(candidates: &[Candidate], current: &str, target: &str) -> Option<Update> {
    let mut newest: Option<&Candidate> = None;
    for candidate in candidates {
        if !is_newer(&candidate.version, current) {
            continue;
        }
        let greater = newest.is_none_or(|best| {
            matches!(
                cmp_versions(version_of(&candidate.version), version_of(&best.version)),
                Ok(std::cmp::Ordering::Greater)
            )
        });
        if greater {
            newest = Some(candidate);
        }
    }
    let newest = newest?;
    let version = version_of(&newest.version).to_owned();
    let tag = format!("v{version}");
    let names: Vec<&str> = newest
        .assets
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    let archive = pick_archive(&names, &tag, target)?;
    let (_, url) = newest.assets.iter().find(|(name, _)| name == archive)?;
    if !url.starts_with("https://") {
        return None;
    }
    Some(Update {
        version,
        archive: archive.to_owned(),
        url: url.clone(),
    })
}

/// Asks the releases of the repository.
///
/// `Ok(None)`: nothing is newer, or the newest release has no archive for
/// this target yet.
///
/// Blocks on the network: call it on the background executor.
///
/// # Errors
///
/// [`UpdateError::Releases`] when the releases cannot be asked or read.
pub(crate) fn check() -> Result<Option<Update>, UpdateError> {
    let releases = ReleaseList::configure()
        .repo_owner(OWNER)
        .repo_name(REPOSITORY)
        .build()
        .and_then(|list| list.fetch())
        .map_err(|error| UpdateError::Releases(Box::new(error)))?;
    let candidates: Vec<Candidate> = releases
        .all()
        .iter()
        .map(|release| Candidate {
            version: release.version().to_owned(),
            assets: release
                .assets()
                .iter()
                .map(|asset| (asset.name().to_owned(), asset.download_url().to_owned()))
                .collect(),
        })
        .collect();
    Ok(select(&candidates, CURRENT, self_update::get_target()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TARGET: &str = "aarch64-apple-darwin";

    fn candidate(version: &str, names: &[&str]) -> Candidate {
        Candidate {
            version: version.to_owned(),
            assets: names
                .iter()
                .map(|name| ((*name).to_owned(), format!("https://example.test/{name}")))
                .collect(),
        }
    }

    #[test]
    fn the_archive_is_named_after_the_tag_and_the_target() {
        assert_eq!(
            archive_name("v0.1.0", TARGET),
            "mbook-v0.1.0-aarch64-apple-darwin.tar.gz"
        );
    }

    #[test]
    fn the_archive_of_the_target_is_picked_by_its_whole_name() {
        let names = [
            "mbook-v0.1.0-x86_64-apple-darwin.tar.gz",
            "mbook-v0.1.0-aarch64-apple-darwin.dmg",
            "mbook-v0.1.0-aarch64-apple-darwin.tar.gz",
        ];
        assert_eq!(
            pick_archive(&names, "v0.1.0", TARGET),
            Some("mbook-v0.1.0-aarch64-apple-darwin.tar.gz")
        );
    }

    #[test]
    fn no_archive_is_picked_for_a_target_without_one() {
        let names = ["mbook-v0.1.0-x86_64-apple-darwin.tar.gz"];
        assert_eq!(pick_archive(&names, "v0.1.0", TARGET), None);
    }

    #[test]
    fn no_archive_is_picked_from_another_tag() {
        let names = ["mbook-v0.0.9-aarch64-apple-darwin.tar.gz"];
        assert_eq!(pick_archive(&names, "v0.1.0", TARGET), None);
    }

    #[test]
    fn no_archive_is_picked_from_nothing() {
        assert_eq!(pick_archive(&[], "v0.1.0", TARGET), None);
    }

    #[test]
    fn a_greater_tag_is_newer() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("v0.1.1", "0.1.0"));
        assert!(is_newer("v1.0.0", "0.9.9"));
    }

    #[test]
    fn a_tag_without_a_v_is_read_too() {
        assert!(is_newer("0.2.0", "0.1.0"));
    }

    #[test]
    fn the_same_version_is_not_newer() {
        assert!(!is_newer("v0.1.0", "0.1.0"));
    }

    #[test]
    fn an_older_tag_is_not_newer() {
        assert!(!is_newer("v0.0.9", "0.1.0"));
    }

    #[test]
    fn versions_are_compared_by_number_and_not_by_text() {
        assert!(is_newer("v0.10.0", "0.9.0"));
    }

    #[test]
    fn a_tag_that_is_no_version_is_not_newer() {
        assert!(!is_newer("nightly", "0.1.0"));
        assert!(!is_newer("v1", "0.1.0"));
        assert!(!is_newer("", "0.1.0"));
    }

    #[test]
    fn a_pre_release_is_not_newer() {
        assert!(!is_newer("v0.2.0-rc.1", "0.1.0"));
        assert!(!is_newer("v0.2.0-rc.1+build.5", "0.1.0"));
    }

    #[test]
    fn a_release_over_a_pre_release_of_it_is_newer() {
        assert!(is_newer("v0.2.0", "0.2.0-rc.1"));
    }

    #[test]
    fn build_metadata_with_a_hyphen_is_not_a_pre_release() {
        assert!(is_newer("v0.2.0+linux-x64", "0.1.0"));
    }

    #[test]
    fn the_newest_release_is_offered_with_its_archive() {
        let name = "mbook-v0.3.0-aarch64-apple-darwin.tar.gz";
        let candidates = [
            candidate("0.2.0", &["mbook-v0.2.0-aarch64-apple-darwin.tar.gz"]),
            candidate("0.3.0", &[name, "mbook-v0.3.0-aarch64-apple-darwin.dmg"]),
            candidate("0.1.5", &[]),
        ];
        let update = select(&candidates, "0.1.0", TARGET);
        assert_eq!(
            update,
            Some(Update {
                version: "0.3.0".to_owned(),
                archive: name.to_owned(),
                url: format!("https://example.test/{name}"),
            })
        );
    }

    #[test]
    fn a_tag_with_a_v_is_offered_by_its_version() {
        let candidates = [candidate(
            "v0.2.0",
            &["mbook-v0.2.0-aarch64-apple-darwin.tar.gz"],
        )];
        let update = select(&candidates, "0.1.0", TARGET);
        assert_eq!(
            update.map(|update| update.version),
            Some("0.2.0".to_owned())
        );
    }

    #[test]
    fn nothing_is_offered_when_nothing_is_newer() {
        let candidates = [candidate(
            "0.1.0",
            &["mbook-v0.1.0-aarch64-apple-darwin.tar.gz"],
        )];
        assert_eq!(select(&candidates, "0.1.0", TARGET), None);
    }

    #[test]
    fn nothing_is_offered_without_releases() {
        assert_eq!(select(&[], "0.1.0", TARGET), None);
    }

    #[test]
    fn nothing_is_offered_when_the_newest_release_has_no_archive_yet() {
        let candidates = [
            candidate("0.2.0", &["mbook-v0.2.0-aarch64-apple-darwin.tar.gz"]),
            candidate("0.3.0", &["mbook-v0.3.0-x86_64-apple-darwin.tar.gz"]),
        ];
        assert_eq!(select(&candidates, "0.1.0", TARGET), None);
    }

    #[test]
    fn a_pre_release_is_skipped_for_the_stable_one_below_it() {
        let candidates = [
            candidate(
                "0.3.0-rc.1",
                &["mbook-v0.3.0-rc.1-aarch64-apple-darwin.tar.gz"],
            ),
            candidate("0.2.0", &["mbook-v0.2.0-aarch64-apple-darwin.tar.gz"]),
        ];
        let update = select(&candidates, "0.1.0", TARGET);
        assert_eq!(
            update.map(|update| update.version),
            Some("0.2.0".to_owned())
        );
    }

    #[test]
    fn an_archive_that_is_not_served_over_tls_is_not_offered() {
        let name = "mbook-v0.2.0-aarch64-apple-darwin.tar.gz";
        let candidates = [Candidate {
            version: "0.2.0".to_owned(),
            assets: vec![(name.to_owned(), format!("http://example.test/{name}"))],
        }];
        assert_eq!(select(&candidates, "0.1.0", TARGET), None);
    }

    #[test]
    fn updates_are_checked_only_in_a_release_build() {
        assert_eq!(enabled(), !cfg!(debug_assertions));
    }

    #[test]
    fn the_running_version_is_a_version() {
        assert!(cmp_versions(CURRENT, CURRENT).is_ok());
    }
}
