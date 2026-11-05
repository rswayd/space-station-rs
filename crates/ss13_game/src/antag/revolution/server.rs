//! The revolution on the server.
//!
//! - Head revolutionaries come from dynamic (`Roundstart Revolution`: dormant for 7 minutes,
//!   `Latejoin Revolution`: 1 minute, [`make_dormant`]) or admins ([`add_revolutionary`]). On
//!   reveal ([`reveal_head`]) a head gets a flash in a pocket (`equip_conspicuous_item`) and the
//!   revolution starts ([`start_revolution`]): every head of staff becomes a mutiny target (a
//!   latejoining head too, [`on_latejoin`]).
//! - A head revolutionary's successful targeted flash (`COMSIG_MOB_SUCCESSFUL_FLASHED_MOB`,
//!   [`on_successful_flash`]) converts a conscious crew body with a client that is no head of
//!   staff and has no mindshield: it is flashed again and stunned for 10 s. A head can flash from
//!   behind (`can_flash_from_behind`). Each conversion checks BandaStation's revolution size
//!   (`check_size`): at half of the living crew (40% at 70 and more) the station is told.
//! - Deconversion (`remove_revolutionary`): a mindshield implant (`crate::implants`,
//!   [`on_mindshield_implant`]),
//!   blunt brute force on the head ([`head_trauma`]), or the station's victory. A head
//!   revolutionary resists the implant and blunt force.
//! - Outcome ([`RevolutionHandler`] `process`): every mutiny target dead or off the station
//!   levels: the revolution wins (BandaStation: an announcement, the round goes on, no round end
//!   as in its `check_finished` edit); every head revolutionary dead or off the station: the
//!   station wins, every revolutionary is deconverted.
// shortcut: no mood, memories, codewords, conversion objective, clumsiness, holiday memes, AFK
// grace for mutiny targets, hostile environment for the shuttle, syndicate security HUD implant
// for heads, exiled heads (`enemy_of_the_state`) or borging. tg `update_rev_heads` never promotes anyone (its
// `can_be_headrev` refuses antagonists and every candidate is a revolutionary), so it is left out.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_net::server::ClientToken;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::{AntagInfoRevolutionUi, ROLE_PROVOCATEUR, ROLE_REV_HEAD};
use crate::antag::brother::MINDSHIELD_JOBS;
use crate::antag::dynamic::AntagPreferences;
use crate::antag::objectives::{RoundEnd, considered_alive};
use crate::antag::traitor::print_player;
use crate::antag::{AntagHudData, AntagHudUi, BrotherInfo, ObjectiveInfo};
use crate::health::{Health, Stat};
use crate::inventory::Inventory;
use crate::jobs::{Jobs, PlayerJob};
use crate::medical::{name_of, visible};
use crate::server::PlayerMob;
use crate::status_effects::Incap;
use crate::tools::tell;

/// `/datum/antagonist/rev` `name`.
pub const REV_NAME: &str = "Революционер";
/// `/datum/antagonist/rev/head` `name`.
pub const HEAD_NAME: &str = "Глава Революционеров";
const ANTAG_HUD_DMI: &str = "icons/mob/huds/antag_hud.dmi";
/// `equip_conspicuous_item(new /obj/item/assembly/flash/handheld)`.
pub const REV_FLASH: &str = "/obj/item/assembly/flash/handheld";
/// The preloaded `/obj/item/implanter/mindshield` (`crate::implants`).
pub const MINDSHIELD_IMPLANTER: &str = "/obj/item/implanter/mindshield";
/// `/datum/dynamic_ruleset/roundstart/revolution/assign_role`: `addtimer(reveal_head, 7 MINUTES)`.
pub const ROUNDSTART_REVEAL_SECS: f64 = 7.0 * 60.0;
/// `/datum/dynamic_ruleset/latejoin/revolution/assign_role`: 1 minute.
pub const LATEJOIN_REVEAL_SECS: f64 = 60.0;
/// The roundstart ruleset's `heads_necessary`.
pub const ROUNDSTART_HEADS_NECESSARY: usize = 2;
/// The latejoin ruleset's `heads_necessary` (it reveals with one fewer, "a little leeway").
pub const LATEJOIN_HEADS_NECESSARY: usize = 3;
/// `add_revolutionary(stun = TRUE)`: `Stun(10 SECONDS)`; `remove_revolutionary`:
/// `Unconscious(10 SECONDS)`.
const CONVERT_STUN_SECS: f32 = 10.0;
/// `add_revolutionary(mute = TRUE)`: `set_silence_if_lower(10 SECONDS)`.
const CONVERT_SILENCE_SECS: f32 = 10.0;
const DECONVERT_SECS: f32 = 10.0;
/// BandaStation `REV_HIGHPOP_THRESHOLD`, `REV_ASCENDENT_LOWPOP`, `REV_ASCENDENT_HIGHPOP`.
const REV_HIGHPOP_THRESHOLD: u32 = 70;
const REV_ASCENDENT_LOWPOP: f32 = 0.5;
const REV_ASCENDENT_HIGHPOP: f32 = 0.4;

/// The jobs with `HEAD_OF_STAFF_JOB_FLAGS` in BandaStation (`SSjob.get_all_heads`).
pub const HEAD_JOBS: [&str; 8] = [
    "Captain",
    "Head of Personnel",
    "Head of Security",
    "Chief Engineer",
    "Research Director",
    "Chief Medical Officer",
    "Quartermaster",
    "Nanotrasen Representative",
];

/// A revolutionary antagonist datum on its body.
#[derive(Component, Clone, Debug)]
pub struct Revolutionary {
    /// `/datum/antagonist/rev/head`.
    pub head: bool,
    pub key: String,
}

/// `TRAIT_MINDSHIELD` + `TRAIT_UNCONVERTABLE` from an implanter's mindshield implant.
#[derive(Component, Clone, Copy, Debug)]
pub struct Mindshield;

/// tg "Dormant Head Revolutionary": a selected head revolutionary waiting for `reveal_head`.
#[derive(Component, Clone, Copy, Debug)]
pub struct DormantHeadRev {
    pub reveal_at: f64,
    pub latejoin: bool,
}

/// The panel's content.
#[derive(Component, Clone, Debug)]
pub struct RevInfo(pub BrotherInfo);

/// `GLOB.revolution_handler.result`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevResult {
    /// `REVOLUTION_VICTORY`.
    Revolution,
    /// `STATION_VICTORY`.
    Station,
}

/// Who deconverted a revolutionary (`deconversion_source`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deconverter {
    /// A mob: the implanter's user, the one who hit the head.
    Mob(Entity),
    /// `DECONVERTER_STATION_WIN`.
    StationWin,
    /// An admin removal (no source).
    Admin,
}

/// `GLOB.revolution_handler` with its `/datum/team/revolution`.
#[derive(Resource, Debug, Default)]
pub struct RevolutionHandler {
    /// Bodies in the team, in joining order.
    pub members: Vec<Entity>,
    /// `/datum/objective/mutiny` targets: the heads of staff.
    pub targets: Vec<Entity>,
    /// `ex_headrevs`, `ex_revs` with their keys (`save_members`).
    pub ex_headrevs: Vec<(Entity, String)>,
    pub ex_revs: Vec<(Entity, String)>,
    pub result: Option<RevResult>,
    /// `start_revolution` ran (`DF_ISPROCESSING`).
    pub started: bool,
    /// BandaStation `rev_ascendent`.
    pub ascendent: bool,
}

pub struct RevolutionPlugin;

impl Plugin for RevolutionPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<AntagInfoRevolutionUi>()
            .add_systems(Update, open_info.in_set(UiSystems::Handle))
            .add_systems(
                Update,
                (
                    reveal_dormant.run_if(any_with_component::<DormantHeadRev>),
                    process.run_if(resource_exists::<RevolutionHandler>),
                )
                    .chain()
                    .before(UiSystems::Open),
            );
    }
}

// ---------------------------------------------------------------------------------------------
// Queries

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn client_of(world: &World, mob: Entity) -> Option<Entity> {
    world.get::<PlayerMob>(mob).map(|p| p.client)
}

fn chat_to(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = client_of(world, mob) {
        tell(world, client, span, text);
    }
}

fn job_title(world: &World, mob: Entity) -> Option<String> {
    let job = world.get::<PlayerJob>(mob)?.0;
    Some(
        world
            .get_resource::<Jobs>()?
            .data
            .jobs
            .get(job)?
            .title
            .clone(),
    )
}

/// `assigned_role.job_flags & JOB_HEAD_OF_STAFF`.
pub fn is_head_of_staff(world: &World, mob: Entity) -> bool {
    job_title(world, mob).is_some_and(|t| HEAD_JOBS.contains(&t.as_str()))
}

/// `TRAIT_MINDSHIELD`: an implanted mindshield, or the job's outfit's (`implants`).
pub fn mindshielded(world: &World, mob: Entity) -> bool {
    world.get::<Mindshield>(mob).is_some()
        || job_title(world, mob).is_some_and(|t| MINDSHIELD_JOBS.contains(&t.as_str()))
}

pub fn is_rev(world: &World, mob: Entity) -> bool {
    world.get::<Revolutionary>(mob).is_some()
}

pub fn is_head_rev(world: &World, mob: Entity) -> bool {
    world.get::<Revolutionary>(mob).is_some_and(|r| r.head)
}

/// `SSjob.get_all_heads()`: crew bodies (alive or not) of the head of staff jobs.
pub fn heads_of_staff(world: &mut World) -> Vec<Entity> {
    let mut q = world.query_filtered::<Entity, With<PlayerJob>>();
    let mut heads: Vec<Entity> = q.iter(world).collect();
    heads.retain(|&m| is_head_of_staff(world, m));
    heads.sort();
    heads
}

/// Living heads of staff with a client (the rulesets' `head_check`).
pub fn active_heads(world: &mut World) -> usize {
    heads_of_staff(world)
        .into_iter()
        .filter(|&m| considered_alive(world, m) && client_of(world, m).is_some())
        .count()
}

/// `is_station_level(get_turf(mob))`.
fn on_station(world: &World, mob: Entity) -> bool {
    crate::modpc::pda::turf_of(world, mob)
        .is_some_and(|t| crate::events::controller::station_levels(world).contains(&t.z))
}

/// `/datum/objective/mutiny/check_completion` (without the AFK grace): dead or off the station.
pub fn mutiny_complete(world: &World, target: Entity) -> bool {
    !considered_alive(world, target) || !on_station(world, target)
}

fn is_antag(world: &World, mob: Entity) -> bool {
    world.get::<AntagHudData>(mob).is_some()
}

/// `can_be_headrev(candidate, roundstart)`.
pub fn can_be_headrev(world: &World, mob: Entity, roundstart: bool) -> bool {
    let Some(client) = client_of(world, mob) else {
        return false;
    };
    let wants = world.get::<AntagPreferences>(client).is_some_and(|p| {
        p.be_special.contains(ROLE_REV_HEAD)
            || (!roundstart && p.be_special.contains(ROLE_PROVOCATEUR))
    });
    considered_alive(world, mob)
        && on_station(world, mob)
        && !is_antag(world, mob)
        && wants
        && !is_head_of_staff(world, mob)
        && !mindshielded(world, mob)
}

/// `/datum/antagonist/rev/can_be_converted`: a carbon with a mind that `can_be_owned`.
pub fn can_be_converted(world: &World, mob: Entity) -> bool {
    world.get::<Inventory>(mob).is_some()
        && world.get::<Health>(mob).is_some()
        && !is_rev(world, mob)
        && !is_head_of_staff(world, mob)
        && !mindshielded(world, mob)
}

// ---------------------------------------------------------------------------------------------
// Gaining and losing

/// The selected head revolutionary waits (tg "Dormant Head Revolutionary").
pub fn make_dormant(world: &mut World, mob: Entity, latejoin: bool) {
    let delay = if latejoin {
        LATEJOIN_REVEAL_SECS
    } else {
        ROUNDSTART_REVEAL_SECS
    };
    let reveal_at = now(world) + delay;
    world.entity_mut(mob).insert(DormantHeadRev {
        reveal_at,
        latejoin,
    });
}

fn reveal_dormant(world: &mut World) {
    let t = now(world);
    let mut q = world.query::<(Entity, &DormantHeadRev)>();
    let due: Vec<(Entity, bool)> = q
        .iter(world)
        .filter(|(_, d)| t >= d.reveal_at)
        .map(|(e, d)| (e, d.latejoin))
        .collect();
    for (mob, latejoin) in due {
        world.entity_mut(mob).remove::<DormantHeadRev>();
        reveal_head(world, mob, latejoin);
    }
}

/// `reveal_head` of the roundstart (`find_another_headrev` when the selected one is no longer
/// eligible) and latejoin rulesets.
pub fn reveal_head(world: &mut World, mob: Entity, latejoin: bool) {
    let heads = active_heads(world);
    let needed = if latejoin {
        LATEJOIN_HEADS_NECESSARY - 1
    } else {
        ROUNDSTART_HEADS_NECESSARY
    };
    if heads < needed {
        info!("revolution: not enough heads of staff ({heads}) to start a revolution");
        return;
    }
    let chosen = if can_be_headrev(world, mob, !latejoin) {
        Some(mob)
    } else if latejoin {
        None
    } else {
        let mut q = world.query_filtered::<Entity, (With<PlayerMob>, With<Inventory>)>();
        let mut all: Vec<Entity> = q.iter(world).collect();
        all.sort();
        all.into_iter().find(|&m| can_be_headrev(world, m, true))
    };
    let Some(head) = chosen else {
        info!("revolution: no eligible head revolutionary, the ruleset is canceled");
        return;
    };
    add_revolutionary(world, head, true);
}

/// `add_antag_datum(/datum/antagonist/rev(/head))` (`give_flash` for heads) and, without a
/// running revolution, `start_revolution`. `false` if it cannot be one.
pub fn add_revolutionary(world: &mut World, mob: Entity, head: bool) -> bool {
    if world.get::<Health>(mob).is_none() || is_head_of_staff(world, mob) {
        return false;
    }
    if let Some(rev) = world.get::<Revolutionary>(mob) {
        if rev.head || !head {
            return false;
        }
        // `promote()`.
        world.entity_mut(mob).remove::<Revolutionary>();
    } else if mindshielded(world, mob) && !head {
        return false;
    }
    if !world.contains_resource::<RevolutionHandler>() {
        world.init_resource::<RevolutionHandler>();
    }
    gain(world, mob, head);
    start_revolution(world);
    refresh_info(world);
    if let Some(client) = client_of(world, mob) {
        let name = if head { HEAD_NAME } else { REV_NAME };
        world.write_message(OpenUi::new::<AntagInfoRevolutionUi>(client, mob, mob, name));
    }
    true
}

fn gain(world: &mut World, mob: Entity, head: bool) {
    let client = client_of(world, mob);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let state = if head { "rev_head" } else { "rev" };
    let hud_icon = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(ANTAG_HUD_DMI, state));
    world.entity_mut(mob).insert((
        Revolutionary { head, key },
        AntagHudData {
            hud_icon,
            ..default()
        },
    ));
    {
        let mut handler = world.resource_mut::<RevolutionHandler>();
        if !handler.members.contains(&mob) {
            handler.members.push(mob);
        }
    }
    if head {
        // `equip_rev`: `give_flash`.
        match crate::antag::brother::equip_conspicuous_item(world, mob, REV_FLASH) {
            Some(_) => chat_to(
                world,
                mob,
                Span::Plain,
                "Выданная вам вспышка поможет вам убедить команду присоединиться к вашему делу.",
            ),
            None => chat_to(
                world,
                mob,
                Span::Plain,
                "К сожалению, Синдикат не смог достать вам вспышку.",
            ),
        }
    }
    let name = if head { HEAD_NAME } else { REV_NAME };
    chat_to(world, mob, Span::Bold, format!("Вы - {name}."));
    if let Some(client) = client {
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
    }
    info!("{mob} joined the revolution (head: {head})");
}

/// `/datum/revolution_handler/start_revolution`.
pub fn start_revolution(world: &mut World) {
    let handler = world.resource::<RevolutionHandler>();
    if handler.started || handler.result.is_some() {
        return;
    }
    let targets = heads_of_staff(world);
    let mut handler = world.resource_mut::<RevolutionHandler>();
    handler.started = true;
    handler.targets = targets;
    info!("revolution: started against {:?}", handler.targets);
}

/// `COMSIG_GLOB_JOB_AFTER_LATEJOIN_SPAWN` (`update_objectives`): a latejoining head of staff
/// becomes a mutiny target, every revolutionary is told.
pub fn on_latejoin(world: &mut World, mob: Entity) {
    let running = world
        .get_resource::<RevolutionHandler>()
        .is_some_and(|h| h.started && h.result.is_none());
    if !running || !is_head_of_staff(world, mob) {
        return;
    }
    world.resource_mut::<RevolutionHandler>().targets.push(mob);
    refresh_info(world);
    let text = mutiny_text(world, mob);
    let members = world.resource::<RevolutionHandler>().members.clone();
    for member in members {
        chat_to(world, member, Span::Notice, "Ваши текущие задачи:");
        chat_to(world, member, Span::Plain, text.clone());
    }
}

/// `/datum/objective/mutiny/update_explanation_text`.
fn mutiny_text(world: &World, target: Entity) -> String {
    let job = job_title(world, target)
        .map(|t| ss13_tgui::interfaces::ru_jobs::job_ru(&t).map_or(t, str::to_owned))
        .unwrap_or_default();
    format!(
        "Убейте или отправьте в изгнание {}, {job}.",
        name_of(world, target)
    )
}

/// The panel of every revolutionary (BandaStation: the team's objectives are each one's).
fn refresh_info(world: &mut World) {
    let Some(handler) = world.get_resource::<RevolutionHandler>() else {
        return;
    };
    let (members, targets) = (handler.members.clone(), handler.targets.clone());
    let objectives: Vec<ObjectiveInfo> = targets
        .iter()
        .enumerate()
        .map(|(i, &t)| ObjectiveInfo {
            count: i as u32 + 1,
            explanation: mutiny_text(world, t),
            complete: mutiny_complete(world, t),
        })
        .collect();
    for member in members {
        let Some(rev) = world.get::<Revolutionary>(member) else {
            continue;
        };
        let antag_name = if rev.head { HEAD_NAME } else { REV_NAME }.to_owned();
        world.entity_mut(member).insert(RevInfo(BrotherInfo {
            antag_name,
            objectives: objectives.clone(),
        }));
    }
}

/// `/datum/antagonist/rev(/head)/remove_revolutionary` + `farewell`. A head revolutionary only
/// loses the role when the station wins.
pub fn remove_revolutionary(world: &mut World, mob: Entity, deconverter: Deconverter) -> bool {
    let Some(rev) = world.get::<Revolutionary>(mob).cloned() else {
        return false;
    };
    if rev.head && deconverter != Deconverter::StationWin && deconverter != Deconverter::Admin {
        return false;
    }
    let alive = world.get::<Stat>(mob).is_some_and(|s| *s != Stat::Dead);
    if let Deconverter::Mob(_) = deconverter {
        crate::status_effects::apply(world, mob, Incap::Unconscious, DECONVERT_SECS);
    }
    let name = name_of(world, mob);
    if !rev.head {
        visible(
            world,
            mob,
            "Вы больше не революционер с промытыми мозгами! Ваши воспоминания о временах, когда \
             вы были бунтарем туманны... единственное, что вы помните — это имя того, кто промыл \
             вам мозги...",
            &format!("{name} похоже, только что вспомнил свою истинную приверженность!"),
            Span::Bold,
        );
    } else if deconverter != Deconverter::StationWin {
        let own = if alive {
            "Вы отказались от своей цели свергнуть командный состав. Вы больше не являетесь главой \
             революционеров."
        } else {
            "Долгожданное облегчение после смерти. Вы больше не являетесь главой революционеров."
        };
        chat_to(world, mob, Span::Bold, own);
    }
    world
        .entity_mut(mob)
        .remove::<(Revolutionary, AntagHudData, RevInfo)>();
    if let Some(mut handler) = world.get_resource_mut::<RevolutionHandler>() {
        handler.members.retain(|&m| m != mob);
    }
    info!("{mob} was deconverted from the revolution ({deconverter:?})");
    true
}

// ---------------------------------------------------------------------------------------------
// Conversion

/// `/datum/component/can_flash_from_behind` of a head revolutionary.
pub fn can_flash_from_behind(world: &World, user: Entity) -> bool {
    is_head_rev(world, user)
}

/// `/datum/antagonist/rev/head/on_flash_success`. True when the flashed one was converted (the
/// flash's use is refunded, `flash.times_used--`).
pub fn on_successful_flash(world: &mut World, user: Entity, flashed: Entity) -> bool {
    if !is_head_rev(world, user) {
        return false;
    }
    let stat = world.get::<Stat>(flashed).copied();
    if stat.is_none_or(|s| s == Stat::Dead) || world.get::<Inventory>(flashed).is_none() {
        return false;
    }
    let name = name_of(world, flashed);
    if stat != Some(Stat::Conscious) {
        chat_to(
            world,
            user,
            Span::Warning,
            "Он должен быть в сознании, прежде чем вы сможете конвертировать его!",
        );
        return false;
    }
    if client_of(world, flashed).is_none() {
        chat_to(
            world,
            user,
            Span::Warning,
            format!("Разум {name} настолько пуст, что не поддается влиянию!"),
        );
        return false;
    }
    check_size(world);
    if !can_be_converted(world, flashed) {
        chat_to(
            world,
            user,
            Span::Warning,
            format!("{name} кажется устойчивым к вспышке!"),
        );
        return false;
    }
    // `add_revolutionary(stun = TRUE, mute = TRUE)`: 10 s of silence, `flash_act(1, 1)` and a
    // 10 s stun (no holiday memes: always muted).
    crate::status_effects::set_timed(
        world,
        flashed,
        crate::status_effects::Timed::Silence,
        CONVERT_SILENCE_SECS,
        true,
    );
    crate::status_effects::apply(world, flashed, Incap::Stun, CONVERT_STUN_SECS);
    add_revolutionary(world, flashed, false)
}

/// BandaStation `/datum/team/revolution/check_size`.
fn check_size(world: &mut World) {
    if world
        .get_resource::<RevolutionHandler>()
        .is_none_or(|h| h.ascendent)
    {
        return;
    }
    let mut q = world.query_filtered::<(Entity, &Stat), With<PlayerMob>>();
    let living: Vec<Entity> = q
        .iter(world)
        .filter(|(_, s)| **s != Stat::Dead)
        .map(|(e, _)| e)
        .collect();
    let revs = living.iter().filter(|&&m| is_rev(world, m)).count() as u32;
    let alive = living.len() as u32 - revs;
    if revs == 0 {
        return;
    }
    let threshold = if alive >= REV_HIGHPOP_THRESHOLD {
        REV_ASCENDENT_HIGHPOP
    } else {
        REV_ASCENDENT_LOWPOP
    };
    let ratio = if alive > 0 {
        revs as f32 / alive as f32
    } else {
        1.0
    };
    if ratio < threshold {
        return;
    }
    world.resource_mut::<RevolutionHandler>().ascendent = true;
    let members = world.resource::<RevolutionHandler>().members.clone();
    for member in members {
        chat_to(
            world,
            member,
            Span::BoldWarning,
            "Ваше революционное движение набирает силу. Сотрудникам службы безопасности и \
             командованию станции известно о вашей активности. Пора начинать действовать!",
        );
    }
    let percent = (ratio * 100.0).floor() as u32;
    crate::shuttle::priority_announce(
        world,
        &format!(
            "{}: Отдел мониторинга лояльности",
            crate::events::COMMAND_NAME
        ),
        "",
        &format!(
            "Мы фиксируем антикорпоративные настроения связанные с действиями мятежников на \
             вашей станции. Согласно нашим данным, {percent}% экипажа станции присоединились к \
             мятежникам. Сотрудники службы безопасности наделены полномочиями применять \
             летальную силу против мятежников. Остальному экипажу надлежит сотрудничать с \
             сотрудниками службы безопасности для защиты себя и своих отделов, не ведя охоту на \
             мятежников. Члены экипажа примкнувшие в ряды мятежников, должны быть арестованы и \
             перевоспитаны для восстановления лояльности к корпорации. Пострадавшим членам \
             экипажа должна быть оказана медицинская помощь, как только ситуация на станции \
             будет стабилизирована."
        ),
    );
    info!("revolution: peaked with {revs} revolutionaries");
}

/// `item_attack.dm` `attacked_by`, `BODY_ZONE_HEAD`: blunt brute force on a conscious
/// revolutionary's head may deconvert it (`prob(damage_done + (100 - health) * 0.5)`).
pub fn head_trauma(world: &mut World, attacker: Entity, target: Entity, damage: f32) {
    if attacker == target
        || !is_rev(world, target)
        || world.get::<Stat>(target) != Some(&Stat::Conscious)
    {
        return;
    }
    let health = world.get::<Health>(target).map_or(100.0, Health::health);
    let chance = damage + (100.0 - health) * 0.5;
    if world
        .resource_mut::<crate::combat::CombatRng>()
        .chance(chance)
    {
        remove_revolutionary(world, target, Deconverter::Mob(attacker));
    }
}

// ---------------------------------------------------------------------------------------------
// The mindshield

/// A mindshield implant goes into `target` (`crate::implants`): `COMSIG_PRE_MINDSHIELD_IMPLANT`
/// (a head revolutionary resists: true, the implant is gone) and `COMSIG_MINDSHIELD_IMPLANTED`
/// (`on_mindshield` deconverts).
pub fn on_mindshield_implant(world: &mut World, target: Entity, user: Option<Entity>) -> bool {
    if is_head_rev(world, target) {
        let name = name_of(world, target);
        visible(
            world,
            target,
            "You feel something interfering with your mental conditioning, but you resist it!",
            &format!("{name} seems to resist the implant!"),
            Span::Warning,
        );
        return true;
    }
    let source = user.map_or(Deconverter::Admin, Deconverter::Mob);
    remove_revolutionary(world, target, source);
    false
}

// ---------------------------------------------------------------------------------------------
// The outcome

/// `/datum/revolution_handler/process`.
fn process(world: &mut World) {
    let handler = world.resource::<RevolutionHandler>();
    if !handler.started || handler.result.is_some() {
        return;
    }
    let (targets, members) = (handler.targets.clone(), handler.members.clone());
    if check_rev_victory(world, &targets) {
        declare_revs_win(world);
    } else if check_heads_victory(world, &members) {
        declare_heads_win(world);
    }
}

/// `check_rev_victory`: every mutiny objective complete.
pub fn check_rev_victory(world: &World, targets: &[Entity]) -> bool {
    targets.iter().all(|&t| mutiny_complete(world, t))
}

/// `check_heads_victory`: every current head revolutionary dead or off the station.
pub fn check_heads_victory(world: &World, members: &[Entity]) -> bool {
    members
        .iter()
        .filter(|&&m| is_head_rev(world, m))
        .all(|&m| mutiny_complete(world, m))
}

fn save_members(world: &mut World) {
    let members = world.resource::<RevolutionHandler>().members.clone();
    let (mut heads, mut revs) = (Vec::new(), Vec::new());
    for m in members {
        if let Some(rev) = world.get::<Revolutionary>(m) {
            let entry = (m, rev.key.clone());
            if rev.head {
                heads.push(entry);
            } else {
                revs.push(entry);
            }
        }
    }
    let mut handler = world.resource_mut::<RevolutionHandler>();
    handler.ex_headrevs = heads;
    handler.ex_revs = revs;
}

/// `declare_revs_win` (BandaStation's announcement; the round goes on).
fn declare_revs_win(world: &mut World) {
    save_members(world);
    crate::shuttle::priority_announce(
        world,
        "Оповещение от Синдиката: Viva la Revolution",
        "",
        "Похоже, восстание одержало победу. Пожалуйста, немедленно прекратите работу и окажите \
         помощь пострадавшим лидерам революции и своим товарищам. Лидерам революции разрешается \
         вызвать эвакуационный шаттл для возвращения на оперативную базу.",
    );
    world.resource_mut::<RevolutionHandler>().result = Some(RevResult::Revolution);
    info!("revolution: the revolution won");
}

/// `declare_heads_win`: everyone deconverted.
fn declare_heads_win(world: &mut World) {
    save_members(world);
    let members = world.resource::<RevolutionHandler>().members.clone();
    for m in members {
        remove_revolutionary(world, m, Deconverter::StationWin);
    }
    crate::shuttle::priority_announce(
        world,
        &format!(
            "{}: Отдел мониторинга лояльности",
            crate::events::COMMAND_NAME
        ),
        "",
        "Похоже, мятеж подавлен. Пожалуйста, верните себя и своих недееспособных коллег к \
         работе. Мы дистанционно внесли глав революции в черный список в ваших медицинских \
         записях, чтобы предотвратить нежелательную реанимацию.",
    );
    world.resource_mut::<RevolutionHandler>().result = Some(RevResult::Station);
    info!("revolution: the station won");
}

// ---------------------------------------------------------------------------------------------
// Panel and report

impl ServerUi for AntagInfoRevolutionUi {
    type Source = &'static RevInfo;
    type UserParam = ();
    type Act = ();

    fn static_data(info: &RevInfo) -> BrotherInfo {
        info.0.clone()
    }

    fn data(_: &RevInfo, _: Entity, _: &()) {}

    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

/// `/datum/action/antag_info/Trigger` of a revolutionary.
fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if let Some(rev) = world.get::<Revolutionary>(source) {
            let name = if rev.head { HEAD_NAME } else { REV_NAME };
            refresh_info(world);
            world.write_message(OpenUi::new::<AntagInfoRevolutionUi>(
                client, actor, source, name,
            ));
        }
    }
}

/// `/datum/team/revolution/roundend_report`.
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let Some(handler) = world.get_resource::<RevolutionHandler>() else {
        return Vec::new();
    };
    let (mut headrevs, mut revs) = (handler.ex_headrevs.clone(), handler.ex_revs.clone());
    let members = handler.members.clone();
    if headrevs.is_empty() && revs.is_empty() {
        for m in members {
            if let Some(rev) = world.get::<Revolutionary>(m) {
                let entry = (m, rev.key.clone());
                if rev.head {
                    headrevs.push(entry);
                } else {
                    revs.push(entry);
                }
            }
        }
    }
    if headrevs.is_empty() && revs.is_empty() {
        return Vec::new();
    }
    let line = |span: Span, text: String| ChatMessage {
        channel: ChatChannel::System,
        segments: vec![ChatSegment::new(span, text)],
    };
    let mut out = Vec::new();
    let mut q = world.query_filtered::<(Entity, &Stat), (With<PlayerMob>, With<Inventory>)>();
    let survivors: Vec<Entity> = q
        .iter(world)
        .filter(|(_, s)| **s != Stat::Dead)
        .map(|(e, _)| e)
        .collect();
    if !survivors.is_empty() {
        let in_revs = survivors
            .iter()
            .filter(|s| headrevs.iter().chain(&revs).any(|(m, _)| m == *s))
            .count();
        let approval = 100.0 - (in_revs as f64 / survivors.len() as f64 * 1000.0).round() / 10.0;
        out.push(line(
            Span::Plain,
            format!("Command's Approval Rating: {approval}%"),
        ));
    }
    for (header, list) in [
        ("Главными революционерами были:", &headrevs),
        ("Революционерами были:", &revs),
    ] {
        if list.is_empty() {
            continue;
        }
        out.push(line(Span::Bold, header.to_owned()));
        for (m, key) in list {
            out.push(print_player(world, *m, key));
        }
    }
    let heads = heads_of_staff(world);
    if !heads.is_empty() {
        out.push(line(Span::Bold, "Командованием станции были:".to_owned()));
        for head in heads {
            let key = client_of(world, head)
                .and_then(|c| world.get::<ClientToken>(c))
                .map(|t| t.0.clone())
                .unwrap_or_default();
            out.push(print_player(world, head, &key));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heads_of_staff_follow_head_of_staff_job_flags() {
        assert!(HEAD_JOBS.contains(&"Quartermaster"));
        assert!(HEAD_JOBS.contains(&"Nanotrasen Representative"));
        assert!(!HEAD_JOBS.contains(&"Warden"));
        assert_eq!(ROUNDSTART_REVEAL_SECS, 420.0);
        assert_eq!(
            (ROUNDSTART_HEADS_NECESSARY, LATEJOIN_HEADS_NECESSARY),
            (2, 3)
        );
    }

    #[test]
    fn victory_checks_follow_the_handler() {
        let mut world = World::new();
        let alive = world.spawn(Stat::Conscious).id();
        let dead = world.spawn(Stat::Dead).id();
        let head_rev = world
            .spawn((
                Stat::Conscious,
                Revolutionary {
                    head: true,
                    key: String::new(),
                },
            ))
            .id();
        let dead_head_rev = world
            .spawn((
                Stat::Dead,
                Revolutionary {
                    head: true,
                    key: String::new(),
                },
            ))
            .id();
        let rev = world
            .spawn((
                Stat::Conscious,
                Revolutionary {
                    head: false,
                    key: String::new(),
                },
            ))
            .id();
        // No turf: off the station, so a living target counts as exiled.
        assert!(check_rev_victory(&world, &[dead, alive]));
        // Every target dead.
        assert!(check_rev_victory(&world, &[dead]));
        // tg: no mutiny objectives at all is a revolution victory.
        assert!(check_rev_victory(&world, &[]));
        // A dead head revolutionary and a regular one: the station wins.
        assert!(check_heads_victory(&world, &[dead_head_rev, rev]));
        // No head revolutionaries left: the station wins too.
        assert!(check_heads_victory(&world, &[rev]));
        // `head_rev` has no turf here either: off the station counts as exiled.
        assert!(check_heads_victory(&world, &[head_rev]));
    }
}
