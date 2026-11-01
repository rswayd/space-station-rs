//! The APC's window (server), after tg `/obj/machinery/power/apc` `ui_interact`, `ui_data` and
//! `ui_act` (`code/modules/power/apc/apc_main.dm`), tgui `Apc`.
//!
//! An empty-handed click (tg `attack_hand` with the cover closed) or a silicon's `attack_ai`
//! opens it. Acts are refused for a silicon while the AI wire disabled the APC (`can_use`), and
//! for anyone else while the interface is locked, except "toggle_nightshift" and during a power
//! failure (tg). Acts: the breaker, the charge mode, the three channels (`setsubsystem`), the
//! silicon interface lock, the cover bolts, emergency lighting, night shift, reboot.
// shortcut: no ID swipe to unlock the interface, no malfunction (hack/occupy), no remote
// control console; "overload" (silicons) does nothing; the cover bolts, emergency-lighting
// and night-shift switches are kept and shown but do not change the cover, the lights' backup
// cells or their brightness yet.

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::DmName;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::apc::{ApcData, ApcUi, PowerChannel};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::MACHINE_TICK_SECS;
use super::model::{Apc, CHANNEL_AUTO_OFF, CHANNEL_OFF, Channel, NOT_CHARGING};
use crate::silicon::interact::{SiliconUsers, has_silicon_access};

pub struct ApcWindow;

impl UiInterface for ApcWindow {
    const NAME: &'static str = ApcUi::NAME;
    type Data = ApcData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ApcAct {
    Lock,
    Cover,
    Breaker,
    Nightshift,
    Charge,
    /// A channel and the requested `APC_CHANNEL_*` (1 off, 2 on, 3 auto).
    Channel(Channel, u8),
    Overload,
    Reboot,
    EmergencyLighting,
}

/// tg `siunit(value, unit, maxdecimals)`: an SI prefix and up to `maxdecimals` decimals, DM
/// number printing (no trailing zeros).
pub fn siunit(value: f64, unit: &str, maxdecimals: i32) -> String {
    const PREFIXES: [&str; 21] = [
        "q", "r", "y", "z", "a", "f", "p", "n", "μ", "m", "", "k", "M", "G", "T", "P", "E", "Z",
        "Y", "R", "Q",
    ];
    if value.abs() < 1e-30 {
        return format!("0 {unit}");
    }
    let exponent = value.abs().log10().clamp(-30.0, 30.0);
    let step = (exponent / 3.0).floor();
    let divider = 10f64.powf(step * 3.0);
    let scale = 10f64.powi(maxdecimals);
    let mut coefficient = (value / divider * scale).round() / scale;
    let mut index = step as i32 + 10;
    if coefficient >= 1000.0 && index < 20 {
        coefficient /= 1e3;
        index += 1;
    }
    format!(
        "{coefficient} {}{unit}",
        PREFIXES[index.clamp(0, 20) as usize]
    )
}

/// tg `display_power(joules)`: the energy of a machine tick as watts.
pub fn display_power(joules: f64) -> String {
    siunit(joules / MACHINE_TICK_SECS, "W", 3)
}

impl ServerUi for ApcWindow {
    type Source = &'static Apc;
    type UserParam = SiliconUsers<'static, 'static>;
    type Act = ApcAct;

    fn static_data(_: &Apc) {}

    fn data(
        apc: &Apc,
        user: Entity,
        silicons: &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> ApcData {
        let channel = |title: &str, ch: Channel, key: &str| PowerChannel {
            title: title.to_owned(),
            power_load: display_power(apc.lastused[ch as usize]),
            status: apc.channels[ch as usize],
            key: key.to_owned(),
        };
        ApcData {
            locked: apc.locked,
            fail_time: apc.failure_timer.ceil() as i32,
            is_operating: apc.operating,
            external_power: apc.main_status,
            power_cell_status: apc.cell.as_ref().map(|c| c.percent() as f32),
            charge_mode: apc.chargemode,
            charging_status: apc.charging,
            charging_power_display: display_power(apc.lastused_charge),
            total_load: display_power(apc.lastused.iter().sum()),
            cover_locked: apc.coverlocked,
            remote_access: false,
            silicon_user: silicons.contains(user),
            malf_status: 0,
            emergency_lights: !apc.emergency_lights,
            nightshift_lights: apc.nightshift_lights,
            disable_nightshift_toggle: false,
            power_channels: vec![
                channel("Оборудование", Channel::Equip, "eqp"),
                channel("Освещение", Channel::Light, "lgt"),
                channel("Окружение", Channel::Environ, "env"),
            ],
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ApcAct, UiReject> {
        Ok(match action {
            "lock" => ApcAct::Lock,
            "cover" => ApcAct::Cover,
            "breaker" => ApcAct::Breaker,
            "toggle_nightshift" => ApcAct::Nightshift,
            "charge" => ApcAct::Charge,
            "channel" => {
                let (ch, key) = [
                    (Channel::Equip, "eqp"),
                    (Channel::Light, "lgt"),
                    (Channel::Environ, "env"),
                ]
                .into_iter()
                .find(|(_, k)| params.get(k).is_some())
                .ok_or_else(|| UiReject::BadParams("eqp, lgt or env".into()))?;
                let v = params.number(key)?;
                if !matches!(v, 1.0 | 2.0 | 3.0) {
                    return Err(UiReject::BadParams(format!("{key}: 1, 2 or 3")));
                }
                ApcAct::Channel(ch, v as u8)
            }
            "overload" => ApcAct::Overload,
            "reboot" => ApcAct::Reboot,
            "emergency_lighting" => ApcAct::EmergencyLighting,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

pub struct ApcUiPlugin;

impl Plugin for ApcUiPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<ApcWindow>()
            .add_systems(Update, apply_acts.in_set(UiSystems::Handle));
    }
}

/// `ui_interact`.
pub fn open(world: &mut World, client: Entity, actor: Entity, apc: Entity) {
    let title = world.get::<DmName>(apc).map_or_else(String::new, |n| {
        crate::vending::plain_name(&n.name).to_owned()
    });
    world.write_message(OpenUi::new::<ApcWindow>(client, actor, apc, title));
}

/// tg `attack_hand` with the cover closed: the window. True when the click was the APC's.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<Apc>(target).is_none() {
        return false;
    }
    // An open wire panel shows the wires instead (`crate::wires`).
    if world
        .get::<crate::construction::PanelOpen>(target)
        .is_none_or(|p| !p.0)
    {
        open(world, client, mob, target);
    }
    true
}

fn apply_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ApcWindow>>>) {
    let pending: Vec<(Entity, Entity, Entity, ApcAct)> = acts
        .read(world.resource::<Messages<UiActed<ApcWindow>>>())
        .map(|a| (a.client, a.actor, a.source, a.act))
        .collect();
    for (client, actor, apc, act) in pending {
        apply_act(world, client, actor, apc, act);
    }
}

/// tg `setsubsystem(val)`.
fn setsubsystem(apc: &Apc, val: u8) -> u8 {
    if apc.cell.as_ref().is_some_and(|c| c.charge > 0.0) {
        if val == 1 { CHANNEL_OFF } else { val }
    } else if val == 3 {
        CHANNEL_AUTO_OFF
    } else {
        CHANNEL_OFF
    }
}

/// `ui_act` at the moment of effect.
pub fn apply_act(world: &mut World, client: Entity, actor: Entity, apc: Entity, act: ApcAct) {
    let silicon = has_silicon_access(world, actor);
    let Some(state) = world.get::<Apc>(apc) else {
        return;
    };
    let (locked, failing) = (state.locked, state.failure_timer > 0.0);
    // `can_use(user, 1)`: a silicon is refused while the AI wire disabled it.
    if silicon && !crate::silicon::ai::apc_usable(world, client, apc) {
        return;
    }
    if locked && !silicon && !failing && act != ApcAct::Nightshift {
        return;
    }
    match act {
        ApcAct::Lock => {
            if silicon && let Some(mut a) = world.get_mut::<Apc>(apc) {
                a.locked = !a.locked;
            }
        }
        ApcAct::Cover => {
            if let Some(mut a) = world.get_mut::<Apc>(apc) {
                a.coverlocked = !a.coverlocked;
            }
        }
        ApcAct::Breaker => crate::silicon::ai::toggle_breaker(world, client, apc),
        ApcAct::Nightshift => {
            if let Some(mut a) = world.get_mut::<Apc>(apc) {
                a.nightshift_lights = !a.nightshift_lights;
            }
        }
        ApcAct::Charge => {
            if let Some(mut a) = world.get_mut::<Apc>(apc) {
                a.chargemode = !a.chargemode;
                if !a.chargemode {
                    a.charging = NOT_CHARGING;
                }
            }
        }
        ApcAct::Channel(ch, val) => {
            let value = setsubsystem(world.get::<Apc>(apc).expect("checked"), val);
            crate::wires::apc_channel(world, apc, ch, |_| value);
        }
        ApcAct::Overload => {}
        ApcAct::Reboot => {
            if let Some(mut a) = world.get_mut::<Apc>(apc) {
                a.failure_timer = 0.0;
                a.force_update = false;
            }
            crate::wires::apc_update(world, apc);
        }
        ApcAct::EmergencyLighting => {
            if let Some(mut a) = world.get_mut::<Apc>(apc) {
                a.emergency_lights = !a.emergency_lights;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn siunit_matches_dm() {
        assert_eq!(siunit(0.0, "W", 3), "0 W");
        assert_eq!(siunit(1500.0, "W", 3), "1.5 kW");
        assert_eq!(siunit(999.9999, "W", 3), "1 kW");
        assert_eq!(siunit(250.0, "W", 3), "250 W");
        assert_eq!(display_power(5000.0), "2.5 kW");
    }
}
