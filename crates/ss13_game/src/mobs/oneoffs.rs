//! One-off basic mobs (server): the cytology vatbeast (`cytology/vatbeast.dm`), the festivus
//! pole (`festivus_pole.dm`), the living pine tree (`tree.dm`) and the NPC revolutionary
//! (`revolutionary.dm`). Their trees are in `crate::mobs::ai::trees`.
//!
//! - Vatbeast: fights back and hunts strangers; after 10 s on one target its AI slaps it with
//!   a tentacle (30 brute, thrown 6 tiles along its facing, 12 s cooldown); fries tame it (30%,
//!   `tameable` in `crate::mobs::pets`), and a tamed one is neutral.
//! - Pine tree: its bites knock down for 5 s one time in seven (+50% and +5 s on a carbon
//!   holding a chainsaw, hatchet or wood); it turns up to 9 moles of its tile's CO2 into O2
//!   (13% a second); it leaves a wooden plank.
//! - Festivus pole: petted, it crackles and charges every cell within 2 tiles (APCs', borgs',
//!   loose cells') by 7.5% of a standard cell; it leaves a rod.
//! - Revolutionary: a random gender, name and cause, a scream for its death sound, and its
//!   slogans with monkey screeches.
// Left out: growing the vatbeast in a vat from cell samples (`swabable`, cytology belongs to the
// science group), riding it, the slap for a rider; the pole's APC-charging AI
// (`charge_apc`); the revolutionary's human look, corpse and weapon hit sound; the trees'
// `seethrough_mob`, `aggro_emote`.

use std::sync::Arc;

use bevy::prelude::*;
use ss13_atmos::{Atmos, GasId};
use ss13_mapload::fidelity::is_under;
use ss13_mapload::mobs::{MobDef, MobSpeech};
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};
use ss13_shared::{Dir, Facing, TilePos};

use super::{BasicMob, Faction};
use crate::combat::CombatRng;
use crate::health::{DEFAULT_MESSAGE_RANGE, DamageType, Stat};
use crate::status_effects::Incap;

pub const VATBEAST: &str = "/mob/living/basic/vatbeast";
pub const TREE: &str = "/mob/living/basic/tree";
pub const FESTIVUS: &str = "/mob/living/basic/festivus";
pub const REVOLUTIONARY: &str = "/mob/living/basic/revolutionary";

/// What they leave (`death_drops`, `GAME_RUNTIME_TYPES`).
pub const WOOD: &str = "/obj/item/stack/sheet/mineral/wood";
pub const RODS: &str = "/obj/item/stack/rods";

/// `tentacle_slap`: `cooldown_time`, the damage, the throw.
const SLAP_COOLDOWN: f64 = 12.0;
const SLAP_DAMAGE: f32 = 30.0;
const SLAP_RANGE: u32 = 6;
const SLAP_SPEED: f32 = 4.0;
/// `bb_key_at_least BB_BASIC_MOB_HAS_TARGET_TIME 10 SECONDS`.
const SLAP_AFTER: f64 = 10.0;
const SLAP_SOUND: &str = "sound/effects/emotes/assslap.ogg";

/// The pine tree's `paralyze_prob`, `paralyze_value` and `anger_boost`.
const PARALYZE_PROB: f32 = 15.0;
const PARALYZE_SECS: f32 = 5.0;
const ANGER_BOOST: f32 = 50.0;
const INFURIATING: &[&str] = &["/obj/item/chainsaw", "/obj/item/hatchet", WOOD];

/// `FESTIVUS_RECHARGE_VALUE`: `0.075 * STANDARD_CELL_CHARGE`.
const FESTIVUS_CHARGE: f64 = 0.075 * crate::inducer::STANDARD_CELL_CHARGE;

const SLOGANS: &[&str] = &[
    "The revolution will not be televized!",
    "VIVA!",
    "Dirty pig!",
    "Gondola meat is murder!",
    "Free Cargonia!",
    "Mime rights are human rights!",
    "猫娘 Free Terry!",
];
const CAUSES: &[&str] = &[
    "Worker's rights",
    "Icemoon climate change",
    "Fair clown treatment",
    "Lizards",
    "Moths",
    "Stop Lavaland drilling",
    "The Captain has been replaced by a robot",
    "Free Cargonia",
    "Befriend all space dragons",
    "The Grey Tide",
    "Rising cost of medbay",
];
const MONKEY: &[&str] = &[
    "sound/mobs/non-humanoids/monkey/monkey_screech_1.ogg",
    "sound/mobs/non-humanoids/monkey/monkey_screech_2.ogg",
    "sound/mobs/non-humanoids/monkey/monkey_screech_3.ogg",
    "sound/mobs/non-humanoids/monkey/monkey_screech_4.ogg",
];
const MALE: &[&str] = &[
    "sound/mobs/humanoids/human/scream/malescream_1.ogg",
    "sound/mobs/humanoids/human/scream/malescream_2.ogg",
    "sound/mobs/humanoids/human/scream/malescream_3.ogg",
    "sound/mobs/humanoids/human/scream/malescream_4.ogg",
    "sound/mobs/humanoids/human/scream/malescream_5.ogg",
];
const FEMALE: &[&str] = &[
    "sound/mobs/humanoids/human/scream/femalescream_1.ogg",
    "sound/mobs/humanoids/human/scream/femalescream_2.ogg",
    "sound/mobs/humanoids/human/scream/femalescream_3.ogg",
    "sound/mobs/humanoids/human/scream/femalescream_4.ogg",
    "sound/mobs/humanoids/human/scream/femalescream_5.ogg",
];

/// The vatbeast's `tentacle_slap` and how long it has been on its target.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct TentacleSlap {
    pub ready_at: f64,
    /// The target it is attacking, and since when.
    pub on: Option<(Entity, f64)>,
}

#[derive(Resource, Default)]
struct TreeClock(f64);

pub(super) fn build(app: &mut App) {
    app.init_resource::<TreeClock>()
        .add_systems(Update, photosynthesis.after(crate::health::HealthSystems));
}

use crate::tools::now;

fn roll(world: &mut World, percent: f32) -> bool {
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let hit = rng.chance(percent);
    world.resource_mut::<CombatRng>().0 = rng.0;
    hit
}

fn pick<'a>(world: &mut World, list: &'a [&'a str]) -> &'a str {
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let i = rng.range(0, list.len().saturating_sub(1) as u32) as usize;
    world.resource_mut::<CombatRng>().0 = rng.0;
    list[i]
}

fn visible(world: &mut World, pos: TilePos, span: Span, text: String) {
    let clients = crate::obj_damage::clients_in_range(world, pos, DEFAULT_MESSAGE_RANGE, &[]);
    world.write_message(SendChat {
        clients,
        message: ChatMessage::simple(ChatChannel::Info, span, text),
    });
}

pub(super) fn attach(world: &mut World, mob: Entity, def: &MobDef) {
    if is_under(&def.path, VATBEAST) {
        world.entity_mut(mob).insert(TentacleSlap::default());
    } else if is_under(&def.path, REVOLUTIONARY) {
        revolutionary(world, mob, def);
    }
}

/// `/mob/living/basic/revolutionary/Initialize`.
fn revolutionary(world: &mut World, mob: Entity, def: &MobDef) {
    use super::pets::Gender;
    // `pick(MALE, FEMALE, PLURAL)`.
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let g = rng.range(0, 2);
    world.resource_mut::<CombatRng>().0 = rng.0;
    let names = world
        .get_resource::<crate::jobs::Jobs>()
        .map(|j| j.data.names.clone())
        .unwrap_or_default();
    let (gender, firsts, screams): (Gender, Vec<String>, Vec<&str>) = match g {
        0 => (
            Gender::Male,
            names.first_male.clone(),
            [MALE, MONKEY].concat(),
        ),
        1 => (
            Gender::Female,
            names.first_female.clone(),
            [FEMALE, MONKEY].concat(),
        ),
        // `GLOB.first_names` (both lists); the plural body takes the male one here.
        _ => (
            Gender::Male,
            [names.first_male.clone(), names.first_female.clone()].concat(),
            [MALE, FEMALE, MONKEY].concat(),
        ),
    };
    let scream = pick(world, &screams).to_owned();
    let cause = pick(world, CAUSES).to_owned();
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let first = (!firsts.is_empty())
        .then(|| firsts[rng.range(0, firsts.len() as u32 - 1) as usize].clone());
    let last = (!names.last.is_empty())
        .then(|| names.last[rng.range(0, names.last.len() as u32 - 1) as usize].clone());
    world.resource_mut::<CombatRng>().0 = rng.0;
    let mut own = def.clone();
    own.death_sound = scream;
    own.speech_lines = Some(MobSpeech {
        chance: 15.0,
        speak: SLOGANS.iter().map(|s| (*s).to_owned()).collect(),
        sound: MONKEY.iter().map(|s| (*s).to_owned()).collect(),
        ..default()
    });
    let Some(basic) = world.get::<BasicMob>(mob).cloned() else {
        return;
    };
    let mut e = world.entity_mut(mob);
    e.insert((
        BasicMob {
            def: Arc::new(own),
            ..basic
        },
        gender,
    ));
    if let (Some(first), Some(last)) = (first, last) {
        e.insert(Name::new(format!("{first} {last}")));
    }
    if let Some(mut dm) = e.get_mut::<ss13_mapload::DmName>() {
        dm.desc = format!("{}\nToday, that cause is: #{cause}.", dm.desc);
    }
}

/// `death_drops` of the pine tree and the festivus pole.
pub(super) fn on_death(world: &mut World, def: &MobDef, pos: TilePos) {
    let drop = if is_under(&def.path, TREE) {
        WOOD
    } else if is_under(&def.path, FESTIVUS) {
        RODS
    } else {
        return;
    };
    crate::construction::spawn_obj(world, drop, pos);
}

/// The vatbeast AI's `targeted_mob_ability/melee` instead of its attack: true when it slapped.
pub fn pre_melee(world: &mut World, attacker: Entity, target: Entity) -> bool {
    let Some(slap) = world.get::<TentacleSlap>(attacker).copied() else {
        return false;
    };
    if world.get::<crate::server::PlayerMob>(attacker).is_some() {
        return false;
    }
    let now = now(world);
    let since = match slap.on {
        Some((t, since)) if t == target => since,
        _ => {
            world.entity_mut(attacker).insert(TentacleSlap {
                on: Some((target, now)),
                ..slap
            });
            return false;
        }
    };
    if now - since < SLAP_AFTER || now < slap.ready_at {
        return false;
    }
    tentacle_slap(world, attacker, target);
    if let Some(mut s) = world.get_mut::<TentacleSlap>(attacker) {
        s.ready_at = now + SLAP_COOLDOWN;
    }
    true
}

/// `/datum/action/cooldown/tentacle_slap/Activate`.
pub fn tentacle_slap(world: &mut World, beast: Entity, victim: Entity) {
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(beast), world.get::<TilePos>(victim))
    else {
        return;
    };
    let (who, whom) = (
        super::defense::name_of(world, beast),
        super::defense::name_of(world, victim),
    );
    visible(
        world,
        to,
        Span::Warning,
        format!("{who} шлёпает {whom} щупальцем!"),
    );
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(from, SLAP_SOUND, 90.0, false),
    );
    // `get_edge_target_turf(victim, owner.dir)`: as far as the throw reaches along the facing.
    let dir = world.get::<Facing>(beast).map_or(Dir::South, |f| f.0);
    let (dx, dy) = dir.offset();
    let edge = TilePos::new(
        to.x + dx * SLAP_RANGE as i32,
        to.y + dy * SLAP_RANGE as i32,
        to.z,
    );
    crate::throw::throw_at(world, victim, edge, SLAP_RANGE, SLAP_SPEED);
    super::defense::hurt(world, victim, DamageType::Brute, SLAP_DAMAGE, None);
    super::ai::record_attacker(world, victim, beast);
}

/// `/mob/living/basic/tree/melee_attack` after a landed bite.
pub fn after_melee(world: &mut World, attacker: Entity, target: Entity) {
    let is_tree = world
        .get::<BasicMob>(attacker)
        .is_some_and(|b| is_under(&b.def.path, TREE));
    if !is_tree {
        return;
    }
    // `iscarbon(victim) && victim.is_holding_item_of_types(infuriating_objects)`.
    let angry = world
        .get::<crate::inventory::Inventory>(target)
        .is_some_and(|inv| {
            inv.hands.iter().flatten().any(|&h| {
                world
                    .get::<ss13_mapload::DmItem>(h)
                    .is_some_and(|d| INFURIATING.iter().any(|r| is_under(&d.path, r)))
            })
        });
    let boost = if angry { ANGER_BOOST } else { 0.0 };
    if !roll(world, PARALYZE_PROB + boost) {
        return;
    }
    // `Paralyze(paralyze_value + boost)`: the boost is deciseconds there.
    crate::status_effects::apply(world, target, Incap::Paralyze, PARALYZE_SECS + boost / 10.0);
    if let Some(&pos) = world.get::<TilePos>(target) {
        let (who, whom) = (
            super::defense::name_of(world, attacker),
            super::defense::name_of(world, target),
        );
        visible(
            world,
            pos,
            Span::Danger,
            format!("{} сбивает с ног {whom}!", capitalize(&who)),
        );
    }
}

use ss13_chat::capitalize;

/// The festivus pole's `attack_hand` out of combat mode: every cell within 2 tiles charges.
pub fn festivus_touch(world: &mut World, pole: Entity) {
    let is_pole = world
        .get::<BasicMob>(pole)
        .is_some_and(|b| is_under(&b.def.path, FESTIVUS));
    let Some(&pos) = world.get::<TilePos>(pole).filter(|_| is_pole) else {
        return;
    };
    let name = super::defense::name_of(world, pole);
    visible(
        world,
        pos,
        Span::Warning,
        format!(
            "{} потрескивает от статического электричества!",
            capitalize(&name)
        ),
    );
    let near: Vec<Entity> = world
        .resource::<ss13_world::SpatialIndex>()
        .in_radius(pos, 2)
        .collect();
    for e in near {
        if let Some(mut cell) = world.get_mut::<crate::mecha::core::ItemCell>(e) {
            cell.0.give(FESTIVUS_CHARGE);
        }
        if let Some(mut apc) = world.get_mut::<crate::power::Apc>(e)
            && let Some(cell) = apc.cell.as_mut()
        {
            cell.give(FESTIVUS_CHARGE);
        }
        let borg_cell = world
            .get::<crate::silicon::chassis::Chassis>(e)
            .and_then(|c| c.cell);
        if let Some(cell) = borg_cell
            && let Some(mut c) = world.get_mut::<crate::mecha::core::ItemCell>(cell)
        {
            c.0.give(FESTIVUS_CHARGE);
        }
    }
}

/// The pine tree's `Life`: `SPT_PROB(13)` to turn up to 9 moles of CO2 into O2.
fn photosynthesis(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs_f64();
    let fires = {
        let mut clock = world.resource_mut::<TreeClock>();
        clock.0 += dt;
        let n = (clock.0 / crate::breath::SSMOBS_WAIT_SECS).floor();
        clock.0 -= n * crate::breath::SSMOBS_WAIT_SECS;
        n as u32
    };
    if fires == 0 {
        return;
    }
    let trees: Vec<TilePos> = world
        .query::<(&BasicMob, &TilePos, &Stat)>()
        .iter(world)
        .filter(|(b, _, s)| **s != Stat::Dead && is_under(&b.def.path, TREE))
        .map(|(_, p, _)| *p)
        .collect();
    let seconds = crate::breath::SSMOBS_WAIT_SECS * f64::from(fires);
    // `SPT_PROB(13, seconds_per_tick)`.
    let chance = (1.0 - (1.0 - 0.13_f64).powf(seconds)) * 100.0;
    for pos in trees {
        if !roll(world, chance as f32) {
            continue;
        }
        let Some(mut atmos) = world.get_resource_mut::<Atmos>() else {
            return;
        };
        atmos.modify_air(pos, |air| {
            let amount = air.get(GasId::CO2).min(9.0);
            if amount > 0.0 {
                air.add(GasId::CO2, -amount);
                air.add(GasId::O2, amount);
            }
        });
    }
}

/// `vatbeast/tamed`: neutral from then on (riding is left out).
pub fn vatbeast_tamed(world: &mut World, mob: Entity) {
    world
        .entity_mut(mob)
        .insert(Faction(vec![Faction::NEUTRAL.to_owned()]));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_are_runtime_types() {
        let registry = ss13_mapload::runtime::runtime_types();
        assert!(registry.contains(WOOD) && registry.contains(RODS));
    }

    #[test]
    fn the_slap_waits_ten_seconds_on_one_target() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let beast = world.spawn(TentacleSlap::default()).id();
        let victim = world.spawn_empty().id();
        // The first attack only starts the timer.
        assert!(!pre_melee(&mut world, beast, victim));
        let slap = world.get::<TentacleSlap>(beast).unwrap();
        assert_eq!(slap.on, Some((victim, 0.0)));
        // Under 10 s on it: still a plain attack.
        assert!(!pre_melee(&mut world, beast, victim));
        // A new target restarts the timer.
        let other = world.spawn_empty().id();
        assert!(!pre_melee(&mut world, beast, other));
        assert_eq!(
            world.get::<TentacleSlap>(beast).unwrap().on,
            Some((other, 0.0))
        );
    }

    #[test]
    fn photosynthesis_odds_per_ssmobs_tick() {
        let chance = (1.0 - (1.0 - 0.13_f64).powf(2.0)) * 100.0;
        assert!((chance - 24.31).abs() < 0.01);
    }
}
