//! Lockboxes (server), after tg `/obj/item/storage/lockbox` (`lockbox.dm`): a storage item that
//! starts locked (`/datum/storage/lockbox`: `STORAGE_FULLY_LOCKED`) and toggles with an ID that
//! has its access (`tool_act` -> `can_unlock` -> `toggle_locked`). The medal, mindshield and
//! clusterbang boxes are the same with their own access and contents; the cargo goody case
//! (`lockbox/order`) unlocks only for the card of the buying account.
//!
//! A locked lockbox cannot be opened, reached into, dumped or filled ([`is_locked`] gates
//! `crate::storage`); locking it closes the open views. An emag breaks the lock open for good.
// shortcut: no medal box lid (`click_alt` `open`) or medal overlays, no screentips, no examine
// line for a broken lock, no `cuffable_item`.

use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmLockbox, IconIndex};
use ss13_shared::Appearance;
use ss13_shared::chat::Span;

use crate::inventory::{IdCard, check_access, get_id};

/// The live lock of a lockbox.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Lockbox {
    pub locked: bool,
    /// tg `broken`: emagged open, no longer locks.
    pub broken: bool,
    pub req_access: Vec<String>,
    pub req_one_access: Vec<String>,
    /// `lockbox/order::buyer_account`: only a card of this bank account unlocks it.
    pub buyer_account: Option<u32>,
    icon_locked: String,
    icon_closed: String,
    icon_broken: String,
}

impl Lockbox {
    pub fn from_dm(dm: &DmLockbox) -> Self {
        Self {
            locked: true,
            broken: false,
            req_access: dm.req_access.clone(),
            req_one_access: dm.req_one_access.clone(),
            buyer_account: None,
            icon_locked: dm.icon_locked.clone(),
            icon_closed: dm.icon_closed.clone(),
            icon_broken: dm.icon_broken.clone(),
        }
    }

    /// tg `can_unlock`: the access of the card, or for an order case the card's account.
    pub fn can_unlock(&self, access: &[String], account: Option<u32>) -> bool {
        match self.buyer_account {
            Some(buyer) => account == Some(buyer),
            None => check_access(&self.req_access, &self.req_one_access, access),
        }
    }

    /// `update_icon_state` without the medal box lid.
    fn icon_state(&self) -> &str {
        if self.broken {
            &self.icon_broken
        } else if self.locked {
            &self.icon_locked
        } else {
            &self.icon_closed
        }
    }
}

/// Whether `storage` is a locked lockbox (tg `atom_storage.locked`).
pub fn is_locked(world: &World, storage: Entity) -> bool {
    world.get::<Lockbox>(storage).is_some_and(|l| l.locked)
}

/// The lock of a new lockbox (from `crate::storage::finish_item`): locked, with its sprite.
pub fn init(world: &mut World, item: Entity) {
    let Some(lock) = world
        .get::<DmLockbox>(item)
        .filter(|_| world.get::<Lockbox>(item).is_none())
        .map(Lockbox::from_dm)
    else {
        return;
    };
    world.entity_mut(item).insert(lock);
    refresh_look(world, item);
}

pub(crate) fn refresh_look(world: &mut World, item: Entity) {
    let Some(lock) = world.get::<Lockbox>(item) else {
        return;
    };
    let state = world
        .get::<DmAtom>(item)
        .zip(world.get_resource::<IconIndex>())
        .and_then(|(atom, icons)| icons.get(&atom.icon, lock.icon_state()));
    if let Some(state) = state
        && let Some(mut appearance) = world.get_mut::<Appearance>(item)
        && appearance.icon_state != state
    {
        appearance.icon_state = state;
    }
}

/// tg `lockbox/tool_act` with an ID (or a PDA holding one) on `lockbox`: toggles with access,
/// else refuses. True when the click was the lockbox's.
pub fn item_interaction(world: &mut World, client: Entity, held: Entity, lockbox: Entity) -> bool {
    let Some(lock) = world.get::<Lockbox>(lockbox).cloned() else {
        return false;
    };
    let Some(card) = get_id(world, held) else {
        return false;
    };
    // A broken lock does not lock again (tg toggles it, which re-locks an emagged box).
    if lock.broken {
        return true;
    }
    let access = world
        .get::<IdCard>(card)
        .map(|c| c.access.clone())
        .unwrap_or_default();
    let account = crate::cargo::economy::account_of(world, card);
    if !lock.can_unlock(&access, account) {
        let why = if lock.buyer_account.is_some() {
            "incorrect bank account!"
        } else {
            "в доступе отказано!"
        };
        crate::tools::tell(world, client, Span::Warning, why);
        return true;
    }
    let locked = !lock.locked;
    world
        .get_mut::<Lockbox>(lockbox)
        .expect("checked above")
        .locked = locked;
    refresh_look(world, lockbox);
    let text = if locked {
        "закрыт"
    } else {
        "открыт"
    };
    crate::tools::tell(world, client, Span::Notice, text);
    true
}

/// tg `lockbox/emag_act`: the lock breaks open. False when it was already broken.
pub fn emag_act(world: &mut World, client: Entity, lockbox: Entity) -> bool {
    {
        let Some(mut lock) = world.get_mut::<Lockbox>(lockbox) else {
            return false;
        };
        if lock.broken {
            return false;
        }
        lock.broken = true;
        lock.locked = false;
    }
    refresh_look(world, lockbox);
    crate::tools::tell(world, client, Span::Notice, "замок сломан");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_or_the_buyer_account_unlocks() {
        let dm = DmLockbox {
            req_access: vec!["armory".into()],
            ..default()
        };
        let mut lock = Lockbox::from_dm(&dm);
        assert!(lock.locked, "lockboxes start locked");
        assert!(!lock.can_unlock(&[], None));
        assert!(!lock.can_unlock(&["security".into()], Some(5)));
        assert!(lock.can_unlock(&["armory".into()], None));
        lock.buyer_account = Some(5);
        assert!(
            !lock.can_unlock(&["armory".into()], Some(6)),
            "not the buyer"
        );
        assert!(lock.can_unlock(&[], Some(5)));
    }
}
