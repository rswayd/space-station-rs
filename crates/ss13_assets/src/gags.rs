//! tg's Greyscale Auto-Generated Sprites (`code/datums/greyscale/`), rendered at import time.
//!
//! A `/datum/greyscale_config` names a JSON file of icon states, each a tree of layers over one
//! `.dmi`; an atom with `greyscale_config` + `greyscale_colors` gets the config rendered with
//! those colors as its `icon` (`update_greyscale`), items also their worn and in-hand files.
//! tg renders through rust-g's iconforge (`USE_RUSTG_ICONFORGE_GAGS`, `iconforge::core::gags`
//! 1.0.3), and so do the map previews in `icons/map_icons/`: this follows iconforge, including
//! its integer rounding and its quirks (a `color_matrix` layer recolors the last `icon_state`
//! layer read, not the result so far; a 1-dir layer blended onto a 4-dir one is copied to every
//! dir; a single-frame base blended with an animated layer is repeated per frame).
//!
//! A rendered config is a `.dmi`-like icon keyed by [`icon_key`], so the importer packs it next
//! to the real files and the converter points prototypes at it.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::dmi::{DEFAULT_DELAY, Dmi, DmiMeta, DmiState};

/// Manifest `dmi` names of rendered configs start with this.
pub const PREFIX: &str = "gags:";
const CONFIG_ROOT: &str = "/datum/greyscale_config";

/// Atom vars naming a config rendered with `greyscale_colors`, and the var the result replaces
/// (`/atom/update_greyscale`, `/obj/item/update_greyscale`).
// shortcut: `greyscale_config_belt` (the belt overlay) is not rendered: nothing draws belt
// overlays yet.
pub const CONFIG_VARS: [(&str, &str); 4] = [
    ("greyscale_config", "icon"),
    ("greyscale_config_worn", "worn_icon"),
    ("greyscale_config_inhand_left", "lefthand_file"),
    ("greyscale_config_inhand_right", "righthand_file"),
];

/// The manifest `dmi` of `config` rendered with `colors` (tg `GetColoredIconByType`).
pub fn icon_key(config: &str, colors: &str) -> String {
    format!("{PREFIX}{config}{colors}")
}

/// A config var's value in `dm_types.json` (`{"path": ...}`).
pub fn config_path(value: &Value) -> Option<&str> {
    let path = value
        .get("path")
        .and_then(Value::as_str)
        .or(value.as_str())?;
    path.starts_with(CONFIG_ROOT).then_some(path)
}

/// `greyscale_colors`: a non-empty string (tg skips GAGS for `null` and `""`).
pub fn colors(value: &Value) -> Option<&str> {
    value.as_str().filter(|s| !s.is_empty())
}

/// tg `ParseColorString`.
fn split_colors(colors: &str) -> Vec<String> {
    colors
        .split('#')
        .filter(|c| !c.is_empty())
        .map(|c| format!("#{c}"))
        .collect()
}

/// `#rrggbb[aa]` (or `#rgb[a]`) -> RGBA.
fn hex_rgba(color: &str) -> Result<[u8; 4], String> {
    let hex = color.strip_prefix('#').unwrap_or(color);
    let hex: String = match hex.len() {
        3 | 4 => hex.chars().flat_map(|c| [c, c]).collect(),
        _ => hex.to_owned(),
    };
    let byte = |i: usize| {
        hex.get(i..i + 2)
            .and_then(|b| u8::from_str_radix(b, 16).ok())
            .ok_or_else(|| format!("bad color {color:?}"))
    };
    match hex.len() {
        6 => Ok([byte(0)?, byte(2)?, byte(4)?, 255]),
        8 => Ok([byte(0)?, byte(2)?, byte(4)?, byte(6)?]),
        _ => Err(format!("bad color {color:?}")),
    }
}

/// BYOND `ICON_*` blend modes as iconforge implements them (`blending.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blend {
    Add,
    Subtract,
    Multiply,
    Overlay,
    And,
    Or,
    Underlay,
}

impl Blend {
    fn parse(s: &str) -> Result<Self, String> {
        Ok(match s.to_ascii_lowercase().as_str() {
            "add" => Self::Add,
            "subtract" => Self::Subtract,
            "multiply" => Self::Multiply,
            "overlay" => Self::Overlay,
            "and" => Self::And,
            "or" => Self::Or,
            "underlay" => Self::Underlay,
            _ => return Err(format!("unknown blend_mode {s:?}")),
        })
    }

    /// `dst` (the icon so far) blended with `src` (the layer).
    pub fn pixel(self, dst: [u8; 4], src: [u8; 4]) -> [u8; 4] {
        let [r1, g1, b1, a1] = dst;
        let [r2, g2, b2, a2] = src;
        let add = |a: u8, b: u8| a.saturating_add(b);
        let sub = |a: u8, b: u8| a.saturating_sub(b);
        let mul = |a: u8, b: u8| ((u16::from(a) * u16::from(b)) / 255) as u8;
        // iconforge's `overlay_channel(c_src, c_dst, a_src, a_dst)`, argument names kept.
        let over = |c_src: u8, c_dst: u8, a_src: u8, a_dst: u8| {
            if a_src == 0 {
                c_dst
            } else {
                let delta = (i32::from(c_dst) - i32::from(c_src)) * i32::from(a_dst) / 255;
                (i32::from(c_src) + delta).clamp(0, 255) as u8
            }
        };
        let over_alpha = |a_src: u8, a_dst: u8| {
            let (s, d) = (f32::from(a_src) / 255.0, f32::from(a_dst) / 255.0);
            ((s + d * (1.0 - s)) * 255.0).round().clamp(0.0, 255.0) as u8
        };
        match self {
            Self::Add | Self::And => [add(r1, r2), add(g1, g2), add(b1, b2), alpha(a1, a2)],
            Self::Subtract => [sub(r1, r2), sub(g1, g2), sub(b1, b2), alpha(a1, a2)],
            Self::Multiply => [mul(r1, r2), mul(g1, g2), mul(b1, b2), alpha(a1, a2)],
            Self::Overlay => [
                over(r1, r2, a1, a2),
                over(g1, g2, a1, a2),
                over(b1, b2, a1, a2),
                over_alpha(a1, a2),
            ],
            Self::Or if a1 == 0 => src,
            Self::Or if a2 == 0 => dst,
            Self::Or => {
                let i = (0x10000usize - usize::from(a1) - usize::from(a2) * 256).min(65535);
                [
                    add(r1, r2),
                    add(g1, g2),
                    add(b1, b2),
                    !alpha_table((i / 256) as u8, (i % 256) as u8),
                ]
            }
            Self::Underlay => [
                over(r2, r1, a2, a1),
                over(g2, g1, a2, a1),
                over(b2, b1, a2, a1),
                over_alpha(a2, a1),
            ],
        }
    }
}

/// iconforge `ALPHA_TABLE[dst * 256 + src]`: `src * dst / 255`, rounded half up in `f32`.
fn alpha_table(dst: u8, src: u8) -> u8 {
    let v = (f32::from(src) * (f32::from(dst) / 255.0) + 0.5).floor() as i32;
    if (0..256).contains(&v) { v as u8 } else { 0xff }
}

/// iconforge `alpha_lookup(a_src, a_dst)` = `ALPHA_TABLE[a_dst + a_src * 256]`.
fn alpha(a1: u8, a2: u8) -> u8 {
    alpha_table(a1, a2)
}

#[derive(Clone, Debug)]
enum ColorId {
    /// 1-based index into the colors.
    Index(usize),
    Static(String),
}

#[derive(Clone, Debug)]
enum Kind {
    IconState(String),
    Reference { config: String, state: String },
    ColorMatrix([[f32; 4]; 5]),
}

#[derive(Clone, Debug)]
struct Layer {
    kind: Kind,
    blend: Blend,
    color_ids: Vec<ColorId>,
}

#[derive(Clone, Debug)]
enum Node {
    Layer(Layer),
    Group(Vec<Node>),
}

fn parse_node(v: &Value) -> Result<Node, String> {
    if let Some(items) = v.as_array() {
        return Ok(Node::Group(
            items.iter().map(parse_node).collect::<Result<_, _>>()?,
        ));
    }
    let obj = v.as_object().ok_or("a layer is not an object or a list")?;
    let text = |k: &str| obj.get(k).and_then(Value::as_str);
    let blend = Blend::parse(text("blend_mode").ok_or("layer without blend_mode")?)?;
    let color_ids = match obj.get("color_ids") {
        None => Vec::new(),
        Some(ids) => ids
            .as_array()
            .ok_or("color_ids is not a list")?
            .iter()
            .map(|id| match id {
                Value::Number(n) => n
                    .as_u64()
                    .filter(|&n| n >= 1)
                    .map(|n| ColorId::Index(n as usize))
                    .ok_or_else(|| format!("bad color id {n}")),
                Value::String(s) => Ok(ColorId::Static(s.clone())),
                _ => Err(format!("bad color id {id}")),
            })
            .collect::<Result<_, _>>()?,
    };
    let kind = match text("type") {
        Some("icon_state") => Kind::IconState(
            text("icon_state")
                .ok_or("icon_state layer without icon_state")?
                .to_owned(),
        ),
        Some("reference") => Kind::Reference {
            config: text("reference_type")
                .ok_or("reference layer without reference_type")?
                .to_owned(),
            state: text("icon_state").unwrap_or_default().to_owned(),
        },
        Some("color_matrix") => {
            let rows = obj
                .get("color_matrix")
                .and_then(Value::as_array)
                .ok_or("color_matrix layer without color_matrix")?;
            if !(4..=5).contains(&rows.len()) {
                return Err("color_matrix needs 4 or 5 rows".into());
            }
            let mut m = [[0.0; 4]; 5];
            for (row, values) in m.iter_mut().zip(rows) {
                let values = values.as_array().filter(|r| r.len() == 4);
                let values = values.ok_or("color_matrix rows need 4 numbers")?;
                for (cell, v) in row.iter_mut().zip(values) {
                    *cell = v.as_f64().ok_or("color_matrix needs numbers")? as f32;
                }
            }
            Kind::ColorMatrix(m)
        }
        other => return Err(format!("unknown layer type {other:?}")),
    };
    Ok(Node::Layer(Layer {
        kind,
        blend,
        color_ids,
    }))
}

/// A config's JSON: icon state -> layers, sorted by name (iconforge's output order).
fn parse_config(json: &[u8]) -> Result<Vec<(String, Vec<Node>)>, String> {
    let v: Value = serde_json::from_slice(json).map_err(|e| e.to_string())?;
    let states = v.as_object().ok_or("the config is not an object")?;
    let mut out = Vec::new();
    for (name, layers) in states {
        let Node::Group(layers) = parse_node(layers)? else {
            return Err(format!("state {name:?}: layers are not a list"));
        };
        out.push((name.clone(), layers));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// Frames of one icon state: `icons[frame * dirs + dir]`, RGBA8 rows.
#[derive(Clone, Debug, PartialEq)]
pub struct Frames {
    pub width: u32,
    pub height: u32,
    pub dirs: u8,
    pub frames: u32,
    pub delays: Vec<f32>,
    pub loop_count: u32,
    pub rewind: bool,
    pub movement: bool,
    pub icons: Vec<Vec<u8>>,
}

impl Frames {
    fn of(dmi: &Dmi, s: &DmiState) -> Self {
        Self {
            width: dmi.meta.width,
            height: dmi.meta.height,
            dirs: s.dirs,
            frames: s.frames,
            delays: s.delays.clone(),
            loop_count: s.loop_count,
            rewind: s.rewind,
            movement: s.movement,
            icons: (0..s.icon_count())
                .map(|i| dmi.icon_pixels(s.first_icon + i))
                .collect(),
        }
    }

    fn map_pixels(&mut self, f: impl Fn([u8; 4]) -> [u8; 4]) {
        for icon in &mut self.icons {
            for px in icon.chunks_exact_mut(4) {
                let out = f([px[0], px[1], px[2], px[3]]);
                px.copy_from_slice(&out);
            }
        }
    }
}

/// `src` drawn onto `dst` at BYOND position (1, 1): bottom-left corners aligned, clipped
/// (iconforge `blend_icon`).
fn blend_icon(dst: &mut [u8], (dw, dh): (u32, u32), src: &[u8], (sw, sh): (u32, u32), mode: Blend) {
    let y0 = i64::from(dh) - i64::from(sh);
    for y in 0..sh {
        let ty = i64::from(y) + y0;
        if ty < 0 || ty >= i64::from(dh) {
            continue;
        }
        for x in 0..sw.min(dw) {
            let d = ((ty as usize) * dw as usize + x as usize) * 4;
            let s = (y as usize * sw as usize + x as usize) * 4;
            let out = mode.pixel(
                dst[d..d + 4].try_into().expect("4 bytes"),
                src[s..s + 4].try_into().expect("4 bytes"),
            );
            dst[d..d + 4].copy_from_slice(&out);
        }
    }
}

/// iconforge `blend_images_other`: `layer` blended onto `base`; the result keeps `base`'s
/// metadata.
pub fn blend_frames(mut base: Frames, mut layer: Frames, mode: Blend) -> Result<Frames, String> {
    if base.dirs != layer.dirs {
        if layer.dirs != 1 || base.dirs < layer.dirs {
            return Err(format!(
                "cannot blend {} dirs onto {} dirs",
                layer.dirs, base.dirs
            ));
        }
        let dirs = usize::from(base.dirs);
        layer.icons = layer
            .icons
            .iter()
            .flat_map(|i| std::iter::repeat_n(i.clone(), dirs))
            .collect();
        layer.dirs = base.dirs;
    }
    if base.frames != layer.frames {
        if base.frames != 1 || layer.frames < 2 {
            return Err(format!(
                "cannot blend {} frames onto {} frames",
                layer.frames, base.frames
            ));
        }
        let first = base.icons.clone();
        for _ in 1..layer.frames {
            base.icons.extend(first.iter().cloned());
        }
        base.frames = layer.frames;
        // iconforge keeps the base's delays, padded with its first (BYOND's default for a single
        // frame): the layer's timing is dropped.
        let pad = base.delays.first().copied().unwrap_or(DEFAULT_DELAY);
        base.delays.resize(base.frames as usize, pad);
    }
    let (dsize, ssize) = ((base.width, base.height), (layer.width, layer.height));
    let single = layer.icons.len() == 1;
    for (i, icon) in base.icons.iter_mut().enumerate() {
        let src = &layer.icons[if single { 0 } else { i }];
        blend_icon(icon, dsize, src, ssize, mode);
    }
    Ok(base)
}

/// iconforge `map_colors` with a 5x4 matrix (rows: r, g, b, a, constant).
fn map_colors(px: [u8; 4], m: &[[f32; 4]; 5]) -> [u8; 4] {
    let c = px.map(|v| f32::from(v) / 255.0);
    let mut out = [0u8; 4];
    for (k, o) in out.iter_mut().enumerate() {
        let v = m[0][k] * c[0] + m[1][k] * c[1] + m[2][k] * c[2] + m[3][k] * c[3] + m[4][k];
        *o = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    }
    out
}

struct Config {
    icon_file: String,
    states: Result<Vec<(String, Vec<Node>)>, String>,
}

/// Every config of the source tree, and the `.dmi` files they read (decoded once).
pub struct Library {
    source: PathBuf,
    configs: HashMap<String, Config>,
    icons: Mutex<HashMap<String, Arc<Result<Dmi, String>>>>,
}

/// A rendered config: `.dmi` metadata (`first_icon` set) and its icons in sheet order.
pub struct Rendered {
    pub meta: DmiMeta,
    pub icons: Vec<Vec<u8>>,
}

impl Library {
    /// `configs`: `(config type, json_config, icon_file)`, files relative to `source`. A JSON
    /// that does not read or parse fails the renders that need it.
    pub fn new<'a>(
        source: &Path,
        configs: impl IntoIterator<Item = (&'a str, &'a str, &'a str)>,
    ) -> Self {
        let configs = configs
            .into_iter()
            .map(|(ty, json, icon)| {
                let states = std::fs::read(source.join(json))
                    .map_err(|e| format!("{json}: {e}"))
                    .and_then(|b| parse_config(&b).map_err(|e| format!("{json}: {e}")));
                (
                    ty.to_owned(),
                    Config {
                        icon_file: icon.to_owned(),
                        states,
                    },
                )
            })
            .collect();
        Self {
            source: source.to_owned(),
            configs,
            icons: Mutex::new(HashMap::new()),
        }
    }

    pub fn len(&self) -> usize {
        self.configs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.configs.is_empty()
    }

    fn dmi(&self, file: &str) -> Arc<Result<Dmi, String>> {
        if let Some(d) = self.icons.lock().expect("not poisoned").get(file) {
            return d.clone();
        }
        // Decoded outside the lock; a racing thread may decode the same file twice.
        let dmi = std::fs::read(self.source.join(file))
            .map_err(|e| e.to_string())
            .and_then(|b| Dmi::read(&b).map_err(|e| e.to_string()))
            .map_err(|e| format!("{file}: {e}"));
        let dmi = Arc::new(dmi);
        self.icons
            .lock()
            .expect("not poisoned")
            .entry(file.to_owned())
            .or_insert(dmi)
            .clone()
    }

    fn config(&self, ty: &str) -> Result<&Config, String> {
        self.configs
            .get(ty)
            .ok_or_else(|| format!("unknown greyscale config {ty}"))
    }

    /// The `.dmi` an [`icon_key`]'s config draws from (its `icon_file`), relative to the source.
    pub fn icon_file(&self, key: &str) -> Option<&str> {
        let rest = key.strip_prefix(PREFIX)?;
        let ty = &rest[..rest.find('#').unwrap_or(rest.len())];
        self.configs.get(ty).map(|c| c.icon_file.as_str())
    }

    /// Renders an [`icon_key`]: every icon state of the config (tg `GetColoredIconByType`).
    pub fn render(&self, key: &str) -> Result<Rendered, String> {
        let rest = key
            .strip_prefix(PREFIX)
            .ok_or_else(|| format!("{key}: not a GAGS key"))?;
        let (ty, colors) = rest.split_at(rest.find('#').unwrap_or(rest.len()));
        let colors = split_colors(colors);
        let config = self.config(ty)?;
        let states = config.states.as_ref().map_err(Clone::clone)?;
        let dmi = self.dmi(&config.icon_file);
        let dmi = dmi.as_ref().as_ref().map_err(Clone::clone)?;
        let (width, height) = (dmi.meta.width, dmi.meta.height);
        let mut meta = DmiMeta {
            width,
            height,
            states: Vec::new(),
        };
        let mut icons = Vec::new();
        for (name, layers) in states {
            let mut last = None;
            let f = self
                .group(ty, layers, &colors, &mut last)
                .map_err(|e| format!("{ty} state {name:?}: {e}"))?;
            if (f.width, f.height) != (width, height) {
                return Err(format!(
                    "{ty} state {name:?}: {}x{} in a {width}x{height} icon",
                    f.width, f.height
                ));
            }
            meta.states.push(DmiState {
                name: name.clone(),
                dirs: f.dirs,
                frames: f.frames,
                delays: if f.frames > 1 { f.delays } else { Vec::new() },
                loop_count: f.loop_count,
                rewind: f.rewind,
                movement: f.movement,
                hotspots: Vec::new(),
                first_icon: icons.len() as u32,
            });
            icons.extend(f.icons);
        }
        Ok(Rendered { meta, icons })
    }

    /// One state of `ty` (a reference layer).
    fn state(
        &self,
        ty: &str,
        name: &str,
        colors: &[String],
        last: &mut Option<Frames>,
    ) -> Result<Frames, String> {
        let config = self.config(ty)?;
        let states = config.states.as_ref().map_err(Clone::clone)?;
        let (_, layers) = states
            .iter()
            .find(|(n, _)| n == name)
            .ok_or_else(|| format!("{ty} has no state {name:?}"))?;
        self.group(ty, layers, colors, last)
    }

    /// `last`: the raw frames of the last `icon_state` layer read (iconforge
    /// `last_matched_state`), what a `color_matrix` layer recolors.
    fn group(
        &self,
        ty: &str,
        nodes: &[Node],
        colors: &[String],
        last: &mut Option<Frames>,
    ) -> Result<Frames, String> {
        let mut acc: Option<Frames> = None;
        for node in nodes {
            let (frames, blend) = match node {
                Node::Layer(layer) => (self.layer(ty, layer, colors, last)?, layer.blend),
                Node::Group(group) => {
                    // A group blends with its first layer's mode.
                    let Some(Node::Layer(first)) = group.first() else {
                        return Err("a layer group must start with a layer".into());
                    };
                    (self.group(ty, group, colors, last)?, first.blend)
                }
            };
            acc = Some(match acc {
                None => frames,
                Some(base) => blend_frames(base, frames, blend)?,
            });
        }
        acc.ok_or_else(|| "empty layer group".into())
    }

    fn layer(
        &self,
        ty: &str,
        layer: &Layer,
        colors: &[String],
        last: &mut Option<Frames>,
    ) -> Result<Frames, String> {
        let color = |id: &ColorId| -> Result<String, String> {
            match id {
                ColorId::Index(i) => colors
                    .get(i - 1)
                    .cloned()
                    .ok_or_else(|| format!("color id {i} but only {} colors given", colors.len())),
                ColorId::Static(c) => Ok(c.clone()),
            }
        };
        match &layer.kind {
            Kind::IconState(name) => {
                let file = &self.config(ty)?.icon_file;
                let dmi = self.dmi(file);
                let dmi = dmi.as_ref().as_ref().map_err(Clone::clone)?;
                // iconforge takes the first state with the name, movement or not.
                let state = dmi
                    .meta
                    .states
                    .iter()
                    .find(|s| s.name == *name)
                    .ok_or_else(|| format!("{file} has no state {name:?}"))?;
                let mut frames = Frames::of(dmi, state);
                *last = Some(frames.clone());
                // Only the first id counts (DM refuses more than one on this layer type).
                if let Some(id) = layer.color_ids.first() {
                    let rgba = hex_rgba(&color(id)?)?;
                    frames.map_pixels(|px| Blend::Multiply.pixel(px, rgba));
                }
                Ok(frames)
            }
            Kind::Reference { config, state } => {
                let colors: Vec<String> = if layer.color_ids.is_empty() {
                    colors.to_vec()
                } else {
                    layer
                        .color_ids
                        .iter()
                        .map(color)
                        .collect::<Result<_, _>>()?
                };
                self.state(config, state, &colors, last)
            }
            Kind::ColorMatrix(m) => {
                let mut frames = last
                    .clone()
                    .ok_or("color_matrix layer before any icon_state layer")?;
                frames.map_pixels(|px| map_colors(px, m));
                Ok(frames)
            }
        }
    }
}

/// What the importer renders: the configs and the keys some type uses.
pub struct Gags {
    pub library: Library,
    pub keys: BTreeSet<String>,
}

/// The keys to render: every `(config, colors)` some type uses through [`CONFIG_VARS`].
/// `resolved(type, var)` is the var's value as the type sees it.
pub fn used_keys<'a>(
    types: impl IntoIterator<Item = &'a str>,
    resolved: impl Fn(&str, &str) -> Option<&'a Value>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for ty in types {
        let Some(colors) = resolved(ty, "greyscale_colors").and_then(colors) else {
            continue;
        };
        for (var, _) in CONFIG_VARS {
            if let Some(config) = resolved(ty, var).and_then(config_path) {
                out.insert(icon_key(config, colors));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(dirs: u8, frames: u32, px: &[[u8; 4]]) -> Frames {
        Frames {
            width: 1,
            height: 1,
            dirs,
            frames,
            delays: if frames > 1 {
                vec![2.0; frames as usize]
            } else {
                Vec::new()
            },
            loop_count: 0,
            rewind: false,
            movement: false,
            icons: px.iter().map(|p| p.to_vec()).collect(),
        }
    }

    #[test]
    fn blend_modes_match_iconforge() {
        let dst = [200, 100, 50, 255];
        let src = [100, 200, 10, 128];
        assert_eq!(Blend::Add.pixel(dst, src), [255, 255, 60, 128]);
        assert_eq!(Blend::Subtract.pixel(dst, src), [100, 0, 40, 128]);
        // 200 * 100 / 255 = 78.4 -> 78; alpha 128 * 255 / 255.
        assert_eq!(Blend::Multiply.pixel(dst, src), [78, 78, 1, 128]);
        // Over an opaque pixel: dst + (src - dst) * a_src / 255, truncated toward zero.
        assert_eq!(Blend::Overlay.pixel(dst, src), [150, 150, 30, 255]);
        // A transparent dst takes the src color, alpha by "over".
        assert_eq!(Blend::Overlay.pixel([0, 0, 0, 0], src), [100, 200, 10, 128]);
        assert_eq!(
            Blend::Underlay.pixel(src, dst),
            Blend::Overlay.pixel(dst, src)
        );
        assert_eq!(Blend::Or.pixel([0, 0, 0, 0], src), src);
        assert_eq!(Blend::Or.pixel(dst, [9, 9, 9, 0]), dst);
        // Alpha of `or`: 255 - (255 - a1) * (255 - a2) / 255.
        assert_eq!(
            Blend::Or.pixel([1, 2, 3, 128], [1, 1, 1, 128]),
            [2, 3, 4, 191]
        );
        // Multiply alpha rounds half up: 128 * 128 / 255 = 64.25.
        assert_eq!(
            Blend::Multiply.pixel([255; 4], [255, 255, 255, 128]),
            [255, 255, 255, 128]
        );
        assert_eq!(alpha(128, 128), 64);
        assert_eq!(alpha(255, 77), 77);
    }

    #[test]
    fn one_dir_layers_cover_every_dir_and_single_frames_repeat() {
        let base = frames(4, 1, &[[255, 0, 0, 255]; 4]);
        let layer = frames(1, 1, &[[0, 0, 255, 128]]);
        let out = blend_frames(base.clone(), layer, Blend::Overlay).unwrap();
        assert_eq!(out.dirs, 4);
        assert!(out.icons.iter().all(|i| i == &[127, 0, 128, 255]));

        let anim = frames(4, 3, &[[0, 0, 0, 255]; 12]);
        let out = blend_frames(base.clone(), anim.clone(), Blend::Add).unwrap();
        assert_eq!((out.dirs, out.frames, out.icons.len()), (4, 3, 12));
        // The base's (absent) delays win: BYOND's default per frame.
        assert_eq!(out.delays, [1.0; 3]);
        assert!(blend_frames(anim, base.clone(), Blend::Add).is_err());
        assert!(blend_frames(frames(1, 1, &[[0; 4]]), base, Blend::Add).is_err());
    }

    #[test]
    fn smaller_layers_align_bottom_left() {
        let mut dst = vec![0u8; 2 * 2 * 4];
        blend_icon(&mut dst, (2, 2), &[9, 9, 9, 255], (1, 1), Blend::Overlay);
        // Row 1 (bottom), column 0.
        assert_eq!(&dst[8..12], &[9, 9, 9, 255]);
        assert!(dst[..8].iter().chain(&dst[12..]).all(|&b| b == 0));
    }

    #[test]
    fn colors_parse_like_tg() {
        assert_eq!(split_colors("#b3b3b3#FFF"), ["#b3b3b3", "#FFF"]);
        assert_eq!(hex_rgba("#b3b3b3"), Ok([0xb3, 0xb3, 0xb3, 255]));
        assert_eq!(hex_rgba("#ff000080"), Ok([255, 0, 0, 128]));
        assert_eq!(hex_rgba("#fa0"), Ok([255, 170, 0, 255]));
        assert!(hex_rgba("#zz0000").is_err());
        assert_eq!(
            map_colors(
                [255, 0, 0, 255],
                &[
                    [0.0, 1.0, 0.0, 0.0],
                    [0.0; 4],
                    [0.0; 4],
                    [0.0, 0.0, 0.0, 1.0],
                    [0.0, 0.0, 0.5, 0.0]
                ]
            ),
            [0, 255, 128, 255]
        );
    }

    /// A 1x1, two-state `.dmi`: "base" (4 dirs: white, grey, red, transparent) and "spot"
    /// (1 dir, half-transparent white).
    fn write_fixture(dir: &Path) {
        let desc = "# BEGIN DMI\nversion = 4.0\n\twidth = 1\n\theight = 1\nstate = \"base\"\n\tdirs = 4\n\tframes = 1\nstate = \"spot\"\n\tdirs = 1\n\tframes = 1\n# END DMI\n";
        let px: [[u8; 4]; 5] = [
            [255, 255, 255, 255],
            [128, 128, 128, 255],
            [255, 0, 0, 255],
            [0, 0, 0, 0],
            [255, 255, 255, 128],
        ];
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, 5, 1);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.add_ztxt_chunk("Description".into(), desc.into())
            .unwrap();
        let mut w = enc.write_header().unwrap();
        w.write_image_data(px.as_flattened()).unwrap();
        w.finish().unwrap();
        std::fs::write(dir.join("a.dmi"), out).unwrap();
        std::fs::write(
            dir.join("inner.json"),
            r#"{"": [{"type": "icon_state", "icon_state": "base", "blend_mode": "overlay",
                       "color_ids": [1]}]}"#,
        )
        .unwrap();
        // "spot" over the referenced, recolored base, as a group; then a static color.
        std::fs::write(
            dir.join("outer.json"),
            r##"{"worn": [
                {"type": "reference", "reference_type": "/datum/greyscale_config/inner",
                 "blend_mode": "overlay", "color_ids": [2]},
                [{"type": "icon_state", "icon_state": "spot", "blend_mode": "overlay",
                  "color_ids": ["#0000ff"]}],
                {"type": "icon_state", "icon_state": "spot", "blend_mode": "multiply",
                 "color_ids": [1]}
            ], "b": [{"type": "icon_state", "icon_state": "missing", "blend_mode": "add"}]}"##,
        )
        .unwrap();
    }

    #[test]
    fn layers_references_and_groups_compose() {
        let dir = std::env::temp_dir().join(format!("ss13_gags_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        write_fixture(&dir);
        let lib = Library::new(
            &dir,
            [
                ("/datum/greyscale_config/inner", "inner.json", "a.dmi"),
                ("/datum/greyscale_config/outer", "outer.json", "a.dmi"),
            ],
        );
        let inner = lib
            .render(&icon_key("/datum/greyscale_config/inner", "#00ff00"))
            .unwrap();
        assert_eq!(inner.meta.states.len(), 1);
        assert_eq!(inner.meta.states[0].dirs, 4);
        assert_eq!(
            inner.icons,
            [
                vec![0, 255, 0, 255],
                vec![0, 128, 0, 255],
                vec![0, 0, 0, 255],
                vec![0, 0, 0, 0]
            ]
        );
        // "b" names a state the .dmi lacks: the whole config fails, as in iconforge.
        let err = lib
            .render(&icon_key("/datum/greyscale_config/outer", "#ff0000#00ff00"))
            .err()
            .unwrap();
        assert!(err.contains("no state \"missing\""), "{err}");

        std::fs::write(
            dir.join("outer.json"),
            std::fs::read_to_string(dir.join("outer.json"))
                .unwrap()
                .replace(r#", "b": [{"type": "icon_state", "icon_state": "missing", "blend_mode": "add"}]"#, ""),
        )
        .unwrap();
        let lib = Library::new(
            &dir,
            [
                ("/datum/greyscale_config/inner", "inner.json", "a.dmi"),
                ("/datum/greyscale_config/outer", "outer.json", "a.dmi"),
            ],
        );
        let out = lib
            .render(&icon_key("/datum/greyscale_config/outer", "#ff0000#00ff00"))
            .unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(out.meta.states[0].name, "worn");
        // Dir 0: green base; blue spot (a=128) over it: 0 + (0-0)..., g 255 + (0-255)*128/255;
        // then multiplied by the red spot (alpha 128 * 255 / 255 = 128 on an opaque pixel).
        let over = Blend::Overlay.pixel([0, 255, 0, 255], [0, 0, 255, 128]);
        let red_spot = Blend::Multiply.pixel([255, 255, 255, 128], [255, 0, 0, 255]);
        assert_eq!(out.icons[0], Blend::Multiply.pixel(over, red_spot).to_vec());
        // Dir 3 is transparent in the base: the spots alone.
        let over = Blend::Overlay.pixel([0, 0, 0, 0], [0, 0, 255, 128]);
        assert_eq!(out.icons[3], Blend::Multiply.pixel(over, red_spot).to_vec());
        // Too few colors fail instead of indexing out of range.
        assert!(
            lib.render(&icon_key("/datum/greyscale_config/outer", "#ff0000"))
                .is_err()
        );
    }
}
