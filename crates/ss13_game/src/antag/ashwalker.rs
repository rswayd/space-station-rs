//! Ash walkers after tg `/datum/antagonist/ashwalker` and `/datum/team/ashwalkers`
//! (`code/modules/antagonists/ashwalker/ashwalker.dm`), the necropolis tendril nest
//! `/obj/structure/lavaland/ash_walker` (`mapfluff/ruins/objects_and_mobs/ash_walker_den.dm`)
//! and its egg `/obj/effect/mob_spawn/ghost_role/human/ash_walker` (`mining_roles.dm`).
//!
//! - **The nest** ([`AshNest`], placed by the always-placed lavaland ruin): its tribe
//!   ([`AshTeams`]) starts with the objective "Защитите любой ценой - <nest>." (tg
//!   `protect_object`, done while the nest stands). Every second ([`process_nests`]) it takes
//!   each mob next to it (or on it) that is unconscious, in crit or dead: a cyborg is torn apart
//!   for nothing; a dead ash walker whose player is still around is remade 20 s later
//!   ([`remake_walker`]: a new nude ash walker body next to the nest with its name, the player
//!   in it, the old body gone); anything else is gibbed for meat (+1, a megafauna +20), heals
//!   the nest by 5% and pleases the tribe within 5 tiles (`sacrifice_good`); an ash walker who
//!   dragged it there may get their egg back (40%). Every 2 meat it lays an egg next to itself
//!   (it starts with 6) and the ghosts are told.
//! - **The egg** ([`AshEgg`]): a ghost role of the spawners menu (`crate::ghost_roles` makes the
//!   ash walker from the spawner's species and outfit); one per player unless the Necropolis
//!   gave a bonus ([`allow_spawn`]). The hatchling ([`special`]) gets a Draconic name
//!   (`strings/names/lizard_*.txt`, "Имя-Имя"), the tribe's text, the antagonist, loses Galactic
//!   Common and the neutral faction.
//! - A destroyed nest ([`deconstruct`]) drops a random anomaly core.
//! - [`roundend_report`]: each tribe, its members, the nest's fate, sacrifices and eggs.
// shortcut: the remade walker's old body waits on the nest tile instead of inside it; no
// collapse effect after the nest is destroyed (5 s loot grab, then a chasm); no `headpike`
// examine moodlet; no egg shell breaking into a gibbed yolk; the antagonist sits on the body
// (no mind datum). Add each with its mechanic.

use std::path::PathBuf;

use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_mapload::MapLoadPlugin;
use ss13_net::server::ClientToken;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::{Dir, TilePos};

use super::objectives::RoundEnd;
use crate::combat::CombatRng;
use crate::ghost::Ghost;
use crate::ghost_verbs::GhostRoleSpawner;
use crate::health::{Health, Stat};
use crate::obj_damage::Integrity;
use crate::server::PlayerMob;
use crate::tools::{now, tell, tell_mob};

pub const NEST: &str = "/obj/structure/lavaland/ash_walker";
pub const EGG_SPAWNER: &str = "/obj/effect/mob_spawn/ghost_role/human/ash_walker";
pub const EGGSHELL: &str = "/obj/structure/ash_walker_eggshell";
/// `subtypesof(/obj/item/assembly/signaler/anomaly)`.
pub const ANOMALY_CORES: [&str; 10] = [
    "/obj/item/assembly/signaler/anomaly/pyro",
    "/obj/item/assembly/signaler/anomaly/grav",
    "/obj/item/assembly/signaler/anomaly/flux",
    "/obj/item/assembly/signaler/anomaly/bluespace",
    "/obj/item/assembly/signaler/anomaly/vortex",
    "/obj/item/assembly/signaler/anomaly/bioscrambler",
    "/obj/item/assembly/signaler/anomaly/hallucination",
    "/obj/item/assembly/signaler/anomaly/dimensional",
    "/obj/item/assembly/signaler/anomaly/ectoplasm",
    "/obj/item/assembly/signaler/anomaly/weather",
];
/// `ASH_WALKER_SPAWN_THRESHOLD`.
pub const SPAWN_THRESHOLD: u32 = 2;
/// `meat_counter`'s initial value.
pub const START_MEAT: u32 = 6;
/// `SSprocessing` wait.
const PROCESS_SECS: f64 = 1.0;
/// `remake_walker`'s timer.
pub const REMAKE_SECS: f64 = 20.0;
/// The chance an ash walker's sacrifice unlocks their respawn.
const RESPAWN_UNLOCK_CHANCE: f32 = 40.0;
/// `"\improper Ash Walker"`.
pub const ANTAG_NAME: &str = "Ash Walker";
/// `/datum/team/ashwalkers` `name`, `member_name`.
const TEAM_NAME: &str = "Племя Пеплоходцев";
const MEMBER_NAME: &str = "Пеплоходцы";
const SACRIFICE_GOOD: crate::mood::MoodEvent = crate::mood::MoodEvent {
    id: "sacrifice_good",
    description: "Боги довольны этим подношением!",
    mood_change: 5.0,
    timeout: 180.0,
    hidden: false,
};
const SPECIAL_TEXT: &str = "Drag the corpses of men and beasts to your nest. It will absorb them \
     to create more of your kind. Invade the strange structure of the outsiders if you must. Do \
     not cause unnecessary destruction, as littering the wastes with ugly wreckage is certain to \
     not gain you favor. Glory to the Necropolis!";

/// `/datum/team/ashwalkers`.
#[derive(Clone, Debug)]
pub struct AshTeam {
    /// The nest of its `protect_object` objective.
    pub nest: Entity,
    pub nest_name: String,
    /// Each member's body and key, in the order they joined.
    pub members: Vec<(Entity, String)>,
    /// `players_spawned`: keys that hatched and have no bonus egg.
    pub players_spawned: Vec<String>,
    pub sacrifices_made: u32,
    pub eggs_created: u32,
}

/// Every tribe of the round (one per nest).
#[derive(Resource, Clone, Debug, Default)]
pub struct AshTeams(pub Vec<AshTeam>);

/// `/obj/structure/lavaland/ash_walker`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct AshNest {
    pub team: usize,
    /// `meat_counter`.
    pub meat: u32,
    next: f64,
}

/// An ash walker egg of `team`'s nest (the spawner's `team`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct AshEgg {
    pub team: usize,
}

/// `/datum/antagonist/ashwalker` on its body.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ashwalker {
    pub team: usize,
}

/// A dead ash walker the nest is remaking (tg: it sits inside the nest).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Remaking {
    nest_pos: TilePos,
    due: f64,
}

/// `GLOB.lizard_names_male`, `GLOB.lizard_names_female`.
#[derive(Resource, Clone, Debug, Default)]
struct LizardNames {
    male: Vec<String>,
    female: Vec<String>,
}

pub struct AshwalkerPlugin;

impl Plugin for AshwalkerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AshTeams>()
            .add_observer(nest_added)
            .add_systems(Startup, load_names)
            .add_systems(
                Update,
                (
                    process_nests.run_if(any_with_component::<AshNest>),
                    remake_walkers.run_if(any_with_component::<Remaking>),
                ),
            );
    }
}

fn load_names(world: &mut World) {
    let source: Option<PathBuf> = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.source.clone());
    let read = |file: &str| -> Vec<String> {
        source
            .as_ref()
            .and_then(|s| ss13_data::read(&s.join("strings/names").join(file)).ok())
            .map(|bytes| {
                String::from_utf8_lossy(&bytes)
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    world.insert_resource(LizardNames {
        male: read("lizard_male.txt"),
        female: read("lizard_female.txt"),
    });
}

/// `ash_walker/Initialize`: the tribe and its objective.
fn nest_added(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let e = add.entity;
    if !objs.get(e).is_ok_and(|o| o.path == NEST) {
        return;
    }
    commands.queue(move |world: &mut World| {
        if world.get_entity(e).is_err() || world.get::<AshNest>(e).is_some() {
            return;
        }
        let nest_name = crate::medical::name_of(world, e);
        let mut teams = world.resource_mut::<AshTeams>();
        teams.0.push(AshTeam {
            nest: e,
            nest_name,
            members: Vec::new(),
            players_spawned: Vec::new(),
            sacrifices_made: 0,
            eggs_created: 0,
        });
        let team = teams.0.len() - 1;
        world.entity_mut(e).insert(AshNest {
            team,
            meat: START_MEAT,
            next: 0.0,
        });
    });
}

/// Whether `path` is the nest (a breakable structure, `max_integrity = 200`).
pub fn breakable(path: &str) -> bool {
    path == NEST
}

/// `ash_walker/atom_deconstruct`: a random anomaly core on a random neighbouring tile.
pub fn deconstruct(world: &mut World, obj: &DmObj, pos: TilePos) -> bool {
    if obj.path != NEST {
        return false;
    }
    let (core, dir) = {
        let mut rng = world.resource_mut::<CombatRng>();
        let core = ANOMALY_CORES[rng.range(0, ANOMALY_CORES.len() as u32 - 1) as usize];
        (core, random_dir(&mut rng))
    };
    crate::construction::spawn_obj(world, core, pos.step(dir));
    true
}

fn random_dir(rng: &mut CombatRng) -> Dir {
    Dir::DMI_ORDER[rng.range(0, Dir::DMI_ORDER.len() as u32 - 1) as usize]
}

/// `IS_UNCONSCIOUS_OR_CRIT`.
fn out_or_crit(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) != Some(&Stat::Conscious)
        || crate::status_effects::knocked_out(world, mob)
}

/// `ash_walker/process`: `consume`, then `spawn_mob`, once a second.
// Cost: per second and nest (one per map), a pass over the living mobs.
fn process_nests(world: &mut World) {
    let now = now(world);
    let mut q = world.query::<(Entity, &mut AshNest, &TilePos)>();
    let due: Vec<(Entity, TilePos)> = q
        .iter_mut(world)
        .filter(|(_, n, _)| n.next <= now)
        .map(|(e, mut n, &p)| {
            n.next = now + PROCESS_SECS;
            (e, p)
        })
        .collect();
    for (nest, pos) in due {
        consume(world, nest, pos);
        spawn_egg(world, nest, pos);
    }
}

/// `consume`: the offerings next to the nest, until a cyborg or a returning ash walker.
fn consume(world: &mut World, nest: Entity, pos: TilePos) {
    let mut q = world.query_filtered::<(Entity, &TilePos), (With<Health>, Without<Ghost>)>();
    let mut near: Vec<Entity> = q
        .iter(world)
        .filter(|(_, p)| p.z == pos.z && (p.x - pos.x).abs() <= 1 && (p.y - pos.y).abs() <= 1)
        .map(|(e, _)| e)
        .collect();
    near.sort();
    for mob in near {
        if world.get::<Remaking>(mob).is_some() || !out_or_crit(world, mob) {
            continue;
        }
        if offer(world, nest, pos, mob) {
            return;
        }
    }
}

/// One offering of `consume`; true where tg's loop `return`s.
fn offer(world: &mut World, nest: Entity, nest_pos: TilePos, mob: Entity) -> bool {
    let Some(team) = world.get::<AshNest>(nest).map(|n| n.team) else {
        return true;
    };
    let name = crate::medical::name_of(world, mob);
    if world.get::<crate::silicon::cyborg::Cyborg>(mob).is_some() {
        crate::medical::visible(
            world,
            nest,
            "",
            &format!("Serrated tendrils eagerly pull {name} apart, but find nothing of interest."),
            Span::Notice,
        );
        crate::health::gib(world, mob);
        return true;
    }
    if world.get::<Ashwalker>(mob).is_some()
        && crate::medical::brainmob::mind_of(world, mob).is_some()
    {
        let nest_name = crate::medical::name_of(world, nest);
        crate::medical::visible(
            world,
            nest,
            "",
            &format!(
                "Serrated tendrils carefully pull {name} to {nest_name}, absorbing the body and \
                 creating it anew."
            ),
            Span::Warning,
        );
        let ghost = crate::medical::brainmob::ghost_of(world, mob);
        let soul = ghost.unwrap_or(mob);
        tell_mob(
            world,
            soul,
            Span::Plain,
            "Your body has been returned to the nest. You are being remade anew, and will awaken \
             shortly. Your memories will remain intact in your new body, as your soul is being \
             salvaged",
        );
        if let Some(&p) = world.get::<TilePos>(soul) {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(
                    p,
                    "sound/effects/magic/enter_blood.ogg",
                    100.0,
                    false,
                ),
            );
        }
        let due = now(world) + REMAKE_SECS;
        world
            .entity_mut(mob)
            .insert((nest_pos, Remaking { nest_pos, due }));
        return true;
    }
    let megafauna = world
        .get::<crate::mobs::megafauna::Megafauna>(mob)
        .is_some();
    if let Some(mut n) = world.get_mut::<AshNest>(nest) {
        n.meat += if megafauna { 20 } else { 1 };
    }
    let nest_name = crate::medical::name_of(world, nest);
    crate::medical::visible(
        world,
        nest,
        "",
        &format!(
            "Serrated tendrils eagerly pull {name} to {nest_name}, tearing the body apart as its \
             blood seeps over the eggs."
        ),
        Span::Warning,
    );
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(
            nest_pos,
            "sound/effects/magic/demon_consume.ogg",
            100.0,
            true,
        ),
    );
    // `fingerprintslast` of a dragged body: whoever drags it.
    if let Some(deliverer) = world.get::<crate::pull::PulledBy>(mob).map(|p| p.puller())
        && world.get::<Ashwalker>(deliverer).is_some()
    {
        let key = key_of(world, deliverer);
        let spawned = !key.is_empty()
            && world
                .resource::<AshTeams>()
                .0
                .get(team)
                .is_some_and(|t| t.players_spawned.contains(&key));
        if spawned
            && world
                .resource_mut::<CombatRng>()
                .chance(RESPAWN_UNLOCK_CHANCE)
        {
            tell_mob(
                world,
                deliverer,
                Span::Warning,
                "The Necropolis is pleased with your sacrifice. You feel confident your existence \
                 after death is secure.",
            );
            if let Some(t) = world.resource_mut::<AshTeams>().0.get_mut(team) {
                t.players_spawned.retain(|k| *k != key);
            }
        }
    }
    crate::health::gib(world, mob);
    // `atom_integrity + max_integrity * 0.05`, up to `max_integrity`.
    let max = world.get::<DmObj>(nest).map_or(0.0, |o| o.max_integrity);
    if let Some(mut integrity) = world.get_mut::<Integrity>(nest) {
        integrity.current = (integrity.current + max * 0.05).min(max);
    }
    let mut q = world.query::<(Entity, &Ashwalker, &TilePos)>();
    let pleased: Vec<Entity> = q
        .iter(world)
        .filter(|(_, _, p)| {
            p.z == nest_pos.z && (p.x - nest_pos.x).abs() <= 5 && (p.y - nest_pos.y).abs() <= 5
        })
        .map(|(e, _, _)| e)
        .collect();
    for walker in pleased {
        crate::mood::add_event(world, walker, "tendril sacrifice", &SACRIFICE_GOOD);
    }
    if let Some(t) = world.resource_mut::<AshTeams>().0.get_mut(team) {
        t.sacrifices_made += 1;
    }
    false
}

/// `spawn_mob`: an egg on a random neighbouring tile for every 2 meat (one a second).
fn spawn_egg(world: &mut World, nest: Entity, pos: TilePos) {
    let Some(n) = world.get::<AshNest>(nest).copied() else {
        return;
    };
    if n.meat < SPAWN_THRESHOLD {
        return;
    }
    let dir = random_dir(&mut world.resource_mut::<CombatRng>());
    let egg_pos = pos.step(dir);
    if let Some(mut nest) = world.get_mut::<AshNest>(nest) {
        nest.meat -= SPAWN_THRESHOLD;
    }
    if let Some(t) = world.resource_mut::<AshTeams>().0.get_mut(n.team) {
        t.eggs_created += 1;
    }
    crate::medical::visible(
        world,
        nest,
        "",
        "One of the eggs swells to an unnatural size and tumbles free. It's ready to hatch!",
        Span::Danger,
    );
    lay_egg(world, n.team, egg_pos);
}

/// `new /obj/effect/mob_spawn/ghost_role/human/ash_walker(loc, ashies)`: the egg shell with the
/// role, the ghosts told.
pub fn lay_egg(world: &mut World, team: usize, pos: TilePos) -> Option<Entity> {
    let Some(def) = crate::ghost_roles::def(world, EGG_SPAWNER) else {
        warn!("{EGG_SPAWNER}: no mob spawner data (rerun `ss13-mapload convert`)");
        return None;
    };
    let egg = crate::construction::spawn_obj(world, EGGSHELL, pos)?;
    world.entity_mut(egg).insert((
        GhostRoleSpawner {
            name: def.name.clone(),
            desc: crate::ghost_roles::role_text(&def),
            uses: def.uses.max(1),
            def: Some(EGG_SPAWNER.to_owned()),
        },
        AshEgg { team },
    ));
    let area = crate::bots::area_name(world, pos);
    let alert = format!("An ash walker egg is ready to hatch in {area}.");
    for ghost in crate::ghost_poll::ghosts(world) {
        tell_mob(world, ghost, Span::Notice, alert.clone());
    }
    Some(egg)
}

fn key_of(world: &World, mob: Entity) -> String {
    world
        .get::<PlayerMob>(mob)
        .and_then(|p| world.get::<ClientToken>(p.client))
        .map(|t| t.0.clone())
        .unwrap_or_default()
}

/// The ash walker spawner's `allow_spawn`: true for any other spawner.
pub fn allow_spawn(world: &mut World, client: Entity, spawner: Entity) -> bool {
    let Some(egg) = world.get::<AshEgg>(spawner).copied() else {
        return true;
    };
    let key = world
        .get::<ClientToken>(client)
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let spent = world
        .resource::<AshTeams>()
        .0
        .get(egg.team)
        .is_none_or(|t| t.players_spawned.contains(&key));
    if spent {
        tell(
            world,
            client,
            Span::Warning,
            "You have exhausted your usefulness to the Necropolis.",
        );
    }
    !spent
}

/// A Draconic name (`/datum/language/draconic/get_random_name`): two names of one gender's list
/// joined by "-"; `None` without the lists.
fn draconic_name(world: &mut World) -> Option<String> {
    let names = world.get_resource::<LizardNames>()?.clone();
    let mut rng = world.resource_mut::<CombatRng>();
    let list = if rng.chance(50.0) {
        &names.male
    } else {
        &names.female
    };
    if list.is_empty() {
        return None;
    }
    let mut pick = || list[rng.range(0, list.len() as u32 - 1) as usize].clone();
    Some(format!("{}-{}", pick(), pick()))
}

/// The ash walker spawner's `special` on the hatchling `mob` of `client`: nothing for other
/// spawners.
pub fn special(world: &mut World, mob: Entity, client: Entity, spawner: Entity) {
    let Some(egg) = world.get::<AshEgg>(spawner).copied() else {
        return;
    };
    if let Some(name) = draconic_name(world) {
        world.entity_mut(mob).insert(Name::new(name));
    }
    tell(world, client, Span::Bold, SPECIAL_TEXT);
    add_ashwalker(world, mob, egg.team);
    let key = world
        .get::<ClientToken>(client)
        .map(|t| t.0.clone())
        .unwrap_or_default();
    if let Some(t) = world.resource_mut::<AshTeams>().0.get_mut(egg.team) {
        t.players_spawned.push(key);
    }
}

/// `add_antag_datum(/datum/antagonist/ashwalker, team)` + `remove_language(common)`: the
/// greeting, the tribe, no neutral faction, no Galactic Common.
pub fn add_ashwalker(world: &mut World, mob: Entity, team: usize) {
    if world.get::<Ashwalker>(mob).is_some() {
        return;
    }
    let key = key_of(world, mob);
    world.entity_mut(mob).insert(Ashwalker { team });
    if let Some(t) = world.resource_mut::<AshTeams>().0.get_mut(team) {
        t.members.push((mob, key));
    }
    if let Some(mut faction) = world.get_mut::<crate::mobs::Faction>(mob) {
        faction.0.retain(|f| f != crate::mobs::Faction::NEUTRAL);
    }
    ss13_chat::language::edit_holder(world, mob, |h| {
        h.remove(
            ss13_chat::language::LanguageId::Common,
            ss13_chat::language::ALL,
            None,
        );
    });
    // `greet` (the default): "Вы - Ash Walker."
    tell_mob(world, mob, Span::Bold, format!("Вы - {ANTAG_NAME}."));
}

/// The ash walkers whose remaking is due.
fn remake_walkers(world: &mut World) {
    let now = now(world);
    let mut q = world.query::<(Entity, &Remaking)>();
    let due: Vec<(Entity, Remaking)> = q
        .iter(world)
        .filter(|(_, r)| r.due <= now)
        .map(|(e, r)| (e, *r))
        .collect();
    for (old, r) in due {
        remake_walker(world, old, r.nest_pos);
    }
}

/// `remake_walker`: a new nude ash walker next to the nest with the old one's name, mind and
/// tribe; the old body goes.
pub fn remake_walker(world: &mut World, old: Entity, nest_pos: TilePos) -> Option<Entity> {
    world.entity_mut(old).remove::<Remaking>();
    let team = world.get::<Ashwalker>(old)?.team;
    let pos = nest_pos.step(random_dir(&mut world.resource_mut::<CombatRng>()));
    let seed = world.resource_mut::<CombatRng>().range(0, u32::MAX);
    let mut look = crate::human::AppearancePrefs::random(u64::from(seed)).body();
    look.species = crate::species::SpeciesId::Ashwalker;
    let name = crate::medical::name_of(world, old);
    let new = crate::server::spawn_body(world, pos, name, look);
    let mut inventory = crate::inventory::Inventory::default();
    crate::species::equip_species_outfit(world, new, &mut inventory);
    world.entity_mut(new).insert(inventory);
    crate::inventory::refresh_look(world, new);
    ss13_chat::language::edit_holder(world, new, |h| {
        h.remove(
            ss13_chat::language::LanguageId::Common,
            ss13_chat::language::ALL,
            None,
        );
    });
    // `mind.transfer_to(newwalker)` + `grab_ghost()`.
    let ghost = crate::medical::brainmob::ghost_of(world, old);
    crate::medical::brainmob::transfer_mind(world, old, new);
    if ghost.is_some() {
        crate::ghost::return_to_body(world, new);
    }
    world.entity_mut(old).remove::<Ashwalker>();
    world.entity_mut(new).insert(Ashwalker { team });
    if let Some(mut faction) = world.get_mut::<crate::mobs::Faction>(new) {
        faction.0.retain(|f| f != crate::mobs::Faction::NEUTRAL);
    }
    if let Some(t) = world.resource_mut::<AshTeams>().0.get_mut(team) {
        for member in &mut t.members {
            if member.0 == old {
                member.0 = new;
            }
        }
    }
    tell_mob(
        world,
        new,
        Span::Bold,
        "You have been pulled back from beyond the grave, with a new body and renewed purpose. \
         Glory to the Necropolis!",
    );
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, "sound/effects/magic/exit_blood.ogg", 100.0, true),
    );
    world.despawn(old);
    Some(new)
}

/// `protect_object/check_completion`: the nest still stands.
pub fn nest_stands(world: &World, team: &AshTeam) -> bool {
    world.get::<AshNest>(team.nest).is_some()
}

/// `/datum/team/ashwalkers/roundend_report` of every tribe.
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let teams = world.resource::<AshTeams>().0.clone();
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = Vec::new();
    for team in teams {
        out.push(line(vec![ChatSegment::new(
            Span::Bold,
            "Племя Пеплоходцев населяло пустоши...",
        )]));
        if team.members.is_empty() {
            out.push(line(vec![ChatSegment::new(
                Span::Bold,
                "Но ни одно из яиц не вылупилось!",
            )]));
            continue;
        }
        out.push(line(vec![ChatSegment::new(
            Span::Plain,
            format!("{MEMBER_NAME} были:"),
        )]));
        for (mob, key) in &team.members {
            out.push(super::traitor::print_player(world, *mob, key));
        }
        out.push(line(vec![ChatSegment::new(
            Span::Bold,
            "Племени Пеплоходцев было поручено защищать Некрополь:",
        )]));
        out.push(line(vec![if nest_stands(world, &team) {
            ChatSegment::new(Span::GreenText, "Гнездо уцелело! Слава Некрополю!")
        } else {
            ChatSegment::new(Span::RedText, "Некрополь был разрушен. Племя пало...")
        }]));
        out.push(line(vec![ChatSegment::new(
            Span::Plain,
            format!(
                "{TEAM_NAME} сумело принести {} жертв Некрополю. Взамен Некрополь породил {} яиц \
                 Пеплоходцев.",
                team.sacrifices_made, team.eggs_created
            ),
        )]));
    }
    out
}

/// The objective text of a tribe (`protect_object/update_explanation_text`).
pub fn objective_text(team: &AshTeam) -> String {
    format!("Защитите любой ценой - {}.", team.nest_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_with_nest() -> (World, Entity) {
        let mut world = World::new();
        world.insert_resource(CombatRng(7));
        world.insert_resource(Time::<()>::default());
        world.init_resource::<AshTeams>();
        let nest = world.spawn(TilePos::new(5, 5, 1)).id();
        world.resource_mut::<AshTeams>().0.push(AshTeam {
            nest,
            nest_name: "necropolis tendril nest".into(),
            members: Vec::new(),
            players_spawned: vec!["walker".into()],
            sacrifices_made: 0,
            eggs_created: 0,
        });
        world.entity_mut(nest).insert(AshNest {
            team: 0,
            meat: START_MEAT,
            next: 0.0,
        });
        (world, nest)
    }

    #[test]
    fn one_egg_per_player_until_the_necropolis_gives_another() {
        let (mut world, _) = world_with_nest();
        let egg = world.spawn(AshEgg { team: 0 }).id();
        let other = world.spawn_empty().id();
        let spent = world.spawn(ClientToken("walker".into())).id();
        let fresh = world.spawn(ClientToken("newcomer".into())).id();
        assert!(allow_spawn(&mut world, fresh, egg));
        assert!(!allow_spawn(&mut world, spent, egg));
        // Not an ash walker egg: no limit.
        assert!(allow_spawn(&mut world, spent, other));
        world.resource_mut::<AshTeams>().0[0]
            .players_spawned
            .clear();
        assert!(allow_spawn(&mut world, spent, egg));
    }

    #[test]
    fn the_objective_holds_while_the_nest_stands() {
        let (mut world, nest) = world_with_nest();
        let team = world.resource::<AshTeams>().0[0].clone();
        assert!(nest_stands(&world, &team));
        assert_eq!(
            objective_text(&team),
            "Защитите любой ценой - necropolis tendril nest."
        );
        world.despawn(nest);
        assert!(!nest_stands(&world, &team));
    }

    #[test]
    fn the_nest_lays_an_egg_per_two_meat() {
        let (mut world, nest) = world_with_nest();
        let pos = TilePos::new(5, 5, 1);
        // No spawner data in a bare world: the meat is spent, the egg counted.
        spawn_egg(&mut world, nest, pos);
        assert_eq!(
            world.get::<AshNest>(nest).unwrap().meat,
            START_MEAT - SPAWN_THRESHOLD
        );
        assert_eq!(world.resource::<AshTeams>().0[0].eggs_created, 1);
        world.get_mut::<AshNest>(nest).unwrap().meat = 1;
        spawn_egg(&mut world, nest, pos);
        assert_eq!(world.get::<AshNest>(nest).unwrap().meat, 1);
        assert_eq!(world.resource::<AshTeams>().0[0].eggs_created, 1);
    }
}
