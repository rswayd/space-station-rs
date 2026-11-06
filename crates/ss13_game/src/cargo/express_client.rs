//! The `CargoExpress` window (client): tgui `CargoExpress.tsx` with the `Cargo` window's
//! `CargoCatalog` (its `express` form, [`super::client::catalog_view`]). The data is
//! [`super::CargoExpressData`], the server half [`super::express`].
// shortcut: the lock screen is a text without the big lock icon; points are not animated.

use bevy::app::Propagate;
use bevy::scene::EntityScene;
use ss13_tgui::client::{ClientUi, UiLocalState};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::theme::Bold;

use super::client::{catalog_view, format_money, modal};
use super::*;

impl ClientUi for CargoExpressUi {
    const WIDTH: f32 = 600.0;
    const HEIGHT: f32 = 700.0;

    fn content(st: &CargoStatic, data: &CargoExpressData) -> impl Scene {
        view(st, data, &UiLocalState::default())
    }

    fn content_local(
        st: &CargoStatic,
        data: &CargoExpressData,
        local: &UiLocalState,
    ) -> impl Scene {
        view(st, data, local)
    }
}

fn view(st: &CargoStatic, data: &CargoExpressData, local: &UiLocalState) -> impl Scene + use<> {
    let body: Box<dyn SceneList> = if data.locked {
        Box::new(EntityScene(locked_view()))
    } else {
        Box::new(EntityScene(unlocked_view(st, data, local)))
    };
    bsn! { @WindowContent Children [ {body} ] }
}

fn locked_view() -> impl Scene {
    let red = UiColor::Red.text().unwrap_or_default();
    bsn! {
        @Section { @fill: true }
        Children [
            Node {
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
            }
            Propagate::<Bold>(Bold(true))
            Propagate::<TextColor>(TextColor(red))
            Children [
                @Icon { @name: "lock", @size: 5.0, @color: UiColor::Red }
                --
                Text("Swipe a Cargo Technician-level ID card to unlock this interface.")
            ]
        ]
    }
}

fn unlocked_view(
    st: &CargoStatic,
    data: &CargoExpressData,
    local: &UiLocalState,
) -> impl Scene + use<> {
    // The catalogue reads the cart and the private flag of the `Cargo` window's data: the
    // express console has neither.
    let cargo = CargoData::default();
    let notice_color = if data.beacon_error || !data.can_beacon {
        UiColor::Red
    } else {
        UiColor::Blue
    };
    let message = data.message.clone();
    let points = format!("{} credits", format_money(data.points));
    let (cargo_color, beacon_color) = if data.using_beacon {
        (UiColor::Default, UiColor::Green)
    } else {
        (UiColor::Green, UiColor::Default)
    };
    let beacon_name = data.beacon_name.clone();
    let beacon_zone = data.beaconzone.clone();
    let print_msg = data.print_msg.clone();
    let no_beacon = !data.has_beacon;
    let cant_print = !data.can_buy_beacon;
    let catalog = EntityScene(catalog_view(st, &cargo, local, true));
    let info = modal(st, &cargo, local);
    let header: Box<dyn SceneList> = Box::new(EntityScene(bsn! {
        Node Propagate::<Bold>(Bold(true)) Children [ Text(points) Name("express: points") ]
    }));
    bsn! {
        Node { width: percent(100), height: percent(100) }
        Children [
            @Stack { @vertical: true, @fill: true }
            Children [
                @StackItem
                Children [ @NoticeBox { @color: notice_color } Children [ Text(message) ] ]
                --
                @StackItem
                Children [
                    @Section { @title: "Cargo Express", @buttons: header }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Landing Location" }
                            Children [
                                Node { flex_direction: FlexDirection::Row }
                                Children [
                                    @ss13_ui_kit::Button { @content: "Cargo Bay" }
                                    ss13_ui_kit::Button { color: cargo_color }
                                    Act({LZ_CARGO.to_owned()})
                                    --
                                    @ss13_ui_kit::Button {
                                        @content: beacon_name,
                                        @tooltip: beacon_zone,
                                    }
                                    ss13_ui_kit::Button { color: beacon_color, disabled: no_beacon }
                                    Act({LZ_BEACON.to_owned()})
                                    --
                                    @ss13_ui_kit::Button { @content: print_msg }
                                    ss13_ui_kit::Button { disabled: cant_print }
                                    Act({PRINT_BEACON.to_owned()})
                                ]
                            ]
                        ]
                    ]
                ]
                --
                @StackItem { @grow: 1.0 }
                Node { min_height: px(0) }
                Children [ {catalog} ]
            ]
            --
            {info}
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_and_unlocked_views_build() {
        let st = CargoStatic::default();
        let mut data = CargoExpressData {
            locked: true,
            ..default()
        };
        let _ = view(&st, &data, &UiLocalState::default());
        data.locked = false;
        let _ = view(&st, &data, &UiLocalState::default());
    }
}
