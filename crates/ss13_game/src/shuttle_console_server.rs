//! The emergency shuttle console (server), after tg `/obj/machinery/computer/emergency_shuttle`
//! (`code/modules/shuttle/mobile_port/variants/emergency/emergency_console.dm`).
//!
//! - Early launch: while the shuttle is docked at the station, three IDs with command access
//!   authorize an early launch in the window ([`EmergencyConsoleUi`]); each user may act once per
//!   5 s, IDs of one registered name count once, "repeal" takes the user's ID back, "abort" takes
//!   every ID back. Each change is announced; with enough IDs (or once emagged) the engines start
//!   at once and the shuttle leaves 10 s later ([`crate::shuttle::Emergency::early_launch`]).
//! - Emag: on a docked shuttle with idle engines it adds ten made-up IDs, so the shuttle leaves
//!   10 s later with a "SYSTEM ERROR" announcement; the window shows the IDs as gibberish.
//! - Hijack: an antagonist alt-clicks the console (in the escape shuttle's area, the shuttle
//!   docked or in flight) for a do_after of 5 s over its hijack speed
//!   ([`hijack_speed`]: 0.5 for most antagonists, 1 with a hijack objective, 2 for nuclear
//!   operatives; none for the crew); each completion is a stage (five in all), announced with
//!   scrambled text, after which the console is on a 5 s timeout. In flight a stage lengthens
//!   the flight by 30 s, the last one cuts it to 10 s. A hijacked shuttle flies to the Syndicate
//!   (`crate::shuttle`), which the hijack objective needs.
// shortcut: no examine hints, no `visible_message` to onlookers, no stranded or disabled modes,
// no `movement_force` of the emag; IDs are read once at authorization (a later rename of the
// card does not show); the gibberish is stable per name.

use std::collections::HashMap;

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::spawner::Rng;
use ss13_mapload::{Areas, DmName, DmObj};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use crate::announce::{AnnounceKind, Priority};
use crate::atmos_machinery::machine_say;
use crate::click::{ClickResolved, ResolveClicks};
use crate::construction::Job;
use crate::inventory::{IdCard, Inventory};
use crate::outfit::IdRegistration;
use crate::shuttle::{
    ACCESS_COMMAND, ENGINE_START_SECS, EmergencyShuttle, HIJACK_COMPLETED, ShuttleMode,
};
use crate::shuttle_console::{ABORT, AUTHORIZE, EmergencyConsoleData, EmergencyConsoleUi, REPEAL};
use crate::tools::{Actor, DoAfter, do_after, tell};

const CONSOLE: &str = "/obj/machinery/computer/emergency_shuttle";
const ESCAPE_AREA: &str = "/area/shuttle/escape";
/// The console's `auth_need`.
pub const AUTH_NEED: usize = 3;
/// `SHUTTLE_CONSOLE_ACTION_DELAY`.
pub const ACTION_DELAY_SECS: f64 = 5.0;
/// The console's `hijack_stage_time` (deciseconds) and `hijack_stage_cooldown`.
pub const HIJACK_STAGE_DS: f32 = 50.0;
pub const HIJACK_STAGE_COOLDOWN_SECS: f64 = 5.0;
/// The emag's made-up IDs.
const EMAG_IDS: usize = 10;
const NOTICE1: &str = "sound/announcer/notice/notice1.ogg";
const NOTICE2: &str = "sound/announcer/notice/notice2.ogg";
const STATUS_TITLE: &str = "Emergency Shuttle Status";
const NO_CONNECTION: &str =
    "Error - Network connectivity: Console has lost connection to the shuttle.";

/// One authorizing ID (tg keeps the card itself).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Authorization {
    /// `None` for the emag's made-up IDs.
    pub card: Option<Entity>,
    pub name: String,
    pub job: String,
}

/// `/obj/machinery/computer/emergency_shuttle`.
#[derive(Component, Clone, Debug, Default)]
pub struct EmergencyConsole {
    pub authorized: Vec<Authorization>,
    /// tg `acted_recently`: mob -> when it may act again.
    pub acted_recently: HashMap<Entity, f64>,
    pub emagged: bool,
    /// tg `hijack_last_stage_increase`.
    pub hijack_last_stage: Option<f64>,
}

impl EmergencyConsole {
    /// The `authorize` proc on an ID: false for an ID (or a registered name) already in.
    pub fn authorize(&mut self, card: Entity, name: &str, job: &str) -> bool {
        if self
            .authorized
            .iter()
            .any(|a| a.card == Some(card) || a.name == name)
        {
            return false;
        }
        self.authorized.push(Authorization {
            card: Some(card),
            name: name.to_owned(),
            job: job.to_owned(),
        });
        true
    }

    pub fn repeal(&mut self, card: Entity) {
        self.authorized.retain(|a| a.card != Some(card));
    }

    pub fn remaining(&self) -> usize {
        AUTH_NEED.saturating_sub(self.authorized.len())
    }

    pub fn acted_recently(&self, mob: Entity, now: f64) -> bool {
        self.acted_recently
            .get(&mob)
            .is_some_and(|&until| now < until)
    }
}

/// A user's act on the console window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleAct {
    Authorize,
    Repeal,
    Abort,
}

pub struct ShuttleConsoleServerPlugin;

impl Plugin for ShuttleConsoleServerPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // An emag or an ID on the console (`emag_act`, `item_interaction`).
        app.add_click_handler(ClickPhase::HeldOnTarget, 330, |w, c| {
            !c.modifiers.right
                && c.held.is_some_and(|h| {
                    crate::shuttle_console_server::item_interaction(w, c.client, c.mob, h, c.target)
                })
        });
        app.register_ui::<EmergencyConsoleUi>()
            .add_systems(Startup, setup_consoles.after(crate::shuttle::setup_shuttle))
            .add_systems(
                Update,
                (
                    console_clicks.after(ResolveClicks).before(UiSystems::Open),
                    console_acts.in_set(UiSystems::Handle),
                    process_consoles,
                ),
            );
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn setup_consoles(world: &mut World) {
    let mut objs = world.query::<(Entity, &DmObj)>();
    let consoles: Vec<Entity> = objs
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, CONSOLE))
        .map(|(e, _)| e)
        .collect();
    for e in consoles {
        world.entity_mut(e).insert(EmergencyConsole::default());
    }
}

/// The console stands in the escape shuttle's area (`istype(get_area(src), /area/shuttle/escape)`).
fn on_shuttle(world: &World, console: Entity) -> bool {
    let Some(&pos) = world.get::<TilePos>(console) else {
        return false;
    };
    world
        .resource::<Areas>()
        .area_at(pos)
        .and_then(|a| world.get::<ss13_mapload::DmPower>(a))
        .is_some_and(|p| is_under(&p.path, ESCAPE_AREA))
}

/// tg `get_idcard(hand_first = TRUE)`: the card, its access, registered name and assignment.
fn id_card(world: &World, mob: Entity) -> Option<(Entity, Vec<String>, String, String)> {
    let inventory = world.get::<Inventory>(mob)?;
    [
        inventory.hands[inventory.active],
        inventory.hands[1 - inventory.active],
        inventory.id,
    ]
    .into_iter()
    .flatten()
    .find_map(|item| {
        let card = crate::inventory::get_id(world, item)?;
        let access = world.get::<IdCard>(card)?.access.clone();
        let (name, job) = world
            .get::<IdRegistration>(card)
            .map(|r| (r.registered_name.clone(), r.assignment.clone()))
            .unwrap_or_default();
        Some((card, access, name, job))
    })
}

fn shuttle_mode(world: &World) -> Option<ShuttleMode> {
    world
        .get_resource::<EmergencyShuttle>()
        .map(|s| s.state.mode)
}

/// tg `IS_DOCKED`: docked, engines started or not.
fn is_docked(world: &World) -> bool {
    matches!(
        shuttle_mode(world),
        Some(ShuttleMode::Docked | ShuttleMode::Igniting)
    )
}

fn console_clicks(
    mut clicks: MessageReader<ClickResolved>,
    consoles: Query<&DmName, With<EmergencyConsole>>,
    inventories: Query<&Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok(name) = consoles.get(click.target) else {
            continue;
        };
        if inventories
            .get(click.mob)
            .is_ok_and(|inv| inv.active_item().is_none())
        {
            open.write(OpenUi::new::<EmergencyConsoleUi>(
                click.client,
                click.mob,
                click.target,
                crate::vending::plain_name(&name.name),
            ));
        }
    }
}

/// An emag (`emag_act`) or an ID (`item_interaction`) on the console. True when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<EmergencyConsole>(target).is_none() {
        return false;
    }
    if crate::emag::is_emag(world, held) {
        emag_act(world, client, mob, target);
        return true;
    }
    if crate::inventory::get_id(world, held).is_some() {
        machine_say(
            world,
            target,
            "Please equip your ID card into your ID slot to authenticate.",
        );
        return true;
    }
    false
}

/// tg `emag_act`: ten made-up IDs authorize the launch.
pub fn emag_act(world: &mut World, client: Entity, mob: Entity, console: Entity) -> bool {
    if !is_docked(world) {
        return false;
    }
    let engines_started = shuttle_mode(world) == Some(ShuttleMode::Igniting);
    if world
        .get::<EmergencyConsole>(console)
        .is_none_or(|c| c.emagged)
        || engines_started
    {
        tell(
            world,
            client,
            Span::Warning,
            "shuttle already about to launch!",
        );
        return false;
    }
    let mut fakes = Vec::with_capacity(EMAG_IDS);
    let mut rng = Rng::new(world.resource::<Time>().elapsed().as_nanos() as u64 ^ 0x454d_4147);
    if let Some(mut jobs) = world.get_resource_mut::<crate::jobs::Jobs>() {
        let taken = Default::default();
        for _ in 0..EMAG_IDS {
            let name = jobs
                .random_name(&taken)
                .unwrap_or_else(|| "John Doe".into());
            let n = jobs.data.jobs.len() as i64;
            let job = if n > 0 {
                jobs.data.jobs[rng.range(0, n - 1) as usize].title.clone()
            } else {
                String::new()
            };
            fakes.push(Authorization {
                card: None,
                name,
                job,
            });
        }
    }
    let left = world
        .get_resource::<EmergencyShuttle>()
        .and_then(|s| s.state.timer.left(now(world)))
        .unwrap_or(0.0);
    info!("{mob} emagged the emergency shuttle {left:.0} seconds before launch");
    if let Some(mut c) = world.get_mut::<EmergencyConsole>(console) {
        c.emagged = true;
        c.authorized.extend(fakes);
    }
    process(world, console);
    true
}

/// tg `process`: enough IDs (or the emag) start the engines.
fn process(world: &mut World, console: Entity) {
    let t = now(world);
    let Some(c) = world.get::<EmergencyConsole>(console) else {
        return;
    };
    if c.authorized.len() < AUTH_NEED && !c.emagged {
        return;
    }
    let emagged = c.emagged;
    let launched = world
        .get_resource_mut::<EmergencyShuttle>()
        .is_some_and(|mut s| s.state.early_launch(t));
    if launched {
        let title = if emagged { "SYSTEM ERROR:" } else { "" };
        let text = format!(
            "Эвакуационный шаттл будет запущен через {} секунд",
            ENGINE_START_SECS as u32
        );
        crate::security_level::minor_announce(world, title, &text, true);
    }
}

fn process_consoles(world: &mut World) {
    if !world.contains_resource::<EmergencyShuttle>() {
        return;
    }
    let mut q = world.query_filtered::<Entity, With<EmergencyConsole>>();
    let consoles: Vec<Entity> = q.iter(world).collect();
    for console in consoles {
        process(world, console);
    }
}

/// Stable `Gibberish(text)`: about half the characters become symbols.
fn gibberish(text: &str) -> String {
    const SYMBOLS: [char; 10] = ['#', '@', '*', '&', '%', '$', '/', '<', '>', ';'];
    let seed = text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    });
    let mut rng = Rng::new(seed);
    text.chars()
        .map(|c| {
            if rng.prob(50.0) {
                SYMBOLS[rng.range(0, SYMBOLS.len() as i64 - 1) as usize]
            } else {
                c
            }
        })
        .collect()
}

impl ServerUi for EmergencyConsoleUi {
    type Source = &'static EmergencyConsole;
    type UserParam = (Option<Res<'static, EmergencyShuttle>>, Res<'static, Time>);
    type Act = ConsoleAct;

    fn static_data(_: &EmergencyConsole) {}

    fn data(
        console: &EmergencyConsole,
        user: Entity,
        (shuttle, time): &bevy::ecs::system::SystemParamItem<'_, '_, Self::UserParam>,
    ) -> EmergencyConsoleData {
        let t = time.elapsed_secs_f64();
        let mode = shuttle.as_deref().map(|s| s.state.mode);
        let left = shuttle
            .as_deref()
            .and_then(|s| s.state.timer.left(t))
            .unwrap_or(0.0);
        let engines_started = mode == Some(ShuttleMode::Igniting);
        let docked = mode == Some(ShuttleMode::Docked) || engines_started;
        let authorizations = console
            .authorized
            .iter()
            .map(|a| {
                if console.emagged {
                    (gibberish(&a.name), gibberish(&a.job))
                } else {
                    (a.name.clone(), a.job.clone())
                }
            })
            .collect();
        EmergencyConsoleData {
            timer_str: crate::antag::nukeop::server::timer_str(left),
            engines_started,
            authorizations_remaining: console.remaining() as u32,
            authorizations,
            enabled: docked && !engines_started && !console.acted_recently(user, t),
            emagged: console.emagged,
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<ConsoleAct, UiReject> {
        match action {
            AUTHORIZE => Ok(ConsoleAct::Authorize),
            REPEAL => Ok(ConsoleAct::Repeal),
            ABORT => Ok(ConsoleAct::Abort),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// tg `ui_act`: docked with idle engines, on the shuttle, an ID with command access, not within
/// 5 s of the user's last act; then the authorization changes and is announced.
fn console_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<EmergencyConsoleUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, ConsoleAct)> = acts
        .read(world.resource::<Messages<UiActed<EmergencyConsoleUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act))
        .collect();
    for (client, actor, console, act) in pending {
        handle_act(world, client, actor, console, act);
    }
}

pub fn handle_act(
    world: &mut World,
    client: Entity,
    actor: Entity,
    console: Entity,
    act: ConsoleAct,
) {
    let t = now(world);
    if world.get::<EmergencyConsole>(console).is_none()
        || shuttle_mode(world) != Some(ShuttleMode::Docked)
        || world.get::<crate::ghost::Ghost>(actor).is_some()
    {
        return;
    }
    if !on_shuttle(world, console) {
        machine_say(world, console, NO_CONNECTION);
        return;
    }
    let Some((card, access, name, job)) = id_card(world, actor) else {
        tell(world, client, Span::Warning, "You don't have an ID.");
        return;
    };
    if !access.iter().any(|a| a == ACCESS_COMMAND) {
        tell(
            world,
            client,
            Span::Warning,
            "The access level of your card is not high enough.",
        );
        return;
    }
    let Some(mut c) = world.get_mut::<EmergencyConsole>(console) else {
        return;
    };
    if c.acted_recently(actor, t) {
        return;
    }
    let old_len = c.authorized.len();
    match act {
        ConsoleAct::Authorize => {
            if c.authorize(card, &name, &job) {
                info!("{actor} ({name}) authorized the early shuttle launch");
            }
        }
        ConsoleAct::Repeal => c.repeal(card),
        ConsoleAct::Abort => c.authorized.clear(),
    }
    let new_len = c.authorized.len();
    let remaining = c.remaining();
    c.acted_recently.retain(|_, &mut until| t < until);
    c.acted_recently.insert(actor, t + ACTION_DELAY_SECS);
    if old_len != new_len {
        if new_len > 0 && remaining > 0 {
            Priority::new(
                format!("{remaining} авторизации необходимо для раннего запуска шаттла."),
                STATUS_TITLE,
            )
            .sound(NOTICE1)
            .kind(AnnounceKind::Priority)
            .important()
            .send(world);
        }
        if new_len < old_len {
            Priority::new(
                format!(
                    "Авторизация на ранний запуск отозвана, {remaining} авторизации необходимо."
                ),
                STATUS_TITLE,
            )
            .sound(NOTICE2)
            .kind(AnnounceKind::Priority)
            .send(world);
        }
    }
    process(world, console);
}

// --- Hijacking ---------------------------------------------------------------------------------

/// tg `mind.get_hijack_speed()`: the fastest of the mob's antagonist datums (tg
/// `/datum/antagonist/hijack_speed`: a datum with a hijack objective hijacks at 1).
pub fn hijack_speed(world: &World, mob: Entity) -> f32 {
    use crate::antag::objectives::{Objective, ObjectiveKind};
    let Ok(e) = world.get_entity(mob) else {
        return 0.0;
    };
    let hijacker = |objectives: &[Objective]| {
        objectives
            .iter()
            .any(|o| matches!(o.kind, ObjectiveKind::Hijack))
    };
    let mut speed: f32 = 0.0;
    if let Some(t) = e.get::<crate::antag::traitor::Traitor>() {
        let hijack = hijacker(&t.objectives) || matches!(t.ending, ObjectiveKind::Hijack);
        speed = speed.max(if hijack { 1.0 } else { 0.5 });
    }
    if let Some(b) = e.get::<crate::antag::brother::Brother>() {
        speed = speed.max(if hijacker(&b.objectives) { 1.0 } else { 0.5 });
    }
    if e.contains::<crate::antag::nukeop::server::Nukeop>() {
        speed = speed.max(2.0);
    }
    if e.contains::<crate::antag::changeling::server::Changeling>()
        || e.contains::<crate::antag::heretic::server::Heretic>()
        || e.contains::<crate::antag::vampire::server::Vampire>()
        || e.contains::<crate::antag::wizard::server::Wizard>()
    {
        speed = speed.max(0.5);
    }
    speed
}

fn hijacking(world: &mut World, console: Entity) -> bool {
    let mut q = world.query::<&DoAfter>();
    q.iter(world)
        .any(|d| d.target == console && d.job == Job::Hijack)
}

/// tg `click_alt` -> `attempt_hijack_stage` (the click's reach is checked by the caller). True
/// when the click was the console's.
pub fn click_alt(world: &mut World, client: Entity, mob: Entity, console: Entity) -> bool {
    if world.get::<EmergencyConsole>(console).is_none() {
        return false;
    }
    attempt_hijack_stage(world, client, mob, console);
    true
}

fn attempt_hijack_stage(world: &mut World, client: Entity, mob: Entity, console: Entity) {
    if crate::status_effects::incapacitation(world, mob).hands_blocked {
        tell(
            world,
            client,
            Span::Warning,
            "You need your hands free before you can manipulate the console.",
        );
        return;
    }
    if !on_shuttle(world, console) {
        machine_say(world, console, NO_CONNECTION);
        return;
    }
    let speed = hijack_speed(world, mob);
    if speed <= 0.0 {
        tell(
            world,
            client,
            Span::Warning,
            "You manage to open a user-mode shell on the console, and hundreds of lines of \
             debugging output fly through your vision. It is probably best to leave this alone.",
        );
        return;
    }
    let Some(state) = world
        .get_resource::<EmergencyShuttle>()
        .map(|s| s.state.clone())
    else {
        return;
    };
    // `EMERGENCY_AT_LEAST_DOCKED`.
    if matches!(
        state.mode,
        ShuttleMode::Idle | ShuttleMode::Recall | ShuttleMode::Call
    ) {
        tell(
            world,
            client,
            Span::Warning,
            "The flight plans for the shuttle haven't been loaded yet, you can't hack this right now.",
        );
        return;
    }
    if hijacking(world, console) {
        return;
    }
    if state.is_hijacked() {
        tell(
            world,
            client,
            Span::Warning,
            "The emergency shuttle is already loaded with a corrupt navigational payload. What \
             more do you want from it?",
        );
        return;
    }
    let t = now(world);
    let timeout = world
        .get::<EmergencyConsole>(console)
        .and_then(|c| c.hijack_last_stage)
        .is_some_and(|last| last >= t - HIJACK_STAGE_COOLDOWN_SECS);
    if timeout {
        machine_say(
            world,
            console,
            "Error - Catastrophic software error detected. Input is currently on timeout.",
        );
        return;
    }
    let item = world.get::<Inventory>(mob).and_then(Inventory::active_item);
    let actor = Actor { client, mob, item };
    if !do_after(
        world,
        actor,
        console,
        HIJACK_STAGE_DS / speed,
        0,
        Job::Hijack,
    ) {
        return;
    }
    let verb = if state.hijack_status == 0 {
        "begin"
    } else {
        "continue"
    };
    tell(
        world,
        client,
        Span::BoldWarning,
        format!("You {verb} to override the console's navigational protocols."),
    );
    machine_say(world, console, "Software override initiated.");
    info!("{mob} is hijacking the emergency shuttle console {console}");
}

/// The hijack do_after's end: `increase_hijack_stage`, if the shuttle can still take it.
pub fn complete_hijack(world: &mut World, mob: Entity, action: &DoAfter) {
    let console = action.target;
    if world.get::<EmergencyConsole>(console).is_none() {
        return;
    }
    let t = now(world);
    let Some(mut shuttle) = world.get_resource_mut::<EmergencyShuttle>() else {
        return;
    };
    if matches!(
        shuttle.state.mode,
        ShuttleMode::Idle | ShuttleMode::Recall | ShuttleMode::Call | ShuttleMode::Endgame
    ) || shuttle.state.is_hijacked()
    {
        return;
    }
    let stage = shuttle.state.hijack_stage(t);
    let in_flight = shuttle.state.mode == ShuttleMode::Escape;
    if let Some(mut c) = world.get_mut::<EmergencyConsole>(console) {
        c.hijack_last_stage = Some(t);
    }
    announce_hijack_stage(world, stage, in_flight);
    machine_say(
        world,
        console,
        "Navigational protocol error! Rebooting systems.",
    );
    tell(
        world,
        action.client,
        Span::Notice,
        format!(
            "You reprogram some of the console's programming, putting it on timeout for {} \
             seconds.",
            HIJACK_STAGE_COOLDOWN_SECS as u32
        ),
    );
    info!("{mob} hijacked the emergency shuttle console: stage {stage} of {HIJACK_COMPLETED}");
}

/// tg `scramble_message_replace_chars(text, replaceprob)`: whitespace stays.
pub fn scramble(text: &str, percent: f32, rng: &mut Rng) -> String {
    const CHARS: [char; 8] = ['$', '@', '!', '#', '%', '^', '&', '*'];
    text.chars()
        .map(|c| {
            if !c.is_whitespace() && rng.prob(percent) {
                CHARS[rng.range(0, CHARS.len() as i64 - 1) as usize]
            } else {
                c
            }
        })
        .collect()
}

/// The text of hijack stage `stage` (tg `announce_hijack_stage`), before the 10% scramble.
pub fn hijack_stage_text(stage: u8, in_flight: bool, rng: &mut Rng) -> Option<String> {
    Some(match stage {
        1 => "ОШИБКА АУТЕНТИФИКАЦИИ. ОШИБКА АУТЕНТИФИКАЦИИ. ОШИБКА АУТЕНТ###### Добро пожаловать, \
              оператор ДЖОН ДОУ."
            .to_owned(),
        2 => format!(
            "Ошибка маршрута: доступ не авторизован \"IS_AUTHORIZED\". Пожалуйста, попробуйте \
             сноваАА{}.",
            scramble("сновасновасновасноваснова", 70.0, rng)
        ),
        3 => "Несовпадение CRC в позиции ~h~ буфера расчётного маршрута. Инициирован полный сброс \
              FTL_NAVIGATION_SERVICES. Память расшифрована для автоматического восстановления."
            .to_owned(),
        4 => {
            "~ACS_directive module_load(cyberdyne.exploit.nanotrasen.shuttlenav)... Несоответствие \
              ключа НТ. Подтвердить загрузку? ДА...###Перезагрузка завершена. $SET \
              transponder_state = 0; Установлено соединение с двигателями..."
                .to_owned()
        }
        HIJACK_COMPLETED => {
            let hashes = |rng: &mut Rng, n: usize| scramble(&"#".repeat(n), 100.0, rng);
            let flight = if in_flight {
                format!(
                    "Отклонение от текущего маршрута - выход из Блюспейса через... {} секунд(-ы).",
                    crate::shuttle::HIJACK_COMPLETION_FLIGHT_SECS as u32
                )
            } else {
                String::new()
            };
            format!(
                "СИСТЕМА ПЕРЕЗАПИСАНА - Переназначение маршрута на [{}] ({}/{}/{}) {{AUTH - ROOT \
                 (uid: 0)}}.{flight}",
                hashes(rng, 11),
                hashes(rng, 7),
                hashes(rng, 7),
                hashes(rng, 7)
            )
        }
        _ => return None,
    })
}

fn announce_hijack_stage(world: &mut World, stage: u8, in_flight: bool) {
    let mut rng = Rng::new(world.resource::<Time>().elapsed().as_nanos() as u64 ^ 0x4849_4a41);
    let Some(text) = hijack_stage_text(stage, in_flight, &mut rng) else {
        return;
    };
    let text = scramble(&text, 10.0, &mut rng);
    crate::security_level::minor_announce(world, "Эвакуационный шаттл", &text, true);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorizations_count_each_id_and_name_once() {
        let mut world = World::new();
        let (a, b, c) = (
            world.spawn_empty().id(),
            world.spawn_empty().id(),
            world.spawn_empty().id(),
        );
        let mut console = EmergencyConsole::default();
        assert!(console.authorize(a, "Алиса", "Captain"));
        assert!(!console.authorize(a, "Алиса", "Captain"), "the same ID");
        assert!(!console.authorize(b, "Алиса", "Captain"), "the same name");
        assert!(console.authorize(b, "Боб", "Head of Personnel"));
        assert_eq!(console.remaining(), 1);
        console.repeal(a);
        assert_eq!(console.remaining(), 2);
        assert!(console.authorize(c, "Ева", "Chief Engineer"));
        assert!(console.authorize(a, "Алиса", "Captain"));
        assert_eq!(console.remaining(), 0);
        console.acted_recently.insert(a, 5.0);
        assert!(console.acted_recently(a, 4.9));
        assert!(!console.acted_recently(a, 5.0));
        assert!(!console.acted_recently(b, 0.0));
    }

    #[test]
    fn scramble_keeps_whitespace_and_length() {
        let mut rng = Rng::new(1);
        let all = scramble("ab cd", 100.0, &mut rng);
        assert_eq!(all.chars().count(), 5);
        assert_eq!(all.chars().nth(2), Some(' '));
        assert!(
            all.chars()
                .filter(|c| *c != ' ')
                .all(|c| "$@!#%^&*".contains(c))
        );
        assert_eq!(scramble("ab cd", 0.0, &mut rng), "ab cd");
        assert_eq!(gibberish("Алиса"), gibberish("Алиса"), "stable");
        assert!(hijack_stage_text(0, false, &mut rng).is_none());
        assert!(hijack_stage_text(6, false, &mut rng).is_none());
        let last = hijack_stage_text(HIJACK_COMPLETED, true, &mut rng).unwrap();
        assert!(last.contains("10 секунд"), "{last}");
    }
}
