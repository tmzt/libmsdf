//! Dynamic atlas management: shelf packing with append + region eviction.
//!
//! `AtlasManager` owns the CPU-side bookkeeping for a fixed-size atlas
//! texture: which glyph occupies which cell, a free list of evicted cells,
//! and eviction hooks so GPU-side callers can invalidate cached glyph-table
//! entries. Pixel uploads are the caller's job (either
//! `GpuSdfRenderer::upload_msdf_atlas_region` with CPU-baked bytes or
//! `gpu::MsdfCompute` for runtime generation).

use std::collections::HashMap;

use crate::font::packer::ShelfPacker;

/// A rectangular region inside the atlas texture (pixels).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtlasRegion {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Called when a glyph's region is evicted: `(glyph_id, region)`.
pub type EvictHook = Box<dyn FnMut(u16, AtlasRegion)>;

/// CPU-side allocator for a fixed-size dynamic glyph atlas.
pub struct AtlasManager {
    width: u32,
    height: u32,
    packer: ShelfPacker,
    /// Evicted cells available for reuse (same-size-first fit).
    free: Vec<AtlasRegion>,
    /// glyph_id → allocated region.
    allocated: HashMap<u16, AtlasRegion>,
    hooks: Vec<EvictHook>,
}

impl AtlasManager {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            packer: ShelfPacker::new(width),
            free: Vec::new(),
            allocated: HashMap::new(),
            hooks: Vec::new(),
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Region currently assigned to a glyph, if any.
    pub fn region_of(&self, glyph_id: u16) -> Option<AtlasRegion> {
        self.allocated.get(&glyph_id).copied()
    }

    /// Number of live allocations.
    pub fn len(&self) -> usize {
        self.allocated.len()
    }

    pub fn is_empty(&self) -> bool {
        self.allocated.is_empty()
    }

    /// Register an eviction hook (e.g. drop the glyph's table entry and
    /// re-upload). Hooks run in registration order on every eviction.
    pub fn on_evict(&mut self, hook: EvictHook) {
        self.hooks.push(hook);
    }

    /// Allocate a `w × h` cell for `glyph_id`.
    ///
    /// Reuses an evicted free cell when one is big enough, otherwise packs a
    /// new cell. Returns None when the atlas is full — the caller decides
    /// which glyphs to [`Self::evict`] and retries. Allocating an
    /// already-present glyph returns its existing region.
    pub fn alloc(&mut self, glyph_id: u16, w: u32, h: u32) -> Option<AtlasRegion> {
        if let Some(existing) = self.allocated.get(&glyph_id) {
            return Some(*existing);
        }

        // Reuse the best-fitting free cell (smallest area that fits).
        let mut best: Option<usize> = None;
        for (i, r) in self.free.iter().enumerate() {
            if r.w >= w && r.h >= h {
                let better = match best {
                    None => true,
                    Some(b) => (r.w * r.h) < (self.free[b].w * self.free[b].h),
                };
                if better {
                    best = Some(i);
                }
            }
        }
        if let Some(i) = best {
            let cell = self.free.swap_remove(i);
            let region = AtlasRegion { x: cell.x, y: cell.y, w, h };
            self.allocated.insert(glyph_id, region);
            return Some(region);
        }

        let (x, y) = self.packer.pack_bounded(w, h, self.height)?;
        let region = AtlasRegion { x, y, w, h };
        self.allocated.insert(glyph_id, region);
        Some(region)
    }

    /// Evict a glyph: its cell moves to the free list for reuse and all
    /// eviction hooks fire. Returns the freed region.
    pub fn evict(&mut self, glyph_id: u16) -> Option<AtlasRegion> {
        let region = self.allocated.remove(&glyph_id)?;
        self.free.push(region);
        for hook in &mut self.hooks {
            hook(glyph_id, region);
        }
        Some(region)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn alloc_evict_reuse_cycle() {
        let mut m = AtlasManager::new(128, 64);
        let a = m.alloc(1, 50, 50).expect("first cell fits");
        let b = m.alloc(2, 50, 50).expect("second cell fits");
        assert_ne!(a, b);
        // Third 50×50 cell: width exhausted (100/128 used, next shelf would
        // exceed height 64 after shelf 0 is 50 tall? shelf 0 holds both) —
        // a third fits on shelf 0? 150 > 128 → new shelf at y=50, 50+50 > 64 → full.
        assert!(m.alloc(3, 50, 50).is_none(), "atlas should be full");

        // Evict #1 → its cell is reused for #3.
        let freed = m.evict(1).expect("evict live glyph");
        assert_eq!(freed, a);
        let c = m.alloc(3, 50, 50).expect("reuses freed cell");
        assert_eq!((c.x, c.y), (a.x, a.y));
    }

    #[test]
    fn same_glyph_alloc_is_idempotent() {
        let mut m = AtlasManager::new(64, 64);
        let a = m.alloc(9, 32, 32).unwrap();
        let b = m.alloc(9, 32, 32).unwrap();
        assert_eq!(a, b);
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn evict_hooks_fire() {
        let mut m = AtlasManager::new(64, 64);
        let log: Rc<RefCell<Vec<(u16, AtlasRegion)>>> = Rc::new(RefCell::new(Vec::new()));
        let log2 = log.clone();
        m.on_evict(Box::new(move |gid, region| {
            log2.borrow_mut().push((gid, region));
        }));

        let r = m.alloc(42, 32, 32).unwrap();
        m.evict(42);
        assert_eq!(log.borrow().as_slice(), &[(42, r)]);
        assert!(m.evict(42).is_none(), "double evict is a no-op");
    }
}
