//! `DestinationTagger` (destination tagger): `tgui/packages/tgui/interfaces/DestinationTagger.tsx`
//! and `/obj/item/dest_tagger/ui_data` (`code/modules/recycling/sortingmachinery.dm`). The server
//! half lives with the game (`ss13_game::disposals`).

use serde::{Deserialize, Serialize};

use crate::UiInterface;

pub struct DestinationTaggerUi;

/// `ui_data` `locations`: `GLOB.TAGGERLOCATIONS` never changes, so it is static data here.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DestinationTaggerStatic {
    pub locations: Vec<String>,
}

/// `ui_data` `currentTag`: 1-based, 0 = none.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DestinationTaggerData {
    pub current_tag: u8,
}

impl UiInterface for DestinationTaggerUi {
    const NAME: &'static str = "DestinationTagger";
    type Data = DestinationTaggerData;
    type StaticData = DestinationTaggerStatic;
}

/// `sortDestinations`: upper-cased names in alphabetical order, each with its 1-based tag.
pub fn sorted_destinations(locations: &[String]) -> Vec<(String, u8)> {
    let mut out: Vec<(String, u8)> = locations
        .iter()
        .enumerate()
        .map(|(i, name)| (name.to_uppercase(), i as u8 + 1))
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The section title.
pub fn title(locations: &[String], current: u8) -> String {
    match current
        .checked_sub(1)
        .and_then(|i| locations.get(usize::from(i)))
    {
        Some(name) => format!("Current Destination: {name}"),
        None => "Please Select A Location".to_owned(),
    }
}

#[cfg(feature = "client")]
mod client_side {
    use ss13_ui_kit::ActParams;
    use ss13_ui_kit::prelude::*;

    use super::{
        DestinationTaggerData, DestinationTaggerStatic, DestinationTaggerUi, sorted_destinations,
        title,
    };
    use crate::client::ClientUi;

    impl ClientUi for DestinationTaggerUi {
        const WIDTH: f32 = 420.0;
        const HEIGHT: f32 = 500.0;

        // shortcut: no `theme="retro"` (ss13_ui_kit has one theme).
        fn content(s: &DestinationTaggerStatic, data: &DestinationTaggerData) -> impl Scene {
            let heading = title(&s.locations, data.current_tag);
            let buttons: Vec<_> = sorted_destinations(&s.locations)
                .into_iter()
                .map(|(name, tag)| {
                    // `Button.Checkbox`: a checked or empty square icon.
                    let icon = if tag == data.current_tag {
                        "check-square-o"
                    } else {
                        "square-o"
                    };
                    let params = ActParams(vec![("index".into(), tag.to_string())]);
                    bsn! {
                        Node { width: px(180), height: px(40) }
                        Children [
                            @Button { @icon: icon, @content: name, @fluid: true }
                            Act("change")
                            params
                        ]
                    }
                })
                .collect();
            bsn! {
                @WindowContent
                Children [
                    @Stack { @vertical: true, @fill: true }
                    Children [
                        @StackItem { @grow: 1.0 }
                        Children [
                            @Section { @title: heading, @fill: true, @scrollable: true }
                            Children [
                                Node { flex_wrap: FlexWrap::Wrap }
                                Children [ {buttons} ]
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
    fn destinations_sort_by_name_and_keep_their_tag() {
        let locations: Vec<String> = ["Disposals", "Cargo Bay", "Medbay"]
            .map(String::from)
            .into();
        assert_eq!(
            sorted_destinations(&locations),
            vec![
                ("CARGO BAY".into(), 2),
                ("DISPOSALS".into(), 1),
                ("MEDBAY".into(), 3)
            ]
        );
        assert_eq!(title(&locations, 0), "Please Select A Location");
        assert_eq!(title(&locations, 3), "Current Destination: Medbay");
    }
}
