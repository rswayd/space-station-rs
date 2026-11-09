//! Zombies after tg `code/modules/zombie` (`organs.dm`, `items.dm`),
//! `/datum/species/zombie` with `/datum/status_effect/zombie`, `/datum/reagent/romerol`.
//!
//! - **The infection** ([`ZombieInfection`], the organ `/obj/item/organ/zombie_infection`):
//!   inserted by a claw hit ([`try_infect`], BIO armour and thick clothing need prior limb
//!   damage; spaceacillin stops 75 %) or romerol ([`romerol_in_mob`], [`infect`]; the
//!   `nodamage` type does no toxin damage while its host lives). Every 2 s the host, if it is an
//!   organic, huskable non-zombie, takes 0.5 toxin a second; a dead host with a brain and no
//!   suicide starts a timer of 45-70 s and then rises ([`zombify`]): the status, a heal of the
//!   damage, a revival, the welcome. A body with no mind nor client is offered to ghosts through
//!   [`crate::ghost_poll`]. Removing the organ first cures it.
//! - **The status** ([`Zombified`]): its traits ([`grants`]), slowdown, 20 % physical damage
//!   resistance, a third of the stamina damage, stuns capped at 2 s, `TRAIT_NODEATH`
//!   ([`crate::health::BodyMods::no_death`]), husked body, combat mode locked on, claws in both
//!   hands, regeneration after 6 s without new damage (mindless zombies: 10 s and slower). Dying
//!   by another way (the head off) deletes the infection and ends it; the zombie species has
//!   its own weak `uninfected` kind. A mob without a mind gets the `mindless` kind.
//! - **The claw** (`/obj/item/mutant_hand/zombie`, [`after_attack`]): infects humans it hits;
//!   against any other dead living mob it feasts ([`check_feast`]).
// shortcut: no zombie tongue (say_mod, speech modification and emote sounds wait for tongue
// accents), no `lighting_cutoff` night vision, no temperature limit changes, no `MOB_UNDEAD`
// biotype (only `process`, here, reads the organic bit), no `TRAIT_STABLEHEART` (only
// handle_heart would read it), wounds are not healed by the regeneration, a claw lost with an
// arm does not come back; `SHUTTLE_UNLOCK_TOMBSTONE` is not unlocked (no shuttle purchase gating).

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_net::server::Networked;
use ss13_reagents::Chemistry;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::chem::{MobReagents, blood_amount, name_of, thick_covered, visible};
use crate::combat::{CombatMode, CombatRng};
use crate::ghost_poll::{self, DEFAULT_POLL_SECS, PollSpec};
use crate::health::{DamageType, Health, HealthSystems, MoveSpeedModifier, Stat, apply_damage};
use crate::inventory::{CarriedBy, Inventory, NoDrop};
use crate::medical::organs::{self, Organ, OrganHost, OrganOf, Slot};
use crate::obj_damage::is_under;
use crate::server::PlayerMob;
use crate::species::{SpeciesId, Trait};
use crate::status_effects::{self, Incap, Timed};
use crate::tools::tell_mob;
use crate::zone::BodyZone;

/// `/datum/reagent/romerol`.
pub const ROMEROL: &str = "/datum/reagent/romerol";
/// `/obj/item/organ/zombie_infection`.
pub const INFECTION: &str = "/obj/item/organ/zombie_infection";
/// `/obj/item/organ/zombie_infection/nodamage`, what romerol leaves.
pub const INFECTION_NODAMAGE: &str = "/obj/item/organ/zombie_infection/nodamage";
/// `/obj/item/mutant_hand/zombie`.
pub const CLAW: &str = "/obj/item/mutant_hand/zombie";
/// `/obj/item/mutant_hand/zombie/weak`.
pub const CLAW_WEAK: &str = "/obj/item/mutant_hand/zombie/weak";
/// `/obj/item/mutant_hand`.
const MUTANT_HAND: &str = "/obj/item/mutant_hand";
/// `POLL_IGNORE_RECOVERED_CREW`.
const POLL_IGNORE_RECOVERED_CREW: &str = "recovered_crew";
const FAR_NOISE: &str = "sound/effects/hallucinations/far_noise.ogg";
const INFECTED_FAR_NOISE_VOLUME: f32 = 50.0;
/// `living_transformation_time`.
const TRANSFORM_SECS: f32 = 3.0;
/// `NUTRITION_LEVEL_FULL`.
const NUTRITION_LEVEL_FULL: f32 = 600.0;
/// Frame: the organ's `SSobj` process and the status' regeneration.
const TICK_SECS: f32 = 2.0;

/// The kinds of `/datum/status_effect/zombie`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZombieKind {
    /// `/datum/status_effect/zombie`.
    Infected,
    /// `/datum/status_effect/zombie/mindless`.
    Mindless,
    /// `/datum/status_effect/zombie/uninfected`, the species' own.
    Uninfected,
}

impl ZombieKind {
    /// `movespeed_mod`'s `multiplicative_slowdown`, deciseconds per tile.
    fn slowdown(self) -> f32 {
        match self {
            Self::Infected => 1.0,
            Self::Mindless => 1.5,
            Self::Uninfected => 0.0,
        }
    }

    /// `zombie_hand`.
    fn claw(self) -> Option<&'static str> {
        match self {
            Self::Infected => Some(CLAW),
            Self::Mindless => Some(CLAW_WEAK),
            Self::Uninfected => None,
        }
    }

    /// `regen_time` and `regen_amount`.
    fn regen(self) -> (f32, f32) {
        match self {
            Self::Infected => (6.0, 0.5),
            Self::Mindless => (10.0, 0.2),
            Self::Uninfected => (0.0, 0.0),
        }
    }

    /// `damage_modifier`, percent.
    fn damage_modifier(self) -> f32 {
        match self {
            Self::Uninfected => 0.0,
            _ => 20.0,
        }
    }

    /// `stamina_modifier`.
    fn stamina(self) -> f32 {
        match self {
            Self::Uninfected => 1.0,
            _ => 0.33,
        }
    }

    /// `max_stun_length`, seconds.
    fn max_stun(self) -> f32 {
        match self {
            Self::Uninfected => f32::INFINITY,
            _ => 2.0,
        }
    }
}

/// On the infection organ inside a host (tg `timer_id`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct ZombieInfection {
    /// Game seconds the host rises at.
    pub revive_at: Option<f64>,
}

/// `/datum/status_effect/zombie` on a mob.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Zombified {
    pub kind: ZombieKind,
    /// The status made the infection itself (only then its removal ends the status).
    pub owns_infection: bool,
    /// Seconds without new damage left before the regeneration starts.
    regen_wait: f32,
    /// The total damage at the last look.
    last_damage: f32,
}

impl Zombified {
    /// `TRAIT_COMBAT_MODE_LOCK`, one of `unique_traits`.
    pub fn combat_locked(&self) -> bool {
        self.kind != ZombieKind::Uninfected
    }
}

/// `zombie_traits`, the ones consumers read.
const ZOMBIE_TRAITS: [Trait; 13] = [
    Trait::ColdBlooded,
    Trait::EasyDismember,
    Trait::FakeDeath,
    Trait::LimbAttachment,
    Trait::LiverlessMetabolism,
    Trait::NoBreath,
    Trait::NoCritDamage,
    Trait::NoHunger,
    Trait::RadImmune,
    Trait::ResistCold,
    Trait::ResistHighPressure,
    Trait::ResistLowPressure,
    Trait::ToxImmune,
];

/// Does the zombie status give `mob` trait `t`?
pub fn grants(world: &World, mob: Entity, t: Trait) -> bool {
    world.get::<Zombified>(mob).is_some_and(|z| {
        ZOMBIE_TRAITS.contains(&t) || (z.kind == ZombieKind::Uninfected && t == Trait::NoBlood)
    })
}

/// `COMSIG_HUMAN_SPEC_STUN`: how long a stun of `secs` lasts.
pub fn spec_stun(world: &World, mob: Entity, secs: f32) -> f32 {
    world
        .get::<Zombified>(mob)
        .map_or(secs, |z| secs.min(z.kind.max_stun()))
}

/// `MODIFY_PHYSIOLOGY(STAMINA)`.
pub fn stamina_mod(world: &World, mob: Entity) -> f32 {
    world
        .get::<Zombified>(mob)
        .map_or(1.0, |z| z.kind.stamina())
}

pub struct ZombiePlugin;

impl Plugin for ZombiePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, tick.before(HealthSystems));
    }
}

/// Cost: every 2 s, one pass over the infected organs and the zombies (a handful of entities).
fn tick(world: &mut World, mut next: Local<f64>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + f64::from(TICK_SECS);
    process_infections(world, now, TICK_SECS);
    process_zombies(world, TICK_SECS);
}

fn prob(world: &mut World, percent: f32) -> bool {
    let mut rng = world.get_resource_or_init::<CombatRng>();
    rng.chance(percent)
}

/// `amount` of `kind` (negative heals), then `update_stat`, which is why a heal goes through
/// here too; the stat is written only on a change, for `Changed<Stat>` readers.
fn damage(world: &mut World, mob: Entity, kind: DamageType, amount: f32) {
    let (Some(mut health), Some(mut stat)) = (
        world.get::<Health>(mob).cloned(),
        world.get::<Stat>(mob).copied(),
    ) else {
        return;
    };
    let changed = apply_damage(&mut health, &mut stat, kind, amount).is_some();
    world.entity_mut(mob).insert(health);
    if changed {
        world.entity_mut(mob).insert(stat);
    }
}

fn total_damage(world: &World, mob: Entity) -> f32 {
    world
        .get::<Health>(mob)
        .map_or(0.0, |h| h.brute + h.burn + h.tox + h.oxy)
}

/// `organs::Organ::on_mob_insert` of the infection.
pub(crate) fn infection_inserted(world: &mut World, organ: Entity) {
    world.entity_mut(organ).insert(ZombieInfection::default());
}

/// `on_mob_remove` of the infection and the status' `organ_removed`.
pub(crate) fn infection_removed(world: &mut World, organ: Entity, mob: Entity) {
    world.entity_mut(organ).remove::<ZombieInfection>();
    let owned = world
        .get::<Zombified>(mob)
        .is_some_and(|z| z.owns_infection && z.kind != ZombieKind::Uninfected);
    if owned {
        clear_status(world, mob);
    }
}

/// A new infection organ into `mob`; false if it has one already or cannot take organs.
pub fn infect(world: &mut World, mob: Entity, path: &str) -> bool {
    if world.get::<OrganHost>(mob).is_none() || organs::organ(world, mob, Slot::Zombie).is_some() {
        return false;
    }
    let Some(organ) = organs::spawn_organ(world, path) else {
        return false;
    };
    organs::insert(world, organ, mob)
}

/// `romerol/expose_mob` for a mob's reagents (ingest, injection, patch, inhalation: any amount).
pub(crate) fn romerol_in_mob(world: &mut World, chem: &Chemistry, mob: Entity) {
    let Some(m) = world.get::<MobReagents>(mob) else {
        return;
    };
    let found = [&m.blood, &m.stomach]
        .iter()
        .any(|h| h.reagents.iter().any(|r| chem.is_type(r.id, ROMEROL)));
    if found {
        infect(world, mob, INFECTION_NODAMAGE);
    }
}

fn process_infections(world: &mut World, now: f64, secs: f32) {
    let mut query = world.query::<(Entity, &ZombieInfection, &OrganOf, &Organ)>();
    let infections: Vec<(Entity, Entity, bool)> = query
        .iter(world)
        .map(|(e, _, of, o)| (e, of.0, !is_under(&o.path, INFECTION_NODAMAGE)))
        .collect();
    for (organ, mob, causes_damage) in infections {
        process_infection(world, organ, mob, causes_damage, now, secs);
    }
}

/// `/obj/item/organ/zombie_infection/process`.
fn process_infection(
    world: &mut World,
    organ: Entity,
    mob: Entity,
    causes_damage: bool,
    now: f64,
    secs: f32,
) {
    let Some(&stat) = world.get::<Stat>(mob) else {
        return;
    };
    let organic =
        crate::species::def_of(world, mob).biotypes & crate::species::biotype::ORGANIC != 0;
    // Zombies lose `MOB_ORGANIC`, so a risen host does not process either.
    if world.get::<Zombified>(mob).is_some()
        || !organic
        || crate::species::has_trait(world, mob, Trait::Unhuskable)
    {
        return;
    }
    if causes_damage && stat != Stat::Dead {
        damage(world, mob, DamageType::Tox, 0.5 * secs);
        // `SPT_PROB(5, seconds_per_tick)`.
        if prob(world, 100.0 * (1.0 - 0.95_f32.powf(secs))) {
            tell_mob(world, mob, Span::Danger, "Вам нехорошо...");
        }
    }
    let revive_at = world
        .get::<ZombieInfection>(organ)
        .and_then(|i| i.revive_at);
    if let Some(at) = revive_at {
        if now >= at {
            if let Some(mut infection) = world.get_mut::<ZombieInfection>(organ) {
                infection.revive_at = None;
            }
            zombify(world, mob);
        }
        return;
    }
    if world.get::<crate::round::Suicided>(mob).is_some()
        || organs::organ(world, mob, Slot::Brain).is_none()
        || stat != Stat::Dead
    {
        return;
    }
    tell_mob(
        world,
        mob,
        Span::Danger,
        "Ваше сердце останавливается, но что-то не так... жизнь не покинула ваше разбитое тело. \
         Вы ощущаете лишь глубокий и неутолимый голод, который не остановит даже смерть — вы \
         восстанете вновь!",
    );
    // `rand(revive_time_min, revive_time_max)`: 45-70 s.
    let delay = {
        let mut rng = world.get_resource_or_init::<CombatRng>();
        rng.range(45, 70)
    };
    if let Some(mut infection) = world.get_mut::<ZombieInfection>(organ) {
        infection.revive_at = Some(now + f64::from(delay));
    }
}

/// `zombify`: the corpse rises; the host's own player gets it back if the mind is still around.
pub fn zombify(world: &mut World, mob: Entity) {
    if world.get::<Stat>(mob) != Some(&Stat::Dead) {
        return;
    }
    let mindless = world.get::<crate::mind::MindData>(mob).is_none();
    let kind = if mindless {
        ZombieKind::Mindless
    } else {
        ZombieKind::Infected
    };
    apply(world, mob, kind);
    // `heal_and_revive(0, ...)`: only the damage types, not the organs.
    if let Some(health) = world.get::<Health>(mob).cloned() {
        world.entity_mut(mob).insert(health.healed());
    }
    // The damage healed here is no reason to delay the regeneration.
    if let Some(mut zombie) = world.get_mut::<Zombified>(mob) {
        zombie.last_damage = 0.0;
    }
    let name = name_of(world, mob);
    if !crate::medical::defib::revive(world, mob) {
        return;
    }
    visible(
        world,
        mob,
        "Ваше тело бьётся в конвульсиях, вы вновь на ногах и вас терзает ненасытный голод!",
        &format!("{name} вдруг бьётся в конвульсиях, поднимается и смотрит с ненасытным голодом!"),
        Span::Danger,
    );
    zombie_welcome(world, mob);
    if world.get::<crate::mind::MindData>(mob).is_some() || world.get::<PlayerMob>(mob).is_some() {
        return;
    }
    poll_ghosts(world, mob);
}

/// `ghost_direct_control` of a mindless zombie: the first to sign up that is still valid.
fn poll_ghosts(world: &mut World, mob: Entity) {
    if !world.contains_resource::<ghost_poll::Polls>() {
        return;
    }
    let spec = PollSpec {
        question: "Вы хотите стать зомби?".to_owned(),
        role_name: "зомби".to_owned(),
        secs: DEFAULT_POLL_SECS,
        ignore: Some(POLL_IGNORE_RECOVERED_CREW),
        target: Some(mob),
        ..default()
    };
    let group = ghost_poll::ghosts(world);
    ghost_poll::poll_candidates(world, spec, group, move |world, mut signed_up| {
        // `check_ghost_control_eligibility`, again at the moment of effect.
        if world.get::<Zombified>(mob).is_none()
            || world.get::<Stat>(mob) == Some(&Stat::Dead)
            || world.get::<PlayerMob>(mob).is_some()
        {
            return;
        }
        let Some(chosen) = ghost_poll::pick_candidate(world, &mut signed_up) else {
            return;
        };
        if crate::medical::brainmob::possess(world, chosen.client, chosen.mob, mob) {
            world.despawn(chosen.mob);
            // `after_ghost_control`.
            status_effects::knockdown(world, mob, TRANSFORM_SECS, 0.0);
            zombie_welcome(world, mob);
        }
    });
}

/// `zombie_welcome`.
fn zombie_welcome(world: &mut World, mob: Entity) {
    if world.get::<PlayerMob>(mob).is_some() {
        tell_mob(world, mob, Span::Danger, "Вы ГОЛОДНЫ!");
        tell_mob(
            world,
            mob,
            Span::BoldWarning,
            "Теперь вы зомби! Не ищите лечения, не помогайте не-зомби, не вредите своим \
             сородичам и распространяйте заразу, убивая других. Вы — существо голода и насилия.",
        );
        if let Some(&pos) = world.get::<TilePos>(mob) {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(pos, FAR_NOISE, INFECTED_FAR_NOISE_VOLUME, true),
            );
        }
    }
    status_effects::set_timed(world, mob, Timed::Jitter, TRANSFORM_SECS, true);
    status_effects::apply(world, mob, Incap::Stun, TRANSFORM_SECS);
}

/// `on_apply`; false for a non-carbon or a `TRAIT_UNHUSKABLE` body, or a zombie already.
pub fn apply(world: &mut World, mob: Entity, kind: ZombieKind) -> bool {
    if world.get::<Zombified>(mob).is_some()
        || world.get::<OrganHost>(mob).is_none()
        || crate::species::has_trait(world, mob, Trait::Unhuskable)
    {
        return false;
    }
    let owns_infection = organs::organ(world, mob, Slot::Zombie).is_none();
    if owns_infection {
        infect(world, mob, INFECTION);
    }
    let last_damage = total_damage(world, mob);
    world.entity_mut(mob).insert(Zombified {
        kind,
        owns_infection,
        regen_wait: 0.0,
        last_damage,
    });
    add_slowdown(world, mob, kind.slowdown());
    if let Some(mut mode) = world.get_mut::<CombatMode>(mob) {
        mode.0 = true;
    }
    crate::death::become_husk(world, mob);
    refresh_mods(world, mob);
    if let Some(claw) = kind.claw() {
        give_claws(world, mob, claw);
    }
    true
}

/// `on_remove`: what the status changed goes back and the infection goes.
fn clear_status(world: &mut World, mob: Entity) {
    let Some(zombified) = world.entity_mut(mob).take::<Zombified>() else {
        return;
    };
    add_slowdown(world, mob, -zombified.kind.slowdown());
    crate::death::cure_husk(world, mob, true);
    take_claws(world, mob);
    refresh_mods(world, mob);
    if let Some(organ) = organs::organ(world, mob, Slot::Zombie) {
        organs::remove(world, organ, true);
        world.try_despawn(organ).ok();
    }
}

fn add_slowdown(world: &mut World, mob: Entity, amount: f32) {
    if amount == 0.0 {
        return;
    }
    let current = world.get::<MoveSpeedModifier>(mob).map_or(0.0, |m| m.0);
    world
        .entity_mut(mob)
        .insert(MoveSpeedModifier(current + amount));
}

/// `damage_resistance`, `TRAIT_TOXIMMUNE` and `TRAIT_NODEATH` over the species' body rules.
fn refresh_mods(world: &mut World, mob: Entity) {
    let Some(mut health) = world.get::<Health>(mob).cloned() else {
        return;
    };
    let mut mods = crate::species::def_of(world, mob).body_mods();
    // The tenacity trauma's traits stay (`crate::medical::traumas::refresh`).
    mods.no_softcrit |= health.mods.no_hardcrit;
    mods.no_hardcrit = health.mods.no_hardcrit;
    if let Some(z) = world.get::<Zombified>(mob) {
        let resisted = 1.0 - z.kind.damage_modifier() / 100.0;
        mods.brute *= resisted;
        mods.burn *= resisted;
        mods.tox_immune = true;
        mods.no_death = true;
    }
    health.mods = mods;
    world.entity_mut(mob).insert(health);
}

/// `COMSIG_SPECIES_GAIN` of the status, and the zombie species' `on_species_gain`/`loss`.
pub fn species_changed(world: &mut World, mob: Entity, old: Option<SpeciesId>, new: SpeciesId) {
    if old == Some(SpeciesId::Zombie)
        && new != SpeciesId::Zombie
        && world
            .get::<Zombified>(mob)
            .is_some_and(|z| z.kind == ZombieKind::Uninfected)
    {
        clear_status(world, mob);
    }
    if new == SpeciesId::Zombie && old != Some(SpeciesId::Zombie) {
        apply(world, mob, ZombieKind::Uninfected);
    }
    if world.get::<Zombified>(mob).is_some() {
        refresh_mods(world, mob);
    }
}

/// `/datum/component/mutant_hands`: a claw in each hand; what they held falls.
fn give_claws(world: &mut World, mob: Entity, path: &str) {
    if world.get::<Inventory>(mob).is_none() {
        return;
    }
    for hand in 0..2 {
        let held = world.get::<Inventory>(mob).and_then(|i| i.hands[hand]);
        if let Some(held) = held {
            // A stuck item stays where it is (a cyberimp's tool, tg's NODROP).
            let Some(&pos) = world.get::<TilePos>(mob) else {
                continue;
            };
            if world.get::<NoDrop>(held).is_some() {
                continue;
            }
            crate::inventory::put_down(world, held, pos);
        }
        let Some(claw) = crate::construction::spawn_detached(world, path) else {
            return;
        };
        world
            .entity_mut(claw)
            .insert((Networked, CarriedBy(mob), NoDrop));
        if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
            inventory.hands[hand] = Some(claw);
        }
    }
}

fn take_claws(world: &mut World, mob: Entity) {
    for hand in 0..2 {
        let Some(item) = world.get::<Inventory>(mob).and_then(|i| i.hands[hand]) else {
            continue;
        };
        if world
            .get::<DmItem>(item)
            .is_some_and(|i| is_under(&i.path, MUTANT_HAND))
        {
            if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
                inventory.hands[hand] = None;
            }
            world.try_despawn(item).ok();
        }
    }
}

/// `zombie_died_somehow` and `/datum/component/regenerator`.
fn process_zombies(world: &mut World, secs: f32) {
    let mut query = world.query::<(Entity, &Zombified)>();
    let zombies: Vec<(Entity, Zombified)> = query.iter(world).map(|(e, z)| (e, *z)).collect();
    for (mob, mut z) in zombies {
        let Some(&stat) = world.get::<Stat>(mob) else {
            continue;
        };
        if stat == Stat::Dead {
            if z.kind != ZombieKind::Uninfected {
                clear_status(world, mob);
            }
            continue;
        }
        let (delay, rate) = z.kind.regen();
        if rate <= 0.0 {
            continue;
        }
        let before = total_damage(world, mob);
        if before > z.last_damage + 0.01 {
            z.regen_wait = delay;
        } else if z.regen_wait > 0.0 {
            z.regen_wait -= secs;
        } else if before > 0.0 {
            for (kind, share) in [
                (DamageType::Brute, 1.0),
                (DamageType::Burn, 1.0),
                (DamageType::Tox, 1.0),
                (DamageType::Oxy, 0.5),
            ] {
                damage(world, mob, kind, -rate * share * secs);
            }
        }
        z.last_damage = total_damage(world, mob);
        world.entity_mut(mob).insert(z);
    }
}

/// `/obj/item/mutant_hand/zombie/afterattack`.
pub(crate) fn after_attack(
    world: &mut World,
    attacker: Entity,
    target: Entity,
    weapon: Entity,
    zone: BodyZone,
) {
    let Some(path) = world.get::<DmItem>(weapon).map(|i| i.path.clone()) else {
        return;
    };
    if !is_under(&path, CLAW) || world.get_entity(target).is_err() {
        return;
    }
    if world.get::<OrganHost>(target).is_some() {
        let chance = if is_under(&path, CLAW_WEAK) {
            50.0
        } else {
            100.0
        };
        try_infect(world, attacker, target, zone, chance);
    } else if world.get::<Health>(target).is_some() {
        check_feast(world, attacker, target);
    }
}

/// `/proc/try_to_zombie_infect`.
pub fn try_infect(world: &mut World, user: Entity, target: Entity, zone: BodyZone, chance: f32) {
    if !prob(world, chance) {
        return;
    }
    let Some(health) = world.get::<Health>(target).cloned() else {
        return;
    };
    if !health.has_part(BodyZone::Head) {
        return;
    }
    // Spaceacillin (`TRAIT_VIRUS_RESISTANCE`).
    if blood_amount(world, target, crate::medical::diseases::SPACEACILLIN) > 0.0
        && prob(world, 75.0)
    {
        return;
    }
    if !health.has_part(zone) {
        return;
    }
    let part = health.part(zone);
    let limb_damage = part.brute + part.burn;
    let mut limb_armor = (crate::armor::getarmor(world, target, zone, "bio") - 25.0).max(0.0);
    if thick_covered(world, target, zone) {
        limb_armor += 25.0;
    }
    if limb_armor > limb_damage {
        return;
    }
    if infect(world, target, INFECTION) {
        let victim = name_of(world, target);
        tell_mob(
            world,
            user,
            Span::Danger,
            format!("{victim} на мгновение дёргается, голову покрывает гнойная масса — заражение."),
        );
    }
}

/// `check_feast`: a zombie gibs a dead non-human and heals by its max health.
fn check_feast(world: &mut World, user: Entity, target: Entity) {
    if world.get::<Stat>(target) != Some(&Stat::Dead) {
        return;
    }
    let Some(hp) = world.get::<Health>(target).map(|h| h.max) else {
        return;
    };
    crate::health::gib(world, target);
    damage(world, user, DamageType::Brute, -hp);
    damage(world, user, DamageType::Tox, -hp);
    damage(world, user, DamageType::Burn, -hp);
    organs::adjust_organ_loss(world, user, Slot::Brain, -hp);
    if let Some(mut hunger) = world.get_mut::<crate::hunger::Hunger>(user) {
        hunger.nutrition = (hunger.nutrition + hp).min(NUTRITION_LEVEL_FULL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.insert_resource(CombatRng(1));
        world
    }

    /// A human body with a brain, no player.
    fn body(world: &mut World, stat: Stat) -> Entity {
        let mob = world.spawn((Health::default(), stat, OrganHost)).id();
        let brain = organs::spawn_organ(world, "/obj/item/organ/brain").unwrap();
        world.entity_mut(brain).insert(OrganOf(mob));
        mob
    }

    #[test]
    fn a_host_takes_toxin_only_with_the_damaging_kind() {
        let mut world = world();
        let sick = body(&mut world, Stat::Conscious);
        let carrier = body(&mut world, Stat::Conscious);
        infect(&mut world, sick, INFECTION);
        infect(&mut world, carrier, INFECTION_NODAMAGE);
        process_infections(&mut world, 10.0, 2.0);
        assert_eq!(world.get::<Health>(sick).unwrap().tox, 1.0);
        assert_eq!(world.get::<Health>(carrier).unwrap().tox, 0.0);
    }

    #[test]
    fn a_dead_host_rises_after_the_timer_and_regenerates() {
        let mut world = world();
        let mob = body(&mut world, Stat::Dead);
        world
            .entity_mut(mob)
            .insert(crate::mind::MindData::new("p", "Иван"));
        let mut hurt = Health::default();
        hurt.adjust(DamageType::Brute, 150.0);
        world.entity_mut(mob).insert(hurt);
        infect(&mut world, mob, INFECTION);
        process_infections(&mut world, 100.0, 2.0);
        let organ = organs::organ(&world, mob, Slot::Zombie).unwrap();
        let at = world
            .get::<ZombieInfection>(organ)
            .unwrap()
            .revive_at
            .unwrap();
        assert!((145.0..=170.0).contains(&at), "{at}");
        // Too soon: nothing.
        process_infections(&mut world, 120.0, 2.0);
        assert_eq!(world.get::<Stat>(mob), Some(&Stat::Dead));
        process_infections(&mut world, at + 0.1, 2.0);
        assert_eq!(world.get::<Stat>(mob), Some(&Stat::Conscious));
        let zombie = *world.get::<Zombified>(mob).unwrap();
        assert_eq!(zombie.kind, ZombieKind::Infected);
        assert!(!zombie.owns_infection);
        assert_eq!(world.get::<Health>(mob).unwrap().brute, 0.0);
        assert!(world.get::<crate::human::Husk>(mob).is_some());
        // The zombie's slowdown and resistances.
        assert_eq!(world.get::<MoveSpeedModifier>(mob).unwrap().0, 1.0);
        assert_eq!(spec_stun(&world, mob, 5.0), 2.0);
        assert!(grants(&world, mob, Trait::NoBreath));
        assert!(crate::species::has_trait(&world, mob, Trait::NoHunger));
        // Regeneration: hurt now, nothing heals until the 6 s pass.
        let mut wounded = world.get::<Health>(mob).unwrap().clone();
        wounded.adjust(DamageType::Burn, 10.0);
        let burned = wounded.burn;
        world.entity_mut(mob).insert(wounded);
        process_zombies(&mut world, 2.0);
        assert_eq!(world.get::<Health>(mob).unwrap().burn, burned);
        for _ in 0..4 {
            process_zombies(&mut world, 2.0);
        }
        assert!(world.get::<Health>(mob).unwrap().burn < burned);
    }

    #[test]
    fn a_mindless_body_gets_the_weak_kind_and_no_player_does_not_block_it() {
        let mut world = world();
        let mob = body(&mut world, Stat::Dead);
        zombify(&mut world, mob);
        assert_eq!(
            world.get::<Zombified>(mob).unwrap().kind,
            ZombieKind::Mindless
        );
    }

    #[test]
    fn death_ends_the_status_and_removing_the_organ_cures_a_host() {
        let mut world = world();
        let mob = body(&mut world, Stat::Conscious);
        infect(&mut world, mob, INFECTION);
        let organ = organs::organ(&world, mob, Slot::Zombie).unwrap();
        organs::remove(&mut world, organ, false);
        assert!(world.get::<ZombieInfection>(organ).is_none());
        assert!(organs::organ(&world, mob, Slot::Zombie).is_none());
        // A zombie that dies by another way (no brain) is a corpse again.
        apply(&mut world, mob, ZombieKind::Infected);
        assert!(world.get::<Zombified>(mob).unwrap().owns_infection);
        world.entity_mut(mob).insert(Stat::Dead);
        process_zombies(&mut world, 2.0);
        assert!(world.get::<Zombified>(mob).is_none());
        assert!(organs::organ(&world, mob, Slot::Zombie).is_none());
    }

    #[test]
    fn no_death_keeps_a_zombie_in_hard_crit() {
        let mut world = world();
        let mob = body(&mut world, Stat::Conscious);
        apply(&mut world, mob, ZombieKind::Infected);
        damage(&mut world, mob, DamageType::Brute, 500.0);
        assert_eq!(world.get::<Stat>(mob), Some(&Stat::HardCrit));
    }

    #[test]
    fn a_claw_infects_once() {
        let mut world = world();
        let attacker = world.spawn_empty().id();
        let target = body(&mut world, Stat::Conscious);
        try_infect(&mut world, attacker, target, BodyZone::Chest, 100.0);
        assert!(organs::organ(&world, target, Slot::Zombie).is_some());
        assert!(!infect(&mut world, target, INFECTION));
    }
}
