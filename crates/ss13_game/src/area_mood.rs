//! Room beauty and area mood bonuses (server), after tg `/datum/element/beauty`, `/area`
//! `totalbeauty`, `update_beauty`, `update_areasize` and `/datum/mood` `check_area_mood`,
//! `update_beauty` (`beauty_events.dm`, `area_events.dm`).
//!
//! - Beauty: dirt and messes (`DmBeauty`, a cleanable's `beauty`), plants, posters, busts,
//!   clocks and piggy banks ([`STATIC_BEAUTY`]) add their beauty to the indoor area they lie in
//!   (items only on the floor, not in hands or containers). An area's beauty is that total over
//!   its open turfs, 0 for an area of 150 tiles or more.
//! - Every second each mood follows its mob's area: the area's `mood_bonus` with its message
//!   (when its `mood_trait` is empty or the mob's: Extrovert in the bar, Introvert in the
//!   library, Spiritual in the chapel), and the room's beauty: a decent, good or great room
//!   cheers anyone up, a bad or horrid one only bothers a Snob.
//!
//! Cost: area totals change only when beauty things move, appear or go (one hash update each);
//! the mood pass is 1 Hz over the moods (one per player) with two lookups each; the area sizes are
//! counted once, on the first use, over the turfs.
// shortcut: moods follow the area every second instead of `COMSIG_ENTER_AREA` and
// `COMSIG_AREA_BEAUTY_UPDATED` (no area signals here); area sizes are counted once (tg recounts
// on blueprint edits); no morbid inversion (`TRAIT_MORBID` is not modelled), no statues
// (`impressiveness`), materials (`beauty_modifier`), engravings, aquariums, fish mounts or
// fantasy affixes. A non-Snob in an ugly room loses the room moodlet (tg's switch leaves the
// last room's one in place).

use std::collections::HashMap;

use bevy::ecs::system::SystemState;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{Areas, DmArea, DmBeauty, DmItem, DmObj, Turf};
use ss13_shared::TilePos;
use ss13_world::Density;

use crate::mood::{ActiveMood, Mood, MoodEvent};
use crate::personality::Personality;

/// `beauty_threshold`: bigger areas have no beauty.
const BEAUTY_THRESHOLD: u32 = 150;
/// `BEAUTY_LEVEL_*`.
const HORRID: f32 = -66.0;
const BAD: f32 = -33.0;
const DECENT: f32 = 33.0;
const GOOD: f32 = 66.0;
const GREAT: f32 = 100.0;
/// `MOOD_CATEGORY_AREA_BEAUTY`, `check_area_mood`'s "area".
pub const BEAUTY_CATEGORY: &str = "area_beauty";
pub const AREA_CATEGORY: &str = "area";
const AREA_WAIT_SECS: f64 = 1.0;

/// The `AddElement(/datum/element/beauty, n)` calls of `Initialize`s, by type.
pub const STATIC_BEAUTY: [(&str, f32); 7] = [
    ("/obj/item/statuebust", 1000.0),
    ("/obj/item/maneki_neko", 800.0),
    ("/obj/item/kirbyplants", 500.0),
    ("/obj/item/piggy_bank", 500.0),
    ("/obj/structure/sign/poster", 300.0),
    ("/obj/machinery/digital_clock", 200.0),
    ("/obj/item/table_clock", 200.0),
];

const fn room(id: &'static str, description: &'static str, mood_change: f32) -> MoodEvent {
    MoodEvent {
        id,
        description,
        mood_change,
        timeout: 0.0,
        hidden: false,
    }
}

pub const HORRIDROOM: MoodEvent = room("horridroom", "Эта комната выглядит ужасно!", -5.0);
pub const BADROOM: MoodEvent = room("badroom", "Эта комната выглядит плохо.", -3.0);
pub const DECENTROOM: MoodEvent = room("decentroom", "Эта комната выглядит неплохо.", 1.0);
pub const GOODROOM: MoodEvent = room("goodroom", "Эта комната выглядит красиво!", 3.0);
pub const GREATROOM: MoodEvent = room("greatroom", "Эта комната выглядит прекрасно!", 5.0);

/// A thing with beauty, and the area its beauty is counted in.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Beauty {
    pub value: f32,
    counted: Option<Entity>,
}

impl Beauty {
    pub fn new(value: f32) -> Self {
        Self {
            value,
            counted: None,
        }
    }
}

/// `totalbeauty` by area, and `areasize` once counted.
#[derive(Resource, Default, Debug)]
pub struct RoomBeauty {
    pub totals: HashMap<Entity, f32>,
    sizes: Option<HashMap<Entity, u32>>,
}

pub struct AreaMoodPlugin;

impl Plugin for AreaMoodPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RoomBeauty>()
            .add_observer(add_cleanable_beauty)
            .add_observer(add_obj_beauty)
            .add_observer(add_item_beauty)
            .add_observer(beauty_removed)
            .add_systems(
                Update,
                (follow_beauty, area_moods)
                    .chain()
                    .after(ss13_world::WorldSystems::Movement),
            );
    }
}

fn static_beauty(path: &str) -> Option<f32> {
    STATIC_BEAUTY
        .iter()
        .find(|(root, _)| is_under(path, root))
        .map(|&(_, b)| b)
}

fn add_cleanable_beauty(add: On<Add<DmBeauty>>, dm: Query<&DmBeauty>, mut commands: Commands) {
    if let Ok(b) = dm.get(add.entity)
        && b.beauty != 0.0
    {
        commands.entity(add.entity).insert(Beauty::new(b.beauty));
    }
}

fn add_obj_beauty(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    if let Some(value) = objs
        .get(add.entity)
        .ok()
        .and_then(|o| static_beauty(&o.path))
    {
        commands.entity(add.entity).insert(Beauty::new(value));
    }
}

fn add_item_beauty(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if let Some(value) = items
        .get(add.entity)
        .ok()
        .and_then(|i| static_beauty(&i.path))
    {
        commands.entity(add.entity).insert(Beauty::new(value));
    }
}

/// `beauty/Detach`: what goes takes its beauty from its area.
fn beauty_removed(
    remove: On<Remove<Beauty>>,
    beauties: Query<&Beauty>,
    mut rooms: ResMut<RoomBeauty>,
) {
    if let Ok(&Beauty {
        value,
        counted: Some(area),
    }) = beauties.get(remove.entity)
    {
        *rooms.totals.entry(area).or_default() -= value;
    }
}

/// `enter_area` / `exit_area` / `on_item_moved`: beauty follows its thing between areas, off the
/// map (picked up, put in a container) it counts nowhere.
#[allow(clippy::type_complexity)]
fn follow_beauty(
    world: &mut World,
    state: &mut SystemState<(
        Query<Entity, (With<Beauty>, Or<(Changed<TilePos>, Added<Beauty>)>)>,
        RemovedComponents<TilePos>,
    )>,
) {
    let things: Vec<Entity> = {
        let Ok((changed, mut lost)) = state.get_mut(world) else {
            return;
        };
        changed.iter().chain(lost.read()).collect()
    };
    for thing in things {
        let area = world.get::<TilePos>(thing).copied().and_then(|pos| {
            let area = world.get_resource::<Areas>()?.area_at(pos)?;
            // An outdoor area has no beauty.
            (!world.get::<DmArea>(area).is_some_and(|a| a.outdoors)).then_some(area)
        });
        let (value, old) = {
            let Some(mut beauty) = world.get_mut::<Beauty>(thing) else {
                continue;
            };
            if beauty.counted == area {
                continue;
            }
            (beauty.value, std::mem::replace(&mut beauty.counted, area))
        };
        let mut rooms = world.resource_mut::<RoomBeauty>();
        if let Some(old) = old {
            *rooms.totals.entry(old).or_default() -= value;
        }
        if let Some(new) = area {
            *rooms.totals.entry(new).or_default() += value;
        }
    }
}

/// tg `update_areasize` of every area at once: its open turfs.
fn count_sizes(world: &mut World) -> HashMap<Entity, u32> {
    let mut sizes = HashMap::new();
    let tiles: Vec<TilePos> = world
        .query_filtered::<&TilePos, (With<Turf>, Without<Density>)>()
        .iter(world)
        .copied()
        .collect();
    if let Some(areas) = world.get_resource::<Areas>() {
        for pos in tiles {
            if let Some(area) = areas.area_at(pos) {
                *sizes.entry(area).or_default() += 1;
            }
        }
    }
    sizes
}

/// `/area/update_beauty`: the average beauty per open tile, 0 for big or empty areas.
pub fn area_beauty(world: &mut World, area: Entity) -> f32 {
    if world.resource::<RoomBeauty>().sizes.is_none() {
        let sizes = count_sizes(world);
        world.resource_mut::<RoomBeauty>().sizes = Some(sizes);
    }
    let rooms = world.resource::<RoomBeauty>();
    let size = rooms
        .sizes
        .as_ref()
        .and_then(|s| s.get(&area).copied())
        .unwrap_or(0);
    beauty_level(rooms.totals.get(&area).copied().unwrap_or(0.0), size)
}

/// `beauty = totalbeauty / areasize` with tg's size limits.
pub fn beauty_level(total: f32, size: u32) -> f32 {
    if size == 0 || size >= BEAUTY_THRESHOLD {
        return 0.0;
    }
    total / size as f32
}

/// `/datum/mood/update_beauty` without the morbid branch: `None` clears the moodlet.
pub fn beauty_event(beauty: f32, snob: bool) -> Option<&'static MoodEvent> {
    if snob && beauty <= HORRID {
        return Some(&HORRIDROOM);
    }
    if snob && beauty <= BAD {
        return Some(&BADROOM);
    }
    if beauty <= DECENT {
        None
    } else if beauty <= GOOD {
        Some(&DECENTROOM)
    } else if beauty <= GREAT {
        Some(&GOODROOM)
    } else {
        Some(&GREATROOM)
    }
}

/// `mood_trait` of an area against the mob's personalities (the traits they grant).
fn has_mood_trait(mood: &Mood, mood_trait: &str) -> bool {
    match mood_trait {
        "" => true,
        "extrovert" => mood.has_personality(Personality::Extrovert),
        "introvert" => mood.has_personality(Personality::Introvert),
        "spiritual" => mood.has_personality(Personality::Spiritual),
        _ => false,
    }
}

/// `check_area_mood` and `update_beauty` on `mood` for an area (`None`: off any area).
pub fn apply_area(mood: &mut Mood, now: f64, area: Option<&DmArea>, beauty: f32) {
    let bonus = area.filter(|a| a.mood_bonus != 0.0 && has_mood_trait(mood, &a.mood_trait));
    match bonus {
        Some(a) => {
            let same = mood
                .get(AREA_CATEGORY)
                .is_some_and(|e| e.mood_change == a.mood_bonus && e.description == a.mood_message);
            if !same {
                let mut event = ActiveMood::new(AREA_CATEGORY, &room("area", "", 0.0));
                event.mood_change = a.mood_bonus;
                event.description.clone_from(&a.mood_message);
                mood.add_instance(now, event);
            }
        }
        None => {
            if mood.get(AREA_CATEGORY).is_some() {
                mood.clear_event(AREA_CATEGORY);
            }
        }
    }
    let outdoors = area.is_none_or(|a| a.outdoors);
    let event = (!outdoors)
        .then(|| beauty_event(beauty, mood.has_personality(Personality::Snob)))
        .flatten();
    match event {
        Some(event) if mood.get(BEAUTY_CATEGORY).is_none_or(|e| e.id != event.id) => {
            mood.add_event(now, BEAUTY_CATEGORY, event);
        }
        Some(_) => {}
        None => {
            if mood.get(BEAUTY_CATEGORY).is_some() {
                mood.clear_event(BEAUTY_CATEGORY);
            }
        }
    }
}

/// Every second, each mood follows its mob's area.
fn area_moods(world: &mut World, mut clock: Local<f64>) {
    *clock += world.resource::<Time>().delta_secs_f64();
    if *clock < AREA_WAIT_SECS {
        return;
    }
    *clock %= AREA_WAIT_SECS;
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mobs: Vec<(Entity, TilePos)> = world
        .query_filtered::<(Entity, &TilePos), With<Mood>>()
        .iter(world)
        .map(|(e, p)| (e, *p))
        .collect();
    for (mob, pos) in mobs {
        let area = world.get_resource::<Areas>().and_then(|a| a.area_at(pos));
        let beauty = area.map_or(0.0, |a| area_beauty(world, a));
        let dm = area.and_then(|a| world.get::<DmArea>(a)).cloned();
        let Some(mut mood) = world.get_mut::<Mood>(mob) else {
            continue;
        };
        let mut new = mood.clone();
        apply_area(&mut new, now, dm.as_ref(), beauty);
        mood.set_if_neq(new);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beauty_levels_follow_tg_bands() {
        assert_eq!(beauty_level(-1000.0, 10), -100.0);
        assert_eq!(beauty_level(-1000.0, 150), 0.0, "too big");
        assert_eq!(beauty_level(500.0, 0), 0.0);
        assert_eq!(
            beauty_event(-100.0, false),
            None,
            "only snobs mind ugly rooms"
        );
        assert_eq!(beauty_event(-66.0, true).unwrap().id, "horridroom");
        assert_eq!(beauty_event(-40.0, true).unwrap().id, "badroom");
        assert_eq!(beauty_event(33.0, true), None);
        assert_eq!(beauty_event(50.0, false).unwrap().id, "decentroom");
        assert_eq!(beauty_event(66.0, false).unwrap().id, "decentroom");
        assert_eq!(beauty_event(100.0, false).unwrap().id, "goodroom");
        assert_eq!(beauty_event(101.0, false).unwrap().id, "greatroom");
    }

    #[test]
    fn area_bonus_needs_its_trait() {
        let mut mood = Mood::default();
        let bar = DmArea {
            mood_bonus: 5.0,
            mood_message: "I love being in the bar!".into(),
            mood_trait: "extrovert".into(),
            ..default()
        };
        apply_area(&mut mood, 0.0, Some(&bar), 0.0);
        assert!(mood.get(AREA_CATEGORY).is_none(), "not an extrovert");
        mood.personalities |= Personality::Extrovert.bit();
        apply_area(&mut mood, 0.0, Some(&bar), 50.0);
        let event = mood.get(AREA_CATEGORY).unwrap();
        assert_eq!(
            (event.mood_change, event.description.as_str()),
            (5.0, "I love being in the bar!")
        );
        assert_eq!(mood.get(BEAUTY_CATEGORY).unwrap().id, "decentroom");
        let psychology = DmArea {
            mood_bonus: 3.0,
            mood_message: "I feel at ease here.".into(),
            ..default()
        };
        apply_area(&mut mood, 1.0, Some(&psychology), 0.0);
        assert_eq!(mood.get(AREA_CATEGORY).unwrap().mood_change, 3.0);
        assert!(mood.get(BEAUTY_CATEGORY).is_none());
        let outside = DmArea {
            outdoors: true,
            ..default()
        };
        apply_area(&mut mood, 2.0, Some(&outside), 200.0);
        assert!(mood.get(AREA_CATEGORY).is_none());
        assert!(
            mood.get(BEAUTY_CATEGORY).is_none(),
            "outdoors has no beauty"
        );
    }
}
