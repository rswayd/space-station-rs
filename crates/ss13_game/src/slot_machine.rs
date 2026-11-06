//! The slot machine's window data, shared by both halves: tgui `SlotMachine`
//! (`tgui/packages/tgui/interfaces/SlotMachine.tsx`). The server half is
//! `crate::service::slots`, the client view `crate::slot_machine_client`.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::vending::ProductIcon;

pub struct SlotMachineUi;

impl UiInterface for SlotMachineUi {
    const NAME: &'static str = "SlotMachine";
    type Data = SlotMachineData;
    type StaticData = SlotMachineStatic;
}

/// One reel symbol (`symbol_data`: `id`, and the sprite instead of tg's spritesheet class).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SlotSymbol {
    /// The symbol's type path.
    pub id: String,
    /// Shown when there is no sprite.
    pub name: String,
    pub icon: Option<ProductIcon>,
}

/// `ui_static_data`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SlotMachineStatic {
    pub symbols: Vec<SlotSymbol>,
    /// `SPIN_PRICE`.
    pub cost: u32,
    /// `PRIZE_JACKPOT`.
    pub jackpot: u32,
    pub jackpot_id: String,
    pub trap_id: Option<String>,
}

/// `ui_data`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotMachineData {
    /// Five reels of three symbol ids, top to bottom.
    pub reels: Vec<Vec<String>>,
    pub balance: u32,
    pub working: bool,
    /// `WINNING_*`: 0 nothing, 1 free spins, 2 prize, 3 big prize, 4 jackpot.
    pub winning: u8,
    pub money: u32,
    pub plays: u32,
    pub jackpots: u32,
    /// `HOLOCHIP` (1) or `COIN` (2).
    pub paymode: u8,
}

pub const SPIN: &str = "spin";
pub const PAYOUT: &str = "payout";

/// `WINNING_TEXTS`.
pub fn winning_text(winning: u8) -> Option<&'static str> {
    match winning {
        1 => Some("FREE SPINS!"),
        2 => Some("PRIZE!"),
        3 => Some("BIG PRIZE!"),
        4 => Some("JACKPOT!!!"),
        _ => None,
    }
}
