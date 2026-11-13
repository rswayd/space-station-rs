//! The Skillsoft station (server), after tg `/obj/machinery/skill_station`
//! (`code/modules/library/skill_learning/skill_station.dm`), with the `SkillStation` window
//! (`ss13_tgui::interfaces::skill_station`). The door and the occupant are
//! `crate::medical::pods`'; the chips are `crate::skillchips`'.
//!
//! - It starts open. An empty hand opens or closes it (closing shuts in the mob on its tile and
//!   opens the window for it); the occupant's own click, or the closing, opens the window, which
//!   only the occupant can use (`contained_state`). A step from inside opens it.
//! - A skillchip used on it goes in (one at a time; "There's already a skillchip inside.").
//! - Window: "implant" puts the inserted chip into the occupant's brain after 15 s (inactive;
//!   failing tells why), "remove" takes an implanted chip out after 15 s into the occupant's
//!   hands (a non-removable one is destroyed), "toggle_activate" activates or deactivates a chip
//!   at once, "eject" hands the inserted chip to the occupant. A chip on cooldown can be neither
//!   removed nor toggled. Opening the station, losing power or the chip leaving it interrupts
//!   the work; everything is checked again when it ends.
// shortcut: no circuit board or construction, no `INTERACT_ATOM_IGNORE_MOBILITY` beyond the
// framework's status.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_shared::chat::Span;
use ss13_shared::ui::UiStatus;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::skill_station::{
    ACT_EJECT, ACT_IMPLANT, ACT_REMOVE, ACT_TOGGLE, PARAM_REF, SkillChipEntry, SkillStationData,
    SkillStationUi,
};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiKind, UiParams, UiReject, UiSystems, UiWindow,
};

use crate::closets::{ClosetRelayMove, InCloset};
use crate::inventory::{CarriedBy, Inventory};
use crate::medical::organs::OrganHost;
use crate::medical::pods::{Pod, close_pod, occupant, open_pod};
use crate::obj_damage::is_under;
use crate::power::Unpowered;
use crate::skillchips::{self, ChipIn, Skillchip};
use crate::storage::StoredIn;
use crate::tools::{now, tell_mob};

pub const ROOT: &str = "/obj/machinery/skill_station";
/// `SKILLCHIP_IMPLANT_TIME`, `SKILLCHIP_REMOVAL_TIME` (15 seconds).
const WORK_SECS: f64 = 15.0;
const ICON: &str = "icons/obj/machines/implant_chair.dmi";

/// What the station is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Work {
    Implant,
    Remove(Entity),
}

/// `/obj/machinery/skill_station` vars and the window's data last read.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct SkillStation {
    /// `working` and `work_timer`: the operation and when it ends (game seconds).
    pub work: Option<(Work, f64)>,
    /// `inserted_skillchip`.
    pub inserted: Option<Entity>,
    view: SkillStationData,
}

pub struct SkillStationPlugin;

impl Plugin for SkillStationPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::EmptyHand, 2651, |w, c| {
            interact(w, c.client, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 2641, |w, c| {
            c.held
                .is_some_and(|held| item_interaction(w, c.mob, held, c.target))
        });
        app.register_ui::<StationWindow>()
            .add_observer(attach)
            .add_systems(
                Update,
                (
                    cap_windows.in_set(UiSystems::StatusHook),
                    acts.in_set(UiSystems::Handle),
                    refresh_views
                        .after(UiSystems::Handle)
                        .before(UiSystems::Send),
                    relay_moves.after(ss13_world::WorldSystems::Movement),
                    timers.run_if(any_with_component::<SkillStation>),
                ),
            );
    }
}

fn attach(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(o) = objs.get(add.entity) else {
        return;
    };
    if !is_under(&o.path, ROOT) {
        return;
    }
    let e = add.entity;
    // `state_open = TRUE`; the occupant works it from inside.
    commands.entity(e).insert_if_new((
        Pod { open: true },
        SkillStation::default(),
        crate::click::ControlsFromInside,
    ));
    commands.queue(move |world: &mut World| refresh_look(world, e));
}

fn is_open(world: &World, e: Entity) -> bool {
    world.get::<Pod>(e).is_some_and(|p| p.open)
}

/// `update_icon_state` (`implantchair`, `_open`, `_occupied`) and the `working` overlay.
pub fn refresh_look(world: &mut World, e: Entity) {
    let Some(working) = world.get::<SkillStation>(e).map(|s| s.work.is_some()) else {
        return;
    };
    let mut state = "implantchair".to_owned();
    if is_open(world, e) {
        state.push_str("_open");
    }
    if occupant(world, e).is_some() {
        state.push_str("_occupied");
    }
    crate::obj_damage::set_icon_state(world, e, &state);
    let overlays: Vec<ss13_shared::Overlay> = working
        .then(|| {
            world
                .get_resource::<ss13_mapload::IconIndex>()
                .and_then(|i| i.get(ICON, "working"))
        })
        .flatten()
        .map(ss13_shared::Overlay::new)
        .into_iter()
        .collect();
    if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(e)
        && look.overlays != overlays
    {
        look.overlays = overlays;
    }
}

/// `interact`: the occupant gets the window, anyone else opens or closes it. True when `e` is
/// a station.
pub fn interact(world: &mut World, client: Entity, mob: Entity, e: Entity) -> bool {
    if world.get::<SkillStation>(e).is_none() {
        return false;
    }
    if occupant(world, e) == Some(mob) {
        open_window(world, client, mob, e);
    } else if is_open(world, e) {
        close(world, e, None);
    } else {
        open(world, e);
    }
    true
}

fn open_window(world: &mut World, client: Entity, mob: Entity, e: Entity) {
    refresh_view(world, e);
    let title = crate::examine::name_of(world, e);
    world.write_message(OpenUi::new::<StationWindow>(client, mob, e, title));
}

/// `close_machine(target)`: shuts in `target`, else the mob on its tile, and opens the window
/// for it.
pub fn close(world: &mut World, e: Entity, target: Option<Entity>) {
    let mob = close_pod(world, e, target);
    refresh_look(world, e);
    refresh_view(world, e);
    if let Some(mob) = mob
        && let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client)
    {
        open_window(world, client, mob, e);
    }
}

/// `open_machine`: the occupant out, the work interrupted.
pub fn open(world: &mut World, e: Entity) {
    open_pod(world, e);
    interrupt(world, e);
}

/// `interrupt_operation`.
fn interrupt(world: &mut World, e: Entity) {
    if let Some(mut s) = world.get_mut::<SkillStation>(e) {
        s.work = None;
    }
    refresh_look(world, e);
    refresh_view(world, e);
}

/// `item_interaction`: a skillchip goes in. True when `held` is a chip used on a station.
pub fn item_interaction(world: &mut World, user: Entity, held: Entity, e: Entity) -> bool {
    if world.get::<SkillStation>(e).is_none() || world.get::<Skillchip>(held).is_none() {
        return false;
    }
    if inserted(world, e).is_some() {
        tell_mob(
            world,
            user,
            Span::Notice,
            "There's already a skillchip inside.",
        );
        return true;
    }
    {
        let Some(mut inventory) = world.get_mut::<Inventory>(user) else {
            return true;
        };
        let Some(hand) = inventory.hands.iter().position(|h| *h == Some(held)) else {
            return true;
        };
        inventory.hands[hand] = None;
    }
    world.entity_mut(held).remove::<CarriedBy>();
    crate::storage::put_inside(world, held, e);
    crate::inventory::refresh_look(world, user);
    if let Some(mut s) = world.get_mut::<SkillStation>(e) {
        s.inserted = Some(held);
    }
    refresh_view(world, e);
    true
}

/// `inserted_skillchip`, cleared (and the work interrupted) when it left (`Exited`).
fn inserted(world: &mut World, e: Entity) -> Option<Entity> {
    let chip = world.get::<SkillStation>(e)?.inserted?;
    if world.get::<StoredIn>(chip).is_some_and(|s| s.0 == e) {
        return Some(chip);
    }
    let implanting = world
        .get::<SkillStation>(e)
        .is_some_and(|s| matches!(s.work, Some((Work::Implant, _))));
    if let Some(mut s) = world.get_mut::<SkillStation>(e) {
        s.inserted = None;
    }
    if implanting {
        interrupt(world, e);
    }
    None
}

/// `relaymove`: a step from inside opens it.
fn relay_moves(world: &mut World, mut moves: Local<MessageCursor<ClosetRelayMove>>) {
    let mobs: Vec<Entity> = moves
        .read(world.resource::<Messages<ClosetRelayMove>>())
        .map(|m| m.mob)
        .collect();
    for mob in mobs {
        let Some(e) = world.get::<InCloset>(mob).map(|c| c.closet) else {
            continue;
        };
        if world.get::<SkillStation>(e).is_some() && !is_open(world, e) {
            open(world, e);
        }
    }
}

/// The work's end, revalidated; power loss and a lost chip interrupt it.
// Cost: per tick one pass over the stations (a few per map), a field read each.
fn timers(world: &mut World) {
    let t = now(world);
    let stations: Vec<(Entity, Work, f64)> = world
        .query::<(Entity, &SkillStation)>()
        .iter(world)
        .filter_map(|(e, s)| s.work.map(|(w, at)| (e, w, at)))
        .collect();
    for (e, work, at) in stations {
        if world.get::<Unpowered>(e).is_some() {
            interrupt(world, e);
            continue;
        }
        if work == Work::Implant && inserted(world, e).is_none() {
            continue;
        }
        if t < at {
            continue;
        }
        if let Some(mut s) = world.get_mut::<SkillStation>(e) {
            s.work = None;
        }
        match work {
            Work::Implant => finish_implant(world, e),
            Work::Remove(chip) => finish_removal(world, e, chip),
        }
        refresh_look(world, e);
        refresh_view(world, e);
    }
}

/// `start_implanting`.
fn start_implant(world: &mut World, e: Entity) {
    let (Some(mob), Some(chip)) = (occupant(world, e), inserted(world, e)) else {
        return;
    };
    if skillchips::mob_incompatibility(world, Some(mob), chip).is_some() {
        return;
    }
    let t = now(world);
    if let Some(mut s) = world.get_mut::<SkillStation>(e) {
        s.work = Some((Work::Implant, t + WORK_SECS));
    }
    refresh_look(world, e);
}

/// `implant`.
fn finish_implant(world: &mut World, e: Entity) {
    let (Some(mob), Some(chip)) = (occupant(world, e), inserted(world, e)) else {
        return;
    };
    match skillchips::implant(world, mob, chip, false) {
        Err(reason) => tell_mob(
            world,
            mob,
            Span::Notice,
            format!("Operation failed! {reason}"),
        ),
        Ok(()) => {
            tell_mob(world, mob, Span::Notice, "Operation complete!");
            if let Some(mut s) = world.get_mut::<SkillStation>(e) {
                s.inserted = None;
            }
        }
    }
}

/// The occupant's brain chip `chip_ref` names (`locate(chipref) in occupant_brain.skillchips`).
fn occupant_chip(world: &mut World, e: Entity, chip_ref: u64) -> Option<(Entity, Entity)> {
    let mob = occupant(world, e)?;
    let brain = skillchips::brain_of(world, mob)?;
    let chip = skillchips::chips(world, brain)
        .into_iter()
        .find(|c| c.to_bits() == chip_ref)?;
    Some((mob, chip))
}

/// `start_removal`.
fn start_removal(world: &mut World, e: Entity, mob: Entity, chip: Entity) {
    let t = now(world);
    if world
        .get::<Skillchip>(chip)
        .is_some_and(|c| c.on_cooldown(t))
    {
        tell_mob(
            world,
            mob,
            Span::Notice,
            "DANGER! Operation cannot be completed, removal is unsafe.",
        );
        return;
    }
    if let Some(mut s) = world.get_mut::<SkillStation>(e) {
        s.work = Some((Work::Remove(chip), t + WORK_SECS));
    }
    refresh_look(world, e);
}

/// `remove_skillchip`: into the occupant's hands, or destroyed when not removable.
fn finish_removal(world: &mut World, e: Entity, chip: Entity) {
    let Some(mob) = occupant(world, e) else {
        return;
    };
    let t = now(world);
    if world
        .get::<Skillchip>(chip)
        .is_some_and(|c| c.on_cooldown(t))
    {
        tell_mob(
            world,
            mob,
            Span::Notice,
            "Safety mechanisms activated! Skillchip cannot be safely removed.",
        );
        return;
    }
    if world.get::<OrganHost>(mob).is_none() {
        tell_mob(
            world,
            mob,
            Span::Notice,
            "Occupant does not appear to be a carbon-based lifeform!",
        );
        return;
    }
    let in_brain = skillchips::brain_of(world, mob)
        .is_some_and(|b| world.get::<ChipIn>(chip).is_some_and(|c| c.0 == b));
    if !in_brain || !skillchips::remove(world, chip, false) {
        tell_mob(world, mob, Span::Notice, "Failed to remove skillchip!");
        return;
    }
    if world.get::<Skillchip>(chip).is_some_and(|c| c.removable) {
        skillchips::give_to_hands(world, mob, chip);
    } else {
        world.despawn(chip);
    }
    tell_mob(world, mob, Span::Notice, "Operation complete!");
}

/// `toggle_chip_active`.
fn toggle(world: &mut World, mob: Entity, chip: Entity) {
    let t = now(world);
    let Some(state) = world.get::<Skillchip>(chip).cloned() else {
        return;
    };
    if state.on_cooldown(t) {
        tell_mob(
            world,
            mob,
            Span::Notice,
            "Safety mechanisms activated! Skillchip cannot be safely modified.",
        );
        return;
    }
    if state.active {
        if let Some(reason) = skillchips::try_deactivate(world, chip, false, false, None) {
            tell_mob(
                world,
                mob,
                Span::Notice,
                format!("Failed to deactivate skillchip! {reason}"),
            );
        }
        return;
    }
    if let Some(reason) = skillchips::try_activate(world, chip, false, false) {
        tell_mob(
            world,
            mob,
            Span::Notice,
            format!("Failed to activate skillchip! {reason}"),
        );
    }
}

/// `get_chip_data`.
fn chip_entry(world: &World, chip: Entity, t: f64) -> Option<SkillChipEntry> {
    let state = world.get::<Skillchip>(chip)?;
    let brain = world.get::<ChipIn>(chip).map(|c| c.0);
    Some(SkillChipEntry {
        name: state.def.skill_name.to_owned(),
        icon: state.def.icon.to_owned(),
        desc: state.def.desc.to_owned(),
        complexity: state.def.complexity,
        slot_use: state.def.slot_use,
        removable: state.removable,
        chip_ref: chip.to_bits().to_string(),
        active: state.active,
        active_error: skillchips::activate_incompatibility(world, brain, chip),
        cooldown: state.cooldown_left(t) as f32,
    })
}

/// `ui_data`.
fn compute_view(world: &mut World, e: Entity) -> SkillStationData {
    let t = now(world);
    let work = world.get::<SkillStation>(e).and_then(|s| s.work);
    let mut data = SkillStationData {
        working: work.is_some(),
        timeleft: work.map(|(_, at)| (at - t).max(0.0) as f32),
        ..default()
    };
    let mob = occupant(world, e);
    if let Some(chip) = inserted(world, e)
        && let Some(entry) = chip_entry(world, chip, t)
    {
        let reason = skillchips::mob_incompatibility(world, mob, chip);
        data.skillchip_ready = true;
        data.implantable = reason.is_none();
        data.implantable_reason = reason;
        data.skill_name = entry.name;
        data.skill_desc = entry.desc;
        data.skill_icon = entry.icon;
        data.complexity = entry.complexity;
        data.slot_use = entry.slot_use;
    }
    let Some(mob) = mob else {
        data.error = Some(
            "No valid occupant detected. Please consult nearest medical practitioner.".to_owned(),
        );
        return data;
    };
    let Some(brain) = skillchips::brain_of(world, mob) else {
        data.error =
            Some("Brain not detected. Please consult nearest medical practitioner.".to_owned());
        return data;
    };
    data.complexity_used = skillchips::used_complexity(world, brain);
    data.complexity_max = skillchips::max_complexity(world, brain);
    data.slots_used = skillchips::used_slots(world, brain);
    data.slots_max = skillchips::MAX_SLOTS;
    data.current = skillchips::chips(world, brain)
        .into_iter()
        .filter_map(|c| chip_entry(world, c, t))
        .collect();
    data
}

fn refresh_view(world: &mut World, e: Entity) {
    if world.get::<SkillStation>(e).is_none() {
        return;
    }
    let view = compute_view(world, e);
    if let Some(mut s) = world.get_mut::<SkillStation>(e)
        && s.view != view
    {
        s.view = view;
    }
}

// Cost: per tick, one pass over the open station windows.
fn refresh_views(world: &mut World) {
    let mut q = world.query_filtered::<&UiWindow, With<UiKind<StationWindow>>>();
    let mut stations: Vec<Entity> = q.iter(world).map(|w| w.source).collect();
    stations.sort();
    stations.dedup();
    for e in stations {
        refresh_view(world, e);
    }
}

/// `contained_state`: only the occupant uses it.
fn cap_windows(world: &mut World) {
    let windows: Vec<(Entity, Entity, Entity)> = world
        .query_filtered::<(Entity, &UiWindow), With<UiKind<StationWindow>>>()
        .iter(world)
        .map(|(e, w)| (e, w.actor, w.source))
        .collect();
    for (window, user, e) in windows {
        if world.get::<InCloset>(user).is_none_or(|c| c.closet != e)
            && let Some(mut w) = world.get_mut::<UiWindow>(window)
        {
            w.cap_status(UiStatus::Close);
        }
    }
}

/// The server side of the `SkillStation` interface.
pub struct StationWindow;

impl UiInterface for StationWindow {
    const NAME: &'static str = SkillStationUi::NAME;
    type Data = SkillStationData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StationAct {
    Implant,
    Eject,
    /// The chip's `ref`.
    Remove(u64),
    Toggle(u64),
}

impl ServerUi for StationWindow {
    type Source = &'static SkillStation;
    type UserParam = ();
    type Act = StationAct;

    fn static_data(_: &SkillStation) {}

    fn data(station: &SkillStation, _: Entity, _: &()) -> SkillStationData {
        station.view.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<StationAct, UiReject> {
        let chip_ref = || {
            params
                .get(PARAM_REF)
                .and_then(|r| r.parse::<u64>().ok())
                .ok_or_else(|| UiReject::BadParams(format!("{PARAM_REF}: expected a chip")))
        };
        Ok(match action {
            ACT_IMPLANT => StationAct::Implant,
            ACT_EJECT => StationAct::Eject,
            ACT_REMOVE => StationAct::Remove(chip_ref()?),
            ACT_TOGGLE => StationAct::Toggle(chip_ref()?),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

/// `ui_act`, checked at the moment of effect: the actor is the occupant, the station idle.
fn acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<StationWindow>>>) {
    let pending: Vec<(Entity, Entity, StationAct)> = acts
        .read(world.resource::<Messages<UiActed<StationWindow>>>())
        .map(|a| (a.source, a.actor, a.act))
        .collect();
    for (e, actor, act) in pending {
        let Some(working) = world.get::<SkillStation>(e).map(|s| s.work.is_some()) else {
            continue;
        };
        if occupant(world, e) != Some(actor) || working {
            continue;
        }
        match act {
            StationAct::Implant => start_implant(world, e),
            StationAct::Eject => {
                if let Some(chip) = inserted(world, e) {
                    tell_mob(world, actor, Span::Notice, "You eject the skillchip.");
                    if let Some(mut s) = world.get_mut::<SkillStation>(e) {
                        s.inserted = None;
                    }
                    skillchips::give_to_hands(world, actor, chip);
                }
            }
            StationAct::Remove(chip_ref) => {
                if let Some((mob, chip)) = occupant_chip(world, e, chip_ref) {
                    start_removal(world, e, mob, chip);
                }
            }
            StationAct::Toggle(chip_ref) => {
                if let Some((mob, chip)) = occupant_chip(world, e, chip_ref) {
                    toggle(world, mob, chip);
                }
            }
        }
        refresh_view(world, e);
    }
}
