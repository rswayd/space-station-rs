//! Marker beacons (server), after tg `/obj/item/stack/marker_beacon` and
//! `/obj/structure/marker_beacon` (`code/modules/mining/equipment/marker_beacons.dm`).
//!
//! - The stack used in hand anchors one beacon on the user's tile (one per tile), in the
//!   stack's colour; "Random" picks one of the colours.
//! - A beacon glows (range 2, power 3) in its colour and shows it in its sprite
//!   (`marker<colour>-on`); the stack's sprite follows its colour too.
//! - Alt-click on either asks for a colour (`tgui_input_list` of `GLOB.marker_beacon_colors`).
//! - An empty hand picks a beacon up into a new stack after 1.5 s; a beacon stack collects it
//!   into itself after 1.5 s while below 100.
//! - Mapped presets (`/obj/structure/marker_beacon/<colour>`) take their colour from the type.
// shortcut: a destroyed beacon drops nothing (`atom_deconstruct`), no light eater, no cyborg
// beacon synthesizer (`/datum/robot_energy_storage/beacon`).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::light::DmLight;
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::light::Optics;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_world::SpatialIndex;

use super::MiningJob;
use crate::examine::Line;
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, DoAfter, Stack, do_after, stack_amount, tell, use_stack};

pub const STACK: &str = "/obj/item/stack/marker_beacon";
pub const BEACON: &str = "/obj/structure/marker_beacon";
/// `max_amount`.
const MAX_AMOUNT: u32 = 100;
/// `remove_speed`, deciseconds.
const REMOVE_SPEED: f32 = 15.0;
const RANDOM: &str = "Random";
/// `GLOB.marker_beacon_colors` without "Random" (`LIGHT_COLOR_*`), sorted by name.
pub const COLORS: [(&str, [u8; 3]); 12] = [
    ("Bronze", [0xFA, 0x96, 0x32]),
    ("Burgundy", [0xFA, 0x64, 0x4B]),
    ("Cerulean", [0x64, 0x96, 0xFA]),
    ("Fuchsia", [0xE1, 0x7D, 0xE1]),
    ("Indigo", [0x64, 0x96, 0xFA]),
    ("Jade", [0x7D, 0xE1, 0xAF]),
    ("Lime", [0xAF, 0xC8, 0x4B]),
    ("Olive", [0x64, 0xC8, 0x64]),
    ("Purple", [0x95, 0x2C, 0xF4]),
    ("Teal", [0x40, 0xCE, 0xFF]),
    ("Violet", [0x9B, 0x51, 0xFF]),
    ("Yellow", [0xE1, 0xE1, 0x7D]),
];

/// `picked_color` of a beacon stack or a beacon.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct MarkerColor(pub String);

/// On a prompt: the stack or beacon it picks the colour of.
#[derive(Component, Clone, Copy, Debug)]
struct ColorFor(Entity);

pub(super) struct BeaconsPlugin;

impl Plugin for BeaconsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(ss13_world::MapInit, setup_mapped)
            .add_systems(
                Update,
                color_answers
                    .in_set(ss13_tgui::server::UiSystems::Handle)
                    .after(ss13_tgui::prompt::PromptSystems::Answer),
            );
    }
}

/// The alt-click list: `sort_list(GLOB.marker_beacon_colors)` keys.
pub fn color_list() -> Vec<String> {
    let mut items: Vec<String> = COLORS.iter().map(|(n, _)| (*n).to_owned()).collect();
    items.push(RANDOM.to_owned());
    items.sort();
    items
}

fn color_rgb(name: &str) -> Option<[u8; 3]> {
    COLORS.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

fn path_of(world: &World, e: Entity) -> Option<String> {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.clone())
        .or_else(|| world.get::<DmObj>(e).map(|o| o.path.clone()))
}

fn is_stack(world: &World, e: Entity) -> bool {
    path_of(world, e).is_some_and(|p| is_under(&p, STACK))
}

fn is_beacon(world: &World, e: Entity) -> bool {
    path_of(world, e).is_some_and(|p| is_under(&p, BEACON))
}

fn picked(world: &World, e: Entity) -> String {
    world
        .get::<MarkerColor>(e)
        .map_or_else(|| RANDOM.to_owned(), |c| c.0.clone())
}

/// The preset colour of a mapped beacon type (`/obj/structure/marker_beacon/jade`).
pub fn preset(path: &str) -> Option<&'static str> {
    let last = path.strip_prefix(BEACON)?.strip_prefix('/')?;
    COLORS
        .iter()
        .map(|(n, _)| *n)
        .find(|n| n.eq_ignore_ascii_case(last))
}

fn setup_mapped(world: &mut World) {
    let mapped: Vec<(Entity, String)> = world
        .query_filtered::<(Entity, &DmObj), (With<ss13_world::Mapped>, Without<MarkerColor>)>()
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, BEACON))
        .map(|(e, o)| (e, o.path.clone()))
        .collect();
    for (e, path) in mapped {
        set_beacon_color(world, e, preset(&path).unwrap_or(RANDOM));
    }
}

/// The stack's `update_icon_state`: `marker<colour>`.
fn set_stack_color(world: &mut World, stack: Entity, color: &str) {
    world
        .entity_mut(stack)
        .insert(MarkerColor(color.to_owned()));
    let state = format!("marker{}", color.to_lowercase());
    crate::obj_damage::set_icon_state(world, stack, &state);
}

/// The beacon's `update_appearance`: "Random" (or nothing) rolls a real colour, then the light
/// and the `marker<colour>-on` sprite follow it.
pub fn set_beacon_color(world: &mut World, beacon: Entity, color: &str) {
    let color = if color_rgb(color).is_some() {
        color.to_owned()
    } else {
        let i = world
            .resource_mut::<crate::combat::CombatRng>()
            .range(0, COLORS.len() as u32 - 1) as usize;
        COLORS[i].0.to_owned()
    };
    let rgb = color_rgb(&color).expect("a listed colour");
    let state = format!("marker{}-on", color.to_lowercase());
    crate::obj_damage::set_icon_state(world, beacon, &state);
    world.entity_mut(beacon).insert(MarkerColor(color));
    // `set_light(light_range, light_power, colour)`.
    let base = world.get::<DmLight>(beacon).cloned();
    let light = world
        .get::<Optics>(beacon)
        .and_then(|o| o.light)
        .or_else(|| base.and_then(|l| ss13_mapload::light::source(BEACON, &l, 2, [0, 0])));
    let Some(mut light) = light else {
        return;
    };
    light.color = rgb;
    let mut entity = world.entity_mut(beacon);
    match entity.get_mut::<Optics>() {
        Some(mut o) => o.light = Some(light),
        None => {
            entity.insert(Optics {
                light: Some(light),
                ..default()
            });
        }
    }
}

/// The stack's `attack_self`: anchor a beacon on the user's tile. True when `item` is a beacon
/// stack.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if !is_stack(world, item) {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        tell(
            world,
            client,
            Span::Warning,
            "You need more space to place a marker beacon here.",
        );
        return true;
    };
    let taken = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| is_beacon(world, e));
    if taken {
        tell(
            world,
            client,
            Span::Warning,
            "There is already a marker beacon here.",
        );
        return true;
    }
    let color = picked(world, item);
    if !use_stack(world, item, 1) {
        return true;
    }
    let left = world.get_entity(item).is_ok();
    tell(
        world,
        client,
        Span::Notice,
        format!(
            "You activate and anchor {} marker beacon in place.",
            if left { "a" } else { "the" }
        ),
    );
    playsound(
        world,
        PlaySound::new(pos, "sound/machines/click.ogg", 50.0, true),
    );
    if let Some(beacon) = crate::construction::spawn_obj(world, BEACON, pos) {
        set_beacon_color(world, beacon, &color);
    }
    true
}

/// Alt-click on a stack or a beacon: ask for a colour. True when handled.
pub fn click_alt(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if !is_stack(world, target) && !is_beacon(world, target) {
        return false;
    }
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Beacon Color",
            "Choose a color",
            PromptKind::List {
                items: color_list(),
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(ColorFor(target));
    true
}

/// The picks, revalidated (`can_perform_action`: conscious, the thing still in reach).
fn color_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, actor, answer) in answers {
        let Some(&ColorFor(target)) = world.get::<ColorFor>(prompt) else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let Some(color) = color_list().into_iter().nth(i) else {
            continue;
        };
        let held = world
            .get::<crate::inventory::CarriedBy>(target)
            .is_some_and(|c| c.0 == actor);
        let able = world.get_entity(target).is_ok()
            && world.get::<crate::health::Stat>(actor) == Some(&crate::health::Stat::Conscious)
            && (held || crate::storage::in_reach(world, actor, target));
        if !able {
            continue;
        }
        if is_beacon(world, target) {
            set_beacon_color(world, target, &color);
        } else if is_stack(world, target) {
            set_stack_color(world, target, &color);
        }
    }
}

/// The beacon's `attack_hand`: start picking it up. True when `target` is a beacon.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if !is_beacon(world, target) {
        return false;
    }
    tell(
        world,
        client,
        Span::Notice,
        "You start picking marker beacon up...",
    );
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    do_after(
        world,
        actor,
        target,
        REMOVE_SPEED,
        0,
        crate::construction::Job::Mining(MiningJob::PickUpBeacon),
    );
    true
}

/// A beacon stack on a beacon: start collecting it. True when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if !is_stack(world, held) || !is_beacon(world, target) {
        return false;
    }
    tell(
        world,
        client,
        Span::Notice,
        "You start picking marker beacon up...",
    );
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    do_after(
        world,
        actor,
        target,
        REMOVE_SPEED,
        0,
        crate::construction::Job::Mining(MiningJob::CollectBeacon),
    );
    true
}

/// The end of a pickup (`job`): the beacon becomes one unit in a new stack in the user's hands,
/// or one more unit of the stack used.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: MiningJob) {
    let beacon = action.target;
    if !is_beacon(world, beacon) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(beacon) else {
        return;
    };
    let color = picked(world, beacon);
    match job {
        MiningJob::CollectBeacon => {
            let Some(stack) = action.item.filter(|&s| is_stack(world, s)) else {
                return;
            };
            // `collection.amount + 1 > collection.max_amount`.
            if stack_amount(world, stack) + 1 > MAX_AMOUNT {
                return;
            }
            if let Some(mut s) = world.get_mut::<Stack>(stack) {
                s.amount += 1;
            }
        }
        MiningJob::PickUpBeacon => {
            let Some(stack) = crate::construction::spawn_detached(world, STACK) else {
                return;
            };
            world.entity_mut(stack).insert(Stack { amount: 1 });
            set_stack_color(world, stack, &color);
            match world.get::<crate::inventory::Inventory>(mob).cloned() {
                Some(inventory) => crate::storage::put_in_hands(world, mob, &inventory, stack),
                None => {
                    crate::storage::take_out(world, stack, pos);
                }
            }
        }
        _ => return,
    }
    playsound(
        world,
        PlaySound::new(pos, "sound/items/deconstruct.ogg", 50.0, true),
    );
    world.despawn(beacon);
}

/// "Use in-hand to place a marker beacon." / "Alt-click to select a color. Current color is X."
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<Line>) {
    let color = picked(world, target);
    if is_stack(world, target) {
        lines.push((
            Span::Notice,
            "Use in-hand to place a marker beacon.".to_owned(),
        ));
    } else if !is_beacon(world, target) {
        return;
    }
    lines.push((
        Span::Notice,
        format!("Alt-click to select a color. Current color is {color}."),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_list_is_sorted_with_random() {
        let list = color_list();
        assert_eq!(list.len(), 13);
        assert_eq!(list.first().map(String::as_str), Some("Bronze"));
        assert!(list.contains(&RANDOM.to_owned()));
        assert!(list.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn mapped_presets_take_the_type_colour() {
        assert_eq!(preset("/obj/structure/marker_beacon/jade"), Some("Jade"));
        assert_eq!(
            preset("/obj/structure/marker_beacon/fuchsia"),
            Some("Fuchsia")
        );
        assert_eq!(preset(BEACON), None);
        assert_eq!(color_rgb("Teal"), Some([0x40, 0xCE, 0xFF]));
        assert_eq!(color_rgb(RANDOM), None);
    }

    #[test]
    fn random_beacons_roll_a_real_colour() {
        let mut world = World::new();
        world.insert_resource(crate::combat::CombatRng::default());
        let beacon = world.spawn_empty().id();
        set_beacon_color(&mut world, beacon, RANDOM);
        let c = world.get::<MarkerColor>(beacon).unwrap().0.clone();
        assert!(color_rgb(&c).is_some());
        set_beacon_color(&mut world, beacon, "Lime");
        assert_eq!(world.get::<MarkerColor>(beacon).unwrap().0, "Lime");
    }
}
