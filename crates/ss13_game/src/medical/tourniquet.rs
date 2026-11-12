//! Tourniquets (server), after tg `/obj/item/tourniquet` (`code/game/objects/items/tourniquet.dm`),
//! its `limb_applicable` component and the limb side (`refresh_bleed_rate`, carbon examine,
//! `Topic(remove_tourniquet)`).
//!
//! - A tourniquet on a living carbon goes onto the selected arm, leg or neck after
//!   `do_after(5 s × speed)` (×0.5 on someone else, ×2 on a limb without bleeding, ×2 more on
//!   the neck), hurting 5 brute and 20 stamina; one already there is replaced (it drops).
//! - A tourniqueted limb bleeds a tenth; a leg slows by 0.5 ds a step. Around the neck it
//!   chokes every `Life()`: +1 missed breath and 1 oxygen damage a second, now and then
//!   stamina damage, blur, dizziness and confusion (not for `TRAIT_NOBREATH`).
//! - Examine shows it, with a link within 4 tiles: 4 s of `do_after` and it is in the hands.
// shortcut: the limb keeps the item's type only (a new one is made on removal); no faster
// application with holo-scanned wounds (no `TRAIT_WOUND_SCANNED`), no arm interaction or
// click delay, no suicide; the item drops when its limb is cut off.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};

use super::{MedJob, name_of, visible, wounds};
use crate::combat::CombatRng;
use crate::examine::{ChatLinkClicked, ChatLinkSystems, chat_link};
use crate::health::{DamageType, Health, Stat};
use crate::inventory::Inventory;
use crate::tools::{Actor, DoAfter, do_after, tell};
use crate::zone::BodyZone;

pub const TOURNIQUET: &str = "/obj/item/tourniquet";
/// `do_after(user, 5 SECONDS * speed_multiplier)`, deciseconds.
const APPLY_DS: f32 = 50.0;
/// `Topic(remove_tourniquet)`: `do_after(usr, 4 SECONDS)`.
const REMOVE_DS: f32 = 40.0;
/// `CARBON_EXAMINE_EMBEDDING_MAX_DIST`.
const LINK_DIST: u32 = 4;
/// The leg's `speed_modifier + 0.5`.
pub const LEG_SLOWDOWN: f32 = 0.5;
/// `cached_bleed_rate *= 0.1`.
pub const BLEED_FACTOR: f32 = 0.1;
const TOPIC_PREFIX: &str = "tourniquet";

pub(super) fn build(app: &mut App) {
    app.add_systems(Update, link_clicks.after(ChatLinkSystems));
}

/// `/datum/component/limb_applicable/on_apply` with `do_apply_tourniquet`: the arms, legs and
/// head (the neck), not the chest.
pub fn begin(world: &mut World, actor: Actor, patient: Entity, zone: BodyZone) {
    if world.get::<DoAfter>(actor.mob).is_some() {
        return;
    }
    let Some((missing, bleeding)) = world.get::<wounds::Limbs>(patient).map(|l| {
        let limb = l.limb(zone);
        (limb.missing, limb.wounds.iter().any(|w| w.blood_flow > 0.0))
    }) else {
        return;
    };
    if missing {
        crate::balloon::balloon_alert(world, actor.mob, patient, "нет конечности!");
        return;
    }
    if zone == BodyZone::Chest {
        crate::balloon::balloon_alert(world, actor.mob, patient, "сюда не наложить!");
        return;
    }
    let own = actor.mob == patient;
    let mut speed = if bleeding { 1.0 } else { 2.0 };
    if zone == BodyZone::Head {
        speed *= 2.0;
    } else if !own {
        speed *= 0.5;
    }
    let (user, target) = (name_of(world, actor.mob), name_of(world, patient));
    if zone == BodyZone::Head {
        let (own_text, others) = if own {
            (
                "Вы начинаете затягивать жгут на своей шее, хотя уже сомневаетесь, что это \
                 хорошая идея...?"
                    .to_owned(),
                format!("{user} начинает затягивать жгут на своей шее..."),
            )
        } else {
            (
                format!(
                    "Вы начинаете затягивать жгут на шее {target}, хотя уже сомневаетесь, что \
                     это хорошая идея...?"
                ),
                format!("{user} начинает затягивать жгут на шее {target}..."),
            )
        };
        visible(world, actor.mob, &own_text, &others, Span::Warning);
        if !own {
            crate::tools::tell_mob(
                world,
                patient,
                Span::Danger,
                format!(
                    "{user} начинает затягивать жгут на вашей шее! Это не может быть хорошей идеей...!"
                ),
            );
        }
    } else {
        let part = wounds::zone_prepositional(zone);
        let (own_text, others) = if own {
            (
                format!("Вы начинаете накладывать жгут на свою {}...", zone.ru().1),
                format!(
                    "{user} начинает накладывать жгут на свою {}...",
                    zone.ru().1
                ),
            )
        } else {
            (
                format!(
                    "Вы начинаете накладывать жгут на {} {target}...",
                    zone.ru().1
                ),
                format!(
                    "{user} начинает накладывать жгут на {} {target}...",
                    zone.ru().1
                ),
            )
        };
        visible(world, actor.mob, &own_text, &others, Span::Warning);
        if !own {
            crate::tools::tell_mob(
                world,
                patient,
                Span::Warning,
                format!("{user} начинает накладывать жгут на вашей {part}..."),
            );
        }
    }
    do_after(
        world,
        actor,
        patient,
        APPLY_DS * speed,
        0,
        crate::construction::Job::Medical(MedJob::Tourniquet { zone }),
    );
}

/// The end of the `do_after` (`on_apply_async` -> `apply_item`, `on_applied_to_limb`), checked
/// again: the item still in the hands, the limb still there.
pub fn complete(world: &mut World, actor: Actor, patient: Entity, zone: BodyZone) {
    let Some(item) = actor.item else {
        return;
    };
    let held = world
        .get::<Inventory>(actor.mob)
        .is_some_and(|i| i.slot_of(item).is_some());
    let Some(replaced) = world
        .get::<wounds::Limbs>(patient)
        .filter(|l| !l.limb(zone).missing)
        .map(|l| l.limb(zone).tourniquet)
    else {
        return;
    };
    if !held || zone == BodyZone::Chest {
        return;
    }
    // `override_existing`: the old one comes off (tg leaves it lost inside the limb).
    if let Some(mut limbs) = world.get_mut::<wounds::Limbs>(patient) {
        limbs.limb_mut(zone).tourniquet = true;
    }
    // Pain from the tight wrapping.
    super::hurt_limb(world, patient, zone, DamageType::Brute, 5.0);
    crate::status_effects::adjust_stamina(world, patient, 20.0);
    if let Some(mut inventory) = world.get_mut::<Inventory>(actor.mob)
        && let Some(slot) = inventory.slot_of(item)
    {
        *inventory.slot_mut(slot) = None;
    }
    world.despawn(item);
    if replaced && let Some(&pos) = world.get::<TilePos>(patient) {
        crate::construction::spawn_obj(world, TOURNIQUET, pos);
    }
}

/// The examine lines of a carbon's tourniquets, linked to their removal within reach.
pub fn examine_messages(
    world: &mut World,
    client: Entity,
    examiner: Entity,
    target: Entity,
) -> Vec<ChatMessage> {
    let Some(limbs) = world.get::<wounds::Limbs>(target) else {
        return Vec::new();
    };
    let zones: Vec<BodyZone> = BodyZone::ALL
        .into_iter()
        .filter(|&z| limbs.limb(z).tourniquet && !limbs.limb(z).missing)
        .collect();
    let near = world.get::<Health>(examiner).is_some()
        && matches!(
            (world.get::<TilePos>(examiner), world.get::<TilePos>(target)),
            (Some(a), Some(b)) if a.chebyshev(*b).is_some_and(|d| d <= LINK_DIST)
        );
    let mut out = Vec::new();
    for zone in zones {
        let span = if zone == BodyZone::Head {
            Span::BoldWarning
        } else {
            Span::Notice
        };
        let link = if near {
            Span::Link(chat_link(world, client, topic(target, zone)))
        } else {
            span
        };
        let place = if zone == BodyZone::Head {
            "шее".to_owned()
        } else {
            wounds::zone_prepositional(zone).to_owned()
        };
        out.push(ChatMessage {
            channel: ChatChannel::Info,
            segments: vec![
                ChatSegment::new(span, "На его "),
                ChatSegment::new(span, format!("{place} туго затянут ")),
                ChatSegment::new(link, "жгут"),
                ChatSegment::new(span, "."),
            ],
        });
    }
    out
}

fn topic(mob: Entity, zone: BodyZone) -> String {
    format!("{TOPIC_PREFIX}:{}:{}", mob.to_bits(), zone.index())
}

fn parse_topic(topic: &str) -> Option<(Entity, BodyZone)> {
    let mut parts = topic
        .strip_prefix(TOPIC_PREFIX)?
        .strip_prefix(':')?
        .split(':');
    let mob = Entity::try_from_bits(parts.next()?.parse().ok()?)?;
    let zone = *BodyZone::ALL.get(parts.next()?.parse::<usize>().ok()?)?;
    Some((mob, zone))
}

/// `Topic(remove_tourniquet)`: within reach, the removal starts.
fn link_clicks(world: &mut World, mut cursor: Local<MessageCursor<ChatLinkClicked>>) {
    let Some(messages) = world.get_resource::<Messages<ChatLinkClicked>>() else {
        return;
    };
    let clicks: Vec<ChatLinkClicked> = cursor.read(messages).cloned().collect();
    for click in clicks {
        let Some((patient, zone)) = parse_topic(&click.topic) else {
            continue;
        };
        let Some(mob) = world.resource::<crate::server::Players>().mob(click.client) else {
            continue;
        };
        if !has_tourniquet(world, patient, zone)
            || world.get::<Stat>(mob) != Some(&Stat::Conscious)
            || world.get::<DoAfter>(mob).is_some()
        {
            continue;
        }
        let near = matches!(
            (world.get::<TilePos>(mob), world.get::<TilePos>(patient)),
            (Some(a), Some(b)) if a.chebyshev(*b).is_some_and(|d| d <= 1)
        );
        if !near {
            tell(world, click.client, Span::Warning, "Слишком далеко!");
            continue;
        }
        crate::balloon::balloon_alert_to_viewers(world, patient, "снимают жгут...", None, &[]);
        let actor = Actor {
            client: click.client,
            mob,
            item: None,
        };
        do_after(
            world,
            actor,
            patient,
            REMOVE_DS,
            0,
            crate::construction::Job::Medical(MedJob::RemoveTourniquet { zone }),
        );
    }
}

fn has_tourniquet(world: &World, mob: Entity, zone: BodyZone) -> bool {
    world
        .get::<wounds::Limbs>(mob)
        .is_some_and(|l| l.limb(zone).tourniquet && !l.limb(zone).missing)
}

/// The end of the removal, checked again: the tourniquet into the remover's hands.
pub fn complete_removal(world: &mut World, mob: Entity, patient: Entity, zone: BodyZone) {
    if !has_tourniquet(world, patient, zone) {
        return;
    }
    if let Some(mut limbs) = world.get_mut::<wounds::Limbs>(patient) {
        limbs.limb_mut(zone).tourniquet = false;
    }
    crate::balloon::balloon_alert_to_viewers(world, patient, "жгут снят", None, &[]);
    let Some(item) = crate::construction::spawn_detached(world, TOURNIQUET) else {
        return;
    };
    world.entity_mut(item).insert(ss13_net::server::Networked);
    match world.get::<Inventory>(mob).cloned() {
        Some(inventory) => crate::storage::put_in_hands(world, mob, &inventory, item),
        None => {
            if let Some(&pos) = world.get::<TilePos>(patient) {
                world.entity_mut(item).insert(pos);
            }
        }
    }
}

/// `/obj/item/tourniquet/process` around the neck, for one `Life()` of `secs`.
pub fn neck_tick(world: &mut World, rng: &mut CombatRng, mob: Entity, secs: f32) {
    if !has_tourniquet(world, mob, BodyZone::Head)
        || world.get::<Stat>(mob) == Some(&Stat::Dead)
        || crate::species::has_trait(world, mob, crate::species::Trait::NoBreath)
    {
        return;
    }
    if let Some(mut r) = world.get_mut::<crate::breath::Respiration>(mob) {
        r.losebreath += secs;
    }
    super::hurt_limb(world, mob, BodyZone::Head, DamageType::Oxy, secs);
    if wounds::spt_prob(rng, 6.0, secs) {
        crate::status_effects::adjust_stamina(world, mob, 10.0);
    }
    if wounds::spt_prob(rng, 5.0, secs) {
        crate::vision::adjust_eye_blur(world, mob, 4.0);
    }
    if wounds::spt_prob(rng, 4.0, secs) {
        use crate::status_effects::{Timed, adjust_timed};
        adjust_timed(world, mob, Timed::Dizziness, 4.0, None);
        adjust_timed(world, mob, Timed::Confusion, 2.0, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tourniquet_cuts_the_bleeding_to_a_tenth() {
        let mut limb = wounds::Limb::default();
        wounds::apply_wound(&mut limb, wounds::WoundKind::SlashSevere);
        let open = limb.bleed_rate(BodyZone::LArm, false);
        limb.tourniquet = true;
        let tied = limb.bleed_rate(BodyZone::LArm, false);
        assert!(open > 0.0);
        assert!(
            (tied - open * BLEED_FACTOR).abs() < 1e-6,
            "{open} -> {tied}"
        );
    }

    #[test]
    fn topics_round_trip() {
        let mob = Entity::from_bits(0x0000_0001_0000_0009);
        assert_eq!(
            parse_topic(&topic(mob, BodyZone::RLeg)),
            Some((mob, BodyZone::RLeg))
        );
        assert_eq!(parse_topic("embed:1"), None);
    }
}
