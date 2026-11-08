//! Integrated circuits (tg `code/modules/wiremod/`): the circuit board, its components with
//! typed ports wired together, the shells that carry it (compact remote, controller, bot) and
//! the component printer (`crate::science::lathe`, with the `Fabricator` window).
//!
//! The window data (tgui `IntegratedCircuit`) and the port datatypes live here, shared by both
//! halves; [`circuit`] is the simulation, [`server`] its world side, [`client`] the window.

#[cfg(feature = "server")]
pub mod circuit;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod devices;
#[cfg(feature = "server")]
pub mod server;

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// A port datatype (tg `/datum/circuit_datatype`, `PORT_TYPE_*`).
// shortcut: no table, datum or associative list types (their components are not ported).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ty {
    Any,
    String,
    Number,
    Signal,
    Boolean,
    Option,
    Entity,
    User,
    /// `PORT_TYPE_LIST(inner)`.
    List(Box<Ty>),
}

impl Ty {
    /// The `PORT_TYPE_*` key; a list is `"<inner> list"` (`generate_name`).
    pub fn name(&self) -> String {
        match self {
            Ty::Any => "any".into(),
            Ty::String => "string".into(),
            Ty::Number => "number".into(),
            Ty::Signal => "signal".into(),
            Ty::Boolean => "boolean".into(),
            Ty::Option => "option".into(),
            Ty::Entity => "entity".into(),
            Ty::User => "user".into(),
            Ty::List(inner) => format!("{} list", inner.name()),
        }
    }

    pub fn parse(name: &str) -> Option<Ty> {
        if let Some(inner) = name.strip_suffix(" list") {
            return Some(Ty::List(Box::new(Ty::parse(inner)?)));
        }
        Some(match name {
            "any" => Ty::Any,
            "string" => Ty::String,
            "number" => Ty::Number,
            "signal" => Ty::Signal,
            "boolean" => Ty::Boolean,
            "option" => Ty::Option,
            "entity" => Ty::Entity,
            "user" => Ty::User,
            _ => return None,
        })
    }

    /// The port color in the window.
    pub fn color(&self) -> &'static str {
        match self {
            Ty::Any => "blue",
            Ty::String => "orange",
            Ty::Number => "green",
            Ty::Signal => "teal",
            Ty::Boolean => "bad",
            Ty::Option => "violet",
            Ty::Entity => "purple",
            Ty::User => "label",
            Ty::List(_) => "white",
        }
    }

    /// `can_receive_from_datatype`: an input of this type accepts a wire from `from`.
    pub fn can_receive_from(&self, from: &Ty) -> bool {
        if self == from {
            return true;
        }
        match self {
            Ty::Any | Ty::String | Ty::Boolean => true,
            Ty::Number => matches!(from, Ty::Boolean),
            Ty::Signal => matches!(from, Ty::Number | Ty::Boolean),
            Ty::Option => matches!(from, Ty::String),
            Ty::Entity => matches!(from, Ty::User),
            Ty::User => false,
            Ty::List(inner) => matches!(from, Ty::List(f) if inner.can_receive_from(f)),
        }
    }

    /// `DATATYPE_FLAG_ALLOW_MANUAL_INPUT`.
    // shortcut: entities are never typed in (tg takes them from the circuit marker multitool,
    // not ported).
    pub fn manual_input(&self) -> bool {
        !matches!(self, Ty::Entity | Ty::User | Ty::List(_))
    }
}

/// `GLOB.wiremod_basic_types` (without `table`).
pub const BASIC_TYPES: [&str; 5] = ["any", "entity", "number", "signal", "string"];

/// tgui `IntegratedCircuit` (`tgui/packages/tgui/interfaces/IntegratedCircuit/index.jsx`).
pub struct IntegratedCircuitUi;

impl UiInterface for IntegratedCircuitUi {
    const NAME: &'static str = "IntegratedCircuit";
    type Data = CircuitData;
    type StaticData = ();
}

/// `/obj/item/integrated_circuit/ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct CircuitData {
    pub components: Vec<ComponentData>,
    pub display_name: String,
    /// The cell's charge in percent (`examine`'s charge meter); `None` without a cell.
    pub charge: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ComponentData {
    pub name: String,
    pub category: String,
    pub color: String,
    pub x: i32,
    pub y: i32,
    pub removable: bool,
    pub input_ports: Vec<InputPortData>,
    pub output_ports: Vec<OutputPortData>,
    /// `ui_buttons`: icon -> action name.
    pub ui_buttons: Vec<(String, String)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct InputPortData {
    pub name: String,
    /// [`Ty::name`].
    pub datatype: String,
    pub color: String,
    /// `REF(port)`: `i<component>.<port>`.
    pub port_ref: String,
    /// Refs of the output ports wired into it.
    pub connected_to: Vec<String>,
    /// The value as text; `None` for null and entities (tg hides atoms).
    pub current_data: Option<String>,
    /// An option port's options (`datatype_ui_data`).
    pub options: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct OutputPortData {
    pub name: String,
    pub datatype: String,
    pub color: String,
    /// `o<component>.<port>`.
    pub port_ref: String,
}

/// Act names and params, as the tgui interface sends them. Component and port ids are 1-based
/// positions (tg `WITHIN_RANGE`).
pub mod acts {
    pub const ADD_CONNECTION: &str = "add_connection";
    pub const REMOVE_CONNECTION: &str = "remove_connection";
    pub const DETACH_COMPONENT: &str = "detach_component";
    pub const SET_COORDINATES: &str = "set_component_coordinates";
    pub const SET_INPUT: &str = "set_component_input";
    pub const GET_VALUE: &str = "get_component_value";
    pub const SET_DISPLAY_NAME: &str = "set_display_name";
    pub const PERFORM_ACTION: &str = "perform_action";

    pub const INPUT_COMPONENT_ID: &str = "input_component_id";
    pub const OUTPUT_COMPONENT_ID: &str = "output_component_id";
    pub const INPUT_PORT_ID: &str = "input_port_id";
    pub const OUTPUT_PORT_ID: &str = "output_port_id";
    /// Of [`ADD_CONNECTION`] instead of the two output ids: `"<component>.<port> ..."`, what the
    /// simplified window's output dropdown sends.
    pub const OUTPUT: &str = "output";
    pub const COMPONENT_ID: &str = "component_id";
    pub const PORT_ID: &str = "port_id";
    pub const IS_INPUT: &str = "is_input";
    pub const INPUT: &str = "input";
    pub const SET_NULL: &str = "set_null";
    pub const REL_X: &str = "rel_x";
    pub const REL_Y: &str = "rel_y";
    pub const DISPLAY_NAME: &str = "display_name";
    pub const ACTION_NAME: &str = "action_name";
}

/// The `"<component>.<port>"` prefix of an output dropdown entry.
pub fn output_label(
    component: usize,
    port: usize,
    component_name: &str,
    port_name: &str,
) -> String {
    format!("{component}.{port} {component_name}: {port_name}")
}

/// Parses [`output_label`]'s prefix back into 1-based ids.
pub fn parse_output_label(label: &str) -> Option<(usize, usize)> {
    let head = label.split_whitespace().next()?;
    let (c, p) = head.split_once('.')?;
    Some((c.parse().ok()?, p.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datatypes_follow_tg_compatibility() {
        assert!(Ty::Number.can_receive_from(&Ty::Boolean));
        assert!(!Ty::Number.can_receive_from(&Ty::String));
        assert!(Ty::String.can_receive_from(&Ty::Entity));
        assert!(Ty::Signal.can_receive_from(&Ty::Number));
        assert!(!Ty::Signal.can_receive_from(&Ty::String));
        assert!(Ty::Entity.can_receive_from(&Ty::User));
        assert!(!Ty::User.can_receive_from(&Ty::Entity));
        let numbers = Ty::List(Box::new(Ty::Number));
        assert!(numbers.can_receive_from(&Ty::List(Box::new(Ty::Boolean))));
        assert!(!numbers.can_receive_from(&Ty::Number));
        assert_eq!(numbers.name(), "number list");
        assert_eq!(Ty::parse("number list"), Some(numbers));
    }

    #[test]
    fn output_labels_round_trip() {
        let label = output_label(3, 2, "Arithmetic", "Output");
        assert_eq!(parse_output_label(&label), Some((3, 2)));
        assert_eq!(parse_output_label("x"), None);
    }
}
