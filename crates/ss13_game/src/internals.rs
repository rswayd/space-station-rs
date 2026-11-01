//! Gas tanks and internals (server), after tg `code/game/objects/items/tanks/tanks.dm`,
//! `tank_types.dm` and `/mob/living/carbon/human/toggle_internals`.
//!
//! - Every `/obj/item/tank` gets [`TankAir`] when it spawns: its `volume`, `distribute_pressure`
//!   and the gas of its type's `populate_gas()` ([`populate_gas`]).
//! - Alt+click on a tank in the inventory (`ClientIntent::AltClickSlot`) toggles it as the mob's
//!   internals ([`Internals`]) when a mask with `MASKINTERNALS` or a helmet with
//!   `HEADINTERNALS` is worn; `crate::breath` then breathes [`TankAir::remove_air_volume`].
//! - A tank that leaves the mob closes at once (tg `doUnEquip`); a mask taken off closes it on
//!   the next breath (tg `invalid_internals`).

use bevy::prelude::*;
use ss13_atmos::gas::{ONE_ATMOSPHERE, R_IDEAL_GAS_EQUATION, T20C};
use ss13_atmos::{GasId, GasMixture};
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmClothing, DmItem, DmName, DmTank};
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};

use crate::inventory::{CarriedBy, Inventory};
use crate::server::PlayerMob;

/// `clothing_flags` bits (`code/__DEFINES/obj_flags.dm`).
pub const STOPSPRESSUREDAMAGE: u32 = 1 << 1;
pub const MASKINTERNALS: u32 = 1 << 3;
pub const HEADINTERNALS: u32 = 1 << 11;

/// A tank's gas (tg `air_contents`, `volume` litres) and release pressure.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct TankAir {
    pub air: GasMixture,
    pub volume: f64,
    pub distribute_pressure: f64,
}

impl TankAir {
    /// kPa (tg `return_pressure` of a `volume`-litre mixture).
    pub fn pressure(&self) -> f64 {
        pressure(&self.air, self.volume)
    }

    /// tg `remove_air_volume`: `volume_to_return` litres at the release pressure (or less, when
    /// the tank holds less). `None` when nothing comes out.
    pub fn remove_air_volume(&mut self, volume_to_return: f64) -> Option<GasMixture> {
        let actual = self.pressure().clamp(0.0, self.distribute_pressure);
        let moles_needed =
            actual * volume_to_return / (R_IDEAL_GAS_EQUATION * self.air.temperature);
        self.air.remove(moles_needed)
    }
}

/// kPa of `mix` in `volume` litres (`ss13_atmos` mixtures are turf-sized).
pub fn pressure(mix: &GasMixture, volume: f64) -> f64 {
    mix.total_moles() * R_IDEAL_GAS_EQUATION * mix.temperature / volume
}

/// tg `internal`: the tank the mob breathes from.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Internals(pub Option<Entity>);

/// `populate_gas()` of the internals tanks (`tank_types.dm`): per type, `(gas, atmospheres)`
/// of `X*ONE_ATMOSPHERE*volume/(R_IDEAL_GAS_EQUATION*T20C)` moles each. The longest matching
/// type wins (procs are inherited); an empty list is a `populate_gas()` that adds nothing.
const POPULATE: [(&str, &[(&str, f64)]); 20] = [
    ("/obj/item/tank/internals/oxygen", &[("o2", 6.0)]),
    ("/obj/item/tank/internals/oxygen/empty", &[]),
    (
        "/obj/item/tank/internals/anesthetic",
        &[("o2", 3.0 * 0.21), ("n2o", 3.0 * 0.79)],
    ),
    ("/obj/item/tank/internals/anesthetic/pure", &[("n2o", 10.0)]),
    ("/obj/item/tank/internals/plasma", &[("plasma", 3.0)]),
    ("/obj/item/tank/internals/plasma/full", &[("plasma", 10.0)]),
    ("/obj/item/tank/internals/plasma/empty", &[]),
    ("/obj/item/tank/internals/plasmaman", &[("plasma", 3.0)]),
    (
        "/obj/item/tank/internals/plasmaman/full",
        &[("plasma", 10.0)],
    ),
    (
        "/obj/item/tank/internals/plasmaman/belt/full",
        &[("plasma", 10.0)],
    ),
    ("/obj/item/tank/internals/plasmaman/belt/empty", &[]),
    ("/obj/item/tank/internals/emergency_oxygen", &[("o2", 10.0)]),
    ("/obj/item/tank/internals/emergency_oxygen/empty", &[]),
    ("/obj/item/tank/internals/emergency_oxygen/engi/empty", &[]),
    (
        "/obj/item/tank/internals/emergency_oxygen/double/empty",
        &[],
    ),
    ("/obj/item/tank/internals/generic", &[]),
    (
        "/obj/item/tank/internals/emergency_oxygen/engi/clown/n2o",
        &[("o2", 10.0 * 0.95), ("n2o", 10.0 * 0.05)],
    ),
    (
        "/obj/item/tank/internals/emergency_oxygen/engi/clown/bz",
        &[("o2", 10.0 * 0.9), ("bz", 10.0 * 0.1)],
    ),
    (
        "/obj/item/tank/internals/emergency_oxygen/engi/clown/helium",
        &[("o2", 10.0 * 0.75), ("helium", 10.0 * 0.25)],
    ),
    // `/obj/item/tank/populate_gas()` returns: jetpacks and other tanks start empty.
    ("/obj/item/tank", &[]),
];

/// tg `Initialize`: `new(volume)` at `T20C`, then the type's `populate_gas()`.
pub fn populate_gas(path: &str, volume: f64) -> GasMixture {
    let mut air = GasMixture::vacuum();
    air.temperature = T20C;
    let gases = POPULATE
        .iter()
        .filter(|(root, _)| is_under(path, root))
        .max_by_key(|(root, _)| root.len())
        .map_or(&[][..], |(_, gases)| *gases);
    for &(id, atmospheres) in gases {
        let gas = GasId::parse(id).expect("tg gas ids");
        air.moles[usize::from(gas.0)] =
            atmospheres * ONE_ATMOSPHERE * volume / (R_IDEAL_GAS_EQUATION * T20C);
    }
    air
}

pub struct InternalsPlugin;

impl Plugin for InternalsPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(fill_tank).add_observer(tank_left_mob);
    }
}

/// Deferred: the prototype may insert `DmItem` after `DmTank`.
fn fill_tank(add: On<Add<DmTank>>, mut commands: Commands) {
    let tank = add.entity;
    commands.queue(move |world: &mut World| {
        let Ok(entity) = world.get_entity(tank) else {
            return;
        };
        if entity.contains::<TankAir>() {
            return;
        }
        let (Some(dm), Some(item)) = (entity.get::<DmTank>(), entity.get::<DmItem>()) else {
            return;
        };
        let volume = f64::from(dm.volume);
        let air = TankAir {
            air: populate_gas(&item.path, volume),
            volume,
            distribute_pressure: f64::from(dm.distribute_pressure),
        };
        world.entity_mut(tank).insert(air);
    });
}

/// tg `/mob/living/carbon/doUnEquip`: the open tank left the mob, its valve closes.
fn tank_left_mob(
    remove: On<Remove<CarriedBy>>,
    carried: Query<&CarriedBy>,
    mut mobs: Query<(&mut Internals, Option<&PlayerMob>)>,
    names: Query<&DmName>,
    mut chat: MessageWriter<SendChat>,
) {
    let tank = remove.entity;
    let Ok(&CarriedBy(mob)) = carried.get(tank) else {
        return;
    };
    if let Ok((mut internals, player)) = mobs.get_mut(mob)
        && internals.0 == Some(tank)
    {
        internals.0 = None;
        if let Some(player) = player {
            chat.write(cutoff_message(player.client, name_of(&names, tank)));
        }
    }
}

fn name_of(names: &Query<&DmName>, item: Entity) -> String {
    names
        .get(item)
        .map_or_else(|_| "баллон".into(), |n| n.name.clone())
}

fn notice(client: Entity, text: String) -> SendChat {
    SendChat {
        clients: vec![client],
        message: ChatMessage {
            channel: ChatChannel::Info,
            segments: vec![ChatSegment::new(Span::Notice, text)],
        },
    }
}

/// tg `cutoff_internals`.
// shortcut: item names are not declined (no `declent_ru`: the ru_names translations are not
// loaded), here and in the other internals messages.
pub(crate) fn cutoff_message(client: Entity, tank: String) -> SendChat {
    notice(
        client,
        format!("Вы перестаете дышать из {tank}, и клапан перекрывается."),
    )
}

/// tg `can_breathe_internals` of a human without a breathing tube: a worn mask with
/// `MASKINTERNALS` or a helmet with `HEADINTERNALS`.
// shortcut: masks and visors are never pulled down (`up`), so `visor_flags` never apply.
pub fn can_breathe_internals(world: &World, inventory: &Inventory) -> bool {
    let flags = |item: Option<Entity>| {
        item.and_then(|i| world.get::<DmClothing>(i))
            .map_or(0, |c| c.clothing_flags)
    };
    flags(inventory.mask) & MASKINTERNALS != 0 || flags(inventory.head) & HEADINTERNALS != 0
}

/// tg `/mob/living/carbon/human/toggle_internals(tank)`: closes the tank if it is the open one,
/// opens it if the mob can breathe from it, tells the user why not otherwise.
pub fn toggle(world: &mut World, client: Entity, mob: Entity, tank: Entity) {
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return;
    };
    let current = world.get::<Internals>(mob).copied().unwrap_or_default().0;
    let name = world
        .get::<DmName>(tank)
        .map_or_else(|| "баллон".into(), |n| n.name.clone());
    let text = if current == Some(tank) {
        world.entity_mut(mob).insert(Internals(None));
        format!("Вы перекрываете клапан на {name}.")
    } else if can_breathe_internals(world, &inventory) {
        // tg `toggle_open_internals`: switching from another open tank reads differently.
        world.entity_mut(mob).insert(Internals(Some(tank)));
        if current.is_some() {
            format!("Вы начинаете дышать из {name}.")
        } else {
            format!("Вы открываете клапан на {name}.")
        }
    } else if let Some(mask) = inventory.mask {
        let mask = world
            .get::<DmName>(mask)
            .map_or_else(String::new, |n| n.name.clone());
        format!("{mask} не подходит!")
    } else {
        "Вам нужна подходящая маска или шлем!".to_owned()
    };
    world.write_message(notice(client, text));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tanks_start_with_tg_populate_gas() {
        // Emergency oxygen: 10 atm in 3 L at 20 C.
        let air = populate_gas("/obj/item/tank/internals/emergency_oxygen", 3.0);
        let expected = 10.0 * ONE_ATMOSPHERE * 3.0 / (R_IDEAL_GAS_EQUATION * T20C);
        assert!((air.get(GasId::O2) - expected).abs() < 1e-9);
        assert!((pressure(&air, 3.0) - 1013.25).abs() < 1e-6);
        // `engi` inherits the proc with its own 6 L.
        let engi = populate_gas("/obj/item/tank/internals/emergency_oxygen/engi", 6.0);
        assert!((pressure(&engi, 6.0) - 1013.25).abs() < 1e-6);
        assert_eq!(
            populate_gas("/obj/item/tank/internals/emergency_oxygen/engi/empty", 6.0).total_moles(),
            0.0
        );
        let anesthetic = populate_gas("/obj/item/tank/internals/anesthetic", 70.0);
        assert!((pressure(&anesthetic, 70.0) - 3.0 * ONE_ATMOSPHERE).abs() < 1e-6);
        assert_eq!(
            populate_gas("/obj/item/tank/jetpack", 70.0).total_moles(),
            0.0
        );
    }

    #[test]
    fn a_breath_from_a_tank_is_at_release_pressure() {
        let mut tank = TankAir {
            air: populate_gas("/obj/item/tank/internals/emergency_oxygen", 3.0),
            volume: 3.0,
            distribute_pressure: 16.0,
        };
        let breath = tank.remove_air_volume(1.99).unwrap();
        // 16 kPa * 1.99 L / (R * T20C) = 0.013072 mol, quantized to 0.0131.
        assert!((breath.get(GasId::O2) - 0.0131).abs() < 1e-12);
        let pp = pressure(&breath, 1.99);
        assert!(pp > 16.0 && pp < 16.1, "{pp}");
        // An empty tank gives nothing.
        tank.air = GasMixture::vacuum();
        assert!(tank.remove_air_volume(1.99).is_none());
    }
}
