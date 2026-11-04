//! `.dmi` files -> atlas PNGs + [`AssetManifest`].
//!
//! Output layout under `<out>`: `manifest.postcard` (postcard-encoded [`AssetManifest`]) and
//! `atlas/<n>.png`. Files are packed in [`order_by_usage`] order when map usage is given (else by
//! sorted path), states of a file in Description order, so `IconStateId`s are stable for an
//! unchanged source tree and maps.
//!
//! Licensing ([`crate::license`]): [`ICON_SOURCES_FILE`] lists every packed `.dmi` (and GAGS
//! icon) with its source `.dmi`, its atlas pages and the source commit, and the attribution
//! files of the source `.dmi` directories and their parents are copied to [`LICENSES_DIR`] at
//! their source paths.

use std::collections::HashMap;
use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, BufWriter};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rayon::prelude::*;
use thiserror::Error;

use crate::atlas::{AtlasPacker, capacity};
use crate::dmi::{Dmi, DmiMeta};
use crate::gags::{Gags, PREFIX as GAGS_PREFIX};
use crate::manifest::{AssetManifest, IconStateInfo};
use crate::usage::{DmiUsage, order_by_usage};

/// Regular page side. 1024: a page decodes and uploads within a frame (2048 took ~30 ms each
/// on the browser main thread, a visible hitch whenever a page was first needed).
pub const ATLAS_MAX: u32 = 1024;
pub const MANIFEST_FILE: &str = "manifest.postcard";
pub const ATLAS_DIR: &str = "atlas";
pub const ICON_SOURCES_FILE: &str = "ICON-SOURCES.txt";
/// `<out>/licenses/icons/<source dir>/attributions.txt` etc.
pub const LICENSES_DIR: &str = "licenses/icons";
/// Files decoded in parallel per batch; bounds peak memory to a batch of decoded sheets.
const BATCH: usize = 256;

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("png encode: {0}")]
    Encode(#[from] png::EncodingError),
    #[error("manifest encode: {0}")]
    Manifest(#[from] postcard::Error),
}

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> ImportError + '_ {
    move |source| ImportError::Io {
        path: path.to_owned(),
        source,
    }
}

/// A `.dmi` that was skipped, with the reason.
#[derive(Clone, Debug)]
pub struct Failure {
    pub dmi: String,
    pub error: String,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub files: usize,
    pub states: usize,
    /// Icons referenced by states (frames x dirs), before deduplication.
    pub icons: usize,
    /// Distinct images actually stored in atlases.
    pub stored_frames: usize,
    pub atlases: usize,
    /// Rendered GAGS icons ([`crate::gags`]) among `files`.
    pub gags: usize,
    /// Attribution/license files copied to [`LICENSES_DIR`].
    pub attributions: usize,
    pub failures: Vec<Failure>,
}

pub struct AtlasImage {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// All `.dmi` files under `source`, relative and `/`-separated, sorted. Hidden directories and
/// `node_modules` are skipped.
pub fn find_dmi_files(source: &Path) -> io::Result<Vec<String>> {
    fn walk(dir: &Path, rel: &str, out: &mut Vec<String>) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            let ty = entry.file_type()?;
            if ty.is_dir() {
                if !name.starts_with('.') && name != "node_modules" {
                    walk(&entry.path(), &rel, out)?;
                }
            } else if ty.is_file() && name.ends_with(".dmi") {
                out.push(rel);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(source, "", &mut out)?;
    out.sort();
    Ok(out)
}

/// Imports every `.dmi` under `source` into `out` (see module docs), plus the GAGS icons of
/// `gags`, packed like files named by their key. Files that fail to parse (or icons that fail
/// to render) are skipped and listed in the report; the rest is still written.
pub fn import_dmi(
    source: &Path,
    out: &Path,
    usage: Option<&DmiUsage>,
    gags: Option<&Gags>,
) -> Result<Report, ImportError> {
    let mut names = find_dmi_files(source).map_err(io_err(source))?;
    names.extend(gags.iter().flat_map(|g| g.keys.iter().cloned()));
    if let Some(usage) = usage {
        order_by_usage(&mut names, usage);
    }
    let atlas_dir = out.join(ATLAS_DIR);
    // Stale pages from a larger previous import must not survive.
    if atlas_dir.exists() {
        fs::remove_dir_all(&atlas_dir).map_err(io_err(&atlas_dir))?;
    }
    fs::create_dir_all(&atlas_dir).map_err(io_err(&atlas_dir))?;

    let (manifest, mut report) = build_with(
        &names,
        |name| match (name.strip_prefix(GAGS_PREFIX), gags) {
            (Some(_), Some(g)) => g
                .library
                .render(name)
                .map(|r| Decoded::from_icons(r.meta, r.icons)),
            _ => decode(&fs::read(source.join(name)).map_err(|e| e.to_string())?),
        },
        ATLAS_MAX,
        |page, img| {
            let path = out.join(atlas_path(page));
            write_png(&path, &img)
        },
    )?;
    report.gags = manifest
        .states
        .iter()
        .filter(|s| s.dmi.starts_with(GAGS_PREFIX))
        .map(|s| &s.dmi)
        .collect::<std::collections::HashSet<_>>()
        .len();
    let path = out.join(MANIFEST_FILE);
    fs::write(&path, postcard::to_allocvec(&manifest)?).map_err(io_err(&path))?;
    report.attributions = write_icon_sources(source, out, &manifest, gags)?;
    Ok(report)
}

/// Writes [`ICON_SOURCES_FILE`] for `manifest` and copies the attribution files of its source
/// `.dmi` files to [`LICENSES_DIR`]; returns how many were copied.
fn write_icon_sources(
    source: &Path,
    out: &Path,
    manifest: &AssetManifest,
    gags: Option<&Gags>,
) -> Result<usize, ImportError> {
    // In packing order; a file split by state spans several pages.
    let mut pages: Vec<(&str, Vec<u32>)> = Vec::new();
    let mut index: HashMap<&str, usize> = HashMap::new();
    for state in &manifest.states {
        let i = *index.entry(&state.dmi).or_insert_with(|| {
            pages.push((&state.dmi, Vec::new()));
            pages.len() - 1
        });
        if !pages[i].1.contains(&state.atlas) {
            pages[i].1.push(state.atlas);
        }
    }
    let commit = crate::license::source_commit(source);
    let mut text = format!(
        "# Icons packed into atlas/*.png by `ss13-import dmi` from BandaStation (tgstation fork), \
         commit {commit}.\n# Licenses: CC BY-SA 3.0 unless the attribution/license files in \
         {LICENSES_DIR}/ (at the source paths) say otherwise; see ASSETS-LICENSE.md.\n\
         # .dmi or GAGS icon\tsource .dmi\tatlas pages\n"
    );
    let mut attributions = crate::license::Attributions::default();
    for (dmi, atlases) in &pages {
        let src = match dmi.strip_prefix(GAGS_PREFIX) {
            Some(_) => gags.and_then(|g| g.library.icon_file(dmi)).unwrap_or("?"),
            None => dmi,
        };
        if src != "?" {
            attributions.add(Path::new(src), src);
        }
        let atlases: Vec<String> = atlases.iter().map(|&p| atlas_path(p)).collect();
        text.push_str(&format!("{dmi}\t{src}\t{}\n", atlases.join(",")));
    }
    let licenses = out.join(LICENSES_DIR);
    // Files of `.dmi`s no longer packed must not survive.
    if licenses.exists() {
        fs::remove_dir_all(&licenses).map_err(io_err(&licenses))?;
    }
    let copied = attributions
        .copy(source, &licenses)
        .map_err(io_err(&licenses))?;
    let path = out.join(ICON_SOURCES_FILE);
    fs::write(&path, text).map_err(io_err(&path))?;
    Ok(copied)
}

pub fn atlas_path(page: u32) -> String {
    format!("{ATLAS_DIR}/{page}.png")
}

fn write_png(path: &Path, img: &AtlasImage) -> Result<(), ImportError> {
    let file = fs::File::create(path).map_err(io_err(path))?;
    let mut enc = png::Encoder::new(BufWriter::new(file), img.width, img.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Fast);
    let mut writer = enc.write_header()?;
    writer.write_image_data(&img.rgba)?;
    writer.finish()?;
    Ok(())
}

/// A decoded `.dmi` with identical icons collapsed.
struct Decoded {
    meta: DmiMeta,
    /// Distinct icons: `(hash, rgba)`.
    frames: Vec<(u64, Vec<u8>)>,
    /// Sheet icon index -> index in `frames`.
    icon_frame: Vec<u32>,
}

fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    let dmi = Dmi::read(bytes).map_err(|e| e.to_string())?;
    // The metadata is cloned so the icons stream through deduplication without a copy.
    let icons = (0..dmi.meta.icon_count() as u32).map(|i| dmi.icon_pixels(i));
    Ok(Decoded::from_icons(dmi.meta.clone(), icons))
}

impl Decoded {
    /// `icons`: the sheet's icons in order (`meta.icon_count()` of them).
    fn from_icons(meta: DmiMeta, icons: impl IntoIterator<Item = Vec<u8>>) -> Self {
        let mut frames: Vec<(u64, Vec<u8>)> = Vec::new();
        let mut by_hash: HashMap<u64, Vec<u32>> = HashMap::new();
        let icon_frame = icons
            .into_iter()
            .map(|px| {
                let hash = hash_pixels(&px);
                let same = by_hash.entry(hash).or_default();
                if let Some(&f) = same.iter().find(|&&f| frames[f as usize].1 == px) {
                    return f;
                }
                let f = frames.len() as u32;
                frames.push((hash, px));
                same.push(f);
                f
            })
            .collect();
        Self {
            meta,
            frames,
            icon_frame,
        }
    }
}

fn hash_pixels(px: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    px.hash(&mut h);
    h.finish()
}

/// The page being filled: pixels plus what is already on it, so files sharing an image
/// (blank frames, copied sprites) store it once per page.
struct Page {
    /// Side in pixels (the row stride of `rgba`).
    size: u32,
    rgba: Vec<u8>,
    stored: HashMap<(u32, u32, u64), Vec<(u32, u32)>>,
}

impl Page {
    fn new(size: u32) -> Self {
        Self {
            size,
            rgba: vec![0; size as usize * size as usize * 4],
            stored: HashMap::new(),
        }
    }

    fn offset(&self, x: u32, y: u32) -> usize {
        (y as usize * self.size as usize + x as usize) * 4
    }

    fn find(&self, w: u32, h: u32, hash: u64, px: &[u8]) -> Option<(u32, u32)> {
        let row = w as usize * 4;
        self.stored
            .get(&(w, h, hash))?
            .iter()
            .copied()
            .find(|&(x, y)| {
                (0..h).all(|r| {
                    let start = self.offset(x, y + r);
                    let r = r as usize;
                    self.rgba[start..start + row] == px[r * row..(r + 1) * row]
                })
            })
    }

    fn store(&mut self, (x, y): (u32, u32), w: u32, h: u32, hash: u64, px: &[u8]) {
        let row = w as usize * 4;
        for r in 0..h {
            let start = self.offset(x, y + r);
            let r = r as usize;
            self.rgba[start..start + row].copy_from_slice(&px[r * row..(r + 1) * row]);
        }
        self.stored.entry((w, h, hash)).or_default().push((x, y));
    }

    /// Crops to the used area.
    fn finish(self, (w, h): (u32, u32)) -> AtlasImage {
        let row = w as usize * 4;
        let mut rgba = Vec::with_capacity(row * h as usize);
        for r in 0..h {
            let start = self.offset(0, r);
            rgba.extend_from_slice(&self.rgba[start..start + row]);
        }
        AtlasImage {
            width: w,
            height: h,
            rgba,
        }
    }
}

/// Packs `names` (read through `read`) into atlas pages of at most `max x max`, handing each
/// finished page to `sink` (called concurrently, from rayon threads). A state too large for such
/// a page gets a page of its own of up to `2 * max`, so that big texture is loaded only when the
/// state is drawn. Files that fail to read or parse are skipped and reported. Deterministic for
/// the same inputs.
pub fn build<R, S>(
    names: &[String],
    read: R,
    max: u32,
    sink: S,
) -> Result<(AssetManifest, Report), ImportError>
where
    R: Fn(&str) -> io::Result<Vec<u8>> + Sync,
    S: Fn(u32, AtlasImage) -> Result<(), ImportError> + Sync,
{
    build_with(
        names,
        |name| decode(&read(name).map_err(|e| e.to_string())?),
        max,
        sink,
    )
}

/// [`build`] with the decoding step given: `.dmi` files or rendered icons.
fn build_with<L, S>(
    names: &[String],
    load: L,
    max: u32,
    sink: S,
) -> Result<(AssetManifest, Report), ImportError>
where
    L: Fn(&str) -> Result<Decoded, String> + Sync,
    S: Fn(u32, AtlasImage) -> Result<(), ImportError> + Sync,
{
    let mut manifest = AssetManifest::default();
    let mut report = Report::default();
    let sink_error = Mutex::new(None);
    let mut packer = AtlasPacker::new(max);

    rayon::scope(|scope| {
        let sink = &sink;
        let sink_error = &sink_error;
        let close = |packer: &AtlasPacker, page: Page| {
            let index = packer.current_page().expect("a page is open");
            let img = page.finish(packer.pages()[index as usize]);
            scope.spawn(move |_| {
                if let Err(e) = sink(index, img) {
                    sink_error.lock().expect("not poisoned").get_or_insert(e);
                }
            });
        };
        let mut page: Option<Page> = None;

        for batch in names.chunks(BATCH) {
            let decoded: Vec<Result<Decoded, String>> =
                batch.par_iter().map(|name| load(name)).collect();
            for (name, decoded) in batch.iter().zip(decoded) {
                let d = match decoded {
                    Ok(d) => d,
                    Err(error) => {
                        report.failures.push(Failure {
                            dmi: name.clone(),
                            error,
                        });
                        continue;
                    }
                };
                let (w, h) = (d.meta.width, d.meta.height);
                report.files += 1;
                report.icons += d.icon_frame.len();
                // The manifest has one atlas per state: a file goes on one page when it can,
                // otherwise each state does.
                let groups: Vec<Range<usize>> =
                    if packer.page_capacity(w, h) >= d.frames.len() as u64 {
                        std::iter::once(0..d.meta.states.len()).collect()
                    } else {
                        (0..d.meta.states.len()).map(|s| s..s + 1).collect()
                    };
                let mut rects: Vec<Option<[u32; 4]>> = vec![None; d.frames.len()];
                for group in groups {
                    let states = &d.meta.states[group];
                    let mut frames: Vec<u32> = states
                        .iter()
                        .flat_map(|s| s.first_icon..s.first_icon + s.icon_count())
                        .map(|i| d.icon_frame[i as usize])
                        .collect();
                    frames.sort_unstable();
                    frames.dedup();
                    let oversized = packer.page_capacity(w, h) < frames.len() as u64;
                    if oversized && capacity(2 * max, w, h) < frames.len() as u64 {
                        report.failures.push(Failure {
                            dmi: name.clone(),
                            error: format!(
                                "state {:?}: {} frames of {w}x{h} do not fit one {big}x{big} atlas",
                                states[0].name,
                                frames.len(),
                                big = 2 * max,
                            ),
                        });
                        continue;
                    }
                    let missing = match &page {
                        Some(p) if !oversized => frames
                            .iter()
                            .filter(|&&f| {
                                let (hash, px) = &d.frames[f as usize];
                                p.find(w, h, *hash, px).is_none()
                            })
                            .count(),
                        _ => frames.len(),
                    };
                    if oversized || page.is_none() || !packer.fits(w, h, missing) {
                        if let Some(p) = page.take() {
                            close(&packer, p);
                        }
                        let size = if oversized { 2 * max } else { max };
                        packer.new_page_sized(size);
                        page = Some(Page::new(size));
                    }
                    let p = page.as_mut().expect("opened above");
                    let atlas = packer.current_page().expect("opened above");
                    for &f in &frames {
                        let (hash, px) = &d.frames[f as usize];
                        let (x, y) = p.find(w, h, *hash, px).unwrap_or_else(|| {
                            let pos = packer.place(w, h);
                            p.store(pos, w, h, *hash, px);
                            report.stored_frames += 1;
                            pos
                        });
                        rects[f as usize] = Some([x, y, w, h]);
                    }

                    report.states += states.len();
                    manifest.states.extend(states.iter().map(|s| {
                        IconStateInfo {
                            dmi: name.clone(),
                            state: s.name.clone(),
                            atlas,
                            width: w,
                            height: h,
                            dirs: s.dirs,
                            rects: (s.first_icon..s.first_icon + s.icon_count())
                                .map(|i| {
                                    rects[d.icon_frame[i as usize] as usize].expect("placed above")
                                })
                                .collect(),
                            delays: s.delays.clone(),
                            loop_count: s.loop_count,
                            rewind: s.rewind,
                            movement: s.movement,
                        }
                    }));
                    if oversized && let Some(p) = page.take() {
                        close(&packer, p);
                    }
                }
            }
        }
        if let Some(p) = page.take() {
            close(&packer, p);
        }
    });
    if let Some(e) = sink_error.into_inner().expect("not poisoned") {
        return Err(e);
    }
    report.atlases = packer.pages().len();
    manifest.atlases = (0..report.atlases as u32).map(atlas_path).collect();
    Ok((manifest, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dmi::tests::{DESC, synth_dmi};

    fn run(files: &[(&str, Vec<u8>)], max: u32) -> (AssetManifest, Report, Vec<AtlasImage>) {
        let names: Vec<String> = files.iter().map(|(n, _)| n.to_string()).collect();
        let pages = Mutex::new(Vec::new());
        let (m, r) = build(
            &names,
            |n| {
                let (_, b) = files.iter().find(|(f, _)| *f == n).unwrap();
                Ok(b.clone())
            },
            max,
            |i, img| {
                pages.lock().unwrap().push((i, img));
                Ok(())
            },
        )
        .unwrap();
        let mut pages = pages.into_inner().unwrap();
        pages.sort_by_key(|(i, _)| *i);
        (m, r, pages.into_iter().map(|(_, p)| p).collect())
    }

    fn pixel(img: &AtlasImage, x: u32, y: u32) -> [u8; 4] {
        let p = ((y * img.width + x) * 4) as usize;
        img.rgba[p..p + 4].try_into().unwrap()
    }

    #[test]
    fn rects_point_at_the_right_icons() {
        // Icon `i` of a synthetic sheet is filled with `[i, i, i, 255]`.
        let desc32 = DESC
            .replace("width = 16", "width = 32")
            .replace("height = 24", "height = 32");
        let a = synth_dmi(DESC, 16, 24, 6, 5);
        let c = synth_dmi(&desc32, 32, 32, 6, 5);
        let files = [
            ("a.dmi", a.clone()),
            ("b.dmi", a),
            ("bad.dmi", vec![1, 2, 3]),
            ("c.dmi", c),
        ];
        // a and b share 29 rects (72px of height); c's 29 32x32 rects need a second page.
        let (m, r, pages) = run(&files, 192);

        assert_eq!(r.failures.len(), 1);
        assert_eq!(r.failures[0].dmi, "bad.dmi");
        assert_eq!((r.files, r.states, r.icons), (3, 12, 87));
        assert_eq!((r.atlases, r.stored_frames), (2, 58));
        assert_eq!(m.atlases, ["atlas/0.png", "atlas/1.png"]);
        assert_eq!(pages.len(), 2);

        for (k, s) in m.states.iter().enumerate() {
            let (desc, size) = if s.dmi == "c.dmi" {
                (desc32.as_str(), 32)
            } else {
                (DESC, 16)
            };
            let src = &crate::dmi::parse_description(desc).unwrap().states[k % 4];
            assert_eq!(s.atlas, u32::from(s.dmi == "c.dmi"));
            assert_eq!((s.dirs, s.width), (src.dirs, size));
            assert_eq!(s.state, src.name);
            assert_eq!(s.delays, src.delays);
            assert_eq!(s.rects.len() as u32, src.icon_count());
            for f in 0..src.frames {
                for d in 0..src.dirs {
                    let [x, y, w, h] = s.rects[(f * u32::from(src.dirs) + u32::from(d)) as usize];
                    assert_eq!((w, h), (s.width, s.height));
                    let v = src.icon_index(f, d) as u8;
                    let img = &pages[s.atlas as usize];
                    assert_eq!(pixel(img, x, y), [v, v, v, 255]);
                    assert_eq!(pixel(img, x + w - 1, y + h - 1), [v, v, v, 255]);
                }
            }
        }
    }

    #[test]
    fn identical_files_share_rects_on_a_page() {
        let a = synth_dmi(DESC, 16, 24, 6, 5);
        let (m, r, _) = run(&[("a.dmi", a.clone()), ("b.dmi", a)], 4096);
        assert_eq!(r.atlases, 1);
        assert_eq!(r.stored_frames, 29);
        assert_eq!(m.states[1].rects, m.states[5].rects);
    }

    #[test]
    fn oversized_file_is_split_by_state() {
        // 29 distinct 16x24 icons; a 64x64 page holds 4 x 2 = 8, so states are placed one by one
        // and the 12-icon movement state gets a 128x128 page of its own.
        let a = synth_dmi(DESC, 16, 24, 6, 5);
        let (m, r, pages) = run(&[("a.dmi", a.clone())], 64);
        assert!(r.failures.is_empty(), "{:?}", r.failures);
        let placed: Vec<(&str, bool, u32)> = m
            .states
            .iter()
            .map(|s| (s.state.as_str(), s.movement, s.atlas))
            .collect();
        assert_eq!(
            placed,
            [
                ("", false, 0),
                ("walk", false, 1),
                ("walk", true, 2),
                ("q\"uote\\", false, 3)
            ]
        );
        let sizes: Vec<(u32, u32)> = pages.iter().map(|p| (p.width, p.height)).collect();
        assert_eq!(
            sizes[2],
            (128, 48),
            "12 rects of 16x24, 8 per row of a 128 page"
        );
        assert!(
            sizes
                .iter()
                .enumerate()
                .all(|(i, &(w, h))| i == 2 || (w <= 64 && h <= 64))
        );

        // A 32x32 page holds 2 icons, a 64x64 one 8: the movement state cannot be placed.
        let (_, r, _) = run(&[("a.dmi", a)], 32);
        assert_eq!(r.failures.len(), 1, "{:?}", r.failures);
        assert!(r.failures[0].error.contains("state \"walk\": 12 frames"));
    }

    #[test]
    fn icon_sources_and_attributions_follow_the_atlases() {
        let dir = std::env::temp_dir().join(format!("ss13-icon-sources-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let (source, out) = (dir.join("src"), dir.join("out"));
        fs::create_dir_all(source.join("icons/obj")).unwrap();
        fs::create_dir_all(source.join("modular/turfs")).unwrap();
        fs::write(
            source.join("icons/obj/a.dmi"),
            synth_dmi(DESC, 16, 24, 6, 5),
        )
        .unwrap();
        fs::write(
            source.join("modular/turfs/water.dmi"),
            synth_dmi(DESC, 16, 24, 6, 5),
        )
        .unwrap();
        fs::write(source.join("modular/turfs/attributions.txt"), "BY-NC-SA").unwrap();
        fs::write(source.join("icons/license.txt"), "icons").unwrap();
        // A stale file from a previous import is removed.
        fs::create_dir_all(out.join(LICENSES_DIR).join("old")).unwrap();
        fs::write(out.join(LICENSES_DIR).join("old/license.txt"), "x").unwrap();
        let report = import_dmi(&source, &out, None, None).unwrap();
        assert_eq!(report.attributions, 2);
        let licenses = out.join(LICENSES_DIR);
        assert!(licenses.join("modular/turfs/attributions.txt").is_file());
        assert!(licenses.join("icons/license.txt").is_file());
        assert!(!licenses.join("old").exists());
        let sources = fs::read_to_string(out.join(ICON_SOURCES_FILE)).unwrap();
        assert!(sources.contains("icons/obj/a.dmi\ticons/obj/a.dmi\tatlas/0.png"));
        assert!(sources.contains("modular/turfs/water.dmi\tmodular/turfs/water.dmi\tatlas/"));
        let _ = fs::remove_dir_all(&dir);
    }
}
