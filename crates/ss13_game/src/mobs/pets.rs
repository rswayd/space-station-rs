//! Pets (server): friends, taming, pet commands and breeding, after tg `befriend`/`unfriend`
//! (`mob/living/living.dm`), `/datum/component/tameable`, `/datum/component/obeys_commands` with
//! the `/datum/pet_command`s of `pet_commands_basic.dm`, and `/datum/component/breed`
//! (`breeding.dm`) with `find_partner`/`make_babies` (`ai/babies`).
//!
//! - [`Friends`] is tg `BB_FRIENDS_LIST` and the ally factions `befriend` adds: a mob never
//!   targets its friends ([`crate::mobs::same_faction`]).
//! - Taming ([`Tameable`]): a mob fed its favourite food by hand (`crate::mobs::eating`) is
//!   tamed with `tame_chance` percent, else the chance grows by `bonus_tame_chance`; the tamer
//!   becomes a friend. Cows, pigs, ponies, carp (who then obey), parrots, dogs, mice and snakes.
//! - Pet commands ([`ObeysCommands`]): a friend in view says a command word (tg
//!   `respond_to_command` on `COMSIG_MOB_SAY`): stay, loose, follow, attack (then point at the
//!   victim), play dead, good boy. The command runs in the pet command subtree that heads the
//!   mob's tree (`trees::pet_command_root`, tg `SUBPLAN_ID_PET_COMMAND`).
//! - Breeding ([`Breeds`]): cats and corgis look for a partner of the other sex in view (at most
//!   3 of their kind around), walk up to it and a baby appears; then 40 s before the next.
// shortcut: no radial menu or callouts for commands (voice and pointing only), no balloon
// alerts (`command_feedback`), no `move`/`fetch`/trick commands, no beast empathy or pet renaming;
// add the radial with a radial menu UI.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_chat::server::Spoke;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::mobs::MobDef;
use ss13_shared::TilePos;

use super::BasicMob;
use super::ai::{AiController, Key, Value};
use crate::combat::CombatRng;
use crate::health::Stat;

/// `MOB_SIZE_HUMAN`: the size of anything that is not a basic mob.
pub const MOB_SIZE_HUMAN: u8 = 2;
/// The pet command's `sense_radius`.
const SENSE_RADIUS: u32 = 7;
/// `FIND_PARTNER_COOLDOWN`, `find_partner` `range`, `max_nearby_pop`.
const FIND_PARTNER_COOLDOWN: f64 = 60.0;
const PARTNER_RANGE: u32 = 7;
const MAX_NEARBY_POP: usize = 3;
/// `/datum/component/breed` `breed_timer` default.
const BREED_TIMER: f64 = 40.0;

/// tg `/datum/pet_command` subtypes that are ported.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PetCommand {
    /// `idle`: stay put.
    Stay,
    /// `free`: back to its own behaviour.
    Free,
    /// `follow`: follow the commander.
    Follow,
    /// `attack`: bite what the commander points at.
    Attack,
    /// `play_dead`.
    PlayDead,
    /// `good_boy` (hidden): a spin and back to its own behaviour.
    GoodBoy,
}

impl PetCommand {
    /// `speech_commands` (BandaStation adds the Russian ones). `good_boy` adds the mob's name.
    fn words(self) -> &'static [&'static str] {
        match self {
            Self::Stay => &[
                "sit",
                "stay",
                "stop",
                "сидеть",
                "лежать",
                "место",
                "фу",
                "стоп",
                "стой",
                "стоять",
            ],
            Self::Free => &["free", "loose", "гулять", "вон", "свобод", "брысь"],
            Self::Follow => &[
                "heel",
                "follow",
                "за мной",
                "след",
                "охран",
                "к ноге",
                "ко мне",
            ],
            Self::Attack => &["attack", "sic", "kill", "апорт", "фас", "бить", "атак"],
            Self::PlayDead => &["play dead", "притворись мертвым", "умри"],
            Self::GoodBoy => &["хорош"],
        }
    }

    /// tg `find_command_in_text` (DM `findtext` ignores case).
    pub fn heard_in(self, text: &str, pet_name: &str, gender: Gender) -> bool {
        let text = text.to_lowercase();
        if self.words().iter().any(|w| text.contains(w)) {
            return true;
        }
        if self != Self::GoodBoy {
            return false;
        }
        let name = format!("good {}", pet_name.to_lowercase());
        text.contains(&name)
            || match gender {
                Gender::Male => text.contains("good boy"),
                Gender::Female => text.contains("good girl"),
                Gender::Neuter => false,
            }
    }
}

/// A basic mob's gender (tg `gender`; `PLURAL` is randomized at `Initialize`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gender {
    Male,
    Female,
    Neuter,
}

/// tg `BB_FRIENDS_LIST` (and the `REF(friend)` factions of `add_ally`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Friends(pub Vec<Entity>);

/// `/datum/component/tameable` (`current_tame_chance`).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Tameable {
    pub foods: &'static [&'static str],
    pub chance: f32,
    pub bonus: f32,
    pub current: f32,
    /// `unique`: the component goes away once tamed.
    pub unique: bool,
}

/// `/datum/component/obeys_commands`: its commands, and whether `follow` starts on befriending
/// (`/datum/pet_command/follow/start_active`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ObeysCommands {
    pub commands: &'static [PetCommand],
    pub follow_on_befriend: bool,
}

/// `/datum/component/breed`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Breeds {
    /// `can_breed_with` and `BB_BABIES_PARTNER_TYPES`.
    pub partners: &'static str,
    /// `baby_paths` and `BB_BABIES_CHILD_TYPES`.
    pub baby: &'static str,
    /// Game time `ready_to_breed` comes back (0: ready).
    pub ready_at: f64,
}

struct TameDef {
    root: &'static str,
    foods: &'static [&'static str],
    chance: f32,
    bonus: f32,
    unique: bool,
}

/// The `tameable` components of the ported mobs (their `Initialize`/`make_tameable`).
static TAMES: [TameDef; 9] = [
    TameDef {
        root: "/mob/living/basic/vatbeast",
        foods: &[
            "/obj/item/food/carrotfries",
            "/obj/item/food/cheesyfries",
            "/obj/item/food/cornchips",
            "/obj/item/food/fries",
        ],
        chance: 30.0,
        bonus: 0.0,
        unique: true,
    },
    TameDef {
        root: "/mob/living/basic/cow",
        foods: &["/obj/item/food/grown/wheat"],
        chance: 25.0,
        bonus: 15.0,
        unique: true,
    },
    TameDef {
        root: "/mob/living/basic/pig",
        foods: &["/obj/item/food/grown/carrotlike/carrot"],
        chance: 25.0,
        bonus: 15.0,
        unique: true,
    },
    TameDef {
        root: "/mob/living/basic/pony",
        foods: &["/obj/item/food/grown/apple"],
        chance: 25.0,
        bonus: 15.0,
        unique: false,
    },
    // BandaStation: carp take any meat (`CROCODILE` edit).
    TameDef {
        root: "/mob/living/basic/carp",
        foods: &["/obj/item/food/meat"],
        chance: 10.0,
        bonus: 5.0,
        unique: true,
    },
    TameDef {
        root: "/mob/living/basic/parrot",
        foods: &["/obj/item/food/cracker"],
        chance: 100.0,
        bonus: 0.0,
        unique: true,
    },
    TameDef {
        root: "/mob/living/basic/pet/dog",
        foods: &[
            "/obj/item/food/meat/slab/human/mutant/skeleton",
            "/obj/item/stack/sheet/bone",
        ],
        chance: 30.0,
        bonus: 15.0,
        unique: false,
    },
    TameDef {
        root: "/mob/living/basic/mouse",
        foods: &["/obj/item/food/cheese"],
        chance: 100.0,
        bonus: 0.0,
        unique: true,
    },
    TameDef {
        root: "/mob/living/basic/snake",
        foods: &["/obj/item/food/deadmouse"],
        chance: 75.0,
        bonus: 10.0,
        unique: true,
    },
];

const DOG_COMMANDS: &[PetCommand] = &[
    PetCommand::Stay,
    PetCommand::Free,
    PetCommand::GoodBoy,
    PetCommand::Follow,
    PetCommand::Attack,
    PetCommand::PlayDead,
];
const FOX_COMMANDS: &[PetCommand] = &[
    PetCommand::Stay,
    PetCommand::Free,
    PetCommand::Follow,
    PetCommand::Attack,
];
/// Cats, mice, mothroaches, parrots (their `perform_trick_sequence` is not ported).
const SMALL_PET_COMMANDS: &[PetCommand] = &[PetCommand::Stay, PetCommand::Free, PetCommand::Follow];
/// The carp's `tamed_commands`, granted when tamed.
const CARP_COMMANDS: &[PetCommand] = &[
    PetCommand::Stay,
    PetCommand::Free,
    PetCommand::Follow,
    PetCommand::Attack,
];

/// The commands mob type `path` obeys from the start (`None`: none, or only once tamed).
fn commands_of(path: &str) -> Option<ObeysCommands> {
    let under = |root: &str| is_under(path, root);
    let (commands, follow_on_befriend) = if under("/mob/living/basic/pet/dog") {
        (DOG_COMMANDS, false)
    } else if under("/mob/living/basic/pet/fox") {
        (FOX_COMMANDS, true)
    } else if under("/mob/living/basic/pet/cat")
        || (under("/mob/living/basic/mouse") && !under("/mob/living/basic/mouse/rat"))
        || under("/mob/living/basic/mothroach")
    {
        (SMALL_PET_COMMANDS, true)
    } else if under("/mob/living/basic/parrot") {
        (SMALL_PET_COMMANDS, false)
    } else {
        return None;
    };
    Some(ObeysCommands {
        commands,
        follow_on_befriend,
    })
}

/// Whether mob type `path` heads its tree with the pet command subtree (its tg tree has the
/// `SUBPLAN_ID_PET_COMMAND` override slot): the obedient ones and carp.
pub fn has_pet_subtree(path: &str) -> bool {
    commands_of(path).is_some() || is_under(path, "/mob/living/basic/carp")
}

/// tg `/datum/component/breed` of the cat and corgi `add_breeding_component` (kittens and
/// puppies do not breed: `can_breed = FALSE`, `BB_FUCKS = FALSE`).
fn breeding_of(path: &str) -> Option<Breeds> {
    let under = |root: &str| is_under(path, root);
    let (partners, baby) = if under("/mob/living/basic/pet/cat") {
        if under("/mob/living/basic/pet/cat/kitten") {
            return None;
        }
        (
            "/mob/living/basic/pet/cat",
            "/mob/living/basic/pet/cat/kitten",
        )
    } else if under("/mob/living/basic/pet/dog/corgi") {
        if under("/mob/living/basic/pet/dog/corgi/puppy") {
            return None;
        }
        (
            "/mob/living/basic/pet/dog/corgi",
            "/mob/living/basic/pet/dog/corgi/puppy",
        )
    } else {
        return None;
    };
    Some(Breeds {
        partners,
        baby,
        ready_at: 0.0,
    })
}

pub(super) fn build(app: &mut App) {
    app.add_systems(
        Update,
        (hear_commands, point_commands).after(crate::health::HealthSystems),
    );
}

/// The pet parts of a new basic mob (`Initialize`).
pub(super) fn attach(world: &mut World, mob: Entity, def: &MobDef) {
    let gender = match def.gender.as_str() {
        "male" => Gender::Male,
        "female" => Gender::Female,
        "neuter" => Gender::Neuter,
        // `PLURAL` (the basic mob default): `pick(MALE, FEMALE)`.
        _ => {
            let mut rng = CombatRng(world.resource::<CombatRng>().0);
            let male = rng.chance(50.0);
            world.resource_mut::<CombatRng>().0 = rng.0;
            if male { Gender::Male } else { Gender::Female }
        }
    };
    let mut e = world.entity_mut(mob);
    e.insert((gender, Friends::default()));
    if let Some(tame) = TAMES.iter().find(|t| is_under(&def.path, t.root))
        // Space rats never take to anyone; Tom is tame from the start (`TRAIT_TAMED`).
        && !is_under(&def.path, "/mob/living/basic/mouse/rat")
        && !is_under(&def.path, "/mob/living/basic/mouse/brown/tom")
        // The wisdom cow's `setup_eating` (where cows become tameable) does nothing.
        && !is_under(&def.path, super::farm::WISDOM_COW)
    {
        e.insert(Tameable {
            foods: tame.foods,
            chance: tame.chance,
            bonus: tame.bonus,
            current: tame.chance,
            unique: tame.unique,
        });
    }
    if let Some(obeys) = commands_of(&def.path) {
        e.insert(obeys);
    }
    if let Some(breeds) = breeding_of(&def.path) {
        e.insert(breeds);
        if let Some(mut c) = e.get_mut::<AiController>() {
            c.bb.set(Key::BreedReady, Value::Bool(true));
            // tg sets `BB_FUCKS` on cats only; the corgi tree has the same `find_partner` and
            // its puppies opt out explicitly, so corgis breed too (the flag is missing in tg).
            c.bb.set(Key::WantsToBreed, Value::Bool(true));
        }
    }
    if is_under(&def.path, "/mob/living/basic/mouse/brown/tom") {
        add_faction(world, mob, super::Faction::NEUTRAL);
    }
}

fn add_faction(world: &mut World, mob: Entity, faction: &str) {
    if let Some(mut f) = world.get_mut::<super::Faction>(mob)
        && !f.0.iter().any(|x| x == faction)
    {
        f.0.push(faction.to_owned());
    }
}

/// `friend in BB_FRIENDS_LIST`.
pub fn is_friend(world: &World, mob: Entity, other: Entity) -> bool {
    world
        .get::<Friends>(mob)
        .is_some_and(|f| f.0.contains(&other))
}

/// tg `has_ally` either way: a pet and its friend never count as enemies.
pub fn allied(world: &World, a: Entity, b: Entity) -> bool {
    is_friend(world, a, b) || is_friend(world, b, a)
}

/// tg `mob_size` (`MOB_SIZE_HUMAN` for anything but a basic mob).
pub fn mob_size(world: &World, mob: Entity) -> u8 {
    world
        .get::<BasicMob>(mob)
        .map_or(MOB_SIZE_HUMAN, |b| b.def.mob_size)
}

pub fn gender_of(world: &World, mob: Entity) -> Gender {
    world.get::<Gender>(mob).copied().unwrap_or(Gender::Neuter)
}

/// tg `befriend`: `false` when already friends.
pub fn befriend(world: &mut World, mob: Entity, friend: Entity) -> bool {
    if mob == friend || is_friend(world, mob, friend) {
        return false;
    }
    match world.get_mut::<Friends>(mob) {
        Some(mut f) => f.0.push(friend),
        None => {
            world.entity_mut(mob).insert(Friends(vec![friend]));
        }
    }
    // `/datum/pet_command/follow/start_active` `add_new_friend`.
    let follow = world
        .get::<ObeysCommands>(mob)
        .is_some_and(|o| o.follow_on_befriend);
    let idle = world
        .get::<AiController>(mob)
        .is_some_and(|c| c.bb.get(Key::ActivePetCommand).is_none());
    if follow && idle {
        activate(world, mob, friend, PetCommand::Follow);
    }
    true
}

/// tg `unfriend`.
pub fn unfriend(world: &mut World, mob: Entity, friend: Entity) -> bool {
    let Some(mut f) = world.get_mut::<Friends>(mob) else {
        return false;
    };
    let before = f.0.len();
    f.0.retain(|&e| e != friend);
    before != f.0.len()
}

/// Whether `food` (a DM type) is one of the mob's taming foods.
pub fn is_tame_food(world: &World, mob: Entity, food_path: &str) -> bool {
    world
        .get::<Tameable>(mob)
        .is_some_and(|t| t.foods.iter().any(|root| is_under(food_path, root)))
}

/// tg `tameable/try_tame` (`COMSIG_MOB_ATE` from the feeder's hand).
pub fn try_tame(world: &mut World, mob: Entity, feeder: Entity, food_path: &str) {
    if !is_tame_food(world, mob, food_path) || is_friend(world, mob, feeder) {
        return;
    }
    let Some(chance) = world.get::<Tameable>(mob).map(|t| t.current) else {
        return;
    };
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let tamed = rng.chance(chance);
    world.resource_mut::<CombatRng>().0 = rng.0;
    if !tamed {
        if let Some(mut t) = world.get_mut::<Tameable>(mob) {
            t.current += t.bonus;
        }
        return;
    }
    tame(world, mob, feeder);
}

/// tg `tameable/on_tame`: the mob's `tamed`, then `befriend`.
pub fn tame(world: &mut World, mob: Entity, tamer: Entity) {
    tamed(world, mob, tamer);
    befriend(world, mob, tamer);
    let unique = world.get::<Tameable>(mob).is_none_or(|t| t.unique);
    if unique {
        world.entity_mut(mob).remove::<Tameable>();
    } else if let Some(mut t) = world.get_mut::<Tameable>(mob) {
        t.current = t.chance;
    }
}

/// The ported `tamed()` overrides.
fn tamed(world: &mut World, mob: Entity, tamer: Entity) {
    let Some(path) = world.get::<BasicMob>(mob).map(|b| b.def.path.clone()) else {
        return;
    };
    let under = |root: &str| is_under(&path, root);
    let tamer_name = super::defense::name_of(world, tamer);
    if under(super::oneoffs::VATBEAST) {
        super::oneoffs::vatbeast_tamed(world, mob);
    } else if under("/mob/living/basic/cow") {
        super::defense::emote(
            world,
            mob,
            &format!("радостно мычит, похоже, привязавшись к {tamer_name}."),
        );
    } else if under("/mob/living/basic/pony") {
        if let Some(&pos) = world.get::<TilePos>(mob) {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(
                    pos,
                    "sound/mobs/non-humanoids/pony/snort.ogg",
                    50.0,
                    true,
                ),
            );
        }
        super::defense::emote(world, mob, "радостно фыркает.");
    } else if under("/mob/living/basic/carp") {
        // `tamed_commands`; BandaStation's message.
        world.entity_mut(mob).insert(ObeysCommands {
            commands: CARP_COMMANDS,
            follow_on_befriend: false,
        });
        super::defense::emote(
            world,
            mob,
            &format!("крутится на месте и, видимо, сближается с {tamer_name}."),
        );
    } else if under("/mob/living/basic/pet/dog") {
        super::defense::emote(world, mob, &format!("дружелюбно облизывает {tamer_name}!"));
    } else if under("/mob/living/basic/mouse") {
        // `add_faction(FACTION_NEUTRAL)` and `cancel_current_plan` (stops fleeing).
        add_faction(world, mob, super::Faction::NEUTRAL);
        if let Some(mut c) = world.get_mut::<AiController>(mob) {
            c.bb.clear(Key::CurrentTarget);
            c.cancel = true;
        }
    }
}

/// tg `pet_able_to_respond`: a conscious mob with a controller, not tipped over.
fn able_to_respond(world: &World, mob: Entity) -> bool {
    world.get::<AiController>(mob).is_some()
        && world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && !super::farm::immobilized(world, mob)
}

/// tg `set_command_active` (+ `execute_action`): the command goes on the blackboard and the
/// current plan is dropped, so the pet command subtree takes over at the next tick.
pub fn activate(world: &mut World, mob: Entity, commander: Entity, command: PetCommand) {
    if !able_to_respond(world, mob) {
        return;
    }
    if command == PetCommand::GoodBoy {
        // `good_boy/execute_action`: back to its own ways, a heart and a spin.
        super::defense::emote(world, mob, "кружится на месте!");
    }
    let Some(mut c) = world.get_mut::<AiController>(mob) else {
        return;
    };
    c.bb.clear(Key::PetTarget);
    match command {
        PetCommand::Free | PetCommand::GoodBoy => c.bb.clear(Key::ActivePetCommand),
        _ => c.bb.set(Key::ActivePetCommand, Value::Pet(command)),
    }
    if command == PetCommand::Follow {
        c.bb.set(Key::PetTarget, Value::Entity(commander));
    }
    c.cancel = true;
}

/// Friends' speech near their pets (tg `respond_to_command` on the friend's `COMSIG_MOB_SAY`).
fn hear_commands(world: &mut World, mut cursor: Local<MessageCursor<Spoke>>) {
    if !world.contains_resource::<Messages<Spoke>>() {
        return;
    }
    let spoken: Vec<Spoke> = cursor
        .read(world.resource::<Messages<Spoke>>())
        .cloned()
        .collect();
    if spoken.is_empty() {
        return;
    }
    let pets: Vec<(Entity, ObeysCommands)> = world
        .query::<(Entity, &ObeysCommands)>()
        .iter(world)
        .map(|(e, o)| (e, *o))
        .collect();
    for said in spoken {
        for &(pet, obeys) in &pets {
            if !is_friend(world, pet, said.speaker) || !able_to_respond(world, pet) {
                continue;
            }
            let Some(&at) = world.get::<TilePos>(pet) else {
                continue;
            };
            if !super::ai::behaviors::can_see(world, at, said.pos, SENSE_RADIUS) {
                continue;
            }
            let name = super::defense::name_of(world, pet);
            let gender = gender_of(world, pet);
            // Every command that hears itself activates in turn: the last one stays.
            let heard = obeys
                .commands
                .iter()
                .copied()
                .rev()
                .find(|c| c.heard_in(&said.text, &name, gender));
            if let Some(command) = heard {
                activate(world, pet, said.speaker, command);
            }
        }
    }
}

/// A friend points at something (tg `point_on_target` of `requires_pointing` commands): the
/// attack command takes it as its target, or refuses a friend.
fn point_commands(world: &mut World, mut cursor: Local<MessageCursor<crate::click::PointClick>>) {
    if !world.contains_resource::<Messages<crate::click::PointClick>>() {
        return;
    }
    let clicks: Vec<crate::click::PointClick> = cursor
        .read(world.resource::<Messages<crate::click::PointClick>>())
        .copied()
        .collect();
    for click in clicks {
        if world.get::<Stat>(click.mob) != Some(&Stat::Conscious) {
            continue;
        }
        let pets: Vec<Entity> = world
            .query_filtered::<Entity, With<ObeysCommands>>()
            .iter(world)
            .collect();
        for pet in pets {
            if !is_friend(world, pet, click.mob) || !able_to_respond(world, pet) {
                continue;
            }
            let attacking = world.get::<AiController>(pet).is_some_and(|c| {
                c.bb.get(Key::ActivePetCommand) == Some(&Value::Pet(PetCommand::Attack))
                    && c.bb.entity(Key::PetTarget) != Some(click.target)
            });
            if !attacking {
                continue;
            }
            let (Some(&at), Some(&to)) = (
                world.get::<TilePos>(pet),
                world.get::<TilePos>(click.target),
            ) else {
                continue;
            };
            if !super::ai::behaviors::can_see(world, at, to, SENSE_RADIUS) {
                continue;
            }
            // `attack/set_command_target`: the pet targeting strategy (`basic/not_friends`).
            let valid = click.target != pet
                && world.get::<crate::health::Health>(click.target).is_some()
                && world
                    .get::<Stat>(click.target)
                    .is_some_and(|s| *s != Stat::Dead)
                && !is_friend(world, pet, click.target);
            if !valid {
                let target = super::defense::name_of(world, click.target);
                super::defense::emote(world, pet, &format!("отказывается нападать на {target}."));
                continue;
            }
            if let Some(mut c) = world.get_mut::<AiController>(pet) {
                c.bb.set(Key::PetTarget, Value::Entity(click.target));
            }
        }
    }
}

/// What [`find_partner`] saw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartnerSearch {
    Found(Entity),
    /// Nobody fit.
    Nobody,
    /// `max_nearby_pop` of its kind around: no search for `FIND_PARTNER_COOLDOWN`.
    Crowded,
    /// Something of another faction in view (tg fails the search).
    Watched,
}

/// The partner search's cooldown after a crowded look around.
pub const PARTNER_TIMEOUT: f64 = FIND_PARTNER_COOLDOWN;

/// tg `find_partner` `perform`: a breeder of the other sex in view, unless the place is
/// crowded with its kind or something of another faction is around.
pub fn find_partner(world: &World, pawn: Entity) -> PartnerSearch {
    let (Some(breeds), Some(&from)) = (
        world.get::<Breeds>(pawn).copied(),
        world.get::<TilePos>(pawn),
    ) else {
        return PartnerSearch::Nobody;
    };
    let gender = gender_of(world, pawn);
    let mut pop = 0;
    let mut partners = Vec::new();
    let mut seen: Vec<Entity> = world
        .resource::<ss13_world::SpatialIndex>()
        .in_radius(from, PARTNER_RANGE)
        .filter(|&e| e != pawn && world.get::<crate::health::Health>(e).is_some())
        .collect();
    seen.sort_by_key(|e| e.to_bits());
    for other in seen {
        let Some(&at) = world.get::<TilePos>(other) else {
            continue;
        };
        if !super::ai::behaviors::can_see(world, from, at, PARTNER_RANGE) {
            continue;
        }
        if !super::same_faction(world, pawn, other) {
            return PartnerSearch::Watched;
        }
        let Some(path) = world.get::<BasicMob>(other).map(|b| b.def.path.as_str()) else {
            continue;
        };
        if !is_under(path, breeds.partners) && !is_under(path, breeds.baby) {
            continue;
        }
        pop += 1;
        if pop >= MAX_NEARBY_POP {
            return PartnerSearch::Crowded;
        }
        let breeder = world.get::<Breeds>(other).is_some();
        let player = world.get::<crate::server::PlayerMob>(other).is_some();
        let conscious = world.get::<Stat>(other) == Some(&Stat::Conscious);
        if breeder && !player && conscious && gender_of(world, other) != gender {
            partners.push(other);
        }
    }
    partners
        .first()
        .copied()
        .map_or(PartnerSearch::Nobody, PartnerSearch::Found)
}

/// tg `unfriend_attacker` (carp): a friend who hurts the mob is a friend no more.
pub fn on_attacked(world: &mut World, victim: Entity, attacker: Entity) {
    let carp = world
        .get::<BasicMob>(victim)
        .is_some_and(|b| is_under(&b.def.path, "/mob/living/basic/carp"));
    if carp && unfriend(world, victim, attacker) {
        let name = super::defense::name_of(world, attacker);
        super::defense::emote(
            world,
            victim,
            &format!("смотрит на {name} с видом преданного."),
        );
    }
}

/// tg `breed_with_partner` (the pawn's `ai_interact` with its partner out of combat mode): a
/// baby of `baby_paths` appears under the pawn, which then waits `breed_timer`.
pub fn breed_with(world: &mut World, pawn: Entity, partner: Entity) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(breeds) = world.get::<Breeds>(pawn).copied() else {
        return false;
    };
    let Some(path) = world.get::<BasicMob>(partner).map(|b| b.def.path.clone()) else {
        return false;
    };
    let players = world.get::<crate::server::PlayerMob>(pawn).is_some()
        || world.get::<crate::server::PlayerMob>(partner).is_some();
    if players
        || !is_under(&path, breeds.partners)
        || world.get::<Breeds>(partner).is_none()
        || gender_of(world, partner) == gender_of(world, pawn)
    {
        return false;
    }
    if now < breeds.ready_at {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(pawn) else {
        return false;
    };
    let reach = world.get::<TilePos>(partner).is_some_and(|&p| {
        world
            .resource::<ss13_world::SpatialIndex>()
            .adjacent(pos, p, |e| e == partner)
            || p == pos
    });
    if !reach {
        return false;
    }
    super::spawn::spawn_mob(world, breeds.baby, pos);
    if let Some(mut b) = world.get_mut::<Breeds>(pawn) {
        b.ready_at = now + BREED_TIMER;
    }
    if let Some(mut c) = world.get_mut::<AiController>(pawn) {
        c.bb.set(Key::BreedReady, Value::Bool(false));
    }
    true
}

/// `toggle_status` after `breed_timer`: ready again.
pub fn refresh_breed_ready(world: &World, pawn: Entity, now: f64) -> bool {
    world.get::<Breeds>(pawn).is_some_and(|b| now >= b.ready_at)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_are_heard_in_speech() {
        assert!(PetCommand::Stay.heard_in("Иан, СИДЕТЬ!", "Иан", Gender::Male));
        assert!(PetCommand::Follow.heard_in("ко мне, мальчик", "Иан", Gender::Male));
        assert!(PetCommand::Attack.heard_in("Фас его!", "Иан", Gender::Male));
        assert!(!PetCommand::Attack.heard_in("привет", "Иан", Gender::Male));
        assert!(PetCommand::GoodBoy.heard_in("Good Ian!", "Ian", Gender::Male));
        assert!(PetCommand::GoodBoy.heard_in("good boy", "Ian", Gender::Male));
        assert!(!PetCommand::GoodBoy.heard_in("good boy", "Lisa", Gender::Female));
        assert!(PetCommand::GoodBoy.heard_in("good girl", "Lisa", Gender::Female));
    }

    #[test]
    fn mob_tables() {
        assert!(commands_of("/mob/living/basic/pet/dog/corgi/ian").is_some());
        assert!(commands_of("/mob/living/basic/mouse/rat").is_none());
        assert!(commands_of("/mob/living/basic/carp").is_none());
        assert!(has_pet_subtree("/mob/living/basic/carp"));
        assert!(breeding_of("/mob/living/basic/pet/cat/kitten").is_none());
        assert_eq!(
            breeding_of("/mob/living/basic/pet/cat/runtime").map(|b| b.baby),
            Some("/mob/living/basic/pet/cat/kitten")
        );
        assert!(
            TAMES
                .iter()
                .any(|t| is_under("/mob/living/basic/cow/moonicorn", t.root))
        );
    }
}
