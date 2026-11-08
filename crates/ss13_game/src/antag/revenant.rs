//! The revenant after tg `/mob/living/basic/revenant` (`_revenant.dm`, `revenant_harvest.dm`,
//! `revenant_abilities.dm`, `revenant_effects.dm`, `revenant_items.dm`), its antagonist
//! (`/datum/antagonist/revenant`, `revenant_objectives.dm`), the abilities' essence costs
//! (`/datum/component/revenant_ability`), the blight (`/datum/disease/revblight`), haunted items
//! (`/datum/component/haunted_item`) and the "Revenant" light midround
//! (`/datum/dynamic_ruleset/midround/from_ghosts/revenant`).
//!
//! - **Selection** ([`can_spawn`], [`spawn_revenants`]): more than 10 corpses on the station
//!   levels; ghosts who want [`ROLE_REVENANT`]; the revenant appears on a corpse's tile (fewer
//!   than 12 corpses: carp spawns too), named from `strings/revenant_names.json`; the greeting,
//!   the objectives (absorb 350-600 essence, a fluff one), `sound/effects/ghost.ogg`.
//! - **Hidden** ([`Incorporeal`]): unseen and unclickable by the living (its own client sees it),
//!   moves through walls (`INCORPOREAL_MOVE_JAUNT`, steps from the movement queue), takes no
//!   damage. Essence is its health and its mana: 75, regenerating 2.5/s up to its maximum
//!   unless inhibited.
//! - **Revealed** (by abilities and harvesting, for their durations): seen, solid to
//!   attacks, damage drains essence; at 0 essence it dies: a fading screech, 3 s later the
//!   "glimmering residue" (`/obj/item/ectoplasm/revenant`) holds it; left alone for a minute the
//!   residue reforms it; scattered by hand (`attack_self`) or otherwise gone, it is gone for good.
//! - **Harvest** (a click on a human next to it in crit or dead, once per soul): three timed
//!   stages (1-2, 1.5-2, 1.5-2.5 s) that weigh the soul (15-20, +20-30 with a player, +40-50
//!   alive), then the drain: revealed and frozen 5 s, 4.6 s; the essence, +5 max for a living
//!   soul, +15 and a perfect soul above 90; the victim dies.
//! - **Abilities** (buttons; locked ones are unlocked with stolen essence, then cost essence,
//!   reveal and freeze it): Defile (lights flicker, windows cracked, closets opened), Overload
//!   Lights (lit fixtures shock humans within 2 tiles 2 s later, 15 burn), Malfunction (EMP on
//!   humans and cyborgs, 20% of objects emagged), Blight (humans catch or worsen the blight,
//!   other living things take 5 toxin), Haunt Objects (up to 7 items nearby, likelier the
//!   closer, throw themselves at the living for 1-3 minutes), Vortex and Scatter (storage
//!   emptied around it, things pulled in or thrown away). Not in a wall or a dense object.
//! - **Blight** ([`revblight_act`]): rest may cure it, fits of confusion and exhaustion, toxin
//!   per stage reached, at stage 5 a last bout and the cure 10 s later; holy water cures.
//! - [`roundend_report`]: each revenant and its objectives.
// shortcut: no ghost poll (willing ghosts are taken), ghosts do not see a hidden revenant, no
// deadchat speech or telepathy (it speaks aloud), no essence HUD (gains and losses go to chat),
// salt, blessed tiles, the null rod's inhibition, orbiting, spirit boards and mirror prisons are
// not ported, projectiles stop at a hidden revenant (without harm); Defile skips floors, rust,
// showers, drinks, food, morgue trays and the medicine labels; Blight skips hair colour, plants
// and non-human carbons' plasma; harvest skips holy resistance and lesser humanoids; the vortex
// throws what it moves (no single steps); haunted items gain no throwforce; the residue's
// reform keeps the dead revenant's player (no new poll when it left).

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemState;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::spawner::Rng;
use ss13_mapload::{DmItem, DmObj, IconIndex};
use ss13_net::server::ClientToken;
use ss13_net::sound::server::SendSound;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::SoundEvent;
use ss13_shared::{Appearance, MouseOpacity, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiSystems};
use ss13_world::SpatialIndex;

use super::dynamic::DynamicRng;
use super::objectives::RoundEnd;
use super::{AntagHudData, AntagHudUi, BrotherInfo, ObjectiveInfo};
use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::click::ClickResolved;
use crate::health::{DamageType, Health, Stat};
use crate::medical::diseases::{Disease, Diseases, Kind, Outcome};
use crate::server::{PlayerMob, SpawnPoints};
use crate::status_effects::Incap;
use crate::tools::{now, tell, tell_mob};

pub use super::ROLE_REVENANT;
/// `name` ("\improper Revenant").
pub const ANTAG_NAME: &str = "Ревенант";
const ROUNDEND_CATEGORY: &str = "Ревенанты";
pub const REVENANT_MOB: &str = "/mob/living/basic/revenant";
pub const RESIDUE: &str = "/obj/item/ectoplasm/revenant";
const REVENANT_JOB: &str = "/datum/job/revenant";
/// `required_station_corpses`; `get_revenant_spawns`' comfortable pool.
pub const REQUIRED_CORPSES: usize = 10;
const COMFORTABLE_SPAWNS: usize = 12;

/// `essence`, `max_essence`, `essence_regen_amount` (per second).
pub const START_ESSENCE: f32 = 75.0;
pub const ESSENCE_REGEN: f32 = 2.5;
/// `death` -> `move_to_ectoplasm` (3 s), the residue's `reform` (1 minute).
const FADE_SECS: f64 = 3.0;
const REFORM_SECS: f64 = 60.0;
/// The harvest's stages: the soul search, the weighing, the reach, the drain (deciseconds
/// ranges, the drain in seconds); the reveal and freeze of the drain.
const SEARCH_DS: (i64, i64) = (10, 20);
const WEIGH_DS: (i64, i64) = (15, 20);
const REACH_DS: (i64, i64) = (15, 25);
const DRAIN_SECS: f64 = 4.6;
const HARVEST_REVEAL: f64 = 5.0;
const SOFTCRIT_STUN: f32 = 4.6;
/// Overload Lights: the shock's delay, range and damage.
const OVERLOAD_DELAY: f64 = 2.0;
const SHOCK_RANGE: i32 = 2;
const SHOCK_DAMAGE: f32 = 15.0;
/// Defile's window damage (`REVENANT_DEFILE_MIN/MAX_DAMAGE`).
const DEFILE_MIN: i64 = 30;
const DEFILE_MAX: i64 = 50;
/// Haunt Objects: `max_targets`, the haunting's length (seconds), how often a haunted item looks
/// for a victim.
const HAUNT_TARGETS: usize = 7;
const HAUNT_MIN: i64 = 60;
const HAUNT_MAX: i64 = 180;
const HAUNT_TICK: f64 = 2.0;
/// `EMP_HEAVY`.
const EMP_HEAVY: u8 = 1;
/// Malfunction's `no_emag_typecache`.
const NO_EMAG: [&str; 3] = [
    "/obj/machinery/power/apc",
    "/obj/machinery/power/smes",
    "/obj/machinery/computer/emergency_shuttle",
];

const MOB_DMI: &str = "icons/mob/simple/mob.dmi";
const ACTIONS_DMI: &str = "icons/mob/actions/actions_revenant.dmi";
const BACKGROUNDS_DMI: &str = "icons/mob/actions/backgrounds.dmi";
const SELF_LOOK: &str = "revenant_self";
const GHOST_SOUND: &str = "sound/effects/ghost.ogg";
const SCREECH: &str = "sound/effects/screech.ogg";
const ZAP: &str = "sound/machines/defib/defib_zap.ogg";
const WOOSH: &str = "sound/machines/woosh.ogg";
const HISS: &str = "sound/machines/hiss.ogg";
const HOLY_WATER: &str = "/datum/reagent/water/holywater";

/// `strings/revenant_names.json` (BandaStation: no " of ").
const SPIRIT_TYPES: [&str; 4] = ["Призрак", "Фантом", "Дух", "Тень"];
const ADVERBS: [&str; 10] = [
    " вечной ",
    " тихой ",
    " тёмной ",
    " скорбной ",
    " яростной ",
    " голодной ",
    " проклятой ",
    " забытой ",
    " безмолвной ",
    " ледяной ",
];
const THEMES: [&str; 10] = [
    "ночи",
    "зимы",
    "мести",
    "ненависти",
    "агонии",
    "тоски",
    "печали",
    "обиды",
    "пустоты",
    "боли",
];
/// `/datum/objective/revenant_fluff` texts.
const FLUFF: [&str; 11] = [
    "Помогайте существующим угрозам и усугубляйте их в критические моменты.",
    "Вызывайте как можно больше хаоса и гнева, избегая смерти.",
    "Повреждайте и приводите в негодность как можно больше оборудования станции.",
    "Выводите из строя и вызывайте сбои в работе как можно большего числа машин.",
    "Убедитесь, что любое святое оружие стало непригодным для использования.",
    "Прислушивайтесь к просьбам мёртвых и выполняйте их, если это не слишком неудобно или не ведёт к самоуничтожению.",
    "Выдавайте себя за Бога или добивайтесь поклонения себе как Божеству.",
    "Доставляйте капитану как можно больше страданий.",
    "Доставляйте клоуну как можно больше страданий.",
    "Доставляйте экипажу как можно больше страданий.",
    "По возможности предотвращайте использование энергетического оружия.",
];

/// The `/aoe/revenant` spells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ability {
    Defile,
    Overload,
    Malfunction,
    Blight,
    Haunt,
    Vortex,
    Scatter,
}

/// A spell's vars: name, button state, `unlock_amount`, `cast_amount`, `cooldown_time`,
/// `reveal_duration`, `stun_duration` (seconds), `aoe_radius`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AbilityDef {
    pub name: &'static str,
    pub state: &'static str,
    pub unlock: f32,
    pub cast: f32,
    pub cooldown: f64,
    pub reveal: f64,
    pub stun: f64,
    pub radius: i32,
}

impl Ability {
    pub const ALL: [Self; 7] = [
        Self::Blight,
        Self::Defile,
        Self::Haunt,
        Self::Malfunction,
        Self::Overload,
        Self::Vortex,
        Self::Scatter,
    ];

    pub fn def(self) -> AbilityDef {
        let base = AbilityDef {
            name: "",
            state: "",
            unlock: 100.0,
            cast: 50.0,
            cooldown: 0.0,
            reveal: 8.0,
            stun: 2.0,
            radius: 0,
        };
        match self {
            Self::Overload => AbilityDef {
                name: "Перегрузка света",
                state: "overload_lights",
                unlock: 25.0,
                cast: 40.0,
                cooldown: 20.0,
                stun: 3.0,
                radius: 5,
                ..base
            },
            Self::Defile => AbilityDef {
                name: "Осквернение",
                state: "defile",
                unlock: 10.0,
                cast: 30.0,
                cooldown: 15.0,
                reveal: 4.0,
                radius: 4,
                ..base
            },
            Self::Malfunction => AbilityDef {
                name: "Сбой",
                state: "malfunction",
                unlock: 125.0,
                cast: 60.0,
                cooldown: 20.0,
                radius: 4,
                ..base
            },
            Self::Blight => AbilityDef {
                name: "Порча",
                state: "blight",
                unlock: 75.0,
                cast: 50.0,
                cooldown: 20.0,
                radius: 3,
                ..base
            },
            Self::Haunt => AbilityDef {
                name: "Одержимые объекты",
                state: "r_haunt",
                unlock: 30.0,
                cast: 50.0,
                reveal: 6.0,
                stun: 3.0,
                radius: 5,
                ..base
            },
            Self::Vortex => AbilityDef {
                name: "Vortex",
                state: "r_vortex",
                unlock: 45.0,
                cast: 30.0,
                cooldown: 15.0,
                reveal: 5.0,
                radius: 1,
                ..base
            },
            Self::Scatter => AbilityDef {
                name: "Scatter",
                state: "r_scatter",
                unlock: 60.0,
                cast: 55.0,
                cooldown: 25.0,
                reveal: 5.0,
                stun: 3.0,
                radius: 1,
            },
        }
    }

    fn id(self) -> String {
        format!("revenant:{self:?}")
    }

    /// `update_spell_name`.
    pub fn button_name(self, locked: bool) -> String {
        let def = self.def();
        if locked {
            format!("{} ({}SE)", def.name, def.unlock)
        } else {
            format!("{} ({}E)", def.name, def.cast)
        }
    }
}

/// A harvest under way: the stage that ends at `at`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Harvest {
    pub target: Entity,
    pub stage: u8,
    pub at: f64,
    pub essence: f32,
    pub user_pos: TilePos,
    pub target_pos: TilePos,
}

/// On the revenant: the mob's state and `/datum/antagonist/revenant`.
#[derive(Component, Clone, Debug)]
pub struct Revenant {
    pub key: String,
    pub essence: f32,
    pub max_essence: f32,
    pub essence_accumulated: f32,
    pub essence_excess: f32,
    pub perfect_souls: u32,
    /// `drained_mobs`.
    pub drained: Vec<Entity>,
    /// `TRAIT_REVENANT_REVEALED` until then.
    pub revealed_until: f64,
    /// `TRAIT_REVENANT_INHIBITED` until then.
    pub inhibited_until: f64,
    /// `TRAIT_NO_TRANSFORM` (the revenant paralysis) until then.
    pub stunned_until: f64,
    /// `dormant`: dead, fading or in its residue.
    pub dormant: bool,
    pub harvest: Option<Harvest>,
    /// Each ability: unlocked, ready at.
    pub abilities: Vec<(Ability, bool, f64)>,
    /// `/datum/objective/revenant` `target_amount`.
    pub target_amount: u32,
    pub fluff: String,
    pub next_step: f64,
    /// The residue holding it and when it reforms; or when it moves into one.
    pub residue: Option<Entity>,
    pub fade_at: Option<f64>,
}

impl Revenant {
    pub fn new(key: String, target_amount: u32, fluff: String) -> Self {
        Self {
            key,
            essence: START_ESSENCE,
            max_essence: START_ESSENCE,
            essence_accumulated: 0.0,
            essence_excess: 0.0,
            perfect_souls: 0,
            drained: Vec::new(),
            revealed_until: 0.0,
            inhibited_until: 0.0,
            stunned_until: 0.0,
            dormant: false,
            harvest: None,
            abilities: Ability::ALL.iter().map(|&a| (a, false, 0.0)).collect(),
            target_amount,
            fluff,
            next_step: 0.0,
            residue: None,
            fade_at: None,
        }
    }

    pub fn revealed(&self, t: f64) -> bool {
        t < self.revealed_until
    }

    pub fn inhibited(&self, t: f64) -> bool {
        t < self.inhibited_until
    }

    pub fn stunned(&self, t: f64) -> bool {
        t < self.stunned_until
    }

    /// `change_essence_amount`: false when it would go below zero.
    pub fn change_essence(&mut self, by: f32) -> bool {
        if self.essence + by < 0.0 {
            return false;
        }
        self.essence = (self.essence + by).max(0.0);
        if by > 0.0 {
            self.essence_accumulated = (self.essence_accumulated + by).max(0.0);
            self.essence_excess = (self.essence_excess + by).max(0.0);
        }
        true
    }

    /// `/datum/objective/revenant/check_completion` (alive is the caller's).
    pub fn objective_done(&self) -> bool {
        self.essence_accumulated >= self.target_amount as f32
    }
}

/// Hidden: moves through walls (`incorporeal_move`), unseen and untouchable.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Incorporeal;

/// The glimmering residue holding a revenant (`revenant_prison`).
#[derive(Component, Clone, Copy, Debug)]
pub struct Residue {
    pub revenant: Entity,
    pub reform_at: f64,
}

/// `/datum/component/haunted_item`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Haunted {
    pub until: f64,
    pub next: f64,
    pub aggro: i32,
}

/// An Overload Lights shock due.
#[derive(Component, Clone, Copy, Debug)]
struct OverloadShock {
    at: f64,
    revenant: Entity,
}

/// The blight's own state on its host (`stagedamage`, `finalstage`'s cure time).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Blighted {
    pub stage_damage: u8,
    pub cure_at: Option<f64>,
}

/// The revenant's info panel (the generic one with its objectives).
pub struct RevenantInfoWindow;

impl UiInterface for RevenantInfoWindow {
    const NAME: &'static str = crate::antag::cult::AntagInfoGenericUi::NAME;
    type Data = ();
    type StaticData = BrotherInfo;
}

impl ServerUi for RevenantInfoWindow {
    type Source = &'static Revenant;
    type UserParam = ();
    type Act = ();

    fn static_data(r: &Revenant) -> BrotherInfo {
        BrotherInfo {
            antag_name: ANTAG_NAME.to_owned(),
            objectives: objectives(r)
                .into_iter()
                .enumerate()
                .map(|(i, (text, done))| ObjectiveInfo {
                    count: i as u32 + 1,
                    explanation: text,
                    complete: done,
                })
                .collect(),
        }
    }

    fn data(_: &Revenant, _: Entity, _: &()) {}
}

fn objectives(r: &Revenant) -> Vec<(String, bool)> {
    vec![
        (
            format!("Поглотите {} очков эссенции с гуманоидов.", r.target_amount),
            r.objective_done(),
        ),
        (r.fluff.clone(), true),
    ]
}

pub struct RevenantPlugin;

impl Plugin for RevenantPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // The residue scattered by hand (`attack_self`).
        app.add_click_handler(ClickPhase::AttackSelf, 3277, |w, c| {
            scatter_residue(w, c.client, c.mob, c.target)
        });
        app.register_ui::<RevenantInfoWindow>()
            .add_systems(
                Update,
                (open_info, trigger_buttons).in_set(UiSystems::Handle),
            )
            .add_systems(
                Update,
                (revenant_clicks, steps)
                    .after(crate::click::ResolveClicks)
                    .before(crate::health::HealthSystems),
            )
            .add_systems(
                Update,
                (life, harvests, shocks, haunts, look).before(crate::health::HealthSystems),
            )
            .add_systems(
                Update,
                (essence_damage, residues_gone, blight_cures).after(crate::health::HealthSystems),
            );
    }
}

fn with_rng<R>(world: &mut World, f: impl FnOnce(&mut Rng) -> R) -> R {
    if !world.contains_resource::<DynamicRng>() {
        world.insert_resource(DynamicRng(Rng::new(0)));
    }
    f(&mut world.resource_mut::<DynamicRng>().0)
}

fn name_of(world: &World, e: Entity) -> String {
    crate::security::name_of(world, e)
}

fn visible(world: &mut World, mob: Entity, span: Span, others: String, own: Option<String>) {
    let Some(pos) = crate::explosions::turf_of(world, mob) else {
        return;
    };
    let personal: Vec<(Entity, Span, String)> = own.map(|t| (mob, span, t)).into_iter().collect();
    for m in
        crate::pull::visible_message(world, pos, ChatChannel::Warning, (span, others), &personal)
    {
        world.write_message(m);
    }
}

fn play(world: &mut World, pos: TilePos, sound: &str, vol: f32) {
    crate::sound::playsound(world, crate::sound::PlaySound::new(pos, sound, vol, true));
}

fn is_human(world: &World, e: Entity) -> bool {
    world.get::<crate::human::HumanLook>(e).is_some()
}

fn dist(a: TilePos, b: TilePos) -> i32 {
    (a.x - b.x).abs().max((a.y - b.y).abs())
}

/// The living (not revenants) on the tiles within `radius` of `center`.
fn living_around(world: &World, center: TilePos, radius: i32) -> Vec<Entity> {
    world
        .resource::<SpatialIndex>()
        .in_radius(center, radius as u32)
        .filter(|&e| {
            world.get::<Stat>(e).is_some()
                && world.get::<Health>(e).is_some()
                && world.get::<Revenant>(e).is_none()
                && world.get::<TilePos>(e).is_some_and(|p| p.z == center.z)
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Selection

/// Corpses on the station levels (`GLOB.dead_mob_list` there).
fn station_corpses(world: &mut World) -> Vec<TilePos> {
    let levels = crate::events::controller::station_levels(world);
    let mut out: Vec<TilePos> = world
        .query::<(&Stat, &TilePos)>()
        .iter(world)
        .filter(|(s, p)| **s == Stat::Dead && levels.contains(&p.z))
        .map(|(_, p)| *p)
        .collect();
    out.sort_by_key(|p| (p.z, p.y, p.x));
    out
}

/// `can_be_selected`: more than [`REQUIRED_CORPSES`] corpses on the station.
pub fn can_spawn(world: &mut World) -> bool {
    station_corpses(world).len() > REQUIRED_CORPSES
}

/// `get_revenant_spawns`: corpse tiles, with carp spawns when few.
fn spawn_tiles(world: &mut World) -> Vec<TilePos> {
    let mut tiles = station_corpses(world);
    if tiles.len() < COMFORTABLE_SPAWNS
        && let Some(s) = world.get_resource::<SpawnPoints>()
    {
        tiles.extend(s.carp.iter().copied());
    }
    tiles
}

/// `create_ruleset_body` for each `(client, ghost)`; `Login` makes it the antagonist.
pub fn spawn_revenants(world: &mut World, ghosts: &[(Entity, Entity)]) -> Vec<Entity> {
    let mut out = Vec::new();
    for &(client, ghost) in ghosts {
        let tiles = spawn_tiles(world);
        if tiles.is_empty() {
            break;
        }
        let i = with_rng(world, |rng| rng.range(0, tiles.len() as i64 - 1)) as usize;
        let Some(mob) = crate::mobs::spawn::spawn_mob(world, REVENANT_MOB, tiles[i]) else {
            break;
        };
        if !crate::medical::brainmob::possess(world, client, ghost, mob) {
            world.despawn(mob);
            continue;
        }
        world.try_despawn(ghost).ok();
        make_revenant(world, mob);
        info!("{mob} ({client}) is a revenant at {:?}", tiles[i]);
        out.push(mob);
    }
    out
}

/// `generate_random_mob_name`.
fn random_name(rng: &mut Rng) -> String {
    let pick =
        |rng: &mut Rng, list: &[&'static str]| list[rng.range(0, list.len() as i64 - 1) as usize];
    format!(
        "{}{}{}",
        pick(rng, &SPIRIT_TYPES),
        pick(rng, &ADVERBS),
        pick(rng, &THEMES)
    )
}

/// `Login` + `add_antag_datum(/datum/antagonist/revenant)`: the name, the job, the objectives,
/// the greeting, the buttons, hidden. `false` if `mob` already is one.
pub fn make_revenant(world: &mut World, mob: Entity) -> bool {
    if world.get::<Revenant>(mob).is_some() || world.get_entity(mob).is_err() {
        return false;
    }
    crate::noncrew_jobs::set_assigned_role(world, mob, REVENANT_JOB);
    let (name, target, fluff) = with_rng(world, |rng| {
        let name = random_name(rng);
        let target = rng.range(350, 600) as u32;
        let fluff = FLUFF[rng.range(0, FLUFF.len() as i64 - 1) as usize].to_owned();
        (name, target, fluff)
    });
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    world.entity_mut(mob).insert((
        Revenant::new(key, target, fluff),
        Incorporeal,
        Name::new(name),
        AntagHudData::default(),
    ));
    refresh_buttons(world, mob);
    if let Some(client) = client {
        for (span, text) in [
            (Span::Bold, "Вы - Ревенант."),
            (
                Span::Bold,
                "Ваш некогда обычный дух был наполнен чужеродной энергией и превращён в ревенанта.",
            ),
            (
                Span::Bold,
                "Вы не мертвы и не живы, а находитесь где-то посередине. Вы способны ограниченно взаимодействовать с обоими мирами.",
            ),
            (
                Span::Bold,
                "Вы неуязвимы и невидимы для всех, кроме других призраков. Большинство способностей раскроют вас, сделав уязвимым.",
            ),
            (
                Span::Bold,
                "Для функционирования вам необходимо поглощать жизненную эссенцию гуманоидов. Эта эссенция является ресурсом, вашим здоровьем и источником силы для всех способностей.",
            ),
            (
                Span::Italics,
                "Вы не помните ничего из своих прошлых жизней, как и не будете помнить ничего об этой после своей смерти.",
            ),
        ] {
            tell(world, client, span, text);
        }
        let lines = world
            .get::<Revenant>(mob)
            .map(objectives)
            .unwrap_or_default();
        for (i, (text, _)) in lines.into_iter().enumerate() {
            tell(
                world,
                client,
                Span::Plain,
                format!("Задача №{}: {text}", i + 1),
            );
        }
        if world.contains_resource::<Messages<SendSound>>() {
            world.write_message(SendSound {
                client,
                sound: SoundEvent {
                    file: GHOST_SOUND.into(),
                    volume: 100.0,
                    speed: 1.0,
                    offset: None,
                },
            });
        }
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
        world.write_message(OpenUi::new::<RevenantInfoWindow>(
            client, mob, mob, ANTAG_NAME,
        ));
    }
    true
}

// ---------------------------------------------------------------------------------------------
// Life, look and movement

/// `on_life` once a second: essence regeneration, the reveal and freeze ending, the death of
/// a revealed revenant without essence, the fade into the residue and the reform.
fn life(world: &mut World, mut last: Local<f64>) {
    let t = now(world);
    if t - *last < 1.0 {
        return;
    }
    let dt = if *last == 0.0 { 1.0 } else { t - *last } as f32;
    *last = t;
    let all: Vec<Entity> = world
        .query_filtered::<Entity, With<Revenant>>()
        .iter(world)
        .collect();
    for mob in all {
        let Some(mut r) = world.get::<Revenant>(mob).cloned() else {
            continue;
        };
        if let Some(at) = r.fade_at
            && t >= at
        {
            move_to_residue(world, mob);
            continue;
        }
        if r.dormant {
            continue;
        }
        if r.revealed(t) && r.essence <= 0.0 {
            death(world, mob);
            continue;
        }
        if !r.inhibited(t) && r.essence < r.max_essence {
            r.essence = (r.essence + ESSENCE_REGEN * dt).min(r.max_essence);
            world.entity_mut(mob).insert(r);
        }
        sync_state(world, mob);
    }
    let residues: Vec<(Entity, Residue)> = world
        .query::<(Entity, &Residue)>()
        .iter(world)
        .map(|(e, r)| (e, *r))
        .collect();
    for (residue, r) in residues {
        if t >= r.reform_at {
            reform(world, residue, r.revenant);
        }
    }
}

/// [`Incorporeal`] while hidden and awake; the buttons' availability.
fn sync_state(world: &mut World, mob: Entity) {
    let t = now(world);
    let Some(r) = world.get::<Revenant>(mob) else {
        return;
    };
    let hidden = !r.revealed(t) && !r.dormant;
    let has = world.get::<Incorporeal>(mob).is_some();
    if hidden && !has {
        world.entity_mut(mob).insert(Incorporeal);
    } else if !hidden && has {
        world.entity_mut(mob).remove::<Incorporeal>();
    }
    refresh_buttons(world, mob);
}

/// `update_icon_state` and the invisibility: hidden it is transparent to the others, its own
/// client sees it (`SEE_SELF`).
fn look(world: &mut World) {
    let t = now(world);
    let all: Vec<(Entity, Revenant)> = world
        .query::<(Entity, &Revenant)>()
        .iter(world)
        .map(|(e, r)| (e, r.clone()))
        .collect();
    for (mob, r) in all {
        // Held by its residue: hidden there (`crate::closets` keeps the look).
        if world.get::<crate::closets::InCloset>(mob).is_some() {
            continue;
        }
        let state = if r.revealed(t) {
            "revenant_revealed"
        } else if r.stunned(t) || r.dormant {
            if r.harvest.is_some() || r.dormant {
                "revenant_draining"
            } else {
                "revenant_stun"
            }
        } else {
            "revenant_idle"
        };
        let Some(icon) = world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(MOB_DMI, state))
        else {
            continue;
        };
        let hidden = !r.revealed(t) && !(r.dormant && r.fade_at.is_some());
        let Some(mut base) = world.get::<Appearance>(mob).cloned() else {
            continue;
        };
        let mut own = base.clone();
        own.icon_state = icon;
        own.color = None;
        own.mouse_opacity = MouseOpacity::Icon;
        base.icon_state = icon;
        if hidden {
            base.color = Some([255, 255, 255, 0]);
            base.mouse_opacity = MouseOpacity::Transparent;
        } else {
            base.color = None;
            base.mouse_opacity = MouseOpacity::Icon;
        }
        if world.get::<Appearance>(mob) != Some(&base) {
            world.entity_mut(mob).insert(base);
        }
        let current = crate::alt_appearance::alt_appearance(world, mob, SELF_LOOK)
            .map(|a| a.appearance.clone());
        if current.as_ref() != Some(&own) {
            crate::alt_appearance::remove_alt_appearance(world, mob, SELF_LOOK);
            crate::alt_appearance::add_alt_appearance(
                world,
                mob,
                SELF_LOOK,
                crate::alt_appearance::Seers::mob(mob),
                own,
            );
        }
    }
}

/// A hidden revenant's steps (handed over as ghost steps): through anything but out of the
/// map, at its move delay, not while frozen.
fn steps(world: &mut World, mut cursor: Local<MessageCursor<crate::ghost::GhostStep>>) {
    let Some(messages) = world.get_resource::<Messages<crate::ghost::GhostStep>>() else {
        return;
    };
    let pending: Vec<crate::ghost::GhostStep> = cursor.read(messages).copied().collect();
    let t = now(world);
    for step in pending {
        let mob = step.ghost;
        if world.get::<Incorporeal>(mob).is_none() {
            continue;
        }
        let Some(r) = world.get::<Revenant>(mob) else {
            continue;
        };
        if r.stunned(t) || r.dormant || t < r.next_step {
            continue;
        }
        let Some(&pos) = world.get::<TilePos>(mob) else {
            continue;
        };
        if let Some(mut facing) = world.get_mut::<ss13_shared::Facing>(mob) {
            facing.set_if_neq(ss13_shared::Facing(step.dir));
        }
        let to = pos.step(step.dir);
        if !world.resource::<SpatialIndex>().in_bounds(to) {
            continue;
        }
        let delay = world
            .get::<ss13_world::Mover>(mob)
            .map_or(0.1, |m| m.delay.max(0.05)) as f64;
        world.entity_mut(mob).insert(to);
        if let Some(mut r) = world.get_mut::<Revenant>(mob) {
            r.next_step = t + delay;
        }
    }
}

/// `on_damage_loss_changed`: damage comes off the essence (only while revealed); at 0 it dies.
fn essence_damage(world: &mut World) {
    let t = now(world);
    let hurt: Vec<(Entity, f32)> = world
        .query_filtered::<(Entity, &Health), (With<Revenant>, Changed<Health>)>()
        .iter(world)
        .map(|(e, h)| (e, h.brute + h.burn + h.tox + h.oxy))
        .filter(|(_, d)| *d > 0.0)
        .collect();
    for (mob, damage) in hurt {
        if let Some(health) = world.get::<Health>(mob).cloned() {
            world
                .entity_mut(mob)
                .insert((health.healed(), Stat::Conscious));
        }
        let Some(mut r) = world.get::<Revenant>(mob).cloned() else {
            continue;
        };
        if !r.revealed(t) || r.dormant {
            continue;
        }
        r.essence = (r.essence - damage).clamp(0.0, r.max_essence);
        let dead = r.essence <= 0.0;
        world.entity_mut(mob).insert(r);
        if dead {
            death(world, mob);
        }
    }
}

/// `death`: only revealed; dormant, a screech, fading into the residue in 3 s.
pub fn death(world: &mut World, mob: Entity) {
    let t = now(world);
    let Some(mut r) = world.get::<Revenant>(mob).cloned() else {
        return;
    };
    if !r.revealed(t) || r.dormant {
        return;
    }
    r.dormant = true;
    r.harvest = None;
    r.stunned_until = f64::INFINITY;
    r.fade_at = Some(t + FADE_SECS);
    world.entity_mut(mob).insert(r);
    let name = name_of(world, mob);
    let fade = with_rng(world, |rng| rng.prob(50.0));
    visible(
        world,
        mob,
        Span::Warning,
        format!(
            "{name} издаёт затихающий визг, пока фиолетовый туман клубится вокруг его растворяющегося тела!"
        ),
        Some(format!(
            "НЕТ! Нет... уже слишком поздно, вы чувствуете, как ваша эссенция {}...",
            if fade {
                "распадается на части"
            } else {
                "рассеивается"
            }
        )),
    );
    if let Some(&pos) = world.get::<TilePos>(mob) {
        play(world, pos, SCREECH, 100.0);
    }
    sync_state(world, mob);
    info!("{mob} (revenant) died");
}

/// `move_to_ectoplasm`: the residue on its tile holds it.
fn move_to_residue(world: &mut World, mob: Entity) {
    let Some(pos) = crate::explosions::turf_of(world, mob) else {
        return;
    };
    let name = name_of(world, mob);
    visible(
        world,
        mob,
        Span::Danger,
        format!("{name} рассыпается в мелкую голубую пыль."),
        None,
    );
    let residue = crate::construction::spawn_obj(world, RESIDUE, pos);
    if let Some(mut r) = world.get_mut::<Revenant>(mob) {
        r.fade_at = None;
        r.residue = residue;
    }
    let Some(residue) = residue else {
        return;
    };
    let reform_at = now(world) + REFORM_SECS;
    world.entity_mut(residue).insert(Residue {
        revenant: mob,
        reform_at,
    });
    crate::closets::put_mob_inside(world, mob, residue);
}

/// The residue's `reform` -> `release_revenant` -> `reform` + `death_reset`: out on the
/// residue's tile, awake and hidden, with the essence it has.
fn reform(world: &mut World, residue: Entity, mob: Entity) {
    let Some(pos) = crate::explosions::turf_of(world, residue) else {
        return;
    };
    if world.get::<Revenant>(mob).is_none() {
        world.despawn(residue);
        return;
    }
    let name = name_of(world, residue);
    visible(
        world,
        residue,
        Span::BoldNotice,
        format!(
            "{} внезапно взмывает в воздух, прежде чем исчезнуть.",
            ss13_shared::capitalize(&name)
        ),
        None,
    );
    // Taken out of the residue first, so its going does not end the revenant.
    world.entity_mut(residue).remove::<Residue>();
    world.entity_mut(mob).insert(pos);
    crate::closets::let_out(world, mob);
    world.despawn(residue);
    crate::construction::tile_changed(world, pos);
    if let Some(mut r) = world.get_mut::<Revenant>(mob) {
        r.dormant = false;
        r.residue = None;
        r.revealed_until = 0.0;
        r.inhibited_until = 0.0;
        r.stunned_until = 0.0;
        r.harvest = None;
    }
    sync_state(world, mob);
    info!("{mob} (revenant) reformed at {pos:?}");
}

/// The residue's `attack_self`: scattered, the revenant inside is gone.
fn scatter_residue(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world.get::<Residue>(item).is_none()
        && !world.get::<DmItem>(item).is_some_and(|i| i.path == RESIDUE)
    {
        return false;
    }
    let name = name_of(world, item);
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        Span::Notice,
        format!("{user} разбрасывает {name} во все стороны."),
        None,
    );
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы разбрасываете {name} по всей области. Частицы медленно исчезают."),
    );
    world.despawn(item);
    true
}

/// A residue gone before it reformed (`revenant_prison/Destroy`): its revenant is no more.
fn residues_gone(world: &mut World, removed: &mut SystemState<RemovedComponents<Residue>>) {
    let gone: Vec<Entity> = removed
        .get_mut(world)
        .expect("RemovedComponents always validates")
        .read()
        .collect();
    for residue in gone {
        let held: Vec<Entity> = world
            .query::<(Entity, &Revenant)>()
            .iter(world)
            .filter(|(_, r)| r.residue == Some(residue))
            .map(|(e, _)| e)
            .collect();
        for mob in held {
            if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
                crate::ghost::ghostize_client(world, client);
            }
            info!("{mob} (revenant) was destroyed with its residue");
            world.despawn(mob);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Harvest

/// `ClickOn`: a human next to it is harvested.
fn revenant_clicks(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        if world.get::<Revenant>(click.mob).is_none()
            || click.modifiers.right
            || !is_human(world, click.target)
            || dist(click.mob_pos, click.target_pos) > 1
        {
            continue;
        }
        attempt_harvest(world, click.mob, click.target);
    }
}

/// `cast_check(cost)`: enough essence, awake, not inhibited, not inside a wall or dense object.
pub fn cast_check(world: &mut World, mob: Entity, cost: f32, silent: bool) -> bool {
    let t = now(world);
    let Some(r) = world.get::<Revenant>(mob).cloned() else {
        return false;
    };
    let fail = if cost > r.essence {
        Some("Вам не хватает эссенции, чтобы использовать эту способность!")
    } else if r.dormant {
        Some("Ваши силы сейчас дремлют!")
    } else if r.inhibited(t) {
        Some("Ваши силы были подавлены нуллифицирующей энергией!")
    } else {
        density_lock(world, mob)
    };
    if let Some(text) = fail {
        if !silent {
            tell_mob(world, mob, Span::Warning, text);
        }
        return false;
    }
    true
}

/// `turf_density_check`.
fn density_lock(world: &World, mob: Entity) -> Option<&'static str> {
    let pos = *world.get::<TilePos>(mob)?;
    let closed = crate::construction::turf_at(world, pos)
        .and_then(|t| crate::construction::path_of(world, t))
        .is_some_and(|p| is_under(&p, "/turf/closed"));
    if closed {
        return Some("Вы не можете использовать способности, находясь внутри стены.");
    }
    let dense = world
        .resource::<SpatialIndex>()
        .dense_on_tile(pos)
        .iter()
        .any(|&e| e != mob && world.get::<DmObj>(e).is_some());
    dense.then_some("Вы не можете использовать способности внутри плотного объекта.")
}

/// `attempt_harvest` + the first checks of `harvest_soul`.
fn attempt_harvest(world: &mut World, mob: Entity, target: Entity) {
    let t = now(world);
    let Some(r) = world.get::<Revenant>(mob).cloned() else {
        return;
    };
    let name = name_of(world, target);
    if r.drained.contains(&target) {
        tell_mob(
            world,
            mob,
            Span::Warning,
            format!("Душа {name} мертва и пуста."),
        );
        return;
    }
    if !cast_check(world, mob, 0.0, false) {
        return;
    }
    if r.harvest.is_some() {
        tell_mob(
            world,
            mob,
            Span::Warning,
            "Вы уже вытягиваете эссенцию из души!",
        );
        return;
    }
    if world.get::<Stat>(target) == Some(&Stat::Conscious) {
        tell_mob(
            world,
            mob,
            Span::Notice,
            "Его душа слишком сильна для поглощения.",
        );
        if with_rng(world, |rng| rng.prob(10.0)) {
            tell_mob(
                world,
                target,
                Span::Notice,
                "Вам кажется, что за вами наблюдают.",
            );
        }
        return;
    }
    let (Some(&user_pos), Some(&target_pos)) =
        (world.get::<TilePos>(mob), world.get::<TilePos>(target))
    else {
        return;
    };
    let (essence, delay) = with_rng(world, |rng| {
        (
            rng.range(15, 20) as f32,
            rng.range(SEARCH_DS.0, SEARCH_DS.1) as f64 / 10.0,
        )
    });
    tell_mob(world, mob, Span::Notice, format!("Вы ищете душу {name}."));
    if let Some(mut r) = world.get_mut::<Revenant>(mob) {
        r.harvest = Some(Harvest {
            target,
            stage: 0,
            at: t + delay,
            essence,
            user_pos,
            target_pos,
        });
    }
}

/// The harvest's `do_after`s, each checked at its end.
fn harvests(world: &mut World) {
    let t = now(world);
    let due: Vec<(Entity, Harvest)> = world
        .query::<(Entity, &Revenant)>()
        .iter(world)
        .filter_map(|(e, r)| r.harvest.filter(|h| t >= h.at).map(|h| (e, h)))
        .collect();
    for (mob, h) in due {
        let next = harvest_step(world, mob, h, t);
        if let Some(mut r) = world.get_mut::<Revenant>(mob) {
            r.harvest = next;
        }
    }
}

fn harvest_step(world: &mut World, mob: Entity, mut h: Harvest, t: f64) -> Option<Harvest> {
    let target = h.target;
    let name = name_of(world, target);
    let still = world.get::<TilePos>(mob) == Some(&h.user_pos)
        && world.get::<TilePos>(target) == Some(&h.target_pos)
        && world.get::<Revenant>(mob).is_some_and(|r| !r.dormant);
    if !still {
        let text = match h.stage {
            0 => None,
            1 => Some("Поглощение прервано.".to_owned()),
            2 => Some(format!(
                "Вы недостаточно близко, чтобы проникнуть в душу {name}. Связь прервана."
            )),
            _ => Some(format!(
                "Душа {name} была вырвана из ваших объятий. Связь разорвана."
            )),
        };
        if let Some(text) = text {
            tell_mob(world, mob, Span::Warning, text);
        }
        if h.stage >= 3 {
            fall_message(world, target, &name);
        }
        return None;
    }
    match h.stage {
        0 => {
            let has_player = world.get::<PlayerMob>(target).is_some();
            let alive = world.get::<Stat>(target) != Some(&Stat::Dead);
            let (bonus_mind, bonus_life, delay) = with_rng(world, |rng| {
                (
                    rng.range(20, 30) as f32,
                    rng.range(40, 50) as f32,
                    rng.range(WEIGH_DS.0, WEIGH_DS.1) as f64 / 10.0,
                )
            });
            if has_player {
                tell_mob(world, mob, Span::Notice, "Его душа пылает разумом.");
                h.essence += bonus_mind;
            }
            if alive {
                tell_mob(world, mob, Span::Notice, "Его душа пылает жизнью!");
                h.essence += bonus_life;
            }
            tell_mob(
                world,
                mob,
                Span::Notice,
                "Его душа слаба и колеблется. Пришло время поглощать.",
            );
            h.stage = 1;
            h.at = t + delay;
            Some(h)
        }
        1 => {
            let (span, text) = weigh_message(h.essence, &name);
            tell_mob(world, mob, span, text);
            let delay = with_rng(world, |rng| rng.range(REACH_DS.0, REACH_DS.1)) as f64 / 10.0;
            h.stage = 2;
            h.at = t + delay;
            Some(h)
        }
        2 => {
            if world.get::<Stat>(target) == Some(&Stat::Conscious) {
                tell_mob(
                    world,
                    mob,
                    Span::Warning,
                    "Он теперь достаточно силён, чтобы противостоять вашему поглощению!",
                );
                tell_mob(
                    world,
                    target,
                    Span::Danger,
                    "Вы чувствуете, как что-то тянется по вашему телу, прежде чем утихнуть.",
                );
                return None;
            }
            tell_mob(
                world,
                mob,
                Span::Notice,
                format!("Вы начинаете поглощать эссенцию из души {name}."),
            );
            if world.get::<Stat>(target) != Some(&Stat::Dead) {
                tell_mob(
                    world,
                    target,
                    Span::Warning,
                    "Вы испытываете ужасно неприятное ощущение опустошения по мере того, как ваша хватка за жизнь ослабевает...",
                );
            }
            if world.get::<Stat>(target) == Some(&Stat::SoftCrit) {
                crate::status_effects::apply(world, target, Incap::Stun, SOFTCRIT_STUN);
            }
            reveal_and_freeze(world, mob, HARVEST_REVEAL, HARVEST_REVEAL);
            visible(
                world,
                target,
                Span::Warning,
                format!(
                    "{name} внезапно слегка приподнимается в воздух, его кожа становится пепельно-серой."
                ),
                None,
            );
            h.stage = 3;
            h.at = t + DRAIN_SECS;
            Some(h)
        }
        _ => {
            finish_harvest(world, mob, target, h.essence, &name);
            None
        }
    }
}

/// The messages by how much the soul is worth.
pub fn weigh_message(essence: f32, name: &str) -> (Span, String) {
    if essence > 90.0 {
        (
            Span::BoldNotice,
            format!("Ах, совершенная душа. {name} даст вам огромное количество эссенции."),
        )
    } else if essence >= 70.0 {
        (
            Span::BoldNotice,
            format!("Пир для гурманов! {name} принесет вам много пользы."),
        )
    } else if essence >= 30.0 {
        (
            Span::Notice,
            format!("{name} получится среднее количество эссенции."),
        )
    } else {
        (
            Span::Notice,
            format!("{name} не принесёт особой пользы. Тем не менее, важна каждая мелочь."),
        )
    }
}

/// The max essence a drained soul adds: +5 for a living one (≤ 90), +15 for a perfect one.
pub fn max_essence_gain(essence: f32, alive: bool) -> (f32, bool) {
    if essence > 90.0 {
        (15.0, true)
    } else if alive {
        (5.0, false)
    } else {
        (0.0, false)
    }
}

fn fall_message(world: &mut World, target: Entity, name: &str) {
    visible(
        world,
        target,
        Span::Warning,
        format!("{name} падает на землю."),
        Some("Фиолетовые огни, танцующие в вашем поле зрения отсту--".to_owned()),
    );
}

fn finish_harvest(world: &mut World, mob: Entity, target: Entity, essence: f32, name: &str) {
    let alive = world.get::<Stat>(target) != Some(&Stat::Dead);
    let Some(mut r) = world.get::<Revenant>(mob).cloned() else {
        return;
    };
    r.change_essence(essence);
    let (gain, perfect) = max_essence_gain(essence, alive);
    r.max_essence += gain;
    if perfect {
        r.perfect_souls += 1;
    }
    r.drained.push(target);
    let max = r.max_essence;
    world.entity_mut(mob).insert(r);
    tell_mob(
        world,
        mob,
        Span::Notice,
        format!("Получено {essence}Э из {name}."),
    );
    if perfect {
        tell_mob(
            world,
            mob,
            Span::BoldNotice,
            format!(
                "Совершенство души {name} повысило ваш максимальный уровень эссенции. Ваша новая максимальная эссенции - {max}."
            ),
        );
    } else if gain > 0.0 {
        tell_mob(
            world,
            mob,
            Span::BoldNotice,
            format!(
                "Поглощение живой души {name} повысило ваш максимальный уровень эссенции. Ваша новая максимальная эссенция - {max}."
            ),
        );
    }
    tell_mob(
        world,
        mob,
        Span::Notice,
        format!(
            "Душа {name} значительно ослаблена и в настоящее время больше не будет давать эссенции."
        ),
    );
    visible(
        world,
        target,
        Span::Warning,
        format!("{name} падает на землю."),
        Some("Фиолетовый свет, танцующий перед глазами, приближа--".to_owned()),
    );
    if alive {
        info!("{target} died from a revenant harvest by {mob}");
        world.entity_mut(target).insert(Stat::Dead);
    }
    refresh_buttons(world, mob);
}

/// `revealed` and the revenant paralysis for these seconds.
fn reveal_and_freeze(world: &mut World, mob: Entity, reveal: f64, stun: f64) {
    let t = now(world);
    if let Some(mut r) = world.get_mut::<Revenant>(mob) {
        if reveal > 0.0 {
            r.revealed_until = r.revealed_until.max(t + reveal);
        }
        if stun > 0.0 {
            r.stunned_until = r.stunned_until.max(t + stun);
        }
    }
    if stun > 0.0 {
        crate::status_effects::apply(world, mob, Incap::Paralyze, stun as f32);
    }
    sync_state(world, mob);
}

// ---------------------------------------------------------------------------------------------
// Abilities

fn refresh_buttons(world: &mut World, mob: Entity) {
    let t = now(world);
    let Some(r) = world.get::<Revenant>(mob).cloned() else {
        return;
    };
    let icon = |world: &World, dmi: &str, state: &str| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(dmi, state))
    };
    let mut buttons: Vec<(ActionButton, f64)> = world
        .get::<ActionButtons>(mob)
        .map(|b| {
            b.0.iter()
                .filter(|(button, _)| !button.id.starts_with("revenant:"))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let able = !r.dormant && !r.inhibited(t) && !r.stunned(t);
    for &(ability, unlocked, ready_at) in &r.abilities {
        let def = ability.def();
        let affordable = if unlocked {
            r.essence >= def.cast
        } else {
            r.essence_excess >= def.unlock
        };
        buttons.push((
            ActionButton {
                id: ability.id(),
                name: ability.button_name(!unlocked),
                icon: icon(world, ACTIONS_DMI, def.state),
                background: icon(world, BACKGROUNDS_DMI, "bg_revenant"),
                border: icon(world, BACKGROUNDS_DMI, "bg_revenant_border"),
                available: able && affordable,
                cooldown: 0.0,
                active: false,
                maptext: (!unlocked).then(|| "🔒".to_owned()),
            },
            ready_at,
        ));
    }
    set_buttons(world, mob, buttons);
}

fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client) {
            continue;
        }
        let Some(ability) = Ability::ALL.into_iter().find(|a| a.id() == id) else {
            continue;
        };
        use_ability(world, actor, ability);
    }
}

/// `can_cast` + `before_cast` + `cast` + `after_cast` of the ability's component.
pub fn use_ability(world: &mut World, mob: Entity, ability: Ability) -> bool {
    let t = now(world);
    let Some(r) = world.get::<Revenant>(mob).cloned() else {
        return false;
    };
    let Some(&(_, unlocked, ready_at)) = r.abilities.iter().find(|(a, ..)| *a == ability) else {
        return false;
    };
    let def = ability.def();
    if t < ready_at || r.stunned(t) || world.get::<Stat>(mob) != Some(&Stat::Conscious) {
        return false;
    }
    if !unlocked {
        let ok = r.essence_excess >= def.unlock;
        if ok {
            if let Some(mut r) = world.get_mut::<Revenant>(mob) {
                r.essence_excess -= def.unlock;
                if let Some(entry) = r.abilities.iter_mut().find(|(a, ..)| *a == ability) {
                    entry.1 = true;
                }
            }
            tell_mob(
                world,
                mob,
                Span::Notice,
                format!("You have unlocked {}!", def.name),
            );
        } else {
            tell_mob(
                world,
                mob,
                Span::Warning,
                format!("You don't have enough essence to unlock {}!", def.name),
            );
        }
        refresh_buttons(world, mob);
        return false;
    }
    if !cast_check(world, mob, def.cast, false) {
        return false;
    }
    if let Some(mut r) = world.get_mut::<Revenant>(mob) {
        r.change_essence(-def.cast);
        if let Some(entry) = r.abilities.iter_mut().find(|(a, ..)| *a == ability) {
            entry.2 = t + def.cooldown;
        }
    }
    let Some(&center) = world.get::<TilePos>(mob) else {
        return false;
    };
    match ability {
        Ability::Defile => defile(world, center, def.radius),
        Ability::Overload => overload(world, mob, center, def.radius),
        Ability::Malfunction => malfunction(world, mob, center, def.radius),
        Ability::Blight => blight(world, mob, center, def.radius),
        Ability::Haunt => haunt(world, center, def.radius),
        Ability::Vortex => vortex(world, mob, center, def.radius, false),
        Ability::Scatter => vortex(world, mob, center, def.radius, true),
    }
    reveal_and_freeze(world, mob, def.reveal, def.stun);
    info!("{mob} (revenant) cast {ability:?}");
    true
}

/// The entities on the tiles within `radius` (`RANGE_TURFS`).
fn things_around(world: &World, center: TilePos, radius: i32) -> Vec<Entity> {
    world
        .resource::<SpatialIndex>()
        .in_radius(center, radius as u32)
        .filter(|&e| world.get::<TilePos>(e).is_some_and(|p| p.z == center.z))
        .collect()
}

/// Defile: lights flicker, windows crack, unlocked closets open.
fn defile(world: &mut World, center: TilePos, radius: i32) {
    for e in things_around(world, center, radius) {
        if world.get::<crate::power::LightFixture>(e).is_some() {
            let times = with_rng(world, |rng| rng.range(3, 5)) as u32;
            crate::power::lighting::flicker(world, e, times);
            continue;
        }
        let window = world
            .get::<DmObj>(e)
            .is_some_and(|o| is_under(&o.path, crate::obj_damage::WINDOW));
        if window
            && world
                .get::<crate::obj_damage::Integrity>(e)
                .is_some_and(|i| i.current > DEFILE_MAX as f32)
        {
            let damage = with_rng(world, |rng| rng.range(DEFILE_MIN, DEFILE_MAX)) as f32;
            crate::obj_damage::take_damage_flagless(world, e, damage);
            continue;
        }
        if world
            .get::<crate::closets::Closet>(e)
            .is_some_and(|c| !c.locked && !c.opened)
        {
            crate::closets::open(world, e);
        }
    }
}

/// Overload Lights: every lit fixture sparks, 2 s later it shocks the humans within 2 tiles.
fn overload(world: &mut World, mob: Entity, center: TilePos, radius: i32) {
    let at = now(world) + OVERLOAD_DELAY;
    for e in things_around(world, center, radius) {
        let lit = world
            .get::<crate::power::LightFixture>(e)
            .is_some_and(|l| l.mode == crate::power::LightMode::On);
        if !lit {
            continue;
        }
        let name = name_of(world, e);
        let text = format!(
            "{} внезапно ярко вспыхивает и начинает искрить!",
            ss13_shared::capitalize(&name)
        );
        if let Some(&pos) = world.get::<TilePos>(e) {
            for m in crate::pull::visible_message(
                world,
                pos,
                ChatChannel::Warning,
                (Span::BoldWarning, text),
                &[],
            ) {
                world.write_message(m);
            }
        }
        world
            .entity_mut(e)
            .insert(OverloadShock { at, revenant: mob });
    }
}

fn shocks(world: &mut World) {
    let t = now(world);
    let due: Vec<(Entity, OverloadShock)> = world
        .query::<(Entity, &OverloadShock)>()
        .iter(world)
        .filter(|(_, s)| t >= s.at)
        .map(|(e, s)| (e, *s))
        .collect();
    for (light, shock) in due {
        world.entity_mut(light).remove::<OverloadShock>();
        let Some(&pos) = world.get::<TilePos>(light) else {
            continue;
        };
        for victim in living_around(world, pos, SHOCK_RANGE) {
            if victim == shock.revenant || !is_human(world, victim) {
                continue;
            }
            crate::wires::electrocute_act(world, victim, SHOCK_DAMAGE, light, 1.0);
            if let Some(&at) = world.get::<TilePos>(victim) {
                play(world, at, ZAP, 50.0);
            }
        }
    }
}

/// Malfunction: humans and cyborgs take a heavy EMP, one object in five is emagged.
fn malfunction(world: &mut World, mob: Entity, center: TilePos, radius: i32) {
    let client = world.get::<PlayerMob>(mob).map_or(mob, |p| p.client);
    for e in things_around(world, center, radius) {
        if e == mob {
            continue;
        }
        if is_human(world, e) {
            let text = with_rng(world, |rng| {
                [
                    ", что ваше чувство направления исчезает",
                    " пронзительную боль в голове",
                    ", что ваш разум наполняется помехами",
                ][rng.range(0, 2) as usize]
            });
            tell_mob(world, e, Span::Warning, format!("Вы чувствуете{text}."));
            crate::emp::emp_act(world, e, EMP_HEAVY);
            continue;
        }
        if world.get::<crate::silicon::cyborg::Cyborg>(e).is_some() {
            if let Some(&pos) = world.get::<TilePos>(e) {
                play(world, pos, "sound/machines/warning-buzzer.ogg", 50.0);
            }
            crate::emp::emp_act(world, e, EMP_HEAVY);
            continue;
        }
        let Some(path) = world
            .get::<DmObj>(e)
            .map(|o| o.path.clone())
            .or_else(|| world.get::<DmItem>(e).map(|i| i.path.clone()))
        else {
            continue;
        };
        if NO_EMAG.iter().any(|p| is_under(&path, p)) || !with_rng(world, |rng| rng.prob(20.0)) {
            continue;
        }
        let actor = crate::tools::Actor {
            client,
            mob,
            item: None,
        };
        crate::emag::emag_act(world, actor, mob, e);
    }
}

/// Blight: humans catch the blight or it worsens, other living things take 5 toxin.
fn blight(world: &mut World, mob: Entity, center: TilePos, radius: i32) {
    for victim in living_around(world, center, radius) {
        if victim == mob {
            continue;
        }
        if !is_human(world, victim) {
            if let Some(mut health) = world.get::<Health>(victim).cloned() {
                health.adjust(DamageType::Tox, 5.0);
                world.entity_mut(victim).insert(health);
            }
            continue;
        }
        let worsened = world.get_mut::<Diseases>(victim).is_some_and(|mut d| {
            let mut found = false;
            for disease in d.list.iter_mut().filter(|d| d.kind == Kind::Revblight) {
                found = true;
                if disease.stage < 5 {
                    disease.stage += 1;
                }
            }
            found
        });
        if worsened {
            continue;
        }
        crate::medical::diseases::force_contract(world, victim, &Disease::new(Kind::Revblight));
        let text = with_rng(world, |rng| {
            [
                "что внезапно заболели",
                "как подступает тошнота",
                "будто с вашей кожей что-то не так",
            ][rng.range(0, 2) as usize]
        });
        tell_mob(
            world,
            victim,
            Span::Warning,
            format!("Вы чувствуете, {text}"),
        );
    }
}

/// Haunt Objects: up to 7 loose items around, likelier the closer (`150 / distance`%).
fn haunt(world: &mut World, center: TilePos, radius: i32) {
    let t = now(world);
    let mut items: Vec<(Entity, TilePos)> = things_around(world, center, radius)
        .into_iter()
        .filter(|&e| {
            world.get::<DmItem>(e).is_some_and(DmItem::pickable)
                && world.get::<crate::inventory::CarriedBy>(e).is_none()
                && world.get::<Haunted>(e).is_none()
        })
        .filter_map(|e| world.get::<TilePos>(e).map(|&p| (e, p)))
        .collect();
    items.sort_by_key(|(e, _)| *e);
    items.truncate(HAUNT_TARGETS);
    for (item, pos) in items {
        let distance = dist(center, pos).max(1) as f32;
        if !with_rng(world, |rng| rng.prob(150.0 / distance)) {
            continue;
        }
        let secs = with_rng(world, |rng| rng.range(HAUNT_MIN, HAUNT_MAX)) as f64;
        world.entity_mut(item).insert(Haunted {
            until: t + secs,
            next: t + HAUNT_TICK,
            aggro: radius - 1,
        });
        let name = name_of(world, item);
        for m in crate::pull::visible_message(
            world,
            pos,
            ChatChannel::Warning,
            (
                Span::Warning,
                format!(
                    "{} начинает парить и кружиться в воздухе, светясь ужасным фиолетовым светом!",
                    ss13_shared::capitalize(&name)
                ),
            ),
            &[],
        ) {
            world.write_message(m);
        }
    }
}

/// Haunted items throw themselves at the living near them; the haunting ends.
fn haunts(world: &mut World) {
    let t = now(world);
    let due: Vec<(Entity, Haunted)> = world
        .query::<(Entity, &Haunted)>()
        .iter(world)
        .filter(|(_, h)| t >= h.next || t >= h.until)
        .map(|(e, h)| (e, *h))
        .collect();
    for (item, h) in due {
        let Some(&pos) = world.get::<TilePos>(item) else {
            // Picked up or gone: the haunting holds until it lands again.
            if t >= h.until {
                world.entity_mut(item).remove::<Haunted>();
            }
            continue;
        };
        if t >= h.until {
            world.entity_mut(item).remove::<Haunted>();
            let name = name_of(world, item);
            for m in crate::pull::visible_message(
                world,
                pos,
                ChatChannel::Warning,
                (
                    Span::Warning,
                    format!(
                        "{} падает обратно на землю и снова замирает.",
                        ss13_shared::capitalize(&name)
                    ),
                ),
                &[],
            ) {
                world.write_message(m);
            }
            continue;
        }
        if let Some(mut haunted) = world.get_mut::<Haunted>(item) {
            haunted.next = t + HAUNT_TICK;
        }
        let mut victims: Vec<(i32, Entity, TilePos)> = living_around(world, pos, h.aggro)
            .into_iter()
            .filter(|&e| world.get::<Stat>(e) != Some(&Stat::Dead))
            .filter_map(|e| world.get::<TilePos>(e).map(|&p| (dist(pos, p), e, p)))
            .collect();
        victims.sort_by_key(|(d, e, _)| (*d, *e));
        if let Some(&(d, _, to)) = victims.first() {
            crate::throw::throw_at(world, item, to, (d + 1) as u32, 1.0);
        }
    }
}

/// Vortex (pull) and Scatter (push): storage around it is emptied, then `goonchem_vortex`.
fn vortex(world: &mut World, mob: Entity, center: TilePos, radius: i32, push: bool) {
    for e in things_around(world, center, radius) {
        let is_storage = world
            .get::<DmItem>(e)
            .is_some_and(|i| is_under(&i.path, "/obj/item/storage"));
        if is_storage && let Some(&pos) = world.get::<TilePos>(e) {
            // `emptyStorage`.
            let inside: Vec<Entity> = world
                .query::<(Entity, &crate::storage::StoredIn)>()
                .iter(world)
                .filter(|(_, s)| s.0 == e)
                .map(|(i, _)| i)
                .collect();
            for item in inside {
                crate::storage::take_out(world, item, pos);
            }
            crate::construction::tile_changed(world, pos);
        }
    }
    let range = if push { 5 } else { 3 };
    play(world, center, if push { HISS } else { WOOSH }, 50.0);
    let movers: Vec<(Entity, TilePos)> = world
        .resource::<SpatialIndex>()
        .in_radius(center, range as u32)
        .filter(|&e| e != mob)
        .filter_map(|e| world.get::<TilePos>(e).map(|&p| (e, p)))
        .filter(|(e, p)| {
            p.z == center.z
                && *p != center
                && (world.get::<DmItem>(*e).is_some_and(DmItem::pickable)
                    || (world.get::<Stat>(*e).is_some_and(|s| *s != Stat::Dead)
                        && world.get::<Revenant>(*e).is_none()))
                && world.get::<crate::inventory::CarriedBy>(*e).is_none()
        })
        .collect();
    for (e, pos) in movers {
        let power = vortex_power(range, dist(center, pos));
        let to = if push {
            let (dx, dy) = ((pos.x - center.x).signum(), (pos.y - center.y).signum());
            TilePos::new(pos.x + dx * power, pos.y + dy * power, pos.z)
        } else {
            center
        };
        crate::throw::throw_at(world, e, to, power as u32, 1.0);
    }
}

/// `goonchem_vortex`'s `moving_power`.
pub fn vortex_power(range: i32, distance: i32) -> i32 {
    (range - distance).max(1)
}

// ---------------------------------------------------------------------------------------------
// The blight

/// `/datum/disease/revblight/stage_act` after the base one let it act.
pub fn revblight_act(
    world: &mut World,
    mob: Entity,
    stage: u8,
    secs: f32,
    rng: &mut crate::combat::CombatRng,
) -> Outcome {
    use crate::medical::wounds::spt_prob;
    let t = now(world);
    let state = world.get::<Blighted>(mob).copied().unwrap_or_default();
    let final_stage = state.cure_at.is_some();
    let stage_f = f32::from(stage);
    if !final_stage {
        let lying = world
            .get::<crate::health::BodyPosition>(mob)
            .is_some_and(|b| b.lying());
        if lying && spt_prob(rng, 3.0 * stage_f, secs) {
            return cure_blight(world, mob);
        }
        if spt_prob(rng, 1.5 * stage_f, secs) {
            let feel = [
                "sick and tired",
                "disoriented",
                "tired and confused",
                "nauseated",
                "faint",
                "dizzy",
            ][rng.range(0, 5) as usize];
            tell_mob(
                world,
                mob,
                Span::Notice,
                format!("You suddenly feel {feel}..."),
            );
            crate::status_effects::adjust_timed(
                world,
                mob,
                crate::status_effects::Timed::Confusion,
                8.0,
                None,
            );
            crate::status_effects::adjust_stamina(world, mob, 20.0);
        }
        if state.stage_damage < stage {
            if let Some(mut health) = world.get::<Health>(mob).cloned() {
                health.adjust(DamageType::Tox, stage_f * secs);
                world.entity_mut(mob).insert(health);
            }
            world.entity_mut(mob).insert(Blighted {
                stage_damage: state.stage_damage + 1,
                ..state
            });
        }
        if spt_prob(rng, 25.0, secs) {
            crate::status_effects::adjust_stamina(world, mob, stage_f);
        }
    }
    let emotes: &[&str] = match stage {
        2 if spt_prob(rng, 2.5, secs) => &["pale"],
        3 if spt_prob(rng, 5.0, secs) => &["pale", "shiver"],
        4 if spt_prob(rng, 7.5, secs) => &["pale", "shiver", "cries"],
        _ => &[],
    };
    if !emotes.is_empty() {
        let key = emotes[rng.range(0, emotes.len() as u32 - 1) as usize];
        if world.get::<Stat>(mob) == Some(&Stat::Conscious)
            && let Some(emote) = ss13_chat::emote::find(key)
        {
            let text: &str = emote.message;
            let text = text.to_owned();
            crate::mobs::defense::emote(world, mob, &text);
        }
    }
    if stage == 5 && !final_stage {
        let feel = [
            "nothing's worth it anymore",
            "nobody ever needed your help",
            "nothing you did mattered",
            "everything you tried to do was worthless",
        ][rng.range(0, 3) as usize];
        tell_mob(world, mob, Span::Notice, format!("You feel like {feel}."));
        crate::status_effects::adjust_stamina(world, mob, 22.5 * secs);
        let name = name_of(world, mob);
        visible(
            world,
            mob,
            Span::Warning,
            format!("{name} looks terrifyingly gaunt..."),
            Some("You suddenly feel like your skin is wrong...".to_owned()),
        );
        let stage_damage = world
            .get::<Blighted>(mob)
            .map_or(state.stage_damage, |b| b.stage_damage);
        world.entity_mut(mob).insert(Blighted {
            stage_damage,
            cure_at: Some(t + 10.0),
        });
    }
    Outcome::Keep
}

/// `revblight/cure`.
fn cure_blight(world: &mut World, mob: Entity) -> Outcome {
    world.entity_mut(mob).remove::<Blighted>();
    tell_mob(world, mob, Span::Notice, "You feel better.");
    Outcome::Cure { resistance: false }
}

/// The final stage's `addtimer(cure, 10 SECONDS)`.
fn blight_cures(world: &mut World) {
    let t = now(world);
    let due: Vec<Entity> = world
        .query::<(Entity, &Blighted)>()
        .iter(world)
        .filter(|(_, b)| b.cure_at.is_some_and(|at| t >= at))
        .map(|(e, _)| e)
        .collect();
    for mob in due {
        world.entity_mut(mob).remove::<Blighted>();
        if let Some(mut d) = world.get_mut::<Diseases>(mob) {
            d.list.retain(|d| d.kind != Kind::Revblight);
        }
        tell_mob(world, mob, Span::Notice, "You feel better.");
    }
}

/// Holy water is the blight's cure (`cures`).
pub const REVBLIGHT_CURES: &[&str] = &[HOLY_WATER];

// ---------------------------------------------------------------------------------------------
// Info and the round end

fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if world.get::<Revenant>(source).is_some() {
            world.write_message(OpenUi::new::<RevenantInfoWindow>(
                client, actor, source, ANTAG_NAME,
            ));
        }
    }
}

/// Each revenant and its objectives (the essence one fails if it is dormant).
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let mut all: Vec<(Entity, Revenant)> = world
        .query::<(Entity, &Revenant)>()
        .iter(world)
        .map(|(e, r)| (e, r.clone()))
        .collect();
    if all.is_empty() {
        return Vec::new();
    }
    all.sort_by_key(|(e, _)| *e);
    let line = |span: Span, text: String| ChatMessage {
        channel: ChatChannel::System,
        segments: vec![ChatSegment::new(span, text)],
    };
    let mut out = vec![line(Span::Bold, format!("{ROUNDEND_CATEGORY} были:"))];
    for (mob, r) in all {
        out.push(super::traitor::print_player(world, mob, &r.key));
        let mut all_done = true;
        for (i, (text, done)) in objectives(&r).into_iter().enumerate() {
            let done = done && !(i == 0 && r.dormant);
            all_done &= done;
            let (span, verdict) = if done {
                (Span::GreenText, "Успех!")
            } else {
                (Span::RedText, "Провал.")
            };
            out.push(line(span, format!("Задача №{}: {text} {verdict}", i + 1)));
        }
        out.push(if all_done {
            line(Span::GreenText, format!("{ANTAG_NAME} преуспел!"))
        } else {
            line(Span::RedText, format!("{ANTAG_NAME} провалился!"))
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn essence_changes_count_what_was_stolen() {
        let mut r = Revenant::new(String::new(), 350, String::new());
        assert!(r.change_essence(-50.0));
        assert_eq!(r.essence, 25.0);
        assert!(!r.change_essence(-30.0), "never below zero");
        assert_eq!(r.essence, 25.0);
        assert!(r.change_essence(60.0));
        assert_eq!((r.essence_accumulated, r.essence_excess), (60.0, 60.0));
        assert!(!r.objective_done());
        r.essence_accumulated = 350.0;
        assert!(r.objective_done());
    }

    #[test]
    fn souls_are_weighed() {
        assert_eq!(max_essence_gain(95.0, true), (15.0, true));
        assert_eq!(max_essence_gain(95.0, false), (15.0, true));
        assert_eq!(max_essence_gain(60.0, true), (5.0, false));
        assert_eq!(max_essence_gain(20.0, false), (0.0, false));
        assert_eq!(weigh_message(91.0, "x").0, Span::BoldNotice);
        assert_eq!(weigh_message(70.0, "x").0, Span::BoldNotice);
        assert_eq!(weigh_message(30.0, "x").0, Span::Notice);
        assert!(weigh_message(10.0, "x").1.contains("не принесёт"));
    }

    #[test]
    fn abilities_follow_their_spells() {
        let defile = Ability::Defile.def();
        assert_eq!((defile.unlock, defile.cast, defile.radius), (10.0, 30.0, 4));
        assert_eq!(Ability::Malfunction.def().unlock, 125.0);
        assert_eq!(Ability::Haunt.def().cooldown, 0.0);
        assert_eq!(Ability::Scatter.def().cooldown, 25.0);
        assert_eq!(
            Ability::Overload.button_name(true),
            "Перегрузка света (25SE)"
        );
        assert_eq!(
            Ability::Overload.button_name(false),
            "Перегрузка света (40E)"
        );
        assert_eq!(vortex_power(3, 0), 3);
        assert_eq!(vortex_power(3, 3), 1);
        assert_eq!(vortex_power(5, 9), 1);
    }

    #[test]
    fn names_come_from_the_strings() {
        let mut rng = Rng::new(3);
        for _ in 0..20 {
            let name = random_name(&mut rng);
            assert!(SPIRIT_TYPES.iter().any(|s| name.starts_with(s)), "{name}");
            assert!(THEMES.iter().any(|s| name.ends_with(s)), "{name}");
        }
    }

    /// Unlocking spends stolen essence; casting spends essence, starts the cooldown and
    /// reveals and freezes the revenant.
    #[test]
    fn abilities_unlock_then_cast() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let mob = world
            .spawn((
                Revenant::new(String::new(), 350, String::new()),
                Stat::Conscious,
                TilePos::new(1, 1, 1),
            ))
            .id();
        assert!(!use_ability(&mut world, mob, Ability::Defile));
        let r = world.get::<Revenant>(mob).unwrap();
        assert!(
            !r.abilities.iter().any(|(_, u, _)| *u),
            "nothing to unlock with"
        );
        world.get_mut::<Revenant>(mob).unwrap().essence_excess = 15.0;
        assert!(!use_ability(&mut world, mob, Ability::Defile));
        let r = world.get::<Revenant>(mob).unwrap();
        assert_eq!(r.essence_excess, 5.0);
        assert!(
            r.abilities
                .iter()
                .any(|(a, u, _)| *a == Ability::Defile && *u)
        );
    }

    #[test]
    fn the_ruleset_takes_the_revenant_role() {
        let r = super::super::midround::ruleset(super::super::midround::REVENANT).unwrap();
        assert_eq!(r.role, Some(ROLE_REVENANT));
        assert!(!r.repeatable);
        assert_eq!(r.min_pop, 10);
    }
}
