//! **The baker**: one public entry point that turns font files into atlas
//! bytes, used by libmsdf's build script, by a consumer's build script and
//! by the `libmsdf-bake` CLI (bundled fonts plan, section 4.5, with the
//! 13:34 and 13:40 EDT amendments).
//!
//! Pure Rust: no subprocess and no network. It wraps [`FontAtlasBuilder`]
//! in the order `examples/bake_atlas.rs` uses, so every caller gets the same
//! bake.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::atlas::{BuildError, FontAtlasBuilder};
use crate::bundled::{BUNDLED_ATLAS_GLYPH_SIZE, BUNDLED_ATLAS_PX_RANGE, BundledAtlas, hex_of};
use crate::coverage::{
    COVERAGE_MATH, COVERAGE_SYMBOLS, COVERAGE_TEXT, Coverage, CoverageGroupId, FaceCoverage,
    GroupCoverage, MissingPolicy,
};
use crate::{ATLAS_COLS, GlyphSet, GlyphStyle, LayoutError, LayoutOptions};

/// One face to bake, and how. Every field is an input to the bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct BakeOptions {
    /// Cell size in texels (48 for the bundled atlases).
    pub glyph_size: u32,
    /// Distance range in texels (6.0 for the bundled atlases).
    pub px_range: f64,
    /// Additional cuts baked as style layers: (style, face file).
    pub styles: Vec<(GlyphStyle, PathBuf)>,
    /// One per licence; at least one covering every baked style.
    pub licences: Vec<LicenceSpec>,
    /// Layout tables and the per-face optional features; `None` bakes no
    /// layout tables. With [`Coverage::Groups`] each face's
    /// [`LayoutOptions::coverage`] is replaced by the groups' ranges.
    pub layout: Option<Vec<(GlyphStyle, LayoutOptions)>>,
    /// Face files that must have these sha256s, checked before anything is
    /// baked.
    pub expect_sha256: Vec<(PathBuf, [u8; 32])>,
    /// What the atlas covers.
    pub coverage: Coverage,
    /// Cell columns per layer (`ATLAS_COLS`, 8, unless more are needed).
    pub columns: u32,
    /// TEXT codepoints a face lacks, each waived with its reason.
    pub waive: Vec<(GlyphStyle, char, &'static str)>,
}

impl BakeOptions {
    /// Options with no styles, licences, layout, hashes or waivers, shipped
    /// coverage and [`ATLAS_COLS`] columns.
    pub fn new(glyph_size: u32, px_range: f64) -> Self {
        Self {
            glyph_size,
            px_range,
            styles: Vec::new(),
            licences: Vec::new(),
            layout: None,
            expect_sha256: Vec::new(),
            coverage: Coverage::Shipped,
            columns: ATLAS_COLS,
            waive: Vec::new(),
        }
    }
}

/// One licence record to bake (`FontAtlasBuilder::add_licence`).
#[derive(Debug, Clone, PartialEq)]
pub struct LicenceSpec {
    /// The SPDX identifier, from `LICENCE_REGISTRY`.
    pub spdx: String,
    /// What is licensed, e.g. `Roboto`.
    pub source: String,
    /// The styles it covers.
    pub styles: Vec<GlyphStyle>,
}

/// Why a bake failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum BakeError {
    /// A face file could not be read.
    Io {
        /// The file.
        path: PathBuf,
        /// What went wrong.
        error: std::io::Error,
    },
    /// A face file is not the recorded bytes.
    FaceHash {
        /// The file.
        path: PathBuf,
        /// The recorded sha256.
        expected: [u8; 32],
        /// The file's sha256.
        found: [u8; 32],
    },
    /// The layout extraction or its equivalence check refused a face.
    Layout(GlyphStyle, LayoutError),
    /// A licence was refused.
    Licence(String),
    /// TEXT codepoints the face lacks, not waived.
    MissingText {
        /// The face's style.
        style: GlyphStyle,
        /// Every missing TEXT codepoint.
        codepoints: Vec<char>,
    },
    /// Any other refusal of `FontAtlasBuilder::build`.
    Build(String),
}

impl core::fmt::Display for BakeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BakeError::Io { path, error } => write!(f, "{}: {error}", path.display()),
            BakeError::FaceHash {
                path,
                expected,
                found,
            } => write!(
                f,
                "{}: sha256 {}, expected {} - not the recorded face",
                path.display(),
                hex_of(found),
                hex_of(expected)
            ),
            BakeError::Layout(style, e) => write!(f, "{style:?} layout: {e}"),
            BakeError::Licence(e) => write!(f, "licence: {e}"),
            BakeError::MissingText { style, codepoints } => write!(
                f,
                "the {style:?} face lacks TEXT codepoints (waive them per face, with a reason, \
                 or use another face): {}",
                codepoints
                    .iter()
                    .map(|c| format!("U+{:04X}", *c as u32))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            BakeError::Build(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for BakeError {}

/// What a bake covered, per face and group.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BakeReport {
    /// One per face with coverage groups.
    pub faces: Vec<FaceReport>,
}

/// One face of a [`BakeReport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaceReport {
    /// The face's style.
    pub style: GlyphStyle,
    /// Its groups.
    pub groups: Vec<GroupReport>,
}

/// One group of a [`FaceReport`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupReport {
    /// Which group.
    pub id: CoverageGroupId,
    /// Codepoints the group declares.
    pub declared: usize,
    /// Codepoints the face maps and the atlas carries.
    pub carried: usize,
    /// Declared codepoints the face lacks, skipped.
    pub skipped: Vec<char>,
    /// Declared TEXT codepoints the face lacks, waived, with the reason.
    pub waived: Vec<(char, &'static str)>,
}

impl core::fmt::Display for BakeReport {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let cps = |v: &mut dyn Iterator<Item = char>| {
            v.map(|c| format!("U+{:04X}", c as u32))
                .collect::<Vec<_>>()
                .join(" ")
        };
        if self.faces.is_empty() {
            return writeln!(f, "shipped coverage (no coverage groups)");
        }
        for face in &self.faces {
            for g in &face.groups {
                writeln!(
                    f,
                    "{:?} {:?}: {} of {} carried, {} skipped, {} waived",
                    face.style,
                    g.id,
                    g.carried,
                    g.declared,
                    g.skipped.len(),
                    g.waived.len()
                )?;
                if !g.skipped.is_empty() {
                    writeln!(f, "  skipped: {}", cps(&mut g.skipped.iter().copied()))?;
                }
                for (c, why) in &g.waived {
                    writeln!(f, "  waived: U+{:04X} ({why})", *c as u32)?;
                }
            }
        }
        Ok(())
    }
}

/// The sha256 of `bytes`.
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn read(path: &Path) -> Result<Vec<u8>, BakeError> {
    std::fs::read(path).map_err(|error| BakeError::Io {
        path: path.to_path_buf(),
        error,
    })
}

/// The baker. Every method is a static entry point.
#[derive(Debug, Clone, Copy)]
pub struct Baker;

impl Baker {
    /// Bake `ttf` (the Regular face) with `opts` and return the atlas file
    /// bytes (format 5).
    pub fn bake_face(ttf: &Path, opts: &BakeOptions) -> Result<Vec<u8>, BakeError> {
        Self::bake_face_report(ttf, opts).map(|(bytes, _)| bytes)
    }

    /// [`Baker::bake_face`], with the coverage report.
    pub fn bake_face_report(
        ttf: &Path,
        opts: &BakeOptions,
    ) -> Result<(Vec<u8>, BakeReport), BakeError> {
        for (path, expected) in &opts.expect_sha256 {
            let found = sha256(&read(path)?);
            if &found != expected {
                return Err(BakeError::FaceHash {
                    path: path.clone(),
                    expected: *expected,
                    found,
                });
            }
        }
        let regular = read(ttf)?;
        let mut faces: Vec<(GlyphStyle, Vec<u8>)> = vec![(GlyphStyle::Regular, regular.clone())];
        for (style, path) in &opts.styles {
            faces.push((*style, read(path)?));
        }

        let mut builder = FontAtlasBuilder::new(regular, opts.glyph_size, opts.px_range);
        builder.with_columns(opts.columns);
        let mut report = BakeReport::default();
        let mut coverage: Vec<FaceCoverage> = Vec::new();
        match &opts.coverage {
            Coverage::Shipped => {
                builder.add_shipped_coverage();
                for (style, data) in &faces[1..] {
                    builder
                        .add_styled_coverage(*style, data.clone())
                        .map_err(BakeError::Build)?;
                }
            }
            Coverage::Groups(groups) => {
                let mut groups = groups.clone();
                groups.sort_by_key(|g| g.id);
                for (style, data) in &faces {
                    if *style != GlyphStyle::Regular {
                        builder
                            .add_styled_face(*style, data.clone())
                            .map_err(BakeError::Build)?;
                    }
                    let face = ttf_parser::Face::parse(data, 0)
                        .map_err(|e| BakeError::Build(format!("{style:?} face: {e}")))?;
                    let mut face_cov = FaceCoverage {
                        style: *style,
                        groups: Vec::new(),
                    };
                    let mut face_report = FaceReport {
                        style: *style,
                        groups: Vec::new(),
                    };
                    let mut missing_text = Vec::new();
                    for g in &groups {
                        let declared = g.codepoints();
                        let (mut carried, mut skipped, mut waived) = (0, Vec::new(), Vec::new());
                        for &ch in &declared {
                            let mapped = face.glyph_index(ch).is_some_and(|g| g.0 != 0);
                            if mapped {
                                carried += 1;
                                builder.add_codepoint_range(style.text_set(), ch, ch);
                                continue;
                            }
                            // TEXT's rule wins for a codepoint in TEXT and
                            // another group.
                            let in_text = groups.iter().any(|t| {
                                t.missing == MissingPolicy::Fail && t.codepoints().contains(&ch)
                            });
                            if g.missing == MissingPolicy::Fail || in_text {
                                match opts.waive.iter().find(|w| w.0 == *style && w.1 == ch) {
                                    Some(w) => waived.push((ch, w.2)),
                                    None if g.missing == MissingPolicy::Fail => {
                                        missing_text.push(ch)
                                    }
                                    None => {}
                                }
                            } else {
                                skipped.push(ch);
                            }
                        }
                        face_cov.groups.push(GroupCoverage {
                            id: g.id as u8,
                            policy: g.missing,
                            ranges: g.ranges.to_vec(),
                            skipped: skipped.clone(),
                            waived: waived.iter().map(|w| w.0).collect(),
                        });
                        face_report.groups.push(GroupReport {
                            id: g.id,
                            declared: declared.len(),
                            carried,
                            skipped,
                            waived,
                        });
                    }
                    if !missing_text.is_empty() {
                        missing_text.sort_unstable();
                        missing_text.dedup();
                        return Err(BakeError::MissingText {
                            style: *style,
                            codepoints: missing_text,
                        });
                    }
                    builder.add_shaped_ascii_of(*style);
                    coverage.push(face_cov);
                    report.faces.push(face_report);
                }
                builder.add_glyph(GlyphSet::Placeholder, 0);
            }
        }
        for l in &opts.licences {
            builder
                .add_licence(&l.styles, &l.spdx, &l.source)
                .map_err(BakeError::Licence)?;
        }
        if let Some(layouts) = &opts.layout {
            for (style, lo) in layouts {
                let mut lo = lo.clone();
                if let Coverage::Groups(_) = &opts.coverage {
                    lo.coverage = opts.coverage.ranges();
                }
                builder.with_style_layout(*style, lo);
            }
        }
        let mut atlas = builder.build_checked().map_err(|e| match e {
            BuildError::Layout(style, e) => BakeError::Layout(style, e),
            BuildError::Other(e) if e.contains("licence") => BakeError::Licence(e),
            BuildError::Other(e) => BakeError::Build(e),
        })?;
        atlas.coverage = coverage;
        let bytes = atlas
            .try_to_bytes()
            .map_err(|e| BakeError::Build(e.to_string()))?;
        Ok((bytes, report))
    }

    /// The fixed options of one bundled atlas, with its faces under
    /// `fonts_dir` (libmsdf's `fonts/`).
    pub fn bundled_options(atlas: BundledAtlas, fonts_dir: &Path) -> (PathBuf, BakeOptions) {
        let faces = atlas.faces();
        let mut opts = BakeOptions::new(BUNDLED_ATLAS_GLYPH_SIZE, BUNDLED_ATLAS_PX_RANGE as f64);
        opts.styles = faces[1..]
            .iter()
            .map(|f| (f.style, fonts_dir.join(f.file)))
            .collect();
        opts.licences = atlas
            .licences()
            .iter()
            .map(|(spdx, source, styles)| LicenceSpec {
                spdx: spdx.to_string(),
                source: source.to_string(),
                styles: styles.to_vec(),
            })
            .collect();
        opts.layout = Some(
            faces
                .iter()
                .map(|f| (f.style, LayoutOptions::default()))
                .collect(),
        );
        opts.expect_sha256 = faces
            .iter()
            .map(|f| (fonts_dir.join(f.file), f.sha256))
            .collect();
        if atlas.is_ofl() {
            opts.coverage = Coverage::Groups(vec![COVERAGE_TEXT, COVERAGE_SYMBOLS, COVERAGE_MATH]);
            opts.columns = 16;
        }
        opts.waive = atlas.waivers().to_vec();
        (fonts_dir.join(faces[0].file), opts)
    }

    /// Bake one bundled atlas from the face files under `fonts_dir`, with
    /// its fixed options and recorded hashes.
    pub fn bake_bundled(atlas: BundledAtlas, fonts_dir: &Path) -> Result<Vec<u8>, BakeError> {
        Self::bake_bundled_report(atlas, fonts_dir).map(|(bytes, _)| bytes)
    }

    /// [`Baker::bake_bundled`], with the coverage report.
    pub fn bake_bundled_report(
        atlas: BundledAtlas,
        fonts_dir: &Path,
    ) -> Result<(Vec<u8>, BakeReport), BakeError> {
        let (ttf, opts) = Self::bundled_options(atlas, fonts_dir);
        Self::bake_face_report(&ttf, &opts)
    }
}
