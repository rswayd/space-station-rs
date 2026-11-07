//! Client half of the statpanel ([`crate::statpanel`]): the tabs across the top (Status first,
//! then the verb categories), the Status lines, a verb category's buttons; the shown tab is
//! window-local state (the stat browser's `current_tab`).

use bevy::scene::{EntityScene, SceneList};
use ss13_tgui::client::{AppTguiClientExt, ClientUi, SetLocal, UiLocalState};
use ss13_ui_kit::ActParams;
use ss13_ui_kit::prelude::*;

use crate::statpanel::*;

/// `useState` key: the shown tab's name.
const SHOWN: &str = "tab";

pub struct StatPanelClientPlugin;

impl Plugin for StatPanelClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<StatPanelUi>();
    }
}

impl ClientUi for StatPanelUi {
    const WIDTH: f32 = 560.0;
    const HEIGHT: f32 = 420.0;

    fn content(_: &(), data: &StatPanelData) -> impl Scene {
        view(data, &UiLocalState::default())
    }

    fn content_local(_: &(), data: &StatPanelData, local: &UiLocalState) -> impl Scene {
        view(data, local)
    }
}

/// The shown tab: the chosen one while the mob still has it, else the first (Status).
pub fn shown_tab<'a>(data: &'a StatPanelData, chosen: &str) -> Option<&'a StatTab> {
    data.tabs
        .iter()
        .find(|t| t.name == chosen)
        .or_else(|| data.tabs.first())
}

fn view(data: &StatPanelData, local: &UiLocalState) -> impl Scene + use<> {
    let shown = shown_tab(data, local.get(SHOWN))
        .cloned()
        .unwrap_or_default();
    let tabs: Vec<_> = data
        .tabs
        .iter()
        .map(|t| {
            let label = t.name.clone();
            let value = t.name.clone();
            let selected = t.name == shown.name;
            let name = Name::new(format!("statpanel tab: {}", t.name));
            bsn! {
                @TabsTab { @content: label }
                TabsTab { selected: selected }
                SetLocal { key: {SHOWN.to_owned()}, value: value, rerender: true }
                name
            }
        })
        .collect();
    let body = if shown.verbs.is_empty() {
        Box::new(EntityScene(lines(&shown.lines))) as Box<dyn SceneList>
    } else {
        Box::new(EntityScene(verbs(&shown.name, &shown.verbs))) as Box<dyn SceneList>
    };
    bsn! {
        @WindowContent
        Children [
            @Stack { @vertical: true, @fill: true }
            Children [
                @StackItem
                Children [ @Tabs { @fluid: true } Children [ {tabs} ] ]
                --
                @StackItem { @grow: 1.0 }
                Node { min_height: px(0) }
                Children [
                    @Section { @fill: true, @scrollable: true }
                    Children [ {body} ]
                ]
            ]
        ]
    }
}

/// The Status tab's lines.
fn lines(lines: &[String]) -> impl Scene + use<> {
    let rows: Vec<_> = lines
        .iter()
        .map(|line| {
            let line = line.clone();
            bsn! { Node Children [ Text(line) ] }
        })
        .collect();
    bsn! {
        Node { flex_direction: FlexDirection::Column, row_gap: px(2) }
        Children [ {rows} ]
    }
}

/// A verb category: one button per verb, wrapped (the stat browser's verb grid).
fn verbs(tab: &str, verbs: &[String]) -> impl Scene + use<> {
    let buttons: Vec<_> = verbs
        .iter()
        .map(|verb| {
            let label = verb.clone();
            let params = ActParams(vec![
                (TAB.to_owned(), tab.to_owned()),
                (NAME.to_owned(), verb.clone()),
            ]);
            let name = Name::new(format!("statpanel verb: {verb}"));
            bsn! {
                @ss13_ui_kit::Button { @content: label, @compact: true }
                Act({VERB.to_owned()})
                params
                name
            }
        })
        .collect();
    bsn! {
        Node {
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Wrap,
            column_gap: px(4),
            row_gap: px(4),
        }
        Children [ {buttons} ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lost_tab_falls_back_to_status() {
        let data = StatPanelData {
            tabs: vec![
                StatTab {
                    name: STATUS_TAB.to_owned(),
                    ..default()
                },
                StatTab {
                    name: EMOTES_TAB.to_owned(),
                    verbs: vec!["▷ Смеяться".to_owned()],
                    ..default()
                },
            ],
        };
        assert_eq!(shown_tab(&data, EMOTES_TAB).unwrap().name, EMOTES_TAB);
        assert_eq!(shown_tab(&data, "").unwrap().name, STATUS_TAB);
        assert_eq!(shown_tab(&data, "IC").unwrap().name, STATUS_TAB);
    }
}
