//! The circuit simulation, free of the world: tg `integrated_circuit.dm`, `component.dm`,
//! `port.dm`, `datatypes/` and the ported components.
//!
//! - A [`Circuit`] holds its components ([`Node`]s) with typed input and output ports. An input
//!   wired to an output gets every value the output is set to; the delivery is queued for the
//!   next 0.1 s step ([`Circuit::tick`], tg `SScircuit_component`), so a feedback loop runs one
//!   hop per step. A delivery triggers the component (unless the port has no trigger); a
//!   component with a `Trigger` input (`CIRCUIT_FLAG_INPUT_SIGNAL`) only runs when that port
//!   gets a signal, and one with a `Triggered` output (`CIRCUIT_FLAG_OUTPUT_SIGNAL`) sends it
//!   after running. Every run while the circuit is on costs `energy_usage_per_input` (10 J)
//!   from its cell; without the charge nothing runs.
//! - Signals are numbers (`COMPONENT_SIGNAL`, the time in seconds). Values convert on the way
//!   in: strings never show entity names, booleans are 0/1, an option port keeps only its
//!   options, a list port only lists.
//! - Components: arithmetic, comparison, logic, not, random, length; concatenate, to string,
//!   to number, text case, string contains; list literal, index list; clock (0.9 s), delay;
//!   speech, radio (the signaler band), voice activator; and the shells' own (compact remote,
//!   controller, bot).
//!
//! What touches the world goes out as [`Effect`]s; [`super::server`] applies them.
// shortcut: no variables, setters/getters, modules, tables, assoc lists or instant execution;
// no admin circuits; a radio's `private` mode is `public` (no owner IDs); list literals do not
// count nested lists (`max_list_count`).

use bevy::prelude::Entity;

use super::{BASIC_TYPES, ComponentData, InputPortData, OutputPortData, Ty};
use crate::combat::CombatRng;

/// `energy_usage_per_input`: `0.001 * STANDARD_CELL_CHARGE`, joules.
pub const ENERGY_PER_INPUT: f64 = 10.0;
/// `SScircuit_component` `wait`.
pub const STEP: f64 = 0.1;
/// `COMP_CLOCK_DELAY`.
pub const CLOCK_DELAY: f64 = 0.9;
/// `COMP_DELAY_MIN_VALUE`.
pub const DELAY_MIN: f64 = 0.1;
/// The speech component's `speech_cooldown` and the radio's `signal_cooldown_time`.
pub const SPEECH_COOLDOWN: f64 = 1.0;
pub const SIGNAL_COOLDOWN: f64 = 1.0;
/// `PORT_MAX_STRING_LENGTH`, `PORT_MAX_STRING_DISPLAY`, `COMPONENT_MAX_POS`.
pub const MAX_STRING: usize = 5000;
pub const MAX_DISPLAY: usize = 100;
pub const MAX_POS: f64 = 10000.0;
/// `COMPONENT_MIN_RANDOM_POS`, `COMPONENT_MAX_RANDOM_POS`.
const RANDOM_POS: (u32, u32) = (200, 400);
/// The integrated circuit's `label_max_length`.
pub const LABEL_MAX: usize = 24;
/// `FREQ_SIGNALER`, `DEFAULT_SIGNALER_CODE`.
const FREQ_SIGNALER: f64 = 1457.0;
const DEFAULT_CODE: f64 = 30.0;
/// Deliveries per step.
// shortcut: a fixed cap instead of `MC_TICK_CHECK`; the rest waits for the next step.
const MAX_DELIVERIES: usize = 2000;

const IN_SIGNAL: u8 = 1;
const OUT_SIGNAL: u8 = 2;

pub const ARITHMETIC: [&str; 8] = [
    "Add", "Subtract", "Multiply", "Divide", "Modulo", "Minimum", "Maximum", "Power",
];
pub const COMPARISON: [&str; 6] = ["=", "!=", ">", "<", ">=", "<="];
pub const LOGIC: [&str; 3] = ["AND", "OR", "XOR"];
pub const TEXT_CASE: [&str; 2] = ["To Lower", "To Upper"];
pub const RADIO_PUBLICITY: [&str; 2] = ["public", "private"];

/// A port value (a DM value of the port's type).
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Value {
    #[default]
    Null,
    Num(f64),
    Str(String),
    Entity(Entity),
    List(Vec<Value>),
}

impl Value {
    pub fn truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Num(n) => *n != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::Entity(_) | Value::List(_) => true,
        }
    }

    pub fn num(&self) -> Option<f64> {
        match self {
            Value::Num(n) => Some(*n),
            _ => None,
        }
    }

    /// DM `"[value]"` (an entity shows as `entity`: names go through [`Ctx::name_of`]).
    pub fn text(&self) -> String {
        match self {
            Value::Null => String::new(),
            Value::Num(n) => crate::chem::dm_num(*n as f32),
            Value::Str(s) => s.clone(),
            Value::Entity(_) => "entity".into(),
            Value::List(_) => "/list".into(),
        }
    }
}

fn boolean(b: bool) -> Value {
    Value::Num(if b { 1.0 } else { 0.0 })
}

/// `COMPONENT_SIGNAL` (`world.time / (1 SECONDS)`); never 0, which would not trigger.
pub fn signal(now: f64) -> Value {
    Value::Num(now.max(STEP))
}

fn truncate(s: String, max: usize) -> String {
    if s.chars().count() < max {
        s
    } else {
        s.chars().take(max - 1).collect()
    }
}

/// `convert_value` of the port's datatype.
pub fn convert(ty: &Ty, v: Value, options: &[String]) -> Value {
    match ty {
        Ty::String => match v {
            Value::Null => Value::Null,
            Value::Entity(_) => Value::Str("entity".into()),
            other => Value::Str(truncate(other.text(), MAX_STRING)),
        },
        Ty::Boolean => boolean(v.truthy()),
        Ty::Option => match v {
            Value::Str(s) if options.contains(&s) => Value::Str(s),
            _ => options
                .first()
                .map_or(Value::Null, |o| Value::Str(o.clone())),
        },
        Ty::Entity | Ty::User => match v {
            Value::Entity(e) => Value::Entity(e),
            _ => Value::Null,
        },
        // `convert_value_extensive`: `for(data in value)` of a non-list is an empty list.
        Ty::List(inner) => match v {
            Value::List(items) => {
                Value::List(items.into_iter().map(|i| convert(inner, i, &[])).collect())
            }
            _ => Value::List(Vec::new()),
        },
        Ty::Any | Ty::Number | Ty::Signal => v,
    }
}

/// DM `text2num`: the number the text starts with.
pub fn text2num(s: &str) -> Option<f64> {
    let t = s.trim_start();
    let end = t
        .char_indices()
        .find(|&(_, c)| !(c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E')))
        .map_or(t.len(), |(i, _)| i);
    (1..=end)
        .rev()
        .find_map(|n| t[..n].parse::<f64>().ok().filter(|v| v.is_finite()))
}

/// What a port's update calls on its component (`trigger`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// `trigger = null`: the value is only stored.
    None,
    /// `input_received`.
    Input,
    /// The delay component's `trigger_delay`.
    Delay,
    /// The delay component's `interrupt_timer`.
    Interrupt,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InPort {
    pub name: String,
    pub ty: Ty,
    pub value: Value,
    pub trigger: Trigger,
    /// An option port's `possible_options`.
    pub options: Vec<String>,
    /// The outputs wired into it: (component id, output index).
    pub links: Vec<(u32, usize)>,
    /// Added by the component's `+`/`-` buttons (`circuit_component_add_port`).
    pub extra: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutPort {
    pub name: String,
    pub ty: Ty,
    pub value: Value,
}

fn inp(name: &str, ty: Ty, trigger: Trigger) -> InPort {
    InPort {
        name: name.into(),
        value: convert(&ty, Value::Null, &[]),
        ty,
        trigger,
        options: Vec::new(),
        links: Vec::new(),
        extra: false,
    }
}

fn opt(name: &str, options: &[&str]) -> InPort {
    let options: Vec<String> = options.iter().map(|&o| o.to_owned()).collect();
    InPort {
        name: name.into(),
        ty: Ty::Option,
        value: options
            .first()
            .map_or(Value::Null, |o| Value::Str(o.clone())),
        trigger: Trigger::Input,
        options,
        links: Vec::new(),
        extra: false,
    }
}

fn extra(prefix: &str, n: usize, ty: Ty) -> InPort {
    InPort {
        extra: true,
        ..inp(&format!("{prefix} {n}"), ty, Trigger::Input)
    }
}

fn out(name: &str, ty: Ty) -> OutPort {
    OutPort {
        name: name.into(),
        ty,
        value: Value::Null,
    }
}

fn sig(name: &str) -> InPort {
    inp(name, Ty::Signal, Trigger::Input)
}

/// The ported component types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Arithmetic,
    Comparison,
    Logic,
    Contains,
    Not,
    Random,
    Length,
    Concat,
    ToString,
    ToNumber,
    TextCase,
    ListLiteral,
    Index,
    Clock,
    Delay,
    Speech,
    Radio,
    Hear,
    CompactRemote,
    Controller,
    Bot,
    WireBundle,
    /// `/obj/item/circuit_component/modpc`: a modular computer shell's own component.
    Modpc,
}

pub struct KindDef {
    pub kind: Kind,
    pub path: &'static str,
    /// `display_name`.
    pub name: &'static str,
    pub category: &'static str,
    pub desc: &'static str,
}

const fn def(
    kind: Kind,
    path: &'static str,
    name: &'static str,
    category: &'static str,
    desc: &'static str,
) -> KindDef {
    KindDef {
        kind,
        path,
        name,
        category,
        desc,
    }
}

pub static KINDS: [KindDef; 23] = [
    def(
        Kind::Arithmetic,
        "/obj/item/circuit_component/arithmetic",
        "Arithmetic",
        "Math",
        "General arithmetic component with arithmetic capabilities.",
    ),
    def(
        Kind::Comparison,
        "/obj/item/circuit_component/compare/comparison",
        "Comparison",
        "Math",
        "A component that compares two objects.",
    ),
    def(
        Kind::Logic,
        "/obj/item/circuit_component/compare/logic",
        "Logic",
        "Math",
        "A component with 'and' and 'or' capabilities.",
    ),
    def(
        Kind::Contains,
        "/obj/item/circuit_component/compare/contains",
        "String Contains",
        "String",
        "Checks if a string contains a word/letter",
    ),
    def(
        Kind::Not,
        "/obj/item/circuit_component/not",
        "Not",
        "Math",
        "A component that inverts its input.",
    ),
    def(
        Kind::Random,
        "/obj/item/circuit_component/random",
        "Random",
        "Math",
        "A component that returns random values.",
    ),
    def(
        Kind::Length,
        "/obj/item/circuit_component/length",
        "Length",
        "Math",
        "A component that returns the length of its input.",
    ),
    def(
        Kind::Concat,
        "/obj/item/circuit_component/concat",
        "Concatenate",
        "String",
        "A component that combines strings.",
    ),
    def(
        Kind::ToString,
        "/obj/item/circuit_component/tostring",
        "To String",
        "String",
        "A component that converts its input to text.",
    ),
    def(
        Kind::ToNumber,
        "/obj/item/circuit_component/tonumber",
        "To Number",
        "String",
        "A component that converts its input (a string) to a number.",
    ),
    def(
        Kind::TextCase,
        "/obj/item/circuit_component/textcase",
        "Text Case",
        "String",
        "A component that makes its input uppercase or lowercase.",
    ),
    def(
        Kind::ListLiteral,
        "/obj/item/circuit_component/list_literal",
        "List Literal",
        "List",
        "A component that creates a list from whatever input you give it.",
    ),
    def(
        Kind::Index,
        "/obj/item/circuit_component/index",
        "Index List",
        "List",
        "A component that returns the value of a list at a given index.",
    ),
    def(
        Kind::Clock,
        "/obj/item/circuit_component/clock",
        "Clock",
        "Utility",
        "A component that repeatedly fires.",
    ),
    def(
        Kind::Delay,
        "/obj/item/circuit_component/delay",
        "Delay",
        "Utility",
        "A component that delays a signal by a specified duration. Timer gets reset when triggered again.",
    ),
    def(
        Kind::Speech,
        "/obj/item/circuit_component/speech",
        "Speech",
        "Action",
        "A component that sends a message. Requires a shell.",
    ),
    def(
        Kind::Radio,
        "/obj/item/circuit_component/radio",
        "Radio",
        "Action",
        "A component that can listen and send frequencies. It has a 1 second cooldown between sending signals.",
    ),
    def(
        Kind::Hear,
        "/obj/item/circuit_component/hear",
        "Voice Activator",
        "Entity",
        "A component that listens for messages. Requires a shell.",
    ),
    def(
        Kind::CompactRemote,
        "/obj/item/circuit_component/compact_remote",
        "Compact Remote",
        "Unassigned",
        "Used to receive inputs from the compact remote shell. Use the shell in hand to trigger the output signal.",
    ),
    def(
        Kind::Controller,
        "/obj/item/circuit_component/controller",
        "Controller",
        "Unassigned",
        "Used to receive inputs from the controller shell. Use the shell in hand to trigger the output signal.",
    ),
    def(
        Kind::Bot,
        "/obj/item/circuit_component/bot",
        "Bot",
        "Unassigned",
        "Triggers when someone interacts with the bot.",
    ),
    def(
        Kind::WireBundle,
        "/obj/item/circuit_component/wire_bundle",
        "Wire Bundle",
        "Utility",
        "A bundle of exposed wires that assemblies can be attached to. Ports will only show up once the circuit is inserted into a shell.",
    ),
    def(
        Kind::Modpc,
        "/obj/item/circuit_component/modpc",
        "Modular Computer",
        "Unassigned",
        "Circuit for basic functions of a modular computer.",
    ),
];

impl Kind {
    pub fn def(self) -> &'static KindDef {
        KINDS
            .iter()
            .find(|d| d.kind == self)
            .expect("every kind has a def")
    }

    /// The exact DM type (subtypes such as `index/assoc_string` are other components).
    pub fn from_path(path: &str) -> Option<Kind> {
        KINDS.iter().find(|d| d.path == path).map(|d| d.kind)
    }

    /// `circuit_component_add_port`: (minimum, maximum, prefix).
    fn extra_ports(self) -> Option<(usize, usize, &'static str)> {
        match self {
            Kind::Arithmetic | Kind::Logic | Kind::Concat => Some((2, 10, "Port")),
            Kind::ListLiteral => Some((1, 20, "Index")),
            _ => None,
        }
    }
}

/// Per-component state outside the ports.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NodeState {
    /// A running clock's next signal.
    pub clock_next: Option<f64>,
    /// A delay's pending output: when, and the value.
    pub delay: Option<(f64, Value)>,
    /// The frequency a radio listens on (`current_freq`, set by its first input).
    pub radio_freq: Option<u16>,
    /// The comparison's ports are numbers (`current_type`).
    pub numeric: bool,
}

/// A component in a circuit.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    /// Stable within its circuit; wires refer to it.
    pub id: u32,
    pub kind: Kind,
    pub inputs: Vec<InPort>,
    pub outputs: Vec<OutPort>,
    pub x: i32,
    pub y: i32,
    pub removable: bool,
    /// `circuit_size` (0 for a shell's own components).
    pub size: u32,
    /// Its item while it sits in the circuit; `None` for a shell's own components.
    pub item: Option<Entity>,
    flags: u8,
    trigger_in: Option<usize>,
    trigger_out: Option<usize>,
    pub state: NodeState,
}

impl Node {
    pub fn new(kind: Kind, id: u32) -> Node {
        use Trigger::{Delay, Input, Interrupt, None as NoTrigger};
        let both = IN_SIGNAL | OUT_SIGNAL;
        let compare_outs = || {
            vec![
                out("True", Ty::Signal),
                out("False", Ty::Signal),
                out("Result", Ty::Boolean),
            ]
        };
        let (mut inputs, outputs, flags) = match kind {
            Kind::Arithmetic => (
                vec![
                    opt("Arithmetic Option", &ARITHMETIC),
                    extra("Port", 1, Ty::Number),
                    extra("Port", 2, Ty::Number),
                ],
                vec![out("Output", Ty::Number)],
                both,
            ),
            Kind::Comparison => (
                vec![
                    opt("Comparison Option", &COMPARISON),
                    inp("A", Ty::Any, Input),
                    inp("B", Ty::Any, Input),
                    sig("Compare"),
                ],
                compare_outs(),
                0,
            ),
            Kind::Logic => (
                vec![
                    opt("Logic Options", &LOGIC),
                    extra("Port", 1, Ty::Any),
                    extra("Port", 2, Ty::Any),
                    sig("Compare"),
                ],
                compare_outs(),
                0,
            ),
            Kind::Contains => (
                vec![
                    inp("Needle", Ty::String, Input),
                    inp("Haystack", Ty::String, Input),
                    sig("Compare"),
                ],
                compare_outs(),
                0,
            ),
            Kind::Not => (
                vec![inp("Input", Ty::Any, Input)],
                vec![out("Result", Ty::Boolean)],
                both,
            ),
            Kind::Random => (
                vec![
                    inp("Minimum", Ty::Number, NoTrigger),
                    inp("Maximum", Ty::Number, NoTrigger),
                ],
                vec![out("Output", Ty::Number)],
                both,
            ),
            Kind::Length => (
                vec![inp("Input", Ty::Any, Input)],
                vec![out("Length", Ty::Number)],
                both,
            ),
            Kind::Concat => (
                vec![extra("Port", 1, Ty::String), extra("Port", 2, Ty::String)],
                vec![out("Output", Ty::String)],
                both,
            ),
            Kind::ToString => (
                vec![inp("Input", Ty::Any, Input)],
                vec![out("Output", Ty::String)],
                both,
            ),
            Kind::ToNumber => (
                vec![inp("Input", Ty::String, Input)],
                vec![out("Output", Ty::Number)],
                both,
            ),
            Kind::TextCase => (
                vec![
                    opt("Textcase Options", &TEXT_CASE),
                    inp("Input", Ty::String, Input),
                ],
                vec![out("Output", Ty::String)],
                both,
            ),
            Kind::ListLiteral => (
                vec![opt("List Type", &BASIC_TYPES), extra("Index", 1, Ty::Any)],
                vec![out("Value", Ty::List(Box::new(Ty::Any)))],
                both,
            ),
            Kind::Index => (
                vec![
                    opt("List Type", &BASIC_TYPES),
                    inp("Index", Ty::Number, Input),
                    inp("List", Ty::List(Box::new(Ty::Any)), Input),
                ],
                vec![out("Value", Ty::Any)],
                both,
            ),
            Kind::Clock => (
                vec![inp("On", Ty::Boolean, Input)],
                vec![out("Signal", Ty::Signal)],
                0,
            ),
            Kind::Delay => (
                vec![
                    inp("Delay", Ty::Number, NoTrigger),
                    inp("Trigger", Ty::Signal, Delay),
                    inp("Interrupt", Ty::Signal, Interrupt),
                ],
                vec![out("Result", Ty::Signal)],
                0,
            ),
            Kind::Speech => (
                vec![
                    inp("Message", Ty::String, NoTrigger),
                    inp("Quiet Mode", Ty::Boolean, Input),
                ],
                Vec::new(),
                both,
            ),
            Kind::Radio => {
                let mut freq = inp("Frequency", Ty::Number, Input);
                freq.value = Value::Num(FREQ_SIGNALER);
                let mut code = inp("Code", Ty::Number, Input);
                code.value = Value::Num(DEFAULT_CODE);
                (
                    vec![
                        opt("Encryption Options", &RADIO_PUBLICITY),
                        freq,
                        code,
                        sig("Send"),
                    ],
                    vec![out("Received", Ty::Signal)],
                    0,
                )
            }
            Kind::Hear => {
                let mut on = inp("On", Ty::Boolean, Input);
                on.value = boolean(true);
                (
                    vec![on],
                    vec![
                        out("Message", Ty::String),
                        out("Language", Ty::String),
                        out("Speaker", Ty::Entity),
                        out("Speaker Name", Ty::String),
                        out("Triggered", Ty::Signal),
                    ],
                    0,
                )
            }
            Kind::CompactRemote => (
                Vec::new(),
                vec![out("User", Ty::Entity), out("Signal", Ty::Signal)],
                0,
            ),
            Kind::Bot => (
                Vec::new(),
                vec![out("User", Ty::User), out("Signal", Ty::Signal)],
                0,
            ),
            // tg adds the light ports only to computers with a light; here every computer has
            // them and a computer without a light ignores them.
            Kind::Modpc => (
                vec![
                    sig("Turn On/Off"),
                    inp("Print Text", Ty::String, NoTrigger),
                    sig("Print"),
                    sig("Toggle Lights"),
                    inp("Red", Ty::Number, NoTrigger),
                    inp("Green", Ty::Number, NoTrigger),
                    inp("Blue", Ty::Number, NoTrigger),
                    sig("Set Color"),
                ],
                vec![out("Turned On", Ty::Signal), out("Shut Down", Ty::Signal)],
                0,
            ),
            // Its ports come with the shell's wires ([`Circuit::set_bundle_ports`]).
            Kind::WireBundle => (Vec::new(), Vec::new(), 0),
            Kind::Controller => (
                Vec::new(),
                vec![
                    out("User", Ty::User),
                    out("Signal", Ty::Signal),
                    out("Alternate Signal", Ty::Signal),
                    out("Extra Signal", Ty::Signal),
                ],
                0,
            ),
        };
        let mut outputs = outputs;
        let mut trigger_in = None;
        let mut trigger_out = None;
        if flags & IN_SIGNAL != 0 {
            inputs.push(sig("Trigger"));
            trigger_in = Some(inputs.len() - 1);
        }
        if flags & OUT_SIGNAL != 0 {
            outputs.push(out("Triggered", Ty::Signal));
            trigger_out = Some(outputs.len() - 1);
        }
        Node {
            id,
            kind,
            inputs,
            outputs,
            x: 0,
            y: 0,
            removable: true,
            size: 1,
            item: None,
            flags,
            trigger_in,
            trigger_out,
            state: NodeState::default(),
        }
    }

    /// The `Compare` port of the compare components (their last input).
    fn compare_port(&self) -> usize {
        self.inputs.len() - 1
    }

    fn extras(&self) -> Vec<Value> {
        self.inputs
            .iter()
            .filter(|p| p.extra)
            .map(|p| p.value.clone())
            .collect()
    }

    fn value(&self, port: usize) -> &Value {
        &self.inputs[port].value
    }

    fn option(&self, port: usize) -> String {
        self.inputs[port].value.text()
    }
}

/// What a run changes in the world.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// The shell says it (`say`, whispered range in quiet mode).
    Say { text: String, quiet: bool },
    /// The wire bundle's `Pulse <wire>` input: the assembly on that wire of the shell
    /// activates (`attached?.activate()`); `port` is the wire's index.
    BundleActivate { port: usize },
    /// A radio component posts a signal (`post_signal`).
    Radio {
        node: u32,
        frequency: u16,
        code: i32,
    },
    /// The modular computer component acts on its computer (`crate::modpc::circuit`).
    Computer(ComputerEffect),
}

/// What the modular computer component asks of its computer.
#[derive(Clone, Debug, PartialEq)]
pub enum ComputerEffect {
    /// `toggle_power`: shut down if on, else `turn_on`.
    TogglePower,
    /// `print_text` of the `Print Text` value.
    Print(String),
    /// `toggle_flashlight`.
    ToggleLight,
    /// `set_flashlight_color`: each channel clamped to 0-255.
    LightColor([u8; 3]),
}

/// What a run sees of the world.
pub struct Ctx<'a> {
    pub now: f64,
    /// The cell's charge, joules; `None` without a cell. Runs take from it.
    pub charge: Option<f64>,
    pub has_shell: bool,
    pub rng: &'a mut CombatRng,
    /// An entity's name when it is in range of the circuit (`tostring`'s `max_range`).
    pub name_of: &'a dyn Fn(Entity) -> Option<String>,
    pub effects: Vec<Effect>,
}

#[derive(Default)]
struct Outcome {
    outs: Vec<(usize, Value)>,
    /// `input_received` returned true: no `Triggered` signal.
    stop: bool,
}

/// tg `/obj/item/integrated_circuit`'s logic.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Circuit {
    pub nodes: Vec<Node>,
    next_id: u32,
    /// Set by the shell.
    pub on: bool,
    pub display_name: String,
    /// Deliveries for the next step: (component id, input index, value).
    pending: Vec<(u32, usize, Value)>,
    /// `current_size`.
    pub size: u32,
    /// `COOLDOWN_SIGNALLER_SEND` on the circuit.
    pub signal_ready: f64,
    /// `COOLDOWN_CIRCUIT_SPEECH` on the shell.
    pub speech_ready: f64,
}

impl Circuit {
    pub fn index_of(&self, id: u32) -> Option<usize> {
        self.nodes.iter().position(|n| n.id == id)
    }

    /// Something to do on the next steps: deliveries, running clocks, pending delays.
    pub fn busy(&self) -> bool {
        !self.pending.is_empty()
            || self
                .nodes
                .iter()
                .any(|n| n.state.clock_next.is_some() || n.state.delay.is_some())
    }

    /// `add_component`: at a random spot of the board. A shell's own components are
    /// unremovable and take no space.
    pub fn add(
        &mut self,
        kind: Kind,
        item: Option<Entity>,
        removable: bool,
        rng: &mut CombatRng,
    ) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let mut node = Node::new(kind, id);
        node.item = item;
        node.removable = removable;
        node.size = u32::from(removable);
        node.x = rng.range(RANDOM_POS.0, RANDOM_POS.1) as i32;
        node.y = rng.range(RANDOM_POS.0, RANDOM_POS.1) as i32;
        self.size += node.size;
        self.nodes.push(node);
        id
    }

    /// `remove_component` + `disconnect`: its wires go.
    pub fn remove(&mut self, id: u32) -> Option<Node> {
        let i = self.index_of(id)?;
        let node = self.nodes.remove(i);
        for n in &mut self.nodes {
            for p in &mut n.inputs {
                p.links.retain(|&(from, _)| from != id);
            }
        }
        self.pending.retain(|&(to, _, _)| to != id);
        self.size -= node.size;
        Some(node)
    }

    /// `set_shell`: the shell's components join, the circuit turns on and every component
    /// runs once.
    pub fn attach_shell(&mut self, parts: &[Kind], ctx: &mut Ctx) {
        for &kind in parts {
            self.add(kind, None, false, ctx.rng);
        }
        self.on = true;
        let ids: Vec<u32> = self.nodes.iter().map(|n| n.id).collect();
        for id in ids {
            self.trigger(id, None, ctx);
        }
    }

    /// `remove_current_shell`: the shell's components leave, the circuit turns off.
    pub fn detach_shell(&mut self) {
        let parts: Vec<u32> = self
            .nodes
            .iter()
            .filter(|n| !n.removable && n.item.is_none())
            .map(|n| n.id)
            .collect();
        for id in parts {
            self.remove(id);
        }
        self.on = false;
    }

    fn output_ty(&self, id: u32, port: usize) -> Option<&Ty> {
        let i = self.index_of(id)?;
        self.nodes[i].outputs.get(port).map(|p| &p.ty)
    }

    /// `ui_act("add_connection")` by 0-based positions.
    pub fn connect(
        &mut self,
        input: (usize, usize),
        output: (usize, usize),
        ctx: &mut Ctx,
    ) -> bool {
        let (Some(to), Some(from)) = (self.nodes.get(input.0), self.nodes.get(output.0)) else {
            return false;
        };
        let (Some(in_port), Some(out_port)) = (to.inputs.get(input.1), from.outputs.get(output.1))
        else {
            return false;
        };
        if !in_port.ty.can_receive_from(&out_port.ty) {
            return false;
        }
        let link = (from.id, output.1);
        if in_port.links.contains(&link) {
            return true;
        }
        let (to_id, value, avoid_update) =
            (to.id, out_port.value.clone(), in_port.ty == Ty::Signal);
        self.nodes[input.0].inputs[input.1].links.push(link);
        // `DATATYPE_FLAG_AVOID_VALUE_UPDATE`: wiring a signal does not fire it.
        if !avoid_update {
            self.set_input(to_id, input.1, value, ctx);
        }
        true
    }

    /// `ui_act("remove_connection")`: the port's `disconnect_all` (its value goes too).
    pub fn disconnect(&mut self, component: usize, is_input: bool, port: usize) -> bool {
        let Some(node) = self.nodes.get_mut(component) else {
            return false;
        };
        let id = node.id;
        if is_input {
            let Some(p) = node.inputs.get_mut(port) else {
                return false;
            };
            p.value = Value::Null;
            p.links.clear();
        } else {
            let Some(p) = node.outputs.get_mut(port) else {
                return false;
            };
            p.value = Value::Null;
            for n in &mut self.nodes {
                for p in &mut n.inputs {
                    p.links.retain(|&l| l != (id, port));
                }
            }
        }
        true
    }

    /// `set_output`: every wired input gets it on the next step.
    pub fn set_output(&mut self, id: u32, port: usize, value: Value) {
        let Some(i) = self.index_of(id) else {
            return;
        };
        let Some(p) = self.nodes[i].outputs.get_mut(port) else {
            return;
        };
        let value = convert(&p.ty, value, &[]);
        p.value = value.clone();
        for n in &self.nodes {
            for (j, input) in n.inputs.iter().enumerate() {
                if input.links.contains(&(id, port)) {
                    self.pending.push((n.id, j, value.clone()));
                }
            }
        }
    }

    /// `set_input`: stores the value and triggers the component.
    pub fn set_input(&mut self, id: u32, port: usize, value: Value, ctx: &mut Ctx) {
        let Some(i) = self.index_of(id) else {
            return;
        };
        let Some(p) = self.nodes[i].inputs.get_mut(port) else {
            return;
        };
        p.value = convert(&p.ty, value, &p.options);
        if p.trigger != Trigger::None {
            self.trigger(id, Some(port), ctx);
        }
    }

    /// `trigger_component`.
    pub fn trigger(&mut self, id: u32, port: Option<usize>, ctx: &mut Ctx) -> bool {
        self.pre_input(id, port);
        let Some(i) = self.index_of(id) else {
            return false;
        };
        if !self.should_receive(i, port, ctx) {
            return false;
        }
        let outcome = match port.map(|p| self.nodes[i].inputs[p].trigger) {
            Some(Trigger::None) => return false,
            Some(Trigger::Delay) => self.delay_trigger(i, ctx.now),
            Some(Trigger::Interrupt) => {
                self.nodes[i].state.delay = None;
                Outcome::default()
            }
            _ => self.input_received(i, port, ctx),
        };
        for (p, v) in outcome.outs {
            self.set_output(id, p, v);
        }
        if outcome.stop {
            return false;
        }
        if let Some(i) = self.index_of(id)
            && let Some(t) = self.nodes[i].trigger_out
            && self.nodes[i].flags & OUT_SIGNAL != 0
        {
            self.set_output(id, t, signal(ctx.now));
        }
        true
    }

    /// `should_receive_input`: on, powered, and for `Trigger`ed components, by that port.
    fn should_receive(&self, i: usize, port: Option<usize>, ctx: &mut Ctx) -> bool {
        if !self.on {
            return false;
        }
        match &mut ctx.charge {
            Some(c) if *c >= ENERGY_PER_INPUT => *c -= ENERGY_PER_INPUT,
            _ => return false,
        }
        let n = &self.nodes[i];
        let default_trigger = port.is_none_or(|p| n.inputs[p].trigger == Trigger::Input);
        let by_trigger =
            port.is_some_and(|p| Some(p) == n.trigger_in && n.inputs[p].value.truthy());
        if default_trigger && n.flags & IN_SIGNAL != 0 && !by_trigger {
            return false;
        }
        true
    }

    /// `pre_input_received`: port types that follow an option, the radio's frequency.
    fn pre_input(&mut self, id: u32, port: Option<usize>) {
        let Some(i) = self.index_of(id) else {
            return;
        };
        let n = &self.nodes[i];
        let kind = n.kind;
        match kind {
            Kind::Comparison => {
                let numeric = !matches!(n.option(0).as_str(), "=" | "!=");
                if n.state.numeric != numeric {
                    self.nodes[i].state.numeric = numeric;
                    let ty = if numeric { Ty::Number } else { Ty::Any };
                    self.set_input_type(i, 1, ty.clone());
                    self.set_input_type(i, 2, ty);
                }
            }
            Kind::ListLiteral if port == Some(0) => {
                let ty = Ty::parse(&n.option(0)).unwrap_or(Ty::Any);
                let extras: Vec<usize> =
                    (0..n.inputs.len()).filter(|&p| n.inputs[p].extra).collect();
                self.set_output_type(i, 0, Ty::List(Box::new(ty.clone())));
                for p in extras {
                    self.set_input_type(i, p, ty.clone());
                }
            }
            Kind::Index if port == Some(0) => {
                let ty = Ty::parse(&n.option(0)).unwrap_or(Ty::Any);
                self.set_input_type(i, 2, Ty::List(Box::new(ty.clone())));
                self.set_output_type(i, 0, ty);
            }
            Kind::Radio => {
                let freq = n.value(1).num().unwrap_or(0.0);
                let freq = ss13_chat::radio::sanitize_frequency(freq, true, false);
                self.nodes[i].inputs[1].value = Value::Num(f64::from(freq));
            }
            _ => {}
        }
    }

    /// `set_datatype` of an input: its value goes, wires it cannot take break.
    fn set_input_type(&mut self, i: usize, port: usize, ty: Ty) {
        if self.nodes[i].inputs[port].ty == ty {
            return;
        }
        let links = self.nodes[i].inputs[port].links.clone();
        let kept: Vec<(u32, usize)> = links
            .into_iter()
            .filter(|&(n, o)| self.output_ty(n, o).is_some_and(|t| ty.can_receive_from(t)))
            .collect();
        let p = &mut self.nodes[i].inputs[port];
        p.ty = ty;
        p.value = Value::Null;
        p.links = kept;
    }

    /// `set_datatype` of an output: wired inputs that cannot take it disconnect.
    fn set_output_type(&mut self, i: usize, port: usize, ty: Ty) {
        if self.nodes[i].outputs[port].ty == ty {
            return;
        }
        let id = self.nodes[i].id;
        self.nodes[i].outputs[port].ty = ty.clone();
        self.nodes[i].outputs[port].value = Value::Null;
        for n in &mut self.nodes {
            for p in &mut n.inputs {
                if p.links.contains(&(id, port)) && !p.ty.can_receive_from(&ty) {
                    p.links.retain(|&l| l != (id, port));
                }
            }
        }
    }

    fn delay_trigger(&mut self, i: usize, now: f64) -> Outcome {
        let n = &mut self.nodes[i];
        let delay = n.value(0).num().unwrap_or(0.0);
        let value = n.value(1).clone();
        if delay > DELAY_MIN {
            n.state.delay = Some((now + delay, value));
            Outcome::default()
        } else {
            n.state.delay = None;
            Outcome {
                outs: vec![(0, value)],
                stop: false,
            }
        }
    }

    /// `input_received` of each component.
    fn input_received(&mut self, i: usize, port: Option<usize>, ctx: &mut Ctx) -> Outcome {
        let id = self.nodes[i].id;
        let n = &self.nodes[i];
        let mut outs = Vec::new();
        let kind = n.kind;
        match kind {
            Kind::Arithmetic => outs.push((0, arithmetic(&n.option(0), &n.extras()))),
            Kind::Comparison | Kind::Logic | Kind::Contains => {
                let result = match n.kind {
                    Kind::Comparison => compare(&n.option(0), n.value(1), n.value(2)),
                    Kind::Logic => logic(&n.option(0), &n.extras()),
                    _ => contains(n.value(0), n.value(1)),
                };
                let compare_port = n.compare_port();
                if port == Some(compare_port) && n.value(compare_port).truthy() {
                    outs.push((if result { 0 } else { 1 }, signal(ctx.now)));
                }
                outs.push((2, boolean(result)));
            }
            Kind::Not => outs.push((0, boolean(!n.value(0).truthy()))),
            Kind::Random => {
                let low = n.value(0).num().unwrap_or(0.0).round();
                let high = n.value(1).num().unwrap_or(0.0).round();
                let result = if high < low {
                    0.0
                } else {
                    let span = (high - low).min(f64::from(u32::MAX - 1)) as u32;
                    low + f64::from(ctx.rng.range(0, span))
                };
                outs.push((0, Value::Num(result)));
            }
            Kind::Length => {
                let len = match n.value(0) {
                    Value::Str(s) => s.chars().count(),
                    Value::List(l) => l.len(),
                    _ => 0,
                };
                outs.push((0, Value::Num(len as f64)));
            }
            Kind::Concat => {
                let text: String = n
                    .extras()
                    .iter()
                    .filter(|v| **v != Value::Null)
                    .map(Value::text)
                    .collect();
                outs.push((0, Value::Str(text)));
            }
            Kind::ToString => {
                let text = match n.value(0) {
                    Value::Entity(e) => (ctx.name_of)(*e).unwrap_or_else(|| "entity".into()),
                    v => v.text(),
                };
                outs.push((0, Value::Str(text)));
            }
            Kind::ToNumber => {
                let v = text2num(&n.value(0).text()).map_or(Value::Null, Value::Num);
                outs.push((0, v));
            }
            Kind::TextCase => {
                if let Value::Str(s) = n.value(1) {
                    let s = if n.option(0) == TEXT_CASE[1] {
                        s.to_uppercase()
                    } else {
                        s.to_lowercase()
                    };
                    outs.push((0, Value::Str(s)));
                }
            }
            Kind::ListLiteral => {
                let ty = Ty::parse(&n.option(0)).unwrap_or(Ty::Any);
                let items = n
                    .extras()
                    .into_iter()
                    .map(|v| convert(&ty, v, &[]))
                    .collect();
                outs.push((0, Value::List(items)));
            }
            Kind::Index => {
                let v = match (n.value(1), n.value(2)) {
                    (Value::Num(index), Value::List(list)) if *index != 0.0 => {
                        let index = index.floor();
                        if (1.0..=list.len() as f64).contains(&index) {
                            list[index as usize - 1].clone()
                        } else {
                            Value::Null
                        }
                    }
                    _ => Value::Null,
                };
                outs.push((0, v));
            }
            Kind::Clock => {
                let on = n.value(0).truthy();
                let state = &mut self.nodes[i].state;
                if !on {
                    state.clock_next = None;
                } else if state.clock_next.is_none() {
                    state.clock_next = Some(ctx.now + CLOCK_DELAY);
                }
            }
            Kind::Speech => {
                if ctx.has_shell && ctx.now >= self.speech_ready && n.value(0).truthy() {
                    ctx.effects.push(Effect::Say {
                        text: n.value(0).text(),
                        quiet: n.value(1).num().unwrap_or(0.0) > 0.0,
                    });
                    self.speech_ready = ctx.now + SPEECH_COOLDOWN;
                }
            }
            Kind::Radio => {
                if ctx.now < self.signal_ready {
                    return Outcome::default();
                }
                let frequency = n.value(1).num().unwrap_or(0.0) as u16;
                let send = port == Some(3) && n.value(3).truthy();
                let code = n.value(2).num().unwrap_or(0.0).round() as i32;
                self.nodes[i].state.radio_freq = Some(frequency);
                if send {
                    self.signal_ready = ctx.now + SIGNAL_COOLDOWN;
                    ctx.effects.push(Effect::Radio {
                        node: id,
                        frequency,
                        code,
                    });
                }
            }
            Kind::WireBundle => {
                if let Some(port) = port {
                    ctx.effects.push(Effect::BundleActivate { port });
                }
            }
            Kind::Modpc if ctx.has_shell => {
                let channel = |p: usize| n.value(p).num().unwrap_or(0.0).clamp(0.0, 255.0) as u8;
                let effect = match port {
                    Some(0) => Some(ComputerEffect::TogglePower),
                    Some(2) => Some(ComputerEffect::Print(n.value(1).text())),
                    Some(3) => Some(ComputerEffect::ToggleLight),
                    Some(7) => Some(ComputerEffect::LightColor([
                        channel(4),
                        channel(5),
                        channel(6),
                    ])),
                    _ => None,
                };
                ctx.effects.extend(effect.map(Effect::Computer));
            }
            Kind::Delay
            | Kind::Hear
            | Kind::CompactRemote
            | Kind::Controller
            | Kind::Bot
            | Kind::Modpc => {}
        }
        Outcome { outs, stop: false }
    }

    /// One `SScircuit_component` step: the deliveries queued before it, then due clocks
    /// (`SSclock_component`) and delays (`addtimer`).
    pub fn tick(&mut self, ctx: &mut Ctx) {
        let mut batch = std::mem::take(&mut self.pending);
        if batch.len() > MAX_DELIVERIES {
            let rest = batch.split_off(MAX_DELIVERIES);
            self.pending = rest;
        }
        for (id, port, value) in batch {
            self.set_input(id, port, value, ctx);
        }
        let now = ctx.now;
        let due: Vec<(u32, Option<f64>, Option<Value>)> = self
            .nodes
            .iter()
            .map(|n| {
                let clock = n.state.clock_next.filter(|&t| t <= now);
                let delay = n
                    .state
                    .delay
                    .as_ref()
                    .filter(|(t, _)| *t <= now)
                    .map(|(_, v)| v.clone());
                (n.id, clock, delay)
            })
            .filter(|(_, c, d)| c.is_some() || d.is_some())
            .collect();
        for (id, clock, delay) in due {
            if let Some(at) = clock {
                if let Some(i) = self.index_of(id) {
                    self.nodes[i].state.clock_next = Some((at + CLOCK_DELAY).max(now));
                }
                self.set_output(id, 0, signal(now));
            }
            if let Some(v) = delay {
                if let Some(i) = self.index_of(id) {
                    self.nodes[i].state.delay = None;
                }
                self.set_output(id, 0, v);
            }
        }
    }

    /// Whether a wire bundle is in the circuit (`TRAIT_COMPONENT_WIRE_BUNDLE`: one at most).
    pub fn has_bundle(&self) -> bool {
        self.nodes.iter().any(|n| n.kind == Kind::WireBundle)
    }

    /// `wire_bundle/register_shell`: a `Pulse <wire>` input and a `<wire> Pulsed` output per
    /// wire of the shell.
    pub fn set_bundle_ports(&mut self, wires: &[String]) {
        for n in self.nodes.iter_mut().filter(|n| n.kind == Kind::WireBundle) {
            n.inputs = wires.iter().map(|w| sig(&format!("Pulse {w}"))).collect();
            n.outputs = wires
                .iter()
                .map(|w| out(&format!("{w} Pulsed"), Ty::Signal))
                .collect();
        }
    }

    /// `wire_bundle/unregister_shell`: its ports go, with the wires into and out of them.
    pub fn clear_bundle_ports(&mut self) {
        let ids: Vec<u32> = self
            .nodes
            .iter()
            .filter(|n| n.kind == Kind::WireBundle)
            .map(|n| n.id)
            .collect();
        for n in &mut self.nodes {
            if n.kind == Kind::WireBundle {
                n.inputs.clear();
                n.outputs.clear();
            }
            for p in &mut n.inputs {
                p.links.retain(|(from, _)| !ids.contains(from));
            }
        }
        self.pending.retain(|(to, _, _)| !ids.contains(to));
    }

    /// `wire_bundle/on_pulse_wire`: the wire's `Pulsed` output signals.
    pub fn bundle_pulsed(&mut self, index: usize, now: f64) {
        let ids: Vec<u32> = self
            .nodes
            .iter()
            .filter(|n| n.kind == Kind::WireBundle && index < n.outputs.len())
            .map(|n| n.id)
            .collect();
        for id in ids {
            self.set_output(id, index, signal(now));
        }
    }

    /// A shell's own component sends a signal on `port` (no `User` port: the modular
    /// computer's `Turned On` / `Shut Down`).
    pub fn shell_output(&mut self, kind: Kind, port: usize, now: f64) -> bool {
        let Some(id) = self.nodes.iter().find(|n| n.kind == kind).map(|n| n.id) else {
            return false;
        };
        self.set_output(id, port, signal(now));
        true
    }

    /// A shell's own component sends its signal: the user on its `User` port, then `port`.
    pub fn shell_event(&mut self, kind: Kind, port: usize, user: Entity, now: f64) -> bool {
        let Some(id) = self.nodes.iter().find(|n| n.kind == kind).map(|n| n.id) else {
            return false;
        };
        self.set_output(id, 0, Value::Entity(user));
        self.set_output(id, port, signal(now));
        true
    }

    /// `receive_signal` of the radios listening on `frequency` with the same code.
    pub fn radio_receive(&mut self, from: Option<u32>, frequency: u16, code: i32, now: f64) {
        let hits: Vec<u32> = self
            .nodes
            .iter()
            .filter(|n| {
                n.kind == Kind::Radio
                    && Some(n.id) != from
                    && n.state.radio_freq == Some(frequency)
                    && n.value(2).num().unwrap_or(0.0).round() as i32 == code
            })
            .map(|n| n.id)
            .collect();
        for id in hits {
            self.set_output(id, 0, signal(now));
        }
    }

    /// Voice activators: `Hear`.
    pub fn hear(
        &mut self,
        speaker: Entity,
        shell: Option<Entity>,
        message: &str,
        language: &str,
        voice: &str,
        now: f64,
    ) {
        if shell == Some(speaker) {
            return;
        }
        let hits: Vec<u32> = self
            .nodes
            .iter()
            .filter(|n| n.kind == Kind::Hear && n.value(0).truthy())
            .map(|n| n.id)
            .collect();
        for id in hits {
            self.set_output(id, 0, Value::Str(message.to_owned()));
            self.set_output(id, 1, Value::Str(language.to_owned()));
            self.set_output(id, 2, Value::Entity(speaker));
            self.set_output(id, 3, Value::Str(voice.to_owned()));
            self.set_output(id, 4, signal(now));
        }
    }

    /// `ui_act("set_component_input")` by 0-based positions: `None` is `set_null`. The
    /// handled text (`handle_manual_input`) goes in; a signal answers with its balloon.
    pub fn manual_input(
        &mut self,
        component: usize,
        port: usize,
        input: Option<&str>,
        ctx: &mut Ctx,
    ) -> Option<String> {
        let n = self.nodes.get(component)?;
        let p = n.inputs.get(port)?;
        let id = n.id;
        let Some(text) = input else {
            self.set_input(id, port, Value::Null, ctx);
            return None;
        };
        if !p.ty.manual_input() {
            return None;
        }
        let mut balloon = None;
        let value = match p.ty {
            Ty::Number => Value::Num(text2num(text)?),
            Ty::Any => text2num(text).map_or_else(|| Value::Str(text.to_owned()), Value::Num),
            Ty::Signal => {
                balloon = Some(format!("triggered {}", p.name));
                signal(ctx.now)
            }
            Ty::Boolean => boolean(text2num(text).map_or(!text.is_empty(), |v| v != 0.0)),
            _ => Value::Str(text.to_owned()),
        };
        self.set_input(id, port, value, ctx);
        balloon
    }

    /// `ui_act("get_component_value")`: `"<port> value: <text>"`.
    pub fn output_text(&self, component: usize, port: usize) -> Option<String> {
        let p = self.nodes.get(component)?.outputs.get(port)?;
        let mut text = match &p.value {
            Value::Null => "null".to_owned(),
            v => v.text(),
        };
        if text.chars().count() >= MAX_DISPLAY - 1 {
            text = text.chars().take(MAX_DISPLAY - 1).collect();
            text.push_str("...");
        }
        Some(format!("{} value: {text}", p.name))
    }

    /// `ui_perform_action`: the `+`/`-` buttons of the components with variable ports.
    pub fn perform_action(&mut self, component: usize, action: &str) -> bool {
        let Some(n) = self.nodes.get_mut(component) else {
            return false;
        };
        let Some((min, max, prefix)) = n.kind.extra_ports() else {
            return false;
        };
        let extras: Vec<usize> = (0..n.inputs.len()).filter(|&p| n.inputs[p].extra).collect();
        let (Some(&first), Some(&last)) = (extras.first(), extras.last()) else {
            return false;
        };
        let id = n.id;
        match action {
            "add" if extras.len() < max => {
                let ty = n.inputs[first].ty.clone();
                n.inputs
                    .insert(last + 1, extra(prefix, extras.len() + 1, ty));
                if let Some(t) = &mut n.trigger_in
                    && *t > last
                {
                    *t += 1;
                }
                for (to, p, _) in &mut self.pending {
                    if *to == id && *p > last {
                        *p += 1;
                    }
                }
            }
            "remove" if extras.len() > min => {
                n.inputs.remove(last);
                if let Some(t) = &mut n.trigger_in
                    && *t > last
                {
                    *t -= 1;
                }
                self.pending.retain(|&(to, p, _)| !(to == id && p == last));
                for (to, p, _) in &mut self.pending {
                    if *to == id && *p > last {
                        *p -= 1;
                    }
                }
            }
            _ => return false,
        }
        true
    }

    /// `ui_act("set_component_coordinates")`.
    pub fn set_coordinates(&mut self, component: usize, x: f64, y: f64) -> bool {
        let Some(n) = self.nodes.get_mut(component) else {
            return false;
        };
        n.x = x.clamp(-MAX_POS, MAX_POS) as i32;
        n.y = y.clamp(-MAX_POS, MAX_POS) as i32;
        true
    }

    /// `set_display_name`: `copytext_char(name, 1, label_max_length)`.
    pub fn set_display_name(&mut self, name: &str) {
        self.display_name = name.chars().take(LABEL_MAX - 1).collect();
    }

    /// `ui_data`'s components.
    pub fn ui_data(&self) -> Vec<ComponentData> {
        self.nodes
            .iter()
            .map(|n| {
                let d = n.kind.def();
                ComponentData {
                    name: d.name.to_owned(),
                    category: d.category.to_owned(),
                    color: "blue".to_owned(),
                    x: n.x,
                    y: n.y,
                    removable: n.removable,
                    input_ports: n
                        .inputs
                        .iter()
                        .enumerate()
                        .map(|(j, p)| InputPortData {
                            name: p.name.clone(),
                            datatype: p.ty.name(),
                            color: p.ty.color().to_owned(),
                            port_ref: format!("i{}.{j}", n.id),
                            connected_to: p
                                .links
                                .iter()
                                .map(|(from, o)| format!("o{from}.{o}"))
                                .collect(),
                            current_data: match &p.value {
                                Value::Null | Value::Entity(_) => None,
                                Value::List(l) => Some(format!("list ({})", l.len())),
                                v => Some(v.text()),
                            },
                            options: p.options.clone(),
                        })
                        .collect(),
                    output_ports: n
                        .outputs
                        .iter()
                        .enumerate()
                        .map(|(j, p)| OutputPortData {
                            name: p.name.clone(),
                            datatype: p.ty.name(),
                            color: p.ty.color().to_owned(),
                            port_ref: format!("o{}.{j}", n.id),
                        })
                        .collect(),
                    ui_buttons: if n.kind.extra_ports().is_some() {
                        vec![
                            ("plus".to_owned(), "add".to_owned()),
                            ("minus".to_owned(), "remove".to_owned()),
                        ]
                    } else {
                        Vec::new()
                    },
                }
            })
            .collect()
    }
}

/// The arithmetic component: null ports are skipped; a division by zero gives null.
fn arithmetic(op: &str, ports: &[Value]) -> Value {
    let Some((first, rest)) = ports.split_first() else {
        return Value::Null;
    };
    let mut result = first.num();
    for v in rest {
        let Some(v) = v.num() else {
            continue;
        };
        let r = result.unwrap_or(0.0);
        result = Some(match op {
            "Subtract" => r - v,
            "Multiply" => r * v,
            "Divide" => {
                if v == 0.0 {
                    return Value::Null;
                }
                r / v
            }
            // DM `%` works on integers. A zero divisor is a runtime error in tg: null here.
            "Modulo" => {
                let d = v.trunc() as i64;
                if d == 0 {
                    return Value::Null;
                }
                ((r.trunc() as i64) % d) as f64
            }
            "Minimum" => r.min(v),
            "Maximum" => r.max(v),
            "Power" => r.powf(v),
            _ => r + v,
        });
    }
    result.map_or(Value::Null, Value::Num)
}

fn compare(op: &str, a: &Value, b: &Value) -> bool {
    let (x, y) = (a.num().unwrap_or(0.0), b.num().unwrap_or(0.0));
    match op {
        "!=" => a != b,
        ">" => x > y,
        "<" => x < y,
        ">=" => x >= y,
        "<=" => x <= y,
        _ => a == b,
    }
}

fn logic(op: &str, ports: &[Value]) -> bool {
    let truthy = ports.iter().filter(|v| v.truthy()).count();
    match op {
        "OR" => truthy > 0,
        "XOR" => truthy > 0 && truthy < ports.len(),
        _ => !ports.is_empty() && truthy == ports.len(),
    }
}

/// `findtext` (case-insensitive).
fn contains(needle: &Value, haystack: &Value) -> bool {
    match (needle, haystack) {
        (Value::Str(n), Value::Str(h)) if !n.is_empty() && !h.is_empty() => {
            h.to_lowercase().contains(&n.to_lowercase())
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_names(_: Entity) -> Option<String> {
        None
    }

    struct Bench {
        circuit: Circuit,
        rng: CombatRng,
        charge: f64,
        now: f64,
        effects: Vec<Effect>,
    }

    impl Bench {
        fn new() -> Self {
            let circuit = Circuit {
                on: true,
                ..Default::default()
            };
            Self {
                circuit,
                rng: CombatRng(7),
                charge: 10_000.0,
                now: 10.0,
                effects: Vec::new(),
            }
        }

        fn with<R>(&mut self, f: impl FnOnce(&mut Circuit, &mut Ctx) -> R) -> R {
            let mut ctx = Ctx {
                now: self.now,
                charge: Some(self.charge),
                has_shell: true,
                rng: &mut self.rng,
                name_of: &no_names,
                effects: Vec::new(),
            };
            let r = f(&mut self.circuit, &mut ctx);
            self.charge = ctx.charge.unwrap_or(0.0);
            self.effects.extend(ctx.effects);
            r
        }

        fn add(&mut self, kind: Kind) -> usize {
            let id = self.circuit.add(kind, None, true, &mut self.rng);
            self.circuit.index_of(id).unwrap()
        }

        fn input(&mut self, c: usize, p: usize, text: &str) {
            self.with(|circuit, ctx| circuit.manual_input(c, p, Some(text), ctx));
        }

        fn connect(&mut self, input: (usize, usize), output: (usize, usize)) -> bool {
            self.with(|c, ctx| c.connect(input, output, ctx))
        }

        fn step(&mut self) {
            self.now += STEP;
            self.with(|c, ctx| c.tick(ctx));
        }

        fn out(&self, c: usize, p: usize) -> &Value {
            &self.circuit.nodes[c].outputs[p].value
        }
    }

    #[test]
    fn arithmetic_runs_on_its_trigger_only() {
        let mut b = Bench::new();
        let add = b.add(Kind::Arithmetic);
        b.input(add, 1, "2");
        b.input(add, 2, "3.5");
        assert_eq!(*b.out(add, 0), Value::Null, "no trigger yet");
        b.input(add, 3, "");
        assert_eq!(*b.out(add, 0), Value::Num(5.5));
        assert!(b.out(add, 1).truthy(), "Triggered fires");
        b.input(add, 0, "Divide");
        b.input(add, 2, "0");
        b.input(add, 3, "");
        assert_eq!(*b.out(add, 0), Value::Null, "division by zero");
        assert_eq!(
            arithmetic("Modulo", &[Value::Num(7.5), Value::Num(2.0)]),
            Value::Num(1.0)
        );
        assert_eq!(
            arithmetic("Subtract", &[Value::Null, Value::Num(4.0)]),
            Value::Num(-4.0)
        );
        assert_eq!(arithmetic("Add", &[Value::Null, Value::Null]), Value::Null);
    }

    #[test]
    fn wires_deliver_on_the_next_step_and_chain() {
        let mut b = Bench::new();
        let add = b.add(Kind::Arithmetic);
        let tostring = b.add(Kind::ToString);
        let concat = b.add(Kind::Concat);
        // Arithmetic Output -> To String Input; Triggered -> To String Trigger.
        assert!(b.connect((tostring, 0), (add, 0)));
        assert!(b.connect((tostring, 1), (add, 1)));
        assert!(b.connect((concat, 1), (tostring, 0)));
        assert!(b.connect((concat, 2), (tostring, 1)));
        assert!(!b.connect((add, 1), (tostring, 0)), "string into number");
        b.input(concat, 0, "x=");
        b.input(add, 1, "40");
        b.input(add, 2, "2");
        b.input(add, 3, "");
        assert_eq!(*b.out(tostring, 0), Value::Null, "queued for the next step");
        b.step();
        assert_eq!(*b.out(tostring, 0), Value::Str("42".into()));
        b.step();
        assert_eq!(*b.out(concat, 0), Value::Str("x=42".into()));
        // Energy: every run of a powered circuit takes 10 J.
        assert!(b.charge < 10_000.0);
    }

    #[test]
    fn no_charge_no_run() {
        let mut b = Bench::new();
        let not = b.add(Kind::Not);
        b.charge = 5.0;
        b.input(not, 1, "");
        assert_eq!(*b.out(not, 0), Value::Null);
        b.charge = 100.0;
        b.input(not, 1, "");
        assert_eq!(*b.out(not, 0), Value::Num(1.0));
        b.circuit.on = false;
        b.input(not, 0, "1");
        b.input(not, 1, "");
        assert_eq!(*b.out(not, 0), Value::Num(1.0), "off: unchanged");
    }

    #[test]
    fn comparison_signals_true_or_false_and_retypes() {
        let mut b = Bench::new();
        let cmp = b.add(Kind::Comparison);
        b.input(cmp, 1, "5");
        b.input(cmp, 2, "5");
        assert_eq!(*b.out(cmp, 2), Value::Num(1.0), "result on any input");
        assert_eq!(*b.out(cmp, 0), Value::Null, "no True without Compare");
        b.input(cmp, 3, "");
        assert!(b.out(cmp, 0).truthy());
        b.input(cmp, 0, ">");
        assert_eq!(b.circuit.nodes[cmp].inputs[1].ty, Ty::Number);
        assert_eq!(b.circuit.nodes[cmp].inputs[1].value, Value::Null);
        b.input(cmp, 1, "3");
        b.input(cmp, 2, "1");
        b.input(cmp, 3, "");
        assert_eq!(*b.out(cmp, 2), Value::Num(1.0));
        assert!(!logic("XOR", &[Value::Num(1.0), Value::Num(1.0)]));
        assert!(logic("XOR", &[Value::Num(1.0), Value::Null]));
        assert!(!logic("AND", &[Value::Num(1.0), Value::Null]));
        assert!(contains(
            &Value::Str("LO".into()),
            &Value::Str("hello".into())
        ));
    }

    #[test]
    fn variable_ports_and_lists() {
        let mut b = Bench::new();
        let list = b.add(Kind::ListLiteral);
        assert!(b.circuit.perform_action(list, "add"));
        assert_eq!(b.circuit.nodes[list].inputs[2].name, "Index 2");
        assert_eq!(b.circuit.nodes[list].trigger_in, Some(3));
        b.input(list, 0, "number");
        assert_eq!(
            b.circuit.nodes[list].outputs[0].ty,
            Ty::List(Box::new(Ty::Number))
        );
        b.input(list, 1, "4");
        b.input(list, 2, "9");
        b.input(list, 3, "");
        let index = b.add(Kind::Index);
        b.input(index, 0, "number");
        assert!(b.connect((index, 2), (list, 0)));
        b.input(index, 1, "2");
        b.input(index, 3, "");
        assert_eq!(*b.out(index, 0), Value::Num(9.0));
        assert!(b.circuit.perform_action(list, "remove"));
        assert!(!b.circuit.perform_action(list, "remove"), "minimum 1");
    }

    #[test]
    fn clock_and_delay_follow_time() {
        let mut b = Bench::new();
        let clock = b.add(Kind::Clock);
        let delay = b.add(Kind::Delay);
        assert!(b.connect((delay, 1), (clock, 0)));
        b.input(delay, 0, "2");
        b.input(clock, 0, "1");
        assert!(b.circuit.busy());
        for _ in 0..8 {
            b.step();
        }
        assert_eq!(*b.out(clock, 0), Value::Null, "not before 0.9 s");
        for _ in 0..2 {
            b.step();
        }
        assert!(b.out(clock, 0).truthy(), "fired after 0.9 s");
        // A running clock would re-arm the delay every 0.9 s (`TIMER_OVERRIDE`).
        b.input(clock, 0, "0");
        assert_eq!(b.circuit.nodes[clock].state.clock_next, None);
        b.step();
        assert_eq!(*b.out(delay, 0), Value::Null, "delayed by 2 s");
        for _ in 0..25 {
            b.step();
        }
        assert!(b.out(delay, 0).truthy());
        assert!(!b.circuit.busy());
    }

    #[test]
    fn speech_radio_and_shell_parts() {
        let mut b = Bench::new();
        let speech = b.add(Kind::Speech);
        let radio = b.add(Kind::Radio);
        b.input(speech, 0, "hello");
        b.input(speech, 2, "");
        b.input(speech, 2, "");
        assert_eq!(
            b.effects,
            vec![Effect::Say {
                text: "hello".into(),
                quiet: false
            }],
            "one line per cooldown"
        );
        b.effects.clear();
        b.input(radio, 3, "");
        let id = b.circuit.nodes[radio].id;
        assert_eq!(
            b.effects,
            vec![Effect::Radio {
                node: id,
                frequency: 1457,
                code: 30
            }]
        );
        b.circuit.radio_receive(None, 1457, 30, 11.0);
        assert!(b.out(radio, 0).truthy(), "listens on its frequency");

        let mut b = Bench::new();
        let not = b.add(Kind::Not);
        b.circuit.on = false;
        b.with(|c, ctx| c.attach_shell(&[Kind::CompactRemote], ctx));
        assert!(b.circuit.on);
        assert_eq!(b.circuit.size, 1, "shell parts take no space");
        let remote = b.circuit.nodes.len() - 1;
        assert!(b.connect((not, 1), (remote, 1)));
        assert!(
            b.circuit
                .shell_event(Kind::CompactRemote, 1, Entity::PLACEHOLDER, 12.0)
        );
        b.step();
        assert_eq!(*b.out(not, 0), Value::Num(1.0));
        b.circuit.detach_shell();
        assert!(!b.circuit.on);
        assert_eq!(b.circuit.nodes.len(), 1);
        assert!(b.circuit.nodes[not].inputs[1].links.is_empty(), "wires go");
    }

    #[test]
    fn wire_bundle_ports_follow_the_shell_wires() {
        let mut b = Bench::new();
        let not = b.add(Kind::Not);
        let bundle = b.add(Kind::WireBundle);
        assert!(b.circuit.has_bundle());
        assert!(
            b.circuit.nodes[bundle].inputs.is_empty(),
            "no ports before a shell"
        );
        b.circuit
            .set_bundle_ports(&["Port 1".to_owned(), "Port 2".to_owned()]);
        let node = &b.circuit.nodes[bundle];
        assert_eq!(node.inputs[1].name, "Pulse Port 2");
        assert_eq!(node.outputs[0].name, "Port 1 Pulsed");
        // An input pulses the assembly on that wire.
        b.input(bundle, 1, "");
        assert_eq!(b.effects, vec![Effect::BundleActivate { port: 1 }]);
        // A pulsed wire signals its output.
        assert!(b.connect((not, 1), (bundle, 0)));
        b.circuit.bundle_pulsed(0, 20.0);
        b.step();
        assert_eq!(*b.out(not, 0), Value::Num(1.0));
        // The shell gone, the ports and their wires go.
        b.circuit.clear_bundle_ports();
        assert!(b.circuit.nodes[bundle].outputs.is_empty());
        assert!(b.circuit.nodes[not].inputs[1].links.is_empty());
    }

    #[test]
    fn conversions_like_dm() {
        assert_eq!(text2num("12abc"), Some(12.0));
        assert_eq!(text2num(" -1.5e2x"), Some(-150.0));
        assert_eq!(text2num("abc"), None);
        assert_eq!(
            convert(&Ty::String, Value::Entity(Entity::PLACEHOLDER), &[]),
            Value::Str("entity".into())
        );
        let options = vec!["a".to_owned(), "b".to_owned()];
        assert_eq!(
            convert(&Ty::Option, Value::Str("zzz".into()), &options),
            Value::Str("a".into())
        );
        assert_eq!(
            convert(&Ty::List(Box::new(Ty::Number)), Value::Null, &[]),
            Value::List(Vec::new())
        );
    }
}
