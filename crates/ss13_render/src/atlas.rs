//! The importer output: the manifest at startup, atlas pages on demand, idle pages evicted over
//! a memory budget. Rect selection per the `IconStateInfo` contract.
//!
//! A page is requested the first time a sprite needs it: the sprite gets the page's handle at
//! once and draws nothing until the page is decoded off the main thread and inserted (a few
//! frames). Sprites hold strong handles, so a page in use is never freed; the atlas keeps its
//! own handle to idle pages until the resident total exceeds the budget, then drops the least
//! recently used ones (Bevy frees the GPU texture with the last handle).
//!
//! Each loaded page keeps a CPU-side [`AlphaMask`] (one bit per pixel, 1/32 of the RGBA page)
//! for mouse picking: BYOND clicks an atom only where its icon is not fully transparent.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, ensure};
use bevy::asset::{AssetHandleProvider, Handle, RenderAssetUsages};
use bevy::ecs::resource::Resource;
use bevy::image::Image;
use bevy::image::ImageSampler;
use bevy::log::error;
use bevy::math::Rect;
use bevy::platform::time::Instant;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::tasks::futures::check_ready;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use ss13_assets::manifest::{AssetManifest, IconStateInfo};
use ss13_shared::{Dir, IconStateId};

/// Default for `SS13_ATLAS_BUDGET_MB`.
pub const DEFAULT_BUDGET_MB: u64 = 256;
/// An idle page is kept at least this long, so a sprite that blinks in and out of view does not
/// reload its page every time.
pub const MIN_IDLE_SECS: f64 = 10.0;

/// Manifest plus atlas pages loaded on demand. `IconStateId` resolves through it.
#[derive(Resource, Default)]
pub struct IconAtlas {
    pub manifest: AssetManifest,
    root: PathBuf,
    /// One per `manifest.atlases`.
    pages: Vec<Page>,
    handles: Option<AssetHandleProvider>,
    /// Resident bytes above which idle pages are evicted.
    pub budget: u64,
}

#[derive(Default)]
struct Page {
    /// Present while requested: loading, loaded, or failed to load.
    handle: Option<Handle<Image>>,
    task: Option<(Task<anyhow::Result<(Image, AlphaMask)>>, Instant)>,
    /// Present while loaded.
    mask: Option<AlphaMask>,
    /// GPU bytes once loaded (RGBA8, no mips).
    bytes: u64,
    /// When a sprite last held the page (`Time` seconds).
    last_used: f64,
}

/// What [`IconAtlas`] holds right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AtlasStats {
    pub pages: usize,
    pub loaded: usize,
    pub loading: usize,
    /// Texture bytes of loaded pages.
    pub resident_bytes: u64,
    /// CPU bytes of their alpha masks.
    pub mask_bytes: u64,
}

/// Which pixels of an atlas page are not fully transparent (alpha > 0), one bit each.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AlphaMask {
    width: u32,
    height: u32,
    bits: Vec<u64>,
}

impl AlphaMask {
    /// From RGBA8 pixels, rows top-down. `None` when `rgba` does not hold `width * height` pixels.
    pub fn from_rgba8(width: u32, height: u32, rgba: &[u8]) -> Option<Self> {
        let pixels = width as usize * height as usize;
        if rgba.len() != pixels * 4 {
            return None;
        }
        let mut bits = vec![0u64; pixels.div_ceil(64)];
        for (i, px) in rgba.as_chunks::<4>().0.iter().enumerate() {
            if px[3] != 0 {
                bits[i / 64] |= 1 << (i % 64);
            }
        }
        Some(Self {
            width,
            height,
            bits,
        })
    }

    /// Whether the pixel at column `x`, row `y` (from the top) is drawn; false outside the page.
    pub fn opaque(&self, x: u32, y: u32) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        let i = y as usize * self.width as usize + x as usize;
        self.bits[i / 64] & (1 << (i % 64)) != 0
    }

    pub fn bytes(&self) -> u64 {
        self.bits.len() as u64 * 8
    }
}

impl IconAtlas {
    /// Reads `<root>/manifest.postcard`; pages are loaded later, on demand. `handles` comes from
    /// `Assets::<Image>::get_handle_provider`.
    pub fn open(root: &Path, handles: AssetHandleProvider, budget: u64) -> anyhow::Result<Self> {
        let manifest_path = root.join("manifest.postcard");
        let bytes = ss13_data::read(&manifest_path)
            .with_context(|| format!("reading {}", manifest_path.display()))?;
        let manifest: AssetManifest = postcard::from_bytes(&bytes)
            .with_context(|| format!("decoding {}", manifest_path.display()))?;
        for (i, s) in manifest.states.iter().enumerate() {
            ensure!(
                (s.atlas as usize) < manifest.atlases.len(),
                "state {i} ({} / {}) points to missing atlas {}",
                s.dmi,
                s.state,
                s.atlas
            );
        }
        Ok(Self {
            pages: manifest.atlases.iter().map(|_| Page::default()).collect(),
            manifest,
            root: root.to_owned(),
            handles: Some(handles),
            budget,
        })
    }

    /// The generated assets directory the atlas was opened from (other client files, such as
    /// the nanomap images, live there too).
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The state and its page's handle; starts loading the page if it is not resident.
    pub fn state(&mut self, id: IconStateId) -> Option<(&IconStateInfo, Handle<Image>)> {
        let page = self.manifest.get(id)?.atlas as usize;
        let handle = self.page(page)?;
        Some((self.manifest.get(id)?, handle))
    }

    fn page(&mut self, index: usize) -> Option<Handle<Image>> {
        let page = self.pages.get_mut(index)?;
        if let Some(handle) = &page.handle {
            return Some(handle.clone());
        }
        let handle = self.handles.as_ref()?.reserve_handle().typed::<Image>();
        let path = self.root.join(&self.manifest.atlases[index]);
        let task = AsyncComputeTaskPool::get().spawn(async move { decode(&path) });
        page.task = Some((task, Instant::now()));
        page.handle = Some(handle.clone());
        Some(handle)
    }

    /// The alpha mask of the page `id` is drawn from, while that page is loaded.
    pub fn mask(&self, id: IconStateId) -> Option<&AlphaMask> {
        let page = self.manifest.get(id)?.atlas as usize;
        self.pages.get(page)?.mask.as_ref()
    }

    pub fn stats(&self) -> AtlasStats {
        let loaded = self.pages.iter().filter(|p| p.bytes > 0);
        AtlasStats {
            pages: self.pages.len(),
            loaded: loaded.clone().count(),
            loading: self.pages.iter().filter(|p| p.task.is_some()).count(),
            resident_bytes: loaded.clone().map(|p| p.bytes).sum(),
            mask_bytes: loaded
                .filter_map(|p| p.mask.as_ref())
                .map(AlphaMask::bytes)
                .sum(),
        }
    }

    /// Takes decoded pages from their tasks; `insert(handle, image)` stores each one.
    /// Returns `(page, bytes, time since the request)` of each page loaded.
    pub fn finish_loads(
        &mut self,
        mut insert: impl FnMut(&Handle<Image>, Image),
    ) -> Vec<(usize, u64, Duration)> {
        let mut loaded = Vec::new();
        for (i, page) in self.pages.iter_mut().enumerate() {
            let Some((result, requested)) = page
                .task
                .as_mut()
                .and_then(|(task, at)| Some((check_ready(task)?, *at)))
            else {
                continue;
            };
            page.task = None;
            let handle = page.handle.as_ref().expect("a loading page has a handle");
            match result {
                Ok((image, mask)) => {
                    page.bytes = u64::from(image.width()) * u64::from(image.height()) * 4;
                    page.mask = Some(mask);
                    loaded.push((i, page.bytes, requested.elapsed()));
                    insert(handle, image);
                }
                // The handle stays: its sprites draw nothing instead of retrying every frame.
                Err(e) => error!("atlas page {i}: {e:#}"),
            }
        }
        loaded
    }

    /// Marks pages that sprites hold as used at `now`, then drops idle pages, least recently
    /// used first, while the resident total is over the budget. Returns the evicted pages.
    pub fn evict(&mut self, now: f64) -> Vec<usize> {
        let mut resident = 0;
        let mut idle = Vec::new();
        for (i, page) in self.pages.iter_mut().enumerate() {
            let Some(handle) = &page.handle else { continue };
            if held_elsewhere(handle) {
                page.last_used = now;
            } else if page.task.is_none() && now - page.last_used >= MIN_IDLE_SECS {
                idle.push(i);
            }
            resident += page.bytes;
        }
        if resident <= self.budget {
            return Vec::new();
        }
        idle.sort_by(|&a, &b| self.pages[a].last_used.total_cmp(&self.pages[b].last_used));
        let mut evicted = Vec::new();
        for i in idle {
            if resident <= self.budget {
                break;
            }
            let page = &mut self.pages[i];
            resident -= page.bytes;
            page.handle = None;
            page.bytes = 0;
            page.mask = None;
            evicted.push(i);
        }
        evicted
    }
}

/// Whether anything but the atlas holds `handle` (sprites clone it).
fn held_elsewhere(handle: &Handle<Image>) -> bool {
    match handle {
        Handle::Strong(arc) => Arc::strong_count(arc) > 1,
        Handle::Uuid(..) => false,
    }
}

fn decode(path: &Path) -> anyhow::Result<(Image, AlphaMask)> {
    let bytes = ss13_data::read(path).with_context(|| format!("reading {}", path.display()))?;
    page_image(&bytes).with_context(|| format!("decoding {}", path.display()))
}

/// A QOI file (`ss13_assets::page`) as an sRGB RGBA8 texture with `sampler`, kept on the GPU
/// only: an atlas page, or a station map image (`ss13_game::nanomap`).
pub fn qoi_image(bytes: &[u8], sampler: ImageSampler) -> anyhow::Result<Image> {
    let page = ss13_assets::page::decode(bytes)?;
    Ok(rgba_image(page, sampler))
}

fn rgba_image(page: ss13_assets::page::PagePixels, sampler: ImageSampler) -> Image {
    let mut image = Image::new(
        Extent3d {
            width: page.width,
            height: page.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        page.rgba,
        TextureFormat::Rgba8UnormSrgb,
        // The pixels live on the GPU only: Bevy drops the CPU copy after the upload.
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = sampler;
    image
}

/// An atlas page file as an sRGB RGBA8 texture with nearest sampling, plus its alpha mask.
fn page_image(bytes: &[u8]) -> anyhow::Result<(Image, AlphaMask)> {
    let page = ss13_assets::page::decode(bytes)?;
    let mask = AlphaMask::from_rgba8(page.width, page.height, &page.rgba)
        .context("pixel count does not match the size")?;
    Ok((rgba_image(page, ImageSampler::nearest()), mask))
}

/// `SS13_ATLAS_BUDGET_MB` in bytes, [`DEFAULT_BUDGET_MB`] when unset or invalid.
pub fn budget_from_env() -> u64 {
    let mb = std::env::var("SS13_ATLAS_BUDGET_MB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_BUDGET_MB);
    mb * 1024 * 1024
}

/// Index of `dir` inside a state with `dirs` directions (`Dir::DMI_ORDER`).
/// BYOND draws diagonals of a 4-dir state with the horizontal component (EAST/WEST).
pub fn dir_index(dirs: u8, dir: Dir) -> usize {
    let dir = match (dirs, dir) {
        (4, Dir::NorthEast | Dir::SouthEast) => Dir::East,
        (4, Dir::NorthWest | Dir::SouthWest) => Dir::West,
        _ => dir,
    };
    match dirs {
        4 | 8 => Dir::DMI_ORDER[..dirs as usize]
            .iter()
            .position(|d| *d == dir)
            .unwrap_or(0),
        _ => 0,
    }
}

pub fn frame_count(info: &IconStateInfo) -> usize {
    info.rects.len() / usize::from(info.dirs.max(1))
}

/// Atlas rect for `frame` facing `dir`, per `rects[f * dirs + d]`.
pub fn frame_rect(info: &IconStateInfo, dir: Dir, frame: usize) -> Option<Rect> {
    let d = dir_index(info.dirs, dir);
    let [x, y, w, h] = *info.rects.get(frame * usize::from(info.dirs.max(1)) + d)?;
    Some(Rect::new(
        x as f32,
        y as f32,
        (x + w) as f32,
        (y + h) as f32,
    ))
}

#[cfg(test)]
mod tests {
    use bevy::asset::Assets;

    use super::*;

    /// Four loaded 10-byte pages with the given `last_used`.
    fn atlas(images: &Assets<Image>, last_used: [f64; 4], budget: u64) -> IconAtlas {
        IconAtlas {
            pages: last_used
                .into_iter()
                .map(|t| Page {
                    handle: Some(images.reserve_handle()),
                    task: None,
                    mask: None,
                    bytes: 10,
                    last_used: t,
                })
                .collect(),
            budget,
            ..Default::default()
        }
    }

    #[test]
    fn evicts_idle_pages_lru_first_down_to_the_budget() {
        let images = Assets::<Image>::default();
        let mut a = atlas(&images, [0.0, 0.0, 5.0, 12.0], 25);
        // Page 0 is on screen; page 3 has been idle for less than MIN_IDLE_SECS.
        let sprite = a.pages[0].handle.clone();
        assert_eq!(a.evict(20.0), [1, 2]);
        assert_eq!(a.stats().resident_bytes, 20);
        assert_eq!(a.pages[0].last_used, 20.0);
        assert!(a.pages[3].handle.is_some());

        // Under the budget nothing goes, however old.
        drop(sprite);
        assert!(a.evict(100.0).is_empty());

        // Over it, a page released by its last sprite still waits MIN_IDLE_SECS.
        a.budget = 0;
        assert_eq!(a.evict(25.0), [3]);
        assert_eq!(a.evict(20.0 + MIN_IDLE_SECS), [0]);
        assert_eq!(
            a.stats(),
            AtlasStats {
                pages: 4,
                ..Default::default()
            }
        );
    }

    #[test]
    fn evicted_page_is_requested_again() {
        let images = Assets::<Image>::default();
        let mut a = atlas(&images, [0.0; 4], 0);
        a.handles = Some(images.get_handle_provider());
        a.manifest.atlases = (0..4).map(ss13_assets::import::atlas_path).collect();
        let old = a.pages[1].handle.as_ref().unwrap().id();
        assert_eq!(a.evict(MIN_IDLE_SECS), [0, 1, 2, 3]);
        bevy::tasks::AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default);
        let new = a.page(1).unwrap();
        assert_ne!(new.id(), old);
        assert_eq!(a.stats().loading, 1);
    }

    #[test]
    fn page_file_becomes_an_srgb_texture_and_its_mask() {
        // 2x2, rows top-down: only (1,0) is drawn.
        let rgba = [0, 0, 0, 0, 10, 20, 30, 255, 1, 2, 3, 0, 0, 0, 0, 0];
        let file = ss13_assets::page::encode(2, 2, &rgba).unwrap();
        let (image, mask) = page_image(&file).unwrap();
        assert_eq!((image.width(), image.height()), (2, 2));
        assert_eq!(
            image.texture_descriptor.format,
            TextureFormat::Rgba8UnormSrgb
        );
        assert_eq!(image.data.as_deref(), Some(&rgba[..]));
        let ImageSampler::Descriptor(sampler) = &image.sampler else {
            panic!("nearest sampler expected")
        };
        assert_eq!(sampler.mag_filter, bevy::image::ImageFilterMode::Nearest);
        assert_eq!(mask, AlphaMask::from_rgba8(2, 2, &rgba).unwrap());
        assert!(mask.opaque(1, 0) && !mask.opaque(0, 1));
        assert!(page_image(b"png?").is_err());
    }

    /// Decode cost of every page of real assets through the client's path.
    /// `SS13_ASSETS=... cargo test --release -p ss13_render page_decode_time -- --ignored --nocapture`
    #[test]
    #[ignore = "needs generated assets; time it in release"]
    fn page_decode_time() {
        let root = PathBuf::from(std::env::var("SS13_ASSETS").expect("SS13_ASSETS"));
        let images = Assets::<Image>::default();
        let atlas = IconAtlas::open(&root, images.get_handle_provider(), 0).unwrap();
        let mut times = Vec::new();
        for rel in &atlas.manifest.atlases {
            let start = Instant::now();
            decode(&root.join(rel)).unwrap();
            times.push(start.elapsed().as_secs_f64() * 1e3);
        }
        let mean = times.iter().sum::<f64>() / times.len() as f64;
        let max = times.iter().copied().fold(0.0, f64::max);
        println!(
            "{} pages: mean {mean:.2} ms, max {max:.2} ms (read + decode + mask)",
            times.len()
        );
    }

    #[test]
    fn alpha_mask_marks_pixels_with_any_alpha() {
        // 3x2, rows top-down: only (1,0) (alpha 1) and (2,1) (alpha 255) are drawn.
        let mut rgba = vec![255u8; 3 * 2 * 4];
        for (i, px) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            px[3] = match i {
                1 => 1,
                5 => 255,
                _ => 0,
            };
        }
        let mask = AlphaMask::from_rgba8(3, 2, &rgba).unwrap();
        let drawn: Vec<(u32, u32)> = (0..2)
            .flat_map(|y| (0..3).map(move |x| (x, y)))
            .filter(|&(x, y)| mask.opaque(x, y))
            .collect();
        assert_eq!(drawn, [(1, 0), (2, 1)]);
        assert!(!mask.opaque(3, 0) && !mask.opaque(0, 2), "outside");
        assert_eq!(mask.bytes(), 8);
        assert_eq!(AlphaMask::from_rgba8(3, 3, &rgba), None);
        // A 2048x2048 page: 512 KiB instead of 16 MiB of RGBA.
        let page = vec![0u8; 2048 * 2048 * 4];
        assert_eq!(
            AlphaMask::from_rgba8(2048, 2048, &page).unwrap().bytes(),
            512 * 1024
        );
    }

    fn state(dirs: u8, frames: u32) -> IconStateInfo {
        IconStateInfo {
            dmi: "t.dmi".into(),
            state: "s".into(),
            atlas: 0,
            width: 32,
            height: 32,
            dirs,
            // Frame f, dir d lives at x = d * 32, y = f * 32.
            rects: (0..frames)
                .flat_map(|f| (0..u32::from(dirs)).map(move |d| [d * 32, f * 32, 32, 32]))
                .collect(),
            delays: vec![1.0; frames as usize],
            loop_count: 0,
            rewind: false,
            movement: false,
        }
    }

    #[test]
    fn one_dir_ignores_dir() {
        let s = state(1, 2);
        for d in Dir::DMI_ORDER {
            assert_eq!(frame_rect(&s, d, 1), Some(Rect::new(0., 32., 32., 64.)));
        }
    }

    #[test]
    fn four_dir_cardinals_and_diagonals() {
        let s = state(4, 1);
        let x = |d| frame_rect(&s, d, 0).unwrap().min.x as u32 / 32;
        assert_eq!(x(Dir::South), 0);
        assert_eq!(x(Dir::North), 1);
        assert_eq!(x(Dir::East), 2);
        assert_eq!(x(Dir::West), 3);
        assert_eq!(x(Dir::NorthEast), 2);
        assert_eq!(x(Dir::SouthEast), 2);
        assert_eq!(x(Dir::NorthWest), 3);
        assert_eq!(x(Dir::SouthWest), 3);
    }

    #[test]
    fn eight_dir_follows_dmi_order_and_frames() {
        let s = state(8, 3);
        for (i, d) in Dir::DMI_ORDER.into_iter().enumerate() {
            let r = frame_rect(&s, d, 2).unwrap();
            assert_eq!((r.min.x as usize / 32, r.min.y as usize / 32), (i, 2));
        }
        assert_eq!(frame_count(&s), 3);
        assert_eq!(frame_rect(&s, Dir::South, 3), None);
    }
}
