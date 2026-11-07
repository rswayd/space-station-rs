//! Basic mobs (server), after tg `/mob/living/basic` (`code/modules/mob/living/basic/basic.dm`,
//! `basic_defense.dm`, `damage_procs.dm`) and their AI (`ai`, tg `/datum/ai_controller`).
//!
//! A prototype with `ss13_mapload::DmMob` (every `/mob/living/basic` type; map placements and
//! runtime spawns alike) becomes a basic mob from its `mobs.ron` definition (`ss13-mapload
//! convert`): [`BasicMob`], a basic [`Health`] (one brute pool scaled by `physiology`, dead at
//! 0, no crit), [`Faction`], a body temperature, a `Mover` at its `speed` and, when its
//! `ai_controller` is ported, an [`ai::AiController`].
//!
//! - Every `SSmobs` fire (2 s): tg `atmos_requirements` (gas moles of the tile against
//!   `habitable_atmos`, `unsuitable_atmos_damage` per second as brute) and `body_temp_sensitive`
//!   (the body temperature follows the tile air, `/mob/living/handle_environment`; burn outside
//!   `minimum/maximum_survivable_temperature`).
//! - Death (tg `death` / `look_dead`): `icon_dead`, not dense (unless `REMAIN_DENSE_WHILE_DEAD`),
//!   the deathgasp "перестает двигаться..."; `DEL_ON_DEATH` mobs vanish; a mouse leaves a dead
//!   mouse item. A revived mob looks alive again.
//! - `defense`: humans' hands and items on basic mobs (tg `attack_hand`, `attacked_by`), basic
//!   mob melee (tg `melee_attack` -> `attack_animal`).
//! - `spawn`: round-start vermin on exposed wires (tg `SSminor_mapping.trigger_migration`).
//! - `pets` (friends, taming, pet commands, breeding), `farm` (udders, tipping, eggs, chicks),
//!   `parrot` (speech learning), `critters` (crabs, turtles), `ranged` (shooting mobs),
//!   `troopers` (NPC soldiers' guns and drops),
//!   `lavaland` (the mining level's fauna, their abilities and cave population).
//!   `megafauna` (the lavaland bosses).

pub mod ai;
pub mod constructs;
pub mod critters;
pub mod defense;
pub mod eating;
pub mod farm;
pub mod illusion;
pub mod lavaland;
pub mod megafauna;
pub mod oneoffs;
pub mod parrot;
pub mod pets;
pub mod ranged;
pub mod spawn;
pub mod trader;
pub mod troopers;
pub mod wisdoms;

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use ss13_atmos::{Atmos, GasId};
use ss13_mapload::mobs::{MOBS_FILE, MobDef, MobSpeech, MobsData};
use ss13_mapload::{DmMob, DmName, IconIndex, MapLoadPlugin};
use ss13_net::chat::server::SendChat;
use ss13_shared::{Appearance, TilePos};
use ss13_world::{Density, Mover};

use crate::breath::SSMOBS_WAIT_SECS;
use crate::combat::{CombatRng, NextMove};
use crate::health::{DEFAULT_MESSAGE_RANGE, DamageType, Health, HealthSystems, Stat, update_stat};

/// `DEL_ON_DEATH`, `FLIP_ON_DEATH`, `REMAIN_DENSE_WHILE_DEAD` (`basic_mob_flags`).
pub const DEL_ON_DEATH: u32 = 1 << 0;
pub const REMAIN_DENSE_WHILE_DEAD: u32 = 1 << 2;
/// `BODYTEMP_NORMAL`, `BODYTEMP_DIVISOR`, `BODYTEMP_COOLING_MAX`, `BODYTEMP_HEATING_MAX`.
const BODYTEMP_NORMAL: f64 = 310.15;
const BODYTEMP_DIVISOR: f64 = 16.0;
const BODYTEMP_COOLING_MAX: f64 = -30.0;
const BODYTEMP_HEATING_MAX: f64 = 30.0;
/// `PLUOXIUM_PROPORTION`.
const PLUOXIUM_PROPORTION: f64 = 8.0;
const MOUSE_ROOT: &str = "/mob/living/basic/mouse";
const DEAD_MOUSE: &str = "/obj/item/food/deadmouse";

/// Every `mobs.ron` definition by DM path.
#[derive(Resource, Default)]
pub struct MobDefs(pub HashMap<String, Arc<MobDef>>);

/// The `random_speech` types of `mobs.ron` by DM path (`MobsData::speech`), for the speech
/// leaves of the AI.
#[derive(Resource, Default)]
pub struct MobSpeeches(pub HashMap<String, Arc<MobSpeech>>);

/// A basic mob: its definition and its living/dead sprites (a mouse's body color picks them).
#[derive(Component, Clone, Debug)]
pub struct BasicMob {
    pub def: Arc<MobDef>,
    pub icon_living: String,
    pub icon_dead: String,
    /// GAGS carp (`greyscale_config/carp`): the body tinted, the `_mouth` layer over it.
    pub mouth: bool,
}

/// `GLOB.carp_colors` with their weights.
const CARP_COLORS: [([u8; 3], u32); 15] = [
    ([0xab, 0xa2, 0xff], 7),
    ([0xda, 0x77, 0xa8], 7),
    ([0x70, 0xff, 0x25], 7),
    ([0xdf, 0x0a, 0xfb], 7),
    ([0xe5, 0xe7, 0x5a], 7),
    ([0x04, 0xe1, 0xed], 7),
    ([0xca, 0x80, 0x5a], 7),
    ([0x20, 0xe2, 0x8e], 7),
    ([0x4d, 0x88, 0xcc], 7),
    ([0xdd, 0x5f, 0x34], 7),
    ([0xfd, 0x67, 0x67], 7),
    ([0xf3, 0xca, 0x4a], 7),
    ([0x09, 0xba, 0xe1], 7),
    ([0x7e, 0xf0, 0x99], 7),
    ([0xfd, 0xfb, 0xf3], 1),
];
const CARP_DMI: &str = "icons/mob/simple/carp.dmi";

/// tg `faction`. A mob without one is `neutral` (tg `/mob` default), as humans are.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Faction(pub Vec<String>);

impl Faction {
    pub const NEUTRAL: &'static str = "neutral";
}

/// tg `faction_check_atom` (not exact): any shared faction.
pub fn same_faction(world: &World, a: Entity, b: Entity) -> bool {
    let of = |e: Entity| {
        world
            .get::<Faction>(e)
            .map_or_else(|| vec![Faction::NEUTRAL.to_owned()], |f| f.0.clone())
    };
    let fb = of(b);
    of(a).iter().any(|f| fb.contains(f)) || pets::allied(world, a, b)
}

/// tg `bodytemperature` of a basic mob, kelvin.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct BodyTemperature(pub f64);

/// The `SSmobs` clock of basic mobs.
#[derive(Resource, Default)]
struct MobsClock(f64);

pub struct MobsServerPlugin;

impl Plugin for MobsServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MobsClock>()
            .add_observer(on_mob_added)
            .add_systems(
                Update,
                (
                    life.before(HealthSystems),
                    (
                        look_dead_or_alive,
                        defense::squeak_on_cross,
                        ranged::fire_bursts,
                    )
                        .after(HealthSystems),
                ),
            )
            .add_systems(ss13_world::MapInit, attach_mapped)
            .add_systems(
                PostStartup,
                spawn::roundstart_vermin.after(ss13_world::run_map_init),
            );
        defense::build(app);
        ai::build(app);
        farm::build(app);
        pets::build(app);
        parrot::build(app);
        critters::build(app);
        illusion::build(app);
        trader::build(app);
        oneoffs::build(app);
        lavaland::build(app);
        megafauna::build(app);
    }
}

/// The definitions, read from `<assets>/mobs.ron` once (empty without it: the mobs stay
/// pictures).
pub fn mob_defs(world: &mut World) -> &MobDefs {
    if !world.contains_resource::<MobDefs>() {
        let path = world
            .get_resource::<MapLoadPlugin>()
            .map(|m| m.assets.join(MOBS_FILE));
        let (defs, speech) = match path.as_deref().map(MobsData::read) {
            Some(Ok(data)) => {
                info!("mobs: {} basic mob types", data.mobs.len());
                (
                    data.mobs
                        .into_iter()
                        .map(|(k, v)| (k, Arc::new(v)))
                        .collect(),
                    data.speech
                        .into_iter()
                        .map(|(k, v)| (k, Arc::new(v)))
                        .collect(),
                )
            }
            Some(Err(e)) => {
                warn!("{e:#} (rerun `ss13-mapload convert`): basic mobs do nothing");
                (HashMap::new(), HashMap::new())
            }
            None => (HashMap::new(), HashMap::new()),
        };
        world.insert_resource(MobDefs(defs));
        if !world.contains_resource::<MobSpeeches>() {
            world.insert_resource(MobSpeeches(speech));
        }
    }
    world.resource::<MobDefs>()
}

/// A mob spawned after the map load becomes a basic mob at once; mapped ones wait for
/// [`attach_mapped`], after the load's fidelity pass rebuilt their looks.
fn on_mob_added(
    add: On<Add<DmMob>>,
    loaded: Option<Res<ss13_mapload::MapLoadReport>>,
    mut commands: Commands,
) {
    if loaded.is_none() {
        return;
    }
    let entity = add.entity;
    commands.queue(move |world: &mut World| attach(world, entity));
}

fn attach_mapped(world: &mut World) {
    let mobs: Vec<Entity> = world
        .query_filtered::<Entity, (With<DmMob>, Without<BasicMob>, With<ss13_world::Mapped>)>()
        .iter(world)
        .collect();
    for mob in mobs {
        attach(world, mob);
    }
}

/// tg `/mob/living/basic/Initialize` for what is ported.
fn attach(world: &mut World, entity: Entity) {
    let Some(path) = world.get::<DmMob>(entity).map(|m| m.path.clone()) else {
        return;
    };
    let Some(def) = mob_defs(world).0.get(&path).cloned() else {
        return;
    };
    if world.get::<BasicMob>(entity).is_some() {
        return;
    }
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    // BandaStation mice: `animal_variety` with a random `body_color`.
    let (living, dead) = if def.body_icon_state.is_empty() {
        (def.icon_living.clone(), def.icon_dead.clone())
    } else {
        let color = if def.body_color.is_empty() && !def.possible_body_colors.is_empty() {
            let i = rng.range(0, def.possible_body_colors.len() as u32 - 1) as usize;
            def.possible_body_colors[i].clone()
        } else {
            def.body_color.clone()
        };
        let base = format!("{}_{color}", def.body_icon_state);
        (base.clone(), format!("{base}_dead"))
    };
    // tg carp `apply_colour`: `pick_weight(GLOB.carp_colors)` into the GAGS config (one tinted
    // layer under the mouth).
    let mouth = def.icon == CARP_DMI && def.icon_living == "base";
    let tint = mouth.then(|| {
        let total: u32 = CARP_COLORS.iter().map(|c| c.1).sum();
        let mut roll = rng.range(1, total);
        CARP_COLORS
            .iter()
            .find(|c| {
                if roll <= c.1 {
                    return true;
                }
                roll -= c.1;
                false
            })
            .map_or([255; 3], |c| c.0)
    });
    world.resource_mut::<CombatRng>().0 = rng.0;
    let name = world
        .get::<DmName>(entity)
        .map_or_else(|| def.name.clone(), |n| plain_name(&n.name));
    let mut health = Health::basic(def.max_health, def.physiology);
    if def.health < def.max_health {
        health.adjust(DamageType::Brute, def.max_health - def.health);
    }
    let mut e = world.entity_mut(entity);
    e.insert((
        BasicMob {
            def: def.clone(),
            icon_living: living.clone(),
            icon_dead: dead,
            mouth,
        },
        health,
        Stat::Conscious,
        Faction(def.faction.clone()),
        BodyTemperature(BODYTEMP_NORMAL),
        NextMove::default(),
        Name::new(name),
        // tg `simplemob_varspeed`: `speed` deciseconds per tile.
        Mover::new(def.speed.max(0.0) / 10.0),
    ));
    if let Some(controller) = ai::AiController::for_mob(&def) {
        if controller.retaliates {
            e.insert(ai::Retaliates);
        }
        e.insert(controller);
    }
    // BandaStation: carp swarm and pass tables and mobs (`TRAIT_UNDENSE`).
    if ss13_mapload::fidelity::is_under(&path, "/mob/living/basic/carp") {
        e.remove::<Density>();
    }
    if let (Some([r, g, b]), Some(mut appearance)) = (tint, world.get_mut::<Appearance>(entity)) {
        appearance.color = Some([r, g, b, 255]);
    }
    // Gender first: a goat's udder depends on it.
    pets::attach(world, entity, &def);
    farm::attach(world, entity, &def);
    parrot::attach(world, entity, &def);
    critters::attach(world, entity, &def);
    ranged::attach(world, entity, &def);
    troopers::attach(world, entity, &def);
    trader::attach(world, entity, &def);
    oneoffs::attach(world, entity, &def);
    lavaland::attach(world, entity, &def);
    megafauna::attach(world, entity, &def);
    set_look(world, entity, false);
}

/// `icon_living` or `icon_dead`, with the carp's mouth layer.
pub(crate) fn set_look(world: &mut World, entity: Entity, dead: bool) {
    let Some(basic) = world.get::<BasicMob>(entity).cloned() else {
        return;
    };
    let state = if dead {
        &basic.icon_dead
    } else {
        &basic.icon_living
    };
    set_icon_state(world, entity, &basic.def.icon, state);
    if !basic.mouth {
        return;
    }
    let mouth = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(&basic.def.icon, &format!("{state}_mouth")));
    if let Some(mut appearance) = world.get_mut::<Appearance>(entity) {
        appearance.overlays = mouth
            .map(|icon_state| ss13_shared::Overlay {
                icon_state,
                dir: None,
                color: Some([255; 4]),
                pixel_offset: [0, 0],
                plane: None,
                only_dirs: 0,
            })
            .into_iter()
            .collect();
    }
}

fn plain_name(name: &str) -> String {
    name.trim_start_matches("\\improper ")
        .trim_start_matches("\\proper ")
        .to_owned()
}

/// Points the mob's sprite at `state` of `icon`, when the manifest has it.
pub(crate) fn set_icon_state(world: &mut World, entity: Entity, icon: &str, state: &str) {
    let Some(id) = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(icon, state))
    else {
        return;
    };
    if let Some(mut appearance) = world.get_mut::<Appearance>(entity)
        && appearance.icon_state != id
    {
        appearance.icon_state = id;
    }
}

/// The air a basic mob lives in: its tile's (`None`: no air, as on a wall), or station air on
/// levels without simulated air (as `crate::breath` breathes them).
fn environment(world: &World, pos: TilePos) -> Option<ss13_atmos::GasMixture> {
    match world.get_resource::<Atmos>() {
        Some(atmos) if atmos.level(pos.z).is_some() => atmos.air(pos).cloned(),
        _ => Some(crate::breath::station_air()),
    }
}

/// tg `atmos_requirements` `is_breathable_atmos` over the tile's moles.
pub fn breathable(def: &MobDef, air: Option<&ss13_atmos::GasMixture>) -> bool {
    let Some(req) = def.habitable_atmos else {
        return true;
    };
    let Some(air) = air else {
        // No air at all (space): only fine without minimums.
        return req.min_oxy == 0.0
            && req.min_plas == 0.0
            && req.min_n2 == 0.0
            && req.min_co2 == 0.0;
    };
    let moles = |g: GasId| air.get(g);
    let oxy = moles(GasId::O2) + moles(GasId::PLUOXIUM) * PLUOXIUM_PROPORTION;
    let within = |value: f64, min: f32, max: f32| {
        let max = if max == 0.0 {
            f64::INFINITY
        } else {
            f64::from(max)
        };
        value >= f64::from(min) && value <= max
    };
    within(oxy, req.min_oxy, req.max_oxy)
        && within(moles(GasId::PLASMA), req.min_plas, req.max_plas)
        && within(moles(GasId::N2), req.min_n2, req.max_n2)
        && within(moles(GasId::CO2), req.min_co2, req.max_co2)
}

/// `/mob/living/handle_environment`: one step of the body temperature towards `loc_temp`.
pub fn body_temperature_step(body: f64, loc_temp: f64, seconds: f64) -> f64 {
    let delta = loc_temp - body;
    body + if delta < 0.0 {
        (delta / BODYTEMP_DIVISOR).max(BODYTEMP_COOLING_MAX) * seconds
    } else {
        (delta / BODYTEMP_DIVISOR).min(BODYTEMP_HEATING_MAX) * seconds
    }
    .clamp(delta.min(0.0), delta.max(0.0))
}

/// tg `SSmobs` -> `Life()` of every basic mob: atmos requirements, then body temperature.
// Cost: every 2 s one pass over basic mobs (tens on a station): a tile air read each.
fn life(world: &mut World) {
    let delta = world.resource::<Time>().delta_secs_f64();
    let fires = {
        let mut clock = world.resource_mut::<MobsClock>();
        clock.0 += delta;
        let n = (clock.0 / SSMOBS_WAIT_SECS).floor();
        clock.0 -= n * SSMOBS_WAIT_SECS;
        n as u32
    };
    for _ in 0..fires {
        let mobs: Vec<Entity> = world
            .query_filtered::<Entity, (With<BasicMob>, With<TilePos>)>()
            .iter(world)
            .collect();
        for mob in mobs {
            live(world, mob, SSMOBS_WAIT_SECS);
        }
    }
}

fn live(world: &mut World, mob: Entity, seconds: f64) {
    let e = world.entity(mob);
    let (Some(def), Some(&pos), Some(&stat), Some(&BodyTemperature(body))) = (
        e.get::<BasicMob>().map(|b| b.def.clone()),
        e.get::<TilePos>(),
        e.get::<Stat>(),
        e.get::<BodyTemperature>(),
    ) else {
        return;
    };
    if e.contains::<crate::closets::InCloset>() {
        return;
    }
    let air = environment(world, pos);
    let mut damage = Vec::new();
    // `handle_breathing` -> `atmos_requirements` (living mobs only).
    if stat != Stat::Dead && def.unsuitable_atmos_damage > 0.0 && !breathable(&def, air.as_ref()) {
        damage.push((
            DamageType::Brute,
            def.unsuitable_atmos_damage * seconds as f32,
        ));
    }
    // `handle_environment`, then `body_temp_sensitive` (`COMSIG_LIVING_LIFE`, before it in tg;
    // the order only shifts the reaction by one fire).
    let loc_temp = air
        .as_ref()
        .map_or(ss13_atmos::gas::TCMB, |a| a.temperature);
    let body = body_temperature_step(body, loc_temp, seconds);
    world.entity_mut(mob).insert(BodyTemperature(body));
    let sensitive = (def.unsuitable_cold_damage != 0.0 || def.unsuitable_heat_damage != 0.0)
        && !(def.minimum_survivable_temperature <= 0.0
            && def.maximum_survivable_temperature >= 1e30);
    if sensitive {
        if body < f64::from(def.minimum_survivable_temperature) && def.unsuitable_cold_damage > 0.0
        {
            damage.push((
                DamageType::Burn,
                def.unsuitable_cold_damage * seconds as f32,
            ));
        } else if body > f64::from(def.maximum_survivable_temperature)
            && def.unsuitable_heat_damage > 0.0
        {
            damage.push((
                DamageType::Burn,
                def.unsuitable_heat_damage * seconds as f32,
            ));
        }
    }
    if damage.is_empty() {
        return;
    }
    let mut e = world.entity_mut(mob);
    let Some(mut health) = e.get_mut::<Health>() else {
        return;
    };
    for (kind, amount) in damage {
        health.adjust(kind, amount);
    }
    let health = health.clone();
    if let Some(mut stat) = e.get_mut::<Stat>() {
        let mut new = *stat;
        update_stat(&health, &mut new);
        stat.set_if_neq(new);
    }
}

/// tg `death` -> `look_dead` and `revive` -> `look_alive`, on the stat's change.
fn look_dead_or_alive(world: &mut World) {
    let changed: Vec<(Entity, Stat)> = world
        .query_filtered::<(Entity, Ref<Stat>), With<BasicMob>>()
        .iter(world)
        .filter(|(_, s)| s.is_changed() && !s.is_added())
        .map(|(e, s)| (e, *s))
        .collect();
    for (mob, stat) in changed {
        let Some(basic) = world.get::<BasicMob>(mob).cloned() else {
            continue;
        };
        let flags = basic.def.basic_mob_flags;
        if stat == Stat::Dead {
            death(world, mob, &basic);
        } else {
            set_look(world, mob, false);
            if basic.def.density && flags & REMAIN_DENSE_WHILE_DEAD == 0 {
                world.entity_mut(mob).insert(Density);
            }
        }
    }
}

/// tg `/mob/living/basic/death`.
fn death(world: &mut World, mob: Entity, basic: &BasicMob) {
    let def = &basic.def;
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let name = world
        .get::<Name>(mob)
        .map_or_else(|| def.name.clone(), |n| n.as_str().to_owned());
    // `/mob/living/death`: the deathgasp emote with a death sound or message.
    if !def.death_sound.is_empty() || !def.death_message.is_empty() {
        let text = if def.death_message.is_empty() {
            "перестает двигаться...".to_owned()
        } else {
            def.death_message.clone()
        };
        let clients = crate::obj_damage::clients_in_range(world, pos, DEFAULT_MESSAGE_RANGE, &[]);
        world.write_message(SendChat {
            clients,
            message: ss13_chat::server::emote_message(&name, &text),
        });
    }
    lavaland::on_death(world, mob, def, pos);
    megafauna::on_death(world, mob, pos);
    troopers::on_death(world, def, pos);
    constructs::on_death(world, def, pos);
    trader::on_death(world, def, pos);
    oneoffs::on_death(world, def, pos);
    // A mouse leaves its corpse as a food item (`/obj/item/food/deadmouse`, `copy_corpse`).
    if ss13_mapload::fidelity::is_under(&def.path, MOUSE_ROOT) {
        if let Some(corpse) = crate::construction::spawn_obj(world, DEAD_MOUSE, pos) {
            let id = world
                .get_resource::<IconIndex>()
                .and_then(|i| i.get(&def.icon, &basic.icon_dead));
            let mut c = world.entity_mut(corpse);
            c.insert(Name::new(name));
            if let (Some(id), Some(mut appearance)) = (id, c.get_mut::<Appearance>()) {
                appearance.icon_state = id;
            }
        }
        world.despawn(mob);
        return;
    }
    if def.basic_mob_flags & DEL_ON_DEATH != 0 {
        // A bot blows apart first (`/mob/living/basic/bot/death` -> `explode`).
        crate::bots::explode(world, mob);
        world.despawn(mob);
        return;
    }
    set_look(world, mob, true);
    if def.basic_mob_flags & REMAIN_DENSE_WHILE_DEAD == 0 {
        world.entity_mut(mob).remove::<Density>();
    }
}

/// `/mob/living/basic/examine`: the description, and that it is dead.
pub fn examine_lines(world: &World, mob: Entity, lines: &mut Vec<crate::examine::Line>) {
    use ss13_shared::chat::Span;
    if let Some(desc) = world
        .get::<DmName>(mob)
        .map(|n| n.desc.clone())
        .filter(|d| !d.is_empty())
    {
        lines.push((Span::Italics, desc));
    }
    crate::bots::examine_lines(world, mob, lines);
    farm::examine_lines(world, mob, lines);
    critters::examine_lines(world, mob, lines);
    constructs::examine_lines(world, mob, lines);
    if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        // tg `span_deadsay`, untranslated in BandaStation; the plural pronoun of
        // `p_they()` without a gender.
        lines.push((
            Span::Danger,
            "Upon closer examination, it appears to be dead.".to_owned(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_mapload::mobs::HabitableAtmos;

    #[test]
    fn body_temperature_follows_tg_handle_environment() {
        // Space: delta / 16 per second (under the -30 K cap).
        let t = body_temperature_step(310.15, 2.7, 2.0);
        assert!((t - (310.15 - 307.45 / 16.0 * 2.0)).abs() < 1e-9, "{t}");
        // Very hot air: capped at +30 K per second.
        let t = body_temperature_step(300.0, 2000.0, 2.0);
        assert!((t - 360.0).abs() < 1e-9, "{t}");
        // Close to the air: delta / 16 per second, never past the air.
        let t = body_temperature_step(300.0, 293.0, 2.0);
        assert!((t - (300.0 - 7.0 / 16.0 * 2.0)).abs() < 1e-9, "{t}");
        assert_eq!(body_temperature_step(293.5, 293.0, 100.0), 293.0);
        let t = body_temperature_step(290.0, 400.0, 1.0);
        assert!((t - (290.0 + 110.0 / 16.0)).abs() < 1e-9);
    }

    #[test]
    fn breathable_checks_moles_against_habitable_atmos() {
        let def = MobDef {
            habitable_atmos: Some(HabitableAtmos {
                min_oxy: 5.0,
                max_plas: 1.0,
                max_co2: 5.0,
                ..default()
            }),
            ..default()
        };
        assert!(!breathable(&def, None), "vacuum");
        let mut air = ss13_atmos::GasMixture::default();
        air.add(GasId::O2, 21.0);
        air.add(GasId::N2, 80.0);
        assert!(breathable(&def, Some(&air)));
        air.add(GasId::PLASMA, 2.0);
        assert!(!breathable(&def, Some(&air)), "too much plasma");
        let carp = MobDef::default();
        assert!(breathable(&carp, None), "no requirements");
    }
}
