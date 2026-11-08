//! GPS units (server), after tg `/obj/item/gps` (`code/game/objects/items/devices/gps.dm`) and
//! `/datum/component/gps/item` (`code/datums/components/gps.dm`), tgui `Gps`
//! (`ss13_tgui::interfaces::gps`).
//!
//! Every `/obj/item/gps` gets a [`Gps`] with its type's tag, named "<name> (<tag>)" and drawn
//! with the "working" overlay while tracking. Used in hand (the survival pod's computer: clicked)
//! it opens the window: its own area and coordinates and every other tracking GPS (all levels,
//! or its own level in "LOCAL" range) with the distance and bearing on the same level. The window
//! renames the tag (20 characters), switches tracking (an untracked GPS is invisible to the
//! others) and the automatic refresh ("MANUAL" refreshes only on a button). An EMP knocks it out
//! for 30 s ("emp" overlay, the window closes, "слабо шипит" when used).
// shortcut: no alt-click tracking toggle (the inventory's alt-click chain is shared; the window's
// power button does the same), no bearings to linked levels (`get_linked_z_angle`), no
// `visible_debug` floor tagging, no GPS on non-items but emagged meteor shields (no tendrils or
// kheiral cuffs), the pAI's `inventory_state` is the item's default.
// Cost: the views of GPS units with an open window, once a second, each one pass over the GPS
// units (dozens on a station).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmItem, DmName, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::interfaces::gps::{GpsData, GpsSignal, GpsUi};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_tgui::server::{
    AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_tgui::{UiInterface, UiStatus};

use crate::health::Stat;
use crate::inventory::Inventory;
use crate::modpc::pda::turf_of;
use crate::tools::tell;

const GPS: &str = "/obj/item/gps";
/// `/obj/item/gps/computer`: the survival pod's, used by hand where it stands.
const POD_COMPUTER: &str = "/obj/item/gps/computer";
/// `/datum/component/gps/Initialize(_gpstag = "COM0")`.
const DEFAULT_TAG: &str = "COM0";
/// `tgui_input_text(max_length = 20)`.
const TAG_MAX: u32 = 20;
/// `addtimer(CALLBACK(src, PROC_REF(reboot)), 300)`.
const EMP_SECS: f64 = 30.0;
/// SStgui autoupdate, about once a second.
const REFRESH_SECS: f64 = 1.0;
const WORKING: &str = "working";
const EMP_OVERLAY: &str = "emp";

/// `/datum/component/gps/item`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Gps {
    pub tag: String,
    pub tracking: bool,
    /// Game seconds the EMP lasts until (`emped` while `Some`).
    pub emped_until: Option<f64>,
    pub updating: bool,
    pub global_mode: bool,
    /// `overlay_state`: the visible debug GPS has none.
    pub overlay: bool,
    /// What the window shows (`ui_data`), refreshed while it is open and auto-updating.
    pub view: GpsData,
    /// `initial(name)`.
    base_name: String,
    ready: bool,
}

impl Gps {
    /// The type's `gpstag` and `tracking_on`, `overlay_icon` (`gps.dm`).
    pub fn of(path: &str) -> Self {
        let tag = [
            ("/obj/item/gps/spaceruin", "Distant Signal"),
            ("/obj/item/gps/science", "SCI0"),
            ("/obj/item/gps/engineering", "ENG0"),
            ("/obj/item/gps/mining/internal", "MINER"),
            ("/obj/item/gps/mining", "MINE0"),
            ("/obj/item/gps/cyborg", "BORG0"),
            ("/obj/item/gps/pai", "PAI0"),
            ("/obj/item/gps/visible_debug", "ADMIN"),
        ]
        .iter()
        .find(|(root, _)| is_under(path, root))
        .map_or(DEFAULT_TAG, |(_, tag)| tag);
        let debug = is_under(path, "/obj/item/gps/visible_debug");
        Self {
            tag: tag.to_owned(),
            tracking: !debug,
            emped_until: None,
            updating: true,
            global_mode: true,
            overlay: !debug,
            view: GpsData::default(),
            base_name: String::new(),
            ready: false,
        }
    }

    /// `/datum/component/gps` on a non-item (`AddComponent(/datum/component/gps, tag)`):
    /// tracking under `tag`, never renamed, no overlay and no EMP handling.
    pub fn fixed(tag: &str) -> Self {
        Self {
            tag: tag.to_owned(),
            overlay: false,
            ready: true,
            ..Self::of("")
        }
    }

    pub fn emped(&self) -> bool {
        self.emped_until.is_some()
    }
}

/// The GPS a rename prompt is about, and the window it came from.
#[derive(Component, Clone, Copy, Debug)]
struct GpsRename {
    gps: Entity,
    window: Entity,
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // `COMSIG_ITEM_ATTACK_SELF` -> `interact`.
    app.add_click_handler(ClickPhase::AttackSelf, 1720, |w, c| {
        interact(w, c.client, c.mob, c.target)
    });
    // `/obj/item/gps/computer/attack_hand` -> `attack_self`.
    app.add_click_handler(ClickPhase::EmptyHand, 3457, |w, c| {
        w.get::<DmItem>(c.target)
            .is_some_and(|i| is_under(&i.path, POD_COMPUTER))
            && interact(w, c.client, c.mob, c.target)
    });
    app.register_ui::<GpsWindow>()
        .add_observer(add_gps)
        .add_systems(
            Update,
            (
                init_gps,
                gps_emps.in_set(crate::emp::EmpSystems::Effects),
                reboot,
                refresh_views,
                window_status.in_set(UiSystems::StatusHook),
                (gps_acts, rename_answers)
                    .chain()
                    .in_set(UiSystems::Handle)
                    .after(ss13_tgui::prompt::PromptSystems::Answer),
            ),
        );
}

fn add_gps(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if let Ok(item) = items.get(add.entity)
        && is_under(&item.path, GPS)
    {
        commands
            .entity(add.entity)
            .try_insert_if_new((Gps::of(&item.path), AlwaysState));
    }
}

/// `Initialize`: the name with the tag and the "working" overlay, once the prototype is
/// complete (a pass over the GPS units; only new ones are not `ready`).
fn init_gps(world: &mut World) {
    let mut q = world.query::<(Entity, &Gps)>();
    let new: Vec<Entity> = q
        .iter(world)
        .filter(|(_, g)| !g.ready)
        .map(|(e, _)| e)
        .collect();
    for e in new {
        let base = world
            .get::<DmName>(e)
            .map(|n| n.name.clone())
            .unwrap_or_default();
        if let Some(mut gps) = world.get_mut::<Gps>(e) {
            gps.base_name = base;
            gps.ready = true;
        }
        rename(world, e);
        update_overlay(world, e);
    }
}

/// `A.name = "[initial(A.name)] ([gpstag])"`.
fn rename(world: &mut World, e: Entity) {
    let Some(gps) = world.get::<Gps>(e) else {
        return;
    };
    let name = format!("{} ({})", gps.base_name, gps.tag);
    match world.get_mut::<DmName>(e) {
        Some(mut n) => {
            if n.name != name {
                n.name = name;
            }
        }
        None => {
            world.entity_mut(e).insert(DmName {
                name,
                desc: String::new(),
            });
        }
    }
}

/// "working" while tracking, "emp" while EMPed, nothing otherwise.
fn update_overlay(world: &mut World, e: Entity) {
    let (Some(gps), Some(icon)) = (
        world.get::<Gps>(e),
        world.get::<DmAtom>(e).map(|a| a.icon.clone()),
    ) else {
        return;
    };
    let state = if gps.emped() {
        Some(EMP_OVERLAY)
    } else if gps.tracking && gps.overlay {
        Some(WORKING)
    } else {
        None
    };
    let overlays: Vec<Overlay> = state
        .and_then(|s| world.get_resource::<IconIndex>()?.get(&icon, s))
        .map(Overlay::new)
        .into_iter()
        .collect();
    if let Some(mut appearance) = world.get_mut::<Appearance>(e)
        && appearance.overlays != overlays
    {
        appearance.overlays = overlays;
    }
}

/// `interact` -> `ui_interact`: an EMPed unit only fizzles. True when `item` is a GPS.
pub fn interact(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(gps) = world.get::<Gps>(item) else {
        return false;
    };
    let name = crate::medical::name_of(world, item);
    if gps.emped() {
        tell(world, client, Span::Notice, format!("{name} слабо шипит."));
        return true;
    }
    refresh(world, item);
    world.write_message(OpenUi::new::<GpsWindow>(client, mob, item, name));
    true
}

/// BYOND `get_angle` without pixel offsets (`delta_to_angle`): 0 north, clockwise.
pub fn bearing(from: TilePos, to: TilePos) -> f32 {
    let (dx, dy) = (f64::from(to.x - from.x), f64::from(to.y - from.y));
    if dy == 0.0 {
        return if dx >= 0.0 { 90.0 } else { 270.0 };
    }
    let mut angle = (dx / dy).atan().to_degrees();
    if dy < 0.0 {
        angle += 180.0;
    } else if dx < 0.0 {
        angle += 360.0;
    }
    angle.round() as f32
}

fn coords(pos: TilePos) -> String {
    format!("{}, {}, {}", pos.x, pos.y, pos.z)
}

/// `ui_data` of `e`.
pub fn view(world: &mut World, e: Entity) -> GpsData {
    let Some(gps) = world.get::<Gps>(e).cloned() else {
        return GpsData::default();
    };
    let mut data = GpsData {
        power: gps.tracking,
        tag: gps.tag.clone(),
        updating: gps.updating,
        globalmode: gps.global_mode,
        ..default()
    };
    if !gps.tracking || gps.emped() {
        return data;
    }
    let Some(curr) = turf_of(world, e) else {
        return data;
    };
    data.current_area = crate::bots::area_name(world, curr);
    data.current_coords = coords(curr);
    let mut q = world.query::<(Entity, &Gps)>();
    let others: Vec<(Entity, String)> = q
        .iter(world)
        .filter(|(o, g)| *o != e && g.tracking && !g.emped())
        .map(|(o, g)| (o, g.tag.clone()))
        .collect();
    for (other, tag) in others {
        let Some(pos) = turf_of(world, other) else {
            continue;
        };
        if !gps.global_mode && pos.z != curr.z {
            continue;
        }
        let same = pos.z == curr.z;
        let (dx, dy) = (f64::from(pos.x - curr.x), f64::from(pos.y - curr.y));
        data.signals.push(GpsSignal {
            entrytag: tag,
            coords: coords(pos),
            dist: same.then(|| dx.hypot(dy).round() as u32),
            degrees: same.then(|| bearing(curr, pos)),
        });
    }
    data
}

fn refresh(world: &mut World, e: Entity) {
    let data = view(world, e);
    if let Some(mut gps) = world.get_mut::<Gps>(e)
        && gps.view != data
    {
        gps.view = data;
    }
}

/// `ui.set_autoupdate(updating)`: the open windows of auto-updating units follow the world.
fn refresh_views(world: &mut World, mut next: Local<f64>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + REFRESH_SECS;
    let mut windows = world.query::<&UiWindow>();
    let mut sources: Vec<Entity> = windows
        .iter(world)
        .filter(|w| w.interface == GpsUi::NAME)
        .map(|w| w.source)
        .collect();
    sources.sort_unstable();
    sources.dedup();
    for e in sources {
        if world.get::<Gps>(e).is_some_and(|g| g.updating) {
            refresh(world, e);
        }
    }
}

/// tg `default_state` for an item (`super::item_ui_status`), closed while EMPed
/// (`SStgui.close_uis`).
fn window_status(
    mut windows: Query<&mut UiWindow>,
    units: Query<(&Gps, Option<&TilePos>)>,
    users: Query<(&Inventory, Option<&Stat>, Option<&TilePos>)>,
) {
    for mut window in &mut windows {
        if window.interface != GpsUi::NAME {
            continue;
        }
        let Ok((gps, at)) = units.get(window.source) else {
            window.cap_status(UiStatus::Close);
            continue;
        };
        let status = if gps.emped() {
            UiStatus::Close
        } else {
            super::item_ui_status(window.source, at, users.get(window.actor).ok())
        };
        window.cap_status(status);
    }
}

/// `toggletracking(user)`.
fn toggle_tracking(world: &mut World, client: Entity, e: Entity) {
    let Some((emped, tracking)) = world.get::<Gps>(e).map(|g| (g.emped(), !g.tracking)) else {
        return;
    };
    let name = crate::medical::name_of(world, e);
    if emped {
        tell(world, client, Span::Warning, "Он сломан!");
        return;
    }
    if let Some(mut gps) = world.get_mut::<Gps>(e) {
        gps.tracking = tracking;
    }
    let text = if tracking {
        format!("{name} теперь отслеживает и виден другим GPS.")
    } else {
        format!("{name} больше не отслеживает и не виден другим GPS.")
    };
    tell(world, client, Span::Notice, text);
    update_overlay(world, e);
}

pub struct GpsWindow;

impl UiInterface for GpsWindow {
    const NAME: &'static str = GpsUi::NAME;
    type Data = GpsData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpsAct {
    Rename,
    Power,
    Updating,
    GlobalMode,
}

impl ServerUi for GpsWindow {
    type Source = &'static Gps;
    type UserParam = ();
    type Act = GpsAct;

    fn static_data(_: &Gps) {}

    fn data(gps: &Gps, _: Entity, _: &()) -> GpsData {
        gps.view.clone()
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<GpsAct, UiReject> {
        match action {
            "rename" => Ok(GpsAct::Rename),
            "power" => Ok(GpsAct::Power),
            "updating" => Ok(GpsAct::Updating),
            "globalmode" => Ok(GpsAct::GlobalMode),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// `ui_act`. Every handled act refreshes the view (tgui sends the data after an act, also in
/// manual mode).
fn gps_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<GpsWindow>>>) {
    let pending: Vec<(Entity, Entity, Entity, Entity, GpsAct)> = acts
        .read(world.resource::<Messages<UiActed<GpsWindow>>>())
        .map(|a| (a.window, a.client, a.actor, a.source, a.act))
        .collect();
    for (window, client, actor, source, act) in pending {
        let Some(gps) = world.get::<Gps>(source).cloned() else {
            continue;
        };
        match act {
            GpsAct::Rename => {
                let prompt = ask_world(
                    world,
                    Ask::new(
                        client,
                        actor,
                        "GPS Tag",
                        "Введите желаемую метку",
                        PromptKind::Text {
                            default: gps.tag.clone(),
                            max_length: TAG_MAX,
                            multiline: false,
                        },
                    ),
                );
                world.entity_mut(prompt).insert(GpsRename {
                    gps: source,
                    window,
                });
            }
            GpsAct::Power => toggle_tracking(world, client, source),
            GpsAct::Updating => {
                if let Some(mut g) = world.get_mut::<Gps>(source) {
                    g.updating = !g.updating;
                }
            }
            GpsAct::GlobalMode => {
                if let Some(mut g) = world.get_mut::<Gps>(source) {
                    g.global_mode = !g.global_mode;
                }
            }
        }
        refresh(world, source);
    }
}

/// The new tag, if the window is still open and interactive (`QDELETED(ui) || ui.status !=
/// UI_INTERACTIVE`) and the answer is not empty.
fn rename_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.answer.clone()))
        .collect();
    for (prompt, answer) in answers {
        let Some(&GpsRename { gps, window }) = world.get::<GpsRename>(prompt) else {
            continue;
        };
        let Some(PromptAnswer::Text(tag)) = answer else {
            continue;
        };
        let open = world
            .get::<UiWindow>(window)
            .is_some_and(|w| w.source == gps && w.status() == UiStatus::Interactive);
        let tag: String = tag.trim().chars().take(TAG_MAX as usize).collect();
        if !open || tag.is_empty() {
            continue;
        }
        let Some(mut g) = world.get_mut::<Gps>(gps) else {
            continue;
        };
        g.tag = tag;
        rename(world, gps);
        refresh(world, gps);
    }
}

/// `on_emp_act`: out for 30 s from the last pulse (`TIMER_OVERRIDE`), the window closes.
fn gps_emps(world: &mut World, mut cursor: Local<MessageCursor<crate::emp::EmpAct>>) {
    for ev in crate::emp::emp_acts(world, &mut cursor) {
        // The base component (a non-item) has no `emp_act` hook.
        if !ev.hits_self()
            || world.get::<Gps>(ev.target).is_none()
            || world.get::<DmItem>(ev.target).is_none()
        {
            continue;
        }
        let until = world.resource::<Time>().elapsed_secs_f64() + EMP_SECS;
        if let Some(mut gps) = world.get_mut::<Gps>(ev.target) {
            gps.emped_until = Some(until);
        }
        update_overlay(world, ev.target);
    }
}

/// `reboot`.
fn reboot(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &Gps)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, g)| g.emped_until.is_some_and(|t| now >= t))
        .map(|(e, _)| e)
        .collect();
    for e in due {
        if let Some(mut gps) = world.get_mut::<Gps>(e) {
            gps.emped_until = None;
        }
        update_overlay(world, e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_follow_the_type() {
        assert_eq!(Gps::of("/obj/item/gps").tag, "COM0");
        assert_eq!(Gps::of("/obj/item/gps/mining").tag, "MINE0");
        assert_eq!(Gps::of("/obj/item/gps/mining/internal").tag, "MINER");
        assert_eq!(Gps::of("/obj/item/gps/computer").tag, "COM0");
        let debug = Gps::of("/obj/item/gps/visible_debug");
        assert!(!debug.tracking && !debug.overlay);
    }

    #[test]
    fn bearings_are_clockwise_from_north() {
        let at = TilePos::new(10, 10, 1);
        assert_eq!(bearing(at, TilePos::new(10, 20, 1)), 0.0);
        assert_eq!(bearing(at, TilePos::new(20, 10, 1)), 90.0);
        assert_eq!(bearing(at, TilePos::new(10, 0, 1)), 180.0);
        assert_eq!(bearing(at, TilePos::new(0, 10, 1)), 270.0);
        assert_eq!(bearing(at, TilePos::new(20, 20, 1)), 45.0);
        assert_eq!(bearing(at, TilePos::new(0, 20, 1)), 315.0);
    }
}
