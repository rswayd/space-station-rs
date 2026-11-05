//! The radio window: tgui `Radio` (`tgui/packages/tgui/interfaces/Radio.tsx`) and
//! `/obj/item/radio/ui_data`: the frequency with its tuner, the microphone and speaker switches
//! and the key channels. The server half is in `crate::radio`.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

pub struct RadioUi;

impl UiInterface for RadioUi {
    const NAME: &'static str = "Radio";
    type Data = RadioData;
    type StaticData = ();
}

/// `/obj/item/radio/ui_data`, without the loud mode and subspace switch (no radio here has
/// them).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RadioData {
    pub broadcasting: bool,
    pub listening: bool,
    /// Deci-kHz, as tg stores it (1459 is 145.9).
    pub frequency: u16,
    pub min_frequency: u16,
    pub max_frequency: u16,
    pub freqlock: bool,
    /// The key channels and whether each is listened to.
    pub channels: Vec<(String, bool)>,
    pub headset: bool,
}

pub const FREQUENCY: &str = "frequency";
pub const TUNE_TO_CHANNEL: &str = "tune_to_channel";
pub const LISTEN: &str = "listen";
pub const BROADCAST: &str = "broadcast";
pub const CHANNEL: &str = "channel";
/// Params of [`FREQUENCY`]: `adjust` steps (±1 = ±0.1, ±0.2...: tg `adjust * 10` deci-units on
/// a kHz value), `tune` a frequency in kHz.
pub const ADJUST: &str = "adjust";
pub const TUNE: &str = "tune";

/// `format_frequency`: `145.9`.
pub fn format_frequency(freq: u16) -> String {
    format!("{}.{}", freq / 10, freq % 10)
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_tgui::client::ClientUi;
    use ss13_ui_kit::prelude::*;

    use super::*;

    impl ClientUi for RadioUi {
        const WIDTH: f32 = 360.0;
        const HEIGHT: f32 = 250.0;

        fn content(_: &(), data: &RadioData) -> impl Scene {
            view(data)
        }
    }

    fn act(
        icon: &'static str,
        content: String,
        action: &str,
        params: Vec<(String, String)>,
        disabled: bool,
        selected: bool,
    ) -> impl Scene + use<> {
        let action = action.to_owned();
        let params = ActParams(params);
        bsn! {
            @ss13_ui_kit::Button { @icon: icon, @content: content }
            ss13_ui_kit::Button { disabled: disabled, selected: selected }
            Act(action)
            params
        }
    }

    fn view(data: &RadioData) -> impl Scene + use<> {
        let freq = format!("Frequency: {}", format_frequency(data.frequency));
        let tuner: Vec<_> = ["-1", "-0.2", "+0.2", "+1"]
            .iter()
            .map(|step| {
                let adjust = step.trim_start_matches('+').to_owned();
                act(
                    "",
                    (*step).to_owned(),
                    FREQUENCY,
                    vec![(ADJUST.to_owned(), adjust)],
                    data.freqlock,
                    false,
                )
            })
            .collect();
        let mic = data.broadcasting;
        let speaker = data.listening;
        let mic_label = if mic {
            "Microphone: on"
        } else {
            "Microphone: off"
        };
        let speaker_label = if speaker {
            "Speaker: on"
        } else {
            "Speaker: off"
        };
        let mic = EntityScene(act(
            "microphone",
            mic_label.to_owned(),
            BROADCAST,
            Vec::new(),
            false,
            mic,
        ));
        let speaker = EntityScene(act(
            "volume-up",
            speaker_label.to_owned(),
            LISTEN,
            Vec::new(),
            false,
            speaker,
        ));
        let channels: Vec<_> = data
            .channels
            .iter()
            .map(|(name, on)| {
                let icon = if *on { "check-square-o" } else { "square-o" };
                act(
                    icon,
                    name.clone(),
                    CHANNEL,
                    vec![(CHANNEL.to_owned(), name.clone())],
                    false,
                    *on,
                )
            })
            .collect();
        let has_channels = !data.channels.is_empty();
        let channel_section = has_channels.then(|| {
            EntityScene(bsn! {
                @Section { @title: "Channels" }
                Children [
                    Node { flex_direction: FlexDirection::Column, row_gap: px(2) }
                    Children [ {channels} ]
                ]
            })
        });
        bsn! {
            @WindowContent
            Children [
                @Section
                Children [
                    Node Children [ Text(freq) ]
                    --
                    Node { flex_direction: FlexDirection::Row, margin: UiRect::top(px(4)) }
                    Children [ {tuner} ]
                    --
                    Node { flex_direction: FlexDirection::Row, margin: UiRect::top(px(4)) }
                    Children [ {mic} -- {speaker} ]
                ]
                --
                {channel_section}
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequencies_print_like_tg() {
        assert_eq!(format_frequency(1459), "145.9");
        assert_eq!(format_frequency(1351), "135.1");
    }
}
