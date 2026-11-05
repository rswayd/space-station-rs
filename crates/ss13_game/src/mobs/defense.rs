//! Fighting with basic mobs, after tg `basic_defense.dm` (`attack_hand`), `item_attack.dm`
//! (`attacked_by` on a living mob), `basic.dm` (`melee_attack`) and `living_defense.dm`
//! (`attack_animal`), with BandaStation's messages (`ru_attack_verb` applied by the converter).
//!
//! - A human's click on a basic mob (checked for sight and reach by `crate::click`): with an
//!   item, its `force` as its `damtype` (`attack_speed` cooldown); with an empty hand out of
//!   combat mode, `response_help` ("гладит") and the mob's `pet_bonus` emote one time in three;
//!   in combat mode, `response_harm` for 5-10 brute (the arm's unarmed damage; basic mobs are
//!   never missed). Damage scales by the mob's `physiology`; an attacked mob remembers its
//!   attacker (tg `ai_retaliate`).
//! - [`melee_attack`]: a basic mob bites (or nuzzles, without damage) whatever living thing it
//!   is next to, `melee_damage_lower..upper` of `melee_damage_type`, then waits
//!   `melee_attack_cooldown`. A human takes it on a random zone (tg `attack_zone_randomiser`).
// shortcut: no armor, blocking, wounds or attack sounds on these hits; add with armor.

use bevy::prelude::*;
use ss13_mapload::{DmItem, DmName};
use ss13_net::chat::server::SendChat;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_world::Moved;

use super::{BasicMob, ai};
use crate::click::{ClickResolved, ResolveClicks};
use crate::combat::{ARM, CLICK_CD_MELEE, CombatMode, CombatRng, MeleeSystems, NextMove};
use crate::health::{
    COMBAT_MESSAGE_RANGE, DEFAULT_MESSAGE_RANGE, DamageType, Health, HealthSystems, Stat,
    apply_damage, apply_damage_zone,
};
use crate::inventory::Inventory;
use crate::server::PlayerMob;
use crate::zone::BodyZone;

pub(super) fn build(app: &mut App) {
    app.add_systems(
        Update,
        attacked_by_humans
            .in_set(MeleeSystems)
            .after(ResolveClicks)
            .before(HealthSystems),
    );
}

/// What the actor, the target and everyone else in range read.
struct Seen {
    span: Span,
    range: u32,
    actor: String,
    target: String,
    others: String,
}

fn send_seen(world: &mut World, actor: Entity, target: Entity, pos: TilePos, seen: Seen) {
    let channel = if seen.span == Span::Notice {
        ChatChannel::Info
    } else {
        ChatChannel::Combat
    };
    let message = |text: String, span| ChatMessage {
        channel,
        segments: vec![ChatSegment::new(span, text)],
    };
    let others = crate::obj_damage::clients_in_range(world, pos, seen.range, &[actor, target]);
    world.write_message(SendChat {
        clients: others,
        message: message(seen.others, seen.span),
    });
    if let Some(p) = world.get::<PlayerMob>(target)
        && target != actor
    {
        let span = if seen.span == Span::Notice {
            Span::Notice
        } else {
            Span::BoldWarning
        };
        world.write_message(SendChat {
            clients: vec![p.client],
            message: message(seen.target, span),
        });
    }
    if let Some(p) = world.get::<PlayerMob>(actor) {
        world.write_message(SendChat {
            clients: vec![p.client],
            message: message(seen.actor, seen.span),
        });
    }
}

pub(super) fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<Name>(e)
        .map(|n| n.as_str().to_owned())
        .or_else(|| world.get::<DmName>(e).map(|n| n.name.clone()))
        .unwrap_or_else(|| "Кто-то".to_owned())
}

/// tg `capitalize`.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Applies `amount` of `kind` to a living mob and updates its stat; `zone` for carbons.
pub(super) fn hurt(
    world: &mut World,
    mob: Entity,
    kind: DamageType,
    amount: f32,
    zone: Option<BodyZone>,
) {
    let mut e = world.entity_mut(mob);
    let (Some(mut health), Some(stat)) = (e.get::<Health>().cloned(), e.get::<Stat>().copied())
    else {
        return;
    };
    let mut new = stat;
    match zone {
        Some(zone) if health.basic.is_none() => {
            apply_damage_zone(&mut health, &mut new, zone, kind, amount);
        }
        _ => {
            apply_damage(&mut health, &mut new, kind, amount);
        }
    }
    e.insert(health);
    if new != stat {
        e.insert(new);
    }
}

/// Clicks of humans on basic mobs (tg `attack_hand`, `attackby` -> `attacked_by`).
fn attacked_by_humans(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<ClickResolved>>,
) {
    let clicks: Vec<ClickResolved> = cursor
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for click in clicks {
        let (user, mob) = (click.mob, click.target);
        if click.modifiers.right || user == mob {
            continue;
        }
        if world
            .resource::<crate::security::MeleeHandled>()
            .0
            .contains(&user)
            || world
                .resource::<crate::medical::MedicalHandled>()
                .0
                .contains(&user)
        {
            continue;
        }
        let Some(basic) = world.get::<BasicMob>(mob).cloned() else {
            continue;
        };
        let (Some(&stat), Some(&combat), Some(&NextMove(next))) = (
            world.get::<Stat>(user),
            world.get::<CombatMode>(user),
            world.get::<NextMove>(user),
        ) else {
            continue;
        };
        if stat != Stat::Conscious || now < next {
            continue;
        }
        let held = world
            .get::<Inventory>(user)
            .and_then(Inventory::active_item)
            .and_then(|i| world.get::<DmItem>(i).cloned().map(|d| (i, d)));
        let def = &basic.def;
        let user_name = name_of(world, user);
        let mob_name = name_of(world, mob);
        let mob_stat = world.get::<Stat>(mob).copied().unwrap_or_default();
        match held {
            Some((item, dm)) => {
                // tg `basic_eating/try_feed` (`COMSIG_ATOM_ITEM_INTERACTION`, before the item's
                // own use): out of combat mode the mob eats what it likes.
                if !combat.0 && super::eating::try_feed(world, mob, item, user) {
                    world
                        .entity_mut(user)
                        .insert(NextMove(now + CLICK_CD_MELEE));
                    continue;
                }
                // Items that interact with mobs (medicine, syringes, food out of combat) take
                // the click elsewhere.
                if crate::medical::interacts_with_mobs(&dm.path)
                    || crate::chem::interacts_with_mobs(&dm.path)
                    || (crate::food::is_food(&dm.path) && !combat.0)
                {
                    continue;
                }
                world
                    .entity_mut(user)
                    .insert(NextMove(now + f64::from(dm.attack_speed) / 10.0));
                if dm.force <= 0.0 {
                    continue;
                }
                let kind = DamageType::from_dm(&dm.damtype).unwrap_or(DamageType::Brute);
                let item_name = name_of(world, item);
                hurt(world, mob, kind, dm.force, None);
                ai::record_attacker(world, mob, user);
                send_seen(
                    world,
                    user,
                    mob,
                    click.target_pos,
                    Seen {
                        span: Span::Danger,
                        range: COMBAT_MESSAGE_RANGE,
                        actor: format!("Вы атакуете {mob_name} с помощью {item_name}!"),
                        target: format!("{user_name} атакует вас с помощью {item_name}!"),
                        others: format!("{user_name} атакует {mob_name} с помощью {item_name}!"),
                    },
                );
            }
            None if !combat.0 => {
                world
                    .entity_mut(user)
                    .insert(NextMove(now + CLICK_CD_MELEE));
                if mob_stat == Stat::Dead {
                    continue;
                }
                let (cont, simple) = (&def.response_help_continuous, &def.response_help_simple);
                send_seen(
                    world,
                    user,
                    mob,
                    click.target_pos,
                    Seen {
                        span: Span::Notice,
                        range: DEFAULT_MESSAGE_RANGE,
                        actor: format!("Вы {simple} {mob_name}."),
                        target: format!("{} {cont} вас.", capitalize(&user_name)),
                        others: format!("{} {cont} {mob_name}.", capitalize(&user_name)),
                    },
                );
                pet_bonus(world, mob, &basic);
                super::parrot::on_pet(world, mob);
            }
            None => {
                world
                    .entity_mut(user)
                    .insert(NextMove(now + CLICK_CD_MELEE));
                let mut rng = CombatRng(world.resource::<CombatRng>().0);
                let damage = rng.range(ARM.damage_low, ARM.damage_high) as f32;
                world.resource_mut::<CombatRng>().0 = rng.0;
                let (cont, simple) = (&def.response_harm_continuous, &def.response_harm_simple);
                send_seen(
                    world,
                    user,
                    mob,
                    click.target_pos,
                    Seen {
                        span: Span::Danger,
                        range: COMBAT_MESSAGE_RANGE,
                        actor: format!("Вы {simple} {mob_name}!"),
                        target: format!("{} {cont} вас!", capitalize(&user_name)),
                        others: format!("{} {cont} {mob_name}!", capitalize(&user_name)),
                    },
                );
                hurt(world, mob, DamageType::Brute, damage, None);
                ai::record_attacker(world, mob, user);
            }
        }
    }
}

/// tg `pet_bonus`: one time in three the pet does its emote (`/datum/emote/...` messages in
/// BandaStation's `ru_emotes.toml` translation).
fn pet_bonus(world: &mut World, mob: Entity, basic: &BasicMob) {
    let path = basic.def.path.as_str();
    let under = |root: &str| ss13_mapload::fidelity::is_under(path, root);
    let text = if under("/mob/living/basic/pet/dog") {
        "радостно гавкает!"
    } else if under("/mob/living/basic/pet/fox") {
        "радостно тявкает!"
    } else if under("/mob/living/basic/pet/cat") {
        "purrs."
    } else if under("/mob/living/basic/mouse") {
        "пищит!"
    } else if under("/mob/living/basic/carp") {
        "булькает!"
    } else {
        return;
    };
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let roll = rng.chance(33.0);
    world.resource_mut::<CombatRng>().0 = rng.0;
    if roll {
        emote(world, mob, text);
    }
}

/// tg `manual_emote` / `emote`: `<b>Name</b> text` to everyone in view.
pub fn emote(world: &mut World, mob: Entity, text: &str) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let name = name_of(world, mob);
    let clients = crate::obj_damage::clients_in_range(world, pos, DEFAULT_MESSAGE_RANGE, &[]);
    world.write_message(SendChat {
        clients,
        message: ss13_chat::server::emote_message(&name, text),
    });
}

/// tg `/mob/living/basic/proc/melee_attack` on a living `target` next to `attacker`:
/// `attack_animal`. Returns whether the attack happened (the cooldown starts then).
pub fn melee_attack(world: &mut World, attacker: Entity, target: Entity) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(basic) = world.get::<BasicMob>(attacker).cloned() else {
        return false;
    };
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(attacker), world.get::<TilePos>(target))
    else {
        return false;
    };
    let reach = world
        .resource::<ss13_world::SpatialIndex>()
        .adjacent(from, to, |e| e == target);
    let alive = world.get::<Stat>(attacker) == Some(&Stat::Conscious);
    let ready = world.get::<NextMove>(attacker).is_none_or(|n| now >= n.0);
    if !reach || !alive || !ready || world.get::<Health>(target).is_none() {
        return false;
    }
    // tg `face_atom`.
    if let Some(dir) = crate::pull::dir_between(from, to)
        && let Some(mut facing) = world.get_mut::<ss13_shared::Facing>(attacker)
    {
        facing.set_if_neq(ss13_shared::Facing(dir));
    }
    let def = &basic.def;
    let user = capitalize(&name_of(world, attacker));
    let victim = name_of(world, target);
    world
        .entity_mut(attacker)
        .insert(NextMove(now + f64::from(def.melee_attack_cooldown) / 10.0));
    if def.melee_damage_upper == 0.0 {
        let (cont, simple) = (&def.friendly_verb_continuous, &def.friendly_verb_simple);
        send_seen(
            world,
            attacker,
            target,
            to,
            Seen {
                span: Span::Notice,
                range: COMBAT_MESSAGE_RANGE,
                actor: format!("Вы {simple} {victim}!"),
                target: format!("{user} {cont} вас!"),
                others: format!("{user} {cont} {victim}!"),
            },
        );
        return true;
    }
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    // DM `rand(lower, upper)` on whole numbers.
    let damage = rng.range(def.melee_damage_lower as u32, def.melee_damage_upper as u32) as f32;
    // `attack_zone_randomiser` over `dismembering_strike`'s zones; hands are the arms here.
    let zone = [
        BodyZone::Chest,
        BodyZone::LArm,
        BodyZone::RArm,
        BodyZone::LLeg,
        BodyZone::RLeg,
    ][rng.range(0, 4) as usize];
    world.resource_mut::<CombatRng>().0 = rng.0;
    let (cont, simple) = (&def.attack_verb_continuous, &def.attack_verb_simple);
    send_seen(
        world,
        attacker,
        target,
        to,
        Seen {
            span: Span::Danger,
            range: COMBAT_MESSAGE_RANGE,
            actor: format!("Вы {simple} {victim}!"),
            target: format!("{user} {cont} вас!"),
            others: format!("{user} {cont} {victim}!"),
        },
    );
    let kind = DamageType::from_dm(&def.melee_damage_type).unwrap_or(DamageType::Brute);
    hurt(world, target, kind, damage, Some(zone));
    ai::record_attacker(world, target, attacker);
    true
}

/// tg mouse `on_entered` ("Сквик!" to a human walking over a conscious mouse) and the
/// cockroach's `squashable` (a 50% chance to be stepped on, 1 brute; it is gibbed).
pub(super) fn squeak_on_cross(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<Moved>>,
) {
    let moved: Vec<Moved> = cursor
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    for m in moved {
        let Some(client) = world.get::<PlayerMob>(m.entity).map(|p| p.client) else {
            continue;
        };
        if world
            .get::<Stat>(m.entity)
            .is_some_and(|s| *s != Stat::Conscious)
        {
            continue;
        }
        let here: Vec<Entity> = world
            .resource::<ss13_world::SpatialIndex>()
            .on_tile(m.to)
            .to_vec();
        for critter in here {
            let Some(path) = world.get::<BasicMob>(critter).map(|b| b.def.path.clone()) else {
                continue;
            };
            let alive = world.get::<Stat>(critter) == Some(&Stat::Conscious);
            if ss13_mapload::fidelity::is_under(&path, "/mob/living/basic/mouse") && alive {
                world.write_message(SendChat {
                    clients: vec![client],
                    message: ChatMessage::simple(ChatChannel::Info, Span::Notice, "Сквик!"),
                });
            } else if ss13_mapload::fidelity::is_under(&path, "/mob/living/basic/cockroach") {
                let mut rng = CombatRng(world.resource::<CombatRng>().0);
                let squash = rng.chance(50.0);
                world.resource_mut::<CombatRng>().0 = rng.0;
                if squash || !alive {
                    hurt(world, critter, DamageType::Brute, 1.0, None);
                }
            }
        }
    }
}
