//! Doors (server): tg `/obj/machinery/door` and subtypes open on a bump or a click from an
//! adjacent mob whose ID has the access (tg `allowed()`, else the deny animation), pass through `opening` -> `open` -> `closing` -> `closed` with tg's timings
//! (`door.dm`, `airlock.dm`, `windowdoor.dm`), and close by themselves when nothing dense stands
//! in the doorway.
//!
//! Live state is [`Door`], built once at startup from the prototype data [`DmDoor`]. Only doors
//! with something pending (an animation or an autoclose) carry [`DoorActive`], so the per-tick
//! pass touches those and nothing else.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::platform::time::Instant;
use bevy::prelude::*;
use ss13_mapload::{
    DmAirlockHelper, DmAtom, DmDoor, DoorKind, MapLoadPlugin, Opaque, read_manifest,
};
use ss13_shared::appearance::Overlay;
use ss13_shared::{Appearance, Dir, IconStateId, Layer, TilePos};
use ss13_world::{Bumped, Density, MapInit, Mapped, Mover, SpatialIndex, WorldSystems};

use crate::click::{ClickResolved, ResolveClicks};
use crate::inventory::{IdCard, IdCards, Inventory, check_access};
use crate::power::Unpowered;
use crate::wires::{
    Electrified, WIRE_BACKUP1, WIRE_BACKUP2, WIRE_BOLTS, WIRE_IDSCAN, WIRE_OPEN, WIRE_POWER1,
    WIRE_POWER2, Wires,
};

/// tg `DOOR_CLOSE_WAIT`: retry delay of an autoclose that found the doorway blocked, and the
/// autoclose delay of plain doors.
pub const DOOR_CLOSE_WAIT_SECS: f64 = 6.0;

/// Seconds from the start of an animation at which each effect happens
/// (`animation_segment_delay`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timings {
    pub open_transparent: f64,
    pub open_passable: f64,
    pub open_done: f64,
    pub close_dense: f64,
    pub close_opaque: f64,
    pub close_done: f64,
}

impl Timings {
    // shortcut: morgue doors (`/obj/machinery/door/morgue`, 1.5 s animations) use the plain door
    // timings; give them a kind if their slowness starts to matter.
    pub fn of(kind: DoorKind) -> Self {
        match kind {
            DoorKind::Airlock => Self {
                open_transparent: 0.1,
                open_passable: 0.5,
                open_done: 0.6,
                close_dense: 0.2,
                close_opaque: 0.5,
                close_done: 0.6,
            },
            DoorKind::Windoor => Self {
                open_transparent: 0.0,
                open_passable: 0.7,
                open_done: 0.9,
                close_dense: 0.2,
                close_opaque: 0.9,
                close_done: 0.9,
            },
            // `animation_segment_delay` of `poddoor` and `firedoor`.
            DoorKind::Poddoor => Self {
                open_transparent: 0.0,
                open_passable: 0.5,
                open_done: 1.1,
                close_dense: 0.2,
                close_opaque: 1.1,
                close_done: 1.1,
            },
            DoorKind::Firedoor => Self {
                open_transparent: 0.0,
                open_passable: 1.0,
                open_done: 1.2,
                close_dense: 0.2,
                close_opaque: 1.2,
                close_done: 1.2,
            },
            DoorKind::Basic => Self {
                open_transparent: 0.0,
                open_passable: 0.5,
                open_done: 0.6,
                close_dense: 0.2,
                close_opaque: 0.6,
                close_done: 0.6,
            },
        }
    }
}

/// `since` is `Time::elapsed_secs_f64` at the start of the animation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DoorState {
    Closed,
    Opening { since: f64 },
    Open,
    Closing { since: f64 },
}

impl DoorState {
    fn frame(self) -> usize {
        match self {
            Self::Closed => 0,
            Self::Opening { .. } => 1,
            Self::Open => 2,
            Self::Closing { .. } => 3,
        }
    }
}

/// Icon states per frame (closed, opening, open, closing), resolved once per distinct look.
#[derive(Debug, Default)]
struct DoorSprites {
    frames: [Option<IconStateId>; 4],
    /// Airlock panel fill (`fill_*` or `<material>_*`).
    fills: [Option<IconStateId>; 4],
    bolt_lights: Option<IconStateId>,
    opening_lights: Option<IconStateId>,
    closing_lights: Option<IconStateId>,
    welded: Option<IconStateId>,
    /// Airlock `sealed` (a pneumatic seal on the closed door).
    sealed: Option<IconStateId>,
    /// Basic doors and windoors: the deny frame; airlocks: the denied lights.
    deny: Option<IconStateId>,
    /// Airlock maintenance panel per frame (`panel_<frame>`).
    panel: [Option<IconStateId>; 4],
    /// Airlock `sparks` while being emagged.
    sparks: Option<IconStateId>,
}

#[derive(Component, Debug)]
pub struct Door {
    pub kind: DoorKind,
    pub state: DoorState,
    /// Bolted.
    pub locked: bool,
    pub welded: bool,
    pub req_access: Vec<String>,
    pub req_one_access: Vec<String>,
    /// BYOND dir bits of the sides that open without access (tg `unres_sides`).
    pub unres_sides: u8,
    pub autoclose: bool,
    pub normalspeed: bool,
    pub safe: bool,
    can_open_with_hands: bool,
    opaque_when_closed: bool,
    open_layer: f32,
    closing_layer: f32,
    /// When an open door tries to close by itself.
    close_at: Option<f64>,
    /// Until when the deny animation shows (tg `run_animation(DOOR_DENY_ANIMATION)`).
    denied_until: Option<f64>,
    sprites: Arc<DoorSprites>,
    /// Multi-tile airlocks: the [`DoorFiller`] on the second tile and that tile.
    filler: Option<(Entity, TilePos)>,
    /// Airlocks (`crate::wires`): tg `panel_open`, the maintenance panel a screwdriver opens.
    pub panel_open: bool,
    /// tg `feedback`: the lights work.
    pub feedback: bool,
    /// Seconds until main / backup power comes back (tg `main_power_time`,
    /// `backup_power_time`); 0: that power is on.
    pub main_outage: f64,
    pub backup_outage: f64,
    /// tg `aiControlDisabled` (`AI_WIRE_*`): mapped, changed by the AI wire, read by
    /// `crate::silicon::ai`.
    pub ai_control_disabled: i8,
    /// tg `shockCooldown`: the door shocks again from this time on.
    pub shock_ready_at: f64,
    /// tg `unres_latch`: the door was mapped with an unrestricted side, so the wire works.
    pub unres_latch: bool,
    /// tg `emergency`: emergency access, anyone opens it (`allowed()`).
    pub emergency: bool,
    /// tg `red_alert_access` (the `red_alert_access` mapping helper): at red alert and above
    /// the door takes any access (`check_access_list`).
    pub red_alert_access: bool,
    /// The station is at red alert or above (`crate::security_level` keeps it on the doors with
    /// [`Self::red_alert_access`]).
    pub red_alert: bool,
    /// tg `aiDisabledIdScanner`: the AI turned the ID scanner off.
    pub ai_disabled_id_scanner: bool,
    /// Airlocks: `doorOpen`, `doorClose`, `doorDeni`, `boltUp`, `boltDown`.
    pub sounds: Arc<DoorSounds>,
    /// The animation sound to play this tick ([`door_sounds`]).
    sound: Option<DoorSound>,
    /// tg `cycle_pump`: the airlock pump cycling this airlock
    /// (`crate::atmos_machinery::airlock_pump`).
    pub cycle_pump: Option<Entity>,
    /// An opening was turned into a cycle request this tick (tg `airlock_act`).
    pub cycle_requested: bool,
    /// tg `obj_flags & EMAGGED` (`crate::emag`): nothing opens or closes an airlock or windoor
    /// any more, no hand or bump works it.
    pub emagged: bool,
    /// tg `operating` (and an airlock's `MAINT` sparks) between `emag_act` and
    /// `finish_emag_act`.
    pub emagging: bool,
    /// tg `dangerous_close`: the closing door crushes what is in its doorway ([`crush`]).
    crush: bool,
    /// tg `/obj/machinery/door/password` (`crate::password_door`): no hand, ID or bump opens it,
    /// only its password; blast door frames and timings.
    pub password_lock: bool,
    /// tg airlock `seal`: the pneumatic seal braced on it (`crate::devices::door_seal`); a
    /// sealed airlock neither opens nor is pried.
    pub seal: Option<Entity>,
}

/// An airlock's sound vars (empty: silent).
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct DoorSounds {
    pub open: String,
    pub close: String,
    pub deny: String,
    pub bolt_up: String,
    pub bolt_down: String,
    /// Blast doors and shutters: `animation_sound`.
    pub animation: String,
}

/// tg `animation_effects` animations that play a sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DoorSound {
    Open,
    Close,
    Deny,
    /// An airlock animation run with `BYPASS_DOOR_CHECKS`.
    Forced,
}

impl Door {
    pub fn is_operating(&self) -> bool {
        self.emagging
            || matches!(
                self.state,
                DoorState::Opening { .. } | DoorState::Closing { .. }
            )
    }

    pub fn close_at(&self) -> Option<f64> {
        self.close_at
    }

    /// Drops a pending autoclose (tg `autoclose()` finds `autoclose` off when its timer fires).
    pub fn close_at_clear(&mut self) {
        self.close_at = None;
    }

    /// The tiles the door occupies.
    fn doorway(&self, pos: TilePos) -> impl Iterator<Item = TilePos> + use<> {
        std::iter::once(pos).chain(self.filler.map(|(_, p)| p))
    }

    fn timings(&self) -> Timings {
        // `password/animation_segment_delay`: the blast door's.
        if self.password_lock {
            return Timings::of(DoorKind::Poddoor);
        }
        Timings::of(self.kind)
    }

    /// tg `allowed()` against the opener's ID access (`None`: no ID).
    pub fn allows(&self, access: Option<&[String]>) -> bool {
        // `door/check_access_list`.
        if self.red_alert_access && self.red_alert {
            return true;
        }
        check_access(
            &self.req_access,
            &self.req_one_access,
            access.unwrap_or_default(),
        )
    }

    /// tg `unrestricted_side`: `get_dir(door, opener) & unres_sides`.
    pub fn unrestricted_from(&self, door: TilePos, opener: TilePos) -> bool {
        let dx = opener.x - door.x;
        let dy = opener.y - door.y;
        let dir = u8::from(dy > 0)
            | u8::from(dy < 0) << 1
            | u8::from(dx > 0) << 2
            | u8::from(dx < 0) << 3;
        dir & self.unres_sides != 0
    }

    /// tg `hasPower()`: airlocks and windoors neither open nor close without power (tg forces
    /// them with a crowbar); other doors have no motor to stop. An airlock also needs its main
    /// or its backup power (`remaining_main_outage`, `remaining_backup_outage`).
    // shortcut: no crowbar prying of unpowered airlocks yet.
    pub fn has_power(&self, powered: bool) -> bool {
        match self.kind {
            DoorKind::Airlock => powered && self.wire_power(),
            DoorKind::Windoor => powered,
            _ => true,
        }
    }

    /// Main or backup power is on (the airlock half of `hasPower`, without the area's).
    pub fn wire_power(&self) -> bool {
        self.main_outage <= 0.0 || self.backup_outage <= 0.0
    }

    /// tg airlock `requiresID()`: false once the ID scan wire is cut or the AI disabled the
    /// scanner; then no ID opens the door by hand.
    pub fn requires_id(&self, wires: Option<&Wires>) -> bool {
        !(wires.is_some_and(|w| w.is_cut(WIRE_IDSCAN)) || self.ai_disabled_id_scanner)
    }

    /// tg `isAllPowerCut()`.
    pub fn all_power_cut(wires: &Wires) -> bool {
        (wires.is_cut(WIRE_POWER1) || wires.is_cut(WIRE_POWER2))
            && (wires.is_cut(WIRE_BACKUP1) || wires.is_cut(WIRE_BACKUP2))
    }

    pub fn denied(&self) -> bool {
        self.denied_until.is_some()
    }

    /// How long the deny animation shows: airlocks `AIRLOCK_DENY_ANIMATION_TIME`, plain doors
    /// `animation_length(DOOR_DENY_ANIMATION)`; windoors define no deny length, so tg resets
    /// the frame on the next tick.
    fn deny_secs(&self) -> f64 {
        match self.kind {
            DoorKind::Airlock => 0.6,
            DoorKind::Basic => 0.3,
            _ => 0.0,
        }
    }

    /// The parts of `appearance` the door owns, for its current state.
    pub fn apply_look(&self, appearance: &mut Appearance) {
        let frame = self.state.frame();
        let s = &self.sprites;
        let deny = s
            .deny
            .filter(|_| self.denied() && self.state == DoorState::Closed);
        match deny {
            Some(id) if self.kind != DoorKind::Airlock => appearance.icon_state = id,
            _ => {
                if let Some(id) = s.frames[frame] {
                    appearance.icon_state = id;
                }
            }
        }
        appearance.layer = Layer(if self.state == DoorState::Open {
            self.open_layer
        } else {
            self.closing_layer
        });
        if self.kind != DoorKind::Airlock {
            return;
        }
        let lights = match self.state {
            DoorState::Closed if self.locked => s.bolt_lights,
            DoorState::Closed if deny.is_some() => deny,
            DoorState::Opening { .. } => s.opening_lights,
            DoorState::Closing { .. } => s.closing_lights,
            _ => None,
        };
        // tg `feedback && hasPower() && light_state`; no lights while being emagged (`MAINT`).
        // shortcut: the area's power is not checked (the look has no access to it); an airlock
        // in an unpowered area keeps its lights.
        let lights = lights.filter(|_| self.feedback && self.wire_power() && !self.emagging);
        let panel = s.panel[frame].filter(|_| self.panel_open);
        let welded = s.welded.filter(|_| self.welded);
        let sealed = s
            .sealed
            .filter(|_| self.seal.is_some() && self.state == DoorState::Closed);
        let sparks = s.sparks.filter(|_| self.emagging);
        appearance.overlays = [s.fills[frame], lights, panel, welded, sealed, sparks]
            .into_iter()
            .flatten()
            .map(Overlay::new)
            .collect();
    }
}

/// tg `/obj/structure/fluff/airlock_filler`: the invisible dense part of a multi-tile airlock on
/// its second tile. Density and opacity follow the door's; bumping it bumps the door. A child of
/// the door, so it despawns with it.
// shortcut: clicks reach the door only from next to its own tile (tg `bound_width` makes both
// tiles the door's); give click reach multi-tile bounds when clicking the second half matters.
#[derive(Component, Clone, Copy, Debug)]
pub struct DoorFiller(pub Entity);

/// tg `get_adjusted_dir`: a multi-tile airlock facing north or south extends east, otherwise
/// north.
pub fn filler_offset(dir: u8) -> Dir {
    if dir == Dir::North as u8 || dir == Dir::South as u8 {
        Dir::East
    } else {
        Dir::North
    }
}

/// On doors with an animation running or an autoclose pending.
#[derive(Component, Debug, Default)]
pub struct DoorActive;

/// Doors recognized at startup, by kind.
#[derive(Resource, Debug, Default)]
pub struct DoorReport {
    pub by_kind: std::collections::BTreeMap<DoorKind, usize>,
    /// Of those, open (non-dense) at start.
    pub open: usize,
    /// Of those, closed and bolted or welded at start.
    pub sealed: usize,
    /// Airlock helpers applied (and despawned).
    pub helpers: usize,
    /// Airlock helpers with no airlock on their tile.
    pub orphan_helpers: usize,
}

pub struct DoorServerPlugin;

impl Plugin for DoorServerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DoorReport>()
            .add_systems(MapInit, init_doors)
            .add_systems(
                Update,
                (door_bumps, door_clicks, run_doors)
                    .chain()
                    .after(WorldSystems::Movement)
                    .after(ResolveClicks),
            )
            .add_systems(PostUpdate, door_sounds.before(crate::sound::SoundSystems));
    }
}

/// [`Door`] for every [`DmDoor`] entity, then airlock mapping helpers applied and removed.
// Doors spawned later (`crate::airlock_construction`) get the same through [`setup_built`].
fn init_doors(world: &mut World) {
    let start = Instant::now();
    let Some(assets) = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.assets.clone())
    else {
        return;
    };
    // shortcut: a second manifest read at startup (the mob template reads it too); keep one
    // index resource if more startup code needs icon lookups.
    let manifest = read_manifest(&assets).unwrap_or_else(|e| panic!("doors: {e:#}"));
    let index = manifest.index();
    let mut looks: HashMap<(String, String, String, String, DoorKind), Arc<DoorSprites>> =
        HashMap::new();
    let mut query = world
        .query_filtered::<(Entity, &DmDoor, &DmAtom, Has<Density>), (With<Mapped>, Without<Door>)>(
        );
    let mut report = DoorReport::default();
    let mut sounds: HashMap<DoorSounds, Arc<DoorSounds>> = HashMap::new();
    let doors: Vec<(Entity, Door)> = query
        .iter(world)
        .map(|(entity, dm, atom, dense)| {
            let material = fill_material(dm);
            let key = (
                atom.icon.clone(),
                dm.base_icon_state.clone(),
                material,
                dm.overlays_file.clone(),
                sprite_kind(dm),
            );
            let sprites = looks
                .entry(key)
                .or_insert_with_key(|(icon, base, material, overlays, kind)| {
                    let get = |dmi: &str, state: &str| index.get(&(dmi, state)).copied();
                    Arc::new(resolve_sprites(&get, *kind, icon, base, material, overlays))
                })
                .clone();
            *report.by_kind.entry(dm.kind).or_default() += 1;
            let door_sounds = sounds
                .entry(sounds_of(dm))
                .or_insert_with_key(|k| Arc::new(k.clone()))
                .clone();
            let door = door_from(dm, atom, dense, sprites, door_sounds);
            (entity, door)
        })
        .collect();
    world.insert_batch(doors);
    spawn_fillers(world);

    let mut helpers = world.query::<(Entity, &DmAirlockHelper, &DmAtom, &TilePos)>();
    let helpers: Vec<(Entity, DmAirlockHelper, u8, TilePos)> = helpers
        .iter(world)
        .map(|(e, h, atom, p)| (e, h.clone(), atom.dir, *p))
        .collect();
    for (helper, payload, dir, pos) in helpers {
        let airlock = world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| {
                world
                    .get::<Door>(e)
                    .is_some_and(|d| d.kind == DoorKind::Airlock)
            });
        match airlock.and_then(|e| world.get_mut::<Door>(e)) {
            Some(mut door) => {
                apply_helper(&mut door, &payload, dir, pos);
                report.helpers += 1;
            }
            None => {
                warn!("airlock helper at {pos:?} has no airlock on its tile");
                report.orphan_helpers += 1;
            }
        }
        world.despawn(helper);
    }

    let mut looks = world.query_filtered::<(&Door, &mut Appearance), With<Mapped>>();
    for (door, mut appearance) in looks.iter_mut(world) {
        report.open += usize::from(door.state == DoorState::Open);
        report.sealed +=
            usize::from(door.state == DoorState::Closed && (door.locked || door.welded));
        door.apply_look(&mut appearance);
    }
    info!(
        "doors: {:?}, {} open, {} sealed, {} helpers applied ({} orphan), in {:.2?}",
        report.by_kind,
        report.open,
        report.sealed,
        report.helpers,
        report.orphan_helpers,
        start.elapsed()
    );
    world.insert_resource(report);
}

/// An airlock's sound vars as [`DoorSounds`].
fn sounds_of(dm: &DmDoor) -> DoorSounds {
    DoorSounds {
        open: dm.door_open.clone(),
        close: dm.door_close.clone(),
        deny: dm.door_deni.clone(),
        bolt_up: dm.bolt_up.clone(),
        bolt_down: dm.bolt_down.clone(),
        animation: dm.animation_sound.clone(),
    }
}

/// The live [`Door`] of a door prototype (tg `door/Initialize`).
fn door_from(
    dm: &DmDoor,
    atom: &DmAtom,
    dense: bool,
    sprites: Arc<DoorSprites>,
    sounds: Arc<DoorSounds>,
) -> Door {
    Door {
        kind: dm.kind,
        state: if dense {
            DoorState::Closed
        } else {
            DoorState::Open
        },
        locked: dm.locked,
        welded: dm.welded,
        req_access: dm.req_access.clone(),
        req_one_access: dm.req_one_access.clone(),
        unres_sides: 0,
        autoclose: dm.autoclose,
        normalspeed: dm.normalspeed,
        safe: dm.safe,
        can_open_with_hands: dm.can_open_with_hands,
        opaque_when_closed: dm.visible && !dm.glass,
        open_layer: atom.layer,
        closing_layer: dm.closing_layer,
        close_at: None,
        denied_until: None,
        sprites,
        filler: None,
        panel_open: false,
        feedback: true,
        main_outage: 0.0,
        backup_outage: 0.0,
        ai_control_disabled: dm.ai_control_disabled,
        shock_ready_at: 0.0,
        unres_latch: false,
        emergency: false,
        red_alert_access: false,
        red_alert: false,
        ai_disabled_id_scanner: false,
        sounds,
        sound: None,
        cycle_pump: None,
        cycle_requested: false,
        emagged: false,
        emagging: false,
        crush: false,
        password_lock: !dm.password.is_empty(),
        seal: None,
    }
}

/// The family whose frame names a door draws: password doors draw the bare `closed`,
/// `opening`, ... of blast doors (`password/update_icon_state`).
fn sprite_kind(dm: &DmDoor) -> DoorKind {
    if dm.password.is_empty() {
        dm.kind
    } else {
        DoorKind::Poddoor
    }
}

/// The material fill a door draws: glass airlocks draw the glass fill (tg airlock
/// `Initialize`).
fn fill_material(dm: &DmDoor) -> String {
    if dm.airlock_material.is_empty() && dm.glass {
        "glass".to_owned()
    } else {
        dm.airlock_material.clone()
    }
}

/// [`Door`] for a door spawned after map load (`crate::airlock_construction`): built as
/// [`init_doors`] builds mapped ones, its filler spawned, its look applied. False without its
/// prototype data.
pub fn setup_built(world: &mut World, entity: Entity) -> bool {
    let (Some(dm), Some(atom)) = (
        world.get::<DmDoor>(entity).cloned(),
        world.get::<DmAtom>(entity).cloned(),
    ) else {
        return false;
    };
    let dense = world.get::<Density>(entity).is_some();
    let sprites = Arc::new(sprites_from_index(
        world,
        sprite_kind(&dm),
        &atom.icon,
        &dm.base_icon_state,
        &fill_material(&dm),
        &dm.overlays_file,
    ));
    let door = door_from(&dm, &atom, dense, sprites, Arc::new(sounds_of(&dm)));
    world.entity_mut(entity).insert(door);
    spawn_fillers(world);
    with_look(world, entity);
    true
}

/// Resolves a door's sprites from the [`ss13_mapload::IconIndex`] resource (empty without it).
fn sprites_from_index(
    world: &World,
    kind: DoorKind,
    icon: &str,
    base: &str,
    material: &str,
    overlays: &str,
) -> DoorSprites {
    let Some(index) = world.get_resource::<ss13_mapload::IconIndex>() else {
        return DoorSprites::default();
    };
    resolve_sprites(
        &|dmi: &str, state: &str| index.get(dmi, state),
        kind,
        icon,
        base,
        material,
        overlays,
    )
}

/// tg airlock `change_paintjob`: another airlock type's `icon` and `overlays_file` (the
/// material fill stays), the look redrawn.
pub fn repaint(world: &mut World, entity: Entity, icon: &str, overlays_file: &str) {
    let Some((kind, base, material)) = world
        .get::<DmDoor>(entity)
        .map(|dm| (dm.kind, dm.base_icon_state.clone(), fill_material(dm)))
    else {
        return;
    };
    let sprites = Arc::new(sprites_from_index(
        world,
        kind,
        icon,
        &base,
        &material,
        overlays_file,
    ));
    if let Some(mut door) = world.get_mut::<Door>(entity) {
        door.sprites = sprites;
    }
    if let Some(mut dm) = world.get_mut::<DmDoor>(entity) {
        dm.overlays_file = overlays_file.to_owned();
    }
    if let Some(mut atom) = world.get_mut::<DmAtom>(entity) {
        atom.icon = icon.to_owned();
    }
    with_look(world, entity);
}

fn with_look(world: &mut World, entity: Entity) {
    let mut q = world.query::<(&Door, &mut Appearance)>();
    if let Ok((door, mut appearance)) = q.get_mut(world, entity) {
        door.apply_look(&mut appearance);
    }
}

/// tg `set_filler` at `Initialize` for every multi-tile door.
fn spawn_fillers(world: &mut World) {
    let mut query = world.query::<(
        Entity,
        &DmDoor,
        Option<&Door>,
        &DmAtom,
        &TilePos,
        Has<Density>,
        Has<Opaque>,
    )>();
    let multi: Vec<_> = query
        .iter(world)
        .filter(|(_, dm, door, ..)| dm.multi_tile && door.is_some_and(|d| d.filler.is_none()))
        .map(|(e, _, _, atom, pos, dense, opaque)| {
            (e, pos.step(filler_offset(atom.dir)), dense, opaque)
        })
        .collect();
    for (door, pos, dense, opaque) in multi {
        // tg `/obj/structure/fluff/airlock_filler`: `can_atmos_pass = ATMOS_PASS_DENSITY`.
        let blocker = world.get::<ss13_atmos::AtmosBlocker>(door).copied();
        let mut filler = world.spawn((DoorFiller(door), ChildOf(door)));
        if let Some(blocker) = blocker {
            filler.insert(blocker);
        }
        if dense {
            filler.insert(Density);
        }
        if opaque {
            filler.insert(Opaque);
        }
        let filler = filler.insert(pos).id();
        if let Some(mut d) = world.get_mut::<Door>(door) {
            d.filler = Some((filler, pos));
        }
    }
}

/// tg `payload()` of the airlock helpers (`mapping_helpers.dm`, `access_helpers.dm`).
// shortcut: `unres/delayed` opens at once; tg opens it after a 2-4 s `do_after` (needs do_after).
fn apply_helper(door: &mut Door, h: &DmAirlockHelper, dir: u8, pos: TilePos) {
    door.locked |= h.locked;
    door.welded |= h.welded;
    door.red_alert_access |= h.red_alert_access;
    if h.unres {
        door.unres_sides ^= dir;
        door.unres_latch = true;
    }
    // tg checks `!= null`; an empty list here is null (a map edit to `list()` is not told apart).
    if !h.access_any.is_empty() {
        if door.req_access.is_empty() {
            door.req_one_access.extend(h.access_any.iter().cloned());
        } else {
            warn!("access helper at {pos:?}: cannot mix any/all access helpers");
        }
    }
    if !h.access_all.is_empty() {
        if door.req_one_access.is_empty() {
            door.req_access.extend(h.access_all.iter().cloned());
        } else {
            warn!("access helper at {pos:?}: cannot mix any/all access helpers");
        }
    }
}

fn resolve_sprites(
    get: &dyn Fn(&str, &str) -> Option<IconStateId>,
    kind: DoorKind,
    icon: &str,
    base: &str,
    material: &str,
    overlays: &str,
) -> DoorSprites {
    const FRAMES: [&str; 4] = ["closed", "opening", "open", "closing"];
    let frames = FRAMES.map(|f| match kind {
        // `update_icon_state` of each family.
        DoorKind::Airlock => get(icon, &format!("{base}{f}")),
        DoorKind::Windoor if f == "closed" => get(icon, base),
        DoorKind::Windoor => get(icon, &format!("{base}{f}")),
        DoorKind::Basic | DoorKind::Firedoor => get(icon, &format!("{base}_{f}")),
        // `poddoor/update_icon_state`: the bare frame names.
        DoorKind::Poddoor => get(icon, f),
    });
    if kind != DoorKind::Airlock {
        let deny = match kind {
            DoorKind::Basic => get(icon, &format!("{base}_deny")),
            DoorKind::Windoor => get(icon, &format!("{base}deny")),
            _ => None,
        };
        return DoorSprites {
            frames,
            deny,
            ..default()
        };
    }
    let fills = FRAMES.map(|f| {
        if material.is_empty() {
            get(icon, &format!("fill_{f}"))
        } else {
            get(overlays, &format!("{material}_{f}"))
        }
    });
    DoorSprites {
        frames,
        fills,
        bolt_lights: get(overlays, "lights_bolts"),
        opening_lights: get(overlays, "lights_opening"),
        closing_lights: get(overlays, "lights_closing"),
        welded: get(overlays, "welded"),
        sealed: get(overlays, "sealed"),
        deny: get(overlays, "lights_denied"),
        panel: FRAMES.map(|f| get(overlays, &format!("panel_{f}"))),
        sparks: get(overlays, "sparks"),
    }
}

/// What a door's open and close checks read besides the door itself: the area's power (tg
/// `machine_stat & NOPOWER`) and the airlock's wires.
#[derive(Clone, Copy, Default)]
pub struct DoorEnv<'a> {
    pub powered: bool,
    pub wires: Option<&'a Wires>,
}

/// A mob opens or closes the door, tg `try_to_activate_door`. `bumped`: walked into it;
/// `access`: the mob's ID access. Returns whether the door needs [`DoorActive`] (it started
/// moving or shows the deny animation).
// shortcut: no buckled mobs' access or silicon access; add them with those mechanics.
#[allow(clippy::too_many_arguments)]
fn activate(
    door: &mut Door,
    appearance: &mut Appearance,
    pos: TilePos,
    self_entity: Entity,
    index: &SpatialIndex,
    now: f64,
    bumped: bool,
    opener: (TilePos, Option<&[String]>),
    env: DoorEnv,
) -> bool {
    // `try_to_activate_door` and `Bumped`: nothing works an emagged door.
    if door.emagged
        || door.password_lock
        || !door.can_open_with_hands
        || door.is_operating()
        || !door.has_power(env.powered)
    {
        return false;
    }
    let (opener_pos, access) = opener;
    // "if the ID wire is cut no ID cards are checked at all!": nothing opens it by hand.
    let allowed = door.allows(access) || door.emergency || door.unrestricted_from(pos, opener_pos);
    if !door.requires_id(env.wires) || !allowed {
        if door.state != DoorState::Closed {
            return false;
        }
        door.denied_until = Some(now + door.deny_secs());
        door.sound = Some(DoorSound::Deny);
        door.apply_look(appearance);
        return true;
    }
    match door.state {
        DoorState::Closed => open(door, appearance, now, bumped, env, false),
        DoorState::Open if !bumped => close(
            door,
            appearance,
            (pos, self_entity, index),
            now,
            false,
            env,
            false,
        ),
        _ => false,
    }
}

/// tg `open()`. Bolted or welded doors stay shut. `forced` is tg `FORCING_DOOR_CHECKS` (the
/// open wire); otherwise an airlock needs power and an uncut open wire
/// (`try_to_force_door_open`).
fn open(
    door: &mut Door,
    appearance: &mut Appearance,
    now: f64,
    bumped: bool,
    env: DoorEnv,
    forced: bool,
) -> bool {
    // `airlock/open`: a bolted airlock with a cycling pump asks the pump instead.
    if door.cycle_pump.is_some()
        && door.state == DoorState::Closed
        && door.locked
        && !door.welded
        && door.seal.is_none()
    {
        door.cycle_requested = true;
        return false;
    }
    if door.state != DoorState::Closed
        || door.locked
        || door.welded
        || door.seal.is_some()
        || emag_stuck(door)
    {
        return false;
    }
    if door.kind == DoorKind::Airlock
        && !forced
        && (!door.has_power(env.powered) || env.wires.is_some_and(|w| w.is_cut(WIRE_OPEN)))
    {
        return false;
    }
    start_opening(door, appearance, now, bumped);
    true
}

/// tg `try_to_force_door_open`/`try_to_force_door_shut` of airlocks and windoors refuse an
/// emagged door unless `BYPASS_DOOR_CHECKS`.
// shortcut: the crowbar's `BYPASS_DOOR_CHECKS` (`forced` here, as the open wire's
// `FORCING_DOOR_CHECKS`) is refused too; it only matters for an emagged airlock its wires
// unbolted.
fn emag_stuck(door: &Door) -> bool {
    door.emagged && matches!(door.kind, DoorKind::Airlock | DoorKind::Windoor)
}

/// The opening animation starts; `bumped` schedules a windoor's close.
fn start_opening(door: &mut Door, appearance: &mut Appearance, now: f64, bumped: bool) {
    door.denied_until = None;
    let t = door.timings();
    door.close_at = match door.kind {
        DoorKind::Airlock if door.autoclose => Some(now + if door.normalspeed { 8.0 } else { 1.5 }),
        // `open_and_close`: only a bump schedules the close; secure windoors close sooner.
        DoorKind::Windoor if bumped => {
            let free = door.req_access.is_empty() && door.req_one_access.is_empty();
            Some(now + t.open_done + if free { 8.0 } else { 5.0 })
        }
        DoorKind::Basic if door.autoclose => Some(now + t.open_done + DOOR_CLOSE_WAIT_SECS),
        _ => None,
    };
    door.state = DoorState::Opening { since: now };
    door.sound = Some(DoorSound::Open);
    door.apply_look(appearance);
}

/// tg `close()`: a safe door refuses while anything else dense is in the doorway (both tiles of
/// a multi-tile door); an automatic close (`auto`) or an autoclosing door retries
/// [`DOOR_CLOSE_WAIT_SECS`] later. Windoors (`windowdoor.dm` `close`) never check. Unless
/// `forced`, an airlock needs power and an uncut bolts wire.
fn close(
    door: &mut Door,
    appearance: &mut Appearance,
    (pos, self_entity, index): (TilePos, Entity, &SpatialIndex),
    now: f64,
    auto: bool,
    env: DoorEnv,
    forced: bool,
) -> bool {
    if door.state != DoorState::Open || door.locked || door.welded || emag_stuck(door) {
        return false;
    }
    if door.kind == DoorKind::Airlock
        && !forced
        && (!door.has_power(env.powered) || env.wires.is_some_and(|w| w.is_cut(WIRE_BOLTS)))
    {
        return false;
    }
    let filler = door.filler.map(|(e, _)| e);
    let blocked = door.doorway(pos).any(|tile| {
        let border = index.border_on_tile(tile).iter().map(|&(e, _)| e);
        index
            .dense_on_tile(tile)
            .iter()
            .copied()
            .chain(border)
            .any(|e| e != self_entity && Some(e) != filler)
    });
    if door.safe && door.kind != DoorKind::Windoor && blocked {
        if auto || door.autoclose {
            door.close_at = Some(now + DOOR_CLOSE_WAIT_SECS);
        }
        return false;
    }
    door.close_at = None;
    // tg `dangerous_close = !safe`; windoors override `close` without `crush`.
    door.crush = !door.safe && door.kind != DoorKind::Windoor;
    door.state = DoorState::Closing { since: now };
    door.sound = Some(DoorSound::Close);
    door.apply_look(appearance);
    true
}

type DoorItem<'a> = (
    Entity,
    &'a mut Door,
    &'a mut Appearance,
    &'a TilePos,
    Has<Unpowered>,
    Option<&'a Wires>,
);

/// tg `Bumped`: a mob walking into a closed door tries to open it.
/// Windoors are border objects (`ss13_world::Border`), so movement bumps them only across their
/// edge: from the front, or leaving their own tile through it. A bumped filler bumps its door.
/// A mech opens doors with its pilot's access (`/obj/vehicle/sealed/Bump`), a ridden vehicle with
/// its driver's, a bot with its own.
// shortcut: "mob" = has `Mover`; tg also lets thrown items with access bump doors.
#[allow(clippy::too_many_arguments)]
fn door_bumps(
    mut bumps: MessageReader<Bumped>,
    fillers: Query<&DoorFiller>,
    time: Res<Time>,
    index: Res<SpatialIndex>,
    movers: Query<
        (
            &TilePos,
            Option<&Inventory>,
            Option<&crate::mecha::Mech>,
            Option<&crate::bots::Bot>,
            Option<&crate::vehicles::Vehicle>,
        ),
        With<Mover>,
    >,
    pilots: Query<&Inventory>,
    cards: IdCards,
    // Electrified airlocks shock first (`crate::wires`).
    mut doors: Query<DoorItem, Without<Electrified>>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for bump in bumps.read() {
        let Ok((&mover_pos, inventory, mech, bot, vehicle)) = movers.get(bump.entity) else {
            continue;
        };
        // A ridden vehicle opens with its driver's (`/datum/component/riding/vehicle_bump`).
        let inventory = match (mech, vehicle) {
            (Some(mech), _) => mech.occupant.and_then(|p| pilots.get(p).ok()),
            (None, Some(vehicle)) => vehicle.rider.and_then(|p| pilots.get(p).ok()),
            (None, None) => inventory,
        };
        let obstacle = fillers.get(bump.obstacle).map_or(bump.obstacle, |f| f.0);
        let Ok((entity, mut door, mut appearance, pos, unpowered, wires)) = doors.get_mut(obstacle)
        else {
            continue;
        };
        // A bot opens with its access card (`/mob/living/basic/bot` `access_card`).
        let access = inventory
            .and_then(|i| i.access(&cards))
            .or(bot.map(|b| b.access.as_slice()));
        if activate(
            &mut door,
            &mut appearance,
            *pos,
            entity,
            &index,
            now,
            true,
            (mover_pos, access),
            DoorEnv {
                powered: !unpowered,
                wires,
            },
        ) {
            commands.entity(entity).insert(DoorActive);
        }
    }
}

/// A resolved click (`crate::click`: alive, seen, adjacent) on a door activates it, unless the
/// airlock's panel or wires take it (`crate::wires::airlock_takes_click`).
#[allow(clippy::too_many_arguments)]
fn door_clicks(
    mut clicks: MessageReader<ClickResolved>,
    time: Res<Time>,
    index: Res<SpatialIndex>,
    inventories: Query<&Inventory>,
    cards: IdCards,
    tools: Query<&ss13_mapload::DmTool>,
    items: Query<&ss13_mapload::DmItem>,
    electrified: Query<(), With<Electrified>>,
    mut doors: Query<DoorItem>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for click in clicks.read() {
        let Ok((entity, mut door, mut appearance, pos, unpowered, wires)) =
            doors.get_mut(click.target)
        else {
            continue;
        };
        let inventory = inventories.get(click.mob).ok();
        let held = inventory.and_then(Inventory::active_item);
        // An emag is used on the door (`crate::emag`), an airlock painter paints it
        // (`crate::airlock_construction`); neither works it.
        if held.and_then(|h| items.get(h).ok()).is_some_and(|i| {
            ss13_mapload::fidelity::is_under(&i.path, crate::emag::EMAG)
                || ss13_mapload::fidelity::is_under(&i.path, crate::airlock_construction::PAINTER)
        }) {
            continue;
        }
        // `airlock/item_interaction` with a pneumatic seal (`crate::devices::door_seal`).
        if door.kind == DoorKind::Airlock
            && let Some(seal) = held.filter(|&h| {
                items.get(h).is_ok_and(|i| {
                    ss13_mapload::fidelity::is_under(&i.path, crate::devices::door_seal::DOOR_SEAL)
                })
            })
        {
            let (client, mob) = (click.client, click.mob);
            commands.queue(move |world: &mut World| {
                crate::devices::door_seal::start_seal(world, client, mob, entity, seal);
            });
            continue;
        }
        // `password/try_to_activate_door`: a closed one asks for its password.
        if door.password_lock {
            if door.state == DoorState::Closed && !door.is_operating() {
                let (client, mob) = (click.client, click.mob);
                commands.queue(move |world: &mut World| {
                    crate::password_door::ask_for_pass(world, client, mob, entity);
                });
            }
            continue;
        }
        // BandaStation knock: a bare right click, unless an open panel takes the hand first
        // (`machinery/attack_hand` -> the wires); firelocks keep their own `attack_hand`.
        if click.modifiers.right
            && held.is_none()
            && door.kind != DoorKind::Firedoor
            && !(door.kind == DoorKind::Airlock && door.panel_open)
        {
            let mob = click.mob;
            commands.queue(move |world: &mut World| knock(world, mob, entity));
            continue;
        }
        let tool = held
            .and_then(|h| tools.get(h).ok())
            .and_then(|t| crate::tools::ToolKind::from_dm(&t.behaviour));
        if crate::wires::airlock_takes_click(
            &door,
            held.is_some(),
            tool,
            electrified.contains(entity),
        ) {
            continue;
        }
        // `door/attack_hand` -> `try_remove_seal`.
        if held.is_none() && door.seal.is_some() {
            let (client, mob) = (click.client, click.mob);
            commands.queue(move |world: &mut World| {
                crate::devices::door_seal::start_unseal(world, client, mob, entity);
            });
            continue;
        }
        let access = inventory.and_then(|i| i.access(&cards));
        if activate(
            &mut door,
            &mut appearance,
            *pos,
            entity,
            &index,
            now,
            false,
            (click.mob_pos, access),
            DoorEnv {
                powered: !unpowered,
                wires,
            },
        ) {
            commands.entity(entity).insert(DoorActive);
        }
    }
}

/// Advances running animations and pending autocloses, applying density and opacity at tg's
/// points of each animation. Effects due in one tick all apply in it, so a slow tick never skips
/// a step.
fn run_doors(
    time: Res<Time>,
    index: Res<SpatialIndex>,
    movers: Query<(), With<Mover>>,
    mut doors: Query<(DoorItem, Has<Density>, Has<Opaque>), With<DoorActive>>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for ((entity, mut door, mut appearance, pos, unpowered, wires), dense, opaque) in &mut doors {
        let env = DoorEnv {
            powered: !unpowered,
            wires,
        };
        let t = door.timings();
        let filler = door.filler.map(|(f, _)| f);
        let mut e = commands.entity(entity);
        if door.denied_until.is_some_and(|until| now >= until) {
            door.denied_until = None;
            door.apply_look(&mut appearance);
        }
        match door.state {
            DoorState::Opening { since } => {
                let elapsed = now - since;
                if opaque && elapsed >= t.open_transparent {
                    e.remove::<Opaque>();
                    if let Some(f) = filler {
                        e.commands().entity(f).remove::<Opaque>();
                    }
                }
                if dense && elapsed >= t.open_passable {
                    e.remove::<Density>();
                    if let Some(f) = filler {
                        e.commands().entity(f).remove::<Density>();
                    }
                }
                if elapsed >= t.open_done {
                    door.state = DoorState::Open;
                    door.apply_look(&mut appearance);
                }
            }
            DoorState::Open => {
                if door.close_at.is_some_and(|at| now >= at) {
                    door.close_at = None;
                    // tg `autoclose()`: bolted-open, welded-open or unpowered doors stay open.
                    if door.has_power(!unpowered) {
                        close(
                            &mut door,
                            &mut appearance,
                            (*pos, entity, &index),
                            now,
                            true,
                            env,
                            false,
                        );
                    }
                }
            }
            DoorState::Closing { since } => {
                let elapsed = now - since;
                if !dense && elapsed >= t.close_dense {
                    e.insert(Density);
                    if let Some(f) = filler {
                        e.commands().entity(f).insert(Density);
                    }
                }
                // `airlock/close` crushes before turning opaque, `door/close` at the end.
                let crush_at = if door.kind == DoorKind::Airlock {
                    t.close_opaque
                } else {
                    t.close_done
                };
                if door.crush && elapsed >= crush_at {
                    door.crush = false;
                    let tiles: Vec<TilePos> = door.doorway(*pos).collect();
                    e.commands()
                        .queue(move |world: &mut World| crush(world, entity, &tiles));
                }
                if !opaque && door.opaque_when_closed && elapsed >= t.close_opaque {
                    e.insert(Opaque);
                    if let Some(f) = filler {
                        e.commands().entity(f).insert(Opaque);
                    }
                }
                if elapsed >= t.close_done {
                    door.state = DoorState::Closed;
                    door.apply_look(&mut appearance);
                    // tg `CheckForMobs`: a mob that stepped in while it closed reopens a safe
                    // door (an unsafe one crushed it instead); windoors override `close`
                    // without it.
                    // shortcut: at once instead of tg's 0.1 s later.
                    let mob_inside = door
                        .doorway(*pos)
                        .any(|tile| index.on_tile(tile).iter().any(|&m| movers.contains(m)));
                    if mob_inside && door.safe && door.kind != DoorKind::Windoor {
                        open(&mut door, &mut appearance, now, false, env, false);
                    }
                }
            }
            DoorState::Closed => {}
        }
        let pending = door.is_operating() || door.close_at.is_some() || door.denied();
        if !pending {
            e.remove::<DoorActive>();
        }
    }
}

/// tg `animation_effects` for doors that started an animation this tick, wherever it was
/// started: airlocks (`doorOpen`/`doorClose` 30, `doorDeni` 50 with the lights working,
/// `airlockforced` 30 when forced with `BYPASS_DOOR_CHECKS`), blast doors and shutters
/// (`animation_sound` 50) and windoor `open()`/`close()` (`windowdoor.ogg` 100). Plain doors and
/// firelocks have no animation sound in tg.
fn door_sounds(
    mut doors: Query<(&mut Door, &TilePos), Changed<Door>>,
    mut out: MessageWriter<crate::sound::PlaySound>,
) {
    use crate::sound::PlaySound;
    use ss13_mapload::sound::files;
    for (mut door, &pos) in &mut doors {
        let Some(sound) = door.sound else {
            continue;
        };
        door.bypass_change_detection().sound = None;
        let s = &door.sounds;
        let play = match (door.kind, sound) {
            // `password/animation_effects`.
            (_, DoorSound::Open) if door.password_lock => PlaySound::new(pos, &*s.open, 50.0, true),
            (_, DoorSound::Close) if door.password_lock => {
                PlaySound::new(pos, &*s.close, 50.0, true)
            }
            (_, DoorSound::Deny) if door.password_lock => PlaySound::new(pos, &*s.deny, 30.0, true),
            (DoorKind::Airlock, DoorSound::Forced) => {
                PlaySound::new(pos, files::AIRLOCK_FORCED, 30.0, true)
            }
            (DoorKind::Airlock, DoorSound::Open) => PlaySound::new(pos, &*s.open, 30.0, true),
            (DoorKind::Airlock, DoorSound::Close) => PlaySound::new(pos, &*s.close, 30.0, true),
            (DoorKind::Airlock, DoorSound::Deny) if door.feedback => {
                PlaySound::new(pos, &*s.deny, 50.0, false).extrarange(3.0)
            }
            (DoorKind::Poddoor, DoorSound::Open | DoorSound::Close) => {
                PlaySound::new(pos, &*s.animation, 50.0, true)
            }
            (DoorKind::Windoor, DoorSound::Open | DoorSound::Close) => {
                PlaySound::new(pos, files::WINDOOR, 100.0, true)
            }
            _ => continue,
        };
        if !play.sound.is_empty() {
            out.write(play);
        }
    }
}

/// tg `DOOR_CRUSH_DAMAGE`.
pub const DOOR_CRUSH_DAMAGE: f32 = 20.0;
/// `crush()`: a human's `Paralyze(10 SECONDS)`.
const CRUSH_PARALYZE_SECS: f32 = 10.0;

/// tg `/obj/machinery/door/proc/crush`: every living mob in the doorway takes
/// [`DOOR_CRUSH_DAMAGE`] brute to the chest (a human also screams and is paralyzed for 10 s),
/// every mech takes it as object damage. Border firelocks have `can_crush = FALSE`.
// shortcut: no blood splatter, wounds (`wound_bonus` 10) or xenomorph ×1.5 (no aliens yet).
fn crush(world: &mut World, door: Entity, tiles: &[TilePos]) {
    use crate::health::{DamageType, Health, Stat, apply_damage_zone};
    if world.get_entity(door).is_err() || world.get::<ss13_world::Border>(door).is_some() {
        return;
    }
    let door_name = crate::security::name_of(world, door);
    for &tile in tiles {
        let here: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(tile).to_vec();
        for victim in here {
            if world.get::<crate::mecha::Mech>(victim).is_some() {
                crate::mecha::core::take_damage(
                    world,
                    victim,
                    DOOR_CRUSH_DAMAGE,
                    DamageType::Brute,
                    "",
                    None,
                );
                continue;
            }
            if world.get::<Health>(victim).is_none() || world.get::<Stat>(victim).is_none() {
                continue;
            }
            let name = crate::security::name_of(world, victim);
            crate::security::show(
                world,
                tile,
                7,
                ss13_shared::chat::ChatChannel::Combat,
                (
                    ss13_shared::chat::Span::Warning,
                    format!("{door_name} закрывается, раздавливая {name}!"),
                ),
                &[(
                    victim,
                    ss13_shared::chat::Span::Danger,
                    format!("{door_name} закрывается и раздавливает вас!"),
                )],
            );
            let human = world.get::<crate::human::HumanLook>(victim).is_some();
            if human {
                crate::medical::diseases::emote(world, victim, "scream");
            }
            if let Ok(mut entity) = world.get_entity_mut(victim)
                && let Ok((mut health, mut stat)) =
                    entity.get_components_mut::<(&mut Health, &mut Stat)>()
            {
                apply_damage_zone(
                    &mut health,
                    &mut stat,
                    crate::zone::BodyZone::Chest,
                    DamageType::Brute,
                    DOOR_CRUSH_DAMAGE,
                );
            }
            if human {
                crate::status_effects::apply(
                    world,
                    victim,
                    crate::status_effects::Incap::Paralyze,
                    CRUSH_PARALYZE_SECS,
                );
            }
        }
    }
}

/// tg `/obj/machinery/door/airlock/try_to_crowbar` with a plain crowbar: bolts and welds stop
/// it; an unpowered airlock that is not moving opens or closes at once with
/// `BYPASS_DOOR_CHECKS` (`airlockforced`), a powered one resists. True when `target` is an
/// airlock (the click is handled).
// shortcut: no electronics removal (open panel, `security_level` 0). A forcing
// tool on a powered airlock is [`force_open_act`].
pub fn crowbar_act(world: &mut World, client: Entity, target: Entity) -> bool {
    let warning = with_door(world, target, |door, appearance, ctx| {
        if door.kind != DoorKind::Airlock {
            return (None, false);
        }
        let text = if door.seal.is_some() {
            "Сначала снимите герметизатор!"
        } else if door.locked {
            "The airlock's bolts prevent it from being forced!"
        } else if door.welded {
            "It's welded, it won't budge!"
        } else if door.has_power(ctx.env.powered) {
            "The airlock's motors resist your efforts to force it!"
        } else {
            let moved = match door.state {
                DoorState::Closed => ctx.open(door, appearance, true),
                DoorState::Open => ctx.close(door, appearance, true),
                _ => false,
            };
            if moved {
                door.sound = Some(DoorSound::Forced);
            }
            return (Some(None), moved);
        };
        (Some(Some(text)), false)
    });
    match warning.flatten() {
        None => false,
        Some(None) => true,
        Some(Some(text)) => {
            crate::tools::tell(world, client, ss13_shared::chat::Span::Warning, text);
            true
        }
    }
}

/// A timed door tool action ([`crate::construction::Job::Door`]), with the welded state it
/// started from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoorJob {
    /// `try_to_weld_secondary`: weld shut (`welded: false`) or unweld.
    Weld { welded: bool },
    /// `try_to_crowbar(forced)`: a forcing tool prying a powered airlock open.
    Pry { tool: Entity },
}

/// Items with `force_opens` that pry powered airlocks: the jaws of life and their subtypes.
// shortcut: no wielded fireaxe or other forcing items; add them with their own modules.
const FORCING_TOOLS: [&str; 1] = ["/obj/item/crowbar/power"];
/// `/obj/item/crowbar/power/paramedic`: `limit_jaws_access`, `radio_alert`.
const PARAMEDIC_JAWS: &str = "/obj/item/crowbar/power/paramedic";
/// The paramedic jaws' `blacklisted_access`.
const PARAMEDIC_BLACKLIST: [&str; 15] = [
    "command",
    "ai_upload",
    "captain",
    "hop",
    "security",
    "brig",
    "armory",
    "hos",
    "detective",
    "ce",
    "cmo",
    "qm",
    "vault",
    "rd",
    "syndicate",
];
/// The jaws' `ignored_access`: doors with these never raise the radio alert.
const JAWS_IGNORED_ACCESS: [&str; 3] = ["maint_tunnels", "aux_base", "external airlocks"];
/// `try_to_crowbar`'s `time_to_open`, deciseconds.
const PRY_DS: f32 = 50.0;
/// The jaws' `alert_cooldown_time`, seconds.
const JAWS_ALERT_COOLDOWN: f64 = 60.0;

/// The paramedic jaws' `alert_cooldown`: the next radio alert is allowed from this time on.
#[derive(Component, Clone, Copy, Debug)]
pub struct JawsAlertReadyAt(pub f64);

/// `/obj/item/crowbar/power/on_force_open`'s `limit_jaws_access`: false when the door needs a
/// blacklisted access.
pub fn jaws_may_force(tool_path: &str, door: &Door) -> bool {
    !crate::obj_damage::is_under(tool_path, PARAMEDIC_JAWS)
        || !door
            .req_access
            .iter()
            .chain(&door.req_one_access)
            .any(|a| PARAMEDIC_BLACKLIST.contains(&a.as_str()))
}

/// `on_force_open`'s radio alert check: the door has some access and none of it is ignored.
/// tg's `!collective_access` never holds for a list, so a door without access alerts there; here
/// it does not (the intended "no access at all" early return).
pub fn jaws_alert_needed(door: &Door) -> bool {
    let mut access = door
        .req_access
        .iter()
        .chain(&door.req_one_access)
        .peekable();
    access.peek().is_some() && !access.any(|a| JAWS_IGNORED_ACCESS.contains(&a.as_str()))
}

/// tg `/obj/machinery/door/airlock/try_to_crowbar(tool, user, forced = TRUE)` with a forcing
/// tool on a powered airlock: an electrified door shocks first, then 5 s of prying
/// (`airlock_alien_prying.ogg`). True when the click is handled; false leaves the bolted,
/// welded or unpowered airlock to [`crowbar_act`].
// shortcut: the paramedic jaws refuse a blacklisted door before the 5 s, not after (tg checks
// on `COMSIG_TOOL_FORCE_OPEN_AIRLOCK` at the end); no `AIRLOCK_PRY_DAMAGE` (doors have no
// integrity yet).
pub fn force_open_act(world: &mut World, actor: crate::tools::Actor, target: Entity) -> bool {
    use ss13_shared::chat::Span;
    let Some(item) = actor.item else {
        return false;
    };
    let Some(path) = world
        .get::<ss13_mapload::DmItem>(item)
        .map(|i| i.path.clone())
    else {
        return false;
    };
    if !FORCING_TOOLS
        .iter()
        .any(|root| crate::obj_damage::is_under(&path, root))
    {
        return false;
    }
    let powered = world.get::<Unpowered>(target).is_none();
    let Some(door) = world.get::<Door>(target) else {
        return false;
    };
    if door.kind != DoorKind::Airlock
        || door.locked
        || door.welded
        || door.seal.is_some()
        || !door.has_power(powered)
    {
        return false;
    }
    let (closed, allowed) = (door.state == DoorState::Closed, jaws_may_force(&path, door));
    if world.get::<Electrified>(target).is_some()
        && crate::wires::shock(world, target, actor.mob, 100.0)
    {
        return true;
    }
    if !closed {
        return true;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return true;
    };
    if !allowed {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/machines/buzz/buzz-sigh.ogg", 50.0, false),
        );
        crate::tools::tell(world, actor.client, Span::Warning, "не удаётся вскрыть!");
        return true;
    }
    if crate::tools::use_tool(
        world,
        actor,
        target,
        PRY_DS,
        0,
        crate::construction::Job::Door(DoorJob::Pry { tool: item }),
    ) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(
                pos,
                ss13_mapload::sound::files::POWER_TOOLS[2],
                100.0,
                true,
            ),
        );
    }
    true
}

/// The end of [`DoorJob::Pry`]: still a closed, unbolted, unwelded airlock (re-checked after the
/// 5 s); an electrified one shocks again; then it opens with `BYPASS_DOOR_CHECKS` and the
/// paramedic jaws alert the radio.
fn complete_pry(world: &mut World, mob: Entity, target: Entity, tool: Entity) {
    if world.get::<Electrified>(target).is_some() && crate::wires::shock(world, target, mob, 100.0)
    {
        return;
    }
    // `Some(alert needed)` once it opened.
    let Some(alert) = with_door(world, target, |door, appearance, ctx| {
        if door.kind != DoorKind::Airlock || door.locked || door.welded || door.seal.is_some() {
            return (None, false);
        }
        let opened = ctx.open_bypass(door, appearance);
        if opened {
            door.sound = Some(DoorSound::Forced);
        }
        (opened.then(|| jaws_alert_needed(door)), opened)
    })
    .flatten() else {
        return;
    };
    let paramedic = world
        .get::<ss13_mapload::DmItem>(tool)
        .is_some_and(|i| crate::obj_damage::is_under(&i.path, PARAMEDIC_JAWS));
    let now = world.resource::<Time>().elapsed_secs_f64();
    if !alert
        || !paramedic
        || world
            .get::<JawsAlertReadyAt>(tool)
            .is_some_and(|r| now < r.0)
    {
        return;
    }
    world
        .entity_mut(tool)
        .insert(JawsAlertReadyAt(now + JAWS_ALERT_COOLDOWN));
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    let (person, tool_name) = (
        crate::security::name_of(world, mob),
        crate::security::name_of(world, tool),
    );
    let location = crate::bots::area_name(world, pos);
    let tail = format!(
        "{person} has forced open a door at {location} using {tool_name}. Confirm that this was done during an emergency by authorized staff."
    );
    // `/datum/aas_config_entry/jaws_entry_alert_paramedic`.
    for (freq, head) in [
        (ss13_chat::radio::FREQ_SECURITY, "SECURITY ALERT"),
        (ss13_chat::radio::FREQ_MEDICAL, "MEDICAL ALERT"),
    ] {
        world.write_message(crate::radio::MachineRadio {
            freq,
            z: pos.z,
            name: "Automated Announcement System".to_owned(),
            text: format!("{head}: {tail}"),
        });
    }
}

/// `use_tool(src, user, 40)` of `try_to_weld_secondary`, deciseconds.
const WELD_DS: f32 = 40.0;

/// tg `weld_checks`: not moving and closed.
fn weld_checks(door: &Door) -> bool {
    !door.is_operating() && door.state == DoorState::Closed
}

/// The weld flips if the door still is as the job found it (`weld_checks`, same weld).
fn toggle_weld(door: &mut Door, from: bool) -> bool {
    if door.welded != from || !weld_checks(door) {
        return false;
    }
    door.welded = !from;
    true
}

/// tg `/obj/machinery/door/airlock/try_to_weld_secondary` (right click with a lit welder):
/// 4 s, then the airlock is welded shut or unwelded. True when `target` is an airlock.
// shortcut: no left-click repair (`try_to_weld`): doors have no integrity yet.
pub fn welder_act_secondary(world: &mut World, actor: crate::tools::Actor, target: Entity) -> bool {
    use ss13_shared::chat::Span;
    let Some(door) = world.get::<Door>(target) else {
        return false;
    };
    // `firedoor/try_to_weld_secondary`: the same 4 s weld (the first step of taking it apart,
    // `crate::door_structures`).
    if !matches!(door.kind, DoorKind::Airlock | DoorKind::Firedoor) {
        return false;
    }
    let welded = door.welded;
    let Some(item) = actor.item else {
        return true;
    };
    // `tool_start_check(user, amount = 1)`.
    if !crate::tools::tool_usable(world, item, 1, 0.0) {
        let text = crate::tools::unusable_message(world, item, 1);
        crate::tools::tell(world, actor.client, Span::Warning, text);
        return true;
    }
    // `extra_checks = weld_checks` already fail on the first tick of the `do_after`.
    if !world.get::<Door>(target).is_some_and(weld_checks) {
        return true;
    }
    if crate::tools::use_tool(
        world,
        actor,
        target,
        WELD_DS,
        0,
        crate::construction::Job::Door(DoorJob::Weld { welded }),
    ) {
        let Some(&pos) = world.get::<TilePos>(target) else {
            return true;
        };
        let user = crate::security::name_of(world, actor.mob);
        let (others, own) = if welded {
            (
                format!("{user} начинает разваривать шлюз."),
                "Вы начинаете разваривать шлюз...",
            )
        } else {
            (
                format!("{user} начинает заваривать шлюз."),
                "Вы начинаете заваривать шлюз...",
            )
        };
        crate::security::show(
            world,
            pos,
            7,
            ss13_shared::chat::ChatChannel::Local,
            (Span::Notice, others),
            &[(actor.mob, Span::Notice, own.to_owned())],
        );
    }
    true
}

/// The end of a [`DoorJob`], re-checked at the moment of effect: the door is still closed and
/// still, and its weld is as the job found it.
pub fn complete(world: &mut World, _client: Entity, mob: Entity, target: Entity, job: DoorJob) {
    use ss13_shared::chat::Span;
    let from = match job {
        DoorJob::Weld { welded } => welded,
        DoorJob::Pry { tool } => return complete_pry(world, mob, target, tool),
    };
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    let done = with_door(world, target, |door, appearance, _| {
        let done = toggle_weld(door, from);
        if done {
            door.apply_look(appearance);
        }
        (done, false)
    });
    if done != Some(true) {
        return;
    }
    let user = crate::security::name_of(world, mob);
    let (others, own) = if from {
        (format!("{user} разваривает шлюз."), "Вы развариваете шлюз.")
    } else {
        (format!("{user} заваривает шлюз."), "Вы завариваете шлюз.")
    };
    info!(target: "game", "{mob:?} {} airlock {target:?}", if from { "unwelded" } else { "welded" });
    crate::security::show(
        world,
        pos,
        7,
        ss13_shared::chat::ChatChannel::Local,
        (Span::Notice, others),
        &[(mob, Span::Notice, own.to_owned())],
    );
}

/// BandaStation `knock_sound` of doors (`door_metal_knock_1.ogg`) and of glass airlocks and
/// windoors (`glassknock.ogg`).
pub const KNOCK_METAL: &str = ss13_mapload::sound::files::DOORS[0];
pub const KNOCK_GLASS: &str = ss13_mapload::sound::files::DOORS[1];

/// BandaStation `knock_on` (right click with an empty hand, `door/attack_hand`): the knock at
/// volume 100, the message, `CLICK_CD_MELEE`.
fn knock(world: &mut World, mob: Entity, door: Entity) {
    use ss13_shared::chat::Span;
    let Some((&pos, d)) = world.get::<TilePos>(door).zip(world.get::<Door>(door)) else {
        return;
    };
    let glass =
        d.kind == DoorKind::Windoor || (d.kind == DoorKind::Airlock && !d.opaque_when_closed);
    let now = world.resource::<Time>().elapsed_secs_f64();
    world
        .entity_mut(mob)
        .insert(crate::combat::NextMove(now + crate::combat::CLICK_CD_MELEE));
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(
            pos,
            if glass { KNOCK_GLASS } else { KNOCK_METAL },
            100.0,
            true,
        ),
    );
    let (user, name) = (
        crate::security::name_of(world, mob),
        crate::security::name_of(world, door),
    );
    crate::security::show(
        world,
        pos,
        7,
        ss13_shared::chat::ChatChannel::Local,
        (Span::Notice, format!("{user} стучит в {name}.")),
        &[(mob, Span::Notice, format!("Вы стучите в {name}."))],
    );
}

/// Runs `f` on a door's live state at the moment of effect, for other modules (`crate::wires`),
/// and marks it [`DoorActive`] when `f` says something is pending. `None`: not a door.
pub fn with_door<R>(
    world: &mut World,
    door: Entity,
    f: impl FnOnce(&mut Door, &mut Appearance, DoorCtx) -> (R, bool),
) -> Option<R> {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let (result, active) = world.resource_scope(|world, index: Mut<SpatialIndex>| {
        let mut query = world.query::<DoorItem>();
        let (entity, mut door, mut appearance, &pos, unpowered, wires) =
            query.get_mut(world, door).ok()?;
        let ctx = DoorCtx {
            entity,
            pos,
            index: &index,
            now,
            env: DoorEnv {
                powered: !unpowered,
                wires,
            },
        };
        Some(f(&mut door, &mut appearance, ctx))
    })?;
    if active {
        world.entity_mut(door).insert(DoorActive);
    }
    Some(result)
}

/// Where a door is and what its checks read, for [`with_door`].
pub struct DoorCtx<'a> {
    pub entity: Entity,
    pub pos: TilePos,
    pub index: &'a SpatialIndex,
    pub now: f64,
    pub env: DoorEnv<'a>,
}

impl DoorCtx<'_> {
    /// tg `open(forced)`: `forced` is `FORCING_DOOR_CHECKS`.
    pub fn open(&self, door: &mut Door, appearance: &mut Appearance, forced: bool) -> bool {
        open(door, appearance, self.now, false, self.env, forced)
    }

    /// tg windoor `open(BYPASS_DOOR_CHECKS)`: a closed door opens whatever else holds it.
    pub fn open_bypass(&self, door: &mut Door, appearance: &mut Appearance) -> bool {
        // `airlock/open` refuses a sealed airlock even with `BYPASS_DOOR_CHECKS`.
        if door.state != DoorState::Closed || door.seal.is_some() {
            return false;
        }
        start_opening(door, appearance, self.now, false);
        true
    }

    /// tg `close(forced)`.
    pub fn close(&self, door: &mut Door, appearance: &mut Appearance, forced: bool) -> bool {
        close(
            door,
            appearance,
            (self.pos, self.entity, self.index),
            self.now,
            false,
            self.env,
            forced,
        )
    }

    /// tg `run_animation(DOOR_DENY_ANIMATION)`.
    pub fn deny(&self, door: &mut Door, appearance: &mut Appearance) -> bool {
        door.denied_until = Some(self.now + door.deny_secs());
        door.sound = Some(DoorSound::Deny);
        door.apply_look(appearance);
        true
    }

    /// tg `try_to_activate_door` by `opener` (its tile and ID access).
    pub fn activate(
        &self,
        door: &mut Door,
        appearance: &mut Appearance,
        bumped: bool,
        opener: (TilePos, Option<&[String]>),
    ) -> bool {
        activate(
            door,
            appearance,
            self.pos,
            self.entity,
            self.index,
            self.now,
            bumped,
            opener,
            self.env,
        )
    }
}

/// tg `door/hostile_lockdown(origin)` (the malfunctioning AI's lockdown): a powered door
/// closes. An airlock also needs the AI to control it (`canAIControl`); it is unbolted ("for
/// airlocks that were bolted open"), made unsafe (it crushes), closed, bolted and electrified
/// for good. True when the door took part.
// shortcut: `shockedby` is not logged (no airlock shock log yet).
pub fn hostile_lockdown(world: &mut World, door: Entity) -> bool {
    let Some(kind) = world.get::<Door>(door).map(|d| d.kind) else {
        return false;
    };
    // `if(!machine_stat)`.
    if world.get::<Unpowered>(door).is_some() {
        return false;
    }
    let airlock = kind == DoorKind::Airlock;
    if airlock && !crate::silicon::ai::can_ai_control(world, door) {
        return false;
    }
    with_door(world, door, |d, appearance, ctx| {
        if airlock {
            d.locked = false;
            d.safe = false;
        }
        let moved = d.state == DoorState::Open && ctx.close(d, appearance, false);
        ((), moved)
    });
    if airlock {
        crate::wires::set_bolts(world, door, true);
        crate::wires::set_electrified(world, door, Some(crate::wires::Electrified::Permanent));
    }
    true
}

/// tg `door/disable_lockdown()`: a powered door opens; an airlock the AI controls is unbolted,
/// no longer electrified, opened and safe again. True when the door took part.
pub fn disable_lockdown(world: &mut World, door: Entity) -> bool {
    let Some(kind) = world.get::<Door>(door).map(|d| d.kind) else {
        return false;
    };
    if world.get::<Unpowered>(door).is_some() {
        return false;
    }
    let airlock = kind == DoorKind::Airlock;
    if airlock {
        if !crate::silicon::ai::can_ai_control(world, door) {
            return false;
        }
        crate::wires::set_bolts(world, door, false);
        crate::wires::set_electrified(world, door, None);
    }
    with_door(world, door, |d, appearance, ctx| {
        let moved = d.state == DoorState::Closed && ctx.open(d, appearance, false);
        if airlock {
            d.safe = true;
        }
        ((), moved)
    });
    true
}

/// tg `try_to_activate_door(user)` from outside the door systems (a shock that did not happen
/// lets the touch through): `mob`'s tile and ID, read now.
pub fn try_activate(world: &mut World, door: Entity, mob: Entity, bumped: bool) {
    let Some(&mob_pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let access: Option<Vec<String>> = world
        .get::<Inventory>(mob)
        .and_then(|i| i.id_card(world))
        .and_then(|card| world.get::<IdCard>(card))
        .map(|card| card.access.clone());
    with_door(world, door, |door, appearance, ctx| {
        (
            (),
            ctx.activate(door, appearance, bumped, (mob_pos, access.as_deref())),
        )
    });
}

/// A closed airlock for unit tests.
#[cfg(test)]
pub(crate) fn test_airlock(req_access: &[&str]) -> Door {
    Door {
        kind: DoorKind::Airlock,
        state: DoorState::Closed,
        locked: false,
        welded: false,
        req_access: req_access.iter().map(|s| (*s).to_owned()).collect(),
        req_one_access: vec![],
        unres_sides: 0,
        autoclose: true,
        normalspeed: true,
        safe: true,
        can_open_with_hands: true,
        opaque_when_closed: true,
        open_layer: 2.7,
        closing_layer: 3.1,
        close_at: None,
        denied_until: None,
        sprites: default(),
        filler: None,
        panel_open: false,
        feedback: true,
        main_outage: 0.0,
        backup_outage: 0.0,
        ai_control_disabled: 0,
        shock_ready_at: 0.0,
        unres_latch: false,
        emergency: false,
        red_alert_access: false,
        red_alert: false,
        ai_disabled_id_scanner: false,
        sounds: default(),
        sound: None,
        cycle_pump: None,
        cycle_requested: false,
        emagged: false,
        emagging: false,
        crush: false,
        password_lock: false,
        seal: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn airlock(req_access: &[&str]) -> Door {
        test_airlock(req_access)
    }

    fn list(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn paramedic_jaws_refuse_secure_doors_and_alert_on_restricted_ones() {
        let paramedic = "/obj/item/crowbar/power/paramedic";
        let jaws = "/obj/item/crowbar/power";
        let brig = airlock(&["brig"]);
        assert!(!jaws_may_force(paramedic, &brig));
        assert!(jaws_may_force(jaws, &brig), "the plain jaws open anything");
        let one_of = Door {
            req_one_access: list(&["medical", "command"]),
            ..airlock(&[])
        };
        assert!(!jaws_may_force(paramedic, &one_of));
        assert!(jaws_may_force(paramedic, &airlock(&["medical"])));
        // `ignored_access` and doors without access stay quiet.
        assert!(jaws_alert_needed(&airlock(&["medical"])));
        assert!(!jaws_alert_needed(&airlock(&["maint_tunnels"])));
        assert!(!jaws_alert_needed(&airlock(&[
            "medical",
            "external airlocks"
        ])));
        assert!(!jaws_alert_needed(&airlock(&[])));
    }

    #[test]
    fn forced_airlocks_shutters_and_firelocks_sound_like_tg() {
        use crate::sound::PlaySound;
        use ss13_mapload::sound::files;
        let mut app = App::new();
        app.add_message::<PlaySound>()
            .init_resource::<SpatialIndex>()
            .init_resource::<Time>()
            .add_systems(Update, door_sounds);
        let pos = TilePos::new(1, 1, 1);
        let world = app.world_mut();
        let look = Appearance {
            icon_state: IconStateId(0),
            layer: Layer(3.0),
            plane: -1,
            color: None,
            pixel_offset: [0, 0],
            rotation: 0,
            mouse_opacity: default(),
            overlays: vec![],
        };
        let airlock = (test_airlock(&[]), look, pos, Unpowered);
        let airlock = world.spawn(airlock).id();
        let shutter = Door {
            kind: DoorKind::Poddoor,
            sounds: Arc::new(DoorSounds {
                animation: "sound/machines/shutter.ogg".into(),
                ..default()
            }),
            sound: Some(DoorSound::Open),
            ..test_airlock(&[])
        };
        world.spawn((shutter, pos));
        let firelock = Door {
            kind: DoorKind::Firedoor,
            sound: Some(DoorSound::Close),
            ..test_airlock(&[])
        };
        world.spawn((firelock, pos));
        // A crowbar opens an unpowered airlock at once (`BYPASS_DOOR_CHECKS`).
        assert!(crowbar_act(world, Entity::PLACEHOLDER, airlock));
        assert!(world.get::<Door>(airlock).unwrap().is_operating());
        app.update();
        let mut played: Vec<(String, f32)> = app
            .world_mut()
            .resource_mut::<Messages<PlaySound>>()
            .drain()
            .map(|s| (s.sound, s.vol))
            .collect();
        played.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(
            played,
            vec![
                (files::AIRLOCK_FORCED.to_owned(), 30.0),
                ("sound/machines/shutter.ogg".to_owned(), 50.0),
            ]
        );
    }

    #[test]
    fn welding_needs_a_still_closed_door_in_the_state_the_job_saw() {
        let mut door = airlock(&[]);
        assert!(toggle_weld(&mut door, false));
        assert!(door.welded);
        // A second job that also started unwelded finds it welded.
        assert!(!toggle_weld(&mut door, false));
        assert!(toggle_weld(&mut door, true));
        assert!(!door.welded);
        door.state = DoorState::Opening { since: 0.0 };
        assert!(!toggle_weld(&mut door, false));
        door.state = DoorState::Open;
        assert!(!toggle_weld(&mut door, false));
        // A welded door does not open; an unsafe close arms the crush.
        let mut door = airlock(&[]);
        door.welded = true;
        let mut look = Appearance {
            icon_state: IconStateId(0),
            layer: Layer(3.0),
            plane: -1,
            color: None,
            pixel_offset: [0, 0],
            rotation: 0,
            mouse_opacity: default(),
            overlays: vec![],
        };
        let env = DoorEnv {
            powered: true,
            wires: None,
        };
        assert!(!open(&mut door, &mut look, 0.0, false, env, false));
        // A sealed one neither, even forced by the open wire.
        let mut door = airlock(&[]);
        door.seal = Some(Entity::PLACEHOLDER);
        assert!(!open(&mut door, &mut look, 0.0, false, env, true));
        let mut door = airlock(&[]);
        door.state = DoorState::Open;
        door.safe = false;
        let index = SpatialIndex::default();
        let here = (TilePos::new(1, 1, 1), Entity::PLACEHOLDER, &index);
        assert!(close(&mut door, &mut look, here, 0.0, false, env, false));
        assert!(door.crush);
    }

    #[test]
    fn helpers_any_all_unres() {
        let pos = TilePos::new(5, 5, 1);
        let any = DmAirlockHelper {
            access_any: list(&["command", "maint_tunnels"]),
            ..default()
        };
        let all = DmAirlockHelper {
            access_all: list(&["security", "brig"]),
            ..default()
        };

        // any: one of them opens it.
        let mut door = airlock(&[]);
        apply_helper(&mut door, &any, 2, pos);
        assert_eq!(door.req_one_access, list(&["command", "maint_tunnels"]));
        assert!(door.allows(Some(&list(&["maint_tunnels"]))));
        assert!(!door.allows(Some(&list(&["brig"]))));
        assert!(!door.allows(None));

        // all: every one is needed; two helpers stack.
        let mut door = airlock(&[]);
        apply_helper(&mut door, &all, 2, pos);
        apply_helper(
            &mut door,
            &DmAirlockHelper {
                access_all: list(&["armory"]),
                ..default()
            },
            2,
            pos,
        );
        assert_eq!(door.req_access, list(&["security", "brig", "armory"]));
        assert!(!door.allows(Some(&list(&["security", "brig"]))));
        assert!(door.allows(Some(&list(&["security", "brig", "armory"]))));

        // tg refuses to mix them: the second kind is dropped, whichever came first.
        let mut door = airlock(&[]);
        apply_helper(&mut door, &all, 2, pos);
        apply_helper(&mut door, &any, 2, pos);
        assert!(door.req_one_access.is_empty());
        let mut door = airlock(&["xenobiology"]);
        apply_helper(&mut door, &any, 2, pos);
        assert!(door.req_one_access.is_empty());

        // unres: the helper's dir side (north) opens without access, other sides do not;
        // a second helper on the same side toggles it back (`^=`).
        let mut door = airlock(&["brig"]);
        let unres = DmAirlockHelper {
            unres: true,
            ..default()
        };
        apply_helper(&mut door, &unres, Dir::North as u8, pos);
        assert!(door.unrestricted_from(pos, TilePos::new(5, 6, 1)));
        assert!(door.unrestricted_from(pos, TilePos::new(4, 6, 1)));
        assert!(!door.unrestricted_from(pos, TilePos::new(5, 4, 1)));
        apply_helper(&mut door, &unres, Dir::North as u8, pos);
        assert_eq!(door.unres_sides, 0);

        // bolts and welds.
        let mut door = airlock(&[]);
        let seal = DmAirlockHelper {
            locked: true,
            welded: true,
            ..default()
        };
        apply_helper(&mut door, &seal, 2, pos);
        assert!(door.locked && door.welded);
    }
}
