//! The strip menu and loot panel windows (client): tgui `StripMenu.tsx` and `LootPanel/`
//! (`index.tsx`, `GroupedContents.tsx`, `LootBox.tsx`). The data is [`crate::panels`], the server
//! halves `crate::strip` and `crate::loot`.
// shortcut: the strip menu's empty slots show the HUD style's slot sprites (tg: the
// `icons/ui/inventory/*.png` assets, which are not in the atlas); the loot panel has no search
// box or grouping toggle, and its entries click without the modifiers held (tg passes
// ctrl/shift/alt and right clicks along).

use std::collections::HashMap;

use bevy::scene::EntityScene;
use ss13_render::IconAtlas;
use ss13_shared::IconStateId;
use ss13_tgui::client::{AppTguiClientExt, ClientUi, UiSprite};
use ss13_ui_kit::prelude::*;

use crate::panels::{
    LOOT_GRAB, LootEntry, LootPanelData, LootPanelUi, OBSCURING_COMPLETELY, OBSCURING_HIDDEN,
    OBSCURING_INACCESSIBLE, STRIP_USE, StripMenuData, StripMenuUi, StripSlot,
};

/// The HUD style the slot sprites come from (`crate::inventory_client`).
const STYLE_DMI: &str = "icons/hud/screen_midnight.dmi";
const BUTTON_PX: f32 = 64.0;
const GAP_PX: f32 = 6.0;
const ROWS: usize = 5;
const COLUMNS: usize = 6;

pub struct PanelsClientPlugin;

impl Plugin for PanelsClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<StripMenuUi>()
            .register_ui_view::<LootPanelUi>()
            .add_systems(PostUpdate, resolve_style_sprites);
    }
}

/// A sprite of the HUD style `.dmi` inside a window, by state name; becomes a [`UiSprite`].
#[derive(Component, Clone, Debug, Default, PartialEq)]
#[require(Node)]
pub struct StyleSprite {
    pub state: String,
    pub color: Option<[u8; 4]>,
}

fn resolve_style_sprites(
    sprites: Query<(Entity, &StyleSprite), Added<StyleSprite>>,
    atlas: Option<Res<IconAtlas>>,
    mut states: Local<HashMap<String, IconStateId>>,
    mut commands: Commands,
) {
    let Some(atlas) = atlas else {
        return;
    };
    for (entity, sprite) in &sprites {
        if states.is_empty() {
            *states = atlas
                .manifest
                .states
                .iter()
                .enumerate()
                .filter(|(_, s)| s.dmi == STYLE_DMI)
                .map(|(i, s)| (s.state.clone(), IconStateId(i as u32)))
                .collect();
        }
        if let Some(&id) = states.get(&sprite.state) {
            commands.entity(entity).insert(UiSprite {
                icon_state: Some(id),
                color: sprite.color,
                dir: None,
            });
        }
    }
}

/// `SLOTS` of `StripMenu.tsx`: tooltip name, grid spot, slot picture (a HUD style state here),
/// corner text.
struct SlotLook {
    key: &'static str,
    display: &'static str,
    spot: (usize, usize),
    style: Option<&'static str>,
    corner: Option<(&'static str, bool)>,
}

const fn look(
    key: &'static str,
    display: &'static str,
    spot: (usize, usize),
    style: Option<&'static str>,
    corner: Option<(&'static str, bool)>,
) -> SlotLook {
    SlotLook {
        key,
        display,
        spot,
        style,
        corner,
    }
}

const SLOT_LOOKS: [SlotLook; 19] = [
    look("eyes", "eyewear", (0, 0), Some("glasses"), None),
    look("head", "headwear", (0, 1), Some("head"), None),
    look("neck", "neckwear", (1, 0), Some("neck"), None),
    look("mask", "mask", (1, 1), Some("mask"), None),
    look("ears", "earwear", (0, 2), Some("ears"), None),
    look("handcuffs", "handcuffs", (3, 5), None, None),
    look("legcuffs", "legcuffs", (4, 2), None, None),
    look("jumpsuit", "uniform", (2, 1), Some("uniform"), None),
    look("suit", "suit", (2, 2), Some("suit"), None),
    look("gloves", "gloves", (3, 4), Some("gloves"), None),
    look(
        "right_hand",
        "right hand",
        (2, 4),
        Some("hand_r"),
        Some(("R", true)),
    ),
    look(
        "left_hand",
        "left hand",
        (2, 5),
        Some("hand_l"),
        Some(("L", false)),
    ),
    look("shoes", "shoes", (4, 1), Some("shoes"), None),
    look(
        "suit_storage",
        "suit storage item",
        (2, 0),
        Some("suit_storage"),
        None,
    ),
    look("id", "ID", (4, 0), Some("id"), None),
    look("belt", "belt", (3, 1), Some("belt"), None),
    look("back", "backpack", (1, 2), Some("back"), None),
    look("left_pocket", "left pocket", (3, 0), Some("pocket"), None),
    look("right_pocket", "right pocket", (3, 2), Some("pocket"), None),
];

impl ClientUi for StripMenuUi {
    // (64 + 6) * 6 + 6, (64 + 6) * 5 + 6 + 31.
    const WIDTH: f32 = 426.0;
    const HEIGHT: f32 = 387.0;

    fn content(_: &(), data: &StripMenuData) -> impl Scene {
        strip_view(data)
    }
}

fn strip_view(data: &StripMenuData) -> impl Scene + use<> {
    let rows: Vec<_> = (0..ROWS)
        .map(|row| {
            let cells: Vec<Box<dyn Scene>> = (0..COLUMNS)
                .map(|column| {
                    let found = SLOT_LOOKS
                        .iter()
                        .find(|l| l.spot == (row, column))
                        .and_then(|look| {
                            Some((look, data.items.iter().find(|i| i.key == look.key)?))
                        });
                    match found {
                        Some((look, slot)) => Box::new(strip_button(look, slot)) as Box<dyn Scene>,
                        None => Box::new(bsn! {
                            Node { width: px(BUTTON_PX), height: px(BUTTON_PX) }
                        }),
                    }
                })
                .collect();
            bsn! {
                Node { flex_direction: FlexDirection::Row, column_gap: px(GAP_PX) }
                Children [ {cells} ]
            }
        })
        .collect();
    bsn! {
        @WindowContent
        Children [
            Node { flex_direction: FlexDirection::Column, row_gap: px(GAP_PX) }
            Children [ {rows} ]
        ]
    }
}

/// One slot button: `act('use', { key })`.
fn strip_button(look: &SlotLook, slot: &StripSlot) -> impl Scene + use<> {
    let tooltip = match (&slot.item, slot.obscured) {
        (Some(item), _) => item.name.clone(),
        (None, OBSCURING_HIDDEN | OBSCURING_COMPLETELY) => format!("obscured {}", look.display),
        _ => look.display.to_owned(),
    };
    let color = if slot.interacting {
        UiColor::Css([132, 89, 21])
    } else {
        UiColor::Default
    };
    let disabled = slot.obscured == OBSCURING_INACCESSIBLE;
    let fill = || Node {
        position_type: PositionType::Absolute,
        left: px(0),
        top: px(0),
        width: px(BUTTON_PX),
        height: px(BUTTON_PX),
        ..default()
    };
    let background = look.style.filter(|_| slot.item.is_none()).map(|state| {
        let node = fill();
        EntityScene(bsn! {
            node
            StyleSprite { state: {state.to_owned()}, color: {Some([255, 255, 255, 178])} }
        })
    });
    let item = slot.item.as_ref().and_then(|i| i.icon).map(|icon| {
        let node = fill();
        EntityScene(bsn! {
            node
            UiSprite { icon_state: {Some(icon.icon_state)}, color: {icon.color} }
        })
    });
    let hidden = (slot.item.is_none()
        && matches!(slot.obscured, OBSCURING_HIDDEN | OBSCURING_COMPLETELY))
    .then(|| {
        let name = if slot.obscured == OBSCURING_COMPLETELY {
            "ban"
        } else {
            "eye-slash"
        };
        let node = Node {
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..fill()
        };
        EntityScene(bsn! {
            node
            Children [ @Icon { @name: name, @size: 3.0 } ]
        })
    });
    let corner = look.corner.map(|(text, left)| {
        let node = Node {
            position_type: PositionType::Absolute,
            top: px(0),
            left: if left { px(2) } else { Val::Auto },
            right: if left { Val::Auto } else { px(2) },
            ..default()
        };
        EntityScene(bsn! { node Children [ Text({text.to_owned()}) ] })
    });
    let params = ActParams(vec![("key".to_owned(), look.key.to_owned())]);
    bsn! {
        @ss13_ui_kit::Button { @tooltip: tooltip, @fluid: true }
        ss13_ui_kit::Button { color: color, disabled: disabled }
        Node {
            width: px(BUTTON_PX),
            height: px(BUTTON_PX),
            min_height: px(BUTTON_PX),
            padding: UiRect::ZERO,
        }
        Act({STRIP_USE.to_owned()})
        params
        Children [ {background} -- {item} -- {hidden} -- {corner} ]
    }
}

/// `contentsByPathName`: objs of one type and name share an entry, in first-seen order.
fn grouped(contents: &[LootEntry]) -> Vec<(&LootEntry, usize)> {
    let mut groups: Vec<(&LootEntry, usize)> = Vec::new();
    for entry in contents {
        let same = groups.iter_mut().find(|(first, _)| {
            !entry.path.is_empty() && first.path == entry.path && first.name == entry.name
        });
        match same {
            Some((_, amount)) => *amount += 1,
            None => groups.push((entry, 1)),
        }
    }
    groups
}

impl ClientUi for LootPanelUi {
    const WIDTH: f32 = 300.0;
    const HEIGHT: f32 = 76.0;

    /// `height = clamp(38 + groups * 38, 76, 418)`.
    fn size(_: &(), data: &LootPanelData) -> (f32, f32) {
        let rows = grouped(&data.contents).len() as f32;
        (Self::WIDTH, (38.0 + rows * 38.0).clamp(76.0, 418.0))
    }

    /// `<Window.Content fitted scrollable={height === maxHeight}><Section>`.
    // shortcut: the section is fitted too (no padding), so one entry fits tg's 76 px minimum.
    fn content(_: &(), data: &LootPanelData) -> impl Scene {
        let boxes: Vec<_> = grouped(&data.contents)
            .into_iter()
            .map(|(entry, amount)| loot_box(entry, amount))
            .collect();
        let scrollable = boxes.len() >= 10;
        bsn! {
            @WindowContent { @fitted: true, @scrollable: scrollable }
            Children [
                @Section { @fitted: true }
                Children [ {boxes} ]
            ]
        }
    }
}

/// `LootBox`: icon, capitalized name, `xN` for a group; a click grabs it.
fn loot_box(entry: &LootEntry, amount: usize) -> impl Scene + use<> {
    let mut name = if entry.name.is_empty() {
        "???".to_owned()
    } else {
        entry.name.clone()
    };
    if let Some(first) = name.chars().next() {
        name = first.to_uppercase().chain(name.chars().skip(1)).collect();
    }
    let count = if amount > 1 {
        format!("x{amount}")
    } else {
        String::new()
    };
    let icon = entry.icon.map(|icon| {
        EntityScene(bsn! {
            Node { width: px(32), height: px(32), margin: UiRect::right(px(4)) }
            UiSprite { icon_state: {Some(icon.icon_state)}, color: {icon.color} }
        })
    });
    let params = ActParams(vec![("ref".to_owned(), entry.key.to_string())]);
    bsn! {
        @ss13_ui_kit::Button { @fluid: true }
        ss13_ui_kit::Button { color: UiColor::Transparent }
        Node { min_height: px(36), padding: UiRect::ZERO }
        Act({LOOT_GRAB.to_owned()})
        params
        Children [
            {icon}
            --
            Node { flex_grow: 1.0, overflow: Overflow::clip() }
            Children [ Text(name) ]
            --
            Node { margin: UiRect::right(px(6)) }
            Children [ Text(count) ]
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_strippable_key_has_a_grid_spot() {
        let spots: Vec<_> = SLOT_LOOKS.iter().map(|l| l.spot).collect();
        for look in &SLOT_LOOKS {
            assert_eq!(
                spots.iter().filter(|s| **s == look.spot).count(),
                1,
                "{}",
                look.key
            );
            assert!(look.spot.0 < ROWS && look.spot.1 < COLUMNS);
        }
    }

    #[test]
    fn loot_groups_objs_of_one_type_and_name() {
        let entry = |name: &str, path: &str, key| LootEntry {
            name: name.into(),
            path: path.into(),
            icon: None,
            key,
        };
        let contents = [
            entry("floor", "", 1),
            entry("pen", "/obj/item/pen", 2),
            entry("Bob", "", 3),
            entry("pen", "/obj/item/pen", 4),
            entry("Alice", "", 5),
        ];
        let groups: Vec<(u64, usize)> = grouped(&contents)
            .iter()
            .map(|(e, n)| (e.key, *n))
            .collect();
        assert_eq!(groups, vec![(1, 1), (2, 2), (3, 1), (5, 1)]);
    }
}
