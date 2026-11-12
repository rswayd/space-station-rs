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
//! - Canned laughter's reagent makes the drinker laugh once, with its moodlet. The balloon
//!   mallet giggles on every swing at a living mob; the target's sanity sets the force (8 when
//!   crazy or worse, 4 when unstable or disturbed, else 1) and humiliates the less mad; it only
//!   hits in combat mode (`crate::combat`).
//! - The Guide to Dank Mimery (the mime's backpack) offers the routines not known yet on a
//!   radial: invisible wall, chair or box (`crate::spells`, which need the vow); the pick is
//!   learned and the book is gone.
//! - The mime spawns under the vow of silence (`TRAIT_MIMING`): it cannot say, whisper or use
//!   the radio (`crate::medical::traumas::refresh` folds it into the mute flag).
// shortcut: the Advanced Mimery books (Invisible Blockade, Finger Guns) are not ported; a
// non-mime reading the mimery book gets no vow action.
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

// --- Canned laughter and the balloon mallet ------------------------------------------------

const BALLOON_MALLET: &str = "/obj/item/balloon_mallet";
const HEHE: &str = "sound/mobs/non-humanoids/clown/hehe.ogg";
const LAUGHTER: &str = "/datum/reagent/consumable/laughter";

/// `/datum/mood_event/mallet_humiliation`.
const MALLET_HUMILIATION: MoodEvent = MoodEvent {
    id: "mallet_humiliation",
    description: "Попасть под удар такого глупого оружия довольно унизительно...",
    mood_change: -3.0,
    timeout: 10.0,
    hidden: false,
};

/// `/datum/mood_event/chemical_laughter`.
const CHEMICAL_LAUGHTER: MoodEvent = MoodEvent {
    id: "chemical_laughter",
    description: "Смех и правда продлевает жизнь, не так ли?",
    mood_change: 4.0,
    timeout: 180.0,
    hidden: false,
};

pub fn is_balloon_mallet(path: &str) -> bool {
    is_under(path, BALLOON_MALLET)
}

/// `balloon_mallet/attack`'s `switch(target.mob_mood.sanity)`: the force of this hit and
/// whether the target is humiliated. tg keeps the last force set; here every hit starts from
/// the mallet's own 1.
pub fn mallet_hit(sanity: f32) -> (f32, bool) {
    use crate::mood::{
        SANITY_CRAZY, SANITY_DISTURBED, SANITY_GREAT, SANITY_INSANE, SANITY_NEUTRAL,
        SANITY_UNSTABLE,
    };
    if (SANITY_INSANE..=SANITY_CRAZY).contains(&sanity) {
        (8.0, false)
    } else if (SANITY_UNSTABLE..=SANITY_DISTURBED).contains(&sanity) {
        (4.0, true)
    } else if (SANITY_NEUTRAL..=SANITY_GREAT).contains(&sanity) {
        (1.0, true)
    } else {
        (1.0, false)
    }
}

/// The giggle at the attacker's tile and the humiliation of a hit with the mallet.
pub fn mallet_effects(
    world: &mut World,
    at: ss13_shared::TilePos,
    target: Entity,
    humiliated: bool,
) {
    crate::sound::playsound(world, crate::sound::PlaySound::new(at, HEHE, 20.0, true));
    if humiliated {
        crate::mood::add_event(world, target, "humiliated", &MALLET_HUMILIATION);
    }
}

/// `laughter/on_mob_life` (once: it metabolizes at `INFINITY`): a laugh and its moodlet.
pub fn laughter(world: &mut World, mob: Entity) {
    if crate::chem::blood_amount(world, mob, LAUGHTER) <= 0.0
        || world.get::<crate::health::Stat>(mob) == Some(&crate::health::Stat::Dead)
    {
        return;
    }
    crate::mobs::space_fauna::run_emote(world, mob, "laugh");
    crate::mood::add_event(world, mob, "chemical_laughter", &CHEMICAL_LAUGHTER);
}

// --- The mime's book ------------------------------------------------------------------------

const MIMERY: &str = "/obj/item/book/granter/action/spell/mime/mimery";
/// `mimery/on_reading_start`'s list, in tg's order.
const MIMERY_SPELLS: [crate::spells::Spell; 3] = [
    crate::spells::Spell::InvisibleWall,
    crate::spells::Spell::InvisibleChair,
    crate::spells::Spell::InvisibleBox,
];

/// The radial of a Guide to Dank Mimery: the spells offered, in the slices' order.
#[derive(Clone, Debug)]
pub struct MimeryAsk {
    pub book: Entity,
    pub spells: Vec<crate::spells::Spell>,
}

fn knows(world: &World, mob: Entity, spell: crate::spells::Spell) -> bool {
    world
        .get::<crate::spells::Spells>(mob)
        .is_some_and(|s| s.get(spell).is_some())
}

/// `/obj/item/book/granter/attack_self` of the Guide to Dank Mimery (no pages, no reading
/// time): the routines not known yet on a radial. True when `item` is that book.
pub(super) fn mimery_attack_self(
    world: &mut World,
    client: Entity,
    mob: Entity,
    item: Entity,
) -> bool {
    if !is_under(&item_path(world, item), MIMERY) {
        return false;
    }
    if crate::vision::is_blind(world, mob) {
        tell(
            world,
            client,
            Span::Warning,
            "Вы слепы и не можете ничего прочесть!",
        );
        return true;
    }
    let spells: Vec<crate::spells::Spell> = MIMERY_SPELLS
        .into_iter()
        .filter(|&s| !knows(world, mob, s))
        .collect();
    // `mimery/can_learn`.
    if spells.is_empty() {
        tell(
            world,
            client,
            Span::Warning,
            "You already know the secrets of mimery!",
        );
        return true;
    }
    let choices = spells
        .iter()
        .map(|s| {
            let def = s.def();
            ss13_tgui::prompt::RadialChoice {
                name: def.name.to_owned(),
                icon: crate::radial::icon_of(world, def.dmi, def.icon),
            }
        })
        .collect();
    let title = crate::chem::name_of(world, item);
    if let Some(prompt) = crate::radial::show(
        world,
        crate::radial::RadialMenu {
            client,
            actor: mob,
            anchor: item,
            title,
            choices,
            require_near: true,
        },
    ) {
        world
            .entity_mut(prompt)
            .insert(super::server::PendingAsk::Mimery(MimeryAsk {
                book: item,
                spells,
            }));
    }
    true
}

/// The routine picked: `check_menu` again (a human holding the book, not incapacitated), then
/// `mimery/on_reading_finished`.
pub(super) fn mimery_answer(world: &mut World, mob: Entity, ask: MimeryAsk, choice: usize) {
    let Some(&spell) = ask.spells.get(choice) else {
        return;
    };
    let holds = world
        .get::<crate::inventory::Inventory>(mob)
        .is_some_and(|i| i.hands.contains(&Some(ask.book)));
    if !holds
        || world.get::<crate::human::HumanLook>(mob).is_none()
        || crate::status_effects::incapacitation(world, mob).incapacitated
        || knows(world, mob, spell)
    {
        return;
    }
    crate::spells::learn(world, mob, spell);
    if let Some(client) = super::server::client_of(world, mob) {
        tell(
            world,
            client,
            Span::Warning,
            "The book disappears into thin air.",
        );
    }
    if world.get_entity(ask.book).is_ok() {
        world.despawn(ask.book);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mallet_hits_harder_the_madder_the_target() {
        assert_eq!(mallet_hit(10.0), (8.0, false));
        assert_eq!(mallet_hit(60.0), (4.0, true));
        assert_eq!(mallet_hit(110.0), (1.0, true));
        // Between tg's ranges nothing changes.
        assert_eq!(mallet_hit(35.0), (1.0, false));
        assert_eq!(mallet_hit(140.0), (1.0, false));
    }
}
