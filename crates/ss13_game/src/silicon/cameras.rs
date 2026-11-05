//! Security cameras, the camera network and the camera console (server), after tg
//! `/obj/machinery/camera`, `SScameras` with `/datum/camerachunk`
//! (`code/modules/mob/living/silicon/ai/freelook/chunk.dm`, `code/controllers/subsystem/cameras.dm`)
//! and `/obj/machinery/computer/security` (`code/game/machinery/computer/camera.dm`).
//!
//! - Cameras come from the map ([`DmCamera`]: `c_tag`, `network`, `view_range`); autoname
//!   cameras get tg's `"<area> #<n>"`.
//! - The network is cut in `CHUNK_SIZE` (8) chunks. A chunk knows the cameras within
//!   `MAX_CAMERA_RANGE` of it and which of its 64 tiles they see: a camera sees `view(view_range)`
//!   from its tile (opaque tiles are seen and block, BYOND `view()`), and a chunk takes only the
//!   cameras whose distance to its center is at most `MAX_CAMERA_RANGE + CHUNK_SIZE / 2`
//!   (tg `update()`). A tile is visible to the network when its chunk says so
//!   ([`CameraNet::visible`], tg `is_visible_by_cameras`).
//! - Opacity changes (doors, walls built or destroyed) and camera changes mark the chunks within
//!   `MAX_CAMERA_RANGE + 1` dirty (tg `major_chunk_change`); dirty chunks are recomputed in the
//!   same tick (tg updates watched chunks at once and the others lazily).
//! - The console opens tgui `CameraConsole220` on an empty-hand click; `switch_camera` makes its
//!   active camera the one the client looks through: the client's replication viewer moves to
//!   the camera and the headless `CameraView` sends what the camera sees, static elsewhere.
//!   Closing the window (or walking away, which closes it) puts the view back on the mob.
//! - Under static nothing is replicated (tg hides mobs and objects there): a console's client
//!   gets a `ViewMask` of its camera's view, an AI eye's client the network's chunks.
// shortcut: all chunks with cameras are built at startup instead of tg's on-demand
// `generate_chunk`; MetaStation's are ~1.5k chunks, see the startup log line.
// shortcut: cameras do not lose power or break; `enabled` comes from the map, EMPs
// (`crate::emp::machines`), Camera Failure (`crate::events::camera_failure`) and wirecutters
// (`crate::defenses::cameras`, which also builds and takes cameras apart).
// shortcut: `/turf/open/ai_visible` is not always visible, x-ray cameras see like the others.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::ecs::entity::EntityHashMap;
use bevy::platform::time::Instant;
use bevy::prelude::*;
use ss13_mapload::{Areas, DmCamera, DmCameraConsole, DmName, Opaque};
use ss13_net::server::{
    ControlledEntity, MASK_CHUNK, MaskId, ViewGrid, ViewMask, ViewMasks, Viewer,
};
use ss13_shared::TilePos;
use ss13_shared::ui::UiStatus;
use ss13_tgui::server::{
    AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_world::SpatialIndex;

use super::ui::*;
use crate::click::{ClickResolved, ResolveClicks};
use crate::inventory::Inventory;
use crate::power::Unpowered;
use crate::server::Players;
use ss13_tgui::UiInterface;

/// tg `CHUNK_SIZE`.
pub const CHUNK_SIZE: i32 = 8;
/// tg `MAX_CAMERA_RANGE`.
pub const MAX_CAMERA_RANGE: i32 = 7;
/// The box a [`CameraViewData`] covers: the replication view radius.
pub const VIEW_RADIUS: u8 = ss13_net::server::DEFAULT_VIEW_RADIUS as u8;
const AUTONAME: &str = "/obj/machinery/camera/autoname";
/// The replication mask of the whole camera network (the AI eye); a camera's own mask (a
/// console looking through it) is [`camera_mask`].
pub const CAMERANET_MASK: MaskId = MaskId(u64::MAX);
// Network chunks are fed to the replication mask as they are.
const _: () = assert!(CHUNK_SIZE == MASK_CHUNK);

/// The replication mask of what one camera sees.
// shortcut: a camera's mask stays in `ViewMasks` after its last viewer leaves (a few chunks per
// camera ever watched, like its `CameraViewSource`); drop both when cameras get destroyed.
pub fn camera_mask(camera: Entity) -> MaskId {
    MaskId(camera.to_bits())
}

/// A security camera (tg `/obj/machinery/camera`).
#[derive(Component, Clone, Debug)]
pub struct Camera {
    pub c_tag: String,
    pub network: Vec<String>,
    pub view_range: u8,
    /// tg `camera_enabled`; `can_use()`.
    pub enabled: bool,
}

/// Chunk coordinates: tg `GET_CHUNK_COORD` / `CHUNK_SIZE`.
pub type ChunkKey = (i32, i32, u16);

pub fn chunk_of(pos: TilePos) -> ChunkKey {
    (
        pos.x.div_euclid(CHUNK_SIZE),
        pos.y.div_euclid(CHUNK_SIZE),
        pos.z,
    )
}

fn bit(pos: TilePos) -> u64 {
    let (x, y) = (pos.x.rem_euclid(CHUNK_SIZE), pos.y.rem_euclid(CHUNK_SIZE));
    1 << (y * CHUNK_SIZE + x)
}

#[derive(Debug, Default, Clone)]
struct Chunk {
    /// Bit `y * 8 + x` of the chunk's tiles.
    visible: u64,
    cameras: Vec<Entity>,
}

/// A camera as the network sees it.
#[derive(Clone, Copy, Debug)]
pub struct NetCamera {
    pub pos: TilePos,
    pub range: u8,
    pub enabled: bool,
}

/// `SScameras.chunks`: what the cameras see, per chunk.
#[derive(Resource, Default, Debug)]
pub struct CameraNet {
    chunks: HashMap<ChunkKey, Chunk>,
    cameras: EntityHashMap<NetCamera>,
    dirty: HashSet<ChunkKey>,
    /// Tiles whose sight changed (opacity, a camera added, moved or removed) since the watched
    /// cameras' views were last refreshed (`refresh_camera_views`).
    sight_changes: Vec<TilePos>,
    /// Bumped whenever a chunk's visibility changed.
    pub version: u64,
}

impl CameraNet {
    /// tg `is_visible_by_cameras` for a tile.
    pub fn visible(&self, pos: TilePos) -> bool {
        self.chunks
            .get(&chunk_of(pos))
            .is_some_and(|c| c.visible & bit(pos) != 0)
    }

    /// The visible tiles of one chunk (bit `y * 8 + x`).
    pub fn chunk_bits(&self, key: ChunkKey) -> u64 {
        self.chunks.get(&key).map_or(0, |c| c.visible)
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Approximate heap bytes of the chunks and the camera table.
    pub fn memory_bytes(&self) -> usize {
        let chunk = std::mem::size_of::<(ChunkKey, Chunk)>() + 8;
        let cams: usize = self.chunks.values().map(|c| c.cameras.capacity() * 8).sum();
        self.chunks.capacity() * chunk
            + cams
            + self.cameras.capacity() * (std::mem::size_of::<(Entity, NetCamera)>() + 8)
    }

    /// Adds or moves a camera (tg `add_camera_to_chunk` / `camera_moved`); its chunks become
    /// dirty.
    pub fn set_camera(&mut self, camera: Entity, cam: NetCamera) {
        self.remove_camera(camera);
        self.cameras.insert(camera, cam);
        self.sight_changes.push(cam.pos);
        for key in chunks_around(cam.pos, MAX_CAMERA_RANGE) {
            self.chunks.entry(key).or_default().cameras.push(camera);
            self.dirty.insert(key);
        }
    }

    pub fn remove_camera(&mut self, camera: Entity) {
        let Some(old) = self.cameras.remove(&camera) else {
            return;
        };
        self.sight_changes.push(old.pos);
        for key in chunks_around(old.pos, MAX_CAMERA_RANGE) {
            if let Some(chunk) = self.chunks.get_mut(&key) {
                chunk.cameras.retain(|&c| c != camera);
                self.dirty.insert(key);
            }
        }
    }

    /// tg `update_visibility` / `bare_major_chunk_change`: what blocks sight changed on `pos`.
    pub fn tile_changed(&mut self, pos: TilePos) {
        self.sight_changes.push(pos);
        for key in chunks_around(pos, MAX_CAMERA_RANGE + 1) {
            if self.chunks.contains_key(&key) {
                self.dirty.insert(key);
            }
        }
    }

    pub fn has_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }

    /// Recomputes the dirty chunks (tg `camerachunk/update`); returns those that changed. Each
    /// camera of the dirty chunks casts its view once and lights the dirty chunks that take it.
    // Cost: one shadowcast per camera near the change (a door: ~3 cameras x 225 tiles), not one
    // per camera and dirty chunk.
    pub fn update(
        &mut self,
        opaque: impl Fn(TilePos) -> bool,
        in_bounds: impl Fn(TilePos) -> bool,
    ) -> Vec<ChunkKey> {
        let chunks = &self.chunks;
        let mut fresh: HashMap<ChunkKey, u64> = self
            .dirty
            .drain()
            .filter(|k| chunks.contains_key(k))
            .map(|k| (k, 0))
            .collect();
        let mut cams: Vec<Entity> = fresh
            .keys()
            .flat_map(|k| chunks[k].cameras.iter().copied())
            .collect();
        cams.sort_unstable();
        cams.dedup();
        for camera in cams {
            let Some(cam) = self.cameras.get(&camera).copied().filter(|c| c.enabled) else {
                continue;
            };
            let takes: HashSet<ChunkKey> = fresh
                .keys()
                .copied()
                .filter(|&k| chunks[&k].cameras.contains(&camera) && takes_camera(k, cam.pos))
                .collect();
            if takes.is_empty() {
                continue;
            }
            camera_sees(cam.pos, cam.range, &opaque, &in_bounds, |p| {
                let key = chunk_of(p);
                if takes.contains(&key)
                    && let Some(bits) = fresh.get_mut(&key)
                {
                    *bits |= bit(p);
                }
            });
        }
        let mut changed = Vec::new();
        for (key, visible) in fresh {
            let chunk = self.chunks.get_mut(&key).expect("filtered above");
            if chunk.visible != visible {
                chunk.visible = visible;
                changed.push(key);
            }
        }
        if !changed.is_empty() {
            self.version += 1;
        }
        changed
    }

    /// The box of `radius` around `center` as a [`CameraViewData`] bit set.
    pub fn view_bits(&self, center: TilePos, radius: u8) -> Vec<u8> {
        box_bits(center, radius, |p| self.visible(p))
    }
}

/// Chunks whose tiles are within `range` of `pos`.
fn chunks_around(pos: TilePos, range: i32) -> impl Iterator<Item = ChunkKey> {
    let (lo, hi) = (
        chunk_of(TilePos::new(pos.x - range, pos.y - range, pos.z)),
        chunk_of(TilePos::new(pos.x + range, pos.y + range, pos.z)),
    );
    (lo.1..=hi.1).flat_map(move |y| (lo.0..=hi.0).map(move |x| (x, y, pos.z)))
}

/// tg `camerachunk/update`: a chunk takes the cameras within `MAX_CAMERA_RANGE + CHUNK_SIZE / 2`
/// of its center.
fn takes_camera(key: ChunkKey, camera: TilePos) -> bool {
    let center = TilePos::new(
        key.0 * CHUNK_SIZE + CHUNK_SIZE / 2,
        key.1 * CHUNK_SIZE + CHUNK_SIZE / 2,
        key.2,
    );
    camera
        .chebyshev(center)
        .is_some_and(|d| d as i32 <= MAX_CAMERA_RANGE + CHUNK_SIZE / 2)
}

/// tg `camera/can_see()`: `view(view_range)` from the camera's tile (the camera lights its own
/// tile, so darkness does not matter).
pub fn camera_sees(
    pos: TilePos,
    range: u8,
    opaque: &impl Fn(TilePos) -> bool,
    in_bounds: &impl Fn(TilePos) -> bool,
    mut visit: impl FnMut(TilePos),
) {
    let at = |dx: i32, dy: i32| TilePos::new(pos.x + dx, pos.y + dy, pos.z);
    view(
        i32::from(range),
        |dx, dy| {
            let p = at(dx, dy);
            // Off the map nothing is seen through.
            !in_bounds(p) || opaque(p)
        },
        |dx, dy| {
            let p = at(dx, dy);
            if in_bounds(p) {
                visit(p);
            }
        },
    );
}

/// `(2 * radius + 1)²` bits, row-major from the bottom-left corner of the box.
pub fn box_bits(center: TilePos, radius: u8, sees: impl Fn(TilePos) -> bool) -> Vec<u8> {
    let r = i32::from(radius);
    let side = 2 * r + 1;
    let mut bits = vec![0u8; ((side * side) as usize).div_ceil(8)];
    for dy in 0..side {
        for dx in 0..side {
            let p = TilePos::new(center.x - r + dx, center.y - r + dy, center.z);
            if sees(p) {
                let i = (dy * side + dx) as usize;
                bits[i / 8] |= 1 << (i % 8);
            }
        }
    }
    bits
}

/// Tiles visible from `(0, 0)` within Chebyshev distance `radius`: BYOND `view()`, the same
/// computation as players' line of sight (`ss13_net::server::ViewGrid`).
pub fn view(radius: i32, opaque: impl Fn(i32, i32) -> bool, mut visit: impl FnMut(i32, i32)) {
    let mut grid = ViewGrid::default();
    grid.compute(radius, opaque);
    for (x, y) in grid.seen() {
        visit(x, y);
    }
}

/// A camera console (tg `/obj/machinery/computer/security`): its networks, the cameras it lists
/// and its `active_camera`.
#[derive(Component, Clone, Debug)]
pub struct CameraConsole {
    pub statics: Arc<CameraConsoleStatic>,
    pub active: Option<Entity>,
}

/// On a client looking through a console's active camera.
#[derive(Component, Clone, Copy, Debug)]
pub struct ConsoleView {
    pub console: Entity,
    pub camera: Option<Entity>,
}

/// On a client whose replication viewer something else than its mob places
/// (`crate::server::follow_mobs` leaves it alone).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ViewOverride;

/// The CameraView HUD's source data (on a camera someone looks through, or an AI eye).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct CameraViewSource(pub CameraViewData);

pub struct CamerasPlugin;

impl Plugin for CamerasPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraNet>()
            .init_resource::<OpaqueTiles>()
            .register_ui::<CameraConsoleWindow>()
            .register_ui::<CameraViewWindow>()
            .add_systems(Startup, (init_cameras, build_net, init_consoles).chain())
            .add_systems(
                Update,
                (
                    (track_opacity, update_net).chain().after(ResolveClicks),
                    open_console.after(ResolveClicks).before(UiSystems::Open),
                    switch_camera.in_set(UiSystems::Handle),
                    (
                        follow_console_views,
                        refresh_camera_views,
                        assign_view_masks,
                    )
                        .chain()
                        .after(UiSystems::Handle)
                        .before(UiSystems::Send),
                    close_stale_views.in_set(UiSystems::StatusHook),
                ),
            );
    }
}

fn init_cameras(world: &mut World) {
    let mut query = world.query::<(Entity, &DmCamera, Option<&TilePos>)>();
    let found: Vec<(Entity, DmCamera, Option<TilePos>)> = query
        .iter(world)
        .map(|(e, d, p)| (e, d.clone(), p.copied()))
        .collect();
    // tg `/obj/machinery/camera/autoname/post_machine_initialize`: `"<area> #<n>"`, numbered
    // per area.
    let mut numbers: EntityHashMap<u32> = EntityHashMap::default();
    for (entity, dm, pos) in found {
        let mut c_tag = dm.c_tag.clone();
        if crate::obj_damage::is_under(&dm.path, AUTONAME)
            && let Some(area) = pos.and_then(|p| world.resource::<Areas>().area_at(p))
        {
            let n = numbers.entry(area).or_insert(0);
            *n += 1;
            let name = world
                .get::<DmName>(area)
                .map(|d| crate::vending::plain_name(&d.name).to_owned())
                .unwrap_or_default();
            c_tag = format!("{name} #{n}");
        }
        world.entity_mut(entity).insert(Camera {
            c_tag,
            network: dm.network.iter().map(|n| n.to_lowercase()).collect(),
            view_range: dm.view_range.clamp(0.0, 32.0) as u8,
            enabled: dm.camera_enabled,
        });
    }
}

/// Every opaque entity's tile, so a despawned wall still tells where sight opened up.
#[derive(Resource, Default)]
struct OpaqueTiles(EntityHashMap<TilePos>);

fn opacity_fns(world: &World) -> (impl Fn(TilePos) -> bool + '_, impl Fn(TilePos) -> bool + '_) {
    let index = world.resource::<SpatialIndex>();
    let opaque = move |p: TilePos| {
        index
            .on_tile(p)
            .iter()
            .any(|&e| world.get::<Opaque>(e).is_some())
    };
    (opaque, move |p| index.in_bounds(p))
}

fn build_net(world: &mut World) {
    let start = Instant::now();
    let mut query = world.query::<(Entity, &Camera, &TilePos)>();
    let cams: Vec<(Entity, NetCamera)> = query
        .iter(world)
        .map(|(e, c, &pos)| {
            (
                e,
                NetCamera {
                    pos,
                    range: c.view_range,
                    enabled: c.enabled,
                },
            )
        })
        .collect();
    let mut net = std::mem::take(&mut *world.resource_mut::<CameraNet>());
    for (e, cam) in &cams {
        net.set_camera(*e, *cam);
    }
    {
        let (opaque, in_bounds) = opacity_fns(world);
        net.update(opaque, in_bounds);
    }
    if let Some(mut masks) = world.get_resource_mut::<ViewMasks>() {
        for (&key, chunk) in &net.chunks {
            masks.set_chunk(CAMERANET_MASK, key, chunk.visible);
        }
    }
    let visible: u32 = net.chunks.values().map(|c| c.visible.count_ones()).sum();
    info!(
        "cameranet: {} cameras, {} chunks, {visible} visible tiles, ~{} KiB, built in {:.2?}",
        cams.len(),
        net.chunk_count(),
        net.memory_bytes() / 1024,
        start.elapsed()
    );
    world.insert_resource(net);
    let mut opaque = world.query_filtered::<(Entity, &TilePos), With<Opaque>>();
    let tiles: EntityHashMap<TilePos> = opaque.iter(world).map(|(e, &p)| (e, p)).collect();
    world.insert_resource(OpaqueTiles(tiles));
}

/// Opacity changes and camera changes since the last tick mark chunks dirty.
// Cost: per tick, one entry per entity that gained, lost or moved with opacity (door
// animations) and a few chunk keys each.
#[allow(clippy::type_complexity)]
fn track_opacity(
    changed: Query<(Entity, &TilePos), (With<Opaque>, Changed<TilePos>)>,
    added: Query<(Entity, &TilePos), Added<Opaque>>,
    mut removed: RemovedComponents<Opaque>,
    cameras: Query<(Entity, &Camera, &TilePos), Changed<Camera>>,
    mut tiles: ResMut<OpaqueTiles>,
    mut net: ResMut<CameraNet>,
    mut started: Local<bool>,
) {
    // The first run sees every entity as changed; the network was built from them.
    if !std::mem::replace(&mut *started, true) {
        removed.clear();
        return;
    }
    for e in removed.read() {
        if let Some(pos) = tiles.0.remove(&e) {
            net.tile_changed(pos);
        }
    }
    for (e, &pos) in changed.iter().chain(added.iter()) {
        if let Some(old) = tiles.0.insert(e, pos)
            && old != pos
        {
            net.tile_changed(old);
        }
        net.tile_changed(pos);
    }
    for (e, cam, &pos) in &cameras {
        net.set_camera(
            e,
            NetCamera {
                pos,
                range: cam.view_range,
                enabled: cam.enabled,
            },
        );
    }
}

fn update_net(world: &mut World) {
    if !world.resource::<CameraNet>().has_dirty() {
        return;
    }
    let mut net = std::mem::take(&mut *world.resource_mut::<CameraNet>());
    let changed = {
        let (opaque, in_bounds) = opacity_fns(world);
        net.update(opaque, in_bounds)
    };
    if let Some(mut masks) = world.get_resource_mut::<ViewMasks>() {
        for key in changed {
            masks.set_chunk(CAMERANET_MASK, key, net.chunk_bits(key));
        }
    }
    world.insert_resource(net);
}

fn init_consoles(world: &mut World) {
    let mut cams = world.query::<(Entity, &Camera, &TilePos)>();
    let mut all: Vec<(Entity, Camera, TilePos)> = cams
        .iter(world)
        .map(|(e, c, &p)| (e, c.clone(), p))
        .collect();
    // tg `cmp_camera_ctag_asc`.
    all.sort_by(|a, b| a.1.c_tag.cmp(&b.1.c_tag));
    let mut consoles = world.query::<(Entity, &DmCameraConsole)>();
    let found: Vec<(Entity, Vec<String>)> = consoles
        .iter(world)
        .map(|(e, d)| (e, d.network.iter().map(|n| n.to_lowercase()).collect()))
        .collect();
    let mut by_network: HashMap<Vec<String>, Arc<CameraConsoleStatic>> = HashMap::new();
    for (console, network) in found {
        let statics = by_network
            .entry(network.clone())
            .or_insert_with(|| {
                // tg `is_camera_available`: a `c_tag` and a shared network.
                let cameras = all
                    .iter()
                    .filter(|(_, c, _)| {
                        !c.c_tag.is_empty() && c.network.iter().any(|n| network.contains(n))
                    })
                    .map(|(e, c, p)| CameraEntry {
                        name: c.c_tag.clone(),
                        cam_ref: e.to_bits().to_string(),
                        x: p.x,
                        y: p.y,
                        z: p.z,
                        status: c.enabled,
                    })
                    .collect();
                Arc::new(CameraConsoleStatic { network, cameras })
            })
            .clone();
        world.entity_mut(console).insert(CameraConsole {
            statics,
            active: None,
        });
    }
}

/// The server side of `CameraConsole220`.
pub struct CameraConsoleWindow;

impl ss13_tgui::UiInterface for CameraConsoleWindow {
    const NAME: &'static str = CameraConsoleUi::NAME;
    type Data = CameraConsoleData;
    type StaticData = CameraConsoleStatic;
}

impl ServerUi for CameraConsoleWindow {
    type Source = &'static CameraConsole;
    type UserParam = Query<'static, 'static, &'static Camera>;
    type Act = Entity;

    fn static_data(console: &CameraConsole) -> CameraConsoleStatic {
        (*console.statics).clone()
    }

    fn data(console: &CameraConsole, _: Entity, cameras: &Query<&Camera>) -> CameraConsoleData {
        CameraConsoleData {
            active_camera: console.active.and_then(|e| {
                let cam = cameras.get(e).ok()?;
                Some(ActiveCamera {
                    name: cam.c_tag.clone(),
                    cam_ref: e.to_bits().to_string(),
                    status: cam.enabled,
                })
            }),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<Entity, UiReject> {
        if action != SWITCH_CAMERA {
            return Err(UiReject::UnknownAction);
        }
        params
            .get("camera")
            .and_then(|v| v.parse::<u64>().ok())
            .and_then(Entity::try_from_bits)
            .ok_or_else(|| UiReject::BadParams("camera: expected a camera ref".into()))
    }
}

/// The server side of the headless `CameraView`.
pub struct CameraViewWindow;

impl ss13_tgui::UiInterface for CameraViewWindow {
    const NAME: &'static str = CameraViewUi::NAME;
    type Data = CameraViewData;
    type StaticData = ();
}

impl ServerUi for CameraViewWindow {
    type Source = &'static CameraViewSource;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &CameraViewSource) {}

    fn data(source: &CameraViewSource, _: Entity, _: &()) -> CameraViewData {
        source.0.clone()
    }

    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

/// tg `/obj/machinery/computer/security` `interact` with an empty hand: the window.
fn open_console(
    mut clicks: MessageReader<ClickResolved>,
    consoles: Query<&DmName, (With<CameraConsole>, Without<Unpowered>)>,
    inventories: Query<&Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok(name) = consoles.get(click.target) else {
            continue;
        };
        if inventories
            .get(click.mob)
            .is_ok_and(|i| i.active_item().is_none())
        {
            open.write(OpenUi::new::<CameraConsoleWindow>(
                click.client,
                click.mob,
                click.target,
                crate::vending::plain_name(&name.name).to_owned(),
            ));
        }
    }
}

/// `ui_act("switch_camera")`: `locate(params["camera"]) in SScameras.cameras`, then the
/// console's listed cameras (its networks).
fn switch_camera(
    mut acts: MessageReader<UiActed<CameraConsoleWindow>>,
    mut consoles: Query<&mut CameraConsole>,
    cameras: Query<(), With<Camera>>,
    mut commands: Commands,
) {
    for act in acts.read() {
        let Ok(mut console) = consoles.get_mut(act.source) else {
            continue;
        };
        let wanted = act.act.to_bits().to_string();
        if !cameras.contains(act.act)
            || !console.statics.cameras.iter().any(|c| c.cam_ref == wanted)
        {
            continue;
        }
        console.active = Some(act.act);
        commands.entity(act.client).try_insert((
            ConsoleView {
                console: act.source,
                camera: None,
            },
            ViewOverride,
        ));
    }
}

/// Clients looking through a console follow its active camera; one whose console window closed
/// gets its view back on its mob.
#[allow(clippy::type_complexity)]
fn follow_console_views(
    mut clients: Query<(
        Entity,
        &mut ConsoleView,
        &mut Viewer,
        Option<&ControlledEntity>,
    )>,
    windows: Query<&UiWindow>,
    mut consoles: Query<&mut CameraConsole>,
    cameras: Query<(&Camera, &TilePos)>,
    positions: Query<&TilePos>,
    players: Res<Players>,
    mut open: MessageWriter<OpenUi>,
    mut commands: Commands,
) {
    for (client, mut view, mut viewer, controlled) in &mut clients {
        let open_window = windows.iter().any(|w| {
            w.client == client && w.source == view.console && w.interface == CameraConsoleUi::NAME
        });
        let active = consoles
            .get(view.console)
            .ok()
            .and_then(|c| c.active)
            .filter(|&cam| cameras.get(cam).is_ok_and(|(c, _)| c.enabled));
        let Some(camera) = active.filter(|_| open_window) else {
            commands
                .entity(client)
                .remove::<(ConsoleView, ViewOverride)>();
            // tg `ui_close` of the last user: the console's camera goes off.
            let in_use = windows
                .iter()
                .any(|w| w.source == view.console && w.interface == CameraConsoleUi::NAME);
            if !in_use && let Ok(mut console) = consoles.get_mut(view.console) {
                console.active = None;
            }
            let mob = players.mob(client).or(controlled.map(|c| c.0));
            if let Some(&pos) = mob.and_then(|m| positions.get(m).ok()) {
                viewer.center = pos;
            }
            continue;
        };
        if view.camera == Some(camera) {
            continue;
        }
        view.camera = Some(camera);
        let Ok((cam, &pos)) = cameras.get(camera) else {
            continue;
        };
        viewer.center = pos;
        // tg `update_active_camera_screen`: only what this camera sees, static elsewhere
        // (`refresh_camera_views` fills the bits).
        let _ = cam;
        commands.entity(camera).try_insert((
            CameraViewSource(CameraViewData {
                center: pos,
                radius: VIEW_RADIUS,
                visible: Vec::new(),
                moves_view: true,
            }),
            AlwaysState,
            NeedsView,
        ));
        if let Some(actor) = players.mob(client) {
            open.write(OpenUi::new::<CameraViewWindow>(client, actor, camera, ""));
        }
    }
}

/// A camera whose view bits must be (re)computed.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct NeedsView;

/// Recomputes the CameraView data and mask of a camera someone starts watching, and of a watched
/// camera whose sight changed within its range. The camera's own view, not the network's: a
/// door closing in front of it changes what it sees even when another camera still sees the
/// tiles behind (the network's chunks stay the same).
// Cost: per tick, the clients at consoles x the tiles whose sight changed; a shadowcast per
// affected watched camera. Cameras nobody watches cost nothing.
fn refresh_camera_views(world: &mut World) {
    let changes = std::mem::take(&mut world.resource_mut::<CameraNet>().sight_changes);
    let watched: HashSet<Entity> = world
        .query::<&ConsoleView>()
        .iter(world)
        .filter_map(|v| v.camera)
        .collect();
    let mut query = world
        .query_filtered::<(Entity, &Camera, &TilePos, Has<NeedsView>), With<CameraViewSource>>();
    let targets: Vec<(Entity, u8, TilePos)> = query
        .iter(world)
        .filter(|&(camera, cam, pos, needs)| {
            needs
                || (watched.contains(&camera)
                    && changes.iter().any(|c| {
                        c.chebyshev(*pos)
                            .is_some_and(|d| d <= u32::from(cam.view_range))
                    }))
        })
        .map(|(e, c, &p, _)| (e, c.view_range, p))
        .collect();
    for (camera, range, pos) in targets {
        let seen = {
            let (opaque, in_bounds) = opacity_fns(world);
            let mut seen = HashSet::new();
            camera_sees(pos, range, &opaque, &in_bounds, |p| {
                seen.insert(p);
            });
            seen
        };
        let bits = box_bits(pos, VIEW_RADIUS, |p| seen.contains(&p));
        if let Some(mut masks) = world.get_resource_mut::<ViewMasks>() {
            masks.set_tiles(camera_mask(camera), seen);
        }
        let mut e = world.entity_mut(camera);
        e.remove::<NeedsView>();
        if let Some(mut source) = e.get_mut::<CameraViewSource>()
            && source.0.visible != bits
        {
            source.0.visible = bits;
        }
    }
}

/// Clients whose view comes from cameras see only what they see (tg: mobs and objects under
/// camera static are hidden): a console's camera its own view, an AI eye the network.
// Cost: one pass over the clients per tick, a component write only when the source changes.
fn assign_view_masks(
    clients: Query<
        (
            Entity,
            Option<&ConsoleView>,
            Option<&ControlledEntity>,
            Option<&ViewMask>,
        ),
        With<Viewer>,
    >,
    eyes: Query<&super::ai::AiEye>,
    inside: Query<&super::ai::AiInside>,
    mut commands: Commands,
) {
    for (client, console, controlled, mask) in &clients {
        let want = match console.and_then(|c| c.camera) {
            Some(camera) => Some(camera_mask(camera)),
            None => controlled.and_then(|c| eyes.get(c.0).ok()).and_then(|eye| {
                match inside.get(eye.ai) {
                    // A carded AI sees around the card without cameras.
                    Ok(super::ai::AiInside::Card(_)) if !eye.blind => None,
                    _ if eye.blind => Some(super::ai::AI_BLIND_MASK),
                    _ => Some(CAMERANET_MASK),
                }
            }),
        };
        if mask.map(|m| m.0) == want {
            continue;
        }
        match want {
            Some(mask) => commands.entity(client).try_insert(ViewMask(mask)),
            None => commands.entity(client).try_remove::<ViewMask>(),
        };
    }
}

/// A CameraView window whose client no longer looks through its source closes.
fn close_stale_views(
    mut windows: Query<&mut UiWindow>,
    views: Query<&ConsoleView>,
    eyes: Query<(), With<super::ai::AiEye>>,
) {
    for mut window in &mut windows {
        if window.interface != CameraViewUi::NAME || eyes.contains(window.source) {
            continue;
        }
        let watching = views
            .get(window.client)
            .is_ok_and(|v| v.camera == Some(window.source));
        if !watching {
            window.cap_status(UiStatus::Close);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 24x24 level, walls where `walls` says.
    fn grid(walls: &[(i32, i32)]) -> (impl Fn(TilePos) -> bool + '_, impl Fn(TilePos) -> bool) {
        (
            move |p: TilePos| walls.contains(&(p.x, p.y)),
            |p: TilePos| (1..=24).contains(&p.x) && (1..=24).contains(&p.y),
        )
    }

    fn cam(x: i32, y: i32) -> NetCamera {
        NetCamera {
            pos: TilePos::new(x, y, 1),
            range: 7,
            enabled: true,
        }
    }

    #[test]
    fn walls_block_and_are_seen() {
        // A wall column at x = 8 from y = 1 to 24, the camera at (4, 10).
        let walls: Vec<(i32, i32)> = (1..=24).map(|y| (8, y)).collect();
        let (opaque, in_bounds) = grid(&walls);
        let mut net = CameraNet::default();
        net.set_camera(Entity::from_bits(1), cam(4, 10));
        net.update(&opaque, &in_bounds);
        assert!(net.visible(TilePos::new(4, 10, 1)));
        assert!(net.visible(TilePos::new(7, 12, 1)));
        assert!(
            net.visible(TilePos::new(8, 10, 1)),
            "the wall itself is seen"
        );
        assert!(!net.visible(TilePos::new(9, 10, 1)), "behind the wall");
        assert!(!net.visible(TilePos::new(4, 18, 1)), "beyond view_range");
        assert!(!net.visible(TilePos::new(4, 10, 2)), "another z");
    }

    #[test]
    fn opening_a_door_updates_the_chunks() {
        let mut walls: Vec<(i32, i32)> = (1..=24).map(|y| (8, y)).collect();
        let mut net = CameraNet::default();
        net.set_camera(Entity::from_bits(1), cam(4, 10));
        {
            let (opaque, in_bounds) = grid(&walls);
            net.update(&opaque, &in_bounds);
        }
        let v = net.version;
        assert!(!net.visible(TilePos::new(10, 10, 1)));
        // The door at (8, 10) opens.
        walls.retain(|&w| w != (8, 10));
        net.tile_changed(TilePos::new(8, 10, 1));
        let (opaque, in_bounds) = grid(&walls);
        assert!(!net.update(&opaque, &in_bounds).is_empty());
        assert!(net.version > v);
        assert!(net.visible(TilePos::new(10, 10, 1)));
        assert!(
            !net.visible(TilePos::new(10, 4, 1)),
            "still behind the wall"
        );
    }

    #[test]
    fn disabled_and_removed_cameras_see_nothing() {
        let (opaque, in_bounds) = grid(&[]);
        let mut net = CameraNet::default();
        let e = Entity::from_bits(1);
        net.set_camera(
            e,
            NetCamera {
                enabled: false,
                ..cam(4, 4)
            },
        );
        net.update(&opaque, &in_bounds);
        assert!(!net.visible(TilePos::new(4, 4, 1)));
        net.set_camera(e, cam(4, 4));
        net.update(&opaque, &in_bounds);
        assert!(net.visible(TilePos::new(5, 5, 1)));
        net.remove_camera(e);
        net.update(&opaque, &in_bounds);
        assert!(!net.visible(TilePos::new(5, 5, 1)));
    }

    #[test]
    fn far_chunks_ignore_a_camera_at_the_edge() {
        // tg takes a camera into a chunk only within MAX_CAMERA_RANGE + CHUNK_SIZE / 2 of its
        // center: from (1, 1) the camera reaches (8, 8), whose chunk center (12, 12) is 11 away.
        let (opaque, in_bounds) = grid(&[]);
        let mut net = CameraNet::default();
        net.set_camera(Entity::from_bits(1), cam(1, 1));
        net.update(&opaque, &in_bounds);
        assert!(net.visible(TilePos::new(7, 7, 1)));
        assert!(net.visible(TilePos::new(8, 8, 1)));
        let mut far = CameraNet::default();
        far.set_camera(Entity::from_bits(1), cam(0, 0));
        far.update(&opaque, &in_bounds);
        // (0, 0) to the (1, 1) chunk's center (12, 12) is 12 > 11: that chunk drops it.
        assert!(!far.visible(TilePos::new(8, 8, 1)));
    }

    #[test]
    fn view_bits_match_visibility() {
        let (opaque, in_bounds) = grid(&[]);
        let mut net = CameraNet::default();
        net.set_camera(Entity::from_bits(1), cam(10, 10));
        net.update(&opaque, &in_bounds);
        let data = CameraViewData {
            center: TilePos::new(12, 12, 1),
            radius: 3,
            visible: net.view_bits(TilePos::new(12, 12, 1), 3),
            moves_view: false,
        };
        assert!(data.sees(TilePos::new(10, 10, 1)));
        assert!(!data.sees(TilePos::new(20, 20, 1)));
        assert!(!data.sees(TilePos::new(18, 12, 1)), "outside the box");
    }
}
