//! The chemistry machines' windows (client): tgui `SmokeMachine.tsx`, `ChemSeparator.tsx`,
//! `MassSpec.tsx`, `ChemDebugSynthesizer.tsx`, `ChemRecipeDebug.tsx` and `Pandemic/`. The data
//! is in [`super`], the server halves are its submodules.
// shortcut: the separator's knob is a number input, the mass spectrometer has no absorbance
// graph (its range is set with three number inputs), the PANDEMIC's virus tabs are one
// section per virus, the tester's dropdowns are button rows.

use bevy::scene::EntityScene;
use ss13_tgui::client::{ActOnChange, AppTguiClientExt, ClientUi};
use ss13_tgui::interfaces::chem_dispenser::js_num;
use ss13_ui_kit::prelude::*;

use super::*;

pub struct ChemMachinesClientPlugin;

impl Plugin for ChemMachinesClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<SmokeMachineUi>()
            .register_ui_view::<ChemSeparatorUi>()
            .register_ui_view::<MassSpecUi>()
            .register_ui_view::<ChemSynthesizerUi>()
            .register_ui_view::<RecipeDebugUi>()
            .register_ui_view::<PandemicUi>();
    }
}

fn label(text: String) -> impl Scene + use<> {
    bsn! { Text(text) TextColor({UiColor::Label.base()}) }
}

fn button(
    icon: &'static str,
    text: String,
    action: &'static str,
    disabled: bool,
    selected: bool,
) -> impl Scene + use<> {
    bsn! {
        @Button { @icon: icon, @content: text }
        Button { disabled: disabled, selected: selected }
        Act(action)
    }
}

fn button_with(
    icon: &'static str,
    text: String,
    action: &'static str,
    params: Vec<(&'static str, String)>,
    disabled: bool,
    selected: bool,
) -> impl Scene + use<> {
    let params = ActParams(params.into_iter().map(|(k, v)| (k.to_owned(), v)).collect());
    bsn! {
        @Button { @icon: icon, @content: text }
        Button { disabled: disabled, selected: selected }
        Act(action)
        params
    }
}

fn buttons(scene: impl Scene + 'static) -> Box<dyn SceneList> {
    Box::new(EntityScene(scene))
}

fn number(
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    unit: &str,
    action: &str,
    param: &str,
) -> impl Scene + use<> {
    let input = NumberInput {
        value: f64::from(value),
        min_value: f64::from(min),
        max_value: f64::from(max),
        step: f64::from(step),
        step_pixel_size: 2.0,
        unit: unit.into(),
        ..Default::default()
    };
    let on_change = ActOnChange {
        action: action.into(),
        param: param.into(),
    };
    bsn! { Node { width: px(80) } input on_change }
}

fn bar(value: f32, max: f32, text: String, color: UiColor) -> impl Scene + use<> {
    let bar = ProgressBar {
        value,
        min_value: 0.0,
        max_value: max.max(f32::EPSILON),
        color,
        ..Default::default()
    };
    bsn! {
        Node { width: px(170) }
        Children [ @ProgressBar { @empty: true } bar Children [ Text(text) ] ]
    }
}

fn contents(lines: &[ReagentLine]) -> Vec<Box<dyn Scene>> {
    if lines.is_empty() {
        return vec![Box::new(label("Nothing".to_owned()))];
    }
    lines
        .iter()
        .map(|c| {
            Box::new(label(format!("{} units of {}", js_num(c.volume), c.name))) as Box<dyn Scene>
        })
        .collect()
}

fn column(children: Vec<Box<dyn Scene>>) -> impl Scene + use<> {
    bsn! { Node { flex_direction: FlexDirection::Column } Children [ {children} ] }
}

fn row(children: Vec<Box<dyn Scene>>) -> impl Scene + use<> {
    bsn! {
        Node { flex_direction: FlexDirection::Row, flex_wrap: FlexWrap::Wrap, column_gap: px(4) }
        Children [ {children} ]
    }
}

fn item(text: &'static str, value: impl Scene + 'static) -> impl Scene {
    bsn! { @LabeledListItem { @label: text } Children [ {EntityScene(value)} ] }
}

impl ClientUi for SmokeMachineUi {
    const WIDTH: f32 = 350.0;
    const HEIGHT: f32 = 350.0;

    fn content(_: &(), d: &SmokeMachineData) -> impl Scene {
        let (icon, text) = if d.active {
            ("power-off", "On")
        } else {
            ("times", "Off")
        };
        let power = buttons(button(icon, text.to_owned(), "power", false, d.active));
        let purge = buttons(button("trash", "Purge".to_owned(), "purge", false, false));
        let tank = ProgressBar {
            value: if d.max_volume > 0.0 {
                d.current_volume / d.max_volume
            } else {
                0.0
            },
            ranges: vec![(UiColor::Bad, f32::NEG_INFINITY, 0.3)],
            ..Default::default()
        };
        let level = format!(
            "{} / {}",
            js_num(d.current_volume.round()),
            js_num(d.max_volume)
        );
        let ranges: Vec<Box<dyn Scene>> = (1..=5u32)
            .map(|n| {
                Box::new(button_with(
                    "plus",
                    (n * 3).to_string(),
                    "setting",
                    vec![("amount", n.to_string())],
                    d.max_setting < n,
                    d.setting == n,
                )) as Box<dyn Scene>
            })
            .collect();
        let range = EntityScene(row(ranges));
        let lines = EntityScene(column(contents(&d.contents)));
        bsn! {
            @WindowContent
            Children [
                @Section { @title: "Dispersal Tank", @buttons: {Some(power)} }
                Children [
                    @ProgressBar { @empty: true } tank Children [ Text(level) ]
                    --
                    @LabeledList
                    Children [ @LabeledListItem { @label: "Range" } Children [ {range} ] ]
                ]
                --
                @Section { @title: "Contents", @buttons: {Some(purge)} }
                Children [ {lines} ]
            ]
        }
    }
}

fn holder_bar(h: &HolderBar) -> impl Scene + use<> {
    let text = format!(
        "{} of {} units at {}K",
        js_num(h.total_volume.ceil()),
        js_num(h.maximum_volume),
        js_num(h.temp.ceil())
    );
    let [r, g, b] = h.color;
    bar(
        h.total_volume,
        h.maximum_volume,
        text,
        UiColor::Css([r, g, b]),
    )
}

impl ClientUi for ChemSeparatorUi {
    const WIDTH: f32 = 370.0;
    const HEIGHT: f32 = 215.0;

    fn content(st: &SeparatorStatic, d: &SeparatorData) -> impl Scene {
        let mut items: Vec<Box<dyn Scene>> = Vec::new();
        let drain = button(
            "arrow-down",
            "Drain".to_owned(),
            "drain",
            d.flask.total_volume <= 0.0 || d.beaker.is_none(),
            false,
        );
        items.push(Box::new(item(
            "Flask",
            row(vec![Box::new(holder_bar(&d.flask)), Box::new(drain)]),
        )));
        if let Some(b) = &d.beaker {
            let filter = button(
                "filter",
                "Filter".to_owned(),
                "filter",
                b.total_volume <= 0.0,
                false,
            );
            items.push(Box::new(item(
                "Beaker",
                row(vec![Box::new(holder_bar(b)), Box::new(filter)]),
            )));
        }
        items.push(Box::new(item(
            "Burner Knob",
            number(
                d.knob as f32,
                1.0,
                st.max_burner_knob_settings as f32,
                1.0,
                "",
                "knob",
                "amount",
            ),
        )));
        if let Some(f) = &d.fuel {
            items.push(Box::new(item("Fuel Source", holder_bar(f))));
        }
        let fuel = d.fuel.as_ref().is_some_and(|f| f.total_volume > 0.0);
        let any =
            d.flask.total_volume > 0.0 || d.beaker.as_ref().is_some_and(|b| b.total_volume > 0.0);
        if st.condenser_installed && any && fuel {
            items.push(Box::new(item(
                "Cooling",
                button(
                    "thermometer-full",
                    "Start".to_owned(),
                    "cool",
                    d.burner_on,
                    d.condenser_on,
                ),
            )));
        }
        bsn! {
            @WindowContent
            Children [ @LabeledList Children [ {items} ] ]
        }
    }
}

fn spec_beaker(
    title: &'static str,
    beaker: &Option<MassSpecBeaker>,
    eject: &'static str,
    insert: &'static str,
    in_hand: bool,
    processing: bool,
) -> impl Scene + use<> {
    let (head, body): (Box<dyn SceneList>, Box<dyn Scene>) = match beaker {
        Some(b) => {
            let volume = format!(
                "{} / {} units",
                js_num(b.current_volume),
                js_num(b.max_volume)
            );
            let head = buttons(row(vec![
                Box::new(label(volume)),
                Box::new(button(
                    "eject",
                    "Eject".to_owned(),
                    eject,
                    processing,
                    false,
                )),
            ]));
            let lines: Vec<Box<dyn Scene>> = if b.contents.is_empty() {
                vec![Box::new(label("Empty".to_owned()))]
            } else {
                b.contents
                    .iter()
                    .map(|r| {
                        let mut text = format!(
                            "{}u {} — mass {}, {}% ({})",
                            js_num(r.volume),
                            r.name,
                            js_num(r.mass),
                            js_num(r.purity),
                            r.kind
                        );
                        if !r.log.is_empty() {
                            text.push_str(&format!(" — {}", r.log));
                        }
                        Box::new(label(text)) as Box<dyn Scene>
                    })
                    .collect()
            };
            (head, Box::new(column(lines)))
        }
        None => {
            let tooltip = if in_hand {
                ""
            } else {
                "You need to hold a container in your hand"
            };
            let head = buttons(bsn! {
                @Button { @icon: "eject", @content: "Insert", @tooltip: tooltip }
                Button { disabled: processing }
                Act(insert)
            });
            (head, Box::new(label("No beaker loaded".to_owned())))
        }
    };
    bsn! {
        @Section { @title: title, @buttons: {Some(head)} }
        Children [ {EntityScene(body)} ]
    }
}

impl ClientUi for MassSpecUi {
    const WIDTH: f32 = 490.0;
    const HEIGHT: f32 = 650.0;

    fn content(_: &(), d: &MassSpecData) -> impl Scene {
        let status = if d.processing {
            format!("Purifying... {}s", js_num(d.eta.max(0.0).round()))
        } else {
            format!("Estimated time: {}s", js_num(d.eta.max(0.0).round()))
        };
        let start = buttons(button(
            "power-off",
            "Start".to_owned(),
            "activate",
            d.processing || d.beaker1.is_none() || d.beaker2.is_none(),
            d.processing,
        ));
        let center = (d.lower_range + d.upper_range) / 2.0;
        let top = d.graph_upper_range;
        let ranges = EntityScene(column(vec![
            Box::new(item(
                "Lower mass",
                number(d.lower_range, 0.0, top, 1.0, "", "leftSlider", "value"),
            )),
            Box::new(item(
                "Center",
                number(center, 0.0, top, 1.0, "", "centerSlider", "value"),
            )),
            Box::new(item(
                "Upper mass",
                number(d.upper_range, 0.0, top, 1.0, "", "rightSlider", "value"),
            )),
        ]));
        let input = EntityScene(spec_beaker(
            "Input",
            &d.beaker1,
            "eject1",
            "insert1",
            d.has_beaker_in_hand,
            d.processing,
        ));
        let output = EntityScene(spec_beaker(
            "Output",
            &d.beaker2,
            "eject2",
            "insert2",
            d.has_beaker_in_hand,
            d.processing,
        ));
        bsn! {
            @WindowContent { @scrollable: true }
            Children [
                @Section { @title: "Purity Analysis", @buttons: {Some(start)} }
                Children [ Text(status) -- @LabeledList Children [ {ranges} ] ]
                --
                {input}
                --
                {output}
            ]
        }
    }
}

fn beaker_view(b: &Option<BeakerView>) -> Box<dyn Scene> {
    match b {
        Some(b) => {
            let mut lines: Vec<Box<dyn Scene>> = vec![Box::new(label(format!(
                "{} / {} units, pH {}",
                js_num(b.current_volume),
                js_num(b.max_volume),
                js_num(b.ph)
            )))];
            lines.extend(contents(&b.contents));
            Box::new(column(lines))
        }
        None => Box::new(label("No beaker loaded".to_owned())),
    }
}

impl ClientUi for ChemSynthesizerUi {
    const WIDTH: f32 = 390.0;
    const HEIGHT: f32 = 330.0;

    fn content(_: &(), d: &SynthesizerData) -> impl Scene {
        let head: Box<dyn SceneList> = match &d.beaker {
            Some(b) => buttons(row(vec![
                Box::new(number(
                    d.amount,
                    1.0,
                    b.max_volume,
                    1.0,
                    "u",
                    "amount",
                    "amount",
                )),
                Box::new(number(d.temp, 0.0, 1000.0, 1.0, "K", "temp", "amount")),
                Box::new(number(d.purity, 0.0, 120.0, 1.0, "%", "purity", "amount")),
                Box::new(button("plus", "Input".to_owned(), "input", false, false)),
            ])),
            None => buttons(button(
                "plus",
                "Create Beaker".to_owned(),
                "makecup",
                false,
                false,
            )),
        };
        let body = beaker_view(&d.beaker);
        bsn! {
            @WindowContent { @scrollable: true }
            Children [
                @Section { @title: "Recipient", @buttons: {Some(head)} }
                Children [ {EntityScene(body)} ]
            ]
        }
    }
}

fn choices(list: &[&'static str], action: &'static str, current: usize) -> impl Scene + use<> {
    let all: Vec<Box<dyn Scene>> = list
        .iter()
        .enumerate()
        .map(|(i, name)| {
            Box::new(button_with(
                "",
                (*name).to_owned(),
                action,
                vec![("target", (*name).to_owned())],
                false,
                i == current,
            )) as Box<dyn Scene>
        })
        .collect();
    row(all)
}

fn toggle(on: bool, action: &'static str) -> impl Scene + use<> {
    let (icon, text) = if on {
        ("check-square-o", "On")
    } else {
        ("square-o", "Off")
    };
    button(icon, text.to_owned(), action, false, on)
}

impl ClientUi for RecipeDebugUi {
    const WIDTH: f32 = 450.0;
    const HEIGHT: f32 = 750.0;

    fn content(_: &(), d: &RecipeDebugData) -> impl Scene {
        let settings = EntityScene(column(vec![
            Box::new(item(
                "Temp mode",
                choices(&TEMP_MODES, "temp_mode", d.temp_mode as usize),
            )),
            Box::new(item(
                "Forced temp",
                number(
                    d.forced_temp,
                    0.0,
                    1000.0,
                    1.0,
                    "K",
                    "forced_temp",
                    "target",
                ),
            )),
            Box::new(item(
                "Forced pH",
                row(vec![
                    Box::new(number(
                        d.forced_ph,
                        0.0,
                        14.0,
                        0.1,
                        "",
                        "forced_ph",
                        "target",
                    )),
                    Box::new(toggle(d.use_forced_ph, "toggle_forced_ph")),
                ]),
            )),
            Box::new(item(
                "Forced purity",
                row(vec![
                    Box::new(number(
                        d.forced_purity,
                        0.0,
                        1.0,
                        0.01,
                        "",
                        "forced_purity",
                        "target",
                    )),
                    Box::new(toggle(d.use_forced_purity, "toggle_forced_purity")),
                ]),
            )),
            Box::new(item(
                "Volume multiplier",
                number(
                    d.volume_multiplier,
                    1.0,
                    100.0,
                    1.0,
                    "x",
                    "volume_multiplier",
                    "target",
                ),
            )),
        ]));
        let reaction = EntityScene(column(vec![
            Box::new(item(
                "Reactions",
                button("search", "Pick".to_owned(), "pick_reaction", false, false),
            )),
            Box::new(item(
                "Mode",
                choices(
                    &REACTION_MODES,
                    "reaction_mode",
                    d.current_reaction_mode as usize,
                ),
            )),
            Box::new(item(
                "Current",
                row(vec![
                    Box::new(label(d.current_reaction_name.clone())),
                    Box::new(button(
                        "play",
                        "Start".to_owned(),
                        "start_reaction",
                        false,
                        false,
                    )),
                ]),
            )),
        ]));
        let mut edit: Vec<Box<dyn Scene>> = vec![Box::new(item(
            "Reaction",
            button("pen", "Edit".to_owned(), "edit_reaction", false, false),
        ))];
        if let Some(e) = &d.edit_reaction {
            let current = EDIT_VARS.iter().position(|v| *v == e.edit_var).unwrap_or(0);
            edit.push(Box::new(item("Name", label(e.name.clone()))));
            edit.push(Box::new(item(
                "Var",
                choices(&EDIT_VARS, "edit_var", current),
            )));
            edit.push(Box::new(item(
                "Value",
                row(vec![
                    Box::new(number(
                        e.edit_value,
                        -100000.0,
                        100000.0,
                        0.1,
                        "",
                        "edit_value",
                        "target",
                    )),
                    Box::new(button(
                        "undo",
                        "Reset".to_owned(),
                        "reset_value",
                        false,
                        false,
                    )),
                    Box::new(button("save", "Export".to_owned(), "export", false, false)),
                ]),
            )));
        }
        let edit = EntityScene(column(edit));
        let reactions: Vec<Box<dyn Scene>> = if d.active_reactions.is_empty() {
            vec![Box::new(label("No active reactions.".to_owned()))]
        } else {
            d.active_reactions
                .iter()
                .map(|r| {
                    let text = format!(
                        "{}: {}% — {} / {}u{}",
                        r.name,
                        (r.quality * 100.0).round(),
                        js_num(r.reacted_vol),
                        js_num(r.target_vol),
                        if r.overheat { " (overheat)" } else { "" }
                    );
                    let color = if r.danger {
                        UiColor::Red
                    } else {
                        UiColor::Default
                    };
                    Box::new(bsn! { Text(text) TextColor({color.base()}) }) as Box<dyn Scene>
                })
                .collect()
        };
        let reacting = if d.is_reacting { "Reacting" } else { "Idle" };
        let reactions = EntityScene(column(reactions));
        let eject = buttons(button("eject", "Eject".to_owned(), "eject", false, false));
        let beaker: Box<dyn Scene> = match &d.beaker {
            Some(b) => {
                let mut lines: Vec<Box<dyn Scene>> = vec![Box::new(label(format!(
                    "{} / {} units, {}K, pH {}, purity {}",
                    js_num(b.current_volume),
                    js_num(b.max_volume),
                    js_num(b.current_temp),
                    js_num(b.ph),
                    js_num(b.purity)
                )))];
                lines.extend(contents(&b.contents));
                Box::new(column(lines))
            }
            None => Box::new(label("Empty".to_owned())),
        };
        bsn! {
            @WindowContent { @scrollable: true }
            Children [
                @Section { @title: "Settings" }
                Children [ @LabeledList Children [ {settings} ] ]
                --
                @Section { @title: "Reaction" }
                Children [ @LabeledList Children [ {reaction} ] ]
                --
                @Section { @title: "Edit" }
                Children [ @LabeledList Children [ {edit} ] ]
                --
                @Section { @title: reacting }
                Children [ {reactions} ]
                --
                @Section { @title: "Beaker", @buttons: {Some(eject)} }
                Children [ {EntityScene(beaker)} ]
            ]
        }
    }
}

fn virus_section(v: &VirusView, ready: bool) -> impl Scene + use<> {
    let culture = buttons(button_with(
        "flask",
        "Create Culture Bottle".to_owned(),
        "create_culture_bottle",
        vec![("index", v.index.to_string())],
        !ready,
        false,
    ));
    let mut lines: Vec<Box<dyn Scene>> = vec![
        Box::new(item("Agent", label(v.agent.clone()))),
        Box::new(item("Description", label(v.description.clone()))),
        Box::new(item("Spread", label(v.spread.clone()))),
        Box::new(item("Possible Cure", label(v.cure.clone()))),
    ];
    if v.is_adv {
        lines.push(Box::new(item(
            "Statistics",
            label(format!(
                "Resistance {}, Stealth {}, Stage speed {}, Transmissibility {}",
                v.resistance, v.stealth, v.stage_speed, v.transmission
            )),
        )));
        for s in &v.symptoms {
            let neutered = if s.neutered { " (neutered)" } else { "" };
            lines.push(Box::new(item(
                "Symptom",
                label(format!(
                    "{}{neutered}: level {}, res {}, stealth {}, speed {}, trans {}; cure: {}",
                    s.name, s.level, s.resistance, s.stealth, s.stage_speed, s.transmission, s.cure
                )),
            )));
        }
        if v.can_rename {
            let rename = ss13_tgui::client::ActOnSubmit {
                action: "rename_disease".into(),
                param: "name".into(),
                params: vec![("index".into(), v.index.to_string())],
            };
            let field = Input {
                placeholder: "Input a name".into(),
                max_length: Some(42),
                ..Default::default()
            };
            lines.push(Box::new(item(
                "Rename",
                bsn! { Node { width: px(200) } field rename },
            )));
        }
    }
    let title = v.name.clone();
    let lines = EntityScene(column(lines));
    bsn! {
        @Section { @title: title, @buttons: {Some(culture)} }
        Children [ @LabeledList Children [ {lines} ] ]
    }
}

impl ClientUi for PandemicUi {
    const WIDTH: f32 = 650.0;
    const HEIGHT: f32 = 500.0;

    fn content(_: &(), d: &PandemicData) -> impl Scene {
        let has_beaker = d.beaker.is_some();
        let head = buttons(row(vec![
            Box::new(button(
                "eject",
                "Empty and Eject".to_owned(),
                "empty_eject_beaker",
                !has_beaker,
                false,
            )),
            Box::new(button(
                "trash",
                "Empty".to_owned(),
                "empty_beaker",
                !has_beaker,
                false,
            )),
            Box::new(button(
                "eject",
                "Eject".to_owned(),
                "eject_beaker",
                !has_beaker,
                false,
            )),
        ]));
        let mut info: Vec<Box<dyn Scene>> = Vec::new();
        match d.beaker {
            None => info.push(Box::new(label("No beaker loaded.".to_owned()))),
            Some((volume, capacity)) => {
                info.push(Box::new(item(
                    "Volume",
                    label(format!("{} / {}", js_num(volume), js_num(capacity))),
                )));
                match &d.blood {
                    None => info.push(Box::new(label("No blood sample detected.".to_owned()))),
                    Some((dna, kind)) => {
                        info.push(Box::new(item("Blood DNA", label(dna.clone()))));
                        info.push(Box::new(item("Blood Type", label(kind.clone()))));
                    }
                }
            }
        }
        let antibodies: Vec<Box<dyn Scene>> = d
            .resistances
            .iter()
            .map(|r| {
                Box::new(button_with(
                    "eye-dropper",
                    r.name.clone(),
                    "create_vaccine_bottle",
                    vec![("index", r.id.clone())],
                    !d.is_ready,
                    false,
                )) as Box<dyn Scene>
            })
            .collect();
        if !antibodies.is_empty() {
            info.push(Box::new(item("Antibodies", row(antibodies))));
        }
        let info = EntityScene(column(info));
        let viruses: Vec<Box<dyn Scene>> = if d.blood.is_some() {
            d.viruses
                .iter()
                .map(|v| Box::new(virus_section(v, d.is_ready)) as Box<dyn Scene>)
                .collect()
        } else {
            Vec::new()
        };
        bsn! {
            @WindowContent { @scrollable: true }
            Children [
                @Section { @title: "Beaker", @buttons: {Some(head)} }
                Children [ @LabeledList Children [ {info} ] ]
                --
                {viruses}
            ]
        }
    }
}
