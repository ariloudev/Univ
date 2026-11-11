//! Version 4's kind of object: what a Galaxy tracer is, coded after version 3's columns.

use crate::object::ObjectKind;

/// Whether tiles of `version` carry each point's kind of object: from version 4 on.
#[must_use]
pub const fn has_object_kind(version: u16) -> bool {
    version >= 4
}

/// The kinds of object of version 4, each coded as its index plus one; 0 codes a point whose
/// object is one body, which names no kind.
pub const OBJECT_KINDS: [ObjectKind; 7] = [
    ObjectKind::GalaxyGroup,
    ObjectKind::OpenCluster,
    ObjectKind::GlobularCluster,
    ObjectKind::ClassicalCepheid,
    ObjectKind::RrLyraeStar,
    ObjectKind::StarFormingRegion,
    ObjectKind::LuminousHotStar,
];

/// The code version 4 gives a point's kind of object: 1 + its index in [`OBJECT_KINDS`], or 0
/// for none or a body.
#[must_use]
pub fn object_kind_code(kind: Option<ObjectKind>) -> u8 {
    kind.and_then(|k| OBJECT_KINDS.iter().position(|o| *o == k))
        .and_then(|i| u8::try_from(i + 1).ok())
        .unwrap_or_default()
}

/// The kind of object of a version 4 code: none for 0.
///
/// # Errors
///
/// The code itself, where it lies beyond [`OBJECT_KINDS`].
pub fn object_kind_of_code(code: u8) -> Result<Option<ObjectKind>, u8> {
    match code {
        0 => Ok(None),
        c => OBJECT_KINDS
            .get(usize::from(c) - 1)
            .copied()
            .map(Some)
            .ok_or(c),
    }
}
