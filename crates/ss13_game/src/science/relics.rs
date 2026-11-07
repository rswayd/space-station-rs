//! Strange objects (server), after tg `/obj/item/assembly/relic` (`relics.dm`): an unknown
//! artefact. Until the E.X.P.E.R.I-MENTOR's `Discover` reveals it ([`reveal`]: its real name, a
//! random power, a 6-30 s cooldown) using it in hand only says R&D might know what it is;
//! revealed, it fires its power (`activate`), then waits out the cooldown.
//!
//! Powers: corgi cannon, animal summoning (1-25 animals, 60% the relic falls apart), the
//! uncontrolled teleport (1-3 s later the holder, or the relic, within 8 tiles), heat and explode
//! (3.5-10 s later, if still held by the user), the rapid self-duplicator (5-10 copies that vanish
//! in 1-10 s) and the hugger.
// Also the flashbanger (a flashbang goes off) and, for the necrotech theme, monster summoning
// (3-9, 80% falls apart) and the area teleport (view 2 onto the relic, 30% it vanishes).
// shortcut: cleaning and acid foam, the drink dispenser, tummy ache, the charger, dimensional
// shift, the disguiser, the shield holders, rocks and the blood powers are not ported (foam
// grenades, organ scrambling, dimension themes, costumes, shield components, mineral turfs) and
// not rolled. No themed icon states, no throws (copies and corgis land around the relic), no
// gift auto-reveal, relic mob cap, assembly signals or admin warnings.

use bevy::prelude::*;
use ss13_mapload::{DmName, DmObj};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::click_handlers::{AddClickHandler, ClickPhase};
use crate::combat::CombatRng;
use crate::inventory::CarriedBy;
use crate::obj_damage::is_under;
use crate::tools::tell;

pub const RELIC_ROOT: &str = "/obj/item/assembly/relic";
const NECROTECH: &str = "/obj/item/assembly/relic/lavaland";
/// `min_cooldown`, `max_cooldown`, seconds.
const MIN_COOLDOWN: u32 = 6;
const MAX_COOLDOWN: u32 = 30;
/// `summon_animals`'s `valid_animals` (`ss13_mapload::runtime::GAME_RUNTIME_TYPES`).
const ANIMALS: [&str; 12] = [
    "/mob/living/basic/bear",
    "/mob/living/basic/bee",
    "/mob/living/basic/butterfly",
    "/mob/living/basic/carp",
    "/mob/living/basic/crab",
    "/mob/living/basic/lizard",
    "/mob/living/basic/mouse",
    "/mob/living/basic/parrot",
    "/mob/living/basic/pet/cat",
    "/mob/living/basic/pet/dog/corgi",
    "/mob/living/basic/pet/dog/pug",
    "/mob/living/basic/pet/fox",
];
const CORGI: &str = "/mob/living/basic/pet/dog/corgi";
/// `random_themed_appearance`'s words.
const PROTOTYPE_PREFIXES: &[&str] = &[
    "experimental",
    "prototype",
    "artificial",
    "handcrafted",
    "ramshackle",
    "odd",
];
const PROTOTYPE_SUFFIXES: &[&str] = &[
    "device",
    "assembly",
    "gadget",
    "gizmo",
    "contraption",
    "machine",
    "widget",
    "object",
];
const NECROTECH_PREFIXES: &[&str] = &[
    "dark",
    "bloodied",
    "unholy",
    "archeotechnological",
    "dismal",
    "ruined",
    "thrumming",
];
const NECROTECH_SUFFIXES: &[&str] = &[
    "instrument",
    "shard",
    "fetish",
    "bibelot",
    "trinket",
    "offering",
    "relic",
];

/// `hidden_power`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelicPower {
    CorgiCannon,
    SummonAnimals,
    UncontrolledTeleport,
    HeatAndExplode,
    RapidSelfDupe,
    Hugger,
    Flashbanger,
    SummonMonsters,
    AoeTeleport,
}

/// The ported powers of the prototype theme.
const PROTOTYPE_POWERS: [RelicPower; 7] = [
    RelicPower::CorgiCannon,
    RelicPower::SummonAnimals,
    RelicPower::UncontrolledTeleport,
    RelicPower::HeatAndExplode,
    RelicPower::RapidSelfDupe,
    RelicPower::Hugger,
    RelicPower::Flashbanger,
];
/// The ported powers of the necrotech theme.
const NECROTECH_POWERS: [RelicPower; 4] = [
    RelicPower::SummonMonsters,
    RelicPower::HeatAndExplode,
    RelicPower::UncontrolledTeleport,
    RelicPower::AoeTeleport,
];
const FLASHBANG: &str = "/obj/item/grenade/flashbang";
/// `summon_animals_monsters`'s `valid_monsters` (`ss13_mapload::runtime::GAME_RUNTIME_TYPES`).
const MONSTERS: [&str; 18] = [
    "/mob/living/basic/construct/artificer/hostile",
    "/mob/living/basic/construct/juggernaut/hostile",
    "/mob/living/basic/construct/proteon/hostile",
    "/mob/living/basic/construct/wraith/hostile",
    "/mob/living/basic/mining/brimdemon",
    "/mob/living/basic/mining/goldgrub",
    "/mob/living/basic/mining/goliath",
    "/mob/living/basic/mining/hivelord",
    "/mob/living/basic/mining/ice_demon",
    "/mob/living/basic/mining/legion",
    "/mob/living/basic/mining/lobstrosity",
    "/mob/living/basic/mining/watcher",
    "/mob/living/basic/raptor/blue",
    "/mob/living/basic/raptor/green",
    "/mob/living/basic/raptor/purple",
    "/mob/living/basic/raptor/red",
    "/mob/living/basic/raptor/white",
    "/mob/living/basic/raptor/yellow",
];

#[derive(Component, Clone, Debug, Default)]
pub struct Relic {
    pub real_name: String,
    pub activated: bool,
    pub hidden_power: Option<RelicPower>,
    /// `activation_cooldown`, seconds.
    pub cooldown: f64,
    /// Game time the next activation may happen (`next_activate`).
    pub next_activate: f64,
}

/// A delayed power: teleport or blow up at game time `at`, for `user`.
#[derive(Component, Clone, Copy, Debug)]
pub struct RelicTimer {
    pub at: f64,
    pub power: RelicPower,
    pub user: Entity,
}

/// A rapid duplicate: vanishes at game time `.0`.
#[derive(Component, Clone, Copy, Debug)]
pub struct RelicDupe(pub f64);

pub struct RelicPlugin;

impl Plugin for RelicPlugin {
    fn build(&self, app: &mut App) {
        app.add_click_handler(ClickPhase::AttackSelf, 1897, |w, c| {
            attack_self(w, c.client, c.mob, c.target)
        })
        .add_systems(Update, relic_timers);
    }
}

pub fn is_relic(path: &str) -> bool {
    is_under(path, RELIC_ROOT)
}

fn rand_range(world: &mut World, low: u32, high: u32) -> u32 {
    world
        .get_resource_mut::<CombatRng>()
        .map_or(low, |mut r| r.range(low, high))
}

fn prob(world: &mut World, percent: f32) -> bool {
    world
        .get_resource_mut::<CombatRng>()
        .is_some_and(|mut r| r.chance(percent))
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

/// `Initialize`'s `random_themed_appearance`, on first use: the strange name and the real one.
pub fn ensure_relic(world: &mut World, item: Entity) -> bool {
    if world.get::<Relic>(item).is_some() {
        return true;
    }
    let Some(path) = world.get::<DmObj>(item).map(|o| o.path.clone()) else {
        return false;
    };
    if !is_relic(&path) {
        return false;
    }
    let (prefixes, suffixes) = if is_under(&path, NECROTECH) {
        (NECROTECH_PREFIXES, NECROTECH_SUFFIXES)
    } else {
        (PROTOTYPE_PREFIXES, PROTOTYPE_SUFFIXES)
    };
    let prefix = prefixes[rand_range(world, 0, prefixes.len() as u32 - 1) as usize];
    let suffix = suffixes[rand_range(world, 0, suffixes.len() as u32 - 1) as usize];
    let strange = if is_under(&path, NECROTECH) {
        "strange relic".to_owned()
    } else {
        let shown = suffixes[rand_range(world, 0, suffixes.len() as u32 - 1) as usize];
        format!("strange {shown}")
    };
    world.entity_mut(item).insert(Relic {
        real_name: format!("{prefix} {suffix}"),
        ..default()
    });
    set_name(world, item, &strange);
    // `relic/revealed`, `relic/lavaland/revealed`: `auto_reveal` on spawn.
    if path.ends_with("/revealed") {
        reveal(world, item);
    }
    true
}

fn set_name(world: &mut World, item: Entity, name: &str) {
    if let Some(mut n) = world.get_mut::<DmName>(item) {
        n.name = name.to_owned();
    }
}

/// tg `reveal()`: the real name, a random power and cooldown; never rerolled.
pub fn reveal(world: &mut World, item: Entity) {
    if !ensure_relic(world, item) {
        return;
    }
    if world.get::<Relic>(item).is_some_and(|r| r.activated) {
        return;
    }
    let cooldown = f64::from(rand_range(world, MIN_COOLDOWN, MAX_COOLDOWN));
    let necrotech = world
        .get::<DmObj>(item)
        .is_some_and(|o| is_under(&o.path, NECROTECH));
    let powers: &[RelicPower] = if necrotech {
        &NECROTECH_POWERS
    } else {
        &PROTOTYPE_POWERS
    };
    let power = powers[rand_range(world, 0, powers.len() as u32 - 1) as usize];
    let name = {
        let Some(mut relic) = world.get_mut::<Relic>(item) else {
            return;
        };
        relic.activated = true;
        if relic.cooldown == 0.0 {
            relic.cooldown = cooldown;
        }
        if relic.hidden_power.is_none() {
            relic.hidden_power = Some(power);
        }
        relic.real_name.clone()
    };
    set_name(world, item, &name);
}

/// tg `attack_self`. True when `item` is a relic.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if !ensure_relic(world, item) {
        return false;
    }
    let relic = world.get::<Relic>(item).cloned().unwrap_or_default();
    if !relic.activated {
        tell(
            world,
            client,
            Span::Notice,
            "You aren't quite sure what this is. Maybe R&D knows what to do with it?",
        );
        return true;
    }
    activate(world, client, mob, item);
    true
}

/// tg `activate(activator)`.
pub fn activate(world: &mut World, client: Entity, user: Entity, item: Entity) -> bool {
    let Some(relic) = world.get::<Relic>(item).cloned() else {
        return false;
    };
    let t = now(world);
    if t < relic.next_activate {
        let name = item_name(world, item);
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} does not react!"),
        );
        return false;
    }
    let Some(power) = relic.hidden_power else {
        return false;
    };
    if let Some(mut r) = world.get_mut::<Relic>(item) {
        r.next_activate = t + relic.cooldown;
    }
    let Some(pos) = turf_of(world, item) else {
        return false;
    };
    match power {
        RelicPower::CorgiCannon => {
            if let Some(tile) = random_tile(world, pos, 10) {
                crate::mobs::spawn::spawn_mob(world, CORGI, tile);
                crate::effects::smoke::do_smoke(
                    world,
                    0,
                    tile,
                    None,
                    crate::effects::smoke::SmokeKind::Basic,
                );
            }
        }
        RelicPower::SummonAnimals => {
            let mut spawned = 0;
            for _ in 0..rand_range(world, 1, 25) {
                let path = ANIMALS[rand_range(world, 0, ANIMALS.len() as u32 - 1) as usize];
                if let Some(tile) = random_tile(world, pos, 7)
                    && crate::mobs::spawn::spawn_mob(world, path, tile).is_some()
                {
                    spawned += 1;
                }
            }
            let name = item_name(world, item);
            let (span, text) = if spawned > 0 {
                (
                    Span::Danger,
                    format!(
                        "{name} begins to shake, and in the distance the sound of rampaging \
                         animals arises!"
                    ),
                )
            } else {
                (
                    Span::Warning,
                    format!("{name} begins to shake, but nothing happens..."),
                )
            };
            crate::medical::visible(world, user, &text, &text, span);
            if spawned > 0 && prob(world, 60.0) {
                fall_apart(world, user, item);
            }
        }
        RelicPower::UncontrolledTeleport | RelicPower::HeatAndExplode => {
            let (text, low, high) = if power == RelicPower::HeatAndExplode {
                (" begins to heat up!", 35, 100)
            } else {
                (" begins to vibrate!", 10, 30)
            };
            let name = item_name(world, item);
            let span = if power == RelicPower::HeatAndExplode {
                Span::Danger
            } else {
                Span::Notice
            };
            tell(world, client, span, format!("{name}{text}"));
            let at = t + f64::from(rand_range(world, low, high)) / 10.0;
            world
                .entity_mut(item)
                .insert(RelicTimer { at, power, user });
        }
        RelicPower::RapidSelfDupe => {
            let name = item_name(world, item);
            crate::medical::visible(
                world,
                user,
                &format!("{name} emits a loud pop!"),
                &format!("{name} emits a loud pop!"),
                Span::Info,
            );
            let path = world
                .get::<DmObj>(item)
                .map(|o| o.path.clone())
                .unwrap_or_default();
            for _ in 0..rand_range(world, 5, 10) {
                let Some(tile) = random_tile(world, pos, 7) else {
                    continue;
                };
                let Some(dupe) = crate::construction::spawn_obj(world, &path, tile) else {
                    continue;
                };
                let vanish = t + f64::from(rand_range(world, 10, 100)) / 10.0;
                world
                    .entity_mut(dupe)
                    .insert((relic.clone(), RelicDupe(vanish)));
                set_name(world, dupe, &name);
            }
        }
        RelicPower::Hugger => {
            let near = crate::obj_damage::clients_in_range(world, pos, 3, &[user]);
            let text = if near.is_empty() {
                "You hug yourself, for some reason."
            } else {
                "You feel friendly!"
            };
            tell(world, client, Span::Notice, text);
        }
        RelicPower::Flashbanger => {
            if let Some(bang) = crate::construction::spawn_obj(world, FLASHBANG, pos) {
                crate::grenades::detonate(world, bang);
            }
        }
        RelicPower::SummonMonsters => {
            let mut spawned = 0;
            for _ in 0..rand_range(world, 3, 9) {
                let path = MONSTERS[rand_range(world, 0, MONSTERS.len() as u32 - 1) as usize];
                if let Some(tile) = random_tile(world, pos, 7)
                    && crate::mobs::spawn::spawn_mob(world, path, tile).is_some()
                {
                    spawned += 1;
                }
            }
            let name = item_name(world, item);
            let (span, text) = if spawned > 0 {
                (
                    Span::Danger,
                    format!(
                        "{name} begins to shake, and in the distance the sound of roaring arises!"
                    ),
                )
            } else {
                (
                    Span::Warning,
                    format!("{name} begins to shake, but nothing happens..."),
                )
            };
            crate::medical::visible(world, user, &text, &text, span);
            if spawned > 0 && prob(world, 80.0) {
                fall_apart(world, user, item);
            }
        }
        RelicPower::AoeTeleport => {
            let name = item_name(world, item);
            tell(
                world,
                client,
                Span::Notice,
                format!("{name} begins to vibrate intensely!"),
            );
            let at = t + f64::from(rand_range(world, 10, 30)) / 10.0;
            world
                .entity_mut(item)
                .insert(RelicTimer { at, power, user });
        }
    }
    true
}

/// `do_the_aoe_teleport`: every loose living mob and item in view 2 lands near the relic; 30%
/// the relic is gone.
// shortcut: no holy-magic resistance, no cult phase effects.
fn aoe_teleport(world: &mut World, item: Entity, user: Entity, pos: TilePos) {
    let name = item_name(world, item);
    let text = format!("{name} twists and bends, relocating anything nearby!");
    crate::medical::visible(world, user, &text, &text, Span::Notice);
    let index = world.resource::<ss13_world::SpatialIndex>();
    let mut movers = Vec::new();
    for dy in -2..=2 {
        for dx in -2..=2 {
            let p = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
            movers.extend(index.on_tile(p).iter().copied().filter(|&e| {
                let anchored = world
                    .get::<crate::obj_damage::Anchored>(e)
                    .is_some_and(|a| a.0);
                !anchored
                    && (world.get::<crate::health::Health>(e).is_some()
                        || world.get::<ss13_mapload::DmItem>(e).is_some())
            }));
        }
    }
    for mover in movers {
        crate::teleport::do_teleport(
            world,
            mover,
            pos,
            crate::teleport::Tele {
                precision: 5,
                channel: crate::teleport::Channel::Cult,
                sound_in: Some("sound/effects/phasein.ogg"),
                ..default()
            },
        );
    }
    if prob(world, 30.0) && world.get_entity(item).is_ok() {
        let text = format!("{name} teleports away, never to be seen again!");
        crate::medical::visible(world, user, &text, &text, Span::Warning);
        world.despawn(item);
    }
}

fn item_name(world: &World, item: Entity) -> String {
    world
        .get::<DmName>(item)
        .map(|n| n.name.clone())
        .unwrap_or_default()
}

/// The relic's turf: its own tile, or its holder's.
fn turf_of(world: &World, item: Entity) -> Option<TilePos> {
    world.get::<TilePos>(item).copied().or_else(|| {
        world
            .get::<CarriedBy>(item)
            .and_then(|c| world.get::<TilePos>(c.0).copied())
    })
}

/// A random in-bounds tile within `range` (`pick(oview(range))`).
fn random_tile(world: &mut World, pos: TilePos, range: i32) -> Option<TilePos> {
    let index = world.resource::<ss13_world::SpatialIndex>();
    let tiles: Vec<TilePos> = (-range..=range)
        .flat_map(|dy| (-range..=range).map(move |dx| (dx, dy)))
        .filter(|&d| d != (0, 0))
        .map(|(dx, dy)| TilePos::new(pos.x + dx, pos.y + dy, pos.z))
        .filter(|&p| index.in_bounds(p))
        .collect();
    if tiles.is_empty() {
        return None;
    }
    Some(tiles[rand_range(world, 0, tiles.len() as u32 - 1) as usize])
}

/// `relic_message("falls apart!")` and `deconstruct(FALSE)`.
fn fall_apart(world: &mut World, user: Entity, item: Entity) {
    let name = item_name(world, item);
    let text = format!("{name} falls apart!");
    crate::medical::visible(world, user, &text, &text, Span::Warning);
    world.despawn(item);
}

/// The teleport and explosion timers, and the duplicates' end.
fn relic_timers(world: &mut World) {
    let t = now(world);
    let mut dupes = world.query::<(Entity, &RelicDupe)>();
    let gone: Vec<Entity> = dupes
        .iter(world)
        .filter(|(_, d)| d.0 <= t)
        .map(|(e, _)| e)
        .collect();
    for e in gone {
        world.despawn(e);
    }
    let mut timers = world.query::<(Entity, &RelicTimer)>();
    let due: Vec<(Entity, RelicTimer)> = timers
        .iter(world)
        .filter(|(_, r)| r.at <= t)
        .map(|(e, r)| (e, *r))
        .collect();
    for (item, timer) in due {
        world.entity_mut(item).remove::<RelicTimer>();
        let Some(pos) = turf_of(world, item) else {
            continue;
        };
        if timer.power == RelicPower::AoeTeleport {
            aoe_teleport(world, item, timer.user, pos);
        } else if timer.power == RelicPower::HeatAndExplode {
            // `blow_up`: only while the user still holds it (or nobody does).
            let holder = world.get::<CarriedBy>(item).map(|c| c.0);
            if holder.is_some_and(|h| h != timer.user) {
                continue;
            }
            let name = item_name(world, item);
            let text = format!("The {name}'s top opens, releasing a powerful blast!");
            crate::medical::visible(world, timer.user, &text, &text, Span::Notice);
            let heavy = rand_range(world, 1, 5) as i32;
            let light = rand_range(world, 1, 5) as i32;
            let flash = rand_range(world, 1, 5) as i32;
            world.despawn(item);
            crate::explosions::explosion(
                world,
                pos,
                crate::explosions::Explosion::new(0, heavy, light)
                    .flame(2)
                    .flash(flash),
            );
        } else {
            // `do_the_teleport`: the holder (`get_loose_container`), or the relic itself.
            let mover = world.get::<CarriedBy>(item).map_or(item, |c| c.0);
            let name = item_name(world, mover);
            let text = format!("{name} twists and bends, relocating itself!");
            crate::medical::visible(world, mover, &text, &text, Span::Notice);
            let smoke = crate::effects::smoke::SmokeKind::Quick;
            crate::effects::smoke::do_smoke(world, 0, pos, None, smoke);
            crate::teleport::do_teleport(
                world,
                mover,
                pos,
                crate::teleport::Tele {
                    precision: 8,
                    sound_in: Some("sound/effects/phasein.ogg"),
                    ..default()
                },
            );
            if let Some(&now_at) = world.get::<TilePos>(mover) {
                crate::effects::smoke::do_smoke(world, 0, now_at, None, smoke);
            }
        }
    }
}
