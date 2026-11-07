//! T-ray scanner (server): tg `/obj/item/t_scanner`
//! (`code/game/objects/items/devices/scanners/t_scanner.dm`, `t_ray_scan`).
//!
//! In hand, `attack_self` switches it on (`t-ray1`). While on and directly on a mob (hand,
//! belt, pocket: tg `t_ray_scan(loc)` needs `loc` to be the mob), every second the objects
//! hidden under floor tiles (`ss13_mapload::HiddenUnderTile`, tg `TRAIT_T_RAY_VISIBLE`) within
//! 3 tiles show to that mob's client for 0.8 s at half alpha (`flick_overlay_global`, 8 ds).
//!
//! Hidden objects are not [`Networked`] at all (`crate::server`), so a pulse replicates them
//! with [`Invisibility`] above every client's `see_invisible` and [`VisibleToMobs`] of the
//! scanning mobs: only their clients get them, through an [`AltAppearances`] entry at alpha 128
//! that clicks never stop at (tg images are not clickable). The pulse ends by taking all of it
//! away again; a tile pried up meanwhile hands the object back to everyone.
//! All scanners pulse together on one 1 s clock, so an object seen by several scanning mobs is
//! replicated once with all of them (the visibility of a replicated entity is read only when it
//! first reaches a client).

use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmItem, HiddenUnderTile, IconIndex};
use ss13_net::server::{
    AltAppearance, AltAppearances, Invisibility, Networked, Seers, VisibleToMobs,
};
use ss13_shared::{Appearance, MouseOpacity, TilePos};
use ss13_world::SpatialIndex;

use crate::inventory::{CarriedBy, Inventory};
use crate::obj_damage::is_under;

const T_SCANNER: &str = "/obj/item/t_scanner";
/// A mining scanner subtype with its own `scan()` (`crate::mining`).
const MINING_SCANNER: &str = "/obj/item/t_scanner/adv_mining_scanner";
/// `SSobj` process interval of the scanner.
const PULSE_SECS: f64 = 1.0;
/// `flick_time = 8` deciseconds.
const FLICK_SECS: f64 = 0.8;
/// `distance = 3`.
const SCAN_RANGE: u32 = 3;
/// `MA.alpha = 128`.
const T_RAY_ALPHA: u8 = 128;
/// The [`AltAppearance`] key of a scanned object.
const ALT_KEY: &str = "t_ray";

/// A T-ray scanner's `on`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TScanner {
    pub on: bool,
}

/// An under-tile object a pulse replicates until `until` (`Time::elapsed_secs_f64`).
#[derive(Component, Clone, Copy, Debug)]
pub struct TRayShown {
    pub until: f64,
}

/// Mobs scanned by the next pulse once, without a scanner (a ghost's "T-ray scan",
/// `crate::ghost_verbs`).
#[derive(Resource, Default, Debug)]
pub struct TRayRequests(pub Vec<Entity>);

/// The shared pulse clock.
#[derive(Resource, Default)]
struct TRayClock {
    next: f64,
}

pub struct TScannerPlugin;

impl Plugin for TScannerPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::AttackSelf, 210, |w, c| {
            toggle(w, c.mob, c.target)
        });
        app.init_resource::<TRayClock>()
            .init_resource::<TRayRequests>()
            .add_observer(add_scanner)
            .add_observer(uncovered)
            .add_systems(Update, (end_flicks, pulse).chain());
    }
}

/// True for the scanners this module runs.
pub fn is_t_scanner(path: &str) -> bool {
    is_under(path, T_SCANNER) && !is_under(path, MINING_SCANNER)
}

fn add_scanner(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if items.get(add.entity).is_ok_and(|i| is_t_scanner(&i.path)) {
        commands
            .entity(add.entity)
            .try_insert_if_new(TScanner::default());
    }
}

/// `toggle_on`: `on` flips, the sprite's last digit follows (`t-ray0` -> `t-ray1`).
// shortcut: no `SFX_INDUSTRIAL_SCAN` (not among the converted sounds yet).
fn toggle(world: &mut World, mob: Entity, item: Entity) -> bool {
    let Some(mut scanner) = world.get_mut::<TScanner>(item) else {
        return false;
    };
    scanner.on = !scanner.on;
    let on = scanner.on;
    let id = match (world.get::<DmAtom>(item), world.get_resource::<IconIndex>()) {
        (Some(atom), Some(icons)) => icons.get(&atom.icon, &state_for(&atom.icon_state, on)),
        _ => None,
    };
    if let (Some(id), Some(mut appearance)) = (id, world.get_mut::<Appearance>(item))
        && appearance.icon_state != id
    {
        appearance.icon_state = id;
    }
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.set_changed();
    }
    true
}

/// `copytext_char(icon_state, 1, -1) + "[on]"` of the mapped (off) state.
fn state_for(base: &str, on: bool) -> String {
    let mut chars = base.chars();
    chars.next_back();
    format!("{}{}", chars.as_str(), u8::from(on))
}

/// `MA.alpha = 128` over the object's own color.
fn t_ray_color(color: Option<[u8; 4]>) -> [u8; 4] {
    let [r, g, b, a] = color.unwrap_or([255; 4]);
    [
        r,
        g,
        b,
        ((u16::from(a) * u16::from(T_RAY_ALPHA)) / 255) as u8,
    ]
}

/// Every second: each scanning mob's surroundings (`orange(3)`) are shown to it.
/// Cost: one pass over the scanners (a few dozen) per second, a 7x7 tile box each.
fn pulse(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < world.resource::<TRayClock>().next {
        return;
    }
    world.resource_mut::<TRayClock>().next = now + PULSE_SECS;
    let mut scanners = world.query::<(&TScanner, &CarriedBy)>();
    let mut viewers: Vec<Entity> = scanners
        .iter(world)
        .filter(|(s, _)| s.on)
        .map(|(_, c)| c.0)
        .collect();
    viewers.append(&mut world.resource_mut::<TRayRequests>().0);
    viewers.sort();
    viewers.dedup();
    let mut seen: Vec<(Entity, Entity)> = Vec::new();
    for mob in viewers {
        let Some(&pos) = world.get::<TilePos>(mob) else {
            continue;
        };
        let index = world.resource::<SpatialIndex>();
        seen.extend(
            index
                .in_radius(pos, SCAN_RANGE)
                .filter(|&e| {
                    world.get::<HiddenUnderTile>(e).is_some()
                        && world.get::<TilePos>(e).is_some_and(|p| p.z == pos.z)
                        && world.get::<Appearance>(e).is_some()
                        && world.get::<Networked>(e).is_none()
                        && world.get::<AltAppearances>(e).is_none()
                        && world.get::<DmAtom>(e).is_none_or(|a| a.invisibility == 0)
                })
                .map(|e| (e, mob)),
        );
    }
    seen.sort();
    let until = now + FLICK_SECS;
    for chunk in seen.chunk_by(|a, b| a.0 == b.0) {
        let thing = chunk[0].0;
        let mobs: Vec<Entity> = chunk.iter().map(|&(_, m)| m).collect();
        let Some(mut look) = world.get::<Appearance>(thing).cloned() else {
            continue;
        };
        look.color = Some(t_ray_color(look.color));
        look.mouse_opacity = MouseOpacity::Transparent;
        world.entity_mut(thing).insert((
            TRayShown { until },
            Invisibility(u8::MAX),
            VisibleToMobs(mobs.clone()),
            AltAppearances(vec![AltAppearance {
                key: ALT_KEY.to_owned(),
                seers: Seers { mobs, groups: 0 },
                appearance: look,
            }]),
            Networked,
        ));
    }
}

/// The flick is over: clients forget the object again.
fn end_flicks(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut shown = world.query::<(Entity, &TRayShown)>();
    let done: Vec<Entity> = shown
        .iter(world)
        .filter(|(_, s)| s.until <= now)
        .map(|(e, _)| e)
        .collect();
    for thing in done {
        world.entity_mut(thing).remove::<(
            TRayShown,
            Invisibility,
            VisibleToMobs,
            AltAppearances,
            Networked,
        )>();
    }
}

/// The floor above a shown object is gone (`crate::server` replicates it again): everyone sees
/// it as itself. The appearance is touched so the clients that were not told about it spawn it.
fn uncovered(
    remove: On<Remove<HiddenUnderTile>>,
    shown: Query<(), With<TRayShown>>,
    mut commands: Commands,
) {
    if shown.get(remove.entity).is_err() {
        return;
    }
    let thing = remove.entity;
    commands.queue(move |world: &mut World| {
        let Ok(mut entity) = world.get_entity_mut(thing) else {
            return;
        };
        entity.remove::<(TRayShown, Invisibility, VisibleToMobs, AltAppearances)>();
        if let Some(mut appearance) = entity.get_mut::<Appearance>() {
            appearance.set_changed();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprite_digit_follows_on() {
        assert_eq!(state_for("t-ray0", true), "t-ray1");
        assert_eq!(state_for("t-ray0", false), "t-ray0");
    }

    #[test]
    fn half_alpha_keeps_tint() {
        assert_eq!(t_ray_color(None), [255, 255, 255, 128]);
        assert_eq!(t_ray_color(Some([10, 20, 30, 255])), [10, 20, 30, 128]);
        assert_eq!(t_ray_color(Some([10, 20, 30, 0])), [10, 20, 30, 0]);
    }

    #[test]
    fn mining_scanner_is_not_a_t_ray() {
        assert!(is_t_scanner("/obj/item/t_scanner"));
        assert!(is_t_scanner("/obj/item/t_scanner/drone"));
        assert!(!is_t_scanner(
            "/obj/item/t_scanner/adv_mining_scanner/lesser"
        ));
        assert!(!is_t_scanner("/obj/item/t_scanner_x"));
    }
}
