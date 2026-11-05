//! Farm animals (server): udders, tipping cows, laying hens, fertile eggs and chicks growing
//! up, after tg `/datum/component/udder` (`/obj/item/udder`), `/datum/component/tippable` with the
//! cow's `after_cow_tipped` and `tipped_reaction`, `/datum/component/egg_layer`,
//! `/datum/component/fertile_egg` and `/datum/component/growth_and_differentiation`.
//!
//! - Cows and female goats have an [`Udder`] of 50 u: 20 u of milk to begin with, then every
//!   `SSobj` fire (2 s) a 5% chance of 5-10 u more. A reagent cup clicked on the living animal
//!   takes 5-10 u (`milk`); examining tells how full it is.
//! - A cow is tipped by a right click in combat mode (0.5 s), lies on its back immobile until
//!   someone rights it (right click again, 0.5 s) or it rights itself after its own 25-50 s;
//!   10-20 s after the tip its AI begins pleading with someone nearby (`tip_reaction`).
//! - Hens ([`EggLayer`]) eat wheat from the hand (1-4 eggs more, at most 8) and lay organic eggs
//!   (`SPT_PROB(1.5)` per `SSobj` fire); a quarter of them are fertile while there are at most
//!   `MAX_CHICKENS` chickens ([`FertileEgg`]): an egg lying on a floor hatches a chick after
//!   200 growth (1-2 per second), one picked up spoils. A chick ([`Growth`]) grows up into a
//!   chicken (`growth_and_differentiation`, 0.5-1% a second) unless a player has it.
// shortcut: the moonicorn's and the cow of wisdom's own milk is plain milk; no riding a tamed
// cow; eggs are not heated or incubated (tg only needs a turf).

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::mobs::MobDef;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};

use super::BasicMob;
use super::ai::{AiController, Key, Value};
use crate::click_handlers::ClickCtx;
use crate::combat::{CombatMode, CombatRng};
use crate::health::Stat;
use crate::inventory::CarriedBy;
use crate::tools::{Actor, DoAfter};

/// `/obj/item/udder` `size`, `production_probability`, the 20 u it starts with.
const UDDER_SIZE: f32 = 50.0;
const UDDER_PRODUCTION_PROB: f32 = 5.0;
const UDDER_START: f32 = 20.0;
const MILK: &str = "/datum/reagent/consumable/milk";
const CUP_ROOT: &str = "/obj/item/reagent_containers/cup";
/// `DEFAULT_REAGENT_TEMPERATURE`.
const REAGENT_TEMP: f32 = 300.0;
/// `SSobj` and `SSdcs` waits.
const SSOBJ_WAIT: f64 = 2.0;
const SSDCS_WAIT: f64 = 1.0;
/// The cow's `tip_time`, `untip_time` (deciseconds).
const TIP_DS: f32 = 5.0;
const COW: &str = "/mob/living/basic/cow";
const GOAT: &str = "/mob/living/basic/goat";
const CHICKEN: &str = "/mob/living/basic/chicken";
const CHICK: &str = "/mob/living/basic/chick";
const ORGANIC_EGG: &str = "/obj/item/food/egg/organic";
const WHEAT: &str = "/obj/item/food/grown/wheat";
/// `MAX_CHICKENS`.
pub const MAX_CHICKENS: usize = 50;
/// `EGG_LAYING_MESSAGES` (BandaStation keeps them in English).
const EGG_LAYING_MESSAGES: [&str; 4] = [
    "lays an egg.",
    "squats down and croons.",
    "begins making a huge racket.",
    "begins clucking raucously.",
];
/// `fertile_egg` `total_growth_required` of the chicken's eggs.
const EGG_GROWTH_REQUIRED: f32 = 200.0;

/// `/obj/item/udder` with its reagents (one reagent: what the animal makes).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Udder {
    pub reagent: &'static str,
    pub volume: f32,
}

/// `/datum/component/tippable` of a cow.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Tippable {
    pub tipped: bool,
    /// `self_right_time`, rolled at `Initialize`.
    pub self_right_secs: f64,
    /// When it rights itself (while tipped).
    pub right_at: f64,
    /// `after_cow_tipped`: when the AI starts pleading, and with whom.
    pub react_at: Option<(f64, Entity)>,
}

/// `/datum/component/egg_layer`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct EggLayer {
    pub eggs_left: u32,
    /// `eggs_added_from_eating`, `max_eggs_held`.
    pub per_feed: u32,
    pub max: u32,
}

/// `/datum/component/fertile_egg` of a chicken's egg.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct FertileEgg {
    pub growth: f32,
}

/// `/datum/component/growth_and_differentiation` of a chick (`percent_grown`).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Growth {
    pub percent: f32,
    pub into: &'static str,
}

/// A timed farm action ([`crate::construction::Job::Fauna`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FarmJob {
    Tip,
    Untip,
    /// A turtle drinks from a container (`crate::mobs::critters`).
    TurtleDrink,
}

#[derive(Resource, Default)]
struct FarmClock {
    obj: f64,
    dcs: f64,
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    app.init_resource::<FarmClock>()
        .add_systems(Update, process.after(crate::health::HealthSystems));
    // A hen eats wheat from the hand (before `food::item_interaction` takes food on mobs).
    app.add_click_handler(ClickPhase::HeldOnTarget, 3260, feed_egg_layer);
    // A cup milks an udder; a turtle drinks nutrients (before `chem::item_interaction`).
    app.add_click_handler(ClickPhase::HeldOnTarget, 3660, |w, c| {
        milk(w, c) || super::critters::turtle_drink(w, c)
    });
}

/// The farm parts of a new basic mob.
pub(super) fn attach(world: &mut World, mob: Entity, def: &MobDef) {
    let path = def.path.as_str();
    let female = world.get::<super::pets::Gender>(mob) == Some(&super::pets::Gender::Female);
    if is_under(path, COW) || (is_under(path, GOAT) && female) {
        world.entity_mut(mob).insert(Udder {
            reagent: MILK,
            volume: UDDER_START,
        });
    }
    if is_under(path, COW) {
        let mut rng = CombatRng(world.resource::<CombatRng>().0);
        let secs = f64::from(rng.range(250, 500)) / 10.0;
        world.resource_mut::<CombatRng>().0 = rng.0;
        world.entity_mut(mob).insert(Tippable {
            tipped: false,
            self_right_secs: secs,
            right_at: 0.0,
            react_at: None,
        });
        if let Some(mut c) = world.get_mut::<AiController>(mob) {
            c.bb.set(Key::TipReacting, Value::Bool(false));
        }
    }
    // BandaStation: the rooster lays no eggs (`can_egg_layer = FALSE`).
    if is_under(path, CHICKEN) && !is_under(path, "/mob/living/basic/chicken/cock") {
        let mut rng = CombatRng(world.resource::<CombatRng>().0);
        let per_feed = rng.range(1, 4);
        world.resource_mut::<CombatRng>().0 = rng.0;
        world.entity_mut(mob).insert(EggLayer {
            eggs_left: 0,
            per_feed,
            max: 8,
        });
    }
    // `/mob/living/basic/chick/permanent` has no `grow_as`.
    if is_under(path, CHICK) && !is_under(path, "/mob/living/basic/chick/permanent") {
        world.entity_mut(mob).insert(Growth {
            percent: 0.0,
            into: CHICKEN,
        });
    }
}

/// Tipped over (tg `TRAIT_IMMOBILIZED` from `TIPPED_OVER`): no steps.
pub fn immobilized(world: &World, mob: Entity) -> bool {
    world.get::<Tippable>(mob).is_some_and(|t| t.tipped)
}

/// `udder/on_examine`.
pub fn examine_lines(world: &World, mob: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(udder) = world.get::<Udder>(mob) else {
        return;
    };
    if world.get::<Stat>(mob) != Some(&Stat::Conscious) {
        return;
    }
    let name = super::defense::name_of(world, mob);
    let percent = (udder.volume / UDDER_SIZE * 100.0).round();
    let text = if percent <= 10.0 {
        format!("Вымя {name} пустое.")
    } else if percent < 100.0 {
        format!("Вымя {name} можно подоить, если есть во что.")
    } else {
        format!("Вымя {name} полное и круглое, его можно подоить, если есть во что.")
    };
    lines.push((Span::Notice, text));
    if world.get::<Tippable>(mob).is_some_and(|t| t.tipped) {
        lines.push((Span::Warning, format!("{name} лежит на спине!")));
    }
}

/// `udder/on_attackby` -> `milk`: a reagent cup on a conscious animal.
fn milk(world: &mut World, ctx: &ClickCtx) -> bool {
    let (Some(cup), mob) = (ctx.held, ctx.target) else {
        return false;
    };
    if world.get::<Udder>(mob).is_none() || ctx.modifiers.right {
        return false;
    }
    let is_cup = world
        .get::<ss13_mapload::DmItem>(cup)
        .is_some_and(|i| is_under(&i.path, CUP_ROOT));
    if !is_cup || world.get::<Stat>(mob) != Some(&Stat::Conscious) {
        return false;
    }
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return false;
    };
    let Some(holder) = world.get::<crate::chem::Reagents>(cup).map(|r| r.0.clone()) else {
        return false;
    };
    let cup_name = super::defense::name_of(world, cup);
    let mob_name = super::defense::name_of(world, mob);
    if holder.total_volume >= holder.maximum_volume {
        crate::tools::tell(
            world,
            ctx.client,
            Span::Warning,
            format!("{cup_name} полон."),
        );
        return true;
    }
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let want = rng.range(5, 10) as f32;
    world.resource_mut::<CombatRng>().0 = rng.0;
    let Some(udder) = world.get::<Udder>(mob).cloned() else {
        return false;
    };
    let amount = want
        .min(udder.volume)
        .min(holder.maximum_volume - holder.total_volume);
    let id = chem.id(udder.reagent);
    if amount <= 0.0 || id.is_none() {
        crate::tools::tell(
            world,
            ctx.client,
            Span::Warning,
            "Вымя пустое. Подождите ещё немного...",
        );
        return true;
    }
    let mut holder = holder;
    let added = holder.add_reagent(
        &chem,
        id.expect("checked"),
        amount,
        REAGENT_TEMP,
        Some(1.0),
        None,
        false,
        false,
    );
    if let Some(mut r) = world.get_mut::<crate::chem::Reagents>(cup) {
        r.0 = holder;
    }
    crate::chem::refresh_container(world, &chem, cup);
    if let Some(mut u) = world.get_mut::<Udder>(mob) {
        u.volume = (u.volume - added.max(0.0)).max(0.0);
    }
    let user = super::defense::name_of(world, ctx.mob);
    crate::chem::visible(
        world,
        ctx.mob,
        &format!("Вы доите {mob_name} в {cup_name}."),
        &format!("{user} доит {mob_name} в {cup_name}."),
        Span::Notice,
    );
    true
}

/// `tippable/interact_with_tippable` (right click with an empty hand, in combat mode): tips the
/// cow over or rights it. True when the click is taken (no shove then).
pub fn tip_click(world: &mut World, actor: Actor, target: Entity) -> bool {
    let Some(tip) = world.get::<Tippable>(target).cloned() else {
        return false;
    };
    if !world.get::<CombatMode>(actor.mob).is_some_and(|c| c.0) {
        return false;
    }
    // `DOING_INTERACTION_WITH_TARGET`.
    if world
        .get::<DoAfter>(actor.mob)
        .is_some_and(|d| d.target == target)
    {
        return true;
    }
    let name = super::defense::name_of(world, target);
    let user = super::defense::name_of(world, actor.mob);
    if tip.tipped {
        crate::chem::visible(
            world,
            actor.mob,
            &format!("Вы начинаете поднимать {name}..."),
            &format!("{user} начинает поднимать {name}."),
            Span::Notice,
        );
        crate::tools::do_after(
            world,
            actor,
            target,
            TIP_DS,
            0,
            crate::construction::Job::Fauna(FarmJob::Untip),
        );
        return true;
    }
    // `try_tip`: not while it is down already.
    if world.get::<Stat>(target) != Some(&Stat::Conscious) {
        return true;
    }
    crate::chem::visible(
        world,
        actor.mob,
        &format!("Вы начинаете опрокидывать {name}..."),
        &format!("{user} начинает опрокидывать {name}."),
        Span::Warning,
    );
    crate::tools::do_after(
        world,
        actor,
        target,
        TIP_DS,
        0,
        crate::construction::Job::Fauna(FarmJob::Tip),
    );
    true
}

/// A finished [`FarmJob`].
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: FarmJob) {
    match job {
        FarmJob::Tip => do_tip(world, mob, action.target),
        FarmJob::Untip => do_untip(world, Some(mob), action.target),
        FarmJob::TurtleDrink => super::critters::turtle_drank(world, mob, action),
    }
}

/// `do_tip` + the cow's `after_cow_tipped`.
pub fn do_tip(world: &mut World, tipper: Entity, cow: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(tip) = world.get::<Tippable>(cow).cloned() else {
        return;
    };
    // Two tippers at once: the second does nothing.
    if tip.tipped {
        return;
    }
    let name = super::defense::name_of(world, cow);
    let user = super::defense::name_of(world, tipper);
    crate::chem::visible(
        world,
        tipper,
        &format!("Вы опрокидываете {name}."),
        &format!("{user} опрокидывает {name}."),
        Span::Warning,
    );
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let react = f64::from(rng.range(100, 200)) / 10.0;
    world.resource_mut::<CombatRng>().0 = rng.0;
    if let Some(mut t) = world.get_mut::<Tippable>(cow) {
        t.tipped = true;
        t.right_at = now + t.self_right_secs;
        t.react_at = Some((now + react, tipper));
    }
    set_rotation(world, cow, 180);
    if let Some(mut c) = world.get_mut::<AiController>(cow) {
        c.movement.stop();
        c.cancel = true;
    }
}

/// `do_untip` (someone rights it) or `right_self` (`None`).
pub fn do_untip(world: &mut World, untipper: Option<Entity>, cow: Entity) {
    if !world.get::<Tippable>(cow).is_some_and(|t| t.tipped) {
        return;
    }
    let name = super::defense::name_of(world, cow);
    match untipper {
        Some(user) => {
            let user_name = super::defense::name_of(world, user);
            crate::chem::visible(
                world,
                user,
                &format!("Вы поднимаете {name}."),
                &format!("{user_name} поднимает {name}."),
                Span::Notice,
            );
        }
        None => super::defense::emote(world, cow, "встаёт на ноги."),
    }
    if let Some(mut t) = world.get_mut::<Tippable>(cow) {
        t.tipped = false;
        t.react_at = None;
    }
    set_rotation(world, cow, 0);
    if let Some(mut c) = world.get_mut::<AiController>(cow) {
        c.bb.set(Key::TipReacting, Value::Bool(false));
        c.bb.clear(Key::Tipper);
    }
}

/// `transform = turn(transform, 180)` while tipped.
fn set_rotation(world: &mut World, mob: Entity, degrees: u16) {
    if let Some(mut look) = world.get_mut::<Appearance>(mob)
        && look.rotation != degrees
    {
        look.rotation = degrees;
    }
}

/// `egg_layer/feed_food`: wheat from the hand, out of combat mode.
fn feed_egg_layer(world: &mut World, ctx: &ClickCtx) -> bool {
    let (Some(food), hen) = (ctx.held, ctx.target) else {
        return false;
    };
    let Some(layer) = world.get::<EggLayer>(hen).cloned() else {
        return false;
    };
    let wheat = world
        .get::<ss13_mapload::DmItem>(food)
        .is_some_and(|i| is_under(&i.path, WHEAT));
    if !wheat || world.get::<CombatMode>(ctx.mob).is_some_and(|c| c.0) {
        return false;
    }
    let name = super::defense::name_of(world, hen);
    if world.get::<Stat>(hen) == Some(&Stat::Dead) {
        crate::tools::tell(world, ctx.client, Span::Warning, format!("{name} мертва!"));
        return true;
    }
    if layer.eggs_left > layer.max {
        crate::tools::tell(
            world,
            ctx.client,
            Span::Warning,
            format!("{name} не выглядит голодной!"),
        );
        return true;
    }
    let food_name = super::defense::name_of(world, food);
    let user = super::defense::name_of(world, ctx.mob);
    crate::chem::visible(
        world,
        ctx.mob,
        &format!("Вы кормите {name} с руки: {food_name}."),
        &format!("{user} кормит {name} с руки: {food_name}."),
        Span::Notice,
    );
    super::defense::emote(world, hen, "довольно кудахчет.");
    world.despawn(food);
    // tg adds `min(eggs_left + eggs_added_from_eating, max)` on top of `eggs_left` (doubling
    // it); the intended total is that minimum.
    if let Some(mut l) = world.get_mut::<EggLayer>(hen) {
        l.eggs_left = (l.eggs_left + l.per_feed).min(l.max);
    }
    true
}

/// The `SSobj` (2 s: udders, egg layers, fertile eggs) and `SSdcs` (1 s: chicks growing up)
/// processing, and the tippable timers.
// Cost: every 2 s a pass over the few udders, hens and eggs; every 1 s over the chicks.
fn process(world: &mut World) {
    let delta = world.resource::<Time>().delta_secs_f64();
    let now = world.resource::<Time>().elapsed_secs_f64();
    let (obj_fires, dcs_fires) = {
        let mut clock = world.resource_mut::<FarmClock>();
        clock.obj += delta;
        clock.dcs += delta;
        let obj = (clock.obj / SSOBJ_WAIT).floor();
        clock.obj -= obj * SSOBJ_WAIT;
        let dcs = (clock.dcs / SSDCS_WAIT).floor();
        clock.dcs -= dcs * SSDCS_WAIT;
        (obj as u32, dcs as u32)
    };
    tip_timers(world, now);
    for _ in 0..obj_fires {
        udders(world);
        lay_eggs(world);
        fertile_eggs(world);
    }
    for _ in 0..dcs_fires {
        grow(world);
    }
}

/// `right_self` after `self_right_time`; `set_tip_react_blackboard` after 10-20 s.
fn tip_timers(world: &mut World, now: f64) {
    let tipped: Vec<(Entity, Tippable)> = world
        .query::<(Entity, &Tippable)>()
        .iter(world)
        .filter(|(_, t)| t.tipped)
        .map(|(e, t)| (e, t.clone()))
        .collect();
    for (cow, tip) in tipped {
        if now >= tip.right_at {
            do_untip(world, None, cow);
            continue;
        }
        if let Some((at, tipper)) = tip.react_at
            && now >= at
        {
            if let Some(mut t) = world.get_mut::<Tippable>(cow) {
                t.react_at = None;
            }
            if let Some(mut c) = world.get_mut::<AiController>(cow) {
                c.bb.set(Key::TipReacting, Value::Bool(true));
                c.bb.set(Key::Tipper, Value::Entity(tipper));
            }
        }
    }
}

/// `udder/process` -> `generate`.
fn udders(world: &mut World) {
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let mut q = world.query::<(&mut Udder, &Stat)>();
    for (mut udder, stat) in q.iter_mut(world) {
        if *stat == Stat::Dead || !rng.chance(UDDER_PRODUCTION_PROB) {
            continue;
        }
        let add = rng.range(5, 10) as f32;
        udder.volume = (udder.volume + add).min(UDDER_SIZE);
    }
    world.resource_mut::<CombatRng>().0 = rng.0;
}

/// tg `SPT_PROB(percent_per_second, seconds)`.
fn spt_prob(rng: &mut CombatRng, percent_per_second: f64, seconds: f64) -> bool {
    let rate = 1.0 - (1.0 - percent_per_second / 100.0).powf(seconds);
    rng.chance((rate * 100.0) as f32)
}

/// `egg_layer/process` and the chicken's `egg_laid`.
fn lay_eggs(world: &mut World) {
    let hens: Vec<(Entity, TilePos)> = world
        .query::<(Entity, &EggLayer, &Stat, &TilePos)>()
        .iter(world)
        .filter(|(_, l, s, _)| l.eggs_left > 0 && **s == Stat::Conscious)
        .map(|(e, _, _, p)| (e, *p))
        .collect();
    for (hen, pos) in hens {
        if world.get::<crate::closets::InCloset>(hen).is_some() {
            continue;
        }
        let mut rng = CombatRng(world.resource::<CombatRng>().0);
        let lays = spt_prob(&mut rng, 1.5, SSOBJ_WAIT);
        let message = EGG_LAYING_MESSAGES[rng.range(0, 3) as usize];
        let offset = [0, 0].map(|_: i16| rng.range(0, 12) as i16 - 6);
        let fertile = rng.chance(25.0);
        world.resource_mut::<CombatRng>().0 = rng.0;
        if !lays {
            continue;
        }
        super::defense::emote(world, hen, message);
        if let Some(mut l) = world.get_mut::<EggLayer>(hen) {
            l.eggs_left -= 1;
        }
        let Some(egg) = crate::construction::spawn_obj(world, ORGANIC_EGG, pos) else {
            continue;
        };
        if let Some(mut look) = world.get_mut::<Appearance>(egg) {
            look.pixel_offset = offset;
        }
        if fertile && chicken_count(world) <= MAX_CHICKENS {
            world.entity_mut(egg).insert(FertileEgg { growth: 0.0 });
        }
    }
}

/// `GLOB.chicken_count`: chickens in the world.
pub fn chicken_count(world: &mut World) -> usize {
    world
        .query::<&BasicMob>()
        .iter(world)
        .filter(|b| is_under(&b.def.path, CHICKEN))
        .count()
}

/// `fertile_egg/process`: grows on a turf, spoils elsewhere, hatches at 200.
fn fertile_eggs(world: &mut World) {
    let eggs: Vec<Entity> = world
        .query_filtered::<Entity, With<FertileEgg>>()
        .iter(world)
        .collect();
    for egg in eggs {
        let on_turf = world.get::<TilePos>(egg).is_some() && world.get::<CarriedBy>(egg).is_none();
        if !on_turf {
            world.entity_mut(egg).remove::<FertileEgg>();
            continue;
        }
        let mut rng = CombatRng(world.resource::<CombatRng>().0);
        let rate = rng.range(1, 2) as f32;
        world.resource_mut::<CombatRng>().0 = rng.0;
        let Some(mut fertile) = world.get_mut::<FertileEgg>(egg) else {
            continue;
        };
        fertile.growth += rate * SSOBJ_WAIT as f32;
        if fertile.growth < EGG_GROWTH_REQUIRED {
            continue;
        }
        let Some(&pos) = world.get::<TilePos>(egg) else {
            continue;
        };
        let name = super::defense::name_of(world, egg);
        let clients = crate::obj_damage::clients_in_range(
            world,
            pos,
            crate::health::DEFAULT_MESSAGE_RANGE,
            &[],
        );
        world.write_message(ss13_net::chat::server::SendChat {
            clients,
            message: ss13_shared::chat::ChatMessage::simple(
                ss13_shared::chat::ChatChannel::Info,
                Span::Notice,
                format!("{name} с тихим треском вылупляется."),
            ),
        });
        world.despawn(egg);
        super::spawn::spawn_mob(world, CHICK, pos);
    }
}

/// `growth_and_differentiation/process` + `grow` (`ready_to_grow`: conscious; a player's chick
/// never grows, `COMSIG_MOB_CLIENT_LOGIN` kills the component).
fn grow(world: &mut World) {
    let chicks: Vec<(Entity, Growth)> = world
        .query::<(Entity, &Growth)>()
        .iter(world)
        .map(|(e, g)| (e, g.clone()))
        .collect();
    for (chick, growth) in chicks {
        if world.get::<crate::server::PlayerMob>(chick).is_some() {
            world.entity_mut(chick).remove::<Growth>();
            continue;
        }
        let stat = world.get::<Stat>(chick).copied();
        if stat == Some(Stat::Dead) {
            world.entity_mut(chick).remove::<Growth>();
            continue;
        }
        if growth.percent < 100.0 {
            // `SPT_PROB(100)` then `rand(0.5, 1)`.
            let mut rng = CombatRng(world.resource::<CombatRng>().0);
            let add = 0.5 + rng.unit() * 0.5;
            world.resource_mut::<CombatRng>().0 = rng.0;
            if let Some(mut g) = world.get_mut::<Growth>(chick) {
                g.percent += add;
            }
            continue;
        }
        let in_closet = world.get::<crate::closets::InCloset>(chick).is_some();
        if stat != Some(Stat::Conscious) || in_closet {
            continue;
        }
        let Some(&pos) = world.get::<TilePos>(chick) else {
            continue;
        };
        let new_name = super::mob_defs(world)
            .0
            .get(growth.into)
            .map(|d| d.name.clone())
            .unwrap_or_default();
        super::defense::emote(world, chick, &format!("вырастает в {new_name}!"));
        world.despawn(chick);
        super::spawn::spawn_mob(world, growth.into, pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut app = App::new();
        app.add_plugins(ss13_world::WorldPlugin);
        app.world_mut()
            .resource_mut::<ss13_world::SpatialIndex>()
            .create_level(1, 20, 20)
            .unwrap();
        let mut world = std::mem::take(app.world_mut());
        world.init_resource::<Time>();
        world.insert_resource(CombatRng(11));
        world
    }

    #[test]
    fn a_tipped_cow_pleads_then_rights_itself() {
        let mut w = world();
        let cow = w
            .spawn((
                TilePos::new(5, 5, 1),
                Stat::Conscious,
                Tippable {
                    tipped: false,
                    self_right_secs: 30.0,
                    right_at: 0.0,
                    react_at: None,
                },
                AiController::default(),
            ))
            .id();
        let farmer = w.spawn((TilePos::new(6, 5, 1), Stat::Conscious)).id();
        do_tip(&mut w, farmer, cow);
        assert!(immobilized(&w, cow));
        let (react_at, tipper) = w.get::<Tippable>(cow).unwrap().react_at.unwrap();
        assert!((10.0..=20.0).contains(&react_at), "{react_at}");
        assert_eq!(tipper, farmer);
        // A second tipper changes nothing.
        do_tip(&mut w, farmer, cow);
        tip_timers(&mut w, 20.5);
        let bb = &w.get::<AiController>(cow).unwrap().bb;
        assert_eq!(bb.get(Key::TipReacting), Some(&Value::Bool(true)));
        assert_eq!(bb.entity(Key::Tipper), Some(farmer));
        tip_timers(&mut w, 30.5);
        assert!(!immobilized(&w, cow), "`self_right_time` passed");
        let bb = &w.get::<AiController>(cow).unwrap().bb;
        assert_eq!(bb.get(Key::TipReacting), Some(&Value::Bool(false)));
    }

    #[test]
    fn spt_prob_scales_with_time() {
        let mut rng = CombatRng(7);
        let hits = (0..10_000).filter(|_| spt_prob(&mut rng, 1.5, 2.0)).count();
        // 1 - 0.985^2 = 2.98%.
        assert!((250..350).contains(&hits), "{hits}");
    }
}
