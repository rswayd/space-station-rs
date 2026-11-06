//! Proto-kinetic accelerator modkits (server), after tg `/obj/item/borg/upgrade/modkit`
//! (`code/modules/projectiles/guns/energy/kinetic_accelerator.dm`).
//!
//! - A modkit used on an accelerator (or the accelerator on a modkit) goes inside it
//!   (`install`) when it fits: its `cost` within what is left of `max_mod_capacity` (100, the
//!   cyborg's 90), at most `maximum_of_type` of its kind and none of its `denied_type`; minebot
//!   kits only in a minebot's gun and the other way round. A crowbar pulls every removable kit
//!   out onto the floor.
//! - On a shot (`modify_projectile`): range and damage kits add their `modifier`; cooldown kits
//!   add theirs to the gun's `recharge_time` (16 ds, never below 0); "decrease pressure
//!   penalty" kits multiply the 0.25 pressure penalty by 2; the human passthrough kit lets shots
//!   pass through humans (the minebot one through minebots).
//! - On a hit: the lifesteal crystal heals the firer 2.5 (brute, then burn, then oxygen) when it
//!   strikes a living target; the rapid repeater empties the gun and reloads it in a quarter of
//!   its recharge time when it strikes a living target or rock; mining explosion kits drill the
//!   rock around the struck tile.
// shortcut: no right-click removal of one kit (the crowbar takes all), no cyborg modkit
// upgrades (`crate::silicon::borg_upgrades` refuses them), no visuals (tracer bolts, chassis
// colours, kinetic blast effects), no resonator blasts or death syphon marks, and the offensive
// explosion kits hurt nothing (they only hit mining fauna, which the station has none of).
// tg recomputes `recharge_time` from the kits of the installed kit's exact type only
// (`istype(modkit_upgrade, src)` on an instance); every cooldown kit counts here, as intended.

use bevy::prelude::*;
use ss13_mapload::{DmItem, DmModkit, DmName, ProjectileData};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::examine::Line;
use crate::health::{DamageType, Health, Stat};
use crate::inventory::CarriedBy;
use crate::obj_damage::is_under;
use crate::projectile::Projectile;
use crate::sound::{PlaySound, playsound};
use crate::storage::StoredIn;
use crate::tools::{ToolKind, tell, tool_of};

pub const PKA: &str = "/obj/item/gun/energy/recharge/kinetic_accelerator";
const PKA_CYBORG: &str = "/obj/item/gun/energy/recharge/kinetic_accelerator/cyborg";
const PKA_MINEBOT: &str = "/obj/item/gun/energy/recharge/kinetic_accelerator/minebot";
const KIT_RANGE: &str = "/obj/item/borg/upgrade/modkit/range";
const KIT_DAMAGE: &str = "/obj/item/borg/upgrade/modkit/damage";
const KIT_COOLDOWN: &str = "/obj/item/borg/upgrade/modkit/cooldown";
const KIT_AOE: &str = "/obj/item/borg/upgrade/modkit/cooldown/aoe";
const KIT_REPEATER: &str = "/obj/item/borg/upgrade/modkit/cooldown/repeater";
const KIT_LIFESTEAL: &str = "/obj/item/borg/upgrade/modkit/lifesteal";
const KIT_INDOORS: &str = "/obj/item/borg/upgrade/modkit/indoors";
const KIT_HUMAN_PASS: &str = "/obj/item/borg/upgrade/modkit/human_passthrough";
const KIT_MINEBOT_PASS: &str = "/obj/item/borg/upgrade/modkit/minebot_passthrough";
const MINEBOT: &str = "/mob/living/basic/mining_drone";
/// `/obj/projectile/kinetic::pressure_decrease`.
pub const PRESSURE_DECREASE: f32 = 0.25;

/// On a modkit inside an accelerator (tg `modkits`, in its contents). Despawning the gun
/// despawns its kits.
#[derive(Component, Debug)]
#[relationship(relationship_target = Modkits)]
pub struct ModkitIn(pub Entity);

/// An accelerator's kits, in installing order.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = ModkitIn, linked_spawn)]
pub struct Modkits(Vec<Entity>);

impl Modkits {
    pub fn kits(&self) -> &[Entity] {
        &self.0
    }
}

/// An accelerator's `recharge_time` with its cooldown kits, deciseconds
/// (`crate::mining::rocks` reloads by it).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct RechargeTime(pub f64);

fn path(world: &World, e: Entity) -> Option<String> {
    crate::construction::path_of(world, e)
}

pub fn is_pka(world: &World, e: Entity) -> bool {
    world
        .get::<DmItem>(e)
        .is_some_and(|i| is_under(&i.path, PKA))
}

fn is_kit(world: &World, e: Entity) -> bool {
    world.get::<DmModkit>(e).is_some()
}

/// The kits of `gun` with their paths.
fn kits(world: &World, gun: Entity) -> Vec<(Entity, String)> {
    world
        .get::<Modkits>(gun)
        .map(|m| m.kits().to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|k| Some((k, world.get::<DmItem>(k)?.path.clone())))
        .collect()
}

fn kits_under(world: &World, gun: Entity, root: &str) -> Vec<Entity> {
    if !is_pka(world, gun) {
        return Vec::new();
    }
    kits(world, gun)
        .into_iter()
        .filter(|(_, p)| is_under(p, root))
        .map(|(k, _)| k)
        .collect()
}

/// `max_mod_capacity`.
fn max_capacity(gun_path: &str) -> f32 {
    if is_under(gun_path, PKA_CYBORG) {
        90.0
    } else {
        100.0
    }
}

/// `get_remaining_mod_capacity`.
pub fn remaining_capacity(world: &World, gun: Entity) -> f32 {
    let gun_path = path(world, gun).unwrap_or_default();
    let used: f32 = kits(world, gun)
        .iter()
        .filter_map(|(k, _)| world.get::<DmModkit>(*k))
        .map(|m| m.cost)
        .sum();
    max_capacity(&gun_path) - used
}

/// `initial(recharge_time)` of a self-recharging gun, deciseconds.
pub fn base_recharge(gun_path: &str) -> f64 {
    if is_under(gun_path, "/obj/item/gun/energy/recharge/ebow") || is_under(gun_path, PKA_MINEBOT) {
        20.0
    } else {
        16.0
    }
}

/// `cooldown/get_recharge_time`: the base plus every cooldown kit's `modifier`.
fn refresh(world: &mut World, gun: Entity) {
    let Some(gun_path) = path(world, gun) else {
        return;
    };
    let extra: f64 = kits_under(world, gun, KIT_COOLDOWN)
        .into_iter()
        .filter_map(|k| world.get::<DmModkit>(k))
        .map(|m| f64::from(m.modifier))
        .sum();
    let time = (base_recharge(&gun_path) + extra).max(0.0);
    world.entity_mut(gun).insert(RechargeTime(time));
}

fn tell_mob(world: &mut World, mob: Option<(Entity, Entity)>, span: Span, text: impl Into<String>) {
    if let Some((client, _)) = mob {
        tell(world, client, span, text);
    }
}

/// `modkit/install`. `user` is `(client, mob)`. True when the kit went in.
pub fn install(
    world: &mut World,
    kit: Entity,
    gun: Entity,
    user: Option<(Entity, Entity)>,
) -> bool {
    let (Some(dm), Some(kit_path), Some(gun_path)) = (
        world.get::<DmModkit>(kit).cloned(),
        path(world, kit),
        path(world, gun),
    ) else {
        return false;
    };
    if !is_under(&gun_path, PKA) || world.get::<ModkitIn>(kit).is_some() {
        return false;
    }
    let minebot = is_under(&gun_path, PKA_MINEBOT);
    if dm.minebot_upgrade {
        if dm.minebot_exclusive && !minebot {
            tell_mob(
                world,
                user,
                Span::Notice,
                "Этот модуль поддерживается только дронами.",
            );
            return false;
        }
    } else if minebot {
        tell_mob(
            world,
            user,
            Span::Notice,
            "Этот модуль не поддерживается модулями.",
        );
        return false;
    }
    let limit = if !dm.denied_type.is_empty() {
        Some(dm.denied_type.clone())
    } else {
        (dm.maximum_of_type > 0).then(|| kit_path.clone())
    };
    if let Some(limit) = limit {
        let count = kits(world, gun)
            .iter()
            .filter(|(_, p)| is_under(p, &limit))
            .count() as u32;
        let denied = if dm.maximum_of_type > 0 {
            count >= dm.maximum_of_type
        } else {
            count > 0
        };
        if denied {
            tell_mob(
                world,
                user,
                Span::Notice,
                "The modkit you're trying to install would conflict with an already installed modkit. Remove existing modkits first.",
            );
            return false;
        }
    }
    let left = remaining_capacity(world, gun);
    if left < dm.cost {
        let left_text = format_percent(left);
        tell_mob(
            world,
            user,
            Span::Notice,
            format!(
                "Не хватает места({left_text}% остал{}, {}% нужно) для установки модуля. Используйте ломик или ПКМ пустой рукой для снятия установленных модулей.",
                declension(left),
                format_percent(dm.cost)
            ),
        );
        return false;
    }
    world
        .entity_mut(kit)
        .remove::<(CarriedBy, TilePos, StoredIn)>()
        .insert(ModkitIn(gun));
    if user.is_some() {
        tell_mob(world, user, Span::Notice, "Вы устанавливаете модуль.");
        if let Some(pos) = crate::explosions::turf_of(world, gun) {
            playsound(
                world,
                PlaySound::new(pos, "sound/items/tools/screwdriver.ogg", 100.0, true),
            );
        }
    }
    refresh(world, gun);
    true
}

/// `Exited` -> `uninstall`: the kit out of the gun onto `pos`.
fn uninstall(world: &mut World, kit: Entity, gun: Entity, pos: TilePos) {
    world.entity_mut(kit).remove::<ModkitIn>();
    crate::storage::take_out(world, kit, pos);
    refresh(world, gun);
}

/// `kinetic_accelerator/crowbar_act`: every removable kit drops.
fn crowbar_act(world: &mut World, client: Entity, gun: Entity) {
    let all = kits(world, gun);
    if all.is_empty() {
        tell(
            world,
            client,
            Span::Notice,
            "Не имеются установленные модули.",
        );
        return;
    }
    tell(world, client, Span::Notice, "Вы вытаскиваете все модули.");
    let Some(pos) = crate::explosions::turf_of(world, gun) else {
        return;
    };
    // `play_tool_sound(src, 100)`: the crowbar's `usesound`.
    playsound(
        world,
        PlaySound::new(pos, "sound/items/tools/crowbar.ogg", 100.0, true),
    );
    for (kit, _) in all {
        if world.get::<DmModkit>(kit).is_some_and(|m| m.removable) {
            uninstall(world, kit, gun, pos);
        }
    }
}

/// A modkit on an accelerator, the accelerator on a modkit, a crowbar on an accelerator. True
/// when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let user = Some((client, mob));
    if is_kit(world, held) && is_pka(world, target) {
        install(world, held, target, user);
        return true;
    }
    if is_pka(world, held) && is_kit(world, target) {
        install(world, target, held, user);
        return true;
    }
    let combat = world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0);
    if !combat
        && is_pka(world, target)
        && tool_of(world, held).map(|t| t.0) == Some(ToolKind::Crowbar)
    {
        crowbar_act(world, client, target);
        return true;
    }
    false
}

/// `modify_projectile` of every kit in the gun that fired.
pub fn modify_projectile(world: &World, gun: Entity, data: &mut ProjectileData) {
    if !is_pka(world, gun) {
        return;
    }
    for (kit, kit_path) in kits(world, gun) {
        let Some(m) = world.get::<DmModkit>(kit) else {
            continue;
        };
        if is_under(&kit_path, KIT_RANGE) {
            data.range += m.modifier;
        } else if is_under(&kit_path, KIT_DAMAGE) {
            data.damage += m.modifier;
        }
    }
}

/// `pressure_decrease` of a kinetic shot from `gun`: the "decrease pressure penalty" kits
/// multiply it.
pub fn pressure_decrease(world: &World, gun: Option<Entity>) -> f32 {
    let Some(gun) = gun else {
        return PRESSURE_DECREASE;
    };
    kits_under(world, gun, KIT_INDOORS)
        .into_iter()
        .filter_map(|k| world.get::<DmModkit>(k))
        .fold(PRESSURE_DECREASE, |p, m| p * m.modifier)
}

/// `kinetic/prehit_pierce`: `is_type_in_typecache(target, kinetic_gun?.ignored_mob_types)`.
pub fn phases(world: &World, p: &Projectile, target: Entity) -> bool {
    let Some(gun) = p.fired_from else {
        return false;
    };
    if world.get::<Modkits>(gun).is_none() {
        return false;
    }
    let human = world.get::<crate::human::HumanLook>(target).is_some();
    let minebot = world
        .get::<crate::mobs::BasicMob>(target)
        .is_some_and(|m| is_under(&m.def.path, MINEBOT));
    (human && !kits_under(world, gun, KIT_HUMAN_PASS).is_empty())
        || (minebot && !kits_under(world, gun, KIT_MINEBOT_PASS).is_empty())
}

fn living(world: &World, e: Entity) -> bool {
    world.get::<Health>(e).is_some() && world.get::<Stat>(e).is_some_and(|s| *s != Stat::Dead)
}

/// `projectile_prehit` of the kits: the lifesteal crystal's `heal_ordered_damage(2.5, BRUTE,
/// BURN, OXY)` on the firer for a living target.
pub fn prehit(world: &mut World, p: &Projectile, target: Entity) {
    let Some(gun) = p.fired_from else {
        return;
    };
    if !living(world, target) {
        return;
    }
    let heals: Vec<f32> = kits_under(world, gun, KIT_LIFESTEAL)
        .into_iter()
        .filter_map(|k| world.get::<DmModkit>(k).map(|m| m.modifier))
        .collect();
    let Some(mut health) = world.get::<Health>(p.firer).cloned() else {
        return;
    };
    for amount in heals {
        let mut left = amount;
        for kind in [DamageType::Brute, DamageType::Burn, DamageType::Oxy] {
            let had = match kind {
                DamageType::Brute => health.brute,
                DamageType::Burn => health.burn,
                _ => health.oxy,
            };
            let healed = had.min(left);
            if healed > 0.0 {
                health.adjust(kind, -healed);
                left -= healed;
            }
        }
    }
    let mut stat = world.get::<Stat>(p.firer).copied().unwrap_or_default();
    let changed = crate::health::update_stat(&health, &mut stat).is_some();
    let mut firer = world.entity_mut(p.firer);
    firer.insert(health);
    if changed {
        firer.insert(stat);
    }
}

/// `projectile_strike_predamage` and `projectile_strike` of the kits on `target` at
/// `target_turf`: the repeater's quick reload, the mining explosions.
pub fn strike(
    world: &mut World,
    p: &Projectile,
    target: Option<Entity>,
    target_turf: TilePos,
    mineral: bool,
) {
    let Some(gun) = p.fired_from else {
        return;
    };
    if world.get::<Modkits>(gun).is_none() {
        return;
    }
    let repeat = mineral || target.is_some_and(|t| living(world, t));
    if repeat && !kits_under(world, gun, KIT_REPEATER).is_empty() {
        // `KA.cell.use(KA.cell.charge)`, `attempt_reload(recharge_time * 0.25)`.
        if let Some(mut g) = world.get_mut::<crate::guns::Gun>(gun) {
            g.charge = 0.0;
        }
        let time = recharge_time(world, gun) * 0.25;
        super::rocks::attempt_reload(world, gun, time, true);
    }
    let drills = kits_under(world, gun, KIT_AOE)
        .into_iter()
        .any(|k| world.get::<DmModkit>(k).is_some_and(|m| m.turf_aoe));
    if drills {
        for dx in -1..=1 {
            for dy in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let pos = TilePos::new(target_turf.x + dx, target_turf.y + dy, target_turf.z);
                let rock = crate::construction::turf_at(world, pos)
                    .filter(|&t| world.get::<ss13_mapload::DmMineral>(t).is_some());
                if let Some(rock) = rock {
                    // `gets_drilled(firer, 0.1)`.
                    crate::skills::mining_experience(world, p.firer, rock, 0.1);
                    super::rocks::gets_drilled(world, pos, Some(p.firer));
                }
            }
        }
    }
}

/// The gun's `recharge_time`, deciseconds.
pub fn recharge_time(world: &World, gun: Entity) -> f64 {
    world.get::<RechargeTime>(gun).map_or_else(
        || base_recharge(&path(world, gun).unwrap_or_default()),
        |r| r.0,
    )
}

/// `declension_ru(n, "ся", "ось", "ось")`.
fn declension(n: f32) -> &'static str {
    let n = n.abs().floor() as i64;
    if n % 10 == 1 && n % 100 != 11 {
        "ся"
    } else {
        "ось"
    }
}

fn format_percent(n: f32) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// The accelerator's and the modkit's `examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<Line>) {
    if let Some(m) = world.get::<DmModkit>(target) {
        lines.push((
            Span::Notice,
            format!("Занимает {}% вместимости модулей.", format_percent(m.cost)),
        ));
        return;
    }
    if !is_pka(world, target) {
        return;
    }
    let left = remaining_capacity(world, target);
    lines.push((
        Span::Plain,
        format!(
            "Остал{} {}% вместимости для модулей.",
            declension(left),
            format_percent(left)
        ),
    ));
    lines.push((
        Span::Info,
        "Вы можете использовать ломик для снятия всех модулей.".into(),
    ));
    for (kit, _) in kits(world, target) {
        let name = world
            .get::<DmName>(kit)
            .map(|n| crate::vending::plain_name(&n.name).to_owned())
            .unwrap_or_default();
        let cost = world.get::<DmModkit>(kit).map_or(0.0, |m| m.cost);
        lines.push((
            Span::Notice,
            format!(
                "Имеет установленный модуль '{name}', использующий {}% вместимости.",
                format_percent(cost)
            ),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declension_and_capacity() {
        assert_eq!(declension(1.0), "ся");
        assert_eq!(declension(21.0), "ся");
        assert_eq!(declension(11.0), "ось");
        assert_eq!(declension(40.0), "ось");
        assert_eq!(max_capacity(PKA_CYBORG), 90.0);
        assert_eq!(max_capacity(PKA), 100.0);
        assert_eq!(base_recharge(PKA_MINEBOT), 20.0);
        assert_eq!(base_recharge(PKA), 16.0);
        assert_eq!(format_percent(25.0), "25");
    }
}
