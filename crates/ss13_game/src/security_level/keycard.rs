//! The keycard authentication device (server), after tg `/obj/machinery/keycard_auth`
//! (`code/modules/security_levels/keycard_authentication.dm`): a head with `ACCESS_KEYCARD_AUTH`
//! picks an event on one device; every device then asks for a confirmation for 2 s, and another
//! head with the same access confirms it on another device (`auth_swipe`): red alert, emergency
//! maintenance access, or the bluespace artillery lock. The window is tgui `KeycardAuth`
//! ([`crate::keycard`]).
// The confirmer must be another mob than the one who sent the event: the device exists for two
// people (tg only relies on the sender's own device being busy for those 2 s).
// shortcut: no "Grant Janitor Access" (needs the janitor's key, `COMSIG_ON_DEPARTMENT_ACCESS`),
// no blue light while asking (overlays only), no silicon or animal users, no construction.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmName, DmObj};
use ss13_shared::{Appearance, EMISSIVE_PLANE, Overlay};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_tgui::{UiInterface as _, UiStatus};

use super::{SecLevel, SecurityLevel, StationAccess};
use crate::click::{ClickResolved, ResolveClicks};
use crate::inventory::{IdCard, Inventory, check_access};
use crate::keycard::{
    AUTH_SWIPE, BSA_UNLOCK, EMERGENCY_MAINT, KeycardAuthData, KeycardAuthUi, RED_ALERT,
};
use crate::power::Unpowered;

const KEYCARD_AUTH: &str = "/obj/machinery/keycard_auth";
/// tg `ACCESS_KEYCARD_AUTH`, the device's `req_access`.
pub const ACCESS_KEYCARD_AUTH: &str = "keycard_auth";
/// `addtimer(..., 2 SECONDS)` of `sendEvent` and `triggerEvent`.
pub const EVENT_SECS: f64 = 2.0;

/// tg `KEYCARD_*` events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeycardEvent {
    RedAlert,
    EmergencyMaintenanceAccess,
    BsaUnlock,
}

impl KeycardEvent {
    /// The `KEYCARD_*` text the confirming window shows.
    pub fn name(self) -> &'static str {
        match self {
            Self::RedAlert => "Red Alert",
            Self::EmergencyMaintenanceAccess => "Emergency Maintenance Access",
            Self::BsaUnlock => "Bluespace Artillery Unlock",
        }
    }
}

/// One device (tg `event`, `triggerer`, `waiting`, `event_source` and their 2 s timers).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct KeycardAuth {
    /// The event this device sent, who sent it, and until when it waits (`waiting`).
    pub sent: Option<(KeycardEvent, Entity, f64)>,
    /// The device asking for a confirmation, its event and until when (`event_source`).
    pub asked_by: Option<(Entity, KeycardEvent, f64)>,
}

pub(super) struct KeycardPlugin;

impl Plugin for KeycardPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<KeycardAuthUi>()
            .add_systems(ss13_world::MapInit, setup_devices)
            .add_systems(
                Update,
                (
                    open_device.after(ResolveClicks).before(UiSystems::Open),
                    device_acts.in_set(UiSystems::Handle),
                    unpowered_windows.in_set(UiSystems::StatusHook),
                    expire_events,
                ),
            );
    }
}

fn setup_devices(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let devices: Vec<Entity> = query
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, KEYCARD_AUTH))
        .map(|(e, _)| e)
        .collect();
    info!("{} keycard authentication devices", devices.len());
    for e in devices {
        world.entity_mut(e).insert(KeycardAuth::default());
    }
}

/// `attack_hand` -> `ui_interact` with an empty hand.
fn open_device(world: &mut World, mut cursor: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = cursor
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        if world.get::<KeycardAuth>(click.target).is_none()
            || world.get::<Unpowered>(click.target).is_some()
        {
            continue;
        }
        let empty_hand = world
            .get::<Inventory>(click.mob)
            .is_some_and(|i| i.active_item().is_none());
        if !empty_hand {
            continue;
        }
        let name = world
            .get::<DmName>(click.target)
            .map(|n| crate::vending::plain_name(&n.name).to_owned())
            .unwrap_or_default();
        world.write_message(OpenUi::new::<KeycardAuthUi>(
            click.client,
            click.mob,
            click.target,
            name,
        ));
    }
}

/// An unpowered device is update-only.
fn unpowered_windows(
    mut windows: Query<&mut UiWindow>,
    unpowered: Query<(), (With<KeycardAuth>, With<Unpowered>)>,
) {
    for mut window in &mut windows {
        if window.interface == KeycardAuthUi::NAME && unpowered.contains(window.source) {
            window.cap_status(UiStatus::Update);
        }
    }
}

impl ServerUi for KeycardAuthUi {
    type Source = &'static KeycardAuth;
    type UserParam = (Res<'static, SecurityLevel>, Res<'static, StationAccess>);
    type Act = KeycardEventAct;

    fn static_data(_: &KeycardAuth) {}

    fn data(
        device: &KeycardAuth,
        _: Entity,
        (level, access): &bevy::ecs::system::SystemParamItem<
            '_,
            '_,
            (Res<'static, SecurityLevel>, Res<'static, StationAccess>),
        >,
    ) -> KeycardAuthData {
        KeycardAuthData {
            waiting: device.sent.is_some(),
            auth_required: device.asked_by.map(|(_, event, _)| event.name().to_owned()),
            red_alert: level.current >= SecLevel::Red,
            emergency_maint: access.emergency_access,
            bsa_unlock: access.bsa_unlock,
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<KeycardEventAct, UiReject> {
        Ok(match action {
            RED_ALERT => KeycardEventAct::Send(KeycardEvent::RedAlert),
            EMERGENCY_MAINT => KeycardEventAct::Send(KeycardEvent::EmergencyMaintenanceAccess),
            BSA_UNLOCK => KeycardEventAct::Send(KeycardEvent::BsaUnlock),
            AUTH_SWIPE => KeycardEventAct::Confirm,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeycardEventAct {
    Send(KeycardEvent),
    Confirm,
}

/// tg `allowed(user)` against `ACCESS_KEYCARD_AUTH`: the ID in hand or worn.
fn allowed(world: &World, user: Entity) -> bool {
    let Some(card) = world.get::<Inventory>(user).and_then(|i| i.id_card(world)) else {
        return false;
    };
    world
        .get::<IdCard>(card)
        .is_some_and(|c| check_access(&[ACCESS_KEYCARD_AUTH.to_owned()], &[], &c.access))
}

/// tg `ui_act`, at the moment of effect: powered, not waiting, the user allowed.
fn device_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<KeycardAuthUi>>>) {
    let pending: Vec<(Entity, Entity, KeycardEventAct)> = acts
        .read(world.resource::<Messages<UiActed<KeycardAuthUi>>>())
        .map(|a| (a.actor, a.source, a.act))
        .collect();
    for (actor, device, act) in pending {
        let Some(state) = world.get::<KeycardAuth>(device).copied() else {
            continue;
        };
        if world.get::<Unpowered>(device).is_some()
            || state.sent.is_some()
            || !allowed(world, actor)
        {
            continue;
        }
        match act {
            KeycardEventAct::Send(event) if state.asked_by.is_none() => {
                send_event(world, device, event, actor);
            }
            KeycardEventAct::Send(_) => {}
            KeycardEventAct::Confirm => {
                let Some((source, ..)) = state.asked_by else {
                    continue;
                };
                confirm(world, device, source, actor);
            }
        }
    }
}

/// tg `sendEvent` -> `GLOB.keycard_events.fireEvent("triggerEvent", src)`: every device (this
/// one included, which is busy waiting) asks for a confirmation for 2 s.
pub fn send_event(world: &mut World, device: Entity, event: KeycardEvent, user: Entity) {
    let until = world.resource::<Time>().elapsed_secs_f64() + EVENT_SECS;
    if let Some(mut d) = world.get_mut::<KeycardAuth>(device) {
        d.sent = Some((event, user, until));
    }
    let mut query = world.query_filtered::<Entity, With<KeycardAuth>>();
    let devices: Vec<Entity> = query.iter(world).collect();
    for e in devices {
        if let Some(mut d) = world.get_mut::<KeycardAuth>(e) {
            d.asked_by = Some((device, event, until));
        }
        refresh_look(world, e);
    }
    info!("{user} sent keycard event {}", event.name());
}

/// `auth_swipe` -> `event_source.trigger_event(usr)`, the confirming device stops asking.
fn confirm(world: &mut World, device: Entity, source: Entity, confirmer: Entity) {
    if let Some(mut d) = world.get_mut::<KeycardAuth>(device) {
        d.asked_by = None;
    }
    refresh_look(world, device);
    let Some((event, triggerer, _)) = world.get::<KeycardAuth>(source).and_then(|s| s.sent) else {
        return;
    };
    if triggerer == confirmer {
        return;
    }
    info!(
        "{triggerer} triggered and {confirmer} confirmed keycard event {}",
        event.name()
    );
    match event {
        KeycardEvent::RedAlert => super::set_level(world, SecLevel::Red, true),
        KeycardEvent::EmergencyMaintenanceAccess => super::make_maint_all_access(world),
        KeycardEvent::BsaUnlock => super::toggle_bluespace_artillery(world),
    }
}

/// `eventSent` and `eventTriggered` after their 2 s.
fn expire_events(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &KeycardAuth)>();
    let due: Vec<Entity> = query
        .iter(world)
        .filter(|(_, d)| {
            d.sent.is_some_and(|(.., at)| now >= at) || d.asked_by.is_some_and(|(.., at)| now >= at)
        })
        .map(|(e, _)| e)
        .collect();
    for e in due {
        let Some(mut d) = world.get_mut::<KeycardAuth>(e) else {
            continue;
        };
        if d.sent.is_some_and(|(.., at)| now >= at) {
            d.sent = None;
        }
        if d.asked_by.is_some_and(|(.., at)| now >= at) {
            d.asked_by = None;
        }
        refresh_look(world, e);
    }
}

/// `update_overlays`: `auth_on` (and its emissive) while a device asks, when powered.
fn refresh_look(world: &mut World, device: Entity) {
    let asking = world
        .get::<KeycardAuth>(device)
        .is_some_and(|d| d.asked_by.is_some())
        && world.get::<Unpowered>(device).is_none();
    let overlays = if asking {
        let Some(icon) = world.get::<DmAtom>(device).map(|a| a.icon.clone()) else {
            return;
        };
        let Some(id) = world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(&icon, "auth_on"))
        else {
            return;
        };
        vec![
            Overlay::new(id),
            Overlay {
                plane: Some(EMISSIVE_PLANE),
                ..Overlay::new(id)
            },
        ]
    } else {
        Vec::new()
    };
    if let Some(mut appearance) = world.get_mut::<Appearance>(device)
        && appearance.overlays != overlays
    {
        appearance.overlays = overlays;
    }
}
