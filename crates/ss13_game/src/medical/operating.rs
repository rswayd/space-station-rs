//! The operating computer (server), after tg `code/game/machinery/computer/operating_computer.dm`
//! (`/obj/machinery/computer/operating`), `surgery_disks.dm` (`/obj/item/disk/surgery`) and the
//! operating table's `get_surgeries` (`tables_racks.dm`).
//!
//! - The computer's `advanced_surgeries` are what its disks loaded plus the station techweb's
//!   researched surgery designs (`/datum/design/surgery`, tg links every computer to the science
//!   web at roundstart and follows its research). A patient lying on an operating table next to
//!   the computer (the first computer around the table, `alldirs` order) is offered those locked
//!   operations ([`super::surgery::pool_for`]), with or without power, as in tg.
//! - A surgery disk clicked on the computer loads after 1 s. An empty hand, or a surgical tool out
//!   of combat mode, opens the window: the patient on the table (state, blood, damage, the
//!   surgical state of the selected zone), the operations it is ready for next, the catalog.
//!   "change_zone" picks the zone the window shows and sets the user's zone selector.
//! - The window closes without power, for an unconscious user and for the patient on the table;
//!   it is read-only for anyone lying elsewhere, incapacitated or over 2 tiles away.
// shortcut: no experiments tab (autopsy experiments), no multitool relinking to another
// techweb, the zone selector is not reverted on close, and tgui's pinning, tool filter and
// search of the catalog are client-side only (not ported).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_tgui::server::{UiKind, UiWindow};
use ss13_tgui::{UiInterface, UiStatus};

use super::surgery::{ALLDIRS, Op, flags, is_available, operation_pool, readable_states};
use super::{MedJob, name_of, visible};
use crate::health::{BodyPosition, HEALTH_THRESHOLD_DEAD, Health, Stat, ZoneSelected};
use crate::obj_damage::is_under;
use crate::operating::{
    CHANGE_ZONE, NEW_ZONE, OperatingComputerData, OperatingComputerUi, OperationData, PatientData,
};
use crate::tools::{Actor, do_after, tell};
use crate::zone::TargetZone;

pub const COMPUTER: &str = "/obj/machinery/computer/operating";
pub const DISK: &str = "/obj/item/disk/surgery";
const OPTABLE: &str = "/obj/structure/table/optable";

/// `/obj/machinery/computer/operating`: the disks' operations and the zone its window shows.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct OperatingComputer {
    /// Operation type paths loaded from disks.
    pub disks: Vec<&'static str>,
    pub target_zone: TargetZone,
    /// The window's data, rebuilt while a window is open.
    pub view: OperatingComputerData,
}

/// The research designs that unlock ported operations (`/datum/design/surgery` `surgery`).
const DESIGNS: [(&str, &str); 5] = [
    (
        "/datum/design/surgery/tend_wounds_upgrade",
        "/datum/surgery_operation/basic/tend_wounds/upgraded",
    ),
    (
        "/datum/design/surgery/tend_wounds_upgrade/femto",
        "/datum/surgery_operation/basic/tend_wounds/upgraded/master",
    ),
    (
        "/datum/design/surgery/tend_wounds_combo",
        "/datum/surgery_operation/basic/tend_wounds/combo",
    ),
    (
        "/datum/design/surgery/tend_wounds_combo/upgrade",
        "/datum/surgery_operation/basic/tend_wounds/combo/upgraded",
    ),
    (
        "/datum/design/surgery/tend_wounds_combo/upgrade/femto",
        "/datum/surgery_operation/basic/tend_wounds/combo/upgraded/master",
    ),
];

/// A disk's `surgeries` (the debug disk: every operation). Types this port has no operation
/// for load too and are never offered.
pub fn disk_surgeries(path: &str) -> Vec<&'static str> {
    if is_under(path, "/obj/item/disk/surgery/debug") {
        return Op::types().into_iter().map(|op| op.def().path).collect();
    }
    let list: &[&'static str] = if is_under(path, "/obj/item/disk/surgery/advanced_plastic_surgery")
    {
        &["/datum/surgery_operation/limb/add_plastic"]
    } else if is_under(path, "/obj/item/disk/surgery/brainwashing") {
        &[
            "/datum/surgery_operation/organ/brainwash",
            "/datum/surgery_operation/organ/brainwash/mechanic",
        ]
    } else if is_under(path, "/obj/item/disk/surgery/sleeper_protocol") {
        &[
            "/datum/surgery_operation/organ/brainwash/sleeper",
            "/datum/surgery_operation/organ/brainwash/sleeper/mechanic",
        ]
    } else {
        &[]
    };
    list.to_vec()
}

/// `advanced_surgeries`: the disks' operations and the station's researched designs.
pub fn advanced_surgeries(world: &World, computer: Entity) -> Vec<&'static str> {
    let mut paths = world
        .get::<OperatingComputer>(computer)
        .map(|c| c.disks.clone())
        .unwrap_or_default();
    if let (Some(web), Some(db)) = (
        world.get_resource::<crate::science::techweb::Techweb>(),
        world.get_resource::<crate::science::techweb::ResearchDb>(),
    ) {
        for &design in &web.researched_designs {
            let Some(id) = db.designs.get(design).map(|d| d.id.as_str()) else {
                continue;
            };
            if let Some((_, op)) = DESIGNS.iter().find(|(d, _)| *d == id)
                && !paths.contains(op)
            {
                paths.push(op);
            }
        }
    }
    paths
}

fn add_computer(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    if objs
        .get(add.entity)
        .is_ok_and(|o| is_under(&o.path, COMPUTER))
    {
        commands
            .entity(add.entity)
            .insert(OperatingComputer::default());
    }
}

/// The computer's table (`find_table`: the first operating table around it).
pub fn table_of(world: &World, computer: Entity) -> Option<Entity> {
    let pos = *world.get::<TilePos>(computer)?;
    let index = world.resource::<ss13_world::SpatialIndex>();
    ALLDIRS.iter().find_map(|&(dx, dy)| {
        let tile = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
        index.on_tile(tile).iter().copied().find(|&e| {
            crate::construction::path_of(world, e).is_some_and(|p| is_under(&p, OPTABLE))
        })
    })
}

/// `table.patient`: a living mob lying on the table (`recheck_patient`).
pub fn patient_on(world: &World, table: Entity) -> Option<Entity> {
    let pos = *world.get::<TilePos>(table)?;
    world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| {
            world.get::<Health>(e).is_some()
                && world.get::<BodyPosition>(e).is_some_and(|b| b.lying())
        })
}

/// tg `get_surgery_state_as_list(target_zone)`.
fn surgery_state(world: &World, patient: Entity, zone: TargetZone) -> Vec<String> {
    let body = zone.check_zone();
    let Some(limbs) = world.get::<super::wounds::Limbs>(patient) else {
        return vec!["Часть тела отсуствует".into()];
    };
    let limb = limbs.limb(body);
    let mut list = Vec::new();
    if limb.prepped {
        list.push("Готов к операции".to_owned());
    }
    let covered = world
        .get::<crate::inventory::Inventory>(patient)
        .is_some_and(|inv| {
            inv.equipped()
                .filter(|&item| {
                    inv.slot_of(item) != Some(ss13_shared::protocol::InventorySlot::Neck)
                })
                .filter_map(|item| world.get::<ss13_mapload::DmClothing>(item))
                .any(|c| c.body_parts_covered & crate::chem::body_part_bit(body) != 0)
        });
    if covered {
        list.push("Часть тела закрыта одеждой".into());
    }
    list.extend(readable_states(limb.surgery).into_iter().map(str::to_owned));
    list
}

/// `ui_data` + `ui_static_data` of `computer` as `user` sees it.
pub fn view(world: &World, computer: Entity) -> OperatingComputerData {
    let Some(state) = world.get::<OperatingComputer>(computer) else {
        return OperatingComputerData::default();
    };
    let zone = state.target_zone;
    let table = table_of(world, computer);
    let patient = table.and_then(|t| patient_on(world, t));
    let powered = world.get::<crate::power::Unpowered>(computer).is_none();
    let mut data = OperatingComputerData {
        has_table: table.is_some(),
        target_zone: zone.dm().to_owned(),
        patient: patient.map(|p| patient_data(world, p, zone)),
        surgeries: Vec::new(),
    };
    if !powered {
        return data;
    }
    let pool = operation_pool(&advanced_surgeries(world, computer));
    let mut any_recommended = false;
    for op in Op::types() {
        // The catalog lists a type once; any of its options in the pool counts.
        let options: Vec<Op> = pool
            .iter()
            .copied()
            .filter(|o| o.def().path == op.def().path)
            .collect();
        if options.is_empty() {
            continue;
        }
        let next =
            patient.is_some_and(|p| options.iter().any(|&o| is_available(world, o, p, zone)));
        any_recommended |= next;
        let def = op.def();
        data.surgeries.push(OperationData {
            name: def.catalog_name.to_owned(),
            desc: def.desc.to_owned(),
            tool_rec: def.tool_rec.to_owned(),
            requirements: op.requirements().into(),
            show_as_next: next,
            show_in_list: true,
            priority: def.flags & flags::PRIORITY_NEXT_STEP != 0,
        });
    }
    let prepped = patient.is_some_and(|p| {
        world
            .get::<super::wounds::Limbs>(p)
            .is_some_and(|l| l.parts.iter().any(|part| part.prepped))
    });
    if !any_recommended && patient.is_some() && !prepped {
        data.surgeries.push(OperationData {
            name: "Подготовка к операции".into(),
            desc: "Начните операцию, постелив хирургическую простынь на пациента.".into(),
            tool_rec: "surgical drapes".into(),
            requirements: Vec::new(),
            show_as_next: true,
            show_in_list: false,
            priority: false,
        });
    }
    data
}

fn patient_data(world: &World, patient: Entity, zone: TargetZone) -> PatientData {
    let health = world.get::<Health>(patient).cloned().unwrap_or_default();
    let stat = world.get::<Stat>(patient).copied().unwrap_or_default();
    let (stat_text, statstate) = match stat {
        Stat::Dead => ("Мёртв", "bad"),
        Stat::HardCrit => ("Крит. состояние", "bad"),
        Stat::SoftCrit => ("Крит. состояние", "average"),
        Stat::Conscious if health.knocked_out(stat) => ("Без сознания", "average"),
        Stat::Conscious => ("Стабилен", "good"),
    };
    let blood = world.get::<super::blood::Blood>(patient);
    PatientData {
        stat: stat_text.into(),
        statstate: statstate.into(),
        health: health.health(),
        blood_type: blood.map_or_else(|| "НЕИЗВЕСТНО".into(), |b| b.blood_type.name().into()),
        max_health: 100.0,
        min_health: HEALTH_THRESHOLD_DEAD,
        brute_loss: health.brute,
        fire_loss: health.burn,
        tox_loss: health.tox,
        oxy_loss: health.oxy,
        blood_level: blood.map_or(0.0, |b| b.volume),
        standard_blood_level: super::blood::BLOOD_VOLUME_NORMAL,
        surgery_state: surgery_state(world, patient, zone),
    }
}

/// The server half of `OperatingComputer`.
pub struct OperatingWindow;

impl UiInterface for OperatingWindow {
    const NAME: &'static str = OperatingComputerUi::NAME;
    type Data = OperatingComputerData;
    type StaticData = ();
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperatingAct {
    ChangeZone(TargetZone),
}

impl ServerUi for OperatingWindow {
    type Source = &'static OperatingComputer;
    type UserParam = ();
    type Act = OperatingAct;

    fn static_data(_: &OperatingComputer) {}

    fn data(computer: &OperatingComputer, _: Entity, _: &()) -> OperatingComputerData {
        computer.view.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<OperatingAct, UiReject> {
        match action {
            CHANGE_ZONE => {
                let zone = params
                    .get(NEW_ZONE)
                    .and_then(TargetZone::from_dm)
                    .ok_or_else(|| UiReject::BadParams("new_zone: expected a body zone".into()))?;
                Ok(OperatingAct::ChangeZone(zone))
            }
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// Rebuilds the data of computers with an open window.
// Cost: per tick, per open window (a handful per round): the catalog (~30 operation types)
// against one patient.
fn refresh_views(world: &mut World) {
    let mut sources: Vec<Entity> = world
        .query_filtered::<&UiWindow, With<UiKind<OperatingWindow>>>()
        .iter(world)
        .map(|w| w.source)
        .collect();
    sources.sort_unstable();
    sources.dedup();
    for computer in sources {
        let data = view(world, computer);
        if let Some(mut state) = world.get_mut::<OperatingComputer>(computer)
            && state.view != data
        {
            state.view = data;
        }
    }
}

/// tg `ui_check`, lowering the default status.
fn cap_windows(world: &mut World) {
    let windows: Vec<(Entity, Entity, Entity)> = world
        .query_filtered::<(Entity, &UiWindow), With<UiKind<OperatingWindow>>>()
        .iter(world)
        .map(|(e, w)| (e, w.actor, w.source))
        .collect();
    for (window, user, computer) in windows {
        let cap = ui_check(world, user, computer);
        if let Some(mut w) = world.get_mut::<UiWindow>(window) {
            w.cap_status(cap);
        }
    }
}

fn ui_check(world: &World, user: Entity, computer: Entity) -> UiStatus {
    if world.get::<crate::power::Unpowered>(computer).is_some() {
        return UiStatus::Close;
    }
    let health = world.get::<Health>(user);
    let stat = world.get::<Stat>(user).copied().unwrap_or_default();
    if health.is_some_and(|h| h.knocked_out(stat)) {
        return UiStatus::Close;
    }
    if world.get::<BodyPosition>(user).is_some_and(|b| b.lying()) {
        let on_table = table_of(world, computer)
            .and_then(|t| world.get::<TilePos>(t))
            .is_some_and(|t| world.get::<TilePos>(user) == Some(t));
        return if on_table {
            UiStatus::Close
        } else {
            UiStatus::Disabled
        };
    }
    let far = match (world.get::<TilePos>(user), world.get::<TilePos>(computer)) {
        (Some(a), Some(b)) => a.chebyshev(*b).is_none_or(|d| d > 2),
        _ => true,
    };
    if far || crate::status_effects::incapacitation(world, user).incapacitated {
        return UiStatus::Disabled;
    }
    UiStatus::Interactive
}

fn acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<OperatingWindow>>>) {
    let pending: Vec<(Entity, Entity, OperatingAct)> = acts
        .read(world.resource::<Messages<UiActed<OperatingWindow>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (user, computer, act) in pending {
        let OperatingAct::ChangeZone(zone) = act;
        if let Some(mut state) = world.get_mut::<OperatingComputer>(computer) {
            state.target_zone = zone;
        }
        if world.get::<ZoneSelected>(user).is_some() {
            world.entity_mut(user).insert(ZoneSelected(zone));
        }
    }
}

/// Opens the window (`ui_interact`); the first viewer's selected zone becomes the computer's.
pub fn open(world: &mut World, actor: Actor, computer: Entity) {
    let viewers = world
        .query_filtered::<&UiWindow, With<UiKind<OperatingWindow>>>()
        .iter(world)
        .any(|w| w.source == computer);
    if !viewers
        && let Some(&ZoneSelected(zone)) = world.get::<ZoneSelected>(actor.mob)
        && let Some(mut state) = world.get_mut::<OperatingComputer>(computer)
    {
        state.target_zone = zone;
    }
    let data = view(world, computer);
    if let Some(mut state) = world.get_mut::<OperatingComputer>(computer) {
        state.view = data;
    }
    let title = name_of(world, computer);
    world.write_message(OpenUi::new::<OperatingWindow>(
        actor.client,
        actor.mob,
        computer,
        title,
    ));
}

/// tg `attack_hand` / `item_interaction` of the computer. True when the click was its.
pub fn interact(world: &mut World, actor: Actor, computer: Entity) -> bool {
    if world.get::<OperatingComputer>(computer).is_none() {
        return false;
    }
    let Some(item) = actor.item else {
        // `INTERACT_MACHINE_REQUIRES_STANDING`; the window itself closes without power.
        if world
            .get::<BodyPosition>(actor.mob)
            .is_some_and(|b| b.lying())
            || world.get::<crate::power::Unpowered>(computer).is_some()
        {
            return true;
        }
        open(world, actor, computer);
        return true;
    };
    let path = world
        .get::<DmItem>(item)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    if is_under(&path, DISK) {
        let (user, disk, what) = (
            name_of(world, actor.mob),
            name_of(world, item),
            name_of(world, computer),
        );
        visible(
            world,
            actor.mob,
            &format!("Вы начинаете загружать хирургические протоколы с {disk}..."),
            &format!("{user} начинает загружать {disk} в {what}..."),
            Span::Notice,
        );
        do_after(
            world,
            actor,
            computer,
            10.0,
            0,
            crate::construction::Job::Medical(MedJob::LoadDisk),
        );
        return true;
    }
    let surgical = world
        .get::<ss13_mapload::DmWounding>(item)
        .is_some_and(|w| w.surgical_tool);
    if surgical && !super::surgery::combat_mode(world, actor.mob) {
        if world.get::<crate::power::Unpowered>(computer).is_none() {
            open(world, actor, computer);
        }
        return true;
    }
    false
}

/// The end of a disk's loading: the disk still in hand.
pub fn finish_load(world: &mut World, actor: Actor, computer: Entity) {
    let Some(item) = actor.item else {
        return;
    };
    let held = world
        .get::<crate::inventory::Inventory>(actor.mob)
        .and_then(crate::inventory::Inventory::active_item);
    let path = world
        .get::<DmItem>(item)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    if held != Some(item) || !is_under(&path, DISK) {
        return;
    }
    let Some(mut state) = world.get_mut::<OperatingComputer>(computer) else {
        return;
    };
    for op in disk_surgeries(&path) {
        if !state.disks.contains(&op) {
            state.disks.push(op);
        }
    }
    tell(world, actor.client, Span::Notice, "операции загружены");
}

pub(super) fn build(app: &mut App) {
    app.register_ui::<OperatingWindow>()
        .add_observer(add_computer)
        .add_systems(
            Update,
            (
                cap_windows.in_set(UiSystems::StatusHook),
                acts.in_set(UiSystems::Handle),
                refresh_views
                    .after(UiSystems::Handle)
                    .before(UiSystems::Send),
            ),
        );
}
