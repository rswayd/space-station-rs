//! Space fauna (server): statues and mannequins (`space_fauna/statue/`), the regal rat
//! (`space_fauna/regal_rat/`) and the changeling's creatures (`space_fauna/changeling/`). Their
//! trees are in `crate::mobs::ai::trees` (`space`).
//!
//! - Statues (and mannequins, which are statues): `unobserved_actor` with
//!   `NO_OBSERVED_MOVEMENT | NO_OBSERVED_ATTACKS`: while a living player that is not blind and not
//!   itself a statue (`TRAIT_UNOBSERVANT`) sees it within 7 tiles on a lit tile (static lumcount
//!   0.1 or more), it can neither step nor attack ([`Observed`], kept up every tick). The AI
//!   smashes lit lights in view and claws whoever it finds. Players get "Flicker Lights"
//!   (every lit fixture within 14 flickers, 30 s) and "Blindness" (everyone else within 14 is
//!   blind for 8 s, 1 min). Frosty leaves a geladikinesis injector.
//! - Regal rat: "Rat King's Domain" (toggle: every 6 s 4 moles of miasma at 20 °C on its tile
//!   and dirt, vomit or slippery oil on it or an open neighbour) and "Raise Army" (8 s: every
//!   living mouse, cockroach, frog and snail it sees within 5 becomes its minion type - a rat,
//!   a sewer roach, a crazy frog, an angry snail - its friend of its factions that obeys
//!   stay/free/follow/attack). Its AI keeps the domain on, runs from whoever hurt it raising
//!   the army and biting back. A player gets a random title ("Король Сыра, Великий") and a
//!   moniker on examine; it heals 1 a `Life` in air of 25% miasma or more; it does not bite its
//!   own subjects or the dead. `/controlled` asks the ghosts for a player at once.
//! - Headslug: its bite on a dead carbon (not a monkey, not an alien host) lays a changeling egg
//!   (once) and the slug dies 10 s later. The egg hatches 4 minutes later: a monkey bursts out
//!   of the gibbed host and, when the slug is dead, its player becomes it, a changeling.
//! - Flesh spider: its bite injects 5 u of hunter spider toxin; 3 s after its last hurt it
//!   regenerates 15 brute a second; it leaves bubblegum blood on 5% of its steps.
// shortcut: no observed balloon alert or turning lock; no rat spit (licking food), cheese
// eating, door prying, poster tearing or ventcrawl toggles of the rat abilities; the headslug's
// egg cannot be cut out (no `removal_time`) and gives no DNA profile or lesser form; the flesh
// spider lays no webs (no web abilities are ported) and has no stamina or `web_walker` speed.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_atmos::{Atmos, GasId};
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::mobs::MobDef;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};
use ss13_tgui::server::{UiActed, UiSystems};
use ss13_world::{Moved, SpatialIndex};

use super::ai::behaviors::{DELAY_FAIL, DELAY_SUCCEED, FAIL, Perform, SUCCEED, can_see};
use super::ai::{AiController, AiCtx, Key, Value};
use super::{BasicMob, Faction};
use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::combat::CombatRng;
use crate::health::{DEFAULT_MESSAGE_RANGE, DamageType, Health, Stat};
use crate::server::PlayerMob;
use crate::tools::{now, tell_mob};

pub const STATUE: &str = "/mob/living/basic/statue";
const FROSTY: &str = "/mob/living/basic/statue/frosty";
pub const REGAL_RAT: &str = "/mob/living/basic/regal_rat";
const REGAL_RAT_CONTROLLED: &str = "/mob/living/basic/regal_rat/controlled";
pub const HEADSLUG: &str = "/mob/living/basic/headslug";
const HEADSLUG_BEAKLESS: &str = "/mob/living/basic/headslug/beakless";
pub const FLESH_SPIDER: &str = "/mob/living/basic/flesh_spider";

/// What they leave or spread (`GAME_RUNTIME_TYPES`).
pub const GELADIKINESIS: &str = "/obj/item/dnainjector/geladikinesis";
const DIRT: &str = "/obj/effect/decal/cleanable/dirt";
const VOMIT: &str = "/obj/effect/decal/cleanable/vomit";
const OLD_VOMIT: &str = "/obj/effect/decal/cleanable/vomit/old";
const SLIPPERY_OIL: &str = "/obj/effect/decal/cleanable/blood/oil/slippery";
const BUBBLEGUM_BLOOD: &str = "/obj/effect/decal/cleanable/blood/bubblegum";
const HUNTER_TOXIN: &str = "/datum/reagent/toxin/hunterspider";

/// `unobserved_actor/can_be_seen`: `oview(7)`, `check_lumcount_below(0.1)`.
const OBSERVE_RANGE: u32 = 7;
const DARK_LUMCOUNT: f32 = 0.1;
/// `flicker_lights`, `blindness`: `aoe_radius`, `cooldown_time`; the blindness.
const STATUE_SPELL_RADIUS: u32 = 14;
const FLICKER_COOLDOWN: f64 = 30.0;
const BLIND_COOLDOWN: f64 = 60.0;
const BLIND_SECS: f32 = 8.0;
/// `domain`: `mess_interval`, the toggle's cooldown; `riot`: `cooldown_time`, `range`.
const MESS_INTERVAL: f64 = 6.0;
const DOMAIN_COOLDOWN: f64 = 1.0;
const RIOT_COOLDOWN: f64 = 8.0;
const RIOT_RANGE: u32 = 5;
const RIOT_SOUND: &str = "sound/mobs/non-humanoids/mouse/mousesqueek.ogg";
/// The regal rat's `handle_environment`: miasma share that heals, and by how much a `Life`.
const MIASMA_HEAL_SHARE: f64 = 0.25;
const MIASMA_HEAL: f32 = 1.0;
const LIFE_SECS: f64 = 2.0;
/// `addtimer(death, 10 SECONDS)`, `EGG_INCUBATION_TIME`.
const SLUG_DEATH_SECS: f64 = 10.0;
const EGG_INCUBATION: f64 = 240.0;
/// The flesh spider's `regenerator` (`regeneration_delay`, `maxHealth / 6`), `blood_walk`
/// chance and venom.
const REGEN_DELAY: f64 = 3.0;
const REGEN_PER_SEC: f32 = 90.0 / 6.0;
const BLOOD_WALK_CHANCE: f32 = 5.0;
const VENOM_UNITS: f32 = 5.0;

const AI_ACTIONS_DMI: &str = "icons/mob/actions/actions_AI.dmi";
const SPELLS_DMI: &str = "icons/mob/actions/actions_spells.dmi";
const ANIMAL_DMI: &str = "icons/mob/actions/actions_animal.dmi";
const BACKGROUNDS_DMI: &str = "icons/mob/actions/backgrounds.dmi";
const FLICKER_ID: &str = "statue:flicker";
const BLIND_ID: &str = "statue:blindness";
const DOMAIN_ID: &str = "regal_rat:domain";
const RIOT_ID: &str = "regal_rat:riot";

/// A statue's `unobserved_actor` and spell cooldowns.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Statue {
    flicker_ready: f64,
    blind_ready: f64,
}

/// The statue is being seen: it can neither move nor attack.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Observed;

/// The regal rat's abilities and title.
#[derive(Component, Clone, Debug, Default)]
pub struct RegalRat {
    domain: bool,
    next_mess: f64,
    domain_ready: f64,
    riot_ready: f64,
    moniker: String,
}

/// A headslug's `egg_lain` and its coming death.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Headslug {
    egg_lain: bool,
    dies_at: Option<f64>,
}

/// `/obj/item/organ/body_egg/changeling_egg` in a corpse: hatches then; `origin` is the slug.
#[derive(Component, Clone, Copy, Debug)]
pub struct ChangelingEgg {
    hatch_at: f64,
    origin: Entity,
}

/// The flesh spider's `regenerator`: its health last seen and when it was last hurt.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Regenerator {
    last_health: f32,
    hurt_at: f64,
}

#[derive(Resource, Default)]
struct FaunaClock {
    life: f64,
}

pub(super) fn build(app: &mut App) {
    app.init_resource::<FaunaClock>()
        .add_systems(
            Update,
            (
                update_observed,
                (fauna_tick, blood_walk, logins).after(crate::health::HealthSystems),
            ),
        )
        .add_systems(Update, trigger_buttons.in_set(UiSystems::Handle))
        .add_systems(
            Update,
            fauna_clicks
                .after(crate::click::ResolveClicks)
                .before(crate::health::HealthSystems),
        );
}

/// A player's statue, regal rat, headslug, flesh spider, gorilla or bee bites the living it
/// clicks (`UnarmedAttack` -> `melee_attack`).
fn fauna_clicks(world: &mut World, mut clicks: Local<MessageCursor<crate::click::ClickResolved>>) {
    let clicks: Vec<crate::click::ClickResolved> = clicks
        .read(world.resource::<Messages<crate::click::ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        let (mob, target) = (click.mob, click.target);
        let ours = world.get_entity(mob).is_ok_and(|e| {
            e.contains::<Statue>()
                || e.contains::<RegalRat>()
                || e.contains::<Headslug>()
                || e.contains::<Regenerator>()
                || e.contains::<super::gorilla::Gorilla>()
                || e.contains::<super::bees::Bee>()
        });
        if !ours
            || mob == target
            || click.modifiers.right
            || world.get::<Stat>(mob) != Some(&Stat::Conscious)
            || world.get::<Health>(target).is_none()
        {
            continue;
        }
        super::defense::melee_attack(world, mob, target);
    }
}

/// The space fauna parts of a new basic mob (`Initialize`).
pub(super) fn attach(world: &mut World, mob: Entity, def: &MobDef) {
    let path = def.path.as_str();
    if is_under(path, STATUE) {
        world.entity_mut(mob).insert(Statue::default());
    } else if is_under(path, REGAL_RAT) {
        world.entity_mut(mob).insert(RegalRat::default());
        if is_under(path, REGAL_RAT_CONTROLLED) {
            poll_rat_ghosts(world, mob);
        }
    } else if is_under(path, HEADSLUG) {
        world.entity_mut(mob).insert(Headslug {
            egg_lain: is_under(path, HEADSLUG_BEAKLESS),
            dies_at: None,
        });
    } else if is_under(path, FLESH_SPIDER) {
        world.entity_mut(mob).insert(Regenerator {
            last_health: def.health,
            hurt_at: 0.0,
        });
    }
}

/// `death_drops` (Frosty's injector).
pub(super) fn on_death(world: &mut World, def: &MobDef, pos: TilePos) {
    if is_under(&def.path, FROSTY) {
        crate::construction::spawn_obj(world, GELADIKINESIS, pos);
    }
}

/// The examine lines of the headslug and the regal rat.
pub fn examine_lines(world: &World, mob: Entity, lines: &mut Vec<crate::examine::Line>) {
    if let Some(slug) = world.get::<Headslug>(mob) {
        if world.get::<Stat>(mob) != Some(&Stat::Dead) {
            lines.push(if world.get::<PlayerMob>(mob).is_some() {
                (Span::Warning, "Он двигается осмысленно!".to_owned())
            } else {
                (
                    Span::Notice,
                    "Кажется, что он передвигается бесцельно.".to_owned(),
                )
            });
        }
        if slug.egg_lain {
            lines.push((
                Span::Notice,
                "Его репродуктивный орган, похоже, иссяк.".to_owned(),
            ));
        }
    }
    if let Some(rat) = world.get::<RegalRat>(mob)
        && !rat.moniker.is_empty()
    {
        lines.push((Span::Plain, rat.moniker.clone()));
    }
}

fn roll(world: &mut World, percent: f32) -> bool {
    world.resource_mut::<CombatRng>().chance(percent)
}

fn pick<'a>(world: &mut World, list: &[&'a str]) -> &'a str {
    let i = world
        .resource_mut::<CombatRng>()
        .range(0, list.len().saturating_sub(1) as u32) as usize;
    list[i]
}

fn visible(world: &mut World, pos: TilePos, span: Span, text: String) {
    let clients = crate::obj_damage::clients_in_range(world, pos, DEFAULT_MESSAGE_RANGE, &[]);
    world.write_message(ss13_net::chat::server::SendChat {
        clients,
        message: ChatMessage::simple(ChatChannel::Info, span, text),
    });
}

/// `emote(key)` of a basic mob: the emote's text seen around, its sound.
pub fn run_emote(world: &mut World, mob: Entity, key: &str) {
    let Some(emote) = ss13_chat::emote::find(key) else {
        return;
    };
    super::defense::emote(world, mob, emote.message);
    if !emote.sounds.is_empty()
        && let Some(&pos) = world.get::<TilePos>(mob)
    {
        let sound = pick(world, emote.sounds);
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, sound, 50.0, emote.vary),
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Statues

/// Whether the statue `mob` is seen now (it may not move or attack).
pub fn observed(world: &World, mob: Entity) -> bool {
    world.get::<Observed>(mob).is_some()
}

/// `unobserved_actor/can_be_seen`.
fn can_be_seen(world: &World, mob: Entity) -> bool {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return false;
    };
    if crate::botany::lumcount(world, pos).is_some_and(|l| l < DARK_LUMCOUNT) {
        return false;
    }
    world
        .resource::<SpatialIndex>()
        .in_radius(pos, OBSERVE_RANGE)
        .any(|e| {
            e != mob
                && world.get::<PlayerMob>(e).is_some()
                && world.get::<Health>(e).is_some()
                && world.get::<Stat>(e).is_some_and(|s| *s != Stat::Dead)
                && world.get::<Statue>(e).is_none()
                && !crate::vision::is_blind(world, e)
                && world
                    .get::<TilePos>(e)
                    .is_some_and(|&from| can_see(world, from, pos, OBSERVE_RANGE))
        })
}

/// Keeps [`Observed`] on the statues that are seen.
// Cost: per tick, per statue (a handful at most) one radius-7 index scan.
fn update_observed(world: &mut World) {
    let statues: Vec<(Entity, bool)> = world
        .query_filtered::<(Entity, Has<Observed>), With<Statue>>()
        .iter(world)
        .collect();
    for (statue, was) in statues {
        let seen =
            world.get::<Stat>(statue) == Some(&Stat::Conscious) && can_be_seen(world, statue);
        if seen != was {
            let mut e = world.entity_mut(statue);
            if seen {
                e.insert(Observed);
            } else {
                e.remove::<Observed>();
            }
        }
    }
}

/// The statue AI's `unbroken_light` target within 7 tiles in view.
pub(crate) fn acquire_light(ctx: &mut AiCtx, key: Key) -> Perform {
    const VISION: u32 = 7;
    let Some(from) = ctx.pawn_pos() else {
        return DELAY_FAIL;
    };
    let world = &*ctx.world;
    let mut lights: Vec<(u32, Entity)> = world
        .resource::<SpatialIndex>()
        .in_radius(from, VISION)
        .filter(|&e| {
            world
                .get::<crate::power::LightFixture>(e)
                .is_some_and(|f| f.status != crate::power::lighting::LightStatus::Broken)
        })
        .filter_map(|e| {
            let to = *world.get::<TilePos>(e)?;
            can_see(world, from, to, VISION).then(|| (from.chebyshev(to).unwrap_or(0), e))
        })
        .collect();
    lights.sort_by_key(|&(d, e)| (d, e.to_bits()));
    match lights.first() {
        Some(&(_, light)) => {
            ctx.bb.set(key, Value::Entity(light));
            DELAY_SUCCEED
        }
        None => DELAY_FAIL,
    }
}

/// `hunt_target/interact_with_target` on a light: the statue's blow (`obj_damage`) on it.
pub(crate) fn smash_light(ctx: &mut AiCtx, key: Key) -> Perform {
    let Some(light) = ctx.bb.entity(key) else {
        return FAIL;
    };
    let (Some(from), Some(&to)) = (ctx.pawn_pos(), ctx.world.get::<TilePos>(light)) else {
        return FAIL;
    };
    if from.chebyshev(to).is_none_or(|d| d > 1) || observed(ctx.world, ctx.pawn) {
        return FAIL;
    }
    let damage = ctx
        .world
        .get::<BasicMob>(ctx.pawn)
        .map_or(0.0, |b| b.def.obj_damage);
    crate::power::lighting::take_damage(ctx.world, light, damage);
    SUCCEED
}

/// `/datum/action/cooldown/spell/aoe/flicker_lights`.
fn flicker_lights(world: &mut World, statue: Entity) {
    let Some(&pos) = world.get::<TilePos>(statue) else {
        return;
    };
    let lights: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(pos, STATUE_SPELL_RADIUS)
        .filter(|&e| {
            world
                .get::<crate::power::LightFixture>(e)
                .is_some_and(|f| f.mode == crate::power::LightMode::On)
        })
        .collect();
    for light in lights {
        crate::power::lighting::flicker(world, light, 1);
    }
}

/// `/datum/action/cooldown/spell/aoe/blindness`.
fn blindness(world: &mut World, statue: Entity) {
    let Some(&pos) = world.get::<TilePos>(statue) else {
        return;
    };
    let name = super::defense::name_of(world, statue);
    visible(
        world,
        pos,
        Span::Danger,
        format!("{name} glares their eyes."),
    );
    let victims: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(pos, STATUE_SPELL_RADIUS)
        .filter(|&e| e != statue && world.get::<Health>(e).is_some())
        .collect();
    for victim in victims {
        crate::vision::adjust_temp_blindness(world, victim, BLIND_SECS);
    }
}

// ---------------------------------------------------------------------------------------------
// The regal rat

/// What a mob becomes when a regal rat riots it (`regal_rat_minion` `converted_path`).
pub fn minion_path(path: &str) -> Option<&'static str> {
    const TABLE: &[(&str, Option<&str>)] = &[
        ("/mob/living/basic/cockroach/glockroach/mobroach/goon", None),
        (
            "/mob/living/basic/cockroach/glockroach/mobroach",
            Some("/mob/living/basic/cockroach/glockroach/mobroach/goon"),
        ),
        ("/mob/living/basic/cockroach/glockroach/gang", None),
        (
            "/mob/living/basic/cockroach/glockroach",
            Some("/mob/living/basic/cockroach/glockroach/gang"),
        ),
        ("/mob/living/basic/cockroach/hauberoach/imperial", None),
        (
            "/mob/living/basic/cockroach/hauberoach",
            Some("/mob/living/basic/cockroach/hauberoach/imperial"),
        ),
        ("/mob/living/basic/cockroach/sewer", None),
        (
            "/mob/living/basic/cockroach",
            Some("/mob/living/basic/cockroach/sewer"),
        ),
        ("/mob/living/basic/frog/crazy", None),
        ("/mob/living/basic/frog/suicide", None),
        (
            "/mob/living/basic/frog/rare",
            Some("/mob/living/basic/frog/crazy/rare"),
        ),
        (
            "/mob/living/basic/frog",
            Some("/mob/living/basic/frog/crazy"),
        ),
        ("/mob/living/basic/snail/angry", None),
        (
            "/mob/living/basic/snail",
            Some("/mob/living/basic/snail/angry"),
        ),
        (
            "/mob/living/basic/mouse",
            Some("/mob/living/basic/mouse/rat"),
        ),
    ];
    TABLE
        .iter()
        .find(|(root, _)| is_under(path, root))
        .and_then(|(_, to)| *to)
}

/// `GLOB.regal_rat_minion_commands` that are ported (`protect_owner` is not).
const MINION_COMMANDS: &[super::pets::PetCommand] = &[
    super::pets::PetCommand::Stay,
    super::pets::PetCommand::Free,
    super::pets::PetCommand::Follow,
    super::pets::PetCommand::Attack,
];

/// `/datum/action/cooldown/mob_cooldown/riot/riot`: every minion type in view within 5 turns.
pub fn riot(world: &mut World, rat: Entity) {
    let Some(&pos) = world.get::<TilePos>(rat) else {
        return;
    };
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, RIOT_SOUND, 150.0, false),
    );
    let minions: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(pos, RIOT_RANGE)
        .filter(|&e| {
            e != rat
                && world.get::<Stat>(e).is_some_and(|s| *s != Stat::Dead)
                && world
                    .get::<BasicMob>(e)
                    .is_some_and(|b| minion_path(&b.def.path).is_some())
                && world
                    .get::<TilePos>(e)
                    .is_some_and(|&p| can_see(world, pos, p, RIOT_RANGE))
        })
        .collect();
    for minion in minions {
        convert_minion(world, minion, rat);
    }
}

/// `regal_rat_minion/do_conversion`: `change_mob_type`, the commands, the rat as friend and
/// its factions.
fn convert_minion(world: &mut World, minion: Entity, master: Entity) {
    let Some(old) = world.get::<BasicMob>(minion).map(|b| b.def.clone()) else {
        return;
    };
    let (Some(path), Some(&pos)) = (minion_path(&old.path), world.get::<TilePos>(minion)) else {
        return;
    };
    // `minion.name == initial(minion.name) ? result::name : minion.name`.
    let renamed = world
        .get::<Name>(minion)
        .map(|n| n.as_str().to_owned())
        .filter(|n| *n != old.name);
    let Some(new) = super::spawn::spawn_mob(world, path, pos) else {
        return;
    };
    if let Some(name) = renamed {
        world.entity_mut(new).insert(Name::new(name));
    }
    if world.get::<PlayerMob>(minion).is_some() {
        crate::medical::brainmob::transfer_mind(world, minion, new);
    }
    world.despawn(minion);
    give_pet_commands(world, new, MINION_COMMANDS);
    world.entity_mut(new).remove::<super::pets::Tameable>();
    super::pets::befriend(world, new, master);
    if let Some(faction) = world.get::<Faction>(master).cloned() {
        world.entity_mut(new).insert(faction);
    }
}

/// `AddComponent(/datum/component/obeys_commands)` after `Initialize`: the commands and the
/// pet command subtree at the head of its tree.
fn give_pet_commands(world: &mut World, mob: Entity, commands: &'static [super::pets::PetCommand]) {
    world.entity_mut(mob).insert(super::pets::ObeysCommands {
        commands,
        follow_on_befriend: false,
    });
    let Some(def) = world.get::<BasicMob>(mob).map(|b| b.def.clone()) else {
        return;
    };
    if super::pets::has_pet_subtree(&def.path) {
        return;
    }
    let Some(mut built) = super::ai::trees::build(&def.ai_controller, &def) else {
        return;
    };
    built.roots.insert(0, super::ai::trees::pet_command_root());
    if let Some(mut controller) = world.get_mut::<AiController>(mob) {
        controller.tree = super::ai::bt::Tree::new(built.roots);
    }
}

/// `domain/set_domain_active`.
fn set_domain(world: &mut World, rat: Entity, active: bool) {
    let t = now(world);
    let Some(mut r) = world.get_mut::<RegalRat>(rat) else {
        return;
    };
    if r.domain == active {
        return;
    }
    r.domain = active;
    if active {
        r.next_mess = r.next_mess.max(t);
    }
    refresh_buttons(world, rat);
}

/// `domain/spread_domain`: miasma and a mess.
fn spread_domain(world: &mut World, rat: Entity) {
    let Some(&pos) = world.get::<TilePos>(rat) else {
        return;
    };
    if let Some(mut atmos) = world.get_resource_mut::<Atmos>() {
        let _ = atmos.spawn_air(pos, "miasma=4;TEMP=293.15");
    }
    // `list(our_location) + get_adjacent_open_turfs(owner)`.
    let mut spots = vec![pos];
    if let Some(atmos) = world.get_resource::<Atmos>() {
        spots.extend(
            ss13_shared::Dir::CARDINALS
                .into_iter()
                .map(|d| pos.step(d))
                .filter(|&p| atmos.air(p).is_some()),
        );
    }
    let at = spots[world
        .resource_mut::<CombatRng>()
        .range(0, spots.len() as u32 - 1) as usize];
    let mess = match world.resource_mut::<CombatRng>().range(1, 10) {
        8 => VOMIT,
        9 => OLD_VOMIT,
        10 => SLIPPERY_OIL,
        _ => DIRT,
    };
    crate::construction::spawn_obj(world, mess, at);
}

/// The rat AI's `targeted_mob_ability` of the riot, when it is off cooldown.
pub(crate) fn rat_riot_leaf(ctx: &mut AiCtx) -> Perform {
    let ready = ctx
        .world
        .get::<RegalRat>(ctx.pawn)
        .is_some_and(|r| ctx.now >= r.riot_ready);
    if !ready {
        return FAIL;
    }
    use_riot(ctx.world, ctx.pawn);
    DELAY_SUCCEED
}

/// `use_mob_ability/domain`: switches the domain on when it is off.
pub(crate) fn rat_domain_leaf(ctx: &mut AiCtx) -> Perform {
    let Some(r) = ctx.world.get::<RegalRat>(ctx.pawn) else {
        return FAIL;
    };
    if r.domain || ctx.now < r.domain_ready {
        return DELAY_FAIL;
    }
    use_domain(ctx.world, ctx.pawn);
    DELAY_SUCCEED
}

fn use_riot(world: &mut World, rat: Entity) {
    let t = now(world);
    if let Some(mut r) = world.get_mut::<RegalRat>(rat) {
        r.riot_ready = t + RIOT_COOLDOWN;
    }
    riot(world, rat);
    refresh_buttons(world, rat);
}

fn use_domain(world: &mut World, rat: Entity) {
    let t = now(world);
    let Some(active) = world.get::<RegalRat>(rat).map(|r| r.domain) else {
        return;
    };
    if let Some(mut r) = world.get_mut::<RegalRat>(rat) {
        r.domain_ready = t + DOMAIN_COOLDOWN;
    }
    set_domain(world, rat, !active);
}

/// `grant_titles`.
fn grant_titles(world: &mut World, rat: Entity) {
    const TITLES: &[&str] = &[
        "Боярин",
        "Император",
        "Король",
        "Лорд",
        "Мастер",
        "Повелитель",
        "Принц",
        "Сёгун",
        "Государь",
        "Царь",
    ];
    const KINGDOMS: &[&str] = &[
        "Сыра",
        "Отбросов",
        "Техов",
        "Миазмы",
        "Чумы",
        "Мусора",
        "Вредительства",
    ];
    const DESCRIPTORS: &[&str] = &[
        "Большесырый",
        "Жестокий",
        "Чемпион заблудших существ",
        "Омерзительный",
        "Великий",
        "Серый",
        "Ужасный",
        "Популятор",
        "Могущественный",
        "Тихий",
        "Тщеславный",
    ];
    let title = pick(world, TITLES);
    let kingdom = pick(world, KINGDOMS);
    let descriptor = pick(world, DESCRIPTORS);
    let moniker =
        format!("Крыса {kingdom}. {title}... Да кто вообще дал ему такой титул этой дыры?");
    let mut e = world.entity_mut(rat);
    e.insert(Name::new(format!("{title} {kingdom}, {descriptor}")));
    if let Some(mut r) = e.get_mut::<RegalRat>() {
        r.moniker = moniker;
    }
}

/// `ghost_direct_control` with `poll_candidates` of `/mob/living/basic/regal_rat/controlled`.
fn poll_rat_ghosts(world: &mut World, rat: Entity) {
    use crate::ghost_poll::{self, DEFAULT_POLL_SECS, PollSpec};
    if !world.contains_resource::<ghost_poll::Polls>() {
        return;
    }
    let spec = PollSpec {
        question: "Вы хотите стать королевской крысой?".to_owned(),
        role_name: "Королевская Крыса! Пусть корона тлится сыром Его Величеству!".to_owned(),
        secs: DEFAULT_POLL_SECS,
        ignore: Some("regal_rat"),
        target: Some(rat),
        ..default()
    };
    let group = ghost_poll::ghosts(world);
    ghost_poll::poll_candidates(world, spec, group, move |world, mut signed_up| {
        if world.get::<RegalRat>(rat).is_none()
            || world.get::<Stat>(rat) == Some(&Stat::Dead)
            || world.get::<PlayerMob>(rat).is_some()
        {
            return;
        }
        let Some(chosen) = ghost_poll::pick_candidate(world, &mut signed_up) else {
            return;
        };
        if crate::medical::brainmob::possess(world, chosen.client, chosen.mob, rat) {
            world.despawn(chosen.mob);
            tell_mob(
                world,
                rat,
                Span::BoldNotice,
                "Вы - независимая, дикая сила на станции! Собирайте монеты, мусор, сыр и прочее, укрываясь в темноте!",
            );
        }
    });
}

// ---------------------------------------------------------------------------------------------
// Melee

/// `COMSIG_HOSTILE_PRE_ATTACKINGTARGET` and `early_melee_attack` blocks: a seen statue does not
/// attack; a regal rat spares the dead and its own subjects.
pub fn blocks_melee(world: &World, attacker: Entity, target: Entity) -> bool {
    if observed(world, attacker) {
        return true;
    }
    if world.get::<RegalRat>(attacker).is_some() {
        let dead = world.get::<Stat>(target) == Some(&Stat::Dead);
        let subject = matches!(
            (world.get::<Faction>(attacker), world.get::<Faction>(target)),
            (Some(a), Some(b)) if a == b
        );
        return dead || subject;
    }
    false
}

/// After a landed bite: the headslug's egg, the flesh spider's venom.
pub fn after_melee(world: &mut World, attacker: Entity, target: Entity) {
    if world.get::<Headslug>(attacker).is_some() {
        check_and_implant(world, attacker, target);
    } else if world.get::<Regenerator>(attacker).is_some()
        && world.get::<Stat>(target).is_some_and(|s| *s != Stat::Dead)
    {
        // `venomous` (`INJECT_CHECK_PENETRATE_THICK`: through any clothing).
        crate::medical::cyberimps::add_blood_reagent(world, target, HUNTER_TOXIN, VENOM_UNITS);
    }
}

/// `headslug/check_and_implant`.
fn check_and_implant(world: &mut World, slug: Entity, victim: Entity) {
    let Some(state) = world.get::<Headslug>(slug).copied() else {
        return;
    };
    let carbon = world.get::<crate::human::HumanLook>(victim).is_some();
    if state.egg_lain
        || !carbon
        || crate::transform::is_monkey(world, victim)
        || world.get::<Stat>(victim) != Some(&Stat::Dead)
    {
        return;
    }
    if world
        .get::<crate::antag::xeno::reproduction::AlienEmbryo>(victim)
        .is_some()
        || world.get::<ChangelingEgg>(victim).is_some()
    {
        tell_mob(world, slug, Span::Warning, "носитель уже беременный!");
        return;
    }
    let t = now(world);
    world.entity_mut(victim).insert(ChangelingEgg {
        hatch_at: t + EGG_INCUBATION,
        origin: slug,
    });
    world.entity_mut(slug).insert(Headslug {
        egg_lain: true,
        dies_at: Some(t + SLUG_DEATH_SECS),
    });
    if let Some(&pos) = world.get::<TilePos>(victim) {
        let (who, whom) = (
            super::defense::name_of(world, slug),
            crate::chem::name_of(world, victim),
        );
        visible(
            world,
            pos,
            Span::Warning,
            format!(
                "{} вводит что-то в плоть {whom}!",
                ss13_chat::capitalize(&who)
            ),
        );
    }
    tell_mob(
        world,
        slug,
        Span::Danger,
        "После того как мы отложили яйцо, наша смерть стремительно приближается...",
    );
}

/// `changeling_egg/pop`: a monkey from the gibbed host, the slug's player in it as a
/// changeling.
fn pop_egg(world: &mut World, host: Entity, egg: ChangelingEgg) {
    world.entity_mut(host).remove::<ChangelingEgg>();
    let Some(pos) = crate::explosions::turf_of(world, host) else {
        return;
    };
    let seed = world.resource_mut::<CombatRng>().range(0, u32::MAX);
    let mut look = crate::human::AppearancePrefs::random(u64::from(seed)).body();
    look.species = crate::species::SpeciesId::Monkey;
    let monkey = crate::server::spawn_body(world, pos, "monkey".to_owned(), look);
    // `origin.current.stat == DEAD` (or a ghost): the slug's player takes the monkey.
    let origin_dead = world
        .get::<Stat>(egg.origin)
        .is_none_or(|s| *s == Stat::Dead);
    if origin_dead
        && world.get_entity(egg.origin).is_ok()
        && crate::medical::brainmob::transfer_mind(world, egg.origin, monkey)
    {
        crate::antag::changeling::server::add_changeling(world, monkey);
    }
    info!("{host} was gibbed by a changeling egg burst");
    crate::health::gib(world, host);
}

// ---------------------------------------------------------------------------------------------
// Ticks

/// The rat's domain and miasma, the slugs' deaths, the eggs, the spiders' regeneration.
fn fauna_tick(world: &mut World) {
    let t = now(world);
    let delta = world.resource::<Time>().delta_secs_f64();
    // Regal rats.
    let rats: Vec<(Entity, RegalRat)> = world
        .query::<(Entity, &RegalRat)>()
        .iter(world)
        .map(|(e, r)| (e, r.clone()))
        .collect();
    for (rat, r) in &rats {
        let conscious = world.get::<Stat>(*rat) == Some(&Stat::Conscious);
        if r.domain && !conscious {
            // `update_status_on_signal`: unavailable, off.
            set_domain(world, *rat, false);
            continue;
        }
        if r.domain && t >= r.next_mess {
            spread_domain(world, *rat);
            if let Some(mut r) = world.get_mut::<RegalRat>(*rat) {
                r.next_mess = t + MESS_INTERVAL;
            }
        }
    }
    let life = {
        let mut clock = world.resource_mut::<FaunaClock>();
        clock.life += delta;
        let fired = clock.life >= LIFE_SECS;
        if fired {
            clock.life -= LIFE_SECS;
        }
        fired
    };
    if life {
        for (rat, _) in &rats {
            heal_in_miasma(world, *rat);
        }
    }
    // Headslugs that laid their egg.
    let dying: Vec<Entity> = world
        .query::<(Entity, &Headslug)>()
        .iter(world)
        .filter(|(_, h)| h.dies_at.is_some_and(|at| t >= at))
        .map(|(e, _)| e)
        .collect();
    for slug in dying {
        if let Some(mut h) = world.get_mut::<Headslug>(slug) {
            h.dies_at = None;
        }
        if let Some(mut stat) = world.get_mut::<Stat>(slug) {
            stat.set_if_neq(Stat::Dead);
        }
    }
    let hatching: Vec<(Entity, ChangelingEgg)> = world
        .query::<(Entity, &ChangelingEgg)>()
        .iter(world)
        .filter(|(_, egg)| t >= egg.hatch_at)
        .map(|(e, egg)| (e, *egg))
        .collect();
    for (host, egg) in hatching {
        pop_egg(world, host, egg);
    }
    regenerate(world, t, delta as f32);
}

/// The regal rat's `handle_environment`: `heal_bodypart_damage(1)` in a quarter of miasma.
fn heal_in_miasma(world: &mut World, rat: Entity) {
    let (Some(&pos), Some(&stat)) = (world.get::<TilePos>(rat), world.get::<Stat>(rat)) else {
        return;
    };
    if stat == Stat::Dead {
        return;
    }
    let share = world
        .get_resource::<Atmos>()
        .and_then(|a| a.air(pos))
        .filter(|air| air.total_moles() > 0.0)
        .map_or(0.0, |air| air.get(GasId::MIASMA) / air.total_moles());
    if share >= MIASMA_HEAL_SHARE
        && let Some(mut health) = world.get_mut::<Health>(rat)
    {
        health.adjust(DamageType::Brute, -MIASMA_HEAL);
    }
}

/// The flesh spider's `regenerator`: a hurt restarts the delay; past it, brute heals.
fn regenerate(world: &mut World, t: f64, delta: f32) {
    let spiders: Vec<(Entity, Regenerator, f32, f32, Stat)> = world
        .query::<(Entity, &Regenerator, &Health, &Stat)>()
        .iter(world)
        .map(|(e, r, h, s)| (e, *r, h.health(), h.max, *s))
        .collect();
    for (spider, mut regen, health, max, stat) in spiders {
        if stat == Stat::Dead {
            continue;
        }
        if health < regen.last_health {
            regen.hurt_at = t;
        } else if health < max
            && t - regen.hurt_at >= REGEN_DELAY
            && let Some(mut h) = world.get_mut::<Health>(spider)
        {
            h.adjust(DamageType::Brute, -REGEN_PER_SEC * delta);
        }
        regen.last_health = world.get::<Health>(spider).map_or(health, Health::health);
        world.entity_mut(spider).insert(regen);
    }
}

/// The flesh spider's `blood_walk`.
fn blood_walk(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = cursor
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    for m in moved {
        if world.get::<Regenerator>(m.entity).is_none() || !roll(world, BLOOD_WALK_CHANCE) {
            continue;
        }
        crate::construction::spawn_obj(world, BUBBLEGUM_BLOOD, m.from);
    }
}

// ---------------------------------------------------------------------------------------------
// Buttons

/// A player in a statue or a regal rat gets its buttons (and the rat its title, `on_login`).
fn logins(
    new: Query<Entity, (Added<PlayerMob>, Or<(With<Statue>, With<RegalRat>)>)>,
    mut commands: Commands,
) {
    for mob in &new {
        commands.queue(move |world: &mut World| {
            if world
                .get::<RegalRat>(mob)
                .is_some_and(|r| r.moniker.is_empty())
            {
                grant_titles(world, mob);
            }
            // A fresh component opens the HUD for the new client.
            let Ok(mut e) = world.get_entity_mut(mob) else {
                return;
            };
            e.remove::<ActionButtons>();
            refresh_buttons(world, mob);
        });
    }
}

fn refresh_buttons(world: &mut World, mob: Entity) {
    let icon = |world: &World, dmi: &str, state: &str| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(dmi, state))
    };
    let conscious = world.get::<Stat>(mob) == Some(&Stat::Conscious);
    let button =
        |world: &World, id: &str, name: &str, (dmi, state): (&str, &str), bg: &str| ActionButton {
            id: id.to_owned(),
            name: name.to_owned(),
            icon: icon(world, dmi, state),
            background: icon(world, BACKGROUNDS_DMI, bg),
            border: icon(world, BACKGROUNDS_DMI, &format!("{bg}_border")),
            available: conscious,
            cooldown: 0.0,
            active: false,
            maptext: None,
        };
    let buttons = if let Some(s) = world.get::<Statue>(mob).copied() {
        vec![
            (
                button(
                    world,
                    BLIND_ID,
                    "Blindness",
                    (SPELLS_DMI, "blind"),
                    "bg_spell",
                ),
                s.blind_ready,
            ),
            (
                button(
                    world,
                    FLICKER_ID,
                    "Flicker Lights",
                    (AI_ACTIONS_DMI, "blackout"),
                    "bg_spell",
                ),
                s.flicker_ready,
            ),
        ]
    } else if let Some(r) = world.get::<RegalRat>(mob).cloned() {
        let coffer = if r.domain { "coffer" } else { "coffer_off" };
        vec![
            (
                button(
                    world,
                    DOMAIN_ID,
                    "Rat King's Domain",
                    (ANIMAL_DMI, coffer),
                    "bg_clock",
                ),
                r.domain_ready,
            ),
            (
                button(
                    world,
                    RIOT_ID,
                    "Raise Army",
                    (ANIMAL_DMI, "riot"),
                    "bg_clock",
                ),
                r.riot_ready,
            ),
        ]
    } else {
        return;
    };
    set_buttons(world, mob, buttons);
}

fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if !matches!(id.as_str(), FLICKER_ID | BLIND_ID | DOMAIN_ID | RIOT_ID)
            || actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world.get::<Stat>(actor) != Some(&Stat::Conscious)
        {
            continue;
        }
        let t = now(world);
        match id.as_str() {
            FLICKER_ID => {
                let Some(s) = world.get::<Statue>(actor).copied() else {
                    continue;
                };
                if t < s.flicker_ready {
                    continue;
                }
                world.entity_mut(actor).insert(Statue {
                    flicker_ready: t + FLICKER_COOLDOWN,
                    ..s
                });
                flicker_lights(world, actor);
            }
            BLIND_ID => {
                let Some(s) = world.get::<Statue>(actor).copied() else {
                    continue;
                };
                if t < s.blind_ready {
                    continue;
                }
                world.entity_mut(actor).insert(Statue {
                    blind_ready: t + BLIND_COOLDOWN,
                    ..s
                });
                blindness(world, actor);
            }
            DOMAIN_ID => {
                if world
                    .get::<RegalRat>(actor)
                    .is_none_or(|r| t < r.domain_ready)
                {
                    continue;
                }
                use_domain(world, actor);
            }
            _ => {
                if world
                    .get::<RegalRat>(actor)
                    .is_none_or(|r| t < r.riot_ready)
                {
                    continue;
                }
                use_riot(world, actor);
            }
        }
        refresh_buttons(world, actor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn riot_converts_the_tg_minion_types() {
        assert_eq!(
            minion_path("/mob/living/basic/mouse/gray"),
            Some("/mob/living/basic/mouse/rat")
        );
        assert_eq!(
            minion_path("/mob/living/basic/cockroach"),
            Some("/mob/living/basic/cockroach/sewer")
        );
        assert_eq!(minion_path("/mob/living/basic/cockroach/sewer"), None);
        assert_eq!(
            minion_path("/mob/living/basic/cockroach/glockroach"),
            Some("/mob/living/basic/cockroach/glockroach/gang")
        );
        assert_eq!(minion_path("/mob/living/basic/frog/crazy/rare"), None);
        assert_eq!(
            minion_path("/mob/living/basic/frog/rare"),
            Some("/mob/living/basic/frog/crazy/rare")
        );
        assert_eq!(minion_path("/mob/living/basic/snail/angry"), None);
        assert_eq!(minion_path("/mob/living/basic/carp"), None);
    }
}
