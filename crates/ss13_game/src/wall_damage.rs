//! Wall dents and rust (server), after tg `/turf/closed/wall/proc/add_dent`, `try_clean`,
//! `rust_turf` (`code/game/turfs/closed/walls.dm`, `reinf_walls.dm`, `floor.dm`, `turf.dm`) and
//! `/datum/element/rust` (`code/datums/elements/rust.dm`).
//!
//! - Dents ([`WallDents`]): bullet holes where a brute or burn projectile hit a wall (75%),
//!   impact marks from a Hulk's punch; at most 15. A lit welder with 1 fuel (not in combat
//!   mode) on a dented wall welds every dent away at once, before it would cut the wall.
//! - Rust ([`Rusty`]): mapped rusty walls and plating carry it (the fork's overlay); a
//!   wirebrush (2 s) or a welder with 1 fuel (5 s), right click, scrapes or burns it off; a
//!   rusty floor takes no tiles or rods. [`rust_turf`] rusts a turf: a rusty wall crumbles, a
//!   rusty reinforced wall becomes a plain rusty wall, a floor becomes rusty plating.
// shortcut: dents land at a random offset rather than the projectile's entry point; no space
// cola washing rust off, no heretic rust effects (runes, corruption of who walks on it), no
// `NO_RUST`/`TRAIT_RUSTIMMUNE` turfs (no caller rusts those yet).

use bevy::prelude::*;
use ss13_mapload::construct::DmTurfBuild;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmTool, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, IconStateId, Overlay, TilePos};

use crate::combat::{CombatMode, CombatRng};
use crate::construction::{Job, baseturfs_of, change_turf, path_of, scrape_away, turf_at};
use crate::security::name_of;
use crate::tools::{Actor, ToolKind, tell, tool_of, tool_usable, use_tool};

const WALL: &str = "/turf/closed/wall";
const R_WALL: &str = "/turf/closed/wall/r_wall";
const FLOOR: &str = "/turf/open/floor";
const PLATING: &str = "/turf/open/floor/plating";
pub const RUSTY_WALL: &str = "/turf/closed/wall/rust";
const EFFECTS_DMI: &str = "icons/effects/effects.dmi";
/// BandaStation's `/datum/element/rust` icon (`SS220 EDIT - AESTHETICS`).
const RUST_DMI: &str = "modular_bandastation/aesthetics/walls/icons/rust_overlay.dmi";
const RUST_STATE: &str = "rust_default";
/// `MAX_DENT_DECALS`.
const MAX_DENTS: usize = 15;
/// `TOOL_RUSTSCRAPER`.
const RUSTSCRAPER: &str = "rustscraper";
const TILE: &str = "/obj/item/stack/tile";
const RODS: &str = "/obj/item/stack/rods";

/// Types that `AddElement(/datum/element/rust)` (or its heretic kind) in `Initialize`.
const MAPPED_RUST: [&str; 7] = [
    "/turf/closed/wall/rust",
    "/turf/closed/wall/heretic_rust",
    "/turf/closed/wall/r_wall/rust",
    "/turf/closed/wall/r_wall/heretic_rust",
    "/turf/open/floor/plating/rust",
    "/turf/open/floor/plating/heretic_rust",
    "/turf/open/floor/plating/plasma/rust",
];
/// Mapped only tinted (`color = COLOR_BROWN`), cleared by their `Initialize`.
const MAP_TINTED: [&str; 1] = ["/turf/open/floor/plating/rust"];

/// `WALL_DENT_SHOT`, `WALL_DENT_HIT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dent {
    Shot,
    Hit,
}

/// A wall's `dent_decals`: the overlays added for them.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct WallDents(pub Vec<IconStateId>);

/// `TRAIT_RUSTY` of a turf.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rusty;

/// A timed rust removal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallJob {
    ScrapeRust,
}

pub struct WallDamagePlugin;

impl Plugin for WallDamagePlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // Welding dents, scraping or burning rust off, tiles refused by rusty floors.
        app.add_click_handler(ClickPhase::HeldOnTarget, 2969, |w, c| {
            c.held.is_some_and(|h| {
                crate::wall_damage::item_interaction(
                    w,
                    c.client,
                    c.mob,
                    h,
                    c.target,
                    c.modifiers.right,
                )
            })
        });
        app.add_observer(mapped_rust);
    }
}

/// The mapped rusty types get their rust (and lose the mapping tint).
fn mapped_rust(add: On<Add<DmTurfBuild>>, turfs: Query<&DmTurfBuild>, mut commands: Commands) {
    let e = add.entity;
    let Ok(build) = turfs.get(e) else {
        return;
    };
    if !MAPPED_RUST.iter().any(|p| is_under(&build.path, p)) {
        return;
    }
    let untint = MAP_TINTED.iter().any(|p| is_under(&build.path, p));
    commands.queue(move |world: &mut World| {
        if world.get_entity(e).is_err() {
            return;
        }
        if untint && let Some(mut appearance) = world.get_mut::<Appearance>(e) {
            appearance.color = None;
        }
        add_rust(world, e);
    });
}

fn icon(world: &World, dmi: &str, state: &str) -> Option<IconStateId> {
    world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(dmi, state))
}

/// `add_dent(denttype, x, y)` at a random offset within 8 px.
pub fn add_dent(world: &mut World, wall: Entity, kind: Dent) {
    if world
        .get::<WallDents>(wall)
        .is_some_and(|d| d.0.len() >= MAX_DENTS)
    {
        return;
    }
    let (state, x, y) = {
        let mut rng = world.resource_mut::<CombatRng>();
        let state = match kind {
            Dent::Shot => "bullet_hole".to_owned(),
            Dent::Hit => format!("impact{}", rng.range(1, 3)),
        };
        let x = rng.range(0, 16) as i16 - 8;
        let y = rng.range(0, 16) as i16 - 8;
        (state, x, y)
    };
    let Some(id) = icon(world, EFFECTS_DMI, &state) else {
        return;
    };
    let Some(mut appearance) = world.get_mut::<Appearance>(wall) else {
        return;
    };
    let mut overlay = Overlay::new(id);
    overlay.pixel_offset = [x, y];
    appearance.overlays.push(overlay);
    match world.get_mut::<WallDents>(wall) {
        Some(mut dents) => dents.0.push(id),
        None => {
            world.entity_mut(wall).insert(WallDents(vec![id]));
        }
    }
}

/// A brute or burn projectile hitting a wall (`projectile/on_hit`, 75%).
pub fn projectile_hit(world: &mut World, target: Entity) {
    let wall = path_of(world, target).is_some_and(|p| is_under(&p, WALL));
    if wall && world.resource_mut::<CombatRng>().chance(75.0) {
        add_dent(world, target, Dent::Shot);
    }
}

/// `try_clean`: every dent off at once.
fn clean_dents(world: &mut World, wall: Entity) {
    let Some(dents) = world.get::<WallDents>(wall).cloned() else {
        return;
    };
    world.entity_mut(wall).remove::<WallDents>();
    if let Some(mut appearance) = world.get_mut::<Appearance>(wall) {
        // One overlay per dent, even when two share an icon.
        for id in dents.0 {
            if let Some(i) = appearance.overlays.iter().position(|o| o.icon_state == id) {
                appearance.overlays.remove(i);
            }
        }
    }
}

/// `AddElement(/datum/element/rust)`.
pub fn add_rust(world: &mut World, turf: Entity) {
    if world.get::<Rusty>(turf).is_some() {
        return;
    }
    world.entity_mut(turf).insert(Rusty);
    if let Some(id) = icon(world, RUST_DMI, RUST_STATE)
        && let Some(mut appearance) = world.get_mut::<Appearance>(turf)
    {
        appearance.overlays.push(Overlay::new(id));
    }
}

/// The rust element's `Detach`.
pub fn remove_rust(world: &mut World, turf: Entity) {
    if world.entity_mut(turf).take::<Rusty>().is_none() {
        return;
    }
    if let Some(id) = icon(world, RUST_DMI, RUST_STATE)
        && let Some(mut appearance) = world.get_mut::<Appearance>(turf)
        && let Some(i) = appearance.overlays.iter().position(|o| o.icon_state == id)
    {
        appearance.overlays.remove(i);
    }
}

/// tg `rust_turf()` of the turf of `pos`. True when something happened.
pub fn rust_turf(world: &mut World, pos: TilePos) -> bool {
    let Some(turf) = turf_at(world, pos) else {
        return false;
    };
    let Some(path) = path_of(world, turf) else {
        return false;
    };
    let rusty = world.get::<Rusty>(turf).is_some();
    if is_under(&path, R_WALL) && rusty {
        // `/turf/closed/wall/r_wall/rust_turf`: `ChangeTurf(/turf/closed/wall/rust)`.
        let stack = baseturfs_of(world, turf);
        return change_turf(world, pos, RUSTY_WALL, stack, false).is_some();
    }
    if is_under(&path, WALL) && rusty {
        return scrape_away(world, pos, false).is_some();
    }
    if rusty {
        return false;
    }
    let turf = if is_under(&path, FLOOR) {
        // `/turf/open/floor/rust_turf`: `ChangeTurf(/turf/open/floor/plating)` first.
        let stack = baseturfs_of(world, turf);
        match change_turf(world, pos, PLATING, stack, true) {
            Some(t) => t,
            None => return false,
        }
    } else {
        turf
    };
    add_rust(world, turf);
    true
}

/// The clicks of this module. True when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    right: bool,
) -> bool {
    let Some(path) = path_of(world, target) else {
        return false;
    };
    let rusty = world.get::<Rusty>(target).is_some();
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    let welder = tool_of(world, held).map(|t| t.0) == Some(ToolKind::Welder);
    if right && rusty {
        // `secondary_tool_act` of the rust element: a welder or a rust scraper.
        let scraper = world
            .get::<DmTool>(held)
            .is_some_and(|t| t.behaviour == RUSTSCRAPER);
        let (delay, text) = if scraper {
            (20.0, "Вы начинаете соскребать ржавчину...")
        } else if welder {
            if !tool_usable(world, held, 1, 0.0) {
                let text = crate::tools::unusable_message(world, held, 1);
                tell(world, client, Span::Warning, text);
                return true;
            }
            (50.0, "Вы начинаете выжигать ржавчину...")
        } else {
            return false;
        };
        if use_tool(
            world,
            actor,
            target,
            delay,
            0,
            Job::WallDamage(WallJob::ScrapeRust),
        ) {
            tell(world, client, Span::Notice, text);
        }
        return true;
    }
    if right {
        return false;
    }
    // `on_interaction` of the rust element: no tiles or rods on a rusty turf.
    if rusty
        && world
            .get::<DmItem>(held)
            .is_some_and(|i| is_under(&i.path, TILE) || is_under(&i.path, RODS))
    {
        tell(world, client, Span::Warning, "Пол слишком проржавел!");
        return true;
    }
    // `try_clean`: a welder on a dented wall, out of combat mode.
    let combat = world.get::<CombatMode>(mob).is_some_and(|c| c.0);
    if welder && !combat && is_under(&path, WALL) && world.get::<WallDents>(target).is_some() {
        if !tool_usable(world, held, 1, 0.0) {
            return false;
        }
        let what = name_of(world, target);
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы начинаете чинить вмятины на {what}..."),
        );
        clean_dents(world, target);
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы запаиваете некоторые вмятины на {what}."),
        );
        return true;
    }
    false
}

/// The end of a [`WallJob`].
pub fn complete(world: &mut World, client: Entity, target: Entity, job: WallJob) {
    match job {
        WallJob::ScrapeRust => {
            if world.get::<Rusty>(target).is_none() {
                return;
            }
            remove_rust(world, target);
            tell(world, client, Span::Notice, "Вы счищаете ржавчину.");
        }
    }
}

/// The rust element's examine line.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    if world.get::<Rusty>(target).is_none() {
        return;
    }
    let what = ss13_shared::capitalize(&name_of(world, target));
    lines.push((
        Span::Notice,
        format!(
            "{what} сильно проржавело — ржавчину, пожалуй, можно выжечь или соскрести, а то и \
             полить космоколой."
        ),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapped_rust_types() {
        let rusty = |p: &str| MAPPED_RUST.iter().any(|r| is_under(p, r));
        assert!(rusty("/turf/closed/wall/rust"));
        assert!(rusty("/turf/open/floor/plating/rust/airless"));
        assert!(rusty("/turf/closed/wall/r_wall/heretic_rust"));
        assert!(!rusty("/turf/closed/wall"));
        assert!(!rusty("/turf/open/floor/plating/rusty"));
    }
}
