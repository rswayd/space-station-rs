//! The slaughter and laughter demons after tg `/mob/living/basic/demon/slaughter` (`demon.dm`,
//! `demon_subtypes.dm`), their blood crawl (`/datum/action/cooldown/spell/jaunt/bloodcrawl`,
//! `bloodcrawl.dm`: `slaughter_demon` and `slaughter_demon/funny`) and the "Slaughter Demon" heavy
//! midround (`/datum/dynamic_ruleset/midround/from_ghosts/slaughter_demon`, weight 0: admins
//! force it). The antagonist datum and its objectives are the wizard's
//! (`docs/parity/antagonists/wizard.md`).
//!
//! - **Selection** ([`can_spawn`], [`spawn_demons`]): the ghosts polled
//!   ([`ROLE_SLAUGHTER_DEMON`], tg has no `pref_flag`); the demon appears on a carp spawn already in the blood (`phased_mob/blood`), the job, the
//!   greeting and `demon_dies.ogg`.
//! - **Blood Crawl** (button, [`blood_crawl`]): next to blood (a blood decal or a tomato smudge
//!   within a tile) it sinks in at once (`crate::spells::Jaunting`, through walls, no time
//!   limit) and rises out after 2 s on blood. A human it pulls into the blood in crit or dead is
//!   consumed: three 3 s rounds (the demon must stay), the demon fully healed, the victim dead
//!   and gone (the laughter demon keeps it, and its death gives everyone back alive). A
//!   conscious victim kicks free. Every 20 s in the blood without a meal costs 5% of its max
//!   health. Entering or leaving: faster (-1 ds) for 6 s.
//! - **Attacks**: a click bites the living; a right click is the body slam (20 brute, thrown 10
//!   tiles, 45 s cooldown).
//! - **The bottles** ([`use_bottle`], `/obj/item/antag_spawner/slaughter_demon`): shattered on
//!   the station, a willing ghost becomes the demon in the blood where it broke.
//! - **Antagonist** (`/datum/antagonist/slaughter`): the "kill everyone" objective and the
//!   slam hint in the greeting (no summoner: tg's bottles never set one). `/datum/antagonist/imp`
//!   has no port: nothing in tg creates it.
//! - **Death** (`DEL_ON_DEATH`, [`on_death`]): blood, innards and a demon heart are left (the
//!   laughter demon leaves a kitten named Laughter instead).
// shortcut: the bottle takes the first willing ghost at once (tg polls for 5 s); the objective
// is told in chat (no `AntagInfoDemon` window); no hit streak wound bonus, no blood colour
// on leaving, the blood hands of carbon crawlers and the laughter demon's devastation rule; a
// failed meal lets the victim out where the demon is; the revived friends of the laughter demon
// get their ghost back only through `crate::ghost::return_to_body`.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::mobs::MobDef;
use ss13_mapload::spawner::Rng;
use ss13_net::server::ClientToken;
use ss13_net::sound::server::SendSound;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::SoundEvent;
use ss13_shared::{Appearance, Facing, MouseOpacity, TilePos};
use ss13_world::{Mover, SpatialIndex};

use super::dynamic::DynamicRng;
use super::objectives::RoundEnd;
use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::click::ClickResolved;
use crate::combat::NextMove;
use crate::health::{DamageType, Health, Stat};
use crate::server::{PlayerMob, SpawnPoints};
use crate::spells::Jaunting;
use crate::tools::{now, tell, tell_mob};
use ss13_tgui::server::{UiActed, UiSystems};

pub use super::ROLE_SLAUGHTER_DEMON;
pub const SLAUGHTER: &str = "/mob/living/basic/demon/slaughter";
pub const LAUGHTER: &str = "/mob/living/basic/demon/slaughter/laughter";
const JOB: &str = "/datum/job/slaughter_demon";
pub const BLOOD_BOTTLE: &str = "/obj/item/antag_spawner/slaughter_demon";
pub const HUG_BOTTLE: &str = "/obj/item/antag_spawner/slaughter_demon/laughter";
const ROUNDEND_CATEGORY: &str = "Демоны";
/// `grant_loot` of the slaughter demon; the laughter demon's kitten.
pub const BLOOD: &str = "/obj/effect/decal/cleanable/blood";
pub const INNARDS: &str = "/obj/effect/decal/cleanable/blood/innards";
pub const DEMON_HEART: &str = "/obj/item/organ/heart/demon";
pub const KITTEN: &str = "/mob/living/basic/pet/cat/kitten";
const TOMATO: &str = "/obj/effect/decal/cleanable/food/tomato_smudge";

/// `exit_blood_time`, `blood_radius`.
const EXIT_SECS: f64 = 2.0;
const BLOOD_RADIUS: u32 = 1;
/// `consume_victim`: three rounds of 3 s.
const CONSUME_ROUNDS: u8 = 3;
const CONSUME_SECS: f64 = 3.0;
/// `damage_for_lazy_demon`: every 20 s, 5% of the max health.
const LAZY_SECS: f64 = 20.0;
const LAZY_SHARE: f32 = 0.05;
/// `/datum/movespeed_modifier/slaughter` (-1 ds) for 6 s.
const CRAWL_SPEEDUP: f32 = 0.1;
const CRAWL_SPEED_SECS: f64 = 6.0;
/// `bodyslam`: 20 brute, `throw_at(..., 10, 5)`, `slam_cooldown_time`.
const SLAM_DAMAGE: f32 = 20.0;
const SLAM_RANGE: i32 = 10;
const SLAM_COOLDOWN: f64 = 45.0;

const ENTER_SOUND: &str = "sound/effects/magic/enter_blood.ogg";
const EXIT_SOUND: &str = "sound/effects/magic/exit_blood.ogg";
const CONSUME_SOUND: &str = "sound/effects/magic/demon_consume.ogg";
const HORN: &str = "sound/misc/scary_horn.ogg";
const DIES: &str = "sound/effects/magic/demon_dies.ogg";
const MINOR_DMI: &str = "icons/mob/actions/actions_minor_antag.dmi";
const BACKGROUNDS_DMI: &str = "icons/mob/actions/backgrounds.dmi";
const CRAWL_ID: &str = "slaughter:blood_crawl";

/// A meal under way: the round ending at `at`, where the demon must still be.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Meal {
    pub victim: Entity,
    pub round: u8,
    pub at: f64,
    pub pos: TilePos,
}

/// On a player's demon.
#[derive(Component, Clone, Debug)]
pub struct SlaughterDemon {
    pub key: String,
    /// The laughter demon (`funny` crawl).
    pub laughter: bool,
    pub meal: Option<Meal>,
    /// The laughter demon's friends (`consumed_mobs`).
    pub friends: Vec<Entity>,
    pub consume_count: u32,
    /// The next `damage_for_lazy_demon` while crawling.
    pub lazy_at: Option<f64>,
    /// Rising out of the blood at that time, on that tile.
    pub exiting: Option<(f64, TilePos)>,
    pub slam_ready: f64,
    /// The crawl speed-up until then, and the delay without it.
    pub fast_until: f64,
    pub base_delay: f32,
}

pub struct SlaughterPlugin;

impl Plugin for SlaughterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, trigger_buttons.in_set(UiSystems::Handle))
            .add_systems(
                Update,
                demon_clicks
                    .after(crate::click::ResolveClicks)
                    .before(crate::health::HealthSystems),
            )
            .add_systems(Update, tick.before(crate::health::HealthSystems))
            .add_systems(Update, friends_revived.after(crate::health::HealthSystems));
    }
}

fn with_rng<R>(world: &mut World, f: impl FnOnce(&mut Rng) -> R) -> R {
    if !world.contains_resource::<DynamicRng>() {
        world.insert_resource(DynamicRng(Rng::new(0)));
    }
    f(&mut world.resource_mut::<DynamicRng>().0)
}

fn name_of(world: &World, e: Entity) -> String {
    crate::security::name_of(world, e)
}

fn show(world: &mut World, pos: TilePos, span: Span, text: String) {
    for m in crate::pull::visible_message(world, pos, ChatChannel::Warning, (span, text), &[]) {
        world.write_message(m);
    }
}

fn play(world: &mut World, pos: TilePos, sound: &str) {
    crate::sound::playsound(world, crate::sound::PlaySound::new(pos, sound, 50.0, true));
}

// ---------------------------------------------------------------------------------------------
// Selection

/// `can_be_selected`: `find_space_spawn()`.
pub fn can_spawn(world: &mut World) -> bool {
    world
        .get_resource::<SpawnPoints>()
        .is_some_and(|s| !s.carp.is_empty())
}

/// `create_ruleset_body`: the demon in the blood on a carp spawn, for each `(client, ghost)`.
pub fn spawn_demons(world: &mut World, ghosts: &[(Entity, Entity)]) -> Vec<Entity> {
    let mut out = Vec::new();
    for &(client, ghost) in ghosts {
        let tiles = world
            .get_resource::<SpawnPoints>()
            .map(|s| s.carp.clone())
            .unwrap_or_default();
        if tiles.is_empty() {
            break;
        }
        let i = with_rng(world, |rng| rng.range(0, tiles.len() as i64 - 1)) as usize;
        let Some(demon) = crate::mobs::spawn::spawn_mob(world, SLAUGHTER, tiles[i]) else {
            break;
        };
        if !crate::medical::brainmob::possess(world, client, ghost, demon) {
            world.despawn(demon);
            continue;
        }
        world.try_despawn(ghost).ok();
        make_demon(world, demon);
        enter_blood(world, demon, tiles[i]);
        info!("{demon} ({client}) is a slaughter demon at {:?}", tiles[i]);
        out.push(demon);
    }
    out
}

/// `mind_initialize`: the job, the crawl, the greeting.
pub fn make_demon(world: &mut World, mob: Entity) -> bool {
    if world.get::<SlaughterDemon>(mob).is_some() {
        return false;
    }
    let laughter = world
        .get::<crate::mobs::BasicMob>(mob)
        .is_some_and(|b| is_under(&b.def.path, LAUGHTER));
    crate::noncrew_jobs::set_assigned_role(world, mob, JOB);
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let base_delay = world.get::<Mover>(mob).map_or(0.0, |m| m.delay);
    world.entity_mut(mob).insert(SlaughterDemon {
        key,
        laughter,
        meal: None,
        friends: Vec::new(),
        consume_count: 0,
        lazy_at: None,
        exiting: None,
        slam_ready: 0.0,
        fast_until: 0.0,
        base_delay,
    });
    refresh_button(world, mob);
    if let Some(client) = client {
        if world.contains_resource::<Messages<SendSound>>() {
            world.write_message(SendSound {
                client,
                sound: SoundEvent {
                    file: DIES.into(),
                    volume: 100.0,
                    speed: 1.0,
                    offset: None,
                },
            });
        }
        tell(
            world,
            client,
            Span::Bold,
            "You are currently not currently in the same plane of existence as the station. Use your Blood Crawl ability near a pool of blood to manifest and wreak havoc.",
        );
        tell(world, client, Span::Bold, objective_text(laughter));
        tell(
            world,
            client,
            Span::Warning,
            "You have a powerful alt-attack that slams people backwards that you can activate by right-clicking your target!",
        );
    }
    true
}

/// `forge_objectives` without a summoner.
pub fn objective_text(laughter: bool) -> &'static str {
    if laughter {
        "Hug and tickle everyone."
    } else {
        "Kill everyone."
    }
}

/// `/obj/item/antag_spawner/slaughter_demon/attack_self`: true when `item` is a bottle.
pub fn use_bottle(world: &mut World, mob: Entity, item: Entity) -> bool {
    let Some(path) =
        crate::construction::path_of(world, item).filter(|p| is_under(p, BLOOD_BOTTLE))
    else {
        return false;
    };
    let Some(pos) = crate::explosions::turf_of(world, mob) else {
        return true;
    };
    if !crate::events::controller::station_levels(world).contains(&pos.z) {
        tell_mob(
            world,
            mob,
            Span::Warning,
            "You should probably wait until you reach the station.",
        );
        return true;
    }
    let laughter = is_under(&path, HUG_BOTTLE);
    let mut ghosts: Vec<(Entity, Entity)> = world
        .query_filtered::<(Entity, &PlayerMob), With<crate::ghost::Ghost>>()
        .iter(world)
        .map(|(g, p)| (g, p.client))
        .collect();
    ghosts.sort();
    let chosen = ghosts.into_iter().find(|&(_, c)| {
        world
            .get::<super::dynamic::AntagPreferences>(c)
            .is_some_and(|p| p.wants(ROLE_SLAUGHTER_DEMON))
    });
    let spawned = chosen.and_then(|(ghost, client)| {
        let demon =
            crate::mobs::spawn::spawn_mob(world, if laughter { LAUGHTER } else { SLAUGHTER }, pos)?;
        if !crate::medical::brainmob::possess(world, client, ghost, demon) {
            world.despawn(demon);
            return None;
        }
        world.try_despawn(ghost).ok();
        Some(demon)
    });
    let Some(demon) = spawned else {
        tell_mob(
            world,
            mob,
            Span::Warning,
            "The bottle's contents usually pop and boil constantly, but right now they're eerily still and calm. Perhaps you should try again later.",
        );
        return true;
    };
    make_demon(world, demon);
    enter_blood(world, demon, pos);
    tell_mob(
        world,
        mob,
        Span::Notice,
        "You shatter the bottle, no turning back now!",
    );
    tell_mob(
        world,
        mob,
        Span::Warning,
        if laughter {
            "You sense an adorable presence lurking just beyond the veil..."
        } else {
            "You sense a dark presence lurking just beyond the veil..."
        },
    );
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, "sound/effects/glass/glassbr1.ogg", 100.0, true),
    );
    if let Some(mut inventory) = world.get_mut::<crate::inventory::Inventory>(mob) {
        for hand in inventory.hands.iter_mut() {
            if *hand == Some(item) {
                *hand = None;
            }
        }
    }
    world.despawn(item);
    true
}

// ---------------------------------------------------------------------------------------------
// Blood crawl

fn refresh_button(world: &mut World, mob: Entity) {
    let Some(d) = world.get::<SlaughterDemon>(mob) else {
        return;
    };
    let name = if d.laughter {
        "Friendly Blood Crawl"
    } else {
        "Voracious Blood Crawl"
    };
    let icon = |world: &World, dmi: &str, state: &str| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(dmi, state))
    };
    let mut buttons: Vec<(ActionButton, f64)> = world
        .get::<ActionButtons>(mob)
        .map(|b| {
            b.0.iter()
                .filter(|(button, _)| button.id != CRAWL_ID)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    buttons.push((
        ActionButton {
            id: CRAWL_ID.to_owned(),
            name: name.to_owned(),
            icon: icon(world, MINOR_DMI, "bloodcrawl"),
            background: icon(world, BACKGROUNDS_DMI, "bg_demon"),
            border: icon(world, BACKGROUNDS_DMI, "bg_demon_border"),
            available: world.get::<Stat>(mob) == Some(&Stat::Conscious),
            cooldown: 0.0,
            active: world.get::<Jaunting>(mob).is_some(),
            maptext: None,
        },
        0.0,
    ));
    set_buttons(world, mob, buttons);
}

fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if id != CRAWL_ID
            || actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world.get::<SlaughterDemon>(actor).is_none()
            || world.get::<Stat>(actor) != Some(&Stat::Conscious)
        {
            continue;
        }
        blood_crawl(world, actor);
    }
}

/// A blood pool one can crawl in (`can_bloodcrawl_in`): blood decals and tomato smudges.
pub fn crawlable(path: &str) -> bool {
    is_under(path, BLOOD) || is_under(path, TOMATO)
}

/// `find_nearby_blood`: the tile of a pool within a tile of `origin`.
fn nearby_blood(world: &World, origin: TilePos) -> Option<TilePos> {
    let mut pools: Vec<TilePos> = world
        .resource::<SpatialIndex>()
        .in_radius(origin, BLOOD_RADIUS)
        .filter(|&e| crate::construction::path_of(world, e).is_some_and(|p| crawlable(&p)))
        .filter_map(|e| world.get::<TilePos>(e).copied())
        .filter(|p| p.z == origin.z)
        .collect();
    pools.sort_by_key(|p| (p != &origin, p.y, p.x));
    pools.first().copied()
}

/// The button: `do_bloodcrawl` into or out of the nearest pool.
pub fn blood_crawl(world: &mut World, mob: Entity) -> bool {
    let Some(d) = world.get::<SlaughterDemon>(mob).cloned() else {
        return false;
    };
    let Some(pos) = crate::explosions::turf_of(world, mob) else {
        return false;
    };
    let Some(pool) = nearby_blood(world, pos) else {
        tell_mob(
            world,
            mob,
            Span::Warning,
            "There must be a nearby source of blood!",
        );
        return false;
    };
    if world.get::<Jaunting>(mob).is_some() {
        // `try_exit_jaunt`: not mid-meal, 2 s of bubbling.
        if d.meal.is_some() || d.exiting.is_some() {
            tell_mob(world, mob, Span::Warning, "Вы еще не можете выйти!!");
            return false;
        }
        show(
            world,
            pool,
            Span::Warning,
            "The blood starts to bubble...".to_owned(),
        );
        let at = now(world) + EXIT_SECS;
        if let Some(mut d) = world.get_mut::<SlaughterDemon>(mob) {
            d.exiting = Some((at, pool));
        }
        return true;
    }
    let pulled = world.get::<crate::pull::Pulling>(mob).map(|p| p.0);
    enter_blood(world, mob, pool);
    let Some(victim) = pulled.filter(|&v| world.get::<crate::human::HumanLook>(v).is_some()) else {
        return true;
    };
    let victim_name = name_of(world, victim);
    if world.get::<Stat>(victim) == Some(&Stat::Conscious) {
        show(
            world,
            pool,
            Span::Warning,
            format!("{victim_name} kicks free of the blood just before entering it!"),
        );
        return true;
    }
    world.entity_mut(mob).remove::<crate::pull::Pulling>();
    crate::closets::put_mob_inside(world, victim, mob);
    crate::speech::run_emote(world, victim, "scream");
    let user = name_of(world, mob);
    show(
        world,
        pool,
        Span::BoldWarning,
        format!("{user} drags {victim_name} into the blood!"),
    );
    let laughter = d.laughter;
    let text = if laughter {
        format!(
            "You invite {victim_name} to your party! You can not move while you are doing this."
        )
    } else {
        format!("You begin to feast on {victim_name}... You can not move while you are doing this.")
    };
    tell_mob(world, mob, Span::Danger, text);
    play(world, pool, if laughter { HORN } else { CONSUME_SOUND });
    let at = now(world) + CONSUME_SECS;
    if let Some(mut d) = world.get_mut::<SlaughterDemon>(mob) {
        d.meal = Some(Meal {
            victim,
            round: 1,
            at,
            pos: pool,
        });
        // `on_victim_start_consume`: no lazy damage while eating.
        d.lazy_at = None;
    }
    true
}

/// `enter_jaunt` on `pool`'s tile: in the blood, through walls, without a time limit.
fn enter_blood(world: &mut World, mob: Entity, pool: TilePos) {
    let t = now(world);
    let (color, mouse_opacity) = world
        .get::<Appearance>(mob)
        .map_or((None, MouseOpacity::Icon), |a| (a.color, a.mouse_opacity));
    world.entity_mut(mob).insert((
        pool,
        Jaunting {
            start: pool,
            until: f64::INFINITY,
            exit_points: Vec::new(),
            reappear_at: None,
            next_step: t,
            color,
            mouse_opacity,
        },
    ));
    let user = name_of(world, mob);
    show(
        world,
        pool,
        Span::Warning,
        format!("{user} sinks into the blood!"),
    );
    play(world, pool, ENTER_SOUND);
    crate::fire::extinguish_mob(world, mob);
    if let Some(mut d) = world.get_mut::<SlaughterDemon>(mob) {
        d.lazy_at = Some(t + LAZY_SECS);
        d.fast_until = t + CRAWL_SPEED_SECS;
    }
    refresh_button(world, mob);
}

/// `exit_jaunt` + `exit_blood_effect` on the pool's tile.
fn exit_blood(world: &mut World, mob: Entity, pool: TilePos) {
    let t = now(world);
    if world.get::<Jaunting>(mob).is_none() {
        return;
    }
    crate::spells::effects::exit_jaunt(world, mob);
    world.entity_mut(mob).insert(pool);
    play(world, pool, EXIT_SOUND);
    let user = name_of(world, mob);
    show(
        world,
        pool,
        Span::BoldWarning,
        format!("{user} rises out of the blood!"),
    );
    if let Some(mut d) = world.get_mut::<SlaughterDemon>(mob) {
        d.lazy_at = None;
        d.exiting = None;
        d.fast_until = t + CRAWL_SPEED_SECS;
    }
    refresh_button(world, mob);
}

/// The demon's speed with the crawl speed-up.
pub fn crawl_delay(base: f32, fast: bool) -> f32 {
    if fast {
        (base - CRAWL_SPEEDUP).max(0.0)
    } else {
        base
    }
}

/// The meal rounds, the rising, the lazy damage, the speed-up ending.
fn tick(world: &mut World) {
    let t = now(world);
    let demons: Vec<(Entity, SlaughterDemon)> = world
        .query::<(Entity, &SlaughterDemon)>()
        .iter(world)
        .map(|(e, d)| (e, d.clone()))
        .collect();
    for (mob, d) in demons {
        if world.get::<Stat>(mob) == Some(&Stat::Dead) {
            continue;
        }
        let delay = crawl_delay(d.base_delay, t < d.fast_until);
        if let Some(mut m) = world.get_mut::<Mover>(mob)
            && m.delay != delay
        {
            m.delay = delay;
        }
        if let Some(meal) = d.meal
            && t >= meal.at
        {
            meal_round(world, mob, meal, t);
        }
        if let Some((at, pool)) = d.exiting
            && t >= at
        {
            // The `do_after` on the pool: still in the blood and next to it.
            let here = crate::explosions::turf_of(world, mob);
            let near = here.is_some_and(|p| {
                p.z == pool.z && (p.x - pool.x).abs() <= 1 && (p.y - pool.y).abs() <= 1
            });
            let still_blood = nearby_blood(world, pool) == Some(pool);
            if near && still_blood {
                exit_blood(world, mob, pool);
            } else if let Some(mut d) = world.get_mut::<SlaughterDemon>(mob) {
                d.exiting = None;
            }
        }
        if let Some(at) = d.lazy_at
            && t >= at
        {
            if world.get::<Jaunting>(mob).is_some() && d.meal.is_none() {
                if let Some(mut health) = world.get::<Health>(mob).cloned() {
                    health.adjust(DamageType::Brute, health.max * LAZY_SHARE);
                    world.entity_mut(mob).insert(health);
                }
                tell_mob(
                    world,
                    mob,
                    Span::Warning,
                    "You feel your flesh dissolving into the sea of blood. You shouldn't stay in Blood Crawl for too long!",
                );
            }
            if let Some(mut d) = world.get_mut::<SlaughterDemon>(mob) {
                d.lazy_at = d.lazy_at.map(|_| t + LAZY_SECS);
            }
        }
    }
}

/// One `consume_victim` round: the demon must not have moved; after the third the meal.
fn meal_round(world: &mut World, mob: Entity, meal: Meal, t: f64) {
    let moved = crate::explosions::turf_of(world, mob) != Some(meal.pos);
    let gone = world
        .get::<crate::closets::InCloset>(meal.victim)
        .is_none_or(|c| c.closet != mob);
    let laughter = world.get::<SlaughterDemon>(mob).is_some_and(|d| d.laughter);
    if moved || gone {
        tell_mob(world, mob, Span::Danger, "You lose your victim!");
        if !gone && let Some(pos) = crate::explosions::turf_of(world, mob) {
            world.entity_mut(meal.victim).insert(pos);
            crate::closets::let_out(world, meal.victim);
        }
        if let Some(mut d) = world.get_mut::<SlaughterDemon>(mob) {
            d.meal = None;
            d.lazy_at = Some(t + LAZY_SECS);
        }
        return;
    }
    if meal.round < CONSUME_ROUNDS {
        play(world, meal.pos, if laughter { HORN } else { CONSUME_SOUND });
        if let Some(mut d) = world.get_mut::<SlaughterDemon>(mob) {
            d.meal = Some(Meal {
                round: meal.round + 1,
                at: t + CONSUME_SECS,
                ..meal
            });
        }
        return;
    }
    consume(world, mob, meal.victim, laughter);
    if let Some(mut d) = world.get_mut::<SlaughterDemon>(mob) {
        d.meal = None;
        d.consume_count += 1;
        d.lazy_at = Some(t + LAZY_SECS);
    }
}

/// The end of `consume_victim`: the demon healed, the victim dead; eaten by a slaughter
/// demon, kept by a laughter one.
fn consume(world: &mut World, mob: Entity, victim: Entity, laughter: bool) {
    if let Some(health) = world.get::<Health>(mob).cloned() {
        world.entity_mut(mob).insert(health.healed());
    }
    if let Some(mut health) = world.get::<Health>(victim).cloned() {
        health.adjust(DamageType::Brute, 1000.0);
        world.entity_mut(victim).insert((health, Stat::Dead));
    }
    let name = name_of(world, victim);
    info!("{victim} was consumed by the demon {mob}");
    if laughter {
        tell_mob(
            world,
            mob,
            Span::Notice,
            format!("{name} joins your party! Your health is fully restored."),
        );
        if let Some(mut d) = world.get_mut::<SlaughterDemon>(mob) {
            d.friends.push(victim);
        }
        return;
    }
    tell_mob(
        world,
        mob,
        Span::Danger,
        format!("You devour {name}. Your health is fully restored."),
    );
    if let Some(client) = world.get::<PlayerMob>(victim).map(|p| p.client)
        && let Some(ghost) = crate::ghost::ghostize_client(world, client)
        && let Some(mut g) = world.get_mut::<crate::ghost::Ghost>(ghost)
    {
        g.body = None;
    }
    world.despawn(victim);
}

/// `on_victim_statchange`: a friend brought back to life falls out.
fn friends_revived(world: &mut World) {
    let revived: Vec<(Entity, Entity)> = world
        .query::<(Entity, Ref<Stat>, &crate::closets::InCloset)>()
        .iter(world)
        .filter(|(_, s, _)| s.is_changed() && **s != Stat::Dead)
        .map(|(e, _, c)| (e, c.closet))
        .collect();
    for (friend, demon) in revived {
        let is_friend = world
            .get::<SlaughterDemon>(demon)
            .is_some_and(|d| d.friends.contains(&friend));
        if !is_friend {
            continue;
        }
        let Some(pos) = crate::explosions::turf_of(world, demon) else {
            continue;
        };
        if let Some(mut d) = world.get_mut::<SlaughterDemon>(demon) {
            d.friends.retain(|&f| f != friend);
        }
        world.entity_mut(friend).insert(pos);
        crate::closets::let_out(world, friend);
        let name = name_of(world, friend);
        show(
            world,
            pos,
            Span::Warning,
            format!(
                "{name} falls out of the air, covered in blood, with a confused look on their face."
            ),
        );
        play(world, pos, EXIT_SOUND);
    }
}

// ---------------------------------------------------------------------------------------------
// Attacks

/// `on_attack`: a right click slams, a plain click bites.
fn demon_clicks(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    let t = now(world);
    for click in clicks {
        let (mob, target) = (click.mob, click.target);
        let Some(d) = world.get::<SlaughterDemon>(mob).cloned() else {
            continue;
        };
        if target == mob
            || world.get::<Stat>(mob) != Some(&Stat::Conscious)
            || world.get::<Jaunting>(mob).is_some()
            || world.get::<Health>(target).is_none()
        {
            continue;
        }
        if !click.modifiers.right {
            crate::mobs::defense::melee_attack(world, mob, target);
            continue;
        }
        if t < d.slam_ready {
            tell_mob(
                world,
                mob,
                Span::Warning,
                "Your slam ability is still on cooldown!",
            );
            continue;
        }
        bodyslam(world, mob, target, click.mob_pos, click.target_pos);
        if let Some(mut d) = world.get_mut::<SlaughterDemon>(mob) {
            d.slam_ready = t + SLAM_COOLDOWN;
        }
        world
            .entity_mut(mob)
            .insert(NextMove(t + crate::combat::CLICK_CD_MELEE));
    }
}

/// `bodyslam`: 20 brute and thrown 10 tiles the way the demon faces.
fn bodyslam(world: &mut World, mob: Entity, victim: Entity, from: TilePos, to: TilePos) {
    let dir = crate::pull::dir_between(from, to).unwrap_or_default();
    if let Some(mut facing) = world.get_mut::<Facing>(mob) {
        facing.set_if_neq(Facing(dir));
    }
    crate::mobs::lavaland::hurt(world, victim, DamageType::Brute, SLAM_DAMAGE, None);
    let (user, name) = (name_of(world, mob), name_of(world, victim));
    show(
        world,
        to,
        Span::Danger,
        format!("{user} slams into {name} with monstrous strength!"),
    );
    tell_mob(
        world,
        victim,
        Span::Danger,
        format!(
            "{user} slams into you with monstrous strength, sending you flying like a ragdoll!"
        ),
    );
    let (dx, dy) = dir.offset();
    let target = TilePos::new(to.x + dx * SLAM_RANGE, to.y + dy * SLAM_RANGE, to.z);
    crate::throw::throw_at(world, victim, target, SLAM_RANGE as u32, 5.0);
    info!("{mob} (slaughter demon) slammed {victim}");
}

// ---------------------------------------------------------------------------------------------
// Death and the round end

/// The demon's `death` (`crate::mobs`, before `DEL_ON_DEATH` takes it): `death_drops`, the
/// laughter demon's kitten and its friends back alive, its player a ghost.
pub fn on_death(world: &mut World, mob: Entity, def: &MobDef, pos: TilePos) {
    if !is_under(&def.path, SLAUGHTER) {
        return;
    }
    if is_under(&def.path, LAUGHTER) {
        if let Some(kitten) = crate::mobs::spawn::spawn_mob(world, KITTEN, pos) {
            world.entity_mut(kitten).insert(Name::new("Laughter"));
        }
        let friends = world
            .get::<SlaughterDemon>(mob)
            .map(|d| d.friends.clone())
            .unwrap_or_default();
        for friend in friends {
            world.entity_mut(friend).insert(pos);
            crate::closets::let_out(world, friend);
            crate::spells::effects::full_heal(world, friend);
            world.entity_mut(friend).insert(Stat::Conscious);
            crate::ghost::return_to_body(world, friend);
            tell_mob(
                world,
                friend,
                Span::Notice,
                "You leave the demon's warm embrace, and feel ready to take on the world.",
            );
        }
    } else {
        for path in [BLOOD, INNARDS, DEMON_HEART] {
            crate::construction::spawn_obj(world, path, pos);
        }
    }
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        crate::ghost::ghostize_client(world, client);
    }
}

/// Each player's demon and how many it consumed.
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let mut all: Vec<(Entity, String, u32)> = world
        .query::<(Entity, &SlaughterDemon)>()
        .iter(world)
        .map(|(e, d)| (e, d.key.clone(), d.consume_count))
        .collect();
    if all.is_empty() {
        return Vec::new();
    }
    all.sort_by_key(|(e, ..)| *e);
    let mut out = vec![ChatMessage {
        channel: ChatChannel::System,
        segments: vec![ChatSegment::new(
            Span::Bold,
            format!("{ROUNDEND_CATEGORY} были:"),
        )],
    }];
    for (mob, key, eaten) in all {
        out.push(super::traitor::print_player(world, mob, &key));
        out.push(ChatMessage {
            channel: ChatChannel::System,
            segments: vec![ChatSegment::new(Span::Plain, format!("Поглощено: {eaten}"))],
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blood_and_speed() {
        assert!(crawlable(BLOOD));
        assert!(crawlable(INNARDS));
        assert!(crawlable(TOMATO));
        assert!(!crawlable("/obj/effect/decal/cleanable/vomit"));
        assert!((crawl_delay(0.1, true) - 0.0).abs() < 1e-6);
        assert_eq!(crawl_delay(0.1, false), 0.1);
    }

    #[test]
    fn the_ruleset_takes_the_demon_role() {
        let r = super::super::midround::ruleset(super::super::midround::SLAUGHTER_DEMON).unwrap();
        assert_eq!(r.role, Some(ROLE_SLAUGHTER_DEMON));
        assert_eq!(r.weight, [0; 4]);
        assert_eq!(r.min_pop, 20);
    }

    /// A meal takes three rounds of the demon standing still; the slaughter demon is healed and
    /// its victim gone.
    #[test]
    fn a_meal_heals_and_takes_the_victim() {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<Messages<crate::sound::PlaySound>>();
        let pos = TilePos::new(2, 2, 1);
        let mut hurt = Health::basic(200.0, [1.0; 5]);
        hurt.adjust(DamageType::Brute, 150.0);
        let demon = world.spawn((pos, hurt, Stat::Conscious)).id();
        world.entity_mut(demon).insert(SlaughterDemon {
            key: String::new(),
            laughter: false,
            meal: None,
            friends: Vec::new(),
            consume_count: 0,
            lazy_at: None,
            exiting: None,
            slam_ready: 0.0,
            fast_until: 0.0,
            base_delay: 0.1,
        });
        let victim = world.spawn((pos, Health::default(), Stat::HardCrit)).id();
        crate::closets::put_mob_inside(&mut world, victim, demon);
        let meal = Meal {
            victim,
            round: 1,
            at: 0.0,
            pos,
        };
        meal_round(&mut world, demon, meal, 0.0);
        let next = world.get::<SlaughterDemon>(demon).unwrap().meal.unwrap();
        assert_eq!(next.round, 2);
        meal_round(&mut world, demon, Meal { round: 3, ..next }, 6.0);
        assert!(world.get_entity(victim).is_err());
        let d = world.get::<SlaughterDemon>(demon).unwrap();
        assert_eq!(d.consume_count, 1);
        assert!(d.meal.is_none());
        assert_eq!(world.get::<Health>(demon).unwrap().brute, 0.0);
    }
}
