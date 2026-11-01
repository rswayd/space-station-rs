//! Shelf packer for atlas pages, filled incrementally one `.dmi` at a time.
//!
//! Every frame of a `.dmi` has the same size and the manifest stores one atlas per state, so a
//! file is placed as a group of equal rects on a single page. Each page keeps one open shelf per
//! rect height: with few distinct sizes in the source repo (most are 32x32), shelves fill almost
//! completely regardless of file order. A page can be opened larger than `max` for a group that
//! does not fit a regular page ([`AtlasPacker::new_page_sized`]).
//! shortcut: no padding between rects; add a gutter here if the renderer ever samples linearly.

use std::collections::HashMap;

#[derive(Debug)]
pub struct AtlasPacker {
    max: u32,
    /// Side of the current page: `max` unless opened with [`Self::new_page_sized`].
    size: u32,
    /// Used `(width, height)` of each page.
    pages: Vec<(u32, u32)>,
    /// Open shelf per height on the current page: `height -> (fill x, top y)`.
    shelves: HashMap<u32, (u32, u32)>,
    /// Top of the free area below all shelves of the current page.
    bottom: u32,
}

impl AtlasPacker {
    /// Pages are at most `max x max`.
    pub fn new(max: u32) -> Self {
        Self {
            max,
            size: max,
            pages: Vec::new(),
            shelves: HashMap::new(),
            bottom: 0,
        }
    }

    /// Used `(width, height)` of each page opened so far.
    pub fn pages(&self) -> &[(u32, u32)] {
        &self.pages
    }

    /// Index of the page `place` writes to, if any is open.
    pub fn current_page(&self) -> Option<u32> {
        self.pages.len().checked_sub(1).map(|p| p as u32)
    }

    /// How many `w x h` rects an empty regular page holds.
    pub fn page_capacity(&self, w: u32, h: u32) -> u64 {
        capacity(self.max, w, h)
    }

    /// Side of the current page.
    pub fn page_size(&self) -> u32 {
        self.size
    }

    /// Whether `n` more rects of `w x h` fit on the current page.
    pub fn fits(&self, w: u32, h: u32, n: usize) -> bool {
        let max = self.size;
        if self.pages.is_empty() || w > max || h > max {
            return n == 0;
        }
        let in_shelf = self.shelves.get(&h).map_or(0, |&(x, _)| (max - x) / w);
        let new_rows = (max - self.bottom) / h;
        u64::from(in_shelf) + u64::from(new_rows) * u64::from(max / w) >= n as u64
    }

    pub fn new_page(&mut self) {
        self.new_page_sized(self.max);
    }

    /// Opens a page of `size x size`.
    pub fn new_page_sized(&mut self, size: u32) {
        self.size = size;
        self.pages.push((0, 0));
        self.shelves.clear();
        self.bottom = 0;
    }

    /// Places one `w x h` rect on the current page and returns its top-left corner.
    /// The caller checks [`Self::fits`] first.
    pub fn place(&mut self, w: u32, h: u32) -> (u32, u32) {
        assert!(self.fits(w, h, 1), "{w}x{h} does not fit the current page");
        let max = self.size;
        let shelf = match self.shelves.get_mut(&h) {
            Some(s) if s.0 + w <= max => s,
            _ => {
                let top = self.bottom;
                self.bottom += h;
                self.shelves.insert(h, (0, top));
                self.shelves.get_mut(&h).expect("just inserted")
            }
        };
        let pos = *shelf;
        shelf.0 += w;
        let used = self.pages.last_mut().expect("fits() needs an open page");
        *used = (used.0.max(pos.0 + w), used.1.max(pos.1 + h));
        pos
    }
}

/// How many `w x h` rects a `size x size` page holds.
pub fn capacity(size: u32, w: u32, h: u32) -> u64 {
    u64::from(size / w.max(1)) * u64::from(size / h.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Places groups of equal rects like the importer does; returns `(page, x, y, w, h)`.
    fn pack(groups: &[(u32, u32, usize)], max: u32) -> (AtlasPacker, Vec<[u32; 5]>) {
        let mut p = AtlasPacker::new(max);
        let mut out = Vec::new();
        for &(w, h, n) in groups {
            if !p.fits(w, h, n) {
                p.new_page();
                assert!(p.fits(w, h, n), "group {w}x{h}x{n} too large for a page");
            }
            for _ in 0..n {
                let (x, y) = p.place(w, h);
                out.push([p.current_page().unwrap(), x, y, w, h]);
            }
        }
        (p, out)
    }

    #[test]
    fn packs_without_overlap_within_bounds() {
        let groups = [
            (32, 32, 40),
            (96, 96, 3),
            (32, 32, 30),
            (64, 32, 5),
            (32, 64, 7),
            (100, 50, 2),
            (16, 16, 9),
            (128, 128, 4),
            (32, 32, 60),
        ];
        let (p, rects) = pack(&groups, 256);
        assert!(p.pages().len() > 1);
        for (i, a) in rects.iter().enumerate() {
            let (pw, ph) = p.pages()[a[0] as usize];
            assert!(
                a[1] + a[3] <= pw && a[2] + a[4] <= ph,
                "rect {i} outside its page"
            );
            for (j, b) in rects.iter().enumerate().skip(i + 1) {
                let overlap = a[0] == b[0]
                    && a[1] < b[1] + b[3]
                    && b[1] < a[1] + a[3]
                    && a[2] < b[2] + b[4]
                    && b[2] < a[2] + a[4];
                assert!(!overlap, "rects {i} and {j} overlap");
            }
        }
    }

    #[test]
    fn equal_rects_fill_a_page_exactly() {
        let (p, _) = pack(&[(32, 32, 60), (32, 32, 4), (32, 32, 1)], 256);
        assert_eq!(p.pages(), [(256, 256), (32, 32)]);
    }

    #[test]
    fn interleaved_heights_keep_their_own_shelves() {
        // A 64-high group between two 32-high groups must not leave a half-empty 32 shelf.
        let (p, rects) = pack(&[(32, 32, 4), (64, 64, 1), (32, 32, 4)], 256);
        assert_eq!(p.pages().len(), 1);
        assert_eq!(rects[5][1..3], [128, 0]);
    }

    #[test]
    fn sized_page_holds_more_and_the_next_page_is_regular() {
        let mut p = AtlasPacker::new(64);
        p.new_page_sized(128);
        assert!(p.fits(64, 64, 4));
        for _ in 0..4 {
            p.place(64, 64);
        }
        p.new_page();
        assert!(!p.fits(64, 64, 2));
        assert_eq!(p.pages(), [(128, 128), (0, 0)]);
    }

    #[test]
    fn oversized_rect_never_fits() {
        let mut p = AtlasPacker::new(256);
        p.new_page();
        assert!(!p.fits(300, 5, 1));
        assert!(p.fits(300, 5, 0));
    }
}
