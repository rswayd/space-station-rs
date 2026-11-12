//! Giant spiders as a ghost antagonist (server), after tg `/datum/antagonist/spider`
//! (`code/modules/antagonists/spiders/spiders.dm`), the egg clusters
//! (`code/modules/mob_spawn/ghost_roles/spider_roles.dm`), the spiders' life stages
//! (`mob/living/basic/space_fauna/spider/`: `growing`, `growth_and_differentiation`), their
//! abilities (`spider_abilities/`: webs, eggs, wrap, directive, commands) and the `Spiders`
//! midround ruleset.
//!
//! - Egg clusters ([`SpiderEggs`]) grow by `rand(5, 15)` per second; at 100 the ghosts are told
//!   and the cluster is a spawner of the spawners menu (`crate::ghost_verbs`). Taking it asks
//!   for the spiderling (a list for tg's radial menu, when the cluster has several) and hatches
//!   it for the ghost: the spider antagonist with its mother's directive (the flesh spider's own
//!   objectives), the cluster gone.
//! - Spiderlings grow into young spiders after 40 s and those into giant spiders after a minute
//!   (or by the 25 %-per-second growth rolls first), on a turf, alive; the player, the directive
//!   and the damage go along.
//! - A player's spider bites what it clicks (with its venom) and has its buttons: Spin Web
//!   (sealing a web into a wall for nurses, tanglers, tarantulas and broodmothers), solid and
//!   sticky webs, Lay Eggs (normal, abnormal, enriched: one per human eaten), Wrap (a click on a
//!   mob next to it: 5 s, a cocoon; a human is eaten), Set Directive and the spider commands.
//! - The `Spiders` ruleset hides two midwife egg clusters, nearly grown, in dark maintenance; the
//!   station hears of it 375-600 s later.
// shortcut: no web passages, spikes, reflectors or effigies, no sneaking, charging, defensive
// mode, wall tearing, healing touch or web walking speed; no taming, riding or vent crawling of
// player spiders; items are not cocooned and nobody breaks out of a cocoon (it must be broken);
// the spawners menu lists a cluster once it is ready (tg also shows growing ones); no hivemind
// TTS; no atmos check of the maintenance spawn and no false alarm.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_mapload::fidelity::is_under;
use ss13_net::server::ClientToken;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::{UiActed, UiSystems};

use crate::actions::{ActionButton, ActionButtonsUi, set_buttons};
use crate::click::{ClickIntercept, ClickResolved, InterceptedClick};
use crate::construction::{Job, path_of, spawn_obj};
use crate::ghost::Ghost;
use crate::ghost_verbs::GhostRoleSpawner;
use crate::health::{DamageType, HEALTH_THRESHOLD_DEAD, Health, Stat, apply_damage};
use crate::mobs::BasicMob;
use crate::server::PlayerMob;
use crate::tools::{Actor, DoAfter, do_after, now, tell, tell_mob};

pub const SPIDER: &str = "/mob/living/basic/spider";
pub const GIANT: &str = "/mob/living/basic/spider/giant";
const SPIDERLING: &str = "/mob/living/basic/spider/growing/spiderling";
const YOUNG: &str = "/mob/living/basic/spider/growing/young";
const COCOON: &str = "/obj/structure/spider/cocoon";
const STICKYWEB: &str = crate::effects::webs::STICKYWEB;
const SEALED: &str = "/obj/structure/spider/stickyweb/sealed";
const TOUGH: &str = "/obj/structure/spider/stickyweb/sealed/tough";
const VERY_STICKY: &str = "/obj/structure/spider/stickyweb/very_sticky";
const ANIMAL_DMI: &str = "icons/mob/actions/actions_animal.dmi";
const BACKGROUNDS_DMI: &str = "icons/mob/actions/backgrounds.dmi";
const EFFECTS_DMI: &str = "icons/effects/effects.dmi";
/// `egg_lay_time`, `wrap_time`.
const EGG_LAY_DS: f32 = 40.0;
const WRAP_DS: f32 = 50.0;
/// The ruleset's `egg_count` and the eggs' `amount_grown`.
const RULESET_EGGS: usize = 2;
const RULESET_GROWN: f32 = 98.0;
/// `spider_growth_time` of spiderlings and of the rest.
const SPIDERLING_GROWTH_SECS: f64 = 40.0;
const GROWTH_SECS: f64 = 60.0;
const GROWTH_PROBABILITY: f32 = 25.0;

/// The kinds of egg cluster (`/obj/effect/mob_spawn/ghost_role/spider/*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EggKind {
    Normal,
    Abnormal,
    Enriched,
    Midwife,
    Bloody,
}

impl EggKind {
    /// `cluster_type`.
    pub fn cluster(self) -> &'static str {
        match self {
            Self::Normal => "/obj/structure/spider/eggcluster",
            Self::Abnormal => "/obj/structure/spider/eggcluster/abnormal",
            Self::Enriched => "/obj/structure/spider/eggcluster/enriched",
            Self::Midwife => "/obj/structure/spider/eggcluster/midwife",
            Self::Bloody => "/obj/structure/spider/eggcluster/bloody",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Normal => "egg cluster",
            Self::Abnormal => "abnormal egg cluster",
            Self::Enriched => "enriched egg cluster",
            Self::Midwife => "midwife egg cluster",
            Self::Bloody => "bloody egg cluster",
        }
    }

    fn you_are(self) -> &'static str {
        match self {
            Self::Enriched => "You are an enriched spider.",
            Self::Bloody => "You are a flesh spider.",
            Self::Midwife => "You are a midwife spider.",
            _ => "You are a spider.",
        }
    }

    fn flavour(self) -> &'static str {
        match self {
            Self::Bloody => {
                "An abomination of nature set upon the station by changelings. Your only goal is \
                 to kill, terrorize, and survive."
            }
            Self::Midwife => {
                "The crux of the spider horde. You have the ability to reproduce and create more \
                 spiders, and turn victims into special spider eggs."
            }
            _ => {
                "For the hive! Choose a spider and fulfill your role to take over the station... \
                 if that is within your directives, of course."
            }
        }
    }

    /// `potentialspawns`.
    pub fn spawns(self) -> &'static [&'static str] {
        match self {
            Self::Normal => &[
                "/mob/living/basic/spider/growing/spiderling/nurse",
                "/mob/living/basic/spider/growing/spiderling/hunter",
                "/mob/living/basic/spider/growing/spiderling/ambush",
                "/mob/living/basic/spider/growing/spiderling/tangle",
                "/mob/living/basic/spider/growing/spiderling/guard",
                "/mob/living/basic/spider/growing/spiderling/scout",
            ],
            Self::Abnormal => &[
                "/mob/living/basic/spider/growing/spiderling/tank",
                "/mob/living/basic/spider/growing/spiderling/viper",
            ],
            Self::Enriched => &[
                "/mob/living/basic/spider/growing/spiderling/tarantula",
                "/mob/living/basic/spider/growing/spiderling/breacher",
                "/mob/living/basic/spider/growing/spiderling/midwife",
            ],
            Self::Midwife => &["/mob/living/basic/spider/growing/spiderling/midwife"],
            Self::Bloody => &[crate::mobs::space_fauna::FLESH_SPIDER],
        }
    }
}

/// An egg cluster: `amount_grown`, `ready` and the mother's directive.
#[derive(Component, Clone, Debug)]
pub struct SpiderEggs {
    pub kind: EggKind,
    pub amount_grown: f32,
    pub ready: bool,
    pub directive: String,
}

/// `/datum/antagonist/spider` (or `/flesh`) on a player's spider.
#[derive(Component, Clone, Debug, Default)]
pub struct Spider {
    pub key: String,
    pub directive: String,
    pub flesh: bool,
}

/// A spider's ability state: cooldowns (game time they end), the broodmother's directive and
/// enriched egg charges, an armed Wrap.
#[derive(Component, Clone, Debug, Default)]
pub struct SpiderAbilities {
    pub ready_at: Vec<(&'static str, f64)>,
    pub current_directive: String,
    pub enriched_charges: u32,
    pub wrapping: bool,
}

impl SpiderAbilities {
    fn ready(&self, id: &str, t: f64) -> bool {
        self.ready_at.iter().all(|(i, at)| *i != id || *at <= t)
    }

    fn cool(&mut self, id: &'static str, until: f64) {
        self.ready_at.retain(|(i, _)| *i != id);
        self.ready_at.push((id, until));
    }
}

/// A spider growing up (`growth_and_differentiation`): when, and the rolled percent.
#[derive(Component, Clone, Copy, Debug)]
pub struct SpiderGrowth {
    pub at: f64,
    pub percent: f32,
}

/// A cocoon: what it holds goes out when it is gone.
#[derive(Component, Clone, Copy, Debug)]
pub struct Cocoon;

/// The spiders' timed steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpiderJob {
    Web(WebKind),
    Eggs(EggKind),
    Wrap,
}

/// `lay_web` and its subtypes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebKind {
    Plain,
    /// `lay_web/sealer`: a web on a web seals it.
    Sealer,
    Solid,
    Sticky,
}

impl WebKind {
    fn id(self) -> &'static str {
        match self {
            Self::Plain | Self::Sealer => "spider_web",
            Self::Solid => "spider_solid_web",
            Self::Sticky => "spider_sticky_web",
        }
    }

    /// `webbing_time` (deciseconds) and `cooldown_time` (seconds).
    fn times(self) -> (f32, f64) {
        match self {
            Self::Plain | Self::Sealer => (40.0, 0.0),
            Self::Solid => (50.0, 0.0),
            Self::Sticky => (30.0, 20.0),
        }
    }

    /// `obstructed_by_other_web`.
    fn obstruction(self) -> &'static str {
        match self {
            Self::Plain => STICKYWEB,
            Self::Sealer => SEALED,
            Self::Solid => TOUGH,
            Self::Sticky => VERY_STICKY,
        }
    }
}

/// `web_speed` and the `web_type` of a spider type.
fn web_of(path: &str) -> (f32, WebKind) {
    let any = |kinds: &[&str]| {
        kinds.iter().any(|k| {
            is_under(path, &format!("{GIANT}/{k}")) || is_under(path, &format!("{YOUNG}/{k}"))
        })
    };
    let speed = if is_under(path, &format!("{GIANT}/tarantula")) {
        0.7
    } else if is_under(path, &format!("{GIANT}/midwife")) || any(&["midwife"]) {
        0.5
    } else if is_under(path, &format!("{YOUNG}/nurse")) {
        0.5
    } else if any(&["nurse", "tangle", "breacher"]) {
        0.25
    } else {
        1.0
    };
    let sealer =
        any(&["nurse", "tangle", "midwife"]) || is_under(path, &format!("{GIANT}/tarantula"));
    (
        speed,
        if sealer {
            WebKind::Sealer
        } else {
            WebKind::Plain
        },
    )
}

/// `poison_type`, `poison_per_bite` of a spider type.
fn venom_of(path: &str) -> Option<(&'static str, f32)> {
    const HUNTER: &str = "/datum/reagent/toxin/hunterspider";
    const CONFUSE: &str = "/datum/reagent/peaceborg/confuse";
    const ACID: &str = "/datum/reagent/toxin/acid";
    const VIPER: &str = "/datum/reagent/toxin/viperspider";
    const TABLE: [(&str, &str, &str, f32); 8] = [
        (GIANT, "hunter", HUNTER, 5.0),
        (GIANT, "scout", CONFUSE, 10.0),
        (GIANT, "tangle", ACID, 2.5),
        (GIANT, "viper", VIPER, 5.0),
        (YOUNG, "hunter", HUNTER, 2.0),
        (YOUNG, "scout", CONFUSE, 4.0),
        (YOUNG, "tangle", ACID, 2.0),
        (YOUNG, "viper", VIPER, 2.0),
    ];
    TABLE
        .iter()
        .find(|(stage, kind, ..)| is_under(path, &format!("{stage}/{kind}")))
        .map(|&(_, _, reagent, units)| (reagent, units))
}

/// The extra buttons of a spider type (`innate_actions`).
fn extras_of(path: &str) -> &'static [&'static str] {
    let giant = |k: &str| is_under(path, &format!("{GIANT}/{k}"));
    if giant("midwife") {
        &[
            "spider_solid_web",
            "spider_sticky_web",
            "spider_lay_eggs",
            "spider_lay_abnormal_eggs",
            "spider_lay_enriched_eggs",
            "spider_wrap",
            "spider_directive",
            "spider_command",
        ]
    } else if giant("tangle") {
        &["spider_solid_web", "spider_sticky_web"]
    } else if giant("tarantula") {
        &["spider_solid_web"]
    } else if giant("breacher") {
        &["spider_solid_web", "spider_warning"]
    } else if giant("scout") {
        &["spider_communication"]
    } else if is_under(path, crate::mobs::space_fauna::FLESH_SPIDER) {
        &["spider_sticky_web"]
    } else {
        &[]
    }
}

fn mob_path(world: &World, mob: Entity) -> Option<String> {
    world.get::<BasicMob>(mob).map(|b| b.def.path.clone())
}

fn is_spider_path(path: &str) -> bool {
    is_under(path, SPIDER) || is_under(path, crate::mobs::space_fauna::FLESH_SPIDER)
}

fn icon(world: &World, dmi: &str, state: &str) -> Option<ss13_shared::IconStateId> {
    world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(dmi, state))
}

fn rand_range(world: &mut World, low: u32, high: u32) -> u32 {
    world
        .resource_mut::<crate::combat::CombatRng>()
        .range(low, high)
}

// ---------------------------------------------------------------------------------------------
// Eggs

/// `new egg_type(loc)`: an egg cluster with the mother's directive.
pub fn lay_cluster(
    world: &mut World,
    kind: EggKind,
    pos: TilePos,
    directive: String,
    amount_grown: f32,
) -> Option<Entity> {
    let egg = spawn_obj(world, kind.cluster(), pos)?;
    let desc = format!("{} {}", kind.you_are(), kind.flavour());
    world.entity_mut(egg).insert((
        SpiderEggs {
            kind,
            amount_grown,
            ready: false,
            directive,
        },
        GhostRoleSpawner {
            name: kind.name().to_owned(),
            desc,
            uses: 0,
            def: None,
        },
    ));
    Some(egg)
}

/// `mob_spawn/ghost_role/spider/process`: frame-independent growth; ready at 100, the ghosts are
/// told and the spawner opens.
// Cost: per tick, one pass over the egg clusters (a handful).
fn grow_eggs(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs();
    let eggs: Vec<Entity> = world
        .query_filtered::<Entity, With<SpiderEggs>>()
        .iter(world)
        .collect();
    for egg in eggs {
        let roll = rand_range(world, 5, 15) as f32;
        let Some(mut e) = world.get_mut::<SpiderEggs>(egg) else {
            continue;
        };
        if e.ready {
            continue;
        }
        e.amount_grown += roll * dt;
        if e.amount_grown < 100.0 {
            continue;
        }
        e.ready = true;
        let name = e.kind.name();
        if let Some(mut s) = world.get_mut::<GhostRoleSpawner>(egg) {
            s.uses = 1;
        }
        for ghost in crate::ghost_poll::ghosts(world) {
            tell_mob(
                world,
                ghost,
                Span::Notice,
                format!("Spider Infestation: {name} is ready to hatch!"),
            );
        }
    }
}

/// The spiderling choice of a ghost at a cluster (tg `get_radial_choice`).
#[derive(Component, Clone, Copy, Debug)]
pub struct HatchPrompt {
    ghost: Entity,
    egg: Entity,
}

pub fn is_egg(world: &World, spawner: Entity) -> bool {
    world.get::<SpiderEggs>(spawner).is_some()
}

/// The menu name of a spiderling: the giant spider it grows into (`initial(spider.name)`).
fn giant_name(world: &World, spiderling: &str) -> String {
    let giant = grown_path(grown_path(spiderling.to_owned()).unwrap_or_default())
        .unwrap_or_else(|| spiderling.to_owned());
    world
        .get_resource::<crate::mobs::MobDefs>()
        .and_then(|d| d.0.get(&giant).map(|m| m.name.clone()))
        .unwrap_or(giant)
}

/// `attack_ghost` -> `allow_spawn` + `pre_ghost_take` of a cluster: the choice, or the hatching
/// with its only spiderling.
pub fn take_egg(world: &mut World, client: Entity, ghost: Entity, egg: Entity) {
    let Some(eggs) = world.get::<SpiderEggs>(egg).cloned() else {
        return;
    };
    if !eggs.ready {
        tell(
            world,
            client,
            Span::Warning,
            format!("The {} is not ready to hatch yet!", eggs.kind.name()),
        );
        return;
    }
    let spawns = eggs.kind.spawns();
    if spawns.len() == 1 {
        hatch(world, client, ghost, egg, spawns[0]);
        return;
    }
    let items: Vec<String> = spawns.iter().map(|p| giant_name(world, p)).collect();
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            ghost,
            eggs.kind.name(),
            "",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(HatchPrompt { ghost, egg });
}

fn hatch_picked(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.answer.clone()))
        .collect();
    for (prompt, client, answer) in pending {
        let Some(asked) = world.get::<HatchPrompt>(prompt).copied() else {
            continue;
        };
        let (Some(PromptAnswer::Item(i)), Some(kind)) =
            (answer, world.get::<SpiderEggs>(asked.egg).map(|e| e.kind))
        else {
            continue;
        };
        if let Some(&path) = kind.spawns().get(i) {
            hatch(world, client, asked.ghost, asked.egg, path);
        }
    }
}

/// `create` + `special`: the spiderling for the ghost's player, the antagonist, the cluster
/// gone. Rechecked: the cluster ready, the ghost still the player's.
fn hatch(world: &mut World, client: Entity, ghost: Entity, egg: Entity, path: &str) {
    let Some(eggs) = world.get::<SpiderEggs>(egg).cloned() else {
        return;
    };
    if !eggs.ready
        || world.get::<Ghost>(ghost).is_none()
        || world.get::<PlayerMob>(ghost).map(|p| p.client) != Some(client)
    {
        return;
    }
    let Some(pos) = world.get::<TilePos>(egg).copied() else {
        return;
    };
    let Some(mob) = crate::mobs::spawn::spawn_mob(world, path, pos) else {
        warn!("no prototype for {path}");
        return;
    };
    if !crate::medical::brainmob::possess(world, client, ghost, mob) {
        world.despawn(mob);
        return;
    }
    world.despawn(ghost);
    world.despawn(egg);
    crate::noncrew_jobs::set_assigned_role(world, mob, "/datum/job/spider");
    for (span, text) in [
        (Span::BoldNotice, eggs.kind.you_are()),
        (Span::Notice, eggs.kind.flavour()),
        (Span::Danger, "Follow your directives at all costs."),
    ] {
        tell(world, client, span, text);
    }
    make_spider(world, mob, eggs.directive, eggs.kind == EggKind::Bloody);
    info!("client {client} hatched as {path} at {pos:?}");
}

/// `add_antag_datum(/datum/antagonist/spider)`: the objective and the greeting.
pub fn make_spider(world: &mut World, mob: Entity, directive: String, flesh: bool) {
    let key = world
        .get::<PlayerMob>(mob)
        .and_then(|p| world.get::<ClientToken>(p.client))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let objectives = if flesh {
        vec![
            "Wreak havoc and consume living flesh.".to_owned(),
            "Остаться в живых.".to_owned(),
        ]
    } else if directive.is_empty() {
        vec!["Spread the infestation.".to_owned()]
    } else {
        vec![format!(
            "Your queen has given you a directive! Follow it at all costs: {directive}"
        )]
    };
    world.entity_mut(mob).insert((
        Spider {
            key,
            directive,
            flesh,
        },
        SpiderAbilities::default(),
    ));
    let name = if flesh { "Flesh Spider" } else { "Spider" };
    tell_mob(world, mob, Span::Bold, format!("Вы - {name}."));
    tell_mob(world, mob, Span::Notice, "Ваши текущие задачи:");
    for (i, text) in objectives.iter().enumerate() {
        tell_mob(
            world,
            mob,
            Span::Plain,
            format!("Задача #{}: {text}", i + 1),
        );
    }
    if flesh {
        tell_mob(
            world,
            mob,
            Span::BoldWarning,
            "An abomination of flesh set upon the station by changelings, you are aggressive to \
             all living beings outside of your species and know no loyalties... even to your \
             creator. Your malleable flesh quickly regenerates if you can avoid taking damage \
             for a few seconds.",
        );
    }
    refresh_buttons(world, mob);
}

// ---------------------------------------------------------------------------------------------
// Growing up

/// `grow_as` of a growing spider: spiderling to young, young to giant; `None` for a giant.
fn grown_path(path: String) -> Option<String> {
    if let Some(rest) = path.strip_prefix(SPIDERLING) {
        return Some(format!("{YOUNG}{rest}"));
    }
    path.strip_prefix(YOUNG)
        .map(|rest| format!("{GIANT}{rest}"))
}

/// `grow_up` of a base spiderling or young spider: 3 % a rare giant, else a common one.
fn random_giant(world: &mut World) -> String {
    const RARE: [&str; 3] = ["tarantula", "viper", "midwife"];
    const COMMON: [&str; 6] = ["guard", "ambush", "hunter", "scout", "nurse", "tangle"];
    let rare = world.resource_mut::<crate::combat::CombatRng>().chance(3.0);
    let kind = if rare {
        RARE[rand_range(world, 0, 2) as usize]
    } else {
        COMMON[rand_range(world, 0, 5) as usize]
    };
    format!("{GIANT}/{kind}")
}

/// A new growing spider starts its growth (`/mob/living/basic/spider/growing/Initialize`).
pub fn attach(world: &mut World, mob: Entity, def: &ss13_mapload::mobs::MobDef) {
    let secs = if is_under(&def.path, SPIDERLING) {
        SPIDERLING_GROWTH_SECS
    } else if is_under(&def.path, YOUNG) {
        GROWTH_SECS
    } else {
        return;
    };
    let at = now(world) + secs;
    world
        .entity_mut(mob)
        .insert(SpiderGrowth { at, percent: 0.0 });
}

/// `growth_and_differentiation/process` and its timer.
// Cost: per tick, one pass over the growing spiders.
fn grow_spiders(world: &mut World) {
    let t = now(world);
    let dt = world.resource::<Time>().delta_secs();
    let mobs: Vec<(Entity, SpiderGrowth)> = world
        .query::<(Entity, &SpiderGrowth)>()
        .iter(world)
        .map(|(e, g)| (e, *g))
        .collect();
    // `SPT_PROB(25, seconds_per_tick)`.
    let chance = (1.0 - (1.0 - GROWTH_PROBABILITY / 100.0).powf(dt)) * 100.0;
    for (mob, growth) in mobs {
        if world.get::<Stat>(mob) == Some(&Stat::Dead) {
            world.entity_mut(mob).remove::<SpiderGrowth>();
            continue;
        }
        let mut percent = growth.percent;
        if percent < 100.0
            && world
                .resource_mut::<crate::combat::CombatRng>()
                .chance(chance)
        {
            percent += rand_range(world, 1, 2) as f32;
            if let Some(mut g) = world.get_mut::<SpiderGrowth>(mob) {
                g.percent = percent;
            }
        }
        // `ready_to_grow`: on a turf.
        if (t >= growth.at || percent >= 100.0)
            && world.get::<crate::closets::InCloset>(mob).is_none()
        {
            grow_up(world, mob);
        }
    }
}

/// `grow_up`: the next stage on the same turf with the damage, the directive and the player.
fn grow_up(world: &mut World, mob: Entity) {
    let (Some(path), Some(&pos)) = (mob_path(world, mob), world.get::<TilePos>(mob)) else {
        return;
    };
    let next = match grown_path(path.clone()) {
        Some(next) if next != GIANT && next != YOUNG => next,
        _ => random_giant(world),
    };
    let Some(grown) = crate::mobs::spawn::spawn_mob(world, &next, pos) else {
        warn!("no prototype for {next}");
        world.entity_mut(mob).remove::<SpiderGrowth>();
        return;
    };
    if let (Some(old), Some(mut new)) = (
        world.get::<Health>(mob).cloned(),
        world.get::<Health>(grown).cloned(),
    ) {
        new.adjust(DamageType::Brute, old.brute);
        new.adjust(DamageType::Burn, old.burn);
        let mut stat = world.get::<Stat>(grown).copied().unwrap_or_default();
        crate::health::update_stat(&new, &mut stat);
        world.entity_mut(grown).insert((new, stat));
    }
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        crate::mind::transfer(world, mob, grown);
        crate::medical::brainmob::possess(world, client, mob, grown);
    }
    if let Some(spider) = world.entity_mut(mob).take::<Spider>() {
        world.entity_mut(grown).insert(spider);
    }
    if let Some(abilities) = world.entity_mut(mob).take::<SpiderAbilities>() {
        world.entity_mut(grown).insert(abilities);
    }
    world.despawn(mob);
    refresh_buttons(world, grown);
    info!("{mob} grew into {next} ({grown})");
}

// ---------------------------------------------------------------------------------------------
// Buttons

fn button_name(id: &str) -> (&'static str, &'static str) {
    match id {
        "spider_web" => ("Spin Web", "spider_web"),
        "spider_solid_web" => ("Spin Solid Web", "spider_wall"),
        "spider_sticky_web" => ("Spin Sticky Web", "spider_ropes"),
        "spider_lay_eggs" => ("Lay Eggs", "lay_eggs"),
        "spider_lay_abnormal_eggs" => ("Lay Abnormal Eggs", "lay_abnormal_eggs"),
        "spider_lay_enriched_eggs" => ("Lay Enriched Eggs", "lay_enriched_eggs"),
        "spider_wrap" => ("Wrap", "wrap_0"),
        "spider_directive" => ("Set Directive", "directive"),
        "spider_command" => ("Command", "command"),
        "spider_communication" => ("Communication", "message"),
        "spider_warning" => ("Warning", "warning"),
        _ => ("", ""),
    }
}

/// The player spider's buttons from its type and state.
pub fn refresh_buttons(world: &mut World, mob: Entity) {
    let (Some(path), Some(abilities)) = (
        mob_path(world, mob),
        world.get::<SpiderAbilities>(mob).cloned(),
    ) else {
        return;
    };
    let t = now(world);
    let alive = world.get::<Stat>(mob) == Some(&Stat::Conscious);
    let background = icon(world, BACKGROUNDS_DMI, "bg_alien");
    let border = icon(world, BACKGROUNDS_DMI, "bg_alien_border");
    let mut ids: Vec<&'static str> = vec!["spider_web"];
    ids.extend(extras_of(&path));
    let buttons = ids
        .into_iter()
        .map(|id| {
            let (name, state) = button_name(id);
            let state = if id == "spider_wrap" && abilities.wrapping {
                "wrap_1"
            } else {
                state
            };
            let ready_at = abilities
                .ready_at
                .iter()
                .find(|(i, _)| *i == id)
                .map_or(0.0, |(_, at)| *at);
            let maptext =
                (id == "spider_lay_enriched_eggs").then(|| abilities.enriched_charges.to_string());
            (
                ActionButton {
                    id: id.to_owned(),
                    name: name.to_owned(),
                    icon: icon(world, ANIMAL_DMI, state),
                    background,
                    border,
                    available: alive,
                    cooldown: (ready_at - t).max(0.0) as f32,
                    active: id == "spider_wrap" && abilities.wrapping,
                    maptext,
                },
                ready_at,
            )
        })
        .collect();
    set_buttons(world, mob, buttons);
}

/// What a text prompt of a spider is for.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpiderText {
    Directive,
    /// The prefix of the message to all spiders.
    Command(&'static str),
}

fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world.get::<SpiderAbilities>(actor).is_none()
            || !id.starts_with("spider_")
        {
            continue;
        }
        // `AB_CHECK_CONSCIOUS`.
        if world.get::<Stat>(actor) != Some(&Stat::Conscious) {
            continue;
        }
        let path = mob_path(world, actor).unwrap_or_default();
        let allowed = id == "spider_web" || extras_of(&path).iter().any(|e| *e == id);
        if !allowed {
            continue;
        }
        let t = now(world);
        if !world
            .get::<SpiderAbilities>(actor)
            .is_some_and(|a| a.ready(button_static(&id), t))
        {
            continue;
        }
        let actor_ref = Actor {
            client,
            mob: actor,
            item: None,
        };
        match id.as_str() {
            "spider_web" => start_web(world, actor_ref, web_of(&path).1, web_of(&path).0),
            "spider_solid_web" => start_web(world, actor_ref, WebKind::Solid, 1.0),
            "spider_sticky_web" => start_web(world, actor_ref, WebKind::Sticky, 1.0),
            "spider_lay_eggs" => start_eggs(world, actor_ref, EggKind::Normal),
            "spider_lay_abnormal_eggs" => start_eggs(world, actor_ref, EggKind::Abnormal),
            "spider_lay_enriched_eggs" => start_eggs(world, actor_ref, EggKind::Enriched),
            "spider_wrap" => toggle_wrap(world, actor),
            "spider_directive" => {
                let current = world
                    .get::<SpiderAbilities>(actor)
                    .map(|a| a.current_directive.clone())
                    .unwrap_or_default();
                open_text(
                    world,
                    client,
                    actor,
                    SpiderText::Directive,
                    "Create directive",
                    "Enter the new directive",
                    current,
                );
            }
            "spider_command" => open_text(
                world,
                client,
                actor,
                SpiderText::Command("Command from"),
                "Command",
                "Input a command for your legions to follow.",
                String::new(),
            ),
            "spider_communication" => open_text(
                world,
                client,
                actor,
                SpiderText::Command("Report from"),
                "Command",
                "Input a command for your legions to follow.",
                String::new(),
            ),
            "spider_warning" => open_text(
                world,
                client,
                actor,
                SpiderText::Command("Warning from"),
                "Command",
                "Input a command for your legions to follow.",
                String::new(),
            ),
            _ => {}
        }
    }
}

/// The `'static` id of a button id string (cooldown keys).
fn button_static(id: &str) -> &'static str {
    const IDS: [&str; 11] = [
        "spider_web",
        "spider_solid_web",
        "spider_sticky_web",
        "spider_lay_eggs",
        "spider_lay_abnormal_eggs",
        "spider_lay_enriched_eggs",
        "spider_wrap",
        "spider_directive",
        "spider_command",
        "spider_communication",
        "spider_warning",
    ];
    IDS.into_iter().find(|i| *i == id).unwrap_or("")
}

fn open_text(
    world: &mut World,
    client: Entity,
    mob: Entity,
    kind: SpiderText,
    title: &str,
    message: &str,
    default: String,
) {
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            title,
            message,
            PromptKind::Text {
                default,
                max_length: ss13_shared::chat::MAX_MESSAGE_LEN as u32,
                multiline: false,
            },
        ),
    );
    world.entity_mut(prompt).insert(kind);
}

/// The directive set, or the message to every spider (and the ghosts) sent; the spider still
/// conscious.
fn text_answered(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, mob, answer) in pending {
        let Some(kind) = world.get::<SpiderText>(prompt).copied() else {
            continue;
        };
        let Some(PromptAnswer::Text(text)) = answer else {
            continue;
        };
        if world.get::<Stat>(mob) != Some(&Stat::Conscious)
            || world.get::<SpiderAbilities>(mob).is_none()
        {
            continue;
        }
        match kind {
            SpiderText::Directive => {
                info!("{mob} set its directive to: '{text}'");
                if let Some(mut a) = world.get_mut::<SpiderAbilities>(mob) {
                    a.current_directive = text;
                }
            }
            SpiderText::Command(prefix) => {
                if text.is_empty() {
                    continue;
                }
                let name = crate::security::name_of(world, mob);
                let line = format!("{prefix} {name}: {text}");
                info!(target: "say", "spider command {name}: {text}");
                let spiders: Vec<Entity> = world
                    .query::<(Entity, &BasicMob)>()
                    .iter(world)
                    .filter(|(_, b)| is_spider_path(&b.def.path))
                    .map(|(e, _)| e)
                    .collect();
                let span = match prefix {
                    "Command from" => Span::Danger,
                    "Warning from" => Span::Warning,
                    _ => Span::Notice,
                };
                for spider in spiders.into_iter().chain(crate::ghost_poll::ghosts(world)) {
                    tell_mob(world, spider, span, line.clone());
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Webs, eggs, wrapping

fn on_tile(world: &World, pos: TilePos, path: &str) -> Option<Entity> {
    world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| path_of(world, e).is_some_and(|p| is_under(&p, path)))
}

/// `lay_web/IsAvailable` + `Activate`: the `do_after` on the spider's turf.
fn start_web(world: &mut World, actor: Actor, kind: WebKind, speed: f32) {
    let Some(&pos) = world.get::<TilePos>(actor.mob) else {
        return;
    };
    if world.get::<DoAfter>(actor.mob).is_some() {
        tell(world, actor.client, Span::Warning, "busy!");
        return;
    }
    if world.get::<crate::closets::InCloset>(actor.mob).is_some() {
        tell(world, actor.client, Span::Warning, "invalid location!");
        return;
    }
    if on_tile(world, pos, kind.obstruction()).is_some() {
        tell(world, actor.client, Span::Warning, "already webbed!");
        return;
    }
    let sealing = kind == WebKind::Sealer && on_tile(world, pos, STICKYWEB).is_some();
    let text = if sealing {
        "sealing web..."
    } else {
        "spinning web..."
    };
    let name = crate::security::name_of(world, actor.mob);
    crate::chem::visible(
        world,
        actor.mob,
        text,
        &format!("{name}: {text}"),
        Span::Notice,
    );
    let (time, _) = kind.times();
    do_after(
        world,
        actor,
        actor.mob,
        time * speed,
        0,
        Job::Spider(SpiderJob::Web(kind)),
    );
}

/// `lay_eggs/IsAvailable` + `Activate`.
fn start_eggs(world: &mut World, actor: Actor, kind: EggKind) {
    let Some(&pos) = world.get::<TilePos>(actor.mob) else {
        return;
    };
    if world.get::<DoAfter>(actor.mob).is_some() {
        tell(world, actor.client, Span::Warning, "busy!");
        return;
    }
    if eggs_on(world, pos) {
        tell(world, actor.client, Span::Warning, "already eggs here!");
        return;
    }
    if kind == EggKind::Enriched
        && world
            .get::<SpiderAbilities>(actor.mob)
            .is_some_and(|a| a.enriched_charges == 0)
    {
        tell(world, actor.client, Span::Warning, "must feed first!");
        return;
    }
    let name = crate::security::name_of(world, actor.mob);
    crate::chem::visible(
        world,
        actor.mob,
        "laying eggs...",
        &format!("{name}: laying eggs..."),
        Span::Notice,
    );
    do_after(
        world,
        actor,
        actor.mob,
        EGG_LAY_DS,
        0,
        Job::Spider(SpiderJob::Eggs(kind)),
    );
}

fn eggs_on(world: &World, pos: TilePos) -> bool {
    world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .any(|&e| world.get::<SpiderEggs>(e).is_some())
}

/// `cooldown_time` of an egg button.
fn egg_cooldown(kind: EggKind) -> (&'static str, f64) {
    match kind {
        EggKind::Abnormal => ("spider_lay_abnormal_eggs", 180.0),
        EggKind::Enriched => ("spider_lay_enriched_eggs", 60.0),
        _ => ("spider_lay_eggs", 20.0),
    }
}

/// The end of a spider's `do_after`.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: SpiderJob) {
    if world.get::<Stat>(mob) != Some(&Stat::Conscious) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let t = now(world);
    match job {
        SpiderJob::Web(kind) => {
            if on_tile(world, pos, kind.obstruction()).is_some() {
                return;
            }
            // `plant_web`.
            let existing = on_tile(world, pos, STICKYWEB);
            let path = match kind {
                WebKind::Sealer if existing.is_some() => {
                    if let Some(web) = existing {
                        world.despawn(web);
                    }
                    SEALED
                }
                WebKind::Plain | WebKind::Sealer => STICKYWEB,
                WebKind::Solid => TOUGH,
                WebKind::Sticky => VERY_STICKY,
            };
            spawn_obj(world, path, pos);
            let (_, cooldown) = kind.times();
            if let Some(mut a) = world.get_mut::<SpiderAbilities>(mob) {
                a.cool(kind.id(), t + cooldown);
            }
        }
        SpiderJob::Eggs(kind) => {
            if eggs_on(world, pos) {
                tell(world, action.client, Span::Warning, "already eggs here!");
                return;
            }
            let Some(mut a) = world.get_mut::<SpiderAbilities>(mob) else {
                return;
            };
            if kind == EggKind::Enriched {
                if a.enriched_charges == 0 {
                    return;
                }
                a.enriched_charges -= 1;
            }
            let (id, cooldown) = egg_cooldown(kind);
            a.cool(id, t + cooldown);
            let directive = a.current_directive.clone();
            lay_cluster(world, kind, pos, directive, 0.0);
        }
        SpiderJob::Wrap => wrap_target(world, mob, action.target),
    }
    refresh_buttons(world, mob);
}

fn toggle_wrap(world: &mut World, mob: Entity) {
    let Some(on) = world.get::<SpiderAbilities>(mob).map(|a| !a.wrapping) else {
        return;
    };
    set_wrap(world, mob, on);
    tell_mob(
        world,
        mob,
        Span::Notice,
        if on {
            "prepared to wrap"
        } else {
            "wrap cancelled"
        },
    );
}

fn set_wrap(world: &mut World, mob: Entity, on: bool) {
    if let Some(mut a) = world.get_mut::<SpiderAbilities>(mob) {
        a.wrapping = on;
    }
    if on {
        world.entity_mut(mob).insert(ClickIntercept);
    } else {
        world.entity_mut(mob).remove::<ClickIntercept>();
    }
    refresh_buttons(world, mob);
}

/// `wrap/Activate` on the clicked thing.
fn wrap_clicks(world: &mut World, mut clicks: Local<MessageCursor<InterceptedClick>>) {
    let clicks: Vec<InterceptedClick> = clicks
        .read(world.resource::<Messages<InterceptedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let mob = click.mob;
        if !world
            .get::<SpiderAbilities>(mob)
            .is_some_and(|a| a.wrapping)
            || world.get::<PlayerMob>(mob).map(|p| p.client) != Some(click.client)
        {
            continue;
        }
        let target = click.target;
        if world.get::<Stat>(mob) != Some(&Stat::Conscious) {
            continue;
        }
        if world.get::<DoAfter>(mob).is_some() {
            tell(world, click.client, Span::Warning, "busy!");
            continue;
        }
        if click
            .mob_pos
            .chebyshev(click.target_pos)
            .is_none_or(|d| d > 1)
        {
            tell(world, click.client, Span::Warning, "must be closer!");
            continue;
        }
        if target == mob || world.get::<Health>(target).is_none() {
            continue;
        }
        let path = mob_path(world, target).unwrap_or_default();
        if crate::mobs::constructs::is_construct_or_shade(world, target) {
            tell(world, click.client, Span::Warning, "can't wrap ghosts!");
            continue;
        }
        if is_spider_path(&path) {
            tell(world, click.client, Span::Warning, "can't wrap spiders!");
            continue;
        }
        if world.get::<crate::closets::InCloset>(target).is_some() {
            continue;
        }
        set_wrap(world, mob, false);
        let (who, what) = (
            crate::security::name_of(world, mob),
            crate::security::name_of(world, target),
        );
        tell_mob(
            world,
            target,
            Span::Danger,
            format!("{who} begins to secrete a sticky substance around you."),
        );
        crate::chem::visible(
            world,
            mob,
            &format!("You begin wrapping {what} into a cocoon."),
            &format!("{who} begins to secrete a sticky substance around {what}."),
            Span::Notice,
        );
        let t = now(world);
        if let Some(mut a) = world.get_mut::<SpiderAbilities>(mob) {
            a.cool("spider_wrap", t + f64::from(WRAP_DS) / 10.0);
        }
        let actor = Actor {
            client: click.client,
            mob,
            item: None,
        };
        do_after(
            world,
            actor,
            target,
            WRAP_DS,
            0,
            Job::Spider(SpiderJob::Wrap),
        );
        refresh_buttons(world, mob);
    }
}

/// `wrap_target`: a cocoon around the target; a human (living, or a corpse not eaten before) is
/// eaten for an enriched egg.
fn wrap_target(world: &mut World, mob: Entity, target: Entity) {
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    if world.get::<crate::closets::InCloset>(target).is_some() {
        return;
    }
    let Some(cocoon) = spawn_obj(world, COCOON, pos) else {
        return;
    };
    world.entity_mut(cocoon).insert(Cocoon);
    let human = world.get::<crate::human::HumanLook>(target).is_some();
    let dead = world.get::<Stat>(target) == Some(&Stat::Dead);
    if human && (!dead || world.get::<SpiderConsumed>(target).is_none()) {
        let enriched = extras_of(&mob_path(world, mob).unwrap_or_default())
            .contains(&"spider_lay_enriched_eggs");
        if enriched && let Some(mut a) = world.get_mut::<SpiderAbilities>(mob) {
            a.enriched_charges += 1;
            let (who, what) = (
                crate::security::name_of(world, mob),
                crate::security::name_of(world, target),
            );
            crate::chem::visible(
                world,
                mob,
                &format!(
                    "You suck the nutriment out of {what}, feeding you enough to lay a cluster \
                     of enriched eggs."
                ),
                &format!("{who} sticks a proboscis into {what} and sucks a viscous substance out."),
                Span::Danger,
            );
        }
        world.entity_mut(target).insert(SpiderConsumed);
        // `death()`: they were eaten.
        if let (Some(mut health), Some(mut stat)) = (
            world.get::<Health>(target).cloned(),
            world.get::<Stat>(target).copied(),
        ) && stat != Stat::Dead
        {
            let amount = health.health() - HEALTH_THRESHOLD_DEAD;
            apply_damage(&mut health, &mut stat, DamageType::Oxy, amount);
            world.entity_mut(target).insert((health, Stat::Dead));
        }
        info!("{mob} cocooned and ate {target}");
    } else if !human {
        let what = crate::security::name_of(world, target);
        tell_mob(world, mob, Span::Warning, format!("{what} is not edible!"));
    }
    crate::closets::put_mob_inside(world, target, cocoon);
    let states: [&str; 3] = if human {
        ["cocoon_large1", "cocoon_large2", "cocoon_large3"]
    } else {
        ["cocoon1", "cocoon2", "cocoon3"]
    };
    let state = states[rand_range(world, 0, 2) as usize];
    if let Some(id) = icon(world, EFFECTS_DMI, state)
        && let Some(mut a) = world.get_mut::<ss13_shared::Appearance>(cocoon)
    {
        a.icon_state = id;
    }
}

/// `TRAIT_SPIDER_CONSUMED`: a corpse feeds a broodmother once.
#[derive(Component, Clone, Copy, Debug)]
pub struct SpiderConsumed;

/// `cocoon/Destroy`: "splits open", what it held falls out.
fn cocoon_destroyed(
    remove: On<Remove<Cocoon>>,
    inside: Query<(Entity, &crate::closets::InCloset)>,
    places: Query<&TilePos>,
    mut commands: Commands,
) {
    let cocoon = remove.entity;
    let pos = places.get(cocoon).ok().copied();
    let held: Vec<Entity> = inside
        .iter()
        .filter(|(_, c)| c.closet == cocoon)
        .map(|(e, _)| e)
        .collect();
    commands.queue(move |world: &mut World| {
        let Some(pos) = pos else {
            return;
        };
        let clients = crate::chem::clients_near(world, pos, None);
        world.write_message(ss13_net::chat::server::SendChat {
            clients,
            message: crate::chem::line(Span::Warning, "The cocoon splits open."),
        });
        for mob in held {
            if world.get_entity(mob).is_ok() {
                crate::closets::let_out(world, mob);
                world.entity_mut(mob).insert(pos);
            }
        }
    });
}

/// A player's spider bites what it clicks (`UnarmedAttack` -> `melee_attack`), with its venom
/// (`/datum/element/venomous`).
fn spider_clicks(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        let (mob, target) = (click.mob, click.target);
        let Some(path) = mob_path(world, mob).filter(|p| is_under(p, SPIDER)) else {
            continue;
        };
        if mob == target
            || click.modifiers.right
            || world.get::<Stat>(mob) != Some(&Stat::Conscious)
            || world.get::<Health>(target).is_none()
        {
            continue;
        }
        let alive = world.get::<Stat>(target) != Some(&Stat::Dead);
        if crate::mobs::defense::melee_attack(world, mob, target)
            && alive
            && let Some((reagent, units)) = venom_of(&path)
        {
            crate::medical::cyberimps::add_blood_reagent(world, target, reagent, units);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The ruleset

/// The `Spiders` announcement's time (`announce_spiders`).
#[derive(Resource, Default)]
pub struct SpiderAnnouncement(pub Option<f64>);

/// `can_be_selected`: a dark maintenance spawn.
pub fn can_spawn(world: &mut World) -> bool {
    super::nightmare::can_spawn(world)
}

/// `spiders/execute`: midwife egg clusters, almost grown, in dark maintenance; the
/// announcement later.
pub fn start_infestation(world: &mut World) -> bool {
    let mut laid = 0;
    for _ in 0..RULESET_EGGS {
        let spawns = super::nightmare::dark_spawns(world);
        if spawns.is_empty() {
            break;
        }
        let i = rand_range(world, 0, spawns.len() as u32 - 1) as usize;
        let directive = "Ensure the survival of the spider species and overtake whatever \
                         structure you find yourself in."
            .to_owned();
        if lay_cluster(world, EggKind::Midwife, spawns[i], directive, RULESET_GROWN).is_some() {
            laid += 1;
        }
    }
    if laid == 0 {
        return false;
    }
    let delay = f64::from(rand_range(world, 375, 600));
    let at = now(world) + delay;
    world.insert_resource(SpiderAnnouncement(Some(at)));
    true
}

fn announce(world: &mut World) {
    let t = now(world);
    let Some(at) = world.get_resource::<SpiderAnnouncement>().and_then(|a| a.0) else {
        return;
    };
    if t < at {
        return;
    }
    world.resource_mut::<SpiderAnnouncement>().0 = None;
    let station = crate::events::station_name(world);
    crate::events::priority_announce_sound(
        world,
        "Неопознанные формы жизни",
        &format!(
            "На борту {station} обнаружены неопознанные формы жизни. Перекройте все внешние \
             шлюзы, включая трубы и вентиляции."
        ),
        crate::announce::ANNOUNCER_ALIENS,
    );
}

pub struct SpidersPlugin;

impl Plugin for SpidersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SpiderAnnouncement>()
            .add_observer(cocoon_destroyed)
            .add_systems(
                Update,
                (
                    trigger_buttons,
                    wrap_clicks,
                    hatch_picked.after(PromptSystems::Answer),
                    text_answered.after(PromptSystems::Answer),
                )
                    .in_set(UiSystems::Handle),
            )
            .add_systems(
                Update,
                spider_clicks
                    .after(crate::click::ResolveClicks)
                    .before(crate::health::HealthSystems),
            )
            .add_systems(
                Update,
                (
                    grow_eggs.run_if(any_with_component::<SpiderEggs>),
                    grow_spiders.run_if(any_with_component::<SpiderGrowth>),
                    announce,
                ),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spiders_grow_through_their_stages() {
        assert_eq!(
            grown_path(format!("{SPIDERLING}/nurse")).as_deref(),
            Some("/mob/living/basic/spider/growing/young/nurse")
        );
        assert_eq!(
            grown_path(format!("{YOUNG}/nurse")).as_deref(),
            Some("/mob/living/basic/spider/giant/nurse")
        );
        assert_eq!(grown_path(format!("{GIANT}/nurse")), None);
        // A base spiderling has no `grow_as`: its giant is rolled.
        assert_eq!(grown_path(SPIDERLING.to_owned()).as_deref(), Some(YOUNG));
    }

    #[test]
    fn webs_and_venom_follow_the_types() {
        assert_eq!(web_of(&format!("{GIANT}/nurse")), (0.25, WebKind::Sealer));
        assert_eq!(web_of(&format!("{GIANT}/midwife")), (0.5, WebKind::Sealer));
        assert_eq!(
            web_of(&format!("{GIANT}/tarantula")),
            (0.7, WebKind::Sealer)
        );
        assert_eq!(web_of(&format!("{GIANT}/breacher")), (0.25, WebKind::Plain));
        assert_eq!(web_of(&format!("{GIANT}/guard")), (1.0, WebKind::Plain));
        assert_eq!(
            venom_of(&format!("{GIANT}/scout")),
            Some(("/datum/reagent/peaceborg/confuse", 10.0))
        );
        assert_eq!(venom_of(&format!("{GIANT}/guard")), None);
        assert!(extras_of(&format!("{GIANT}/midwife")).contains(&"spider_wrap"));
        assert!(EggKind::Enriched.spawns().len() == 3);
    }
}
