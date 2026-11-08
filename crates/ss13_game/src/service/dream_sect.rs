//! The dream god (server), after tg `/datum/religion_sect/dreams` and
//! `code/modules/religion/dreams` (`dream_portent.dm`, `dream_protection.dm`,
//! `slumber_party.dm`, `deaconize_dreamer.dm`).
//!
//! - Dreams ([`crate::dreams`]): a holy dreamer earns 10 favor a dream ("approves of your
//!   slumber"); 10% of their dreams, 30 s apart, may be a vague portent about antagonists (15
//!   favor). The dream pool hooks are [`dream_pool`] and [`dream_started`].
//! - The blessing works on sleepers only: a dreamer is healed (the standard 10/10) and, once in
//!   6 minutes, gives 20 favor (halved without a mind, halved when only dreaming); an
//!   unconscious body starts a dream and is healed (and protected for its sleep with dream
//!   protection); the waking are ignored. A corpse gives 10 favor (5 without a mind) once in 6
//!   minutes and, with dream protection, rests protected until revived.
//! - "Deaconize" takes up to three deacons. "Dream Portent" puts the priest to sleep (10 s) and
//!   forces a dream 2-5 s later with the portent likely in the pool; another dream refunds 80%.
//!   "Dream Protection" (once) protects every holy one's sleep ([`DreamProtection`]: tg
//!   `TRAIT_HOLY` while asleep or dreaming). "Slumber Party" puts the humans within 5 tiles of
//!   the altar to sleep for 20 s with one shared dream, healing 2 (4 for the holy) brute, burn,
//!   toxin and suffocation a second.
// shortcut: dream protection does not reduce damage (no damage modifier hook yet) and has no
// glow; damage does not wake a slumber party or a portent dreamer; the portent foresees only
// hazy answers and the dead (no antagonist, nuke, AI, shuttle or threat-level portents); no
// cyan spans. tg's slumber party *adds* toxin and oxygen damage while healing the rest (an
// obvious sign slip): here it heals them.

use bevy::prelude::*;
use ss13_shared::chat::Span;

use super::religion::{
    Blessing, HolyRole, Rite, Sect, glob_deity, heal_hurt_parts, holy_role, is_human, religion,
    religion_mut, tell_mob,
};
use super::server::{say_around, sound_at};
use crate::chem::name_of;
use crate::dreams::{DreamKind, DreamSource, Dreaming, ExtraDream, Fragment};
use crate::health::{DamageType, Health, Stat};
use crate::status_effects::{Incap, Timed};

const PUNCH: &str = "sound/effects/hit_punch.ogg";
/// `max_deacons`.
const MAX_DEACONS: u32 = 3;
/// `recent_bless_refs` timer: 6 minutes.
const BLESS_COOLDOWN_SECS: f64 = 360.0;
/// `vague_portent_cooldown`.
const VAGUE_PORTENT_COOLDOWN_SECS: f64 = 30.0;
/// `vague_portent_chance`, percent.
const VAGUE_PORTENT_CHANCE: f32 = 10.0;
/// `/datum/status_effect/dream_protection/temporary` and `/deceased` `duration`.
const PROTECTION_SECS: f64 = 180.0;
/// `/datum/status_effect/slumber_party` `duration`, `healing`.
const SLUMBER_SECS: f64 = 20.0;
const SLUMBER_HEALING: f32 = 2.0;
const SLUMBER_RANGE: u32 = 5;
/// The antagonist flags a vague portent dreams of (`jobban_flag || pref_flag` of the rulesets).
const ANTAG_NOUNS: [&str; 12] = [
    "traitor",
    "changeling",
    "heretic",
    "cultist",
    "wizard",
    "nuclear operative",
    "revolutionary",
    "blob",
    "space ninja",
    "abductor",
    "xenomorph",
    "nightmare",
];

const DREAM_FAILED: crate::mood::MoodEvent = crate::mood::MoodEvent {
    id: "dream_failed",
    description: "I couldn't receive a clear vision from my dreams!",
    mood_change: -2.0,
    timeout: 300.0,
    hidden: false,
};

/// The dream god's state (tg `/datum/religion_sect/dreams` vars).
#[derive(Clone, Debug, Default)]
pub struct DreamState {
    /// `dream_protection`: the rite was performed.
    pub protection: bool,
    pub deacon_count: u32,
    /// `recent_bless_refs`, with when each may give favor again.
    pub recent_blessings: Vec<(Entity, f64)>,
    pub vague_portent_ready_at: f64,
}

/// `/datum/status_effect/dream_protection` (permanent for the holy) and its `/temporary` and
/// `/deceased` versions, which end at `until` or when the sleep (death) ends.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct DreamProtection {
    pub kind: ProtectionKind,
    pub until: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProtectionKind {
    Permanent,
    Temporary,
    Deceased,
}

/// `/datum/status_effect/slumber_party`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct SlumberParty {
    pub until: f64,
    /// Per second.
    pub healing: f32,
    pub shared: Vec<Fragment>,
    /// `force_dream`, 4-8 s in.
    pub dream_at: Option<f64>,
}

/// The dream portent rite waits for its dream (`add_portent`, `check_portent`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct PortentPending {
    pub favor_paid: f32,
    /// `force_dream`, 2-5 s after the rite.
    pub dream_at: Option<f64>,
}

pub(super) fn build(app: &mut App) {
    app.add_systems(
        Update,
        (
            slumber_parties.run_if(any_with_component::<SlumberParty>),
            portents.run_if(any_with_component::<PortentPending>),
            end_protections.run_if(any_with_component::<DreamProtection>),
        ),
    );
}

fn now(world: &World) -> f64 {
    world
        .get_resource::<Time>()
        .map_or(0.0, |t| t.elapsed_secs_f64())
}

fn dream_sect(world: &World) -> bool {
    religion(world).sect == Some(Sect::Dreams)
}

/// `IS_UNCONSCIOUS`: asleep, unconscious or in hard crit (or dead).
fn unconscious(world: &World, mob: Entity) -> bool {
    crate::status_effects::knocked_out(world, mob)
        || world.get::<Stat>(mob).is_some_and(|&s| s >= Stat::HardCrit)
}

/// `IS_UNCONSCIOUS_AND_ALIVE`.
fn unconscious_and_alive(world: &World, mob: Entity) -> bool {
    unconscious(world, mob) && world.get::<Stat>(mob) != Some(&Stat::Dead)
}

fn has_mind(world: &World, mob: Entity) -> bool {
    world.get::<crate::mind::MindData>(mob).is_some()
}

/// `/datum/religion_sect/dreams/on_conversion`: the protection, when the rite was done.
pub(super) fn on_conversion(world: &mut World, mob: Entity) {
    if religion(world).dreams.protection {
        protect(world, mob, ProtectionKind::Permanent);
    }
}

fn protect(world: &mut World, mob: Entity, kind: ProtectionKind) {
    let until = (kind != ProtectionKind::Permanent).then(|| now(world) + PROTECTION_SECS);
    world
        .entity_mut(mob)
        .insert(DreamProtection { kind, until });
}

/// tg `TRAIT_HOLY` of dream protection: while asleep or dreaming.
pub(super) fn protected_now(world: &World, mob: Entity) -> bool {
    world.get::<DreamProtection>(mob).is_some()
        && (unconscious_and_alive(world, mob)
            || world.get::<Dreaming>(mob).is_some()
            || world
                .get::<DreamProtection>(mob)
                .is_some_and(|p| p.kind == ProtectionKind::Deceased))
}

/// `COMSIG_PRE_DREAMING` of the portent rite, the slumber party and the dream god.
pub fn dream_pool(world: &mut World, mob: Entity) -> Vec<ExtraDream> {
    let mut pool = Vec::new();
    if world.get::<PortentPending>(mob).is_some() {
        // The rest of the portent is foreseen when it is dreamt (`dream_started`).
        pool.push(ExtraDream {
            kind: DreamKind::SpecificPortent,
            weight: 2000,
            source: DreamSource::Fragments(vec![Fragment::Text("a portent of the future".into())]),
        });
        return pool;
    }
    if let Some(party) = world.get::<SlumberParty>(mob)
        && !party.shared.is_empty()
    {
        pool.push(ExtraDream {
            kind: DreamKind::Shared,
            weight: 2000,
            source: DreamSource::Fragments(party.shared.clone()),
        });
    }
    let ready = religion(world).dreams.vague_portent_ready_at <= now(world);
    if dream_sect(world)
        && holy_role(world, mob).is_some()
        && ready
        && world
            .resource_mut::<crate::combat::CombatRng>()
            .chance(VAGUE_PORTENT_CHANCE)
    {
        // `/datum/dream/random::weight * 0.1`.
        pool.push(ExtraDream {
            kind: DreamKind::VaguePortent,
            weight: 100,
            source: DreamSource::RandomOf(ANTAG_NOUNS.to_vec()),
        });
    }
    pool
}

/// `COMSIG_START_DREAMING`: the portent's vision (or its failure) and the dream god's favor.
pub fn dream_started(world: &mut World, mob: Entity, kind: DreamKind) {
    if let Some(pending) = world.get::<PortentPending>(mob).copied() {
        world.entity_mut(mob).remove::<PortentPending>();
        if kind == DreamKind::SpecificPortent {
            let rest = portent(world, mob);
            if let Some(mut dreaming) = world.get_mut::<Dreaming>(mob) {
                dreaming
                    .fragments
                    .extend(rest.into_iter().map(Fragment::Text));
            }
        } else {
            tell_mob(
                world,
                mob,
                Span::Notice,
                "Your mind wanders, yet you receive no clear vision... You must try again later.",
            );
            religion_mut(world).adjust_favor(pending.favor_paid * 0.8);
            crate::status_effects::adjust_timed(world, mob, Timed::Drowsiness, 10.0, None);
            crate::mood::add_event(world, mob, "dream_failed", &DREAM_FAILED);
        }
    }
    // No reward for the dream the god is giving you.
    if !dream_sect(world) || holy_role(world, mob).is_none() || kind == DreamKind::SpecificPortent {
        return;
    }
    let mut favor = 10.0;
    if kind == DreamKind::VaguePortent {
        let ready = now(world) + VAGUE_PORTENT_COOLDOWN_SECS;
        religion_mut(world).dreams.vague_portent_ready_at = ready;
        favor *= 1.5;
    }
    let deity = glob_deity(world);
    tell_mob(
        world,
        mob,
        Span::Notice,
        format!("{deity} approves of your slumber."),
    );
    religion_mut(world).adjust_favor(favor);
}

/// `/datum/dream/specific_portent/GenerateDream` past its first line.
fn portent(world: &mut World, dreamer: Entity) -> Vec<String> {
    let deity = glob_deity(world);
    let rng = |world: &mut World, n: u32| {
        world
            .resource_mut::<crate::combat::CombatRng>()
            .range(0, n - 1) as usize
    };
    let (open, close) = {
        let figure = ["man", "woman", "prophet", "oracle"][rng(world, 4)];
        let feeling = ["wonder", "dread", "curiosity", "foreboding"][rng(world, 4)];
        let types: [(String, String); 12] = [
            (
                format!("{deity} greets you warmly"),
                format!("{deity} bids you farewell, though you feel their presence watch over you"),
            ),
            (
                "a crystal ball reveals a vision of the future".into(),
                "ultimately, the crystal ball returns to its normal, opaque state".into(),
            ),
            (
                "a divine light blinds you, revealing glimpses of what is to come".into(),
                "finally, the light fades, leaving you with a lingering warmth".into(),
            ),
            (
                "a full moon illuminates the sky".into(),
                "the moon crosses the horizon, bringing forth a new dawn".into(),
            ),
            (
                "a mysterious figure appears, cloaked in shadow".into(),
                format!("they depart, leaving you with a sense of {feeling}"),
            ),
            (
                "an incomprehensible entity envelops you, showing you visions of the past, \
                 present, and future"
                    .into(),
                "the entity releases you, leaving you with a sense of awe and fear".into(),
            ),
            (
                format!("an old {figure} approaches you, offering cryptic advice"),
                "they vanish before you can ask any questions".into(),
            ),
            (
                "the stars align in a way you've never seen before".into(),
                "finally, the stars return to their normal constellations".into(),
            ),
            (
                "the trees ahead parts to reveal a hidden path".into(),
                "ultimately, you lose the path as the trees sway back into place".into(),
            ),
            (
                "walking through a featureless landscape, shapes begin to form".into(),
                "finally, the shapes fade away, leaving you alone in the void".into(),
            ),
            (
                "you see yourself sleeping peacefully".into(),
                "finally, you see yourself waking up calmly".into(),
            ),
            (
                "your third eye opens to reveal a hidden truth".into(),
                "finally, your third eye closes, but the vision lingers in your mind".into(),
            ),
        ];
        types[rng(world, 12)].clone()
    };
    let mut out = vec![open];
    out.extend(foresee(world, dreamer).iter().map(|s| (*s).to_owned()));
    out.push(close);
    out
}

/// `get_portent`: what the vision is about.
fn foresee(world: &mut World, _dreamer: Entity) -> Vec<&'static str> {
    const HAZY: [&[&str]; 5] = [
        &["reply hazy", "try again later"],
        &["ask again later"],
        &["better not tell you now"],
        &["cannot predict now"],
        &["concentrate and ask again"],
    ];
    let pick = |world: &mut World, n: usize| {
        world
            .resource_mut::<crate::combat::CombatRng>()
            .range(0, n as u32 - 1) as usize
    };
    if world.resource_mut::<crate::combat::CombatRng>().chance(1.0) {
        religion_mut(world).adjust_favor(25.0);
        return HAZY[pick(world, HAZY.len())].to_vec();
    }
    let (mut players, mut dead) = (0u32, 0u32);
    let mut q = world.query_filtered::<Option<&Stat>, With<crate::server::PlayerMob>>();
    for stat in q.iter(world) {
        players += 1;
        if stat == Some(&Stat::Dead) {
            dead += 1;
        }
    }
    let options: &[&[&str]] = match dead_ratio_tier(dead, players) {
        Some(0) => &[
            &[
                "you find yourself",
                "in a small graveyard",
                "humble in size but lovingly maintained",
                "with fresh flowers on the graves",
            ],
            &[
                "you see",
                "a vision of spirits",
                "floating throughout the station",
            ],
        ],
        Some(1) => &[
            &[
                "you find yourself",
                "in a dimly lit hallway",
                "with a sense of dread in the air",
            ],
            &[
                "you see",
                "a vision of an encroaching darkness",
                "threatening you eerily",
            ],
        ],
        Some(2) => &[
            &[
                "you find yourself",
                "in a lonely ballroom",
                "barely lit with flickering lights",
            ],
            &[
                "you see",
                "a picture of a silent battlefield",
                "no clear victor, but heavy losses on all sides",
            ],
        ],
        Some(_) => &[
            &[
                "you find yourself",
                "alone",
                "no sight but your darkness",
                "no sound but your heartbeat",
                "a bleak and hopeless vision",
            ],
            &[
                "you see",
                "a vision of yourself, alone",
                "in a desolate wasteland",
                "with no signs of life or hope in sight",
            ],
        ],
        None => &[&[
            "you see",
            "nothing of note",
            "but have a lingering feeling of unease about the future",
        ]],
    };
    options[pick(world, options.len())].to_vec()
}

/// The `switch(dead / length(GLOB.joined_player_list))` bands: 0.25-0.5, 0.5-0.75, 0.75-0.9,
/// 0.9-1.
pub fn dead_ratio_tier(dead: u32, players: u32) -> Option<u8> {
    if players == 0 {
        return None;
    }
    let ratio = dead as f32 / players as f32;
    match ratio {
        r if r < 0.25 => None,
        r if r <= 0.5 => Some(0),
        r if r <= 0.75 => Some(1),
        r if r <= 0.9 => Some(2),
        _ => Some(3),
    }
}

/// `standard_bless_healing` with the station's deity.
fn standard_bless_healing(world: &mut World, chap: Entity, target: Entity) -> Blessing {
    if !is_human(world, target) {
        if let Some(mut health) = world.get::<Health>(target).cloned() {
            health.adjust(DamageType::Brute, -10.0);
            health.adjust(DamageType::Burn, -10.0);
            world.entity_mut(target).insert(health);
        }
        return Blessing::Success;
    }
    if !heal_hurt_parts(world, target, 10.0) {
        return Blessing::Ignored;
    }
    let (name, chap_name, deity) = (
        name_of(world, target),
        name_of(world, chap),
        glob_deity(world),
    );
    say_around(
        world,
        target,
        Span::Notice,
        format!("{chap_name} heals {name} with the power of {deity}!"),
        &[],
    );
    tell_mob(
        world,
        target,
        Span::BoldNotice,
        format!("May the power of {deity} compel you to be healed!"),
    );
    sound_at(world, chap, PUNCH, 25.0, true);
    crate::mood::add_event(world, target, "blessing", &super::library::BLESSING);
    Blessing::Success
}

/// `LAZYFIND(recent_bless_refs)`, the expired ones forgotten first.
fn recently_blessed(world: &mut World, target: Entity) -> bool {
    let now = now(world);
    let mut station = religion_mut(world);
    station.dreams.recent_blessings.retain(|(_, at)| *at > now);
    station
        .dreams
        .recent_blessings
        .iter()
        .any(|(e, _)| *e == target)
}

fn remember_blessing(world: &mut World, target: Entity) {
    let until = now(world) + BLESS_COOLDOWN_SECS;
    religion_mut(world)
        .dreams
        .recent_blessings
        .push((target, until));
}

/// The dreamer blessing's favor: 20, halved without a mind, halved when not asleep.
pub fn dreamer_favor(mind: bool, sleeping: bool) -> f32 {
    20.0 * if mind { 1.0 } else { 0.5 } * if sleeping { 1.0 } else { 0.5 }
}

/// `/datum/religion_sect/dreams/sect_bless`.
pub(super) fn sect_bless(world: &mut World, chap: Entity, target: Entity) -> Blessing {
    let deity = glob_deity(world);
    let name = name_of(world, target);
    if world.get::<Dreaming>(target).is_some() {
        let mut result = standard_bless_healing(world, chap, target);
        if !recently_blessed(world, target) {
            let sleeping = crate::status_effects::knocked_out(world, target);
            let favor = dreamer_favor(has_mind(world, target), sleeping);
            religion_mut(world).adjust_favor(favor);
            remember_blessing(world, target);
            result = Blessing::Success;
        }
        if result == Blessing::Success {
            tell_mob(
                world,
                chap,
                Span::Notice,
                format!("{deity} approves of {name}'s slumber."),
            );
        }
        return result;
    }
    if unconscious(world, target) {
        if is_human(world, target) {
            crate::dreams::dream(world, target);
        }
        tell_mob(
            world,
            chap,
            Span::Notice,
            format!("{deity} blesses {name}'s slumber."),
        );
        let mut result = standard_bless_healing(world, chap, target);
        if religion(world).dreams.protection
            && has_mind(world, target)
            && unconscious_and_alive(world, target)
            && world.get::<DreamProtection>(target).is_none()
        {
            protect(world, target, ProtectionKind::Temporary);
            result = Blessing::Success;
        }
        return result;
    }
    tell_mob(
        world,
        chap,
        Span::Warning,
        format!("{deity} has no interest in blessing the waking."),
    );
    Blessing::Ignored
}

/// `/datum/religion_sect/dreams/sect_dead_bless`.
pub(super) fn sect_dead_bless(world: &mut World, chap: Entity, target: Entity) -> Blessing {
    if recently_blessed(world, target) {
        return Blessing::Ignored;
    }
    let (deity, name) = (glob_deity(world), name_of(world, target));
    tell_mob(
        world,
        chap,
        Span::Notice,
        format!("{deity} watches over {name}'s eternal rest."),
    );
    let mind = has_mind(world, target);
    if religion(world).dreams.protection && mind && world.get::<DreamProtection>(target).is_none() {
        protect(world, target, ProtectionKind::Deceased);
    }
    religion_mut(world).adjust_favor(if mind { 10.0 } else { 5.0 });
    remember_blessing(world, target);
    Blessing::Success
}

/// The dream rites' `perform_rite` and `can_afford` checks.
pub(super) fn rite_target(world: &mut World, user: Entity, rite: Rite) -> Option<Option<Entity>> {
    if rite == Rite::DreamPortent && !is_human(world, user) {
        tell_mob(
            world,
            user,
            Span::Warning,
            "You are not the sort of creature that can receive a portent.",
        );
        return None;
    }
    Some(None)
}

/// `invoke_effect` of the dream rites.
pub(super) fn invoke_effect(world: &mut World, user: Entity, rite: Rite) -> bool {
    if rite != Rite::DreamPortent {
        return true;
    }
    crate::status_effects::set(world, user, Incap::Sleeping, 10.0);
    if !crate::status_effects::knocked_out(world, user) {
        tell_mob(world, user, Span::Warning, "You fail to fall asleep.");
        return false;
    }
    let name = name_of(world, user);
    say_around(
        world,
        user,
        Span::Notice,
        format!("{name} suddenly falls into a deep slumber, their eyes fluttering..."),
        &[],
    );
    crate::status_effects::adjust_timed(world, user, Timed::Drowsiness, 30.0, None);
    true
}

fn rand_secs(world: &mut World, low: u32, high: u32) -> f64 {
    f64::from(
        world
            .resource_mut::<crate::combat::CombatRng>()
            .range(low, high),
    )
}

/// `post_invoke_effects` of the dream rites.
pub(super) fn post_invoke(world: &mut World, user: Entity, rite: Rite, altar: Entity) {
    match rite {
        Rite::DeaconizeDreamers => {
            let mut station = religion_mut(world);
            station.dreams.deacon_count += 1;
            if station.dreams.deacon_count >= MAX_DEACONS {
                station.used_rites.push(Rite::DeaconizeDreamers);
            }
        }
        Rite::DreamPortent => {
            let at = now(world) + rand_secs(world, 2, 5);
            world.entity_mut(user).insert(PortentPending {
                favor_paid: rite.def().favor_cost,
                dream_at: Some(at),
            });
        }
        Rite::DreamProtection => {
            religion_mut(world).dreams.protection = true;
            let mut q = world.query_filtered::<Entity, With<HolyRole>>();
            let mut holy: Vec<Entity> = q.iter(world).collect();
            if !holy.contains(&user) {
                holy.push(user);
            }
            for mob in holy {
                protect(world, mob, ProtectionKind::Permanent);
            }
        }
        Rite::SlumberParty => slumber_party(world, altar),
        _ => {}
    }
}

/// `slumber_party/post_invoke_effects`: one dream for every human in view of the altar.
fn slumber_party(world: &mut World, altar: Entity) {
    let Some(pos) = crate::chem::map_pos(world, altar) else {
        return;
    };
    let nouns: Vec<&str> = Vec::new();
    let shared = if world.contains_resource::<crate::dreams::DreamStrings>() {
        world.resource_scope(|world, strings: Mut<crate::dreams::DreamStrings>| {
            crate::dreams::random_dream(
                &strings,
                &nouns,
                &mut world.resource_mut::<crate::dreams::DreamRng>().0,
            )
        })
    } else {
        Vec::new()
    };
    let now = now(world);
    for mob in crate::grenades::hearers(world, pos, SLUMBER_RANGE) {
        if !is_human(world, mob) || world.get::<Stat>(mob) == Some(&Stat::Dead) {
            continue;
        }
        let holy = holy_role(world, mob).is_some();
        // Checked before the sleep (tg puts them to sleep first, then refuses the effect).
        if !holy
            && crate::spells::can_block_magic(
                world,
                mob,
                crate::spells::MAGIC_RESISTANCE_HOLY | crate::spells::MAGIC_RESISTANCE_MIND,
            )
        {
            continue;
        }
        crate::status_effects::set(world, mob, Incap::Sleeping, SLUMBER_SECS as f32);
        if !crate::status_effects::knocked_out(world, mob) {
            continue;
        }
        let dream_at = now + rand_secs(world, 4, 8);
        world.entity_mut(mob).insert(SlumberParty {
            until: now + SLUMBER_SECS,
            healing: if holy {
                SLUMBER_HEALING * 2.0
            } else {
                SLUMBER_HEALING
            },
            shared: shared.clone(),
            dream_at: Some(dream_at),
        });
    }
}

/// `slumber_party/tick` (every second), `force_dream` and `on_remove`.
fn slumber_parties(world: &mut World, mut last: Local<f64>) {
    let now = now(world);
    let secs = (now - *last).clamp(0.0, 1.0) as f32;
    if now - *last < 1.0 {
        return;
    }
    *last = now;
    let sleepers: Vec<(Entity, SlumberParty)> = world
        .query::<(Entity, &SlumberParty)>()
        .iter(world)
        .map(|(e, p)| (e, p.clone()))
        .collect();
    for (mob, party) in sleepers {
        if party.until <= now || !unconscious_and_alive(world, mob) {
            world.entity_mut(mob).remove::<SlumberParty>();
            crate::status_effects::adjust_timed(world, mob, Timed::Drowsiness, 20.0, None);
            continue;
        }
        if let Some(mut health) = world.get::<Health>(mob).cloned() {
            let amount = party.healing * secs;
            health.adjust(DamageType::Brute, -amount);
            health.adjust(DamageType::Burn, -amount);
            health.oxy = (health.oxy - amount * 0.25).max(0.0);
            health.tox = (health.tox - amount * 0.5).max(0.0);
            world.entity_mut(mob).insert(health);
        }
        if party.dream_at.is_some_and(|at| at <= now) {
            if let Some(mut p) = world.get_mut::<SlumberParty>(mob) {
                p.dream_at = None;
            }
            crate::dreams::dream(world, mob);
        }
    }
}

/// The portent rite's `force_dream`.
fn portents(world: &mut World) {
    let now = now(world);
    let due: Vec<Entity> = world
        .query::<(Entity, &PortentPending)>()
        .iter(world)
        .filter(|(_, p)| p.dream_at.is_some_and(|at| at <= now))
        .map(|(e, _)| e)
        .collect();
    for mob in due {
        if let Some(mut p) = world.get_mut::<PortentPending>(mob) {
            p.dream_at = None;
        }
        crate::dreams::dream(world, mob);
        // Awake before it came: the rite is gone (tg drops it with the dream's end).
        if world.get::<Dreaming>(mob).is_none() && !unconscious(world, mob) {
            world.entity_mut(mob).remove::<PortentPending>();
        }
    }
}

/// The temporary protection ends with the sleep, the deceased one with the revival.
fn end_protections(world: &mut World) {
    let now = now(world);
    let ended: Vec<Entity> = world
        .query::<(Entity, &DreamProtection, Option<&Stat>)>()
        .iter(world)
        .filter(|(_, p, stat)| {
            let timed_out = p.until.is_some_and(|u| u <= now);
            match p.kind {
                ProtectionKind::Permanent => false,
                ProtectionKind::Temporary => timed_out,
                ProtectionKind::Deceased => timed_out || *stat != Some(&Stat::Dead),
            }
        })
        .map(|(e, ..)| e)
        .collect();
    for mob in ended {
        world.entity_mut(mob).remove::<DreamProtection>();
    }
    let woke: Vec<Entity> = world
        .query::<(Entity, &DreamProtection)>()
        .iter(world)
        .filter(|(_, p)| p.kind == ProtectionKind::Temporary)
        .map(|(e, _)| e)
        .collect();
    for mob in woke {
        if !unconscious_and_alive(world, mob) && world.get::<Dreaming>(mob).is_none() {
            world.entity_mut(mob).remove::<DreamProtection>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blessing_a_dreamer_pays_by_mind_and_sleep() {
        assert_eq!(dreamer_favor(true, true), 20.0);
        assert_eq!(dreamer_favor(false, true), 10.0);
        assert_eq!(dreamer_favor(true, false), 10.0);
        assert_eq!(dreamer_favor(false, false), 5.0);
    }

    #[test]
    fn the_dead_shape_the_portent() {
        assert_eq!(dead_ratio_tier(0, 0), None);
        assert_eq!(dead_ratio_tier(1, 10), None);
        assert_eq!(dead_ratio_tier(3, 10), Some(0));
        assert_eq!(dead_ratio_tier(6, 10), Some(1));
        assert_eq!(dead_ratio_tier(8, 10), Some(2));
        assert_eq!(dead_ratio_tier(10, 10), Some(3));
    }

    #[test]
    fn three_deacons_close_the_rite() {
        let mut world = World::new();
        world.insert_resource(super::super::Religion {
            sect: Some(Sect::Dreams),
            ..Default::default()
        });
        let (user, altar) = (world.spawn_empty().id(), world.spawn_empty().id());
        assert!(religion(&world).rites().contains(&Rite::DeaconizeDreamers));
        assert!(!religion(&world).rites().contains(&Rite::Deaconize));
        for _ in 0..3 {
            post_invoke(&mut world, user, Rite::DeaconizeDreamers, altar);
        }
        assert!(!religion(&world).rites().contains(&Rite::DeaconizeDreamers));
    }

    #[test]
    fn dream_protection_covers_the_holy_and_a_portent_waits_for_its_dream() {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.insert_resource(crate::combat::CombatRng(7));
        world.insert_resource(super::super::Religion {
            sect: Some(Sect::Dreams),
            ..Default::default()
        });
        let priest = world.spawn(HolyRole::Priest).id();
        let altar = world.spawn_empty().id();
        post_invoke(&mut world, priest, Rite::DreamProtection, altar);
        assert!(religion(&world).dreams.protection);
        assert_eq!(
            world.get::<DreamProtection>(priest).map(|p| p.kind),
            Some(ProtectionKind::Permanent)
        );
        post_invoke(&mut world, priest, Rite::DreamPortent, altar);
        let pool = dream_pool(&mut world, priest);
        assert_eq!(pool.len(), 1);
        assert_eq!(pool[0].kind, DreamKind::SpecificPortent);
        // Another dream came: 80% of the 50 favor back.
        dream_started(&mut world, priest, DreamKind::Random);
        assert!(world.get::<PortentPending>(priest).is_none());
        // 40 refunded, then 10 for the holy dreamer's slumber.
        assert_eq!(religion(&world).favor, 50.0);
    }
}
