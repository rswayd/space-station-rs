//! Medicine (server): the zone selector's server half, medical stacks, health analyzers and CPR,
//! after tg `code/game/objects/items/stacks/medical.dm` (`begin_heal_loop`, `try_heal`,
//! `try_heal_checks`, `heal_carbon`), `/obj/item/stack/medical/wrap` with
//! `/datum/component/limb_applicable`, `/obj/item/healthanalyzer` and
//! `/mob/living/carbon/human/do_cpr`.
//!
//! - Zone selection: the `zone` act of the health HUD window sets [`ZoneSelected`].
//! - A medical stack clicked on a living mob heals the selected zone's bodypart (`check_zone`):
//!   `heal_brute`/`heal_burn` after `self_delay` (on yourself) or `other_delay` (on someone
//!   else) through `crate::tools::do_after` (moving, dropping the stack or the patient moving
//!   cancels), one unit used per application. Left click picks the next damaged limb when the
//!   zone is done (`auto_change_zone`), right click stays on the selected zone (90% delay).
//!   Repeating stacks (sutures, meshes) go on while there is more and something to heal.
//! - Wraps (gauze) go on a limb (`always_applicable`: also without wounds) and replace a wrap
//!   at least 20% worse; they soak bleeding, splint fractures and dress burns
//!   (`crate::medical::wounds`). Sutures take bleeding off a cut, ointment and meshes dress burns.
//! - A health analyzer clicked on a living mob prints tg `healthscan` to the user's chat; the
//!   wound analyzer (`/simple`) prints tg `woundscan`.
//! - CPR: an empty hand out of combat mode on a lying mob that is not stable (`crate::combat`
//!   calls [`do_cpr`]): 3 s, then up to 7 oxyloss healed while the patient is in crit, again
//!   every 0.8 s while it stays there.
//!
//! Submodules: [`blood`] (blood volume, bleeding, transfusions, IV drips), [`wounds`], [`surgery`]
//! (operations, drapes, operating tables), [`defib`] (defibrillators, organ decay, revival).
//! Clicks on living mobs that these take (wound treatments, drapes, surgery, paddles, IV drips,
//! tables) are claimed in [`MedicalClicks`] before melee and the inventory see them.
// shortcut: no `try_inject` check (thick clothing blocks treatment in tg), no sounds, balloon
// alerts are chat notices.

pub mod blood;
pub mod defib;
pub mod operating;
pub mod surgery;
pub mod wounds;

use bevy::ecs::entity::EntityHashSet;
use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmMedical, DmName};
use ss13_net::chat::server::SendChat;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::{ClickModifiers, InventorySlot};
use ss13_tgui::server::{UiActed, UiSystems};

use crate::combat::CombatRng;
use crate::health::{
    DEFAULT_MESSAGE_RANGE, DamageType, HEALTH_THRESHOLD_CRIT, Health, Stat, ZoneSelected,
    update_stat,
};
use crate::hud::HealthHudUi;
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::server::PlayerMob;
use crate::tools::{Actor, DoAfter, do_after, stack_amount, tell, use_stack};
use crate::zone::BodyZone;

pub const MEDICAL_ROOT: &str = "/obj/item/stack/medical";
const WRAP_ROOT: &str = "/obj/item/stack/medical/wrap";
pub const ANALYZER_ROOT: &str = "/obj/item/healthanalyzer";
const WOUND_ANALYZER: &str = "/obj/item/healthanalyzer/simple";
/// tg `do_cpr`: `3 SECONDS`, `CPR_PANIC_SPEED`, deciseconds.
pub const CPR_DELAY: f32 = 30.0;
pub const CPR_PANIC_DELAY: f32 = 8.0;
/// Oxyloss one CPR heals at most.
pub const CPR_OXY_HEAL: f32 = 7.0;

/// The medical part of tg `fully_heal(HEAL_ALL)` (admin rejuvenate): `restore_blood` and
/// `HEAL_WOUNDS`. Damage and stat are the caller's.
pub fn heal_all(world: &mut World, mob: Entity) {
    if let Some(mut blood) = world.get_mut::<blood::Blood>(mob) {
        blood.volume = blood::BLOOD_VOLUME_NORMAL;
    }
    if let Some(mut limbs) = world.get_mut::<wounds::Limbs>(mob) {
        for limb in &mut limbs.parts {
            while !limb.wounds.is_empty() {
                wounds::remove_wound(limb, 0);
            }
        }
    }
}

/// A medical item: its `interact_with_atom` takes clicks on living mobs before any attack.
pub fn interacts_with_mobs(path: &str) -> bool {
    is_under(path, MEDICAL_ROOT) || is_under(path, ANALYZER_ROOT)
}

/// A medical stack application in progress (see [`crate::construction::Job::Heal`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HealJob {
    pub zone: BodyZone,
    /// tg `auto_change_zone` (left click).
    pub auto: bool,
    /// A wrap going on the limb (`limb_applicable`), not a heal.
    pub wrap: bool,
}

/// A timed medical action ([`crate::construction::Job::Medical`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MedJob {
    /// A blood sample from someone else ([`blood::finish_draw`]).
    DrawBlood,
    /// An IV drip being attached ([`blood::finish_attach`]).
    AttachIv { drip: Entity },
    /// A surgical operation ([`surgery::complete`]).
    Surgery {
        op: surgery::Op,
        zone: crate::zone::TargetZone,
    },
    /// Buckling someone to a table or a bed ([`surgery::buckle`]).
    Buckle { to: Entity },
    /// A surgery disk loading into an operating computer ([`operating::finish_load`]).
    LoadDisk,
    /// A stage of a defibrillator shock ([`defib::complete`]).
    Defib { unit: Entity, stage: defib::Stage },
    /// A wound treatment ([`wounds::finish_treatment`]).
    Treat {
        zone: BodyZone,
        treat: wounds::Treat,
        kind: wounds::WoundKind,
    },
}

/// The end of a [`MedJob`] `do_after`.
pub fn complete_med(world: &mut World, mob: Entity, action: &DoAfter, job: MedJob) {
    let actor = Actor {
        client: action.client,
        mob,
        item: action.item,
    };
    let target = action.target;
    match job {
        MedJob::DrawBlood => {
            if let Some(item) = action.item {
                blood::finish_draw(world, actor, item, target);
            }
        }
        MedJob::AttachIv { drip } => blood::finish_attach(world, actor, drip, target),
        MedJob::Surgery { op, zone } => surgery::complete(world, mob, action, op, zone),
        MedJob::Buckle { to } => surgery::buckle(world, actor, target, to),
        MedJob::LoadDisk => operating::finish_load(world, actor, target),
        MedJob::Defib { unit, stage } => defib::complete(world, mob, action, unit, stage),
        MedJob::Treat { zone, treat, kind } => {
            wounds::finish_treatment(world, actor, target, zone, treat, kind);
        }
    }
}

/// Mobs whose click this tick a medical interaction took: melee and the inventory skip them.
#[derive(Resource, Default)]
pub struct MedicalHandled(pub EntityHashSet);

/// [`medical_clicks`] runs in this set, before melee and the inventory.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MedicalClicks;

/// What the wounds and buckling do to moving: [`wounds::Limits`] plus buckled immobility.
/// Read by `crate::status_effects` (floored, immobilized) and `crate::health` (slowdown).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct LimbLimits {
    /// Deciseconds per step.
    pub slowdown: f32,
    pub floored: bool,
    pub immobilized: bool,
}

pub struct MedicalServerPlugin;

impl Plugin for MedicalServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MedicalHandled>()
            .add_observer(give_body)
            .add_observer(give_drip)
            .add_systems(Update, select_zone.in_set(UiSystems::Handle))
            .add_systems(
                Update,
                medical_clicks
                    .in_set(MedicalClicks)
                    .after(crate::click::ResolveClicks)
                    .before(crate::combat::MeleeSystems),
            )
            .add_systems(
                Update,
                (medical_life, machines_life, refresh_limits)
                    .chain()
                    .before(crate::status_effects::StatusSystems)
                    .before(crate::health::HealthSystems),
            );
        surgery::build(app);
        operating::build(app);
        defib::build(app);
    }
}

/// A human's blood, limbs and organs (tg carbon `Initialize`, `dna.blood_type`).
fn give_body(
    add: On<Add<Health>>,
    basics: Query<(), With<crate::mobs::BasicMob>>,
    mut rng: ResMut<CombatRng>,
    mut commands: Commands,
) {
    // shortcut: basic mobs have no blood or limbs (tg gives them `blood_volume`).
    if basics.contains(add.entity) {
        return;
    }
    let blood_type = blood::BloodType::random_human(&mut rng);
    commands.entity(add.entity).insert((
        blood::Blood::new(blood_type),
        wounds::Limbs::default(),
        defib::OrganDecay::default(),
        LimbLimits::default(),
    ));
}

/// An IV drip's state (`/obj/machinery/iv_drip`).
fn give_drip(
    add: On<Add<ss13_mapload::DmObj>>,
    objs: Query<&ss13_mapload::DmObj>,
    mut commands: Commands,
) {
    if objs
        .get(add.entity)
        .is_ok_and(|o| is_under(&o.path, blood::IV_DRIP_ROOT))
    {
        commands.entity(add.entity).insert(blood::IvDrip::default());
    }
}

/// A copy of the combat rng for one batch of rolls; the shared one advances.
pub(crate) fn fork_rng(world: &mut World) -> CombatRng {
    let mut rng = world.get_resource_or_init::<CombatRng>();
    let high = u64::from(rng.range(0, u32::MAX - 1));
    let low = u64::from(rng.range(0, u32::MAX - 1));
    CombatRng(high << 32 | low)
}

/// `apply_damage(amount, kind, zone, wound_bonus = CANT_WOUND)` with the stat update.
pub(crate) fn hurt_limb(
    world: &mut World,
    mob: Entity,
    zone: BodyZone,
    kind: DamageType,
    amount: f32,
) {
    let (Some(mut health), Some(&stat)) =
        (world.get::<Health>(mob).cloned(), world.get::<Stat>(mob))
    else {
        return;
    };
    let mut new = stat;
    crate::health::apply_damage_zone(&mut health, &mut new, zone, kind, amount);
    world.entity_mut(mob).insert(health);
    if new != stat {
        world.entity_mut(mob).insert(new);
    }
}

/// tg `/atom/movable/screen/zone_sel/set_selected_zone`: only the HUD's own mob.
fn select_zone(
    mut acts: Local<MessageCursor<UiActed<HealthHudUi>>>,
    messages: Res<Messages<UiActed<HealthHudUi>>>,
    mut mobs: Query<&mut ZoneSelected>,
) {
    for act in acts.read(&messages) {
        if act.actor != act.source {
            continue;
        }
        if let Ok(mut zone) = mobs.get_mut(act.actor) {
            zone.set_if_neq(ZoneSelected(act.act));
        }
    }
}

/// tg `get_random_valid_zone(base_zone, probability)` on a human: the zone with `probability`
/// percent, else a random bodypart weighted head 1, chest 1, limbs 4 (`ran_zone`).
pub fn random_valid_zone(rng: &mut CombatRng, base: BodyZone, probability: f32) -> BodyZone {
    if rng.chance(probability) {
        return base;
    }
    let roll = rng.range(1, 18);
    match roll {
        1 => BodyZone::Head,
        2 => BodyZone::Chest,
        3..=6 => BodyZone::LArm,
        7..=10 => BodyZone::RArm,
        11..=14 => BodyZone::LLeg,
        _ => BodyZone::RLeg,
    }
}

pub(crate) fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<Name>(e)
        .map(|n| n.as_str().to_owned())
        .or_else(|| world.get::<DmName>(e).map(|n| n.name.clone()))
        .unwrap_or_else(|| "кто-то".to_owned())
}

fn selected_zone(world: &World, mob: Entity) -> BodyZone {
    world
        .get::<ZoneSelected>(mob)
        .copied()
        .unwrap_or_default()
        .0
        .check_zone()
}

/// tg `visible_message(message, self_message)` with `ALWAYS_SHOW_SELF_MESSAGE`: `others` to
/// every client in view of `actor` but the actor, `own` to the actor.
// shortcut: range only, no line of sight, as `crate::health::clients_in_view`.
pub(crate) fn visible(world: &mut World, actor: Entity, own: &str, others: &str, span: Span) {
    visible_range(world, actor, own, others, span, DEFAULT_MESSAGE_RANGE);
}

/// [`visible`] with a `vision_distance`.
pub(crate) fn visible_range(
    world: &mut World,
    actor: Entity,
    own: &str,
    others: &str,
    span: Span,
    range: u32,
) {
    let Some(&pos) = world.get::<TilePos>(actor) else {
        return;
    };
    let clients = crate::obj_damage::clients_in_range(world, pos, range, &[actor]);
    let message = |text: &str| ChatMessage {
        channel: ChatChannel::Info,
        segments: vec![ChatSegment::new(span, text)],
    };
    if !clients.is_empty() {
        world.write_message(SendChat {
            clients,
            message: message(others),
        });
    }
    if let Some(client) = world.get::<PlayerMob>(actor).map(|p| p.client) {
        world.write_message(SendChat {
            clients: vec![client],
            message: message(own),
        });
    }
}

/// tg `base_item_interaction` -> `interact_with_atom` of `held` on a living `target`. True when
/// the click was the item's (it then never attacks).
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    if world.get::<Health>(target).is_none() {
        return false;
    }
    let Some(path) = world.get::<DmItem>(held).map(|i| i.path.clone()) else {
        return false;
    };
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    if world.get::<DmMedical>(held).is_some() {
        if is_under(&path, WRAP_ROOT) && !modifiers.right {
            begin_wrap(world, actor, target);
        } else {
            begin_heal_loop(world, actor, target, !modifiers.right);
        }
        return true;
    }
    if is_under(&path, ANALYZER_ROOT) {
        // shortcut: the right click chemical scan (`chemscan`) is not ported and does nothing.
        if !modifiers.right {
            analyze(world, actor, target, is_under(&path, WOUND_ANALYZER));
        }
        return true;
    }
    false
}

/// tg `begin_heal_loop`.
fn begin_heal_loop(world: &mut World, actor: Actor, patient: Entity, auto: bool) {
    // tg `DOING_INTERACTION_WITH_TARGET`.
    if world.get::<DoAfter>(actor.mob).is_some() {
        return;
    }
    let zone = selected_zone(world, actor.mob);
    if !try_heal_checks(world, actor, patient, zone, false) {
        return;
    }
    let verb = medical(world, actor).apply_verb;
    tell(
        world,
        actor.client,
        Span::Notice,
        format!("{verb} {}...", zone.ru().1),
    );
    try_heal(world, actor, patient, zone, false, auto);
}

fn medical(world: &World, actor: Actor) -> DmMedical {
    actor
        .item
        .and_then(|i| world.get::<DmMedical>(i))
        .cloned()
        .unwrap_or_default()
}

/// tg `try_heal` up to its `do_after`; [`complete_heal`] is the rest.
fn try_heal(
    world: &mut World,
    actor: Actor,
    patient: Entity,
    zone: BodyZone,
    silent: bool,
    auto: bool,
) {
    let med = medical(world, actor);
    let item = actor.item.map(|i| name_of(world, i)).unwrap_or_default();
    let (user, target) = (name_of(world, actor.mob), name_of(world, patient));
    let factor = if auto { 1.0 } else { 0.9 };
    let delay = if patient == actor.mob {
        if !silent {
            visible(
                world,
                actor.mob,
                &format!("Вы начинаете применять {item} на себя..."),
                &format!("{user} начинает применять {item} на себя..."),
                Span::Notice,
            );
        }
        med.self_delay * factor
    } else if med.other_delay > 0.0 {
        if !silent {
            visible(
                world,
                actor.mob,
                &format!("Вы начинаете применять {item} на {target}..."),
                &format!("{user} начинает применять {item} на {target}."),
                Span::Notice,
            );
        }
        med.other_delay * factor
    } else {
        if !silent {
            visible(
                world,
                actor.mob,
                &format!("Вы применяете {item} на {target}."),
                &format!("{user} применяет {item} на {target}."),
                Span::Notice,
            );
        }
        0.0
    };
    let job = HealJob {
        zone,
        auto,
        wrap: false,
    };
    do_after(
        world,
        actor,
        patient,
        delay,
        1,
        crate::construction::Job::Heal(job),
    );
}

/// tg `try_heal_checks` (with `can_heal`); feedback unless `silent`.
fn try_heal_checks(
    world: &mut World,
    actor: Actor,
    patient: Entity,
    zone: BodyZone,
    silent: bool,
) -> bool {
    let med = medical(world, actor);
    let (Some(health), Some(&stat)) = (world.get::<Health>(patient), world.get::<Stat>(patient))
    else {
        return false;
    };
    let fail = |world: &mut World, text: String| {
        if !silent {
            tell(world, actor.client, Span::Warning, text);
        }
        false
    };
    if !med.works_on_dead && stat == Stat::Dead {
        return fail(world, "мёртв!".to_owned());
    }
    let part = health.part(zone);
    let lying = world
        .get::<crate::health::BodyPosition>(patient)
        .is_some_and(|b| b.lying());
    let limb = world
        .get::<wounds::Limbs>(patient)
        .map(|l| l.limb(zone).clone())
        .unwrap_or_default();
    let burn_wound = limb.wound(wounds::Series::Burn);
    let can_heal_burn_wounds = (med.flesh_regeneration > 0.0 || med.sanitization > 0.0)
        && burn_wound.is_some_and(wounds::Wound::can_be_ointmented_or_meshed);
    let can_suture_bleeding = med.stop_bleeding > 0.0 && limb.bleed_rate(zone, lying) > 0.0;
    let brute_to_heal = med.heal_brute > 0.0 && part.brute > 0.0;
    let burn_to_heal = med.heal_burn > 0.0 && part.burn > 0.0;
    if brute_to_heal || burn_to_heal || can_heal_burn_wounds || can_suture_bleeding {
        return true;
    }
    let (name, _) = zone.ru();
    let text = if !brute_to_heal && med.stop_bleeding > 0.0 {
        format!("{name} не кровоточит или не повреждена!")
    } else if !burn_to_heal
        && (med.flesh_regeneration > 0.0 || med.sanitization > 0.0)
        && burn_wound.is_some()
    {
        format!("{name} полностью обработана - дайте время!")
    } else if part.damage() <= 0.0 {
        format!("{name} не повреждена!")
    } else {
        let item = actor.item.map(|i| name_of(world, i)).unwrap_or_default();
        format!("не вылечить {name} с помощью {item}!")
    };
    fail(world, text)
}

/// The end of tg `try_heal`'s `do_after`: `heal_carbon`, `use(1)` and the loop.
pub fn complete_heal(world: &mut World, mob: Entity, action: &DoAfter, job: HealJob) {
    let actor = Actor {
        client: action.client,
        mob,
        item: action.item,
    };
    let patient = action.target;
    if job.wrap {
        complete_wrap(world, actor, patient, job.zone);
        return;
    }
    let zone = if job.auto {
        job.zone
    } else {
        selected_zone(world, mob)
    };
    if !try_heal_checks(world, actor, patient, zone, false) {
        return;
    }
    let Some(item) = actor.item else {
        return;
    };
    let med = medical(world, actor);
    // tg `heal_carbon`.
    let (user, target, item_name) = (
        name_of(world, mob),
        name_of(world, patient),
        name_of(world, item),
    );
    let (zone_name, _) = zone.ru();
    visible(
        world,
        mob,
        &format!("Вы применяете {item_name} на {zone_name} у {target}."),
        &format!("{user} применяет {item_name} на {zone_name} у {target}."),
        Span::Notice,
    );
    heal(world, patient, zone, med.heal_brute, med.heal_burn);
    treat_wounds(world, patient, zone, &med, patient == mob);
    let used = use_stack(world, item, 1);
    let left = if used { stack_amount(world, item) } else { 0 };
    let item_gone = world.get_entity(item).is_err();
    if !used || !med.repeating || left == 0 || item_gone {
        let text = if med.repeating {
            "всё истрачено!".to_owned()
        } else {
            format!("{zone_name} обработана")
        };
        tell(world, actor.client, Span::Notice, text);
        return;
    }
    // The loop: the zone the user aims at now, else (left click) the next hurt limb.
    let preferred = selected_zone(world, mob);
    if try_heal_checks(world, actor, patient, preferred, true) {
        if preferred != zone {
            say_applying(world, actor, preferred, &med);
        }
        try_heal(world, actor, patient, preferred, true, job.auto);
        return;
    }
    if job.auto {
        // tg `try_heal_auto_change_zone` over `get_bodyparts()` (a human's: chest, head, arms,
        // legs).
        let next = [
            BodyZone::Chest,
            BodyZone::Head,
            BodyZone::LArm,
            BodyZone::RArm,
            BodyZone::RLeg,
            BodyZone::LLeg,
        ]
        .into_iter()
        .find(|&z| try_heal_checks(world, actor, patient, z, true));
        match next {
            Some(next) => {
                if next != zone {
                    say_applying(world, actor, next, &med);
                }
                try_heal(world, actor, patient, next, true, true);
            }
            None => tell(world, actor.client, Span::Notice, "полностью обработано"),
        }
    }
    // shortcut: right click (tg `try_heal_manual_target`) waits 1 s for another zone to be
    // selected; here the loop ends and a new click starts it again.
}

fn say_applying(world: &mut World, actor: Actor, zone: BodyZone, med: &DmMedical) {
    let text = format!("{} {}...", med.apply_verb, zone.ru().1);
    tell(world, actor.client, Span::Notice, text);
}

/// The wound part of tg `heal_carbon`: `stop_bleeding` off the first bleeding wound (70% on
/// yourself), `flesh_regeneration` and `sanitization` onto the first burn that needs them.
fn treat_wounds(world: &mut World, patient: Entity, zone: BodyZone, med: &DmMedical, own: bool) {
    let Some(mut limbs) = world.get::<wounds::Limbs>(patient).cloned() else {
        return;
    };
    let limb = limbs.limb_mut(zone);
    if med.stop_bleeding > 0.0
        && let Some(i) = limb.wounds.iter().position(|w| w.blood_flow > 0.0)
    {
        let by = -med.stop_bleeding * if own { 0.7 } else { 1.0 };
        let mut told = Vec::new();
        match limb.wounds[i].def().series {
            wounds::Series::Slash => {
                wounds::adjust_slash(limb, i, by, zone, &mut told);
            }
            _ => {
                wounds::adjust_pierce(limb, i, by);
            }
        }
    }
    if (med.flesh_regeneration > 0.0 || med.sanitization > 0.0)
        && let Some(burn) = limb
            .wounds
            .iter_mut()
            .find(|w| w.def().series == wounds::Series::Burn && w.can_be_ointmented_or_meshed())
    {
        burn.flesh_healing += med.flesh_regeneration;
        burn.sanitization += med.sanitization;
    }
    world.entity_mut(patient).insert(limbs);
}

/// tg `heal_damage` on the patient's bodypart, then `updatehealth` (the stat follows).
fn heal(world: &mut World, patient: Entity, zone: BodyZone, brute: f32, burn: f32) {
    let Ok(mut entity) = world.get_entity_mut(patient) else {
        return;
    };
    let Some(mut health) = entity.get_mut::<Health>() else {
        return;
    };
    health.heal_zone(zone, brute, burn);
    let health = health.clone();
    if let Some(mut stat) = entity.get_mut::<Stat>() {
        let mut new = *stat;
        update_stat(&health, &mut new);
        stat.set_if_neq(new);
    }
}

/// `/datum/component/limb_applicable/on_apply` with the wrap's `can_gauze_limb` and
/// `do_gauze_limb`: a limb with wounds, or any limb for `always_applicable` wraps (1.25× the
/// delay without wounds); a wrap at least 20% better than the current one replaces it.
fn begin_wrap(world: &mut World, actor: Actor, patient: Entity) {
    if world.get::<DoAfter>(actor.mob).is_some() {
        return;
    }
    let med = medical(world, actor);
    let zone = selected_zone(world, actor.mob);
    let limb = world
        .get::<wounds::Limbs>(patient)
        .map(|l| l.limb(zone).clone())
        .unwrap_or_default();
    let any_wound = !limb.wounds.is_empty();
    if !med.always_applicable && !any_wound {
        tell(world, actor.client, Span::Warning, "no wounds!");
        return;
    }
    if limb
        .gauze
        .as_ref()
        .is_some_and(|g| g.absorption_capacity * 1.2 > med.absorption_capacity)
    {
        tell(world, actor.client, Span::Warning, "already bandaged!");
        return;
    }
    let (zone_name, _) = zone.ru();
    let item = actor.item.map(|i| name_of(world, i)).unwrap_or_default();
    let (user, target) = (name_of(world, actor.mob), name_of(world, patient));
    let what = if any_wound { "the wounds on " } else { "" };
    let own_delay = if patient == actor.mob {
        visible(
            world,
            actor.mob,
            &format!("You begin to wrap {what}your {zone_name} with {item}..."),
            &format!("{user} begins to wrap {what}{target}'s {zone_name} with {item}..."),
            Span::Notice,
        );
        med.self_delay
    } else {
        visible(
            world,
            actor.mob,
            &format!("You begin to wrap {what}{target}'s {zone_name} with {item}..."),
            &format!("{user} begins to wrap {what}{target}'s {zone_name} with {item}..."),
            Span::Notice,
        );
        med.other_delay
    };
    let job = HealJob {
        zone,
        auto: true,
        wrap: true,
    };
    let delay = if any_wound {
        own_delay
    } else {
        own_delay * 1.25
    };
    do_after(
        world,
        actor,
        patient,
        delay,
        1,
        crate::construction::Job::Heal(job),
    );
}

/// `limb_applicable/on_apply_async` after the `do_after`, then `on_gauze_limb`: the wrap goes
/// on, burns get its one-time dressing bonus, fractures are splinted.
// shortcut: the wrap's unit is used up instead of moving into the limb; nothing takes it off by
// hand (tg `help_remove_gauze`), it falls off when soaked.
fn complete_wrap(world: &mut World, actor: Actor, patient: Entity, zone: BodyZone) {
    let Some(item) = actor.item else {
        return;
    };
    let med = medical(world, actor);
    let Some(mut limbs) = world.get::<wounds::Limbs>(patient).cloned() else {
        return;
    };
    let limb = limbs.limb_mut(zone);
    if limb
        .gauze
        .as_ref()
        .is_some_and(|g| g.absorption_capacity * 1.2 > med.absorption_capacity)
        || !use_stack(world, item, 1)
    {
        return;
    }
    let item_name = world
        .get::<DmName>(item)
        .map_or_else(|| "the wrap".to_owned(), |n| n.name.clone());
    limb.gauze = Some(wounds::Gauze::from_dm(item_name.clone(), &med));
    for burn in limb
        .wounds
        .iter_mut()
        .filter(|w| w.def().series == wounds::Series::Burn)
    {
        let infected = burn.infection > 0.1;
        burn.sanitization += med.sanitization * if infected { 0.2 } else { 1.0 };
        burn.flesh_healing += med.flesh_regeneration * if infected { 0.0 } else { 1.0 };
    }
    wounds::refresh_splints(limb);
    let any_wound = !limb.wounds.is_empty();
    world.entity_mut(patient).insert(limbs);
    let (zone_name, _) = zone.ru();
    let (user, target) = (name_of(world, actor.mob), name_of(world, patient));
    tell(
        world,
        actor.client,
        Span::Notice,
        format!("wrapped {zone_name}"),
    );
    let own = match (patient == actor.mob, any_wound) {
        (true, true) => format!("You bandage the wounds on your {zone_name}."),
        (true, false) => format!("You wrap your {zone_name} with {item_name}."),
        (false, true) => format!("You bandage the wounds on {target}'s {zone_name}."),
        (false, false) => format!("You wrap {target}'s {zone_name} with {item_name}."),
    };
    visible(
        world,
        actor.mob,
        &own,
        &format!("{user} applies {item_name} to {target}'s {zone_name}."),
        Span::Notice,
    );
}

/// tg `healthanalyzer/attack_self`: `scanmode` cycles between health and wounds.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WoundScanMode(pub bool);

/// tg `/obj/item/healthanalyzer/attack_self` (not the wound analyzer). True when `item` is one.
pub fn attack_self(world: &mut World, client: Entity, item: Entity) -> bool {
    let Some(path) = world.get::<DmItem>(item).map(|i| i.path.clone()) else {
        return false;
    };
    if !is_under(&path, ANALYZER_ROOT) || is_under(&path, WOUND_ANALYZER) {
        return false;
    }
    let wounds = !world.get::<WoundScanMode>(item).is_some_and(|m| m.0);
    world.entity_mut(item).insert(WoundScanMode(wounds));
    let text = if wounds {
        "Вы переключаете анализатор здоровья на отображение дополнительной информации о ранах."
    } else {
        "Вы переключаете анализатор здоровья на проверку физического состояния."
    };
    tell(world, client, Span::Notice, text);
    true
}

/// `/obj/item/healthanalyzer/interact_with_atom` (health mode) and the wound analyzer's.
fn analyze(world: &mut World, actor: Actor, patient: Entity, wound_analyzer: bool) {
    let (user, target) = (name_of(world, actor.mob), name_of(world, patient));
    let wound_mode = actor
        .item
        .and_then(|i| world.get::<WoundScanMode>(i))
        .is_some_and(|m| m.0);
    let message = if wound_mode && !wound_analyzer {
        let text = format!("{user} анализирует жизненные показатели {target}.");
        visible(world, actor.mob, &text, &text, Span::Notice);
        crate::health_scan::woundscan(world, actor.item, patient, false)
    } else if wound_analyzer {
        let own = format!("Вы сканируете {target} на серьёзные увечья.");
        let others = format!("{user} сканирует {target} на серьёзные увечья.");
        visible(world, actor.mob, &own, &others, Span::Notice);
        crate::health_scan::woundscan(world, actor.item, patient, true)
    } else {
        let text = format!("{user} анализирует жизненные показатели {target}.");
        visible(world, actor.mob, &text, &text, Span::Notice);
        crate::health_scan::healthscan(world, patient)
    };
    if let Some(message) = message {
        world.write_message(SendChat {
            clients: vec![actor.client],
            message,
        });
    }
}

/// tg `/mob/living/carbon/human/do_cpr` up to its `do_after` (the loop's start checks).
pub fn do_cpr(world: &mut World, client: Entity, rescuer: Entity, patient: Entity) {
    start_cpr(world, client, rescuer, patient, false);
}

fn start_cpr(world: &mut World, client: Entity, rescuer: Entity, patient: Entity, panic: bool) {
    if patient == rescuer || world.get::<DoAfter>(rescuer).is_some() {
        return;
    }
    if world.get::<Stat>(patient) == Some(&Stat::Dead) {
        tell(world, client, Span::Warning, "он мертв!");
        return;
    }
    // shortcut: anything in the mask slot covers the mouth (tg `is_mouth_covered`: masks with
    // `MASKCOVERSMOUTH`, helmets with `HEADCOVERSMOUTH`).
    let masked = |e: Entity| {
        world
            .get::<Inventory>(e)
            .and_then(|i| i.slot(InventorySlot::Mask))
            .is_some()
    };
    if masked(rescuer) {
        tell(world, client, Span::Warning, "снимите свою маску!");
        return;
    }
    if masked(patient) {
        tell(world, client, Span::Warning, "снимите его маску!");
        return;
    }
    let (user, target) = (name_of(world, rescuer), name_of(world, patient));
    visible(
        world,
        rescuer,
        &format!("Вы пытаетесь провести СЛР {target}!... Стойте на месте!"),
        &format!("{user} пытается провести СЛР {target}!"),
        Span::Notice,
    );
    let item = world
        .get::<Inventory>(rescuer)
        .and_then(Inventory::active_item);
    let actor = Actor {
        client,
        mob: rescuer,
        item,
    };
    let delay = if panic { CPR_PANIC_DELAY } else { CPR_DELAY };
    do_after(
        world,
        actor,
        patient,
        delay,
        0,
        crate::construction::Job::Cpr { panicking: panic },
    );
}

/// The rest of tg `do_cpr` after its `do_after`.
pub fn complete_cpr(world: &mut World, rescuer: Entity, action: &DoAfter, panicking: bool) {
    let patient = action.target;
    let client = action.client;
    let Some(health) = world.get::<Health>(patient).cloned() else {
        return;
    };
    if health.health() > HEALTH_THRESHOLD_CRIT || world.get::<Stat>(patient) == Some(&Stat::Dead) {
        return;
    }
    let (user, target) = (name_of(world, rescuer), name_of(world, patient));
    visible(
        world,
        rescuer,
        &format!("Вы проводите СЛР {target}."),
        &format!("{user} проводит СЛР {target}!"),
        Span::Notice,
    );
    let mut health = health;
    health.adjust(DamageType::Oxy, -health.oxy.min(CPR_OXY_HEAL));
    let still_down = health.health() <= HEALTH_THRESHOLD_CRIT;
    let mut stat = *world.get::<Stat>(patient).expect("checked above");
    update_stat(&health, &mut stat);
    let mut entity = world.entity_mut(patient);
    entity.insert(health);
    if let Some(mut old) = entity.get_mut::<Stat>() {
        old.set_if_neq(stat);
    }
    if let Some(patient_client) = world.get::<PlayerMob>(patient).map(|p| p.client) {
        tell(
            world,
            patient_client,
            Span::Notice,
            "Вы чувствуете глоток свежего воздуха, входящий в ваши легкие... Вам становится лучше...",
        );
    }
    if still_down {
        if !panicking {
            tell(
                world,
                client,
                Span::Warning,
                format!("{target} всё ещё лежит! Вы стараетесь усерднее!"),
            );
        }
        start_cpr(world, client, rescuer, patient, true);
    }
}

/// Clicks a medical interaction takes before melee and the inventory: wound treatments
/// (tg `try_treating`, signals first), surgical drapes (`surgery_aid`), surgery on a prepared
/// mob out of combat mode (`/mob/living/item_interaction`), defibrillator paddles, IV drips and
/// unbuckling from a table or bed with an empty hand.
fn medical_clicks(
    world: &mut World,
    mut clicks: Local<MessageCursor<crate::click::ClickResolved>>,
) {
    let clicks: Vec<crate::click::ClickResolved> = clicks
        .read(world.resource::<Messages<crate::click::ClickResolved>>())
        .copied()
        .collect();
    world.resource_mut::<MedicalHandled>().0.clear();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for click in clicks {
        let (mob, target) = (click.mob, click.target);
        if world
            .get::<crate::combat::NextMove>(mob)
            .is_some_and(|n| now < n.0)
        {
            continue;
        }
        let held = world.get::<Inventory>(mob).and_then(Inventory::active_item);
        let actor = Actor {
            client: click.client,
            mob,
            item: held,
        };
        let handled = claim_click(world, actor, target, click.modifiers);
        if handled {
            world.resource_mut::<MedicalHandled>().0.insert(mob);
        }
    }
}

fn claim_click(world: &mut World, actor: Actor, target: Entity, modifiers: ClickModifiers) -> bool {
    let held_path = actor
        .item
        .and_then(|i| world.get::<DmItem>(i))
        .map(|i| i.path.clone())
        .unwrap_or_default();
    if world.get::<Health>(target).is_some() {
        if !modifiers.right && wounds::try_treating(world, actor, target) {
            return true;
        }
        if is_under(&held_path, surgery::DRAPES) {
            surgery::drapes(world, actor, target);
            return true;
        }
        let prepped = world
            .get::<wounds::Limbs>(target)
            .is_some_and(|l| l.parts.iter().any(|p| p.prepped));
        if prepped
            && !surgery::combat_mode(world, actor.mob)
            && surgery::perform_surgery(world, actor, target)
        {
            return true;
        }
        if let Some(item) = actor.item
            && world.get::<defib::Paddles>(item).is_some()
        {
            return defib::paddles_attack(world, actor, item, target, modifiers.right);
        }
        return false;
    }
    if operating::interact(world, actor, target) {
        return true;
    }
    if world.get::<blood::IvDrip>(target).is_some() {
        return match actor.item {
            Some(item) => blood::iv_item_interaction(world, actor, item, target),
            None if modifiers.right => blood::iv_quick_toggle(world, actor, target),
            None => false,
        };
    }
    let furniture = crate::construction::path_of(world, target).unwrap_or_default();
    if actor.item.is_none()
        && !modifiers.right
        && surgery::buckles_lying(&furniture)
        && let Some(buckled) = surgery::buckled_to(world, target)
    {
        surgery::unbuckle(world, actor, buckled);
        return true;
    }
    false
}

/// tg `Life()` of the medical parts each `SSmobs` fire: wounds (also of the dead), organ decay,
/// `handle_blood` of the living, the drapes of a lying mob settling.
// Cost: every 2 s one pass over humans (one per player); wound loops run over at most a few
// wounds per limb.
fn medical_life(world: &mut World, mut last_fired: Local<u64>) {
    let fired = world.resource::<crate::breath::MobsClock>().times_fired;
    let fires = fired.saturating_sub(*last_fired);
    *last_fired = fired;
    if fires == 0 {
        return;
    }
    let secs = crate::breath::SSMOBS_WAIT_SECS as f32;
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mobs: Vec<Entity> = world
        .query_filtered::<Entity, (With<wounds::Limbs>, With<Health>)>()
        .iter(world)
        .collect();
    for _ in 0..fires {
        for &mob in &mobs {
            life_tick(world, mob, secs, now);
        }
    }
}

fn life_tick(world: &mut World, mob: Entity, secs: f32, now: f64) {
    let Ok(entity) = world.get_entity(mob) else {
        return;
    };
    let dead = entity.get::<Stat>() == Some(&Stat::Dead);
    let lying = entity
        .get::<crate::health::BodyPosition>()
        .is_some_and(|b| b.lying());
    let bodytemperature = entity
        .get::<crate::breath::Respiration>()
        .map_or(crate::breath::BODYTEMP_NORMAL, |r| r.bodytemperature);
    let nutrition = entity
        .get::<crate::hunger::Hunger>()
        .map_or(0.0, |h| h.nutrition);
    let health = entity.get::<Health>().cloned().unwrap_or_default();
    let has_blood = entity.get::<blood::Blood>().is_some_and(|b| b.volume > 0.0);
    let Some(mut limbs) = entity.get::<wounds::Limbs>().cloned() else {
        return;
    };
    // Wounds.
    let mut rng = fork_rng(world);
    let mut effects = Vec::new();
    for zone in BodyZone::ALL {
        let ctx = wounds::TickCtx {
            secs,
            lying,
            bodytemperature,
            nutrition,
            burn_damage: health.part(zone).burn,
            has_blood,
        };
        for effect in wounds::process(&mut rng, limbs.limb_mut(zone), zone, ctx) {
            effects.push((zone, effect));
        }
    }
    // `surgery_prepped/tick`: lying settles the drapes.
    if lying && limbs.prep_moves > 0 {
        limbs.prep_moves -= 1;
    }
    if world.get::<wounds::Limbs>(mob) != Some(&limbs) {
        world.entity_mut(mob).insert(limbs);
    }
    for (zone, effect) in effects {
        match effect {
            wounds::TickEffect::Tox(amount) => hurt_limb(world, mob, zone, DamageType::Tox, amount),
            wounds::TickEffect::Pain { brute, stamina } => {
                hurt_limb(world, mob, zone, DamageType::Brute, brute);
                crate::status_effects::adjust_stamina(world, mob, stamina);
            }
            wounds::TickEffect::Tell(span, text) => {
                if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
                    tell(world, client, span, text);
                }
            }
        }
    }
    // Organs.
    if let Some(mut organs) = world.get::<defib::OrganDecay>(mob).copied() {
        organs.tick(dead, bodytemperature, secs);
        if world.get::<defib::OrganDecay>(mob) != Some(&organs) {
            world.entity_mut(mob).insert(organs);
        }
    }
    if !dead {
        blood::handle_blood(world, mob, secs, now);
    }
}

/// `SSmachines` fires (2 s) for the IV drips.
fn machines_life(world: &mut World, mut elapsed: Local<f64>) {
    *elapsed += world.resource::<Time>().delta_secs_f64();
    while *elapsed >= blood::MACHINES_WAIT_SECS {
        *elapsed -= blood::MACHINES_WAIT_SECS;
        blood::process_drips(world, blood::MACHINES_WAIT_SECS as f32);
    }
}

/// [`LimbLimits`] follow the wounds, the limb damage and buckling.
fn refresh_limits(
    changed: Query<
        Entity,
        Or<(
            Changed<wounds::Limbs>,
            Changed<Health>,
            Added<surgery::Buckled>,
        )>,
    >,
    mut unbuckled: RemovedComponents<surgery::Buckled>,
    mut mobs: Query<(
        &wounds::Limbs,
        &Health,
        Has<surgery::Buckled>,
        &mut LimbLimits,
    )>,
) {
    let gone: Vec<Entity> = unbuckled.read().collect();
    for mob in changed.iter().chain(gone) {
        let Ok((limbs, health, buckled, mut out)) = mobs.get_mut(mob) else {
            continue;
        };
        let limits = limbs.limits(health);
        out.set_if_neq(LimbLimits {
            slowdown: limits.slowdown,
            floored: limits.floored || buckled,
            immobilized: buckled,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heal_all_restores_blood_and_closes_wounds() {
        let mut world = World::new();
        let mut blood = blood::Blood::new(blood::BloodType::random_human(&mut CombatRng(1)));
        blood.volume = 50.0;
        let mut limbs = wounds::Limbs::default();
        wounds::apply_wound(
            limbs.limb_mut(BodyZone::Chest),
            wounds::WoundKind::SlashCritical,
        );
        let mob = world.spawn((blood, limbs)).id();
        heal_all(&mut world, mob);
        assert_eq!(
            world.get::<blood::Blood>(mob).unwrap().volume,
            blood::BLOOD_VOLUME_NORMAL
        );
        let limbs = world.get::<wounds::Limbs>(mob).unwrap();
        assert!(
            limbs
                .parts
                .iter()
                .all(|l| l.wounds.is_empty() && l.surgery == 0)
        );
    }

    #[test]
    fn random_zones_follow_ran_zone_weights() {
        let mut rng = CombatRng(3);
        let mut counts = [0u32; 6];
        for _ in 0..18_000 {
            counts[random_valid_zone(&mut rng, BodyZone::Head, 0.0).index()] += 1;
        }
        // Head and chest 1/18 each, every limb 4/18.
        assert!(
            (700..1300).contains(&counts[BodyZone::Head.index()]),
            "{counts:?}"
        );
        assert!(
            (700..1300).contains(&counts[BodyZone::Chest.index()]),
            "{counts:?}"
        );
        for limb in [
            BodyZone::LArm,
            BodyZone::RArm,
            BodyZone::LLeg,
            BodyZone::RLeg,
        ] {
            assert!((3500..4500).contains(&counts[limb.index()]), "{counts:?}");
        }
        let hits = (0..10_000)
            .filter(|_| random_valid_zone(&mut rng, BodyZone::LLeg, 80.0) == BodyZone::LLeg)
            .count();
        // 80% + 20% * 4/18.
        assert!((8100..8800).contains(&hits), "{hits}");
        assert!(interacts_with_mobs("/obj/item/stack/medical/bruise_pack"));
        assert!(interacts_with_mobs("/obj/item/healthanalyzer/simple"));
        assert!(!interacts_with_mobs("/obj/item/stack/sheet/iron"));
    }
}
