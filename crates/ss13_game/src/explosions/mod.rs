//! Explosions (server), after tg `SSexplosions` (`code/controllers/subsystem/explosions.dm`):
//! `explosion()` -> `explode()` -> `propagate_blastwave()`, `dyn_explosion()`, the subsystem's
//! `fire()` and `shake_the_room()`.
//!
//! - [`explosion`] caps the ranges by the bomb cap ([`ExplosionConfig`], tg `bombcap`, unless
//!   `ignorecap`), plays the sounds and shakes the cameras of the z-level's players, flashes the
//!   living mobs that see the epicenter (`flash_range`, default the devastation range), then walks
//!   the tiles out to the largest range ([`wave::prepare_turfs`], optionally a directional arc)
//!   and gives each a severity by its distance plus the resistance it sits behind
//!   (`reactionary_explosions`: the walls', doors' and windows' `explosive_resistance` summed
//!   along the steps towards the epicenter). Tiles within `flame_range` get a hotspot 40% of the
//!   time; everything inside things at the epicenter meets the blast too.
//! - The tiles wait in [`ExplosionQueue`] for the next tick, as tg's lists wait for the
//!   subsystem: then turfs (light, heavy, devastating; each with what is on it queued first,
//!   `EX_ACT`), the flame tiles, the movables (devastating first) and finally the throws: loose
//!   items on blasted tiles fly away from the epicenter (`explosion_throw_details`). What the
//!   blasts queue on the way (a gibbed body's things, a storage's contents) waits for the
//!   next tick.
//! - [`ex_act`]: what each atom does; [`sources`]: fuel tanks, gas tanks, C-4.
//!
//! Cost: one explosion walks `(2r + 1)^2` tiles once (the cap, r = 14, is 841 tiles: a few
//! entity lookups each), then every entity on its blasted tiles once; nothing runs while
//! nothing explodes (one `is_empty` check per tick).
// shortcut: no explosion smoke or particles (`/datum/effect_system/explosion`), no doppler
// arrays (`COMSIG_GLOB_EXPLOSION`), no z-level bomb cap multipliers, no `COMSIG_ATOM_EXPLODE`
// cancellation (nothing here cancels an explosion); thrown mobs and structures are not thrown
// (items are); a flame tile only lights a fire where the air burns (`hotspot_expose`), tg's
// short-lived ambient hotspot in plain air is not drawn.

pub mod ex_act;
pub mod sources;
pub mod wave;

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_mapload::construct::DmTurfBuild;
use ss13_mapload::{DmItem, DmObj};
use ss13_net::camera::server::SendCameraEffect;
use ss13_net::sound::server::SendSound;
use ss13_shared::TilePos;
use ss13_shared::protocol::{CameraEffect, SoundEvent};
use ss13_world::{Density, SpatialIndex};

use crate::combat::CombatRng;
use crate::construction::{path_of, turf_at};
use crate::obj_damage::is_under;
use crate::server::PlayerMob;

pub use wave::Severity;
use wave::{BombCap, Ranges};

/// tg `bombcap` (`config/game_options.txt`, default 14; BandaStation's config sets 20) and
/// `reactionary_explosions` (on in BandaStation). `SS13_BOMBCAP` overrides the cap.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct ExplosionConfig {
    pub bombcap: f32,
    pub reactionary: bool,
}

impl Default for ExplosionConfig {
    fn default() -> Self {
        let bombcap = std::env::var("SS13_BOMBCAP")
            .ok()
            .and_then(|v| v.trim().parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(14.0)
            .max(4.0);
        Self {
            bombcap,
            reactionary: true,
        }
    }
}

/// The arguments of tg `explosion()` after `origin`.
#[derive(Clone, Debug, PartialEq)]
pub struct Explosion {
    pub devastation: f32,
    pub heavy: f32,
    pub light: f32,
    /// `None`: the light range.
    pub flame: Option<f32>,
    /// `None`: the devastation range.
    pub flash: Option<f32>,
    pub ignorecap: bool,
    pub silent: bool,
    pub protect_epicenter: bool,
    /// Degrees clockwise from north; an `arc` of 360 is no direction.
    pub direction: f32,
    pub arc: f32,
    /// For the log (tg `explosion_cause`).
    pub cause: String,
}

impl Explosion {
    pub fn new(devastation: i32, heavy: i32, light: i32) -> Self {
        Self {
            devastation: devastation as f32,
            heavy: heavy as f32,
            light: light as f32,
            flame: None,
            flash: None,
            ignorecap: false,
            silent: false,
            protect_epicenter: false,
            direction: 0.0,
            arc: 360.0,
            cause: String::new(),
        }
    }

    pub fn flame(mut self, flame: i32) -> Self {
        self.flame = Some(flame as f32);
        self
    }

    pub fn flash(mut self, flash: i32) -> Self {
        self.flash = Some(flash as f32);
        self
    }

    pub fn ignorecap(mut self) -> Self {
        self.ignorecap = true;
        self
    }

    pub fn directional(mut self, direction: f32, arc: f32) -> Self {
        self.direction = direction;
        self.arc = arc;
        self
    }

    pub fn cause(mut self, cause: &str) -> Self {
        self.cause = cause.to_owned();
        self
    }
}

/// tg `explosion_throw_details` of a tile: `(range, center, max range, severity)`.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ThrowDetails {
    range: f32,
    center: TilePos,
    max_range: f32,
    severity: Option<Severity>,
}

/// tg's `SSexplosions` lists: what the next `fire()` blasts.
#[derive(Resource, Default, Debug)]
pub struct ExplosionQueue {
    turfs: [Vec<TilePos>; 3],
    flame: Vec<TilePos>,
    movables: [Vec<Entity>; 3],
    throws: Vec<TilePos>,
    throw_details: HashMap<TilePos, ThrowDetails>,
    /// Hull creaking due (`CREAK_DELAY`): game time, client, volume, speed.
    creaks: Vec<(f64, Entity, f32, f32)>,
    /// tg `explosion_index`.
    pub count: u32,
}

impl ExplosionQueue {
    /// The severity queued for the turf of `pos` (tg's check of which list holds it).
    pub fn queued(&self, pos: TilePos) -> Option<Severity> {
        [Severity::Devastate, Severity::Heavy, Severity::Light]
            .into_iter()
            .find(|&s| self.turfs[slot(s)].contains(&pos))
    }

    /// tg `is_exploding()`.
    pub fn is_exploding(&self) -> bool {
        self.turfs.iter().any(|t| !t.is_empty())
            || !self.flame.is_empty()
            || self.movables.iter().any(|m| !m.is_empty())
            || !self.throws.is_empty()
    }
}

fn slot(severity: Severity) -> usize {
    severity as usize - 1
}

pub struct ExplosionsServerPlugin;

impl Plugin for ExplosionsServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ExplosionConfig>()
            .init_resource::<ExplosionQueue>()
            .add_systems(
                Update,
                (
                    (sources::detonate_charges, fire).chain(),
                    sources::c4_prompt_answers
                        .in_set(ss13_tgui::server::UiSystems::Handle)
                        .after(ss13_tgui::prompt::PromptSystems::Answer),
                ),
            );
    }
}

/// Queues `things` for the blast of `severity` on the subsystem's next round.
pub fn queue_movables(world: &mut World, things: &[Entity], severity: Severity) {
    if let Some(mut q) = world.get_resource_mut::<ExplosionQueue>() {
        q.movables[slot(severity)].extend_from_slice(things);
    }
}

/// tg `dyn_explosion(epicenter, power, flame_range, flash_range, ignorecap)`: ranges from one
/// power ([`wave::dyn_ranges`]); `flame` and `flash` are multiples of the light range (`None`:
/// the defaults of `explosion()`).
pub fn dyn_explosion(
    world: &mut World,
    epicenter: TilePos,
    power: f32,
    flame: Option<f32>,
    flash: Option<f32>,
    ignorecap: bool,
) {
    if power == 0.0 || !power.is_finite() {
        return;
    }
    let (r, range) = wave::dyn_ranges(power);
    let ex = Explosion {
        devastation: r.devastation,
        heavy: r.heavy,
        light: r.light,
        flame: flame.map(|f| f * range),
        flash: flash.map(|f| f * range),
        ignorecap,
        ..Explosion::new(0, 0, 0)
    };
    explosion(world, epicenter, ex);
}

/// The block of one tile (tg `explosive_resistance`): its turf's, plus the `explosion_block` of
/// what stands on it (doors only while closed, windows only reinforced full-tile ones).
fn resistance(world: &World, pos: TilePos) -> f32 {
    let mut block = 0.0;
    for &e in world.resource::<SpatialIndex>().on_tile(pos) {
        if let Some(turf) = world.get::<DmTurfBuild>(e) {
            block += turf.explosive_resistance;
            continue;
        }
        let Some(obj) = world.get::<DmObj>(e) else {
            continue;
        };
        if obj.explosion_block == 0.0 {
            continue;
        }
        // `/obj/machinery/door/set_explosion_block`: "doors only block while dense".
        if is_under(&obj.path, "/obj/machinery/door") && world.get::<Density>(e).is_none() {
            continue;
        }
        // `/obj/structure/window/Initialize`: "windows only block while reinforced and
        // fulltile".
        if is_under(&obj.path, crate::obj_damage::WINDOW) && !(obj.reinf && obj.fulltile) {
            continue;
        }
        block += obj.explosion_block;
    }
    block
}

/// Everything inside `thing`, recursively (tg `get_all_contents` without `thing`): a storage's
/// or machine's contents, a mob's worn and held items.
fn all_contents(world: &World, thing: Entity, out: &mut Vec<Entity>) {
    let mut direct: Vec<Entity> = world
        .get::<crate::storage::Stored>(thing)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    if let Some(inv) = world.get::<crate::inventory::Inventory>(thing) {
        direct.extend(
            crate::inventory::ALL_SLOTS
                .iter()
                .filter_map(|&s| inv.slot(s)),
        );
    }
    for e in direct {
        if out.len() > 10_000 || out.contains(&e) {
            continue;
        }
        out.push(e);
        all_contents(world, e, out);
    }
}

/// tg `explosion()` -> `explode()` -> `propagate_blastwave()` at `epicenter`.
pub fn explosion(world: &mut World, epicenter: TilePos, ex: Explosion) {
    let Some((w, h)) = world.resource::<SpatialIndex>().level_size(epicenter.z) else {
        return;
    };
    let config = world
        .get_resource::<ExplosionConfig>()
        .copied()
        .unwrap_or_default();
    let mut dev = ex.devastation;
    let mut heavy = ex.heavy;
    let mut light = ex.light;
    let mut flame = ex.flame.unwrap_or(light);
    let mut flash = ex.flash.unwrap_or(dev);
    let orig_max = dev.max(heavy).max(light).max(flame).max(flash);
    if !ex.ignorecap {
        let cap = BombCap::new(config.bombcap);
        dev = dev.min(cap.devastation);
        heavy = heavy.min(cap.heavy);
        light = light.min(cap.light);
        flash = flash.min(cap.flash);
        flame = flame.min(cap.flame);
    }
    let max_range = dev.max(heavy).max(light).max(flame);
    info!(
        "Explosion with size ({dev}, {heavy}, {light}, {flame}, {flash}) at {epicenter:?}. \
         Possible cause: {}.",
        ex.cause
    );
    let far_dist = heavy * 15.0 + dev * 20.0;
    if !ex.silent {
        shake_the_room(world, epicenter, orig_max, far_dist, dev, heavy);
    }
    if flash > 0.0 {
        flash_viewers(world, epicenter, flash);
    }
    let ranges = Ranges {
        devastation: dev,
        heavy,
        light,
    };
    // `for(var/i in 1 to range)` with a fractional range stops at its floor.
    let tiles = wave::prepare_turfs(
        max_range.max(0.0).floor() as i32,
        epicenter,
        ex.protect_epicenter,
        ex.direction,
        ex.arc,
        w as i32,
        h as i32,
    );
    let mut cached_block: HashMap<TilePos, f32> = HashMap::with_capacity(tiles.len());
    for pos in tiles {
        let dist = wave::hypotenuse(pos.x - epicenter.x, pos.y - epicenter.y);
        let mut block = 0.0;
        if config.reactionary {
            let own = resistance(world, pos);
            if pos == epicenter {
                cached_block.insert(pos, own);
            } else {
                // Tiles come ring by ring, so the step towards the epicenter is cached (or
                // outside the arc: no block from there).
                let before = cached_block
                    .get(&wave::step_towards(pos, epicenter))
                    .copied()
                    .unwrap_or(0.0);
                block = before;
                cached_block.insert(pos, before + own);
            }
        }
        let severity = wave::severity(dist, block, ranges);
        if pos == epicenter
            && let Some(severity) = severity
        {
            // "Ensures explosives detonating from bags trigger other explosives in that bag":
            // what is inside things here, beyond what `contents_explosion` reaches.
            let mut inside = Vec::new();
            for holder in ex_act::tile_contents(world, pos) {
                let mut nested = Vec::new();
                all_contents(world, holder, &mut nested);
                let direct = ex_act::contents_of(world, holder);
                inside.extend(nested.into_iter().filter(|e| !direct.contains(e)));
            }
            queue_movables(world, &inside, severity);
        }
        if let Some(severity) = severity {
            world.resource_mut::<ExplosionQueue>().turfs[slot(severity)].push(pos);
        }
        let flame_roll = dist < flame && world.resource_mut::<CombatRng>().chance(40.0);
        if flame_roll && open_non_space(world, pos) {
            world.resource_mut::<ExplosionQueue>().flame.push(pos);
        }
        let mut q = world.resource_mut::<ExplosionQueue>();
        let details = ThrowDetails {
            range: max_range - dist,
            center: epicenter,
            max_range,
            severity,
        };
        match q.throw_details.get_mut(&pos) {
            Some(d) => {
                if d.range < details.range {
                    *d = details;
                }
            }
            None => {
                q.throw_details.insert(pos, details);
                q.throws.push(pos);
            }
        }
    }
    world.resource_mut::<ExplosionQueue>().count += 1;
}

/// The tile of `e`, or of what carries or holds it.
fn turf_of(world: &World, mut e: Entity) -> Option<TilePos> {
    for _ in 0..8 {
        if let Some(&pos) = world.get::<TilePos>(e) {
            return Some(pos);
        }
        e = world
            .get::<crate::inventory::CarriedBy>(e)
            .map(|c| c.0)
            .or_else(|| world.get::<crate::storage::StoredIn>(e).map(|s| s.0))?;
    }
    None
}

/// A file for `sound` (an SFX key picks one of its files).
fn pick_file(world: &mut World, sound: &str) -> String {
    world.resource_scope(|world, sfx: Mut<crate::sound::Sfx>| {
        let mut rng = world.resource_mut::<crate::sound::SoundRng>();
        crate::sound::pick_sfx(&sfx, &mut rng.0, sound)
    })
}

/// `!isspaceturf(T) && !T.density`.
fn open_non_space(world: &World, pos: TilePos) -> bool {
    turf_at(world, pos)
        .filter(|&t| world.get::<Density>(t).is_none())
        .and_then(|t| path_of(world, t))
        .is_some_and(|p| is_under(&p, "/turf/open") && !is_under(&p, "/turf/open/space"))
}

/// `for(var/mob/living/L in viewers(flash_range, epicenter)) L.flash_act()`.
fn flash_viewers(world: &mut World, epicenter: TilePos, range: f32) {
    let seen = {
        let index = world.resource::<SpatialIndex>();
        let opaque = |p: TilePos| {
            index
                .on_tile(p)
                .iter()
                .any(|&e| world.get::<ss13_mapload::Opaque>(e).is_some())
        };
        crate::sound::hearing_view(epicenter, range, opaque, |p| index.in_bounds(p))
    };
    let mobs: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(epicenter, range.ceil() as u32)
        .filter(|&e| {
            world.get::<crate::health::Health>(e).is_some()
                && world.get::<crate::closets::InCloset>(e).is_none()
                && world.get::<TilePos>(e).is_some_and(|p| seen.contains(p))
        })
        .collect();
    for mob in mobs {
        // `flash_act()` defaults: intensity 1, `length = 25`.
        crate::vision::flash_act(world, mob, 1, false, false, 2.5);
    }
}

/// `QUAKE_CREAK_PROB`, `ECHO_CREAK_PROB`, `CREAK_DELAY` (s), `FAR_LOWER`, `FAR_UPPER`,
/// `FAR_SOUND_PROB`, `NEAR_SHAKE_CAP`, `FAR_SHAKE_CAP`, `NEAR_SHAKE_DURATION` and
/// `FAR_SHAKE_DURATION` (ds), `FREQ_LOWER`, `FREQ_UPPER`; BYOND `world.view` (7).
const QUAKE_CREAK_PROB: f32 = 30.0;
const ECHO_CREAK_PROB: f32 = 5.0;
const CREAK_DELAY: f64 = 5.0;
const FAR_LOWER: f32 = 40.0;
const FAR_UPPER: f32 = 60.0;
const FAR_SOUND_PROB: f32 = 75.0;
const NEAR_SHAKE_CAP: f32 = 5.0;
const FAR_SHAKE_CAP: f32 = 1.5;
const NEAR_SHAKE_DURATION: f32 = 15.0;
const FAR_SHAKE_DURATION: f32 = 10.0;
const FREQ_LOWER: u32 = 25;
const FREQ_UPPER: u32 = 40;
const WORLD_VIEW: f32 = 7.0;
const SFX_EXPLOSION: &str = "explosion";
const SFX_EXPLOSION_CREAKING: &str = "explosion_creaking";
const SFX_HULL_CREAKING: &str = "hull_creaking";
const FAR_SOUND: &str = "sound/effects/explosion/explosionfar.ogg";
const ECHO_SOUND: &str = "sound/effects/explosion/explosion_distant.ogg";

/// What one listener gets from [`shake_the_room`]: `(sound, volume, positional)` and
/// `(shake deciseconds, strength)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Rumble {
    pub sound: Option<(&'static str, f32, bool)>,
    pub shake: Option<(f32, f32)>,
}

/// tg `shake_the_room` for one listener `distance` tiles away (`get_dist`); `far_roll` is
/// `prob(FAR_SOUND_PROB)`, `echoes` whether the hull carries an echo to it (not in space).
pub fn rumble(
    distance: f32,
    near_distance: f32,
    far_distance: f32,
    quake_factor: f32,
    echo_factor: f32,
    creaking: bool,
    far_roll: bool,
    echoes: bool,
) -> Rumble {
    let base_shake = (near_distance / (distance + 1.0)).sqrt();
    if distance <= (near_distance + WORLD_VIEW - 2.0).round() {
        return Rumble {
            sound: Some((SFX_EXPLOSION, 100.0, true)),
            shake: (base_shake > 0.0)
                .then_some((NEAR_SHAKE_DURATION, base_shake.clamp(0.0, NEAR_SHAKE_CAP))),
        };
    }
    if distance < far_distance {
        let volume = (far_distance / 2.0).clamp(FAR_LOWER, FAR_UPPER);
        let sound = if creaking {
            SFX_EXPLOSION_CREAKING
        } else if far_roll {
            FAR_SOUND
        } else {
            ECHO_SOUND
        };
        let shake = (base_shake > 0.0 || quake_factor > 0.0).then(|| {
            let amount = base_shake.max(quake_factor * 3.0).max(0.0);
            (FAR_SHAKE_DURATION, amount.min(FAR_SHAKE_CAP))
        });
        return Rumble {
            sound: Some((sound, volume, false)),
            shake,
        };
    }
    if echoes && echo_factor > 0.0 {
        let (volume, shake) = if quake_factor > 0.0 {
            (
                60.0,
                Some((
                    FAR_SHAKE_DURATION,
                    (quake_factor / 4.0).clamp(0.0, FAR_SHAKE_CAP),
                )),
            )
        } else {
            (40.0, None)
        };
        return Rumble {
            sound: Some((ECHO_SOUND, volume, false)),
            shake,
        };
    }
    Rumble {
        sound: None,
        shake: None,
    }
}

/// tg `shake_the_room(epicenter, near_distance, far_distance, quake_factor, echo_factor)`:
/// every player on the z-level hears the blast by distance (near, far or creaking, or the
/// hull's echo) and has its view shaken; on the station a big enough blast makes the hull
/// creak five seconds later.
// shortcut: the planetary-map exception of the echo is moot (no planetary maps here).
fn shake_the_room(
    world: &mut World,
    epicenter: TilePos,
    near_distance: f32,
    far_distance: f32,
    quake_factor: f32,
    echo_factor: f32,
) {
    let (frequency, creaking) = {
        let on_station = crate::events::controller::station_levels(world).contains(&epicenter.z);
        let mut rng = world.resource_mut::<CombatRng>();
        let frequency = rng.range(32000, 55000) as f32;
        let creaking = on_station
            && rng.chance(quake_factor * QUAKE_CREAK_PROB + echo_factor * ECHO_CREAK_PROB);
        (frequency, creaking)
    };
    let speed = crate::sound::speed_of(frequency);
    let listeners: Vec<(Entity, Entity)> = world
        .query::<(Entity, &PlayerMob)>()
        .iter(world)
        .map(|(mob, p)| (mob, p.client))
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for (mob, client) in listeners {
        let Some(at) = turf_of(world, mob) else {
            continue;
        };
        if at.z != epicenter.z {
            continue;
        }
        let distance = (at.x - epicenter.x).abs().max((at.y - epicenter.y).abs()) as f32;
        let in_space = turf_at(world, at)
            .and_then(|t| path_of(world, t))
            .is_some_and(|p| is_under(&p, "/turf/open/space"));
        let far_roll = world.resource_mut::<CombatRng>().chance(FAR_SOUND_PROB);
        let r = rumble(
            distance,
            near_distance,
            far_distance,
            quake_factor,
            echo_factor,
            creaking,
            far_roll,
            !in_space,
        );
        let hears = crate::sound::can_hear(world, mob);
        if let Some((sound, volume, positional)) = r.sound
            && hears
        {
            send_sound(
                world, client, at, epicenter, sound, volume, speed, positional,
            );
        }
        if let Some((duration_ds, strength)) = r.shake {
            world.write_message(SendCameraEffect {
                client,
                effect: CameraEffect::Shake {
                    duration_ds,
                    strength,
                },
            });
        }
        if creaking {
            let volume = world
                .resource_mut::<CombatRng>()
                .range(FREQ_LOWER, FREQ_UPPER) as f32;
            world.resource_mut::<ExplosionQueue>().creaks.push((
                now + CREAK_DELAY,
                client,
                volume,
                speed,
            ));
        }
    }
}

/// `playsound_local(epicenter, ...)` to one client: an SFX key picked, the pressure of the two
/// tiles applied (`pressure_affected`), positional only for the near blast
/// (`distance_multiplier = 0` for the far ones).
#[allow(clippy::too_many_arguments)]
fn send_sound(
    world: &mut World,
    client: Entity,
    at: TilePos,
    epicenter: TilePos,
    sound: &str,
    volume: f32,
    speed: f32,
    positional: bool,
) {
    let file = pick_file(world, sound);
    let (dx, dy) = (epicenter.x - at.x, epicenter.y - at.y);
    let pressure = |pos| match world.get_resource::<ss13_atmos::Atmos>() {
        Some(a) => a.pressure(pos),
        None => Some(crate::sound::ONE_ATMOSPHERE),
    };
    let distance = ((dx * dx + dy * dy) as f32).sqrt();
    let volume =
        volume * crate::sound::pressure_factor(pressure(at), pressure(epicenter), distance);
    if volume <= 0.0 {
        return;
    }
    world.write_message(SendSound {
        client,
        sound: SoundEvent {
            file,
            volume,
            speed,
            offset: positional.then_some([dx as i16, dy as i16]),
        },
    });
}

/// tg `SSexplosions.fire()`: turfs, then movables, then throws; and the hull creaking due.
// Cost: one `is_exploding` check per tick when nothing explodes.
pub fn fire(world: &mut World) {
    deliver_creaks(world);
    if !world.resource::<ExplosionQueue>().is_exploding() {
        return;
    }
    let started = std::time::Instant::now();
    let (turfs, flame) = {
        let mut q = world.resource_mut::<ExplosionQueue>();
        (std::mem::take(&mut q.turfs), std::mem::take(&mut q.flame))
    };
    let mut blasted = 0usize;
    for severity in [Severity::Light, Severity::Heavy, Severity::Devastate] {
        for &pos in &turfs[slot(severity)] {
            // `EX_ACT(turf)`: `contents_explosion` queues what is on it, then `ex_act`.
            let things = ex_act::tile_contents(world, pos);
            world.resource_mut::<ExplosionQueue>().movables[slot(severity)].extend(things);
            ex_act::ex_act_turf(world, pos, severity);
            blasted += 1;
        }
    }
    for pos in flame {
        if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
            atmos.hotspot_expose(pos, 1000.0, 2500.0, false);
        }
    }
    let turfs_done = started.elapsed();
    let movables = std::mem::take(&mut world.resource_mut::<ExplosionQueue>().movables);
    for severity in [Severity::Devastate, Severity::Heavy, Severity::Light] {
        for &thing in &movables[slot(severity)] {
            if world.get_entity(thing).is_err() {
                continue;
            }
            // `EX_ACT(movable)`: its contents first, then itself.
            let inside = ex_act::contents_of(world, thing);
            queue_movables(world, &inside, severity);
            ex_act::ex_act_movable(world, thing, severity);
            blasted += 1;
        }
    }
    let movables_done = started.elapsed();
    let (throws, details) = {
        let mut q = world.resource_mut::<ExplosionQueue>();
        (
            std::mem::take(&mut q.throws),
            std::mem::take(&mut q.throw_details),
        )
    };
    for pos in throws {
        if let Some(d) = details.get(&pos) {
            throw_from(world, pos, *d);
        }
    }
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    if ms > 5.0 {
        // tg's `stat_entry` costs: turfs, movables, throws.
        info!(
            "SSexplosions fire: {blasted} atoms in {ms:.1} ms (turfs {:.1}, movables {:.1})",
            turfs_done.as_secs_f64() * 1000.0,
            (movables_done - turfs_done).as_secs_f64() * 1000.0,
        );
    }
}

/// `MOVE_FORCE_*` an explosion of each severity overcomes (`EXPLODE_DEVASTATE` -> overpowering,
/// heavy -> very strong, light -> strong) against `MOVE_RESIST_DEFAULT` of loose items: every
/// one flies.
const EXPLOSION_THROW_SPEED: f32 = 4.0;

/// The throw step of `fire()` on one tile: loose items fly away from the epicenter, at least
/// as far as the blast reached past them and up to its range, plus 30%.
fn throw_from(world: &mut World, pos: TilePos, d: ThrowDetails) {
    if d.severity.is_none() {
        return;
    }
    let items: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| world.get::<DmItem>(e).is_some_and(|i| !i.anchored))
        .collect();
    for item in items {
        let low = d.range.max(0.0).floor() as u32;
        let high = (d.max_range.floor() as u32).max(low);
        let roll = world.resource_mut::<CombatRng>().range(low, high) as f32;
        let range = (roll + d.max_range * 0.3).round() as i32;
        let target = {
            let index = world.resource::<SpatialIndex>();
            wave::ranged_target_direct(pos, d.center, range, 180.0, |p| index.in_bounds(p))
        };
        crate::throw::throw_at(
            world,
            item,
            target,
            range.max(0) as u32,
            EXPLOSION_THROW_SPEED,
        );
    }
}

/// The hull creaking `CREAK_DELAY` after a big blast.
fn deliver_creaks(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let due: Vec<(f64, Entity, f32, f32)> = {
        let mut q = world.resource_mut::<ExplosionQueue>();
        if q.creaks.is_empty() {
            return;
        }
        let (due, wait) = q.creaks.drain(..).partition(|c| c.0 <= now);
        q.creaks = wait;
        due
    };
    for (_, client, volume, speed) in due {
        let file = pick_file(world, SFX_HULL_CREAKING);
        world.write_message(SendSound {
            client,
            sound: SoundEvent {
                file,
                volume,
                speed,
                offset: None,
            },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_far_and_echo() {
        // A (1, 2, 4) blast: near within round(4 + 7 - 2) = 9 tiles, far under 50.
        let near = rumble(3.0, 4.0, 50.0, 1.0, 2.0, false, true, true);
        assert_eq!(near.sound, Some((SFX_EXPLOSION, 100.0, true)));
        assert_eq!(near.shake, Some((15.0, 1.0)));
        let far = rumble(20.0, 4.0, 50.0, 1.0, 2.0, false, true, true);
        assert_eq!(far.sound, Some((FAR_SOUND, 40.0, false)));
        // `max(base, quake * 3)` capped at 1.5.
        assert_eq!(far.shake, Some((10.0, 1.5)));
        let creak = rumble(20.0, 4.0, 50.0, 1.0, 2.0, true, true, true);
        assert_eq!(creak.sound.unwrap().0, SFX_EXPLOSION_CREAKING);
        let echo = rumble(80.0, 4.0, 50.0, 1.0, 2.0, false, true, true);
        assert_eq!(echo.sound, Some((ECHO_SOUND, 60.0, false)));
        assert_eq!(echo.shake, Some((10.0, 0.25)));
        // No echo in space, nor without heavy impact.
        assert_eq!(
            rumble(80.0, 4.0, 50.0, 1.0, 2.0, false, true, false).sound,
            None
        );
        assert_eq!(
            rumble(80.0, 4.0, 0.0, 0.0, 0.0, false, true, true).sound,
            None
        );
    }
}
