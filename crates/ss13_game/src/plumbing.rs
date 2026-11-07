//! Sinks, showers, toilets and urinals (server), after tg
//! `code/game/objects/structures/water_structures/sink.dm`, `toilet.dm`, `urinal.dm` and
//! `code/game/objects/structures/shower.dm`.
//!
//! A sink holds `capacity` units of its `dispensedreagent` (water; a gas station's plasma) and,
//! with a water recycler, refills `reclaim_rate` units a second.
//! - An empty hand washes the hands (or the face, aiming at the head) in 4 s for 5 units: worn
//!   gloves or the hands lose their blood and prints (`wash_hands`, `CLEAN_WASH`).
//! - A reagent container fills from it (left click, `crate::chem`'s drain: the sink is a
//!   drainable holder) or empties down the drain (right click); a mop gets 5 units.
//! - Any other item (out of combat mode) is washed in 4 s for 5 units.
//! - A wrench takes it apart (its stack and the recycler), a crowbar pries the recycler out.
//! - "Someone's already washing here": one wash at a time per sink.
//!
//! A shower ([`Shower`], 200 units of water, +0.5 a second with its recycler while off) is turned
//! on and off by hand. Every second (`SSplumbing`) a running shower washes its tile and everything
//! on it (`CLEAN_WASH` when 70 % of it is water, `CLEAN_RAD` unless 20 % is radium, uranium or
//! triple polonium: the radiation wash of `crate::radiation::clean_act`), splashes its reagents on
//! them (`SHOWER_EXPOSURE_MULTIPLIER × 5` units' worth), freezes or scalds the living at a cold or
//! boiling setting, gives carbons the shower mood and the washing stamina regen, and uses 5 units.
//! It stops when dry (the auto shut-off: until empty, after 15 s, or forever, resuming when
//! refilled) and leaves the floor wet. A multitool cycles the shut-off mode, a screwdriver (5 s)
//! the temperature, an analyzer reads it, a crowbar pries the recycler out, a recycler goes back
//! in, a wrench takes it apart (2 iron and the recycler).
//!
//! A toilet ([`Toilet`]): alt-click lifts or drops the lid, a right click flushes (50 units, 4 s;
//! something stuck in the bowl spills the cistern on the floor instead), a crowbar (3 s) lifts the
//! cistern's lid; an open cistern hides up to a huge weight of normal items (an empty hand takes a
//! random one with the seat down), takes a recycler back, a screwdriver takes it out; small items
//! dropped in the open bowl get stuck, containers fill from it; a wrench takes it apart. An empty
//! hand aggressively grabbing someone standing on it slams them into it for 5 brute. A urinal
//! ([`Urinal`]) hides a urinal cake behind a cap a screwdriver (2 s) opens; a tiny item goes in,
//! an empty hand takes it; a wrench (right click) takes it off the wall as its frame; an
//! aggressive grab slams for 8 brute.
// shortcut: sinks: no washing the face clean of makeup (no lipstick), no shocking yourself with
// an active baton, no reagents' touch effects on the washer (`expose` is not ported for mobs), no
// pickle jars, no installing a recycler back, no sink frames, no plumbing ducts; water sources
// are not ported. Showers: no mist, no looping sound, no plunger, no shower frames; the washing
// regen is applied per shower tick (no status effect alert), the water hater/adaptation and
// morbid traits are not read. Toilets: no swirlies (internals and oxygen loss), no fish, no foam
// (an overflow splashes the cistern on the floor), no monkey cubes, no flush animation. Urinals:
// no screams, frames are not mounted back (no wallframes).
// Cost: one pass over the sinks below capacity per tick, one over showers and toilets a second
// (a few dozen per map).

use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmItem, DmObj, DmWeight};
use ss13_reagents::{Holder, Owner, flags};
use ss13_shared::chat::Span;
use ss13_shared::protocol::{ClickModifiers, InventorySlot};
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_world::SpatialIndex;

use crate::chem::{Chem, Reagents};
use crate::construction::Job;
use crate::health::ZoneSelected;
use crate::inventory::Inventory;
use crate::medical::{name_of, visible};
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, DoAfter, ToolKind, do_after, tell, tool_of};
use crate::zone::BodyZone;

const SINK: &str = "/obj/structure/sink";
const SHOWER: &str = "/obj/machinery/shower";
const TOILET: &str = "/obj/structure/toilet";
const TOILET_GREYSCALE: &str = "/obj/structure/toilet/greyscale";
const URINAL: &str = "/obj/structure/urinal";
const URINAL_CAKE: &str = "/obj/item/food/urinalcake";
const URINAL_FRAME: &str = "/obj/item/wallframe/urinal";
const ANALYZER: &str = "/obj/item/analyzer";
const IRON: &str = "/obj/item/stack/sheet/iron";
const WATER: &str = "/datum/reagent/water";
const BLOOD: &str = "/datum/reagent/blood";
const RADIUM: &str = "/datum/reagent/uranium/radium";
const URANIUM: &str = "/datum/reagent/uranium";
const POLONIUM: &str = "/datum/reagent/toxin/polonium";
const RATCHET: &str = "sound/items/tools/ratchet.ogg";
const STONE_DOOR: &str = "sound/effects/stonedoor_openclose.ogg";
const FLUSH: &str = "sound/machines/toilet_flush.ogg";
const BANG: &str = "sound/effects/bang.ogg";
/// `SHOWER_SPRAY_VOLUME`: what a shower uses a second, and the least it turns on with.
const SHOWER_SPRAY_VOLUME: f32 = 5.0;
/// `SHOWER_EXPOSURE_MULTIPLIER`.
const SHOWER_EXPOSURE_MULTIPLIER: f32 = 2.0;
/// `SHOWER_TIMED_LENGTH`, seconds.
const SHOWER_TIMED_SECS: f64 = 15.0;
/// `/datum/controller/subsystem/processing/plumbing::wait`, seconds.
const PLUMBING_WAIT: f32 = 1.0;
/// `screwdriver_act`: `use_tool(src, user, 50)`, deciseconds.
const SHOWER_TEMP_DS: f32 = 50.0;
/// `toilet/crowbar_act`: `use_tool(src, user, 30)`.
const CISTERN_DS: f32 = 30.0;
/// `urinal/screwdriver_act`: `use_tool(src, user, 20)`.
const URINAL_CAP_DS: f32 = 20.0;
/// `toilet/attack_hand_secondary`: a flush takes 50 units and 4 s.
const FLUSH_UNITS: f32 = 50.0;
const FLUSH_SECS: f64 = 4.0;
/// `WEIGHT_CLASS_TINY`, `_SMALL`, `_NORMAL`, `_HUGE`.
const W_TINY: u8 = 1;
const W_SMALL: u8 = 2;
const W_NORMAL: u8 = 3;
const W_HUGE: u8 = 5;
/// `/obj/machinery/shower` and `/obj/structure/toilet` defaults (`reagent_capacity`,
/// `refill_rate` / `reclaim_rate`), for prototypes converted before these fields.
const BATH_CAPACITY: f32 = 200.0;
const BATH_REFILL: f32 = 0.5;
const MOP: &str = "/obj/item/mop";
const WATER_RECYCLER: &str = "/obj/item/stock_parts/water_recycler";
const FAUCET: &str = "sound/machines/sink-faucet.ogg";
const SLOSH: &str = "sound/effects/slosh.ogg";
const CLICK: &str = "sound/machines/click.ogg";
/// `do_after(user, 4 SECONDS, target = src)`, deciseconds.
const WASH_DS: f32 = 40.0;
/// What one wash uses.
const WASH_UNITS: f32 = 5.0;

/// `/obj/structure/sink`: `has_water_reclaimer`, `reclaim_rate`, `dispensedreagent`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Sink {
    pub reclaimer: bool,
    pub reclaim_rate: f32,
    pub reagent: String,
}

/// The `do_after`s of a sink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SinkJob {
    /// The washer's hands, or its face.
    Hands { face: bool },
    /// The item in the washer's hand.
    Item,
    /// `shower/screwdriver_act`: the temperature valve.
    ShowerTemperature,
    /// `toilet/crowbar_act`: the cistern's lid.
    Cistern,
    /// `urinal/screwdriver_act`: the drain protector's cap.
    UrinalCap,
}

pub struct PlumbingPlugin;

impl Plugin for PlumbingPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `attack_hand(_secondary)` of a sink (washing the hands or face), a shower, a toilet,
        // a urinal.
        app.add_click_handler(ClickPhase::EmptyHand, 3100, |w, c| {
            crate::plumbing::interact(w, c.client, c.mob, c.target, c.modifiers)
        });
        // tg `sink/item_interaction(_secondary)` (washing, emptying, a mop wetted), and the tool
        // acts and `item_interaction` of showers, toilets and urinals.
        app.add_click_handler(ClickPhase::HeldOnTarget, 3400, |w, c| {
            c.held.is_some_and(|h| {
                crate::plumbing::item_interaction(w, c.client, c.mob, h, c.target, c.modifiers)
            })
        });
        app.add_observer(add_sink)
            .add_observer(add_bath)
            .add_systems(
                Update,
                (reclaim_water, run_showers, run_toilets).after(crate::click::ResolveClicks),
            );
    }
}

fn is_sink(world: &World, e: Entity) -> bool {
    world.get::<Sink>(e).is_some()
}

/// `SHOWER_FREEZING`, `SHOWER_NORMAL`, `SHOWER_BOILING` (`current_temperature`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShowerTemp {
    Freezing,
    #[default]
    Normal,
    Boiling,
}

impl ShowerTemp {
    /// `screwdriver_act`: normal -> freezing -> boiling -> normal.
    fn next(self) -> Self {
        match self {
            Self::Normal => Self::Freezing,
            Self::Freezing => Self::Boiling,
            Self::Boiling => Self::Normal,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Self::Freezing => "ледяная",
            Self::Normal => "нормальная",
            Self::Boiling => "кипящая",
        }
    }
}

/// `SHOWER_MODE_UNTIL_EMPTY`, `SHOWER_MODE_TIMED`, `SHOWER_MODE_FOREVER` (`mode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShowerMode {
    #[default]
    UntilEmpty,
    Timed,
    Forever,
}

impl ShowerMode {
    /// `mode = (mode + 1) % SHOWER_MODE_COUNT`.
    fn next(self) -> Self {
        match self {
            Self::UntilEmpty => Self::Timed,
            Self::Timed => Self::Forever,
            Self::Forever => Self::UntilEmpty,
        }
    }

    /// `GLOB.shower_mode_descriptions`.
    fn description(self) -> &'static str {
        match self {
            Self::UntilEmpty => "работать до опустошения",
            Self::Timed => "работать 15 секунд или до опустошения",
            Self::Forever => "работать всегда и включаться снова",
        }
    }
}

/// `/obj/machinery/shower`: `intended_on`, `actually_on`, `current_temperature`, `mode`,
/// `has_water_reclaimer`, `refill_rate`, `reagent_id`, `timed_cooldown`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Shower {
    pub intended_on: bool,
    pub actually_on: bool,
    pub temperature: ShowerTemp,
    pub mode: ShowerMode,
    pub reclaimer: bool,
    pub refill_rate: f32,
    pub reagent: String,
    /// `timed_cooldown`: game seconds the timed mode shuts off at.
    pub timed_until: f64,
}

/// `/obj/structure/toilet`: `cover_open`, `cistern_open`, `has_water_reclaimer`,
/// `reclaim_rate`, `reagent_id`, `cistern_items`, `stuck_item`, `flushing`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Toilet {
    pub cover_open: bool,
    pub cistern_open: bool,
    pub reclaimer: bool,
    pub reclaim_rate: f32,
    pub reagent: String,
    pub cistern: Vec<Entity>,
    pub stuck: Option<Entity>,
    /// `flushing`: game seconds the flush ends at.
    pub flushing_until: Option<f64>,
}

/// `/obj/structure/urinal`: `exposed`, `hidden_item`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Urinal {
    pub exposed: bool,
    pub hidden: Option<Entity>,
}

use crate::tools::now;

/// The reagent store of a shower or toilet: `capacity` (or tg's 200) units of `reagent`, full
/// with a recycler (`create_reagents(reagent_capacity)`, `add_reagent` in `Initialize`).
fn bath_reagents(chem: Option<&Chem>, dm: &DmObj, reclaimer: bool) -> (Reagents, String, f32) {
    let capacity = if dm.capacity > 0.0 {
        dm.capacity
    } else {
        BATH_CAPACITY
    };
    let reagent = if dm.dispensed_reagent.is_empty() {
        WATER.to_owned()
    } else {
        dm.dispensed_reagent.clone()
    };
    let rate = if dm.reclaim_rate > 0.0 {
        dm.reclaim_rate
    } else {
        BATH_REFILL
    };
    let owner = Owner {
        path: dm.path.clone(),
        living: false,
    };
    let mut holder = Holder::new(capacity, 0, owner);
    if reclaimer
        && let Some(chem) = chem
        && let Some(id) = chem.0.id(&reagent)
    {
        holder.add_reagent(
            &chem.0,
            id,
            capacity,
            ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
            None,
            None,
            true,
            false,
        );
    }
    (Reagents(holder), reagent, rate)
}

/// `shower/Initialize`, `toilet/Initialize` (`cover_open = rand(0, 1)`), `urinal/Initialize`
/// (`hidden_item = new /obj/item/food/urinalcake(src)` on a map urinal).
fn add_bath(
    add: On<Add<DmObj>>,
    objs: Query<&DmObj>,
    chem: Option<Res<Chem>>,
    mut rng: Option<ResMut<crate::combat::CombatRng>>,
    mut commands: Commands,
) {
    let Ok(dm) = objs.get(add.entity) else {
        return;
    };
    let entity = add.entity;
    // Prototypes converted before the shower and toilet fields have no capacity: tg's
    // `has_water_reclaimer = TRUE` default.
    let reclaimer = dm.water_reclaimer || dm.capacity <= 0.0;
    if is_under(&dm.path, SHOWER) {
        let (reagents, reagent, refill_rate) = bath_reagents(chem.as_deref(), dm, reclaimer);
        commands.entity(entity).insert((
            Shower {
                intended_on: false,
                actually_on: false,
                temperature: ShowerTemp::Normal,
                mode: ShowerMode::UntilEmpty,
                reclaimer,
                refill_rate,
                reagent,
                timed_until: 0.0,
            },
            reagents,
        ));
    } else if is_under(&dm.path, TOILET) {
        // `/obj/structure/toilet/greyscale`: `has_water_reclaimer = FALSE`.
        let reclaimer = reclaimer && (dm.capacity > 0.0 || !is_under(&dm.path, TOILET_GREYSCALE));
        let (reagents, reagent, reclaim_rate) = bath_reagents(chem.as_deref(), dm, reclaimer);
        let cover_open = rng.as_mut().is_some_and(|r| r.range(0, 1) == 1);
        commands.entity(entity).insert((
            Toilet {
                cover_open,
                cistern_open: false,
                reclaimer,
                reclaim_rate,
                reagent,
                cistern: Vec::new(),
                stuck: None,
                flushing_until: None,
            },
            reagents,
        ));
        commands.queue(move |world: &mut World| refresh_toilet(world, entity));
    } else if is_under(&dm.path, URINAL) {
        commands.entity(entity).insert(Urinal::default());
        commands.queue(move |world: &mut World| {
            if world.get_entity(entity).is_err() {
                return;
            }
            let cake = world
                .get::<TilePos>(entity)
                .copied()
                .and_then(|pos| crate::construction::spawn_obj(world, URINAL_CAKE, pos));
            if let Some(cake) = cake {
                crate::storage::put_inside(world, cake, entity);
                if let Some(mut urinal) = world.get_mut::<Urinal>(entity) {
                    urinal.hidden = Some(cake);
                }
            }
        });
    }
}

/// `item` is still inside `holder` (tg `Exited` forgets what left some other way).
fn inside(world: &World, holder: Entity, item: Option<Entity>) -> Option<Entity> {
    item.filter(|&i| {
        world
            .get::<crate::storage::StoredIn>(i)
            .is_some_and(|s| s.0 == holder)
    })
}

fn w_class(world: &World, item: Entity) -> u8 {
    world
        .get::<DmWeight>(item)
        .copied()
        .unwrap_or_default()
        .w_class
}

fn combat_mode(world: &World, mob: Entity) -> bool {
    world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
}

fn icon_id(world: &World, e: Entity, state: &str) -> Option<ss13_shared::IconStateId> {
    let icon = &world.get::<DmAtom>(e)?.icon;
    world.get_resource::<IconIndex>()?.get(icon, state)
}

fn set_overlays(world: &mut World, e: Entity, overlays: Vec<Overlay>) {
    if let Some(mut appearance) = world.get_mut::<Appearance>(e)
        && appearance.overlays != overlays
    {
        appearance.overlays = overlays;
    }
}

/// `tg transferItemToLoc`: `held` out of the user's hand into `holder`; false (with tg's line)
/// when it is stuck to the hand.
fn hand_into(world: &mut World, client: Entity, held: Entity, holder: Entity) -> bool {
    if crate::inventory::no_drop(world, held) {
        let what = name_of(world, held);
        tell(
            world,
            client,
            Span::Warning,
            format!("{what} прилип к вашей руке!"),
        );
        return false;
    }
    crate::storage::put_inside(world, held, holder);
    true
}

fn give(world: &mut World, mob: Entity, item: Entity) {
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, item);
    }
}

/// Sums the reagents of `holder` of type `path` or its subtypes (`has_reagent`,
/// `get_reagent_amount` with subtypes).
fn amount_of(chem: &ss13_reagents::Chemistry, holder: &Holder, path: &str) -> f32 {
    holder
        .reagents
        .iter()
        .filter(|r| chem.is_type(r.id, path))
        .map(|r| r.volume)
        .sum()
}

// --- Showers ---------------------------------------------------------------------------------

/// `shower/update_overlays`: the falling water while it runs, drawn on the tile's centre
/// (`pixel_shift` back against the shower's own wall offset), tinted by its reagents.
fn refresh_shower(world: &mut World, shower: Entity) {
    let on = world.get::<Shower>(shower).is_some_and(|s| s.actually_on);
    let mut overlays = Vec::new();
    if on && let Some(id) = icon_id(world, shower, "water") {
        let offset = world
            .get::<Appearance>(shower)
            .map_or([0, 0], |a| a.pixel_offset);
        let color = world.get::<Reagents>(shower).and_then(|r| {
            let chem = world.get_resource::<Chem>()?;
            ss13_reagents::mix_color(&chem.0, &r.0.reagents)
        });
        overlays.push(Overlay {
            pixel_offset: [-offset[0], -offset[1]],
            color,
            ..Overlay::new(id)
        });
    }
    set_overlays(world, shower, overlays);
}

/// `update_actually_on(new_on_state)`: true when the state changed (not with less than
/// `SHOWER_SPRAY_VOLUME` left to turn on).
fn set_shower_on(world: &mut World, shower: Entity, on: bool) -> bool {
    let Some(state) = world.get::<Shower>(shower).cloned() else {
        return false;
    };
    if state.actually_on == on {
        return false;
    }
    if on && volume(world, shower) < SHOWER_SPRAY_VOLUME {
        return false;
    }
    let until = now(world) + SHOWER_TIMED_SECS;
    if let Some(mut s) = world.get_mut::<Shower>(shower) {
        s.actually_on = on;
        if on {
            s.timed_until = until;
        }
    }
    refresh_shower(world, shower);
    if !on && let Some(&pos) = world.get::<TilePos>(shower) {
        // `MakeSlippery(TURF_WET_WATER, min_wet_time = 5 SECONDS, wet_time_to_add = 1 SECONDS)`.
        crate::janitor::make_slippery(world, pos, crate::janitor::TURF_WET_WATER, 5.0, 1.0);
    }
    true
}

/// `shower/interact`: the user wants it on (or off).
fn shower_interact(world: &mut World, client: Entity, shower: Entity) {
    let Some(intended) = world.get::<Shower>(shower).map(|s| !s.intended_on) else {
        return;
    };
    if let Some(mut s) = world.get_mut::<Shower>(shower) {
        s.intended_on = intended;
    }
    let name = name_of(world, shower);
    if !set_shower_on(world, shower, intended) {
        tell(world, client, Span::Warning, format!("{name} пуст!"));
        return;
    }
    let what = if intended {
        "включён"
    } else {
        "выключен"
    };
    tell(world, client, Span::Notice, format!("{name}: {what}."));
}

/// The held-item half of a shower (tg `tool_act`s, then `item_interaction`). True when handled.
fn shower_item(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    shower: Entity,
    path: &str,
) -> bool {
    let combat = combat_mode(world, mob);
    let tool = tool_of(world, held)
        .map(|(kind, _)| kind)
        .filter(|_| !combat);
    let name = name_of(world, shower);
    match tool {
        Some(ToolKind::Wrench) => {
            sound(world, shower, RATCHET, 50.0);
            crate::obj_damage::deconstruct(world, shower, true);
            return true;
        }
        Some(ToolKind::Multitool) => {
            let Some(mode) = world.get::<Shower>(shower).map(|s| s.mode.next()) else {
                return true;
            };
            if let Some(mut s) = world.get_mut::<Shower>(shower) {
                s.mode = mode;
            }
            tell(
                world,
                client,
                Span::Notice,
                format!(
                    "Вы переключаете автоотключение душа: {}.",
                    mode.description()
                ),
            );
            return true;
        }
        Some(ToolKind::Crowbar) => {
            if !world.get::<Shower>(shower).is_some_and(|s| s.reclaimer) {
                tell(world, client, Span::Warning, "Здесь нет утилизатора воды.");
                return true;
            }
            if let Some(mut s) = world.get_mut::<Shower>(shower) {
                s.reclaimer = false;
            }
            sound(world, shower, CLICK, 20.0);
            if let Some(&pos) = world.get::<TilePos>(shower) {
                crate::construction::spawn_obj(world, WATER_RECYCLER, pos);
            }
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы снимаете утилизатор воды с {name}."),
            );
            return true;
        }
        Some(ToolKind::Screwdriver) => {
            tell(
                world,
                client,
                Span::Notice,
                "Вы начинаете регулировать клапан температуры...",
            );
            let actor = Actor {
                client,
                mob,
                item: Some(held),
            };
            crate::tools::use_tool(
                world,
                actor,
                shower,
                SHOWER_TEMP_DS,
                0,
                Job::Sink(SinkJob::ShowerTemperature),
            );
            return true;
        }
        _ => {}
    }
    if is_under(path, ANALYZER) && !combat {
        let temp = world
            .get::<Shower>(shower)
            .map_or(ShowerTemp::Normal, |s| s.temperature);
        tell(
            world,
            client,
            Span::Notice,
            format!("Температура воды: {}.", temp.word()),
        );
        return true;
    }
    if is_under(path, WATER_RECYCLER) {
        if world.get::<Shower>(shower).is_some_and(|s| s.reclaimer) {
            tell(
                world,
                client,
                Span::Warning,
                "Утилизатор воды уже установлен.",
            );
            return true;
        }
        sound(world, shower, CLICK, 20.0);
        world.despawn(held);
        if let Some(mut s) = world.get_mut::<Shower>(shower) {
            s.reclaimer = true;
        }
        return true;
    }
    false
}

/// `shower/check_heat`.
fn check_heat(world: &mut World, mob: Entity, temp: ShowerTemp, name: &str) {
    match temp {
        ShowerTemp::Freezing => {
            // `adjust_bodytemperature(-80, 80)`.
            crate::medical::diseases::adjust_bodytemperature(world, mob, -80.0, 80.0);
            crate::chem::tell_mob(world, mob, Span::Warning, &format!("{name} ледяной!"));
        }
        ShowerTemp::Boiling => {
            // `adjust_bodytemperature(35, 0, 500)`.
            if let Some(mut r) = world.get_mut::<crate::breath::Respiration>(mob)
                && r.bodytemperature <= 500.0
            {
                r.bodytemperature = (r.bodytemperature + 35.0).min(500.0);
            }
            crate::medical::diseases::damage(world, mob, crate::health::DamageType::Burn, 5.0);
            crate::chem::tell_mob(world, mob, Span::Danger, &format!("{name} обжигает!"));
        }
        ShowerTemp::Normal => {}
    }
}

/// What a shower mostly sprays (`expose_to_reagents`: 70 % of it blood or water, else none).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShowerReagent {
    Water,
    Blood,
    Dirty,
}

/// `/datum/mood_event/shower` and its `add_effects(shower_reagent)`.
fn shower_mood(kind: ShowerReagent) -> crate::mood::MoodEvent {
    let (description, mood_change, timeout) = match kind {
        ShowerReagent::Water => ("Я недавно принял приятный душ.", 4.0, 300.0),
        ShowerReagent::Blood => (
            "Недавно я принял ужасный душ, из которого лилась кровь!",
            -4.0,
            180.0,
        ),
        ShowerReagent::Dirty => ("Недавно я принял грязный душ!", -3.0, 180.0),
    };
    crate::mood::MoodEvent {
        id: "shower",
        description,
        mood_change,
        timeout,
        hidden: false,
    }
}

/// `expose_to_reagents(target)` of one atom under `shower`.
fn expose_to_reagents(
    world: &mut World,
    shower: Entity,
    target: Entity,
    holder: &Holder,
    kind: ShowerReagent,
    temp: ShowerTemp,
) {
    let modifier = SHOWER_EXPOSURE_MULTIPLIER * SHOWER_SPRAY_VOLUME
        / holder.total_volume.max(SHOWER_SPRAY_VOLUME);
    crate::janitor::expose(world, target, holder, modifier);
    if world.get::<crate::health::Health>(target).is_none() {
        return;
    }
    let name = name_of(world, shower);
    check_heat(world, target, temp, &name);
    if world.get::<crate::chem::MobReagents>(target).is_none() {
        return;
    }
    // `/datum/status_effect/washing_regen/tick`: 4 stamina a second under clean water.
    if kind == ShowerReagent::Water {
        crate::status_effects::adjust_stamina(world, target, -4.0 * PLUMBING_WAIT);
    }
    crate::mood::add_event(world, target, "shower", &shower_mood(kind));
}

/// `shower/process(seconds_per_tick)` of every shower, each `SSplumbing` tick.
fn run_showers(world: &mut World, mut since: Local<f32>) {
    *since += world.resource::<Time>().delta_secs();
    if *since < PLUMBING_WAIT {
        return;
    }
    let secs = *since;
    *since = 0.0;
    let showers: Vec<Entity> = world
        .query_filtered::<Entity, With<Shower>>()
        .iter(world)
        .collect();
    let t = now(world);
    for shower in showers {
        let Some(state) = world.get::<Shower>(shower).cloned() else {
            continue;
        };
        // The timed mode's cut-off: the user turns it back on by hand.
        if state.intended_on
            && state.mode == ShowerMode::Timed
            && t >= state.timed_until
            && let Some(mut s) = world.get_mut::<Shower>(shower)
        {
            s.intended_on = false;
        }
        if state.actually_on && volume(world, shower) < SHOWER_SPRAY_VOLUME {
            set_shower_on(world, shower, false);
            if state.mode != ShowerMode::Forever
                && let Some(mut s) = world.get_mut::<Shower>(shower)
            {
                s.intended_on = false;
            }
        } else {
            let intended = world.get::<Shower>(shower).is_some_and(|s| s.intended_on);
            set_shower_on(world, shower, intended);
        }
        let Some(state) = world.get::<Shower>(shower).cloned() else {
            continue;
        };
        if !state.actually_on {
            if state.reclaimer
                && let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone())
                && let Some(id) = chem.id(&state.reagent)
                && let Some(mut r) = world.get_mut::<Reagents>(shower)
                && !r.0.holder_full()
            {
                r.0.add_reagent(
                    &chem,
                    id,
                    state.refill_rate * secs,
                    ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
                    None,
                    None,
                    true,
                    false,
                );
            }
            continue;
        }
        spray(world, shower, state.temperature);
    }
}

/// The running half of `shower/process`: wash the tile, splash everything on it, use 5 units.
fn spray(world: &mut World, shower: Entity, temp: ShowerTemp) {
    let (Some(&pos), Some(holder), Some(chem)) = (
        world.get::<TilePos>(shower),
        world.get::<Reagents>(shower).map(|r| r.0.clone()),
        world.get_resource::<Chem>().map(|c| c.0.clone()),
    ) else {
        return;
    };
    let total = holder.total_volume;
    let purity = total * 0.7;
    let water = amount_of(&chem, &holder, WATER) >= purity && total > 0.0;
    let blood = amount_of(&chem, &holder, BLOOD) >= purity && total > 0.0;
    let kind = if blood {
        ShowerReagent::Blood
    } else if water {
        ShowerReagent::Water
    } else {
        ShowerReagent::Dirty
    };
    // "radiation my beloved": 20 % radium, uranium or triple polonium makes it radioactive.
    let radioactive = amount_of(&chem, &holder, RADIUM)
        + amount_of(&chem, &holder, URANIUM)
        + amount_of(&chem, &holder, POLONIUM) * 3.0
        >= total * 0.2;
    let mut flags = 0;
    if water {
        flags |= crate::janitor::CLEAN_WASH;
    }
    if !radioactive {
        flags |= crate::radiation::CLEAN_TYPE_RADIATION;
    }
    // `loc.wash(wash_flags, TRUE)`.
    crate::janitor::wash_turf(world, pos, flags, true);
    if let Some(turf) = crate::construction::turf_at(world, pos) {
        expose_to_reagents(world, shower, turf, &holder, kind, temp);
    }
    let contents: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| e != shower && world.get::<ss13_mapload::Turf>(e).is_none())
        .collect();
    for e in contents {
        if world.get_entity(e).is_ok() {
            expose_to_reagents(world, shower, e, &holder, kind, temp);
        }
    }
    if let Some(mut r) = world.get_mut::<Reagents>(shower) {
        r.0.remove_all(SHOWER_SPRAY_VOLUME);
    }
}

// --- Toilets ---------------------------------------------------------------------------------

/// `toilet/update_icon_state` (`"toilet[cover_open][cistern_open]"`) and `update_overlays` (the
/// bowl's water with the lid up, not while flushing).
fn refresh_toilet(world: &mut World, toilet: Entity) {
    let Some(state) = world.get::<Toilet>(toilet).cloned() else {
        return;
    };
    let icon_state = format!(
        "toilet{}{}",
        u8::from(state.cover_open),
        u8::from(state.cistern_open)
    );
    crate::obj_damage::set_icon_state(world, toilet, &icon_state);
    let overlays = if state.cover_open && state.flushing_until.is_none() {
        icon_id(world, toilet, "toilet-water")
            .map(Overlay::new)
            .into_iter()
            .collect()
    } else {
        Vec::new()
    };
    set_overlays(world, toilet, overlays);
}

/// `toilet/click_alt`: the lid goes up or down (not while flushing). True when `target` is a
/// toilet.
pub fn click_alt(world: &mut World, target: Entity) -> bool {
    // `simple_rotation` of the plumbing machines (`crate::plumbing_net`).
    if crate::plumbing_net::rotate_machine(world, target) {
        return true;
    }
    let Some(state) = world.get::<Toilet>(target).cloned() else {
        return false;
    };
    if state.flushing_until.is_some() {
        return true;
    }
    if let Some(mut t) = world.get_mut::<Toilet>(target) {
        t.cover_open = !t.cover_open;
    }
    refresh_toilet(world, target);
    true
}

/// tg's slam of an aggressively grabbed mob on the fixture's tile into it (`toilet` and
/// `urinal/attack_hand`). False when the user pulls nobody living.
fn slam(world: &mut World, client: Entity, mob: Entity, fixture: Entity, brute: f32) -> bool {
    let Some(victim) = world.get::<crate::pull::Pulling>(mob).map(|p| p.0) else {
        return false;
    };
    if world.get::<crate::health::Health>(victim).is_none() {
        return false;
    }
    let grab = world
        .get::<crate::pull::GrabState>(mob)
        .copied()
        .unwrap_or_default();
    if grab < crate::pull::GrabState::Aggressive {
        tell(world, client, Span::Warning, "Нужно схватить покрепче!");
        return true;
    }
    let (victim_name, name) = (name_of(world, victim), name_of(world, fixture));
    if world.get::<TilePos>(victim) != world.get::<TilePos>(fixture) {
        tell(
            world,
            client,
            Span::Warning,
            format!("{victim_name} должен быть на {name}!"),
        );
        return true;
    }
    let t = now(world);
    if let Some(mut next) = world.get_mut::<crate::combat::NextMove>(mob) {
        // `changeNext_move(CLICK_CD_MELEE)`.
        next.0 = t + 0.8;
    }
    if world.get::<Toilet>(fixture).is_some() {
        sound(world, fixture, BANG, 25.0);
    }
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        &format!("Вы впечатываете {victim_name} в {name}!"),
        &format!("{user} впечатывает {victim_name} в {name}!"),
        Span::Danger,
    );
    crate::medical::diseases::damage(world, victim, crate::health::DamageType::Brute, brute);
    true
}

/// `toilet/attack_hand(_secondary)`.
fn toilet_hand(world: &mut World, client: Entity, mob: Entity, toilet: Entity, right: bool) {
    let Some(state) = world.get::<Toilet>(toilet).cloned() else {
        return;
    };
    if right {
        flush(world, client, toilet, &state);
        return;
    }
    if slam(world, client, mob, toilet, 5.0) {
        return;
    }
    if !state.cistern_open || state.cover_open {
        return;
    }
    let items: Vec<Entity> = state
        .cistern
        .iter()
        .copied()
        .filter(|&i| inside(world, toilet, Some(i)).is_some())
        .collect();
    if items.is_empty() {
        tell(world, client, Span::Notice, "Бачок пуст.");
        return;
    }
    let pick = world
        .get_resource_mut::<crate::combat::CombatRng>()
        .map_or(0, |mut r| r.range(0, items.len() as u32 - 1) as usize);
    let item = items[pick];
    if let Some(mut t) = world.get_mut::<Toilet>(toilet) {
        t.cistern.retain(|&i| i != item);
    }
    give(world, mob, item);
    let what = name_of(world, item);
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы находите {what} в бачке."),
    );
}

/// `toilet/attack_hand_secondary`: the flush.
fn flush(world: &mut World, client: Entity, toilet: Entity, state: &Toilet) {
    if state.flushing_until.is_some() {
        return;
    }
    if volume(world, toilet) <= FLUSH_UNITS {
        tell(
            world,
            client,
            Span::Notice,
            "Вы нажимаете на рычаг смыва, но ничего не происходит.",
        );
        return;
    }
    let until = now(world) + FLUSH_SECS;
    if let Some(mut t) = world.get_mut::<Toilet>(toilet) {
        t.flushing_until = Some(until);
    }
    if inside(world, toilet, state.stuck).is_some() {
        // shortcut: tg foams the cistern out (`create_foam`); here it splashes the floor.
        let name = name_of(world, toilet);
        if let (Some(&pos), Some(holder)) = (
            world.get::<TilePos>(toilet),
            world.get::<Reagents>(toilet).map(|r| r.0.clone()),
        ) && let Some(turf) = crate::construction::turf_at(world, pos)
        {
            crate::janitor::expose(world, turf, &holder, 1.0);
        }
        if let Some(mut r) = world.get_mut::<Reagents>(toilet) {
            r.0.clear();
        }
        crate::chem::visible(
            world,
            toilet,
            "",
            &format!("{name} переполняется, разливая содержимое бачка повсюду!"),
            Span::Danger,
        );
    } else if let Some(mut r) = world.get_mut::<Reagents>(toilet) {
        r.0.remove_all(FLUSH_UNITS);
    }
    let vol = if state.cover_open { 40.0 } else { 20.0 };
    sound(world, toilet, FLUSH, vol);
    refresh_toilet(world, toilet);
}

/// The held-item half of a toilet (tg `tool_act`s, then `item_interaction`). True when handled.
fn toilet_item(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    toilet: Entity,
    path: &str,
) -> bool {
    // `if(user.combat_mode) return NONE`, and tool acts run only out of combat mode.
    if combat_mode(world, mob) {
        return false;
    }
    let Some(state) = world.get::<Toilet>(toilet).cloned() else {
        return false;
    };
    let name = name_of(world, toilet);
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    match tool_of(world, held).map(|(kind, _)| kind) {
        Some(ToolKind::Crowbar) => {
            let text = if state.cistern_open {
                "Вы начинаете возвращать крышку на бачок..."
            } else {
                "Вы начинаете поднимать крышку бачка..."
            };
            tell(world, client, Span::Notice, text);
            sound(world, toilet, STONE_DOOR, 50.0);
            crate::tools::use_tool(
                world,
                actor,
                toilet,
                CISTERN_DS,
                0,
                Job::Sink(SinkJob::Cistern),
            );
            return true;
        }
        Some(ToolKind::Screwdriver) => {
            if !state.cistern_open {
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!("Сначала откройте бачок {name}!"),
                );
            } else if !state.reclaimer {
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!("В {name} не установлен утилизатор воды."),
                );
            } else {
                if let Some(mut t) = world.get_mut::<Toilet>(toilet) {
                    t.reclaimer = false;
                }
                sound(world, toilet, "sound/items/tools/screwdriver.ogg", 50.0);
                if let Some(&pos) = world.get::<TilePos>(toilet) {
                    crate::construction::spawn_obj(world, WATER_RECYCLER, pos);
                }
                tell(
                    world,
                    client,
                    Span::Notice,
                    format!("Вы снимаете утилизатор воды с {name}."),
                );
            }
            return true;
        }
        Some(ToolKind::Wrench) => {
            sound(world, toilet, RATCHET, 50.0);
            crate::obj_damage::deconstruct(world, toilet, true);
            return true;
        }
        _ => {}
    }
    let what = name_of(world, held);
    if state.cistern_open {
        if is_under(path, WATER_RECYCLER) {
            if state.reclaimer {
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!("В {name} уже установлен утилизатор воды."),
                );
                return true;
            }
            sound(world, toilet, CLICK, 20.0);
            world.despawn(held);
            if let Some(mut t) = world.get_mut::<Toilet>(toilet) {
                t.reclaimer = true;
            }
            return true;
        }
        let w = w_class(world, held);
        if w > W_NORMAL {
            tell(world, client, Span::Warning, format!("{what} не влезает!"));
            return true;
        }
        let weight: u8 = state
            .cistern
            .iter()
            .filter(|&&i| inside(world, toilet, Some(i)).is_some())
            .map(|&i| w_class(world, i))
            .sum();
        if weight + w > W_HUGE {
            tell(world, client, Span::Warning, "Бачок полон!");
            return true;
        }
        if !hand_into(world, client, held, toilet) {
            return true;
        }
        if let Some(mut t) = world.get_mut::<Toilet>(toilet) {
            t.cistern.retain(|&i| i != held);
            t.cistern.push(held);
        }
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы аккуратно кладёте {what} в бачок."),
        );
        return true;
    }
    if !state.cover_open {
        return false;
    }
    if !is_under(path, crate::chem::CONTAINER_ROOT) {
        if w_class(world, held) > W_SMALL {
            return false;
        }
        if inside(world, toilet, state.stuck).is_some() {
            tell(
                world,
                client,
                Span::Warning,
                format!("Сток {name} уже чем-то забит!"),
            );
            return true;
        }
        if !hand_into(world, client, held, toilet) {
            return true;
        }
        if let Some(mut t) = world.get_mut::<Toilet>(toilet) {
            t.stuck = Some(held);
        }
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы бросаете {what} в чашу {name}."),
        );
        return true;
    }
    if volume(world, toilet) <= 0.0 {
        tell(world, client, Span::Notice, format!("{name} сух."));
        return true;
    }
    let Some(full) = world
        .get::<Reagents>(held)
        .filter(|r| r.0.is_refillable())
        .map(|r| r.0.holder_full())
    else {
        return false;
    };
    if full {
        tell(world, client, Span::Notice, format!("{what} полон."));
        return true;
    }
    let amount = world
        .get::<crate::chem::TransferAmount>(held)
        .map_or(5.0, |t| t.0);
    crate::chem::transfer(world, toilet, held, amount);
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы наполняете {what} из {name}. Мерзко."),
    );
    true
}

/// `toilet/process` (the recycler refills it) and `end_flushing`.
fn run_toilets(world: &mut World) {
    let secs = world.resource::<Time>().delta_secs();
    let t = now(world);
    let chem = world.get_resource::<Chem>().map(|c| c.0.clone());
    let toilets: Vec<(Entity, Toilet)> = world
        .query::<(Entity, &Toilet)>()
        .iter(world)
        .map(|(e, t)| (e, t.clone()))
        .collect();
    for (toilet, state) in toilets {
        if state.flushing_until.is_some_and(|until| t >= until) {
            if let Some(mut s) = world.get_mut::<Toilet>(toilet) {
                s.flushing_until = None;
            }
            refresh_toilet(world, toilet);
        }
        if !state.reclaimer {
            continue;
        }
        if let Some(chem) = &chem
            && let Some(id) = chem.id(&state.reagent)
            && let Some(mut r) = world.get_mut::<Reagents>(toilet)
            && !r.0.holder_full()
        {
            r.0.add_reagent(
                chem,
                id,
                state.reclaim_rate * secs,
                ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
                None,
                None,
                true,
                false,
            );
        }
    }
}

// --- Urinals ---------------------------------------------------------------------------------

/// `urinal/attack_hand`. True when handled.
fn urinal_hand(world: &mut World, client: Entity, mob: Entity, urinal: Entity) -> bool {
    if slam(world, client, mob, urinal, 8.0) {
        return true;
    }
    let Some(state) = world.get::<Urinal>(urinal).copied() else {
        return false;
    };
    if !state.exposed {
        return false;
    }
    match inside(world, urinal, state.hidden) {
        Some(item) => {
            let what = name_of(world, item);
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы выуживаете {what} из сливной решётки."),
            );
            if let Some(mut u) = world.get_mut::<Urinal>(urinal) {
                u.hidden = None;
            }
            give(world, mob, item);
        }
        None => tell(
            world,
            client,
            Span::Warning,
            "В сливной решётке ничего нет!",
        ),
    }
    true
}

/// The held-item half of a urinal (tg `screwdriver_act`, `wrench_act_secondary`,
/// `item_interaction`). True when handled.
fn urinal_item(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    urinal: Entity,
    modifiers: ClickModifiers,
) -> bool {
    let Some(state) = world.get::<Urinal>(urinal).copied() else {
        return false;
    };
    let tool = tool_of(world, held)
        .map(|(kind, _)| kind)
        .filter(|_| !combat_mode(world, mob));
    match tool {
        Some(ToolKind::Screwdriver) if !modifiers.right => {
            let text = if state.exposed {
                "Вы начинаете прикручивать крышку обратно..."
            } else {
                "Вы начинаете откручивать крышку сливной решётки..."
            };
            tell(world, client, Span::Notice, text);
            sound(world, urinal, STONE_DOOR, 50.0);
            let actor = Actor {
                client,
                mob,
                item: Some(held),
            };
            crate::tools::use_tool(
                world,
                actor,
                urinal,
                URINAL_CAP_DS,
                0,
                Job::Sink(SinkJob::UrinalCap),
            );
            return true;
        }
        Some(ToolKind::Wrench) if modifiers.right => {
            sound(world, urinal, RATCHET, 50.0);
            crate::obj_damage::deconstruct(world, urinal, true);
            tell(world, client, Span::Notice, "писсуар снят");
            return true;
        }
        _ => {}
    }
    if !state.exposed || modifiers.right {
        return false;
    }
    let what = name_of(world, held);
    if inside(world, urinal, state.hidden).is_some() {
        tell(
            world,
            client,
            Span::Warning,
            "В сливной решётке уже что-то есть!",
        );
        return true;
    }
    if w_class(world, held) > W_TINY {
        tell(
            world,
            client,
            Span::Warning,
            format!("{what} слишком велик для сливной решётки."),
        );
        return true;
    }
    if !hand_into(world, client, held, urinal) {
        return true;
    }
    if let Some(mut u) = world.get_mut::<Urinal>(urinal) {
        u.hidden = Some(held);
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы кладёте {what} в сливную решётку."),
    );
    true
}

/// Out of a toilet or a urinal onto `pos` (`dump_contents`, `hidden_item?.forceMove`).
fn dump(world: &mut World, holder: Entity, items: Vec<Entity>, pos: TilePos) {
    for item in items {
        if inside(world, holder, Some(item)).is_some() {
            crate::storage::take_out(world, item, pos);
        }
    }
}

/// `sink/Initialize`: `create_reagents(capacity, NO_REACT)`, full with a recycler.
fn add_sink(
    add: On<Add<DmObj>>,
    objs: Query<&DmObj>,
    chem: Option<Res<Chem>>,
    mut commands: Commands,
) {
    let Ok(dm) = objs.get(add.entity) else {
        return;
    };
    if !is_under(&dm.path, SINK) {
        return;
    }
    let owner = Owner {
        path: dm.path.clone(),
        living: false,
    };
    // Drainable: a held container's `interact_with_atom` fills from it (`crate::chem`).
    let mut holder = Holder::new(dm.capacity, flags::NO_REACT | flags::DRAINABLE, owner);
    if dm.water_reclaimer
        && let Some(chem) = chem
        && let Some(id) = chem.0.id(&dm.dispensed_reagent)
    {
        holder.add_reagent(
            &chem.0,
            id,
            dm.capacity,
            ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
            None,
            None,
            true,
            false,
        );
    }
    commands.entity(add.entity).insert((
        Sink {
            reclaimer: dm.water_reclaimer,
            reclaim_rate: dm.reclaim_rate,
            reagent: dm.dispensed_reagent.clone(),
        },
        Reagents(holder),
    ));
}

/// `sink/process(seconds_per_tick)`: the recycler tops the sink up.
fn reclaim_water(
    time: Res<Time>,
    chem: Option<Res<Chem>>,
    mut sinks: Query<(&Sink, &mut Reagents)>,
) {
    let Some(chem) = chem else {
        return;
    };
    let secs = time.delta_secs();
    for (sink, mut reagents) in &mut sinks {
        if !sink.reclaimer || reagents.0.holder_full() {
            continue;
        }
        let Some(id) = chem.0.id(&sink.reagent) else {
            continue;
        };
        reagents.0.add_reagent(
            &chem.0,
            id,
            sink.reclaim_rate * secs,
            ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
            None,
            None,
            true,
            false,
        );
    }
}

fn volume(world: &World, sink: Entity) -> f32 {
    world
        .get::<Reagents>(sink)
        .map_or(0.0, |r| r.0.total_volume)
}

/// `busy`: someone's wash at this sink is under way.
fn busy(world: &mut World, sink: Entity) -> bool {
    world
        .query::<&DoAfter>()
        .iter(world)
        .any(|d| d.target == sink && matches!(d.job, Job::Sink(_)))
}

fn sound(world: &mut World, sink: Entity, file: &str, vol: f32) {
    if let Some(&pos) = world.get::<TilePos>(sink) {
        playsound(world, PlaySound::new(pos, file, vol, true));
    }
}

/// `sink/attack_hand`: washing the hands, or the face when aiming at the head; a shower's,
/// toilet's or urinal's `attack_hand`. True when handled.
pub fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    if world.get::<Shower>(target).is_some() {
        shower_interact(world, client, target);
        return true;
    }
    if world.get::<Toilet>(target).is_some() {
        toilet_hand(world, client, mob, target, modifiers.right);
        return true;
    }
    if world.get::<Urinal>(target).is_some() {
        return urinal_hand(world, client, mob, target);
    }
    if !is_sink(world, target) {
        return false;
    }
    // `iscarbon(user)`.
    if world.get::<crate::chem::MobReagents>(mob).is_none() {
        return true;
    }
    if volume(world, target) < WASH_UNITS {
        tell(world, client, Span::Warning, "Раковина пуста!");
        return true;
    }
    if busy(world, target) {
        tell(world, client, Span::Warning, "Здесь уже кто-то моется!");
        return true;
    }
    let face = world
        .get::<ZoneSelected>(mob)
        .is_some_and(|z| z.0.check_zone() == BodyZone::Head);
    sound(world, target, FAUCET, 50.0);
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    if do_after(
        world,
        actor,
        target,
        WASH_DS,
        0,
        Job::Sink(SinkJob::Hands { face }),
    ) {
        let user = name_of(world, mob);
        let what = if face { "лицо" } else { "руки" };
        visible(
            world,
            mob,
            &format!("Вы начинаете мыть {what}..."),
            &format!("{user} начинает мыть {what}..."),
            Span::Notice,
        );
    }
    true
}

/// `sink/item_interaction(_secondary)` of `held`, and the held-item clicks of showers, toilets
/// and urinals. True when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    let path = world
        .get::<DmItem>(held)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    if world.get::<Shower>(target).is_some() {
        return !modifiers.right && shower_item(world, client, mob, held, target, &path);
    }
    if world.get::<Toilet>(target).is_some() {
        return !modifiers.right && toilet_item(world, client, mob, held, target, &path);
    }
    if world.get::<Urinal>(target).is_some() {
        return urinal_item(world, client, mob, held, target, modifiers);
    }
    if !is_sink(world, target) {
        return false;
    }
    let container = is_under(&path, crate::chem::CONTAINER_ROOT);
    let tool = tool_of(world, held).map(|(kind, _)| kind);
    if !modifiers.right && tool == Some(ToolKind::Wrench) {
        // `wrench_act`: taken apart at once.
        sound(world, target, "sound/items/tools/ratchet.ogg", 50.0);
        crate::obj_damage::deconstruct(world, target, true);
        return true;
    }
    if !modifiers.right && tool == Some(ToolKind::Crowbar) {
        crowbar(world, client, target);
        return true;
    }
    if busy(world, target) {
        tell(world, client, Span::Warning, "Здесь уже кто-то моется!");
        return true;
    }
    if modifiers.right {
        // `item_interaction_secondary`: a container (or a mop) is emptied down the drain.
        if !container && !is_under(&path, MOP) {
            return false;
        }
        let Some(mut reagents) = world.get_mut::<Reagents>(held) else {
            return true;
        };
        if reagents.0.total_volume <= 0.0 {
            tell(world, client, Span::Warning, "Уже пусто!");
            return true;
        }
        reagents.0.clear();
        sound(world, target, SLOSH, 25.0);
        let (user, what, sink) = (
            name_of(world, mob),
            name_of(world, held),
            name_of(world, target),
        );
        visible(
            world,
            mob,
            &format!("Вы выливаете {what} в {sink}, смывая содержимое в слив."),
            &format!("{user} выливает {what} в {sink}."),
            Span::Notice,
        );
        return true;
    }
    if container {
        // Filled by `crate::chem` (the sink is drainable).
        return false;
    }
    if is_under(&path, MOP) {
        let name = name_of(world, target);
        if volume(world, target) <= 0.0 {
            tell(world, client, Span::Notice, format!("{name} пуста."));
            return true;
        }
        crate::chem::transfer(world, target, held, WASH_UNITS);
        sound(world, target, SLOSH, 25.0);
        let what = name_of(world, held);
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы смачиваете {what} в {name}."),
        );
        return true;
    }
    if world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
    {
        return false;
    }
    if volume(world, target) < WASH_UNITS {
        tell(world, client, Span::Warning, "Раковина пуста!");
        return true;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    if do_after(world, actor, target, WASH_DS, 0, Job::Sink(SinkJob::Item)) {
        let what = name_of(world, held);
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы начинаете мыть {what}..."),
        );
        sound(world, target, FAUCET, 50.0);
    }
    true
}

/// `sink/crowbar_act`: the recycler out.
fn crowbar(world: &mut World, client: Entity, sink: Entity) {
    let Some(reclaimer) = world.get::<Sink>(sink).map(|s| s.reclaimer) else {
        return;
    };
    if !reclaimer {
        tell(world, client, Span::Warning, "Здесь нет утилизатора воды.");
        return;
    }
    if let Some(mut s) = world.get_mut::<Sink>(sink) {
        s.reclaimer = false;
    }
    sound(world, sink, CLICK, 20.0);
    if let Some(&pos) = world.get::<TilePos>(sink) {
        crate::construction::spawn_obj(world, WATER_RECYCLER, pos);
    }
    let name = name_of(world, sink);
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы снимаете утилизатор воды с {name}."),
    );
}

/// `atom_deconstruct` of a sink (its stack and its recycler), a shower, a toilet or a urinal.
/// True when it was one of them.
pub(crate) fn deconstruct(world: &mut World, sink: Entity, dm: &DmObj, pos: TilePos) -> bool {
    if let Some(shower) = world.get::<Shower>(sink).cloned() {
        // `shower/on_deconstruction`: 2 iron and the recycler.
        crate::construction::spawn_stack(world, IRON, pos, 2);
        if shower.reclaimer {
            crate::construction::spawn_obj(world, WATER_RECYCLER, pos);
        }
        return true;
    }
    if let Some(toilet) = world.get::<Toilet>(sink).cloned() {
        // `toilet/atom_deconstruct`: its contents, its sheet (`drop_custom_materials`, none for
        // the greyscale ones) and the recycler.
        let mut items = toilet.cistern.clone();
        items.extend(toilet.stuck);
        dump(world, sink, items, pos);
        if !is_under(&dm.path, TOILET_GREYSCALE) {
            crate::construction::spawn_stack(world, IRON, pos, 1);
        }
        if toilet.reclaimer {
            crate::construction::spawn_obj(world, WATER_RECYCLER, pos);
        }
        return true;
    }
    if let Some(urinal) = world.get::<Urinal>(sink).copied() {
        // `urinal/atom_deconstruct`: its frame and what it hid.
        crate::construction::spawn_obj(world, URINAL_FRAME, pos);
        dump(world, sink, urinal.hidden.into_iter().collect(), pos);
        return true;
    }
    if !is_under(&dm.path, SINK) {
        return false;
    }
    if !dm.build_stack.is_empty() && dm.build_stack_amount > 0 {
        crate::construction::spawn_stack(world, &dm.build_stack, pos, dm.build_stack_amount);
    }
    if world.get::<Sink>(sink).is_some_and(|s| s.reclaimer) {
        crate::construction::spawn_obj(world, WATER_RECYCLER, pos);
    }
    true
}

/// End of a [`SinkJob`] (`crate::tools` checked nobody moved): 5 units go, then the wash.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: SinkJob) {
    let sink = action.target;
    match job {
        SinkJob::ShowerTemperature => {
            let Some(temp) = world.get::<Shower>(sink).map(|s| s.temperature.next()) else {
                return;
            };
            if let Some(mut s) = world.get_mut::<Shower>(sink) {
                s.temperature = temp;
            }
            let (user, tool) = (
                name_of(world, mob),
                action.item.map(|i| name_of(world, i)).unwrap_or_default(),
            );
            visible(
                world,
                mob,
                &format!(
                    "Вы регулируете душ с помощью {tool}: температура {}.",
                    temp.word()
                ),
                &format!("{user} регулирует душ с помощью {tool}."),
                Span::Notice,
            );
            return;
        }
        SinkJob::Cistern => {
            let Some(open) = world.get::<Toilet>(sink).map(|t| t.cistern_open) else {
                return;
            };
            if let Some(mut t) = world.get_mut::<Toilet>(sink) {
                t.cistern_open = !open;
            }
            refresh_toilet(world, sink);
            let user = name_of(world, mob);
            let (own, others) = if open {
                (
                    "Вы возвращаете крышку на бачок!",
                    "возвращает крышку на бачок!",
                )
            } else {
                ("Вы поднимаете крышку бачка!", "поднимает крышку бачка!")
            };
            visible(world, mob, own, &format!("{user} {others}"), Span::Notice);
            return;
        }
        SinkJob::UrinalCap => {
            let Some(exposed) = world.get::<Urinal>(sink).map(|u| u.exposed) else {
                return;
            };
            if let Some(mut u) = world.get_mut::<Urinal>(sink) {
                u.exposed = !exposed;
            }
            let user = name_of(world, mob);
            let (own, others) = if exposed {
                (
                    "Вы прикручиваете крышку обратно!",
                    "прикручивает крышку обратно!",
                )
            } else {
                (
                    "Вы откручиваете крышку сливной решётки!",
                    "откручивает крышку сливной решётки!",
                )
            };
            visible(world, mob, own, &format!("{user} {others}"), Span::Notice);
            return;
        }
        SinkJob::Hands { .. } | SinkJob::Item => {}
    }
    if volume(world, sink) < WASH_UNITS {
        tell(world, action.client, Span::Warning, "Раковина пуста!");
        return;
    }
    if let Some(mut reagents) = world.get_mut::<Reagents>(sink) {
        reagents.0.remove_all(WASH_UNITS);
    }
    let (user, name) = (name_of(world, mob), name_of(world, sink));
    match job {
        SinkJob::Hands { face } => {
            if !face {
                // `wash_hands`: the gloves, else the hands.
                let gloves = world
                    .get::<Inventory>(mob)
                    .and_then(|i| i.slot(InventorySlot::Gloves));
                crate::janitor::wash(world, gloves.unwrap_or(mob), crate::janitor::CLEAN_WASH);
            }
            let what = if face { "лицо" } else { "руки" };
            visible(
                world,
                mob,
                &format!("Вы моете {what} в {name}."),
                &format!("{user} моет {what} в {name}."),
                Span::Notice,
            );
        }
        SinkJob::ShowerTemperature | SinkJob::Cistern | SinkJob::UrinalCap => {}
        SinkJob::Item => {
            let Some(item) = action.item else {
                return;
            };
            crate::janitor::wash(world, item, crate::janitor::CLEAN_WASH);
            let what = name_of(world, item);
            visible(
                world,
                mob,
                &format!("Вы моете {what} в {name}."),
                &format!("{user} моет {what} в {name}."),
                Span::Notice,
            );
        }
    }
}

/// tg `sink/examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let liquids = |lines: &mut Vec<crate::examine::Line>| {
        if let Some(r) = world.get::<Reagents>(target) {
            lines.push((
                Span::Notice,
                format!(
                    "Осталось жидкости: {}/{}.",
                    r.0.total_volume.floor(),
                    r.0.maximum_volume.floor()
                ),
            ));
        }
    };
    if let Some(shower) = world.get::<Shower>(target) {
        // `shower/examine`.
        lines.push((
            Span::Notice,
            "Похоже, у термостата есть регулировочный винт.".to_owned(),
        ));
        if shower.reclaimer {
            lines.push((
                Span::Notice,
                "Установлен утилизатор воды. Похоже, его можно поддеть ломом.".to_owned(),
            ));
        }
        lines.push((
            Span::Notice,
            format!(
                "Автоотключение настроено так: {}.",
                shower.mode.description()
            ),
        ));
        liquids(lines);
        return;
    }
    if let Some(toilet) = world.get::<Toilet>(target) {
        // `toilet/examine`.
        if toilet.cover_open && inside(world, target, toilet.stuck).is_some() {
            lines.push((
                Span::Notice,
                "Кажется, в чаше застряло что-то маленькое...".to_owned(),
            ));
        }
        if toilet.cistern_open && toilet.reclaimer {
            lines.push((
                Span::Notice,
                "Установлен утилизатор воды. Он держится на паре винтов.".to_owned(),
            ));
            liquids(lines);
        }
        return;
    }
    let Some(sink) = world.get::<Sink>(target) else {
        return;
    };
    if sink.reclaimer {
        lines.push((
            Span::Notice,
            "Установлен утилизатор воды. Похоже, его можно поддеть ломом.".to_owned(),
        ));
    }
    if let Some(r) = world.get::<Reagents>(target) {
        lines.push((
            Span::Notice,
            format!(
                "Осталось жидкости: {}/{}.",
                r.0.total_volume.floor(),
                r.0.maximum_volume.floor()
            ),
        ));
    }
    lines.push((
        Span::Notice,
        "Контейнер можно опустошить в слив правым кликом.".to_owned(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shower_settings_cycle_like_tg() {
        // `screwdriver_act`: normal -> freezing -> boiling -> normal.
        let t = ShowerTemp::Normal;
        assert_eq!(t.next(), ShowerTemp::Freezing);
        assert_eq!(t.next().next(), ShowerTemp::Boiling);
        assert_eq!(t.next().next().next(), ShowerTemp::Normal);
        // `multitool_act`: `(mode + 1) % SHOWER_MODE_COUNT`.
        let m = ShowerMode::UntilEmpty;
        assert_eq!(m.next(), ShowerMode::Timed);
        assert_eq!(m.next().next(), ShowerMode::Forever);
        assert_eq!(m.next().next().next(), ShowerMode::UntilEmpty);
    }

    #[test]
    fn shower_mood_follows_the_reagent() {
        let water = shower_mood(ShowerReagent::Water);
        assert_eq!((water.mood_change, water.timeout), (4.0, 300.0));
        let blood = shower_mood(ShowerReagent::Blood);
        assert_eq!((blood.mood_change, blood.timeout), (-4.0, 180.0));
        let dirty = shower_mood(ShowerReagent::Dirty);
        assert_eq!((dirty.mood_change, dirty.timeout), (-3.0, 180.0));
        assert_eq!(water.id, "shower");
    }

    #[test]
    fn old_prototypes_get_tg_bath_defaults() {
        let dm = DmObj {
            path: SHOWER.into(),
            ..default()
        };
        let (reagents, reagent, rate) = bath_reagents(None, &dm, true);
        assert_eq!(reagents.0.maximum_volume, BATH_CAPACITY);
        assert_eq!(reagent, WATER);
        assert_eq!(rate, BATH_REFILL);
    }
}
