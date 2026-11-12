//! The SMES window, look and building (server): tg `/obj/machinery/power/smes` `ui_act`,
//! `update_overlays`, `item_interaction` (terminal), `wirecutter_act`, `crowbar_act`,
//! `RefreshParts`, `on_construction` (`code/modules/power/smes.dm`, `terminal.dm`). Charging and
//! output are [`super::model::Smes`].
//!
//! - An empty hand on a SMES with a terminal opens the `Smes` window: input auto/off, output
//!   on/off, both levels (min, max, ±10 kW, a value), clamped to `0..=*_level_max`.
//! - Look: `smes`/`smes-o` (panel open, `crate::machine_frame` screwdriver); with the panel shut
//!   and a terminal, `smes-op<outputting>`, `smes-oc<inputting>`, `smes-og<1..5>` (charge
//!   display) and the emissive `smes-emissive`.
//! - 10 cable on an open SMES from a cardinal tile with the floor up (2 s) lays a terminal on the
//!   user's tile facing the SMES; 50% to be shocked from the net there instead (the cable stays).
//!   Wirecutters on an open SMES or on its terminal (5 s) cut the terminal back into 10 cable,
//!   with the same shock chance. A crowbar takes it apart only without a terminal
//!   (`crate::machine_frame` family `Smes`); the charge goes into its batteries.
//! - Built from its board: capacity from its batteries, levels `200 kW × capacitor tiers`, the
//!   charge its batteries hold, the first free terminal next to it that faces it.
// shortcut: no right-click cable layer choice (the coil's default layer 2), no wrench rotation
// (`default_change_direction_wrench`), no multitool layer change, no admin log of the
// deconstruction; a terminal laid next to cables already linked to the SMES tile does not
// split them (tg `connect_cable` runs only on placement either).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::power::TERMINAL_ROOT;
use ss13_mapload::{DmAtom, DmItem, DmPower};
use ss13_shared::appearance::Overlay;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, EMISSIVE_PLANE, Facing, IconStateId, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::smes::{SmesData, SmesStatic, SmesUi};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject};

use super::model::SMES_LEVEL_MAX;
use super::{Grid, MACHINE_TICK_SECS, NodeKind, Smes};
use crate::combat::CombatRng;
use crate::construction::{Job, PanelOpen};
use crate::mecha::core::ItemCell;
use crate::stock_parts::{ComponentParts, PartKind};
use crate::tools::{Actor, DoAfter, ToolKind, tell};

use ss13_mapload::circuit::CABLE_COIL;
/// Cable a terminal takes and gives back.
pub const TERMINAL_CABLE: u32 = 10;
/// `do_after(user, 2 SECONDS)`, `use_tool(src, user, 50)`: deciseconds.
const LAY_DS: f32 = 20.0;
const CUT_DS: f32 = 50.0;
/// `CABLE_LAYER_2`: a coil's default `target_layer`.
const COIL_LAYER: u8 = 2;
/// `UNDERFLOOR_INTERACTABLE`.
const UNDERFLOOR_INTERACTABLE: u8 = 2;
const DECONSTRUCT_SOUND: &str = ss13_mapload::sound::files::DECONSTRUCT;

/// The SMES's overlay states, resolved once.
#[derive(Component, Clone, Debug, Default)]
pub struct SmesSprites {
    base: Option<IconStateId>,
    open: Option<IconStateId>,
    op: [Option<IconStateId>; 2],
    oc: [Option<IconStateId>; 2],
    og: [Option<IconStateId>; 5],
    emissive: Option<IconStateId>,
    /// A connector's `bp-c`, `bp-o`, `bp-d` (`super::smes_portable`).
    bp: [Option<IconStateId>; 3],
}

/// A timed step of the terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmesJob {
    /// Cable on the SMES from the user's tile.
    Lay {
        from: TilePos,
    },
    /// Wirecutters on the terminal (the job's target).
    Cut,
    /// A wrench on a portable bank (the job's target): onto `connector`, or off its connector
    /// (`super::smes_portable`).
    BankConnect {
        connector: Entity,
    },
    BankDisconnect,
}

pub struct SmesPlugin;

impl Plugin for SmesPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // `ui_interact` of an operational SMES.
        app.add_click_handler(ClickPhase::EmptyHand, 1960, |w, c| {
            interact(w, c.client, c.mob, c.target)
        });
        // `item_interaction` (cable), `wirecutter_act` of the SMES and of its terminal.
        app.add_click_handler(ClickPhase::HeldOnTarget, 2860, |w, c| {
            c.held.is_some_and(|h| {
                item_interaction(
                    w,
                    Actor {
                        client: c.client,
                        mob: c.mob,
                        item: Some(h),
                    },
                    c.target,
                )
            })
        });
        app.register_ui::<SmesWindow>()
            .add_systems(ss13_world::MapInit, init_sprites.after(super::init_power))
            .add_systems(
                Update,
                (
                    smes_acts.in_set(ss13_tgui::server::UiSystems::Handle),
                    smes_looks.after(super::PowerSystems),
                ),
            );
    }
}

fn sprites_for(world: &World, e: Entity) -> SmesSprites {
    let (Some(index), Some(icon)) = (
        world.get_resource::<IconIndex>(),
        world.get::<DmAtom>(e).map(|a| a.icon.clone()),
    ) else {
        return SmesSprites::default();
    };
    let get = |s: &str| index.get(&icon, s);
    let connector = world
        .get::<DmPower>(e)
        .is_some_and(|d| is_under(&d.path, super::smes_portable::CONNECTOR));
    if connector {
        // `show_display_lights = FALSE`: only its own state and the bank overlays.
        return SmesSprites {
            base: get("battery_port"),
            open: get("battery_port-o"),
            bp: [get("bp-c"), get("bp-o"), get("bp-d")],
            ..SmesSprites::default()
        };
    }
    SmesSprites {
        bp: [None; 3],
        base: get("smes"),
        open: get("smes-o"),
        op: [get("smes-op0"), get("smes-op1")],
        oc: [get("smes-oc0"), get("smes-oc1")],
        og: std::array::from_fn(|i| get(&format!("smes-og{}", i + 1))),
        emissive: get("smes-emissive"),
    }
}

fn init_sprites(world: &mut World) {
    let mut q = world.query_filtered::<Entity, (With<Smes>, With<ss13_world::Mapped>)>();
    let all: Vec<Entity> = q.iter(world).collect();
    for e in all {
        let sprites = sprites_for(world, e);
        world.entity_mut(e).insert(sprites);
        super::smes_portable::setup_connector(world, e);
    }
}

/// `update_icon_state`, `update_overlays`.
#[allow(clippy::type_complexity)]
fn smes_looks(
    mut q: Query<
        (
            &Smes,
            Option<&PanelOpen>,
            &SmesSprites,
            &mut Appearance,
            Option<&super::smes_portable::SmesConnector>,
        ),
        Or<(Changed<Smes>, Changed<PanelOpen>, Added<SmesSprites>)>,
    >,
) {
    for (s, panel, sprites, mut appearance, connector) in &mut q {
        let open = panel.is_some_and(|p| p.0);
        if let Some(state) = if open { sprites.open } else { sprites.base }
            && appearance.icon_state != state
        {
            appearance.icon_state = state;
        }
        let mut overlays = Vec::new();
        if !open && s.terminal.is_some() {
            let level = s.charge_display();
            let states = [
                sprites.op[usize::from(s.outputting)],
                sprites.oc[usize::from(s.inputting)],
                (level > 0)
                    .then(|| sprites.og[usize::from(level - 1)])
                    .flatten(),
            ];
            overlays.extend(states.into_iter().flatten().map(Overlay::new));
            overlays.extend(sprites.emissive.map(|state| Overlay {
                plane: Some(EMISSIVE_PLANE),
                ..Overlay::new(state)
            }));
        }
        // `connector/update_overlays`: a bank on it.
        if connector.is_some_and(|c| c.bank.is_some()) {
            let bp = if s.inputting {
                sprites.bp[0]
            } else if s.charge > 0.0 {
                sprites.bp[1]
            } else {
                sprites.bp[2]
            };
            overlays.extend(bp.map(Overlay::new));
        }
        if appearance.overlays != overlays {
            appearance.overlays = overlays;
        }
    }
}

use crate::construction::panel_open;

/// An empty hand opens the window of a SMES that works (a broken one, without a terminal,
/// cannot be interacted with).
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(s) = world.get::<Smes>(target) else {
        return false;
    };
    if s.terminal.is_none() || super::smes_portable::connector_refuses_ui(world, mob, target) {
        return true;
    }
    let title = crate::security::name_of(world, target);
    world.write_message(OpenUi::new::<SmesWindow>(client, mob, target, title));
    true
}

/// The SMES whose terminal `terminal` is.
fn master_of(world: &mut World, terminal: Entity) -> Option<Entity> {
    let mut q = world.query::<(Entity, &Smes)>();
    q.iter(world)
        .find(|(_, s)| s.terminal == Some(terminal))
        .map(|(e, _)| e)
}

fn is_coil(world: &World, item: Entity) -> bool {
    world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, CABLE_COIL))
}

/// `can_place_terminal`: the warning when it cannot.
fn cannot_lay(
    world: &World,
    smes: Entity,
    user_pos: TilePos,
    coil: Entity,
) -> Option<&'static str> {
    let (Some(s), Some(&pos)) = (world.get::<Smes>(smes), world.get::<TilePos>(smes)) else {
        return Some("");
    };
    if Dir::CARDINALS.into_iter().all(|d| user_pos.step(d) != pos) {
        return Some("");
    }
    if !panel_open(world, smes) {
        return Some("сначала откройте панель обслуживания!");
    }
    if ss13_mapload::undertile::accessibility(world, user_pos) < UNDERFLOOR_INTERACTABLE {
        return Some("сначала снимите плитку пола!");
    }
    if s.terminal.is_some() {
        return Some("клемма уже подключена!");
    }
    if crate::tools::stack_amount(world, coil) < TERMINAL_CABLE {
        return Some("нужно десять отрезков кабеля!");
    }
    None
}

/// Cable on a SMES, wirecutters on a SMES or its terminal. True when handled.
pub fn item_interaction(world: &mut World, actor: Actor, target: Entity) -> bool {
    let Some(held) = actor.item else {
        return false;
    };
    let client = actor.client;
    let is_smes = world.get::<Smes>(target).is_some();
    if is_smes && is_coil(world, held) {
        let Some(&user_pos) = world.get::<TilePos>(actor.mob) else {
            return true;
        };
        if let Some(why) = cannot_lay(world, target, user_pos, held) {
            if !why.is_empty() {
                tell(world, client, Span::Warning, why);
            }
            return true;
        }
        if crate::tools::do_after(
            world,
            actor,
            target,
            LAY_DS,
            TERMINAL_CABLE,
            Job::Smes(SmesJob::Lay { from: user_pos }),
        ) {
            tell(
                world,
                client,
                Span::Notice,
                "Вы начинаете прокладывать кабель...",
            );
            if let Some(&pos) = world.get::<TilePos>(target) {
                crate::sound::playsound(
                    world,
                    crate::sound::PlaySound::new(pos, DECONSTRUCT_SOUND, 50.0, true),
                );
            }
        }
        return true;
    }
    if crate::tools::tool_of(world, held).map(|t| t.0) != Some(ToolKind::Wirecutters) {
        return false;
    }
    let (smes, terminal) = if is_smes {
        // `wirecutter_act`: `ITEM_INTERACT_BLOCKING` without an open panel and a terminal.
        let Some(t) = world.get::<Smes>(target).and_then(|s| s.terminal) else {
            return true;
        };
        (target, t)
    } else if world
        .get::<DmPower>(target)
        .is_some_and(|d| is_under(&d.path, TERMINAL_ROOT))
    {
        let Some(smes) = master_of(world, target) else {
            return false;
        };
        (smes, target)
    } else {
        return false;
    };
    // `can_terminal_dismantle`.
    if !panel_open(world, smes) {
        return true;
    }
    let Some(&tpos) = world.get::<TilePos>(terminal) else {
        return true;
    };
    if ss13_mapload::undertile::accessibility(world, tpos) < UNDERFLOOR_INTERACTABLE {
        tell(
            world,
            client,
            Span::Warning,
            "необходимо обнажить клемму кабеля!",
        );
        return true;
    }
    if crate::tools::use_tool(world, actor, terminal, CUT_DS, 0, Job::Smes(SmesJob::Cut)) {
        tell(world, client, Span::Notice, "Вы перерезаете провода...");
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(tpos, DECONSTRUCT_SOUND, 50.0, true),
        );
    }
    true
}

/// `if(prob(50) && electrocute_mob(user, powernet, terminal, 1, TRUE)) do_sparks(5, TRUE, ...)`.
fn shocked(world: &mut World, mob: Entity, terminal: Entity, sparks_at: Entity) -> bool {
    if !world.resource_mut::<CombatRng>().chance(50.0)
        || !crate::wires::electrocute_from_node(world, mob, terminal, terminal)
    {
        return false;
    }
    if let Some(&pos) = world.get::<TilePos>(sparks_at) {
        crate::effects::particles::do_sparks(world, 5, true, pos);
    }
    true
}

fn drop_terminal(world: &mut World, terminal: Entity) {
    world.resource_mut::<Grid>().remove_node(terminal);
    let pos = world.get::<TilePos>(terminal).copied();
    world.despawn(terminal);
    if let Some(pos) = pos {
        crate::construction::tile_changed(world, pos);
    }
}

/// The end of a [`SmesJob`], against the state now.
pub fn complete(
    world: &mut World,
    mob: Entity,
    action: &DoAfter,
    job: SmesJob,
    consume: impl FnOnce(&mut World) -> bool,
) {
    let client = action.client;
    match job {
        SmesJob::Lay { from } => {
            let smes = action.target;
            let (Some(coil), Some(&user_pos), Some(&pos)) = (
                action.item,
                world.get::<TilePos>(mob),
                world.get::<TilePos>(smes),
            ) else {
                return;
            };
            if user_pos != from || cannot_lay(world, smes, user_pos, coil).is_some() {
                return;
            }
            let Some(dir) = Dir::CARDINALS
                .into_iter()
                .find(|&d| user_pos.step(d) == pos)
            else {
                return;
            };
            let Some(terminal) = crate::construction::spawn_obj(world, TERMINAL_ROOT, user_pos)
            else {
                return;
            };
            world.entity_mut(terminal).insert(Facing(dir));
            world.resource_mut::<Grid>().add_node(
                terminal,
                user_pos,
                COIL_LAYER,
                NodeKind::Terminal,
            );
            if shocked(world, mob, terminal, smes) || !consume(world) {
                drop_terminal(world, terminal);
                return;
            }
            if let Some(mut s) = world.get_mut::<Smes>(smes) {
                s.terminal = Some(terminal);
            }
            tell(world, client, Span::Notice, "Вы подключаете кабели.");
        }
        SmesJob::Cut => {
            let terminal = action.target;
            let Some(smes) = master_of(world, terminal) else {
                return;
            };
            if !panel_open(world, smes) || shocked(world, mob, terminal, smes) {
                return;
            }
            let Some(&pos) = world.get::<TilePos>(terminal) else {
                return;
            };
            crate::construction::spawn_stack(world, CABLE_COIL, pos, TERMINAL_CABLE);
            // `Destroy` -> `master.disconnect_terminal()`: broken until a new terminal.
            if let Some(mut s) = world.get_mut::<Smes>(smes) {
                s.terminal = None;
                s.inputting = false;
                s.outputting = false;
            }
            drop_terminal(world, terminal);
            tell(world, client, Span::Notice, "клемма разобрана");
        }
        SmesJob::BankConnect { .. } | SmesJob::BankDisconnect => {
            super::smes_portable::complete(world, mob, action, job);
        }
    }
}

/// `crowbar_act`: refused while a terminal is there. True when refused (told).
pub fn crowbar_refused(world: &mut World, client: Entity, smes: Entity) -> bool {
    if world
        .get::<Smes>(smes)
        .is_some_and(|s| s.terminal.is_some())
    {
        tell(world, client, Span::Warning, "сначала снимите клемму!");
        return true;
    }
    false
}

/// Battery parts of a SMES with their charge state.
fn batteries(world: &World, e: Entity) -> Vec<(Entity, ItemCell)> {
    world
        .get::<ComponentParts>(e)
        .map(|parts| {
            parts
                .parts()
                .iter()
                .filter_map(|&p| world.get::<ItemCell>(p).map(|c| (p, *c)))
                .collect()
        })
        .unwrap_or_default()
}

/// `RefreshParts` (from `crate::stock_parts::refresh`): capacity from the batteries, level caps
/// from the capacitors; levels and charge clamped to them.
pub fn refresh_parts(world: &mut World, e: Entity) {
    if world.get::<Smes>(e).is_none() {
        return;
    }
    let r = crate::stock_parts::ratings(world, e);
    let capacity: f64 = r.cells.iter().sum();
    let coefficient = f64::from(r.sum(PartKind::Capacitor));
    if let Some(mut s) = world.get_mut::<Smes>(e) {
        if capacity > 0.0 {
            s.capacity = capacity;
        }
        s.input_level_max = SMES_LEVEL_MAX * coefficient;
        s.output_level_max = SMES_LEVEL_MAX * coefficient;
        s.input_level = s.input_level.min(s.input_level_max);
        s.output_level = s.output_level.min(s.output_level_max);
        s.charge = s.charge.min(s.capacity);
    }
}

/// A SMES built from its board (`crate::machine_frame::setup_built`): the state map load gives
/// one, the charge its batteries hold, its net node and the first free terminal facing it.
pub fn setup(world: &mut World, e: Entity) {
    let (Some(dm), Some(&pos)) = (world.get::<DmPower>(e).cloned(), world.get::<TilePos>(e)) else {
        return;
    };
    let charge: f64 = batteries(world, e).iter().map(|(_, c)| c.0.charge).sum();
    let mut terminals =
        world.query::<(Entity, &DmPower, &TilePos, Option<&Facing>, Option<&DmAtom>)>();
    let candidates: Vec<(Entity, TilePos, Option<Dir>, u8)> = terminals
        .iter(world)
        .filter(|(_, d, ..)| is_under(&d.path, TERMINAL_ROOT))
        .filter(|(_, _, p, ..)| p.z == pos.z && (p.x - pos.x).abs() + (p.y - pos.y).abs() == 1)
        .map(|(t, _, &p, f, a)| (t, p, f.map(|f| f.0), a.map_or(2, |a| a.dir)))
        .collect();
    let mut masters = world.query::<&Smes>();
    let taken: Vec<Entity> = masters.iter(world).filter_map(|s| s.terminal).collect();
    let terminal = Dir::CARDINALS.into_iter().find_map(|d| {
        let at = pos.step(d);
        candidates
            .iter()
            .find(|(t, p, facing, dir)| {
                *p == at
                    && !taken.contains(t)
                    && facing.map_or(*dir == d.reverse() as u8, |f| f == d.reverse())
            })
            .map(|c| c.0)
    });
    let s = Smes {
        charge: 0.0,
        capacity: dm.capacity,
        input_level: dm.input_level,
        output_level: dm.output_level,
        input_level_max: SMES_LEVEL_MAX,
        output_level_max: SMES_LEVEL_MAX,
        input_attempt: dm.input_attempt,
        output_attempt: dm.output_attempt,
        inputting: false,
        outputting: false,
        input_available: 0.0,
        output_used: 0.0,
        terminal,
    };
    let sprites = sprites_for(world, e);
    world.entity_mut(e).insert((s, sprites));
    refresh_parts(world, e);
    if let Some(mut s) = world.get_mut::<Smes>(e) {
        s.charge = charge.min(s.capacity);
    }
    world
        .resource_mut::<Grid>()
        .add_node(e, pos, super::layer(&dm), NodeKind::Smes);
    super::smes_portable::setup_connector(world, e);
}

/// Taken apart (`crate::machine_frame::on_deconstruction`): the charge back into its batteries
/// (tg keeps it there all along), off the net.
pub fn on_deconstruction(world: &mut World, e: Entity) {
    let Some(mut left) = world.get::<Smes>(e).map(|s| s.charge) else {
        return;
    };
    for (cell, mut state) in batteries(world, e) {
        let give = left.min(state.0.maxcharge).max(0.0);
        state.0.charge = give;
        left -= give;
        world.entity_mut(cell).insert(state);
    }
    world.resource_mut::<Grid>().remove_node(e);
}

pub struct SmesWindow;

impl UiInterface for SmesWindow {
    const NAME: &'static str = SmesUi::NAME;
    type Data = SmesData;
    type StaticData = SmesStatic;
}

/// A level act: `target=min|max|<watts>` or `adjust=<watts>`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Level {
    Min,
    Max,
    Adjust(f64),
    Set(f64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SmesAct {
    TryInput,
    TryOutput,
    Input(Level),
    Output(Level),
}

/// `ui_act` `input`/`output` params, in tg's order: `min`, `max`, a nonzero `adjust`, a number.
fn parse_level(params: &UiParams) -> Result<Level, UiReject> {
    match params.get("target") {
        Some("min") => return Ok(Level::Min),
        Some("max") => return Ok(Level::Max),
        _ => {}
    }
    if let Ok(adjust) = params.number("adjust")
        && adjust != 0.0
    {
        return Ok(Level::Adjust(adjust));
    }
    params.number("target").map(Level::Set)
}

impl Level {
    /// The new level, clamped to `0..=max`.
    pub fn apply(self, current: f64, max: f64) -> f64 {
        let target = match self {
            Level::Min => 0.0,
            Level::Max => max,
            Level::Adjust(d) => current + d,
            Level::Set(v) => v,
        };
        target.clamp(0.0, max.max(0.0))
    }
}

impl ServerUi for SmesWindow {
    type Source = &'static Smes;
    type UserParam = ();
    type Act = SmesAct;

    fn static_data(s: &Smes) -> SmesStatic {
        SmesStatic {
            capacity: s.capacity,
            input_level_max: s.input_level_max,
            output_level_max: s.output_level_max,
        }
    }

    fn data(s: &Smes, _: Entity, _: &()) -> SmesData {
        SmesData {
            charge: s.charge,
            input_attempt: s.input_attempt,
            inputting: s.inputting,
            input_level: s.input_level,
            // `energy_to_power`: joules per machine tick as watts.
            input_available: s.input_available / MACHINE_TICK_SECS,
            output_attempt: s.output_attempt,
            output_level: s.output_level,
            output_used: s.output_used / MACHINE_TICK_SECS,
            outputting: s.outputting,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<SmesAct, UiReject> {
        match action {
            "tryinput" => Ok(SmesAct::TryInput),
            "tryoutput" => Ok(SmesAct::TryOutput),
            "input" => parse_level(params).map(SmesAct::Input),
            "output" => parse_level(params).map(SmesAct::Output),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// `ui_act`, applied to the SMES as it is now.
fn smes_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<SmesWindow>>>) {
    let pending: Vec<(Entity, Entity, SmesAct)> = acts
        .read(world.resource::<Messages<UiActed<SmesWindow>>>())
        .map(|a| (a.source, a.actor, a.act))
        .collect();
    for (source, actor, act) in pending {
        let Some(mut s) = world.get_mut::<Smes>(source) else {
            continue;
        };
        match act {
            SmesAct::TryInput => s.input_attempt = !s.input_attempt,
            SmesAct::TryOutput => s.output_attempt = !s.output_attempt,
            SmesAct::Input(level) => s.input_level = level.apply(s.input_level, s.input_level_max),
            SmesAct::Output(level) => {
                s.output_level = level.apply(s.output_level, s.output_level_max)
            }
        }
        // `log_smes`.
        let (input, output, charge) = (s.input_level, s.output_level, s.charge);
        info!(
            target: "engine",
            "SMES {source:?} input/output {input}/{output}, charge {charge}, by {actor:?}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_clamp_to_the_caps() {
        assert_eq!(Level::Min.apply(50_000.0, 200_000.0), 0.0);
        assert_eq!(Level::Max.apply(50_000.0, 200_000.0), 200_000.0);
        assert_eq!(Level::Adjust(-10_000.0).apply(5_000.0, 200_000.0), 0.0);
        assert_eq!(
            Level::Adjust(10_000.0).apply(195_000.0, 200_000.0),
            200_000.0
        );
        assert_eq!(Level::Set(90_000.0).apply(0.0, 200_000.0), 90_000.0);
        assert_eq!(Level::Set(1e9).apply(0.0, 400_000.0), 400_000.0);
    }

    #[test]
    fn level_params_follow_tg_order() {
        let p = |v: &[(&str, &str)]| -> Vec<(String, String)> {
            v.iter().map(|(a, b)| ((*a).into(), (*b).into())).collect()
        };
        let min = p(&[("target", "min")]);
        assert_eq!(parse_level(&UiParams::new(&min)), Ok(Level::Min));
        let adjust = p(&[("adjust", "-10000")]);
        assert_eq!(
            parse_level(&UiParams::new(&adjust)),
            Ok(Level::Adjust(-10_000.0))
        );
        let set = p(&[("target", "75000")]);
        assert_eq!(parse_level(&UiParams::new(&set)), Ok(Level::Set(75_000.0)));
        let bad = p(&[("target", "lots")]);
        assert!(parse_level(&UiParams::new(&bad)).is_err());
    }
}
