//! Special helmets: the monkey mind magnification helmet (`code/modules/clothing/head/
//! mind_monkey_helmet.dm`) and the perceptomatrix helm (`perceptomatrix.dm`).
//!
//! Monkey mind magnification helmet ([`MonkeyHelmet`]):
//! - Its lights are one of three colors (`monkeymind1`-`3`), `up` once a mind is in.
//! - Put on the head of a mindless monkey it powers up and polls the ghosts for 5 s
//!   ("mind-magnified monkey", "Не в этом раунде" possible): the chosen one takes the monkey
//!   and is told to protect the helmet. On anyone else's head it stabs: 5 brute to the head.
//! - Nobody signing up: it falls silent and drops; each failure raises `rage_chance` by 7
//!   (sparks, then heavier sparks and a shatter, then smoke and a double buzz as it climbs), and
//!   past 0 that chance of a malfunction on the monkey.
//! - Taken off a magnified monkey (or during the poll) it breaks apart into ash: the monkey's
//!   mind leaves for good ("You feel your flicker of sentience ripped away...") and 10% of
//!   the time it malfunctions too. A monkey that stops being one drops it.
//! - Malfunctions: blood rage (the monkey AI turns aggressive), brain death (500 brain damage),
//!   primal gene (a gorilla) or mass susceptibility (gibbed).
//!
//! Perceptomatrix helm ([`Perceptomatrix`]): without a hallucination anomaly core it is opaque
//! (`tint = INFINITY`) and `_inactive`; a core goes in in 3 s (`/functioning` has one). With
//! it the tint is gone and its wearer cannot hallucinate (`TRAIT_HALLUCINATION_IMMUNE`: a
//! running hallucination ends, [`hallucination_immune`]).
// shortcut: the monkey helmet's steam particles, its policy text and the
// `GHOSTROLE_STATION_SENTIENCE` admin switch (always on) are missing. The perceptomatrix gives
// none of its other traits (no flash, true night vision, good hearing, expanded view,
// madness/mindswap/conversion immunity, ear protection, mind antimagic, casting clothing), no
// client colour and no "Направить галлюцинации" spell; add them with those traits.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmClothing, DmItem};
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};

use crate::balloon::balloon_alert;
use crate::construction::Job;
use crate::health::{DEFAULT_MESSAGE_RANGE, DamageType, Stat};
use crate::inventory::{CarriedBy, Inventory};
use crate::security::{name_of, show};
use crate::server::PlayerMob;
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, DoAfter, do_after, tell_mob};
use crate::zone::BodyZone;

pub const MONKEY_HELMET: &str = "/obj/item/clothing/head/helmet/monkey_sentience";
pub const PERCEPTOMATRIX: &str = "/obj/item/clothing/head/helmet/perceptomatrix";
const PERCEPTOMATRIX_FUNCTIONING: &str =
    "/obj/item/clothing/head/helmet/perceptomatrix/functioning";
const HALLUCINATION_CORE: &str = "/obj/item/assembly/signaler/anomaly/hallucination";
const ASH: &str = "/obj/effect/decal/cleanable/ash";
const BUZZ_SIGH: &str = "sound/machines/buzz/buzz-sigh.ogg";
/// `POLL_IGNORE_MONKEY_HELMET`.
const POLL_IGNORE_MONKEY_HELMET: &str = "monkey_helmet";
const POLL_SECS: f64 = 5.0;
const INSERT_DS: f32 = 30.0;

/// The monkey helmet's state.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonkeyHelmet {
    /// `magnification`: the monkey it is on, polling or magnified.
    pub magnification: Option<Entity>,
    pub polling: bool,
    /// `light_colors`, 1 to 3.
    pub light: u8,
    pub rage_chance: i32,
    /// The head it was last seen on (`equipped` runs when this changes).
    worn_by: Option<Entity>,
}

/// The perceptomatrix: `core_installed`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Perceptomatrix {
    pub core: bool,
}

/// The perceptomatrix's core insertion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HelmetJob {
    pub helmet: Entity,
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    app.add_click_handler(ClickPhase::HeldOnTarget, 479, |w, c| {
        c.held
            .is_some_and(|held| insert_core(w, c.client, c.mob, held, c.target))
    })
    .add_click_handler(ClickPhase::OtherHand, 439, |w, c| {
        c.held
            .is_some_and(|held| insert_core(w, c.client, c.mob, held, c.target))
    })
    .add_systems(
        Update,
        (init_helmets, watch_monkey_helmets, perceptomatrix_wearers)
            .chain()
            .after(crate::inventory::inventory_intents),
    );
}

fn init_helmets(
    world: &mut World,
    mut added: Local<QueryState<(Entity, &'static DmItem), Added<DmItem>>>,
) {
    let new: Vec<(Entity, String)> = added
        .iter(world)
        .filter(|(_, dm)| is_under(&dm.path, MONKEY_HELMET) || is_under(&dm.path, PERCEPTOMATRIX))
        .map(|(e, dm)| (e, dm.path.clone()))
        .collect();
    for (item, path) in new {
        if is_under(&path, MONKEY_HELMET) {
            let light = world.resource_mut::<crate::combat::CombatRng>().range(1, 3) as u8;
            world.entity_mut(item).insert(MonkeyHelmet {
                magnification: None,
                polling: false,
                light,
                rage_chance: -7,
                worn_by: None,
            });
            update_monkey_icon(world, item);
        } else {
            let core = is_under(&path, PERCEPTOMATRIX_FUNCTIONING);
            world.entity_mut(item).insert(Perceptomatrix { core });
            update_perceptomatrix(world, item, core);
        }
    }
}

fn sound_at(world: &mut World, at: Entity, sound: &str, vol: f32, vary: bool) {
    if let Some(pos) = crate::explosions::turf_of(world, at) {
        playsound(world, PlaySound::new(pos, sound, vol, vary));
    }
}

fn visible(world: &mut World, at: Entity, span: Span, text: String) {
    if let Some(pos) = crate::explosions::turf_of(world, at) {
        show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            (span, text),
            &[],
        );
    }
}

/// `update_icon_state`: `monkeymind<light>`, `up` while magnifying.
fn update_monkey_icon(world: &mut World, helmet: Entity) {
    let Some(h) = world.get::<MonkeyHelmet>(helmet).copied() else {
        return;
    };
    let up = if h.magnification.is_some() { "up" } else { "" };
    let state = format!("monkeymind{}{up}", h.light);
    super::toggles::set_states(world, helmet, &state, String::new());
}

/// The head `helmet` is on, if it is on one.
fn head_of(world: &World, helmet: Entity) -> Option<Entity> {
    let mob = world.get::<CarriedBy>(helmet)?.0;
    (world.get::<Inventory>(mob)?.head == Some(helmet)).then_some(mob)
}

/// `equipped`, `dropped` and `make_fall_off`, by watching where each helmet is.
fn watch_monkey_helmets(world: &mut World) {
    let mut q = world.query::<(Entity, &MonkeyHelmet)>();
    let helmets: Vec<(Entity, MonkeyHelmet)> = q.iter(world).map(|(e, h)| (e, *h)).collect();
    for (helmet, state) in helmets {
        let head = head_of(world, helmet);
        if let Some(monkey) = state.magnification {
            if head != Some(monkey) {
                // `dropped` with a magnification: `qdel(src)`.
                disconnect(world, helmet);
                continue;
            }
            if !crate::transform::is_monkey(world, monkey) {
                // `make_fall_off` (`COMSIG_SPECIES_LOSS`): the drop above breaks it next tick.
                visible(
                    world,
                    helmet,
                    Span::Warning,
                    format!(
                        "{} falls off of {}'s head as it changes shape!",
                        name_of(world, helmet),
                        name_of(world, monkey)
                    ),
                );
                drop_from_head(world, monkey, helmet);
            }
            continue;
        }
        if head == state.worn_by {
            continue;
        }
        if let Some(mut h) = world.get_mut::<MonkeyHelmet>(helmet) {
            h.worn_by = head;
        }
        if let Some(mob) = head {
            equipped(world, helmet, mob);
        }
    }
}

fn drop_from_head(world: &mut World, mob: Entity, helmet: Entity) {
    let Some(pos) = crate::explosions::turf_of(world, mob) else {
        return;
    };
    if world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.head == Some(helmet))
    {
        crate::inventory::unequip(world, mob, pos, ss13_shared::protocol::InventorySlot::Head);
        crate::inventory::put_down(world, helmet, pos);
    }
}

/// `equipped` on the head.
fn equipped(world: &mut World, helmet: Entity, mob: Entity) {
    // `TRAIT_LESSER_HUMANOID` without a player.
    if !crate::transform::is_monkey(world, mob) || world.get::<PlayerMob>(mob).is_some() {
        tell_mob(
            world,
            mob,
            Span::BoldNotice,
            "You feel a stabbing pain in the back of your head for a moment.",
        );
        crate::mobs::defense::hurt(world, mob, DamageType::Brute, 5.0, Some(BodyZone::Head));
        sound_at(world, helmet, BUZZ_SIGH, 30.0, true);
        return;
    }
    if let Some(mut h) = world.get_mut::<MonkeyHelmet>(helmet) {
        h.magnification = Some(mob);
        h.polling = true;
    }
    visible(
        world,
        helmet,
        Span::Warning,
        format!("{} powers up!", name_of(world, helmet)),
    );
    sound_at(world, helmet, "sound/machines/ping.ogg", 30.0, true);
    let spec = crate::ghost_poll::PollSpec {
        question: "Хотите стать ролью: mind-magnified monkey?".to_owned(),
        role_name: "mind-magnified monkey".to_owned(),
        secs: POLL_SECS,
        ignore: Some(POLL_IGNORE_MONKEY_HELMET),
        target: Some(mob),
        ..default()
    };
    let ghosts = crate::ghost_poll::ghosts(world);
    crate::ghost_poll::poll_candidates(world, spec, ghosts, move |world, mut signed_up| {
        poll_done(world, helmet, mob, &mut signed_up);
    });
}

/// The rest of `equipped` once the poll is over.
fn poll_done(
    world: &mut World,
    helmet: Entity,
    monkey: Entity,
    signed_up: &mut Vec<crate::ghost_poll::Candidate>,
) {
    // Taken off meanwhile: `disconnect` already broke it.
    let Some(state) = world.get::<MonkeyHelmet>(helmet).copied() else {
        return;
    };
    if state.magnification != Some(monkey) || !state.polling {
        return;
    }
    if let Some(mut h) = world.get_mut::<MonkeyHelmet>(helmet) {
        h.polling = false;
    }
    // Revalidated now: still a mindless monkey wearing it.
    let chosen =
        if world.get::<PlayerMob>(monkey).is_none() && head_of(world, helmet) == Some(monkey) {
            crate::ghost_poll::pick_candidate(world, signed_up)
        } else {
            None
        };
    let possessed = chosen.is_some_and(|c| {
        let ok = crate::medical::brainmob::possess(world, c.client, c.mob, monkey);
        if ok {
            world.despawn(c.mob);
        }
        ok
    });
    if !possessed {
        fail(world, helmet, monkey, state.rage_chance);
        return;
    }
    sound_at(
        world,
        helmet,
        "sound/machines/microwave/microwave-end.ogg",
        100.0,
        false,
    );
    tell_mob(
        world,
        monkey,
        Span::Notice,
        "You're a mind magnified monkey! Protect your helmet with your life- if you lose it, your \
         sentience goes with it!",
    );
    update_monkey_icon(world, helmet);
}

/// No volunteer: the helmet drops, angrier each time.
fn fail(world: &mut World, helmet: Entity, monkey: Entity, rage_chance: i32) {
    if let Some(mut h) = world.get_mut::<MonkeyHelmet>(helmet) {
        h.magnification = None;
        h.rage_chance += 7;
    }
    let name = name_of(world, helmet);
    match rage_chance {
        -7..=0 => {
            visible(
                world,
                monkey,
                Span::Notice,
                format!("{name} falls silent and drops on the floor. Try again later?"),
            );
            sound_at(world, helmet, BUZZ_SIGH, 30.0, true);
        }
        7..=13 => {
            visible(
                world,
                monkey,
                Span::Notice,
                format!(
                    "{name} sparkles momentarily, then falls silent and drops on the floor. Maybe \
                     you should try again later?"
                ),
            );
            sparks(world, helmet, 2);
        }
        14..=20 => {
            visible(
                world,
                monkey,
                Span::Notice,
                format!(
                    "{name} sparkles and shatters ominously, then falls silent and drops on the \
                     floor. Maybe you shouldn't try again later."
                ),
            );
            sparks(world, helmet, 4);
            sound_at(world, helmet, "shatter", 30.0, true);
        }
        21.. => {
            visible(
                world,
                monkey,
                Span::Notice,
                format!(
                    "{name} buzzes and smokes heavily, then falls silent and drops on the floor. \
                     This is clearly a bad idea."
                ),
            );
            sparks(world, helmet, 6);
            sound_at(
                world,
                helmet,
                "sound/machines/buzz/buzz-two.ogg",
                30.0,
                true,
            );
        }
        _ => {}
    }
    let rage = rage_chance + 7;
    if rage > 0
        && world
            .resource_mut::<crate::combat::CombatRng>()
            .chance(rage as f32)
    {
        malfunction(world, monkey);
    }
    if world.get_entity(monkey).is_ok() {
        drop_from_head(world, monkey, helmet);
    }
    update_monkey_icon(world, helmet);
}

fn sparks(world: &mut World, at: Entity, number: u32) {
    if let Some(pos) = crate::explosions::turf_of(world, at) {
        crate::effects::particles::do_sparks(world, number, false, pos);
    }
}

/// `disconnect` + `qdel`: the helmet breaks apart into ash.
fn disconnect(world: &mut World, helmet: Entity) {
    let Some(state) = world.get::<MonkeyHelmet>(helmet).copied() else {
        return;
    };
    let pos: Option<TilePos> = crate::explosions::turf_of(world, helmet);
    if let Some(monkey) = state.magnification {
        if !state.polling {
            if world.get::<PlayerMob>(monkey).is_some() {
                tell_mob(
                    world,
                    monkey,
                    Span::BoldWarning,
                    "You feel your flicker of sentience ripped away from you, as everything \
                     becomes dim...",
                );
                crate::health::ghostize_gone(world, monkey);
            }
            if world
                .resource_mut::<crate::combat::CombatRng>()
                .chance(10.0)
            {
                malfunction(world, monkey);
            }
        }
        if let Some(pos) = pos {
            playsound(world, PlaySound::new(pos, BUZZ_SIGH, 30.0, true));
            playsound(world, PlaySound::new(pos, "sparks", 100.0, true));
        }
        visible(
            world,
            helmet,
            Span::Warning,
            format!("{} fizzles and breaks apart!", name_of(world, helmet)),
        );
        if let Some(pos) = pos {
            crate::construction::spawn_obj(world, ASH, pos);
        }
    }
    world.despawn(helmet);
}

/// `malfunction`: one of four ends for the monkey.
fn malfunction(world: &mut World, target: Entity) {
    if world.get_entity(target).is_err() {
        return;
    }
    let roll = world.resource_mut::<crate::combat::CombatRng>().range(1, 4);
    match roll {
        1 => {
            // Blood rage.
            if let Some(mut ai) = world.get_mut::<crate::mobs::ai::AiController>(target) {
                ai.bb.set(
                    crate::mobs::ai::Key::MonkeyAggressive,
                    crate::mobs::ai::Value::Bool(true),
                );
            }
        }
        2 => crate::medical::organs::adjust_organ_loss(
            world,
            target,
            crate::medical::organs::Slot::Brain,
            500.0,
        ),
        3 => {
            crate::transform::gorillize(world, target, false);
        }
        _ => crate::health::gib(world, target),
    }
}

// --- Perceptomatrix ---------------------------------------------------------------------------

/// `update_anomaly_state` + `update_icon_state`.
fn update_perceptomatrix(world: &mut World, helmet: Entity, core: bool) {
    let state = if core {
        "perceptomatrix_helmet"
    } else {
        "perceptomatrix_helmet_inactive"
    };
    super::toggles::set_states(world, helmet, state, state.to_owned());
    if let Some(mut clothing) = world.get_mut::<DmClothing>(helmet) {
        clothing.tint = if core { 0.0 } else { f32::INFINITY };
    }
    if let Some(mob) = world.get::<CarriedBy>(helmet).map(|c| c.0) {
        crate::inventory::refresh_look(world, mob);
    }
}

/// A hallucination anomaly core into the helm (`item_interaction`).
fn insert_core(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let core = world
        .get::<DmItem>(held)
        .is_some_and(|d| is_under(&d.path, HALLUCINATION_CORE));
    if !core || world.get::<Perceptomatrix>(target).is_none() {
        return false;
    }
    balloon_alert(world, mob, target, "вставка...");
    let on = if world.get::<TilePos>(target).is_some() {
        target
    } else {
        mob
    };
    do_after(
        world,
        Actor {
            client,
            mob,
            item: Some(held),
        },
        on,
        INSERT_DS,
        0,
        Job::Clothing(super::ClothingJob::Helmet(HelmetJob { helmet: target })),
    );
    true
}

/// The end of the insertion, revalidated: the core still in hand, the helm in reach.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: HelmetJob) {
    let Some(core) = action.item else {
        return;
    };
    let helmet = job.helmet;
    if !world.get::<Inventory>(mob).is_some_and(|i| i.holds(core))
        || world.get::<Perceptomatrix>(helmet).is_none()
        || !crate::storage::in_reach(world, mob, helmet)
    {
        return;
    }
    world.despawn(core);
    world
        .entity_mut(helmet)
        .insert(Perceptomatrix { core: true });
    update_perceptomatrix(world, helmet, true);
    sound_at(
        world,
        helmet,
        "sound/machines/crate/crate_open.ogg",
        50.0,
        false,
    );
}

/// `TRAIT_HALLUCINATION_IMMUNE` of an active perceptomatrix on the head.
pub fn hallucination_immune(world: &World, mob: Entity) -> bool {
    world
        .get::<Inventory>(mob)
        .and_then(|i| i.head)
        .and_then(|h| world.get::<Perceptomatrix>(h))
        .is_some_and(|p| p.core)
}

/// `SIGNAL_ADDTRAIT(TRAIT_HALLUCINATION_IMMUNE)`: a running hallucination ends on the wearer.
fn perceptomatrix_wearers(
    helmets: Query<(Entity, &Perceptomatrix, &CarriedBy)>,
    inventories: Query<&Inventory>,
    hallucinating: Query<(), With<crate::hallucination::Hallucinating>>,
    mut commands: Commands,
) {
    for (helmet, p, carried) in &helmets {
        let mob = carried.0;
        if p.core
            && hallucinating.contains(mob)
            && inventories.get(mob).is_ok_and(|i| i.head == Some(helmet))
        {
            commands
                .entity(mob)
                .remove::<crate::hallucination::Hallucinating>();
        }
    }
}

/// `examine`: the warnings of the monkey helmet, the missing core of the helm.
pub fn examine_lines(world: &World, item: Entity, lines: &mut Vec<crate::examine::Line>) {
    if world.get::<MonkeyHelmet>(item).is_some() {
        for (span, text) in [
            (
                Span::BoldWarning,
                "---WARNING: REMOVAL OF HELMET ON SUBJECT, OR REPEATED SENTIENCE GENERATION \
                 FAILURES MAY LEAD TO:---",
            ),
            (Span::Warning, "BLOOD RAGE"),
            (Span::Warning, "BRAIN DEATH"),
            (Span::Warning, "PRIMAL GENE ACTIVATION"),
            (Span::Warning, "GENETIC MAKEUP MASS SUSCEPTIBILITY"),
            (
                Span::Notice,
                "Warranty voided if helmet is placed after more than two mind magnification \
                 failures.",
            ),
            (
                Span::BoldNotice,
                "Ask your CMO if mind magnification is right for you!",
            ),
        ] {
            lines.push((span, text.to_owned()));
        }
    }
    if world.get::<Perceptomatrix>(item).is_some_and(|p| !p.core) {
        lines.push((
            Span::Warning,
            "Для работы требуется галлюцинаторное аномальное ядро.".to_owned(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_active_helm_on_the_head_blocks_hallucinations() {
        let mut world = World::new();
        let helm = world.spawn(Perceptomatrix { core: false }).id();
        let mob = world
            .spawn(Inventory {
                head: Some(helm),
                ..default()
            })
            .id();
        assert!(!hallucination_immune(&world, mob));
        world.entity_mut(helm).insert(Perceptomatrix { core: true });
        assert!(hallucination_immune(&world, mob));
    }

    #[test]
    fn the_monkey_helmet_breaks_into_ash_when_taken_off() {
        let mut world = World::new();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        world.init_resource::<crate::combat::CombatRng>();
        let monkey = world.spawn(Inventory::default()).id();
        let helmet = world
            .spawn(MonkeyHelmet {
                magnification: Some(monkey),
                polling: true,
                light: 1,
                rage_chance: -7,
                worn_by: Some(monkey),
            })
            .id();
        // Not on the head: `dropped` destroys it.
        watch_monkey_helmets(&mut world);
        assert!(world.get_entity(helmet).is_err());
    }
}
