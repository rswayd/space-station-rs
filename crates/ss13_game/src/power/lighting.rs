//! Light fixtures' bulbs (server), after tg `code/modules/power/lighting/` (`light.dm`,
//! `light_items.dm`, `light_construct.dm`, `light_wallframes.dm`) and
//! `code/game/objects/items/devices/lightreplacer.dm`.
//!
//! - A fixture's bulb is [`LightStatus`] on its [`LightFixture`]: tubes break at round start
//!   (2%, bulbs 5%), from hits (`prob(damage * 5)`), explosions, heat, the light eater; every
//!   time a fixture switches on its `switchcount` grows and it burns out at
//!   `prob(min(60, switchcount² / 100))`. [`flicker`] dims it on and off (grey tide).
//! - Bulbs and tubes as items ([`LightBulb`], `/obj/item/light`): whole, burnt or broken; they
//!   shatter when thrown, stepped on or swung at someone.
//! - Right click with an empty hand takes the bulb out (a lit one burns the hand without
//!   heat-proof gloves); a bulb of the fixture's kind goes in (swapping the old one out); a
//!   screwdriver turns an empty fixture into its frame (`light_construct`, wired), which wire
//!   cutters, a wrench, cable and a screwdriver take apart or finish; the wall frame item
//!   (`/obj/item/wallframe/light_fixture`) puts a frame on a wall.
//! - The light replacer ([`LightReplacer`]) refills from bulbs, glass sheets and shards (4
//!   shards make a bulb) and replaces the bulbs of a fixture or of every fixture on a tile.
// shortcut: no reagent-rigged bulbs, no ethereal draining, no emergency cell inside frames, no
// sticking things into empty sockets (no shock), no bluespace replacer ranged use, no floor
// light frames (`/obj/item/wallframe/light_fixture/small` attack_self), no in-hand states of
// the bulbs; a fixture's integrity is tg's 100 without armor.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::light::{DmLight, source};
use ss13_mapload::{Areas, DmAtom, DmItem, DmPower};
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;
use ss13_shared::{Appearance, Dir, Facing, LightSource, Optics, TilePos};
use ss13_world::{Moved, SpatialIndex};

use super::{AreaMembers, AreaPower, EngineRng, LightFixture, LightMode, LightProcessing};
use crate::combat::{CombatMode, CombatRng};
use crate::construction::Job;
use crate::health::{DamageType, Health, Stat, apply_damage_zone};
use crate::inventory::{CarriedBy, Inventory};
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, ToolKind, tell, tool_of};

pub const LIGHT_ITEM: &str = "/obj/item/light";
pub const TUBE: &str = "/obj/item/light/tube";
pub const BULB: &str = "/obj/item/light/bulb";
pub const FRAME: &str = "/obj/structure/light_construct";
pub const FRAME_SMALL: &str = "/obj/structure/light_construct/small";
pub const FRAME_FLOOR: &str = "/obj/structure/light_construct/floor";
pub const WALLFRAME: &str = "/obj/item/wallframe/light_fixture";
pub const WALLFRAME_SMALL: &str = "/obj/item/wallframe/light_fixture/small";
/// What a frame's screwdriver builds (`/obj/machinery/light/empty` and kin).
pub const EMPTY_TUBE: &str = "/obj/machinery/light/empty";
pub const EMPTY_SMALL: &str = "/obj/machinery/light/small/empty";
pub const EMPTY_FLOOR: &str = "/obj/machinery/light/floor/empty";
pub const REPLACER: &str = "/obj/item/lightreplacer";
const REPLACER_ADVANCED: &str = "/obj/item/lightreplacer/advanced";
const CABLE: &str = "/obj/item/stack/cable_coil";
const IRON: &str = "/obj/item/stack/sheet/iron";
const GLASS: &str = "/obj/item/stack/sheet/glass";
const SHARD: &str = "/obj/item/shard";

pub const GLASSHIT: &str = "sound/effects/glass/glasshit.ogg";
const GLASS_STEP: &str = "sound/effects/footstep/glass_step.ogg";
const SMASH: &str = "sound/items/weapons/smash.ogg";
const HIT_SHATTERED: &str = "sound/effects/hit_on_shattered_glass.ogg";
const CLICK: &str = "sound/machines/click.ogg";
const DING: &str = "sound/machines/ding.ogg";
const DECONSTRUCT: &str = "sound/items/deconstruct.ogg";
const SCREWDRIVER: &str = "sound/items/tools/screwdriver.ogg";
const WIRECUTTER: &str = "sound/items/tools/wirecutter.ogg";
const RATCHET: &str = "sound/items/tools/ratchet.ogg";

/// `/obj/machinery/light` `max_integrity`.
pub const FIXTURE_INTEGRITY: f32 = 100.0;
/// `LIGHT_CONSTRUCT_EMPTY`, `LIGHT_CONSTRUCT_WIRED`.
pub const STAGE_EMPTY: u8 = 1;
pub const STAGE_WIRED: u8 = 2;
/// `use_tool(src, user, 30)` of a frame's wrench, deciseconds.
const FRAME_WRENCH_DS: f32 = 30.0;
/// `GLASS_SHEET_USES`, `BULB_SHARDS_REQUIRED`.
const GLASS_SHEET_USES: u32 = 5;
const BULB_SHARDS_REQUIRED: u32 = 4;
/// The lit bulb's burn on bare hands, and the gloves' `max_heat_protection_temperature` that
/// stop it.
const HOT_BULB_BURN: f32 = 5.0;
const HEAT_PROOF_GLOVES: f32 = 360.0;
/// `flicker()`: the first dim step, then `pick(2, 4, 6)` seconds per toggle.
const FLICKER_FIRST_SECS: f64 = 0.7;

/// tg `LIGHT_OK`, `LIGHT_EMPTY`, `LIGHT_BROKEN`, `LIGHT_BURNED`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LightStatus {
    #[default]
    Ok,
    Empty,
    Broken,
    Burned,
}

impl LightStatus {
    pub fn from_dm(status: u8) -> Self {
        match status {
            1 => Self::Empty,
            2 => Self::Broken,
            3 => Self::Burned,
            _ => Self::Ok,
        }
    }

    /// `"[base_state]-empty"` etc.; `None` for a working bulb.
    pub fn suffix(self) -> Option<&'static str> {
        match self {
            Self::Ok => None,
            Self::Empty => Some("-empty"),
            Self::Broken => Some("-broken"),
            Self::Burned => Some("-burned"),
        }
    }
}

/// An `/obj/item/light`'s `status`, `brightness` and `switchcount`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct LightBulb {
    pub status: LightStatus,
    pub brightness: f32,
    pub switchcount: u32,
}

/// `/obj/structure/light_construct` `stage`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightFrame(pub u8);

/// `/obj/item/lightreplacer` `uses`, `max_uses`, `bulb_shards`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LightReplacer {
    pub uses: u32,
    pub max_uses: u32,
    pub shards: u32,
}

/// A running `flicker(amount)`: toggles left and when the next one comes.
#[derive(Component, Clone, Copy, Debug)]
pub struct Flicker {
    left: u32,
    next_at: f64,
}

/// tg `#ifndef MAP_TEST` around the round-start breakage: off in tests.
#[derive(Resource, Clone, Copy, Debug)]
pub struct RoundstartLightBreak(pub bool);

impl Default for RoundstartLightBreak {
    fn default() -> Self {
        Self(true)
    }
}

/// A timed light step ([`Job::Light`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightJob {
    /// Wrenching an unwired frame off the wall.
    FrameApart,
    /// Wrenching an unwired floodlight frame apart.
    FloodFrameApart,
}

pub struct LightingPlugin;

impl Plugin for LightingPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.init_resource::<RoundstartLightBreak>()
            .add_click_handler(ClickPhase::EmptyHand, 2010, |w, c| {
                c.modifiers.right && remove_by_hand(w, c.client, c.mob, c.target)
            })
            .add_click_handler(ClickPhase::HeldOnTarget, 2710, |w, c| {
                c.held
                    .is_some_and(|held| item_interaction(w, c.client, c.mob, held, c.target, c.pos))
            })
            .add_click_handler(ClickPhase::HeldOnTarget, 2715, |w, c| {
                c.held
                    .is_some_and(|held| swing_bulb(w, c.mob, held, c.target))
            })
            .add_click_handler(ClickPhase::EmptyHand, 2012, |w, c| {
                floodlight_hand(w, c.client, c.mob, c.target)
            })
            .add_systems(
                ss13_world::MapInit,
                init_floodlights.after(super::init_power),
            )
            .add_systems(
                Update,
                (floodlight_tick, floodlight_emp).after(super::PowerSystems),
            )
            .add_click_handler(ClickPhase::AttackSelf, 3192, |w, c| {
                replacer_attack_self(w, c.client, c.mob, c.target)
            })
            .add_systems(
                Update,
                (
                    run_flickers.run_if(any_with_component::<Flicker>),
                    step_on_bulbs,
                ),
            );
    }
}

/// The fixture of a map or built light (`init_power`, [`build_fixture`]).
#[allow(clippy::too_many_arguments)]
pub(crate) fn new_fixture(
    index: Option<&IconIndex>,
    dm: &DmPower,
    light: &DmLight,
    template: Option<LightSource>,
    icon: &str,
    area: Entity,
    emergency: Option<f64>,
    status: LightStatus,
) -> LightFixture {
    let state =
        |suffix: &str| index.and_then(|i| i.get(icon, &format!("{}{suffix}", dm.base_state)));
    let overlay = |suffix: &str| {
        index.and_then(|i| i.get(&dm.overlay_icon, &format!("{}{suffix}", dm.base_state)))
    };
    let path = dm.path.as_str();
    let bulb = is_under(path, "/obj/machinery/light/small")
        || is_under(path, "/obj/machinery/light/floor");
    LightFixture {
        area,
        mode: LightMode::Off,
        on_light: template.filter(|_| status == LightStatus::Ok),
        brightness: light.brightness,
        emergency,
        watts: f64::from(light.brightness * light.bulb_power * dm.power_consumption_rate as f32)
            .ceil(),
        states: [state(""), state("_emergency")],
        fire_light: template.map(|on| LightSource {
            range: light.fire_brightness,
            power: light.fire_power,
            color: light.fire_colour.map_or([255; 3], |[r, g, b, _]| [r, g, b]),
            ..on
        }),
        fire: false,
        night_light: template.map(|on| LightSource {
            range: light.nightshift_brightness,
            power: light.nightshift_light_power,
            color: light
                .nightshift_light_color
                .map_or([255; 3], |[r, g, b, _]| [r, g, b]),
            ..on
        }),
        nightshift: false,
        status,
        template,
        status_states: [state("-empty"), state("-broken"), state("-burned")],
        overlay_states: [
            overlay(""),
            overlay("_emergency"),
            overlay("_nightshift"),
            overlay("_flickering"),
        ],
        bulb,
        switchcount: 0,
        flickering: false,
        integrity: FIXTURE_INTEGRITY,
        bulb_power: light.bulb_power,
        consumption_rate: dm.power_consumption_rate as f32,
    }
}

/// tg round-start `break_light_tube(TRUE)` of `post_machine_initialize`.
pub(crate) fn roundstart_break(world: &mut World, fixture: &LightFixture) -> bool {
    if fixture.status != LightStatus::Ok
        || !world
            .get_resource::<RoundstartLightBreak>()
            .is_none_or(|c| c.0)
    {
        return false;
    }
    let chance = if fixture.bulb { 5.0 } else { 2.0 };
    world.resource_mut::<EngineRng>().0.chance(chance)
}

/// Sets the fixture's bulb, then tg `update()`: its light, load and icon follow the area.
pub fn set_status(world: &mut World, light: Entity, status: LightStatus, brightness: Option<f32>) {
    let Some(mut fixture) = world.get_mut::<LightFixture>(light) else {
        return;
    };
    fixture.status = status;
    fixture.flickering = false;
    if let Some(brightness) = brightness {
        fixture.brightness = brightness;
        fixture.watts =
            f64::from(brightness * fixture.bulb_power * fixture.consumption_rate).ceil();
        if let Some(t) = fixture.template.as_mut() {
            t.range = brightness;
        }
    }
    fixture.on_light = fixture.template.filter(|_| status == LightStatus::Ok);
    update(world, light);
}

/// tg `update(FALSE)` of one fixture: the mode its area gives it now.
pub fn update(world: &mut World, light: Entity) {
    let Some(area) = world.get::<LightFixture>(light).map(|f| f.area) else {
        return;
    };
    let Ok([mut light_e, mut area_e]) = world.get_entity_mut([light, area]) else {
        return;
    };
    let Some(mut area_power) = area_e.get_mut::<AreaPower>() else {
        return;
    };
    let Ok((mut fixture, optics, appearance)) = light_e.get_components_mut::<(
        &mut LightFixture,
        Option<&mut Optics>,
        Option<&mut Appearance>,
    )>() else {
        return;
    };
    let mode = if fixture.on_light.is_some() {
        super::model::light_mode(&area_power, fixture.emergency, super::MACHINE_TICK_SECS)
    } else {
        LightMode::Off
    };
    super::set_mode(&mut fixture, mode, &mut area_power, optics, appearance);
    let processing = mode == LightMode::Emergency
        || (mode == LightMode::On
            && fixture
                .emergency
                .is_some_and(|c| c < super::EMERGENCY_CELL_CHARGE));
    if processing {
        world.entity_mut(light).insert(LightProcessing);
    } else {
        world.entity_mut(light).remove::<LightProcessing>();
    }
}

/// tg `break_light_tube(skip_sound_and_sparks)`: glass sound if the bulb was whole, sparks if
/// it was lit. False when there was nothing to break.
pub fn break_tube(world: &mut World, light: Entity, quiet: bool) -> bool {
    let Some(fixture) = world.get::<LightFixture>(light) else {
        return false;
    };
    if matches!(fixture.status, LightStatus::Empty | LightStatus::Broken) {
        return false;
    }
    let on = fixture.mode == LightMode::On;
    set_status(world, light, LightStatus::Broken, None);
    if !quiet && let Some(&pos) = world.get::<TilePos>(light) {
        // Whole or burnt (`was_ok`, the only bulbs that get here): the glass sound.
        playsound(world, PlaySound::new(pos, GLASSHIT, 75.0, true));
        if on {
            crate::effects::particles::do_sparks(world, 3, true, pos);
        }
    }
    true
}

/// tg `burn_out()` of a working bulb.
pub fn burn_out(world: &mut World, light: Entity) -> bool {
    if world
        .get::<LightFixture>(light)
        .is_none_or(|f| f.status != LightStatus::Ok)
    {
        return false;
    }
    set_status(world, light, LightStatus::Burned, None);
    true
}

/// tg `take_damage` of a fixture: `prob(damage * 5)` breaks the tube; at 0 integrity it comes
/// apart (`on_deconstruction(FALSE)`).
pub fn take_damage(world: &mut World, light: Entity, damage: f32) {
    let Some(mut fixture) = world.get_mut::<LightFixture>(light) else {
        return;
    };
    if damage <= 0.0 {
        return;
    }
    fixture.integrity = (fixture.integrity - damage).max(0.0);
    let destroyed = fixture.integrity <= 0.0;
    if world.resource_mut::<CombatRng>().chance(damage * 5.0) {
        break_tube(world, light, false);
    }
    if destroyed {
        deconstruct(world, light, false);
    }
}

/// tg `flicker(amount)`: a lit working fixture dims and comes back `amount` times. False when it
/// cannot flicker now.
pub fn flicker(world: &mut World, light: Entity, amount: u32) -> bool {
    let Some(fixture) = world.get::<LightFixture>(light) else {
        return false;
    };
    if world.get::<Flicker>(light).is_some()
        || fixture.mode != LightMode::On
        || fixture.status != LightStatus::Ok
    {
        return false;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(light).insert(Flicker {
        left: amount,
        next_at: now + FLICKER_FIRST_SECS,
    });
    set_flickering(world, light, true);
    true
}

fn set_flickering(world: &mut World, light: Entity, on: bool) {
    let Ok(mut e) = world.get_entity_mut(light) else {
        return;
    };
    let Ok((mut fixture, optics, appearance)) = e.get_components_mut::<(
        &mut LightFixture,
        Option<&mut Optics>,
        Option<&mut Appearance>,
    )>() else {
        return;
    };
    fixture.flickering = on;
    super::refresh_light(&fixture, optics, appearance);
}

/// The flicker loop of `flicker()`: after the first dim step, 30% sparks; then a toggle every
/// 2, 4 or 6 seconds while the bulb works and has power; at the end it is lit as `update()`
/// says.
fn run_flickers(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &Flicker)>();
    let due: Vec<(Entity, Flicker)> = query
        .iter(world)
        .filter(|(_, f)| f.next_at <= now)
        .map(|(e, f)| (e, *f))
        .collect();
    for (light, flick) in due {
        let Some(fixture) = world.get::<LightFixture>(light) else {
            world.entity_mut(light).remove::<Flicker>();
            continue;
        };
        let working = fixture.status == LightStatus::Ok && fixture.mode == LightMode::On;
        let dim = fixture.flickering;
        if flick.left == 0 || !working {
            world.entity_mut(light).remove::<Flicker>();
            set_flickering(world, light, false);
            continue;
        }
        let (sparks, wait) = {
            let mut rng = world.resource_mut::<EngineRng>();
            (
                rng.0.chance(30.0),
                [2.0, 4.0, 6.0][rng.0.range(0, 2) as usize],
            )
        };
        if sparks && let Some(&pos) = world.get::<TilePos>(light) {
            crate::effects::particles::do_sparks(world, 2, true, pos);
        }
        world.entity_mut(light).insert(Flicker {
            left: flick.left - 1,
            next_at: now + wait,
        });
        set_flickering(world, light, !dim);
    }
}

/// `COMSIG_GLOB_GREY_TIDE_LIGHT`: the fixtures of areas under `roots` flicker.
pub fn grey_tide_flicker(world: &mut World, roots: &[&str]) {
    let mut query = world.query::<(Entity, &LightFixture)>();
    let lights: Vec<(Entity, Entity)> = query.iter(world).map(|(e, f)| (e, f.area)).collect();
    for (light, area) in lights {
        let hit = world
            .get::<DmPower>(area)
            .is_some_and(|p| roots.iter().any(|r| is_under(&p.path, r)));
        if hit {
            flicker(world, light, 1);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Bulbs as items

/// The bulb state of an `/obj/item/light` (its type's when never changed).
pub fn bulb_of(world: &World, item: Entity) -> Option<LightBulb> {
    if let Some(bulb) = world.get::<LightBulb>(item) {
        return Some(*bulb);
    }
    let path = &world.get::<DmItem>(item)?.path;
    if !is_under(path, LIGHT_ITEM) {
        return None;
    }
    let brightness = if is_under(path, TUBE) {
        8.0
    } else if is_under(path, BULB) {
        4.0
    } else {
        2.0
    };
    let status = if path.ends_with("/broken") {
        LightStatus::Broken
    } else {
        LightStatus::Ok
    };
    Some(LightBulb {
        status,
        brightness,
        switchcount: 0,
    })
}

fn is_tube(world: &World, item: Entity) -> bool {
    world
        .get::<DmItem>(item)
        .is_some_and(|d| is_under(&d.path, TUBE))
}

/// Writes the bulb's state and its `update_icon_state` (`ltube`, `ltube-burned`, `ltube-broken`).
pub fn set_bulb(world: &mut World, item: Entity, bulb: LightBulb) {
    world.entity_mut(item).insert(bulb);
    let base = if is_tube(world, item) {
        "ltube"
    } else {
        "lbulb"
    };
    let state = match bulb.status {
        LightStatus::Burned => format!("{base}-burned"),
        LightStatus::Broken => format!("{base}-broken"),
        _ => base.to_owned(),
    };
    crate::obj_damage::set_icon_state(world, item, &state);
}

/// `/obj/item/light/proc/shatter()`: a whole or burnt bulb breaks. False when it did not.
pub fn shatter(world: &mut World, item: Entity) -> bool {
    let Some(mut bulb) = bulb_of(world, item) else {
        return false;
    };
    if !matches!(bulb.status, LightStatus::Ok | LightStatus::Burned) {
        return false;
    }
    bulb.status = LightStatus::Broken;
    set_bulb(world, item, bulb);
    let pos = world.get::<TilePos>(item).copied().or_else(|| {
        let holder = world.get::<CarriedBy>(item)?.0;
        world.get::<TilePos>(holder).copied()
    });
    if let Some(pos) = pos {
        let name = crate::examine::name_of(world, item);
        for client in crate::construction::clients_near(world, pos, 7, &[]) {
            tell(world, client, Span::Danger, format!("{name} разбивается."));
        }
        playsound(world, PlaySound::new(pos, GLASSHIT, 75.0, true));
    }
    true
}

/// `/obj/item/light/throw_impact`: a bulb not caught shatters ([`crate::throw`]).
pub fn thrown(world: &mut World, item: Entity) {
    if world.get_entity(item).is_ok() && world.get::<CarriedBy>(item).is_none() {
        shatter(world, item);
    }
}

/// `/obj/item/light/attack`: a bulb swung at a mob in combat mode shatters on it; the attack
/// goes on.
fn swing_bulb(world: &mut World, mob: Entity, held: Entity, target: Entity) -> bool {
    if world.get::<Health>(target).is_some()
        && world.get::<CombatMode>(mob).is_some_and(|c| c.0)
        && bulb_of(world, held).is_some()
    {
        shatter(world, held);
    }
    false
}

/// `/obj/item/light/on_entered`: a living mob walking onto a bulb crunches it.
// Cost: per step of a living mob, one look at the tile's contents.
fn step_on_bulbs(world: &mut World, mut cursor: Local<MessageCursor<Moved>>) {
    let moved: Vec<Moved> = cursor
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    for m in moved {
        if world.get::<Health>(m.entity).is_none() {
            continue;
        }
        let bulbs: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(m.to)
            .iter()
            .copied()
            .filter(|&e| {
                world
                    .get::<DmItem>(e)
                    .is_some_and(|d| is_under(&d.path, LIGHT_ITEM))
            })
            .collect();
        for bulb in bulbs {
            playsound(world, PlaySound::new(m.to, GLASS_STEP, 50.0, true));
            shatter(world, bulb);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Taking bulbs out and in, frames

fn fitting(fixture: &LightFixture) -> &'static str {
    if fixture.bulb {
        "лампочка"
    } else {
        "трубка"
    }
}

/// tg `drop_light_tube(user)`: the fixture's bulb as an item, in the user's hands or on the
/// fixture's tile; the fixture is empty.
pub fn drop_light_tube(world: &mut World, light: Entity, user: Option<Entity>) -> Option<Entity> {
    let fixture = world.get::<LightFixture>(light)?.clone();
    let &pos = world.get::<TilePos>(light)?;
    if fixture.status == LightStatus::Empty {
        return None;
    }
    let path = if fixture.bulb { BULB } else { TUBE };
    let item = crate::construction::spawn_obj(world, path, pos);
    if let Some(item) = item {
        set_bulb(
            world,
            item,
            LightBulb {
                status: fixture.status,
                brightness: fixture.brightness,
                switchcount: fixture.switchcount,
            },
        );
        if let Some(mob) = user
            && let Some(inventory) = world.get::<Inventory>(mob).cloned()
        {
            crate::storage::put_in_hands(world, mob, &inventory, item);
        }
    }
    if let Some(mut f) = world.get_mut::<LightFixture>(light) {
        f.switchcount = 0;
    }
    set_status(world, light, LightStatus::Empty, None);
    item
}

/// tg `attack_hand_secondary`: the bulb out by hand; a lit one burns an unprotected hand (5
/// burn) and stays in.
fn remove_by_hand(world: &mut World, client: Entity, mob: Entity, light: Entity) -> bool {
    let Some(fixture) = world.get::<LightFixture>(light).cloned() else {
        return false;
    };
    let fit = fitting(&fixture);
    if fixture.status == LightStatus::Empty {
        tell(
            world,
            client,
            Span::Warning,
            format!("В светильнике нет: {fit}!"),
        );
        return true;
    }
    let lit = fixture.mode != LightMode::Off;
    if lit && !heat_proof_hands(world, mob) {
        if let Some(zone) = crate::martial::active_arm(world, mob)
            && let Ok(mut e) = world.get_entity_mut(mob)
            && let Ok((mut health, mut stat)) = e.get_components_mut::<(&mut Health, &mut Stat)>()
        {
            apply_damage_zone(
                &mut health,
                &mut stat,
                zone,
                DamageType::Burn,
                HOT_BULB_BURN,
            );
        }
        tell(
            world,
            client,
            Span::Warning,
            "Вы пытаетесь вынуть лампу, но обжигаете об неё руку!",
        );
        return true;
    }
    tell(world, client, Span::Notice, format!("Вы вынимаете: {fit}."));
    drop_light_tube(world, light, Some(mob));
    true
}

/// Gloves with `max_heat_protection_temperature` over 360 K.
fn heat_proof_hands(world: &World, mob: Entity) -> bool {
    world
        .get::<Inventory>(mob)
        .and_then(|i| i.slot(InventorySlot::Gloves))
        .and_then(|g| world.get::<ss13_mapload::DmClothing>(g))
        .is_some_and(|c| c.max_heat_protection_temperature > HEAT_PROOF_GLOVES)
}

fn frame_kind(path: &str) -> &'static str {
    if is_under(path, FRAME_SMALL) {
        "bulb"
    } else if is_under(path, FRAME_FLOOR) {
        "floor"
    } else {
        "tube"
    }
}

fn frame_stage(world: &World, frame: Entity) -> u8 {
    world.get::<LightFrame>(frame).map_or(STAGE_EMPTY, |f| f.0)
}

fn set_frame_stage(world: &mut World, frame: Entity, stage: u8) {
    world.entity_mut(frame).insert(LightFrame(stage));
    let path = crate::emp::path_of(world, frame).unwrap_or_default();
    let state = format!("{}-construct-stage{stage}", frame_kind(&path));
    crate::obj_damage::set_icon_state(world, frame, &state);
}

/// The held item on a fixture, a frame, a wall (the frame item) or anything (the replacer).
fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    pos: TilePos,
) -> bool {
    let held_path = crate::emp::path_of(world, held).unwrap_or_default();
    if is_under(&held_path, REPLACER) {
        return replacer_on(world, client, mob, held, target, pos);
    }
    if world.get::<LightFixture>(target).is_some() {
        return fixture_interaction(world, client, mob, held, target);
    }
    if let Some(replacer) = world.get::<LightReplacer>(target).copied().or_else(|| {
        crate::emp::path_of(world, target)
            .filter(|p| is_under(p, REPLACER))
            .map(|p| new_replacer(&p))
    }) {
        return refill(world, client, held, target, replacer);
    }
    let target_path = crate::emp::path_of(world, target).unwrap_or_default();
    if world.get::<Floodlight>(target).is_some() {
        return floodlight_tool(world, client, held, target);
    }
    if is_under(&target_path, FLOOD_FRAME) {
        return flood_frame_interaction(world, client, mob, held, target);
    }
    if is_under(&target_path, FRAME) {
        return frame_interaction(world, client, mob, held, target, &target_path);
    }
    if is_under(&held_path, WALLFRAME) && !is_under(&held_path, WALLFRAME_SMALL) {
        return place_frame(world, client, mob, held, target);
    }
    false
}

/// `/obj/machinery/light/item_interaction` and `screwdriver_act`, and `attacked_by` in combat
/// mode.
fn fixture_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    light: Entity,
) -> bool {
    let Some(fixture) = world.get::<LightFixture>(light).cloned() else {
        return false;
    };
    let fit = fitting(&fixture);
    let combat = world.get::<CombatMode>(mob).is_some_and(|c| c.0);
    if let Some(bulb) = bulb_of(world, held) {
        if fixture.status == LightStatus::Ok {
            tell(
                world,
                client,
                Span::Warning,
                format!("Здесь уже стоит {fit}!"),
            );
            return true;
        }
        if is_tube(world, held) == fixture.bulb {
            tell(
                world,
                client,
                Span::Warning,
                format!("Этому светильнику нужна {fit}!"),
            );
            return true;
        }
        if fixture.status == LightStatus::Empty {
            tell(world, client, Span::Notice, "Вы вставляете лампу.");
        } else {
            drop_light_tube(world, light, Some(mob));
            tell(world, client, Span::Notice, "Вы заменяете лампу.");
        }
        world.despawn(held);
        if let Some(mut f) = world.get_mut::<LightFixture>(light) {
            f.switchcount = bulb.switchcount;
        }
        set_status(world, light, bulb.status, Some(bulb.brightness));
        return true;
    }
    if let Some((ToolKind::Screwdriver, _)) = tool_of(world, held)
        && fixture.status == LightStatus::Empty
        && !combat
    {
        if let Some(&pos) = world.get::<TilePos>(light) {
            playsound(world, PlaySound::new(pos, SCREWDRIVER, 75.0, true));
        }
        tell(
            world,
            client,
            Span::Notice,
            "Вы открываете корпус светильника.",
        );
        deconstruct(world, light, true);
        return true;
    }
    if !combat {
        return false;
    }
    // `attacked_by` -> `take_damage(force)` with the fixture's attack sound.
    let force = world.get::<DmItem>(held).map_or(0.0, |d| d.force);
    if let Some(&pos) = world.get::<TilePos>(light) {
        let sound = match fixture.status {
            LightStatus::Empty => SMASH,
            LightStatus::Broken => HIT_SHATTERED,
            _ => GLASSHIT,
        };
        playsound(world, PlaySound::new(pos, sound, 90.0, true));
    }
    take_damage(world, light, force);
    true
}

/// tg `on_deconstruction(disassembled)`: the fixture becomes its wired frame; torn down by
/// force the bulb breaks and drops and a cable piece falls out.
pub fn deconstruct(world: &mut World, light: Entity, disassembled: bool) {
    let Some(fixture) = world.get::<LightFixture>(light).cloned() else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(light) else {
        return;
    };
    let path = world
        .get::<DmPower>(light)
        .map(|d| d.path.clone())
        .unwrap_or_default();
    let frame_path = if is_under(&path, "/obj/machinery/light/floor") {
        FRAME_FLOOR
    } else if fixture.bulb {
        FRAME_SMALL
    } else {
        FRAME
    };
    let facing = world.get::<Facing>(light).copied();
    if !disassembled {
        break_tube(world, light, false);
        drop_light_tube(world, light, None);
        crate::construction::spawn_stack(world, CABLE, pos, 1);
    }
    // Off: its load leaves the area.
    set_status(world, light, LightStatus::Empty, None);
    if let Some(mut members) = world.get_mut::<AreaMembers>(fixture.area) {
        members.lights.retain(|&l| l != light);
    }
    world.despawn(light);
    if let Some(frame) = crate::construction::spawn_obj(world, frame_path, pos) {
        if let Some(facing) = facing {
            world.entity_mut(frame).insert(facing);
        }
        set_frame_stage(world, frame, STAGE_WIRED);
    }
    crate::construction::tile_changed(world, pos);
}

/// `/obj/structure/light_construct/item_interaction`, `screwdriver_act`, `wirecutter_act`,
/// `wrench_act`.
fn frame_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    frame: Entity,
    path: &str,
) -> bool {
    let stage = frame_stage(world, frame);
    let Some(&pos) = world.get::<TilePos>(frame) else {
        return false;
    };
    if bulb_of(world, held).is_some() {
        tell(
            world,
            client,
            Span::Warning,
            "Каркас светильника ещё не собран!",
        );
        return true;
    }
    let held_path = crate::emp::path_of(world, held).unwrap_or_default();
    if stage == STAGE_EMPTY && is_under(&held_path, CABLE) {
        if !crate::tools::use_stack(world, held, 1) {
            tell(world, client, Span::Warning, "Нужен один кусок кабеля!");
            return true;
        }
        set_frame_stage(world, frame, STAGE_WIRED);
        tell(
            world,
            client,
            Span::Notice,
            "Вы проводите проводку в каркас.",
        );
        return true;
    }
    match (tool_of(world, held).map(|t| t.0), stage) {
        (Some(ToolKind::Screwdriver), STAGE_WIRED) => {
            playsound(world, PlaySound::new(pos, SCREWDRIVER, 75.0, true));
            tell(world, client, Span::Notice, "Вы закрываете корпус каркаса.");
            let built = match frame_kind(path) {
                "bulb" => EMPTY_SMALL,
                "floor" => EMPTY_FLOOR,
                _ => EMPTY_TUBE,
            };
            let facing = world.get::<Facing>(frame).copied();
            world.despawn(frame);
            build_fixture(world, built, pos, facing.map(|f| f.0));
            true
        }
        (Some(ToolKind::Wirecutters), STAGE_WIRED) => {
            set_frame_stage(world, frame, STAGE_EMPTY);
            crate::construction::spawn_stack(world, CABLE, pos, 1);
            playsound(world, PlaySound::new(pos, WIRECUTTER, 100.0, true));
            tell(
                world,
                client,
                Span::Notice,
                "Вы вынимаете проводку из каркаса.",
            );
            true
        }
        (Some(ToolKind::Wrench), STAGE_WIRED) => {
            tell(world, client, Span::Warning, "Сначала уберите провода!");
            true
        }
        (Some(ToolKind::Wrench), _) => {
            tell(
                world,
                client,
                Span::Notice,
                "Вы начинаете разбирать каркас...",
            );
            playsound(world, PlaySound::new(pos, RATCHET, 50.0, true));
            crate::tools::use_tool(
                world,
                Actor {
                    client,
                    mob,
                    item: Some(held),
                },
                frame,
                FRAME_WRENCH_DS,
                0,
                Job::Light(LightJob::FrameApart),
            );
            true
        }
        _ => false,
    }
}

/// [`Job::Light`] at its end (`extra_checks`: the frame is still unwired).
pub fn complete(world: &mut World, action: &crate::tools::DoAfter, job: LightJob) {
    if job == LightJob::FloodFrameApart {
        flood_frame_apart(world, action);
        return;
    }
    let frame = action.target;
    if frame_stage(world, frame) != STAGE_EMPTY {
        return;
    }
    let path = crate::emp::path_of(world, frame).unwrap_or_default();
    let Some(&pos) = world.get::<TilePos>(frame) else {
        return;
    };
    // `atom_deconstruct`: `sheets_refunded`.
    let sheets = if frame_kind(&path) == "tube" { 2 } else { 1 };
    world.despawn(frame);
    crate::construction::spawn_stack(world, IRON, pos, sheets);
    playsound(world, PlaySound::new(pos, DECONSTRUCT, 75.0, true));
    tell(world, action.client, Span::Notice, "Вы разбираете каркас.");
    crate::construction::tile_changed(world, pos);
}

/// `/obj/item/wallframe/try_build` + `attach` of the fixture frame: on a wall next to the user
/// in a cardinal direction, a frame on the user's tile facing the wall.
fn place_frame(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let wall = crate::construction::path_of(world, target).unwrap_or_default();
    if !is_under(&wall, "/turf/closed/wall") {
        return false;
    }
    let (Some(&wall), Some(&at)) = (world.get::<TilePos>(target), world.get::<TilePos>(mob)) else {
        return false;
    };
    let Some(dir) = crate::pull::dir_between(at, wall).filter(|d| Dir::CARDINALS.contains(d))
    else {
        tell(
            world,
            client,
            Span::Warning,
            "Нужно стоять прямо перед стеной!",
        );
        return true;
    };
    let area = world.resource::<Areas>().area_at(at);
    let unpowered = area
        .and_then(|a| world.get::<AreaPower>(a))
        .is_some_and(|p| p.always_unpowered);
    if unpowered {
        tell(
            world,
            client,
            Span::Warning,
            "Здесь нельзя установить светильник!",
        );
        return true;
    }
    let Some(frame) = crate::construction::spawn_obj(world, FRAME, at) else {
        return true;
    };
    world.despawn(held);
    world.entity_mut(frame).insert(Facing(dir));
    set_frame_stage(world, frame, STAGE_EMPTY);
    tell(
        world,
        client,
        Span::Notice,
        "Вы закрепляете каркас светильника на стене.",
    );
    true
}

/// A new fixture of `path` on `pos` facing `dir` (towards its wall): tg `new path(loc)` with
/// `setDir`, registered with its area like a map light.
pub fn build_fixture(
    world: &mut World,
    path: &str,
    pos: TilePos,
    dir: Option<Dir>,
) -> Option<Entity> {
    let light = crate::construction::spawn_obj(world, path, pos)?;
    let dir = dir.unwrap_or(Dir::South);
    world.entity_mut(light).insert(Facing(dir));
    let area = world.resource::<Areas>().area_at(pos)?;
    let dm = world.get::<DmPower>(light)?.clone();
    let dm_light = world.get::<DmLight>(light).cloned().unwrap_or_default();
    let (icon, pixel, color) = world.get::<DmAtom>(light).map_or_else(
        || (String::new(), [0, 0], None),
        |a| (a.icon.clone(), [a.pixel_x, a.pixel_y], a.color),
    );
    let mut template = source(
        path,
        &DmLight {
            status: 0,
            ..dm_light.clone()
        },
        dir as u8,
        pixel,
    );
    if let Some(t) = template.as_mut() {
        ss13_mapload::light::tint_fixture(path, t, color);
    }
    let status = LightStatus::from_dm(dm_light.status);
    let night = world
        .get::<AreaPower>(area)
        .and_then(|a| a.apc)
        .and_then(|apc| world.get::<super::Apc>(apc))
        .is_some_and(|a| a.nightshift_lights);
    let mut fixture = {
        let index = world.get_resource::<IconIndex>();
        new_fixture(index, &dm, &dm_light, template, &icon, area, None, status)
    };
    // `/obj/machinery/light/Initialize(mapload = FALSE)`: night shift follows the APC.
    fixture.nightshift = night;
    fixture.fire = world.get::<AreaPower>(area).is_some_and(|a| a.fire);
    if dm.start_with_cell && !dm.no_low_power {
        fixture.emergency = Some(super::EMERGENCY_CELL_CHARGE);
    }
    world.entity_mut(light).insert(fixture);
    if let Some(mut optics) = world.get_mut::<Optics>(light) {
        optics.light = None;
    }
    if let Some(mut members) = world.get_mut::<AreaMembers>(area) {
        members.lights.push(light);
    }
    update(world, light);
    Some(light)
}

// ---------------------------------------------------------------------------------------------
// Light replacer

fn new_replacer(path: &str) -> LightReplacer {
    LightReplacer {
        uses: 10,
        max_uses: if is_under(path, REPLACER_ADVANCED) {
            50
        } else {
            20
        },
        shards: 0,
    }
}

fn replacer_of(world: &World, item: Entity) -> Option<LightReplacer> {
    if let Some(r) = world.get::<LightReplacer>(item) {
        return Some(*r);
    }
    crate::emp::path_of(world, item)
        .filter(|p| is_under(p, REPLACER))
        .map(|p| new_replacer(&p))
}

fn status_string(r: &LightReplacer) -> String {
    format!(
        "Осталось ламп: {} (и {}/{BULB_SHARDS_REQUIRED} осколков).",
        r.uses, r.shards
    )
}

/// `add_shard`: every 4 shards make a bulb. True when one was made.
fn add_shard(world: &mut World, client: Entity, replacer: Entity, r: &mut LightReplacer) -> bool {
    r.shards += 1;
    if r.shards < BULB_SHARDS_REQUIRED {
        return false;
    }
    r.shards = 0;
    r.uses = (r.uses + 1).min(r.max_uses);
    let text = format!(
        "Лампозаменитель изготавливает новую лампу из накопленного стекла. {}",
        status_string(r)
    );
    tell(world, client, Span::Notice, text);
    if let Some(pos) = holder_pos(world, replacer) {
        playsound(world, PlaySound::new(pos, DING, 50.0, true));
    }
    true
}

fn holder_pos(world: &World, item: Entity) -> Option<TilePos> {
    world.get::<TilePos>(item).copied().or_else(|| {
        let holder = world.get::<CarriedBy>(item)?.0;
        world.get::<TilePos>(holder).copied()
    })
}

/// `/obj/item/lightreplacer/item_interaction`: bulbs, glass sheets and plain shards in.
fn refill(
    world: &mut World,
    client: Entity,
    held: Entity,
    replacer: Entity,
    mut r: LightReplacer,
) -> bool {
    let held_path = crate::emp::path_of(world, held).unwrap_or_default();
    let usable =
        is_under(&held_path, GLASS) || held_path == SHARD || bulb_of(world, held).is_some();
    if !usable {
        return false;
    }
    if r.uses >= r.max_uses {
        tell(world, client, Span::Warning, "Лампозаменитель уже полон!");
        return true;
    }
    if is_under(&held_path, GLASS) {
        if !crate::tools::use_stack(world, held, 1) {
            tell(world, client, Span::Warning, "Нужен лист стекла!");
            return true;
        }
        r.uses = (r.uses + GLASS_SHEET_USES).min(r.max_uses);
        tell(world, client, Span::Notice, "Стекло загружено.");
    } else if held_path == SHARD {
        world.despawn(held);
        if !add_shard(world, client, replacer, &mut r) {
            tell(world, client, Span::Notice, "Осколок загружен.");
        }
    } else if let Some(bulb) = bulb_of(world, held) {
        world.despawn(held);
        let made = if bulb.status == LightStatus::Ok {
            r.uses = (r.uses + 1).min(r.max_uses);
            false
        } else {
            add_shard(world, client, replacer, &mut r)
        };
        if !made {
            tell(world, client, Span::Notice, "Лампа загружена.");
        }
    }
    world.entity_mut(replacer).insert(r);
    true
}

/// `do_action`: a fixture directly, or every fixture on a clicked turf.
fn replacer_on(
    world: &mut World,
    client: Entity,
    mob: Entity,
    replacer: Entity,
    target: Entity,
    pos: TilePos,
) -> bool {
    if world.get::<LightFixture>(target).is_some() {
        replace_light(world, client, mob, replacer, target);
        return true;
    }
    // `do_action` on a floodlight frame waiting for its tube: one use finishes it.
    if crate::emp::path_of(world, target).is_some_and(|p| is_under(&p, FLOOD_FRAME)) {
        if flood_stage(world, target) == FLOOD_NEEDS_LIGHTS {
            let Some(mut r) = replacer_of(world, replacer) else {
                return true;
            };
            if r.uses == 0 {
                tell(world, client, Span::Warning, "Лампозаменитель пуст!");
                return true;
            }
            r.uses -= 1;
            world.entity_mut(replacer).insert(r);
            playsound(world, PlaySound::new(pos, CLICK, 50.0, true));
            finish_floodlight(world, target);
            tell(
                world,
                client,
                Span::Notice,
                "Вы завершаете прожектор трубкой.",
            );
        }
        return true;
    }
    if world.get::<ss13_mapload::Turf>(target).is_none() {
        return false;
    }
    let lights: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| world.get::<LightFixture>(e).is_some())
        .collect();
    for light in lights {
        replace_light(world, client, mob, replacer, light);
    }
    true
}

/// `attack_self`: the fixtures on the user's own tile, else the status line.
fn replacer_attack_self(world: &mut World, client: Entity, mob: Entity, replacer: Entity) -> bool {
    let Some(r) = replacer_of(world, replacer) else {
        return false;
    };
    let Some(&at) = world.get::<TilePos>(mob) else {
        return false;
    };
    let lights: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(at)
        .iter()
        .copied()
        .filter(|&e| world.get::<LightFixture>(e).is_some())
        .collect();
    if lights.is_empty() {
        tell(world, client, Span::Notice, status_string(&r));
    }
    for light in lights {
        replace_light(world, client, mob, replacer, light);
    }
    true
}

/// `replace_light`: a fresh bulb of the fixture's kind for one use; a broken or burnt one goes
/// in as a shard.
fn replace_light(world: &mut World, client: Entity, mob: Entity, replacer: Entity, light: Entity) {
    let _ = mob;
    let (Some(mut r), Some(fixture)) = (
        replacer_of(world, replacer),
        world.get::<LightFixture>(light).cloned(),
    ) else {
        return;
    };
    if fixture.status == LightStatus::Ok {
        tell(world, client, Span::Warning, "Лампа уже установлена!");
        return;
    }
    if r.uses == 0 {
        tell(world, client, Span::Warning, "Лампозаменитель пуст!");
        return;
    }
    r.uses -= 1;
    if let Some(pos) = holder_pos(world, replacer) {
        playsound(world, PlaySound::new(pos, CLICK, 50.0, true));
    }
    if fixture.status != LightStatus::Empty {
        add_shard(world, client, replacer, &mut r);
    }
    world.entity_mut(replacer).insert(r);
    if let Some(mut f) = world.get_mut::<LightFixture>(light) {
        f.switchcount = 0;
    }
    let brightness = if fixture.bulb { 4.0 } else { 8.0 };
    set_status(world, light, LightStatus::Ok, Some(brightness));
}

// ---------------------------------------------------------------------------------------------
// Floodlight (`code/modules/power/floodlight.dm`)
// shortcut: wrenching a floodlight is instant (tg `default_unfasten_wrench`, 2 s); no spray
// paint colour, no cable layer tool, no damage or `integrity_failure` breaking (only EMPs and
// saboteurs break it here: the EMP part).

pub const FLOODLIGHT: &str = "/obj/machinery/power/floodlight";
pub const FLOOD_FRAME: &str = "/obj/structure/floodlight_frame";
/// `FLOODLIGHT_NEEDS_WIRES`, `FLOODLIGHT_NEEDS_LIGHTS`, `FLOODLIGHT_NEEDS_SECURING`.
pub const FLOOD_NEEDS_WIRES: u8 = 0;
pub const FLOOD_NEEDS_LIGHTS: u8 = 1;
pub const FLOOD_NEEDS_SECURING: u8 = 2;
/// `light_setting_list` (index `setting - 1`), `light_power_coefficient`, `light_power`.
const FLOOD_RANGES: [f32; 4] = [0.0, 5.0, 10.0, 15.0];
const FLOOD_POWER_COEFF: f64 = 200.0;
const FLOOD_LIGHT_POWER: f32 = 1.75;
/// `coil.use(5)`, `new /obj/item/stack/sheet/iron(loc, 5)`.
const FLOOD_CABLE: u32 = 5;
const FLOOD_IRON: u32 = 5;
const GLASSBR3: &str = "sound/effects/glass/glassbr3.ogg";

/// `/obj/machinery/power/floodlight`: `setting` 1 (off) to 4 (high), `panel_open`, its cable
/// layer.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Floodlight {
    pub setting: u8,
    pub panel_open: bool,
    pub layer: u8,
}

impl Floodlight {
    /// `active_power_usage` of a setting, watts.
    pub fn watts(setting: u8) -> f64 {
        f64::from(FLOOD_RANGES[usize::from(setting.clamp(1, 4) - 1)]) * FLOOD_POWER_COEFF
    }
}

/// `/obj/structure/floodlight_frame` `state`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FloodFrame(pub u8);

fn flood_stage(world: &World, frame: Entity) -> u8 {
    if let Some(f) = world.get::<FloodFrame>(frame) {
        return f.0;
    }
    // `/obj/structure/floodlight_frame/completed`.
    let completed = crate::emp::path_of(world, frame)
        .is_some_and(|p| is_under(&p, "/obj/structure/floodlight_frame/completed"));
    if completed {
        FLOOD_NEEDS_LIGHTS
    } else {
        FLOOD_NEEDS_WIRES
    }
}

fn set_flood_stage(world: &mut World, frame: Entity, stage: u8) {
    world.entity_mut(frame).insert(FloodFrame(stage));
    let state = match stage {
        FLOOD_NEEDS_SECURING => "floodlight_c2",
        FLOOD_NEEDS_LIGHTS => "floodlight_c3",
        _ => "floodlight_c1",
    };
    crate::obj_damage::set_icon_state(world, frame, state);
}

/// Map floodlights: `if(mapload) set_anchored(TRUE); connect_to_network()`.
fn init_floodlights(world: &mut World) {
    let mut query = world
        .query_filtered::<(Entity, &ss13_mapload::DmObj, &TilePos), With<ss13_world::Mapped>>();
    let found: Vec<(Entity, TilePos)> = query
        .iter(world)
        .filter(|(_, o, _)| is_under(&o.path, FLOODLIGHT))
        .map(|(e, _, &p)| (e, p))
        .collect();
    for (e, pos) in found {
        let layer = world.get::<DmPower>(e).map_or(2, super::layer);
        world.entity_mut(e).insert(Floodlight {
            setting: 1,
            panel_open: false,
            layer,
        });
        crate::obj_damage::set_anchored(world, e, true);
        world
            .resource_mut::<super::Grid>()
            .add_node(e, pos, layer, super::NodeKind::Machine);
    }
}

/// tg `change_setting(newval)`: steps down while the net cannot feed it, then the light and
/// icon state. Returns the setting it ended on.
pub fn change_setting(world: &mut World, flood: Entity, setting: u8) -> u8 {
    let net_surplus = {
        let grid = world.resource::<super::Grid>();
        grid.node_net(flood)
            .and_then(|n| grid.net(n))
            .map_or(0.0, |n| n.surplus())
    };
    let mut setting = setting.clamp(1, 4);
    while setting > 1 && net_surplus < Floodlight::watts(setting) * super::MACHINE_TICK_SECS {
        setting -= 1;
    }
    let Some(mut f) = world.get_mut::<Floodlight>(flood) else {
        return 1;
    };
    f.setting = setting;
    let color = world
        .get::<DmAtom>(flood)
        .and_then(|a| a.color)
        .map_or([255; 3], |[r, g, b, _]| [r, g, b]);
    let light = (setting > 1).then_some(LightSource {
        range: FLOOD_RANGES[usize::from(setting - 1)],
        power: FLOOD_LIGHT_POWER,
        color,
        height: 1.0,
        angle: 360,
        dir: Dir::North,
        offset: [0, 0],
    });
    match world.get_mut::<Optics>(flood) {
        Some(mut optics) => {
            if optics.light != light {
                optics.light = light;
            }
        }
        None => {
            world
                .entity_mut(flood)
                .insert(Optics { light, ..default() });
        }
    }
    let state = if setting > 1 {
        "floodlight_on"
    } else {
        "floodlight"
    };
    crate::obj_damage::set_icon_state(world, flood, state);
    setting
}

/// tg `process()` once per machine tick: a lit floodlight draws from its cable's net, or goes
/// off when the net cannot pay.
// Cost: per machine tick (0.5 Hz), one lookup per lit floodlight (a handful per round).
fn floodlight_tick(world: &mut World, mut last: Local<u64>) {
    let ticks = world.resource::<super::MachineTicks>().0;
    if ticks == *last {
        return;
    }
    *last = ticks;
    let mut query = world.query::<(Entity, &Floodlight)>();
    let lit: Vec<(Entity, u8)> = query
        .iter(world)
        .filter(|(_, f)| f.setting > 1)
        .map(|(e, f)| (e, f.setting))
        .collect();
    for (flood, setting) in lit {
        let need = Floodlight::watts(setting) * super::MACHINE_TICK_SECS;
        let paid = {
            let mut grid = world.resource_mut::<super::Grid>();
            let net = grid.node_net(flood);
            match net.and_then(|n| grid.net_mut(n)) {
                Some(net) if net.surplus() >= need => {
                    net.load += need;
                    true
                }
                _ => false,
            }
        };
        if !paid {
            change_setting(world, flood, 1);
        }
    }
}

/// `attack_hand`: with the panel open the tube comes out (the frame stays, waiting for a
/// tube); else the setting steps high -> med -> low -> off -> high.
fn floodlight_hand(world: &mut World, client: Entity, mob: Entity, flood: Entity) -> bool {
    let Some(f) = world.get::<Floodlight>(flood).copied() else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(flood) else {
        return false;
    };
    if f.panel_open {
        break_floodlight(world, flood, pos, false);
        if let Some(tube) = crate::construction::spawn_obj(world, TUBE, pos)
            && let Some(inventory) = world.get::<Inventory>(mob).cloned()
        {
            crate::storage::put_in_hands(world, mob, &inventory, tube);
        }
        return true;
    }
    let wanted = if f.setting == 1 { 4 } else { f.setting - 1 };
    let got = change_setting(world, flood, wanted);
    let text = match got {
        2 => "слабую мощность",
        3 => "стандартное освещение",
        4 => "высокую мощность",
        _ => "ВЫКЛ",
    };
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы переключаете прожектор на {text}."),
    );
    true
}

/// The floodlight gone, its frame (waiting for a tube) in its place; `shattered`: tg
/// `atom_break` (glass sound, a broken tube on the floor).
fn break_floodlight(world: &mut World, flood: Entity, pos: TilePos, shattered: bool) {
    world.resource_mut::<super::Grid>().remove_node(flood);
    world.despawn(flood);
    if let Some(frame) = crate::construction::spawn_obj(world, FLOOD_FRAME, pos) {
        set_flood_stage(world, frame, FLOOD_NEEDS_LIGHTS);
    }
    if shattered {
        playsound(world, PlaySound::new(pos, GLASSBR3, 100.0, true));
        if let Some(tube) = crate::construction::spawn_obj(world, TUBE, pos) {
            shatter(world, tube);
        }
    }
    crate::construction::tile_changed(world, pos);
}

/// Screwdriver (panel; opening switches it off) and wrench (anchor and connect, or loose) on a
/// floodlight.
fn floodlight_tool(world: &mut World, client: Entity, held: Entity, flood: Entity) -> bool {
    let (Some(f), Some(&pos)) = (
        world.get::<Floodlight>(flood).copied(),
        world.get::<TilePos>(flood),
    ) else {
        return false;
    };
    match tool_of(world, held).map(|t| t.0) {
        Some(ToolKind::Screwdriver) => {
            if !f.panel_open {
                change_setting(world, flood, 1);
            }
            if let Some(mut f) = world.get_mut::<Floodlight>(flood) {
                f.panel_open = !f.panel_open;
            }
            playsound(world, PlaySound::new(pos, SCREWDRIVER, 50.0, true));
            let text = if f.panel_open {
                "Панель закрыта."
            } else {
                "Панель открыта."
            };
            tell(world, client, Span::Notice, text);
            true
        }
        Some(ToolKind::Wrench) => {
            let anchored = !world
                .get::<crate::obj_damage::Anchored>(flood)
                .is_some_and(|a| a.0);
            crate::obj_damage::set_anchored(world, flood, anchored);
            change_setting(world, flood, 1);
            {
                let mut grid = world.resource_mut::<super::Grid>();
                grid.remove_node(flood);
                if anchored {
                    grid.add_node(flood, pos, f.layer, super::NodeKind::Machine);
                }
            }
            playsound(world, PlaySound::new(pos, RATCHET, 50.0, true));
            let text = if anchored {
                "Вы закрепляете прожектор."
            } else {
                "Вы откручиваете прожектор."
            };
            tell(world, client, Span::Notice, text);
            true
        }
        _ => false,
    }
}

/// `/obj/structure/floodlight_frame` `item_interaction`, `screwdriver_act`, `wirecutter_act`,
/// `wrench_act`.
fn flood_frame_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    frame: Entity,
) -> bool {
    let stage = flood_stage(world, frame);
    let Some(&pos) = world.get::<TilePos>(frame) else {
        return false;
    };
    let held_path = crate::emp::path_of(world, held).unwrap_or_default();
    if is_under(&held_path, CABLE) && stage == FLOOD_NEEDS_WIRES {
        if crate::tools::stack_amount(world, held) < FLOOD_CABLE
            || !crate::tools::use_stack(world, held, FLOOD_CABLE)
        {
            tell(world, client, Span::Warning, "Нужно 5 кусков кабеля!");
            return true;
        }
        set_flood_stage(world, frame, FLOOD_NEEDS_SECURING);
        return true;
    }
    if is_under(&held_path, TUBE) {
        if stage != FLOOD_NEEDS_LIGHTS {
            tell(world, client, Span::Warning, "Сборка не закончена!");
            return true;
        }
        if bulb_of(world, held).is_some_and(|b| b.status == LightStatus::Broken) {
            tell(world, client, Span::Warning, "Трубка разбита!");
            return true;
        }
        world.despawn(held);
        finish_floodlight(world, frame);
        return true;
    }
    match tool_of(world, held).map(|t| t.0) {
        Some(ToolKind::Screwdriver) => {
            match stage {
                FLOOD_NEEDS_SECURING => set_flood_stage(world, frame, FLOOD_NEEDS_LIGHTS),
                FLOOD_NEEDS_LIGHTS => set_flood_stage(world, frame, FLOOD_NEEDS_SECURING),
                _ => {}
            }
            true
        }
        Some(ToolKind::Wirecutters) => {
            if stage == FLOOD_NEEDS_SECURING {
                set_flood_stage(world, frame, FLOOD_NEEDS_WIRES);
                crate::construction::spawn_stack(world, CABLE, pos, FLOOD_CABLE);
            }
            true
        }
        Some(ToolKind::Wrench) => {
            if stage == FLOOD_NEEDS_WIRES {
                crate::tools::use_tool(
                    world,
                    Actor {
                        client,
                        mob,
                        item: Some(held),
                    },
                    frame,
                    FRAME_WRENCH_DS,
                    0,
                    Job::Light(LightJob::FloodFrameApart),
                );
            }
            true
        }
        _ => false,
    }
}

/// The frame becomes a floodlight (unanchored, off).
fn finish_floodlight(world: &mut World, frame: Entity) {
    let Some(&pos) = world.get::<TilePos>(frame) else {
        return;
    };
    world.despawn(frame);
    if let Some(flood) = crate::construction::spawn_obj(world, FLOODLIGHT, pos) {
        let layer = world.get::<DmPower>(flood).map_or(2, super::layer);
        world.entity_mut(flood).insert(Floodlight {
            setting: 1,
            panel_open: false,
            layer,
        });
        crate::obj_damage::set_anchored(world, flood, false);
    }
    crate::construction::tile_changed(world, pos);
}

/// [`LightJob::FloodFrameApart`] at its end: still unwired, 5 iron sheets.
fn flood_frame_apart(world: &mut World, action: &crate::tools::DoAfter) {
    let frame = action.target;
    if flood_stage(world, frame) != FLOOD_NEEDS_WIRES {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(frame) else {
        return;
    };
    world.despawn(frame);
    crate::construction::spawn_stack(world, IRON, pos, FLOOD_IRON);
    crate::construction::tile_changed(world, pos);
}

/// `/obj/machinery/power/floodlight/emp_act`: `prob(150 / severity)` breaks it.
fn floodlight_emp(world: &mut World, mut cursor: Local<MessageCursor<crate::emp::EmpAct>>) {
    let Some(messages) = world.get_resource::<Messages<crate::emp::EmpAct>>() else {
        return;
    };
    let hits: Vec<crate::emp::EmpAct> = cursor
        .read(messages)
        .filter(|e| e.hits_self())
        .copied()
        .collect();
    for ev in hits {
        if world.get::<Floodlight>(ev.target).is_none() {
            continue;
        }
        let Some(&pos) = world.get::<TilePos>(ev.target) else {
            continue;
        };
        let chance = 150.0 / f32::from(ev.severity.max(1));
        if world.resource_mut::<EngineRng>().0.chance(chance) {
            break_floodlight(world, ev.target, pos, true);
        }
    }
}

/// Examine lines of fixtures, frames and replacers.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    if let Some(f) = world.get::<LightFixture>(target) {
        let fit = fitting(f);
        lines.push(match f.status {
            LightStatus::Ok => (
                Span::Notice,
                format!(
                    "Он {}.",
                    if f.mode == LightMode::On {
                        "включён"
                    } else {
                        "выключен"
                    }
                ),
            ),
            LightStatus::Empty => (Span::Notice, format!("Отсутствует: {fit}.")),
            LightStatus::Burned => (Span::Danger, format!("Перегорела: {fit}.")),
            LightStatus::Broken => (Span::Danger, format!("Разбита: {fit}.")),
        });
        if let Some(charge) = f.emergency {
            let percent = (charge / super::EMERGENCY_CELL_CHARGE * 1000.0).round() / 10.0;
            lines.push((
                Span::Notice,
                format!("Индикатор резервного питания показывает {percent}%."),
            ));
        }
    } else if world.get::<LightFrame>(target).is_some() || is_frame(world, target) {
        let text = if frame_stage(world, target) == STAGE_WIRED {
            "Проводка на месте, но болты не закручены."
        } else {
            "Пустой каркас без проводов."
        };
        lines.push((Span::Notice, text.to_owned()));
    } else if let Some(r) = replacer_of(world, target) {
        lines.push((Span::Notice, status_string(&r)));
    } else if let Some(b) = world.get::<LightBulb>(target) {
        match b.status {
            LightStatus::Burned => lines.push((Span::Notice, "Перегоревшая лампа.".to_owned())),
            LightStatus::Broken => lines.push((Span::Notice, "Разбитая лампа.".to_owned())),
            _ => {}
        }
    }
}

fn is_frame(world: &World, e: Entity) -> bool {
    crate::emp::path_of(world, e).is_some_and(|p| is_under(&p, FRAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_follow_tg_defines() {
        assert_eq!(LightStatus::from_dm(0), LightStatus::Ok);
        assert_eq!(LightStatus::from_dm(1), LightStatus::Empty);
        assert_eq!(LightStatus::from_dm(2), LightStatus::Broken);
        assert_eq!(LightStatus::from_dm(3), LightStatus::Burned);
        assert_eq!(LightStatus::Burned.suffix(), Some("-burned"));
    }

    fn lit_world() -> (World, Entity, Entity) {
        let mut w = World::new();
        w.init_resource::<Time>();
        w.init_resource::<EngineRng>();
        w.init_resource::<Messages<PlaySound>>();
        // `get_components_mut` with an `Option<&mut Appearance>` fails on a world that never
        // registered the component; a server world always has.
        w.register_component::<Appearance>();
        let area = w
            .spawn(AreaPower {
                requires_power: true,
                power: [true; 3],
                lightswitch: true,
                ..default()
            })
            .id();
        let on = LightSource {
            range: 8.0,
            power: 1.0,
            color: [255; 3],
            height: 1.0,
            angle: 170,
            dir: Dir::North,
            offset: [0, 0],
        };
        let dm = DmPower {
            power_consumption_rate: 20.0,
            ..default()
        };
        let light = DmLight {
            brightness: 8.0,
            ..default()
        };
        let fixture = new_fixture(None, &dm, &light, Some(on), "", area, None, LightStatus::Ok);
        let e = w.spawn((fixture, Optics::default())).id();
        update(&mut w, e);
        (w, area, e)
    }

    fn lit(w: &World, e: Entity) -> Option<LightSource> {
        w.get::<Optics>(e).unwrap().light
    }

    #[test]
    fn a_broken_tube_goes_dark_and_a_new_one_lights_again() {
        let (mut w, area, e) = lit_world();
        assert_eq!(w.get::<LightFixture>(e).unwrap().mode, LightMode::On);
        assert_eq!(w.get::<AreaPower>(area).unwrap().static_power[1], 160.0);
        assert!(break_tube(&mut w, e, true));
        assert!(!break_tube(&mut w, e, true), "nothing left to break");
        assert_eq!(
            w.get::<LightFixture>(e).unwrap().status,
            LightStatus::Broken
        );
        assert_eq!(lit(&w, e), None);
        assert_eq!(w.get::<AreaPower>(area).unwrap().static_power[1], 0.0);
        // A bulb of brightness 4 in: lit at its range, its load.
        set_status(&mut w, e, LightStatus::Ok, Some(4.0));
        assert_eq!(lit(&w, e).map(|l| l.range), Some(4.0));
        assert_eq!(w.get::<AreaPower>(area).unwrap().static_power[1], 80.0);
        assert!(burn_out(&mut w, e));
        assert_eq!(
            w.get::<LightFixture>(e).unwrap().status,
            LightStatus::Burned
        );
        assert_eq!(lit(&w, e), None);
    }

    #[test]
    fn a_flicker_dims_then_ends_lit() {
        let (mut w, _, e) = lit_world();
        assert!(flicker(&mut w, e, 1));
        assert!(!flicker(&mut w, e, 1), "already flickering");
        assert_eq!(lit(&w, e).map(|l| l.range), Some(2.0));
        for _ in 0..3 {
            w.resource_mut::<Time>()
                .advance_by(std::time::Duration::from_secs(7));
            run_flickers(&mut w);
        }
        assert!(w.get::<Flicker>(e).is_none());
        assert!(!w.get::<LightFixture>(e).unwrap().flickering);
        assert_eq!(lit(&w, e).map(|l| l.range), Some(8.0));
    }

    #[test]
    fn a_floodlight_steps_down_to_what_its_net_pays() {
        let mut w = World::new();
        let mut grid = super::super::Grid::default();
        let cable = w.spawn_empty().id();
        let pos = TilePos::new(1, 1, 1);
        grid.build([(cable, pos, 2)]);
        let flood = w
            .spawn((
                Floodlight {
                    setting: 1,
                    panel_open: false,
                    layer: 2,
                },
                pos,
            ))
            .id();
        grid.add_node(flood, pos, 2, super::super::NodeKind::Machine);
        let net = grid.node_net(flood).expect("on the cable");
        // 2.5 kW for a machine tick: medium (2 kW) but not high (3 kW).
        grid.net_mut(net).unwrap().avail = 2500.0 * super::super::MACHINE_TICK_SECS;
        w.insert_resource(grid);
        assert_eq!(change_setting(&mut w, flood, 4), 3);
        assert_eq!(lit(&w, flood).map(|l| l.range), Some(10.0));
        assert_eq!(change_setting(&mut w, flood, 1), 1);
        assert_eq!(lit(&w, flood), None);
        assert_eq!(Floodlight::watts(2), 1000.0);
    }

    #[test]
    fn advanced_replacer_holds_more() {
        assert_eq!(new_replacer(REPLACER).max_uses, 20);
        assert_eq!(new_replacer(REPLACER_ADVANCED).max_uses, 50);
        assert_eq!(new_replacer(REPLACER).uses, 10);
    }
}
