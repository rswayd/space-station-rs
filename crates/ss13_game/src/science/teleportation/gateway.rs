//! Gateways (server): tg `/obj/machinery/gateway` with its `/datum/gateway_destination/gateway`
//! and `/obj/machinery/computer/gateway_control` (tgui `Gateway`).
//!
//! - Every gateway is a destination of the others (the station's `centerstation` is "Home
//!   Gateway", the rest "Unknown Gateway"). One is available while it is calibrated, unused and
//!   powered; otherwise the window says why.
//! - The control console links to the nearest gateway within 7 tiles (at first use and on
//!   "Linkup"). "Activate" opens the gateway to an available destination, which opens back to
//!   it if it is free (`activate`); "Deactivate" closes both. A gateway that loses power closes.
//! - Whatever bumps into an open gateway from the side it faces (`dir`) steps out in front of
//!   the other one, facing its way (`Transfer`: a plain move, no teleport checks).
//! - An away gateway needs no console: an empty hand opens it to the home gateway (closing the
//!   home gateway's other link first) or closes it. A multitool recalibrates a gateway.
//!
//! The port loads no away missions, so on the station maps the home gateway has no
//! destination ("No gateway nodes detected."); two gateways on one map link as above.
// shortcut: no point destinations of away missions (`/datum/gateway_destination/point`, no away
// levels), no round-start `wait`, no exile implant check (`/obj/item/implant/exile` is not
// ported), no portal view (`ByondUi` map) or light overlays, the gateway blocks only its own
// tile (tg: a 3×2 frame), no ghost click.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmObj};
use ss13_shared::chat::Span;
use ss13_shared::{Dir, Facing, TilePos};
use ss13_tgui::server::{ServerUi, UiActed, UiParams, UiReject};

use super::server::{is_obj, open, operational};
use super::{ACT_ACTIVATE, ACT_DEACTIVATE, ACT_LINKUP, GatewayData, GatewayDestination, GatewayUi};
use crate::obj_damage::is_under;
use crate::power::Unpowered;
use crate::tools::tell;

pub const GATEWAY: &str = "/obj/machinery/gateway";
pub const HOME: &str = "/obj/machinery/gateway/centerstation";
pub const AWAY: &str = "/obj/machinery/gateway/away";
pub const CONSOLE: &str = "/obj/machinery/computer/gateway_control";
/// `try_to_linkup`: `view(7)`.
const LINK_RANGE: u32 = 7;
const SND_OPEN: &str = "sound/machines/gateway/gateway_open.ogg";
const SND_CLOSE: &str = "sound/machines/gateway/gateway_close.ogg";
const SND_TRAVEL: &str = "sound/machines/gateway/gateway_travel.ogg";
const SND_CALIBRATED: &str = "sound/machines/gateway/gateway_calibrated.ogg";
const SHORT_RANGE_EXTRARANGE: f32 = -7.0;

/// A gateway's `target` (the gateway it is open to) and `calibrated`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gateway {
    pub target: Option<Entity>,
    pub calibrated: bool,
}

impl Default for Gateway {
    fn default() -> Self {
        Self {
            target: None,
            calibrated: true,
        }
    }
}

/// A control console's `G` (`None` inside: linked to nothing).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GatewayConsole(pub Option<Entity>);

/// What the console's window shows.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct GatewayView(pub GatewayData);

fn is_gateway(world: &World, e: Entity) -> bool {
    world.get_entity(e).is_ok() && is_obj(world, e, GATEWAY)
}

pub fn state(world: &World, gw: Entity) -> Gateway {
    world.get::<Gateway>(gw).copied().unwrap_or_default()
}

fn powered(world: &World, gw: Entity) -> bool {
    world.get::<Unpowered>(gw).is_none()
}

/// `destination_name`.
fn destination_name(world: &World, gw: Entity) -> &'static str {
    if is_obj(world, gw, HOME) {
        "Home Gateway"
    } else {
        "Unknown Gateway"
    }
}

fn facing(world: &World, gw: Entity) -> Dir {
    world
        .get::<DmAtom>(gw)
        .and_then(|a| Dir::from_bits(a.dir))
        .unwrap_or(Dir::South)
}

fn all_gateways(world: &mut World) -> Vec<Entity> {
    let mut out: Vec<Entity> = world
        .query::<(Entity, &DmObj)>()
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, GATEWAY))
        .map(|(e, _)| e)
        .collect();
    out.sort_unstable();
    out
}

/// `is_available()` of the destination `dest`.
fn is_available(world: &World, dest: Entity) -> bool {
    let s = state(world, dest);
    s.calibrated && s.target.is_none() && powered(world, dest)
}

/// `get_available_reason()`.
fn reason(world: &World, dest: Entity) -> &'static str {
    let s = state(world, dest);
    let mut why = "Unreachable";
    if !s.calibrated {
        why = "Exit gateway malfunction. Manual recalibration required.";
    }
    if s.target.is_some() {
        why = "Exit gateway in use.";
    }
    if !powered(world, dest) {
        why = "Exit gateway unpowered.";
    }
    why
}

fn ui_entry(world: &World, dest: Entity) -> GatewayDestination {
    GatewayDestination {
        r#ref: dest.to_bits().to_string(),
        name: destination_name(world, dest).to_owned(),
        available: is_available(world, dest),
        reason: reason(world, dest).to_owned(),
        timeout: None,
    }
}

fn play(world: &mut World, gw: Entity, sound: &'static str, volume: f32, extrarange: f32) {
    if let Some(&at) = world.get::<TilePos>(gw) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(at, sound, volume, true).extrarange(extrarange),
        );
    }
}

/// `activate(D)`: open to `dest`, which opens back if it is free.
pub fn activate(world: &mut World, gw: Entity, dest: Entity) {
    let mut s = state(world, gw);
    if !powered(world, gw) || s.target.is_some() || !is_gateway(world, dest) {
        return;
    }
    s.target = Some(dest);
    world.entity_mut(gw).insert(s);
    if state(world, dest).target.is_none() {
        activate(world, dest, gw);
    }
    play(world, gw, SND_OPEN, 140.0, 0.0);
}

/// `deactivate()`: closed, and the other side if it was open to this one.
pub fn deactivate(world: &mut World, gw: Entity) {
    let mut s = state(world, gw);
    let Some(dest) = s.target.take() else {
        return;
    };
    world.entity_mut(gw).insert(s);
    play(world, gw, SND_CLOSE, 140.0, 0.0);
    if is_gateway(world, dest) && state(world, dest).target == Some(gw) {
        deactivate(world, dest);
    }
}

/// `Transfer(AM)`: out in front of the destination, facing its way.
pub fn transfer(world: &mut World, gw: Entity, mover: Entity) {
    let Some(dest) = state(world, gw).target.filter(|&d| is_gateway(world, d)) else {
        return;
    };
    let Some(&at) = world.get::<TilePos>(dest) else {
        return;
    };
    let dir = facing(world, dest);
    world.entity_mut(mover).insert(at.step(dir));
    if world.get::<Facing>(mover).is_some() {
        world.entity_mut(mover).insert(Facing(dir));
    }
}

/// The portal bumper's `Bumped`: only from the side the gateway faces. True when `obstacle` is
/// a gateway.
pub fn bumped(world: &mut World, mover: Entity, obstacle: Entity) -> bool {
    if !is_obj(world, obstacle, GATEWAY) {
        return false;
    }
    if state(world, obstacle).target.is_none() {
        return true;
    }
    let (Some(&gw_at), Some(&mover_at)) =
        (world.get::<TilePos>(obstacle), world.get::<TilePos>(mover))
    else {
        return true;
    };
    if crate::mobs::ai::movement::dir_towards(gw_at, mover_at) == Some(facing(world, obstacle)) {
        play(world, obstacle, SND_TRAVEL, 70.0, SHORT_RANGE_EXTRARANGE);
        transfer(world, obstacle, mover);
    }
    true
}

/// `process()`: an unpowered gateway closes.
pub(super) fn process_gateways(world: &mut World) {
    let closing: Vec<Entity> = world
        .query_filtered::<(Entity, &Gateway), With<Unpowered>>()
        .iter(world)
        .filter(|(_, g)| g.target.is_some())
        .map(|(e, _)| e)
        .collect();
    for gw in closing {
        deactivate(world, gw);
    }
}

/// `multitool_act`: recalibration.
pub fn multitool_act(world: &mut World, client: Entity, gw: Entity) -> bool {
    if !is_obj(world, gw, GATEWAY) {
        return false;
    }
    let mut s = state(world, gw);
    if s.calibrated {
        tell(
            world,
            client,
            Span::Warning,
            "The gate is already calibrated, there is no work for you to do here.",
        );
        return true;
    }
    play(world, gw, SND_CALIBRATED, 80.0, SHORT_RANGE_EXTRARANGE);
    tell(
        world,
        client,
        Span::BoldNotice,
        "Recalibration successful!: This gate's systems have been fine tuned. Travel to this \
         gate will now be on target.",
    );
    s.calibrated = true;
    world.entity_mut(gw).insert(s);
    true
}

/// `try_to_linkup()`: the nearest gateway in range on the console's level.
fn try_to_linkup(world: &mut World, console: Entity) {
    let Some(&here) = world.get::<TilePos>(console) else {
        return;
    };
    let found = all_gateways(world)
        .into_iter()
        .filter_map(|g| {
            let d = here.chebyshev(*world.get::<TilePos>(g)?)?;
            (d <= LINK_RANGE).then_some((d, g))
        })
        .min_by_key(|&(d, _)| d)
        .map(|(_, g)| g);
    world.entity_mut(console).insert(GatewayConsole(found));
}

fn linked(world: &mut World, console: Entity) -> Option<Entity> {
    if world.get::<GatewayConsole>(console).is_none() {
        try_to_linkup(world, console);
    }
    world
        .get::<GatewayConsole>(console)
        .and_then(|c| c.0)
        .filter(|&g| is_gateway(world, g))
}

/// The destinations `gw` may open to (`valid_destination`): every other gateway.
fn destinations(world: &mut World, gw: Entity) -> Vec<Entity> {
    all_gateways(world)
        .into_iter()
        .filter(|&g| g != gw)
        .collect()
}

/// `ui_data`.
pub(super) fn view(world: &mut World, console: Entity) -> Option<GatewayView> {
    if !is_obj(world, console, CONSOLE) {
        return None;
    }
    let Some(gw) = linked(world, console) else {
        return Some(GatewayView(GatewayData::default()));
    };
    let current_target = state(world, gw)
        .target
        .filter(|&d| is_gateway(world, d))
        .map(|d| ui_entry(world, d));
    let destinations = destinations(world, gw)
        .into_iter()
        .map(|d| ui_entry(world, d))
        .collect();
    Some(GatewayView(GatewayData {
        gateway_present: true,
        gateway_status: powered(world, gw),
        current_target,
        destinations,
    }))
}

/// `attack_hand`: the console's window; an away gateway links to home or closes.
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if is_obj(world, target, CONSOLE) {
        if operational(world, target)
            && let Some(v) = view(world, target)
        {
            open::<GatewayUi, _>(world, client, mob, target, v);
        }
        return true;
    }
    if !is_obj(world, target, AWAY) {
        return false;
    }
    if state(world, target).target.is_some() {
        deactivate(world, target);
        return true;
    }
    let home = all_gateways(world)
        .into_iter()
        .find(|&g| is_obj(world, g, HOME));
    let Some(home) = home else {
        tell(
            world,
            client,
            Span::Warning,
            "Home gateway is not responding!",
        );
        return true;
    };
    if state(world, home).target.is_some() {
        deactivate(world, home);
    }
    activate(world, target, home);
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GatewayAct {
    Linkup,
    Activate(Entity),
    Deactivate,
}

impl ServerUi for GatewayUi {
    type Source = &'static GatewayView;
    type UserParam = ();
    type Act = GatewayAct;

    fn static_data(_: &GatewayView) {}

    fn data(view: &GatewayView, _: Entity, _: &()) -> GatewayData {
        view.0.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<GatewayAct, UiReject> {
        match action {
            ACT_LINKUP => Ok(GatewayAct::Linkup),
            ACT_DEACTIVATE => Ok(GatewayAct::Deactivate),
            ACT_ACTIVATE => params
                .get("destination")
                .and_then(|k| k.trim().parse::<u64>().ok())
                .and_then(Entity::try_from_bits)
                .map(GatewayAct::Activate)
                .ok_or_else(|| UiReject::BadParams("destination: expected a gateway".into())),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// `ui_act`: the destination must be one of the linked gateway's (`locate(...) in
/// GLOB.gateway_destinations`) and available.
pub(super) fn console_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<GatewayUi>>>) {
    let pending: Vec<(Entity, GatewayAct)> = acts
        .read(world.resource::<Messages<UiActed<GatewayUi>>>())
        .map(|a| (a.source, a.act))
        .collect();
    for (console, act) in pending {
        if !is_obj(world, console, CONSOLE) || !operational(world, console) {
            continue;
        }
        match act {
            GatewayAct::Linkup => try_to_linkup(world, console),
            GatewayAct::Activate(dest) => {
                let Some(gw) = linked(world, console) else {
                    continue;
                };
                if destinations(world, gw).contains(&dest)
                    && is_available(world, dest)
                    && state(world, gw).target.is_none()
                {
                    activate(world, gw, dest);
                }
            }
            GatewayAct::Deactivate => {
                if let Some(gw) = linked(world, console) {
                    deactivate(world, gw);
                }
            }
        }
    }
}
