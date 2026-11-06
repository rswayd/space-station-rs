//! Mail (`sortingmachinery.dm`, `wrap.dm`): the destination tagger, parcels and the package
//! wrapper.
// shortcut: items only: no large parcels (wrapped closets, crates, canisters), gift wrap,
// notes, barcodes or the sales tagger; taggers do not edit junction filters (pipes are under
// the floor). Add with crate wrapping and the export barcode economy.

use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmItem, DmName, DmWeight};
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::destination_tagger::{
    DestinationTaggerData, DestinationTaggerStatic, DestinationTaggerUi,
};
use ss13_tgui::server::{AlwaysState, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiWindow};

use super::DisposalJob;
use crate::inventory::{CarriedBy, Inventory};
use crate::storage::{Storage, StoredIn, put_inside};
use crate::tools::Actor;

pub const TAGGER_ROOT: &str = "/obj/item/dest_tagger";
pub const DELIVERY_ROOT: &str = "/obj/item/delivery";
pub const SMALL_PARCEL: &str = "/obj/item/delivery/small";
pub const PACKAGE_WRAP_ROOT: &str = "/obj/item/stack/package_wrap";
/// `attempt_pre_unwrap_contents`: 1.5 s.
const UNWRAP_DELAY_DS: f32 = 15.0;
/// The tagger window's title (`<Window title>`).
const TAGGER_TITLE: &str = "TagMaster 2.4";

/// `GLOB.TAGGERLOCATIONS` (`code/_globalvars/lists/disposals.dm`): tag `n` is entry `n - 1`.
pub const TAGGER_LOCATIONS: [&str; 30] = [
    "Disposals",
    "Cargo Bay",
    "QM Office",
    "Engineering",
    "CE Office",
    "Atmospherics",
    "Security",
    "HoS Office",
    "Medbay",
    "CMO Office",
    "Chemistry",
    "Research",
    "RD Office",
    "Robotics",
    "HoP Office",
    "Library",
    "Chapel",
    "Theatre",
    "Bar",
    "Kitchen",
    "Hydroponics",
    "Janitor Closet",
    "Genetics",
    "Experimentor Lab",
    "Ordnance",
    "Dormitories",
    "Virology",
    "Xenobiology",
    "Law Office",
    "Detective's Office",
];

/// `/obj/item/dest_tagger` `currTag` (0: none).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DestTagger {
    pub curr_tag: u8,
}

/// `/obj/item/delivery` `sort_tag` (0: none).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Delivery {
    pub sort_tag: u8,
}

/// `Initialize` of taggers and parcels, however they come into existence.
pub(super) fn attach(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    if is_under(&item.path, TAGGER_ROOT) {
        // The window follows the tagger's holder, not the map distance (`inventory_state`).
        commands
            .entity(add.entity)
            .insert_if_new((DestTagger::default(), AlwaysState));
    } else if is_under(&item.path, DELIVERY_ROOT) {
        commands
            .entity(add.entity)
            .insert_if_new(Delivery::default());
    }
}

/// `attack_self`: a tagger opens its window, a small parcel starts unwrapping.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world.get::<DestTagger>(item).is_some() {
        world.write_message(OpenUi::new::<DestTaggerWindow>(
            client,
            mob,
            item,
            TAGGER_TITLE.to_owned(),
        ));
        return true;
    }
    if world.get::<Delivery>(item).is_some() {
        crate::tools::tell(
            world,
            client,
            Span::Notice,
            "Вы начинаете распаковывать посылку...",
        );
        crate::tools::do_after(
            world,
            Actor {
                client,
                mob,
                item: Some(item),
            },
            mob,
            UNWRAP_DELAY_DS,
            0,
            crate::construction::Job::Disposal(DisposalJob::Unwrap { parcel: item }),
        );
        return true;
    }
    false
}

/// `delivery/small/attack_self` after the `do_after`: the contents into the hands, the parcel
/// gone.
pub(super) fn complete_unwrap(world: &mut World, mob: Entity, parcel: Entity) {
    if world.get::<CarriedBy>(parcel).is_none_or(|c| c.0 != mob) {
        return;
    }
    world.entity_mut(parcel).remove::<CarriedBy>();
    let contents: Vec<Entity> = world
        .get::<crate::storage::Stored>(parcel)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    let armed = crate::cargo::mail::is_armed(world, parcel);
    for &item in &contents {
        let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
            break;
        };
        crate::storage::put_in_hands(world, mob, &inventory, item);
    }
    world.despawn(parcel);
    crate::cargo::mail::after_unwrap(world, mob, armed, &contents);
}

/// `delivery/item_interaction` with a tagger (sets `sort_tag`) and
/// `package_wrap/interact_with_atom` on an item (wraps it). `target` is on the floor or in the
/// user's other hand.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if let (Some(tagger), Some(parcel)) = (
        world.get::<DestTagger>(held).copied(),
        world.get::<Delivery>(target).copied(),
    ) {
        if tagger.curr_tag != parcel.sort_tag {
            let Some(name) = TAGGER_LOCATIONS.get(usize::from(tagger.curr_tag).wrapping_sub(1))
            else {
                return true;
            };
            crate::tools::tell(
                world,
                client,
                Span::Notice,
                format!("*{}*", name.to_uppercase()),
            );
            world.entity_mut(target).insert(Delivery {
                sort_tag: tagger.curr_tag,
            });
        }
        return true;
    }
    let is_wrap = world
        .get::<DmItem>(held)
        .is_some_and(|i| is_under(&i.path, PACKAGE_WRAP_ROOT));
    if !is_wrap || target == held {
        return false;
    }
    let Some(item) = world.get::<DmItem>(target).cloned() else {
        // `isobj`: turfs and mobs are not wrapped at all.
        if world.get::<TilePos>(target).is_some()
            && world.get::<ss13_mapload::Turf>(target).is_none()
            && world.get::<crate::health::Health>(target).is_none()
        {
            crate::tools::tell(world, client, Span::Warning, "Нельзя завернуть!");
            return true;
        }
        return false;
    };
    if item.anchored {
        return false;
    }
    wrap(world, client, mob, held, target, &item);
    true
}

/// `can_be_package_wrapped`: not storage (boxes excepted), not parcels.
fn can_be_wrapped(world: &World, item: Entity, path: &str) -> bool {
    let storage = is_under(path, "/obj/item/storage") || world.get::<Storage>(item).is_some();
    !(storage && !is_under(path, "/obj/item/storage/box")) && !is_under(path, DELIVERY_ROOT)
}

/// tg `weight_class_to_text` (BandaStation's Russian).
fn weight_class_to_text(w_class: u8) -> &'static str {
    match w_class {
        1 => "крохотного размера",
        2 => "маленького размера",
        3 => "обычного размера",
        4 => "громоздкого размера",
        5 => "огромного размера",
        6 => "гигантского размера",
        _ => "",
    }
}

fn wrap(
    world: &mut World,
    client: Entity,
    mob: Entity,
    wrap: Entity,
    target: Entity,
    item: &DmItem,
) {
    if !can_be_wrapped(world, target, &item.path) {
        crate::tools::tell(world, client, Span::Warning, "Это нельзя завернуть!");
        return;
    }
    let held_by_user = world.get::<CarriedBy>(target).is_some_and(|c| c.0 == mob);
    let pos = if held_by_user {
        world.get::<TilePos>(mob).copied()
    } else {
        world.get::<TilePos>(target).copied()
    };
    // Inside something else (a closet, a backpack): blocked.
    let (Some(pos), false) = (pos, world.get::<StoredIn>(target).is_some()) else {
        return;
    };
    if !crate::tools::consume(world, wrap, 1) {
        return;
    }
    let Some(parcel) = crate::construction::spawn_obj(world, SMALL_PARCEL, pos) else {
        return;
    };
    put_inside(world, target, parcel);
    let size = world
        .get::<DmWeight>(target)
        .copied()
        .unwrap_or_default()
        .w_class;
    let (base, name) = if is_under(&item.path, "/obj/item/disk") {
        ("deliveryfloppy".to_owned(), "floppy disk parcel".to_owned())
    } else {
        (
            format!("deliverypackage{}", size.min(5)),
            format!("{} parcel", weight_class_to_text(size)),
        )
    };
    let icon = world.get::<DmAtom>(parcel).map(|a| a.icon.clone());
    let state = icon.and_then(|icon| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(&icon, &base))
    });
    let desc = world
        .get::<DmName>(parcel)
        .map(|n| n.desc.clone())
        .unwrap_or_default();
    world
        .entity_mut(parcel)
        .insert((DmName { name, desc }, DmWeight { w_class: size }));
    if let Some(state) = state
        && let Some(mut appearance) = world.get_mut::<Appearance>(parcel)
    {
        appearance.icon_state = state;
    }
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, parcel);
    }
    let (user, what) = (
        crate::security::name_of(world, mob),
        crate::security::name_of(world, target),
    );
    if let Some(&at) = world.get::<TilePos>(mob) {
        crate::security::show(
            world,
            at,
            crate::health::DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            (Span::Notice, format!("{user} заворачивает {what}.")),
            &[],
        );
    }
}

/// The server side of the `DestinationTagger` interface.
pub struct DestTaggerWindow;

impl UiInterface for DestTaggerWindow {
    const NAME: &'static str = DestinationTaggerUi::NAME;
    type Data = DestinationTaggerData;
    type StaticData = DestinationTaggerStatic;
}

impl ServerUi for DestTaggerWindow {
    type Source = &'static DestTagger;
    type UserParam = ();
    type Act = u8;

    fn static_data(_: &DestTagger) -> DestinationTaggerStatic {
        DestinationTaggerStatic {
            locations: TAGGER_LOCATIONS.iter().map(|&s| s.to_owned()).collect(),
        }
    }

    fn data(tagger: &DestTagger, _: Entity, _: &()) -> DestinationTaggerData {
        DestinationTaggerData {
            current_tag: tagger.curr_tag,
        }
    }

    /// `change` with `index` (`round(text2num(...))`, 1..30).
    fn parse_act(action: &str, params: &UiParams) -> Result<u8, UiReject> {
        if action != "change" {
            return Err(UiReject::UnknownAction);
        }
        let index = params.number("index")?.round();
        if !(1.0..=TAGGER_LOCATIONS.len() as f64).contains(&index) {
            return Err(UiReject::BadParams("index: 1..30".into()));
        }
        Ok(index as u8)
    }
}

pub(super) fn apply_acts(
    mut acts: MessageReader<UiActed<DestTaggerWindow>>,
    mut taggers: Query<&mut DestTagger>,
) {
    for acted in acts.read() {
        if let Ok(mut tagger) = taggers.get_mut(acted.source) {
            tagger.set_if_neq(DestTagger {
                curr_tag: acted.act,
            });
        }
    }
}

/// `inventory_state`: only while the user carries the tagger.
pub(super) fn window_status(mut windows: Query<&mut UiWindow>, carried: Query<&CarriedBy>) {
    for mut window in &mut windows {
        if window.interface == DestTaggerWindow::NAME
            && !carried
                .get(window.source)
                .is_ok_and(|c| c.0 == window.actor)
        {
            window.cap_status(ss13_tgui::UiStatus::Close);
        }
    }
}
