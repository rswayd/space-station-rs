//! Noise makers and the mime's vow (server), after tg `datums/components/squeak.dm`,
//! `items/clown_items.dm` (bike horns, rubber duckies), `clothing/shoes/clown.dm`,
//! `paperwork/desk_bell.dm` and `spells/spell_types/self/mime_vow.dm`.
//!
//! - A squeaky thing (bike and air horns, rubber duckies, clown shoes and suits) plays one of its
//!   sounds when used in hand (at most every 2 s), when it hits a living mob, when a standing
//!   human steps onto its tile and, worn as shoes, every third step. A clown honking someone
//!   else gives them the `honk` mood event.
//! - A desk bell rings on an empty-hand click (0.3 s apart) and when hit; every ring may break
//!   its clapper (`prob(times_rang / 100)`), and a broken one is silent.
//! - The mime spawns under the vow of silence (`TRAIT_MIMING`): it cannot say, whisper or use
//!   the radio (`crate::medical::traumas::refresh` folds it into the mute flag).
// shortcut: no squeaks on bumps, throws, projectiles, disposals or hulk hits; plushies keep
// silent; the golden horn does not flip people; the vow cannot be broken (the spell action is
// not ported) and mime emotes are not muted; desk bells cannot be fixed, taken apart or picked
// up.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_shared::chat::Span;
use ss13_world::{Moved, SpatialIndex};

use super::server::{item_path, job_title};
use crate::combat::CombatRng;
use crate::health::{BodyPosition, Health};
use crate::inventory::{CarriedBy, Inventory};
use crate::mood::MoodEvent;
use crate::obj_damage::is_under;
use crate::tools::tell;

const BIKEHORN: &str = "sound/items/bikehorn.ogg";
const AIRHORN: &str = "sound/items/airhorn/airhorn2.ogg";
const QUACK: &str = "sound/effects/quack.ogg";
const CLOWNSTEPS: [&str; 2] = [
    "sound/effects/footstep/clownstep1.ogg",
    "sound/effects/footstep/clownstep2.ogg",
];
const DESK_BELL: &str = "/obj/structure/desk_bell";
const SPEED_DEMON: &str = "/obj/structure/desk_bell/speed_demon";
const RING_SOUND: &str = "sound/machines/microwave/microwave-end.ogg";
/// `/datum/component/squeak` `use_delay`, `step_delay`; the bell's `ring_cooldown_length`.
const SQUEAK_USE_DELAY: f64 = 2.0;
const SQUEAK_STEP_DELAY: u8 = 1;
const RING_COOLDOWN: f64 = 0.3;
const CLOWN: &str = "Clown";
const MIME: &str = "Mime";

/// `/datum/mood_event/honk`.
const HONK: MoodEvent = MoodEvent {
    id: "honk",
    description: "Меня захонкали!",
    mood_change: 2.0,
    timeout: 240.0,
    hidden: false,
};

/// `/datum/component/squeak` with the item's sounds (equal weights), volume and
/// `falloff_exponent`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Squeak {
    pub sounds: &'static [&'static str],
    pub volume: f32,
    pub falloff_exponent: f32,
    /// `spam_cooldown` of in-hand use.
    pub next_use: f64,
    pub steps: u8,
}

/// `/obj/structure/desk_bell` state.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct DeskBell {
    pub times_rang: f32,
    pub broken: bool,
    pub next_ring: f64,
    pub cooldown: f64,
}

/// `TRAIT_MIMING` from the mime's vow.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MimeVow;

/// The squeak each ported type adds in `Initialize`, most specific first.
fn squeak_for(path: &str) -> Option<(&'static [&'static str], f32)> {
    const TABLE: [(&str, &[&str], f32); 6] = [
        ("/obj/item/bikehorn/airhorn", &[AIRHORN], 50.0),
        ("/obj/item/bikehorn/rubberducky", &[QUACK], 50.0),
        ("/obj/item/bikehorn", &[BIKEHORN], 50.0),
        (
            "/obj/item/clothing/shoes/clown_shoes/banana_shoes",
            &[BIKEHORN],
            75.0,
        ),
        ("/obj/item/clothing/shoes/clown_shoes", &CLOWNSTEPS, 50.0),
        (
            "/obj/item/clothing/under/rank/civilian/clown",
            &[BIKEHORN],
            50.0,
        ),
    ];
    TABLE
        .iter()
        .find(|(root, ..)| is_under(path, root))
        .map(|&(_, sounds, volume)| (sounds, volume))
}

pub(super) fn add_parts(world: &mut World, e: Entity, path: &str) {
    if let Some((sounds, volume)) = squeak_for(path) {
        world.entity_mut(e).insert(Squeak {
            sounds,
            volume,
            falloff_exponent: 20.0,
            next_use: 0.0,
            steps: 0,
        });
    }
}

pub(super) fn add_obj_parts(world: &mut World, e: Entity, path: &str) {
    if is_under(path, DESK_BELL) {
        let cooldown = if is_under(path, SPEED_DEMON) {
            0.0
        } else {
            RING_COOLDOWN
        };
        world.entity_mut(e).insert(DeskBell {
            times_rang: 0.0,
            broken: false,
            next_ring: 0.0,
            cooldown,
        });
    }
}

/// `play_squeak` (`squeak_chance` 100, `sound_extra_range` -1).
fn play_squeak(world: &mut World, item: Entity) {
    let Some(squeak) = world.get::<Squeak>(item).cloned() else {
        return;
    };
    let Some(at) = crate::chem::map_pos(world, item) else {
        return;
    };
    let pick = world
        .get_resource_mut::<CombatRng>()
        .map_or(1, |mut rng| rng.range(1, squeak.sounds.len() as u32));
    let file = squeak.sounds[pick as usize - 1];
    let sound = crate::sound::PlaySound::new(at, file, squeak.volume, true)
        .extrarange(-1.0)
        .falloff_exponent(squeak.falloff_exponent);
    crate::sound::playsound(world, sound);
}

/// `use_squeak` (`COMSIG_ITEM_ATTACK_SELF`). True when `item` squeaks.
pub(super) fn attack_self(world: &mut World, _mob: Entity, item: Entity) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let ready = match world.get_mut::<Squeak>(item) {
        Some(mut squeak) if now >= squeak.next_use => {
            squeak.next_use = now + SQUEAK_USE_DELAY;
            true
        }
        Some(_) => false,
        None => return false,
    };
    if ready {
        play_squeak(world, item);
    }
    true
}

/// `COMSIG_ITEM_ATTACK` of a squeaky thing on a living mob (`bikehorn/attack`: a clown's honk
/// cheers the target up) and `desk_bell/attackby`.
pub(super) fn used_on(world: &mut World, mob: Entity, held: Entity, target: Entity) {
    if world.get::<Squeak>(held).is_some() && world.get::<Health>(target).is_some() {
        play_squeak(world, held);
        let horn = is_under(&item_path(world, held), "/obj/item/bikehorn");
        if horn && target != mob && job_title(world, mob).as_deref() == Some(CLOWN) {
            crate::mood::add_event(world, target, "honk", &HONK);
        }
    }
    if world.get::<DeskBell>(target).is_some() {
        let force = world
            .get::<ss13_mapload::DmItem>(held)
            .map_or(0.0, |i| i.force);
        if let Some(mut bell) = world.get_mut::<DeskBell>(target) {
            bell.times_rang += force;
        }
        ring(world, target, Some(mob));
    }
}

/// `desk_bell/attack_hand`. True when `target` is a bell.
pub(super) fn ring_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(bell) = world.get::<DeskBell>(target).copied() else {
        return false;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    if bell.cooldown > 0.0 && now < bell.next_ring {
        return true;
    }
    if !ring(world, target, Some(mob)) {
        let name = crate::chem::name_of(world, target);
        tell(
            world,
            client,
            Span::Notice,
            format!("{name} is silent. Some idiot broke it."),
        );
    }
    if let Some(mut bell) = world.get_mut::<DeskBell>(target) {
        bell.next_ring = now + bell.cooldown;
    }
    true
}

/// `ring_bell` with `check_clapper`: false when the clapper is already broken.
fn ring(world: &mut World, bell: Entity, user: Option<Entity>) -> bool {
    let Some(state) = world.get::<DeskBell>(bell).copied() else {
        return false;
    };
    if state.broken {
        return false;
    }
    let breaks = state.cooldown > 0.0
        && (state.times_rang >= 10_000.0
            || world
                .get_resource_mut::<CombatRng>()
                .is_some_and(|mut rng| rng.chance(state.times_rang / 100.0)));
    if breaks {
        let name = crate::chem::name_of(world, bell);
        let text =
            format!("You hear {name}'s clapper fall off of its hinge. Nice job, you broke it.");
        match user.and_then(|u| super::server::client_of(world, u)) {
            Some(client) => tell(world, client, Span::Notice, text),
            None => super::server::say_around(world, bell, Span::Notice, text, &[]),
        }
    }
    if let Some(at) = world.get::<ss13_shared::TilePos>(bell).copied() {
        let sound = crate::sound::PlaySound::new(at, RING_SOUND, 70.0, breaks)
            .extrarange(crate::sound::SHORT_RANGE_SOUND_EXTRARANGE);
        crate::sound::playsound(world, sound);
    }
    if let Some(mut s) = world.get_mut::<DeskBell>(bell) {
        s.broken = breaks;
        s.times_rang += 1.0;
    }
    true
}

/// `play_squeak_crossed` of squeaky things on the floor and `step_squeak` of worn clown shoes:
/// one look at the entered tile and the shoes per human step.
pub(super) fn squeak_on_moves(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moves: Vec<(Entity, ss13_shared::TilePos)> = cursor
        .read(world.resource::<Messages<Moved>>())
        .map(|m| (m.entity, m.to))
        .collect();
    for (mob, to) in moves {
        if world.get::<crate::human::HumanLook>(mob).is_none()
            || world.get::<BodyPosition>(mob).is_some_and(|b| b.lying())
        {
            continue;
        }
        let crossed: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(to)
            .iter()
            .copied()
            .filter(|&e| world.get::<Squeak>(e).is_some() && world.get::<CarriedBy>(e).is_none())
            .collect();
        for item in crossed {
            play_squeak(world, item);
        }
        let Some(shoes) = world.get::<Inventory>(mob).and_then(|i| i.shoes) else {
            continue;
        };
        let step = match world.get_mut::<Squeak>(shoes) {
            Some(mut squeak) if squeak.steps > SQUEAK_STEP_DELAY => {
                squeak.steps = 0;
                true
            }
            Some(mut squeak) => {
                squeak.steps += 1;
                false
            }
            None => false,
        };
        if step {
            play_squeak(world, shoes);
        }
    }
}

/// The mime's `/datum/action/cooldown/spell/vow_of_silence` grant: miming from the spawn.
pub(super) fn mime_takes_the_vow(add: On<Add<crate::jobs::PlayerJob>>, mut commands: Commands) {
    let mob = add.entity;
    commands.queue(move |world: &mut World| {
        if job_title(world, mob).as_deref() != Some(MIME) || world.get_entity(mob).is_err() {
            return;
        }
        world.entity_mut(mob).insert(MimeVow);
        crate::medical::traumas::refresh(world, mob);
    });
}
