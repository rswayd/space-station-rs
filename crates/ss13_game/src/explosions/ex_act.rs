//! `ex_act` (server): what one atom does at an explosion severity, after tg `EX_ACT`
//! (`code/__DEFINES/explosions.dm`), `/atom/ex_act`, `contents_explosion` and their overrides.
//!
//! - Walls (`/turf/closed/wall/ex_act`): devastating scrapes the wall away, heavy dismantles it
//!   (half the time devastated: sheets only), light dismantles it with `prob(hardness)`.
//! - Floors (`/turf/open/floor/ex_act`): shielded below devastation by a dense object on them;
//!   devastating scrapes two layers off, heavy rolls 1-3 (two layers, or a lattice over space,
//!   and iron at 33%; two layers; one layer at 80% else a broken tile, a hotspot and iron at
//!   33%), light breaks the tile and lights a hotspot half the time.
//! - Objects (`/obj/ex_act`): `INFINITY`, `rand(100, 250)` or `rand(10, 90)` brute through the
//!   `bomb` armor, unless `INDESTRUCTIBLE`/`BOMB_PROOF`; doors take it one step worse; fuel
//!   tanks blow up ([`super::sources`]).
//! - Items: the same damage against their `max_integrity` (kept from the first hit); at 0 they
//!   are destroyed (a gas tank fragments first, [`super::sources`]).
//! - Humans (`/mob/living/carbon/human/ex_act`): gibbed by devastation under 50 `bomb` armor,
//!   else brute and burn over the bodyparts through each part's `bomb` armor, wounds and
//!   dismemberment, unconsciousness, a knockdown and ringing ears (`soundbang_act`).
//! - Basic mobs: gibbed by devastation under 50 armor, else 600/60/30 brute. Silicons: 60/60 or
//!   30 brute while alive.
//! - Contents (`contents_explosion`): storage items and closets pass the same severity on to
//!   what they hold.

use bevy::prelude::*;
use ss13_mapload::construct::DmTurfBuild;
use ss13_mapload::{DmArmor, DmItem, DmObj};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::{Density, SpatialIndex};

use super::Severity;
use crate::combat::CombatRng;
use crate::construction::{path_of, scrape_away, spawn_obj, spawn_stack, turf_at};
use crate::health::{DamageType, Health, Stat, apply_damage_zone};
use crate::medical::organs::{self, Slot};
use crate::medical::wounds::{Limbs, WoundKind, apply_wound};
use crate::obj_damage::{INDESTRUCTIBLE, Integrity, is_under};
use crate::zone::BodyZone;

/// `BOMB_PROOF` (`resistance_flags`).
pub const BOMB_PROOF: u32 = 1 << 9;
/// `EXPLODE_GIB_THRESHOLD`: devastation gibs a mob with less `bomb` armor.
pub const EXPLODE_GIB_THRESHOLD: f32 = 50.0;
/// `/obj/item` `max_integrity` when the prototype has none.
const ITEM_MAX_INTEGRITY: f32 = 200.0;
/// `CELL_VOLUME`.
const CELL_VOLUME: f64 = 2500.0;

const WALL: &str = "/turf/closed/wall";
const R_WALL: &str = "/turf/closed/wall/r_wall";
const FLOOR: &str = "/turf/open/floor";
const IRON: &str = "/obj/item/stack/sheet/iron";
const RODS: &str = "/obj/item/stack/rods";
const DOOR: &str = "/obj/machinery/door";
pub const LATTICE: &str = "/obj/structure/lattice";
pub const CATWALK: &str = "/obj/structure/lattice/catwalk";
const CLOSET: &str = "/obj/structure/closet";
const EARMUFFS: &str = "/obj/item/clothing/ears/earmuffs";

/// `EX_ACT(turf, severity)` at once: what is on the tile, then the turf (meteors ram tiles
/// this way; explosions queue the same steps, [`super`]).
pub fn ex_act_tile(world: &mut World, pos: TilePos, severity: Severity) {
    for thing in tile_contents(world, pos) {
        ex_act_movable(world, thing, severity);
    }
    ex_act_turf(world, pos, severity);
}

/// `/turf/contents_explosion`: the movables on the tile (mobs in a closet stand on its tile
/// here, so they come with them rather than with the closet).
pub fn tile_contents(world: &World, pos: TilePos) -> Vec<Entity> {
    let turf = turf_at(world, pos);
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        // A mech's pilot meets the blast through the mech (`mecha/contents_explosion`).
        .filter(|&e| Some(e) != turf && world.get::<crate::mecha::core::Pilot>(e).is_none())
        .collect()
}

/// `contents_explosion` of a movable: what a storage item or a closet holds.
pub fn contents_of(world: &World, thing: Entity) -> Vec<Entity> {
    let holds = world.get::<crate::storage::Storage>(thing).is_some()
        || world
            .get::<DmObj>(thing)
            .is_some_and(|o| is_under(&o.path, CLOSET));
    if !holds {
        return Vec::new();
    }
    world
        .get::<crate::storage::Stored>(thing)
        .map(|s| s.items().to_vec())
        .unwrap_or_default()
}

/// `ex_act` of one movable (without its contents: see [`contents_of`]).
pub fn ex_act_movable(world: &mut World, thing: Entity, severity: Severity) {
    if world.get_entity(thing).is_err() {
        return;
    }
    if world.get::<Health>(thing).is_some() {
        ex_act_mob(world, thing, severity);
    } else if world.get::<DmItem>(thing).is_some() {
        ex_act_item(world, thing, severity);
    } else if world.get::<DmObj>(thing).is_some() {
        ex_act_obj(world, thing, severity);
    }
}

/// `/obj/ex_act`'s damage roll.
fn bomb_damage(world: &mut World, severity: Severity) -> f32 {
    let mut rng = world.resource_mut::<CombatRng>();
    match severity {
        Severity::Devastate => f32::INFINITY,
        Severity::Heavy => rng.range(100, 250) as f32,
        Severity::Light => rng.range(10, 90) as f32,
    }
}

/// `run_atom_armor` of `BOMB` damage: no deflection, `round(amount * (100 - armor)%, 0.1)`.
pub fn through_armor(amount: f32, armor: f32) -> f32 {
    let armor = armor.clamp(0.0, 100.0);
    if amount.is_infinite() {
        return amount;
    }
    (amount * (100.0 - armor) * 0.01 * 10.0).round() / 10.0
}

/// `/obj/ex_act` of a structure or machine.
// shortcut: only objects whose destruction is ported take damage (windows, grilles, girders,
// frames, gas containers, vendors, machines built from boards, lattices, catwalks); doors,
// closets, tables and the rest wait for their `atom_deconstruct`.
fn ex_act_obj(world: &mut World, obj: Entity, severity: Severity) {
    let Some(dm) = world.get::<DmObj>(obj).cloned() else {
        return;
    };
    if dm.resistance_flags & (INDESTRUCTIBLE | BOMB_PROOF) != 0 {
        return;
    }
    if super::sources::fueltank_ex_act(world, obj) {
        return;
    }
    if let Some(occupant) = world
        .get::<crate::mecha::core::Mech>(obj)
        .map(|m| m.occupant)
    {
        // `mecha/contents_explosion`: the pilot one step milder; then `take_damage(BOMB)`.
        if let (Some(pilot), Some(milder)) = (occupant, severity.milder()) {
            super::queue_movables(world, &[pilot], milder);
        }
        let damage = bomb_damage(world, severity);
        crate::mecha::core::take_damage(world, obj, damage, DamageType::Brute, "bomb", None);
        return;
    }
    // `/obj/machinery/door/ex_act`: "if it blows up a wall it should blow up a door".
    let severity = if is_under(&dm.path, DOOR) {
        severity.worse()
    } else {
        severity
    };
    if world.get::<Integrity>(obj).is_none() {
        let destructible = is_under(&dm.path, LATTICE)
            || crate::machine_frame::buildable_machine(world, obj).is_some();
        if !destructible || dm.max_integrity <= 0.0 {
            return;
        }
        world.entity_mut(obj).insert(Integrity {
            current: dm.max_integrity,
            broken: false,
        });
    }
    let damage = bomb_damage(world, severity);
    crate::obj_damage::take_bomb_damage(world, obj, damage);
}

/// `/obj/ex_act` of an item: `take_damage(..., BRUTE, BOMB)` against its integrity.
fn ex_act_item(world: &mut World, item: Entity, severity: Severity) {
    let Some(dm) = world.get::<DmItem>(item).cloned() else {
        return;
    };
    // `/obj/item/gibtonite/ex_act`, `/obj/item/stack/ore/ex_act`.
    if crate::mining::rocks::item_ex_act(world, item, severity) {
        return;
    }
    if dm.resistance_flags & (INDESTRUCTIBLE | BOMB_PROOF) != 0 {
        return;
    }
    let damage = bomb_damage(world, severity);
    let armor = world.get::<DmArmor>(item).map_or(0.0, |a| a.bomb);
    let damage = through_armor(damage, armor);
    damage_item(world, item, damage);
}

/// tg `take_damage` of an item: `atom_break` at `integrity_failure` (gas tanks), destruction
/// at 0.
pub fn damage_item(world: &mut World, item: Entity, damage: f32) {
    if damage < crate::obj_damage::DAMAGE_PRECISION {
        return;
    }
    let max = world
        .get::<DmItem>(item)
        .map(|d| d.max_integrity)
        .filter(|m| *m > 0.0)
        .unwrap_or(ITEM_MAX_INTEGRITY);
    let mut integrity = world.get::<Integrity>(item).copied().unwrap_or(Integrity {
        current: max,
        broken: false,
    });
    if integrity.current <= 0.0 {
        return;
    }
    let previous = integrity.current;
    integrity.current = (previous - damage).max(0.0);
    world.entity_mut(item).insert(integrity);
    if let Some(failure) = super::sources::item_integrity_failure(world, item)
        && previous > failure * max
        && integrity.current <= failure * max
    {
        super::sources::item_break(world, item);
    }
    if integrity.current <= 0.0 && world.get_entity(item).is_ok() {
        destroy_item(world, item);
    }
}

/// tg `atom_destruction` -> `deconstruct(FALSE)` of an item: its own effects (a tank's
/// fragmentation), then it is gone.
pub fn destroy_item(world: &mut World, item: Entity) {
    super::sources::item_destruction(world, item);
    // `/obj/item/grenade/atom_deconstruct(disassembled = FALSE)`: it goes off.
    crate::grenades::on_destroyed(world, item);
    if world.get_entity(item).is_ok() {
        world.despawn(item);
    }
}

/// `/mob/living/.../ex_act` by kind of mob.
pub fn ex_act_mob(world: &mut World, mob: Entity, severity: Severity) {
    if world.get::<Stat>(mob).is_none() {
        return;
    }
    if world.get::<crate::silicon::Silicon>(mob).is_some() {
        ex_act_silicon(world, mob, severity);
    } else if world.get::<Health>(mob).is_some_and(|h| h.basic.is_some()) {
        ex_act_basic(world, mob, severity);
    } else {
        ex_act_human(world, mob, severity);
    }
}

/// `/mob/living/silicon/robot/ex_act` and the AI's.
// shortcut: devastation kills instead of gibbing (no silicon debris, MMI or core remains).
fn ex_act_silicon(world: &mut World, mob: Entity, severity: Severity) {
    let dead = world.get::<Stat>(mob) == Some(&Stat::Dead);
    let (brute, burn) = match severity {
        Severity::Devastate => (f32::from(u16::MAX), 0.0),
        Severity::Heavy => (60.0, 60.0),
        Severity::Light => (30.0, 0.0),
    };
    if dead {
        return;
    }
    adjust(world, mob, DamageType::Brute, brute);
    adjust(world, mob, DamageType::Burn, burn);
}

fn adjust(world: &mut World, mob: Entity, kind: DamageType, amount: f32) {
    if amount <= 0.0 {
        return;
    }
    if let Ok(mut entity) = world.get_entity_mut(mob)
        && let Ok((mut health, mut stat)) = entity.get_components_mut::<(&mut Health, &mut Stat)>()
    {
        crate::health::apply_damage(&mut health, &mut stat, kind, amount);
    }
}

/// `/mob/living/basic/ex_act` (and `simple_animal`'s).
fn ex_act_basic(world: &mut World, mob: Entity, severity: Severity) {
    // shortcut: basic mobs wear no armor here (`getarmor` is 0).
    let bomb_armor = 0.0_f32;
    let multi = (1.0 - bomb_armor / 166.6).clamp(0.0, 1.0);
    let loss = match severity {
        Severity::Devastate if bomb_armor < EXPLODE_GIB_THRESHOLD => {
            crate::health::gib(world, mob);
            return;
        }
        Severity::Devastate => 600.0,
        Severity::Heavy => 60.0,
        Severity::Light => 30.0,
    };
    adjust(
        world,
        mob,
        DamageType::Brute,
        (loss * multi * 10.0).round() / 10.0,
    );
}

/// `SOUNDBANG_*`.
const SOUNDBANG_STRONG: f32 = 2.0;
const SOUNDBANG_MASSIVE: f32 = 3.0;
const SOUNDBANG_OVERWHELMING: f32 = 4.0;
/// `EAR_PROTECTION_NORMAL` (earmuffs), `EAR_PROTECTION_VACUUM`.
const EAR_PROTECTION_NORMAL: f32 = 1.0;
const EAR_PROTECTION_VACUUM: f32 = 3.0;
/// `SOUND_MINIMUM_PRESSURE`, kPa.
const SOUND_MINIMUM_PRESSURE: f64 = 10.0;

/// One severity's numbers of `/mob/living/carbon/human/ex_act`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanBlast {
    pub brute: f32,
    pub burn: f32,
    pub soundbang: f32,
    pub ear_damage: f32,
    /// Seconds.
    pub deafen: f32,
    pub max_wounds: u32,
    pub wound_prob: f32,
    pub dismember_prob: f32,
    /// `WOUND_SEVERITY_*` the wounds pick from.
    pub wound_severities: &'static [u8],
    /// Seconds.
    pub knockdown: f32,
    pub unconscious: f32,
}

impl HumanBlast {
    pub fn of(severity: Severity) -> Self {
        match severity {
            Severity::Devastate => Self {
                brute: 300.0,
                burn: 300.0,
                soundbang: SOUNDBANG_OVERWHELMING,
                ear_damage: 60.0,
                deafen: 360.0,
                max_wounds: 5,
                wound_prob: 75.0,
                dismember_prob: 50.0,
                wound_severities: &[2, 3],
                knockdown: 20.0,
                unconscious: 3.0,
            },
            Severity::Heavy => Self {
                brute: 60.0,
                burn: 60.0,
                soundbang: SOUNDBANG_MASSIVE,
                ear_damage: 30.0,
                deafen: 240.0,
                max_wounds: 3,
                wound_prob: 40.0,
                dismember_prob: 33.0,
                wound_severities: &[1, 2, 3],
                knockdown: 20.0,
                unconscious: 2.0,
            },
            Severity::Light => Self {
                brute: 30.0,
                burn: 0.0,
                soundbang: SOUNDBANG_STRONG,
                ear_damage: 15.0,
                deafen: 120.0,
                max_wounds: 2,
                wound_prob: 30.0,
                dismember_prob: 33.0,
                wound_severities: &[1, 2],
                knockdown: 16.0,
                unconscious: 0.0,
            },
        }
    }
}

/// tg `getarmor(null, BOMB)` of a human: the average over its bodyparts.
fn average_armor(world: &World, mob: Entity, flag: &str) -> f32 {
    let missing = world.get::<Health>(mob).map_or(0, |h| h.missing);
    let parts: Vec<BodyZone> = BodyZone::ALL
        .into_iter()
        .filter(|z| missing & (1 << z.index()) == 0)
        .collect();
    let total: f32 = parts
        .iter()
        .map(|&z| crate::armor::getarmor(world, mob, z, flag))
        .sum();
    total / parts.len().max(1) as f32
}

/// `/mob/living/carbon/human/ex_act`.
// shortcut: a devastated human above the gib threshold is not thrown (no mob throws yet), and
// worn clothes take no damage (`damage_clothes`: no clothing integrity).
fn ex_act_human(world: &mut World, mob: Entity, severity: Severity) {
    let bomb_armor = average_armor(world, mob, "bomb");
    if severity == Severity::Devastate && bomb_armor < EXPLODE_GIB_THRESHOLD {
        // What it carried meets the blast too, next round of the subsystem.
        let carried: Vec<Entity> = world
            .get::<crate::inventory::Inventory>(mob)
            .map(|inv| {
                crate::inventory::ALL_SLOTS
                    .iter()
                    .filter_map(|&s| inv.slot(s))
                    .collect()
            })
            .unwrap_or_default();
        crate::health::gib(world, mob);
        super::queue_movables(world, &carried, Severity::Devastate);
        return;
    }
    let blast = HumanBlast::of(severity);
    // "Do this first, before wounds, so we DONT get wound text".
    if blast.unconscious > 0.0 {
        crate::status_effects::apply(
            world,
            mob,
            crate::status_effects::Incap::Unconscious,
            blast.unconscious,
        );
    }
    let missing = world.get::<Health>(mob).map_or(0, |h| h.missing);
    let parts: Vec<BodyZone> = BodyZone::ALL
        .into_iter()
        .filter(|z| missing & (1 << z.index()) == 0)
        .collect();
    let n = parts.len().max(1) as f32;
    // `round(loss / bodyparts, DAMAGE_PRECISION)`.
    let share = |loss: f32| (loss / n * 10.0).round() / 10.0;
    let mut wounds_left = blast.max_wounds;
    for zone in parts {
        let bp_armor = crate::armor::getarmor(world, mob, zone, "bomb");
        // "scales linearly up to 0.6 at 100 armor"
        let multi = (bp_armor / 166.6).clamp(0.0, 1.0);
        if let Ok(mut entity) = world.get_entity_mut(mob)
            && let Ok((mut health, mut stat)) =
                entity.get_components_mut::<(&mut Health, &mut Stat)>()
        {
            for (kind, loss) in [
                (DamageType::Brute, blast.brute),
                (DamageType::Burn, blast.burn),
            ] {
                let amount = share(loss) * (1.0 - multi);
                if amount > 0.0 {
                    apply_damage_zone(&mut health, &mut stat, zone, kind, amount);
                }
            }
        }
        let rolled = world
            .resource_mut::<CombatRng>()
            .chance(blast.wound_prob * (1.0 - multi));
        if !rolled {
            continue;
        }
        let limb = !matches!(zone, BodyZone::Head | BodyZone::Chest);
        let dismember_roll = world
            .resource_mut::<CombatRng>()
            .chance(blast.dismember_prob);
        if dismember_roll && limb && (severity >= Severity::Heavy || bp_armor == 0.0) {
            // "bomb armor has a second chance to stop dismemberment if it's over 50".
            let saved = world
                .resource_mut::<CombatRng>()
                .chance((bomb_armor - 50.0) * 2.0);
            // `BP.receive_damage(INFINITY)` of the removed limb: it leaves wrecked.
            if !saved
                && let Some(Some(item)) =
                    crate::medical::bodyparts::dismember_limb(world, mob, zone, false)
                && let Some(mut limb) = world.get_mut::<crate::medical::bodyparts::Limb>(item)
            {
                limb.brute = (zone.max_damage() - limb.burn).max(limb.brute);
            }
        } else {
            let wound_armor = crate::armor::getarmor(world, mob, zone, "wound");
            if !world.resource_mut::<CombatRng>().chance(wound_armor) {
                blast_wound(world, mob, zone, blast.wound_severities);
            }
        }
        wounds_left = wounds_left.saturating_sub(1);
        if wounds_left == 0 {
            break;
        }
    }
    // `Knockdown(knockdown_duration - bomb_armor * 1.6)`: deciseconds of armor off.
    let knockdown = blast.knockdown - bomb_armor * 0.16;
    if knockdown > 0.0 {
        crate::status_effects::knockdown(world, mob, knockdown, 0.0);
    }
    soundbang_act(
        world,
        mob,
        blast.soundbang,
        0.0,
        blast.ear_damage,
        blast.deafen,
    );
}

/// `cause_wound_of_type_and_severity(pick_weight(slash 1, blunt 2, burn 2), part, moderate,
/// pick(severities), WOUND_PICK_HIGHEST_SEVERITY)`.
fn blast_wound(world: &mut World, mob: Entity, zone: BodyZone, severities: &[u8]) {
    let (kind_roll, severity) = {
        let mut rng = world.resource_mut::<CombatRng>();
        let kind = rng.range(1, 5);
        let i = rng.range(0, severities.len() as u32 - 1) as usize;
        (kind, severities[i])
    };
    let kind = match (kind_roll, severity) {
        (1, 1) => WoundKind::SlashModerate,
        (1, 2) => WoundKind::SlashSevere,
        (1, _) => WoundKind::SlashCritical,
        (2 | 3, 1) => WoundKind::Dislocation,
        (2 | 3, 2) => WoundKind::Hairline,
        (2 | 3, _) => WoundKind::Compound,
        (_, 1) => WoundKind::BurnModerate,
        (_, 2) => WoundKind::BurnSevere,
        (_, _) => WoundKind::BurnCritical,
    };
    let Some(mut limbs) = world.get::<Limbs>(mob).cloned() else {
        return;
    };
    let limb = limbs.limb_mut(zone);
    if limb.missing {
        return;
    }
    apply_wound(limb, kind);
    world.entity_mut(mob).insert(limbs);
}

/// tg `get_ear_protection(ignore_deafness = TRUE)` of a carbon: earmuffs, and vacuum around.
// shortcut: other ear-protecting headgear (`wearertargeting/earprotection`) and the ears'
// `bang_protect` are 0.
fn ear_protection(world: &World, mob: Entity) -> f32 {
    let mut protection = 0.0;
    let muffs = world
        .get::<crate::inventory::Inventory>(mob)
        .and_then(|i| i.ears)
        .and_then(|e| world.get::<DmItem>(e))
        .is_some_and(|i| is_under(&i.path, EARMUFFS));
    if muffs {
        protection += EAR_PROTECTION_NORMAL;
    }
    let pressure = world
        .get::<TilePos>(mob)
        .and_then(|&p| world.get_resource::<ss13_atmos::Atmos>()?.pressure(p));
    if pressure.is_some_and(|p| p < SOUND_MINIMUM_PRESSURE) {
        protection += EAR_PROTECTION_VACUUM;
    }
    protection
}

/// `soundbang_act(intensity, stun_pwr, damage_pwr, deafen_pwr, ignore_deafness = TRUE,
/// send_sound = FALSE)`: `stun` seconds (tg `stun_pwr`) of knockdown and a tenth of it of
/// paralysis, then the ears take damage and ring for `deafen` seconds, all scaled by how little
/// the protection covers. Returns that scale (tg's return value), 0 when fully protected.
pub fn soundbang_act(
    world: &mut World,
    mob: Entity,
    intensity: f32,
    stun: f32,
    damage: f32,
    deafen: f32,
) -> f32 {
    let protection = ear_protection(world, mob);
    if protection >= intensity {
        return 0.0;
    }
    let effect = if protection > 0.0 {
        1.0 - protection / intensity
    } else {
        1.0 - protection
    };
    if stun > 0.0 {
        crate::status_effects::apply(
            world,
            mob,
            crate::status_effects::Incap::Paralyze,
            stun * effect * 0.1,
        );
        crate::status_effects::knockdown(world, mob, stun * effect, 0.0);
    }
    let Some(ears) = organs::organ(world, mob, Slot::Ears) else {
        return effect;
    };
    if damage <= 0.0 && deafen <= 0.0 {
        return effect;
    }
    // `sound_damage`: organ damage, then `adjust_temporary_deafness`.
    organs::damage_organ(world, ears, damage * effect, f32::INFINITY);
    if let Some(mut organ) = world.get_mut::<organs::Organ>(ears) {
        organ.deafness = (organ.deafness + deafen * effect).max(0.0);
    }
    let Some(state) = organs::state(world, mob, Slot::Ears).cloned() else {
        return effect;
    };
    let tell = |world: &mut World, span, text: &str| {
        if let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) {
            crate::tools::tell(world, client, span, text);
        }
    };
    let deafened =
        state.damage >= 15.0 && world.resource_mut::<CombatRng>().chance(state.damage - 5.0);
    if deafened {
        tell(world, Span::Danger, "Вы ничего не слышите!");
        organs::set_organ_damage(world, mob, Slot::Ears, state.spec().max_health);
    } else if state.damage >= 5.0 {
        let text = if state.damage >= 15.0 {
            "У вас начинает очень сильно звенеть в ушах!"
        } else {
            "У вас начинает звенеть в ушах!"
        };
        tell(world, Span::Warning, text);
    }
    effect
}

/// tg `ex_act` of the turf of `pos`.
pub fn ex_act_turf(world: &mut World, pos: TilePos, severity: Severity) {
    let Some(turf) = turf_at(world, pos) else {
        return;
    };
    let Some(path) = path_of(world, turf) else {
        return;
    };
    if is_under(&path, WALL) {
        let hardness = if is_under(&path, R_WALL) { 10.0 } else { 40.0 };
        match severity {
            Severity::Devastate => {
                scrape_away(world, pos, false);
            }
            Severity::Heavy => {
                let devastated = world.resource_mut::<CombatRng>().chance(50.0);
                dismantle_wall(world, turf, pos, devastated);
            }
            Severity::Light => {
                if world.resource_mut::<CombatRng>().chance(hardness) {
                    dismantle_wall(world, turf, pos, false);
                }
            }
        }
    } else if is_under(&path, FLOOR) {
        ex_act_floor(world, pos, severity);
    } else {
        crate::mining::rocks::ex_act(world, pos, severity);
    }
}

/// `/turf/open/floor/is_explosion_shielded`: a dense object on the floor shields it from all
/// but devastation.
fn shielded(world: &World, pos: TilePos, severity: Severity) -> bool {
    if severity >= Severity::Devastate {
        return false;
    }
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| world.get::<DmObj>(e).is_some() && world.get::<Density>(e).is_some())
}

/// `/turf/open/floor/ex_act`.
// shortcut: `lattice_underneath` is always TRUE (tg clears it only for plating built over a
// lattice that was then removed).
fn ex_act_floor(world: &mut World, pos: TilePos, severity: Severity) {
    if shielded(world, pos, severity) {
        return;
    }
    let hotspot = |world: &mut World| {
        if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
            atmos.hotspot_expose(pos, 1000.0, CELL_VOLUME, false);
        }
    };
    match severity {
        Severity::Devastate => scrape(world, pos, 2),
        Severity::Heavy => match world.resource_mut::<CombatRng>().range(1, 3) {
            1 => {
                // `attempt_lattice_replacement` when two layers down is not a floor.
                scrape(world, pos, 2);
                let floor = turf_at(world, pos)
                    .and_then(|t| path_of(world, t))
                    .is_some_and(|p| is_under(&p, FLOOR));
                if !floor {
                    spawn_obj(world, LATTICE, pos);
                }
                if world.resource_mut::<CombatRng>().chance(33.0) {
                    spawn_stack(world, IRON, pos, 1);
                }
            }
            2 => scrape(world, pos, 2),
            _ => {
                if world.resource_mut::<CombatRng>().chance(80.0) {
                    scrape(world, pos, 1);
                } else {
                    crate::turf_damage::break_tile(world, pos);
                }
                hotspot(world);
                if world.resource_mut::<CombatRng>().chance(33.0) {
                    spawn_stack(world, IRON, pos, 1);
                }
            }
        },
        Severity::Light => {
            if world.resource_mut::<CombatRng>().chance(50.0) {
                crate::turf_damage::break_tile(world, pos);
                hotspot(world);
            }
        }
    }
}

fn scrape(world: &mut World, pos: TilePos, layers: usize) {
    for _ in 0..layers {
        scrape_away(world, pos, true);
    }
}

/// tg `dismantle_wall(devastated, explode)`: `break_wall` (sheets and girder) or
/// `devastate_wall` (sheets, and iron for the girder), then `ScrapeAway`.
fn dismantle_wall(world: &mut World, wall: Entity, pos: TilePos, devastated: bool) {
    let Some(build) = world.get::<DmTurfBuild>(wall).cloned() else {
        return;
    };
    if scrape_away(world, pos, false).is_none() {
        return;
    }
    if !build.sheet_type.is_empty() {
        spawn_stack(world, &build.sheet_type, pos, build.sheet_amount);
    }
    if !build.girder_type.is_empty() {
        if devastated {
            spawn_stack(world, IRON, pos, 1);
        } else {
            spawn_obj(world, &build.girder_type, pos);
        }
    }
}

/// `/obj/structure/lattice/atom_deconstruct`: its `build_material` (rods), one for a lattice,
/// two for a catwalk.
pub fn lattice_debris(world: &mut World, path: &str, pos: TilePos) {
    let mats = if is_under(path, CATWALK) { 2 } else { 1 };
    spawn_stack(world, RODS, pos, mats);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bomb_armor_scales_object_damage() {
        assert_eq!(through_armor(200.0, 0.0), 200.0);
        assert_eq!(through_armor(200.0, 10.0), 180.0);
        assert_eq!(through_armor(55.0, 30.0), 38.5);
        assert!(through_armor(f32::INFINITY, 90.0).is_infinite());
        assert_eq!(through_armor(100.0, 100.0), 0.0);
    }

    #[test]
    fn human_numbers_by_severity() {
        let d = HumanBlast::of(Severity::Devastate);
        assert_eq!(
            (d.brute, d.burn, d.max_wounds, d.dismember_prob),
            (300.0, 300.0, 5, 50.0)
        );
        let l = HumanBlast::of(Severity::Light);
        assert_eq!((l.brute, l.burn, l.unconscious), (30.0, 0.0, 0.0));
        assert_eq!(l.wound_severities, &[1, 2]);
        assert_eq!(Severity::Light.worse(), Severity::Heavy);
        assert_eq!(Severity::Devastate.worse(), Severity::Devastate);
    }
}
