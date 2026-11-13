//! The biogenerator (server): tg `/obj/machinery/biogenerator`
//! (`code/modules/hydroponics/biogenerator.dm`, designs in
//! `code/modules/research/designs/biogenerator_designs.dm` and BandaStation's
//! `modular_bandastation/hydroponics/code/biogenerator_designs.dm`) and its `Biogenerator` window
//! (`ss13_tgui::interfaces::biogenerator`).
//!
//! - Food goes in by hand or out of a storage bag, up to `max_items` (10 + 10 per matter bin
//!   tier); a cup goes into its container slot (swapping the old one into the hand).
//! - "Generate" (with food inside, powered, panel closed) runs it: every `SSmachines` fire (2 s)
//!   it turns `processed_items_per_cycle` (5 per servo tier) foods into biomass, each worth its
//!   nutriment (and subtypes) rounded up times `productivity` (the servo tiers), until the food
//!   is gone.
//! - "Create" of an `initial` biogenerator design: `amount` (1 to the free space of the
//!   container for a reagent, else to 50) costs the design's biomass × amount / `efficiency`
//!   (the servo tiers); a reagent goes into the container at purity 0.3, an item or a stack
//!   onto its tile.
//! - Its look: the panel, container, biomass level (7 steps up to 5000), processing and screen
//!   overlays.
// shortcut: no welding to the floor, wrench anchoring, alt-click eject, biomass spill on
// deconstruction, light, sound loop or emissive overlays; the window's container bar is green
// instead of the reagents' colour; another item on it says nothing (tools keep working).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::research::build_type;
use ss13_mapload::{DmItem, DmObj, IconIndex};
use ss13_reagents::is_under;
use ss13_shared::chat::Span;
use ss13_shared::ui::UiStatus;
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::biogenerator::{
    BiogeneratorCategory, BiogeneratorData, BiogeneratorDesign, BiogeneratorStatic, BiogeneratorUi,
};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiKind, UiParams, UiReject, UiSystems, UiWindow,
};

use crate::chem::{Chem, Reagents, name_of};
use crate::medical::pods::{operational, panel_open};
use crate::science::techweb::ResearchDb;
use crate::stock_parts::PartKind;
use crate::storage::{Stored, put_inside};
use crate::tools::tell;

pub const BIOGENERATOR: &str = "/obj/machinery/biogenerator";
const BIOGEN_DMI: &str = "icons/obj/machines/biogenerator.dmi";
const CUP_ROOT: &str = "/obj/item/reagent_containers/cup";
const BAG_ROOT: &str = "/obj/item/storage/bag";
const NUTRIMENT: &str = "/datum/reagent/consumable/nutriment";
const BIOMASS: &str = "/datum/material/biomass";
/// `RND_CATEGORY_INITIAL` and the tabs (`show_categories`: `RND_CATEGORY_BIO_*`).
const INITIAL: &str = "initial";
const CATEGORIES: [&str; 3] = ["Еда", "Химикаты", "Материалы"];
/// `MAX_ITEMS_PER_RATING`, `PROCESSED_ITEMS_PER_RATING`.
const MAX_ITEMS_PER_RATING: u32 = 10;
const PROCESSED_ITEMS_PER_RATING: u32 = 5;
/// `max_visual_biomass`, `max_output`, `BIOGEN_REAGENT_PURITY`.
const MAX_VISUAL_BIOMASS: f32 = 5000.0;
const MAX_OUTPUT: u32 = 50;
const REAGENT_PURITY: f32 = 0.3;
/// `SSmachines` `wait`, seconds.
const MACHINES_WAIT: f64 = 2.0;

/// A biogenerator's `processing`, `biomass` and container (`beaker`, inside it).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Biogenerator {
    pub processing: bool,
    pub biomass: f32,
    pub beaker: Option<Entity>,
    view: BiogeneratorData,
    static_view: BiogeneratorStatic,
}

/// `RefreshParts`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BiogenStats {
    pub efficiency: f32,
    pub productivity: f32,
    pub per_cycle: u32,
    pub max_items: usize,
}

impl BiogenStats {
    /// From the matter bins' and the servos' tier sums.
    // tg divides by `efficiency` (the servo tiers): a frame without servos counts as tier 1.
    pub fn from_tiers(bins: u32, servos: u32) -> Self {
        let servos = servos.max(1);
        Self {
            efficiency: servos as f32,
            productivity: servos as f32,
            per_cycle: PROCESSED_ITEMS_PER_RATING * servos,
            max_items: (10 + MAX_ITEMS_PER_RATING * bins) as usize,
        }
    }
}

fn stats(world: &World, e: Entity) -> BiogenStats {
    let r = crate::stock_parts::ratings(world, e);
    BiogenStats::from_tiers(r.sum(PartKind::MatterBin), r.sum(PartKind::Servo))
}

pub(super) fn build(app: &mut App) {
    app.register_ui::<BiogeneratorWindow>()
        .add_observer(attach)
        .add_systems(
            Update,
            (
                cap_windows.in_set(UiSystems::StatusHook),
                acts.in_set(UiSystems::Handle),
                refresh_views
                    .after(UiSystems::Handle)
                    .before(UiSystems::Send),
                process.run_if(any_with_component::<Biogenerator>),
            ),
        );
}

fn attach(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(o) = objs.get(add.entity) else {
        return;
    };
    if is_under(&o.path, BIOGENERATOR) {
        commands
            .entity(add.entity)
            .insert_if_new(Biogenerator::default());
    }
}

fn path_of(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmItem>(e).map(|i| i.path.as_str())
}

/// The food inside (`locate(/obj/item/food) in contents`).
fn foods(world: &World, e: Entity) -> Vec<Entity> {
    world
        .get::<Stored>(e)
        .map(|s| {
            s.items()
                .iter()
                .copied()
                .filter(|&i| path_of(world, i).is_some_and(crate::food::is_food))
                .collect()
        })
        .unwrap_or_default()
}

/// `ui_interact` with an empty hand. True when `target` is a biogenerator.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<Biogenerator>(target).is_none() {
        return false;
    }
    if !operational(world, target) || panel_open(world, target) {
        return true;
    }
    refresh_static(world, target);
    refresh_view(world, target);
    let name = name_of(world, target);
    world.write_message(OpenUi::new::<BiogeneratorWindow>(client, mob, target, name));
    true
}

/// `item_interaction`: a cup into the container slot, a bag's food or a food inside. True when
/// the click was the biogenerator's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<Biogenerator>(target).is_none()
        || world
            .get::<crate::combat::CombatMode>(mob)
            .is_some_and(|c| c.0)
    {
        return false;
    }
    let Some(path) = path_of(world, held).map(str::to_owned) else {
        return false;
    };
    let (what, here) = (name_of(world, held), name_of(world, target));
    if is_under(&path, CUP_ROOT) {
        if panel_open(world, target) {
            tell(
                world,
                client,
                Span::Warning,
                "Сначала закройте панель техобслуживания!",
            );
            return true;
        }
        insert_beaker(world, client, mob, target, held);
        return true;
    }
    let max = stats(world, target).max_items;
    let count = foods(world, target).len();
    if is_under(&path, BAG_ROOT) {
        if count >= max {
            tell(
                world,
                client,
                Span::Warning,
                format!("{here} уже заполнен! Запустите его, чтобы освободить место."),
            );
            return true;
        }
        let inside = foods(world, held);
        let room = max - count;
        let moved = inside.len().min(room);
        for &food in inside.iter().take(room) {
            put_inside(world, food, target);
        }
        let text = if moved == inside.len() {
            format!("Вы высыпаете {what} в {here}.")
        } else if count + moved >= max {
            format!("Вы заполняете {here} из {what} до отказа.")
        } else {
            format!("Вы заполняете {here} из {what}.")
        };
        tell(world, client, Span::Info, text);
        refresh_view(world, target);
        return true;
    }
    if crate::food::is_food(&path) {
        if count >= max {
            tell(
                world,
                client,
                Span::Warning,
                format!("{here} уже заполнен! Запустите его, чтобы освободить место."),
            );
            return true;
        }
        put_inside(world, held, target);
        tell(
            world,
            client,
            Span::Info,
            format!("Вы помещаете {what} в {here}."),
        );
        refresh_view(world, target);
        return true;
    }
    false
}

/// `insert_beaker`: the cup goes in, the old one comes out into the hand.
fn insert_beaker(world: &mut World, client: Entity, mob: Entity, e: Entity, cup: Entity) {
    let old = world.get::<Biogenerator>(e).and_then(|b| b.beaker);
    put_inside(world, cup, e);
    let (new_name, here) = (name_of(world, cup), name_of(world, e));
    match old {
        Some(old) => {
            let old_name = name_of(world, old);
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы меняете {old_name} в {here} на {new_name}."),
            );
            eject_beaker(world, None, mob, e);
        }
        None => tell(
            world,
            client,
            Span::Notice,
            format!("Вы ставите {new_name} в {here}."),
        ),
    }
    if let Some(mut b) = world.get_mut::<Biogenerator>(e) {
        b.beaker = Some(cup);
    }
    refresh_look(world, e);
    refresh_view(world, e);
}

/// `eject_beaker`: into the hands, else onto the floor; `client` gets the message.
fn eject_beaker(world: &mut World, client: Option<Entity>, mob: Entity, e: Entity) {
    let Some(beaker) = world.get::<Biogenerator>(e).and_then(|b| b.beaker) else {
        return;
    };
    if let Some(mut b) = world.get_mut::<Biogenerator>(e) {
        b.beaker = None;
    }
    let near = world
        .get::<TilePos>(mob)
        .zip(world.get::<TilePos>(e))
        .and_then(|(a, b)| a.chebyshev(*b))
        .is_some_and(|d| d <= 1);
    let name = name_of(world, beaker);
    let free_hand = world
        .get::<crate::inventory::Inventory>(mob)
        .filter(|i| near && i.hands.iter().any(Option::is_none))
        .cloned();
    let text = match free_hand {
        Some(inventory) => {
            crate::storage::put_in_hands(world, mob, &inventory, beaker);
            format!("Вы извлекаете {name}.")
        }
        None => {
            if let Some(&pos) = world.get::<TilePos>(e) {
                crate::storage::take_out(world, beaker, pos);
            }
            format!("Вы извлекаете {name} на пол.")
        }
    };
    if let Some(client) = client {
        tell(world, client, Span::Notice, text);
    }
    refresh_look(world, e);
}

/// `start_process`.
fn start_process(world: &mut World, e: Entity) {
    if !operational(world, e) || panel_open(world, e) {
        return;
    }
    if world.get::<Biogenerator>(e).is_some_and(|b| b.processing) {
        crate::atmos_machinery::machine_say(world, e, "Уже работаю!");
        return;
    }
    if foods(world, e).is_empty() {
        crate::atmos_machinery::machine_say(world, e, "Продукты не обнаружены!");
        return;
    }
    if let Some(mut b) = world.get_mut::<Biogenerator>(e) {
        b.processing = true;
    }
    refresh_look(world, e);
}

/// `convert_to_biomass`: the food's nutriment (and subtypes), rounded up, times productivity.
pub fn biomass_of(world: &World, food: Entity, productivity: f32) -> f32 {
    let Some(chem) = world.get_resource::<Chem>() else {
        return 0.0;
    };
    let nutriment: f32 = world
        .get::<Reagents>(food)
        .map(|r| {
            r.0.reagents
                .iter()
                .filter(|x| is_under(&chem.0.def(x.id).path, NUTRIMENT))
                .map(|x| x.volume)
                .sum()
        })
        .unwrap_or(0.0);
    nutriment.ceil() * productivity
}

/// `process` every `SSmachines` fire of the running ones.
// Cost: one pass over the biogenerators (a few per map) every 2 s.
fn process(world: &mut World, mut next: Local<f64>) {
    let now = crate::tools::now(world);
    if now < *next {
        return;
    }
    *next = now + MACHINES_WAIT;
    let running: Vec<Entity> = world
        .query::<(Entity, &Biogenerator)>()
        .iter(world)
        .filter(|(_, b)| b.processing)
        .map(|(e, _)| e)
        .collect();
    for e in running {
        process_one(world, e);
        refresh_look(world, e);
        refresh_view(world, e);
    }
}

fn process_one(world: &mut World, e: Entity) {
    let stop = |world: &mut World| {
        if let Some(mut b) = world.get_mut::<Biogenerator>(e) {
            b.processing = false;
        }
    };
    if !operational(world, e) || panel_open(world, e) || foods(world, e).is_empty() {
        stop(world);
        return;
    }
    let s = stats(world, e);
    let mut gained = 0.0;
    for food in foods(world, e).into_iter().take(s.per_cycle as usize) {
        gained += biomass_of(world, food, s.productivity);
        world.despawn(food);
    }
    let watts = world
        .get::<ss13_mapload::DmPower>(e)
        .map_or(0.0, |p| p.active_power_usage);
    crate::kitchen::machine_use_energy(world, e, watts * MACHINES_WAIT);
    if let Some(mut b) = world.get_mut::<Biogenerator>(e) {
        b.biomass += gained;
    }
    if foods(world, e).is_empty() {
        stop(world);
    }
}

/// The designs it prints: `initial` biogenerator designs (`researched_designs` of the
/// autounlocking techweb).
fn designs(db: &ResearchDb) -> impl Iterator<Item = (usize, &ss13_mapload::research::DesignDef)> {
    db.designs.iter().enumerate().filter(|(_, d)| {
        d.build_type & build_type::BIOGENERATOR != 0 && d.category.iter().any(|c| c == INITIAL)
    })
}

/// The design's biomass per unit (`materials[biomass]`); `None` for anything else.
fn biomass_cost(d: &ss13_mapload::research::DesignDef) -> Option<f32> {
    match d.materials.as_slice() {
        [(m, n)] if m == BIOMASS => Some(*n as f32),
        _ => None,
    }
}

fn compute_static(world: &World, e: Entity) -> BiogeneratorStatic {
    let efficiency = stats(world, e).efficiency;
    let Some(db) = world.get_resource::<ResearchDb>() else {
        return BiogeneratorStatic::default();
    };
    let categories = CATEGORIES
        .iter()
        .map(|&category| BiogeneratorCategory {
            name: category.to_owned(),
            items: designs(db)
                .filter(|(_, d)| d.category.iter().any(|c| c == category))
                .filter_map(|(_, d)| {
                    Some(BiogeneratorDesign {
                        path: d.id.clone(),
                        name: d.name.clone(),
                        is_reagent: d.make_reagent.is_some(),
                        cost: biomass_cost(d)? / efficiency,
                    })
                })
                .collect(),
        })
        .collect();
    BiogeneratorStatic {
        categories,
        max_visual_biomass: MAX_VISUAL_BIOMASS,
    }
}

fn compute_view(world: &World, e: Entity) -> BiogeneratorData {
    let Some(b) = world.get::<Biogenerator>(e) else {
        return BiogeneratorData::default();
    };
    let holder = b
        .beaker
        .and_then(|c| world.get::<Reagents>(c))
        .map(|r| &r.0);
    BiogeneratorData {
        beaker: b.beaker.is_some(),
        beaker_current_volume: holder.map_or(0.0, |h| (h.total_volume * 100.0).round() / 100.0),
        beaker_max_volume: holder.map_or(0.0, |h| h.maximum_volume),
        biomass: b.biomass,
        processing: b.processing,
        max_output: MAX_OUTPUT,
        can_process: !foods(world, e).is_empty(),
    }
}

fn refresh_view(world: &mut World, e: Entity) {
    let view = compute_view(world, e);
    if let Some(mut b) = world.get_mut::<Biogenerator>(e)
        && b.view != view
    {
        b.view = view;
    }
}

fn refresh_static(world: &mut World, e: Entity) {
    let st = compute_static(world, e);
    let changed = world
        .get::<Biogenerator>(e)
        .is_some_and(|b| b.static_view != st);
    if !changed {
        return;
    }
    if let Some(mut b) = world.get_mut::<Biogenerator>(e) {
        b.static_view = st;
    }
    let mut q = world.query_filtered::<&mut UiWindow, With<UiKind<BiogeneratorWindow>>>();
    for mut w in q.iter_mut(world) {
        if w.source == e {
            w.refresh_static();
        }
    }
}

/// `update_overlays`: panel, container, biomass level, processing, screen.
fn refresh_look(world: &mut World, e: Entity) {
    let Some(b) = world.get::<Biogenerator>(e).cloned() else {
        return;
    };
    let powered = operational(world, e);
    let mut states: Vec<String> = Vec::new();
    if panel_open(world, e) {
        states.push("biogenerator_o_panel".into());
    }
    if b.beaker.is_some() {
        states.push("biogenerator_o_container".into());
    }
    if b.biomass > 0.0 {
        states.push(format!(
            "biogenerator_o_biomass_{}",
            biomass_level(b.biomass)
        ));
    }
    if powered {
        if b.processing {
            states.push("biogenerator_o_process".into());
        }
        states.push("biogenerator_o_screen".into());
    }
    let overlays: Vec<ss13_shared::Overlay> = {
        let Some(index) = world.get_resource::<IconIndex>() else {
            return;
        };
        states
            .iter()
            .filter_map(|s| index.get(BIOGEN_DMI, s))
            .map(|icon_state| ss13_shared::Overlay {
                icon_state,
                dir: None,
                color: None,
                pixel_offset: [0, 0],
                plane: None,
                only_dirs: 0,
            })
            .collect()
    };
    if let Some(mut appearance) = world.get_mut::<Appearance>(e)
        && appearance.overlays != overlays
    {
        appearance.overlays = overlays;
    }
}

/// `ROUND_UP(sin(min(biomass / max_visual_biomass, 1) * 90) * 7)`.
pub fn biomass_level(biomass: f32) -> u32 {
    let fill = (biomass / MAX_VISUAL_BIOMASS).min(1.0);
    ((fill * std::f32::consts::FRAC_PI_2).sin() * 7.0).ceil() as u32
}

/// `create_product` of `amount` (clamped as `ui_act` does). True when it was made.
pub fn create(world: &mut World, e: Entity, design: &str, amount: f32) -> bool {
    let Some(def) = world.get_resource::<ResearchDb>().and_then(|db| {
        designs(db)
            .find(|(_, d)| d.id == design)
            .map(|(_, d)| d.clone())
    }) else {
        return false;
    };
    let Some(unit_cost) = biomass_cost(&def) else {
        return false;
    };
    if !amount.is_finite() || amount < 1.0 {
        return false;
    }
    let Some(b) = world.get::<Biogenerator>(e).cloned() else {
        return false;
    };
    let free = b
        .beaker
        .and_then(|c| world.get::<Reagents>(c))
        .map(|r| r.0.maximum_volume - r.0.total_volume);
    let cap = match (&def.make_reagent, free) {
        (Some(_), Some(free)) => free.max(1.0),
        _ => MAX_OUTPUT as f32,
    };
    let amount = amount.min(cap).floor();
    let cost = unit_cost * amount / stats(world, e).efficiency;
    if let Some(reagent) = &def.make_reagent {
        let Some(free) = free else {
            return false;
        };
        if free < amount {
            crate::atmos_machinery::machine_say(
                world,
                e,
                "Внимание: в подключённом контейнере недостаточно свободного места!",
            );
            return false;
        }
        if cost > b.biomass {
            return false;
        }
        let (Some(chem), Some(beaker)) =
            (world.get_resource::<Chem>().map(|c| c.0.clone()), b.beaker)
        else {
            return false;
        };
        let Some(id) = chem.id(reagent) else {
            return false;
        };
        pay(world, e, cost);
        if let Some(mut r) = world.get_mut::<Reagents>(beaker) {
            r.0.add_reagent(
                &chem,
                id,
                amount,
                300.0,
                Some(REAGENT_PURITY),
                None,
                false,
                false,
            );
        }
        crate::chem::refresh_container(world, &chem, beaker);
        return true;
    }
    let (Some(build), Some(&pos)) = (def.build_path.clone(), world.get::<TilePos>(e)) else {
        return false;
    };
    if cost > b.biomass {
        return false;
    }
    pay(world, e, cost);
    if def.stack {
        crate::construction::spawn_stack(world, &build, pos, amount as u32);
    } else {
        for _ in 0..amount as u32 {
            crate::construction::spawn_obj(world, &build, pos);
        }
    }
    true
}

fn pay(world: &mut World, e: Entity, cost: f32) {
    if let Some(mut b) = world.get_mut::<Biogenerator>(e) {
        b.biomass -= cost;
    }
}

fn cap_windows(world: &mut World) {
    let windows: Vec<(Entity, Entity)> = world
        .query_filtered::<(Entity, &UiWindow), With<UiKind<BiogeneratorWindow>>>()
        .iter(world)
        .map(|(w, window)| (w, window.source))
        .collect();
    for (window, e) in windows {
        // `ui_status`: broken or open panel closes it.
        let broken = world
            .get::<crate::obj_damage::Integrity>(e)
            .is_some_and(|i| i.broken);
        if (broken || panel_open(world, e))
            && let Some(mut w) = world.get_mut::<UiWindow>(window)
        {
            w.cap_status(UiStatus::Close);
        }
    }
}

// Cost: per tick, one pass over the open windows; the data is a few numbers.
fn refresh_views(world: &mut World) {
    let mut q = world.query_filtered::<&UiWindow, With<UiKind<BiogeneratorWindow>>>();
    let mut gens: Vec<Entity> = q.iter(world).map(|w| w.source).collect();
    gens.sort();
    gens.dedup();
    for e in gens {
        refresh_view(world, e);
    }
}

/// The server side of the `Biogenerator` interface.
pub struct BiogeneratorWindow;

impl UiInterface for BiogeneratorWindow {
    const NAME: &'static str = BiogeneratorUi::NAME;
    type Data = BiogeneratorData;
    type StaticData = BiogeneratorStatic;
}

#[derive(Clone, Debug, PartialEq)]
pub enum BiogeneratorAct {
    Activate,
    Eject,
    /// `design_path`, `amount`.
    Create(String, f32),
}

impl ServerUi for BiogeneratorWindow {
    type Source = &'static Biogenerator;
    type UserParam = ();
    type Act = BiogeneratorAct;

    fn static_data(b: &Biogenerator) -> BiogeneratorStatic {
        b.static_view.clone()
    }

    fn data(b: &Biogenerator, _: Entity, _: &()) -> BiogeneratorData {
        b.view.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<BiogeneratorAct, UiReject> {
        Ok(match action {
            "activate" => BiogeneratorAct::Activate,
            "eject" => BiogeneratorAct::Eject,
            "create" => {
                let path = params
                    .get("design_path")
                    .filter(|v| v.len() <= 128)
                    .map(str::to_owned)
                    .ok_or_else(|| UiReject::BadParams("design_path".into()))?;
                let amount = params
                    .get("amount")
                    .and_then(|v| v.parse::<f32>().ok())
                    .filter(|n| n.is_finite())
                    .ok_or_else(|| UiReject::BadParams("amount".into()))?;
                BiogeneratorAct::Create(path, amount)
            }
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

fn acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<BiogeneratorWindow>>>) {
    let pending: Vec<(Entity, Entity, Entity, BiogeneratorAct)> = acts
        .read(world.resource::<Messages<UiActed<BiogeneratorWindow>>>())
        .map(|a| (a.source, a.client, a.actor, a.act.clone()))
        .collect();
    for (e, client, actor, act) in pending {
        if world.get::<Biogenerator>(e).is_none() {
            continue;
        }
        match act {
            BiogeneratorAct::Activate => start_process(world, e),
            BiogeneratorAct::Eject => eject_beaker(world, Some(client), actor, e),
            BiogeneratorAct::Create(design, amount) => {
                create(world, e, &design, amount);
            }
        }
        refresh_look(world, e);
        refresh_view(world, e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parts_set_the_rates() {
        let s = BiogenStats::from_tiers(1, 1);
        assert_eq!((s.max_items, s.per_cycle, s.efficiency), (20, 5, 1.0));
        let s = BiogenStats::from_tiers(4, 4);
        assert_eq!((s.max_items, s.per_cycle, s.efficiency), (50, 20, 4.0));
        assert_eq!(BiogenStats::from_tiers(0, 0).efficiency, 1.0);
    }

    #[test]
    fn biomass_levels() {
        assert_eq!(biomass_level(1.0), 1);
        assert_eq!(biomass_level(2500.0), 5);
        assert_eq!(biomass_level(5000.0), 7);
        assert_eq!(biomass_level(90_000.0), 7);
    }
}
