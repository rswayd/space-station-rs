//! The space changeling (`/datum/antagonist/changeling/space`, `generate_changeling_meteor`,
//! the `Midround Changeling` and `Mass Changelings` rulesets) and void adaption
//! (`/datum/action/changeling/void_adaption`).

use super::*;
use crate::breath::{BodyLife, HAZARD_LOW_PRESSURE, Worn, environment_of};

/// `SEND_SOUND(new_changeling, 'sound/effects/magic/mutate.ogg')`.
const MUTATE_SOUND: &str = "sound/effects/magic/mutate.ogg";
/// `/datum/antagonist/changeling/space/greet`.
const SPACE_GREETING: &str = "Наш разум пробуждается к жизни из глубин бесконечной дремоты...";
/// Void adaption's `recharge_slowdown`, chemicals per second.
const VOID_SLOWDOWN: f32 = 0.25;

/// Void adaption's `gain_traits` (`TRAIT_RESISTCOLD`, `TRAIT_RESISTLOWPRESSURE`,
/// `TRAIT_NO_BREATHLESS_DAMAGE`): the changeling has the power.
pub fn void_adapted(world: &World, mob: Entity) -> bool {
    world
        .get::<Changeling>(mob)
        .is_some_and(|c| !c.fallen && c.purchased.contains(&Power::VoidAdaption))
}

/// `give_power(power_path)`: the power without its genetic cost.
fn give_power(world: &mut World, mob: Entity, power: Power) {
    if let Some(mut ling) = world.get_mut::<Changeling>(mob)
        && !ling.purchased.contains(&power)
    {
        ling.purchased.push(power);
    }
}

/// `void_adaption/check_environment` on each `Life()`: protecting from vacuum or cold costs
/// [`VOID_SLOWDOWN`] of the chemical regeneration.
// tg subtracts the slowdown on activation (speeding the regeneration up, against the power's
// description); the described behaviour is ported.
pub(super) fn void_check_environment(world: &mut World, mob: Entity) {
    if !void_adapted(world, mob) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let environment = environment_of(world, mob, pos);
    let temperature = environment.temperature;
    let mut vulnerable = BodyLife::of(world, mob).cold_limit;
    let mut reasons = Vec::new();
    if world.get::<HumanLook>(mob).is_some()
        && let Some(inventory) = world.get::<Inventory>(mob)
    {
        let worn = Worn::read(world, inventory);
        vulnerable *= 1.0 - worn.cold_protection(temperature);
        if worn.affecting_pressure(environment.pressure()) < HAZARD_LOW_PRESSURE {
            reasons.push("vacuum");
        }
    }
    if temperature < vulnerable {
        reasons.push("cold");
    }
    let active = !reasons.is_empty();
    if world.get::<Changeling>(mob).map(|c| c.void_active) != Some(!active) {
        return;
    }
    let text = if active {
        let i = world
            .get_resource_or_init::<crate::combat::CombatRng>()
            .range(0, reasons.len() as u32 - 1) as usize;
        format!("Наши клетки укрепляются против {}.", reasons[i])
    } else {
        "Наши клетки отдыхают в безопасном воздухе.".to_owned()
    };
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.void_active = active;
        ling.chem_recharge_slowdown += if active {
            VOID_SLOWDOWN
        } else {
            -VOID_SLOWDOWN
        };
    }
    chat_to(world, mob, Span::Changeling, text);
}

/// `generate_changeling_meteor` for each client: a new human at the meteor's start, made a
/// space changeling with void adaption and the arm blade (one held in the left hand,
/// `/datum/outfit/changeling_space`), inside a changeling meteor flying at a random station
/// turf. The bodies made.
pub fn spawn_space_changelings(world: &mut World, clients: &[Entity]) -> Vec<Entity> {
    let mut out = Vec::new();
    for &client in clients {
        let Some(start) = crate::events::meteors::changeling_start(world) else {
            warn!("No valid spawn location for changeling meteor");
            break;
        };
        let Some(landing) =
            crate::events::electrical_storm::random_station_turf(world).or_else(|| {
                world
                    .get_resource::<crate::server::SpawnPoints>()
                    .and_then(|s| s.observer)
            })
        else {
            break;
        };
        let name = world
            .get_resource_mut::<crate::jobs::Jobs>()
            .and_then(|mut jobs| jobs.random_name(&Default::default()))
            .unwrap_or_default();
        let mob = crate::server::spawn_mob(world, client, start, name);
        world.entity_mut(client).remove::<crate::round::NewPlayer>();
        if !add_changeling_greeted(world, mob, Some(SPACE_GREETING)) {
            continue;
        }
        give_power(world, mob, Power::VoidAdaption);
        give_power(world, mob, Power::ArmBlade);
        if world.get::<Inventory>(mob).is_none() {
            world.entity_mut(mob).insert(Inventory::default());
        }
        if let Some(blade) = grow(world, mob, ARM_BLADE)
            && let Some(mut inventory) = world.get_mut::<Inventory>(mob)
        {
            inventory.hands[0] = Some(blade);
        }
        crate::inventory::refresh_look(world, mob);
        refresh(world, mob);
        crate::events::meteors::launch_with(world, mob, start, landing);
        if world.contains_resource::<Messages<ss13_net::sound::server::SendSound>>() {
            world.write_message(ss13_net::sound::server::SendSound {
                client,
                sound: ss13_shared::protocol::SoundEvent {
                    file: MUTATE_SOUND.into(),
                    volume: 100.0,
                    speed: 1.0,
                    offset: None,
                },
            });
        }
        info!("{mob} ({client}) is a space changeling, launched from {start:?} at {landing:?}");
        out.push(mob);
    }
    out
}
