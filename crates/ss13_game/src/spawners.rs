//! Mob spawners (server), after tg `code/game/objects/structures/spawner.dm` and
//! `/datum/component/spawner` (`code/datums/components/spawner.dm`).
//!
//! A nest ([`Spawner`]) makes one mob of a random type of its kind on its tile every
//! `spawn_time` while fewer than `max_mobs` of its mobs are alive (dead or deleted ones stop
//! counting), gives it the nest's faction and says "<mob> <spawn_text> <nest>.". A mapped nest
//! starts at once, a new one waits a full `spawn_time`. Nests are breakable structures
//! (`crate::obj_damage`); a destroyed one stops.
// shortcut: no GPS holotags with a mining scanner, no netherworld link pulling in and digesting
// who touches it, no ghost-controlled (sentient) proteon gateway, basic mobs of the nest's
// faction still hit it. Add each with its mechanic.

use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::combat::CombatRng;
use crate::health::Stat;
use crate::medical::{name_of, visible};
use crate::obj_damage::is_under;

pub const SPAWNER: &str = "/obj/structure/spawner";

/// One kind of nest: `max_mobs`, `spawn_time` (s), `mob_types`, `faction`, `spawn_text`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NestKind {
    pub root: &'static str,
    pub max_mobs: usize,
    pub spawn_time: f64,
    pub mobs: &'static [&'static str],
    pub faction: &'static str,
    pub text: &'static str,
}

const CLOWNS: &[&str] = &[
    "/mob/living/basic/clown",
    "/mob/living/basic/clown/banana",
    "/mob/living/basic/clown/clownhulk",
    "/mob/living/basic/clown/clownhulk/chlown",
    "/mob/living/basic/clown/clownhulk/honkmunculus",
    "/mob/living/basic/clown/fleshclown",
    "/mob/living/basic/clown/mutant/glutton",
    "/mob/living/basic/clown/honkling",
    "/mob/living/basic/clown/longface",
    "/mob/living/basic/clown/lube",
];
const GOLDGRUB: &str = "/mob/living/basic/mining/goldgrub";
const GOLIATH: &str = "/mob/living/basic/mining/goliath/ancient";
const HIVELORD: &str = "/mob/living/basic/mining/hivelord";
const BASILISK: &str = "/mob/living/basic/mining/basilisk";
const FUGU: &str = "/mob/living/basic/wumborian_fugu";

/// Most specific first (`is_under` picks the first match).
pub const KINDS: &[NestKind] = &[
    NestKind {
        root: "/obj/structure/spawner/syndicate",
        max_mobs: 5,
        spawn_time: 30.0,
        mobs: &["/mob/living/basic/trooper/syndicate/ranged"],
        faction: "Syndicate",
        text: "появляется из",
    },
    NestKind {
        root: "/obj/structure/spawner/skeleton",
        max_mobs: 15,
        spawn_time: 15.0,
        mobs: &["/mob/living/basic/skeleton"],
        faction: "skeleton",
        text: "выбирается из",
    },
    NestKind {
        root: "/obj/structure/spawner/clown",
        max_mobs: 15,
        spawn_time: 15.0,
        mobs: CLOWNS,
        faction: "clowns",
        text: "выбирается из",
    },
    NestKind {
        root: "/obj/structure/spawner/mining/goldgrub",
        max_mobs: 3,
        spawn_time: 30.0,
        mobs: &[GOLDGRUB],
        faction: "mining",
        text: "выползает из",
    },
    NestKind {
        root: "/obj/structure/spawner/mining/goliath",
        max_mobs: 3,
        spawn_time: 30.0,
        mobs: &[GOLIATH],
        faction: "mining",
        text: "выползает из",
    },
    NestKind {
        root: "/obj/structure/spawner/mining/hivelord",
        max_mobs: 3,
        spawn_time: 30.0,
        mobs: &[HIVELORD],
        faction: "mining",
        text: "выползает из",
    },
    NestKind {
        root: "/obj/structure/spawner/mining/basilisk",
        max_mobs: 3,
        spawn_time: 30.0,
        mobs: &[BASILISK],
        faction: "mining",
        text: "выползает из",
    },
    NestKind {
        root: "/obj/structure/spawner/mining/wumborian",
        max_mobs: 3,
        spawn_time: 30.0,
        mobs: &[FUGU],
        faction: "mining",
        text: "выползает из",
    },
    NestKind {
        root: "/obj/structure/spawner/mining",
        max_mobs: 3,
        spawn_time: 30.0,
        mobs: &[BASILISK, GOLDGRUB, GOLIATH, HIVELORD, FUGU],
        faction: "mining",
        text: "выползает из",
    },
    NestKind {
        root: "/obj/structure/spawner/nether",
        max_mobs: 15,
        spawn_time: 60.0,
        mobs: &[
            "/mob/living/basic/blankbody",
            "/mob/living/basic/creature",
            "/mob/living/basic/migo",
        ],
        faction: "nether",
        text: "пролезает сквозь",
    },
    NestKind {
        root: "/obj/structure/spawner/sentient/proteon_spawner",
        max_mobs: 2,
        spawn_time: 15.0,
        mobs: &["/mob/living/basic/construct/proteon/hostile"],
        faction: "cult",
        text: "выходит из",
    },
    NestKind {
        root: SPAWNER,
        max_mobs: 5,
        spawn_time: 30.0,
        mobs: &["/mob/living/basic/carp"],
        faction: "hostile",
        text: "появляется из",
    },
];

/// Every path a nest spawns (`ss13_mapload::runtime::GAME_RUNTIME_TYPES` lists them).
pub fn kind_of(path: &str) -> Option<&'static NestKind> {
    KINDS.iter().find(|k| is_under(path, k.root))
}

/// The live `/datum/component/spawner`: its kind, `spawned_things`, the `spawn_delay` cooldown.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Spawner {
    pub kind: &'static NestKind,
    pub spawned: Vec<Entity>,
    /// When the next spawn may happen (`Time::elapsed_secs_f64`).
    pub next_at: f64,
}

pub struct SpawnersPlugin;

impl Plugin for SpawnersPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(add_spawner)
            .add_systems(Update, process.run_if(any_with_component::<Spawner>));
    }
}

/// What [`crate::obj_damage`] gives integrity to here.
pub fn breakable(path: &str) -> bool {
    is_under(path, SPAWNER)
}

/// `spawner/Initialize(mapload)`: `initial_spawn_delay = !mapload`.
fn add_spawner(
    add: On<Add<DmObj>>,
    objs: Query<(&DmObj, Has<ss13_world::Mapped>)>,
    mut commands: Commands,
) {
    let Ok((dm, mapped)) = objs.get(add.entity) else {
        return;
    };
    let Some(kind) = kind_of(&dm.path) else {
        return;
    };
    let entity = add.entity;
    commands.queue(move |world: &mut World| {
        let now = world.resource::<Time>().elapsed_secs_f64();
        let next_at = if mapped { now } else { now + kind.spawn_time };
        world.entity_mut(entity).insert(Spawner {
            kind,
            spawned: Vec::new(),
            next_at,
        });
    });
}

/// `validate_references`: gone or dead mobs stop counting.
fn alive(world: &World, mob: Entity) -> bool {
    world.get_entity(mob).is_ok() && world.get::<Stat>(mob).is_none_or(|s| *s != Stat::Dead)
}

/// Whether a nest with `alive` mobs may spawn at `now` (`try_spawn_mob`'s checks).
pub fn may_spawn(spawner: &Spawner, alive: usize, now: f64) -> bool {
    now >= spawner.next_at && alive < spawner.kind.max_mobs && !spawner.kind.mobs.is_empty()
}

/// `process` -> `try_spawn_mob`.
// Cost: per tick, the nests on the map (a handful) with their few spawned mobs.
fn process(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let nests: Vec<Entity> = world
        .query_filtered::<Entity, With<Spawner>>()
        .iter(world)
        .collect();
    for nest in nests {
        let Some(mut s) = world.get::<Spawner>(nest).cloned() else {
            continue;
        };
        if now < s.next_at {
            continue;
        }
        s.spawned.retain(|&m| alive(world, m));
        if !may_spawn(&s, s.spawned.len(), now) {
            world.entity_mut(nest).insert(s);
            continue;
        }
        s.next_at = now + s.kind.spawn_time;
        let pick = world
            .resource_mut::<CombatRng>()
            .range(0, s.kind.mobs.len() as u32 - 1) as usize;
        let path = s.kind.mobs[pick];
        let Some(&pos) = world.get::<TilePos>(nest) else {
            continue;
        };
        if let Some(mob) = crate::mobs::spawn::spawn_mob(world, path, pos) {
            world
                .entity_mut(mob)
                .insert(crate::mobs::Faction(vec![s.kind.faction.to_owned()]));
            s.spawned.push(mob);
            let text = format!(
                "{} {} {}.",
                crate::cooking::capitalize(&name_of(world, mob)),
                s.kind.text,
                name_of(world, nest)
            );
            visible(world, mob, "", &text, Span::Danger);
        }
        world.entity_mut(nest).insert(s);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nests_pick_their_most_specific_kind() {
        assert_eq!(
            kind_of("/obj/structure/spawner/mining/goliath").map(|k| k.mobs),
            Some(&[GOLIATH][..])
        );
        assert_eq!(
            kind_of("/obj/structure/spawner/mining").map(|k| k.mobs.len()),
            Some(5)
        );
        assert_eq!(
            kind_of("/obj/structure/spawner").map(|k| k.faction),
            Some("hostile")
        );
        assert_eq!(kind_of("/obj/structure/grille"), None);
    }

    #[test]
    fn a_nest_waits_its_cooldown_and_stops_at_max_mobs() {
        let s = Spawner {
            kind: kind_of("/obj/structure/spawner/skeleton").unwrap(),
            spawned: Vec::new(),
            next_at: 10.0,
        };
        assert!(!may_spawn(&s, 0, 9.0));
        assert!(may_spawn(&s, 14, 10.0));
        assert!(!may_spawn(&s, 15, 10.0));
    }
}
