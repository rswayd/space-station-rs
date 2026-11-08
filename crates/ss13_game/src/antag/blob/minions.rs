//! Blob minions after tg `/mob/living/basic/blob_minion` (`blob_minions/*.dm`) and
//! `/datum/component/blob_minion`: factory spores that raise human corpses into blob zombies, and
//! blobbernauts played by ghosts.
//!
//! - **Spores** ([`spawn_spore`]): a factory keeps up to 3 (one every 8 s while it has no living
//!   blobbernaut); hostile basic mobs of the blob faction (`crate::mobs`, the `blob_spore`
//!   controller fights like `simple_hostile`). A spore next to a dead human raises it
//!   ([`zombify`]): a blob zombie with the corpse inside, which falls out when the zombie dies.
//! - **Blobbernauts** ([`spawn_blobbernaut`], from `super::create_blobbernaut`): a ghost polled
//!   for 20 s (`POLL_IGNORE_BLOB`; nobody: refund) takes a 200-health brute, starting at half; its factory drops to a quarter of
//!   its integrity while it lives. Near the core it heals 5% a second, near a node 2.5%; away from
//!   blobs or without its factory it loses 1.25% a second for each ([`blobbernaut_life`]). Its
//!   clicks attack (4 brute and 20 units of the strain's reagent, 20 brute without an overmind;
//!   60 to objects, none to blobs).
//! - The factory gone: its spores and zombies die, its blobbernaut is orphaned. The overmind gone:
//!   minions lose the strain's colour and the blobbernaut hits for 20 again.
//! - A blob touching a hurt minion heals it by 1.25% of its health (`on_blob_touched`).
// shortcut: no rally, no telepathy, no death clouds of the strain's reagent, spores raise corpses next to them
// instead of attacking them, a zombie's corpse is hidden where the zombie stands (it does not
// show through it), no z-level death of factory spores, no distributed neurons ghost control.

use std::sync::Arc;

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Appearance, TilePos};
use ss13_world::PassFlags;

use super::structures::{Blob, Kind, blobs_in_range};
use super::{FACTION_BLOB, PASSBLOB, attack_living_volume, hurt, strain_of};
use crate::click::ClickResolved;
use crate::combat::{CLICK_CD_MELEE, NextMove};
use crate::health::{DamageType, Health, Stat};
use crate::mobs::{BasicMob, Faction};
use crate::server::PlayerMob;

pub const SPORE: &str = "/mob/living/basic/blob_minion/spore";
pub const SPORE_MINION: &str = "/mob/living/basic/blob_minion/spore/minion";
pub const ZOMBIE_CONTROLLED: &str = "/mob/living/basic/blob_minion/zombie/controlled";
pub const BLOBBERNAUT_MINION: &str = "/mob/living/basic/blob_minion/blobbernaut/minion";
pub const MINION_RUNTIME_TYPES: [&str; 4] =
    [SPORE, SPORE_MINION, ZOMBIE_CONTROLLED, BLOBBERNAUT_MINION];

/// `BLOBMOB_HEALING_MULTIPLIER`.
const TOUCH_HEALING: f32 = 0.0125;
/// `BLOBMOB_BLOBBERNAUT_HEALING_CORE`, `_NODE`, `_HEALTH_DECAY` (per second).
const NAUT_HEAL_CORE: f32 = 0.05;
const NAUT_HEAL_NODE: f32 = 0.025;
const NAUT_DECAY: f32 = 0.0125;
/// `BLOBMOB_BLOBBERNAUT_DMG_*`, `_DMG_OBJ`.
const NAUT_DAMAGE: f32 = 4.0;
const NAUT_DAMAGE_SOLO: f32 = 20.0;
const NAUT_OBJ_DAMAGE: f32 = 60.0;
/// The blobbernaut's `Life` interval here (`SSmobs`), seconds.
const LIFE_SECS: f64 = 2.0;

/// What kind of minion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MinionKind {
    Spore,
    Zombie,
    Blobbernaut,
}

/// tg `/datum/component/blob_minion` plus the factory link (`link_to_factory`).
#[derive(Component, Clone, Debug)]
pub struct BlobMinion {
    pub kind: MinionKind,
    pub overmind: Option<Entity>,
    pub factory: Option<Entity>,
    /// A blobbernaut whose factory died (`orphaned`).
    pub orphaned: bool,
}

/// On a corpse inside a blob zombie.
#[derive(Component, Clone, Copy, Debug)]
pub struct ZombieCorpse {
    pub zombie: Entity,
}

#[derive(Resource, Default)]
struct MinionClock {
    next_life: f64,
}

pub(super) fn build(app: &mut App) {
    app.init_resource::<MinionClock>().add_systems(
        Update,
        (
            (
                zombify_near_corpses,
                sync_zombie_corpses,
                minion_links,
                blobbernaut_life,
            )
                .chain()
                .after(crate::health::HealthSystems),
            blobbernaut_clicks
                .after(crate::click::ResolveClicks)
                .before(crate::health::HealthSystems),
        ),
    );
}

/// Makes a fresh basic mob a minion of `overmind` (`AddComponent(/datum/component/blob_minion)`).
fn make_minion(
    world: &mut World,
    mob: Entity,
    kind: MinionKind,
    overmind: Option<Entity>,
    factory: Option<Entity>,
) {
    world.entity_mut(mob).insert((
        BlobMinion {
            kind,
            overmind,
            factory,
            orphaned: false,
        },
        Faction(vec![FACTION_BLOB.to_owned()]),
        PassFlags(PASSBLOB),
    ));
    strain_changed(world, mob);
}

/// A spore of `factory` on its tile (`create_spore`).
pub fn spawn_spore(world: &mut World, factory: Entity, overmind: Option<Entity>) -> Option<Entity> {
    let pos = *world.get::<TilePos>(factory)?;
    let spore = crate::mobs::spawn::spawn_mob(world, SPORE_MINION, pos)?;
    make_minion(world, spore, MinionKind::Spore, overmind, Some(factory));
    Some(spore)
}

/// A spore without factory on `pos` (`victory_sequence`'s, over each body).
pub fn spawn_free_spore(world: &mut World, overmind: Entity, pos: TilePos) {
    if let Some(spore) = crate::mobs::spawn::spawn_mob(world, SPORE, pos) {
        make_minion(world, spore, MinionKind::Spore, Some(overmind), None);
    }
}

/// Spores and zombies alive of `factory` (`spores_and_zombies`).
pub fn spores_of(world: &mut World, factory: Entity) -> usize {
    world
        .query::<(&BlobMinion, &Stat)>()
        .iter(world)
        .filter(|(m, s)| {
            m.factory == Some(factory) && m.kind != MinionKind::Blobbernaut && **s != Stat::Dead
        })
        .count()
}

/// The overmind's strain on a minion: its colour (`on_update_appearance`) and, for a
/// blobbernaut, its damage (`on_strain_updated`).
pub fn strain_changed(world: &mut World, mob: Entity) {
    let Some(minion) = world.get::<BlobMinion>(mob).cloned() else {
        return;
    };
    let strain = minion.overmind.and_then(|o| strain_of(world, o));
    if let Some(mut look) = world.get_mut::<Appearance>(mob) {
        look.color = strain.map(|s| s.color());
    }
    if minion.kind == MinionKind::Blobbernaut
        && let Some(mut basic) = world.get_mut::<BasicMob>(mob)
    {
        let damage = if strain.is_some() {
            NAUT_DAMAGE
        } else {
            NAUT_DAMAGE_SOLO
        };
        if basic.def.melee_damage_upper != damage {
            let mut def = (*basic.def).clone();
            def.melee_damage_lower = damage;
            def.melee_damage_upper = damage;
            if let Some(s) = strain {
                def.attack_verb_continuous = s.def().blobbernaut_message.to_owned();
            }
            basic.def = Arc::new(def);
        }
    }
}

/// `on_blob_touched`: a living, hurt minion heals 1.25% of its health.
pub fn blob_touched(world: &mut World, mob: Entity) {
    if world.get::<Stat>(mob).is_none_or(|s| *s == Stat::Dead) {
        return;
    }
    let Some(health) = world.get::<Health>(mob) else {
        return;
    };
    if health.health() >= health.max {
        return;
    }
    let amount = health.max * TOUCH_HEALING;
    hurt(world, mob, DamageType::Tox, -amount);
}

/// The factory is gone: its spores and zombies die, its blobbernaut is orphaned
/// (`on_factory_destroyed`).
pub fn factory_destroyed(world: &mut World, factory: Entity) {
    let linked: Vec<(Entity, MinionKind)> = world
        .query::<(Entity, &BlobMinion)>()
        .iter(world)
        .filter(|(_, m)| m.factory == Some(factory))
        .map(|(e, m)| (e, m.kind))
        .collect();
    for (mob, kind) in linked {
        if let Some(mut m) = world.get_mut::<BlobMinion>(mob) {
            m.factory = None;
            m.orphaned = kind == MinionKind::Blobbernaut;
        }
        match kind {
            MinionKind::Blobbernaut => crate::tools::tell_mob(
                world,
                mob,
                Span::Danger,
                "Ваша фабрика уничтожена! Вы чувствуете, что умираете!",
            ),
            _ => {
                if kind == MinionKind::Zombie {
                    crate::tools::tell_mob(
                        world,
                        mob,
                        Span::Danger,
                        "Ваша фабрика уничтожена! Вы чувствуете, что умираете!",
                    );
                }
                crate::medical::organs::kill(world, mob);
            }
        }
    }
}

/// Factories and overminds that went away without telling (deleted by other means), and
/// overmind strains changing: every tick, cheap (a handful of minions).
fn minion_links(world: &mut World) {
    let minions: Vec<(Entity, BlobMinion)> = world
        .query::<(Entity, &BlobMinion)>()
        .iter(world)
        .map(|(e, m)| (e, m.clone()))
        .collect();
    for (mob, m) in minions {
        if let Some(f) = m.factory
            && world.get::<Blob>(f).is_none()
        {
            factory_destroyed(world, f);
        }
        if let Some(o) = m.overmind
            && world.get::<super::BlobOvermind>(o).is_none()
        {
            if let Some(mut m) = world.get_mut::<BlobMinion>(mob) {
                m.overmind = None;
            }
            strain_changed(world, mob);
        }
    }
}

/// A spore next to a dead human raises it (`zombify`).
fn zombify_near_corpses(world: &mut World) {
    let spores: Vec<(Entity, TilePos, Option<Entity>, Option<Entity>)> = world
        .query::<(Entity, &BlobMinion, &TilePos, &Stat)>()
        .iter(world)
        .filter(|(_, m, _, s)| m.kind == MinionKind::Spore && **s == Stat::Conscious)
        .map(|(e, m, p, _)| (e, *p, m.overmind, m.factory))
        .collect();
    for (spore, pos, overmind, factory) in spores {
        let corpse = {
            let index = world.resource::<ss13_world::SpatialIndex>();
            let mut found = None;
            'outer: for dy in -1..=1 {
                for dx in -1..=1 {
                    let p = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
                    if !index.in_bounds(p) {
                        continue;
                    }
                    for &e in index.on_tile(p) {
                        if is_raisable(world, e) {
                            found = Some(e);
                            break 'outer;
                        }
                    }
                }
            }
            found
        };
        if let Some(corpse) = corpse {
            zombify(world, spore, corpse, overmind, factory);
        }
    }
}

/// A dead human not already in a zombie, a closet or a morph.
fn is_raisable(world: &World, e: Entity) -> bool {
    world.get::<crate::inventory::Inventory>(e).is_some()
        && world.get::<crate::silicon::Silicon>(e).is_none()
        && world.get::<Stat>(e) == Some(&Stat::Dead)
        && world.get::<crate::closets::InCloset>(e).is_none()
        && world.get::<ZombieCorpse>(e).is_none()
}

/// tg `zombify`: "The corpse of X suddenly rises!"; the spore becomes a blob zombie with the body
/// inside.
pub fn zombify(
    world: &mut World,
    spore: Entity,
    corpse: Entity,
    overmind: Option<Entity>,
    factory: Option<Entity>,
) -> Option<Entity> {
    let pos = *world.get::<TilePos>(spore)?;
    let name = crate::medical::name_of(world, corpse);
    for m in crate::pull::visible_message(
        world,
        pos,
        ChatChannel::Warning,
        (Span::Warning, format!("Труп {name} внезапно поднимается!")),
        &[],
    ) {
        world.write_message(m);
    }
    let zombie = crate::mobs::spawn::spawn_mob(world, ZOMBIE_CONTROLLED, pos)?;
    make_minion(world, zombie, MinionKind::Zombie, overmind, factory);
    crate::closets::put_mob_inside(world, corpse, zombie);
    world.entity_mut(corpse).insert(ZombieCorpse { zombie });
    world.despawn(spore);
    info!("{spore} (blob spore) raised {corpse} into {zombie}");
    Some(zombie)
}

/// The corpse goes where its zombie goes and falls out when the zombie dies or is gone.
fn sync_zombie_corpses(world: &mut World) {
    let corpses: Vec<(Entity, Entity)> = world
        .query::<(Entity, &ZombieCorpse)>()
        .iter(world)
        .map(|(e, z)| (e, z.zombie))
        .collect();
    for (corpse, zombie) in corpses {
        let alive = world.get::<Stat>(zombie).is_some_and(|s| *s != Stat::Dead);
        let at = world.get::<TilePos>(zombie).copied();
        if let Some(at) = at
            && world.get::<TilePos>(corpse) != Some(&at)
        {
            world.entity_mut(corpse).insert(at);
        }
        if alive {
            continue;
        }
        world.entity_mut(corpse).remove::<ZombieCorpse>();
        crate::closets::let_out(world, corpse);
        if let Some(&pos) = world.get::<TilePos>(corpse) {
            crate::construction::tile_changed(world, pos);
        }
    }
}

/// A blobbernaut for `ghost` on `factory` (`on_poll_concluded` + `assign_key`).
pub fn spawn_blobbernaut(
    world: &mut World,
    factory: Entity,
    overmind: Entity,
    client: Entity,
    ghost: Entity,
) -> Option<Entity> {
    let pos = *world.get::<TilePos>(factory)?;
    let naut = crate::mobs::spawn::spawn_mob(world, BLOBBERNAUT_MINION, pos)?;
    make_minion(
        world,
        naut,
        MinionKind::Blobbernaut,
        Some(overmind),
        Some(factory),
    );
    // "Start out injured to encourage not beelining away from the blob."
    if let Some(max) = world.get::<Health>(naut).map(|h| h.max) {
        // Toxin: the basic mob's `physiology` leaves it whole (brute is halved).
        hurt(world, naut, DamageType::Tox, max / 2.0);
    }
    if !crate::medical::brainmob::possess(world, client, ghost, naut) {
        world.despawn(naut);
        return None;
    }
    world.try_despawn(ghost).ok();
    let strain = strain_of(world, overmind);
    let tell =
        |world: &mut World, span, text: String| crate::tools::tell(world, client, span, text);
    tell(
        world,
        Span::Notice,
        "Вы сильны, вас трудно убить, и вы медленно восстанавливаетесь рядом с узлами и ядром, но \
         медленно умираете вдали от блоба или если создавшая вас фабрика уничтожена."
            .to_owned(),
    );
    if let Some(s) = strain {
        tell(
            world,
            Span::Notice,
            format!("Реагент вашего надзирателя: {}!", s.def().name),
        );
        tell(
            world,
            Span::Notice,
            format!("Реагент {} {}", s.def().name, s.def().description),
        );
    }
    info!("{naut} ({client}) is a blobbernaut of {overmind}");
    Some(naut)
}

/// `/mob/living/basic/blob_minion/blobbernaut/minion/Life`.
fn blobbernaut_life(world: &mut World) {
    let t = crate::tools::now(world);
    {
        let mut clock = world.resource_mut::<MinionClock>();
        if t < clock.next_life {
            return;
        }
        clock.next_life = t + LIFE_SECS;
    }
    let nauts: Vec<(Entity, TilePos, bool)> = world
        .query::<(Entity, &BlobMinion, &TilePos, &Stat)>()
        .iter(world)
        .filter(|(_, m, _, s)| m.kind == MinionKind::Blobbernaut && **s == Stat::Conscious)
        .map(|(e, m, p, _)| (e, *p, m.orphaned || m.factory.is_none()))
        .collect();
    let secs = LIFE_SECS as f32;
    for (naut, pos, orphaned) in nauts {
        let near = blobs_in_range(world, pos, 2);
        let Some(max) = world.get::<Health>(naut).map(|h| h.max) else {
            continue;
        };
        let mut sources = 0.0;
        if near.is_empty() {
            sources += 1.0;
        }
        if orphaned {
            sources += 1.0;
        } else {
            let heal = blobbernaut_healing(&near);
            if heal > 0.0 {
                hurt(world, naut, DamageType::Tox, -max * heal * secs);
            }
        }
        if sources > 0.0 {
            hurt(
                world,
                naut,
                DamageType::Tox,
                max * NAUT_DECAY * sources * secs,
            );
        }
    }
}

/// The share of health a second a blobbernaut heals near the structures `near`.
pub fn blobbernaut_healing(near: &[(Entity, TilePos, Kind)]) -> f32 {
    let mut heal = 0.0;
    if near.iter().any(|(_, _, k)| *k == Kind::Core) {
        heal += NAUT_HEAL_CORE;
    }
    if near.iter().any(|(_, _, k)| *k == Kind::Node) {
        heal += NAUT_HEAL_NODE;
    }
    heal
}

/// A player blobbernaut's clicks: living things that are not the blob's are slammed (the
/// strain's reagent follows, `blobbernaut_attack`), objects take 60, blobs nothing.
fn blobbernaut_clicks(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    let t = crate::tools::now(world);
    for click in clicks {
        let (mob, target) = (click.mob, click.target);
        let Some(minion) = world.get::<BlobMinion>(mob).cloned() else {
            continue;
        };
        if minion.kind != MinionKind::Blobbernaut
            || target == mob
            || world.get::<PlayerMob>(mob).map(|p| p.client) != Some(click.client)
            || world.get::<Stat>(mob) != Some(&Stat::Conscious)
            || world.get::<NextMove>(mob).is_some_and(|n| t < n.0)
        {
            continue;
        }
        if world.get::<Health>(target).is_some() {
            if super::is_ally(world, target) {
                continue;
            }
            let alive = world.get::<Stat>(target) != Some(&Stat::Dead);
            if crate::mobs::defense::melee_attack(world, mob, target)
                && alive
                && let Some(overmind) = minion.overmind
            {
                attack_living_volume(
                    world,
                    overmind,
                    target,
                    super::strains::BLOBBERNAUT_ATTACK_VOLUME,
                    false,
                );
            }
            continue;
        }
        if world.get::<Blob>(target).is_some() {
            continue;
        }
        if world.get::<crate::obj_damage::Integrity>(target).is_some() {
            crate::obj_damage::take_damage(world, target, NAUT_OBJ_DAMAGE, DamageType::Brute);
            world.entity_mut(mob).insert(NextMove(t + CLICK_CD_MELEE));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blobbernauts_heal_near_the_core_and_nodes() {
        let e = Entity::PLACEHOLDER;
        let p = TilePos::new(1, 1, 1);
        assert_eq!(blobbernaut_healing(&[]), 0.0);
        assert_eq!(blobbernaut_healing(&[(e, p, Kind::Normal)]), 0.0);
        assert_eq!(blobbernaut_healing(&[(e, p, Kind::Core)]), 0.05);
        let both = blobbernaut_healing(&[(e, p, Kind::Core), (e, p, Kind::Node)]);
        assert!((both - 0.075).abs() < 1e-6);
        assert_eq!(MINION_RUNTIME_TYPES.len(), 4);
    }
}
