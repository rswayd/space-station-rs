//! Station programs (server) on the systems that already exist:
//!
//! - NT Pay (`/datum/computer_file/program/nt_pay`): the ID's bank account, its pay token,
//!   transfers to another account by token, token lookup by holder name, transfer history;
//! - Lifeline and Custodial Locator (`/datum/computer_file/program/radar`): a scan (2 s
//!   cooldown) of crew with suit sensors on tracking / of janitorial equipment, a target from the
//!   scanned list, its position on the radar grid or a rotating arrow past 24 tiles;
//! - SiliConnect (`/datum/computer_file/program/borg_monitor`): the station's cyborgs with cell
//!   charge and location; a message signed with the computer's ID;
//! - BotKeeper (`/datum/computer_file/program/robocontrol`): remote-enabled bots on the
//!   computer's z with mode and location; on/off for whoever the bot allows;
//! - Revelation (`/datum/computer_file/program/revelation`, emagged hub only): armed, it wipes
//!   every file and powers the computer off when run or activated;
//! - maintenance disks (`/obj/item/disk/computer/maintenance/*`): used on a computer they copy
//!   their program once (`PROGRAM_UNIQUE_COPY`); Physical Scanner (`tap`: a health scan of the
//!   mob the PDA is used on) and Spectre-Meter (ghosts within 5 tiles, manual or 1 s automatic
//!   scans).
// shortcut: no circuit components; NT Pay history holds only its own transfers (the economy
// keeps no transaction log); the radar has no overlay icon states or `setDir`; SiliConnect
// shows no model, upgrades or integrity; BotKeeper only toggles power (no summon/patrol orders,
// no per-bot window); Revelation has no obfuscation (filedesc is per kind), no 25 brute or
// battery explosion (no computer integrity or cells) and no silicon purge; Spectre-Meter
// counts only observers (no spirit mobs, hauntium), no geiger loop; disks are used directly
// on the computer (no disk slot or file manager).

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_net::chat::server::SendChat;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};
use ss13_tgui::server::{UiParams, UiReject};

use super::ProgramKind;
use super::station_kinds::StationProgram;
use crate::cargo::economy::{Economy, account_of, bank_card_talk};
use crate::modpc::pda::{ModularComputer, turf_of};
use crate::modpc::station_ui::*;

/// `COOLDOWN_START(src, next_scan, 2 SECONDS)`.
const RADAR_SCAN_COOLDOWN: f64 = 2.0;
/// `get_dist_euclidean > 24`: a rotating arrow instead of the grid.
const RADAR_GRID_RADIUS: f32 = 24.0;
/// `SPOOK_COOLDOWN`, the automatic scan period (`SSprocessing`), the scan range.
const SPOOK_COOLDOWN: f64 = 2.0;
const SPOOK_AUTO_PERIOD: f64 = 1.0;
const SPOOK_RANGE: i32 = 5;
const SPOOK_VALUE_DEF_MOB: f32 = 10.0;
const SPOOK_VALUE_SAME_TURF_MULT: f32 = 1.5;
const SPOOK_PING: &str = "sound/effects/ping_hit.ogg";
/// `MAX_MESSAGE_LEN`.
const MAX_MESSAGE_LEN: usize = 1024;

/// The program state of a computer running these programs, and the screen of the one in front
/// (refreshed every tick by [`refresh`]).
#[derive(Component, Clone, Debug, Default)]
pub struct StationApps {
    pub view: Option<StationScreen>,
    /// `objects` of the last radar scan and `selected`.
    pub radar_objects: Vec<(Entity, String)>,
    pub radar_selected: Option<Entity>,
    pub radar_program: Option<StationProgram>,
    pub next_scan: f64,
    /// The cyborgs or bots listed on the screen (`locate(ref) in list`).
    pub listed: Vec<Entity>,
    /// NT Pay `wanted_token`.
    pub wanted_token: Option<String>,
    /// Revelation `armed`.
    pub armed: bool,
    /// Physical Scanner `last_record`.
    pub last_record: Vec<String>,
    /// Spectre-Meter `last_spook_value`, `auto_mode`, cooldowns.
    pub spook_value: u32,
    pub auto_mode: bool,
    pub spook_ready: f64,
    pub next_auto_scan: f64,
    /// The station program in front at the last refresh: a different one has just started.
    pub front: Option<StationProgram>,
}

/// `transaction_history` of the accounts, by account id, newest last.
#[derive(Resource, Default, Debug)]
pub struct PayHistory(pub HashMap<u32, Vec<PayRecord>>);

/// On a maintenance disk whose program was copied (`PROGRAM_UNIQUE_COPY`).
#[derive(Component, Clone, Copy, Debug)]
pub struct DiskEmpty;

pub fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // A maintenance disk used on a computer copies its program.
    app.add_click_handler(ClickPhase::HeldOnTarget, 5195, |w, c| {
        c.held
            .is_some_and(|h| insert_disk(w, c.client, h, c.target))
    });
    // The same with the PDA in the other hand.
    app.add_click_handler(ClickPhase::OtherHand, 1195, |w, c| {
        c.held
            .is_some_and(|h| insert_disk(w, c.client, h, c.target))
    });
    // `tap`: the Physical Scanner scans the mob the PDA is used on.
    app.add_click_handler(ClickPhase::HeldOnTarget, 5205, |w, c| {
        c.held
            .is_some_and(|h| phys_scan(w, c.client, c.mob, h, c.target))
    });
    app.init_resource::<PayHistory>().add_systems(
        Update,
        refresh
            .after(ss13_tgui::server::UiSystems::Handle)
            .before(ss13_tgui::server::UiSystems::Send)
            .run_if(any_with_component::<ModularComputer>),
    );
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn tell(world: &mut World, client: Entity, span: Span, text: impl Into<String>) {
    world.write_message(SendChat {
        clients: vec![client],
        message: ChatMessage::simple(ChatChannel::System, span, text),
    });
}

/// `REF()` of a listed entity.
pub fn ref_of(entity: Entity) -> String {
    entity.to_bits().to_string()
}

fn find_ref(list: impl IntoIterator<Item = Entity>, r: &str) -> Option<Entity> {
    list.into_iter().find(|&e| ref_of(e) == r)
}

/// `/datum/bank_account/pay_token`: `<first><last letter of the holder>-<account id>` (tg adds
/// a random letter and number; the id keeps it unique and stable here).
pub fn pay_token(id: u32, holder: &str) -> String {
    let first = holder.chars().next().unwrap_or('X');
    let last = holder.chars().last().unwrap_or('X');
    format!("{first}{last}-{id}").to_uppercase()
}

fn active_station(computer: &ModularComputer) -> Option<StationProgram> {
    match computer.active {
        Some(ProgramKind::Station(p)) if computer.enabled => Some(p),
        _ => None,
    }
}

/// The name on the computer's ID (`checkID`).
fn id_name(world: &World, computer: &ModularComputer) -> Option<String> {
    let card = computer.stored_id?;
    world
        .get::<crate::outfit::IdRegistration>(card)
        .map(|r| r.registered_name.clone())
        .filter(|n| !n.is_empty())
}

// ---------------------------------------------------------------------------------------------
// Screens

/// Every computer with a station program in front: its screen, and the per-tick parts of the
/// programs (Revelation run while armed, the Spectre-Meter's automatic scan).
fn refresh(world: &mut World) {
    let mut running = world.query::<(Entity, &ModularComputer, Option<&mut StationApps>)>();
    let mut fronts = Vec::new();
    for (e, c, apps) in running.iter_mut(world) {
        match active_station(c) {
            Some(p) => fronts.push((e, p)),
            None => {
                if let Some(mut apps) = apps.filter(|a| a.front.is_some()) {
                    apps.bypass_change_detection().front = None;
                }
            }
        }
    }
    for (pc, program) in fronts {
        let mut state = world.get::<StationApps>(pc).cloned().unwrap_or_default();
        let started = state.front != Some(program);
        state.front = Some(program);
        if program == StationProgram::Revelation && state.armed && started {
            // `on_start`: an armed Revelation goes off when run.
            activate_revelation(world, pc);
            continue;
        }
        let time = now(world);
        if program == StationProgram::SpectreMeter
            && state.auto_mode
            && time >= state.next_auto_scan
        {
            state.next_auto_scan = time + SPOOK_AUTO_PERIOD;
            state.spook_value = spook_value(world, pc);
        }
        if program != StationProgram::SpectreMeter {
            state.auto_mode = false;
        }
        let radar = matches!(
            program,
            StationProgram::Lifeline | StationProgram::CustodialLocator
        );
        if radar && state.radar_program != Some(program) {
            // `on_start`: a first scan.
            state.radar_selected = None;
            state.radar_objects = radar_scan(world, pc, program);
            state.radar_program = Some(program);
            state.next_scan = time + RADAR_SCAN_COOLDOWN;
        }
        let view = screen(world, pc, program, &mut state);
        state.view = Some(view);
        let changed = world
            .get::<StationApps>(pc)
            .is_none_or(|old| old.view != state.view || old.listed != state.listed);
        if changed || world.get::<StationApps>(pc).is_none() {
            world.entity_mut(pc).insert(state);
        } else if let Some(mut old) = world.get_mut::<StationApps>(pc) {
            // Keep the cooldowns and scans without touching the view.
            old.bypass_change_detection().clone_from(&state);
        }
    }
}

fn screen(
    world: &World,
    pc: Entity,
    program: StationProgram,
    state: &mut StationApps,
) -> StationScreen {
    match program {
        StationProgram::NtPay => StationScreen::Pay(pay_data(world, pc, state)),
        StationProgram::Lifeline | StationProgram::CustodialLocator => {
            if state.radar_program != Some(program) {
                state.radar_objects.clear();
                state.radar_selected = None;
                state.radar_program = Some(program);
            }
            StationScreen::Radar(radar_data(world, pc, program, state))
        }
        StationProgram::BorgMonitor => {
            let rows = borg_rows(world);
            state.listed = rows.iter().map(|(e, _)| *e).collect();
            StationScreen::Borgs(rows.into_iter().map(|(_, r)| r).collect())
        }
        StationProgram::RoboControl => {
            let rows = bot_rows(world, pc);
            state.listed = rows.iter().map(|(e, _)| *e).collect();
            StationScreen::Bots(rows.into_iter().map(|(_, r)| r).collect())
        }
        StationProgram::Revelation => StationScreen::Revelation { armed: state.armed },
        StationProgram::PhysScanner => StationScreen::PhysScanner {
            last_record: state.last_record.clone(),
        },
        StationProgram::SpectreMeter => StationScreen::SpectreMeter {
            spook_value: state.spook_value,
            auto_mode: state.auto_mode,
            on_cooldown: now(world) < state.spook_ready,
        },
    }
}

fn pay_data(world: &World, pc: Entity, state: &StationApps) -> PayData {
    let Some(id) = account_of(world, pc) else {
        return PayData::default();
    };
    let Some(account) = world.get_resource::<Economy>().and_then(|e| e.account(id)) else {
        return PayData::default();
    };
    let transactions = world
        .get_resource::<PayHistory>()
        .and_then(|h| h.0.get(&id))
        .map(|h| h.iter().rev().cloned().collect())
        .unwrap_or_default();
    PayData {
        name: Some(account.holder.clone()),
        owner_token: pay_token(id, &account.holder),
        money: account.balance,
        wanted_token: state.wanted_token.clone(),
        transactions,
    }
}

/// `is_valid_z_level`: here the same z.
fn radar_trackable(world: &World, pc: Entity, program: StationProgram, target: Entity) -> bool {
    let (Some(here), Some(there)) = (turf_of(world, pc), turf_of(world, target)) else {
        return false;
    };
    if here.z != there.z {
        return false;
    }
    match program {
        // Suit sensors must be on maximum.
        StationProgram::Lifeline => crate::clothing::sensors::trackable(world, target, false),
        _ => true,
    }
}

/// `track()`.
pub fn track(here: TilePos, there: TilePos) -> RadarTarget {
    let (dx, dy) = (there.x - here.x, there.y - here.y);
    let distance = ((dx * dx + dy * dy) as f32).sqrt();
    let userot = distance > RADAR_GRID_RADIUS;
    // `get_angle`: clockwise from north.
    let rot = if userot {
        let deg = (dx as f32).atan2(dy as f32).to_degrees();
        (deg.rem_euclid(360.0)).round() as i32
    } else {
        0
    };
    let pointer = if there.z > here.z {
        "caret-up"
    } else if there.z < here.z {
        "caret-down"
    } else {
        "crosshairs"
    };
    RadarTarget {
        locx: dx + 24,
        locy: -dy + 24,
        userot,
        rot,
        pointer: pointer.to_owned(),
        distance,
    }
}

fn radar_data(
    world: &World,
    pc: Entity,
    program: StationProgram,
    state: &StationApps,
) -> RadarData {
    let target = state
        .radar_selected
        .filter(|&t| world.get_entity(t).is_ok() && radar_trackable(world, pc, program, t))
        .and_then(|t| Some(track(turf_of(world, pc)?, turf_of(world, t)?)));
    RadarData {
        selected: state.radar_selected.map(ref_of),
        scanning: now(world) < state.next_scan,
        objects: state
            .radar_objects
            .iter()
            .map(|(e, name)| RadarObject {
                object_ref: ref_of(*e),
                name: name.clone(),
            })
            .collect(),
        target,
    }
}

fn under(path: &str, root: &str) -> bool {
    ss13_mapload::fidelity::is_under(path, root)
}

/// `scan()` of Lifeline (`GLOB.human_list`) or the Custodial Locator (`GLOB.janitor_devices`).
pub fn radar_scan(world: &mut World, pc: Entity, program: StationProgram) -> Vec<(Entity, String)> {
    let mut found = Vec::new();
    match program {
        StationProgram::Lifeline => {
            let mut mobs = world.query::<(Entity, &crate::inventory::Inventory)>();
            let mobs: Vec<(Entity, Option<Entity>)> =
                mobs.iter(world).map(|(e, i)| (e, i.id)).collect();
            for (mob, id) in mobs {
                if !radar_trackable(world, pc, program, mob) {
                    continue;
                }
                let name = id
                    .and_then(|id| crate::inventory::get_id(world, id))
                    .and_then(|card| world.get::<crate::outfit::IdRegistration>(card))
                    .map(|r| r.registered_name.clone())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| "Unknown".to_owned());
                found.push((mob, name));
            }
        }
        _ => {
            let mut items = world.query::<(Entity, &ss13_mapload::DmItem)>();
            let mut tools: Vec<Entity> = items
                .iter(world)
                .filter(|(_, i)| {
                    under(&i.path, "/obj/item/mop") || under(&i.path, "/obj/item/key/janitor")
                })
                .map(|(e, _)| e)
                .collect();
            let mut objs = world.query::<(Entity, &ss13_mapload::construct::DmObj)>();
            tools.extend(
                objs.iter(world)
                    .filter(|(_, o)| {
                        under(&o.path, "/obj/structure/mop_bucket/janitorialcart")
                            || under(&o.path, "/obj/vehicle/ridden/janicart")
                    })
                    .map(|(e, _)| e),
            );
            let mut bots = world.query::<(Entity, &crate::bots::Bot)>();
            let cleanbots: Vec<(Entity, bool)> = bots
                .iter(world)
                .filter(|(_, b)| b.kind == crate::bots::BotKind::Clean)
                .map(|(e, b)| (e, b.on()))
                .collect();
            for tool in tools {
                if radar_trackable(world, pc, program, tool) {
                    found.push((tool, crate::examine::name_of(world, tool)));
                }
            }
            for (bot, on) in cleanbots {
                if radar_trackable(world, pc, program, bot) {
                    let status = if on { "Online" } else { "Offline" };
                    found.push((
                        bot,
                        format!("{} - {status}", crate::examine::name_of(world, bot)),
                    ));
                }
            }
        }
    }
    found
}

fn borg_rows(world: &World) -> Vec<(Entity, BorgRow)> {
    let Some(mut borgs) = world.try_query::<(Entity, &crate::silicon::cyborg::Cyborg)>() else {
        return Vec::new();
    };
    let list: Vec<(Entity, f64, f64, bool)> = borgs
        .iter(world)
        .map(|(e, c)| (e, c.charge, c.max_charge, c.low_power))
        .collect();
    list.into_iter()
        .map(|(e, charge, max, low_power)| {
            let area = turf_of(world, e)
                .map(|p| crate::bots::area_name(world, p))
                .unwrap_or_default();
            let row = BorgRow {
                name: crate::examine::name_of(world, e),
                borg_ref: ref_of(e),
                charge: (max > 0.0).then(|| (charge / max * 100.0).clamp(0.0, 100.0).round() as u8),
                low_power,
                area,
            };
            (e, row)
        })
        .collect()
}

fn bot_rows(world: &World, pc: Entity) -> Vec<(Entity, BotRow)> {
    let Some(here) = turf_of(world, pc) else {
        return Vec::new();
    };
    let Some(mut bots) = world.try_query::<(Entity, &crate::bots::Bot)>() else {
        return Vec::new();
    };
    let list: Vec<(Entity, String, bool)> = bots
        .iter(world)
        .filter(|(_, b)| b.mode_flags & crate::bots::MODE_REMOTE_ENABLED != 0)
        .map(|(e, b)| (e, b.mode.to_owned(), b.on()))
        .collect();
    list.into_iter()
        .filter_map(|(e, mode, on)| {
            let at = turf_of(world, e)?;
            (at.z == here.z).then(|| {
                let row = BotRow {
                    name: crate::bots::bot_name(world, e),
                    bot_ref: ref_of(e),
                    mode,
                    on,
                    area: crate::bots::area_name(world, at),
                };
                (e, row)
            })
        })
        .collect()
}

/// `scan_surroundings`: observers within 5 tiles, closer and same-tile ones weigh more.
pub fn spook_value(world: &mut World, pc: Entity) -> u32 {
    let Some(here) = turf_of(world, pc) else {
        return 0;
    };
    let mut ghosts = world.query_filtered::<&TilePos, With<crate::ghost::Ghost>>();
    let total: f32 = ghosts
        .iter(world)
        .filter(|p| p.z == here.z)
        .filter_map(|p| {
            let dist = (p.x - here.x).abs().max((p.y - here.y).abs());
            (dist <= SPOOK_RANGE).then(|| {
                let mut amount = SPOOK_VALUE_DEF_MOB;
                if dist == 0 {
                    amount *= SPOOK_VALUE_SAME_TURF_MULT;
                }
                amount / dist.max(1) as f32
            })
        })
        .sum();
    total.round() as u32
}

// ---------------------------------------------------------------------------------------------
// Acts

#[derive(Clone, Debug, PartialEq)]
pub enum StationAct {
    Pay { token: String, amount: f64 },
    GetToken(String),
    RadarSelect(String),
    RadarScan,
    BorgMessage { borg: String, message: String },
    BotPower { bot: String, on: bool },
    Arm,
    Activate,
    SpectreScan,
    SpectreMode,
}

impl StationAct {
    fn programs(&self) -> &'static [StationProgram] {
        use StationProgram::*;
        match self {
            Self::Pay { .. } | Self::GetToken(_) => &[NtPay],
            Self::RadarSelect(_) | Self::RadarScan => &[Lifeline, CustodialLocator],
            Self::BorgMessage { .. } => &[BorgMonitor],
            Self::BotPower { .. } => &[RoboControl],
            Self::Arm | Self::Activate => &[Revelation],
            Self::SpectreScan | Self::SpectreMode => &[SpectreMeter],
        }
    }
}

/// `None` for an action that is not one of these programs'.
pub fn parse(action: &str, params: &UiParams) -> Option<Result<StationAct, UiReject>> {
    let text = |key: &str| params.get(key).unwrap_or_default().to_owned();
    Some(match action {
        // tgui sends `token` and `amount`; this port's screen sends both as `transfer`.
        PAY_TRANSACTION => match params.get("transfer") {
            Some(transfer) => {
                let mut words = transfer.split_whitespace();
                match (
                    words.next(),
                    words.next().and_then(|a| a.parse::<f64>().ok()),
                ) {
                    (Some(token), Some(amount)) if amount.is_finite() => Ok(StationAct::Pay {
                        token: token.to_owned(),
                        amount,
                    }),
                    _ => Err(UiReject::BadParams("transfer: TOKEN AMOUNT".into())),
                }
            }
            None => params.number("amount").map(|amount| StationAct::Pay {
                token: text("token"),
                amount,
            }),
        },
        PAY_GET_TOKEN => Ok(StationAct::GetToken(text("wanted_name"))),
        RADAR_SELECT => Ok(StationAct::RadarSelect(text("ref"))),
        RADAR_SCAN => Ok(StationAct::RadarScan),
        BORG_MESSAGE => Ok(StationAct::BorgMessage {
            borg: text("ref"),
            message: text("message"),
        }),
        BOT_INTERACT => match params.get("interaction") {
            Some("on") => Ok(StationAct::BotPower {
                bot: text("ref"),
                on: true,
            }),
            Some("off") => Ok(StationAct::BotPower {
                bot: text("ref"),
                on: false,
            }),
            _ => Err(UiReject::BadParams("interaction: on or off".into())),
        },
        REVELATION_ARM => Ok(StationAct::Arm),
        REVELATION_ACTIVATE => Ok(StationAct::Activate),
        SPECTRE_SCAN => Ok(StationAct::SpectreScan),
        SPECTRE_MODE => Ok(StationAct::SpectreMode),
        _ => return None,
    })
}

/// `ui_act` of the station program in front of `pc`.
pub fn apply(
    world: &mut World,
    client: Entity,
    actor: Entity,
    pc: Entity,
    computer: &ModularComputer,
    act: StationAct,
) -> Result<(), &'static str> {
    let Some(program) = active_station(computer) else {
        return Err("no station program in front");
    };
    if !act.programs().contains(&program) {
        return Err("program not in front");
    }
    let mut state = world.get::<StationApps>(pc).cloned().unwrap_or_default();
    let result = match act {
        StationAct::Pay { token, amount } => pay(world, client, pc, &token, amount),
        StationAct::GetToken(name) => {
            let economy = world.get_resource::<Economy>();
            state.wanted_token = Some(
                economy
                    .and_then(|e| e.accounts().find(|(_, a)| a.holder == name))
                    .map_or_else(
                        || format!("Account \"{name}\" not found."),
                        |(id, a)| format!("Token: {}", pay_token(id, &a.holder)),
                    ),
            );
            Ok(())
        }
        StationAct::RadarSelect(r) => {
            // `if(selected_new_ref in trackable_object_refs())`.
            state.radar_selected =
                find_ref(state.radar_objects.iter().map(|(e, _)| *e), &r).or(state.radar_selected);
            Ok(())
        }
        StationAct::RadarScan => {
            let time = now(world);
            if time >= state.next_scan {
                state.next_scan = time + RADAR_SCAN_COOLDOWN;
                state.radar_objects = radar_scan(world, pc, program);
                state.radar_program = Some(program);
            }
            Ok(())
        }
        StationAct::BorgMessage { borg, message } => {
            message_borg(world, client, actor, computer, &state, &borg, &message)
        }
        StationAct::BotPower { bot, on } => {
            let Some(bot) = find_ref(state.listed.iter().copied(), &bot) else {
                return Err("not a listed bot");
            };
            if !crate::bots::allowed(world, bot, actor) {
                tell(world, client, Span::Warning, "Access denied.");
                return Err("access denied");
            }
            if on {
                crate::bots::turn_on(world, bot);
            } else {
                crate::bots::turn_off(world, bot);
            }
            Ok(())
        }
        StationAct::Arm => {
            state.armed = !state.armed;
            Ok(())
        }
        StationAct::Activate => {
            activate_revelation(world, pc);
            return Ok(());
        }
        StationAct::SpectreScan => {
            let time = now(world);
            if time < state.spook_ready {
                return Err("cooling down");
            }
            state.spook_value = spook_value(world, pc);
            state.spook_ready = time + SPOOK_COOLDOWN;
            if let Some(pos) = turf_of(world, pc) {
                crate::sound::playsound(
                    world,
                    crate::sound::PlaySound::new(pos, SPOOK_PING, 40.0, true),
                );
            }
            Ok(())
        }
        StationAct::SpectreMode => {
            state.auto_mode = !state.auto_mode;
            Ok(())
        }
    };
    if world.get_entity(pc).is_ok() {
        world.entity_mut(pc).insert(state);
    }
    result
}

/// `_pay`.
fn pay(
    world: &mut World,
    client: Entity,
    pc: Entity,
    token: &str,
    amount: f64,
) -> Result<(), &'static str> {
    let Some(from) = account_of(world, pc) else {
        return Err("no account");
    };
    let amount = amount.round() as i64;
    let token = token.trim().to_uppercase();
    if token.is_empty() {
        tell(
            world,
            client,
            Span::Notice,
            "You need to enter your transfer target's pay token.",
        );
        return Err("no token");
    }
    if amount <= 0 {
        tell(
            world,
            client,
            Span::Notice,
            "You need to specify how much you're sending.",
        );
        return Err("no amount");
    }
    let Some(economy) = world.get_resource::<Economy>() else {
        return Err("no economy");
    };
    let Some(sender) = economy.account(from).cloned() else {
        return Err("no account");
    };
    if token == pay_token(from, &sender.holder) {
        tell(
            world,
            client,
            Span::Notice,
            "You can't send credits to yourself.",
        );
        return Err("sender is receiver");
    }
    let Some((to, recipient)) = economy
        .accounts()
        .find(|(id, a)| pay_token(*id, &a.holder) == token)
        .map(|(id, a)| (id, a.holder.clone()))
    else {
        tell(
            world,
            client,
            Span::Notice,
            "The app can't find who you're trying to pay. Did you enter the pay token right?",
        );
        return Err("invalid token");
    };
    if !sender.has_money(amount) {
        bank_card_talk(world, from, "You cannot afford it.");
        return Err("cannot afford");
    }
    let mut economy = world.resource_mut::<Economy>();
    let withdrawn = economy
        .account_mut(from)
        .is_some_and(|a| a.adjust_money(-amount));
    if !withdrawn {
        return Err("cannot afford");
    }
    if let Some(a) = economy.account_mut(to) {
        a.adjust_money(amount);
    }
    let balance = economy.account(from).map_or(0, |a| a.balance);
    let mut history = world.resource_mut::<PayHistory>();
    history.0.entry(to).or_default().push(PayRecord {
        adjusted_money: amount,
        reason: format!("Transfer from {}", sender.holder),
    });
    history.0.entry(from).or_default().push(PayRecord {
        adjusted_money: -amount,
        reason: format!("Transfer to {recipient}"),
    });
    let money = crate::cargo::MONEY_SYMBOL;
    bank_card_talk(
        world,
        to,
        &format!(
            "You received {amount}{money}. Reason: transfer from {}",
            sender.holder
        ),
    );
    bank_card_talk(
        world,
        from,
        &format!("You send {amount}{money} to {recipient}. Now you have {balance}{money}"),
    );
    Ok(())
}

/// `message_robot` + `send_message`.
fn message_borg(
    world: &mut World,
    client: Entity,
    actor: Entity,
    computer: &ModularComputer,
    state: &StationApps,
    borg: &str,
    message: &str,
) -> Result<(), &'static str> {
    let Some(borg) = find_ref(state.listed.iter().copied(), borg) else {
        return Err("not a listed cyborg");
    };
    let Some(id) = id_name(world, computer) else {
        return Err("no ID");
    };
    if world.get::<crate::health::Stat>(borg) == Some(&crate::health::Stat::Dead) {
        tell(
            world,
            client,
            Span::Warning,
            format!(
                "Error -- Could not open a connection to unit:{}",
                crate::examine::name_of(world, borg)
            ),
        );
        return Err("dead cyborg");
    }
    let message: String = message.trim().chars().take(MAX_MESSAGE_LEN).collect();
    if message.is_empty() {
        return Err("empty message");
    }
    if let Some(borg_client) = world
        .get::<crate::server::PlayerMob>(borg)
        .map(|p| p.client)
    {
        tell(
            world,
            borg_client,
            Span::Notice,
            format!("Message from {id} -- \"{message}\""),
        );
    }
    tell(
        world,
        client,
        Span::Notice,
        format!(
            "Message sent to {}: {message}",
            crate::examine::name_of(world, borg)
        ),
    );
    info!("{actor} sent a message to the cyborg {borg}: {message}");
    Ok(())
}

/// `revelation/activate`: every file wiped, the computer off.
pub fn activate_revelation(world: &mut World, pc: Entity) {
    let Some(mut computer) = world.get_mut::<ModularComputer>(pc) else {
        return;
    };
    computer.shutdown();
    computer.programs.clear();
    let name = computer.display_name();
    world.entity_mut(pc).remove::<StationApps>();
    let around = crate::modpc::pda::holder_mob(world, pc)
        .and_then(|m| world.get::<crate::server::PlayerMob>(m).map(|p| p.client));
    if let Some(client) = around {
        tell(
            world,
            client,
            Span::Notice,
            format!("Экран {name} ярко вспыхивает, слышно громкое электрическое жужжание."),
        );
    }
    info!("Revelation went off on {pc}");
}

/// A maintenance disk used on a computer: its program is copied once.
pub fn insert_disk(world: &mut World, client: Entity, disk: Entity, pc: Entity) -> bool {
    let Some(program) = world
        .get::<ss13_mapload::DmItem>(disk)
        .and_then(|i| StationProgram::from_maintenance_disk(&i.path))
    else {
        return false;
    };
    let Some(computer) = world.get::<ModularComputer>(pc) else {
        return false;
    };
    if world.get::<DiskEmpty>(disk).is_some() {
        tell(world, client, Span::Warning, "Диск пуст.");
        return true;
    }
    let kind = ProgramKind::Station(program);
    if !kind.runs_on(computer.is_pda()) {
        tell(
            world,
            client,
            Span::Warning,
            "Программа несовместима с устройством.",
        );
        return true;
    }
    if !super::downloader::can_store(computer, kind) {
        tell(
            world,
            client,
            Span::Warning,
            "Не удалось скопировать программу: она уже установлена или не хватает места.",
        );
        return true;
    }
    world
        .get_mut::<ModularComputer>(pc)
        .expect("checked above")
        .programs
        .push(kind);
    world.entity_mut(disk).insert(DiskEmpty);
    tell(
        world,
        client,
        Span::Notice,
        format!("Программа {} скопирована с диска.", program.filedesc()),
    );
    true
}

/// `phys_scanner/tap`: the PDA, its Physical Scanner in front, used on a mob.
pub fn phys_scan(
    world: &mut World,
    client: Entity,
    user: Entity,
    pda: Entity,
    target: Entity,
) -> bool {
    let running = world
        .get::<ModularComputer>(pda)
        .is_some_and(|c| active_station(c) == Some(StationProgram::PhysScanner));
    if !running || world.get::<crate::inventory::Inventory>(target).is_none() {
        return false;
    }
    let Some(scan) = crate::health_scan::healthscan(world, target) else {
        return false;
    };
    let record: Vec<String> = scan.plain_text().lines().map(str::to_owned).collect();
    let mut state = world.get::<StationApps>(pda).cloned().unwrap_or_default();
    state.last_record = record;
    world.entity_mut(pda).insert(state);
    tell(
        world,
        client,
        Span::Notice,
        format!(
            "{} анализирует жизненные показатели {}.",
            crate::examine::name_of(world, user),
            crate::examine::name_of(world, target)
        ),
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(x: i32, y: i32, z: u16) -> TilePos {
        TilePos { x, y, z }
    }

    #[test]
    fn radar_tracks_on_the_grid_then_with_an_arrow() {
        let near = track(pos(10, 10, 1), pos(13, 6, 1));
        assert_eq!((near.locx, near.locy), (27, 28));
        assert!(!near.userot);
        assert_eq!(near.pointer, "crosshairs");
        // 30 tiles due east: an arrow at 90 degrees.
        let far = track(pos(0, 0, 1), pos(30, 0, 1));
        assert!(far.userot);
        assert_eq!(far.rot, 90);
        assert_eq!(track(pos(0, 0, 1), pos(0, 0, 2)).pointer, "caret-up");
    }

    #[test]
    fn pay_tokens_are_unique_per_account() {
        assert_eq!(pay_token(123_456, "John Doe"), "JE-123456");
        assert_ne!(pay_token(1, "Ann"), pay_token(2, "Ann"));
    }

    #[test]
    fn acts_parse_hostile_params() {
        let parse_with = |action: &str, params: &[(&str, &str)]| {
            let params: Vec<(String, String)> = params
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect();
            parse(action, &UiParams::new(&params))
        };
        assert!(matches!(
            parse_with(PAY_TRANSACTION, &[("token", "X"), ("amount", "NaN")]),
            Some(Err(_))
        ));
        assert_eq!(
            parse_with(PAY_TRANSACTION, &[("transfer", "je-123456 50")]),
            Some(Ok(StationAct::Pay {
                token: "je-123456".into(),
                amount: 50.0
            }))
        );
        assert!(matches!(
            parse_with(PAY_TRANSACTION, &[("transfer", "JE-1 inf")]),
            Some(Err(_))
        ));
        assert!(matches!(
            parse_with(BOT_INTERACT, &[("ref", "1"), ("interaction", "summon")]),
            Some(Err(_))
        ));
        assert_eq!(
            parse_with(BOT_INTERACT, &[("ref", "1"), ("interaction", "off")]),
            Some(Ok(StationAct::BotPower {
                bot: "1".into(),
                on: false
            }))
        );
        assert_eq!(parse_with("PC_exit", &[]), None);
    }

    /// A world with the economy and a PDA running `program`.
    fn world_with_pda(program: StationProgram) -> (World, Entity) {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.insert_resource(Economy::new(1));
        world.init_resource::<PayHistory>();
        let mut computer = ModularComputer::pda("/obj/item/modular_computer/pda/crew", "PDA");
        computer.enabled = true;
        computer.programs.push(ProgramKind::Station(program));
        computer.active = Some(ProgramKind::Station(program));
        let pc = world.spawn((computer, TilePos { x: 5, y: 5, z: 1 })).id();
        (world, pc)
    }

    #[test]
    fn nt_pay_transfers_by_token() {
        use crate::cargo::economy::BankAccountId;
        let (mut world, pc) = world_with_pda(StationProgram::NtPay);
        let client = world.spawn_empty().id();
        let (from, to) = {
            let mut economy = world.resource_mut::<Economy>();
            let from = economy.open_account("Alice Smith", 10, "civilian");
            let to = economy.open_account("Bob Jones", 10, "civilian");
            economy.account_mut(from).unwrap().balance = 100;
            economy.account_mut(to).unwrap().balance = 0;
            (from, to)
        };
        let card = world
            .spawn((
                crate::inventory::IdCard { access: Vec::new() },
                BankAccountId(from),
            ))
            .id();
        world.get_mut::<ModularComputer>(pc).unwrap().stored_id = Some(card);
        let token = pay_token(to, "Bob Jones");
        // More than the balance, to oneself, a bad token: nothing moves.
        assert!(pay(&mut world, client, pc, &token, 500.0).is_err());
        assert!(pay(&mut world, client, pc, &pay_token(from, "Alice Smith"), 5.0).is_err());
        assert!(pay(&mut world, client, pc, "NO-1", 5.0).is_err());
        assert!(pay(&mut world, client, pc, &token, -5.0).is_err());
        assert!(pay(&mut world, client, pc, &token.to_lowercase(), 40.4).is_ok());
        let economy = world.resource::<Economy>();
        assert_eq!(economy.account(from).unwrap().balance, 60);
        assert_eq!(economy.account(to).unwrap().balance, 40);
        let data = pay_data(&world, pc, &StationApps::default());
        assert_eq!(data.name.as_deref(), Some("Alice Smith"));
        assert_eq!(data.transactions[0].adjusted_money, -40);
    }

    #[test]
    fn armed_revelation_wipes_the_computer() {
        let (mut world, pc) = world_with_pda(StationProgram::Revelation);
        let client = world.spawn_empty().id();
        let actor = world.spawn_empty().id();
        let computer = world.get::<ModularComputer>(pc).unwrap().clone();
        apply(&mut world, client, actor, pc, &computer, StationAct::Arm).unwrap();
        assert!(world.get::<StationApps>(pc).unwrap().armed);
        // Armed while in front: nothing until it is run again.
        world.get_mut::<StationApps>(pc).unwrap().front = Some(StationProgram::Revelation);
        refresh(&mut world);
        assert!(world.get::<ModularComputer>(pc).unwrap().enabled);
        // Not its acts while another program is in front.
        let mut other = computer.clone();
        other.active = Some(ProgramKind::Notepad);
        assert!(apply(&mut world, client, actor, pc, &other, StationAct::Activate).is_err());
        apply(
            &mut world,
            client,
            actor,
            pc,
            &computer,
            StationAct::Activate,
        )
        .unwrap();
        let computer = world.get::<ModularComputer>(pc).unwrap();
        assert!(!computer.enabled);
        assert!(computer.programs.is_empty());
    }

    #[test]
    fn spectre_meter_weighs_ghosts_by_distance() {
        let (mut world, pc) = world_with_pda(StationProgram::SpectreMeter);
        assert_eq!(spook_value(&mut world, pc), 0);
        world.spawn((
            crate::ghost::Ghost { body: None },
            TilePos { x: 5, y: 5, z: 1 },
        ));
        world.spawn((
            crate::ghost::Ghost { body: None },
            TilePos { x: 7, y: 5, z: 1 },
        ));
        // Out of range and another z: ignored.
        world.spawn((
            crate::ghost::Ghost { body: None },
            TilePos { x: 20, y: 5, z: 1 },
        ));
        world.spawn((
            crate::ghost::Ghost { body: None },
            TilePos { x: 5, y: 5, z: 2 },
        ));
        assert_eq!(spook_value(&mut world, pc), 20);
    }

    #[test]
    fn a_maintenance_disk_copies_its_program_once() {
        let (mut world, pc) = world_with_pda(StationProgram::NtPay);
        let client = world.spawn_empty().id();
        let disk = world
            .spawn(ss13_mapload::DmItem {
                path: "/obj/item/disk/computer/maintenance/scanner".into(),
                ..Default::default()
            })
            .id();
        assert!(insert_disk(&mut world, client, disk, pc));
        let scanner = ProgramKind::Station(StationProgram::PhysScanner);
        assert!(
            world
                .get::<ModularComputer>(pc)
                .unwrap()
                .programs
                .contains(&scanner)
        );
        assert!(world.get::<DiskEmpty>(disk).is_some());
        world
            .get_mut::<ModularComputer>(pc)
            .unwrap()
            .programs
            .retain(|&p| p != scanner);
        assert!(insert_disk(&mut world, client, disk, pc));
        assert!(
            !world
                .get::<ModularComputer>(pc)
                .unwrap()
                .programs
                .contains(&scanner)
        );
    }

    #[test]
    fn maintenance_disks_carry_their_programs() {
        assert_eq!(
            StationProgram::from_maintenance_disk("/obj/item/disk/computer/maintenance/scanner"),
            Some(StationProgram::PhysScanner)
        );
        assert!(!StationProgram::PhysScanner.on_ntnet_store());
        assert!(StationProgram::Revelation.on_syndinet_store());
        assert!(!StationProgram::Lifeline.runs_on(false));
    }
}
