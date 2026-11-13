//! The vault's bank machine (server), after tg `/obj/machinery/computer/bank_machine`
//! (`code/game/machinery/bank_machine.dm`) with tgui's `BankMachine` ([`super::bank_ui`]).
//!
//! - Linked to the cargo budget (`account_department = ACCOUNT_CAR`): money used on it (bills,
//!   holochips, coins, poker chips; [`super::money::credit_value`]) is deposited there.
//! - `siphon` (on a station level only): every machine tick 100 credits per second leave the
//!   budget into the machine; without vault access on the user's ID the siphon is unauthorized.
//!   Now and then (15% per tick, at most every 40 s) the machine reports the withdrawal on the
//!   common channel. No power or an empty budget halts it; `halt` and every halt drop the
//!   siphoned sum as space cash of the largest bills (`credits_to_spacecash`).
// shortcut: no "Forbidden Cash Signal" GPS of a rebuilt machine and no halt when the console
// changes z-level (consoles here never move between levels).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_mapload::spawner::Rng;
use ss13_shared::TilePos;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::MONEY_SYMBOL;
use super::bank_ui::{BankMachineData, BankMachineUi, HALT, SIPHON};
use super::console::machine_say;
use super::economy::{ACCOUNT_CAR, Economy};
use crate::obj_damage::is_under;
use crate::power::{MACHINE_TICK_SECS, MachineTicks, Unpowered};
use crate::sound::{PlaySound, playsound};

pub const BANK_MACHINE: &str = "/obj/machinery/computer/bank_machine";
/// `req_access = list(ACCESS_VAULT)`.
const ACCESS_VAULT: &str = "vault";
/// Credits per second of a siphon.
const SIPHON_PER_SEC: f64 = 100.0;
/// `minimum_time_between_warnings`.
const WARNING_SECS: f64 = 40.0;
const WARNING_CHANCE: f32 = 15.0;
const SIPHON_SOUND: &str = "sound/items/poster/poster_being_created.ogg";
/// `credits_to_spacecash`'s bills, largest first.
pub const BILLS: [(&str, i64); 9] = [
    ("/obj/item/stack/spacecash/c10000", 10000),
    ("/obj/item/stack/spacecash/c1000", 1000),
    ("/obj/item/stack/spacecash/c500", 500),
    ("/obj/item/stack/spacecash/c200", 200),
    ("/obj/item/stack/spacecash/c100", 100),
    ("/obj/item/stack/spacecash/c50", 50),
    ("/obj/item/stack/spacecash/c20", 20),
    ("/obj/item/stack/spacecash/c10", 10),
    ("/obj/item/stack/spacecash/c1", 1),
];

/// A bank machine's siphon.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct BankMachine {
    pub siphoning: bool,
    /// `syphoning_credits`.
    pub siphoned: i64,
    pub unauthorized: bool,
    /// `next_warning`, seconds.
    pub next_warning: f64,
}

#[derive(Resource)]
struct BankRng(Rng);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BankAct {
    Siphon,
    Halt,
}

pub struct BankMachinePlugin;

impl Plugin for BankMachinePlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::EmptyHand, 3501, |w, c| {
            crate::cargo::bank_machine::interact(w, c.client, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 2991, |w, c| {
            c.held
                .is_some_and(|h| crate::cargo::bank_machine::deposit(w, h, c.target))
        });
        app.register_ui::<BankMachineUi>()
            .init_resource::<BankStationName>()
            .add_systems(ss13_world::MapInit, setup_mapped)
            .add_systems(
                Update,
                (
                    acts.in_set(UiSystems::Handle),
                    siphon_tick.run_if(any_with_component::<BankMachine>),
                ),
            );
    }
}

fn setup_mapped(world: &mut World) {
    let seed = world
        .get_resource::<ss13_mapload::MapSeed>()
        .map_or(0, |s| s.0);
    world.insert_resource(BankRng(Rng::new(seed ^ 0x6261_6e6b)));
    let mut q = world.query::<(Entity, &DmObj)>();
    let found: Vec<Entity> = q
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, BANK_MACHINE))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        world.entity_mut(e).insert(BankMachine::default());
    }
}

fn cargo_balance(world: &World) -> i64 {
    world
        .get_resource::<Economy>()
        .and_then(|e| e.department(ACCOUNT_CAR))
        .map_or(0, |a| a.balance)
}

impl ServerUi for BankMachineUi {
    type Source = &'static BankMachine;
    type UserParam = (Option<Res<'static, Economy>>, Res<'static, BankStationName>);
    type Act = BankAct;

    fn static_data(_: &BankMachine) {}

    fn data(
        machine: &BankMachine,
        _: Entity,
        (economy, station): &(Option<Res<'_, Economy>>, Res<'_, BankStationName>),
    ) -> BankMachineData {
        BankMachineData {
            current_balance: economy
                .as_ref()
                .and_then(|e| e.department(ACCOUNT_CAR))
                .map_or(0, |a| a.balance),
            siphoning: machine.siphoning,
            station_name: station.0.clone(),
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<BankAct, UiReject> {
        match action {
            SIPHON => Ok(BankAct::Siphon),
            HALT => Ok(BankAct::Halt),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// `station_name()` for the window, refreshed when a machine's window opens.
#[derive(Resource, Clone, Debug, Default)]
pub struct BankStationName(pub String);

/// An empty hand opens the window.
pub fn interact(world: &mut World, client: Entity, mob: Entity, e: Entity) -> bool {
    if world.get::<BankMachine>(e).is_none() {
        return false;
    }
    let station = crate::events::station_name(world);
    world.insert_resource(BankStationName(station));
    let title = crate::security::name_of(world, e);
    world.write_message(OpenUi::new::<BankMachineUi>(client, mob, e, title));
    true
}

/// tg `item_interaction`: money is deposited into the cargo budget and consumed.
pub fn deposit(world: &mut World, held: Entity, e: Entity) -> bool {
    if world.get::<BankMachine>(e).is_none() {
        return false;
    }
    let value = super::money::credit_value(world, held);
    if value <= 0 {
        return false;
    }
    let said = world.get_resource_mut::<Economy>().and_then(|mut economy| {
        let account = economy.department_mut(ACCOUNT_CAR)?;
        account.adjust_money(value);
        Some(format!(
            "{MONEY_SYMBOL} deposited! The {} is now {} {MONEY_SYMBOL}.",
            account.holder, account.balance
        ))
    });
    if let Some(text) = said {
        machine_say(world, e, &text);
    }
    world.despawn(held);
    true
}

/// `credits_to_spacecash`: the fewest bills for `amount`, as (type, count).
pub fn credits_to_spacecash(amount: i64) -> Vec<(&'static str, u32)> {
    let mut left = amount.max(0);
    let mut out = Vec::new();
    for (path, value) in BILLS {
        let n = left / value;
        if n > 0 {
            out.push((path, n as u32));
            left -= n * value;
        }
    }
    out
}

/// `end_siphon`: the siphoned credits drop as cash on the machine's tile.
pub fn end_siphon(world: &mut World, e: Entity) {
    let Some(siphoned) = world.get_mut::<BankMachine>(e).map(|mut m| {
        m.siphoning = false;
        m.unauthorized = false;
        std::mem::take(&mut m.siphoned)
    }) else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    for (path, count) in credits_to_spacecash(siphoned) {
        if crate::construction::spawn_stack(world, path, pos, count).is_none() {
            warn!("no {path} prototype (rerun `ss13-mapload convert`)");
        }
    }
}

/// tg `ui_act`.
fn acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<BankMachineUi>>>) {
    let pending: Vec<(Entity, Entity, BankAct)> = cursor
        .read(world.resource::<Messages<UiActed<BankMachineUi>>>())
        .map(|a| (a.actor, a.source, a.act))
        .collect();
    for (actor, e, act) in pending {
        if world.get::<BankMachine>(e).is_none() {
            continue;
        }
        match act {
            BankAct::Siphon => {
                let on_station = world
                    .get::<TilePos>(e)
                    .is_some_and(|&p| crate::comms_server::on_station_level(world, p));
                if !on_station {
                    machine_say(
                        world,
                        e,
                        "Error: Console not in reach of station, withdrawal cannot begin.",
                    );
                    continue;
                }
                machine_say(world, e, "Siphon of station credits has begun!");
                // `start_siphon`: the user's ID (hands first) against `req_access`.
                let authorized =
                    crate::inventory::id_allowed(world, actor, &[ACCESS_VAULT.to_owned()], &[]);
                if let Some(mut m) = world.get_mut::<BankMachine>(e) {
                    m.unauthorized = !authorized;
                    m.siphoning = true;
                }
            }
            BankAct::Halt => {
                machine_say(world, e, "Station credits withdrawal halted.");
                end_siphon(world, e);
            }
        }
    }
}

/// `process(seconds_per_tick)` on each machine tick.
fn siphon_tick(world: &mut World, mut last: Local<Option<u64>>) {
    let ticks = world.resource::<MachineTicks>().0;
    let from = last.unwrap_or(ticks);
    *last = Some(ticks);
    for _ in from..ticks {
        let machines: Vec<Entity> = world
            .query::<(Entity, &BankMachine)>()
            .iter(world)
            .filter(|(_, m)| m.siphoning)
            .map(|(e, _)| e)
            .collect();
        for e in machines {
            process(world, e, MACHINE_TICK_SECS);
        }
    }
}

fn process(world: &mut World, e: Entity, seconds_per_tick: f64) {
    if world.get::<Unpowered>(e).is_some() {
        machine_say(world, e, "Insufficient power. Halting siphon.");
        end_siphon(world, e);
        return;
    }
    let amount = (SIPHON_PER_SEC * seconds_per_tick).round() as i64;
    let taken = world.get_resource_mut::<Economy>().and_then(|mut economy| {
        let account = economy.department_mut(ACCOUNT_CAR)?;
        if account.has_money(amount) {
            account.adjust_money(-amount);
            Some(())
        } else {
            None
        }
    });
    if taken.is_none() {
        let holder = world
            .get_resource::<Economy>()
            .and_then(|e| e.department(ACCOUNT_CAR))
            .map(|a| a.holder.clone())
            .unwrap_or_default();
        machine_say(world, e, &format!("{holder} depleted. Halting siphon."));
        end_siphon(world, e);
        return;
    }
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    playsound(world, PlaySound::new(pos, SIPHON_SOUND, 100.0, true));
    let now = crate::tools::now(world);
    let warn = {
        let Some(mut m) = world.get_mut::<BankMachine>(e) else {
            return;
        };
        m.siphoned += amount;
        m.next_warning < now
    } && world.resource_mut::<BankRng>().0.prob(WARNING_CHANCE);
    if !warn {
        return;
    }
    let unauthorized = world.get::<BankMachine>(e).is_some_and(|m| m.unauthorized);
    if let Some(mut m) = world.get_mut::<BankMachine>(e) {
        m.next_warning = now + WARNING_SECS;
    }
    let area = world
        .get_resource::<ss13_mapload::Areas>()
        .and_then(|a| a.area_at(pos))
        .map(|a| crate::shuttle::area_name(world, a))
        .unwrap_or_default();
    let text = if unauthorized {
        format!("Unauthorized credit withdrawal underway in {area}!!")
    } else {
        format!("Credit withdrawal underway in {area}...")
    };
    let name = crate::security::name_of(world, e);
    world.write_message(crate::radio::MachineRadio {
        freq: ss13_chat::radio::FREQ_COMMON,
        z: pos.z,
        name,
        text,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cash_takes_the_largest_bills() {
        assert_eq!(
            credits_to_spacecash(12_731),
            vec![
                ("/obj/item/stack/spacecash/c10000", 1),
                ("/obj/item/stack/spacecash/c1000", 2),
                ("/obj/item/stack/spacecash/c500", 1),
                ("/obj/item/stack/spacecash/c200", 1),
                ("/obj/item/stack/spacecash/c20", 1),
                ("/obj/item/stack/spacecash/c10", 1),
                ("/obj/item/stack/spacecash/c1", 1),
            ]
        );
        assert!(credits_to_spacecash(0).is_empty());
        assert!(credits_to_spacecash(-5).is_empty());
    }

    #[test]
    fn siphon_drains_the_cargo_budget() {
        let mut world = World::new();
        let mut economy = Economy::new(1);
        economy
            .department_mut(ACCOUNT_CAR)
            .unwrap()
            .adjust_money(450);
        world.insert_resource(economy);
        world.insert_resource(BankRng(Rng::new(1)));
        world.insert_resource(Time::<()>::default());
        let e = world
            .spawn((
                BankMachine {
                    siphoning: true,
                    ..default()
                },
                TilePos::new(1, 1, 1),
            ))
            .id();
        process(&mut world, e, MACHINE_TICK_SECS);
        process(&mut world, e, MACHINE_TICK_SECS);
        assert_eq!(world.get::<BankMachine>(e).unwrap().siphoned, 400);
        assert_eq!(cargo_balance(&world), 50);
    }
}
