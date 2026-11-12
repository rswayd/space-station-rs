//! Storage items and container contents (server), after tg `/datum/storage`
//! (`code/datums/storage/storage.dm`) and `/obj/item/storage`.
//!
//! - [`StoredIn`]: an item inside a container (a storage item or a closet). Like a carried item it
//!   has no `TilePos`, so the spatial index and replication do not see it.
//! - [`Storage`]: the live `atom_storage` limits, from [`DmStorage`]; [`fits`] is tg
//!   `can_insert` for the limits that exist here.
//! - Contents programs ([`DmContents`]) are run by [`spawn_contents`]: storage items fill
//!   themselves when they appear (tg `Initialize` -> `PopulateContents`).
//! - The open storage of a player is [`StorageView`] on its mob, sent through the headless tgui
//!   interface [`StorageUi`]; its acts take an item into the hands or put the held one in.
//!
//! Player actions arrive from `crate::inventory` (slot and world clicks with the live world in
//! hand): [`click_storage`] opens a held or worn storage or inserts the active hand's item. Drags
//! arrive from `crate::drag` (tg `on_mousedrop_onto`, `on_mousedropped_onto`): a storage dragged
//! onto an empty hand goes into it, onto its user opens, onto another storage empties into it at
//! once, onto a floor or an object on it empties there after a 2 s `do_after`; the active
//! hand's item dragged onto a storage goes in. Alt-clicks open it (`crate::loot`). A view stays
//! open while the storage is carried or within reach on the floor; dragging an item onto
//! another cell of the open grid moves it there (`mousedrop_receive`).

use std::collections::HashMap;

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::contents::{collect_paths, run};
use ss13_mapload::spawner::Rng;
use ss13_mapload::{
    DmContents, DmItem, DmName, DmStorage, DmVending, DmWeight, IconIndex, MapSeed, Template,
};
use ss13_net::server::Networked;
use ss13_proto::Prototypes;
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_world::{MapInit, Mapped};

use crate::hud::{HudIcon, StorageData, StorageUi, StoredItem};
use crate::inventory::{CarriedBy, IdCard, Inventory};

/// `ITEM_SLOT_BACK` (`code/__DEFINES/inventory.dm`).
pub const ITEM_SLOT_BACK: u32 = 1 << 0;

/// On an item inside a container. Despawning the container despawns its contents (tg deletes
/// an atom's contents with it).
#[derive(Component, Debug)]
#[relationship(relationship_target = Stored)]
pub struct StoredIn(pub Entity);

/// The contents of a container, in insertion order (tg `contents`).
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = StoredIn, linked_spawn)]
pub struct Stored(Vec<Entity>);

impl Stored {
    pub fn items(&self) -> &[Entity] {
        &self.0
    }
}

/// Live `atom_storage` of a storage item.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Storage {
    pub max_slots: u32,
    pub max_specific_storage: u8,
    pub max_total_storage: u32,
}

impl From<&DmStorage> for Storage {
    fn from(dm: &DmStorage) -> Self {
        Self {
            max_slots: dm.max_slots,
            max_specific_storage: dm.max_specific_storage,
            max_total_storage: dm.max_total_storage,
        }
    }
}

/// Why [`fits`] refused (tg's balloon alerts).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// `w_class > max_specific_storage` ("не помещается!").
    TooBig,
    /// `max_slots` or `max_total_storage` reached ("нет места!").
    Full,
    Anchored,
    /// A storage item as big as the container or bigger ("слишком большое!").
    Nesting,
    /// The container sits in a storage that holds smaller things ("... на пути!").
    Outer,
    /// The item is the container.
    Itself,
    /// Not a type this storage takes (an RPED: parts, boards and a few stacks; a mining satchel:
    /// ore).
    CantHold,
    /// A locked lockbox (`crate::lockbox`).
    Locked,
}

/// tg `can_insert`'s balloon for `refusal`; `None` where it says nothing.
pub fn refusal_text(world: &World, refusal: Refusal, storage: Entity) -> Option<String> {
    Some(match refusal {
        Refusal::TooBig => "не помещается!".into(),
        Refusal::Full => "нет места!".into(),
        Refusal::Anchored => "anchored!".into(),
        Refusal::Nesting => "слишком большое!".into(),
        Refusal::CantHold => "не может вмещать!".into(),
        Refusal::Locked => "закрыто!".into(),
        Refusal::Outer => {
            let outer = world.get::<StoredIn>(storage)?.0;
            let name = world.get::<DmName>(outer)?.name.to_lowercase();
            format!("{name} на пути!")
        }
        Refusal::Itself => return None,
    })
}

/// What [`fits`] needs to know about the item and the container.
#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub w_class: u8,
    pub anchored: bool,
    pub is_storage: bool,
}

/// tg `/datum/storage/proc/can_insert` for the rules that exist here: weight class, slots,
/// total weight, anchoring, nesting (`allow_big_nesting = FALSE`) and the outer storage.
// shortcut: no `TRAIT_NODROP`. The `can_hold`/`cant_hold`/`exception_hold` type lists
// ([`ss13_mapload::DmHoldable`]) and a locked lockbox are checked in [`can_insert`].
pub fn fits(
    storage: &Storage,
    container_w_class: u8,
    contents: &[u8],
    outer: Option<&Storage>,
    item: Candidate,
) -> Result<(), Refusal> {
    if item.w_class > storage.max_specific_storage {
        return Err(Refusal::TooBig);
    }
    if contents.len() as u32 >= storage.max_slots {
        return Err(Refusal::Full);
    }
    let total: u32 = contents.iter().map(|&w| u32::from(w)).sum();
    if total + u32::from(item.w_class) > storage.max_total_storage {
        return Err(Refusal::Full);
    }
    if item.anchored {
        return Err(Refusal::Anchored);
    }
    if outer.is_some_and(|o| o.max_specific_storage < storage.max_specific_storage) {
        return Err(Refusal::Outer);
    }
    if item.is_storage && item.w_class >= container_w_class {
        return Err(Refusal::Nesting);
    }
    Ok(())
}

fn w_class(world: &World, item: Entity) -> u8 {
    world
        .get::<DmWeight>(item)
        .copied()
        .unwrap_or_default()
        .w_class
}

/// [`fits`] against the live world.
pub fn can_insert(world: &World, storage: Entity, item: Entity) -> Result<(), Refusal> {
    if storage == item {
        return Err(Refusal::Itself);
    }
    let Some(limits) = world.get::<Storage>(storage) else {
        return Err(Refusal::TooBig);
    };
    if crate::lockbox::is_locked(world, storage) {
        return Err(Refusal::Locked);
    }
    let Some(dm) = world.get::<DmItem>(item) else {
        return Err(Refusal::TooBig);
    };
    if crate::stock_parts::is_rped(world, storage)
        && !crate::stock_parts::rped_accepts(world, storage, item)
    {
        return Err(Refusal::CantHold);
    }
    if crate::mining::rocks::satchel_refuses(world, storage, item) {
        return Err(Refusal::CantHold);
    }
    // A holster's `set_holdable` (`crate::clothing::holsters`).
    if crate::clothing::holsters::refuses(world, storage, item) {
        return Err(Refusal::CantHold);
    }
    if crate::cargo::mail::bag_refuses(world, storage, item) {
        return Err(Refusal::CantHold);
    }
    if crate::implants::parts::pocket_refuses(world, storage, item) {
        return Err(Refusal::CantHold);
    }
    let contents: Vec<u8> = world
        .get::<Stored>(storage)
        .map(|s| s.items().iter().map(|&e| w_class(world, e)).collect())
        .unwrap_or_default();
    let outer = world
        .get::<StoredIn>(storage)
        .and_then(|s| world.get::<Storage>(s.0));
    let candidate = Candidate {
        w_class: w_class(world, item),
        anchored: dm.anchored,
        is_storage: world.get::<Storage>(item).is_some(),
    };
    // tg `can_insert`: the storage type's `set_holdable` lists.
    let mut limits = *limits;
    if let Some(hold) = world.get::<ss13_mapload::DmHoldable>(storage) {
        let listed = |list: &[String]| {
            list.iter()
                .any(|root| ss13_mapload::fidelity::is_under(&dm.path, root))
        };
        let exception = listed(&hold.exception_hold);
        if candidate.w_class > limits.max_specific_storage {
            if !exception {
                return Err(Refusal::TooBig);
            }
            let held = world.get::<Stored>(storage).map_or(0, |s| {
                s.items()
                    .iter()
                    .filter(|&&e| {
                        world.get::<DmItem>(e).is_some_and(|i| {
                            hold.exception_hold
                                .iter()
                                .any(|r| ss13_mapload::fidelity::is_under(&i.path, r))
                        })
                    })
                    .count()
            });
            if held as u32 >= hold.exception_max {
                return Err(Refusal::Full);
            }
            limits.max_specific_storage = candidate.w_class;
        }
        let can = hold.can_hold.is_empty() || listed(&hold.can_hold) || exception;
        if !can || listed(&hold.cant_hold) {
            return Err(Refusal::CantHold);
        }
    }
    fits(
        &limits,
        w_class(world, storage),
        &contents,
        outer,
        candidate,
    )
}

/// Into `container` from wherever it is (a hand, the floor, another container).
pub fn put_inside(world: &mut World, item: Entity, container: Entity) {
    world
        .entity_mut(item)
        .remove::<(CarriedBy, TilePos, StoredIn)>()
        .insert(StoredIn(container));
}

/// Out of its container onto `pos`.
pub fn take_out(world: &mut World, item: Entity, pos: TilePos) {
    let mut entity = world.entity_mut(item);
    entity.remove::<StoredIn>();
    if !entity.contains::<Networked>() {
        entity.insert(Networked);
    }
    entity.insert(pos);
}

/// Templates of every type a contents program can spawn, built at startup and extended by a
/// later map load.
#[derive(Resource, Default)]
pub struct ContentTemplates {
    by_path: HashMap<String, Template>,
}

/// Rolls of contents programs (`prob`, `pick_weight`), seeded from the map seed.
#[derive(Resource)]
pub struct ContentsRng(pub Rng);

impl ContentsRng {
    fn unit(&mut self) -> f64 {
        (self.0.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// The player's open storage. `data` is what [`StorageUi`] sends.
#[derive(Component, Debug, Default)]
pub struct StorageView {
    pub storage: Option<Entity>,
    pub data: StorageData,
}

#[derive(Debug, Clone, Copy)]
pub enum StorageAct {
    /// The item [`StoredItem::key`] names; checked to be in the open storage when applied.
    Take(Entity),
    Insert,
    /// tg `mousedrop_receive` / `storage/cell/mouse_drop_receive`: the item to the given place in
    /// the contents order (past the end: last).
    Move(Entity, usize),
}

impl ServerUi for StorageUi {
    type Source = &'static StorageView;
    type UserParam = ();
    type Act = StorageAct;

    fn static_data(_: &StorageView) {}

    fn data(source: &StorageView, _: Entity, _: &()) -> StorageData {
        source.data.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<StorageAct, UiReject> {
        match action {
            "take" => params
                .get("key")
                .and_then(|k| k.trim().parse::<u64>().ok())
                .and_then(Entity::try_from_bits)
                .map(StorageAct::Take)
                .ok_or_else(|| UiReject::BadParams("key: expected a stored item".into())),
            "insert" => Ok(StorageAct::Insert),
            "move" => {
                let item = params
                    .get("key")
                    .and_then(|k| k.trim().parse::<u64>().ok())
                    .and_then(Entity::try_from_bits)
                    .ok_or_else(|| UiReject::BadParams("key: expected a stored item".into()))?;
                let to = params
                    .get("to")
                    .and_then(|k| k.trim().parse::<usize>().ok())
                    .ok_or_else(|| UiReject::BadParams("to: expected a cell".into()))?;
                Ok(StorageAct::Move(item, to))
            }
            _ => Err(UiReject::UnknownAction),
        }
    }
}

pub struct StorageServerPlugin;

/// Startup work that needs [`ContentTemplates`] runs after this set.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StorageStartup;

impl Plugin for StorageServerPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<StorageUi>()
            // After the jobs: the outfits' storage items fill from the same templates.
            .add_systems(
                Startup,
                build_content_templates
                    .in_set(StorageStartup)
                    .after(crate::jobs::JobsStartup),
            )
            .add_systems(
                MapInit,
                init_storage
                    .in_set(StorageStartup)
                    .after(crate::jobs::JobsStartup),
            )
            .add_systems(
                Update,
                (
                    storage_acts.in_set(UiSystems::Handle),
                    refresh_views
                        .after(crate::inventory::inventory_intents)
                        .before(UiSystems::Open),
                    close_lost_views.in_set(UiSystems::StatusHook),
                ),
            )
            .add_observer(forget_view);
    }
}

/// Templates for every type a container on the map or in the kit can spawn.
fn build_content_templates(world: &mut World) {
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    world.insert_resource(ContentsRng(Rng::new(seed ^ 0x5707_a6e5)));
    let mut paths: Vec<String> = ss13_mapload::RUNTIME_CONTAINER_TYPES
        .iter()
        .copied()
        .chain(crate::clothing::holsters::content_paths())
        .map(str::to_owned)
        .collect();
    // What job outfits spawn fills itself too (a holster with its gun).
    if let Some(jobs) = world.get_resource::<crate::jobs::Jobs>() {
        paths.extend(jobs.data.outfit_items().into_iter().map(str::to_owned));
    }
    let mut query = world.query::<&DmContents>();
    for contents in query.iter(world) {
        collect_paths(&contents.populate, &mut paths);
        collect_paths(&contents.immediate, &mut paths);
    }
    // Vended items fill themselves too (a pack of cigarettes).
    let mut query = world.query::<&DmVending>();
    for vending in query.iter(world) {
        let products = vending.products.iter().chain(&vending.contraband);
        paths.extend(products.chain(&vending.premium).map(|p| p.path.clone()));
    }
    let templates = build_templates(world, paths);
    info!("storage: {} content templates", templates.by_path.len());
    world.insert_resource(templates);
}

/// Every storage item and ID card of the map pass gets its live limits and fills itself; types
/// a later load brings that the templates lack are added first.
fn init_storage(world: &mut World) {
    let mut paths = Vec::new();
    let mut query = world.query_filtered::<&DmContents, With<Mapped>>();
    for contents in query.iter(world) {
        collect_paths(&contents.populate, &mut paths);
        collect_paths(&contents.immediate, &mut paths);
    }
    let mut query = world.query_filtered::<&DmVending, With<Mapped>>();
    for vending in query.iter(world) {
        let products = vending.products.iter().chain(&vending.contraband);
        paths.extend(products.chain(&vending.premium).map(|p| p.path.clone()));
    }
    if let Some(known) = world.get_resource::<ContentTemplates>() {
        paths.retain(|p| !known.by_path.contains_key(p));
        if !paths.is_empty() {
            let more = build_templates(world, paths).by_path;
            world
                .resource_mut::<ContentTemplates>()
                .by_path
                .extend(more);
        }
    }

    // Every map-placed item gets what tg's `Initialize` gives it, before closets (which run
    // after this set) take items in or fill themselves.
    let mut query = world
        .query_filtered::<(Entity, Option<&DmItem>, Has<DmStorage>), (With<TilePos>, With<Mapped>)>(
        );
    let mut items: Vec<Entity> = query
        .iter(world)
        .filter(|(_, dm, storage)| {
            *storage
                || dm.is_some_and(|dm| {
                    dm.is_id_card() || crate::clothing::holsters::spec(&dm.path).is_some()
                })
        })
        .map(|(e, ..)| e)
        .collect();
    // Same order for the same map, so the seed gives the same contents.
    items.sort();
    let filled = items.len();
    for item in items {
        finish_item(world, item, 0);
    }
    info!("storage: {filled} map ID cards and storage items initialized");
}

/// Templates for `paths` and everything their own programs spawn.
fn build_templates(world: &World, mut paths: Vec<String>) -> ContentTemplates {
    let Some(protos) = world.get_resource::<Prototypes>() else {
        return ContentTemplates::default();
    };
    let icons = world.get_resource::<IconIndex>();
    let index: HashMap<(&str, &str), ss13_shared::IconStateId> = icons
        .map(|i| {
            i.0.iter()
                .map(|((dmi, state), id)| ((dmi.as_str(), state.as_str()), *id))
                .collect()
        })
        .unwrap_or_default();
    let mut out = ContentTemplates::default();
    let mut missing = 0usize;
    while let Some(path) = paths.pop() {
        if out.by_path.contains_key(&path) {
            continue;
        }
        let Some(proto) = protos.get(&path).cloned() else {
            missing += 1;
            continue;
        };
        if let Some(contents) = proto.get::<DmContents>() {
            collect_paths(&contents.populate, &mut paths);
            collect_paths(&contents.immediate, &mut paths);
        }
        out.by_path.insert(path, Template::new(proto, &index));
    }
    if missing > 0 {
        warn!("storage: {missing} content types have no prototype (rerun `ss13-mapload convert`)");
    }
    out
}

/// Nested storages deeper than this do not fill (a box of boxes of ...).
const MAX_FILL_DEPTH: u32 = 4;

/// Spawns `path` inside `container`. Storage items get their limits and fill themselves.
pub fn spawn_inside(
    world: &mut World,
    path: &str,
    container: Entity,
    depth: u32,
) -> Option<Entity> {
    let item = world
        .try_resource_scope(|world, templates: Mut<ContentTemplates>| {
            Some(templates.by_path.get(path)?.spawn(world, None))
        })
        .flatten()?;
    world
        .entity_mut(item)
        .insert((Networked, StoredIn(container)));
    finish_item(world, item, depth + 1);
    Some(item)
}

/// tg `Initialize` of an item, for every way one comes into existence (map load, contents
/// programs, vending, kits): an ID card's live access, a storage item's limits and its
/// `PopulateContents` (at nesting `depth`). Idempotent.
pub fn finish_item(world: &mut World, item: Entity, depth: u32) {
    let entity = world.entity(item);
    let card = entity
        .get::<DmItem>()
        .filter(|dm| dm.is_id_card() && !entity.contains::<IdCard>())
        .map(|dm| {
            let mut card = IdCard { access: Vec::new() };
            // The map's static `access` takes wildcard slots where it can (no trim).
            let mut data = crate::id_access::CardAccess::of_type(&dm.path);
            crate::id_access::add_access(
                &mut card,
                &mut data,
                &dm.access,
                None,
                crate::id_access::AddMode::ForceAddAll,
            );
            (card, data)
        });
    // Accessory holsters make theirs in `Initialize` (`crate::clothing::holsters`).
    let path = entity.get::<DmItem>().map(|dm| dm.path.clone());
    let storage = entity
        .get::<DmStorage>()
        .map(Storage::from)
        .or_else(|| {
            let spec = crate::clothing::holsters::spec(path.as_deref()?)?;
            Some(spec.storage)
        })
        .filter(|_| !entity.contains::<Storage>());
    if let Some(card) = card {
        world.entity_mut(item).insert(card);
    }
    // Filled before it locks: the lock does not apply to its own contents program.
    if let Some(storage) = storage {
        world.entity_mut(item).insert(storage);
        populate_storage(world, item, depth);
        let extra = path.map_or(&[][..], |p| {
            crate::clothing::holsters::initialize_contents(&p)
        });
        for path in extra {
            spawn_inside(world, path, item, depth);
        }
    }
    crate::lockbox::init(world, item);
}

/// tg `/obj/item/storage/Initialize` -> `PopulateContents`.
fn populate_storage(world: &mut World, storage: Entity, depth: u32) {
    if depth > MAX_FILL_DEPTH {
        return;
    }
    let Some(contents) = world.get::<DmContents>(storage).cloned() else {
        return;
    };
    spawn_contents(world, storage, &contents.immediate, depth);
    spawn_contents(world, storage, &contents.populate, depth);
    crate::coffee::populated(world, storage);
}

/// Runs `fills` with `container` as `src`.
pub fn spawn_contents(
    world: &mut World,
    container: Entity,
    fills: &[ss13_mapload::Fill],
    depth: u32,
) {
    let mut paths = Vec::new();
    world.try_resource_scope(|_, mut rng: Mut<ContentsRng>| {
        run(fills, &mut || rng.unit(), &mut |p| paths.push(p.to_owned()));
    });
    for path in paths {
        spawn_inside(world, &path, container, depth);
    }
}

/// A new player's backpack with the job box in it (`/datum/outfit/job`: `back`, `box`).
/// `None` without the backpack prototype.
pub fn spawn_backpack(world: &mut World, mob: Entity) -> Option<Entity> {
    let backpack = ss13_mapload::RUNTIME_CONTAINER_TYPES[0];
    let pack = world
        .try_resource_scope(|world, templates: Mut<ContentTemplates>| {
            Some(templates.by_path.get(backpack)?.spawn(world, None))
        })
        .flatten()?;
    world.entity_mut(pack).insert((Networked, CarriedBy(mob)));
    finish_item(world, pack, 0);
    Some(pack)
}

/// A click on a storage item the mob holds or wears (tg `attack_hand` on an item in the
/// inventory opens its storage; `attackby` with an item inserts it).
pub fn click_storage(world: &mut World, client: Entity, mob: Entity, storage: Entity) {
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return;
    };
    match inventory.active_item() {
        None => open_view(world, client, mob, storage),
        // An ID on a lockbox toggles its lock instead of going in.
        Some(held) if crate::lockbox::item_interaction(world, client, held, storage) => {}
        Some(held) => {
            insert_from_hand(world, held, storage);
        }
    }
}

/// The active hand's `held` into `storage`, if it fits; otherwise tg's refusal balloon to the
/// holder (`can_insert(messages = TRUE)`).
pub fn insert_from_hand(world: &mut World, held: Entity, storage: Entity) -> bool {
    if let Err(refusal) = can_insert(world, storage, held) {
        if let Some(user) = world.get::<CarriedBy>(held).map(|c| c.0)
            && let Some(text) = refusal_text(world, refusal, storage)
        {
            crate::balloon::balloon_alert(world, user, user, &text);
        }
        return false;
    }
    put_inside(world, held, storage);
    true
}

/// tg `open_storage`: shows `storage` to `mob` if it can reach it.
pub fn open_view(world: &mut World, client: Entity, mob: Entity, storage: Entity) {
    if !reachable(world, mob, storage) {
        return;
    }
    let mut entity = world.entity_mut(mob);
    match entity.get_mut::<StorageView>() {
        Some(mut view) => view.storage = Some(storage),
        None => {
            entity.insert(StorageView {
                storage: Some(storage),
                data: StorageData::default(),
            });
        }
    }
    let title = world
        .get::<DmName>(storage)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    world
        .resource_mut::<Messages<OpenUi>>()
        .write(OpenUi::new::<StorageUi>(client, mob, mob, title));
}

/// Whether `mob` can reach into `storage` right now: it holds or wears it, or it lies on a tile
/// next to the mob (tg `CanReach`), and it is not a locked lockbox (tg `locked` storage shows,
/// gives and dumps nothing; locking it closes the views).
/// A holster pinned to a jumpsuit is reached where the jumpsuit is.
// shortcut: storages inside other storages are not opened (tg reaches into nested ones).
pub fn reachable(world: &World, mob: Entity, storage: Entity) -> bool {
    if world.get::<Storage>(storage).is_none() || crate::lockbox::is_locked(world, storage) {
        return false;
    }
    let holder = crate::clothing::holsters::jumpsuit_of(world, storage).unwrap_or(storage);
    if world.get::<CarriedBy>(holder).is_some_and(|c| c.0 == mob)
        || crate::implants::parts::host_of(world, storage) == Some(mob)
    {
        return true;
    }
    in_reach(world, mob, holder)
}

/// The storage a click on `item` reaches: its own, or the one of a storage accessory pinned to
/// it (tg's jumpsuit `clone_storage`).
pub fn storage_of(world: &World, item: Entity) -> Option<Entity> {
    if world.get::<Storage>(item).is_some() {
        return Some(item);
    }
    crate::clothing::holsters::storage_accessory(world, item)
}

/// On the map and adjacent to `mob` (tg `Adjacent` for clicks).
pub fn in_reach(world: &World, mob: Entity, thing: Entity) -> bool {
    let (Some(&at), Some(&pos)) = (world.get::<TilePos>(mob), world.get::<TilePos>(thing)) else {
        return false;
    };
    world
        .resource::<ss13_world::SpatialIndex>()
        .adjacent(at, pos, |e| {
            e == thing
                || world.entity(e).contains::<ss13_world::Mover>()
                || crate::inventory::lets_pass(
                    world,
                    e,
                    crate::inventory::LETPASSTHROW | crate::inventory::LETPASSCLICKS,
                )
        })
}

fn view_data(world: &World, storage: Entity) -> StorageData {
    let item = |e: Entity| StoredItem {
        key: e.to_bits(),
        icon: world.get::<Appearance>(e).map(|a| HudIcon {
            icon_state: a.icon_state,
            color: a.color,
        }),
    };
    let items = world
        .get::<Stored>(storage)
        .map(|s| s.items().iter().map(|&e| item(e)).collect())
        .unwrap_or_default();
    StorageData {
        title: world
            .get::<DmName>(storage)
            .map(|n| n.name.clone())
            .unwrap_or_default(),
        max_slots: world
            .get::<Storage>(storage)
            .map_or(0, |s| s.max_slots.min(u32::from(u8::MAX)) as u8),
        items,
    }
}

/// Recomputes open views (a handful of players at a time, each up to ~21 items) and refreshes
/// their window at once when the contents changed; a storage out of reach is closed.
fn refresh_views(world: &mut World) {
    let mut query = world.query::<(Entity, &StorageView)>();
    let views: Vec<(Entity, Entity)> = query
        .iter(world)
        .filter_map(|(mob, v)| Some((mob, v.storage?)))
        .collect();
    for (mob, storage) in views {
        if !reachable(world, mob, storage) {
            let mut view = world.get_mut::<StorageView>(mob).expect("queried above");
            view.storage = None;
            continue;
        }
        let data = view_data(world, storage);
        let mut view = world.get_mut::<StorageView>(mob).expect("queried above");
        if view.data != data {
            view.data = data;
            let client = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client);
            if let Some(client) = client {
                world
                    .resource_mut::<Messages<OpenUi>>()
                    .write(OpenUi::new::<StorageUi>(client, mob, mob, ""));
            }
        }
    }
}

/// A shown storage that is gone or out of reach closes its window (tg `hide_contents`).
fn close_lost_views(mut windows: Query<&mut UiWindow>, views: Query<&StorageView>) {
    for mut window in &mut windows {
        if window.interface != StorageUi::NAME {
            continue;
        }
        if views.get(window.actor).is_ok_and(|v| v.storage.is_none()) {
            window.cap_status(ss13_shared::ui::UiStatus::Close);
        }
    }
}

/// The player closed the storage window (tg close button): the view forgets the storage.
fn forget_view(
    remove: On<Remove<UiWindow>>,
    windows: Query<&UiWindow>,
    mut views: Query<&mut StorageView>,
) {
    let Ok(window) = windows.get(remove.entity) else {
        return;
    };
    if window.interface == StorageUi::NAME
        && let Ok(mut view) = views.get_mut(window.actor)
    {
        view.storage = None;
        view.data = StorageData::default();
    }
}

/// tg storage screen clicks: an item slot takes that item into the hands
/// (`attempt_remove` -> `put_in_hands`), an empty slot with an item in hand inserts it.
fn storage_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<StorageUi>>>) {
    let acts: Vec<(Entity, StorageAct)> = acts
        .read(world.resource::<Messages<UiActed<StorageUi>>>())
        .map(|a| (a.actor, a.act))
        .collect();
    for (mob, act) in acts {
        let Some(storage) = world.get::<StorageView>(mob).and_then(|v| v.storage) else {
            continue;
        };
        // Able to act (tg screen clicks go through `ClickOn`): conscious, hands free, the
        // storage in reach; from inside a closet only what the mob carries (`CanReach`).
        let traits = crate::status_effects::incapacitation(world, mob);
        let carried = world.get::<CarriedBy>(storage).is_some_and(|c| c.0 == mob)
            || crate::implants::parts::host_of(world, storage) == Some(mob);
        if world.get::<crate::health::Stat>(mob) != Some(&crate::health::Stat::Conscious)
            || traits.incapacitated
            || traits.hands_blocked
            || (!carried && world.get::<crate::closets::InCloset>(mob).is_some())
            || !reachable(world, mob, storage)
        {
            continue;
        }
        let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
            continue;
        };
        match act {
            StorageAct::Take(item) => {
                // A key from the client: only an item in this storage right now.
                if world.get::<StoredIn>(item).is_some_and(|s| s.0 == storage) {
                    put_in_hands(world, mob, &inventory, item);
                }
            }
            StorageAct::Insert => {
                if let Some(held) = inventory.active_item() {
                    insert_from_hand(world, held, storage);
                }
            }
            StorageAct::Move(item, to) => {
                if world.get::<StoredIn>(item).is_some_and(|s| s.0 == storage) {
                    move_within(world, storage, item, to);
                }
            }
        }
    }
}

/// `contents -= target; contents.Insert(to, target)`: re-inserting keeps the relationship's
/// order (a re-added item goes last), so the item and everything after `to` go in again.
fn move_within(world: &mut World, storage: Entity, item: Entity, to: usize) {
    let Some(contents) = world.get::<Stored>(storage).map(|s| s.items().to_vec()) else {
        return;
    };
    let mut order: Vec<Entity> = contents.iter().copied().filter(|&e| e != item).collect();
    order.insert(to.min(order.len()), item);
    let first = order
        .iter()
        .zip(&contents)
        .position(|(a, b)| a != b)
        .unwrap_or(order.len());
    for &e in &order[first..] {
        world
            .entity_mut(e)
            .remove::<StoredIn>()
            .insert(StoredIn(storage));
    }
}

/// The drop target of a storage dragged somewhere, as tg's `over_object` sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropOnto {
    /// An atom: on the map, or an item in the user's inventory (drawn on its HUD slot).
    Atom(Entity),
    /// An empty hand slot (`/atom/movable/screen/inventory/hand`), by index.
    Hand(usize),
    /// Another empty HUD slot.
    Screen,
}

/// tg `/datum/storage/proc/on_mousedrop_onto` (`storage` dragged onto `over` by `actor`, who can
/// act). True when the storage handled the drop.
pub fn mousedrop_onto(
    world: &mut World,
    actor: crate::tools::Actor,
    storage: Entity,
    over: DropOnto,
) -> bool {
    let mob = actor.mob;
    match over {
        DropOnto::Hand(hand) => {
            // `parent.loc != user`: only from the inventory into an empty hand.
            let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
                return false;
            };
            let Some(slot) = inventory.slot_of(storage) else {
                return false;
            };
            if inventory.hands[hand].is_some() {
                return true;
            }
            // `putItemFromInventoryInHandIfPossible`: unequip (what falls with it falls) and
            // into the hand.
            let Some(&pos) = world.get::<TilePos>(mob) else {
                return true;
            };
            crate::inventory::unequip(world, mob, pos, slot);
            world.get_mut::<Inventory>(mob).expect("checked").hands[hand] = Some(storage);
            true
        }
        DropOnto::Screen => false,
        DropOnto::Atom(over) if over == mob => {
            open_view(world, actor.client, mob, storage);
            true
        }
        DropOnto::Atom(over) => {
            if world.get::<crate::health::Health>(over).is_some() {
                return false;
            }
            let over_carried = world.get::<CarriedBy>(over).is_some_and(|c| c.0 == mob);
            if !over_carried && !in_reach(world, mob, over) {
                return false;
            }
            let Some(dump_at) = dumping_location(world, mob, over) else {
                return false;
            };
            dump_content_at(world, actor, storage, over, dump_at);
            true
        }
    }
}

/// tg `get_dumping_location`: an open turf itself, an object's turf (an item in the user's hands
/// is on the user's turf), nothing for walls, doors, windows and grilles.
fn dumping_location(world: &World, mob: Entity, over: Entity) -> Option<TilePos> {
    let entity = world.entity(over);
    if entity.contains::<CarriedBy>() {
        return world.get::<TilePos>(mob).copied();
    }
    let pos = *entity.get::<TilePos>()?;
    if entity.contains::<ss13_mapload::Turf>() {
        return (!entity.contains::<ss13_world::Density>()).then_some(pos);
    }
    let path = crate::construction::path_of(world, over).unwrap_or_default();
    let refuses = entity.contains::<crate::doors::Door>()
        || [
            "/obj/structure/window",
            "/obj/structure/grille",
            "/obj/structure/falsewall",
            "/obj/structure/tram",
        ]
        .iter()
        .any(|root| ss13_mapload::fidelity::is_under(&path, root));
    (!refuses).then_some(pos)
}

/// tg `dump_content_at`: into another storage at once, onto a tile after a `do_after`.
fn dump_content_at(
    world: &mut World,
    actor: crate::tools::Actor,
    storage: Entity,
    dest: Entity,
    at: TilePos,
) {
    if !reachable(world, actor.mob, storage) {
        return;
    }
    let (what, onto) = (
        crate::security::name_of(world, storage),
        crate::security::name_of(world, dest),
    );
    if world.get::<Storage>(dest).is_some() {
        crate::tools::tell(
            world,
            actor.client,
            ss13_shared::chat::Span::Notice,
            format!("Вы вытряхиваете содержимое {what} в {onto}."),
        );
        let items: Vec<Entity> = world
            .get::<Stored>(storage)
            .map(|s| s.items().to_vec())
            .unwrap_or_default();
        for item in items {
            if can_insert(world, dest, item).is_ok() {
                put_inside(world, item, dest);
            }
        }
        return;
    }
    crate::tools::tell(
        world,
        actor.client,
        ss13_shared::chat::Span::Notice,
        format!("Вы начинаете вытряхивать {what} на {onto}..."),
    );
    // An item in the user's hands has no tile of its own: the timer watches the user instead.
    let target = if world.get::<CarriedBy>(dest).is_some() {
        actor.mob
    } else {
        dest
    };
    crate::tools::do_after(
        world,
        actor,
        target,
        DUMP_DELAY_DS,
        0,
        crate::construction::Job::Interact(crate::drag::DragJob::Dump { storage, at }),
    );
}

/// `do_after(user, 2 SECONDS, target = dest_object)` of a dump onto a tile.
pub const DUMP_DELAY_DS: f32 = 20.0;

/// End of the dump `do_after`: tg `remove_all(dump_loc)`, if the user still reaches the storage.
// shortcut: items land centered (tg offsets each by -8..8 pixels).
pub fn complete_dump(world: &mut World, mob: Entity, storage: Entity, at: TilePos) {
    if !reachable(world, mob, storage) {
        return;
    }
    let items: Vec<Entity> = world
        .get::<Stored>(storage)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    for item in items {
        take_out(world, item, at);
    }
}

/// tg `on_mousedropped_onto`: the active hand's `dropping` dragged onto `storage` goes in
/// (`attempt_insert`); a storage item would dump instead, so it does not. True when handled.
pub fn mousedropped_onto(
    world: &mut World,
    mob: Entity,
    storage: Entity,
    dropping: Entity,
) -> bool {
    let held = world.get::<Inventory>(mob).and_then(Inventory::active_item);
    if held != Some(dropping)
        || world.get::<Storage>(dropping).is_some()
        || !reachable(world, mob, storage)
    {
        return false;
    }
    insert_from_hand(world, dropping, storage);
    true
}

/// tg `put_in_hands`: the active hand, else the other one, else at the mob's feet.
pub(crate) fn put_in_hands(world: &mut World, mob: Entity, inventory: &Inventory, item: Entity) {
    // A cyborg's module never falls: `activate_module` or it stays in the model.
    if world
        .get::<crate::silicon::cyborg::RobotModule>(item)
        .is_some()
        && world
            .get::<crate::silicon::cyborg::ModuleSlots>(mob)
            .is_some()
    {
        crate::silicon::cyborg::activate_module(world, mob, item);
        return;
    }
    let hand = [inventory.active, 1 - inventory.active]
        .into_iter()
        .find(|&h| inventory.hands[h].is_none());
    match hand {
        Some(hand) => {
            world
                .entity_mut(item)
                .remove::<(StoredIn, TilePos)>()
                .insert(CarriedBy(mob));
            world
                .get_mut::<Inventory>(mob)
                .expect("checked by the caller")
                .hands[hand] = Some(item);
        }
        None => {
            if let Some(&pos) = world.get::<TilePos>(mob) {
                take_out(world, item, pos);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BACKPACK: Storage = Storage {
        max_slots: 21,
        max_specific_storage: 3,
        max_total_storage: 21,
    };
    const BOX: Storage = Storage {
        max_slots: 7,
        max_specific_storage: 2,
        max_total_storage: 14,
    };

    fn item(w_class: u8) -> Candidate {
        Candidate {
            w_class,
            anchored: false,
            is_storage: false,
        }
    }

    #[test]
    fn holdable_lists_follow_can_insert() {
        let mut world = World::new();
        let item = |world: &mut World, path: &str, w_class: u8| {
            world
                .spawn((
                    DmItem {
                        path: path.into(),
                        ..default()
                    },
                    DmWeight { w_class },
                ))
                .id()
        };
        let belt = world
            .spawn((
                DmItem {
                    path: "/obj/item/storage/belt".into(),
                    ..default()
                },
                DmWeight { w_class: 4 },
                Storage {
                    max_slots: 7,
                    max_specific_storage: 2,
                    max_total_storage: 21,
                },
                ss13_mapload::DmHoldable {
                    can_hold: vec!["/obj/item/crowbar".into()],
                    cant_hold: vec!["/obj/item/crowbar/red".into()],
                    exception_hold: vec!["/obj/item/fish_tank".into()],
                    exception_max: 1,
                },
            ))
            .id();
        let crowbar = item(&mut world, "/obj/item/crowbar/large", 2);
        let red = item(&mut world, "/obj/item/crowbar/red", 2);
        let pen = item(&mut world, "/obj/item/pen", 1);
        let tank = item(&mut world, "/obj/item/fish_tank", 4);
        let big = item(&mut world, "/obj/item/crowbar/huge", 4);
        assert_eq!(can_insert(&world, belt, crowbar), Ok(()));
        assert_eq!(can_insert(&world, belt, red), Err(Refusal::CantHold));
        assert_eq!(can_insert(&world, belt, pen), Err(Refusal::CantHold));
        // An exception fits whatever its size, once.
        assert_eq!(can_insert(&world, belt, tank), Ok(()));
        assert_eq!(can_insert(&world, belt, big), Err(Refusal::TooBig));
        put_inside(&mut world, tank, belt);
        let second = item(&mut world, "/obj/item/fish_tank", 4);
        assert_eq!(can_insert(&world, belt, second), Err(Refusal::Full));
    }

    #[test]
    fn weight_and_slot_rules_follow_can_insert() {
        assert_eq!(fits(&BACKPACK, 4, &[], None, item(3)), Ok(()));
        assert_eq!(
            fits(&BACKPACK, 4, &[], None, item(4)),
            Err(Refusal::TooBig),
            "bulky items do not go into backpacks"
        );
        // 21 total weight: seven normal items fill it, though slots remain.
        let seven_normal = [3; 7];
        assert_eq!(
            fits(&BACKPACK, 4, &seven_normal, None, item(1)),
            Err(Refusal::Full)
        );
        assert_eq!(fits(&BACKPACK, 4, &[3; 6], None, item(3)), Ok(()));
        // Slots: a box takes 7 tiny items, not 8.
        assert_eq!(fits(&BOX, 2, &[1; 6], None, item(1)), Ok(()));
        assert_eq!(fits(&BOX, 2, &[1; 7], None, item(1)), Err(Refusal::Full));
        assert_eq!(
            fits(
                &BOX,
                2,
                &[],
                None,
                Candidate {
                    anchored: true,
                    ..item(1)
                }
            ),
            Err(Refusal::Anchored)
        );
        // Nesting: a normal box fits into a backpack (bulky), not into another normal box.
        let a_box = Candidate {
            is_storage: true,
            ..item(2)
        };
        assert_eq!(fits(&BACKPACK, 4, &[], None, a_box), Ok(()));
        let big = Storage {
            max_specific_storage: 3,
            ..BOX
        };
        assert_eq!(fits(&big, 2, &[], None, a_box), Err(Refusal::Nesting));
        // A backpack-sized storage inside a box-limited one refuses what the outer one would.
        assert_eq!(
            fits(&BACKPACK, 4, &[], Some(&BOX), item(1)),
            Err(Refusal::Outer)
        );
    }
}
