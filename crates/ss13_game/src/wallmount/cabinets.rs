//! Extinguisher and fire axe cabinets (tg `extinguisher.dm`, `fireaxe.dm`).
//!
//! - Extinguisher cabinet: a map cabinet starts closed with a fire extinguisher. An empty hand
//!   takes the extinguisher (opening the cabinet), or opens/closes an empty one; a right click
//!   opens/closes it; an extinguisher clicked on an open, empty cabinet goes in, any other item
//!   (out of combat mode) opens/closes it.
//! - Fire axe cabinet: locked, with a fire axe. A multitool toggles the lock after 2 s; an
//!   unlocked cabinet opens and closes by hand (right click too); an open one gives its axe to
//!   an empty hand and takes an axe back.
// shortcut: no glass damage, breaking or repair (the cabinets' integrity overlays, shards), no
// wrench deconstruction, no wielding check, no telekinesis; cyborgs and aliens are not told
// apart.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmWallMount};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::{Look, WallJob, refresh_look};
use crate::construction::Job;
use crate::inventory::Inventory;
use crate::sound::{MEDIUM_RANGE_SOUND_EXTRARANGE, PlaySound, playsound};
use crate::storage::StoredIn;
use crate::tools::{Actor, ToolKind, tell, tool_of};

const EXTINGUISHER: &str = "/obj/item/extinguisher";
const CLICK: &str = "sound/machines/click.ogg";
const LOCK_TOGGLE: &str = "sound/machines/locktoggle.ogg";
/// `toggle_lock`: `do_after(user, 2 SECONDS)`, deciseconds.
const LOCK_TOGGLE_DS: f32 = 20.0;

/// `/obj/structure/extinguisher_cabinet`: `stored_extinguisher`, `opened`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ExtinguisherCabinet {
    pub stored: Option<Entity>,
    pub opened: bool,
}

/// `/obj/structure/fireaxecabinet`: `held_item`, `open`, `locked`, `item_path`.
#[derive(Component, Clone, Debug)]
pub struct FireaxeCabinet {
    pub held: Option<Entity>,
    pub open: bool,
    pub locked: bool,
    pub item_path: String,
}

/// `new path(src)`: an item spawned straight into `cabinet`.
fn spawn_into(world: &mut World, path: &str, cabinet: Entity) -> Option<Entity> {
    let &pos = world.get::<TilePos>(cabinet)?;
    let item = crate::construction::spawn_obj(world, path, pos)?;
    crate::storage::put_inside(world, item, cabinet);
    Some(item)
}

/// `extinguisher_cabinet/Initialize(mapload)`: closed, with an extinguisher.
pub(super) fn add_extinguisher_cabinet(world: &mut World, e: Entity) {
    let stored = spawn_into(world, EXTINGUISHER, e);
    world.entity_mut(e).insert(ExtinguisherCabinet {
        stored,
        opened: false,
    });
}

/// `fireaxecabinet/Initialize`: `held_item = new item_path(src)` when it populates.
pub(super) fn add_fireaxe_cabinet(world: &mut World, e: Entity, dm: &DmWallMount) {
    let held = if dm.populate_contents && !dm.item_path.is_empty() {
        spawn_into(world, &dm.item_path, e)
    } else {
        None
    };
    world.entity_mut(e).insert(FireaxeCabinet {
        held,
        open: false,
        locked: dm.locked,
        item_path: dm.item_path.clone(),
    });
}

/// The item still in `cabinet` (tg `Exited` clears the var when it leaves some other way).
fn inside(world: &World, cabinet: Entity, item: Option<Entity>) -> Option<Entity> {
    item.filter(|&i| world.get::<StoredIn>(i).is_some_and(|s| s.0 == cabinet))
}

fn item_path(world: &World, item: Entity) -> Option<&str> {
    world.get::<DmItem>(item).map(|i| i.path.as_str())
}

fn click_sound(world: &mut World, cabinet: Entity, vol: f32, quiet: bool) {
    if let Some(&pos) = world.get::<TilePos>(cabinet) {
        let sound = PlaySound::new(pos, CLICK, vol, true);
        // `playsound(loc, 'click.ogg', 15, TRUE, -3)`.
        let sound = if quiet {
            sound.extrarange(MEDIUM_RANGE_SOUND_EXTRARANGE)
        } else {
            sound
        };
        playsound(world, sound);
    }
}

fn give(world: &mut World, mob: Entity, item: Entity) {
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, item);
    }
}

/// tg `attack_hand` / `attack_hand_secondary` of the cabinets. True when `target` is one.
pub(super) fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    right: bool,
) -> bool {
    if let Some(cabinet) = world.get::<ExtinguisherCabinet>(target).copied() {
        let stored = inside(world, target, cabinet.stored);
        match stored {
            Some(item) if !right => {
                give(world, mob, item);
                tell(world, client, Span::Notice, "extinguisher removed");
                let opened = !cabinet.opened;
                world.entity_mut(target).insert(ExtinguisherCabinet {
                    stored: None,
                    opened: true,
                });
                if opened {
                    click_sound(world, target, 15.0, true);
                }
                refresh_look(world, target);
            }
            _ => {
                world
                    .entity_mut(target)
                    .insert(ExtinguisherCabinet { stored, ..cabinet });
                toggle_extinguisher_cabinet(world, target);
            }
        }
        return true;
    }
    if let Some(cabinet) = world.get::<FireaxeCabinet>(target).cloned() {
        let held = inside(world, target, cabinet.held);
        if !right
            && cabinet.open
            && let Some(item) = held
        {
            give(world, mob, item);
            if let Some(mut c) = world.get_mut::<FireaxeCabinet>(target) {
                c.held = None;
            }
            refresh_look(world, target);
        } else {
            toggle_open(world, client, target);
        }
        return true;
    }
    false
}

/// tg `item_interaction` of the cabinets. True when the click was theirs.
pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    _modifiers: ss13_shared::protocol::ClickModifiers,
) -> bool {
    let combat = world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0);
    if let Some(cabinet) = world.get::<ExtinguisherCabinet>(target).copied() {
        // `ITEM_INTERACT_SKIP_TO_ATTACK`.
        if combat {
            return false;
        }
        let stored = inside(world, target, cabinet.stored);
        let is_extinguisher = item_path(world, held).is_some_and(|p| is_under(p, EXTINGUISHER));
        if is_extinguisher && stored.is_none() && cabinet.opened {
            crate::storage::put_inside(world, held, target);
            world.entity_mut(target).insert(ExtinguisherCabinet {
                stored: Some(held),
                opened: true,
            });
            tell(world, client, Span::Notice, "extinguisher stored");
            refresh_look(world, target);
        } else {
            world
                .entity_mut(target)
                .insert(ExtinguisherCabinet { stored, ..cabinet });
            toggle_extinguisher_cabinet(world, target);
        }
        return true;
    }
    if let Some(cabinet) = world.get::<FireaxeCabinet>(target).cloned() {
        if tool_of(world, held).is_some_and(|(kind, _)| kind == ToolKind::Multitool) {
            start_toggle_lock(world, client, mob, held, target);
            return true;
        }
        if !cabinet.open {
            return false;
        }
        let fits = !cabinet.item_path.is_empty()
            && item_path(world, held).is_some_and(|p| is_under(p, &cabinet.item_path));
        if fits && inside(world, target, cabinet.held).is_none() {
            crate::storage::put_inside(world, held, target);
            if let Some(mut c) = world.get_mut::<FireaxeCabinet>(target) {
                c.held = Some(held);
            }
            refresh_look(world, target);
        } else {
            toggle_open(world, client, target);
        }
        return true;
    }
    false
}

/// tg `extinguisher_cabinet/toggle_cabinet`.
fn toggle_extinguisher_cabinet(world: &mut World, cabinet: Entity) {
    click_sound(world, cabinet, 15.0, true);
    if let Some(mut c) = world.get_mut::<ExtinguisherCabinet>(cabinet) {
        c.opened = !c.opened;
    }
    refresh_look(world, cabinet);
}

/// tg `fireaxecabinet/toggle_open`.
fn toggle_open(world: &mut World, client: Entity, cabinet: Entity) {
    let Some(locked) = world.get::<FireaxeCabinet>(cabinet).map(|c| c.locked) else {
        return;
    };
    if locked {
        tell(world, client, Span::Warning, "won't budge!");
        return;
    }
    if let Some(mut c) = world.get_mut::<FireaxeCabinet>(cabinet) {
        c.open = !c.open;
    }
    click_sound(world, cabinet, 30.0, false);
    refresh_look(world, cabinet);
}

/// tg `fireaxecabinet/toggle_lock`: the message and sound, then the `do_after`.
fn start_toggle_lock(
    world: &mut World,
    client: Entity,
    mob: Entity,
    tool: Entity,
    cabinet: Entity,
) {
    tell(world, client, Span::Notice, "Resetting circuitry...");
    if let Some(&pos) = world.get::<TilePos>(cabinet) {
        playsound(world, PlaySound::new(pos, LOCK_TOGGLE, 50.0, true));
    }
    crate::tools::do_after(
        world,
        Actor {
            client,
            mob,
            item: Some(tool),
        },
        cabinet,
        LOCK_TOGGLE_DS,
        0,
        Job::WallMount(WallJob::ToggleLock),
    );
}

/// The end of `toggle_lock`'s `do_after`.
pub(super) fn finish_toggle_lock(world: &mut World, client: Entity, cabinet: Entity) {
    let Some(mut c) = world.get_mut::<FireaxeCabinet>(cabinet) else {
        return;
    };
    c.locked = !c.locked;
    let text = if !c.locked {
        "You disable the locking modules."
    } else {
        "You re-enable the locking modules."
    };
    tell(world, client, Span::Notice, text);
    refresh_look(world, cabinet);
}

/// tg `update_icon_state` / `update_overlays` of the cabinets (undamaged glass).
pub(super) fn look(world: &World, e: Entity) -> Option<Look> {
    if let Some(cabinet) = world.get::<ExtinguisherCabinet>(e) {
        let kind = inside(world, e, cabinet.stored)
            .and_then(|i| item_path(world, i))
            .map(|p| {
                ["mini", "advanced", "crafted"]
                    .into_iter()
                    .find(|k| is_under(p, &format!("{EXTINGUISHER}/{k}")))
                    .unwrap_or("default")
            });
        return Some(Look {
            icon_state: kind.map_or_else(
                || "extinguisher".to_owned(),
                |k| format!("extinguisher_{k}"),
            ),
            overlays: vec![(
                if cabinet.opened { "door_open" } else { "door" }.to_owned(),
                false,
            )],
            light: None,
        });
    }
    let cabinet = world.get::<FireaxeCabinet>(e)?;
    let dm = world.get::<DmWallMount>(e)?;
    let mut overlays = Vec::new();
    if inside(world, e, cabinet.held).is_some() {
        overlays.push((dm.item_overlay.clone(), false));
    }
    if cabinet.open {
        overlays.push(("glass_raised".to_owned(), false));
    } else {
        overlays.push(("glass".to_owned(), false));
        overlays.push((
            if cabinet.locked { "locked" } else { "unlocked" }.to_owned(),
            false,
        ));
    }
    let icon_state = world
        .get::<ss13_mapload::DmAtom>(e)
        .map_or_else(String::new, |a| a.icon_state.clone());
    Some(Look {
        icon_state,
        overlays,
        light: None,
    })
}
