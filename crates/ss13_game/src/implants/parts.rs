//! The implants whose state lives on the implant object in tg and so get an entity of their own
//! here, spawned at implantation and despawned at removal ([`ImplantPart`], a child of the host
//! so it goes with the host's despawn): the storage implant's bluespace pocket (a
//! `crate::storage` container, `/datum/storage/implant`) and the internal radio implant's radio
//! (a `crate::radio::BuiltInRadio` heard by the host alone). The other implants stay data
//! ([`super::Implant`]).

use bevy::prelude::*;
use ss13_chat::radio::{
    FREQ_COMMON, RADIO_CHANNEL_SCIENCE, RADIO_CHANNEL_SUPPLY, RADIO_CHANNEL_SYNDICATE,
    RADIO_SPECIAL_SYNDIE,
};
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmName, DmWeight};
use ss13_shared::chat::Span;
use ss13_tgui::server::{AlwaysState, OpenUi};

use crate::health::DamageType;
use crate::radio::{BuiltInRadio, ImplantRadio, Radio, RadioKind, RadioTuning};
use crate::storage::{Storage, Stored};
use crate::zone::BodyZone;

pub const RADIO_MINING: &str = "/obj/item/implant/radio/mining";
pub const RADIO_SYNDICATE: &str = "/obj/item/implant/radio/syndicate";
pub const RADIO_SLIME: &str = "/obj/item/implant/radio/slime";
const NUKE_DISK: &str = "/obj/item/disk/nuclear";

/// `/datum/storage/implant`: `max_slots`, `max_total_storage` (and the default
/// `max_specific_storage`, `WEIGHT_CLASS_NORMAL`).
const POCKET_SLOTS: u32 = 2;
const POCKET_TOTAL: u32 = 6;
const POCKET_SPECIFIC: u8 = 3;
/// `storage/max_slot_stacking`.
const MAX_SLOT_STACKING: u32 = 4;
/// `storage/removed`: `apply_damage(20, BRUTE, BODY_ZONE_CHEST)`.
const POCKET_RUPTURE: f32 = 20.0;

/// The entity of an implant in `host` (the implant object of tg, `imp_in`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImplantPart {
    pub host: Entity,
}

/// The host of an implant entity.
pub fn host_of(world: &World, part: Entity) -> Option<Entity> {
    world.get::<ImplantPart>(part).map(|p| p.host)
}

/// `storage/implant`: `create_storage(/datum/storage/implant)` on the new implant.
pub(super) fn spawn_pocket(world: &mut World, host: Entity) -> Entity {
    world
        .spawn((
            ImplantPart { host },
            Storage {
                max_slots: POCKET_SLOTS,
                max_specific_storage: POCKET_SPECIFIC,
                max_total_storage: POCKET_TOTAL,
            },
            // `allow_big_nesting`: any storage item that fits goes in.
            DmWeight { w_class: u8::MAX },
            DmName {
                name: "имплант-хранилище".to_owned(),
                desc: "Хранит до двух больших предметов в блюспейс-кармане.".to_owned(),
            },
            ChildOf(host),
        ))
        .id()
}

/// `storage/implant` with one already in the host: its pocket grows by the initial size up to
/// [`MAX_SLOT_STACKING`] slots (the new implant is used up); false when it is at the limit.
pub(super) fn grow_pocket(world: &mut World, pocket: Entity) -> bool {
    let Some(mut storage) = world.get_mut::<Storage>(pocket) else {
        return false;
    };
    if storage.max_slots >= MAX_SLOT_STACKING {
        return false;
    }
    storage.max_slots += POCKET_SLOTS;
    storage.max_total_storage += POCKET_TOTAL;
    true
}

/// `/datum/storage/implant` `set_holdable(cant_hold_list = /obj/item/disk/nuclear)`.
pub fn pocket_refuses(world: &World, storage: Entity, item: Entity) -> bool {
    world.get::<ImplantPart>(storage).is_some()
        && world
            .get::<ss13_mapload::DmItem>(item)
            .is_some_and(|i| is_under(&i.path, NUKE_DISK))
}

/// `storage/removed`: the contents, bloodied, fall out at the host's feet, the host takes 20
/// brute to the chest and the pocket is gone.
pub(super) fn rupture_pocket(world: &mut World, host: Entity, pocket: Entity) {
    let items: Vec<Entity> = world
        .get::<Stored>(pocket)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    let dna: Vec<_> = crate::forensics::server::blood_dna_of(world, host)
        .into_iter()
        .collect();
    let pos = crate::explosions::turf_of(world, host);
    for item in items {
        crate::forensics::server::add_blood_dna(world, item, &dna);
        match pos {
            Some(pos) => crate::storage::take_out(world, item, pos),
            None => {
                world.despawn(item);
            }
        }
    }
    let host_name = crate::medical::name_of(world, host);
    let text = format!(
        "Вокруг импланта-хранилища раскрывается блюспейс-карман, когда тот покидает \
         {host_name}, извергая содержимое и разрывая окружающие ткани!"
    );
    crate::medical::visible(world, host, &text, &text, Span::Warning);
    crate::medical::hurt_limb(
        world,
        host,
        BodyZone::Chest,
        DamageType::Brute,
        POCKET_RUPTURE,
    );
    world.despawn(pocket);
}

/// `/obj/item/implant/radio/Initialize`: a radio with `canhear_range` 0 (only the host hears
/// it), the subtype's key and `subspace_transmission`; the host's `;`, `:h` and the key's
/// channels go into it (`/mob/living/radio`).
pub(super) fn spawn_radio(world: &mut World, host: Entity, path: &str) -> Entity {
    let (channel, subspace, syndie) = if is_under(path, RADIO_MINING) {
        (Some(RADIO_CHANNEL_SUPPLY), false, false)
    } else if is_under(path, RADIO_SYNDICATE) {
        (Some(RADIO_CHANNEL_SYNDICATE), true, true)
    } else if is_under(path, RADIO_SLIME) {
        (Some(RADIO_CHANNEL_SCIENCE), true, false)
    } else {
        (None, false, false)
    };
    let radio = Radio {
        // Only headsets are subspace-only here (`Radio::subspace`).
        kind: if subspace {
            RadioKind::Headset
        } else {
            RadioKind::Other
        },
        on: true,
        frequency: FREQ_COMMON,
        // A headset talks only while broadcasting; a plain radio talks regardless.
        broadcasting: subspace,
        listening: true,
        canhear_range: 0,
        special_channels: if syndie { RADIO_SPECIAL_SYNDIE } else { 0 },
        channels: channel.map(|c| (c.to_owned(), true)).into_iter().collect(),
        tx_cut: false,
    };
    let entity = world
        .spawn((
            ImplantPart { host },
            radio,
            RadioTuning::default(),
            // `GLOB.deep_inventory_state`: the window follows the host (`crate::radio`).
            AlwaysState,
            BuiltInRadio { listener: host },
            DmName {
                name: "внутреннее радио".to_owned(),
                desc: String::new(),
            },
            ChildOf(host),
        ))
        .id();
    world.entity_mut(host).insert(ImplantRadio(entity));
    world.entity_mut(host).insert(ss13_chat::speech::EarsWorn);
    entity
}

/// The radio implant out: its radio is gone and the host's ears are its headset's again.
pub(super) fn remove_radio(world: &mut World, host: Entity, radio: Entity) {
    world.despawn(radio);
    if world
        .get::<ImplantRadio>(host)
        .is_some_and(|r| r.0 == radio)
    {
        world.entity_mut(host).remove::<ImplantRadio>();
        let ears = world
            .get::<crate::inventory::Inventory>(host)
            .is_some_and(|i| i.ears.is_some());
        if !ears {
            world
                .entity_mut(host)
                .remove::<ss13_chat::speech::EarsWorn>();
        }
    }
}

/// `radio/activate`: the radio's window (`deep_inventory_state`).
pub(super) fn open_radio(world: &mut World, client: Entity, host: Entity, radio: Entity) {
    world.write_message(OpenUi::new::<crate::radio_ui::RadioUi>(
        client,
        host,
        radio,
        "внутреннее радио",
    ));
}
