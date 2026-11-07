//! Special and magic brain traumas (server), after tg `code/datums/brain_damage/special.dm` and
//! `magic.dm`: their `on_gain`/`on_life`/`on_lose` effects, called by
//! [`super::traumas`] (the kinds, texts and resiliences live there).
//!
//! Special: godwoken (loud godly lines now and then, `TRAIT_HOLY`), bluespace prophet (pairs of
//! bluespace streams only the owner sees; a click on one and 2 s standing still take them to
//! the other), quantum alignment (entangles with what it pulls, an adjacent mob, a held item or
//! the floor, then snaps back to it), tenacity (no soft or hard crit), death whispers (reads
//! deadchat for a while, [`ss13_chat::server::SixthSense`]), existential crisis (fades out of
//! reality for 5–45 s), the criminal's phantom securitron, combat PTSD (no mood, disturbed
//! sanity, battle hallucinations). Magic: lumiphobia (light burns), poltergeist (throws the
//! nearest heaviest item at the owner), antimagic, the psychic stalker (and its cosmic kind).
// shortcut: no psychotic brawling (needs its martial art), primal instincts (no monkey AI for
// humans) or fire-axe adoration; godwoken speaks without the voice of god's commands (not
// ported); the phantoms' images exist only for a player owner; the stalker's heartbeat plays
// once instead of looping; no wibbly filter on the streams; tenacity's `TRAIT_ANALGESIA`
// (no pain system); PTSD gives no `desensitized` status effect and no stray bullet.

use bevy::prelude::*;
use ss13_mapload::{DmItem, IconIndex};
use ss13_net::server::{Invisibility, Networked, VisibleToMobs};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, IconStateId, Layer, MouseOpacity, TilePos};

use super::traumas::{Kind, Link, Trauma};
use super::wounds::spt_prob;
use crate::combat::CombatRng;
use crate::hallucination::{ImageLook, hide_image, show_image};
use crate::health::{DamageType, Stat};

pub const INVOKE: &str = "sound/effects/magic/clockwork/invoke_general.ogg";
pub const CHARGEUP: &str = "sound/effects/magic/lightning_chargeup.ogg";
pub const REPULSE: &str = "sound/effects/magic/repulse.ogg";
pub const IAMTHELAW: &str = "sound/mobs/non-humanoids/beepsky/iamthelaw.ogg";
pub const CRIMINAL: &str = "sound/mobs/non-humanoids/beepsky/criminal.ogg";
pub const EGLOVES: &str = "sound/items/weapons/egloves.ogg";
pub const DEMON_ATTACK: &str = "sound/effects/magic/demon_attack1.ogg";
pub const SLOWBEAT: &str = "sound/effects/health/slowbeat.ogg";
/// The files these traumas play (for the asset converter's list).
pub const SOUNDS: [&str; 8] = [
    INVOKE,
    CHARGEUP,
    REPULSE,
    IAMTHELAW,
    CRIMINAL,
    EGLOVES,
    DEMON_ATTACK,
    SLOWBEAT,
];

/// `ABOVE_MOB_LAYER`, `GAME_PLANE`.
const ABOVE_MOB_LAYER: f32 = 4.1;
const GAME_PLANE: i16 = -4;
const STREAM_ICON: (&str, &str) = ("icons/effects/effects.dmi", "bluestream");
const SECBOT: ImageLook = ImageLook::new("icons/mob/silicon/aibots.dmi", "secbot-c", 4.0);
const PHANTOM: ImageLook = ImageLook::new(
    "icons/mob/simple/lavaland/lavaland_monsters.dmi",
    "curseblob",
    4.0,
);
const STAR_GAZER: ImageLook = ImageLook::new(
    "icons/mob/nonhuman-player/96x96eldritch_mobs.dmi",
    "star_gazer",
    4.0,
);
/// `QDEL_IN(src, 30 SECONDS)` of a stream.
const STREAM_SECS: f64 = 30.0;
/// The bluespace stream's `do_after(user, 2 SECONDS)`.
const STREAM_WAIT: f64 = 2.0;
/// `SHADOW_SPECIES_LIGHT_THRESHOLD`.
const LIGHT_THRESHOLD: f32 = crate::antag::nightmare::LIGHT_THRESHOLD;

/// `strings/traumas.json` `god_*` (BandaStation).
const GOD_NEUTRAL: [&str; 13] = [
    "БУДЬТЕ ИСЦЕЛЕНЫ, СМЕРТНЫЕ. Я ВЕЛИКОДУШЕН.",
    "ИСЧЕЗНИТЕ, СМЕРТНЫЕ.",
    "ТАНЦУЙТЕ ДЛЯ МЕНЯ, МАЛЕНЬКИЕ СМЕРТНЫЕ.",
    "СТОП.",
    "ХОНК...",
    "СМЕРТНЫЕ, НАЗОВИТЕ СВОЁ ИМЯ.",
    "ОТДОХНИТЕ, СМЕРТНЫЕ, ЗАВТРА ДЛИННЫЙ ДЕНЬ.",
    "УЗРИТЕ ИСТИНУ ПЕРЕД СОБОЙ, СМЕРТНЫЕ.",
    "ВЫ, СМЕРТНЫЕ, ВЫЗЫВАЕТЕ У МЕНЯ ОТВРАЩЕНИЕ.",
    "ТЫ. ОСТАНОВИСЬ.",
    "ЗАТКНИСЬ.",
    "ТИШЕ",
    "ХВАТИТ",
];
const GOD_AGGRESSIVE: [&str; 7] = [
    "ВСЕ ПАДУТ ПРЕДО МНОЙ!",
    "ГОРИ, @!",
    "СМЕРТЬ ТЕБЕ, @!",
    "ВЕЧНЫЕ СТРАДАНИЯ ЖДУТ ТЕБЯ, @.",
    "ИСЧЕЗНИ, @!",
    "ИСТЕКАЙ КРОВЬЮ, @!",
    "УМРИ, @!",
];
const GOD_FOE: [&str; 8] = [
    "БОГОХУЛЬНИК",
    "ПАРАЗИТ",
    "ПРОСТОЛЮДИН",
    "НЕВЕРУЮЩИЙ",
    "СЛАБАК",
    "ЕРЕТИК",
    "НАСЕКОМОЕ",
    "СМЕРТНЫЙ",
];
const GOD_UNSTUN: [&str; 3] = [
    "ВСТАНЬ, ЖРЕЦ.",
    "ВСТАНЬ. У МЕНЯ НЕТ ВРЕМЕНИ.",
    "ПОДНИМАЙСЯ.",
];
const GOD_HEAL: [&str; 4] = [
    "БУДЬ ИСЦЕЛЁН, ЖРЕЦ.",
    "ТЫ ВЫЖИВЕШЬ, МОЙ ЖРЕЦ.",
    "ТЫ УВИДИШЬ ЕЩЁ ОДИН ДЕНЬ.",
    "ТВОЯ ЖИЗНЬ ВАЖНА. БЕРЕГИ ЕЁ.",
];
const CRISIS_LINES: [&str; 6] = [
    "Вы вообще существуете?",
    "Быть или не быть...",
    "Зачем существовать?",
    "Вы просто исчезаете.",
    "Вы перестаете быть настоящим.",
    "Вы на мгновение перестаете думать. Следовательно, вас нет.",
];
const SLIP_IN: [&str; 5] = [
    "странным образом скользит в сторону и исчезает",
    "прыгает в невидимое измерение",
    "вытягивает одну ногу прямо, шевелит своей ногой и внезапно исчезает",
    "останавливается, затем исчезает из реальности",
    "затягивается в невидимый вихрь, исчезая из виду",
];
const SLIP_OUT: [&str; 5] = [
    "бесшумно появляется из ниоткуда",
    "выпрыгивает из воздуха",
    "появляется",
    "выходит из невидимого дверного проема",
    "выскальзывает из складки пространства-времени",
];

/// `/obj/effect/client_image_holder/bluespace_stream`: seen by its owner only.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct BluespaceStream {
    pub owner: Entity,
    pub linked: Option<Entity>,
    pub expires_at: f64,
}

/// A mob adjusting to a stream (the stream's `do_after`): where it stood and when it is done.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct StreamWait {
    pub stream: Entity,
    pub from: TilePos,
    pub done_at: f64,
}

/// `/obj/effect/abstract/sync_holder/veil`: what an existential crisis hides its owner in
/// (`crate::closets::InCloset` of a holder that is no closet).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Veil;

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    app.add_click_handler(ClickPhase::EmptyHand, 3471, stream_click);
    app.add_systems(
        Update,
        (
            expire_streams.run_if(any_with_component::<BluespaceStream>),
            stream_waits.run_if(any_with_component::<StreamWait>),
        ),
    );
}

use crate::tools::now;

fn say_to(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    super::diseases::say_to(world, mob, span, text);
}

fn pick<'a>(rng: &mut CombatRng, list: &[&'a str]) -> &'a str {
    list[rng.range(0, list.len() as u32 - 1) as usize]
}

fn pos_of(world: &World, e: Entity) -> Option<TilePos> {
    crate::explosions::turf_of(world, e)
}

/// `IS_UNCONSCIOUS_OR_CRIT`.
fn out_or_crit(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) != Some(&Stat::Conscious)
        || crate::status_effects::knocked_out(world, mob)
}

/// `get_step_towards(from, to)`.
fn step_towards(from: TilePos, to: TilePos) -> TilePos {
    TilePos::new(
        from.x + (to.x - from.x).signum(),
        from.y + (to.y - from.y).signum(),
        from.z,
    )
}

/// `locate(owner.x + pick(-12, 12), owner.y + pick(-12, 12), owner.z)`.
fn random_corner(pos: TilePos, rng: &mut CombatRng) -> TilePos {
    let off = |rng: &mut CombatRng| if rng.chance(50.0) { -12 } else { 12 };
    TilePos::new(pos.x + off(rng), pos.y + off(rng), pos.z)
}

fn play_local(world: &mut World, mob: Entity, source: Option<TilePos>, sound: &str, vol: f32) {
    crate::hallucination::play_local(world, mob, source, sound, vol, false);
}

/// The trauma's `on_gain` past the shared part.
pub fn on_gain(world: &mut World, mob: Entity, trauma: &mut Trauma, rng: &mut CombatRng) {
    match trauma.kind {
        Kind::Beepsky => spawn_phantom(world, mob, trauma, SECBOT, rng),
        Kind::Stalker { cosmic } => {
            let look = if cosmic { STAR_GAZER } else { PHANTOM };
            spawn_phantom(world, mob, trauma, look, rng);
        }
        Kind::Ptsd => {
            crate::mood::add_event(world, mob, "combat_ptsd", &DESENTIZED);
            if let Some(mut mood) = world.get_mut::<crate::mood::Mood>(mob) {
                mood.mood_modifier -= 1.0;
                mood.set_sanity(
                    crate::mood::SANITY_DISTURBED,
                    crate::mood::SANITY_INSANE,
                    crate::mood::SANITY_MAXIMUM,
                    true,
                );
            }
        }
        _ => {}
    }
}

/// The trauma's `on_lose` past the shared part.
pub fn on_lose(world: &mut World, mob: Option<Entity>, trauma: &Trauma) {
    if let Some((_, Some(image))) = trauma.extra.phantom {
        hide_image(world, image);
    }
    let Some(mob) = mob else {
        return;
    };
    match trauma.kind {
        Kind::DeathWhispers => {
            world
                .entity_mut(mob)
                .remove::<ss13_chat::server::SixthSense>();
        }
        Kind::ExistentialCrisis => fade_in(world, mob, None),
        Kind::Ptsd => {
            crate::mood::clear_event(world, mob, "combat_ptsd");
            if let Some(mut mood) = world.get_mut::<crate::mood::Mood>(mob) {
                mood.mood_modifier += 1.0;
                mood.set_sanity(
                    crate::mood::SANITY_GREAT,
                    crate::mood::SANITY_INSANE,
                    crate::mood::SANITY_MAXIMUM,
                    true,
                );
            }
        }
        _ => {}
    }
}

/// `/datum/mood_event/desentized`.
const DESENTIZED: crate::mood::MoodEvent = crate::mood::MoodEvent {
    id: "desentized",
    description: "Ничто и никогда не сравнится с тем, что я пережил...",
    mood_change: -3.0,
    timeout: 0.0,
    hidden: false,
};

/// The trauma's `on_life`.
pub fn on_life(
    world: &mut World,
    mob: Entity,
    trauma: &mut Trauma,
    secs: f32,
    rng: &mut CombatRng,
) {
    match trauma.kind {
        Kind::Godwoken => godwoken(world, mob, rng, secs),
        Kind::BluespaceProphet => bluespace_prophet(world, mob, trauma, rng),
        Kind::QuantumAlignment => quantum_alignment(world, mob, trauma, rng, secs),
        Kind::DeathWhispers => death_whispers(world, mob, trauma, rng),
        Kind::ExistentialCrisis => existential_crisis(world, mob, trauma, rng, secs),
        Kind::Beepsky => beepsky(world, mob, trauma, rng, secs),
        Kind::Ptsd => ptsd(world, mob, trauma, rng),
        Kind::Lumiphobia => lumiphobia(world, mob, trauma, secs),
        Kind::Poltergeist => poltergeist(world, mob, rng, secs),
        Kind::Stalker { cosmic } => stalker(world, mob, trauma, cosmic, rng, secs),
        _ => {}
    }
}

/// `/datum/brain_trauma/special/godwoken/on_life` and `speak`.
fn godwoken(world: &mut World, mob: Entity, rng: &mut CombatRng, secs: f32) {
    if !spt_prob(rng, 2.0, secs) {
        return;
    }
    let stunned = world
        .get::<crate::status_effects::StatusEffects>(mob)
        .is_some_and(|e| e.stun > 0.0 || e.paralyze > 0.0 || e.unconscious > 0.0);
    let critical = world
        .get::<Stat>(mob)
        .is_some_and(|&s| s != Stat::Conscious);
    let combat = world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0);
    let line = if rng.chance(33.0) && stunned {
        pick(rng, &GOD_UNSTUN).to_owned()
    } else if rng.chance(60.0) && critical {
        pick(rng, &GOD_HEAL).to_owned()
    } else if rng.chance(30.0) && combat {
        let foe = pick(rng, &GOD_FOE);
        pick(rng, &GOD_AGGRESSIVE).replace('@', foe)
    } else {
        pick(rng, &GOD_NEUTRAL).to_owned()
    };
    let Some(pos) = pos_of(world, mob) else {
        return;
    };
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, INVOKE, 200.0, true).extrarange(5.0),
    );
    let name = super::name_of(world, mob);
    let clients =
        crate::obj_damage::clients_in_range(world, pos, crate::health::DEFAULT_MESSAGE_RANGE, &[]);
    if !clients.is_empty() {
        // `voice_of_god(..., list("colossus", "yell"))`: the words loud.
        let mut message = ss13_chat::server::say_message(&name, &line);
        if let Some(last) = message.segments.last_mut() {
            last.span = Span::BoldAnnounce;
        }
        world.write_message(ss13_net::chat::server::SendChat { clients, message });
    }
}

/// `/datum/brain_trauma/special/bluespace_prophet/on_life`: every 10 s two linked streams on
/// clear tiles within 8, at least 4 apart.
fn bluespace_prophet(world: &mut World, mob: Entity, trauma: &mut Trauma, rng: &mut CombatRng) {
    let now = now(world);
    if now < trauma.extra.next_at {
        return;
    }
    trauma.extra.next_at = now + 10.0;
    let Some(pos) = pos_of(world, mob) else {
        return;
    };
    let Some(index) = world.get_resource::<ss13_world::SpatialIndex>() else {
        return;
    };
    let mut tiles = Vec::new();
    for dy in -8..=8 {
        for dx in -8..=8 {
            let at = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
            if index.in_bounds(at) && index.dense_on_tile(at).is_empty() {
                tiles.push(at);
            }
        }
    }
    if tiles.is_empty() {
        return;
    }
    let first = tiles[rng.range(0, tiles.len() as u32 - 1) as usize];
    tiles.retain(|t| t.chebyshev(first).is_none_or(|d| d > 3));
    if tiles.is_empty() {
        return;
    }
    let second = tiles[rng.range(0, tiles.len() as u32 - 1) as usize];
    let a = spawn_stream(world, mob, first, now);
    let b = spawn_stream(world, mob, second, now);
    for (stream, other) in [(a, b), (b, a)] {
        if let Some(mut s) = world.get_mut::<BluespaceStream>(stream) {
            s.linked = Some(other);
        }
    }
}

fn spawn_stream(world: &mut World, owner: Entity, pos: TilePos, now: f64) -> Entity {
    let icon = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(STREAM_ICON.0, STREAM_ICON.1))
        .unwrap_or(IconStateId(u32::MAX));
    world
        .spawn((
            Name::new("bluespace stream"),
            pos,
            Appearance {
                icon_state: icon,
                layer: Layer(ABOVE_MOB_LAYER),
                plane: GAME_PLANE,
                color: None,
                pixel_offset: [0, 0],
                rotation: 0,
                overlays: Vec::new(),
                mouse_opacity: MouseOpacity::Icon,
            },
            Networked,
            Invisibility(u8::MAX),
            VisibleToMobs(vec![owner]),
            BluespaceStream {
                owner,
                linked: None,
                expires_at: now + STREAM_SECS,
            },
        ))
        .id()
}

/// A stream lasts 30 s and takes its twin along (`Destroy`).
fn expire_streams(world: &mut World) {
    let now = now(world);
    let streams: Vec<(Entity, BluespaceStream)> = world
        .query::<(Entity, &BluespaceStream)>()
        .iter(world)
        .map(|(e, s)| (e, *s))
        .collect();
    for (stream, s) in streams {
        let twin_gone = s.linked.is_some_and(|l| world.get_entity(l).is_err());
        if now >= s.expires_at || twin_gone || world.get_entity(s.owner).is_err() {
            world.try_despawn(stream).ok();
        }
    }
}

/// The stream's `attack_hand`: only its owner, next to it, starts adjusting (2 s standing).
/// Click handler ([`crate::click_handlers`] `EmptyHand` 3471).
pub fn stream_click(world: &mut World, ctx: &crate::click_handlers::ClickCtx) -> bool {
    let Some(stream) = world.get::<BluespaceStream>(ctx.target).copied() else {
        return false;
    };
    if stream.owner != ctx.mob || stream.linked.is_none() {
        return true;
    }
    let near = world
        .get::<TilePos>(ctx.target)
        .and_then(|&p| p.chebyshev(ctx.mob_pos))
        .is_some_and(|d| d <= 1);
    if !near || world.get::<StreamWait>(ctx.mob).is_some() {
        return true;
    }
    say_to(
        world,
        ctx.mob,
        Span::Notice,
        "Вы пытаетесь приспособиться к потоку блюспейса...",
    );
    let done_at = now(world) + STREAM_WAIT;
    world.entity_mut(ctx.mob).insert(StreamWait {
        stream: ctx.target,
        from: ctx.mob_pos,
        done_at,
    });
    true
}

/// The end of the stream's `do_after`, checked again: the mob stayed, the streams still exist.
fn stream_waits(world: &mut World) {
    let now = now(world);
    let waits: Vec<(Entity, StreamWait)> = world
        .query::<(Entity, &StreamWait)>()
        .iter(world)
        .map(|(e, w)| (e, *w))
        .collect();
    for (mob, wait) in waits {
        let here = pos_of(world, mob);
        let dest = world
            .get::<BluespaceStream>(wait.stream)
            .and_then(|s| s.linked)
            .and_then(|l| world.get::<TilePos>(l).copied());
        if here != Some(wait.from) || dest.is_none() || out_or_crit(world, mob) {
            world.entity_mut(mob).remove::<StreamWait>();
            continue;
        }
        if now < wait.done_at {
            continue;
        }
        world.entity_mut(mob).remove::<StreamWait>();
        let Some(dest) = dest else {
            continue;
        };
        let mut rng = super::fork_rng(world);
        let name = super::name_of(world, mob);
        let slip_in = pick(&mut rng, &SLIP_IN);
        let slip_out = pick(&mut rng, &SLIP_OUT);
        super::visible(world, mob, "", &format!("{name} {slip_in}."), Span::Warning);
        let tele = crate::teleport::Tele {
            no_effects: true,
            ..default()
        };
        if crate::teleport::do_teleport(world, mob, dest, tele) {
            super::visible(
                world,
                mob,
                "...и найти дорогу на другую сторону.",
                &format!("{name} {slip_out}."),
                Span::Warning,
            );
        } else {
            super::visible(
                world,
                mob,
                "...и окажетесь там, где начали?",
                &format!("{name} {slip_out}, оказавшись точно там же, откуда ушли."),
                Span::Warning,
            );
        }
    }
}

/// `/datum/brain_trauma/special/quantum_alignment/on_life`, `try_entangle`, `snapback`.
fn quantum_alignment(
    world: &mut World,
    mob: Entity,
    trauma: &mut Trauma,
    rng: &mut CombatRng,
    secs: f32,
) {
    let now = now(world);
    if let Some(link) = trauma.extra.link {
        let target = match link {
            Link::Entity(e) => pos_of(world, e),
            Link::Turf(pos) => Some(pos),
        };
        let Some(target) = target else {
            if trauma.extra.until.is_some() {
                say_to(
                    world,
                    mob,
                    Span::Notice,
                    "Связь резко обрывается, а вместе с ней и притяжение.",
                );
            }
            trauma.extra.link = None;
            trauma.extra.until = None;
            return;
        };
        match trauma.extra.until {
            // `start_snapback`.
            None if now >= trauma.extra.next_at => {
                let name = link_name(world, link);
                say_to(
                    world,
                    mob,
                    Span::Warning,
                    format!(
                        "Ваша связь с {name} внезапно становится невероятно сильной... вы можете почувствовать, как она притягивает вас!"
                    ),
                );
                play_local(world, mob, None, CHARGEUP, 75.0);
                trauma.extra.until = Some(now + 10.0);
            }
            // `snapback`.
            Some(at) if now >= at => {
                say_to(
                    world,
                    mob,
                    Span::Warning,
                    "Вас тянет сквозь пространство-время!",
                );
                let tele = crate::teleport::Tele {
                    channel: crate::teleport::Channel::Quantum,
                    ..default()
                };
                crate::teleport::do_teleport(world, mob, target, tele);
                play_local(world, mob, None, REPULSE, 100.0);
                trauma.extra.link = None;
                trauma.extra.until = None;
            }
            _ => {}
        }
        return;
    }
    if !spt_prob(rng, 2.0, secs) {
        return;
    }
    let link = entangle_target(world, mob);
    let Some(link) = link else {
        return;
    };
    let name = link_name(world, link);
    say_to(
        world,
        mob,
        Span::Notice,
        format!("Вы начинаете испытывать сильное чувство связи с {name}."),
    );
    trauma.extra.link = Some(link);
    trauma.extra.until = None;
    trauma.extra.next_at = now + f64::from(rng.range(45, 600));
}

fn link_name(world: &World, link: Link) -> String {
    match link {
        Link::Entity(e) => super::name_of(world, e),
        Link::Turf(_) => "полом".to_owned(),
    }
}

/// `try_entangle`: a pulled mob, an adjacent mob, a pulled object, the active then the other
/// hand's item (not `TRAIT_NODROP`), else the floor.
pub fn entangle_target(world: &World, mob: Entity) -> Option<Link> {
    let pos = pos_of(world, mob)?;
    let pulled = world.get::<crate::pull::Pulling>(mob).map(|p| p.0);
    let is_mob = |e: Entity| world.get::<crate::health::Health>(e).is_some();
    if let Some(p) = pulled.filter(|&p| is_mob(p)) {
        return Some(Link::Entity(p));
    }
    if let Some(index) = world.get_resource::<ss13_world::SpatialIndex>() {
        let mut near: Vec<Entity> = index
            .in_radius(pos, 1)
            .filter(|&e| e != mob && is_mob(e))
            .filter(|&e| {
                world
                    .get::<TilePos>(e)
                    .is_some_and(|&at| index.adjacent(pos, at, |x| x == e))
            })
            .collect();
        near.sort_unstable();
        if let Some(&e) = near.first() {
            return Some(Link::Entity(e));
        }
    }
    if let Some(p) = pulled {
        return Some(Link::Entity(p));
    }
    if let Some(inventory) = world.get::<crate::inventory::Inventory>(mob) {
        let active = inventory.active;
        for hand in [active, 1 - active] {
            if let Some(item) = inventory.hands[hand]
                && !crate::inventory::no_drop(world, item)
            {
                return Some(Link::Entity(item));
            }
        }
    }
    Some(Link::Turf(pos))
}

/// `/datum/brain_trauma/special/death_whispers/on_life`: `prob(2)` a `Life()` to read deadchat
/// for 5–30 s.
fn death_whispers(world: &mut World, mob: Entity, trauma: &mut Trauma, rng: &mut CombatRng) {
    let now = now(world);
    match trauma.extra.until {
        Some(at) if now >= at => {
            trauma.extra.until = None;
            world
                .entity_mut(mob)
                .remove::<ss13_chat::server::SixthSense>();
        }
        Some(_) => {}
        None if rng.chance(2.0) => {
            trauma.extra.until = Some(now + f64::from(rng.range(5, 30)));
            world.entity_mut(mob).insert(ss13_chat::server::SixthSense);
        }
        None => {}
    }
}

/// `/datum/brain_trauma/special/existential_crisis/on_life`, `fade_out`, `fade_in`.
fn existential_crisis(
    world: &mut World,
    mob: Entity,
    trauma: &mut Trauma,
    rng: &mut CombatRng,
    secs: f32,
) {
    let now = now(world);
    if let Some(at) = trauma.extra.until {
        if now >= at {
            fade_in(world, mob, Some(trauma));
        }
        return;
    }
    if now < trauma.extra.next_at || !spt_prob(rng, 1.5, secs) {
        return;
    }
    // `isturf(owner.loc)`.
    if world.get::<crate::closets::InCloset>(mob).is_some()
        || world.get::<crate::inventory::CarriedBy>(mob).is_some()
    {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let veil = world.spawn((Veil, Name::new("небытие"), pos)).id();
    crate::closets::put_mob_inside(world, mob, veil);
    say_to(world, mob, Span::Warning, pick(rng, &CRISIS_LINES));
    trauma.extra.entity = Some(veil);
    trauma.extra.until = Some(now + f64::from(rng.range(5, 45)));
    trauma.extra.next_at = now + 60.0;
}

/// `fade_in`: out of the veil (also when the trauma goes: `trauma` is `None` then).
fn fade_in(world: &mut World, mob: Entity, trauma: Option<&mut Trauma>) {
    let veil = world
        .get::<crate::closets::InCloset>(mob)
        .map(|c| c.closet)
        .filter(|&v| world.get::<Veil>(v).is_some());
    let Some(veil) = veil else {
        return;
    };
    crate::closets::let_out(world, mob);
    world.despawn(veil);
    say_to(world, mob, Span::Notice, "Вы возвращаетесь в реальность.");
    if let Some(trauma) = trauma {
        trauma.extra.entity = None;
        trauma.extra.until = None;
        trauma.extra.next_at = now(world) + 60.0;
    }
}

/// A phantom chasing the owner from a random corner 12 tiles away (`create_securitron`,
/// `create_stalker`).
fn spawn_phantom(
    world: &mut World,
    mob: Entity,
    trauma: &mut Trauma,
    look: ImageLook,
    rng: &mut CombatRng,
) {
    if let Some((_, Some(image))) = trauma.extra.phantom.take() {
        hide_image(world, image);
    }
    let Some(pos) = pos_of(world, mob) else {
        return;
    };
    let at = random_corner(pos, rng);
    let image = show_image(world, mob, at, look, None);
    trauma.extra.phantom = Some((at, image));
}

fn move_phantom(world: &mut World, trauma: &mut Trauma, to: TilePos) {
    if let Some((pos, image)) = trauma.extra.phantom.as_mut() {
        *pos = to;
        if let Some(image) = *image
            && world.get_entity(image).is_ok()
        {
            world.entity_mut(image).insert(to);
        }
    }
}

fn phantom_distance(world: &World, mob: Entity, trauma: &Trauma) -> Option<u32> {
    let (at, _) = trauma.extra.phantom?;
    pos_of(world, mob)?.chebyshev(at)
}

/// `/datum/brain_trauma/special/beepsky/on_life` and the securitron's `process` (40% a fast
/// tick to stand still, else a step towards the owner).
fn beepsky(world: &mut World, mob: Entity, trauma: &mut Trauma, rng: &mut CombatRng, secs: f32) {
    let Some(pos) = pos_of(world, mob) else {
        return;
    };
    if phantom_distance(world, mob, trauma).is_none() {
        if !rng.chance(30.0) {
            return;
        }
        spawn_phantom(world, mob, trauma, SECBOT, rng);
    }
    if phantom_distance(world, mob, trauma).is_some_and(|d| d >= 10) && rng.chance(20.0) {
        spawn_phantom(world, mob, trauma, SECBOT, rng);
    }
    let phantom_at = trauma.extra.phantom.map(|(at, _)| at);
    if out_or_crit(world, mob) {
        if rng.chance(20.0) {
            play_local(world, mob, phantom_at, IAMTHELAW, 50.0);
        }
        return;
    }
    if phantom_distance(world, mob, trauma).is_some_and(|d| d <= 1) {
        play_local(world, mob, None, EGLOVES, 50.0);
        let name = super::name_of(world, mob);
        super::visible(
            world,
            mob,
            "Вы чувствуете силу ЗАКОНА.",
            &format!("Тело {name} дергается, как будто его ударили током."),
            Span::Warning,
        );
        crate::status_effects::adjust_stamina(world, mob, rng.range(40, 70) as f32);
        if let Some((_, Some(image))) = trauma.extra.phantom.take() {
            hide_image(world, image);
        }
        trauma.extra.phantom = None;
        return;
    }
    if rng.chance(20.0) && phantom_distance(world, mob, trauma).is_some_and(|d| d <= 8) {
        play_local(world, mob, phantom_at, CRIMINAL, 40.0);
    }
    // `SSfastprocess` ticks (0.2 s) of this `Life()`.
    let ticks = (secs / 0.2).round().max(1.0) as u32;
    for _ in 0..ticks {
        let Some((at, _)) = trauma.extra.phantom else {
            break;
        };
        if rng.chance(40.0) {
            continue;
        }
        move_phantom(world, trauma, step_towards(at, pos));
        if rng.chance(5.0) {
            say_to(
                world,
                mob,
                Span::Name,
                "Officer Beepsky exclaims, \"Level 10 infraction alert!\"",
            );
        }
    }
}

/// `/datum/brain_trauma/special/ptsd/on_life`: a battle hallucination every 10 s to 10 min.
fn ptsd(world: &mut World, mob: Entity, trauma: &mut Trauma, rng: &mut CombatRng) {
    use crate::hallucination::Kind as H;
    use crate::hallucination::kinds::{Battle, Snd};
    let now = now(world);
    if out_or_crit(world, mob) || now < trauma.extra.next_at {
        return;
    }
    let list = [
        H::Sound(Snd::Boom),
        H::Sound(Snd::DistantBoom),
        H::Battle(Battle::Disabler),
        H::Battle(Battle::Laser),
        H::Battle(Battle::Bomb),
        H::Battle(Battle::ESword),
        H::Battle(Battle::HarmBaton),
        H::Battle(Battle::StunProd),
    ];
    let kind = list[rng.range(0, list.len() as u32 - 1) as usize];
    crate::hallucination::cause_hallucination(
        world,
        mob,
        kind,
        "Caused by The Combat PTSD brain trauma",
    );
    trauma.extra.next_at = now + f64::from(rng.range(10, 600));
}

/// `/datum/brain_trauma/magic/lumiphobia/on_life`: light burns 1.5 a second.
fn lumiphobia(world: &mut World, mob: Entity, trauma: &mut Trauma, secs: f32) {
    if world.get::<crate::closets::InCloset>(mob).is_some()
        || world.get::<crate::inventory::CarriedBy>(mob).is_some()
        || !world.contains_resource::<ss13_world::SpatialIndex>()
    {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let lit = crate::antag::nightmare::is_lit(crate::botany::lumcount(world, pos), LIGHT_THRESHOLD);
    if !lit {
        return;
    }
    let now = now(world);
    if now >= trauma.extra.next_at {
        say_to(world, mob, Span::Warning, "<b>Свет обжигает вас!</b>");
        trauma.extra.next_at = now + 10.0;
    }
    super::diseases::damage(world, mob, DamageType::Burn, 1.5 * secs);
}

/// `/datum/brain_trauma/magic/poltergeist/on_life`: 2% a second the item in view (5) with the
/// most throwforce flies at the owner.
fn poltergeist(world: &mut World, mob: Entity, rng: &mut CombatRng, secs: f32) {
    if !spt_prob(rng, 2.0, secs) {
        return;
    }
    let Some(pos) = pos_of(world, mob) else {
        return;
    };
    if let Some(item) = poltergeist_pick(world, mob) {
        crate::throw::throw_at(world, item, pos, 8, 2.0);
    }
}

/// The poltergeist's pick: unanchored items on the floor in view, the highest `throwforce`
/// first (the first seen among equals).
pub fn poltergeist_pick(world: &World, mob: Entity) -> Option<Entity> {
    let mut best: Option<(Entity, f32)> = None;
    for e in super::fear::in_view(world, mob, 5) {
        let Some(item) = world.get::<DmItem>(e) else {
            continue;
        };
        if item.anchored || world.get::<crate::inventory::CarriedBy>(e).is_some() {
            continue;
        }
        if best.is_none_or(|(_, force)| item.throwforce > force) {
            best = Some((e, item.throwforce));
        }
    }
    best.map(|(e, _)| e)
}

/// `/datum/brain_trauma/magic/stalker/on_life`.
fn stalker(
    world: &mut World,
    mob: Entity,
    trauma: &mut Trauma,
    cosmic: bool,
    rng: &mut CombatRng,
    secs: f32,
) {
    if out_or_crit(world, mob) {
        return;
    }
    let Some(pos) = pos_of(world, mob) else {
        return;
    };
    let look = if cosmic { STAR_GAZER } else { PHANTOM };
    // Not even nullspace will keep it at bay.
    if phantom_distance(world, mob, trauma).is_none() {
        spawn_phantom(world, mob, trauma, look, rng);
    }
    if phantom_distance(world, mob, trauma).is_some_and(|d| d <= 1) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, DEMON_ATTACK, 50.0, true),
        );
        let name = super::name_of(world, mob);
        super::visible(
            world,
            mob,
            "Призрачные когти разрывают ваше тело на части!",
            &format!("{name} разорвало на части невидимыми когтями!"),
            Span::Warning,
        );
        let zone = crate::zone::BodyZone::ALL[rng.range(0, 5) as usize];
        let amount = rng.range(20, 45) as f32;
        super::hurt_limb(world, mob, zone, DamageType::Brute, amount);
    } else if spt_prob(rng, 30.0, secs)
        && let Some((at, _)) = trauma.extra.phantom
    {
        move_phantom(world, trauma, step_towards(at, pos));
    }
    let close = phantom_distance(world, mob, trauma).is_some_and(|d| d <= 8);
    if close && !trauma.extra.close {
        play_local(world, mob, None, SLOWBEAT, 40.0);
    }
    trauma.extra.close = close;
}

/// Whether `mob` gets its magic resistance from a trauma: antimagic (`TRAIT_ANTIMAGIC`),
/// godwoken (`TRAIT_HOLY`).
pub fn antimagic_flags(world: &World, mob: Entity) -> u32 {
    let mut flags = 0;
    if super::traumas::has(world, mob, Kind::Antimagic) {
        flags |= crate::spells::MAGIC_RESISTANCE;
    }
    if super::traumas::has(world, mob, Kind::Godwoken) {
        flags |= crate::spells::MAGIC_RESISTANCE_HOLY;
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phantoms_walk_straight_at_their_victim() {
        let from = TilePos::new(10, 4, 1);
        assert_eq!(
            step_towards(from, TilePos::new(2, 4, 1)),
            TilePos::new(9, 4, 1)
        );
        assert_eq!(
            step_towards(from, TilePos::new(12, 9, 1)),
            TilePos::new(11, 5, 1)
        );
        let mut rng = CombatRng(1);
        let corner = random_corner(from, &mut rng);
        assert_eq!(corner.chebyshev(from), Some(12));
    }
}
