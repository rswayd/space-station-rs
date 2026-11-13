//! Special objects of themed shuttles (server), after tg `code/modules/shuttle/misc/special.dm`.
//!
//! - The energy cannon (`/obj/machinery/power/emitter/energycannon`) is an emitter that fires
//!   welded, locked and without power (`crate::power::emitter`).
//! - Wabbajack altars (`/obj/structure/table/abductor/wabbajack`, every 2 s like `SSobj`): a
//!   living carbon lying on one is put to sleep (refreshed to 20 s) with a purple glow message;
//!   after 10 s asleep there it leaves its body as a ghost that can come back, and leaving the
//!   altar pulls the ghost back in. An occupied altar chants its description (the first time,
//!   then 5%). With two occupied altars the wabbajack statue within 4 tiles opens its eyes.
//! - The shuttle bar (`/obj/structure/table/wood/shuttle_bar`): whoever climbs onto it and is
//!   not bar staff (a Bartender, a bardrone, an ID with CentCom bar access) is paralyzed 4 s and
//!   thrown 5 tiles off it.
//! - The luxury shuttle's ticket field (`/obj/machinery/scanner_gate/luxury_shuttle`): only
//!   approved passengers (and items, and structures without anyone inside) pass. A mob (or a
//!   vehicle, for its driver) bumping it pays 500 cr from its ID's account, then from the
//!   money it carries; partial payments add up; change comes back as a holochip; then it (the
//!   vehicle and its driver) is approved for every such gate. A vehicle with an unapproved
//!   occupant is refused ("Stowaway detected").
//!
//! Cost: the altars once per 2 s; one tile scan per step onto a bar; the gates per bump.
// shortcut: the wabbajack statue fires nothing (no `/obj/projectile/magic/change`), no purple
// colour on sleepers, no "inert" altar name; the bar throws north (no mapped `boot_dir`); the
// gate takes no pulled ID card or money and gives no armless hint; no bardrones (drones are not
// ported) or fight pit bear behaviour beyond a bear's; add with those mechanics.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::DmObj;
use ss13_shared::chat::Span;
use ss13_shared::{Dir, TilePos};
use ss13_world::{Bumped, EnterPolicy, Moved, SpatialIndex};
use std::collections::{HashMap, HashSet};

use crate::cargo::MONEY_SYMBOL;
use crate::combat::CombatRng;
use crate::health::{Health, Stat};
use crate::medical::name_of;
use crate::obj_damage::is_under;
use crate::power::emitter::Emitter;
use crate::sound::{PlaySound, playsound};
use crate::status_effects::Incap;
use crate::tools::{client_of, tell, tell_mob};

pub const WABBAJACK_ALTAR: &str = "/obj/structure/table/abductor/wabbajack";
const ALTAR_LEFT: &str = "/obj/structure/table/abductor/wabbajack/left";
const ALTAR_RIGHT: &str = "/obj/structure/table/abductor/wabbajack/right";
pub const SHUTTLE_BAR: &str = "/obj/structure/table/wood/shuttle_bar";
pub const LUXURY_GATE: &str = "/obj/machinery/scanner_gate/luxury_shuttle";
const BARDRONE: &str = "/mob/living/basic/drone/snowflake/bardrone";
const STRUCTURE: &str = "/obj/structure";
const BUZZ: &str = "sound/machines/scanner/scanbuzz.ogg";
/// `ACCESS_CENT_BAR`.
const ACCESS_CENT_BAR: &str = "cent_bar";

/// `SSobj` wait, seconds; the sleepers' `SetSleeping(200)`; `sleeper_dreams` after 10 s.
const PROCESS_SECS: f64 = 2.0;
const SLEEP_SECS: f32 = 20.0;
const DREAM_SECS: f64 = 10.0;
/// `tables_required`, `orange(4, src)`.
const TABLES_REQUIRED: usize = 2;
const STATUE_RANGE: u32 = 4;
/// The bar's `Paralyze(40)` and `throw_at(throwtarget, 5, 1)`.
const BAR_PARALYZE_SECS: f32 = 4.0;
const BAR_THROW_RANGE: u32 = 5;
/// The gate's `threshold`, `LUXURY_MESSAGE_COOLDOWN`, the alarm's beep cooldown.
pub const LUXURY_THRESHOLD: i64 = 500;
const MESSAGE_SECS: f64 = 10.0;
const BEEP_SECS: f64 = 2.0;

/// A wabbajack altar's state.
#[derive(Component, Clone, Debug, Default)]
pub struct WabbajackAltar {
    /// `sleepers`, with when each one dreams itself out of its body.
    pub sleepers: Vec<(Entity, f64)>,
    pub never_spoken: bool,
}

/// The wabbajack statue's `active_tables`.
#[derive(Component, Clone, Debug, Default)]
pub struct WabbajackStatue {
    pub tables: Vec<Entity>,
}

/// `/obj/structure/table/wood/shuttle_bar`.
#[derive(Component, Clone, Copy, Debug)]
pub struct ShuttleBar;

/// The luxury ticket field: `payees` and its beep cooldown.
#[derive(Component, Clone, Debug, Default)]
pub struct LuxuryGate {
    pub payees: HashMap<Entity, i64>,
    next_beep: f64,
}

/// The luxury gates' shared `approved_passengers` and `check_times`.
#[derive(Resource, Clone, Debug, Default)]
pub struct LuxuryPassengers {
    pub approved: HashSet<Entity>,
    check_times: HashMap<Entity, f64>,
}

pub struct ShuttleSpecialPlugin;

impl Plugin for ShuttleSpecialPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LuxuryPassengers>()
            .add_observer(add_special)
            .add_systems(
                Update,
                (
                    altars,
                    (bar_climbs, luxury_bumps).after(ss13_world::WorldSystems::Movement),
                ),
            );
    }
}

fn add_special(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(obj) = objs.get(add.entity) else {
        return;
    };
    let mut e = commands.entity(add.entity);
    if is_under(&obj.path, WABBAJACK_ALTAR) {
        e.insert(WabbajackAltar {
            sleepers: Vec::new(),
            never_spoken: true,
        });
    } else if is_under(&obj.path, SHUTTLE_BAR) {
        e.insert(ShuttleBar);
    } else if is_under(&obj.path, LUXURY_GATE) {
        e.insert((LuxuryGate::default(), EnterPolicy(luxury_allows)));
    }
}

// --- Wabbajack altars -----------------------------------------------------------------------

/// The altar's `desc`, which it chants.
fn altar_line(path: &str) -> &'static str {
    if is_under(path, ALTAR_LEFT) {
        "Ты спишь, чтобы оно проснулось."
    } else if is_under(path, ALTAR_RIGHT) {
        "Оно просыпается, чтобы ты спал."
    } else {
        "Спишь ты или бодрствуешь — будет весьма хаотично."
    }
}

/// `/obj/structure/table/abductor/wabbajack/process` and the statue's `process_early` eyes.
fn altars(world: &mut World, mut next: Local<f64>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + PROCESS_SECS;
    let mut q = world.query::<(Entity, &WabbajackAltar, &DmObj, &TilePos)>();
    let altars: Vec<(Entity, WabbajackAltar, String, TilePos)> = q
        .iter(world)
        .map(|(e, a, o, &p)| (e, a.clone(), o.path.clone(), p))
        .collect();
    let mut statues: Vec<Entity> = Vec::new();
    for (altar, state, path, pos) in altars {
        let Some(statue) = statue_near(world, pos) else {
            continue;
        };
        if !statues.contains(&statue) {
            statues.push(statue);
        }
        let found: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .filter(|&m| {
                crate::throw::is_carbon(world, m)
                    && world.get::<Stat>(m).is_some_and(|s| *s != Stat::Dead)
            })
            .collect();
        let mut sleepers = Vec::new();
        for &mob in &found {
            match state.sleepers.iter().find(|(m, _)| *m == mob) {
                Some(&(_, dream_at)) => sleepers.push((mob, dream_at)),
                None => {
                    let who = name_of(world, mob);
                    crate::medical::visible(
                        world,
                        mob,
                        altar_line(&path),
                        &format!(
                            "Странное фиолетовое свечение окутывает {who}, и он внезапно \
                             засыпает."
                        ),
                        Span::Notice,
                    );
                    sleepers.push((mob, now + DREAM_SECS));
                }
            }
            crate::status_effects::apply(world, mob, Incap::Sleeping, SLEEP_SECS);
        }
        for &(mob, _) in &state.sleepers {
            if found.contains(&mob) || world.get_entity(mob).is_err() {
                continue;
            }
            let who = name_of(world, mob);
            crate::medical::visible(
                world,
                mob,
                "",
                &format!("Свечение вокруг {who} угасает."),
                Span::Notice,
            );
            crate::mind::grab_ghost(world, mob, false);
        }
        // `sleeper_dreams`: out of the body as a ghost that may come back.
        for (mob, dream_at) in &mut sleepers {
            if now < *dream_at || *dream_at < 0.0 {
                continue;
            }
            *dream_at = -1.0;
            if let Some(client) = client_of(world, *mob) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Во сне вам снится странный сон, будто вы видите себя со стороны.",
                );
                crate::ghost::ghostize_client(world, client);
            }
        }
        let occupied = !sleepers.is_empty();
        let speak =
            occupied && (state.never_spoken || world.resource_mut::<CombatRng>().chance(5.0));
        if let Some(mut a) = world.get_mut::<WabbajackAltar>(altar) {
            a.sleepers = sleepers;
            if speak {
                a.never_spoken = false;
            }
        }
        if speak {
            crate::atmos_machinery::machine_say(world, altar, altar_line(&path));
        }
        let mut s = world
            .get::<WabbajackStatue>(statue)
            .cloned()
            .unwrap_or_default();
        s.tables.retain(|&t| t != altar);
        if occupied {
            s.tables.push(altar);
        }
        world.entity_mut(statue).insert(s);
    }
    for statue in statues {
        statue_eyes(world, statue);
    }
}

/// `locate(/obj/machinery/power/emitter/energycannon/magical) in orange(4, src)`.
fn statue_near(world: &World, pos: TilePos) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .in_radius(pos, STATUE_RANGE)
        .filter(|&e| world.get::<TilePos>(e) != Some(&pos))
        .find(|&e| world.get::<Emitter>(e).is_some_and(|em| em.magical))
}

/// `energycannon/magical/process_early`: the eyes open with enough occupied altars.
fn statue_eyes(world: &mut World, statue: Entity) {
    let tables = world
        .get::<WabbajackStatue>(statue)
        .map_or(0, |s| s.tables.len());
    let active = tables >= TABLES_REQUIRED;
    let Some(was) = world.get::<Emitter>(statue).map(|e| e.active) else {
        return;
    };
    if was == active {
        return;
    }
    world.get_mut::<Emitter>(statue).expect("checked").active = active;
    let what = name_of(world, statue);
    if let Some(&pos) = world.get::<TilePos>(statue) {
        let text = if active {
            format!("{what} открывает глаза.")
        } else {
            format!("{what} закрывает глаза.")
        };
        crate::mecha::core::visible(world, pos, Span::Notice, text, &[]);
    }
    let state = if active {
        "wabbajack_statue_on"
    } else {
        "wabbajack_statue"
    };
    crate::obj_damage::set_icon_state(world, statue, state);
}

// --- The shuttle bar ------------------------------------------------------------------------

/// `is_barstaff`.
fn is_barstaff(world: &World, mob: Entity) -> bool {
    if world.get::<crate::human::HumanLook>(mob).is_some()
        && crate::service::server::job_title(world, mob).as_deref() == Some("Bartender")
    {
        return true;
    }
    if is_under(&crate::bots::mob_path(world, mob), BARDRONE) {
        return true;
    }
    crate::inventory::mob_access(world, mob)
        .iter()
        .any(|a| a == ACCESS_CENT_BAR)
}

/// `/obj/structure/table/wood/shuttle_bar/on_climbed` (`COMSIG_ATOM_ENTERED` of its tile).
fn bar_climbs(world: &mut World, mut moves: Local<MessageCursor<Moved>>) {
    let moved: Vec<(Entity, TilePos)> = moves
        .read(world.resource::<Messages<Moved>>())
        .map(|m| (m.entity, m.to))
        .collect();
    for (mob, to) in moved {
        if world.get::<Health>(mob).is_none()
            || world
                .get::<crate::antag::revenant::Incorporeal>(mob)
                .is_some()
        {
            continue;
        }
        let on_bar = world
            .resource::<SpatialIndex>()
            .on_tile(to)
            .iter()
            .any(|&e| world.get::<ShuttleBar>(e).is_some());
        if !on_bar || is_barstaff(world, mob) {
            continue;
        }
        crate::status_effects::apply(world, mob, Incap::Paralyze, BAR_PARALYZE_SECS);
        let target = (0..BAR_THROW_RANGE).fold(to, |p, _| p.step(Dir::North));
        crate::throw::throw_at(world, mob, target, BAR_THROW_RANGE, 1.0);
        tell_mob(
            world,
            mob,
            Span::Notice,
            "Не залезайте на барную стойку, пожалуйста.",
        );
    }
}

// --- The luxury ticket field ----------------------------------------------------------------

/// The living occupants of a vehicle (`vehicle.occupants`).
fn occupants(world: &World, vehicle: Entity) -> Vec<Entity> {
    if let Some(car) = world.get::<crate::cars::Car>(vehicle) {
        return car.occupants.clone();
    }
    world
        .get::<crate::vehicles::Vehicle>(vehicle)
        .and_then(|v| v.rider)
        .into_iter()
        .collect()
}

fn is_vehicle(world: &World, e: Entity) -> bool {
    world.get::<crate::cars::Car>(e).is_some() || world.get::<crate::vehicles::Vehicle>(e).is_some()
}

/// `/obj/machinery/scanner_gate/luxury_shuttle/CanAllowThrough`.
fn luxury_allows(world: &World, mover: Entity, _gate: Entity) -> bool {
    let approved = |e: Entity| {
        world
            .get_resource::<LuxuryPassengers>()
            .is_some_and(|p| p.approved.contains(&e))
    };
    if approved(mover) {
        return occupants(world, mover).into_iter().all(approved);
    }
    if world.get::<ss13_mapload::DmItem>(mover).is_some() {
        return true;
    }
    let structure = world
        .get::<DmObj>(mover)
        .is_some_and(|o| is_under(&o.path, STRUCTURE));
    if structure {
        // Nobody inside (`for(var/mob/living/rat in struct.contents)`): the mobs inside keep
        // its tile.
        let Some(&pos) = world.get::<TilePos>(mover) else {
            return true;
        };
        return !world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .any(|&e| {
                world
                    .get::<crate::closets::InCloset>(e)
                    .is_some_and(|c| c.closet == mover)
            });
    }
    false
}

/// tg `alarm_beep` of the gate: the buzz once per 2 s.
fn alarm_beep(world: &mut World, gate: Entity, now: f64) {
    let Some(mut g) = world.get_mut::<LuxuryGate>(gate) else {
        return;
    };
    if now < g.next_beep {
        return;
    }
    g.next_beep = now + BEEP_SECS;
    if let Some(&pos) = world.get::<TilePos>(gate) {
        playsound(
            world,
            PlaySound::new(pos, BUZZ, 30.0, false).extrarange(-3.0),
        );
    }
}

/// A message to `mob` at most once per `LUXURY_MESSAGE_COOLDOWN` (`check_times`).
fn throttled(world: &mut World, mob: Entity, now: f64, text: String) {
    let mut p = world.resource_mut::<LuxuryPassengers>();
    if p.check_times.get(&mob).is_some_and(|&t| now < t) {
        return;
    }
    p.check_times.insert(mob, now + MESSAGE_SECS);
    tell_mob(world, mob, Span::Notice, text);
}

/// The bank account of `mob`'s ID (`get_bank_account`, the driver's `get_idcard`).
fn account_of(world: &World, mob: Entity) -> Option<u32> {
    let card = world
        .get::<crate::inventory::Inventory>(mob)?
        .id_card(world)?;
    crate::cargo::economy::account_of(world, card)
}

/// `/obj/machinery/scanner_gate/luxury_shuttle/Bumped`.
fn luxury_bumps(world: &mut World, mut bumps: Local<MessageCursor<Bumped>>) {
    let bumped: Vec<(Entity, Entity)> = bumps
        .read(world.resource::<Messages<Bumped>>())
        .filter(|b| world.get::<LuxuryGate>(b.obstacle).is_some())
        .map(|b| (b.entity, b.obstacle))
        .collect();
    for (mover, gate) in bumped {
        luxury_bump(world, gate, mover);
    }
}

fn luxury_bump(world: &mut World, gate: Entity, mover: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let vehicle = is_vehicle(world, mover);
    if world.get::<Health>(mover).is_none() && !vehicle {
        alarm_beep(world, gate, now);
        return;
    }
    if world
        .resource::<LuxuryPassengers>()
        .approved
        .contains(&mover)
    {
        // An approved vehicle with a stowaway (`CanAllowThrough`).
        crate::atmos_machinery::machine_say(
            world,
            gate,
            "Обнаружен безбилетник. Пожалуйста, сначала покиньте транспорт.",
        );
        return;
    }
    let driver = if vehicle {
        occupants(world, mover).into_iter().find(|&m| {
            world
                .get::<crate::cars::CarOccupant>(m)
                .is_none_or(crate::cars::CarOccupant::driver)
        })
    } else {
        None
    };
    let payer = driver.unwrap_or(mover);
    let mut paid = world
        .get::<LuxuryGate>(gate)
        .and_then(|g| g.payees.get(&mover).copied())
        .unwrap_or(0);
    if let Some(id) = account_of(world, payer)
        && let Some(mut economy) = world.get_resource_mut::<crate::cargo::economy::Economy>()
        && let Some(account) = economy.account_mut(id)
    {
        let owed = LUXURY_THRESHOLD - paid;
        if account.balance < owed {
            let all = account.balance.max(0);
            account.adjust_money_for(-all, "Scanner Gate: Entry Fee");
            paid += all;
        } else {
            account.adjust_money_for(-owed, "Scanner Gate: Partial Entry Fee");
            paid += owed;
        }
    }
    // Coins, cash and holochips the payer carries.
    let mut money = world.query_filtered::<Entity, With<crate::cargo::money::Money>>();
    let mut carried: Vec<Entity> = money
        .iter(world)
        .filter(|&e| crate::cargo::economy::holder_of(world, e) == Some(payer))
        .collect();
    carried.sort_unstable();
    let mut counted = Vec::new();
    for item in carried {
        if paid >= LUXURY_THRESHOLD {
            break;
        }
        paid += crate::cargo::money::credit_value(world, item);
        counted.push(item);
    }
    if paid >= LUXURY_THRESHOLD {
        for item in counted {
            world.despawn(item);
        }
        let change = paid - LUXURY_THRESHOLD;
        if change > 0 {
            crate::cargo::money::give_holochip(world, payer, change as u32);
        }
        if let Some(mut g) = world.get_mut::<LuxuryGate>(gate) {
            g.payees.remove(&mover);
        }
        let who = name_of(world, payer);
        let tail = if change > 0 {
            " Вот ваша сдача."
        } else {
            ""
        };
        crate::atmos_machinery::machine_say(
            world,
            gate,
            &format!("Добро пожаловать в первый класс, {who}!{tail}"),
        );
        let mut p = world.resource_mut::<LuxuryPassengers>();
        p.approved.insert(mover);
        p.approved.insert(payer);
        p.check_times.remove(&mover);
        info!("luxury shuttle gate {gate}: {mover} approved");
        return;
    }
    if let Some(mut g) = world.get_mut::<LuxuryGate>(gate) {
        g.payees.insert(mover, paid);
    }
    if paid > 0 {
        for item in counted {
            world.despawn(item);
        }
        throttled(
            world,
            payer,
            now,
            format!(
                "Получено {paid}{MONEY_SYMBOL}. Нужно ещё {}{MONEY_SYMBOL}.",
                LUXURY_THRESHOLD - paid
            ),
        );
    }
    alarm_beep(world, gate, now);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn altar_lines_follow_the_altar() {
        assert_eq!(altar_line(ALTAR_LEFT), "Ты спишь, чтобы оно проснулось.");
        assert_eq!(altar_line(ALTAR_RIGHT), "Оно просыпается, чтобы ты спал.");
        assert_eq!(
            altar_line(WABBAJACK_ALTAR),
            "Спишь ты или бодрствуешь — будет весьма хаотично."
        );
    }
}
