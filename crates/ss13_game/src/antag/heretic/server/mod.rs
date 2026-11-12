//! The heretic (server) after tg `/datum/antagonist/heretic` (`heretic_antag.dm`), the knowledge
//! (`heretic_knowledge.dm`, `knowledge/`), the influences (`influences.dm`), the transmutation
//! rune (`transmutation_rune.dm`), the living heart (`heretic_living_heart.dm`), the sacrifice
//! (`knowledge/sacrifice_knowledge/`), the Mansus grasp (`magic/mansus_grasp.dm`), the sickly
//! blades (`items/heretic_blades.dm`) and the Path of Ash (`knowledge/ash_lore.dm`, its spells
//! in `magic/`).
//!
//! - [`add_heretic`] is `add_antag_datum(/datum/antagonist/heretic)`: one knowledge point, the
//!   start knowledge (the grasp, the living heart, the sacrifice, the Feast of Owls), the
//!   "pick a path" objective, the `AntagInfoHeretic` window, the heretic HUD icon, the spell
//!   buttons, influences spawned on the station for it and a point every 20 minutes.
//! - Influences (`/obj/effect/heretic_influence`) are invisible (observer level) and replicated
//!   only to heretics' clients ([`HERETIC_SIGHT`]) and ghosts. A right click with an empty hand
//!   drains one for a point in 10 s (a Codex Cicatrix: two points in 5-7.5 s); it becomes a
//!   pierced reality everyone sees.
//! - A writing tool on a floor with the grasp in the other hand draws the transmutation rune
//!   (20 s); an empty hand on it lists the rituals the heretic can do; a ritual takes what lies
//!   on and around the rune ([`super::knowledge::match_recipe`]).
//! - Researching a path's starting knowledge picks the path: its tree, drafts and shop, and the
//!   objectives (research, minor sacrifices, a head of staff with two or more heads alive).
//! - Ash: the grasp marks and blurs, the ashen blade triggers the mark (stamina and burn that
//!   spreads), Volcano Blast chains fire, the Fiery Rebirth drains the burning, the blade upgrade
//!   ignites, and the Ritual of the Ash Lord (three burning or husked corpses) ascends with
//!   CentCom's announcement, the emergency shuttle and the Oath of Flame / Fire Cascade spells.
// shortcut: the Path of Ash is here; the other paths' marks, grasp effects, blade upgrades, Void
// Pull, Realignment and the Blade and Void ascensions are in [`super::paths`]. Their other
// spells get a button that says it is not ported, recipes spawn their results when the
// prototype exists, and passives do nothing. Side knowledge has no effects beyond its ritual's
// results and what [`items`], [`paintings`] and [`summon`] give them (curses: [`curses`]);
// the knowledge ritual, the heretic aura, dreams, x-ray near influences, rust healing and spell
// recharge by holy water/passive are not ported. Ash Passage (jaunt) is not ported. No shadow realm:
// see [`begin_sacrifice`].
// The radials are list windows; balloon alerts are chat warnings.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_chat::capitalize;
use ss13_chat::server::{say_message, whisper_message};
use ss13_mapload::{Areas, DmName, IconIndex};
use ss13_net::chat::server::SendChat;
use ss13_net::server::{ClientToken, Invisibility, SeeGroups, VisibleToGroups};
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::list_input::{ListInputData, ListInputUi};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};

use super::data::{
    INFLUENCE_DRAIN_MESSAGE, INFLUENCE_DRAINED, INFLUENCE_POSTFIX, INFLUENCE_PREFIX, PATHS,
};
use super::knowledge::{
    self, BASIC, Candidate, Category, Entry, FEAST_OF_OWLS, HUNT_AND_SACRIFICE, Kind, KnowledgeDef,
    LIVING_HEART, Shop,
};
use super::{AntagInfoHereticUi, HereticInfo, HereticPath, HereticStatic, KnowledgeNode, RESEARCH};
use crate::actions::{ActionButton, ActionButtonsUi, set_buttons};
use crate::antag::objectives::RoundEnd;
use crate::antag::{AntagHudData, AntagHudUi, ObjectiveInfo};
use crate::combat::{CombatRng, NextMove};
use crate::construction::{Job, path_of};
use crate::ghost::Ghost;
use crate::health::{DamageType, Health, Stat, apply_damage};
use crate::human::HumanLook;
use crate::inventory::{CarriedBy, Inventory, NoDrop};
use crate::jobs::{Jobs, PlayerJob};
use crate::medical::{name_of, visible};
use crate::server::{PlayerMob, Players};
use crate::status_effects::{Incap, effects_mut};
use crate::tools::{Actor, DoAfter, do_after, tell};

mod buttons;
mod clicks;
mod curses;
mod gain;
mod items;
mod paintings;
mod rune;
mod sacrifice;
mod summon;
mod tick;
mod windows;
pub use buttons::*;
pub use clicks::*;
pub use curses::{CODEX_MORBUS, CorrosionCurse, CurseKind, ParalysisCurse, RaceSwap, curse};
use curses::{curse_prompts, morbus_on_rune, tick_heretic_curses};
pub use gain::*;
pub use items::{
    CRUCIBLE, CRUCIBLE_MAX_MASS, Crucible, DuskAndDawn, FocusWorn, POTIONS, examine_lines,
    focus_recharge_amount, has_heretic_vision, hastened, heretic_vision,
};
use items::{PAINTING, brew_potion, squeeze_medallion};
use paintings::painting_recipe;
pub use paintings::{EldritchPainting, PaintingCurse, PaintingExamined, PaintingKind};
pub use rune::*;
pub use sacrifice::*;
use summon::begin_summon;
pub use summon::{
    HereticMonster, PendingSummon, SUMMON_POLL_SECS, is_heretic_or_monster, summon_of,
};
pub use tick::*;
pub use windows::*;

/// `name = "\improper Еретик"`.
pub const HERETIC_NAME: &str = "Еретик";
pub const ROUNDEND_CATEGORY: &str = "Еретики";
const OBJECTIVE_NAME: &str = "Задача";

/// The [`VisibleToGroups`] bit of what only heretics see (tg's `has_antagonist/heretic` HUD).
pub const HERETIC_SIGHT: u32 = 1 << 0;

pub const INFLUENCE: &str = "/obj/effect/heretic_influence";
pub const VISIBLE_INFLUENCE: &str = "/obj/effect/visible_heretic_influence";
pub const RUNE: &str = "/obj/effect/heretic_rune/big";
pub const MANSUS_FIST: &str = "/obj/item/melee/touch_attack/mansus_fist";
pub const SICKLY_BLADE: &str = "/obj/item/melee/sickly_blade";
pub const CODEX: &str = "/obj/item/codex_cicatrix";
const CULT_CUFFS: &str = "/obj/item/restraints/handcuffs/cult";

const ELDRITCH_DMI: &str = "icons/effects/eldritch.dmi";
const ANTAG_HUD_DMI: &str = "icons/mob/huds/antag_hud.dmi";
const BACKGROUNDS_DMI: &str = "icons/mob/actions/backgrounds.dmi";
const KNOWLEDGE_DMI: &str = "icons/ui_icons/antags/heretic/knowledge.dmi";
const ECULT_DMI: &str = "icons/mob/actions/actions_ecult.dmi";

/// `NUM_INFLUENCES_PER_HERETIC`.
pub const NUM_INFLUENCES_PER_HERETIC: u32 = 5;
/// `HERETIC_RIFT_DEFAULT_DRAIN_SPEED` (deciseconds); the codex's `drain_speed` closed and open.
pub const DRAIN_DS: f32 = 100.0;
const CODEX_DRAIN_DS: f32 = 75.0;
/// `passive_gain_timer`.
pub const PASSIVE_GAIN_SECS: f64 = 20.0 * 60.0;
/// `points_to_aura`.
pub const POINTS_TO_AURA: u32 = 8;
/// `draw_rune` `drawing_time` (deciseconds).
pub const RUNE_DRAW_DS: f32 = 200.0;
/// The grasp's `cooldown_time`.
pub const GRASP_COOLDOWN: f64 = 10.0;
/// `/datum/status_effect/eldritch` `duration`.
pub const MARK_SECS: f64 = 15.0;
/// `SACRIFICE_SLEEP_DURATION`.
pub const SACRIFICE_SLEEP_SECS: f64 = 12.0;
/// `hunt_and_sacrifice` `num_targets_to_generate`.
const NUM_TARGETS: usize = 5;
/// BandaStation `MIN_DISTANSE_TO_EXPLODE`.
const MIN_DISTANCE_TO_EXPLODE: i32 = 10;

/// Jobs with tg `JOB_HEAD_OF_STAFF` (BandaStation adds the representative and the magistrate).
pub const HEADS_OF_STAFF: [&str; 9] = [
    "Captain",
    "Head of Personnel",
    "Head of Security",
    "Chief Engineer",
    "Research Director",
    "Chief Medical Officer",
    "Quartermaster",
    "Nanotrasen Representative",
    "Magistrate",
];

const FIRE_BLAST: &str = "/datum/action/cooldown/spell/charged/beam/fire_blast";
const FIERY_REBIRTH: &str = "/datum/action/cooldown/spell/aoe/fiery_rebirth";
const GRASP_ACTION: &str = "/datum/action/cooldown/spell/touch/mansus_grasp";
const ASH_START: &str = "/datum/heretic_knowledge/limited_amount/starting/base_ash";
const ASH_BLADE_UPGRADE: &str = "/datum/heretic_knowledge/blade_upgrade/ash";
const ASH_FINAL: &str = "/datum/heretic_knowledge/ultimate/ash_final";

const SPELL_PREFIX: &str = "heretic_spell:";
const HEART_ID: &str = "heretic_living_heart";
const FIRE_SWORN_ID: &str = "heretic_fire_sworn";
const FIRE_CASCADE_ID: &str = "heretic_fire_cascade";

/// `GLOB.heretic_path_to_color`.
pub fn path_color(route: Option<&str>) -> [u8; 3] {
    match route {
        Some("Путь Ржавчины") => [0xB1, 0x86, 0x44],
        Some("Путь Плоти") => [0xFA, 0x82, 0x82],
        Some("Путь Пепла") => [0xFF, 0x32, 0x32],
        Some("Путь Пустоты") => [0x00, 0xFF, 0xFF],
        Some("Путь Клинка") => [0xC0, 0xC0, 0xC0],
        Some("Путь Космоса") => [0x80, 0x00, 0x80],
        Some("Путь Замка") => [0xFF, 0xFF, 0x00],
        Some("Путь Луны") => [0x33, 0xCC, 0xFF],
        _ => [0x32, 0xCD, 0x32],
    }
}

// ---------------------------------------------------------------------------------------------
// State

/// A researched knowledge (`researched_knowledge[path]`: the entry and the instance's state).
#[derive(Clone, Debug)]
pub struct Researched {
    pub entry: Entry,
    /// Spells: `charges`.
    pub charges: u32,
    /// `limited_amount` `created_items`.
    pub created: Vec<Entity>,
}

impl Researched {
    pub fn def(&self) -> &'static KnowledgeDef {
        self.entry.def()
    }
}

/// A heretic objective.
#[derive(Clone, Debug, PartialEq)]
pub enum HereticObjective {
    /// `/datum/objective/pick_path`.
    PickPath,
    /// `/datum/objective/heretic_research`.
    Research { target: u32 },
    /// `/datum/objective/minor_sacrifice`.
    MinorSacrifice { target: u32 },
    /// `/datum/objective/major_sacrifice`.
    MajorSacrifice,
}

impl HereticObjective {
    pub fn explanation(&self) -> String {
        match self {
            Self::PickPath => "Pick a path to pursue.".into(),
            Self::Research { target } => format!(
                "Изучите как минимум {target} знаний Мансуса. Изначально исследовано: {}",
                knowledge::start_knowledge().count()
            ),
            Self::MinorSacrifice { target } => {
                format!("Принесите в жертву не менее {target} членов экипажа.")
            }
            Self::MajorSacrifice => "Пожертвуйте главу станции.".into(),
        }
    }

    pub fn complete(&self, heretic: &Heretic) -> bool {
        match self {
            Self::PickPath => false,
            Self::Research { target } => heretic.researched.len() as u32 >= *target,
            Self::MinorSacrifice { target } => heretic.total_sacrifices >= *target,
            Self::MajorSacrifice => heretic.high_value_sacrifices >= 1,
        }
    }
}

/// The heretic antagonist datum, on its body.
#[derive(Component, Clone, Debug)]
pub struct Heretic {
    pub key: String,
    /// `knowledge_points`.
    pub points: u32,
    pub knowledge_gained: u32,
    pub start_shop: Shop,
    pub tree: Shop,
    pub shop: Shop,
    pub draft: Shop,
    pub researched: Vec<Researched>,
    /// `heretic_path.route`.
    pub route: Option<&'static str>,
    pub objectives: Vec<HereticObjective>,
    /// `sac_targets` (bodies).
    pub sac_targets: Vec<Entity>,
    /// `all_sac_targets`: the names, for the report.
    pub all_sac_targets: Vec<String>,
    pub total_sacrifices: u32,
    pub high_value_sacrifices: u32,
    pub ascended: bool,
    pub feast_of_owls: bool,
    /// A living heart in the chest (tg: a `living_heart` component on the heart organ).
    pub living_heart: bool,
    pub drawing_rune: bool,
    pub unlimited_blades: bool,
    /// `exploded_blades` (BandaStation).
    pub exploded_blades: u32,
    pub next_passive_gain: f64,
    /// Spell cooldowns: (button id, ready at).
    pub cooldowns: Vec<(String, f64)>,
    /// The living heart's last target (`last_tracked_name`).
    pub last_tracked: Option<Entity>,
    /// Ascension spells.
    pub ash_ascended_spells: bool,
}

impl Heretic {
    fn new(key: String, now: f64) -> Self {
        Self {
            key,
            points: 1,
            knowledge_gained: 0,
            start_shop: knowledge::start_shop(),
            tree: Vec::new(),
            shop: Vec::new(),
            draft: Vec::new(),
            researched: Vec::new(),
            route: None,
            objectives: vec![HereticObjective::PickPath],
            sac_targets: Vec::new(),
            all_sac_targets: Vec::new(),
            total_sacrifices: 0,
            high_value_sacrifices: 0,
            ascended: false,
            feast_of_owls: false,
            living_heart: false,
            drawing_rune: false,
            unlimited_blades: false,
            exploded_blades: 0,
            next_passive_gain: now + PASSIVE_GAIN_SECS,
            cooldowns: Vec::new(),
            last_tracked: None,
            ash_ascended_spells: false,
        }
    }

    pub fn has(&self, path: &str) -> bool {
        self.researched.iter().any(|r| r.entry.path == path)
    }

    pub fn get(&self, path: &str) -> Option<&Researched> {
        self.researched.iter().find(|r| r.entry.path == path)
    }

    fn get_mut(&mut self, path: &str) -> Option<&mut Researched> {
        self.researched.iter_mut().find(|r| r.entry.path == path)
    }

    pub fn shop_of(&self, category: Category) -> &Shop {
        match category {
            Category::Start => &self.start_shop,
            Category::Tree => &self.tree,
            Category::Shop => &self.shop,
            Category::Draft => &self.draft,
        }
    }

    fn shop_mut(&mut self, category: Category) -> &mut Shop {
        match category {
            Category::Start => &mut self.start_shop,
            Category::Tree => &mut self.tree,
            Category::Shop => &mut self.shop,
            Category::Draft => &mut self.draft,
        }
    }

    pub fn researchable(&self) -> Vec<String> {
        knowledge::researchable(self.researched.iter().map(|r| &r.entry))
    }

    /// `can_ascend`: `Ok` or tg's reason.
    pub fn can_ascend(&self) -> Result<(), &'static str> {
        if self.feast_of_owls {
            return Err("Совы отняли у вас право на восхождение (отказано в вознесении).");
        }
        if self.objectives.iter().any(|o| !o.complete(self)) {
            return Err("Необходимо выполнить все задания для возвышения.");
        }
        // `minimum_ascension_time` defaults to 0 in BandaStation's config.
        Ok(())
    }

    /// `adjust_knowledge_points`; true when it crossed the aura threshold.
    fn adjust_points(&mut self, amount: i32) -> bool {
        self.points = (self.points as i32 + amount).max(0) as u32;
        self.knowledge_gained += amount.max(0) as u32;
        self.knowledge_gained > POINTS_TO_AURA && !self.unlimited_blades
    }

    fn cooldown(&self, id: &str) -> f64 {
        self.cooldowns
            .iter()
            .find(|(i, _)| i == id)
            .map_or(0.0, |(_, t)| *t)
    }

    fn set_cooldown(&mut self, id: &str, ready_at: f64) {
        match self.cooldowns.iter_mut().find(|(i, _)| i == id) {
            Some(c) => c.1 = ready_at,
            None => self.cooldowns.push((id.to_owned(), ready_at)),
        }
    }
}

/// `GLOB.reality_smash_track`.
#[derive(Resource, Clone, Debug, Default)]
pub struct RealityTracker {
    pub num_drained: u32,
    pub smashes: Vec<Entity>,
    /// `tracked_heretics` (bodies).
    pub tracked: Vec<Entity>,
}

/// `hunt_and_sacrifice` `target_blacklist` (static): sacrificed bodies never targeted again.
#[derive(Resource, Clone, Debug, Default)]
pub struct SacrificeBlacklist(pub Vec<Entity>);

/// `/obj/effect/heretic_influence`.
#[derive(Component, Clone, Debug, Default)]
pub struct Influence {
    pub being_drained: bool,
}

/// `/obj/effect/visible_heretic_influence`: fades in 15 s after it appears, over 15 s.
#[derive(Component, Clone, Copy, Debug)]
pub struct VisibleInfluence {
    pub born: f64,
}

/// `/obj/effect/heretic_rune`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct TransmutationRune {
    pub in_use: bool,
}

/// `/obj/item/melee/touch_attack/mansus_fist` in its caster's hand.
#[derive(Component, Clone, Copy, Debug)]
pub struct MansusFist {
    pub owner: Entity,
}

/// `/datum/status_effect/eldritch/ash` (the only ported mark).
#[derive(Component, Clone, Copy, Debug)]
pub struct AshMark {
    pub until: f64,
    pub repetitions: u32,
}

/// A body in the sacrifice chain: it wakes and returns at `wake_at`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Sacrificed {
    pub heretic: Entity,
    pub wake_at: f64,
}

/// `/datum/status_effect/fire_blasted` and the beam bouncing on from it.
#[derive(Component, Clone, Copy, Debug)]
pub struct FireBlasted {
    pub until: f64,
    pub next_tick: f64,
    /// Damage per second (negative heals: the caster).
    pub tick_damage: f32,
    /// (caster, bounces left, when).
    pub bounce: Option<(Entity, u32, f64)>,
}

/// `/datum/status_effect/fire_ring` (Oath of Flame).
#[derive(Component, Clone, Copy, Debug)]
pub struct FireRing {
    pub until: f64,
    pub next_tick: f64,
}

/// A Fire Cascade spreading from `center`.
#[derive(Component, Clone, Copy, Debug)]
pub struct FireCascade {
    pub caster: Entity,
    pub center: TilePos,
    pub step: i32,
    pub radius: i32,
    pub next_at: f64,
}

/// Melee hits with a sickly blade waiting for the attack to land (`afterattack`).
#[derive(Resource, Default)]
struct PendingBladeHits(Vec<(Entity, Entity, f64)>);

/// What a list prompt picks.
#[derive(Clone, Debug, PartialEq)]
pub enum PromptKind {
    /// The rituals of a rune: knowledge paths in `values`.
    Ritual { rune: Entity },
    /// The living heart's targets.
    Track,
    /// `pre_research` of final knowledge.
    ConfirmFinal {
        path: &'static str,
        category: Category,
    },
    /// The Feast of Owls' alert.
    ConfirmFeast { rune: Entity },
    /// The mawed crucible's brews (`create_potion`'s radial): types in `paths`.
    Potion { crucible: Entity },
}

/// A list prompt on the heretic (`tgui_input_list`, tg radials and alerts).
#[derive(Component, Clone, Debug)]
pub struct HereticPrompt {
    pub kind: PromptKind,
    pub title: String,
    pub message: String,
    pub items: Vec<String>,
    pub paths: Vec<&'static str>,
    pub targets: Vec<Entity>,
}

/// The list prompt window (`ListInputWindow`).
pub struct HereticListWindow;

impl UiInterface for HereticListWindow {
    const NAME: &'static str = ListInputUi::NAME;
    type Data = ListInputData;
    type StaticData = ();
}

/// Timed actions of the heretic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HereticJob {
    DrawRune,
    Drain {
        points: u32,
    },
    FireBlast,
    /// The crimson medallion squeezed.
    Squeeze,
}

pub struct HereticPlugin;

impl Plugin for HereticPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // A sickly blade breaks to teleport its heretic.
        app.add_click_handler(ClickPhase::AttackSelf, 600, |w, c| {
            crate::antag::heretic::server::attack_self(w, c.client, c.mob, c.target)
        });
        // A heretic's rune (the rituals), an influence (drained by a right click).
        app.add_click_handler(ClickPhase::EmptyHand, 400, |w, c| {
            crate::antag::heretic::server::interact(w, c.client, c.mob, c.target, c.modifiers.right)
        });
        // A pen draws a heretic's rune, the codex drains, the grasp erases a rune.
        app.add_click_handler(ClickPhase::HeldOnTarget, 1400, |w, c| {
            c.held.is_some_and(|h| {
                crate::antag::heretic::server::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        // The Codex Morbus right-clicked on a rune casts a curse (before the codex's own clicks).
        app.add_click_handler(ClickPhase::HeldOnTarget, 1393, |w, c| {
            c.modifiers.right
                && c.held
                    .is_some_and(|h| morbus_on_rune(w, c.client, c.mob, h, c.target))
        });
        items::build(app);
        paintings::build(app);
        app.register_ui::<AntagInfoHereticUi>()
            .register_ui::<HereticListWindow>()
            .init_resource::<RealityTracker>()
            .init_resource::<SacrificeBlacklist>()
            .init_resource::<PendingBladeHits>()
            .add_systems(
                Update,
                heretic_clicks
                    .after(crate::security::security_clicks)
                    .before(crate::combat::MeleeSystems),
            )
            .add_systems(
                Update,
                after_melee
                    .after(crate::combat::MeleeSystems)
                    .before(crate::health::HealthSystems),
            )
            .add_systems(
                Update,
                (open_info, trigger_buttons, list_acts, research_acts).in_set(UiSystems::Handle),
            )
            .add_systems(
                Update,
                (heretic_tick, sync_sight)
                    .chain()
                    .run_if(any_with_component::<Heretic>)
                    .before(crate::health::HealthSystems)
                    .before(UiSystems::Open),
            )
            .add_systems(
                Update,
                fade_in_influences.run_if(any_with_component::<VisibleInfluence>),
            )
            .add_systems(
                Update,
                (
                    curse_prompts
                        .in_set(UiSystems::Handle)
                        .after(ss13_tgui::prompt::PromptSystems::Answer),
                    tick_heretic_curses.run_if(
                        |q: Query<
                            (),
                            Or<(With<ParalysisCurse>, With<CorrosionCurse>, With<RaceSwap>)>,
                        >| !q.is_empty(),
                    ),
                ),
            );
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers

pub(super) use crate::tools::tell_mob as chat_to;

fn icon(world: &World, dmi: &str, state: &str) -> Option<ss13_shared::IconStateId> {
    world
        .get_resource::<IconIndex>()
        .and_then(|index| index.get(dmi, state))
}

pub(super) use crate::tools::now;

pub fn is_heretic(world: &World, mob: Entity) -> bool {
    world.get::<Heretic>(mob).is_some()
}

pub(super) fn say(world: &mut World, mob: Entity, text: &str) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let name = name_of(world, mob);
    let clients = crate::chem::clients_near(world, pos, None);
    world.write_message(SendChat {
        clients,
        message: say_message(&name, text),
    });
}

pub(super) fn whisper(world: &mut World, mob: Entity, text: &str) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let name = name_of(world, mob);
    let clients = crate::obj_damage::clients_in_range(world, pos, 1, &[]);
    world.write_message(SendChat {
        clients,
        message: whisper_message(&name, text),
    });
}

fn can_act(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, mob).incapacitated
}

fn job_title(world: &World, mob: Entity) -> Option<String> {
    let job = world.get::<PlayerJob>(mob)?.0;
    Some(
        world
            .get_resource::<Jobs>()?
            .data
            .jobs
            .get(job)?
            .title
            .clone(),
    )
}

fn is_head(world: &World, mob: Entity) -> bool {
    job_title(world, mob).is_some_and(|t| HEADS_OF_STAFF.contains(&t.as_str()))
}

pub(super) fn hurt(world: &mut World, mob: Entity, kind: DamageType, amount: f32) {
    let (Some(mut health), Some(mut stat)) = (
        world.get::<Health>(mob).cloned(),
        world.get::<Stat>(mob).copied(),
    ) else {
        return;
    };
    let changed = apply_damage(&mut health, &mut stat, kind, amount);
    world.entity_mut(mob).insert(health);
    if changed.is_some() {
        world.entity_mut(mob).insert(stat);
    }
}

/// `adjust_*_loss(-amount)` of every kind, and stamina.
pub(super) fn heal(world: &mut World, mob: Entity, amount: f32) {
    if let Some(mut health) = world.get::<Health>(mob).cloned() {
        for kind in [
            DamageType::Brute,
            DamageType::Burn,
            DamageType::Tox,
            DamageType::Oxy,
        ] {
            health.adjust(kind, -amount);
        }
        let stat = world.get::<Stat>(mob).copied().unwrap_or_default();
        world.entity_mut(mob).insert(health.clone());
        if stat != Stat::Dead {
            let new = Stat::for_health(health.health());
            if new != stat {
                world.entity_mut(mob).insert(new);
            }
        }
    }
    crate::status_effects::adjust_stamina(world, mob, -amount);
}

pub(super) fn rand(world: &mut World, n: usize) -> usize {
    if n <= 1 {
        return 0;
    }
    world
        .get_resource_mut::<CombatRng>()
        .map_or(0, |mut r| r.range(0, n as u32 - 1) as usize)
}

fn pick_str(world: &mut World, list: &[&'static str]) -> &'static str {
    let i = rand(world, list.len());
    list.get(i).copied().unwrap_or_default()
}

fn area_name(world: &World, pos: TilePos) -> String {
    world
        .get_resource::<Areas>()
        .and_then(|a| a.area_at(pos))
        .and_then(|a| world.get::<DmName>(a))
        .map(|n| {
            n.name
                .trim_start_matches("\\improper ")
                .trim_start_matches("\\proper ")
                .to_owned()
        })
        .unwrap_or_default()
}

pub(super) fn hands(world: &World, mob: Entity) -> Vec<Entity> {
    world
        .get::<Inventory>(mob)
        .map(|i| i.hands.iter().flatten().copied().collect())
        .unwrap_or_default()
}

fn is_living(world: &World, e: Entity) -> bool {
    world.get::<Health>(e).is_some()
}

fn is_blade(world: &World, item: Entity) -> bool {
    path_of(world, item).is_some_and(|p| knowledge::is_type(&p, SICKLY_BLADE))
}

/// tg `IS_WRITING_UTENSIL`: pens and crayons.
fn is_writing_utensil(path: &str) -> bool {
    knowledge::is_type(path, "/obj/item/pen") || knowledge::is_type(path, "/obj/item/toy/crayon")
}

fn put_in_hands(world: &mut World, mob: Entity, item: Entity) -> bool {
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return false;
    };
    let hand = if inventory.hands[inventory.active].is_none() {
        Some(inventory.active)
    } else {
        inventory.hands.iter().position(Option::is_none)
    };
    let Some(hand) = hand else {
        return false;
    };
    world
        .entity_mut(item)
        .remove::<TilePos>()
        .insert(CarriedBy(mob));
    if let Some(mut inv) = world.get_mut::<Inventory>(mob) {
        inv.hands[hand] = Some(item);
    }
    true
}

fn despawn_held(world: &mut World, mob: Entity, item: Entity) {
    if let Some(mut inv) = world.get_mut::<Inventory>(mob) {
        for h in &mut inv.hands {
            if *h == Some(item) {
                *h = None;
            }
        }
    }
    if world.get_entity(item).is_ok() {
        world.despawn(item);
    }
}

fn despawn(world: &mut World, e: Entity) {
    if world.get_entity(e).is_ok() {
        let pos = world.get::<TilePos>(e).copied();
        world.despawn(e);
        if let Some(pos) = pos {
            crate::construction::tile_changed(world, pos);
        }
    }
}

#[cfg(test)]
mod tests;
