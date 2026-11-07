//! P.A.C.M.A.N.-type portable generators (server): tg `/obj/machinery/power/port_gen/pacman`
//! and its `super` (uranium) and `wood` (P.L.A.N.K.M.A.N.) kinds, `code/modules/power/
//! port_gen.dm`.
//!
//! - Sheets of the generator's fuel go in by hand, up to `max_sheets`. A wrench anchors it and
//!   puts it on the powernet of its tile; a screwdriver opens its panel; neither works while
//!   it runs.
//! - The `PortableGenerator` window (an empty hand on an anchored generator) switches it on
//!   with fuel, ejects the fuel while it is off and sets the output multiplier 1..4 (higher
//!   when emagged).
//! - Every machine tick a running generator puts `power_gen * power_output` watts into its
//!   powernet (`Generation::process` in [`super`]), burns `power_output / time_per_sheet`
//!   sheets and heats toward `56..76 + 10 * power_output` (up to 400 above 4); over 300 it
//!   explodes (`explosion(src, 2, 5, 2, flash_range = -1)`). Out of fuel or unanchored it
//!   stops; stopped, it cools 2 per tick.
//! - A crowbar on a stopped generator with its panel open takes it apart
//!   (`default_deconstruction_crowbar`, `crate::machine_frame` family `Pacman`): the fuel drops
//!   (`on_deconstruction` → `DropFuel`). Built from its board ([`setup`]): a PACMAN board set to
//!   uranium by a screwdriver ([`PacmanBoard`], `high_production_profile`) builds the `super`
//!   numbers (`on_construction`).
//! - An emag unlocks output levels above 4 (`emag_act`).
// shortcut: no generator sound loop, no `isinspace` check on anchoring, no `emp_act(EMP_HEAVY)`
// of the emag (machine `emp_act` only draws power the generator does not use); power and fuel
// are by kind, map var edits of `power_gen` or `sheets` beyond `pre_loaded` are not read.

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmItem, DmPower};
use ss13_shared::appearance::Overlay;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, EMISSIVE_PLANE, IconStateId, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::portable_generator::{PortableGeneratorData, PortableGeneratorUi};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject};

use super::{Grid, NodeKind};
use crate::combat::CombatRng;
use crate::obj_damage::{Anchored, set_anchored};
use crate::tools::{ToolKind, tell};

pub const PACMAN_ROOT: &str = "/obj/machinery/power/port_gen/pacman";
pub const PACMAN_SUPER: &str = "/obj/machinery/power/port_gen/pacman/super";
pub const PACMAN_WOOD: &str = "/obj/machinery/power/port_gen/pacman/wood";
pub const PACMAN_PRE_LOADED: &str = "/obj/machinery/power/port_gen/pacman/pre_loaded";
pub const PLASMA_SHEET: &str = "/obj/item/stack/sheet/mineral/plasma";
pub const URANIUM_SHEET: &str = "/obj/item/stack/sheet/mineral/uranium";
pub const WOOD_SHEET: &str = "/obj/item/stack/sheet/mineral/wood";
/// `/obj/item/circuitboard/machine/pacman` (not the `plankman` board).
pub const PACMAN_BOARD: &str = "/obj/item/circuitboard/machine/pacman";
/// `current_heat > 300` blows it up.
pub const OVERHEAT: f64 = 300.0;

/// tg `/obj/machinery/power/port_gen/pacman`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Pacman {
    pub active: bool,
    /// Watts per output level.
    pub power_gen: f64,
    /// 1..4 (more when emagged).
    pub power_output: u32,
    pub sheets: u32,
    pub max_sheets: u32,
    pub sheet_path: &'static str,
    pub sheet_name: &'static str,
    /// How much is left of the sheet being burnt.
    pub sheet_left: f64,
    /// Machine ticks one sheet lasts at output 1.
    pub time_per_sheet: f64,
    pub current_heat: f64,
    pub emagged: bool,
    pub panel_open: bool,
    pub layer: u8,
    /// Went over [`OVERHEAT`] this tick: explodes after the machine tick.
    pub overheated: bool,
    /// `<base>_0`, `<base>_1`, the emissive mask.
    sprites: [Option<IconStateId>; 3],
}

/// A PACMAN board's `high_production_profile` (absent: plasma mode).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PacmanBoard {
    pub high: bool,
}

impl Pacman {
    /// The kind of `path`: plasma, uranium (`super`) or wood.
    pub fn of_path(path: &str, layer: u8) -> Self {
        Self::of_kind(path, is_under(path, PACMAN_SUPER), layer)
    }

    /// `high`: the uranium numbers (`super`, or a board's `high_production_profile`).
    pub fn of_kind(path: &str, high: bool, layer: u8) -> Self {
        let (power_gen, max_sheets, time_per_sheet, sheet_path, sheet_name) = if high {
            (30_000.0, 20, 60.0, URANIUM_SHEET, "uranium")
        } else if is_under(path, PACMAN_WOOD) {
            (5_000.0, 75, 360.0, WOOD_SHEET, "wooden plank")
        } else {
            (10_000.0, 50, 180.0, PLASMA_SHEET, "solid plasma")
        };
        Self {
            active: false,
            power_gen,
            power_output: 1,
            sheets: if is_under(path, PACMAN_PRE_LOADED) {
                15
            } else {
                0
            },
            max_sheets,
            sheet_path,
            sheet_name,
            sheet_left: 0.0,
            time_per_sheet,
            current_heat: 0.0,
            emagged: false,
            panel_open: false,
            layer,
            overheated: false,
            sprites: [None; 3],
        }
    }

    fn per_tick(&self) -> f64 {
        1.0 / (self.time_per_sheet / f64::from(self.power_output))
    }

    /// `HasFuel`.
    pub fn has_fuel(&self) -> bool {
        f64::from(self.sheets) >= self.per_tick() - self.sheet_left
    }

    /// `TogglePower`.
    pub fn toggle_power(&mut self) {
        if self.active {
            self.active = false;
        } else if self.has_fuel() {
            self.active = true;
        }
    }

    /// `process` of one machine tick: the energy it made (`power_to_energy(power_gen *
    /// power_output)` over `dt` seconds), 0 when it did not run.
    pub fn process(&mut self, anchored: bool, dt: f64, rng: &mut CombatRng) -> f64 {
        if !self.active {
            // `handleInactive`.
            self.current_heat = (self.current_heat - 2.0).max(0.0);
            return 0.0;
        }
        if !self.has_fuel() || !anchored {
            self.toggle_power();
            return 0.0;
        }
        let energy = self.power_gen * f64::from(self.power_output) * dt;
        self.use_fuel(rng);
        energy
    }

    /// `UseFuel`: the burnt fraction off the current sheet, the next sheet started, the heat.
    /// DM's one-argument `round` floors.
    pub fn use_fuel(&mut self, rng: &mut CombatRng) {
        let mut needed = self.per_tick();
        let burnt = needed.min(self.sheet_left);
        needed -= burnt;
        self.sheet_left -= burnt;
        let whole = needed.floor();
        self.sheets = self.sheets.saturating_sub(whole as u32);
        needed -= whole;
        if self.sheet_left <= 0.0 && self.sheets > 0 {
            self.sheet_left = 1.0 - needed;
            self.sheets -= 1;
        }
        let output = f64::from(self.power_output);
        let lower = 56.0 + output * 10.0;
        let mut upper = 76.0 + output * 10.0;
        let mut bias = 0.0;
        if self.power_output > 4 {
            upper = 400.0;
            bias = output - 3.0;
        }
        if self.current_heat < lower {
            self.current_heat += 3.0;
        } else {
            // `rand(-7 + bias, 7 + bias)`.
            self.current_heat += f64::from(rng.range(0, 14)) - 7.0 + bias;
            self.current_heat = self.current_heat.clamp(lower, upper);
        }
        if self.current_heat > OVERHEAT {
            self.overheated = true;
        }
    }

    /// `ui_act` `lower_power` / `higher_power`.
    pub fn adjust_output(&mut self, up: bool) -> bool {
        if up && (self.power_output < 4 || self.emagged) {
            self.power_output += 1;
            true
        } else if !up && self.power_output > 1 {
            self.power_output -= 1;
            true
        } else {
            false
        }
    }
}

pub struct PacmanPlugin;

impl Plugin for PacmanPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `interact` (`ui_interact`) of an anchored generator.
        app.add_click_handler(ClickPhase::EmptyHand, 1950, |w, c| {
            interact(w, c.client, c.mob, c.target)
        });
        // tg `item_interaction` (fuel), `wrench_act`, `screwdriver_act`.
        app.add_click_handler(ClickPhase::HeldOnTarget, 2850, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, h, c.target))
        });
        app.register_ui::<PacmanWindow>()
            .add_systems(ss13_world::MapInit, init_pacmans.after(super::init_power))
            .add_systems(
                Update,
                (
                    pacman_acts.in_set(ss13_tgui::server::UiSystems::Handle),
                    (overheat, pacman_looks).chain().after(super::PowerSystems),
                ),
            );
    }
}

fn init_pacmans(world: &mut World) {
    let mut query = world
        .query_filtered::<(Entity, &DmPower, &TilePos, Option<&DmAtom>), With<ss13_world::Mapped>>(
        );
    let found: Vec<(Entity, String, TilePos, u8, Option<String>)> = query
        .iter(world)
        .filter(|(_, dm, ..)| is_under(&dm.path, PACMAN_ROOT))
        .map(|(e, dm, &pos, atom)| {
            (
                e,
                dm.path.clone(),
                pos,
                super::layer(dm),
                atom.map(|a| a.icon.clone()),
            )
        })
        .collect();
    for (e, path, pos, layer, icon) in found {
        let high = is_under(&path, PACMAN_SUPER);
        install(world, e, &path, high, pos, layer, icon);
    }
}

/// The live generator of `e`, with its sprites, on the powernet when anchored
/// (`pacman/Initialize`: `if(anchored) connect_to_network()`).
fn install(
    world: &mut World,
    e: Entity,
    path: &str,
    high: bool,
    pos: TilePos,
    layer: u8,
    icon: Option<String>,
) {
    let mut pacman = Pacman::of_kind(path, high, layer);
    let base = if high {
        "portgen1"
    } else if is_under(path, PACMAN_WOOD) {
        "portgen3"
    } else {
        "portgen0"
    };
    let mask = if is_under(path, PACMAN_WOOD) {
        "portgen3-emissive"
    } else {
        "portgen-emissive"
    };
    if let (Some(index), Some(icon)) = (world.get_resource::<IconIndex>(), icon) {
        pacman.sprites = [
            index.get(&icon, &format!("{base}_0")),
            index.get(&icon, &format!("{base}_1")),
            index.get(&icon, mask),
        ];
    }
    if world.get::<Anchored>(e).is_some_and(|a| a.0) {
        world
            .resource_mut::<Grid>()
            .add_node(e, pos, layer, NodeKind::Machine);
    }
    world.entity_mut(e).insert(pacman);
}

/// A generator built from its board (`crate::machine_frame::setup_built`): `on_construction`
/// takes the uranium numbers from a board set to them.
pub fn setup(world: &mut World, e: Entity) {
    let (Some(dm), Some(&pos)) = (world.get::<DmPower>(e).cloned(), world.get::<TilePos>(e)) else {
        return;
    };
    let high = world
        .get::<crate::stock_parts::ComponentParts>(e)
        .is_some_and(|parts| {
            parts
                .parts()
                .iter()
                .any(|&p| world.get::<PacmanBoard>(p).is_some_and(|b| b.high))
        });
    let icon = world.get::<DmAtom>(e).map(|a| a.icon.clone());
    install(world, e, &dm.path, high, pos, super::layer(&dm), icon);
}

/// `pacman/on_deconstruction` (from `crate::machine_frame::on_deconstruction`): `DropFuel`, off
/// the powernet.
pub fn on_deconstruction(world: &mut World, e: Entity, pos: TilePos) {
    let Some(sheets) = world.get_mut::<Pacman>(e).map(|mut p| {
        p.active = false;
        (p.sheet_path, std::mem::take(&mut p.sheets))
    }) else {
        return;
    };
    world.resource_mut::<Grid>().remove_node(e);
    if sheets.1 > 0 {
        crate::construction::spawn_stack(world, sheets.0, pos, sheets.1);
    }
}

/// `pacman/emag_act` (from `crate::emag`): output levels above 4 unlocked. True when it took.
pub fn emag_act(world: &mut World, client: Entity, e: Entity) -> bool {
    let Some(mut p) = world.get_mut::<Pacman>(e) else {
        return false;
    };
    if p.emagged {
        return false;
    }
    p.emagged = true;
    tell(
        world,
        client,
        Span::Notice,
        "максимальная мощность разблокирована",
    );
    true
}

/// `overheat`: the explosion, the generator gone.
fn overheat(world: &mut World) {
    let mut query = world.query::<(Entity, &Pacman, &TilePos)>();
    let blown: Vec<(Entity, TilePos)> = query
        .iter(world)
        .filter(|(_, p, _)| p.overheated)
        .map(|(e, _, &pos)| (e, pos))
        .collect();
    for (e, pos) in blown {
        info!(target: "engine", "P.A.C.M.A.N. {e:?} overheated at {pos:?}");
        world.resource_mut::<Grid>().remove_node(e);
        world.despawn(e);
        let mut ex = crate::explosions::Explosion::new(2, 5, 2).flash(-1);
        ex.cause = "pacman overheat".into();
        crate::explosions::explosion(world, pos, ex);
    }
}

/// `update_icon_state` (`<base>_<active>`) and the emissive while running with the panel shut.
fn pacman_looks(mut pacmans: Query<(&Pacman, &mut Appearance), Changed<Pacman>>) {
    for (p, mut appearance) in &mut pacmans {
        if let Some(state) = p.sprites[usize::from(p.active)]
            && appearance.icon_state != state
        {
            appearance.icon_state = state;
        }
        let overlays: Vec<Overlay> = p.sprites[2]
            .filter(|_| p.active && !p.panel_open)
            .map(|state| Overlay {
                plane: Some(EMISSIVE_PLANE),
                ..Overlay::new(state)
            })
            .into_iter()
            .collect();
        if appearance.overlays != overlays {
            appearance.overlays = overlays;
        }
    }
}

use crate::obj_damage::anchored;

/// An empty hand opens the window of an anchored generator (`INTERACT_ATOM_REQUIRES_ANCHORED`).
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<Pacman>(target).is_none() {
        return false;
    }
    if !anchored(world, target) {
        return true;
    }
    let title = world
        .get::<ss13_mapload::DmName>(target)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<PacmanWindow>(client, mob, target, title));
    true
}

/// `item_interaction` with fuel sheets, `wrench_act`, `screwdriver_act`. True when handled.
pub fn item_interaction(world: &mut World, client: Entity, held: Entity, target: Entity) -> bool {
    if board_screwdriver(world, client, held, target) {
        return true;
    }
    let Some(p) = world.get::<Pacman>(target).cloned() else {
        return false;
    };
    let name = crate::security::name_of(world, target);
    if world
        .get::<DmItem>(held)
        .is_some_and(|i| is_under(&i.path, p.sheet_path))
    {
        let have = crate::tools::stack_amount(world, held);
        let amount = (p.max_sheets - p.sheets.min(p.max_sheets)).min(have);
        if amount < 1 {
            tell(world, client, Span::Notice, format!("{name} полон!"));
            return true;
        }
        if crate::tools::use_stack(world, held, amount)
            && let Some(mut p) = world.get_mut::<Pacman>(target)
        {
            p.sheets += amount;
        }
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы добавляете {amount} листов в {name}."),
        );
        return true;
    }
    let Some((tool, _)) = crate::tools::tool_of(world, held) else {
        return false;
    };
    // Both return `NONE` while it runs: the click goes on down the chain.
    if p.active {
        return false;
    }
    match tool {
        ToolKind::Wrench => {
            let Some(&pos) = world.get::<TilePos>(target) else {
                return true;
            };
            let was = anchored(world, target);
            set_anchored(world, target, !was);
            // `pacman/set_anchored`: `connect_to_network` / `disconnect_from_network`.
            {
                let mut grid = world.resource_mut::<Grid>();
                if was {
                    grid.remove_node(target);
                } else {
                    grid.add_node(target, pos, p.layer, NodeKind::Machine);
                }
            }
            if was {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы откручиваете генератор от пола.",
                );
                crate::sound::playsound(
                    world,
                    crate::sound::PlaySound::new(
                        pos,
                        ss13_mapload::sound::files::DECONSTRUCT,
                        50.0,
                        true,
                    ),
                );
            } else {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы прикручиваете генератор к полу.",
                );
            }
            true
        }
        ToolKind::Screwdriver => {
            let open = !p.panel_open;
            if let Some(mut p) = world.get_mut::<Pacman>(target) {
                p.panel_open = open;
            }
            let text = if open {
                "Вы открываете панель доступа."
            } else {
                "Вы закрываете панель доступа."
            };
            tell(world, client, Span::Notice, text);
            true
        }
        _ => false,
    }
}

/// `circuitboard/machine/pacman/screwdriver_act`: plasma or uranium mode.
fn board_screwdriver(world: &mut World, client: Entity, held: Entity, target: Entity) -> bool {
    let is_board = world
        .get::<DmItem>(target)
        .is_some_and(|i| is_under(&i.path, PACMAN_BOARD));
    if !is_board || crate::tools::tool_of(world, held).map(|t| t.0) != Some(ToolKind::Screwdriver) {
        return false;
    }
    let high = !world.get::<PacmanBoard>(target).is_some_and(|b| b.high);
    world.entity_mut(target).insert(PacmanBoard { high });
    let mode = if high {
        "мощный урановый режим"
    } else {
        "средний плазменный режим"
    };
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы переключаете плату в {mode}."),
    );
    true
}

pub struct PacmanWindow;

impl UiInterface for PacmanWindow {
    const NAME: &'static str = PortableGeneratorUi::NAME;
    type Data = PortableGeneratorData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PacmanAct {
    TogglePower,
    Eject,
    LowerPower,
    HigherPower,
}

use ss13_chat::capitalize;

impl ServerUi for PacmanWindow {
    type Source = (Entity, &'static Pacman, Option<&'static Anchored>);
    type UserParam = Res<'static, Grid>;
    type Act = PacmanAct;

    fn static_data(_: (Entity, &Pacman, Option<&Anchored>)) {}

    fn data(
        (e, p, anchored): (Entity, &Pacman, Option<&Anchored>),
        _: Entity,
        grid: &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> PortableGeneratorData {
        let anchored = anchored.is_some_and(|a| a.0);
        let net = grid.node_net(e).and_then(|n| grid.net(n));
        PortableGeneratorData {
            active: p.active,
            sheet_name: capitalize(p.sheet_name),
            sheets: p.sheets,
            stack_percent: ((p.sheet_left * 1000.0).round() / 10.0) as f32,
            anchored,
            connected: net.is_some(),
            ready_to_boot: anchored && p.has_fuel(),
            power_generated: p.power_gen as f32,
            power_output: (p.power_gen * f64::from(p.power_output)) as f32,
            // `display_power(avail())`: the net's energy per tick as watts.
            power_available: net.map_or(0.0, |n| (n.avail / super::MACHINE_TICK_SECS) as f32),
            current_heat: p.current_heat as f32,
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<PacmanAct, UiReject> {
        match action {
            "toggle_power" => Ok(PacmanAct::TogglePower),
            "eject" => Ok(PacmanAct::Eject),
            "lower_power" => Ok(PacmanAct::LowerPower),
            "higher_power" => Ok(PacmanAct::HigherPower),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// `ui_act`, against the generator's state now (the window's status already checked the user).
fn pacman_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<PacmanWindow>>>) {
    let pending: Vec<(Entity, PacmanAct)> = acts
        .read(world.resource::<Messages<UiActed<PacmanWindow>>>())
        .map(|a| (a.source, a.act))
        .collect();
    for (source, act) in pending {
        // `DropFuel` while off: the sheets onto its tile.
        let dropped = world.get_mut::<Pacman>(source).and_then(|mut p| match act {
            PacmanAct::TogglePower => {
                p.toggle_power();
                None
            }
            PacmanAct::LowerPower => {
                p.adjust_output(false);
                None
            }
            PacmanAct::HigherPower => {
                p.adjust_output(true);
                None
            }
            PacmanAct::Eject if !p.active && p.sheets > 0 => {
                let sheets = std::mem::take(&mut p.sheets);
                Some((p.sheet_path, sheets))
            }
            PacmanAct::Eject => None,
        });
        if let Some((path, sheets)) = dropped
            && let Some(&pos) = world.get::<TilePos>(source)
        {
            crate::construction::spawn_stack(world, path, pos, sheets);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plasma_pacman_burns_a_sheet_per_180_ticks_at_output_one() {
        let mut rng = CombatRng(3);
        let mut p = Pacman::of_path(PACMAN_PRE_LOADED, 2);
        assert_eq!((p.sheets, p.max_sheets, p.power_gen), (15, 50, 10_000.0));
        p.toggle_power();
        assert!(p.active);
        // The first tick starts a sheet: 15 -> 14 with 179/180 of it left.
        let joules = p.process(true, 2.0, &mut rng);
        assert_eq!(joules, 20_000.0);
        assert_eq!(p.sheets, 14);
        assert!((p.sheet_left - 179.0 / 180.0).abs() < 1e-9);
        for _ in 0..178 {
            p.process(true, 2.0, &mut rng);
        }
        assert_eq!(p.sheets, 14, "the first sheet lasts 180 ticks");
        p.process(true, 2.0, &mut rng);
        p.process(true, 2.0, &mut rng);
        assert_eq!(p.sheets, 13, "the next sheet starts");
        // Heat climbs by 3 to 66 and then stays within 66..86.
        assert!((66.0..=86.0).contains(&p.current_heat));
        assert!(!p.overheated);
        // Unanchored, it stops; stopped, it cools.
        assert_eq!(p.process(false, 2.0, &mut rng), 0.0);
        assert!(!p.active);
        let heat = p.current_heat;
        p.process(false, 2.0, &mut rng);
        assert_eq!(p.current_heat, heat - 2.0);
    }

    #[test]
    fn output_stays_within_one_to_four_unless_emagged_and_overheats_past_300() {
        let mut p = Pacman::of_path(PACMAN_SUPER, 2);
        assert_eq!((p.sheet_path, p.max_sheets), (URANIUM_SHEET, 20));
        assert!(!p.adjust_output(false));
        for _ in 0..3 {
            assert!(p.adjust_output(true));
        }
        assert!(!p.adjust_output(true));
        p.emagged = true;
        assert!(p.adjust_output(true));
        assert_eq!(p.power_output, 5);
        // Output 5 heats up to 400 with a +2 bias: it blows up eventually.
        let mut rng = CombatRng(11);
        p.sheets = 20;
        p.current_heat = 290.0;
        p.toggle_power();
        for _ in 0..200 {
            p.process(true, 2.0, &mut rng);
            if p.overheated {
                break;
            }
        }
        assert!(p.overheated);
        // No fuel: it does not start.
        let mut empty = Pacman::of_path(PACMAN_ROOT, 2);
        empty.toggle_power();
        assert!(!empty.active);
    }

    #[test]
    fn a_board_in_uranium_mode_builds_the_super_numbers() {
        let p = Pacman::of_kind(PACMAN_ROOT, true, 2);
        assert_eq!(
            (p.power_gen, p.max_sheets, p.time_per_sheet, p.sheet_path),
            (30_000.0, 20, 60.0, URANIUM_SHEET)
        );
        let wood = Pacman::of_kind(PACMAN_WOOD, false, 2);
        assert_eq!((wood.sheet_path, wood.max_sheets), (WOOD_SHEET, 75));
    }
}
