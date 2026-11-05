//! Kitchen machines (server): tg `/obj/machinery/microwave` (`food_and_drinks/machinery/
//! microwave.dm`), `/obj/machinery/processor` (`processor.dm`, `/datum/food_processor_process`)
//! and `/obj/machinery/reagentgrinder` (`reagentgrinder.dm`, `/obj/item/proc/grind`).
//!
//! - Microwave: an item of normal size or smaller goes in on a click out of combat mode (10 at
//!   most). A click with an empty hand cooks (tg's radial "cook"), a right click ejects ("eject").
//!   Cooking runs 10 cycles of 1 s, each using `active_power_usage`; then every food turns into
//!   its `microwavable` result (a real recipe such as an egg into a boiled egg, anything else
//!   into a burned mess) with the food's reagents, and everything is ejected. Each cooked item
//!   dirties it (`dirty`): a cook mucks up with `dirty * 5`% chance (4 cycles, then filthy at
//!   100 and unusable). Losing power while it runs breaks it and ejects the contents. A
//!   storage used on it loads its food (`handle_dumping`: a tray at once, a bag after 2 s).
//! - Food processor: food with a recipe goes in; an empty-hand click runs it for the recipes'
//!   `time` and puts out `food_multiplier` of the output per input, splitting its reagents.
//! - Grinder: starts with a large beaker (map load). An open container in hand replaces the
//!   beaker, anything with reagents goes in (up to `6 * matter bin tier` total weight); an
//!   empty-hand click grinds for `6 s / servo tier` (every ingredient's reagents into the beaker, then it is gone), a right click
//!   takes the beaker out, or ejects the contents without a beaker.
// shortcut: the radial menus are fixed clicks (microwave: cook / right click eject; grinder:
// grind / right click beaker or eject); no juicing or mixing, no `grind_results()` procs, no
// wires, anchoring, cleaning
// a filthy microwave, repairing a broken one, PDA charging, cells, shocks of mobs inside, the
// processor's mob gibbing, sounds or the ingredients drawn inside the microwave.

use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmFood, DmPower, DmWeight, IconIndex};
use ss13_reagents::{Holder, is_under};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};

use crate::chem::{Chem, Reagents, name_of, visible};
use crate::combat::{CombatMode, CombatRng};
use crate::inventory::{CarriedBy, Inventory};
use crate::power::Unpowered;
use crate::storage::{Stored, put_inside, take_out};
use crate::tools::tell;

pub const MICROWAVE_ROOT: &str = "/obj/machinery/microwave";
pub const PROCESSOR_ROOT: &str = ss13_mapload::food::PROCESSOR;
pub const GRINDER_ROOT: &str = ss13_mapload::food::GRINDER;
/// `MAX_MICROWAVE_DIRTINESS`.
pub const MAX_MICROWAVE_DIRTINESS: u32 = 100;
/// `WEIGHT_CLASS_GIGANTIC`: the grinder's `maximum_weight` per matter bin tier.
const WEIGHT_CLASS_GIGANTIC: u32 = 6;
/// `start()`: 10 cycles; `muck()` and `start_can_fail()`: 4.
const COOK_CYCLES: u32 = 10;
const SHORT_CYCLES: u32 = 4;
/// `open(autoclose = 2 SECONDS)` after a cook, `0.6 SECONDS` after an insert.
const OPEN_AFTER_COOK: f64 = 2.0;
const OPEN_AFTER_INSERT: f64 = 0.6;
/// `WEIGHT_CLASS_NORMAL`, `WEIGHT_CLASS_BULKY` (the grinder's `maximum_weight` without parts).
const WEIGHT_CLASS_NORMAL: u8 = 3;
const WEIGHT_CLASS_BULKY: u8 = 5;

use crate::stock_parts::PartKind;

/// tg `microwave/RefreshParts`: `max_n_of_items = 10 * (first) matter bin tier`.
pub fn microwave_max_items(world: &World, e: Entity) -> usize {
    crate::stock_parts::ratings(world, e)
        .first(PartKind::MatterBin)
        .map_or(10, |t| 10 * t as usize)
}

/// tg `microwave/RefreshParts`: `efficiency` is the sum of the micro-laser tiers (a microwave
/// without a board, which tg cannot have, counts as one tier 1 laser).
pub fn microwave_efficiency(world: &World, e: Entity) -> f64 {
    let r = crate::stock_parts::ratings(world, e);
    if r.first(PartKind::MicroLaser).is_none() {
        return 1.0;
    }
    f64::from(r.sum(PartKind::MicroLaser))
}

/// `cook_loop` `wait = max(12 - 2 * efficiency, 2)` deciseconds.
pub fn microwave_cycle_secs(efficiency: f64) -> f64 {
    (12.0 - 2.0 * efficiency).max(2.0) / 10.0
}

/// tg `processor/RefreshParts`: (`rating_amount`, `rating_speed`) the last matter bin's and
/// servo's tiers, else 1.
pub fn processor_ratings(world: &World, e: Entity) -> (u32, u32) {
    let r = crate::stock_parts::ratings(world, e);
    (
        r.last(PartKind::MatterBin).unwrap_or(1),
        r.last(PartKind::Servo).unwrap_or(1).max(1),
    )
}

/// tg `reagentgrinder/RefreshParts`: (`speed`, `maximum_weight`): the servo's tier and
/// `WEIGHT_CLASS_GIGANTIC * matter bin tier` (the last of each), else 1 and bulky.
pub fn grinder_ratings(world: &World, e: Entity) -> (u32, u32) {
    let r = crate::stock_parts::ratings(world, e);
    (
        r.last(PartKind::Servo).unwrap_or(1).max(1),
        r.last(PartKind::MatterBin)
            .map_or(u32::from(WEIGHT_CLASS_BULKY), |t| WEIGHT_CLASS_GIGANTIC * t),
    )
}
/// `operate_for(6 SECONDS)`.
const GRIND_SECS: f64 = 6.0;

/// What a microwave cycle loop ends in (tg `MICROWAVE_NORMAL`, `MICROWAVE_MUCK`,
/// `MICROWAVE_PRE`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CookKind {
    Normal,
    Muck,
    Pre,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cooking {
    pub kind: CookKind,
    pub cycles_left: u32,
    pub next_cycle: f64,
}

/// A microwave's live state.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Microwave {
    pub operating: Option<Cooking>,
    pub dirty: u32,
    /// `broken`: 0 fine, 2 `REALLY_BROKEN`.
    pub broken: u8,
    /// The door closes at this time (`open`).
    pub open_until: Option<f64>,
}

/// A food processor's live state: when its run ends.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Processor {
    pub ends_at: Option<f64>,
}

/// A grinder's live state.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Grinder {
    pub beaker: Option<Entity>,
    pub ends_at: Option<f64>,
}

pub struct KitchenServerPlugin;

impl Plugin for KitchenServerPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `interact` / `attack_hand_secondary` of kitchen machines.
        app.add_click_handler(ClickPhase::EmptyHand, 500, |w, c| {
            crate::kitchen::interact(w, c.client, c.mob, c.target, c.modifiers.right)
        });
        // tg `item_interaction` of kitchen machines (inserting); food has reagents, so before chem.
        app.add_click_handler(ClickPhase::HeldOnTarget, 1800, |w, c| {
            c.held
                .is_some_and(|h| crate::kitchen::item_interaction(w, c.client, c.mob, h, c.target))
        });
        app.add_systems(Startup, setup_machines)
            .add_systems(Update, kitchen_tick.run_if(any_running));
    }
}

/// A machine with a timer pending (the exclusive tick runs only then).
fn any_running(
    microwaves: Query<&Microwave>,
    processors: Query<&Processor>,
    grinders: Query<&Grinder>,
) -> bool {
    microwaves
        .iter()
        .any(|m| m.operating.is_some() || m.open_until.is_some())
        || processors.iter().any(|p| p.ends_at.is_some())
        || grinders.iter().any(|g| g.ends_at.is_some())
}

/// The machines on the map get their state; grinders their beaker (`Initialize(mapload)`).
fn setup_machines(world: &mut World) {
    let mut query = world.query::<(Entity, &DmPower)>();
    let machines: Vec<Entity> = query
        .iter(world)
        .filter(|(_, p)| {
            [MICROWAVE_ROOT, PROCESSOR_ROOT, GRINDER_ROOT]
                .iter()
                .any(|root| is_under(&p.path, root))
        })
        .map(|(e, _)| e)
        .collect();
    for e in machines {
        setup_machine(world, e, true);
    }
}

/// A microwave's, processor's or grinder's state; only a mapped grinder starts with a beaker
/// (`reagentgrinder/Initialize(mapload)`). Also for machines built from a frame
/// (`crate::machine_frame`).
pub(crate) fn setup_machine(world: &mut World, e: Entity, mapload: bool) {
    let Some(path) = world.get::<DmPower>(e).map(|p| p.path.clone()) else {
        return;
    };
    if [
        crate::cooking::OVEN_ROOT,
        crate::cooking::GRIDDLE_ROOT,
        crate::cooking::FRYER_ROOT,
    ]
    .iter()
    .any(|r| is_under(&path, r))
    {
        crate::cooking::setup_machine(world, e, mapload);
        if is_under(&path, "/obj/machinery/oven/range") {
            crate::stove::setup(world, e, mapload);
        }
        return;
    }
    if is_under(&path, crate::stove::STOVE_ROOT) {
        crate::stove::setup(world, e, mapload);
        return;
    }
    if is_under(&path, crate::gibber::GIBBER_ROOT) {
        crate::gibber::setup(world, e, mapload);
        return;
    }
    if is_under(&path, crate::coffee::COFFEEMAKER_ROOT) {
        crate::coffee::setup(world, e, mapload);
        return;
    }
    if is_under(&path, MICROWAVE_ROOT) {
        world.entity_mut(e).insert(Microwave::default());
        refresh_microwave(world, e);
    } else if is_under(&path, PROCESSOR_ROOT) {
        world.entity_mut(e).insert(Processor::default());
    } else if is_under(&path, GRINDER_ROOT) {
        let beaker = world
            .get::<TilePos>(e)
            .copied()
            .filter(|_| mapload)
            .and_then(|pos| {
                crate::construction::spawn_obj(world, ss13_mapload::food::GRINDER_BEAKER, pos)
            });
        if let Some(beaker) = beaker {
            put_inside(world, beaker, e);
        }
        world.entity_mut(e).insert(Grinder {
            beaker,
            ends_at: None,
        });
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn contents(world: &World, machine: Entity) -> Vec<Entity> {
    world
        .get::<Stored>(machine)
        .map(|s| s.items().to_vec())
        .unwrap_or_default()
}

fn w_class(world: &World, item: Entity) -> u8 {
    world
        .get::<DmWeight>(item)
        .copied()
        .unwrap_or_default()
        .w_class
}

/// tg `use_energy(amount)` of a machine from its area (grid surplus, then the APC cell).
pub(crate) fn machine_use_energy(world: &mut World, machine: Entity, joules: f64) -> f64 {
    crate::power::machine_use_energy(world, machine, joules, false)
}

fn active_power(world: &World, machine: Entity) -> f64 {
    world
        .get::<DmPower>(machine)
        .map_or(0.0, |p| p.active_power_usage)
}

/// tg `item_interaction` of the kitchen machines with `held`. True when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let combat = world.get::<CombatMode>(mob).is_some_and(|c| c.0);
    if world.get::<Microwave>(target).is_some() {
        if world.get::<crate::storage::Storage>(held).is_some() {
            handle_dumping(world, client, mob, held, target);
            return true;
        }
        return microwave_insert(world, client, mob, held, target, combat);
    }
    if world.get::<Processor>(target).is_some() {
        if combat {
            return false;
        }
        processor_insert(world, client, mob, held, target);
        return true;
    }
    if world.get::<Grinder>(target).is_some() {
        return grinder_insert(world, client, mob, held, target, combat);
    }
    false
}

/// An empty-hand click on a kitchen machine (`interact`, `attack_hand_secondary`). True when
/// the click was theirs.
pub fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    right: bool,
) -> bool {
    if world.get::<Microwave>(target).is_some() {
        microwave_interact(world, client, target, right);
        return true;
    }
    if world.get::<Processor>(target).is_some() {
        processor_interact(world, client, mob, target);
        return true;
    }
    if world.get::<Grinder>(target).is_some() {
        grinder_interact(world, client, mob, target, right);
        return true;
    }
    false
}

/// Out of the mob's hands into `machine`.
/// `/obj/item/storage/bag/tray`: loads at once.
const TRAY: &str = "/obj/item/storage/bag/tray";
/// `handle_dumping`: `do_after(user, 2 SECONDS, target = tool)`.
const DUMP_DELAY_DS: f32 = 20.0;

/// tg `/obj/machinery/microwave/handle_dumping` (a storage used on it): a tray at once, any
/// other storage after a `do_after`.
// shortcut: the `do_after` targets the microwave (tg: the storage in hand, which has no tile
// here); the user moving or switching hands still interrupts it.
pub(crate) fn handle_dumping(
    world: &mut World,
    client: Entity,
    mob: Entity,
    storage: Entity,
    mw: Entity,
) {
    let tray = world
        .get::<ss13_mapload::DmItem>(storage)
        .is_some_and(|i| is_under(&i.path, TRAY));
    if tray {
        load_from(world, client, storage, mw);
        return;
    }
    let (what, name) = (name_of(world, storage), name_of(world, mw));
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы начинаете перемещать содержимое {what} в {name}..."),
    );
    let actor = crate::tools::Actor {
        client,
        mob,
        item: Some(storage),
    };
    crate::tools::do_after(
        world,
        actor,
        mw,
        DUMP_DELAY_DS,
        0,
        crate::construction::Job::Interact(crate::drag::DragJob::LoadMicrowave { storage }),
    );
}

/// The rest of `handle_dumping`: every edible item of `storage` in, until the microwave (an
/// oven's tray, a griddle: [`crate::cooking::load_from`]) is full.
pub fn load_from(world: &mut World, client: Entity, storage: Entity, mw: Entity) {
    if crate::cooking::load_from(world, client, storage, mw) {
        return;
    }
    if world
        .get::<Microwave>(mw)
        .is_none_or(|m| m.operating.is_some())
    {
        return;
    }
    let mut loaded = 0;
    for item in contents(world, storage) {
        if world.get::<DmFood>(item).is_none() {
            continue;
        }
        if contents(world, mw).len() >= microwave_max_items(world, mw) {
            tell(world, client, Span::Warning, "заполнено!");
            break;
        }
        put_inside(world, item, mw);
        loaded += 1;
    }
    if loaded > 0 {
        open_door(world, mw, OPEN_AFTER_INSERT);
        let name = name_of(world, mw);
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы вставляете {loaded} {} в {name}.", items_word(loaded)),
        );
    }
}

/// `declension_ru(n, "предмет", "предмета", "предметов")`.
pub(crate) fn items_word(n: usize) -> &'static str {
    match (n % 10, n % 100) {
        (1, h) if h != 11 => "предмет",
        (2..=4, h) if !(12..=14).contains(&h) => "предмета",
        _ => "предметов",
    }
}

fn insert_held(world: &mut World, item: Entity, machine: Entity) {
    world.entity_mut(item).remove::<CarriedBy>();
    put_inside(world, item, machine);
}

/// Everything inside `machine` (but `keep`) onto its tile.
fn eject(world: &mut World, machine: Entity, keep: Option<Entity>) {
    let Some(&pos) = world.get::<TilePos>(machine) else {
        return;
    };
    for item in contents(world, machine) {
        if Some(item) != keep {
            take_out(world, item, pos);
        }
    }
    crate::construction::tile_changed(world, pos);
}

// --- Microwave ---------------------------------------------------------------------------

/// tg `update_icon_state` (`base_icon_state` is empty).
fn refresh_microwave(world: &mut World, e: Entity) {
    let Some(mw) = world.get::<Microwave>(e).cloned() else {
        return;
    };
    let state = if mw.broken > 0 {
        "mwb"
    } else if mw.dirty >= MAX_MICROWAVE_DIRTINESS {
        if mw.open_until.is_some() {
            "mwbloodyo"
        } else {
            "mwbloody"
        }
    } else if matches!(mw.operating, Some(c) if c.kind == CookKind::Muck) {
        "mwbloody1"
    } else if mw.operating.is_some() {
        "back_on"
    } else if mw.open_until.is_some() {
        "back_open"
    } else {
        "back_off"
    };
    // `update_overlays`: the door, the frame and the handle while clean and working.
    let open = mw.open_until.is_some();
    let overlays: Vec<&str> = if mw.broken > 0 || mw.dirty >= MAX_MICROWAVE_DIRTINESS {
        Vec::new()
    } else if open {
        vec!["door_open", "mwo"]
    } else if mw.operating.is_some() {
        vec!["door_on", "mw1", "door_handle"]
    } else {
        vec!["door_off", "mw", "door_handle"]
    };
    let (Some(atom), Some(icons)) = (world.get::<DmAtom>(e), world.get_resource::<IconIndex>())
    else {
        return;
    };
    let id = icons.get(&atom.icon, state);
    let overlays: Vec<Overlay> = overlays
        .into_iter()
        .filter_map(|o| icons.get(&atom.icon, o).map(Overlay::new))
        .collect();
    let Some(mut appearance) = world.get_mut::<Appearance>(e) else {
        return;
    };
    if let Some(id) = id
        && appearance.icon_state != id
    {
        appearance.icon_state = id;
    }
    if appearance.overlays != overlays {
        appearance.overlays = overlays;
    }
}

fn open_door(world: &mut World, e: Entity, secs: f64) {
    let until = now(world) + secs;
    if let Some(mut mw) = world.get_mut::<Microwave>(e) {
        mw.open_until = Some(until);
    }
    refresh_microwave(world, e);
}

fn microwave_insert(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    mw_entity: Entity,
    combat: bool,
) -> bool {
    let mw = world.get::<Microwave>(mw_entity).cloned().expect("checked");
    if mw.operating.is_some() {
        return false;
    }
    let edible = world.get::<DmFood>(held).is_some();
    if mw.dirty >= MAX_MICROWAVE_DIRTINESS {
        if edible {
            tell(world, client, Span::Warning, "слишком грязно!");
        }
        return edible;
    }
    if mw.broken > 0 {
        if edible {
            tell(world, client, Span::Warning, "сломано!");
        }
        return edible;
    }
    if w_class(world, held) > WEIGHT_CLASS_NORMAL
        || combat
        || world.get::<crate::storage::Storage>(held).is_some()
    {
        return false;
    }
    if contents(world, mw_entity).len() >= microwave_max_items(world, mw_entity) {
        tell(world, client, Span::Warning, "заполнено!");
        return true;
    }
    let (user, item, name) = (
        name_of(world, mob),
        name_of(world, held),
        name_of(world, mw_entity),
    );
    insert_held(world, held, mw_entity);
    open_door(world, mw_entity, OPEN_AFTER_INSERT);
    visible(
        world,
        mob,
        &format!("Вы добавляете {item} в {name}..."),
        &format!("{user} добавляет {item} в {name}."),
        Span::Notice,
    );
    true
}

fn microwave_interact(world: &mut World, client: Entity, e: Entity, right: bool) {
    let mw = world.get::<Microwave>(e).cloned().expect("checked");
    if mw.operating.is_some() {
        return;
    }
    if contents(world, e).is_empty() {
        tell(world, client, Span::Warning, "пусто!");
        return;
    }
    if right {
        eject(world, e, None);
        return;
    }
    cook(world, e);
}

/// tg `cook()`: the checks, then a normal, a mucky or a possibly failing cycle loop.
fn cook(world: &mut World, e: Entity) {
    let mw = world.get::<Microwave>(e).cloned().expect("checked");
    if world.get::<Unpowered>(e).is_some()
        || mw.operating.is_some()
        || mw.broken > 0
        || mw.dirty >= MAX_MICROWAVE_DIRTINESS
    {
        return;
    }
    let non_food = contents(world, e)
        .iter()
        .any(|&i| world.get::<DmFood>(i).is_none());
    // `prob(max((5 / efficiency) - 5, dirty * 5))`: a clean tier 1 microwave never mucks.
    let efficiency = microwave_efficiency(world, e);
    let fail = 5.0 / efficiency - 5.0;
    let mut rng = world.resource_mut::<CombatRng>();
    let muck = rng.chance((fail as f32).max(mw.dirty as f32 * 5.0));
    let risky = rng.chance((mw.dirty as f32 * 5.0).min(100.0));
    let (kind, cycles) = if muck {
        (CookKind::Muck, SHORT_CYCLES)
    } else if non_food && risky {
        (CookKind::Pre, SHORT_CYCLES)
    } else {
        (CookKind::Normal, COOK_CYCLES)
    };
    start_loop(world, e, kind, cycles);
}

/// tg `wzhzhzh()` and the first `cook_loop`.
fn start_loop(world: &mut World, e: Entity, kind: CookKind, cycles: u32) {
    let name = name_of(world, e);
    let next_cycle = now(world);
    if let Some(mut mw) = world.get_mut::<Microwave>(e) {
        mw.operating = Some(Cooking {
            kind,
            cycles_left: cycles,
            next_cycle,
        });
        mw.open_until = None;
    }
    refresh_microwave(world, e);
    visible(world, e, "", &format!("{name} включается!"), Span::Notice);
}

/// One `cook_loop` step of a running microwave whose cycle is due.
fn microwave_cycle(world: &mut World, e: Entity, cooking: Cooking) {
    if world.get::<Unpowered>(e).is_some() {
        // tg `power_change()`: `pre_fail()` and `eject()`.
        let name = name_of(world, e);
        if let Some(mut mw) = world.get_mut::<Microwave>(e) {
            mw.broken = 2;
            mw.operating = None;
        }
        visible(
            world,
            e,
            "",
            &format!("Искры разлетаются вокруг {name}!"),
            Span::Warning,
        );
        after_finish_loop(world, e);
        return;
    }
    if cooking.cycles_left == 0 || contents(world, e).is_empty() {
        match cooking.kind {
            CookKind::Normal => loop_finish(world, e),
            CookKind::Muck => {
                let name = name_of(world, e);
                visible(
                    world,
                    e,
                    "",
                    &format!("{name} покрывается грязью!"),
                    Span::Warning,
                );
                if let Some(mut mw) = world.get_mut::<Microwave>(e) {
                    mw.dirty = MAX_MICROWAVE_DIRTINESS;
                    mw.operating = None;
                }
                after_finish_loop(world, e);
            }
            // `pre_success()`: the machine is not broken, so the normal loop follows.
            CookKind::Pre => start_loop(world, e, CookKind::Normal, COOK_CYCLES),
        }
        return;
    }
    machine_use_energy(world, e, active_power(world, e));
    let next = now(world) + microwave_cycle_secs(microwave_efficiency(world, e));
    if let Some(mut mw) = world.get_mut::<Microwave>(e) {
        mw.operating = Some(Cooking {
            cycles_left: cooking.cycles_left - 1,
            next_cycle: next,
            ..cooking
        });
    }
}

/// tg `loop_finish()`: every item's `microwave_act`, then `after_finish_loop()`.
fn loop_finish(world: &mut World, e: Entity) {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let mut cooked = 0;
    for item in contents(world, e) {
        let Some(result) = world
            .get::<DmFood>(item)
            .map(|f| f.microwaved.clone())
            .filter(|r| !r.is_empty())
        else {
            continue;
        };
        if microwave_act(world, item, &result, pos).is_some() {
            cooked += 1;
        }
    }
    if let Some(mut mw) = world.get_mut::<Microwave>(e) {
        mw.operating = None;
        mw.dirty += cooked;
    }
    after_finish_loop(world, e);
}

/// `/datum/element/microwavable/on_microwaved`: the result on the microwave's tile with the
/// source's reagents; the source is deleted.
pub fn microwave_act(
    world: &mut World,
    item: Entity,
    result: &str,
    pos: TilePos,
) -> Option<Entity> {
    let made = crate::construction::spawn_obj(world, result, pos)?;
    // `microwavable`: `ADD_TRAIT(result, TRAIT_HANDMADE, ...)` for the microwaver.
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
    world.despawn(item);
    Some(made)
}

/// tg `after_finish_loop()`: eject and open the door for 2 s.
fn after_finish_loop(world: &mut World, e: Entity) {
    eject(world, e, None);
    open_door(world, e, OPEN_AFTER_COOK);
}

// --- Food processor ----------------------------------------------------------------------

fn processor_insert(world: &mut World, client: Entity, mob: Entity, held: Entity, e: Entity) {
    if world
        .get::<Processor>(e)
        .is_some_and(|p| p.ends_at.is_some())
    {
        let name = name_of(world, e);
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} находится в работе!"),
        );
        return;
    }
    let has_recipe = world
        .get::<DmFood>(held)
        .is_some_and(|f| !f.processor_output.is_empty());
    if !has_recipe {
        tell(
            world,
            client,
            Span::Warning,
            "Это, вероятно, не будет смешиваться!",
        );
        return;
    }
    let (user, item, name) = (name_of(world, mob), name_of(world, held), name_of(world, e));
    insert_held(world, held, e);
    visible(
        world,
        mob,
        &format!("Вы помещаете {item} в {name}."),
        &format!("{user} помещает {item} в {name}."),
        Span::Notice,
    );
}

/// tg `interact()` -> `processing()`.
fn processor_interact(world: &mut World, client: Entity, mob: Entity, e: Entity) {
    let name = name_of(world, e);
    if world
        .get::<Processor>(e)
        .is_some_and(|p| p.ends_at.is_some())
    {
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} находится в работе!"),
        );
        return;
    }
    let items = contents(world, e);
    if items.is_empty() {
        tell(
            world,
            client,
            Span::Warning,
            format!("Внутри {name} пусто!"),
        );
        return;
    }
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        &format!("Вы включаете {name}."),
        &format!("{user} включает {name}."),
        Span::Notice,
    );
    machine_use_energy(world, e, active_power(world, e));
    let total_ds: f32 = items
        .iter()
        .filter_map(|&i| world.get::<DmFood>(i).map(|f| f.processor_time))
        .sum();
    let (_, speed) = processor_ratings(world, e);
    let ends_at = now(world) + f64::from(total_ds) / 10.0 / f64::from(speed);
    if let Some(mut p) = world.get_mut::<Processor>(e) {
        p.ends_at = Some(ends_at);
    }
}

/// tg `complete_processing()` -> `process_food()` for every input.
fn complete_processing(world: &mut World, e: Entity) {
    if let Some(mut p) = world.get_mut::<Processor>(e) {
        p.ends_at = None;
    }
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let chem = world.get_resource::<Chem>().map(|c| c.0.clone());
    let (amount, _) = processor_ratings(world, e);
    for item in contents(world, e) {
        let Some(dm) = world.get::<DmFood>(item).cloned() else {
            continue;
        };
        if dm.processor_output.is_empty() {
            continue;
        }
        // `food_multiplier * rating_amount`.
        let multiplier = dm.processor_multiplier.max(1) * amount.max(1);
        let source = world.get::<Reagents>(item).map(|r| r.0.clone());
        for _ in 0..multiplier {
            let Some(made) = crate::construction::spawn_obj(world, &dm.processor_output, pos)
            else {
                continue;
            };
            if let (Some(source), Some(chem)) = (&source, &chem)
                && let Some(mut target) = world.get_mut::<Reagents>(made)
            {
                target.0.clear();
                let total = source.total_volume;
                source.clone().trans_to(
                    chem,
                    &mut target.0,
                    total,
                    1.0 / multiplier as f32,
                    None,
                    false,
                    true,
                );
            }
        }
        world.despawn(item);
    }
    let name = name_of(world, e);
    visible(
        world,
        e,
        "",
        &format!("{name} заканчивает обработку."),
        Span::Notice,
    );
}

// --- Grinder -----------------------------------------------------------------------------

fn grinder_insert(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    e: Entity,
    combat: bool,
) -> bool {
    let grinder = world.get::<Grinder>(e).cloned().expect("checked");
    let open_container = world
        .get::<Reagents>(held)
        .is_some_and(|r| r.0.is_open_container())
        && world.get::<ss13_mapload::DmReagents>(held).is_some();
    if combat && !open_container {
        return false;
    }
    if grinder.ends_at.is_some() {
        return true;
    }
    let (item, name) = (name_of(world, held), name_of(world, e));
    if open_container {
        // `replace_beaker`: the old beaker to the user's hands, the new one in.
        let hand = world
            .get::<Inventory>(mob)
            .and_then(|i| i.hands.iter().position(|&h| h == Some(held)));
        insert_held(world, held, e);
        if let (Some(old), Some(hand), Some(&pos)) =
            (grinder.beaker, hand, world.get::<TilePos>(mob))
        {
            take_out(world, old, pos);
            crate::inventory::pick_up(world, mob, old, hand);
        }
        if let Some(mut g) = world.get_mut::<Grinder>(e) {
            g.beaker = Some(held);
        }
        tell(
            world,
            client,
            Span::Notice,
            format!("You add {item} to {name}."),
        );
        return true;
    }
    let reagents = world
        .get::<Reagents>(held)
        .map_or(0.0, |r| r.0.total_volume);
    if reagents <= 0.0 {
        return false;
    }
    // `load_items`: the total `w_class` stays within `maximum_weight`.
    let weight: u32 = contents(world, e)
        .iter()
        .filter(|&&i| Some(i) != grinder.beaker)
        .map(|&i| u32::from(w_class(world, i)))
        .sum();
    let (_, maximum_weight) = grinder_ratings(world, e);
    if weight + u32::from(w_class(world, held)) > maximum_weight {
        tell(
            world,
            client,
            Span::Warning,
            format!("{item} is too big to fit into {name}."),
        );
        return true;
    }
    insert_held(world, held, e);
    tell(
        world,
        client,
        Span::Notice,
        format!("{item} was loaded into {name}."),
    );
    tell(
        world,
        client,
        Span::Notice,
        format!("{item} was added to {name}."),
    );
    true
}

fn grinder_interact(world: &mut World, client: Entity, mob: Entity, e: Entity, right: bool) {
    let grinder = world.get::<Grinder>(e).cloned().expect("checked");
    if grinder.ends_at.is_some() {
        return;
    }
    if right {
        match grinder.beaker {
            // `attack_hand_secondary` -> `replace_beaker(user)`.
            Some(beaker) => {
                let hand = world.get::<Inventory>(mob).and_then(|i| {
                    let free = i.hands.iter().position(Option::is_none)?;
                    Some(if i.hands[i.active].is_none() {
                        i.active
                    } else {
                        free
                    })
                });
                if let (Some(hand), Some(&pos)) = (hand, world.get::<TilePos>(mob)) {
                    take_out(world, beaker, pos);
                    crate::inventory::pick_up(world, mob, beaker, hand);
                    if let Some(mut g) = world.get_mut::<Grinder>(e) {
                        g.beaker = None;
                    }
                }
            }
            None => eject(world, e, None),
        }
        return;
    }
    grind(world, client, e);
}

/// tg `operate_for(6 SECONDS)` grinding: every ingredient's `grind()` into the beaker at once,
/// the machine busy for the duration.
fn grind(world: &mut World, client: Entity, e: Entity) {
    let grinder = world.get::<Grinder>(e).cloned().expect("checked");
    let Some(beaker) = grinder.beaker else {
        return;
    };
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let full = |world: &World| {
        world
            .get::<Reagents>(beaker)
            .is_none_or(|r| r.0.holder_full())
    };
    if world.get::<Unpowered>(e).is_some() || full(world) {
        return;
    }
    let name = name_of(world, e);
    let mut weight = 0u32;
    for item in contents(world, e) {
        if item == beaker || full(world) {
            continue;
        }
        let source = world
            .get_mut::<Reagents>(item)
            .map(|mut r| std::mem::replace(&mut r.0, Holder::new(0.0, 0, Default::default())));
        let moved = match (source, world.get_mut::<Reagents>(beaker)) {
            (Some(mut source), Some(mut target)) => {
                let total = source.total_volume;
                let moved = source.trans_to(&chem, &mut target.0, total, 1.0, None, false, false);
                world.entity_mut(item).insert(Reagents(source));
                moved
            }
            (Some(source), None) => {
                world.entity_mut(item).insert(Reagents(source));
                0.0
            }
            _ => 0.0,
        };
        if moved <= 0.0 {
            let item_name = name_of(world, item);
            tell(
                world,
                client,
                Span::Danger,
                format!(
                    "{name} shorts out as it tries to grind up {item_name}, and transfers it back to storage."
                ),
            );
            continue;
        }
        weight += u32::from(w_class(world, item));
        // `blended()`: the ground item is gone.
        world.despawn(item);
    }
    // `operate_for`: `duration = time / speed`, energy `active * duration * weight / maximum`.
    let (speed, maximum_weight) = grinder_ratings(world, e);
    let duration = GRIND_SECS / f64::from(speed);
    let joules = active_power(world, e) * duration * f64::from(weight) / f64::from(maximum_weight);
    machine_use_energy(world, e, joules);
    let ends_at = now(world) + duration;
    if let Some(mut g) = world.get_mut::<Grinder>(e) {
        g.ends_at = Some(ends_at);
    }
}

// --- Timers ------------------------------------------------------------------------------

/// Due microwave cycles and door closings, finished processor and grinder runs.
// Cost: the run condition scans the kitchen machines (a dozen on MetaStation) every frame;
// this exclusive pass runs only while one of them has a timer.
fn kitchen_tick(world: &mut World) {
    let t = now(world);
    let mut query = world.query::<(Entity, &Microwave)>();
    let microwaves: Vec<(Entity, Microwave)> =
        query.iter(world).map(|(e, m)| (e, m.clone())).collect();
    for (e, mw) in microwaves {
        if let Some(cooking) = mw.operating
            && t >= cooking.next_cycle
        {
            microwave_cycle(world, e, cooking);
        }
        if let Some(until) = world.get::<Microwave>(e).and_then(|m| m.open_until)
            && t >= until
        {
            if let Some(mut m) = world.get_mut::<Microwave>(e) {
                m.open_until = None;
            }
            refresh_microwave(world, e);
        }
    }
    let mut query = world.query::<(Entity, &Processor)>();
    let done: Vec<Entity> = query
        .iter(world)
        .filter(|(_, p)| p.ends_at.is_some_and(|end| t >= end))
        .map(|(e, _)| e)
        .collect();
    for e in done {
        complete_processing(world, e);
    }
    let mut query = world.query::<&mut Grinder>();
    for mut g in query.iter_mut(world) {
        if g.ends_at.is_some_and(|end| t >= end) {
            g.ends_at = None;
        }
    }
}
