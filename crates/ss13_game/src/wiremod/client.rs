//! The integrated circuit window (client), after tgui `IntegratedCircuit/index.jsx`.
// shortcut: a list instead of the draggable canvas with drawn wires: every component is a
// section of its ports; an input wires to an output through a dropdown of the compatible
// outputs (`add_connection` with `output`), typed values save on Enter, option ports are
// dropdowns, signal ports a button. No variables panel, no component menu, no examine panel.

use bevy::scene::{EntityScene, SceneList};
use ss13_tgui::client::{ActOnSubmit, AppTguiClientExt, ClientUi};
use ss13_ui_kit::ActParams;
use ss13_ui_kit::prelude::*;

use super::*;

pub struct WiremodClientPlugin;

impl Plugin for WiremodClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<IntegratedCircuitUi>();
    }
}

type Part = Box<dyn SceneList>;

fn params(pairs: &[(&str, String)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.clone()))
        .collect()
}

fn button(
    icon: &'static str,
    content: String,
    action: &str,
    params: Vec<(String, String)>,
) -> Part {
    let action = action.to_owned();
    let params = ActParams(params);
    Box::new(EntityScene(bsn! {
        @Button { @icon: icon, @content: content }
        Act(action)
        params
    }))
}

fn label(text: String) -> Part {
    Box::new(EntityScene(bsn! {
        Node { margin: UiRect::right(px(4)) }
        Children [ Text(text) ]
    }))
}

fn row(children: Vec<Part>) -> Part {
    Box::new(EntityScene(bsn! {
        Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, align_items: AlignItems::Center, margin: UiRect::bottom(px(2)) }
        Children [ {children} ]
    }))
}

fn dropdown(
    options: Vec<String>,
    selected: String,
    placeholder: String,
    action: &str,
    value_param: String,
    params: Vec<(String, String)>,
) -> Part {
    let action = action.to_owned();
    let params = ActParams(params);
    Box::new(EntityScene(bsn! {
        @Dropdown { @value_param: value_param, @width: 12.0, @placeholder: placeholder }
        Dropdown { options: options, selected: selected }
        Act(action)
        params
    }))
}

fn text_input(value: String, params: Vec<(String, String)>) -> Part {
    let action = acts::SET_INPUT.to_owned();
    let param = acts::INPUT.to_owned();
    Box::new(EntityScene(bsn! {
        Input { value: value, max_length: {Some(256)} }
        ActOnSubmit { action: action, param: param, params: params }
    }))
}

/// Every output port: its ref, its dropdown label and its type.
struct Output {
    port_ref: String,
    label: String,
    ty: Option<Ty>,
}

fn outputs(data: &CircuitData) -> Vec<Output> {
    data.components
        .iter()
        .enumerate()
        .flat_map(|(c, comp)| {
            comp.output_ports
                .iter()
                .enumerate()
                .map(move |(p, port)| Output {
                    port_ref: port.port_ref.clone(),
                    label: output_label(c + 1, p + 1, &comp.name, &port.name),
                    ty: Ty::parse(&port.datatype),
                })
        })
        .collect()
}

/// An input port: its value, its editor, its wiring.
fn input_row(c: usize, p: usize, port: &InputPortData, all: &[Output]) -> Part {
    let ids = [
        (acts::COMPONENT_ID, c.to_string()),
        (acts::PORT_ID, p.to_string()),
    ];
    let wired: Vec<String> = port
        .connected_to
        .iter()
        .filter_map(|r| all.iter().find(|o| &o.port_ref == r))
        .map(|o| o.label.clone())
        .collect();
    let value = port
        .current_data
        .clone()
        .unwrap_or_else(|| "null".to_owned());
    let mut text = format!("{} [{}]: {value}", port.name, port.datatype);
    if !wired.is_empty() {
        text.push_str(&format!(" <- {}", wired.join(", ")));
    }
    let mut parts = vec![label(text)];
    let ty = Ty::parse(&port.datatype);
    match &ty {
        Some(Ty::Option) => parts.push(dropdown(
            port.options.clone(),
            port.current_data.clone().unwrap_or_default(),
            "Select...".to_owned(),
            acts::SET_INPUT,
            acts::INPUT.to_owned(),
            params(&ids),
        )),
        Some(Ty::Signal) => parts.push(button(
            "bolt",
            "Trigger".to_owned(),
            acts::SET_INPUT,
            params(&[ids[0].clone(), ids[1].clone(), (acts::INPUT, String::new())]),
        )),
        Some(Ty::Boolean) => {
            let on = port.current_data.as_deref().is_some_and(|v| v != "0");
            parts.push(button(
                if on { "check-square-o" } else { "square-o" },
                if on { "On" } else { "Off" }.to_owned(),
                acts::SET_INPUT,
                params(&[
                    ids[0].clone(),
                    ids[1].clone(),
                    (acts::INPUT, if on { "0" } else { "1" }.to_owned()),
                ]),
            ));
        }
        Some(t) if t.manual_input() => parts.push(text_input(
            port.current_data.clone().unwrap_or_default(),
            params(&ids),
        )),
        _ => {}
    }
    let compatible: Vec<String> = all
        .iter()
        .filter(|o| {
            ty.as_ref()
                .zip(o.ty.as_ref())
                .is_some_and(|(i, out)| i.can_receive_from(out))
                && !port.connected_to.contains(&o.port_ref)
        })
        .map(|o| o.label.clone())
        .collect();
    if !compatible.is_empty() {
        parts.push(dropdown(
            compatible,
            String::new(),
            "Connect...".to_owned(),
            acts::ADD_CONNECTION,
            acts::OUTPUT.to_owned(),
            params(&[
                (acts::INPUT_COMPONENT_ID, c.to_string()),
                (acts::INPUT_PORT_ID, p.to_string()),
            ]),
        ));
    }
    if !wired.is_empty() {
        parts.push(button(
            "unlink",
            String::new(),
            acts::REMOVE_CONNECTION,
            params(&[
                ids[0].clone(),
                ids[1].clone(),
                (acts::IS_INPUT, "1".to_owned()),
            ]),
        ));
    }
    row(parts)
}

fn output_row(c: usize, p: usize, port: &OutputPortData) -> Part {
    let ids = [
        (acts::COMPONENT_ID, c.to_string()),
        (acts::PORT_ID, p.to_string()),
    ];
    row(vec![
        label(format!("-> {} [{}]", port.name, port.datatype)),
        button("question", String::new(), acts::GET_VALUE, params(&ids)),
        button(
            "unlink",
            String::new(),
            acts::REMOVE_CONNECTION,
            params(&[
                ids[0].clone(),
                ids[1].clone(),
                (acts::IS_INPUT, "0".to_owned()),
            ]),
        ),
    ])
}

fn component(c: usize, comp: &ComponentData, all: &[Output]) -> Part {
    let id = [(acts::COMPONENT_ID, c.to_string())];
    let mut buttons: Vec<Part> = comp
        .ui_buttons
        .iter()
        .map(|(icon, action)| {
            let icon: &'static str = if icon == "plus" { "plus" } else { "minus" };
            button(
                icon,
                String::new(),
                acts::PERFORM_ACTION,
                params(&[id[0].clone(), (acts::ACTION_NAME, action.clone())]),
            )
        })
        .collect();
    if comp.removable {
        buttons.push(button(
            "eject",
            "Eject".to_owned(),
            acts::DETACH_COMPONENT,
            params(&id),
        ));
    }
    let mut rows = vec![row(buttons)];
    rows.extend(
        comp.input_ports
            .iter()
            .enumerate()
            .map(|(p, port)| input_row(c, p + 1, port, all)),
    );
    rows.extend(
        comp.output_ports
            .iter()
            .enumerate()
            .map(|(p, port)| output_row(c, p + 1, port)),
    );
    let title = format!("{c}. {} ({})", comp.name, comp.category);
    Box::new(EntityScene(bsn! {
        @Section { @title: title }
        Children [ {rows} ]
    }))
}

impl ClientUi for IntegratedCircuitUi {
    const WIDTH: f32 = 640.0;
    const HEIGHT: f32 = 640.0;

    fn content(_: &(), data: &CircuitData) -> impl Scene {
        let all = outputs(data);
        let charge = data
            .charge
            .map_or_else(|| "No cell".to_owned(), |c| format!("{c}%"));
        let name_input: Vec<Part> = {
            let action = acts::SET_DISPLAY_NAME.to_owned();
            let param = acts::DISPLAY_NAME.to_owned();
            let value = data.display_name.clone();
            vec![Box::new(EntityScene(bsn! {
                Input { value: value, placeholder: {"Label".to_owned()}, max_length: {Some(23)} }
                ActOnSubmit { action: action, param: param, params: {Vec::new()} }
            }))]
        };
        let header = vec![row(name_input), label(format!("Charge: {charge}"))];
        let mut components: Vec<Part> = data
            .components
            .iter()
            .enumerate()
            .map(|(c, comp)| component(c + 1, comp, &all))
            .collect();
        if components.is_empty() {
            components.push(label("No components.".to_owned()));
        }
        bsn! {
            @WindowContent
            Children [
                @Section { @title: "Circuit" }
                Children [ {header} ]
                --
                @Section { @title: "Components", @scrollable: true }
                Children [ {components} ]
            ]
        }
    }
}
