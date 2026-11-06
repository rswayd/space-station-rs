//! The strip menu (server), after tg `/datum/element/strippable`, `/datum/strip_menu`
//! (`code/datums/elements/strippable.dm`), `human_stripping.dm` and `carbon_stripping.dm`.
//!
//! Dragging another human onto yourself (`crate::drag`, tg `COMSIG_MOUSEDROP_ONTO` of the
//! element) opens the tgui `StripMenu` window on them: their slots in tg's
//! `strippable_human_items` order, the items' names and icons, pockets with something in them
//! hidden (`STRIPPABLE_OBSCURING_HIDDEN`), slots their clothing hides (`obscured_slots`: a helmet
//! hiding the glasses) shown as obscured without the item, slots it only covers
//! (`covered_slots`, `transparent_protection`) shown but out of reach, the handcuffs entry while
//! they are cuffed. Its `use` act on a slot that is neither (tg `ui_act` refuses those):
//! - with an item there: tg `start_unequip` (a `do_after` of the item's `strip_delay`, 4 s in
//!   pockets, `POCKET_STRIP_DELAY`), then `finish_unequip` (`doStrip` drops it at their feet,
//!   handcuffs come off) if it is still the same item and the user still next to them;
//! - empty, with an item in the active hand: tg `start_equip` (a `do_after` of
//!   `equip_delay_other`, 1 s for pockets), then the item goes into the slot if the slot is still
//!   empty, the item still fits (`can_equip`, a free hand) and the user still next to them.
//!
//! The window's `ui_status` is tg's: closed beyond reach, update-only with blocked hands,
//! disabled when the user is incapacitated or unconscious. The slot the user works on is
//! highlighted (`interacting`).
// shortcut: no alternate actions (internals valves, suit sensors, shoelaces, item cuffs),
// per-item `strip_delay` and `equip_delay_other` are tg's defaults (4 s and 2 s: not converted
// per type yet), no legcuffs, no AFK theft log, no fireman carry exception (`should_strip`).

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::{DmClothing, DmName};
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::protocol::InventorySlot;
use ss13_shared::ui::UiStatus;
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};

use crate::health::{DEFAULT_MESSAGE_RANGE, Stat};
use crate::hud::HudIcon;
use crate::inventory::{CarriedBy, Inventory, can_equip, unequip};
use crate::panels::{
    OBSCURING_COMPLETELY, OBSCURING_HIDDEN, OBSCURING_INACCESSIBLE, OBSCURING_NONE, STRIP_USE,
    StripItem, StripMenuData, StripMenuUi, StripSlot,
};
use crate::restraints::Handcuffed;
use crate::security::{name_of, show};
use crate::server::PlayerMob;
use crate::tools::{Actor, DoAfter, do_after, tell};

/// `/obj/item::strip_delay`, `/obj/item::equip_delay_other`, deciseconds.
pub const STRIP_DELAY_DS: f32 = 40.0;
pub const EQUIP_DELAY_OTHER_DS: f32 = 20.0;
/// `POCKET_STRIP_DELAY`, `POCKET_EQUIP_DELAY`.
pub const POCKET_STRIP_DELAY_DS: f32 = 40.0;
pub const POCKET_EQUIP_DELAY_DS: f32 = 10.0;

/// What a strip menu entry reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reach {
    Slot(InventorySlot),
    /// `ITEM_SLOT_HANDCUFFED`: shown only while cuffed, nothing can be put there.
    Handcuffs,
}

/// `strippable_human_items` for a human with two hands (the upper hands, legcuffs and the
/// BandaStation headpocket do not show).
const ENTRIES: [(&str, Reach); 18] = [
    ("head", Reach::Slot(InventorySlot::Head)),
    ("back", Reach::Slot(InventorySlot::Back)),
    ("mask", Reach::Slot(InventorySlot::Mask)),
    ("neck", Reach::Slot(InventorySlot::Neck)),
    ("eyes", Reach::Slot(InventorySlot::Eyes)),
    ("ears", Reach::Slot(InventorySlot::Ears)),
    ("jumpsuit", Reach::Slot(InventorySlot::Uniform)),
    ("suit", Reach::Slot(InventorySlot::Suit)),
    ("gloves", Reach::Slot(InventorySlot::Gloves)),
    ("shoes", Reach::Slot(InventorySlot::Shoes)),
    ("suit_storage", Reach::Slot(InventorySlot::SuitStorage)),
    ("id", Reach::Slot(InventorySlot::Id)),
    ("belt", Reach::Slot(InventorySlot::Belt)),
    ("left_pocket", Reach::Slot(InventorySlot::LeftPocket)),
    ("right_pocket", Reach::Slot(InventorySlot::RightPocket)),
    ("left_hand", Reach::Slot(InventorySlot::LeftHand)),
    ("right_hand", Reach::Slot(InventorySlot::RightHand)),
    ("handcuffs", Reach::Handcuffs),
];

fn is_pocket(reach: Reach) -> bool {
    matches!(
        reach,
        Reach::Slot(InventorySlot::LeftPocket | InventorySlot::RightPocket)
    )
}

/// The `do_after` of a `use`: `entry` indexes [`ENTRIES`], `item` is the item taken off or put
/// on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StripJob {
    Unequip { entry: u8, item: Entity },
    Equip { entry: u8, item: Entity },
}

impl StripJob {
    fn entry(self) -> u8 {
        match self {
            Self::Unequip { entry, .. } | Self::Equip { entry, .. } => entry,
        }
    }
}

/// `get_item(source)`.
fn item_in(inventory: &Inventory, cuffs: Option<&Handcuffed>, reach: Reach) -> Option<Entity> {
    match reach {
        Reach::Slot(slot) => inventory.slot(slot),
        Reach::Handcuffs => cuffs.map(|c| c.0),
    }
}

/// tg `get_obscuring` of an entry: a pocket with something in it is hidden
/// (`pocket/get_obscuring`); a slot the worn clothing's `flags_inv` hides is obscured completely,
/// one only its `transparent_protection` covers is inaccessible (`mob_item_slot/get_obscuring`).
/// `clothing` gives an item's (`flags_inv`, `transparent_protection`).
fn obscuring(
    inventory: &Inventory,
    reach: Reach,
    item: Option<Entity>,
    clothing: impl Fn(Entity) -> (u32, u32),
) -> u8 {
    if is_pocket(reach) {
        return if item.is_some() {
            OBSCURING_HIDDEN
        } else {
            OBSCURING_NONE
        };
    }
    let Reach::Slot(slot) = reach else {
        return OBSCURING_NONE;
    };
    let obscured = crate::clothing::cover::obscured_by(inventory, |e| clothing(e).0);
    let covered = crate::clothing::cover::obscured_by(inventory, |e| {
        let (inv, transparent) = clothing(e);
        inv | transparent
    });
    if crate::clothing::cover::slot_hidden(obscured, slot) {
        OBSCURING_COMPLETELY
    } else if crate::clothing::cover::slot_hidden(covered, slot) {
        OBSCURING_INACCESSIBLE
    } else {
        OBSCURING_NONE
    }
}

fn clothing_flags(c: Option<&DmClothing>) -> (u32, u32) {
    c.map_or((0, 0), |c| (c.flags_inv, c.transparent_protection))
}

type UserQueries = (
    Query<'static, 'static, (Option<&'static Appearance>, Option<&'static DmName>)>,
    Query<'static, 'static, &'static DoAfter>,
    Query<'static, 'static, &'static DmClothing>,
);

impl ServerUi for StripMenuUi {
    type Source = (
        Entity,
        &'static Inventory,
        Option<&'static Handcuffed>,
        Option<&'static Name>,
    );
    type UserParam = UserQueries;
    type Act = u8;

    fn static_data(_: (Entity, &Inventory, Option<&Handcuffed>, Option<&Name>)) {}

    fn data(
        (owner, inventory, cuffs, name): (Entity, &Inventory, Option<&Handcuffed>, Option<&Name>),
        user: Entity,
        (items, actions, clothing): &SystemParamItem<'_, '_, UserQueries>,
    ) -> StripMenuData {
        let working = actions
            .get(user)
            .ok()
            .filter(|a| a.target == owner)
            .and_then(|a| match a.job {
                crate::construction::Job::Strip(job) => Some(job.entry()),
                _ => None,
            });
        let slots = ENTRIES
            .iter()
            .enumerate()
            // `handcuffs/should_show`.
            .filter(|(_, (_, reach))| *reach != Reach::Handcuffs || cuffs.is_some())
            .map(|(i, &(key, reach))| {
                let item = item_in(inventory, cuffs, reach);
                let obscured = obscuring(inventory, reach, item, |e| {
                    clothing_flags(clothing.get(e).ok())
                });
                // An inaccessible slot still shows its item.
                let shown = matches!(obscured, OBSCURING_NONE | OBSCURING_INACCESSIBLE);
                let item = item.filter(|_| shown).map(|e| {
                    let (appearance, dm) = items.get(e).unwrap_or((None, None));
                    StripItem {
                        name: dm.map(|n| n.name.clone()).unwrap_or_default(),
                        icon: appearance.map(|a| HudIcon {
                            icon_state: a.icon_state,
                            color: a.color,
                        }),
                    }
                });
                StripSlot {
                    key: key.to_owned(),
                    item,
                    obscured,
                    interacting: working == Some(i as u8),
                }
            })
            .collect();
        StripMenuData {
            name: name.map(|n| n.as_str().to_owned()).unwrap_or_default(),
            items: slots,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<u8, UiReject> {
        if action != STRIP_USE {
            return Err(UiReject::UnknownAction);
        }
        let key = params.get("key").unwrap_or_default();
        ENTRIES
            .iter()
            .position(|(k, _)| *k == key)
            .map(|i| i as u8)
            .ok_or_else(|| UiReject::BadParams("key: not a strippable slot".into()))
    }
}

pub struct StripServerPlugin;

impl Plugin for StripServerPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<StripMenuUi>().add_systems(
            Update,
            (
                strip_status.in_set(UiSystems::StatusHook),
                strip_acts.in_set(UiSystems::Handle),
            ),
        );
    }
}

/// tg `/datum/element/strippable/mouse_drop_onto` for `owner` dragged onto `user`: opens the
/// menu. The caller checked that `user` can act on `owner` (adjacent, conscious, not
/// incapacitated). True when `owner` is strippable (the drop is handled).
pub fn open_menu(world: &mut World, client: Entity, user: Entity, owner: Entity) -> bool {
    // tg gives `strippable` to carbons only: a cyborg's slots hold its model, modules and ID.
    if user == owner
        || world.get::<Inventory>(owner).is_none()
        || world.get::<crate::silicon::cyborg::Cyborg>(owner).is_some()
    {
        return false;
    }
    let title = format!("Stripping {}", name_of(world, owner));
    world.write_message(OpenUi::new::<StripMenuUi>(client, user, owner, title));
    true
}

/// tg `/datum/strip_menu/ui_status`: `ui_status_user_is_adjacent` (closed beyond reach),
/// `ui_status_user_has_free_hands` (update-only with blocked hands), `ui_status_user_is_abled`
/// (disabled when incapacitated or not conscious).
fn strip_status(
    mut windows: Query<&mut UiWindow>,
    positions: Query<&TilePos>,
    mobs: Query<(
        Option<&Stat>,
        Option<&crate::status_effects::Incapacitation>,
    )>,
) {
    for mut window in &mut windows {
        if window.interface != StripMenuUi::NAME {
            continue;
        }
        let adjacent = match (positions.get(window.actor), positions.get(window.source)) {
            (Ok(a), Ok(b)) => a.chebyshev(*b).is_some_and(|d| d <= 1),
            _ => false,
        };
        if !adjacent {
            window.cap_status(UiStatus::Close);
            continue;
        }
        let (stat, traits) = mobs.get(window.actor).unwrap_or((None, None));
        let traits = traits.copied().unwrap_or_default();
        if stat != Some(&Stat::Conscious) || traits.incapacitated {
            window.cap_status(UiStatus::Disabled);
        } else if traits.hands_blocked {
            window.cap_status(UiStatus::Update);
        }
    }
}

/// tg `/datum/strip_menu/ui_act` `use`, for acts that passed the window's status this tick.
fn strip_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<StripMenuUi>>>) {
    let acts: Vec<(Entity, Entity, Entity, u8)> = acts
        .read(world.resource::<Messages<UiActed<StripMenuUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act))
        .collect();
    for (client, user, owner, entry) in acts {
        use_slot(world, client, user, owner, entry);
    }
}

fn use_slot(world: &mut World, client: Entity, user: Entity, owner: Entity, entry: u8) {
    let Some(&(_, reach)) = ENTRIES.get(usize::from(entry)) else {
        return;
    };
    let Some(inventory) = world.get::<Inventory>(owner).cloned() else {
        return;
    };
    let cuffs = world.get::<Handcuffed>(owner).copied();
    if reach == Reach::Handcuffs && cuffs.is_none() {
        return;
    }
    if world.get::<DoAfter>(user).is_some() {
        return;
    }
    // `ui_act` `use`: nothing on a slot the clothing hides or covers.
    let current = item_in(&inventory, cuffs.as_ref(), reach);
    let obscured = obscuring(&inventory, reach, current, |e| {
        clothing_flags(world.get::<DmClothing>(e))
    });
    if matches!(obscured, OBSCURING_COMPLETELY | OBSCURING_INACCESSIBLE) {
        return;
    }
    let held = world
        .get::<Inventory>(user)
        .and_then(Inventory::active_item);
    let actor = Actor {
        client,
        mob: user,
        item: held,
    };
    let Some(&at) = world.get::<TilePos>(owner) else {
        return;
    };
    let (who, whom) = (name_of(world, user), name_of(world, owner));
    match item_in(&inventory, cuffs.as_ref(), reach) {
        // `strippable_item/start_unequip`: `TRAIT_NODROP` (tg "You can't remove ...").
        Some(item) if crate::inventory::no_drop(world, item) => {
            let what = name_of(world, item);
            tell(
                world,
                client,
                Span::Warning,
                format!("Вы не можете снять {what}, оно прилипло!"),
            );
        }
        Some(item) => {
            let what = name_of(world, item);
            let (delay, hidden) = if is_pocket(reach) {
                (POCKET_STRIP_DELAY_DS, true)
            } else {
                (STRIP_DELAY_DS, false)
            };
            let job = crate::construction::Job::Strip(StripJob::Unequip { entry, item });
            if !do_after(world, actor, owner, delay, 0, job) {
                return;
            }
            if hidden {
                // `pocket/start_unequip`.
                let side = if reach == Reach::Slot(InventorySlot::LeftPocket) {
                    "левый"
                } else {
                    "правый"
                };
                tell(
                    world,
                    client,
                    Span::Notice,
                    format!("Вы пытаетесь опустошить {side} карман {whom}."),
                );
                return;
            }
            // `strippable_item/start_unequip`.
            show(
                world,
                at,
                DEFAULT_MESSAGE_RANGE,
                ChatChannel::Combat,
                (
                    Span::Warning,
                    format!("{who} пытается снять {what} у {whom}."),
                ),
                &[
                    (
                        owner,
                        Span::BoldWarning,
                        format!("{who} пытается снять ваш {what}."),
                    ),
                    (
                        user,
                        Span::Danger,
                        format!("Вы пытаетесь снять {what} у {whom}..."),
                    ),
                ],
            );
        }
        None => {
            let Some(held) = held else {
                return;
            };
            if !fits(world, owner, reach, held) {
                let what = name_of(world, held);
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!("{what} не подходит сюда!"),
                );
                return;
            }
            let delay = if is_pocket(reach) {
                POCKET_EQUIP_DELAY_DS
            } else {
                EQUIP_DELAY_OTHER_DS
            };
            let job = crate::construction::Job::Strip(StripJob::Equip { entry, item: held });
            if !do_after(world, actor, owner, delay, 0, job) {
                return;
            }
            // `item_start_equip` (hands: `show_visible_message = FALSE`).
            let hand = matches!(
                reach,
                Reach::Slot(InventorySlot::LeftHand | InventorySlot::RightHand)
            );
            if !hand {
                let what = name_of(world, held);
                show(
                    world,
                    at,
                    DEFAULT_MESSAGE_RANGE,
                    ChatChannel::Combat,
                    (
                        Span::Warning,
                        format!("{who} пытается надеть {what} на {whom}."),
                    ),
                    &[
                        (
                            owner,
                            Span::BoldWarning,
                            format!("{who} пытается надеть на вас {what}."),
                        ),
                        (
                            user,
                            Span::Notice,
                            format!("Вы пытаетесь надеть {what} на {whom}..."),
                        ),
                    ],
                );
            }
        }
    }
    refresh(world, client, user, owner);
}

/// `try_equip`: handcuffs take nothing; a hand must be free (`can_put_in_hand`); a slot follows
/// `mob_can_equip`.
fn fits(world: &World, owner: Entity, reach: Reach, item: Entity) -> bool {
    let Some(inventory) = world.get::<Inventory>(owner) else {
        return false;
    };
    match reach {
        Reach::Handcuffs => false,
        Reach::Slot(slot @ (InventorySlot::LeftHand | InventorySlot::RightHand)) => {
            inventory.slot(slot).is_none()
        }
        Reach::Slot(slot) => can_equip(world, owner, inventory, item, slot),
    }
}

/// The window shows the new state at once (tgui `SStgui.update_uis` after the act).
fn refresh(world: &mut World, client: Entity, user: Entity, owner: Entity) {
    world.write_message(OpenUi::new::<StripMenuUi>(client, user, owner, ""));
}

/// End of a [`StripJob`] `do_after` (`crate::tools` checked that neither moved and the user
/// holds what it held). The rest of tg's `ui_act` after the yield is checked here.
pub fn complete(world: &mut World, user: Entity, action: &DoAfter, job: StripJob) {
    let owner = action.target;
    let Some(&(_, reach)) = ENTRIES.get(usize::from(job.entry())) else {
        return;
    };
    let (Some(inventory), Some(&pos), Some(&user_pos)) = (
        world.get::<Inventory>(owner).cloned(),
        world.get::<TilePos>(owner),
        world.get::<TilePos>(user),
    ) else {
        return;
    };
    // `user.Adjacent(owner)`.
    if !user_pos.chebyshev(pos).is_some_and(|d| d <= 1) {
        return;
    }
    let cuffs = world.get::<Handcuffed>(owner).copied();
    let current = item_in(&inventory, cuffs.as_ref(), reach);
    match job {
        StripJob::Unequip { item, .. } => {
            // "They changed the item in the meantime".
            if current != Some(item) {
                return;
            }
            // `doStrip` -> `dropItemToGround`.
            match reach {
                Reach::Handcuffs => crate::restraints::uncuff(world, owner, pos),
                Reach::Slot(slot) => {
                    unequip(world, owner, pos, slot);
                    world.entity_mut(item).remove::<CarriedBy>().insert(pos);
                }
            }
        }
        StripJob::Equip { item, .. } => {
            // "They equipped an item in the meantime" (the item stays in the user's hand).
            let held = world
                .get::<Inventory>(user)
                .and_then(Inventory::active_item);
            if current.is_some() || held != Some(item) || !fits(world, owner, reach, item) {
                return;
            }
            let Reach::Slot(slot) = reach else {
                return;
            };
            // `temporarilyRemoveItemFromInventory`, then `equip_to_slot` / `put_in_hand`.
            world.entity_mut(item).remove::<CarriedBy>();
            world.entity_mut(item).insert(CarriedBy(owner));
            if let Some(mut inventory) = world.get_mut::<Inventory>(owner) {
                *inventory.slot_mut(slot) = Some(item);
            }
        }
    }
    let client = world.get::<PlayerMob>(user).map(|p| p.client);
    if let Some(client) = client {
        refresh(world, client, user, owner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worn_clothing_obscures_and_covers_slots() {
        use crate::clothing::cover::{HIDEEYES, HIDEGLOVES};
        let mut world = World::new();
        let helmet = world.spawn_empty().id();
        let suit = world.spawn_empty().id();
        let glasses = world.spawn_empty().id();
        let wallet = world.spawn_empty().id();
        let inventory = Inventory {
            head: Some(helmet),
            suit: Some(suit),
            eyes: Some(glasses),
            pockets: [Some(wallet), None],
            ..default()
        };
        // The helmet hides the eyes; the suit only covers the gloves.
        let flags = |e: Entity| {
            if e == helmet {
                (HIDEEYES, 0)
            } else if e == suit {
                (0, HIDEGLOVES)
            } else {
                (0, 0)
            }
        };
        let at = |slot| Reach::Slot(slot);
        assert_eq!(
            obscuring(&inventory, at(InventorySlot::Eyes), Some(glasses), flags),
            OBSCURING_COMPLETELY
        );
        assert_eq!(
            obscuring(&inventory, at(InventorySlot::Gloves), None, flags),
            OBSCURING_INACCESSIBLE
        );
        assert_eq!(
            obscuring(&inventory, at(InventorySlot::Head), Some(helmet), flags),
            OBSCURING_NONE
        );
        assert_eq!(
            obscuring(
                &inventory,
                at(InventorySlot::LeftPocket),
                Some(wallet),
                flags
            ),
            OBSCURING_HIDDEN
        );
        assert_eq!(
            obscuring(&inventory, Reach::Handcuffs, None, flags),
            OBSCURING_NONE
        );
    }

    #[test]
    fn entries_follow_tg_keys() {
        // Every key the tgui `SLOTS` table knows, once.
        let keys: Vec<&str> = ENTRIES.iter().map(|(k, _)| *k).collect();
        for key in &keys {
            assert_eq!(keys.iter().filter(|k| *k == key).count(), 1, "{key}");
        }
        assert_eq!(
            StripMenuUi::parse_act(
                STRIP_USE,
                &UiParams::new(&[("key".into(), "left_pocket".into())])
            )
            .unwrap(),
            13
        );
        assert!(
            StripMenuUi::parse_act(
                STRIP_USE,
                &UiParams::new(&[("key".into(), "legcuffs".into())])
            )
            .is_err()
        );
        assert!(StripMenuUi::parse_act("alt", &UiParams::new(&[])).is_err());
        assert!(is_pocket(ENTRIES[13].1) && is_pocket(ENTRIES[14].1) && !is_pocket(ENTRIES[15].1));
    }

    #[test]
    fn cyborgs_are_not_strippable() {
        let mut world = World::new();
        world.init_resource::<Messages<OpenUi>>();
        let client = world.spawn_empty().id();
        let user = world.spawn(Inventory::default()).id();
        let human = world.spawn(Inventory::default()).id();
        let borg = world
            .spawn((
                Inventory::default(),
                crate::silicon::cyborg::Cyborg {
                    model: None,
                    model_item: None,
                    charge: 0.0,
                    max_charge: 0.0,
                    low_power: false,
                    connected_ai: None,
                    lawupdate: false,
                },
            ))
            .id();
        assert!(open_menu(&mut world, client, user, human));
        assert!(!open_menu(&mut world, client, user, borg));
        assert_eq!(world.resource::<Messages<OpenUi>>().len(), 1);
    }
}
