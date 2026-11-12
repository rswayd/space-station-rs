//! `PowerMonitor` (power monitoring console): `tgui/packages/tgui/interfaces/PowerMonitor.tsx`
//! and `/obj/machinery/computer/monitor/ui_data` (`code/modules/power/monitor.dm`). The server
//! half lives with the game (`ss13_game::power::monitor`).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct PowerMonitorUi;

/// One APC on the console's powernet.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PowerMonitorArea {
    pub name: String,
    /// Cell percent, 0 without a cell.
    pub charge: f32,
    /// `display_power(lastused_total)`.
    pub load: String,
    /// 0 not charging, 1 charging, 2 full.
    pub charging: u8,
    /// `APC_CHANNEL_*`: bit 2 on, bit 1 auto.
    pub eqp: u8,
    pub lgt: u8,
    pub env: u8,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PowerMonitorData {
    /// `record_size`, `record_interval` in seconds.
    pub stored: u32,
    pub interval: f32,
    pub attached: bool,
    /// `history["supply"]`, `history["demand"]`, watts.
    pub history_supply: Vec<f32>,
    pub history_demand: Vec<f32>,
    pub areas: Vec<PowerMonitorArea>,
}

impl UiInterface for PowerMonitorUi {
    const NAME: &'static str = "PowerMonitor";
    type Data = PowerMonitorData;
    type StaticData = ();
}

/// `PEAK_DRAW`: the bars' and chart's floor for the maximum.
pub const PEAK_DRAW: f32 = 500_000.0;

/// `AreaStatusColorBox`: on or off, `M` for a manual channel.
pub fn channel_status(status: u8) -> (bool, &'static str) {
    let power = status & 2 != 0;
    let auto = status & 1 != 0;
    (power, if auto { "" } else { "M" })
}

#[cfg(feature = "client")]
mod client_side {
    use ss13_ui_kit::prelude::*;

    use super::{PEAK_DRAW, PowerMonitorArea, PowerMonitorData, PowerMonitorUi, channel_status};
    use crate::client::ClientUi;

    fn status_cell(status: u8) -> impl Scene + use<> {
        let (power, label) = channel_status(status);
        let color = if power { UiColor::Good } else { UiColor::Bad };
        let content = format!("■{label}");
        bsn! { @TableCell { @collapsing: true, @content: content, @color: color } }
    }

    fn row(area: &PowerMonitorArea) -> impl Scene + use<> {
        let name = area.name.clone();
        let charge_color = match area.charging {
            0 if area.charge > 50.0 => UiColor::Yellow,
            0 => UiColor::Red,
            1 => UiColor::Yellow,
            _ => UiColor::Green,
        };
        let charge = format!("{:.0}%", area.charge);
        let load = area.load.clone();
        let cells: Vec<_> = [area.eqp, area.lgt, area.env]
            .into_iter()
            .map(status_cell)
            .collect();
        bsn! {
            @TableRow
            Children [
                @TableCell { @content: name }
                --
                @TableCell { @collapsing: true, @content: charge, @color: charge_color }
                --
                @TableCell { @collapsing: true, @content: load }
                --
                {cells}
            ]
        }
    }

    impl ClientUi for PowerMonitorUi {
        const WIDTH: f32 = 550.0;
        const HEIGHT: f32 = 700.0;

        // shortcut: no `Chart.Line` in ss13_ui_kit yet (the chart box is drawn empty, as for
        // `SolarControl`) and no client-side sort buttons; the areas come in powernet order.
        fn content(_: &(), data: &PowerMonitorData) -> impl Scene {
            let supply = data.history_supply.last().copied().unwrap_or(0.0);
            let demand = data.history_demand.last().copied().unwrap_or(0.0);
            let max = data
                .history_supply
                .iter()
                .chain(&data.history_demand)
                .copied()
                .fold(PEAK_DRAW, f32::max);
            let supply_text = format!("{:.0} kW", supply / 1000.0);
            let demand_text = format!("{:.0} kW", demand / 1000.0);
            let loading = data.history_supply.is_empty() || data.history_demand.is_empty();
            let shown = |on: bool| if on { Display::Flex } else { Display::None };
            let (body, wait) = (shown(!loading), shown(loading));
            let rows: Vec<_> = data.areas.iter().map(row).collect();
            bsn! {
                @WindowContent
                Children [
                    Node { display: wait }
                    Children [ Text("Loading...") ]
                    --
                    Node { display: body, flex_direction: FlexDirection::Column }
                    Children [
                        @Section
                        Children [
                            Node { height: px(64), margin: UiRect::bottom(px(6)) }
                            BackgroundColor(Color::BLACK)
                            --
                            @LabeledList
                            Children [
                                @LabeledListItem { @label: "Supply" }
                                Children [
                                    @ProgressBar { @empty: true }
                                    ProgressBar { value: supply, max_value: max, color: UiColor::Teal }
                                    Children [ Text(supply_text) ]
                                ]
                                --
                                @LabeledListItem { @label: "Draw" }
                                Children [
                                    @ProgressBar { @empty: true }
                                    ProgressBar { value: demand, max_value: max, color: UiColor::Pink }
                                    Children [ Text(demand_text) ]
                                ]
                            ]
                        ]
                        --
                        @Section
                        Children [
                            @Table
                            Children [
                                @TableRow { @header: true }
                                Children [
                                    @TableCell { @header: true, @content: "Area" }
                                    --
                                    @TableCell { @header: true, @collapsing: true, @content: "Charge" }
                                    --
                                    @TableCell { @header: true, @collapsing: true, @content: "Draw" }
                                    --
                                    @TableCell { @header: true, @collapsing: true, @content: "Eqp" }
                                    --
                                    @TableCell { @header: true, @collapsing: true, @content: "Lgt" }
                                    --
                                    @TableCell { @header: true, @collapsing: true, @content: "Env" }
                                ]
                                --
                                {rows}
                            ]
                        ]
                    ]
                ]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_round_trips_and_channels_read_like_tgui() {
        let data = PowerMonitorData {
            stored: 60,
            interval: 5.0,
            attached: true,
            history_supply: vec![1.0e5, 2.0e5],
            history_demand: vec![5.0e4, 6.0e4],
            areas: vec![PowerMonitorArea {
                name: "Bridge".into(),
                charge: 99.0,
                load: "1.20 kW".into(),
                charging: 2,
                eqp: 3,
                lgt: 2,
                env: 0,
            }],
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<PowerMonitorData>(&bytes).unwrap(), data);
        assert_eq!(channel_status(3), (true, ""));
        assert_eq!(channel_status(2), (true, "M"));
        assert_eq!(channel_status(1), (false, ""));
    }
}
