//! Tilting vendors (server), after tg `code/modules/vending/vendor/tilting.dm` (`tilt`,
//! `fall_and_crush`, the vendor crit cases, `untilt`) and `interaction.dm` (`attacked_by`,
//! `freebie`, `interact`).
//!
//! - A hit that damages a tiltable, upright vendor rolls `rand(1, 100)`: 1-5 three free
//!   products fall out, 6-15 two, 16-25 one, 76-100 it falls onto the attacker.
//! - Falling (`fall_and_crush`, only next to the target): unless the tile is blocked by
//!   something dense other than mobs, everything on it is crushed with `squish_damage` (75)
//!   brute: a carbon takes it spread over the body (30%) or as two half hits on random limbs
//!   with a wound bonus, through melee armor; objects with integrity take it as melee damage.
//!   Living things are paralyzed 6 s. With `crit_chance` (15%) per thing a vendor crit: glass
//!   candy (a third of the damage, shards) or a pin (the vendor holds the mob down until it is
//!   righted). The vendor ends on the tile, turned 90 or 270 degrees, above mobs.
//! - An empty hand rights it in 5 s (`untilt`), freeing pinned mobs; a pinned mob cannot.
// shortcut: glass candy drops its 7 shards on the floor (no embedding yet); no thrown mobs
// (`hitby`), tackles or ranged tilts (malf AI, fishing); no `post_crush_living` achievement
// or memories.

use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Appearance, Layer, TilePos};
use ss13_world::{Mover, SpatialIndex};

use super::{RecordKind, Vendor, dispense, name_of};
use crate::combat::CombatRng;
use crate::health::{DEFAULT_MESSAGE_RANGE, DamageType, Health, Stat, update_stat};
use crate::obj_damage::{Integrity, is_under};
use crate::status_effects::{Incap, StatusEffects};
use crate::tools::Actor;
use crate::zone::BodyZone;

/// `paralyze_time` of a tilt, seconds.
const PARALYZE_SECS: f32 = 6.0;
/// `ABOVE_MOB_LAYER`.
const ABOVE_MOB_LAYER: f32 = 4.1;
/// `interact`: righting takes 5 seconds (deciseconds).
const RIGHT_DS: f32 = 50.0;
/// `VENDOR_CRUSH_CRIT_GLASSCANDY`'s damage multiplier (`fall_and_crush_crit_rebate_table`).
const GLASS_CANDY_REBATE: f32 = 0.33;
/// `take_bodypart_damage(..., wound_bonus = 5)`.
const CRUSH_WOUND_BONUS: f32 = 5.0;
const SHARD: &str = "/obj/item/shard";
const BANG: &str = "sound/effects/bang.ogg";
const SPLAT: &str = "sound/effects/splat.ogg";
const BLOB_ATTACK: &str = "sound/effects/blob/blobattack.ogg";
/// `vendor_uncrushable_objects`: chairs, conveyors and wall mounts (here: what hangs on walls
/// is not on the floor tile's dense contents anyway).
const UNCRUSHABLE: [&str; 2] = ["/obj/structure/chair", "/obj/machinery/conveyor"];

/// On a tilted vendor: the rotation it fell with and the layer it had.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Tilted {
    pub rotation: u16,
    layer: f32,
}

/// On a mob a vendor pinned (`VENDOR_CRUSH_CRIT_PIN`, tg buckles it to the vendor).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinnedUnder(pub Entity);

/// The vendor crit cases (`get_crit_crush_chances`, equal weights).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Crit {
    GlassCandy,
    Pin,
}

pub fn is_tilted(world: &World, vendor: Entity) -> bool {
    world.get::<Tilted>(vendor).is_some()
}

/// tg `/obj/machinery/vending/attacked_by` after a hit that did damage.
pub fn attacked(world: &mut World, attacker: Entity, vendor: Entity) {
    let Some(machine) = world.get::<Vendor>(vendor) else {
        return;
    };
    if !machine.tiltable || is_tilted(world, vendor) {
        return;
    }
    let crit = machine.crit_chance;
    let roll = world.resource_mut::<CombatRng>().range(1, 100);
    match roll {
        1..=5 => freebie(world, vendor, 3),
        6..=15 => freebie(world, vendor, 2),
        16..=25 => freebie(world, vendor, 1),
        76..=100 => {
            tilt(world, vendor, attacker, crit, None);
        }
        _ => {}
    }
}

/// tg `freebie`: free products onto the vendor's tile (new stock before returned items).
pub fn freebie(world: &mut World, vendor: Entity, count: u32) {
    let Some(&pos) = world.get::<TilePos>(vendor) else {
        return;
    };
    let name = name_of(world, vendor);
    let text = if count > 1 {
        format!("{name} выдаёт несколько бесплатных товаров!")
    } else {
        format!("{name} выдаёт бесплатный товар!")
    };
    crate::security::show(
        world,
        pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Info,
        (Span::Notice, text),
        &[],
    );
    for _ in 0..count {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, super::VEND_SOUND, 50.0, true).extrarange(-3.0),
        );
        let Some(machine) = world.get::<Vendor>(vendor) else {
            return;
        };
        let mut order: Vec<usize> = (0..machine.records.len())
            .filter(|&i| {
                let r = &machine.records[i];
                r.kind == RecordKind::Product && r.amount > 0
            })
            .collect();
        super::shuffle(world, &mut order);
        let Some(&i) = order.first() else {
            continue;
        };
        let record = &world.get::<Vendor>(vendor).expect("checked").records[i];
        let only_returned = record.amount as usize <= record.returned.len();
        if let Some(item) = dispense(world, vendor, i, pos, true, only_returned) {
            world
                .entity_mut(item)
                .insert((ss13_net::server::Networked, pos));
        }
    }
}

/// tg `tilt(fatty)`: falls onto `target`'s tile if next to it. True when it fell over.
pub fn tilt(
    world: &mut World,
    vendor: Entity,
    target: Entity,
    crit_chance: f32,
    forced: Option<Crit>,
) -> bool {
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(vendor), world.get::<TilePos>(target))
    else {
        return false;
    };
    if from.chebyshev(to).is_none_or(|d| d > 1) {
        return false;
    }
    let rotation = if world.resource_mut::<CombatRng>().chance(50.0) {
        90
    } else {
        270
    };
    let damage = world.get::<Vendor>(vendor).map_or(0.0, |v| v.squish_damage);
    if !fall_and_crush(world, vendor, to, damage, crit_chance, forced, rotation) {
        return false;
    }
    let layer = world.get::<Appearance>(vendor).map_or(2.9, |a| a.layer.0);
    if let Some(mut appearance) = world.get_mut::<Appearance>(vendor) {
        appearance.layer = Layer(ABOVE_MOB_LAYER);
    }
    world.entity_mut(vendor).insert(Tilted { rotation, layer });
    let text = format!("{} опрокидывается!", name_of(world, vendor));
    show(world, to, Span::Danger, text, &[]);
    true
}

fn show(
    world: &mut World,
    at: TilePos,
    span: Span,
    text: String,
    personal: &[(Entity, Span, String)],
) {
    crate::security::show(
        world,
        at,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (span, text),
        personal,
    );
}

/// `is_blocked_turf(TRUE, src, list(src))`: a dense turf or a dense non-mob thing other than
/// the vendor.
fn blocked(world: &World, pos: TilePos, vendor: Entity) -> bool {
    world
        .resource::<SpatialIndex>()
        .dense_on_tile(pos)
        .iter()
        .any(|&e| e != vendor && world.get::<Mover>(e).is_none())
}

/// tg `check_atom_crushable`: something with integrity, not a chair or conveyor.
fn crushable(world: &World, e: Entity) -> bool {
    let Some(obj) = world.get::<DmObj>(e) else {
        return false;
    };
    world.get::<Integrity>(e).is_some() && !UNCRUSHABLE.iter().any(|r| is_under(&obj.path, r))
}

/// tg `fall_and_crush` of a vendor onto `target`. True when it fell over (`SUCCESSFULLY_FELL_OVER`).
pub fn fall_and_crush(
    world: &mut World,
    vendor: Entity,
    target: TilePos,
    damage: f32,
    crit_chance: f32,
    forced: Option<Crit>,
    rotation: u16,
) -> bool {
    let name = name_of(world, vendor);
    if blocked(world, target, vendor) {
        let text = format!("{name} комично отскакивает, не сумев упасть!");
        show(world, target, Span::Danger, text, &[]);
        return false;
    }
    let on_tile: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(target).to_vec();
    for e in on_tile {
        if e == vendor {
            continue;
        }
        let living = world.get::<Health>(e).is_some();
        if !living && !crushable(world, e) {
            continue;
        }
        let crit = forced.or_else(|| {
            let mut rng = world.resource_mut::<CombatRng>();
            (crit_chance > 0.0 && rng.chance(crit_chance)).then(|| {
                if rng.chance(50.0) {
                    Crit::GlassCandy
                } else {
                    Crit::Pin
                }
            })
        });
        let mut rebate = 1.0;
        if let Some(crit) = crit
            && apply_crit(world, vendor, e, crit, target)
            && crit == Crit::GlassCandy
        {
            rebate = GLASS_CANDY_REBATE;
        }
        let adjusted = damage * rebate;
        if living {
            crush_living(world, vendor, e, adjusted);
        } else {
            crate::obj_damage::take_damage(world, e, adjusted, DamageType::Brute);
        }
        if world.get_entity(e).is_ok() {
            let victim = name_of(world, e);
            show(
                world,
                target,
                Span::Danger,
                format!("{victim} раздавлен {name}!"),
                &[(e, Span::BoldWarning, format!("Вас раздавил {name}!"))],
            );
        }
    }
    if let Some(mut appearance) = world.get_mut::<Appearance>(vendor) {
        appearance.rotation = rotation;
    }
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(target, BANG, 40.0, false),
    );
    show(
        world,
        target,
        Span::Danger,
        format!("{name} опрокидывается, тяжело обрушиваясь вниз!"),
        &[],
    );
    // `Move(target, crush_dir)`: the crushed are lying now, so it lands on their tile.
    let from = world.get::<TilePos>(vendor).copied();
    world.entity_mut(vendor).insert(target);
    if let Some(from) = from {
        crate::construction::tile_changed(world, from);
    }
    crate::construction::tile_changed(world, target);
    true
}

/// tg `apply_crit_crush` of a vendor. True when the crit applied (carbon or living).
fn apply_crit(world: &mut World, vendor: Entity, e: Entity, crit: Crit, at: TilePos) -> bool {
    let Some(health) = world.get::<Health>(e) else {
        return false;
    };
    match crit {
        Crit::GlassCandy => {
            if health.basic.is_some() {
                return false;
            }
            for _ in 0..7 {
                crate::construction::spawn_obj(world, SHARD, at);
            }
            true
        }
        Crit::Pin => {
            world.entity_mut(e).insert(PinnedUnder(vendor));
            let (victim, name) = (name_of(world, e), name_of(world, vendor));
            show(
                world,
                at,
                Span::Danger,
                format!("{victim} прижат к полу {name}!"),
                &[(e, Span::BoldWarning, format!("Вас прижал к полу {name}!"))],
            );
            true
        }
    }
}

/// tg `run_armor_check(attack_flag = MELEE)` without a zone: the body's average melee armor.
fn body_armor(world: &World, mob: Entity) -> f32 {
    BodyZone::ALL
        .iter()
        .map(|&z| crate::armor::getarmor(world, mob, z, "melee"))
        .sum::<f32>()
        / BodyZone::ALL.len() as f32
}

/// The living half of `fall_and_crush`: the damage, paralysis, scream and sounds.
fn crush_living(world: &mut World, vendor: Entity, mob: Entity, damage: f32) {
    let _ = vendor;
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let carbon = world.get::<Health>(mob).is_some_and(|h| h.basic.is_none());
    let mut hits = Vec::new();
    if carbon && !world.resource_mut::<CombatRng>().chance(30.0) {
        // Two half hits on random limbs, each through that limb's armor, wounding.
        for _ in 0..2 {
            let zones = BodyZone::ALL;
            let zone = zones[world
                .resource_mut::<CombatRng>()
                .range(0, zones.len() as u32 - 1) as usize];
            let armor = crate::armor::getarmor(world, mob, zone, "melee");
            hits.push((
                Some(zone),
                crate::armor::blocked_damage(damage * 0.5, armor),
            ));
        }
    } else {
        // 30%: spread over the body (no wounds).
        let armor = body_armor(world, mob);
        hits.push((None, crate::armor::blocked_damage(damage.max(0.0), armor)));
    }
    let mut wounds = Vec::new();
    if let (Some(mut health), Some(mut stat)) = (
        world.get::<Health>(mob).cloned(),
        world.get::<Stat>(mob).copied(),
    ) {
        for (zone, amount) in hits {
            match zone {
                Some(zone) => {
                    let before = health.part(zone).damage();
                    health.damage_zone(zone, DamageType::Brute, amount);
                    wounds.push((
                        zone,
                        crate::medical::wounds::Hit {
                            kind: DamageType::Brute,
                            damage: amount,
                            sharpness: 0,
                            wound_bonus: CRUSH_WOUND_BONUS,
                            exposed_wound_bonus: 0.0,
                            part_damage_before: before,
                        },
                    ));
                }
                None => health.adjust(DamageType::Brute, amount),
            }
        }
        let changed = update_stat(&health, &mut stat).is_some();
        world.entity_mut(mob).insert(health);
        if changed {
            world.entity_mut(mob).insert(stat);
        }
    }
    for (zone, hit) in wounds {
        crate::medical::wounds::after_hit(world, mob, zone, hit);
    }
    crate::status_effects::apply(world, mob, Incap::Paralyze, PARALYZE_SECS);
    crate::mobs::defense::emote(world, mob, "кричит!");
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, BLOB_ATTACK, 40.0, true),
    );
    crate::sound::playsound(world, crate::sound::PlaySound::new(pos, SPLAT, 50.0, true));
}

/// A pinned mob stays down while its vendor lies on it (tg: buckled, `unbuckle_mob` refuses
/// without force); it is free once the vendor is righted or gone from its tile.
pub(super) fn hold_pinned(
    mut mobs: Query<(Entity, &PinnedUnder, &TilePos, Option<&mut StatusEffects>)>,
    vendors: Query<&TilePos, With<Tilted>>,
    mut commands: Commands,
) {
    for (mob, pin, pos, effects) in &mut mobs {
        if vendors.get(pin.0).is_ok_and(|v| v == pos) {
            if let Some(mut effects) = effects
                && effects.remaining(Incap::Immobilize) < 0.5
            {
                effects.set(Incap::Immobilize, 0.5);
            }
        } else {
            commands.entity(mob).remove::<PinnedUnder>();
        }
    }
}

/// tg `untilt`: upright again, pinned mobs free.
pub fn untilt(world: &mut World, vendor: Entity, user: Option<Entity>) {
    let Some(tilted) = world.get::<Tilted>(vendor).copied() else {
        return;
    };
    if let Some(user) = user
        && let Some(&pos) = world.get::<TilePos>(vendor)
    {
        let (who, name) = (name_of(world, user), name_of(world, vendor));
        crate::security::show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            (Span::Notice, format!("{who} поднимает {name}.")),
            &[(user, Span::Notice, format!("Вы поднимаете {name}."))],
        );
    }
    let pinned: Vec<Entity> = world
        .query::<(Entity, &PinnedUnder)>()
        .iter(world)
        .filter(|(_, p)| p.0 == vendor)
        .map(|(e, _)| e)
        .collect();
    for mob in pinned {
        world.entity_mut(mob).remove::<PinnedUnder>();
        if let Some(mut effects) = crate::status_effects::effects_mut(world, mob) {
            effects.set(Incap::Immobilize, 0.0);
        }
    }
    world.entity_mut(vendor).remove::<Tilted>();
    if let Some(mut appearance) = world.get_mut::<Appearance>(vendor) {
        appearance.rotation = 0;
        appearance.layer = Layer(tilted.layer);
    }
}

/// tg `interact` of a tilted vendor: "You begin righting", a 5 s `do_after`.
pub(super) fn start_righting(world: &mut World, client: Entity, mob: Entity, vendor: Entity) {
    if world.get::<PinnedUnder>(mob).is_some() {
        return;
    }
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    if crate::tools::do_after(
        world,
        actor,
        vendor,
        RIGHT_DS,
        0,
        crate::construction::Job::Vending(super::VendJob::Right),
    ) {
        let text = format!("Вы начинаете поднимать {}.", name_of(world, vendor));
        crate::tools::tell(world, client, Span::Notice, text);
    }
}

pub(super) fn finish_righting(world: &mut World, mob: Entity, action: &crate::tools::DoAfter) {
    if is_tilted(world, action.target) {
        untilt(world, action.target, Some(mob));
    }
}
