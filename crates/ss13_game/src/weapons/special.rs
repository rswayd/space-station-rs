//! Special melee weapons (server): the power-fist (`weaponry/melee/powerfist.dm`) and the
//! supermatter sword (`weaponry/melee/supermatter_sword.dm`).
//!
//! - **Power-fist**: an internals tank (more than 3 L) clicked onto it is hooked up, a
//!   screwdriver takes it back out, a wrench cycles the piston strength (low, medium, high).
//!   A hit on a living mob draws `3 × strength` moles from the tank: none — a dull punch of
//!   `force / 5`; too few — a weak hiss and `force / 2`; enough — `force × strength` brute and
//!   the target flies `5 × strength` tiles away. The gas goes into the air; the next click comes
//!   after `CLICK_CD_MELEE × 0.15`. Without a tank or by a pacifist it does nothing.
//! - **Supermatter sword**: whatever it hits is consumed: a mob turns to dust, an object is
//!   deleted, a turf is scraped away, each with a radiation pulse; hitting yourself drops it.
// shortcut: the sword is not balanced on its hilt (it does not eat its tile when dropped), does
// not eat bullets, thrown hits or explosions; the power-fist's gas is merged on the target's
// turf and its flight is instant (`crate::martial::fling`). His Grace and the soulscythe are not
// ported. Add with item processing, thrown impacts and possessed items.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::health::{DamageType, Health};
use crate::tools::{ToolKind, tell};

pub const POWERFIST: &str = "/obj/item/melee/powerfist";
pub const SM_SWORD: &str = "/obj/item/melee/supermatter_sword";

const FORCE: f32 = 20.0;
const GAS_PER_FIST: f64 = 3.0;
const CLICK_DELAY: f64 = 0.15;

/// A power-fist's tank and `fist_pressure_setting` (1-3).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerFist {
    pub tank: Option<Entity>,
    pub pressure: u8,
}

impl Default for PowerFist {
    fn default() -> Self {
        Self {
            tank: None,
            pressure: 1,
        }
    }
}

fn is(world: &World, item: Entity, root: &str) -> bool {
    world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, root))
}

fn pressure_name(p: u8) -> &'static str {
    match p {
        1 => "низкую",
        2 => "среднюю",
        _ => "высокую",
    }
}

/// `wrench_act`, `screwdriver_act`, `item_interaction` of a power-fist: true when taken.
pub fn powerfist_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    fist: Entity,
) -> bool {
    if held == fist || !is(world, fist, POWERFIST) {
        return false;
    }
    let mut state = world.get::<PowerFist>(fist).copied().unwrap_or_default();
    match crate::tools::tool_of(world, held).map(|t| t.0) {
        Some(ToolKind::Wrench) => {
            state.pressure = if state.pressure >= 3 {
                1
            } else {
                state.pressure + 1
            };
            world.entity_mut(fist).insert(state);
            crate::martial::play(world, fist, "sound/items/tools/ratchet.ogg", 50.0);
            crate::balloon::balloon_alert(
                world,
                mob,
                fist,
                &format!("сила поршня: {}", pressure_name(state.pressure)),
            );
            return true;
        }
        Some(ToolKind::Screwdriver) => {
            let Some(tank) = state.tank.take() else {
                crate::balloon::balloon_alert(world, mob, fist, "баллона нет");
                return true;
            };
            world.entity_mut(fist).insert(state);
            if let Some(inventory) = world.get::<crate::inventory::Inventory>(mob).cloned() {
                crate::storage::put_in_hands(world, mob, &inventory, tank);
            }
            tell(world, client, Span::Notice, "Вы отсоединяете баллон.");
            return true;
        }
        _ => {}
    }
    let Some(volume) = world
        .get::<crate::internals::TankAir>(held)
        .map(|t| t.volume)
    else {
        return false;
    };
    if state.tank.is_some() {
        tell(
            world,
            client,
            Span::Notice,
            "Баллон уже установлен, сначала снимите его отвёрткой.",
        );
        return true;
    }
    if volume <= 3.0 {
        tell(world, client, Span::Warning, "Этот баллон слишком мал.");
        return true;
    }
    crate::storage::put_inside(world, held, fist);
    state.tank = Some(held);
    world.entity_mut(fist).insert(state);
    tell(world, client, Span::Notice, "Вы подключаете баллон.");
    true
}

/// `/obj/item/melee/powerfist/attack` and the supermatter sword's `pre_attack` on a living
/// mob: true when the hit is theirs (melee does not follow).
pub fn attack(
    world: &mut World,
    client: Entity,
    user: Entity,
    item: Entity,
    target: Entity,
) -> bool {
    if is(world, item, SM_SWORD) {
        sm_consume(world, user, item, target);
        return true;
    }
    if !is(world, item, POWERFIST) {
        return false;
    }
    let state = world.get::<PowerFist>(item).copied().unwrap_or_default();
    let Some(tank) = state.tank.filter(|&t| world.get_entity(t).is_ok()) else {
        tell(
            world,
            client,
            Span::Warning,
            "Силовой кулак не работает без источника газа!",
        );
        return true;
    };
    if world
        .get::<crate::medical::traumas::Pacifist>(user)
        .is_some()
    {
        tell(
            world,
            client,
            Span::Warning,
            "Вы не хотите вредить другим живым существам!",
        );
        return true;
    }
    let Some(at) = crate::explosions::turf_of(world, target) else {
        return true;
    };
    let wanted = GAS_PER_FIST * f64::from(state.pressure);
    let used = world
        .get_mut::<crate::internals::TankAir>(tank)
        .and_then(|mut t| t.air.remove(wanted));
    let (un, tn) = (
        crate::martial::name(world, user),
        crate::martial::name(world, target),
    );
    let Some(used) = used else {
        tell(world, client, Span::Warning, "Баллон силового кулака пуст!");
        crate::martial::hurt(
            world,
            target,
            None,
            DamageType::Brute,
            FORCE / 5.0,
            Some(0.0),
        );
        crate::martial::play(world, user, "sound/items/weapons/punch1.ogg", 50.0);
        crate::martial::message(
            world,
            user,
            target,
            crate::martial::NEAR,
            (
                Span::Danger,
                format!("Силовой кулак {un} глухо стукает, ударяя {tn}!"),
            ),
            format!("{un} бьёт вас!"),
            None,
        );
        return true;
    };
    let merge = |world: &mut World, gas: &ss13_atmos::GasMixture| {
        if let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>() {
            atmos.merge_air(at, gas);
        }
    };
    if (used.total_moles() - wanted).abs() > 1e-4 {
        merge(world, &used);
        tell(
            world,
            client,
            Span::Warning,
            "Поршень силового кулака слабо шипит, ему нужно больше газа!",
        );
        crate::martial::play(world, user, "sound/items/weapons/punch4.ogg", 50.0);
        crate::martial::hurt(
            world,
            target,
            None,
            DamageType::Brute,
            FORCE / 2.0,
            Some(0.0),
        );
        crate::martial::message(
            world,
            user,
            target,
            crate::martial::NEAR,
            (
                Span::Danger,
                format!("Силовой кулак {un} слабо шипит, ударяя {tn}!"),
            ),
            format!("Удар {un} обрушивается на вас с силой!"),
            None,
        );
        return true;
    }
    crate::martial::message(
        world,
        user,
        target,
        crate::martial::NEAR,
        (
            Span::Danger,
            format!("Силовой кулак {un} громко шипит, ударяя {tn}!"),
        ),
        format!("Вы кричите от боли, когда удар {un} отбрасывает вас назад!"),
        None,
    );
    let pressure = f32::from(state.pressure);
    crate::martial::hurt(
        world,
        target,
        None,
        DamageType::Brute,
        FORCE * pressure,
        None,
    );
    crate::martial::play(world, item, "sound/items/weapons/resonator_blast.ogg", 50.0);
    crate::martial::play(world, item, "sound/items/weapons/genhit2.ogg", 50.0);
    if world.get_entity(target).is_ok()
        && let Some(dir) = crate::martial::dir_to(world, user, target)
    {
        crate::martial::fling(
            world,
            target,
            dir,
            5 * u32::from(state.pressure),
            0.5 + pressure / 2.0,
        );
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    world.entity_mut(user).insert(crate::combat::NextMove(
        now + crate::combat::CLICK_CD_MELEE * CLICK_DELAY,
    ));
    merge(world, &used);
    true
}

/// The supermatter sword's `pre_attack` on an object or a turf: true for the sword.
pub fn sm_pre_attack(world: &mut World, user: Entity, item: Entity, target: Entity) -> bool {
    if !is(world, item, SM_SWORD) || world.get::<Health>(target).is_some() || item == target {
        return false;
    }
    sm_consume(world, user, item, target);
    true
}

/// `consume_everything(target)`.
fn sm_consume(world: &mut World, user: Entity, sword: Entity, target: Entity) {
    if target == user
        && let Some(at) = world.get::<TilePos>(user).copied()
    {
        crate::inventory::put_down(world, sword, at);
    }
    let Some(at) = crate::explosions::turf_of(world, target) else {
        return;
    };
    let tn = crate::martial::name(world, target);
    let seen = format!("{tn} касается суперматерии и мгновенно обращается в пепел.");
    if let Some(&pos) = world.get::<TilePos>(target) {
        let clients = crate::obj_damage::clients_in_range(world, pos, 7, &[]);
        world.write_message(ss13_net::chat::server::SendChat {
            clients,
            message: ss13_shared::chat::ChatMessage {
                channel: ss13_shared::chat::ChatChannel::Combat,
                segments: vec![ss13_shared::chat::ChatSegment::new(Span::Danger, &seen)],
            },
        });
    }
    if world.get::<Health>(target).is_some() {
        crate::health::dust(world, target);
    } else if world.get::<ss13_mapload::Turf>(target).is_some() {
        crate::construction::scrape_away(world, at, true);
    } else {
        crate::power::singularity::qdel(world, target);
    }
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(at, "sound/effects/supermatter.ogg", 50.0, true),
    );
    crate::radiation::radiation_pulse(world, sword, 6.0, 0.3, 30.0, 0.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn piston_strength_names() {
        assert_eq!(PowerFist::default().pressure, 1);
        assert_eq!(pressure_name(3), "высокую");
    }
}
