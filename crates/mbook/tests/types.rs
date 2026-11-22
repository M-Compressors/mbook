//! Value types, options and the guarantees every public type gives.

use mbook::{
    Anchor, ChapterId, Destination, Diagram, DiagramEntry, DiagramId, Error, Formula, FormulaEntry,
    FormulaId, Limits, Meta, PictureId, Position, Problems, ReadOptions, ResourceKind, Size,
    SourceError, TocEntry, TocItem, Version, WriteOptions,
};

fn assert_send_sync<T: Send + Sync + 'static>() {}
fn assert_value<T: Clone + std::fmt::Debug + PartialEq + Send + Sync + 'static>() {}
fn assert_copy<T: Copy + Eq + std::hash::Hash>() {}

#[test]
fn the_error_type_can_cross_threads_and_be_boxed() {
    assert_send_sync::<Error>();
}

#[test]
fn value_types_are_plain_data() {
    assert_value::<Meta>();
    assert_value::<Anchor>();
    assert_value::<TocEntry>();
    assert_value::<TocItem>();
    assert_value::<Destination>();
    assert_value::<Problems>();
    assert_value::<Diagram>();
    assert_value::<Formula>();
    assert_value::<DiagramEntry<'static>>();
    assert_value::<FormulaEntry<'static>>();
    assert_value::<SourceError>();
}

#[test]
fn ids_and_positions_are_copy_and_hashable() {
    assert_copy::<ChapterId>();
    assert_copy::<PictureId>();
    assert_copy::<DiagramId>();
    assert_copy::<FormulaId>();
    assert_copy::<Position>();
    assert_copy::<Size>();
    assert_copy::<Version>();
}

#[cfg(feature = "read")]
#[test]
fn a_reader_over_a_file_can_move_to_another_thread() {
    assert_send_sync::<mbook::BookReader<std::fs::File>>();
}

#[test]
fn default_limits_match_the_api_rfc() {
    let limits = Limits::default();
    assert_eq!(
        (
            limits.max_resources,
            limits.max_resource_bytes,
            limits.max_file_bytes
        ),
        (1_000_000, 64 << 20, 16 << 30)
    );
}

#[test]
fn read_options_start_from_the_default_limits() {
    assert_eq!(ReadOptions::default().limits, Limits::default());
}

#[test]
fn write_options_leave_the_level_to_the_compressor() {
    assert_eq!(WriteOptions::default().level, None);
}

#[test]
fn limits_can_be_tightened_field_by_field() {
    let mut options = ReadOptions::default();
    options.limits.max_file_bytes = 1 << 20;
    assert_eq!(options.limits.max_file_bytes, 1 << 20);
}

#[test]
fn a_version_prints_as_major_dot_minor() {
    assert_eq!(Version { major: 1, minor: 0 }.to_string(), "1.0");
}

#[test]
fn versions_order_by_major_then_minor() {
    assert!(Version { major: 1, minor: 9 } < Version { major: 2, minor: 0 });
}

#[test]
fn resource_kinds_follow_the_codes_of_the_format() {
    let kinds = [0, 1, 2].map(ResourceKind::from);
    assert_eq!(
        kinds,
        [
            ResourceKind::Service,
            ResourceKind::Text,
            ResourceKind::Picture
        ]
    );
}

#[test]
fn kind_code_three_is_the_anchor_table_of_a_chapter() {
    assert_eq!(ResourceKind::from(3), ResourceKind::ChapterAnchors);
}

#[test]
fn kind_code_four_is_the_landmarks() {
    assert_eq!(ResourceKind::from(4), ResourceKind::Landmarks);
}

#[test]
fn kind_codes_five_and_six_are_the_diagram_and_the_formula() {
    assert_eq!(
        [5, 6].map(ResourceKind::from),
        [ResourceKind::Diagram, ResourceKind::Formula]
    );
}

#[test]
fn an_undefined_kind_code_is_kept() {
    assert_eq!(ResourceKind::from(7), ResourceKind::Unknown(7));
}

#[test]
fn a_toc_item_is_built_from_a_link_destination() {
    let item = TocItem::new(2, "The station", "ch01.md#the-station");
    assert_eq!(
        (
            item.level,
            item.title.as_str(),
            item.chapter.as_str(),
            item.anchor.as_deref()
        ),
        (2, "The station", "ch01.md", Some("the-station"))
    );
}
