//! Melee (server): combat mode, unarmed strikes, hugs and attacks with held items, after tg
//! `code/_onclick/click.dm` (`ClickOn`), `_species.dm` (`spec_attack_hand`, `help`, `harm`) and
//! `item_attack.dm` (`attackby`, `attacked_by`).
//!
//! A click arrives checked for sight and reach by `crate::click` (from a conscious mob). At the
//! moment of effect this module checks again that the attacker is conscious and off its click
//! cooldown (tg `next_move`), that the target is a living mob, and applies:
//! - a held item: its `force` as its `damtype` (`attacked_by`), cooldown `attack_speed`; in tg an
//!   item attacks whatever the combat mode;
//! - an empty hand in combat mode: a punch (arm, 5-10, misses `20 - 10 + own brute+burn / 2`
//!   percent of the time, capped at 80) or, on a lying target, a kick (leg, 7-15, never misses);
//! - an empty hand without combat mode: a hug, a shake for a lying stable target, CPR for a
//!   lying target that is not (`crate::medical::do_cpr`);
//!
//! with `CLICK_CD_MELEE` (0.8 s) after an empty-hand click on a mob. Messages go to the clients in
//! view, worded as BandaStation's translation.
//!
//! Hits land on a bodypart: the attacker's selected zone, or a random one (tg
//! `get_random_valid_zone`). Medical items never attack (`crate::medical` takes their clicks).

use bevy::prelude::*;
use ss13_mapload::{DmItem, DmName, DmWounding};
use ss13_net::chat::server::SendChat;
use ss13_net::server::IntentReceived;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::ClientIntent;
use ss13_shared::{Dir, Facing, TilePos};
use ss13_world::SpatialIndex;

use crate::click::{ClickResolved, ResolveClicks};
use crate::health::{
    BodyPosition, COMBAT_MESSAGE_RANGE, DEFAULT_MESSAGE_RANGE, DamageType, Health, HealthSystems,
    Stat, ZoneSelected, apply_damage_zone, clients_in_view,
};
use crate::inventory::Inventory;
use crate::medical::random_valid_zone;
use crate::server::{PlayerMob, Players};
use crate::sound::PlaySound;
use crate::zone::BodyZone;
use ss13_mapload::sound::files;

/// `CLICK_CD_MELEE`, seconds.
pub const CLICK_CD_MELEE: f64 = 0.8;
/// `UNARMED_MISS_CHANCE_BASE`, `UNARMED_MISS_CHANCE_MAX`, percent.
pub const UNARMED_MISS_CHANCE_BASE: f32 = 20.0;
pub const UNARMED_MISS_CHANCE_MAX: f32 = 80.0;

/// tg `combat_mode` (toggled with F).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CombatMode(pub bool);

/// tg `next_move`: `Time::elapsed_secs_f64` before which the mob's clicks do nothing.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct NextMove(pub f64);

/// Combat rolls (DM `rand`/`prob`): splitmix64, seeded from the clock at startup and reseeded
/// from the map seed ([`crate::server::seed_rngs`]).
#[derive(Resource, Debug)]
pub struct CombatRng(pub u64);

impl Default for CombatRng {
    fn default() -> Self {
        let seed = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        Self(seed)
    }
}

impl CombatRng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// DM `rand(low, high)` on integers, both ends included.
    pub fn range(&mut self, low: u32, high: u32) -> u32 {
        low + (self.next() % (u64::from(high - low) + 1)) as u32
    }

    /// DM `rand()`: uniform in [0, 1).
    pub fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// DM `prob(percent)`.
    pub fn chance(&mut self, percent: f32) -> bool {
        (self.next() % 10_000) as f32 / 100.0 < percent
    }
}

/// The limb an unarmed strike uses (`/obj/item/bodypart/arm` and `leg` of a human).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limb {
    /// BandaStation's `unarmed_attack_verbs` (third person).
    pub verb: &'static str,
    pub damage_low: u32,
    pub damage_high: u32,
    /// `unarmed_effectiveness`.
    pub effectiveness: f32,
}

pub const ARM: Limb = Limb {
    verb: "бьет",
    damage_low: 5,
    damage_high: 10,
    effectiveness: 10.0,
};

pub const LEG: Limb = Limb {
    verb: "пинает",
    damage_low: 7,
    damage_high: 15,
    effectiveness: 15.0,
};

/// tg `brain.get_attacking_limb`: legs against a lying target, else the active hand.
pub fn attacking_limb(target_lying: bool) -> Limb {
    if target_lying { LEG } else { ARM }
}

/// tg `harm` miss chance: lying targets are never missed; otherwise the limb's accuracy and the
/// attacker's own brute + burn.
pub fn miss_chance(limb: Limb, attacker_brute_burn: f32, target_lying: bool) -> f32 {
    if target_lying {
        0.0
    } else {
        (UNARMED_MISS_CHANCE_BASE - limb.effectiveness + attacker_brute_burn / 2.0)
            .clamp(0.0, UNARMED_MISS_CHANCE_MAX)
    }
}

/// One unarmed strike: `Some(damage)` on a hit (rolled first, as tg does).
pub fn unarmed_strike(
    rng: &mut CombatRng,
    limb: Limb,
    attacker_brute_burn: f32,
    target_lying: bool,
) -> Option<u32> {
    let damage = rng.range(limb.damage_low, limb.damage_high);
    let miss = miss_chance(limb, attacker_brute_burn, target_lying);
    (damage > 0 && !rng.chance(miss)).then_some(damage)
}

/// Melee runs in this set, after `crate::security` took the clicks it handles.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MeleeSystems;

pub struct CombatServerPlugin;

impl Plugin for CombatServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CombatRng>().add_systems(
            Update,
            (toggle_combat_mode, melee)
                .chain()
                .in_set(MeleeSystems)
                .after(ResolveClicks)
                .before(HealthSystems),
        );
    }
}

/// tg `set_combat_mode(!combat_mode)` from the client's living mob.
fn toggle_combat_mode(
    mut intents: MessageReader<IntentReceived>,
    players: Res<Players>,
    mut mobs: Query<(&PlayerMob, &mut CombatMode)>,
) {
    for intent in intents.read() {
        if !matches!(intent.intent, ClientIntent::ToggleCombatMode) {
            continue;
        }
        let Some(mob) = players.mob(intent.client) else {
            continue;
        };
        if let Ok((player, mut mode)) = mobs.get_mut(mob)
            && player.client == intent.client
        {
            mode.0 = !mode.0;
        }
    }
}

/// One side of a tg `visible_message`: what the actor, the target and everyone else in range see.
struct Seen {
    range: u32,
    channel: ChatChannel,
    span: Span,
    actor: String,
    target: String,
    others: String,
}

type Mob<'a> = (
    &'a mut Health,
    &'a mut Stat,
    &'a BodyPosition,
    &'a CombatMode,
    &'a mut NextMove,
    Option<&'a Inventory>,
    Option<&'a mut Facing>,
    Option<&'a ZoneSelected>,
    Option<&'a crate::medical::wounds::Limbs>,
);

/// What a held item hits with.
struct Weapon {
    entity: Entity,
    force: f32,
    kind: DamageType,
    name: String,
    wounding: DmWounding,
}

fn melee(
    mut clicks: MessageReader<ClickResolved>,
    time: Res<Time>,
    mut rng: ResMut<CombatRng>,
    mut mobs: Query<Mob>,
    items: Query<(&DmItem, Option<&DmName>, Option<&DmWounding>)>,
    names: Query<&Name>,
    players: Query<&PlayerMob>,
    index: Res<SpatialIndex>,
    senses: ss13_chat::server::Senses,
    mut chat: MessageWriter<SendChat>,
    handled: Res<crate::security::MeleeHandled>,
    medical: Res<crate::medical::MedicalHandled>,
    clothing: Query<(&ss13_mapload::DmClothing, &ss13_mapload::DmArmor)>,
    (mut sounds, pacifists, artists): (
        MessageWriter<PlaySound>,
        Query<(), With<crate::medical::traumas::Pacifist>>,
        Query<&crate::martial::MartialArts>,
    ),
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for click in clicks.read() {
        let (attacker, target) = (click.mob, click.target);
        // A baton's stun, a flash, cuffs, a shove (`crate::security`).
        // Wound treatments, surgery, drapes, paddles (`crate::medical`).
        if handled.0.contains(&attacker) || medical.0.contains(&attacker) {
            continue;
        }
        // Only living mobs are attacked here; everything else is someone else's click.
        let Ok(lying) = mobs.get(target).map(|m| m.2.lying()) else {
            continue;
        };
        let Ok((own, stat, _, &combat, mut next_move, inventory, facing, zone, _)) =
            mobs.get_mut(attacker)
        else {
            continue;
        };
        let zone = zone.copied().unwrap_or_default().0.check_zone();
        let stat = *stat;
        // tg `ClickOn`: `INCAPACITATED` returns, then `next_move` swallows the click.
        if stat != Stat::Conscious || now < next_move.0 {
            continue;
        }
        let own_brute_burn = own.brute + own.burn;
        if let (Some(mut facing), Some(dir)) = (facing, dir_to(click.mob_pos, click.target_pos)) {
            // tg `face_atom`.
            facing.set_if_neq(Facing(dir));
        }
        let held_entity = inventory.and_then(Inventory::active_item);
        let held = held_entity.and_then(|item| items.get(item).ok());
        // tg `interact_with_atom` of medical items, health analyzers and reagent containers
        // succeeds, so they never attack (`crate::medical`, `crate::chem` handle that click).
        if held.is_some_and(|(item, _, _)| {
            crate::medical::interacts_with_mobs(&item.path)
                || crate::chem::interacts_with_mobs(&item.path)
                || crate::forensics::server::interacts_with_mobs(&item.path)
                || crate::antag::syndie_items::interacts_with_mobs(&item.path)
                || crate::silicon::aicard::interacts_with_mobs(&item.path)
                || crate::service::barber::interacts_with_mobs(&item.path)
                // Food is eaten out of combat mode (`crate::food`), hits in it.
                || (crate::food::is_food(&item.path) && !combat.0)
        }) {
            continue;
        }
        let weapon = held.map(|(item, name, wounding)| {
            // tg `/mob/living/attackby`: the item's `attack_speed`.
            next_move.0 = now + f64::from(item.attack_speed) / 10.0;
            Weapon {
                entity: held_entity.expect("held"),
                force: item.force,
                kind: DamageType::from_dm(&item.damtype).unwrap_or(DamageType::Brute),
                name: name.map_or_else(|| "предмет".to_owned(), |n| n.name.clone()),
                wounding: wounding.cloned().unwrap_or_default(),
            }
        });
        if weapon.is_none() {
            next_move.0 = now + CLICK_CD_MELEE;
        }
        let name = |e: Entity| names.get(e).map_or("Кто-то", |n| n.as_str()).to_owned();
        let (user, victim) = (name(attacker), name(target));
        // tg `TRAIT_PACIFISM`: `attack` with a damaging item (not stamina), `harm` unarmed.
        if pacifists.contains(attacker) {
            let refusal = match &weapon {
                Some(w) if w.force > 0.0 && w.kind != DamageType::Stamina => {
                    Some("Вы не хотите причинять вред другим живым существам!".to_owned())
                }
                None if combat.0 => Some(format!("Вы не хотите причинить вред {victim}!")),
                _ => None,
            };
            if let Some(text) = refusal {
                chat.write(SendChat {
                    clients: vec![click.client],
                    message: ChatMessage::simple(ChatChannel::Warning, Span::Warning, text),
                });
                continue;
            }
        }
        // tg carbon `attack_hand`: skin-contact diseases pass both ways.
        if weapon.is_none() && attacker != target {
            commands.queue(move |world: &mut World| {
                crate::medical::diseases::touch(world, attacker, target);
            });
        }
        let Ok((mut health, mut target_stat, _, _, _, worn, _, _, limbs)) = mobs.get_mut(target)
        else {
            continue;
        };
        // tg `run_armor_check(zone, MELEE, absorb, soften)`: the worn armor and the victim's line.
        let mut armor_line = None;
        let mut armor_check = |zone: BodyZone, absorb: Option<String>, soften: Option<String>| {
            let value = worn.map_or(0.0, |inv| {
                crate::armor::worn_value(
                    crate::armor::worn_pieces(inv, |i| clothing.get(i).ok(), "melee"),
                    crate::chem::body_part_bit(zone),
                )
            });
            // shortcut: items carry no `armour_penetration` yet.
            let (armor, line) = crate::armor::run_armor_check(value, 0.0, false, absorb, soften);
            armor_line = line;
            armor
        };
        // tg `wound_damage_multiplier` of the hit bodypart (burn wounds).
        let multiplier = |zone: BodyZone| limbs.map_or(1.0, |l| l.limb(zone).damage_multiplier());
        let mut wound_hit: Option<(BodyZone, crate::medical::wounds::Hit)> = None;
        // Written back only on a change: `Changed<Stat>` readers act on stat transitions.
        let mut new_stat = *target_stat;
        // tg item `attack` and unarmed `harm` lunge at the target, hit or miss.
        if weapon.is_some() || combat.0 {
            commands.queue(move |world: &mut World| {
                crate::animation::do_attack_animation(world, attacker, target);
            });
        }
        let seen = match weapon {
            // tg `attack` -> `attacked_by`: no message and no damage for a forceless item
            // without attack verbs.
            Some(weapon) if weapon.force <= 0.0 => {
                let (item, pos) = (weapon.entity, click.mob_pos);
                commands.queue(move |world: &mut World| {
                    item_hit_sound(world, item, pos);
                    crate::forensics::server::add_fingerprint(world, item, attacker, false);
                    crate::botany::unique::on_item_attack(world, attacker, target, item);
                });
                None
            }
            Some(Weapon {
                entity: weapon_entity,
                force,
                kind,
                name,
                wounding,
            }) => {
                // tg `attacked_by`: 80% the selected zone (90% on a lying target), else a random
                // one; none of that on yourself.
                let hit = if attacker == target {
                    zone
                } else {
                    random_valid_zone(&mut rng, zone, if lying { 90.0 } else { 80.0 })
                };
                let before = health.part(hit).damage();
                let zone_text = hit.ru().1;
                let pos = click.mob_pos;
                commands.queue(move |world: &mut World| {
                    item_hit_sound(world, weapon_entity, pos);
                });
                let armor = armor_check(
                    hit,
                    Some(format!("Ваша броня защитила вашу {zone_text}!")),
                    Some(format!(
                        "Ваша броня смягчила удар, нанесённый в вашу {zone_text}!"
                    )),
                );
                let force = crate::armor::blocked_damage(force * multiplier(hit), armor);
                apply_damage_zone(&mut health, &mut new_stat, hit, kind, force);
                // tg `attack_effects` and the attack's `add_fingerprint(user)`.
                let brute = kind == DamageType::Brute;
                commands.queue(move |world: &mut World| {
                    crate::forensics::server::item_attack(
                        world,
                        attacker,
                        target,
                        weapon_entity,
                        force,
                        brute,
                    );
                    crate::botany::unique::on_item_attack(world, attacker, target, weapon_entity);
                });
                // tg `attacked_by`, `BODY_ZONE_HEAD`: blunt brute force may deconvert a rev.
                if hit == BodyZone::Head && kind == DamageType::Brute && wounding.sharpness == 0 {
                    commands.queue(move |world: &mut World| {
                        crate::antag::revolution::server::head_trauma(
                            world, attacker, target, force,
                        );
                    });
                }
                wound_hit = Some((
                    hit,
                    crate::medical::wounds::Hit {
                        kind,
                        damage: force,
                        sharpness: wounding.sharpness,
                        wound_bonus: wounding.wound_bonus,
                        exposed_wound_bonus: wounding.exposed_wound_bonus,
                        part_damage_before: before,
                    },
                ));
                // shortcut: tg picks the item's `attack_verb_continuous` and names the hit zone;
                // without them it says "атакует", as here. Add both with bodyparts.
                Some(Seen {
                    range: COMBAT_MESSAGE_RANGE,
                    channel: ChatChannel::Combat,
                    span: Span::Danger,
                    actor: format!("Вы атакуете {victim} с помощью {name}!"),
                    target: format!("{user} атакует вас с помощью {name}!"),
                    others: format!("{user} атакует {victim} с помощью {name}!"),
                })
            }
            None if combat.0 => {
                let limb = attacking_limb(lying);
                let verb = limb.verb;
                // tg `harm`: `get_random_valid_zone(user.zone_selected)`.
                // shortcut: punching yourself can hit the punching arm (tg blacklists it).
                let hit = random_valid_zone(&mut rng, zone, 80.0);
                // tg `TRAIT_PERFECT_ATTACKER` (the sleeping carp): never a miss.
                let strike = if crate::martial::perfect_attacker(artists.get(attacker).ok()) {
                    Some(rng.range(limb.damage_low, limb.damage_high))
                } else {
                    unarmed_strike(&mut rng, limb, own_brute_burn, lying)
                };
                // tg `harm`: the limb's `unarmed_attack_sound` / `unarmed_miss_sound`.
                let file = if strike.is_some() {
                    files::PUNCH
                } else {
                    files::PUNCH_MISS
                };
                sounds.write(PlaySound::new(click.target_pos, file, 25.0, true).extrarange(-1.0));
                match strike {
                    Some(damage) => {
                        let before = health.part(hit).damage();
                        let armor = armor_check(hit, None, None);
                        let damage =
                            crate::armor::blocked_damage(damage as f32 * multiplier(hit), armor);
                        apply_damage_zone(
                            &mut health,
                            &mut new_stat,
                            hit,
                            DamageType::Brute,
                            damage,
                        );
                        wound_hit = Some((
                            hit,
                            crate::medical::wounds::Hit {
                                kind: DamageType::Brute,
                                damage,
                                sharpness: 0,
                                wound_bonus: 0.0,
                                exposed_wound_bonus: 0.0,
                                part_damage_before: before,
                            },
                        ));
                        Some(Seen {
                            range: COMBAT_MESSAGE_RANGE,
                            channel: ChatChannel::Combat,
                            span: Span::Danger,
                            actor: format!("Вы {verb}е {victim}!"),
                            target: format!("{user} {verb} вас!"),
                            others: format!("{user} {verb} {victim}!"),
                        })
                    }
                    None => Some(Seen {
                        range: COMBAT_MESSAGE_RANGE,
                        channel: ChatChannel::Combat,
                        span: Span::Warning,
                        actor: format!("Вы {verb}е и промахиваетесь по {victim}!"),
                        target: format!("{user} {verb} и промахивается по вам!"),
                        others: format!("{user} {verb} и промахивается по {victim}!"),
                    }),
                }
            }
            // shortcut: tg `check_self_for_injuries` lists the own wounds; nothing yet.
            None if attacker == target => None,
            // tg `help`: CPR on a lying target that is dead or not stable.
            None if lying && *target_stat != Stat::Conscious => {
                let (client, rescuer) = (click.client, attacker);
                commands.queue(move |world: &mut World| {
                    crate::medical::do_cpr(world, client, rescuer, target);
                });
                None
            }
            None if lying => {
                // `help_shake_act`: `adjust_status_effects_on_shake_up`.
                commands.queue(move |world: &mut World| {
                    crate::status_effects::shake_up(world, target);
                });
                Some(Seen {
                    range: DEFAULT_MESSAGE_RANGE,
                    channel: ChatChannel::Info,
                    span: Span::Notice,
                    actor: format!("Вы трясете {victim}, пытаясь его поднять!"),
                    target: format!("{user} трясет вас, чтобы вас поднять!"),
                    others: format!("{user} трясет {victim}, пытаясь его поднять!"),
                })
            }
            // tg default `zone_selected` is the chest: a hug. `ru_p_them` of the male body.
            None => {
                let helper = attacker;
                commands.queue(move |world: &mut World| {
                    crate::medical::fear::on_hug(world, helper, target);
                    crate::mood::on_hug(world, helper, target);
                    crate::status_effects::shake_up(world, target);
                });
                Some(Seen {
                    range: DEFAULT_MESSAGE_RANGE,
                    channel: ChatChannel::Info,
                    span: Span::Notice,
                    actor: format!("Вы обнимаете {victim}, чтобы поднять его настроение!"),
                    target: format!("{user} обнимает вас, чтобы поднять ваше настроение!"),
                    others: format!("{user} обнимает {victim}, чтобы поднять его настроение!"),
                })
            }
        };
        target_stat.set_if_neq(new_stat);
        if let Some((zone, hit)) = wound_hit {
            commands.queue(move |world: &mut World| {
                crate::medical::wounds::after_hit(world, target, zone, hit);
            });
        }
        let Some(seen) = seen else {
            continue;
        };
        let message = |text: String, span| ChatMessage {
            channel: seen.channel,
            segments: vec![ChatSegment::new(span, text)],
        };
        let client = |e: Entity| players.get(e).ok().map(|p| p.client);
        let others = clients_in_view(
            &index,
            &players,
            &senses,
            click.target_pos,
            seen.range,
            &[attacker, target],
        );
        chat.write(SendChat {
            clients: others,
            message: message(seen.others, seen.span),
        });
        if attacker != target
            && let Some(client) = client(target)
        {
            // tg `span_userdanger` for the victim of an attack.
            let span = if seen.span == Span::Notice {
                Span::Notice
            } else {
                Span::BoldWarning
            };
            chat.write(SendChat {
                clients: vec![client],
                message: message(seen.target, span),
            });
        }
        if let Some(client) = client(attacker) {
            chat.write(SendChat {
                clients: vec![client],
                message: message(seen.actor, seen.span),
            });
        }
        if let (Some((span, text)), Some(client)) = (armor_line, client(target)) {
            chat.write(SendChat {
                clients: vec![client],
                message: message(text, span),
            });
        }
    }
}

/// The direction from `from` to an adjacent (or same) tile.
/// tg `/obj/item/proc/attack`: a forceless item taps, another plays its `hitsound`, at
/// `get_clamped_volume()`.
fn item_hit_sound(world: &mut World, item: Entity, at: TilePos) {
    let Some(dm) = world.get::<DmItem>(item) else {
        return;
    };
    let w_class = world
        .get::<ss13_mapload::DmWeight>(item)
        .copied()
        .unwrap_or_default()
        .w_class;
    let volume = crate::sound::clamped_volume(dm.force, w_class);
    let file = if dm.force <= 0.0 {
        files::TAP
    } else {
        crate::sound::hitsound(dm)
    };
    let sound = PlaySound::new(at, file, volume, true).extrarange(-1.0);
    crate::sound::playsound(world, sound);
}

fn dir_to(from: TilePos, to: TilePos) -> Option<Dir> {
    let (dx, dy) = ((to.x - from.x).signum(), (to.y - from.y).signum());
    Dir::DMI_ORDER.into_iter().find(|d| d.offset() == (dx, dy))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn miss_chance_follows_tg_harm() {
        assert_eq!(miss_chance(ARM, 0.0, false), 10.0);
        assert_eq!(miss_chance(ARM, 40.0, false), 30.0);
        assert_eq!(miss_chance(ARM, 400.0, false), 80.0);
        assert_eq!(miss_chance(LEG, 0.0, false), 5.0);
        assert_eq!(miss_chance(ARM, 400.0, true), 0.0);
        assert_eq!(attacking_limb(true), LEG);
        assert_eq!(attacking_limb(false), ARM);
    }

    #[test]
    fn strikes_roll_tg_ranges() {
        let mut rng = CombatRng(7);
        let (mut hits, mut misses) = (0u32, 0u32);
        for _ in 0..10_000 {
            match unarmed_strike(&mut rng, ARM, 0.0, false) {
                Some(d) => {
                    assert!((5..=10).contains(&d));
                    hits += 1;
                }
                None => misses += 1,
            }
        }
        // 10% misses.
        assert!(
            (800..1200).contains(&misses),
            "{misses} misses, {hits} hits"
        );
        for _ in 0..1000 {
            let d = unarmed_strike(&mut rng, LEG, 300.0, true).expect("kicks never miss");
            assert!((7..=15).contains(&d));
        }
        let rolls: std::collections::HashSet<u32> = (0..1000).map(|_| rng.range(5, 10)).collect();
        assert_eq!(rolls.len(), 6, "both ends are rolled");
    }

    #[test]
    fn directions_to_neighbours() {
        let at = TilePos::new(5, 5, 1);
        assert_eq!(dir_to(at, TilePos::new(6, 6, 1)), Some(Dir::NorthEast));
        assert_eq!(dir_to(at, TilePos::new(5, 4, 1)), Some(Dir::South));
        assert_eq!(dir_to(at, at), None);
    }
}
