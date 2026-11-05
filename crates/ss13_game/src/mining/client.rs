//! The mining windows (client): tgui `OreBox.tsx`, `OreRedemptionMachine.tsx` and
//! `ProduceConsole.tsx`. The data is in [`super`], the server halves in [`super::rocks`],
//! [`super::orm`] and [`super::console`].
//!
//! Client-only state (`useState`): the ORM's materials/alloys tab, the console's tab and
//! category ([`UiLocalState`] keys).
// shortcut: no search bars, compact/condensed toggles, the ORM's custom release amount input
// (`[Max: n]` releases the maximum) or free-use counters.

use bevy::scene::EntityScene;
use ss13_tgui::client::{ActOnChange, AppTguiClientExt, ClientUi, SetLocal, UiLocalState};
use ss13_ui_kit::prelude::*;

use super::*;
use crate::science::client::sprite;

const ORM_TAB: &str = "tab";
const CONSOLE_TAB: &str = "console_tab";
const CATEGORY: &str = "shop_category";
const CHECKOUT: &str = "checkout";

pub struct MiningClientPlugin;

impl Plugin for MiningClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<OreBoxUi>()
            .register_ui_view::<OrmUi>()
            .register_ui_view::<ProduceConsoleUi>();
    }
}

/// tgui-core `toTitleCase`.
fn title_case(text: &str) -> String {
    text.split(' ')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect())
                .unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

// --- Ore box ---------------------------------------------------------------------------------

impl ClientUi for OreBoxUi {
    const WIDTH: f32 = 335.0;
    const HEIGHT: f32 = 415.0;

    fn content(_: &(), data: &OreBoxData) -> impl Scene {
        ore_box_view(data)
    }
}

fn ore_box_view(data: &OreBoxData) -> impl Scene + use<> {
    let rows: Vec<_> = data
        .materials
        .iter()
        .map(|m| {
            let name = title_case(&m.name);
            let amount = m.amount.to_string();
            bsn! {
                @TableRow
                Children [
                    @TableCell { @content: name }
                    --
                    @TableCell { @collapsing: true, @content: amount, @color: UiColor::Label }
                ]
            }
        })
        .collect();
    let empty = data.materials.is_empty();
    let buttons: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        @ss13_ui_kit::Button { @content: "Empty" }
        ss13_ui_kit::Button { disabled: empty }
        Act({REMOVE_ALL.to_owned()})
        Name("orebox: empty")
    }));
    bsn! {
        @WindowContent
        Children [
            @Section { @title: "Ores & Boulders", @buttons: buttons }
            Children [
                @Table
                Children [
                    @TableRow
                    Children [
                        @TableCell { @header: true, @content: "Item" }
                        --
                        @TableCell { @header: true, @collapsing: true, @content: "Amount" }
                    ]
                    --
                    {rows}
                ]
            ]
            --
            @Section
            Children [
                Text("Ores can be loaded here via a mining satchel or by hand. Boulders can also be stored here.\nGibtonite is not accepted.")
            ]
        ]
    }
}

// --- Ore redemption machine ------------------------------------------------------------------

impl ClientUi for OrmUi {
    const WIDTH: f32 = 435.0;
    const HEIGHT: f32 = 500.0;

    fn content(_: &(), data: &OrmData) -> impl Scene {
        orm_view(data, &UiLocalState::default())
    }

    fn content_local(_: &(), data: &OrmData, local: &UiLocalState) -> impl Scene {
        orm_view(data, local)
    }
}

fn orm_view(data: &OrmData, local: &UiLocalState) -> impl Scene + use<> {
    let alloys = local.get(ORM_TAB) == "alloy";
    let (name, cash) = match &data.user {
        Some(u) => (u.name.clone(), u.cash.to_string()),
        None => (
            "No Name Detected".to_owned(),
            "No Balance Detected".to_owned(),
        ),
    };
    let id_color = if data.user.is_some() {
        UiColor::Green
    } else {
        UiColor::Red
    };
    let points = format!(" {}", data.unclaimed_points);
    let claim_disabled = data.unclaimed_points == 0 || data.disconnected.is_some();
    let claim_tooltip = data.disconnected.clone().unwrap_or_default();
    let rows: Vec<_> = data
        .materials
        .iter()
        .filter(|m| m.alloy == alloys)
        .map(material_row)
        .collect();
    let tab = |key: &str, label: &str, selected: bool| {
        let (key, label) = (key.to_owned(), label.to_owned());
        bsn! {
            @TabsTab { @content: label, @icon: "list" }
            TabsTab { selected: selected }
            SetLocal { key: {ORM_TAB.to_owned()}, value: key, rerender: true }
        }
    };
    let materials_tab = tab("material", "Materials", !alloys);
    let alloys_tab = tab("alloy", "Alloys", alloys);
    bsn! {
        @WindowContent
        Children [
            @Section
            Children [
                Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
                Children [
                    Node { margin: UiRect::right(px(6)) }
                    Children [ @Icon { @name: "id-card", @size: 3.0, @color: id_color } ]
                    --
                    Node { flex_grow: 1.0 }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Name", @content: name }
                            --
                            @LabeledListItem { @label: "Point Balance", @content: cash }
                        ]
                    ]
                ]
            ]
            --
            @Section
            Children [
                Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
                Children [
                    @Icon { @name: "coins", @color: UiColor::Gold }
                    --
                    Node { flex_grow: 1.0, margin: UiRect::left(px(6)) }
                    Children [ Text("Unclaimed points:") -- Text(points) ]
                    --
                    @ss13_ui_kit::Button { @content: "Claim", @tooltip: claim_tooltip }
                    ss13_ui_kit::Button { disabled: claim_disabled }
                    Act({CLAIM.to_owned()})
                    Name("orm: claim")
                ]
            ]
            --
            @NoticeBox { @info: true }
            Children [ Text("This machine only accepts ore. Gibtonite and Slag are not accepted.") ]
            --
            @Section
            Children [
                @Tabs
                Children [ {materials_tab} -- {alloys_tab} ]
            ]
            --
            @Section { @scrollable: true }
            Node { flex_grow: 1.0, min_height: px(0) }
            Children [ @Table Children [ {rows} ] ]
        ]
    }
}

/// `MaterialRow`: x1, x5 and the maximum (50 or what there is).
fn material_row(m: &OrmMaterial) -> impl Scene + use<> {
    let image = sprite(m.icon);
    let name = title_case(&m.name);
    let sheets = m.amount.max(0.0).floor() as u32;
    let amount = format!("{sheets} {}", if sheets == 1 { "sheet" } else { "sheets" });
    let (action, key) = if m.alloy {
        (SMELT, PATH)
    } else {
        (RELEASE, MATERIAL_REF)
    };
    let id = m.id.to_string();
    let params = |n: u32| {
        ActParams(vec![
            (key.to_owned(), id.clone()),
            (SHEETS.to_owned(), n.to_string()),
        ])
    };
    let max = sheets.min(50);
    let (one, five, all) = (params(1), params(5), params(max));
    let cost = |n: f32| {
        if m.value > 0.0 {
            format!("{} cr", m.value * n)
        } else {
            "No cost".to_owned()
        }
    };
    let (tip_one, tip_five) = (cost(1.0), cost(5.0));
    let max_text = format!("[Max: {max}]");
    let row_name = Name::new(format!("orm: {}", m.name));
    let (a1, a5, am) = (action.to_owned(), action.to_owned(), action.to_owned());
    bsn! {
        @TableRow
        row_name
        Children [
            @TableCell { @collapsing: true }
            Children [ Node { width: px(18), height: px(18) } Children [ {image} ] ]
            --
            @TableCell { @content: name }
            --
            @TableCell { @collapsing: true, @content: amount, @color: UiColor::Label }
            --
            @TableCell { @collapsing: true }
            Children [
                Node { flex_wrap: FlexWrap::NoWrap }
                Children [
                    @ss13_ui_kit::Button { @content: "x1", @tooltip: tip_one }
                    ss13_ui_kit::Button { color: UiColor::Transparent }
                    Act(a1)
                    one
                    --
                    @ss13_ui_kit::Button { @content: "x5", @tooltip: tip_five }
                    ss13_ui_kit::Button { color: UiColor::Transparent }
                    Act(a5)
                    five
                    --
                    @ss13_ui_kit::Button { @content: max_text }
                    ss13_ui_kit::Button { color: UiColor::Transparent, disabled: {max == 0} }
                    Act(am)
                    all
                ]
            ]
        ]
    }
}

// --- Order console ---------------------------------------------------------------------------

impl ClientUi for ProduceConsoleUi {
    const WIDTH: f32 = 500.0;
    const HEIGHT: f32 = 400.0;

    fn content(st: &ProduceStatic, data: &ProduceData) -> impl Scene {
        console_view(st, data, &UiLocalState::default())
    }

    fn content_local(st: &ProduceStatic, data: &ProduceData, local: &UiLocalState) -> impl Scene {
        console_view(st, data, local)
    }
}

fn console_view(
    st: &ProduceStatic,
    data: &ProduceData,
    local: &UiLocalState,
) -> impl Scene + use<> {
    let checkout = local.get(CONSOLE_TAB) == CHECKOUT;
    let points = format!("{} {}", data.points.unwrap_or(0), st.credit_type);
    let body: Box<dyn SceneList> = if checkout {
        Box::new(EntityScene(checkout_view(st, data)))
    } else {
        Box::new(EntityScene(shopping_view(st, data, local)))
    };
    let cooldown = (!data.off_cooldown).then(|| {
        EntityScene(bsn! {
            @NoticeBox { @success: true }
            Children [ Text("Order sent! Machine on cooldown...") ]
        })
    });
    bsn! {
        @WindowContent
        Children [
            {cooldown}
            --
            @Section
            Children [
                Node { flex_direction: FlexDirection::Row }
                Children [
                    Node { flex_grow: 3.0 }
                    Children [
                        @ss13_ui_kit::Button { @icon: "cart-plus", @content: "Покупки", @fluid: true }
                        ss13_ui_kit::Button { color: UiColor::Green, selected: {!checkout} }
                        SetLocal { key: {CONSOLE_TAB.to_owned()}, value: {String::new()}, rerender: true }
                    ]
                    --
                    Node { flex_grow: 1.0 }
                    Children [
                        @ss13_ui_kit::Button { @icon: "dollar-sign", @content: "Оплата", @fluid: true }
                        ss13_ui_kit::Button { color: UiColor::Green, selected: checkout }
                        SetLocal { key: {CONSOLE_TAB.to_owned()}, value: {CHECKOUT.to_owned()}, rerender: true }
                        Name("console: checkout tab")
                    ]
                ]
                --
                Node { justify_content: JustifyContent::FlexEnd }
                Children [ Text(points) Name("console: points") ]
            ]
            --
            Node { flex_grow: 1.0, min_height: px(0), flex_direction: FlexDirection::Column }
            Children [ {body} ]
        ]
    }
}

/// `ShoppingTab`: the category tabs and its items.
fn shopping_view(
    st: &ProduceStatic,
    data: &ProduceData,
    local: &UiLocalState,
) -> impl Scene + use<> {
    let selected = match local.get(CATEGORY) {
        "" => st.order_categories.first().cloned().unwrap_or_default(),
        c => c.to_owned(),
    };
    let tabs: Vec<_> = st
        .order_categories
        .iter()
        .map(|c| {
            let is = *c == selected;
            let label = c.clone();
            let value = c.clone();
            bsn! {
                @TabsTab { @content: label }
                TabsTab { selected: is }
                SetLocal { key: {CATEGORY.to_owned()}, value: value, rerender: true }
            }
        })
        .collect();
    let rows: Vec<_> = st
        .order_datums
        .iter()
        .filter(|d| d.cat == selected)
        .map(|d| order_row(st, data, d))
        .collect();
    bsn! {
        Node { flex_direction: FlexDirection::Column, flex_grow: 1.0 }
        Children [
            @Section
            Children [ @Tabs { @fluid: true } Children [ {tabs} ] ]
            --
            @Section { @scrollable: true }
            Node { flex_grow: 1.0, min_height: px(0) }
            Children [ @Table Children [ {rows} ] ]
        ]
    }
}

fn order_row(st: &ProduceStatic, data: &ProduceData, d: &OrderDatum) -> impl Scene + use<> {
    let image = sprite(d.icon);
    let name = title_case(&d.name);
    let desc = d.desc.clone();
    let cost = format!("{} {}", d.cost, st.credit_type);
    let amount = data.amount(&d.name);
    let target = || ActParams(vec![(TARGET.to_owned(), d.r#ref.to_string())]);
    let (minus, plus, set) = (target(), target(), target());
    let input = NumberInput {
        value: f64::from(amount),
        min_value: 0.0,
        max_value: f64::from(d.max),
        step: 1.0,
        ..Default::default()
    };
    let on_change = ActOnChange {
        action: CART_SET.into(),
        param: AMT.into(),
    };
    let row_name = Name::new(format!("console: {}", d.name));
    bsn! {
        @TableRow
        row_name
        Children [
            @TableCell { @collapsing: true }
            Children [ Node { width: px(36), height: px(36) } Children [ {image} ] ]
            --
            @TableCell { @content: name, @color: UiColor::Label }
            --
            @TableCell { @collapsing: true }
            Children [
                @ss13_ui_kit::Button { @icon: "info", @tooltip: desc }
                ss13_ui_kit::Button { color: UiColor::Transparent }
            ]
            --
            @TableCell { @collapsing: true, @content: cost }
            --
            @TableCell { @collapsing: true }
            Children [
                Node { flex_wrap: FlexWrap::NoWrap, align_items: AlignItems::Center }
                Children [
                    @ss13_ui_kit::Button { @icon: "minus" }
                    ss13_ui_kit::Button { disabled: {amount == 0} }
                    Act({REMOVE_ONE.to_owned()})
                    minus
                    --
                    @ss13_ui_kit::Button { @icon: "plus" }
                    ss13_ui_kit::Button { disabled: {amount >= d.max} }
                    Act({ADD_ONE.to_owned()})
                    plus
                    --
                    Node { width: px(41) }
                    input
                    on_change
                    set
                ]
            ]
        ]
    }
}

/// `CheckoutTab`: the cart and the two ways to pay.
fn checkout_view(st: &ProduceStatic, data: &ProduceData) -> impl Scene + use<> {
    let rows: Vec<_> = st
        .order_datums
        .iter()
        .filter(|d| data.amount(&d.name) > 0)
        .map(|d| {
            let name = title_case(&d.name);
            let line = format!("{} × {} {}", data.amount(&d.name), d.cost, st.credit_type);
            bsn! {
                @TableRow
                Children [
                    @TableCell { @content: name }
                    --
                    @TableCell { @collapsing: true, @content: line, @color: UiColor::Label }
                ]
            }
        })
        .collect();
    let nothing = rows.is_empty().then(|| {
        EntityScene(bsn! {
            Node { justify_content: JustifyContent::Center, margin: UiRect::top(px(24)) }
            Children [ Text("Ничего! (Закажем чего, а?)") ]
        })
    });
    let cargo_cost = (data.total_cost as f32 * st.cargo_cost_multiplier).floor() as u32;
    let express_cost = (data.total_cost as f32 * st.express_cost_multiplier).round() as u32;
    let items: u32 = data.item_amts.iter().map(|i| i.amt).sum();
    let purchase = (!st.forced_express).then(|| {
        let too_cheap = cargo_cost < st.cargo_value;
        let tooltip = if too_cheap {
            format!("Общая сумма должна быть больше или равна {}", st.cargo_value)
        } else {
            st.purchase_tooltip.clone()
        };
        let label = format!("Покупки: {cargo_cost} {}", st.credit_type);
        EntityScene(bsn! {
            Node { flex_grow: 1.0 }
            Children [
                @ss13_ui_kit::Button { @icon: "plane-departure", @content: label, @tooltip: tooltip, @fluid: true }
                ss13_ui_kit::Button { disabled: too_cheap }
                Act({PURCHASE.to_owned()})
                Name("console: purchase")
            ]
        })
    });
    let express_tooltip = if items == 0 {
        "Order at least 1 item".to_owned()
    } else {
        st.express_tooltip.clone()
    };
    let express_label = format!("Экспресс: {express_cost} {}", st.credit_type);
    bsn! {
        Node { flex_direction: FlexDirection::Column, flex_grow: 1.0 }
        Children [
            @Section { @title: "Список покупок:", @scrollable: true }
            Node { flex_grow: 1.0, min_height: px(0) }
            Children [ {nothing} -- @Table Children [ {rows} ] ]
            --
            @Section
            Children [
                Node { flex_direction: FlexDirection::Row }
                Children [
                    {purchase}
                    --
                    Node { flex_grow: 1.0 }
                    Children [
                        @ss13_ui_kit::Button { @icon: "parachute-box", @content: express_label, @tooltip: express_tooltip, @fluid: true }
                        ss13_ui_kit::Button { color: UiColor::Yellow, disabled: {items == 0} }
                        Act({EXPRESS.to_owned()})
                        Name("console: express")
                    ]
                ]
            ]
        ]
    }
}
