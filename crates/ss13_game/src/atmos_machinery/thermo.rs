//! A thermomachine's panel, tools, sprite and examine (`unary_devices/thermomachine.dm`).
//!
//! - A screwdriver opens the panel of a machine that is off; with it open the window, ctrl- and
//!   alt-click are refused.
//! - With the panel open a wrench turns it clockwise, a multitool cycles its piping layer, a
//!   right-click multitool its pipe color; each reconnects it (`reconnect_nodes`), keeping the
//!   gas in its port.
//! - Sprite: `thermo-open`, `thermo_1` (on and powered) or `thermo_base`, with its pipe stub in
//!   the node's color shifted by layer, and the emissive while it runs.
// shortcut: no unanchoring (right-click wrench) and no `RefreshParts` (tier 1 numbers whatever
// its parts; a crowbar with the panel open takes it apart into a frame, `crate::machine_frame`);
// the GAGS color by target temperature keeps the type's color (only the mapped colors are
// rendered, see `ss13_mapload::gags`).

use bevy::prelude::*;
use ss13_atmos::pipenet::Pipenet;
use ss13_chat::Span;
use ss13_mapload::{DmAtom, IconIndex};
use ss13_shared::{Appearance, Dir, Facing, Overlay};

use super::AtmosDevice;
use super::devices::ThermoMachine;
use crate::power::Unpowered;
use crate::tools::{ToolKind, tell};

const THERMO_DMI: &str = "icons/obj/machines/atmospherics/thermomachine.dmi";
/// `PIPING_LAYER_MIN`, `PIPING_LAYER_MAX`.
const LAYER_MIN: u8 = 1;
const LAYER_MAX: u8 = 5;
/// `PIPING_LAYER_P_X`, `PIPING_LAYER_P_Y`.
const LAYER_SHIFT: i16 = 5;
const T0C: f64 = 273.15;

/// `GLOB.pipe_paint_colors` in list order.
pub const PIPE_PAINT_COLORS: [(&str, [u8; 3]); 12] = [
    ("omni", [0xEE, 0xEE, 0xEE]),
    ("green", [0x00, 0xFF, 0x00]),
    ("blue", [0x00, 0x00, 0xFF]),
    ("red", [0xFF, 0x00, 0x00]),
    ("orange", [0xFF, 0xA6, 0x2B]),
    ("cyan", [0x00, 0xFF, 0xFF]),
    ("dark", [0x45, 0x45, 0x45]),
    ("yellow", [0xFF, 0xFF, 0x00]),
    ("brown", [0xBA, 0x9F, 0x6D]),
    ("pink", [0xFF, 0x3C, 0xC8]),
    ("purple", [0x80, 0x00, 0x80]),
    ("violet", [0x69, 0x27, 0xC5]),
];

/// `turn(dir, -90)`.
pub fn clockwise(dir: u8) -> u8 {
    match dir {
        1 => 4,
        4 => 2,
        2 => 8,
        _ => 1,
    }
}

/// The next piping layer, wrapping from the top to the bottom.
pub fn next_layer(layer: u8) -> u8 {
    if layer >= LAYER_MAX {
        LAYER_MIN
    } else {
        layer + 1
    }
}

/// Reconnects a unary machine facing `dir` on `layer` in `color`.
fn reconnect(world: &mut World, machine: Entity, dir: u8, layer: u8, color: [u8; 3]) -> bool {
    let Some(device) = world.get::<AtmosDevice>(machine).copied() else {
        return false;
    };
    let Some(new) =
        world
            .resource_mut::<Pipenet>()
            .reconnect(device.id, dir, vec![dir], layer, color)
    else {
        return false;
    };
    let mut e = world.entity_mut(machine);
    e.insert(AtmosDevice {
        id: new,
        layer,
        dir,
    });
    if let Some(facing) = Dir::from_bits(dir) {
        e.insert(Facing(facing));
    }
    true
}

fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<ss13_mapload::DmName>(e)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default()
}

/// The thermomachine's tool acts. True when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    target: Entity,
    tool: Option<ToolKind>,
    right: bool,
) -> bool {
    let Some(machine) = world.get::<ThermoMachine>(target) else {
        return false;
    };
    let (on, open) = (machine.on, machine.panel_open);
    let Some(device) = world.get::<AtmosDevice>(target).copied() else {
        return false;
    };
    let color = world
        .resource::<Pipenet>()
        .device(device.id)
        .map_or(ss13_atmos::pipenet::COLOR_OMNI, |d| d.color);
    match (tool, right) {
        // `screwdriver_act` -> `default_deconstruction_screwdriver`.
        (Some(ToolKind::Screwdriver), false) => {
            if on {
                tell(world, client, Span::Warning, "Сначала выключите!");
                return true;
            }
            if let Some(mut m) = world.get_mut::<ThermoMachine>(target) {
                m.panel_open = !open;
            }
            let text = if open {
                "панель обслуживания закрыта"
            } else {
                "панель обслуживания открыта"
            };
            tell(world, client, Span::Notice, text);
            true
        }
        // `default_change_direction_wrench`: only with the panel open.
        (Some(ToolKind::Wrench), false) if open => {
            let dir = clockwise(device.dir);
            if reconnect(world, target, dir, device.layer, color) {
                let text = format!("Вы поворачиваете {}.", name_of(world, target));
                tell(world, client, Span::Notice, &text);
            }
            true
        }
        (Some(ToolKind::Multitool), _) if !open => {
            tell(world, client, Span::Warning, "Сначала откройте панель!");
            true
        }
        // `multitool_act`.
        (Some(ToolKind::Multitool), false) => {
            let layer = next_layer(device.layer);
            if reconnect(world, target, device.dir, layer, color) {
                let text = format!("Вы переключаете плату на слой {layer}.");
                tell(world, client, Span::Notice, &text);
            }
            true
        }
        // `multitool_act_secondary`: the next of `pipe_paint_colors`, from `color_index`.
        (Some(ToolKind::Multitool), true) => {
            let index = (machine.color_index + 1) % PIPE_PAINT_COLORS.len();
            let (name, rgb) = PIPE_PAINT_COLORS[index];
            if let Some(mut m) = world.get_mut::<ThermoMachine>(target) {
                m.color_index = index;
            }
            if reconnect(world, target, device.dir, device.layer, rgb) {
                let text = format!(
                    "Вы окрашиваете трубу {} в цвет {name}.",
                    name_of(world, target)
                );
                tell(world, client, Span::Notice, &text);
            }
            true
        }
        _ => false,
    }
}

/// `thermomachine/examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(m) = world.get::<ThermoMachine>(target) else {
        return;
    };
    let num = |v: f64| crate::chem::dm_num(v as f32);
    // `(T0C - k) * -1`.
    let celsius = |k: f64| num(-(T0C - k));
    for line in [
        "С открытой панелью:",
        " — гаечный ключ (ЛКМ) поворачивает машину, (ПКМ) откручивает её;",
        " — мультитул (ЛКМ) меняет слой трубы, (ПКМ) — её цвет;",
        " — <b>Alt-клик</b> переключает диапазон температуры;",
        " — <b>Ctrl-клик</b> включает и выключает машину.",
    ] {
        lines.push((Span::Notice, line.to_owned()));
    }
    lines.push((
        Span::Notice,
        format!(
            "Термостат выставлен на {}K ({}C).",
            num(m.target_temperature),
            celsius(m.target_temperature)
        ),
    ));
    // shortcut: tg shows the next two only within reach; examine here has no viewer.
    lines.push((
        Span::Notice,
        format!("Теплоёмкость: <b>{} Дж/К</b>.", num(m.heat_capacity)),
    ));
    lines.push((
        Span::Notice,
        format!(
            "Диапазон температуры: <b>{}K - {}K ({}C - {}C)</b>.",
            num(m.min_temperature),
            num(m.max_temperature),
            celsius(m.min_temperature),
            celsius(m.max_temperature)
        ),
    ));
}

/// `update_icon_state` and `update_overlays` of every thermomachine whose look changed.
pub fn sync_thermo_looks(world: &mut World) {
    let mut q = world.query::<(
        Entity,
        &ThermoMachine,
        &AtmosDevice,
        &DmAtom,
        Has<Unpowered>,
        &Appearance,
    )>();
    let Some(icons) = world.get_resource::<IconIndex>() else {
        return;
    };
    let pipe_states = [
        icons.get(THERMO_DMI, "pipe"),
        icons.get(THERMO_DMI, "thermo-emissive"),
    ];
    let net = world.resource::<Pipenet>();
    let mut updates: Vec<(Entity, Appearance)> = Vec::new();
    for (e, m, device, atom, unpowered, appearance) in q.iter(world) {
        let state = if m.panel_open {
            "thermo-open"
        } else if m.on && !unpowered {
            "thermo_1"
        } else {
            "thermo_base"
        };
        let mut new = appearance.clone();
        if let Some(s) = icons.get(&atom.icon, state) {
            new.icon_state = s;
        }
        new.overlays.clear();
        if let (Some(pipe), Some(d)) = (pipe_states[0], net.device(device.id)) {
            // `SELECT_ATMOS_NODE_COLOR(src, nodes[1])`.
            let node = d
                .nodes
                .first()
                .copied()
                .flatten()
                .and_then(|n| net.device(n));
            let rgb = match node {
                Some(n)
                    if n.flags & ss13_atmos::pipenet::flags::DONT_SHARE_COLOR == 0
                        && n.color != ss13_atmos::pipenet::COLOR_OMNI =>
                {
                    n.color
                }
                _ => d.color,
            };
            // `PIPING_LAYER_SHIFT`.
            let delta = (i16::from(device.layer) - 3) * LAYER_SHIFT;
            let offset = match device.dir {
                1 | 2 => [delta, 0],
                _ => [0, delta],
            };
            new.overlays.push(Overlay {
                icon_state: pipe,
                dir: Dir::from_bits(device.dir),
                color: Some([rgb[0], rgb[1], rgb[2], 255]),
                pixel_offset: offset,
                plane: None,
                only_dirs: 0,
            });
        }
        if m.on
            && !unpowered
            && let Some(glow) = pipe_states[1]
        {
            new.overlays.push(Overlay {
                icon_state: glow,
                dir: None,
                color: None,
                pixel_offset: [0, 0],
                plane: Some(ss13_shared::EMISSIVE_PLANE),
                only_dirs: 0,
            });
        }
        if new != *appearance {
            updates.push((e, new));
        }
    }
    for (e, new) in updates {
        if let Some(mut a) = world.get_mut::<Appearance>(e) {
            *a = new;
        }
    }
}
