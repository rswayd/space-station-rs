//! Cooking machines (server): tg `/obj/machinery/oven` (`food_and_drinks/machinery/oven.dm`,
//! `/datum/component/bakeable`), `/obj/machinery/griddle` (`griddle.dm`,
//! `/datum/component/grillable`) and `/obj/machinery/deepfryer` (`deep_fryer.dm`,
//! `/datum/element/fried_item`).
//!
//! - Oven (and range): starts with a tray (map load). An empty-hand click opens or closes it; open,
//!   food goes onto the tray (6 at most), a storage loads its food (`handle_dumping`), a right
//!   click takes the tray out and a held tray goes in. Closed, with food on the tray and power,
//!   every machine tick (2 s) bakes each item by 2 s; past its `bake_time` (rolled once) it
//!   becomes its `bake_result` with its reagents (anything else into a burned mess, `bake_good`
//!   false).
//! - Griddle: out of combat mode any item clicked on it lies on it at the clicked point (8 at
//!   most); a storage loads its food. An empty hand switches it on or off. On, every machine tick
//!   grills each food on it by 2 s into its `grill_result` the same way.
//! - Deep fryer: 25 units of cooking oil to start (an open container pours more). Out of combat
//!   mode an item goes in when there is fat or oil and nothing is frying; every machine tick it
//!   soaks oil (`oil_use * 2 s`, three times as much per `fry_speed`) and cooks
//!   `fry_speed * 2 s`. An empty hand takes it out: it becomes fried (name, color) and edible.
//!   `RefreshParts`: `fry_speed` is the micro-lasers' tier sum, `oil_use = 0.025 - 0.00475 *
//!   fry_speed`.
//! - What nothing bakes or grills burns (`fire_act(1000)`, [`scorch`]): 20 damage a tick until
//!   it is ash; a griddle uses its active power only for those, as tg does, and exposes its tile
//!   to 800 K.
// shortcut: no sounds, smoke particles, emissive light, grilling by hot air, the `burning`
// component of burning items, pancake batter on a griddle, freeze cubes, dunking mobs in the
// fryer, the fryer's fishing spot or its `chem_temp`; the tray and griddle contents are drawn as
// their base sprites. The range's burner is `crate::stove`.

use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmFood, DmItem, DmName, DmPower, IconIndex};
use ss13_reagents::{Holder, Owner, flags, is_under};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};

use crate::chem::{Chem, Reagents, name_of, visible};
use crate::combat::{CombatMode, CombatRng};
use crate::inventory::{CarriedBy, Inventory};
use crate::power::{MACHINE_TICK_SECS, MachineTicks, Unpowered};
use crate::storage::{Stored, put_inside, take_out};
use crate::tools::tell;

pub const OVEN_ROOT: &str = ss13_mapload::food::OVEN;
pub const GRIDDLE_ROOT: &str = "/obj/machinery/griddle";
pub const FRYER_ROOT: &str = "/obj/machinery/deepfryer";
const OVEN_TRAY: &str = ss13_mapload::food::OVEN_TRAY;
const GRIDDLE_STAND: &str = "/obj/machinery/griddle/stand";
/// `/obj/item/plate/oven_tray` `max_items`, `/obj/machinery/griddle` `max_items`.
pub const TRAY_MAX_ITEMS: usize = 6;
pub const GRIDDLE_MAX_ITEMS: usize = 8;
/// `OVEN_TRAY_X_OFFSET`, `OVEN_TRAY_Y_OFFSET`, `OVEN_LID_Y_OFFSET`.
const TRAY_OFFSET: [i16; 2] = [-2, -16];
const LID_OFFSET: i16 = -15;
/// `FRYING_TIME_FRIED`, `_PERFECT`, `_BURNT`, `_WARNING`, deciseconds.
pub const FRYING_TIME_FRIED: f32 = 150.0;
pub const FRYING_TIME_PERFECT: f32 = 500.0;
pub const FRYING_TIME_BURNT: f32 = 850.0;
pub const FRYING_TIME_WARNING: f32 = 1200.0;
/// `/obj/machinery/deepfryer` `oil_use`, `grease_increase_chance`, `grease_Increase_amount`.
const OIL_USE: f32 = 0.025;
const GREASE_CHANCE: f32 = 50.0;
const GREASE_AMOUNT: f32 = 0.1;
const FAT: &str = "/datum/reagent/consumable/nutriment/fat";
const COOKING_OIL: &str = "/datum/reagent/consumable/nutriment/fat/oil";
/// `deepfry_blacklisted_items` and `GLOB.oilfry_blacklisted_items`.
const FRY_BLACKLIST: [&str; 15] = [
    "/obj/item/screwdriver",
    "/obj/item/crowbar",
    "/obj/item/wrench",
    "/obj/item/wirecutters",
    "/obj/item/multitool",
    "/obj/item/weldingtool",
    "/obj/item/bodybag/bluespace",
    "/obj/item/delivery",
    "/obj/item/his_grace",
    "/obj/item/mod/control",
    "/obj/item/reagent_containers/condiment",
    "/obj/item/reagent_containers/cup",
    "/obj/item/reagent_containers/syringe",
    "/obj/item/reagent_containers/hypospray/medipen",
    "/obj/item/slimecrossbeaker/autoinjector",
];
/// `fried_colors`: `COLOR_LIGHT_BROWN`, `COLOR_BROWNER_BROWN`, `COLOR_DARKER_BROWN`,
/// `COLOR_BLACK` (tg applies them as saturation-override color matrices).
// shortcut: a plain multiply by the color instead of `color_transition_filter`.
const FRIED_COLORS: [[u8; 4]; 4] = [
    [0x99, 0x66, 0x33, 255],
    [0x66, 0x33, 0x00, 255],
    [0x33, 0x00, 0x00, 255],
    [0x00, 0x00, 0x00, 255],
];

/// An oven's live state.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Oven {
    pub open: bool,
    pub tray: Option<Entity>,
}

/// A griddle's live state: what lies on it (on its tile, `griddled_objects`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Griddle {
    pub on: bool,
    pub items: Vec<Entity>,
    /// `variant`: 1-3, rolled at spawn; `None` for the stand.
    pub variant: Option<u8>,
}

/// A deep fryer's live state.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct DeepFryer {
    pub frying: Option<Entity>,
    /// Deciseconds.
    pub cook_time: f32,
    pub fried: bool,
    pub burnt: bool,
    pub grease: f32,
}

/// How long an item has baked or grilled and how long it needs (rolled the first time).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct CookTimes {
    pub baked: f32,
    pub bake_required: Option<f32>,
    pub grilled: f32,
    pub grill_required: Option<f32>,
}

pub struct CookingServerPlugin;

impl Plugin for CookingServerPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `attack_hand` of ovens, griddles and fryers; the soda and booze dispensers.
        app.add_click_handler(ClickPhase::EmptyHand, 600, |w, c| {
            crate::cooking::interact(w, c.client, c.mob, c.target, c.modifiers.right)
        });
        // Food on a mob (eating) and tools on food (slicing); before chem.
        app.add_click_handler(ClickPhase::HeldOnTarget, 1900, |w, c| {
            c.held.is_some_and(|h| {
                crate::cooking::item_interaction(w, c.client, c.mob, h, c.target, c.modifiers.pixel)
            })
        });
        app.add_systems(
            ss13_world::MapInit,
            setup_machines.after(crate::power::init_power),
        )
        .add_systems(Update, cooking_tick.after(crate::power::PowerSystems));
    }
}

fn path_of(world: &World, e: Entity) -> Option<String> {
    world
        .get::<DmPower>(e)
        .map(|p| p.path.clone())
        .or_else(|| world.get::<DmItem>(e).map(|i| i.path.clone()))
}

fn setup_machines(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmPower), With<ss13_world::Mapped>>();
    let machines: Vec<Entity> = query
        .iter(world)
        .filter(|(_, p)| {
            [OVEN_ROOT, GRIDDLE_ROOT, FRYER_ROOT]
                .iter()
                .any(|r| is_under(&p.path, r))
        })
        .map(|(e, _)| e)
        .collect();
    for e in machines {
        setup_machine(world, e, true);
    }
}

/// The state of an oven, griddle or fryer, mapped or built from a frame; a mapped oven starts
/// with a tray (`oven/Initialize(mapload)`).
pub(crate) fn setup_machine(world: &mut World, e: Entity, mapload: bool) {
    let Some(path) = path_of(world, e) else {
        return;
    };
    if is_under(&path, OVEN_ROOT) {
        let tray = world
            .get::<TilePos>(e)
            .copied()
            .filter(|_| mapload)
            .and_then(|pos| crate::construction::spawn_obj(world, OVEN_TRAY, pos));
        if let Some(tray) = tray {
            put_inside(world, tray, e);
        }
        world.entity_mut(e).insert(Oven { open: false, tray });
    } else if is_under(&path, GRIDDLE_ROOT) {
        let variant = (!is_under(&path, GRIDDLE_STAND))
            .then(|| world.resource_mut::<CombatRng>().range(1, 3) as u8);
        world.entity_mut(e).insert(Griddle {
            variant,
            ..default()
        });
    } else if is_under(&path, FRYER_ROOT) {
        // `create_reagents(50, OPENCONTAINER)` with 25 units of cooking oil.
        let mut holder = Holder::new(
            50.0,
            flags::OPENCONTAINER,
            Owner {
                path: path.clone(),
                living: false,
            },
        );
        if let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone())
            && let Some(oil) = chem.id(COOKING_OIL)
        {
            holder.add_reagent_list(&chem, &[(oil, 25.0)], None, true);
        }
        world
            .entity_mut(e)
            .insert((DeepFryer::default(), Reagents(holder)));
    } else {
        return;
    }
    refresh_look(world, e);
}

fn contents(world: &World, e: Entity) -> Vec<Entity> {
    world
        .get::<Stored>(e)
        .map(|s| s.items().to_vec())
        .unwrap_or_default()
}

fn is_storage(world: &World, item: Entity) -> bool {
    world.get::<crate::storage::Storage>(item).is_some()
}

fn item_path(world: &World, item: Entity) -> String {
    world
        .get::<DmItem>(item)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

/// tg `item_interaction` of the cooking machines. True when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    pixel: Option<[i8; 2]>,
) -> bool {
    if world.get::<Oven>(target).is_some() {
        return oven_item(world, client, mob, held, target);
    }
    if world.get::<Griddle>(target).is_some() {
        return griddle_item(world, client, mob, held, target, pixel);
    }
    if world.get::<DeepFryer>(target).is_some() {
        return fryer_item(world, client, mob, held, target);
    }
    false
}

/// An empty-hand click (`attack_hand`), a right click on an open oven takes its tray out. True
/// when the click was theirs.
pub fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    right: bool,
) -> bool {
    if let Some(oven) = world.get::<Oven>(target).cloned() {
        if right && oven.open {
            if let Some(tray) = oven.tray {
                take_tray(world, mob, target, tray);
            }
            return true;
        }
        // A closed range's right click is its burner's (`crate::stove`).
        if right && world.get::<crate::stove::Stove>(target).is_some() {
            return false;
        }
        toggle_oven(world, client, target);
        return true;
    }
    if let Some(griddle) = world.get::<Griddle>(target).cloned() {
        if let Some(mut g) = world.get_mut::<Griddle>(target) {
            g.on = !griddle.on;
        }
        refresh_look(world, target);
        return true;
    }
    if let Some(fryer) = world.get::<DeepFryer>(target).cloned() {
        let Some(item) = fryer.frying else {
            return false;
        };
        let (what, name) = (name_of(world, item), name_of(world, target));
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы вытаскиваете {what} из {name}."),
        );
        stop_frying(world, target);
        give_to(world, mob, target, item);
        return true;
    }
    false
}

/// `user.put_in_hands(...)`, else onto the machine's tile.
pub(crate) fn give_to(world: &mut World, mob: Entity, machine: Entity, item: Entity) {
    if let Some(&pos) = world.get::<TilePos>(machine) {
        take_out(world, item, pos);
    }
    if let Some(inventory) = world.get::<Inventory>(mob).cloned()
        && let Some(hand) = [inventory.active, 1 - inventory.active]
            .into_iter()
            .find(|&h| inventory.hands[h].is_none())
    {
        crate::inventory::pick_up(world, mob, item, hand);
    }
}

pub(crate) fn from_hand(world: &mut World, item: Entity, into: Entity) {
    world.entity_mut(item).remove::<CarriedBy>();
    put_inside(world, item, into);
}

// --- Oven --------------------------------------------------------------------------------

fn oven_item(world: &mut World, client: Entity, mob: Entity, held: Entity, e: Entity) -> bool {
    let oven = world.get::<Oven>(e).cloned().expect("checked");
    if oven.open
        && let Some(tray) = oven.tray
    {
        if is_storage(world, held) {
            crate::kitchen::handle_dumping(world, client, mob, held, e);
            return true;
        }
        // The open tray takes the item (`plate/item_interaction` -> `AddToPlate`).
        if world.get::<DmFood>(held).is_none() {
            return false;
        }
        if contents(world, tray).len() >= TRAY_MAX_ITEMS {
            tell(world, client, Span::Warning, "заполнено!");
            return true;
        }
        from_hand(world, held, tray);
        refresh_look(world, e);
        return true;
    }
    if !oven.open || oven.tray.is_some() || !is_under(&item_path(world, held), OVEN_TRAY) {
        return false;
    }
    let (tray, name) = (name_of(world, held), name_of(world, e));
    from_hand(world, held, e);
    if let Some(mut o) = world.get_mut::<Oven>(e) {
        o.tray = Some(held);
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы помещаете {tray} в {name}."),
    );
    refresh_look(world, e);
    true
}

/// The tray of an open oven into the user's hands (tg: the tray is clicked in the oven's
/// `vis_contents`).
fn take_tray(world: &mut World, mob: Entity, oven: Entity, tray: Entity) {
    if let Some(mut o) = world.get_mut::<Oven>(oven) {
        o.tray = None;
    }
    give_to(world, mob, oven, tray);
    refresh_look(world, oven);
}

fn toggle_oven(world: &mut World, client: Entity, e: Entity) {
    let open = !world.get::<Oven>(e).is_some_and(|o| o.open);
    if let Some(mut o) = world.get_mut::<Oven>(e) {
        o.open = open;
    }
    let name = name_of(world, e);
    let text = if open {
        format!("Вы открываете {name}.")
    } else {
        format!("Вы закрываете {name}.")
    };
    tell(world, client, Span::Notice, text);
    refresh_look(world, e);
}

/// tg `appears_active()`: closed, food on the tray, powered.
fn oven_active(world: &World, e: Entity) -> bool {
    let Some(oven) = world.get::<Oven>(e) else {
        return false;
    };
    !oven.open
        && oven.tray.is_some_and(|t| !contents(world, t).is_empty())
        && world.get::<Unpowered>(e).is_none()
}

/// The rest of `handle_dumping` onto an oven's tray or a griddle: every edible item of
/// `storage` until it is full. True when `target` is one of them.
pub fn load_from(world: &mut World, client: Entity, storage: Entity, target: Entity) -> bool {
    let (into, max) = if let Some(oven) = world.get::<Oven>(target) {
        match oven.tray.filter(|_| oven.open) {
            Some(tray) => (Some(tray), TRAY_MAX_ITEMS),
            None => return true,
        }
    } else if world.get::<Griddle>(target).is_some() {
        (None, GRIDDLE_MAX_ITEMS)
    } else {
        return false;
    };
    let mut loaded = 0;
    for item in contents(world, storage) {
        if world.get::<DmFood>(item).is_none() {
            continue;
        }
        let count = match into {
            Some(tray) => contents(world, tray).len(),
            None => world.get::<Griddle>(target).map_or(0, |g| g.items.len()),
        };
        if count >= max {
            break;
        }
        match into {
            Some(tray) => put_inside(world, item, tray),
            None => {
                let mut rng = world.resource_mut::<CombatRng>();
                let offset = [0, 0].map(|_| rng.range(0, 10) as i16 - 5);
                add_to_griddle(world, target, item, offset);
            }
        }
        loaded += 1;
    }
    if loaded > 0 {
        let name = name_of(world, target);
        tell(
            world,
            client,
            Span::Notice,
            format!(
                "Вы помещаете {loaded} {} в {name}.",
                crate::kitchen::items_word(loaded)
            ),
        );
    }
    refresh_look(world, target);
    true
}

/// One `oven/process`: every item on the tray bakes `dt`.
fn process_oven(world: &mut World, e: Entity, dt: f32) {
    if !oven_active(world, e) {
        return;
    }
    let Some(tray) = world.get::<Oven>(e).and_then(|o| o.tray) else {
        return;
    };
    for item in contents(world, tray) {
        let Some(food) = world
            .get::<DmFood>(item)
            .filter(|f| !f.bake_result.is_empty())
            .cloned()
        else {
            // Nothing bakes it: `fire_act(1000)`, and now and then the smell.
            scorch(world, item, e, COOKING_FIRE_TEMP);
            if world
                .resource_mut::<CombatRng>()
                .chance(spt_percent(10.0, dt))
            {
                let name = capitalize(&name_of(world, e));
                visible(
                    world,
                    e,
                    "",
                    &format!("{name} источает запах гари!"),
                    Span::Danger,
                );
            }
            continue;
        };
        let mut times = cook_times(world, item);
        let required = *times
            .bake_required
            .get_or_insert_with(|| roll(world, food.bake_time));
        times.baked += dt * 10.0;
        world.entity_mut(item).insert(times);
        if times.baked >= required {
            finish_cooking(
                world,
                item,
                &food.bake_result,
                food.bake_good,
                e,
                Some(tray),
            );
        }
    }
    let joules = active_power(world, e);
    crate::kitchen::machine_use_energy(world, e, joules);
    refresh_look(world, e);
}

pub(crate) fn cook_times(world: &World, item: Entity) -> CookTimes {
    world.get::<CookTimes>(item).copied().unwrap_or_default()
}

/// `rand(low, high)` deciseconds.
pub(crate) fn roll(world: &mut World, [low, high]: [f32; 2]) -> f32 {
    let (low, high) = (low.max(0.0) as u32, high.max(low).max(0.0) as u32);
    world.resource_mut::<CombatRng>().range(low, high) as f32
}

fn active_power(world: &World, e: Entity) -> f64 {
    world
        .get::<DmPower>(e)
        .map_or(0.0, |p| p.active_power_usage)
}

/// `finish_baking` / `finish_grilling`: the result where the item was (on the tray, or on the
/// griddle's tile at its offset) with the item's reagents; the item is gone.
fn finish_cooking(
    world: &mut World,
    item: Entity,
    result: &str,
    good: bool,
    machine: Entity,
    tray: Option<Entity>,
) -> Option<Entity> {
    let pos = world
        .get::<TilePos>(item)
        .or_else(|| world.get::<TilePos>(machine))
        .copied()?;
    let offset = world.get::<Appearance>(item).map(|a| a.pixel_offset);
    let made = crate::construction::spawn_obj(world, result, pos)?;
    // `ADD_TRAIT(result, TRAIT_HANDMADE, who_baked_us)`.
    world.entity_mut(made).insert(crate::food::Handmade);
    let source = world.get::<Reagents>(item).map(|r| r.0.clone());
    let chem = world.get_resource::<Chem>().map(|c| c.0.clone());
    if let (Some(mut source), Some(chem)) = (source, chem)
        && let Some(mut target) = world.get_mut::<Reagents>(made)
    {
        target.0.clear();
        let total = source.total_volume;
        source.trans_to(&chem, &mut target.0, total, 1.0, None, false, false);
    }
    crate::food::warm_bonus(world, item, made);
    if let (Some(offset), Some(mut look)) = (offset, world.get_mut::<Appearance>(made)) {
        look.pixel_offset = offset;
    }
    let (what, made_name, name) = (
        name_of(world, item),
        name_of(world, made),
        name_of(world, machine),
    );
    world.despawn(item);
    if let Some(tray) = tray {
        put_inside(world, made, tray);
        let text = if good {
            format!("Вы чувствуете, как {name} источает приятный запах.")
        } else {
            format!("Вы чувствуете, как {name} испускает горелое зловоние.")
        };
        let span = if good { Span::Notice } else { Span::Warning };
        visible(world, machine, "", &text, span);
    } else {
        let span = if good { Span::Notice } else { Span::Warning };
        visible(
            world,
            machine,
            "",
            &format!("{what} превращается в {made_name}!"),
            span,
        );
    }
    Some(made)
}

/// `/datum/component/grillable/on_grill` (`COMSIG_ITEM_GRILL_PROCESS`) from a grill that is not
/// a griddle (a bonfire's): the food grills `dt` seconds and past its time becomes its
/// `grill_result`. False when nothing grills it.
pub fn grill_process(world: &mut World, item: Entity, grill: Entity, dt: f32) -> bool {
    let Some(food) = world
        .get::<DmFood>(item)
        .filter(|f| !f.grill_result.is_empty())
        .cloned()
    else {
        return false;
    };
    let mut times = cook_times(world, item);
    let required = *times
        .grill_required
        .get_or_insert_with(|| roll(world, food.grill_time));
    times.grilled += dt * 10.0;
    world.entity_mut(item).insert(times);
    if times.grilled >= required {
        finish_cooking(
            world,
            item,
            &food.grill_result,
            food.grill_good,
            grill,
            None,
        );
    }
    true
}

// --- Griddle -----------------------------------------------------------------------------

fn griddle_item(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    e: Entity,
    pixel: Option<[i8; 2]>,
) -> bool {
    if world.get::<CombatMode>(mob).is_some_and(|c| c.0) {
        return false;
    }
    prune_griddle(world, e);
    let count = world.get::<Griddle>(e).map_or(0, |g| g.items.len());
    if is_storage(world, held) {
        if count >= GRIDDLE_MAX_ITEMS {
            tell(world, client, Span::Warning, "заполнено");
            return true;
        }
        crate::kitchen::handle_dumping(world, client, mob, held, e);
        return true;
    }
    let name = name_of(world, e);
    if count >= GRIDDLE_MAX_ITEMS {
        tell(
            world,
            client,
            Span::Notice,
            format!("{name} не может уместить больше предметов!"),
        );
        return true;
    }
    // Centered where the user clicked, never more than 16 pixels off the tile.
    let offset = pixel.map_or([0, 0], |p| p.map(|v| i16::from(v).clamp(-16, 16)));
    let what = name_of(world, held);
    world.entity_mut(held).remove::<CarriedBy>();
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob)
        && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(held))
    {
        *slot = None;
    }
    add_to_griddle(world, e, held, offset);
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы помещаете {what} на {name}."),
    );
    true
}

/// `AddToGrill`: on the griddle's tile at `offset`.
pub(crate) fn add_to_griddle(world: &mut World, griddle: Entity, item: Entity, offset: [i16; 2]) {
    let Some(&pos) = world.get::<TilePos>(griddle) else {
        return;
    };
    world.entity_mut(item).remove::<crate::storage::StoredIn>();
    crate::inventory::put_down_at(world, item, pos, offset);
    crate::construction::tile_changed(world, pos);
    if let Some(mut g) = world.get_mut::<Griddle>(griddle)
        && !g.items.contains(&item)
    {
        g.items.push(item);
    }
    refresh_look(world, griddle);
}

/// `ItemMoved` / `ItemRemovedFromGrill`: what left the griddle's tile is off it.
fn prune_griddle(world: &mut World, e: Entity) {
    let pos = world.get::<TilePos>(e).copied();
    let Some(items) = world.get::<Griddle>(e).map(|g| g.items.clone()) else {
        return;
    };
    let kept: Vec<Entity> = items
        .into_iter()
        .filter(|&i| world.get_entity(i).is_ok() && world.get::<TilePos>(i).copied() == pos)
        .collect();
    if let Some(mut g) = world.get_mut::<Griddle>(e)
        && g.items != kept
    {
        g.items = kept;
    }
}

/// One `griddle/process`: every food on it grills `dt`.
fn process_griddle(world: &mut World, e: Entity, dt: f32) {
    prune_griddle(world, e);
    let Some(griddle) = world.get::<Griddle>(e).cloned() else {
        return;
    };
    if !griddle.on || world.get::<Unpowered>(e).is_some() {
        return;
    }
    for item in griddle.items {
        let Some(food) = world
            .get::<DmFood>(item)
            .filter(|f| !f.grill_result.is_empty())
            .cloned()
        else {
            // Nothing grills it: it burns, and only burning draws the active power (tg
            // `use_energy` sits inside this branch of the loop).
            let what = capitalize(&name_of(world, item));
            scorch(world, item, e, COOKING_FIRE_TEMP);
            if world.resource_mut::<CombatRng>().chance(10.0) {
                let name = name_of(world, e);
                visible(
                    world,
                    e,
                    "",
                    &format!("Кажется, {what} не очень удачно готовится на {name}!"),
                    Span::Danger,
                );
            }
            let joules = active_power(world, e);
            crate::kitchen::machine_use_energy(world, e, joules);
            continue;
        };
        let mut times = cook_times(world, item);
        let required = *times
            .grill_required
            .get_or_insert_with(|| roll(world, food.grill_time));
        times.grilled += dt * 10.0;
        world.entity_mut(item).insert(times);
        if times.grilled >= required
            && let Some(made) =
                finish_cooking(world, item, &food.grill_result, food.grill_good, e, None)
            && let Some(mut g) = world.get_mut::<Griddle>(e)
            && let Some(slot) = g.items.iter_mut().find(|i| **i == item)
        {
            // `GrillCompleted` -> `AddToGrill(grilled_result)`.
            *slot = made;
        }
    }
    // `griddle_loc.hotspot_expose(800, 100)`: a hot griddle lights plasma in the air.
    if let Some(&pos) = world.get::<TilePos>(e)
        && let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>()
    {
        atmos.hotspot_expose(pos, 800.0, 100.0, false);
    }
}

/// `fire_act(1000)` of the oven and the griddle.
const COOKING_FIRE_TEMP: f32 = 1000.0;
/// `/obj/item` `max_integrity` when the prototype has none.
const ITEM_MAX_INTEGRITY: f32 = 200.0;
/// `w_class` from which `/obj/item/burn()` leaves large ash (`WEIGHT_CLASS_HUGE`).
const WEIGHT_CLASS_HUGE: u32 = 5;
const ASH: &str = "/obj/effect/decal/cleanable/ash";
const ASH_LARGE: &str = "/obj/effect/decal/cleanable/ash/large";

/// `SPT_PROB(percent, seconds)`: the chance over `seconds` of a per-second `percent`.
pub(crate) fn spt_percent(percent: f32, seconds: f32) -> f32 {
    100.0 * (1.0 - (1.0 - percent / 100.0).powf(seconds))
}

pub(crate) use ss13_chat::capitalize;

/// `/obj/fire_act(temperature)` of an item in a cooking machine: `0.02 × temperature` damage
/// (at most 20) unless `FIRE_PROOF`; at 0 integrity it burns to ash (`/obj/item/burn()`).
// shortcut: no `/datum/component/burning` (flames on the item, damage while it burns outside the
// machine); items that are not food burn only in ovens and on griddles, not in other fires.
pub fn scorch(world: &mut World, item: Entity, machine: Entity, temperature: f32) {
    let Some(dm) = world.get::<DmItem>(item).cloned() else {
        return;
    };
    let damage = (0.02 * temperature).clamp(0.0, 20.0);
    if dm.resistance_flags & (crate::fire::FIRE_PROOF | crate::obj_damage::INDESTRUCTIBLE) != 0
        || damage <= 0.0
    {
        return;
    }
    let max = if dm.max_integrity > 0.0 {
        dm.max_integrity
    } else {
        ITEM_MAX_INTEGRITY
    };
    let mut integrity = world
        .get::<crate::obj_damage::Integrity>(item)
        .copied()
        .unwrap_or(crate::obj_damage::Integrity {
            current: max,
            broken: false,
        });
    integrity.current = (integrity.current - damage).max(0.0);
    world.entity_mut(item).insert(integrity);
    if integrity.current > 0.0 {
        return;
    }
    let pos = world
        .get::<TilePos>(item)
        .or_else(|| world.get::<TilePos>(machine))
        .copied();
    let large = world
        .get::<ss13_mapload::DmWeight>(item)
        .is_some_and(|w| u32::from(w.w_class) >= WEIGHT_CLASS_HUGE);
    let name = name_of(world, item);
    if let Some(pos) = pos
        && let Some(ash) =
            crate::construction::spawn_obj(world, if large { ASH_LARGE } else { ASH }, pos)
        && let Some(mut desc) = world.get_mut::<ss13_mapload::DmName>(ash)
    {
        desc.desc
            .push_str(&format!("\nПохоже, когда-то это было {name}."));
    }
    world.despawn(item);
}

// --- Deep fryer --------------------------------------------------------------------------

fn has_fat(world: &World, e: Entity) -> bool {
    let (Some(r), Some(chem)) = (world.get::<Reagents>(e), world.get_resource::<Chem>()) else {
        return false;
    };
    r.0.reagents
        .iter()
        .any(|x| is_under(&chem.0.def(x.id).path, FAT))
}

fn fryer_item(world: &mut World, client: Entity, mob: Entity, held: Entity, e: Entity) -> bool {
    let path = item_path(world, held);
    // A pill dissolves and a drainable container pours (`crate::chem`).
    if is_under(&path, "/obj/item/reagent_containers/applicator/pill")
        || world.get::<CombatMode>(mob).is_some_and(|c| c.0)
        || world
            .get::<Reagents>(held)
            .is_some_and(|r| r.0.is_drainable())
    {
        return false;
    }
    let name = name_of(world, e);
    if !has_fat(world, e) {
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} не имеет жира или масла для жарки!"),
        );
        return true;
    }
    if FRY_BLACKLIST.iter().any(|b| is_under(&path, b)) || is_storage(world, held) {
        return false;
    }
    if world
        .get::<DeepFryer>(e)
        .is_some_and(|f| f.frying.is_some())
    {
        return false;
    }
    let what = name_of(world, held);
    from_hand(world, held, e);
    // `start_fry`: something to soak the oil into.
    if world.get::<Reagents>(held).is_none() {
        let holder = Holder::new(
            50.0,
            flags::INJECTABLE,
            Owner {
                path,
                living: false,
            },
        );
        world.entity_mut(held).insert(Reagents(holder));
    }
    if let Some(mut f) = world.get_mut::<DeepFryer>(e) {
        f.frying = Some(held);
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы помещаете {what} в {name}."),
    );
    refresh_look(world, e);
    true
}

/// `/obj/machinery/deepfryer/RefreshParts`: (`fry_speed`, `oil_use`).
pub fn fryer_ratings(world: &World, e: Entity) -> (f32, f32) {
    let r = crate::stock_parts::ratings(world, e);
    let efficiency = r.sum(crate::stock_parts::PartKind::MicroLaser) as f32;
    (efficiency, OIL_USE - efficiency * 0.00475)
}

/// One `deepfryer/process`.
fn process_fryer(world: &mut World, e: Entity, dt: f32) {
    let Some(fryer) = world.get::<DeepFryer>(e).cloned() else {
        return;
    };
    if !has_fat(world, e) {
        return;
    }
    let Some(item) = fryer.frying else {
        return;
    };
    let (speed, oil_use) = fryer_ratings(world, e);
    let chem = world.get_resource::<Chem>().map(|c| c.0.clone());
    if let Some(chem) = chem
        && let Some(mut oil) = world.get::<Reagents>(e).map(|r| r.0.clone())
        && let Some(mut soaked) = world.get::<Reagents>(item).map(|r| r.0.clone())
    {
        // "Fried foods gain more of the reagent thanks to space magic."
        oil.trans_to(
            &chem,
            &mut soaked,
            oil_use * dt,
            speed * 3.0,
            None,
            false,
            false,
        );
        world.entity_mut(e).insert(Reagents(oil));
        world.entity_mut(item).insert(Reagents(soaked));
    }
    let grease = world.resource_mut::<CombatRng>().chance(GREASE_CHANCE);
    let cook_time = fryer.cook_time + speed * dt * 10.0;
    let mut f = fryer.clone();
    if grease {
        f.grease += GREASE_AMOUNT;
    }
    f.cook_time = cook_time;
    let name = name_of(world, e);
    if cook_time >= FRYING_TIME_PERFECT && !f.fried {
        f.fried = true;
        visible(world, e, "", &format!("{name} звенит!"), Span::Notice);
    } else if cook_time >= FRYING_TIME_WARNING && !f.burnt {
        f.burnt = true;
        visible(
            world,
            e,
            "",
            &format!("{name} источает едкий запах!"),
            Span::Warning,
        );
    }
    world.entity_mut(e).insert(f);
    let joules = active_power(world, e);
    crate::kitchen::machine_use_energy(world, e, joules);
    refresh_look(world, e);
}

/// `reset_frying` when the item leaves: it is fried for the time it cooked.
fn stop_frying(world: &mut World, e: Entity) {
    let Some(fryer) = world.get::<DeepFryer>(e).cloned() else {
        return;
    };
    if let Some(item) = fryer.frying
        && world.get_entity(item).is_ok()
    {
        fry(world, item, fryer.cook_time);
    }
    world.entity_mut(e).insert(DeepFryer {
        grease: fryer.grease,
        ..default()
    });
    refresh_look(world, e);
}

/// The fryer's frying item left it some other way (deconstruction) or was deleted.
pub fn on_removed(world: &mut World, e: Entity) {
    if world
        .get::<DeepFryer>(e)
        .and_then(|f| f.frying)
        .is_some_and(|i| {
            world
                .get::<crate::storage::StoredIn>(i)
                .is_none_or(|s| s.0 != e)
        })
    {
        stop_frying(world, e);
    }
}

/// `/datum/element/fried_item/Attach(target, fry_time)`.
pub fn fry(world: &mut World, item: Entity, fry_time: f32) {
    let pct = world.resource_mut::<CombatRng>().range(1, 75);
    let (level, prefix) = if fry_time < FRYING_TIME_FRIED {
        (0, "lightly-fried")
    } else if fry_time < FRYING_TIME_PERFECT {
        (1, "fried")
    } else if fry_time < FRYING_TIME_BURNT {
        (2, "deep-fried")
    } else {
        (3, "")
    };
    if let Some(mut name) = world.get_mut::<DmName>(item) {
        match level {
            0 => {
                name.name = format!("{prefix} {}", name.name);
                name.desc
                    .push_str(" It's been lightly fried in a deep fryer.");
            }
            1 => {
                name.name = format!("{prefix} {}", name.name);
                name.desc.push_str(&format!(
                    " It's been fried, increasing its tastiness value by {pct}%."
                ));
            }
            2 => {
                name.name = format!("{prefix} {}", name.name);
                name.desc.push_str(" Deep-fried to perfection.");
            }
            _ => {
                name.name = "the physical manifestation of the very concept of fried foods".into();
                name.desc = "A heavily-fried... something. Who can tell anymore?".into();
            }
        }
    }
    if let Some(mut look) = world.get_mut::<Appearance>(item) {
        look.color = Some(FRIED_COLORS[level]);
    }
    // `AddComponentFrom(SOURCE_EDIBLE_FRIED, /datum/component/edible, bite_consumption = 2,
    // junkiness = 10, ...)`: anything fried can be eaten; edible food keeps its numbers.
    if world.get::<DmFood>(item).is_none() {
        let reagents = world.get::<Reagents>(item).map(|r| r.0.clone());
        world.entity_mut(item).insert(DmFood {
            volume: 50.0,
            purity: 0.5,
            bite_consumption: 2.0,
            eat_time: 10.0,
            junkiness: 10.0,
            eatverbs: ["bite", "chew", "nibble", "gnaw", "gobble", "chomp"]
                .map(str::to_owned)
                .to_vec(),
            ..default()
        });
        // The edible observer gave it an empty holder: the oil it soaked stays.
        world.flush();
        if let Some(holder) = reagents {
            world.entity_mut(item).insert(Reagents(holder));
        }
    }
    world.entity_mut(item).insert(Fried);
}

/// tg `TRAIT_FOOD_FRIED`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fried;

/// `bakeable/on_examine`, `grillable/on_examine` and the fryer's `examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    if let Some(fryer) = world.get::<DeepFryer>(target) {
        if let Some(item) = fryer.frying {
            lines.push((
                Span::Plain,
                format!("Вы разглядываете {} в масле.", name_of(world, item)),
            ));
        }
        let (speed, oil_use) = fryer_ratings(world, target);
        lines.push((
            Span::Notice,
            format!(
                "На дисплее состояния отображается: Скорость жарки - {}%. Используется {} \
                 масла в секунду.",
                crate::chem::dm_num(speed * 100.0),
                crate::chem::dm_num(oil_use)
            ),
        ));
        return;
    }
    let Some(food) = world.get::<DmFood>(target) else {
        return;
    };
    let times = world.get::<CookTimes>(target).copied().unwrap_or_default();
    let name = name_of(world, target);
    if !food.bake_result.is_empty() {
        match times.bake_required {
            _ if times.baked == 0.0 => {
                if food.bake_good {
                    lines.push((Span::Notice, format!("{name} запекается в духовке.")));
                }
            }
            Some(required) if food.bake_good && times.baked <= required * 0.75 => lines.push((
                Span::Notice,
                format!("Наверное, {name} нужно выпекать немного дольше!"),
            )),
            Some(_) if food.bake_good => lines.push((
                Span::Notice,
                format!("Кажется, что {name} вот-вот запечется!"),
            )),
            _ => lines.push((
                Span::Danger,
                format!("{name} не стоит засовывать в духовку."),
            )),
        }
    }
    if !food.grill_result.is_empty() && times.grilled > 0.0 {
        let required = times.grill_required.unwrap_or(f32::INFINITY);
        let text = if !food.grill_good {
            (
                Span::Danger,
                format!("{name} should probably not be put on the grill."),
            )
        } else if times.grilled <= required * 0.75 {
            (
                Span::Notice,
                format!("{name} probably needs to be cooked a bit longer!"),
            )
        } else {
            (
                Span::Notice,
                format!("{name} seems to be almost finished cooking!"),
            )
        };
        lines.push(text);
    } else if food.grill_good {
        lines.push((Span::Notice, format!("{name} can be grilled.")));
    }
}

// --- Looks -------------------------------------------------------------------------------

fn icon_state(world: &World, e: Entity, state: &str) -> Option<ss13_shared::IconStateId> {
    let atom = world.get::<DmAtom>(e)?;
    world.get_resource::<IconIndex>()?.get(&atom.icon, state)
}

/// Base sprites of `items` as overlays at their offsets plus `shift`.
fn item_overlays(world: &World, items: &[Entity], shift: [i16; 2]) -> Vec<Overlay> {
    items
        .iter()
        .filter_map(|&i| {
            let look = world.get::<Appearance>(i)?;
            let mut o = Overlay::new(look.icon_state);
            o.color = look.color;
            o.pixel_offset = [
                look.pixel_offset[0] + shift[0],
                look.pixel_offset[1] + shift[1],
            ];
            Some(o)
        })
        .collect()
}

/// `update_icon_state` and `update_overlays` of an oven, griddle or fryer.
pub fn refresh_look(world: &mut World, e: Entity) {
    let base = world
        .get::<DmAtom>(e)
        .map(|a| a.icon_state.clone())
        .unwrap_or_default();
    let base_state = base
        .rsplit_once('_')
        .map_or(base.as_str(), |(b, _)| b)
        .to_owned();
    let (state, overlays): (String, Vec<Overlay>) = if let Some(oven) = world.get::<Oven>(e) {
        let oven = oven.clone();
        let active = oven_active(world, e);
        let state = format!("{base_state}_{}", if active { "on" } else { "off" });
        let mut overlays = Vec::new();
        if oven.open {
            if let Some(id) = icon_state(world, e, &format!("{base_state}_lid_open")) {
                let mut o = Overlay::new(id);
                o.pixel_offset = [0, LID_OFFSET];
                overlays.push(o);
            }
            // The tray in `vis_contents`, shown while open.
            if let Some(tray) = oven.tray
                && let Some(look) = world.get::<Appearance>(tray)
            {
                let mut o = Overlay::new(look.icon_state);
                o.pixel_offset = TRAY_OFFSET;
                overlays.push(o);
                overlays.extend(item_overlays(world, &contents(world, tray), TRAY_OFFSET));
            }
        } else if let Some(id) = icon_state(world, e, &format!("{base_state}_lid_closed")) {
            overlays.push(Overlay::new(id));
        }
        (state, overlays)
    } else if let Some(griddle) = world.get::<Griddle>(e) {
        let on = if griddle.on { "on" } else { "off" };
        let state = match griddle.variant {
            Some(v) => format!("griddle{v}_{on}"),
            None => format!("griddlestand_{on}"),
        };
        let overlays = match griddle.variant {
            None => icon_state(world, e, "front_bar")
                .map(Overlay::new)
                .into_iter()
                .collect(),
            Some(_) => Vec::new(),
        };
        (state, overlays)
    } else if let Some(fryer) = world.get::<DeepFryer>(e) {
        let panel = world
            .get::<crate::construction::PanelOpen>(e)
            .is_some_and(|p| p.0);
        let state = if fryer.frying.is_some() {
            "fryer_on"
        } else if panel {
            "fryer_open"
        } else {
            "fryer_off"
        };
        let overlays = if fryer.grease >= 1.0 {
            icon_state(world, e, "fryer_greasy")
                .map(Overlay::new)
                .into_iter()
                .collect()
        } else {
            Vec::new()
        };
        (state.to_owned(), overlays)
    } else {
        return;
    };
    // The range's burner (`/datum/component/stove` adds its overlays).
    let mut overlays = overlays;
    overlays.extend(crate::stove::burner_overlays(world, e));
    let id = icon_state(world, e, &state);
    let Some(mut look) = world.get_mut::<Appearance>(e) else {
        return;
    };
    if let Some(id) = id
        && look.icon_state != id
    {
        look.icon_state = id;
    }
    if look.overlays != overlays {
        look.overlays = overlays;
    }
}

// --- Timers ------------------------------------------------------------------------------

/// Every `SSmachines` tick: ovens, griddles and fryers process.
// Cost: a scan of the cooking machines (two dozen on MetaStation) every 2 s.
fn cooking_tick(world: &mut World, mut last: Local<Option<u64>>) {
    let ticks = world.resource::<MachineTicks>().0;
    let from = last.unwrap_or(ticks);
    *last = Some(ticks);
    for _ in from..ticks {
        let dt = MACHINE_TICK_SECS as f32;
        let mut q =
            world.query_filtered::<Entity, Or<(With<Oven>, With<Griddle>, With<DeepFryer>)>>();
        let machines: Vec<Entity> = q.iter(world).collect();
        for e in machines {
            if world.get::<Oven>(e).is_some() {
                process_oven(world, e, dt);
            } else if world.get::<Griddle>(e).is_some() {
                process_griddle(world, e, dt);
            } else {
                on_removed(world, e);
                process_fryer(world, e, dt);
            }
        }
    }
}
