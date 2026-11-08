//! Integrated circuits in the world (server), after tg `integrated_circuit.dm`,
//! `datums/components/shell.dm`, `shell/compact_remote.dm`, `shell/controller.dm`,
//! `shell/bot.dm` and `shell/shell_items.dm`.
//!
//! - Every `/obj/item/integrated_circuit` gets an [`IntegratedCircuit`] (its [`Circuit`], cell
//!   and shell), every ported `/obj/item/circuit_component` type a [`CircuitPart`], the compact
//!   remote, the controller and the bot a [`Shell`].
//! - A component used on a circuit goes in (`add_component`); a power cell goes in, a
//!   screwdriver takes it out. Used in hand, the circuit opens its window (tgui
//!   `IntegratedCircuit`, [`super::client`]): wiring, typed-in values, `+`/`-` ports, ejecting
//!   components (to the user's hands), the label.
//! - A circuit used on a shell goes in if it fits (`capacity`: 25, 50, 100); the shell's own
//!   component joins it and it turns on. With a circuit inside, components go in through the
//!   shell, a multitool opens the window and a screwdriver takes the circuit out.
//! - Shell triggers: the compact remote and the controller used in hand, the bot touched with
//!   an empty hand send `Signal` with the user.
//! - A screwdriver finishes a bot assembly (`/obj/item/shell/bot`, 3 s) into the bot.
//! - USB cables connect a circuit in a shell with a USB port to machines ([`super::devices`]).
//! - Effects: speech is the shell's `say` to the players in range (1 tile quiet); a radio
//!   posts on the signaler band ([`crate::assemblies::server::post_signal`]), assembly
//!   signalers included, and circuits receive their signals; voice activators hear speech
//!   within 7 tiles of the circuit.
//!
//! Cost: every 0.1 s one pass over the circuits, stepping those with deliveries, a running
//! clock or a pending delay (a handful); a said line is one pass over the circuits.
// shortcut: no owner IDs or shell locking, no circuit marker, no duplicator, no
// remote printing from the window, no `loaded` circuit's cell, no controller alt/right-click
// signals, no inducers, no examine of the components; circuit speech is not heard by other
// circuits or voice analyzers (it does not go through `Spoke`).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_chat::server::Spoke;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmName, DmObj};
use ss13_net::chat::server::SendChat;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::server::{
    AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_tgui::{UiInterface as _, UiStatus};

use super::circuit::{Circuit, Ctx, Effect, Kind, STEP};
use super::*;
use crate::assemblies::server::{
    drop_out, in_hands_of, insert, name_of, now, to_hands, within_reach,
};
use crate::combat::CombatRng;
use crate::construction::Job;
use crate::mecha::core::ItemCell;
use crate::tools::{Actor, DoAfter, ToolKind, tell, tool_of};

pub const CIRCUIT_PATH: &str = "/obj/item/integrated_circuit";
const ADMIN_CIRCUIT: &str = "/obj/item/integrated_circuit/admin";
const COMPACT_REMOTE: &str = "/obj/item/compact_remote";
const CONTROLLER: &str = "/obj/item/controller";
const BOT: &str = "/obj/structure/bot";
const BOT_ASSEMBLY: &str = "/obj/item/shell/bot";
/// `/obj/item/shell` `screw_delay`, deciseconds.
const SCREW_DELAY_DS: f32 = 30.0;
/// `SFX_TERMINAL_TYPE` at 25 (the controller's `SFX_KEYBOARD_CLICKS`).
const TERMINAL_SOUND: &str = "terminal_type";
const KEYBOARD_SOUND: &str = "keyboard_clicks";
const SHELL_SOUND_VOLUME: f32 = 25.0;
/// `MESSAGE_RANGE`, `WHISPER_RANGE`; `tostring`'s `max_range`.
const SAY_RANGE: u32 = 7;
const WHISPER_RANGE: u32 = 1;
const TOSTRING_RANGE: u32 = 7;

/// tg `/obj/item/integrated_circuit`.
#[derive(Component, Clone, Debug, Default)]
pub struct IntegratedCircuit {
    pub circuit: Circuit,
    pub cell: Option<Entity>,
    pub shell: Option<Entity>,
    /// The machines connected by USB cable ([`super::devices`]).
    pub usb: Vec<super::devices::UsbLink>,
}

/// A circuit component item that is not in a circuit (in one, it is a [`super::circuit::Node`]).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CircuitPart(pub Kind);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellKind {
    CompactRemote,
    Controller,
    Bot,
    /// A carried modular computer (`crate::modpc::circuit`).
    Computer,
    /// A stationary modular console.
    Console,
    /// An airlock built with shell electronics (`crate::airlock_construction`).
    Airlock,
}

impl ShellKind {
    fn from_path(path: &str) -> Option<Self> {
        if is_under(path, COMPACT_REMOTE) {
            Some(Self::CompactRemote)
        } else if is_under(path, CONTROLLER) {
            Some(Self::Controller)
        } else if is_under(path, BOT) {
            Some(Self::Bot)
        } else if is_under(path, crate::modpc::circuit::COMPUTER_ITEM) {
            Some(Self::Computer)
        } else if is_under(path, crate::modpc::programs::CONSOLE_ROOT) {
            Some(Self::Console)
        } else {
            None
        }
    }

    /// `unremovable_circuit_components`.
    pub fn parts(self) -> &'static [Kind] {
        match self {
            Self::CompactRemote => &[Kind::CompactRemote],
            Self::Controller => &[Kind::Controller],
            Self::Bot => &[Kind::Bot],
            Self::Computer | Self::Console => &[Kind::Modpc],
            Self::Airlock => &[Kind::Airlock],
        }
    }

    /// `SHELL_CAPACITY_SMALL`, `_MEDIUM`, `_LARGE`.
    pub fn capacity(self) -> u32 {
        match self {
            Self::CompactRemote => 25,
            Self::Controller | Self::Computer => 50,
            Self::Bot | Self::Console | Self::Airlock => 100,
        }
    }

    /// `SHELL_FLAG_USB_PORT`.
    pub fn usb(self) -> bool {
        matches!(self, Self::Bot | Self::Console)
    }
}

/// tg `/datum/component/shell`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shell {
    pub kind: ShellKind,
    pub circuit: Option<Entity>,
}

/// tg `/obj/item/shell`: a screwdriver turns it into `shell_to_spawn`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShellAssembly {
    pub spawns: &'static str,
}

pub struct WiremodServerPlugin;

impl Plugin for WiremodServerPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // Circuits open their window, the remote and the controller send their signal.
        app.add_click_handler(ClickPhase::AttackSelf, 1960, |w, c| {
            attack_self(w, c.client, c.mob, c.target)
        });
        // Circuits, components, cells and tools on circuits and shells in the other hand.
        app.add_click_handler(ClickPhase::OtherHand, 760, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
        // The same on the floor; screwdrivers on shell assemblies.
        app.add_click_handler(ClickPhase::HeldOnTarget, 555, |w, c| {
            c.held
                .is_some_and(|h| super::devices::cable_interaction(w, c.client, c.mob, h, c.target))
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 560, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
        // The bot's button.
        app.add_click_handler(ClickPhase::EmptyHand, 3460, |w, c| {
            attack_hand(w, c.client, c.mob, c.target)
        });
        app.register_ui::<IntegratedCircuitUi>()
            .add_observer(make_items)
            .add_observer(make_structures)
            .add_systems(
                Update,
                (
                    tick_circuits.run_if(any_with_component::<IntegratedCircuit>),
                    super::devices::watch.run_if(any_with_component::<super::devices::UsbCable>),
                    hear_speech.after(ss13_chat::server::SaySystems),
                    window_status.in_set(UiSystems::StatusHook),
                    circuit_acts.in_set(UiSystems::Handle),
                ),
            );
    }
}

fn make_items(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    let path = item.path.as_str();
    let mut entity = commands.entity(add.entity);
    if is_under(path, CIRCUIT_PATH) {
        if !is_under(path, ADMIN_CIRCUIT) {
            // The window follows the user's hands or reach ([`window_status`]).
            entity.try_insert((IntegratedCircuit::default(), AlwaysState));
        }
    } else if let Some(kind) = Kind::from_path(path) {
        // `Initialize`: `"[lowertext(display_name)] component"`.
        let name = format!("{} component", kind.def().name.to_lowercase());
        entity.try_insert((CircuitPart(kind), Name::new(name)));
    } else if let Some(kind) = ShellKind::from_path(path) {
        entity.try_insert(Shell {
            kind,
            circuit: None,
        });
    } else if is_under(path, BOT_ASSEMBLY) {
        entity.try_insert(ShellAssembly { spawns: BOT });
    } else if is_under(path, super::devices::CABLE) {
        entity.try_insert(super::devices::UsbCable::default());
    }
}

fn make_structures(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(obj) = objs.get(add.entity) else {
        return;
    };
    let kind = if is_under(&obj.path, BOT) {
        ShellKind::Bot
    } else if is_under(&obj.path, crate::modpc::programs::CONSOLE_ROOT) {
        ShellKind::Console
    } else {
        return;
    };
    commands.entity(add.entity).try_insert(Shell {
        kind,
        circuit: None,
    });
}

// --- Running ----------------------------------------------------------------------------

/// Runs `f` on circuit `e` with its cell's charge, the round's RNG and the names in range;
/// then writes the charge back and applies the effects.
pub fn with_circuit<R>(
    world: &mut World,
    e: Entity,
    f: impl FnOnce(&mut Circuit, &mut Ctx) -> R,
) -> Option<R> {
    let mut ic = world.get_entity_mut(e).ok()?.take::<IntegratedCircuit>()?;
    let now = now(world);
    // A computer shell powers its circuit from its own cell (`use_energy_for_circuits`).
    let computer_cell = ic
        .shell
        .filter(|_| ic.cell.is_none())
        .filter(|&s| crate::modpc::circuit::powers_circuit(world, s));
    let charge = match computer_cell {
        Some(shell) => crate::modpc::circuit::circuit_charge(world, shell),
        None => ic
            .cell
            .and_then(|c| world.get::<ItemCell>(c))
            .map(|c| c.0.charge),
    };
    let before = charge;
    let pos = crate::explosions::turf_of(world, e);
    let mut rng = CombatRng(world.get_resource::<CombatRng>().map_or(0x5eed, |r| r.0));
    let (result, charge, effects) = {
        let w: &World = world;
        let names = move |t: Entity| -> Option<String> {
            let (a, b) = (pos?, crate::explosions::turf_of(w, t)?);
            a.chebyshev(b)
                .filter(|&d| d <= TOSTRING_RANGE)
                .map(|_| name_of(w, t))
        };
        let mut ctx = Ctx {
            now,
            charge,
            has_shell: ic.shell.is_some(),
            rng: &mut rng,
            name_of: &names,
            effects: Vec::new(),
        };
        let result = f(&mut ic.circuit, &mut ctx);
        (result, ctx.charge, ctx.effects)
    };
    if let (Some(cell), Some(charge)) = (ic.cell, charge)
        && let Some(mut c) = world.get_mut::<ItemCell>(cell)
        && c.0.charge != charge
    {
        c.0.charge = charge;
    }
    if let (Some(shell), Some(before), Some(after)) = (computer_cell, before, charge)
        && before > after
    {
        crate::modpc::power::use_energy(world, shell, before - after, false);
    }
    if let Some(mut r) = world.get_resource_mut::<CombatRng>() {
        r.0 = rng.0;
    }
    let shell = ic.shell;
    world.entity_mut(e).insert(ic);
    for effect in effects {
        apply(world, e, shell, effect);
    }
    Some(result)
}

fn apply(world: &mut World, circuit: Entity, shell: Option<Entity>, effect: Effect) {
    match effect {
        Effect::Say { text, quiet } => {
            let Some(shell) = shell.filter(|&s| world.get_entity(s).is_ok()) else {
                return;
            };
            let Some(pos) = crate::explosions::turf_of(world, shell) else {
                return;
            };
            let range = if quiet { WHISPER_RANGE } else { SAY_RANGE };
            let clients = crate::obj_damage::clients_in_range(world, pos, range, &[]);
            if clients.is_empty() {
                return;
            }
            let name = name_of(world, shell);
            world.write_message(SendChat {
                clients,
                message: ss13_chat::server::say_message(&name, &text),
            });
        }
        Effect::BundleActivate { port } => {
            if let Some(shell) = shell {
                bundle_activate(world, shell, port);
            }
        }
        Effect::Radio {
            node,
            frequency,
            code,
        } => {
            let from = crate::explosions::turf_of(world, circuit);
            info!(
                "circuit {circuit} at {from:?} sent {}/{code}",
                crate::assemblies::format_frequency(frequency)
            );
            crate::assemblies::server::post_signal(
                world,
                circuit,
                Some(node),
                frequency,
                code,
                from,
                0,
            );
        }
        Effect::Computer(effect) => {
            if let Some(shell) = shell {
                crate::modpc::circuit::apply(world, shell, effect);
            }
        }
        Effect::Device { node, port } => {
            super::devices::device_input(world, circuit, shell, node, port);
        }
    }
}

/// Deliveries per step across all circuits (tg's `MC_TICK_CHECK` budget) and the smallest
/// share of it a busy circuit gets.
const STEP_DELIVERIES: usize = 20_000;
const MIN_SHARE: usize = 50;

/// `SScircuit_component` and `SSclock_component`: a step every 0.1 s.
fn tick_circuits(world: &mut World, mut elapsed: Local<f64>) {
    *elapsed += world.resource::<Time>().delta_secs_f64();
    if *elapsed < STEP {
        return;
    }
    *elapsed = (*elapsed - STEP).min(STEP);
    super::devices::airlock_states(world);
    let busy: Vec<Entity> = world
        .query::<(Entity, &IntegratedCircuit)>()
        .iter(world)
        .filter(|(_, c)| c.circuit.busy())
        .map(|(e, _)| e)
        .collect();
    if busy.is_empty() {
        return;
    }
    // Every circuit a fair share of the step: one with a loop cannot starve the others.
    let limit = (STEP_DELIVERIES / busy.len()).max(MIN_SHARE);
    for e in busy {
        with_circuit(world, e, |c, ctx| c.tick_with(ctx, limit));
    }
}

/// The circuit radios' `receive_signal` (from [`crate::assemblies::server::post_signal`]):
/// every circuit listening on `frequency`, but the sending component.
pub fn receive_signal(
    world: &mut World,
    source: Entity,
    node: Option<u32>,
    frequency: u16,
    code: i32,
    from: Option<TilePos>,
    range: u32,
) {
    let listeners: Vec<Entity> = world
        .query::<(Entity, &IntegratedCircuit)>()
        .iter(world)
        .filter(|(_, c)| {
            c.circuit
                .nodes
                .iter()
                .any(|n| n.kind == Kind::Radio && n.state.radio_freq == Some(frequency))
        })
        .map(|(e, _)| e)
        .collect();
    let now = now(world);
    for c in listeners {
        if range > 0 {
            let (Some(from), Some(to)) = (from, crate::explosions::turf_of(world, c)) else {
                continue;
            };
            if from.chebyshev(to).is_none_or(|d| d > range) {
                continue;
            }
        }
        let skip = if c == source { node } else { None };
        with_circuit(world, c, |circuit, _| {
            circuit.radio_receive(skip, frequency, code, now)
        });
    }
}

/// Voice activators: what is said within 7 tiles of the circuit (1 for whispers).
fn hear_speech(world: &mut World, mut cursor: Local<MessageCursor<Spoke>>) {
    if !world.contains_resource::<Messages<Spoke>>() {
        return;
    }
    let spoken: Vec<(Entity, TilePos, String, String, String, bool)> = cursor
        .read(world.resource::<Messages<Spoke>>())
        .map(|s| {
            let language = s.language.map_or("", |l| l.def().name).to_owned();
            (
                s.speaker,
                s.pos,
                s.name.clone(),
                s.text.clone(),
                language,
                s.whisper,
            )
        })
        .collect();
    if spoken.is_empty() {
        return;
    }
    let listeners: Vec<(Entity, Option<Entity>)> = world
        .query::<(Entity, &IntegratedCircuit)>()
        .iter(world)
        .filter(|(_, c)| c.circuit.nodes.iter().any(|n| n.kind == Kind::Hear))
        .map(|(e, c)| (e, c.shell))
        .collect();
    for (speaker, pos, voice, text, language, whisper) in spoken {
        let range = if whisper { WHISPER_RANGE } else { SAY_RANGE };
        for &(c, shell) in &listeners {
            let Some(at) = crate::explosions::turf_of(world, c) else {
                continue;
            };
            if at.chebyshev(pos).is_none_or(|d| d > range) {
                continue;
            }
            with_circuit(world, c, |circuit, ctx| {
                circuit.hear(speaker, shell, &text, &language, &voice, ctx.now)
            });
        }
    }
}

// --- Clicks -------------------------------------------------------------------------------

pub(super) fn open_window(world: &mut World, client: Entity, mob: Entity, circuit: Entity) {
    let title = name_of(world, circuit);
    world.write_message(OpenUi::new::<IntegratedCircuitUi>(
        client, mob, circuit, title,
    ));
}

/// The shell's component sends its `Signal` with the user (`send_trigger`, `on_attack_hand`).
fn shell_signal(
    world: &mut World,
    client: Entity,
    mob: Entity,
    shell: Entity,
    circuit: Entity,
    kind: Kind,
) {
    let (text, sound) = match kind {
        Kind::Bot => ("pushed button", TERMINAL_SOUND),
        Kind::Controller => ("clicked primary button", KEYBOARD_SOUND),
        _ => ("clicked primary button", TERMINAL_SOUND),
    };
    tell(world, client, Span::Notice, text);
    if let Some(pos) = crate::explosions::turf_of(world, shell) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, sound, SHELL_SOUND_VOLUME, false),
        );
    }
    let now = now(world);
    with_circuit(world, circuit, |c, _| c.shell_event(kind, 1, mob, now));
}

/// Used in hand: a circuit's window, the remote's and the controller's primary signal.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if let Some(shell) = world.get::<Shell>(item).copied() {
        let Some(circuit) = shell.circuit else {
            return false;
        };
        let kind = match shell.kind {
            ShellKind::CompactRemote => Kind::CompactRemote,
            ShellKind::Controller => Kind::Controller,
            // A computer used in hand turns on (`crate::modpc`).
            ShellKind::Bot | ShellKind::Computer | ShellKind::Console | ShellKind::Airlock => {
                return false;
            }
        };
        shell_signal(world, client, mob, item, circuit, kind);
        return true;
    }
    if world.get::<IntegratedCircuit>(item).is_some() {
        open_window(world, client, mob, item);
        return true;
    }
    false
}

/// The bot's `on_attack_hand`.
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(Shell {
        kind: ShellKind::Bot,
        circuit: Some(circuit),
    }) = world.get::<Shell>(target).copied()
    else {
        return false;
    };
    shell_signal(world, client, mob, target, circuit, Kind::Bot);
    true
}

/// Items used on circuits, shells and shell assemblies (tg `item_interaction`,
/// `screwdriver_act`, `on_item_interaction`, `on_multitool_act`, `on_screwdriver_act`).
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let tool = tool_of(world, held).map(|(t, _)| t);
    let part = world.get::<CircuitPart>(held).copied();
    let cell = world.get::<ItemCell>(held).is_some();
    if let Some(ic) = world.get::<IntegratedCircuit>(target) {
        let has_cell = ic.cell.is_some();
        if let Some(CircuitPart(kind)) = part {
            add_component(world, client, target, held, kind);
            return true;
        }
        if cell {
            if has_cell {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "there already is a cell inside!",
                );
            } else {
                insert(world, held, target);
                if let Some(mut ic) = world.get_mut::<IntegratedCircuit>(target) {
                    ic.cell = Some(held);
                }
                let text = format!("You insert the power cell into {}.", name_of(world, target));
                tell(world, client, Span::Notice, text);
            }
            return true;
        }
        if tool == Some(ToolKind::Screwdriver) {
            let Some(c) = world
                .get::<IntegratedCircuit>(target)
                .and_then(|ic| ic.cell)
            else {
                tell(world, client, Span::Warning, "power cell missing!");
                return true;
            };
            if let Some(mut ic) = world.get_mut::<IntegratedCircuit>(target) {
                ic.cell = None;
            }
            if world.get_entity(c).is_ok() {
                drop_out(world, c, target);
            }
            let text = format!(
                "You unscrew the power cell from {}.",
                name_of(world, target)
            );
            tell(world, client, Span::Notice, text);
            return true;
        }
        return false;
    }
    if let Some(shell) = world.get::<Shell>(target).copied() {
        if cell {
            tell(world, client, Span::Warning, "can't put cell in directly!");
            return true;
        }
        if let Some(circuit) = shell.circuit {
            if let Some(CircuitPart(kind)) = part {
                add_component(world, client, circuit, held, kind);
                return true;
            }
            match tool {
                Some(ToolKind::Multitool) => {
                    open_window(world, client, mob, circuit);
                    return true;
                }
                Some(ToolKind::Screwdriver) => {
                    let text = format!(
                        "you unscrew {} from {}.",
                        name_of(world, circuit),
                        name_of(world, target)
                    );
                    remove_circuit(world, target);
                    tell(world, client, Span::Notice, text);
                    return true;
                }
                _ => {}
            }
            if world.get::<IntegratedCircuit>(held).is_some() {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "there is already a circuitboard inside!",
                );
                return true;
            }
            return false;
        }
        if world.get::<IntegratedCircuit>(held).is_some() {
            attach_circuit(world, client, target, held);
            return true;
        }
        return false;
    }
    if world.get::<ShellAssembly>(target).is_some() && tool == Some(ToolKind::Screwdriver) {
        let text = format!("You begin finishing {}.", name_of(world, target));
        tell(world, client, Span::Notice, text);
        // A held assembly has no tile of its own: the action is on the user.
        let on = if world.get::<TilePos>(target).is_some() {
            target
        } else {
            mob
        };
        let actor = Actor {
            client,
            mob,
            item: Some(held),
        };
        crate::tools::do_after(world, actor, on, SCREW_DELAY_DS, 0, Job::Wiremod(target));
        return true;
    }
    false
}

/// `add_component` (through the shell: `on_circuit_add_component_manually`'s capacity).
fn add_component(world: &mut World, client: Entity, circuit: Entity, item: Entity, kind: Kind) {
    let Some(ic) = world.get::<IntegratedCircuit>(circuit) else {
        return;
    };
    let capacity = ic
        .shell
        .and_then(|s| world.get::<Shell>(s))
        .map(|s| s.kind.capacity());
    if capacity.is_some_and(|cap| ic.circuit.size + 1 > cap) {
        tell(world, client, Span::Warning, "it won't fit!");
        return;
    }
    // `TRAIT_COMPONENT_WIRE_BUNDLE`: one bundle per circuit.
    if kind == Kind::WireBundle && ic.circuit.has_bundle() {
        return;
    }
    let shell = ic.shell;
    insert(world, item, circuit);
    world.entity_mut(item).remove::<CircuitPart>();
    with_circuit(world, circuit, |c, ctx| {
        c.add(kind, Some(item), true, ctx.rng)
    });
    if kind == Kind::WireBundle
        && let Some(shell) = shell
    {
        register_bundle(world, shell, circuit);
    }
}

/// `CAPACITY_PER_WIRE`, `MAX_WIRE_COUNT`.
const CAPACITY_PER_WIRE: f64 = 10.0;
const MAX_WIRE_COUNT: f64 = 17.0;

/// `wire_bundle/register_shell`: a shell without wires gets `Port 1`..`Port n` (one per 10
/// capacity, 1-17; `/datum/wires/wire_bundle_component`, random per instance) and the bundle
/// its ports.
fn register_bundle(world: &mut World, shell: Entity, circuit: Entity) {
    let has_bundle = world
        .get::<IntegratedCircuit>(circuit)
        .is_some_and(|ic| ic.circuit.has_bundle());
    let Some(kind) = world.get::<Shell>(shell).map(|s| s.kind) else {
        return;
    };
    if !has_bundle || world.get::<crate::wires::Wires>(shell).is_some() {
        return;
    }
    let count = (f64::from(kind.capacity()) / CAPACITY_PER_WIRE)
        .round()
        .clamp(1.0, MAX_WIRE_COUNT) as usize;
    let names: Vec<String> = (1..=count).map(|i| format!("Port {i}")).collect();
    let set = crate::wires::WireSet {
        key: "/datum/wires/wire_bundle_component",
        proper_name: "Unknown",
        wires: names.clone(),
    };
    let seed = world
        .get_resource::<crate::wires::WireColors>()
        .map_or(0, |c| c.seed);
    let wires = crate::wires::Wires {
        holder: crate::wires::WireHolder::Machine(crate::machine_wires::MachineWires::WireBundle),
        proper_name: set.proper_name,
        colors: crate::wires::colors_for(seed ^ shell.to_bits(), &set),
        cut: Vec::new(),
    };
    world.entity_mut(shell).insert(wires);
    with_circuit(world, circuit, |c, _| c.set_bundle_ports(&names));
}

/// `wire_bundle/unregister_shell`: the assemblies drop out, the wires and the ports go.
fn unregister_bundle(world: &mut World, shell: Entity, circuit: Entity) {
    let is_bundle = world.get::<crate::wires::Wires>(shell).is_some_and(|w| {
        w.holder
            == crate::wires::WireHolder::Machine(crate::machine_wires::MachineWires::WireBundle)
    });
    if !is_bundle {
        return;
    }
    let colors: Vec<&'static str> = world
        .get::<crate::wires::Wires>(shell)
        .map(|w| w.colors.iter().map(|(c, _)| *c).collect())
        .unwrap_or_default();
    for color in colors {
        crate::wires::detach_assembly(world, shell, color);
    }
    world
        .entity_mut(shell)
        .remove::<(crate::wires::Wires, crate::wires::WireAssemblies)>();
    with_circuit(world, circuit, |c, _| c.clear_bundle_ports());
}

/// The bundle's `input_received`: the assembly on the shell's wire `port` activates, unless
/// the wire is cut.
fn bundle_activate(world: &mut World, shell: Entity, port: usize) {
    let wire = format!("Port {}", port + 1);
    let Some(color) = world
        .get::<crate::wires::Wires>(shell)
        .filter(|w| !w.is_cut(&wire))
        .and_then(|w| w.color_of(&wire))
    else {
        return;
    };
    if let Some(a) = world
        .get::<crate::wires::WireAssemblies>(shell)
        .and_then(|w| w.at(color))
    {
        crate::assemblies::server::activate(world, a);
    }
}

/// `wire_bundle/on_pulse_wire` (from the shell's wires): the wire's `Pulsed` output, unless cut.
pub fn bundle_pulsed(world: &mut World, shell: Entity, wire: &str) {
    let cut = world
        .get::<crate::wires::Wires>(shell)
        .is_none_or(|w| w.is_cut(wire));
    let Some(circuit) = world.get::<Shell>(shell).and_then(|s| s.circuit) else {
        return;
    };
    let Some(index) = wire
        .strip_prefix("Port ")
        .and_then(|n| n.parse::<usize>().ok())
        .and_then(|n| n.checked_sub(1))
    else {
        return;
    };
    if cut {
        return;
    }
    let now = now(world);
    with_circuit(world, circuit, |c, _| c.bundle_pulsed(index, now));
}

/// `attach_circuit`: the shell's components join, the circuit turns on.
fn attach_circuit(world: &mut World, client: Entity, shell: Entity, circuit: Entity) {
    let (Some(s), Some(ic)) = (
        world.get::<Shell>(shell).copied(),
        world.get::<IntegratedCircuit>(circuit),
    ) else {
        return;
    };
    if ic.shell.is_some() {
        return;
    }
    if ic.circuit.size > s.kind.capacity() {
        let text = format!("this is too large to fit into {}!", name_of(world, shell));
        tell(world, client, Span::Warning, text);
        return;
    }
    insert(world, circuit, shell);
    if let Some(mut ic) = world.get_mut::<IntegratedCircuit>(circuit) {
        ic.shell = Some(shell);
    }
    if let Some(mut s) = world.get_mut::<Shell>(shell) {
        s.circuit = Some(circuit);
    }
    with_circuit(world, circuit, |c, ctx| c.attach_shell(s.kind.parts(), ctx));
    register_bundle(world, shell, circuit);
    rename_shell(world, circuit);
}

/// `remove_circuit`: onto the shell's tile, off.
pub fn remove_circuit(world: &mut World, shell: Entity) {
    let Some(circuit) = world
        .get_mut::<Shell>(shell)
        .and_then(|mut s| s.circuit.take())
    else {
        return;
    };
    unregister_bundle(world, shell, circuit);
    super::devices::detach_all(world, circuit);
    with_circuit(world, circuit, |c, _| c.detach_shell());
    if let Some(mut ic) = world.get_mut::<IntegratedCircuit>(circuit) {
        ic.shell = None;
    }
    if world.get_entity(circuit).is_ok() {
        drop_out(world, circuit, shell);
    }
    let name = initial_name(world, shell);
    world.entity_mut(shell).insert(Name::new(name));
}

fn initial_name(world: &World, e: Entity) -> String {
    world
        .get::<DmName>(e)
        .map(|n| crate::science::lathe::plain(&n.name).to_owned())
        .unwrap_or_default()
}

/// `set_display_name` on the shell: `"<shell> (<label>)"`.
fn rename_shell(world: &mut World, circuit: Entity) {
    let Some((Some(shell), label)) = world
        .get::<IntegratedCircuit>(circuit)
        .map(|ic| (ic.shell, ic.circuit.display_name.clone()))
    else {
        return;
    };
    let initial = initial_name(world, shell);
    let name = if label.is_empty() {
        initial
    } else {
        format!("{initial} ({label})")
    };
    world.entity_mut(shell).insert(Name::new(name));
}

/// The end of a shell assembly's `screwdriver_act` (`Job::Wiremod`).
pub fn complete(world: &mut World, _mob: Entity, action: &DoAfter, assembly: Entity) {
    let Some(ShellAssembly { spawns }) = world.get::<ShellAssembly>(assembly).copied() else {
        return;
    };
    let Some(pos) = crate::explosions::turf_of(world, assembly) else {
        return;
    };
    let text = format!("You finish {}.", name_of(world, assembly));
    tell(world, action.client, Span::Notice, text);
    world.despawn(assembly);
    crate::construction::spawn_obj(world, spawns, pos);
}

// --- Window -------------------------------------------------------------------------------

/// `ui_state`: `hands_state` without a shell, `physical_obscured_state` (within reach of the
/// shell) in one.
fn window_status(world: &mut World) {
    let mut windows = world.query::<(Entity, &UiWindow)>();
    let open: Vec<(Entity, Entity, Entity)> = windows
        .iter(world)
        .filter(|(_, w)| w.interface == IntegratedCircuitUi::NAME)
        .map(|(e, w)| (e, w.source, w.actor))
        .collect();
    for (window, source, actor) in open {
        let ok = match world.get::<IntegratedCircuit>(source).map(|c| c.shell) {
            Some(None) => in_hands_of(world, source, actor),
            Some(Some(shell)) => {
                world.get_entity(shell).is_ok() && within_reach(world, actor, shell)
            }
            None => false,
        };
        if !ok && let Some(mut w) = world.get_mut::<UiWindow>(window) {
            w.cap_status(UiStatus::Close);
        }
    }
}

impl ServerUi for IntegratedCircuitUi {
    type Source = &'static IntegratedCircuit;
    type UserParam = Query<'static, 'static, &'static ItemCell>;
    type Act = CircuitAct;

    fn static_data(_: &IntegratedCircuit) {}

    fn data(
        ic: &IntegratedCircuit,
        _: Entity,
        cells: &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> CircuitData {
        CircuitData {
            components: ic.circuit.ui_data(),
            display_name: ic.circuit.display_name.clone(),
            charge: ic
                .cell
                .and_then(|c| cells.get(c).ok())
                .map(|c| c.0.percent().round().max(0.0) as u32),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<CircuitAct, UiReject> {
        Ok(match action {
            acts::ADD_CONNECTION => {
                let output = match params.get(acts::OUTPUT) {
                    Some(label) => parse_output_label(label)
                        .filter(|&(c, p)| c >= 1 && p >= 1)
                        .map(|(c, p)| (c - 1, p - 1))
                        .ok_or_else(|| UiReject::BadParams("output: expected c.p".into()))?,
                    None => (
                        index(params, acts::OUTPUT_COMPONENT_ID)?,
                        index(params, acts::OUTPUT_PORT_ID)?,
                    ),
                };
                CircuitAct::AddConnection {
                    input: (
                        index(params, acts::INPUT_COMPONENT_ID)?,
                        index(params, acts::INPUT_PORT_ID)?,
                    ),
                    output,
                }
            }
            acts::REMOVE_CONNECTION => CircuitAct::RemoveConnection {
                component: index(params, acts::COMPONENT_ID)?,
                is_input: flag(params, acts::IS_INPUT),
                port: index(params, acts::PORT_ID)?,
            },
            acts::DETACH_COMPONENT => CircuitAct::Detach(index(params, acts::COMPONENT_ID)?),
            acts::SET_COORDINATES => CircuitAct::Coordinates {
                component: index(params, acts::COMPONENT_ID)?,
                x: params.number(acts::REL_X)?,
                y: params.number(acts::REL_Y)?,
            },
            acts::SET_INPUT => CircuitAct::SetInput {
                component: index(params, acts::COMPONENT_ID)?,
                port: index(params, acts::PORT_ID)?,
                input: (!flag(params, acts::SET_NULL))
                    .then(|| params.text(acts::INPUT).unwrap_or_default()),
            },
            acts::GET_VALUE => CircuitAct::GetValue {
                component: index(params, acts::COMPONENT_ID)?,
                port: index(params, acts::PORT_ID)?,
            },
            acts::SET_DISPLAY_NAME => {
                CircuitAct::DisplayName(params.text(acts::DISPLAY_NAME).unwrap_or_default())
            }
            acts::PERFORM_ACTION => CircuitAct::Perform {
                component: index(params, acts::COMPONENT_ID)?,
                action: params.text(acts::ACTION_NAME)?,
            },
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

/// A 1-based id param as a 0-based position.
fn index(params: &UiParams, key: &str) -> Result<usize, UiReject> {
    let n = params.number(key)?;
    if !(1.0..=100_000.0).contains(&n) {
        return Err(UiReject::BadParams(format!("{key}: out of range")));
    }
    Ok(n as usize - 1)
}

/// A truthy param (tgui sends booleans as text).
fn flag(params: &UiParams, key: &str) -> bool {
    params
        .get(key)
        .is_some_and(|v| !matches!(v, "" | "0" | "false" | "null"))
}

/// A parsed act; positions are 0-based.
#[derive(Clone, Debug, PartialEq)]
pub enum CircuitAct {
    AddConnection {
        input: (usize, usize),
        output: (usize, usize),
    },
    RemoveConnection {
        component: usize,
        is_input: bool,
        port: usize,
    },
    Detach(usize),
    Coordinates {
        component: usize,
        x: f64,
        y: f64,
    },
    /// `None`: `set_null`.
    SetInput {
        component: usize,
        port: usize,
        input: Option<String>,
    },
    GetValue {
        component: usize,
        port: usize,
    },
    DisplayName(String),
    Perform {
        component: usize,
        action: String,
    },
}

/// `/obj/item/integrated_circuit/ui_act`.
fn circuit_acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<IntegratedCircuitUi>>>) {
    let acts: Vec<(Entity, Entity, CircuitAct)> = cursor
        .read(world.resource::<Messages<UiActed<IntegratedCircuitUi>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, circuit, act) in acts {
        if world.get::<IntegratedCircuit>(circuit).is_none() {
            continue;
        }
        match act {
            CircuitAct::AddConnection { input, output } => {
                with_circuit(world, circuit, |c, ctx| c.connect(input, output, ctx));
            }
            CircuitAct::RemoveConnection {
                component,
                is_input,
                port,
            } => {
                with_circuit(world, circuit, |c, _| {
                    c.disconnect(component, is_input, port)
                });
            }
            CircuitAct::Detach(component) => detach(world, actor, circuit, component),
            CircuitAct::Coordinates { component, x, y } => {
                with_circuit(world, circuit, |c, _| c.set_coordinates(component, x, y));
            }
            CircuitAct::SetInput {
                component,
                port,
                input,
            } => {
                let balloon = with_circuit(world, circuit, |c, ctx| {
                    c.manual_input(component, port, input.as_deref(), ctx)
                })
                .flatten();
                if let Some(text) = balloon {
                    crate::balloon::balloon_alert(world, actor, circuit, &text);
                }
            }
            CircuitAct::GetValue { component, port } => {
                let text = world
                    .get::<IntegratedCircuit>(circuit)
                    .and_then(|ic| ic.circuit.output_text(component, port));
                if let Some(text) = text {
                    crate::balloon::balloon_alert(world, actor, circuit, &text);
                }
            }
            CircuitAct::DisplayName(name) => {
                if let Some(mut ic) = world.get_mut::<IntegratedCircuit>(circuit) {
                    ic.circuit.set_display_name(&name);
                }
                rename_shell(world, circuit);
            }
            CircuitAct::Perform { component, action } => {
                with_circuit(world, circuit, |c, _| c.perform_action(component, &action));
            }
        }
    }
}

/// `detach_component`: out of the circuit into the user's hands.
fn detach(world: &mut World, actor: Entity, circuit: Entity, component: usize) {
    let Some((id, kind, item, usb)) = world
        .get::<IntegratedCircuit>(circuit)
        .and_then(|ic| ic.circuit.nodes.get(component))
        .filter(|n| n.removable || n.usb.is_some())
        .map(|n| (n.id, n.kind, n.item, n.usb))
    else {
        return;
    };
    // `on_circuit_component_removed`: ejecting a USB component pulls its cable.
    if let Some(machine) = usb {
        super::devices::detach(world, circuit, machine);
        return;
    }
    if kind == Kind::WireBundle
        && let Some(shell) = world
            .get::<IntegratedCircuit>(circuit)
            .and_then(|ic| ic.shell)
    {
        unregister_bundle(world, shell, circuit);
    }
    with_circuit(world, circuit, |c, _| c.remove(id));
    if let Some(item) = item
        && world.get_entity(item).is_ok()
    {
        world.entity_mut(item).insert(CircuitPart(kind));
        to_hands(world, actor, item);
    }
}

/// Examine lines of circuits and shells (`examine`, the shell's `on_examine`).
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let meter = |circuit: Entity| -> String {
        let charge = world
            .get::<IntegratedCircuit>(circuit)
            .and_then(|ic| ic.cell)
            .and_then(|c| world.get::<ItemCell>(c))
            .map_or(0.0, |c| c.0.percent().round());
        format!("The charge meter reads {charge}%.")
    };
    if let Some(ic) = world.get::<IntegratedCircuit>(target) {
        let text = if ic.cell.is_some() {
            meter(target)
        } else {
            "There is no power cell installed.".to_owned()
        };
        lines.push((Span::Notice, text));
    }
    if let Some(shell) = world.get::<Shell>(target) {
        let Some(circuit) = shell.circuit else {
            lines.push((
                Span::Notice,
                "Нет прикреплённой интегральной платы.".to_owned(),
            ));
            return;
        };
        lines.push((
            Span::Notice,
            format!(
                "Прикреплена интегральная плата. Используйте мультитул для доступа к проводке. \
                 Используйте отвёртку, чтобы извлечь её из {}.",
                name_of(world, target)
            ),
        ));
        lines.push((Span::Notice, meter(circuit)));
    }
    super::devices::examine_lines(world, target, lines);
}
