//! Atmos consoles (server): tg `/obj/machinery/computer/atmos_alert`
//! (`code/game/machinery/computer/atmos_alert.dm`). Every machine tick the console lists the
//! areas whose air alarms are in hazard (priority) or warning (minor), on its z-level or, for
//! `station_only` boards, in station and mining areas; its screen shows `alert:2` / `alert:1`.
//! A button clears an area until the next check.
// shortcut: `station_only` takes the station areas by path (`/area/station`, `/area/mine`)
// rather than `GLOB.the_station_areas` (areas found on station z-levels at load).

use bevy::prelude::*;
use ss13_chat::Span;
use ss13_mapload::{DmAtom, DmName, DmPower, IconIndex};
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::atmos_alert::{AtmosAlertData, AtmosAlertUi};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::alarm::{ALERT_HAZARD, ALERT_WARNING, AirAlarm};
use crate::obj_damage::is_under;
use crate::power::Unpowered;
use crate::tools::tell;

pub const ALERT_CONSOLE: &str = "/obj/machinery/computer/atmos_alert";
const STATION_ONLY: &str = "/obj/machinery/computer/atmos_alert/station_only";
/// `SSmachines` wait.
const TICK_SECS: f64 = 2.0;

/// One alert console.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct AtmosAlertConsole {
    pub station_only: bool,
    pub priority: Vec<String>,
    pub minor: Vec<String>,
}

#[derive(Resource, Default)]
struct ConsoleClock(f64);

pub fn plugin(app: &mut App) {
    app.init_resource::<ConsoleClock>()
        .register_ui::<AlertWindow>()
        .add_systems(
            ss13_world::MapInit,
            init_consoles.after(super::init_atmos_machinery),
        )
        .add_systems(
            Update,
            (
                alert_acts.in_set(UiSystems::Handle),
                (process_alert_consoles, alert_screens).chain(),
            ),
        );
}

fn init_consoles(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &DmPower), With<ss13_world::Mapped>>();
    let found: Vec<Entity> = q
        .iter(world)
        .filter(|(_, dm)| is_under(&dm.path, ALERT_CONSOLE))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup_console(world, e);
    }
}

/// An atmospheric alert console's state (map load and `crate::machine_frame`).
pub(crate) fn setup_console(world: &mut World, e: Entity) {
    let Some(path) = world.get::<DmPower>(e).map(|d| d.path.clone()) else {
        return;
    };
    if is_under(&path, ALERT_CONSOLE) {
        world.entity_mut(e).insert(AtmosAlertConsole {
            station_only: is_under(&path, STATION_ONLY),
            ..default()
        });
    }
}

/// `get_area_name(alarm, format_text = TRUE)` and whether the area counts for `station_only`.
fn area_of(world: &World, area: Entity) -> (String, bool) {
    let name = world.get::<DmName>(area).map_or_else(String::new, |n| {
        crate::vending::plain_name(&n.name).to_owned()
    });
    let station = world
        .get::<DmPower>(area)
        .is_some_and(|p| is_under(&p.path, "/area/station") || is_under(&p.path, "/area/mine"));
    (name, station)
}

/// `process`: the alarms again.
fn process_alert_consoles(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs_f64();
    {
        let mut clock = world.resource_mut::<ConsoleClock>();
        clock.0 += dt;
        if clock.0 < TICK_SECS {
            return;
        }
        clock.0 -= TICK_SECS;
    }
    let mut alarms = world.query::<(&AirAlarm, &TilePos)>();
    let alarms: Vec<(u8, Option<Entity>, u16)> = alarms
        .iter(world)
        .map(|(a, p)| (a.danger_level, a.area, p.z))
        .collect();
    let mut consoles =
        world.query_filtered::<(Entity, &TilePos), (With<AtmosAlertConsole>, Without<Unpowered>)>();
    let consoles: Vec<(Entity, u16)> = consoles.iter(world).map(|(e, p)| (e, p.z)).collect();
    for (console, z) in consoles {
        let station_only = world
            .get::<AtmosAlertConsole>(console)
            .is_some_and(|c| c.station_only);
        let (mut priority, mut minor) = (Vec::new(), Vec::new());
        for &(danger, area, alarm_z) in &alarms {
            let Some(area) = area else { continue };
            let (name, station) = area_of(world, area);
            if (station_only && !station) || (!station_only && alarm_z != z) {
                continue;
            }
            match danger {
                ALERT_HAZARD => priority.push(name),
                ALERT_WARNING => minor.push(name),
                _ => {}
            }
        }
        // `priority_alarms += name`: an associative list keeps one entry per area.
        for list in [&mut priority, &mut minor] {
            let mut seen = std::collections::HashSet::new();
            list.retain(|n| seen.insert(n.clone()));
        }
        let mut c = world.get_mut::<AtmosAlertConsole>(console).expect("listed");
        if c.priority != priority || c.minor != minor {
            c.priority = priority;
            c.minor = minor;
        }
    }
}

const SCREENS: [&str; 2] = ["alert:1", "alert:2"];

/// `update_overlays`: `alert:2` over the screen with priority alarms, else `alert:1` with minor
/// ones.
fn alert_screens(
    icons: Option<Res<IconIndex>>,
    mut consoles: Query<
        (&AtmosAlertConsole, &DmAtom, &mut Appearance, Has<Unpowered>),
        Or<(Changed<AtmosAlertConsole>, Added<Unpowered>)>,
    >,
) {
    let Some(icons) = icons else { return };
    for (console, atom, mut appearance, unpowered) in &mut consoles {
        let ids: Vec<_> = SCREENS
            .iter()
            .filter_map(|s| icons.get(&atom.icon, s))
            .collect();
        appearance.overlays.retain(|o| !ids.contains(&o.icon_state));
        if unpowered {
            continue;
        }
        let state = if !console.priority.is_empty() {
            Some(SCREENS[1])
        } else if !console.minor.is_empty() {
            Some(SCREENS[0])
        } else {
            None
        };
        if let Some(id) = state.and_then(|s| icons.get(&atom.icon, s)) {
            appearance.overlays.push(Overlay::new(id));
        }
    }
}

/// A click on a powered alert console opens its window.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<AtmosAlertConsole>(target).is_none() {
        return false;
    }
    if world.get::<Unpowered>(target).is_none() {
        let title = world.get::<DmName>(target).map_or_else(String::new, |n| {
            crate::vending::plain_name(&n.name).to_owned()
        });
        world.write_message(OpenUi::new::<AlertWindow>(client, mob, target, title));
    }
    true
}

/// `examine`: which alarms it tracks.
pub fn examine_line(world: &World, e: Entity) -> Option<String> {
    let c = world.get::<AtmosAlertConsole>(e)?;
    Some(if c.station_only {
        "Консоль отслеживает все тревоги станции и шахты.".into()
    } else {
        "Консоль отслеживает тревоги на своём уровне.".into()
    })
}

pub struct AlertWindow;

impl UiInterface for AlertWindow {
    const NAME: &'static str = AtmosAlertUi::NAME;
    type Data = AtmosAlertData;
    type StaticData = ();
}

impl ServerUi for AlertWindow {
    type Source = &'static AtmosAlertConsole;
    type UserParam = ();
    type Act = String;

    fn static_data(_: &AtmosAlertConsole) {}

    fn data(c: &AtmosAlertConsole, _: Entity, _: &()) -> AtmosAlertData {
        AtmosAlertData {
            priority: c.priority.clone(),
            minor: c.minor.clone(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<String, UiReject> {
        match action {
            "clear" => params
                .get("zone")
                .map(str::to_owned)
                .ok_or_else(|| UiReject::BadParams("zone".into())),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// `ui_act("clear")`: the area leaves its list until the next check.
fn alert_acts(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<UiActed<AlertWindow>>>,
) {
    let acts: Vec<(Entity, Entity, String)> = cursor
        .read(world.resource::<bevy::ecs::message::Messages<UiActed<AlertWindow>>>())
        .map(|a| (a.source, a.client, a.act.clone()))
        .collect();
    for (console, client, zone) in acts {
        let Some(mut c) = world.get_mut::<AtmosAlertConsole>(console) else {
            continue;
        };
        let (priority, minor) = (c.priority.contains(&zone), c.minor.contains(&zone));
        c.priority.retain(|z| *z != zone);
        c.minor.retain(|z| *z != zone);
        if priority {
            tell(
                world,
                client,
                Span::Notice,
                format!("Приоритетная тревога для {zone} снята."),
            );
        }
        if minor {
            tell(
                world,
                client,
                Span::Notice,
                format!("Малая тревога для {zone} снята."),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_needs_a_zone() {
        let params = UiParams::new(&[]);
        assert!(AlertWindow::parse_act("clear", &params).is_err());
        let p = [("zone".to_owned(), "Bridge".to_owned())];
        assert_eq!(
            AlertWindow::parse_act("clear", &UiParams::new(&p)).unwrap(),
            "Bridge"
        );
    }
}
