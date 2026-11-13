//! What worn items and tongues add to speech in the game (server), after tg
//! `/datum/component/speechmod` on items and `code/modules/surgery/organs/internal/tongue/_tongue.dm`.
//!
//! - Worn speechmods: the French beret on the head, the gondola, luchador and Italian moustache
//!   masks in the mask slot, and the crab hats (fishing hat, foam lobster head) in any slot, the
//!   hands too (tg gives them no `slots`), add their accent to the wearer's
//!   [`Accents`] while worn ([`WornAccents`]); the chat applies them (`ss13_chat::accents`).
//! - The alien tongue hisses (`SFX_HISS`) whenever its owner speaks out loud (not through a
//!   saymode).
//! - The zombie tongue growls now and then while its owner is conscious (`on_life`).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_chat::accents::{Accent, Accents, TongueSpeech};
use ss13_chat::server::{SaySystems, Spoke};
use ss13_chat::speech::{RadioTarget, SpeakerStat};
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::protocol::InventorySlot;

use crate::combat::CombatRng;
use crate::inventory::{ALL_SLOTS, Inventory};
use crate::sound::{PlaySound, playsound};

/// The speechmod items: type, the slot they work in (`None`: any), their accent.
const WORN: [(&str, Option<InventorySlot>, Accent); 6] = [
    (
        "/obj/item/clothing/head/beret/frenchberet",
        Some(InventorySlot::Head),
        Accent::French,
    ),
    (
        "/obj/item/clothing/mask/gondola",
        Some(InventorySlot::Mask),
        Accent::Spurdo,
    ),
    (
        "/obj/item/clothing/mask/luchador",
        Some(InventorySlot::Mask),
        Accent::Luchador,
    ),
    (
        "/obj/item/clothing/mask/fakemoustache/italian",
        Some(InventorySlot::Mask),
        Accent::Italian,
    ),
    (
        "/obj/item/clothing/head/soft/fishing_hat",
        None,
        Accent::Crustacean,
    ),
    (
        "/obj/item/clothing/head/costume/lobsterhat",
        None,
        Accent::Crustacean,
    ),
];

/// `SFX_HISS` of `/obj/item/organ/tongue/alien/modify_speech`.
const HISS: &str = "hiss";
/// `/obj/item/organ/tongue/zombie::spooks`.
const SPOOKS: [&str; 5] = [
    "sound/effects/hallucinations/growl1.ogg",
    "sound/effects/hallucinations/growl2.ogg",
    "sound/effects/hallucinations/growl3.ogg",
    "sound/effects/hallucinations/veryfar_noise.ogg",
    "sound/effects/hallucinations/wail.ogg",
];

/// The accents the body's worn items gave it, a part of its [`Accents`] (a mutation's stay
/// there when the item comes off).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct WornAccents(pub Vec<Accent>);

pub struct SpeechmodsPlugin;

impl Plugin for SpeechmodsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Spoke>()
            .add_message::<PlaySound>()
            .add_systems(
                Update,
                (
                    sync_worn.after(crate::inventory::inventory_intents),
                    alien_hiss.after(SaySystems),
                    zombie_spooks,
                ),
            );
    }
}

/// The accent `item` gives worn in `slot`.
pub fn worn_accent(path: &str, slot: InventorySlot) -> Option<Accent> {
    WORN.iter()
        .find(|(root, only, _)| is_under(path, root) && only.is_none_or(|s| s == slot))
        .map(|&(.., accent)| accent)
}

/// `list - other` keeping duplicates (tg `COMPONENT_DUPE_ALLOWED`: two crab hats are two
/// speechmods).
fn minus(list: &[Accent], other: &[Accent]) -> Vec<Accent> {
    let mut left = other.to_vec();
    list.iter()
        .copied()
        .filter(|a| match left.iter().position(|b| b == a) {
            Some(i) => {
                left.remove(i);
                false
            }
            None => true,
        })
        .collect()
}

/// `speechmod/on_equipped`, `on_unequipped`: the worn accents follow the inventory.
/// Cost: per changed inventory, its 17 slots.
fn sync_worn(world: &mut World) {
    let mut changed = world.query_filtered::<(Entity, &Inventory), Changed<Inventory>>();
    let mobs: Vec<(Entity, Vec<(Entity, InventorySlot)>)> = changed
        .iter(world)
        .map(|(mob, inventory)| {
            let items = ALL_SLOTS
                .iter()
                .filter_map(|&slot| inventory.slot(slot).map(|item| (item, slot)))
                .collect();
            (mob, items)
        })
        .collect();
    for (mob, items) in mobs {
        let want: Vec<Accent> = items
            .iter()
            .filter_map(|&(item, slot)| {
                let path = world.get::<DmItem>(item)?.path.as_str();
                worn_accent(path, slot)
            })
            .collect();
        let had = world
            .get::<WornAccents>(mob)
            .map(|w| w.0.clone())
            .unwrap_or_default();
        if had == want {
            continue;
        }
        let (gone, new) = (minus(&had, &want), minus(&want, &had));
        let mut accents = world.get::<Accents>(mob).cloned().unwrap_or_default();
        accents.0 = minus(&accents.0, &gone);
        accents.0.extend(new);
        let mut entity = world.entity_mut(mob);
        if accents.0.is_empty() {
            entity.remove::<Accents>();
        } else {
            entity.insert(accents);
        }
        if want.is_empty() {
            entity.remove::<WornAccents>();
        } else {
            entity.insert(WornAccents(want));
        }
    }
}

/// `/obj/item/organ/tongue/alien/modify_speech`: `playsound(owner, SFX_HISS, 25, TRUE, TRUE)`
/// unless a saymode took the words.
fn alien_hiss(world: &mut World, mut cursor: Local<MessageCursor<Spoke>>) {
    let Some(spoken) = world.get_resource::<Messages<Spoke>>() else {
        return;
    };
    let hisses: Vec<TilePos> = cursor
        .read(spoken)
        .filter(|s| {
            !matches!(
                s.radio,
                Some(RadioTarget::Binary | RadioTarget::Changeling | RadioTarget::Hive)
            )
        })
        .filter(|s| world.get::<TongueSpeech>(s.speaker) == Some(&TongueSpeech::Alien))
        .map(|s| s.pos)
        .collect();
    for pos in hisses {
        playsound(world, PlaySound::new(pos, HISS, 25.0, true).extrarange(1.0));
    }
}

/// `/obj/item/organ/tongue/zombie/on_life`: `SPT_PROB(2)` of a spooky growl while not
/// unconscious or in crit. Cost: per frame, one check per zombie-tongued body.
fn zombie_spooks(world: &mut World) {
    let secs = world.get_resource::<Time>().map_or(0.0, Time::delta_secs);
    let mut q = world.query::<(&TongueSpeech, Option<&SpeakerStat>, &TilePos)>();
    let zombies: Vec<TilePos> = q
        .iter(world)
        .filter(|(speech, stat, _)| {
            **speech == TongueSpeech::Zombie && stat.is_none_or(|s| *s == SpeakerStat::Conscious)
        })
        .map(|(.., &pos)| pos)
        .collect();
    for pos in zombies {
        let spook = {
            let mut rng = world.get_resource_or_init::<CombatRng>();
            crate::medical::wounds::spt_prob(&mut rng, 2.0, secs)
                .then(|| SPOOKS[rng.range(0, SPOOKS.len() as u32 - 1) as usize])
        };
        if let Some(spook) = spook {
            playsound(
                world,
                PlaySound::new(pos, spook, 50.0, true).extrarange(10.0),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worn_accents_need_their_slot() {
        let beret = "/obj/item/clothing/head/beret/frenchberet";
        assert_eq!(
            worn_accent(beret, InventorySlot::Head),
            Some(Accent::French)
        );
        assert_eq!(worn_accent(beret, InventorySlot::LeftHand), None);
        assert_eq!(
            worn_accent(
                "/obj/item/clothing/mask/luchador/tecnicos",
                InventorySlot::Mask
            ),
            Some(Accent::Luchador)
        );
        // The crab hats have no `slots`: held works too.
        assert_eq!(
            worn_accent(
                "/obj/item/clothing/head/costume/lobsterhat",
                InventorySlot::RightHand
            ),
            Some(Accent::Crustacean)
        );
        assert_eq!(
            worn_accent("/obj/item/clothing/mask/fakemoustache", InventorySlot::Mask),
            None
        );
    }

    #[test]
    fn minus_keeps_duplicates() {
        use Accent::*;
        assert_eq!(
            minus(&[Crustacean, Hulk, Crustacean], &[Crustacean]),
            vec![Hulk, Crustacean]
        );
        assert_eq!(minus(&[French], &[French, Spurdo]), Vec::<Accent>::new());
    }
}
