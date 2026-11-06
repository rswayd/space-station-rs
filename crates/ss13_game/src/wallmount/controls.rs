//! Buttons and the controllers inside them (tg `buttons.dm` `attempt_press`, `setup_device`;
//! `doorcontrol.dm` `activate`), and mounted flashers (`flasher.dm` `flash`).
//!
//! A press (an empty hand, or an item out of combat mode) needs power and the button's access
//! (its `req_access`/`req_one_access`, the presser's ID); it flicks the success or error light
//! for a second and pulses the controller, which acts on everything sharing its `id` and then
//! cools down:
//! - blast door controller (door buttons): opens or closes the blast doors and shutters, all
//!   following the first one (`sync_doors`), 1 s;
//! - airlock controller (`normaldoorcontrol`): per `specialfunctions` bit opens/closes the
//!   airlocks with that `id_tag` (all close if any is open), toggles their ID scan, bolts
//!   (powered, bolt wire intact), electrification and safety, 1 s;
//! - mass driver controller: opens the blast doors, drives the mass drivers 1 s later
//!   ([`super::machines::drive`]), closes the doors 6 s after that, 8 s;
//! - ignition controller: sparks the sparkers and toggles the igniters
//!   ([`super::machines`]), 3 s;
//! - flasher controller: fires the mounted flashers, 5 s.
//!
//! A mounted flasher (`flash()`), powered, with a working bulb and off its 15 s cooldown, may
//! burn its bulb out (`flash_recharge(30)`: 3% per recent use), then flashes every living mob
//! that sees it within `flash_range`: the bulb's targeted `flash_mob` (from the flasher, no
//! user; `strength * 1.5` confusion) and `strength` of paralysis on success.
// shortcut: curtain, crematorium, photobooth and ticket buttons do nothing; no panel (devices and boards in and
// out, renaming, skins), no shuttle id prefixes (`connect_to_shuttle`), the airlock controller's
// shock bit sets the intended toggle (tg's `secondsElectrified` check is inverted). Flashers:
// no bulb removal or replacement (wirecutters), no flick or flash light, no portable flasher.

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::light::DmLight;
use ss13_mapload::wallmount::DOOR_BUTTON_ROOT;
use ss13_mapload::{DmDoor, DmName, DmWallMount, DoorKind};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::{Density, SpatialIndex};

use super::{Look, light_of, refresh_look};
use crate::doors::{Door, with_door};
use crate::health::Health;
use crate::inventory::{IdCard, Inventory, check_access};
use crate::power::Unpowered;
use crate::security::{FlashSource, flash_mob_from};
use crate::sound::{PlaySound, playsound};
use crate::status_effects::Incap;
use crate::tools::tell;
use crate::wires::{Electrified, WIRE_BOLTS, Wires};

/// `specialfunctions` bits (`code/__DEFINES/assemblies.dm`).
pub const OPEN: u8 = 1;
pub const IDSCAN: u8 = 2;
pub const BOLTS: u8 = 4;
pub const SHOCK: u8 = 8;
pub const SAFE: u8 = 16;
/// `flick_overlay_view(..., 1 SECONDS)` of a press.
const FLICK_SECS: f64 = 1.0;
const MASSDRIVER: &str = "/obj/item/assembly/control/massdriver";
const IGNITER_CONTROL: &str = "/obj/item/assembly/control/igniter";
const FLASHER_CONTROL: &str = "/obj/item/assembly/control/flasher";
const BLAST_DOOR_CONTROL: &str = "/obj/item/assembly/control/blast_door";
const AIRLOCK_CONTROL: &str = "/obj/item/assembly/control/airlock";
/// `/obj/machinery/flasher`: `flash_cooldown_duration`, `flash_recharge(30)`, `use_energy`.
const FLASH_COOLDOWN: f64 = 15.0;
const FLASH_RECHARGE: f64 = 30.0;
const FLASH_ENERGY: f64 = 1000.0;
const FLASH_SOUND: &str = "sound/items/weapons/flash.ogg";

/// The `assembly/control` in a button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    BlastDoor {
        sync: bool,
    },
    Airlock {
        functions: u8,
    },
    MassDriver,
    Igniter,
    Flasher,
    /// A controller this server does not run (curtain, crematorium, ...).
    Other,
}

impl Device {
    /// The controller's `cooldown` after `activate()`.
    fn cooldown(self) -> f64 {
        match self {
            Self::BlastDoor { .. } | Self::Airlock { .. } => 1.0,
            // Open, 1 s, drive, 6 s, close, 1 s.
            Self::MassDriver => 8.0,
            Self::Igniter => 3.0,
            Self::Flasher => 5.0,
            Self::Other => 0.0,
        }
    }
}

/// `/obj/machinery/button` with its `device` and access.
#[derive(Component, Clone, Debug)]
pub struct Button {
    pub device: Option<Device>,
    /// The device's `id` (the button's, `setup_device`); `None` controls nothing.
    pub id: Option<String>,
    pub req_access: Vec<String>,
    pub req_one_access: Vec<String>,
    /// The device's `cooldown` ends.
    pub ready_at: f64,
    /// The `-overlay-success` (true) or `-overlay-error` flick and its end.
    pub flick: Option<(bool, f64)>,
    /// A mass driver controller's drive (`activate_stage2`) and closing of its blast doors.
    pub drive_at: Option<f64>,
    pub close_at: Option<f64>,
}

/// A button with a flick or a timer running; [`run_timers`] watches only these.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ButtonPending;

/// `/obj/machinery/flasher` with its bulb (`bulb.burnt_out`, `times_used`, `last_used`).
#[derive(Component, Clone, Debug)]
pub struct Flasher {
    pub id: Option<String>,
    pub range: u32,
    /// `strength`, seconds of paralysis.
    pub strength: f32,
    pub ready_at: f64,
    pub burnt_out: bool,
    pub times_used: u32,
    pub last_used: f64,
}

/// Who listens on which `id`: blast doors (`poddoor.id`), airlocks (`id_tag`), flashers, mass
/// drivers, igniters and sparkers. Built
/// at startup; entries are checked to still be what they were at the moment of effect.
// shortcut: doors and flashers exist only from map load; register built ones when
// construction makes them.
#[derive(Resource, Default, Debug)]
pub struct Links {
    pub poddoors: HashMap<String, Vec<Entity>>,
    pub airlocks: HashMap<String, Vec<Entity>>,
    pub flashers: HashMap<String, Vec<Entity>>,
    pub mass_drivers: HashMap<String, Vec<Entity>>,
    pub igniters: HashMap<String, Vec<Entity>>,
    pub sparkers: HashMap<String, Vec<Entity>>,
}

/// Registers `e` on `id` in one of the [`Links`] lists.
pub(super) fn link(
    world: &mut World,
    list: impl FnOnce(&mut Links) -> &mut HashMap<String, Vec<Entity>>,
    id: &Option<String>,
    e: Entity,
) {
    if let Some(id) = id {
        let mut links = world.resource_mut::<Links>();
        list(&mut links).entry(id.clone()).or_default().push(e);
    }
}

fn linked(
    world: &World,
    list: impl FnOnce(&Links) -> &HashMap<String, Vec<Entity>>,
    id: &str,
) -> Vec<Entity> {
    list(world.resource::<Links>())
        .get(id)
        .cloned()
        .unwrap_or_default()
}

pub(super) fn init_links(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &Door, &DmDoor), With<ss13_world::Mapped>>();
    let mut links = world.remove_resource::<Links>().unwrap_or_default();
    for (e, door, dm) in query.iter(world) {
        match door.kind {
            DoorKind::Poddoor if !dm.id.is_empty() => {
                links.poddoors.entry(dm.id.clone()).or_default().push(e);
            }
            DoorKind::Airlock if !dm.id_tag.is_empty() => {
                links.airlocks.entry(dm.id_tag.clone()).or_default().push(e);
            }
            _ => {}
        }
    }
    world.insert_resource(links);
}

pub(super) fn id_of(dm: &DmWallMount) -> Option<String> {
    (!dm.id.is_empty()).then(|| dm.id.clone())
}

/// tg `button/Initialize(mapload)` -> `setup_device`.
pub(super) fn add_button(world: &mut World, e: Entity, dm: &DmWallMount) {
    let device = if is_under(&dm.path, DOOR_BUTTON_ROOT) {
        Some(if dm.normaldoorcontrol {
            Device::Airlock {
                functions: dm.specialfunctions,
            }
        } else {
            Device::BlastDoor {
                sync: dm.sync_doors,
            }
        })
    } else if dm.device_type.is_empty() {
        None
    } else if is_under(&dm.device_type, MASSDRIVER) {
        Some(Device::MassDriver)
    } else if is_under(&dm.device_type, IGNITER_CONTROL) {
        Some(Device::Igniter)
    } else if is_under(&dm.device_type, FLASHER_CONTROL) {
        Some(Device::Flasher)
    } else if is_under(&dm.device_type, BLAST_DOOR_CONTROL) {
        Some(Device::BlastDoor { sync: true })
    } else if is_under(&dm.device_type, AIRLOCK_CONTROL) {
        Some(Device::Airlock { functions: OPEN })
    } else {
        Some(Device::Other)
    };
    world.entity_mut(e).insert(Button {
        device,
        id: id_of(dm),
        req_access: dm.req_access.clone(),
        req_one_access: dm.req_one_access.clone(),
        ready_at: 0.0,
        flick: None,
        drive_at: None,
        close_at: None,
    });
}

pub(super) fn add_flasher(world: &mut World, e: Entity, dm: &DmWallMount) {
    let id = id_of(dm);
    link(world, |l| &mut l.flashers, &id, e);
    world.entity_mut(e).insert(Flasher {
        id,
        range: dm.flash_range.max(0.0) as u32,
        strength: dm.strength / 10.0,
        ready_at: 0.0,
        burnt_out: false,
        times_used: 0,
        last_used: f64::NEG_INFINITY,
    });
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

/// The access of the ID `mob` holds or wears (tg `allowed()` -> `GetID`).
fn access_of(world: &World, mob: Entity) -> Vec<String> {
    world
        .get::<Inventory>(mob)
        .and_then(|i| i.id_card(world))
        .and_then(|card| world.get::<IdCard>(card))
        .map(|card| card.access.clone())
        .unwrap_or_default()
}

/// tg `base_item_interaction`: out of combat mode, an item presses the button.
// shortcut: tools press it too (no panel to screw open); `NOBLUDGEON` items are not told apart.
pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    _held: Entity,
    target: Entity,
) -> bool {
    if world.get::<Button>(target).is_none()
        || world
            .get::<crate::combat::CombatMode>(mob)
            .is_some_and(|c| c.0)
    {
        return false;
    }
    attempt_press(world, client, mob, target);
    true
}

/// tg `button/attempt_press(user)`. True when it pressed.
pub fn attempt_press(world: &mut World, client: Entity, mob: Entity, button: Entity) -> bool {
    let Some(state) = world.get::<Button>(button).cloned() else {
        return false;
    };
    if world.get::<Unpowered>(button).is_some() {
        return false;
    }
    let t = now(world);
    let allowed = check_access(
        &state.req_access,
        &state.req_one_access,
        &access_of(world, mob),
    );
    if !allowed {
        tell(world, client, Span::Warning, "access denied");
        flick(world, button, false, t);
        return false;
    }
    crate::power::machine_use_energy(world, button, 5.0, false);
    flick(world, button, true, t);
    if let (Some(device), Some(id)) = (state.device, state.id.clone())
        && t >= state.ready_at
    {
        if let Some(mut b) = world.get_mut::<Button>(button) {
            b.ready_at = t + device.cooldown();
        }
        activate(world, button, device, &id, t);
    }
    true
}

fn flick(world: &mut World, button: Entity, success: bool, t: f64) {
    if let Some(mut b) = world.get_mut::<Button>(button) {
        b.flick = Some((success, t + FLICK_SECS));
    }
    world.entity_mut(button).insert(ButtonPending);
    refresh_look(world, button);
}

/// The controller's `activate()`.
fn activate(world: &mut World, button: Entity, device: Device, id: &str, t: f64) {
    match device {
        Device::BlastDoor { sync } => blast_doors(world, id, sync),
        Device::Airlock { functions } => airlocks(world, id, functions),
        Device::MassDriver => {
            move_poddoors(world, id, true);
            if let Some(mut b) = world.get_mut::<Button>(button) {
                b.drive_at = Some(t + 1.0);
                b.close_at = Some(t + 7.0);
            }
            world.entity_mut(button).insert(ButtonPending);
        }
        Device::Igniter => {
            for sparker in linked(world, |l| &l.sparkers, id) {
                super::machines::ignite(world, sparker);
            }
            for igniter in linked(world, |l| &l.igniters, id) {
                super::machines::toggle_igniter(world, igniter);
            }
        }
        Device::Flasher => {
            for f in linked(world, |l| &l.flashers, id) {
                flash(world, f);
            }
        }
        Device::Other => {}
    }
}

fn poddoors(world: &World, id: &str) -> Vec<Entity> {
    world
        .resource::<Links>()
        .poddoors
        .get(id)
        .into_iter()
        .flatten()
        .copied()
        .filter(|&e| {
            world
                .get::<Door>(e)
                .is_some_and(|d| d.kind == DoorKind::Poddoor)
        })
        .collect()
}

/// `blast_door/activate`: each door opens if it is dense, else closes; with `sync_doors` all
/// follow the first.
pub fn blast_doors(world: &mut World, id: &str, sync: bool) {
    let mut open: Option<bool> = None;
    for door in poddoors(world, id) {
        let dense = world.get::<Density>(door).is_some();
        if open.is_none() || !sync {
            open = Some(dense);
        }
        move_poddoor(world, door, open.unwrap_or(dense));
    }
}

fn move_poddoors(world: &mut World, id: &str, open: bool) {
    for door in poddoors(world, id) {
        move_poddoor(world, door, open);
    }
}

fn move_poddoor(world: &mut World, door: Entity, open: bool) {
    with_door(world, door, |d, appearance, ctx| {
        let moved = if open {
            ctx.open(d, appearance, false)
        } else {
            ctx.close(d, appearance, false)
        };
        ((), moved)
    });
}

/// `airlock/activate` with `specialfunctions`.
pub fn airlocks(world: &mut World, id: &str, functions: u8) {
    let doors: Vec<Entity> = world
        .resource::<Links>()
        .airlocks
        .get(id)
        .into_iter()
        .flatten()
        .copied()
        .filter(|&e| {
            world
                .get::<Door>(e)
                .is_some_and(|d| d.kind == DoorKind::Airlock)
        })
        .collect();
    let mut need_closing = false;
    for &door in &doors {
        if functions & OPEN != 0 && world.get::<Density>(door).is_none() {
            need_closing = true;
        }
        if functions & IDSCAN != 0
            && let Some(mut d) = world.get_mut::<Door>(door)
        {
            d.ai_disabled_id_scanner = !d.ai_disabled_id_scanner;
        }
        if functions & BOLTS != 0 {
            let powered = world.get::<Unpowered>(door).is_none();
            let cut = world
                .get::<Wires>(door)
                .is_some_and(|w| w.is_cut(WIRE_BOLTS));
            if let Some(locked) = world
                .get::<Door>(door)
                .filter(|d| !cut && d.has_power(powered))
                .map(|d| d.locked)
            {
                crate::wires::set_bolts(world, door, !locked);
            }
        }
        if functions & SHOCK != 0 {
            let shocked = world.get::<Electrified>(door).is_some();
            crate::wires::set_electrified(
                world,
                door,
                (!shocked).then_some(Electrified::Permanent),
            );
        }
        if functions & SAFE != 0
            && let Some(mut d) = world.get_mut::<Door>(door)
        {
            d.safe = !d.safe;
        }
    }
    if functions & OPEN == 0 {
        return;
    }
    for door in doors {
        with_door(world, door, |d, appearance, ctx| {
            let moved = if need_closing {
                ctx.close(d, appearance, false)
            } else {
                ctx.open(d, appearance, false)
            };
            ((), moved)
        });
    }
}

/// Flick ends, mass driver drives and closings.
pub(super) fn run_timers(world: &mut World) {
    let t = now(world);
    let mut query = world.query_filtered::<(Entity, &Button), With<ButtonPending>>();
    let pending: Vec<(Entity, Button)> = query.iter(world).map(|(e, b)| (e, b.clone())).collect();
    for (e, b) in pending {
        let mut b = b;
        let mut changed = false;
        if b.flick.is_some_and(|(_, until)| t >= until) {
            b.flick = None;
            changed = true;
        }
        if b.drive_at.is_some_and(|at| t >= at) {
            b.drive_at = None;
            if let Some(id) = b.id.as_deref() {
                for driver in linked(world, |l| &l.mass_drivers, id) {
                    super::machines::drive(world, driver);
                }
            }
        }
        if b.close_at.is_some_and(|at| t >= at) {
            b.close_at = None;
            if let Some(id) = b.id.as_deref() {
                move_poddoors(world, id, false);
            }
        }
        let done = b.flick.is_none() && b.drive_at.is_none() && b.close_at.is_none();
        let (flick, drive_at, close_at) = (b.flick, b.drive_at, b.close_at);
        if let Some(mut live) = world.get_mut::<Button>(e) {
            live.flick = flick;
            live.drive_at = drive_at;
            live.close_at = close_at;
        }
        if done {
            world.entity_mut(e).remove::<ButtonPending>();
        }
        if changed {
            refresh_look(world, e);
        }
    }
}

/// tg `button/update_icon_state`, `update_overlays`, `update_appearance` (panel closed).
pub(super) fn button_look(world: &World, e: Entity, button: &Button, powered: bool) -> Look {
    let dm = world.get::<DmWallMount>(e);
    let base = dm.map_or("button", |d| d.base_icon_state.as_str());
    let skin = dm.map_or("", |d| d.skin.as_str());
    let mut overlays = Vec::new();
    if let Some((success, _)) = button.flick {
        let kind = if success { "success" } else { "error" };
        overlays.push((format!("{base}-overlay-{kind}"), false));
    }
    if !powered {
        return Look {
            icon_state: format!("{base}{skin}-nopower"),
            overlays,
            light: None,
        };
    }
    overlays.push((format!("{base}-light-mask"), true));
    let light = world.get::<DmLight>(e).cloned().unwrap_or_default();
    let color = light.light_color.map_or([255; 3], |[r, g, b, _]| [r, g, b]);
    Look {
        icon_state: format!("{base}{skin}"),
        overlays,
        light: light_of(world, e, light.light_range, light.light_power, color),
    }
}

/// tg `flasher/update_icon_state`.
pub(super) fn flasher_look(world: &World, e: Entity, flasher: &Flasher, powered: bool) -> Look {
    let base = world
        .get::<DmWallMount>(e)
        .map_or("mflash", |d| d.base_icon_state.as_str());
    let dark = if flasher.burnt_out || !powered {
        "-p"
    } else {
        ""
    };
    Look {
        icon_state: format!("{base}1{dark}"),
        overlays: Vec::new(),
        light: None,
    }
}

/// tg `/obj/machinery/flasher/proc/flash()`. True when it flashed.
pub fn flash(world: &mut World, flasher: Entity) -> bool {
    let (Some(state), Some(&at)) = (
        world.get::<Flasher>(flasher).cloned(),
        world.get::<TilePos>(flasher),
    ) else {
        return false;
    };
    let t = now(world);
    if world.get::<Unpowered>(flasher).is_some() || state.burnt_out || t < state.ready_at {
        return false;
    }
    // `bulb.flash_recharge(30)`: a charge back per 30 s, then the burnout roll.
    let recharged = ((t - state.last_used) / FLASH_RECHARGE).floor().max(0.0);
    let times_used = state
        .times_used
        .saturating_sub(recharged.min(f64::from(u32::MAX)) as u32);
    let burn = world
        .resource_mut::<crate::combat::CombatRng>()
        .chance(times_used as f32 * 3.0);
    if let Some(mut f) = world.get_mut::<Flasher>(flasher) {
        f.times_used = times_used;
        f.last_used = t;
        f.burnt_out = burn;
    }
    if burn {
        refresh_look(world, flasher);
        return false;
    }
    playsound(world, PlaySound::new(at, FLASH_SOUND, 100.0, true));
    if let Some(mut f) = world.get_mut::<Flasher>(flasher) {
        f.ready_at = t + FLASH_COOLDOWN;
    }
    crate::power::machine_use_energy(world, flasher, FLASH_ENERGY, false);
    // `viewers(src)` within `flash_range`.
    let victims: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(at, state.range)
        .filter(|&m| world.get::<Health>(m).is_some())
        .filter(|&m| {
            world.get::<TilePos>(m).is_some_and(|&p| {
                p == at || crate::mobs::ai::behaviors::can_see(world, at, p, state.range)
            })
        })
        .collect();
    let source = FlashSource {
        user: None,
        at,
        name: world.get::<DmName>(flasher).map_or_else(String::new, |n| {
            crate::vending::plain_name(&n.name).to_owned()
        }),
    };
    let mut flashed = false;
    for victim in victims {
        if flash_mob_from(world, &source, victim, state.strength * 1.5, true) {
            crate::status_effects::apply(world, victim, Incap::Paralyze, state.strength);
            flashed = true;
        }
    }
    if flashed && let Some(mut f) = world.get_mut::<Flasher>(flasher) {
        f.times_used += 1;
    }
    true
}
