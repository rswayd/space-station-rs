//! Fishing (server): rods, casting, the challenge's phases, the reward roll and the minigame
//! window. Fish are [`super::fish`], aquariums and the fish-porter [`super::aquarium`].

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::fishing::{
    AQUARIUM_SOURCE, DUD, DmFish, DmFishingRod, FishSourceDef, fish_source, turf_source,
};
use ss13_mapload::{DmFood, DmItem, DmTurfBuild, Turf};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::UiInterface as _;
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};

use super::fish::Fish;
use super::minigame::{self, Movement, Setup};
use super::{FishingMinigameUi, LOSE, MinigameParams, WIN};
use crate::click::{RangedClick, ResolveClicks};
use crate::click_handlers::{AddClickHandler, ClickPhase};
use crate::combat::CombatRng;
use crate::health::Stat;
use crate::inventory::Inventory;
use crate::server::PlayerMob;
use crate::storage::{Stored, StoredIn};

/// `/obj/item/food`: every food is bait (`TRAIT_FISHING_BAIT`).
const FOOD_ROOT: &str = "/obj/item/food";
/// `/obj/item/food/bait` subtypes' `bait_quality` (the dump spells the basic trait
/// `baic_quality_bait`).
const BAIT_QUALITIES: [(&str, BaitQuality); 5] = [
    ("/obj/item/food/bait/natural", BaitQuality::Great),
    (
        "/obj/item/food/bait/doughball/synthetic/super",
        BaitQuality::Great,
    ),
    ("/obj/item/food/bait/worm/premium", BaitQuality::Good),
    (
        "/obj/item/food/bait/doughball/synthetic",
        BaitQuality::Basic,
    ),
    ("/obj/item/food/bait", BaitQuality::Basic),
];
/// Food type bits (`code/__DEFINES/food.dm`).
const MEAT: u32 = 1 << 0;
const VEGETABLES: u32 = 1 << 1;
const FRUIT: u32 = 1 << 5;
const DAIRY: u32 = 1 << 6;
const SEAFOOD: u32 = 1 << 16;
const BUGS: u32 = 1 << 18;
const GORE: u32 = 1 << 19;
const GROWN_ROOT: &str = "/obj/item/food/grown";
/// `FISH_WEIGHT_MULT_WITHOUT_BAIT`.
const WEIGHT_MULT_WITHOUT_BAIT: f64 = 0.15;
/// `FISHING_MINIMUM_DIFFICULTY`.
const MINIMUM_DIFFICULTY: f32 = 6.0;
/// `FISH_TRAIT_MINOR_DIFFICULTY_BOOST`.
const MINOR_DIFFICULTY_BOOST: f32 = 5.0;
/// The default line's `cast_range` (`/obj/item/fishing_line`).
const LINE_CAST_RANGE: f32 = 2.0;
/// `wait_time_range` of `/datum/fishing_challenge`, seconds.
const WAIT_MIN: u32 = 3;
const WAIT_MAX: u32 = 25;
/// `casting_cd`.
const CAST_COOLDOWN: f64 = 1.0;
/// A window that does not open (no client view) ends the challenge after this.
const WINDOW_GRACE_SECS: f64 = 5.0;
const SPLASH: &str = "sound/effects/splash.ogg";
const FISH_SPLASH: &str = "sound/effects/fish_splash.ogg";
const BIG_SPLASH: &str = "sound/effects/bigsplash.ogg";

/// `TRAIT_*_QUALITY_BAIT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaitQuality {
    Basic,
    Good,
    Great,
}

impl BaitQuality {
    /// `weight_result_multiplier` and `weight_leveling_exponents`.
    fn mods(self) -> (f64, f64) {
        match self {
            Self::Great => (9.0, 0.7),
            Self::Good => (3.5, 0.55),
            Self::Basic => (2.0, 0.4),
        }
    }
}

/// A rod's stats and its bait (an item stored inside it).
#[derive(Component, Clone, Debug)]
pub struct FishingRod {
    pub stats: DmFishingRod,
    pub bait: Option<Entity>,
    pub cast_ready_at: f64,
}

impl FishingRod {
    /// `get_cast_range` without the skill bonus.
    pub fn cast_range(&self) -> u32 {
        let line = if self.stats.has_line {
            LINE_CAST_RANGE
        } else {
            0.0
        };
        (self.stats.cast_range + line).max(1.0) as u32
    }
}

/// What the bait is, for the weights.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BaitInfo {
    pub quality: Option<BaitQuality>,
    pub foodtypes: u32,
    pub grown: bool,
}

/// What was rolled at the bite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reward {
    Dud,
    /// A new atom of this type.
    Path(&'static str),
    /// A fish already there (inside an aquarium).
    Fish(Entity),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// The float bobs (`WAIT_PHASE`).
    Waiting,
    /// Something bites: strike now (`BITING_PHASE`).
    Biting,
    /// The window is open (`MINIGAME_PHASE`).
    Minigame,
}

/// On a fisher: the running `/datum/fishing_challenge`.
#[derive(Component, Clone, Debug)]
pub struct Fishing {
    pub rod: Entity,
    pub spot: Entity,
    pub source: &'static FishSourceDef,
    pub phase: Phase,
    /// When the phase ends by itself (the bite, the bite's window).
    pub next_at: f64,
    pub reward: Reward,
    pub last_click: f64,
    pub params: Option<MinigameParams>,
    pub started_at: f64,
    pub window_seen: bool,
}

/// Limited rewards left per source (`fish_counts`) and the ones on their way back.
#[derive(Resource, Default)]
pub struct FishCounts {
    left: Vec<((&'static str, &'static str), u32)>,
    regen: Vec<(f64, &'static str, &'static str)>,
}

impl FishCounts {
    fn left(&self, source: &'static FishSourceDef, reward: &str) -> Option<u32> {
        let def = source.counts.iter().find(|(p, ..)| *p == reward)?;
        Some(
            self.left
                .iter()
                .find(|((s, r), _)| *s == source.path && *r == reward)
                .map_or(def.1, |(_, n)| *n),
        )
    }

    /// One taken: false when none is left.
    fn take(&mut self, source: &'static FishSourceDef, reward: &'static str, now: f64) -> bool {
        let Some(&(_, count, regen)) = source.counts.iter().find(|(p, ..)| *p == reward) else {
            return true;
        };
        let key = (source.path, reward);
        let entry = match self.left.iter().position(|(k, _)| *k == key) {
            Some(i) => &mut self.left[i].1,
            None => {
                self.left.push((key, count));
                &mut self.left.last_mut().expect("just pushed").1
            }
        };
        if *entry == 0 {
            return false;
        }
        *entry -= 1;
        self.regen.push((now + regen, source.path, reward));
        true
    }

    /// `regen_count` of the rewards whose time came.
    fn regenerate(&mut self, now: f64) {
        let (due, waiting): (Vec<_>, Vec<_>) = self.regen.drain(..).partition(|r| r.0 <= now);
        self.regen = waiting;
        for (_, source, reward) in due {
            if let Some((_, n)) = self.left.iter_mut().find(|(k, _)| *k == (source, reward)) {
                *n += 1;
            }
        }
    }
}

pub struct FishingServerPlugin;

impl Plugin for FishingServerPlugin {
    fn build(&self, app: &mut App) {
        // Rod clicks first: a fisher's clicks all go to the challenge (tg `COMSIG_MOB_CLICKON`).
        app.add_click_handler(ClickPhase::HeldOnTarget, 50, |w, c| {
            c.held.is_some_and(|held| {
                held_on_target(w, c.client, c.mob, held, c.target)
                    || super::aquarium::item_interaction(w, c.client, c.mob, held, c.target)
            })
        })
        .add_click_handler(ClickPhase::OtherHand, 450, |w, c| {
            c.held.is_some_and(|held| {
                insert_bait(w, c.client, c.mob, held, c.target)
                    || super::aquarium::item_interaction(w, c.client, c.mob, held, c.target)
            })
        })
        .add_click_handler(ClickPhase::AttackSelf, 3050, |w, c| {
            attack_self(w, c.client, c.mob, c.target)
                || super::aquarium::attack_self(w, c.client, c.mob, c.target)
        })
        .add_click_handler(ClickPhase::EmptyHand, 3250, |w, c| {
            super::aquarium::interact(w, c.client, c.mob, c.target)
        })
        .register_ui::<FishingMinigameUi>()
        .init_resource::<FishCounts>()
        .add_systems(
            Update,
            (
                ranged_clicks.after(ResolveClicks),
                challenges
                    .after(ResolveClicks)
                    .after(ranged_clicks)
                    .before(UiSystems::Open),
                minigame_acts.in_set(UiSystems::Handle),
            ),
        )
        .add_observer(add_rod);
        super::fish::plugin(app);
        super::aquarium::plugin(app);
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn tell(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        crate::tools::tell(world, client, span, text);
    }
}

fn sound_at(world: &mut World, source: Entity, file: &str, vol: f32) {
    if let Some(at) = crate::chem::map_pos(world, source) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(at, file, vol, false));
    }
}

fn item_path(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmItem>(e).map(|i| i.path.as_str())
}

/// `Initialize` of a rod.
fn add_rod(add: On<Add<DmFishingRod>>, rods: Query<&DmFishingRod>, mut commands: Commands) {
    if let Ok(stats) = rods.get(add.entity) {
        commands.entity(add.entity).insert_if_new(FishingRod {
            stats: stats.clone(),
            bait: None,
            cast_ready_at: 0.0,
        });
    }
}

/// The bait's weighting facts.
pub fn bait_info(world: &World, bait: Entity) -> BaitInfo {
    let path = item_path(world, bait).unwrap_or_default();
    BaitInfo {
        quality: BAIT_QUALITIES
            .iter()
            .find(|(root, _)| is_under(path, root))
            .map(|&(_, q)| q),
        foodtypes: world.get::<DmFood>(bait).map_or(0, |f| f.foodtypes),
        grown: is_under(path, GROWN_ROOT),
    }
}

/// Food used on a rod: it becomes the bait, the old bait goes to the hand (tg `use_slot`).
fn insert_bait(world: &mut World, _client: Entity, mob: Entity, held: Entity, rod: Entity) -> bool {
    if world.get::<FishingRod>(rod).is_none()
        || !item_path(world, held).is_some_and(|p| is_under(p, FOOD_ROOT))
    {
        return false;
    }
    if world.get::<Fishing>(mob).is_some_and(|f| f.rod == rod) {
        return true;
    }
    let old = world.get::<FishingRod>(rod).and_then(|r| r.bait);
    crate::storage::put_inside(world, held, rod);
    world.get_mut::<FishingRod>(rod).expect("checked").bait = Some(held);
    if let Some(old) = old
        && let Some(inventory) = world.get::<Inventory>(mob).cloned()
    {
        crate::storage::put_in_hands(world, mob, &inventory, old);
    }
    let name = crate::chem::name_of(world, held);
    tell(
        world,
        mob,
        Span::Notice,
        format!("You put {name} on the hook."),
    );
    true
}

/// The rod in hand: stops fishing (`on_attack_self`), else takes the bait off.
// shortcut: stopping in the minigame phase is immediate (tg: a 3 s `do_after`).
fn attack_self(world: &mut World, _client: Entity, mob: Entity, rod: Entity) -> bool {
    if world.get::<FishingRod>(rod).is_none() {
        return false;
    }
    if world.get::<Fishing>(mob).is_some_and(|f| f.rod == rod) {
        tell(world, mob, Span::Notice, "stopped fishing");
        complete(world, mob, false);
        return true;
    }
    let Some(bait) = world.get::<FishingRod>(rod).and_then(|r| r.bait) else {
        return true;
    };
    world.get_mut::<FishingRod>(rod).expect("checked").bait = None;
    let pos = world.get::<TilePos>(mob).copied();
    crate::storage::take_out(world, bait, pos.unwrap_or_default());
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, bait);
    }
    true
}

/// A world click with the rod (or anything else while fishing with it).
fn held_on_target(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<Fishing>(mob).is_some_and(|f| f.rod == held) {
        handle_click(world, mob);
        return true;
    }
    if world.get::<FishingRod>(target).is_some() && insert_bait(world, client, mob, held, target) {
        return true;
    }
    if world.get::<FishingRod>(held).is_none() {
        return false;
    }
    cast_line(world, mob, held, target)
}

/// Clicks past reach: a cast, or the challenge's click.
fn ranged_clicks(world: &mut World, mut cursor: Local<MessageCursor<RangedClick>>) {
    let clicks: Vec<RangedClick> = cursor
        .read(world.resource::<Messages<RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(held) = world
            .get::<Inventory>(click.mob)
            .and_then(Inventory::active_item)
        else {
            continue;
        };
        if world
            .get::<Fishing>(click.mob)
            .is_some_and(|f| f.rod == held)
        {
            handle_click(world, click.mob);
        } else if world.get::<FishingRod>(held).is_some() && !click.modifiers.right {
            cast_line(world, click.mob, held, click.target);
        }
    }
}

/// The spot a cast at `target` lands in and its source: the target, else its turf.
pub fn spot_of(world: &World, target: Entity) -> Option<(Entity, &'static FishSourceDef)> {
    if let Some(source) = own_source(world, target) {
        return Some((target, source));
    }
    let pos = *world.get::<TilePos>(target)?;
    let turf = crate::construction::turf_at(world, pos)?;
    own_source(world, turf).map(|s| (turf, s))
}

fn own_source(world: &World, e: Entity) -> Option<&'static FishSourceDef> {
    if world.get::<super::aquarium::Aquarium>(e).is_some() {
        return fish_source(AQUARIUM_SOURCE);
    }
    if let Some(portal) = world.get::<super::aquarium::FishPortal>(e) {
        return portal.active;
    }
    if world.get::<Turf>(e).is_some() {
        return turf_source(&world.get::<DmTurfBuild>(e)?.path);
    }
    None
}

/// `cast_line` + `hook_hit` + `fishing_spot/try_start_fishing`. True when the click was used.
// shortcut: the cast does not fly (no projectile, no line of sight); a cast at something that
// is not a fishing spot does nothing (tg hooks items and reels them in).
fn cast_line(world: &mut World, mob: Entity, rod: Entity, target: Entity) -> bool {
    let Some(r) = world.get::<FishingRod>(rod).cloned() else {
        return false;
    };
    let Some((spot, source)) = spot_of(world, target) else {
        return false;
    };
    let now = now(world);
    if !r.stats.has_hook {
        tell(world, mob, Span::Warning, "install a hook first!");
        return true;
    }
    if now < r.cast_ready_at {
        return true;
    }
    let (Some(at), Some(pos)) = (
        world.get::<TilePos>(mob).copied(),
        crate::chem::map_pos(world, spot),
    ) else {
        return false;
    };
    if at.chebyshev(pos).is_none_or(|d| d > r.cast_range()) {
        return false;
    }
    world
        .get_mut::<FishingRod>(rod)
        .expect("checked")
        .cast_ready_at = now + CAST_COOLDOWN;
    let wait = wait_time(world);
    world.entity_mut(mob).insert(Fishing {
        rod,
        spot,
        source,
        phase: Phase::Waiting,
        next_at: now + wait,
        reward: Reward::Dud,
        last_click: now,
        params: None,
        started_at: now,
        window_seen: false,
    });
    tell(world, mob, Span::Notice, "You start fishing...");
    sound_at(world, spot, SPLASH, 100.0);
    true
}

fn wait_time(world: &mut World) -> f64 {
    f64::from(world.resource_mut::<CombatRng>().range(WAIT_MIN, WAIT_MAX))
}

/// `handle_click`: a miss while waiting, the strike while biting.
fn handle_click(world: &mut World, mob: Entity) {
    let now = now(world);
    let Some(fishing) = world.get::<Fishing>(mob).cloned() else {
        return;
    };
    match fishing.phase {
        Phase::Waiting => {
            // A double click is forgiven.
            if now < fishing.last_click + 0.25 {
                return;
            }
            tell(world, mob, Span::Warning, "miss!");
            let extra = f64::from(world.resource_mut::<CombatRng>().range(3, 5));
            let mut f = world.get_mut::<Fishing>(mob).expect("checked");
            f.next_at = now + ((f.next_at - now).max(0.0) + extra).min(30.0);
            f.last_click = now;
            f.reward = Reward::Dud;
        }
        Phase::Biting => start_minigame(world, mob),
        Phase::Minigame => {}
    }
}

/// Every challenge: interruptions, then the phase timers.
fn challenges(world: &mut World) {
    let now = now(world);
    world.resource_mut::<FishCounts>().regenerate(now);
    let fishers: Vec<(Entity, Fishing)> = world
        .query::<(Entity, &Fishing)>()
        .iter(world)
        .map(|(e, f)| (e, f.clone()))
        .collect();
    for (mob, fishing) in fishers {
        if let Some(reason) = interruption(world, mob, &fishing) {
            tell(world, mob, Span::Warning, reason);
            complete(world, mob, false);
            continue;
        }
        match fishing.phase {
            Phase::Waiting if now >= fishing.next_at => start_biting(world, mob, &fishing, now),
            Phase::Biting if now >= fishing.next_at => {
                let wait = wait_time(world);
                let mut f = world.get_mut::<Fishing>(mob).expect("listed");
                f.phase = Phase::Waiting;
                f.next_at = now + wait;
                f.reward = Reward::Dud;
                f.last_click = now;
            }
            Phase::Minigame => {
                let open = world
                    .query::<&UiWindow>()
                    .iter(world)
                    .any(|w| w.source == mob && w.interface == FishingMinigameUi::NAME);
                if open && !fishing.window_seen {
                    world.get_mut::<Fishing>(mob).expect("listed").window_seen = true;
                } else if !open
                    && (fishing.window_seen || now > fishing.started_at + WINDOW_GRACE_SECS)
                {
                    // The window was closed: the fish gets away.
                    tell(world, mob, Span::Warning, "it got away!");
                    complete(world, mob, false);
                }
            }
            _ => {}
        }
    }
}

/// Why the line snapped, if it did (`on_line_deleted`, `on_spot_gone`, `check_los`).
fn interruption(world: &World, mob: Entity, fishing: &Fishing) -> Option<&'static str> {
    let conscious = world.get::<Stat>(mob) == Some(&Stat::Conscious);
    let holds = world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.holds(fishing.rod) && i.hands.contains(&Some(fishing.rod)));
    if !conscious || !holds {
        return Some("rod dropped");
    }
    let (Some(at), Some(spot)) = (
        world.get::<TilePos>(mob).copied(),
        crate::chem::map_pos(world, fishing.spot),
    ) else {
        return Some("fishing spot gone!");
    };
    if own_source(world, fishing.spot).is_none_or(|s| s.path != fishing.source.path) {
        return Some("fishing spot gone!");
    }
    let range = world
        .get::<FishingRod>(fishing.rod)
        .map_or(1, FishingRod::cast_range);
    if at.chebyshev(spot).is_none_or(|d| d > range) {
        return Some("line snapped");
    }
    None
}

/// `start_biting_phase`: the reward is rolled.
fn start_biting(world: &mut World, mob: Entity, fishing: &Fishing, now: f64) {
    let reward = roll_reward(world, fishing);
    {
        let mut f = world.get_mut::<Fishing>(mob).expect("listed");
        f.phase = Phase::Biting;
        f.reward = reward;
        f.next_at = now + minigame::BITING_SECS;
    }
    tell(world, mob, Span::Notice, "!!!");
    sound_at(world, fishing.spot, FISH_SPLASH, 100.0);
}

/// One row of a fish table: the reward, its weight and the fish's traits (fish only).
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub reward: Reward,
    pub weight: f64,
    pub traits: Option<Vec<String>>,
}

/// `get_fish_table` of the challenge's source at its spot.
fn fish_table(world: &World, fishing: &Fishing) -> Vec<Row> {
    let source = fishing.source;
    if source.path == AQUARIUM_SOURCE {
        let fish: Vec<Row> = world
            .get::<Stored>(fishing.spot)
            .map(|s| s.items().to_vec())
            .unwrap_or_default()
            .into_iter()
            .filter(|&e| world.get::<Fish>(e).is_some_and(|f| f.alive))
            .map(|e| Row {
                reward: Reward::Fish(e),
                weight: 10.0,
                traits: Some(
                    world
                        .get::<DmFish>(e)
                        .map(|d| d.fish_traits.clone())
                        .unwrap_or_default(),
                ),
            })
            .collect();
        if !fish.is_empty() {
            return fish;
        }
    }
    let counts = world.resource::<FishCounts>();
    let protos = world.get_resource::<ss13_proto::Prototypes>();
    source
        .table
        .iter()
        .filter(|(path, _)| counts.left(source, path).is_none_or(|n| n > 0))
        .map(|&(path, weight)| {
            if path == DUD {
                return Row {
                    reward: Reward::Dud,
                    weight: f64::from(weight),
                    traits: None,
                };
            }
            let traits = is_under(path, ss13_mapload::fishing::FISH_ROOT).then(|| {
                protos
                    .and_then(|p| fish_traits_of(p, path))
                    .unwrap_or_default()
            });
            Row {
                reward: Reward::Path(path),
                weight: f64::from(weight),
                traits,
            }
        })
        .collect()
}

/// A fish type's `fish_traits` from its prototype.
fn fish_traits_of(protos: &ss13_proto::Prototypes, path: &str) -> Option<Vec<String>> {
    protos
        .get(path)
        .and_then(|p| p.get::<DmFish>())
        .map(|f| f.fish_traits.clone())
}

/// `get_modified_fish_table` with the default hook (no bonus).
pub fn modified_table(rows: Vec<Row>, bait: Option<&BaitInfo>) -> Vec<Row> {
    let (mult, exponent) = bait
        .and_then(|b| b.quality)
        .map_or((1.0, 0.0), BaitQuality::mods);
    let mut out = Vec::new();
    for mut row in rows {
        // A bait without `TRAIT_BAIT_ALLOW_FISHING_DUD` gives the rod `TRAIT_ROD_REMOVE_FISHING_DUD`.
        if row.reward == Reward::Dud && bait.is_some() {
            continue;
        }
        if let Some(traits) = &row.traits {
            row.weight = match bait {
                Some(_) => (row.weight * mult).round(),
                None => (row.weight * WEIGHT_MULT_WITHOUT_BAIT).round(),
            };
            row.weight = (row.weight * trait_catch_mult(traits, bait)).round();
        }
        if row.weight > 0.0 {
            out.push(row);
        }
    }
    if exponent > 0.0 {
        level_out(&mut out, exponent);
    }
    out
}

/// `level_out_fish`.
fn level_out(rows: &mut [Row], exponent: f64) {
    let highest = rows
        .iter()
        .filter(|r| r.traits.is_some())
        .map(|r| r.weight)
        .fold(0.0, f64::max);
    for row in rows.iter_mut().filter(|r| r.traits.is_some()) {
        let difference = highest - row.weight;
        if difference > 0.0 {
            row.weight += difference.powf(exponent).round();
        }
    }
}

/// The traits' `catch_weight_mod` multipliers (none of them adds).
// shortcut: no `nocturnal` (no light levels), no shiny hooks or bananium/bamboo rods, no fish
// as bait.
pub fn trait_catch_mult(traits: &[String], bait: Option<&BaitInfo>) -> f64 {
    let mut mult = 1.0;
    for t in traits {
        let Some(name) = t.strip_prefix("/datum/fish_trait/") else {
            continue;
        };
        mult *= match (name, bait) {
            ("shiny_lover", _) => 0.5,
            ("picky_eater" | "carnivore" | "vegan", None) => 0.0,
            ("picky_eater", Some(b)) => {
                if matches!(b.quality, Some(BaitQuality::Good | BaitQuality::Great)) {
                    1.0
                } else {
                    0.0
                }
            }
            ("carnivore", Some(b)) => {
                if b.foodtypes & (MEAT | SEAFOOD | BUGS) != 0 {
                    1.0
                } else {
                    0.0
                }
            }
            ("vegan", Some(b)) => {
                let liked = b.grown || b.foodtypes & (VEGETABLES | FRUIT) != 0;
                if liked && (b.grown || b.foodtypes & (MEAT | SEAFOOD | GORE | BUGS | DAIRY) == 0) {
                    1.0
                } else {
                    0.0
                }
            }
            _ => 1.0,
        };
    }
    mult
}

/// `pick_weight`.
fn pick(rows: &[Row], rng: &mut CombatRng) -> Reward {
    let total: f64 = rows.iter().map(|r| r.weight).sum();
    if total <= 0.0 {
        return Reward::Dud;
    }
    let mut roll = f64::from(rng.unit()) * total;
    for row in rows {
        if roll < row.weight {
            return row.reward;
        }
        roll -= row.weight;
    }
    rows.last().map_or(Reward::Dud, |r| r.reward)
}

fn roll_reward(world: &mut World, fishing: &Fishing) -> Reward {
    let bait = world
        .get::<FishingRod>(fishing.rod)
        .and_then(|r| r.bait)
        .map(|b| bait_info(world, b));
    let rows = modified_table(fish_table(world, fishing), bait.as_ref());
    pick(&rows, &mut world.resource_mut::<CombatRng>())
}

/// `calculate_difficulty` + the skill modifier, before `get_difficulty`'s clamp.
pub fn difficulty(
    source: &FishSourceDef,
    rod: &DmFishingRod,
    fish: Option<(&DmFish, &[String])>,
) -> f32 {
    let mut d = source.fishing_difficulty + rod.difficulty_modifier;
    if let Some((fish, traits)) = fish {
        d += fish.fishing_difficulty_modifier;
        for t in traits {
            // Wary fish see any line but a cloaked one (none here).
            if t == "/datum/fish_trait/wary" {
                d += MINOR_DIFFICULTY_BOOST;
            }
        }
    }
    d + minigame::NO_SKILL_MODIFIER
}

/// The fish data of a reward, from the instance or the type's prototype.
fn reward_fish(world: &World, reward: Reward) -> Option<DmFish> {
    match reward {
        Reward::Fish(e) => world.get::<DmFish>(e).cloned(),
        Reward::Path(path) => world
            .get_resource::<ss13_proto::Prototypes>()?
            .get(path)?
            .get::<DmFish>()
            .cloned(),
        Reward::Dud => None,
    }
}

/// `start_minigame_phase`.
fn start_minigame(world: &mut World, mob: Entity) {
    let now = now(world);
    let Some(fishing) = world.get::<Fishing>(mob).cloned() else {
        return;
    };
    let Some(rod) = world.get::<FishingRod>(fishing.rod).cloned() else {
        return;
    };
    let fish = reward_fish(world, fishing.reward);
    let traits: Vec<String> = fish
        .as_ref()
        .map(|f| f.fish_traits.clone())
        .unwrap_or_default();
    let raw = difficulty(
        fishing.source,
        &rod.stats,
        fish.as_ref().map(|f| (f, traits.as_slice())),
    );
    if raw <= 0.0 {
        complete(world, mob, true);
        return;
    }
    let has = |t: &str| traits.iter().any(|x| x.ends_with(t));
    let s = Setup {
        difficulty: raw.round().clamp(MINIMUM_DIFFICULTY, 100.0),
        has_line: rod.stats.has_line,
        completion_speed_mult: rod.stats.completion_speed_mult,
        bait_speed_mult: rod.stats.bait_speed_mult,
        deceleration_mult: rod.stats.deceleration_mult,
        bounciness_mult: rod.stats.bounciness_mult,
        gravity_mult: rod.stats.gravity_mult,
        bait_height_mult: rod.stats.bait_height_mult,
        reaction_left: Some((fishing.next_at - now).max(0.0)),
        movement: fish.as_ref().map_or(Movement::Default, |f| {
            Movement::of_path(&f.fish_movement_type)
        }),
        heavy: has("/heavy"),
        lubed: has("/lubed"),
        antigrav: has("/antigrav"),
        camo: has("/camouflage"),
    };
    let mut params = {
        let mut rng = world.resource_mut::<CombatRng>();
        minigame::setup(&s, |low, high| {
            if high <= low {
                low
            } else {
                let span = (i64::from(high) - i64::from(low)) as f64 + 1.0;
                low + (f64::from(rng.unit()) * span).floor().min(span - 1.0) as i32
            }
        })
    };
    params.background = fishing.source.background.to_owned();
    {
        let mut f = world.get_mut::<Fishing>(mob).expect("checked");
        f.phase = Phase::Minigame;
        f.params = Some(params);
        f.started_at = now;
        f.window_seen = false;
    }
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        world.write_message(OpenUi::new::<FishingMinigameUi>(
            client, mob, mob, "Fishing",
        ));
    }
}

/// `complete(win)`: the challenge ends; a win dispenses the reward.
fn complete(world: &mut World, mob: Entity, win: bool) {
    let Some(fishing) = world.get::<Fishing>(mob).cloned() else {
        return;
    };
    // The window closes with it (its source lost `Fishing`).
    world.entity_mut(mob).remove::<Fishing>();
    if !win {
        return;
    }
    if fishing.reward != Reward::Dud {
        sound_at(world, fishing.spot, BIG_SPLASH, 100.0);
    }
    let caught = dispense(world, mob, &fishing);
    // `on_reward_caught`: a catch that is a live fish eats the bait.
    let live_fish = caught.is_some_and(|c| world.get::<Fish>(c).is_some_and(|f| f.alive));
    if live_fish
        && let Some(bait) = world
            .get_mut::<FishingRod>(fishing.rod)
            .and_then(|mut r| r.bait.take())
    {
        world.entity_mut(bait).despawn();
    }
}

/// `dispense_reward`: into the hands, or the dud's line.
fn dispense(world: &mut World, mob: Entity, fishing: &Fishing) -> Option<Entity> {
    let at = world.get::<TilePos>(mob).copied()?;
    let now = now(world);
    let reward = match fishing.reward {
        Reward::Dud => None,
        Reward::Path(path) => {
            if world
                .resource_mut::<FishCounts>()
                .take(fishing.source, path, now)
            {
                let e = crate::construction::spawn_obj(world, path, at);
                if let Some(e) = e {
                    super::fish::randomize_size_and_weight(world, e);
                }
                e
            } else {
                None
            }
        }
        // Still in the aquarium it was rolled in.
        Reward::Fish(e) => world
            .get::<StoredIn>(e)
            .is_some_and(|s| s.0 == fishing.spot)
            .then(|| {
                crate::storage::take_out(world, e, at);
                e
            }),
    };
    let Some(reward) = reward else {
        let duds = fishing.source.duds;
        let i = world
            .resource_mut::<CombatRng>()
            .range(0, duds.len().saturating_sub(1) as u32) as usize;
        let line = duds.get(i).copied().unwrap_or("it was nothing");
        tell(world, mob, Span::Notice, line);
        return None;
    };
    if world.get::<DmItem>(reward).is_some()
        && let Some(inventory) = world.get::<Inventory>(mob).cloned()
    {
        crate::storage::put_in_hands(world, mob, &inventory, reward);
    }
    let name = crate::chem::name_of(world, reward);
    tell(world, mob, Span::Notice, format!("caught {name}!"));
    Some(reward)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MinigameAct {
    Win,
    Lose,
}

impl ServerUi for FishingMinigameUi {
    type Source = &'static Fishing;
    type UserParam = ();
    type Act = MinigameAct;

    fn static_data(fishing: &Fishing) -> MinigameParams {
        fishing.params.clone().unwrap_or_default()
    }

    fn data(_: &Fishing, _: Entity, _: &()) {}

    fn parse_act(action: &str, _: &UiParams) -> Result<MinigameAct, UiReject> {
        match action {
            WIN => Ok(MinigameAct::Win),
            LOSE => Ok(MinigameAct::Lose),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// The client's outcome, checked against the challenge: only its fisher, only in the minigame,
/// and no win sooner than the completion gain allows.
fn minigame_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<FishingMinigameUi>>>) {
    let pending: Vec<(Entity, Entity, MinigameAct)> = acts
        .read(world.resource::<Messages<UiActed<FishingMinigameUi>>>())
        .map(|a| (a.actor, a.source, a.act))
        .collect();
    let now = now(world);
    for (actor, source, act) in pending {
        let Some(fishing) = world.get::<Fishing>(source).cloned() else {
            continue;
        };
        if actor != source || fishing.phase != Phase::Minigame {
            continue;
        }
        match act {
            MinigameAct::Lose => {
                tell(world, actor, Span::Warning, "it got away!");
                complete(world, actor, false);
            }
            MinigameAct::Win => {
                let min = fishing.params.as_ref().map_or(0.0, minigame::min_win_secs);
                if now - fishing.started_at + 0.05 >= min {
                    complete(world, actor, true);
                } else {
                    debug!(
                        "fishing win from {actor} after {:.2}s refused",
                        now - fishing.started_at
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fish(weight: f64, traits: &[&str]) -> Row {
        Row {
            reward: Reward::Path("/obj/item/fish/goldfish"),
            weight,
            traits: Some(traits.iter().map(|t| (*t).to_owned()).collect()),
        }
    }

    fn dud(weight: f64) -> Row {
        Row {
            reward: Reward::Dud,
            weight,
            traits: None,
        }
    }

    #[test]
    fn no_bait_mostly_duds_and_bait_removes_them() {
        let rows = vec![dud(4.0), fish(5.0, &[]), fish(4.0, &[])];
        let bare = modified_table(rows.clone(), None);
        // round(5 * 0.15) = 1, round(4 * 0.15) = 1.
        assert_eq!(
            bare.iter().map(|r| r.weight).collect::<Vec<_>>(),
            [4.0, 1.0, 1.0]
        );
        let worm = BaitInfo {
            quality: Some(BaitQuality::Basic),
            foodtypes: MEAT | BUGS,
            grown: false,
        };
        let baited = modified_table(rows, Some(&worm));
        assert!(baited.iter().all(|r| r.reward != Reward::Dud));
        // x2, then levelled out by 0.4: 10 and 8 + round(2^0.4) = 9.
        assert_eq!(
            baited.iter().map(|r| r.weight).collect::<Vec<_>>(),
            [10.0, 9.0]
        );
    }

    #[test]
    fn traits_gate_the_bait() {
        let carnivore = ["/datum/fish_trait/carnivore".to_owned()];
        let dough = BaitInfo {
            quality: Some(BaitQuality::Basic),
            foodtypes: 0,
            grown: false,
        };
        let worm = BaitInfo {
            foodtypes: MEAT,
            ..dough.clone()
        };
        assert_eq!(trait_catch_mult(&carnivore, Some(&dough)), 0.0);
        assert_eq!(trait_catch_mult(&carnivore, Some(&worm)), 1.0);
        let picky = ["/datum/fish_trait/picky_eater".to_owned()];
        assert_eq!(trait_catch_mult(&picky, Some(&worm)), 0.0);
        let premium = BaitInfo {
            quality: Some(BaitQuality::Good),
            ..worm
        };
        assert_eq!(trait_catch_mult(&picky, Some(&premium)), 1.0);
        let shiny = ["/datum/fish_trait/shiny_lover".to_owned()];
        assert_eq!(trait_catch_mult(&shiny, None), 0.5);
    }

    #[test]
    fn difficulty_adds_source_rod_fish_and_traits() {
        let source = fish_source("/datum/fish_source/river").unwrap();
        let rod = DmFishingRod {
            difficulty_modifier: -5.0,
            ..default()
        };
        let pike = DmFish {
            fishing_difficulty_modifier: 10.0,
            ..default()
        };
        let wary = ["/datum/fish_trait/wary".to_owned()];
        // 30 - 5 + 10 + 5 + 1 (no fishing skill).
        assert_eq!(difficulty(source, &rod, Some((&pike, &wary))), 41.0);
        assert_eq!(difficulty(source, &rod, None), 26.0);
    }

    #[test]
    fn limited_rewards_run_out_and_come_back() {
        let source = fish_source("/datum/fish_source/river").unwrap();
        let pike = "/obj/item/fish/pike";
        let mut counts = FishCounts::default();
        assert_eq!(counts.left(source, pike), Some(3));
        for _ in 0..3 {
            assert!(counts.take(source, pike, 0.0));
        }
        assert!(!counts.take(source, pike, 0.0));
        assert_eq!(counts.left(source, pike), Some(0));
        counts.regenerate(240.0);
        assert_eq!(counts.left(source, pike), Some(3));
        assert!(counts.take(source, "/obj/item/fish/goldfish", 0.0));
    }
}
