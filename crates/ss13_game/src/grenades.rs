//! Grenades (server), after tg `/obj/item/grenade` (`code/game/objects/items/grenades/`):
//! `_grenade.dm` (`attack_self`, `arm_grenade`, `detonate`, `change_det_time`,
//! `atom_deconstruct`), `flashbang.dm`, `emgrenade.dm`, `smokebomb.dm`, `syndieminibomb.dm` and
//! `clusterbuster.dm`. C-4 and X-4 (`/obj/item/grenade/c4`) are `crate::explosions::sources`.
//!
//! - Use in hand arms a grenade ([`arm`]): the arm sound, the `_active` icon state, and
//!   [`Armed`] with the game time it goes off (`det_time`, 5 s by default). An armed grenade
//!   goes off wherever it is then (in a hand, in flight, in a bag: tg `get_turf`). A screwdriver
//!   cycles the fuse through `possible_fuse_time`, a multitool asks for one; neither works on an
//!   armed grenade.
//! - [`detonate`]: a used grenade (`GRENADE_USED`) is a dud. Then by kind: the flashbang flashes
//!   and bangs the living mobs that hear it (`bang`: paralysis and knockdown in the sweet spot,
//!   `soundbang_act` by distance, staggering and dropped items further out); the EMP grenade
//!   pulses `empulse(4, 10)`; the smoke grenade fills up to `DIAMOND_AREA(4)` atmos-connected
//!   tiles with bad smoke for 16 s (opaque; a mob in it without internals drops what it holds,
//!   takes 1 oxygen damage and coughs, once a second); explosive grenades call `explosion()`
//!   with their `ex_*` ranges and the frag ones fire shrapnel at every tile of the ring at
//!   `shrapnel_radius` (`pellet_cloud`); a clusterbang throws 4-8 payload grenades (35% each
//!   turn into a segment that throws 4-8 more), each armed for 1.5-6 s.
//! - A grenade destroyed by a blast goes off (`atom_deconstruct(disassembled = FALSE)`,
//!   [`on_destroyed`]).
//!
//! Cost: one pass over armed grenades and smoke clouds per tick, only while any exist.
// shortcut: no clumsy fumbles or sticky tape (`botch_check`: no clumsiness in the port), no
// projectile hit setting a held grenade off (`hit_reaction`), no ranged click throwing an armed
// grenade, no fantasy bonuses, no cosmic fields, no flashbang light or sparks, no pellet cloud
// martyrs or aggregate hit messages, no
// gradual smoke spread or fade, no smoke protection from masks (only internals), no blob damage
// from smoke; arming is logged to the server log, not to admins (`log_bomber`).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmItem, IconIndex, Opaque, ProjectileData};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_world::SpatialIndex;

use crate::combat::CombatRng;
use crate::health::{DamageType, Health, Stat};
use crate::inventory::{CarriedBy, Inventory};
use crate::server::PlayerMob;
use crate::status_effects::Incap;
use crate::tools::ToolKind;

const GRENADE: &str = "/obj/item/grenade";
const C4: &str = "/obj/item/grenade/c4";
const FLASHBANG: &str = "/obj/item/grenade/flashbang";
const FLASHBANG_CLUSTER: &str = "/obj/item/grenade/flashbang/cluster";
const EMP: &str = "/obj/item/grenade/empgrenade";
const SMOKE: &str = "/obj/item/grenade/smokebomb";
const MINIBOMB: &str = "/obj/item/grenade/syndieminibomb";
const CONCUSSION: &str = "/obj/item/grenade/syndieminibomb/concussion";
const FRAG: &str = "/obj/item/grenade/frag";
const FRAG_MEGA: &str = "/obj/item/grenade/frag/mega";
const CLUSTER: &str = "/obj/item/grenade/clusterbuster";
const CLUSTER_SEGMENT: &str = "/obj/item/grenade/clusterbuster/segment";
const SMOKE_EFFECT: &str = "/obj/effect/particle_effect/fluid/smoke/bad";
const SHRAPNEL: &str = "/obj/projectile/bullet/shrapnel";
const SHRAPNEL_MEGA: &str = "/obj/projectile/bullet/shrapnel/mega";

const ARM_SOUND: &str = "sound/items/weapons/armbomb.ogg";
const FLASHBANG_SOUND: &str = "sound/items/weapons/flashbang.ogg";
const SMOKE_SOUND: &str = "sound/effects/smoke.ogg";

/// `det_time` of `/obj/item/grenade`, seconds.
const DET_TIME: f64 = 5.0;
/// `possible_fuse_time` (`"Instant"` is 0).
const FUSES: &[f64] = &[0.0, 3.0, 4.0, 5.0];
const FLASHBANG_FUSES: &[f64] = &[3.0, 4.0, 5.0];
/// `flashbang_range`, `sweetspot_divider`.
const FLASHBANG_RANGE: u32 = 7;
const SWEETSPOT_DIVIDER: u32 = 3;
/// `SOUND_MINIMUM_PRESSURE`, kPa.
const SOUND_MINIMUM_PRESSURE: f64 = 10.0;
/// `SOUNDBANG_NORMAL`, `SOUNDBANG_STRONG`, `SOUNDBANG_OVERWHELMING`.
const SOUNDBANG_NORMAL: f32 = 1.0;
const SOUNDBANG_STRONG: f32 = 2.0;
const SOUNDBANG_OVERWHELMING: f32 = 4.0;
/// `soundbang_act`'s default `deafen_pwr` (1.5 SECONDS), and the flashbang's `15` deciseconds.
const DEAFEN: f32 = 1.5;
/// `empulse(src, 4, 10)`.
const EMP_HEAVY: i32 = 4;
const EMP_LIGHT: i32 = 10;
/// `do_smoke(4, ...)` and `/obj/effect/particle_effect/fluid/smoke/bad` `lifetime`.
const SMOKE_RANGE: u32 = 4;
const SMOKE_LIFETIME: f64 = 16.0;
/// `smoke_delay`: one smoke effect per mob per second.
const SMOKE_DELAY: f64 = 1.0;
/// Clusterbang `min_spawned`, `max_spawned`, `segment_chance`, `RANDOM_DETONATE_MIN_TIME` and
/// `RANDOM_DETONATE_MAX_TIME` (deciseconds).
const CLUSTER_MIN: u32 = 4;
const CLUSTER_MAX: u32 = 8;
const SEGMENT_CHANCE: f32 = 35.0;
const RANDOM_DETONATE_DS: (u32, u32) = (15, 60);

/// What a grenade does when it goes off.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Flashbang,
    Emp,
    Smoke,
    /// `ex_dev`, `ex_heavy`, `ex_light`, `ex_flame`; `shrapnel_radius` (0: none) and whether
    /// the shrapnel is the mega kind.
    Explosive {
        boom: [i32; 4],
        shrapnel: u32,
        mega: bool,
    },
    /// `payload`; a segment throws only payloads.
    Cluster {
        payload: String,
        segment: bool,
    },
}

/// A grenade's state (tg vars of `/obj/item/grenade`).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Grenade {
    pub kind: Kind,
    /// `det_time`, seconds.
    pub det_time: f64,
    /// `possible_fuse_time`, seconds.
    pub fuses: &'static [f64],
    /// `dud_flags & GRENADE_USED`.
    pub used: bool,
    /// `type_cluster`: not logged when armed.
    pub cluster: bool,
}

/// An armed grenade (`active`): it goes off at `at` (game seconds).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Armed {
    pub at: f64,
}

/// A cloud tile of bad smoke, gone at `until`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Smoke {
    pub until: f64,
}

/// `smoke_delay` of a mob: no smoke effect before this game time.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct SmokeDelay(pub f64);

/// The multitool's fuse prompt of a grenade.
#[derive(Component, Clone, Copy, Debug)]
pub struct FusePrompt {
    pub grenade: Entity,
}

pub struct GrenadesServerPlugin;

impl Plugin for GrenadesServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                detonate_due.run_if(any_with_component::<Armed>),
                smoke_tick.run_if(any_with_component::<Smoke>),
                fuse_prompt_answers
                    .in_set(ss13_tgui::server::UiSystems::Handle)
                    .after(ss13_tgui::prompt::PromptSystems::Answer),
            ),
        );
    }
}

/// The ported grenade types by `path` (most specific first); `None` for anything else (C-4,
/// chemical grenades and the other families are not ported here).
pub fn kind_of(path: &str) -> Option<(Kind, &'static [f64])> {
    if !is_under(path, GRENADE) || is_under(path, C4) {
        return None;
    }
    let explosive = |boom, shrapnel, mega| Kind::Explosive {
        boom,
        shrapnel,
        mega,
    };
    let kind = if is_under(path, FLASHBANG) {
        return Some((Kind::Flashbang, FLASHBANG_FUSES));
    } else if is_under(path, EMP) {
        Kind::Emp
    } else if is_under(path, SMOKE) {
        Kind::Smoke
    } else if is_under(path, CONCUSSION) {
        // `ex_dev` stays the minibomb's 1.
        explosive([1, 2, 3, 3], 0, false)
    } else if is_under(path, MINIBOMB) {
        explosive([1, 2, 4, 2], 0, false)
    } else if is_under(path, FRAG_MEGA) {
        explosive([0, 1, 3, 4], 12, true)
    } else if is_under(path, FRAG) {
        explosive([0, 1, 3, 4], 4, false)
    } else if is_under(path, CLUSTER) {
        let segment = is_under(path, CLUSTER_SEGMENT);
        let payload = match path.strip_prefix(CLUSTER).unwrap_or("") {
            "" | "/segment" => FLASHBANG_CLUSTER,
            "/emp" => EMP,
            "/smoke" => SMOKE,
            "/syndieminibomb" => MINIBOMB,
            // Chemical, spawner and slime payloads are not ported.
            _ => return None,
        };
        Kind::Cluster {
            payload: payload.to_owned(),
            segment,
        }
    } else {
        return None;
    };
    Some((kind, FUSES))
}

/// The grenade state of `item`, given on first use from its type.
pub fn grenade(world: &mut World, item: Entity) -> Option<Grenade> {
    if let Some(g) = world.get::<Grenade>(item) {
        return Some(g.clone());
    }
    let path = world.get::<DmItem>(item)?.path.clone();
    let (kind, fuses) = kind_of(&path)?;
    let g = Grenade {
        kind,
        det_time: DET_TIME,
        fuses,
        used: false,
        cluster: false,
    };
    world.entity_mut(item).insert(g.clone());
    Some(g)
}

fn name_of(world: &World, e: Entity) -> String {
    crate::chem::name_of(world, e)
}

fn tell_mob(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        crate::tools::tell(world, client, span, text);
    }
}

/// `/obj/item/grenade/attack_self`: arms it (an armed one ignores it). False for anything that
/// is not a ported grenade.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if grenade(world, item).is_none() {
        return false;
    }
    if world.get::<Armed>(item).is_none() {
        arm(world, item, Some((client, mob)), None);
    }
    true
}

/// tg `arm_grenade(user, delayoverride)`: goes off `delay` seconds from now (default its
/// `det_time`).
pub fn arm(world: &mut World, item: Entity, user: Option<(Entity, Entity)>, delay: Option<f64>) {
    let Some(g) = grenade(world, item) else {
        return;
    };
    let name = name_of(world, item);
    let pos = crate::explosions::turf_of(world, item);
    if !g.cluster {
        // `log_bomber`.
        info!(
            "{:?} has primed a {name} for detonation at {pos:?}",
            user.map(|(_, mob)| mob)
        );
    }
    if let Some((client, _)) = user {
        let time = crate::cooking::capitalize(&crate::jobs::display_time_text(g.det_time));
        crate::tools::tell(
            world,
            client,
            Span::Warning,
            format!("You prime {name}! {time}!"),
        );
    }
    if let Some(pos) = pos {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, ARM_SOUND, 60.0, true),
        );
    }
    // `icon_state = (base_icon_state || initial(icon_state)) + "_active"`.
    if let Some(state) = world
        .get::<DmAtom>(item)
        .map(|a| format!("{}_active", a.icon_state))
    {
        set_icon_state(world, item, &state);
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(item).insert(Armed {
        at: now + delay.unwrap_or(g.det_time),
    });
}

/// The item's sprite shows `state` of its own icon (when the icon has it).
fn set_icon_state(world: &mut World, item: Entity, state: &str) {
    let Some(icon) = world.get::<DmAtom>(item).map(|a| a.icon.clone()) else {
        return;
    };
    let Some(id) = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(&icon, state))
    else {
        return;
    };
    if let Some(mut look) = world.get_mut::<Appearance>(item) {
        look.icon_state = id;
    }
    if let Some(mob) = world.get::<CarriedBy>(item).map(|c| c.0) {
        crate::inventory::refresh_look(world, mob);
    }
}

/// Armed grenades whose time came.
// Cost: per tick, one pass over armed grenades (a handful), only while any exist.
fn detonate_due(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let due: Vec<Entity> = world
        .query::<(Entity, &Armed)>()
        .iter(world)
        .filter(|(_, a)| a.at <= now)
        .map(|(e, _)| e)
        .collect();
    for item in due {
        if world.get_entity(item).is_ok() {
            detonate(world, item);
        }
    }
}

/// tg `detonate`: false for a dud (it stays, disarmed) or something that is not a grenade.
pub fn detonate(world: &mut World, item: Entity) -> bool {
    let Some(g) = grenade(world, item) else {
        return false;
    };
    world.entity_mut(item).remove::<Armed>();
    if g.used {
        if let Some(state) = world.get::<DmAtom>(item).map(|a| a.icon_state.clone()) {
            set_icon_state(world, item, &state);
        }
        return false;
    }
    if let Some(mut g) = world.get_mut::<Grenade>(item) {
        g.used = true;
    }
    match g.kind {
        Kind::Flashbang => flashbang(world, item),
        Kind::Emp => {
            update_mob(world, item);
            if let Some(epicenter) = crate::explosions::turf_of(world, item) {
                world.write_message(crate::emp::Empulse {
                    epicenter,
                    heavy: EMP_HEAVY,
                    light: EMP_LIGHT,
                });
            }
        }
        Kind::Smoke => {
            update_mob(world, item);
            if let Some(pos) = crate::explosions::turf_of(world, item) {
                crate::sound::playsound(
                    world,
                    crate::sound::PlaySound::new(pos, SMOKE_SOUND, 50.0, true).extrarange(-3.0),
                );
                do_smoke(world, pos, SMOKE_RANGE);
            }
        }
        Kind::Explosive {
            boom: [dev, heavy, light, flame],
            shrapnel,
            mega,
        } => {
            if let Some(pos) = crate::explosions::turf_of(world, item) {
                if shrapnel > 0 {
                    fire_shrapnel(world, item, pos, shrapnel, mega);
                }
                let name = name_of(world, item);
                crate::explosions::explosion(
                    world,
                    pos,
                    crate::explosions::Explosion::new(dev, heavy, light)
                        .flame(flame)
                        .cause(&name),
                );
            }
        }
        Kind::Cluster { payload, segment } => cluster(world, item, &payload, segment),
    }
    if world.get_entity(item).is_ok() {
        world.despawn(item);
    }
    true
}

/// `/obj/item/grenade/atom_deconstruct(disassembled = FALSE)`: a grenade (or a C-4 charge)
/// destroyed by a blast goes off.
pub fn on_destroyed(world: &mut World, item: Entity) {
    if crate::explosions::sources::c4_destroyed(world, item) {
        return;
    }
    detonate(world, item);
}

/// tg `update_mob`: a grenade in a mob's hands falls to its feet.
fn update_mob(world: &mut World, item: Entity) {
    let Some(mob) = world.get::<CarriedBy>(item).map(|c| c.0) else {
        return;
    };
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::inventory::put_down(world, item, pos);
    }
}

/// `dropItemToGround` of both held items.
fn drop_hands(world: &mut World, mob: Entity) {
    let (Some(hands), Some(&pos)) = (
        world.get::<Inventory>(mob).map(|i| i.hands),
        world.get::<TilePos>(mob),
    ) else {
        return;
    };
    for item in hands.into_iter().flatten() {
        if !crate::inventory::no_drop(world, item) {
            crate::inventory::put_down(world, item, pos);
        }
    }
}

// --- Flashbang ----------------------------------------------------------------------------

/// `/obj/item/grenade/flashbang/detonate`.
fn flashbang(world: &mut World, item: Entity) {
    update_mob(world, item);
    let Some(pos) = crate::explosions::turf_of(world, item) else {
        return;
    };
    // "Check that there's enough pressure on the detonation turf for the 'bang' part".
    let soundbang = world
        .get_resource::<ss13_atmos::Atmos>()
        .is_none_or(|a| a.pressure(pos).is_some_and(|p| p >= SOUND_MINIMUM_PRESSURE));
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, FLASHBANG_SOUND, 100.0, true)
            .extrarange(8.0)
            .falloff_exponent(0.9),
    );
    for mob in hearers(world, pos, FLASHBANG_RANGE) {
        bang(world, pos, mob, soundbang);
    }
}

/// `get_hearers_in_view(range, turf)`: the living mobs on the tiles seen from `pos`.
fn hearers(world: &World, pos: TilePos, range: u32) -> Vec<Entity> {
    let index = world.resource::<SpatialIndex>();
    let opaque = |p: TilePos| {
        index
            .on_tile(p)
            .iter()
            .any(|&e| world.get::<Opaque>(e).is_some())
    };
    let seen = crate::sound::hearing_view(pos, range as f32, opaque, |p| index.in_bounds(p));
    index
        .in_radius(pos, range)
        .filter(|&e| {
            world.get::<Health>(e).is_some()
                && world.get::<TilePos>(e).is_some_and(|p| seen.contains(p))
        })
        .collect()
}

/// `clamp(ceil(flashbang_range / sweetspot_divider), 0, flashbang_range)`.
pub fn sweetspot(range: u32) -> u32 {
    range.div_ceil(SWEETSPOT_DIVIDER).min(range)
}

/// `/obj/item/grenade/flashbang/proc/bang` on `mob`.
fn bang(world: &mut World, at: TilePos, mob: Entity, soundbang: bool) {
    if world.get::<Stat>(mob).is_none_or(|s| *s == Stat::Dead) {
        return;
    }
    let hears = crate::sound::can_hear(world, mob);
    if hears {
        tell_mob(world, mob, Span::Warning, "BANG");
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let distance = at.chebyshev(pos).unwrap_or(0);
    let d = distance.max(1) as f32;
    let sweet = sweetspot(FLASHBANG_RANGE);
    // Flash: `flash_act(affect_silicon = 1)`.
    if crate::vision::flash_act(world, mob, 1, false, false, 2.5) {
        let silicon = world.get::<crate::silicon::Silicon>(mob).is_some();
        if distance <= sweet || silicon {
            // `Paralyze(max(20 / distance, 5))`, `Knockdown(max(200 / distance, 60))` (ds).
            crate::status_effects::apply(world, mob, Incap::Paralyze, (20.0 / d).max(5.0) / 10.0);
            crate::status_effects::knockdown(world, mob, (200.0 / d).max(60.0) / 10.0, 0.0);
        } else {
            // `adjust_dizzy_up_to(max(200 / distance, 5), 20 SECONDS)` (ds).
            crate::status_effects::adjust_timed(
                world,
                mob,
                crate::status_effects::Timed::Dizziness,
                (200.0 / d).max(5.0) / 10.0,
                Some(20.0),
            );
        }
        drop_hands(world, mob);
    }
    // Bang.
    if !soundbang && distance > 0 {
        return;
    }
    // `get_ear_protection(ignore_deafness = FALSE)`: the deaf are fully protected.
    if !hears {
        return;
    }
    let ear = |world: &mut World, max: u32| world.resource_mut::<CombatRng>().range(0, max) as f32;
    let ear_soundbang = crate::explosions::ex_act::soundbang_act;
    if distance == 0 {
        ear_soundbang(world, mob, SOUNDBANG_OVERWHELMING, 20.0, 10.0, DEAFEN);
        return;
    }
    // "Adds more stun as to not prime n' pull".
    if distance <= 1 {
        ear_soundbang(world, mob, SOUNDBANG_STRONG, 3.0, 5.0, DEAFEN);
        return;
    }
    if distance <= sweet {
        let damage = ear(world, 5);
        ear_soundbang(
            world,
            mob,
            SOUNDBANG_NORMAL,
            (20.0 / d).max(6.0),
            damage,
            DEAFEN,
        );
        return;
    }
    let damage = ear(world, 2);
    if ear_soundbang(world, mob, SOUNDBANG_NORMAL, 0.0, damage, DEAFEN) <= 0.0 {
        return;
    }
    // `adjust_staggered_up_to(max(200 / distance, 5), 10 SECONDS)`.
    let dead = world.get::<Stat>(mob) == Some(&Stat::Dead);
    if let Some(mut e) = crate::status_effects::effects_mut(world, mob) {
        e.adjust_staggered_up_to((200.0 / d).max(5.0) / 10.0, 10.0, dead);
    }
    drop_hands(world, mob);
}

// --- Smoke --------------------------------------------------------------------------------

/// `DIAMOND_AREA(range)`: the smoke's `amount`.
pub fn diamond_area(range: u32) -> u32 {
    1 + 2 * range * (range + 1)
}

/// `do_smoke(range, location, smoke_type = bad)`: a cloud over up to `diamond_area(range)`
/// tiles, spreading through atmos-adjacent tiles, never onto a tile that already has smoke.
pub(crate) fn do_smoke(world: &mut World, origin: TilePos, range: u32) {
    let target = diamond_area(range) as usize;
    let mut placed: Vec<TilePos> = Vec::new();
    let mut seen = std::collections::HashSet::from([origin]);
    let mut queue = std::collections::VecDeque::from([origin]);
    while let Some(pos) = queue.pop_front() {
        if placed.len() >= target {
            break;
        }
        if pos != origin && has_smoke(world, pos) {
            continue;
        }
        placed.push(pos);
        for next in spread_from(world, pos) {
            if seen.insert(next) {
                queue.push_back(next);
            }
        }
    }
    let until = world.resource::<Time>().elapsed_secs_f64() + SMOKE_LIFETIME;
    for pos in placed {
        if let Some(smoke) = crate::construction::spawn_obj(world, SMOKE_EFFECT, pos) {
            world.entity_mut(smoke).insert((Smoke { until }, Opaque));
        }
    }
}

fn has_smoke(world: &World, pos: TilePos) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| world.get::<Smoke>(e).is_some())
}

/// `get_atmos_adjacent_turfs()`; without the air model, the cardinal tiles whose turf is open.
fn spread_from(world: &World, pos: TilePos) -> Vec<TilePos> {
    if let Some(atmos) = world.get_resource::<ss13_atmos::Atmos>() {
        return atmos.adjacent_turfs(pos, false);
    }
    let index = world.resource::<SpatialIndex>();
    Dir::CARDINALS
        .into_iter()
        .map(|d| pos.step(d))
        .filter(|&p| {
            index.in_bounds(p)
                && crate::construction::turf_at(world, p)
                    .is_some_and(|t| world.get::<ss13_world::Density>(t).is_none())
        })
        .collect()
}

/// Smoke `process`: gone after its lifetime; `smoke_mob` of the bad smoke on the mobs in it.
// Cost: per tick, the smoke tiles (at most 41 per grenade) and what stands on them.
fn smoke_tick(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let clouds: Vec<(Entity, TilePos, f64)> = world
        .query::<(Entity, &Smoke, &TilePos)>()
        .iter(world)
        .map(|(e, s, p)| (e, *p, s.until))
        .collect();
    for (smoke, pos, until) in clouds {
        if now >= until {
            world.despawn(smoke);
            crate::construction::tile_changed(world, pos);
            continue;
        }
        let mobs: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .filter(|&e| world.get::<Inventory>(e).is_some() && world.get::<Health>(e).is_some())
            .collect();
        for mob in mobs {
            smoke_mob(world, mob, now);
        }
    }
}

/// `/obj/effect/particle_effect/fluid/smoke/bad/smoke_mob` on a carbon.
fn smoke_mob(world: &mut World, mob: Entity, now: f64) {
    let internals = world
        .get::<crate::internals::Internals>(mob)
        .is_some_and(|i| i.0.is_some());
    let delayed = world.get::<SmokeDelay>(mob).is_some_and(|d| now < d.0);
    if internals || delayed || world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return;
    }
    world.entity_mut(mob).insert(SmokeDelay(now + SMOKE_DELAY));
    drop_hands(world, mob);
    if let Ok(mut entity) = world.get_entity_mut(mob)
        && let Ok((mut health, mut stat)) = entity.get_components_mut::<(&mut Health, &mut Stat)>()
    {
        crate::health::apply_damage(&mut health, &mut stat, DamageType::Oxy, 1.0);
    }
    if let Some(emote) = ss13_chat::emote::find("cough") {
        crate::mobs::defense::emote(world, mob, emote.message);
    }
}

// --- Shrapnel -----------------------------------------------------------------------------

/// `/obj/projectile/bullet/shrapnel` (and `/mega`) with the `/obj/projectile/bullet` and
/// `/obj/projectile` defaults it inherits.
pub fn shrapnel_data(mega: bool) -> ProjectileData {
    ProjectileData {
        path: if mega { SHRAPNEL_MEGA } else { SHRAPNEL }.to_owned(),
        damage: 14.0,
        damage_type: "brute".into(),
        armor_flag: "bullet".into(),
        weak_against_armour: true,
        speed: 1.25,
        range: if mega { 45.0 } else { 20.0 },
        pass_flags: crate::projectile::PASSTABLE,
        accurate_range: 100.0,
        accuracy_falloff: 7.0,
        ricochets_max: if mega { 6 } else { 2 },
        ricochet_chance: if mega { 130.0 } else { 70.0 },
        ricochet_decay_chance: if mega { 0.9 } else { 0.7 },
        ricochet_decay_damage: 0.7,
        ricochet_incidence_leeway: if mega { 0.0 } else { 60.0 },
        wound_bonus: 30.0,
        wound_falloff_tile: -5.0,
        // `SHARP_EDGED`.
        sharpness: 1,
        ..default()
    }
}

/// The tiles of the square ring `radius` tiles out (`RANGE_TURFS(r) - RANGE_TURFS(r - 1)`).
pub fn ring(center: TilePos, radius: u32) -> Vec<TilePos> {
    let r = radius as i32;
    let mut out = Vec::new();
    for dy in -r..=r {
        for dx in -r..=r {
            if dx.abs().max(dy.abs()) == r {
                out.push(TilePos::new(center.x + dx, center.y + dy, center.z));
            }
        }
    }
    out
}

/// `pellet_cloud/create_blast_pellets`: one pellet at every tile of the ring.
fn fire_shrapnel(world: &mut World, grenade: Entity, pos: TilePos, radius: u32, mega: bool) {
    let data = shrapnel_data(mega);
    for target in ring(pos, radius) {
        crate::projectile::fire(
            world,
            crate::projectile::Shot {
                data: &data,
                firer: grenade,
                from: pos,
                target: None,
                target_pos: target,
                zone: crate::zone::BodyZone::Chest,
                hit_prone: true,
                pixel: [0, 0],
                deviation: 0.0,
                fired_from: Some(grenade),
            },
        );
    }
}

// --- Clusterbang --------------------------------------------------------------------------

/// How a clusterbang splits: `(payloads, segments)` from `rand(min, max)` payloads, each of
/// which turns into a segment with `segment_chance` (`roll` is DM `prob`).
pub fn cluster_split(spawned: u32, mut roll: impl FnMut() -> bool) -> (u32, u32) {
    let mut payloads = spawned;
    let mut segments = 0;
    for _ in 0..spawned {
        if roll() {
            segments += 1;
            payloads -= 1;
        }
    }
    (payloads, segments)
}

/// `/obj/item/grenade/clusterbuster/detonate` (and the segment's).
fn cluster(world: &mut World, item: Entity, payload: &str, segment: bool) {
    update_mob(world, item);
    let Some(pos) = crate::explosions::turf_of(world, item) else {
        return;
    };
    let spawned = world
        .resource_mut::<CombatRng>()
        .range(CLUSTER_MIN, CLUSTER_MAX);
    let (payloads, segments) = if segment {
        (spawned, 0)
    } else {
        cluster_split(spawned, || {
            world.resource_mut::<CombatRng>().chance(SEGMENT_CHANCE)
        })
    };
    for _ in 0..segments {
        // `new /obj/item/grenade/clusterbuster/segment(drop_location(), src)`.
        let kind = Kind::Cluster {
            payload: payload.to_owned(),
            segment: true,
        };
        spawn_armed(world, CLUSTER_SEGMENT, pos, Some(kind));
    }
    for _ in 0..payloads {
        spawn_armed(world, payload, pos, None);
    }
    let name = name_of(world, item);
    info!("{name} at {pos:?} split into {payloads} payloads and {segments} segments");
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, ARM_SOUND, 75.0, true).extrarange(-3.0),
    );
}

/// A payload or segment: armed for 1.5-6 s (`type_cluster`), it scatters 1-4 random steps.
// shortcut: tg's payload spawner steps itself between payloads (`step_away(src, loc)`); here
// each payload takes its own random steps.
fn spawn_armed(world: &mut World, path: &str, pos: TilePos, kind: Option<Kind>) {
    let Some(e) = crate::construction::spawn_obj(world, path, pos) else {
        return;
    };
    let Some(mut g) = grenade(world, e) else {
        return;
    };
    if let Some(kind) = kind {
        g.kind = kind;
    }
    g.cluster = true;
    let (ds, steps) = {
        let mut rng = world.resource_mut::<CombatRng>();
        (
            rng.range(RANDOM_DETONATE_DS.0, RANDOM_DETONATE_DS.1),
            rng.range(1, 4),
        )
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(e).insert((
        g,
        Armed {
            at: now + f64::from(ds) / 10.0,
        },
    ));
    for _ in 0..steps {
        let dir = Dir::DMI_ORDER[world.resource_mut::<CombatRng>().range(0, 7) as usize];
        let _ = crate::pull::step_once(world, e, dir, None);
    }
}

// --- Fuse ---------------------------------------------------------------------------------

/// The screwdriver's `change_det_time()`: the next of `fuses` after `current` (the first when
/// `current` is the last or not among them).
pub fn next_fuse(fuses: &[f64], current: f64) -> f64 {
    let next = fuses
        .iter()
        .position(|&f| f == current)
        .map_or(0, |i| i + 1);
    fuses
        .get(next)
        .or(fuses.first())
        .copied()
        .unwrap_or(DET_TIME)
}

fn fuse_message(det_time: f64) -> String {
    if det_time <= 0.0 {
        "You modify the time delay. It's set to be instantaneous.".to_owned()
    } else {
        format!(
            "You modify the time delay. It's set for {}.",
            crate::jobs::display_time_text(det_time)
        )
    }
}

/// `screwdriver_act` and `multitool_act` of a grenade held in the other hand or next to the
/// user. False when `held` is not one of those tools or `target` is not a ported grenade.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some((tool, _)) = crate::tools::tool_of(world, held) else {
        return false;
    };
    if !matches!(tool, ToolKind::Screwdriver | ToolKind::Multitool) {
        return false;
    }
    let Some(g) = grenade(world, target) else {
        return false;
    };
    if world.get::<Armed>(target).is_some() {
        return false;
    }
    if tool == ToolKind::Screwdriver {
        let det_time = next_fuse(g.fuses, g.det_time);
        if let Some(mut g) = world.get_mut::<Grenade>(target) {
            g.det_time = det_time;
        }
        crate::tools::tell(world, client, Span::Notice, fuse_message(det_time));
        return true;
    }
    let items = g
        .fuses
        .iter()
        .map(|&f| {
            if f <= 0.0 {
                "Instant".to_owned()
            } else {
                format!("{f}")
            }
        })
        .collect();
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Detonation Timer",
            "Please enter a new detonation time",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(FusePrompt { grenade: target });
    true
}

/// The multitool's answer, checked now (`can_perform_action`): the grenade in the user's hands
/// or next to it, not armed.
fn fuse_prompt_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answers {
        let Some(FusePrompt { grenade }) = world.get::<FusePrompt>(prompt).copied() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        if world.get::<Armed>(grenade).is_some() || !within_reach(world, actor, grenade) {
            continue;
        }
        let Some(&fuse) = world.get::<Grenade>(grenade).and_then(|g| g.fuses.get(i)) else {
            continue;
        };
        // `change_det_time(time)`: `clamp(time SECONDS, 0, 5 SECONDS)`.
        let det_time = fuse.clamp(0.0, DET_TIME);
        if let Some(mut g) = world.get_mut::<Grenade>(grenade) {
            g.det_time = det_time;
        }
        crate::tools::tell(world, client, Span::Notice, fuse_message(det_time));
    }
}

/// Held by `mob`, or on a tile next to it.
fn within_reach(world: &World, mob: Entity, item: Entity) -> bool {
    if world.get::<CarriedBy>(item).is_some_and(|c| c.0 == mob) {
        return true;
    }
    match (world.get::<TilePos>(mob), world.get::<TilePos>(item)) {
        (Some(&a), Some(&b)) => a.chebyshev(b).is_some_and(|d| d <= 1),
        _ => false,
    }
}

/// `/obj/item/grenade/examine`: the timer.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let det_time = match world.get::<Grenade>(target) {
        Some(g) => g.det_time,
        None => match world.get::<DmItem>(target).and_then(|d| kind_of(&d.path)) {
            Some(_) => DET_TIME,
            None => return,
        },
    };
    let line = if det_time > 0.0 {
        format!(
            "The timer is set to {}.",
            crate::jobs::display_time_text(det_time)
        )
    } else {
        "It is set for instant detonation.".to_owned()
    };
    lines.push((Span::Plain, line));
    if world.get::<Grenade>(target).is_some_and(|g| g.used) {
        lines.push((
            Span::Warning,
            "It looks like it's already been used.".to_owned(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_follow_the_type_tree() {
        assert_eq!(
            kind_of("/obj/item/grenade/flashbang/cluster").map(|k| k.0),
            Some(Kind::Flashbang)
        );
        assert_eq!(
            kind_of("/obj/item/grenade/syndieminibomb/concussion").map(|k| k.0),
            Some(Kind::Explosive {
                boom: [1, 2, 3, 3],
                shrapnel: 0,
                mega: false
            })
        );
        assert_eq!(
            kind_of("/obj/item/grenade/clusterbuster/emp").map(|k| k.0),
            Some(Kind::Cluster {
                payload: EMP.into(),
                segment: false
            })
        );
        assert_eq!(kind_of("/obj/item/grenade/clusterbuster/inferno"), None);
        assert_eq!(kind_of("/obj/item/grenade/c4/x4"), None);
        assert_eq!(kind_of("/obj/item/grenade/chem_grenade"), None);
        assert_eq!(kind_of("/obj/item/toy"), None);
    }

    #[test]
    fn screwdriver_cycles_the_fuse() {
        assert_eq!(next_fuse(FUSES, 5.0), 0.0);
        assert_eq!(next_fuse(FUSES, 0.0), 3.0);
        assert_eq!(next_fuse(FUSES, 3.0), 4.0);
        assert_eq!(next_fuse(FLASHBANG_FUSES, 5.0), 3.0);
        // Not in the list: the first.
        assert_eq!(next_fuse(FLASHBANG_FUSES, 0.0), 3.0);
    }

    #[test]
    fn flashbang_sweetspot_and_smoke_area() {
        assert_eq!(sweetspot(7), 3);
        assert_eq!(sweetspot(1), 1);
        assert_eq!(diamond_area(4), 41);
        assert_eq!(diamond_area(0), 1);
    }

    #[test]
    fn shrapnel_ring_is_the_outer_square() {
        let c = TilePos::new(10, 10, 1);
        let tiles = ring(c, 4);
        assert_eq!(tiles.len(), 81 - 49);
        assert!(tiles.iter().all(|t| c.chebyshev(*t) == Some(4)));
        assert_eq!(ring(c, 1).len(), 8);
    }

    #[test]
    fn clusterbang_segments_take_from_the_payloads() {
        assert_eq!(cluster_split(6, || false), (6, 0));
        assert_eq!(cluster_split(6, || true), (0, 6));
        let mut flip = false;
        assert_eq!(
            cluster_split(4, || {
                flip = !flip;
                flip
            }),
            (2, 2)
        );
    }
}
