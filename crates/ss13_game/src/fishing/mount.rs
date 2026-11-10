//! Trophy fish mounts (server), after tg `/obj/structure/fish_mount` and its frame
//! `/obj/item/wallframe/fish` (`code/modules/fishing/fish_mount.dm`).
//!
//! - The frame mounts like the other wall frames (`crate::wallmount::frames`).
//! - A fish used on a mount is mounted after 3 s (one at a time): it shows on the mount, keeps
//!   its catcher's name and the catch date, and everyone sees its size and weight.
//! - A hand on the mount takes the fish down after 3 s (6 s for a trophy that came with the
//!   mount, which survives only by a roll: `100 / (w_class + weight rank)` % + 4 % per fishing
//!   level, else it turns to ash).
//! - A screwdriver takes the mount off the wall after 3 s: the frame and the fish drop.
//! - Mounts with a `persistence_id` (the bar's) start with a random dead trophy fish.
// shortcut: no persistence between rounds (SQL/persistence is ported last): a persistent mount
// always gets a fresh random trophy, as on its first round; no beauty, no
// `fish_id_redirect_path`.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::fishing::FISH_ROOT;
use ss13_mapload::{DmObj, DmWeight};
use ss13_shared::appearance::Overlay;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};

use super::fish::Fish;
use crate::combat::CombatRng;
use crate::construction::Job;
use crate::inventory::Inventory;
use crate::sound::{PlaySound, playsound};
use crate::storage::StoredIn;
use crate::tools::{Actor, ToolKind, tool_of};

pub const MOUNT_ROOT: &str = "/obj/structure/fish_mount";
/// `/obj/structure/fish_mount/bar` (`persistence_id = "Bar"`).
const PERSISTENT_ROOT: &str = "/obj/structure/fish_mount/bar";
const FRAME: &str = "/obj/item/wallframe/fish";
const ASH: &str = "/obj/effect/decal/cleanable/ash";
const ASH_LARGE: &str = "/obj/effect/decal/cleanable/ash/large";
const CLICK: &str = "sound/machines/click.ogg";
const DECONSTRUCT: &str = "sound/items/deconstruct.ogg";
/// `FISH_WEIGHT_FORCE_DIVISOR`.
const WEIGHT_FORCE_DIVISOR: f32 = 250.0;
/// `WEIGHT_CLASS_BULKY`.
const BULKY: u8 = 4;
/// `add_first_fish`'s fluff catchers (`AQUARIUM_COMPANY` is "Aquatech Ltd.").
const FLUFF_CATCHERS: [&str; 6] = [
    "John Trasen III",
    "a nameless intern",
    "Pun Pun",
    "Aquatech Ltd.",
    "Unknown",
    "Central Command",
];

/// A mount's state.
#[derive(Component, Clone, Debug, Default)]
pub struct FishMount {
    pub fish: Option<Entity>,
    /// `persistence_loaded_fish`: the trophy it came with (hard to take down whole).
    pub from_persistence: bool,
}

/// On a mounted (or once mounted) fish: `catcher_name`, `catch_date`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Trophy {
    pub catcher: String,
    pub date: String,
}

/// Timed steps on a mount.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MountJob {
    /// The fish in hand onto the mount.
    Mount(Entity),
    /// The mounted fish into the hands.
    Remove(Entity),
    /// The mount off the wall.
    Unscrew,
}

pub(super) fn plugin(app: &mut App) {
    app.add_observer(add_mount);
}

fn add_mount(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(obj) = objs.get(add.entity) else {
        return;
    };
    if !is_under(&obj.path, MOUNT_ROOT) {
        return;
    }
    let e = add.entity;
    let persistent = is_under(&obj.path, PERSISTENT_ROOT);
    commands.entity(e).try_insert_if_new(FishMount::default());
    if persistent {
        commands.queue(move |world: &mut World| add_first_fish(world, e));
    }
}

/// `add_first_fish`: a random dead fish of any kind but the holographic ones and the
/// chrystarfish (it shatters when placed).
fn add_first_fish(world: &mut World, mount: Entity) {
    if world
        .get::<FishMount>(mount)
        .is_none_or(|m| m.fish.is_some())
    {
        return;
    }
    let Some(&at) = world.get::<TilePos>(mount) else {
        return;
    };
    let picks: Vec<String> = world
        .get_resource::<ss13_proto::Prototypes>()
        .map(|p| {
            p.ids()
                .filter(|id| {
                    is_under(id, FISH_ROOT)
                        && *id != FISH_ROOT
                        && !is_under(id, "/obj/item/fish/holo")
                        && *id != "/obj/item/fish/starfish/chrystarfish"
                })
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    if picks.is_empty() {
        return;
    }
    let (i, j) = {
        let mut rng = world.resource_mut::<CombatRng>();
        (
            rng.range(0, picks.len() as u32 - 1) as usize,
            rng.range(0, FLUFF_CATCHERS.len() as u32 - 1) as usize,
        )
    };
    let Some(fish) = crate::construction::spawn_obj(world, &picks[i], at) else {
        return;
    };
    super::fish::randomize_size_and_weight(world, fish);
    super::fish::kill(world, fish);
    add_fish(world, mount, fish, true, FLUFF_CATCHERS[j]);
}

/// `catch_date`: "Day, Month DD, <station year>".
fn catch_date() -> String {
    const DAYS: [&str; 7] = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let secs = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let d = crate::events::holidays::Date::at(secs, 0.0);
    // `STATION_YEAR_OFFSET` (BandaStation: 544).
    format!(
        "{}, {} {:02}, {}",
        DAYS[(d.weekday as usize).clamp(1, 7) - 1],
        MONTHS[(d.month as usize).clamp(1, 12) - 1],
        d.day,
        d.year + 544
    )
}

/// `add_fish`: into the mount, shown on it.
fn add_fish(world: &mut World, mount: Entity, fish: Entity, from_persistence: bool, catcher: &str) {
    if let Some(old) = world.get::<FishMount>(mount).and_then(|m| m.fish)
        && let Some(&at) = world.get::<TilePos>(mount)
    {
        crate::storage::take_out(world, old, at);
    }
    crate::storage::put_inside(world, fish, mount);
    if world.get::<Trophy>(fish).is_none() {
        let date = catch_date();
        world.entity_mut(fish).insert(Trophy {
            catcher: catcher.to_owned(),
            date,
        });
    }
    if let Some(mut m) = world.get_mut::<FishMount>(mount) {
        m.fish = Some(fish);
        m.from_persistence = from_persistence;
    }
    refresh(world, mount);
}

/// The mounted fish drawn on the mount (tg `vis_contents`).
fn refresh(world: &mut World, mount: Entity) {
    let fish = world.get::<FishMount>(mount).and_then(|m| m.fish);
    let overlays: Vec<Overlay> = fish
        .and_then(|f| world.get::<Appearance>(f))
        .map(|a| Overlay::new(a.icon_state))
        .into_iter()
        .collect();
    if let Some(mut look) = world.get_mut::<Appearance>(mount)
        && look.overlays != overlays
    {
        look.overlays = overlays;
    }
}

/// The mounted fish, if it is still inside.
fn mounted(world: &World, mount: Entity) -> Option<Entity> {
    world
        .get::<FishMount>(mount)?
        .fish
        .filter(|&f| world.get::<StoredIn>(f).is_some_and(|s| s.0 == mount))
}

fn tell(world: &mut World, client: Entity, span: Span, text: &str) {
    crate::tools::tell(world, client, span, text);
}

/// `item_interaction` (a fish onto the mount) and `screwdriver_act`. True when the click was
/// the mount's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<FishMount>(target).is_none() {
        return false;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    if matches!(tool_of(world, held), Some((ToolKind::Screwdriver, _))) {
        if crate::tools::use_tool(
            world,
            actor,
            target,
            30.0,
            0,
            Job::FishMount(MountJob::Unscrew),
        ) {
            tell(world, client, Span::Notice, "removing mount...");
        }
        return true;
    }
    if world.get::<Fish>(held).is_none() || crate::medical::surgery::combat_mode(world, mob) {
        return false;
    }
    if mounted(world, target).is_some() {
        tell(world, client, Span::Warning, "remove other fish first!");
        return true;
    }
    if crate::tools::do_after(
        world,
        actor,
        target,
        30.0,
        0,
        Job::FishMount(MountJob::Mount(held)),
    ) {
        tell(world, client, Span::Notice, "mounting fish...");
    }
    true
}

/// `attack_hand_secondary` and the fish's `attack_hand`: take the fish down.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<FishMount>(target).is_none() {
        return false;
    }
    let Some(fish) = mounted(world, target) else {
        tell(world, client, Span::Warning, "no fish mounted!");
        return true;
    };
    let persistent = world
        .get::<FishMount>(target)
        .is_some_and(|m| m.from_persistence);
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    let delay = if persistent { 60.0 } else { 30.0 };
    if crate::tools::do_after(
        world,
        actor,
        target,
        delay,
        0,
        Job::FishMount(MountJob::Remove(fish)),
    ) {
        tell(world, client, Span::Notice, "removing fish...");
    }
    true
}

/// `roll_for_safe_removal`'s chance, percent: `100 / (w_class + weight rank)` plus 4 per fishing
/// level.
pub fn safe_removal_chance(w_class: u8, weight: f32, fishing_level: u8) -> f32 {
    let rank = (1.0 + (weight / WEIGHT_FORCE_DIVISOR).max(1.0).log2())
        .round()
        .max(1.0);
    100.0 / (f32::from(w_class) + rank) + f32::from(fishing_level) * 4.0
}

/// `roll_for_safe_removal`: false when the fish turned to dust (it is gone, ash on the tile).
fn roll_safe_removal(world: &mut World, mount: Entity, fish: Entity, user: Option<Entity>) -> bool {
    let w_class = world.get::<DmWeight>(fish).map_or(BULKY, |w| w.w_class);
    let weight = world.get::<Fish>(fish).map_or(1000.0, |f| f.weight);
    let level = user.map_or(0, |u| {
        crate::skills::level(world, u, crate::skills::Skill::Fishing)
    });
    let chance = safe_removal_chance(w_class, weight, level);
    let roll = world.resource_mut::<CombatRng>().unit() * 100.0;
    if let Some(mut m) = world.get_mut::<FishMount>(mount) {
        m.from_persistence = false;
    }
    if roll < chance {
        return true;
    }
    let Some(&at) = world.get::<TilePos>(mount) else {
        return false;
    };
    let name = crate::chem::name_of(world, fish);
    if let Some(mut m) = world.get_mut::<FishMount>(mount) {
        m.fish = None;
    }
    world.entity_mut(fish).despawn();
    crate::construction::spawn_obj(world, if w_class >= BULKY { ASH_LARGE } else { ASH }, at);
    let mount_name = crate::chem::name_of(world, mount);
    let lines = crate::pull::visible_message(
        world,
        at,
        ss13_shared::chat::ChatChannel::Local,
        (
            Span::Plain,
            format!("{name} turns into dust as it's removed from {mount_name}."),
        ),
        &[],
    );
    for line in lines {
        world.write_message(line);
    }
    refresh(world, mount);
    false
}

/// The fish leaves the mount (tg `Exited`).
fn release(world: &mut World, mount: Entity, fish: Entity, at: TilePos) {
    if let Some(mut m) = world.get_mut::<FishMount>(mount) {
        m.fish = None;
        m.from_persistence = false;
    }
    crate::storage::take_out(world, fish, at);
    refresh(world, mount);
}

/// End of a [`MountJob`], every check again.
pub fn complete(world: &mut World, mob: Entity, action: &crate::tools::DoAfter, job: MountJob) {
    let (client, mount) = (action.client, action.target);
    if world.get::<FishMount>(mount).is_none() {
        return;
    }
    let Some(&at) = world.get::<TilePos>(mount) else {
        return;
    };
    match job {
        MountJob::Mount(fish) => {
            let in_hand = world
                .get::<Inventory>(mob)
                .is_some_and(|i| i.hands.contains(&Some(fish)));
            if !in_hand || world.get::<Fish>(fish).is_none() || mounted(world, mount).is_some() {
                return;
            }
            if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
                for hand in inventory.hands.iter_mut() {
                    if *hand == Some(fish) {
                        *hand = None;
                    }
                }
            }
            let catcher = crate::chem::name_of(world, mob);
            add_fish(world, mount, fish, false, &catcher);
            tell(world, client, Span::Notice, "fish mounted");
            playsound(world, PlaySound::new(at, CLICK, 30.0, true));
        }
        MountJob::Remove(fish) => {
            if mounted(world, mount) != Some(fish) {
                return;
            }
            let persistent = world
                .get::<FishMount>(mount)
                .is_some_and(|m| m.from_persistence);
            if persistent && !roll_safe_removal(world, mount, fish, Some(mob)) {
                tell(world, client, Span::Notice, "fish removed");
                return;
            }
            release(world, mount, fish, at);
            if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
                crate::storage::put_in_hands(world, mob, &inventory, fish);
            }
            tell(world, client, Span::Notice, "fish removed");
        }
        MountJob::Unscrew => {
            playsound(world, PlaySound::new(at, DECONSTRUCT, 50.0, true));
            tell(world, client, Span::Notice, "fish mount removed");
            crate::construction::spawn_obj(world, FRAME, at);
            if let Some(fish) = mounted(world, mount) {
                let persistent = world
                    .get::<FishMount>(mount)
                    .is_some_and(|m| m.from_persistence);
                if !persistent || roll_safe_removal(world, mount, fish, None) {
                    release(world, mount, fish, at);
                }
            }
            world.entity_mut(mount).despawn();
            crate::construction::tile_changed(world, at);
        }
    }
}

/// `TRAIT_EXAMINE_FISH` of a mounted fish is [`super::fish::examine_lines`]; here the
/// fish's `Caught by ... on ...` line.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    if let Some(t) = world.get::<Trophy>(target) {
        lines.push((
            Span::Notice,
            format!("Caught by {} on {}.", t.catcher, t.date),
        ));
    }
    if let Some(fish) = world.get::<FishMount>(target).and(mounted(world, target)) {
        lines.push((
            Span::Notice,
            format!("{} is mounted on it.", crate::chem::name_of(world, fish)),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trophies_survive_by_size_weight_and_skill() {
        // A normal 1 kg fish (rank 3), w_class 2: 100 / 5.
        assert_eq!(safe_removal_chance(2, 1000.0, 0), 20.0);
        // Light fish rank 1.
        assert_eq!(safe_removal_chance(1, 100.0, 0), 50.0);
        // A Legendary angler adds 28.
        assert_eq!(safe_removal_chance(2, 1000.0, 7), 48.0);
    }
}
