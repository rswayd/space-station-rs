//! The portable gravity unit (server): tg `/obj/machinery/power/portagrav`
//! (`code/game/machinery/portagrav.dm`) and its `Portagrav` window.
//!
//! - Anchored and with power for the whole range, a right click (or the window) turns it on: a
//!   gravity field of `grav_strength` G over every tile within `range`
//!   ([`crate::gravity::GravityFields`]). Every machine tick it draws `draw_per_range * range`
//!   joules from its cell, or with wire mode from the powernet under it; short of that it turns
//!   off.
//! - The window sets the gravity (0..2 G, 0..4 G emagged) and the range (0..`max_range`), and
//!   switches wire mode. `RefreshParts`: `draw_per_range = 1 kW - 100 W per micro laser tier
//!   over 1`, `max_range = 4 + capacitor tiers`.
//! - A wrench anchors it (refused while on); with the panel open (a screwdriver,
//!   `crate::machine_frame`) a cell goes in, and an empty hand out of combat mode takes it out.
//!   Built ones start with a high-capacity cell like mapped ones.
// shortcut: no contextual screentips, no field particles (`/obj/effect/gravity_fluff_field`)
// or entrance balloon alerts. A range of 0 draws nothing instead of tg's `cell.use(0)` failing.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmPower};
use ss13_shared::appearance::Overlay;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::portagrav::{PortagravData, PortagravUi};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::{Grid, MachineTicks, NodeKind};
use crate::construction::PanelOpen;
use crate::gravity::{GravityField, GravityFields};
use crate::mecha::core::ItemCell;
use crate::obj_damage::anchored;
use crate::stock_parts::PartKind;
use crate::storage::StoredIn;
use crate::tools::{Actor, ToolKind, tell, tool_of, use_tool};

pub const PORTAGRAV: &str = "/obj/machinery/power/portagrav";
/// The `cell` var.
const CELL: &str = "/obj/item/stock_parts/power_store/cell/high";
/// `BASE_MACHINE_ACTIVE_CONSUMPTION`.
const BASE_DRAW: f64 = 1000.0;
/// `GRAVITY_DAMAGE_THRESHOLD - 1`, and the emag's `+ 2`.
const MAX_GRAVITY: f32 = 2.0;
const EMAG_BONUS: f32 = 2.0;
const WRENCH_DS: f32 = 20.0;
const BUTTON_SOUND: &str = "sound/machines/terminal/terminal_button07.ogg";
const SPARKS: &str = "sparks";

/// tg `/obj/machinery/power/portagrav`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Portagrav {
    pub on: bool,
    pub wire_mode: bool,
    pub grav_strength: f32,
    pub range: u32,
    pub max_range: u32,
    /// Joules per machine tick per tile of range.
    pub draw_per_range: f64,
    pub emagged: bool,
    /// `cell`; [`cell_of`] checks it is still inside.
    pub cell: Option<Entity>,
}

impl Default for Portagrav {
    fn default() -> Self {
        Self {
            on: false,
            wire_mode: false,
            grav_strength: 1.0,
            range: 4,
            max_range: 6,
            draw_per_range: BASE_DRAW,
            emagged: false,
            cell: None,
        }
    }
}

impl Portagrav {
    /// `draw_per_range * range`.
    pub fn draw(&self) -> f64 {
        self.draw_per_range * f64::from(self.range)
    }

    /// `adjust_grav`: clamped to 0..2 G (0..4 emagged). True when it changed.
    pub fn adjust_gravity(&mut self, by: f32) -> bool {
        let bonus = if self.emagged { EMAG_BONUS } else { 0.0 };
        let result = (self.grav_strength + by).clamp(0.0, MAX_GRAVITY + bonus);
        if result == self.grav_strength {
            return false;
        }
        self.grav_strength = result;
        true
    }

    /// `adjust_range`: clamped to 0..`max_range`. True when it changed.
    pub fn adjust_range(&mut self, by: i64) -> bool {
        let result = (i64::from(self.range) + by).clamp(0, i64::from(self.max_range)) as u32;
        if result == self.range {
            return false;
        }
        self.range = result;
        true
    }
}

pub struct PortagravPlugin;

impl Plugin for PortagravPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // `ui_interact` and `attack_hand` (the cell out); right: `attack_hand_secondary`.
        app.add_click_handler(ClickPhase::EmptyHand, 1991, |w, c| {
            interact(w, c.client, c.mob, c.target, c.modifiers.right)
        });
        // A cell (`item_interaction`), `wrench_act`.
        app.add_click_handler(ClickPhase::HeldOnTarget, 2881, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
        app.register_ui::<PortagravWindow>()
            .add_systems(ss13_world::MapInit, setup_mapped.after(super::init_power))
            .add_systems(
                Update,
                (
                    portagrav_acts.in_set(UiSystems::Handle),
                    (process_portagravs, looks)
                        .chain()
                        .after(super::PowerSystems),
                ),
            );
    }
}

fn setup_mapped(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &DmPower), With<ss13_world::Mapped>>();
    let found: Vec<Entity> = q
        .iter(world)
        .filter(|(_, dm)| is_under(&dm.path, PORTAGRAV))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup(world, e);
    }
}

/// `Initialize` (`cell = new cell(src)`) and `RefreshParts`; also for units built from a frame
/// (`crate::machine_frame::setup_built`).
pub fn setup(world: &mut World, e: Entity) {
    let mut p = world.get::<Portagrav>(e).cloned().unwrap_or_default();
    let r = crate::stock_parts::ratings(world, e);
    let lasers: Vec<u32> = r.tiers(PartKind::MicroLaser).collect();
    let capacitors: Vec<u32> = r.tiers(PartKind::Capacitor).collect();
    // A mapped unit has no parts: its board's two tier 1 lasers and capacitors.
    let lasers = if lasers.is_empty() {
        vec![1, 1]
    } else {
        lasers
    };
    let capacitors = if capacitors.is_empty() {
        vec![1, 1]
    } else {
        capacitors
    };
    p.draw_per_range = BASE_DRAW
        - lasers
            .iter()
            .map(|&t| BASE_DRAW / 10.0 * f64::from(t.saturating_sub(1)))
            .sum::<f64>();
    p.max_range = 4 + capacitors.iter().sum::<u32>();
    p.range = p.range.min(p.max_range);
    if p.cell.is_none()
        && let Some(cell) = crate::construction::spawn_detached(world, CELL)
    {
        crate::storage::put_inside(world, cell, e);
        p.cell = Some(cell);
    }
    world.entity_mut(e).insert(p);
}

/// The cell still inside.
pub fn cell_of(world: &World, e: Entity) -> Option<Entity> {
    let cell = world.get::<Portagrav>(e)?.cell?;
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

/// `connect_to_network` / `disconnect_from_network`: on the powernet only anchored in wire
/// mode.
fn sync_node(world: &mut World, e: Entity) {
    let want = anchored(world, e) && world.get::<Portagrav>(e).is_some_and(|p| p.wire_mode);
    let (Some(&pos), Some(layer)) = (
        world.get::<TilePos>(e),
        world.get::<DmPower>(e).map(super::layer),
    ) else {
        return;
    };
    let mut grid = world.resource_mut::<Grid>();
    let has = grid.node_net(e).is_some();
    if want && !has {
        grid.add_node(e, pos, layer, NodeKind::Machine);
    } else if !want && has {
        grid.remove_node(e);
    }
}

fn surplus(world: &World, e: Entity) -> Option<f64> {
    let grid = world.resource::<Grid>();
    grid.node_net(e)
        .and_then(|n| grid.net(n))
        .map(|n| n.surplus())
}

/// The field of a unit that is on, at its tile.
fn update_field(world: &mut World, e: Entity) {
    let field = world.get::<Portagrav>(e).filter(|p| p.on).and_then(|p| {
        Some(GravityField {
            source: e,
            center: *world.get::<TilePos>(e)?,
            range: p.range,
            gravity: p.grav_strength,
        })
    });
    world.resource_mut::<GravityFields>().set(e, field);
}

/// `turn_on(user)`.
pub fn turn_on(world: &mut World, mob: Option<Entity>, e: Entity) -> bool {
    let Some(p) = world.get::<Portagrav>(e).cloned() else {
        return false;
    };
    let alert = |world: &mut World, text: &str| {
        if let Some(mob) = mob {
            crate::balloon::balloon_alert(world, mob, e, text);
        }
    };
    if !anchored(world, e) {
        alert(world, "не закреплено!");
        return false;
    }
    sync_node(world, e);
    let draw = p.draw();
    let enough = if p.wire_mode {
        surplus(world, e).unwrap_or(0.0) >= draw
    } else {
        charge(world, e).is_some_and(|c| c >= draw)
    };
    if !enough {
        alert(world, "недостаточно энергии!");
        return false;
    }
    alert(world, "включено");
    if let Some(mut p) = world.get_mut::<Portagrav>(e) {
        p.on = true;
    }
    update_field(world, e);
    true
}

/// `turn_off(user)`.
pub fn turn_off(world: &mut World, mob: Option<Entity>, e: Entity) {
    if let Some(mut p) = world.get_mut::<Portagrav>(e) {
        p.on = false;
    }
    if let Some(mob) = mob {
        crate::balloon::balloon_alert(world, mob, e, "выключено");
    }
    update_field(world, e);
}

fn toggle_on(world: &mut World, mob: Entity, e: Entity) {
    if world.get::<Portagrav>(e).is_some_and(|p| p.on) {
        turn_off(world, Some(mob), e);
    } else {
        turn_on(world, Some(mob), e);
    }
}

/// `process()` of every unit that is on, once per machine tick.
fn process_portagravs(world: &mut World, mut last: Local<u64>) {
    let ticks = world.resource::<MachineTicks>().0;
    if ticks == *last {
        return;
    }
    *last = ticks;
    let units: Vec<(Entity, Portagrav)> = world
        .query::<(Entity, &Portagrav)>()
        .iter(world)
        .map(|(e, p)| (e, p.clone()))
        .collect();
    for (e, p) in units {
        // The wrench (`crate::construction` `Unfasten`) does not reconnect it.
        sync_node(world, e);
        if !p.on {
            continue;
        }
        if !anchored(world, e) {
            turn_off(world, None, e);
            continue;
        }
        let draw = p.draw();
        let powered = if p.wire_mode {
            let grid = world.resource::<Grid>();
            let net = grid.node_net(e);
            let mut grid = world.resource_mut::<Grid>();
            match net.and_then(|n| grid.net_mut(n)) {
                Some(net) if net.surplus() >= draw => {
                    net.load += draw;
                    true
                }
                _ => false,
            }
        } else if draw <= 0.0 {
            true
        } else {
            cell_of(world, e)
                .and_then(|c| world.get_mut::<ItemCell>(c))
                .is_some_and(|mut c| c.0.take(draw, false) > 0.0)
        };
        if !powered {
            turn_off(world, None, e);
        }
    }
}

/// `update_icon_state` (`portagrav`, `portagrav_o` with the panel open) and `update_overlays`
/// (anchors, lights and `activated` when on).
fn looks(
    icons: Option<Res<IconIndex>>,
    mut q: Query<
        (
            &Portagrav,
            &DmAtom,
            Option<&PanelOpen>,
            Option<&crate::obj_damage::Anchored>,
            &mut Appearance,
        ),
        Or<(
            Changed<Portagrav>,
            Changed<PanelOpen>,
            Changed<crate::obj_damage::Anchored>,
        )>,
    >,
) {
    let Some(icons) = icons else {
        return;
    };
    for (p, atom, panel, anchored, mut appearance) in &mut q {
        let open = panel.is_some_and(|o| o.0);
        let mut new = appearance.clone();
        if let Some(id) = icons.get(&atom.icon, if open { "portagrav_o" } else { "portagrav" }) {
            new.icon_state = id;
        }
        let mut states = Vec::new();
        if anchored.is_some_and(|a| a.0) {
            states.push("portagrav_anchors");
        }
        if p.on {
            states.extend(["portagrav_lights", "activated"]);
        }
        new.overlays = states
            .into_iter()
            .filter_map(|s| icons.get(&atom.icon, s))
            .map(Overlay::new)
            .collect();
        if *appearance != new {
            *appearance = new;
        }
    }
}

/// An empty hand: the window (and, with the panel open, the cell out of combat mode); a right
/// click switches it.
pub fn interact(world: &mut World, client: Entity, mob: Entity, e: Entity, right: bool) -> bool {
    if world.get::<Portagrav>(e).is_none() {
        return false;
    }
    if right {
        toggle_on(world, mob, e);
        return true;
    }
    let title = crate::security::name_of(world, e);
    world.write_message(OpenUi::new::<PortagravWindow>(client, mob, e, title));
    let open = world.get::<PanelOpen>(e).is_some_and(|p| p.0);
    let combat = world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0);
    if open
        && !combat
        && let Some(cell) = cell_of(world, e)
    {
        if let Some(mut p) = world.get_mut::<Portagrav>(e) {
            p.cell = None;
        }
        crate::cooking::give_to(world, mob, e, cell);
    }
    true
}

/// A cell into an open unit, a wrench. True when it was ours.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    e: Entity,
) -> bool {
    let Some(p) = world.get::<Portagrav>(e).cloned() else {
        return false;
    };
    if world.get::<ItemCell>(held).is_some() {
        if !world.get::<PanelOpen>(e).is_some_and(|o| o.0) {
            crate::balloon::balloon_alert(world, mob, e, "нужно открыть панель!");
        } else if cell_of(world, e).is_some() {
            crate::balloon::balloon_alert(world, mob, e, "элемент питания уже есть!");
        } else {
            crate::storage::put_inside(world, held, e);
            if let Some(mut p) = world.get_mut::<Portagrav>(e) {
                p.cell = Some(held);
            }
        }
        return true;
    }
    if tool_of(world, held).map(|(t, _)| t) != Some(ToolKind::Wrench) {
        return false;
    }
    if p.on {
        crate::balloon::balloon_alert(world, mob, e, "сначала выключите!");
        return true;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    use_tool(
        world,
        actor,
        e,
        WRENCH_DS,
        0,
        crate::construction::Job::Unfasten {
            anchored: anchored(world, e),
        },
    );
    true
}

/// Taken apart (from `crate::machine_frame::on_deconstruction`): off the powernet, the field
/// gone; the cell drops with the rest of its contents.
pub fn on_deconstruction(world: &mut World, e: Entity) {
    if world.get::<Portagrav>(e).is_none() {
        return;
    }
    world.resource_mut::<Grid>().remove_node(e);
    world.resource_mut::<GravityFields>().set(e, None);
}

/// `emag_act` (from `crate::emag`): unsafe gravity unlocked. True when it took.
pub fn emag_act(world: &mut World, client: Entity, e: Entity) -> bool {
    let Some(mut p) = world.get_mut::<Portagrav>(e) else {
        return false;
    };
    if p.emagged {
        return false;
    }
    p.emagged = true;
    tell(world, client, Span::Warning, "Из устройства летят искры!");
    if let Some(&pos) = world.get::<TilePos>(e) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, SPARKS, 50.0, true));
    }
    true
}

/// `examine`.
pub fn examine_lines(world: &World, e: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(p) = world.get::<Portagrav>(e) else {
        return;
    };
    lines.push((
        Span::Plain,
        format!(
            "Устройство {}.",
            if p.on {
                "включено"
            } else {
                "выключено"
            }
        ),
    ));
    let meter = match cell_of(world, e).and_then(|c| world.get::<ItemCell>(c)) {
        Some(cell) => format!("{}%", cell.0.percent().round()),
        None => "НЕТ ЯЧЕЙКИ".to_owned(),
    };
    lines.push((Span::Plain, format!("Индикатор заряда: {meter}.")));
    lines.push((
        Span::Plain,
        if anchored(world, e) {
            "Устройство закреплено."
        } else {
            "Устройство не закреплено."
        }
        .to_owned(),
    ));
    lines.push((
        Span::Notice,
        format!(
            "<b>ПКМ</b>, чтобы {}.",
            if p.on {
                "выключить"
            } else {
                "включить"
            }
        ),
    ));
}

pub struct PortagravWindow;

impl UiInterface for PortagravWindow {
    const NAME: &'static str = PortagravUi::NAME;
    type Data = PortagravData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PortagravAct {
    AdjustGrav(f64),
    TogglePower,
    ToggleWire,
    AdjustRange(f64),
}

impl ServerUi for PortagravWindow {
    type Source = (Entity, &'static Portagrav);
    type UserParam = Query<'static, 'static, (&'static ItemCell, &'static StoredIn)>;
    type Act = PortagravAct;

    fn static_data(_: (Entity, &Portagrav)) {}

    fn data(
        (e, p): (Entity, &Portagrav),
        _: Entity,
        cells: &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> PortagravData {
        let percentage = p
            .cell
            .and_then(|c| cells.get(c).ok())
            .filter(|(_, inside)| inside.0 == e)
            .map(|(c, _)| c.0.percent() as f32);
        PortagravData {
            percentage,
            gravity: p.grav_strength,
            range: p.range,
            maxrange: p.max_range,
            on: p.on,
            wiremode: p.wire_mode,
            // tg shows the joules per machine tick as watts (`display_power`).
            draw: p.draw() as f32,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<PortagravAct, UiReject> {
        match action {
            "adjust_grav" => Ok(PortagravAct::AdjustGrav(params.number("adjustment")?)),
            "toggle_power" => Ok(PortagravAct::TogglePower),
            "toggle_wire" => Ok(PortagravAct::ToggleWire),
            "adjust_range" => Ok(PortagravAct::AdjustRange(params.number("adjustment")?)),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// `ui_act`, against the unit's state now.
fn portagrav_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<PortagravWindow>>>) {
    let pending: Vec<(Entity, Entity, PortagravAct)> = acts
        .read(world.resource::<Messages<UiActed<PortagravWindow>>>())
        .map(|a| (a.actor, a.source, a.act))
        .collect();
    for (actor, e, act) in pending {
        if world.get::<Portagrav>(e).is_none() {
            continue;
        }
        if let Some(&pos) = world.get::<TilePos>(e) {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(pos, BUTTON_SOUND, 45.0, true),
            );
        }
        match act {
            PortagravAct::AdjustGrav(by) if by.is_finite() => {
                let changed = world
                    .get_mut::<Portagrav>(e)
                    .is_some_and(|mut p| p.adjust_gravity(by as f32));
                if changed {
                    update_field(world, e);
                }
            }
            PortagravAct::AdjustRange(by) if by.is_finite() => {
                let changed = world
                    .get_mut::<Portagrav>(e)
                    .is_some_and(|mut p| p.adjust_range(by.trunc() as i64));
                if changed {
                    update_field(world, e);
                }
            }
            PortagravAct::AdjustGrav(_) | PortagravAct::AdjustRange(_) => {}
            PortagravAct::TogglePower => toggle_on(world, actor, e),
            PortagravAct::ToggleWire => {
                if let Some(mut p) = world.get_mut::<Portagrav>(e) {
                    p.wire_mode = !p.wire_mode;
                }
                sync_node(world, e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gravity_and_range_stay_in_bounds() {
        let mut p = Portagrav::default();
        assert_eq!(p.draw(), 4000.0);
        assert!(p.adjust_gravity(1.0));
        assert!(!p.adjust_gravity(1.0), "2 G at most");
        assert_eq!(p.grav_strength, 2.0);
        p.emagged = true;
        assert!(p.adjust_gravity(5.0));
        assert_eq!(p.grav_strength, 4.0);
        assert!(p.adjust_gravity(-10.0));
        assert_eq!(p.grav_strength, 0.0);
        assert!(p.adjust_range(10));
        assert_eq!(p.range, 6);
        assert!(p.adjust_range(-10));
        assert_eq!((p.range, p.draw()), (0, 0.0));
    }
}
