//! Meteors (`code/modules/meteors/`) and their events (`code/modules/events/meteors/`,
//! `code/modules/events/dust.dm`): meteor waves, Major Space Dust, Space Dust: Minor and the
//! Stray Meteor.
//!
//! A meteor ([`Meteor`]) spawns on a space tile 13 tiles in from a random edge of a station
//! level and flies a straight line to a random point by the opposite edge, one tile every
//! 0.1 s (tg `move_towards`, `delay = 1`). Entering a tile rams it (`ram_turf`: `ex_act` with
//! the meteor's `hitpwr`) and, off space, takes a hit 10% of the time; a dense thing in the way
//! is bumped: rammed, and a hit taken. Out of hits it drops its ore and goes off (its
//! `meteor_effect`: an [`explosion`](crate::explosions::explosion) for the bigger ones).
//!
//! A meteor can be punched or batted away by an athlete or broken up with a mining tool
//! (`meteor_combat`: [`punch`], [`attacked`]); the meteor mode ([`MeteorMode`]) throws meteors
//! every second until stopped.
//!
//! Cost: a wave keeps up to ~100 meteors in flight, 10 steps per second each, every step one
//! tile lookup; far below a millisecond per tick.

use crate::construction::is_space;
use bevy::prelude::*;
use ss13_mapload::{DmName, IconIndex};
use ss13_net::server::Networked;
use ss13_shared::appearance::{Layer, MouseOpacity};
use ss13_shared::{Appearance, IconStateId, TilePos};
use ss13_world::{Density, SpatialIndex};

use super::controller::{EventRng, EventState, RoundEvent, pick_weight, station_levels};
use crate::construction::{path_of, spawn_obj, spawn_stack, turf_at};
use crate::explosions::ex_act::{ex_act_movable, ex_act_tile, ex_act_turf};
use crate::explosions::{Explosion, Severity, explosion};
use crate::obj_damage::is_under;
use crate::security_level::SecLevel;

pub const WAVE_NORMAL: &str = "Meteor Wave: Normal";
pub const WAVE_THREATENING: &str = "Meteor Wave: Threatening";
pub const WAVE_CATASTROPHIC: &str = "Meteor Wave: Catastrophic";
pub const WAVE_MEATY: &str = "Meteor Wave: Meaty";
pub const DUST_STORM: &str = "Major Space Dust";
pub const SPACE_DUST: &str = "Space Dust: Minor";
pub const STRAY_METEOR: &str = "Stray Meteor";
pub const SANDSTORM: &str = "Sandstorm: Directional";
pub const SANDSTORM_CLASSIC: &str = "Sandstorm: Classic";
pub const DARK_MATTEOR: &str = "Dark Matt-eor";

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
/// `EVENT_PROTECTED` (`area_flags`): arrivals, spared by space sand.
const EVENT_PROTECTED: u32 = 1 << 9;
/// `/obj/singularity/dark_matter`: what a spent dark matt-eor leaves.
pub const DARK_MATTER_SINGULARITY: &str = "/obj/singularity/dark_matter";

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
    /// `meteor/sand/ram_turf`: spares hard walls (`hardness <= 25`) and `EVENT_PROTECTED` areas.
    Sand,
    /// Sets red alert while it flies; missing the station restores the level.
    DarkMatteor,
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
    /// `explosion(devastation, heavy, light, flame, flash)` of `meteor_effect` (a `null`
    /// flame range is the light range).
    blast: Option<[i32; 5]>,
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
    blast: Some([0, 1, 2, 2, 3]),
    ..kind("/obj/effect/meteor/medium", "meteor", "small")
};
static BIG: MeteorKind = MeteorKind {
    hits: 6,
    dropamt: 4,
    blast: Some([1, 2, 3, 3, 4]),
    ..kind("/obj/effect/meteor/big", "big meteor", "large")
};
static FLAMING: MeteorKind = MeteorKind {
    hits: 5,
    drops: &["/obj/item/stack/ore/plasma"],
    blast: Some([1, 2, 3, 5, 4]),
    signature: "тепла",
    ..kind("/obj/effect/meteor/flaming", "flaming meteor", "flaming")
};
static IRRADIATED: MeteorKind = MeteorKind {
    hits: 9,
    drops: &["/obj/item/stack/ore/uranium"],
    blast: Some([0, 1, 3, 3, 6]),
    signature: "радиации",
    ..kind("/obj/effect/meteor/irradiated", "glowing meteor", "glowing")
};
static CLUSTER: MeteorKind = MeteorKind {
    hits: 9,
    blast: Some([0, 2, 3, 3, 4]),
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
/// `/obj/effect/meteor/meaty/changeling`: carries a space changeling ([`MeteorCargo`]); not in
/// any wave.
pub static CHANGELING: MeteorKind = MeteorKind {
    hits: 1,
    hitpwr: Severity::Light,
    signature: "ксенобиологических форм жизни",
    ..MeteorKind {
        path: "/obj/effect/meteor/meaty/changeling",
        name: "unsettlingly meaty meteor",
        icon_state: "changeling",
        ..MEATY
    }
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
    blast: Some([5, 10, 15, 15, 20]),
    signature: "армаггедона",
    special: Special::Tunguska,
    ..kind("/obj/effect/meteor/tunguska", "tunguska meteor", "flaming")
};

static SAND: MeteorKind = MeteorKind {
    hits: 2,
    hitpwr: Severity::Light,
    dropamt: 0,
    drops: &[],
    special: Special::Sand,
    ..kind("/obj/effect/meteor/sand", "space sand", "dust")
};
// shortcut: no warp ring or quantum sparks around the dark matt-eor (visual only).
static DARK_MATTEOR_KIND: MeteorKind = MeteorKind {
    hits: 15,
    hitpwr: Severity::Devastate,
    dropamt: 1,
    drops: &[DARK_MATTER_SINGULARITY],
    signature: "темной материи",
    special: Special::DarkMatteor,
    ..kind(
        "/obj/effect/meteor/dark_matteor",
        "dark matt-eor",
        "dark_matter",
    )
};

/// Every ported meteor type, for lookups by path.
static KINDS: [&MeteorKind; 16] = [
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
    &SAND,
    &DARK_MATTEOR_KIND,
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
pub static METEORS_SANDSTORM: Wave = &[(&SAND, 45), (&DUST, 5)];
static DARK_MATTEORS: Wave = &[(&DARK_MATTEOR_KIND, 1)];
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
    towards: Towards,
    next_move: f64,
    expires: f64,
}

/// `move_towards`: steps per move along each axis (the longer axis is 1), the fractional
/// progress along the shorter one and the direction (also flies the immovable rod).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Towards {
    rate: (f64, f64),
    sign: (i32, i32),
    ticker: (f64, f64),
}

impl Towards {
    pub(super) fn new(start: TilePos, goal: TilePos) -> Self {
        let (dx, dy) = (goal.x - start.x, goal.y - start.y);
        let (ax, ay) = (f64::from(dx.abs()), f64::from(dy.abs()));
        let rate = if ax == 0.0 && ay == 0.0 {
            (0.0, 0.0)
        } else if ax > ay {
            (1.0, ay / ax)
        } else {
            (ax / ay, 1.0)
        };
        Self {
            rate,
            sign: (dx.signum(), dy.signum()),
            ticker: (0.0, 0.0),
        }
    }

    /// The next tile from `pos`; [`Self::advance`] commits the step.
    pub(super) fn next(&self, pos: TilePos) -> TilePos {
        let (sx, sy) = (
            (self.ticker.0 + self.rate.0).floor() as i32,
            (self.ticker.1 + self.rate.1).floor() as i32,
        );
        TilePos::new(pos.x + sx * self.sign.0, pos.y + sy * self.sign.1, pos.z)
    }

    pub(super) fn advance(&mut self) {
        self.ticker = (
            (self.ticker.0 + self.rate.0).fract(),
            (self.ticker.1 + self.rate.1).fract(),
        );
    }
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

/// `/datum/round_event/sandstorm`: the side picked on the announcement, then 15 sand meteors
/// from it every tick.
struct Sandstorm {
    /// `GLOB.cardinals` index (`None` until announced: then from random sides, as tg's null
    /// `direction`).
    start_side: Option<usize>,
}

/// `Sandstorm: Directional` with its side preset (the admin's `listed_options` choice).
pub fn make_sandstorm_from(side: Option<usize>) -> Box<dyn RoundEvent> {
    Box::new(Sandstorm { start_side: side })
}

pub fn make_sandstorm(_: &mut World) -> Box<dyn RoundEvent> {
    make_sandstorm_from(None)
}

/// The side's name in the announcement (north, south, east, west).
pub fn sandstorm_side_text(side: usize) -> &'static str {
    ["носа", "кормы", "правого борта", "левого борта"][side.min(3)]
}

impl RoundEvent for Sandstorm {
    fn setup(&mut self, world: &mut World, ev: &mut EventState) {
        let rng = &mut world.resource_mut::<EventRng>().0;
        ev.start_when = rng.range(70, 90) as u32;
        ev.end_when = rng.range(110, 140) as u32;
        ev.announce_when = 1;
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, _: bool) {
        let side = *self
            .start_side
            .get_or_insert_with(|| world.resource_mut::<EventRng>().0.range(0, 3) as usize);
        let text = format!(
            "Большая волна космической пыли приближается с {} станции. Удар ожидается в \
             ближайшие две минуты. Всем сотрудникам рекомендуется по возможности оказать помощь \
             в ремонте и уменьшении ущерба.",
            sandstorm_side_text(side)
        );
        super::priority_announce(world, "Экстренное оповещение о столкновении", &text);
    }

    fn tick(&mut self, world: &mut World, _: &mut EventState) {
        spawn_meteors_from(world, 15, METEORS_SANDSTORM, self.start_side);
    }
}

/// `/datum/round_event/sandstorm_classic`: 10 dust meteors from random sides every tick for
/// ~5 minutes, no announcement.
struct SandstormClassic;

pub fn make_sandstorm_classic(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(SandstormClassic)
}

impl RoundEvent for SandstormClassic {
    fn setup(&mut self, _: &mut World, ev: &mut EventState) {
        ev.start_when = 1;
        ev.end_when = 150;
        ev.announce_when = 0;
        // tg announces nothing (no `announce()` override).
    }

    fn tick(&mut self, world: &mut World, _: &mut EventState) {
        spawn_meteors(world, 10, METEORS_DUST);
    }
}

/// `/datum/round_event/dark_matteor`: one dark matt-eor 10 tiles further in, at the first
/// living mob on a station level standing on ground (none: by the opposite edge).
struct DarkMatteorEvent;

pub fn make_dark_matteor(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(DarkMatteorEvent)
}

/// The dark matt-eor's target: a living mob on a station level, not over a groundless turf.
pub fn dark_matteor_target(world: &mut World) -> Option<TilePos> {
    let levels = station_levels(world);
    let mut q = world.query_filtered::<&TilePos, With<crate::health::Health>>();
    let mobs: Vec<TilePos> = q
        .iter(world)
        .copied()
        .filter(|p| levels.contains(&p.z))
        .collect();
    mobs.into_iter().find(|&pos| {
        !turf_at(world, pos)
            .and_then(|t| world.get::<ss13_mapload::DmTurf>(t))
            .is_some_and(|t| t.groundless)
    })
}

impl RoundEvent for DarkMatteorEvent {
    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let target = dark_matteor_target(world);
        spawn_meteor_from(world, DARK_MATTEORS, None, target, 10);
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, _: bool) {
        super::priority_announce_sound(
            world,
            "Метеоры",
            &format!(
                "Тревога. Помехи в работе метеоритных спутников привлекли темный маттериор. \
                 Объект стремительно приближается к {}. Приготовьтесь к столкновению",
                super::station_name(world)
            ),
            AIRRAID,
        );
    }
}

/// The Dark Matt-eor announcement's sound.
const AIRRAID: &str = "sound/announcer/alarm/airraid.ogg";

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
    spawn_meteor_from(world, wave, None, None, 0)
}

/// tg `spawn_meteors(number, meteor_types, direction)`: every meteor from the side `side`.
pub fn spawn_meteors_from(world: &mut World, number: usize, wave: Wave, side: Option<usize>) {
    for _ in 0..number {
        spawn_meteor_from(world, wave, side, None, 0);
    }
}

/// tg `spawn_meteor(meteor_types, direction, target, distance_from_edge)`: the start side is
/// `side` (`GLOB.cardinals` index) or random, the goal `target` or by the opposite edge.
pub fn spawn_meteor_from(
    world: &mut World,
    wave: Wave,
    side: Option<usize>,
    target: Option<TilePos>,
    distance_from_edge: i32,
) -> Option<Entity> {
    let levels: Vec<u16> = station_levels(world).collect();
    for _ in 0..10 {
        let mut rng = world.resource_mut::<EventRng>();
        let side = side.unwrap_or_else(|| rng.0.range(0, 3) as usize);
        let &z = rng.pick(&levels)?;
        let size = world.resource::<SpatialIndex>().level_size(z)?;
        let rng = &mut world.resource_mut::<EventRng>().0;
        let start = debris_start_from(side, z, size, distance_from_edge, rng);
        let goal = match target {
            Some(target) => Some(target),
            None => debris_finish(side, z, size, rng),
        };
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
pub(super) fn debris_start(
    side: usize,
    z: u16,
    size: (u32, u32),
    rng: &mut ss13_mapload::spawner::Rng,
) -> Option<TilePos> {
    debris_start_from(side, z, size, 0, rng)
}

/// tg `spaceDebrisStartLoc(start_side, Z, distance_from_edge)`.
pub(super) fn debris_start_from(
    side: usize,
    z: u16,
    (w, h): (u32, u32),
    distance: i32,
    rng: &mut ss13_mapload::spawner::Rng,
) -> Option<TilePos> {
    let (w, h) = (w as i32, h as i32);
    let edge = EDGE + distance;
    let (x, y) = match side {
        0 => (
            rng.range(i64::from(edge), i64::from(w - edge)) as i32,
            h - edge,
        ),
        1 => (rng.range(i64::from(edge), i64::from(w - edge)) as i32, edge),
        2 => (
            w - edge,
            rng.range(i64::from(edge), i64::from(h - edge)) as i32,
        ),
        _ => (edge, rng.range(i64::from(edge), i64::from(h - edge)) as i32),
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
                glide: 0,
            },
            Density,
            Networked,
            Meteor {
                kind,
                hits: kind.hits,
                dest: goal,
                towards: Towards::new(start, goal),
                next_move: now + MOVE_SECS,
                expires: now + LIFETIME_SECS,
            },
        ))
        .id();
    if kind.special == Special::DarkMatteor {
        // `dark_matteor/Initialize`: red alert, remembering a lower level.
        let level = crate::security_level::current(world);
        let previous = (level < SecLevel::Red).then_some(level);
        if previous.is_some() {
            crate::security_level::set_level(world, SecLevel::Red, true);
        }
        world.entity_mut(entity).insert(DarkMatteor { previous });
    }
    Some(entity)
}

/// The mobs inside a changeling meteor (tg its `contents`) and its `landing_target`. They ride
/// hidden ([`crate::closets::InCloset`]) on its tile; when it breaks they are dropped there,
/// when it is gone otherwise ([`release_cargo`]) they are thrown at the landing target.
#[derive(Component, Debug, Default)]
pub struct MeteorCargo {
    pub mobs: Vec<Entity>,
    pub landing: Option<TilePos>,
    /// `meteor_effect` ran: a plain drop, no throw.
    landed: bool,
}

const SPLAT: &str = "sound/effects/splat.ogg";

/// `meteor_effect` (broken) or `yeet_changeling` (missed, expired, mined away): the cargo leaves
/// the despawning meteor.
pub(super) fn release_cargo(
    remove: On<Remove<MeteorCargo>>,
    cargo: Query<(&MeteorCargo, Option<&TilePos>)>,
    mut commands: Commands,
) {
    let Ok((cargo, pos)) = cargo.get(remove.entity) else {
        return;
    };
    let (mobs, landing, landed, pos) = (
        cargo.mobs.clone(),
        cargo.landing,
        cargo.landed,
        pos.copied(),
    );
    commands.queue(move |world: &mut World| {
        let Some(pos) = pos else {
            return;
        };
        for mob in mobs {
            if world.get_entity(mob).is_err() {
                continue;
            }
            world.entity_mut(mob).insert(pos);
            crate::closets::let_out(world, mob);
            if landed {
                continue;
            }
            // `yeet_changeling`: the observer start is the last resort.
            let Some(target) = landing.or_else(|| {
                world
                    .get_resource::<crate::server::SpawnPoints>()
                    .and_then(|s| s.observer)
            }) else {
                continue;
            };
            if crate::throw::throw_at(world, mob, target, 2, 2.0) {
                world
                    .entity_mut(mob)
                    .insert(crate::throw::ThrownMob::default());
            }
            let who = crate::security::name_of(world, mob);
            crate::chem::visible(
                world,
                mob,
                "Почувствовав, что что-то не так, мы с силой выбрасываем себя из метеора!",
                &format!(
                    "{} вылетает из unsettlingly meaty meteor!",
                    ss13_shared::capitalize(&who)
                ),
                ss13_shared::chat::Span::Warning,
            );
            let mut sound = crate::sound::PlaySound::new(pos, SPLAT, 50.0, true);
            sound.pressure_affected = false;
            crate::sound::playsound(world, sound);
        }
    });
}

/// `new /obj/effect/meteor/meaty/changeling(start, target)` with `mob` moved inside.
pub fn launch_with(
    world: &mut World,
    mob: Entity,
    start: TilePos,
    landing: TilePos,
) -> Option<Entity> {
    let meteor = spawn_meteor_at(world, &CHANGELING, start, landing)?;
    world.entity_mut(mob).insert(start);
    crate::closets::put_mob_inside(world, mob, meteor);
    world.entity_mut(meteor).insert(MeteorCargo {
        mobs: vec![mob],
        landing: Some(landing),
        landed: false,
    });
    Some(meteor)
}

/// `generate_changeling_meteor`'s start: a carp spawn on a planetary map, else
/// `spaceDebrisStartLoc` of a random side of a random station level.
pub fn changeling_start(world: &mut World) -> Option<TilePos> {
    if super::earthquake::is_planetary(world) {
        let carp = world
            .get_resource::<crate::server::SpawnPoints>()
            .map(|s| s.carp.clone())
            .unwrap_or_default();
        return world.resource_mut::<EventRng>().pick(&carp).copied();
    }
    let levels: Vec<u16> = station_levels(world).collect();
    let &z = world.resource_mut::<EventRng>().pick(&levels)?;
    let size = world.resource::<SpatialIndex>().level_size(z)?;
    let rng = &mut world.resource_mut::<EventRng>().0;
    let side = rng.range(0, 3) as usize;
    debris_start(side, z, size, rng)
}

/// `changeling/shield_defense`: a meteor shield shooting it down throws the cargo at itself.
pub fn retarget_cargo(world: &mut World, meteor: Entity, at: TilePos) {
    if let Some(mut cargo) = world.get_mut::<MeteorCargo>(meteor) {
        cargo.landing = Some(at);
    }
}

/// The cargo moves with its meteor.
fn carry_cargo(world: &mut World, meteor: Entity, pos: TilePos) {
    let Some(mobs) = world.get::<MeteorCargo>(meteor).map(|c| c.mobs.clone()) else {
        return;
    };
    for mob in mobs {
        if world
            .get::<crate::closets::InCloset>(mob)
            .is_some_and(|c| c.closet == meteor)
        {
            world.entity_mut(mob).insert(pos);
        }
    }
}

/// A flying dark matt-eor: the security level it found below red.
#[derive(Component, Debug)]
pub struct DarkMatteor {
    pub previous: Option<SecLevel>,
}

/// `moved_off_z()`: the meteor is gone without going off. A dark matt-eor missed: the level it
/// raised goes back (not from delta) and the station hears of its luck.
fn moved_off_z(world: &mut World, e: Entity) {
    let missed = world.get::<DarkMatteor>(e).map(|d| d.previous);
    world.despawn(e);
    let Some(previous) = missed else {
        return;
    };
    if let Some(previous) = previous
        && crate::security_level::current(world) != SecLevel::Delta
    {
        crate::security_level::set_level(world, previous, true);
    }
    super::priority_announce(
        world,
        "Отслеживание метеора",
        "Ого. Темный Маттеор действительно промахнулся мимо вашей станции. Не забудьте \
         поблагодарить своего священника за его явное божественное вмешательство.",
    );
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
    let next = meteor.towards.next(pos);
    if next == pos {
        // At the goal already (`home` reached): the loop has nothing left to do.
        moved_off_z(world, e);
        return;
    }
    if !world.resource::<SpatialIndex>().in_bounds(next) {
        moved_off_z(world, e);
        return;
    }
    if kind.special != Special::Banana
        && let Some(blocker) = blocker(world, e, next, kind)
    {
        bump(world, e, blocker, next);
        return;
    }
    world
        .get_mut::<Meteor>(e)
        .expect("meteor")
        .towards
        .advance();
    world.entity_mut(e).insert(next);
    carry_cargo(world, e, next);
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
        moved_off_z(world, e);
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
            explosion(
                world,
                here,
                Explosion::new(2, 4, 6).flash(8).cause("tunguska"),
            );
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
        Special::Sand if sand_spares(world, pos) => {}
        _ => ex_act_tile(world, pos, kind.hitpwr),
    }
}

/// `meteor/sand/ram_turf`: too weak for walls with `hardness <= 25`; never in an
/// `EVENT_PROTECTED` area (arrivals).
pub fn sand_spares(world: &World, pos: TilePos) -> bool {
    let hard_wall = turf_at(world, pos)
        .and_then(|t| path_of(world, t))
        .and_then(|p| wall_hardness(&p))
        .is_some_and(|h| h <= 25);
    hard_wall || crate::teleport::area_flags(world, pos).1 & EVENT_PROTECTED != 0
}

/// `/turf/closed/wall` `hardness` (`code/game/turfs/closed/wall/`), `None` off walls.
pub fn wall_hardness(path: &str) -> Option<u32> {
    const HARDNESS: &[(&str, u32)] = &[
        ("/turf/closed/wall/r_wall/plastitanium", 25),
        ("/turf/closed/wall/r_wall", 10),
        ("/turf/closed/wall/concrete/reinforced", 15),
        ("/turf/closed/wall/concrete", 30),
        ("/turf/closed/wall/mineral/diamond", 5),
        ("/turf/closed/wall/mineral/abductor", 10),
        ("/turf/closed/wall/mineral/plastitanium", 25),
        ("/turf/closed/wall/mineral/gold", 65),
        ("/turf/closed/wall/mineral/silver", 65),
        ("/turf/closed/wall/mineral/bananium", 70),
        ("/turf/closed/wall/mineral/plasma", 70),
        ("/turf/closed/wall/mineral/sandstone", 50),
        ("/turf/closed/wall/mineral/wood/nonmetal", 67),
        ("/turf/closed/wall/mineral/wood", 80),
        ("/turf/closed/wall/mineral/bamboo", 80),
        ("/turf/closed/wall/mineral/snow", 80),
        ("/turf/closed/wall/mineral/iron", 60),
    ];
    if !is_under(path, "/turf/closed/wall") {
        return None;
    }
    Some(
        HARDNESS
            .iter()
            .find(|(root, _)| is_under(path, root))
            .map_or(40, |&(_, h)| h),
    )
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
    // The changeling meteor's `meteor_effect`: its contents drop on its tile.
    if let Some(mut cargo) = world.get_mut::<MeteorCargo>(e) {
        cargo.landed = true;
    }
    world.despawn(e);
    make_debris(world, kind, pos);
    meteor_effect(world, kind, pos);
}

/// `make_debris()`: `dropamt` picks from `meteordrop` (every drop is in
/// `ss13_mapload::runtime::EVENT_RUNTIME_TYPES`, so the real assets have its prototype).
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
    if let Some([dev, heavy, light, flame, flash]) = kind.blast {
        let ex = Explosion::new(dev, heavy, light)
            .flame(flame)
            .flash(flash)
            .cause("meteor");
        explosion(world, pos, ex);
    }
    if kind.special == Special::Emp {
        world.write_message(crate::emp::Empulse {
            epicenter: pos,
            heavy: 3,
            light: 8,
        });
    }
}

pub fn is_dark_matteor(world: &World, e: Entity) -> bool {
    world
        .get::<Meteor>(e)
        .is_some_and(|m| m.kind.special == Special::DarkMatteor)
}

// --- Meteor combat ------------------------------------------------------------------------

/// `TOOL_MINING`.
const TOOL_MINING: &str = "mining";
const BASEBALL_BAT: &str = "/obj/item/melee/baseball_bat";
/// `SKILL_LEVEL_EXPERT`.
const SKILL_EXPERT: u8 = 5;
const PICKAXE_SOUND: &str = "sound/effects/pickaxe/picaxe1.ogg";
const BAT_SOUND: &str = "sound/items/baseballhit.ogg";
/// `SFX_PUNCH`.
const PUNCH_SOUND: &str = "punch";

fn meteor_sound(world: &mut World, meteor: Entity, sound: &str, vol: f32) {
    if let Some(&pos) = world.get::<TilePos>(meteor) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, sound, vol, true));
    }
}

/// `/datum/component/meteor_combat/on_punched`: a legendary athlete in combat mode punches the
/// meteor away. False lets the click go on.
pub fn punch(world: &mut World, mob: Entity, meteor: Entity) -> bool {
    if world.get::<Meteor>(meteor).is_none() {
        return false;
    }
    let combat = world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0);
    let level = crate::skills::level(world, mob, crate::skills::Skill::Athletics);
    if !combat || level < crate::skills::LEVEL_LEGENDARY {
        return false;
    }
    // shortcut: no meteor punch achievement (no achievements).
    meteor_sound(world, meteor, PUNCH_SOUND, 50.0);
    redirect(world, meteor, mob);
    true
}

/// `/datum/component/meteor_combat/on_attacked`: a mining tool breaks the meteor into its
/// debris; an expert athlete's baseball bat sends it away. False lets the click go on.
// shortcut: no power fist (not ported).
pub fn attacked(
    world: &mut World,
    client: Entity,
    mob: Entity,
    item: Entity,
    meteor: Entity,
) -> bool {
    let Some(kind) = world.get::<Meteor>(meteor).map(|m| m.kind) else {
        return false;
    };
    if world
        .get::<ss13_mapload::DmTool>(item)
        .is_some_and(|t| t.behaviour == TOOL_MINING)
    {
        let pos = world.get::<TilePos>(meteor).copied();
        meteor_sound(world, meteor, PICKAXE_SOUND, 50.0);
        world.despawn(meteor);
        if let Some(pos) = pos {
            make_debris(world, kind, pos);
        }
        return true;
    }
    if path_of(world, item).is_some_and(|p| is_under(&p, BASEBALL_BAT)) {
        if crate::skills::level(world, mob, crate::skills::Skill::Athletics) < SKILL_EXPERT {
            crate::tools::tell(
                world,
                client,
                ss13_shared::chat::Span::Warning,
                format!("The {} is too heavy for you!", kind.name),
            );
            return true;
        }
        meteor_sound(world, meteor, BAT_SOUND, 100.0);
        redirect(world, meteor, mob);
        return true;
    }
    false
}

/// tg `get_cardinal_dir(start, end)` as a `GLOB.cardinals` index (north, south, east, west):
/// the vertical axis with the chance of its share of the distance. `None` on the same tile.
pub fn cardinal_side(from: TilePos, to: TilePos, roll: f64) -> Option<usize> {
    let (dx, dy) = (to.x - from.x, to.y - from.y);
    if dx == 0 && dy == 0 {
        return None;
    }
    let vertical = roll * f64::from(dx.abs() + dy.abs()) < f64::from(dy.abs());
    Some(match (vertical, dx, dy) {
        (true, _, dy) if dy > 0 => 0,
        (true, _, dy) if dy < 0 => 1,
        (false, dx, _) if dx > 0 => 2,
        (false, dx, _) if dx < 0 => 3,
        // `get_dir & mask` empty: no edge, the meteor flies on.
        _ => return None,
    })
}

/// `/obj/effect/meteor/redirect(athlete)`: off to the edge away from the athlete, a new flight.
pub fn redirect(world: &mut World, meteor: Entity, athlete: Entity) {
    let (Some(&from), Some(&pos)) = (world.get::<TilePos>(athlete), world.get::<TilePos>(meteor))
    else {
        return;
    };
    let roll = world.resource_mut::<EventRng>().unit();
    let Some(side) = cardinal_side(from, pos, roll) else {
        return;
    };
    let Some(size) = world.resource::<SpatialIndex>().level_size(pos.z) else {
        return;
    };
    let Some(dest) = debris_start(side, pos.z, size, &mut world.resource_mut::<EventRng>().0)
    else {
        return;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut m) = world.get_mut::<Meteor>(meteor) {
        m.dest = dest;
        m.towards = Towards::new(pos, dest);
        m.expires = now + LIFETIME_SECS;
    }
}

// --- Meteor mode --------------------------------------------------------------------------

/// `meteordelay` default (5 minutes), seconds of round time.
pub const METEOR_MODE_DELAY_SECS: f64 = 300.0;
/// `rampupdelta` default, minutes.
pub const METEOR_MODE_RAMPUP_MINUTES: f64 = 5.0;
/// `SSprocessing` wait.
const METEOR_MODE_TICK_SECS: f64 = 1.0;

/// tg `GLOB.meteor_mode` (`/datum/meteor_mode_controller`): meteors every second from
/// `start_at` seconds of round time on, worse and more of them as minutes pass.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct MeteorMode {
    /// `meteordelay`: seconds of round time.
    pub start_at: f64,
    /// `rampupdelta`: minutes per extra meteor.
    pub rampup: f64,
    next_fire: f64,
}

/// `GLOB.meteor_mode = new()` + `start_meteor()`. The admin secret's inputs (30 s minimum
/// ramp) bound `rampup` from below. Replaces a running controller.
pub fn start_meteor_mode(world: &mut World, start_at: f64, rampup_minutes: f64) {
    world.insert_resource(MeteorMode {
        start_at: start_at.max(0.0),
        rampup: rampup_minutes.max(0.5),
        next_fire: 0.0,
    });
}

/// `QDEL_NULL(GLOB.meteor_mode)`.
pub fn stop_meteor_mode(world: &mut World) {
    world.remove_resource::<MeteorMode>();
}

/// One `process()` past the delay: how many meteors and from which list, `meteorminutes`
/// after the delay.
pub fn meteor_mode_wave(
    minutes: f64,
    rampup: f64,
    rng: &mut ss13_mapload::spawner::Rng,
) -> (usize, Wave) {
    let mut wave = METEORS_NORMAL;
    if rng.prob(minutes as f32) {
        wave = METEORS_THREATENING;
    }
    if rng.prob((minutes / 2.0) as f32) {
        wave = METEORS_CATASTROPHIC;
    }
    // DM `round(x)` floors.
    let count = (minutes / rampup).floor().clamp(1.0, 10.0) as usize;
    (count, wave)
}

/// `/datum/meteor_mode_controller/process`.
// Cost: up to 10 meteors a second once ramped up; each flies as any other.
pub(super) fn process_meteor_mode(world: &mut World) {
    let Some(mode) = world.get_resource::<MeteorMode>().cloned() else {
        return;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < mode.next_fire {
        return;
    }
    world.resource_mut::<MeteorMode>().next_fire = now + METEOR_MODE_TICK_SECS;
    let Some(round) = world
        .get_resource::<crate::round::Ticker>()
        .map(|t| t.round_duration(now))
    else {
        return;
    };
    if mode.start_at > round {
        return;
    }
    let minutes = (round - mode.start_at) / 60.0;
    let (count, wave) = meteor_mode_wave(
        minutes,
        mode.rampup,
        &mut world.resource_mut::<EventRng>().0,
    );
    spawn_meteors(world, count, wave);
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

    #[test]
    fn sand_and_dark_matteor_follow_tg() {
        assert_eq!(SAND.hits, 2);
        assert_eq!(SAND.dropamt, 0, "sand drops nothing");
        assert_eq!(METEORS_SANDSTORM.iter().map(|(_, n)| n).sum::<i64>(), 50);
        let dark = kind_by_path("/obj/effect/meteor/dark_matteor").unwrap();
        assert_eq!((dark.hits, dark.hitpwr), (15, Severity::Devastate));
        assert_eq!(dark.drops, &[DARK_MATTER_SINGULARITY]);
        assert_eq!(sandstorm_side_text(0), "носа");
        assert_eq!(sandstorm_side_text(3), "левого борта");
    }

    #[test]
    fn wall_hardness_spares_reinforced_walls_from_sand() {
        assert_eq!(wall_hardness("/turf/closed/wall"), Some(40));
        assert_eq!(wall_hardness("/turf/closed/wall/r_wall"), Some(10));
        assert_eq!(
            wall_hardness("/turf/closed/wall/r_wall/plastitanium"),
            Some(25)
        );
        assert_eq!(
            wall_hardness("/turf/closed/wall/mineral/titanium"),
            Some(40)
        );
        assert_eq!(wall_hardness("/turf/open/floor"), None);
    }

    #[test]
    fn distance_from_edge_moves_the_start_in() {
        let mut rng = ss13_mapload::spawner::Rng::new(3);
        for _ in 0..50 {
            let e = debris_start_from(2, 1, (255, 255), 10, &mut rng).unwrap();
            assert_eq!(e.x, 255 - 23);
            assert!((23..=232).contains(&e.y));
        }
    }

    #[test]
    fn cardinal_side_follows_the_longer_share() {
        let me = TilePos::new(10, 10, 1);
        // Straight up: always north; straight left: always west.
        assert_eq!(cardinal_side(me, TilePos::new(10, 15, 1), 0.99), Some(0));
        assert_eq!(cardinal_side(me, TilePos::new(4, 10, 1), 0.0), Some(3));
        // Diagonal south-east: the roll picks the axis.
        assert_eq!(cardinal_side(me, TilePos::new(13, 7, 1), 0.2), Some(1));
        assert_eq!(cardinal_side(me, TilePos::new(13, 7, 1), 0.7), Some(2));
        assert_eq!(cardinal_side(me, me, 0.5), None);
    }

    #[test]
    fn meteor_mode_ramps_up() {
        let mut rng = ss13_mapload::spawner::Rng::new(9);
        // Right at the start: one normal meteor a second.
        let (n, wave) = meteor_mode_wave(0.0, 5.0, &mut rng);
        assert_eq!(n, 1);
        assert!(std::ptr::eq(wave, METEORS_NORMAL));
        assert_eq!(meteor_mode_wave(12.0, 5.0, &mut rng).0, 2);
        // Capped at 10; past 200 minutes always catastrophic.
        let (n, wave) = meteor_mode_wave(500.0, 5.0, &mut rng);
        assert_eq!(n, 10);
        assert!(std::ptr::eq(wave, METEORS_CATASTROPHIC));
    }

    #[test]
    fn drops_are_runtime_types() {
        let registry = ss13_mapload::runtime::runtime_types();
        let missing: Vec<&str> = KINDS
            .iter()
            .flat_map(|k| k.drops.iter().copied())
            .filter(|p| !registry.contains(p))
            .collect();
        assert!(
            missing.is_empty(),
            "not in ss13_mapload::runtime: {missing:?}"
        );
    }
}
