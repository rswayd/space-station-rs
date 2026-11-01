//! Closets, lockers and crates (server), after tg `/obj/structure/closet`
//! (`code/game/objects/structures/crates_lockers/closets.dm`, `crates.dm`).
//!
//! - A closed closet holds its contents as [`StoredIn`] entities (off the map, not replicated);
//!   opening moves them onto its tile (`dump_contents`), closing takes the items lying there
//!   (`take_contents`, up to `storage_capacity`).
//! - Contents from code: `populate_contents_immediate()` runs at startup, `PopulateContents()` on
//!   the first opening (tg `contents_initialized`), both from [`DmContents`].
//! - At startup a closed closet takes the items mapped on its tile (tg `LateInitialize` ->
//!   `take_contents`); a closet mapped open (`opened = TRUE`) opens instead and fills at once.
//! - Clicks (tg `attack_hand` -> `toggle`, else `togglelock`; `item_interaction` with an ID ->
//!   `togglelock`; with any other item on an open closet -> onto its tile): secure closets unlock
//!   with the clicker's ID access, through the doors' [`check_access`].
//! - Mobs: closing takes the living mobs on the tile inside ([`InCloset`]: tg `insert`, up to
//!   `mob_storage_capacity`; a crate does not close over a standing mob), opening lets them out,
//!   a step of a mob inside opens an unlocked closet (tg `relaymove` -> `container_resist_act`).
//!   Dragging an item or a mob from the floor onto an open closet (`mouse_drop_receive`, from
//!   `crate::drag`) puts the item on its tile, or stuffs the mob in after a 4 s `do_after`
//!   (paralyzed 4 s, the closet closes).
//! - The look is tg `update_icon`: closets keep their `icon_state` and draw the door
//!   (`<state>_open` / `<icon_door or state>_door`), `welded` and `locked`/`unlocked` as
//!   overlays; crates switch to `<base>open` and draw their lock lights; the layer drops to
//!   `BELOW_OBJ_LAYER` while open.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmCloset, DmContents, DmItem, IconIndex};
use ss13_shared::appearance::Overlay;
use ss13_shared::{Appearance, IconStateId, Layer, TilePos};
use ss13_world::{Density, Mover, SpatialIndex, WorldSystems};

use crate::click::{ClickResolved, ResolveClicks};
use crate::inventory::{CarriedBy, IdCard, Inventory, check_access};
use crate::storage::{Stored, StoredIn, put_inside, spawn_contents, take_out};

/// BYOND `OBJ_LAYER`.
const OBJ_LAYER: f32 = 3.0;
/// `BELOW_OBJ_LAYER` (`code/__DEFINES/layers.dm`).
const BELOW_OBJ_LAYER: f32 = 2.9;

/// Live closet state, built once from [`DmCloset`].
#[derive(Component, Debug, Clone)]
pub struct Closet {
    pub opened: bool,
    pub locked: bool,
    pub welded: bool,
    pub secure: bool,
    pub req_access: Vec<String>,
    pub req_one_access: Vec<String>,
    /// `PopulateContents()` already ran.
    pub contents_initialized: bool,
    dense_when_open: bool,
    wall_mounted: bool,
    /// Crates: no closing over a standing mob.
    horizontal: bool,
    storage_capacity: usize,
    sprites: ClosetSprites,
}

impl Closet {
    pub fn from_dm(dm: &DmCloset, atom: &DmAtom, icons: Option<&IconIndex>) -> Self {
        Self {
            opened: dm.opened,
            locked: dm.locked,
            welded: dm.welded,
            secure: dm.secure,
            req_access: dm.req_access.clone(),
            req_one_access: dm.req_one_access.clone(),
            contents_initialized: false,
            dense_when_open: dm.dense_when_open,
            wall_mounted: dm.wall_mounted,
            horizontal: dm.horizontal,
            storage_capacity: dm.storage_capacity as usize,
            sprites: icons.map_or_else(ClosetSprites::default, |icons| {
                ClosetSprites::resolve(dm, atom, icons)
            }),
        }
    }

    /// tg `can_open` without the mob checks: welded or locked closets stay shut.
    // shortcut: no "something large on top" check (anchored or dense mobs on a horizontal
    // closet) and no strong-pull check; add with buckling and pulling.
    pub fn can_open(&self) -> bool {
        !self.opened && !self.welded && !self.locked
    }

    /// tg `togglelock` access check: only unlocking needs access.
    pub fn may_toggle_lock(&self, access: Option<&[String]>) -> bool {
        self.secure
            && (!self.locked
                || check_access(
                    &self.req_access,
                    &self.req_one_access,
                    access.unwrap_or_default(),
                ))
    }

    fn apply_look(&self, appearance: &mut Appearance) {
        let s = &self.sprites;
        if let Some(base) = if self.opened { s.open } else { s.closed } {
            appearance.icon_state = base;
        }
        appearance.layer = Layer(if self.opened {
            BELOW_OBJ_LAYER
        } else {
            OBJ_LAYER
        });
        let mut overlays = Vec::new();
        if s.is_crate {
            if !self.opened {
                overlays.push(if self.locked {
                    s.crate_locked
                } else if self.secure {
                    s.crate_unlocked
                } else {
                    None
                });
            }
            overlays.push(s.welded.filter(|_| self.welded));
        } else {
            overlays.push(if self.opened {
                s.door_open
            } else {
                s.door_closed
            });
            if !self.opened {
                overlays.push(s.welded.filter(|_| self.welded));
                if self.secure {
                    overlays.push(if self.locked { s.locked } else { s.unlocked });
                }
            }
        }
        appearance.overlays = overlays.into_iter().flatten().map(Overlay::new).collect();
    }
}

/// Icon states of each look, resolved once per closet from its icon.
// shortcut: no emissive lock lights (no emissive plane yet), no `broken` sparks (no damage),
// no crate lids (`lid_icon_state`) or manifests, no door animation (`animate_door`).
#[derive(Debug, Clone, Default)]
struct ClosetSprites {
    is_crate: bool,
    closed: Option<IconStateId>,
    open: Option<IconStateId>,
    door_open: Option<IconStateId>,
    door_closed: Option<IconStateId>,
    welded: Option<IconStateId>,
    locked: Option<IconStateId>,
    unlocked: Option<IconStateId>,
    crate_locked: Option<IconStateId>,
    crate_unlocked: Option<IconStateId>,
}

impl ClosetSprites {
    fn resolve(dm: &DmCloset, atom: &DmAtom, icons: &IconIndex) -> Self {
        let get = |state: &str| icons.get(&atom.icon, state);
        // `isnull(base_icon_state) ? initial(icon_state) : base_icon_state`.
        let base = if dm.base_icon_state.is_empty() {
            atom.icon_state.as_str()
        } else {
            dm.base_icon_state.as_str()
        };
        if dm.is_crate {
            return Self {
                is_crate: true,
                closed: get(base),
                open: get(&format!("{base}open")),
                welded: get(&dm.icon_welded),
                crate_locked: get("securecrater"),
                crate_unlocked: get("securecrateg"),
                ..default()
            };
        }
        let door = if dm.icon_door.is_empty() {
            base
        } else {
            dm.icon_door.as_str()
        };
        let overlay = |on: bool, state: String| if on { get(&state) } else { None };
        Self {
            is_crate: false,
            closed: None,
            open: None,
            door_open: overlay(
                dm.enable_door_overlay && dm.has_opened_overlay,
                format!("{base}_open"),
            ),
            door_closed: overlay(
                dm.enable_door_overlay && dm.has_closed_overlay,
                format!("{door}_door"),
            ),
            welded: get(&dm.icon_welded),
            locked: get("locked"),
            unlocked: get("unlocked"),
            ..default()
        }
    }
}

/// Closets recognized at startup.
#[derive(Resource, Debug, Default, Clone)]
pub struct ClosetReport {
    pub closets: usize,
    /// Closets whose contents program has statements that were not understood.
    pub partial: usize,
    /// Items found on closed closets' tiles and taken inside.
    pub taken_at_start: usize,
    pub opened_at_start: usize,
}

/// `/obj/structure/closet::mob_storage_capacity`.
const MOB_STORAGE_CAPACITY: usize = 3;
/// `closet/mouse_drop_receive`: stuffing a mob in takes `do_after(user, 4 SECONDS, O)` and
/// `Paralyze(40)`.
pub const STUFF_DELAY_DS: f32 = 40.0;
const STUFF_PARALYZE_SECS: f32 = 4.0;

/// A living mob inside a closed closet (tg `loc` is the closet). The mob keeps its tile (the
/// closet's) so its view and the systems that follow it stay; it is drawn fully transparent and
/// cannot be clicked. `look` is what it had before.
// shortcut: hidden by its look, not by replication (others still get the entity, invisible),
// and the mob's lungs, fire and throws act as on the tile; give mobs a real container
// location when more containers hold mobs (morgue trays, disposals).
#[derive(Component, Clone, Copy, Debug)]
pub struct InCloset {
    pub closet: Entity,
    look: (Option<[u8; 4]>, ss13_shared::MouseOpacity),
}

/// A step of a mob inside a closet (written by `crate::server` instead of moving it).
#[derive(Message, Clone, Copy, Debug)]
pub struct ClosetRelayMove {
    pub mob: Entity,
}

pub struct ClosetServerPlugin;

impl Plugin for ClosetServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ClosetReport>()
            .add_systems(Startup, init_closets.after(crate::storage::StorageStartup))
            .add_message::<ClosetRelayMove>()
            .add_systems(
                Update,
                (
                    closet_clicks
                        .after(ResolveClicks)
                        .before(crate::inventory::inventory_intents),
                    relay_moves.after(WorldSystems::Movement),
                    hide_closeted.after(crate::inventory::inventory_intents),
                ),
            )
            .add_observer(release_on_despawn);
    }
}

/// [`Closet`] for every [`DmCloset`]; immediate contents; mapped items on closed closets go in.
fn init_closets(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmCloset, &DmAtom, Option<&DmContents>), (
        Without<Closet>,
        With<TilePos>,
    )>();
    let icons = world.get_resource::<IconIndex>();
    let found: Vec<(Entity, Closet, Option<DmContents>)> = query
        .iter(world)
        .map(|(e, dm, atom, contents)| (e, Closet::from_dm(dm, atom, icons), contents.cloned()))
        .collect();
    let mut report = ClosetReport {
        closets: found.len(),
        ..default()
    };
    // Same order for the same map, so the seed gives the same contents.
    let mut found = found;
    found.sort_by_key(|(e, ..)| *e);
    for (entity, closet, contents) in found {
        let opened = closet.opened;
        world.entity_mut(entity).insert(Closet {
            opened: false,
            ..closet
        });
        if let Some(contents) = &contents {
            report.partial += usize::from(contents.unsupported > 0);
            spawn_contents(world, entity, &contents.immediate, 0);
        }
        if opened {
            // tg `Initialize`: closets mapped open open without effects (and fill now).
            open(world, entity);
            report.opened_at_start += 1;
        } else {
            report.taken_at_start += take_contents(world, entity);
            refresh_look(world, entity);
        }
    }
    info!(
        "closets: {} ({} with partly understood contents), {} opened at start, {} mapped items \
         taken inside",
        report.closets, report.partial, report.opened_at_start, report.taken_at_start
    );
    world.insert_resource(report);
}

pub(crate) fn refresh_look(world: &mut World, closet: Entity) {
    let Some(state) = world.get::<Closet>(closet).cloned() else {
        return;
    };
    if let Some(mut appearance) = world.get_mut::<Appearance>(closet) {
        let mut look = appearance.clone();
        state.apply_look(&mut look);
        appearance.set_if_neq(look);
    }
}

/// tg `open()` (`force` skips the lock and weld checks). Fills on the first opening, then
/// moves the contents onto the closet's tile.
pub fn open(world: &mut World, closet: Entity) -> bool {
    let Some(state) = world.get::<Closet>(closet) else {
        return false;
    };
    if state.opened {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(closet) else {
        return false;
    };
    let dense_when_open = state.dense_when_open;
    let initialized = state.contents_initialized;
    {
        let mut state = world.get_mut::<Closet>(closet).expect("checked above");
        state.welded = false;
        state.locked = false;
        state.opened = true;
        state.contents_initialized = true;
    }
    if !dense_when_open {
        world.entity_mut(closet).remove::<Density>();
    }
    if !initialized && let Some(contents) = world.get::<DmContents>(closet).cloned() {
        spawn_contents(world, closet, &contents.populate, 0);
    }
    let contents: Vec<Entity> = world
        .get::<Stored>(closet)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    for item in contents {
        take_out(world, item, pos);
    }
    let mut inside = world.query::<(Entity, &InCloset)>();
    let mobs: Vec<Entity> = inside
        .iter(world)
        .filter(|(_, c)| c.closet == closet)
        .map(|(e, _)| e)
        .collect();
    for mob in mobs {
        let_out(world, mob);
    }
    refresh_look(world, closet);
    true
}

/// Out of its closet: back to its look (tg `dump_contents` moves it onto the closet's turf, where
/// it already is).
pub(crate) fn let_out(world: &mut World, mob: Entity) {
    let Some(inside) = world.entity_mut(mob).take::<InCloset>() else {
        return;
    };
    if let Some(mut appearance) = world.get_mut::<Appearance>(mob) {
        (appearance.color, appearance.mouse_opacity) = inside.look;
    }
    // The look is recomputed from the inventory (dressed humans).
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.set_changed();
    }
}

/// A despawned closet lets its mobs out where it stood.
fn release_on_despawn(
    remove: On<Remove<Closet>>,
    inside: Query<(Entity, &InCloset)>,
    mut commands: Commands,
) {
    for (mob, c) in &inside {
        if c.closet == remove.entity {
            commands.queue(move |world: &mut World| let_out(world, mob));
        }
    }
}

/// tg `insertion_allowed` for a living mob on the tile: not in another closet, not anchored, and
/// a crate takes only lying mobs.
fn insertable(world: &World, closet: &Closet, mob: Entity) -> bool {
    let entity = world.entity(mob);
    entity.contains::<crate::health::Health>()
        && !entity.contains::<InCloset>()
        && !(closet.horizontal && entity.contains::<Density>())
}

/// Every closet's mobs stay invisible and unclickable, also after their look is rebuilt.
fn hide_closeted(
    mut mobs: Query<&mut Appearance, (With<InCloset>, Or<(Changed<Appearance>, Added<InCloset>)>)>,
) {
    for mut appearance in &mut mobs {
        let hidden = (
            Some([255, 255, 255, 0]),
            ss13_shared::MouseOpacity::Transparent,
        );
        if (appearance.color, appearance.mouse_opacity) != hidden {
            (appearance.color, appearance.mouse_opacity) = hidden;
        }
    }
}

/// tg `/obj/structure/closet/relaymove` -> `container_resist_act`: a step opens the closet
/// unless it is welded or locked.
// shortcut: no breaking out of a locked or welded closet (tg's 2 minute resist); someone has to
// open it from outside.
fn relay_moves(world: &mut World, mut moves: Local<MessageCursor<ClosetRelayMove>>) {
    let mobs: Vec<Entity> = moves
        .read(world.resource::<Messages<ClosetRelayMove>>())
        .map(|m| m.mob)
        .collect();
    for mob in mobs {
        // `IS_UNCONSCIOUS_OR_CRIT(user)`: nothing.
        if world.get::<crate::health::Stat>(mob) != Some(&crate::health::Stat::Conscious) {
            continue;
        }
        if let Some(c) = world.get::<InCloset>(mob).copied() {
            open_if_allowed(world, c.closet);
        }
    }
}

/// `closet/mouse_drop_receive` (from `crate::drag`): `dropped` lies on the floor next to `user`
/// and the closet is open. An item goes onto the closet's tile at once; a living mob is stuffed
/// in after a `do_after`. True when the drop was the closet's.
pub fn mouse_drop_receive(
    world: &mut World,
    actor: crate::tools::Actor,
    closet: Entity,
    dropped: Entity,
) -> bool {
    let Some(state) = world.get::<Closet>(closet) else {
        return false;
    };
    // `user == O`: climbing onto it (buckling), which closets here do not have.
    if !state.opened || dropped == actor.mob {
        return state.opened;
    }
    let (Some(&pos), Some(&at)) = (world.get::<TilePos>(closet), world.get::<TilePos>(dropped))
    else {
        return true;
    };
    let entity = world.entity(dropped);
    let mob = entity.contains::<crate::health::Health>();
    let anchored = entity.get::<DmItem>().is_some_and(|i| i.anchored);
    if anchored || (!mob && !entity.contains::<DmItem>()) {
        return true;
    }
    let (user, what) = (
        crate::security::name_of(world, actor.mob),
        crate::security::name_of(world, dropped),
    );
    let tries = if mob {
        "пытается запихнуть"
    } else {
        "запихивает"
    };
    crate::security::show(
        world,
        at,
        crate::health::DEFAULT_MESSAGE_RANGE,
        ss13_shared::chat::ChatChannel::Combat,
        (
            ss13_shared::chat::Span::Warning,
            format!("{user} {tries} {what} в шкаф."),
        ),
        &[],
    );
    if !mob {
        world.entity_mut(dropped).insert(pos);
        return true;
    }
    crate::tools::do_after(
        world,
        actor,
        dropped,
        STUFF_DELAY_DS,
        0,
        crate::construction::Job::Interact(crate::drag::DragJob::StuffCloset { closet, at: pos }),
    );
    true
}

/// End of the stuffing `do_after` (the mob did not move, nor did the user). The victim goes to
/// `pos`, the closet's tile at the start (tg `forceMove(T)`), so a closet dragged away since
/// closes without it instead of pulling it along.
pub fn complete_stuff(
    world: &mut World,
    mob: Entity,
    victim: Entity,
    closet: Entity,
    pos: TilePos,
) {
    if !world.get::<Closet>(closet).is_some_and(|c| c.opened)
        || world.get::<crate::health::Health>(victim).is_none()
        || world.get::<InCloset>(victim).is_some()
    {
        return;
    }
    let (user, what) = (
        crate::security::name_of(world, mob),
        crate::security::name_of(world, victim),
    );
    crate::security::show(
        world,
        pos,
        crate::health::DEFAULT_MESSAGE_RANGE,
        ss13_shared::chat::ChatChannel::Combat,
        (
            ss13_shared::chat::Span::Notice,
            format!("{user} запихивает {what} в шкаф."),
        ),
        &[],
    );
    crate::status_effects::apply(
        world,
        victim,
        crate::status_effects::Incap::Paralyze,
        STUFF_PARALYZE_SECS,
    );
    world.entity_mut(victim).insert(pos);
    close(world, closet);
}

/// tg `can_close`: no other closet on the tile, no mob it cannot take (a standing mob under a
/// crate).
fn can_close(world: &World, closet: Entity, pos: TilePos) -> bool {
    let Some(state) = world.get::<Closet>(closet) else {
        return false;
    };
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .filter(|&&e| e != closet)
        .all(|&e| {
            let other_closet = world.get::<Closet>(e).is_some_and(|c| !c.wall_mounted);
            let mover = world.entity(e).contains::<Mover>();
            !other_closet
                && (!mover || world.get::<InCloset>(e).is_some() || insertable(world, state, e))
        })
}

/// tg `close()`: takes the items on its tile and becomes dense again.
pub fn close(world: &mut World, closet: Entity) -> bool {
    let Some(state) = world.get::<Closet>(closet) else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(closet) else {
        return false;
    };
    if !state.opened || !can_close(world, closet, pos) {
        return false;
    }
    take_contents(world, closet);
    world
        .get_mut::<Closet>(closet)
        .expect("checked above")
        .opened = false;
    world.entity_mut(closet).insert(Density);
    refresh_look(world, closet);
    true
}

/// tg `take_contents` + `insertion_allowed` for items: unanchored items on the closet's tile go
/// in until `storage_capacity` is reached. Returns how many went in.
// shortcut: items only; tg crates (`allow_objects`, `allow_dense`) also take loose structures
// and mobs go into closets, neither has a container model here yet.
fn take_contents(world: &mut World, closet: Entity) -> usize {
    let Some(&pos) = world.get::<TilePos>(closet) else {
        return 0;
    };
    let capacity = world
        .get::<Closet>(closet)
        .map_or(0, |c| c.storage_capacity);
    let held = world.get::<Stored>(closet).map_or(0, |s| s.items().len());
    let items: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| {
            world
                .get::<DmItem>(e)
                .is_some_and(|i| !i.anchored && !world.entity(e).contains::<Density>())
        })
        .collect();
    let taken = items.len().min(capacity.saturating_sub(held));
    for &item in &items[..taken] {
        put_inside(world, item, closet);
    }
    let Some(state) = world.get::<Closet>(closet).cloned() else {
        return taken;
    };
    let mobs: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| insertable(world, &state, e))
        .take(MOB_STORAGE_CAPACITY)
        .collect();
    for mob in mobs {
        put_mob_inside(world, mob, closet);
    }
    taken
}

/// tg `insert` of a living mob: it stops pulling and is no longer pulled.
pub(crate) fn put_mob_inside(world: &mut World, mob: Entity, closet: Entity) {
    world.entity_mut(mob).remove::<crate::pull::Pulling>();
    if let Some(puller) = world.get::<crate::pull::PulledBy>(mob).map(|p| p.puller()) {
        world.entity_mut(puller).remove::<crate::pull::Pulling>();
    }
    let look = world
        .get::<Appearance>(mob)
        .map_or((None, ss13_shared::MouseOpacity::Icon), |a| {
            (a.color, a.mouse_opacity)
        });
    world.entity_mut(mob).insert(InCloset { closet, look });
}

/// tg `togglelock`; `true` when the click was handled (even when access was denied).
fn toggle_lock(world: &mut World, closet: Entity, access: Option<&[String]>) -> bool {
    let Some(state) = world.get::<Closet>(closet) else {
        return false;
    };
    if !state.secure {
        return false;
    }
    if state.may_toggle_lock(access) {
        let mut state = world.get_mut::<Closet>(closet).expect("checked above");
        state.locked = !state.locked;
        refresh_look(world, closet);
    }
    true
}

/// Resolved clicks on closets, before `crate::inventory` sees them (so an item put on an open
/// closet's tile is not also handled there).
fn closet_clicks(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(state) = world.get::<Closet>(click.target).cloned() else {
            continue;
        };
        // Revalidated: the clicker still has its hands and the closet is still where it was.
        let Some(inventory) = world.get::<Inventory>(click.mob).cloned() else {
            continue;
        };
        if world.get::<TilePos>(click.target) != Some(&click.target_pos) {
            continue;
        }
        let access = |world: &mut World| -> Option<Vec<String>> {
            let card = inventory.id_card(world)?;
            world.get::<IdCard>(card).map(|card| card.access.clone())
        };
        match inventory.active_item() {
            None => {
                let toggled = if state.opened {
                    close(world, click.target)
                } else {
                    open_if_allowed(world, click.target)
                };
                if !toggled && !state.opened {
                    let access = access(world);
                    toggle_lock(world, click.target, access.as_deref());
                }
            }
            Some(held) => {
                if state.opened {
                    // tg `transfer_item_to_turf`.
                    if world
                        .get::<CarriedBy>(held)
                        .is_some_and(|c| c.0 == click.mob)
                    {
                        world
                            .entity_mut(held)
                            .remove::<CarriedBy>()
                            .insert(click.target_pos);
                    }
                } else if let Some(card) = crate::inventory::get_id(world, held) {
                    let card = world.get::<IdCard>(card).map(|c| c.access.clone());
                    toggle_lock(world, click.target, card.as_deref());
                }
            }
        }
    }
}

fn open_if_allowed(world: &mut World, closet: Entity) -> bool {
    world.get::<Closet>(closet).is_some_and(Closet::can_open) && open(world, closet)
}

/// Whether `item` is inside `closet`.
pub fn holds(world: &World, closet: Entity, item: Entity) -> bool {
    world.get::<StoredIn>(item).is_some_and(|s| s.0 == closet)
}

#[cfg(test)]
mod tests {
    use ss13_mapload::{Fill, PickArm};
    use ss13_world::WorldPlugin;

    use super::*;
    use crate::storage::{ContentTemplates, ContentsRng};

    const POS: TilePos = TilePos::new(2, 2, 1);

    fn world() -> World {
        let mut app = App::new();
        app.add_plugins(WorldPlugin);
        let mut world = std::mem::take(app.world_mut());
        world
            .resource_mut::<SpatialIndex>()
            .create_level(1, 4, 4)
            .unwrap();
        world.insert_resource(ContentTemplates::default());
        world.insert_resource(ContentsRng(ss13_mapload::spawner::Rng::new(1)));
        world
    }

    fn appearance() -> Appearance {
        Appearance {
            icon_state: IconStateId(0),
            layer: Layer(OBJ_LAYER),
            plane: 0,
            color: None,
            pixel_offset: [0, 0],
            overlays: vec![],
            rotation: 0,
            mouse_opacity: Default::default(),
        }
    }

    fn closet(world: &mut World, dm: DmCloset) -> Entity {
        let state = Closet::from_dm(
            &DmCloset {
                storage_capacity: 2,
                ..dm
            },
            &DmAtom::default(),
            None,
        );
        world.spawn((state, appearance(), Density, POS)).id()
    }

    fn item(world: &mut World, anchored: bool) -> Entity {
        let dm = DmItem {
            anchored,
            ..default()
        };
        world.spawn((dm, POS)).id()
    }

    #[test]
    fn closing_takes_items_and_opening_dumps_them() {
        let mut world = world();
        let c = closet(&mut world, DmCloset::default());
        let items = [item(&mut world, false), item(&mut world, false)];
        let bolted = item(&mut world, true);
        let extra = item(&mut world, false);

        assert!(!close(&mut world, c), "already closed");
        assert_eq!(take_contents(&mut world, c), 2, "storage_capacity 2");
        for i in items {
            assert!(holds(&world, c, i));
            assert_eq!(world.get::<TilePos>(i), None, "off the map");
        }
        assert!(!holds(&world, c, bolted) && !holds(&world, c, extra));
        assert!(
            world
                .resource::<SpatialIndex>()
                .on_tile(POS)
                .contains(&extra)
        );

        assert!(open(&mut world, c));
        assert!(!world.entity(c).contains::<Density>());
        for i in items {
            assert!(!holds(&world, c, i));
            assert_eq!(world.get::<TilePos>(i), Some(&POS));
            assert!(world.entity(i).contains::<ss13_net::server::Networked>());
        }
        assert_eq!(
            world.get::<Appearance>(c).unwrap().layer,
            Layer(BELOW_OBJ_LAYER)
        );

        // A mob on the tile keeps it open (no mob contents yet).
        let mob = world.spawn((Mover::new(0.2), POS)).id();
        assert!(!close(&mut world, c));
        world.despawn(mob);
        assert!(close(&mut world, c));
        assert!(world.entity(c).contains::<Density>());
        assert_eq!(world.get::<Stored>(c).unwrap().items().len(), 2);
    }

    #[test]
    fn locked_closets_open_only_after_an_allowed_unlock() {
        let mut world = world();
        let dm = DmCloset {
            secure: true,
            locked: true,
            req_access: vec!["captain".into()],
            ..default()
        };
        let c = closet(&mut world, dm);
        assert!(!open_if_allowed(&mut world, c));
        let assistant = vec!["maint_tunnels".to_owned()];
        assert!(toggle_lock(&mut world, c, Some(&assistant)), "handled");
        assert!(world.get::<Closet>(c).unwrap().locked, "denied");
        assert!(toggle_lock(&mut world, c, None));
        assert!(world.get::<Closet>(c).unwrap().locked, "no ID");
        let captain = vec!["captain".to_owned()];
        assert!(toggle_lock(&mut world, c, Some(&captain)));
        assert!(!world.get::<Closet>(c).unwrap().locked);
        assert!(open_if_allowed(&mut world, c));
        // Locking needs no access.
        close(&mut world, c);
        toggle_lock(&mut world, c, None);
        assert!(world.get::<Closet>(c).unwrap().locked);
    }

    #[test]
    fn first_opening_runs_populate_contents_once() {
        let mut world = world();
        let c = closet(&mut world, DmCloset::default());
        // No templates: the program runs, spawns nothing it has no prototype for, and is marked
        // done, so a second opening does not roll again.
        let fills = vec![Fill::Pick {
            arms: vec![PickArm {
                weight: 1.0,
                body: vec![Fill::New("/obj/item/missing".into())],
            }],
        }];
        world.entity_mut(c).insert(DmContents {
            populate: fills,
            ..default()
        });
        assert!(!world.get::<Closet>(c).unwrap().contents_initialized);
        assert!(open(&mut world, c));
        assert!(world.get::<Closet>(c).unwrap().contents_initialized);
    }
}
