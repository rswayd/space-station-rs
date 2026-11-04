//! Meteors (`code/modules/meteors/`) and their events (`code/modules/events/meteors/`,
//! `code/modules/events/dust.dm`): meteor waves, Major Space Dust, Space Dust: Minor and the
//! Stray Meteor.
//!
//! A meteor ([`Meteor`]) spawns on a space tile 13 tiles in from a random edge of a station
//! level and flies a straight line to a random point by the opposite edge, one tile every
//! 0.1 s (tg `move_towards`, `delay = 1`). Entering a tile rams it (`ram_turf`: `ex_act` with
//! the meteor's `hitpwr`) and, off space, takes a hit 10% of the time; a dense thing in the way
//! is bumped: rammed, and a hit taken. Out of hits it drops its ore and goes off (its
//! `meteor_effect`: an [`explosion`](super::explosion::explosion) for the bigger ones).
//!
//! Cost: a wave keeps up to ~100 meteors in flight, 10 steps per second each, every step one
//! tile lookup; far below a millisecond per tick.

use bevy::prelude::*;
use ss13_mapload::{DmName, IconIndex};
use ss13_net::server::Networked;
use ss13_shared::appearance::{Layer, MouseOpacity};
use ss13_shared::{Appearance, IconStateId, TilePos};
use ss13_world::{Density, SpatialIndex};

use super::controller::{EventRng, EventState, RoundEvent, pick_weight, station_levels};
use super::explosion::{Severity, ex_act_movable, ex_act_tile, ex_act_turf, explosion};
use crate::construction::{path_of, spawn_obj, spawn_stack, turf_at};
use crate::obj_damage::is_under;

pub const WAVE_NORMAL: &str = "Meteor Wave: Normal";
pub const WAVE_THREATENING: &str = "Meteor Wave: Threatening";
pub const WAVE_CATASTROPHIC: &str = "Meteor Wave: Catastrophic";
pub const WAVE_MEATY: &str = "Meteor Wave: Meaty";
pub const DUST_STORM: &str = "Major Space Dust";
pub const SPACE_DUST: &str = "Space Dust: Minor";
pub const STRAY_METEOR: &str = "Stray Meteor";

const ICON: &str = "icons/obj/meteor.dmi";
/// `TRANSITIONEDGE + MAP_EDGE_PAD`.
const EDGE: i32 = 8 + 5;
/// `move_towards` with `delay = 1` decisecond.
const MOVE_SECS: f64 = 0.1;
/// `DEFAULT_METEOR_LIFETIME` (deciseconds), the move loop's timeout.
const LIFETIME_SECS: f64 = 180.0;
/// `/obj/effect` on `GAME_PLANE`, `OBJ_LAYER`.
const GAME_PLANE: i16 = -4;
const OBJ_LAYER: f32 = 3.0;
/// `/obj/effect/meteor/cluster` `cluster_count`.
const CLUSTER_COUNT: usize = 8;

const ORE_IRON: &str = "/obj/item/stack/ore/iron";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Special {
    None,
    /// Spawns 8 fragments flying to random edges when it goes off.
    Cluster,
    /// 35% to teleport up to 6 tiles away after a bump.
    Bluespace,
    /// Phases through everything, knocks mobs down and takes a hit on every tile.
    Banana,
    /// Rams nothing on moving; a bump `ex_act`s only what it hit.
    Meaty,
    /// `meteor/emp/meteor_effect`: `empulse(src, 3, 8)`.
    Emp,
    /// 20% to explode on a bump.
    Tunguska,
}

/// One `/obj/effect/meteor` subtype.
#[derive(Debug)]
pub struct MeteorKind {
    pub path: &'static str,
    name: &'static str,
    icon_state: &'static str,
    color: Option<[u8; 4]>,
    hits: i32,
    hitpwr: Severity,
    dropamt: u32,
    drops: &'static [&'static str],
    /// `explosion(devastation, heavy, light, flame)` of `meteor_effect`.
    blast: Option<[i32; 4]>,
    /// `PASSGRILLE` (space dust).
    pass_grille: bool,
    /// `signature`: what "detected" it in the Stray Meteor announcement.
    signature: &'static str,
    special: Special,
}

const fn kind(path: &'static str, name: &'static str, icon_state: &'static str) -> MeteorKind {
    MeteorKind {
        path,
        name,
        icon_state,
        color: None,
        hits: 4,
        hitpwr: Severity::Heavy,
        dropamt: 2,
        drops: &[ORE_IRON],
        blast: None,
        pass_grille: false,
        signature: "движения",
        special: Special::None,
    }
}

// shortcut: no screen shake or impact sound for `heavy` meteors, no spin animation, no
// greenglow from irradiated ones, no zap sound of electromagnetic ones and no carp from frozen ones
// (no simple mobs); bananium ones do not throw things around.
static DUST: MeteorKind = MeteorKind {
    hits: 1,
    hitpwr: Severity::Light,
    drops: &["/obj/item/stack/ore/glass"],
    pass_grille: true,
    ..kind("/obj/effect/meteor/dust", "space dust", "dust")
};
static MEDIUM: MeteorKind = MeteorKind {
    dropamt: 3,
    blast: Some([0, 1, 2, 0]),
    ..kind("/obj/effect/meteor/medium", "meteor", "small")
};
static BIG: MeteorKind = MeteorKind {
    hits: 6,
    dropamt: 4,
    blast: Some([1, 2, 3, 0]),
    ..kind("/obj/effect/meteor/big", "big meteor", "large")
};
static FLAMING: MeteorKind = MeteorKind {
    hits: 5,
    drops: &["/obj/item/stack/ore/plasma"],
    blast: Some([1, 2, 3, 5]),
    signature: "тепла",
    ..kind("/obj/effect/meteor/flaming", "flaming meteor", "flaming")
};
static IRRADIATED: MeteorKind = MeteorKind {
    hits: 9,
    drops: &["/obj/item/stack/ore/uranium"],
    blast: Some([0, 1, 3, 0]),
    signature: "радиации",
    ..kind("/obj/effect/meteor/irradiated", "glowing meteor", "glowing")
};
static CLUSTER: MeteorKind = MeteorKind {
    hits: 9,
    blast: Some([0, 2, 3, 0]),
    signature: "артиллерии",
    special: Special::Cluster,
    ..kind("/obj/effect/meteor/cluster", "cluster meteor", "sharp")
};
static CLUSTER_FRAGMENT: MeteorKind = kind(
    "/obj/effect/meteor/cluster_fragment",
    "cluster meteor fragment",
    "dust",
);
static CARP: MeteorKind = MeteorKind {
    dropamt: 1,
    drops: &[],
    signature: "рыболовства",
    ..kind("/obj/effect/meteor/carp", "frozen carp", "carp")
};
static BLUESPACE: MeteorKind = MeteorKind {
    hits: 12,
    dropamt: 3,
    drops: &["/obj/item/stack/ore/bluespace_crystal"],
    signature: "блюспейс потоков",
    special: Special::Bluespace,
    ..kind(
        "/obj/effect/meteor/bluespace",
        "bluespace meteor",
        "bluespace",
    )
};
static BANANA: MeteorKind = MeteorKind {
    hits: 175,
    dropamt: 4,
    drops: &["/obj/item/stack/ore/bananium"],
    signature: "комедии",
    special: Special::Banana,
    ..kind("/obj/effect/meteor/banana", "bananium meteor", "bananium")
};
static EMP: MeteorKind = MeteorKind {
    hits: 6,
    signature: "электромагнитных помех",
    special: Special::Emp,
    ..kind(
        "/obj/effect/meteor/emp",
        "electromagnetically charged meteor",
        "bluespace",
    )
};
const MEATY_DROPS: &[&str] = &[
    "/obj/item/food/meat/slab/human",
    "/obj/item/organ/heart",
    "/obj/item/organ/lungs",
    "/obj/item/organ/appendix",
];
static MEATY: MeteorKind = MeteorKind {
    hits: 2,
    drops: MEATY_DROPS,
    signature: "кулинарных ингридиентов",
    special: Special::Meaty,
    ..kind("/obj/effect/meteor/meaty", "meaty ore", "meateor")
};
static MEATY_XENO: MeteorKind = MeteorKind {
    color: Some([0x5E, 0xFF, 0x00, 255]),
    drops: &[
        "/obj/item/food/meat/slab/xeno",
        "/obj/item/organ/tongue/alien",
    ],
    signature: "экзотических кулинарных ингридиентов",
    ..MeteorKind {
        path: "/obj/effect/meteor/meaty/xeno",
        ..MEATY
    }
};
static TUNGUSKA: MeteorKind = MeteorKind {
    hits: 30,
    hitpwr: Severity::Devastate,
    drops: &["/obj/item/stack/ore/plasma"],
    blast: Some([5, 10, 15, 0]),
    signature: "армаггедона",
    special: Special::Tunguska,
    ..kind("/obj/effect/meteor/tunguska", "tunguska meteor", "flaming")
};

/// Every ported meteor type, for lookups by path.
static KINDS: [&MeteorKind; 14] = [
    &DUST,
    &MEDIUM,
    &BIG,
    &FLAMING,
    &IRRADIATED,
    &CLUSTER,
    &CLUSTER_FRAGMENT,
    &CARP,
    &BLUESPACE,
    &BANANA,
    &EMP,
    &MEATY,
    &MEATY_XENO,
    &TUNGUSKA,
];

type Wave = &'static [(&'static MeteorKind, i64)];

/// `GLOB.meteors_normal` and the other wave lists (`meteor_waves.dm`).
pub static METEORS_NORMAL: Wave = &[
    (&DUST, 3),
    (&MEDIUM, 8),
    (&BIG, 3),
    (&FLAMING, 1),
    (&IRRADIATED, 2),
    (&CARP, 1),
    (&BLUESPACE, 1),
    (&BANANA, 1),
    (&EMP, 1),
];
pub static METEORS_THREATENING: Wave = &[
    (&MEDIUM, 4),
    (&BIG, 8),
    (&FLAMING, 3),
    (&IRRADIATED, 3),
    (&CLUSTER, 1),
    (&CARP, 1),
    (&BLUESPACE, 2),
    (&EMP, 2),
];
// shortcut: no Halloween (`meteorsSPOOKY` instead of the catastrophic list): no holidays yet.
pub static METEORS_CATASTROPHIC: Wave = &[
    (&MEDIUM, 5),
    (&BIG, 75),
    (&FLAMING, 10),
    (&IRRADIATED, 8),
    (&CLUSTER, 8),
    (&TUNGUSKA, 1),
    (&CARP, 2),
    (&BLUESPACE, 10),
    (&EMP, 8),
];
pub static MEATEORS: Wave = &[(&MEATY, 5), (&MEATY_XENO, 1)];
pub static METEORS_DUST: Wave = &[(&DUST, 1)];
pub static METEORS_STRAY: Wave = &[
    (&MEDIUM, 15),
    (&BIG, 10),
    (&FLAMING, 25),
    (&IRRADIATED, 30),
    (&CARP, 25),
    (&BLUESPACE, 30),
    (&BANANA, 25),
    (&MEATY, 10),
    (&MEATY_XENO, 8),
    (&EMP, 30),
    (&CLUSTER, 20),
    (&TUNGUSKA, 1),
];

pub fn kind_by_path(path: &str) -> Option<&'static MeteorKind> {
    KINDS.iter().copied().find(|k| k.path == path)
}

/// A flying `/obj/effect/meteor`.
#[derive(Component, Debug)]
pub struct Meteor {
    pub kind: &'static MeteorKind,
    pub hits: i32,
    pub dest: TilePos,
    /// `move_towards`: steps per move along each axis (the longer axis is 1) and the
    /// fractional progress along the shorter one.
    rate: (f64, f64),
    sign: (i32, i32),
    ticker: (f64, f64),
    next_move: f64,
    expires: f64,
}

// --- Events -------------------------------------------------------------------------------

/// `/datum/round_event/meteor_wave` and its subtypes.
struct MeteorWave {
    wave: Wave,
    announce: fn(&mut World),
}

fn wave(wave: Wave, announce: fn(&mut World)) -> Box<dyn RoundEvent> {
    Box::new(MeteorWave { wave, announce })
}

fn announce_meteors(world: &mut World) {
    super::priority_announce_sound(
        world,
        "Метеоры",
        "Зафиксировано движение метеоритов на встречном со станцией курсе.",
        crate::announce::ANNOUNCER_METEORS,
    );
}

pub fn make_normal(_: &mut World) -> Box<dyn RoundEvent> {
    wave(METEORS_NORMAL, announce_meteors)
}

pub fn make_threatening(_: &mut World) -> Box<dyn RoundEvent> {
    wave(METEORS_THREATENING, announce_meteors)
}

pub fn make_catastrophic(_: &mut World) -> Box<dyn RoundEvent> {
    wave(METEORS_CATASTROPHIC, announce_meteors)
}

pub fn make_meaty(_: &mut World) -> Box<dyn RoundEvent> {
    wave(MEATEORS, |world| {
        super::priority_announce_sound(
            world,
            "Мясориты",
            "Зафиксировано движение мясоритов на встречном со станцией курсе.",
            crate::announce::ANNOUNCER_METEORS,
        );
    })
}

/// `/datum/round_event/meteor_wave/dust_storm`: `announce_chance = 85`.
struct DustStorm(MeteorWave);

pub fn make_dust_storm(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(DustStorm(MeteorWave {
        wave: METEORS_DUST,
        announce: announce_dust_storm,
    }))
}

fn announce_dust_storm(world: &mut World) {
    let station = super::station_name(world);
    let mut rng = world.resource_mut::<EventRng>();
    let weapon = *rng
        .pick(&[
            "флагманского крейсера",
            "электромагнитной пушки",
            "нано-квантового ускорителя",
            "реактивной артиллерии",
            "[ДАННЫЕ УДАЛЕНЫ]",
        ])
        .expect("non-empty");
    let reasons = [
        format!(
            "{station} проходит через облако обломков, возможны незначительные повреждения \
             внешнего оборудования."
        ),
        format!(
            "Отдел экспериментального вооружения Нанотрейзен испытывает новый прототип {weapon} \
             возможны остатки космического мусора."
        ),
        "Зафиксированы незначительные космические обьекты на встречном со станцией курсе."
            .to_owned(),
        format!(
            "Орбита {station} проходит через остатки астероида после шахтерской выработки. \
             Ожидаются незначительные повреждения корпуса."
        ),
        format!(
            "На встречном курсе с {station} был уничтожен крупный метеороид. Остатки обломков \
             могут повредить обшивку станции."
        ),
        format!(
            "{station} проходит через опасный участок космоса. Не обращайте внимания на \
             турбулентность и повреждения от обломков."
        ),
    ];
    let reason = rng.pick(&reasons).expect("non-empty").clone();
    super::priority_announce(world, "Предупреждение о столкновении", &reason);
}

impl RoundEvent for MeteorWave {
    fn setup(&mut self, _: &mut World, ev: &mut EventState) {
        ev.start_when = 6;
        ev.end_when = 66;
        ev.announce_when = 1;
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, _: bool) {
        (self.announce)(world);
    }

    fn tick(&mut self, world: &mut World, ev: &mut EventState) {
        if ev.active_for.is_multiple_of(3) {
            spawn_meteors(world, 5, self.wave);
        }
    }
}

impl RoundEvent for DustStorm {
    fn setup(&mut self, world: &mut World, ev: &mut EventState) {
        self.0.setup(world, ev);
        ev.announce_chance = 85.0;
    }

    fn announce(&mut self, world: &mut World, ev: &mut EventState, fake: bool) {
        self.0.announce(world, ev, fake);
    }

    fn tick(&mut self, world: &mut World, ev: &mut EventState) {
        self.0.tick(world, ev);
    }
}

/// `/datum/round_event/space_dust`.
struct SpaceDust;

pub fn make_space_dust(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(SpaceDust)
}

impl RoundEvent for SpaceDust {
    fn setup(&mut self, _: &mut World, ev: &mut EventState) {
        ev.start_when = 1;
        ev.end_when = 2;
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        spawn_meteors(world, 1, METEORS_DUST);
    }
}

/// `/datum/round_event/stray_meteor`.
struct StrayMeteor;

pub fn make_stray(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(StrayMeteor)
}

impl RoundEvent for StrayMeteor {
    fn setup(&mut self, _: &mut World, ev: &mut EventState) {
        ev.announce_when = 1;
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        spawn_meteor(world, METEORS_STRAY);
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, _: bool) {
        let mut q = world.query::<&Meteor>();
        let signatures: Vec<&'static str> = q.iter(world).map(|m| m.kind.signature).collect();
        let Some(&signature) = world.resource_mut::<EventRng>().pick(&signatures) else {
            return;
        };
        let text = format!(
            "Наши датчики {signature} обнаружили приближающуюся к {} сигнатуру. Приготовьтесь к \
             столкновению.",
            super::station_name(world)
        );
        super::priority_announce(world, "Метеоры", &text);
    }
}

// --- Spawning -----------------------------------------------------------------------------

/// tg `spawn_meteors(number, meteor_types)`.
pub fn spawn_meteors(world: &mut World, number: usize, wave: Wave) {
    for _ in 0..number {
        spawn_meteor(world, wave);
    }
}

/// tg `spawn_meteor(meteor_types)`: up to 10 tries for a space start tile on a random side of
/// a random station level, flying to the opposite side.
pub fn spawn_meteor(world: &mut World, wave: Wave) -> Option<Entity> {
    let levels: Vec<u16> = station_levels(world).collect();
    for _ in 0..10 {
        let mut rng = world.resource_mut::<EventRng>();
        let side = rng.0.range(0, 3) as usize;
        let &z = rng.pick(&levels)?;
        let size = world.resource::<SpatialIndex>().level_size(z)?;
        let rng = &mut world.resource_mut::<EventRng>().0;
        let start = debris_start(side, z, size, rng);
        let goal = debris_finish(side, z, size, rng);
        let (Some(start), Some(goal)) = (start, goal) else {
            continue;
        };
        if !is_space(world, start) {
            continue;
        }
        let kind = pick_weight(wave, &mut world.resource_mut::<EventRng>().0)?;
        return spawn_meteor_at(world, kind, start, goal);
    }
    None
}

/// Sides in `GLOB.cardinals` order: north, south, east, west.
fn debris_start(
    side: usize,
    z: u16,
    (w, h): (u32, u32),
    rng: &mut ss13_mapload::spawner::Rng,
) -> Option<TilePos> {
    let (w, h) = (w as i32, h as i32);
    let (x, y) = match side {
        0 => (
            rng.range(i64::from(EDGE), i64::from(w - EDGE)) as i32,
            h - EDGE,
        ),
        1 => (rng.range(i64::from(EDGE), i64::from(w - EDGE)) as i32, EDGE),
        2 => (
            w - EDGE,
            rng.range(i64::from(EDGE), i64::from(h - EDGE)) as i32,
        ),
        _ => (EDGE, rng.range(i64::from(EDGE), i64::from(h - EDGE)) as i32),
    };
    (x >= 1 && y >= 1 && x <= w && y <= h).then(|| TilePos::new(x, y, z))
}

/// tg `spaceDebrisFinishLoc`: by the opposite edge.
fn debris_finish(
    side: usize,
    z: u16,
    (w, h): (u32, u32),
    rng: &mut ss13_mapload::spawner::Rng,
) -> Option<TilePos> {
    let (w, h) = (w as i32, h as i32);
    let (x, y) = match side {
        0 => (rng.range(i64::from(EDGE), i64::from(w - EDGE)) as i32, EDGE),
        1 => (
            rng.range(i64::from(EDGE), i64::from(w - EDGE)) as i32,
            h - EDGE,
        ),
        2 => (EDGE, rng.range(i64::from(EDGE), i64::from(h - EDGE)) as i32),
        _ => (
            w - EDGE,
            rng.range(i64::from(EDGE), i64::from(h - EDGE)) as i32,
        ),
    };
    (x >= 1 && y >= 1 && x <= w && y <= h).then(|| TilePos::new(x, y, z))
}

fn turf_path(world: &World, pos: TilePos) -> Option<String> {
    turf_at(world, pos).and_then(|t| path_of(world, t))
}

fn is_space(world: &World, pos: TilePos) -> bool {
    turf_path(world, pos).is_some_and(|p| is_under(&p, "/turf/open/space"))
}

/// `new kind(start, goal)`: a meteor on `start` flying at `goal`.
pub fn spawn_meteor_at(
    world: &mut World,
    kind: &'static MeteorKind,
    start: TilePos,
    goal: TilePos,
) -> Option<Entity> {
    let icon = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(ICON, kind.icon_state))
        .unwrap_or(IconStateId(u32::MAX));
    let now = world.resource::<Time>().elapsed_secs_f64();
    let (dx, dy) = (goal.x - start.x, goal.y - start.y);
    let (ax, ay) = (f64::from(dx.abs()), f64::from(dy.abs()));
    let rate = if ax == 0.0 && ay == 0.0 {
        (0.0, 0.0)
    } else if ax > ay {
        (1.0, ay / ax)
    } else {
        (ax / ay, 1.0)
    };
    let entity = world
        .spawn((
            Name::new(kind.path),
            DmName {
                name: kind.name.to_owned(),
                desc: String::new(),
            },
            start,
            Appearance {
                icon_state: icon,
                layer: Layer(OBJ_LAYER),
                plane: GAME_PLANE,
                color: kind.color,
                pixel_offset: [0, 0],
                rotation: 0,
                overlays: vec![],
                mouse_opacity: MouseOpacity::Icon,
            },
            Density,
            Networked,
            Meteor {
                kind,
                hits: kind.hits,
                dest: goal,
                rate,
                sign: (dx.signum(), dy.signum()),
                ticker: (0.0, 0.0),
                next_move: now + MOVE_SECS,
                expires: now + LIFETIME_SECS,
            },
        ))
        .id();
    Some(entity)
}

// --- Flight -------------------------------------------------------------------------------

/// Moves every meteor its due steps.
pub(super) fn move_meteors(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query_filtered::<Entity, With<Meteor>>();
    let meteors: Vec<Entity> = q.iter(world).collect();
    for e in meteors {
        while let Some(meteor) = world.get::<Meteor>(e) {
            if meteor.expires <= now {
                world.despawn(e);
                break;
            }
            if meteor.next_move > now {
                break;
            }
            world.get_mut::<Meteor>(e).expect("meteor").next_move += MOVE_SECS;
            step(world, e);
        }
    }
}

/// One `move_towards` step: `Move()` into the next tile, or `Bump()` what blocks it.
fn step(world: &mut World, e: Entity) {
    let (Some(&pos), Some(meteor)) = (world.get::<TilePos>(e), world.get::<Meteor>(e)) else {
        return;
    };
    let kind = meteor.kind;
    let mut ticker = (
        meteor.ticker.0 + meteor.rate.0,
        meteor.ticker.1 + meteor.rate.1,
    );
    let (sx, sy) = (ticker.0.floor() as i32, ticker.1.floor() as i32);
    ticker = (ticker.0.fract(), ticker.1.fract());
    let next = TilePos::new(
        pos.x + sx * meteor.sign.0,
        pos.y + sy * meteor.sign.1,
        pos.z,
    );
    if next == pos {
        // At the goal already (`home` reached): the loop has nothing left to do.
        world.despawn(e);
        return;
    }
    if !world.resource::<SpatialIndex>().in_bounds(next) {
        world.despawn(e);
        return;
    }
    if kind.special != Special::Banana
        && let Some(blocker) = blocker(world, e, next, kind)
    {
        bump(world, e, blocker, next);
        return;
    }
    world.get_mut::<Meteor>(e).expect("meteor").ticker = ticker;
    world.entity_mut(e).insert(next);
    moved(world, e, next);
}

/// The first dense thing on `pos` this meteor cannot pass (`PASSTABLE`, dust `PASSGRILLE`).
fn blocker(world: &World, me: Entity, pos: TilePos, kind: &MeteorKind) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .dense_on_tile(pos)
        .iter()
        .copied()
        .filter(|&d| d != me)
        .find(|&d| {
            let path = path_of(world, d).unwrap_or_default();
            !is_under(&path, "/obj/structure/table")
                && !(kind.pass_grille && is_under(&path, crate::obj_damage::GRILLE))
        })
}

/// `/obj/effect/meteor/Moved`: ram the new tile; off space, a hit 10% of the time; gone at the
/// goal.
fn moved(world: &mut World, e: Entity, pos: TilePos) {
    let kind = world.get::<Meteor>(e).expect("meteor").kind;
    ram_turf(world, e, pos, kind);
    if world.get_entity(e).is_err() {
        return;
    }
    if !is_space(world, pos) && world.resource_mut::<EventRng>().0.prob(10.0) {
        get_hit(world, e);
    }
    if world.get::<Meteor>(e).is_some_and(|m| m.dest == pos) {
        world.despawn(e);
    }
}

/// `/obj/effect/meteor/Bump`.
fn bump(world: &mut World, e: Entity, blocker: Entity, pos: TilePos) {
    let kind = world.get::<Meteor>(e).expect("meteor").kind;
    match kind.special {
        // `EX_ACT(A, hitpwr)` on what it hit only.
        Special::Meaty => {
            if turf_at(world, pos) == Some(blocker) {
                ex_act_turf(world, pos, kind.hitpwr);
            } else {
                ex_act_movable(world, blocker, kind.hitpwr);
            }
        }
        _ => ram_turf(world, e, pos, kind),
    }
    get_hit(world, e);
    if world.get_entity(e).is_err() {
        return;
    }
    let Some(&here) = world.get::<TilePos>(e) else {
        return;
    };
    match kind.special {
        Special::Bluespace if world.resource_mut::<EventRng>().0.prob(35.0) => {
            // `do_teleport(src, get_turf(src), 6)`.
            let rng = &mut world.resource_mut::<EventRng>().0;
            let to = TilePos::new(
                here.x + rng.range(-6, 6) as i32,
                here.y + rng.range(-6, 6) as i32,
                here.z,
            );
            if world.resource::<SpatialIndex>().in_bounds(to) {
                world.entity_mut(e).insert(to);
            }
        }
        Special::Tunguska if world.resource_mut::<EventRng>().0.prob(20.0) => {
            explosion(world, here, 2, 4, 6, 0);
        }
        _ => {}
    }
}

/// `ram_turf`: `ex_act` of the tile with the meteor's `hitpwr`.
fn ram_turf(world: &mut World, e: Entity, pos: TilePos, kind: &MeteorKind) {
    match kind.special {
        // shortcut: no blood decals from meaty ores.
        Special::Meaty => {}
        // `slip(100)`: 10 s down; then a hit on every tile it passes.
        Special::Banana => {
            let mobs: Vec<Entity> = world
                .resource::<SpatialIndex>()
                .on_tile(pos)
                .iter()
                .copied()
                .filter(|&m| world.get::<crate::health::Health>(m).is_some())
                .collect();
            for mob in mobs {
                crate::status_effects::knockdown(world, mob, 10.0, 0.0);
            }
            get_hit(world, e);
        }
        _ => ex_act_tile(world, pos, kind.hitpwr),
    }
}

/// `get_hit()`: out of hits, it drops its debris and goes off.
fn get_hit(world: &mut World, e: Entity) {
    let Some(mut meteor) = world.get_mut::<Meteor>(e) else {
        return;
    };
    meteor.hits -= 1;
    if meteor.hits > 0 {
        return;
    }
    let kind = meteor.kind;
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    world.despawn(e);
    make_debris(world, kind, pos);
    meteor_effect(world, kind, pos);
}

/// `make_debris()`: `dropamt` picks from `meteordrop`.
// shortcut: drops without a prototype (plasma, uranium, bananium and bluespace ores; meat and
// organs) are skipped.
fn make_debris(world: &mut World, kind: &MeteorKind, pos: TilePos) {
    for _ in 0..kind.dropamt {
        let Some(&path) = world.resource_mut::<EventRng>().pick(kind.drops) else {
            return;
        };
        if is_under(path, "/obj/item/stack") {
            spawn_stack(world, path, pos, 1);
        } else {
            spawn_obj(world, path, pos);
        }
    }
}

/// `meteor_effect()`.
fn meteor_effect(world: &mut World, kind: &MeteorKind, pos: TilePos) {
    if kind.special == Special::Cluster {
        let size = world.resource::<SpatialIndex>().level_size(pos.z);
        for _ in 0..CLUSTER_COUNT {
            let rng = &mut world.resource_mut::<EventRng>().0;
            let side = rng.range(0, 3) as usize;
            let Some(dest) = size.and_then(|s| debris_start(side, pos.z, s, rng)) else {
                continue;
            };
            spawn_meteor_at(world, &CLUSTER_FRAGMENT, pos, dest);
        }
    }
    if let Some([dev, heavy, light, flame]) = kind.blast {
        explosion(world, pos, dev, heavy, light, flame);
    }
    if kind.special == Special::Emp {
        world.write_message(crate::emp::Empulse {
            epicenter: pos,
            heavy: 3,
            light: 8,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debris_starts_and_finishes_13_tiles_in_on_opposite_sides() {
        let mut rng = ss13_mapload::spawner::Rng::new(5);
        let size = (255, 255);
        for _ in 0..200 {
            let n = debris_start(0, 1, size, &mut rng).unwrap();
            let n_end = debris_finish(0, 1, size, &mut rng).unwrap();
            assert_eq!((n.y, n_end.y), (242, 13));
            assert!((13..=242).contains(&n.x) && (13..=242).contains(&n_end.x));
            let w = debris_start(3, 1, size, &mut rng).unwrap();
            let w_end = debris_finish(3, 1, size, &mut rng).unwrap();
            assert_eq!((w.x, w_end.x), (13, 242));
        }
        assert_eq!(
            debris_start(0, 1, (10, 10), &mut rng),
            None,
            "level too small"
        );
    }

    #[test]
    fn kinds_and_waves_follow_tg() {
        assert_eq!(kind_by_path("/obj/effect/meteor/big").unwrap().hits, 6);
        assert_eq!(MEATY_XENO.hits, 2);
        assert_eq!(MEATY_XENO.special, Special::Meaty);
        let total = |w: Wave| w.iter().map(|(_, n)| n).sum::<i64>();
        assert_eq!(total(METEORS_NORMAL), 21);
        assert_eq!(total(METEORS_CATASTROPHIC), 127);
        assert_eq!(total(METEORS_STRAY), 229);
    }
}
