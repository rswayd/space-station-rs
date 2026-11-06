//! Monster cores (server), after tg `/obj/item/organ/monster_core` and its regenerative cores
//! (`code/modules/mining/equipment/monster_organs/`), with `/obj/item/mining_stabilizer`.
//!
//! - A core decays four minutes after it drops (`time_to_decay`): it turns "decayed", takes
//!   its inert sprite and description and does nothing any more. Its examine tells how fresh
//!   it is.
//! - Stabilizing serum on a fresh core preserves it (no decay, the preserved sprite and
//!   description) and is used up; on a decayed one: "organ decayed!".
//! - A core used in hand or on a living mob (`try_apply`) applies its effect and is used up;
//!   a decayed core or a dead target refuse. The regenerative core's
//!   `/datum/status_effect/regenerative_core`: 25 brute and 25 burn healed, the stuns, knockdowns,
//!   paralysis, immobilisation, unconsciousness and sleep removed (`fully_heal(HEAL_CC_STATUS)`),
//!   the body temperature normal, and the `healsbadman` moodlet.
// shortcut: no implanting (`ORGAN_SLOT_MONSTER_CORE`, the "Regenerate" action and the
// auto-revive in crit) and no damage slowdown immunity for the status effect's minute; the
// rush gland and the brimdust sac are not dropped by the ported fauna. Upgrade with the organ
// slot and the movespeed modifier immunities.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmItem, DmName, IconIndex};
use ss13_shared::Appearance;
use ss13_shared::chat::Span;

use crate::examine::Line;
use crate::health::{DamageType, Health, Stat, update_stat};
use crate::mood::MoodEvent;
use crate::status_effects::{Incap, effects_mut};
use crate::tools::tell;

pub const MONSTER_CORE: &str = "/obj/item/organ/monster_core";
pub const REGENERATIVE_CORE: &str = "/obj/item/organ/monster_core/regenerative_core";
pub const LEGION_CORE: &str = "/obj/item/organ/monster_core/regenerative_core/legion";
pub const STABILIZER: &str = "/obj/item/mining_stabilizer";
/// `time_to_decay`, seconds.
pub const TIME_TO_DECAY: f64 = 240.0;
/// `/datum/status_effect/regenerative_core/on_apply` brute and burn heal.
pub const REGEN_HEAL: f32 = 25.0;

/// `/datum/mood_event/healsbadman` (BandaStation text).
const HEALSBADMAN: MoodEvent = MoodEvent {
    id: "healsbadman",
    description: "Мне кажется, что я держусь на хлипких ниточках и могу развалиться в любой момент!",
    mood_change: -4.0,
    timeout: 120.0,
    hidden: false,
};

/// A monster core's decay: `decay_timer` (the game time it goes inert; `None`: preserved or
/// inert) and `inert`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct MonsterCore {
    pub decay_at: Option<f64>,
    pub inert: bool,
}

/// The `icon_state_*` and `desc_*` of a core type.
struct CoreLook {
    preserved_state: Option<&'static str>,
    inert_state: Option<&'static str>,
    desc_preserved: Option<&'static str>,
    desc_inert: Option<&'static str>,
}

fn look_of(path: &str) -> CoreLook {
    if is_under(path, LEGION_CORE) {
        CoreLook {
            preserved_state: Some("legion_core_stable"),
            inert_state: Some("legion_core_decayed"),
            desc_preserved: Some(
                "The core has been stabilized, allowing you to use it to heal completely without danger of decay.",
            ),
            desc_inert: Some("The core has decayed, and is completely useless."),
        }
    } else if is_under(path, REGENERATIVE_CORE) {
        CoreLook {
            preserved_state: None,
            inert_state: Some("hivelord_core_decayed"),
            desc_preserved: Some(
                "All that remains of a hivelord. It is preserved, allowing you to use it to heal completely without danger of decay.",
            ),
            desc_inert: Some(
                "All that remains of a hivelord. It has decayed, and is completely useless.",
            ),
        }
    } else {
        CoreLook {
            preserved_state: None,
            inert_state: None,
            desc_preserved: None,
            desc_inert: None,
        }
    }
}

pub(super) struct CoresPlugin;

impl Plugin for CoresPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (start_decay, decay).chain());
    }
}

fn now(world: &World) -> f64 {
    world
        .get_resource::<Time>()
        .map_or(0.0, |t| t.elapsed_secs_f64())
}

fn path_of(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmItem>(e).map(|i| i.path.as_str())
}

pub fn is_core(world: &World, e: Entity) -> bool {
    path_of(world, e).is_some_and(|p| is_under(p, MONSTER_CORE))
}

/// `Initialize`: the decay timer starts when the core appears.
fn start_decay(
    time: Res<Time>,
    new: Query<(Entity, &DmItem), (Added<DmItem>, Without<MonsterCore>)>,
    mut commands: Commands,
) {
    let at = time.elapsed_secs_f64() + TIME_TO_DECAY;
    for (e, item) in &new {
        if is_under(&item.path, MONSTER_CORE) {
            commands.entity(e).insert(MonsterCore {
                decay_at: Some(at),
                inert: false,
            });
        }
    }
}

/// The decay timer runs out: `go_inert`.
fn decay(world: &mut World) {
    let t = now(world);
    let due: Vec<Entity> = world
        .query::<(Entity, &MonsterCore)>()
        .iter(world)
        .filter(|(_, c)| c.decay_at.is_some_and(|at| at <= t))
        .map(|(e, _)| e)
        .collect();
    for core in due {
        go_inert(world, core);
    }
}

fn set_icon_state(world: &mut World, e: Entity, state: &str) {
    let Some(icon) = world.get::<DmAtom>(e).map(|a| a.icon.clone()) else {
        return;
    };
    let id = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(&icon, state));
    if let (Some(id), Some(mut look)) = (id, world.get_mut::<Appearance>(e)) {
        look.icon_state = id;
    }
}

/// `go_inert`: the core is useless from now on. False when it already was.
pub fn go_inert(world: &mut World, core: Entity) -> bool {
    let Some(mut c) = world.get_mut::<MonsterCore>(core) else {
        return false;
    };
    if c.inert {
        return false;
    }
    c.inert = true;
    c.decay_at = None;
    let look = path_of(world, core).map(look_of);
    if let Some(mut name) = world.get_mut::<DmName>(core) {
        name.name = format!("decayed {}", name.name);
        if let Some(desc) = look.as_ref().and_then(|l| l.desc_inert) {
            name.desc = desc.to_owned();
        }
    }
    if let Some(state) = look.and_then(|l| l.inert_state) {
        set_icon_state(world, core, state);
    }
    true
}

/// `preserve`: no decay from now on. False when the core is inert.
pub fn preserve(world: &mut World, core: Entity) -> bool {
    let Some(mut c) = world.get_mut::<MonsterCore>(core) else {
        return false;
    };
    if c.inert {
        return false;
    }
    c.decay_at = None;
    let look = path_of(world, core).map(look_of);
    if let Some(desc) = look.as_ref().and_then(|l| l.desc_preserved)
        && let Some(mut name) = world.get_mut::<DmName>(core)
    {
        name.desc = desc.to_owned();
    }
    if let Some(state) = look.and_then(|l| l.preserved_state) {
        set_icon_state(world, core, state);
    }
    true
}

fn inert(world: &World, core: Entity) -> bool {
    world.get::<MonsterCore>(core).is_some_and(|c| c.inert)
}

/// A core used in hand (`attack_self`, `can_perform_action` with `ALLOW_RESTING`). True when
/// `item` is a core.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if !is_core(world, item) {
        return false;
    }
    if crate::status_effects::incapacitation(world, mob).incapacitated
        || world.get::<Stat>(mob).is_none_or(|s| *s != Stat::Conscious)
    {
        return true;
    }
    try_apply(world, client, mob, item, mob);
    true
}

/// A core on a living mob (`interact_with_atom`), serum on a core. True when handled.
// shortcut: this hook runs before `crate::medical`'s, so a core on a patient applies instead of
// going into a surgery; there is no implanting anyway (see the module docs).
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if path_of(world, held).is_some_and(|p| is_under(p, STABILIZER)) {
        if world.get::<MonsterCore>(target).is_none() {
            // tg: `isorgan` but not a monster core.
            if path_of(world, target).is_some_and(|p| is_under(p, "/obj/item/organ")) {
                tell(world, client, Span::Warning, "invalid target!");
                return true;
            }
            return false;
        }
        if !preserve(world, target) {
            tell(world, client, Span::Warning, "organ decayed!");
            return true;
        }
        tell(world, client, Span::Notice, "organ stabilized");
        world.despawn(held);
        return true;
    }
    if !is_core(world, held) || world.get::<Health>(target).is_none() {
        return false;
    }
    try_apply(world, client, mob, held, target);
    true
}

/// `try_apply`, then `apply_to`.
fn try_apply(world: &mut World, client: Entity, user: Entity, core: Entity, target: Entity) {
    if world.get::<Health>(target).is_none() {
        tell(world, client, Span::Warning, "invalid target!");
        return;
    }
    if inert(world, core) {
        tell(world, client, Span::Warning, "organ decayed!");
        return;
    }
    if world.get::<Stat>(target) == Some(&Stat::Dead) {
        tell(world, client, Span::Warning, "they're dead!");
        return;
    }
    tell(world, client, Span::Notice, "applied organ");
    if path_of(world, core).is_some_and(|p| is_under(p, REGENERATIVE_CORE)) {
        apply_regenerative(world, client, user, core, target);
    }
    world.despawn(core);
}

/// `/obj/item/organ/monster_core/regenerative_core/apply_to` and its status effect.
fn apply_regenerative(
    world: &mut World,
    client: Entity,
    user: Entity,
    core: Entity,
    target: Entity,
) {
    crate::mood::add_event(world, target, "legion_core", &HEALSBADMAN);
    let core_name = crate::medical::name_of(world, core);
    if target != user {
        let (u, t) = (
            crate::medical::name_of(world, user),
            crate::medical::name_of(world, target),
        );
        let text = format!(
            "{u} forces {t} to apply {core_name}... Black tendrils entangle and reinforce them!"
        );
        crate::medical::visible(world, user, &text, &text, Span::Notice);
    } else {
        tell(
            world,
            client,
            Span::Notice,
            format!(
                "You start to smear {core_name} on yourself. Disgusting tendrils hold you together and allow you to keep moving, but for how long?"
            ),
        );
    }
    regenerate(world, target);
}

/// `/datum/status_effect/regenerative_core/on_apply`.
pub fn regenerate(world: &mut World, mob: Entity) {
    if let Some(mut stat) = world.get::<Stat>(mob).copied()
        && let Some(mut health) = world.get_mut::<Health>(mob)
    {
        health.adjust(DamageType::Brute, -REGEN_HEAL);
        health.adjust(DamageType::Burn, -REGEN_HEAL);
        let health = health.clone();
        if update_stat(&health, &mut stat).is_some()
            && let Some(mut s) = world.get_mut::<Stat>(mob)
        {
            *s = stat;
        }
    }
    // `fully_heal(HEAL_CC_STATUS)`: every `/datum/status_effect/incapacitating`.
    if let Some(mut effects) = effects_mut(world, mob) {
        for kind in [
            Incap::Stun,
            Incap::Knockdown,
            Incap::Paralyze,
            Incap::Immobilize,
            Incap::Incapacitated,
            Incap::Unconscious,
            Incap::Sleeping,
        ] {
            effects.set(kind, 0.0);
        }
    }
    if let Some(mut r) = world.get_mut::<crate::breath::Respiration>(mob) {
        r.bodytemperature = r.body.normal;
        r.coretemperature = r.body.normal;
    }
}

/// `examine`'s freshness line for `left` seconds until decay (`round(timeleft, 1 MINUTES)`).
pub fn freshness(left: f64) -> Line {
    match (left / 60.0).round() as i64 {
        n if n >= 4 => (
            Span::Notice,
            "It's fresh and still pulsating with the last vestiges of life.".to_owned(),
        ),
        3 => (Span::Notice, "It still looks pretty fresh.".to_owned()),
        2 => (Span::Notice, "It's not as fresh as could be.".to_owned()),
        1 => (
            Span::Notice,
            "Signs of decay are starting to set in. It might not be good for much longer."
                .to_owned(),
        ),
        _ => (
            Span::Warning,
            "Signs of decay have set in, but it still looks alive. It's probably about to become unusable really quickly."
                .to_owned(),
        ),
    }
}

/// `/obj/item/organ/monster_core/examine`: how fresh a decaying core is.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<Line>) {
    if let Some(at) = world.get::<MonsterCore>(target).and_then(|c| c.decay_at) {
        lines.push(freshness(at - now(world)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core(world: &mut World, path: &str) -> Entity {
        world
            .spawn((
                DmItem {
                    path: path.to_owned(),
                    ..default()
                },
                DmName {
                    name: "regenerative core".to_owned(),
                    desc: String::new(),
                },
                MonsterCore {
                    decay_at: Some(TIME_TO_DECAY),
                    inert: false,
                },
            ))
            .id()
    }

    #[test]
    fn decays_unless_preserved() {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        let fresh = core(&mut world, LEGION_CORE);
        let kept = core(&mut world, LEGION_CORE);
        assert!(preserve(&mut world, kept));
        assert_eq!(world.get::<MonsterCore>(kept).unwrap().decay_at, None);
        assert!(
            world
                .get::<DmName>(kept)
                .unwrap()
                .desc
                .contains("stabilized")
        );
        assert!(go_inert(&mut world, fresh));
        assert!(!go_inert(&mut world, fresh));
        assert_eq!(
            world.get::<DmName>(fresh).unwrap().name,
            "decayed regenerative core"
        );
        assert!(!preserve(&mut world, fresh));
    }

    #[test]
    fn regenerate_heals_25_each_and_clears_stuns() {
        let mut world = World::new();
        let mut health = Health::default();
        health.adjust(DamageType::Brute, 40.0);
        health.adjust(DamageType::Burn, 10.0);
        let mob = world
            .spawn((
                health,
                Stat::Conscious,
                crate::status_effects::StatusEffects::default(),
            ))
            .id();
        effects_mut(&mut world, mob)
            .unwrap()
            .set(Incap::Paralyze, 10.0);
        regenerate(&mut world, mob);
        let h = world.get::<Health>(mob).unwrap();
        assert!((h.brute - 15.0).abs() < 1e-3, "{}", h.brute);
        assert!(h.burn.abs() < 1e-3);
        assert_eq!(
            world
                .get::<crate::status_effects::StatusEffects>(mob)
                .unwrap()
                .remaining(Incap::Paralyze),
            0.0
        );
    }

    #[test]
    fn freshness_follows_minutes_left() {
        assert!(freshness(240.0).1.contains("fresh and still"));
        assert!(freshness(170.0).1.contains("pretty fresh"));
        assert!(freshness(65.0).1.contains("starting to set in"));
        assert_eq!(freshness(20.0).0, Span::Warning);
    }
}
