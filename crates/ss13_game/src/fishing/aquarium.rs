//! Aquariums, fish tanks and the fish-porter (server): tg `/obj/structure/aquarium`,
//! `/obj/item/fish_tank` and `/datum/component/aquarium` (`code/modules/fishing/aquarium/`,
//! `code/datums/components/aquarium.dm`) with the `Aquarium` window, and
//! `/obj/machinery/fishing_portal_generator` (`fishing_portal_machine.dm`).
// shortcut: the window opens on a hand click (tg: the panel opened by alt-click first, else the
// fish are admired); no props, feed storage or autofeeding, plunging, beauty, breaking or
// fish visuals inside; Assisted mode never falls back to stasis; the fish-porter has no
// research-unlocked dimensions, multitool links or catch-and-release.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::fishing::{
    DmFish, FISH_SOURCES, FishSourceDef, PORTAL_SOURCE, PREFILLED_FISH, SYNDICATE_PORTAL_SOURCE,
    fish_source,
};
use ss13_mapload::{DmAtom, DmItem, DmName, DmObj, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay};
use ss13_tgui::UiInterface as _;
use ss13_tgui::UiStatus;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::{
    AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
    distance_status,
};

use super::fish::{Fish, happiness};
use super::{
    AquariumData, AquariumFish, AquariumStatic, AquariumUi, CHANGE_MODE, FEEDING_INTERVAL, FLUID,
    ITEM_REFERENCE, NEW_MODE, REMOVE_ITEM, TEMPERATURE,
};
use crate::inventory::{CarriedBy, Inventory};
use crate::server::PlayerMob;
use crate::storage::{Stored, StoredIn};

const AQUARIUM_ROOT: &str = "/obj/structure/aquarium";
const PREFILLED: &str = "/obj/structure/aquarium/prefilled";
const TANK_ROOT: &str = "/obj/item/fish_tank";
const LAWYER_TANK: &str = "/obj/item/fish_tank/lawyer";
const PORTAL_ROOT: &str = "/obj/machinery/fishing_portal_generator";
const PORTAL_EMAGGED: &str = "/obj/machinery/fishing_portal_generator/emagged";
const PORTAL_FULL: &str = "/obj/machinery/fishing_portal_generator/full";
const FISH_FEED: &str = "/obj/item/reagent_containers/cup/fish_feed";
/// `T0C`, `MIN_AQUARIUM_TEMP`, `MAX_AQUARIUM_TEMP`, `DEFAULT_AQUARIUM_TEMP`.
const T0C: f32 = 273.15;
const DEFAULT_TEMP: f32 = T0C + 24.0;
/// `/datum/component/aquarium` `fluid_types`, in tg's order.
pub const FLUID_TYPES: [&str; 4] = ["Saltwater", "Freshwater", "Sulfuric Water", "Air"];
/// The fish tank's `maximum_relative_size` and `max_total_size`.
const TANK_MAX_FISH_SIZE: f32 = 100.0;
const TANK_MAX_TOTAL_SIZE: f32 = 220.0;
/// What a fed fish takes from the feed (`fish/feed`: 0.1 units).
const FEED_UNITS: f32 = 0.1;
const SPARKS: &str = "sparks";

/// `AQUARIUM_MODE_*`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AquariumMode {
    Manual,
    #[default]
    Assisted,
    Stasis,
}

impl AquariumMode {
    pub const ALL: [Self; 3] = [Self::Manual, Self::Assisted, Self::Stasis];

    pub fn name(self) -> &'static str {
        match self {
            Self::Manual => "Manual",
            Self::Assisted => "Assisted",
            Self::Stasis => "Stasis",
        }
    }

    fn of_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == name)
    }
}

/// `/datum/component/aquarium` on an aquarium or fish tank; its fish are [`Stored`] in it.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Aquarium {
    pub fluid: String,
    pub temperature: f32,
    pub mode: AquariumMode,
    pub min_temp: f32,
    pub max_temp: f32,
    /// Minutes.
    pub feeding_interval: f32,
    /// A fish tank (an item, with size limits).
    pub tank: bool,
}

impl Default for Aquarium {
    fn default() -> Self {
        Self {
            fluid: "Freshwater".to_owned(),
            temperature: DEFAULT_TEMP,
            mode: AquariumMode::Assisted,
            min_temp: T0C,
            max_temp: T0C + 100.0,
            feeding_interval: 3.0,
            tank: false,
        }
    }
}

/// A prefilled aquarium waiting for its fish.
#[derive(Component)]
struct Prefill;

/// The fish-porter: the dimension it is open to.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct FishPortal {
    pub active: Option<&'static FishSourceDef>,
    pub emagged: bool,
    /// `all_destinations` (the deluxe one).
    pub all: bool,
}

/// The fish-porter's question, on the prompt.
#[derive(Component, Clone, Debug)]
struct PendingPortal {
    portal: Entity,
    options: Vec<&'static FishSourceDef>,
}

/// `GLOB.fish_compatible_fluid_types`: whether a fish needing `required` lives in `fluid`.
pub fn fluid_compatible(required: &str, fluid: &str) -> bool {
    compatible_fluids(required).contains(&fluid)
}

fn compatible_fluids(required: &str) -> &'static [&'static str] {
    match required {
        "Any Fluid" => &["Saltwater", "Freshwater", "Sulfuric Water"],
        "Anadromous" => &["Saltwater", "Freshwater"],
        "Saltwater" => &["Saltwater"],
        "Freshwater" => &["Freshwater"],
        "Sulfuric Water" => &["Sulfuric Water"],
        "Air" => &["Air"],
        _ => &[],
    }
}

pub(super) fn plugin(app: &mut App) {
    app.register_ui::<AquariumUi>()
        .add_observer(add_obj_parts)
        .add_observer(add_item_parts)
        .add_systems(
            Update,
            (
                fill_prefilled,
                portal_power,
                window_status.in_set(UiSystems::StatusHook),
                aquarium_acts.in_set(UiSystems::Handle),
                portal_answers
                    .in_set(UiSystems::Handle)
                    .after(PromptSystems::Answer),
            ),
        );
}

fn add_obj_parts(add: On<Add<DmObj>>, mut commands: Commands) {
    let e = add.entity;
    commands.queue(move |world: &mut World| {
        let Some(path) = world.get::<DmObj>(e).map(|o| o.path.clone()) else {
            return;
        };
        if is_under(&path, AQUARIUM_ROOT) && world.get::<Aquarium>(e).is_none() {
            let mut aquarium = Aquarium::default();
            if path == PREFILLED {
                aquarium.mode = AquariumMode::Stasis;
                world.entity_mut(e).insert(Prefill);
            }
            world.entity_mut(e).insert(aquarium);
            refresh_look(world, e);
        } else if is_under(&path, PORTAL_ROOT) && world.get::<FishPortal>(e).is_none() {
            world.entity_mut(e).insert(FishPortal {
                active: None,
                emagged: path == PORTAL_EMAGGED,
                all: path == PORTAL_FULL,
            });
        }
    });
}

fn add_item_parts(add: On<Add<DmItem>>, mut commands: Commands) {
    let e = add.entity;
    commands.queue(move |world: &mut World| {
        let Some(path) = world.get::<DmItem>(e).map(|i| i.path.clone()) else {
            return;
        };
        if !is_under(&path, TANK_ROOT) || world.get::<Aquarium>(e).is_some() {
            return;
        }
        world.entity_mut(e).insert((
            Aquarium {
                mode: if path == LAWYER_TANK {
                    AquariumMode::Stasis
                } else {
                    AquariumMode::Assisted
                },
                min_temp: T0C + 12.0,
                max_temp: T0C + 68.0,
                tank: true,
                ..Aquarium::default()
            },
            AlwaysState,
        ));
        refresh_look(world, e);
    });
}

/// `/obj/structure/aquarium/prefilled/Initialize`: its three fish.
// shortcut: no sand and seaweed props, no starting feed (it is in stasis anyway).
fn fill_prefilled(world: &mut World) {
    let pending: Vec<Entity> = world
        .query_filtered::<Entity, With<Prefill>>()
        .iter(world)
        .collect();
    for aquarium in pending {
        world.entity_mut(aquarium).remove::<Prefill>();
        for path in PREFILLED_FISH {
            if let Some(fish) = crate::construction::spawn_detached(world, path) {
                crate::storage::put_inside(world, fish, aquarium);
            }
        }
    }
}

fn tell(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        crate::tools::tell(world, client, span, text);
    }
}

use crate::tools::now;

/// The live fish inside `aquarium`.
fn fish_in(world: &World, aquarium: Entity) -> Vec<Entity> {
    world
        .get::<Stored>(aquarium)
        .map(|s| s.items().to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter(|&e| world.get::<Fish>(e).is_some())
        .collect()
}

/// `update_icon` + `update_overlays`: the base sprite, the water or air glass, the borders.
fn refresh_look(world: &mut World, e: Entity) {
    let (Some(atom), Some(aquarium)) = (
        world.get::<DmAtom>(e).cloned(),
        world.get::<Aquarium>(e).cloned(),
    ) else {
        return;
    };
    let base = atom
        .icon_state
        .strip_suffix("_map")
        .unwrap_or(&atom.icon_state)
        .to_owned();
    let medium = if aquarium.fluid == "Air" {
        "air"
    } else {
        "water"
    };
    let (state, overlays) = if aquarium.tank {
        (
            base.clone(),
            vec![
                format!("{base}_panel"),
                format!("{base}_{medium}"),
                format!("{base}_borders"),
            ],
        )
    } else {
        (
            format!("{base}_base"),
            vec![format!("{base}_glass_{medium}"), format!("{base}_borders")],
        )
    };
    let Some(index) = world.get_resource::<IconIndex>() else {
        return;
    };
    let state = index.get(&atom.icon, &state);
    let overlays: Vec<Overlay> = overlays
        .iter()
        .filter_map(|s| index.get(&atom.icon, s))
        .map(Overlay::new)
        .collect();
    if let Some(mut look) = world.get_mut::<Appearance>(e) {
        if let Some(state) = state
            && look.icon_state != state
        {
            look.icon_state = state;
        }
        if look.overlays != overlays {
            look.overlays = overlays;
        }
    }
}

/// `get_optimal_aquarium_settings`: the fluid and middle temperature most live fish share.
// shortcut: types are taken greedily by population (tg pairs every type with its compatible
// ones first); no stasis fallback.
fn optimal_settings(world: &mut World, aquarium: Entity) {
    let mut by_type: Vec<(String, u32, DmFish)> = Vec::new();
    for fish in fish_in(world, aquarium) {
        if !world.get::<Fish>(fish).is_some_and(|f| f.alive) {
            continue;
        }
        let (Some(path), Some(def)) = (
            world.get::<DmItem>(fish).map(|i| i.path.clone()),
            world.get::<DmFish>(fish).cloned(),
        ) else {
            continue;
        };
        match by_type.iter_mut().find(|(p, ..)| *p == path) {
            Some(entry) => entry.1 += 1,
            None => by_type.push((path, 1, def)),
        }
    }
    if by_type.is_empty() {
        return;
    }
    by_type.sort_by_key(|b| std::cmp::Reverse(b.1));
    let mut fluids: Vec<&str> = FLUID_TYPES.to_vec();
    let (mut min, mut max) = (T0C, T0C + 100.0);
    for (_, _, def) in &by_type {
        let shared: Vec<&str> = fluids
            .iter()
            .copied()
            .filter(|f| fluid_compatible(&def.required_fluid_type, f))
            .collect();
        let (lo, hi) = (
            min.max(def.required_temperature_min),
            max.min(def.required_temperature_max),
        );
        if shared.is_empty() || lo > hi {
            continue;
        }
        fluids = shared;
        (min, max) = (lo, hi);
    }
    let Some(mut a) = world.get_mut::<Aquarium>(aquarium) else {
        return;
    };
    a.temperature = (min + ((max - min) / 2.0).round()).clamp(a.min_temp, a.max_temp);
    a.fluid = fluids.first().copied().unwrap_or("Freshwater").to_owned();
    refresh_look(world, aquarium);
}

/// Something held used on an aquarium or a fish tank: fish feed feeds, a fish goes in; an
/// emag on the fish-porter loads the Syndicate setting.
pub fn item_interaction(
    world: &mut World,
    _client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<FishPortal>(target).is_some() && crate::emag::is_emag(world, held) {
        return emag_portal(world, mob, target);
    }
    let Some(aquarium) = world.get::<Aquarium>(target).cloned() else {
        return false;
    };
    let path = world
        .get::<DmItem>(held)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    if is_under(&path, FISH_FEED) {
        feed(world, mob, held, target);
        return true;
    }
    let Some(fish) = world.get::<Fish>(held).cloned() else {
        return false;
    };
    // `/obj/item/fish_tank/can_insert`.
    if aquarium.tank {
        let total: f32 = fish_in(world, target)
            .into_iter()
            .filter_map(|f| world.get::<Fish>(f).map(|f| f.size))
            .sum();
        if fish.size > TANK_MAX_FISH_SIZE || total + fish.size > TANK_MAX_TOTAL_SIZE {
            tell(world, mob, Span::Warning, "it won't fit!");
            return true;
        }
    }
    crate::storage::put_inside(world, held, target);
    tell(world, mob, Span::Notice, "added to aquarium");
    if aquarium.mode == AquariumMode::Assisted {
        optimal_settings(world, target);
    }
    true
}

/// `feed_fishes` from a can of fish feed.
fn feed(world: &mut World, mob: Entity, can: Entity, aquarium: Entity) {
    let name = crate::chem::name_of(world, can);
    let empty = world
        .get::<crate::chem::Reagents>(can)
        .is_none_or(|r| r.0.total_volume <= 0.0);
    if empty {
        tell(world, mob, Span::Warning, format!("{name} is empty!"));
        return;
    }
    let fishes: Vec<Entity> = fish_in(world, aquarium)
        .into_iter()
        .filter(|&f| world.get::<Fish>(f).is_some_and(|f| f.alive))
        .collect();
    if fishes.is_empty() {
        tell(world, mob, Span::Warning, "no fish to feed!");
        return;
    }
    let now = now(world);
    for fish in fishes {
        let fed = world
            .get_mut::<crate::chem::Reagents>(can)
            .map_or(0.0, |mut r| r.0.remove_all(FEED_UNITS));
        if fed <= 0.0 {
            break;
        }
        if let Some(mut f) = world.get_mut::<Fish>(fish) {
            f.last_fed = now;
        }
    }
    tell(world, mob, Span::Notice, "fed the fish");
}

fn open_window(world: &mut World, client: Entity, mob: Entity, aquarium: Entity) {
    let title = world
        .get::<DmName>(aquarium)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<AquariumUi>(client, mob, aquarium, title));
}

/// A fish tank used in hand opens its window.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world.get::<Aquarium>(item).is_none() {
        return false;
    }
    open_window(world, client, mob, item);
    true
}

/// An empty hand on an aquarium (its window) or the fish-porter (`interact`).
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<Aquarium>(target).is_some() {
        open_window(world, client, mob, target);
        return true;
    }
    if world.get::<FishPortal>(target).is_some() {
        portal_interact(world, client, mob, target);
        return true;
    }
    false
}

/// The name a dimension is listed under (`radial_name`).
fn dimension_name(source: &FishSourceDef) -> String {
    match source.path {
        PORTAL_SOURCE => "Aquarium".to_owned(),
        SYNDICATE_PORTAL_SOURCE => "Syndicate".to_owned(),
        path => path.rsplit('/').next().unwrap_or(path).replace('_', " "),
    }
}

/// `update_overlays`: `portal_on` and the dimension's `overlay_state`.
fn portal_look(world: &mut World, portal: Entity) {
    let Some(state) = world.get::<FishPortal>(portal).copied() else {
        return;
    };
    let Some(icon) = world.get::<DmAtom>(portal).map(|a| a.icon.clone()) else {
        return;
    };
    let mut names = Vec::new();
    if let Some(source) = state.active {
        names.push("portal_on");
        names.push(match source.path {
            SYNDICATE_PORTAL_SOURCE => "portal_syndicate",
            "/datum/fish_source/river" => "portal_river",
            "/datum/fish_source/ocean" | "/datum/fish_source/ice_fishing" => "portal_ocean",
            "/datum/fish_source/ocean/beach" | "/datum/fish_source/tizira" => "portal_beach",
            _ => "portal_aquarium",
        });
    }
    let overlays: Vec<Overlay> = world
        .get_resource::<IconIndex>()
        .map(|i| {
            names
                .iter()
                .filter_map(|n| i.get(&icon, n))
                .map(Overlay::new)
                .collect()
        })
        .unwrap_or_default();
    if let Some(mut look) = world.get_mut::<Appearance>(portal)
        && look.overlays != overlays
    {
        look.overlays = overlays;
    }
}

fn powered(world: &World, machine: Entity) -> bool {
    world.get::<crate::power::Unpowered>(machine).is_none()
}

/// `interact`: an open portal closes, a closed one opens to the only dimension or asks.
fn portal_interact(world: &mut World, client: Entity, mob: Entity, portal: Entity) {
    let Some(state) = world.get::<FishPortal>(portal).copied() else {
        return;
    };
    if state.active.is_some() {
        set_portal(world, portal, None);
        return;
    }
    if !powered(world, portal) {
        tell(world, mob, Span::Warning, "no power!");
        return;
    }
    let options: Vec<&'static FishSourceDef> = if state.all {
        FISH_SOURCES.iter().collect()
    } else {
        [PORTAL_SOURCE]
            .into_iter()
            .chain(state.emagged.then_some(SYNDICATE_PORTAL_SOURCE))
            .filter_map(fish_source)
            .collect()
    };
    if options.len() == 1 {
        set_portal(world, portal, options.first().copied());
        return;
    }
    let items = options.iter().map(|s| dimension_name(s)).collect();
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Fish-porter",
            "Choose a dimension",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(PendingPortal { portal, options });
}

fn set_portal(world: &mut World, portal: Entity, source: Option<&'static FishSourceDef>) {
    if let Some(mut p) = world.get_mut::<FishPortal>(portal) {
        p.active = source;
    }
    portal_look(world, portal);
}

/// The chosen dimension, if the porter is still closed, powered and in reach.
fn portal_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, actor, answer) in answers {
        let Some(pending) = world.get::<PendingPortal>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let ready = world
            .get::<FishPortal>(pending.portal)
            .is_some_and(|p| p.active.is_none())
            && powered(world, pending.portal)
            && world.get::<crate::health::Stat>(actor) == Some(&crate::health::Stat::Conscious)
            && crate::storage::in_reach(world, actor, pending.portal);
        if ready && let Some(&source) = pending.options.get(i) {
            set_portal(world, pending.portal, Some(source));
        }
    }
}

/// `on_set_is_operational`: power lost closes the portal.
fn portal_power(world: &mut World) {
    let lost: Vec<Entity> = world
        .query_filtered::<(Entity, &FishPortal), With<crate::power::Unpowered>>()
        .iter(world)
        .filter(|(_, p)| p.active.is_some())
        .map(|(e, _)| e)
        .collect();
    for portal in lost {
        set_portal(world, portal, None);
    }
}

/// `emag_act`.
fn emag_portal(world: &mut World, mob: Entity, portal: Entity) -> bool {
    let Some(mut p) = world.get_mut::<FishPortal>(portal) else {
        return false;
    };
    if p.emagged {
        return false;
    }
    p.emagged = true;
    tell(world, mob, Span::Notice, "syndicate setting loaded");
    if let Some(at) = crate::chem::map_pos(world, portal) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(at, SPARKS, 25.0, true));
    }
    true
}

impl ServerUi for AquariumUi {
    type Source = (Entity, &'static Aquarium, Option<&'static Stored>);
    type UserParam = (
        Query<'static, 'static, (&'static Fish, &'static DmFish, Option<&'static DmName>)>,
        Res<'static, Time>,
    );
    type Act = AquariumAct;

    fn static_data((_, aquarium, _): (Entity, &Aquarium, Option<&Stored>)) -> AquariumStatic {
        AquariumStatic {
            min_temperature: aquarium.min_temp,
            max_temperature: aquarium.max_temp,
            fluid_types: FLUID_TYPES.iter().map(|f| (*f).to_owned()).collect(),
            aquarium_modes: AquariumMode::ALL
                .iter()
                .map(|m| m.name().to_owned())
                .collect(),
        }
    }

    fn data(
        (_, aquarium, stored): (Entity, &Aquarium, Option<&Stored>),
        _: Entity,
        (fish, time): &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> AquariumData {
        let now = time.elapsed_secs_f64();
        let fish_data = stored
            .map(|s| s.items())
            .unwrap_or_default()
            .iter()
            .filter_map(|&e| {
                let (f, def, name) = fish.get(e).ok()?;
                Some(AquariumFish {
                    fish_ref: e.to_bits().to_string(),
                    fish_name: name.map(|n| n.name.to_uppercase()).unwrap_or_default(),
                    fish_happiness: happiness(f, def, Some(aquarium), now),
                    fish_alive: f.alive,
                })
            })
            .collect();
        AquariumData {
            fluid_type: aquarium.fluid.clone(),
            temperature: aquarium.temperature,
            locked_fluid_temp: aquarium.mode == AquariumMode::Assisted,
            current_mode: aquarium.mode.name().to_owned(),
            fish_data,
            feeding_interval: aquarium.feeding_interval,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<AquariumAct, UiReject> {
        match action {
            TEMPERATURE => Ok(AquariumAct::Temperature(params.number(TEMPERATURE)? as f32)),
            FLUID => Ok(AquariumAct::Fluid(params.text(FLUID)?)),
            CHANGE_MODE => Ok(AquariumAct::Mode(params.text(NEW_MODE)?)),
            FEEDING_INTERVAL => Ok(AquariumAct::FeedingInterval(
                params.number(FEEDING_INTERVAL)? as f32,
            )),
            REMOVE_ITEM => params
                .get(ITEM_REFERENCE)
                .and_then(|r| r.parse::<u64>().ok())
                .and_then(Entity::try_from_bits)
                .map(AquariumAct::Remove)
                .ok_or_else(|| UiReject::BadParams(format!("{ITEM_REFERENCE}: not a ref"))),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AquariumAct {
    Temperature(f32),
    Fluid(String),
    Mode(String),
    FeedingInterval(f32),
    Remove(Entity),
}

/// `ui_act`, checked at the moment of effect: fluid and temperature are locked in Assisted
/// mode, values must be in range, the fish must be inside this aquarium.
fn aquarium_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<AquariumUi>>>) {
    let pending: Vec<(Entity, Entity, AquariumAct)> = acts
        .read(world.resource::<Messages<UiActed<AquariumUi>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, source, act) in pending {
        let Some(aquarium) = world.get::<Aquarium>(source).cloned() else {
            continue;
        };
        let locked = aquarium.mode == AquariumMode::Assisted;
        match act {
            AquariumAct::Temperature(t) if !locked && t.is_finite() => {
                let mut a = world.get_mut::<Aquarium>(source).expect("checked");
                a.temperature = t.clamp(a.min_temp, a.max_temp);
            }
            AquariumAct::Fluid(fluid) if !locked && FLUID_TYPES.contains(&fluid.as_str()) => {
                world.get_mut::<Aquarium>(source).expect("checked").fluid = fluid;
                refresh_look(world, source);
            }
            AquariumAct::Mode(name) => {
                let Some(mode) = AquariumMode::of_name(&name) else {
                    continue;
                };
                world.get_mut::<Aquarium>(source).expect("checked").mode = mode;
                if mode == AquariumMode::Assisted {
                    optimal_settings(world, source);
                }
            }
            AquariumAct::FeedingInterval(minutes) if minutes.is_finite() => {
                world
                    .get_mut::<Aquarium>(source)
                    .expect("checked")
                    .feeding_interval = minutes.clamp(1.0, 60.0);
            }
            AquariumAct::Remove(item) => {
                if world.get::<StoredIn>(item).is_none_or(|s| s.0 != source) {
                    continue;
                }
                let Some(at) = crate::chem::map_pos(world, source) else {
                    continue;
                };
                crate::storage::take_out(world, item, at);
                let (what, from) = (
                    crate::chem::name_of(world, item),
                    crate::chem::name_of(world, source),
                );
                tell(
                    world,
                    actor,
                    Span::Notice,
                    format!("You take out {what} from {from}."),
                );
                if aquarium.mode == AquariumMode::Assisted {
                    optimal_settings(world, source);
                }
            }
            _ => {}
        }
    }
}

/// A fish tank's window (`AlwaysState`): interactive while held, else by distance.
fn window_status(
    mut windows: Query<&mut UiWindow>,
    tanks: Query<(&Aquarium, Option<&CarriedBy>, Option<&ss13_shared::TilePos>)>,
    users: Query<(&Inventory, Option<&ss13_shared::TilePos>)>,
) {
    for mut window in &mut windows {
        if window.interface != AquariumUi::NAME {
            continue;
        }
        let Ok((aquarium, carried, pos)) = tanks.get(window.source) else {
            continue;
        };
        if !aquarium.tank {
            continue;
        }
        let status = match (users.get(window.actor), carried, pos) {
            (Ok(_), Some(c), _) if c.0 == window.actor => UiStatus::Interactive,
            (Ok((_, Some(at))), _, Some(pos)) => distance_status(*at, *pos),
            _ => UiStatus::Close,
        };
        window.cap_status(status);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fluids_follow_the_compatibility_table() {
        assert!(fluid_compatible("Anadromous", "Saltwater"));
        assert!(fluid_compatible("Any Fluid", "Sulfuric Water"));
        assert!(!fluid_compatible("Any Fluid", "Air"));
        assert!(!fluid_compatible("Freshwater", "Saltwater"));
        assert_eq!(AquariumMode::of_name("Stasis"), Some(AquariumMode::Stasis));
        assert_eq!(
            dimension_name(fish_source(SYNDICATE_PORTAL_SOURCE).unwrap()),
            "Syndicate"
        );
    }
}
