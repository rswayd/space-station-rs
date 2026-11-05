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
// shortcut: only the Path of Ash has effects. The other paths' knowledge is in the tree and can
// be researched (spells get a button that says it is not ported; recipes spawn their results
// when the prototype exists), but their marks, passives, blade upgrades and ascensions do
// nothing. Side knowledge has no effects beyond its ritual's results; summons (no basic mobs
// for heretic monsters yet), curses, the knowledge ritual, the focus, the heretic aura, dreams,
// x-ray near influences, rust healing and spell recharge by focus/holy water/passive are not
// ported. Ash Passage (jaunt) is not ported. No shadow realm: see [`begin_sacrifice`].
// The radials are list windows; balloon alerts are chat warnings.

use bevy::ecs::message::MessageCursor;

use bevy::prelude::*;
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
    Drain { points: u32 },
    FireBlast,
}

pub struct HereticPlugin;

impl Plugin for HereticPlugin {
    fn build(&self, app: &mut App) {
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
            );
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers

fn chat_to(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

fn icon(world: &World, dmi: &str, state: &str) -> Option<ss13_shared::IconStateId> {
    world
        .get_resource::<IconIndex>()
        .and_then(|index| index.get(dmi, state))
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

pub fn is_heretic(world: &World, mob: Entity) -> bool {
    world.get::<Heretic>(mob).is_some()
}

fn say(world: &mut World, mob: Entity, text: &str) {
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

fn whisper(world: &mut World, mob: Entity, text: &str) {
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

fn hurt(world: &mut World, mob: Entity, kind: DamageType, amount: f32) {
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
fn heal(world: &mut World, mob: Entity, amount: f32) {
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

fn rand(world: &mut World, n: usize) -> usize {
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

fn hands(world: &World, mob: Entity) -> Vec<Entity> {
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

// ---------------------------------------------------------------------------------------------
// Gaining the heretic

/// `add_antag_datum(/datum/antagonist/heretic)`. `false` if it already is one or cannot be.
pub fn add_heretic(world: &mut World, mob: Entity) -> bool {
    if is_heretic(world, mob) || world.get::<Health>(mob).is_none() {
        return false;
    }
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let hud_icon = icon(world, ANTAG_HUD_DMI, "heretic");
    let t = now(world);
    let paths = static_info(world.get_resource::<IconIndex>());
    world.entity_mut(mob).insert((
        Heretic::new(key, t),
        HereticPaths(paths),
        AntagHudData {
            hud_icon,
            ..default()
        },
    ));
    // `on_gain`: the start knowledge for free.
    let starts: Vec<&'static str> = knowledge::start_knowledge().map(|k| k.path).collect();
    for path in starts {
        gain_knowledge(world, mob, path, Category::Start);
    }
    // `apply_innate_effects`: `add_tracked_mind`, new influences for an on-station heretic.
    world.resource_mut::<RealityTracker>().tracked.push(mob);
    generate_influences(world);
    if let Some(client) = client {
        crate::hallucination::see_groups(world, client, HERETIC_SIGHT, true);
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
    }
    greet(world, mob);
    refresh(world, mob);
    info!("{mob} became a heretic");
    true
}

/// `greet()` and `announce_objectives`, then the window.
fn greet(world: &mut World, mob: Entity) {
    chat_to(world, mob, Span::Bold, format!("Вы - {HERETIC_NAME}."));
    announce_objectives(world, mob);
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        world.write_message(OpenUi::new::<AntagInfoHereticUi>(
            client,
            mob,
            mob,
            HERETIC_NAME,
        ));
    }
}

fn announce_objectives(world: &mut World, mob: Entity) {
    let Some(lines) = world.get::<Heretic>(mob).map(|h| {
        h.objectives
            .iter()
            .enumerate()
            .map(|(i, o)| format!("{OBJECTIVE_NAME} #{}: {}", i + 1, o.explanation()))
            .collect::<Vec<_>>()
    }) else {
        return;
    };
    chat_to(world, mob, Span::Notice, "Ваши текущие задачи:");
    for line in lines {
        chat_to(world, mob, Span::Plain, line);
    }
}

/// `gain_knowledge`: the entry moves from its shop to the researched knowledge, then
/// `on_research`.
fn gain_knowledge(world: &mut World, mob: Entity, path: &'static str, category: Category) -> bool {
    let Some(mut heretic) = world.get_mut::<Heretic>(mob) else {
        return false;
    };
    if heretic.has(path) {
        return false;
    }
    let shop = heretic.shop_mut(category);
    let entry = match shop.iter().position(|e| e.path == path) {
        Some(i) => shop.remove(i),
        None => Entry::new(path, None, category, 1, None),
    };
    let mut entry = entry;
    if entry.purchased_depth != 0 {
        entry.depth = entry.purchased_depth;
    }
    let def = entry.def();
    heretic.researched.push(Researched {
        entry,
        charges: def.max_charges,
        created: Vec::new(),
    });
    on_research(world, mob, def);
    true
}

/// `purchase_knowledge`: the cost, then [`gain_knowledge`].
fn purchase(world: &mut World, mob: Entity, path: &'static str, category: Category) -> bool {
    let Some(cost) = world.get::<Heretic>(mob).and_then(|h| {
        h.shop_of(category)
            .iter()
            .find(|e| e.path == path)
            .map(|e| e.cost)
    }) else {
        return false;
    };
    if world.get::<Heretic>(mob).is_none_or(|h| cost > h.points) {
        return false;
    }
    if !gain_knowledge(world, mob, path, category) {
        return false;
    }
    if let Some(mut h) = world.get_mut::<Heretic>(mob) {
        h.adjust_points(-(cost as i32));
    }
    info!("heretic {mob} gained knowledge: {path}");
    true
}

/// `on_research` of each knowledge family.
fn on_research(world: &mut World, mob: Entity, def: &'static KnowledgeDef) {
    if !def.gain_text.is_empty() {
        chat_to(world, mob, Span::Warning, def.gain_text);
    }
    match def.path {
        LIVING_HEART => {
            if let Some(mut h) = world.get_mut::<Heretic>(mob) {
                h.living_heart = true;
            }
        }
        HUNT_AND_SACRIFICE => {
            obtain_targets(world, mob, true);
        }
        _ => {}
    }
    if def.kind == Kind::Starting {
        pick_path(world, mob, def);
    }
    if def.is_final {
        disable_blade_breaking(world, mob);
    }
    refresh(world, mob);
}

/// `limited_amount/starting/on_research`: the path, its tree, drafts and shop, the objectives.
fn pick_path(world: &mut World, mob: Entity, def: &'static KnowledgeDef) {
    let Some(column) = knowledge::path_of_start(def.path) else {
        chat_to(
            world,
            mob,
            Span::Warning,
            "Вот чёрт! Что-то сломалось, путь не найден!",
        );
        return;
    };
    let mut tree = knowledge::path_tree(column);
    let mut picks: Vec<u32> = Vec::new();
    for _ in 0..64 {
        let v = world
            .get_resource_mut::<CombatRng>()
            .map_or(0, |mut r| r.range(0, u32::MAX - 1));
        picks.push(v);
    }
    let mut i = 0;
    let (shop, draft) = knowledge::drafts(column, &mut tree, |n| {
        let v = picks[i % picks.len()] as usize;
        i += 1;
        if n == 0 { 0 } else { v % n }
    });
    let extra = world
        .get_resource_mut::<CombatRng>()
        .map_or(2, |mut r| r.range(2, 4));
    let heads = alive_heads(world);
    let Some(mut h) = world.get_mut::<Heretic>(mob) else {
        return;
    };
    h.route = Some(column.route);
    let target = knowledge::research_target(tree.len(), draft.len(), extra);
    h.tree = tree;
    h.shop = shop;
    h.draft = draft;
    // `forge_primary_objectives`.
    h.objectives = vec![
        HereticObjective::Research { target },
        HereticObjective::MinorSacrifice {
            target: if heads < 2 { 5 } else { 4 },
        },
    ];
    if heads >= 2 {
        h.objectives.push(HereticObjective::MajorSacrifice);
    }
    info!("heretic {mob} took the {}", column.route);
    announce_objectives(world, mob);
}

fn alive_heads(world: &mut World) -> u32 {
    let mut q = world.query_filtered::<(Entity, &Stat), With<PlayerMob>>();
    let alive: Vec<Entity> = q
        .iter(world)
        .filter(|(_, s)| **s != Stat::Dead)
        .map(|(e, _)| e)
        .collect();
    alive.into_iter().filter(|&e| is_head(world, e)).count() as u32
}

/// `disable_blade_breaking`.
fn disable_blade_breaking(world: &mut World, mob: Entity) {
    let Some(mut h) = world.get_mut::<Heretic>(mob) else {
        return;
    };
    if h.unlimited_blades {
        return;
    }
    h.unlimited_blades = true;
    chat_to(
        world,
        mob,
        Span::BoldWarning,
        "Вы обрели значительное могущество, Мансус больше не позволит вам ломать свои клинки, но \
         теперь вы можете сделать их столько, сколько пожелаете.",
    );
}

fn adjust_points(world: &mut World, mob: Entity, amount: i32) {
    let crossed = world
        .get_mut::<Heretic>(mob)
        .is_some_and(|mut h| h.adjust_points(amount));
    if crossed {
        disable_blade_breaking(world, mob);
    }
}

/// `COMSIG_HERETIC_INFLUENCE_DRAINED` and `COMSIG_HERETIC_SACRIFICE` on the basic knowledge:
/// the grasp is fully recharged.
fn recharge_grasp(world: &mut World, mob: Entity) {
    if let Some(mut h) = world.get_mut::<Heretic>(mob)
        && let Some(r) = h.get_mut(BASIC)
    {
        r.charges = r.def().max_charges;
    }
    refresh(world, mob);
}

// ---------------------------------------------------------------------------------------------
// Influences

/// `generate_new_influences`: `max(5 - n + 1, 1)` for the n-th tracked heretic, minus what
/// exists or was drained; at least a tile between two.
pub fn generate_influences(world: &mut World) {
    let tracker = world.resource::<RealityTracker>().clone();
    let wanted: u32 = (1..=tracker.tracked.len() as u32)
        .map(|n| (NUM_INFLUENCES_PER_HERETIC + 1).saturating_sub(n).max(1))
        .sum();
    let mut sanity = 0;
    while (world.resource::<RealityTracker>().smashes.len() as u32
        + world.resource::<RealityTracker>().num_drained)
        < wanted
        && sanity < 100
    {
        let Some(pos) = crate::events::electrical_storm::random_station_turf(world) else {
            break;
        };
        let near = {
            let index = world.resource::<ss13_world::SpatialIndex>();
            (-1..=1).any(|dx| {
                (-1..=1).any(|dy| {
                    index
                        .on_tile(TilePos::new(pos.x + dx, pos.y + dy, pos.z))
                        .iter()
                        .any(|&e| {
                            world.get::<Influence>(e).is_some()
                                || world.get::<VisibleInfluence>(e).is_some()
                        })
                })
            })
        };
        if near {
            sanity += 1;
            continue;
        }
        if spawn_influence(world, pos).is_none() {
            break;
        }
    }
}

/// A new `/obj/effect/heretic_influence`: its `real_icon_state` image for heretics and ghosts.
pub fn spawn_influence(world: &mut World, pos: TilePos) -> Option<Entity> {
    let e = crate::construction::spawn_obj(world, INFLUENCE, pos)?;
    let name = format!(
        "{} {}",
        pick_str(world, INFLUENCE_PREFIX),
        pick_str(world, INFLUENCE_POSTFIX)
    );
    let state = icon(world, ELDRITCH_DMI, "reality_smash");
    let mut entity = world.entity_mut(e);
    entity.insert((
        Influence::default(),
        ss13_net::server::Networked,
        Invisibility(crate::ghost::INVISIBILITY_OBSERVER),
        VisibleToGroups(HERETIC_SIGHT),
        Name::new(name.clone()),
    ));
    if let Some(mut n) = entity.get_mut::<DmName>() {
        n.name = name;
    }
    // tg draws it only as the heretics' image (`real_icon_state`); its own `icon_state` is null.
    if let Some(state) = state {
        match entity.get_mut::<ss13_shared::Appearance>() {
            Some(mut appearance) => appearance.icon_state = state,
            None => {
                entity.insert(ss13_shared::Appearance {
                    icon_state: state,
                    // `OBJ_LAYER` on `GAME_PLANE`.
                    layer: ss13_shared::Layer(3.0),
                    plane: -4,
                    color: None,
                    pixel_offset: [0, 0],
                    rotation: 0,
                    overlays: Vec::new(),
                    mouse_opacity: ss13_shared::MouseOpacity::Icon,
                });
            }
        }
    }
    world.resource_mut::<RealityTracker>().smashes.push(e);
    Some(e)
}

/// Starts draining an influence (`drain_influence`): `points` knowledge in `delay_ds`.
fn start_drain(
    world: &mut World,
    client: Entity,
    mob: Entity,
    influence: Entity,
    points: u32,
    delay_ds: f32,
) {
    if world
        .get::<Influence>(influence)
        .is_some_and(|i| i.being_drained)
    {
        tell(world, client, Span::Warning, "уже добывается!");
        return;
    }
    if let Some(mut i) = world.get_mut::<Influence>(influence) {
        i.being_drained = true;
    }
    tell(world, client, Span::Notice, "добыча влияния...");
    let item = world.get::<Inventory>(mob).and_then(Inventory::active_item);
    let actor = Actor { client, mob, item };
    if !do_after(
        world,
        actor,
        influence,
        delay_ds,
        0,
        Job::Heretic(HereticJob::Drain { points }),
    ) && let Some(mut i) = world.get_mut::<Influence>(influence)
    {
        i.being_drained = false;
    }
}

fn finish_drain(world: &mut World, mob: Entity, action: &DoAfter, points: u32) {
    let influence = action.target;
    if world.get::<Influence>(influence).is_none() || !is_heretic(world, mob) {
        return;
    }
    tell(world, action.client, Span::Notice, "влияние добыто");
    adjust_points(world, mob, points as i32);
    recharge_grasp(world, mob);
    let drain = pick_str(world, INFLUENCE_DRAIN_MESSAGE);
    chat_to(world, mob, Span::Hypnophrase, drain);
    let name = name_of(world, influence);
    chat_to(
        world,
        mob,
        Span::Warning,
        format!("{} начинает проясняться в реальность!", capitalize(&name)),
    );
    let pos = world.get::<TilePos>(influence).copied();
    {
        let mut tracker = world.resource_mut::<RealityTracker>();
        tracker.smashes.retain(|&e| e != influence);
        tracker.num_drained += 1;
    }
    despawn(world, influence);
    if let Some(pos) = pos
        && let Some(visible) = crate::construction::spawn_obj(world, VISIBLE_INFLUENCE, pos)
    {
        let drained = pick_str(world, INFLUENCE_DRAINED);
        let full = format!("{drained} {name}");
        let born = now(world);
        let mut e = world.entity_mut(visible);
        e.insert((VisibleInfluence { born }, Name::new(full.clone())));
        if let Some(mut n) = e.get_mut::<DmName>() {
            n.name = full;
        }
        if let Some(mut appearance) = e.get_mut::<ss13_shared::Appearance>() {
            appearance.color = Some([255, 255, 255, 0]);
        }
    }
    update_ui(world, mob);
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// `show_presence`: `alpha` 0 for 15 s, then up to 255 over 15 s.
fn fade_in_influences(
    time: Res<Time>,
    mut q: Query<(&VisibleInfluence, &mut ss13_shared::Appearance)>,
) {
    let t = time.elapsed_secs_f64();
    for (v, mut appearance) in &mut q {
        // Steps of 1/15 so the appearance changes once a second, not every tick.
        let alpha = (((t - v.born - 15.0) / 15.0).clamp(0.0, 1.0) * 15.0).floor() / 15.0;
        let color = if alpha >= 1.0 {
            None
        } else {
            Some([255, 255, 255, (alpha * 255.0) as u8])
        };
        if appearance.color != color {
            appearance.color = color;
        }
    }
}

/// Clients whose body is a heretic see [`HERETIC_SIGHT`]; others lose it (ghosts see influences
/// anyway, by their `see_invisible`).
fn sync_sight(world: &mut World) {
    let mut heretics = world.query_filtered::<&PlayerMob, With<Heretic>>();
    let clients: Vec<Entity> = heretics.iter(world).map(|p| p.client).collect();
    for client in clients {
        if world
            .get::<SeeGroups>(client)
            .is_none_or(|g| g.0 & HERETIC_SIGHT == 0)
        {
            crate::hallucination::see_groups(world, client, HERETIC_SIGHT, true);
        }
    }
    // Other bits are hallucination images (`crate::hallucination`): only this one is ours.
    let mut seeing = world.query::<(Entity, &SeeGroups)>();
    let seeing: Vec<Entity> = seeing
        .iter(world)
        .filter(|(_, g)| g.0 & HERETIC_SIGHT != 0)
        .map(|(e, _)| e)
        .collect();
    for client in seeing {
        let mob = world.get_resource::<Players>().and_then(|p| p.mob(client));
        let keep = mob.is_some_and(|m| is_heretic(world, m) || world.get::<Ghost>(m).is_some());
        if !keep {
            crate::hallucination::see_groups(world, client, HERETIC_SIGHT, false);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Clicks and items

/// An empty hand on a heretic thing (`attack_hand` / `attack_hand_secondary`).
pub fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    right: bool,
) -> bool {
    if world.get::<Influence>(target).is_some() {
        if !is_heretic(world, mob) {
            return false;
        }
        if right {
            start_drain(world, client, mob, target, 1, DRAIN_DS);
        }
        return true;
    }
    if world.get::<VisibleInfluence>(target).is_some() {
        if is_heretic(world, mob) {
            tell(
                world,
                client,
                Span::BoldWarning,
                "Вам лучше не искушать неподвластные вам силы!",
            );
        } else {
            // shortcut: no dismemberment yet (25% in tg).
            tell(
                world,
                client,
                Span::Danger,
                "Вы отдёргиваете руку от отверстия, когда мистическая энергия бьется, пытаясь \
                 зацепиться за этот мир!",
            );
        }
        return true;
    }
    if world.get::<TransmutationRune>(target).is_some() {
        if !is_heretic(world, mob) {
            return false;
        }
        if world
            .get::<TransmutationRune>(target)
            .is_some_and(|r| r.in_use)
        {
            return true;
        }
        try_rituals(world, client, mob, target);
        return true;
    }
    false
}

/// A held item used on a target: a writing tool on a floor (the rune), the codex on an
/// influence, the grasp on a rune (erasing it).
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<MansusFist>(held).is_some() {
        if world.get::<TransmutationRune>(target).is_some() {
            // `effect_remover` (0.4 s): the rune is gone, the hand used without a refund.
            let name = name_of(world, target);
            tell(world, client, Span::Notice, format!("Вы убираете {name}."));
            despawn(world, target);
            despawn_held(world, mob, held);
            let t = now(world);
            if let Some(mut h) = world.get_mut::<Heretic>(mob) {
                h.set_cooldown(&spell_id(BASIC), t + GRASP_COOLDOWN);
            }
            refresh(world, mob);
            return true;
        }
        // Living targets are the melee click's (`heretic_clicks`).
        return !is_living(world, target);
    }
    let Some(path) = path_of(world, held) else {
        return false;
    };
    if knowledge::is_type(&path, CODEX) && world.get::<Influence>(target).is_some() {
        if is_heretic(world, mob) {
            start_drain(world, client, mob, target, 2, CODEX_DRAIN_DS);
            return true;
        }
        return false;
    }
    if is_writing_utensil(&path) && is_heretic(world, mob) {
        let Some(&pos) = world.get::<TilePos>(target) else {
            return false;
        };
        if crate::construction::turf_at(world, pos) != Some(target) {
            return false;
        }
        if !grasp_in_offhand(world, mob) {
            return false;
        }
        try_draw_rune(world, client, mob, held, target, pos);
        return true;
    }
    false
}

/// `attack_self`: a sickly blade breaks to teleport the heretic.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world.get::<MansusFist>(item).is_some() {
        return true;
    }
    if !is_blade(world, item) {
        return false;
    }
    if world
        .get::<Heretic>(mob)
        .is_some_and(|h| h.unlimited_blades)
    {
        return true;
    }
    // `seek_safety`.
    let name = name_of(world, item);
    let usable = is_heretic(world, mob);
    let dest = crate::events::electrical_storm::random_station_turf(world);
    match (usable, dest) {
        (true, Some(dest)) => {
            world.entity_mut(mob).insert(dest);
            tell(
                world,
                client,
                Span::Warning,
                format!(
                    "Когда вы разбиваете {name}, вы чувствуете, как порыв энергии проходит через \
                     ваше тело. {}",
                    blade_use_message(&path_of(world, item).unwrap_or_default())
                ),
            );
        }
        (true, None) => tell(
            world,
            client,
            Span::Warning,
            format!("Вы разбиваете {name}, но ваша мольба остается без ответа."),
        ),
        (false, _) => tell(
            world,
            client,
            Span::Warning,
            format!("Вы разбиваете {name}."),
        ),
    }
    despawn_held(world, mob, item);
    true
}

/// `after_use_message` of the blades.
fn blade_use_message(path: &str) -> &'static str {
    match path.rsplit('/').next().unwrap_or_default() {
        "rust" => "Ржавые холмы слышат ваш зов...",
        "ash" => "Ночной дозорный слышит ваш зов...",
        "flesh" => "Маршал слышит ваш зов...",
        _ => "",
    }
}

/// The grasp in the inactive hand (`check_mansus_grasp_offhand`).
pub fn grasp_in_offhand(world: &World, mob: Entity) -> bool {
    world.get::<Inventory>(mob).is_some_and(|i| {
        i.hands[1 - i.active].is_some_and(|h| world.get::<MansusFist>(h).is_some())
    })
}

/// The grasp or a sickly blade on a mob, before melee.
fn heretic_clicks(
    world: &mut World,
    mut clicks: Local<MessageCursor<crate::click::ClickResolved>>,
) {
    let clicks: Vec<crate::click::ClickResolved> = clicks
        .read(world.resource::<Messages<crate::click::ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        let (mob, target) = (click.mob, click.target);
        if world
            .resource::<crate::security::MeleeHandled>()
            .0
            .contains(&mob)
            || !is_living(world, target)
        {
            continue;
        }
        let Some(held) = world.get::<Inventory>(mob).and_then(Inventory::active_item) else {
            continue;
        };
        let t = now(world);
        if world.get::<NextMove>(mob).is_some_and(|n| t < n.0) {
            continue;
        }
        if world.get::<MansusFist>(held).is_some() {
            world
                .resource_mut::<crate::security::MeleeHandled>()
                .0
                .insert(mob);
            grasp_hit(world, mob, held, target);
            continue;
        }
        if is_blade(world, held) {
            if !is_heretic(world, mob) {
                // `pre_attack` of a non-heretic.
                world
                    .resource_mut::<crate::security::MeleeHandled>()
                    .0
                    .insert(mob);
                chat_to(
                    world,
                    mob,
                    Span::Danger,
                    "Ты чувствуешь, как импульс чужого интеллекта бьет по твоему разуму!",
                );
                crate::status_effects::apply(world, mob, Incap::Paralyze, 5.0);
            } else {
                world
                    .resource_mut::<PendingBladeHits>()
                    .0
                    .push((mob, target, t));
            }
        }
    }
}

/// `afterattack` of the sickly blade: the melee landed (it set the attacker's `next_move`).
fn after_melee(world: &mut World) {
    let pending = std::mem::take(&mut world.resource_mut::<PendingBladeHits>().0);
    for (mob, target, at) in pending {
        if !world.get::<NextMove>(mob).is_some_and(|n| n.0 > at) {
            continue;
        }
        blade_attack(world, mob, target);
    }
}

/// `COMSIG_HERETIC_BLADE_ATTACK`: the starting knowledge triggers the mark, the blade upgrade
/// adds its effect.
fn blade_attack(world: &mut World, mob: Entity, target: Entity) {
    let Some(h) = world.get::<Heretic>(mob) else {
        return;
    };
    let (ash, upgrade) = (h.has(ASH_START), h.has(ASH_BLADE_UPGRADE));
    if ash && trigger_ash_mark(world, target) {
        // `base_ash/trigger_mark`: 75% of the grasp's cooldown back.
        if let Some(mut h) = world.get_mut::<Heretic>(mob) {
            let id = spell_id(BASIC);
            let ready = h.cooldown(&id) - GRASP_COOLDOWN * 0.75;
            h.set_cooldown(&id, ready);
        }
        refresh(world, mob);
    }
    if upgrade && mob != target {
        // `blade_upgrade/ash`.
        crate::fire::adjust_fire_stacks(world, target, 1.0);
        crate::fire::ignite_mob(world, target);
    }
}

/// `/datum/status_effect/eldritch/ash/on_effect`: stamina and burn by repetitions, the mark
/// jumps to a carbon next to it with one repetition less.
fn trigger_ash_mark(world: &mut World, target: Entity) -> bool {
    let t = now(world);
    let Some(mark) = world
        .get::<AshMark>(target)
        .copied()
        .filter(|m| m.until > t)
    else {
        return false;
    };
    world.entity_mut(target).remove::<AshMark>();
    let reps = mark.repetitions as f32;
    crate::status_effects::adjust_stamina(world, target, 6.0 * reps);
    hurt(world, target, DamageType::Burn, 3.0 * reps);
    let Some(&pos) = world.get::<TilePos>(target) else {
        return true;
    };
    let mut victims: Vec<Entity> = Vec::new();
    {
        let index = world.resource::<ss13_world::SpatialIndex>();
        for dx in -1..=1 {
            for dy in -1..=1 {
                for &e in index.on_tile(TilePos::new(pos.x + dx, pos.y + dy, pos.z)) {
                    if e != target && world.get::<HumanLook>(e).is_some() {
                        victims.push(e);
                    }
                }
            }
        }
    }
    victims.retain(|&v| !is_heretic(world, v));
    if !victims.is_empty() {
        let v = victims[rand(world, victims.len())];
        world.entity_mut(v).insert(AshMark {
            until: t + MARK_SECS,
            repetitions: mark.repetitions.saturating_sub(1).max(1),
        });
    }
    true
}

/// `mansus_grasp/cast_on_hand_hit` and the spell's after-cast: the invocation, the cooldown, a
/// charge, the hand gone.
fn grasp_hit(world: &mut World, mob: Entity, hand: Entity, target: Entity) {
    let Some(owner) = world.get::<MansusFist>(hand).map(|f| f.owner) else {
        return;
    };
    if owner != mob || !is_heretic(world, mob) {
        despawn_held(world, mob, hand);
        return;
    }
    say(world, mob, "R'CH T'H TR'TH!");
    // `COMSIG_HERETIC_MANSUS_GRASP_ATTACK`: the starting knowledge's mark.
    let ash = world.get::<Heretic>(mob).is_some_and(|h| h.has(ASH_START));
    let t = now(world);
    if ash && world.get::<Stat>(target) != Some(&Stat::Dead) {
        world.entity_mut(target).insert(AshMark {
            until: t + MARK_SECS,
            repetitions: 5,
        });
    }
    // `base_ash/on_mansus_grasp`: the eyes burn unless blind or missing.
    if ash
        && !crate::vision::is_blind(world, target)
        && crate::medical::organs::organ(world, target, crate::medical::organs::Slot::Eyes)
            .is_some()
    {
        chat_to(
            world,
            target,
            Span::Danger,
            "Яркий зеленый свет ужасно жжет глаза!",
        );
        crate::medical::organs::adjust_organ_loss(
            world,
            target,
            crate::medical::organs::Slot::Eyes,
            15.0,
        );
        crate::vision::set_eye_blur_if_lower(world, target, 20.0);
    }
    hurt(world, target, DamageType::Brute, 10.0);
    if world.get::<HumanLook>(target).is_some() {
        if crate::antag::cult::server::is_cultist(world, target) {
            crate::status_effects::knockdown(world, target, 0.5, 0.0);
            if let Some(mut e) = effects_mut(world, target) {
                e.confusion = (e.confusion + 1.5).min(3.0).max(e.confusion);
            }
            let victim = name_of(world, target);
            let user = name_of(world, mob);
            chat_to(
                world,
                mob,
                Span::Warning,
                format!(
                    "Нечестивая сила вмешивается, когда вы наносите {victim}, поглощая большую \
                     часть эффекта!"
                ),
            );
            chat_to(
                world,
                target,
                Span::Warning,
                format!(
                    "Когда {user} хватает вас потусторонними силами, ваша кровавая магия \
                     поглощает большую часть эффектов!"
                ),
            );
        } else {
            crate::status_effects::knockdown(world, target, 5.0, 3.0);
            crate::status_effects::adjust_timed(
                world,
                target,
                crate::status_effects::Timed::Slurring(crate::status_effects::SlurKind::Heretic),
                4.0,
                None,
            );
            crate::status_effects::adjust_stamina(world, target, 80.0);
        }
    }
    despawn_held(world, mob, hand);
    spend_charge(world, mob, BASIC);
    if let Some(mut h) = world.get_mut::<Heretic>(mob) {
        h.set_cooldown(&spell_id(BASIC), t + GRASP_COOLDOWN);
    }
    refresh(world, mob);
}

fn spend_charge(world: &mut World, mob: Entity, path: &str) {
    if let Some(mut h) = world.get_mut::<Heretic>(mob) {
        if h.ascended {
            return;
        }
        if let Some(r) = h.get_mut(path) {
            r.charges = r.charges.saturating_sub(1);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The transmutation rune

/// `try_draw_rune`: open, non-space floor around, no rune within 3 tiles, not already drawing.
fn try_draw_rune(
    world: &mut World,
    client: Entity,
    mob: Entity,
    pen: Entity,
    turf: Entity,
    pos: TilePos,
) {
    for dx in -1..=1 {
        for dy in -1..=1 {
            let p = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
            let ok = crate::construction::turf_at(world, p)
                .and_then(|t| path_of(world, t))
                .is_some_and(|t| {
                    knowledge::is_type(&t, "/turf/open")
                        && ![
                            "/turf/open/space",
                            "/turf/open/openspace",
                            "/turf/open/lava",
                            "/turf/open/chasm",
                        ]
                        .iter()
                        .any(|b| knowledge::is_type(&t, b))
                });
            if !ok {
                tell(world, client, Span::Warning, "плохое место для руны!");
                return;
            }
        }
    }
    let near = {
        let index = world.resource::<ss13_world::SpatialIndex>();
        index
            .in_rect(pos.z, (pos.x - 3, pos.y - 3), (pos.x + 3, pos.y + 3))
            .any(|e| world.get::<TransmutationRune>(e).is_some())
    };
    if near {
        tell(
            world,
            client,
            Span::Warning,
            "слишком близко к другой руне!",
        );
        return;
    }
    if world.get::<Heretic>(mob).is_some_and(|h| h.drawing_rune) {
        tell(world, client, Span::Warning, "уже рисуете руну!");
        return;
    }
    tell(world, client, Span::Notice, "создание руны...");
    let actor = Actor {
        client,
        mob,
        item: Some(pen),
    };
    if do_after(
        world,
        actor,
        turf,
        RUNE_DRAW_DS,
        0,
        Job::Heretic(HereticJob::DrawRune),
    ) && let Some(mut h) = world.get_mut::<Heretic>(mob)
    {
        h.drawing_rune = true;
    }
}

/// [`Job::Heretic`] completions.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: HereticJob) {
    match job {
        HereticJob::DrawRune => {
            if let Some(mut h) = world.get_mut::<Heretic>(mob) {
                h.drawing_rune = false;
            }
            if !is_heretic(world, mob) || !grasp_in_offhand(world, mob) {
                return;
            }
            let route = world.get::<Heretic>(mob).and_then(|h| h.route);
            spawn_rune(world, action.target_pos, route);
            tell(world, action.client, Span::Notice, "руна создана");
        }
        HereticJob::Drain { points } => finish_drain(world, mob, action, points),
        HereticJob::FireBlast => cast_fire_blast(world, mob),
    }
}

/// An interrupted job: tg resets `drawing_rune` and `being_drained`.
pub fn interrupted(world: &mut World, mob: Entity, action: &DoAfter, job: HereticJob) {
    match job {
        HereticJob::DrawRune => {
            if let Some(mut h) = world.get_mut::<Heretic>(mob) {
                h.drawing_rune = false;
            }
        }
        HereticJob::Drain { .. } => {
            if let Some(mut i) = world.get_mut::<Influence>(action.target) {
                i.being_drained = false;
            }
        }
        HereticJob::FireBlast => return,
    }
    tell(world, action.client, Span::Warning, "прервано!");
}

/// `new /obj/effect/heretic_rune/big(turf, path colour)`.
pub fn spawn_rune(world: &mut World, pos: TilePos, route: Option<&str>) -> Option<Entity> {
    let rune = crate::construction::spawn_obj(world, RUNE, pos)?;
    let [r, g, b] = path_color(route);
    let mut e = world.entity_mut(rune);
    e.insert(TransmutationRune::default());
    if let Some(mut appearance) = e.get_mut::<ss13_shared::Appearance>() {
        appearance.color = Some([r, g, b, 255]);
        // `pixel_x = -30`, `pixel_y = 18`, `pixel_z = -48`.
        appearance.pixel_offset = [-30, -30];
    }
    Some(rune)
}

/// An admin's or a test's spawn: a rune gets its state like a drawn one.
pub fn spawn_obj_or_rune(world: &mut World, path: &str, pos: TilePos) -> Option<Entity> {
    if knowledge::is_type(path, "/obj/effect/heretic_rune") {
        return spawn_rune(world, pos, None);
    }
    if path == INFLUENCE {
        return spawn_influence(world, pos);
    }
    crate::antag::cult::server::spawn_obj_or_rune(world, path, pos)
}

/// `can_be_invoked` of a researched knowledge.
fn can_be_invoked(heretic: &Heretic, r: &Researched) -> bool {
    let def = r.def();
    match def.kind {
        _ if def.path == FEAST_OF_OWLS => !heretic.feast_of_owls,
        _ if def.path == LIVING_HEART => !heretic.living_heart,
        Kind::Spell => !def.required.is_empty() && r.charges < def.max_charges,
        Kind::Ultimate => !heretic.ascended && heretic.can_ascend().is_ok(),
        _ => !def.required.is_empty(),
    }
}

/// `get_rituals`, by priority.
fn rituals(heretic: &Heretic) -> Vec<&'static KnowledgeDef> {
    let mut out: Vec<&'static KnowledgeDef> = heretic
        .researched
        .iter()
        .filter(|r| can_be_invoked(heretic, r))
        .map(Researched::def)
        .collect();
    out.sort_by_key(|k| std::cmp::Reverse(k.priority));
    out
}

// shortcut: the type's last path segment, not its `initial(name)` (no prototype name lookup).
fn type_name(_: &World, path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).replace('_', " ")
}

fn requirements_text(world: &World, def: &KnowledgeDef) -> String {
    let parts: Vec<String> = def
        .required
        .iter()
        .map(|(types, n)| {
            types
                .iter()
                .map(|t| knowledge::describe_required(t, *n, |p| type_name(world, p)))
                .collect::<Vec<_>>()
                .join(" or ")
        })
        .collect();
    english_list(&parts)
}

fn english_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} и {last}", rest.join(", ")),
    }
}

/// `try_rituals`: the list of rituals (tg's radial).
fn try_rituals(world: &mut World, client: Entity, mob: Entity, rune: Entity) {
    let Some(heretic) = world.get::<Heretic>(mob) else {
        return;
    };
    let list = rituals(heretic);
    if list.is_empty() {
        tell(world, client, Span::Warning, "нет доступных ритуалов!");
        return;
    }
    let items: Vec<String> = list.iter().map(|k| k.name.to_owned()).collect();
    let message = list
        .iter()
        .filter(|k| !k.required.is_empty())
        .map(|k| {
            let needs = if k.transmute_text.is_empty() {
                format!("Требуется: {}", requirements_text(world, k))
            } else {
                k.transmute_text.to_owned()
            };
            format!("{}: {needs}", k.name)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let paths = list.iter().map(|k| k.path).collect();
    if let Some(mut r) = world.get_mut::<TransmutationRune>(rune) {
        r.in_use = true;
    }
    open_prompt(
        world,
        client,
        mob,
        HereticPrompt {
            kind: PromptKind::Ritual { rune },
            title: "Ритуалы".into(),
            message,
            items,
            paths,
            targets: Vec::new(),
        },
    );
}

/// The atoms a ritual may use: what stands on and around the rune, visible, not the user
/// (`do_ritual`'s `range(1, src)`).
fn atoms_near(world: &World, rune: Entity, user: Entity) -> Vec<Candidate<Entity>> {
    let Some(&pos) = world.get::<TilePos>(rune) else {
        return Vec::new();
    };
    let index = world.resource::<ss13_world::SpatialIndex>();
    let mut out = Vec::new();
    for dx in -1..=1 {
        for dy in -1..=1 {
            for &e in index.on_tile(TilePos::new(pos.x + dx, pos.y + dy, pos.z)) {
                if e == user || e == rune || world.get::<Invisibility>(e).is_some_and(|i| i.0 > 0) {
                    continue;
                }
                if crate::construction::turf_at(world, TilePos::new(pos.x + dx, pos.y + dy, pos.z))
                    == Some(e)
                {
                    continue;
                }
                let path = if is_living(world, e) && world.get::<HumanLook>(e).is_some() {
                    Some("/mob/living/carbon/human".to_owned())
                } else {
                    path_of(world, e).or_else(|| {
                        world
                            .get::<ss13_mapload::DmCleanable>(e)
                            .map(|c| c.path.clone())
                    })
                };
                let Some(path) = path else {
                    continue;
                };
                let stack = world.get::<crate::tools::Stack>(e).map(|s| s.amount);
                out.push(Candidate {
                    thing: e,
                    path,
                    stack,
                });
            }
        }
    }
    out
}

/// `do_ritual`: the snowflake check, the matching, the ritual, the cleanup.
fn do_ritual(world: &mut World, client: Entity, mob: Entity, rune: Entity, path: &'static str) {
    let Some(def) = knowledge::def(path) else {
        return;
    };
    let mut atoms = atoms_near(world, rune, mob);
    let loc = world.get::<TilePos>(rune).copied();
    let Some(loc) = loc else {
        return;
    };
    if !snowflake_check(world, client, mob, def, &mut atoms) {
        return;
    }
    let matched = match knowledge::match_recipe(def.required, &atoms) {
        Ok(m) => m,
        Err(missing) => {
            tell(
                world,
                client,
                Span::Warning,
                "ритуал провален, нехватает компонентов!",
            );
            let parts: Vec<String> = missing
                .iter()
                .map(|&(i, n)| {
                    def.required[i]
                        .0
                        .iter()
                        .map(|t| knowledge::describe_required(t, n, |p| type_name(world, p)))
                        .collect::<Vec<_>>()
                        .join(" or ")
                })
                .collect();
            tell(
                world,
                client,
                Span::Mansus,
                format!(
                    "Вам не хватает {} для завершения ритуала \"{}\".",
                    english_list(&parts),
                    def.name
                ),
            );
            return;
        }
    };
    let done = finish_recipe(world, client, mob, rune, def, &matched.atoms, loc);
    if !done {
        return;
    }
    // `cleanup_atoms`: everything but the living goes.
    for &e in &matched.atoms {
        if def.kind == Kind::Ultimate && is_living(world, e) {
            crate::health::gib(world, e);
            continue;
        }
        if !is_living(world, e) {
            despawn(world, e);
        }
    }
    for &(stack, amount) in &matched.stacks {
        crate::tools::use_stack(world, stack, amount);
    }
    tell(world, client, Span::Notice, "ритуал завершен");
    update_ui(world, mob);
}

/// `recipe_snowflake_check`: may drop atoms or refuse.
fn snowflake_check(
    world: &mut World,
    client: Entity,
    mob: Entity,
    def: &'static KnowledgeDef,
    atoms: &mut Vec<Candidate<Entity>>,
) -> bool {
    let Some(heretic) = world.get::<Heretic>(mob).cloned() else {
        return false;
    };
    match def.kind {
        _ if def.path == HUNT_AND_SACRIFICE => {
            if !heretic.living_heart {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "ритуал провален, нет живого сердца!",
                );
                return false;
            }
            if heretic.sac_targets.is_empty() {
                atoms.push(Candidate {
                    thing: mob,
                    path: "/mob/living/carbon/human".into(),
                    stack: None,
                });
                return true;
            }
            atoms.retain(|c| {
                if c.path != "/mob/living/carbon/human" {
                    return true;
                }
                let stat = world.get::<Stat>(c.thing).copied().unwrap_or_default();
                stat >= Stat::SoftCrit
                    && (heretic.sac_targets.contains(&c.thing)
                        || crate::antag::cult::server::is_cultist(world, c.thing))
            });
            if atoms.iter().any(|c| c.path == "/mob/living/carbon/human") {
                return true;
            }
            tell(
                world,
                client,
                Span::Warning,
                "ритуал провален, нет жертвы для приношения!",
            );
            false
        }
        _ if def.path == LIVING_HEART => {
            if heretic.living_heart {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "ритуал провален, у вас уже есть Живое сердце!",
                );
                return false;
            }
            true
        }
        Kind::Ultimate => {
            if heretic.ascended || heretic.can_ascend().is_err() {
                return false;
            }
            atoms.retain(|c| {
                c.path != "/mob/living/carbon/human"
                    || valid_ascension_sacrifice(world, def, c.thing)
            });
            true
        }
        Kind::Starting | Kind::Limited => limit_check(world, client, mob, def),
        _ => true,
    }
}

/// `ultimate/is_valid_sacrifice`: dead; the Ash Lord wants them burning or husked.
fn valid_ascension_sacrifice(world: &World, def: &KnowledgeDef, body: Entity) -> bool {
    if world.get::<Stat>(body) != Some(&Stat::Dead) {
        return false;
    }
    if def.path == ASH_FINAL {
        return crate::fire::on_fire(world, body)
            || world.get::<HumanLook>(body).is_some_and(|l| l.husk);
    }
    true
}

/// `limited_amount/recipe_snowflake_check` with BandaStation's blade renouncing.
fn limit_check(world: &mut World, client: Entity, mob: Entity, def: &'static KnowledgeDef) -> bool {
    let Some(h) = world.get::<Heretic>(mob) else {
        return false;
    };
    let blades = def
        .results
        .iter()
        .any(|r| knowledge::is_type(r, SICKLY_BLADE));
    if h.unlimited_blades && blades {
        return true;
    }
    let created: Vec<Entity> = h
        .get(def.path)
        .map(|r| r.created.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|&e| world.get_entity(e).is_ok())
        .collect();
    let exploded = h.exploded_blades;
    if let Some(mut h) = world.get_mut::<Heretic>(mob)
        && let Some(r) = h.get_mut(def.path)
    {
        r.created = created.clone();
    }
    if (created.len() as u32) < def.limit.max(1) {
        return true;
    }
    if def.kind != Kind::Starting {
        tell(
            world,
            client,
            Span::Warning,
            "ритуал провален - превышен лимит!",
        );
        return false;
    }
    // BandaStation: the farthest blade (10+ tiles) may be renounced once.
    let Some(&at) = world.get::<TilePos>(mob) else {
        return false;
    };
    let farthest = created
        .iter()
        .filter_map(|&e| {
            let pos = crate::modpc::pda::turf_of(world, e)?;
            (pos.z == at.z).then(|| (e, (pos.x - at.x).abs().max((pos.y - at.y).abs())))
        })
        .max_by_key(|(_, d)| *d);
    let Some((item, distance)) = farthest.filter(|(_, d)| *d >= MIN_DISTANCE_TO_EXPLODE) else {
        tell(world, client, Span::Warning, "ritual failed, at limit!");
        return false;
    };
    let _ = distance;
    if exploded >= 1 {
        tell(
            world,
            client,
            Span::Warning,
            "ritual failed, at limit, we can't renounce our blades anymore!",
        );
        return false;
    }
    tell(
        world,
        client,
        Span::Warning,
        "we renounce the furthest blade!",
    );
    if let Some(pos) = crate::modpc::pda::turf_of(world, item) {
        crate::explosions::explosion(
            world,
            pos,
            crate::explosions::Explosion::new(0, 0, 1).cause("heretic blade"),
        );
    }
    if let Some(holder) = world.get::<CarriedBy>(item).map(|c| c.0) {
        despawn_held(world, holder, item);
    } else {
        despawn(world, item);
    }
    if let Some(mut h) = world.get_mut::<Heretic>(mob) {
        h.exploded_blades += 1;
        if let Some(r) = h.get_mut(def.path) {
            r.created.retain(|&e| e != item);
        }
    }
    true
}

/// `on_finished_recipe`: true when the atoms are consumed.
fn finish_recipe(
    world: &mut World,
    client: Entity,
    mob: Entity,
    rune: Entity,
    def: &'static KnowledgeDef,
    atoms: &[Entity],
    loc: TilePos,
) -> bool {
    match def.kind {
        _ if def.path == HUNT_AND_SACRIFICE => {
            let Some(&victim) = atoms.first() else {
                return false;
            };
            let no_targets = world
                .get::<Heretic>(mob)
                .is_some_and(|h| h.sac_targets.is_empty());
            if no_targets && !crate::antag::cult::server::is_cultist(world, victim) {
                if obtain_targets(world, mob, false) {
                    return true;
                }
                tell(
                    world,
                    client,
                    Span::Warning,
                    "ритуал провален, цель не обнаружена!",
                );
                return false;
            }
            sacrifice_process(world, mob, victim);
            true
        }
        _ if def.path == LIVING_HEART => {
            if let Some(mut h) = world.get_mut::<Heretic>(mob) {
                h.living_heart = true;
            }
            chat_to(
                world,
                mob,
                Span::Warning,
                "Вы чувствуете, как ваше сердце начинает пульсировать все быстрее и быстрее по \
                 мере того, как оно пробуждается!",
            );
            refresh(world, mob);
            true
        }
        _ if def.path == FEAST_OF_OWLS => {
            open_prompt(
                world,
                client,
                mob,
                HereticPrompt {
                    kind: PromptKind::ConfirmFeast { rune },
                    title: "Пир для Сов".into(),
                    message: "Вы действительно хотите отказаться от своего вознесения? Это \
                              действие невозможно отменить."
                        .into(),
                    items: vec!["Да, я уверен".into(), "Нет".into()],
                    paths: Vec::new(),
                    targets: Vec::new(),
                },
            );
            false
        }
        Kind::Spell => {
            // `spell/on_finished_recipe`: `recharge_amount` (1.0) of the max charges.
            if let Some(mut h) = world.get_mut::<Heretic>(mob)
                && let Some(r) = h.get_mut(def.path)
            {
                r.charges = r.def().max_charges;
            }
            refresh(world, mob);
            true
        }
        Kind::Ultimate => {
            ascend(world, mob, def);
            true
        }
        Kind::Summon => {
            // shortcut: heretic monsters need basic mobs and ghost polls (not ported).
            tell(
                world,
                client,
                Span::Warning,
                "ритуал провален, нет призраков!",
            );
            false
        }
        _ => {
            if def.results.is_empty() {
                return false;
            }
            let mut made = Vec::new();
            for result in def.results {
                match crate::construction::spawn_obj(world, result, loc) {
                    Some(e) => made.push(e),
                    None => warn!("heretic: no prototype for {result}"),
                }
            }
            if made.is_empty() {
                return false;
            }
            if matches!(def.kind, Kind::Starting | Kind::Limited)
                && let Some(mut h) = world.get_mut::<Heretic>(mob)
                && let Some(r) = h.get_mut(def.path)
            {
                r.created.extend(made);
            }
            true
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Sacrifices

/// `obtain_targets`: a head of staff, a security officer, someone of the heretic's department,
/// then random crew up to five; living humans, not the heretic, not sacrificed before.
fn obtain_targets(world: &mut World, mob: Entity, silent: bool) -> bool {
    let blacklist = world.resource::<SacrificeBlacklist>().0.clone();
    let mut q = world
        .query_filtered::<(Entity, &Stat), (With<PlayerMob>, With<PlayerJob>, With<HumanLook>)>();
    let mut valid: Vec<Entity> = q
        .iter(world)
        .filter(|(e, s)| *e != mob && **s != Stat::Dead && !blacklist.contains(e))
        .map(|(e, _)| e)
        .collect();
    valid.sort();
    if valid.is_empty() {
        if !silent {
            chat_to(world, mob, Span::Mansus, "Не удалось найти ни одной цели!");
        }
        return false;
    }
    let departments = world
        .get_resource::<Jobs>()
        .map(|j| j.data.departments.clone())
        .unwrap_or_default();
    let job_of = |world: &World, e: Entity| world.get::<PlayerJob>(e).map(|j| j.0 as u16);
    let in_department = |world: &World, e: Entity, name: &str| {
        job_of(world, e).is_some_and(|j| {
            departments
                .iter()
                .any(|d| d.name == name && d.jobs.contains(&j))
        })
    };
    let own: Vec<String> = job_of(world, mob)
        .map(|j| {
            departments
                .iter()
                .filter(|d| d.jobs.contains(&j))
                .map(|d| d.name.clone())
                .collect()
        })
        .unwrap_or_default();
    let mut finals: Vec<Entity> = Vec::new();
    let take = |world: &mut World, valid: &mut Vec<Entity>, f: &dyn Fn(&World, Entity) -> bool| {
        let mut shuffled = valid.clone();
        for i in (1..shuffled.len()).rev() {
            let j = rand(world, i + 1);
            shuffled.swap(i, j);
        }
        if let Some(&e) = shuffled.iter().find(|&&e| f(world, e)) {
            valid.retain(|&v| v != e);
            Some(e)
        } else {
            None
        }
    };
    if let Some(e) = take(world, &mut valid, &|w, e| is_head(w, e)) {
        finals.push(e);
    }
    if let Some(e) = take(world, &mut valid, &|w, e| in_department(w, e, "Security")) {
        finals.push(e);
    }
    if let Some(e) = take(world, &mut valid, &|w, e| {
        own.iter().any(|d| in_department(w, e, d))
    }) {
        finals.push(e);
    }
    let mut sanity = 0;
    while finals.len() < NUM_TARGETS && valid.len() > NUM_TARGETS && sanity < 25 {
        let i = rand(world, valid.len());
        finals.push(valid.remove(i));
        sanity += 1;
    }
    if !silent {
        chat_to(
            world,
            mob,
            Span::Danger,
            "Ваши цели были определены. Ваше живое сердце позволит вам отслеживать их положение. \
             Идите и принесите их в жертву!",
        );
    }
    for target in finals {
        let name = name_of(world, target);
        let title = job_title(world, target).unwrap_or_default();
        if let Some(mut h) = world.get_mut::<Heretic>(mob) {
            if !h.sac_targets.contains(&target) {
                h.sac_targets.push(target);
            }
            h.all_sac_targets.push(name.clone());
        }
        if !silent {
            chat_to(world, mob, Span::Danger, format!("{name}, {title}."));
        }
    }
    true
}

/// `sacrifice_process`: the points (2, a head 3, a cultist 1), the target off every heretic's
/// list, then the victim's journey.
fn sacrifice_process(world: &mut World, mob: Entity, victim: Entity) {
    world.resource_mut::<SacrificeBlacklist>().0.push(victim);
    let mut q = world.query::<&mut Heretic>();
    for mut h in q.iter_mut(world) {
        h.sac_targets.retain(|&t| t != victim);
    }
    let head = is_head(world, victim);
    let cultist = crate::antag::cult::server::is_cultist(world, victim);
    if let Some(mut h) = world.get_mut::<Heretic>(mob) {
        h.total_sacrifices += 1;
        if head {
            h.high_value_sacrifices += 1;
        }
    }
    recharge_grasp(world, mob);
    let mut feedback = "Твои покровители принимают твоё подношение".to_owned();
    if head {
        adjust_points(world, mob, 3);
        feedback.push_str(" с великодушием");
    }
    if cultist {
        // shortcut: the cultist is dusted (gibbed here) without the reward item.
        adjust_points(world, mob, 1);
        chat_to(
            world,
            mob,
            Span::Hypnophrase,
            "A servant of the Sanguine Apostate!",
        );
        chat_to(world, mob, Span::Mansus, "Your patrons are rapturous!");
        crate::health::gib(world, victim);
        info!("heretic {mob} sacrificed the cultist {victim}");
        return;
    }
    adjust_points(world, mob, 2);
    chat_to(world, mob, Span::Hypnophrase, format!("{feedback}."));
    info!("heretic {mob} sacrificed {victim} (head: {head})");
    if !begin_sacrifice(world, mob, victim) {
        disembowel(world, victim);
    }
}

/// `begin_sacrifice`: cult shackles, back from the dead at 50 health, paralysed.
// shortcut: no shadow realm (tg's lazy template map and its hands minigame, the cursed organs
// and the heretic curse): the victim lies paralysed on the rune for SACRIFICE_SLEEP_DURATION,
// then wakes up uncuffed at a random safe station tile as a survivor, with tg's after-effects
// (confusion, knockdown, stamina damage), and the heretic hears where.
fn begin_sacrifice(world: &mut World, mob: Entity, victim: Entity) -> bool {
    let name = name_of(world, victim);
    visible(
        world,
        victim,
        "Тёмные усики начинают затягивать вас в воздух!",
        &format!(
            "{} начинает сильно дрожать, когда темные усики начинают затягивать их в воздух!",
            capitalize(&name)
        ),
        Span::Danger,
    );
    if world.get::<crate::restraints::Handcuffed>(victim).is_none()
        && let Some(&pos) = world.get::<TilePos>(victim)
        && let Some(cuffs) = crate::construction::spawn_obj(world, CULT_CUFFS, pos)
    {
        crate::restraints::apply_cuffs(world, cuffs, victim);
    }
    if !heal_and_revive(world, victim, 50.0) {
        return false;
    }
    chat_to(
        world,
        victim,
        Span::Hypnophrase,
        "Ваш разум словно разрывается на части, когда вы погружаетесь в неглубокую дрему...",
    );
    let secs = SACRIFICE_SLEEP_SECS as f32 * 1.2;
    crate::status_effects::apply(world, victim, Incap::Paralyze, secs);
    crate::status_effects::apply(world, victim, Incap::Immobilize, secs);
    let t = now(world);
    world.entity_mut(victim).insert(Sacrificed {
        heretic: mob,
        wake_at: t + SACRIFICE_SLEEP_SECS,
    });
    true
}

/// `heal_and_revive(heal_to)`: a dead body is healed up to `heal_to` health and revived.
fn heal_and_revive(world: &mut World, mob: Entity, heal_to: f32) -> bool {
    let Some(mut health) = world.get::<Health>(mob).cloned() else {
        return false;
    };
    health.oxy = (health.oxy - 100.0).max(0.0);
    if world.get::<Stat>(mob) != Some(&Stat::Dead) {
        world.entity_mut(mob).insert(health);
        return true;
    }
    let mut guard = 0;
    while health.health() < heal_to && guard < 50 {
        for kind in [
            DamageType::Brute,
            DamageType::Burn,
            DamageType::Tox,
            DamageType::Oxy,
        ] {
            health.adjust(kind, -5.0);
        }
        guard += 1;
    }
    world.entity_mut(mob).insert(health);
    let revived = crate::medical::defib::revive(world, mob);
    if revived {
        let name = name_of(world, mob);
        visible(
            world,
            mob,
            "Ваше сердце начинает биться с нечестивой силой!",
            &format!(
                "Сердце {name} начинает биться с нечестивой силой, когда оно возвращается из \
                 мертвых!"
            ),
            Span::Danger,
        );
        let ghost = {
            let mut q = world.query::<(Entity, &Ghost)>();
            q.iter(world)
                .find(|(_, g)| g.body == Some(mob))
                .map(|(e, _)| e)
        };
        if ghost.is_some() {
            crate::ghost::return_to_body(world, mob);
        }
    }
    revived
}

/// `return_target` + `after_return_live_target`.
fn return_sacrifice(world: &mut World, victim: Entity, chain: Sacrificed) {
    world.entity_mut(victim).remove::<Sacrificed>();
    if let Some(&pos) = world.get::<TilePos>(victim) {
        crate::restraints::uncuff(world, victim, pos);
    }
    let dest = crate::events::electrical_storm::random_station_turf(world);
    if let Some(dest) = dest {
        world.entity_mut(victim).insert(dest);
    }
    let dead = world.get::<Stat>(victim) == Some(&Stat::Dead);
    if dead {
        chat_to(
            world,
            victim,
            Span::Hypnophrase,
            "Вы не смогли противостоять ужасам Мансуса! Ваше разрушенное тело было возвращено на \
             станцию.",
        );
    } else {
        chat_to(
            world,
            victim,
            Span::Hypnophrase,
            "Борьба окончена, но высокой ценой. Вы вернулись на станцию невредимыми.",
        );
        chat_to(
            world,
            victim,
            Span::Hypnophrase,
            "Вы не помните ничего из того, что предшествовало этому событию - все, о чем вы можете \
             думать, это те ужасные руки...",
        );
        if let Some(mut e) = effects_mut(world, victim) {
            e.confusion = e.confusion.max(60.0);
        }
        crate::status_effects::knockdown(world, victim, 8.0, 0.0);
        crate::status_effects::adjust_stamina(world, victim, 120.0);
    }
    let name = name_of(world, victim);
    let area = world
        .get::<TilePos>(victim)
        .map(|&p| area_name(world, p))
        .unwrap_or_default();
    if world.get_entity(chain.heretic).is_ok() {
        let state = if dead {
            "мертвой. "
        } else {
            "живой, но с разбитым сознанием. "
        };
        chat_to(
            world,
            chain.heretic,
            Span::Notice,
            format!(
                "Ваша жертва, {name}, была возвращена на станцию - {state}Вы слышите шепот... \
                 {area}"
            ),
        );
    }
}

/// `disembowel_target`: 250 brute and death.
fn disembowel(world: &mut World, victim: Entity) {
    hurt(world, victim, DamageType::Brute, 250.0);
    world.entity_mut(victim).insert(Stat::Dead);
    world.entity_mut(victim).remove::<Sacrificed>();
    let name = name_of(world, victim);
    visible(
        world,
        victim,
        "Ваши органы жестоко вытаскиваются из груди теневыми руками!",
        &format!("Органы {name} вытаскиваются из их груди теневыми руами!"),
        Span::Danger,
    );
}

// ---------------------------------------------------------------------------------------------
// Ascension

/// `ultimate/on_finished_recipe`: ascended, CentCom's announcement, the shuttle; the Ash Lord's
/// spells.
// shortcut: no physiology (brute/burn halved), no environment immunity traits, no achievement.
fn ascend(world: &mut World, mob: Entity, def: &'static KnowledgeDef) {
    if let Some(mut h) = world.get_mut::<Heretic>(mob) {
        h.ascended = true;
        if def.path == ASH_FINAL {
            h.ash_ascended_spells = true;
        }
    }
    let name = world
        .get::<Name>(mob)
        .map(|n| n.to_string())
        .unwrap_or_else(|| name_of(world, mob));
    let text = ascension_text(def.announcement, &name, || spooky(world));
    let title = spooky(world);
    crate::events::priority_announce(world, &title, &text);
    info!("heretic {mob} ascended ({})", def.path);
    if let Some(shuttle) = world.get_resource::<crate::shuttle::EmergencyShuttle>()
        && matches!(
            shuttle.state.mode,
            crate::shuttle::ShuttleMode::Idle | crate::shuttle::ShuttleMode::Recall
        )
    {
        crate::shuttle::call_shuttle(
            world,
            "Сканерами дальнего действия зафиксирован критический разрыв в реальности, вызванный \
             потусторонними силами. Вероятность массовых потерь экипажа и уничтожения станции \
             превышает допустимые рассчётные вероятности. Запущен приоритетный эвакуационный \
             шаттл.",
            None,
            1.0,
        );
    }
    if let Some(mut shuttle) = world.get_resource_mut::<crate::shuttle::EmergencyShuttle>() {
        shuttle.state.no_recall = true;
    }
    refresh(world, mob);
}

/// `generate_heretic_text()`: 25 random symbols.
fn spooky(world: &mut World) -> String {
    const SYMBOLS: [&str; 10] = ["!", "$", "^", "@", "&", "#", "*", "(", ")", "?"];
    (0..25).map(|_| pick_str(world, &SYMBOLS)).collect()
}

/// The announcement with `%NAME%` and each `%SPOOKY%` replaced.
pub fn ascension_text(template: &str, name: &str, mut spooky: impl FnMut() -> String) -> String {
    let named = template.replace("%NAME%", name);
    let mut out = String::new();
    let mut rest = named.as_str();
    while let Some(i) = rest.find("%SPOOKY%") {
        out.push_str(&rest[..i]);
        out.push_str(&spooky());
        rest = &rest[i + "%SPOOKY%".len()..];
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------------------------
// Buttons and spells

fn spell_id(path: &str) -> String {
    format!("{SPELL_PREFIX}{path}")
}

/// The heretic's buttons: its spells (charges as maptext), the living heart, the Ash Lord's.
pub fn refresh(world: &mut World, mob: Entity) {
    let Some(h) = world.get::<Heretic>(mob).cloned() else {
        return;
    };
    let (background, border) = (
        icon(world, BACKGROUNDS_DMI, "bg_heretic"),
        icon(world, BACKGROUNDS_DMI, "bg_heretic_border"),
    );
    let in_hand = hands(world, mob)
        .iter()
        .any(|&e| world.get::<MansusFist>(e).is_some());
    let mut buttons = Vec::new();
    let mut add =
        |id: String, name: &str, icon, maptext: Option<String>, active: bool, ready: f64| {
            buttons.push((
                ActionButton {
                    id,
                    name: name.to_owned(),
                    icon,
                    background,
                    border,
                    available: true,
                    cooldown: 0.0,
                    active,
                    maptext,
                },
                ready,
            ));
        };
    for r in &h.researched {
        let def = r.def();
        if def.kind != Kind::Spell || def.action.is_empty() {
            continue;
        }
        let id = spell_id(def.path);
        let maptext = (def.max_charges != u32::MAX && r.charges < 100 && !h.ascended)
            .then(|| r.charges.to_string());
        let active = def.action == GRASP_ACTION && in_hand;
        add(
            id.clone(),
            def.action_name,
            icon(world, def.icon.0, def.icon.1),
            maptext,
            active,
            h.cooldown(&id),
        );
    }
    if h.living_heart {
        add(
            HEART_ID.into(),
            "Биение живого сердца",
            icon(world, "icons/obj/antags/eldritch.dmi", "living_heart"),
            None,
            false,
            h.cooldown(HEART_ID),
        );
    }
    if h.ash_ascended_spells {
        add(
            FIRE_SWORN_ID.into(),
            "Клятва Пламени",
            icon(world, ECULT_DMI, "fire_ring"),
            None,
            false,
            h.cooldown(FIRE_SWORN_ID),
        );
        add(
            FIRE_CASCADE_ID.into(),
            "Великий огненный каскад",
            icon(world, ECULT_DMI, "fire_ring"),
            None,
            false,
            h.cooldown(FIRE_CASCADE_ID),
        );
    }
    set_buttons(world, mob, buttons);
}

fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || !is_heretic(world, actor)
            || !can_act(world, actor)
        {
            continue;
        }
        let t = now(world);
        let ready = world.get::<Heretic>(actor).map_or(0.0, |h| h.cooldown(&id));
        if t < ready {
            continue;
        }
        if id == HEART_ID {
            track_target(world, client, actor);
        } else if id == FIRE_SWORN_ID {
            whisper(world, actor, "FL'MS.");
            world.entity_mut(actor).insert(FireRing {
                until: t + 60.0,
                next_tick: t,
            });
            set_cooldown(world, actor, &id, t + 70.0);
        } else if id == FIRE_CASCADE_ID {
            whisper(world, actor, "C'SC'D.");
            if let Some(&center) = world.get::<TilePos>(actor) {
                world.spawn(FireCascade {
                    caster: actor,
                    center,
                    step: 0,
                    radius: 6,
                    next_at: t,
                });
            }
            set_cooldown(world, actor, &id, t + 30.0);
        } else if let Some(path) = id.strip_prefix(SPELL_PREFIX) {
            let Some(def) = knowledge::def(path) else {
                continue;
            };
            cast_spell(world, client, actor, def);
        }
    }
}

fn set_cooldown(world: &mut World, mob: Entity, id: &str, ready: f64) {
    if let Some(mut h) = world.get_mut::<Heretic>(mob) {
        h.set_cooldown(id, ready);
    }
    refresh(world, mob);
}

/// A spell button: the charge check (`check_charges`), then the spell.
fn cast_spell(world: &mut World, client: Entity, mob: Entity, def: &'static KnowledgeDef) {
    let Some(h) = world.get::<Heretic>(mob) else {
        return;
    };
    let Some(r) = h.get(def.path) else {
        return;
    };
    let has_hand = hands(world, mob)
        .iter()
        .any(|&e| world.get::<MansusFist>(e).is_some());
    if r.charges == 0 && !h.ascended && !(def.action == GRASP_ACTION && has_hand) {
        tell(
            world,
            client,
            Span::Mansus,
            format!(
                "У вас недостаточно зарядов, чтобы произнести это заклинание! {}",
                def.transmute_text
            ),
        );
        return;
    }
    match def.action {
        GRASP_ACTION => toggle_grasp(world, client, mob),
        FIRE_BLAST => {
            // `channel_time` 2.5 s.
            let actor = Actor {
                client,
                mob,
                item: world.get::<Inventory>(mob).and_then(Inventory::active_item),
            };
            do_after(
                world,
                actor,
                mob,
                25.0,
                0,
                Job::Heretic(HereticJob::FireBlast),
            );
        }
        FIERY_REBIRTH => cast_fiery_rebirth(world, mob),
        _ => tell(
            world,
            client,
            Span::Warning,
            "Это заклинание ещё не перенесено в ss13-rs.",
        ),
    }
}

/// `touch/Activate`: a hand into an empty hand, or the hand back.
fn toggle_grasp(world: &mut World, client: Entity, mob: Entity) {
    if let Some(hand) = hands(world, mob)
        .into_iter()
        .find(|&e| world.get::<MansusFist>(e).is_some())
    {
        despawn_held(world, mob, hand);
        tell(
            world,
            client,
            Span::Notice,
            "You draw the power out of your hand.",
        );
        refresh(world, mob);
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let Some(hand) = crate::construction::spawn_obj(world, MANSUS_FIST, pos) else {
        return;
    };
    world
        .entity_mut(hand)
        .insert((NoDrop, MansusFist { owner: mob }));
    if !put_in_hands(world, mob, hand) {
        world.despawn(hand);
        crate::construction::tile_changed(world, pos);
        tell(world, client, Span::Warning, "Ваши руки заняты!");
        return;
    }
    tell(
        world,
        client,
        Span::Notice,
        "You channel the power of the spell to your hand.",
    );
    refresh(world, mob);
}

/// Carbons around `center` within `radius` (`range`): (entity, tile).
fn carbons_in_range(world: &World, center: TilePos, radius: i32) -> Vec<Entity> {
    let index = world.resource::<ss13_world::SpatialIndex>();
    let mut out: Vec<Entity> = index
        .in_rect(
            center.z,
            (center.x - radius, center.y - radius),
            (center.x + radius, center.y + radius),
        )
        .filter(|&e| world.get::<HumanLook>(e).is_some() && is_living(world, e))
        .collect();
    out.sort();
    out
}

/// `fire_blast/get_target`: a carbon in view, not blasted, not a heretic; the burning first.
fn fire_blast_target(world: &mut World, center: Entity, caster: Entity) -> Option<Entity> {
    let &pos = world.get::<TilePos>(center)?;
    let t = now(world);
    let mut possible = Vec::new();
    let mut priority = Vec::new();
    for e in carbons_in_range(world, pos, 7) {
        if e == center || e == caster || is_heretic(world, e) {
            continue;
        }
        if world.get::<FireBlasted>(e).is_some_and(|f| f.until > t) {
            continue;
        }
        // shortcut: within 7 tiles, without tg's `view()` and `get_path_to` checks.
        possible.push(e);
        if crate::fire::on_fire(world, e) && world.get::<Stat>(e) != Some(&Stat::Dead) {
            priority.push(e);
        }
    }
    let list = if priority.is_empty() {
        possible
    } else {
        priority
    };
    if list.is_empty() {
        return None;
    }
    let i = rand(world, list.len());
    Some(list[i])
}

/// `fire_blast/cast` after the channel: the caster is "blasted" (heals), the beam goes out.
fn cast_fire_blast(world: &mut World, mob: Entity) {
    if !is_heretic(world, mob) {
        return;
    }
    shout(world, mob, "V'LC'N!");
    let t = now(world);
    world.entity_mut(mob).insert(FireBlasted {
        until: t + 2.0,
        next_tick: t + 0.6,
        tick_damage: -2.0,
        bounce: None,
    });
    let path = "/datum/heretic_knowledge/spell/fire_blast";
    if let Some(target) = fire_blast_target(world, mob, mob) {
        send_fire_beam(world, mob, target, 4);
    }
    spend_charge(world, mob, path);
    set_cooldown(world, mob, &spell_id(path), t + 45.0);
}

fn shout(world: &mut World, mob: Entity, text: &str) {
    say(world, mob, text);
}

/// `send_beam`: 20 burn, 3 fire stacks and fire; it bounces on in a second while they burn.
fn send_fire_beam(world: &mut World, caster: Entity, target: Entity, bounces: u32) {
    let t = now(world);
    hurt(world, target, DamageType::Burn, 20.0);
    crate::fire::adjust_fire_stacks(world, target, 3.0);
    crate::fire::ignite_mob(world, target);
    world.entity_mut(target).insert(FireBlasted {
        until: t + 5.0,
        next_tick: t + 0.6,
        tick_damage: 1.0,
        bounce: (bounces >= 1).then_some((caster, bounces, t + 1.0)),
    });
    if bounces == 0 {
        // The chain's end: everyone around is knocked down and set on fire.
        let Some(&pos) = world.get::<TilePos>(target) else {
            return;
        };
        for e in carbons_in_range(world, pos, 1) {
            if e == caster || is_heretic(world, e) {
                continue;
            }
            crate::status_effects::knockdown(world, e, 0.8, 0.0);
            hurt(world, e, DamageType::Burn, 15.0);
            crate::fire::adjust_fire_stacks(world, e, 2.0);
            crate::fire::ignite_mob(world, e);
        }
    }
}

/// `fiery_rebirth`: the caster is put out; every burning living non-heretic within 14 tiles is
/// drained (dies in crit, else 20 burn and put out) and heals the caster 10 of each damage.
fn cast_fiery_rebirth(world: &mut World, mob: Entity) {
    whisper(world, mob, "GL'RY T' TH' N'GHT'W'TCH'ER.");
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    extinguish(world, mob);
    let mut victims = 0u32;
    for e in carbons_in_range(world, pos, 14) {
        if e == mob || is_heretic(world, e) || world.get::<Stat>(e) == Some(&Stat::Dead) {
            continue;
        }
        if !crate::fire::on_fire(world, e) {
            continue;
        }
        victims += 1;
        if world.get::<Stat>(e).is_some_and(|s| *s >= Stat::SoftCrit) {
            // `CAN_SUCCUMB`: the death mark.
            world.entity_mut(e).insert(Stat::Dead);
        }
        hurt(world, e, DamageType::Burn, 20.0);
        extinguish(world, e);
        heal(world, mob, 10.0);
    }
    let t = now(world);
    let path = "/datum/heretic_knowledge/spell/flame_birth";
    let cooldown = if victims == 0 {
        60.0
    } else {
        (60.0 - f64::from(victims) * 10.0).max(9.0)
    };
    let cooldown = if world.get::<Heretic>(mob).is_some_and(|h| h.ascended) {
        cooldown * 0.16
    } else {
        cooldown
    };
    spend_charge(world, mob, path);
    set_cooldown(world, mob, &spell_id(path), t + cooldown);
}

fn extinguish(world: &mut World, mob: Entity) {
    let stacks = world
        .get::<crate::fire::FireStacks>(mob)
        .map_or(0.0, |f| f.stacks);
    if stacks > 0.0 {
        crate::fire::adjust_fire_stacks(world, mob, -stacks);
    }
}

/// The living heart (`track_target`): the targets (and the heretic's blades elsewhere) in a
/// list, then where the pick is.
fn track_target(world: &mut World, client: Entity, mob: Entity) {
    let Some(h) = world.get::<Heretic>(mob).cloned() else {
        return;
    };
    let t = now(world);
    let targets: Vec<Entity> = h
        .sac_targets
        .iter()
        .copied()
        .filter(|&e| world.get_entity(e).is_ok())
        .collect();
    let blades: Vec<Entity> = h
        .researched
        .iter()
        .filter(|r| r.def().kind == Kind::Starting)
        .flat_map(|r| r.created.clone())
        .filter(|&e| {
            world.get_entity(e).is_ok() && world.get::<CarriedBy>(e).map(|c| c.0) != Some(mob)
        })
        .collect();
    if targets.is_empty() {
        tell(world, client, Span::Warning, "нет цели, посетите руну");
        set_cooldown(world, mob, HEART_ID, t + 1.0);
        return;
    }
    let mut items: Vec<String> = Vec::new();
    let mut values: Vec<Entity> = Vec::new();
    for e in blades.into_iter().chain(targets) {
        let mut name = name_of(world, e);
        let n = items.iter().filter(|i| i.starts_with(&name)).count();
        if n > 0 {
            name = format!("{name} ({})", n + 1);
        }
        items.push(name);
        values.push(e);
    }
    open_prompt(
        world,
        client,
        mob,
        HereticPrompt {
            kind: PromptKind::Track,
            title: "Биение живого сердца".into(),
            message: "Выберите цель для отслеживания".into(),
            items,
            paths: Vec::new(),
            targets: values,
        },
    );
}

/// `get_balloon_message`.
pub fn heart_message(from: TilePos, to: Option<TilePos>, dead: bool) -> String {
    let base = match to {
        None => "в другом мире!".to_owned(),
        Some(to) if to.z != from.z => "в другом мире!".to_owned(),
        Some(to) => {
            let dist = (to.x - from.x).abs().max((to.y - from.y).abs());
            let dir = dir_text(to.x - from.x, to.y - from.y);
            match dist {
                0..=15 => format!("очень близко, {dir}!"),
                16..=31 => format!("близко, {dir}!"),
                32..=127 => format!("далеко, {dir}!"),
                _ => "очень далеко!".to_owned(),
            }
        }
    };
    if dead {
        format!("мертвы, {base}")
    } else {
        base
    }
}

/// DM `dir2text(get_dir())` (BandaStation's are English).
fn dir_text(dx: i32, dy: i32) -> &'static str {
    match (dx.signum(), dy.signum()) {
        (0, 1) => "north",
        (0, -1) => "south",
        (1, 0) => "east",
        (-1, 0) => "west",
        (1, 1) => "northeast",
        (-1, 1) => "northwest",
        (1, -1) => "southeast",
        (-1, -1) => "southwest",
        _ => "",
    }
}

fn track_pick(world: &mut World, client: Entity, mob: Entity, target: Entity) {
    let Some(&from) = world.get::<TilePos>(mob) else {
        return;
    };
    if world.get_entity(target).is_err() {
        return;
    }
    let to = crate::modpc::pda::turf_of(world, target);
    let dead = world.get::<Stat>(target) == Some(&Stat::Dead);
    let text = heart_message(from, to, dead);
    tell(world, client, Span::Notice, text);
    if dead {
        let name = capitalize(&name_of(world, target));
        let sac = knowledge::def(HUNT_AND_SACRIFICE).map_or("", |k| k.name);
        tell(
            world,
            client,
            Span::Mansus,
            format!(
                "{name} в состоянии смерти. Принесите их к руне трансмутации и вызовите \"{sac}\", \
                 чтобы принести их в жертву!"
            ),
        );
    }
    if let Some(mut h) = world.get_mut::<Heretic>(mob) {
        h.last_tracked = Some(target);
    }
    let t = now(world);
    set_cooldown(world, mob, HEART_ID, t + 4.0);
}

// ---------------------------------------------------------------------------------------------
// Prompts

fn open_prompt(world: &mut World, client: Entity, mob: Entity, prompt: HereticPrompt) {
    let title = prompt.title.clone();
    world.entity_mut(mob).insert(prompt);
    world.write_message(OpenUi::new::<HereticListWindow>(client, mob, mob, &title));
}

impl ServerUi for HereticListWindow {
    type Source = &'static HereticPrompt;
    type UserParam = ();
    type Act = Option<String>;

    fn static_data(_: &HereticPrompt) {}

    fn data(prompt: &HereticPrompt, _: Entity, _: &()) -> ListInputData {
        ListInputData {
            title: prompt.title.clone(),
            message: prompt.message.clone(),
            init_value: prompt.items.first().cloned().unwrap_or_default(),
            items: prompt.items.clone(),
            timeout: None,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<Option<String>, UiReject> {
        match action {
            "submit" => params.text("entry").map(Some),
            "cancel" => Ok(None),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn close_window(world: &mut World, window: Entity) {
    if let Some(mut window) = world.get_mut::<UiWindow>(window) {
        window.cap_status(ss13_shared::ui::UiStatus::Close);
    }
}

fn release_rune(world: &mut World, kind: &PromptKind) {
    if let PromptKind::Ritual { rune } | PromptKind::ConfirmFeast { rune } = kind
        && let Some(mut r) = world.get_mut::<TransmutationRune>(*rune)
    {
        r.in_use = false;
    }
}

fn list_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<HereticListWindow>>>) {
    let pending: Vec<(Entity, Entity, Entity, Entity, Option<String>)> = acts
        .read(world.resource::<Messages<UiActed<HereticListWindow>>>())
        .map(|a| (a.window, a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (window, client, actor, source, pick) in pending {
        close_window(world, window);
        let Some(prompt) = world.entity_mut(source).take::<HereticPrompt>() else {
            continue;
        };
        release_rune(world, &prompt.kind);
        let Some(pick) = pick else {
            continue;
        };
        if actor != source || !is_heretic(world, actor) || !can_act(world, actor) {
            continue;
        }
        let Some(i) = prompt.items.iter().position(|it| *it == pick) else {
            continue;
        };
        match prompt.kind {
            PromptKind::Ritual { rune } => {
                let Some(&path) = prompt.paths.get(i) else {
                    continue;
                };
                // The rune must still be there and in reach (`require_near`).
                let near = match (world.get::<TilePos>(rune), world.get::<TilePos>(actor)) {
                    (Some(r), Some(a)) => {
                        r.z == a.z && (r.x - a.x).abs() <= 1 && (r.y - a.y).abs() <= 1
                    }
                    _ => false,
                };
                let invokable = world
                    .get::<Heretic>(actor)
                    .is_some_and(|h| h.get(path).is_some_and(|r| can_be_invoked(h, r)));
                if near && invokable {
                    do_ritual(world, client, actor, rune, path);
                }
            }
            PromptKind::Track => {
                if let Some(&target) = prompt.targets.get(i) {
                    track_pick(world, client, actor, target);
                }
            }
            PromptKind::ConfirmFinal { path, category } => {
                if pick == "Да" {
                    try_research(world, actor, path, category, true);
                }
            }
            PromptKind::ConfirmFeast { rune } => {
                let near = match (world.get::<TilePos>(rune), world.get::<TilePos>(actor)) {
                    (Some(r), Some(a)) => {
                        r.z == a.z && (r.x - a.x).abs() <= 2 && (r.y - a.y).abs() <= 2
                    }
                    _ => false,
                };
                if pick == "Да, я уверен" && near {
                    feast_of_owls(world, actor);
                }
            }
        }
    }
}

/// `feast_of_owls/on_finished_recipe` after the alert: five points, no ascension.
// shortcut: the five points come at once (tg: one a second with screams and jitter).
fn feast_of_owls(world: &mut World, mob: Entity) {
    let Some(h) = world.get::<Heretic>(mob) else {
        return;
    };
    if h.feast_of_owls {
        return;
    }
    if let Some(mut h) = world.get_mut::<Heretic>(mob) {
        h.feast_of_owls = true;
    }
    crate::status_effects::apply(world, mob, Incap::Paralyze, 5.0);
    for _ in 0..5 {
        adjust_points(world, mob, 1);
    }
    chat_to(
        world,
        mob,
        Span::Danger,
        "Вы чувствуете, как что-то невидимое разрушает саму вашу сущность!",
    );
    chat_to(
        world,
        mob,
        Span::Danger,
        "Ваши амбиции разрушены, но на их месте пробуждается нечто могущественное...",
    );
    let drain = pick_str(world, INFLUENCE_DRAIN_MESSAGE);
    chat_to(world, mob, Span::Hypnophrase, drain);
    update_ui(world, mob);
}

// ---------------------------------------------------------------------------------------------
// The research window

/// `act('research', { path, category })`.
#[derive(Clone, Debug, PartialEq)]
pub struct ResearchAct {
    pub path: String,
    pub category: String,
}

impl ServerUi for AntagInfoHereticUi {
    type Source = (&'static Heretic, &'static HereticPaths);
    type UserParam = Option<Res<'static, IconIndex>>;
    type Act = ResearchAct;

    fn static_data((_, paths): (&Heretic, &HereticPaths)) -> HereticStatic {
        paths.0.clone()
    }

    fn data(
        (heretic, _): (&Heretic, &HereticPaths),
        _: Entity,
        icons: &Option<Res<IconIndex>>,
    ) -> HereticInfo {
        info_of(heretic, icons.as_deref())
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ResearchAct, UiReject> {
        if action != RESEARCH {
            return Err(UiReject::UnknownAction);
        }
        let get = |k: &str| {
            params
                .get(k)
                .map(str::to_owned)
                .ok_or_else(|| UiReject::BadParams(format!("{k}: missing")))
        };
        Ok(ResearchAct {
            path: get("path")?,
            category: get("category")?,
        })
    }
}

fn node(
    entry: &Entry,
    researched: Option<&Researched>,
    heretic: &Heretic,
    icons: Option<&IconIndex>,
) -> KnowledgeNode {
    let def = entry.def();
    let resolve = |dmi: &str, state: &str| icons.and_then(|i| i.get(dmi, state));
    let mut desc = def.desc.to_owned();
    if researched.is_some() && def.kind == Kind::Spell && def.max_charges != u32::MAX {
        desc.push_str(&format!(
            "<br>Количество зарядов: {}{}.",
            def.max_charges,
            if def.transmute_text.is_empty() {
                ""
            } else {
                ". После их расхода заклинание нужно перезарядить"
            }
        ));
    }
    let mut notice = def.notice.to_owned();
    let mut disabled = false;
    if def.kind == Kind::Ultimate
        && let Err(reason) = heretic.can_ascend()
    {
        disabled = true;
        notice.push_str("<br>");
        notice.push_str(reason);
    }
    KnowledgeNode {
        path: def.path.to_owned(),
        name: def.name.to_owned(),
        desc,
        gain_flavor: def.gain_text.to_owned(),
        cost: entry.cost,
        depth: entry.depth,
        bgr: entry.bgr.to_owned(),
        bgr_icon: resolve(KNOWLEDGE_DMI, entry.bgr),
        icon: resolve(def.icon.0, def.icon.1),
        category: entry.category.as_str().to_owned(),
        done: researched.is_some(),
        ascension: def.kind == Kind::Ultimate,
        disabled,
        notice,
        info: def.transmute_text.to_owned(),
    }
}

/// `ui_data`.
pub fn info_of(heretic: &Heretic, icons: Option<&IconIndex>) -> HereticInfo {
    let mut tiers: Vec<Vec<KnowledgeNode>> = Vec::new();
    let mut shop_nodes = Vec::new();
    let place = |tiers: &mut Vec<Vec<KnowledgeNode>>, n: KnowledgeNode| {
        let depth = n.depth.max(1) as usize;
        while tiers.len() < depth {
            tiers.push(Vec::new());
        }
        tiers[depth - 1].push(n);
    };
    for r in &heretic.researched {
        let n = node(&r.entry, Some(r), heretic, icons);
        if matches!(r.entry.category, Category::Shop | Category::Draft) {
            shop_nodes.push(n);
        } else {
            place(&mut tiers, n);
        }
    }
    let can = heretic.researchable();
    for e in &heretic.tree {
        if e.def().kind == Kind::Starting || !can.contains(&e.id) {
            continue;
        }
        place(&mut tiers, node(e, None, heretic, icons));
    }
    if heretic.route.is_some() {
        for e in &heretic.draft {
            if can.contains(&e.id) {
                place(&mut tiers, node(e, None, heretic, icons));
            }
        }
        for e in &heretic.shop {
            if can.contains(&e.id) {
                shop_nodes.push(node(e, None, heretic, icons));
            }
        }
    }
    HereticInfo {
        charges: heretic.points,
        total_sacrifices: heretic.total_sacrifices,
        ascended: heretic.ascended,
        objectives: heretic
            .objectives
            .iter()
            .enumerate()
            .map(|(i, o)| ObjectiveInfo {
                count: i as u32 + 1,
                explanation: o.explanation(),
                complete: o.complete(heretic),
            })
            .collect(),
        can_change_objective: false,
        passive_level: 1,
        points_to_aura: POINTS_TO_AURA,
        knowledge_tiers: tiers,
        knowledge_shop: shop_nodes,
    }
}

/// `path_info`: every path's preview from a fresh start shop.
pub fn static_info(icons: Option<&IconIndex>) -> HereticStatic {
    let shop = knowledge::start_shop();
    let blank = Heretic::new(String::new(), 0.0);
    let entry = |path: &str| shop.iter().find(|e| e.path == path);
    let paths = PATHS
        .iter()
        .filter_map(|p| {
            let start = entry(p.start)?;
            Some(HereticPath {
                route: p.route.to_owned(),
                complexity: p.complexity.to_owned(),
                complexity_color: p.complexity_color.to_owned(),
                description: p.description.iter().map(|s| (*s).to_owned()).collect(),
                pros: p.pros.iter().map(|s| (*s).to_owned()).collect(),
                cons: p.cons.iter().map(|s| (*s).to_owned()).collect(),
                tips: p.tips.iter().map(|s| (*s).to_owned()).collect(),
                starting_knowledge: node(start, None, &blank, icons),
                preview_abilities: [p.tier1, p.tier2, p.tier3, p.tier4]
                    .iter()
                    .filter_map(|t| entry(t))
                    .map(|e| node(e, None, &blank, icons))
                    .collect(),
                passive_name: p.passive.name.to_owned(),
                passive_recharge: p.passive.recharge.to_owned(),
                passive_description: p
                    .passive
                    .descriptions
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect(),
            })
        })
        .collect();
    HereticStatic {
        paths,
        node_finished: icons.and_then(|i| i.get(KNOWLEDGE_DMI, "node_finished")),
        node_locked: icons.and_then(|i| i.get(KNOWLEDGE_DMI, "node_locked")),
    }
}

/// `path_info` with the sprites resolved, on the heretic (static data does not see resources).
#[derive(Component, Clone, Debug)]
pub struct HereticPaths(pub HereticStatic);

fn update_ui(world: &mut World, mob: Entity) {
    // The component changed: the window's data follows (`update_data_for_all_viewers`).
    if let Some(mut h) = world.get_mut::<Heretic>(mob) {
        h.set_changed();
    }
}

fn research_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagInfoHereticUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, ResearchAct)> = acts
        .read(world.resource::<Messages<UiActed<AntagInfoHereticUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, act) in pending {
        if actor != source || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client) {
            continue;
        }
        if world.get::<Stat>(actor) == Some(&Stat::Dead) {
            continue;
        }
        let (Some(def), Some(category)) =
            (knowledge::def(&act.path), Category::parse(&act.category))
        else {
            warn!("heretic {actor}: research of an unknown knowledge {act:?}");
            continue;
        };
        try_research(world, actor, def.path, category, false);
    }
}

/// `ui_act("research")`: researchable, ascension allowed, final knowledge confirmed, paid.
fn try_research(
    world: &mut World,
    mob: Entity,
    path: &'static str,
    category: Category,
    confirmed: bool,
) {
    let Some(h) = world.get::<Heretic>(mob) else {
        return;
    };
    let Some(entry) = h.shop_of(category).iter().find(|e| e.path == path) else {
        warn!("heretic {mob} potentially attempted to href exploit: {path} not in its shop");
        return;
    };
    if !h.researchable().contains(&entry.id) {
        warn!("heretic {mob} potentially attempted to href exploit: {path} not researchable");
        return;
    }
    let def = entry.def();
    if def.kind == Kind::Ultimate && h.can_ascend().is_err() {
        warn!("heretic {mob} attempted a final knowledge without its objectives");
        return;
    }
    if entry.cost > h.points {
        return;
    }
    if def.is_final && !h.unlimited_blades && !confirmed {
        if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
            open_prompt(
                world,
                client,
                mob,
                HereticPrompt {
                    kind: PromptKind::ConfirmFinal { path, category },
                    title: "Изучить финальное заклинание?".into(),
                    message: "ЭТО ОТКЛЮЧИТ ВОЗМОЖНОСТЬ РАЗБИТЬ ВАШ КЛИНОК. Вы готовы изучить это? \
                              Ограничение на количества клинков будет убрано."
                        .into(),
                    items: vec!["Да".into(), "Нет".into()],
                    paths: Vec::new(),
                    targets: Vec::new(),
                },
            );
        }
        return;
    }
    purchase(world, mob, path, category);
}

/// `/datum/action/antag_info/Trigger` of a heretic.
fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if is_heretic(world, source) {
            world.write_message(OpenUi::new::<AntagInfoHereticUi>(
                client,
                actor,
                source,
                HERETIC_NAME,
            ));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Ticking

/// Every tick: the passive point, marks, the sacrifice chain, the fire blasts, the Ash Lord's
/// fire.
fn heretic_tick(world: &mut World) {
    let t = now(world);
    // `passive_influence_gain`.
    let mut q = world.query::<(Entity, &Heretic)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, h)| t >= h.next_passive_gain)
        .map(|(e, _)| e)
        .collect();
    for mob in due {
        if let Some(mut h) = world.get_mut::<Heretic>(mob) {
            h.next_passive_gain += PASSIVE_GAIN_SECS;
        }
        adjust_points(world, mob, 1);
        if world.get::<Stat>(mob).is_some_and(|s| *s < Stat::HardCrit) {
            let drain = pick_str(world, INFLUENCE_DRAIN_MESSAGE);
            chat_to(
                world,
                mob,
                Span::Mansus,
                format!("Вы слышите шепот... {drain}"),
            );
        }
        update_ui(world, mob);
    }
    // Marks run out.
    let mut q = world.query::<(Entity, &AshMark)>();
    let expired: Vec<Entity> = q
        .iter(world)
        .filter(|(_, m)| m.until <= t)
        .map(|(e, _)| e)
        .collect();
    for e in expired {
        world.entity_mut(e).remove::<AshMark>();
    }
    // The sacrificed come back.
    let mut q = world.query::<(Entity, &Sacrificed)>();
    let woke: Vec<(Entity, Sacrificed)> = q
        .iter(world)
        .filter(|(_, s)| s.wake_at <= t)
        .map(|(e, s)| (e, *s))
        .collect();
    for (victim, chain) in woke {
        return_sacrifice(world, victim, chain);
    }
    tick_fire_blasts(world, t);
    tick_fire_rings(world, t);
    tick_cascades(world, t);
}

fn tick_fire_blasts(world: &mut World, t: f64) {
    let mut q = world.query::<(Entity, &FireBlasted)>();
    let all: Vec<(Entity, FireBlasted)> = q.iter(world).map(|(e, f)| (e, *f)).collect();
    for (e, mut f) in all {
        if let Some((caster, bounces, at)) = f.bounce
            && t >= at
        {
            f.bounce = None;
            // `continue_beam`: only while they still burn.
            if crate::fire::on_fire(world, e)
                && let Some(next) = fire_blast_target(world, e, caster)
            {
                world.entity_mut(e).insert(f);
                send_fire_beam(world, caster, next, bounces - 1);
            }
        }
        while f.next_tick <= t.min(f.until) {
            f.next_tick += 0.6;
            let dmg = f.tick_damage * 0.6;
            if dmg > 0.0 {
                hurt(world, e, DamageType::Burn, dmg);
            } else {
                heal(world, e, -dmg);
            }
            crate::status_effects::adjust_stamina(world, e, 2.0 * dmg);
        }
        if t >= f.until && f.bounce.is_none() {
            world.entity_mut(e).remove::<FireBlasted>();
        } else if world.get_entity(e).is_ok() {
            world.entity_mut(e).insert(f);
        }
    }
}

/// `fire_ring/tick` every 0.2 s: the 3x3 around burns, 2.5 burn/s to the others there.
fn tick_fire_rings(world: &mut World, t: f64) {
    let mut q = world.query::<(Entity, &FireRing)>();
    let rings: Vec<(Entity, FireRing)> = q.iter(world).map(|(e, r)| (e, *r)).collect();
    for (owner, mut ring) in rings {
        if t >= ring.until || world.get::<Stat>(owner) == Some(&Stat::Dead) {
            world.entity_mut(owner).remove::<FireRing>();
            continue;
        }
        while ring.next_tick <= t {
            ring.next_tick += 0.2;
            let Some(&pos) = world.get::<TilePos>(owner) else {
                break;
            };
            burn_tiles(world, owner, pos, 1, 25.0 * 0.2, 2.5 * 0.2);
        }
        world.entity_mut(owner).insert(ring);
    }
}

fn burn_tiles(
    world: &mut World,
    owner: Entity,
    center: TilePos,
    radius: i32,
    volume: f64,
    burn: f32,
) {
    let mut tiles = Vec::new();
    for dx in -radius..=radius {
        for dy in -radius..=radius {
            tiles.push(TilePos::new(center.x + dx, center.y + dy, center.z));
        }
    }
    burn_list(world, owner, &tiles, volume, burn);
}

fn burn_list(world: &mut World, owner: Entity, tiles: &[TilePos], volume: f64, burn: f32) {
    let mut victims = Vec::new();
    {
        let index = world.resource::<ss13_world::SpatialIndex>();
        for &p in tiles {
            for &e in index.on_tile(p) {
                if e != owner && world.get::<Health>(e).is_some() {
                    victims.push(e);
                }
            }
        }
    }
    if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
        for &p in tiles {
            atmos.hotspot_expose(p, 750.0, volume, true);
        }
    }
    for v in victims {
        hurt(world, v, DamageType::Burn, burn);
    }
}

/// `fire_cascade`: rings of fire spreading out one a 0.3 s.
fn tick_cascades(world: &mut World, t: f64) {
    let mut q = world.query::<(Entity, &FireCascade)>();
    let all: Vec<(Entity, FireCascade)> = q.iter(world).map(|(e, c)| (e, *c)).collect();
    for (e, mut c) in all {
        if t < c.next_at {
            continue;
        }
        let r = c.step + 1;
        let mut ring = Vec::new();
        for dx in -r..=r {
            for dy in -r..=r {
                ring.push(TilePos::new(c.center.x + dx, c.center.y + dy, c.center.z));
            }
        }
        burn_list(world, c.caster, &ring, 50.0, 5.0);
        c.step += 1;
        c.next_at = t + 0.3;
        if c.step > c.radius {
            world.despawn(e);
        } else {
            world.entity_mut(e).insert(c);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Round end

/// `/datum/antagonist/heretic/roundend_report` of every heretic.
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let mut q = world.query::<(Entity, &Heretic)>();
    let mut heretics: Vec<(Entity, Heretic)> = q.iter(world).map(|(e, h)| (e, h.clone())).collect();
    if heretics.is_empty() {
        return Vec::new();
    }
    heretics.sort_by_key(|(e, _)| *e);
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = vec![line(vec![ChatSegment::new(
        Span::Bold,
        format!("{ROUNDEND_CATEGORY} были:"),
    )])];
    for (mob, h) in heretics {
        out.push(crate::antag::traitor::print_player(world, mob, &h.key));
        out.push(line(vec![
            ChatSegment::new(Span::Bold, "Жертвоприношений сделано:"),
            ChatSegment::new(Span::Plain, format!(" {}", h.total_sacrifices)),
        ]));
        let targets = if h.all_sac_targets.is_empty() {
            "Никто".to_owned()
        } else {
            english_list(&h.all_sac_targets)
        };
        out.push(line(vec![ChatSegment::new(
            Span::Plain,
            format!("Целями для жертвоприношения были: {targets}."),
        )]));
        let mut succeeded = true;
        for (i, o) in h.objectives.iter().enumerate() {
            let done = o.complete(&h);
            succeeded &= done;
            out.push(line(vec![
                ChatSegment::new(Span::Bold, format!("{OBJECTIVE_NAME} #{}", i + 1)),
                ChatSegment::new(Span::Plain, format!(": {} ", o.explanation())),
                if done {
                    ChatSegment::new(Span::GreenText, "Успех!")
                } else {
                    ChatSegment::new(Span::RedText, "Провал.")
                },
            ]));
        }
        if h.feast_of_owls {
            out.push(line(vec![ChatSegment::new(
                Span::GreenText,
                "Вознесение отринуто",
            )]));
        }
        out.push(line(vec![if h.ascended {
            ChatSegment::new(Span::GreenText, "ЕРЕТИК ВОЗНЕССЯ!")
        } else if succeeded {
            ChatSegment::new(Span::GreenText, "Еретик преуспел, но не вознёсся!")
        } else {
            ChatSegment::new(Span::RedText, "Еретик провалился.")
        }]));
        let names: Vec<String> = h
            .researched
            .iter()
            .map(|r| r.def().name.to_owned())
            .collect();
        out.push(line(vec![
            ChatSegment::new(Span::Bold, "Исследованные знания: "),
            ChatSegment::new(Span::Plain, english_list(&names)),
        ]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn objectives_explain_and_complete() {
        let mut h = Heretic::new("k".into(), 0.0);
        assert!(!HereticObjective::PickPath.complete(&h));
        let minor = HereticObjective::MinorSacrifice { target: 4 };
        assert_eq!(
            minor.explanation(),
            "Принесите в жертву не менее 4 членов экипажа."
        );
        h.total_sacrifices = 4;
        assert!(minor.complete(&h));
        assert!(!HereticObjective::MajorSacrifice.complete(&h));
        h.high_value_sacrifices = 1;
        assert!(HereticObjective::MajorSacrifice.complete(&h));
        h.objectives = vec![minor, HereticObjective::MajorSacrifice];
        assert_eq!(h.can_ascend(), Ok(()));
        h.feast_of_owls = true;
        assert!(h.can_ascend().is_err());
    }

    #[test]
    fn points_cross_the_aura_threshold() {
        let mut h = Heretic::new("k".into(), 0.0);
        assert_eq!(h.points, 1);
        assert!(!h.adjust_points(8));
        assert!(h.adjust_points(1), "nine gained is more than eight");
        assert_eq!(h.points, 10);
        h.adjust_points(-20);
        assert_eq!((h.points, h.knowledge_gained), (0, 9));
    }

    #[test]
    fn influences_per_heretic_follow_tg() {
        let total = |n: u32| -> u32 {
            (1..=n)
                .map(|i| (NUM_INFLUENCES_PER_HERETIC + 1).saturating_sub(i).max(1))
                .sum()
        };
        assert_eq!(total(1), 5);
        assert_eq!(total(2), 9);
        assert_eq!(total(6), 5 + 4 + 3 + 2 + 1 + 1);
    }

    #[test]
    fn the_heart_says_how_far() {
        let at = |x, y| TilePos::new(x, y, 1);
        assert_eq!(
            heart_message(at(1, 1), Some(at(5, 1)), false),
            "очень близко, east!"
        );
        assert_eq!(
            heart_message(at(1, 1), Some(at(1, 30)), false),
            "близко, north!"
        );
        assert_eq!(
            heart_message(at(50, 50), Some(at(10, 10)), true),
            "мертвы, далеко, southwest!"
        );
        assert_eq!(
            heart_message(at(1, 1), Some(TilePos::new(1, 1, 2)), false),
            "в другом мире!"
        );
        assert_eq!(
            heart_message(at(1, 1), Some(at(200, 1)), false),
            "очень далеко!"
        );
    }

    #[test]
    fn ascension_text_fills_name_and_spooks() {
        let mut n = 0;
        let text = ascension_text(
            "%SPOOKY% Лорд %NAME% вознесся! %SPOOKY%",
            "Боб",
            || {
                n += 1;
                format!("<{n}>")
            },
        );
        assert_eq!(text, "<1> Лорд Боб вознесся! <2>");
    }
}
