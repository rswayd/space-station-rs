//! The nuclear fission explosive (server) after tg `/obj/machinery/nuclearbomb`
//! (`_nuclear_bomb.dm`, `syndicate_nuke.dm`): [`NukeState`] is the keypad and timer state
//! machine of `ui_act`, `update_ui_mode`, `ui_data`, `toggle_nuke_safety`, `toggle_nuke_armed`
//! and `process`; the systems below wire it to clicks, the disk, the `NuclearBomb` window, the
//! sprite and the detonation (`explode` -> `actually_explode` 10 s later).
//!
//! A real disk ([`NukeDisk`](super::server::NukeDisk), not a fake) goes in by clicking the bomb
//! with it or by `eject_disk` while holding it; the code (`r_code`, the team's
//! `memorized_code` for the syndicate bomb) unlocks the timer input (90-3600 s), which takes the
//! safety off; ARM starts the countdown, ARM again (disk in, code entered) stops it. `anchor`
//! needs the disk and the code and refuses in space.
//! Arming sets the delta security level and disarming restores the level it was armed at
//! (`arm_nuke`, `disarm_nuke`).
// shortcut: no deconstruction (nuke core, screwdriver/welder states), countdown effect, sounds,
// pinpointer alert, ghost notification, minimap blip or cinematic; no red alert after an
// off-station detonation (`drop_level`).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::{Areas, DEFAULT_AREA, DmName, DmObj, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::server::{NukeDisk, chat_to};
use super::{ANCHOR, ARM, DIGIT, EJECT_DISK, KEYPAD, NuclearBombData, NuclearBombUi};
use crate::click::{ClickResolved, ResolveClicks};
use crate::inventory::Inventory;
use crate::obj_damage::Anchored;

/// tg `/obj/machinery/nuclearbomb`.
pub const NUKE_ROOT: &str = "/obj/machinery/nuclearbomb";
/// tg `/obj/machinery/nuclearbomb/syndicate`: its code is the operatives' `memorized_code`.
pub const SYNDICATE_NUKE: &str = "/obj/machinery/nuclearbomb/syndicate";
/// The beer nuke (`/obj/machinery/nuclearbomb/beer`) only sprays beer: not modelled.
const BEER_NUKE: &str = "/obj/machinery/nuclearbomb/beer";
const NUKE_DMI: &str = "icons/obj/machines/nuke.dmi";
/// tg `NUKE_CODE_UNSET`: never enterable on the five-digit keypad.
pub const NUKE_CODE_UNSET: &str = "admin";
/// `explode` -> `actually_explode`.
pub const DETONATION_DELAY_SECS: f64 = 10.0;
/// tg `NUKE_RADIUS`.
const NUKE_RADIUS: i32 = 127;

/// tg `NUKEUI_*`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiMode {
    #[default]
    AwaitDisk,
    AwaitCode,
    AwaitTimer,
    AwaitArm,
    Timing,
    Exploded,
}

/// tg `get_nuke_state()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NukeLook {
    OffLocked,
    OffUnlocked,
    Timing,
    Exploding,
}

/// What a keypad press asks the world to do besides changing the state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyEffect {
    None,
    /// The countdown stopped because the safety went on (`disarm_nuke`).
    Disarmed,
}

/// The keypad and timer of one bomb (tg's vars of the same names).
#[derive(Clone, Debug, PartialEq)]
pub struct NukeState {
    pub timer_set: u32,
    pub minimum_timer_set: u32,
    pub maximum_timer_set: u32,
    pub numeric_input: String,
    pub ui_mode: UiMode,
    pub timing: bool,
    pub exploding: bool,
    pub exploded: bool,
    /// Game seconds of the detonation while timing.
    pub detonation_at: Option<f64>,
    pub r_code: String,
    pub yes_code: bool,
    pub safety: bool,
    pub auth: bool,
    pub anchored: bool,
}

impl Default for NukeState {
    fn default() -> Self {
        Self {
            timer_set: 90,
            minimum_timer_set: 90,
            maximum_timer_set: 3600,
            numeric_input: String::new(),
            ui_mode: UiMode::AwaitDisk,
            timing: false,
            exploding: false,
            exploded: false,
            detonation_at: None,
            r_code: NUKE_CODE_UNSET.to_owned(),
            yes_code: false,
            safety: true,
            auth: false,
            anchored: false,
        }
    }
}

impl NukeState {
    /// `update_ui_mode`.
    pub fn update_ui_mode(&mut self) {
        self.ui_mode = if self.exploded {
            UiMode::Exploded
        } else if !self.auth {
            UiMode::AwaitDisk
        } else if self.timing {
            UiMode::Timing
        } else if !self.safety {
            UiMode::AwaitArm
        } else if !self.yes_code {
            UiMode::AwaitCode
        } else {
            UiMode::AwaitTimer
        };
    }

    pub fn look(&self) -> NukeLook {
        if self.exploding {
            NukeLook::Exploding
        } else if self.timing {
            NukeLook::Timing
        } else if self.safety {
            NukeLook::OffLocked
        } else {
            NukeLook::OffUnlocked
        }
    }

    /// `get_time_left`: seconds to the detonation while timing, else `timer_set`.
    pub fn time_left(&self, now: f64) -> u32 {
        match (self.timing, self.detonation_at) {
            (true, Some(at)) => (at - now).max(0.0).round() as u32,
            _ => self.timer_set,
        }
    }

    /// `toggle_nuke_safety`.
    fn toggle_safety(&mut self) -> KeyEffect {
        self.safety = !self.safety;
        let mut effect = KeyEffect::None;
        if self.safety {
            if self.timing {
                self.timing = false;
                effect = KeyEffect::Disarmed;
            }
            self.detonation_at = None;
        }
        effect
    }

    /// `ui_act("keypad")`. `None` when the press is refused (no disk).
    pub fn keypad(&mut self, digit: &str) -> Option<KeyEffect> {
        if !self.auth {
            return None;
        }
        let mut effect = KeyEffect::None;
        match digit {
            "C" => {
                if self.ui_mode == UiMode::AwaitArm {
                    effect = self.toggle_safety();
                    self.yes_code = false;
                    self.update_ui_mode();
                }
                self.numeric_input.clear();
            }
            "E" => {
                match self.ui_mode {
                    UiMode::AwaitCode => {
                        if self.numeric_input == self.r_code {
                            self.numeric_input.clear();
                            self.yes_code = true;
                        } else {
                            self.numeric_input = "ERROR".into();
                        }
                    }
                    UiMode::AwaitTimer => {
                        // `text2num`: a non-number or zero does nothing.
                        if let Some(n) = self.numeric_input.parse::<u32>().ok().filter(|&n| n > 0) {
                            self.timer_set =
                                n.clamp(self.minimum_timer_set, self.maximum_timer_set);
                            effect = self.toggle_safety();
                        }
                    }
                    _ => {}
                }
                self.update_ui_mode();
            }
            d if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() => {
                if self.numeric_input != "ERROR" {
                    self.numeric_input.push_str(d);
                    if self.numeric_input.len() > 5 {
                        self.numeric_input = "ERROR".into();
                    }
                }
            }
            _ => return None,
        }
        Some(effect)
    }

    /// `ui_act("arm")`: `true` when the countdown started or stopped.
    pub fn arm(&mut self, now: f64) -> Option<bool> {
        if !(self.auth && self.yes_code && !self.safety && !self.exploded) {
            return None;
        }
        self.timing = !self.timing;
        if self.timing {
            self.detonation_at = Some(now + f64::from(self.timer_set));
        } else {
            self.detonation_at = None;
        }
        self.update_ui_mode();
        Some(self.timing)
    }

    /// `process`: `true` when the timer ran out and `explode` begins.
    pub fn tick(&mut self, now: f64) -> bool {
        if !self.timing || self.exploding {
            return false;
        }
        if self.detonation_at.is_some_and(|at| at < now) {
            return self.explode();
        }
        false
    }

    /// `explode`: the safety stops it; else it is exploding, safe and without code.
    pub fn explode(&mut self) -> bool {
        if self.safety {
            self.timing = false;
            return false;
        }
        self.exploding = true;
        self.yes_code = false;
        self.safety = true;
        true
    }

    /// `ui_data`.
    pub fn ui_data(&self, now: f64) -> NuclearBombData {
        let hidden = self.ui_mode == UiMode::AwaitCode && self.numeric_input != "ERROR";
        let mut code = if hidden {
            "*".repeat(self.numeric_input.len())
        } else {
            self.numeric_input.clone()
        };
        while code.chars().count() < 5 {
            code.push('-');
        }
        let time = format!("TIME: {}", self.time_left(now));
        let (first, second) = match self.ui_mode {
            UiMode::AwaitDisk => (
                "DEVICE LOCKED",
                if self.timing {
                    time
                } else {
                    "AWAIT DISK".into()
                },
            ),
            UiMode::AwaitCode => ("INPUT CODE", format!("CODE: {code}")),
            UiMode::AwaitTimer => ("INPUT TIME", format!("TIME: {code}")),
            UiMode::AwaitArm => ("DEVICE READY", time),
            UiMode::Timing => ("DEVICE ARMED", time),
            UiMode::Exploded => ("DEVICE DEPLOYED", "THANK YOU".into()),
        };
        NuclearBombData {
            disk_present: self.auth,
            status1: first.into(),
            status2: second,
            anchored: self.anchored,
        }
    }
}

/// tg `DETONATION_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detonation {
    HitStation,
    NearMissedStation,
    HitSyndieBase,
    MissedStation,
}

/// `actually_explode`'s classification of where the bomb went off.
pub fn classify(
    pos: TilePos,
    station: bool,
    in_space: bool,
    on_syndie_base: bool,
    level_size: (u32, u32),
) -> Detonation {
    if station {
        // `128 ± NUKE_RADIUS` on a 255-wide station level: scaled to this level's centre.
        let (cx, cy) = (level_size.0 as i32 / 2 + 1, level_size.1 as i32 / 2 + 1);
        let far = (pos.x - cx).abs() > NUKE_RADIUS || (pos.y - cy).abs() > NUKE_RADIUS;
        if in_space || far {
            Detonation::NearMissedStation
        } else {
            Detonation::HitStation
        }
    } else if on_syndie_base {
        Detonation::HitSyndieBase
    } else {
        Detonation::MissedStation
    }
}

/// A nuclear bomb on the map.
#[derive(Component, Clone, Debug)]
pub struct NuclearBomb {
    pub state: NukeState,
    /// The disk inside (tg `auth`).
    pub disk: Option<Entity>,
    pub syndicate: bool,
    /// Game seconds of `actually_explode` once exploding.
    pub actually_explode_at: Option<f64>,
    /// tg `previous_level`: the security level when it was armed.
    pub previous_level: crate::security_level::SecLevel,
}

pub struct NuclearBombPlugin;

impl Plugin for NuclearBombPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<NuclearBombUi>()
            .add_systems(ss13_world::MapInit, setup_bombs)
            .add_systems(
                Update,
                (
                    clicks.after(ResolveClicks).before(UiSystems::Open),
                    bomb_acts.in_set(UiSystems::Handle),
                    tick_bombs,
                ),
            );
    }
}

fn setup_bombs(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let bombs: Vec<(Entity, bool)> = query
        .iter(world)
        .filter(|(_, o)| {
            ss13_mapload::fidelity::is_under(&o.path, NUKE_ROOT)
                && !ss13_mapload::fidelity::is_under(&o.path, BEER_NUKE)
        })
        .map(|(e, o)| (e, ss13_mapload::fidelity::is_under(&o.path, SYNDICATE_NUKE)))
        .collect();
    for (e, syndicate) in bombs {
        let anchored = world.get::<Anchored>(e).is_some_and(|a| a.0);
        info!(
            "nuclear bomb {e} at {:?}{}",
            world.get::<TilePos>(e),
            if syndicate { " (syndicate)" } else { "" }
        );
        // BandaStation `nuclearbomb/Initialize`: `r_code = random_nukecode()` for every bomb
        // instead of `NUKE_CODE_UNSET`.
        let seed = world
            .get_resource::<ss13_mapload::MapSeed>()
            .map_or(0, |s| s.0);
        let mut rng = ss13_mapload::spawner::Rng::new(seed ^ 0x6e75_6b63 ^ e.to_bits());
        let r_code = super::server::random_nukecode(&mut rng);
        world.entity_mut(e).insert(NuclearBomb {
            state: NukeState {
                anchored,
                r_code,
                ..default()
            },
            disk: None,
            syndicate,
            actually_explode_at: None,
            previous_level: default(),
        });
    }
}

/// An empty hand opens the keypad; a disk in hand goes in (`item_interaction`).
fn clicks(world: &mut World, mut cursor: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = cursor
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(bomb) = world.get::<NuclearBomb>(click.target).cloned() else {
            continue;
        };
        let held = world
            .get::<Inventory>(click.mob)
            .and_then(Inventory::active_item);
        match held {
            None => {
                let name = world
                    .get::<DmName>(click.target)
                    .map(|n| crate::vending::plain_name(&n.name).to_owned())
                    .unwrap_or_default();
                world.write_message(OpenUi::new::<NuclearBombUi>(
                    click.client,
                    click.mob,
                    click.target,
                    name,
                ));
            }
            Some(item) if world.get::<NukeDisk>(item).is_some() || is_fake_disk(world, item) => {
                insert_disk(world, click.client, click.target, item, bomb);
            }
            Some(_) => {}
        }
    }
}

fn is_fake_disk(world: &World, item: Entity) -> bool {
    world
        .get::<ss13_mapload::DmItem>(item)
        .is_some_and(|i| ss13_mapload::fidelity::is_under(&i.path, "/obj/item/disk/nuclear/fake"))
}

/// `disk_check` + `transferItemToLoc`.
fn insert_disk(world: &mut World, client: Entity, bomb_e: Entity, disk: Entity, bomb: NuclearBomb) {
    if bomb.disk.is_some() {
        return;
    }
    if world.get::<NukeDisk>(disk).is_none() {
        chat_to(
            world,
            Some(client),
            Span::Plain,
            "Ядерное устройство говорит: \"Authentication failure; disk not recognised.\"",
        );
        return;
    }
    crate::storage::put_inside(world, disk, bomb_e);
    if let Some(mut b) = world.get_mut::<NuclearBomb>(bomb_e) {
        b.disk = Some(disk);
        b.state.auth = true;
        b.state.update_ui_mode();
    }
}

impl ServerUi for NuclearBombUi {
    type Source = &'static NuclearBomb;
    type UserParam = Res<'static, Time>;
    type Act = BombAct;

    fn static_data(_: &NuclearBomb) {}

    fn data(bomb: &NuclearBomb, _: Entity, time: &Res<Time>) -> NuclearBombData {
        bomb.state.ui_data(time.elapsed_secs_f64())
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<BombAct, UiReject> {
        match action {
            EJECT_DISK => Ok(BombAct::EjectDisk),
            KEYPAD => {
                let digit = params
                    .get(DIGIT)
                    .filter(|d| {
                        matches!(*d, "C" | "E")
                            || (d.len() == 1 && d.as_bytes()[0].is_ascii_digit())
                    })
                    .ok_or_else(|| UiReject::BadParams("digit: 0-9, C or E".into()))?;
                Ok(BombAct::Keypad(digit.to_owned()))
            }
            ARM => Ok(BombAct::Arm),
            ANCHOR => Ok(BombAct::Anchor),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BombAct {
    EjectDisk,
    Keypad(String),
    Arm,
    Anchor,
}

/// tg `ui_act`, at the moment of effect.
fn bomb_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<NuclearBombUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, BombAct)> = acts
        .read(world.resource::<Messages<UiActed<NuclearBombUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for (client, actor, bomb_e, act) in pending {
        let Some(mut bomb) = world.get::<NuclearBomb>(bomb_e).cloned() else {
            continue;
        };
        match act {
            BombAct::EjectDisk => {
                if let Some(disk) = bomb.disk {
                    let pos = world.get::<TilePos>(bomb_e).copied();
                    if let Some(pos) = pos {
                        crate::storage::take_out(world, disk, pos);
                    }
                    bomb.disk = None;
                    bomb.state.auth = false;
                    bomb.state.update_ui_mode();
                    world.entity_mut(bomb_e).insert(bomb);
                } else if let Some(held) = world.get::<Inventory>(actor).and_then(|inv| {
                    inv.hands
                        .iter()
                        .flatten()
                        .copied()
                        .find(|&i| world.get::<NukeDisk>(i).is_some() || is_fake_disk(world, i))
                }) {
                    insert_disk(world, client, bomb_e, held, bomb);
                }
                continue;
            }
            BombAct::Keypad(digit) => {
                if bomb.state.keypad(&digit) == Some(KeyEffect::Disarmed) {
                    info!("{bomb_e} was disarmed by {actor}");
                    crate::security_level::set_level(world, bomb.previous_level, true);
                }
            }
            BombAct::Arm => match bomb.state.arm(now) {
                Some(true) => {
                    info!("{bomb_e} was armed by {actor} ({} s)", bomb.state.timer_set);
                    super::server::bomb_armed(world, bomb_e);
                    bomb.previous_level = crate::security_level::current(world);
                    crate::security_level::set_level(
                        world,
                        crate::security_level::SecLevel::Delta,
                        true,
                    );
                }
                Some(false) => {
                    info!("{bomb_e} was disarmed by {actor}");
                    crate::security_level::set_level(world, bomb.previous_level, true);
                }
                None => {}
            },
            BombAct::Anchor => {
                if bomb.state.auth && bomb.state.yes_code {
                    let in_space = world
                        .get::<TilePos>(bomb_e)
                        .is_some_and(|&p| is_space_area(world, p));
                    if in_space && !bomb.state.anchored {
                        chat_to(world, Some(client), Span::Warning, "Не к чему крепить!");
                    } else {
                        bomb.state.anchored = !bomb.state.anchored;
                        world
                            .entity_mut(bomb_e)
                            .insert(Anchored(bomb.state.anchored));
                    }
                }
            }
        }
        world.entity_mut(bomb_e).insert(bomb);
        update_look(world, bomb_e);
    }
}

/// tg `isinspace()` / `istype(area, /area/space)`: the tile's area is space.
pub(crate) fn is_space_area(world: &World, pos: TilePos) -> bool {
    let Some(area) = world.resource::<Areas>().area_at(pos) else {
        return true;
    };
    area_path(world, area).is_none_or(|p| ss13_mapload::fidelity::is_under(p, DEFAULT_AREA))
}

/// An area's type (areas carry it in [`ss13_mapload::DmPower`]).
pub(crate) fn area_path(world: &World, area: Entity) -> Option<&str> {
    world
        .get::<ss13_mapload::DmPower>(area)
        .map(|p| p.path.as_str())
}

/// `update_icon_state` + `update_overlays`.
fn update_look(world: &mut World, bomb_e: Entity) {
    let Some(look) = world.get::<NuclearBomb>(bomb_e).map(|b| b.state.look()) else {
        return;
    };
    let (state, lights) = match look {
        NukeLook::OffLocked => ("nuclearbomb_base", None),
        NukeLook::OffUnlocked => ("nuclearbomb_base", Some("lights-safety")),
        NukeLook::Timing => ("nuclearbomb_timing", Some("lights-timing")),
        NukeLook::Exploding => ("nuclearbomb_exploding", Some("lights-exploding")),
    };
    let Some(index) = world.get_resource::<IconIndex>() else {
        return;
    };
    let state = index.get(NUKE_DMI, state);
    let lights = lights.and_then(|l| index.get(NUKE_DMI, l));
    if let Some(mut appearance) = world.get_mut::<Appearance>(bomb_e) {
        if let Some(state) = state {
            appearance.icon_state = state;
        }
        appearance.overlays = lights.map(Overlay::new).into_iter().collect();
    }
}

/// `process` and the `actually_explode` timer.
fn tick_bombs(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &NuclearBomb)>();
    let active: Vec<Entity> = query
        .iter(world)
        .filter(|(_, b)| b.state.timing || b.actually_explode_at.is_some())
        .map(|(e, _)| e)
        .collect();
    for e in active {
        let mut bomb = world.get::<NuclearBomb>(e).cloned().expect("queried");
        if let Some(at) = bomb.actually_explode_at {
            if now >= at {
                bomb.actually_explode_at = None;
                bomb.state.exploded = true;
                bomb.state.timing = false;
                bomb.state.update_ui_mode();
                world.entity_mut(e).insert(bomb);
                super::server::actually_explode(world, e);
            }
            continue;
        }
        if bomb.state.tick(now) {
            bomb.actually_explode_at = Some(now + DETONATION_DELAY_SECS);
            info!("{e} is exploding");
            world.entity_mut(e).insert(bomb);
            update_look(world, e);
            super::server::bomb_exploding(world, e);
        } else if !bomb.state.timing {
            world.entity_mut(e).insert(bomb);
            update_look(world, e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn armed_ready() -> NukeState {
        let mut s = NukeState {
            r_code: "01234".into(),
            auth: true,
            ..default()
        };
        s.update_ui_mode();
        s
    }

    #[test]
    fn code_entry_follows_the_keypad() {
        let mut s = NukeState::default();
        // No disk: every key is refused.
        assert_eq!(s.keypad("1"), None);
        assert_eq!(s.ui_data(0.0).status1, "DEVICE LOCKED");
        assert_eq!(s.ui_data(0.0).status2, "AWAIT DISK");
        let mut s = armed_ready();
        assert_eq!(s.ui_mode, UiMode::AwaitCode);
        for d in ["0", "1", "2"] {
            s.keypad(d);
        }
        // The code is hidden while it is typed.
        assert_eq!(s.ui_data(0.0).status2, "CODE: ***--");
        s.keypad("9");
        s.keypad("9");
        s.keypad("E");
        assert_eq!(s.numeric_input, "ERROR");
        assert_eq!(s.ui_data(0.0).status2, "CODE: ERROR");
        // Digits do nothing over an error; C clears it.
        s.keypad("1");
        assert_eq!(s.numeric_input, "ERROR");
        s.keypad("C");
        for d in ["0", "1", "2", "3", "4"] {
            s.keypad(d);
        }
        // A sixth digit is an error.
        let mut long = s.clone();
        long.keypad("5");
        assert_eq!(long.numeric_input, "ERROR");
        s.keypad("E");
        assert!(s.yes_code);
        assert_eq!(s.ui_mode, UiMode::AwaitTimer);
        assert_eq!(s.ui_data(0.0).status1, "INPUT TIME");
        assert!(s.safety);
    }

    #[test]
    fn timer_is_clamped_and_takes_the_safety_off() {
        let mut s = armed_ready();
        for d in ["0", "1", "2", "3", "4", "E"] {
            s.keypad(d);
        }
        // Zero is no number: nothing happens.
        s.keypad("0");
        s.keypad("E");
        assert!(s.safety);
        s.keypad("C");
        // 10 s is below the 90 s minimum.
        s.keypad("1");
        s.keypad("0");
        s.keypad("E");
        assert_eq!(s.timer_set, 90);
        assert!(!s.safety);
        assert_eq!(s.ui_mode, UiMode::AwaitArm);
        assert_eq!(s.ui_data(0.0).status2, "TIME: 90");
        // C in AwaitArm puts the safety back on and forgets the code.
        s.keypad("C");
        assert!(s.safety && !s.yes_code);
        assert_eq!(s.ui_mode, UiMode::AwaitCode);
        let mut big = armed_ready();
        for d in ["0", "1", "2", "3", "4", "E", "9", "9", "9", "9", "9", "E"] {
            big.keypad(d);
        }
        assert_eq!(big.timer_set, 3600);
    }

    #[test]
    fn countdown_detonates_and_arm_disarms() {
        let mut s = armed_ready();
        for d in ["0", "1", "2", "3", "4", "E", "1", "2", "0", "E"] {
            s.keypad(d);
        }
        assert_eq!(s.timer_set, 120);
        // Arming needs the disk.
        let mut no_disk = s.clone();
        no_disk.auth = false;
        assert_eq!(no_disk.arm(0.0), None);
        assert_eq!(s.arm(10.0), Some(true));
        assert_eq!(s.ui_mode, UiMode::Timing);
        assert_eq!(s.time_left(70.0), 60);
        assert_eq!(s.ui_data(70.0).status2, "TIME: 60");
        assert!(!s.tick(100.0));
        // Disarm with ARM again, then re-arm.
        assert_eq!(s.arm(100.0), Some(false));
        assert_eq!(s.ui_mode, UiMode::AwaitArm);
        assert!(!s.tick(1000.0));
        assert_eq!(s.arm(200.0), Some(true));
        assert!(!s.tick(320.0));
        assert!(s.tick(320.5));
        assert!(s.exploding && s.safety && !s.yes_code);
        assert_eq!(s.look(), NukeLook::Exploding);
    }

    #[test]
    fn detonation_is_classified_by_place() {
        let p = TilePos::new(100, 100, 1);
        assert_eq!(
            classify(p, true, false, false, (255, 255)),
            Detonation::HitStation
        );
        assert_eq!(
            classify(p, true, true, false, (255, 255)),
            Detonation::NearMissedStation
        );
        assert_eq!(
            classify(p, false, false, true, (255, 255)),
            Detonation::HitSyndieBase
        );
        assert_eq!(
            classify(p, false, true, false, (255, 255)),
            Detonation::MissedStation
        );
    }
}
