//! Brig cell timers (server) after tg `/obj/machinery/status_display/door_timer`
//! (`brigdoors.dm`, BandaStation's limits): a click with an empty hand on a powered timer opens
//! `BrigTimer`. Every act needs the timer's access (`allowed(usr)`, `ACCESS_SECURITY`).
//!
//! At map load a timer links the brig windoors (`/obj/machinery/door/window/brigdoor`) and
//! security airlocks with its `id` within 20 tiles, and the flashers and brig lockers
//! (`secure_closet/brig`) with it (`urange(20)`). Starting closes the open doors and closes and
//! locks the lockers (unless broken), stopping or running out opens the closed doors and
//! unlocks the closed lockers; running out also announces the release on the security channel. The timer is at most 30 minutes, with
//! BandaStation's 5/10/15 minute presets.
//!
//! Cost: a pass over the running timers per tick (a handful per map).
// shortcut: the display's text is not drawn (no maptext; examine reads it); no AAS config
// of the announcement (tg's default line), no investigate logs, no silicon access. tg breaks a
// timer that links only lockers (`atom_break` when no doors and no flashers); not ported, the
// lockers still follow it.

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::construct::DmObj;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::wallmount::{DOOR_TIMER_ROOT, FLASHER_ROOT};
use ss13_mapload::{DmDoor, DmName, DmWallMount};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::*;
use crate::click::{ClickResolved, ResolveClicks};
use crate::doors::with_door;
use crate::inventory::{IdCard, Inventory, check_access};
use crate::power::Unpowered;
use crate::radio::MachineRadio;
use crate::wallmount::controls::{Flasher, flash};

/// BandaStation `MAX_TIMER`, `PRESET_SHORT`, `PRESET_MEDIUM`, `PRESET_LONG`, seconds.
pub const MAX_TIMER: f64 = 30.0 * 60.0;
pub const SHORT_SECS: f64 = 5.0 * 60.0;
pub const MEDIUM_SECS: f64 = 10.0 * 60.0;
pub const LONG_SECS: f64 = 15.0 * 60.0;
/// `urange(20, src)` of `Initialize`.
const LINK_RANGE: u32 = 20;
const BRIGDOOR: &str = "/obj/machinery/door/window/brigdoor";
const SECURITY_AIRLOCK: &str = "/obj/machinery/door/airlock/security";
const BRIG_CLOSET: &str = "/obj/structure/closet/secure_closet/brig";

/// A door timer: its links, access and countdown.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct DoorTimer {
    pub id: Option<String>,
    pub req_access: Vec<String>,
    pub req_one_access: Vec<String>,
    pub doors: Vec<Entity>,
    pub flashers: Vec<Entity>,
    /// `closets`: the brig lockers with the timer's `id`.
    pub closets: Vec<Entity>,
    /// `activation_time`; `None` is tg's 0.
    pub activation_time: Option<f64>,
    /// `timer_duration`, seconds.
    pub duration: f64,
    pub timing: bool,
}

impl DoorTimer {
    /// `time_left(seconds = TRUE)`.
    pub fn time_left(&self, now: f64) -> f64 {
        (self.duration + self.activation_time.map_or(0.0, |at| at - now)).max(0.0)
    }
}

pub struct DoorTimerPlugin;

impl Plugin for DoorTimerPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<BrigTimerUi>()
            .add_systems(PostStartup, link_timers)
            .add_systems(
                Update,
                (
                    open_timer.after(ResolveClicks).before(UiSystems::Open),
                    timer_acts.in_set(UiSystems::Handle),
                    run_timers.after(crate::power::PowerSystems),
                ),
            );
    }
}

/// `door_timer/Initialize`: the doors, flashers and brig lockers with the timer's `id` within
/// 20 tiles.
fn link_timers(world: &mut World) {
    let mut q = world.query::<(Entity, &DmWallMount, &TilePos)>();
    let mut timers = Vec::new();
    let mut flashers = Vec::new();
    for (e, dm, &pos) in q.iter(world) {
        if is_under(&dm.path, DOOR_TIMER_ROOT) {
            timers.push((e, dm.clone(), pos));
        } else if is_under(&dm.path, FLASHER_ROOT) && !dm.id.is_empty() {
            flashers.push((e, dm.id.clone(), pos));
        }
    }
    if timers.is_empty() {
        return;
    }
    let mut q = world.query::<(Entity, &DmDoor, &DmObj, &TilePos)>();
    let doors: Vec<(Entity, String, TilePos)> = q
        .iter(world)
        .filter(|(_, d, o, _)| {
            !d.id.is_empty() && (is_under(&o.path, BRIGDOOR) || is_under(&o.path, SECURITY_AIRLOCK))
        })
        .map(|(e, d, _, &p)| (e, d.id.clone(), p))
        .collect();
    let mut q = world.query::<(Entity, &crate::closets::Closet, &DmObj, &TilePos)>();
    let closets: Vec<(Entity, String, TilePos)> = q
        .iter(world)
        .filter(|(_, _, o, _)| is_under(&o.path, BRIG_CLOSET))
        .filter_map(|(e, c, _, &p)| Some((e, c.id.clone()?, p)))
        .collect();
    let near = |list: &[(Entity, String, TilePos)], id: &str, at: TilePos| -> Vec<Entity> {
        list.iter()
            .filter(|(_, i, p)| i == id && at.chebyshev(*p).is_some_and(|d| d <= LINK_RANGE))
            .map(|(e, ..)| *e)
            .collect()
    };
    for (e, dm, pos) in timers {
        let id = (!dm.id.is_empty()).then(|| dm.id.clone());
        let (linked_doors, linked_flashers, linked_closets) = match id.as_deref() {
            Some(id) => (
                near(&doors, id, pos),
                near(&flashers, id, pos),
                near(&closets, id, pos),
            ),
            None => (Vec::new(), Vec::new(), Vec::new()),
        };
        world.entity_mut(e).insert(DoorTimer {
            id,
            req_access: dm.req_access.clone(),
            req_one_access: dm.req_one_access.clone(),
            doors: linked_doors,
            flashers: linked_flashers,
            closets: linked_closets,
            ..default()
        });
    }
}

/// `attack_hand` -> `ui_interact` of a powered timer, with an empty hand.
fn open_timer(
    mut clicks: MessageReader<ClickResolved>,
    timers: Query<&DmName, (With<DoorTimer>, Without<Unpowered>)>,
    inventories: Query<&Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok(name) = timers.get(click.target) else {
            continue;
        };
        if inventories
            .get(click.mob)
            .is_ok_and(|i| i.active_item().is_some())
        {
            continue;
        }
        let title = crate::vending::plain_name(&name.name).to_owned();
        open.write(OpenUi::new::<BrigTimerUi>(
            click.client,
            click.mob,
            click.target,
            title,
        ));
    }
}

impl ServerUi for BrigTimerUi {
    type Source = &'static DoorTimer;
    type UserParam = (
        Res<'static, Time>,
        Query<'static, 'static, &'static Flasher>,
    );
    type Act = TimerAct;

    fn static_data(_: &DoorTimer) -> BrigTimerPresets {
        BrigTimerPresets {
            short: (SHORT_SECS / 60.0) as u32,
            medium: (MEDIUM_SECS / 60.0) as u32,
            long: (LONG_SECS / 60.0) as u32,
        }
    }

    fn data(
        timer: &DoorTimer,
        _: Entity,
        (time, flashers): &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> BrigTimerData {
        let now = time.elapsed_secs_f64();
        let left = timer.time_left(now).floor() as u32;
        BrigTimerData {
            timing: timer.timing,
            minutes: left / 60,
            seconds: left % 60,
            // `COOLDOWN_FINISHED(flasher, flash_cooldown)`.
            flash_charging: timer
                .flashers
                .iter()
                .filter_map(|&f| flashers.get(f).ok())
                .any(|f| now < f.ready_at),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<TimerAct, UiReject> {
        Ok(match action {
            TIME => TimerAct::Adjust(
                params
                    .get("adjust")
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| UiReject::BadParams("adjust: expected a number".into()))?,
            ),
            START => TimerAct::Start,
            STOP => TimerAct::Stop,
            FLASH => TimerAct::Flash,
            PRESET => TimerAct::Preset(params.get("preset").unwrap_or_default().to_owned()),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

/// A parsed `BrigTimer` act.
#[derive(Clone, Debug, PartialEq)]
pub enum TimerAct {
    /// `time` by `adjust` deciseconds.
    Adjust(f64),
    Start,
    Stop,
    Flash,
    Preset(String),
}

fn timer_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<BrigTimerUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, TimerAct)> = acts
        .read(world.resource::<Messages<UiActed<BrigTimerUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, timer, act) in pending {
        if let Err(why) = apply(world, client, actor, timer, act.clone()) {
            debug!("BrigTimer {act:?} on {timer} refused: {why}");
        }
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

/// The access of the ID `user` holds or wears, read now.
fn access_of(world: &World, user: Entity) -> Vec<String> {
    world
        .get::<Inventory>(user)
        .and_then(|i| i.id_card(world))
        .and_then(|c| world.get::<IdCard>(c))
        .map(|c| c.access.clone())
        .unwrap_or_default()
}

/// `door_timer/ui_act` at the moment of effect.
fn apply(
    world: &mut World,
    client: Entity,
    user: Entity,
    timer: Entity,
    act: TimerAct,
) -> Result<(), &'static str> {
    let state = world
        .get::<DoorTimer>(timer)
        .cloned()
        .ok_or("not a timer")?;
    if world.get::<Unpowered>(timer).is_some() {
        return Err("unpowered");
    }
    if !check_access(
        &state.req_access,
        &state.req_one_access,
        &access_of(world, user),
    ) {
        crate::tools::tell(world, client, Span::Warning, "Доступ запрещен.");
        return Err("no access");
    }
    let t = now(world);
    match act {
        // `if(value)`: zero adjusts nothing.
        TimerAct::Adjust(v) if v != 0.0 => set_timer(world, timer, state.duration + v / 10.0),
        TimerAct::Adjust(_) => {}
        TimerAct::Start => {
            timer_start(world, timer);
        }
        TimerAct::Stop => {
            timer_end(world, timer, true);
        }
        TimerAct::Flash => {
            for f in state.flashers {
                flash(world, f);
            }
        }
        TimerAct::Preset(preset) => {
            let secs = match preset.as_str() {
                PRESET_SHORT => SHORT_SECS,
                PRESET_MEDIUM => MEDIUM_SECS,
                PRESET_LONG => LONG_SECS,
                _ => state.time_left(t),
            };
            set_timer(world, timer, secs);
            if let Some(mut live) = world.get_mut::<DoorTimer>(timer)
                && live.timing
            {
                live.activation_time = Some(t);
            }
        }
    }
    Ok(())
}

/// `set_timer(value)`: clamped to `MAX_TIMER`; does not start counting.
pub fn set_timer(world: &mut World, timer: Entity, secs: f64) {
    if let Some(mut t) = world.get_mut::<DoorTimer>(timer) {
        t.duration = secs.clamp(0.0, MAX_TIMER);
    }
}

/// `timer_start()`: counting from now, the open doors close, the lockers lock. False without
/// power.
pub fn timer_start(world: &mut World, timer: Entity) -> bool {
    if world.get::<Unpowered>(timer).is_some() {
        return false;
    }
    let t = now(world);
    let (doors, closets) = {
        let Some(mut state) = world.get_mut::<DoorTimer>(timer) else {
            return false;
        };
        state.activation_time = Some(t);
        state.timing = true;
        (state.doors.clone(), state.closets.clone())
    };
    for door in doors {
        with_door(world, door, |d, appearance, ctx| {
            let moved = ctx.close(d, appearance, false);
            ((), moved)
        });
    }
    for closet in closets {
        crate::closets::timer_lock(world, closet);
    }
    true
}

/// `timer_end(forced)`: the timer resets, the closed doors open, the closed lockers unlock; not
/// `forced` (it ran out), the release is announced on the security channel. False without
/// power.
pub fn timer_end(world: &mut World, timer: Entity, forced: bool) -> bool {
    if world.get::<Unpowered>(timer).is_some() {
        return false;
    }
    if !forced {
        announce_release(world, timer);
    }
    let (doors, closets) = {
        let Some(mut state) = world.get_mut::<DoorTimer>(timer) else {
            return false;
        };
        state.timing = false;
        state.activation_time = None;
        state.duration = 0.0;
        (state.doors.clone(), state.closets.clone())
    };
    for door in doors {
        with_door(world, door, |d, appearance, ctx| {
            let moved = ctx.open(d, appearance, false);
            ((), moved)
        });
    }
    for closet in closets {
        crate::closets::timer_unlock(world, closet);
    }
    true
}

fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<DmName>(e)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default()
}

/// `aas_config_announce(/datum/aas_config_entry/brig_cell_release_announcement)`.
fn announce_release(world: &mut World, timer: Entity) {
    let Some(&pos) = world.get::<TilePos>(timer) else {
        return;
    };
    let name = name_of(world, timer);
    world.write_message(MachineRadio {
        freq: ss13_chat::radio::FREQ_SECURITY,
        z: pos.z,
        text: format!("Timer for {name} has expired. Releasing prisoner."),
        name,
    });
}

/// `process()`: a powered running timer past its duration ends.
// shortcut: tg kills processing while unpowered and never restarts it; here the countdown
// resumes with the power (the intended behaviour).
fn run_timers(world: &mut World) {
    let t = now(world);
    let mut q = world.query_filtered::<(Entity, &DoorTimer), Without<Unpowered>>();
    let done: Vec<Entity> = q
        .iter(world)
        .filter(|(_, d)| d.timing && d.activation_time.is_some_and(|at| t - at >= d.duration))
        .map(|(e, _)| e)
        .collect();
    for timer in done {
        timer_end(world, timer, false);
    }
}

/// `grey_tide`: a timer in a struck area stops (`timer_end(forced = TRUE)`).
pub fn grey_tide(world: &mut World, timers: Vec<Entity>) {
    for timer in timers {
        timer_end(world, timer, true);
    }
}

/// `update_content()` read on examine: the name and `mm:ss` while time is left.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(timer) = world.get::<DoorTimer>(target) else {
        return;
    };
    let left = timer.time_left(now(world)) as u32;
    if left == 0 || world.get::<Unpowered>(target).is_some() {
        return;
    }
    lines.push((Span::Plain, "The display says:".to_owned()));
    lines.push((Span::Plain, format!("\t{}", name_of(world, target))));
    lines.push((
        Span::Plain,
        format!("\t{:02}:{:02}", (left / 60) % 60, left % 60),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_left_counts_down_from_activation() {
        let mut t = DoorTimer {
            duration: 300.0,
            ..default()
        };
        assert_eq!(t.time_left(50.0), 300.0);
        t.activation_time = Some(100.0);
        assert_eq!(t.time_left(160.0), 240.0);
        assert_eq!(t.time_left(500.0), 0.0);
    }
}
