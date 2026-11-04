//! Tiny generated atlas set in the importer's output format (`manifest.postcard` +
//! `atlas/<n>.qoi`), for tests, the example and the benchmark before real assets exist.

use std::path::Path;

use ss13_assets::manifest::{AssetManifest, IconStateInfo};
use ss13_shared::{Dir, IconStateId};

pub const FLOOR: IconStateId = IconStateId(0);
pub const WALL: IconStateId = IconStateId(1);
/// 4 dirs, the "nose" points where it faces.
pub const MOB: IconStateId = IconStateId(2);
/// 1 dir, 4 frames, looping.
pub const BLINK: IconStateId = IconStateId(3);
/// 1 dir, 3 frames, rewind.
pub const PULSE: IconStateId = IconStateId(4);
/// Overlay, 4 dirs.
pub const HAT: IconStateId = IconStateId(5);
/// Overlay, 1 dir.
pub const BADGE: IconStateId = IconStateId(6);
/// 8 dirs, second atlas.
pub const PIPE: IconStateId = IconStateId(7);
/// 64x64, second atlas.
pub const BIG: IconStateId = IconStateId(8);
/// 4 dirs x 2 frames, second atlas.
pub const WALKER: IconStateId = IconStateId(9);

type Rgba = [u8; 4];

struct Canvas {
    w: u32,
    h: u32,
    px: Vec<u8>,
}

impl Canvas {
    fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            px: vec![0; (w * h * 4) as usize],
        }
    }

    /// Fills `[x0, x1) x [y0, y1)` relative to `cell`; y grows downwards (image space).
    fn fill(&mut self, cell: [u32; 4], x0: u32, y0: u32, x1: u32, y1: u32, c: Rgba) {
        for y in y0..y1.min(cell[3]) {
            for x in x0..x1.min(cell[2]) {
                let i = (((cell[1] + y) * self.w + cell[0] + x) * 4) as usize;
                self.px[i..i + 4].copy_from_slice(&c);
            }
        }
    }
}

/// One state: `draw(canvas, cell, frame, dir)` paints a `w x h` cell.
struct Spec {
    name: &'static str,
    atlas: u32,
    size: u32,
    dirs: u8,
    delays: Vec<f32>,
    rewind: bool,
    draw: fn(&mut Canvas, [u32; 4], usize, Dir),
}

/// Paints a 6x6 marker at the cell edge `dir` points to.
fn nose(c: &mut Canvas, cell: [u32; 4], dir: Dir, color: Rgba) {
    let (dx, dy) = dir.offset();
    let cx = (13 + dx * 11) as u32;
    let cy = (13 - dy * 11) as u32;
    c.fill(cell, cx, cy, cx + 6, cy + 6, color);
}

fn specs() -> Vec<Spec> {
    vec![
        Spec {
            name: "floor",
            atlas: 0,
            size: 32,
            dirs: 1,
            delays: vec![],
            rewind: false,
            draw: |c, cell, _, _| {
                c.fill(cell, 0, 0, 32, 32, [70, 74, 80, 255]);
                c.fill(cell, 1, 1, 31, 31, [110, 116, 124, 255]);
            },
        },
        Spec {
            name: "wall",
            atlas: 0,
            size: 32,
            dirs: 1,
            delays: vec![],
            rewind: false,
            draw: |c, cell, _, _| {
                c.fill(cell, 0, 0, 32, 32, [40, 44, 60, 255]);
                c.fill(cell, 0, 0, 32, 4, [90, 96, 120, 255]);
            },
        },
        Spec {
            name: "mob",
            atlas: 0,
            size: 32,
            dirs: 4,
            delays: vec![],
            rewind: false,
            draw: |c, cell, _, dir| {
                c.fill(cell, 6, 6, 26, 26, [200, 120, 60, 255]);
                nose(c, cell, dir, [255, 255, 255, 255]);
            },
        },
        Spec {
            name: "blink",
            atlas: 0,
            size: 32,
            dirs: 1,
            delays: vec![3.0; 4],
            rewind: false,
            draw: |c, cell, f, _| {
                let colors = [
                    [255, 40, 40, 255],
                    [255, 220, 40, 255],
                    [40, 255, 80, 255],
                    [60, 120, 255, 255],
                ];
                c.fill(cell, 10, 10, 22, 22, colors[f]);
            },
        },
        Spec {
            name: "pulse",
            atlas: 0,
            size: 32,
            dirs: 1,
            delays: vec![2.0; 3],
            rewind: true,
            draw: |c, cell, f, _| {
                let r = 4 + 5 * f as u32;
                c.fill(cell, 16 - r, 16 - r, 16 + r, 16 + r, [180, 80, 220, 255]);
            },
        },
        Spec {
            name: "hat",
            atlas: 0,
            size: 32,
            dirs: 4,
            delays: vec![],
            rewind: false,
            draw: |c, cell, _, dir| {
                c.fill(cell, 6, 2, 26, 8, [240, 220, 40, 255]);
                nose(c, cell, dir, [40, 40, 40, 255]);
            },
        },
        Spec {
            name: "badge",
            atlas: 0,
            size: 32,
            dirs: 1,
            delays: vec![],
            rewind: false,
            draw: |c, cell, _, _| c.fill(cell, 20, 18, 25, 23, [220, 30, 30, 255]),
        },
        Spec {
            name: "pipe",
            atlas: 1,
            size: 32,
            dirs: 8,
            delays: vec![],
            rewind: false,
            draw: |c, cell, _, dir| {
                c.fill(cell, 13, 13, 19, 19, [40, 200, 220, 255]);
                nose(c, cell, dir, [40, 200, 220, 255]);
            },
        },
        Spec {
            name: "big",
            atlas: 1,
            size: 64,
            dirs: 1,
            delays: vec![],
            rewind: false,
            draw: |c, cell, _, _| {
                c.fill(cell, 2, 2, 62, 62, [30, 120, 60, 255]);
                c.fill(cell, 8, 8, 56, 20, [160, 230, 160, 255]);
            },
        },
        Spec {
            name: "walker",
            atlas: 1,
            size: 32,
            dirs: 4,
            delays: vec![4.0, 4.0],
            rewind: false,
            draw: |c, cell, f, dir| {
                let lift = 2 * f as u32;
                c.fill(cell, 8, 6 - lift, 24, 28 - lift, [60, 160, 230, 255]);
                nose(c, cell, dir, [255, 255, 255, 255]);
            },
        },
    ]
}

/// Builds the manifest and RGBA atlases (one row of cells per state, cell `f * dirs + d`).
pub fn build() -> (AssetManifest, Vec<(u32, u32, Vec<u8>)>) {
    let specs = specs();
    let atlas_count = specs.iter().map(|s| s.atlas + 1).max().unwrap_or(0);
    let mut manifest = AssetManifest::default();
    let mut images = Vec::new();
    for atlas in 0..atlas_count {
        let mine: Vec<&Spec> = specs.iter().filter(|s| s.atlas == atlas).collect();
        let cells = |s: &Spec| s.delays.len().max(1) as u32 * u32::from(s.dirs);
        let w = mine.iter().map(|s| cells(s) * s.size).max().unwrap_or(1);
        let h = mine.iter().map(|s| s.size).sum::<u32>().max(1);
        let mut canvas = Canvas::new(w, h);
        let mut y = 0;
        for s in mine {
            let mut rects = Vec::new();
            for f in 0..s.delays.len().max(1) {
                for (d, dir) in Dir::DMI_ORDER[..s.dirs as usize].iter().enumerate() {
                    let k = (f * s.dirs as usize + d) as u32;
                    let cell = [k * s.size, y, s.size, s.size];
                    (s.draw)(&mut canvas, cell, f, *dir);
                    rects.push(cell);
                }
            }
            manifest.states.push(IconStateInfo {
                dmi: "synthetic.dmi".into(),
                state: s.name.into(),
                atlas,
                width: s.size,
                height: s.size,
                dirs: s.dirs,
                rects,
                delays: s.delays.clone(),
                loop_count: 0,
                rewind: s.rewind,
                movement: false,
            });
            y += s.size;
        }
        manifest
            .atlases
            .push(ss13_assets::import::atlas_path(atlas));
        images.push((canvas.w, canvas.h, canvas.px));
    }
    // The ids above rely on `specs()` listing atlas 0 states first.
    (manifest, images)
}

/// Writes the synthetic set to `root` in the importer's layout.
pub fn write(root: &Path) -> anyhow::Result<()> {
    let (manifest, images) = build();
    std::fs::create_dir_all(root.join("atlas"))?;
    std::fs::write(
        root.join("manifest.postcard"),
        postcard::to_stdvec(&manifest)?,
    )?;
    for ((w, h, px), rel) in images.iter().zip(&manifest.atlases) {
        std::fs::write(root.join(rel), ss13_assets::page::encode(*w, *h, px)?)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use bevy::prelude::*;

    use super::*;
    use crate::IconAtlas;

    #[test]
    fn roundtrips_through_the_importer_layout() {
        let dir = std::env::temp_dir().join(format!("ss13_render_synth_{}", std::process::id()));
        write(&dir).unwrap();
        let images = Assets::<Image>::default();
        let atlas = IconAtlas::open(&dir, images.get_handle_provider(), 0).unwrap();
        assert_eq!(atlas.manifest.atlases.len(), 2);
        assert_eq!(atlas.manifest.get(MOB).unwrap().state, "mob");
        assert_eq!(atlas.manifest.get(WALKER).unwrap().rects.len(), 8);
        let big = atlas.manifest.get(BIG).unwrap();
        assert_eq!((big.width, big.atlas), (64, 1));
        // Every rect lies inside its atlas.
        let sizes: Vec<(u32, u32)> = atlas
            .manifest
            .atlases
            .iter()
            .map(|a| {
                let page = ss13_assets::page::decode(&std::fs::read(dir.join(a)).unwrap()).unwrap();
                (page.width, page.height)
            })
            .collect();
        for s in &atlas.manifest.states {
            let (w0, h0) = sizes[s.atlas as usize];
            for [x, y, w, h] in &s.rects {
                assert!(x + w <= w0 && y + h <= h0, "{}", s.state);
            }
        }
        assert!(sizes[1].0 >= 64);
        std::fs::remove_dir_all(dir).ok();
    }
}
