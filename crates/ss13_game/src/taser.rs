//! Tasers: tg `/obj/projectile/energy/electrode` and `/datum/status_effect/tased`
//! (`code/modules/projectiles/projectile/energy/stun.dm`).
//!
//! An electrode that hits a living mob (not fully blocked by armor) hooks it to the gun it came
//! from: while the firer holds that gun, the victim stays within the electrodes' reach (`range +
//! 1` tiles, in sight within 5) and the cell pays `STANDARD_CELL_CHARGE * 0.05` per second, the
//! victim takes the electrode's `tase_stamina` stamina damage per second. The victim is slowed
//! (`being_tased`, 4), the firer too (`tasing_someone`, 2), and the gun cannot fire meanwhile.
//! It ends when the firer clicks the victim, drops or stows the gun, the cell runs dry, the
//! victim gets out of reach or sight, or someone pulls the electrodes out (resist: a 5 second
//! `do_after` on yourself).
//!
//! A hit sparks at the victim and at the gun; an electrode that flies its full range sparks
//! where it falls. Silicons and bots shrug electrodes off. The first set upsets the victim
//! (`/datum/mood_event/tased`); a wet victim (wet stacks) burns `max(1, wet * 0.5)` per tick and
//! sparks, a clumsy one lying down may bang their head (10 brute) while tased.
//!
//! Messages are written in Russian here (the checkout has them in English).
// shortcut: no electrode beams (no beam visuals in the port), no alert, no hulk shouts; drones
// and `TRAIT_PIERCEIMMUNE` are not exempt; a helper cannot pull the electrodes out of someone
// else; add them with those systems.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::DmGun;
use ss13_net::server::IntentReceived;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::protocol::ClientIntent;

use crate::click::{ClickResolved, RangedClick};
use crate::health::{COMBAT_MESSAGE_RANGE, Health, Stat};
use crate::projectile::Projectile;
use crate::security::{name_of, show};

/// `STANDARD_CELL_CHARGE * 0.05`, joules per second.
pub const ENERGY_DRAIN: f64 = 10_000.0 * 0.05;
/// `/datum/movespeed_modifier/being_tased`, `tasing_someone`: deciseconds per tile.
pub const BEING_TASED_SLOWDOWN: f32 = 4.0;
pub const TASING_SLOWDOWN: f32 = 2.0;
/// `can_see(taser, owner, 5)`.
const SIGHT: u32 = 5;
/// `tick_interval`, seconds.
const TICK_SECS: f32 = 0.25;
const HEAD_SMASH: &str = "sound/effects/tableheadsmash.ogg";
/// `/datum/mood_event/tased`.
const TASED_MOOD: crate::mood::MoodEvent = crate::mood::MoodEvent {
    id: "tased",
    description: "В слове «шокер» нет буквы «з». Она - в звуке «ззз».",
    mood_change: -3.0,
    timeout: 120.0,
    hidden: false,
};
/// The removal `do_after`, deciseconds.
pub const REMOVAL_DS: f32 = 50.0;

/// One set of electrodes in a mob (`STATUS_EFFECT_MULTIPLE`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tase {
    pub gun: Entity,
    pub firer: Entity,
    pub stamina_per_second: f32,
    /// `tase_range`: `maximum_range + 1`.
    pub range: u32,
}

/// The electrodes in this mob.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Tased(pub Vec<Tase>);

/// The tasing movespeed modifiers of a mob, deciseconds per tile; kept at 0 instead of
/// removed, so the speed follows its changes.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct TaseSlowdown(pub f32);

/// Runs with the guns, after their clicks.
pub(crate) fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (cancel_by_click, resist_intents, tick)
            .chain()
            .after(crate::click::ResolveClicks)
            .after(crate::guns::GunSystems)
            .before(crate::health::HealthSystems),
    );
}

/// Whether `gun` is tasing someone (`block_firing`).
pub fn tasing_with(world: &mut World, gun: Entity) -> bool {
    let mut query = world.query::<&Tased>();
    query.iter(world).any(|t| t.0.iter().any(|t| t.gun == gun))
}

/// `/obj/projectile/energy/electrode/on_hit` -> `apply_status_effect(/datum/status_effect/tased)`.
pub fn tase(world: &mut World, p: &Projectile, target: Entity) {
    let Some(&at) = world.get::<TilePos>(target) else {
        return;
    };
    let victim = name_of(world, target);
    // `can_tase_with`: an energy gun with a cell.
    let Some(gun) = p
        .fired_from
        .filter(|&g| world.get::<DmGun>(g).is_some_and(|d| d.energy))
    else {
        show(
            world,
            at,
            COMBAT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Warning,
                format!("Электроды безвредно ударяются о {victim}."),
            ),
            &[(
                target,
                Span::Notice,
                "Электроды безвредно ударяются о вас.".to_owned(),
            )],
        );
        return;
    };
    crate::effects::particles::do_sparks(world, 1, true, at);
    if let Some(gun_at) = crate::modpc::pda::turf_of(world, gun) {
        crate::effects::particles::do_sparks(world, 1, true, gun_at);
    }
    // `tased/on_apply`: silicons and bots.
    if world.get::<crate::silicon::Silicon>(target).is_some()
        || world.get::<crate::bots::Bot>(target).is_some()
    {
        show(
            world,
            at,
            COMBAT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Warning,
                format!("Электроды не цепляются за {victim}!"),
            ),
            &[],
        );
        return;
    }
    let tase = Tase {
        gun,
        firer: p.firer,
        stamina_per_second: p.data.tase_stamina,
        range: p.data.range as u32 + 1,
    };
    let first = world.get::<Tased>(target).is_none_or(|t| t.0.is_empty());
    let mut tased = world.get::<Tased>(target).cloned().unwrap_or_default();
    tased.0.push(tase);
    world.entity_mut(target).insert(tased);
    if first {
        crate::mood::add_event(world, target, "tased", &TASED_MOOD);
        crate::mobs::defense::emote(world, target, "кричит!");
    }
    update_slowdown(world, target);
    update_slowdown(world, p.firer);
}

/// `being_tased` and `tasing_someone` of `mob`.
fn update_slowdown(world: &mut World, mob: Entity) {
    let tased = world.get::<Tased>(mob).is_some_and(|t| !t.0.is_empty());
    let tasing = world
        .query::<&Tased>()
        .iter(world)
        .any(|t| t.0.iter().any(|t| t.firer == mob));
    let slowdown =
        if tased { BEING_TASED_SLOWDOWN } else { 0.0 } + if tasing { TASING_SLOWDOWN } else { 0.0 };
    if let Ok(mut entity) = world.get_entity_mut(mob) {
        match entity.get_mut::<TaseSlowdown>() {
            Some(mut s) => {
                s.set_if_neq(TaseSlowdown(slowdown));
            }
            None if slowdown != 0.0 => {
                entity.insert(TaseSlowdown(slowdown));
            }
            None => {}
        }
    }
}

/// `end_tase` of the electrodes `index` in `owner`.
fn end_tase(world: &mut World, owner: Entity, index: usize) {
    let Some(mut tased) = world.get::<Tased>(owner).cloned() else {
        return;
    };
    if index >= tased.0.len() {
        return;
    }
    let tase = tased.0.remove(index);
    let last = tased.0.is_empty();
    world.entity_mut(owner).insert(tased);
    // `tased/on_remove`: the last electrodes leave the victim jittering.
    if last {
        crate::status_effects::adjust_timed(
            world,
            owner,
            crate::status_effects::Timed::Jitter,
            10.0,
            Some(60.0),
        );
    }
    if let Some(&at) = world.get::<TilePos>(owner) {
        let victim = name_of(world, owner);
        show(
            world,
            at,
            COMBAT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Warning,
                format!("Электроды перестают бить током {victim}."),
            ),
            &[(
                owner,
                Span::Notice,
                "Электроды перестают бить вас током.".to_owned(),
            )],
        );
    }
    update_slowdown(world, owner);
    update_slowdown(world, tase.firer);
}

/// `do_tase_with`: the firer still holds the gun, the victim is in sight and reach, the cell
/// pays for `secs`.
fn holds(world: &mut World, owner: Entity, tase: &Tase, secs: f32) -> bool {
    let held = world
        .get::<crate::inventory::Inventory>(tase.firer)
        .is_some_and(|inv| inv.holds(tase.gun));
    let (Some(&from), Some(&to)) = (
        world.get::<TilePos>(tase.firer),
        world.get::<TilePos>(owner),
    ) else {
        return false;
    };
    if !held
        || world
            .get::<Stat>(tase.firer)
            .is_none_or(|s| *s == Stat::Dead)
        || from.chebyshev(to).is_none_or(|d| d > tase.range)
        || !crate::mobs::ai::behaviors::can_see(world, from, to, SIGHT)
    {
        return false;
    }
    let Some(mut gun) = world.get_mut::<crate::guns::Gun>(tase.gun) else {
        return false;
    };
    let cost = ENERGY_DRAIN * f64::from(secs);
    if gun.charge < cost {
        return false;
    }
    gun.charge -= cost;
    true
}

/// `/datum/status_effect/tased/tick`, every frame for its share of a second.
fn tick(world: &mut World) {
    let secs = world.resource::<Time>().delta_secs();
    let mut query = world.query::<(Entity, &Tased)>();
    let owners: Vec<(Entity, Vec<Tase>)> = query
        .iter(world)
        .filter(|(_, t)| !t.0.is_empty())
        .map(|(e, t)| (e, t.0.clone()))
        .collect();
    for (owner, tases) in owners {
        for tase in tases.iter().rev() {
            let index = world
                .get::<Tased>(owner)
                .and_then(|t| t.0.iter().position(|x| x == tase));
            let Some(index) = index else {
                continue;
            };
            if !holds(world, owner, tase, secs) {
                end_tase(world, owner, index);
                continue;
            }
            // `check_stun_immunity(CANSTUN|CANKNOCKDOWN)`.
            if world.get::<Health>(owner).is_some() {
                crate::status_effects::adjust_stamina(world, owner, tase.stamina_per_second * secs);
            }
            // `adjust_stutter_up_to(10 SECONDS, 20 SECONDS)`, `adjust_jitter_up_to(20 SECONDS,
            // 30 SECONDS)` every 0.25 s tick, per frame by its share.
            use crate::status_effects::{Timed, adjust_timed};
            adjust_timed(world, owner, Timed::Stutter, 40.0 * secs, Some(20.0));
            adjust_timed(world, owner, Timed::Jitter, 80.0 * secs, Some(30.0));
            hazards(world, owner, secs);
        }
    }
}

/// `tased/tick`'s wet burns and clumsy head bumps, for `secs` of the 0.25 s ticks.
fn hazards(world: &mut World, owner: Entity, secs: f32) {
    use crate::health::DamageType;
    use crate::medical::wounds::spt_prob;
    // "You are damp, that's bad when you're being tased": `max(1, wet * 0.5 * tick)` a tick.
    let wet = crate::fire::wet_stacks(world, owner);
    if wet > 0.0 {
        let per_tick = (wet * 0.5 * TICK_SECS).max(1.0);
        crate::mobs::defense::hurt(
            world,
            owner,
            DamageType::Burn,
            per_tick * secs / TICK_SECS,
            None,
        );
        let spark = spt_prob(
            &mut world.resource_mut::<crate::combat::CombatRng>(),
            25.0,
            secs,
        );
        if spark && let Some(&at) = world.get::<TilePos>(owner) {
            crate::effects::particles::do_sparks(world, 1, false, at);
        }
    }
    // "clumsy people might hit their head while being tased".
    let lying = world
        .get::<crate::health::BodyPosition>(owner)
        .is_some_and(|b| b.lying());
    if lying
        && crate::genetics::is_clumsy(world, owner)
        && spt_prob(
            &mut world.resource_mut::<crate::combat::CombatRng>(),
            20.0,
            secs,
        )
    {
        crate::mobs::defense::hurt(
            world,
            owner,
            DamageType::Brute,
            10.0,
            Some(crate::zone::BodyZone::Head),
        );
        if let Some(&at) = world.get::<TilePos>(owner) {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(at, HEAD_SMASH, 75.0, true),
            );
        }
    }
}

/// `user_cancel_tase`: the firer clicking the victim (not shift) ends it.
fn cancel_by_click(
    world: &mut World,
    mut resolved: Local<MessageCursor<ClickResolved>>,
    mut ranged: Local<MessageCursor<RangedClick>>,
) {
    let mut clicks: Vec<(Entity, Entity)> = resolved
        .read(world.resource::<Messages<ClickResolved>>())
        .map(|c| (c.mob, c.target))
        .collect();
    clicks.extend(
        ranged
            .read(world.resource::<Messages<RangedClick>>())
            .map(|c| (c.mob, c.target)),
    );
    for (mob, target) in clicks {
        let index = world
            .get::<Tased>(target)
            .and_then(|t| t.0.iter().position(|t| t.firer == mob));
        if let Some(index) = index {
            end_tase(world, target, index);
            let now = world.resource::<Time>().elapsed_secs_f64();
            world.entity_mut(mob).insert(crate::combat::NextMove(
                now + crate::pull::CLICK_CD_GRABBING,
            ));
        }
    }
}

/// `try_remove_taser` on `COMSIG_LIVING_RESIST`: a 5 second `do_after` on yourself.
fn resist_intents(world: &mut World, mut intents: Local<MessageCursor<IntentReceived>>) {
    let clients: Vec<Entity> = intents
        .read(world.resource::<Messages<IntentReceived>>())
        .filter(|i| matches!(i.intent, ClientIntent::Resist))
        .map(|i| i.client)
        .collect();
    for client in clients {
        let Some(mob) = world.resource::<crate::server::Players>().mob(client) else {
            continue;
        };
        if world.get::<Tased>(mob).is_none_or(|t| t.0.is_empty())
            || world.get::<crate::tools::DoAfter>(mob).is_some()
        {
            continue;
        }
        let Some(&at) = world.get::<TilePos>(mob) else {
            continue;
        };
        let name = name_of(world, mob);
        show(
            world,
            at,
            COMBAT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Warning,
                format!("{name} пытается вытащить электроды!"),
            ),
            &[(
                mob,
                Span::Notice,
                "Вы пытаетесь вытащить электроды!".to_owned(),
            )],
        );
        let actor = crate::tools::Actor {
            client,
            mob,
            item: None,
        };
        crate::tools::do_after(
            world,
            actor,
            mob,
            REMOVAL_DS,
            0,
            crate::construction::Job::RemoveElectrodes,
        );
    }
}

/// The removal `do_after` finished: the last set of electrodes comes out.
pub fn complete_removal(world: &mut World, mob: Entity) {
    if world.get::<Tased>(mob).is_none_or(|t| t.0.is_empty()) {
        return;
    }
    if let Some(&at) = world.get::<TilePos>(mob) {
        let name = name_of(world, mob);
        show(
            world,
            at,
            COMBAT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Warning,
                format!("{name} вытаскивает электроды из своего тела!"),
            ),
            &[(mob, Span::Notice, "Вы вытаскиваете электроды!".to_owned())],
        );
    }
    // One `do_after` per set of electrodes (each effect shares the interaction key).
    if let Some(last) = world.get::<Tased>(mob).map(|t| t.0.len() - 1) {
        end_tase(world, mob, last);
    }
}
