//! Light sources and tile lighting from DM data, as tg sets them up at round start
//! (`code/modules/lighting/`, `code/modules/power/lighting/light.dm`).
//!
//! Static vars come through [`DmLight`] (type defaults + `.dmm` edits). Lights tg sets in code
//! follow the rule of the code that sets them, evaluated once per template:
//! - `/obj/machinery/light` (fixtures): `update()` on a powered fixture with a working bulb sets
//!   `light_range = brightness`, `light_power = bulb_power`, `light_color = color || bulb_colour`;
//!   the light points away from the wall (`light_dir = REVERSE_DIR(dir)`, `light_angle = 170`) and
//!   sits half a tile towards the wall (`get_light_offset`). Floor lights: no offset, 360°.
//!   Bulbs are on: `status == LIGHT_OK` and the area is taken as powered.
//!   shortcut: no APC/area power, no nightshift, no 2%/5% roundstart breakage, no fire alarm
//!   colours; add them when power and alarms exist.
//! - `/obj/machinery/computer`: `power_change()` sets `set_light(brightness_on)` when powered.
//! - `/obj/item/flashlight`: `light_on` follows `start_on` (desk lamps start on).
//! - `/obj/machinery/firealarm`: `update_overlays()` colours it by the security level, green at
//!   round start (`fire_alarm_light_color = LIGHT_COLOR_BLUEGREEN`).
//! - Everything else: `light_on && light_range && light_power` from the vars. `set_light()`
//!   raises ranges below `MINIMUM_USEFUL_LIGHT_RANGE`; vars applied by `update_light()` are not.
//!   `OVERLAY_LIGHT` systems (flashlights, PDAs) are drawn as complex lights of the same range.
//! - Space (`space_lit` turfs with a light): starlight is enabled on space next to any tile with
//!   a lighting object (`/atom/movable/lighting_object/Initialize` -> `enable_starlight`).
//!
//! Tiles: a `space_lit` turf or a turf in an area without `static_lighting` has no lighting
//! object (sources do not light it); area base lighting, or starlight on `space_lit` turfs of an
//! area without it, is added as ambient.

use std::collections::HashMap;

use bevy::ecs::entity::EntityHashMap;
use bevy::prelude::*;
use ss13_shared::{Dir, LightSource, Optics, TileLighting, TilePos};
use ss13_world::SpatialIndex;

use crate::fidelity::is_under;
use crate::value::{Field, Skip, Val, parse_color};
use crate::{Areas, Opaque, Turf};

/// tg `MINIMUM_USEFUL_LIGHT_RANGE`.
pub const MINIMUM_USEFUL_LIGHT_RANGE: f32 = 1.4;
/// tg `LIGHT_OK`.
const LIGHT_OK: u8 = 0;
/// tg `LIGHT_IGNORE_OFFSET`.
const LIGHT_IGNORE_OFFSET: u16 = 1 << 2;
const WHITE: [u8; 4] = [255, 255, 255, 255];
/// tg `COLOR_FIRE_LIGHT_RED`, a fixture's `fire_colour`.
const COLOR_FIRE_LIGHT_RED: [u8; 4] = [0xB6, 0x1C, 0x1C, 255];
/// tg `LIGHT_COLOR_BLUEGREEN`, the green security level's fire alarm light.
const LIGHT_COLOR_BLUEGREEN: [u8; 4] = [0x7d, 0xe1, 0xaf, 255];

/// The lighting vars of a DM type (field = DM var name). Prototype data like
/// [`DmAtom`](crate::DmAtom); `Default` is tg's `/atom`, `/area` and `/turf` defaults.
#[derive(Component, Reflect, Clone, Debug, PartialEq)]
pub struct DmLight {
    pub light_range: f32,
    pub light_power: f32,
    /// `None` = `null`, treated as white like tg's `PARSE_LIGHT_COLOR` fallthrough.
    pub light_color: Option<[u8; 4]>,
    pub light_on: bool,
    pub light_angle: f32,
    /// BYOND direction bits.
    pub light_dir: u8,
    pub light_height: f32,
    pub light_flags: u16,
    /// Fixture (`/obj/machinery/light`) vars.
    pub brightness: f32,
    pub bulb_power: f32,
    pub bulb_colour: Option<[u8; 4]>,
    pub status: u8,
    /// Fixtures in an area with a fire alarm (`area.fire`): `fire_brightness`, `fire_power`,
    /// `fire_colour`.
    pub fire_brightness: f32,
    pub fire_power: f32,
    pub fire_colour: Option<[u8; 4]>,
    /// `/obj/machinery/computer`.
    pub brightness_on: f32,
    /// `/obj/item/flashlight`.
    pub start_on: bool,
    /// Area vars.
    pub static_lighting: bool,
    pub base_lighting_alpha: u8,
    pub base_lighting_color: Option<[u8; 4]>,
    /// Turf var.
    pub space_lit: bool,
}

impl Default for DmLight {
    fn default() -> Self {
        Self {
            light_range: 0.0,
            light_power: 1.0,
            light_color: Some(WHITE),
            light_on: true,
            light_angle: 360.0,
            light_dir: Dir::North as u8,
            light_height: 1.0,
            light_flags: 0,
            brightness: 0.0,
            bulb_power: 1.0,
            bulb_colour: Some(WHITE),
            status: LIGHT_OK,
            // `/obj/machinery/light` defaults (only fixtures read them).
            fire_brightness: 9.0,
            fire_power: 0.5,
            fire_colour: Some(COLOR_FIRE_LIGHT_RED),
            brightness_on: 0.0,
            start_on: false,
            static_lighting: true,
            base_lighting_alpha: 0,
            base_lighting_color: Some(WHITE),
            space_lit: false,
        }
    }
}

/// The var names [`field`] understands.
pub const LIGHT_VARS: [&str; 21] = [
    "light_range",
    "light_power",
    "light_color",
    "light_on",
    "light_angle",
    "light_dir",
    "light_height",
    "light_flags",
    "brightness",
    "bulb_power",
    "bulb_colour",
    "status",
    "fire_brightness",
    "fire_power",
    "fire_colour",
    "brightness_on",
    "start_on",
    "static_lighting",
    "base_lighting_alpha",
    "base_lighting_color",
    "space_lit",
];

/// Converts `var = value` for a var of [`LIGHT_VARS`].
pub fn field(var: &str, value: Val) -> Result<Field, Skip> {
    let Some(&var) = LIGHT_VARS.iter().find(|v| **v == var) else {
        return Err(Skip::UnknownVar);
    };
    let num = match value {
        Val::Num(n) => Ok(n),
        Val::Null => Ok(0.0),
        _ => Err(Skip::Unsupported),
    };
    let byte = |n: f64| n.round().clamp(0.0, 255.0) as u8;
    let ron = match var {
        "light_color" | "bulb_colour" | "fire_colour" | "base_lighting_color" => match value {
            Val::Null => "None".to_owned(),
            Val::Str(s) => {
                let [r, g, b, a] = parse_color(s).ok_or(Skip::Unsupported)?;
                format!("Some(({r}, {g}, {b}, {a}))")
            }
            _ => return Err(Skip::Unsupported),
        },
        "light_on" | "start_on" | "static_lighting" | "space_lit" => (num? != 0.0).to_string(),
        "light_dir" | "status" | "base_lighting_alpha" => byte(num?).to_string(),
        "light_flags" => (num?.clamp(0.0, f64::from(u16::MAX)) as u16).to_string(),
        _ => format!("{:?}", num? as f32),
    };
    Ok(crate::fidelity::ext("dm_light", var, ron))
}

fn rgb(color: Option<[u8; 4]>) -> [u8; 3] {
    let [r, g, b, _] = color.unwrap_or(WHITE);
    [r, g, b]
}

/// tg `get_visual_offset` -> `calculate_light_offset`: the source shifts by minus its pixel
/// offset, in 1/32 tile.
fn pixel_offset(light: &DmLight, pixel: [i16; 2]) -> [i32; 2] {
    if light.light_flags & LIGHT_IGNORE_OFFSET != 0 {
        return [0, 0];
    }
    pixel.map(|p| -i32::from(p))
}

/// The light a template of DM type `path` emits at round start (see the module docs).
pub fn source(path: &str, light: &DmLight, dir: u8, pixel: [i16; 2]) -> Option<LightSource> {
    let mut range = light.light_range;
    let mut power = light.light_power;
    let mut color = light.light_color;
    let mut on = light.light_on;
    let mut light_dir = light.light_dir;
    let mut offset = pixel_offset(light, pixel);
    if is_under(path, "/obj/machinery/light") {
        on = light.status == LIGHT_OK;
        range = light.brightness;
        power = light.bulb_power;
        // `color` of the fixture overrides the bulb; DmAtom carries it, the caller folds it in.
        color = light.bulb_colour;
        let reverse = Dir::from_bits(dir).unwrap_or_default().reverse();
        light_dir = reverse as u8;
        if !is_under(path, "/obj/machinery/light/floor") {
            let (dx, dy) = reverse.offset();
            offset[0] += dx * 16;
            offset[1] += dy * 16;
        }
    } else if is_under(path, "/obj/machinery/computer") {
        range = light.brightness_on;
        if range > 0.0 {
            range = range.max(MINIMUM_USEFUL_LIGHT_RANGE);
        }
    } else if is_under(path, "/obj/item/flashlight") {
        on = light.start_on;
    } else if is_under(path, "/obj/machinery/firealarm") {
        color = Some(LIGHT_COLOR_BLUEGREEN);
    }
    if light.space_lit {
        on = false; // Starlight is enabled per tile, see `light_tiles`.
    }
    (on && range > 0.0 && power != 0.0).then(|| LightSource {
        range,
        power,
        color: rgb(color),
        height: light.light_height,
        angle: light.light_angle.round().clamp(0.0, 360.0) as u16,
        dir: Dir::from_bits(light_dir).unwrap_or(Dir::North),
        offset: offset.map(|o| o.clamp(-128, 127) as i8),
    })
}

/// The light a fixture with an atom `color` shows: tg `update()` prefers `color` over the bulb.
pub fn tint_fixture(path: &str, light: &mut LightSource, atom_color: Option<[u8; 4]>) {
    if let (true, Some([r, g, b, _])) = (is_under(path, "/obj/machinery/light"), atom_color) {
        light.color = [r, g, b];
    }
}

/// Tile lighting and starlight for every turf of levels `zs`, after they are spawned.
pub fn light_tiles(world: &mut World, zs: impl IntoIterator<Item = u16>) {
    let mut turfs = world.query_filtered::<(Entity, &TilePos, Option<&DmLight>), With<Turf>>();
    for z in zs {
        let Some((w, h)) = world.resource::<SpatialIndex>().level_size(z) else {
            continue;
        };
        let (w, h) = (w as usize, h as usize);
        let index = |x: i32, y: i32| (y as usize - 1) * w + (x as usize - 1);
        // Per tile: the turf, its lighting, whether it can show starlight.
        let mut tiles: Vec<Option<(Entity, TileLighting, Option<LightSource>)>> = vec![None; w * h];
        let areas = world.resource::<Areas>();
        let area_lights: EntityHashMap<DmLight> = areas
            .entities()
            .iter()
            .map(|&a| (a, world.get::<DmLight>(a).cloned().unwrap_or_default()))
            .collect();
        for (entity, pos, light) in turfs.iter(world) {
            if pos.z != z {
                continue;
            }
            let default = DmLight::default();
            let light = light.unwrap_or(&default);
            let area = areas
                .area_at(*pos)
                .and_then(|a| area_lights.get(&a))
                .unwrap_or(&default);
            let (tile, star) = tile_light(light, area);
            tiles[index(pos.x, pos.y)] = Some((entity, tile, star));
        }
        let dynamic = |x: i32, y: i32| {
            x >= 1
                && y >= 1
                && x as usize <= w
                && y as usize <= h
                && tiles[index(x, y)].is_some_and(|(_, t, _)| t.dynamic())
        };
        let mut updates = Vec::new();
        for y in 1..=h as i32 {
            for x in 1..=w as i32 {
                let Some((entity, tile, star)) = tiles[index(x, y)] else {
                    continue;
                };
                let lit_neighbour =
                    star.is_some() && (-1..=1).any(|dy| (-1..=1).any(|dx| dynamic(x + dx, y + dy)));
                let tile = Some(tile).filter(|t| *t != DEFAULT_TILE);
                let light = star.filter(|_| lit_neighbour);
                if tile.is_some() || light.is_some() {
                    updates.push((entity, tile, light));
                }
            }
        }
        for (entity, tile, light) in updates {
            let mut e = world.entity_mut(entity);
            let mut optics = e.take::<Optics>().unwrap_or_default();
            optics.tile = tile;
            if light.is_some() {
                optics.light = light;
            }
            e.insert(optics);
        }
    }
}

/// [`light_tiles`] again for the turfs in the rectangle `min..=max` of level `z` after its turfs
/// or areas changed (a shuttle moved): their tile lighting from their area, and starlight on
/// `space_lit` turfs next to a lit tile, the ring just outside the rectangle included.
pub fn relight_rect(world: &mut World, z: u16, min: (i32, i32), max: (i32, i32)) {
    let Some((w, h)) = world.resource::<SpatialIndex>().level_size(z) else {
        return;
    };
    // The ring's starlight depends on one more tile beyond it.
    let lo = ((min.0 - 2).max(1), (min.1 - 2).max(1));
    let hi = ((max.0 + 2).min(w as i32), (max.1 + 2).min(h as i32));
    let no_light = DmLight::default();
    let mut tiles: HashMap<TilePos, LitTurf> = HashMap::new();
    {
        let index = world.resource::<SpatialIndex>();
        let areas = world.resource::<Areas>();
        for y in lo.1..=hi.1 {
            for x in lo.0..=hi.0 {
                let pos = TilePos::new(x, y, z);
                let Some(&turf) = index
                    .on_tile(pos)
                    .iter()
                    .find(|&&e| world.get::<Turf>(e).is_some())
                else {
                    continue;
                };
                let light = world.get::<DmLight>(turf).unwrap_or(&no_light);
                let area = areas
                    .area_at(pos)
                    .and_then(|a| world.get::<DmLight>(a))
                    .unwrap_or(&no_light);
                let (tile, star) = tile_light(light, area);
                tiles.insert(
                    pos,
                    LitTurf {
                        entity: turf,
                        tile,
                        star,
                        space_lit: light.space_lit,
                    },
                );
            }
        }
    }
    let dynamic = |pos: TilePos| tiles.get(&pos).is_some_and(|t| t.tile.dynamic());
    let mut updates = Vec::new();
    for (&pos, t) in &tiles {
        let inside =
            (min.0 - 1..=max.0 + 1).contains(&pos.x) && (min.1 - 1..=max.1 + 1).contains(&pos.y);
        if !inside {
            continue;
        }
        let lit_neighbour = t.star.is_some()
            && (-1..=1)
                .any(|dy| (-1..=1).any(|dx| dynamic(TilePos::new(pos.x + dx, pos.y + dy, z))));
        let tile = Some(t.tile).filter(|t| *t != DEFAULT_TILE);
        updates.push((
            t.entity,
            tile,
            t.star.filter(|_| lit_neighbour),
            t.space_lit,
        ));
    }
    for (entity, tile, light, space_lit) in updates {
        let Some(optics) = world.get::<Optics>(entity) else {
            if tile.is_some() || light.is_some() {
                world.entity_mut(entity).insert(Optics {
                    tile,
                    light,
                    ..default()
                });
            }
            continue;
        };
        // Only starlight is recomputed: other turf lights come from the turf's own vars.
        let new_light = if space_lit { light } else { optics.light };
        if optics.tile != tile || optics.light != new_light {
            let mut optics = world.get_mut::<Optics>(entity).expect("checked above");
            optics.tile = tile;
            optics.light = new_light;
        }
    }
}

struct LitTurf {
    entity: Entity,
    tile: TileLighting,
    star: Option<LightSource>,
    space_lit: bool,
}

/// A turf's tile lighting in its area and the starlight it shows next to lit tiles.
fn tile_light(light: &DmLight, area: &DmLight) -> (TileLighting, Option<LightSource>) {
    let base = (area.base_lighting_alpha > 0)
        .then_some(area.base_lighting_color)
        .flatten()
        .map(|[r, g, b, _]| [r, g, b, area.base_lighting_alpha]);
    let starlight = light.space_lit.then(|| {
        let [r, g, b] = rgb(light.light_color);
        [r, g, b, 255]
    });
    let tile = TileLighting::new(
        area.static_lighting && !light.space_lit,
        base.or(starlight).unwrap_or([0; 4]),
    );
    let star =
        (light.space_lit && light.light_range > 0.0 && light.light_power != 0.0).then(|| {
            LightSource {
                range: light.light_range,
                power: light.light_power,
                color: rgb(light.light_color),
                height: light.light_height,
                angle: 360,
                dir: Dir::North,
                offset: [0, 0],
            }
        });
    (tile, star)
}

/// A tile tg lights with its lighting object only: the meaning of `Optics::tile == None`.
const DEFAULT_TILE: TileLighting = TileLighting::Area {
    dynamic: true,
    ambient: [0; 4],
};

/// Keeps `Optics::opaque` equal to the presence of [`Opaque`], so systems that open doors only
/// toggle `Opaque`. Observers, so the change is replicated in the tick it happens and idle
/// ticks cost nothing.
pub struct OpticsPlugin;

impl Plugin for OpticsPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(opaque_added).add_observer(opaque_removed);
    }
}

fn opaque_added(add: On<Add<Opaque>>, mut optics: Query<&mut Optics>, mut commands: Commands) {
    match optics.get_mut(add.entity) {
        Ok(mut o) => {
            if !o.opaque {
                o.opaque = true;
            }
        }
        Err(_) => {
            commands.entity(add.entity).try_insert(Optics {
                opaque: true,
                ..default()
            });
        }
    }
}

fn opaque_removed(remove: On<Remove<Opaque>>, mut optics: Query<&mut Optics>) {
    if let Ok(mut o) = optics.get_mut(remove.entity)
        && o.opaque
    {
        o.opaque = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> DmLight {
        // `/obj/machinery/light` as BandaStation declares it.
        DmLight {
            brightness: 8.0,
            bulb_power: 0.8,
            bulb_colour: Some([255, 247, 242, 255]),
            light_angle: 170.0,
            ..default()
        }
    }

    #[test]
    fn wall_fixture_points_away_from_its_wall() {
        // Mounted on the north wall: `dir = NORTH`, light towards the south, shifted north.
        let l = source(
            "/obj/machinery/light/directional/north",
            &fixture(),
            1,
            [0, 0],
        )
        .unwrap();
        assert_eq!((l.range, l.power, l.color), (8.0, 0.8, [255, 247, 242]));
        assert_eq!((l.angle, l.dir, l.offset), (170, Dir::South, [0, -16]));
        let floor = DmLight {
            brightness: 4.0,
            light_angle: 360.0,
            ..fixture()
        };
        let l = source("/obj/machinery/light/floor", &floor, 2, [0, 0]).unwrap();
        assert_eq!((l.range, l.angle, l.offset), (4.0, 360, [0, 0]));
        let broken = DmLight {
            status: 2,
            ..fixture()
        };
        assert_eq!(
            source("/obj/machinery/light/broken", &broken, 1, [0, 0]),
            None
        );
        // A light switch is not a fixture.
        assert_eq!(
            source("/obj/machinery/light_switch", &fixture(), 1, [0, 0]),
            None
        );
    }

    #[test]
    fn code_set_lights() {
        let console = DmLight {
            brightness_on: 1.0,
            light_color: Some([100, 150, 250, 255]),
            ..default()
        };
        let l = source("/obj/machinery/computer/crew", &console, 2, [0, 0]).unwrap();
        assert_eq!(
            (l.range, l.color),
            (MINIMUM_USEFUL_LIGHT_RANGE, [100, 150, 250])
        );
        let lamp = DmLight {
            light_range: 3.5,
            light_on: true,
            start_on: true,
            ..default()
        };
        assert!(source("/obj/item/flashlight/lamp", &lamp, 2, [0, 0]).is_some());
        let off = DmLight {
            start_on: false,
            ..lamp.clone()
        };
        assert_eq!(source("/obj/item/flashlight", &off, 2, [0, 0]), None);
        // Static vars, shifted by the pixel offset (tg moves the light with the sprite).
        let glow = DmLight {
            light_range: 2.0,
            ..default()
        };
        let l = source("/obj/structure/sign", &glow, 2, [0, 32]).unwrap();
        assert_eq!(l.offset, [0, -32]);
        assert_eq!(source("/obj/structure/sign", &default(), 2, [0, 0]), None);
    }

    #[test]
    fn vars_convert() {
        let ron = |f: Field| match f {
            Field::Ext {
                component: "dm_light",
                ron,
                ..
            } => ron,
            other => panic!("{other:?}"),
        };
        assert_eq!(ron(field("light_range", Val::Num(2.5)).unwrap()), "2.5");
        assert_eq!(ron(field("light_on", Val::Num(0.0)).unwrap()), "false");
        assert_eq!(
            ron(field("bulb_colour", Val::Str("#ff0000")).unwrap()),
            "Some((255, 0, 0, 255))"
        );
        assert_eq!(field("status", Val::Str("x")), Err(Skip::Unsupported));
        assert_eq!(field("id", Val::Num(1.0)), Err(Skip::UnknownVar));
    }
}
