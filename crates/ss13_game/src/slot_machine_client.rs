//! The slot machine window (client): tgui `SlotMachine.tsx`. The data is `crate::slot_machine`,
//! the server half `crate::service::slots`.
// shortcut: no spinning reel animation (the reels show "Spinning..." until the result) and no
// rotating banner pages; the banner shows the win or the current jackpot.

use bevy::scene::EntityScene;
use ss13_tgui::client::{AppTguiClientExt, ClientUi, UiSprite};
use ss13_ui_kit::prelude::*;

use crate::slot_machine::{
    PAYOUT, SPIN, SlotMachineData, SlotMachineStatic, SlotMachineUi, SlotSymbol, winning_text,
};

pub struct SlotMachineClientPlugin;

impl Plugin for SlotMachineClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<SlotMachineUi>();
    }
}

impl ClientUi for SlotMachineUi {
    const WIDTH: f32 = 300.0;
    const HEIGHT: f32 = 445.0;

    fn content(statics: &SlotMachineStatic, data: &SlotMachineData) -> impl Scene {
        view(statics, data)
    }
}

/// A 32x32 symbol: its sprite, else its name.
fn symbol(statics: &SlotMachineStatic, id: &str) -> impl Scene + use<> {
    let found: Option<SlotSymbol> = statics.symbols.iter().find(|s| s.id == id).cloned();
    let sprite = found.as_ref().and_then(|s| s.icon).map(|icon| {
        EntityScene(bsn! {
            Node { width: px(32), height: px(32) }
            UiSprite { icon_state: {Some(icon.icon_state)}, color: {icon.color} }
        })
    });
    let label = match (&found, &sprite) {
        (Some(s), None) => {
            let name = s.name.clone();
            Some(EntityScene(bsn! {
                Node { width: px(32), height: px(32), overflow: Overflow::clip() }
                Children [ Text(name) ]
            }))
        }
        _ => None,
    };
    bsn! {
        Node { margin: UiRect::all(px(2)) }
        Children [ {sprite} -- {label} ]
    }
}

fn view(statics: &SlotMachineStatic, data: &SlotMachineData) -> impl Scene + use<> {
    let banner = match winning_text(data.winning) {
        Some(text) => text.to_owned(),
        None => format!(
            "Current jackpot: {} credits! {} plays, {} jackpots!",
            data.money + statics.jackpot,
            data.plays,
            data.jackpots
        ),
    };
    let spinning = data.working.then(|| {
        EntityScene(bsn! {
            Node { height: px(108), justify_content: JustifyContent::Center, align_items: AlignItems::Center }
            Children [ Text({"Spinning...".to_owned()}) ]
        })
    });
    let reels: Vec<_> = data
        .reels
        .iter()
        .filter(|_| !data.working)
        .map(|reel| {
            let cells: Vec<_> = reel.iter().map(|id| symbol(statics, id)).collect();
            bsn! {
                Node { flex_direction: FlexDirection::Column }
                Children [ {cells} ]
            }
        })
        .collect();
    let jackpot = EntityScene(symbol(statics, &statics.jackpot_id));
    let trap = statics.trap_id.as_deref().map(|id| {
        let icon = EntityScene(symbol(statics, id));
        EntityScene(bsn! {
            Node { align_items: AlignItems::Center, margin: UiRect::left(px(20)) }
            Children [ Text({"Trap:".to_owned()}) -- {icon} ]
        })
    });
    let balance = format!("{} cr", data.balance);
    let empty = data.balance == 0;
    let refund: Box<dyn bevy::scene::SceneList> = Box::new(EntityScene(bsn! {
        @ss13_ui_kit::Button { @content: "Refund" }
        ss13_ui_kit::Button { disabled: empty }
        Act({PAYOUT.to_owned()})
    }));
    let cant_spin = data.working || data.balance < statics.cost;
    bsn! {
        @WindowContent
        Children [
            @Section
            Children [ Text(banner) ]
            --
            @Section
            Children [
                Node { justify_content: JustifyContent::Center }
                Children [ {spinning} -- {reels} ]
            ]
            --
            @Section
            Children [
                Node { justify_content: JustifyContent::Center, align_items: AlignItems::Center }
                Children [ Text({"Jackpot:".to_owned()}) -- {jackpot} -- {trap} ]
            ]
            --
            @Section { @title: "Balance", @buttons: refund }
            Children [ Text(balance) ]
            --
            @Section
            Children [
                @ss13_ui_kit::Button { @content: "Spin!", @fluid: true }
                ss13_ui_kit::Button { color: UiColor::Good, disabled: cant_spin }
                Act({SPIN.to_owned()})
            ]
        ]
    }
}
