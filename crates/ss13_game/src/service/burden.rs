//! The punished god (server), after tg `/datum/religion_sect/burden`,
//! `code/modules/religion/burdened` (`burdened_trauma.dm`, `psyker.dm`).
//!
//! - A convert (human) is [`Burdened`] (tg `/datum/brain_trauma/special/burdened`): the burden
//!   level follows what the body lacks or suffers: missing limbs, missing critical organs, active
//!   addictions, negative mutations and severe traumas, one level each. Each level up or down
//!   says its line; level 3 brings telepathy and unintelligible speech, level 6 telekinesis and
//!   mind reading (whichever of them genetics has), losing the level takes them back; reaching 9
//!   turns the body into a psyker over 10 s (`slow_psykerize`): the head is torn open, the eyes
//!   are gone, the brain becomes a [`Psyker`]'s (mind antimagic).
//! - The blessing takes the target's burden on: the hurt bodyparts' brute and burn, their
//!   wounds, a burn husk, the toxin and suffocation damage move to the chaplain, scaled down by
//!   7% per burden level. Corpses are blessed the same way.
//! - "Transmogrify" (burden 9): a null rod on the altar becomes the chaplain's revolver, which
//!   refills its cylinder by prayer (5 s, burden 9) and takes no manual reloads.
// shortcut: the burden is recounted once a second from the body instead of tg's gain/lose
// signals: aheals are not told apart and every one of the eight organ slots counts for every
// species (tg leaves out the slots a species has no organ for, and blinding eyes). No outline
// filter, no psyker head sprite, echolocation or psyker spells, no blood transfer in the
// blessing, no lucky-bullet crits or ricochets.

use bevy::prelude::*;
use ss13_mapload::service::NULLROD_VARIANTS;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::SpatialIndex;

use super::religion::{
    Blessing, PerformingRite, ReligionJob, glob_deity, is_human, pick, rename, say, tell_mob,
};
use super::server::item_path;
use crate::chem::name_of;
use crate::construction::Job;
use crate::health::{DamageType, Health, Stat};
use crate::inventory::Inventory;
use crate::medical::organs::Slot;
use crate::obj_damage::is_under;
use crate::tools::{Actor, DoAfter, do_after};
use crate::zone::BodyZone;

const NULLROD: &str = "/obj/item/nullrod";
pub(super) const CHAPLAIN_REVOLVER: &str = "/obj/item/gun/ballistic/revolver/chaplain";
/// `burden_level` that unlocks the psyker, the rite and the prayer.
pub const MAX_BURDEN: u32 = 9;
/// `burden_organ_slots`.
const BURDEN_ORGANS: [Slot; 8] = [
    Slot::Brain,
    Slot::Heart,
    Slot::Lungs,
    Slot::Eyes,
    Slot::Ears,
    Slot::Tongue,
    Slot::Liver,
    Slot::Stomach,
];
const TELEPATHY: &str = "/datum/mutation/telepathy";
const UNINTELLIGIBLE: &str = "/datum/mutation/unintelligible";
const TELEKINESIS: &str = "/datum/mutation/telekinesis";
const MINDREADER: &str = "/datum/mutation/mindreader";
/// `slow_psykerize`'s two `sleep(5 SECONDS)`.
const PSYKERIZE_STEP_SECS: f64 = 5.0;
/// `pray_refill`'s `do_after`, deciseconds.
const PRAY_REFILL_DS: f32 = 50.0;
const PUNCH: &str = "sound/effects/hit_punch.ogg";
const MAGIC_BLOCK_HOLY: &str = "sound/effects/magic/magic_block_holy.ogg";

/// `/obj/item/gun/ballistic/revolver/chaplain` `possible_names`.
const REVOLVER_NAMES: [(&str, &str); 33] = [
    ("Requiescat", "May they rest in peace."),
    ("Requiem", "They will never reach truth."),
    (
        "Vade Retro",
        "Having a gun might make exorcisms more effective, who knows?",
    ),
    ("Extra Nos", "Salvation is given externally."),
    ("Ordo Salutis", "First step? Fire."),
    ("Absolution", "Free of your sins."),
    ("Rod of God", "Splitting the red sea again."),
    ("Holy Grail", "You found it!"),
    ("Burning Bush", "Useful for any burning ambush."),
    (
        "Judgement",
        "First of all, damn. Alpha much? Dude, so cool, and so are you! Strong, too!",
    ),
    ("Paradiso", "A divine end to the comedy of life."),
    (
        "DVNO",
        "Don't need to ask my name to figure out how cool I am.",
    ),
    (
        "Venus Supermax",
        "Did you know nearly everyone working and living on Venus is involved in sulfur \
         extraction? Quite fitting for this weapon of gunpowder.",
    ),
    (
        "Nirvana",
        "The giver of quietude, freedom, and highest happiness.",
    ),
    (
        "Cerebrum Dispersio",
        "Latin for \"brain splitting\". How fitting.",
    ),
    ("Ultimort", "Your hope dies last."),
    ("Lifelight", "No escape, no greater fate to be made."),
    ("Bendbreaker", "FRAGILE: Please do not bend or break."),
    (
        "Pop Pop",
        "The name referring to an onomatopeia (phonetic imitation) of a gun firing.",
    ),
    ("Justice", "Justice is Splendor."),
    ("Splendor", "Splendor is Justice."),
    ("Revelation", "Awaken your faith."),
    (
        "New Safety M62",
        "This model of firearm is popular hundreds of years later due to masculine \
         associations created by the film industry.",
    ),
    ("Unmaker", "What the !@#%* is this!"),
    (
        "INKVD",
        "Savior of the soul and fighter against dirty thoughts.",
    ),
    (
        "Life Leech",
        "An artifact said to draw its power from the life energy of others.",
    ),
    (
        "Nullray",
        "Starless metal on the barrel imbibes light and routes it to the null place. The grip \
         acrylic is patterned after ley lines.",
    ),
    ("Mortis", "Put your faith into this weapon working."),
    (
        "Ramiel",
        "Literally meaning \"God has thundered\". You could even interpret the gunshot as a \
         thunder.",
    ),
    (
        "Daredevil",
        "Hey now, you won't be reckless with this, will you?",
    ),
    ("Lacytanga", "Rules are written by the strong."),
    ("A10", "The fist of God. Keep away from the terrible."),
    ("Lucky", "Ain't that a kick in the head?"),
];

/// tg `/datum/brain_trauma/special/burdened`: its `burden_level`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Burdened {
    pub level: u32,
    /// The levels counted at the gain are silent (`on_gain`'s `silent = TRUE`).
    pub counted: bool,
}

/// tg `/obj/item/organ/brain/psyker` in the head.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Psyker;

/// `slow_psykerize` under way: the steps done and when the next is due.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
struct Psykerizing {
    step: u8,
    at: f64,
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // The chaplain's revolver prays for its rounds (before `guns::attack_self`).
    app.add_click_handler(ClickPhase::AttackSelf, 989, |w, c| {
        pray_refill(w, c.client, c.mob, c.target)
    });
    // ... and takes no rounds by hand (before `guns::item_interaction`).
    app.add_click_handler(ClickPhase::OtherHand, 1489, |w, c| {
        c.held
            .is_some_and(|held| no_manual_reload(w, c.client, held, c.target))
    });
    app.add_systems(
        Update,
        (
            refresh_burdens.run_if(any_with_component::<Burdened>),
            psykerize_steps.run_if(any_with_component::<Psykerizing>),
        ),
    );
}

/// `on_conversion`: a human becomes burdened.
pub(super) fn on_conversion(world: &mut World, mob: Entity) {
    if !is_human(world, mob) {
        let deity = glob_deity(world);
        tell_mob(
            world,
            mob,
            Span::Warning,
            format!(
                "{deity} needs higher level creatures to fully comprehend the suffering. You are \
                 not burdened."
            ),
        );
        return;
    }
    if world.get::<Burdened>(mob).is_none() {
        world.entity_mut(mob).insert(Burdened::default());
        tell_mob(world, mob, Span::Notice, "You feel burdened!");
    }
}

/// `tool_examine`: the burden level.
pub(super) fn tool_examine(world: &World, mob: Entity) -> String {
    match world.get::<Burdened>(mob) {
        Some(b) => format!("You are at burden level {}/9.", b.level),
        None => "You are not burdened.".to_owned(),
    }
}

fn burden_level(world: &World, mob: Entity) -> u32 {
    world.get::<Burdened>(mob).map_or(0, |b| b.level)
}

/// What the body lacks or suffers, one burden each (the trauma's signals, counted).
pub fn burden_count(world: &World, mob: Entity) -> u32 {
    let limbs = world.get::<Health>(mob).map_or(0, |h| {
        BodyZone::ALL.iter().filter(|&&z| !h.has_part(z)).count() as u32
    });
    let psyker = world.get::<Psyker>(mob).is_some();
    let organs = BURDEN_ORGANS
        .iter()
        // `psykerize` takes the eyes "specially": no burden for them.
        .filter(|&&slot| !(psyker && slot == Slot::Eyes))
        .filter(|&&slot| crate::medical::organs::lacks(world, mob, slot))
        .count() as u32;
    let addictions = world
        .get::<crate::addictions::Addictions>(mob)
        .map_or(0, |a| a.active.len() as u32);
    let mutations = world.get::<crate::genetics::Dna>(mob).map_or(0, |dna| {
        dna.mutations
            .iter()
            .filter(|m| crate::genetics::MUTATIONS[m.id].quality == crate::genetics::NEGATIVE)
            .count() as u32
    });
    let traumas = crate::medical::organs::organ(world, mob, Slot::Brain)
        .and_then(|brain| world.get::<crate::medical::traumas::Traumas>(brain))
        .map_or(0, |t| {
            t.0.iter()
                .filter(|t| t.kind.def().category == crate::medical::traumas::Category::Severe)
                .count() as u32
        });
    limbs + organs + addictions + mutations + traumas
}

/// Once a second: every burdened body's level steps to its count, a level at a time.
/// Cost: the (one or two) burdened chaplains, a handful of component reads each.
fn refresh_burdens(world: &mut World, mut next: Local<f64>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + 1.0;
    let mobs: Vec<Entity> = world
        .query_filtered::<Entity, With<Burdened>>()
        .iter(world)
        .collect();
    for mob in mobs {
        let Some(state) = world.get::<Burdened>(mob).copied() else {
            continue;
        };
        let target = burden_count(world, mob);
        let silent = !state.counted;
        let mut level = state.level;
        while level != target {
            let increase = level < target;
            level = if increase { level + 1 } else { level - 1 };
            update_burden(world, mob, level, increase, silent);
        }
        world.entity_mut(mob).insert(Burdened {
            level,
            counted: true,
        });
    }
}

/// `update_burden` arrived at `level`: its line and its rewards.
fn update_burden(world: &mut World, mob: Entity, level: u32, increase: bool, silent: bool) {
    let deity = glob_deity(world);
    let (up, down) = burden_lines(level, &deity);
    if !silent {
        let (span, text) = if increase {
            (Span::Notice, up)
        } else {
            (Span::Warning, down)
        };
        if !text.is_empty() {
            tell_mob(world, mob, span, text);
        }
    }
    let source = crate::genetics::SOURCE_BURDENED;
    let mutate = |world: &mut World, paths: [&str; 2], add: bool| {
        for id in paths.into_iter().filter_map(crate::genetics::mutation_id) {
            if add {
                crate::genetics::add_mutation(world, mob, id, source);
            } else {
                crate::genetics::remove_mutation(world, mob, id, source);
            }
        }
    };
    match (level, increase) {
        (3, true) => mutate(world, [TELEPATHY, UNINTELLIGIBLE], true),
        (2, false) => mutate(world, [TELEPATHY, UNINTELLIGIBLE], false),
        (6, true) => mutate(world, [TELEKINESIS, MINDREADER], true),
        (5, false) => mutate(world, [TELEKINESIS, MINDREADER], false),
        (MAX_BURDEN, true) => slow_psykerize(world, mob),
        _ => {}
    }
}

/// The lines of `update_burden` at `level`: (going up, going down).
pub fn burden_lines(level: u32, deity: &str) -> (String, String) {
    let lighter = "The weight on your shoulders feels lighter.";
    match level {
        0 => (
            String::new(),
            format!(
                "You feel no weight on your shoulders. You are not feeling {deity}'s suffering."
            ),
        ),
        1 => (
            format!(
                "You begin to feel the scars on {deity}. You must continue to burden yourself."
            ),
            format!("{lighter} You are barely feeling {deity}'s suffering."),
        ),
        2 => (
            format!("You have done well to understand {deity}. You are almost at a breakthrough."),
            format!("{lighter} You have lost some universal truths."),
        ),
        3 => (
            format!(
                "Your suffering is only a fraction of {deity}'s, and yet the universal truths are \
                 coming to you."
            ),
            format!("{lighter} You feel like you're about to forget."),
        ),
        4 => (
            format!(
                "It hurts, each ounce of pain a lesson told. How does {deity} bear this weight?"
            ),
            format!("{lighter} You're growing further from your goal."),
        ),
        5 => (
            "Your body is a canvas of loss. You are almost at a breakthrough.".to_owned(),
            format!("{lighter} You have lost some universal truths."),
        ),
        6 => (
            "Your suffering is respectful, your scars immaculate. More universal truths are \
             clear, but you do not fully understand yet."
                .to_owned(),
            format!("{lighter} You feel like you're about to forget."),
        ),
        7 => (
            format!(
                "The weight on your shoulders is immense. {deity} is shattered across the cosmos."
            ),
            format!("{lighter} You're growing further from your goal."),
        ),
        8 => (
            format!("You're on the cusp of another breakthrough. {deity} lost everything."),
            format!("{lighter} You have lost some universal truths."),
        ),
        9 => (
            format!(
                "You have finally broken yourself enough to understand {deity}. It's all so clear to you."
            ),
            String::new(),
        ),
        _ => (String::new(), String::new()),
    }
}

fn has_head(world: &World, mob: Entity) -> bool {
    world
        .get::<Health>(mob)
        .is_some_and(|h| h.has_part(BodyZone::Head))
}

fn dead(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) == Some(&Stat::Dead)
}

/// `slow_psykerize`: "You feel unwell...", then the two steps 5 s apart.
fn slow_psykerize(world: &mut World, mob: Entity) {
    if !is_human(world, mob)
        || dead(world, mob)
        || !has_head(world, mob)
        || world.get::<Psyker>(mob).is_some()
        || world.get::<Psykerizing>(mob).is_some()
    {
        return;
    }
    tell_mob(world, mob, Span::Danger, "You feel unwell...");
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(mob).insert(Psykerizing {
        step: 0,
        at: now + PSYKERIZE_STEP_SECS,
    });
}

fn psykerize_steps(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let due: Vec<(Entity, Psykerizing)> = world
        .query::<(Entity, &Psykerizing)>()
        .iter(world)
        .filter(|(_, p)| p.at <= now)
        .map(|(e, p)| (e, *p))
        .collect();
    for (mob, state) in due {
        if dead(world, mob) || !has_head(world, mob) {
            world.entity_mut(mob).remove::<Psykerizing>();
            continue;
        }
        if state.step == 0 {
            tell_mob(world, mob, Span::Danger, "You feel your skin ripping off!");
            crate::medical::diseases::emote(world, mob, "scream");
            hurt_zone(world, mob, BodyZone::Head, 30.0, 0.0);
            world.entity_mut(mob).insert(Psykerizing {
                step: 1,
                at: now + PSYKERIZE_STEP_SECS,
            });
            continue;
        }
        world.entity_mut(mob).remove::<Psykerizing>();
        if !psykerize(world, mob) {
            tell_mob(world, mob, Span::Warning, "The transformation subsides...");
            continue;
        }
        hurt_zone(world, mob, BodyZone::Head, 50.0, 0.0);
        tell_mob(
            world,
            mob,
            Span::Danger,
            "Your head splits open! Your brain mutates!",
        );
        crate::medical::diseases::emote(world, mob, "scream");
    }
}

/// `psykerize`: the psyker brain in, the eyes gone. False when it could not happen.
pub fn psykerize(world: &mut World, mob: Entity) -> bool {
    if dead(world, mob)
        || !has_head(world, mob)
        || crate::medical::organs::organ(world, mob, Slot::Brain).is_none()
    {
        return false;
    }
    world.entity_mut(mob).insert(Psyker);
    if let Some(eyes) = crate::medical::organs::organ(world, mob, Slot::Eyes) {
        crate::medical::organs::remove(world, eyes, true);
        world.despawn(eyes);
    }
    true
}

/// carbon `apply_damage` of brute and burn to one bodypart, then `update_stat`.
fn hurt_zone(world: &mut World, mob: Entity, zone: BodyZone, brute: f32, burn: f32) {
    let (Some(mut health), Some(mut stat)) = (
        world.get::<Health>(mob).cloned(),
        world.get::<Stat>(mob).copied(),
    ) else {
        return;
    };
    let before = stat;
    if brute > 0.0 {
        crate::health::apply_damage_zone(&mut health, &mut stat, zone, DamageType::Brute, brute);
    }
    if burn > 0.0 {
        crate::health::apply_damage_zone(&mut health, &mut stat, zone, DamageType::Burn, burn);
    }
    world.entity_mut(mob).insert(health);
    if stat != before {
        world.entity_mut(mob).insert(stat);
    }
}

/// `max(1 - 0.07 * burden_level, 0.01)`: how much of a burden the chaplain takes.
pub fn burden_modifier(level: u32) -> f32 {
    (1.0 - 0.07 * level as f32).max(0.01)
}

/// `/datum/religion_sect/burden/sect_bless` (and `sect_dead_bless`).
pub(super) fn sect_bless(world: &mut World, chaplain: Entity, target: Entity) -> Blessing {
    if !is_human(world, target) || !is_human(world, chaplain) {
        return Blessing::Failed;
    }
    let Some(burden) = world.get::<Burdened>(chaplain).copied() else {
        return Blessing::Failed;
    };
    let modifier = burden_modifier(burden.level);
    let (Some(mut theirs), Some(mut mine)) = (
        world.get::<Health>(target).cloned(),
        world.get::<Health>(chaplain).cloned(),
    ) else {
        return Blessing::Failed;
    };
    let mut transferred = false;
    let my_parts: Vec<BodyZone> = BodyZone::ALL
        .into_iter()
        .filter(|&z| mine.has_part(z))
        .collect();
    let mut wounds_moved: Vec<(BodyZone, BodyZone)> = Vec::new();
    if !my_parts.is_empty() {
        let their_parts: Vec<BodyZone> = BodyZone::ALL
            .into_iter()
            .filter(|&z| theirs.has_part(z))
            .collect();
        for zone in their_parts {
            let mine_zone = if mine.has_part(zone) {
                zone
            } else {
                let i = world
                    .resource_mut::<crate::combat::CombatRng>()
                    .range(0, my_parts.len() as u32 - 1);
                my_parts[i as usize]
            };
            let part = theirs.parts[zone.index()];
            let (brute, burn) = (part.brute, part.burn);
            if brute > 0.0 || burn > 0.0 {
                transferred = true;
                theirs.heal_zone(zone, brute, burn);
                // `forced = TRUE, wound_bonus = CANT_WOUND`.
                mine.damage_zone(mine_zone, DamageType::Brute, brute * modifier);
                mine.damage_zone(mine_zone, DamageType::Burn, burn * modifier);
            }
            wounds_moved.push((zone, mine_zone));
        }
    }
    let tox = theirs.tox;
    if tox > 0.0 && !mine.mods.tox_immune {
        transferred = true;
        // `adjust_tox_loss(-toxin_damage)` of the whole loss: none left, toxin lover or not.
        theirs.tox = 0.0;
        mine.adjust(DamageType::Tox, tox * modifier);
    }
    let oxy = theirs.oxy;
    if oxy > 0.0 {
        transferred = true;
        theirs.adjust(DamageType::Oxy, -oxy);
        mine.adjust(DamageType::Oxy, oxy * modifier);
    }
    for (mob, health) in [(target, theirs), (chaplain, mine)] {
        let mut stat = world.get::<Stat>(mob).copied().unwrap_or_default();
        let before = stat;
        crate::health::update_stat(&health, &mut stat);
        world.entity_mut(mob).insert(health);
        if stat != before {
            world.entity_mut(mob).insert(stat);
        }
    }
    transferred |= move_wounds(world, target, chaplain, &wounds_moved);
    if world.get::<crate::death::BurnHusk>(target).is_some() {
        transferred = true;
        crate::death::cure_husk(world, target, true);
        crate::death::become_husk(world, chaplain);
    }
    if !transferred {
        tell_mob(world, chaplain, Span::Warning, "They hold no burden!");
        return Blessing::Ignored;
    }
    let (name, chaplain_name, deity) = (
        name_of(world, target),
        name_of(world, chaplain),
        glob_deity(world),
    );
    super::server::say_around(
        world,
        target,
        Span::Notice,
        format!("{chaplain_name} takes on {name}'s burden!"),
        &[],
    );
    tell_mob(
        world,
        target,
        Span::BoldNotice,
        format!("May the power of {deity} compel you to be healed!"),
    );
    super::server::sound_at(world, chaplain, PUNCH, 25.0, true);
    crate::mood::add_event(world, target, "blessing", &super::library::BLESSING);
    Blessing::Success
}

/// `remove_wound` + `apply_wound(chaplains_limb)` for every wound of the target's limbs.
fn move_wounds(
    world: &mut World,
    target: Entity,
    chaplain: Entity,
    zones: &[(BodyZone, BodyZone)],
) -> bool {
    use crate::medical::wounds::{Limbs, apply_wound, remove_wound};
    let mut moved = Vec::new();
    if let Some(mut limbs) = world.get_mut::<Limbs>(target) {
        for &(from, to) in zones {
            let limb = limbs.limb_mut(from);
            while !limb.wounds.is_empty() {
                moved.push((to, remove_wound(limb, 0).kind));
            }
        }
    }
    if moved.is_empty() {
        return false;
    }
    if let Some(mut limbs) = world.get_mut::<Limbs>(chaplain) {
        for (to, kind) in &moved {
            apply_wound(limbs.limb_mut(*to), *kind);
        }
    }
    true
}

/// A null rod's trait (`TRAIT_NULLROD_ITEM`, `nullrod_core`): the rods, their forms and the
/// chaplain's revolver.
fn is_nullrod_item(path: &str) -> bool {
    is_under(path, NULLROD) || NULLROD_VARIANTS.contains(&path) || path == CHAPLAIN_REVOLVER
}

/// `nullrod_transformation/perform_rite`: burden 9 and a null rod on the altar.
pub(super) fn rite_target(
    world: &mut World,
    user: Entity,
    altar: Entity,
) -> Option<Option<Entity>> {
    if !is_human(world, user) {
        return None;
    }
    if burden_level(world, user) < MAX_BURDEN {
        tell_mob(world, user, Span::Warning, "You aren't burdened enough.");
        return None;
    }
    let pos = *world.get::<TilePos>(altar)?;
    let rod = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| is_nullrod_item(&item_path(world, e)));
    if rod.is_none() {
        let what = name_of(world, altar);
        tell_mob(
            world,
            user,
            Span::Warning,
            format!("You need to place a null rod on {what} to do this!"),
        );
        return None;
    }
    Some(rod)
}

/// `nullrod_transformation/invoke_effect`: the rod still on the altar becomes the revolver.
pub(super) fn transmogrify(
    world: &mut World,
    user: Entity,
    state: PerformingRite,
    pos: TilePos,
) -> bool {
    let Some(rod) = state
        .target
        .filter(|&r| world.get::<TilePos>(r) == Some(&pos))
    else {
        tell_mob(world, user, Span::Warning, "Your target left the altar!");
        return false;
    };
    let name = name_of(world, rod);
    tell_mob(
        world,
        user,
        Span::Warning,
        format!("{name} turns into a gun!"),
    );
    crate::medical::diseases::emote(world, user, "smile");
    world.despawn(rod);
    if let Some(gun) = crate::construction::spawn_obj(world, CHAPLAIN_REVOLVER, pos) {
        name_revolver(world, gun);
    }
    true
}

/// `revolver/chaplain/Initialize`: `name = pick(possible_names)`, its description.
fn name_revolver(world: &mut World, gun: Entity) {
    let names: Vec<&str> = REVOLVER_NAMES.iter().map(|(n, _)| *n).collect();
    let name = pick(world, &names);
    let desc = REVOLVER_NAMES
        .iter()
        .find(|(n, _)| *n == name)
        .map_or("", |(_, d)| *d);
    rename(world, gun, name);
    if let Some(mut dm) = world.get_mut::<ss13_mapload::DmName>(gun) {
        desc.clone_into(&mut dm.desc);
    }
}

/// `revolver/chaplain/attack_self` -> `pray_refill` up to its `do_after`.
fn pray_refill(world: &mut World, client: Entity, mob: Entity, gun: Entity) -> bool {
    if item_path(world, gun) != CHAPLAIN_REVOLVER {
        return false;
    }
    if world.get::<DoAfter>(mob).is_some() || !is_human(world, mob) {
        return true;
    }
    if burden_level(world, mob) < MAX_BURDEN {
        crate::tools::tell(world, client, Span::Warning, "You aren't burdened enough.");
        return true;
    }
    crate::mobs::defense::emote(world, mob, "presses their palms together...");
    do_after(
        world,
        Actor {
            client,
            mob,
            item: Some(gun),
        },
        mob,
        PRAY_REFILL_DS,
        0,
        Job::Religion(ReligionJob::PrayRefill),
    );
    true
}

/// The prayer's `do_after` finished: the cylinder is topped off.
pub(super) fn refill_complete(world: &mut World, mob: Entity, action: &DoAfter) {
    let Some(gun) = action.item.filter(|&g| {
        world
            .get::<Inventory>(mob)
            .is_some_and(|i| i.hands.contains(&Some(g)))
    }) else {
        return;
    };
    if burden_level(world, mob) < MAX_BURDEN {
        return;
    }
    let deity = glob_deity(world);
    say(
        world,
        mob,
        &format!("О великий {deity}, дай мне боеприпасы, в которых я нуждаюсь!"),
    );
    if let Some(mut state) = world.get_mut::<crate::guns::Gun>(gun)
        && let Some(mag) = state.magazine.as_mut()
    {
        top_off(mag);
    }
    let at = crate::chem::map_pos(world, gun);
    crate::hallucination::play_local(world, mob, at, MAGIC_BLOCK_HOLY, 50.0, true);
}

/// `magazine.top_off()`: spent casings out, live rounds of its ammo up to `max_ammo`.
pub fn top_off(mag: &mut crate::guns::Magazine) {
    mag.rounds.retain(|r| r.live);
    while mag.rounds.len() < mag.data.max_ammo as usize {
        mag.rounds.push(crate::guns::Round {
            casing: mag.data.ammo.clone(),
            live: true,
        });
    }
}

/// `revolver/chaplain/item_interaction`: casings and ammo boxes are refused.
fn no_manual_reload(world: &mut World, client: Entity, held: Entity, target: Entity) -> bool {
    if item_path(world, target) != CHAPLAIN_REVOLVER {
        return false;
    }
    let ammo = world.get::<crate::guns::Magazine>(held).is_some()
        || is_under(&item_path(world, held), "/obj/item/ammo_casing");
    if ammo {
        crate::tools::tell(world, client, Span::Warning, "нельзя перезаряжать вручную!");
    }
    ammo
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_burden_scales_down_with_its_level() {
        assert_eq!(burden_modifier(0), 1.0);
        assert!((burden_modifier(9) - 0.37).abs() < 1e-5);
        assert_eq!(burden_modifier(15), 0.01);
    }

    #[test]
    fn every_level_has_its_lines() {
        for level in 1..=8 {
            let (up, down) = burden_lines(level, "Bob");
            assert!(!up.is_empty() && !down.is_empty(), "level {level}");
        }
        assert!(burden_lines(9, "Bob").0.contains("understand Bob"));
        assert!(burden_lines(0, "Bob").0.is_empty());
    }

    #[test]
    fn the_revolver_counts_as_a_null_rod() {
        assert!(is_nullrod_item("/obj/item/nullrod/claymore"));
        assert!(is_nullrod_item(CHAPLAIN_REVOLVER));
        assert!(!is_nullrod_item("/obj/item/book/bible"));
        assert!(
            REVOLVER_NAMES
                .iter()
                .all(|(n, d)| !n.is_empty() && !d.is_empty())
        );
    }

    #[test]
    fn missing_limbs_add_burden() {
        let mut world = World::new();
        let health = Health {
            missing: 0b1100, // both arms
            ..Default::default()
        };
        let mob = world.spawn(health).id();
        assert_eq!(burden_count(&world, mob), 2);
    }
}
