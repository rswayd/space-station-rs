//! The electrolyzer: tg `/obj/machinery/electrolyzer` (`machinery/components/electrolyzer/
//! electrolyzer.dm`) and its `Electrolyzer` window.
//!
//! - On, every SSair fire it electrolyzes its tile's air
//!   ([`ss13_atmos::GasMixture::electrolyze`]) and pays `15 * wp² / (efficiency + wp)` joules:
//!   from the area's equipment channel while anchored, else from its cell. Unanchored with an
//!   empty cell, or broken, it switches off.
//! - Alt-click turns it on or off (not with the panel open; it needs a cell or to be anchored).
//! - A screwdriver opens the panel; with it open a cell goes in and the window ejects it; a
//!   wrench (2 s) anchors it (`default_unfasten_wrench`).
// shortcut: the stock parts are the board's tier 1 ones (two servos, two capacitors), there is
// no crowbar deconstruction and no screentips (machine frames and parts:
// docs/parity/engineering/machines.md).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_chat::Span;
use ss13_mapload::{DmAtom, DmObj, IconIndex};
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::electrolyzer::{ElectrolyzerData, ElectrolyzerUi};
use ss13_tgui::server::{OpenUi, ServerUi, UiActed, UiParams, UiReject};

use crate::mecha::core::ItemCell;
use crate::obj_damage::{Anchored, is_under};
use crate::storage::StoredIn;
use crate::tools::{Actor, ToolKind, tell};

pub const ELECTROLYZER: &str = "/obj/machinery/electrolyzer";
/// `RefreshParts` with two tier 1 servos: the sum of their tiers.
const WORKING_POWER: f64 = 2.0;
/// `RefreshParts` with two tier 1 capacitors: `(tiers + 1) * 0.5`.
const EFFICIENCY: f64 = 1.5;
/// `default_unfasten_wrench(user, tool)`: 2 s.
const UNFASTEN_DS: f32 = 20.0;

#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Electrolyzer {
    pub on: bool,
    /// `mode == ELECTROLYZER_MODE_WORKING` (else standby).
    pub working: bool,
    pub panel_open: bool,
    /// `cell`; [`cell_of`] checks it is still inside.
    pub cell: Option<Entity>,
}

/// `process_atmos`: joules a fire costs.
pub fn energy_per_fire() -> f64 {
    5.0 * (3.0 * WORKING_POWER) * WORKING_POWER / (EFFICIENCY + WORKING_POWER)
}

pub fn init_electrolyzers(mut commands: Commands, added: Query<(Entity, &DmObj), Added<DmObj>>) {
    for (e, obj) in &added {
        if is_under(&obj.path, ELECTROLYZER) {
            commands.entity(e).insert(Electrolyzer::default());
        }
    }
}

fn anchored(world: &World, e: Entity) -> bool {
    world.get::<Anchored>(e).is_some_and(|a| a.0)
}

/// The cell still inside.
pub fn cell_of(world: &World, e: Entity) -> Option<Entity> {
    let cell = world.get::<Electrolyzer>(e)?.cell?;
    world
        .get::<StoredIn>(cell)
        .is_some_and(|s| s.0 == e)
        .then_some(cell)
}

fn charge(world: &World, e: Entity) -> Option<f64> {
    cell_of(world, e)
        .and_then(|c| world.get::<ItemCell>(c))
        .map(|c| c.0.charge)
}

/// `process_atmos` of every electrolyzer.
pub fn process_electrolyzers(world: &mut World) {
    let mut q = world.query::<(Entity, &Electrolyzer)>();
    let on: Vec<Entity> = q
        .iter(world)
        .filter(|(_, m)| m.on)
        .map(|(e, _)| e)
        .collect();
    let joules = energy_per_fire();
    for e in on {
        let broken = world
            .get::<crate::obj_damage::Integrity>(e)
            .is_some_and(|i| i.broken);
        let anchored = anchored(world, e);
        let empty = charge(world, e).is_none_or(|c| c <= 0.0);
        if broken || (empty && !anchored) {
            if let Some(mut m) = world.get_mut::<Electrolyzer>(e) {
                m.on = false;
                m.working = false;
            }
            continue;
        }
        // tg's `use_energy` takes nothing from a dead area and the machine works on regardless
        // (it uses no power of its own); here an anchored one waits for the power instead.
        if anchored && !super::canister::equipment_powered(world, e) {
            set_working(world, e, false);
            continue;
        }
        let Some(&pos) = world.get::<TilePos>(e) else {
            continue;
        };
        set_working(world, e, true);
        let reacted = world
            .resource_mut::<ss13_atmos::Atmos>()
            .modify_air(pos, |air| air.electrolyze(WORKING_POWER, None))
            .is_some();
        if !reacted {
            continue;
        }
        if anchored {
            crate::power::machine_use_energy(world, e, joules, false);
        } else if let Some(mut cell) = cell_of(world, e).and_then(|c| world.get_mut::<ItemCell>(c))
        {
            // `cell.use()` takes nothing when short, so the last joules kept it running for
            // ever; the rest is drained instead and the next fire switches it off.
            cell.0.take(joules, true);
        }
    }
}

fn set_working(world: &mut World, e: Entity, working: bool) {
    if let Some(mut m) = world.get_mut::<Electrolyzer>(e)
        && m.working != working
    {
        m.working = working;
    }
}

/// `update_icon_state` and `update_overlays`.
pub fn electrolyzer_looks(
    icons: Option<Res<IconIndex>>,
    mut q: Query<(&Electrolyzer, &DmAtom, &mut Appearance), Changed<Electrolyzer>>,
) {
    let Some(icons) = icons else {
        return;
    };
    for (m, atom, mut appearance) in &mut q {
        let state = match (m.on, m.working) {
            (false, _) => "electrolyzer-off",
            (true, false) => "electrolyzer-standby",
            (true, true) => "electrolyzer-working",
        };
        let mut new = appearance.clone();
        if let Some(id) = icons.get(&atom.icon, state) {
            new.icon_state = id;
        }
        new.overlays.clear();
        if m.panel_open
            && let Some(id) = icons.get(&atom.icon, "electrolyzer-open")
        {
            new.overlays.push(Overlay::new(id));
        }
        if *appearance != new {
            *appearance = new;
        }
    }
}

/// `toggle_power`.
fn toggle_power(world: &mut World, client: Entity, e: Entity) {
    if !anchored(world, e) && cell_of(world, e).is_none() {
        tell(
            world,
            client,
            Span::Warning,
            "Вставьте элемент питания или закрепите!",
        );
        return;
    }
    let Some(mut m) = world.get_mut::<Electrolyzer>(e) else {
        return;
    };
    m.on = !m.on;
    m.working = false;
    let text = if m.on {
        "включено"
    } else {
        "выключено"
    };
    tell(world, client, Span::Notice, text);
}

/// An empty hand: alt-click toggles, a click opens the window. True when it was one.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity, alt: bool) -> bool {
    let Some(m) = world.get::<Electrolyzer>(target) else {
        return false;
    };
    if alt {
        // `click_alt`.
        if m.panel_open {
            tell(world, client, Span::Warning, "Сначала закройте панель!");
        } else {
            toggle_power(world, client, target);
        }
        return true;
    }
    let title = world
        .get::<ss13_mapload::DmName>(target)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<ElectrolyzerWindow>(
        client, mob, target, title,
    ));
    true
}

/// `item_interaction` (a cell), `screwdriver_act` and `wrench_act`. True when the click was
/// theirs.
pub fn item_interaction(
    world: &mut World,
    actor: Actor,
    held: Entity,
    target: Entity,
    tool: Option<ToolKind>,
) -> bool {
    let Some(m) = world.get::<Electrolyzer>(target) else {
        return false;
    };
    let client = actor.client;
    let open = m.panel_open;
    if world.get::<ItemCell>(held).is_some() {
        if !open {
            tell(world, client, Span::Warning, "Сначала откройте панель!");
        } else if cell_of(world, target).is_some() {
            tell(
                world,
                client,
                Span::Warning,
                "Внутри уже есть элемент питания!",
            );
        } else {
            crate::storage::put_inside(world, held, target);
            if let Some(mut m) = world.get_mut::<Electrolyzer>(target) {
                m.cell = Some(held);
            }
            tell(world, client, Span::Notice, "элемент питания установлен");
        }
        return true;
    }
    match tool {
        Some(ToolKind::Screwdriver) => {
            if let Some(mut m) = world.get_mut::<Electrolyzer>(target) {
                m.panel_open = !open;
            }
            let text = if open {
                "панель закрыта"
            } else {
                "панель открыта"
            };
            tell(world, client, Span::Notice, text);
            true
        }
        Some(ToolKind::Wrench) => {
            let anchored = anchored(world, target);
            if !anchored && crate::construction::blocked_for(world, target) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вам не удалось прикрутить электролизёр.",
                );
                return true;
            }
            if crate::tools::use_tool(
                world,
                actor,
                target,
                UNFASTEN_DS,
                0,
                crate::construction::Job::Atmos(super::windows::AtmosJob::Unfasten { anchored }),
            ) {
                let text = if anchored {
                    "Вы начинаете откреплять электролизёр..."
                } else {
                    "Вы начинаете прикреплять электролизёр..."
                };
                tell(world, client, Span::Notice, text);
            }
            true
        }
        _ => false,
    }
}

pub struct ElectrolyzerWindow;

impl UiInterface for ElectrolyzerWindow {
    const NAME: &'static str = ElectrolyzerUi::NAME;
    type Data = ElectrolyzerData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElectrolyzerAct {
    Power,
    Eject,
}

impl ServerUi for ElectrolyzerWindow {
    type Source = (Entity, &'static Electrolyzer, Option<&'static Anchored>);
    type UserParam = Query<'static, 'static, (&'static ItemCell, &'static StoredIn)>;
    type Act = ElectrolyzerAct;

    fn static_data(_: (Entity, &Electrolyzer, Option<&Anchored>)) {}

    fn data(
        (e, m, anchored): (Entity, &Electrolyzer, Option<&Anchored>),
        _: Entity,
        cells: &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> ElectrolyzerData {
        let cell = m
            .cell
            .and_then(|c| cells.get(c).ok())
            .filter(|(_, inside)| inside.0 == e)
            .map(|(c, _)| c.0.percent());
        ElectrolyzerData {
            open: m.panel_open,
            on: m.on,
            has_powercell: cell.is_some(),
            anchored: anchored.is_some_and(|a| a.0),
            power_level: cell.map(|p| p.round() as f32),
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<ElectrolyzerAct, UiReject> {
        match action {
            "power" => Ok(ElectrolyzerAct::Power),
            "eject" => Ok(ElectrolyzerAct::Eject),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

pub fn electrolyzer_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<ElectrolyzerWindow>>>,
) {
    let pending: Vec<(Entity, Entity, ElectrolyzerAct)> = acts
        .read(world.resource::<Messages<UiActed<ElectrolyzerWindow>>>())
        .map(|a| (a.client, a.source, a.act))
        .collect();
    for (client, source, act) in pending {
        match act {
            ElectrolyzerAct::Power => toggle_power(world, client, source),
            // `eject`: only with the panel open, onto the floor (`drop_location`).
            ElectrolyzerAct::Eject => {
                let open = world
                    .get::<Electrolyzer>(source)
                    .is_some_and(|m| m.panel_open);
                if open
                    && let Some(cell) = cell_of(world, source)
                    && let Some(&pos) = world.get::<TilePos>(source)
                {
                    crate::storage::take_out(world, cell, pos);
                    if let Some(mut m) = world.get_mut::<Electrolyzer>(source) {
                        m.cell = None;
                    }
                }
            }
        }
    }
}

/// `examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(m) = world.get::<Electrolyzer>(target) else {
        return;
    };
    let (on, panel) = (
        if m.on {
            "включён"
        } else {
            "выключен"
        },
        if m.panel_open {
            "открыта"
        } else {
            "закрыта"
        },
    );
    lines.push((Span::Plain, format!("Электролизёр {on}, панель {panel}.")));
    let cell = cell_of(world, target)
        .and_then(|c| world.get::<ItemCell>(c))
        .map(|c| c.0.percent());
    lines.push((
        Span::Plain,
        match cell {
            Some(p) => format!("Индикатор заряда показывает {}%.", p.round()),
            None => "Элемент питания не установлен.".into(),
        },
    ));
    let toggle = if m.on {
        "выключить"
    } else {
        "включить"
    };
    lines.push((Span::Notice, format!("<b>Alt-клик</b>, чтобы {toggle}.")));
    lines.push((
        Span::Notice,
        "<b>Закрепите</b> его, чтобы он питался от ЛКП зоны, а не от элемента питания.".into(),
    ));
    let source = if anchored(world, target) {
        "ЛКП зоны"
    } else {
        "внутреннего элемента питания"
    };
    lines.push((Span::Notice, format!("Он питается от {source}.")));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tier_one_electrolyzer_costs_what_tg_says() {
        // 5 * (3 * 2) * 2 / (1.5 + 2).
        assert!((energy_per_fire() - 60.0 / 3.5).abs() < 1e-9);
    }

    #[test]
    fn only_power_and_eject_are_acts() {
        let params = UiParams::new(&[]);
        assert_eq!(
            ElectrolyzerWindow::parse_act("power", &params),
            Ok(ElectrolyzerAct::Power)
        );
        assert_eq!(
            ElectrolyzerWindow::parse_act("eject", &params),
            Ok(ElectrolyzerAct::Eject)
        );
        assert!(ElectrolyzerWindow::parse_act("spawn", &params).is_err());
    }
}
