//! `DnaConsole`: `tgui/packages/tgui/interfaces/DnaConsole/` and
//! `/obj/machinery/computer/dna_console/ui_data` (`code/game/machinery/computer/dna_console.dm`).
//! The server half lives with the game (`ss13_game::genetics`); this module has the shared data
//! and the client view.
// shortcut: the genetic sequencer and the scanner status only: no storage, disks, chromosomes,
// advanced injectors, genetic makeup or enzyme pulses. Port the other tabs with those features.

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct DnaConsoleUi;

/// One block of the occupant's `mutation_index` (`tgui_occupant_mutations`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DnaMutation {
    /// `alias` ("Mutation 5"): the act key.
    pub alias: String,
    /// The real name once discovered, else the alias.
    pub name: String,
    pub description: String,
    pub discovered: bool,
    /// `Active`: the mutation is on the occupant.
    pub active: bool,
    /// `Sequence`: the occupant's gene string, `X` for a missing base.
    pub sequence: String,
    pub instability: i32,
    /// `MUTATION_QUALITY`: 1 positive, 2 negative, 4 minor negative.
    pub quality: u8,
    /// `Class`: 1 activated, 2 mutator, 0 not active.
    pub class: u8,
    /// Not in the sequence (added by a mutator or radiation).
    pub extra: bool,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DnaConsoleData {
    pub is_scanner_connected: bool,
    pub is_open: bool,
    pub is_viable_subject: bool,
    pub subject_name: String,
    /// `SubjectStatus`: 0 conscious, 1 unconscious, 2 dead.
    pub subject_status: u8,
    pub subject_health: f32,
    pub subject_enzymes: String,
    pub subject_unique_identity: String,
    pub subject_stability: i32,
    /// Seconds until the injector printer is ready (0: ready).
    pub injector_seconds: f32,
    pub mutations: Vec<DnaMutation>,
}

impl UiInterface for DnaConsoleUi {
    const NAME: &'static str = "DnaConsole";
    type Data = DnaConsoleData;
    type StaticData = ();
}

/// `GeneticSequencer` status text.
pub fn status_text(status: u8) -> &'static str {
    match status {
        0 => "Conscious",
        1 => "Unconscious",
        _ => "Dead",
    }
}

#[cfg(feature = "client")]
mod client_side {
    use ss13_ui_kit::prelude::*;

    use super::{DnaConsoleData, DnaConsoleUi, DnaMutation, status_text};
    use crate::client::ClientUi;

    impl ClientUi for DnaConsoleUi {
        const WIDTH: f32 = 560.0;
        const HEIGHT: f32 = 640.0;

        fn content(_: &(), data: &DnaConsoleData) -> impl Scene {
            let door_text = if data.is_open {
                "Close Scanner"
            } else {
                "Open Scanner"
            };
            let subject = if data.is_viable_subject {
                data.subject_name.clone()
            } else {
                "No viable subject".to_owned()
            };
            let status = status_text(data.subject_status);
            let health = format!("{:.0}%", data.subject_health);
            let stability = format!("{}", data.subject_stability);
            let stability_color = match data.subject_stability {
                65.. => UiColor::Good,
                20..65 => UiColor::Average,
                _ => UiColor::Bad,
            };
            let enzymes = data.subject_enzymes.clone();
            let identity = data.subject_unique_identity.clone();
            let injector = if data.injector_seconds > 0.0 {
                format!("{:.0} s", data.injector_seconds)
            } else {
                "Ready".to_owned()
            };
            let no_scanner = !data.is_scanner_connected;
            let mutations: Vec<_> = data
                .mutations
                .iter()
                .map(|m| mutation_section(m, data.injector_seconds <= 0.0))
                .collect();
            // tg `DnaConsole`: `<Window.Content scrollable>`.
            bsn! {
                @WindowContent { @scrollable: true }
                Children [
                    @Section {
                        @title: "Scanner",
                        @buttons: bsn_list! {
                            @ss13_ui_kit::Button { @icon: "door-open", @content: door_text }
                            ss13_ui_kit::Button { disabled: no_scanner }
                            Act("toggle_door")
                        },
                    }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Subject", @content: subject }
                            --
                            @LabeledListItem { @label: "Status", @content: status }
                            --
                            @LabeledListItem { @label: "Health", @content: health }
                            --
                            @LabeledListItem {
                                @label: "Stability",
                                @color: stability_color,
                                @content: stability,
                            }
                            --
                            @LabeledListItem { @label: "Enzymes", @content: enzymes }
                            --
                            @LabeledListItem { @label: "Identity", @content: identity }
                            --
                            @LabeledListItem { @label: "Injector", @content: injector }
                        ]
                    ]
                    --
                    @Section { @title: "Genetic Sequencer" }
                    Children [ {mutations} ]
                ]
            }
        }
    }

    /// `GeneticSequencer` -> `MutationInfo` + `GenomeSequencer` of one block.
    fn mutation_section(m: &DnaMutation, printer_ready: bool) -> impl Scene + use<> {
        let alias = m.alias.clone();
        let title = if m.discovered {
            format!("{} ({})", m.name, m.alias)
        } else {
            m.alias.clone()
        };
        let title = if m.active {
            format!("{title} [active]")
        } else {
            title
        };
        let param = |extra: &[(&str, String)]| {
            let mut v = vec![("alias".to_owned(), alias.clone())];
            v.extend(extra.iter().map(|(k, x)| ((*k).to_owned(), x.clone())));
            ActParams(v)
        };
        let genes: Vec<_> = m
            .sequence
            .chars()
            .enumerate()
            .map(|(i, c)| {
                let text = c.to_string();
                let params = param(&[
                    ("pos", (i + 1).to_string()),
                    ("pulseAction", "1".to_owned()),
                ]);
                let disabled = m.extra;
                bsn! {
                    @ss13_ui_kit::Button { @content: text }
                    ss13_ui_kit::Button { disabled: disabled }
                    Act("pulse_gene")
                    params
                }
            })
            .collect();
        let no_print = !(m.active && m.discovered && printer_ready);
        let (activator, mutator, nullify) = (
            param(&[("is_activator", "1".to_owned())]),
            param(&[("is_activator", "0".to_owned())]),
            param(&[]),
        );
        let no_nullify = m.class != 2;
        let info = format!("Instability: {}", m.instability);
        bsn! {
            @Section {
                @title: title,
                @buttons: bsn_list! {
                    @ss13_ui_kit::Button { @content: "Activator" }
                    ss13_ui_kit::Button { disabled: no_print }
                    Act("print_injector")
                    activator
                    --
                    @ss13_ui_kit::Button { @content: "Mutator" }
                    ss13_ui_kit::Button { disabled: no_print }
                    Act("print_injector")
                    mutator
                    --
                    @ss13_ui_kit::Button { @content: "Nullify" }
                    ss13_ui_kit::Button { disabled: no_nullify }
                    Act("nullify")
                    nullify
                },
            }
            Children [
                @UiBox Children [ Text(info) ]
                --
                Node { flex_wrap: FlexWrap::Wrap }
                Children [ {genes} ]
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_follows_subject_status() {
        assert_eq!(status_text(0), "Conscious");
        assert_eq!(status_text(1), "Unconscious");
        assert_eq!(status_text(2), "Dead");
    }
}
