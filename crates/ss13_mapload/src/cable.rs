//! Power cables pick their sprite from their links (`code/modules/power/cable.dm`): maps place
//! `l2-1-2-4-8-node` everywhere, `connect_cable()` links each cable to cables of a shared layer on
//! the cardinal neighbors and `update_icon_state()` draws `l<layer>-<dirs>[-node]`, with a node
//! where a power machine or a grille sits on the tile.
// shortcut: computed once at load; cutting or laying cable must rerun it for the tile and its
// neighbors when that exists. The SMES/terminal link exception is not reproduced.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use ss13_shared::{Appearance, TilePos};

use crate::DmAtom;
use crate::convert::DmTypes;
use crate::fidelity::{IconIndex, ext, is_under, num};
use crate::value::Field;

const COMPONENT: &str = "dm_cable";
const CABLE_ROOT: &str = "/obj/structure/cable";
/// Draws its own hub sprite.
const MULTILAYER: &str = "/obj/structure/cable/multilayer";
/// `GLOB.wire_node_generating_types` and the `/obj/machinery/power` types whose
/// `should_have_node()` is true at round start (anchored generators included).
const NODE_ROOTS: [&str; 8] = [
    "/obj/structure/grille",
    "/obj/structure/table/reinforced",
    "/obj/machinery/power/smes",
    "/obj/machinery/power/terminal",
    "/obj/machinery/power/solar",
    "/obj/machinery/power/port_gen",
    "/obj/machinery/power/energy_accumulator",
    "/obj/machinery/power/shieldwallgen",
];
/// `GLOB.cardinals` order, which is the order of the dirs in the state name.
const CARDINALS: [(u8, i32, i32); 4] = [(1, 0, 1), (2, 0, -1), (4, 1, 0), (8, -1, 0)];

/// Cable data of a type (prototype data). `layer == 0`: not a cable.
#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
pub struct DmCable {
    /// `cable_layer` bits.
    pub layer: u8,
    /// `banned_links` dir bits.
    pub banned_links: u8,
    /// Puts a node on cables of its tile.
    pub node: bool,
}

/// Converter fields: cable vars on cable types, `node` on node makers.
pub fn type_fields(types: &DmTypes, path: &str) -> Vec<Field> {
    let node = NODE_ROOTS.iter().any(|r| is_under(path, r));
    let cable = is_under(path, CABLE_ROOT) && !is_under(path, MULTILAYER);
    if !node && !cable {
        return Vec::new();
    }
    let var = |v| num(types.resolved(path, v)).unwrap_or(0.0) as u8;
    let (layer, banned) = if cable {
        (var("cable_layer"), var("banned_links"))
    } else {
        (0, 0)
    };
    vec![
        ext(COMPONENT, "layer", layer.to_string()),
        ext(COMPONENT, "banned_links", banned.to_string()),
        ext(COMPONENT, "node", node.to_string()),
    ]
}

/// `get_dir_string`.
pub fn state(layer: u8, links: u8, node: bool) -> String {
    if links == 0 {
        return format!("l{layer}-noconnection");
    }
    let dirs: Vec<String> = CARDINALS
        .iter()
        .filter(|(d, ..)| links & d != 0)
        .map(|(d, ..)| d.to_string())
        .collect();
    let suffix = if dirs.len() > 1 && node { "-node" } else { "" };
    format!("l{layer}-{}{suffix}", dirs.join("-"))
}

/// Links every cable and sets its sprite. Returns how many were changed and how many states
/// the manifest lacks.
pub fn connect_all(world: &mut World, icons: &IconIndex) -> (usize, usize) {
    let mut query =
        world.query_filtered::<(Entity, &TilePos, &DmCable), With<ss13_world::Mapped>>();
    let mut cables: HashMap<TilePos, Vec<(Entity, u8, u8)>> = HashMap::new();
    let mut nodes = HashSet::new();
    for (e, &pos, c) in query.iter(world) {
        if c.node {
            nodes.insert(pos);
        }
        if c.layer != 0 {
            cables
                .entry(pos)
                .or_default()
                .push((e, c.layer, c.banned_links));
        }
    }
    let mut updates = Vec::new();
    for (&pos, list) in &cables {
        for &(e, layer, banned) in list {
            let mut links = 0;
            for &(dir, dx, dy) in &CARDINALS {
                if banned & dir != 0 {
                    continue;
                }
                let inverse = match dir {
                    1 => 2,
                    2 => 1,
                    4 => 8,
                    _ => 4,
                };
                let next = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
                let linked = cables.get(&next).is_some_and(|other| {
                    other
                        .iter()
                        .any(|&(_, l, b)| l & layer != 0 && b & inverse == 0)
                });
                if linked {
                    links |= dir;
                }
            }
            let node = banned != 0 || nodes.contains(&pos);
            updates.push((e, state(layer, links, node)));
        }
    }
    let (mut changed, mut missing) = (0, 0);
    for (e, state) in updates {
        let Some(icon) = world.get::<DmAtom>(e).map(|a| a.icon.clone()) else {
            continue;
        };
        let Some(id) = icons.get(&icon, &state) else {
            missing += 1;
            continue;
        };
        if let Some(mut appearance) = world.get_mut::<Appearance>(e)
            && appearance.icon_state != id
        {
            appearance.icon_state = id;
            changed += 1;
        }
    }
    (changed, missing)
}

#[cfg(test)]
mod tests {
    use ss13_shared::IconStateId;

    use super::*;

    #[test]
    fn states_match_get_dir_string() {
        assert_eq!(state(2, 0, true), "l2-noconnection");
        assert_eq!(state(2, 1, true), "l2-1");
        assert_eq!(state(1, 1 | 2, false), "l1-1-2");
        assert_eq!(state(4, 1 | 2 | 4 | 8, true), "l4-1-2-4-8-node");
        assert_eq!(state(2, 4 | 8, true), "l2-4-8-node");
    }

    fn appearance() -> Appearance {
        Appearance {
            icon_state: IconStateId(0),
            layer: ss13_shared::Layer(2.0),
            plane: 0,
            color: None,
            pixel_offset: [0, 0],
            overlays: Vec::new(),
            rotation: 0,
            mouse_opacity: Default::default(),
        }
    }

    #[test]
    fn cables_link_on_shared_layers_with_nodes_under_machines() {
        let icon = "icons/obj/pipes_n_cables/layer_cable.dmi";
        let names = [
            "l2-1-2-4-8-node",
            "l2-4",
            "l2-4-8-node",
            "l2-4-8",
            "l2-8",
            "l1-noconnection",
        ];
        let icons = IconIndex(
            names
                .iter()
                .enumerate()
                .map(|(i, n)| ((icon.to_owned(), (*n).to_owned()), IconStateId(i as u32)))
                .collect(),
        );
        let mut world = World::new();
        let mut cable = |x, layer, node| {
            let atom = DmAtom {
                icon: icon.into(),
                ..default()
            };
            let c = DmCable {
                layer,
                banned_links: 0,
                node,
            };
            world
                .spawn((
                    atom,
                    c,
                    appearance(),
                    TilePos::new(x, 1, 1),
                    ss13_world::Mapped,
                ))
                .id()
        };
        // Layer-2 run x = 1..=3, a node maker on x = 2, a layer-1 cable on x = 4.
        let run = [cable(1, 2, false), cable(2, 2, false), cable(3, 2, false)];
        let lone = cable(4, 1, false);
        let grille = DmCable {
            layer: 0,
            banned_links: 0,
            node: true,
        };
        world.spawn((grille, TilePos::new(2, 1, 1), ss13_world::Mapped));
        assert_eq!(connect_all(&mut world, &icons), (4, 0));
        let state_of = |e| names[world.get::<Appearance>(e).unwrap().icon_state.0 as usize];
        assert_eq!(run.map(state_of), ["l2-4", "l2-4-8-node", "l2-8"]);
        assert_eq!(state_of(lone), "l1-noconnection");
    }
}
