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
use crate::font::atlas::{ATLAS_TABLE_FACE_ALL, TableBody};

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

/// **A licence the bake knows**: its fourcc and version, the compact key
/// carried in `LICS`, and the SPDX identifier that is its unambiguous name.
/// The fourcc and version are libmsdf's own; SPDX is the standard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LicenceKind {
    /// The fourcc.
    pub fourcc: [u8; 4],
    /// `[major, minor, patch, subpatch]`; all zero for an unversioned licence.
    pub version: [u8; 4],
    /// The SPDX licence identifier.
    pub spdx: &'static str,
}

/// **Every licence the bake accepts.** Each `(fourcc, version)` maps to one
/// SPDX identifier and each SPDX identifier to one `(fourcc, version)`.
/// Adding a licence is one row here plus its round-trip test.
pub const LICENCE_REGISTRY: &[LicenceKind] = &[
    LicenceKind {
        fourcc: *b"OFL ",
        version: [1, 1, 0, 0],
        spdx: "OFL-1.1",
    },
    LicenceKind {
        fourcc: *b"APAC",
        version: [2, 0, 0, 0],
        spdx: "Apache-2.0",
    },
    LicenceKind {
        fourcc: *b"CC0 ",
        version: [1, 0, 0, 0],
        spdx: "CC0-1.0",
    },
    LicenceKind {
        fourcc: *b"CCBY",
        version: [4, 0, 0, 0],
        spdx: "CC-BY-4.0",
    },
    LicenceKind {
        fourcc: *b"MIT ",
        version: [0, 0, 0, 0],
        spdx: "MIT",
    },
    LicenceKind {
        fourcc: *b"UFL ",
        version: [1, 0, 0, 0],
        spdx: "Ubuntu-font-1.0",
    },
];

/// The registry row for an SPDX identifier.
pub fn licence_by_spdx(spdx: &str) -> Option<&'static LicenceKind> {
    LICENCE_REGISTRY.iter().find(|k| k.spdx == spdx)
}

/// The registry row for a fourcc and version.
pub fn licence_by_key(fourcc: [u8; 4], version: [u8; 4]) -> Option<&'static LicenceKind> {
    LICENCE_REGISTRY
        .iter()
        .find(|k| k.fourcc == fourcc && k.version == version)
}

/// Why a `LICT` string could not be written or read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenceTextError {
    /// Longer than 255 bytes.
    TooLong(usize),
    /// A byte outside printable ASCII (0x20 to 0x7E).
    NotAscii(u8),
    /// No length byte where one was expected.
    MissingLength,
    /// The length runs past the end of the table.
    Truncated,
}

impl core::fmt::Display for LicenceTextError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooLong(n) => write!(f, "licence string is {n} bytes; LICT holds at most 255"),
            Self::NotAscii(b) => {
                write!(
                    f,
                    "licence string has byte 0x{b:02X}, outside printable ASCII"
                )
            }
            Self::MissingLength => write!(f, "LICT ends where a string's length byte should be"),
            Self::Truncated => write!(f, "a LICT string runs past the end of the table"),
        }
    }
}

impl LicenceTextError {
    fn as_str(self) -> &'static str {
        match self {
            Self::TooLong(_) => "a LICT string is longer than 255 bytes",
            Self::NotAscii(_) => "a LICT string has a byte outside printable ASCII",
            Self::MissingLength => "LICT ends where a string's length byte should be",
            Self::Truncated => "a LICT string runs past the end of the table",
        }
    }
}

fn check_text(s: &[u8]) -> Result<(), LicenceTextError> {
    match s.iter().find(|&&b| !(0x20..=0x7E).contains(&b)) {
        Some(&b) => Err(LicenceTextError::NotAscii(b)),
        None => Ok(()),
    }
}

/// Append `s` as `<u8 length><bytes>`. Errors: not printable ASCII, longer
/// than 255 bytes.
pub(crate) fn put_pstr(out: &mut Vec<u8>, s: &str) -> Result<(), LicenceTextError> {
    if s.len() > 255 {
        return Err(LicenceTextError::TooLong(s.len()));
    }
    check_text(s.as_bytes())?;
    out.push(s.len() as u8);
    out.extend_from_slice(s.as_bytes());
    Ok(())
}

/// Read one length-prefixed string at `*pos`, advancing it. Errors: no
/// length byte, length past the end, non-ASCII byte.
pub(crate) fn take_pstr<'a>(data: &'a [u8], pos: &mut usize) -> Result<&'a str, LicenceTextError> {
    let len = *data.get(*pos).ok_or(LicenceTextError::MissingLength)? as usize;
    let text = data
        .get(*pos + 1..*pos + 1 + len)
        .ok_or(LicenceTextError::Truncated)?;
    check_text(text)?;
    *pos += 1 + len;
    Ok(core::str::from_utf8(text).expect("printable ASCII is UTF-8"))
}

/// The style bitmask of `styles`.
fn style_mask(styles: &[GlyphStyle]) -> u8 {
    styles.iter().fold(0, |m, &s| m | (1 << s as u8))
}

/// Check one record the way the bake and the encoder do: a registry row
/// whose SPDX agrees, if its key is in the registry; at least one style;
/// strings that `LICT` can hold.
pub(crate) fn validate(record: &LicenceRecord) -> Result<(), &'static str> {
    if let Some(kind) = licence_by_key(record.fourcc, record.version)
        && kind.spdx != record.spdx
    {
        return Err("LICS names a registered licence whose SPDX identifier disagrees with it");
    }
    if record.styles.is_empty() {
        return Err("a licence record covers no style");
    }
    let mut scratch = Vec::new();
    for s in [
        &record.spdx,
        &record.source,
        &record.copyright,
        &record.reserved_font_name,
    ] {
        put_pstr(&mut scratch, s).map_err(LicenceTextError::as_str)?;
    }
    Ok(())
}

/// The `LICS` and `LICT` tables of `records` as table bodies (none for no
/// records), after checking every rule.
pub(crate) fn encode_tables(records: &[LicenceRecord]) -> Result<Vec<TableBody>, &'static str> {
    if records.is_empty() {
        return Ok(Vec::new());
    }
    if records.len() > u16::MAX as usize {
        return Err("more than 65535 licence records");
    }
    let mut lics = Vec::with_capacity(4 + 12 * records.len());
    let mut lict = Vec::new();
    lics.extend_from_slice(&(records.len() as u16).to_le_bytes());
    lics.extend_from_slice(&[0; 2]);
    lict.extend_from_slice(&(records.len() as u16).to_le_bytes());
    lict.extend_from_slice(&[0; 2]);
    for (i, r) in records.iter().enumerate() {
        validate(r)?;
        lics.extend_from_slice(&r.fourcc);
        lics.extend_from_slice(&r.version);
        lics.push(style_mask(&r.styles));
        lics.push(0);
        lics.extend_from_slice(&(i as u16).to_le_bytes());
        for s in [&r.spdx, &r.source, &r.copyright, &r.reserved_font_name] {
            put_pstr(&mut lict, s).map_err(LicenceTextError::as_str)?;
        }
    }
    Ok(vec![
        TableBody {
            tag: *b"LICS",
            face: ATLAS_TABLE_FACE_ALL,
            body: lics,
        },
        TableBody {
            tag: *b"LICT",
            face: ATLAS_TABLE_FACE_ALL,
            body: lict,
        },
    ])
}

/// The licence records among `bodies` (tag, face byte, body).
pub(crate) fn decode_tables(
    bodies: &[(&[u8; 4], u8, &[u8])],
) -> Result<Vec<LicenceRecord>, &'static str> {
    let find = |want: &[u8; 4]| -> Result<Option<&[u8]>, &'static str> {
        let mut hit = None;
        for &(tag, face, body) in bodies {
            if tag == want {
                if face != ATLAS_TABLE_FACE_ALL {
                    return Err("a LICS or LICT table belongs to a face rather than the atlas");
                }
                hit = Some(body);
            }
        }
        Ok(hit)
    };
    let (lics, lict) = match (find(b"LICS")?, find(b"LICT")?) {
        (None, None) => return Ok(Vec::new()),
        (Some(s), Some(t)) => (s, t),
        (Some(_), None) => return Err("a LICS table without its LICT"),
        (None, Some(_)) => return Err("a LICT table without a LICS"),
    };
    if lics.len() < 4 || lics[2..4] != [0, 0] {
        return Err("LICS header is truncated or has non-zero reserved bytes");
    }
    let count = u16::from_le_bytes([lics[0], lics[1]]) as usize;
    if lics.len() != 4 + 12 * count {
        return Err("LICS is not 4 + 12 x count bytes (wrong length)");
    }
    if lict.len() < 4 || lict[2..4] != [0, 0] {
        return Err("LICT header is truncated or has non-zero reserved bytes");
    }
    if u16::from_le_bytes([lict[0], lict[1]]) as usize != count {
        return Err("LICT's count differs from LICS's");
    }
    let mut pos = 4;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let r = &lics[4 + 12 * i..][..12];
        if r[9] != 0 {
            return Err("LICS record has a non-zero reserved byte");
        }
        if u16::from_le_bytes([r[10], r[11]]) as usize != i {
            return Err("a LICS record's lict_index differs from its position");
        }
        let mask = r[8];
        if mask == 0 || mask >> GlyphStyle::ALL.len() != 0 {
            return Err("a LICS record's style mask is empty or names a style this build lacks");
        }
        let styles = GlyphStyle::ALL
            .iter()
            .copied()
            .filter(|&s| mask & (1 << s as u8) != 0)
            .collect();
        let mut text = [""; 4];
        for t in &mut text {
            *t = take_pstr(lict, &mut pos).map_err(LicenceTextError::as_str)?;
        }
        let record = LicenceRecord {
            fourcc: r[0..4].try_into().unwrap(),
            version: r[4..8].try_into().unwrap(),
            styles,
            spdx: text[0].to_string(),
            source: text[1].to_string(),
            copyright: text[2].to_string(),
            reserved_font_name: text[3].to_string(),
        };
        validate(&record)?;
        out.push(record);
    }
    if pos != lict.len() {
        return Err("LICT has bytes left over after its last entry");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::FontAtlas;

    const GRENZE: &str = "Copyright 2020 The Grenze Gotisch Project Authors \
                          (https://github.com/Omnibus-Type/Grenze-Gotisch)";

    fn record(kind: &LicenceKind, styles: &[GlyphStyle]) -> LicenceRecord {
        LicenceRecord {
            fourcc: kind.fourcc,
            version: kind.version,
            styles: styles.to_vec(),
            spdx: kind.spdx.to_string(),
            source: "Grenze Gotisch".to_string(),
            copyright: GRENZE.to_string(),
            reserved_font_name: String::new(),
        }
    }

    fn round_trip(records: &[LicenceRecord]) -> Result<Vec<LicenceRecord>, &'static str> {
        let mut atlas = FontAtlas::empty(1, 1, 3);
        atlas.licences = records.to_vec();
        FontAtlas::from_bytes(&atlas.try_to_bytes()?).map(|a| a.licences)
    }

    fn tables(records: &[LicenceRecord]) -> (Vec<u8>, Vec<u8>) {
        let t = encode_tables(records).unwrap();
        (t[0].body.clone(), t[1].body.clone())
    }

    fn load(lics: Option<Vec<u8>>, lict: Option<Vec<u8>>) -> Result<FontAtlas, &'static str> {
        let mut t = Vec::new();
        if let Some(b) = lics {
            t.push(TableBody {
                tag: *b"LICS",
                face: ATLAS_TABLE_FACE_ALL,
                body: b,
            });
        }
        if let Some(b) = lict {
            t.push(TableBody {
                tag: *b"LICT",
                face: ATLAS_TABLE_FACE_ALL,
                body: b,
            });
        }
        FontAtlas::from_bytes(&FontAtlas::empty(1, 1, 3).write_v5(&t))
    }

    #[test]
    fn every_registry_row_round_trips_and_the_registry_is_a_bijection() {
        for (i, kind) in LICENCE_REGISTRY.iter().enumerate() {
            let r = record(kind, &[GlyphStyle::Regular, GlyphStyle::Italic]);
            assert_eq!(round_trip(std::slice::from_ref(&r)).unwrap(), vec![r]);
            assert_eq!(licence_by_spdx(kind.spdx), Some(kind));
            assert_eq!(licence_by_key(kind.fourcc, kind.version), Some(kind));
            for other in &LICENCE_REGISTRY[i + 1..] {
                assert_ne!((kind.fourcc, kind.version), (other.fourcc, other.version));
                assert_ne!(kind.spdx, other.spdx);
            }
        }
    }

    #[test]
    fn a_registered_fourcc_with_a_contradicting_spdx_is_refused() {
        let mut r = record(&LICENCE_REGISTRY[0], &[GlyphStyle::Regular]);
        r.spdx = "Apache-2.0".into();
        assert!(
            round_trip(std::slice::from_ref(&r)).is_err(),
            "the encoder refuses"
        );
        // ...and so does the reader, given such bytes.
        let (lics, mut lict) = tables(&[record(&LICENCE_REGISTRY[0], &[GlyphStyle::Regular])]);
        // Rewrite the SPDX string in place: "OFL-1.1" is 7 bytes, so is "OFL-1.0".
        lict[4 + 1 + 6] = b'0';
        let err = load(Some(lics), Some(lict)).expect_err("contradiction");
        assert!(err.contains("SPDX"), "{err}");
    }

    #[test]
    fn an_unknown_fourcc_loads_raw() {
        let r = LicenceRecord {
            fourcc: *b"ZZZZ",
            version: [9, 8, 7, 6],
            styles: vec![GlyphStyle::Bold],
            spdx: "LicenseRef-Future".into(),
            source: "Someone".into(),
            copyright: "Copyright 2030 Someone".into(),
            reserved_font_name: "Someone".into(),
        };
        assert_eq!(round_trip(std::slice::from_ref(&r)).unwrap(), vec![r]);
    }

    #[test]
    fn pstr_reads() {
        let mut pos = 0;
        let data = b"\x07OFL-1.1rest";
        assert_eq!(take_pstr(data, &mut pos), Ok("OFL-1.1"));
        assert_eq!(pos, 8);

        let data = b"\x00\x07OFL-1.1";
        let mut pos = 0;
        assert_eq!(take_pstr(data, &mut pos), Ok(""));
        assert_eq!(take_pstr(data, &mut pos), Ok("OFL-1.1"));
        assert_eq!(pos, data.len());

        let long = "a".repeat(255);
        let mut buf = vec![255u8];
        buf.extend_from_slice(long.as_bytes());
        let mut pos = 0;
        assert_eq!(take_pstr(&buf, &mut pos), Ok(long.as_str()));

        // The whole Grenze example entry.
        let mut entry = vec![0x07];
        entry.extend_from_slice(b"OFL-1.1");
        entry.push(0x0E);
        entry.extend_from_slice(b"Grenze Gotisch");
        entry.push(0x62);
        entry.extend_from_slice(GRENZE.as_bytes());
        entry.push(0x00);
        assert_eq!(GRENZE.len(), 98);
        let mut pos = 0;
        let got: Vec<&str> = (0..4)
            .map(|_| take_pstr(&entry, &mut pos).unwrap())
            .collect();
        assert_eq!(got, ["OFL-1.1", "Grenze Gotisch", GRENZE, ""]);
        assert_eq!(pos, entry.len());
    }

    #[test]
    fn pstr_refusals() {
        let mut pos = 0;
        assert_eq!(
            take_pstr(b"\x0Aabc", &mut pos),
            Err(LicenceTextError::Truncated)
        );
        let mut pos = 0;
        assert_eq!(
            take_pstr(b"", &mut pos),
            Err(LicenceTextError::MissingLength)
        );
        let mut pos = 0;
        assert_eq!(
            take_pstr(b"\x02a\x80", &mut pos),
            Err(LicenceTextError::NotAscii(0x80))
        );
        let mut pos = 0;
        assert_eq!(
            take_pstr(b"\x02a\x0A", &mut pos),
            Err(LicenceTextError::NotAscii(0x0A))
        );

        // A table ending before an entry's fourth string.
        let (lics, mut lict) = tables(&[record(&LICENCE_REGISTRY[0], &[GlyphStyle::Regular])]);
        lict.pop(); // the rfn's length byte
        let err = load(Some(lics.clone()), Some(lict)).expect_err("short");
        assert!(err.contains("length byte"), "{err}");
        // A length byte with nothing after it.
        let (_, mut lict) = tables(&[record(&LICENCE_REGISTRY[0], &[GlyphStyle::Regular])]);
        let last = lict.len() - 1;
        lict[last] = 4;
        let err = load(Some(lics), Some(lict)).expect_err("dangling length");
        assert!(err.contains("past the end"), "{err}");

        let mut out = Vec::new();
        assert_eq!(
            put_pstr(&mut out, "José"),
            Err(LicenceTextError::NotAscii(0xC3))
        );
        assert_eq!(
            put_pstr(&mut out, &"a".repeat(256)),
            Err(LicenceTextError::TooLong(256))
        );
        assert!(out.is_empty());
    }

    #[test]
    fn bundled_licence_strings_round_trip() {
        let face = roboto_copyright();
        let mut out = Vec::new();
        for s in ["Apache-2.0", "Roboto", "Material Symbols", face, ""] {
            put_pstr(&mut out, s).unwrap();
        }
        let mut pos = 0;
        for s in ["Apache-2.0", "Roboto", "Material Symbols", face, ""] {
            assert_eq!(take_pstr(&out, &mut pos), Ok(s));
        }
    }

    /// The bundled Roboto faces' `name` ID 0, as fontTools reads it.
    fn roboto_copyright() -> &'static str {
        "Copyright 2011 Google Inc. All Rights Reserved."
    }

    #[test]
    fn lics_and_lict_must_agree() {
        let two = [
            record(&LICENCE_REGISTRY[0], &[GlyphStyle::Regular]),
            record(&LICENCE_REGISTRY[1], &[GlyphStyle::Bold]),
        ];
        let (lics, lict) = tables(&two);
        assert_eq!(
            load(Some(lics.clone()), Some(lict.clone()))
                .unwrap()
                .licences,
            two
        );

        let mut bad_index = lics.clone();
        bad_index[4 + 12 + 10] = 0; // record 1 names entry 0
        let err = load(Some(bad_index), Some(lict.clone())).expect_err("index");
        assert!(err.contains("lict_index"), "{err}");

        let mut bad_count = lict.clone();
        bad_count[0] = 3;
        let err = load(Some(lics.clone()), Some(bad_count)).expect_err("count");
        assert!(err.contains("count"), "{err}");

        let err = load(Some(lics.clone()), None).expect_err("no LICT");
        assert!(err.contains("without its LICT"), "{err}");

        let mut extra = lict.clone();
        extra.push(0);
        let err = load(Some(lics), Some(extra)).expect_err("left over");
        assert!(err.contains("left over"), "{err}");
    }
}
