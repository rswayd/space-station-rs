//! Summon Events' item events (`code/modules/events/wizard/`), wizard mode or forced only.
//!
//! - Make Everything Embeddable / Sticky (`embeddies.dm`, `GLOB.global_funny_embedding`): every
//!   item that does not embed yet, and every item made afterwards, gets the "pointy" (or
//!   harmless "sticky") `/datum/embedding/global_funny` with `ignore_throwspeed_threshold` and
//!   the prefix in its name ([`FunnyEmbedding`]); the thrown-item embedding is
//!   `crate::embedding`. Neither runs while one of them is on.
//! - Cursed Items (`curseditems.dm`): one random set (a trippy rollie, boxing gloves and a
//!   luchador mask, kitty ears, a cursed katana, a chameleon mask or a wizard costume) on every
//!   living human, replacing what was in those slots; the items cannot be dropped
//!   (`TRAIT_NODROP`), vanish once off the body (`DROPDEL`) and are named "cursed ...". Humans in
//!   space, plasmamen (`outfit_important_for_life`) and, for sets that ruin a space suit, humans
//!   off the station levels are spared; wizards keep their outfit for the kitty ears. A puff of
//!   smoke on every victim.
//! - Greentext (`greentext.dm`): the greentext appears under a random player human. Whoever
//!   holds or wears it turns green and is cursed; losing it turns them red. Holding it on
//!   CentCom when the round ends wins ("winner", objective "Succeed"): the item is gone and the
//!   cursed are freed with "A dark temptation has passed from this world!" (to everyone unless
//!   the greentext is `quiet`).
// shortcut: no RPG Loot (the fantasy component with its affixes and the fortification scrolls
// is not ported). Cursed Items does not change the kitty ears' victims' gender (pronouns are not
// ported). The greentext's warning for holders with other objectives and its point of interest
// are missing; a destroyed greentext frees its victims the next tick.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmEmbed, DmItem, DmName, EmbedData};
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::InventorySlot;

use super::controller::{EventRng, EventState, RoundEvent, station_levels};
use crate::health::Stat;
use crate::human::HumanLook;
use crate::inventory::{ALL_SLOTS, CarriedBy, Inventory, NoDrop, slot_bit};
use crate::server::PlayerMob;
use crate::tools::{tell, tell_mob};

pub const EMBEDPOCALYPSE: &str = "Make Everything Embeddable";
pub const STICKY: &str = "Make Everything Sticky";
pub const CURSED_ITEMS: &str = "Cursed Items";
pub const GREENTEXT: &str = "Greentext";

pub const GREENTEXT_PATH: &str = "/obj/item/greentext";
const GREENTEXT_QUIET: &str = "/obj/item/greentext/quiet";
const GREEN: [u8; 4] = [0x00, 0xff, 0x00, 0xff];
const RED: [u8; 4] = [0xff, 0x00, 0x00, 0xff];

/// The paths these events spawn.
pub const RUNTIME_TYPES: [&str; 10] = [
    GREENTEXT_PATH,
    "/obj/item/cigarette/rollie/trippy",
    "/obj/item/clothing/mask/luchador",
    "/obj/item/clothing/gloves/boxing",
    "/obj/item/clothing/head/costume/kitty",
    "/obj/item/katana/cursed",
    "/obj/item/clothing/mask/chameleon",
    "/obj/item/clothing/suit/wizrobe",
    "/obj/item/clothing/shoes/sandal/magic",
    "/obj/item/clothing/head/wizard",
];

/// `GLOB.global_funny_embedding`: on while the resource exists.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FunnyEmbedding {
    pub sticky: bool,
}

impl FunnyEmbedding {
    fn prefix(self) -> &'static str {
        if self.sticky { "sticky" } else { "pointy" }
    }

    /// `/datum/embedding/global_funny` (and `/sticky`) over the `/datum/embedding` defaults.
    pub fn data(self) -> EmbedData {
        EmbedData {
            path: if self.sticky {
                "/datum/embedding/global_funny/sticky"
            } else {
                "/datum/embedding/global_funny"
            }
            .to_owned(),
            embed_chance: 45.0,
            fall_chance: 5.0,
            pain_chance: 15.0,
            pain_mult: if self.sticky { 0.0 } else { 2.0 },
            impact_pain_mult: 4.0,
            remove_pain_mult: 6.0,
            rip_time: 30.0,
            ignore_throwspeed_threshold: true,
            jostle_chance: 5.0,
            jostle_pain_mult: if self.sticky { 0.0 } else { 1.0 },
            pain_stam_pct: 0.0,
            ignores_pierce_immunity: false,
            stealthy_embed: false,
        }
    }
}

/// On a Cursed Items item: `DROPDEL`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct CursedItem;

/// The greentexts and the mobs they cursed (`color_altered_mobs`, `TRAIT_GREENTEXT_CURSED`).
#[derive(Resource, Clone, Debug, Default)]
pub struct Greentexts(pub Vec<GreentextState>);

#[derive(Clone, Debug, PartialEq)]
pub struct GreentextState {
    pub item: Entity,
    pub holder: Option<Entity>,
    pub victims: Vec<Entity>,
    pub quiet: bool,
}

pub(super) fn build(app: &mut App) {
    app.init_resource::<Greentexts>().add_systems(
        Update,
        (
            bless_new_items.run_if(resource_exists::<FunnyEmbedding>),
            init_greentexts,
            watch_greentexts,
            cursed_dropdel.run_if(any_with_component::<CursedItem>),
        )
            .chain(),
    );
}

pub fn make_embedpocalypse(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Embedpocalypse { sticky: false })
}

pub fn make_sticky(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Embedpocalypse { sticky: true })
}

/// `can_spawn_event`: not while either is on.
pub fn embedding_can_run(world: &mut World) -> bool {
    !world.contains_resource::<FunnyEmbedding>()
}

struct Embedpocalypse {
    sticky: bool,
}

impl RoundEvent for Embedpocalypse {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let funny = FunnyEmbedding {
            sticky: self.sticky,
        };
        world.insert_resource(funny);
        // `handle_current_items`.
        let mut query = world.query_filtered::<Entity, With<DmItem>>();
        let mut items: Vec<Entity> = query.iter(world).collect();
        items.sort();
        for item in items {
            bless(world, item, funny);
        }
    }
}

/// An item that does not embed yet gets the funny embedding and the prefix.
pub fn bless(world: &mut World, item: Entity, funny: FunnyEmbedding) {
    if crate::embedding::embed_of(world, item).is_some() {
        return;
    }
    let data = funny.data();
    match world.get_mut::<DmEmbed>(item) {
        Some(mut embed) => embed.data = data,
        None => {
            world.entity_mut(item).insert(DmEmbed { data, ..default() });
        }
    }
    let name = crate::security::name_of(world, item);
    rename(world, item, &format!("{} {name}", funny.prefix()));
}

/// The name shown for `e` (a [`Name`] when it has one, else its [`DmName`]).
fn rename(world: &mut World, e: Entity, name: &str) {
    if let Some(mut dm) = world.get_mut::<DmName>(e) {
        name.clone_into(&mut dm.name);
    }
    if world.get::<Name>(e).is_some() || world.get::<DmName>(e).is_none() {
        world.entity_mut(e).insert(Name::new(name.to_owned()));
    }
}

/// `COMSIG_GLOB_NEW_ITEM` of the funny embedding.
fn bless_new_items(world: &mut World, mut added: Local<QueryState<Entity, Added<DmItem>>>) {
    let funny = *world.resource::<FunnyEmbedding>();
    let mut items: Vec<Entity> = added.iter(world).collect();
    items.sort();
    for item in items {
        bless(world, item, funny);
    }
}

pub fn make_cursed_items(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(CursedItems)
}

/// One `item_set`: its loadout, `ruins_spaceworthiness`, `ruins_wizard_loadout`.
struct CursedSet {
    loadout: &'static [&'static str],
    ruins_spaceworthiness: bool,
    ruins_wizard_loadout: bool,
}

/// `BIG_FAT_DOOBIE`, `BOXING`, `CATGIRLS_2015`, `CURSED_SWORDS`, `VOICE_MODULATORS`,
/// `WIZARD_MIMICRY`, in tg's `pick` order.
const CURSED_SETS: [CursedSet; 6] = [
    CursedSet {
        loadout: &["/obj/item/cigarette/rollie/trippy"],
        ruins_spaceworthiness: true,
        ruins_wizard_loadout: false,
    },
    CursedSet {
        loadout: &[
            "/obj/item/clothing/mask/luchador",
            "/obj/item/clothing/gloves/boxing",
        ],
        ruins_spaceworthiness: true,
        ruins_wizard_loadout: false,
    },
    CursedSet {
        loadout: &["/obj/item/clothing/head/costume/kitty"],
        ruins_spaceworthiness: true,
        ruins_wizard_loadout: true,
    },
    CursedSet {
        loadout: &["/obj/item/katana/cursed"],
        ruins_spaceworthiness: false,
        ruins_wizard_loadout: false,
    },
    CursedSet {
        loadout: &["/obj/item/clothing/mask/chameleon"],
        ruins_spaceworthiness: false,
        ruins_wizard_loadout: false,
    },
    CursedSet {
        loadout: &[
            "/obj/item/clothing/suit/wizrobe",
            "/obj/item/clothing/shoes/sandal/magic",
            "/obj/item/clothing/head/wizard",
        ],
        ruins_spaceworthiness: true,
        ruins_wizard_loadout: false,
    },
];

struct CursedItems;

impl RoundEvent for CursedItems {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let i = world
            .resource_mut::<EventRng>()
            .0
            .range(0, CURSED_SETS.len() as i64 - 1) as usize;
        let set = &CURSED_SETS[i];
        let levels = station_levels(world);
        let mut query =
            world.query_filtered::<(Entity, &Stat, &TilePos), (With<HumanLook>, With<Inventory>)>();
        let mut humans: Vec<(Entity, TilePos)> = query
            .iter(world)
            .filter(|(_, stat, _)| **stat != Stat::Dead)
            .map(|(e, _, pos)| (e, *pos))
            .collect();
        humans.sort_by_key(|(e, _)| *e);
        let mut victims = Vec::new();
        for (mob, pos) in humans {
            // #savetheminers
            if crate::construction::is_space(world, pos)
                || crate::species::def_of(world, mob).outfit.is_some()
                || (set.ruins_spaceworthiness && !levels.contains(&pos.z))
            {
                continue;
            }
            if set.ruins_wizard_loadout
                && world
                    .get::<crate::antag::wizard::server::Wizard>(mob)
                    .is_some()
            {
                continue;
            }
            for path in set.loadout {
                curse(world, mob, pos, path);
            }
            victims.push(pos);
        }
        for pos in victims {
            crate::grenades::do_smoke(world, pos, 0);
        }
    }
}

/// The non-hand slot a clothing item's `slot_flags` name first.
fn clothing_slot(slot_flags: u32) -> Option<InventorySlot> {
    ALL_SLOTS.into_iter().find(|&slot| {
        !matches!(slot, InventorySlot::LeftHand | InventorySlot::RightHand)
            && slot_bit(slot) & slot_flags != 0
    })
}

/// One cursed item onto `mob`: what was in its slot falls (`dropItemToGround(force)`); a hand
/// item without a free hand is deleted (`equip_to_slot_or_del`).
fn curse(world: &mut World, mob: Entity, pos: TilePos, path: &str) -> Option<Entity> {
    let Some(item) = crate::construction::spawn_detached(world, path) else {
        warn!("{CURSED_ITEMS}: no {path} prototype (rerun `ss13-mapload convert`)");
        return None;
    };
    let slot_flags = world.get::<DmItem>(item).map_or(0, |d| d.slot_flags);
    let inventory = world.get::<Inventory>(mob)?.clone();
    let slot = if is_under(path, "/obj/item/clothing") {
        clothing_slot(slot_flags)
    } else {
        [inventory.active, 1 - inventory.active]
            .into_iter()
            .find(|&h| inventory.hands[h].is_none())
            .map(|h| ALL_SLOTS[h])
    };
    let Some(slot) = slot else {
        world.despawn(item);
        return None;
    };
    if let Some(old) = inventory.slot(slot) {
        crate::inventory::unequip(world, mob, pos, slot);
        crate::inventory::put_down(world, old, pos);
    }
    world
        .entity_mut(item)
        .insert((Networked, CarriedBy(mob), NoDrop, CursedItem));
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        *inventory.slot_mut(slot) = Some(item);
    }
    let name = crate::security::name_of(world, item);
    rename(world, item, &format!("cursed {name}"));
    crate::inventory::refresh_look(world, mob);
    Some(item)
}

/// `DROPDEL`: a cursed item off its body is gone.
fn cursed_dropdel(
    items: Query<Entity, (With<CursedItem>, Without<CarriedBy>)>,
    mut commands: Commands,
) {
    for item in &items {
        commands.entity(item).despawn();
    }
}

pub fn make_greentext(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(GreentextEvent)
}

struct GreentextEvent;

impl RoundEvent for GreentextEvent {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let mut query = world.query_filtered::<(Entity, &PlayerMob, &TilePos), With<HumanLook>>();
        let mut players: Vec<(Entity, Entity, TilePos)> = query
            .iter(world)
            .map(|(e, p, pos)| (e, p.client, *pos))
            .collect();
        players.sort_by_key(|(e, _, _)| *e);
        let Some(&(_, client, pos)) = world.resource_mut::<EventRng>().pick(&players) else {
            return;
        };
        if crate::construction::spawn_obj(world, GREENTEXT_PATH, pos).is_none() {
            warn!("{GREENTEXT}: no {GREENTEXT_PATH} prototype (rerun `ss13-mapload convert`)");
            return;
        }
        tell(
            world,
            client,
            Span::GreenText,
            "The mythical greentext appear at your feet! Pick it up if you dare...",
        );
    }
}

/// Every new greentext (the event's or an admin's) is tracked.
fn init_greentexts(
    world: &mut World,
    mut added: Local<QueryState<(Entity, &'static DmItem), Added<DmItem>>>,
) {
    let new: Vec<(Entity, bool)> = added
        .iter(world)
        .filter(|(_, dm)| is_under(&dm.path, GREENTEXT_PATH))
        .map(|(e, dm)| (e, is_under(&dm.path, GREENTEXT_QUIET)))
        .collect();
    if new.is_empty() {
        return;
    }
    let mut greentexts = world.resource_mut::<Greentexts>();
    for (item, quiet) in new {
        greentexts.0.push(GreentextState {
            item,
            holder: None,
            victims: Vec::new(),
            quiet,
        });
    }
}

fn set_color(world: &mut World, mob: Entity, color: Option<[u8; 4]>) {
    if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(mob)
        && look.color != color
    {
        look.color = color;
    }
}

fn cursed(states: &[GreentextState], mob: Entity) -> bool {
    states.iter().any(|g| g.victims.contains(&mob))
}

/// `equipped`/`dropped` as the greentext changes hands, and `Destroy`'s release of a gone one.
fn watch_greentexts(world: &mut World) {
    let mut states = std::mem::take(&mut world.resource_mut::<Greentexts>().0);
    let mut i = 0;
    while i < states.len() {
        let item = states[i].item;
        if world.get_entity(item).is_err() {
            let state = states.remove(i);
            release(world, &state.victims, state.quiet, &states);
            continue;
        }
        let holder = world
            .get::<CarriedBy>(item)
            .map(|c| c.0)
            .filter(|&m| world.get::<Stat>(m).is_some());
        let old = states[i].holder;
        if holder != old {
            if let Some(old) = old {
                // `dropped`.
                if cursed(&states, old) {
                    tell_mob(
                        world,
                        old,
                        Span::Warning,
                        "A sudden wave of failure washes over you...",
                    );
                    set_color(world, old, Some(RED));
                }
            }
            if let Some(new) = holder {
                // `equipped`.
                tell_mob(
                    world,
                    new,
                    Span::GreenText,
                    "So long as you leave this place with greentext in hand you know will be \
                     happy...",
                );
                if !cursed(&states, new) {
                    states[i].victims.push(new);
                }
                set_color(world, new, Some(GREEN));
            }
            states[i].holder = holder;
        }
        i += 1;
    }
    let mut greentexts = world.resource_mut::<Greentexts>();
    states.append(&mut greentexts.0);
    greentexts.0 = states;
}

/// `release_victims`: the announcement (to the victims only when `quiet`, else to every
/// player) and the colors back for those no other greentext curses.
fn release(world: &mut World, victims: &[Entity], quiet: bool, others: &[GreentextState]) {
    let listeners: Vec<Entity> = if quiet {
        victims.to_vec()
    } else {
        let mut q = world.query::<(Entity, &PlayerMob)>();
        let mut all: Vec<Entity> = q.iter(world).map(|(e, _)| e).collect();
        all.sort();
        all
    };
    for mob in listeners {
        tell_mob(
            world,
            mob,
            Span::Warning,
            "A dark temptation has passed from this world!",
        );
        if victims.contains(&mob) {
            tell_mob(
                world,
                mob,
                Span::GreenText,
                "You're finally able to forgive yourself...",
            );
        }
    }
    for &mob in victims {
        if !cursed(others, mob) {
            set_color(world, mob, None);
        }
    }
}

/// `check_winner` of every greentext at round end (tg `SSticker.OnRoundend`), with the
/// "winner" antagonists' report lines.
pub fn roundend_report(world: &mut World) -> Vec<ChatMessage> {
    let Some(states) = world
        .get_resource_mut::<Greentexts>()
        .map(|mut g| std::mem::take(&mut g.0))
    else {
        return Vec::new();
    };
    let mut winners = Vec::new();
    let mut kept = Vec::new();
    for state in states {
        let won = state.holder.filter(|&h| {
            world.get_entity(state.item).is_ok() && crate::shuttle::escape_status(world, h).0
        });
        let Some(holder) = won else {
            kept.push(state);
            continue;
        };
        let victims: Vec<Entity> = state
            .victims
            .iter()
            .copied()
            .filter(|&v| v != holder)
            .collect();
        release(world, &victims, state.quiet, &kept);
        if !cursed(&kept, holder) {
            set_color(world, holder, None);
        }
        tell_mob(
            world,
            holder,
            Span::GreenText,
            "At last it feels like victory is assured!",
        );
        info!("{holder} won with greentext!!!");
        world.despawn(state.item);
        winners.push(holder);
    }
    world.resource_mut::<Greentexts>().0 = kept;
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = Vec::new();
    for winner in winners {
        out.push(line(vec![
            ChatSegment::new(Span::Bold, crate::security::name_of(world, winner)),
            ChatSegment::new(Span::Plain, " был winner"),
        ]));
        out.push(crate::antag::raiders::objective_line(1, "Succeed", true));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sticky_embeds_are_harmless() {
        let sticky = FunnyEmbedding { sticky: true }.data();
        assert!(sticky.embeds());
        assert!(sticky.is_harmless(false));
        let pointy = FunnyEmbedding { sticky: false }.data();
        assert!(pointy.ignore_throwspeed_threshold);
        assert!(!pointy.is_harmless(false));
    }

    #[test]
    fn blessing_skips_embedders_and_prefixes_the_rest() {
        let mut world = World::new();
        let spoon = world
            .spawn((
                DmItem::default(),
                DmName {
                    name: "spoon".into(),
                    desc: String::new(),
                },
            ))
            .id();
        let funny = FunnyEmbedding { sticky: false };
        bless(&mut world, spoon, funny);
        assert_eq!(world.get::<DmName>(spoon).unwrap().name, "pointy spoon");
        assert!(crate::embedding::embed_of(&world, spoon).is_some());
        // Already embeds: left alone.
        bless(&mut world, spoon, funny);
        assert_eq!(world.get::<DmName>(spoon).unwrap().name, "pointy spoon");
    }

    #[test]
    fn clothing_goes_to_its_slot() {
        // `ITEM_SLOT_MASK`, `ITEM_SLOT_OCLOTHING`.
        assert_eq!(clothing_slot(1 << 5), Some(InventorySlot::Mask));
        assert_eq!(clothing_slot(1 << 0), Some(InventorySlot::Suit));
        assert_eq!(clothing_slot(0), None);
    }

    #[test]
    fn the_greentext_curses_its_holder_green_then_red() {
        let mut world = World::new();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        world.init_resource::<Greentexts>();
        let mob = world
            .spawn((
                Stat::Conscious,
                ss13_shared::Appearance {
                    icon_state: ss13_shared::IconStateId(0),
                    layer: ss13_shared::Layer(3.0),
                    plane: -6,
                    color: None,
                    pixel_offset: [0, 0],
                    rotation: 0,
                    mouse_opacity: Default::default(),
                    overlays: Vec::new(),
                },
            ))
            .id();
        let item = world.spawn(CarriedBy(mob)).id();
        world.resource_mut::<Greentexts>().0.push(GreentextState {
            item,
            holder: None,
            victims: Vec::new(),
            quiet: false,
        });
        watch_greentexts(&mut world);
        assert_eq!(
            world.get::<ss13_shared::Appearance>(mob).unwrap().color,
            Some(GREEN)
        );
        world.entity_mut(item).remove::<CarriedBy>();
        watch_greentexts(&mut world);
        assert_eq!(
            world.get::<ss13_shared::Appearance>(mob).unwrap().color,
            Some(RED)
        );
        // Gone: the curse lifts.
        world.despawn(item);
        watch_greentexts(&mut world);
        assert_eq!(
            world.get::<ss13_shared::Appearance>(mob).unwrap().color,
            None
        );
        assert!(world.resource::<Greentexts>().0.is_empty());
    }
}
