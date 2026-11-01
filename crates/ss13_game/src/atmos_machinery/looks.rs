//! tg's runtime icons of atmos objects. Maps place preview states (`pipe11-3`, `vent_map_on-3`);
//! tg replaces them in `update_icon` once nodes are known:
//! - smart pipes: `!pipes_bitmask.dmi` `"[bitfield]_[piping_layer]"`, full pipes toward the
//!   connected nodes plus short stubs (`pipe/smart/update_pipe_icon`), turned toward them;
//! - components: their state (`vent_out`, `pump_on-1`, ...) from `update_icon_nopipes`; on an
//!   interactable tile (plating) the pipe underlays toward their nodes, under the sprite, and
//!   the piping layer shift; under a floor tile the floor plane without underlays
//!   (`components/update_icon`, `hide_pipe`).
//!
//! Appearance has no underlays: the first underlay is drawn as the base sprite and the rest,
//! then the machine's own sprite, as overlays in tg's draw order.
// shortcut: no pipe caps (`cap_overlay`), no emissive masks, no flick animations
// (`vent_out-starting`), no layer manifold or color adapter overlays; layer offsets by piping
// layer and pipe color are not applied.

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_atmos::pipenet::{DeviceKind, Pipenet};
use ss13_mapload::DmAtom;
use ss13_mapload::fidelity::IconIndex;
use ss13_mapload::undertile::{BELOW_CATWALK_LAYER, FLOOR_PLANE, accessibility};
use ss13_shared::{Appearance, Dir, Facing, IconStateId, Layer, Overlay, TilePos};

use super::devices::{
    GasFilter, GasMixer, GasPump, OutletInjector, PassiveGate, PassiveVent, PortsConnector, Valve,
    VentPump, VentScrubber, VolumePump, valve_open,
};
use super::portable::Canister;
use super::{AirAlarm, AtmosDevice};
use crate::power::Unpowered;

const BITMASK: &str = "icons/obj/pipes_n_cables/!pipes_bitmask.dmi";
const UNDERLAYS: &str = "icons/obj/pipes_n_cables/pipe_underlays.dmi";
const CANISTERS: &str = "icons/obj/pipes_n_cables/canisters.dmi";
/// `UNDERFLOOR_INTERACTABLE`.
const INTERACTABLE: u8 = 2;
/// `PIPING_LAYER_P_X`, `PIPING_LAYER_P_Y`.
const LAYER_SHIFT: i16 = 5;
const GAME_PLANE: i16 = -4;

/// Icon lookups the looks need, resolved once.
#[derive(Resource, Default)]
pub struct AtmosIcons {
    index: HashMap<(String, String), IconStateId>,
}

impl AtmosIcons {
    fn get(&self, icon: &str, state: &str) -> Option<IconStateId> {
        self.index
            .get(&(icon.to_owned(), state.to_owned()))
            .copied()
    }
}

pub fn init_looks(world: &mut World) {
    let index = world
        .get_resource::<IconIndex>()
        .map(|i| i.0.clone())
        .unwrap_or_default();
    world.insert_resource(AtmosIcons { index });
    // Every machine draws its state once.
    let mut q = world.query_filtered::<Entity, With<AtmosDevice>>();
    let all: Vec<Entity> = q.iter(world).collect();
    for e in all {
        world.entity_mut(e).insert(LookDirty);
    }
    let mut q = world.query_filtered::<Entity, Or<(With<Canister>, With<AirAlarm>)>>();
    let all: Vec<Entity> = q.iter(world).collect();
    for e in all {
        world.entity_mut(e).insert(LookDirty);
    }
}

/// The entity's look must be redone.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct LookDirty;

/// `pipe/smart/update_pipe_icon`: the bitfield and dir of a smart pipe with `connections`
/// among `init_dirs`.
pub fn smart_pipe_icon(connections: u8, init_dirs: u8) -> (u16, u8) {
    let is_stub = |bits: u8| bits & bits.wrapping_sub(1) == 0;
    let dir = match connections {
        12 => 4,
        3 => 2,
        c => c,
    };
    let mut bitfield = u16::from(connections);
    if is_stub(connections) {
        let mut add = 0u8;
        if connections != 0 {
            add |= super::reverse_dir(connections) & init_dirs;
        }
        let mut shift = 0;
        while is_stub(connections | add) && (init_dirs >> shift) != 0 {
            let candidate = init_dirs & (1 << shift);
            if candidate & connections == 0 {
                add |= candidate;
            }
            shift += 1;
        }
        bitfield |= u16::from(add) << 4;
    }
    (bitfield, dir)
}

/// Smart pipes whose connections changed get tg's bitmask state.
pub fn sync_pipe_looks(
    mut net: ResMut<Pipenet>,
    icons: Option<Res<AtmosIcons>>,
    devices: Query<&AtmosDevice>,
    mut pipes: Query<(&DmAtom, &mut Appearance, Option<&mut Facing>)>,
    mut commands: Commands,
) {
    let changed = net.take_changed();
    let Some(icons) = icons else { return };
    for id in changed {
        let Some(d) = net.device(id) else { continue };
        let entity = d.entity;
        if !matches!(d.kind, DeviceKind::Pipe { smart: true, .. }) {
            commands.entity(entity).insert(LookDirty);
            continue;
        }
        let connections = d.connections(&net);
        let (bitfield, dir) = smart_pipe_icon(connections, d.init_dirs);
        let layer = devices.get(entity).map_or(3, |a| a.layer);
        let Some(state) = icons.get(BITMASK, &format!("{bitfield}_{layer}")) else {
            continue;
        };
        let Ok((_, mut appearance, facing)) = pipes.get_mut(entity) else {
            continue;
        };
        if appearance.icon_state != state {
            appearance.icon_state = state;
        }
        let dir = Dir::from_bits(dir).unwrap_or_default();
        match facing {
            Some(mut f) if f.0 != dir => f.0 = dir,
            Some(_) => {}
            None => {
                commands.entity(entity).insert(Facing(dir));
            }
        }
    }
}

/// The machine sprite's state (`update_icon_nopipes`), and whether it shifts with its piping
/// layer and is hidden under floors.
struct Look {
    icon: String,
    state: String,
    shift: bool,
    hides: bool,
    /// tg `shift_underlay_only = FALSE`: underlays drawn on layer 3, the atom shifted.
    underlay_layer_3: bool,
}

fn layer_offset(layer: u8) -> u8 {
    if layer % 2 == 1 { 1 } else { 2 }
}

type MachineData = (
    Entity,
    &'static AtmosDevice,
    &'static DmAtom,
    &'static TilePos,
    Has<Unpowered>,
    (
        Option<&'static VentPump>,
        Option<&'static VentScrubber>,
        Option<&'static GasPump>,
        Option<&'static VolumePump>,
        Option<&'static PassiveGate>,
        Option<&'static Valve>,
        Option<&'static GasFilter>,
        Option<&'static GasMixer>,
        Option<&'static OutletInjector>,
        Option<&'static PortsConnector>,
        Has<PassiveVent>,
    ),
);

type MachineFilter = Or<(
    With<LookDirty>,
    Changed<VentPump>,
    Changed<VentScrubber>,
    Changed<GasPump>,
    Changed<VolumePump>,
    Changed<PassiveGate>,
    Changed<GasFilter>,
    Changed<GasMixer>,
    Changed<OutletInjector>,
)>;

/// Machines whose state, power or nodes changed redraw.
pub fn sync_machine_looks(world: &mut World) {
    let Some(icons) = world.remove_resource::<AtmosIcons>() else {
        return;
    };
    let mut changed: Vec<(Entity, Look, u8, u8, TilePos)> = Vec::new();
    {
        let mut q = world.query_filtered::<MachineData, MachineFilter>();
        let net = world.resource::<Pipenet>();
        for (e, device, atom, &pos, unpowered, m) in q.iter(world) {
            let (
                vent,
                scrubber,
                pump,
                volume,
                gate,
                valve,
                filter,
                mixer,
                injector,
                connector,
                passive,
            ) = m;
            let id = device.id;
            let all_nodes = net
                .device(id)
                .is_some_and(|d| d.nodes.iter().all(Option::is_some));
            let node1 = net
                .device(id)
                .is_some_and(|d| d.nodes.first().copied().flatten().is_some());
            let o = layer_offset(device.layer);
            let icon = atom.icon.clone();
            let look = |state: String, shift: bool, hides: bool| Look {
                icon: icon.clone(),
                state,
                shift,
                hides,
                underlay_layer_3: shift,
            };
            let look = if let Some(v) = vent {
                let s = if v.welded {
                    "vent_welded"
                } else if !node1 || !v.on || unpowered {
                    "vent_off"
                } else if v.releasing {
                    "vent_out"
                } else {
                    "vent_in"
                };
                look(s.into(), true, true)
            } else if let Some(s) = scrubber {
                let st = if s.welded {
                    "scrub_welded"
                } else if !node1 || !s.on || unpowered {
                    "scrub_off"
                } else if !s.scrubbing {
                    "scrub_purge"
                } else if s.widenet {
                    "scrub_wide"
                } else {
                    "scrub_on"
                };
                look(st.into(), true, true)
            } else if let Some(p) = pump {
                let on = if p.on && !unpowered { "on" } else { "off" };
                look(format!("pump_{on}-{o}"), true, false)
            } else if let Some(p) = volume {
                let on = if p.on && !unpowered { "on" } else { "off" };
                look(format!("volpump_{on}-{o}"), true, false)
            } else if let Some(p) = gate {
                let on = if p.on { "on" } else { "off" };
                look(format!("passgate_{on}-{o}"), true, false)
            } else if let Some(v) = valve {
                let kind = if v.digital { "d" } else { "m" };
                let on = if valve_open(net, id) { "on" } else { "off" };
                look(format!("{kind}valve_{on}-{o}"), true, false)
            } else if filter.is_some() || mixer.is_some() {
                let (name, on, flipped) = match (filter, mixer) {
                    (Some(f), _) => ("filter", f.on, f.flipped),
                    (_, Some(m)) => ("mixer", m.on, m.flipped),
                    _ => unreachable!(),
                };
                let on = if on && all_nodes && !unpowered {
                    "on"
                } else {
                    "off"
                };
                let offset = match device.layer {
                    1 => 1,
                    5 => 5,
                    _ => 0,
                };
                let f = if flipped { "_f" } else { "" };
                let mut l = look(format!("{name}_{on}-{offset}{f}"), false, false);
                l.underlay_layer_3 = false;
                l
            } else if let Some(i) = injector {
                let on = if node1 && i.on && !unpowered {
                    "inje_on"
                } else {
                    "inje_off"
                };
                look(on.into(), true, true)
            } else if connector.is_some() {
                look("connector".into(), true, true)
            } else if passive {
                look("passive_vent".into(), true, true)
            } else {
                continue;
            };
            changed.push((e, look, device.layer, device.dir, pos));
        }
    }
    for (e, look, layer, dir, pos) in changed {
        world.entity_mut(e).remove::<LookDirty>();
        draw_component(world, &icons, e, &look, layer, dir, pos);
    }
    redraw_portables(world, &icons);
    world.insert_resource(icons);
}

/// `components/update_icon`.
fn draw_component(
    world: &mut World,
    icons: &AtmosIcons,
    e: Entity,
    look: &Look,
    layer: u8,
    dir: u8,
    pos: TilePos,
) {
    let Some(sprite) = icons.get(&look.icon, &look.state) else {
        return;
    };
    let access = accessibility(world, pos);
    let interactable = access >= INTERACTABLE || !look.hides;
    let Some(atom) = world.get::<DmAtom>(e).cloned() else {
        return;
    };
    let mut underlays: Vec<(IconStateId, [u8; 4])> = Vec::new();
    if interactable && let Some(device) = world.get::<AtmosDevice>(e).copied() {
        let net = world.resource::<Pipenet>();
        if let Some(d) = net.device(device.id) {
            let under_layer = if look.underlay_layer_3 { 3 } else { layer };
            let own = [d.color[0], d.color[1], d.color[2], 255];
            let mut connected = 0u8;
            for n in d.nodes.iter().flatten() {
                let Some(nd) = net.device(*n) else { continue };
                let node_dir = cardinal(pos, nd.pos);
                connected |= node_dir;
                // `SELECT_ATMOS_NODE_COLOR`.
                let color = if nd.flags & ss13_atmos::pipenet::flags::DONT_SHARE_COLOR != 0
                    || nd.color == ss13_atmos::pipenet::COLOR_OMNI
                {
                    own
                } else {
                    [nd.color[0], nd.color[1], nd.color[2], 255]
                };
                if let Some(s) = icons.get(UNDERLAYS, &format!("intact_{node_dir}_{under_layer}")) {
                    underlays.push((s, color));
                }
            }
            for c in ss13_atmos::pipenet::CARDINALS {
                if d.init_dirs & c != 0
                    && connected & c == 0
                    && let Some(s) = icons.get(UNDERLAYS, &format!("exposed_{c}_{under_layer}"))
                {
                    underlays.push((s, own));
                }
            }
        }
    }
    let shift = if interactable && look.shift {
        let delta = (i16::from(layer) - 3) * LAYER_SHIFT;
        match dir {
            1 | 2 => [delta, 0],
            _ => [0, delta],
        }
    } else {
        [0, 0]
    };
    let (plane, layer_value) = if interactable {
        (GAME_PLANE, atom.layer)
    } else {
        (FLOOR_PLANE, BELOW_CATWALK_LAYER)
    };
    let white = Some([255, 255, 255, 255]);
    let overlay = |icon_state: IconStateId, color: Option<[u8; 4]>, dir: Option<Dir>| Overlay {
        icon_state,
        dir,
        color,
        pixel_offset: [0, 0],
        plane: None,
        only_dirs: 0,
    };
    let (base, base_color, overlays) = match underlays.split_first() {
        Some((&(first, color), rest)) => {
            let mut overlays: Vec<Overlay> = rest
                .iter()
                .map(|&(s, c)| overlay(s, Some(c), Some(Dir::South)))
                .collect();
            overlays.push(overlay(sprite, white, None));
            (first, Some(color), overlays)
        }
        None => (sprite, None, Vec::new()),
    };
    let Some(mut appearance) = world.get_mut::<Appearance>(e) else {
        return;
    };
    let new = Appearance {
        icon_state: base,
        layer: Layer(layer_value),
        plane,
        color: base_color,
        pixel_offset: [atom.pixel_x + shift[0], atom.pixel_y + shift[1]],
        rotation: 0,
        overlays,
        mouse_opacity: appearance.mouse_opacity,
    };
    if *appearance != new {
        *appearance = new;
    }
}

fn cardinal(from: TilePos, to: TilePos) -> u8 {
    match (to.x - from.x, to.y - from.y) {
        (0, 1) => 1,
        (0, -1) => 2,
        (1, 0) => 4,
        (-1, 0) => 8,
        _ => 0,
    }
}

/// Canisters: the pressure light and the connector overlay (`canister/update_overlays`);
/// air alarms: the alert light (`airalarm/update_overlays`).
fn redraw_portables(world: &mut World, icons: &AtmosIcons) {
    let mut q = world.query_filtered::<(
        Entity,
        Option<&Canister>,
        Option<&super::PortableAtmos>,
        Option<&AirAlarm>,
        Has<Unpowered>,
        &DmAtom,
    ), Or<(
        With<LookDirty>,
        Changed<Canister>,
        Changed<super::PortableAtmos>,
        Changed<AirAlarm>,
        Added<Unpowered>,
    )>>();
    // (state, emissive): tg adds the alert light twice, as a sprite and as an emissive.
    let mut updates: Vec<(Entity, Vec<(IconStateId, bool)>)> = Vec::new();
    for (e, canister, portable, alarm, unpowered, atom) in q.iter(world) {
        let mut overlays = Vec::new();
        if let Some(c) = canister {
            if portable.is_some_and(|p| p.holding.is_some())
                && let Some(s) = icons.get(CANISTERS, "can-open")
            {
                overlays.push((s, false));
            }
            if portable.is_some_and(|p| p.port.is_some())
                && let Some(s) = icons.get(CANISTERS, "can-connector")
            {
                overlays.push((s, false));
            }
            if let Some(s) = c.light.and_then(|l| icons.get(CANISTERS, l)) {
                overlays.push((s, false));
            }
        } else if let Some(a) = alarm {
            if unpowered {
                updates.push((e, Vec::new()));
                continue;
            }
            let state = if a.danger_level == super::alarm::ALERT_HAZARD {
                "alarm1"
            } else if a.danger_level == super::alarm::ALERT_WARNING || a.area_danger {
                "alarm2"
            } else {
                "alarm0"
            };
            if let Some(s) = icons.get(&atom.icon, state) {
                overlays.push((s, false));
                overlays.push((s, true));
            }
        } else {
            continue;
        }
        updates.push((e, overlays));
    }
    for (e, states) in updates {
        world.entity_mut(e).remove::<LookDirty>();
        let Some(mut appearance) = world.get_mut::<Appearance>(e) else {
            continue;
        };
        let overlays: Vec<Overlay> = states
            .into_iter()
            .map(|(icon_state, emissive)| Overlay {
                icon_state,
                dir: None,
                color: None,
                pixel_offset: [0, 0],
                plane: emissive.then_some(ss13_shared::EMISSIVE_PLANE),
                only_dirs: 0,
            })
            .collect();
        if appearance.overlays != overlays {
            appearance.overlays = overlays;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smart_pipe_states_follow_tg() {
        // A straight east-west pipe: full pipes east and west, faced east.
        assert_eq!(smart_pipe_icon(4 | 8, 15), (12, 4));
        // A north-south pipe faces south.
        assert_eq!(smart_pipe_icon(1 | 2, 15), (3, 2));
        // A T: three full pipes, faced their bits.
        assert_eq!(smart_pipe_icon(1 | 4 | 8, 15), (13, 13));
        // Connected only north: a short pipe south is added (the reverse first).
        assert_eq!(smart_pipe_icon(1, 15), (1 | (2 << 4), 1));
        // Nothing connected: short pipes north and south.
        assert_eq!(smart_pipe_icon(0, 15), ((1 | 2) << 4, 0));
    }
}
