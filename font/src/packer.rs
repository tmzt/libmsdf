//! Shelf-based bin packer for atlas layout (extracted from
//! matterstream-font's atlas internals and made public so the atlas
//! manager can drive dynamic appends).

/// Shelf-based bin packer. Rectangles are packed left-to-right into
/// horizontal shelves; a new shelf opens when the current shelves can't
/// fit the rectangle.
#[derive(Debug, Clone)]
pub struct ShelfPacker {
    width: u32,
    shelves: Vec<Shelf>,
}

#[derive(Debug, Clone)]
struct Shelf {
    y: u32,
    height: u32,
    x_cursor: u32,
}

impl ShelfPacker {
    pub fn new(width: u32) -> Self {
        Self {
            width,
            shelves: Vec::new(),
        }
    }

    /// Atlas width this packer packs into.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Pack a rectangle of (w, h). Returns (x, y) position. The packer
    /// grows downward without bound — use [`Self::pack_bounded`] when the
    /// target texture height is fixed.
    pub fn pack(&mut self, w: u32, h: u32) -> (u32, u32) {
        // Try existing shelves
        for shelf in &mut self.shelves {
            if shelf.height >= h && shelf.x_cursor + w <= self.width {
                let x = shelf.x_cursor;
                let y = shelf.y;
                shelf.x_cursor += w;
                return (x, y);
            }
        }

        // New shelf
        let y = self.shelves.last().map_or(0, |s| s.y + s.height);
        self.shelves.push(Shelf {
            y,
            height: h,
            x_cursor: w,
        });
        (0, y)
    }

    /// Pack a rectangle without exceeding `max_height`. Returns None when
    /// the rectangle doesn't fit (caller should evict or grow the atlas).
    pub fn pack_bounded(&mut self, w: u32, h: u32, max_height: u32) -> Option<(u32, u32)> {
        if w > self.width {
            return None;
        }
        for shelf in &mut self.shelves {
            if shelf.height >= h && shelf.x_cursor + w <= self.width {
                let x = shelf.x_cursor;
                let y = shelf.y;
                shelf.x_cursor += w;
                return Some((x, y));
            }
        }
        let y = self.shelves.last().map_or(0, |s| s.y + s.height);
        if y + h > max_height {
            return None;
        }
        self.shelves.push(Shelf {
            y,
            height: h,
            x_cursor: w,
        });
        Some((0, y))
    }

    /// Total height used so far.
    pub fn used_height(&self) -> u32 {
        self.shelves.last().map_or(0, |s| s.y + s.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_left_to_right_then_new_shelf() {
        let mut p = ShelfPacker::new(100);
        assert_eq!(p.pack(40, 20), (0, 0));
        assert_eq!(p.pack(40, 20), (40, 0));
        // Doesn't fit on shelf 0 (would end at 120) → new shelf.
        assert_eq!(p.pack(40, 20), (0, 20));
        assert_eq!(p.used_height(), 40);
    }

    #[test]
    fn bounded_pack_rejects_overflow() {
        let mut p = ShelfPacker::new(64);
        assert!(p.pack_bounded(64, 32, 64).is_some());
        assert!(p.pack_bounded(64, 32, 64).is_some());
        assert!(
            p.pack_bounded(64, 32, 64).is_none(),
            "third shelf exceeds max height"
        );
        assert!(p.pack_bounded(128, 8, 64).is_none(), "wider than atlas");
    }
}
