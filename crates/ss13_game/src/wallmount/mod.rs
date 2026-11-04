//! Wall-mounted machines and cabinets (server), after tg's fire alarms
//! (`code/game/machinery/firealarm.dm`), buttons and their controllers (`buttons.dm`,
//! `code/modules/assembly/doorcontrol.dm`), light switches (`lightswitch.dm`, BandaStation's
//! `aesthetics/lightswitch`), mounted flashers (`flasher.dm`), extinguisher cabinets
//! (`code/game/objects/structures/extinguisher.dm`) and fire axe cabinets (`fireaxe.dm`).
//!
//! Live state is built once at `PostStartup` (after doors and power) from the prototype data
//! [`DmWallMount`]. They hang on the floor tile in front of their wall (pixel offset), so a click
//! needs the usual adjacency to that tile. Unpowered machines (tg `NOPOWER`) do nothing on a
//! click (`can_interact`); their looks follow [`Unpowered`].
//!
//! - [`fire_alarm`]: pull (left click) and reset (right click), the area's firelocks, red
//!   lights, the siren.
//! - [`controls`]: buttons opening blast doors and airlocks by `id`, mass driver and flasher
//!   buttons; mounted flashers.
//! - [`cabinets`]: extinguisher and fire axe cabinets.
//! - The light switch, here: it flips its area's `lightswitch`.
//!
//! Cost: nothing per tick but the firelocks still moving to their alarm state, ringing fire
//! alarms (one sound per 2.4 s each) and buttons with a pending flick or timer; everything else
//! runs on a click or a power change.
// shortcut: no construction or deconstruction (wall frames, screwdriver panels, wires,
// electronics), no damage, breaking, emag or EMP, no silicon access (AI clicks, ctrl-click
// thermal sensors), no circuit USB ports; the light switch's `area` var (controlling another
// area) is not read. Add each with its mechanic.

pub mod cabinets;
pub mod controls;
pub mod fire_alarm;

use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::light::{DmLight, MINIMUM_USEFUL_LIGHT_RANGE, source};
use ss13_mapload::wallmount::{
    BUTTON_ROOT, EXTINGUISHER_CABINET_ROOT, FIREALARM_ROOT, FIREAXE_CABINET_ROOT, FLASHER_ROOT,
    LIGHT_SWITCH_ROOT, PORTABLE_FLASHER,
};
use ss13_mapload::{Areas, DmAtom, DmWallMount};
use ss13_shared::protocol::ClickModifiers;
use ss13_shared::{Appearance, EMISSIVE_PLANE, LightSource, Optics, Overlay, TilePos};

use crate::power::{AreaPower, AreaPowerChanged, Unpowered};
use crate::sound::{PlaySound, playsound};

/// `/obj/machinery/light_switch`: `sound_on`, `sound_off`, and BandaStation's click played with
/// either (`modular_bandastation/aesthetics/lightswitch`).
const SWITCH_ON: &str = "sound/items/weapons/magin.ogg";
const SWITCH_OFF: &str = "sound/items/weapons/magout.ogg";
const SWITCH_CLICK: &str = "modular_bandastation/aesthetics/lightswitch/sound/lightswitch.ogg";

/// tg `/obj/machinery/light_switch`, bound to its area.
#[derive(Component, Clone, Copy, Debug)]
pub struct LightSwitch {
    pub area: Entity,
}

/// A timed action on a wall mount (tg `do_after` in its procs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallJob {
    /// tg `fireaxecabinet/toggle_lock`.
    ToggleLock,
}

pub struct WallMountPlugin;

impl Plugin for WallMountPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<controls::Links>()
            // After doors (`Door`) and power (`AreaPower`, `PowerMachine`) are built.
            .add_systems(PostStartup, init_wallmounts)
            .add_systems(
                Update,
                (
                    fire_alarm::correct_firelocks,
                    fire_alarm::ring_alarms,
                    controls::run_timers,
                    power_looks,
                )
                    .chain()
                    .after(crate::power::PowerSystems)
                    .after(crate::click::ResolveClicks),
            );
    }
}

/// Live state for every [`DmWallMount`] on the map.
fn init_wallmounts(world: &mut World) {
    let mut query = world.query::<(Entity, &DmWallMount, &TilePos)>();
    let mounts: Vec<(Entity, DmWallMount, TilePos)> = query
        .iter(world)
        .map(|(e, dm, &pos)| (e, dm.clone(), pos))
        .collect();
    fire_alarm::init_firelocks(world);
    controls::init_links(world);
    for (e, dm, pos) in mounts {
        let area = world.resource::<Areas>().area_at(pos);
        let path = dm.path.as_str();
        if is_under(path, FIREALARM_ROOT) {
            if let Some(area) = area {
                fire_alarm::add_alarm(world, e, area);
            }
        } else if is_under(path, LIGHT_SWITCH_ROOT) {
            if let Some(area) = area {
                world.entity_mut(e).insert(LightSwitch { area });
            }
        } else if is_under(path, BUTTON_ROOT) {
            controls::add_button(world, e, &dm);
        } else if is_under(path, FLASHER_ROOT) && !is_under(path, PORTABLE_FLASHER) {
            controls::add_flasher(world, e, &dm);
        } else if is_under(path, EXTINGUISHER_CABINET_ROOT) {
            cabinets::add_extinguisher_cabinet(world, e);
        } else if is_under(path, FIREAXE_CABINET_ROOT) {
            cabinets::add_fireaxe_cabinet(world, e, &dm);
        }
        refresh_look(world, e);
    }
}

/// tg `attack_hand` / `attack_hand_secondary` of a wall mount with an empty hand. True when the
/// click was theirs.
pub fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    if world.get::<fire_alarm::FireAlarm>(target).is_some() {
        if !modifiers.right {
            fire_alarm::alarm(world, target, Some(client));
        } else {
            fire_alarm::reset(world, target, Some(client));
        }
        return true;
    }
    if world.get::<LightSwitch>(target).is_some() {
        flip_switch(world, target);
        return true;
    }
    if world.get::<controls::Button>(target).is_some() {
        // `attack_hand_secondary` with the panel closed falls through to the normal press.
        controls::attempt_press(world, client, mob, target);
        return true;
    }
    cabinets::interact(world, client, mob, target, modifiers.right)
}

/// tg `item_interaction` of a wall mount with `held`. True when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    controls::item_interaction(world, client, mob, held, target)
        || cabinets::item_interaction(world, client, mob, held, target, modifiers)
}

/// `crate::construction::complete` of a [`WallJob`].
pub fn complete(world: &mut World, client: Entity, target: Entity, job: WallJob) {
    match job {
        WallJob::ToggleLock => cabinets::finish_toggle_lock(world, client, target),
    }
}

/// tg `light_switch/interact`: the sound, then `set_lights(!area.lightswitch)`.
fn flip_switch(world: &mut World, switch: Entity) {
    if world.get::<Unpowered>(switch).is_some() {
        return;
    }
    let (Some(&LightSwitch { area }), Some(&pos)) = (
        world.get::<LightSwitch>(switch),
        world.get::<TilePos>(switch),
    ) else {
        return;
    };
    let on = world.get::<AreaPower>(area).is_some_and(|a| a.lightswitch);
    playsound(
        world,
        PlaySound::new(pos, if on { SWITCH_OFF } else { SWITCH_ON }, 40.0, true),
    );
    playsound(world, PlaySound::new(pos, SWITCH_CLICK, 100.0, true));
    set_lights(world, area, !on);
}

/// tg `light_switch/set_lights(status)`: the area's switch state, every switch of the area,
/// then `area.power_change()` (its lights follow).
pub fn set_lights(world: &mut World, area: Entity, status: bool) {
    let Some(mut power) = world.get_mut::<AreaPower>(area) else {
        return;
    };
    if power.lightswitch == status {
        return;
    }
    power.lightswitch = status;
    world.write_message(AreaPowerChanged(area));
    let mut switches = world.query::<(Entity, &LightSwitch)>();
    let same: Vec<Entity> = switches
        .iter(world)
        .filter(|(_, s)| s.area == area)
        .map(|(e, _)| e)
        .collect();
    for e in same {
        refresh_look(world, e);
    }
}

/// One wall mount's icon state, overlays (own icon; `true` = emissive) and light.
struct Look {
    icon_state: String,
    overlays: Vec<(String, bool)>,
    light: Option<LightSource>,
}

/// The light `e`'s light vars give with `range`, `power` and `color` set (tg `set_light`).
fn light_of(
    world: &World,
    e: Entity,
    range: f32,
    power: f32,
    color: [u8; 3],
) -> Option<LightSource> {
    let atom = world.get::<DmAtom>(e)?;
    let path = &world.get::<DmWallMount>(e)?.path;
    let light = DmLight {
        light_range: range.max(MINIMUM_USEFUL_LIGHT_RANGE),
        light_power: power,
        light_on: true,
        ..world.get::<DmLight>(e).cloned().unwrap_or_default()
    };
    source(path, &light, atom.dir, [atom.pixel_x, atom.pixel_y]).map(|l| LightSource { color, ..l })
}

/// What `e` shows now, by its kind; `None` for something else.
fn look_of(world: &World, e: Entity) -> Option<Look> {
    let powered = world.get::<Unpowered>(e).is_none();
    if let Some(alarm) = world.get::<fire_alarm::FireAlarm>(e) {
        return Some(fire_alarm::look(world, e, alarm, powered));
    }
    if let Some(switch) = world.get::<LightSwitch>(e) {
        let base = &world.get::<DmWallMount>(e)?.base_icon_state;
        let on = world
            .get::<AreaPower>(switch.area)
            .is_some_and(|a| a.lightswitch);
        let state = if on { "on" } else { "off" };
        // `update_icon_state`: `set_light(area.lightswitch ? 0 : light_on_range)`.
        let range = world.get::<DmWallMount>(e)?.light_on_range;
        let light = world.get::<DmLight>(e).cloned().unwrap_or_default();
        let color = light.light_color.map_or([255; 3], |[r, g, b, _]| [r, g, b]);
        return Some(if powered {
            Look {
                icon_state: format!("{base}-{state}"),
                overlays: vec![(format!("{base}-emissive-{state}"), true)],
                light: (!on && range > 0.0)
                    .then(|| light_of(world, e, range, light.light_power, color))
                    .flatten(),
            }
        } else {
            Look {
                icon_state: format!("{base}-nopower"),
                overlays: Vec::new(),
                light: None,
            }
        });
    }
    if let Some(button) = world.get::<controls::Button>(e) {
        return Some(controls::button_look(world, e, button, powered));
    }
    if let Some(flasher) = world.get::<controls::Flasher>(e) {
        return Some(controls::flasher_look(world, e, flasher, powered));
    }
    cabinets::look(world, e)
}

/// Writes `e`'s [`Look`]: the appearance and light, touching them only on a change so
/// replication sends nothing for an unchanged mount.
pub(crate) fn refresh_look(world: &mut World, e: Entity) {
    if world.get::<fire_alarm::Firelock>(e).is_some() {
        fire_alarm::refresh_firelock(world, e);
        return;
    }
    let Some(look) = look_of(world, e) else {
        return;
    };
    let Some(icon) = world.get::<DmAtom>(e).map(|a| a.icon.clone()) else {
        return;
    };
    let (state, overlays) = {
        let Some(index) = world.get_resource::<IconIndex>() else {
            return;
        };
        let state = index.get(&icon, &look.icon_state);
        let overlays: Vec<Overlay> = look
            .overlays
            .iter()
            .filter_map(|(s, emissive)| {
                let id = index.get(&icon, s)?;
                Some(if *emissive {
                    Overlay {
                        plane: Some(EMISSIVE_PLANE),
                        ..Overlay::new(id)
                    }
                } else {
                    Overlay::new(id)
                })
            })
            .collect();
        (state, overlays)
    };
    if let Some(mut appearance) = world.get_mut::<Appearance>(e) {
        if let Some(state) = state
            && appearance.icon_state != state
        {
            appearance.icon_state = state;
        }
        if appearance.overlays != overlays {
            appearance.overlays = overlays;
        }
    }
    match world.get_mut::<Optics>(e) {
        Some(mut optics) => {
            if optics.light != look.light {
                optics.light = look.light;
            }
        }
        None if look.light.is_some() => {
            world.entity_mut(e).insert(Optics {
                light: look.light,
                ..default()
            });
        }
        None => {}
    }
}

/// tg `power_change()` -> `update_appearance()` of the wall mounts whose area power changed.
fn power_looks(
    added: Query<
        Entity,
        (
            Added<Unpowered>,
            Or<(With<DmWallMount>, With<fire_alarm::Firelock>)>,
        ),
    >,
    mut removed: RemovedComponents<Unpowered>,
    mounts: Query<(), Or<(With<DmWallMount>, With<fire_alarm::Firelock>)>>,
    mut commands: Commands,
) {
    let changed: Vec<Entity> = added
        .iter()
        .chain(removed.read().filter(|&e| mounts.contains(e)))
        .collect();
    if !changed.is_empty() {
        commands.queue(move |world: &mut World| {
            for e in changed {
                refresh_look(world, e);
            }
        });
    }
}
