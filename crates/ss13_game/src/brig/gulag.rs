//! The labor camp teleporter (server) after tg `gulag_teleporter.dm`
//! (`/obj/machinery/gulag_teleporter`, `/obj/structure/gulag_beacon`),
//! `computer/prisoner/gulag_teleporter.dm` (tgui `GulagTeleporterConsole`) and
//! `gulag_item_reclaimer.dm` (tgui `GulagItemReclaimer`).
//!
//! - The teleporter is a pod ([`crate::medical::pods::Pod`]): an empty hand opens or closes it
//!   unless it is locked; a step from inside opens an unlocked one ("door won't budge" at most
//!   every 5 s when locked).
//! - The console (a prisoner console: a prisoner ID goes in from the hand or the window) links
//!   to a working teleporter on a cardinal tile and to the beacon; every act needs
//!   `ACCESS_BRIG`. "Process Prisoner" half a second later strips the human occupant of all but
//!   implants, internals and plasmaman gear into the reclaimer (handcuffs and everything without
//!   a reclaimer onto the floor), dresses them in the prisoner jumpsuit and orange sneakers with
//!   the ID (goal 200 when unset), sets their crew record to prisoner and teleports them onto
//!   the beacon, paralyzed 4 s; the teleporter unlocks and opens.
//! - The reclaimer lists every processed prisoner; with `ACCESS_BRIG`, or as the prisoner whose
//!   own prisoner ID reached its goal, "Retrieve Items" drops them under the reclaimer. A
//!   prisoner who no longer exists has their items dropped.
// shortcut: no breaking out of a locked teleporter (tg's 60 s `do_after`), no screwdriver or
// crowbar acts, no jumpskirt preference, no plasmaman gloves, no logs, no power use, no
// reclaimer emag; the jumpsuit and sneakers that do not fit are deleted (tg `qdel_on_fail`).
// Deviation: tg lets any prisoner `release_items` their own items through a forged act; here
// the prisoner needs the goal the window requires.

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::construct::DmObj;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmName};
use ss13_net::server::Networked;
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;
use ss13_shared::{Dir, TilePos};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::prisoner::{PrisonerCard, PrisonerConsole, id_eject, id_insert};
use super::*;
use crate::closets::{ClosetRelayMove, InCloset};
use crate::health::{Health, Stat};
use crate::inventory::{ALL_SLOTS, CarriedBy, IdCard, IdCards, Inventory};
use crate::medical::pods::{Pod, close_pod, occupant, open_pod, operational, panel_open};
use crate::sound::{PlaySound, playsound};
use crate::storage::StoredIn;
use crate::tools::tell;

pub const TELEPORTER: &str = "/obj/machinery/gulag_teleporter";
pub const BEACON: &str = "/obj/structure/gulag_beacon";
pub const CONSOLE: &str = "/obj/machinery/computer/prisoner/gulag_teleporter_computer";
pub const RECLAIMER: &str = "/obj/machinery/gulag_item_reclaimer";
pub const JUMPSUIT: &str = "/obj/item/clothing/under/rank/prisoner";
pub const SHOES: &str = "/obj/item/clothing/shoes/sneakers/orange";
const HANDCUFFS: &str = "/obj/item/restraints/handcuffs";
/// `req_access = list(ACCESS_BRIG)` of the console and the reclaimer.
const ACCESS_BRIG: &str = "brig";
/// `default_goal`.
pub const DEFAULT_GOAL: u32 = 200;
/// `clamp(new_goal, 0, 1000)`.
const MAX_GOAL: f64 = 1000.0;
/// `addtimer(teleport, 0.5 SECONDS)`.
const TELEPORT_DELAY: f64 = 0.5;
/// `Paralyze(40)`.
const TRAVEL_PARALYZE: f32 = 4.0;
/// `message_cooldown = world.time + 50`.
const MESSAGE_COOLDOWN: f64 = 5.0;
/// `telegulag_required_items`: what stays on the prisoner.
const REQUIRED_ITEMS: [&str; 8] = [
    "/obj/item/implant",
    "/obj/item/clothing/suit/space/eva/plasmaman",
    "/obj/item/clothing/under/plasmaman",
    "/obj/item/clothing/head/helmet/space/plasmaman",
    "/obj/item/clothing/gloves/color/plasmaman",
    "/obj/item/tank/internals",
    "/obj/item/clothing/mask/breath",
    "/obj/item/clothing/mask/gas",
];
const CONFIRM_SOUND: &str = "sound/machines/terminal/terminal_prompt_confirm.ogg";
const EMITTER_SOUND: &str = "sound/items/weapons/emitter.ogg";
/// `SFX_PORTAL_ENTER`.
const PORTAL_SOUND: &str = "portal_enter";

/// `/obj/machinery/gulag_teleporter`: `locked`, `linked_reclaimer`, `message_cooldown`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct GulagTeleporter {
    pub locked: bool,
    pub reclaimer: Option<Entity>,
    pub message_cooldown: f64,
}

/// `/obj/structure/gulag_beacon`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct GulagBeacon;

/// `/obj/machinery/computer/prisoner/gulag_teleporter_computer` (its ID slot is the
/// [`PrisonerConsole`]'s `contained_id`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct GulagConsole {
    pub teleporter: Option<Entity>,
    pub beacon: Option<Entity>,
    /// The `teleport` timer: when, and who pressed it.
    pub pending: Option<(f64, Entity)>,
    view: GulagTeleporterData,
}

/// A processed prisoner's items in the reclaimer (`stored_items[victim]`).
#[derive(Clone, Debug, PartialEq)]
pub struct StoredSet {
    pub mob: Entity,
    pub name: String,
    pub items: Vec<Entity>,
}

/// `/obj/machinery/gulag_item_reclaimer`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct GulagReclaimer {
    pub stored: Vec<StoredSet>,
}

pub struct GulagPlugin;

impl Plugin for GulagPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `interact` of the teleporter, `ui_interact` of the console and the reclaimer.
        app.add_click_handler(ClickPhase::EmptyHand, 2685, |w, c| {
            crate::brig::gulag::interact(w, c.client, c.mob, c.target)
        });
        app.register_ui::<GulagTeleporterUi>()
            .register_ui::<GulagItemReclaimerUi>()
            .add_observer(attach)
            .add_systems(ss13_world::MapInitLate, link)
            .add_systems(
                Update,
                (
                    console_acts.in_set(UiSystems::Handle),
                    reclaimer_acts.in_set(UiSystems::Handle),
                    refresh_views
                        .after(UiSystems::Handle)
                        .before(UiSystems::Send)
                        .run_if(any_with_component::<GulagConsole>),
                    teleport_timers.run_if(any_with_component::<GulagConsole>),
                    relay_moves
                        .after(ss13_world::WorldSystems::Movement)
                        .run_if(any_with_component::<GulagTeleporter>),
                    forget_lost_prisoners.run_if(any_with_component::<GulagReclaimer>),
                ),
            );
    }
}

fn attach(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(o) = objs.get(add.entity) else {
        return;
    };
    let e = add.entity;
    if is_under(&o.path, TELEPORTER) {
        commands
            .entity(e)
            .insert_if_new((Pod { open: false }, GulagTeleporter::default()));
        commands.queue(move |world: &mut World| refresh_look(world, e));
    } else if is_under(&o.path, BEACON) {
        commands.entity(e).insert(GulagBeacon);
    } else if is_under(&o.path, CONSOLE) {
        commands
            .entity(e)
            .insert((PrisonerConsole::default(), GulagConsole::default()));
    } else if is_under(&o.path, RECLAIMER) {
        commands.entity(e).insert(GulagReclaimer::default());
    }
}

/// `Initialize`: the teleporters' `locate_reclaimer`, the consoles' `scan_machinery`.
fn link(world: &mut World) {
    let reclaimer = {
        let mut q = world.query_filtered::<Entity, With<GulagReclaimer>>();
        q.iter(world).next()
    };
    let mut q = world.query::<&mut GulagTeleporter>();
    for mut t in q.iter_mut(world) {
        if t.reclaimer.is_none() {
            t.reclaimer = reclaimer;
        }
    }
    let mut q = world.query_filtered::<Entity, With<GulagConsole>>();
    let consoles: Vec<Entity> = q.iter(world).collect();
    for console in consoles {
        let teleporter = find_teleporter(world, console);
        let beacon = find_beacon(world);
        if let Some(mut c) = world.get_mut::<GulagConsole>(console) {
            c.teleporter = teleporter;
            c.beacon = beacon;
        }
    }
}

/// `findteleporter`: a working teleporter on a cardinal tile.
pub fn find_teleporter(world: &mut World, console: Entity) -> Option<Entity> {
    let &pos = world.get::<TilePos>(console)?;
    let found: Vec<Entity> = Dir::CARDINALS
        .into_iter()
        .flat_map(|d| {
            let at = pos.step(d);
            world
                .resource::<ss13_world::SpatialIndex>()
                .on_tile(at)
                .to_vec()
        })
        .collect();
    found
        .into_iter()
        .find(|&e| world.get::<GulagTeleporter>(e).is_some() && operational(world, e))
}

/// `findbeacon`: `locate(/obj/structure/gulag_beacon)`, the first anywhere.
pub fn find_beacon(world: &mut World) -> Option<Entity> {
    let mut q = world.query_filtered::<Entity, (With<GulagBeacon>, With<TilePos>)>();
    q.iter(world).next()
}

/// `update_icon_state` of the teleporter.
fn refresh_look(world: &mut World, e: Entity) {
    let open = world.get::<Pod>(e).is_some_and(|p| p.open);
    let mut state = String::from("implantchair");
    if open {
        state.push_str("_open");
    }
    if !operational(world, e) {
        state.push_str("_unpowered");
        if panel_open(world, e) {
            state.push_str("_maintenance");
        }
    } else if panel_open(world, e) {
        state.push_str("_maintenance");
    } else if occupant(world, e).is_some() {
        state.push_str("_occupied");
    }
    crate::obj_damage::set_icon_state(world, e, &state);
}

fn sound(world: &mut World, at: Entity, file: &str, vary: bool) {
    if let Some(&pos) = world.get::<TilePos>(at) {
        playsound(world, PlaySound::new(pos, file, 50.0, vary));
    }
}

fn access_of(world: &World, user: Entity) -> Vec<String> {
    world
        .get::<Inventory>(user)
        .and_then(|i| i.id_card(world))
        .and_then(|c| world.get::<IdCard>(c))
        .map(|c| c.access.clone())
        .unwrap_or_default()
}

/// `allowed(user)` with `ACCESS_BRIG`.
fn allowed(world: &World, user: Entity) -> bool {
    crate::inventory::check_access(&[ACCESS_BRIG.to_owned()], &[], &access_of(world, user))
}

/// `real_name`.
fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<Name>(e)
        .map(|n| n.to_string())
        .unwrap_or_else(|| crate::examine::name_of(world, e))
}

fn is_human(world: &World, mob: Entity) -> bool {
    world.get::<Inventory>(mob).is_some()
        && world.get::<crate::medical::wounds::Limbs>(mob).is_some()
}

/// The empty-hand click on the brig's labor camp machines. True when `target` is one.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if let Some(t) = world.get::<GulagTeleporter>(target).copied() {
        if t.locked {
            let name = crate::examine::name_of(world, target);
            tell(
                world,
                client,
                Span::Warning,
                format!("{name} заблокирован!"),
            );
        } else {
            toggle_open(world, Some(client), target);
        }
        return true;
    }
    let window = if world.get::<GulagConsole>(target).is_some() {
        Some(true)
    } else if world.get::<GulagReclaimer>(target).is_some() {
        Some(false)
    } else {
        None
    };
    let Some(console) = window else {
        return false;
    };
    if world.get::<crate::power::Unpowered>(target).is_some() {
        return true;
    }
    let title = world
        .get::<DmName>(target)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    if console {
        world.write_message(OpenUi::new::<GulagTeleporterUi>(client, mob, target, title));
    } else {
        world.write_message(OpenUi::new::<GulagItemReclaimerUi>(
            client, mob, target, title,
        ));
    }
    true
}

/// `toggle_open`: an open teleporter closes on whoever stands in it; a closed, unlocked one
/// opens.
pub fn toggle_open(world: &mut World, client: Option<Entity>, teleporter: Entity) {
    if panel_open(world, teleporter) {
        if let Some(client) = client {
            tell(
                world,
                client,
                Span::Notice,
                "Сначала закройте панель техобслуживания.",
            );
        }
        return;
    }
    let Some(t) = world.get::<GulagTeleporter>(teleporter).copied() else {
        return;
    };
    if world.get::<Pod>(teleporter).is_some_and(|p| p.open) {
        close_pod(world, teleporter, None);
    } else if !t.locked {
        open_pod(world, teleporter);
    }
    refresh_look(world, teleporter);
}

/// `relaymove`: a step from inside opens an unlocked teleporter.
fn relay_moves(world: &mut World, mut moves: Local<MessageCursor<ClosetRelayMove>>) {
    let mobs: Vec<Entity> = moves
        .read(world.resource::<Messages<ClosetRelayMove>>())
        .map(|m| m.mob)
        .collect();
    for mob in mobs {
        if world.get::<Stat>(mob) != Some(&Stat::Conscious) {
            continue;
        }
        let Some(pod) = world.get::<InCloset>(mob).map(|c| c.closet) else {
            continue;
        };
        let Some(t) = world.get::<GulagTeleporter>(pod).copied() else {
            continue;
        };
        if world.get::<Pod>(pod).is_none_or(|p| p.open) {
            continue;
        }
        if t.locked {
            let now = world.resource::<Time>().elapsed_secs_f64();
            if t.message_cooldown <= now {
                if let Some(mut t) = world.get_mut::<GulagTeleporter>(pod) {
                    t.message_cooldown = now + MESSAGE_COOLDOWN;
                }
                if let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) {
                    let name = crate::examine::name_of(world, pod);
                    tell(
                        world,
                        client,
                        Span::Warning,
                        format!("Дверь {name} не поддаётся!"),
                    );
                }
            }
            continue;
        }
        open_pod(world, pod);
        refresh_look(world, pod);
    }
}

impl ServerUi for GulagTeleporterUi {
    type Source = &'static GulagConsole;
    type UserParam = ();
    type Act = GulagAct;

    fn static_data(_: &GulagConsole) {}

    fn data(console: &GulagConsole, _: Entity, _: &SystemParamItem<'_, '_, ()>) -> Self::Data {
        console.view.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<GulagAct, UiReject> {
        Ok(match action {
            SCAN_TELEPORTER => GulagAct::ScanTeleporter,
            SCAN_BEACON => GulagAct::ScanBeacon,
            HANDLE_ID => GulagAct::HandleId,
            SET_GOAL => GulagAct::SetGoal(
                params
                    .get("value")
                    .and_then(|v| v.parse::<f64>().ok())
                    .filter(|v| v.is_finite())
                    .ok_or_else(|| UiReject::BadParams("value: expected a number".into()))?,
            ),
            TOGGLE_OPEN => GulagAct::ToggleOpen,
            TELEPORTER_LOCK => GulagAct::Lock,
            TELEPORT => GulagAct::Teleport,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

/// A parsed `GulagTeleporterConsole` act.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GulagAct {
    ScanTeleporter,
    ScanBeacon,
    HandleId,
    SetGoal(f64),
    ToggleOpen,
    Lock,
    Teleport,
}

fn console_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<GulagTeleporterUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, GulagAct)> = acts
        .read(world.resource::<Messages<UiActed<GulagTeleporterUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act))
        .collect();
    for (client, actor, console, act) in pending {
        console_act(world, client, actor, console, act);
    }
}

/// The goal `set_goal` stores: 0 is the default goal, the rest clamped to 0..1000.
pub fn goal_of(value: f64) -> u32 {
    if value as i64 == 0 {
        DEFAULT_GOAL
    } else {
        value.clamp(0.0, MAX_GOAL) as u32
    }
}

/// `gulag_teleporter_computer/ui_act` at the moment of effect.
pub fn console_act(
    world: &mut World,
    client: Entity,
    user: Entity,
    console: Entity,
    act: GulagAct,
) {
    let Some(state) = world.get::<GulagConsole>(console).cloned() else {
        return;
    };
    if world.get::<crate::power::Unpowered>(console).is_some() {
        return;
    }
    if world.get::<Health>(user).is_some() {
        sound(world, console, CONFIRM_SOUND, false);
    }
    if !allowed(world, user) {
        tell(world, client, Span::Warning, "Доступ запрещён.");
        return;
    }
    let card = world
        .get::<PrisonerConsole>(console)
        .and_then(|c| c.contained_id);
    let teleporter = state
        .teleporter
        .filter(|&t| world.get::<GulagTeleporter>(t).is_some());
    match act {
        GulagAct::ScanTeleporter => {
            let found = find_teleporter(world, console);
            if let Some(mut c) = world.get_mut::<GulagConsole>(console) {
                c.teleporter = found;
            }
        }
        GulagAct::ScanBeacon => {
            let found = find_beacon(world);
            if let Some(mut c) = world.get_mut::<GulagConsole>(console) {
                c.beacon = found;
            }
        }
        GulagAct::HandleId => match card {
            Some(_) => id_eject(world, client, user, console),
            None => {
                let held = world
                    .get::<Inventory>(user)
                    .and_then(Inventory::active_item);
                if let Some(held) = held {
                    id_insert(world, client, user, console, held);
                }
            }
        },
        GulagAct::SetGoal(value) => {
            if let Some(mut c) = card.and_then(|c| world.get_mut::<PrisonerCard>(c)) {
                c.goal = goal_of(value);
            }
        }
        GulagAct::ToggleOpen => {
            let Some(t) = teleporter else { return };
            if world.get::<GulagTeleporter>(t).is_some_and(|t| t.locked) {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Сначала нужно разблокировать телепорт.",
                );
                return;
            }
            toggle_open(world, Some(client), t);
        }
        GulagAct::Lock => {
            let Some(t) = teleporter else { return };
            if world.get::<Pod>(t).is_some_and(|p| p.open) {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Сначала нужно закрыть телепорт.",
                );
                return;
            }
            if let Some(mut t) = world.get_mut::<GulagTeleporter>(t) {
                t.locked = !t.locked;
            }
        }
        GulagAct::Teleport => {
            if teleporter.is_none() || state.beacon.is_none() {
                return;
            }
            let at = world.resource::<Time>().elapsed_secs_f64() + TELEPORT_DELAY;
            if let Some(mut c) = world.get_mut::<GulagConsole>(console) {
                c.pending = Some((at, user));
            }
        }
    }
}

fn teleport_timers(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &GulagConsole)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, c)| c.pending.is_some_and(|(at, _)| at <= now))
        .map(|(e, _)| e)
        .collect();
    for console in due {
        if let Some(mut c) = world.get_mut::<GulagConsole>(console) {
            c.pending = None;
        }
        teleport(world, console);
    }
}

/// `teleport(user)`, re-checking the card, the teleporter, the beacon and the occupant.
pub fn teleport(world: &mut World, console: Entity) {
    let Some(state) = world.get::<GulagConsole>(console).cloned() else {
        return;
    };
    let card = world
        .get::<PrisonerConsole>(console)
        .and_then(|c| c.contained_id)
        .filter(|&c| {
            world.get::<StoredIn>(c).is_some_and(|s| s.0 == console)
                && world.get::<PrisonerCard>(c).is_some()
        });
    let Some(card) = card else {
        crate::cargo::console::machine_say(
            world,
            console,
            "Warning: Unable to transfer prisoner without a valid Prisoner ID inserted!",
        );
        return;
    };
    let teleporter = state
        .teleporter
        .filter(|&t| world.get::<GulagTeleporter>(t).is_some());
    let beacon_pos = state
        .beacon
        .filter(|&b| world.get::<GulagBeacon>(b).is_some())
        .and_then(|b| world.get::<TilePos>(b).copied());
    let (Some(teleporter), Some(beacon_pos)) = (teleporter, beacon_pos) else {
        return;
    };
    let Some(prisoner) = occupant(world, teleporter).filter(|&m| is_human(world, m)) else {
        return;
    };
    if world.get::<PrisonerCard>(card).is_some_and(|c| c.goal == 0) {
        if let Some(mut c) = world.get_mut::<PrisonerCard>(card) {
            c.goal = DEFAULT_GOAL;
        }
        let name = crate::examine::name_of(world, card);
        crate::cargo::console::machine_say(
            world,
            console,
            &format!("Цель ID-карты {name} по умолчанию — {DEFAULT_GOAL} очков."),
        );
    }
    if let Some(mut c) = world.get_mut::<PrisonerConsole>(console) {
        c.contained_id = None;
    }
    handle_prisoner(world, teleporter, prisoner, card);
    sound(world, console, EMITTER_SOUND, true);
    crate::closets::let_out(world, prisoner);
    let tele = crate::teleport::Tele {
        forced: true,
        no_effects: true,
        ..default()
    };
    crate::teleport::do_teleport(world, prisoner, beacon_pos, tele);
    crate::status_effects::apply(
        world,
        prisoner,
        crate::status_effects::Incap::Paralyze,
        TRAVEL_PARALYZE,
    );
    if let Some(client) = world
        .get::<crate::server::PlayerMob>(prisoner)
        .map(|p| p.client)
    {
        tell(
            world,
            client,
            Span::Warning,
            "От телепортации у вас слегка кружится голова.",
        );
    }
    crate::emp::sparks(world, prisoner);
    sound(world, console, PORTAL_SOUND, true);
    if let Some(mut t) = world.get_mut::<GulagTeleporter>(teleporter) {
        t.locked = false;
    }
    toggle_open(world, None, teleporter);
}

/// `handle_prisoner(id, record)`: strip, dress, the ID on, the record to prisoner.
fn handle_prisoner(world: &mut World, teleporter: Entity, prisoner: Entity, card: Entity) {
    strip_prisoner(world, teleporter, prisoner);
    for (path, slot) in [
        (JUMPSUIT, InventorySlot::Uniform),
        (SHOES, InventorySlot::Shoes),
    ] {
        if let Some(item) = crate::construction::spawn_detached(world, path)
            && !equip(world, prisoner, item, &[slot])
        {
            world.despawn(item);
        }
    }
    world.entity_mut(card).remove::<StoredIn>();
    let slots = [
        InventorySlot::Id,
        InventorySlot::LeftHand,
        InventorySlot::RightHand,
    ];
    if !equip(world, prisoner, card, &slots)
        && let Some(&pos) = world.get::<TilePos>(teleporter)
    {
        crate::storage::take_out(world, card, pos);
    }
    let name = name_of(world, prisoner);
    if let Some(mut manifest) = world.get_resource_mut::<crate::records::manifest::Manifest>()
        && let Some(record) = manifest.general.iter_mut().find(|r| r.name == name)
    {
        record.wanted_status = crate::records::WANTED_PRISONER.to_owned();
    }
    crate::inventory::refresh_look(world, prisoner);
}

/// `equip_to_appropriate_slot` over `slots`: true when `item` was put on.
fn equip(world: &mut World, mob: Entity, item: Entity, slots: &[InventorySlot]) -> bool {
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return false;
    };
    let hand = |s: InventorySlot| matches!(s, InventorySlot::LeftHand | InventorySlot::RightHand);
    let Some(slot) = slots.iter().copied().find(|&s| {
        if hand(s) {
            inventory.slot(s).is_none()
        } else {
            crate::inventory::can_equip(world, mob, &inventory, item, s)
        }
    }) else {
        return false;
    };
    world.entity_mut(item).insert((Networked, CarriedBy(mob)));
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        *inventory.slot_mut(slot) = Some(item);
    }
    true
}

/// `strip_prisoner`: everything but the required items into the reclaimer, handcuffs and
/// everything without a reclaimer onto the teleporter's tile.
fn strip_prisoner(world: &mut World, teleporter: Entity, victim: Entity) {
    let Some(&pos) = world.get::<TilePos>(teleporter) else {
        return;
    };
    let reclaimer = world
        .get::<GulagTeleporter>(teleporter)
        .and_then(|t| t.reclaimer)
        .filter(|&r| world.get::<GulagReclaimer>(r).is_some());
    let name = name_of(world, victim);
    let mut stored = Vec::new();
    crate::restraints::uncuff(world, victim, pos);
    for slot in ALL_SLOTS {
        let Some(item) = world.get::<Inventory>(victim).and_then(|i| i.slot(slot)) else {
            continue;
        };
        let path = world
            .get::<DmItem>(item)
            .map(|d| d.path.clone())
            .unwrap_or_default();
        if REQUIRED_ITEMS.iter().any(|r| is_under(&path, r)) {
            continue;
        }
        if let Some(mut inventory) = world.get_mut::<Inventory>(victim) {
            *inventory.slot_mut(slot) = None;
        }
        match reclaimer.filter(|_| !is_under(&path, HANDCUFFS)) {
            Some(r) => {
                crate::storage::put_inside(world, item, r);
                stored.push(item);
            }
            None => crate::inventory::put_down(world, item, pos),
        }
    }
    if let Some(mut r) = reclaimer.and_then(|r| world.get_mut::<GulagReclaimer>(r)) {
        r.stored.retain(|s| s.mob != victim);
        r.stored.push(StoredSet {
            mob: victim,
            name,
            items: stored,
        });
    }
}

/// The consoles' `ui_data`, kept on them.
fn refresh_views(world: &mut World) {
    let mut q = world.query_filtered::<Entity, With<GulagConsole>>();
    let consoles: Vec<Entity> = q.iter(world).collect();
    for console in consoles {
        let Some(state) = world.get::<GulagConsole>(console).cloned() else {
            continue;
        };
        let teleporter = state
            .teleporter
            .filter(|&t| world.get::<GulagTeleporter>(t).is_some());
        let beacon = state
            .beacon
            .filter(|&b| world.get::<GulagBeacon>(b).is_some());
        let card = world
            .get::<PrisonerConsole>(console)
            .and_then(|c| c.contained_id)
            .filter(|&c| world.get::<StoredIn>(c).is_some_and(|s| s.0 == console));
        let prisoner = teleporter
            .and_then(|t| occupant(world, t))
            .filter(|&m| is_human(world, m));
        let location = |world: &World, e: Entity| {
            world
                .get::<TilePos>(e)
                .map(|p| format!("({}, {}, {})", p.x, p.y, p.z))
        };
        let prisoner_name = prisoner.map(|m| name_of(world, m));
        let prisoner_crimstat = prisoner_name.as_ref().and_then(|name| {
            world
                .get_resource::<crate::records::manifest::Manifest>()
                .and_then(|m| m.find(name))
                .map(|r| r.wanted_status.clone())
        });
        let view = GulagTeleporterData {
            can_teleport: prisoner.is_some() && card.is_some(),
            prisoner_name,
            prisoner_crimstat,
            teleporter_location: teleporter.and_then(|t| location(world, t)),
            teleporter_lock: teleporter
                .and_then(|t| world.get::<GulagTeleporter>(t))
                .is_some_and(|t| t.locked),
            teleporter_state_open: teleporter
                .and_then(|t| world.get::<Pod>(t))
                .is_some_and(|p| p.open),
            beacon_location: beacon.and_then(|b| location(world, b)),
            id_name: card.map(|c| {
                world
                    .get::<crate::outfit::IdRegistration>(c)
                    .map(|r| r.registered_name.clone())
                    .unwrap_or_default()
            }),
            goal: card
                .and_then(|c| world.get::<PrisonerCard>(c))
                .map_or(0, |c| c.goal),
        };
        if let Some(mut c) = world.get_mut::<GulagConsole>(console)
            && c.view != view
        {
            c.view = view;
        }
    }
}

/// `ui_data` of the reclaimer: a prisoner that no longer exists has their items dropped, items
/// that left the reclaimer are forgotten (`Exited`).
fn forget_lost_prisoners(world: &mut World) {
    let mut q = world.query::<(Entity, &GulagReclaimer)>();
    let reclaimers: Vec<(Entity, Vec<Entity>)> = q
        .iter(world)
        .map(|(e, r)| (e, r.stored.iter().map(|s| s.mob).collect()))
        .collect();
    for (reclaimer, mobs) in reclaimers {
        for mob in mobs {
            if world.get_entity(mob).is_err() {
                crate::cargo::console::machine_say(
                    world,
                    reclaimer,
                    "Alert! Unable to locate vital signals of a previously processed prisoner. \
                     Ejecting equipment!",
                );
                drop_items(world, reclaimer, mob);
            }
        }
        let inside = |world: &World, item: Entity| {
            world
                .get::<StoredIn>(item)
                .is_some_and(|s| s.0 == reclaimer)
        };
        let Some(r) = world.get::<GulagReclaimer>(reclaimer) else {
            continue;
        };
        let gone = r
            .stored
            .iter()
            .any(|s| s.items.iter().any(|&i| !inside(world, i)));
        if gone {
            let kept: Vec<StoredSet> = r
                .stored
                .iter()
                .map(|s| StoredSet {
                    items: s
                        .items
                        .iter()
                        .copied()
                        .filter(|&i| inside(world, i))
                        .collect(),
                    ..s.clone()
                })
                .collect();
            if let Some(mut r) = world.get_mut::<GulagReclaimer>(reclaimer) {
                r.stored = kept;
            }
        }
    }
}

/// `drop_items(user)`: `mob`'s stored items onto the reclaimer's tile.
pub fn drop_items(world: &mut World, reclaimer: Entity, mob: Entity) {
    let Some(pos) = world.get::<TilePos>(reclaimer).copied() else {
        return;
    };
    let Some(set) = world
        .get_mut::<GulagReclaimer>(reclaimer)
        .and_then(|mut r| {
            let i = r.stored.iter().position(|s| s.mob == mob)?;
            Some(r.stored.remove(i))
        })
    else {
        return;
    };
    for item in set.items {
        if world
            .get::<StoredIn>(item)
            .is_some_and(|s| s.0 == reclaimer)
        {
            crate::storage::take_out(world, item, pos);
        }
    }
}

/// Reads for the reclaimer's `can_reclaim`: the user's IDs and prisoner cards.
type ReclaimUser = (
    Query<'static, 'static, &'static Inventory>,
    IdCards<'static, 'static>,
    Query<'static, 'static, &'static PrisonerCard>,
);

/// `can_reclaim`: brig access, or a prisoner ID (`get_idcard(TRUE)`) that reached its goal.
pub fn can_reclaim(access: Option<&[String]>, card: Option<&PrisonerCard>) -> bool {
    access.is_some_and(|a| crate::inventory::check_access(&[ACCESS_BRIG.to_owned()], &[], a))
        || card.is_some_and(|c| c.points >= c.goal)
}

impl ServerUi for GulagItemReclaimerUi {
    type Source = &'static GulagReclaimer;
    type UserParam = ReclaimUser;
    type Act = Entity;

    fn static_data(_: &GulagReclaimer) {}

    fn data(
        reclaimer: &GulagReclaimer,
        user: Entity,
        (inventories, cards, prisoner_cards): &SystemParamItem<'_, '_, ReclaimUser>,
    ) -> GulagItemReclaimerData {
        let inventory = inventories.get(user).ok();
        let access = inventory.and_then(|i| i.access(cards));
        let card = inventory
            .and_then(|i| i.id_search().find_map(|e| cards.id_of(e)))
            .and_then(|c| prisoner_cards.get(c).ok());
        GulagItemReclaimerData {
            can_reclaim: can_reclaim(access, card),
            mobs: reclaimer
                .stored
                .iter()
                .map(|s| (s.mob.to_bits().to_string(), s.name.clone()))
                .collect(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<Entity, UiReject> {
        if action != RELEASE_ITEMS {
            return Err(UiReject::UnknownAction);
        }
        params
            .get("mobref")
            .and_then(|v| v.parse::<u64>().ok())
            .and_then(Entity::try_from_bits)
            .ok_or_else(|| UiReject::BadParams("mobref: expected a ref".into()))
    }
}

fn reclaimer_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<GulagItemReclaimerUi>>>,
) {
    let pending: Vec<(Entity, Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<GulagItemReclaimerUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act))
        .collect();
    for (client, user, reclaimer, mob) in pending {
        release_items(world, client, user, reclaimer, mob);
    }
}

/// `release_items`: `mob` among the stored prisoners; the user has brig access, or is that
/// prisoner with their goal reached.
pub fn release_items(
    world: &mut World,
    client: Entity,
    user: Entity,
    reclaimer: Entity,
    mob: Entity,
) {
    if world.get::<crate::power::Unpowered>(reclaimer).is_some() {
        return;
    }
    let stored = world
        .get::<GulagReclaimer>(reclaimer)
        .is_some_and(|r| r.stored.iter().any(|s| s.mob == mob));
    if !stored {
        return;
    }
    let own = mob == user && {
        let card = world
            .get::<Inventory>(user)
            .and_then(|i| i.id_card(world))
            .and_then(|c| world.get::<PrisonerCard>(c));
        can_reclaim(None, card)
    };
    if !own && !allowed(world, user) {
        tell(world, client, Span::Warning, "Доступ запрещён.");
        return;
    }
    drop_items(world, reclaimer, mob);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goal_defaults_and_clamps() {
        assert_eq!(goal_of(0.0), DEFAULT_GOAL);
        assert_eq!(goal_of(0.4), DEFAULT_GOAL);
        assert_eq!(goal_of(350.0), 350);
        assert_eq!(goal_of(5000.0), 1000);
        assert_eq!(goal_of(-5.0), 0);
    }

    #[test]
    fn reclaiming_needs_access_or_a_finished_sentence() {
        let brig = vec!["brig".to_owned()];
        assert!(can_reclaim(Some(&brig), None));
        assert!(!can_reclaim(Some(&[]), None));
        let done = PrisonerCard {
            goal: 100,
            points: 100,
            ..Default::default()
        };
        let short = PrisonerCard {
            goal: 100,
            points: 99,
            ..Default::default()
        };
        assert!(can_reclaim(None, Some(&done)));
        assert!(!can_reclaim(None, Some(&short)));
    }
}
