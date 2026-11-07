//! Sword sheaths and the captain's sabre reskin (BandaStation
//! `modular_bandastation/objects/code/items/clothing/belt.dm`, `items/sabre.dm`, after tg
//! `/datum/component/reskinable_item` and `/datum/atom_skin`):
//!
//! - every sheath (`/obj/item/storage/belt/sheath/update_icon_state`, [`SheathLook`]) shows
//!   `<base>-full` (icon, in-hand and worn state) while it holds something;
//! - the sabre sheath ([`SABRE_SHEATH`]) with a sabre inside is alt-clicked: a list (tg's radial)
//!   of the `/datum/atom_skin/cap_sheath` skins; the pick reskins the sheath and the sabre in it
//!   (`cap_sabre` of the same name) at once, and both lose the option (`infinite_reskin` off).
//!   An empty sheath (or one holding something else) warns and the click goes on.
// shortcut: a list prompt instead of the radial with sprite previews; tg's sheath alt-click
// (the blade into the hand) is not ported, an alt-click opens the storage as before.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmItem};
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::UiSystems;

use crate::health::Stat;
use crate::inventory::CarriedBy;
use crate::storage::{Stored, StoredIn};
use crate::tools::tell;

/// `/obj/item/storage/belt/sheath`.
pub const SHEATH: &str = "/obj/item/storage/belt/sheath";
/// `/obj/item/storage/belt/sheath/sabre`.
pub const SABRE_SHEATH: &str = "/obj/item/storage/belt/sheath/sabre";
/// `/obj/item/melee/sabre`.
pub const SABRE: &str = "/obj/item/melee/sabre";

/// `/datum/atom_skin/cap_sheath`: `new_icon`, `new_worn_icon`, `new_lefthand_file`,
/// `new_righthand_file`.
const SHEATH_ICON: &str = "modular_bandastation/objects/icons/obj/clothing/belts.dmi";
const SHEATH_WORN: &str = "modular_bandastation/objects/icons/obj/clothing/belts/sheath.dmi";
const SHEATH_LEFT: &str = "icons/mob/inhands/equipment/belt_lefthand.dmi";
const SHEATH_RIGHT: &str = "icons/mob/inhands/equipment/belt_righthand.dmi";
/// `/datum/atom_skin/cap_sabre`: `new_icon`, `new_lefthand_file`, `new_righthand_file`.
const SABRE_ICON: &str = "modular_bandastation/objects/icons/obj/weapons/sword.dmi";
const SABRE_LEFT: &str = "modular_bandastation/objects/icons/obj/weapons/saber/saber_left.dmi";
const SABRE_RIGHT: &str = "modular_bandastation/objects/icons/obj/weapons/saber/saber_right.dmi";

/// The skins (`preview_name`, state suffix), sorted by name (`sort_list(items)`): the sheath's
/// state is `sheath_<suffix>`, the sabre's `saber_<suffix>`.
pub const SKINS: [(&str, &str); 5] = [
    ("Ceremonial", "ceremonial"),
    ("Classic", "classic"),
    ("Cossack", "cossack"),
    ("Default", "default"),
    ("Katana", "katana"),
];

/// On a sheath: the states `update_icon_state` builds on (`initial(icon_state)`, `initial(
/// inhand_icon_state)`, `initial(worn_icon_state)`; the sabre sheath's `base_icon_state` for
/// all three) and whether it was last drawn full.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct SheathLook {
    pub icon: String,
    pub inhand: String,
    pub worn: String,
    pub full: Option<bool>,
}

/// `/datum/component/reskinable_item` still on the item (the sabre sheath and sabres).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Reskinnable;

/// On a prompt: the sheath whose skin is asked for.
#[derive(Component, Clone, Copy, Debug)]
pub struct ReskinAsk {
    pub sheath: Entity,
}

pub(super) fn build(app: &mut App) {
    app.add_systems(
        Update,
        (
            init_items,
            refresh_sheaths.after(init_items),
            answers
                .in_set(UiSystems::Handle)
                .after(PromptSystems::Answer),
        ),
    );
}

/// `setup_reskins` and the sheath's look bases, once per new item.
fn init_items(
    items: Query<(Entity, &DmItem, Option<&DmAtom>), Added<DmItem>>,
    mut commands: Commands,
) {
    for (e, dm, atom) in &items {
        let path = dm.path.as_str();
        if is_under(path, SHEATH) {
            let icon = atom.map(|a| a.icon_state.clone()).unwrap_or_default();
            let look = if is_under(path, SABRE_SHEATH) {
                // The fork's `update_icon_state`: `base_icon_state` ("sheath") for all three.
                SheathLook {
                    inhand: icon.clone(),
                    worn: icon.clone(),
                    icon,
                    full: None,
                }
            } else {
                SheathLook {
                    inhand: dm.inhand_icon_state.clone(),
                    worn: dm.worn_icon_state.clone(),
                    icon,
                    full: None,
                }
            };
            commands.entity(e).insert(look);
        }
        if is_under(path, SABRE_SHEATH) || is_under(path, SABRE) {
            commands.entity(e).insert(Reskinnable);
        }
    }
}

/// `-full` while the sheath holds something.
// shortcut: checks every sheath each tick (a handful per round); hook storage changes if
// sheaths ever become numerous.
fn refresh_sheaths(world: &mut World) {
    let mut state = world.query::<(Entity, &SheathLook, Option<&Stored>)>();
    let stale: Vec<(Entity, bool)> = state
        .iter(world)
        .filter_map(|(e, look, stored)| {
            let full = stored.is_some_and(|s| !s.items().is_empty());
            (look.full != Some(full)).then_some((e, full))
        })
        .collect();
    for (e, full) in stale {
        draw_sheath(world, e, full);
    }
}

/// `/obj/item/storage/belt/sheath/update_icon_state` (with `update_icon_updates_onmob`).
fn draw_sheath(world: &mut World, e: Entity, full: bool) {
    let look = {
        let Some(mut look) = world.get_mut::<SheathLook>(e) else {
            return;
        };
        look.full = Some(full);
        look.clone()
    };
    let suffix = if full { "-full" } else { "" };
    let icon_state = format!("{}{suffix}", look.icon);
    set_states(
        world,
        e,
        &icon_state,
        &format!("{}{suffix}", look.inhand),
        &format!("{}{suffix}", look.worn),
    );
}

/// The item's `icon_state` (its sprite), `inhand_icon_state` and `worn_icon_state`; the
/// carrier is drawn again.
fn set_states(world: &mut World, e: Entity, icon_state: &str, inhand: &str, worn: &str) {
    if let Some(mut atom) = world.get_mut::<DmAtom>(e) {
        atom.icon_state = icon_state.to_owned();
    }
    crate::obj_damage::set_icon_state(world, e, icon_state);
    if let Some(mut dm) = world.get_mut::<DmItem>(e) {
        dm.inhand_icon_state = inhand.to_owned();
        dm.worn_icon_state = worn.to_owned();
    }
    redraw_carrier(world, e);
}

fn redraw_carrier(world: &mut World, e: Entity) {
    let mut at = e;
    // An item in a carried sheath is drawn through the sheath.
    for _ in 0..4 {
        if let Some(mob) = world.get::<CarriedBy>(at).map(|c| c.0) {
            crate::inventory::refresh_look(world, mob);
            return;
        }
        match world.get::<StoredIn>(at) {
            Some(s) => at = s.0,
            None => return,
        }
    }
}

/// `/datum/atom_skin/apply`: `icon`, `icon_state`, `base_icon_state`, the in-hand files and
/// state, `worn_icon` (when the skin has one) and `worn_icon_state`.
fn apply(
    world: &mut World,
    e: Entity,
    icon: &str,
    worn: Option<&str>,
    hands: [&str; 2],
    state: &str,
) {
    if let Some(mut atom) = world.get_mut::<DmAtom>(e) {
        atom.icon = icon.to_owned();
        atom.icon_state = state.to_owned();
    }
    if let Some(mut dm) = world.get_mut::<DmItem>(e) {
        if let Some(worn) = worn {
            dm.worn_icon = worn.to_owned();
        }
        dm.lefthand_file = hands[0].to_owned();
        dm.righthand_file = hands[1].to_owned();
    }
    let full = world
        .get::<Stored>(e)
        .is_some_and(|s| !s.items().is_empty());
    let sheath = if let Some(mut look) = world.get_mut::<SheathLook>(e) {
        look.icon = state.to_owned();
        look.inhand = state.to_owned();
        look.worn = state.to_owned();
        true
    } else {
        false
    };
    if sheath {
        draw_sheath(world, e, full);
    } else {
        // `icon` is in `DmAtom` already: the sprite is looked up there.
        set_states(world, e, state, state, state);
    }
}

/// The sheath's first content when it is a sabre (`istype(source.contents[1], /obj/item/
/// melee/sabre)`).
fn sheathed_sabre(world: &World, sheath: Entity) -> Option<Entity> {
    let first = *world.get::<Stored>(sheath)?.items().first()?;
    world
        .get::<DmItem>(first)
        .is_some_and(|d| is_under(&d.path, SABRE))
        .then_some(first)
}

/// `can_perform_action(parent, NEED_DEXTERITY)` without the reach part (the click has it).
fn can_act(world: &World, mob: Entity) -> bool {
    let incap = crate::status_effects::incapacitation(world, mob);
    world.get::<Stat>(mob) == Some(&Stat::Conscious) && !incap.incapacitated && !incap.hands_blocked
}

/// `/datum/component/reskinable_item/sabre/on_click_alt_reskin`: the skin list. True when it
/// handled the click; an empty sheath warns and lets the click go on.
pub fn click_alt(world: &mut World, client: Entity, mob: Entity, sheath: Entity) -> bool {
    if world.get::<Reskinnable>(sheath).is_none()
        || !world
            .get::<DmItem>(sheath)
            .is_some_and(|d| is_under(&d.path, SABRE_SHEATH))
    {
        return false;
    }
    if sheathed_sabre(world, sheath).is_none() {
        tell(
            world,
            client,
            Span::Warning,
            "You need to have a sabre in the sheath to reskin it.",
        );
        return false;
    }
    if !can_act(world, mob) {
        return false;
    }
    let title = crate::security::name_of(world, sheath);
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            title,
            "Выберите облик",
            PromptKind::List {
                items: SKINS.iter().map(|(n, _)| (*n).to_owned()).collect(),
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(ReskinAsk { sheath });
    true
}

fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in answers {
        let Some(ask) = world.get::<ReskinAsk>(prompt).copied() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        // `check_reskin_menu`, again at the moment of effect: the sheath still reskinnable,
        // in reach, holding a sabre; the user able to act.
        if i >= SKINS.len()
            || world.get_entity(ask.sheath).is_err()
            || world.get::<Reskinnable>(ask.sheath).is_none()
            || !can_act(world, mob)
            || !crate::storage::in_reach(world, mob, ask.sheath)
        {
            continue;
        }
        let Some(sabre) = sheathed_sabre(world, ask.sheath) else {
            continue;
        };
        reskin(world, ask.sheath, sabre, i);
        let text = format!(
            "{} is now skinned as '{}.'",
            crate::security::name_of(world, ask.sheath),
            SKINS[i].0
        );
        tell(world, client, Span::Info, text);
    }
}

/// `reskin_obj`: skin `i` on the sheath and, while it still has its component, the sabre;
/// both lose the option.
pub fn reskin(world: &mut World, sheath: Entity, sabre: Entity, i: usize) {
    let suffix = SKINS[i].1;
    apply(
        world,
        sheath,
        SHEATH_ICON,
        Some(SHEATH_WORN),
        [SHEATH_LEFT, SHEATH_RIGHT],
        &format!("sheath_{suffix}"),
    );
    world.entity_mut(sheath).remove::<Reskinnable>();
    if world.get::<Reskinnable>(sabre).is_some() {
        apply(
            world,
            sabre,
            SABRE_ICON,
            None,
            [SABRE_LEFT, SABRE_RIGHT],
            &format!("saber_{suffix}"),
        );
        world.entity_mut(sabre).remove::<Reskinnable>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_mapload::fidelity::IconIndex;
    use ss13_shared::{Appearance, IconStateId};
    use std::collections::HashMap;

    fn item(world: &mut World, path: &str, icon: &str, state: &str) -> Entity {
        let atom = DmAtom {
            icon: icon.into(),
            icon_state: state.into(),
            ..default()
        };
        let dm = DmItem {
            path: path.into(),
            inhand_icon_state: state.into(),
            worn_icon_state: state.into(),
            ..default()
        };
        world
            .spawn((atom, dm, Appearance::new(IconStateId(0))))
            .id()
    }

    #[test]
    fn the_sheath_and_its_sabre_take_one_skin_once() {
        let mut app = App::new();
        let mut icons = HashMap::new();
        for (n, (dmi, state)) in [
            (SHEATH_ICON, "sheath_cossack-full"),
            (SABRE_ICON, "saber_cossack"),
            ("belts.dmi", "sheath-full"),
        ]
        .into_iter()
        .enumerate()
        {
            icons.insert(
                (dmi.to_owned(), state.to_owned()),
                IconStateId(n as u32 + 1),
            );
        }
        app.insert_resource(IconIndex(icons));
        app.add_systems(Update, (init_items, refresh_sheaths.after(init_items)));
        let world = app.world_mut();
        let sheath = item(world, SABRE_SHEATH, "belts.dmi", "sheath");
        let sabre = item(world, SABRE, "sword.dmi", "sabre");
        world.entity_mut(sabre).insert(StoredIn(sheath));
        app.update();
        let world = app.world_mut();
        assert_eq!(
            world.get::<Appearance>(sheath).unwrap().icon_state,
            IconStateId(3)
        );
        assert_eq!(
            world.get::<DmItem>(sheath).unwrap().worn_icon_state,
            "sheath-full"
        );
        assert!(world.get::<Reskinnable>(sabre).is_some());

        assert_eq!(SKINS[2].0, "Cossack");
        reskin(world, sheath, sabre, 2);
        let dm = world.get::<DmItem>(sheath).unwrap();
        assert_eq!(dm.worn_icon, SHEATH_WORN);
        assert_eq!(dm.worn_icon_state, "sheath_cossack-full");
        assert_eq!(dm.inhand_icon_state, "sheath_cossack-full");
        assert_eq!(
            world.get::<Appearance>(sheath).unwrap().icon_state,
            IconStateId(1)
        );
        assert_eq!(
            world.get::<Appearance>(sabre).unwrap().icon_state,
            IconStateId(2)
        );
        let dm = world.get::<DmItem>(sabre).unwrap();
        assert_eq!(
            (dm.lefthand_file.as_str(), dm.inhand_icon_state.as_str()),
            (SABRE_LEFT, "saber_cossack")
        );
        assert!(world.get::<Reskinnable>(sheath).is_none());
        assert!(world.get::<Reskinnable>(sabre).is_none());

        // Drawn: the sheath goes back to its skin's empty state.
        world.entity_mut(sabre).remove::<StoredIn>();
        app.update();
        let dm = app.world().get::<DmItem>(sheath).unwrap();
        assert_eq!(dm.worn_icon_state, "sheath_cossack");
    }
}
