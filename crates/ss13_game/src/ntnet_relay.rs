//! The NTNet quantum relay (server): tg `/obj/machinery/ntnet_relay` (`relays.dm`) with tgui's
//! `NtnetRelay` window.
//!
//! NTNet is up while one relay is operational ([`find_functional_ntnet_relay`]): enabled, not
//! crashed by a DoS, powered and not broken. A DoS fills the relay's buffer ([`dos_attack`]);
//! it drains 0.5 per second, crashes the relay past `dos_capacity` (500) and lets it recover once
//! empty. The window toggles the relay and restarts a crashed one.
//!
//! The API for the modular computers is [`find_functional_ntnet_relay`] and [`dos_attack`]; the
//! NTNet logs (`SSmodular_computers.add_log`) are only `info!` lines here.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmName, DmObj};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use crate::obj_damage::Integrity;
use crate::power::Unpowered;
use crate::telecomms_ui::{NtnetRelayData, NtnetRelayUi, RESTART, TOGGLE};

pub const RELAY_PATH: &str = "/obj/machinery/ntnet_relay";

/// `relay_enabled`, `dos_failure`, `dos_overload`, `dos_capacity`, `dos_dissipate`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct NtnetRelay {
    pub enabled: bool,
    pub dos_failure: bool,
    pub dos_overload: f64,
    pub dos_capacity: f64,
    /// Per second.
    pub dos_dissipate: f64,
    /// Whether it was operational at the last `process`, for the icon.
    shown_operational: bool,
}

impl Default for NtnetRelay {
    fn default() -> Self {
        Self {
            enabled: true,
            dos_failure: false,
            dos_overload: 0.0,
            dos_capacity: 500.0,
            dos_dissipate: 0.5,
            shown_operational: true,
        }
    }
}

impl NtnetRelay {
    /// `process(seconds_per_tick)`'s DoS buffer: drains, crashes past capacity, recovers empty.
    pub fn tick(&mut self, secs: f64) {
        if self.dos_overload > 0.0 {
            self.dos_overload = (self.dos_overload - self.dos_dissipate * secs).max(0.0);
        }
        if self.dos_overload > self.dos_capacity && !self.dos_failure {
            self.dos_failure = true;
            info!("ntnet: quantum relay switched to overload recovery mode");
        }
        if self.dos_overload == 0.0 && self.dos_failure {
            self.dos_failure = false;
            info!("ntnet: quantum relay switched back to normal operation mode");
        }
    }
}

pub struct NtnetRelayPlugin;

impl Plugin for NtnetRelayPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.register_ui::<NtnetRelayUi>()
            .add_observer(add_relay)
            .add_systems(
                Update,
                (
                    process.run_if(any_with_component::<NtnetRelay>),
                    relay_acts.in_set(UiSystems::Handle),
                ),
            );
        app.add_click_handler(ClickPhase::EmptyHand, 3517, |w, c| {
            interact(w, c.client, c.mob, c.target)
        });
    }
}

fn add_relay(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    if objs
        .get(add.entity)
        .is_ok_and(|o| is_under(&o.path, RELAY_PATH))
    {
        commands
            .entity(add.entity)
            .try_insert(NtnetRelay::default());
    }
}

fn operational(relay: &NtnetRelay, unpowered: bool, integrity: Option<&Integrity>) -> bool {
    relay.enabled && !relay.dos_failure && !unpowered && integrity.is_none_or(|i| !i.broken)
}

/// `find_functional_ntnet_relay`: NTNet works while any relay is operational.
// shortcut: a scan of the relays (a station has one or two); cache it when callers poll per tick.
pub fn find_functional_ntnet_relay(world: &mut World) -> bool {
    world
        .query::<(&NtnetRelay, Has<Unpowered>, Option<&Integrity>)>()
        .iter(world)
        .any(|(r, unpowered, integrity)| operational(r, unpowered, integrity))
}

/// A DoS program's packets into `relay` (`ntnet_dos` `process`: `dos_overload += amount`).
/// `false` when it is not a relay.
pub fn dos_attack(world: &mut World, relay: Entity, amount: f64) -> bool {
    let Some(mut r) = world.get_mut::<NtnetRelay>(relay) else {
        return false;
    };
    r.dos_overload += amount.max(0.0);
    true
}

fn process(world: &mut World) {
    let secs = world.resource::<Time>().delta_secs_f64();
    let mut q = world.query::<(Entity, &mut NtnetRelay, Has<Unpowered>, Option<&Integrity>)>();
    let mut icons: Vec<(Entity, bool)> = Vec::new();
    for (e, mut relay, unpowered, integrity) in q.iter_mut(world) {
        relay.tick(secs);
        let on = operational(&relay, unpowered, integrity);
        if on != relay.shown_operational {
            relay.shown_operational = on;
            icons.push((e, on));
        }
    }
    for (e, on) in icons {
        // `update_icon_state`: `bus`, `bus_off`.
        crate::obj_damage::set_icon_state(world, e, if on { "bus" } else { "bus_off" });
    }
}

/// `ui_interact` while powered.
fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<NtnetRelay>(target).is_none() {
        return false;
    }
    if world.get::<Unpowered>(target).is_none() {
        let title = world.get::<DmName>(target).map_or_else(String::new, |n| {
            crate::vending::plain_name(&n.name).to_owned()
        });
        world.write_message(OpenUi::new::<NtnetRelayUi>(client, mob, target, title));
    }
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelayAct {
    Restart,
    Toggle,
}

impl ServerUi for NtnetRelayUi {
    type Source = &'static NtnetRelay;
    type UserParam = ();
    type Act = RelayAct;

    fn static_data(_: &NtnetRelay) {}

    fn data(r: &NtnetRelay, _: Entity, _: &()) -> NtnetRelayData {
        NtnetRelayData {
            enabled: r.enabled,
            dos_capacity: r.dos_capacity,
            dos_overload: r.dos_overload,
            dos_crashed: r.dos_failure,
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<RelayAct, UiReject> {
        match action {
            RESTART => Ok(RelayAct::Restart),
            TOGGLE => Ok(RelayAct::Toggle),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// `ui_act`.
pub fn apply_act(relay: &mut NtnetRelay, act: RelayAct) {
    match act {
        RelayAct::Restart => {
            relay.dos_overload = 0.0;
            relay.dos_failure = false;
            info!("ntnet: quantum relay manually restarted");
        }
        RelayAct::Toggle => {
            relay.enabled = !relay.enabled;
            info!("ntnet: quantum relay manually toggled to {}", relay.enabled);
        }
    }
}

fn relay_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<NtnetRelayUi>>>) {
    let pending: Vec<(Entity, RelayAct)> = acts
        .read(world.resource::<Messages<UiActed<NtnetRelayUi>>>())
        .map(|a| (a.source, a.act))
        .collect();
    for (relay, act) in pending {
        if let Some(mut r) = world.get_mut::<NtnetRelay>(relay) {
            apply_act(&mut r, act);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dos_crashes_the_relay_until_its_buffer_drains() {
        let mut world = World::new();
        assert!(!find_functional_ntnet_relay(&mut world));
        let relay = world.spawn(NtnetRelay::default()).id();
        assert!(find_functional_ntnet_relay(&mut world));
        assert!(dos_attack(&mut world, relay, 501.0));
        world.get_mut::<NtnetRelay>(relay).unwrap().tick(1.0);
        // 501 - 0.5 = 500.5 > 500: crashed, NTNet is down.
        assert!(world.get::<NtnetRelay>(relay).unwrap().dos_failure);
        assert!(!find_functional_ntnet_relay(&mut world));
        // Drains at 0.5/s: 1001 s to empty, then it recovers.
        world.get_mut::<NtnetRelay>(relay).unwrap().tick(1000.0);
        assert!(world.get::<NtnetRelay>(relay).unwrap().dos_failure);
        world.get_mut::<NtnetRelay>(relay).unwrap().tick(2.0);
        assert!(!world.get::<NtnetRelay>(relay).unwrap().dos_failure);
        assert!(find_functional_ntnet_relay(&mut world));
        // Window: toggled off is down; restart clears a crash.
        let mut r = world.get_mut::<NtnetRelay>(relay).unwrap();
        apply_act(&mut r, RelayAct::Toggle);
        assert!(!find_functional_ntnet_relay(&mut world));
        let mut r = world.get_mut::<NtnetRelay>(relay).unwrap();
        apply_act(&mut r, RelayAct::Toggle);
        r.dos_overload = 900.0;
        r.dos_failure = true;
        apply_act(&mut r, RelayAct::Restart);
        assert!(!r.dos_failure && r.dos_overload == 0.0);
        // Unpowered is down.
        world.entity_mut(relay).insert(Unpowered);
        assert!(!find_functional_ntnet_relay(&mut world));
    }
}
