//! Coffeemakers (server): tg `/obj/machinery/coffeemaker` and `/impressa`
//! (`food_and_drinks/machinery/coffeemaker.dm`), `/obj/item/coffee_cartridge`.
//!
//! - Map load gives a coffeemaker a coffeepot and a basic cartridge (the Impressa only a pot).
//! - Out of combat mode a coffeepot swaps in (the old one into the hands), an empty coffee cup
//!   of the machine's `cup_type`, a full sugar, sweetener or creamer pack go into its
//!   compartments (15 cups, 10 of each pack), a cartridge swaps in. The Impressa takes dried
//!   coffee beans instead of cartridges, one by one or from a coffee pack (10 at most).
//! - An empty hand opens tg's radial menu as a list (eject pot/cartridge, brew, take a cup or a
//!   pack); a right click takes the pot out. Nothing while brewing.
//! - Brewing (`try_brew`: a cartridge with charges or beans, a pot not full, power) uses
//!   `active_power_usage × brew time`, adds 120 coffee to the pot at once (the Impressa: the
//!   bean's reagents that a fresh bean does not have, then coffee up to 120) and is busy for
//!   `brew_time / speed` (`speed`: the micro-lasers' tier sum).
// shortcut: no sounds, steam particles, or examining from the radial menu; the cups and packs put
// back are counted and deleted, as tg keeps them unreachable inside.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmFood, DmItem, DmPower, IconIndex};
use ss13_reagents::is_under;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};

use crate::chem::{Chem, Reagents, name_of};
use crate::combat::CombatMode;
use crate::inventory::CarriedBy;
use crate::stock_parts::PartKind;
use crate::storage::{Stored, put_inside};
use crate::tools::tell;

pub const COFFEEMAKER_ROOT: &str = "/obj/machinery/coffeemaker";
const IMPRESSA: &str = "/obj/machinery/coffeemaker/impressa";
const COFFEEPOT: &str = "/obj/item/reagent_containers/cup/coffeepot";
const BLUESPACE_POT: &str = "/obj/item/reagent_containers/cup/coffeepot/bluespace";
const CARTRIDGE: &str = "/obj/item/coffee_cartridge";
const COFFEE_CUP: &str = "/obj/item/reagent_containers/cup/glass/coffee_cup";
const IMPRESSA_CUP: &str = "/obj/item/reagent_containers/cup/glass/coffee";
const IMPRESSA_CUP_OUT: &str = "/obj/item/reagent_containers/cup/glass/coffee/no_lid";
const SUGAR: &str = "/obj/item/reagent_containers/condiment/pack/sugar";
const SWEETENER: &str = "/obj/item/reagent_containers/condiment/pack/astrotame";
const CREAMER: &str = "/obj/item/reagent_containers/condiment/creamer";
const BEAN: &str = "/obj/item/food/grown/coffee";
const COFFEEPACK: &str = "/obj/item/storage/box/coffeepack";
const COFFEE: &str = "/datum/reagent/consumable/coffee";
/// `/obj/item/coffee_cartridge` `charges`, `drink_type` (coffee 120 for every cartridge).
const CARTRIDGE_CHARGES: u32 = 4;
const BREW_COFFEE: f32 = 120.0;
/// `BEAN_CAPACITY`, `max_coffee_cups`, `max_*_packs`.
const BEAN_CAPACITY: usize = 10;
const MAX_CUPS: u32 = 15;
const MAX_PACKS: u32 = 10;
/// `brew_time`, seconds.
const BREW_SECS: f64 = 20.0;
const IMPRESSA_BREW_SECS: f64 = 15.0;

/// A coffeemaker's live state.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct CoffeeMaker {
    pub impressa: bool,
    pub pot: Option<Entity>,
    pub cartridge: Option<Entity>,
    /// The Impressa's beans, oldest first.
    pub beans: Vec<Entity>,
    pub cups: u32,
    pub sugar: u32,
    pub sweetener: u32,
    pub creamer: u32,
    /// `brewing`: until when (game seconds).
    pub brewing_until: Option<f64>,
}

/// Charges left in a coffee cartridge.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cartridge {
    pub charges: u32,
}

/// The radial menu options, in tg's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    EjectPot,
    EjectCartridge,
    Brew,
    TakeCup,
    TakeSugar,
    TakeSweetener,
    TakeCreamer,
}

impl Choice {
    fn label(self) -> &'static str {
        match self {
            Self::EjectPot => "Извлечь кофейник",
            Self::EjectCartridge => "Извлечь картридж",
            Self::Brew => "Заварить",
            Self::TakeCup => "Взять чашку",
            Self::TakeSugar => "Взять сахар",
            Self::TakeSweetener => "Взять подсластитель",
            Self::TakeCreamer => "Взять сливки",
        }
    }
}

/// On a prompt: the coffeemaker it is the radial menu of, and its options.
#[derive(Component, Clone, Debug)]
struct MenuFor(Entity, Vec<Choice>);

pub struct CoffeeServerPlugin;

impl Plugin for CoffeeServerPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `ui_interact` (the radial menu) and `attack_hand_secondary` of coffeemakers.
        app.add_click_handler(ClickPhase::EmptyHand, 900, |w, c| {
            crate::coffee::interact(w, c.client, c.mob, c.target, c.modifiers.right)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 2100, |w, c| {
            c.held
                .is_some_and(|h| crate::coffee::item_interaction(w, c.client, c.mob, h, c.target))
        });
        app.add_systems(Startup, setup_mapped)
            .add_systems(
                Update,
                (
                    stop_brewing.run_if(any_with_component::<CoffeeMaker>),
                    menu_answers
                        .in_set(ss13_tgui::server::UiSystems::Handle)
                        .after(ss13_tgui::prompt::PromptSystems::Answer),
                ),
            )
            .add_observer(cartridge_charges);
    }
}

fn cartridge_charges(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if items
        .get(add.entity)
        .is_ok_and(|i| is_under(&i.path, CARTRIDGE))
    {
        commands.entity(add.entity).insert_if_new(Cartridge {
            charges: CARTRIDGE_CHARGES,
        });
    }
}

fn setup_mapped(world: &mut World) {
    let mut q = world.query::<(Entity, &DmPower)>();
    let found: Vec<Entity> = q
        .iter(world)
        .filter(|(_, p)| is_under(&p.path, COFFEEMAKER_ROOT))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup(world, e, true);
    }
}

/// `Initialize(mapload)`: mapped ones get a pot (and a cartridge unless an Impressa).
pub(crate) fn setup(world: &mut World, e: Entity, mapload: bool) {
    let impressa = path_of(world, e).is_some_and(|p| is_under(&p, IMPRESSA));
    let pos = world.get::<TilePos>(e).copied().filter(|_| mapload);
    let spawn_in = |world: &mut World, path: &str| {
        let item = crate::construction::spawn_obj(world, path, pos?)?;
        put_inside(world, item, e);
        Some(item)
    };
    let pot = spawn_in(world, COFFEEPOT);
    let cartridge = if impressa {
        None
    } else {
        spawn_in(world, CARTRIDGE)
    };
    world.entity_mut(e).insert(CoffeeMaker {
        impressa,
        pot,
        cartridge,
        beans: Vec::new(),
        cups: MAX_CUPS,
        sugar: MAX_PACKS,
        sweetener: MAX_PACKS,
        creamer: MAX_PACKS,
        brewing_until: None,
    });
    refresh_look(world, e);
}

fn path_of(world: &World, e: Entity) -> Option<String> {
    world
        .get::<DmPower>(e)
        .map(|p| p.path.clone())
        .or_else(|| world.get::<DmItem>(e).map(|i| i.path.clone()))
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

/// `RefreshParts`: `speed` is the micro-lasers' tier sum (at least 1 without parts).
pub fn speed(world: &World, e: Entity) -> f64 {
    let r = crate::stock_parts::ratings(world, e);
    f64::from(r.sum(PartKind::MicroLaser).max(1))
}

fn panel_open(world: &World, e: Entity) -> bool {
    world
        .get::<crate::construction::PanelOpen>(e)
        .is_some_and(|p| p.0)
}

fn full_pack(world: &World, item: Entity) -> bool {
    world
        .get::<Reagents>(item)
        .is_some_and(|r| r.0.total_volume >= r.0.maximum_volume)
}

/// Removes `held` from the hands into the machine.
fn take_from_hand(world: &mut World, mob: Entity, held: Entity, into: Entity) {
    world.entity_mut(held).remove::<CarriedBy>();
    if let Some(mut inventory) = world.get_mut::<crate::inventory::Inventory>(mob)
        && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(held))
    {
        *slot = None;
    }
    put_inside(world, held, into);
}

/// tg `item_interaction`. True when the click was the coffeemaker's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    e: Entity,
) -> bool {
    let Some(maker) = world.get::<CoffeeMaker>(e).cloned() else {
        return false;
    };
    if panel_open(world, e) || world.get::<CombatMode>(mob).is_some_and(|c| c.0) {
        return false;
    }
    let path = path_of(world, held).unwrap_or_default();
    let open = world
        .get::<Reagents>(held)
        .is_some_and(|r| r.0.is_open_container());
    if is_under(&path, COFFEEPOT) && open {
        take_from_hand(world, mob, held, e);
        if let Some(old) = maker.pot {
            crate::cooking::give_to(world, mob, e, old);
        }
        set(world, e, |m| m.pot = Some(held));
        tell(world, client, Span::Notice, "кофейник вставлен");
        return true;
    }
    let cup_type = if maker.impressa {
        IMPRESSA_CUP
    } else {
        COFFEE_CUP
    };
    if is_under(&path, cup_type) && open {
        if world
            .get::<Reagents>(held)
            .is_some_and(|r| r.0.total_volume > 0.0)
        {
            tell(world, client, Span::Warning, "чашка должна быть пустой!");
        } else if maker.cups >= MAX_CUPS {
            tell(
                world,
                client,
                Span::Warning,
                "отделение для чашек заполнено!",
            );
        } else {
            world.despawn(held);
            set(world, e, |m| m.cups += 1);
        }
        return true;
    }
    let packs: [(&str, u32, &str, fn(&mut CoffeeMaker)); 3] = [
        (
            SUGAR,
            maker.sugar,
            "отделение для сахара заполнено!",
            |m| m.sugar += 1,
        ),
        (
            CREAMER,
            maker.creamer,
            "отделение для сливок заполнено!",
            |m| m.creamer += 1,
        ),
        (
            SWEETENER,
            maker.sweetener,
            "отделение для подсластителя заполнено!",
            |m| m.sweetener += 1,
        ),
    ];
    for (root, count, full, add) in packs {
        if !is_under(&path, root) {
            continue;
        }
        if !full_pack(world, held) {
            tell(world, client, Span::Warning, "пачка должна быть полной!");
        } else if count >= MAX_PACKS {
            tell(world, client, Span::Warning, full);
        } else {
            world.despawn(held);
            set(world, e, add);
        }
        return true;
    }
    if !maker.impressa && is_under(&path, CARTRIDGE) {
        take_from_hand(world, mob, held, e);
        if let Some(old) = maker.cartridge {
            crate::cooking::give_to(world, mob, e, old);
        }
        set(world, e, |m| m.cartridge = Some(held));
        tell(world, client, Span::Notice, "картридж вставлен");
        return true;
    }
    if maker.impressa && is_under(&path, BEAN) {
        if maker.beans.len() >= BEAN_CAPACITY {
            tell(world, client, Span::Warning, "емкость для кофе заполнена!");
        } else if world.get::<crate::food::Dried>(held).is_none() {
            tell(
                world,
                client,
                Span::Warning,
                "кофейные зерна должны быть сухими!",
            );
        } else {
            take_from_hand(world, mob, held, e);
            set(world, e, |m| m.beans.push(held));
            tell(world, client, Span::Notice, "добавлено кофе");
        }
        return true;
    }
    if maker.impressa && is_under(&path, COFFEEPACK) {
        load_pack(world, client, held, e);
        return true;
    }
    false
}

/// The Impressa's coffee pack branch: its dried beans until the grinder is full; a bean that is
/// not dried stops the loading (tg returns at once).
fn load_pack(world: &mut World, client: Entity, pack: Entity, e: Entity) {
    if world
        .get::<CoffeeMaker>(e)
        .is_some_and(|m| m.beans.len() >= BEAN_CAPACITY)
    {
        tell(world, client, Span::Warning, "емкость для кофе заполнена!");
        return;
    }
    let inside: Vec<Entity> = world
        .get::<Stored>(pack)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    let mut added = false;
    for bean in inside {
        if !path_of(world, bean).is_some_and(|p| is_under(&p, BEAN)) {
            continue;
        }
        if world.get::<crate::food::Dried>(bean).is_none() {
            break;
        }
        if world
            .get::<CoffeeMaker>(e)
            .is_some_and(|m| m.beans.len() >= BEAN_CAPACITY)
        {
            break;
        }
        put_inside(world, bean, e);
        set(world, e, |m| m.beans.push(bean));
        added = true;
    }
    let text = if added {
        "добавлено кофе"
    } else {
        "нет зёрен!"
    };
    tell(world, client, Span::Notice, text);
}

fn set(world: &mut World, e: Entity, f: impl FnOnce(&mut CoffeeMaker)) {
    if let Some(mut m) = world.get_mut::<CoffeeMaker>(e) {
        f(&mut m);
    }
    refresh_look(world, e);
}

/// An empty-hand click: the radial menu (left), the pot out (right). True when it was ours.
pub fn interact(world: &mut World, client: Entity, mob: Entity, e: Entity, right: bool) -> bool {
    let Some(maker) = world.get::<CoffeeMaker>(e).cloned() else {
        return false;
    };
    if maker.brewing_until.is_some() {
        return true;
    }
    if right {
        if let Some(pot) = maker.pot {
            set(world, e, |m| m.pot = None);
            crate::cooking::give_to(world, mob, e, pot);
        }
        return true;
    }
    let mut options = Vec::new();
    if maker.pot.is_some() {
        options.push(Choice::EjectPot);
    }
    if maker.cartridge.is_some() {
        options.push(Choice::EjectCartridge);
    }
    options.push(Choice::Brew);
    for (count, choice) in [
        (maker.cups, Choice::TakeCup),
        (maker.sugar, Choice::TakeSugar),
        (maker.sweetener, Choice::TakeSweetener),
        (maker.creamer, Choice::TakeCreamer),
    ] {
        if count > 0 {
            options.push(choice);
        }
    }
    if let [only] = options[..] {
        choose(world, client, mob, e, only);
        return true;
    }
    let items = options.iter().map(|c| c.label().to_owned()).collect();
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            name_of(world, e),
            "",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(MenuFor(e, options));
    true
}

/// The menu's answers, revalidated (`brewing`, `can_perform_action`).
fn menu_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answers {
        let Some(MenuFor(e, options)) = world.get::<MenuFor>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let Some(&choice) = options.get(i) else {
            continue;
        };
        let able = world.get::<crate::health::Stat>(actor) == Some(&crate::health::Stat::Conscious)
            && !crate::status_effects::incapacitation(world, actor).incapacitated
            && world.get_entity(e).is_ok()
            && crate::storage::in_reach(world, actor, e)
            && world
                .get::<CoffeeMaker>(e)
                .is_some_and(|m| m.brewing_until.is_none());
        if able {
            choose(world, client, actor, e, choice);
        }
    }
}

fn choose(world: &mut World, client: Entity, mob: Entity, e: Entity, choice: Choice) {
    let Some(maker) = world.get::<CoffeeMaker>(e).cloned() else {
        return;
    };
    let take = |world: &mut World, path: &str| -> bool {
        let Some(&pos) = world.get::<TilePos>(e) else {
            return false;
        };
        let Some(item) = crate::construction::spawn_obj(world, path, pos) else {
            return false;
        };
        crate::cooking::give_to(world, mob, e, item);
        true
    };
    match choice {
        Choice::EjectPot => {
            if let Some(pot) = maker.pot {
                set(world, e, |m| m.pot = None);
                crate::cooking::give_to(world, mob, e, pot);
            }
        }
        Choice::EjectCartridge => {
            if let Some(c) = maker.cartridge {
                set(world, e, |m| m.cartridge = None);
                crate::cooking::give_to(world, mob, e, c);
            }
        }
        Choice::Brew => brew(world, client, e),
        Choice::TakeCup if maker.cups > 0 => {
            let cup = if maker.impressa {
                IMPRESSA_CUP_OUT
            } else {
                COFFEE_CUP
            };
            if take(world, cup) {
                set(world, e, |m| m.cups -= 1);
            }
        }
        Choice::TakeSugar if maker.sugar > 0 && take(world, SUGAR) => {
            set(world, e, |m| m.sugar -= 1);
        }
        Choice::TakeSweetener if maker.sweetener > 0 && take(world, SWEETENER) => {
            set(world, e, |m| m.sweetener -= 1);
        }
        Choice::TakeCreamer if maker.creamer > 0 && take(world, CREAMER) => {
            set(world, e, |m| m.creamer -= 1);
        }
        _ => {}
    }
}

/// `try_brew` failures, `None` when it can brew.
fn brew_problem(world: &World, maker: &CoffeeMaker, e: Entity) -> Option<&'static str> {
    if maker.impressa {
        if maker.beans.is_empty() {
            return Some("нет кофейных зерен!");
        }
    } else {
        let Some(cartridge) = maker.cartridge else {
            return Some("не вставлен картридж для кофе!");
        };
        if world
            .get::<Cartridge>(cartridge)
            .is_none_or(|c| c.charges < 1)
        {
            return Some("кофейный картридж пуст!");
        }
    }
    let Some(pot) = maker.pot else {
        return Some("нет кофейника внутри!");
    };
    if world.get::<crate::power::Unpowered>(e).is_some() {
        return Some("кофеварка обесточена!");
    }
    if world
        .get::<Reagents>(pot)
        .is_none_or(|r| r.0.total_volume >= r.0.maximum_volume)
    {
        return Some("кофейник уже полон!");
    }
    None
}

/// `brew`: `operate_for(brew_time)` and the coffee into the pot.
fn brew(world: &mut World, client: Entity, e: Entity) {
    let Some(maker) = world.get::<CoffeeMaker>(e).cloned() else {
        return;
    };
    if let Some(problem) = brew_problem(world, &maker, e) {
        tell(world, client, Span::Warning, problem);
        return;
    }
    let Some(pot) = maker.pot else {
        return;
    };
    let secs = if maker.impressa {
        IMPRESSA_BREW_SECS
    } else {
        BREW_SECS
    };
    let watts = world
        .get::<DmPower>(e)
        .map_or(0.0, |p| p.active_power_usage);
    crate::kitchen::machine_use_energy(world, e, watts * secs);
    let until = now(world) + secs / speed(world, e);
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let Some(coffee) = chem.id(COFFEE) else {
        return;
    };
    let Some(mut holder) = world.get::<Reagents>(pot).map(|r| r.0.clone()) else {
        return;
    };
    if maker.impressa {
        // The newest bean (`coffee[coffee_amount]`), its reagents a fresh bean does not have.
        let Some(&bean) = maker.beans.last() else {
            return;
        };
        let standard: Vec<String> = world
            .get::<DmFood>(bean)
            .map(|f| f.food_reagents.iter().map(|r| r.path.clone()).collect())
            .unwrap_or_default();
        let delta: Vec<_> = world
            .get::<Reagents>(bean)
            .map(|r| {
                r.0.reagents
                    .iter()
                    .filter(|x| !standard.contains(&chem.def(x.id).path))
                    .map(|x| (x.id, x.volume))
                    .collect()
            })
            .unwrap_or_default();
        holder.add_reagent_list(&chem, &delta, None, false);
        // `coffee.Cut(1,2)`: the oldest bean goes.
        let oldest = maker.beans[0];
        world.despawn(oldest);
        if holder.total_volume < BREW_COFFEE {
            let extra = BREW_COFFEE - holder.total_volume;
            holder.add_reagent_list(&chem, &[(coffee, extra)], None, false);
        }
        set(world, e, |m| {
            m.beans.remove(0);
        });
    } else {
        holder.add_reagent_list(&chem, &[(coffee, BREW_COFFEE)], None, false);
        if let Some(mut c) = maker.cartridge.and_then(|c| world.get_mut::<Cartridge>(c)) {
            c.charges = c.charges.saturating_sub(1);
        }
    }
    world.entity_mut(pot).insert(Reagents(holder));
    set(world, e, |m| m.brewing_until = Some(until));
}

/// `stop_operating` of every machine whose brew is done.
fn stop_brewing(world: &mut World) {
    let t = now(world);
    let mut q = world.query::<(Entity, &CoffeeMaker)>();
    let done: Vec<Entity> = q
        .iter(world)
        .filter(|(_, m)| m.brewing_until.is_some_and(|u| u <= t))
        .map(|(e, _)| e)
        .collect();
    for e in done {
        set(world, e, |m| m.brewing_until = None);
    }
}

/// `overlay_checks` (and `/impressa`'s).
fn refresh_look(world: &mut World, e: Entity) {
    let Some(maker) = world.get::<CoffeeMaker>(e).cloned() else {
        return;
    };
    let pot = maker.pot.map(|p| {
        let bluespace = path_of(world, p).is_some_and(|x| is_under(&x, BLUESPACE_POT));
        let full = world
            .get::<Reagents>(p)
            .is_some_and(|r| r.0.total_volume > 0.0);
        (bluespace, full)
    });
    let mut states: Vec<String> = Vec::new();
    let prefix = if maker.impressa { "" } else { "coffeemaker_" };
    if let Some((bluespace, full)) = pot {
        states.push(if bluespace {
            format!("{prefix}pot_bluespace")
        } else {
            format!("{prefix}pot_{}", if full { "full" } else { "empty" })
        });
    }
    if maker.impressa {
        let cups = maker.cups as f32;
        let max = MAX_CUPS as f32;
        if maker.cups > 0 {
            states.push(
                if cups >= max / 3.0 {
                    if cups > max / 1.5 { "cups_3" } else { "cups_2" }
                } else {
                    "cups_1"
                }
                .to_owned(),
            );
        }
        for (count, state) in [
            (maker.sugar, "extras_1"),
            (maker.creamer, "extras_2"),
            (maker.sweetener, "extras_3"),
        ] {
            if count > 0 {
                states.push(state.to_owned());
            }
        }
        if !maker.beans.is_empty() {
            let half = (maker.beans.len() as f32) < 0.7 * BEAN_CAPACITY as f32;
            states.push(if half { "grinder_half" } else { "grinder_full" }.to_owned());
        }
    } else if maker.cartridge.is_some() {
        states.push("coffeemaker_cartidge".to_owned());
    }
    let overlays: Vec<Overlay> = {
        let (Some(atom), Some(icons)) = (world.get::<DmAtom>(e), world.get_resource::<IconIndex>())
        else {
            return;
        };
        states
            .iter()
            .filter_map(|s| icons.get(&atom.icon, s))
            .map(Overlay::new)
            .collect()
    };
    if let Some(mut look) = world.get_mut::<Appearance>(e)
        && look.overlays != overlays
    {
        look.overlays = overlays;
    }
}

/// tg `examine` (in range).
pub fn examine_lines(world: &World, e: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(maker) = world.get::<CoffeeMaker>(e) else {
        return;
    };
    let name = crate::cooking::capitalize(&name_of(world, e));
    if maker.brewing_until.is_some() {
        lines.push((Span::Warning, format!("{name} варит.")));
        return;
    }
    if panel_open(world, e) {
        lines.push((Span::Notice, "Панель для обслуживания открыта!".to_owned()));
        return;
    }
    if maker.pot.is_some() || maker.cartridge.is_some() {
        lines.push((Span::Notice, format!("{name} содержит:")));
        for item in [maker.pot, maker.cartridge].into_iter().flatten() {
            lines.push((Span::Notice, format!("- {}.", name_of(world, item))));
        }
        if let Some(c) = maker.cartridge.and_then(|c| world.get::<Cartridge>(c)) {
            lines.push((
                Span::Notice,
                format!("- порций в кофейном картридже: {}.", c.charges),
            ));
        }
    }
    lines.push((
        Span::Notice,
        format!(
            "Чашек: {}, сахара: {}, подсластителя: {}, сливок: {}.",
            maker.cups, maker.sugar, maker.sweetener, maker.creamer
        ),
    ));
    if maker.impressa && !maker.beans.is_empty() {
        lines.push((
            Span::Notice,
            format!(
                "Внутренняя кофемолка содержит порций кофейных зерен: {}",
                maker.beans.len()
            ),
        ));
    }
}

/// `/obj/item/storage/box/coffeepack/PopulateContents`: its beans come dried (and tan).
pub fn populated(world: &mut World, storage: Entity) {
    if !path_of(world, storage).is_some_and(|p| is_under(&p, COFFEEPACK)) {
        return;
    }
    let beans: Vec<Entity> = world
        .get::<Stored>(storage)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    for bean in beans {
        if path_of(world, bean).is_some_and(|p| is_under(&p, BEAN)) {
            crate::food::dry(world, bean);
        }
    }
}
