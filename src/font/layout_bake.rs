//! **Bake-time extraction of a face's layout tables** (plan section 4.9).
//!
//! [`FaceLayout::extract`] reads a TTF with ttf-parser 0.25 and flattens
//! what rustybuzz would apply to the face's baked text into the tables of
//! [`crate::font::layout`]: the lookups of the features rustybuzz enables
//! for horizontal left-to-right text (plus `lnum` and `pnum`, which
//! [`crate::font::TextShaper`] enables, plus any extra features asked for),
//! under script `latn` (falling back to `DFLT`) with no language system,
//! restricted to the glyphs reachable from the face's `Cmap`.
//!
//! Anything the tables cannot carry and that can fire on a reachable glyph
//! is an error rather than an approximation. Then the equivalence check
//! (section 4.9, "The check") shapes a corpus with both shapers and refuses
//! the face on the first difference.

use crate::font::GlyphStyle;
use crate::font::layout::{
    ContextSide, ContextSingle, FaceLayout, GsubLookup, GsubRules, Ligature,
    MAX_LIGATURE_COMPONENTS, MIN_LIGATURE_COMPONENTS,
};
use crate::font::shaper::ShapedRun;
use crate::font::{MAX_RAW_GLYPH_ID, TEXT_RANGES};
use ttf_parser::GlyphId;
use ttf_parser::gpos::{PairAdjustment, PositioningSubtable, ValueRecord};
use ttf_parser::gsub::{SingleSubstitution, SubstitutionSubtable};
use ttf_parser::opentype_layout::{
    ChainedContextLookup, ClassDefinition, Coverage, LayoutTable, Lookup,
};

/// The features rustybuzz 0.20 enables for horizontal left-to-right text
/// with the default shaper (`hb/ot_shape.rs`, `collect_features`): `rvrn`
/// (its own stage), the direction features, `rand`, the common features and
/// the horizontal ones. `frac`, `numr` and `dnom` are left out: they apply
/// only around U+2044 FRACTION SLASH, which is outside the baked ranges.
pub const RUSTYBUZZ_DEFAULT_FEATURES: &[[u8; 4]] = &[
    *b"rvrn", *b"ltra", *b"ltrm", *b"rand", *b"abvm", *b"blwm", *b"ccmp", *b"locl", *b"mark",
    *b"mkmk", *b"rlig", *b"calt", *b"clig", *b"curs", *b"dist", *b"kern", *b"liga", *b"rclt",
];

/// The features [`crate::font::TextShaper`] turns on by default.
pub const TEXT_SHAPER_DEFAULT_FEATURES: &[[u8; 4]] = &[*b"lnum", *b"pnum"];

/// What to extract beyond the defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayoutOptions {
    /// Optional features to apply beyond the defaults, e.g. `*b"tnum"`.
    pub extra_features: Vec<[u8; 4]>,
    /// The declared text coverage: inclusive codepoint ranges. The `Cmap`
    /// holds every codepoint of it the face maps (plus the C0/C1 controls the
    /// face maps), and the equivalence corpus runs over those. Defaults to
    /// [`TEXT_RANGES`], so every existing caller and result is unchanged.
    pub coverage: Vec<(char, char)>,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            extra_features: Vec::new(),
            coverage: TEXT_RANGES.to_vec(),
        }
    }
}

impl LayoutOptions {
    /// The user features a [`crate::font::TextShaper`] must be given to shape
    /// as this layout does: `lnum`, `pnum` and the extras.
    pub fn text_shaper_features(&self) -> Vec<(ttf_parser::Tag, u32)> {
        let mut tags: Vec<[u8; 4]> = TEXT_SHAPER_DEFAULT_FEATURES.to_vec();
        for f in &self.extra_features {
            if !tags.contains(f) {
                tags.push(*f);
            }
        }
        tags.iter()
            .map(|t| (ttf_parser::Tag::from_bytes(t), 1))
            .collect()
    }
}

/// Why a face's layout could not be extracted, or failed the check.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum LayoutError {
    /// A lookup in an applied feature can fire on reachable glyphs and is not
    /// a kind the tables carry.
    UnsupportedLookup {
        /// The feature it came from.
        feature: [u8; 4],
        /// Its index in the GSUB or GPOS lookup list.
        lookup_index: u16,
        /// Its OpenType lookup type (GSUB 1 to 8, GPOS 1 to 9).
        lookup_type: u16,
        /// What about it is unsupported.
        why: &'static str,
    },
    /// A reachable glyph is a mark (GDEF class 3).
    MarkReachable {
        /// The glyph.
        glyph: u16,
    },
    /// A positioning value other than the first glyph's x advance is
    /// non-zero.
    UnsupportedPositioning {
        /// The left glyph of the pair.
        left: u16,
        /// The right glyph of the pair.
        right: u16,
    },
    /// The two shapers disagree on a corpus string.
    Mismatch {
        /// The string.
        text: String,
        /// What rustybuzz (through `TextShaper`) gave.
        rustybuzz: ShapedRun,
        /// What `AtlasShaper` gave.
        atlas: ShapedRun,
    },
    /// A reachable glyph id is above [`MAX_RAW_GLYPH_ID`].
    GlyphIdTooWide(u16),
    /// A limit of the tables is exceeded.
    TooLarge(&'static str),
    /// The face does not parse, or carries something the extraction refuses.
    Face(String),
}

impl core::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let tag = |t: &[u8; 4]| String::from_utf8_lossy(t).into_owned();
        match self {
            Self::UnsupportedLookup {
                feature,
                lookup_index,
                lookup_type,
                why,
            } => write!(
                f,
                "lookup {lookup_index} (type {lookup_type}) of feature '{}' can fire on baked \
                 text and cannot be carried: {why}",
                tag(feature)
            ),
            Self::MarkReachable { glyph } => write!(
                f,
                "glyph {glyph} is a mark (GDEF class 3) reachable from the baked text"
            ),
            Self::UnsupportedPositioning { left, right } => write!(
                f,
                "the pair ({left}, {right}) adjusts something other than the first glyph's x \
                 advance"
            ),
            Self::Mismatch {
                text,
                rustybuzz,
                atlas,
            } => {
                let run = |r: &ShapedRun| {
                    r.glyphs
                        .iter()
                        .map(|g| {
                            format!(
                                "{}@{}+{}({},{})",
                                g.glyph_id, g.cluster, g.x_advance, g.x_offset, g.y_offset
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                write!(
                    f,
                    "the shapers disagree on {text:?}:\n  rustybuzz: [{}] total {} upem {}\n  \
                     atlas:     [{}] total {} upem {}",
                    run(rustybuzz),
                    rustybuzz.total_advance,
                    rustybuzz.units_per_em,
                    run(atlas),
                    atlas.total_advance,
                    atlas.units_per_em
                )
            }
            Self::GlyphIdTooWide(g) => {
                write!(f, "reachable glyph {g} is above MAX_RAW_GLYPH_ID (16383)")
            }
            Self::TooLarge(what) => write!(f, "layout tables too large: {what}"),
            Self::Face(e) => write!(f, "face: {e}"),
        }
    }
}

impl std::error::Error for LayoutError {}

/// The codepoints a `Cmap` covers: the declared `coverage` and the C0/C1
/// controls.
pub(crate) fn cmap_scope(coverage: &[(char, char)]) -> impl Iterator<Item = char> + use<> {
    let controls = [('\u{0}', '\u{1F}'), ('\u{7F}', '\u{9F}')];
    let mut cps: Vec<char> = coverage
        .iter()
        .chain(controls.iter())
        .flat_map(|&(lo, hi)| (lo as u32..=hi as u32).filter_map(char::from_u32))
        .collect();
    cps.sort_unstable();
    cps.dedup();
    cps.into_iter()
}

/// The lookups one table applies, in application order, each with the
/// feature it came from; and the applied features the face has.
#[allow(clippy::type_complexity)]
fn selected_lookups(
    table: &LayoutTable,
    features: &[[u8; 4]],
    coords: &[ttf_parser::NormalizedCoordinate],
) -> Result<(Vec<(u16, [u8; 4])>, Vec<[u8; 4]>), LayoutError> {
    if let Some(v) = table.variations
        && v.find_index(coords).is_some()
    {
        return Err(LayoutError::Face(
            "a FeatureVariations record applies at the default instance".into(),
        ));
    }
    let script = [b"latn", b"DFLT", b"dflt"]
        .iter()
        .find_map(|t| table.scripts.find(ttf_parser::Tag::from_bytes(t)));
    let Some(langsys) = script.and_then(|s| s.default_language) else {
        return Ok((Vec::new(), Vec::new()));
    };
    let feature_of = |idx: u16| table.features.get(idx);
    // (stage, lookup, feature)
    let mut picked: Vec<(u8, u16, [u8; 4])> = Vec::new();
    let mut found: Vec<[u8; 4]> = Vec::new();
    if let Some(req) = langsys.required_feature.and_then(feature_of) {
        found.push(req.tag.to_bytes());
        for l in req.lookup_indices {
            picked.push((1, l, req.tag.to_bytes()));
        }
    }
    for want in features {
        // HarfBuzz takes the FIRST feature of the language system with the
        // tag, not all of them.
        let hit = langsys
            .feature_indices
            .into_iter()
            .filter_map(feature_of)
            .find(|f| &f.tag.to_bytes() == want);
        if let Some(feature) = hit {
            found.push(*want);
            let stage = if want == b"rvrn" { 0 } else { 1 };
            for l in feature.lookup_indices {
                picked.push((stage, l, *want));
            }
        }
    }
    // Within a stage, by lookup index; a lookup in several features once,
    // named after the smallest tag.
    picked.sort();
    picked.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    found.sort_unstable();
    found.dedup();
    Ok((picked.into_iter().map(|(_, l, f)| (l, f)).collect(), found))
}

fn lookup_type_of(sub: &SubstitutionSubtable) -> u16 {
    match sub {
        SubstitutionSubtable::Single(_) => 1,
        SubstitutionSubtable::Multiple(_) => 2,
        SubstitutionSubtable::Alternate(_) => 3,
        SubstitutionSubtable::Ligature(_) => 4,
        SubstitutionSubtable::Context(_) => 5,
        SubstitutionSubtable::ChainContext(_) => 6,
        SubstitutionSubtable::ReverseChainSingle(_) => 8,
    }
}

fn gpos_type_of(sub: &PositioningSubtable) -> u16 {
    match sub {
        PositioningSubtable::Single(_) => 1,
        PositioningSubtable::Pair(_) => 2,
        PositioningSubtable::Cursive(_) => 3,
        PositioningSubtable::MarkToBase(_) => 4,
        PositioningSubtable::MarkToLigature(_) => 5,
        PositioningSubtable::MarkToMark(_) => 6,
        PositioningSubtable::Context(_) => 7,
        PositioningSubtable::ChainContext(_) => 8,
    }
}

/// What a single-substitution lookup does to `g`, if anything (the first
/// subtable that covers it decides).
fn single_of(lookup: &Lookup, g: u16) -> Option<u16> {
    for sub in lookup.subtables.into_iter::<SubstitutionSubtable>() {
        let SubstitutionSubtable::Single(s) = sub else {
            return None;
        };
        let Some(idx) = s.coverage().get(GlyphId(g)) else {
            continue;
        };
        return match s {
            SingleSubstitution::Format1 { delta, .. } => Some(g.wrapping_add(delta as u16)),
            SingleSubstitution::Format2 { substitutes, .. } => substitutes.get(idx).map(|g| g.0),
        };
    }
    None
}

fn is_single_lookup(lookup: &Lookup) -> bool {
    !lookup.subtables.is_empty()
        && lookup
            .subtables
            .into_iter::<SubstitutionSubtable>()
            .all(|s| matches!(s, SubstitutionSubtable::Single(_)))
}

/// The glyphs of `set` among the reachable ones.
fn covered(cov: &Coverage, reach: &[u16]) -> Vec<u16> {
    reach
        .iter()
        .copied()
        .filter(|&g| cov.contains(GlyphId(g)))
        .collect()
}

fn in_class(classes: &ClassDefinition, class: u16, reach: &[u16]) -> Vec<u16> {
    reach
        .iter()
        .copied()
        .filter(|&g| classes.get(GlyphId(g)) == class)
        .collect()
}

/// One chaining rule, resolved over the reachable glyphs: the glyphs each
/// backtrack, input and lookahead position can match (backtrack nearest
/// first), and its nested lookups.
struct ChainRule {
    backtrack: Vec<Vec<u16>>,
    input: Vec<Vec<u16>>,
    lookahead: Vec<Vec<u16>>,
    lookups: Vec<(u16, u16)>,
}

/// Every rule of a chaining subtable, in the order HarfBuzz tries them for
/// a first glyph `g` (only rules whose first position matches `g`).
fn chain_rules_for(sub: &ChainedContextLookup, g: u16, reach: &[u16]) -> Vec<ChainRule> {
    let mut out = Vec::new();
    match *sub {
        ChainedContextLookup::Format1 { coverage, sets } => {
            let Some(idx) = coverage.get(GlyphId(g)) else {
                return out;
            };
            let Some(set) = sets.get(idx) else {
                return out;
            };
            for rule in set {
                let one = |v: u16| if reach.contains(&v) { vec![v] } else { vec![] };
                out.push(ChainRule {
                    backtrack: rule.backtrack.into_iter().map(one).collect(),
                    input: std::iter::once(vec![g])
                        .chain(rule.input.into_iter().map(one))
                        .collect(),
                    lookahead: rule.lookahead.into_iter().map(one).collect(),
                    lookups: rule
                        .lookups
                        .into_iter()
                        .map(|r| (r.sequence_index, r.lookup_list_index))
                        .collect(),
                });
            }
        }
        ChainedContextLookup::Format2 {
            coverage,
            backtrack_classes,
            input_classes,
            lookahead_classes,
            sets,
        } => {
            if coverage.get(GlyphId(g)).is_none() {
                return out;
            }
            let Some(set) = sets.get(input_classes.get(GlyphId(g))) else {
                return out;
            };
            for rule in set {
                out.push(ChainRule {
                    backtrack: rule
                        .backtrack
                        .into_iter()
                        .map(|c| in_class(&backtrack_classes, c, reach))
                        .collect(),
                    input: std::iter::once(vec![g])
                        .chain(
                            rule.input
                                .into_iter()
                                .map(|c| in_class(&input_classes, c, reach)),
                        )
                        .collect(),
                    lookahead: rule
                        .lookahead
                        .into_iter()
                        .map(|c| in_class(&lookahead_classes, c, reach))
                        .collect(),
                    lookups: rule
                        .lookups
                        .into_iter()
                        .map(|r| (r.sequence_index, r.lookup_list_index))
                        .collect(),
                });
            }
        }
        ChainedContextLookup::Format3 {
            coverage,
            backtrack_coverages,
            input_coverages,
            lookahead_coverages,
            lookups,
        } => {
            if coverage.get(GlyphId(g)).is_none() {
                return out;
            }
            out.push(ChainRule {
                backtrack: backtrack_coverages
                    .into_iter()
                    .map(|c| covered(&c, reach))
                    .collect(),
                input: std::iter::once(vec![g])
                    .chain(input_coverages.into_iter().map(|c| covered(&c, reach)))
                    .collect(),
                lookahead: lookahead_coverages
                    .into_iter()
                    .map(|c| covered(&c, reach))
                    .collect(),
                lookups: lookups
                    .into_iter()
                    .map(|r| (r.sequence_index, r.lookup_list_index))
                    .collect(),
            });
        }
    }
    out
}

/// The flattening of one GSUB lookup over the reachable glyphs.
fn flatten_gsub(
    gsub: &LayoutTable,
    lookup_index: u16,
    feature: [u8; 4],
    reach: &[u16],
) -> Result<Option<GsubRules>, LayoutError> {
    let Some(lookup) = gsub.lookups.get(lookup_index) else {
        return Ok(None);
    };
    let unsupported = |lookup_type: u16, why: &'static str| LayoutError::UnsupportedLookup {
        feature,
        lookup_index,
        lookup_type,
        why,
    };
    let subs: Vec<SubstitutionSubtable> = lookup.subtables.into_iter().collect();
    let Some(first) = subs.first() else {
        return Ok(None);
    };
    let kind = lookup_type_of(first);
    // Can any subtable's first glyph be reachable at all?
    let live = subs
        .iter()
        .any(|s| reach.iter().any(|&g| s.coverage().contains(GlyphId(g))));
    if !live {
        return Ok(None);
    }
    if lookup.flags.0 & !0x0008 != 0 || lookup.mark_filtering_set.is_some() {
        return Err(unsupported(
            kind,
            "lookup flags other than IgnoreMarks (which has no effect without reachable marks)",
        ));
    }
    match kind {
        1 => {
            let mut rules = Vec::new();
            for &g in reach {
                if let Some(to) = single_of(&lookup, g) {
                    rules.push((g, to));
                }
            }
            Ok((!rules.is_empty()).then_some(GsubRules::Single(rules)))
        }
        4 => {
            let mut ligs = Vec::new();
            for &g in reach {
                for sub in &subs {
                    let SubstitutionSubtable::Ligature(l) = sub else {
                        continue;
                    };
                    let Some(idx) = l.coverage.get(GlyphId(g)) else {
                        continue;
                    };
                    let Some(set) = l.ligature_sets.get(idx) else {
                        continue;
                    };
                    for lig in set {
                        let mut components = vec![g];
                        components.extend(lig.components.into_iter().map(|c| c.0));
                        if components[1..].iter().any(|c| !reach.contains(c)) {
                            continue; // can never match
                        }
                        if !(MIN_LIGATURE_COMPONENTS..=MAX_LIGATURE_COMPONENTS)
                            .contains(&components.len())
                        {
                            return Err(unsupported(
                                4,
                                "a reachable ligature of 1 or more than 8 components",
                            ));
                        }
                        ligs.push(Ligature {
                            components,
                            out: lig.glyph.0,
                        });
                    }
                }
            }
            Ok((!ligs.is_empty()).then_some(GsubRules::Ligature(ligs)))
        }
        6 => {
            let mut records: Vec<ContextSingle> = Vec::new();
            for &g in reach {
                // Candidates in the order HarfBuzz tries them: (side,
                // contexts, output or None for a rule that matches and does
                // nothing).
                let mut hits: Vec<(ContextSide, Vec<u16>, Option<u16>)> = Vec::new();
                for sub in &subs {
                    let SubstitutionSubtable::ChainContext(c) = sub else {
                        return Err(unsupported(6, "mixed subtable types"));
                    };
                    for rule in chain_rules_for(c, g, reach) {
                        let dead = rule
                            .backtrack
                            .iter()
                            .chain(&rule.input)
                            .chain(&rule.lookahead)
                            .any(|p| p.is_empty());
                        if dead {
                            continue;
                        }
                        let shape_ok = rule.input.len() == 1
                            && rule.backtrack.len() + rule.lookahead.len() == 1
                            && rule.lookups.len() == 1
                            && rule.lookups[0].0 == 0;
                        if !shape_ok {
                            return Err(unsupported(
                                6,
                                "a reachable contextual rule other than one input glyph with one \
                                 context glyph and one nested lookup",
                            ));
                        }
                        let Some(nested) = gsub.lookups.get(rule.lookups[0].1) else {
                            continue;
                        };
                        if !is_single_lookup(&nested) {
                            return Err(unsupported(
                                6,
                                "a reachable contextual rule whose nested lookup is not a single \
                                 substitution",
                            ));
                        }
                        let (side, ctx) = if rule.backtrack.len() == 1 {
                            (ContextSide::Before, rule.backtrack[0].clone())
                        } else {
                            (ContextSide::After, rule.lookahead[0].clone())
                        };
                        hits.push((side, ctx, single_of(&nested, g)));
                    }
                }
                let outs: Vec<Option<u16>> = hits.iter().map(|h| h.2).collect();
                if outs.iter().all(Option::is_none) {
                    continue;
                }
                let first = outs.iter().flatten().next().copied();
                if outs.iter().any(|o| *o != first) {
                    return Err(unsupported(
                        6,
                        "contextual rules for one glyph that substitute differently, or a rule \
                         that matches without substituting ahead of one that does (rule order \
                         would matter)",
                    ));
                }
                let out = first.expect("one is Some");
                for (side, ctxs, _) in hits {
                    for context in ctxs {
                        records.push(ContextSingle {
                            side,
                            context,
                            input: g,
                            out,
                        });
                    }
                }
            }
            records.sort_by_key(|r| (r.input, r.side, r.context));
            records.dedup();
            Ok((!records.is_empty()).then_some(GsubRules::ContextSingle(records)))
        }
        2 => Err(unsupported(2, "multiple substitution")),
        3 => Err(unsupported(3, "alternate substitution")),
        5 => Err(unsupported(5, "contextual substitution (type 5)")),
        8 => Err(unsupported(8, "reverse chaining substitution")),
        _ => Err(unsupported(kind, "unknown lookup type")),
    }
}

/// Every glyph a set of rules can output.
fn outputs(rules: &GsubRules) -> Vec<u16> {
    match rules {
        GsubRules::Single(r) => r.iter().map(|x| x.1).collect(),
        GsubRules::Ligature(r) => r.iter().map(|x| x.out).collect(),
        GsubRules::ContextSingle(r) => r.iter().map(|x| x.out).collect(),
    }
}

/// `ValueRecord` is empty in rustybuzz's sense: no value and no device.
fn value_is_empty(v: &ValueRecord) -> bool {
    v.x_placement == 0
        && v.y_placement == 0
        && v.x_advance == 0
        && v.y_advance == 0
        && v.x_placement_device.is_none()
        && v.y_placement_device.is_none()
        && v.x_advance_device.is_none()
        && v.y_advance_device.is_none()
}

/// The x-advance adjustment one pair lookup gives `(left, right)`, the way
/// rustybuzz's `PairAdjustment::apply` finds it: the first subtable that
/// covers `left` and has a value for the pair.
fn pair_value(subs: &[PairAdjustment], left: u16, right: u16) -> Result<Option<i16>, LayoutError> {
    for sub in subs {
        let Some(idx) = sub.coverage().get(GlyphId(left)) else {
            continue;
        };
        let records = match sub {
            PairAdjustment::Format1 { sets, .. } => {
                sets.get(idx).and_then(|s| s.get(GlyphId(right)))
            }
            PairAdjustment::Format2 {
                classes, matrix, ..
            } => matrix.get((classes.0.get(GlyphId(left)), classes.1.get(GlyphId(right)))),
        };
        let Some((v1, v2)) = records else {
            continue;
        };
        // Devices on the first value add nothing at the default instance with
        // no ppem; any other non-zero value, or a second value at all (which
        // also changes where the next pair starts), cannot be carried.
        if v1.x_placement != 0 || v1.y_placement != 0 || v1.y_advance != 0 || !value_is_empty(&v2) {
            return Err(LayoutError::UnsupportedPositioning { left, right });
        }
        return Ok(Some(v1.x_advance));
    }
    Ok(None)
}

#[cfg_attr(not(feature = "cpu-bake"), allow(dead_code))]
fn name_string(face: &ttf_parser::Face, id: u16) -> Option<String> {
    let names = face.names();
    names
        .into_iter()
        .filter(|n| n.name_id == id)
        .filter_map(|n| n.to_string())
        .next()
}

/// A face's copyright line (`name` ID 0), if any.
#[cfg_attr(not(feature = "cpu-bake"), allow(dead_code))]
pub(crate) fn copyright_of(face: &ttf_parser::Face) -> Option<String> {
    name_string(face, 0)
}

/// The Reserved Font Name from a copyright line: the text after "with
/// Reserved Font Name", without quotes or a final full stop.
#[cfg_attr(not(feature = "cpu-bake"), allow(dead_code))]
pub(crate) fn reserved_font_name(copyright: &str) -> String {
    const KEY: &str = "with Reserved Font Name";
    match copyright.find(KEY) {
        Some(i) => copyright[i + KEY.len()..]
            .trim()
            .trim_end_matches('.')
            .trim_matches(|c| c == '"' || c == '\'' || c == '“' || c == '”')
            .trim()
            .to_string(),
        None => String::new(),
    }
}

/// The SPDX identifier a face's `name` IDs 13 and 14 name, when they name
/// one this bake recognises.
#[cfg_attr(not(feature = "cpu-bake"), allow(dead_code))]
pub(crate) fn declared_licence(face: &ttf_parser::Face) -> Option<&'static str> {
    let text = [name_string(face, 14), name_string(face, 13)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    if text.contains("scripts.sil.org/OFL")
        || text.contains("openfontlicense.org")
        || text.contains("SIL Open Font License")
    {
        Some("OFL-1.1")
    } else if text.contains("apache.org/licenses/LICENSE-2.0") || text.contains("Apache License") {
        Some("Apache-2.0")
    } else {
        None
    }
}

impl FaceLayout {
    /// **Extract one face's tables and run the equivalence check on them**
    /// ([`check_equivalence`]): a face whose layout the tables cannot carry
    /// exactly is an error, never an approximate table.
    ///
    /// `style` is the style the face is baked as; `opts` adds optional
    /// features to this face only.
    pub fn extract(
        face_data: &[u8],
        style: GlyphStyle,
        opts: &LayoutOptions,
    ) -> Result<FaceLayout, LayoutError> {
        let layout = Self::extract_tables(face_data, style, opts)?;
        check_equivalence(face_data, &layout, opts)?;
        Ok(layout)
    }

    /// [`FaceLayout::extract`] without the equivalence check.
    pub(crate) fn extract_tables(
        face_data: &[u8],
        style: GlyphStyle,
        opts: &LayoutOptions,
    ) -> Result<FaceLayout, LayoutError> {
        let face =
            ttf_parser::Face::parse(face_data, 0).map_err(|e| LayoutError::Face(e.to_string()))?;
        let tables = face.tables();
        if tables.morx.is_some() || tables.kerx.is_some() || tables.trak.is_some() {
            return Err(LayoutError::Face(
                "the face carries AAT layout tables (morx, kerx or trak)".into(),
            ));
        }

        let mut features: Vec<[u8; 4]> = RUSTYBUZZ_DEFAULT_FEATURES.to_vec();
        features.extend_from_slice(TEXT_SHAPER_DEFAULT_FEATURES);
        for f in &opts.extra_features {
            if !features.contains(f) {
                features.push(*f);
            }
        }
        let coords = face.variation_coordinates();

        let (gsub_lookups, mut found) = match tables.gsub {
            Some(t) => selected_lookups(&t, &features, coords)?,
            None => (Vec::new(), Vec::new()),
        };
        let (gpos_lookups, found_gpos) = match tables.gpos {
            Some(t) => selected_lookups(&t, &features, coords)?,
            None => (Vec::new(), Vec::new()),
        };
        if tables.kern.is_some() && !found_gpos.contains(b"kern") {
            return Err(LayoutError::Face(
                "the face kerns with a legacy 'kern' table, which the tables do not carry".into(),
            ));
        }
        found.extend(found_gpos);
        found.sort_unstable();
        found.dedup();

        // Cmap over the scope.
        let mut cmap: Vec<(u32, u16)> = Vec::new();
        for ch in cmap_scope(&opts.coverage) {
            if let Some(g) = face.glyph_index(ch)
                && g.0 != 0
            {
                cmap.push((ch as u32, g.0));
            }
        }
        let hidden_glyph = face.glyph_index(' ').map_or(0, |g| g.0);

        // The closure: reachable glyphs and the rules over them, until no
        // new glyph appears.
        let mut reach: Vec<u16> = cmap.iter().map(|c| c.1).collect();
        reach.push(0);
        reach.sort_unstable();
        reach.dedup();
        let gsub_table = tables.gsub;
        let mut gsub: Vec<GsubLookup>;
        loop {
            gsub = Vec::new();
            let before = reach.len();
            if let Some(t) = &gsub_table {
                for &(index, feature) in &gsub_lookups {
                    if let Some(rules) = flatten_gsub(t, index, feature, &reach)? {
                        for g in outputs(&rules) {
                            if let Err(i) = reach.binary_search(&g) {
                                reach.insert(i, g);
                            }
                        }
                        gsub.push(GsubLookup { feature, rules });
                    }
                }
            }
            if reach.len() == before {
                break;
            }
        }
        if gsub.len() > crate::font::layout::MAX_GSUB_LOOKUPS {
            return Err(LayoutError::TooLarge("more than 65535 Gsub lookups"));
        }
        if let Some(&g) = reach.iter().find(|&&g| g > MAX_RAW_GLYPH_ID) {
            return Err(LayoutError::GlyphIdTooWide(g));
        }
        if let Some(gdef) = tables.gdef {
            for &g in &reach {
                if gdef.glyph_class(GlyphId(g)) == Some(ttf_parser::gdef::GlyphClass::Mark) {
                    return Err(LayoutError::MarkReachable { glyph: g });
                }
            }
        }

        // Gpos: pair kerning summed over the applied lookups.
        let mut kern: std::collections::BTreeMap<u32, i32> = Default::default();
        if let Some(t) = &tables.gpos {
            for &(index, feature) in &gpos_lookups {
                let Some(lookup) = t.lookups.get(index) else {
                    continue;
                };
                let subs: Vec<PositioningSubtable> = lookup.subtables.into_iter().collect();
                let Some(first) = subs.first() else {
                    continue;
                };
                let lookup_type = gpos_type_of(first);
                let live = subs
                    .iter()
                    .any(|s| reach.iter().any(|&g| s.coverage().contains(GlyphId(g))));
                if !live {
                    continue;
                }
                let unsupported = |why: &'static str| LayoutError::UnsupportedLookup {
                    feature,
                    lookup_index: index,
                    lookup_type,
                    why,
                };
                if lookup.flags.0 & !0x0008 != 0 || lookup.mark_filtering_set.is_some() {
                    return Err(unsupported(
                        "lookup flags other than IgnoreMarks (which has no effect without \
                         reachable marks)",
                    ));
                }
                let pairs: Vec<PairAdjustment> = subs
                    .iter()
                    .map(|s| match s {
                        PositioningSubtable::Pair(p) => Ok(*p),
                        _ => Err(unsupported(
                            "a reachable positioning lookup other than pair adjustment",
                        )),
                    })
                    .collect::<Result<_, _>>()?;
                for &left in &reach {
                    if !pairs.iter().any(|p| p.coverage().contains(GlyphId(left))) {
                        continue;
                    }
                    for &right in &reach {
                        if let Some(v) = pair_value(&pairs, left, right)? {
                            *kern
                                .entry(((left as u32) << 16) | right as u32)
                                .or_default() += v as i32;
                        }
                    }
                }
            }
        }
        let mut gpos = Vec::with_capacity(kern.len());
        for (key, v) in kern {
            if v != 0 {
                let v = i16::try_from(v)
                    .map_err(|_| LayoutError::TooLarge("a summed kerning value overflows i16"))?;
                gpos.push((key, v));
            }
        }

        // Hmtx over everything the shaper can output.
        let mut hmtx_glyphs = reach.clone();
        hmtx_glyphs.push(hidden_glyph);
        hmtx_glyphs.sort_unstable();
        hmtx_glyphs.dedup();
        let hmtx = hmtx_glyphs
            .iter()
            .map(|&g| (g, face.glyph_hor_advance(GlyphId(g)).unwrap_or(0)))
            .collect();

        let bbox_top = |ch: char| {
            face.glyph_index(ch)
                .and_then(|g| face.glyph_bounding_box(g))
                .map_or(0, |b| b.y_max)
        };
        let os2 = tables.os2;
        let cap_height = os2
            .and_then(|o| o.capital_height())
            .filter(|&v| v != 0)
            .unwrap_or_else(|| bbox_top('H'));
        let x_height = os2
            .and_then(|o| o.x_height())
            .filter(|&v| v != 0)
            .unwrap_or_else(|| bbox_top('x'));

        let layout = FaceLayout {
            style,
            units_per_em: face.units_per_em(),
            hidden_glyph,
            ascender: tables.hhea.ascender,
            descender: tables.hhea.descender,
            line_gap: tables.hhea.line_gap,
            cap_height,
            x_height,
            features: found,
            cmap,
            hmtx,
            gsub,
            gpos,
        };
        layout
            .validate()
            .map_err(|e| LayoutError::Face(e.to_string()))?;
        Ok(layout)
    }
}

/// The pangrams of the equivalence corpus.
pub const LAYOUT_CHECK_PANGRAMS: &[&str] = &[
    "The quick brown fox jumps over the lazy dog",
    "Sphinx of black quartz, judge my vow",
    "Pack my box with five dozen liquor jugs",
    "Waltz, bad nymph, for quick jigs vex",
    "Falsches \u{dc}ben von Xylophonmusik qu\u{e4}lt jeden gr\u{f6}\u{df}eren Zwerg",
    "Portez ce vieux whisky au juge blond qui fume",
];

/// The ledger-style strings of the equivalence corpus.
pub const LAYOUT_CHECK_LEDGER: &[&str] = &[
    "Treasury 50G",
    "Population 5",
    "fifty florins",
    "affluent office baffle",
    "flagon of fine fish",
    "To the Toll",
    "T T T",
    "Tax: +12G",
    "Total \u{2013} 1,250G",
    "\u{201c}Summa\u{201d} \u{2026} 100%",
    "\u{bd} \u{bc} \u{be} 1/2",
    "AVA Ty To Te",
    "`T` \u{b4}A\u{b4} \u{a8}E\u{a8} \u{af}O\u{af}",
    "line\nbreak\ttab\r",
    "soft\u{ad}hyphen of\u{ad}fice",
];

/// Strings with no letter at all, which rustybuzz shapes under the `DFLT`
/// script rather than `latn`.
pub const LAYOUT_CHECK_NO_LETTERS: &[&str] = &[
    "0123456789",
    "1/2 - 3.50, (42)!",
    "12:30 #7 $5 @3 %9 &1 *2",
    "+-=<>[]{}|\\~^_",
    "\u{a7}1 \u{b6}2 \u{b0}3 \u{b1}4 \u{d7}5 \u{f7}6 \u{a9}\u{ae}\u{2122}",
    "1\u{ad}2 . , ; : ' \"",
];

/// **The equivalence check** (plan section 4.9): shape the corpus with
/// [`crate::font::TextShaper`] (with the bake's features) and with an
/// [`crate::font::AtlasShaper`] over `layout`, and require every field of
/// every run to agree. Returns the corpus size.
///
/// The corpus: every `Cmap` codepoint alone and every ordered pair of them;
/// every `Gsub` rule's sequence (with its context) with every `Cmap`
/// codepoint before and after it, and with U+00AD between each two of its
/// characters; every `Gpos` pair with U+00AD between; the pangrams, the
/// ledger strings and strings with no letter at all.
pub fn check_equivalence(
    face_data: &[u8],
    layout: &FaceLayout,
    opts: &LayoutOptions,
) -> Result<usize, LayoutError> {
    let mut text = crate::font::TextShaper::new(face_data.to_vec())
        .map_err(|e| LayoutError::Face(e.to_string()))?;
    text.set_default_features(opts.text_shaper_features());
    let atlas = crate::font::AtlasShaper::from_face(layout.clone());
    let style = layout.style;

    let chars: Vec<char> = layout
        .cmap
        .iter()
        .filter_map(|&(cp, _)| char::from_u32(cp))
        .collect();
    // A codepoint for a glyph, where the Cmap names one.
    let char_of = |g: u16| -> Option<char> {
        layout
            .cmap
            .iter()
            .find(|c| c.1 == g)
            .and_then(|c| char::from_u32(c.0))
    };

    let mut count = 0usize;
    let mut check = |s: &str| -> Result<(), LayoutError> {
        count += 1;
        let mut want = text.shape(s);
        for g in &mut want.glyphs {
            g.glyph_id = style.styled_glyph_id(g.glyph_id).unwrap_or(0);
        }
        let got = atlas.shape(s);
        if got != want {
            return Err(LayoutError::Mismatch {
                text: s.to_string(),
                rustybuzz: want,
                atlas: got,
            });
        }
        Ok(())
    };

    let mut buf = String::new();
    for &a in &chars {
        buf.clear();
        buf.push(a);
        check(&buf)?;
        for &b in &chars {
            buf.clear();
            buf.push(a);
            buf.push(b);
            check(&buf)?;
        }
    }

    // Rule sequences, as text.
    let mut sequences: Vec<Vec<char>> = Vec::new();
    for lookup in &layout.gsub {
        match &lookup.rules {
            GsubRules::Single(r) => {
                for &(from, _) in r {
                    sequences.extend(char_of(from).map(|c| vec![c]));
                }
            }
            GsubRules::Ligature(r) => {
                for lig in r {
                    let seq: Option<Vec<char>> =
                        lig.components.iter().map(|&g| char_of(g)).collect();
                    sequences.extend(seq);
                }
            }
            GsubRules::ContextSingle(r) => {
                for rec in r {
                    let (Some(c), Some(i)) = (char_of(rec.context), char_of(rec.input)) else {
                        continue;
                    };
                    sequences.push(match rec.side {
                        ContextSide::Before => vec![c, i],
                        ContextSide::After => vec![i, c],
                    });
                }
            }
        }
    }
    sequences.sort();
    sequences.dedup();
    for seq in &sequences {
        let body: String = seq.iter().collect();
        for &x in &chars {
            check(&format!("{x}{body}"))?;
            check(&format!("{body}{x}"))?;
        }
        if seq.len() > 1 {
            let shy: String = seq
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join("\u{ad}");
            check(&shy)?;
        }
    }
    for &(key, _) in &layout.gpos {
        if let (Some(l), Some(r)) = (char_of((key >> 16) as u16), char_of(key as u16)) {
            check(&format!("{l}\u{ad}{r}"))?;
        }
    }
    // The fixed strings, except one with a character the face maps but the
    // declared coverage leaves out: that is outside AtlasShaper's contract
    // (it draws the placeholder at the .notdef advance).
    let in_contract = |s: &str| {
        s.chars().all(|c| {
            layout.cmap.binary_search_by_key(&(c as u32), |m| m.0).is_ok() || !text.covers(c)
        })
    };
    for s in LAYOUT_CHECK_PANGRAMS
        .iter()
        .chain(LAYOUT_CHECK_LEDGER)
        .chain(LAYOUT_CHECK_NO_LETTERS)
    {
        if in_contract(s) {
            check(s)?;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(face: &[u8]) -> FaceLayout {
        FaceLayout::extract_tables(face, GlyphStyle::Regular, &LayoutOptions::default())
            .expect("extracts")
    }

    fn gid(face: &[u8], ch: char) -> u16 {
        ttf_parser::Face::parse(face, 0)
            .unwrap()
            .glyph_index(ch)
            .unwrap()
            .0
    }

    /// Step 3's numbers, against fontTools' reading of the bundled faces
    /// (plan section 3 and the 13:02 EDT decision 4).
    #[test]
    fn the_bundled_roboto_faces_match_fonttools() {
        for face in [
            crate::font::ROBOTO_REGULAR_ASCII,
            crate::font::ROBOTO_BOLD_ASCII,
            crate::font::ROBOTO_ITALIC_ASCII,
        ] {
            let l = extract(face);
            assert_eq!(l.features, vec![*b"kern", *b"liga", *b"lnum", *b"pnum"]);
            assert_eq!(l.units_per_em, 2048);
            assert_eq!(
                (
                    l.ascender,
                    l.descender,
                    l.line_gap,
                    l.cap_height,
                    l.x_height
                ),
                (1900, -500, 0, 1456, 1082),
                "hhea and OS/2 (version 4) as fontTools reads them"
            );
            assert_eq!(l.hidden_glyph, gid(face, ' '));
            assert_eq!(
                l.cmap.len(),
                201,
                "every TEXT_RANGES codepoint, no controls"
            );

            let ligs: Vec<&Ligature> = l
                .gsub
                .iter()
                .filter_map(|lk| match &lk.rules {
                    GsubRules::Ligature(r) => Some(r),
                    _ => None,
                })
                .flatten()
                .collect();
            let (f, i, el) = (gid(face, 'f'), gid(face, 'i'), gid(face, 'l'));
            let mut comps: Vec<Vec<u16>> = ligs.iter().map(|x| x.components.clone()).collect();
            comps.sort();
            assert_eq!(
                comps,
                vec![vec![f, f, i], vec![f, f, el], vec![f, i], vec![f, el]]
            );
            let singles: Vec<(&[u8; 4], usize)> = l
                .gsub
                .iter()
                .filter_map(|lk| match &lk.rules {
                    GsubRules::Single(r) => Some((&lk.feature, r.len())),
                    _ => None,
                })
                .collect();
            assert_eq!(
                singles,
                vec![(b"pnum", 10), (b"lnum", 2)],
                "the digit substitutions"
            );
        }
        // Regular's kerning: 434 non-zero pairs, (space, T) and (T, space)
        // among them at -40; the figure forms add none.
        let face = crate::font::ROBOTO_REGULAR_ASCII;
        let l = extract(face);
        assert_eq!(l.gpos.len(), 434);
        let (sp, t) = (gid(face, ' ') as u32, gid(face, 'T') as u32);
        let kern = |a: u32, b: u32| {
            l.gpos
                .binary_search_by_key(&((a << 16) | b), |p| p.0)
                .ok()
                .map(|i| l.gpos[i].1)
        };
        assert_eq!(kern(sp, t), Some(-40));
        assert_eq!(kern(t, sp), Some(-40));
        assert_eq!(
            l.gpos
                .iter()
                .filter(|p| ((p.0 >> 16) as u16 == sp as u16) || (p.0 as u16 == sp as u16))
                .count(),
            2,
            "exactly two pairs involve the space"
        );
    }

    #[test]
    fn a_lookup_the_tables_cannot_carry_is_refused() {
        let face = include_bytes!("../../tests/fixtures/layout/roboto-multiple-subst.ttf");
        match FaceLayout::extract_tables(face, GlyphStyle::Regular, &LayoutOptions::default()) {
            Err(LayoutError::UnsupportedLookup {
                feature,
                lookup_index,
                lookup_type,
                ..
            }) => {
                assert_eq!((&feature, lookup_index, lookup_type), (b"liga", 0, 2));
            }
            other => panic!("expected UnsupportedLookup, got {other:?}"),
        }
    }

    /// The coverage parameter (bundled fonts plan, 13:40 EDT amendment): the
    /// default is TEXT_RANGES, and a narrower coverage narrows the Cmap
    /// while the check still passes.
    #[test]
    fn the_coverage_parameter_scopes_the_cmap() {
        let face = crate::font::ROBOTO_REGULAR_ASCII;
        let default = extract(face);
        let explicit = FaceLayout::extract_tables(
            face,
            GlyphStyle::Regular,
            &LayoutOptions {
                extra_features: Vec::new(),
                coverage: TEXT_RANGES.to_vec(),
            },
        )
        .unwrap();
        assert_eq!(default, explicit);
        let narrower: Vec<(char, char)> = TEXT_RANGES
            .iter()
            .copied()
            .filter(|r| *r != ('\u{2026}', '\u{2026}'))
            .collect();
        let opts = LayoutOptions {
            extra_features: Vec::new(),
            coverage: narrower,
        };
        let l = FaceLayout::extract(face, GlyphStyle::Regular, &opts).expect("still passes");
        assert!(!l.cmap.iter().any(|c| c.0 == 0x2026));
        assert_eq!(l.cmap.len(), default.cmap.len() - 1);
    }

    #[test]
    fn reserved_font_names_are_parsed_from_the_copyright_line() {
        assert_eq!(
            reserved_font_name(
                "Copyright 2010 The Foo Authors, with Reserved Font Name \"Foo Sans\"."
            ),
            "Foo Sans"
        );
        assert_eq!(reserved_font_name("Copyright 2011 Google Inc."), "");
    }
}

/// The equivalence check (atlas layout tables plan, step 5).
#[cfg(test)]
mod equivalence {
    use super::*;

    #[test]
    fn the_bundled_faces_pass() {
        for (face, style) in [
            (crate::font::ROBOTO_REGULAR_ASCII, GlyphStyle::Regular),
            (crate::font::ROBOTO_BOLD_ASCII, GlyphStyle::Bold),
            (crate::font::ROBOTO_ITALIC_ASCII, GlyphStyle::Italic),
            (crate::font::ROBOTO_ASCII_MSYMBOLS, GlyphStyle::Regular),
        ] {
            let opts = LayoutOptions::default();
            let layout = FaceLayout::extract_tables(face, style, &opts).expect("extracts");
            let n = check_equivalence(face, &layout, &opts)
                .unwrap_or_else(|e| panic!("{style:?}: {e}"));
            assert!(n > 40_401, "the corpus is at least every pair: {n}");
            eprintln!("{style:?}: {n} strings agree");
        }
    }

    #[test]
    fn a_corrupted_gpos_value_is_caught() {
        let face = crate::font::ROBOTO_REGULAR_ASCII;
        let opts = LayoutOptions::default();
        let mut layout = FaceLayout::extract_tables(face, GlyphStyle::Regular, &opts).unwrap();
        layout.gpos[0].1 += 1;
        match check_equivalence(face, &layout, &opts) {
            Err(e @ LayoutError::Mismatch { .. }) => {
                let LayoutError::Mismatch { text, .. } = &e else {
                    unreachable!()
                };
                assert!(e.to_string().contains(&format!("{text:?}")), "{e}");
            }
            other => panic!("expected a mismatch, got {other:?}"),
        }
    }

    #[test]
    fn reordered_gsub_lookups_are_caught() {
        // Two single substitutions whose order matters: a -> b, then b -> c.
        let face = include_bytes!("../../tests/fixtures/layout/roboto-ordered-singles.ttf");
        let opts = LayoutOptions::default();
        let mut layout = FaceLayout::extract_tables(face, GlyphStyle::Regular, &opts).unwrap();
        assert_eq!(layout.gsub.len(), 2);
        check_equivalence(face, &layout, &opts).expect("in font order, the shapers agree");
        layout.gsub.swap(0, 1);
        match check_equivalence(face, &layout, &opts) {
            Err(LayoutError::Mismatch { text, .. }) => assert!(text.contains('a'), "{text:?}"),
            other => panic!("expected a mismatch, got {other:?}"),
        }
    }
}
