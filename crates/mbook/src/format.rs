//! Constants of the file format and the format header, resource 0.

#[cfg(feature = "read")]
use crate::types::Version;

/// The newest major version this crate reads and the one it writes.
pub(crate) const MAJOR: u8 = 1;
/// The minor version this crate writes.
#[cfg(feature = "write")]
pub(crate) const MINOR: u8 = 0;

/// The container signature at the start of every mbook file.
pub(crate) const CONTAINER_MAGIC: &[u8] = b"MOZFRM";
/// The prefix of the decoded format header.
pub(crate) const PREFIX: &[u8; 6] = b"MBOOK\0";
/// Where the prefix lands in a file whose header is encoded as RFC 0001
/// requires.
pub(crate) const PREFIX_OFFSET: usize = 47;

/// Resource numbers of the fixed resources.
#[cfg(feature = "read")]
pub(crate) const FORMAT: u32 = 0;
#[cfg(feature = "read")]
pub(crate) const META: u32 = 1;
#[cfg(feature = "read")]
pub(crate) const RESOURCES: u32 = 2;
#[cfg(feature = "read")]
pub(crate) const TOC: u32 = 3;
#[cfg(feature = "read")]
pub(crate) const ANCHORS: u32 = 4;
/// The number of fixed resources; text and pictures follow them.
pub(crate) const SERVICE_COUNT: usize = 5;

/// Codes of the `kind` column of the resource table.
pub(crate) const KIND_SERVICE: u8 = 0;
#[cfg(feature = "write")]
pub(crate) const KIND_TEXT: u8 = 1;
#[cfg(feature = "write")]
pub(crate) const KIND_PICTURE: u8 = 2;
#[cfg(feature = "write")]
pub(crate) const KIND_ANCHORS: u8 = 3;
#[cfg(feature = "write")]
pub(crate) const KIND_LANDMARKS: u8 = 4;
#[cfg(feature = "write")]
pub(crate) const KIND_DIAGRAM: u8 = 5;
#[cfg(feature = "write")]
pub(crate) const KIND_FORMULA: u8 = 6;

/// The decoded format header of the version this crate writes.
#[cfg(feature = "write")]
pub(crate) const HEADER: [u8; 8] = [b'M', b'B', b'O', b'O', b'K', 0, MAJOR, MINOR];

/// The version a decoded format header declares; `None` when the bytes are
/// not a format header.
#[cfg(feature = "read")]
pub(crate) fn parse_header(bytes: &[u8]) -> Option<Version> {
    let (prefix, version) = bytes.split_at_checked(PREFIX.len())?;
    match (prefix == PREFIX, version) {
        (true, &[major, minor]) => Some(Version { major, minor }),
        _ => None,
    }
}

/// Tells whether this crate can read a file of that version.
#[cfg(feature = "read")]
pub(crate) const fn is_supported(version: Version) -> bool {
    version.major == MAJOR
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "read")]
    use super::Version;

    #[cfg(feature = "write")]
    #[test]
    fn the_written_header_is_the_prefix_and_version_one_dot_zero() {
        assert_eq!(&super::HEADER, b"MBOOK\0\x01\x00");
    }

    #[cfg(feature = "read")]
    #[test]
    fn a_header_gives_its_version() {
        let version = super::parse_header(b"MBOOK\0\x01\x07");
        assert_eq!(version, Some(Version { major: 1, minor: 7 }));
    }

    #[cfg(feature = "read")]
    #[test]
    fn a_header_of_another_length_is_not_a_header() {
        assert_eq!(super::parse_header(b"MBOOK\0\x01"), None);
        assert_eq!(super::parse_header(b"MBOOK\0\x01\x00\x00"), None);
    }

    #[cfg(feature = "read")]
    #[test]
    fn a_header_with_another_prefix_is_not_a_header() {
        assert_eq!(super::parse_header(b"MBOOK!\x01\x00"), None);
    }

    #[cfg(feature = "read")]
    #[test]
    fn only_the_current_major_version_is_supported() {
        let supported = [0, 1, 2].map(|major| super::is_supported(Version { major, minor: 9 }));
        assert_eq!(supported, [false, true, false]);
    }
}
