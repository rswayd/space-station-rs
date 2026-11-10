//! Leg restraints (server), after tg `/obj/item/restraints/legcuffs`
//! (`code/game/objects/items/handcuffs.dm`) and `carbon/inventory.dm` (`ITEM_SLOT_LEGCUFFED`).
//!
//! - [`Legcuffed`]: the item on the mob's legs; it slows by its `slowdown`
//!   (`crate::clothing::slowdown`) and shows tg's `legcuff` overlay. It comes off by resisting
//!   (`crate::restraints`, `breakouttime`), the strip menu, the freedom implant, or with a leg.
//! - Bear traps: use in hand arms and disarms one; an armed trap snaps on the living mob that
//!   steps on it ([`spring_trap`]): not on tiny mobs or floaters, a standing carbon takes it on
//!   a leg (legcuffed with two legs and nothing on them yet), anything else in the chest.
//!   Energy snares fade after 10 s when nobody wears them, vanish when taken off and spring on
//!   the hand that touches them on the floor.
//! - Bolas ([`thrown`], `crate::throw`): one that is not caught wraps the legs of the carbon it
//!   hits; the tactical one also knocks down, the energy one is uncatchable and snares with an
//!   energy snare, the gonbola pacifies and mutes while it is worn.
// shortcut: no clumsy fumbles (`attempt_pickup`, no clumsiness in the port), no `on_found`, no
// `TRAIT_UNCATCHABLE` outside the energy bola, no flying mobs (only weightless ones count as
// not touching the ground, the port has no movement types); the cult bola's pickup backfire
// waits for the cult group to hand one out.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmLegcuffs, IconIndex};
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{IconStateId, TilePos};
use ss13_world::{Moved, SpatialIndex};

use crate::combat::CombatRng;
use crate::health::{BodyPosition, DEFAULT_MESSAGE_RANGE, DamageType, Health, Stat};
use crate::inventory::{CarriedBy, Inventory};
use crate::medical::wounds::{Hit, Limbs};
use crate::obj_damage::set_icon_state;
use crate::security::{name_of, show};
use crate::sound::{PlaySound, playsound};
use crate::status_effects::{Incap, Incapacitation};
use crate::tools::{tell, tell_mob};
use crate::zone::BodyZone;

pub const BEARTRAP: &str = "/obj/item/restraints/legcuffs/beartrap";
pub const ENERGY_SNARE: &str = "/obj/item/restraints/legcuffs/beartrap/energy";
pub const ENERGY_SNARE_CYBORG: &str = "/obj/item/restraints/legcuffs/beartrap/energy/cyborg";
pub const BOLA: &str = "/obj/item/restraints/legcuffs/bola";
pub const ENERGY_BOLA: &str = "/obj/item/restraints/legcuffs/bola/energy";
pub const CULT_BOLA: &str = "/obj/item/restraints/legcuffs/bola/cult";
pub const GONBOLA: &str = "/obj/item/restraints/legcuffs/bola/gonbola";
/// `/obj/vehicle/ridden/scooter`, the only vehicle with `are_legs_exposed`.
const SCOOTER: &str = "/obj/vehicle/ridden/scooter";

/// `LEGCUFF_LAYER` (`code/__DEFINES/mobs.dm`).
pub const LEGCUFF_LAYER: f32 = 4.0;
/// `update_worn_legcuffs`: `mutable_appearance('icons/mob/simple/mob.dmi', legcuff_state)`.
pub const LEGCUFF_OVERLAY: (&str, &str) = ("icons/mob/simple/mob.dmi", "legcuff");
/// `MOB_SIZE_TINY`.
const MOB_SIZE_TINY: u8 = 0;
/// `/obj/item/restraints/legcuffs/beartrap/energy` fades after this many seconds.
const SNARE_LIFE: f64 = 10.0;
/// `SHARP_POINTY`.
const SHARP_POINTY: u32 = 2;
const GONBOLA_MOOD_CATEGORY: &str = "gonbola";
/// `/datum/mood_event/gondola`.
const GONBOLA_MOOD: crate::mood::MoodEvent = crate::mood::MoodEvent {
    id: "gondola",
    description: "Я чувствую умиротворенность и не испытываю потребности совершать какие-либо резкие или необдуманные поступки.",
    mood_change: 6.0,
    timeout: 0.0,
    hidden: false,
};

const SNAP_SOUND: &str = "sound/effects/snap.ogg";
const CUFFS_SOUND: &str = "sound/items/weapons/handcuffs.ogg";
const THROW_SOUND: &str = "sound/items/weapons/bolathrow.ogg";

/// tg `legcuffed`: the restraint on the mob's legs (it carries [`CarriedBy`] the mob, outside
/// every inventory slot).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Legcuffed(pub Entity);

/// An energy snare fades at this game time unless a mob has it (`dissipate`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
struct DissipateAt(f64);

pub struct LegcuffsPlugin;

impl Plugin for LegcuffsPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::AttackSelf, 3305, |w, c| {
            attack_self(w, c.client, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::EmptyHand, 3575, |w, c| {
            attack_hand(w, c.mob, c.target)
        });
        app.add_systems(
            Update,
            (init_new, stepped_on, dissipate).after(crate::click::ResolveClicks),
        );
    }
}

/// `beartrap/Initialize`: the armed look, and the energy snare's timer.
fn init_new(
    new: Query<(Entity, &DmLegcuffs), Added<DmLegcuffs>>,
    time: Res<Time>,
    mut commands: Commands,
) {
    for (e, cuffs) in &new {
        if is_under(&cuffs.path, BEARTRAP) {
            commands.queue(move |world: &mut World| refresh_trap_look(world, e));
        }
        if is_under(&cuffs.path, ENERGY_SNARE) {
            commands
                .entity(e)
                .try_insert(DissipateAt(time.elapsed_secs_f64() + SNARE_LIFE));
        }
    }
}

/// `update_icon_state`: `"[initial(icon_state)][armed]"`.
fn refresh_trap_look(world: &mut World, trap: Entity) {
    let Some(armed) = world.get::<DmLegcuffs>(trap).map(|c| c.armed) else {
        return;
    };
    let Some(base) = world.get::<DmAtom>(trap).map(|a| a.icon_state.clone()) else {
        return;
    };
    set_icon_state(world, trap, &format!("{base}{}", u8::from(armed)));
}

/// The legcuff overlay sprite of a legcuffed mob, for the mob's look (`crate::inventory`).
pub fn overlay(icons: &IconIndex) -> Option<(f32, IconStateId)> {
    icons
        .get(LEGCUFF_OVERLAY.0, LEGCUFF_OVERLAY.1)
        .map(|id| (LEGCUFF_LAYER, id))
}

/// `num_legs >= 2`.
pub fn two_legs(world: &World, mob: Entity) -> bool {
    world
        .get::<Limbs>(mob)
        .is_none_or(|l| !l.limb(BodyZone::LLeg).missing && !l.limb(BodyZone::RLeg).missing)
}

/// Whether the worn restraint is a gonbola (`TRAIT_PACIFISM`, `TRAIT_MUTE` of
/// `/datum/status_effect/gonbola_pacify`).
pub fn gonbola_worn(world: &World, mob: Entity) -> bool {
    world
        .get::<Legcuffed>(mob)
        .and_then(|l| world.get::<DmLegcuffs>(l.0))
        .is_some_and(|c| is_under(&c.path, GONBOLA))
}

/// `beartrap/attack_self`: arms or disarms it. True when `item` is a bear trap.
fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world
        .get::<DmLegcuffs>(item)
        .is_none_or(|c| !is_under(&c.path, BEARTRAP))
    {
        return false;
    }
    let hands_blocked = world
        .get::<Incapacitation>(mob)
        .is_some_and(|t| t.hands_blocked);
    if !crate::throw::is_carbon(world, mob)
        || world.get::<Stat>(mob) != Some(&Stat::Conscious)
        || hands_blocked
    {
        return true;
    }
    if let Some(pos) = crate::explosions::turf_of(world, item) {
        playsound(
            world,
            PlaySound::new(pos, CUFFS_SOUND, 30.0, true).extrarange(-3.0),
        );
    }
    let Some(mut cuffs) = world.get_mut::<DmLegcuffs>(item) else {
        return true;
    };
    cuffs.armed = !cuffs.armed;
    let armed = cuffs.armed;
    refresh_trap_look(world, item);
    let name = name_of(world, item);
    let state = if armed {
        "взведён"
    } else {
        "разряжен"
    };
    tell(
        world,
        client,
        Span::Notice,
        format!("{name}: теперь {state}."),
    );
    true
}

/// `energy/attack_hand`: the snare on the floor springs on the hand that touches it. True when
/// the click was spent on it.
fn attack_hand(world: &mut World, mob: Entity, item: Entity) -> bool {
    let snare = world
        .get::<DmLegcuffs>(item)
        .is_some_and(|c| is_under(&c.path, ENERGY_SNARE));
    if !snare || world.get::<TilePos>(item).is_none() {
        return false;
    }
    spring_trap(world, item, mob, false, false);
    // Still lying there: picked up as usual.
    world.get::<TilePos>(item).is_none()
}

/// tg `connect_loc` `COMSIG_ATOM_ENTERED` of armed traps, for the steps taken this tick.
// Cost: (steps this tick) x (things on the tile entered), a handful.
fn stepped_on(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moves: Vec<(Entity, TilePos)> = cursor
        .read(world.resource::<Messages<Moved>>())
        .map(|m| (m.entity, m.to))
        .collect();
    for (mover, to) in moves {
        if world.get::<Health>(mover).is_none() {
            continue;
        }
        let traps: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(to)
            .iter()
            .copied()
            .filter(|&e| {
                e != mover
                    && world
                        .get::<DmLegcuffs>(e)
                        .is_some_and(|c| c.armed && is_under(&c.path, BEARTRAP))
            })
            .collect();
        for trap in traps {
            spring_trap(world, trap, mover, false, false);
        }
    }
}

/// `close_trap`.
fn close_trap(world: &mut World, trap: Entity) {
    if let Some(mut cuffs) = world.get_mut::<DmLegcuffs>(trap) {
        cuffs.armed = false;
    }
    refresh_trap_look(world, trap);
    if let Some(pos) = crate::explosions::turf_of(world, trap) {
        playsound(world, PlaySound::new(pos, SNAP_SOUND, 50.0, true));
    }
}

/// tg `spring_trap`: safe to call on anything; `ignore_movetypes` and `hit_prone` as in tg.
pub fn spring_trap(
    world: &mut World,
    trap: Entity,
    victim: Entity,
    ignore_movetypes: bool,
    hit_prone: bool,
) {
    let Some(cuffs) = world.get::<DmLegcuffs>(trap).cloned() else {
        return;
    };
    let (true, Some(&at)) = (
        cuffs.armed && world.get::<Health>(victim).is_some(),
        world.get::<TilePos>(victim),
    ) else {
        return;
    };
    let (who, what) = (name_of(world, victim), name_of(world, trap));
    if let Some(vehicle) = world
        .get::<crate::vehicles::Rider>(victim)
        .map(|r| r.vehicle)
    {
        let exposed =
            crate::construction::path_of(world, vehicle).is_some_and(|p| is_under(&p, SCOOTER));
        if !exposed {
            // The trap shuts on the vehicle, its rider's legs are inside.
            close_trap(world, trap);
            let ride = name_of(world, vehicle);
            show(
                world,
                at,
                DEFAULT_MESSAGE_RANGE,
                ChatChannel::Combat,
                (Span::Danger, format!("{ride} задевает {what}.")),
                &[],
            );
            return;
        }
    }
    if crate::mobs::pets::mob_size(world, victim) == MOB_SIZE_TINY {
        return;
    }
    if !ignore_movetypes && world.get::<crate::spacemove::Weightless>(victim).is_some() {
        return;
    }
    close_trap(world, trap);
    let (others, own) = if ignore_movetypes {
        (
            format!("{what} опутывает {who}!"),
            format!("{what} опутывает вас!"),
        )
    } else {
        (
            format!("{who} задевает {what}."),
            format!("Вы задеваете {what}!"),
        )
    };
    show(
        world,
        at,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (Span::Danger, others),
        &[(victim, Span::BoldWarning, own)],
    );
    let standing = world
        .get::<BodyPosition>(victim)
        .is_some_and(|b| !b.lying());
    let mut zone = BodyZone::Chest;
    if crate::throw::is_carbon(world, victim) && (standing || hit_prone) {
        zone = if world.resource_mut::<CombatRng>().chance(50.0) {
            BodyZone::LLeg
        } else {
            BodyZone::RLeg
        };
        // A second trap or a missing leg: it bites, but does not hold.
        if world.get::<Legcuffed>(victim).is_none() && two_legs(world, victim) {
            equip(world, trap, victim);
        }
    }
    if cuffs.trap_damage > 0.0 {
        bite(world, victim, zone, cuffs.trap_damage);
    }
}

/// `apply_damage(trap_damage, BRUTE, def_zone, sharpness = SHARP_POINTY, wound_bonus =
/// trap_damage / 2, exposed_wound_bonus = trap_damage)`.
fn bite(world: &mut World, victim: Entity, zone: BodyZone, damage: f32) {
    let before = world
        .get::<Health>(victim)
        .map_or(0.0, |h| h.part(zone).damage());
    crate::bots::hurt_zone_raw(world, victim, zone, DamageType::Brute, damage);
    crate::medical::wounds::after_hit(
        world,
        victim,
        zone,
        Hit {
            kind: DamageType::Brute,
            damage,
            sharpness: SHARP_POINTY,
            wound_bonus: damage / 2.0,
            exposed_wound_bonus: damage,
            part_damage_before: before,
        },
    );
}

/// `equip_to_slot(ITEM_SLOT_LEGCUFFED)`.
pub fn equip(world: &mut World, item: Entity, victim: Entity) {
    world
        .entity_mut(item)
        .remove::<(CarriedBy, TilePos, crate::throw::Thrown)>()
        .insert(CarriedBy(victim));
    world.entity_mut(victim).insert(Legcuffed(item));
    // The look gains the overlay.
    if let Some(mut inventory) = world.get_mut::<Inventory>(victim) {
        inventory.set_changed();
    }
    if is_gonbola(world, item) {
        pacify(world, victim);
    }
}

fn is_gonbola(world: &World, item: Entity) -> bool {
    world
        .get::<DmLegcuffs>(item)
        .is_some_and(|c| is_under(&c.path, GONBOLA))
}

/// `gonbola/throw_impact` -> `apply_status_effect(/datum/status_effect/gonbola_pacify)`. (tg
/// applies it to anyone hit, even if the gonbola was caught or could not wrap the legs, and only
/// takes it off when that gonbola drops: a pacifist for good. Here it lasts while it is worn.)
fn pacify(world: &mut World, mob: Entity) {
    crate::medical::traumas::refresh(world, mob);
    crate::mood::add_event(world, mob, GONBOLA_MOOD_CATEGORY, &GONBOLA_MOOD);
    tell_mob(
        world,
        mob,
        Span::Notice,
        "Вы внезапно чувствуете умиротворение и не испытываете потребности совершать какие-либо резкие или необдуманные поступки...",
    );
}

/// tg `dropItemToGround(legcuffed)`: an energy snare (`DROPDEL`) vanishes, the rest fall at `pos`.
pub fn remove(world: &mut World, mob: Entity, pos: TilePos) {
    let Some(Legcuffed(item)) = world.get::<Legcuffed>(mob).copied() else {
        return;
    };
    let gonbola = is_gonbola(world, item);
    let snare = world
        .get::<DmLegcuffs>(item)
        .is_some_and(|c| is_under(&c.path, ENERGY_SNARE));
    world.entity_mut(mob).remove::<Legcuffed>();
    if snare {
        world.entity_mut(item).despawn();
    } else {
        crate::inventory::put_down(world, item, pos);
    }
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.set_changed();
    }
    if gonbola {
        crate::medical::traumas::refresh(world, mob);
        crate::mood::clear_event(world, mob, GONBOLA_MOOD_CATEGORY);
    }
}

/// The energy bola cannot be caught (`TRAIT_UNCATCHABLE`).
pub fn uncatchable(world: &World, item: Entity) -> bool {
    world
        .get::<DmLegcuffs>(item)
        .is_some_and(|c| is_under(&c.path, ENERGY_BOLA))
}

/// `bola/throw_at`: the throw's sound.
pub fn on_thrown(world: &mut World, item: Entity, from: TilePos) {
    let bola = world
        .get::<DmLegcuffs>(item)
        .is_some_and(|c| is_under(&c.path, BOLA));
    if bola {
        playsound(world, PlaySound::new(from, THROW_SOUND, 75.0, true));
    }
}

/// `bola/throw_impact` and `after_throw` of a bola not caught, having hit `hit`.
pub fn thrown(world: &mut World, item: Entity, hit: Entity) {
    let Some(path) = world.get::<DmLegcuffs>(item).map(|c| c.path.clone()) else {
        return;
    };
    if !is_under(&path, BOLA) || !crate::throw::is_carbon(world, hit) {
        return;
    }
    // `cult/throw_impact`: it passes harmlessly through cultists.
    if is_under(&path, CULT_BOLA) && crate::antag::cult::server::is_cultist(world, hit) {
        return;
    }
    if is_under(&path, ENERGY_BOLA) {
        energy_ensnare(world, item, hit);
    } else {
        ensnare(world, item, hit);
    }
}

/// `bola/ensnare`.
fn ensnare(world: &mut World, item: Entity, mob: Entity) {
    if world.get::<Legcuffed>(mob).is_some() || !two_legs(world, mob) {
        return;
    }
    let Some(&at) = world.get::<TilePos>(mob) else {
        return;
    };
    let knockdown = world.get::<DmLegcuffs>(item).map_or(0.0, |c| c.knockdown);
    let (what, who) = (name_of(world, item), name_of(world, mob));
    show(
        world,
        at,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (Span::Danger, format!("{what} опутывает {who}!")),
        &[(mob, Span::BoldWarning, format!("{what} опутывает вас!"))],
    );
    equip(world, item, mob);
    if knockdown > 0.0 {
        crate::status_effects::apply(world, mob, Incap::Knockdown, knockdown / 10.0);
    }
    playsound(world, PlaySound::new(at, SNAP_SOUND, 50.0, true));
}

/// `energy/ensnare`: an energy snare springs on the target, the bola is gone.
fn energy_ensnare(world: &mut World, item: Entity, mob: Entity) {
    if let Some(&at) = world.get::<TilePos>(mob)
        && let Some(snare) = crate::construction::spawn_obj(world, ENERGY_SNARE_CYBORG, at)
    {
        spring_trap(world, snare, mob, true, true);
        // `if(B.loc != hit_atom) qdel(B)`.
        if world.get::<TilePos>(snare).is_some() {
            world.entity_mut(snare).despawn();
        }
    }
    world.entity_mut(item).despawn();
}

/// `energy/dissipate`: unless a mob holds it, it sparks and is gone.
// Cost: one pass over energy snares (a handful) per tick.
fn dissipate(world: &mut World) {
    let now = crate::tools::now(world);
    let due: Vec<Entity> = world
        .query::<(Entity, &DissipateAt)>()
        .iter(world)
        .filter(|(_, d)| d.0 <= now)
        .map(|(e, _)| e)
        .collect();
    for snare in due {
        world.entity_mut(snare).remove::<DissipateAt>();
        if let Some(&pos) = world.get::<TilePos>(snare) {
            crate::effects::particles::do_sparks(world, 1, true, pos);
            world.entity_mut(snare).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<CombatRng>();
        world
    }

    #[test]
    fn missing_a_leg_is_not_two_legs() {
        let mut world = world();
        let whole = world.spawn_empty().id();
        assert!(two_legs(&world, whole), "no limb data: two legs");
        let mut limbs = Limbs::default();
        limbs.limb_mut(BodyZone::LLeg).missing = true;
        let one_leg = world.spawn(limbs).id();
        assert!(!two_legs(&world, one_leg));
    }

    #[test]
    fn a_gonbola_is_only_a_gonbola_when_worn() {
        let mut world = world();
        let item = world
            .spawn(DmLegcuffs {
                path: GONBOLA.to_owned(),
                ..default()
            })
            .id();
        let bare = world.spawn_empty().id();
        assert!(!gonbola_worn(&world, bare));
        let worn = world.spawn(Legcuffed(item)).id();
        assert!(gonbola_worn(&world, worn));
    }
}
