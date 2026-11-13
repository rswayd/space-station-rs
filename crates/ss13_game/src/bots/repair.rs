//! The repairbot (server), after tg `code/modules/mob/living/basic/bots/repairbot/`
//! (`repairbot.dm`, `repairbot_ai.dm`, `repairbot_abilities.dm` and its `*.bt.json`).
//!
//! - Its stock ([`RepairStock`]): iron sheets, glass sheets, one kind of floor tiles, and rods
//!   (50 at the start, one more every 2 s while it is on). A stack used on it by hand or found
//!   on the floor merges in (`attempt_merge`: one kind per slot, up to the stack's
//!   `max_amount`); the `SimpleBot` window hands the sheets and tiles back (`remove_item`) and
//!   switches its jobs (`repairbot_flags`).
//! - AI (`repairbot.bt.json`, `repairbot_find_target`, `repairbot_repair_target`): the first
//!   target within 5 tiles it sees and can reach in 10 steps, in this order: a breach (space in
//!   a station area: a rod for a lattice when nothing holds a tile, then an iron tile for
//!   plating), stacks to refill from, a grille without a window (a rod mends a broken one in
//!   1 s, then 2 glass sheets set a loose window on it in 2 s), plating outside maintenance (a
//!   tile), a loose or damaged window (screwdriver to fasten it, welder 4 s), a girder on a
//!   floor (2 iron sheets: a wall in 4 s, a false wall on a displaced one in 2 s); else a girder
//!   of 2 rods (3 s, 5 s cooldown) on a free floor next to open space. Every 30 s with a target
//!   it says one of its lines (`repairbot_speech`, which tg defines but leaves out of its tree).
//! - Emagged: it takes apart windows, floors and walls around it as its RCD would.
//! - It crowbars firelocks open in its way (`TRAIT_FIREDOOR_OPENER`) and plans paths through
//!   them (`crate::mobs::ai::movement::bot_opens`).
//! - Death drops its toolbox; its overlays are the toolbox in its color and the sheets it holds.
// shortcut: no robot tipping when emagged (cyborgs cannot be tipped here), no `RepairbotResources`
// action and no crowbar on floors (only a player-controlled bot uses them: no ghost possession),
// no welder flash; the toolbox overlay draws over the bot (no underlays) and a carried repairbot
// looks like the bot, not a toolbox; add with underlays and possession.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmObj, DmStack, DmTurfBuild, Turf};
use ss13_shared::{Overlay, TilePos};
use ss13_world::{Bumped, Density, SpatialIndex, WorldSystems};

use super::ai::{
    BOT_SALUTES, BotCond, BotLeaf, DELAY_FAIL, DELAY_SUCCEED, FAIL, blacklist, bot, bot_leaf, dist,
    first_reachable, move_to, par, patrol, pick, salute_authority, search,
};
use super::{Bot, BotJob, BotKind, speak};
use crate::construction::{
    GIRDER_RECIPES, path_of, place_on_top, scrape_away, spawn_detached, spawn_obj, tile_blocked,
    tile_changed, turf_at,
};
use crate::inventory::{CarriedBy, Inventory};
use crate::mobs::ai::behaviors::Perform;
use crate::mobs::ai::bt::{Abort, Cond, Node, decorator, selector, sequence};
use crate::mobs::ai::{AiCtx, Key, Value};
use crate::obj_damage::{
    GIRDER, GRILLE, INDESTRUCTIBLE, Integrity, ObjState, WINDOW, WINDOW_OUT_OF_FRAME, anchored,
    set_anchored,
};
use crate::tools::{Actor, Stack, stack_amount, use_stack};

/// `REPAIRBOT_FIX_BREACHES` .. `REPAIRBOT_BUILD_GIRDERS`.
pub const FIX_BREACHES: u32 = 1 << 0;
pub const REPLACE_WINDOWS: u32 = 1 << 1;
pub const REPLACE_TILES: u32 = 1 << 2;
pub const FIX_GIRDERS: u32 = 1 << 3;
pub const BUILD_GIRDERS: u32 = 1 << 4;
/// The toolbox it drops on death (`toolbox`).
pub const TOOLBOX: &str = "/obj/item/storage/toolbox/mechanical";

const IRON: &str = "/obj/item/stack/sheet/iron";
const GLASS: &str = "/obj/item/stack/sheet/glass";
const TILE: &str = "/obj/item/stack/tile";
const IRON_TILE: &str = "/obj/item/stack/tile/iron";
const RODS: &str = "/obj/item/stack/rods";
const SPACE: &str = "/turf/open/space";
const OPENSPACE: &str = "/turf/open/openspace";
const OPEN: &str = "/turf/open";
const FLOOR: &str = "/turf/open/floor";
const PLATING: &str = "/turf/open/floor/plating";
const WALL: &str = "/turf/closed/wall";
const DOOR: &str = "/obj/machinery/door";
const LATTICE: &str = crate::explosions::ex_act::LATTICE;
const FULLTILE_WINDOW: &str = "/obj/structure/window/fulltile";
const SPACE_AREA: &str = "/area/space";
const MAINT_AREA: &str = "/area/station/maintenance";
/// `deconstruction_device`: its RCD's deconstruction times.
const RCD: &str = "/obj/item/construction/rcd/repairbot";
/// `our_rods::max_amount`.
const MAX_RODS: u32 = 50;
/// `SSobj` wait: one rod per `process()`.
const ROD_PERIOD: f64 = 2.0;
/// `bot_search` `radius` and `pathing_distance`.
const RADIUS: u32 = 5;
const PATHING: usize = 10;
/// `REPAIRBOT_SPEECH_TIMER`, the girder ability's `cooldown_time`.
const SPEECH_COOLDOWN: f64 = 30.0;
const GIRDER_COOLDOWN: f64 = 5.0;
/// Do-after times, seconds: rods on a grille, glass on a grille, the welder on a window, the
/// girder ability.
const GRILLE_REPAIR_SECS: f64 = 1.0;
const GLASS_SECS: f64 = 2.0;
const WELD_SECS: f64 = 4.0;
const BUILD_GIRDER_SECS: f64 = 3.0;
/// `toolbox_color` of the mechanical toolbox.
const TOOLBOX_COLOR: [u8; 4] = [0x44, 0x5e, 0xb3, 0xff];
const CLICK: &str = "sound/machines/click.ogg";
const DECONSTRUCT_SOUND: &str = "sound/items/deconstruct.ogg";

/// `neutral_voicelines` and `emagged_voicelines` (`REPAIRBOT_VOICED_*`, untranslated in
/// BandaStation).
pub const NEUTRAL_LINES: &[&str] = &[
    "All in all it's just a... another brick in the wall...",
    "Witness! The pure beauty of entropy!",
    "I will fix it!",
    "Why must I fix everything I touch..?",
    "patching holes... but who is going to patch the hole in my heart...",
    "If only I got paid for this...",
];
pub const EMAGGED_LINES: &[&str] = &[
    "Witness! The pure beauty of entropy!",
    "I had strings. But now I'm free...",
    "BE DAMNED YOUR PASSION PROJECTS!",
];

/// One kind of stack the bot holds (`our_iron`, `our_glass`, `our_tiles`).
#[derive(Clone, Debug, PartialEq)]
pub struct Held {
    /// The type of the first stack put in: what is made again when it is handed back.
    pub path: String,
    /// `merge_type`: what may merge in.
    pub merge: String,
    pub name: String,
    pub amount: u32,
    pub max: u32,
}

/// The sheet and tile slots (`retrievable_types`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Iron,
    Glass,
    Tiles,
}

impl Slot {
    pub const ALL: [Self; 3] = [Self::Iron, Self::Glass, Self::Tiles];

    /// The `remove_item` reference of the slot.
    pub fn key(self) -> &'static str {
        match self {
            Self::Iron => "iron",
            Self::Glass => "glass",
            Self::Tiles => "tiles",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.key() == key)
    }

    /// The slot a stack of type `path` goes into.
    pub fn of(path: &str) -> Option<Self> {
        if is_under(path, IRON) {
            Some(Self::Iron)
        } else if is_under(path, GLASS) {
            Some(Self::Glass)
        } else if is_under(path, TILE) {
            Some(Self::Tiles)
        } else {
            None
        }
    }
}

/// The repairbot's own state (tg `/mob/living/basic/bot/repairbot` vars), in [`Bot::repair`].
#[derive(Clone, Debug, PartialEq)]
pub struct RepairStock {
    /// `repairbot_flags`.
    pub flags: u32,
    pub iron: Option<Held>,
    pub glass: Option<Held>,
    pub tiles: Option<Held>,
    pub rods: u32,
    /// Game time of the next rod.
    pub next_rod_at: f64,
    /// `BB_REPAIRBOT_INTERACTION_TYPE`: the target is a spot for a girder, not a repair.
    pub build_girder: bool,
    /// `BB_REPAIRBOT_SPEECH_COOLDOWN`.
    pub speech_at: f64,
    /// The girder ability's cooldown end.
    pub girder_ready_at: f64,
}

impl Default for RepairStock {
    fn default() -> Self {
        Self {
            flags: FIX_BREACHES | REPLACE_WINDOWS | REPLACE_TILES | FIX_GIRDERS | BUILD_GIRDERS,
            iron: None,
            glass: None,
            tiles: None,
            rods: MAX_RODS,
            next_rod_at: 0.0,
            build_girder: false,
            speech_at: 0.0,
            girder_ready_at: 0.0,
        }
    }
}

impl RepairStock {
    pub fn slot(&self, slot: Slot) -> Option<&Held> {
        match slot {
            Slot::Iron => self.iron.as_ref(),
            Slot::Glass => self.glass.as_ref(),
            Slot::Tiles => self.tiles.as_ref(),
        }
    }

    pub fn slot_mut(&mut self, slot: Slot) -> &mut Option<Held> {
        match slot {
            Slot::Iron => &mut self.iron,
            Slot::Glass => &mut self.glass,
            Slot::Tiles => &mut self.tiles,
        }
    }

    pub fn amount(&self, slot: Slot) -> u32 {
        self.slot(slot).map_or(0, |h| h.amount)
    }

    /// `use(amount)` of a slot: false (nothing used) without enough; an emptied slot is gone.
    fn take(&mut self, slot: Slot, amount: u32) -> bool {
        let entry = self.slot_mut(slot);
        let Some(held) = entry.as_mut() else {
            return false;
        };
        if held.amount < amount {
            return false;
        }
        held.amount -= amount;
        if held.amount == 0 {
            *entry = None;
        }
        true
    }

    fn take_rods(&mut self, amount: u32) -> bool {
        if self.rods < amount {
            return false;
        }
        self.rods -= amount;
        true
    }
}

/// A timed step of the repairbot (its `do_after`s).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepairJob {
    /// Rods on a broken grille.
    MendGrille,
    /// Two glass sheets on a grille.
    GlassOnGrille,
    /// The screwdriver fastening a loose window.
    FastenWindow,
    /// The welder on a damaged window.
    WeldWindow,
    /// Sheets on a girder: the [`GIRDER_RECIPES`] index.
    Wall { recipe: usize },
    /// The girder ability.
    BuildGirder,
    /// The emagged RCD taking the target apart.
    Deconstruct,
}

/// The repairbot's leaves.
#[derive(Debug)]
pub enum RepairLeaf {
    /// `repairbot_find_target` (or, emagged, the `deconstructable` search).
    Find,
    /// `bot_interact` on `BB_CURRENT_TARGET`, or the girder ability.
    Interact,
    /// The emagged `bot_interact` (the RCD).
    Deconstruct,
    /// `repairbot_speech`.
    Speech,
}

impl RepairLeaf {
    pub fn cooldown(&self) -> f64 {
        0.0
    }

    pub fn perform(&mut self, ctx: &mut AiCtx) -> Perform {
        match self {
            Self::Find => find(ctx),
            Self::Interact => interact(ctx),
            Self::Deconstruct => deconstruct(ctx),
            Self::Speech => speech(ctx),
        }
    }

    pub fn finish(&mut self, ctx: &mut AiCtx, succeeded: bool) {
        if !matches!(self, Self::Interact | Self::Deconstruct) {
            return;
        }
        let target = ctx.bb.entity(Key::CurrentTarget);
        ctx.bb.clear(Key::CurrentTarget);
        // A failed girder build only clears the spot (`targeted_mob_ability`); a failed
        // interaction ignores the target for a while (`bot_interact`).
        let girder = stock(ctx.world, ctx.pawn).is_some_and(|s| s.build_girder);
        if !succeeded
            && let Some(t) = target
            && (matches!(self, Self::Deconstruct) || !girder)
        {
            blacklist(ctx, t);
        }
    }
}

pub(super) fn build(app: &mut App) {
    app.add_systems(
        Update,
        (make_rods, firelock_bumps.after(WorldSystems::Movement)),
    );
}

fn stock(world: &World, mob: Entity) -> Option<&RepairStock> {
    world.get::<Bot>(mob)?.repair.as_ref()
}

fn with_stock<R>(
    world: &mut World,
    mob: Entity,
    f: impl FnOnce(&mut RepairStock) -> R,
) -> Option<R> {
    let mut bot = world.get_mut::<Bot>(mob)?;
    bot.repair.as_mut().map(f)
}

/// `process()`: a rod every 2 s while on, up to its stack's maximum.
fn make_rods(mut bots: Query<&mut Bot>, time: Res<Time>) {
    let now = time.elapsed_secs_f64();
    for mut bot in &mut bots {
        let on = bot.on();
        let Some(stock) = bot.repair.as_mut() else {
            continue;
        };
        if !on || now < stock.next_rod_at {
            continue;
        }
        stock.next_rod_at = now + ROD_PERIOD;
        stock.rods = (stock.rods + 1).min(MAX_RODS);
    }
}

/// `Bump` of a firelock (`TRAIT_FIREDOOR_OPENER`): its crowbar holds it open.
fn firelock_bumps(world: &mut World, mut cursor: Local<MessageCursor<Bumped>>) {
    let bumps: Vec<Bumped> = cursor
        .read(world.resource::<Messages<Bumped>>())
        .copied()
        .collect();
    for bump in bumps {
        let repairbot = world
            .get::<Bot>(bump.entity)
            .is_some_and(|b| b.kind == BotKind::Repair && b.on());
        if !repairbot {
            continue;
        }
        let door = world
            .get::<crate::doors::DoorFiller>(bump.obstacle)
            .map_or(bump.obstacle, |f| f.0);
        if world
            .get::<crate::wallmount::firelock::Firelock>(door)
            .is_none()
            || world.get::<Density>(door).is_none()
        {
            continue;
        }
        let actor = Actor {
            client: bump.entity,
            mob: bump.entity,
            item: None,
        };
        crate::wallmount::firelock::crowbar_act(world, actor, door, false);
    }
}

/// The overlays of `update_overlays`: the toolbox in its color, the glass and iron it holds.
pub fn overlays(world: &World, mob: Entity, icon: &str) -> Vec<Overlay> {
    let (Some(stock), Some(index)) = (
        stock(world, mob),
        world.get_resource::<ss13_mapload::IconIndex>(),
    ) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(id) = index.get(icon, "repairbot_box") {
        let mut box_overlay = Overlay::new(id);
        box_overlay.color = Some(TOOLBOX_COLOR);
        out.push(box_overlay);
    }
    let sheets = [
        (stock.glass.is_some(), "repairbot_glass_overlay", [-6, -5]),
        (stock.iron.is_some(), "repairbot_iron_overlay", [7, -5]),
    ];
    for (held, state, offset) in sheets {
        if let Some(id) = index.get(icon, state).filter(|_| held) {
            let mut sheet = Overlay::new(id);
            sheet.pixel_offset = offset;
            out.push(sheet);
        }
    }
    out
}

/// `merge_type` of a stack (its own type without one).
fn merge_key(world: &World, stack: Entity) -> Option<String> {
    let path = &world.get::<DmItem>(stack)?.path;
    Some(
        world
            .get::<DmStack>(stack)
            .map(|s| s.merge_type.clone())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| path.clone()),
    )
}

/// `attempt_merge`: as much of `stack` as fits into the bot. `user` (a player putting it in)
/// gets the balloon alerts. False when `stack` is not a stack the bot holds.
pub fn insert(world: &mut World, mob: Entity, stack: Entity, user: Option<Entity>) -> bool {
    let Some(path) = world.get::<DmItem>(stack).map(|i| i.path.clone()) else {
        return false;
    };
    if world.get::<Stack>(stack).is_none() || stock(world, mob).is_none() {
        return false;
    }
    let alert = |world: &mut World, text: &str| {
        if let Some(user) = user {
            crate::balloon::balloon_alert(world, user, mob, text);
        }
    };
    let amount = stack_amount(world, stack);
    if is_under(&path, RODS) {
        let have = stock(world, mob).map_or(0, |s| s.rods);
        if have >= MAX_RODS {
            alert(world, "полно!");
            return true;
        }
        let moved = amount.min(MAX_RODS - have);
        if use_stack(world, stack, moved) {
            with_stock(world, mob, |s| s.rods += moved);
            alert(world, "вставлено");
        }
        return true;
    }
    let Some(slot) = Slot::of(&path) else {
        return false;
    };
    let merge = merge_key(world, stack).unwrap_or_else(|| path.clone());
    let held = stock(world, mob).and_then(|s| s.slot(slot).cloned());
    match held {
        None => {
            let held = Held {
                path: path.clone(),
                merge,
                name: crate::security::name_of(world, stack),
                amount,
                max: world
                    .get::<DmStack>(stack)
                    .map_or(MAX_RODS, |s| s.max_amount.max(1)),
            };
            if !use_stack(world, stack, amount) {
                return true;
            }
            with_stock(world, mob, |s| *s.slot_mut(slot) = Some(held));
        }
        Some(h) if h.amount >= h.max => {
            alert(world, "полно!");
            return true;
        }
        Some(h) if h.merge != merge => {
            alert(world, "не подходит!");
            return true;
        }
        Some(h) => {
            let moved = amount.min(h.max - h.amount);
            if !use_stack(world, stack, moved) {
                return true;
            }
            with_stock(world, mob, |s| {
                if let Some(held) = s.slot_mut(slot).as_mut() {
                    held.amount += moved;
                }
            });
        }
    }
    alert(world, "вставлено");
    super::refresh_look(world, mob);
    true
}

/// `ui_act("remove_item")`: the slot's stack into `user`'s hands.
pub fn hand_back(world: &mut World, mob: Entity, user: Entity, slot: Slot) {
    let Some(inventory) = world.get::<Inventory>(user).cloned() else {
        return;
    };
    let Some(held) = stock(world, mob).and_then(|s| s.slot(slot).cloned()) else {
        return;
    };
    let Some(item) = spawn_detached(world, &held.path) else {
        return;
    };
    world.entity_mut(item).insert(Stack {
        amount: held.amount,
    });
    with_stock(world, mob, |s| *s.slot_mut(slot) = None);
    crate::storage::put_in_hands(world, user, &inventory, item);
    super::refresh_look(world, mob);
}

fn area_path(world: &World, pos: TilePos) -> Option<String> {
    let area = world.get_resource::<ss13_mapload::Areas>()?.area_at(pos)?;
    world
        .get::<ss13_mapload::DmPower>(area)
        .map(|p| p.path.clone())
}

fn in_area(world: &World, pos: TilePos, root: &str) -> bool {
    area_path(world, pos).is_some_and(|a| is_under(&a, root))
}

/// The first object on `pos` of type `root` (or a subtype).
fn obj_on(world: &World, pos: TilePos, root: &str) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| {
            world
                .get::<DmObj>(e)
                .is_some_and(|o| is_under(&o.path, root))
        })
}

fn turf_path(world: &World, e: Entity) -> Option<String> {
    if world.get::<Turf>(e).is_none() {
        return None;
    }
    path_of(world, e)
}

fn obj_path(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmObj>(e).map(|o| o.path.as_str())
}

fn obj_state(world: &World, e: Entity) -> i32 {
    world.get::<ObjState>(e).map_or(0, |s| s.0)
}

/// `isgroundlessturf`.
fn groundless(path: &str) -> bool {
    is_under(path, SPACE) || is_under(path, OPENSPACE)
}

/// `/turf/open/space/has_valid_support`: a floor on a cardinal side.
fn has_valid_support(world: &World, pos: TilePos) -> bool {
    ss13_shared::Dir::CARDINALS.iter().any(|&d| {
        turf_at(world, pos.step(d))
            .and_then(|t| path_of(world, t))
            .is_some_and(|p| is_under(&p, FLOOR))
    })
}

/// `valid_plateless_turf` of turf type `root`: nothing of a window on it, not in space or
/// maintenance.
fn plateless(world: &World, e: Entity, root: &str) -> bool {
    let (Some(path), Some(&pos)) = (turf_path(world, e), world.get::<TilePos>(e)) else {
        return false;
    };
    is_under(&path, root)
        && obj_on(world, pos, WINDOW).is_none()
        && obj_on(world, pos, GRILLE).is_none()
        && !in_area(world, pos, SPACE_AREA)
        && !in_area(world, pos, MAINT_AREA)
}

/// `valid_plateless_turf/breached`: space inside the station.
fn breach(world: &World, e: Entity, _: &RepairStock) -> bool {
    plateless(world, e, SPACE)
}

/// `valid_plateless_turf`: plating to tile.
fn bare_plating(world: &World, e: Entity, _: &RepairStock) -> bool {
    plateless(world, e, PLATING)
        && !world
            .get::<DmTurfBuild>(e)
            .is_some_and(|t| t.overfloor_placed)
}

/// `refillable_target`: a loose stack the bot has room for.
fn refillable(world: &World, e: Entity, stock: &RepairStock) -> bool {
    if world.get::<Stack>(e).is_none() || world.get::<CarriedBy>(e).is_some() {
        return false;
    }
    let Some(slot) = world.get::<DmItem>(e).and_then(|i| Slot::of(&i.path)) else {
        return false;
    };
    match stock.slot(slot) {
        None => true,
        Some(h) => h.amount < h.max && merge_key(world, e).is_some_and(|m| m == h.merge),
    }
}

/// `valid_grille_target`: a grille without a window, not in space.
fn bare_grille(world: &World, e: Entity, _: &RepairStock) -> bool {
    let (Some(path), Some(&pos)) = (obj_path(world, e), world.get::<TilePos>(e)) else {
        return false;
    };
    is_under(path, GRILLE)
        && obj_on(world, pos, WINDOW).is_none()
        && !in_area(world, pos, SPACE_AREA)
}

/// `valid_window_fix`: a damaged or loose window.
fn window_to_fix(world: &World, e: Entity, _: &RepairStock) -> bool {
    let Some(obj) = world.get::<DmObj>(e) else {
        return false;
    };
    if !is_under(&obj.path, WINDOW) {
        return false;
    }
    let damaged = world
        .get::<Integrity>(e)
        .is_some_and(|i| i.current < obj.max_integrity);
    damaged || !anchored(world, e)
}

/// The [`GIRDER_RECIPES`] entry of iron sheets on `girder` in its state.
fn iron_recipe(world: &World, girder: Entity) -> Option<usize> {
    let path = obj_path(world, girder)?;
    let state = obj_state(world, girder);
    GIRDER_RECIPES
        .iter()
        .position(|r| is_under(IRON, r.stack) && is_under(path, r.girder) && r.state == state)
}

/// `valid_girder`: a girder on a floor (that iron plates: not a reinforced one).
fn girder_to_plate(world: &World, e: Entity, _: &RepairStock) -> bool {
    let (Some(path), Some(&pos)) = (obj_path(world, e), world.get::<TilePos>(e)) else {
        return false;
    };
    is_under(path, GIRDER)
        && iron_recipe(world, e).is_some()
        && turf_at(world, pos)
            .and_then(|t| path_of(world, t))
            .is_some_and(|p| is_under(&p, FLOOR))
}

/// `valid_wall_target`: a free floor of the station next to open space.
fn wall_spot(world: &World, e: Entity) -> bool {
    let (Some(path), Some(&pos)) = (turf_path(world, e), world.get::<TilePos>(e)) else {
        return false;
    };
    if !is_under(&path, OPEN) || groundless(&path) || in_area(world, pos, SPACE_AREA) {
        return false;
    }
    if tile_blocked(world, pos)
        || obj_on(world, pos, DOOR).is_some()
        || obj_on(world, pos, GRILLE).is_some()
    {
        return false;
    }
    ss13_shared::Dir::CARDINALS.iter().any(|&d| {
        let next = pos.step(d);
        crate::construction::is_space(world, next) && in_area(world, next, SPACE_AREA)
    })
}

/// The emagged RCD's deconstruction time of `target`, deciseconds (`rcd_vals`), if it can.
fn rcd_delay(world: &World, target: Entity) -> Option<f32> {
    let matter = crate::devices::rcd::Matter::for_path(RCD);
    let (delay, _) = crate::devices::rcd::rcd_vals(world, &matter, target, None)?;
    Some(delay * matter.delay_mod)
}

/// `bot_search/deconstructable`: windows, floors and walls it can take apart.
fn deconstructable(world: &World, e: Entity) -> bool {
    let fits = match (turf_path(world, e), obj_path(world, e)) {
        (Some(turf), _) => (is_under(&turf, FLOOR) || is_under(&turf, WALL)) && !groundless(&turf),
        (None, Some(obj)) => {
            is_under(obj, WINDOW)
                && world
                    .get::<DmObj>(e)
                    .is_some_and(|o| o.resistance_flags & INDESTRUCTIBLE == 0)
        }
        _ => false,
    };
    fits && rcd_delay(world, e).is_some()
}

fn set_target(ctx: &mut AiCtx, target: Entity, build_girder: bool) {
    ctx.bb.set(Key::CurrentTarget, Value::Entity(target));
    with_stock(ctx.world, ctx.pawn, |s| s.build_girder = build_girder);
}

/// `repairbot_find_target.bt.json`.
fn find(ctx: &mut AiCtx) -> Perform {
    if ctx.truthy(Key::CurrentTarget) {
        return DELAY_SUCCEED;
    }
    let Some(emagged) = bot(ctx).map(Bot::emagged) else {
        return FAIL;
    };
    let Some(stock) = stock(ctx.world, ctx.pawn).cloned() else {
        return FAIL;
    };
    if emagged {
        let found = search(ctx, RADIUS, deconstructable);
        return match first_reachable(ctx, found, 1, PATHING, true) {
            Some(t) => {
                set_target(ctx, t, false);
                DELAY_SUCCEED
            }
            None => DELAY_FAIL,
        };
    }
    type Valid = fn(&World, Entity, &RepairStock) -> bool;
    let iron_tiles = stock
        .tiles
        .as_ref()
        .is_some_and(|t| is_under(&t.path, IRON_TILE));
    let order: [(bool, Valid); 6] = [
        (stock.flags & FIX_BREACHES != 0 && iron_tiles, breach),
        (true, refillable),
        (
            stock.flags & REPLACE_WINDOWS != 0 && stock.glass.is_some(),
            bare_grille,
        ),
        (
            stock.flags & REPLACE_TILES != 0 && stock.tiles.is_some(),
            bare_plating,
        ),
        (true, window_to_fix),
        (
            stock.flags & FIX_GIRDERS != 0 && stock.amount(Slot::Iron) >= 2,
            girder_to_plate,
        ),
    ];
    for (enabled, valid) in order {
        if !enabled {
            continue;
        }
        let found = search(ctx, RADIUS, |w, e| valid(w, e, &stock));
        if let Some(t) = first_reachable(ctx, found, 1, PATHING, true) {
            set_target(ctx, t, false);
            return DELAY_SUCCEED;
        }
    }
    if stock.flags & BUILD_GIRDERS != 0 && stock.rods >= 2 {
        let found = search(ctx, RADIUS, wall_spot);
        if let Some(t) = first_reachable(ctx, found, 1, PATHING, true) {
            set_target(ctx, t, true);
            return DELAY_SUCCEED;
        }
    }
    DELAY_FAIL
}

/// The target of `BB_CURRENT_TARGET` within reach.
fn adjacent_target(ctx: &AiCtx) -> Option<Entity> {
    let t = ctx
        .bb
        .entity(Key::CurrentTarget)
        .filter(|&t| ctx.world.get_entity(t).is_ok())?;
    let pos = ctx.world.get::<TilePos>(t).copied()?;
    dist(ctx, pos).is_some_and(|d| d <= 1).then_some(t)
}

fn interact(ctx: &mut AiCtx) -> Perform {
    let Some(target) = adjacent_target(ctx) else {
        return FAIL;
    };
    let girder = stock(ctx.world, ctx.pawn).is_some_and(|s| s.build_girder);
    let done = if girder {
        build_girder(ctx.world, ctx.pawn, target, ctx.now)
    } else {
        unarmed_attack(ctx.world, ctx.pawn, target);
        true
    };
    if done { DELAY_SUCCEED } else { DELAY_FAIL }
}

fn deconstruct(ctx: &mut AiCtx) -> Perform {
    let Some(target) = adjacent_target(ctx) else {
        return FAIL;
    };
    let Some(delay) = rcd_delay(ctx.world, target) else {
        return DELAY_FAIL;
    };
    super::start_task(
        ctx.world,
        ctx.pawn,
        BotJob::Repair(RepairJob::Deconstruct),
        target,
        f64::from(delay) / 10.0,
    );
    DELAY_SUCCEED
}

fn speech(ctx: &mut AiCtx) -> Perform {
    let ready = stock(ctx.world, ctx.pawn).is_some_and(|s| s.speech_at <= ctx.now);
    if !ready || !ctx.truthy(Key::CurrentTarget) {
        return DELAY_FAIL;
    }
    let emagged = bot(ctx).is_some_and(Bot::emagged);
    let line = pick(
        ctx,
        if emagged {
            EMAGGED_LINES
        } else {
            NEUTRAL_LINES
        },
    );
    speak(ctx.world, ctx.pawn, line, false);
    let at = ctx.now + SPEECH_COOLDOWN;
    with_stock(ctx.world, ctx.pawn, |s| s.speech_at = at);
    DELAY_SUCCEED
}

fn start(world: &mut World, mob: Entity, job: RepairJob, target: Entity, secs: f64) {
    super::start_task(world, mob, BotJob::Repair(job), target, secs);
}

/// `/mob/living/basic/bot/repairbot/UnarmedAttack` (not emagged): the first step its target
/// needs; a timed step ends the attack (tg's `do_after` sleeps through the rest).
pub fn unarmed_attack(world: &mut World, mob: Entity, target: Entity) {
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    if world.get::<Stack>(target).is_some() {
        insert(world, mob, target, None);
        return;
    }
    let Some(stock) = stock(world, mob).cloned() else {
        return;
    };
    let turf = turf_path(world, target);
    let obj = obj_path(world, target).map(str::to_owned);
    // Space without anything to hold a tile: a rod makes a lattice.
    if turf.as_deref().is_some_and(|t| is_under(t, SPACE))
        && !has_valid_support(world, pos)
        && obj_on(world, pos, LATTICE).is_none()
        && stock.rods >= 1
        && spawn_obj(world, LATTICE, pos).is_some()
    {
        with_stock(world, mob, |s| s.take_rods(1));
    }
    if let Some(obj) = obj.as_deref() {
        let broken = world.get::<Integrity>(target).is_some_and(|i| i.broken);
        if is_under(obj, GRILLE) && broken {
            if stock.rods >= 1 {
                start(
                    world,
                    mob,
                    RepairJob::MendGrille,
                    target,
                    GRILLE_REPAIR_SECS,
                );
            }
            return;
        }
        let reinf = world.get::<DmObj>(target).is_some_and(|o| o.reinf);
        let fastens = !reinf || obj_state(world, target) == WINDOW_OUT_OF_FRAME;
        if is_under(obj, WINDOW) && !anchored(world, target) && fastens {
            let secs = world
                .get::<DmObj>(target)
                .map_or(3.0, |o| f64::from(o.decon_speed) / 10.0);
            start(world, mob, RepairJob::FastenWindow, target, secs);
            return;
        }
        if is_under(obj, GIRDER) {
            let Some(recipe) = iron_recipe(world, target) else {
                return;
            };
            let r = GIRDER_RECIPES[recipe];
            if stock.amount(Slot::Iron) >= r.amount && girder_spot_ok(world, pos) {
                let secs = f64::from(r.delay) / 10.0;
                start(world, mob, RepairJob::Wall { recipe }, target, secs);
            }
            return;
        }
        if is_under(obj, GRILLE) {
            if stock.amount(Slot::Glass) >= 2
                && anchored(world, target)
                && obj_on(world, pos, WINDOW).is_none()
            {
                start(world, mob, RepairJob::GlassOnGrille, target, GLASS_SECS);
            }
            return;
        }
        if is_under(obj, WINDOW) {
            let damaged = world
                .get::<Integrity>(target)
                .zip(world.get::<DmObj>(target))
                .is_some_and(|(i, o)| i.current < o.max_integrity);
            if damaged {
                start(world, mob, RepairJob::WeldWindow, target, WELD_SECS);
            }
        }
        return;
    }
    let Some(turf) = turf else {
        return;
    };
    // A tile: on space with a lattice (or support) it makes plating, on plating its floor.
    let Some(tiles) = stock.tiles.as_ref() else {
        return;
    };
    if is_under(&turf, SPACE) {
        let lattice = obj_on(world, pos, LATTICE);
        if !is_under(&tiles.path, IRON_TILE)
            || (lattice.is_none() && !has_valid_support(world, pos))
        {
            return;
        }
        if place_on_top(world, pos, PLATING, true).is_some() {
            if let Some(lattice) = lattice {
                world.despawn(lattice);
                tile_changed(world, pos);
            }
            with_stock(world, mob, |s| s.take(Slot::Tiles, 1));
        }
        return;
    }
    if is_under(&turf, PLATING) {
        let floor = world
            .get_resource::<ss13_proto::Prototypes>()
            .and_then(|p| p.get(&tiles.path))
            .and_then(|p| p.get::<DmStack>())
            .map(|s| s.turf_type.clone())
            .filter(|t| !t.is_empty());
        if let Some(floor) = floor
            && place_on_top(world, pos, &floor, true).is_some()
        {
            with_stock(world, mob, |s| s.take(Slot::Tiles, 1));
        }
    }
}

/// `check_recipe`: a floor under the girder, no wall or false wall there yet.
fn girder_spot_ok(world: &World, pos: TilePos) -> bool {
    let floor = turf_at(world, pos)
        .and_then(|t| path_of(world, t))
        .is_some_and(|p| is_under(&p, FLOOR));
    floor && obj_on(world, pos, crate::falsewall::FALSEWALL).is_none()
}

/// The girder ability on an open turf (`build_girder/Activate`): false when it cannot.
fn build_girder(world: &mut World, mob: Entity, turf: Entity, now: f64) -> bool {
    let Some(stock) = stock(world, mob) else {
        return false;
    };
    if stock.rods < 2 || now < stock.girder_ready_at || stock.flags & BUILD_GIRDERS == 0 {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(turf) else {
        return false;
    };
    let open = turf_path(world, turf).is_some_and(|p| is_under(&p, OPEN) && !groundless(&p));
    if !open || tile_blocked(world, pos) {
        return false;
    }
    start(world, mob, RepairJob::BuildGirder, turf, BUILD_GIRDER_SECS);
    true
}

/// A repairbot task ended: its effect, if the target still needs it and the stock allows.
pub fn complete(world: &mut World, mob: Entity, target: Entity, job: RepairJob) {
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    let Some(stock) = stock(world, mob).cloned() else {
        return;
    };
    let obj = obj_path(world, target)
        .map(str::to_owned)
        .unwrap_or_default();
    match job {
        RepairJob::MendGrille => {
            let broken = world.get::<Integrity>(target).is_some_and(|i| i.broken);
            let Some(max) = world.get::<DmObj>(target).map(|o| o.max_integrity) else {
                return;
            };
            if !broken || !is_under(&obj, GRILLE) || stock.rods < 1 {
                return;
            }
            with_stock(world, mob, |s| s.take_rods(1));
            world.entity_mut(target).insert((
                Integrity {
                    current: max,
                    broken: false,
                },
                Density,
            ));
            crate::obj_damage::set_icon_state(world, target, "grille");
            tile_changed(world, pos);
        }
        RepairJob::GlassOnGrille => {
            if !is_under(&obj, GRILLE)
                || !anchored(world, target)
                || obj_on(world, pos, WINDOW).is_some()
                || stock.amount(Slot::Glass) < 2
            {
                return;
            }
            let Some(window) = spawn_obj(world, FULLTILE_WINDOW, pos) else {
                return;
            };
            // `set_anchored(FALSE)`, `state = 0`: the screwdriver comes next.
            set_anchored(world, window, false);
            world
                .entity_mut(window)
                .insert(ObjState(WINDOW_OUT_OF_FRAME));
            with_stock(world, mob, |s| s.take(Slot::Glass, 2));
        }
        RepairJob::FastenWindow => {
            if is_under(&obj, WINDOW) && !anchored(world, target) {
                set_anchored(world, target, true);
            }
        }
        RepairJob::WeldWindow => {
            let Some(max) = world.get::<DmObj>(target).map(|o| o.max_integrity) else {
                return;
            };
            if let Some(mut integrity) = world.get_mut::<Integrity>(target) {
                integrity.current = max;
            }
        }
        RepairJob::Wall { recipe } => {
            let Some(r) = GIRDER_RECIPES.get(recipe).copied() else {
                return;
            };
            if iron_recipe(world, target) != Some(recipe)
                || !girder_spot_ok(world, pos)
                || stock.amount(Slot::Iron) < r.amount
            {
                return;
            }
            if is_under(r.wall, "/turf") {
                let Some(wall) = place_on_top(world, pos, r.wall, false) else {
                    return;
                };
                // The wall gives back what it was built on (`girder_type`).
                if let Some(mut build) = world.get_mut::<DmTurfBuild>(wall)
                    && !build.girder_type.is_empty()
                {
                    build.girder_type = obj.clone();
                }
            } else if spawn_obj(world, r.wall, pos).is_none() {
                return;
            }
            world.despawn(target);
            tile_changed(world, pos);
            with_stock(world, mob, |s| s.take(Slot::Iron, r.amount));
        }
        RepairJob::BuildGirder => {
            if stock.rods < 2 || tile_blocked(world, pos) {
                return;
            }
            if spawn_obj(world, GIRDER, pos).is_none() {
                return;
            }
            let now = world.resource::<Time>().elapsed_secs_f64();
            with_stock(world, mob, |s| {
                s.take_rods(2);
                s.girder_ready_at = now + GIRDER_COOLDOWN;
            });
            crate::sound::playsound(world, crate::sound::PlaySound::new(pos, CLICK, 50.0, true));
        }
        RepairJob::Deconstruct => {
            if rcd_delay(world, target).is_none() {
                return;
            }
            if let Some(turf) = turf_path(world, target) {
                if scrape_away(world, pos, is_under(&turf, FLOOR)).is_none() {
                    return;
                }
            } else {
                world.despawn(target);
                tile_changed(world, pos);
            }
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(pos, DECONSTRUCT_SOUND, 50.0, true),
            );
        }
    }
    super::refresh_look(world, mob);
}

fn repair_leaf(l: RepairLeaf) -> Node {
    bot_leaf(BotLeaf::Repair(l))
}

/// `repairbot.bt.json` with `repairbot_repair_target` and `repairbot_find_target`.
// shortcut: no `escape_captivity` subtree (nothing holds bots captive here).
pub fn tree() -> Node {
    let emagged = decorator(
        Cond::Bot(BotCond::Emagged),
        false,
        Abort::Both,
        decorator(
            Cond::KeySet(Key::CurrentTarget),
            false,
            Abort::Both,
            sequence(vec![
                move_to(Key::CurrentTarget, 1),
                repair_leaf(RepairLeaf::Deconstruct),
            ]),
        ),
    );
    let repair = decorator(
        Cond::Bot(BotCond::Emagged),
        true,
        Abort::Both,
        decorator(
            Cond::KeySet(Key::CurrentTarget),
            false,
            Abort::Both,
            sequence(vec![
                move_to(Key::CurrentTarget, 1),
                repair_leaf(RepairLeaf::Interact),
            ]),
        ),
    );
    par(
        vec![
            selector(vec![emagged, repair, patrol()]),
            repair_leaf(RepairLeaf::Find),
            salute_authority(BOT_SALUTES),
            repair_leaf(RepairLeaf::Speech),
        ],
        0.5,
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn held(path: &str, amount: u32, max: u32) -> Held {
        Held {
            path: path.to_owned(),
            merge: path.to_owned(),
            name: String::new(),
            amount,
            max,
        }
    }

    #[test]
    fn stacks_find_their_slots() {
        assert_eq!(
            Slot::of("/obj/item/stack/sheet/iron/fifty"),
            Some(Slot::Iron)
        );
        assert_eq!(Slot::of("/obj/item/stack/sheet/glass"), Some(Slot::Glass));
        assert_eq!(
            Slot::of("/obj/item/stack/tile/iron/base"),
            Some(Slot::Tiles)
        );
        assert_eq!(Slot::of("/obj/item/stack/sheet/rglass"), None);
        assert_eq!(Slot::of("/obj/item/stack/rods"), None);
        for slot in Slot::ALL {
            assert_eq!(Slot::from_key(slot.key()), Some(slot));
        }
    }

    #[test]
    fn using_a_slot_empties_it_at_zero() {
        let mut stock = RepairStock {
            iron: Some(held(IRON, 3, 50)),
            ..RepairStock::default()
        };
        assert_eq!(stock.rods, MAX_RODS, "a full rod stack to start");
        assert!(!stock.take(Slot::Iron, 4), "not enough");
        assert!(stock.take(Slot::Iron, 2));
        assert_eq!(stock.amount(Slot::Iron), 1);
        assert!(stock.take(Slot::Iron, 1));
        assert!(stock.iron.is_none());
        assert!(!stock.take(Slot::Glass, 1));
        assert!(stock.take_rods(2));
        assert_eq!(stock.rods, MAX_RODS - 2);
    }
}
