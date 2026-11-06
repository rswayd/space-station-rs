//! `.dmm` -> entities. Semantics follow tg's loader (`code/modules/mapping/reader.dm`): per tile
//! the last path is the area, the one before it the turf, the rest are movables in file order.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, bail};
use bevy::ecs::prelude::{Entity, Mut, Resource, World};
use bevy::platform::time::Instant;
use ss13_assets::dmm::{DmmMap, DmmObject, NO_PREFAB};
use ss13_assets::import::MANIFEST_FILE;
use ss13_assets::manifest::AssetManifest;
use ss13_atmos::{AtmosBlocker, BlocksHeat, TurfAir};
use ss13_proto::{Prototype, Prototypes};
use ss13_shared::{
    Appearance, Dir, Facing, IconStateId, Layer, LightSource, MouseOpacity, Optics, TilePos,
};
use ss13_world::{Border, Density, SpatialIndex};

use crate::light::{DmLight, light_tiles, source, tint_fixture};
use crate::value::{Skip, Val, field_for, patch_ron_with};
use crate::{Area, DmAtmosPass, DmAtom, DmBorder, DmLook, DmTurfAtmos, Opaque, Turf};

/// `world.turf` in `code/world.dm`: what a tile holds when the map gives it no turf.
pub const DEFAULT_TURF: &str = "/turf/open/space/basic";
/// `world.area` in `code/world.dm`.
pub const DEFAULT_AREA: &str = "/area/space";
/// Template-only placeholders: "keep what is there", which on a fresh level is the default.
const TURF_NOOP: &str = "/turf/template_noop";
const AREA_NOOP: &str = "/area/template_noop";

pub fn read_manifest(assets: &Path) -> anyhow::Result<AssetManifest> {
    let path = assets.join(MANIFEST_FILE);
    let bytes = ss13_data::read(&path).with_context(|| format!("reading {}", path.display()))?;
    postcard::from_bytes(&bytes).with_context(|| format!("decoding {}", path.display()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    Turf,
    Obj,
    Mob,
    Area,
    Other,
}

impl Category {
    pub fn of(path: &str) -> Self {
        let under = |root: &str| {
            path.strip_prefix(root)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        };
        if under("/turf") {
            Self::Turf
        } else if under("/obj") {
            Self::Obj
        } else if under("/mob") {
            Self::Mob
        } else if under("/area") {
            Self::Area
        } else {
            Self::Other
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IconResolution {
    Exact,
    /// The state is not in the `.dmi`; BYOND then draws the default (unnamed) state.
    DefaultState,
    /// Neither the state nor a default state exists (or the `.dmi` is unknown): not drawn.
    Missing,
    /// `icon = null`: nothing to draw.
    NoIcon,
}

/// Everything needed to spawn one DM type with one set of var edits, computed once and cloned
/// per entity.
pub struct Template {
    pub proto: Arc<Prototype>,
    pub category: Category,
    /// `None` when there is nothing to draw.
    pub appearance: Option<Appearance>,
    pub facing: Option<Facing>,
    pub dense: bool,
    /// Set on border objects ([`crate::border`]).
    pub border: Option<Border>,
    pub opaque: bool,
    /// The light it emits at round start ([`crate::light`]).
    pub light: Option<LightSource>,
    /// Turfs: their air ([`crate::atmos`]).
    pub turf_air: Option<TurfAir>,
    /// Objects that can block air.
    pub atmos_blocker: Option<AtmosBlocker>,
    /// The blocker also stops heat (superconduction).
    pub blocks_heat: bool,
    pub icon: IconResolution,
    /// Sprites of its [`DmLook`] the manifest lacks (`icon:"state"`): not drawn.
    pub missing_look: Vec<String>,
}

impl Template {
    /// `index` is [`AssetManifest::index`].
    pub fn new(proto: Arc<Prototype>, index: &HashMap<(&str, &str), IconStateId>) -> Self {
        let category = Category::of(proto.id());
        let atom = proto.get::<DmAtom>().cloned().unwrap_or_default();
        let (icon, state) = if atom.icon.is_empty() {
            (IconResolution::NoIcon, None)
        } else if let Some(&id) = index.get(&(atom.icon.as_str(), atom.icon_state.as_str())) {
            (IconResolution::Exact, Some(id))
        } else if let Some(id) = map_icon_preview(index, &atom.icon, proto.id()) {
            (IconResolution::Exact, Some(id))
        } else if let Some(&id) = index.get(&(atom.icon.as_str(), "")) {
            (IconResolution::DefaultState, Some(id))
        } else {
            (IconResolution::Missing, None)
        };
        let mut appearance = state.map(|icon_state| Appearance {
            icon_state,
            layer: Layer(atom.layer),
            plane: atom.plane,
            color: fold_color(atom.color, atom.alpha),
            pixel_offset: [atom.pixel_x, atom.pixel_y],
            rotation: 0,
            mouse_opacity: MouseOpacity::from_dm(atom.mouse_opacity),
            overlays: Vec::new(),
        });
        let mut missing_look = Vec::new();
        if let (Some(look), Some(appearance)) = (proto.get::<DmLook>(), appearance.as_mut()) {
            let index = |icon: &str, state: &str| index.get(&(icon, state)).copied();
            missing_look = crate::look::apply(&index, &atom, look, appearance);
        }
        let dir = Dir::from_bits(atom.dir).unwrap_or_default();
        // Absent `Facing` means south; only movables turn, so static atoms skip the default.
        let movable = matches!(category, Category::Obj | Category::Mob | Category::Other);
        let facing = (movable || dir != Dir::South).then_some(Facing(dir));
        let light = proto.get::<DmLight>().and_then(|l| {
            let mut light = source(proto.id(), l, atom.dir, [atom.pixel_x, atom.pixel_y])?;
            tint_fixture(proto.id(), &mut light, atom.color);
            Some(light)
        });
        let border = proto.get::<DmBorder>().and_then(|b| b.border(atom.dir));
        let turf_air = (category == Category::Turf)
            .then(|| proto.get::<DmTurfAtmos>().map(|a| a.turf_air(proto.id())))
            .flatten();
        let pass = proto.get::<DmAtmosPass>();
        let atmos_blocker = pass.and_then(|p| p.blocker());
        let blocks_heat = pass.is_some_and(|p| p.block_superconductivity);
        Self {
            proto,
            category,
            appearance,
            facing,
            dense: atom.density,
            border,
            opaque: atom.opacity,
            light,
            turf_air,
            atmos_blocker,
            blocks_heat,
            icon,
            missing_look,
        }
    }

    /// Prototype components first, then the derived ones, `TilePos` last so the spatial index
    /// hook sees a complete entity.
    pub fn spawn(&self, world: &mut World, pos: Option<TilePos>) -> Entity {
        let mapped = ss13_world::map_init_open(world);
        let mut e = world.spawn_empty();
        if mapped {
            e.insert(ss13_world::Mapped);
        }
        self.proto.insert_into(&mut e);
        let id = self.proto.id();
        if id
            .strip_prefix(crate::CLEANABLE_ROOT)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
        {
            e.insert(crate::DmCleanable {
                path: id.to_owned(),
            });
        }
        if let Some(appearance) = &self.appearance {
            e.insert(appearance.clone());
        }
        if let Some(facing) = self.facing {
            e.insert(facing);
        }
        if self.dense {
            e.insert(Density);
        }
        if let Some(border) = self.border {
            e.insert(border);
        }
        if let Some(air) = &self.turf_air {
            e.insert(air.clone());
        }
        if self.blocks_heat {
            e.insert(BlocksHeat);
        }
        if let Some(blocker) = self.atmos_blocker {
            e.insert(blocker);
        }
        if self.opaque || self.light.is_some() {
            // Before `Opaque`, whose observer would otherwise insert a second time.
            e.insert(Optics {
                opaque: self.opaque,
                tile: None,
                light: self.light,
            });
        }
        if self.opaque {
            e.insert(Opaque);
        }
        match self.category {
            Category::Turf => {
                e.insert(Turf);
            }
            Category::Area => {
                e.insert(Area);
            }
            _ => {}
        }
        if let Some(pos) = pos {
            e.insert(pos);
        }
        e.id()
    }

    fn icon_key(&self) -> String {
        let atom = self.proto.get::<DmAtom>().cloned().unwrap_or_default();
        format!("{}:{:?}", atom.icon, atom.icon_state)
    }
}

/// tg's GAGS map icons (`icons/map_icons/*.dmi`) hold one preview state per type, named by its
/// path. GAGS types draw their rendered icon instead ([`crate::gags`]); a type that keeps a map
/// icon has no config (or no colors), and when its `icon_state` is inherited (or it has no
/// preview, `NO_NEW_GAGS_PREVIEW_1`) it takes its own preview or the nearest ancestor's instead
/// of the `.dmi`'s default state, which in those files is an "ERROR" placeholder.
fn map_icon_preview(
    index: &HashMap<(&str, &str), IconStateId>,
    icon: &str,
    path: &str,
) -> Option<IconStateId> {
    if !icon.starts_with("icons/map_icons/") {
        return None;
    }
    let mut type_path = path;
    loop {
        if let Some(&id) = index.get(&(icon, type_path)) {
            return Some(id);
        }
        type_path = &type_path[..type_path.rfind('/').filter(|&i| i > 0)?];
    }
}

/// BYOND multiplies `color` by `alpha`; the contract has one RGBA.
pub fn fold_color(color: Option<[u8; 4]>, alpha: u8) -> Option<[u8; 4]> {
    match color {
        None if alpha == 255 => None,
        None => Some([255, 255, 255, alpha]),
        Some([r, g, b, a]) => Some([r, g, b, ((u16::from(a) * u16::from(alpha)) / 255) as u8]),
    }
}

/// Area per tile. BYOND has one area object per area type shared by all its tiles, so per-tile
/// area entities would add ~65k entities per level that carry nothing but a pointer;
/// a `u16` per tile is 128 KiB per 255x255 level.
#[derive(Resource, Default)]
pub struct Areas {
    entities: Vec<Entity>,
    levels: Vec<Option<AreaLevel>>,
}

struct AreaLevel {
    width: u32,
    height: u32,
    tiles: Vec<u16>,
}

impl Areas {
    pub fn area_at(&self, pos: TilePos) -> Option<Entity> {
        let level = self.levels.get(usize::from(pos.z))?.as_ref()?;
        let (x, y) = (u32::try_from(pos.x).ok()?, u32::try_from(pos.y).ok()?);
        if x == 0 || y == 0 || x > level.width || y > level.height {
            return None;
        }
        let i = (y - 1) as usize * level.width as usize + (x - 1) as usize;
        Some(self.entities[usize::from(level.tiles[i])])
    }

    /// Every area entity.
    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }

    /// tg `turf.change_area(old, new)`: `pos` now belongs to `area`, an existing area entity.
    /// False (nothing changes) off the map or for an unknown area.
    pub fn set_area(&mut self, pos: TilePos, area: Entity) -> bool {
        let Some(index) = self.entities.iter().position(|&a| a == area) else {
            return false;
        };
        let Some(level) = self
            .levels
            .get_mut(usize::from(pos.z))
            .and_then(Option::as_mut)
        else {
            return false;
        };
        let (Ok(x), Ok(y)) = (u32::try_from(pos.x), u32::try_from(pos.y)) else {
            return false;
        };
        if x == 0 || y == 0 || x > level.width || y > level.height {
            return false;
        }
        level.tiles[(y - 1) as usize * level.width as usize + (x - 1) as usize] = index as u16;
        true
    }

    /// A level created after load (a shuttle transit level): every tile in `area`, which must
    /// be an existing area entity. False for an unknown area.
    pub fn add_level(&mut self, z: u16, width: u32, height: u32, area: Entity) -> bool {
        let Some(index) = self.entities.iter().position(|&a| a == area) else {
            return false;
        };
        let zi = usize::from(z);
        if self.levels.len() <= zi {
            self.levels.resize_with(zi + 1, || None);
        }
        self.levels[zi] = Some(AreaLevel {
            width,
            height,
            tiles: vec![index as u16; width as usize * height as usize],
        });
        true
    }
}

#[derive(Debug)]
pub struct LoadedMap {
    pub path: PathBuf,
    /// Map size `(x, y, z)`; map z `n` is world z `first_z + n - 1`.
    pub size: (u32, u32, u32),
    pub first_z: u16,
}

#[derive(Resource, Debug, Default)]
pub struct MapLoadReport {
    pub maps: Vec<LoadedMap>,
    /// Spawned entities by category (areas: one per area type).
    pub counts: BTreeMap<Category, usize>,
    /// Tiles that got [`DEFAULT_TURF`] because the map gave them no turf.
    pub default_turfs: usize,
    /// Tiles listing more than one turf (deprecated stacking): only the last one is kept.
    pub stacked_turfs: usize,
    /// Distinct (type, var edits) combinations.
    pub templates: usize,
    /// `.dmm` var edits the game has no component for, by var name, per occurrence in the
    /// prefab dictionaries.
    pub unknown_vars: BTreeMap<String, usize>,
    /// Supported vars with a value we cannot represent, by var name.
    pub unsupported_values: BTreeMap<String, usize>,
    /// `icon:"state"` -> spawned entities that are not drawn because of it.
    pub missing_icons: BTreeMap<String, usize>,
    /// Entities drawn with the default state because their state does not exist.
    pub default_state_fallbacks: usize,
    /// `icon:"state"` of round-start looks ([`crate::look`]) the manifest lacks -> entities.
    pub missing_look_sprites: BTreeMap<String, usize>,
    /// Entities with overlays from their round-start look, and of those with emissive ones.
    pub looks: usize,
    pub emissive: usize,
    pub elapsed: Duration,
}

impl MapLoadReport {
    pub fn entities(&self) -> usize {
        self.counts.values().sum()
    }

    /// tg `height_autosetup` (on unless a map config opts out; no station or shuttle map
    /// does): a multi-level map's levels are stacked, map z `n + 1` over `n` (`ZTRAIT_UP`,
    /// `ZTRAIT_DOWN`). Pairs of (below, above) world levels.
    pub fn stacked_levels(&self) -> Vec<(u16, u16)> {
        self.maps
            .iter()
            .flat_map(|m| {
                let levels = m.size.2 as u16;
                (0..levels.saturating_sub(1)).map(move |i| (m.first_z + i, m.first_z + i + 1))
            })
            .collect()
    }
}

impl fmt::Display for MapLoadReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for m in &self.maps {
            writeln!(
                f,
                "map {} {}x{}x{} at z {}",
                m.path.display(),
                m.size.0,
                m.size.1,
                m.size.2,
                m.first_z
            )?;
        }
        writeln!(
            f,
            "{} entities {:?} from {} templates in {:.2?}",
            self.entities(),
            self.counts,
            self.templates,
            self.elapsed
        )?;
        writeln!(
            f,
            "default turfs: {}, stacked turfs: {}, default-state fallbacks: {}",
            self.default_turfs, self.stacked_turfs, self.default_state_fallbacks
        )?;
        let unknown: usize = self.unknown_vars.values().sum();
        writeln!(
            f,
            "ignored var edits: {unknown} unknown {:?}, unsupported values {:?}",
            self.unknown_vars, self.unsupported_values
        )?;
        let missing: usize = self.missing_icons.values().sum();
        writeln!(
            f,
            "undrawable (missing icon state): {missing} entities, {} states",
            self.missing_icons.len()
        )?;
        write!(
            f,
            "round-start looks: {} entities with overlays, {} emissive; sprites missing: {:?}",
            self.looks, self.emissive, self.missing_look_sprites
        )
    }
}

/// What one `.dmm` cell spawns, as indices into the template list.
struct Cell {
    area: u16,
    turf: usize,
    /// The map gave no turf: the tile gets [`DEFAULT_TURF`].
    default_turf: bool,
    movables: Vec<usize>,
}

struct Loader<'a> {
    protos: &'a Prototypes,
    index: HashMap<(&'a str, &'a str), IconStateId>,
    templates: Vec<Template>,
    spawned: Vec<usize>,
    by_key: HashMap<(String, Option<String>), usize>,
    area_by_type: HashMap<String, u16>,
    area_templates: Vec<usize>,
    /// Area entities that existed before this load (an earlier `load_maps` call).
    area_base: usize,
    missing_protos: Vec<String>,
    report: MapLoadReport,
}

impl Loader<'_> {
    fn template(&mut self, path: &str, overrides: Option<String>) -> anyhow::Result<usize> {
        let key = (path.to_owned(), overrides);
        if let Some(&i) = self.by_key.get(&key) {
            return Ok(i);
        }
        let proto = match self.protos.instance(path, key.1.as_deref()) {
            Ok(p) => p,
            Err(e) if self.protos.get(path).is_none() => {
                // Keep planning to list every missing type at once; `load_one` fails before
                // spawning anything from this stand-in.
                self.missing_protos.push(e.to_string());
                self.protos
                    .get(DEFAULT_TURF)
                    .context("default turf prototype missing")?
                    .clone()
            }
            Err(e) => bail!("var edits of {path}: {e}"),
        };
        let i = self.templates.len();
        self.templates.push(Template::new(proto, &self.index));
        self.spawned.push(0);
        self.by_key.insert(key, i);
        Ok(i)
    }

    fn object_template(&mut self, obj: &DmmObject) -> anyhow::Result<usize> {
        let mut fields = Vec::new();
        let mut door_fields = Vec::new();
        let door = crate::door::is_door(&obj.path);
        for (var, value) in &obj.vars {
            if door && let Some(f) = crate::door::field_dmm(var, value) {
                match f {
                    Ok(f) => door_fields.push(f),
                    Err(_) => {
                        *self
                            .report
                            .unsupported_values
                            .entry(var.clone())
                            .or_default() += 1;
                    }
                }
                continue;
            }
            let field = crate::spawner::field_dmm(&obj.path, var, value)
                .or_else(|| crate::contents::field_dmm(&obj.path, var, value))
                .or_else(|| crate::atmos_machine::field_dmm(&obj.path, var, value))
                .or_else(|| crate::silicon::field_dmm(&obj.path, var, value))
                .or_else(|| crate::wallmount::field_dmm(&obj.path, var, value))
                .unwrap_or_else(|| field_for(&obj.path, var, Val::from_dmm(value)));
            match field {
                Ok(f) => fields.push(f),
                Err(Skip::UnknownVar) => {
                    *self.report.unknown_vars.entry(var.clone()).or_default() += 1;
                }
                Err(Skip::Unsupported) => {
                    *self
                        .report
                        .unsupported_values
                        .entry(var.clone())
                        .or_default() += 1;
                }
            }
        }
        let extra: Vec<String> = crate::door::patch_entry(&door_fields).into_iter().collect();
        let overrides =
            (!fields.is_empty() || !extra.is_empty()).then(|| patch_ron_with(&fields, &extra));
        self.template(&obj.path, overrides)
    }

    /// tg keeps one area object per type per map load; var edits of later cells are dropped.
    fn area(&mut self, obj: Option<&DmmObject>) -> anyhow::Result<u16> {
        let obj = obj.filter(|o| o.path != AREA_NOOP);
        let path = obj.map_or(DEFAULT_AREA, |o| o.path.as_str());
        if let Some(&i) = self.area_by_type.get(path) {
            return Ok(i);
        }
        let template = match obj {
            Some(obj) => self.object_template(obj)?,
            None => self.template(DEFAULT_AREA, None)?,
        };
        let i = u16::try_from(self.area_base + self.area_templates.len())
            .context("more than 65536 areas")?;
        self.area_templates.push(template);
        self.area_by_type.insert(path.to_owned(), i);
        Ok(i)
    }

    fn cell(&mut self, objects: &[DmmObject]) -> anyhow::Result<Cell> {
        let mut turfs = Vec::new();
        let mut area = None;
        let mut movables = Vec::new();
        for obj in objects {
            match Category::of(&obj.path) {
                Category::Area => area = Some(obj),
                Category::Turf => turfs.push(obj),
                _ => movables.push(self.object_template(obj)?),
            }
        }
        if turfs.len() > 1 {
            self.report.stacked_turfs += 1;
        }
        let turf = turfs.last().filter(|t| t.path != TURF_NOOP);
        Ok(Cell {
            area: self.area(area)?,
            turf: match turf {
                Some(t) => self.object_template(t)?,
                None => self.template(DEFAULT_TURF, None)?,
            },
            default_turf: turf.is_none(),
            movables,
        })
    }
}

/// Loads `maps` in order onto new z-levels after the highest existing one. The world needs
/// `SpatialIndex` (`WorldPlugin`) and `Prototypes` with the converted DM types.
pub fn load_maps(
    world: &mut World,
    maps: &[PathBuf],
    manifest: &AssetManifest,
) -> anyhow::Result<MapLoadReport> {
    load_maps_with(world, maps, manifest, &|_, _| {})
}

/// [`load_maps`] with `prepare` run on each parsed map before it spawns (terrain generation,
/// [`crate::cave`]).
pub fn load_maps_with(
    world: &mut World,
    maps: &[PathBuf],
    manifest: &AssetManifest,
    prepare: &dyn Fn(&Path, &mut DmmMap),
) -> anyhow::Result<MapLoadReport> {
    let start = Instant::now();
    world.init_resource::<Areas>();
    // What this load spawns is initialized by the pass (`ss13_world::run_map_init`).
    ss13_world::begin_map_init(world, None::<u16>);
    world.resource_scope(|world, protos: Mut<Prototypes>| {
        let mut loader = Loader {
            protos: &protos,
            index: manifest.index(),
            templates: Vec::new(),
            spawned: Vec::new(),
            by_key: HashMap::new(),
            area_by_type: HashMap::new(),
            area_templates: Vec::new(),
            area_base: world.resource::<Areas>().entities.len(),
            missing_protos: Vec::new(),
            report: MapLoadReport::default(),
        };
        for path in maps {
            load_one(world, &mut loader, path, prepare)?;
        }
        let mut report = std::mem::take(&mut loader.report);
        for (t, &n) in loader.templates.iter().zip(&loader.spawned) {
            if n == 0 {
                continue;
            }
            *report.counts.entry(t.category).or_default() += n;
            let overlays = t.appearance.as_ref().map_or(&[][..], |a| &a.overlays[..]);
            if !overlays.is_empty() {
                report.looks += n;
            }
            if overlays
                .iter()
                .any(|o| o.plane == Some(ss13_shared::EMISSIVE_PLANE))
            {
                report.emissive += n;
            }
            for key in &t.missing_look {
                *report.missing_look_sprites.entry(key.clone()).or_default() += n;
            }
            match t.icon {
                IconResolution::Missing => {
                    *report.missing_icons.entry(t.icon_key()).or_default() += n;
                }
                IconResolution::DefaultState => report.default_state_fallbacks += n,
                IconResolution::Exact | IconResolution::NoIcon => {}
            }
        }
        report.templates = loader.templates.len();
        let levels: Vec<u16> = report
            .maps
            .iter()
            .flat_map(|m| (0..m.size.2).map(move |i| m.first_z + i as u16))
            .collect();
        ss13_world::begin_map_init(world, levels.iter().copied());
        light_tiles(world, levels);
        report.elapsed = start.elapsed();
        Ok(report)
    })
}

fn load_one(
    world: &mut World,
    loader: &mut Loader,
    path: &Path,
    prepare: &dyn Fn(&Path, &mut DmmMap),
) -> anyhow::Result<()> {
    let mut map = DmmMap::from_file(path).with_context(|| format!("reading {}", path.display()))?;
    prepare(path, &mut map);
    let (sx, sy, sz) = map.size;
    let mut index = world.resource_mut::<SpatialIndex>();
    let first_z = (1..=u16::MAX)
        .find(|&z| index.level_size(z).is_none())
        .context("no free z-level")?;
    let last_z = u16::try_from(u32::from(first_z) + sz - 1).context("too many z-levels")?;
    for z in first_z..=last_z {
        index.create_level(z, sx, sy)?;
    }

    let cells = map
        .prefabs
        .iter()
        .map(|p| loader.cell(&p.objects))
        .collect::<anyhow::Result<Vec<_>>>()?;
    if !loader.missing_protos.is_empty() {
        loader.missing_protos.sort();
        loader.missing_protos.dedup();
        bail!(
            "{}: types without a prototype (rerun `ss13-mapload convert` with this map):\n  {}",
            path.display(),
            loader.missing_protos.join("\n  ")
        );
    }
    // Cells no grid block covers get world.turf and world.area.
    let empty = if map.grid.contains(&NO_PREFAB) {
        Some(loader.cell(&[])?)
    } else {
        None
    };
    // Areas are spawned once; later maps reuse those already spawned.
    let mut areas = std::mem::take(&mut *world.resource_mut::<Areas>());
    let spawned_areas = areas.entities.len() - loader.area_base;
    for &t in &loader.area_templates[spawned_areas..] {
        areas.entities.push(loader.templates[t].spawn(world, None));
        loader.spawned[t] += 1;
    }

    let mut levels = Vec::with_capacity(sz as usize);
    for mz in 1..=sz {
        let z = first_z + (mz - 1) as u16;
        let mut tiles = vec![0u16; sx as usize * sy as usize];
        for y in 1..=sy {
            for x in 1..=sx {
                let cell = match map.index(x, y, mz) {
                    Some(i) => &cells[i as usize],
                    None => empty.as_ref().expect("the grid has uncovered cells"),
                };
                loader.report.default_turfs += usize::from(cell.default_turf);
                tiles[(y - 1) as usize * sx as usize + (x - 1) as usize] = cell.area;
                let pos = TilePos::new(x as i32, y as i32, z);
                for &t in std::iter::once(&cell.turf).chain(&cell.movables) {
                    loader.templates[t].spawn(world, Some(pos));
                    loader.spawned[t] += 1;
                }
            }
        }
        levels.push((z, tiles));
    }
    for (z, tiles) in levels {
        let zi = usize::from(z);
        if areas.levels.len() <= zi {
            areas.levels.resize_with(zi + 1, || None);
        }
        areas.levels[zi] = Some(AreaLevel {
            width: sx,
            height: sy,
            tiles,
        });
    }
    world.insert_resource(areas);
    loader.report.maps.push(LoadedMap {
        path: path.to_owned(),
        size: map.size,
        first_z,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_icons_fall_back_to_the_nearest_preview_not_the_error_state() {
        const PDA: &str = "icons/map_icons/items/pda.dmi";
        let index = HashMap::from([
            ((PDA, ""), IconStateId(1)),
            ((PDA, "/obj/item/modular_computer/pda/crew"), IconStateId(2)),
            (
                (PDA, "/obj/item/modular_computer/pda/crew/security"),
                IconStateId(3),
            ),
            (("icons/obj/a.dmi", "/obj/item/x"), IconStateId(4)),
        ]);
        let preview = |icon, path| map_icon_preview(&index, icon, path);
        // `NO_NEW_GAGS_PREVIEW_1`: the parent's preview.
        assert_eq!(
            preview(PDA, "/obj/item/modular_computer/pda/crew/assistant"),
            Some(IconStateId(2))
        );
        // Inherited `icon_state`, own preview.
        assert_eq!(
            preview(PDA, "/obj/item/modular_computer/pda/crew/security"),
            Some(IconStateId(3))
        );
        // No ancestor has one: the caller falls back to the default state.
        assert_eq!(preview(PDA, "/obj/item/modular_computer/pda"), None);
        // Only map icon files are named by type path.
        assert_eq!(preview("icons/obj/a.dmi", "/obj/item/x"), None);
    }
}
