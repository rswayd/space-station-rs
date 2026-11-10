//! The black market windows (client): tgui `BlackMarketUplink.tsx` and `LTSRBT.tsx`. The data
//! is [`super::market_ui`], the server half [`super::black_market`].
// shortcut: no hackerman theme or item icons; the shipment choice is a section above the
// catalogue instead of a modal, the category tabs are buttons; the LTSRBT shows the loaded
// item's name instead of its icon and sends its texts on Enter (tgui: on blur).

use bevy::scene::EntityScene;
use ss13_tgui::client::{ActOnChange, ActOnSubmit, ClientUi};
use ss13_ui_kit::prelude::*;

use super::client::format_money;
use super::market_ui::*;

fn act_button(
    text: String,
    action: &'static str,
    params: Vec<(String, String)>,
    disabled: bool,
    selected: bool,
) -> impl Scene + use<> {
    let params = ActParams(params);
    bsn! {
        @Button { @content: text }
        Button { disabled: disabled, selected: selected }
        Act(action)
        params
    }
}

fn label(text: String) -> impl Scene + use<> {
    bsn! { Text(text) TextColor({UiColor::Label.base()}) }
}

impl ClientUi for BlackMarketUi {
    const WIDTH: f32 = 670.0;
    const HEIGHT: f32 = 480.0;

    fn content(_: &(), d: &UplinkData) -> impl Scene {
        let money = d.money.unwrap_or(0);
        let balance = match d.money {
            Some(m) => format!("{} cr", format_money(m)),
            None => format!("N/A{}", super::MONEY_SYMBOL),
        };
        let head: Box<dyn SceneList> = Box::new(EntityScene(bsn! { Text(balance) }));
        let markets: Vec<_> = d
            .markets
            .iter()
            .map(|(id, name)| {
                act_button(
                    name.clone(),
                    "set_market",
                    vec![("market".into(), id.clone())],
                    false,
                    *id == d.viewing_market,
                )
            })
            .collect();
        let categories: Vec<_> = d
            .categories
            .iter()
            .map(|c| {
                act_button(
                    c.clone(),
                    "set_category",
                    vec![("category".into(), c.clone())],
                    false,
                    d.viewing_category.as_ref() == Some(c),
                )
            })
            .collect();
        let items: Vec<_> = d
            .items
            .iter()
            .map(|i| {
                let stock = if i.amount > 0 {
                    format!("{} in stock", i.amount)
                } else {
                    "Out of stock".to_owned()
                };
                let cost = format!("{} cr", format_money(i.cost));
                let name = i.name.clone();
                let desc = i.desc.clone();
                let buy = act_button(
                    "Buy".into(),
                    "select",
                    vec![("item".into(), i.id.clone())],
                    i.amount == 0 || i.cost > money,
                    false,
                );
                bsn! {
                    Node { flex_direction: FlexDirection::Column, padding: UiRect::all(px(6)) }
                    Children [
                        Node { flex_direction: FlexDirection::Row, column_gap: px(12), align_items: AlignItems::Center }
                        Children [
                            Node { flex_grow: 1.0 } Children [ Text(name) ]
                            --
                            {EntityScene(label(stock))}
                            --
                            Text(cost)
                            --
                            {EntityScene(buy)}
                        ]
                        --
                        Text(desc)
                    ]
                }
            })
            .collect();
        let shipment: Vec<Box<dyn Scene>> = if d.buying {
            let methods: Vec<_> = d
                .delivery_methods
                .iter()
                .filter(|(name, _)| name != "LTSRBT" || d.ltsrbt_built)
                .map(|(name, price)| {
                    let description = d
                        .delivery_method_description
                        .iter()
                        .find(|(m, _)| m == name)
                        .map(|(_, t)| t.clone())
                        .unwrap_or_default();
                    let title = name.clone();
                    let button = act_button(
                        format!("{} cr", format_money(*price)),
                        "buy",
                        vec![("method".into(), name.clone())],
                        money < *price,
                        false,
                    );
                    bsn! {
                        Node { flex_direction: FlexDirection::Column, width: px(220), row_gap: px(6) }
                        Children [ Text(title) -- Text(description) -- {EntityScene(button)} ]
                    }
                })
                .collect();
            let cancel = bsn! {
                @Button { @content: "Cancel" }
                Button { color: UiColor::Bad }
                Act("cancel")
            };
            vec![Box::new(bsn! {
                @Section { @title: "Shipping" }
                Children [
                    Node { flex_direction: FlexDirection::Row, column_gap: px(12) }
                    Children [ {methods} ]
                    --
                    {EntityScene(cancel)}
                ]
            }) as Box<dyn Scene>]
        } else {
            Vec::new()
        };
        bsn! {
            @WindowContent { @scrollable: true }
            Children [
                {shipment}
                --
                @Section { @title: "Black Market Uplink", @buttons: {Some(head)} }
                --
                Node { flex_direction: FlexDirection::Row, column_gap: px(4) }
                Children [ {markets} ]
                --
                Node { flex_direction: FlexDirection::Row }
                Children [
                    Node { flex_direction: FlexDirection::Column, row_gap: px(4) }
                    Children [ {categories} ]
                    --
                    Node { flex_direction: FlexDirection::Column, flex_grow: 1.0 }
                    Children [ {items} ]
                ]
            ]
        }
    }
}

impl ClientUi for LtsrbtUi {
    const WIDTH: f32 = 300.0;
    const HEIGHT: f32 = 380.0;

    fn content(_: &(), d: &LtsrbtData) -> impl Scene {
        let name_field = Input {
            value: d.name.clone(),
            placeholder: "Insert a name".into(),
            max_length: Some(42),
            fluid: true,
            ..Default::default()
        };
        let on_name = ActOnSubmit {
            action: "change_name".into(),
            param: "value".into(),
            params: Vec::new(),
        };
        let price = NumberInput {
            value: d.price as f64,
            min_value: d.min_price as f64,
            max_value: d.max_price as f64,
            step: 1.0,
            unit: "cr".into(),
            ..Default::default()
        };
        let on_price = ActOnChange {
            action: "change_price".into(),
            param: "value".into(),
        };
        let desc_field = Input {
            value: d.desc.clone(),
            placeholder: "Insert a description (or don't)".into(),
            max_length: Some(1024),
            fluid: true,
            ..Default::default()
        };
        let on_desc = ActOnSubmit {
            action: "change_desc".into(),
            param: "value".into(),
            params: Vec::new(),
        };
        let loaded = d.loaded.clone();
        bsn! {
            @WindowContent
            Children [
                Node { flex_direction: FlexDirection::Row, column_gap: px(4) }
                Children [
                    Node { width: percent(75) } name_field on_name
                    --
                    Node { width: px(70) } price on_price
                ]
                --
                @Section
                Children [ Text(loaded) ]
                --
                Node { width: percent(100) } desc_field on_desc
                --
                @Button { @icon: "truck-arrow-right", @content: "Place on Market", @fluid: true }
                Act("place_on_market")
            ]
        }
    }
}
