//! Glands (server) after tg `equipment/gland.dm` and `equipment/glands/*.dm`: hearts that do
//! something now and then.
//!
//! A gland is a heart organ (`/obj/item/organ/heart/gland/*`, robotic, it never stops) with a
//! [`Gland`]. Inserted into a carbon (a surgery or the experimentation machine) it starts when
//! it has uses; every `SSmobs` fire of a living owner a started gland past its cooldown
//! activates, spends a use (`-1`: forever) and rolls the next cooldown in
//! `[cooldown_low, cooldown_high]`; out of uses it stops. Removed, it stops (a one-use gland gets
//! its use back). A `human_only` gland stops in a non-human.
//!
//! The effects (`activate`):
//! - access: the owner gets every station access (`simple_access`);
//! - chem: 2 u of a random drug, medicine or toxin in the blood, 5 toxin damage healed;
//! - egg: a gland egg at the owner's feet;
//! - electric: immune to shocks while inside; arcs, then 3-10 s later a tesla zap (range 4);
//! - heal: the first of: a missing or failing appendix, a damaged or missing liver, lungs,
//!   stomach (vomited up) or eyes (fall out, new ones 10-20 s later) is replaced by the
//!   species' own;
//! - mindshock: every other carbon within 4 tiles is stunned 5 s, or confused 15 s and gets 10
//!   brain damage (up to 160), or hallucinates 150 s;
//! - plasma: bloated, a stomachache 15 s later, 20 s later it vomits 50 mol of plasma;
//! - quantum: entangles with a carbon in view, 1-4 minutes later they swap places;
//! - slime: vomits a grey slime; spider: a spiderling at its feet;
//! - transform: a new species (human, lizard, moth, fly);
//! - trauma: a random brain trauma (special 33%, else severe 20%, else mild), up to 5 times;
//! - ventcrawling: ventcrawls from then on.
//!
//! The mental interface device's command ([`mind_control`]): a use of the gland's
//! `mind_control_uses`, the order in the owner's chat, forgotten after `mind_control_duration`.
// shortcut: the blood gland's random blood type and the viral gland's random disease are not
// ported (their activation only tells the owner); the heal gland does not reject implants or
// cybernetics, regrow limbs, purge toxins or restore blood; no mindshield check for the
// mindshock, no slime faction or language, no broadcast of the mindshock's or mirror of the
// quantum's commands, no mind control alert; a gland inserted by any surgery starts at once
// (tg waits for the experiment when an abductor implants it).

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::*;
use crate::health::{DamageType, Stat};
use crate::medical::organs::{Organ, OrganHost, OrganOf, Organs, Slot};
use crate::status_effects::{Incap, Timed};
use crate::tools::now;

pub const GLAND_ROOT: &str = "/obj/item/organ/heart/gland";
pub const GLAND_EGG: &str = "/obj/item/food/egg/gland";
pub const SPIDERLING: &str = "/mob/living/basic/spider/growing/spiderling";
pub const SLIME: &str = "/mob/living/basic/slime";
/// `SSmobs` `wait`: how often organs run `on_life`.
const LIFE_SECS: f64 = 2.0;

/// The gland subtypes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GlandKind {
    Access,
    Blood,
    Chem,
    Egg,
    Electric,
    Heal,
    Mindshock,
    Plasma,
    Quantum,
    Slime,
    Spider,
    Transform,
    Trauma,
    Ventcrawl,
    Viral,
}

/// A gland type's vars.
#[derive(Clone, Copy, Debug)]
pub struct GlandDef {
    pub path: &'static str,
    pub kind: GlandKind,
    /// `abductor_hint`.
    pub hint: &'static str,
    /// `cooldown_low`, `cooldown_high`: seconds.
    pub cooldown: (f64, f64),
    /// `uses`: `-1` forever.
    pub uses: i32,
    pub human_only: bool,
    pub mind_control_uses: u32,
    /// `mind_control_duration`: seconds.
    pub mind_control_secs: f64,
}

const fn gland(
    path: &'static str,
    kind: GlandKind,
    cooldown_ds: (u32, u32),
    uses: i32,
    mind_control: (u32, u32),
    hint: &'static str,
) -> GlandDef {
    GlandDef {
        path,
        kind,
        hint,
        cooldown: (cooldown_ds.0 as f64 / 10.0, cooldown_ds.1 as f64 / 10.0),
        uses,
        human_only: matches!(kind, GlandKind::Heal | GlandKind::Transform),
        mind_control_uses: mind_control.0,
        mind_control_secs: mind_control.1 as f64 / 10.0,
    }
}

/// `subtypesof(/obj/item/organ/heart/gland)`.
pub const GLANDS: [GlandDef; 15] = [
    gland(
        "/obj/item/organ/heart/gland/access",
        GlandKind::Access,
        (600, 1200),
        1,
        (3, 900),
        "anagraphic electro-scrambler. After it activates, grants the abductee intrinsic all \
         access.",
    ),
    gland(
        "/obj/item/organ/heart/gland/blood",
        GlandKind::Blood,
        (1200, 1800),
        -1,
        (3, 1500),
        "pseudonuclear hemo-destabilizer. Periodically randomizes the abductee's bloodtype into \
         a random reagent.",
    ),
    gland(
        "/obj/item/organ/heart/gland/chem",
        GlandKind::Chem,
        (50, 50),
        -1,
        (3, 1200),
        "intrinsic pharma-provider. The abductee constantly produces random chemicals inside \
         their bloodstream. They also quickly regenerate toxin damage.",
    ),
    gland(
        "/obj/item/organ/heart/gland/egg",
        GlandKind::Egg,
        (300, 400),
        -1,
        (2, 1800),
        "roe/enzymatic synthesizer. The abductee will periodically lay eggs filled with random \
         reagents.",
    ),
    gland(
        "/obj/item/organ/heart/gland/electric",
        GlandKind::Electric,
        (800, 1200),
        -1,
        (2, 900),
        "electron accumulator/discharger. The abductee becomes fully immune to electric shocks. \
         Additionally, they will randomly discharge electric bolts.",
    ),
    gland(
        "/obj/item/organ/heart/gland/heal",
        GlandKind::Heal,
        (200, 400),
        -1,
        (3, 3000),
        "organic replicator. Forcibly ejects damaged and robotic organs from the abductee and \
         regenerates them.",
    ),
    gland(
        "/obj/item/organ/heart/gland/mindshock",
        GlandKind::Mindshock,
        (400, 700),
        -1,
        (2, 1200),
        "neural crosstalk uninhibitor. The abductee emits a disrupting psychic wave every so \
         often. This will either stun, cause hallucinations or deal random brain damage to \
         people nearby.",
    ),
    gland(
        "/obj/item/organ/heart/gland/plasma",
        GlandKind::Plasma,
        (1200, 1800),
        -1,
        (1, 800),
        "effluvium sanguine-synonym emitter. The abductee randomly emits clouds of plasma.",
    ),
    gland(
        "/obj/item/organ/heart/gland/quantum",
        GlandKind::Quantum,
        (150, 150),
        -1,
        (2, 1200),
        "quantic de-observation matrix. Periodically links with a random person in view, then \
         the abductee later swaps positions with that person.",
    ),
    gland(
        "/obj/item/organ/heart/gland/slime",
        GlandKind::Slime,
        (600, 1200),
        -1,
        (1, 2400),
        "gastric animation galvanizer. The abductee occasionally vomits slimes. Slimes will no \
         longer attack the abductee.",
    ),
    gland(
        "/obj/item/organ/heart/gland/spiderman",
        GlandKind::Spider,
        (450, 900),
        -1,
        (2, 2400),
        "araneae cloister accelerator. The abductee occasionally exhales spider pheromones and \
         will spawn spiderlings.",
    ),
    gland(
        "/obj/item/organ/heart/gland/transform",
        GlandKind::Transform,
        (900, 1800),
        -1,
        (7, 300),
        "anthropmorphic transmorphosizer. The abductee will occasionally change appearance and \
         species.",
    ),
    gland(
        "/obj/item/organ/heart/gland/trauma",
        GlandKind::Trauma,
        (800, 1200),
        5,
        (3, 1800),
        "white matter randomiser. The abductee occasionally gains a random brain trauma, up to \
         five times. The traumas can range from basic to deep-rooted.",
    ),
    gland(
        "/obj/item/organ/heart/gland/ventcrawling",
        GlandKind::Ventcrawl,
        (1800, 2400),
        1,
        (4, 1800),
        "pliant cartilage enabler. The abductee can crawl through vents without trouble.",
    ),
    gland(
        "/obj/item/organ/heart/gland/viral",
        GlandKind::Viral,
        (1800, 2400),
        1,
        (1, 1800),
        "contamination incubator. The abductee becomes a carrier of a random advanced disease - \
         of which they are unaffected by.",
    ),
];

pub fn gland_def(path: &str) -> Option<&'static GlandDef> {
    GLANDS.iter().find(|g| g.path == path)
}

/// A delayed part of an activation (tg `addtimer`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlandTimer {
    PlasmaAche,
    PlasmaVomit,
    Zap,
    Swap,
    Eyes,
}

/// A live `/obj/item/organ/heart/gland`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Gland {
    pub kind: GlandKind,
    pub uses: i32,
    pub active: bool,
    /// `activation_cooldown`: game time of the next activation.
    pub next_at: f64,
    pub mind_control_uses: u32,
    /// `active_mind_control`: until when.
    pub mind_control_until: Option<f64>,
    pub timers: Vec<(f64, GlandTimer)>,
    /// The quantum gland's `entangled_mob`.
    pub entangled: Option<Entity>,
}

impl Gland {
    pub fn new(def: &GlandDef) -> Self {
        Self {
            kind: def.kind,
            uses: def.uses,
            active: false,
            next_at: 0.0,
            mind_control_uses: def.mind_control_uses,
            mind_control_until: None,
            timers: Vec::new(),
            entangled: None,
        }
    }

    pub fn def(&self) -> &'static GlandDef {
        GLANDS
            .iter()
            .find(|g| g.kind == self.kind)
            .expect("every kind has a def")
    }

    /// One `on_life` of a started gland at `now`: whether it activates (a use spent, the next
    /// cooldown from `roll`).
    pub fn tick(&mut self, now: f64, roll: impl FnOnce(f64, f64) -> f64) -> bool {
        if !self.active {
            return false;
        }
        let mut fired = false;
        if now >= self.next_at {
            fired = true;
            if self.uses > 0 {
                self.uses -= 1;
            }
            let (low, high) = self.def().cooldown;
            self.next_at = now + roll(low, high);
        }
        if self.uses == 0 {
            self.active = false;
        }
        fired
    }
}

/// The glands among `mob`'s organs.
pub fn glands_of(world: &World, mob: Entity) -> Vec<Entity> {
    world
        .get::<Organs>(mob)
        .map(|o| {
            o.iter()
                .filter(|&e| world.get::<Gland>(e).is_some())
                .collect()
        })
        .unwrap_or_default()
}

fn roll(world: &mut World, low: f64, high: f64) -> f64 {
    if high <= low {
        return low;
    }
    with_rng(world, |_, rng| {
        low + rng.range(0, ((high - low) * 10.0) as i64) as f64 / 10.0
    })
}

/// `Start`: active, the first cooldown rolled.
pub fn start(world: &mut World, organ: Entity) {
    let Some(def) = world.get::<Gland>(organ).map(|g| g.def()) else {
        return;
    };
    let delay = roll(world, def.cooldown.0, def.cooldown.1);
    let t = now(world);
    if let Some(mut g) = world.get_mut::<Gland>(organ) {
        g.active = true;
        g.next_at = t + delay;
    }
}

/// `on_mob_insert` (from `crate::medical::organs`): a gland with uses starts.
pub fn gland_inserted(world: &mut World, organ: Entity, _mob: Entity) {
    if world.get::<Gland>(organ).is_some_and(|g| g.uses != 0) {
        start(world, organ);
    }
}

/// `on_mob_remove` (from `crate::medical::organs`): it stops, a one-use gland gets its use back,
/// the mind control ends.
pub fn gland_removed(world: &mut World, organ: Entity, mob: Entity) {
    let Some(mut g) = world.get_mut::<Gland>(organ) else {
        return;
    };
    g.active = false;
    g.timers.clear();
    if g.def().uses == 1 {
        g.uses = 1;
    }
    let controlled = g.mind_control_until.take().is_some();
    if controlled {
        clear_message(world, mob);
    }
}

/// `TRAIT_SHOCKIMMUNE` of an electric gland inside `mob`.
pub fn gland_shock_immune(world: &World, mob: Entity) -> bool {
    glands_of(world, mob).into_iter().any(|g| {
        world
            .get::<Gland>(g)
            .is_some_and(|g| g.kind == GlandKind::Electric)
    })
}

/// The `simple_access` an anagraphic gland gave (all station access).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct GlandAccess;

/// `ownerCheck`: a human, or any carbon for the others.
fn owner_check(world: &World, mob: Entity, def: &GlandDef) -> bool {
    if world.get::<crate::human::HumanLook>(mob).is_some() {
        return true;
    }
    !def.human_only && world.get::<OrganHost>(mob).is_some()
}

/// Glands spawned from a map, the storage or a crate get their state.
fn adopt(add: On<Add<DmItem>>, items: Query<&DmItem, Without<Gland>>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    if let Some(def) = gland_def(&item.path) {
        commands.entity(add.entity).try_insert(Gland::new(def));
    }
}

/// `on_life` of the glands of living owners, the delayed effects, the mind control timers.
// Cost: per tick one pass over the glands in play (a handful).
fn process(world: &mut World, mut last: Local<f64>) {
    let t = now(world);
    let life = t - *last >= LIFE_SECS;
    if life {
        *last = t;
    }
    let mut q = world.query::<(Entity, &Gland, Option<&OrganOf>)>();
    let glands: Vec<(Entity, Option<Entity>)> = q
        .iter(world)
        .map(|(e, _, owner)| (e, owner.map(|o| o.0)))
        .collect();
    for (organ, owner) in glands {
        let Some(mob) = owner else {
            continue;
        };
        timers(world, organ, mob, t);
        if world
            .get::<Gland>(organ)
            .and_then(|g| g.mind_control_until)
            .is_some_and(|until| t >= until)
        {
            if let Some(mut g) = world.get_mut::<Gland>(organ) {
                g.mind_control_until = None;
            }
            clear_message(world, mob);
        }
        if !life || world.get::<Stat>(mob).is_none_or(|s| *s == Stat::Dead) {
            continue;
        }
        let Some(def) = world.get::<Gland>(organ).map(|g| g.def()) else {
            continue;
        };
        if world.get::<Gland>(organ).is_some_and(|g| g.active) && !owner_check(world, mob, def) {
            if let Some(mut g) = world.get_mut::<Gland>(organ) {
                g.active = false;
            }
            continue;
        }
        let delay = roll(world, def.cooldown.0, def.cooldown.1);
        let fired = world
            .get_mut::<Gland>(organ)
            .is_some_and(|mut g| g.tick(t, |_, _| delay));
        if fired {
            activate(world, organ, mob);
        }
    }
}

fn timers(world: &mut World, organ: Entity, mob: Entity, t: f64) {
    let due: Vec<GlandTimer> = {
        let Some(mut g) = world.get_mut::<Gland>(organ) else {
            return;
        };
        if g.timers.iter().all(|(at, _)| t < *at) {
            return;
        }
        let (due, rest): (Vec<(f64, GlandTimer)>, Vec<(f64, GlandTimer)>) =
            std::mem::take(&mut g.timers)
                .into_iter()
                .partition(|(at, _)| t >= *at);
        g.timers = rest;
        due.into_iter().map(|(_, k)| k).collect()
    };
    for timer in due {
        match timer {
            GlandTimer::PlasmaAche => {
                tell_mob(
                    world,
                    mob,
                    Span::BoldWarning,
                    "Вас одолевает сильная боль в животе.",
                );
            }
            GlandTimer::PlasmaVomit => vomit_plasma(world, mob),
            GlandTimer::Zap => zap(world, mob),
            GlandTimer::Swap => quantum_swap(world, organ, mob),
            GlandTimer::Eyes => finish_replace_eyes(world, mob),
        }
    }
}

fn add_timer(world: &mut World, organ: Entity, delay: f64, timer: GlandTimer) {
    let at = now(world) + delay;
    if let Some(mut g) = world.get_mut::<Gland>(organ) {
        g.timers.push((at, timer));
    }
}

fn visible(world: &mut World, mob: Entity, own: &str, others: &str, span: Span) {
    crate::medical::visible(world, mob, own, others, span);
}

fn pos_of(world: &World, mob: Entity) -> Option<TilePos> {
    world.get::<TilePos>(mob).copied()
}

/// Carbons other than `mob` within `range` tiles of it.
fn carbons_near(world: &mut World, mob: Entity, range: u32) -> Vec<Entity> {
    let Some(pos) = pos_of(world, mob) else {
        return Vec::new();
    };
    let mut q = world.query_filtered::<(Entity, &TilePos), With<OrganHost>>();
    let mut out: Vec<Entity> = q
        .iter(world)
        .filter(|(e, p)| *e != mob && p.chebyshev(pos).is_some_and(|d| d <= range))
        .map(|(e, _)| e)
        .collect();
    out.sort();
    out
}

/// `activate()` of each gland type.
pub fn activate(world: &mut World, organ: Entity, mob: Entity) {
    let Some(kind) = world.get::<Gland>(organ).map(|g| g.kind) else {
        return;
    };
    match kind {
        GlandKind::Access => {
            tell_mob(
                world,
                mob,
                Span::Notice,
                "Вы почему-то чувствуете себя VIP-персоной.",
            );
            world.entity_mut(mob).insert(GlandAccess);
        }
        GlandKind::Blood => {
            tell_mob(
                world,
                mob,
                Span::Warning,
                "Вы чувствуете, как ваша кровь на мгновение нагревается.",
            );
        }
        GlandKind::Chem => chem(world, mob),
        GlandKind::Egg => {
            let name = name_of(world, mob);
            let i = with_rng(world, |_, rng| rng.range(0, 3) as usize);
            let message = crate::mobs::farm::EGG_LAYING_MESSAGES[i];
            visible(
                world,
                mob,
                &format!("Вы: {message}"),
                &format!("{name} {message}"),
                Span::Warning,
            );
            if let Some(pos) = pos_of(world, mob) {
                crate::construction::spawn_obj(world, GLAND_EGG, pos);
            }
        }
        GlandKind::Electric => {
            let name = name_of(world, mob);
            visible(
                world,
                mob,
                "Вы чувствуете, как внутри вас накапливается электрическая энергия!",
                &format!("Кожа {name} начинает испускать электрические дуги!"),
                Span::Danger,
            );
            let delay = roll(world, 3.0, 10.0);
            add_timer(world, organ, delay, GlandTimer::Zap);
        }
        GlandKind::Heal => heal(world, organ, mob),
        GlandKind::Mindshock => mindshock(world, mob),
        GlandKind::Plasma => {
            tell_mob(world, mob, Span::Warning, "Вас пучит.");
            add_timer(world, organ, 15.0, GlandTimer::PlasmaAche);
            add_timer(world, organ, 20.0, GlandTimer::PlasmaVomit);
        }
        GlandKind::Quantum => {
            if world
                .get::<Gland>(organ)
                .and_then(|g| g.entangled)
                .is_some()
            {
                return;
            }
            let Some(&target) = carbons_near(world, mob, 7).first() else {
                return;
            };
            if let Some(mut g) = world.get_mut::<Gland>(organ) {
                g.entangled = Some(target);
            }
            let delay = roll(world, 60.0, 240.0);
            add_timer(world, organ, delay, GlandTimer::Swap);
        }
        GlandKind::Slime => {
            tell_mob(world, mob, Span::Warning, "Вас тошнит!");
            crate::medical::diseases::effects::vomit(world, mob, 20.0, false, false, 1);
            if let Some(pos) = pos_of(world, mob) {
                crate::construction::spawn_obj(world, SLIME, pos);
            }
        }
        GlandKind::Spider => {
            tell_mob(
                world,
                mob,
                Span::Warning,
                "Вы чувствуете, как что-то ползает под кожей.",
            );
            if let Some(pos) = pos_of(world, mob) {
                crate::construction::spawn_obj(world, SPIDERLING, pos);
            }
        }
        GlandKind::Transform => {
            use crate::species::SpeciesId;
            tell_mob(
                world,
                mob,
                Span::Notice,
                "Вы чувствуете себя не в своей тарелке.",
            );
            let options = [
                SpeciesId::Human,
                SpeciesId::Lizard,
                SpeciesId::Moth,
                SpeciesId::Fly,
            ];
            let i = with_rng(world, |_, rng| {
                rng.range(0, options.len() as i64 - 1) as usize
            });
            crate::species::set_species(world, mob, options[i]);
        }
        GlandKind::Trauma => trauma(world, mob),
        GlandKind::Ventcrawl => {
            tell_mob(world, mob, Span::Notice, "Вы чувствуете себя очень гибким.");
            world
                .entity_mut(mob)
                .insert(crate::ventcrawl::Ventcrawler { nude: false });
        }
        GlandKind::Viral => {
            tell_mob(world, mob, Span::Warning, "Вам нехорошо.");
        }
    }
}

/// `chem/activate`: a random drug, medicine or toxin; toxin damage heals.
fn chem(world: &mut World, mob: Entity) {
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return;
    };
    let pool: Vec<&str> = chem
        .reagents
        .iter()
        .map(|r| r.path.as_str())
        .filter(|p| {
            p.starts_with("/datum/reagent/drug/")
                || p.starts_with("/datum/reagent/medicine/")
                || crate::obj_damage::is_under(p, "/datum/reagent/toxin")
        })
        .collect();
    if !pool.is_empty() {
        let i = with_rng(world, |_, rng| rng.range(0, pool.len() as i64 - 1) as usize);
        if let (Some(id), Some(mut reagents)) = (
            chem.id(pool[i]),
            world.get::<crate::chem::MobReagents>(mob).cloned(),
        ) {
            reagents
                .blood
                .add_reagent(&chem, id, 2.0, 310.15, None, None, false, false);
            world.entity_mut(mob).insert(reagents);
        }
    }
    if let Some(mut health) = world.get_mut::<Health>(mob) {
        health.adjust(DamageType::Tox, -5.0);
    }
}

/// `electric/zap`.
fn zap(world: &mut World, mob: Entity) {
    let Some(pos) = pos_of(world, mob) else {
        return;
    };
    let mut shocked = Vec::new();
    crate::power::tesla::tesla_zap(
        world,
        mob,
        pos,
        4,
        8e3,
        1e3,
        crate::power::tesla::ZAP_DEFAULT_FLAGS,
        &mut shocked,
        0,
    );
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, "sound/effects/magic/lightningshock.ogg", 50.0, true),
    );
}

/// `mindshock/activate`.
fn mindshock(world: &mut World, mob: Entity) {
    tell_mob(world, mob, Span::Notice, "У вас болит голова.");
    for target in carbons_near(world, mob, 4) {
        match with_rng(world, |_, rng| rng.range(1, 3)) {
            1 => {
                tell_mob(
                    world,
                    target,
                    Span::Danger,
                    "Вы слышите громкое жужжание в голове, заглушающее ваши мысли!",
                );
                crate::status_effects::apply(world, target, Incap::Stun, 5.0);
            }
            2 => {
                tell_mob(
                    world,
                    target,
                    Span::Warning,
                    "Вы слышите раздражающее жужжание в голове.",
                );
                crate::status_effects::adjust_timed(world, target, Timed::Confusion, 15.0, None);
                crate::medical::organs::adjust_organ_loss_up_to(
                    world,
                    target,
                    Slot::Brain,
                    10.0,
                    160.0,
                );
            }
            _ => crate::hallucination::adjust_hallucinations(world, target, 150.0),
        }
    }
}

/// `plasma/vomit_plasma`.
fn vomit_plasma(world: &mut World, mob: Entity) {
    let name = name_of(world, mob);
    visible(
        world,
        mob,
        "Вас рвёт облаком плазмы!",
        &format!("{name} извергает облако плазмы!"),
        Span::Danger,
    );
    if let Some(pos) = pos_of(world, mob)
        && let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>()
    {
        let _ = atmos.spawn_air(pos, "plasma=50;TEMP=293.15");
    }
    crate::medical::diseases::effects::vomit(world, mob, 10.0, false, false, 1);
}

/// `quantum/quantum_swap`.
fn quantum_swap(world: &mut World, organ: Entity, mob: Entity) {
    let Some(other) = world
        .get_mut::<Gland>(organ)
        .and_then(|mut g| g.entangled.take())
    else {
        return;
    };
    let (Some(a), Some(b)) = (pos_of(world, mob), pos_of(world, other)) else {
        return;
    };
    let tele = crate::teleport::Tele {
        channel: crate::teleport::Channel::Quantum,
        ..default()
    };
    crate::teleport::do_teleport(world, mob, b, tele);
    crate::teleport::do_teleport(world, other, a, tele);
    for m in [mob, other] {
        tell_mob(
            world,
            m,
            Span::Warning,
            "Вы внезапно оказываетесь в другом месте!",
        );
    }
}

/// `trauma/activate`.
fn trauma(world: &mut World, mob: Entity) {
    use crate::medical::traumas::{Category, Resilience};
    tell_mob(
        world,
        mob,
        Span::Warning,
        "Вы чувствуете острую боль в голове.",
    );
    let Some(brain) = crate::medical::organs::organ(world, mob, Slot::Brain) else {
        return;
    };
    let (category, resilience) = with_rng(world, |_, rng| {
        let category = if rng.prob(33.0) {
            Category::Special
        } else if rng.prob(20.0) {
            Category::Severe
        } else {
            Category::Mild
        };
        let resilience = match rng.range(1, 3) {
            1 => Resilience::Basic,
            2 => Resilience::Surgery,
            _ => Resilience::Lobotomy,
        };
        (category, resilience)
    });
    let mut rng = crate::medical::fork_rng(world);
    crate::medical::traumas::gain_type(world, brain, category, Some(resilience), false, &mut rng);
}

/// `heal/activate` for the organs: the first replacement it needs.
fn heal(world: &mut World, organ: Entity, mob: Entity) {
    use crate::medical::organs::state;
    let set = crate::species::def_of(world, mob).organs;
    // (slot, needs a replacement): missing (when the species has one) or failing/damaged.
    let needs = |world: &World, slot: Slot| -> bool {
        if set.get(slot).is_none() {
            return false;
        }
        match state(world, mob, slot) {
            None => true,
            Some(o) => {
                let spec = slot.spec();
                o.robotic
                    || match slot {
                        Slot::Appendix => o.failing,
                        Slot::Eyes => o.damage > spec.low_threshold,
                        _ => o.damage > spec.high_threshold,
                    }
            }
        }
    };
    for slot in [
        Slot::Appendix,
        Slot::Liver,
        Slot::Lungs,
        Slot::Stomach,
        Slot::Eyes,
    ] {
        if !needs(&*world, slot) {
            continue;
        }
        let old = crate::medical::organs::organ(world, mob, slot);
        let name = name_of(world, mob);
        if let Some(old) = old {
            crate::medical::organs::remove(world, old, true);
            if let Some(pos) = pos_of(world, mob) {
                crate::storage::take_out(world, old, pos);
            }
            let organ_name = slot.spec().ru.0;
            if slot == Slot::Eyes {
                visible(
                    world,
                    mob,
                    "Ваши глаза выпадают из глазниц!",
                    &format!("Глаза {name} выпадают из глазниц!"),
                    Span::Danger,
                );
            } else {
                visible(
                    world,
                    mob,
                    &format!("Вас внезапно рвёт: {organ_name}!"),
                    &format!("{name} выблёвывает {organ_name}!"),
                    Span::Danger,
                );
                crate::medical::diseases::effects::vomit(world, mob, 0.0, true, true, 1);
            }
        } else {
            tell_mob(
                world,
                mob,
                Span::Warning,
                "Вы чувствуете странное урчание внутри...",
            );
        }
        if slot == Slot::Eyes {
            let delay = roll(world, 10.0, 20.0);
            add_timer(world, organ, delay, GlandTimer::Eyes);
        } else if let Some(path) = set.get(slot)
            && let Some(new) = crate::medical::organs::spawn_organ(world, path)
        {
            crate::medical::organs::insert(world, new, mob);
        }
        return;
    }
}

/// `heal/finish_replace_eyes`.
fn finish_replace_eyes(world: &mut World, mob: Entity) {
    let Some(path) = crate::species::def_of(world, mob).organs.get(Slot::Eyes) else {
        return;
    };
    if let Some(eyes) = crate::medical::organs::spawn_organ(world, path) {
        crate::medical::organs::insert(world, eyes, mob);
    }
    let name = name_of(world, mob);
    visible(
        world,
        mob,
        "В ваших глазницах внезапно надуваются новые глаза!",
        &format!("В глазницах {name} внезапно надуваются новые глаза!"),
        Span::Warning,
    );
}

/// `mind_control(command, user)`: false when the gland is spent, busy or its owner unfit.
pub fn mind_control(world: &mut World, organ: Entity, command: &str, user: Entity) -> bool {
    let Some(mob) = world.get::<OrganOf>(organ).map(|o| o.0) else {
        return false;
    };
    let Some(g) = world.get::<Gland>(organ).cloned() else {
        return false;
    };
    if !owner_check(world, mob, g.def())
        || g.mind_control_uses == 0
        || g.mind_control_until.is_some()
    {
        return false;
    }
    let until = now(world) + g.def().mind_control_secs;
    if let Some(mut g) = world.get_mut::<Gland>(organ) {
        g.mind_control_uses -= 1;
        g.mind_control_until = Some(until);
    }
    tell_mob(
        world,
        mob,
        Span::Danger,
        "Вы внезапно чувствуете непреодолимое желание исполнить приказ...",
    );
    tell_mob(world, mob, Span::Hypnophrase, command);
    info!("{user} sent an abductor mind control message to {mob}: {command}");
    true
}

/// `clear_mind_control`.
fn clear_message(world: &mut World, mob: Entity) {
    tell_mob(
        world,
        mob,
        Span::Danger,
        "Вы чувствуете, как желание угасает, и полностью забываете о прежних приказах.",
    );
}

/// `examine` of a gland by an abductor scientist: its hint.
pub fn examine_hint(world: &World, viewer: Entity, item: Entity) -> Option<String> {
    let g = world.get::<Gland>(item)?;
    has_scientist_training(world, viewer).then(|| format!("Это {}", g.def().hint))
}

pub(super) fn build_glands(app: &mut App) {
    app.add_observer(adopt).add_systems(
        Update,
        process
            .run_if(any_with_component::<Gland>)
            .before(crate::health::HealthSystems),
    );
}

/// An organ is a gland's.
pub fn is_gland(organ: &Organ) -> bool {
    crate::obj_damage::is_under(&organ.path, GLAND_ROOT)
}
