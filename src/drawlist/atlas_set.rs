//! Several atlases in one draw list (docs/design/20261021_03_ATLAS_SET_PLAN.md).
//!
//! An [`AtlasSet`] holds atlases under small ids and lays them out as ONE
//! texture array (each atlas's layers after the previous atlas's) and ONE
//! glyph table (each atlas's entries after the previous atlas's). A list
//! draws from an atlas in the set through its handle, an [`AtlasRef`]
//! ([`DrawList::push_shaped_text_with`](super::DrawList::push_shaped_text_with)),
//! and is lowered against the set
//! ([`DrawList::lower_with`](super::DrawList::lower_with)), which moves each
//! text instance's glyph indices to its atlas's place in the merged table.
//! The shader does not change: a glyph-table entry already names its layer.
//!
//! # Sizes
//!
//! Every layer of a texture array has one size, so the set's layers are the
//! widest and tallest atlas's, and a smaller atlas sits at its layers' top
//! left. That is safe because the shader reads a cell's position in texels
//! from the table and divides by the texture's size, and checks the cell's
//! bounds before it samples, so the margin is never read. The baker makes
//! atlases in standard sizes (400 x 2000, or 800 x 2000 for 16 columns), so
//! a set pads only when it mixes them. The bundled terminal symbol page is
//! the exception: its columns follow its cells, 18 for 0.6.2's 694, so it is
//! 900 x 2000 (`docs/design/20261028_01_CODING_AGENT_GLYPHS_PLAN.md`, the
//! 21:05 EDT amendment), and a set of the terminal atlases pads the text
//! atlas's layers to 900.

use std::ops::Deref;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::font::atlas::FontAtlas;
use crate::font::{AtlasShaper, GlyphStyle, MAX_ATLAS_LAYERS, NoLayout};

/// The most entries a merged glyph table may hold: a draw list packs a glyph
/// as a 16-bit index into it.
pub const MAX_SET_GLYPHS: u32 = 1 << 16;

/// Each set's number, handed out when it is made. 0 is "no set".
static NEXT_SET: AtomicU32 = AtomicU32::new(1);

/// An atlas's place in an [`AtlasSet`]: which set, and which atlas in it.
/// `AtlasId::default()` (`set: 0, index: 0`) is what instances pushed with a
/// plain `&FontAtlas` record, and what lowering against a set reads as the
/// set's first atlas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct AtlasId {
    /// The set's number ([`AtlasSet::number`]); 0 is no set.
    pub set: u32,
    /// The atlas's place in the set, in registration order.
    pub index: u16,
}

/// A handle to an atlas in a set: its id and the atlas. It derefs to
/// [`FontAtlas`], so metrics read through it as through the atlas.
#[derive(Clone, Copy, Debug)]
pub struct AtlasRef<'a> {
    pub id: AtlasId,
    pub atlas: &'a FontAtlas,
}

impl Deref for AtlasRef<'_> {
    type Target = FontAtlas;

    fn deref(&self) -> &FontAtlas {
        self.atlas
    }
}

impl AtlasRef<'_> {
    /// `AtlasShaper::new(self.atlas, style)`: a shaper over this atlas's
    /// layout tables, for runs to push with
    /// [`DrawList::push_shaped_text_with`](super::DrawList::push_shaped_text_with).
    /// The shaper owns its tables, so it outlives the handle.
    pub fn shaper(&self, style: GlyphStyle) -> Result<AtlasShaper, NoLayout> {
        AtlasShaper::new(self.atlas, style)
    }
}

/// Why [`AtlasSet::register`] refused an atlas.
#[derive(Debug, PartialEq, Eq)]
pub enum AtlasSetError {
    /// The set would hold more layers than [`MAX_ATLAS_LAYERS`].
    TooManyLayers { have: u32, adding: u32 },
    /// The merged glyph table would pass [`MAX_SET_GLYPHS`] entries.
    TooManyGlyphs { have: u32, adding: u32 },
    /// The set already holds 65,536 atlases.
    Full,
}

impl std::fmt::Display for AtlasSetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AtlasSetError::TooManyLayers { have, adding } => write!(
                f,
                "the set has {have} layers and this atlas adds {adding}: past {MAX_ATLAS_LAYERS}"
            ),
            AtlasSetError::TooManyGlyphs { have, adding } => write!(
                f,
                "the set has {have} glyphs and this atlas adds {adding}: past {MAX_SET_GLYPHS}"
            ),
            AtlasSetError::Full => write!(f, "the set holds 65,536 atlases already"),
        }
    }
}

impl std::error::Error for AtlasSetError {}

/// Several atlases laid out as one texture array and one glyph table.
#[derive(Debug)]
pub struct AtlasSet {
    number: u32,
    atlases: Vec<FontAtlas>,
    /// Per atlas: its first entry in the merged glyph table.
    table_base: Vec<u32>,
    /// Per atlas: its first layer in the merged texture array.
    layer_base: Vec<u32>,
}

impl Default for AtlasSet {
    fn default() -> Self {
        Self::new()
    }
}

impl AtlasSet {
    /// An empty set, with a number of its own.
    pub fn new() -> Self {
        Self {
            number: NEXT_SET.fetch_add(1, Ordering::Relaxed),
            atlases: Vec::new(),
            table_base: Vec::new(),
            layer_base: Vec::new(),
        }
    }

    /// This set's number, as its ids carry it.
    pub fn number(&self) -> u32 {
        self.number
    }

    /// Takes `atlas` into the set and returns its id.
    pub fn register(&mut self, atlas: FontAtlas) -> Result<AtlasId, AtlasSetError> {
        let index = u16::try_from(self.atlases.len()).map_err(|_| AtlasSetError::Full)?;
        let (have_layers, adding_layers) = (self.layers(), atlas.layer_count());
        if have_layers + adding_layers > MAX_ATLAS_LAYERS as u32 {
            return Err(AtlasSetError::TooManyLayers {
                have: have_layers,
                adding: adding_layers,
            });
        }
        let (have_glyphs, adding_glyphs) = (self.glyphs(), atlas.glyphs.len() as u32);
        if have_glyphs + adding_glyphs > MAX_SET_GLYPHS {
            return Err(AtlasSetError::TooManyGlyphs {
                have: have_glyphs,
                adding: adding_glyphs,
            });
        }
        self.table_base.push(have_glyphs);
        self.layer_base.push(have_layers);
        self.atlases.push(atlas);
        Ok(AtlasId {
            set: self.number,
            index,
        })
    }

    /// The handle for `id`: `None` for an id of another set, or one this set
    /// did not hand out.
    pub fn atlas(&self, id: AtlasId) -> Option<AtlasRef<'_>> {
        let atlas = self.atlases.get(self.slot(id)?)?;
        Some(AtlasRef { id, atlas })
    }

    /// Where `id` sits in this set, if it is this set's. The default id is
    /// the first atlas.
    pub(crate) fn slot(&self, id: AtlasId) -> Option<usize> {
        let ours = id.set == self.number || id == AtlasId::default();
        let index = id.index as usize;
        (ours && index < self.atlases.len()).then_some(index)
    }

    pub fn len(&self) -> usize {
        self.atlases.len()
    }

    pub fn is_empty(&self) -> bool {
        self.atlases.is_empty()
    }

    /// The texture array's layer size: the widest and tallest atlas.
    pub fn extent(&self) -> [u32; 2] {
        self.atlases
            .iter()
            .fold([0, 0], |[w, h], a| [w.max(a.width), h.max(a.height)])
    }

    /// Every atlas's layers, added up.
    pub fn layers(&self) -> u32 {
        self.atlases.iter().map(FontAtlas::layer_count).sum()
    }

    /// Every atlas's glyph-table entries, added up.
    pub fn glyphs(&self) -> u32 {
        self.atlases.iter().map(|a| a.glyphs.len() as u32).sum()
    }

    /// Each atlas with its first layer in the merged texture array, in
    /// registration order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (&FontAtlas, u32)> {
        self.atlases.iter().zip(self.layer_base.iter().copied())
    }

    /// Every atlas's first glyph-table entry, indexed by `AtlasId::index`.
    pub fn table_bases(&self) -> &[u32] {
        &self.table_base
    }

    /// Every atlas's first layer, indexed by `AtlasId::index`.
    pub fn layer_bases(&self) -> &[u32] {
        &self.layer_base
    }

    /// The merged glyph table, 8 u32s per entry as
    /// [`FontAtlas::glyph_table_u32s`], each entry's layer (`g1.w`, the
    /// eighth u32) moved up by its atlas's first layer.
    pub fn glyph_table_u32s(&self) -> Vec<u32> {
        let mut out = Vec::with_capacity(self.glyphs() as usize * 8);
        for (atlas, &base) in self.atlases.iter().zip(&self.layer_base) {
            let start = out.len();
            out.extend(atlas.glyph_table_u32s());
            for entry in out[start..].chunks_exact_mut(8) {
                entry[7] += base;
            }
        }
        out
    }

    /// The merged texels as RGBA8, layer after layer, each `extent()` sized,
    /// with each atlas's layers at their top left and zeros elsewhere.
    pub fn to_rgba_bytes(&self) -> Vec<u8> {
        let [w, h] = self.extent();
        let (w, h) = (w as usize, h as usize);
        let mut out = vec![0u8; w * h * 4 * self.layers() as usize];
        let mut layer = 0usize;
        for atlas in &self.atlases {
            let rgba = atlas.to_rgba_bytes();
            let (aw, ah) = (atlas.width as usize, atlas.height as usize);
            for l in 0..atlas.layer_count() as usize {
                for row in 0..ah {
                    let from = (l * ah + row) * aw * 4;
                    // An atlas with fewer texels than its layers declare (an
                    // empty or partial one) leaves the rest zero.
                    if from + aw * 4 > rgba.len() {
                        break;
                    }
                    let to = (layer * h + row) * w * 4;
                    out[to..to + aw * 4].copy_from_slice(&rgba[from..from + aw * 4]);
                }
                layer += 1;
            }
        }
        out
    }
}
