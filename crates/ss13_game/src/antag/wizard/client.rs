//! Client half of the wizard: the `Spellbook` window (`Spellbook/index.tsx`,
//! `SpellTabDisplay.tsx`): the categories as tabs, each entry with its cost, cooldown, whether it
//! needs the robes, its description and the buy (and refund) button; the points left below.
// shortcut: no table of contents, search, loadouts, randomizing, `wizard` theme or enscribed
// owner's name; the categories are vertical tabs instead of the book's two pages.

use bevy::app::Propagate;
use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi, SetLocal, UiLocalState};
use ss13_ui_kit::ActParams;
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::{Bold, FontPx};

use super::{PURCHASE, REFUND, SpellbookData, SpellbookEntry, SpellbookStatic, SpellbookUi};

/// `useState` key: the shown category.
const TAB: &str = "category";

pub struct WizardClientPlugin;

impl Plugin for WizardClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<SpellbookUi>();
    }
}

impl ClientUi for SpellbookUi {
    const WIDTH: f32 = 950.0;
    const HEIGHT: f32 = 540.0;

    fn content(st: &SpellbookStatic, data: &SpellbookData) -> impl Scene {
        book_view(st, data, &UiLocalState::default())
    }

    fn content_local(
        st: &SpellbookStatic,
        data: &SpellbookData,
        local: &UiLocalState,
    ) -> impl Scene {
        book_view(st, data, local)
    }
}

fn book_view(
    st: &SpellbookStatic,
    data: &SpellbookData,
    local: &UiLocalState,
) -> impl Scene + use<> {
    let last = st.categories.len().saturating_sub(1);
    let shown = local.get(TAB).parse::<usize>().unwrap_or(0).min(last);
    let tabs: Vec<_> = st
        .categories
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let name = c.clone();
            bsn! {
                @TabsTab { @content: name }
                TabsTab { selected: {i == shown} }
                SetLocal { key: {TAB.to_owned()}, value: {i.to_string()}, rerender: true }
            }
        })
        .collect();
    let category = st.categories.get(shown).cloned().unwrap_or_default();
    let rows: Vec<_> = st
        .entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.category == category)
        .map(|(i, e)| entry_view(i, e, data))
        .collect();
    let points = format!("Осталось очков: {}", data.points);
    bsn! {
        @WindowContent
        Children [
            @Stack { @vertical: true, @fill: true }
            Children [
                @StackItem { @grow: 1.0 }
                Node { min_height: px(0) }
                Children [
                    @Stack { @fill: true }
                    Children [
                        @StackItem
                        Node { width: px(160), flex_shrink: 0.0 }
                        Children [
                            @Tabs { @vertical: true, @fluid: true }
                            Children [ {tabs} ]
                        ]
                        --
                        @StackItem { @grow: 1.0 }
                        Node { min_width: px(0) }
                        Children [
                            @Section { @title: category, @fill: true, @scrollable: true }
                            Children [
                                Node { flex_direction: FlexDirection::Column, row_gap: px(10) }
                                Children [ {rows} ]
                            ]
                        ]
                    ]
                ]
                --
                @StackItem
                Children [
                    @Section
                    Children [
                        Node Name("spellbook: points") Propagate::<Bold>(Bold(true))
                        Children [ Text(points) ]
                    ]
                ]
            ]
        ]
    }
}

/// `SpellTabDisplay`'s entry: name, cost and cooldown, description, the buttons.
fn entry_view(i: usize, entry: &SpellbookEntry, data: &SpellbookData) -> impl Scene + use<> {
    let times = data.times.get(i).copied().unwrap_or(0);
    let can_buy = data.can_buy(i, entry);
    let cooldown = data
        .cooldowns
        .get(i)
        .copied()
        .flatten()
        .map(|c| format!("Перезарядка: {c} с"));
    let garb = entry
        .requires_wizard_garb
        .then(|| "Требуется мантия".to_owned());
    let meta = [Some(format!("Стоимость: {}", entry.cost)), cooldown, garb]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" | ");
    let name = entry.name.clone();
    let desc = entry.desc.clone();
    let buyword = entry.buyword.clone();
    let buy_params = ActParams(vec![("spellref".into(), entry.id.clone())]);
    let button_name = Name::new(format!("spellbook: {}", entry.id));
    let refund = (entry.refundable && times > 0).then(|| {
        let params = ActParams(vec![("spellref".into(), entry.id.clone())]);
        EntityScene(bsn! {
            @ss13_ui_kit::Button { @content: "Вернуть", @icon: "arrow-rotate-left" }
            Act({REFUND.to_owned()})
            params
        })
    });
    let dim = Color::srgba(1.0, 1.0, 1.0, 0.6);
    bsn! {
        Node { flex_direction: FlexDirection::Column, row_gap: px(4) }
        Children [
            Node { flex_direction: FlexDirection::Row, column_gap: px(8) }
            Children [
                Node Propagate::<Bold>(Bold(true)) Propagate::<FontPx>(FontPx(14.0))
                Children [ Text(name) ]
                --
                Node Propagate::<TextColor>(TextColor(dim))
                Children [ Text(meta) ]
            ]
            --
            Node Children [ Text(desc) ]
            --
            Node { flex_direction: FlexDirection::Row, column_gap: px(6) }
            Children [
                @ss13_ui_kit::Button { @content: buyword, @icon: "plus" }
                ss13_ui_kit::Button { disabled: {!can_buy} }
                Act({PURCHASE.to_owned()})
                buy_params
                button_name
                --
                {refund}
            ]
        ]
    }
}
