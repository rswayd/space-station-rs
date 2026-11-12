//! The food cart (server): tg `/obj/machinery/food_cart` and `/obj/effect/food_cart_stand`
//! (`food_and_drinks/machinery/food_cart.dm`).
//!
//! - A cart carries a griddle stand, a food smart fridge, a reinforced table and a tent, packed
//!   inside it (made at map load or on first use).
//! - An empty hand with kitchen access (else the buzz) works 5 s, then unpacks or packs it up.
//!   Unpacking needs the three turfs south-east, south and south of the east neighbour free of
//!   dense turfs; the table goes south of the east turf, the fridge and the tent south of the
//!   cart, the griddle south-east of the east turf, and the cart anchors. Packing pulls them all
//!   back in and unanchors it.
//! - A part that leaves its spot or is destroyed while unpacked breaks the cart: it packs up,
//!   the rest of its parts are deleted and it is useless from then on.
// shortcut: no red/green markers of the unpacking spots (`cart_space`); a moved or lost part is
// noticed on the next machine tick (2 s) rather than at once.

use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_reagents::is_under;
use ss13_shared::chat::Span;
use ss13_shared::{Dir, TilePos};

use crate::chem::name_of;
use crate::construction::Job;
use crate::food::FoodJob;
use crate::power::MachineTicks;
use crate::sound::{PlaySound, playsound};
use crate::storage::{StoredIn, put_inside, take_out};
use crate::tools::{Actor, DoAfter, do_after, tell};

pub const FOOD_CART: &str = "/obj/machinery/food_cart";
/// `packed_things`: middle, left, left, right.
const PARTS: [&str; 4] = [
    "/obj/structure/table/reinforced",
    "/obj/machinery/smartfridge/food",
    "/obj/effect/food_cart_stand",
    "/obj/machinery/griddle/stand",
];
/// `turn(SOUTH, angle)` for each part, from the cart's east neighbour.
const PART_DIRS: [Dir; 4] = [Dir::South, Dir::SouthWest, Dir::SouthWest, Dir::SouthEast];
/// `ACCESS_KITCHEN`.
const ACCESS: &str = "kitchen";
/// `do_after(user, 5 SECONDS)`, deciseconds.
const WORK_DS: f32 = 50.0;
const BUZZ: &str = "sound/machines/buzz/buzz-sigh.ogg";

/// A cart's parts (in `PARTS` order), whether they are out, and `BROKEN`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct FoodCart {
    pub parts: Vec<Entity>,
    pub unpacked: bool,
    pub broken: bool,
}

pub struct FoodCartPlugin;

impl Plugin for FoodCartPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // `attack_hand`: pack or unpack.
        app.add_click_handler(ClickPhase::EmptyHand, 561, |w, c| {
            crate::food_cart::attack_hand(w, c.client, c.mob, c.target)
        });
        app.add_systems(
            ss13_world::MapInit,
            setup_carts.after(crate::power::init_power),
        )
        .add_systems(Update, watch_parts.after(crate::power::PowerSystems));
    }
}

fn is_cart(world: &World, e: Entity) -> bool {
    world
        .get::<DmObj>(e)
        .is_some_and(|o| is_under(&o.path, FOOD_CART))
}

fn setup_carts(world: &mut World) {
    let carts: Vec<Entity> = world
        .query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>()
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, FOOD_CART))
        .map(|(e, _)| e)
        .collect();
    for cart in carts {
        ensure_parts(world, cart);
    }
}

/// `Initialize`: the parts made inside the cart.
fn ensure_parts(world: &mut World, cart: Entity) {
    if world.get::<FoodCart>(cart).is_some() {
        return;
    }
    let mut parts = Vec::new();
    for path in PARTS {
        let Some(part) = crate::construction::spawn_detached(world, path) else {
            continue;
        };
        // The griddle and the fridge get the live state a built machine gets.
        crate::machine_frame::setup_built(world, part);
        put_inside(world, part, cart);
        parts.push(part);
    }
    world
        .entity_mut(cart)
        .insert(FoodCart { parts, ..default() });
}

/// The spots of the parts (`PART_DIRS` from the east neighbour).
pub fn part_spots(cart: TilePos) -> [TilePos; 4] {
    let east = cart.step(Dir::East);
    PART_DIRS.map(|d| east.step(d))
}

/// `attack_hand`. True when the click was the cart's.
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if !is_cart(world, target) {
        return false;
    }
    ensure_parts(world, target);
    let cart = world.get::<FoodCart>(target).cloned().unwrap_or_default();
    let name = name_of(world, target);
    if cart.broken {
        tell(
            world,
            client,
            Span::Warning,
            format!("{} полностью разрушена.", ss13_chat::capitalize(&name)),
        );
        return true;
    }
    if !crate::inventory::id_allowed(world, mob, &[ACCESS.to_owned()], &[]) {
        if let Some(&pos) = world.get::<TilePos>(target) {
            playsound(world, PlaySound::new(pos, BUZZ, 30.0, true));
        }
        return true;
    }
    if world.get::<DoAfter>(mob).is_some() {
        return true;
    }
    let verb = if cart.unpacked {
        "разобрать"
    } else {
        "распаковать"
    };
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы пытаетесь {verb} {name}..."),
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
        WORK_DS,
        0,
        Job::Food(FoodJob::FoodCart),
    );
    true
}

/// The 5 s are up: pack or unpack, as the cart is now.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter) {
    let cart = action.target;
    let Some(state) = world.get::<FoodCart>(cart).cloned() else {
        return;
    };
    if state.broken {
        return;
    }
    if state.unpacked {
        pack_up(world, cart);
    } else {
        unpack(world, action.client, mob, cart);
    }
}

fn turf_dense(world: &World, pos: TilePos) -> bool {
    crate::construction::turf_at(world, pos)
        .is_none_or(|t| world.get::<ss13_world::Density>(t).is_some())
}

/// `check_setup_place`: no dense turf where the table, fridge and griddle go.
fn has_space(world: &World, cart: TilePos) -> bool {
    let spots = part_spots(cart);
    [spots[0], spots[1], spots[3]]
        .iter()
        .all(|&p| !turf_dense(world, p))
}

/// `unpack`.
fn unpack(world: &mut World, client: Entity, _mob: Entity, cart: Entity) {
    let Some(&pos) = world.get::<TilePos>(cart) else {
        return;
    };
    if !has_space(world, pos) {
        tell(
            world,
            client,
            Span::Warning,
            "Здесь не хватит места, чтобы все распаковать!",
        );
        return;
    }
    let name = ss13_chat::capitalize(&name_of(world, cart));
    crate::chem::visible(
        world,
        cart,
        "",
        &format!("{name} расширяется, превращаясь в полноценную стойку."),
        Span::Notice,
    );
    crate::obj_damage::set_anchored(world, cart, true);
    let parts = world
        .get::<FoodCart>(cart)
        .map(|c| c.parts.clone())
        .unwrap_or_default();
    for (part, spot) in parts.into_iter().zip(part_spots(pos)) {
        take_out(world, part, spot);
        crate::construction::tile_changed(world, spot);
    }
    if let Some(mut c) = world.get_mut::<FoodCart>(cart) {
        c.unpacked = true;
    }
}

/// `pack_up`.
fn pack_up(world: &mut World, cart: Entity) {
    let Some(state) = world.get::<FoodCart>(cart).cloned() else {
        return;
    };
    if !state.unpacked {
        return;
    }
    let name = ss13_chat::capitalize(&name_of(world, cart));
    crate::chem::visible(
        world,
        cart,
        "",
        &format!("{name} втягивает все свои распакованные части."),
        Span::Notice,
    );
    for part in state.parts {
        if world.get_entity(part).is_err() {
            continue;
        }
        let at = world.get::<TilePos>(part).copied();
        put_inside(world, part, cart);
        if let Some(at) = at {
            crate::construction::tile_changed(world, at);
        }
    }
    crate::obj_damage::set_anchored(world, cart, false);
    if let Some(mut c) = world.get_mut::<FoodCart>(cart) {
        c.unpacked = false;
    }
}

/// `lost_part` -> `atom_break`: packs up, deletes what is left of the parts.
fn break_cart(world: &mut World, cart: Entity) {
    pack_up(world, cart);
    let parts = world
        .get::<FoodCart>(cart)
        .map(|c| c.parts.clone())
        .unwrap_or_default();
    for part in parts {
        if world.get_entity(part).is_ok() {
            world.despawn(part);
        }
    }
    if let Some(mut c) = world.get_mut::<FoodCart>(cart) {
        c.parts.clear();
        c.broken = true;
    }
}

/// `COMSIG_MOVABLE_MOVED` / `COMSIG_QDELETING` of the parts: an unpacked cart whose part is
/// gone or off its spot breaks; a packed one whose part was deleted breaks too.
fn watch_parts(world: &mut World, mut last: Local<Option<u64>>) {
    let ticks = world.resource::<MachineTicks>().0;
    if *last == Some(ticks) {
        return;
    }
    *last = Some(ticks);
    let carts: Vec<(Entity, FoodCart)> = world
        .query::<(Entity, &FoodCart)>()
        .iter(world)
        .filter(|(_, c)| !c.broken)
        .map(|(e, c)| (e, c.clone()))
        .collect();
    for (cart, state) in carts {
        let Some(&pos) = world.get::<TilePos>(cart) else {
            continue;
        };
        let lost = state.parts.len() < PARTS.len()
            || state
                .parts
                .iter()
                .zip(part_spots(pos))
                .any(|(&part, spot)| {
                    if world.get_entity(part).is_err() {
                        return true;
                    }
                    if state.unpacked {
                        world.get::<TilePos>(part) != Some(&spot)
                    } else {
                        world.get::<StoredIn>(part).is_none_or(|s| s.0 != cart)
                    }
                });
        if lost {
            break_cart(world, cart);
        }
    }
}

/// `food_cart/examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(cart) = world.get::<FoodCart>(target) else {
        return;
    };
    if cart.broken {
        return;
    }
    lines.push((Span::Notice, "Гриль стойки целый.".to_owned()));
    lines.push((Span::Notice, "Холодильник стойки целый.".to_owned()));
    lines.push((Span::Notice, "Стол стойки целый.".to_owned()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_go_south_of_the_east_neighbour() {
        let cart = TilePos { x: 10, y: 10, z: 0 };
        let [table, fridge, tent, griddle] = part_spots(cart);
        assert_eq!(table, cart.step(Dir::East).step(Dir::South));
        assert_eq!(fridge, cart.step(Dir::South));
        assert_eq!(tent, fridge);
        assert_eq!(
            griddle,
            cart.step(Dir::East).step(Dir::East).step(Dir::South)
        );
    }
}
