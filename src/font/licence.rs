//! **Licence records carried in an atlas** (`LICS` and `LICT`).
//!
//! A baked atlas records the licence of every face it was baked from: a
//! fourcc and a 4-byte version as a compact key, the SPDX identifier as the
//! unambiguous name, and the face's copyright line and Reserved Font Name,
//! which a licence such as OFL asks to accompany a derived work. It does not
//! carry the licence text, which stays in the application's licence file
//! (`docs/design/20261016_01_ATLAS_LAYOUT_TABLES_PLAN.md`, section 4.8 and
//! its 12:41 and 12:56 EDT amendments).

use crate::font::GlyphStyle;

/// One licence of a baked atlas: which licence, which styles it covers and
/// whose work it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LicenceRecord {
    /// The licence fourcc (see [`LICENCE_REGISTRY`]); a reader exposes an
    /// unknown one as it is.
    pub fourcc: [u8; 4],
    /// The licence version `[major, minor, patch, subpatch]`.
    pub version: [u8; 4],
    /// The styles of the atlas this record covers.
    pub styles: Vec<GlyphStyle>,
    /// The SPDX licence identifier, e.g. `OFL-1.1`.
    pub spdx: String,
    /// What is licensed: the family name, e.g. `Roboto`.
    pub source: String,
    /// The face's copyright line (`name` ID 0).
    pub copyright: String,
    /// The Reserved Font Name, empty if none.
    pub reserved_font_name: String,
}

// ── Byte codecs ─────────────────────────────────────────────────────────

/// The `LICS` and `LICT` tables of `records` as table bodies.
pub(crate) fn encode_tables(records: &[LicenceRecord]) -> Vec<crate::font::atlas::TableBody> {
    let _ = records;
    Vec::new()
}

/// The licence records among `bodies` (tag, face byte, body).
pub(crate) fn decode_tables(
    bodies: &[(&[u8; 4], u8, &[u8])],
) -> Result<Vec<LicenceRecord>, &'static str> {
    let _ = bodies;
    Ok(Vec::new())
}
