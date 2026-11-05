//! The space wizard (tg `/datum/antagonist/wizard`, `code/modules/antagonists/wizard/`): a
//! Wizard Federation magician who starts in its den, buys spells and artefacts from its
//! spellbook and teleports onto the station. [`server`] is the antagonist, its den, book and
//! apprentices; [`client`] draws the spellbook; the spells are `crate::spells`.
//!
//! - `Spellbook` (`Spellbook/index.tsx`): the entries by category with their cost, cooldown and
//!   whether they need the robes; `act('purchase', { spellref })`, `act('refund', { spellref })`.
//! - The info panel is tg's default `AntagInfoGeneric` (name and objectives).
//! - The teleport destinations, the apprentice's school and the wizard's name are tgui prompts.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;

/// tg `ROLE_WIZARD`, `ROLE_WIZARD_MIDROUND`: the `be_special` keys of the roundstart and the
/// midround wizard (the midround one also answers apprentice contracts).
pub const ROLE_WIZARD: &str = "Wizard";
pub const ROLE_WIZARD_MIDROUND: &str = "Wizard (Midround)";

/// `act('purchase', { spellref })`, `act('refund', { spellref })`.
pub const PURCHASE: &str = "purchase";
pub const REFUND: &str = "refund";

/// The wizard's spellbook.
pub struct SpellbookUi;

impl UiInterface for SpellbookUi {
    const NAME: &'static str = "Spellbook";
    type Data = SpellbookData;
    type StaticData = SpellbookStatic;
}

/// One `/datum/spellbook_entry` (`ui_static_data` `entries`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SpellbookEntry {
    /// The `spellref` of its acts.
    pub id: String,
    pub name: String,
    pub desc: String,
    /// `cat`.
    pub category: String,
    pub cost: u32,
    pub refundable: bool,
    pub buyword: String,
    pub requires_wizard_garb: bool,
    /// Purchases allowed (`limit`); `None`: no limit.
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SpellbookStatic {
    pub entries: Vec<SpellbookEntry>,
    /// The categories in the book's order.
    pub categories: Vec<String>,
}

/// `ui_data`, with the entries' `times` and `cooldown` (tg sends them as static data).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SpellbookData {
    pub owner: String,
    pub points: u32,
    /// Per entry, in [`SpellbookStatic::entries`] order.
    pub times: Vec<u32>,
    /// A spell's cooldown at the caster's level, seconds.
    pub cooldowns: Vec<Option<f32>>,
}

impl SpellbookData {
    /// `SpellTabDisplay`'s buy button: enough points and under the limit.
    pub fn can_buy(&self, i: usize, entry: &SpellbookEntry) -> bool {
        let times = self.times.get(i).copied().unwrap_or(0);
        self.points >= entry.cost && entry.limit.is_none_or(|l| times < l)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buying_needs_points_and_room() {
        let entry = SpellbookEntry {
            cost: 2,
            limit: Some(1),
            ..Default::default()
        };
        let mut data = SpellbookData {
            points: 2,
            times: vec![0],
            ..Default::default()
        };
        assert!(data.can_buy(0, &entry));
        data.times[0] = 1;
        assert!(!data.can_buy(0, &entry));
        data.times[0] = 0;
        data.points = 1;
        assert!(!data.can_buy(0, &entry));
    }
}
