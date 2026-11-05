//! Clothing that changes while worn (server): adjustable masks (tg `adjust_visor` /
//! `visor_toggling` of `/obj/item/clothing/mask`, `masks/breath.dm`), jumpsuit styles
//! (`under/_under.dm` `rolldown`, `adjust_to_alt`/`adjust_to_normal`), toggle suits
//! (`/datum/component/toggle_icon`), hoods (`/datum/component/toggle_attached_clothing` of
//! `/obj/item/clothing/suit/hooded`) and their item action buttons (`/datum/action/item_action`).
//!
//! - A mask with the "Adjust" action (breath, sterile, balaclava, sechailer masks) is pushed up
//!   and down: in the hand (`attack_self`), by its button while worn, or alt-click (breath masks).
//!   `visor_flags`, `visor_flags_inv`, `visor_flags_cover` toggle: a breath mask pushed up stops
//!   feeding internals (`MASKINTERNALS`: the next breath closes the tank) and uncovers the mouth;
//!   a sterile mask stops hiding the face. Its sprite takes the `_up` state.
//! - Alt-click on a jumpsuit that `can_adjust` rolls it down (`_d` worn state; chest and arms
//!   uncovered unless `alt_covers_chest`) or back up.
//! - Alt-click on a toggle suit opens or closes it (`_t` state).
//! - A hooded suit worn in the suit slot gets a "Toggle Hood" button: the hood goes onto the empty
//!   head slot (a new item) and the suit takes its `hood_up_affix` state; taking the hood or the
//!   suit off, or the button again, puts the hood away (the item is deleted, as tg's
//!   `alternative_mode`, and made again next time).
// shortcut: no `adjusted_flags` (a bandana's slot change), no visor sounds, per-type
// `toggle_message`s or voice boxes; a jumpsuit drawn from the body's hand-resolved sprites
// (`crate::human::WORN`, the grey jumpsuit) does not show its rolled-down state; hoods are not
// deployed by outfits (`auto_deploy_on_outfit_equip`); buttons only show for worn items.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmClothing, DmItem, IconIndex};
use ss13_net::server::Networked;
use ss13_shared::Appearance;
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;
use ss13_tgui::server::{UiActed, UiSystems};

use super::damage::{ARM_LEFT, ARM_RIGHT, CHEST};
use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::health::Stat;
use crate::inventory::{ALL_SLOTS, CarriedBy, Inventory};
use crate::server::PlayerMob;
use crate::tools::tell;

pub const MASK_ROOT: &str = "/obj/item/clothing/mask";
pub const BREATH_MASK: &str = "/obj/item/clothing/mask/breath";
pub const ADJUST_ACTION: &str = "/datum/action/item_action/adjust";
const BUTTON: &str = "clothing:";
const BUTTON_BACKGROUNDS: &str = "icons/mob/actions/backgrounds.dmi";

/// tg `up` of a mask pushed up, with the worn state it had down.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct MaskUp {
    worn_icon_state: String,
}

/// tg `adjusted == ALT_STYLE` of a rolled-down jumpsuit.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct RolledDown {
    worn_icon_state: String,
}

/// `/datum/component/toggle_icon` `toggled`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct SuitOpen {
    worn_icon_state: String,
}

/// A hooded suit with its hood up (`currently_deployed`).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct HoodUp {
    pub hood: Entity,
    worn_icon_state: String,
}

/// A deployed hood and its suit.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hood {
    pub suit: Entity,
}

pub(super) fn build(app: &mut App) {
    app.add_systems(
        Update,
        (
            watch_hoods.run_if(any_with_component::<Hood>),
            refresh_buttons,
        )
            .chain()
            .after(crate::inventory::inventory_intents),
    )
    .add_systems(
        Update,
        trigger_buttons
            .in_set(UiSystems::Handle)
            .run_if(resource_exists::<Messages<UiActed<ActionButtonsUi>>>),
    );
}

fn clothing(world: &World, item: Entity) -> Option<(&DmItem, &DmClothing)> {
    Some((world.get::<DmItem>(item)?, world.get::<DmClothing>(item)?))
}

fn wearer(world: &World, item: Entity) -> Option<Entity> {
    world.get::<CarriedBy>(item).map(|c| c.0)
}

/// tg `can_use`: not incapacitated.
fn can_use(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, mob).incapacitated
}

/// The item's sprite: `icon_state` on the map and in hands, `worn` (empty: the icon state) on
/// the mob; its wearer is drawn again.
fn set_states(world: &mut World, item: Entity, icon_state: &str, worn: String) {
    let id = world
        .get::<DmAtom>(item)
        .zip(world.get_resource::<IconIndex>())
        .and_then(|(atom, icons)| icons.get(&atom.icon, icon_state));
    if let (Some(id), Some(mut look)) = (id, world.get_mut::<Appearance>(item))
        && look.icon_state != id
    {
        look.icon_state = id;
    }
    if let Some(mut dm) = world.get_mut::<DmItem>(item) {
        dm.worn_icon_state = worn;
    }
    if let Some(mob) = wearer(world, item) {
        crate::inventory::refresh_look(world, mob);
    }
}

fn base_state(world: &World, item: Entity) -> String {
    world
        .get::<DmAtom>(item)
        .map(|a| a.icon_state.clone())
        .unwrap_or_default()
}

// --- Masks -----------------------------------------------------------------------------------

/// A mask with the "Adjust" action that is not kept from adjusting (`can_be_adjusted`,
/// `adjustable`).
pub fn adjustable_mask(world: &World, item: Entity) -> bool {
    clothing(world, item).is_some_and(|(dm, c)| {
        is_under(&dm.path, MASK_ROOT)
            && c.adjustable
            && c.actions.iter().any(|a| is_under(a, ADJUST_ACTION))
    })
}

/// tg `adjust_visor` of a mask: `visor_toggling` (flags and the `_up` state), the message.
pub fn adjust_mask(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if !adjustable_mask(world, item) {
        return false;
    }
    if !can_use(world, mob) {
        return true;
    }
    let up = world.get::<MaskUp>(item).cloned();
    if let Some(mut c) = world.get_mut::<DmClothing>(item) {
        let c = &mut *c;
        c.clothing_flags ^= c.visor_flags;
        c.flags_inv ^= c.visor_flags_inv;
        c.flags_cover ^= c.visor_flags_cover;
    }
    let base = base_state(world, item);
    let name = crate::security::name_of(world, item);
    match up {
        // Down again.
        Some(MaskUp { worn_icon_state }) => {
            world.entity_mut(item).remove::<MaskUp>();
            set_states(world, item, &base, worn_icon_state);
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы ставите обратно {name}."),
            );
        }
        None => {
            let worn_icon_state = world
                .get::<DmItem>(item)
                .map(|i| i.worn_icon_state.clone())
                .unwrap_or_default();
            // `update_icon_state`: the icon gets `_up`; an explicit worn state stays.
            let up_state = format!("{base}_up");
            let worn = if worn_icon_state.is_empty() {
                up_state.clone()
            } else {
                worn_icon_state.clone()
            };
            world.entity_mut(item).insert(MaskUp { worn_icon_state });
            set_states(world, item, &up_state, worn);
            tell(world, client, Span::Notice, format!("Вы убираете {name}."));
        }
    }
    true
}

/// tg `attack_self` of the clothing that has one here: an adjustable mask.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    adjust_mask(world, client, mob, item)
}

// --- Jumpsuits and toggle suits --------------------------------------------------------------

/// tg `/obj/item/clothing/under/click_alt` -> `rolldown`. False for other items.
pub fn rolldown(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some((dm, c)) = clothing(world, item) else {
        return false;
    };
    if !is_under(&dm.path, super::accessories::UNDER_ROOT) {
        return false;
    }
    let (can_adjust, alt_covers_chest, limb_integrity) =
        (c.can_adjust, c.alt_covers_chest, c.limb_integrity);
    if !can_adjust {
        tell(world, client, Span::Warning, "нельзя сменить стиль!");
        return true;
    }
    if !can_use(world, mob) {
        return true;
    }
    let base = base_state(world, item);
    let rolled = world.get::<RolledDown>(item).cloned();
    let torso = CHEST | ARM_LEFT | ARM_RIGHT;
    match rolled {
        // `adjust_to_normal`: chest and arms covered again, but not a zone the damage disabled.
        Some(RolledDown { worn_icon_state }) => {
            world.entity_mut(item).remove::<RolledDown>();
            if !alt_covers_chest {
                let disabled: u32 = world
                    .get::<super::damage::ClothingWear>(item)
                    .map_or(0, |w| {
                        [
                            crate::zone::BodyZone::Chest,
                            crate::zone::BodyZone::LArm,
                            crate::zone::BodyZone::RArm,
                        ]
                        .into_iter()
                        .filter(|z| w.by_zone[z.index()] > limb_integrity)
                        .fold(0, |acc, z| acc | super::damage::zone_cover(z))
                    });
                if let Some(mut c) = world.get_mut::<DmClothing>(item) {
                    c.body_parts_covered |= torso & !disabled;
                }
            }
            set_states(world, item, &base, worn_icon_state);
            tell(
                world,
                client,
                Span::Notice,
                "Вы изменяете стиль на стандартный.",
            );
        }
        // `adjust_to_alt`.
        None => {
            if !alt_covers_chest && let Some(mut c) = world.get_mut::<DmClothing>(item) {
                c.body_parts_covered &= !torso;
            }
            let worn_icon_state = world
                .get::<DmItem>(item)
                .map(|i| i.worn_icon_state.clone())
                .unwrap_or_default();
            let down = if worn_icon_state.is_empty() {
                format!("{base}_d")
            } else {
                format!("{worn_icon_state}_d")
            };
            world
                .entity_mut(item)
                .insert(RolledDown { worn_icon_state });
            set_states(world, item, &base, down);
            tell(
                world,
                client,
                Span::Notice,
                "Вы изменяете стиль на свободный.",
            );
        }
    }
    true
}

/// `/datum/component/toggle_icon/on_click_alt` of a toggle suit. False for other items.
pub fn toggle_suit(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(noun) = clothing(world, item)
        .map(|(_, c)| c.toggle_noun.clone())
        .filter(|n| !n.is_empty())
    else {
        return false;
    };
    if !can_use(world, mob) {
        return true;
    }
    let base = base_state(world, item);
    match world.get::<SuitOpen>(item).cloned() {
        Some(SuitOpen { worn_icon_state }) => {
            world.entity_mut(item).remove::<SuitOpen>();
            set_states(world, item, &base, worn_icon_state);
        }
        None => {
            let worn_icon_state = world
                .get::<DmItem>(item)
                .map(|i| i.worn_icon_state.clone())
                .unwrap_or_default();
            let toggled = format!("{base}_t");
            let worn = if worn_icon_state.is_empty() {
                toggled.clone()
            } else {
                worn_icon_state.clone()
            };
            world.entity_mut(item).insert(SuitOpen { worn_icon_state });
            set_states(world, item, &toggled, worn);
        }
    }
    tell(world, client, Span::Notice, format!("toggled {noun}"));
    true
}

/// `/obj/item/clothing/mask/breath/click_alt`.
pub fn breath_alt(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let breath = world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, BREATH_MASK));
    breath && adjust_mask(world, client, mob, item)
}

// --- Hoods -----------------------------------------------------------------------------------

fn hood_type(world: &World, suit: Entity) -> Option<String> {
    clothing(world, suit)
        .map(|(_, c)| c.hood_type.clone())
        .filter(|h| !h.is_empty())
}

/// `toggle_deployable` of a hooded suit. False for other items.
pub fn toggle_hood(world: &mut World, client: Entity, mob: Entity, suit: Entity) -> bool {
    let Some(path) = hood_type(world, suit) else {
        return false;
    };
    if world.get::<HoodUp>(suit).is_some() {
        retract_hood(world, suit);
        return true;
    }
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return true;
    };
    if inventory.suit != Some(suit) {
        tell(world, client, Span::Warning, "wear it first!");
        return true;
    }
    if inventory.head.is_some() {
        tell(world, client, Span::Warning, "slot occupied!");
        return true;
    }
    let Some(hood) = crate::construction::spawn_detached(world, &path) else {
        return true;
    };
    world
        .entity_mut(hood)
        .insert((Networked, CarriedBy(mob), Hood { suit }));
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.head = Some(hood);
    }
    let affix = clothing(world, suit)
        .map(|(_, c)| c.hood_up_affix.clone())
        .unwrap_or_default();
    let worn_icon_state = world
        .get::<DmItem>(suit)
        .map(|i| i.worn_icon_state.clone())
        .unwrap_or_default();
    world.entity_mut(suit).insert(HoodUp {
        hood,
        worn_icon_state,
    });
    if !affix.is_empty() {
        let state = format!("{}{affix}", base_state(world, suit));
        set_states(world, suit, &state, state.clone());
    }
    crate::inventory::refresh_look(world, mob);
    true
}

/// `remove_deployable`: the hood goes (deleted) and the suit's state comes back.
pub fn retract_hood(world: &mut World, suit: Entity) {
    let Some(up) = world.get::<HoodUp>(suit).cloned() else {
        return;
    };
    world.entity_mut(suit).remove::<HoodUp>();
    if world.get_entity(up.hood).is_ok() {
        if let Some(mob) = wearer(world, up.hood) {
            crate::inventory::refresh_look(world, mob);
        }
        world.despawn(up.hood);
    }
    let affix_used = clothing(world, suit).is_some_and(|(_, c)| !c.hood_up_affix.is_empty());
    if affix_used {
        let base = base_state(world, suit);
        set_states(world, suit, &base, up.worn_icon_state);
    }
}

/// A hood stays only while it is on the head of whoever wears its suit in the suit slot
/// (`on_deployed_dropped`, `on_parent_equipped`, `COMSIG_ITEM_POST_UNEQUIP`).
fn watch_hoods(world: &mut World) {
    let mut q = world.query::<(Entity, &Hood)>();
    let hoods: Vec<(Entity, Hood)> = q.iter(world).map(|(e, h)| (e, *h)).collect();
    for (hood, Hood { suit }) in hoods {
        let kept = world.get::<HoodUp>(suit).is_some_and(|u| u.hood == hood)
            && wearer(world, hood).is_some_and(|mob| {
                world
                    .get::<Inventory>(mob)
                    .is_some_and(|i| i.head == Some(hood) && i.suit == Some(suit))
            });
        if kept {
            continue;
        }
        if world.get::<HoodUp>(suit).is_some() {
            retract_hood(world, suit);
        }
        if world.get_entity(hood).is_ok() {
            world.despawn(hood);
        }
    }
}

// --- Item action buttons ---------------------------------------------------------------------

/// The button a worn item gives (tg `actions_types` and `toggle_attached_clothing`'s action):
/// "Toggle Hood" for a hooded suit in the suit slot, "Adjust" for an adjustable mask worn.
fn button_name(world: &World, slot: InventorySlot, item: Entity) -> Option<&'static str> {
    if slot == InventorySlot::Suit && hood_type(world, item).is_some() {
        return Some("Toggle Hood");
    }
    if slot == InventorySlot::Mask && adjustable_mask(world, item) {
        return Some("Adjust");
    }
    None
}

/// The mob's clothing buttons after its other buttons, when its inventory changed.
fn refresh_buttons(world: &mut World) {
    let mut q = world.query_filtered::<Entity, (Changed<Inventory>, With<PlayerMob>)>();
    let mobs: Vec<Entity> = q.iter(world).collect();
    for mob in mobs {
        let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
            continue;
        };
        let icons = world.get_resource::<IconIndex>();
        let mut ours = Vec::new();
        for (index, &slot) in ALL_SLOTS.iter().enumerate() {
            let Some(item) = inventory.slot(slot) else {
                continue;
            };
            let Some(name) = button_name(world, slot, item) else {
                continue;
            };
            ours.push((
                ActionButton {
                    id: format!("{BUTTON}{index}"),
                    name: name.to_owned(),
                    icon: world.get::<Appearance>(item).map(|a| a.icon_state),
                    background: icons.and_then(|i| i.get(BUTTON_BACKGROUNDS, "bg_default")),
                    border: None,
                    available: true,
                    cooldown: 0.0,
                    active: false,
                    maptext: None,
                },
                0.0,
            ));
        }
        let current = world.get::<ActionButtons>(mob);
        let had = current.is_some_and(|b| b.0.iter().any(|(b, _)| b.id.starts_with(BUTTON)));
        if ours.is_empty() && !had {
            continue;
        }
        let mut buttons: Vec<(ActionButton, f64)> = current
            .map(|b| {
                b.0.iter()
                    .filter(|(b, _)| !b.id.starts_with(BUTTON))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        buttons.extend(ours);
        set_buttons(world, mob, buttons);
    }
}

/// A click on a clothing button, checked at the moment of effect: the client's conscious mob
/// still wears the item in that slot (`AB_CHECK_CONSCIOUS`, `item_action_slot_check`).
fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .filter(|a| a.act.starts_with(BUTTON))
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || !can_use(world, actor)
        {
            continue;
        }
        let Some(slot) = id
            .strip_prefix(BUTTON)
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| ALL_SLOTS.get(n).copied())
        else {
            continue;
        };
        let Some(item) = world.get::<Inventory>(actor).and_then(|i| i.slot(slot)) else {
            continue;
        };
        match button_name(world, slot, item) {
            Some("Toggle Hood") => {
                toggle_hood(world, client, actor, item);
            }
            Some(_) => {
                adjust_mask(world, client, actor, item);
            }
            None => {}
        }
    }
}

/// Examine hints of the toggles (`/obj/item/clothing/under/examine`, `toggle_icon/on_examine`,
/// `/obj/item/clothing/mask/breath/examine`).
pub fn examine_lines(world: &World, item: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some((dm, c)) = clothing(world, item) else {
        return;
    };
    let name = crate::security::name_of(world, item);
    if is_under(&dm.path, super::accessories::UNDER_ROOT) && c.can_adjust {
        let style = if world.get::<RolledDown>(item).is_some() {
            "стандартный"
        } else {
            "свободный"
        };
        lines.push((
            Span::Plain,
            format!("Альт-Клик для смены стиля на {style}."),
        ));
    }
    if !c.toggle_noun.is_empty() {
        lines.push((
            Span::Notice,
            format!("Alt-click on {name} to toggle the {}.", c.toggle_noun),
        ));
    }
    if is_under(&dm.path, BREATH_MASK) && c.adjustable {
        lines.push((Span::Notice, format!("Alt-click {name} to adjust it.")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clothing::cover::{HIDEFACE, MASKCOVERSMOUTH};

    const MASKINTERNALS: u32 = 1 << 3;

    #[test]
    fn a_breath_mask_pushed_up_stops_internals_and_uncovers() {
        let mut world = World::new();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        let mask = world
            .spawn((
                DmItem {
                    path: "/obj/item/clothing/mask/breath".into(),
                    ..default()
                },
                DmAtom {
                    icon_state: "breath".into(),
                    ..default()
                },
                DmClothing {
                    clothing_flags: MASKINTERNALS,
                    visor_flags: MASKINTERNALS,
                    flags_cover: MASKCOVERSMOUTH,
                    visor_flags_cover: MASKCOVERSMOUTH,
                    actions: vec![ADJUST_ACTION.into()],
                    ..default()
                },
            ))
            .id();
        let mob = world
            .spawn((
                Stat::Conscious,
                Inventory {
                    mask: Some(mask),
                    ..default()
                },
            ))
            .id();
        world.entity_mut(mask).insert(CarriedBy(mob));
        let client = world.spawn_empty().id();
        assert!(adjust_mask(&mut world, client, mob, mask));
        let c = world.get::<DmClothing>(mask).unwrap();
        assert_eq!(c.clothing_flags & MASKINTERNALS, 0);
        assert_eq!(c.flags_cover, 0);
        assert_eq!(
            world.get::<DmItem>(mask).unwrap().worn_icon_state,
            "breath_up"
        );
        assert!(!crate::internals::can_breathe_internals(
            &world,
            mob,
            &world.get::<Inventory>(mob).unwrap().clone()
        ));
        // Down again: all back.
        assert!(adjust_mask(&mut world, client, mob, mask));
        let c = world.get::<DmClothing>(mask).unwrap();
        assert_eq!(c.clothing_flags, MASKINTERNALS);
        assert_eq!(c.flags_cover, MASKCOVERSMOUTH);
        assert_eq!(world.get::<DmItem>(mask).unwrap().worn_icon_state, "");
    }

    #[test]
    fn a_sterile_mask_stops_hiding_the_face() {
        let mut world = World::new();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        let mask = world
            .spawn((
                DmItem {
                    path: "/obj/item/clothing/mask/surgical".into(),
                    ..default()
                },
                DmClothing {
                    flags_inv: HIDEFACE,
                    visor_flags_inv: HIDEFACE,
                    actions: vec![ADJUST_ACTION.into()],
                    ..default()
                },
            ))
            .id();
        let mob = world
            .spawn((
                Stat::Conscious,
                Inventory {
                    mask: Some(mask),
                    ..default()
                },
            ))
            .id();
        let client = world.spawn_empty().id();
        assert!(crate::clothing::cover::face_obscured(&world, mob));
        adjust_mask(&mut world, client, mob, mask);
        assert!(!crate::clothing::cover::face_obscured(&world, mob));
        // Not adjustable without the action (a gas mask).
        world.get_mut::<DmClothing>(mask).unwrap().actions.clear();
        assert!(!adjust_mask(&mut world, client, mob, mask));
    }
}
