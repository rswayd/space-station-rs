//! Bluespace launchpads and their console (server): tg `/obj/machinery/launchpad` and
//! `/obj/machinery/computer/launchpad` (tgui `LaunchpadConsole`).
//!
//! - A pad aims at a tile offset from itself by X/Y within its `range` (10 × the servo tiers).
//!   A multitool saves a pad with its panel open; on the console it adds the saved pad (four at
//!   most). The console selects a pad, moves or sets its offset, renames it, unlinks it (after
//!   an "Are you sure?") and launches (everything on the pad goes to the target) or pulls
//!   (everything on the target comes to the pad).
//! - A launch is refused when the pad is unpowered, broken, open, busy, or in the supply or
//!   transport shuttle. It flashes, sparks at the target and 3.5 s later moves every unanchored
//!   movable (not ghosts, nor a mob buckled to something anchored) through
//!   [`crate::teleport::do_teleport`] (bluespace channel; only the first one sparks), with the
//!   emitter sound at both ends and the pad's active power used.
// shortcut: no briefcase launchpad and remote (`/obj/machinery/launchpad/briefcase`,
// `/obj/item/launchpad_remote`: a syndicate kit with its own deploy/close flow), no diagnostic
// HUD target marker, no beam visual, no ghost click to the target, no USB circuit component.
// A pad that stops working while charging is free again (tg leaves `teleporting` set).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::DmPower;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_tgui::server::{ServerUi, UiActed, UiParams, UiReject};
use ss13_world::SpatialIndex;

use super::server::{is_obj, open, operational, use_active_energy};
use super::{
    ACT_LAUNCH, ACT_MOVE_POS, ACT_PULL, ACT_REMOVE, ACT_RENAME, ACT_SELECT_PAD, ACT_SET_POS,
    LaunchpadConsoleData, LaunchpadConsoleUi, LaunchpadEntry, SelectedPad,
};
use crate::construction::{panel_open, path_of};
use crate::examine::Line;
use crate::obj_damage::is_under;
use crate::stock_parts::PartKind;
use crate::teleport::{Channel, Tele, do_teleport};
use crate::tools::{now, tell};

pub const PAD: &str = "/obj/machinery/launchpad";
pub const BRIEFCASE_PAD: &str = "/obj/machinery/launchpad/briefcase";
pub const CONSOLE: &str = "/obj/machinery/computer/launchpad";
/// `initial(range)`, `teleport_speed` (seconds), `maximum_pads`.
const BASE_RANGE: i32 = 10;
const TELEPORT_SPEED: f64 = 3.5;
const MAX_PADS: usize = 4;
/// `MAX_NAME_LEN`.
const MAX_NAME_LEN: usize = 42;
const FLASH: &str = "sound/items/weapons/flash.ogg";
const EMITTER: &str = "sound/items/weapons/emitter2.ogg";
const SHIELDED_AREAS: [&str; 2] = ["/area/shuttle/supply", "/area/shuttle/transport"];

/// A pad's live state (absent: never used).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Launchpad {
    pub display_name: String,
    pub x: i32,
    pub y: i32,
    /// `teleporting`: when it fires and whether it sends.
    pub charging: Option<(f64, bool)>,
}

impl Default for Launchpad {
    fn default() -> Self {
        Self {
            display_name: "Launchpad".to_owned(),
            x: 0,
            y: 0,
            charging: None,
        }
    }
}

/// A console's `launchpads` and `selected_id` (1-based).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct PadConsole {
    pub pads: Vec<Entity>,
    pub selected: Option<u32>,
}

/// What the console's window shows.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct LaunchpadView(pub LaunchpadConsoleData);

/// The unlink question of `console` about `pad`.
#[derive(Component, Clone, Copy, Debug)]
struct RemovePrompt {
    console: Entity,
    pad: Entity,
}

fn is_pad(world: &World, e: Entity) -> bool {
    is_obj(world, e, PAD) && !is_obj(world, e, BRIEFCASE_PAD)
}

pub fn pad_state(world: &World, pad: Entity) -> Launchpad {
    world.get::<Launchpad>(pad).cloned().unwrap_or_default()
}

fn console_state(world: &World, console: Entity) -> PadConsole {
    world
        .get::<PadConsole>(console)
        .cloned()
        .unwrap_or_default()
}

/// `RefreshParts`: `range` = 10 × the servo tiers.
pub fn range(world: &World, pad: Entity) -> i32 {
    BASE_RANGE * crate::stock_parts::ratings(world, pad).sum(PartKind::Servo) as i32
}

/// `is_available()`.
fn is_available(world: &World, pad: Entity) -> bool {
    world.get_entity(pad).is_ok() && operational(world, pad) && !panel_open(world, pad)
}

/// `set_offset(x, y)`: clamped to the range; not while charging.
pub fn set_offset(world: &mut World, pad: Entity, x: Option<i32>, y: Option<i32>) {
    let mut s = pad_state(world, pad);
    if s.charging.is_some() {
        return;
    }
    let r = range(world, pad);
    if let Some(x) = x {
        s.x = x.clamp(-r, r);
    }
    if let Some(y) = y {
        s.y = y.clamp(-r, r);
    }
    world.entity_mut(pad).insert(s);
}

/// `teleport_checks()`: why the pad cannot fire, if it cannot.
pub fn teleport_checks(world: &World, pad: Entity) -> Option<&'static str> {
    if !is_available(world, pad) {
        return Some(
            "ERROR: Launchpad not operative. Make sure the launchpad is ready and powered.",
        );
    }
    if pad_state(world, pad).charging.is_some() {
        return Some("ERROR: Launchpad busy.");
    }
    let shielded = world
        .get::<TilePos>(pad)
        .and_then(|&p| world.get_resource::<ss13_mapload::Areas>()?.area_at(p))
        .and_then(|a| world.get::<DmPower>(a))
        .is_some_and(|a| SHIELDED_AREAS.iter().any(|r| is_under(&a.path, r)));
    if shielded {
        return Some(
            "ERROR: Launchpad not operative. Heavy area shielding makes teleporting impossible.",
        );
    }
    None
}

fn target_of(world: &World, pad: Entity) -> Option<TilePos> {
    let &pos = world.get::<TilePos>(pad)?;
    let s = pad_state(world, pad);
    let target = TilePos::new(pos.x + s.x, pos.y + s.y, pos.z);
    world
        .resource::<SpatialIndex>()
        .in_bounds(target)
        .then_some(target)
}

/// `doteleport(user, sending)`: the charge-up.
pub fn doteleport(world: &mut World, pad: Entity, sending: bool) {
    let (Some(&here), Some(target)) = (world.get::<TilePos>(pad), target_of(world, pad)) else {
        return;
    };
    crate::animation::flick(world, pad, "lpad-beam");
    crate::sound::playsound(world, crate::sound::PlaySound::new(here, FLASH, 25.0, true));
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(target, FLASH, 25.0, true),
    );
    crate::effects::particles::start(
        world,
        crate::effects::particles::Kind::QuantumSparks,
        5,
        true,
        target,
    );
    let mut s = pad_state(world, pad);
    s.charging = Some((now(world) + TELEPORT_SPEED, sending));
    world.entity_mut(pad).insert(s);
}

pub(super) fn finish_teleports(world: &mut World) {
    let t = now(world);
    let due: Vec<(Entity, bool)> = world
        .query::<(Entity, &Launchpad)>()
        .iter(world)
        .filter_map(|(e, p)| p.charging.filter(|c| c.0 <= t).map(|c| (e, c.1)))
        .collect();
    for (pad, sending) in due {
        let mut s = pad_state(world, pad);
        s.charging = None;
        world.entity_mut(pad).insert(s);
        if !is_available(world, pad) {
            continue;
        }
        let (Some(&here), Some(target)) = (world.get::<TilePos>(pad), target_of(world, pad)) else {
            continue;
        };
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(target, EMITTER, 25.0, true),
        );
        use_active_energy(world, pad);
        let (source, dest) = if sending {
            (here, target)
        } else {
            (target, here)
        };
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(here, EMITTER, 25.0, true),
        );
        let movers: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(source).to_vec();
        let mut first = true;
        for e in movers {
            if e == pad || world.get_entity(e).is_err() || !teleportable(world, e) {
                continue;
            }
            let tele = Tele {
                channel: Channel::Bluespace,
                no_effects: !first,
                ..default()
            };
            do_teleport(world, e, dest, tele);
            first = false;
        }
    }
}

/// A movable that goes: an object or a living mob (no ghosts, turfs or bare visuals), not
/// anchored, a mob not buckled to something anchored.
fn teleportable(world: &World, e: Entity) -> bool {
    if world.get::<crate::ghost::Ghost>(e).is_some() || world.get::<ss13_mapload::Turf>(e).is_some()
    {
        return false;
    }
    let mob = world.get::<crate::health::Health>(e).is_some();
    if !mob && path_of(world, e).is_none() {
        return false;
    }
    if crate::obj_damage::anchored(world, e) {
        return false;
    }
    !world
        .get::<crate::medical::surgery::Buckled>(e)
        .is_some_and(|b| crate::obj_damage::anchored(world, b.0))
}

/// `multitool_act` of a pad (panel open: saved) or the console (the saved pad added).
pub fn multitool_act(world: &mut World, client: Entity, tool: Entity, target: Entity) -> bool {
    if is_pad(world, target) {
        if !panel_open(world, target) {
            return false;
        }
        crate::tools::set_multitool_buffer(world, tool, target);
        tell(world, client, Span::Notice, "saved to buffer");
        return true;
    }
    if !is_obj(world, target, CONSOLE) {
        return false;
    }
    let Some(pad) = crate::tools::multitool_buffer(world, tool).filter(|&b| is_pad(world, b))
    else {
        return false;
    };
    let mut c = console_state(world, target);
    if c.pads.len() >= MAX_PADS {
        return false;
    }
    if !c.pads.contains(&pad) {
        c.pads.push(pad);
    }
    world.entity_mut(target).insert(c);
    world
        .entity_mut(tool)
        .remove::<crate::tools::MultitoolBuffer>();
    let name = crate::examine::name_of(world, tool);
    tell(
        world,
        client,
        Span::Notice,
        format!("You upload the data from the {name} buffer."),
    );
    true
}

/// The console's pads that still exist (`pad_exists`).
fn live_pads(world: &World, console: Entity) -> Vec<Entity> {
    console_state(world, console)
        .pads
        .into_iter()
        .filter(|&p| is_pad(world, p))
        .collect()
}

fn selected_pad(world: &World, console: Entity) -> Option<Entity> {
    let c = console_state(world, console);
    let i = c.selected?.checked_sub(1)? as usize;
    c.pads.get(i).copied().filter(|&p| is_pad(world, p))
}

/// `ui_data` (the gone pads dropped).
pub(super) fn view(world: &mut World, console: Entity) -> Option<LaunchpadView> {
    if !is_obj(world, console, CONSOLE) {
        return None;
    }
    let pads = live_pads(world, console);
    let mut c = console_state(world, console);
    if c.pads != pads {
        c.pads = pads.clone();
        world.entity_mut(console).insert(c.clone());
    }
    let launchpads = pads
        .iter()
        .enumerate()
        .map(|(i, &p)| LaunchpadEntry {
            id: i as u32 + 1,
            name: pad_state(world, p).display_name,
            inactive: world.get::<crate::power::Unpowered>(p).is_some(),
        })
        .collect();
    let selected = selected_pad(world, console).map(|p| {
        let s = pad_state(world, p);
        SelectedPad {
            x: s.x,
            y: s.y,
            pad_name: s.display_name,
            range: range(world, p),
            pad_active: world.get::<crate::power::Unpowered>(p).is_none(),
        }
    });
    Some(LaunchpadView(LaunchpadConsoleData {
        launchpads,
        selected_id: c.selected,
        selected,
    }))
}

/// `attack_hand` of the console: its window. True when the click was its.
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if !is_obj(world, target, CONSOLE) {
        return false;
    }
    if operational(world, target)
        && let Some(v) = view(world, target)
    {
        open::<LaunchpadConsoleUi, _>(world, client, mob, target, v);
    }
    true
}

#[derive(Clone, Debug, PartialEq)]
pub enum LaunchpadAct {
    Select(f64),
    SetPos(Option<f64>, Option<f64>),
    MovePos(f64, f64),
    Rename(String),
    Remove,
    Launch,
    Pull,
}

impl ServerUi for LaunchpadConsoleUi {
    type Source = &'static LaunchpadView;
    type UserParam = ();
    type Act = LaunchpadAct;

    fn static_data(_: &LaunchpadView) {}

    fn data(view: &LaunchpadView, _: Entity, _: &()) -> LaunchpadConsoleData {
        view.0.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<LaunchpadAct, UiReject> {
        // DM `text2num` of a missing param is null (0 in `plus_x || plus_y`).
        let opt = |k: &str| params.number(k).ok();
        Ok(match action {
            ACT_SELECT_PAD => LaunchpadAct::Select(params.number("id")?),
            ACT_SET_POS => LaunchpadAct::SetPos(opt("x"), opt("y")),
            ACT_MOVE_POS => LaunchpadAct::MovePos(opt("x").unwrap_or(0.0), opt("y").unwrap_or(0.0)),
            ACT_RENAME => LaunchpadAct::Rename(params.text("name")?),
            ACT_REMOVE => LaunchpadAct::Remove,
            ACT_LAUNCH => LaunchpadAct::Launch,
            ACT_PULL => LaunchpadAct::Pull,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

/// A whole number of tiles from a client number.
fn tiles(v: f64) -> i32 {
    v.round().clamp(-1000.0, 1000.0) as i32
}

/// `ui_act`, at the moment of effect: the console working, the pad still linked.
pub(super) fn console_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<LaunchpadConsoleUi>>>,
) {
    let pending: Vec<(Entity, Entity, Entity, LaunchpadAct)> = acts
        .read(world.resource::<Messages<UiActed<LaunchpadConsoleUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, console, act) in pending {
        if !is_obj(world, console, CONSOLE) || !operational(world, console) {
            continue;
        }
        if let LaunchpadAct::Select(id) = act {
            let n = live_pads(world, console).len();
            let id = id.round();
            if id >= 1.0 && id <= n as f64 {
                let mut c = console_state(world, console);
                c.selected = Some(id as u32);
                world.entity_mut(console).insert(c);
            }
            continue;
        }
        let Some(pad) = selected_pad(world, console) else {
            continue;
        };
        let sending = act == LaunchpadAct::Launch;
        match act {
            LaunchpadAct::Select(_) => {}
            LaunchpadAct::SetPos(x, y) => set_offset(world, pad, x.map(tiles), y.map(tiles)),
            LaunchpadAct::MovePos(dx, dy) => {
                let s = pad_state(world, pad);
                let (dx, dy) = (tiles(dx), tiles(dy));
                if dx != 0 || dy != 0 {
                    set_offset(world, pad, Some(s.x + dx), Some(s.y + dy));
                } else {
                    set_offset(world, pad, Some(0), Some(0));
                }
            }
            LaunchpadAct::Rename(name) => {
                let name: String = name.trim().chars().take(MAX_NAME_LEN).collect();
                if name.is_empty() {
                    continue;
                }
                let mut s = pad_state(world, pad);
                s.display_name = name;
                world.entity_mut(pad).insert(s);
            }
            LaunchpadAct::Remove => {
                let prompt = ask_world(
                    world,
                    Ask::new(
                        client,
                        actor,
                        "Unlink Launchpad",
                        "Are you sure?",
                        PromptKind::Alert {
                            buttons: vec!["I'm Sure".into(), "Abort".into()],
                            autofocus: true,
                        },
                    ),
                );
                world
                    .entity_mut(prompt)
                    .insert(RemovePrompt { console, pad });
            }
            LaunchpadAct::Launch | LaunchpadAct::Pull => match teleport_checks(world, pad) {
                None => doteleport(world, pad, sending),
                Some(why) => tell(world, client, Span::Warning, why),
            },
        }
    }
}

/// "I'm Sure": the pad unlinked from the console.
pub(super) fn remove_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.answer.clone()))
        .collect();
    for (prompt, answer) in answers {
        let Some(RemovePrompt { console, pad }) = world.get::<RemovePrompt>(prompt).copied() else {
            continue;
        };
        if answer != Some(PromptAnswer::Button(0)) || world.get_entity(console).is_err() {
            continue;
        }
        let mut c = console_state(world, console);
        c.pads.retain(|&p| p != pad);
        c.selected = None;
        world.entity_mut(console).insert(c);
    }
}

/// The pad's range.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<Line>) {
    if !is_pad(world, target) {
        return;
    }
    let r = range(world, target);
    lines.push((
        Span::Notice,
        format!("The status display reads: Maximum range: {r} units."),
    ));
}
