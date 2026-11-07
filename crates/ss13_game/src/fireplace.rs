//! Fireplaces and bonfires (server): tg `/obj/structure/fireplace` (`fireplace.dm`) and
//! `/obj/structure/bonfire` (`bonfire.dm`).
//!
//! - Fireplace: wood planks (150 ds each, up to 3000 ds of fuel), a paper (5 ds) or a paper bin
//!   (5 ds a sheet) go in as fuel; a hot item (`get_temperature` at least
//!   `FIRE_MINIMUM_TEMPERATURE_TO_EXIST`) lights it when it has fuel. Lit, every second it exposes
//!   its tile (700 K, 2.5 L/s) and draws `fireplace_fire0..4` plus `fireplace_glow` and a light of
//!   range 1..6 by the burn time left, until the fuel runs out. Water puts it out, keeping the fuel.
//! - Bonfire: a rod builds a stake or a grill (`tgui_alert`); a hot item lights it when there are
//!   5 moles of oxygen on its tile. Lit, every second it needs that oxygen, exposes its tile
//!   (1000 K, 250 L/s, not with a grill), sets the mobs on it alight (2.5 fire stacks/s), grills
//!   the food on its grill and `fire_act`s the rest. A burning mob or item entering an unlit
//!   bonfire lights it; entering a lit one without a grill burns. Out of combat mode a held item
//!   goes onto a grill at the clicked point. An empty hand (5 s) takes an unlit one apart into its
//!   logs and a rod. Prelit ones light once the air exists. Fire on its tile lights it; water puts
//!   it out.
//! - Both play the burning loop (`match_strike`, then `comfyfire` every 5 s) while lit.
// shortcut: no buckling to the stake (`buckle_requires_restraints`, the 13 px shift), the rod is an
// overlay rather than an underlay; no smoke/bonfire particles, no "warm and cozy" description, no
// screentips. Add with buckling by state and particles.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmItem, DmObj, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::protocol::ClickModifiers;
use ss13_shared::{Appearance, Dir, Facing, IconStateId, LightSource, Optics, Overlay, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_world::{Moved, SpatialIndex};

use crate::medical::{name_of, visible};
use crate::obj_damage::{is_under, set_icon_state};
use crate::sound::{MEDIUM_RANGE_SOUND_EXTRARANGE, PlaySound, playsound};
use crate::tools::{Actor, DoAfter, tell};

pub const FIREPLACE: &str = "/obj/structure/fireplace";
pub const BONFIRE: &str = "/obj/structure/bonfire";
const BONFIRE_PRELIT: &str = "/obj/structure/bonfire/prelit";
const BONFIRE_DENSE_PRELIT: &str = "/obj/structure/bonfire/dense/prelit";
const WOOD: &str = "/obj/item/stack/sheet/mineral/wood";
const PAPER: &str = "/obj/item/paper";
const PAPER_BIN: &str = "/obj/item/paper_bin";
const RODS: &str = "/obj/item/stack/rods";
const ROASTING_STICK: &str = "/obj/item/melee/roastingstick";
const LOG: &str = "/obj/item/grown/log";

/// `LOG_BURN_TIMER`, `PAPER_BURN_TIMER`, `MAXIMUM_BURN_TIMER`: deciseconds of fuel.
const LOG_BURN_DS: f64 = 150.0;
const PAPER_BURN_DS: f64 = 5.0;
const MAX_BURN_DS: f64 = 3000.0;
/// `BONFIRE_FIRE_STACK_STRENGTH`.
const BONFIRE_FIRE_STACKS: f32 = 5.0;
/// `check_oxygen`: moles of oxygen a bonfire needs.
const BONFIRE_O2: f64 = 5.0;
/// `attack_hand`: `do_after(user, 5 SECONDS)`.
const DISMANTLE_DS: f32 = 50.0;
/// `fireplace/light_angle`.
const FIREPLACE_LIGHT_ANGLE: u16 = 170;
/// `bonfire/start_burning`: `set_light(6)`.
const BONFIRE_LIGHT: f32 = 6.0;
/// `/datum/looping_sound/burning`.
const LOOP_START: &str = "sound/items/match_strike.ogg";
const LOOP_MID: &str = "sound/effects/comfyfire.ogg";
const LOOP_START_SECS: f64 = 3.0;
const LOOP_MID_SECS: f64 = 5.0;
const LOOP_VOLUME: f32 = 50.0;
/// Every process tick; tg's SSobj runs every 2 s with `seconds_per_tick` 2, the rates scale.
const TICK_SECS: f32 = 1.0;

/// tg `fireplace` vars: `lit`, `fuel_added` (ds, unlit), `flame_expiry_timer` (game seconds, lit).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Fireplace {
    pub lit: bool,
    pub fuel_added: f64,
    pub expiry: f64,
    next_sound: f64,
    overlays: Vec<IconStateId>,
}

/// tg `bonfire` vars: `burning`, `grill`, `can_buckle` (the stake).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Bonfire {
    pub burning: bool,
    pub grill: bool,
    pub stake: bool,
    next_sound: f64,
    overlays: Vec<IconStateId>,
}

/// A `bonfire/prelit` waiting for its `LateInitialize` (the air of its tile).
#[derive(Component, Clone, Copy, Debug)]
struct Prelit;

/// A bonfire's "Stake or Grill" question.
#[derive(Component, Clone, Copy, Debug)]
struct RodAsk {
    bonfire: Entity,
    rods: Entity,
}

pub struct FireplacePlugin;

impl Plugin for FireplacePlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // `bonfire/attack_hand`: taking it apart.
        app.add_click_handler(ClickPhase::EmptyHand, 3466, |w, c| {
            !c.modifiers.right && crate::fireplace::attack_hand(w, c.client, c.mob, c.target)
        });
        // `item_interaction` of fireplaces and bonfires.
        app.add_click_handler(ClickPhase::HeldOnTarget, 3442, |w, c| {
            c.held.is_some_and(|h| {
                crate::fireplace::item_interaction(w, c.client, c.mob, h, c.target, c.modifiers)
            })
        });
        app.add_observer(add_fire)
            .add_systems(
                Update,
                (process, entered).after(crate::click::ResolveClicks),
            )
            .add_systems(
                Update,
                rod_answers
                    .after(ss13_tgui::prompt::PromptSystems::Answer)
                    .in_set(ss13_tgui::server::UiSystems::Handle),
            );
    }
}

fn add_fire(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(obj) = objs.get(add.entity) else {
        return;
    };
    if is_under(&obj.path, FIREPLACE) {
        commands
            .entity(add.entity)
            .try_insert_if_new(Fireplace::default());
    } else if is_under(&obj.path, BONFIRE) {
        let prelit =
            is_under(&obj.path, BONFIRE_PRELIT) || is_under(&obj.path, BONFIRE_DENSE_PRELIT);
        let mut e = commands.entity(add.entity);
        e.try_insert_if_new(Bonfire::default());
        if prelit {
            e.try_insert(Prelit);
        }
    }
}

use crate::tools::now;

fn path_of(world: &World, e: Entity) -> Option<&str> {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.as_str())
        .or_else(|| world.get::<DmObj>(e).map(|o| o.path.as_str()))
}

fn held_is(world: &World, held: Entity, root: &str) -> bool {
    world
        .get::<DmItem>(held)
        .is_some_and(|i| is_under(&i.path, root))
}

/// `get_temperature() >= FIRE_MINIMUM_TEMPERATURE_TO_EXIST` (`ignition_effect`).
fn is_hot(world: &World, item: Entity) -> bool {
    crate::medical::surgery::ToolInfo::read(world, Some(item)).temperature
        >= crate::medical::surgery::FIRE_MINIMUM_TEMPERATURE_TO_EXIST
}

/// Replaces the overlays this module drew last time with `states` (`update_overlays`).
fn set_overlays(world: &mut World, e: Entity, old: &[IconStateId], new: Vec<Overlay>) {
    if let Some(mut look) = world.get_mut::<Appearance>(e) {
        look.overlays.retain(|o| !old.contains(&o.icon_state));
        look.overlays.extend(new);
    }
}

fn icon_id(world: &World, e: Entity, state: &str) -> Option<IconStateId> {
    let icon = world.get::<DmAtom>(e)?.icon.clone();
    world.get_resource::<IconIndex>()?.get(&icon, state)
}

/// `set_light(range)` (0 = off) with the fire's colour, power 1.
fn set_light(world: &mut World, e: Entity, light: Option<LightSource>) {
    let mut optics = world.get::<Optics>(e).cloned().unwrap_or_default();
    if optics.light == light {
        return;
    }
    optics.light = light;
    world.entity_mut(e).insert(optics);
}

fn fire_light(range: f32, angle: u16, dir: Dir, offset: [i8; 2]) -> LightSource {
    LightSource {
        range,
        power: 1.0,
        color: crate::fire::LIGHT_COLOR_FIRE,
        height: 1.0,
        angle,
        dir,
        offset,
    }
}

/// `/datum/looping_sound/burning`: the start sound now, the mid sound after it.
fn start_loop(world: &mut World, e: Entity) -> f64 {
    if let Some(&pos) = world.get::<TilePos>(e) {
        playsound(
            world,
            PlaySound::new(pos, LOOP_START, LOOP_VOLUME, true)
                .extrarange(MEDIUM_RANGE_SOUND_EXTRARANGE),
        );
    }
    now(world) + LOOP_START_SECS
}

/// The mid sound when it is due; the next time it is.
fn tick_loop(world: &mut World, e: Entity, due: f64) -> f64 {
    let t = now(world);
    if t < due {
        return due;
    }
    if let Some(&pos) = world.get::<TilePos>(e) {
        playsound(
            world,
            PlaySound::new(pos, LOOP_MID, LOOP_VOLUME, true)
                .extrarange(MEDIUM_RANGE_SOUND_EXTRARANGE),
        );
    }
    t + LOOP_MID_SECS
}

// --- Fireplace -----------------------------------------------------------------------------

impl Fireplace {
    /// `burn_time_remaining`, deciseconds.
    pub fn remaining(&self, now: f64) -> f64 {
        if self.lit {
            ((self.expiry - now) * 10.0).max(0.0)
        } else {
            self.fuel_added.max(0.0)
        }
    }

    /// `adjust_fuel_timer(amount)`.
    // tg-fork deviation: tg resets a lit fire's timer to the full 3000 ds whenever less than that
    // is left (`< MAXIMUM_BURN_TIMER`), so any fuel refills it; the intent is the cap.
    pub fn add_fuel(&mut self, amount: f64, now: f64) {
        if self.lit {
            self.expiry += amount / 10.0;
            if self.remaining(now) > MAX_BURN_DS {
                self.expiry = now + MAX_BURN_DS / 10.0;
            }
        } else {
            self.fuel_added = (self.fuel_added + amount).clamp(0.0, MAX_BURN_DS);
        }
    }
}

/// `update_overlays` / `adjust_light` step by the burn time left: 0..=4.
pub fn fire_level(remaining: f64) -> usize {
    match remaining {
        r if r <= 500.0 => 0,
        r if r <= 1000.0 => 1,
        r if r <= 1500.0 => 2,
        r if r <= 2000.0 => 3,
        _ => 4,
    }
}

/// `adjust_light`'s ranges by [`fire_level`].
const FIREPLACE_LIGHTS: [f32; 5] = [1.0, 2.0, 3.0, 4.0, 6.0];

/// `update_overlays` and `adjust_light`.
fn fireplace_look(world: &mut World, e: Entity) {
    let Some(f) = world.get::<Fireplace>(e).cloned() else {
        return;
    };
    let mut new = Vec::new();
    let mut ids = Vec::new();
    let light = if f.lit {
        let level = fire_level(f.remaining(now(world)));
        for state in [
            format!("fireplace_fire{level}"),
            "fireplace_glow".to_owned(),
        ] {
            if let Some(id) = icon_id(world, e, &state) {
                new.push(Overlay::new(id));
                ids.push(id);
            }
        }
        // `get_light_offset`: half a tile back into the wall.
        let dir = world.get::<Facing>(e).map_or(Dir::South, |f| f.0);
        let (dx, dy) = dir.reverse().offset();
        let offset = [(dx * 16) as i8, (dy * 16) as i8];
        Some(fire_light(
            FIREPLACE_LIGHTS[level],
            FIREPLACE_LIGHT_ANGLE,
            dir,
            offset,
        ))
    } else {
        None
    };
    set_overlays(world, e, &f.overlays, new);
    if let Some(mut f) = world.get_mut::<Fireplace>(e) {
        f.overlays = ids;
    }
    set_light(world, e, light);
}

/// `fireplace/ignite`.
fn ignite_fireplace(world: &mut World, e: Entity) {
    let t = now(world);
    let next = start_loop(world, e);
    if let Some(mut f) = world.get_mut::<Fireplace>(e) {
        f.lit = true;
        f.expiry = t + f.fuel_added / 10.0;
        f.fuel_added = 0.0;
        f.next_sound = next;
    }
    fireplace_look(world, e);
}

/// `fireplace/put_out`.
fn put_out_fireplace(world: &mut World, e: Entity) {
    if let Some(mut f) = world.get_mut::<Fireplace>(e) {
        f.lit = false;
    }
    fireplace_look(world, e);
}

fn fireplace_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(f) = world.get::<Fireplace>(target).cloned() else {
        return false;
    };
    let t = now(world);
    let (user, what) = (name_of(world, mob), name_of(world, held));
    if held_is(world, held, WOOD) {
        let space = ((MAX_BURN_DS - f.remaining(t)) / LOG_BURN_DS).floor() as u32;
        if space < 1 {
            tell(
                world,
                client,
                Span::Warning,
                format!("Больше {what} в камин не поместится!"),
            );
            return true;
        }
        let used = space.min(crate::tools::stack_amount(world, held));
        crate::tools::use_stack(world, held, used);
        if let Some(mut f) = world.get_mut::<Fireplace>(target) {
            f.add_fuel(LOG_BURN_DS * f64::from(used), t);
        }
        visible(
            world,
            mob,
            "Вы подбрасываете топливо в камин.",
            &format!("{user} подбрасывает дрова в камин."),
            Span::Notice,
        );
        fireplace_look(world, target);
        return true;
    }
    let paper_bin = world
        .get::<crate::service::PaperBin>(held)
        .filter(|_| held_is(world, held, PAPER_BIN))
        .map(|b| b.total);
    if let Some(total) = paper_bin {
        visible(
            world,
            mob,
            &format!("Вы бросаете {what} в камин."),
            &format!("{user} бросает {what} в камин."),
            Span::Notice,
        );
        if let Some(mut f) = world.get_mut::<Fireplace>(target) {
            f.add_fuel(PAPER_BURN_DS * f64::from(total), t);
        }
        world.despawn(held);
        fireplace_look(world, target);
        return true;
    }
    if held_is(world, held, PAPER) {
        visible(
            world,
            mob,
            &format!("Вы бросаете {what} в камин."),
            &format!("{user} бросает {what} в камин."),
            Span::Notice,
        );
        if let Some(mut f) = world.get_mut::<Fireplace>(target) {
            f.add_fuel(PAPER_BURN_DS, t);
        }
        world.despawn(held);
        fireplace_look(world, target);
        return true;
    }
    if !is_hot(world, held) {
        return false;
    }
    // `try_light`.
    if f.lit {
        tell(world, client, Span::Warning, "Он уже горит!");
    } else if f.fuel_added <= 0.0 {
        tell(world, client, Span::Warning, "Камину нужно топливо!");
    } else {
        let name = name_of(world, target);
        let text = format!("{user} зажигает {name} с помощью {what}.");
        visible(world, mob, &text, &text, Span::Notice);
        ignite_fireplace(world, target);
    }
    true
}

/// `fireplace/process`.
fn process_fireplace(world: &mut World, e: Entity) {
    let Some(f) = world.get::<Fireplace>(e).cloned() else {
        return;
    };
    if !f.lit {
        return;
    }
    if now(world) > f.expiry {
        put_out_fireplace(world, e);
        return;
    }
    if let Some(&pos) = world.get::<TilePos>(e)
        && let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>()
    {
        atmos.hotspot_expose(pos, 700.0, 2.5 * f64::from(TICK_SECS), false);
    }
    let next = tick_loop(world, e, f.next_sound);
    if let Some(mut f) = world.get_mut::<Fireplace>(e) {
        f.next_sound = next;
    }
    fireplace_look(world, e);
}

// --- Bonfire -------------------------------------------------------------------------------

fn bonfire_look(world: &mut World, e: Entity) {
    let Some(b) = world.get::<Bonfire>(e).cloned() else {
        return;
    };
    set_icon_state(
        world,
        e,
        if b.burning {
            "bonfire_on_fire"
        } else {
            "bonfire"
        },
    );
    let mut new = Vec::new();
    let mut ids = Vec::new();
    if b.stake
        && let Some(id) = icon_id(world, e, "bonfire_rod")
    {
        new.push(Overlay {
            pixel_offset: [0, 16],
            ..Overlay::new(id)
        });
        ids.push(id);
    }
    if b.grill
        && let Some(id) = icon_id(world, e, "bonfire_grill")
    {
        new.push(Overlay::new(id));
        ids.push(id);
    }
    set_overlays(world, e, &b.overlays, new);
    if let Some(mut b) = world.get_mut::<Bonfire>(e) {
        b.overlays = ids;
    }
    let light = b
        .burning
        .then(|| fire_light(BONFIRE_LIGHT, 360, Dir::South, [0, 0]));
    set_light(world, e, light);
}

/// `check_oxygen`.
fn has_oxygen(world: &World, e: Entity) -> bool {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return false;
    };
    world
        .get_resource::<ss13_atmos::Atmos>()
        .and_then(|a| a.moles(pos, ss13_atmos::GasId::O2))
        .is_some_and(|m| m >= BONFIRE_O2)
}

/// `start_burning`. True when it lit.
pub fn start_burning(world: &mut World, e: Entity) -> bool {
    if world.get::<Bonfire>(e).is_none_or(|b| b.burning) || !has_oxygen(world, e) {
        return false;
    }
    let next = start_loop(world, e);
    if let Some(mut b) = world.get_mut::<Bonfire>(e) {
        b.burning = true;
        b.next_sound = next;
    }
    bonfire_look(world, e);
    bonfire_burn(world, e, 2.0);
    true
}

/// `bonfire/extinguish`.
fn put_out_bonfire(world: &mut World, e: Entity) {
    match world.get_mut::<Bonfire>(e) {
        Some(mut b) if b.burning => b.burning = false,
        _ => return,
    }
    bonfire_look(world, e);
}

/// `bonfire_burn(seconds_per_tick)`.
fn bonfire_burn(world: &mut World, e: Entity, dt: f32) {
    let (Some(&pos), Some(grill)) = (
        world.get::<TilePos>(e),
        world.get::<Bonfire>(e).map(|b| b.grill),
    ) else {
        return;
    };
    if !grill && let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
        atmos.hotspot_expose(pos, 1000.0, 250.0 * f64::from(dt), true);
    }
    let on_tile: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&x| x != e)
        .collect();
    for thing in on_tile {
        burn_one(world, e, thing, grill, dt);
    }
}

fn burn_one(world: &mut World, bonfire: Entity, thing: Entity, grill: bool, dt: f32) {
    if world.get_entity(thing).is_err() || world.get::<ss13_mapload::Turf>(thing).is_some() {
        return;
    }
    if world.get::<crate::health::Health>(thing).is_some() {
        crate::fire::adjust_fire_stacks(world, thing, BONFIRE_FIRE_STACKS * 0.5 * dt);
        crate::fire::ignite_mob(world, thing);
    } else if grill && world.get::<DmItem>(thing).is_some() {
        // `COMSIG_ITEM_GRILL_PROCESS`: only grillable food reacts.
        crate::cooking::grill_process(world, thing, bonfire, dt);
    } else {
        crate::fire::fire_act(world, thing, 1000.0);
    }
}

/// `bonfire/process`.
fn process_bonfire(world: &mut World, e: Entity) {
    if world.get::<Prelit>(e).is_some() {
        // `LateInitialize`: once the air is there.
        if world.get_resource::<ss13_atmos::Atmos>().is_some() {
            world.entity_mut(e).remove::<Prelit>();
            start_burning(world, e);
        }
        return;
    }
    let Some(b) = world.get::<Bonfire>(e).cloned() else {
        return;
    };
    if !b.burning {
        return;
    }
    if !has_oxygen(world, e) {
        put_out_bonfire(world, e);
        return;
    }
    bonfire_burn(world, e, TICK_SECS);
    let next = tick_loop(world, e, b.next_sound);
    if let Some(mut b) = world.get_mut::<Bonfire>(e) {
        b.next_sound = next;
    }
}

fn bonfire_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    let Some(b) = world.get::<Bonfire>(target).cloned() else {
        return false;
    };
    if held_is(world, held, RODS) && !b.stake && !b.grill {
        let ask = Ask::new(
            client,
            mob,
            "Bonfire",
            "What would you like to construct?",
            PromptKind::Alert {
                buttons: vec!["Stake".into(), "Grill".into()],
                autofocus: true,
            },
        );
        let prompt = ask_world(world, ask);
        world.entity_mut(prompt).insert(RodAsk {
            bonfire: target,
            rods: held,
        });
        return true;
    }
    if is_hot(world, held) {
        start_burning(world, target);
        return true;
    }
    if !b.grill {
        return false;
    }
    if held_is(world, held, ROASTING_STICK) {
        return true;
    }
    let combat = world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0);
    if combat || world.get::<crate::inventory::NoDrop>(held).is_some() {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return false;
    };
    // Centered on the click, at most half a tile off.
    let offset = modifiers
        .pixel
        .map_or([0, 0], |p| p.map(|v| i16::from(v).clamp(-16, 16)));
    crate::inventory::put_down_at(world, held, pos, offset);
    crate::construction::tile_changed(world, pos);
    true
}

/// The "Stake or Grill" answer: one rod, checked again after the wait.
fn rod_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let Some(messages) = world.get_resource::<Messages<PromptAnswered>>() else {
        return;
    };
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(messages)
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in pending {
        let Some(ask) = world.get::<RodAsk>(prompt).copied() else {
            continue;
        };
        let Some(PromptAnswer::Button(choice)) = answer else {
            continue;
        };
        let carried = world
            .get::<crate::inventory::CarriedBy>(ask.rods)
            .is_some_and(|c| c.0 == mob);
        let near = world
            .get::<TilePos>(mob)
            .zip(world.get::<TilePos>(ask.bonfire))
            .and_then(|(a, b)| a.chebyshev(*b))
            .is_some_and(|d| d <= 1);
        let free = world
            .get::<Bonfire>(ask.bonfire)
            .is_some_and(|b| !b.stake && !b.grill);
        if !carried || !near || !free || !crate::tools::use_stack(world, ask.rods, 1) {
            continue;
        }
        let name = name_of(world, ask.bonfire);
        if let Some(mut b) = world.get_mut::<Bonfire>(ask.bonfire) {
            if choice == 0 {
                b.stake = true;
            } else {
                b.grill = true;
            }
        }
        let text = if choice == 0 {
            format!("Вы добавляете стержень к {name}.")
        } else {
            format!("Вы добавляете решётку к {name}.")
        };
        tell(world, client, Span::Notice, text);
        bonfire_look(world, ask.bonfire);
    }
}

/// `bonfire/attack_hand`. True when `target` was a bonfire.
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(b) = world.get::<Bonfire>(target).cloned() else {
        return false;
    };
    if b.burning {
        let name = name_of(world, target);
        tell(
            world,
            client,
            Span::Warning,
            format!("Нужно потушить {name}, прежде чем убирать брёвна!"),
        );
        return true;
    }
    let buckled = world
        .query::<&crate::medical::surgery::Buckled>()
        .iter(world)
        .any(|x| x.0 == target);
    if !buckled {
        crate::tools::do_after(
            world,
            Actor {
                client,
                mob,
                item: None,
            },
            target,
            DISMANTLE_DS,
            0,
            crate::construction::Job::Bonfire,
        );
    }
    true
}

/// The end of [`attack_hand`]'s `do_after`: the logs and a rod out, the bonfire gone.
pub fn complete_dismantle(world: &mut World, action: &DoAfter) {
    let target = action.target;
    let Some(b) = world.get::<Bonfire>(target).cloned() else {
        return;
    };
    if b.burning {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    let logs: Vec<Entity> = world
        .get::<crate::storage::Stored>(target)
        .map(|s| s.items().to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter(|&i| path_of(world, i).is_some_and(|p| is_under(p, LOG)))
        .collect();
    for log in logs {
        crate::storage::take_out(world, log, pos);
        let (x, y) = {
            let mut rng = world.resource_mut::<crate::combat::CombatRng>();
            (rng.range(1, 4) as i16, rng.range(1, 4) as i16)
        };
        if let Some(mut look) = world.get_mut::<Appearance>(log) {
            look.pixel_offset[0] += x;
            look.pixel_offset[1] += y;
        }
    }
    if b.stake || b.grill {
        crate::construction::spawn_stack(world, RODS, pos, 1);
    }
    world.despawn(target);
    crate::construction::tile_changed(world, pos);
}

// --- Shared --------------------------------------------------------------------------------

/// `item_interaction` of a fireplace or a bonfire. True when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    fireplace_interaction(world, client, mob, held, target)
        || bonfire_interaction(world, client, mob, held, target, modifiers)
}

/// `bonfire/fire_act`: fire lights it. True when `thing` is a bonfire.
pub fn fire_act(world: &mut World, thing: Entity) -> bool {
    if world.get::<Bonfire>(thing).is_none() {
        return false;
    }
    start_burning(world, thing);
    true
}

/// `/atom/extinguish` of a fireplace (its fuel kept) or a bonfire (water on it).
pub fn extinguish(world: &mut World, thing: Entity) {
    if let Some(f) = world.get::<Fireplace>(thing).cloned() {
        if f.lit {
            let fuel = f.remaining(now(world));
            put_out_fireplace(world, thing);
            if let Some(mut f) = world.get_mut::<Fireplace>(thing) {
                f.expiry = 0.0;
                f.add_fuel(fuel, 0.0);
            }
        }
        return;
    }
    put_out_bonfire(world, thing);
}

/// Every second: fireplaces and bonfires burn (`process`).
fn process(world: &mut World, mut since: Local<f32>) {
    *since += world.resource::<Time>().delta_secs();
    if *since < TICK_SECS {
        return;
    }
    *since = 0.0;
    let fires: Vec<Entity> = world
        .query_filtered::<Entity, Or<(With<Fireplace>, With<Bonfire>)>>()
        .iter(world)
        .collect();
    for e in fires {
        if world.get::<Fireplace>(e).is_some() {
            process_fireplace(world, e);
        } else {
            process_bonfire(world, e);
        }
    }
}

/// `bonfire/on_entered`: what walks into a lit bonfire without a grill burns; a burning mob or
/// item lights an unlit one.
// Cost: (steps this tick) x (things on the tile entered), a handful.
fn entered(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moves: Vec<(Entity, TilePos)> = cursor
        .read(world.resource::<Messages<Moved>>())
        .map(|m| (m.entity, m.to))
        .collect();
    for (mover, to) in moves {
        let bonfire = world
            .resource::<SpatialIndex>()
            .on_tile(to)
            .iter()
            .copied()
            .find(|&e| world.get::<Bonfire>(e).is_some());
        let Some(bonfire) = bonfire else {
            continue;
        };
        let Some(b) = world.get::<Bonfire>(bonfire).cloned() else {
            continue;
        };
        if b.burning {
            if !b.grill {
                bonfire_burn(world, bonfire, 2.0);
            }
            continue;
        }
        let mob_burning = world
            .get::<crate::fire::FireStacks>(mover)
            .is_some_and(|f| f.on_fire);
        let item_burning = world.get::<crate::fire::Burning>(mover).is_some();
        if !(mob_burning || item_burning) || !start_burning(world, bonfire) {
            continue;
        }
        let (what, name) = (name_of(world, mover), name_of(world, bonfire));
        let text = if mob_burning {
            format!("{what} пробегает по {name}, разжигая его!")
        } else {
            format!("Огонь с {what} перекидывается на {name}!")
        };
        visible(world, bonfire, "", &text, Span::Notice);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fire_levels_follow_the_dm_switch() {
        assert_eq!(fire_level(0.0), 0);
        assert_eq!(fire_level(500.0), 0);
        assert_eq!(fire_level(501.0), 1);
        assert_eq!(fire_level(1500.0), 2);
        assert_eq!(fire_level(2000.0), 3);
        assert_eq!(fire_level(3000.0), 4);
    }

    #[test]
    fn fuel_is_capped_lit_or_not() {
        let mut f = Fireplace::default();
        f.add_fuel(LOG_BURN_DS * 30.0, 0.0);
        assert_eq!(f.remaining(0.0), MAX_BURN_DS, "unlit fuel clamps");
        let mut f = Fireplace {
            lit: true,
            expiry: 10.0,
            ..default()
        };
        // 100 ds left at t = 0; a log adds 150 ds.
        f.add_fuel(LOG_BURN_DS, 0.0);
        assert!((f.remaining(0.0) - 250.0).abs() < 1e-6, "lit fuel adds up");
        f.add_fuel(MAX_BURN_DS, 0.0);
        assert!((f.remaining(0.0) - MAX_BURN_DS).abs() < 1e-6, "and caps");
    }
}
