//! `RadialMenu`: the data of tg `/datum/radial_menu` (`code/_onclick/hud/screen_objects/radial.dm`).
//! Headless: `ss13_game::radial_client` draws it around its anchor; the server half is
//! [`crate::prompt::PromptKind::Radial`] and `ss13_game::radial`.
//!
//! Acts as every modal: `submit` with `entry` (one [`RadialSlice::key`]), `cancel`.

use serde::{Deserialize, Serialize};
use ss13_shared::IconStateId;
use ss13_shared::protocol::NetId;

use crate::UiInterface;

pub struct RadialUi;

impl UiInterface for RadialUi {
    const NAME: &'static str = "RadialMenu";
    type Data = RadialData;
    type StaticData = ();
}

/// One choice as the client draws it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct RadialSlice {
    /// The `submit` act's `entry`.
    pub key: String,
    pub name: String,
    pub icon: Option<IconStateId>,
}

/// `icons/hud/radial.dmi`'s frame sprites; a missing one is drawn as a plain disc.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RadialFrame {
    pub slice: Option<IconStateId>,
    pub center: Option<IconStateId>,
    pub next: Option<IconStateId>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct RadialData {
    /// `None`: not replicated to this client any more; drawn over the own mob.
    pub anchor: Option<NetId>,
    pub items: Vec<RadialSlice>,
    pub frame: RadialFrame,
}

/// tg `min_angle = 45`: at most 8 slices on a page.
pub const MAX_SLICES: usize = 8;

/// One slot of a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadialSlot {
    /// Index into [`RadialData::items`].
    Item(usize),
    /// tg `NEXT_PAGE_ID`.
    Next,
}

/// tg `setup_menu`: the pages of `count` choices, the last slot of a full page turning it.
pub fn radial_pages(count: usize) -> Vec<Vec<RadialSlot>> {
    let paged = MAX_SLICES < count;
    let mut pages = Vec::new();
    let mut current = Vec::new();
    let mut next = 0;
    while next < count {
        if current.len() == MAX_SLICES {
            pages.push(std::mem::take(&mut current));
        }
        if paged && current.len() == MAX_SLICES - 1 {
            current.push(RadialSlot::Next);
            continue;
        }
        current.push(RadialSlot::Item(next));
        next += 1;
    }
    if paged && current.len() < MAX_SLICES {
        current.push(RadialSlot::Next);
    }
    pages.push(current);
    pages
}

/// tg `SetElement` / `update_screen_objects`: pixels from the anchor (x east, y north) of slot
/// `i` of a page with `len` slots, `radius` pixels out.
pub fn radial_offset(i: usize, len: usize, radius: f32) -> (f32, f32) {
    let per = (360.0 / len.max(1) as f32).round();
    let angle = (i as f32 * per).to_radians();
    (
        (angle.sin() * radius).round(),
        (angle.cos() * radius).round(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_lists_are_one_page_long_ones_get_next_slots() {
        assert_eq!(radial_pages(3).len(), 1);
        assert_eq!(radial_pages(8)[0].len(), 8);
        assert!(!radial_pages(8)[0].contains(&RadialSlot::Next));
        let pages = radial_pages(9);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].last(), Some(&RadialSlot::Next));
        assert_eq!(pages[1].last(), Some(&RadialSlot::Next));
        let items: usize = pages
            .iter()
            .flatten()
            .filter(|s| matches!(s, RadialSlot::Item(_)))
            .count();
        assert_eq!(items, 9);
    }

    #[test]
    fn slices_start_north_and_go_clockwise() {
        assert_eq!(radial_offset(0, 4, 32.0), (0.0, 32.0));
        assert_eq!(radial_offset(1, 4, 32.0), (32.0, 0.0));
        assert_eq!(radial_offset(2, 4, 32.0), (0.0, -32.0));
    }
}
