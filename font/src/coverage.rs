//! **Coverage groups and the `CVRG` table**
//! (`docs/design/20261016_02_BUNDLED_FONT_FEATURES_PLAN.md`, 13:40 EDT
//! amendment).
//!
//! A bake declares its text coverage as named groups with different ranges
//! and different rules for a codepoint the face lacks: TEXT (letters, digits,
//! punctuation and spaces for Western and Central/Eastern European
//! languages; a missing codepoint fails the bake unless waived), SYMBOLS and
//! MATH (a missing codepoint is skipped and reported). The atlas records the
//! declared groups and what was missing in a per-face `CVRG` table, absent
//! for [`Coverage::Shipped`].
//!
//! The ranges are written twice, here and in `fonts/coverage-groups.txt`
//! (which `fonts/ofl_subset.py` reads); a test holds the two equal.

use crate::GlyphStyle;
use crate::atlas::{ATLAS_TABLE_FACE_ALL, TableBody};

/// Which coverage group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum CoverageGroupId {
    /// European letters, digits, punctuation and spaces.
    Text = 0,
    /// Common symbols: the Latin-1 symbols and common European Unicode ones.
    Symbols = 1,
    /// Common mathematical signs.
    Math = 2,
}

impl CoverageGroupId {
    /// The id for a `CVRG` byte, if this build knows it.
    pub const fn from_u8(n: u8) -> Option<Self> {
        match n {
            0 => Some(Self::Text),
            1 => Some(Self::Symbols),
            2 => Some(Self::Math),
            _ => None,
        }
    }
}

/// What a bake does with a declared codepoint the face lacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingPolicy {
    /// A codepoint the face lacks fails the bake, unless waived for that face.
    Fail = 0,
    /// A codepoint the face lacks is skipped and listed in the bake report.
    Skip = 1,
}

/// One coverage group: its ranges and its missing-codepoint rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoverageGroup {
    /// Which group.
    pub id: CoverageGroupId,
    /// Inclusive codepoint ranges, ascending and non-overlapping.
    pub ranges: &'static [(char, char)],
    /// What a missing codepoint does.
    pub missing: MissingPolicy,
}

impl CoverageGroup {
    /// Every codepoint the group declares.
    pub fn codepoints(&self) -> Vec<char> {
        self.ranges
            .iter()
            .flat_map(|&(lo, hi)| (lo as u32..=hi as u32).filter_map(char::from_u32))
            .collect()
    }
}

/// **TEXT**: 287 codepoints. Basic Latin, no-break space, soft hyphen, the
/// Latin-1 letters and Latin Extended-A.
pub const COVERAGE_TEXT: CoverageGroup = CoverageGroup {
    id: CoverageGroupId::Text,
    ranges: &[
        ('\u{0020}', '\u{007E}'),
        ('\u{00A0}', '\u{00A0}'),
        ('\u{00AD}', '\u{00AD}'),
        ('\u{00C0}', '\u{00D6}'),
        ('\u{00D8}', '\u{00F6}'),
        ('\u{00F8}', '\u{00FF}'),
        ('\u{0100}', '\u{017F}'),
    ],
    missing: MissingPolicy::Fail,
};

/// **SYMBOLS**: 63 codepoints. The Latin-1 symbols, General Punctuation
/// dashes, quotes, daggers, bullet, ellipsis, per mille, primes and
/// guillemets, the European currencies Latin-1 lacks, № ™ ℮.
pub const COVERAGE_SYMBOLS: CoverageGroup = CoverageGroup {
    id: CoverageGroupId::Symbols,
    ranges: &[
        ('\u{00A1}', '\u{00AC}'),
        ('\u{00AE}', '\u{00BF}'),
        ('\u{00D7}', '\u{00D7}'),
        ('\u{00F7}', '\u{00F7}'),
        ('\u{2010}', '\u{2015}'),
        ('\u{2018}', '\u{201F}'),
        ('\u{2020}', '\u{2022}'),
        ('\u{2026}', '\u{2026}'),
        ('\u{2030}', '\u{2030}'),
        ('\u{2032}', '\u{2033}'),
        ('\u{2039}', '\u{203A}'),
        ('\u{20AC}', '\u{20AC}'),
        ('\u{20B4}', '\u{20B4}'),
        ('\u{20BA}', '\u{20BA}'),
        ('\u{20BD}', '\u{20BE}'),
        ('\u{2116}', '\u{2116}'),
        ('\u{2122}', '\u{2122}'),
        ('\u{212E}', '\u{212E}'),
    ],
    missing: MissingPolicy::Skip,
};

/// **MATH**: 77 codepoints of common mathematical signs, superscripts,
/// subscripts and vulgar fractions. U+2044 FRACTION SLASH is deliberately
/// left out: rustybuzz's automatic fractions around it cannot be carried by
/// the layout tables.
pub const COVERAGE_MATH: CoverageGroup = CoverageGroup {
    id: CoverageGroupId::Math,
    ranges: &[
        ('\u{00AC}', '\u{00AC}'),
        ('\u{00B0}', '\u{00B3}'),
        ('\u{00B5}', '\u{00B5}'),
        ('\u{00B7}', '\u{00B7}'),
        ('\u{00B9}', '\u{00B9}'),
        ('\u{00BC}', '\u{00BE}'),
        ('\u{00D7}', '\u{00D7}'),
        ('\u{00F7}', '\u{00F7}'),
        ('\u{2030}', '\u{2030}'),
        ('\u{2032}', '\u{2033}'),
        ('\u{2070}', '\u{2070}'),
        ('\u{2074}', '\u{207F}'),
        ('\u{2080}', '\u{208E}'),
        ('\u{2153}', '\u{215E}'),
        ('\u{2202}', '\u{2202}'),
        ('\u{2205}', '\u{2209}'),
        ('\u{220F}', '\u{220F}'),
        ('\u{2211}', '\u{2213}'),
        ('\u{221A}', '\u{221A}'),
        ('\u{221E}', '\u{221E}'),
        ('\u{2229}', '\u{222B}'),
        ('\u{2248}', '\u{2248}'),
        ('\u{2260}', '\u{2260}'),
        ('\u{2264}', '\u{2265}'),
        ('\u{2282}', '\u{2283}'),
    ],
    missing: MissingPolicy::Skip,
};

/// **What a bake covers.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Coverage {
    /// `TEXT_RANGES` plus what the face defines in the Private Use Area:
    /// `FontAtlasBuilder::add_shipped_coverage`, for the Roboto faces. No
    /// `CVRG` table.
    Shipped,
    /// The union of these groups, recorded in `CVRG`.
    Groups(Vec<CoverageGroup>),
}

impl Coverage {
    /// The declared ranges, merged (for the layout tables' `Cmap` scope).
    pub fn ranges(&self) -> Vec<(char, char)> {
        match self {
            Coverage::Shipped => crate::TEXT_RANGES.to_vec(),
            Coverage::Groups(groups) => {
                let mut cps: Vec<u32> = groups
                    .iter()
                    .flat_map(|g| g.codepoints())
                    .map(|c| c as u32)
                    .collect();
                cps.sort_unstable();
                cps.dedup();
                let mut out: Vec<(char, char)> = Vec::new();
                for cp in cps {
                    let ch = char::from_u32(cp).expect("from a char");
                    match out.last_mut() {
                        Some((_, hi)) if *hi as u32 + 1 == cp => *hi = ch,
                        _ => out.push((ch, ch)),
                    }
                }
                out
            }
        }
    }
}

/// One declared group of one face, as `CVRG` records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupCoverage {
    /// The group id byte ([`CoverageGroupId::from_u8`]; a reader keeps an
    /// unknown one raw).
    pub id: u8,
    /// The group's missing-codepoint rule.
    pub policy: MissingPolicy,
    /// The declared ranges.
    pub ranges: Vec<(char, char)>,
    /// Declared codepoints the face lacks, skipped.
    pub skipped: Vec<char>,
    /// Declared TEXT codepoints the face lacks, waived for this face.
    pub waived: Vec<char>,
}

/// Every declared group of one face (`CVRG`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaceCoverage {
    /// The face's style.
    pub style: GlyphStyle,
    /// Its groups, ascending by id.
    pub groups: Vec<GroupCoverage>,
}

impl FaceCoverage {
    /// Whether `cp` lies in a declared range of any group.
    pub fn declares(&self, cp: u32) -> bool {
        self.groups.iter().any(|g| {
            g.ranges
                .iter()
                .any(|&(lo, hi)| (lo as u32..=hi as u32).contains(&cp))
        })
    }

    /// Every missing (skipped or waived) codepoint of every group.
    pub fn missing(&self) -> impl Iterator<Item = char> + '_ {
        self.groups
            .iter()
            .flat_map(|g| g.skipped.iter().chain(&g.waived).copied())
    }

    fn encode(&self) -> Result<Vec<u8>, &'static str> {
        let mut b = Vec::new();
        b.extend_from_slice(&(self.groups.len() as u16).to_le_bytes());
        b.extend_from_slice(&[0; 2]);
        let mut prev: Option<u8> = None;
        for g in &self.groups {
            if prev.is_some_and(|p| p >= g.id) {
                return Err("CVRG groups are not ascending by id, or one repeats");
            }
            prev = Some(g.id);
            let mut missing: Vec<(u32, u8)> = g
                .skipped
                .iter()
                .map(|&c| (c as u32, 0))
                .chain(g.waived.iter().map(|&c| (c as u32, 1)))
                .collect();
            missing.sort_unstable();
            if missing.windows(2).any(|w| w[0].0 == w[1].0) {
                return Err("CVRG lists a missing codepoint twice");
            }
            check_ranges(&g.ranges)?;
            b.push(g.id);
            b.push(g.policy as u8);
            b.extend_from_slice(&(g.ranges.len() as u16).to_le_bytes());
            b.extend_from_slice(&(missing.len() as u16).to_le_bytes());
            b.extend_from_slice(&[0; 2]);
            for &(lo, hi) in &g.ranges {
                b.extend_from_slice(&(lo as u32).to_le_bytes());
                b.extend_from_slice(&(hi as u32).to_le_bytes());
            }
            for (cp, kind) in missing {
                b.extend_from_slice(&cp.to_le_bytes());
                b.push(kind);
                b.extend_from_slice(&[0; 3]);
            }
        }
        Ok(b)
    }

    fn decode(style: GlyphStyle, data: &[u8]) -> Result<Self, &'static str> {
        const SHORT: &str = "CVRG is truncated or has bytes past its last group (wrong length)";
        let u16_at = |o: usize| -> Result<u16, &'static str> {
            data.get(o..o + 2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .ok_or(SHORT)
        };
        let u32_at = |o: usize| -> Result<u32, &'static str> {
            data.get(o..o + 4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .ok_or(SHORT)
        };
        let n = u16_at(0)? as usize;
        if u16_at(2)? != 0 {
            return Err("CVRG has non-zero reserved bytes");
        }
        let mut pos = 4;
        let mut groups: Vec<GroupCoverage> = Vec::with_capacity(n);
        for _ in 0..n {
            let id = *data.get(pos).ok_or(SHORT)?;
            let policy = match *data.get(pos + 1).ok_or(SHORT)? {
                0 => MissingPolicy::Fail,
                1 => MissingPolicy::Skip,
                _ => return Err("CVRG group has an unknown missing policy"),
            };
            let ranges_n = u16_at(pos + 2)? as usize;
            let missing_n = u16_at(pos + 4)? as usize;
            if u16_at(pos + 6)? != 0 {
                return Err("CVRG has non-zero reserved bytes");
            }
            pos += 8;
            let mut ranges = Vec::with_capacity(ranges_n);
            for _ in 0..ranges_n {
                let lo = char::from_u32(u32_at(pos)?).ok_or("CVRG range is not a codepoint")?;
                let hi = char::from_u32(u32_at(pos + 4)?).ok_or("CVRG range is not a codepoint")?;
                ranges.push((lo, hi));
                pos += 8;
            }
            check_ranges(&ranges)?;
            let (mut skipped, mut waived) = (Vec::new(), Vec::new());
            let mut prev: Option<u32> = None;
            for _ in 0..missing_n {
                let cp = u32_at(pos)?;
                let kind = *data.get(pos + 4).ok_or(SHORT)?;
                if data.get(pos + 5..pos + 8).ok_or(SHORT)? != [0, 0, 0] {
                    return Err("CVRG has non-zero reserved bytes");
                }
                pos += 8;
                if prev.is_some_and(|p| p >= cp) {
                    return Err("CVRG missing codepoints are not ascending, or one repeats");
                }
                prev = Some(cp);
                let ch = char::from_u32(cp).ok_or("CVRG missing entry is not a codepoint")?;
                match kind {
                    0 => skipped.push(ch),
                    1 => waived.push(ch),
                    _ => return Err("CVRG missing entry has an unknown kind"),
                }
            }
            if groups.last().is_some_and(|g| g.id >= id) {
                return Err("CVRG groups are not ascending by id, or one repeats");
            }
            groups.push(GroupCoverage {
                id,
                policy,
                ranges,
                skipped,
                waived,
            });
        }
        if pos != data.len() {
            return Err(SHORT);
        }
        Ok(FaceCoverage { style, groups })
    }
}

fn check_ranges(ranges: &[(char, char)]) -> Result<(), &'static str> {
    for (i, &(lo, hi)) in ranges.iter().enumerate() {
        if lo > hi || (i > 0 && ranges[i - 1].1 >= lo) {
            return Err("CVRG ranges are not ascending and non-overlapping");
        }
    }
    Ok(())
}

/// The `CVRG` tables of `coverage` as table bodies.
pub(crate) fn encode_tables(coverage: &[FaceCoverage]) -> Result<Vec<TableBody>, &'static str> {
    let mut out = Vec::new();
    for face in coverage {
        if out.iter().any(|t: &TableBody| t.face == face.style as u8) {
            return Err("two CVRG tables for one face");
        }
        out.push(TableBody {
            tag: *b"CVRG",
            face: face.style as u8,
            body: face.encode()?,
        });
    }
    Ok(out)
}

/// The `CVRG` tables among `bodies`, checked against each face's `Cmap`:
/// every non-control `Cmap` codepoint lies in a declared range, and no
/// missing codepoint is in the `Cmap`.
pub(crate) fn decode_tables(
    bodies: &[(&[u8; 4], u8, &[u8])],
    layout: Option<&crate::AtlasLayout>,
) -> Result<Vec<FaceCoverage>, &'static str> {
    let mut out = Vec::new();
    for &(tag, face, body) in bodies {
        if tag != b"CVRG" {
            continue;
        }
        if face == ATLAS_TABLE_FACE_ALL {
            return Err("a CVRG table belongs to the whole atlas rather than a face");
        }
        let style = GlyphStyle::try_from_ordinal(face).ok_or("CVRG names an unknown face")?;
        let cov = FaceCoverage::decode(style, body)?;
        if let Some(cmap) = layout.and_then(|l| l.face(style)).map(|f| &f.cmap) {
            for &(cp, _) in cmap {
                let control = char::from_u32(cp).is_some_and(char::is_control);
                if !control && !cov.declares(cp) {
                    return Err("a Cmap codepoint lies outside the CVRG's declared ranges");
                }
            }
            for ch in cov.missing() {
                if cmap.binary_search_by_key(&(ch as u32), |c| c.0).is_ok() {
                    return Err("a CVRG missing codepoint is in the Cmap");
                }
            }
        }
        out.push(cov);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Rust groups and `fonts/coverage-groups.txt` are one list.
    #[test]
    fn the_groups_match_the_file_the_subset_script_reads() {
        let text = include_str!("../../fonts/coverage-groups.txt");
        let mut from_file: Vec<(String, String, u32, u32)> = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let (g, p, r) = (
                parts.next().unwrap(),
                parts.next().unwrap(),
                parts.next().unwrap(),
            );
            let r = r.trim_start_matches("U+");
            let (lo, hi) = r.split_once('-').unwrap_or((r, r));
            from_file.push((
                g.to_string(),
                p.to_string(),
                u32::from_str_radix(lo, 16).unwrap(),
                u32::from_str_radix(hi, 16).unwrap(),
            ));
        }
        let mut from_rust = Vec::new();
        for (name, g) in [
            ("TEXT", COVERAGE_TEXT),
            ("SYMBOLS", COVERAGE_SYMBOLS),
            ("MATH", COVERAGE_MATH),
        ] {
            let p = match g.missing {
                MissingPolicy::Fail => "fail",
                MissingPolicy::Skip => "skip",
            };
            for &(lo, hi) in g.ranges {
                from_rust.push((name.to_string(), p.to_string(), lo as u32, hi as u32));
            }
        }
        assert_eq!(from_file, from_rust);
        assert_eq!(COVERAGE_TEXT.codepoints().len(), 287);
        assert_eq!(COVERAGE_SYMBOLS.codepoints().len(), 63);
        assert_eq!(COVERAGE_MATH.codepoints().len(), 77);
        let all = Coverage::Groups(vec![COVERAGE_TEXT, COVERAGE_SYMBOLS, COVERAGE_MATH]);
        let n: u32 = all
            .ranges()
            .iter()
            .map(|&(lo, hi)| hi as u32 - lo as u32 + 1)
            .sum();
        assert_eq!(n, 411);
    }

    /// A `CVRG` whose missing codepoint is in the face's `Cmap` is refused on
    /// read, and so is a `Cmap` codepoint outside the declared ranges.
    #[test]
    fn cvrg_must_agree_with_the_cmap() {
        use crate::{AtlasLayout, FaceLayout, FontAtlas};
        let face = FaceLayout {
            style: GlyphStyle::Regular,
            units_per_em: 1000,
            hidden_glyph: 1,
            ascender: 800,
            descender: -200,
            line_gap: 0,
            cap_height: 700,
            x_height: 500,
            features: vec![],
            cmap: vec![(0x20, 1), (0x61, 2)],
            hmtx: vec![(0, 500), (1, 250), (2, 500)],
            gsub: vec![],
            gpos: vec![],
        };
        let cov = |skipped: Vec<char>, ranges: Vec<(char, char)>| FaceCoverage {
            style: GlyphStyle::Regular,
            groups: vec![GroupCoverage {
                id: 1,
                policy: MissingPolicy::Skip,
                ranges,
                skipped,
                waived: vec![],
            }],
        };
        let load = |c: FaceCoverage| {
            let mut atlas = FontAtlas::empty(1, 1, 3);
            atlas.layout = Some(AtlasLayout::from_faces(vec![face.clone()]).unwrap());
            atlas.coverage = vec![c];
            FontAtlas::from_bytes(&atlas.to_bytes())
        };
        assert!(load(cov(vec!['b'], vec![(' ', 'z')])).is_ok());
        let err = load(cov(vec!['a'], vec![(' ', 'z')])).expect_err("missing a, but mapped");
        assert!(err.contains("missing codepoint is in the Cmap"), "{err}");
        let err = load(cov(vec![], vec![('b', 'z')])).expect_err("a and space undeclared");
        assert!(err.contains("outside"), "{err}");
    }

    #[test]
    fn cvrg_round_trips_and_refuses_bad_bodies() {
        let face = FaceCoverage {
            style: GlyphStyle::Regular,
            groups: vec![
                GroupCoverage {
                    id: 0,
                    policy: MissingPolicy::Fail,
                    ranges: vec![('a', 'z')],
                    skipped: vec![],
                    waived: vec!['q'],
                },
                GroupCoverage {
                    id: 7,
                    policy: MissingPolicy::Skip,
                    ranges: vec![('\u{2010}', '\u{2015}'), ('\u{2116}', '\u{2116}')],
                    skipped: vec!['\u{2012}', '\u{2015}'],
                    waived: vec![],
                },
            ],
        };
        let body = face.encode().unwrap();
        assert_eq!(
            FaceCoverage::decode(GlyphStyle::Regular, &body).unwrap(),
            face
        );
        let mut extra = body.clone();
        extra.push(0);
        assert!(FaceCoverage::decode(GlyphStyle::Regular, &extra).is_err());
        let mut bad_policy = body.clone();
        bad_policy[5] = 9;
        assert!(FaceCoverage::decode(GlyphStyle::Regular, &bad_policy).is_err());
    }
}
