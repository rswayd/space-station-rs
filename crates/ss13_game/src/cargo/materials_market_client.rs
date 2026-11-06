//! The `MatMarket` window (client): tgui `MatMarket.tsx`. The data is [`super::MatMarketData`],
//! the server half [`super::materials_market`].
// shortcut: no price range tooltip, market crash modal (a notice instead) or collapsible
// instructions; buy buttons have no cost tooltip.

use bevy::app::Propagate;
use bevy::scene::EntityScene;
use ss13_tgui::client::ClientUi;
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::Bold;

use super::client::format_money;
use super::*;

/// The JSX's `CARGO_CRATE_VALUE`.
const CRATE_VALUE: f64 = 200.0;
const AMOUNTS: [u32; 5] = [1, 5, 10, 25, 50];

impl ClientUi for MatMarketUi {
    const WIDTH: f32 = 990.0;
    const HEIGHT: f32 = 615.0;

    fn content(_: &(), data: &MatMarketData) -> impl Scene {
        view(data)
    }
}

/// `toTitleCase`.
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

fn view(data: &MatMarketData) -> impl Scene + use<> {
    let toggle: Box<dyn SceneList> = if data.can_order_cargo {
        let (text, color, icon) = if data.ordering_private {
            ("Order via Cargo Budget?", UiColor::Transparent, "square-o")
        } else {
            (
                "Ordering via Cargo Budget",
                UiColor::Green,
                "check-square-o",
            )
        };
        Box::new(EntityScene(bsn! {
            @ss13_ui_kit::Button {
                @icon: icon,
                @content: text,
                @tooltip: "Place order from cargo budget.",
            }
            ss13_ui_kit::Button { color: color }
            Act({MARKET_TOGGLE_BUDGET.to_owned()})
        }))
    } else {
        Box::new(EntityScene(bsn! { Node }))
    };
    let balance = format!("Balance: {} cr.", format_money(data.credit_balance));
    let order = format!("Order: {} cr.", format_money(data.order_balance));
    let update = format!(
        "{} seconds until next update",
        (data.update_time / 10.0).round()
    );
    let update_color = if data.update_time > 150.0 {
        UiColor::Green.text().unwrap_or_default()
    } else {
        Color::srgb_u8(0xad, 0x75, 0x26)
    };
    let crash = data.catastrophe.then(|| {
        EntityScene(bsn! {
            @NoticeBox { @danger: true }
            Children [ Text("Market crash! Trading is suspended.") ]
        })
    });
    let rows: Vec<_> = data
        .materials
        .iter()
        .map(|m| material_row(m, data))
        .collect();
    bsn! {
        @WindowContent
        Node { overflow: Overflow::scroll_y() }
        Children [
            {crash}
            --
            @Section { @title: "Materials for sale", @buttons: toggle }
            Children [
                Node { flex_direction: FlexDirection::Row, column_gap: px(24) }
                Children [
                    Text(balance)
                    --
                    Text(order)
                    --
                    Node Propagate::<TextColor>(TextColor(update_color)) Children [ Text(update) ]
                    --
                    @ss13_ui_kit::Button { @icon: "times", @content: "Clear" }
                    ss13_ui_kit::Button { color: UiColor::Transparent }
                    Act({MARKET_CLEAR.to_owned()})
                ]
            ]
            --
            Node { flex_direction: FlexDirection::Column }
            Children [ {rows} ]
            --
            @NoticeBox { @info: true }
            Children [
                Text("Buy orders for material sheets placed here will be ordered on the next \
                      cargo shipment. To sell materials, please insert sheets or similar \
                      stacks of materials.")
            ]
        ]
    }
}

fn material_row(m: &MarketMaterial, data: &MatMarketData) -> impl Scene + use<> {
    let name = title_case(&m.name);
    let [r, g, b] = m.color;
    let name_color = Color::srgb_u8(r, g, b);
    let elastic_color = if m.elastic < 33.0 {
        UiColor::Red
    } else if m.elastic < 66.0 {
        UiColor::Orange
    } else {
        UiColor::Green
    };
    let elastic = format!("Elasticity: {}%", m.elastic.round());
    let price = format!("Trading at {} cr.", format_money(m.price.round() as i64));
    let suspended = m.price < m.min_threshold;
    let stock = if suspended {
        "Material price critical! Trading temporarily suspended.".to_owned()
    } else {
        let quantity = if m.quantity > 0.0 {
            format!("{}", m.quantity.round())
        } else {
            "Zero".to_owned()
        };
        let requested = if m.requested > 0 {
            m.requested.to_string()
        } else {
            "Zero".to_owned()
        };
        format!(
            "{quantity} sheets of {} trading. {requested} sheets ordered.",
            m.name
        )
    };
    let trend_color = match m.trend.as_str() {
        "up" => UiColor::Green,
        "down" => UiColor::Red,
        _ => UiColor::Default,
    };
    let trend = format!("{name} is trending {}.", m.trend);
    // The JSX's buy button rules (the server checks again).
    let total_order_cost = if data.order_balance > 0 {
        data.order_balance as f64
    } else {
        CRATE_VALUE
    };
    let multiplier = if data.ordering_private { 1.1 } else { 1.0 };
    let buttons: Vec<_> = AMOUNTS
        .iter()
        .map(|&n| {
            let disabled = data.catastrophe
                || suspended
                || (data.credit_balance as f64 - total_order_cost)
                    < m.price * f64::from(n) * multiplier
                || f64::from(m.requested + n) > m.quantity;
            let label = if n == 1 {
                "Buy 1".to_owned()
            } else {
                n.to_string()
            };
            let params = ActParams(vec![
                (MARKET_QUANTITY.to_owned(), n.to_string()),
                (MARKET_MATERIAL.to_owned(), m.name.clone()),
            ]);
            bsn! {
                @ss13_ui_kit::Button { @content: label }
                ss13_ui_kit::Button { disabled: disabled }
                Act({MARKET_BUY.to_owned()})
                params
            }
        })
        .collect();
    let trend_text = trend_color.text().unwrap_or_default();
    let elastic_text = elastic_color.text().unwrap_or_default();
    bsn! {
        @Section
        Children [
            Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center, column_gap: px(12) }
            Children [
                Node { width: percent(15) }
                Propagate::<TextColor>(TextColor(name_color))
                Propagate::<Bold>(Bold(true))
                Children [ Text(name) ]
                --
                Node { width: percent(10) }
                Propagate::<TextColor>(TextColor(elastic_text))
                Children [ Text(elastic) ]
                --
                Node { width: percent(15) } Children [ Text(price) ]
                --
                Node { width: percent(30) } Children [ Text(stock) ]
                --
                Node { width: percent(15) }
                Propagate::<TextColor>(TextColor(trend_text))
                Children [ Text(trend) ]
                --
                Node { flex_direction: FlexDirection::Row }
                Children [ {buttons} ]
            ]
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_case_words() {
        assert_eq!(title_case("bluespace crystal"), "Bluespace Crystal");
    }
}
