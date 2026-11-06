//! Soil plots and soil sacks (tg `code/modules/hydroponics/soil.dm`): a sack used on an open
//! floor puts down its plot (1 s), a shovel's right click bags a plot back up (3 s), and a sack
//! used on an empty tray fills it with its soil (2 s): the tray takes the soil's flags, and a
//! constructable one its water and nutrient capacity (`RefreshParts`).
//!
//! A bagged plot keeps its water, nutrients and levels in the sack ([`StoredSoil`]); a sack
//! that never held a plot makes a fresh one, full of water and half of its nutrients as
//! E-Z-Nutrient (`transfer_soil`).
// shortcut: sacks are one-handed (no `two_handed` wielding, slowdown or block chance); plots
// are not deconstructed into sand; trays keep their name (BandaStation renames a filled tray);
// the worm plot's wiggle flick is not played.

use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_mapload::plants::SOIL_ROOT;
use ss13_reagents::{Holder, is_under};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::SpatialIndex;

use super::tray::{PlantStatus, Tray};
use super::{BotanyJob, refresh_parts, refresh_tray, setup_tray};
use crate::chem::{Chem, Reagents};
use crate::construction::Job;
use crate::tools::{Actor, DoAfter, do_after, tell};

pub const SACK_ROOT: &str = "/obj/item/soil_sack";
const EZ_NUTRIENT: &str = "/datum/reagent/plantnutriment/eznutriment";
const PLOP: &str = "sound/effects/soil_plop.ogg";
/// `do_after` of placing a plot, filling a tray, `use_tool` of bagging a plot; deciseconds.
const PLACE_TIME: f32 = 10.0;
const FILL_TIME: f32 = 20.0;
const BAG_TIME: f32 = 30.0;

/// A soil type with its sack (`sack_type` / `stored_soil`), `icon_state` (the tray overlay is
/// `"[icon_state]_tray"`), `name` and the sack's `placement_sound`.
pub struct SoilKind {
    pub soil: &'static str,
    pub sack: &'static str,
    pub icon_state: &'static str,
    pub name: &'static str,
    pub sound: &'static str,
}

pub const SOILS: [SoilKind; 6] = [
    SoilKind {
        soil: SOIL_ROOT,
        sack: SACK_ROOT,
        icon_state: "soil",
        name: "soil",
        sound: PLOP,
    },
    SoilKind {
        soil: "/obj/machinery/hydroponics/soil/vermaculite",
        sack: "/obj/item/soil_sack/vermaculite",
        icon_state: "soil_verm",
        name: "vermaculite growing medium",
        sound: PLOP,
    },
    SoilKind {
        soil: "/obj/machinery/hydroponics/soil/gel",
        sack: "/obj/item/soil_sack/gel",
        icon_state: "soil_gel",
        name: "hydrogel beads",
        sound: "sound/effects/meatslap.ogg",
    },
    SoilKind {
        soil: "/obj/machinery/hydroponics/soil/coir",
        sack: "/obj/item/soil_sack/coir",
        icon_state: "soil_coir",
        name: "korta root coir",
        sound: PLOP,
    },
    SoilKind {
        soil: "/obj/machinery/hydroponics/soil/worm",
        sack: "/obj/item/soil_sack/worm",
        icon_state: "soil_worm",
        name: "worm castings",
        sound: PLOP,
    },
    SoilKind {
        soil: "/obj/machinery/hydroponics/soil/rich",
        sack: "/obj/item/soil_sack/rich",
        icon_state: "rich_soil",
        name: "rich soil",
        sound: PLOP,
    },
];

/// The kind of a soil type (its most specific listed ancestor).
pub fn soil_kind(soil: &str) -> Option<&'static SoilKind> {
    SOILS
        .iter()
        .filter(|k| is_under(soil, k.soil))
        .max_by_key(|k| k.soil.len())
}

/// The kind a sack type holds by default (its most specific listed ancestor).
pub fn sack_kind(sack: &str) -> Option<&'static SoilKind> {
    SOILS
        .iter()
        .filter(|k| is_under(sack, k.sack))
        .max_by_key(|k| k.sack.len())
}

/// The plot a sack holds: its type and, once bagged from the floor, its state and nutrients.
#[derive(Component, Clone, Debug)]
pub struct StoredSoil {
    pub soil: String,
    pub state: Option<(Tray, Holder)>,
}

fn path_of(world: &World, e: Entity) -> String {
    world
        .get::<DmObj>(e)
        .map(|o| o.path.clone())
        .unwrap_or_default()
}

fn item_path(world: &World, e: Entity) -> String {
    world
        .get::<ss13_mapload::DmItem>(e)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

pub fn is_soil_plot(world: &World, e: Entity) -> bool {
    is_under(&path_of(world, e), SOIL_ROOT)
}

fn soil_on(world: &World, pos: TilePos) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| is_soil_plot(world, e))
}

/// `isopenturf(target) && !isgroundlessturf(target)`.
fn plantable_turf(world: &World, turf: Entity) -> bool {
    world
        .get::<ss13_mapload::DmTurfBuild>(turf)
        .is_some_and(|b| {
            is_under(&b.path, "/turf/open")
                && ![
                    "/turf/open/space",
                    "/turf/open/openspace",
                    "/turf/open/chasm",
                    "/turf/open/lava",
                ]
                .iter()
                .any(|root| is_under(&b.path, root))
        })
}

/// The soil type in a sack.
fn sack_soil(world: &World, sack: Entity) -> Option<String> {
    if let Some(stored) = world.get::<StoredSoil>(sack) {
        return Some(stored.soil.clone());
    }
    sack_kind(&item_path(world, sack)).map(|k| k.soil.to_owned())
}

/// `/obj/item/soil_sack/interact_with_atom` and the tray's `item_interaction` with a sack.
/// True when `held` is a sack and the click was its.
pub fn sack_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if !is_under(&item_path(world, held), SACK_ROOT) {
        return false;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    if let Some(state) = world.get::<Tray>(target) {
        // A plot is a tray too in tg (a sack would fill it with a second soil); here it is the
        // bed already on that floor.
        if is_soil_plot(world, target) {
            tell(
                world,
                client,
                Span::Warning,
                "There is already a bed of soil there!",
            );
            return true;
        }
        if state.plant_status != PlantStatus::Missing {
            tell(world, client, Span::Warning, "remove the plants first!");
            return true;
        }
        if state.current_soil.is_some() {
            tell(world, client, Span::Warning, "tray is full!");
            return true;
        }
        tell(world, client, Span::Notice, "filling the tray...");
        do_after(
            world,
            actor,
            target,
            FILL_TIME,
            0,
            Job::Botany(BotanyJob::FillTray),
        );
        return true;
    }
    if world.get::<ss13_mapload::Turf>(target).is_none() || !plantable_turf(world, target) {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return false;
    };
    if soil_on(world, pos) {
        tell(
            world,
            client,
            Span::Warning,
            "There is already a bed of soil there!",
        );
        return true;
    }
    do_after(
        world,
        actor,
        target,
        PLACE_TIME,
        0,
        Job::Botany(BotanyJob::PlaceSoil),
    );
    true
}

/// `item_interaction_secondary` of a plot with a shovel: bagging it starts. True when the click
/// was the plot's.
pub fn shovel_secondary(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    plot: Entity,
) -> bool {
    if !is_soil_plot(world, plot) {
        return false;
    }
    tell(world, client, Span::Notice, "digging up soil...");
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    crate::tools::use_tool(
        world,
        actor,
        plot,
        BAG_TIME,
        0,
        Job::Botany(BotanyJob::BagSoil),
    );
    true
}

/// The end of placing a plot (`transfer_soil` onto the floor).
pub fn complete_place(world: &mut World, _mob: Entity, action: &DoAfter) {
    let (Some(sack), pos) = (action.item, action.target_pos) else {
        return;
    };
    if soil_on(world, pos) || !plantable_turf(world, action.target) {
        return;
    }
    let Some(soil) = sack_soil(world, sack) else {
        return;
    };
    let stored = world.get::<StoredSoil>(sack).and_then(|s| s.state.clone());
    let Some(plot) = crate::construction::spawn_obj(world, &soil, pos) else {
        warn!("soil sack at {pos:?}: no prototype for {soil}");
        return;
    };
    match stored {
        Some((state, nutrients)) => {
            world.entity_mut(plot).insert((state, Reagents(nutrients)));
            refresh_tray(world, plot);
        }
        None => {
            setup_tray(world, plot, false);
            fresh_fill(world, plot);
        }
    }
    let sound = soil_kind(&soil).map_or(PLOP, |k| k.sound);
    crate::sound::playsound(world, crate::sound::PlaySound::new(pos, sound, 65.0, true));
    world.despawn(sack);
}

/// A new plot out of a sack: full of water, `maxnutri / 2` E-Z-Nutrient.
fn fresh_fill(world: &mut World, plot: Entity) {
    let Some(maxnutri) = world.get::<Tray>(plot).map(|t| t.maxnutri) else {
        return;
    };
    if let Some(mut state) = world.get_mut::<Tray>(plot) {
        state.waterlevel = state.maxwater;
    }
    let chem = world.get_resource::<Chem>().map(|c| c.0.clone());
    if let (Some(chem), Some(mut r)) = (chem, world.get_mut::<Reagents>(plot))
        && let Some(id) = chem.id(EZ_NUTRIENT)
    {
        r.0.add_reagent_list(&chem, &[(id, maxnutri / 2.0)], None, true);
    }
    refresh_tray(world, plot);
}

/// The end of filling a tray: the soil goes in if the tray is still empty.
pub fn complete_fill(world: &mut World, _mob: Entity, action: &DoAfter) {
    let (Some(sack), tray) = (action.item, action.target) else {
        return;
    };
    let empty = world
        .get::<Tray>(tray)
        .is_some_and(|t| t.plant_status == PlantStatus::Missing && t.current_soil.is_none());
    let Some(soil) = sack_soil(world, sack).filter(|_| empty) else {
        return;
    };
    let flags = world
        .get_resource::<super::Botany>()
        .and_then(|b| b.0.tray_def(&soil).map(|d| d.tray_flags));
    if let Some(mut state) = world.get_mut::<Tray>(tray) {
        state.current_soil = Some(soil.clone());
        if let Some(flags) = flags {
            state.tray_flags = flags;
        }
    }
    refresh_parts(world, tray);
    let sound = soil_kind(&soil).map_or(PLOP, |k| k.sound);
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(action.target_pos, sound, 65.0, true),
    );
    world.despawn(sack);
    refresh_tray(world, tray);
}

/// The end of bagging a plot: `new sack_type(loc, src)` takes the plot, its plant removed.
pub fn complete_bag(world: &mut World, _mob: Entity, action: &DoAfter) {
    let plot = action.target;
    if !is_soil_plot(world, plot) {
        return;
    }
    let (Some(mut state), Some(nutrients)) = (
        world.get::<Tray>(plot).cloned(),
        world.get::<Reagents>(plot).map(|r| r.0.clone()),
    ) else {
        return;
    };
    let soil = path_of(world, plot);
    let Some(kind) = soil_kind(&soil) else {
        return;
    };
    state.remove_plant();
    let pos = action.target_pos;
    let Some(sack) = crate::construction::spawn_obj(world, kind.sack, pos) else {
        warn!("soil plot at {pos:?}: no prototype for {}", kind.sack);
        return;
    };
    world.entity_mut(sack).insert(StoredSoil {
        soil,
        state: Some((state, nutrients)),
    });
    world.despawn(plot);
    crate::construction::tile_changed(world, pos);
    tell(world, action.client, Span::Notice, "bagged");
}

/// tg `update_desc`/`update_overlays` of a tray with soil: its `"[icon_state]_tray"` overlay
/// state and the "Filled with" line.
pub fn tray_soil(state: &Tray) -> Option<&'static SoilKind> {
    state.current_soil.as_deref().and_then(soil_kind)
}

/// tg `update_desc` of a tray with soil.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    if let Some(kind) = world.get::<Tray>(target).and_then(tray_soil) {
        lines.push((Span::Info, format!("Filled with {}.", kind.name)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soil_kinds_follow_their_types() {
        assert_eq!(
            soil_kind("/obj/machinery/hydroponics/soil/gel").map(|k| k.icon_state),
            Some("soil_gel")
        );
        assert_eq!(soil_kind(SOIL_ROOT).map(|k| k.sack), Some(SACK_ROOT));
        assert_eq!(
            sack_kind("/obj/item/soil_sack/worm").map(|k| k.soil),
            Some("/obj/machinery/hydroponics/soil/worm")
        );
        // An unlisted subtype falls back to its listed ancestor.
        assert_eq!(
            sack_kind("/obj/item/soil_sack/rich/special").map(|k| k.name),
            Some("rich soil")
        );
        assert!(soil_kind("/obj/machinery/hydroponics/constructable").is_none());
        let state = Tray {
            current_soil: Some("/obj/machinery/hydroponics/soil/coir".into()),
            ..Tray::default()
        };
        assert_eq!(tray_soil(&state).map(|k| k.icon_state), Some("soil_coir"));
    }
}
