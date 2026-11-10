//! Cameras carried by mobs (server), after tg `/datum/component/simple_bodycam`
//! (`code/datums/components/simple_bodycam.dm`), BandaStation's `/datum/component/pausable_bodycam`,
//! `/obj/machinery/camera/bodycam` (`modular_bandastation/~components/code/`) and its bodycam
//! accessory (`modular_bandastation/objects/code/items/clothing/accessories/bodycam.dm`), and the
//! broadcast camera's `internal_camera` (`crate::devices::broadcast_camera`).
//!
//! - The camera is an entity of its own with a [`Camera`] on its holder's turf (tg: a camera
//!   inside the mob). It follows the holder and the network updates its chunks like any moved
//!   camera, so consoles, telescreens and the AI see through it; it goes away with its holder.
//! - A simple bodycam ([`add_simple_bodycam`]: bitrunning avatars) is always in the network.
//! - The bodycam accessory pinned to a jumpsuit its wearer has on gives the wearer a pausable
//!   bodycam `"!<name> (Нательная камера)"` on `ss13`, view range 3, and a button that switches
//!   it on and off ("включена" / "выключена", "сломана!" when broken). Consoles list it, but it
//!   is in the network only while a console has it as its active camera and it is on; the
//!   wearer then has the "Нательная камера активна" alert. An EMP on the jumpsuit or on the
//!   accessory breaks it ("<name> искрит и гаснет!"); a length of cable on it (or on the
//!   jumpsuit) mends it. A console looking through it when it goes off loses it (BandaStation
//!   `on_camera_disabled`).
// shortcut: no bodycam vendor for subscribers (`prime_only`), no operative or battle royale
// bodycams (no such antagonists or implants yet), no `rotate_cam` (cameras have no direction).
// Cost: per tick, one turf lookup per mob camera (a handful) and one pass over the consoles for
// the watched pausable bodycams; a moving unpaused camera dirties its chunks every step (tg
// `camera_moved` every 0.5 s).

use std::collections::HashSet;

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;
use ss13_tgui::server::UiActed;

use super::cameras::{Camera, CameraConsole, CameraPaused, sync_camera};
use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::breath::{AlertCategory, Alerts};
use crate::clothing::accessories::{Accessories, AccessoryOf};
use crate::emp::{EmpAct, EmpProtection, Emped, PROTECT_ALL};
use crate::health::{Health, Stat};
use crate::inventory::{CarriedBy, Inventory};
use crate::server::PlayerMob;

pub const BODYCAM: &str = "/obj/item/clothing/accessory/bodycam";
const CABLE: &str = "/obj/item/stack/cable_coil";
/// `/obj/machinery/camera/bodycam` `view_range`.
const BODYCAM_RANGE: u8 = 3;
/// The toggle action's `button_icon`, `button_icon_state`.
const BUTTON_DMI: &str = "modular_bandastation/objects/icons/obj/clothing/accessories.dmi";
const BUTTON: &str = "bodycam:";

/// What a mob camera belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MobCameraKind {
    /// `simple_bodycam`: always in the network.
    Simple,
    /// `pausable_bodycam`: in the network only while watched; `source` is the accessory.
    Pausable,
    /// A live broadcast camera's `internal_camera`; `source` is the camera item.
    Broadcast,
}

/// On a camera carried by a mob: whose turf it follows.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MobCamera {
    pub holder: Entity,
    pub kind: MobCameraKind,
    pub source: Option<Entity>,
}

/// The bodycam accessory (`broken`, `camera_on`), its wearer and the camera it gave them.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct BodycamAccessory {
    pub broken: bool,
    pub on: bool,
    pub wearer: Option<Entity>,
    pub camera: Option<Entity>,
}

impl Default for BodycamAccessory {
    fn default() -> Self {
        Self {
            broken: false,
            on: true,
            wearer: None,
            camera: None,
        }
    }
}

pub struct MobCamerasPlugin;

impl Plugin for MobCamerasPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::HeldOnTarget, 2711, |w, c| {
            c.held.is_some_and(|h| repair(w, c.mob, h, c.target))
        });
        app.add_click_handler(ClickPhase::OtherHand, 481, |w, c| {
            c.held.is_some_and(|h| repair(w, c.mob, h, c.target))
        });
        app.add_observer(add_accessory).add_systems(
            Update,
            (
                (wear_bodycams, emp_bodycams, watch_bodycams, follow_holders)
                    .chain()
                    .after(ss13_world::WorldSystems::Movement),
                trigger_buttons.run_if(resource_exists::<Messages<UiActed<ActionButtonsUi>>>),
            ),
        );
    }
}

fn add_accessory(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if items
        .get(add.entity)
        .is_ok_and(|i| is_under(&i.path, BODYCAM))
    {
        commands
            .entity(add.entity)
            .try_insert_if_new(BodycamAccessory::default());
    }
}

/// A camera on `holder`'s turf (tg `new /obj/machinery/camera(parent)`). `None` when the holder
/// is nowhere.
#[allow(clippy::too_many_arguments)]
pub fn spawn_mob_camera(
    world: &mut World,
    holder: Entity,
    c_tag: String,
    network: Vec<String>,
    view_range: u8,
    enabled: bool,
    kind: MobCameraKind,
    source: Option<Entity>,
    emp_proof: bool,
) -> Option<Entity> {
    let pos = crate::explosions::turf_of(world, holder)?;
    let mut e = world.spawn((
        Camera {
            c_tag: c_tag.clone(),
            network: network.iter().map(|n| n.to_lowercase()).collect(),
            view_range,
            enabled,
        },
        MobCamera {
            holder,
            kind,
            source,
        },
        pos,
        Name::new(c_tag),
    ));
    if kind == MobCameraKind::Pausable {
        // `/obj/machinery/camera/bodycam/Initialize`: out of the chunks until watched.
        e.insert(CameraPaused);
    }
    if emp_proof {
        e.insert(EmpProtection(PROTECT_ALL));
    }
    let camera = e.id();
    sync_camera(world, camera);
    Some(camera)
}

/// tg `AddComponent(/datum/component/simple_bodycam, ...)`: none when `mob` already has a
/// bodycam on that network (`CheckDupeComponent`).
pub fn add_simple_bodycam(
    world: &mut World,
    mob: Entity,
    c_tag: String,
    network: &str,
    emp_proof: bool,
) -> Option<Entity> {
    let network = network.to_lowercase();
    let mut q = world.query::<(&MobCamera, &Camera)>();
    if q.iter(world).any(|(m, c)| {
        m.holder == mob && m.kind == MobCameraKind::Simple && c.network.contains(&network)
    }) {
        return None;
    }
    spawn_mob_camera(
        world,
        mob,
        c_tag,
        vec![network],
        7,
        true,
        MobCameraKind::Simple,
        None,
        emp_proof,
    )
}

/// Takes a mob camera out of the world (its chunks follow through `RemovedComponents`).
pub fn remove_mob_camera(world: &mut World, camera: Entity) {
    if let Some(holder) = world.get::<MobCamera>(camera).map(|m| m.holder) {
        set_viewed_alert(world, holder, false);
    }
    if world.get_entity(camera).is_ok() {
        world.despawn(camera);
    }
}

/// The living mob wearing the jumpsuit the accessory is pinned to.
fn wearer_of(world: &World, accessory: Entity) -> Option<Entity> {
    let uniform = world.get::<AccessoryOf>(accessory)?.0;
    let mob = world.get::<CarriedBy>(uniform)?.0;
    let worn = world.get::<Inventory>(mob)?.slot(InventorySlot::Uniform) == Some(uniform);
    (worn && world.get::<Health>(mob).is_some()).then_some(mob)
}

/// `accessory_equipped` / `accessory_dropped`: the wearer gets the bodycam and its button, the
/// one who took it off loses them.
fn wear_bodycams(world: &mut World) {
    let mut q = world.query::<(Entity, &BodycamAccessory)>();
    let accessories: Vec<(Entity, BodycamAccessory)> =
        q.iter(world).map(|(e, a)| (e, *a)).collect();
    for (accessory, mut state) in accessories {
        let wearer = wearer_of(world, accessory);
        if wearer == state.wearer {
            continue;
        }
        if let Some(old) = state.wearer {
            refresh_buttons(world, old);
        }
        if let Some(camera) = state.camera.take() {
            remove_mob_camera(world, camera);
        }
        state.wearer = wearer;
        if let Some(mob) = wearer {
            let name = crate::medical::name_of(world, mob);
            state.camera = spawn_mob_camera(
                world,
                mob,
                format!("!{name} (Нательная камера)"),
                vec!["ss13".to_owned()],
                BODYCAM_RANGE,
                state.on && !state.broken,
                MobCameraKind::Pausable,
                Some(accessory),
                false,
            );
        }
        world.entity_mut(accessory).insert(state);
        if let Some(mob) = wearer {
            refresh_buttons(world, mob);
        }
    }
}

/// tg `emp_act` of the accessory and its `on_emp` of the jumpsuit (`COMSIG_ATOM_EMP_ACT`): it
/// breaks.
fn emp_bodycams(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in crate::emp::emp_acts(world, &mut cursor) {
        let hit: Vec<Entity> = if world.get::<BodycamAccessory>(ev.target).is_some() {
            if ev.hits_self() {
                vec![ev.target]
            } else {
                Vec::new()
            }
        } else {
            world
                .get::<Accessories>(ev.target)
                .map(|a| {
                    a.items()
                        .iter()
                        .copied()
                        .filter(|&e| world.get::<BodycamAccessory>(e).is_some())
                        .collect()
                })
                .unwrap_or_default()
        };
        for accessory in hit {
            do_break(world, accessory);
        }
    }
}

/// `do_break`.
fn do_break(world: &mut World, accessory: Entity) {
    let Some(mut state) = world.get_mut::<BodycamAccessory>(accessory) else {
        return;
    };
    if state.broken {
        return;
    }
    state.broken = true;
    let wearer = state.wearer;
    crate::obj_damage::set_icon_state(world, accessory, "bodycamera_broken");
    let text = format!(
        "{} искрит и гаснет!",
        ss13_shared::capitalize(&crate::medical::name_of(world, accessory))
    );
    let speaker = wearer.unwrap_or(accessory);
    crate::medical::visible(world, speaker, &text, &text, Span::Warning);
}

/// The pausable bodycams: on while their accessory is on, whole and not EMPed; in the network
/// while a console has them as its active camera (`on_watch_start` / `on_watch_stop`); a
/// console loses one that goes off (`notify_watchers_disconnect`).
fn watch_bodycams(world: &mut World) {
    let mut consoles = world.query::<&CameraConsole>();
    let watched: HashSet<Entity> = consoles.iter(world).filter_map(|c| c.active).collect();
    let mut q = world.query::<(Entity, &MobCamera, &Camera, Has<CameraPaused>, Has<Emped>)>();
    let cams: Vec<(Entity, MobCamera, bool, bool, bool)> = q
        .iter(world)
        .filter(|(_, m, ..)| m.kind == MobCameraKind::Pausable)
        .map(|(e, m, c, paused, emped)| (e, *m, c.enabled, paused, emped))
        .collect();
    for (camera, mob_cam, enabled, paused, emped) in cams {
        let on = mob_cam
            .source
            .and_then(|a| world.get::<BodycamAccessory>(a))
            .is_some_and(|a| a.on && !a.broken)
            && !emped;
        if on != enabled
            && let Some(mut cam) = world.get_mut::<Camera>(camera)
        {
            cam.enabled = on;
        }
        if !on && watched.contains(&camera) {
            let mut q = world.query::<&mut CameraConsole>();
            for mut console in q.iter_mut(world) {
                if console.active == Some(camera) {
                    console.active = None;
                }
            }
        }
        let pause = !(on && watched.contains(&camera));
        if pause == paused && on == enabled {
            continue;
        }
        if pause {
            world.entity_mut(camera).insert(CameraPaused);
        } else {
            // It did not follow its wearer while paused.
            if let Some(at) = crate::explosions::turf_of(world, mob_cam.holder) {
                world.entity_mut(camera).insert(at);
            }
            world.entity_mut(camera).remove::<CameraPaused>();
        }
        sync_camera(world, camera);
        set_viewed_alert(world, mob_cam.holder, !pause);
    }
}

/// `throw_alert(ALERT_BODYCAM_VIEWED)` / `clear_alert`.
fn set_viewed_alert(world: &mut World, mob: Entity, on: bool) {
    let Some(mut alerts) = world.get_mut::<Alerts>(mob) else {
        return;
    };
    if on {
        alerts.throw(AlertCategory::BodycamViewed, "recording", 0);
    } else if alerts.get(AlertCategory::BodycamViewed).is_some() {
        alerts.clear(AlertCategory::BodycamViewed);
    }
}

/// Cameras follow their holders' turfs (tg `update_cam` on `COMSIG_MOVABLE_MOVED`); a paused
/// bodycam stays put; a camera whose holder or source is gone goes too (`camera_gone`).
fn follow_holders(world: &mut World) {
    let mut q = world.query::<(Entity, &MobCamera, &TilePos, Has<CameraPaused>)>();
    let cams: Vec<(Entity, Entity, Option<Entity>, TilePos, bool)> = q
        .iter(world)
        .map(|(e, m, &p, paused)| (e, m.holder, m.source, p, paused))
        .collect();
    for (camera, holder, source, pos, paused) in cams {
        // The holder, or the accessory or broadcast camera it came from, is gone.
        if world.get_entity(holder).is_err() || source.is_some_and(|s| world.get_entity(s).is_err())
        {
            remove_mob_camera(world, camera);
            continue;
        }
        if paused {
            continue;
        }
        if let Some(at) = crate::explosions::turf_of(world, holder)
            && at != pos
        {
            world.entity_mut(camera).insert(at);
        }
    }
}

/// The wearer's bodycam button after its other buttons (`toggle_action.Grant` / `Remove`).
fn refresh_buttons(world: &mut World, mob: Entity) {
    if world.get::<PlayerMob>(mob).is_none() {
        return;
    }
    let mut q = world.query::<&BodycamAccessory>();
    let wears = q.iter(world).any(|a| a.wearer == Some(mob));
    let current = world.get::<ActionButtons>(mob);
    let had = current.is_some_and(|b| b.0.iter().any(|(b, _)| b.id.starts_with(BUTTON)));
    if !wears && !had {
        return;
    }
    let mut buttons: Vec<(ActionButton, f64)> = current
        .map(|b| {
            b.0.iter()
                .filter(|(b, _)| !b.id.starts_with(BUTTON))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    if wears {
        let icon = world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(BUTTON_DMI, "bodycamera"));
        buttons.push((
            ActionButton {
                id: format!("{BUTTON}toggle"),
                name: "Вкл/выкл нательную камеру".to_owned(),
                icon,
                background: None,
                border: None,
                available: true,
                cooldown: 0.0,
                active: false,
                maptext: None,
            },
            0.0,
        ));
    }
    set_buttons(world, mob, buttons);
}

/// `ui_action_click`, checked at the moment of effect: the client's mob still wears it.
fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .filter(|a| a.act.starts_with(BUTTON))
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world.get::<Stat>(actor) != Some(&Stat::Conscious)
        {
            continue;
        }
        let mut q = world.query::<(Entity, &BodycamAccessory)>();
        let Some((accessory, state)) = q
            .iter(world)
            .find(|(_, a)| a.wearer == Some(actor))
            .map(|(e, a)| (e, *a))
        else {
            continue;
        };
        if state.broken {
            crate::balloon::balloon_alert(world, actor, actor, "сломана!");
            continue;
        }
        let on = !state.on;
        if let Some(mut s) = world.get_mut::<BodycamAccessory>(accessory) {
            s.on = on;
        }
        let text = if on {
            "включена"
        } else {
            "выключена"
        };
        crate::balloon::balloon_alert(world, actor, actor, text);
    }
}

/// `repair_with_cable` from the accessory's `attackby` or the jumpsuit's `item_interaction`:
/// one length of cable mends a broken bodycam. True when the click was this.
pub fn repair(world: &mut World, mob: Entity, held: Entity, target: Entity) -> bool {
    let cable = world
        .get::<DmItem>(held)
        .is_some_and(|i| is_under(&i.path, CABLE));
    if !cable {
        return false;
    }
    let accessory = if world.get::<BodycamAccessory>(target).is_some() {
        target
    } else {
        // The jumpsuit: its first broken bodycam, else not ours.
        let Some(found) = world.get::<Accessories>(target).and_then(|a| {
            a.items()
                .iter()
                .copied()
                .find(|&e| world.get::<BodycamAccessory>(e).is_some_and(|s| s.broken))
        }) else {
            return false;
        };
        found
    };
    let broken = world
        .get::<BodycamAccessory>(accessory)
        .is_some_and(|s| s.broken);
    if !broken {
        crate::balloon::balloon_alert(world, mob, target, "не сломана!");
        return true;
    }
    if !crate::tools::use_stack(world, held, 1) {
        crate::balloon::balloon_alert(world, mob, target, "нужен моток провода!");
        return true;
    }
    if let Some(mut s) = world.get_mut::<BodycamAccessory>(accessory) {
        s.broken = false;
    }
    crate::obj_damage::set_icon_state(world, accessory, "bodycamera");
    true
}

/// `examine` of the accessory.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(state) = world.get::<BodycamAccessory>(target) else {
        return;
    };
    if state.broken {
        lines.push((
            Span::Warning,
            "Сломана. Можно починить заменой проводки.".to_owned(),
        ));
    } else if !state.on {
        lines.push((Span::Notice, "Сейчас выключена.".to_owned()));
    }
}
