//! Recognising an mbook file from its first bytes.

use mbook_core::is_mbook;

#[test]
fn an_empty_prefix_is_not_an_mbook_file() {
    assert!(!is_mbook(&[]));
}

#[test]
fn the_container_signature_alone_is_not_enough() {
    assert!(!is_mbook(b"MOZFRM\x01\x00"));
}

#[test]
fn both_signatures_at_their_offsets_are_recognised() {
    let mut prefix = [0u8; 53];
    prefix[..6].copy_from_slice(b"MOZFRM");
    prefix[47..].copy_from_slice(b"MBOOK\0");
    assert!(is_mbook(&prefix));
}

#[test]
fn the_format_signature_at_another_offset_is_not_recognised() {
    let mut prefix = [0u8; 64];
    prefix[..6].copy_from_slice(b"MOZFRM");
    prefix[48..54].copy_from_slice(b"MBOOK\0");
    assert!(!is_mbook(&prefix));
}
