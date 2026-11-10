//! Exosuit tracking beacons and the exosuit control console (server), after tg
//! `mecha_control_console.dm`: `/obj/item/mecha_parts/mecha_tracking` (and its `ai_control`
//! version) and `/obj/machinery/computer/mecha` (tgui `ExosuitControlConsole`).
//!
//! - A beacon clicked onto a mech goes inside it (`try_attach_part`) when the mech takes that
//!   kind (`BEACON_TRACKABLE`, the AI one `BEACON_CONTROLLABLE`) and has none of it yet.
//! - The console (an empty hand or a silicon on a powered one) lists every mech with a beacon:
//!   integrity, charge, cabin pressure, pilot, area, the Ripley's used cargo space.
//!   "Message" asks for a text (`tgui_input_text`) and sends it to the pilot while the window
//!   stays open; "EMP Burst" hits the mech with a heavy EMP (`remote_emp_triggered`), then the
//!   beacon recharges for 5 s.
//! - A mech's EMP destroys each of its beacons with a 40% / severity chance
//!   (`crate::emp::power`).
//!
//! Cost: the console's data is rebuilt once per tick while a window is open: one pass over the
//! mechs.
// shortcut: the AI beacon does not give an AI control of the mech (no AI pilots: see "Ближний
// бой меха и взаимодействие с мобами"); no diagnostic HUD beacon icon; beacons do not show in
// the console's examine.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmName, DmObj};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::UiStatus;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiKind, UiParams, UiReject, UiSystems, UiWindow,
};

use super::core::{
    BEACON_CONTROLLABLE, BEACON_TRACKABLE, IS_ENCLOSED, ItemCell, Mech, MechaDb, name_of,
    tell_occupant, visible,
};
use super::{
    ExoControlData, ExosuitControlConsoleUi, SEND_MESSAGE, SHOCK, TRACKER_REF, TrackedMech,
};
use crate::emp::EMP_HEAVY;
use crate::power::Unpowered;
use crate::tools::{Actor, tell};

pub const TRACKER: &str = "/obj/item/mecha_parts/mecha_tracking";
pub const AI_TRACKER: &str = "/obj/item/mecha_parts/mecha_tracking/ai_control";
pub const CONSOLE: &str = "/obj/machinery/computer/mecha";
const RIPLEY: &str = "/obj/vehicle/sealed/mecha/ripley";
/// `remote_emp_triggered`: `addtimer(recharge, 5 SECONDS)`.
const EMP_RECHARGE_SECS: f64 = 5.0;
/// `MAX_MESSAGE_LEN`.
const MAX_MESSAGE_LEN: u32 = 1024;

/// A beacon attached to a mech (tg `chassis`, `recharging`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Tracker {
    pub chassis: Entity,
    pub ai_beacon: bool,
    /// Game seconds until it may pulse again.
    pub recharged_at: f64,
}

/// The console's window data, rebuilt while a window is open.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ExoConsoleView(pub ExoControlData);

/// A pending "Message" prompt: the beacon and the window that asked.
#[derive(Component, Clone, Copy, Debug)]
struct PendingMessage {
    tracker: Entity,
    window: Entity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleAct {
    SendMessage(u64),
    Shock(u64),
}

pub struct MechControlPlugin;

impl Plugin for MechControlPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `attack_hand` of the exosuit control console: its window.
        app.add_click_handler(ClickPhase::EmptyHand, 2207, |w, c| {
            crate::mecha::control::interact(w, c.client, c.mob, c.target)
        });
        app.register_ui::<ExosuitControlConsoleUi>().add_systems(
            Update,
            (
                window_status.in_set(UiSystems::StatusHook),
                console_acts.in_set(UiSystems::Handle),
                message_answers
                    .in_set(UiSystems::Handle)
                    .after(PromptSystems::Answer),
                refresh_views
                    .before(UiSystems::Send)
                    .after(UiSystems::Handle),
            ),
        );
    }
}

impl ServerUi for ExosuitControlConsoleUi {
    type Source = &'static ExoConsoleView;
    type UserParam = ();
    type Act = ConsoleAct;

    fn static_data(_: &ExoConsoleView) {}

    fn data(view: &ExoConsoleView, _: Entity, _: &()) -> ExoControlData {
        view.0.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ConsoleAct, UiReject> {
        let reference = || {
            params
                .get(TRACKER_REF)
                .and_then(|r| r.trim().parse::<u64>().ok())
                .ok_or_else(|| UiReject::BadParams("tracker_ref: expected an entity".into()))
        };
        match action {
            SEND_MESSAGE => Ok(ConsoleAct::SendMessage(reference()?)),
            SHOCK => Ok(ConsoleAct::Shock(reference()?)),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// A tracking beacon item.
pub fn is_tracker(world: &World, item: Entity) -> bool {
    world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, TRACKER))
}

/// `mecha_tracking/try_attach_part` with a held beacon.
pub fn try_attach(world: &mut World, actor: Actor, item: Entity, mech: Entity) {
    let ai_beacon = world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, AI_TRACKER));
    let flag = if ai_beacon {
        BEACON_CONTROLLABLE
    } else {
        BEACON_TRACKABLE
    };
    let Some(m) = world.get::<Mech>(mech) else {
        return;
    };
    let (thing, mech_name) = (name_of(world, item), name_of(world, mech));
    if m.flags & flag == 0 {
        tell(
            world,
            actor.client,
            Span::Notice,
            format!("{thing} is incompatible with {mech_name}."),
        );
        return;
    }
    let taken = m.trackers.iter().any(|&t| {
        world
            .get::<Tracker>(t)
            .is_some_and(|t| t.ai_beacon == ai_beacon)
    });
    if taken {
        tell(
            world,
            actor.client,
            Span::Notice,
            format!("There already exists a version of {thing} attached to {mech_name}."),
        );
        return;
    }
    // `transferItemToLoc`.
    if let Some(mut inventory) = world.get_mut::<crate::inventory::Inventory>(actor.mob) {
        for hand in &mut inventory.hands {
            if *hand == Some(item) {
                *hand = None;
            }
        }
    }
    crate::storage::put_inside(world, item, mech);
    world.entity_mut(item).insert(Tracker {
        chassis: mech,
        ai_beacon,
        recharged_at: 0.0,
    });
    if let Some(mut m) = world.get_mut::<Mech>(mech) {
        m.trackers.push(item);
    }
    let user = name_of(world, actor.mob);
    if let Some(&pos) = world.get::<TilePos>(mech) {
        visible(
            world,
            pos,
            Span::Notice,
            format!("{user} attaches {thing} to {mech_name}."),
            &[actor.mob],
        );
    }
    tell(
        world,
        actor.client,
        Span::Notice,
        format!("You attach {thing} to {mech_name}."),
    );
}

/// The live beacons of `mech` (deleted ones dropped).
pub fn trackers_of(world: &World, mech: Entity) -> Vec<Entity> {
    world.get::<Mech>(mech).map_or_else(Vec::new, |m| {
        m.trackers
            .iter()
            .copied()
            .filter(|&t| world.get::<Tracker>(t).is_some_and(|t| t.chassis == mech))
            .collect()
    })
}

/// A beacon destroyed (an EMP): out of its mech's list and gone.
pub fn destroy_tracker(world: &mut World, tracker: Entity) {
    if let Some(mech) = world.get::<Tracker>(tracker).map(|t| t.chassis)
        && let Some(mut m) = world.get_mut::<Mech>(mech)
    {
        m.trackers.retain(|&t| t != tracker);
    }
    if world.get_entity(tracker).is_ok() {
        world.despawn(tracker);
    }
}

/// `get_area_name(M, TRUE)`.
fn area_name(world: &World, pos: TilePos) -> Option<String> {
    let name = crate::bots::area_name(world, pos);
    (!name.is_empty()).then_some(name)
}

/// The used cargo space of a Ripley, percent.
fn cargo_space(world: &World, m: &Mech) -> Option<i32> {
    let db = world.resource::<MechaDb>();
    if !is_under(&db.data.mechs[m.def].path, RIPLEY) {
        return None;
    }
    let capacity = m
        .modules
        .iter()
        .filter_map(|&e| world.get::<super::equipment::MechEquip>(e))
        .map(|e| db.data.equipment[e.def].cargo_capacity)
        .max()
        .filter(|&c| c > 0)?;
    Some((m.cargo.len() as f64 / f64::from(capacity) * 100.0).round() as i32)
}

/// `ui_data`: every mech's beacons.
pub fn compute_data(world: &World) -> ExoControlData {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut mechs = Vec::new();
    let mut q = world.try_query::<(Entity, &Mech)>();
    let Some(q) = q.as_mut() else {
        return ExoControlData::default();
    };
    let mut all: Vec<(Entity, &Mech)> = q.iter(world).collect();
    all.sort_by_key(|(e, _)| *e);
    for (mech, m) in all {
        for tracker in trackers_of(world, mech) {
            let Some(t) = world.get::<Tracker>(tracker) else {
                continue;
            };
            let def = &world.resource::<MechaDb>().data.mechs[m.def];
            let cell = m.cell.and_then(|c| world.get::<ItemCell>(c)).map(|c| c.0);
            mechs.push(TrackedMech {
                name: crate::obj_damage::strip(&name_of(world, mech)).to_owned(),
                integrity: (m.integrity / def.max_integrity.max(1.0) * 100.0).round() as i32,
                charge: cell.map(|c| (c.charge / c.maxcharge.max(1.0) * 100.0).round() as i32),
                airtank: (m.flags & IS_ENCLOSED != 0).then_some(super::panel::ONE_ATMOSPHERE),
                pilot: m.occupant.map(|p| name_of(world, p)).into_iter().collect(),
                location: world
                    .get::<TilePos>(mech)
                    .and_then(|&p| area_name(world, p)),
                emp_recharging: now < t.recharged_at,
                tracker_ref: tracker.to_bits(),
                cargo_space: cargo_space(world, m),
            });
        }
    }
    ExoControlData { mechs }
}

fn refresh_views(world: &mut World) {
    let mut q = world.query_filtered::<&UiWindow, With<UiKind<ExosuitControlConsoleUi>>>();
    let mut consoles: Vec<Entity> = q.iter(world).map(|w| w.source).collect();
    if consoles.is_empty() {
        return;
    }
    consoles.sort();
    consoles.dedup();
    let data = compute_data(world);
    for console in consoles {
        let view = ExoConsoleView(data.clone());
        match world.get_mut::<ExoConsoleView>(console) {
            Some(mut old) => {
                old.set_if_neq(view);
            }
            None => {
                world.entity_mut(console).insert(view);
            }
        }
    }
}

/// An unpowered console closes (`is_operational`).
fn window_status(
    mut windows: Query<&mut UiWindow, With<UiKind<ExosuitControlConsoleUi>>>,
    unpowered: Query<(), With<Unpowered>>,
) {
    for mut window in &mut windows {
        if unpowered.contains(window.source) {
            window.cap_status(UiStatus::Close);
        }
    }
}

/// The exosuit control console.
pub fn is_console(world: &World, e: Entity) -> bool {
    world
        .get::<DmObj>(e)
        .is_some_and(|o| is_under(&o.path, CONSOLE))
}

/// tg `attack_hand` -> `ui_interact`. True when the click was the console's.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if !is_console(world, target) {
        return false;
    }
    open(world, client, mob, target);
    true
}

/// `ui_interact` of a powered console (also for silicons).
pub fn open(world: &mut World, client: Entity, user: Entity, console: Entity) {
    if !is_console(world, console) || world.get::<Unpowered>(console).is_some() {
        return;
    }
    let view = ExoConsoleView(compute_data(world));
    world.entity_mut(console).insert(view);
    let title = world.get::<DmName>(console).map_or_else(String::new, |n| {
        crate::vending::plain_name(&n.name).to_owned()
    });
    world.write_message(OpenUi::new::<ExosuitControlConsoleUi>(
        client, user, console, title,
    ));
}

/// The beacon a `tracker_ref` names (`locate(params["tracker_ref"])` + `istype`).
fn tracker_of(world: &World, bits: u64) -> Option<Entity> {
    Entity::try_from_bits(bits).filter(|&e| world.get::<Tracker>(e).is_some())
}

fn console_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<ExosuitControlConsoleUi>>>,
) {
    let pending: Vec<(Entity, Entity, Entity, ConsoleAct)> = acts
        .read(world.resource::<Messages<UiActed<ExosuitControlConsoleUi>>>())
        .map(|a| (a.window, a.client, a.actor, a.act))
        .collect();
    for (window, client, actor, act) in pending {
        match act {
            ConsoleAct::SendMessage(bits) => {
                let Some(tracker) = tracker_of(world, bits) else {
                    continue;
                };
                let prompt = ask_world(
                    world,
                    Ask::new(
                        client,
                        actor,
                        "Transmit message",
                        "Input message",
                        PromptKind::Text {
                            default: String::new(),
                            max_length: MAX_MESSAGE_LEN,
                            multiline: false,
                        },
                    ),
                );
                world
                    .entity_mut(prompt)
                    .insert(PendingMessage { tracker, window });
            }
            ConsoleAct::Shock(bits) => {
                let Some(tracker) = tracker_of(world, bits) else {
                    continue;
                };
                remote_emp(world, actor, tracker);
            }
        }
    }
}

/// `remote_emp_triggered`: a heavy EMP on the mech unless the beacon recharges.
pub fn remote_emp(world: &mut World, user: Entity, tracker: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(t) = world.get::<Tracker>(tracker).copied() else {
        return;
    };
    if now < t.recharged_at || world.get::<Mech>(t.chassis).is_none() {
        return;
    }
    let pilot = world
        .get::<Mech>(t.chassis)
        .and_then(|m| m.occupant)
        .map(|p| name_of(world, p));
    info!(
        "{user:?} ({}) activated remote EMP on exosuit {} at {:?}, {}",
        name_of(world, user),
        name_of(world, t.chassis),
        world.get::<TilePos>(t.chassis),
        pilot.map_or_else(
            || "without a pilot".to_owned(),
            |p| format!("occupied by {p}")
        ),
    );
    crate::emp::emp_act(world, t.chassis, EMP_HEAVY);
    if let Some(mut t) = world.get_mut::<Tracker>(tracker) {
        t.recharged_at = now + EMP_RECHARGE_SECS;
    }
}

/// The message, sent if the window is still open and interactive and the text is not blank.
fn message_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.answer.clone()))
        .collect();
    for (prompt, client, answer) in answers {
        let Some(&PendingMessage { tracker, window }) = world.get::<PendingMessage>(prompt) else {
            continue;
        };
        let Some(PromptAnswer::Text(text)) = answer else {
            continue;
        };
        let open = world
            .get::<UiWindow>(window)
            .is_some_and(|w| w.status() == UiStatus::Interactive);
        let text = text.trim().to_owned();
        if !open || text.is_empty() {
            continue;
        }
        let Some(mech) = world.get::<Tracker>(tracker).map(|t| t.chassis) else {
            continue;
        };
        if world.get::<Mech>(mech).is_none() {
            continue;
        }
        tell_occupant(world, mech, Span::Plain, text);
        tell(world, client, Span::Notice, "Message sent.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_mapload::mecha::{MechDef, MechaData};

    #[test]
    fn console_lists_each_beacon_with_tg_numbers() {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        let db = MechaDb::new(MechaData {
            mechs: vec![MechDef {
                path: RIPLEY.to_owned(),
                name: "APLU MK-I \"Ripley\"".to_owned(),
                max_integrity: 200.0,
                mecha_flags: IS_ENCLOSED | BEACON_TRACKABLE,
                ..default()
            }],
            ..default()
        });
        let mut m = Mech::new(0, &db);
        m.integrity = 150.0;
        world.insert_resource(db);
        let mech = world.spawn(()).id();
        let tracker = world
            .spawn(Tracker {
                chassis: mech,
                ai_beacon: false,
                recharged_at: 5.0,
            })
            .id();
        // A beacon that belongs to another mech is not this one's.
        let stray = world
            .spawn(Tracker {
                chassis: Entity::PLACEHOLDER,
                ai_beacon: true,
                recharged_at: 0.0,
            })
            .id();
        m.trackers.extend([tracker, stray]);
        world.entity_mut(mech).insert(m);
        let data = compute_data(&world);
        assert_eq!(data.mechs.len(), 1);
        let entry = &data.mechs[0];
        assert_eq!(entry.integrity, 75);
        assert_eq!(entry.charge, None);
        assert_eq!(entry.airtank, Some(super::super::panel::ONE_ATMOSPHERE));
        assert!(entry.pilot.is_empty());
        assert!(entry.emp_recharging);
        assert_eq!(entry.tracker_ref, tracker.to_bits());
        // A Ripley without a cargo compartment has no cargo line.
        assert_eq!(entry.cargo_space, None);
    }

    #[test]
    fn beacon_flags_follow_tg_defines() {
        assert_eq!(BEACON_TRACKABLE, 1 << 14);
        assert_eq!(BEACON_CONTROLLABLE, 1 << 15);
        // A combat mech's `mecha_flags` from the dump (52324) takes both beacons.
        assert_ne!(52324 & BEACON_TRACKABLE, 0);
        assert_ne!(52324 & BEACON_CONTROLLABLE, 0);
    }
}
