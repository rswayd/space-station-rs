//! The anomalies of the later waves (`code/game/objects/effects/anomalies/`):
//!
//! - Bioscrambler (`anomalies_bioscrambler.dm`): lives twice as long, chases the nearest
//!   conscious carbon on its level through anything (a new target gets a warning nosebleed
//!   line), and every 10 s pulses: each organic carbon within 2 tiles gets one organ swapped for
//!   a random one of another species (`/mob/living/carbon/proc/bioscramble`), unless their bio
//!   armor is total; detonating pulses once more.
//! - Dimensional (`anomalies_dimensional.dm`, `anomalies_dimensional_themes.dm`): never drifts;
//!   picks a theme and transmutes one convertible turf of the spiral within 3 tiles per process
//!   (floors, walls, chairs, tables, toilets, platforms, full-tile windows recolored, random
//!   flora), then relocates to a new area with an announcement, 1-4 times, and is gone.
//! - Ectoplasm (`anomalies_ectoplasm.dm`): never drifts; its power is the share of the dead
//!   players that "orbit" it; detonating at 10% defiles the tiles within `orbiters + 3`
//!   (blood, ectoplasm and dust decals, broken floor tiles, damaged windows), at 50% opens a
//!   ghost portal for 2 minutes with an announcement.
//
// shortcut: bioscrambling swaps organs only among the modelled slots of the ported species
// (heart, eyes, ears, tongue, liver, stomach, appendix; tg: every organ subtype, external ones
// too) and swaps no limbs (a limb has no species of its own here: `crate::human::HumanLook`
// draws one species); the nosebleed is the line only; hearers ignore line of sight.
// shortcut: the dimensional anomaly keeps airlocks (tg replaces them and copies their access),
// spawns no mobs, has no theme overlay or flash effect, and recolors material themes' turfs and
// objects instead of setting their custom materials; the fancy carpet is picked per area, the
// disco floor gets no light color.
// shortcut: ghosts have no orbit verb here, so the ectoplasm anomaly counts the observers on its
// tile as its orbiters; no deadchat control, haunting outburst (35%) or revblight; the portal
// polls no ghost swarm.

use bevy::prelude::*;
use ss13_mapload::{DmObj, Turf};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};
use ss13_world::SpatialIndex;

use super::{Anomaly, AnomalyKind, EventRng, find_valid_area, valid_destination};
use crate::construction::{baseturfs_of, change_turf, path_of, turf_at};
use crate::ghost::Ghost;
use crate::health::{DamageType, Health, Stat};
use crate::medical::organs::{self, Organ, OrganHost, Slot};
use crate::obj_damage::is_under;
use crate::server::PlayerMob;

const COSMIC_ENERGY: &str = "sound/effects/magic/cosmic_energy.ogg";
/// The bioscrambler's `pulse_delay` and `range`.
const PULSE_SECS: f64 = 10.0;
const PULSE_RANGE: u32 = 2;
/// The dimensional anomaly's `range`, `minimum_teleports`, `maximum_teleports`.
const DIMENSION_RANGE: i32 = 3;
const MIN_TELEPORTS: i64 = 1;
const MAX_TELEPORTS: i64 = 4;
/// The ghost portal's lifetime (`QDEL_IN(src, 2 MINUTES)`).
const PORTAL_SECS: f64 = 120.0;
const GHOST_PORTAL: &str = "/obj/structure/ghost_portal";
const SPOOKY_NOISES: [&str; 5] = [
    "sound/effects/hallucinations/growl1.ogg",
    "sound/effects/hallucinations/growl2.ogg",
    "sound/effects/hallucinations/growl3.ogg",
    "sound/effects/hallucinations/veryfar_noise.ogg",
    "sound/effects/hallucinations/wail.ogg",
];

/// The constant spawn paths of this module (`ss13_mapload::GAME_RUNTIME_TYPES` lists them).
pub const SPAWNED: &[&str] = &[
    GHOST_PORTAL,
    "/obj/effect/decal/cleanable/blood",
    "/obj/effect/decal/cleanable/greenglow/ecto",
    "/obj/effect/decal/cleanable/dirt/dust",
];

/// Per-kind state an anomaly spawns with.
pub(super) fn on_spawn(world: &mut World, e: Entity, kind: AnomalyKind) {
    match kind {
        AnomalyKind::Bioscrambler => {
            let target = nearest_target(world, e);
            world.entity_mut(e).insert(Bioscrambler {
                target,
                next_pulse: 0.0,
                docile: false,
            });
        }
        AnomalyKind::Dimensional => {
            let teleports = world
                .resource_mut::<EventRng>()
                .0
                .range(MIN_TELEPORTS, MAX_TELEPORTS) as u32;
            world.entity_mut(e).insert(Dimensional {
                theme: None,
                targets: Vec::new(),
                teleports_left: teleports,
            });
        }
        AnomalyKind::Ectoplasm => {
            world.entity_mut(e).insert(Ectoplasm::default());
        }
        _ => {}
    }
}

fn tell_mob(world: &mut World, mob: Entity, span: Span, text: &str) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        crate::tools::tell(world, client, span, text);
    }
}

// --- Bioscrambler -------------------------------------------------------------------------

/// `/obj/effect/anomaly/bioscrambler`: `pursuit_target`, `pulse_cooldown`.
#[derive(Component, Debug)]
pub struct Bioscrambler {
    pub target: Option<Entity>,
    /// Game time the next pulse may come.
    pub next_pulse: f64,
    /// `/obj/effect/anomaly/bioscrambler/docile`: no pursuit.
    pub docile: bool,
}

/// `find_nearest_target()`: the closest player carbon on its level that is not unconscious.
fn nearest_target(world: &mut World, e: Entity) -> Option<Entity> {
    let pos = *world.get::<TilePos>(e)?;
    let mut q = world.query_filtered::<(Entity, &TilePos, Option<&Stat>), (
        With<PlayerMob>,
        With<OrganHost>,
        With<Health>,
    )>();
    q.iter(world)
        .filter(|(_, p, stat)| p.z == pos.z && !matches!(stat, Some(Stat::HardCrit | Stat::Dead)))
        .filter_map(|(m, p, _)| pos.chebyshev(*p).map(|d| (d, m)))
        .min_by_key(|&(d, m)| (d, m))
        .map(|(_, m)| m)
}

/// `move_anomaly()`: `update_target()`, then a step towards the target through anything (its
/// `pass_flags`); without a target the usual drift. True when it handled the move.
pub(super) fn bioscrambler_move(world: &mut World, e: Entity) -> bool {
    let Some(state) = world.get::<Bioscrambler>(e) else {
        return false;
    };
    if state.docile {
        return false;
    }
    let current = state
        .target
        .filter(|&t| world.get_entity(t).is_ok() && world.get::<TilePos>(t).is_some());
    let keep = current.is_some() && world.resource_mut::<EventRng>().0.prob(80.0);
    let target = if keep {
        current
    } else {
        let found = nearest_target(world, e);
        if let Some(new) = found
            && Some(new) != current
        {
            // `ominous_nosebleed()`.
            tell_mob(world, new, Span::Warning, "У вас пошла кровь из носа.");
        }
        found
    };
    if let Some(mut s) = world.get_mut::<Bioscrambler>(e) {
        s.target = target;
    }
    let (Some(target), Some(&pos)) = (target, world.get::<TilePos>(e)) else {
        return false;
    };
    let Some(&to) = world.get::<TilePos>(target) else {
        return false;
    };
    if to.z != pos.z || to == pos {
        return true;
    }
    let next = TilePos::new(
        pos.x + (to.x - pos.x).signum(),
        pos.y + (to.y - pos.y).signum(),
        pos.z,
    );
    world.entity_mut(e).insert(next);
    true
}

/// The bioscrambler `anomalyEffect`: a pulse when its cooldown is over (`force`: detonation's
/// `COOLDOWN_RESET`).
pub(super) fn bioscrambler_effect(world: &mut World, e: Entity, force: bool) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let name = crate::chem::name_of(world, e);
    {
        let Some(mut s) = world.get_mut::<Bioscrambler>(e) else {
            return;
        };
        if !force && s.next_pulse > now {
            return;
        }
        s.next_pulse = now + PULSE_SECS;
    }
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, COSMIC_ENERGY, 50.0, true),
    );
    let near: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(pos, PULSE_RANGE)
        .filter(|&m| world.get::<OrganHost>(m).is_some() && world.get::<Health>(m).is_some())
        .collect();
    for mob in near {
        bioscramble(world, mob, &name);
    }
}

/// Bioscrambler `bioscrambler_organs_blacklist` (and `ORGAN_ROBOTIC` types).
const ORGAN_BLACKLIST: [&str; 9] = [
    "/obj/item/organ/brain",
    "/obj/item/organ/lungs",
    "/obj/item/organ/alien",
    "/obj/item/organ/ears/dullahan",
    "/obj/item/organ/eyes/dullahan",
    "/obj/item/organ/tongue/dullahan",
    "/obj/item/organ/heart/cursed",
    "/obj/item/organ/heart/demon",
    "/obj/item/organ/cyberimp",
];

/// `GLOB.bioscrambler_valid_organs` among the ported species' organs, sorted.
pub fn scramble_organs() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = crate::species::SPECIES
        .iter()
        .flat_map(|def| Slot::ALL.into_iter().filter_map(|s| def.organs.get(s)))
        .filter(|p| {
            !ORGAN_BLACKLIST.iter().any(|b| is_under(p, b))
                && !p.contains("/cybernetic")
                && !p.contains("/robot")
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// `/mob/living/carbon/proc/bioscramble(scramble_source)`: one random organ replaces the one
/// in its slot (the old one is gone), unless the mob is not organic, geneless or fully
/// bio-armored. True when something changed.
pub fn bioscramble(world: &mut World, mob: Entity, source: &str) -> bool {
    use crate::species::{Trait, def_of, has_trait};
    if def_of(world, mob).biotypes & crate::species::biotype::ORGANIC == 0
        || has_trait(world, mob, Trait::Geneless)
    {
        return false;
    }
    if crate::armor::getarmor(world, mob, crate::zone::BodyZone::Chest, "bio") >= 100.0 {
        tell_mob(
            world,
            mob,
            Span::Warning,
            &format!("Ваша броня поглощает воздействие от {source}!"),
        );
        return false;
    }
    let pool = scramble_organs();
    let Some(&path) = world.resource_mut::<EventRng>().pick(&pool) else {
        return false;
    };
    let Some(slot) = Slot::of_path(path) else {
        return false;
    };
    let replaced = organs::organ(world, mob, slot);
    let replaceable = replaced.is_none_or(|o| world.get::<Organ>(o).is_some_and(|o| !o.robotic));
    let part_there = world
        .get::<crate::medical::wounds::Limbs>(mob)
        .is_none_or(|l| !l.limb(slot.bodypart()).missing);
    if !replaceable || !part_there {
        tell_mob(
            world,
            mob,
            Span::Notice,
            &format!("Ваше аугментированное тело защищает вас от: {source}!"),
        );
        return false;
    }
    let Some(new) = organs::spawn_organ(world, path) else {
        return false;
    };
    // `replace_into`: `DELETE_IF_REPLACED`.
    if let Some(old) = replaced {
        organs::remove(world, old, true);
        world.despawn(old);
    }
    organs::insert(world, new, mob);
    tell_mob(world, mob, Span::Warning, "В вас что-то изменилось.");
    true
}

// --- Dimensional --------------------------------------------------------------------------

/// `/obj/effect/anomaly/dimensional`: its theme (an index into [`THEMES`]), the turfs left to
/// transmute and its teleports.
#[derive(Component, Debug)]
pub struct Dimensional {
    pub theme: Option<usize>,
    pub targets: Vec<TilePos>,
    pub teleports_left: u32,
}

/// A `/datum/dimension_theme`.
pub struct Theme {
    pub name: &'static str,
    /// Its material's color (`material`): material floors, walls and objects take it.
    color: Option<[u8; 3]>,
    sound: &'static str,
    /// `replace_floors` by weight (empty: floors stay).
    floors: &'static [(&'static str, i64)],
    replace_walls: &'static str,
    objs: &'static [(&'static str, &'static [(&'static str, i64)])],
    /// `replace_window` (`None`: full-tile windows are only recolored).
    window: Option<&'static str>,
    window_colour: [u8; 3],
    spawns: &'static [&'static str],
    spawn_chance: f32,
}

const BLIND: &str = "sound/effects/magic/blind.ogg";
const MATERIAL_FLOOR: &[(&str, i64)] = &[("/turf/open/floor/material", 1)];
const MATERIAL_WALL: &str = "/turf/closed/wall/material";
const DEFAULT_OBJS: &[(&str, &[(&str, i64)])] = &[
    (
        "/obj/structure/chair",
        &[("/obj/structure/chair/greyscale", 1)],
    ),
    (
        "/obj/structure/table",
        &[("/obj/structure/table/greyscale", 1)],
    ),
    (
        "/obj/structure/toilet",
        &[("/obj/structure/toilet/greyscale", 1)],
    ),
    (
        "/obj/structure/platform",
        &[("/obj/structure/platform/material", 1)],
    ),
];
const WOOD_OBJS: &[(&str, &[(&str, i64)])] = &[
    ("/obj/structure/chair", &[("/obj/structure/chair/wood", 1)]),
    ("/obj/structure/table", &[("/obj/structure/table/wood", 1)]),
    (
        "/obj/structure/platform",
        &[("/obj/structure/platform/wood", 1)],
    ),
];

const fn material(
    name: &'static str,
    color: [u8; 3],
    sound: &'static str,
    floors: &'static [(&'static str, i64)],
) -> Theme {
    Theme {
        name,
        color: Some(color),
        sound,
        floors,
        replace_walls: MATERIAL_WALL,
        objs: DEFAULT_OBJS,
        window: None,
        window_colour: color,
        spawns: &[],
        spawn_chance: 0.0,
    }
}

/// `subtypesof(/datum/dimension_theme)`.
pub static THEMES: [Theme; 18] = [
    Theme {
        objs: &[
            (
                "/obj/structure/chair",
                &[("/obj/structure/chair/greyscale", 1)],
            ),
            (
                "/obj/structure/table",
                &[("/obj/structure/table/greyscale", 1)],
            ),
            (
                "/obj/structure/toilet",
                &[("/obj/structure/toilet/greyscale", 1)],
            ),
            (
                "/obj/structure/platform",
                &[("/obj/structure/platform/gold", 1)],
            ),
        ],
        ..material("Gold", [0xE6, 0xBB, 0x45], BLIND, MATERIAL_FLOOR)
    },
    material("Plasma", [0xBA, 0x36, 0x92], BLIND, MATERIAL_FLOOR),
    material(
        "Clown",
        [0xFF, 0xFF, 0x00],
        "sound/items/bikehorn.ogg",
        MATERIAL_FLOOR,
    ),
    material(
        "Radioactive",
        [0x2C, 0x99, 0x2C],
        "sound/items/tools/welder.ogg",
        MATERIAL_FLOOR,
    ),
    material(
        "Meat",
        [214, 67, 67],
        "sound/items/eatfood.ogg",
        MATERIAL_FLOOR,
    ),
    Theme {
        objs: &[
            (
                "/obj/structure/chair",
                &[("/obj/structure/chair/greyscale", 1)],
            ),
            (
                "/obj/structure/table",
                &[("/obj/structure/table/greyscale", 1)],
            ),
            (
                "/obj/structure/toilet",
                &[("/obj/structure/toilet/greyscale", 1)],
            ),
            (
                "/obj/structure/platform",
                &[("/obj/structure/platform/pizza", 1)],
            ),
        ],
        ..material(
            "Pizza",
            [0xFF, 0x9F, 0x23],
            "sound/items/eatfood.ogg",
            MATERIAL_FLOOR,
        )
    },
    Theme {
        name: "Natural",
        color: None,
        sound: BLIND,
        floors: &[("/turf/open/floor/grass", 1)],
        replace_walls: "/turf/closed/wall/mineral/wood/nonmetal",
        objs: &[
            (
                "/obj/structure/chair",
                &[
                    ("/obj/structure/chair/wood", 3),
                    ("/obj/structure/chair/wood/wings", 1),
                ],
            ),
            (
                "/obj/structure/table",
                &[
                    ("/obj/structure/table/wood", 5),
                    ("/obj/structure/table/wood/fancy", 1),
                ],
            ),
            (
                "/obj/structure/platform",
                &[("/obj/structure/platform/wood", 1)],
            ),
        ],
        window: None,
        window_colour: [0x00, 0xF7, 0xFF],
        spawns: &[],
        spawn_chance: 0.0,
    },
    Theme {
        name: "Bamboo",
        color: None,
        sound: BLIND,
        floors: &[("/turf/open/floor/bamboo", 1)],
        replace_walls: "/turf/closed/wall/mineral/bamboo",
        objs: &[
            (
                "/obj/structure/chair",
                &[("/obj/structure/chair/stool/bamboo", 1)],
            ),
            ("/obj/structure/table", &[("/obj/structure/table/wood", 1)]),
            (
                "/obj/structure/platform",
                &[("/obj/structure/platform/bamboo", 1)],
            ),
        ],
        window: Some("/obj/structure/window/paperframe"),
        window_colour: [0xFF, 0xFF, 0xFF],
        spawns: &[],
        spawn_chance: 0.0,
    },
    Theme {
        name: "Winter",
        color: Some([0xFF, 0xFF, 0xFF]),
        sound: BLIND,
        floors: &[
            ("/turf/open/floor/fake_snow", 10),
            ("/turf/open/floor/fakeice/slippery", 1),
        ],
        replace_walls: "/turf/closed/wall/mineral/snow",
        objs: DEFAULT_OBJS,
        window: None,
        window_colour: [0x00, 0xF7, 0xFF],
        spawns: &[
            "/obj/structure/flora/grass/both/style_random",
            "/obj/structure/flora/grass/brown/style_random",
            "/obj/structure/flora/grass/green/style_random",
        ],
        spawn_chance: 8.0,
    },
    Theme {
        name: "Winter Cabin",
        color: Some([0xFF, 0xFF, 0xFF]),
        sound: BLIND,
        floors: &[
            ("/turf/open/floor/fake_snow", 10),
            ("/turf/open/floor/fakeice/slippery", 1),
        ],
        replace_walls: "/turf/closed/wall/mineral/wood",
        objs: WOOD_OBJS,
        window: None,
        window_colour: [0x00, 0xF7, 0xFF],
        spawns: &[
            "/obj/structure/flora/grass/both/style_random",
            "/obj/structure/flora/grass/brown/style_random",
            "/obj/structure/flora/grass/green/style_random",
        ],
        spawn_chance: 8.0,
    },
    Theme {
        name: "Lavaland",
        color: None,
        sound: BLIND,
        floors: &[
            ("/turf/open/floor/fakebasalt", 5),
            ("/turf/open/floor/fakepit", 1),
        ],
        replace_walls: "/turf/closed/wall/mineral/cult",
        objs: &[(
            "/obj/structure/platform",
            &[("/obj/structure/platform/cult", 1)],
        )],
        window: None,
        window_colour: [0x86, 0x00, 0x00],
        spawns: &[],
        spawn_chance: 0.0,
    },
    Theme {
        name: "Space",
        color: Some([0x62, 0x92, 0xAF]),
        sound: BLIND,
        floors: &[("/turf/open/floor/fakespace", 1)],
        replace_walls: "/turf/closed/wall/rock/porous",
        objs: &[],
        window: None,
        window_colour: [0, 0, 0],
        spawns: &[],
        spawn_chance: 0.0,
    },
    Theme {
        floors: &[("/turf/open/floor/glass", 1)],
        ..material("Glass", [0x62, 0x92, 0xAF], "shatter", MATERIAL_FLOOR)
    },
    Theme {
        name: "Fancy",
        color: None,
        sound: BLIND,
        floors: &[
            ("/turf/open/floor/carpet/black", 1),
            ("/turf/open/floor/carpet/blue", 1),
            ("/turf/open/floor/carpet/cyan", 1),
            ("/turf/open/floor/carpet/green", 1),
            ("/turf/open/floor/carpet/orange", 1),
            ("/turf/open/floor/carpet/purple", 1),
            ("/turf/open/floor/carpet/red", 1),
            ("/turf/open/floor/carpet/royalblack", 1),
            ("/turf/open/floor/carpet/royalblue", 1),
        ],
        replace_walls: "/turf/closed/wall/mineral/wood/nonmetal",
        objs: &[
            ("/obj/structure/chair", &[("/obj/structure/chair/comfy", 1)]),
            (
                "/obj/structure/platform",
                &[("/obj/structure/platform/paper", 1)],
            ),
        ],
        window: None,
        window_colour: [0xFF, 0xFF, 0xFF],
        spawns: &[],
        spawn_chance: 0.0,
    },
    Theme {
        floors: &[("/turf/open/floor/light", 1)],
        ..material("Disco", [0x62, 0x92, 0xAF], BLIND, MATERIAL_FLOOR)
    },
    Theme {
        name: "Jungle",
        color: None,
        sound: "crunchy_bush_whack",
        floors: &[("/turf/open/floor/grass", 1)],
        replace_walls: "/turf/closed/wall/mineral/wood",
        objs: WOOD_OBJS,
        window: None,
        window_colour: [0xFF, 0xFF, 0xFF],
        spawns: &[
            "/obj/structure/flora/bush/ferny/style_random",
            "/obj/structure/flora/bush/grassy/style_random",
            "/obj/structure/flora/bush/leavy/style_random",
            "/obj/structure/flora/tree/palm/style_random",
            "/obj/structure/flora/bush/sparsegrass/style_random",
            "/obj/structure/flora/bush/sunny/style_random",
        ],
        spawn_chance: 20.0,
    },
    Theme {
        name: "Alien",
        color: Some([0x60, 0x41, 0xAA]),
        sound: BLIND,
        floors: &[("/turf/open/floor/mineral/abductor", 1)],
        replace_walls: "/turf/closed/wall/mineral/abductor",
        objs: &[
            (
                "/obj/structure/chair",
                &[
                    ("/obj/structure/chair/greyscale", 9),
                    ("/obj/structure/bed/abductor", 1),
                ],
            ),
            (
                "/obj/structure/table",
                &[
                    ("/obj/structure/table/greyscale", 9),
                    ("/obj/structure/table/abductor", 1),
                ],
            ),
            (
                "/obj/structure/toilet",
                &[("/obj/structure/toilet/greyscale", 1)],
            ),
            (
                "/obj/structure/platform",
                &[("/obj/structure/platform/uranium", 1)],
            ),
        ],
        window: None,
        window_colour: [0x60, 0x41, 0xAA],
        spawns: &[],
        spawn_chance: 0.0,
    },
    Theme {
        name: "Bronze",
        color: Some([0x87, 0x62, 0x23]),
        sound: "sound/effects/magic/clockwork/fellowship_armory.ogg",
        floors: &[
            ("/turf/open/floor/bronze", 1),
            ("/turf/open/floor/bronze/flat", 1),
            ("/turf/open/floor/bronze/filled", 1),
        ],
        replace_walls: "/turf/closed/wall/mineral/bronze",
        objs: &[
            (
                "/obj/structure/girder",
                &[("/obj/structure/girder/bronze", 1)],
            ),
            (
                "/obj/structure/table",
                &[("/obj/structure/table/bronze", 1)],
            ),
            (
                "/obj/structure/toilet",
                &[("/obj/structure/toilet/greyscale", 1)],
            ),
            (
                "/obj/structure/chair",
                &[("/obj/structure/chair/bronze", 1)],
            ),
            (
                "/obj/structure/platform",
                &[("/obj/structure/platform/bronze", 1)],
            ),
        ],
        window: Some("/obj/structure/window/bronze/fulltile"),
        window_colour: [0x87, 0x62, 0x23],
        spawns: &[],
        spawn_chance: 0.0,
    },
];

/// Every path a theme may put down (for `ss13_mapload::GAME_RUNTIME_TYPES`).
pub fn theme_paths() -> Vec<&'static str> {
    let mut out = Vec::new();
    for t in &THEMES {
        out.extend(t.floors.iter().map(|(p, _)| *p));
        out.push(t.replace_walls);
        for (_, list) in t.objs {
            out.extend(list.iter().map(|(p, _)| *p));
        }
        out.extend(t.window);
        out.extend(t.spawns.iter().copied());
    }
    out.sort_unstable();
    out.dedup();
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TurfClass {
    Floor,
    Wall,
}

/// `can_convert()`: a floor (themes with floors) or a wall, not space or indestructible.
fn convertible(world: &World, pos: TilePos, theme: &Theme) -> Option<TurfClass> {
    let path = turf_at(world, pos).and_then(|t| path_of(world, t))?;
    if is_under(&path, "/turf/open/floor") {
        (!theme.floors.is_empty()).then_some(TurfClass::Floor)
    } else if is_under(&path, "/turf/closed/wall") {
        Some(TurfClass::Wall)
    } else {
        None
    }
}

/// `spiral_range_turfs(range, src)`: the center, then each ring outwards.
fn spiral(center: TilePos, range: i32) -> Vec<TilePos> {
    let mut out = vec![center];
    for r in 1..=range {
        for dy in (-r..=r).rev() {
            for dx in -r..=r {
                if dx.abs().max(dy.abs()) == r {
                    out.push(TilePos::new(center.x + dx, center.y + dy, center.z));
                }
            }
        }
    }
    out
}

/// `prepare_area(new_theme_path)`: a theme (random without one) and the convertible turfs
/// around.
pub fn prepare_area(world: &mut World, e: Entity, theme: Option<usize>) {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let theme = theme.unwrap_or_else(|| {
        world
            .resource_mut::<EventRng>()
            .0
            .range(0, THEMES.len() as i64 - 1) as usize
    });
    let targets: Vec<TilePos> = spiral(pos, DIMENSION_RANGE)
        .into_iter()
        .filter(|&p| {
            world.resource::<SpatialIndex>().in_bounds(p)
                && convertible(world, p, &THEMES[theme]).is_some()
        })
        .collect();
    if let Some(mut d) = world.get_mut::<Dimensional>(e) {
        d.theme = Some(theme);
        d.targets = targets;
    }
}

/// `transmute_area()`: the next turf, or a relocation (the anomaly is gone after its last one
/// unless immortal).
pub(super) fn dimensional_effect(world: &mut World, e: Entity) {
    if world
        .get::<Dimensional>(e)
        .is_some_and(|d| d.theme.is_none())
    {
        prepare_area(world, e, None);
    }
    let Some(d) = world.get::<Dimensional>(e) else {
        return;
    };
    let Some(theme) = d.theme else {
        return;
    };
    if d.targets.is_empty() {
        let immortal = world.get::<Anomaly>(e).is_some_and(|a| a.immortal);
        if d.teleports_left == 0 {
            if !immortal {
                world.despawn(e);
            }
            return;
        }
        if let Some(mut d) = world.get_mut::<Dimensional>(e) {
            d.teleports_left -= 1;
        }
        relocate(world, e);
        return;
    }
    let pos = world
        .get_mut::<Dimensional>(e)
        .map(|mut d| d.targets.remove(0));
    if let Some(pos) = pos {
        apply_theme(world, &THEMES[theme], pos);
    }
}

/// `relocate()`: a valid tile of a new area, announced, and a new theme there.
fn relocate(world: &mut World, e: Entity) {
    let Some((area, tiles)) = find_valid_area(world) else {
        return;
    };
    let valid: Vec<TilePos> = tiles
        .into_iter()
        .filter(|&p| valid_destination(world, p))
        .collect();
    let Some(&to) = world.resource_mut::<EventRng>().pick(&valid) else {
        return;
    };
    let name = crate::shuttle::area_name(world, area);
    crate::events::priority_announce(
        world,
        "Обнаружена аномалия",
        &format!("Пространственная аномалия переместилась. Предполагаемая локация: {name}."),
    );
    world.entity_mut(e).insert(to);
    if let Some(mut a) = world.get_mut::<Anomaly>(e) {
        a.area = Some(area);
    }
    prepare_area(world, e, None);
}

fn pick_path(world: &mut World, list: &[(&'static str, i64)]) -> Option<&'static str> {
    super::super::controller::pick_weight(list, &mut world.resource_mut::<EventRng>().0)
}

fn tint(world: &mut World, e: Entity, color: [u8; 3]) {
    if let Some(mut app) = world.get_mut::<Appearance>(e) {
        app.color = Some([color[0], color[1], color[2], 255]);
    }
}

/// `apply_theme(turf, show_effect = TRUE)`.
pub fn apply_theme(world: &mut World, theme: &Theme, pos: TilePos) {
    let Some(class) = convertible(world, pos, theme) else {
        return;
    };
    let Some(old) = turf_at(world, pos) else {
        return;
    };
    let path = match class {
        TurfClass::Floor => pick_path(world, theme.floors),
        TurfClass::Wall => Some(theme.replace_walls),
    };
    let Some(path) = path else {
        return;
    };
    let stack = baseturfs_of(world, old);
    let Some(turf) = change_turf(world, pos, path, stack, true) else {
        return;
    };
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, theme.sound, 100.0, true),
    );
    let objs: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&o| world.get::<DmObj>(o).is_some() && world.get::<Turf>(o).is_none())
        .collect();
    for obj in objs {
        replace_object(world, theme, obj, pos);
    }
    if !theme.spawns.is_empty() && world.resource_mut::<EventRng>().0.prob(theme.spawn_chance) {
        let blocked = !world
            .resource::<SpatialIndex>()
            .dense_on_tile(pos)
            .iter()
            .all(|&b| world.get::<Health>(b).is_some());
        let spawn = world.resource_mut::<EventRng>().pick(theme.spawns).copied();
        if !blocked && let Some(spawn) = spawn {
            crate::construction::spawn_obj(world, spawn, pos);
        }
    }
    if let Some(color) = theme.color
        && (path == MATERIAL_WALL || path == MATERIAL_FLOOR[0].0)
    {
        tint(world, turf, color);
    }
}

/// `replace_object()` (and `transform_window()`, `apply_materials()` of what is left).
fn replace_object(world: &mut World, theme: &Theme, obj: Entity, pos: TilePos) {
    let Some(path) = path_of(world, obj) else {
        return;
    };
    if is_under(&path, "/obj/structure/window") {
        // `fulltile` types.
        if !path.contains("/fulltile") {
            return;
        }
        let target = match theme.window {
            Some(window) => {
                world.despawn(obj);
                match crate::construction::spawn_obj(world, window, pos) {
                    Some(w) => w,
                    None => return,
                }
            }
            None => obj,
        };
        tint(world, target, theme.window_colour);
        return;
    }
    let replacement = theme
        .objs
        .iter()
        .find(|(root, _)| is_under(&path, root))
        .map(|(_, list)| *list);
    let mut kept = obj;
    if let Some(list) = replacement
        && let Some(new_path) = pick_path(world, list)
    {
        let dir = world.get::<ss13_shared::Facing>(obj).copied();
        if let Some(new) = crate::construction::spawn_obj(world, new_path, pos) {
            if let Some(dir) = dir {
                world.entity_mut(new).insert(dir);
            }
            world.despawn(obj);
            kept = new;
        }
    }
    // `permit_replace_material`.
    const PERMITTED: [&str; 6] = [
        "/obj/structure/chair",
        "/obj/structure/table",
        "/obj/structure/toilet",
        "/obj/structure/window",
        "/obj/structure/sink",
        "/obj/structure/platform",
    ];
    if let Some(color) = theme.color
        && path_of(world, kept).is_some_and(|p| PERMITTED.iter().any(|r| is_under(&p, r)))
    {
        tint(world, kept, color);
    }
}

// --- Ectoplasm ----------------------------------------------------------------------------

/// `/obj/effect/anomaly/ectoplasm`: `effect_power` (percent) and `ghosts_orbiting`;
/// `override_ghosts` pins them (admin setup).
#[derive(Component, Debug, Default)]
pub struct Ectoplasm {
    pub power: f32,
    pub orbiting: u32,
    pub override_ghosts: bool,
}

/// The ectoplasm `anomalyEffect`: the share of the dead players orbiting it.
pub(super) fn ectoplasm_effect(world: &mut World, e: Entity, pos: TilePos) {
    if world.get::<Ectoplasm>(e).is_none_or(|s| s.override_ghosts) {
        return;
    }
    let orbiting = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .filter(|&&g| world.get::<Ghost>(g).is_some() && world.get::<PlayerMob>(g).is_some())
        .count() as u32;
    // `length(GLOB.dead_player_list) + length(GLOB.current_observers_list)`.
    let mut q = world.query_filtered::<(Option<&Ghost>, Option<&Stat>), With<PlayerMob>>();
    let dead = q
        .iter(world)
        .filter(|(ghost, stat)| ghost.is_some() || matches!(stat, Some(Stat::Dead)))
        .count() as u32;
    let power = if orbiting > 0 && dead > 0 {
        orbiting as f32 / dead as f32 * 100.0
    } else {
        0.0
    };
    if let Some(mut s) = world.get_mut::<Ectoplasm>(e) {
        s.orbiting = orbiting;
        s.power = power;
    }
    // `intensity_update()`.
    let state = if power >= 50.0 {
        "ectoplasm_heavy"
    } else {
        "ectoplasm"
    };
    crate::obj_damage::set_icon_state(world, e, state);
}

/// The ectoplasm `detonate()`.
pub(super) fn ectoplasm_detonate(world: &mut World, e: Entity, pos: TilePos) {
    let Some((power, orbiting)) = world.get::<Ectoplasm>(e).map(|s| (s.power, s.orbiting)) else {
        return;
    };
    if power < 10.0 {
        return;
    }
    let range = orbiting + 3;
    defile(world, pos, range);
    if power >= 50.0 {
        spawn_portal(world, pos);
        crate::events::priority_announce(
            world,
            "Обнаружена аномалия",
            "Аномалия достигла критической массы. Обнаружен всплеск эктоплазмы.",
        );
    }
}

/// The revenant-defile part of the detonation within `range`.
fn defile(world: &mut World, center: TilePos, range: u32) {
    let r = range as i32;
    for dy in -r..=r {
        for dx in -r..=r {
            let pos = TilePos::new(center.x + dx, center.y + dy, center.z);
            if !world.resource::<SpatialIndex>().in_bounds(pos) {
                continue;
            }
            let Some(turf) = turf_at(world, pos) else {
                continue;
            };
            let Some(path) = path_of(world, turf) else {
                continue;
            };
            if is_under(&path, "/turf/open/floor") {
                let rng = &mut world.resource_mut::<EventRng>().0;
                let decal = if rng.prob(5.0) {
                    Some("/obj/effect/decal/cleanable/blood")
                } else if rng.prob(10.0) {
                    Some("/obj/effect/decal/cleanable/greenglow/ecto")
                } else if rng.prob(10.0) {
                    Some("/obj/effect/decal/cleanable/dirt/dust")
                } else {
                    None
                };
                if let Some(decal) = decal {
                    crate::construction::spawn_obj(world, decal, pos);
                }
                let tile = world
                    .get::<ss13_mapload::DmTurfBuild>(turf)
                    .filter(|b| b.overfloor_placed && !b.floor_tile.is_empty())
                    .map(|b| b.floor_tile.clone());
                if !is_under(&path, "/turf/open/floor/plating")
                    && let Some(tile) = tile
                    && world.resource_mut::<EventRng>().0.prob(20.0)
                {
                    crate::construction::spawn_obj(world, &tile, pos);
                    if crate::construction::scrape_away(world, pos, true).is_some() {
                        crate::turf_damage::break_tile(world, pos);
                        crate::turf_damage::burn_tile(world, pos);
                    }
                }
            }
            let windows: Vec<Entity> = world
                .resource::<SpatialIndex>()
                .on_tile(pos)
                .iter()
                .copied()
                .filter(|&w| {
                    path_of(world, w).is_some_and(|p| is_under(&p, "/obj/structure/window"))
                })
                .collect();
            for w in windows {
                let damage = world.resource_mut::<EventRng>().0.range(60, 90) as f32;
                crate::obj_damage::take_damage(world, w, damage, DamageType::Brute);
            }
        }
    }
}

/// `/obj/structure/ghost_portal`: gone at `until`.
#[derive(Component, Debug)]
pub struct GhostPortal {
    pub until: f64,
}

fn spooky_noise(world: &mut World, pos: TilePos) {
    let sound = world
        .resource_mut::<EventRng>()
        .pick(&SPOOKY_NOISES)
        .copied();
    if let Some(sound) = sound {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, sound, 100.0, true));
    }
}

fn spawn_portal(world: &mut World, pos: TilePos) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let portal = crate::construction::spawn_obj(world, GHOST_PORTAL, pos)
        .unwrap_or_else(|| world.spawn((Name::new(GHOST_PORTAL), pos)).id());
    world.entity_mut(portal).insert(GhostPortal {
        until: now + PORTAL_SECS,
    });
    spooky_noise(world, pos);
}

/// The portals' `process()` (a spooky noise 5% of the fires) and `QDEL_IN`.
pub(super) fn process_portals(world: &mut World, now: f64, secs: f64) {
    let mut q = world.query::<(Entity, &GhostPortal, &TilePos)>();
    let portals: Vec<(Entity, f64, TilePos)> = q
        .iter(world)
        .map(|(e, p, pos)| (e, p.until, *pos))
        .collect();
    for (e, until, pos) in portals {
        if until <= now {
            world.despawn(e);
            continue;
        }
        let chance = 100.0 * (1.0 - 0.95f64.powf(secs / 2.0));
        if world.resource_mut::<EventRng>().0.prob(chance as f32) {
            spooky_noise(world, pos);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spiral_starts_at_the_center_and_covers_the_square() {
        let c = TilePos::new(10, 10, 1);
        let s = spiral(c, DIMENSION_RANGE);
        assert_eq!(s[0], c);
        assert_eq!(s.len(), 49);
        assert!(s[1..9].iter().all(|p| c.chebyshev(*p) == Some(1)));
    }

    #[test]
    fn scramble_pool_skips_blacklisted_and_robotic_organs() {
        let pool = scramble_organs();
        assert!(!pool.is_empty());
        assert!(pool.iter().all(|p| Slot::of_path(p).is_some()));
        assert!(pool.iter().all(
            |p| !is_under(p, "/obj/item/organ/brain") && !is_under(p, "/obj/item/organ/lungs")
        ));
    }
}

// --- Weather ------------------------------------------------------------------------------

/// `/obj/effect/anomaly/weather`: the weather it started (`active_weathers`).
#[derive(Component, Debug)]
pub struct WeatherAnomaly {
    pub weather: crate::weather::WeatherKind,
    pub thundering: bool,
}

const REPULSE: &str = "sound/effects/magic/repulse.ogg";

/// The weather anomaly's `Initialize`: a rain, snow or sand storm (`select_weather`; the
/// thundering kind always rains, with `THUNDER_CHANCE_HIGH`) over its area and the adjacent ones
/// found stepping out up to 9 tiles each way (not those over 1.5 times its size), indoors,
/// telegraphing a fifth of its life, winding down a fifteenth.
pub fn start_weather(world: &mut World, e: Entity, thundering: bool) {
    use crate::weather::{Forced, THUNDER_CHANCE_HIGH, WeatherKind, run_weather_forced};
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let kind = if thundering {
        WeatherKind::RainStorm
    } else {
        let options = [
            WeatherKind::RainStorm,
            WeatherKind::SnowStorm,
            WeatherKind::SandStorm,
        ];
        *world
            .resource_mut::<EventRng>()
            .pick(&options)
            .unwrap_or(&WeatherKind::RainStorm)
    };
    let areas_res = world.get_resource::<ss13_mapload::Areas>();
    let impact = world
        .get::<Anomaly>(e)
        .and_then(|a| a.area)
        .or_else(|| areas_res.and_then(|a| a.area_at(pos)));
    let Some(impact) = impact else {
        return;
    };
    let mut sizes: std::collections::HashMap<Entity, usize> = std::collections::HashMap::new();
    for (area, _) in crate::events::station_tiles(world) {
        *sizes.entry(area).or_default() += 1;
    }
    let own = sizes.get(&impact).copied().unwrap_or(0) as f64;
    let mut areas = std::collections::HashSet::from([impact]);
    for (dx, dy) in [
        (0, 1),
        (1, 1),
        (1, 0),
        (1, -1),
        (0, -1),
        (-1, -1),
        (-1, 0),
        (-1, 1),
    ] {
        // `find_adjacent_impacted_area`.
        let area_of = |p: TilePos| {
            world
                .get_resource::<ss13_mapload::Areas>()
                .and_then(|a| a.area_at(p))
        };
        let mut next = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
        let mut limit = 9;
        while area_of(next) == Some(impact) && limit > 0 {
            next = TilePos::new(next.x + dx, next.y + dy, next.z);
            limit -= 1;
        }
        if let Some(nearby) = area_of(next)
            && (sizes.get(&nearby).copied().unwrap_or(0) as f64) <= own * 1.5
        {
            areas.insert(nearby);
        }
    }
    let lifespan = world.get::<Anomaly>(e).map_or(0.0, |a| {
        a.death_time - world.resource::<Time>().elapsed_secs_f64()
    });
    let telegraph = lifespan / 5.0;
    let end = lifespan / 15.0;
    run_weather_forced(
        world,
        kind,
        vec![pos.z],
        Forced {
            areas,
            telegraph,
            duration: lifespan - telegraph - end,
            end,
            thunder: if thundering { THUNDER_CHANCE_HIGH } else { 0.0 },
            source: Some(e),
        },
    );
    world.entity_mut(e).insert(WeatherAnomaly {
        weather: kind,
        thundering,
    });
    if thundering && let Some(mut name) = world.get_mut::<ss13_mapload::DmName>(e) {
        name.name = "thundering weather anomaly".to_owned();
    }
}

/// The weather anomaly's `detonate()`: a repulse throwing everything loose within 5 tiles
/// 6 tiles away from it.
// shortcut: mobs are not thrown (no mob throws yet).
pub(super) fn weather_detonate(world: &mut World, e: Entity, pos: TilePos) {
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, REPULSE, 100.0, true),
    );
    let near: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .in_radius(pos, 5)
        .filter(|&t| t != e && super::loose_obj(world, t))
        .collect();
    for t in near {
        let Some(&at) = world.get::<TilePos>(t) else {
            continue;
        };
        let (dx, dy) = ((at.x - pos.x).signum(), (at.y - pos.y).signum());
        let (dx, dy) = if (dx, dy) == (0, 0) { (0, 1) } else { (dx, dy) };
        let to = TilePos::new(at.x + dx * 6, at.y + dy * 6, at.z);
        crate::throw::throw_at(world, t, to, 6, 2.0);
    }
}
