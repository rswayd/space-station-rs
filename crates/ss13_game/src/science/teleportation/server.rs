//! The teleportation plugin (server): click hooks, bumps, window registration and the helpers
//! the machines share.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmName, DmObj, DmPower};
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Dir, TilePos};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiSystems, UiWindow};
use ss13_tgui::{UiInterface as _, UiStatus};
use ss13_world::{Bumped, SpatialIndex};

use super::{GatewayUi, LaunchpadConsoleUi, SatelliteControlUi, TeleporterUi};
use super::{gateway, hand_tele, launchpad, sat_control, teleporter};
use crate::obj_damage::is_under;
use crate::power::Unpowered;
use crate::tools::{ToolKind, tool_of};

/// `BASE_MACHINE_ACTIVE_CONSUMPTION`, joules per use when the type sets none.
const BASE_ACTIVE: f64 = 1000.0;

pub struct TeleportationPlugin;

impl Plugin for TeleportationPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // The hand tele's destination list, the locator's window.
        app.add_click_handler(ClickPhase::AttackSelf, 1745, |w, c| {
            hand_tele::attack_self(w, c.client, c.mob, c.target)
        });
        // Teleporter station toggle; teleporter, launchpad, satellite and gateway consoles;
        // away gateways.
        app.add_click_handler(ClickPhase::EmptyHand, 3475, |w, c| {
            attack_hand(w, c.client, c.mob, c.target)
        });
        // A portal next to an empty hand: through it.
        app.add_click_handler(ClickPhase::EmptyHand, 3485, |w, c| {
            hand_tele::portal_hand(w, c.mob, c.target)
        });
        // Multitool links (teleporter stations, launchpads to their console), gateway
        // calibration.
        app.add_click_handler(ClickPhase::HeldOnTarget, 1575, |w, c| {
            c.held
                .is_some_and(|h| multitool_act(w, c.client, c.mob, h, c.target))
        });
        // The hand tele on its portals and its right click; other items on a portal.
        app.add_click_handler(ClickPhase::HeldOnTarget, 1545, |w, c| {
            c.held.is_some_and(|h| hand_tele::item_interaction(w, c, h))
        });
        app.register_ui::<TeleporterUi>()
            .register_ui::<super::BluespaceLocatorUi>()
            .register_ui::<LaunchpadConsoleUi>()
            .register_ui::<SatelliteControlUi>()
            .register_ui::<GatewayUi>()
            .add_systems(
                Update,
                (
                    (
                        teleporter::console_acts,
                        launchpad::console_acts,
                        sat_control::console_acts,
                        gateway::console_acts,
                    )
                        .in_set(UiSystems::Handle),
                    (
                        teleporter::target_answers,
                        hand_tele::destination_answers,
                        launchpad::remove_answers,
                    )
                        .in_set(UiSystems::Handle)
                        .after(ss13_tgui::prompt::PromptSystems::Answer),
                    (machine_window_status, hand_tele::locator_status)
                        .in_set(UiSystems::StatusHook),
                    (
                        bumps,
                        teleporter::finish_calibrations,
                        teleporter::hub_panels,
                        hand_tele::expire_portals,
                        launchpad::finish_teleports,
                        gateway::process_gateways,
                        sat_control::assign_ids,
                        refresh_views
                            .after(UiSystems::Handle)
                            .before(UiSystems::Send),
                    ),
                ),
            );
    }
}

/// `attack_hand` -> `interact` of the machines here. True when the click was theirs.
fn attack_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    teleporter::attack_hand(world, client, mob, target)
        || launchpad::attack_hand(world, client, mob, target)
        || sat_control::attack_hand(world, client, mob, target)
        || gateway::attack_hand(world, client, mob, target)
}

/// `multitool_act` of the machines here (not in combat mode).
fn multitool_act(
    world: &mut World,
    client: Entity,
    mob: Entity,
    tool: Entity,
    target: Entity,
) -> bool {
    let combat = world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0);
    if combat || tool_of(world, tool).map(|t| t.0) != Some(ToolKind::Multitool) {
        return false;
    }
    teleporter::multitool_act(world, client, tool, target)
        || launchpad::multitool_act(world, client, tool, target)
        || gateway::multitool_act(world, client, target)
}

/// `Bumped` of teleporter hubs, portals and active gateways.
fn bumps(world: &mut World, mut cursor: Local<MessageCursor<Bumped>>) {
    let pending: Vec<(Entity, Entity)> = cursor
        .read(world.resource::<Messages<Bumped>>())
        .map(|b| (b.entity, b.obstacle))
        .collect();
    for (mover, obstacle) in pending {
        if world.get_entity(mover).is_err() || world.get_entity(obstacle).is_err() {
            continue;
        }
        let _ = teleporter::bumped(world, mover, obstacle)
            || hand_tele::bumped(world, mover, obstacle)
            || gateway::bumped(world, mover, obstacle);
    }
}

/// The open windows follow the world (`ui_data` on every update).
fn refresh_views(world: &mut World) {
    refresh_open(world, TeleporterUi::NAME, teleporter::view);
    refresh_open(
        world,
        super::BluespaceLocatorUi::NAME,
        hand_tele::locator_view,
    );
    refresh_open(world, LaunchpadConsoleUi::NAME, launchpad::view);
    refresh_open(world, SatelliteControlUi::NAME, sat_control::view);
    refresh_open(world, GatewayUi::NAME, gateway::view);
}

/// The consoles' windows go inert while unpowered (`is_operational`).
fn machine_window_status(mut windows: Query<&mut UiWindow>, unpowered: Query<(), With<Unpowered>>) {
    const NAMES: [&str; 4] = [
        TeleporterUi::NAME,
        LaunchpadConsoleUi::NAME,
        SatelliteControlUi::NAME,
        GatewayUi::NAME,
    ];
    for mut window in &mut windows {
        if NAMES.contains(&window.interface) && unpowered.contains(window.source) {
            window.cap_status(UiStatus::Disabled);
        }
    }
}

/// Recomputes the view component `C` of every source with an open `name` window.
pub(super) fn refresh_open<C: Component + PartialEq>(
    world: &mut World,
    name: &str,
    view: fn(&mut World, Entity) -> Option<C>,
) {
    let mut windows = world.query::<&UiWindow>();
    let mut sources: Vec<Entity> = windows
        .iter(world)
        .filter(|w| w.interface == name)
        .map(|w| w.source)
        .collect();
    if sources.is_empty() {
        return;
    }
    sources.sort_unstable();
    sources.dedup();
    for e in sources {
        if world.get_entity(e).is_err() {
            continue;
        }
        let Some(v) = view(world, e) else {
            continue;
        };
        if world.get::<C>(e) != Some(&v) {
            world.entity_mut(e).insert(v);
        }
    }
}

/// `ui_interact`: the view first (the window's data source), then the window.
pub(super) fn open<T: ServerUi, C: Component>(
    world: &mut World,
    client: Entity,
    actor: Entity,
    source: Entity,
    view: C,
) {
    world.entity_mut(source).insert(view);
    let title = display_name(world, source);
    world.write_message(OpenUi::new::<T>(client, actor, source, title));
}

/// The DM type of an object.
pub(super) fn obj_path(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmObj>(e).map(|o| o.path.as_str())
}

pub(super) fn is_obj(world: &World, e: Entity, root: &str) -> bool {
    obj_path(world, e).is_some_and(|p| is_under(p, root))
}

/// tg `locate(type, get_step(src, direction))` over `GLOB.cardinals`: the first match.
pub(super) fn adjacent(world: &World, of: Entity, root: &str) -> Option<Entity> {
    let &pos = world.get::<TilePos>(of)?;
    let index = world.resource::<SpatialIndex>();
    Dir::CARDINALS.iter().find_map(|&d| {
        index
            .on_tile(pos.step(d))
            .iter()
            .copied()
            .find(|&e| is_obj(world, e, root))
    })
}

pub(super) fn broken(world: &World, e: Entity) -> bool {
    world
        .get::<crate::obj_damage::Integrity>(e)
        .is_some_and(|i| i.broken)
}

/// `!(machine_stat & (BROKEN|NOPOWER))`.
pub(super) fn operational(world: &World, e: Entity) -> bool {
    world.get::<Unpowered>(e).is_none() && !broken(world, e)
}

/// `active_power_usage`, joules per use.
pub(super) fn active_power(world: &World, e: Entity) -> f64 {
    world
        .get::<DmPower>(e)
        .map(|p| p.active_power_usage)
        .filter(|&p| p > 0.0)
        .unwrap_or(BASE_ACTIVE)
}

/// `use_energy(active_power_usage)`.
pub(super) fn use_active_energy(world: &mut World, e: Entity) {
    let joules = active_power(world, e);
    crate::power::machine_use_energy(world, e, joules, false);
}

/// The name without `\improper`.
pub(super) fn display_name(world: &World, e: Entity) -> String {
    world
        .get::<DmName>(e)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default()
}

/// `get_area(atom)` as text (`format_text(area.name)`).
pub(super) fn area_name_at(world: &World, pos: TilePos) -> String {
    world
        .get_resource::<ss13_mapload::Areas>()
        .and_then(|a| a.area_at(pos))
        .map(|a| crate::shuttle::area_name(world, a))
        .unwrap_or_default()
}

/// tg `avoid_assoc_duplicate_keys`: the second `X` is `X (2)`.
pub(super) fn unique_key(key: String, used: &mut Vec<(String, u32)>) -> String {
    if let Some((_, n)) = used.iter_mut().find(|(k, _)| *k == key) {
        *n += 1;
        return format!("{key} ({n})");
    }
    used.push((key.clone(), 1));
    key
}

/// A visible message from `pos`.
pub(super) fn visible(world: &mut World, pos: TilePos, span: Span, text: &str) {
    for message in
        crate::pull::visible_message(world, pos, ChatChannel::Local, (span, text.to_owned()), &[])
    {
        world.write_message(message);
    }
}

/// `can_perform_action(src)` after a prompt: the user conscious, able and next to `source`.
pub(super) fn can_reach(world: &mut World, mob: Entity, source: Entity) -> bool {
    let near = match (
        world.get::<TilePos>(mob),
        crate::explosions::turf_of(world, source),
    ) {
        (Some(&a), Some(b)) => a.chebyshev(b).is_some_and(|d| d <= 1),
        _ => false,
    };
    near && world.get::<crate::health::Stat>(mob) == Some(&crate::health::Stat::Conscious)
        && !crate::status_effects::incapacitation(world, mob).incapacitated
}

pub(super) fn sort_by_key(items: &mut [(String, Entity)]) {
    items.sort_by(|a, b| a.0.cmp(&b.0));
}
