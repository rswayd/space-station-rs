//! The smoke machine (server): tg `/obj/machinery/smoke_machine` (`smoke_machine.dm`) and its
//! `SmokeMachine` window.
//!
//! - Its tank holds `100 + 100 × matter bin tier` units (injectable); an open container clicked
//!   on it pours `amount_per_transfer_from_this` in. Turned on (the window; logged to admins
//!   with the contents), anchored and powered, every machine tick (2 s) with no smoke on its
//!   tile it puffs `DIAMOND_AREA(setting × 3)` nodes of transparent chemical smoke carrying a
//!   copy of up to 20 units of the tank (`crate::effects::smoke::do_chem_smoke`), and the tank
//!   loses `nodes / efficiency` units (`efficiency` 18 + 2 × capacitor tier). An empty tank,
//!   unanchoring or power loss turns it off.
//! - `setting` 1..`max_range` (1 + servo tiers, at least 3); `purge` empties the tank.
//! - A wrench (4 s), screwdriver or crowbar is refused while it is on.
// shortcut: no plumbing demand, rotation or icon states.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmObj, DmPower, DmReagents};
use ss13_reagents::{Holder, Owner, flags};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::server::{OpenUi, ServerUi, UiActed, UiParams, UiReject};

use super::*;
use crate::chem::{Chem, Reagents};
use crate::effects::smoke::{Smoke, SmokeKind, diamond_area, do_chem_smoke};
use crate::obj_damage::{anchored, is_under};
use crate::power::{MachineTicks, Unpowered};
use crate::stock_parts::PartKind;
use crate::tools::{Actor, ToolKind, tool_of, use_tool};

pub const SMOKE_MACHINE: &str = "/obj/machinery/smoke_machine";
/// `REAGENTS_BASE_VOLUME`.
const BASE_VOLUME: f32 = 100.0;
/// `carry_limit`.
const CARRY_LIMIT: f32 = 20.0;
/// `default_unfasten_wrench(time = 4 SECONDS)`, deciseconds.
const WRENCH_DS: f32 = 40.0;

#[derive(Component, Clone, Debug, PartialEq)]
#[require(Views<SmokeMachineData>)]
pub struct SmokeMachine {
    pub on: bool,
    pub setting: u32,
    pub max_range: u32,
    pub efficiency: f32,
}

pub(super) fn ensure(world: &mut World, e: Entity) -> bool {
    if world.get::<SmokeMachine>(e).is_some() {
        return true;
    }
    let Some(path) = world.get::<DmObj>(e).map(|o| o.path.clone()) else {
        return false;
    };
    if !is_under(&path, SMOKE_MACHINE) {
        return false;
    }
    crate::stock_parts::ensure_parts(world, e);
    world.entity_mut(e).insert((
        SmokeMachine {
            on: false,
            setting: 1,
            max_range: 3,
            efficiency: 20.0,
        },
        Reagents(Holder::new(
            BASE_VOLUME,
            flags::INJECTABLE,
            Owner {
                path,
                living: false,
            },
        )),
    ));
    refresh_parts(world, e);
    true
}

/// tg `RefreshParts`.
pub fn refresh_parts(world: &mut World, e: Entity) {
    if world.get::<SmokeMachine>(e).is_none() {
        return;
    }
    let r = crate::stock_parts::ratings(world, e);
    let volume = r
        .tiers(PartKind::MatterBin)
        .fold(BASE_VOLUME, |v, t| v + BASE_VOLUME * t as f32);
    let efficiency = r
        .tiers(PartKind::Capacitor)
        .fold(18.0, |v, t| v + 2.0 * t as f32);
    let range = r.tiers(PartKind::Servo).fold(1, |v, t| v + t).max(3);
    if let Some(mut m) = world.get_mut::<SmokeMachine>(e) {
        m.efficiency = efficiency;
        m.max_range = range;
        m.setting = m.setting.min(range);
    }
    if let Some(mut r) = world.get_mut::<Reagents>(e) {
        r.0.maximum_volume = volume;
    }
}

pub(super) fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    e: Entity,
    right: bool,
) -> bool {
    if world.get::<SmokeMachine>(e).is_none() {
        return false;
    }
    // `INTERACT_ATOM_REQUIRES_ANCHORED`.
    if !right && anchored(world, e) {
        world.write_message(OpenUi::new::<SmokeMachineUi>(
            client,
            mob,
            e,
            title(world, e),
        ));
    }
    true
}

pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    e: Entity,
) -> bool {
    let Some(on) = world.get::<SmokeMachine>(e).map(|m| m.on) else {
        return false;
    };
    if world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
    {
        return false;
    }
    if let Some((kind, _)) = tool_of(world, held) {
        if !matches!(
            kind,
            ToolKind::Wrench | ToolKind::Screwdriver | ToolKind::Crowbar
        ) {
            return false;
        }
        if on {
            crate::balloon::balloon_alert(world, mob, e, "сначала выключите!");
            return true;
        }
        if kind != ToolKind::Wrench {
            // `default_deconstruction_screwdriver` / `_crowbar` (`crate::machine_frame`).
            return false;
        }
        let actor = Actor {
            client,
            mob,
            item: Some(held),
        };
        let anchored = anchored(world, e);
        use_tool(
            world,
            actor,
            e,
            WRENCH_DS,
            0,
            crate::construction::Job::Unfasten { anchored },
        );
        return true;
    }
    // An open reagent container pours into the tank.
    if world.get::<DmReagents>(held).is_none()
        || !world
            .get::<Reagents>(held)
            .is_some_and(|r| r.0.is_open_container())
    {
        return false;
    }
    let amount = world
        .get::<DmReagents>(held)
        .map_or(0.0, |d| d.amount_per_transfer_from_this);
    let moved = crate::chem::transfer(world, held, e, amount);
    if moved > 0.0 {
        notice(
            world,
            client,
            format!(
                "Вы переливаете {} ед. раствора в {}.",
                crate::chem::dm_num(moved),
                title(world, e)
            ),
        );
    }
    true
}

/// `process`, every machine tick.
// Cost: one pass over the smoke machines per machine tick (2 s).
pub(super) fn process(world: &mut World, mut last: Local<Option<u64>>) {
    let ticks = world.resource::<MachineTicks>().0;
    let from = last.unwrap_or(ticks);
    *last = Some(ticks);
    if from == ticks {
        return;
    }
    let mut q = world.query::<(Entity, &SmokeMachine)>();
    let on: Vec<Entity> = q
        .iter(world)
        .filter(|(_, m)| m.on)
        .map(|(e, _)| e)
        .collect();
    for e in on {
        puff(world, e);
    }
}

fn puff(world: &mut World, e: Entity) {
    let Some(m) = world.get::<SmokeMachine>(e).cloned() else {
        return;
    };
    let total = world.get::<Reagents>(e).map_or(0.0, |r| r.0.total_volume);
    if total <= 0.0 || !anchored(world, e) || world.get::<Unpowered>(e).is_some() {
        if let Some(mut m) = world.get_mut::<SmokeMachine>(e) {
            m.on = false;
        }
        return;
    }
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let smoky = world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&s| world.get::<Smoke>(s).is_some());
    if smoky {
        return;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let nodes = diamond_area(m.setting * 3);
    let Some(mut tank) = world.get::<Reagents>(e).map(|r| r.0.clone()) else {
        return;
    };
    let mut carry = Holder::new(CARRY_LIMIT, flags::NO_REACT, Owner::default());
    tank.trans_to(&chem, &mut carry, CARRY_LIMIT, 1.0, None, true, true);
    do_chem_smoke(world, 0, pos, Some(nodes), &carry, SmokeKind::ChemMachine);
    tank.remove_all(nodes as f32 / m.efficiency);
    world.entity_mut(e).insert(Reagents(tank));
    let active = world
        .get::<DmPower>(e)
        .map_or(0.0, |p| p.active_power_usage);
    crate::kitchen::machine_use_energy(
        world,
        e,
        active * f64::from(m.setting) / f64::from(m.max_range),
    );
}

pub(super) fn view(world: &mut World, e: Entity, _user: Entity) -> Option<SmokeMachineData> {
    let m = world.get::<SmokeMachine>(e)?.clone();
    let chem = world.get_resource::<Chem>()?.0.clone();
    let tank = &world.get::<Reagents>(e)?.0;
    Some(SmokeMachineData {
        active: m.on,
        setting: m.setting,
        max_setting: m.max_range,
        max_volume: tank.maximum_volume,
        current_volume: tank.total_volume,
        contents: lines(&chem, tank),
    })
}

impl ServerUi for SmokeMachineUi {
    type Source = &'static Views<SmokeMachineData>;
    type UserParam = ();
    type Act = SmokeAct;

    fn static_data(_: &Views<SmokeMachineData>) {}

    fn data(views: &Views<SmokeMachineData>, user: Entity, _: &()) -> SmokeMachineData {
        views.of(user)
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<SmokeAct, UiReject> {
        Ok(match action {
            "purge" => SmokeAct::Purge,
            "power" => SmokeAct::Power,
            "setting" => SmokeAct::Setting(params.number("amount")?),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SmokeAct {
    Purge,
    Power,
    Setting(f64),
}

pub(super) fn acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<SmokeMachineUi>>>) {
    let pending: Vec<(Entity, Entity, SmokeAct)> = cursor
        .read(world.resource::<Messages<UiActed<SmokeMachineUi>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, e, act) in pending {
        apply_act(world, actor, e, act);
    }
}

/// tg `ui_act`, checked at the moment of effect.
pub fn apply_act(world: &mut World, actor: Entity, e: Entity, act: SmokeAct) {
    let Some(m) = world.get::<SmokeMachine>(e).cloned() else {
        return;
    };
    match act {
        SmokeAct::Purge => {
            if let Some(mut r) = world.get_mut::<Reagents>(e) {
                r.0.clear();
            }
        }
        SmokeAct::Setting(amount) => {
            if amount.fract() == 0.0 && (1.0..=f64::from(m.max_range)).contains(&amount) {
                if let Some(mut m) = world.get_mut::<SmokeMachine>(e) {
                    m.setting = amount as u32;
                }
            }
        }
        SmokeAct::Power => {
            let contents = world.get::<Reagents>(e).map(|r| r.0.clone());
            let full = contents.as_ref().is_some_and(|h| h.total_volume > 0.0);
            let on = !m.on && full;
            if let Some(mut m) = world.get_mut::<SmokeMachine>(e) {
                m.on = on;
            }
            if on
                && let (Some(h), Some(chem)) =
                    (contents, world.get_resource::<Chem>().map(|c| c.0.clone()))
            {
                let list = h
                    .reagents
                    .iter()
                    .map(|r| chem.def(r.id).name.clone())
                    .collect::<Vec<_>>()
                    .join(", ");
                let who = crate::security::name_of(world, actor);
                let at = world
                    .get::<TilePos>(e)
                    .map(|p| format!(" at ({}, {}, {})", p.x, p.y, p.z))
                    .unwrap_or_default();
                let text = format!("{who} activated a smoke machine that contains {list}{at}.");
                info!("{text}");
                admin_notice(world, text);
            }
        }
    }
}

/// `examine`.
pub fn examine_lines(world: &World, e: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(m) = world.get::<SmokeMachine>(e) else {
        return;
    };
    if let Some(r) = world.get::<Reagents>(e) {
        lines.push((
            Span::Notice,
            format!(
                "Объём реагентов: {}/{}.",
                crate::chem::dm_num(r.0.total_volume),
                crate::chem::dm_num(r.0.maximum_volume)
            ),
        ));
    }
    lines.push((
        Span::Notice,
        format!(
            "Работает с эффективностью {}%.",
            (m.efficiency / 26.0 * 100.0).round()
        ),
    ));
    if !anchored(world, e) {
        lines.push((
            Span::Warning,
            "Для работы его нужно прикрутить к полу.".to_owned(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn puff_size_and_cost() {
        // `DIAMOND_AREA(setting * 3)` nodes, `nodes / efficiency` units per puff.
        assert_eq!(diamond_area(3), 25);
        assert_eq!(diamond_area(9), 181);
        assert!((25.0_f32 / 20.0 - 1.25).abs() < f32::EPSILON);
    }
}
