//! Solar power (server): tg `code/modules/power/solar.dm`, `tracker.dm` and `SSsun`
//! (`code/controllers/subsystem/sun.dm`, with BandaStation's cached `sun_x`/`sun_y`).
//!
//! - [`Sun`] moves `azimuth_mod * 6°` every minute from a random start. A move flags every
//!   panel to recompute its exposure, steps timed controllers and points auto-tracking
//!   controllers through their tracker.
//! - A [`SolarPanel`] turns to its target on the next machine tick; only a turn re-checks
//!   whether the station blocks the sun ([`is_sunlight_blocked`]: 20 steps towards the sun hit
//!   an opaque tile). Exposure is Lambert's cosine between the panel and the sun; output is
//!   `2500 W * exposure * power_tier` into the panel's powernet.
//! - A [`SolarControl`] links the panels and one tracker on its powernet when "refresh" is
//!   pressed (tg `search_for_connected`; nothing links on its own at roundstart, so map panels
//!   stay facing north until someone sets the array up), and shows the `SolarControl` window.
//!
//! Cost: panels run in the machine tick (0.5 Hz), O(panels); MetaStation has 141. A turn costs
//! up to 20 tile lookups per panel, once per minute at most.

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use ss13_atmos::gas::byond_round;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmName, Opaque};
use ss13_shared::TilePos;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::solar_control::{SolarControlData, SolarControlUi};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_world::SpatialIndex;

use super::{Grid, MACHINE_TICK_SECS, Unpowered};
use crate::combat::CombatRng;

pub const SOLAR_ROOT: &str = "/obj/machinery/power/solar";
pub const SOLAR_FAKE: &str = "/obj/machinery/power/solar/fake";
pub const TRACKER_ROOT: &str = "/obj/machinery/power/tracker";
pub const SOLAR_CONTROL_ROOT: &str = "/obj/machinery/power/solar_control";

/// `SOLAR_GEN_RATE`, watts at full exposure and `power_tier` 1.
pub const SOLAR_GEN_RATE: f64 = 2500.0;
/// `SSsun.wait`.
pub const SUN_WAIT: f64 = 60.0;
/// `SSsun.base_rotation`, degrees per fire.
pub const BASE_ROTATION: f64 = 6.0;
/// `is_sunlight_blocked(occlusion_distance = 20)`.
pub const OCCLUSION_DISTANCE: u32 = 20;

/// `SOLAR_TRACK_OFF`, `SOLAR_TRACK_TIMED`, `SOLAR_TRACK_AUTO`.
pub const TRACK_OFF: u8 = 0;
pub const TRACK_TIMED: u8 = 1;
pub const TRACK_AUTO: u8 = 2;

/// tg `SSsun`.
#[derive(Resource, Clone, Debug)]
pub struct Sun {
    /// Clockwise from north, `[0, 360)`.
    pub azimuth: f64,
    /// `-2..-0.5` or `0.5..2`.
    pub azimuth_mod: f64,
    /// BandaStation's cached `round(sin(azimuth), 0.01)` and `round(cos(azimuth), 0.01)`.
    pub sun_x: f64,
    pub sun_y: f64,
    /// Seconds since the last fire.
    since: f64,
}

impl Sun {
    pub fn new(azimuth: f64, azimuth_mod: f64) -> Self {
        let mut sun = Self {
            azimuth,
            azimuth_mod,
            sun_x: 0.0,
            sun_y: 1.0,
            since: 0.0,
        };
        sun.update_direction();
        sun
    }

    /// `SSsun.Initialize`: random azimuth, 50-200% speed, either way round.
    fn random(rng: &mut CombatRng) -> Self {
        let azimuth = f64::from(rng.range(0, 359));
        let mut speed = byond_round(f64::from(rng.range(50, 200)) / 100.0, 0.01);
        if rng.chance(50.0) {
            speed = -speed;
        }
        Self::new(azimuth, speed)
    }

    /// `SSsun.fire`.
    pub fn advance(&mut self) {
        self.azimuth = byond_round(self.azimuth + self.azimuth_mod * BASE_ROTATION, 0.01);
        if self.azimuth >= 360.0 {
            self.azimuth -= 360.0;
        }
        if self.azimuth < 0.0 {
            self.azimuth += 360.0;
        }
        self.update_direction();
    }

    fn update_direction(&mut self) {
        let rad = self.azimuth.to_radians();
        self.sun_x = byond_round(rad.sin(), 0.01);
        self.sun_y = byond_round(rad.cos(), 0.01);
    }
}

/// tg `/obj/machinery/power/solar`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct SolarPanel {
    pub azimuth_current: f64,
    pub azimuth_target: f64,
    pub needs_to_turn: bool,
    pub needs_to_update_solar_exposure: bool,
    pub obscured: bool,
    /// `[0, 1]` exposure.
    pub sunfrac: f64,
    pub control: Option<Entity>,
    /// Glass tier (1 glass .. 4 plastitanium glass).
    pub power_tier: f64,
}

impl Default for SolarPanel {
    fn default() -> Self {
        Self {
            azimuth_current: 0.0,
            azimuth_target: 0.0,
            needs_to_turn: true,
            needs_to_update_solar_exposure: true,
            obscured: false,
            sunfrac: 0.0,
            control: None,
            power_tier: 1.0,
        }
    }
}

impl SolarPanel {
    /// `update_solar_exposure`.
    pub fn update_solar_exposure(&mut self, sun_azimuth: f64) -> f64 {
        self.needs_to_update_solar_exposure = false;
        self.sunfrac = 0.0;
        if self.obscured {
            return 0.0;
        }
        self.sunfrac = if self.azimuth_current == sun_azimuth {
            1.0
        } else {
            byond_round(
                (self.azimuth_current - sun_azimuth).to_radians().cos(),
                0.01,
            )
            .clamp(0.0, 1.0)
        };
        self.sunfrac
    }

    /// `queue_turn`.
    fn queue_turn(&mut self, azimuth: f64) {
        self.needs_to_turn = true;
        self.azimuth_target = azimuth;
    }
}

/// tg `/obj/machinery/power/tracker`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct SolarTracker {
    pub control: Option<Entity>,
    pub azimuth_current: f64,
}

/// tg `/obj/machinery/power/solar_control`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct SolarControl {
    /// Watts generated by its panels this tick, and last tick (what the window shows).
    pub generated: f64,
    pub lastgen: f64,
    pub azimuth_target: f64,
    /// Degrees per sun move while timed.
    pub azimuth_rate: f64,
    pub track: u8,
    pub tracker: Option<Entity>,
    pub panels: Vec<Entity>,
    /// Watts with every panel at full exposure.
    pub total_capacity: f64,
    pub history_supply: Vec<f64>,
    pub history_capacity: Vec<f64>,
    /// Records kept: one full sun cycle; 0 until the first record.
    pub record_size: usize,
    /// Seconds.
    pub next_record: f64,
}

impl Default for SolarControl {
    fn default() -> Self {
        Self {
            generated: 0.0,
            lastgen: 0.0,
            azimuth_target: 0.0,
            azimuth_rate: BASE_ROTATION,
            track: TRACK_OFF,
            tracker: None,
            panels: Vec::new(),
            total_capacity: 0.0,
            history_supply: Vec::new(),
            history_capacity: Vec::new(),
            record_size: 0,
            next_record: 0.0,
        }
    }
}

/// `set_panels` azimuth: rounded, clamped, wrapped into `[0, 360)`.
pub fn normalize_azimuth(azimuth: f64) -> f64 {
    let mut azimuth = byond_round(azimuth, 0.01).clamp(-360.0, 719.99);
    if azimuth >= 360.0 {
        azimuth -= 360.0;
    }
    if azimuth < 0.0 {
        azimuth += 360.0;
    }
    azimuth
}

/// The map's opacity, for [`is_sunlight_blocked`].
#[derive(SystemParam)]
pub struct Occlusion<'w, 's> {
    index: Res<'w, SpatialIndex>,
    opaque: Query<'w, 's, (), With<Opaque>>,
}

impl Occlusion<'_, '_> {
    fn opaque(&self, pos: TilePos) -> bool {
        self.index
            .on_tile(pos)
            .iter()
            .any(|&e| self.opaque.contains(e))
    }
}

/// tg `/turf/proc/is_sunlight_blocked`: steps of (`sun_x`, `sun_y`) from the turf, rounded to
/// tiles, until an opaque tile (blocked) or the map edge (not blocked).
pub fn is_sunlight_blocked(
    pos: TilePos,
    sun: &Sun,
    size: (u32, u32),
    opaque: impl Fn(TilePos) -> bool,
) -> bool {
    let (max_x, max_y) = (size.0 as i32, size.1 as i32);
    let (mut x, mut y) = (f64::from(pos.x), f64::from(pos.y));
    for _ in 0..OCCLUSION_DISTANCE {
        x += sun.sun_x;
        y += sun.sun_y;
        // DM `round(x, 1)`.
        let hit = TilePos::new((x + 0.5).floor() as i32, (y + 0.5).floor() as i32, pos.z);
        if hit.x < 1 || hit.y < 1 || hit.x > max_x || hit.y > max_y {
            // shortcut: tg `locate()` returns null off the map and the proc runtimes; the edge
            // check below stops it first on any map with a margin.
            break;
        }
        if opaque(hit) {
            return true;
        }
        if hit.x == 1 || hit.x == max_x || hit.y == 1 || hit.y == max_y {
            break;
        }
    }
    false
}

/// What the machine tick needs for solars.
#[derive(SystemParam)]
pub struct Solars<'w, 's> {
    sun: Res<'w, Sun>,
    panels: Query<'w, 's, (Entity, &'static TilePos, &'static mut SolarPanel)>,
    controls: Query<'w, 's, &'static mut SolarControl>,
    trackers: Query<'w, 's, (Entity, &'static mut SolarTracker)>,
    occlusion: Occlusion<'w, 's>,
    time: Res<'w, Time>,
}

impl Solars<'_, '_> {
    /// Panels' `process()`, then the controllers' (`lastgen`, history, tracker check).
    pub fn process(&mut self, grid: &mut Grid) {
        let dt = MACHINE_TICK_SECS;
        let sun = self.sun.clone();
        for (e, &pos, mut panel) in &mut self.panels {
            let net = grid.node_net(e);
            if let Some(control) = panel.control {
                let control_net = self.controls.get(control).ok().and(grid.node_net(control));
                if net.is_none() || control_net != net {
                    unset_control(e, &mut panel, &mut self.controls);
                }
            }
            if panel.needs_to_turn {
                panel.needs_to_turn = false;
                if panel.azimuth_current != panel.azimuth_target {
                    panel.azimuth_current = panel.azimuth_target;
                    let size = self.occlusion.index.level_size(pos.z).unwrap_or((0, 0));
                    panel.obscured =
                        is_sunlight_blocked(pos, &sun, size, |p| self.occlusion.opaque(p));
                    panel.needs_to_update_solar_exposure = true;
                }
            }
            if panel.needs_to_update_solar_exposure {
                panel.update_solar_exposure(sun.azimuth);
            }
            if panel.sunfrac <= 0.0 {
                continue;
            }
            let sgen = SOLAR_GEN_RATE * panel.sunfrac * panel.power_tier;
            if let Some(net) = net.and_then(|n| grid.net_mut(n)) {
                net.newavail += sgen * dt;
            }
            if let Some(mut control) = panel.control.and_then(|c| self.controls.get_mut(c).ok()) {
                control.generated += sgen;
            }
        }
        let now = self.time.elapsed_secs_f64();
        for mut control in &mut self.controls {
            control.lastgen = control.generated;
            control.generated = 0.0;
            record(&mut control, &sun, now);
        }
        for (t, mut tracker) in &mut self.trackers {
            let Some(control) = tracker.control else {
                continue;
            };
            let net = grid.node_net(control);
            if net.is_none() || net != grid.node_net(t) {
                unset_tracker(&mut tracker, &mut self.controls);
            }
        }
    }
}

/// `unset_control` of a panel.
fn unset_control(
    panel_entity: Entity,
    panel: &mut SolarPanel,
    controls: &mut Query<&mut SolarControl>,
) {
    if let Some(mut control) = panel.control.take().and_then(|c| controls.get_mut(c).ok()) {
        control.panels.retain(|&p| p != panel_entity);
        control.total_capacity -= SOLAR_GEN_RATE * panel.power_tier;
    }
}

/// `unset_control` of a tracker: auto tracking stops.
fn unset_tracker(tracker: &mut SolarTracker, controls: &mut Query<&mut SolarControl>) {
    if let Some(mut control) = tracker
        .control
        .take()
        .and_then(|c| controls.get_mut(c).ok())
    {
        if control.track == TRACK_AUTO {
            control.track = TRACK_OFF;
        }
        control.tracker = None;
    }
}

/// `record`: one supply and capacity sample per `record_interval` (the sun's minute), a full
/// sun cycle kept.
fn record(control: &mut SolarControl, sun: &Sun, now: f64) {
    if control.record_size == 0 {
        control.record_size = 1
            + (360.0f64 / (control.azimuth_rate * sun.azimuth_mod.abs()))
                .ceil()
                .max(0.0) as usize;
    }
    if now < control.next_record {
        return;
    }
    control.next_record = now + SUN_WAIT;
    // tg appends only with a powernet; the controls here are all on one from map load.
    control.history_supply.push(control.lastgen.round());
    if control.history_supply.len() > control.record_size {
        control.history_supply.remove(0);
    }
    control.history_capacity.push(control.total_capacity);
    if control.history_capacity.len() > control.record_size {
        control.history_capacity.remove(0);
    }
}

/// `set_panels`.
fn set_panels(
    control: &mut SolarControl,
    azimuth: f64,
    panels: &mut Query<(Entity, &TilePos, &mut SolarPanel)>,
) {
    let azimuth = normalize_azimuth(azimuth);
    control.azimuth_target = azimuth;
    for &p in &control.panels {
        if let Ok((_, _, mut panel)) = panels.get_mut(p) {
            panel.queue_turn(azimuth);
        }
    }
}

pub struct SolarServerPlugin;

impl Plugin for SolarServerPlugin {
    fn build(&self, app: &mut App) {
        let mut rng = CombatRng::default();
        app.insert_resource(Sun::random(&mut rng))
            .register_ui::<SolarControlWindow>()
            .add_systems(Startup, init_solars.after(super::init_power))
            .add_systems(
                Update,
                (
                    sun_tick.before(super::PowerSystems),
                    open_on_click
                        .after(ss13_world::WorldSystems::Movement)
                        .after(crate::click::ResolveClicks)
                        .before(UiSystems::Open),
                    apply_acts.in_set(UiSystems::Handle),
                    unpowered_windows.in_set(UiSystems::StatusHook),
                ),
            );
    }
}

/// Panels, trackers and controls from map load (`Initialize`: `connect_to_network`, the
/// controller's `set_panels(azimuth_target)` with no panels yet).
fn init_solars(world: &mut World) {
    let mut query = world.query::<(Entity, &super::DmPower)>();
    let found: Vec<(Entity, String, u8)> = query
        .iter(world)
        .map(|(e, dm)| (e, dm.path.clone(), super::layer(dm)))
        .collect();
    let mut panels = Vec::new();
    let mut trackers = Vec::new();
    let mut controls = Vec::new();
    for (e, path, layer) in found {
        if is_under(&path, SOLAR_FAKE) {
            continue;
        }
        if is_under(&path, SOLAR_ROOT) {
            panels.push((e, layer));
        } else if is_under(&path, TRACKER_ROOT) {
            trackers.push((e, layer));
        } else if is_under(&path, SOLAR_CONTROL_ROOT) {
            controls.push((e, layer));
        }
    }
    info!(
        "solars: {} panels, {} trackers, {} controls",
        panels.len(),
        trackers.len(),
        controls.len()
    );
    world.resource_scope(|world, mut grid: Mut<Grid>| {
        for &(e, layer) in panels.iter().chain(&trackers).chain(&controls) {
            if grid.node_net(e).is_some() {
                continue;
            }
            if let Some(&pos) = world.get::<TilePos>(e) {
                grid.add_node(e, pos, layer, super::NodeKind::Machine);
            }
        }
    });
    world.insert_batch(panels.into_iter().map(|(e, _)| (e, SolarPanel::default())));
    world.insert_batch(
        trackers
            .into_iter()
            .map(|(e, _)| (e, SolarTracker::default())),
    );
    world.insert_batch(
        controls
            .into_iter()
            .map(|(e, _)| (e, SolarControl::default())),
    );
}

/// `SSsun.fire` every minute and the `COMSIG_SUN_MOVED` handlers.
fn sun_tick(
    time: Res<Time>,
    mut sun: ResMut<Sun>,
    mut panels: Query<(Entity, &TilePos, &mut SolarPanel)>,
    mut controls: Query<&mut SolarControl>,
    mut trackers: Query<&mut SolarTracker>,
) {
    sun.since += time.delta_secs_f64();
    if sun.since < SUN_WAIT {
        return;
    }
    sun.since -= SUN_WAIT;
    sun.advance();
    let azimuth = sun.azimuth;
    for (_, _, mut panel) in &mut panels {
        panel.needs_to_update_solar_exposure = true;
    }
    for mut control in &mut controls {
        if control.track == TRACK_TIMED {
            let target = control.azimuth_target + control.azimuth_rate;
            set_panels(&mut control, target, &mut panels);
        }
    }
    for mut tracker in &mut trackers {
        tracker.azimuth_current = azimuth;
        if let Some(mut control) = tracker.control.and_then(|c| controls.get_mut(c).ok())
            && control.track == TRACK_AUTO
        {
            set_panels(&mut control, azimuth, &mut panels);
        }
    }
}

/// The server side of the `SolarControl` interface.
pub struct SolarControlWindow;

impl UiInterface for SolarControlWindow {
    const NAME: &'static str = SolarControlUi::NAME;
    type Data = SolarControlData;
    type StaticData = ();
}

/// A parsed `ui_act` of the solar control.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SolarAct {
    /// `azimuth` with `adjust` (relative, wins when nonzero) or `value`.
    Azimuth {
        adjust: Option<f64>,
        value: Option<f64>,
    },
    AzimuthRate {
        adjust: Option<f64>,
        value: Option<f64>,
    },
    Tracking(u8),
    Refresh,
}

/// DM `text2num(params[key])`: absent or not a number is null.
fn num(params: &UiParams, key: &str) -> Option<f64> {
    params
        .get(key)
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

impl ServerUi for SolarControlWindow {
    type Source = &'static SolarControl;
    type UserParam = ();
    type Act = SolarAct;

    fn static_data(_: &SolarControl) {}

    fn data(control: &SolarControl, _: Entity, _: &()) -> SolarControlData {
        SolarControlData {
            supply: control.lastgen.round() as f32,
            capacity: control.total_capacity as f32,
            azimuth_current: control.azimuth_target as f32,
            azimuth_rate: control.azimuth_rate as f32,
            max_rotation_rate: (BASE_ROTATION * 2.0) as f32,
            tracking_state: control.track,
            connected_panels: control.panels.len() as u32,
            connected_tracker: control.tracker.is_some(),
            history_supply: control.history_supply.iter().map(|&v| v as f32).collect(),
            history_capacity: control.history_capacity.iter().map(|&v| v as f32).collect(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<SolarAct, UiReject> {
        let adjust = num(params, "adjust");
        let value = num(params, "value");
        match action {
            "azimuth" => Ok(SolarAct::Azimuth { adjust, value }),
            "azimuth_rate" => Ok(SolarAct::AzimuthRate { adjust, value }),
            // tg stores any number in `track`; only the three modes mean anything.
            "tracking" => match num(params, "mode") {
                Some(m) if m == 0.0 || m == 1.0 || m == 2.0 => Ok(SolarAct::Tracking(m as u8)),
                _ => Err(UiReject::BadParams("mode: expected 0, 1 or 2".into())),
            },
            "refresh" => Ok(SolarAct::Refresh),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// A click with an empty hand on a powered controller opens the window (tg `attack_hand` ->
/// `ui_interact`).
fn open_on_click(
    mut clicks: MessageReader<crate::click::ClickResolved>,
    controls: Query<&DmName, (With<SolarControl>, Without<Unpowered>)>,
    inventories: Query<&crate::inventory::Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok(name) = controls.get(click.target) else {
            continue;
        };
        if inventories
            .get(click.mob)
            .is_ok_and(|i| i.active_item().is_none())
        {
            open.write(OpenUi::new::<SolarControlWindow>(
                click.client,
                click.mob,
                click.target,
                crate::vending::plain_name(&name.name),
            ));
        }
    }
}

/// `ui_interact` for a silicon (`crate::silicon::interact`): only while powered.
pub fn open(world: &mut World, client: Entity, actor: Entity, control: Entity) {
    if world.get::<Unpowered>(control).is_some() {
        return;
    }
    let title = world.get::<DmName>(control).map_or_else(String::new, |n| {
        crate::vending::plain_name(&n.name).to_owned()
    });
    world.write_message(OpenUi::new::<SolarControlWindow>(
        client, actor, control, title,
    ));
}

/// tg `/atom/ui_status`: an unpowered computer is update-only.
fn unpowered_windows(
    mut windows: Query<&mut UiWindow>,
    unpowered: Query<(), (With<SolarControl>, With<Unpowered>)>,
) {
    for mut window in &mut windows {
        if window.interface == SolarControlWindow::NAME && unpowered.contains(window.source) {
            window.cap_status(ss13_tgui::UiStatus::Update);
        }
    }
}

/// `ui_act`, one exclusive pass: "refresh" walks the powernet's machines.
fn apply_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<SolarControlWindow>>>) {
    let pending: Vec<(Entity, SolarAct)> = acts
        .read(world.resource::<Messages<UiActed<SolarControlWindow>>>())
        .map(|a| (a.source, a.act))
        .collect();
    for (source, act) in pending {
        apply_act(world, source, act);
    }
}

/// The effect of one act on controller `source`, checked now.
pub fn apply_act(world: &mut World, source: Entity, act: SolarAct) {
    let Some(control) = world.get::<SolarControl>(source).cloned() else {
        return;
    };
    let mut state = bevy::ecs::system::SystemState::<(
        Query<&mut SolarControl>,
        Query<(Entity, &TilePos, &mut SolarPanel)>,
        Query<(Entity, &mut SolarTracker)>,
        Res<Grid>,
        Res<Sun>,
    )>::new(world);
    let Ok((mut controls, mut panels, mut trackers, grid, sun)) = state.get_mut(world) else {
        return;
    };
    let mut c = controls.get_mut(source).expect("checked above");
    match act {
        SolarAct::Azimuth { adjust, value } => {
            let value = match adjust {
                Some(a) if a != 0.0 => Some(control.azimuth_target + a),
                _ => value,
            };
            if let Some(v) = value {
                set_panels(&mut c, v, &mut panels);
            }
        }
        SolarAct::AzimuthRate { adjust, value } => {
            let value = match adjust {
                Some(a) if a != 0.0 => Some(control.azimuth_rate + a),
                _ => value,
            };
            if let Some(v) = value {
                let max = 2.0 * BASE_ROTATION;
                c.azimuth_rate = byond_round(v.clamp(-max, max), 0.01);
            }
        }
        SolarAct::Tracking(mode) => {
            c.track = mode;
            if mode == TRACK_AUTO {
                match c.tracker {
                    // `connected_tracker.sun_update(SSsun, SSsun.azimuth)`.
                    Some(tracker) => {
                        if let Ok((_, mut t)) = trackers.get_mut(tracker) {
                            t.azimuth_current = sun.azimuth;
                        }
                        set_panels(&mut c, sun.azimuth, &mut panels);
                    }
                    None => c.track = TRACK_OFF,
                }
            }
        }
        SolarAct::Refresh => {
            // `search_for_connected`: unlinked panels and the first unlinked tracker on the
            // controller's powernet.
            // shortcut: space vines do not exist, so none hide a panel.
            if let Some(net) = grid.node_net(source) {
                let azimuth = c.azimuth_target;
                for (p, _, mut panel) in &mut panels {
                    if panel.control.is_none() && grid.node_net(p) == Some(net) {
                        panel.control = Some(source);
                        c.panels.push(p);
                        c.total_capacity += SOLAR_GEN_RATE * panel.power_tier;
                        panel.queue_turn(azimuth);
                    }
                }
                if c.tracker.is_none()
                    && let Some((t, mut tracker)) = trackers
                        .iter_mut()
                        .find(|(t, tr)| tr.control.is_none() && grid.node_net(*t) == Some(net))
                {
                    tracker.control = Some(source);
                    c.tracker = Some(t);
                }
            }
        }
    }
    state.apply(world);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sun_moves_and_wraps() {
        let mut sun = Sun::new(357.0, 1.5);
        assert_eq!((sun.sun_x, sun.sun_y), (-0.05, 1.0));
        sun.advance();
        assert!((sun.azimuth - 6.0).abs() < 1e-9);
        let mut back = Sun::new(2.0, -0.5);
        back.advance();
        assert!((back.azimuth - 359.0).abs() < 1e-9);
        assert_eq!(normalize_azimuth(-10.0), 350.0);
        assert!((normalize_azimuth(725.0) - 359.99).abs() < 1e-9);
        assert_eq!(normalize_azimuth(360.0), 0.0);
    }

    #[test]
    fn exposure_is_lambert_cosine() {
        let mut panel = SolarPanel::default();
        assert_eq!(panel.update_solar_exposure(0.0), 1.0);
        assert_eq!(panel.update_solar_exposure(60.0), 0.5);
        // round(cos(45), 0.01) = 0.71.
        assert_eq!(panel.update_solar_exposure(315.0), 0.71);
        assert_eq!(panel.update_solar_exposure(120.0), 0.0);
        panel.obscured = true;
        assert_eq!(panel.update_solar_exposure(0.0), 0.0);
        // Output: 2500 W * exposure * tier.
        panel.obscured = false;
        panel.power_tier = 3.0;
        let frac = panel.update_solar_exposure(60.0);
        assert_eq!(SOLAR_GEN_RATE * frac * panel.power_tier, 3750.0);
    }

    #[test]
    fn station_blocks_the_sun() {
        // A wall column at x = 15 on a 40x40 level; the panel at (10, 10).
        let wall = |p: TilePos| p.x == 15 && (5..=15).contains(&p.y);
        let pos = TilePos::new(10, 10, 1);
        let size = (40, 40);
        // Sun to the east (90°): the 5th step hits the wall.
        assert!(is_sunlight_blocked(pos, &Sun::new(90.0, 1.0), size, wall));
        // West and north: open space up to the edge.
        assert!(!is_sunlight_blocked(pos, &Sun::new(270.0, 1.0), size, wall));
        assert!(!is_sunlight_blocked(pos, &Sun::new(0.0, 1.0), size, wall));
        // North-east: (10 + 0.71k, 10 + 0.71k) passes x = 15 at y ≈ 15, inside the wall run.
        assert!(is_sunlight_blocked(pos, &Sun::new(45.0, 1.0), size, wall));
        // Beyond 20 steps nothing counts: a wall 25 tiles west.
        let far = |p: TilePos| p.x == 1;
        let pos = TilePos::new(30, 10, 1);
        assert!(!is_sunlight_blocked(
            pos,
            &Sun::new(270.0, 1.0),
            (60, 40),
            far
        ));
    }
}
