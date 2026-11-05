//! Tables, table frames and racks (server), after tg `code/game/objects/structures/tables_racks.dm`,
//! `table_frames.dm` and the `climbable`, `climb_walkable`, `elevation` and `table_smash`
//! elements.
//!
//! - An item clicked onto a table goes there at the clicked point, onto a rack centered (out of
//!   combat mode; [`place`], from `crate::inventory`); a tray with things in it empties onto a
//!   table instead (`tray_act`).
//! - A right click with an empty hand flips a table onto its side (`max_integrity × 0.25`
//!   deciseconds): it becomes a border on the edge away from the flipper (tg `CanAllowThrough`,
//!   `on_exit`), pushes what lies on it one tile that way and takes nothing; the same click
//!   stands it up again.
//! - Dragging yourself onto a table or rack climbs it (`climbable`: 2 s, twice as long with
//!   blocked hands, then a 2 s stun); someone clicking it meanwhile knocks the climber down.
//!   From a table a mob walks onto the next table (`climb_walkable`, `ss13_world`), and a mob on
//!   a table or rack is drawn 12 pixels up (`elevation`, [`Elevated`]).
//! - A click on a table with an empty hand while pulling a mob (`table_smash`): out of combat
//!   mode lays it on the table after 3.5 s; in combat mode an aggressive grab slams it (3 s
//!   knockdown, 40 stamina, 10 brute), a passive one is not enough. A shove into a table knocks
//!   down onto it (`on_disarm_shoved_into`). A living mob landing on a glass table breaks it.
//! - Right click with a screwdriver takes the top off (the frame stays), with a wrench takes
//!   the table apart (`deconstruction_ready`; a reinforced table is welded loose first, right
//!   click with a welder). A wrench takes a frame apart; a stack whose `table_type` is set builds
//!   that table on it. A wrench (right click) turns a rack into parts, parts used in hand build a
//!   rack, a wrench turns parts into a sheet.
// shortcut: no rolling tables, no greyscale (material) tables, no card decks dealing onto
// tables (no playing cards yet), no neck grabs (the limb smash), no fireman carry, no shock from
// a cable under a reinforced table, no table damage from slams; add with those systems.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::smooth::DmSmooth;
use ss13_mapload::{DmAtom, DmItem, DmObj, DmStack, DmTable, IconIndex};
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::protocol::ClickModifiers;
use ss13_shared::{Appearance, Dir, Facing, Layer, TilePos};
use ss13_world::{Border, ClimbWalkable, Density, SpatialIndex, WorldSystems};

use crate::combat::{CombatMode, NextMove};
use crate::construction::{Job, path_of, spawn_obj, spawn_stack, tile_changed};
use crate::health::{BodyPosition, DamageType, Health, Stat, apply_damage};
use crate::inventory::{CarriedBy, Inventory, put_down_at};
use crate::pull::{GrabState, Pulling, step_passing, visible_message};
use crate::security::name_of;
use crate::sound::{PlaySound, playsound};
use crate::status_effects::{Incap, adjust_stamina, knockdown};
use crate::tools::{Actor, DoAfter, ToolKind, do_after, tell, tool_of, use_tool};

const TABLE: &str = "/obj/structure/table";
const FRAME: &str = "/obj/structure/table_frame";
const RACK: &str = "/obj/structure/rack";
const RACK_PARTS: &str = "/obj/item/rack_parts";
const RAILING_CORNER: &str = "/obj/structure/railing/corner";
const GLASS_TABLE: &str = "/obj/structure/table/glass";
const REINFORCED_TABLE: &str = "/obj/structure/table/reinforced";
const TRAY: &str = "/obj/item/storage/bag/tray";
const IRON: &str = "/obj/item/stack/sheet/iron";

/// `TABLE_LAYER`, `LOW_ITEM_LAYER`, `ABOVE_MOB_LAYER`.
const TABLE_LAYER: f32 = 2.8;
const LOW_ITEM_LAYER: f32 = 2.95;
const ABOVE_MOB_LAYER: f32 = 4.1;
/// `/datum/element/elevation` of tables and racks.
const ELEVATION: i16 = 12;
/// `/datum/element/climbable`: `climb_time`, `climb_stun` (deciseconds, seconds).
const CLIMB_TIME_DS: f32 = 20.0;
const CLIMB_STUN: f32 = 2.0;
/// `attack_hand` of a climbable with someone on the way up: `Paralyze(40)`.
const CLIMB_KNOCKOFF: f32 = 4.0;
/// `perform_table_smash`: the gentle placement, deciseconds.
const TABLE_PLACE_DS: f32 = 35.0;
/// `tablepush`.
const SLAM_KNOCKDOWN: f32 = 3.0;
const SLAM_STAMINA: f32 = 40.0;
const SLAM_BRUTE: f32 = 10.0;
/// `table_shatter`: `Paralyze(100)`.
const SHATTER_PARALYZE: f32 = 10.0;
/// `SHOVE_KNOCKDOWN_TABLE`, its daze.
const SHOVE_KNOCKDOWN_TABLE: f32 = 2.0;
const SHOVE_DAZE: f32 = 3.0;
/// Deconstruction delays, deciseconds.
const SCREW_DS: f32 = 20.0;
const WRENCH_DS: f32 = 40.0;
const WELD_DS: f32 = 50.0;
const FRAME_WRENCH_DS: f32 = 30.0;
const FRAME_BUILD_DS: f32 = 20.0;
const RACK_BUILD_DS: f32 = 50.0;

mod sounds {
    pub use ss13_mapload::sound::files::{
        DECONSTRUCT, TABLE_FLIP as FLIP, TABLE_SLAM as SLAM, TABLE_UNFLIP as UNFLIP,
    };
    /// `SFX_SHATTER`.
    pub const SHATTER: &str = "shatter";
}

pub struct TablesPlugin;

impl Plugin for TablesPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // Rack parts build a rack.
        app.add_click_handler(ClickPhase::AttackSelf, 2100, |w, c| {
            crate::tables::attack_self(w, c.client, c.mob, c.target)
        });
        // tg `attack_hand` of tables and racks: a pulled mob onto it, flipping (right click),
        // knocking a climber down, kicking a rack.
        app.add_click_handler(ClickPhase::EmptyHand, 2700, |w, c| {
            crate::tables::attack_hand(w, c.client, c.mob, c.target, c.modifiers)
        });
        // Tools on tables, frames and racks; a stack builds a table on a frame.
        app.add_click_handler(ClickPhase::HeldOnTarget, 4300, |w, c| {
            c.held.is_some_and(|h| {
                crate::tables::item_interaction(w, c.client, c.mob, h, c.target, c.modifiers)
            })
        });
        app.add_observer(add_table)
            .add_observer(add_rack)
            .add_systems(Update, elevate_moved.after(WorldSystems::Movement));
    }
}

/// The live vars of a table: `deconstruction_ready`, `is_flipped` and what it was built on.
#[derive(Component, Clone, Debug, Default)]
pub struct TableState {
    pub ready: bool,
    /// Flipped toward this side, with the smoothing it had standing.
    pub flipped: Option<(Dir, Option<DmSmooth>)>,
    /// Built on a frame: that frame and what it leaves (tg `apply_frame_properties`).
    pub built_on: Option<(String, String, u32)>,
}

/// Mobs on its tile are drawn this many pixels up (tg `/datum/element/elevation`).
#[derive(Component, Clone, Copy, Debug)]
pub struct Elevating(pub i16);

/// On a mob: how far up it is drawn (the `Elevating` thing on its tile, else 0).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Elevated(pub i16);

/// The `do_after`s of this module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableJob {
    /// Flipping or standing up the target table.
    Flip,
    /// Climbing onto the target table or rack.
    Climb,
    /// Laying the target (a pulled mob) onto `table`.
    Place { table: Entity },
    /// A screwdriver takes the top off.
    Screw,
    /// A wrench takes the table apart.
    Wrench,
    /// A welder toggles a reinforced table's `deconstruction_ready`.
    Weld,
    /// A wrench takes a frame apart.
    FrameApart,
    /// The held stack builds its table on the target frame.
    Build,
    /// Rack parts in hand build a rack.
    Rack,
}

fn add_table(add: On<Add<DmTable>>, tables: Query<(&DmTable, &DmObj)>, mut commands: Commands) {
    let Ok((dm, obj)) = tables.get(add.entity) else {
        return;
    };
    if is_under(&obj.path, TABLE) {
        commands.entity(add.entity).insert((
            TableState {
                ready: dm.deconstruction_ready,
                ..default()
            },
            Elevating(ELEVATION),
            ClimbWalkable,
        ));
    }
}

fn add_rack(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    if objs.get(add.entity).is_ok_and(|o| is_under(&o.path, RACK)) {
        commands.entity(add.entity).insert(Elevating(ELEVATION));
    }
}

/// tg `elevation`: mobs that moved are drawn up on a table or rack, down off it.
fn elevate_moved(
    mut mobs: Query<
        (
            Entity,
            &TilePos,
            Option<&mut Elevated>,
            Option<&crate::furniture::Seated>,
        ),
        (With<BodyPosition>, Changed<TilePos>),
    >,
    lifts: Query<&Elevating>,
    index: Res<SpatialIndex>,
    mut commands: Commands,
) {
    for (mob, &pos, elevated, seat) in &mut mobs {
        // A stool's `add_offsets` adds to the elevation.
        let up = lift_at(&index, &lifts, pos) + seat.map_or(0, |s| s.up);
        match elevated {
            Some(mut e) => {
                e.set_if_neq(Elevated(up));
            }
            None if up != 0 => {
                commands.entity(mob).insert(Elevated(up));
            }
            None => {}
        }
    }
}

fn lift_at(index: &SpatialIndex, lifts: &Query<&Elevating>, pos: TilePos) -> i16 {
    index
        .on_tile(pos)
        .iter()
        .filter_map(|&e| lifts.get(e).ok())
        .map(|l| l.0)
        .max()
        .unwrap_or(0)
}

/// Re-elevates the mobs on `pos` after a table there changed (or a mob sat down or got up).
pub(crate) fn reelevate(world: &mut World, pos: TilePos) {
    let mut lifts = world.query::<&Elevating>();
    let up = (world.resource::<SpatialIndex>().on_tile(pos).iter())
        .filter_map(|&e| lifts.get(world, e).ok())
        .map(|l| l.0)
        .max()
        .unwrap_or(0);
    let mobs: Vec<Entity> = (world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied())
    .filter(|&e| world.get::<BodyPosition>(e).is_some())
    .collect();
    for mob in mobs {
        let up = up
            + world
                .get::<crate::furniture::Seated>(mob)
                .map_or(0, |s| s.up);
        if world.get::<Elevated>(mob) != Some(&Elevated(up)) {
            world.entity_mut(mob).insert(Elevated(up));
        }
    }
}

fn obj_path(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmObj>(e).map(|o| o.path.as_str())
}

/// A table (not a frame).
pub fn is_table(world: &World, e: Entity) -> bool {
    world.get::<TableState>(e).is_some()
}

fn is_frame(world: &World, e: Entity) -> bool {
    obj_path(world, e).is_some_and(|p| is_under(p, FRAME))
}

fn is_rack(world: &World, e: Entity) -> bool {
    obj_path(world, e).is_some_and(|p| is_under(p, RACK))
}

/// A climbable railing (`railing/climbable`: not the corners and ends).
fn is_railing(world: &World, e: Entity) -> bool {
    obj_path(world, e).is_some_and(|p| {
        is_under(p, ss13_mapload::border::RAILING_ROOT) && !is_under(p, RAILING_CORNER)
    }) && world.get::<Border>(e).is_some()
}

fn flipped(world: &World, e: Entity) -> bool {
    world
        .get::<TableState>(e)
        .is_some_and(|t| t.flipped.is_some())
}

fn combat(world: &World, mob: Entity) -> bool {
    world.get::<CombatMode>(mob).is_some_and(|c| c.0)
}

/// Messages to everyone who sees `at`, a personal line to the ones in `personal`.
fn say(world: &mut World, at: TilePos, span: Span, others: String, personal: &[(Entity, String)]) {
    let personal: Vec<_> = personal
        .iter()
        .map(|(e, text)| (*e, span, text.clone()))
        .collect();
    let chat: Vec<SendChat> =
        visible_message(world, at, ChatChannel::Info, (span, others), &personal);
    for message in chat {
        world.write_message(message);
    }
}

fn sound(world: &mut World, at: TilePos, file: &str, vol: f32) {
    playsound(world, PlaySound::new(at, file, vol, true));
}

/// tg `base_item_interaction` of a table (`tray_act`, `table_place_act`) or rack, for
/// `crate::inventory`: false when `target` is neither.
pub fn place(
    world: &mut World,
    mob: Entity,
    held: Entity,
    target: Entity,
    pos: TilePos,
    modifiers: ClickModifiers,
) -> bool {
    let table = is_table(world, target);
    if !table && !is_rack(world, target) {
        return false;
    }
    if flipped(world, target) {
        return true;
    }
    if table && empty_tray(world, mob, held, target, pos) {
        return true;
    }
    if combat(world, mob) {
        return true;
    }
    let offset = match (table, modifiers.pixel) {
        (true, Some(p)) => p.map(|v| i16::from(v).clamp(-16, 16)),
        _ => [0, 0],
    };
    put_down_at(world, held, pos, offset);
    true
}

/// tg `tray_act`: a tray with things in it empties onto the table.
fn empty_tray(world: &mut World, mob: Entity, tray: Entity, table: Entity, pos: TilePos) -> bool {
    if !world
        .get::<DmItem>(tray)
        .is_some_and(|i| is_under(&i.path, TRAY))
    {
        return false;
    }
    let items: Vec<Entity> = world
        .get::<crate::storage::Stored>(tray)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    if items.is_empty() {
        return false;
    }
    for item in items {
        crate::storage::take_out(world, item, pos);
    }
    let (user, what, onto) = (
        name_of(world, mob),
        name_of(world, tray),
        name_of(world, table),
    );
    say(
        world,
        pos,
        Span::Notice,
        format!("{user} опустошает {what} на {onto}."),
        &[],
    );
    true
}

/// tg `attack_hand` / `attack_hand_secondary` of a table or rack with an empty hand (the
/// `table_smash` and `climbable` signals first). False when `target` is neither.
pub fn attack_hand(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    let table = is_table(world, target);
    if !table && !is_rack(world, target) {
        return false;
    }
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    let is_flipped = flipped(world, target);
    // `table_smash/on_interaction`: a pulled mob.
    if table
        && !is_flipped
        && !modifiers.right
        && let Some(pulled) = world.get::<Pulling>(mob).map(|p| p.0)
        && world.get::<Health>(pulled).is_some()
    {
        smash(world, actor, pulled, target);
        return true;
    }
    if table && modifiers.right {
        start_flip(world, actor, target);
        return true;
    }
    if is_flipped || modifiers.right {
        return true;
    }
    // `climbable/attack_hand`: whoever is on the way up falls.
    if knock_climbers_off(world, mob, target) {
        return true;
    }
    // `rack/attack_hand`: a kick in combat mode.
    if !table && combat(world, mob) && !world.get::<BodyPosition>(mob).is_some_and(|b| b.lying()) {
        let now = world.resource::<Time>().elapsed_secs_f64();
        world
            .entity_mut(mob)
            .insert(NextMove(now + crate::combat::CLICK_CD_MELEE));
        let damage = world.resource_mut::<crate::combat::CombatRng>().range(4, 8) as f32;
        crate::obj_damage::take_damage(world, target, damage, DamageType::Brute);
        let (user, rack) = (name_of(world, mob), name_of(world, target));
        let at = world.get::<TilePos>(target).copied().unwrap_or_default();
        say(
            world,
            at,
            Span::Danger,
            format!("{user} пинает {rack}."),
            &[],
        );
    }
    true
}

fn knock_climbers_off(world: &mut World, mob: Entity, target: Entity) -> bool {
    let mut query = world.query::<(Entity, &DoAfter)>();
    let climbers: Vec<Entity> = query
        .iter(world)
        .filter(|(e, d)| {
            *e != mob && d.target == target && matches!(d.job, Job::Table(TableJob::Climb))
        })
        .map(|(e, _)| e)
        .collect();
    if climbers.is_empty() {
        return false;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    world
        .entity_mut(mob)
        .insert(NextMove(now + crate::combat::CLICK_CD_MELEE));
    for climber in climbers {
        crate::status_effects::apply(world, climber, Incap::Paralyze, CLIMB_KNOCKOFF);
        let (who, what) = (name_of(world, climber), name_of(world, target));
        let at = world.get::<TilePos>(climber).copied().unwrap_or_default();
        say(
            world,
            at,
            Span::Warning,
            format!("{who} срывается с {what}."),
            &[(climber, format!("Вы срываетесь с {what}!"))],
        );
    }
    true
}

/// tg `attack_hand_secondary`: a `do_after` of `max_integrity × 0.25` deciseconds.
fn start_flip(world: &mut World, actor: Actor, table: Entity) {
    let Some(dm) = world.get::<DmTable>(table) else {
        return;
    };
    if !dm.can_flip {
        return;
    }
    let delay = world.get::<DmObj>(table).map_or(100.0, |o| o.max_integrity) * 0.25;
    let text = if flipped(world, table) {
        "поднимает стол..."
    } else {
        "переворачивает стол..."
    };
    let user = name_of(world, actor.mob);
    let at = world.get::<TilePos>(table).copied().unwrap_or_default();
    if do_after(world, actor, table, delay, 0, Job::Table(TableJob::Flip)) {
        say(world, at, Span::Notice, format!("{user} {text}"), &[]);
    }
}

/// tg `flip_table(get_dir(user, src))` / `unflip_table`.
fn toggle_flip(world: &mut World, mob: Entity, table: Entity) {
    let (Some(&pos), Some(&from)) = (world.get::<TilePos>(table), world.get::<TilePos>(mob)) else {
        return;
    };
    let Some(state) = world.get::<TableState>(table).cloned() else {
        return;
    };
    let dm = world.get::<DmTable>(table).cloned().unwrap_or_default();
    match state.flipped {
        None => {
            // `flip_table`: east or west wins over north and south.
            let (dx, dy) = (pos.x - from.x, pos.y - from.y);
            let dir = match (dx.signum(), dy.signum()) {
                (1, _) => Dir::East,
                (-1, _) => Dir::West,
                (_, 1) => Dir::North,
                _ => Dir::South,
            };
            sound(world, pos, sounds::FLIP, 100.0);
            // What lies on it is pushed off before it starts to block that side.
            let movables: Vec<Entity> = (world.resource::<SpatialIndex>().on_tile(pos).iter())
                .copied()
                .filter(|&e| e != table && pushable(world, e))
                .collect();
            // `safe_throw_at(range = 1)`: over tables and racks (`LETPASSTHROW`).
            for e in movables {
                let _ = step_passing(world, e, dir, |w, b| {
                    crate::inventory::lets_pass(w, b, crate::inventory::LETPASSTHROW)
                });
            }
            let smooth = world.get::<DmSmooth>(table).cloned();
            let layer = if dir == Dir::South {
                ABOVE_MOB_LAYER
            } else {
                LOW_ITEM_LAYER
            };
            let flipped_state = world
                .get_resource::<IconIndex>()
                .and_then(|i| i.get(&dm.flipped_icon, &dm.base_icon_state));
            let mut entity = world.entity_mut(table);
            entity
                .remove::<(DmSmooth, Density, Elevating, ClimbWalkable)>()
                .insert((Border { edges: dir as u8 }, Facing(dir)));
            if let Some(mut look) = entity.get_mut::<Appearance>() {
                look.layer = Layer(layer);
                if let Some(state) = flipped_state {
                    look.icon_state = state;
                }
            }
            entity.get_mut::<TableState>().expect("checked").flipped = Some((dir, smooth));
        }
        Some((_, smooth)) => {
            sound(world, pos, sounds::UNFLIP, 100.0);
            let layer = world.get::<DmAtom>(table).map_or(TABLE_LAYER, |a| a.layer);
            let mut entity = world.entity_mut(table);
            entity
                .remove::<Border>()
                .insert((Density, Elevating(ELEVATION), ClimbWalkable));
            if let Some(smooth) = smooth {
                entity.insert(smooth);
            }
            if let Some(mut look) = entity.get_mut::<Appearance>() {
                look.layer = Layer(layer);
            }
            entity.get_mut::<TableState>().expect("checked").flipped = None;
        }
    }
    tile_changed(world, pos);
    reelevate(world, pos);
}

/// tg `is_able_to_throw`: not anchored, an obj or a mob.
fn pushable(world: &World, e: Entity) -> bool {
    let anchored = world.get::<DmObj>(e).is_some_and(|o| o.anchored);
    let thing = world.get::<DmItem>(e).is_some() || world.get::<Health>(e).is_some();
    thing && !anchored
}

/// tg `climbable/mousedrop_receive`: `dropped` is the user, onto a table or rack it reaches.
pub fn mousedropped_onto(world: &mut World, actor: Actor, over: Entity, dropped: Entity) -> bool {
    if dropped != actor.mob
        || (!is_table(world, over) && !is_rack(world, over) && !is_railing(world, over))
    {
        return false;
    }
    if flipped(world, over) {
        return false;
    }
    if !crate::storage::in_reach(world, actor.mob, over) {
        return true;
    }
    let what = name_of(world, over);
    let user = name_of(world, actor.mob);
    let at = world.get::<TilePos>(actor.mob).copied().unwrap_or_default();
    // Climbing takes twice as long without help from the hands.
    let hands_blocked = crate::status_effects::incapacitation(world, actor.mob).hands_blocked;
    let delay = CLIMB_TIME_DS * if hands_blocked { 2.0 } else { 1.0 };
    if do_after(world, actor, over, delay, 0, Job::Table(TableJob::Climb)) {
        say(
            world,
            at,
            Span::Warning,
            format!("{user} начинает подниматься на {what}."),
            &[(actor.mob, format!("Вы начинаете подниматься на {what}..."))],
        );
    }
    true
}

/// tg `climbable/do_climb` onto the target's tile, the stun, and the operating table's buckle.
fn climb(world: &mut World, action: &DoAfter, mob: Entity) {
    let target = action.target;
    if flipped(world, target) || !crate::storage::in_reach(world, mob, target) {
        return;
    }
    let what = name_of(world, target);
    let landed = if is_railing(world, target) {
        vault(world, mob, target)
    } else {
        onto_tile(world, mob, action.target_pos, target)
    };
    let to = world.get::<TilePos>(mob).copied().filter(|_| landed);
    let Some(to) = to else {
        tell(
            world,
            action.client,
            Span::Warning,
            format!("Вам не удаётся взобраться на {what}."),
        );
        return;
    };
    let user = name_of(world, mob);
    say(
        world,
        to,
        Span::Warning,
        format!("{user} взбирается на {what}."),
        &[(mob, format!("Вы взобрались на {what}."))],
    );
    crate::status_effects::apply(world, mob, Incap::Stun, CLIMB_STUN);
    let path = path_of(world, target).unwrap_or_default();
    if crate::medical::surgery::buckles_lying(&path) {
        let actor = Actor {
            client: action.client,
            mob,
            item: None,
        };
        crate::medical::surgery::buckle(world, actor, mob, target);
    }
    after_landing(world, mob, target);
}

/// tg `climbable/do_climb` of a border railing: one step with the railing out of the way, over
/// its edge (into the next tile) from its own tile or from the side it does not face.
// shortcut: a diagonal railing is vaulted from its own tile over its vertical edge (tg picks the
// edge by where the drag ended on the icon); a diagonal vault from another tile fails.
fn vault(world: &mut World, mob: Entity, railing: Entity) -> bool {
    let (Some(&from), Some(&at)) = (world.get::<TilePos>(mob), world.get::<TilePos>(railing))
    else {
        return false;
    };
    let Some(facing) = world
        .get::<Border>(railing)
        .and_then(|b| Dir::from_bits(b.edges))
    else {
        return false;
    };
    let toward = |to: TilePos| {
        let (dx, dy) = ((to.x - from.x).signum(), (to.y - from.y).signum());
        Dir::DMI_ORDER.into_iter().find(|d| d.offset() == (dx, dy))
    };
    let same_loc = from == at;
    let mut step = toward(at);
    if same_loc || step.is_some_and(|d| d as u8 & facing.reverse() as u8 == 0) {
        step = toward(at.step(facing));
        if same_loc && !Dir::CARDINALS.contains(&facing) {
            step = Some(if facing as u8 & Dir::North as u8 != 0 {
                Dir::North
            } else {
                Dir::South
            });
        }
    }
    let Some(step) = step.filter(|d| Dir::CARDINALS.contains(d)) else {
        return false;
    };
    step_passing(world, mob, step, |_, e| e == railing).is_ok()
}

/// `mob` onto `to` (the tile of `surface`), past every table there but nothing else dense:
/// tg `step` with the climbed thing's density off, or `Move` with `PASSTABLE`.
fn onto_tile(world: &mut World, mob: Entity, to: TilePos, surface: Entity) -> bool {
    let blocked = (world.resource::<SpatialIndex>().dense_on_tile(to).iter())
        .any(|&e| e != mob && e != surface && !is_table(world, e));
    if blocked
        || world
            .get::<TilePos>(mob)
            .and_then(|p| p.chebyshev(to))
            .is_none_or(|d| d > 1)
    {
        return false;
    }
    world.entity_mut(mob).insert(to);
    true
}

/// A living mob landed on `table`: a glass one breaks (tg `glass/check_break`).
fn after_landing(world: &mut World, mob: Entity, table: Entity) {
    if obj_path(world, table).is_some_and(|p| is_under(p, GLASS_TABLE)) && !flipped(world, table) {
        shatter(world, table, Some(mob));
    }
}

/// tg `table_shatter`: the frame and a shard stay, the victim is paralyzed.
fn shatter(world: &mut World, table: Entity, victim: Option<Entity>) {
    let Some(&pos) = world.get::<TilePos>(table) else {
        return;
    };
    let dm = world.get::<DmTable>(table).cloned().unwrap_or_default();
    let what = name_of(world, table);
    say(
        world,
        pos,
        Span::Warning,
        format!("{what} разбивается!"),
        &[],
    );
    sound(world, pos, sounds::SHATTER, 50.0);
    let frame = frame_of(world, table, &dm);
    world.despawn(table);
    spawn_obj(world, &frame, pos);
    spawn_obj(world, &dm.glass_shard, pos);
    tile_changed(world, pos);
    reelevate(world, pos);
    if let Some(victim) = victim {
        crate::status_effects::apply(world, victim, Incap::Paralyze, SHATTER_PARALYZE);
    }
}

/// The frame a table leaves: the one it was built on, else its type's.
fn frame_of(world: &World, table: Entity, dm: &DmTable) -> String {
    world
        .get::<TableState>(table)
        .and_then(|t| t.built_on.as_ref())
        .map_or_else(|| dm.frame.clone(), |b| b.0.clone())
}

/// tg `table_smash/perform_table_smash`.
fn smash(world: &mut World, actor: Actor, pulled: Entity, table: Entity) {
    let (user, victim, what) = (
        name_of(world, actor.mob),
        name_of(world, pulled),
        name_of(world, table),
    );
    if let Some(buckled) = world
        .get::<crate::medical::surgery::Buckled>(pulled)
        .map(|b| b.0)
    {
        if buckled != table {
            let to = name_of(world, buckled);
            tell(
                world,
                actor.client,
                Span::Warning,
                format!("{victim} пристёгнут к {to}!"),
            );
        }
        return;
    }
    let at = world.get::<TilePos>(pulled).copied().unwrap_or_default();
    if !combat(world, actor.mob) {
        if do_after(
            world,
            actor,
            pulled,
            TABLE_PLACE_DS,
            0,
            Job::Table(TableJob::Place { table }),
        ) {
            say(
                world,
                at,
                Span::Notice,
                format!("{user} начинает укладывать {victim} на {what}..."),
                &[(
                    pulled,
                    format!("{user} начинает укладывать вас на {what}..."),
                )],
            );
        }
        return;
    }
    let grab = world
        .get::<GrabState>(actor.mob)
        .copied()
        .unwrap_or_default();
    if grab == GrabState::Passive {
        tell(
            world,
            actor.client,
            Span::Warning,
            "Нужно схватить покрепче!",
        );
        return;
    }
    let gentle = world.get::<DmTable>(table).is_some_and(|t| t.slam_gently);
    if gentle {
        lay_on(world, actor.mob, pulled, table);
    } else {
        slam(world, actor.mob, pulled, table);
    }
    world.entity_mut(actor.mob).remove::<Pulling>();
}

/// tg `tableplace`.
fn lay_on(world: &mut World, mob: Entity, pulled: Entity, table: Entity) {
    let Some(&to) = world.get::<TilePos>(table) else {
        return;
    };
    world.entity_mut(pulled).insert(to);
    // `set_resting(TRUE, TRUE)`.
    crate::rest::set_resting(world, None, pulled, true);
    let (user, victim, what) = (
        name_of(world, mob),
        name_of(world, pulled),
        name_of(world, table),
    );
    say(
        world,
        to,
        Span::Notice,
        format!("{user} укладывает {victim} на {what}."),
        &[],
    );
    after_landing(world, pulled, table);
}

/// tg `tablepush`.
fn slam(world: &mut World, mob: Entity, pulled: Entity, table: Entity) {
    let Some(&to) = world.get::<TilePos>(table) else {
        return;
    };
    let blocked = (world.resource::<SpatialIndex>().dense_on_tile(to).iter())
        .any(|&e| e != pulled && !is_table(world, e));
    if blocked {
        return;
    }
    world.entity_mut(pulled).insert(to);
    sound(world, to, sounds::SLAM, 90.0);
    let (user, victim, what) = (
        name_of(world, mob),
        name_of(world, pulled),
        name_of(world, table),
    );
    say(
        world,
        to,
        Span::Danger,
        format!("{user} швыряет {victim} на {what}!"),
        &[(pulled, format!("{user} швыряет вас на {what}!"))],
    );
    knockdown(world, pulled, SLAM_KNOCKDOWN, 0.0);
    adjust_stamina(world, pulled, SLAM_STAMINA);
    hurt(world, pulled, DamageType::Brute, SLAM_BRUTE);
    after_landing(world, pulled, table);
}

fn hurt(world: &mut World, mob: Entity, kind: DamageType, amount: f32) {
    if let (Some(mut health), Some(&stat)) =
        (world.get::<Health>(mob).cloned(), world.get::<Stat>(mob))
    {
        let mut new_stat = stat;
        apply_damage(&mut health, &mut new_stat, kind, amount);
        world.entity_mut(mob).insert(health);
        if new_stat != stat {
            world.entity_mut(mob).insert(new_stat);
        }
    }
}

/// tg `on_disarm_shoved_into` for `crate::security`'s shove: `target` was shoved toward `to` and
/// blocked; a standing table there takes it. False when there is none.
pub fn shoved_into(world: &mut World, shover: Entity, target: Entity, to: TilePos) -> bool {
    let table = (world.resource::<SpatialIndex>().on_tile(to).iter().copied())
        .find(|&e| is_table(world, e) && !flipped(world, e));
    let Some(table) = table else {
        return false;
    };
    knockdown(world, target, SHOVE_KNOCKDOWN_TABLE, SHOVE_DAZE);
    let (user, victim, what) = (
        name_of(world, shover),
        name_of(world, target),
        name_of(world, table),
    );
    let at = world.get::<TilePos>(target).copied().unwrap_or(to);
    say(
        world,
        at,
        Span::Danger,
        format!("{user} толкает {victim} на {what}!"),
        &[
            (target, format!("{user} толкает вас на {what}!")),
            (shover, format!("Вы толкаете {victim} на {what}!")),
        ],
    );
    // `throw_at(src, 1, 1)`: a one-tile throw lands on the table (`LETPASSTHROW`).
    if onto_tile(world, target, to, table) {
        after_landing(world, target, table);
    }
    true
}

/// Tools and stacks on tables, frames, racks and rack parts (tg `screwdriver_act_secondary`,
/// `wrench_act_secondary`, `welder_act_secondary`, frame `wrench_act` and `item_interaction`,
/// rack parts `wrench_act`). False when nothing here takes the click.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    let tool = tool_of(world, held).map(|(kind, _)| kind);
    if is_table(world, target) {
        if !modifiers.right {
            return false;
        }
        let ready = world.get::<TableState>(target).is_some_and(|t| t.ready);
        let reinforced = obj_path(world, target).is_some_and(|p| is_under(p, REINFORCED_TABLE));
        let (delay, job, text) = match tool {
            Some(ToolKind::Screwdriver) if ready => {
                (SCREW_DS, TableJob::Screw, "Вы начинаете разбирать стол...")
            }
            Some(ToolKind::Wrench) if ready => (
                WRENCH_DS,
                TableJob::Wrench,
                "Вы начинаете разбирать стол...",
            ),
            Some(ToolKind::Welder) if reinforced => (
                WELD_DS,
                TableJob::Weld,
                if ready {
                    "Вы начинаете усиливать укрепленный стол..."
                } else {
                    "Вы начинаете ослаблять укрепленный стол..."
                },
            ),
            _ => return false,
        };
        if use_tool(world, actor, target, delay, 0, Job::Table(job)) {
            tell(world, client, Span::Notice, text);
        }
        return true;
    }
    if is_frame(world, target) {
        if tool == Some(ToolKind::Wrench) {
            use_tool(
                world,
                actor,
                target,
                FRAME_WRENCH_DS,
                0,
                Job::Table(TableJob::FrameApart),
            );
            return true;
        }
        if table_type(world, target, held).is_none() {
            return false;
        }
        if let Some(blocker) = construction_blocker(world, target) {
            let name = name_of(world, blocker);
            tell(world, client, Span::Warning, format!("Мешает {name}!"));
            return true;
        }
        if do_after(
            world,
            actor,
            target,
            FRAME_BUILD_DS,
            1,
            Job::Table(TableJob::Build),
        ) {
            tell(world, client, Span::Notice, "Вы начинаете собирать стол...");
        }
        return true;
    }
    if is_rack(world, target) && modifiers.right && tool == Some(ToolKind::Wrench) {
        rack_apart(world, target);
        return true;
    }
    let parts = world
        .get::<DmItem>(target)
        .is_some_and(|i| is_under(&i.path, RACK_PARTS));
    if parts && tool == Some(ToolKind::Wrench) && world.get::<CarriedBy>(target).is_none() {
        if let Some(&pos) = world.get::<TilePos>(target) {
            world.despawn(target);
            spawn_stack(world, IRON, pos, 1);
        }
        return true;
    }
    false
}

/// tg `table_frame/get_table_type`: the held stack's `table_type`, wooden frames only wood
/// (`table_frame/wood/get_table_type`: planks a wooden table, carpet a poker table).
fn table_type(world: &World, frame: Entity, stack: Entity) -> Option<String> {
    let dm = world.get::<DmStack>(stack)?;
    let path = &world.get::<DmItem>(stack)?.path;
    let frame_path = obj_path(world, frame)?;
    if is_under(frame_path, "/obj/structure/table_frame/wood") {
        if is_under(path, "/obj/item/stack/sheet/mineral/wood") {
            return Some("/obj/structure/table/wood".to_owned());
        }
        if is_under(path, "/obj/item/stack/tile/carpet") {
            return Some("/obj/structure/table/wood/poker".to_owned());
        }
        return None;
    }
    // shortcut: greyscale tables need a material colour (`material_type`), not ported.
    (!dm.table_type.is_empty() && !is_under(&dm.table_type, "/obj/structure/table/greyscale"))
        .then(|| dm.table_type.clone())
}

/// tg's check of `table_frame/item_interaction`: a dense object (not a table-passing one) on the
/// frame's tile.
fn construction_blocker(world: &World, frame: Entity) -> Option<Entity> {
    let pos = *world.get::<TilePos>(frame)?;
    (world
        .resource::<SpatialIndex>()
        .dense_on_tile(pos)
        .iter()
        .copied())
    .find(|&e| e != frame && world.get::<DmObj>(e).is_some() && !is_table(world, e))
}

fn rack_apart(world: &mut World, rack: Entity) {
    let Some(&pos) = world.get::<TilePos>(rack) else {
        return;
    };
    world.despawn(rack);
    spawn_obj(world, RACK_PARTS, pos);
    reelevate(world, pos);
}

/// tg `rack_parts/attack_self`: a rack after 5 s, on the user's tile.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if !world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, RACK_PARTS))
    {
        return false;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(item),
    };
    if do_after(
        world,
        actor,
        mob,
        RACK_BUILD_DS,
        0,
        Job::Table(TableJob::Rack),
    ) {
        tell(
            world,
            client,
            Span::Notice,
            "Вы начинаете собирать стеллаж...",
        );
    }
    true
}

/// End of a [`TableJob`] `do_after` (`crate::tools` checked that nobody moved).
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: TableJob) {
    let target = action.target;
    let client = action.client;
    match job {
        TableJob::Flip => {
            if is_table(world, target) {
                toggle_flip(world, mob, target);
            }
        }
        TableJob::Climb => climb(world, action, mob),
        TableJob::Place { table } => {
            let pulling = world.get::<Pulling>(mob).map(|p| p.0) == Some(target);
            if pulling && is_table(world, table) && !flipped(world, table) {
                lay_on(world, mob, target, table);
                world.entity_mut(mob).remove::<Pulling>();
            }
        }
        TableJob::Screw | TableJob::Wrench => {
            if is_table(world, target) && world.get::<TableState>(target).is_some_and(|t| t.ready) {
                take_apart(world, target, job == TableJob::Screw);
            }
        }
        TableJob::Weld => {
            if let Some(mut state) = world.get_mut::<TableState>(target) {
                state.ready = !state.ready;
                let text = if state.ready {
                    "Вы ослабляете стол."
                } else {
                    "Вы укрепляете стол."
                };
                tell(world, client, Span::Notice, text);
            }
        }
        TableJob::FrameApart => {
            if is_frame(world, target) {
                let dm = world.get::<DmTable>(target).cloned().unwrap_or_default();
                let pos = action.target_pos;
                world.despawn(target);
                sound(world, pos, sounds::DECONSTRUCT, 50.0);
                spawn_stack(world, &dm.framestack, pos, dm.framestack_amount);
            }
        }
        TableJob::Build => build_on_frame(world, mob, action),
        TableJob::Rack => {
            let held = world.get::<Inventory>(mob).and_then(Inventory::active_item);
            let Some(&pos) = world.get::<TilePos>(mob) else {
                return;
            };
            if held != action.item || held.is_none() {
                return;
            }
            let parts = held.expect("checked");
            world.despawn(parts);
            if spawn_obj(world, RACK, pos).is_some() {
                let user = name_of(world, mob);
                say(
                    world,
                    pos,
                    Span::Notice,
                    format!("{user} собирает стеллаж."),
                    &[(mob, "Вы собираете стеллаж.".to_owned())],
                );
            }
            reelevate(world, pos);
        }
    }
}

/// tg `deconstruct(TRUE)` -> `atom_deconstruct`: the top's stacks, and the frame (screwdriver)
/// or what the frame is made of (wrench).
fn take_apart(world: &mut World, table: Entity, keep_frame: bool) {
    let Some(&pos) = world.get::<TilePos>(table) else {
        return;
    };
    let dm = world.get::<DmTable>(table).cloned().unwrap_or_default();
    let frame = frame_of(world, table, &dm);
    let (framestack, framestack_amount) = world
        .get::<TableState>(table)
        .and_then(|t| t.built_on.clone())
        .map_or((dm.framestack.clone(), dm.framestack_amount), |b| {
            (b.1, b.2)
        });
    world.despawn(table);
    if !keep_frame {
        sound(world, pos, sounds::DECONSTRUCT, 50.0);
    }
    spawn_stack(world, &dm.buildstack, pos, dm.buildstack_amount);
    if keep_frame {
        spawn_obj(world, &frame, pos);
    } else {
        spawn_stack(world, &framestack, pos, framestack_amount);
    }
    tile_changed(world, pos);
    reelevate(world, pos);
}

/// tg `table_frame/item_interaction` after its `do_after`: one unit of the stack, the table.
fn build_on_frame(world: &mut World, mob: Entity, action: &DoAfter) {
    let frame = action.target;
    let held = world.get::<Inventory>(mob).and_then(Inventory::active_item);
    let Some(stack) = held.filter(|&h| Some(h) == action.item) else {
        return;
    };
    if !is_frame(world, frame) || construction_blocker(world, frame).is_some() {
        return;
    }
    let Some(table_type) = table_type(world, frame, stack) else {
        return;
    };
    if !crate::tools::consume(world, stack, 1) {
        return;
    }
    let pos = action.target_pos;
    let frame_path = obj_path(world, frame).unwrap_or_default().to_owned();
    let dm = world.get::<DmTable>(frame).cloned().unwrap_or_default();
    world.despawn(frame);
    if let Some(table) = spawn_obj(world, &table_type, pos)
        && let Some(mut state) = world.get_mut::<TableState>(table)
    {
        state.built_on = Some((frame_path, dm.framestack, dm.framestack_amount));
    }
    tile_changed(world, pos);
    reelevate(world, pos);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_railing_is_vaulted_over_its_edge() {
        let mut app = App::new();
        app.add_plugins(ss13_world::WorldPlugin);
        let world = app.world_mut();
        world
            .resource_mut::<SpatialIndex>()
            .create_level(1, 10, 10)
            .unwrap();
        let at = TilePos::new(5, 5, 1);
        let railing = world
            .spawn((at, Density, Border::facing(Dir::North, true)))
            .id();
        let mob = world.spawn((TilePos::new(5, 4, 1), Density)).id();
        // From the side it does not face: onto its tile.
        assert!(vault(world, mob, railing));
        assert_eq!(world.get::<TilePos>(mob), Some(&at));
        // Walking north off its tile is blocked; vaulting crosses the edge.
        assert!(crate::pull::step_once(world, mob, Dir::North, None).is_err());
        assert!(vault(world, mob, railing));
        assert_eq!(world.get::<TilePos>(mob), Some(&TilePos::new(5, 6, 1)));
        // From the side it faces: back over the edge onto its tile.
        assert!(vault(world, mob, railing));
        assert_eq!(world.get::<TilePos>(mob), Some(&at));
    }
}
