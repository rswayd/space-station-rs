//! Loads a map headless and composes a PNG of a tile rectangle straight from the atlases:
//! what a client would be sent (drawable, `invisibility == 0`, not under a floor tile), sorted
//! by plane, layer and spawn order. For before/after checks of the load passes; no window, no GPU.
//!
//! cargo run -p ss13_mapload --example map_shot -- --assets DIR [--map MetaStation]
//!     [--seed N] [--raw] --rect X0,Y0,X1,Y1 --out FILE.png [--scale N]
//!
//! `--raw` skips `fidelity::apply` (spawners, undertile, smoothing): the map as loaded.

use std::collections::HashMap;
use std::path::PathBuf;

use bevy::prelude::*;
use ss13_assets::manifest::AssetManifest;
use ss13_mapload::{DmAtom, DmProtoPlugin, HiddenUnderTile, MapLoadPlugin, fidelity};
use ss13_shared::{Appearance, Dir, EMISSIVE_PLANE, Facing, IconStateId, TilePos};
use ss13_world::WorldPlugin;

struct Args {
    assets: PathBuf,
    map: String,
    seed: u64,
    raw: bool,
    rect: [i32; 4],
    out: PathBuf,
    scale: u32,
}

fn args() -> Args {
    let mut a = Args {
        assets: PathBuf::from("assets/generated"),
        map: "MetaStation".into(),
        seed: 1,
        raw: false,
        rect: [0; 4],
        out: PathBuf::from("shot.png"),
        scale: 1,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let mut value = || it.next().expect("flag needs a value");
        match flag.as_str() {
            "--assets" => a.assets = value().into(),
            "--map" => a.map = value(),
            "--seed" => a.seed = value().parse().expect("--seed N"),
            "--raw" => a.raw = true,
            "--rect" => {
                let v: Vec<i32> = value()
                    .split(',')
                    .map(|n| n.parse().expect("--rect X0,Y0,X1,Y1"))
                    .collect();
                a.rect = v.try_into().expect("--rect X0,Y0,X1,Y1");
            }
            "--out" => a.out = value().into(),
            "--scale" => a.scale = value().parse().expect("--scale N"),
            other => panic!("unknown argument {other}"),
        }
    }
    a
}

struct Sprite {
    key: (i16, f32, u32),
    pos: TilePos,
    appearance: Appearance,
    dir: Dir,
}

fn main() {
    let a = args();
    let config = MapLoadPlugin {
        source: std::env::var_os("SS13_SOURCE")
            .map_or_else(|| env!("SS13_SOURCE").into(), PathBuf::from),
        maps: vec![a.map.clone()],
        assets: a.assets.clone(),
    };
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        WorldPlugin,
        DmProtoPlugin,
        ss13_proto::ProtoPlugin {
            dir: config.prototypes_dir(),
        },
    ));
    app.finish();
    let manifest = ss13_mapload::read_manifest(&a.assets).expect("manifest");
    let world = app.world_mut();
    let report = ss13_mapload::load_maps(world, &config.map_paths(), &manifest).expect("load");
    eprintln!("{report}");
    if !a.raw {
        eprintln!("{}", fidelity::apply(world, &manifest, a.seed));
    }

    let [x0, y0, x1, y1] = a.rect;
    let mut query = world.query_filtered::<(
        Entity,
        &TilePos,
        &Appearance,
        Option<&Facing>,
        Option<&DmAtom>,
    ), Without<HiddenUnderTile>>();
    let mut sprites: Vec<Sprite> = query
        .iter(world)
        .filter(|(_, p, ..)| p.z == 1 && (x0..=x1).contains(&p.x) && (y0..=y1).contains(&p.y))
        .filter(|(.., atom)| atom.is_none_or(|a| a.invisibility == 0))
        .map(|(e, p, ap, f, _)| Sprite {
            key: (ap.plane, ap.layer.0, e.index_u32()),
            pos: *p,
            appearance: ap.clone(),
            dir: f.map_or(Dir::South, |f| f.0),
        })
        .collect();
    sprites.sort_by(|a, b| {
        a.key
            .0
            .cmp(&b.key.0)
            .then(a.key.1.total_cmp(&b.key.1))
            .then(a.key.2.cmp(&b.key.2))
    });

    let (w, h) = (((x1 - x0 + 1) * 32) as u32, ((y1 - y0 + 1) * 32) as u32);
    let mut canvas = vec![0u8; (w * h * 4) as usize];
    // Space-black background like the client's clear color.
    for px in canvas.chunks_exact_mut(4) {
        px.copy_from_slice(&[0, 0, 0, 255]);
    }
    let mut atlases = Atlases::new(&a.assets, &manifest);
    for s in &sprites {
        let base_x = (s.pos.x - x0) * 32 + i32::from(s.appearance.pixel_offset[0]);
        let base_y = (y1 - s.pos.y) * 32 + 32 - i32::from(s.appearance.pixel_offset[1]);
        let color = s.appearance.color;
        atlases.blit(
            &mut canvas,
            w,
            h,
            s.appearance.icon_state,
            s.dir,
            base_x,
            base_y,
            color,
        );
        // Unlit: emissive masks only matter under lighting.
        let drawn = s
            .appearance
            .overlays
            .iter()
            .filter(|o| o.plane != Some(EMISSIVE_PLANE) && o.shown_for(s.dir));
        for o in drawn {
            atlases.blit(
                &mut canvas,
                w,
                h,
                o.icon_state,
                o.dir.unwrap_or(s.dir),
                base_x + i32::from(o.pixel_offset[0]),
                base_y - i32::from(o.pixel_offset[1]),
                o.color.or(color),
            );
        }
    }
    write_png(&a.out, &canvas, w, h, a.scale);
    if std::env::var_os("MAP_SHOT_LIST").is_some() {
        let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
        for s in &sprites {
            let info = manifest.get(s.appearance.icon_state).expect("known state");
            *counts
                .entry(format!("{}:{}", info.dmi, info.state))
                .or_default() += 1;
        }
        for (k, n) in counts {
            println!("{n} {k}");
        }
    }
    eprintln!(
        "{} sprites -> {} ({}x{})",
        sprites.len(),
        a.out.display(),
        w * a.scale,
        h * a.scale
    );
}

struct Atlases<'a> {
    root: PathBuf,
    manifest: &'a AssetManifest,
    pages: HashMap<u32, (u32, u32, Vec<u8>)>,
}

impl<'a> Atlases<'a> {
    fn new(root: &std::path::Path, manifest: &'a AssetManifest) -> Self {
        Self {
            root: root.to_owned(),
            manifest,
            pages: HashMap::new(),
        }
    }

    /// Draws frame 0 of `id`, bottom-left corner at canvas `(x, bottom)`.
    fn blit(
        &mut self,
        canvas: &mut [u8],
        cw: u32,
        ch: u32,
        id: IconStateId,
        dir: Dir,
        x: i32,
        bottom: i32,
        color: Option<[u8; 4]>,
    ) {
        let Some(info) = self.manifest.get(id) else {
            return;
        };
        let d = Dir::DMI_ORDER[..usize::from(info.dirs.max(1))]
            .iter()
            .position(|o| *o == dir)
            .unwrap_or(0);
        let Some(&[rx, ry, rw, rh]) = info.rects.get(d) else {
            return;
        };
        let root = &self.root;
        let manifest = self.manifest;
        let (aw, _, pixels) = self
            .pages
            .entry(info.atlas)
            .or_insert_with(|| read_png(&root.join(&manifest.atlases[info.atlas as usize])));
        let tint = color.unwrap_or([255; 4]);
        let top = bottom - rh as i32;
        for sy in 0..rh {
            for sx in 0..rw {
                let (cx, cy) = (x + sx as i32, top + sy as i32);
                if cx < 0 || cy < 0 || cx >= cw as i32 || cy >= ch as i32 {
                    continue;
                }
                let si = (((ry + sy) * *aw + rx + sx) * 4) as usize;
                let src: [u8; 4] = std::array::from_fn(|i| {
                    (u16::from(pixels[si + i]) * u16::from(tint[i]) / 255) as u8
                });
                let alpha = u32::from(src[3]);
                if alpha == 0 {
                    continue;
                }
                let di = ((cy as u32 * cw + cx as u32) * 4) as usize;
                for i in 0..3 {
                    let (s, dst) = (u32::from(src[i]), u32::from(canvas[di + i]));
                    canvas[di + i] = ((s * alpha + dst * (255 - alpha)) / 255) as u8;
                }
                canvas[di + 3] = 255;
            }
        }
    }
}

fn read_png(path: &std::path::Path) -> (u32, u32, Vec<u8>) {
    let file = std::fs::File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().expect("atlas png");
    let mut buf = vec![0; reader.output_buffer_size().expect("atlas fits in memory")];
    let frame = reader.next_frame(&mut buf).expect("atlas frame");
    buf.truncate(frame.buffer_size());
    let rgba = match frame.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        other => panic!("atlas color type {other:?}"),
    };
    (frame.width, frame.height, rgba)
}

fn write_png(path: &std::path::Path, rgba: &[u8], w: u32, h: u32, scale: u32) {
    let scale = scale.max(1);
    let (sw, sh) = (w * scale, h * scale);
    let mut out = vec![0u8; (sw * sh * 4) as usize];
    for y in 0..sh {
        for x in 0..sw {
            let s = (((y / scale) * w + x / scale) * 4) as usize;
            let d = ((y * sw + x) * 4) as usize;
            out[d..d + 4].copy_from_slice(&rgba[s..s + 4]);
        }
    }
    let file = std::fs::File::create(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), sw, sh);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut w| w.write_image_data(&out))
        .expect("write png");
}
