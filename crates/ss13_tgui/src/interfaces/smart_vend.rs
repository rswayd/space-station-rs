//! `SmartVend` (smart fridges): `tgui/packages/tgui/interfaces/SmartVend.tsx` and
//! `/obj/machinery/smartfridge/ui_data` (`code/modules/food_and_drinks/machinery/
//! smartfridge.dm`). The server half lives with the game (`ss13_game::smartfridge`).
// shortcut: the list layout only, without the search box or the layout toggle; the amount
// `NumberInput` and the right click "vend all" are one "all" button on each row.

use serde::{Deserialize, Serialize};

use crate::UiInterface;
use crate::interfaces::vending::ProductIcon;

pub struct SmartVendUi;

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SmartVendData {
    /// Sorted by `path`.
    pub contents: Vec<FridgeItem>,
    pub name: String,
    pub isdryer: bool,
    pub drying: bool,
}

/// One `contents` entry: the items of one type and name.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct FridgeItem {
    /// `"[type]-[name]"`, what `Release` sends back.
    pub path: String,
    pub name: String,
    pub amount: u32,
    pub icon: Option<ProductIcon>,
}

impl UiInterface for SmartVendUi {
    const NAME: &'static str = "SmartVend";
    type Data = SmartVendData;
    type StaticData = ();
}

#[cfg(feature = "client")]
mod client_side {
    use bevy::scene::EntityScene;
    use ss13_ui_kit::prelude::*;

    use super::{FridgeItem, SmartVendData, SmartVendUi};
    use crate::client::{ClientUi, UiSprite};

    impl ClientUi for SmartVendUi {
        const WIDTH: f32 = 431.0;
        const HEIGHT: f32 = 575.0;

        fn content(_: &(), data: &SmartVendData) -> impl Scene {
            let rows: Vec<Box<dyn Scene>> = if data.contents.is_empty() {
                vec![Box::new(bsn! {
                    @NoticeBox
                    Children [ Text("Nothing found.") ]
                })]
            } else {
                data.contents.iter().map(row).collect()
            };
            let help: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
                @Button {
                    @icon: "question",
                    @tooltip: "LMB - Vend one\nAll - Vend all",
                    @tooltip_position: Placement::BottomEnd,
                }
            }));
            bsn! {
                @WindowContent
                Children [
                    @Section {
                        @title: "Storage",
                        @fill: true,
                        @scrollable: true,
                        @buttons: {Some(help)},
                    }
                    Children [ {rows} ]
                ]
            }
        }
    }

    /// `ItemList`.
    fn row(item: &FridgeItem) -> Box<dyn Scene> {
        let image: Box<dyn SceneList> = match item.icon {
            Some(icon) => Box::new(EntityScene(bsn! {
                Node { width: percent(100), height: percent(100) }
                UiSprite { icon_state: {Some(icon.icon_state)}, color: {icon.color} }
            })),
            None => Box::new(EntityScene(bsn! {
                Node {
                    width: percent(100),
                    height: percent(100),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                }
                Children [ @Icon { @name: "question", @size: 2.0 } ]
            })),
        };
        let all = item.amount;
        let few = all <= 1;
        let all_params = ActParams(vec![
            ("path".into(), item.path.clone()),
            ("amount".into(), all.to_string()),
        ]);
        let all_button: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
            @Button { @content: "All", @compact: true }
            Button { disabled: few }
            Act("Release")
            all_params
        }));
        let one = ActParams(vec![
            ("path".into(), item.path.clone()),
            ("amount".into(), "1".into()),
        ]);
        let (name, count) = (item.name.clone(), format!("x{}", item.amount));
        let disabled = item.amount < 1;
        Box::new(bsn! {
            @ImageButton { @image: image, @fluid: true, @image_size: 32.0, @buttons: {Some(all_button)} }
            ImageButton { disabled: disabled }
            Act("Release")
            one
            Children [
                @Stack
                Children [
                    @StackItem { @grow: 1.0 }
                    Children [ Text(name) ]
                    --
                    @StackItem
                    Children [ Text(count) TextColor({UiColor::Label.base()}) ]
                ]
            ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_round_trips() {
        let data = SmartVendData {
            contents: vec![FridgeItem {
                path: "/obj/item/food/grown/apple-Apple".into(),
                name: "Apple".into(),
                amount: 3,
                icon: None,
            }],
            name: "smartfridge".into(),
            ..Default::default()
        };
        let bytes = crate::encode(&data);
        assert_eq!(crate::decode::<SmartVendData>(&bytes).unwrap(), data);
    }
}
