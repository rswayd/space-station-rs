//! Oozes (server), after tg `/mob/living/simple_animal/hostile/ooze` (`ooze.dm`): gelatinous
//! creatures from the vats, mostly meant for players.
//!
//! - Read as basic mobs from the simple animal (`ss13_mapload::mobs`): the gelatinous cube (200
//!   health, 20 brute bites, burn 0.6, tox 0.5), the Sholean grapes (12 brute, burn 0.8); faction
//!   `slime`, through tables and grilles. Without a player they hunt like hostile simple mobs
//!   (`simple_hostile`).
//! - A player's ooze has its own nutrition (50, at most 500): -0.15 every `Life`, starving at 0
//!   (0.25 brute a second). Clicking food it can eat (meat; the grapes also vegetables) eats it:
//!   "ест", the food's nutriment as nutrition; other clicks on the living bite.
//! - The gelatinous cube's actions: Consume — the creature it pulls, after 1.5 s standing still,
//!   is swallowed if organic and alive; every second it takes 5 brute, the cube heals 3% of its
//!   health and gains 3 nutrition; it is spat out dead, on a second press, when the cube dies, or
//!   after 6 s of struggling (resist). Metabolic boost (10 nutrition, 24 s cooldown): 1.5 ds a
//!   tile faster for 6 s, 50 K warmer every second.
//! - Death: everything inside comes out (`content_barfer`).
//!
//! Cost: per tick, one pass over oozes (admin or vat spawns, a handful).
// shortcut: no mending globules or gel cocoons of the grapes, no nutrition HUD (the number is on
// the action buttons), food nutrition is gained at once instead of metabolized over time, no
// ventcrawling, no cell samples, no slime language, no organic check of the swallowed (every
// living mob counts organic); add with the grapes' projectile embedding and the cocoon structure.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmFood, IconIndex};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::server::{UiActed, UiSystems};

use super::BasicMob;
use super::ai::AiController;
use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::click::ClickResolved;
use crate::food::foodtype;
use crate::health::{DamageType, Health, Stat};
use crate::medical::{name_of, visible};
use crate::server::PlayerMob;

pub const OOZE: &str = "/mob/living/simple_animal/hostile/ooze";
pub const GELATINOUS: &str = "/mob/living/simple_animal/hostile/ooze/gelatinous";
pub const GRAPES: &str = "/mob/living/simple_animal/hostile/ooze/grapes";
/// The hostile simple animal AI, as a basic controller.
const SIMPLE_HOSTILE: &str = "/datum/ai_controller/basic_controller/simple/simple_hostile";
const SLIME_DMI: &str = "icons/mob/actions/actions_slime.dmi";
const BACKGROUNDS_DMI: &str = "icons/mob/actions/backgrounds.dmi";
const CONSUME_ID: &str = "ooze:consume";
const BOOST_ID: &str = "ooze:boost";
const EAT_SOUND: &str = "sound/items/eatfood.ogg";
const SPLAT: &str = "sound/effects/splat.ogg";
/// `ooze_nutrition`, `ooze_nutrition_loss`, the clamp.
const START_NUTRITION: f32 = 50.0;
const NUTRITION_LOSS: f32 = 0.15;
const MAX_NUTRITION: f32 = 500.0;
/// `consume` `devour_time`, the struggle out, the `process` wait (`SSprocessing`).
const DEVOUR_SECS: f64 = 1.5;
const STRUGGLE_SECS: f64 = 6.0;
const PROCESS_SECS: f64 = 1.0;
/// `metabolicboost`: cost, cooldown, length, speed-up (deciseconds a tile), heat per second.
const BOOST_COST: f32 = 10.0;
const BOOST_COOLDOWN: f64 = 24.0;
const BOOST_SECS: f64 = 6.0;
const BOOST_SPEEDUP_DS: f32 = 1.5;
const BOOST_HEAT: f64 = 50.0;

/// An ooze's own state.
#[derive(Component, Clone, Debug)]
pub struct Ooze {
    pub nutrition: f32,
    /// `edible_food_types`.
    pub edible: u32,
    pub gelatinous: bool,
    /// The mob being digested (`vored_mob`).
    pub vored: Option<Entity>,
    /// A swallow under way: the victim, when it is done, where both stood.
    swallowing: Option<(Entity, f64, TilePos, TilePos)>,
    next_process: f64,
    /// A victim struggling out since then.
    struggle: Option<(Entity, f64)>,
    boost_until: Option<f64>,
    boost_ready: f64,
    next_heat: f64,
    /// The step delay before the boost (seconds).
    base_delay: f32,
}

pub(super) fn build(app: &mut App) {
    app.add_systems(
        Update,
        (
            trigger_buttons.in_set(UiSystems::Handle),
            (ooze_clicks, tick)
                .chain()
                .after(crate::click::ResolveClicks)
                .before(crate::health::HealthSystems),
        ),
    );
}

fn now(world: &World) -> f64 {
    crate::tools::now(world)
}

/// `Initialize` of an ooze: its state, and the hostile AI simple animals have.
pub(super) fn attach(world: &mut World, mob: Entity, def: &ss13_mapload::mobs::MobDef) {
    if !is_under(&def.path, OOZE) {
        return;
    }
    let gelatinous = is_under(&def.path, GELATINOUS);
    let edible = if is_under(&def.path, GRAPES) {
        foodtype::MEAT | foodtype::VEGETABLES
    } else {
        foodtype::MEAT
    };
    let base_delay = world.get::<ss13_world::Mover>(mob).map_or(0.1, |m| m.delay);
    let mut hostile = def.clone();
    hostile.ai_controller = SIMPLE_HOSTILE.to_owned();
    let mut e = world.entity_mut(mob);
    e.insert(Ooze {
        nutrition: START_NUTRITION,
        edible,
        gelatinous,
        vored: None,
        swallowing: None,
        next_process: 0.0,
        struggle: None,
        boost_until: None,
        boost_ready: 0.0,
        next_heat: 0.0,
        base_delay,
    });
    if def.ai_controller.is_empty()
        && let Some(controller) = AiController::for_mob(&hostile)
    {
        if controller.retaliates {
            e.insert(super::ai::Retaliates);
        }
        e.insert(controller);
    }
}

fn is_player(world: &World, mob: Entity) -> bool {
    world.get::<PlayerMob>(mob).is_some()
}

/// `adjust_ooze_nutrition`.
fn adjust_nutrition(world: &mut World, mob: Entity, amount: f32) {
    if let Some(mut o) = world.get_mut::<Ooze>(mob) {
        o.nutrition = (o.nutrition + amount).clamp(0.0, MAX_NUTRITION);
    }
}

fn sound(world: &mut World, pos: TilePos, file: &str, vol: f32) {
    crate::sound::playsound(world, crate::sound::PlaySound::new(pos, file, vol, true));
}

/// A player ooze's clicks: food it can eat is eaten (`resolve_unarmed_attack` -> `eat_atom`),
/// the living are bitten.
fn ooze_clicks(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        let (mob, target) = (click.mob, click.target);
        if click.modifiers.right
            || mob == target
            || world.get::<Ooze>(mob).is_none()
            || world.get::<Stat>(mob) != Some(&Stat::Conscious)
        {
            continue;
        }
        let near = click
            .mob_pos
            .chebyshev(click.target_pos)
            .is_some_and(|d| d <= 1);
        if near && eat_atom(world, mob, target) {
            continue;
        }
        if world.get::<Health>(target).is_some() {
            super::defense::melee_attack(world, mob, target);
        }
    }
}

/// `eat_atom` -> `edible/on_ooze_eat`: food of an edible type is eaten whole. True when it was.
pub fn eat_atom(world: &mut World, mob: Entity, food: Entity) -> bool {
    let Some(edible) = world.get::<Ooze>(mob).map(|o| o.edible) else {
        return false;
    };
    let Some(types) = world.get::<DmFood>(food).map(|f| f.foodtypes) else {
        return false;
    };
    if types & edible == 0 {
        return false;
    }
    // The food's reagents, metabolized at once: their nutriment.
    let gain = match (
        world
            .get_resource::<crate::chem::Chem>()
            .map(|c| c.0.clone()),
        world.get::<crate::chem::Reagents>(food),
    ) {
        (Some(chem), Some(holder)) => holder
            .0
            .reagents
            .iter()
            .map(|r| ss13_reagents::metabolism::nutriment_factor(&chem, r) * r.volume)
            .sum(),
        _ => 0.0,
    };
    let (who, what) = (name_of(world, mob), name_of(world, food));
    visible(
        world,
        mob,
        &format!("Вы едите {what}."),
        &format!("{} ест {what}!", ss13_shared::capitalize(&who)),
        Span::Warning,
    );
    if let Some(&pos) = world.get::<TilePos>(mob) {
        let vol = world
            .resource_mut::<crate::combat::CombatRng>()
            .range(30, 50) as f32;
        sound(world, pos, EAT_SOUND, vol);
    }
    world.despawn(food);
    adjust_nutrition(world, mob, gain);
    refresh_buttons(world, mob);
    true
}

/// The action buttons of a player's gelatinous cube.
fn refresh_buttons(world: &mut World, mob: Entity) {
    let Some(o) = world.get::<Ooze>(mob).cloned() else {
        return;
    };
    if !o.gelatinous || !is_player(world, mob) {
        return;
    }
    let icon = |world: &World, dmi: &str, state: &str| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(dmi, state))
    };
    let conscious = world.get::<Stat>(mob) == Some(&Stat::Conscious);
    let nutrition = Some(format!("{}", o.nutrition.round() as i32));
    let (consume_name, consume_icon) = if o.vored.is_some() {
        ("Выплюнуть", "eject")
    } else {
        ("Поглотить", "consume")
    };
    let mut buttons: Vec<(ActionButton, f64)> = world
        .get::<ActionButtons>(mob)
        .map(|b| {
            b.0.iter()
                .filter(|(button, _)| button.id != CONSUME_ID && button.id != BOOST_ID)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    buttons.push((
        ActionButton {
            id: CONSUME_ID.to_owned(),
            name: consume_name.to_owned(),
            icon: icon(world, SLIME_DMI, consume_icon),
            background: icon(world, BACKGROUNDS_DMI, "bg_hive"),
            border: icon(world, BACKGROUNDS_DMI, "bg_hive_border"),
            available: conscious,
            cooldown: 0.0,
            active: o.vored.is_some(),
            maptext: nutrition.clone(),
        },
        0.0,
    ));
    buttons.push((
        ActionButton {
            id: BOOST_ID.to_owned(),
            name: "Метаболическое ускорение".to_owned(),
            icon: icon(world, SLIME_DMI, "metabolic_boost"),
            background: icon(world, BACKGROUNDS_DMI, "bg_hive"),
            border: icon(world, BACKGROUNDS_DMI, "bg_hive_border"),
            available: conscious && o.nutrition >= BOOST_COST && o.boost_until.is_none(),
            cooldown: 0.0,
            active: o.boost_until.is_some(),
            maptext: nutrition,
        },
        o.boost_ready,
    ));
    set_buttons(world, mob, buttons);
}

fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if (id != CONSUME_ID && id != BOOST_ID)
            || actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world.get::<Ooze>(actor).is_none_or(|o| !o.gelatinous)
            || world.get::<Stat>(actor) != Some(&Stat::Conscious)
        {
            continue;
        }
        if id == CONSUME_ID {
            consume_trigger(world, actor);
        } else {
            boost(world, actor);
        }
        refresh_buttons(world, actor);
    }
}

/// `consume/Trigger`: spit out what it digests, else start swallowing what it pulls.
fn consume_trigger(world: &mut World, mob: Entity) {
    if world.get::<Ooze>(mob).and_then(|o| o.vored).is_some() {
        stop_consuming(world, mob);
        return;
    }
    let pulled = world
        .get::<crate::pull::Pulling>(mob)
        .map(|p| p.0)
        .filter(|&p| world.get::<Health>(p).is_some());
    let Some(victim) = pulled else {
        crate::tools::tell_mob(
            world,
            mob,
            Span::Warning,
            "Для этого нужно тащить существо!",
        );
        return;
    };
    let (Some(&at), Some(&victim_at)) = (world.get::<TilePos>(mob), world.get::<TilePos>(victim))
    else {
        return;
    };
    let (who, whom) = (name_of(world, mob), name_of(world, victim));
    visible(
        world,
        mob,
        &format!("Вы пытаетесь поглотить {whom}."),
        &format!("{who} пытается поглотить {whom}!"),
        Span::Warning,
    );
    let done = now(world) + DEVOUR_SECS;
    if let Some(mut o) = world.get_mut::<Ooze>(mob) {
        o.swallowing = Some((victim, done, at, victim_at));
    }
}

/// `start_consuming`.
fn start_consuming(world: &mut World, mob: Entity, victim: Entity) {
    if world.get::<Stat>(victim) == Some(&Stat::Dead) {
        crate::tools::tell_mob(world, mob, Span::Warning, "Это существо мне не по вкусу!");
        return;
    }
    // Inside it (hidden, kept on its tile below).
    crate::closets::put_mob_inside(world, victim, mob);
    let next = now(world) + PROCESS_SECS;
    if let Some(mut o) = world.get_mut::<Ooze>(mob) {
        o.vored = Some(victim);
        o.next_process = next;
    }
    if let Some(&pos) = world.get::<TilePos>(mob) {
        let vol = world
            .resource_mut::<crate::combat::CombatRng>()
            .range(30, 50) as f32;
        sound(world, pos, EAT_SOUND, vol);
        world.entity_mut(victim).insert(pos);
    }
    let (who, whom) = (name_of(world, mob), name_of(world, victim));
    visible(
        world,
        mob,
        &format!("Вы поглощаете {whom}."),
        &format!("{who} поглощает {whom}!"),
        Span::Warning,
    );
}

/// `stop_consuming`: the victim is spat out on the ooze's tile.
pub fn stop_consuming(world: &mut World, mob: Entity) {
    let Some(victim) = world.get_mut::<Ooze>(mob).and_then(|mut o| {
        o.struggle = None;
        o.vored.take()
    }) else {
        return;
    };
    if world.get_entity(victim).is_err() {
        return;
    }
    if let Some(&pos) = world.get::<TilePos>(mob) {
        world.entity_mut(victim).insert(pos);
        crate::closets::let_out(world, victim);
        sound(world, pos, SPLAT, 50.0);
    }
    let (who, whom) = (name_of(world, mob), name_of(world, victim));
    visible(
        world,
        mob,
        &format!("Вы выплёвываете {whom}."),
        &format!("{who} выплёвывает {whom}!"),
        Span::Warning,
    );
    refresh_buttons(world, mob);
}

/// `metabolicboost/Activate`.
fn boost(world: &mut World, mob: Entity) {
    let now = now(world);
    let Some(o) = world.get::<Ooze>(mob).cloned() else {
        return;
    };
    if o.nutrition < BOOST_COST || o.boost_until.is_some() || now < o.boost_ready {
        return;
    }
    if let Some(mut mover) = world.get_mut::<ss13_world::Mover>(mob) {
        mover.delay = (o.base_delay - BOOST_SPEEDUP_DS / 10.0).max(0.0);
    }
    if let Some(mut o) = world.get_mut::<Ooze>(mob) {
        o.boost_until = Some(now + BOOST_SECS);
        o.next_heat = now + 1.0;
        o.boost_ready = now + BOOST_SECS + BOOST_COOLDOWN;
    }
    adjust_nutrition(world, mob, -BOOST_COST);
    crate::tools::tell_mob(
        world,
        mob,
        Span::Notice,
        "Вы чувствуете себя гораздо быстрее.",
    );
}

/// `container_resist_act` of a gelatinous cube: 6 s of struggling and it spits the victim out.
pub fn container_resist(world: &mut World, victim: Entity) {
    let Some(mob) = world
        .get::<crate::closets::InCloset>(victim)
        .map(|c| c.closet)
    else {
        return;
    };
    let now = now(world);
    let Some(mut o) = world.get_mut::<Ooze>(mob) else {
        return;
    };
    if o.vored != Some(victim) || o.struggle.is_some() {
        return;
    }
    o.struggle = Some((victim, now + STRUGGLE_SECS));
    crate::tools::tell_mob(
        world,
        victim,
        Span::Notice,
        "Вы пытаетесь вырваться наружу...",
    );
}

/// `Life` nutrition, the swallow's `do_after`, digestion, the struggle, the boost.
fn tick(world: &mut World, mut last_life: Local<u64>) {
    let now = now(world);
    let fired = world.resource::<crate::breath::MobsClock>().times_fired;
    let lives = fired.saturating_sub(*last_life);
    *last_life = fired;
    let oozes: Vec<Entity> = world
        .query_filtered::<Entity, With<Ooze>>()
        .iter(world)
        .collect();
    for mob in oozes {
        let Some(o) = world.get::<Ooze>(mob).cloned() else {
            continue;
        };
        let alive = world.get::<Stat>(mob).is_some_and(|s| *s != Stat::Dead);
        // `Life`: a player's ooze hungers.
        if lives > 0 && alive && is_player(world, mob) {
            for _ in 0..lives {
                adjust_nutrition(world, mob, -NUTRITION_LOSS);
            }
            let starving = world.get::<Ooze>(mob).is_some_and(|o| o.nutrition <= 0.0);
            if starving {
                let secs = crate::breath::SSMOBS_WAIT_SECS as f32 * lives as f32;
                super::defense::hurt(world, mob, DamageType::Brute, 0.25 * secs, None);
            }
            refresh_buttons(world, mob);
        }
        // The swallow's `do_after(ooze, 1.5 s, eat_target)`.
        if let Some((victim, done, at, victim_at)) = o.swallowing
            && now >= done
        {
            if let Some(mut o) = world.get_mut::<Ooze>(mob) {
                o.swallowing = None;
            }
            let still = world.get::<TilePos>(mob) == Some(&at)
                && world.get::<TilePos>(victim) == Some(&victim_at)
                && world.get::<crate::pull::Pulling>(mob).map(|p| p.0) == Some(victim)
                && world.get::<Stat>(mob) == Some(&Stat::Conscious);
            if still && o.vored.is_none() {
                start_consuming(world, mob, victim);
                refresh_buttons(world, mob);
            }
        }
        // `consume/process`.
        if let Some(victim) = o.vored
            && now >= o.next_process
        {
            if let Some(mut o) = world.get_mut::<Ooze>(mob) {
                o.next_process = now + PROCESS_SECS;
            }
            if world.get_entity(victim).is_err() {
                if let Some(mut o) = world.get_mut::<Ooze>(mob) {
                    o.vored = None;
                }
            } else {
                // The victim rides inside along.
                if let Some(&pos) = world.get::<TilePos>(mob)
                    && world.get::<TilePos>(victim) != Some(&pos)
                {
                    world.entity_mut(victim).insert(pos);
                }
                super::defense::hurt(world, victim, DamageType::Brute, 5.0, None);
                let heal = world.get::<Health>(mob).map_or(0.0, |h| h.max * 0.03);
                super::defense::hurt(world, mob, DamageType::Brute, -heal, None);
                adjust_nutrition(world, mob, 3.0);
                if world.get::<Stat>(victim) == Some(&Stat::Dead) {
                    stop_consuming(world, mob);
                } else {
                    refresh_buttons(world, mob);
                }
            }
        }
        if let Some((_, at)) = world.get::<Ooze>(mob).and_then(|o| o.struggle)
            && now >= at
        {
            stop_consuming(world, mob);
        }
        // The boost's heat every second and its end.
        if let Some(until) = o.boost_until {
            if now >= o.next_heat && now < until {
                if let Some(mut t) = world.get_mut::<super::BodyTemperature>(mob) {
                    t.0 += BOOST_HEAT;
                }
                if let Some(mut o) = world.get_mut::<Ooze>(mob) {
                    o.next_heat += 1.0;
                }
            }
            if now >= until {
                if let Some(mut mover) = world.get_mut::<ss13_world::Mover>(mob) {
                    mover.delay = o.base_delay;
                }
                if let Some(mut o) = world.get_mut::<Ooze>(mob) {
                    o.boost_until = None;
                }
                crate::tools::tell_mob(world, mob, Span::Notice, "Вы снова замедляетесь.");
                refresh_buttons(world, mob);
            }
        }
    }
}

/// `content_barfer` and `consume`'s `COMSIG_LIVING_DEATH`: what it digests comes out.
pub(super) fn on_death(world: &mut World, mob: Entity) {
    if world.get::<Ooze>(mob).is_some() {
        stop_consuming(world, mob);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grapes_also_eat_vegetables() {
        let mut world = World::new();
        let mob = world.spawn_empty().id();
        let mut def = ss13_mapload::mobs::MobDef::default();
        def.path = GRAPES.to_owned();
        attach(&mut world, mob, &def);
        let o = world.get::<Ooze>(mob).unwrap();
        assert_eq!(o.edible, foodtype::MEAT | foodtype::VEGETABLES);
        assert!(!o.gelatinous);
        assert_eq!(o.nutrition, START_NUTRITION);
        let cube = world.spawn_empty().id();
        def.path = GELATINOUS.to_owned();
        attach(&mut world, cube, &def);
        let o = world.get::<Ooze>(cube).unwrap();
        assert!(o.gelatinous);
        assert_eq!(o.edible, foodtype::MEAT);
    }
}
