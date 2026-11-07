//! Hand gestures (server), after tg `/obj/item/hand_item` (`code/game/objects/items/hand_items.dm`)
//! and the carbon emotes that make them (`living/carbon/emote.dm`).
//!
//! - `*circle`, `*slap`, `*shoesteal` ([`ss13_chat::emote::HAND_GESTURES`]) put the gesture in a
//!   free hand (the active one first). A gesture is the hand itself (`DROPDEL`): out of the hand
//!   it is gone.
//! - The slapper slaps a mob: by the selected zone a slap in the face (a facepalm on oneself,
//!   waking an unconscious mob: −7.5 s sleep, −5 s unconsciousness), on the wrist or plain, with
//!   `slap.ogg`. On a table a click slaps it (three times, then the hand is spent), a right click
//!   slams the fist down (only a fresh hand) and spends it.
//! - The circled hand: a mob next to its owner who examines the owner falls for it — 0.4 s, then
//!   0.6 s later the owner bops them (15 stamina, `hit_punch.ogg`) unless they moved away or the
//!   owner cannot act. The owner examining themselves bops themselves 1 s later (100 stamina,
//!   1 s knockdown). The hand is spent either way.
//! - The stealing hand: lying down next to a human with shoes, the user takes them off in the
//!   shoes' strip delay and holds them.
// shortcut: no offering (`/datum/status_effect/offering`, high fives, the kiss slap), no
// `*noogie`, `*hand` or kisses (`*kiss`, their projectiles); no attack animation, tail unwag,
// hulk variants or "gottem" achievement.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_chat::server::Emoted;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;

use super::DeviceJob;
use crate::click::ExamineClick;
use crate::health::{BodyPosition, Health, Stat, ZoneSelected};
use crate::inventory::{CarriedBy, Inventory};
use crate::sound::{PlaySound, playsound};
use crate::status_effects::Incap;
use crate::tools::{Actor, do_after, tell};
use crate::zone::TargetZone;

pub const CIRCLE: &str = "/obj/item/hand_item/circlegame";
pub const SLAPPER: &str = "/obj/item/hand_item/slapper";
pub const STEALER: &str = "/obj/item/hand_item/stealer";
const SLAP_SOUND: &str = "sound/items/weapons/slap.ogg";
const TABLE_SOUND: &str = "sound/effects/tableslam.ogg";
const BOP_SOUND: &str = "sound/effects/hit_punch.ogg";
/// `table_smacks_left`.
const TABLE_SMACKS: u8 = 3;

/// On a gesture in a hand: what it is and its uses.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct HandItem {
    pub kind: Gesture,
    /// The slapper's `table_smacks_left`.
    pub smacks_left: u8,
    /// The circled hand's suckers: (sucker, due time, stage).
    pending: Vec<(Entity, f64, CircleStage)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    Circle,
    Slapper,
    Stealer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CircleStage {
    /// `waitASecond`.
    Noticed,
    /// `selfGottem`.
    SelfBop,
    /// `GOTTEM`.
    Bop,
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // The slapper on a mob or a table, the stealing hand on a human.
    app.add_click_handler(ClickPhase::HeldOnTarget, 233, |w, c| {
        c.held.is_some_and(|h| {
            crate::devices::hand_items::held_on_target(
                w,
                c.client,
                c.mob,
                h,
                c.target,
                c.modifiers.right,
            )
        })
    });
    app.add_message::<Emoted>()
        .add_message::<ExamineClick>()
        .add_systems(
            Update,
            (make_gestures, examined, circle_timers, drop_del).chain(),
        );
}

/// Gestures never attack (`crate::combat`): their `attack` is their own.
pub fn interacts_with_mobs(path: &str) -> bool {
    ss13_mapload::fidelity::is_under(path, "/obj/item/hand_item")
}

fn gesture_of(world: &World, item: Entity) -> Option<Gesture> {
    world.get::<HandItem>(item).map(|h| h.kind)
}

/// The gesture's emote: it goes into a free hand, or the emote says why not.
fn make_gestures(world: &mut World, mut cursor: Local<MessageCursor<Emoted>>) {
    let emotes: Vec<(Entity, &'static str)> = cursor
        .read(world.resource::<Messages<Emoted>>())
        .filter(|e| ss13_chat::emote::HAND_GESTURES.contains(&e.key))
        .map(|e| (e.speaker, e.key))
        .collect();
    for (mob, key) in emotes {
        let (kind, path) = match key {
            "circle" => (Gesture::Circle, CIRCLE),
            "slap" => (Gesture::Slapper, SLAPPER),
            _ => (Gesture::Stealer, STEALER),
        };
        let given = give(world, mob, kind, path);
        let client = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client);
        match (kind, given, client) {
            (Gesture::Circle, true, Some(c)) => {
                tell(world, c, Span::Notice, "Вы делаете кольцо из пальцев.");
            }
            (Gesture::Circle, false, Some(c)) => tell(
                world,
                c,
                Span::Warning,
                "У вас нет свободной руки, чтобы сделать кольцо.",
            ),
            (Gesture::Slapper, true, Some(c)) => {
                tell(world, c, Span::Notice, "Вы заносите руку для пощёчины.");
            }
            (Gesture::Slapper, false, Some(c)) => {
                tell(world, c, Span::Warning, "Вы не в состоянии дать пощёчину.")
            }
            (Gesture::Stealer, true, _) => {
                crate::balloon::balloon_alert(world, mob, mob, "готовимся красть обувь...");
            }
            (Gesture::Stealer, false, _) => {
                crate::balloon::balloon_alert(world, mob, mob, "не украсть обувь!");
            }
            _ => {}
        }
    }
}

/// `new(user)` + `put_in_hands`: the active hand first, then the other.
fn give(world: &mut World, mob: Entity, kind: Gesture, path: &str) -> bool {
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return false;
    };
    let Some(hand) = [inventory.active, 1 - inventory.active]
        .into_iter()
        .find(|&h| {
            inventory.hands[h].is_none() && crate::medical::bodyparts::has_hand(world, mob, h)
        })
    else {
        return false;
    };
    let Some(item) = crate::construction::spawn_detached(world, path) else {
        return false;
    };
    world.entity_mut(item).remove::<TilePos>().insert((
        CarriedBy(mob),
        HandItem {
            kind,
            smacks_left: TABLE_SMACKS,
            pending: Vec::new(),
        },
    ));
    if let Some(mut inv) = world.get_mut::<Inventory>(mob) {
        inv.hands[hand] = Some(item);
    }
    crate::inventory::refresh_look(world, mob);
    true
}

/// `qdel(src)` of a gesture: out of the hand and gone.
fn spend(world: &mut World, item: Entity) {
    let holder = world.get::<CarriedBy>(item).map(|c| c.0);
    if let Some(mob) = holder
        && let Some(mut inv) = world.get_mut::<Inventory>(mob)
    {
        for hand in inv.hands.iter_mut().filter(|h| **h == Some(item)) {
            *hand = None;
        }
    }
    if world.get_entity(item).is_ok() {
        world.despawn(item);
    }
    if let Some(mob) = holder {
        crate::inventory::refresh_look(world, mob);
    }
}

/// `DROPDEL`: a gesture that is not in its holder's hands any more is gone.
fn drop_del(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, Option<&CarriedBy>), With<HandItem>>();
    let items: Vec<(Entity, Option<Entity>)> =
        q.iter(world).map(|(e, c)| (e, c.map(|c| c.0))).collect();
    for (item, holder) in items {
        let in_hand = holder.is_some_and(|mob| {
            world
                .get::<Inventory>(mob)
                .is_some_and(|i| i.hands.contains(&Some(item)))
        });
        if !in_hand {
            spend(world, item);
        }
    }
}

fn sound(world: &mut World, at: Entity, file: &str, volume: f32) {
    if let Some(pos) = crate::explosions::turf_of(world, at) {
        playsound(
            world,
            PlaySound::new(pos, file, volume, true).extrarange(-1.0),
        );
    }
}

/// The gesture's `pre_attack`/`attack` on what was clicked. True when it was one of ours.
pub fn held_on_target(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    right: bool,
) -> bool {
    match gesture_of(world, held) {
        Some(Gesture::Slapper) if crate::tables::is_table(world, target) => {
            if right {
                slam_table(world, mob, held, target);
            } else {
                slap_table(world, mob, held, target);
            }
            true
        }
        Some(Gesture::Slapper) if world.get::<Health>(target).is_some() => {
            slap(world, mob, target);
            true
        }
        Some(Gesture::Stealer) => {
            steal(world, client, mob, held, target);
            true
        }
        // The circled hand bops nothing by hand (`force = 0`).
        Some(_) => true,
        None => false,
    }
}

/// `slapper/attack`.
fn slap(world: &mut World, user: Entity, slapped: Entity) {
    let zone = world
        .get::<ZoneSelected>(user)
        .map_or(TargetZone::Chest, |z| z.0);
    let them = crate::security::name_of(world, slapped);
    let me = crate::security::name_of(world, user);
    let (own, others, span) = match zone {
        TargetZone::Head | TargetZone::Mouth if user == slapped => (
            "Вы бьёте себя ладонью по лицу.".to_owned(),
            format!("{me} бьёт себя ладонью по лицу!"),
            Span::Notice,
        ),
        TargetZone::Head | TargetZone::Mouth
            if crate::status_effects::knocked_out(world, slapped) =>
        {
            crate::status_effects::adjust(world, slapped, Incap::Sleeping, -7.5);
            crate::status_effects::adjust(world, slapped, Incap::Unconscious, -5.0);
            (
                format!("Вы бьёте {them} по лицу, пытаясь разбудить!"),
                format!("{me} бьёт {them} по лицу, пытаясь разбудить!"),
                Span::Notice,
            )
        }
        TargetZone::Head | TargetZone::Mouth => (
            format!("Вы даёте {them} пощёчину!"),
            format!("{me} даёт {them} пощёчину!"),
            Span::Danger,
        ),
        TargetZone::LArm | TargetZone::RArm => (
            format!("Вы шлёпаете {them} по запястью!"),
            format!("{me} шлёпает {them} по запястью!"),
            Span::Danger,
        ),
        _ => (
            format!("Вы шлёпаете {them}!"),
            format!("{me} шлёпает {them}!"),
            Span::Danger,
        ),
    };
    crate::medical::visible(world, user, &own, &others, span);
    sound(world, slapped, SLAP_SOUND, 50.0);
}

/// `slap_table`: three slaps, then the hand is spent.
fn slap_table(world: &mut World, user: Entity, hand: Entity, table: Entity) {
    sound(world, table, TABLE_SOUND, 40.0);
    let me = crate::security::name_of(world, user);
    let name = crate::security::name_of(world, table);
    crate::medical::visible(
        world,
        user,
        &format!("Вы хлопаете ладонью по {name}."),
        &format!("{me} хлопает ладонью по {name}."),
        Span::Notice,
    );
    let left = world.get_mut::<HandItem>(hand).map(|mut h| {
        h.smacks_left = h.smacks_left.saturating_sub(1);
        h.smacks_left
    });
    if left == Some(0) {
        spend(world, hand);
    }
}

/// `slam_table`: a fresh hand slams the fist down and is spent; a used one only slaps.
fn slam_table(world: &mut World, user: Entity, hand: Entity, table: Entity) {
    if world
        .get::<HandItem>(hand)
        .is_some_and(|h| h.smacks_left < TABLE_SMACKS)
    {
        slap_table(world, user, hand, table);
        return;
    }
    sound(world, table, TABLE_SOUND, 110.0);
    let me = crate::security::name_of(world, user);
    let name = crate::security::name_of(world, table);
    crate::medical::visible(
        world,
        user,
        &format!("Вы с размаху бьёте кулаком по {name}!"),
        &format!("{me} с размаху бьёт кулаком по {name}!"),
        Span::BoldWarning,
    );
    spend(world, hand);
}

/// `stealer/attack`: lying next to a human with shoes, the strip starts.
fn steal(world: &mut World, client: Entity, user: Entity, hand: Entity, target: Entity) {
    if world.get::<crate::human::HumanLook>(target).is_none() {
        return;
    }
    if target == user {
        tell(
            world,
            client,
            Span::Notice,
            "Зачем пытаться украсть собственную обувь?",
        );
        return;
    }
    let Some(shoes) = world.get::<Inventory>(target).and_then(|i| i.shoes) else {
        return;
    };
    if !world.get::<BodyPosition>(user).is_some_and(|b| b.lying()) {
        return;
    }
    let actor = Actor {
        client,
        mob: user,
        item: Some(hand),
    };
    if !do_after(
        world,
        actor,
        target,
        crate::strip::STRIP_DELAY_DS,
        0,
        crate::construction::Job::Device(DeviceJob::StealShoes(shoes)),
    ) {
        return;
    }
    let (me, them) = (
        crate::security::name_of(world, user),
        crate::security::name_of(world, target),
    );
    let shoe_name = crate::security::name_of(world, shoes);
    crate::medical::visible(
        world,
        user,
        &format!("Вы начинаете красть {shoe_name} у {them}..."),
        &format!("{me} начинает красть {shoe_name} у {them}!"),
        Span::Warning,
    );
    if let Some(c) = world
        .get::<crate::server::PlayerMob>(target)
        .map(|p| p.client)
    {
        tell(
            world,
            c,
            Span::Danger,
            format!("{me} начинает красть вашу обувь!"),
        );
    }
}

/// The end of the theft: the same shoes still worn, the user still next to the target; they
/// drop and go into a free hand of the user.
pub fn complete_steal(world: &mut World, user: Entity, target: Entity, shoes: Entity) {
    if world.get::<Inventory>(target).and_then(|i| i.shoes) != Some(shoes) {
        return;
    }
    let (Some(&pos), Some(&user_pos)) = (world.get::<TilePos>(target), world.get::<TilePos>(user))
    else {
        return;
    };
    if !user_pos.chebyshev(pos).is_some_and(|d| d <= 1) {
        return;
    }
    crate::inventory::unequip(world, target, pos, InventorySlot::Shoes);
    world.entity_mut(shoes).remove::<CarriedBy>().insert(pos);
    crate::inventory::refresh_look(world, target);
    if let Some(inventory) = world.get::<Inventory>(user).cloned()
        && let Some(hand) = [inventory.active, 1 - inventory.active]
            .into_iter()
            .find(|&h| inventory.hands[h].is_none())
    {
        crate::inventory::pick_up(world, user, shoes, hand);
        crate::inventory::refresh_look(world, user);
    }
    let (me, them) = (
        crate::security::name_of(world, user),
        crate::security::name_of(world, target),
    );
    let shoe_name = crate::security::name_of(world, shoes);
    crate::medical::visible(
        world,
        user,
        &format!("Вы украли {shoe_name} у {them}!"),
        &format!("{me} украл {shoe_name} у {them}!"),
        Span::Warning,
    );
    if let Some(c) = world
        .get::<crate::server::PlayerMob>(target)
        .map(|p| p.client)
    {
        tell(world, c, Span::Danger, format!("{me} украл вашу обувь!"));
    }
}

/// The circled hand of `owner`, if one is in its hands.
fn circle_of(world: &World, owner: Entity) -> Option<Entity> {
    world
        .get::<Inventory>(owner)?
        .hands
        .iter()
        .flatten()
        .copied()
        .find(|&h| gesture_of(world, h) == Some(Gesture::Circle))
}

fn adjacent(world: &World, a: Entity, b: Entity) -> bool {
    match (world.get::<TilePos>(a), world.get::<TilePos>(b)) {
        (Some(x), Some(y)) => x.chebyshev(*y).is_some_and(|d| d <= 1),
        _ => false,
    }
}

/// `ownerExamined`: a living examiner within reach falls for it in 0.4 s.
fn examined(world: &mut World, mut cursor: Local<MessageCursor<ExamineClick>>) {
    let clicks: Vec<ExamineClick> = cursor
        .read(world.resource::<Messages<ExamineClick>>())
        .copied()
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for click in clicks {
        let Some(hand) = circle_of(world, click.target) else {
            continue;
        };
        if world.get::<Health>(click.examiner).is_none()
            || !adjacent(world, click.target, click.examiner)
        {
            continue;
        }
        if let Some(mut h) = world.get_mut::<HandItem>(hand) {
            h.pending
                .push((click.examiner, now + 0.4, CircleStage::Noticed));
        }
    }
}

/// The circle game's timers (`addtimer` stages).
fn circle_timers(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &HandItem)>();
    let due: Vec<(Entity, Entity, CircleStage)> = q
        .iter(world)
        .flat_map(|(e, h)| {
            h.pending
                .iter()
                .filter(move |p| p.1 <= now)
                .map(move |p| (e, p.0, p.2))
        })
        .collect();
    for (hand, sucker, stage) in due {
        if let Some(mut h) = world.get_mut::<HandItem>(hand) {
            h.pending
                .retain(|p| !(p.0 == sucker && p.2 == stage && p.1 <= now));
        }
        let Some(owner) = world.get::<CarriedBy>(hand).map(|c| c.0) else {
            continue;
        };
        if world.get_entity(sucker).is_err() {
            continue;
        }
        circle_stage(world, hand, owner, sucker, stage, now);
    }
}

fn client_of(world: &World, mob: Entity) -> Option<Entity> {
    world.get::<crate::server::PlayerMob>(mob).map(|p| p.client)
}

fn circle_stage(
    world: &mut World,
    hand: Entity,
    owner: Entity,
    sucker: Entity,
    stage: CircleStage,
    now: f64,
) {
    let name = crate::security::name_of(world, hand);
    match stage {
        CircleStage::Noticed => {
            let (text, next, delay) = if owner == sucker {
                (
                    format!("Секундочку... вы только что посмотрели на свою {name}!"),
                    CircleStage::SelfBop,
                    1.0,
                )
            } else {
                ("Секундочку... это что, -".to_owned(), CircleStage::Bop, 0.6)
            };
            if let Some(c) = client_of(world, sucker) {
                tell(world, c, Span::Danger, text);
            }
            if let Some(mut h) = world.get_mut::<HandItem>(hand) {
                h.pending.push((sucker, now + delay, next));
            }
        }
        CircleStage::SelfBop => {
            sound(world, owner, BOP_SOUND, 50.0);
            let me = crate::security::name_of(world, owner);
            crate::medical::visible(
                world,
                owner,
                &format!("Вы стыдливо стукаете себя своей {name}."),
                &format!("{me} стыдливо стукает себя своей {name}."),
                Span::Danger,
            );
            crate::status_effects::adjust_stamina(world, owner, 100.0);
            crate::status_effects::apply(world, owner, Incap::Knockdown, 1.0);
            spend(world, hand);
        }
        CircleStage::Bop => {
            let me = crate::security::name_of(world, owner);
            let can_act = !crate::status_effects::incapacitation(world, owner).incapacitated
                && world.get::<Stat>(owner) == Some(&Stat::Conscious);
            if !adjacent(world, owner, sucker) || !can_act {
                if let Some(c) = client_of(world, sucker) {
                    tell(
                        world,
                        c,
                        Span::Notice,
                        format!(
                            "Фух... вы отошли раньше, чем {me} заметил, что вы увидели {name}..."
                        ),
                    );
                }
                return;
            }
            let them = crate::security::name_of(world, sucker);
            if let Some(c) = client_of(world, owner) {
                tell(
                    world,
                    c,
                    Span::Warning,
                    format!("{them} смотрит на вашу {name} и пытается отвести взгляд, но поздно!"),
                );
            }
            if let Some(c) = client_of(world, sucker) {
                tell(
                    world,
                    c,
                    Span::Danger,
                    format!("{me} стукает вас своей {name}!"),
                );
            }
            sound(world, owner, BOP_SOUND, 50.0);
            crate::medical::visible(
                world,
                owner,
                &format!("Вы стукаете {them} своей {name}!"),
                &format!("{me} стукает {them} своей {name}!"),
                Span::Danger,
            );
            crate::status_effects::adjust_stamina(world, sucker, 15.0);
            spend(world, hand);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A table slap spends one of three smacks; the third spends the hand.
    #[test]
    fn three_table_slaps_spend_the_hand() {
        let mut world = World::new();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        world.init_resource::<Messages<PlaySound>>();
        let hand = world
            .spawn(HandItem {
                kind: Gesture::Slapper,
                smacks_left: TABLE_SMACKS,
                pending: Vec::new(),
            })
            .id();
        let mob = world
            .spawn(Inventory {
                hands: [Some(hand), None],
                ..default()
            })
            .id();
        world.entity_mut(hand).insert(CarriedBy(mob));
        let table = world.spawn_empty().id();
        slap_table(&mut world, mob, hand, table);
        slap_table(&mut world, mob, hand, table);
        assert_eq!(world.get::<HandItem>(hand).unwrap().smacks_left, 1);
        // A used hand cannot slam: it slaps instead, and that was the last smack.
        slam_table(&mut world, mob, hand, table);
        assert!(world.get_entity(hand).is_err());
        assert_eq!(world.get::<Inventory>(mob).unwrap().hands[0], None);
    }
}
