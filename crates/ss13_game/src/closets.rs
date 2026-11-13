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
//!   `togglelock`; with any other item on an open closet -> onto its tile; a right click with an
//!   empty hand -> `togglelock`): secure closets unlock with the clicker's ID access, through the
//!   doors' [`check_access`]; anyone may lock them. A broken (emagged) lock does not toggle.
//! - Personal closets (`card_reader_installed`): an ID swiped on the closed, unlocked closet makes
//!   it that card's ("<name>'s locker"); then only that card, or an ID with the closet's access
//!   (`ACCESS_ALL_PERSONAL_LOCKERS`), unlocks it; an unowned one unlocks for anyone. The owner's
//!   card gone, the closet is public again.
//! - Brig lockers carry the `id` of their cell: the door timer locks and unlocks them
//!   ([`timer_lock`], [`timer_unlock`], from `crate::brig::timer`).
//! - [`AccountLock`] (tg `locked_to_account`): a bought crate opens only for an ID of the buyer's
//!   bank account (or of the buying department's staff); that first opening removes it.
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
use ss13_mapload::{DmAtom, DmCloset, DmContents, DmItem, DmName, IconIndex};
use ss13_shared::appearance::Overlay;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, IconStateId, Layer, TilePos};
use ss13_world::{Density, MapInit, Mapped, Mover, SpatialIndex, WorldSystems};

use crate::click::{ClickResolved, ResolveClicks};
use crate::click_handlers::{AddClickHandler, ClickPhase};
use crate::inventory::{CarriedBy, IdCard, Inventory, check_access};
use crate::storage::{Stored, StoredIn, put_inside, spawn_contents, take_out};

pub mod air;
pub mod kinds;
pub mod picklock;

pub use kinds::ClosetKind;

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
    /// tg `broken`: the lock was emagged open (`crate::emag`) and no longer locks.
    pub broken: bool,
    pub req_access: Vec<String>,
    pub req_one_access: Vec<String>,
    /// `PopulateContents()` already ran.
    pub contents_initialized: bool,
    /// `secure_closet/brig::id`: the door timer with this `id` locks it.
    pub id: Option<String>,
    /// `card_reader_installed`: a personal closet (its one `access_choices` entry "Personal").
    pub card_reader: bool,
    /// `access_locked`: a multitool locked the access panel (no new registration).
    pub access_locked: bool,
    /// `id_card`: the registered card, and the closet's name from before.
    owner: Option<(Entity, DmName)>,
    dense_when_open: bool,
    wall_mounted: bool,
    /// Crates: no closing over a standing mob.
    horizontal: bool,
    storage_capacity: usize,
    sprites: ClosetSprites,
    /// What the closet's type adds ([`kinds`]), set by [`kinds::attach`].
    pub kind: ClosetKind,
    /// `mob_storage_capacity`.
    pub mob_capacity: usize,
    /// Dense once closed (body bags are not).
    pub dense_when_closed: bool,
    /// `open_sound` / `close_sound` (empty: silent) with their volumes.
    open_sound: (String, f32),
    close_sound: (String, f32),
}

impl Closet {
    pub fn from_dm(dm: &DmCloset, atom: &DmAtom, icons: Option<&IconIndex>) -> Self {
        Self {
            opened: dm.opened,
            locked: dm.locked,
            welded: dm.welded,
            secure: dm.secure,
            broken: false,
            req_access: dm.req_access.clone(),
            req_one_access: dm.req_one_access.clone(),
            contents_initialized: false,
            id: (!dm.id.is_empty()).then(|| dm.id.clone()),
            card_reader: dm.card_reader_installed,
            access_locked: false,
            owner: None,
            dense_when_open: dm.dense_when_open,
            wall_mounted: dm.wall_mounted,
            horizontal: dm.horizontal,
            storage_capacity: dm.storage_capacity as usize,
            sprites: icons.map_or_else(ClosetSprites::default, |icons| {
                ClosetSprites::resolve(dm, atom, icons)
            }),
            kind: ClosetKind::Plain,
            mob_capacity: MOB_STORAGE_CAPACITY,
            dense_when_closed: true,
            open_sound: (dm.open_sound.clone(), dm.open_sound_volume),
            close_sound: (dm.close_sound.clone(), dm.close_sound_volume),
        }
    }

    /// tg `can_open` without the mob checks: welded or locked closets stay shut.
    // shortcut: no "something large on top" check (anchored or dense mobs on a horizontal
    // closet) and no strong-pull check; add with buckling and pulling.
    pub fn can_open(&self) -> bool {
        !self.opened && !self.welded && !self.locked
    }

    /// The card the closet is registered to (tg `id_card`).
    pub fn owner(&self) -> Option<Entity> {
        self.owner.as_ref().map(|(card, _)| *card)
    }

    /// tg `togglelock` access check: a broken lock does not toggle, only unlocking needs
    /// access.
    pub fn may_toggle_lock(&self, access: Option<&[String]>) -> bool {
        self.secure
            && !self.broken
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
                overlays.push(if self.broken {
                    s.crate_broken
                } else if self.locked {
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
                    overlays.push(if self.broken {
                        s.broken
                    } else if self.locked {
                        s.locked
                    } else {
                        s.unlocked
                    });
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
    /// `icon_broken` ("sparking"), crates' `securecrateemag`.
    broken: Option<IconStateId>,
    crate_broken: Option<IconStateId>,
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
                crate_broken: get("securecrateemag"),
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
            broken: get("sparking"),
            ..default()
        }
    }
}

/// tg `/datum/component/locked_to_account`: the buyer of a cargo order. The closet opens only
/// for a user whose ID card (`get_idcard(TRUE)`) has this bank account, or for a department
/// order an account paid by that department; that opening removes the lock. An emag removes it
/// too (`crate::emag`); forced opens ([`open`]) ignore it.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub enum AccountLock {
    Personal(u32),
    /// A department budget's id (`ACCOUNT_*`).
    Department(String),
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
    /// The step's direction (a cardboard box moves with it).
    pub dir: ss13_shared::Dir,
}

pub struct ClosetServerPlugin;

impl Plugin for ClosetServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ClosetReport>()
            .add_systems(MapInit, init_closets.after(crate::storage::StorageStartup))
            .add_message::<ClosetRelayMove>()
            .add_systems(
                Update,
                (
                    closet_clicks
                        .after(ResolveClicks)
                        .before(crate::inventory::inventory_intents),
                    relay_moves.after(WorldSystems::Movement),
                    hide_closeted.after(crate::inventory::inventory_intents),
                    kinds::attach_new.before(ResolveClicks),
                    air::process_closet_air,
                ),
            )
            .add_systems(
                Update,
                kinds::code_answers
                    .after(ss13_tgui::prompt::PromptSystems::Answer)
                    .in_set(ss13_tgui::server::UiSystems::Handle),
            )
            .add_observer(release_on_despawn);
        // Large crates: items and crowbars; abandoned crates: multitools.
        app.add_click_handler(ClickPhase::HeldOnTarget, 4460, kinds::held_on_target);
        // A folded body bag unfolds.
        app.add_click_handler(ClickPhase::AttackSelf, 3194, |w, c| {
            kinds::bodybag_attack_self(w, c.mob, c.target)
        });
        // tg `lockbox/tool_act`: an ID toggles a lockbox's lock (before a storage takes it).
        app.add_click_handler(ClickPhase::HeldOnTarget, 4650, |w, c| {
            c.held
                .is_some_and(|held| crate::lockbox::item_interaction(w, c.client, held, c.target))
        });
    }
}

/// [`Closet`] for every [`DmCloset`]; immediate contents; mapped items on closed closets go in.
fn init_closets(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmCloset, &DmAtom, Option<&DmContents>), (
        Without<Closet>,
        With<TilePos>,
        With<Mapped>,
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
        kinds::attach(world, entity);
        if let Some(contents) = &contents {
            report.partial += usize::from(contents.unsupported > 0);
            spawn_contents(world, entity, &contents.immediate, 0);
        }
        if opened {
            // tg `Initialize`: closets mapped open open without effects (and fill now).
            open_with(world, entity, false);
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
    open_with(world, closet, true)
}

/// tg `playsound(loc, sound, volume, TRUE, -3)` of `open()` / `close()`.
fn door_sound(world: &mut World, pos: TilePos, (sound, volume): (String, f32)) {
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, sound, volume, true).extrarange(-3.0),
    );
}

/// [`open`]; `special_effects` false: without the door sound (tg `open(special_effects =
/// FALSE)`: mapped open, `bust_open`).
pub(crate) fn open_with(world: &mut World, closet: Entity, special_effects: bool) -> bool {
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
    if special_effects {
        let sound = state.open_sound.clone();
        door_sound(world, pos, sound);
    }
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
    if !initialized
        && !crate::mining::necropolis::populate(world, closet)
        && let Some(contents) = world.get::<DmContents>(closet).cloned()
    {
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
    let had_mob = !mobs.is_empty();
    for mob in mobs {
        let_out(world, mob);
    }
    refresh_look(world, closet);
    kinds::cardboard_opened(world, closet, had_mob);
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
fn relay_moves(world: &mut World, mut moves: Local<MessageCursor<ClosetRelayMove>>) {
    let mobs: Vec<(Entity, ss13_shared::Dir)> = moves
        .read(world.resource::<Messages<ClosetRelayMove>>())
        .map(|m| (m.mob, m.dir))
        .collect();
    for (mob, dir) in mobs {
        // `IS_UNCONSCIOUS_OR_CRIT(user)`: nothing.
        if world.get::<crate::health::Stat>(mob) != Some(&crate::health::Stat::Conscious) {
            continue;
        }
        if let Some(c) = world.get::<InCloset>(mob).copied()
            && !kinds::cardboard_step(world, mob, c.closet, dir)
        {
            open_if_allowed(world, c.closet, Some(mob));
        }
    }
}

/// `breakout_time`, deciseconds.
const BREAKOUT_DS: f32 = 1200.0;

/// tg `container_resist_act` of the mob inside (the resist key): an unlocked closet opens; a
/// locked or welded one is pushed at for two minutes, then bursts open.
pub fn container_resist(world: &mut World, client: Entity, mob: Entity) {
    let Some(closet) = world.get::<InCloset>(mob).map(|c| c.closet) else {
        return;
    };
    let Some(state) = world.get::<Closet>(closet) else {
        return;
    };
    if state.opened {
        return;
    }
    if !state.welded && !state.locked {
        open_if_allowed(world, closet, Some(mob));
        return;
    }
    if world.get::<crate::tools::DoAfter>(mob).is_some() {
        return;
    }
    let what = crate::security::name_of(world, closet);
    crate::chem::visible(
        world,
        mob,
        &format!(
            "Вы упираетесь в стенку {what} и начинаете выталкивать дверцу... (это займёт около \
             2 минут.)"
        ),
        &format!("{} начинает сильно трястись!", ss13_chat::capitalize(&what)),
        Span::Warning,
    );
    let actor = crate::tools::Actor {
        client,
        mob,
        item: world
            .get::<crate::inventory::Inventory>(mob)
            .and_then(crate::inventory::Inventory::active_item),
    };
    crate::tools::do_after(
        world,
        actor,
        closet,
        BREAKOUT_DS,
        0,
        crate::construction::Job::ClosetBreakout,
    );
}

/// The end of the two minutes: `bust_open`.
pub fn complete_breakout(world: &mut World, mob: Entity, action: &crate::tools::DoAfter) {
    let closet = action.target;
    if world.get::<InCloset>(mob).map(|c| c.closet) != Some(closet) {
        return;
    }
    let Some(mut state) = world.get_mut::<Closet>(closet) else {
        return;
    };
    if state.opened || !(state.locked || state.welded) {
        return;
    }
    state.welded = false;
    state.locked = false;
    if state.secure {
        state.broken = true;
    }
    let (who, what) = (
        crate::security::name_of(world, mob),
        crate::security::name_of(world, closet),
    );
    crate::chem::visible(
        world,
        mob,
        &format!("Вы вырываетесь из {what}!"),
        &format!("{who} вырывается из {what}!"),
        Span::Danger,
    );
    open(world, closet);
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
    let sound = state.close_sound.clone();
    take_contents(world, closet);
    door_sound(world, pos, sound);
    world
        .get_mut::<Closet>(closet)
        .expect("checked above")
        .opened = false;
    if world
        .get::<Closet>(closet)
        .is_some_and(|c| c.dense_when_closed)
    {
        world.entity_mut(closet).insert(Density);
    }
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
        .take(state.mob_capacity)
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

fn tell_mob(world: &mut World, mob: Option<Entity>, span: Span, text: impl Into<String>) {
    if let Some(client) = mob
        .and_then(|m| world.get::<crate::server::PlayerMob>(m))
        .map(|p| p.client)
    {
        crate::tools::tell(world, client, span, text);
    }
}

fn card_access(world: &World, card: Option<Entity>) -> Vec<String> {
    card.and_then(|c| world.get::<IdCard>(c))
        .map(|c| c.access.clone())
        .unwrap_or_default()
}

/// tg `togglelock` by `user` showing `card` (the swiped ID, or the one held or worn); `true`
/// when the click was handled (even when access was denied). Unlocking needs `can_unlock`:
/// the registered card (or, on a personal closet, the closet's access), else the closet's
/// access (anyone for an unowned personal closet).
fn toggle_lock(
    world: &mut World,
    user: Option<Entity>,
    closet: Entity,
    card: Option<Entity>,
) -> bool {
    let Some(state) = world.get::<Closet>(closet).cloned() else {
        return false;
    };
    if !state.secure {
        return false;
    }
    if state.broken || kinds::lock_blocked(world, closet) {
        return true;
    }
    if state.locked {
        let access = card_access(world, card);
        let refusal = match state.owner() {
            // `id_card.resolve()` failed: the card is gone, the closet is public again.
            Some(owner) if world.get::<IdCard>(owner).is_none() => {
                clear_owner(world, closet);
                return toggle_lock(world, user, closet, card);
            }
            Some(owner) => {
                let override_access = state.card_reader
                    && check_access(&state.req_access, &state.req_one_access, &access);
                (card != Some(owner) && !override_access).then_some("not your locker!")
            }
            None => (!state.card_reader && !state.may_toggle_lock(Some(access.as_slice())))
                .then_some("в доступе отказано!"),
        };
        if let Some(why) = refusal {
            tell_mob(world, user, Span::Warning, why);
            return true;
        }
    }
    let locked = {
        let mut state = world.get_mut::<Closet>(closet).expect("checked above");
        state.locked = !state.locked;
        state.locked
    };
    if locked {
        kinds::relocked(world, closet);
    }
    refresh_look(world, closet);
    // `play_closet_lock_sound`: BandaStation's (`aesthetics_sounds/code/closet.dm`) picks one
    // of its lock sounds at 10, 3 tiles less range, whichever way the lock goes.
    if let Some(&pos) = world.get::<TilePos>(closet) {
        let sound = crate::sound::pick_file(world, &ss13_mapload::sound::files::CLOSET_LOCK);
        let sound = crate::sound::PlaySound::new(pos, sound, 10.0, true).extrarange(-3.0);
        crate::sound::playsound(world, sound);
    }
    let what = crate::security::name_of(world, closet);
    let text = if locked {
        format!("Вы запираете {what}.")
    } else {
        format!("Вы отпираете {what}.")
    };
    tell_mob(world, user, Span::Notice, text);
    true
}

/// tg `item_interaction` with a card reader (personal closets, "Personal"): `card` on the
/// closed, unlocked closet becomes its owner. False when the closet takes no registration.
// shortcut: no "Job"/"None" access choices (only reachable with a card reader installed by
// hand), no card readers or airlock electronics installed or taken out by hand, no renaming with a
// pen.
fn register_owner(world: &mut World, user: Entity, closet: Entity, card: Entity) -> bool {
    let Some(state) = world.get::<Closet>(closet) else {
        return false;
    };
    if !state.secure
        || state.broken
        || !state.card_reader
        || state.locked
        || state.opened
        || state.access_locked
    {
        return false;
    }
    let initial = match &state.owner {
        Some((_, name)) => name.clone(),
        None => world.get::<DmName>(closet).cloned().unwrap_or_default(),
    };
    let holder = world
        .get::<crate::outfit::IdRegistration>(card)
        .map(|r| r.registered_name.clone())
        .unwrap_or_default();
    world.entity_mut(closet).insert(DmName {
        name: format!("{holder}'s locker"),
        desc: format!("{} It has been ID locked to {holder}.", initial.desc),
    });
    world
        .get_mut::<Closet>(closet)
        .expect("checked above")
        .owner = Some((card, initial));
    tell_mob(
        world,
        Some(user),
        Span::Notice,
        format!("now owned by {holder}"),
    );
    true
}

/// tg `closet/multitool_act`: the access panel of a closed, unlocked personal closet locks or
/// unlocks. False when `held` is no multitool or the closet takes none.
fn multitool_act(world: &mut World, user: Entity, closet: Entity, held: Entity) -> bool {
    if crate::tools::tool_of(world, held).map(|(k, _)| k) != Some(crate::tools::ToolKind::Multitool)
    {
        return false;
    }
    let Some(mut state) = world.get_mut::<Closet>(closet) else {
        return false;
    };
    if !state.secure || !state.card_reader || state.broken || state.locked || state.opened {
        return false;
    }
    state.access_locked = !state.access_locked;
    let text = if state.access_locked {
        "панель доступа заблокирована"
    } else {
        "панель доступа разблокирована"
    };
    tell_mob(world, Some(user), Span::Notice, text);
    true
}

/// The registered card is gone (tg `togglelock`): the name back, no owner, no access needed.
fn clear_owner(world: &mut World, closet: Entity) {
    let Some(mut state) = world.get_mut::<Closet>(closet) else {
        return;
    };
    let Some((_, name)) = state.owner.take() else {
        return;
    };
    state.req_access.clear();
    state.req_one_access.clear();
    world.entity_mut(closet).insert(name);
}

/// `door_timer/timer_start` on a linked brig closet: an unbroken one closes (when it can) and
/// locks.
pub fn timer_lock(world: &mut World, closet: Entity) {
    let Some((broken, opened)) = world.get::<Closet>(closet).map(|c| (c.broken, c.opened)) else {
        return;
    };
    if broken || (opened && !close(world, closet)) {
        return;
    }
    world
        .get_mut::<Closet>(closet)
        .expect("checked above")
        .locked = true;
    refresh_look(world, closet);
}

/// `door_timer/timer_end` on a linked brig closet: an unbroken closed one unlocks.
pub fn timer_unlock(world: &mut World, closet: Entity) {
    let Some(state) = world.get::<Closet>(closet) else {
        return;
    };
    if state.broken || state.opened {
        return;
    }
    world
        .get_mut::<Closet>(closet)
        .expect("checked above")
        .locked = false;
    refresh_look(world, closet);
}

/// `locked_to_account/on_pre_open` for `user`: `None` lets it open, else the refusal (empty
/// without a user: nothing to tell).
fn account_refusal(
    world: &World,
    lock: &AccountLock,
    user: Option<Entity>,
) -> Option<&'static str> {
    let Some(user) = user else {
        return Some("");
    };
    let Some(card) = world.get::<Inventory>(user).and_then(|i| i.id_card(world)) else {
        return Some("No ID detected!");
    };
    let Some(account) = crate::cargo::economy::account_of(world, card) else {
        return Some("No linked bank account detected!");
    };
    let matches = match lock {
        AccountLock::Personal(buyer) => *buyer == account,
        AccountLock::Department(department) => world
            .get_resource::<crate::cargo::economy::Economy>()
            .and_then(|e| e.account(account))
            .is_some_and(|a| a.paycheck_department == *department),
    };
    (!matches).then_some("Bank account does not match with buyer!")
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
        let user = Some(click.mob);
        if kinds::intercept_click(world, &click, &state, inventory.active_item()) {
            continue;
        }
        // BandaStation lock picking: screwdriver, wirecutters, multitool.
        if let Some(held) = inventory.active_item()
            && !state.opened
            && picklock::tool_click(world, &click, held)
        {
            continue;
        }
        match inventory.active_item() {
            // tg `attack_hand_secondary`.
            None if click.modifiers.right => {
                if !state.opened {
                    let card = inventory.id_card(world);
                    toggle_lock(world, user, click.target, card);
                }
            }
            None => {
                let toggled = if state.opened {
                    close(world, click.target)
                } else {
                    open_if_allowed(world, click.target, user)
                };
                if !toggled && !state.opened {
                    let card = inventory.id_card(world);
                    toggle_lock(world, user, click.target, card);
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
                } else if !multitool_act(world, click.mob, click.target, held)
                    && let Some(card) = crate::inventory::get_id(world, held)
                    && !register_owner(world, click.mob, click.target, card)
                {
                    toggle_lock(world, user, click.target, Some(card));
                }
            }
        }
    }
}

/// tg `open(user)`: not welded or locked, and an [`AccountLock`] lets `user` through (and is
/// gone after).
fn open_if_allowed(world: &mut World, closet: Entity, user: Option<Entity>) -> bool {
    if !world.get::<Closet>(closet).is_some_and(Closet::can_open) {
        return false;
    }
    if let Some(lock) = world.get::<AccountLock>(closet).cloned() {
        if let Some(why) = account_refusal(world, &lock, user) {
            if !why.is_empty() {
                tell_mob(world, user, Span::Warning, why);
            }
            return false;
        }
        world.entity_mut(closet).remove::<AccountLock>();
        let what = crate::security::name_of(world, closet);
        if let Some(&pos) = world.get::<TilePos>(closet) {
            crate::security::show(
                world,
                pos,
                crate::health::DEFAULT_MESSAGE_RANGE,
                ss13_shared::chat::ChatChannel::Local,
                (
                    Span::Notice,
                    format!("{what}'s account lock disengages with a soft click."),
                ),
                &[],
            );
        }
    }
    open(world, closet)
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
    fn doors_play_the_type_sounds_unless_opened_quietly() {
        use crate::sound::PlaySound;
        let mut world = world();
        world.init_resource::<Messages<PlaySound>>();
        let dm = DmCloset {
            open_sound: "sound/machines/crate/crate_open.ogg".into(),
            open_sound_volume: 35.0,
            close_sound: "sound/machines/crate/crate_close.ogg".into(),
            close_sound_volume: 50.0,
            ..default()
        };
        let c = closet(&mut world, dm.clone());
        assert!(open(&mut world, c));
        assert!(close(&mut world, c));
        let played: Vec<(String, f32, f32)> = world
            .resource_mut::<Messages<PlaySound>>()
            .drain()
            .map(|s| (s.sound, s.vol, s.extrarange))
            .collect();
        assert_eq!(
            played,
            [
                ("sound/machines/crate/crate_open.ogg".to_owned(), 35.0, -3.0),
                (
                    "sound/machines/crate/crate_close.ogg".to_owned(),
                    50.0,
                    -3.0
                ),
            ]
        );
        // Mapped open (`special_effects = FALSE`): no sound.
        world.despawn(c);
        let quiet = closet(&mut world, dm);
        assert!(open_with(&mut world, quiet, false));
        assert_eq!(
            world.resource_mut::<Messages<PlaySound>>().drain().count(),
            0
        );
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
        assert!(!open_if_allowed(&mut world, c, None));
        let assistant = card(&mut world, &["maint_tunnels"]);
        assert!(toggle_lock(&mut world, None, c, Some(assistant)), "handled");
        assert!(world.get::<Closet>(c).unwrap().locked, "denied");
        assert!(toggle_lock(&mut world, None, c, None));
        assert!(world.get::<Closet>(c).unwrap().locked, "no ID");
        let captain = card(&mut world, &["captain"]);
        assert!(toggle_lock(&mut world, None, c, Some(captain)));
        assert!(!world.get::<Closet>(c).unwrap().locked);
        assert!(open_if_allowed(&mut world, c, None));
        // Locking needs no access.
        close(&mut world, c);
        toggle_lock(&mut world, None, c, None);
        assert!(world.get::<Closet>(c).unwrap().locked);
        // A broken lock does not toggle.
        world.get_mut::<Closet>(c).unwrap().broken = true;
        world.get_mut::<Closet>(c).unwrap().locked = false;
        toggle_lock(&mut world, None, c, Some(captain));
        assert!(!world.get::<Closet>(c).unwrap().locked);
    }

    fn card(world: &mut World, access: &[&str]) -> Entity {
        let access = access.iter().map(|a| (*a).to_owned()).collect();
        world.spawn(IdCard { access }).id()
    }

    #[test]
    fn personal_closets_belong_to_the_first_card() {
        let mut world = world();
        let dm = DmCloset {
            secure: true,
            locked: true,
            card_reader_installed: true,
            req_access: vec!["all_personal_lockers".into()],
            ..default()
        };
        let c = closet(&mut world, dm);
        world.entity_mut(c).insert(DmName {
            name: "personal closet".into(),
            desc: "A locker.".into(),
        });
        let user = world.spawn_empty().id();
        let (alice, bob) = (card(&mut world, &[]), card(&mut world, &[]));
        world
            .entity_mut(alice)
            .insert(crate::outfit::IdRegistration {
                registered_name: "Alice".into(),
                assignment: String::new(),
                job: String::new(),
            });
        assert!(!register_owner(&mut world, user, c, alice), "locked");
        // Unowned: anyone unlocks it.
        toggle_lock(&mut world, None, c, Some(bob));
        assert!(!world.get::<Closet>(c).unwrap().locked);
        assert!(register_owner(&mut world, user, c, alice));
        assert_eq!(world.get::<Closet>(c).unwrap().owner(), Some(alice));
        assert_eq!(world.get::<DmName>(c).unwrap().name, "Alice's locker");
        toggle_lock(&mut world, None, c, Some(bob));
        assert!(world.get::<Closet>(c).unwrap().locked, "anyone locks it");
        toggle_lock(&mut world, None, c, Some(bob));
        assert!(world.get::<Closet>(c).unwrap().locked, "not your locker");
        let hop = card(&mut world, &["all_personal_lockers"]);
        toggle_lock(&mut world, None, c, Some(hop));
        assert!(
            !world.get::<Closet>(c).unwrap().locked,
            "the override access"
        );
        toggle_lock(&mut world, None, c, Some(alice));
        assert!(world.get::<Closet>(c).unwrap().locked);
        toggle_lock(&mut world, None, c, Some(alice));
        assert!(!world.get::<Closet>(c).unwrap().locked, "the owner");
        toggle_lock(&mut world, None, c, None);
        // The owner's card gone: public again, with its old name.
        world.despawn(alice);
        toggle_lock(&mut world, None, c, Some(bob));
        let state = world.get::<Closet>(c).unwrap();
        assert!(!state.locked && state.owner().is_none() && state.req_access.is_empty());
        assert_eq!(world.get::<DmName>(c).unwrap().name, "personal closet");
    }

    #[test]
    fn timers_lock_brig_closets_and_account_locks_hold() {
        let mut world = world();
        let dm = DmCloset {
            secure: true,
            id: "Cell 1".into(),
            ..default()
        };
        let c = closet(&mut world, dm);
        assert_eq!(
            world.get::<Closet>(c).unwrap().id.as_deref(),
            Some("Cell 1")
        );
        assert!(open(&mut world, c));
        timer_lock(&mut world, c);
        let state = world.get::<Closet>(c).unwrap();
        assert!(!state.opened && state.locked, "closed and locked");
        timer_unlock(&mut world, c);
        assert!(!world.get::<Closet>(c).unwrap().locked);
        world.get_mut::<Closet>(c).unwrap().broken = true;
        timer_lock(&mut world, c);
        assert!(
            !world.get::<Closet>(c).unwrap().locked,
            "broken locks stay open"
        );

        let bought = closet(&mut world, DmCloset::default());
        world.entity_mut(bought).insert(AccountLock::Personal(7));
        assert!(!open_if_allowed(&mut world, bought, None), "nobody's ID");
        assert!(world.entity(bought).contains::<AccountLock>());
        world.entity_mut(bought).remove::<AccountLock>();
        assert!(open_if_allowed(&mut world, bought, None));
    }

    #[test]
    fn cardboard_boxes_move_with_their_mob_and_body_bags_stay_flat() {
        let mut world = world();
        world.insert_resource(Time::<()>::default());
        // A station area under the box: space has no gravity to walk it with.
        world.init_resource::<ss13_mapload::load::Areas>();
        let area = world
            .spawn(ss13_mapload::DmPower {
                path: "/area/station".into(),
                ..default()
            })
            .id();
        let mut areas = world.resource_mut::<ss13_mapload::load::Areas>();
        areas.add_area(area);
        assert!(areas.add_level(1, 4, 4, area));
        let c = closet(&mut world, DmCloset::default());
        world.entity_mut(c).insert(ss13_mapload::DmObj {
            path: kinds::CARDBOARD.into(),
            ..default()
        });
        kinds::attach(&mut world, c);
        let state = world.get::<Closet>(c).unwrap();
        assert_eq!((state.kind, state.mob_capacity), (ClosetKind::Cardboard, 1));
        let mob = world.spawn((POS, crate::health::Stat::Conscious)).id();
        put_mob_inside(&mut world, mob, c);
        assert!(kinds::cardboard_step(
            &mut world,
            mob,
            c,
            ss13_shared::Dir::East
        ));
        let to = TilePos::new(3, 2, 1);
        assert_eq!(world.get::<TilePos>(c), Some(&to), "the box moved");
        assert_eq!(world.get::<TilePos>(mob), Some(&to), "with its mob");
        // `move_delay`: the next step waits for the walk delay.
        assert!(kinds::cardboard_step(
            &mut world,
            mob,
            c,
            ss13_shared::Dir::West
        ));
        assert_eq!(world.get::<TilePos>(c), Some(&to));

        let bag = closet(&mut world, DmCloset::default());
        world.entity_mut(bag).insert(ss13_mapload::DmObj {
            path: kinds::BODY_BAG.into(),
            ..default()
        });
        kinds::attach(&mut world, bag);
        assert_eq!(world.get::<Closet>(bag).unwrap().mob_capacity, 2);
        assert!(open(&mut world, bag));
        assert!(close(&mut world, bag));
        assert!(!world.entity(bag).contains::<Density>(), "not dense closed");
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
