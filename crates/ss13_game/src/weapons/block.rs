//! Blocking with held items and the shields, after tg `/mob/living/carbon/human/check_block`
//! (`human_defense.dm`), `/obj/item/proc/hit_reaction` (`items.dm`) and `/obj/item/shield`
//! (`code/game/objects/items/weaponry/shields.dm`).
//!
//! An item held in a hand (not clothing) blocks a melee hit with `block_chance` plus
//! `round(damage / -3)` percent (`check_block`): "X блокирует Y с помощью Z!", its
//! `block_sound` at 70. Energy weapons block only while on (`crate::weapons::energy::is_on`).
//! A shield takes the blocked brute or burn damage, through its `melee` armor, and breaks at
//! its `max_integrity` (`on_shield_block`, `atom_destruction`: its `shield_break_sound`, its
//! `shield_break_leftover`, "щит сломан!"). Hitting a bashable shield with a `/obj/item/melee`
//! (`item_interaction`) bangs it: `shield_bash_sound`, an emote, a 3 s cooldown.
//!
//! Projectiles (`/mob/living/carbon/human/bullet_act`, `PROJECTILE_ATTACK`) and thrown things
//! (`/mob/living/hitby`, `THROWN_PROJECTILE_ATTACK`) are blocked the same way through
//! [`held_block`]: the projectile's `armour_penetration` takes half off the chance, shields get
//! +30 against thrown things, the riot shield (`transparent`) lets `PASSGLASS` beams through,
//! the double-bladed sword and the energy shield leave reflectable shots to reflection
//! (double-bladed: −25 against the others), swords and staffs of [`NO_PROJECTILE_BLOCK`] block
//! no shots. A block shows the yellow `block` glow on the holder's tile.
// shortcut: no `armour_penetration` of items (melee hits pierce nothing), no leap or
// overwhelming attacks, shake of the basher, or shield repair; a shield takes shots through its
// `melee` armor (only that one is imported).

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::melee::SHIELD_ROOT;
use ss13_mapload::{DmItem, DmMelee, DmName, DmObj};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::health::DamageType;

/// `BLOCK_SOUND_VOLUME`.
const BLOCK_SOUND_VOLUME: f32 = 70.0;
/// `WEAPON_BASH_COOLDOWN`, seconds.
const WEAPON_BASH_COOLDOWN: f64 = 3.0;
/// `/obj/item/melee`: what bangs on a shield.
const MELEE_ROOT: &str = "/obj/item/melee";

/// Damage a shield took (`atom_integrity` lost) and its next bash time.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct ShieldWear {
    pub damage: f32,
    pub bash_ready_at: f64,
}

/// tg `check_block`'s `final_block_chance` of one held item: `block_chance` with
/// `round(damage / -3)` (DM `round` floors).
pub fn final_block_chance(block_chance: f32, damage: f32) -> f32 {
    block_chance + (damage / -3.0).floor()
}

/// The held item that blocks an attack of `damage` on the holder, rolled in hand order (tg
/// `get_active_handset_items`), or `None`. `on` says whether an item can block now.
pub fn roll_held_block(
    rng: &mut crate::combat::CombatRng,
    hands: [Option<Entity>; 2],
    damage: f32,
    mut chance_of: impl FnMut(Entity) -> Option<f32>,
) -> Option<Entity> {
    hands.into_iter().flatten().find(|&item| {
        chance_of(item).is_some_and(|c| c > 0.0 && rng.chance(final_block_chance(c, damage)))
    })
}

/// The block chance of a held item, `None` when it cannot block: clothing in hand
/// (`check_block` skips it), no `block_chance`, a double-bladed sword not wielded, an energy
/// blade switched off (their `hit_reaction`).
pub fn chance(
    path: &str,
    melee: &DmMelee,
    clothing: bool,
    wielded: bool,
    blade_active: Option<bool>,
) -> Option<f32> {
    // BandaStation `electrostaff/hit_reaction`: 50% while wielded.
    // shortcut: not halved without charge, no +20 against thrown things, no charge cost.
    if is_under(path, crate::security::ELECTROSTAFF) {
        return wielded.then_some(crate::security::ELECTROSTAFF_BLOCK);
    }
    if clothing || melee.block_chance <= 0.0 {
        return None;
    }
    if is_under(path, crate::weapons::energy::DUALSABER) && !wielded {
        return None;
    }
    if crate::antag::syndie_items::is_blade(path) && !blade_active.unwrap_or(false) {
        return None;
    }
    Some(melee.block_chance)
}

/// tg `attack_type` of a `check_block`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AttackType {
    /// `MELEE_ATTACK`, `UNARMED_ATTACK`.
    Melee,
    /// `THROWN_PROJECTILE_ATTACK`.
    Thrown,
    /// `PROJECTILE_ATTACK` of a projectile with these vars.
    Projectile {
        reflectable: bool,
        pass_flags: u32,
        armour_penetration: f32,
    },
}

/// `/obj/item/shield/riot` (`transparent = TRUE`).
const RIOT_SHIELD: &str = "/obj/item/shield/riot";
/// `/obj/item/shield/energy`.
const ENERGY_SHIELD: &str = "/obj/item/shield/energy";
/// The `hit_reaction`s that set `final_block_chance = 0` against `PROJECTILE_ATTACK`.
pub const NO_PROJECTILE_BLOCK: &[&str] = &[
    "/obj/item/melee/sabre",
    "/obj/item/melee/parsnip_sabre",
    "/obj/item/claymore",
    "/obj/item/nullrod/claymore",
    "/obj/item/nullrod/bostaff",
    "/obj/item/gun/magic/staff/spellblade",
    "/obj/item/cursed_katana",
    "/obj/item/chainsaw/dual",
    "/obj/item/melee/beesword",
    "/obj/item/melee/cleric_mace",
    "/obj/item/bambostaff",
    "/obj/item/melee/energy/sword/nullrod",
    "/obj/item/melee/baton/nunchaku",
];

/// The `hit_reaction` overrides of `path` against `attack`, from tg's `final_block_chance`
/// (`check_block`'s); `None` when the item does not react at all.
pub fn against(path: &str, final_chance: f32, attack: AttackType) -> Option<f32> {
    let shield = is_under(path, SHIELD_ROOT);
    let mut chance = final_chance;
    if let AttackType::Projectile {
        reflectable,
        pass_flags,
        armour_penetration,
    } = attack
    {
        // `check_block`: `block_chance - clamp((armour_penetration - item AP) / 2, 0, 100)`.
        chance -= (armour_penetration / 2.0).clamp(0.0, 100.0);
        if is_under(path, RIOT_SHIELD) && pass_flags & crate::projectile::PASSGLASS != 0 {
            return None;
        }
        if NO_PROJECTILE_BLOCK.iter().any(|p| is_under(path, p)) {
            chance = 0.0;
        }
        if is_under(path, crate::weapons::energy::DUALSABER) {
            chance = if reflectable { 0.0 } else { chance - 25.0 };
        }
        if is_under(path, ENERGY_SHIELD) && reflectable {
            chance = 0.0;
        }
    }
    if shield && attack == AttackType::Thrown {
        chance += 30.0;
    }
    if shield {
        chance = chance.clamp(0.0, 100.0);
    }
    Some(chance)
}

/// tg human `check_block` over the held items against a non-melee attack of `damage`: the item
/// that blocks it (its `hit_reaction` effects not yet applied: see [`on_block`]).
pub fn held_block(
    world: &mut World,
    owner: Entity,
    damage: f32,
    attack: AttackType,
) -> Option<Entity> {
    let hands = world.get::<crate::inventory::Inventory>(owner)?.hands;
    let read: &World = world;
    let chances: Vec<(Entity, f32)> = hands
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let path = &read.get::<DmItem>(item)?.path;
            let base = chance(
                path,
                read.get::<DmMelee>(item)?,
                read.get::<ss13_mapload::DmClothing>(item).is_some(),
                crate::weapons::two_handed::is_wielded(read, item),
                read.get::<crate::antag::syndie_items::EnergyBlade>(item)
                    .map(|b| b.active),
            )?;
            let c = against(path, final_block_chance(base, damage), attack)?;
            Some((item, c))
        })
        .collect();
    let mut rng = world.get_resource_or_init::<crate::combat::CombatRng>();
    chances
        .into_iter()
        .find(|&(_, c)| c > 0.0 && rng.chance(c))
        .map(|(item, _)| item)
}

/// `/obj/effect/temp_visual/block` in `COLOR_YELLOW`: 0.67 s, `pixel_x = rand(-12, 12)`,
/// `pixel_y = rand(-9, 0)`.
fn block_glow(world: &mut World, pos: TilePos) {
    use crate::effects::temp_visual::{Visual, spawn};
    let (x, y) = {
        let mut rng = world.get_resource_or_init::<crate::combat::CombatRng>();
        (rng.range(0, 24) as i16 - 12, rng.range(0, 9) as i16 - 9)
    };
    let mut v = Visual::new("icons/effects/effects.dmi", "block", 0.67);
    v.dir = Some(ss13_shared::Dir::South);
    v.color = Some([0xFF, 0xFF, 0x00, 0xFF]);
    v.offset = [x, y];
    spawn(world, pos, v);
}

/// tg `hit_reaction` succeeding: the message, the sound, then the shield's damage.
pub fn on_block(
    world: &mut World,
    owner: Entity,
    item: Entity,
    attack_text: &str,
    damage: f32,
    kind: DamageType,
) {
    let name = |world: &World, e: Entity| {
        world
            .get::<DmName>(e)
            .map(|n| n.name.clone())
            .or_else(|| world.get::<Name>(e).map(|n| n.as_str().to_owned()))
            .unwrap_or_else(|| "Кто-то".to_owned())
    };
    let (who, what) = (name(world, owner), name(world, item));
    let text = format!("{who} блокирует {attack_text} с помощью {what}!");
    crate::chem::visible(world, owner, &text, &text, Span::Danger);
    let sound = world
        .get::<DmMelee>(item)
        .map(|m| m.block_sound.clone())
        .unwrap_or_default();
    if let Some(pos) = crate::explosions::turf_of(world, owner) {
        block_glow(world, pos);
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, sound, BLOCK_SOUND_VOLUME, true),
        );
    }
    shield_block(world, owner, item, damage, kind);
}

/// `/obj/item/shield/on_shield_block`: brute and burn wear the shield down.
pub fn shield_block(
    world: &mut World,
    owner: Entity,
    shield: Entity,
    damage: f32,
    kind: DamageType,
) {
    let Some(melee) = world.get::<DmMelee>(shield).cloned() else {
        return;
    };
    if !is_under(&melee.path, SHIELD_ROOT)
        || !melee.breakable_by_damage
        || !matches!(kind, DamageType::Brute | DamageType::Burn)
    {
        return;
    }
    let taken = match world.get::<DmObj>(shield) {
        Some(obj) => crate::obj_damage::armor_damage(obj, damage, kind),
        None => damage,
    };
    if taken <= 0.0 {
        return;
    }
    let mut wear = world.get::<ShieldWear>(shield).copied().unwrap_or_default();
    wear.damage += taken;
    world.entity_mut(shield).insert(wear);
    if melee.max_integrity > 0.0 && wear.damage >= melee.max_integrity {
        break_shield(world, owner, shield, &melee);
    }
}

/// `/obj/item/shield/atom_destruction`.
fn break_shield(world: &mut World, owner: Entity, shield: Entity, melee: &DmMelee) {
    let Some(pos) = crate::explosions::turf_of(world, shield) else {
        return;
    };
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, melee.shield_break_sound.clone(), 50.0, false),
    );
    if !melee.shield_break_leftover.is_empty() {
        crate::construction::spawn_obj(world, &melee.shield_break_leftover, pos);
    }
    if let Some(client) = world
        .get::<crate::server::PlayerMob>(owner)
        .map(|p| p.client)
    {
        crate::tools::tell(world, client, Span::Warning, "щит сломан!");
    }
    world.despawn(shield);
    crate::inventory::refresh_look(world, owner);
}

/// `/obj/item/shield/item_interaction` with a melee weapon: the bash. True when handled.
pub fn bash(world: &mut World, mob: Entity, tool: Entity, shield: Entity) -> bool {
    let Some(melee) = world.get::<DmMelee>(shield).cloned() else {
        return false;
    };
    let is_melee = world
        .get::<DmItem>(tool)
        .is_some_and(|d| is_under(&d.path, MELEE_ROOT));
    if !is_under(&melee.path, SHIELD_ROOT) || !melee.is_bashable || !is_melee {
        return false;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut wear = world.get::<ShieldWear>(shield).copied().unwrap_or_default();
    if now < wear.bash_ready_at {
        return true;
    }
    wear.bash_ready_at = now + WEAPON_BASH_COOLDOWN;
    world.entity_mut(shield).insert(wear);
    let at = crate::explosions::turf_of(world, shield).unwrap_or(TilePos::new(0, 0, 0));
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(at, melee.shield_bash_sound.clone(), 50.0, true),
    );
    let user = world
        .get::<Name>(mob)
        .map_or_else(|| "Кто-то".to_owned(), |n| n.as_str().to_owned());
    let tool_name = world
        .get::<DmName>(tool)
        .map_or_else(|| "предметом".to_owned(), |n| n.name.clone());
    let shield_name = world
        .get::<DmName>(shield)
        .map_or_else(|| "щиту".to_owned(), |n| n.name.clone());
    // `manual_emote("стучит [tool] по [shield]!")`.
    let text = format!("{user} стучит {tool_name} по {shield_name}!");
    crate::chem::visible(world, mob, &text, &text, Span::Emote);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::CombatRng;

    #[test]
    fn block_chance_drops_with_damage() {
        assert_eq!(final_block_chance(50.0, 0.0), 50.0);
        assert_eq!(final_block_chance(50.0, 24.0), 42.0);
        // DM `round(-25 / 3)` floors to -9.
        assert_eq!(final_block_chance(50.0, 25.0), 41.0);
    }

    /// Energy weapons block only while out; clothing in hand never.
    #[test]
    fn energy_weapons_block_only_when_on() {
        let melee = |c: f32| DmMelee {
            block_chance: c,
            ..default()
        };
        let sword = "/obj/item/melee/energy/sword/saber/red";
        assert_eq!(chance(sword, &melee(50.0), false, false, Some(false)), None);
        assert_eq!(chance(sword, &melee(50.0), false, false, None), None);
        assert_eq!(
            chance(sword, &melee(50.0), false, false, Some(true)),
            Some(50.0)
        );
        let saber = "/obj/item/dualsaber/red";
        assert_eq!(chance(saber, &melee(75.0), false, false, None), None);
        assert_eq!(chance(saber, &melee(75.0), false, true, None), Some(75.0));
        let riot = "/obj/item/shield/riot";
        assert_eq!(chance(riot, &melee(50.0), false, false, None), Some(50.0));
        assert_eq!(chance(riot, &melee(50.0), true, false, None), None);
        assert_eq!(
            chance("/obj/item/pen", &melee(0.0), false, false, None),
            None
        );
    }

    /// The `hit_reaction` overrides against shots and thrown things.
    #[test]
    fn shots_and_throws_follow_hit_reaction() {
        use crate::projectile::PASSGLASS;
        let shot = |reflectable, pass_flags, armour_penetration| AttackType::Projectile {
            reflectable,
            pass_flags,
            armour_penetration,
        };
        let riot = "/obj/item/shield/riot";
        // The riot shield lets lasers through, stops bullets (AP halves off), +30 vs throws.
        assert_eq!(against(riot, 50.0, shot(true, PASSGLASS, 0.0)), None);
        assert_eq!(against(riot, 50.0, shot(false, 0, 20.0)), Some(40.0));
        assert_eq!(against(riot, 80.0, AttackType::Thrown), Some(100.0));
        assert_eq!(against(riot, 50.0, AttackType::Melee), Some(50.0));
        // The double-bladed sword leaves lasers to reflection, bullets at -25.
        let saber = "/obj/item/dualsaber/green";
        assert_eq!(against(saber, 75.0, shot(true, PASSGLASS, 0.0)), Some(0.0));
        assert_eq!(against(saber, 75.0, shot(false, 0, 0.0)), Some(50.0));
        // Sabres do not block shots, only blows; a plain item keeps its chance.
        assert_eq!(
            against("/obj/item/melee/sabre", 50.0, shot(false, 0, 0.0)),
            Some(0.0)
        );
        assert_eq!(
            against(
                "/obj/item/melee/energy/sword/saber",
                50.0,
                shot(false, 0, 0.0)
            ),
            Some(50.0)
        );
        assert_eq!(
            against(
                "/obj/item/melee/energy/sword/saber",
                50.0,
                AttackType::Thrown
            ),
            Some(50.0)
        );
    }

    /// A 50% shield in hand blocks about half of 0-damage hits; an item that cannot block
    /// never does.
    #[test]
    fn held_shield_blocks_by_its_chance() {
        let mut world = World::new();
        let shield = world.spawn_empty().id();
        let other = world.spawn_empty().id();
        let mut rng = CombatRng(11);
        let mut blocks = 0;
        for _ in 0..10_000 {
            let blocked = roll_held_block(&mut rng, [Some(other), Some(shield)], 0.0, |e| {
                (e == shield).then_some(50.0)
            });
            if let Some(e) = blocked {
                assert_eq!(e, shield);
                blocks += 1;
            }
        }
        assert!((4500..5500).contains(&blocks), "{blocks} blocks");
        assert_eq!(
            roll_held_block(&mut rng, [Some(other), None], 0.0, |_| None),
            None
        );
    }

    /// The buckler (55 integrity) breaks on the hit that wears it out; stamina does not wear.
    #[test]
    fn shields_wear_and_break() {
        let mut world = World::new();
        world.init_resource::<Messages<crate::sound::PlaySound>>();
        let owner = world.spawn(TilePos::new(1, 1, 1)).id();
        let buckler = world
            .spawn((
                DmMelee {
                    path: "/obj/item/shield/buckler".into(),
                    block_chance: 30.0,
                    breakable_by_damage: true,
                    max_integrity: 55.0,
                    ..default()
                },
                TilePos::new(1, 1, 1),
            ))
            .id();
        shield_block(&mut world, owner, buckler, 30.0, DamageType::Stamina);
        assert!(world.get::<ShieldWear>(buckler).is_none());
        shield_block(&mut world, owner, buckler, 30.0, DamageType::Brute);
        assert_eq!(world.get::<ShieldWear>(buckler).unwrap().damage, 30.0);
        shield_block(&mut world, owner, buckler, 30.0, DamageType::Burn);
        assert!(world.get_entity(buckler).is_err());
    }
}
