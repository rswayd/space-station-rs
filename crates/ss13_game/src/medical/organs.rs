//! Organs (server), after tg `code/modules/surgery/organs` (`_organ.dm`: `apply_organ_damage`,
//! `check_damage_thresholds`, `on_life`, `on_death`, `handle_failing_organs`; `organ_movement.dm`:
//! `Insert`, `Remove`; `helpers.dm`), the internal organs (`internal/heart`, `lungs`, `liver`,
//! `stomach`, `eyes`, `ears`, `tongue`, `appendix`, `mob/living/brain/brain_item.dm`) and the
//! carbon's `handle_organs`, `handle_heart`, `handle_liver`, `needs_heart`,
//! `undergoing_cardiac_arrest`, `set_heartattack` (`carbon/life.dm`, `human/life.dm`).
//!
//! An organ is an item entity ([`Organ`]) spawned from its tg type: inside a body it is
//! [`OrganOf`] that mob, has no `TilePos` and is not replicated; removed, it is an ordinary item
//! on the floor or in a hand. A human gets the species organs at spawn ([`give_organs`]). Every
//! `SSmobs` fire the organs of a living mob run `on_life` in `organ_process_order` (natural
//! healing by `healing_factor`, failure effects, the heart stopping, the lungs coughing, the
//! ears ringing), then the heart's cardiac arrest and the missing liver hurt; a corpse's organs
//! decay by its body temperature (not with formaldehyde or cryostylane in the blood). A removed
//! organ decays by the air around it (a freezer crate or closet keeps it), a removed heart stops
//! after 12 s. Damage crosses thresholds with tg's messages; at `maxHealth` the organ fails.
//! Losing the brain kills (`ORGAN_VITAL`); the lungs, liver, stomach and ears are read by
//! `crate::breath`, `crate::chem`, `crate::hunger` and `crate::sound`; without a tongue the
//! mob's speech gurgles (`ss13_chat::speech::Tongueless`).
// shortcut: no brainmob (the mind stays with the body: a removed brain does not carry the
// player, an inserted one does not bring one), no skillchips, organ traits,
// actions or status effects, no eating organs, no blood DNA, no EMP, no organ cell lines; eyes
// have no sight effects (no blindness, blur or night vision yet) besides being drawn; species
// organ variants beyond the human set keep the standard numbers.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::{name_of, visible};
use crate::health::{DamageType, Health, Stat};
use crate::obj_damage::is_under;
use crate::server::PlayerMob;
use crate::tools::tell;
use crate::zone::{BodyZone, TargetZone};

/// `STANDARD_ORGAN_THRESHOLD`, `STANDARD_ORGAN_HEALING`, `STANDARD_ORGAN_DECAY` (per second, of
/// the organ's max health).
pub const STANDARD_ORGAN_THRESHOLD: f32 = 100.0;
pub const STANDARD_ORGAN_HEALING: f32 = 50.0 / 100_000.0;
pub const STANDARD_ORGAN_DECAY: f32 = 111.0 / 100_000.0;
/// `BRAIN_DAMAGE_MILD`, `BRAIN_DAMAGE_SEVERE`, `BRAIN_DAMAGE_DEATH`.
pub const BRAIN_DAMAGE_MILD: f32 = 20.0;
pub const BRAIN_DAMAGE_SEVERE: f32 = 100.0;
pub const BRAIN_DAMAGE_DEATH: f32 = 200.0;
/// `LIVER_FAILURE_STAGE_SECONDS`.
const LIVER_FAILURE_STAGE_SECONDS: f32 = 60.0;
/// The heart's `stop_if_unowned` timer after a removal, seconds.
const HEART_STOP_DELAY: f64 = 12.0;
const T0C: f64 = 273.15;
/// `/obj/structure/closet/crate/freezer::minimum_temperature`.
const FREEZER_TEMPERATURE: f64 = T0C - 60.0;
/// Containers that keep their contents cold (tg freezers cool their own air).
const FREEZERS: [&str; 2] = [
    "/obj/structure/closet/crate/freezer",
    "/obj/structure/closet/secure_closet/freezer",
];
/// `/datum/reagent/toxin/formaldehyde`, `/datum/reagent/cryostylane`: no decay of the dead.
const PRESERVATIVES: [&str; 2] = [
    "/datum/reagent/toxin/formaldehyde",
    "/datum/reagent/cryostylane",
];

/// tg `ORGAN_SLOT_*` of the modelled organs, in `GLOB.organ_process_order`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    Brain,
    Appendix,
    Stomach,
    Ears,
    Eyes,
    Lungs,
    Heart,
    Liver,
    Tongue,
}

/// The static part of an organ type (tg vars).
#[derive(Clone, Copy, Debug)]
pub struct Spec {
    /// The species' type (`mutant*`), what a human gets.
    pub path: &'static str,
    /// `zone`: where it sits (`BODY_ZONE_PRECISE_*` for the eyes, tongue and appendix).
    pub zone: TargetZone,
    pub max_health: f32,
    pub healing_factor: f32,
    pub decay_factor: f32,
    pub low_threshold: f32,
    pub high_threshold: f32,
    /// `ORGAN_VITAL`: losing it kills.
    pub vital: bool,
    /// The tg `name`.
    pub name: &'static str,
    /// The organ in the accusative and genitive (for surgery lines).
    pub ru: (&'static str, &'static str),
    pub low_passed: &'static str,
    pub high_passed: &'static str,
    pub now_failing: &'static str,
    pub now_fixed: &'static str,
    pub high_cleared: &'static str,
    pub low_cleared: &'static str,
}

const BASE: Spec = Spec {
    path: "/obj/item/organ",
    zone: TargetZone::Chest,
    max_health: STANDARD_ORGAN_THRESHOLD,
    healing_factor: 0.0,
    decay_factor: 0.0,
    low_threshold: STANDARD_ORGAN_THRESHOLD * 0.1,
    high_threshold: STANDARD_ORGAN_THRESHOLD * 0.45,
    vital: false,
    name: "organ",
    ru: ("орган", "органа"),
    low_passed: "",
    high_passed: "",
    now_failing: "",
    now_fixed: "",
    high_cleared: "",
    low_cleared: "",
};

impl Slot {
    pub const ALL: [Self; 9] = [
        Self::Brain,
        Self::Appendix,
        Self::Stomach,
        Self::Ears,
        Self::Eyes,
        Self::Lungs,
        Self::Heart,
        Self::Liver,
        Self::Tongue,
    ];

    /// The slot of an organ type (its root type).
    pub fn of_path(path: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|slot| is_under(path, slot.spec().path))
    }

    pub fn spec(self) -> Spec {
        match self {
            Self::Brain => Spec {
                path: "/obj/item/organ/brain",
                name: "brain",
                zone: TargetZone::Head,
                max_health: BRAIN_DAMAGE_DEATH,
                decay_factor: STANDARD_ORGAN_DECAY * 0.5,
                low_threshold: 45.0,
                high_threshold: 120.0,
                vital: true,
                ru: ("мозг", "мозга"),
                ..BASE
            },
            Self::Appendix => Spec {
                path: "/obj/item/organ/appendix",
                name: "appendix",
                zone: TargetZone::Groin,
                healing_factor: STANDARD_ORGAN_HEALING,
                decay_factor: STANDARD_ORGAN_DECAY,
                ru: ("аппендикс", "аппендикса"),
                now_failing: "An explosion of pain erupts in your lower right abdomen!",
                now_fixed: "The pain in your abdomen has subsided.",
                ..BASE
            },
            Self::Stomach => Spec {
                path: "/obj/item/organ/stomach",
                name: "stomach",
                healing_factor: STANDARD_ORGAN_HEALING,
                decay_factor: STANDARD_ORGAN_DECAY * 1.15,
                ru: ("желудок", "желудка"),
                low_passed: "Your stomach flashes with pain before subsiding. Food doesn't seem \
                             like a good idea right now.",
                high_passed: "Your stomach flares up with constant pain- you can hardly stomach \
                              the idea of food right now!",
                high_cleared: "The pain in your stomach dies down for now, but food still seems \
                               unappealing.",
                low_cleared: "The last bouts of pain in your stomach have died out.",
                ..BASE
            },
            Self::Ears => Spec {
                path: "/obj/item/organ/ears",
                name: "ears",
                zone: TargetZone::Head,
                healing_factor: STANDARD_ORGAN_HEALING,
                decay_factor: STANDARD_ORGAN_DECAY,
                ru: ("уши", "ушей"),
                low_passed: "Your ears begin to resonate with an internal ring sometimes.",
                now_failing: "You are unable to hear at all!",
                now_fixed: "Noise slowly begins filling your ears once more.",
                low_cleared: "The ringing in your ears has died down.",
                ..BASE
            },
            Self::Eyes => Spec {
                path: "/obj/item/organ/eyes",
                name: "eyes",
                zone: TargetZone::Eyes,
                max_health: 0.5 * STANDARD_ORGAN_THRESHOLD,
                healing_factor: STANDARD_ORGAN_HEALING,
                decay_factor: STANDARD_ORGAN_DECAY,
                low_threshold: 0.2 * STANDARD_ORGAN_THRESHOLD,
                high_threshold: 0.3 * STANDARD_ORGAN_THRESHOLD,
                ru: ("глаза", "глаз"),
                low_passed: "Distant objects become somewhat less tangible.",
                high_passed: "Everything starts to look a lot less clear.",
                now_failing: "Darkness envelopes you, as your eyes go blind!",
                now_fixed: "Color and shapes are once again perceivable.",
                high_cleared: "Your vision functions passably once more.",
                low_cleared: "Your vision is cleared of any ailment.",
                ..BASE
            },
            Self::Lungs => Spec {
                path: "/obj/item/organ/lungs",
                name: "lungs",
                healing_factor: STANDARD_ORGAN_HEALING,
                decay_factor: STANDARD_ORGAN_DECAY * 0.9,
                ru: ("лёгкие", "лёгких"),
                low_passed: "You feel short of breath.",
                high_passed: "You feel some sort of constriction around your chest as your \
                              breathing becomes shallow and rapid.",
                now_fixed: "Your lungs seem to once again be able to hold air.",
                high_cleared: "The constriction around your chest loosens as your breathing \
                               calms down.",
                low_cleared: "You can breathe normally again.",
                ..BASE
            },
            Self::Heart => Spec {
                path: "/obj/item/organ/heart",
                name: "heart",
                healing_factor: STANDARD_ORGAN_HEALING,
                decay_factor: 2.5 * STANDARD_ORGAN_DECAY,
                ru: ("сердце", "сердца"),
                low_passed: "Prickles of pain appear then die out from within your chest...",
                high_passed: "Something inside your chest hurts, and the pain isn't subsiding. \
                              You notice yourself breathing far faster than before.",
                now_fixed: "Your heart begins to beat again.",
                high_cleared: "The pain in your chest has died down, and your breathing becomes \
                               more relaxed.",
                ..BASE
            },
            Self::Liver => Spec {
                path: "/obj/item/organ/liver",
                name: "liver",
                healing_factor: STANDARD_ORGAN_HEALING,
                decay_factor: STANDARD_ORGAN_DECAY,
                ru: ("печень", "печени"),
                ..BASE
            },
            Self::Tongue => Spec {
                path: "/obj/item/organ/tongue",
                name: "tongue",
                zone: TargetZone::Mouth,
                ru: ("язык", "языка"),
                ..BASE
            },
        }
    }

    /// The bodypart it sits in (`deprecise_zone(zone)`).
    pub fn bodypart(self) -> BodyZone {
        self.spec().zone.check_zone()
    }
}

/// On an organ inside a mob (tg `owner`). Despawning the mob despawns its organs.
#[derive(Component, Debug)]
#[relationship(relationship_target = Organs)]
pub struct OrganOf(pub Entity);

/// A mob's organs (tg `organs`).
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = OrganOf, linked_spawn)]
pub struct Organs(Vec<Entity>);

impl Organs {
    pub fn iter(&self) -> impl Iterator<Item = Entity> + '_ {
        self.0.iter().copied()
    }
}

/// The mob has organs modelled (a carbon): a slot without its organ is a missing organ.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct OrganHost;

/// A live organ (tg `/obj/item/organ` vars).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Organ {
    pub slot: Slot,
    pub damage: f32,
    /// `ORGAN_FAILING`.
    pub failing: bool,
    /// `ORGAN_ROBOTIC` (cybernetic types): no natural healing or decay, no heart attacks.
    pub robotic: bool,
    /// `TRAIT_ORGAN_OPERATED_ON`: a one-time repair operation was done.
    pub operated: bool,
    /// `failure_time`, seconds.
    pub failure_time: f32,
    /// The heart's `beating`.
    pub beating: bool,
    /// Game time a removed heart stops at (`stop_if_unowned`).
    pub stop_at: Option<f64>,
    /// The ears' `temporary_deafness`, seconds.
    pub deafness: f32,
    /// The lungs' `failed` (the "can't breathe" message was shown).
    pub lungs_failed: bool,
}

impl Organ {
    pub fn new(slot: Slot, path: &str) -> Self {
        Self {
            slot,
            damage: 0.0,
            failing: false,
            robotic: path.contains("/cybernetic"),
            operated: false,
            failure_time: 0.0,
            beating: true,
            stop_at: None,
            deafness: 0.0,
            lungs_failed: false,
        }
    }

    pub fn spec(&self) -> Spec {
        self.slot.spec()
    }

    /// tg `apply_organ_damage(amount, maximum)` + `check_damage_thresholds`: the message for
    /// the owner, if a threshold was crossed.
    pub fn apply_damage(&mut self, amount: f32, maximum: f32) -> Option<&'static str> {
        let spec = self.spec();
        let maximum = maximum.clamp(0.0, spec.max_health);
        if amount == 0.0 || maximum < self.damage {
            return None;
        }
        let prev = self.damage;
        self.damage = (self.damage + amount).clamp(0.0, maximum);
        let message = self.thresholds(prev);
        let brain = (self.slot == Slot::Brain)
            .then(|| brain_message(prev, self.damage))
            .flatten();
        brain.or(message).filter(|m| !m.is_empty())
    }

    /// `check_damage_thresholds`: the last message of the crossed thresholds.
    fn thresholds(&mut self, prev: f32) -> Option<&'static str> {
        let spec = self.spec();
        let damage = self.damage;
        if damage == prev {
            return None;
        }
        let mut message = None;
        if damage > prev {
            if damage > spec.low_threshold && prev <= spec.low_threshold {
                message = Some(spec.low_passed);
            }
            if damage > spec.high_threshold && prev <= spec.high_threshold {
                message = Some(spec.high_passed);
            }
            if damage >= spec.max_health {
                self.failing = true;
                message = Some(spec.now_failing);
            }
            return message;
        }
        if self.failing {
            self.failing = false;
            message = Some(spec.now_fixed);
        }
        if prev > spec.high_threshold && damage <= spec.high_threshold {
            message = Some(spec.high_cleared);
        }
        if prev > spec.low_threshold && damage <= spec.low_threshold {
            message = Some(spec.low_cleared);
        }
        message
    }

    /// `set_organ_damage`.
    pub fn set_damage(&mut self, damage: f32) -> Option<&'static str> {
        self.apply_damage(damage - self.damage, self.spec().max_health)
    }

    /// The decay of `on_death`: `decay_factor` of max health per second, scaled by how far
    /// above 0 °C it is (full from 20 °C).
    pub fn decay(&mut self, temperature: f64, secs: f32) {
        if self.robotic || temperature <= T0C {
            return;
        }
        let spec = self.spec();
        let factor = ((temperature - T0C) / 20.0).min(1.0) as f32;
        self.apply_damage(
            spec.decay_factor * spec.max_health * secs * factor,
            spec.max_health,
        );
    }

    /// The generic `on_life` of a living owner: failure time, natural healing (more with
    /// positive satiety). Returns the threshold message.
    pub fn heal_tick(&mut self, satiety: f32, secs: f32) -> Option<&'static str> {
        if self.failing {
            self.failure_time += secs;
            return None;
        }
        if self.failure_time > 0.0 {
            self.failure_time = (self.failure_time - 1.0).max(0.0);
        }
        if self.damage == 0.0 || self.robotic {
            return None;
        }
        let spec = self.spec();
        let mut healing = spec.healing_factor;
        if satiety > 0.0 {
            healing += 4.0 * spec.healing_factor * satiety / crate::hunger::MAX_SATIETY;
        }
        let damage = self.damage;
        self.apply_damage(-healing * spec.max_health * secs, damage)
    }
}

/// The brain's extra `check_damage_thresholds` lines on more damage.
fn brain_message(prev: f32, damage: f32) -> Option<&'static str> {
    if damage <= prev {
        return None;
    }
    if prev < BRAIN_DAMAGE_MILD && damage >= BRAIN_DAMAGE_MILD {
        Some("You feel lightheaded.")
    } else if prev < BRAIN_DAMAGE_SEVERE && damage >= BRAIN_DAMAGE_SEVERE {
        Some("You feel less in control of your thoughts.")
    } else if prev < BRAIN_DAMAGE_DEATH - 20.0 && damage >= BRAIN_DAMAGE_DEATH - 20.0 {
        Some("You can feel your mind flickering on and off...")
    } else {
        None
    }
}

/// `get_organ_slot`: the organ entity of `slot` in `mob`.
pub fn organ(world: &World, mob: Entity, slot: Slot) -> Option<Entity> {
    world
        .get::<Organs>(mob)?
        .iter()
        .find(|&o| world.get::<Organ>(o).is_some_and(|org| org.slot == slot))
}

/// The state of `mob`'s organ in `slot`.
pub fn state(world: &World, mob: Entity, slot: Slot) -> Option<&Organ> {
    world.get::<Organ>(organ(world, mob, slot)?)
}

/// The mob has organs modelled and lacks the one of `slot`.
pub fn lacks(world: &World, mob: Entity, slot: Slot) -> bool {
    world.get::<OrganHost>(mob).is_some() && organ(world, mob, slot).is_none()
}

/// `slot` is missing or failing (tg's `!organ || organ.organ_flags & ORGAN_FAILING`), on a mob
/// with organs.
pub fn lacks_or_failing(world: &World, mob: Entity, slot: Slot) -> bool {
    world.get::<OrganHost>(mob).is_some() && state(world, mob, slot).is_none_or(|o| o.failing)
}

/// The owner hears its organ's message unless it is unconscious.
fn tell_owner(world: &mut World, mob: Entity, span: Span, text: &str) {
    if text.is_empty() {
        return;
    }
    let (Some(health), Some(&stat)) = (world.get::<Health>(mob), world.get::<Stat>(mob)) else {
        return;
    };
    if health.knocked_out(stat) {
        return;
    }
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

/// tg `apply_organ_damage` on an organ entity, its message to the owner.
pub fn damage_organ(world: &mut World, organ: Entity, amount: f32, maximum: f32) {
    let Some(mut state) = world.get::<Organ>(organ).cloned() else {
        return;
    };
    let prev = state.damage;
    let message = state.apply_damage(amount, maximum);
    let owner = world.get::<OrganOf>(organ).map(|o| o.0);
    let (slot, damage) = (state.slot, state.damage);
    if world.get::<Organ>(organ) != Some(&state) {
        world.entity_mut(organ).insert(state);
    }
    if let (Some(owner), Some(text)) = (owner, message) {
        tell_owner(world, owner, Span::Warning, text);
    }
    // The brain's `apply_organ_damage`: more damage past the mild threshold rolls a trauma.
    if slot == Slot::Brain && damage > prev && damage > BRAIN_DAMAGE_MILD {
        super::traumas::roll(world, organ, damage - prev, damage);
    }
}

/// tg `adjust_organ_loss(slot, amount)` (up to the organ's max health).
pub fn adjust_organ_loss(world: &mut World, mob: Entity, slot: Slot, amount: f32) {
    if let Some(organ) = organ(world, mob, slot) {
        damage_organ(world, organ, amount, slot.spec().max_health);
    }
}

/// tg `adjust_organ_loss(slot, amount, maximum)`.
pub fn adjust_organ_loss_up_to(
    world: &mut World,
    mob: Entity,
    slot: Slot,
    amount: f32,
    maximum: f32,
) {
    if let Some(organ) = organ(world, mob, slot) {
        damage_organ(world, organ, amount, maximum);
    }
}

/// tg `set_organ_damage` on `mob`'s organ.
pub fn set_organ_damage(world: &mut World, mob: Entity, slot: Slot, damage: f32) {
    if let Some(organ) = organ(world, mob, slot) {
        let current = world.get::<Organ>(organ).map_or(0.0, |o| o.damage);
        damage_organ(world, organ, damage - current, slot.spec().max_health);
    }
}

/// A new organ of type `path` outside the world (tg `new path()`); without its prototype (no
/// generated assets) a bare organ entity.
pub fn spawn_organ(world: &mut World, path: &str) -> Option<Entity> {
    let slot = Slot::of_path(path)?;
    let entity = crate::construction::spawn_detached(world, path)
        .unwrap_or_else(|| world.spawn(Name::new(slot.spec().ru.0)).id());
    world.entity_mut(entity).insert(Organ::new(slot, path));
    Some(entity)
}

/// The human species' organs (`/datum/species/human` `mutant*`).
fn give_organs(
    add: On<Add<Health>>,
    excluded: Query<(), Or<(With<crate::mobs::BasicMob>, With<crate::silicon::Silicon>)>>,
    mut commands: Commands,
) {
    if excluded.contains(add.entity) {
        return;
    }
    let mob = add.entity;
    commands.queue(move |world: &mut World| {
        if world.get_entity(mob).is_err() || world.get::<OrganHost>(mob).is_some() {
            return;
        }
        world.entity_mut(mob).insert(OrganHost);
        for slot in Slot::ALL {
            if let Some(organ) = spawn_organ(world, slot.spec().path) {
                world.entity_mut(organ).insert(OrganOf(mob));
            }
        }
    });
}

/// tg `Insert(receiver)`: `organ` (anywhere) into `mob`; an organ already in its slot drops to
/// the floor (`mob_insert`). False if `mob` cannot take organs.
pub fn insert(world: &mut World, organ: Entity, mob: Entity) -> bool {
    let Some(slot) = world.get::<Organ>(organ).map(|o| o.slot) else {
        return false;
    };
    if world.get::<OrganHost>(mob).is_none() {
        return false;
    }
    if let Some(old) = self::organ(world, mob, slot) {
        remove(world, old, true);
        if let Some(&pos) = world.get::<TilePos>(mob) {
            crate::storage::take_out(world, old, pos);
        }
    }
    world
        .entity_mut(organ)
        .remove::<(
            TilePos,
            crate::storage::StoredIn,
            crate::inventory::CarriedBy,
        )>()
        .remove::<Networked>()
        .insert(OrganOf(mob));
    if let Some(mut state) = world.get_mut::<Organ>(organ) {
        state.stop_at = None;
    }
    on_insert(world, organ, mob, slot);
    true
}

/// `on_mob_insert` of the organ types.
fn on_insert(world: &mut World, organ: Entity, mob: Entity, slot: Slot) {
    match slot {
        Slot::Brain => super::traumas::refresh(world, mob),
        Slot::Stomach => crate::chem::stomach_inserted(world, organ, mob),
        Slot::Eyes => refresh_look(world, mob),
        Slot::Tongue => {
            world
                .entity_mut(mob)
                .remove::<ss13_chat::speech::Tongueless>();
        }
        _ => {}
    }
}

/// tg `Remove(owner, special)`: out of its owner, nowhere yet (the caller places it). Not
/// `special`: a vital organ kills, a heart stops 12 s later.
pub fn remove(world: &mut World, organ: Entity, special: bool) {
    let Some(mob) = world.get::<OrganOf>(organ).map(|o| o.0) else {
        return;
    };
    let Some(state) = world.get::<Organ>(organ).cloned() else {
        return;
    };
    if state.slot == Slot::Brain {
        super::traumas::let_go(world, organ);
    }
    world.entity_mut(organ).remove::<OrganOf>();
    match state.slot {
        Slot::Heart if !special => {
            let now = world.resource::<Time>().elapsed_secs_f64();
            if let Some(mut heart) = world.get_mut::<Organ>(organ) {
                heart.stop_at = Some(now + HEART_STOP_DELAY);
            }
        }
        Slot::Stomach => crate::chem::stomach_removed(world, organ, mob),
        Slot::Eyes => refresh_look(world, mob),
        Slot::Tongue => {
            world.entity_mut(mob).insert(ss13_chat::speech::Tongueless);
        }
        Slot::Brain => super::traumas::refresh(world, mob),
        _ => {}
    }
    if state.spec().vital && !special && world.get::<Stat>(mob) != Some(&Stat::Dead) {
        kill(world, mob);
    }
}

/// tg `death()`.
pub fn kill(world: &mut World, mob: Entity) {
    if let Some(mut stat) = world.get_mut::<Stat>(mob) {
        stat.set_if_neq(Stat::Dead);
    }
}

/// The body is drawn again (tg `update_body_parts`): eyes follow the organ.
fn refresh_look(world: &mut World, mob: Entity) {
    let eyeless = organ(world, mob, Slot::Eyes).is_none();
    if let Some(mut look) = world.get_mut::<crate::human::HumanLook>(mob)
        && look.eyeless != eyeless
    {
        look.eyeless = eyeless;
    }
    crate::inventory::refresh_look(world, mob);
}

/// tg `needs_heart()` of a human, `undergoing_cardiac_arrest()`: no beating heart.
pub fn cardiac_arrest(world: &World, mob: Entity) -> bool {
    world.get::<OrganHost>(mob).is_some()
        && state(world, mob, Slot::Heart).is_none_or(|h| !h.beating)
}

/// tg `get_heart_blood_regeneration_multiplier()`: 0 without a working, beating heart, else
/// its share of health; 1 for a mob without modelled organs.
pub fn heart_blood_multiplier(world: &World, mob: Entity) -> f32 {
    if world.get::<OrganHost>(mob).is_none() {
        return 1.0;
    }
    match state(world, mob, Slot::Heart) {
        Some(h) if h.beating && !h.failing => {
            ((h.spec().max_health - h.damage) / h.spec().max_health).clamp(0.0, 1.0)
        }
        _ => 0.0,
    }
}

/// tg `set_heartattack(status)`: stops (`can_heartattack`: an organic heart) or restarts the
/// heart. True if it changed.
pub fn set_heartattack(world: &mut World, mob: Entity, status: bool) -> bool {
    let Some(heart) = organ(world, mob, Slot::Heart) else {
        return false;
    };
    let Some(mut state) = world.get_mut::<Organ>(heart) else {
        return false;
    };
    if status && state.robotic {
        return false;
    }
    if state.beating == !status {
        return false;
    }
    state.beating = !status;
    let icon = if status { "heart-off" } else { "heart-on" };
    crate::obj_damage::set_icon_state(world, heart, icon);
    true
}

/// tg `regenerate_organs()` of `fully_heal(HEAL_ORGANS)`: missing species organs come back,
/// every organ heals, the heart beats.
pub fn regenerate(world: &mut World, mob: Entity) {
    if world.get::<OrganHost>(mob).is_none() {
        return;
    }
    for slot in Slot::ALL {
        if organ(world, mob, slot).is_none()
            && let Some(new) = spawn_organ(world, slot.spec().path)
        {
            insert(world, new, mob);
        }
        set_organ_damage(world, mob, slot, 0.0);
        if let Some(o) = organ(world, mob, slot)
            && let Some(mut state) = world.get_mut::<Organ>(o)
        {
            state.failure_time = 0.0;
            state.deafness = 0.0;
        }
    }
    set_heartattack(world, mob, false);
}

/// tg `handle_organs` and the organ parts of human `Life()` (`handle_heart`, `handle_liver`)
/// for one `SSmobs` fire of `secs`.
pub fn life(world: &mut World, mob: Entity, secs: f32, dead: bool, bodytemperature: f64) {
    if world.get::<OrganHost>(mob).is_none() {
        return;
    }
    let organs: Vec<Entity> = world
        .get::<Organs>(mob)
        .map(|o| o.iter().collect())
        .unwrap_or_default();
    if dead {
        if let Some(brain) = organ(world, mob, Slot::Brain) {
            super::traumas::let_go(world, brain);
        }
        let preserved = PRESERVATIVES
            .iter()
            .any(|p| crate::chem::blood_amount(world, mob, p) >= 1.0);
        if preserved {
            return;
        }
        for organ in organs {
            if let Some(mut state) = world.get::<Organ>(organ).cloned() {
                state.decay(bodytemperature, secs);
                if world.get::<Organ>(organ) != Some(&state) {
                    world.entity_mut(organ).insert(state);
                }
            }
        }
        return;
    }
    let mut sorted: Vec<(Slot, Entity)> = organs
        .into_iter()
        .filter_map(|o| Some((world.get::<Organ>(o)?.slot, o)))
        .collect();
    sorted.sort_by_key(|&(slot, _)| Slot::ALL.iter().position(|&s| s == slot));
    let satiety = world
        .get::<crate::hunger::Hunger>(mob)
        .map_or(0.0, |h| h.satiety);
    for (slot, organ) in sorted {
        // An earlier organ (the brain) may have killed the mob, or reagents moved organs.
        if world.get::<OrganOf>(organ).is_none_or(|o| o.0 != mob)
            || world.get::<Stat>(mob) == Some(&Stat::Dead)
        {
            continue;
        }
        on_life(world, mob, organ, slot, satiety, secs);
    }
    if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return;
    }
    super::traumas::life(world, mob, secs);
    handle_heart(world, mob, secs);
    handle_liver(world, mob, secs);
}

fn on_life(world: &mut World, mob: Entity, organ: Entity, slot: Slot, satiety: f32, secs: f32) {
    let Some(mut state) = world.get::<Organ>(organ).cloned() else {
        return;
    };
    let ears_prev = state.damage;
    let message = state.heal_tick(satiety, secs);
    let failing = state.failing;
    world.entity_mut(organ).insert(state.clone());
    if let Some(text) = message {
        tell_owner(world, mob, Span::Info, text);
    }
    if failing {
        organ_failure(world, mob, slot, state.failure_time, secs);
    }
    let mut rng = super::fork_rng(world);
    match slot {
        Slot::Brain => {
            if state.damage >= BRAIN_DAMAGE_DEATH {
                tell_owner(
                    world,
                    mob,
                    Span::Danger,
                    "The last spark of life in your brain fizzles out...",
                );
                kill(world, mob);
            }
        }
        Slot::Heart => {
            if (!state.beating || state.failing) && set_heartattack(world, mob, true) {
                let unconscious_or_crit = world
                    .get::<Stat>(mob)
                    .is_some_and(|&s| s != Stat::Conscious)
                    || world
                        .get::<Health>(mob)
                        .zip(world.get::<Stat>(mob))
                        .is_some_and(|(h, &s)| h.knocked_out(s));
                let name = name_of(world, mob);
                let own = "You feel a terrible pain in your chest, as if your heart has stopped!";
                let others = if unconscious_or_crit {
                    String::new()
                } else {
                    format!("{name} clutches at their chest as if their heart is stopping!")
                };
                visible(world, mob, own, &others, Span::Danger);
            }
        }
        Slot::Lungs => {
            let mut changed = state.clone();
            if state.lungs_failed && !state.failing {
                changed.lungs_failed = false;
            } else {
                if state.damage >= state.spec().low_threshold {
                    let percent = if state.damage < state.spec().high_threshold {
                        2.5
                    } else {
                        5.0
                    };
                    if super::wounds::spt_prob(&mut rng, percent, secs) {
                        cough(world, mob);
                    }
                }
                let awake = world
                    .get::<Stat>(mob)
                    .is_some_and(|&s| s == Stat::Conscious)
                    && !world
                        .get::<Health>(mob)
                        .zip(world.get::<Stat>(mob))
                        .is_some_and(|(h, &s)| h.knocked_out(s));
                if state.failing && awake {
                    let name = name_of(world, mob);
                    visible(
                        world,
                        mob,
                        "You suddenly feel like you can't breathe!",
                        &format!("{name} grabs their throat, struggling for breath!"),
                        Span::Danger,
                    );
                    changed.lungs_failed = true;
                }
            }
            if changed != state {
                world.entity_mut(organ).insert(changed);
            }
        }
        Slot::Ears => {
            let low = state.spec().low_threshold;
            let high = state.spec().high_threshold;
            let mut changed = state.clone();
            if (state.damage > low && ears_prev < low) || (state.damage > high && ears_prev < high)
            {
                tell_owner(
                    world,
                    mob,
                    Span::Warning,
                    "The ringing in your ears grows louder, blocking out any external noises for \
                     a moment.",
                );
            }
            if !state.failing {
                changed.deafness = (changed.deafness - secs).max(0.0);
                if state.damage > low
                    && super::wounds::spt_prob(&mut rng, state.damage / 60.0, secs)
                {
                    changed.deafness += 4.0;
                }
            }
            if changed != state {
                world.entity_mut(organ).insert(changed);
            }
        }
        _ => {}
    }
    world.resource_mut::<crate::combat::CombatRng>().0 = rng.0;
}

/// `organ_failure` of the failing liver (the other modelled organs have none).
fn organ_failure(world: &mut World, mob: Entity, slot: Slot, failure_time: f32, secs: f32) {
    if slot != Slot::Liver {
        return;
    }
    let stage = failure_time / LIVER_FAILURE_STAGE_SECONDS;
    let text = match stage as u32 {
        1 if stage.fract() < secs / LIVER_FAILURE_STAGE_SECONDS => {
            Some("You feel stabbing pain in your abdomen!")
        }
        2 if stage.fract() < secs / LIVER_FAILURE_STAGE_SECONDS => {
            Some("You feel a burning sensation in your gut!")
        }
        3 if stage.fract() < secs / LIVER_FAILURE_STAGE_SECONDS => {
            Some("You feel painful acid in your throat!")
        }
        4 if stage.fract() < secs / LIVER_FAILURE_STAGE_SECONDS => {
            Some("Overwhelming pain knocks you out!")
        }
        5 if stage.fract() < secs / LIVER_FAILURE_STAGE_SECONDS => {
            Some("You feel as if your guts are about to melt!")
        }
        _ => None,
    };
    if let Some(text) = text
        && let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client)
    {
        tell(world, client, Span::Danger, text);
    }
    let (tox, organ_damage) = match stage as u32 {
        0 => (0.0, 0.0),
        1 => (0.2, 0.0),
        2 => (0.4, 0.0),
        3 => (0.6, 0.2),
        _ => (0.8, 0.5),
    };
    if tox > 0.0 {
        super::hurt_limb(world, mob, BodyZone::Chest, DamageType::Tox, tox * secs);
    }
    if organ_damage > 0.0 {
        let target = random_failure_target(world);
        adjust_organ_loss(world, mob, target, organ_damage * secs);
    }
}

/// `pick(ORGAN_SLOT_HEART, ORGAN_SLOT_LUNGS, ORGAN_SLOT_STOMACH, ORGAN_SLOT_EYES,
/// ORGAN_SLOT_EARS)` of the liver failure.
fn random_failure_target(world: &mut World) -> Slot {
    const TARGETS: [Slot; 5] = [
        Slot::Heart,
        Slot::Lungs,
        Slot::Stomach,
        Slot::Eyes,
        Slot::Ears,
    ];
    let mut rng = super::fork_rng(world);
    TARGETS[rng.range(0, 4) as usize]
}

/// `handle_heart`: without a beating heart, 4 oxygen damage a second (the human breathes) and
/// 1 brute.
// shortcut: no `Unconscious(80)` (no unconsciousness status yet); the oxygen damage knocks out
// at 50.
fn handle_heart(world: &mut World, mob: Entity, secs: f32) {
    if !cardiac_arrest(world, mob) {
        return;
    }
    super::hurt_limb(world, mob, BodyZone::Chest, DamageType::Oxy, 4.0 * secs);
    super::hurt_limb(world, mob, BodyZone::Chest, DamageType::Brute, 1.0 * secs);
}

/// `handle_liver` without a liver: 0.6 toxin a second and 0.5 damage to a random organ (the
/// liverless metabolism is `crate::chem`'s).
fn handle_liver(world: &mut World, mob: Entity, secs: f32) {
    if !lacks(world, mob, Slot::Liver) {
        return;
    }
    super::hurt_limb(world, mob, BodyZone::Chest, DamageType::Tox, 0.6 * secs);
    let target = random_failure_target(world);
    adjust_organ_loss(world, mob, target, 0.5 * secs);
}

/// `emote("cough")`.
fn cough(world: &mut World, mob: Entity) {
    if let Some(emote) = ss13_chat::emote::find("cough") {
        crate::mobs::defense::emote(world, mob, emote.message);
    }
}

/// The ears' `TRAIT_DEAF` sources: no ears (`NO_EARS`), failing ears or temporary deafness
/// (`EAR_DAMAGE`).
pub fn ear_deaf<'a>(
    organs: Option<&Organs>,
    host: bool,
    state: impl Fn(Entity) -> Option<&'a Organ>,
) -> bool {
    if !host {
        return false;
    }
    let ears = organs.and_then(|o| o.iter().filter_map(&state).find(|s| s.slot == Slot::Ears));
    ears.is_none_or(|e| e.failing || e.deafness > 0.0)
}

/// The air temperature around a removed organ (tg `return_air()` of its location).
fn temperature_around(world: &World, organ: Entity) -> Option<f64> {
    let mut at = organ;
    for _ in 0..4 {
        if let Some(&pos) = world.get::<TilePos>(at) {
            let atmos = world.get_resource::<ss13_atmos::Atmos>();
            return Some(
                atmos
                    .and_then(|a| a.air(pos).map(|air| air.temperature))
                    .unwrap_or(crate::breath::station_air().temperature),
            );
        }
        if let Some(container) = world.get::<crate::storage::StoredIn>(at).map(|s| s.0) {
            if world
                .get::<ss13_mapload::DmObj>(container)
                .is_some_and(|o| FREEZERS.iter().any(|f| is_under(&o.path, f)))
            {
                return Some(FREEZER_TEMPERATURE);
            }
            at = container;
        } else {
            at = world.get::<crate::inventory::CarriedBy>(at)?.0;
        }
    }
    None
}

/// tg `process()` of removed organs (`on_death` decay) and the heart's `stop_if_unowned`, each
/// `SSmobs`-length fire.
// Cost: every 2 s one pass over organs outside bodies (removed by surgery or spawned loose: a
// handful per round), a few component reads each.
fn loose_organs(world: &mut World, mut last_fired: Local<u64>) {
    let fired = world.resource::<crate::breath::MobsClock>().times_fired;
    let fires = fired.saturating_sub(*last_fired);
    *last_fired = fired;
    if fires == 0 {
        return;
    }
    let secs = crate::breath::SSMOBS_WAIT_SECS as f32;
    let now = world.resource::<Time>().elapsed_secs_f64();
    process_loose(world, fires, secs, now);
}

fn process_loose(world: &mut World, fires: u64, secs: f32, now: f64) {
    let loose: Vec<Entity> = world
        .query_filtered::<Entity, (With<Organ>, Without<OrganOf>)>()
        .iter(world)
        .collect();
    for organ in loose {
        let Some(mut state) = world.get::<Organ>(organ).cloned() else {
            continue;
        };
        if let Some(temperature) = temperature_around(world, organ) {
            for _ in 0..fires {
                state.decay(temperature, secs);
            }
        }
        let stop = state.stop_at.is_some_and(|t| now >= t);
        if stop {
            state.stop_at = None;
        }
        if world.get::<Organ>(organ) != Some(&state) {
            world.entity_mut(organ).insert(state.clone());
        }
        if stop && state.slot == Slot::Heart && !state.robotic && state.beating {
            if let Some(mut heart) = world.get_mut::<Organ>(organ) {
                heart.beating = false;
            }
            crate::obj_damage::set_icon_state(world, organ, "heart-off");
        }
    }
}

/// A removed organ's prototype is an item: give spawned organ items (mapped, from crates)
/// their state too.
fn organ_items(
    add: On<Add<DmItem>>,
    items: Query<&DmItem, Without<Organ>>,
    mut commands: Commands,
) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    if let Some(slot) = Slot::of_path(&item.path) {
        commands
            .entity(add.entity)
            .try_insert_if_new(Organ::new(slot, &item.path));
    }
}

pub(super) fn build(app: &mut App) {
    app.add_observer(give_organs)
        .add_observer(organ_items)
        .add_systems(Update, loose_organs.before(crate::health::HealthSystems));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thresholds_fail_and_recover_like_tg() {
        let mut heart = Organ::new(Slot::Heart, "/obj/item/organ/heart");
        assert_eq!(heart.apply_damage(5.0, 100.0), None);
        assert_eq!(
            heart.apply_damage(6.0, 100.0),
            Some(Slot::Heart.spec().low_passed)
        );
        assert_eq!(
            heart.apply_damage(40.0, 100.0),
            Some(Slot::Heart.spec().high_passed)
        );
        // `maximum` caps the damage and refuses once the organ is past it.
        assert_eq!(heart.apply_damage(100.0, 60.0), None);
        assert_eq!(heart.damage, 60.0);
        heart.apply_damage(100.0, 100.0);
        assert!(heart.failing && heart.damage == 100.0);
        assert_eq!(
            heart.set_damage(30.0),
            Some(Slot::Heart.spec().high_cleared)
        );
        assert!(!heart.failing);
    }

    #[test]
    fn decay_and_healing_follow_tg_factors() {
        // A corpse's heart fails after about 6 minutes at body temperature.
        let mut heart = Organ::new(Slot::Heart, "/obj/item/organ/heart");
        let mut secs = 0.0;
        while !heart.failing {
            heart.decay(310.15, 2.0);
            secs += 2.0;
        }
        assert!((355.0..=365.0).contains(&secs), "{secs}");
        // The brain: half the rate of twice the health, 30 minutes.
        let mut brain = Organ::new(Slot::Brain, "/obj/item/organ/brain");
        brain.decay(310.15, 600.0);
        assert!((brain.damage - 66.6).abs() < 0.1, "{}", brain.damage);
        // Cold stops decay; robotic organs never decay.
        let mut cold = Organ::new(Slot::Liver, "/obj/item/organ/liver");
        cold.decay(T0C - 1.0, 100.0);
        assert_eq!(cold.damage, 0.0);
        let mut cyber = Organ::new(Slot::Liver, "/obj/item/organ/liver/cybernetic");
        cyber.decay(310.15, 100.0);
        assert_eq!(cyber.damage, 0.0);
        // The living heal `healing_factor` of max health a second, up to 5x with satiety.
        let mut liver = Organ::new(Slot::Liver, "/obj/item/organ/liver");
        liver.apply_damage(50.0, 100.0);
        liver.heal_tick(0.0, 100.0);
        assert!((liver.damage - 45.0).abs() < 1e-3);
        liver.heal_tick(crate::hunger::MAX_SATIETY, 100.0);
        assert!((liver.damage - 20.0).abs() < 1e-3);
        // The brain does not heal on its own (`healing_factor` 0).
        brain.heal_tick(0.0, 100.0);
        assert!((brain.damage - 66.6).abs() < 0.1);
    }

    #[test]
    fn slots_of_paths() {
        assert_eq!(
            Slot::of_path("/obj/item/organ/heart/cybernetic/tier2"),
            Some(Slot::Heart)
        );
        assert_eq!(
            Slot::of_path("/obj/item/organ/tongue/lizard"),
            Some(Slot::Tongue)
        );
        assert_eq!(Slot::of_path("/obj/item/organ/tail"), None);
        assert_eq!(Slot::Eyes.bodypart(), BodyZone::Head);
        assert_eq!(Slot::Appendix.bodypart(), BodyZone::Chest);
    }
}

#[cfg(test)]
mod world_tests {
    use super::*;

    /// A conscious human with the species organs (bare organ entities: no prototypes here).
    fn human(world: &mut World) -> Entity {
        world.init_resource::<Time>();
        let mob = world
            .spawn((Health::default(), Stat::Conscious, OrganHost))
            .id();
        for slot in Slot::ALL {
            let organ = spawn_organ(world, slot.spec().path).unwrap();
            world.entity_mut(organ).insert(OrganOf(mob));
        }
        mob
    }

    #[test]
    fn a_missing_or_failing_heart_is_cardiac_arrest() {
        let mut world = World::new();
        let mob = human(&mut world);
        assert!(!cardiac_arrest(&world, mob));
        life(&mut world, mob, 2.0, false, 310.15);
        assert_eq!(world.get::<Health>(mob).unwrap().oxy, 0.0);
        // Out of the body: no heartbeat, 4 oxygen and 1 brute a second.
        let heart = organ(&world, mob, Slot::Heart).unwrap();
        remove(&mut world, heart, false);
        assert!(lacks(&world, mob, Slot::Heart) && cardiac_arrest(&world, mob));
        assert!(world.get::<Organ>(heart).unwrap().stop_at.is_some());
        life(&mut world, mob, 2.0, false, 310.15);
        let health = world.get::<Health>(mob).unwrap();
        assert_eq!((health.oxy, health.brute), (8.0, 2.0));
        // A failing heart stops on its own; a restart needs a heart that works.
        insert(&mut world, heart, mob);
        assert!(!cardiac_arrest(&world, mob));
        set_organ_damage(&mut world, mob, Slot::Heart, 100.0);
        life(&mut world, mob, 2.0, false, 310.15);
        assert!(cardiac_arrest(&world, mob));
        // The removed heart's timer stops a heart lying around 12 s later.
        let spare = spawn_organ(&mut world, "/obj/item/organ/heart").unwrap();
        world.entity_mut(spare).insert(OrganOf(mob));
        remove(&mut world, spare, false);
        process_loose(&mut world, 1, 2.0, 11.0);
        assert!(world.get::<Organ>(spare).unwrap().beating);
        process_loose(&mut world, 1, 2.0, 12.0);
        assert!(!world.get::<Organ>(spare).unwrap().beating);
    }

    #[test]
    fn the_brain_is_vital() {
        let mut world = World::new();
        let mob = human(&mut world);
        // 200 brain damage kills at the next `Life()`.
        set_organ_damage(&mut world, mob, Slot::Brain, 200.0);
        assert_eq!(world.get::<Stat>(mob), Some(&Stat::Conscious));
        life(&mut world, mob, 2.0, false, 310.15);
        assert_eq!(world.get::<Stat>(mob), Some(&Stat::Dead));
        // Taking it out kills (not a `special` swap).
        let mob = human(&mut world);
        let brain = organ(&world, mob, Slot::Brain).unwrap();
        remove(&mut world, brain, true);
        assert_eq!(world.get::<Stat>(mob), Some(&Stat::Conscious));
        insert(&mut world, brain, mob);
        remove(&mut world, brain, false);
        assert_eq!(world.get::<Stat>(mob), Some(&Stat::Dead));
        // The dead do not heal; their organs decay unless it is cold.
        insert(&mut world, brain, mob);
        life(&mut world, mob, 100.0, true, 260.0);
        assert_eq!(world.get::<Organ>(brain).unwrap().damage, 0.0);
        life(&mut world, mob, 100.0, true, 310.15);
        assert!(world.get::<Organ>(brain).unwrap().damage > 10.0);
    }

    #[test]
    fn no_liver_poisons_and_regeneration_restores() {
        let mut world = World::new();
        let mob = human(&mut world);
        let liver = organ(&world, mob, Slot::Liver).unwrap();
        remove(&mut world, liver, false);
        world.entity_mut(liver).despawn();
        life(&mut world, mob, 2.0, false, 310.15);
        assert!((world.get::<Health>(mob).unwrap().tox - 1.2).abs() < 1e-4);
        // A failing liver: no toxin from `handle_liver`, but its failure stages after a minute.
        regenerate(&mut world, mob);
        assert!(!lacks(&world, mob, Slot::Liver));
        set_organ_damage(&mut world, mob, Slot::Liver, 100.0);
        let tox = world.get::<Health>(mob).unwrap().tox;
        for _ in 0..29 {
            life(&mut world, mob, 2.0, false, 310.15);
        }
        assert_eq!(world.get::<Health>(mob).unwrap().tox, tox);
        for _ in 0..5 {
            life(&mut world, mob, 2.0, false, 310.15);
        }
        assert!(world.get::<Health>(mob).unwrap().tox > tox);
        regenerate(&mut world, mob);
        let liver = state(&world, mob, Slot::Liver).unwrap();
        assert!(!liver.failing && liver.damage == 0.0);
    }

    #[test]
    fn a_removed_stomach_keeps_its_contents() {
        use ss13_reagents::{Holder, Owner};
        let mut world = World::new();
        let mob = human(&mut world);
        let holder = || Holder::new(1000.0, 0, Owner::default());
        let mut stomach = holder();
        stomach.total_volume = 5.0;
        world.entity_mut(mob).insert(crate::chem::MobReagents {
            blood: holder(),
            stomach,
            drunk: 0.0,
        });
        let organ = organ(&world, mob, Slot::Stomach).unwrap();
        remove(&mut world, organ, false);
        let left = |world: &World| {
            world
                .get::<crate::chem::MobReagents>(mob)
                .unwrap()
                .stomach
                .total_volume
        };
        assert_eq!(left(&world), 0.0);
        assert!(lacks(&world, mob, Slot::Stomach));
        insert(&mut world, organ, mob);
        assert_eq!(left(&world), 5.0);
    }

    #[test]
    fn ears_deafen_when_missing_failing_or_ringing() {
        let mut world = World::new();
        let mob = human(&mut world);
        let deaf =
            |world: &World| ear_deaf(world.get::<Organs>(mob), true, |e| world.get::<Organ>(e));
        assert!(!deaf(&world));
        let ears = organ(&world, mob, Slot::Ears).unwrap();
        world.get_mut::<Organ>(ears).unwrap().deafness = 4.0;
        assert!(deaf(&world));
        // The ringing fades a second per second.
        life(&mut world, mob, 2.0, false, 310.15);
        life(&mut world, mob, 2.0, false, 310.15);
        assert!(!deaf(&world));
        set_organ_damage(&mut world, mob, Slot::Ears, 100.0);
        assert!(deaf(&world));
        remove(&mut world, ears, false);
        assert!(deaf(&world));
        // A mob without modelled organs hears.
        assert!(!ear_deaf(None, false, |_| None));
    }
}
