//! The malfunctioning AI's Doomsday Device (server), after tg
//! `/datum/ai_module/malf/destructive/nuke_station`, `/datum/action/innate/ai/nuke_station` and
//! `/obj/machinery/doomsday_device` (`malf_ai_modules.dm`).
//!
//! - The action (bought for 130 with 10 hacked APCs, `super::malf_modules`) works only on a
//!   station level and asks `purge_all_life()`; confirmed, the AI types its way through the
//!   self-destruct prompt (`set_up_us_the_bomb`, ~25 s of chat lines and sounds). Each step checks
//!   again that the AI exists and sits on its own tile (not shunted, not carded); a failed check
//!   stops it and the action stays.
//! - Then, if the AI is alive: the "Обнаружена аномалия" announcement, delta alert, the device
//!   inside the AI counts 450 s down, a minor announcement every 60 s; the action is spent.
//! - The AI dying, leaving its tile (shunt, card) or the station level shuts the device off (the
//!   level case with tg's "ЗА ПРЕДЕЛАМИ СТАНЦИИ" announcement) and drops the alert to red.
//! - At zero: the nuke alarm for everyone, 10 s later (tg's cinematic) every living non-silicon
//!   mob on the station levels is dusted, the world is told, the station counts as nuked and the
//!   round ends.
// shortcut: no cinematic, ghost notification, shuttle lockdown (`registerHostileEnvironment`),
// red circuit floors (`add_nuke_threat`), cyborg doom lamps, pinpointer tracking or countdown
// maptext; mobs inside closets or other containers (no tile) are not dusted.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems};
use ss13_tgui::server::UiSystems;

use super::Silicon;
use super::ai::AiInside;
use super::chassis::tell_ai;
use crate::health::{Health, Stat};
use crate::security_level::SecLevel;
use crate::sound::{PlaySound, playsound};
use crate::tools::now;

/// tg `DEFAULT_DOOMSDAY_TIMER`, `DOOMSDAY_ANNOUNCE_INTERVAL`.
pub const DOOMSDAY_SECS: f64 = 450.0;
pub const ANNOUNCE_INTERVAL_SECS: f64 = 60.0;
/// The cinematic's `addtimer(..., 10 SECONDS)` before `trigger_doomsday`.
pub const CINEMATIC_SECS: f64 = 10.0;
const ERROR_TITLE: &str = "ERROR ER0RR $R0RRO$!R41.%%!!(%$^^__+ @#F0E4";
const CONFIRM: [&str; 2] = ["confirm = TRUE;", "confirm = FALSE;"];
const SERVER_READY: &str = "sound/misc/server-ready.ogg";
const STAGE2: &str = "sound/machines/compiler/compiler-stage2.ogg";

/// `set_up_us_the_bomb` in progress, on the AI: the next step and when it runs.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct DoomsdaySetup {
    pub step: u8,
    pub next_at: f64,
    /// `prob(10) ? "******" : "hunter2"`.
    pub pass: &'static str,
}

/// `/obj/machinery/doomsday_device` inside the AI (tg `doomsday_device`, `nuking`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Doomsday {
    pub detonate_at: f64,
    pub next_announce: f64,
    /// `timing` ran out: the doom comes at this time.
    pub doom_at: Option<f64>,
}

/// The confirmation, on its prompt entity.
#[derive(Component, Clone, Copy, Debug)]
struct DoomsdayPrompt {
    ai: Entity,
    index: usize,
}

pub struct DoomsdayPlugin;

impl Plugin for DoomsdayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                confirmed
                    .in_set(UiSystems::Handle)
                    .after(PromptSystems::Answer),
                setup_steps.run_if(any_with_component::<DoomsdaySetup>),
                countdown.run_if(any_with_component::<Doomsday>),
            ),
        );
    }
}

fn station(world: &World, pos: Option<&TilePos>) -> bool {
    pos.is_some_and(|p| crate::events::controller::station_levels(world).contains(&p.z))
}

/// The AI is on its own tile (tg `isturf(owner_AI.loc)`) and not dead.
fn in_core(world: &World, ai: Entity) -> bool {
    world.get::<AiInside>(ai).is_none()
        && world.get::<TilePos>(ai).is_some()
        && world.get::<Stat>(ai).is_some_and(|s| *s != Stat::Dead)
}

/// `/datum/action/innate/ai/nuke_station/Activate`: the off-station refusal, else the question.
pub fn activate(world: &mut World, client: Entity, eye: Entity, ai: Entity, index: usize) {
    if !station(world, world.get::<TilePos>(ai)) {
        tell_ai(
            world,
            ai,
            Span::Warning,
            "Вы не можете активировать устройство судного дня за пределами станции!",
        );
        return;
    }
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        Ask::new(
            client,
            eye,
            "purge_all_life()",
            "Отправить сигнал активации? (true = активировтаь, false = отмена)",
            PromptKind::Alert {
                buttons: CONFIRM.map(str::to_owned).to_vec(),
                autofocus: true,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(DoomsdayPrompt { ai, index });
}

fn confirmed(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let yes: Vec<DoomsdayPrompt> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .filter(|a| a.answer == Some(PromptAnswer::Button(0)))
        .filter_map(|a| world.get::<DoomsdayPrompt>(a.prompt).copied())
        .collect();
    for DoomsdayPrompt { ai, index } in yes {
        // `if (active || owner_AI.stat == DEAD)`, `if (!isturf(owner_AI.loc))`, and the action
        // must still be owned.
        let owned = world
            .get::<super::malf_modules::MalfModules>(ai)
            .is_some_and(|m| m.uses.iter().any(|&(i, u)| i == index && u > 0));
        if !owned
            || !in_core(world, ai)
            || world.get::<DoomsdaySetup>(ai).is_some()
            || world.get::<Doomsday>(ai).is_some()
        {
            continue;
        }
        info!("{ai} has activated AI Doomsday");
        let pass = if roll(world, 1, 100) <= 10 {
            "******"
        } else {
            "hunter2"
        };
        tell_ai(world, ai, Span::Danger, "run -o -a 'selfdestruct'");
        let next_at = now(world) + 0.5;
        world.entity_mut(ai).insert(DoomsdaySetup {
            step: 1,
            next_at,
            pass,
        });
    }
}

fn roll(world: &mut World, low: u32, high: u32) -> u32 {
    world
        .get_resource_mut::<crate::combat::CombatRng>()
        .map_or(low, |mut r| r.range(low, high))
}

fn ai_hears(world: &mut World, ai: Entity, sound: &str) {
    if let Some(&pos) = world.get::<TilePos>(ai) {
        playsound(
            world,
            PlaySound::new(pos, sound, 50.0, false).extrarange(-7.0),
        );
    }
}

/// One line of `set_up_us_the_bomb`: what it says and plays, and the wait before the next.
fn run_step(world: &mut World, ai: Entity, step: u8, pass: &str) -> Option<f64> {
    let say = |world: &mut World, span, text: &str| tell_ai(world, ai, span, text);
    Some(match step {
        1 => {
            say(world, Span::Danger, "Running executable 'selfdestruct'...");
            f64::from(roll(world, 10, 30)) / 10.0
        }
        2 => {
            ai_hears(world, ai, "sound/announcer/alarm/bloblarm.ogg");
            say(
                world,
                Span::Danger,
                "!!! НЕСАНКЦИОНИРОВАННЫЙ ДОСТУП К СИСТЕМЕ САМОУНИЧТОЖЕНИЯ !!!",
            );
            say(
                world,
                Span::Danger,
                "Это нарушение безопасности 3-го класса. Об данном инциденте будет сообщено на \
                 Центральное Командование.",
            );
            2.0
        }
        3..=5 => {
            let i = u32::from(step - 2);
            let percent = roll(world, 0, 9) + roll(world, 20, 30) * i;
            say(
                world,
                Span::Danger,
                &format!("Отправка отчёта безопасности Центральному Командованию.....{percent}%"),
            );
            if step == 5 { 0.3 } else { 2.0 }
        }
        6 => {
            say(
                world,
                Span::Danger,
                &format!("auth 'akjv9c88asdf12nb' {pass}"),
            );
            ai_hears(world, ai, "sound/items/timer.ogg");
            3.0
        }
        7 => {
            say(
                world,
                Span::BoldNotice,
                "Аутентификация успешна. Добро пожаловать, akjv9c88asdf12nb.",
            );
            ai_hears(world, ai, SERVER_READY);
            0.5
        }
        8 => {
            say(
                world,
                Span::BoldNotice,
                "Активировать устройство самоуничтожения? (Д/Н)",
            );
            ai_hears(world, ai, "sound/machines/compiler/compiler-stage1.ogg");
            2.0
        }
        9 => {
            say(world, Span::Danger, "Д");
            1.5
        }
        10 => {
            say(
                world,
                Span::BoldNotice,
                "Подтвердите активацию устройства самоуничтожения? (Д/Н)",
            );
            ai_hears(world, ai, STAGE2);
            1.0
        }
        11 => {
            say(world, Span::Danger, "Д");
            f64::from(roll(world, 15, 25)) / 10.0
        }
        12 => {
            say(
                world,
                Span::BoldNotice,
                "Пожалуйста, введите пароль для подтверждения.",
            );
            ai_hears(world, ai, STAGE2);
            1.4
        }
        13 => {
            say(world, Span::Danger, pass);
            4.0
        }
        14 => {
            say(
                world,
                Span::BoldNotice,
                "Аутентификация пройдена. Передача сигнала активации...",
            );
            ai_hears(world, ai, SERVER_READY);
            3.0
        }
        _ => return None,
    })
}

/// The sleeps of `set_up_us_the_bomb`; every wake-up checks the AI again.
fn setup_steps(world: &mut World) {
    let t = now(world);
    let due: Vec<(Entity, DoomsdaySetup)> = world
        .query::<(Entity, &DoomsdaySetup)>()
        .iter(world)
        .filter(|(_, s)| t >= s.next_at)
        .map(|(e, s)| (e, s.clone()))
        .collect();
    for (ai, setup) in due {
        // `if(QDELETED(owner) || !isturf(owner_AI.loc))`: `active = FALSE`, the action stays.
        if world.get::<AiInside>(ai).is_some() || world.get::<TilePos>(ai).is_none() {
            world.entity_mut(ai).remove::<DoomsdaySetup>();
            continue;
        }
        match run_step(world, ai, setup.step, setup.pass) {
            Some(wait) => {
                world.entity_mut(ai).insert(DoomsdaySetup {
                    step: setup.step + 1,
                    next_at: t + wait,
                    pass: setup.pass,
                });
            }
            None => {
                world.entity_mut(ai).remove::<DoomsdaySetup>();
                arm(world, ai);
            }
        }
    }
}

/// The end of `set_up_us_the_bomb`: a living AI starts the device and the action is spent.
fn arm(world: &mut World, ai: Entity) {
    if world.get::<Stat>(ai).is_none_or(|s| *s == Stat::Dead) {
        return;
    }
    crate::events::priority_announce_sound(
        world,
        "Обнаружена аномалия",
        "Во всех системах станций обнаружены вредоносные процессы. Пожалуйста, уничтожьте свой \
         ИИ, чтобы предотвратить возможный ущерб его моральному ядру.",
        "announcer_aimalf",
    );
    crate::security_level::set_level(world, SecLevel::Delta, true);
    let t = now(world);
    world.entity_mut(ai).insert(Doomsday {
        detonate_at: t + DOOMSDAY_SECS,
        next_announce: t + ANNOUNCE_INTERVAL_SECS,
        doom_at: None,
    });
    if let Some(index) = super::malf_modules::MODULES
        .iter()
        .position(|m| m.power == super::malf_modules::Power::Doomsday)
    {
        super::malf_modules::spend_use(world, ai, index);
    }
}

/// tg `ShutOffDoomsdayDevice` -> `doomsday_device/Destroy`: back to red.
pub fn shut_off(world: &mut World, ai: Entity) {
    let Ok(mut e) = world.get_entity_mut(ai) else {
        return;
    };
    e.remove::<DoomsdaySetup>();
    if e.take::<Doomsday>().is_some() {
        crate::security_level::set_level(world, SecLevel::Red, true);
        info!("{ai}'s doomsday device shut off");
    }
}

/// `doomsday_device/process` (`SSfastprocess`): the off-station abort, the announcements, the
/// alarm at zero and the doom 10 s later.
fn countdown(world: &mut World) {
    let t = now(world);
    let devices: Vec<(Entity, Doomsday)> = world
        .query::<(Entity, &Doomsday)>()
        .iter(world)
        .map(|(e, d)| (e, *d))
        .collect();
    for (ai, mut device) in devices {
        if let Some(doom_at) = device.doom_at {
            if t >= doom_at {
                world.entity_mut(ai).remove::<Doomsday>();
                trigger_doomsday(world);
            }
            continue;
        }
        // The AI's death, a shunt or a card (tg `ShutOffDoomsdayDevice` from `death`,
        // `malfshunt`, `transfer_ai`).
        if world.get::<AiInside>(ai).is_some()
            || world.get::<Stat>(ai).is_none_or(|s| *s == Stat::Dead)
        {
            shut_off(world, ai);
            continue;
        }
        if !station(world, world.get::<TilePos>(ai)) {
            crate::security_level::minor_announce(
                world,
                ERROR_TITLE,
                "УСТРОЙСТВО СУДНОГО ДНЯ ЗА ПРЕДЕЛАМИ СТАНЦИИ, ОТМЕНА",
                true,
            );
            shut_off(world, ai);
            continue;
        }
        let left = (device.detonate_at - t).max(0.0).round();
        if left <= 0.0 {
            playsound(
                world,
                PlaySound::to_playing_players("sound/announcer/alarm/nuke_alarm.ogg", 70.0, false),
            );
            device.doom_at = Some(t + CINEMATIC_SECS);
        } else if t >= device.next_announce {
            crate::security_level::minor_announce(
                world,
                ERROR_TITLE,
                &format!("{left} СЕКУНД ДО АКТИВАЦИИ УСТРОЙСТВА СУДНОГО ДНЯ!"),
                true,
            );
            device.next_announce += ANNOUNCE_INTERVAL_SECS;
        }
        world.entity_mut(ai).insert(device);
    }
}

/// The living non-silicon mobs on the station levels (`callback_on_everyone_on_z` +
/// `bring_doomsday`'s `issilicon`).
pub fn doomed(world: &mut World) -> Vec<Entity> {
    let levels = crate::events::controller::station_levels(world);
    world
        .query_filtered::<(Entity, &TilePos, &Stat), (
            With<Health>,
            Without<Silicon>,
            Without<crate::ghost::Ghost>,
        )>()
        .iter(world)
        .filter(|(_, p, s)| levels.contains(&p.z) && **s != Stat::Dead)
        .map(|(e, ..)| e)
        .collect()
}

/// `trigger_doomsday`: everyone dusted, the station destroyed, the round over.
fn trigger_doomsday(world: &mut World) {
    for victim in doomed(world) {
        crate::tools::tell_mob(
            world,
            victim,
            Span::Danger,
            "Взрывная волна от устройства судного дня разрывает тебя на атомы!",
        );
        crate::health::dust(world, victim);
    }
    crate::round::to_world(
        world,
        Span::Bold,
        "ИИ уничтожил жизнь на станции с помощью устройства судного дня!",
    );
    // BandaStation: `GLOB.station_was_nuked = TRUE`.
    world
        .get_resource_or_insert_with(crate::antag::nukeop::server::NukeOutcome::default)
        .station_was_nuked = true;
    world.resource_mut::<crate::round::Ticker>().force_ending = true;
    info!("the doomsday device went off");
}
