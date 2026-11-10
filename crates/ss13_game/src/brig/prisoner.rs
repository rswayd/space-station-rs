//! The prisoner management console (server) after tg `/obj/machinery/computer/prisoner` and
//! `/management` (`computer/prisoner/*.dm`), with the security implants' console hooks
//! (`is_shown_on_console`, `get_management_console_data`/`_buttons`,
//! `handle_management_console_action` of `implant.dm`, `implant_track.dm`, `implant_chem.dm`).
//!
//! A click with an empty hand on a powered console opens `PrisonerManagement`; "login" needs
//! `ACCESS_BRIG` on the user's ID. Logged in, a prisoner ID card goes in (from the active hand)
//! and out, its goal is set (1..1000) and its points reset; every tracking implant on the
//! console's level (connected station levels count), until ten minutes after its host died, shows
//! its host and area and can send a message to the host's mind; every chem implant on the level
//! shows its volume and injects 1, 5 or 10 units; every beacon implant until ten minutes after
//! its host died shows whether it can be teleported to and whether the host's air is safe; each
//! can self-destruct. Questions (goal,
//! message, self-destruct) are tgui prompts, re-checked when answered.
//!
//! Every act but `login` needs the console logged in (tg checks only in the UI), and the
//! window's data is empty until then (tg sends the implants to anyone).
// shortcut: no silicon access, alt-click eject, logs, console deconstruction (the card stays
// inside), chem implants synced to a signaller (`frequency`). Implant categories are sections,
// not tabs. A beacon's environment check (`is_safe_turf`) looks at the air's pressure and
// temperature only.

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::construct::DmObj;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmName};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::*;
use crate::click::{ClickResolved, ResolveClicks};
use crate::health::{Health, Stat};
use crate::implants::{ImplantKind, Implants, chem_inject, remove_implant, tracked};
use crate::inventory::{CarriedBy, IdCard, Inventory, check_access};
use crate::power::Unpowered;
use crate::server::PlayerMob;
use crate::sound::{PlaySound, playsound};
use crate::storage::StoredIn;

pub const MANAGEMENT_CONSOLE: &str = "/obj/machinery/computer/prisoner/management";
pub const PRISONER_CARD: &str = "/obj/item/card/id/advanced/prisoner";
/// `req_access = list(ACCESS_BRIG)`.
const ACCESS_BRIG: &str = "brig";
/// `tracking/lifespan_postmortem`.
const TRACKING_POSTMORTEM_SECS: f64 = 10.0 * 60.0;
/// `chem/implant_sizes`.
pub const IMPLANT_SIZES: [u32; 3] = [1, 5, 10];
/// `tgui_input_number(..., 100, 1000, 1)` of `set_id_goal`.
const GOAL_DEFAULT: f64 = 100.0;
const GOAL_MAX: f64 = 1000.0;
const GOAL_MIN: f64 = 1.0;
/// `MAX_MESSAGE_LEN`.
const MAX_MESSAGE_LEN: u32 = 1024;
const INSERT_SOUND: &str = "sound/machines/terminal/terminal_insert_disc.ogg";
const CONFIRM_SOUND: &str = "sound/machines/terminal/terminal_prompt_confirm.ogg";
const ON_SOUND: &str = "sound/machines/terminal/terminal_on.ogg";
const OFF_SOUND: &str = "sound/machines/terminal/terminal_off.ogg";
const ERROR_SOUND: &str = "sound/machines/terminal/terminal_error.ogg";

/// A prisoner management console: `authenticated`, `contained_id`, and the implant list last
/// read for its window.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct PrisonerConsole {
    pub authenticated: bool,
    pub contained_id: Option<Entity>,
    /// `inserted_id` and `implants` of `ui_data`, refreshed each tick while logged in.
    pub view: PrisonerManagementData,
}

/// `/obj/item/card/id/advanced/prisoner`: `goal` and `points` of the labor camp.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PrisonerCard {
    pub goal: u32,
    pub points: u32,
}

/// On an implanted mob that died: when (tg `timeofdeath`), for the tracking implant's
/// `lifespan_postmortem`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct DiedAt(pub f64);

/// On a prompt: what the console asked.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
enum Question {
    Goal { console: Entity, card: Entity },
    SelfDestruct { console: Entity, implant: u32 },
    Warn { console: Entity, implant: u32 },
}

pub struct PrisonerConsolePlugin;

impl Plugin for PrisonerConsolePlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `item_interaction` of the prisoner console: a prisoner ID goes in.
        app.add_click_handler(ClickPhase::HeldOnTarget, 4900, |w, c| {
            c.held.is_some_and(|h| {
                crate::brig::prisoner::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        app.register_ui::<PrisonerManagementUi>()
            .add_observer(attach_console)
            .add_observer(attach_card)
            .add_systems(
                Update,
                (
                    open_console.after(ResolveClicks).before(UiSystems::Open),
                    console_acts.in_set(UiSystems::Handle),
                    answers
                        .in_set(UiSystems::Handle)
                        .after(PromptSystems::Answer)
                        .after(console_acts),
                    refresh_views
                        .after(UiSystems::Handle)
                        .before(UiSystems::Send),
                    record_deaths,
                ),
            );
    }
}

fn attach_console(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    if objs
        .get(add.entity)
        .is_ok_and(|o| is_under(&o.path, MANAGEMENT_CONSOLE))
    {
        commands
            .entity(add.entity)
            .insert(PrisonerConsole::default());
    }
}

fn attach_card(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if items
        .get(add.entity)
        .is_ok_and(|i| is_under(&i.path, PRISONER_CARD))
    {
        commands.entity(add.entity).insert(PrisonerCard::default());
    }
}

/// tg `timeofdeath` of implanted mobs.
fn record_deaths(
    mobs: Query<(Entity, &Stat, Has<DiedAt>), (Changed<Stat>, With<Implants>)>,
    time: Res<Time>,
    mut commands: Commands,
) {
    for (mob, stat, died) in &mobs {
        match (*stat == Stat::Dead, died) {
            (true, false) => {
                commands.entity(mob).insert(DiedAt(time.elapsed_secs_f64()));
            }
            (false, true) => {
                commands.entity(mob).remove::<DiedAt>();
            }
            _ => {}
        }
    }
}

/// `attack_hand` -> `ui_interact` of a powered console, with an empty hand.
fn open_console(
    mut clicks: MessageReader<ClickResolved>,
    consoles: Query<
        &DmName,
        (
            With<PrisonerConsole>,
            Without<Unpowered>,
            Without<super::gulag::GulagConsole>,
        ),
    >,
    inventories: Query<&Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok(name) = consoles.get(click.target) else {
            continue;
        };
        if inventories
            .get(click.mob)
            .is_ok_and(|i| i.active_item().is_some())
        {
            continue;
        }
        let title = crate::vending::plain_name(&name.name).to_owned();
        open.write(OpenUi::new::<PrisonerManagementUi>(
            click.client,
            click.mob,
            click.target,
            title,
        ));
    }
}

impl ServerUi for PrisonerManagementUi {
    type Source = &'static PrisonerConsole;
    type UserParam = Query<'static, 'static, (), With<Health>>;
    type Act = PrisonerAct;

    fn static_data(_: &PrisonerConsole) {}

    fn data(
        console: &PrisonerConsole,
        user: Entity,
        living: &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> PrisonerManagementData {
        // `(authenticated && isliving(user))`.
        if !console.authenticated || !living.contains(user) {
            return PrisonerManagementData::default();
        }
        PrisonerManagementData {
            authorized: true,
            ..console.view.clone()
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<PrisonerAct, UiReject> {
        Ok(match action {
            LOGIN => PrisonerAct::Login,
            LOGOUT => PrisonerAct::Logout,
            INSERT_ID => PrisonerAct::InsertId,
            EJECT_ID => PrisonerAct::EjectId,
            SET_ID_GOAL => PrisonerAct::SetGoal,
            RESET_ID => PrisonerAct::ResetId,
            HANDLE_IMPLANT => PrisonerAct::Implant {
                implant: params
                    .get("implant_ref")
                    .and_then(|v| v.parse().ok())
                    .ok_or_else(|| UiReject::BadParams("implant_ref: expected a ref".into()))?,
                action: params.get("implant_action").unwrap_or_default().to_owned(),
                amount: params.get("amount").and_then(|v| v.parse().ok()),
            },
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

/// A parsed `PrisonerManagement` act.
#[derive(Clone, Debug, PartialEq)]
pub enum PrisonerAct {
    Login,
    Logout,
    InsertId,
    EjectId,
    SetGoal,
    ResetId,
    Implant {
        implant: u32,
        action: String,
        amount: Option<u32>,
    },
}

fn console_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<PrisonerManagementUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, PrisonerAct)> = acts
        .read(world.resource::<Messages<UiActed<PrisonerManagementUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, console, act) in pending {
        if let Err(why) = apply(world, client, actor, console, act.clone()) {
            debug!("PrisonerManagement {act:?} on {console} refused: {why}");
        }
    }
}

fn sound(world: &mut World, at: Entity, file: &str) {
    if let Some(&pos) = world.get::<TilePos>(at) {
        playsound(world, PlaySound::new(pos, file, 50.0, false));
    }
}

fn access_of(world: &World, user: Entity) -> Vec<String> {
    world
        .get::<Inventory>(user)
        .and_then(|i| i.id_card(world))
        .and_then(|c| world.get::<IdCard>(c))
        .map(|c| c.access.clone())
        .unwrap_or_default()
}

fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<Name>(e)
        .map(|n| n.to_string())
        .or_else(|| {
            world
                .get::<DmName>(e)
                .map(|n| crate::vending::plain_name(&n.name).to_owned())
        })
        .unwrap_or_default()
}

/// The mob holding the implant `id` of a kind the console shows, and its kind.
fn implant_host(world: &mut World, id: u32) -> Option<(Entity, ImplantKind)> {
    [
        ImplantKind::Tracking,
        ImplantKind::Chem,
        ImplantKind::Beacon,
    ]
    .into_iter()
    .find_map(|kind| {
        tracked(world, kind)
            .into_iter()
            .find(|&(_, i)| i == id)
            .map(|(mob, _)| (mob, kind))
    })
}

/// `is_valid_z_level(get_turf(console), get_turf(imp_in))`: the same level, or both on the
/// station's connected levels.
fn same_level(world: &World, a: Entity, b: Entity) -> bool {
    let (Some(a), Some(b)) = (world.get::<TilePos>(a), world.get::<TilePos>(b)) else {
        return false;
    };
    let levels = crate::events::controller::station_levels(world);
    a.z == b.z || (levels.contains(&a.z) && levels.contains(&b.z))
}

/// `is_shown_on_console(console)` of an implant of `kind` in `mob`.
fn shown(world: &World, console: Entity, mob: Entity, kind: ImplantKind) -> bool {
    // `beacon/is_shown_on_console`: any level.
    if kind == ImplantKind::Beacon {
        return crate::implants::beacon_live(world, mob);
    }
    if kind == ImplantKind::Tracking {
        let now = world.resource::<Time>().elapsed_secs_f64();
        let expired = world.get::<Stat>(mob) == Some(&Stat::Dead)
            && world
                .get::<DiedAt>(mob)
                .is_some_and(|d| d.0 + TRACKING_POSTMORTEM_SECS < now);
        if expired {
            return false;
        }
    }
    same_level(world, console, mob)
}

/// `user.can_perform_action(console, NEED_DEXTERITY)` after a prompt, and the console still
/// works and is logged in.
fn can_still_use(world: &World, user: Entity, console: Entity) -> bool {
    world.get::<Stat>(user) == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, user).incapacitated
        && world.get::<Unpowered>(console).is_none()
        && world
            .get::<PrisonerConsole>(console)
            .is_some_and(|c| c.authenticated)
        && crate::storage::in_reach(world, user, console)
}

/// `management/ui_act` at the moment of effect.
fn apply(
    world: &mut World,
    client: Entity,
    user: Entity,
    console: Entity,
    act: PrisonerAct,
) -> Result<(), &'static str> {
    let state = world
        .get::<PrisonerConsole>(console)
        .cloned()
        .ok_or("not a console")?;
    if world.get::<Unpowered>(console).is_some() {
        return Err("unpowered");
    }
    if !state.authenticated && act != PrisonerAct::Login {
        return Err("not logged in");
    }
    if world.get::<Health>(user).is_some() {
        sound(world, console, CONFIRM_SOUND);
    }
    match act {
        PrisonerAct::Login => {
            let ok = check_access(&[ACCESS_BRIG.to_owned()], &[], &access_of(world, user));
            if ok && let Some(mut c) = world.get_mut::<PrisonerConsole>(console) {
                c.authenticated = true;
            }
            sound(world, console, if ok { ON_SOUND } else { ERROR_SOUND });
        }
        PrisonerAct::Logout => {
            if let Some(mut c) = world.get_mut::<PrisonerConsole>(console) {
                c.authenticated = false;
            }
            sound(world, console, OFF_SOUND);
        }
        PrisonerAct::InsertId => {
            let held = world
                .get::<Inventory>(user)
                .and_then(Inventory::active_item);
            if let Some(held) = held {
                id_insert(world, client, user, console, held);
            }
        }
        PrisonerAct::EjectId => id_eject(world, client, user, console),
        PrisonerAct::SetGoal => {
            let card = state.contained_id.ok_or("no card")?;
            let prompt = ask_world(
                world,
                Ask::new(
                    client,
                    user,
                    "Prisoner Management",
                    "Enter the prisoner's goal",
                    PromptKind::Number {
                        default: GOAL_DEFAULT,
                        min: GOAL_MIN,
                        max: GOAL_MAX,
                        round: true,
                    },
                ),
            );
            world
                .entity_mut(prompt)
                .insert(Question::Goal { console, card });
        }
        PrisonerAct::ResetId => {
            let card = state.contained_id.ok_or("no card")?;
            if let Some(mut c) = world.get_mut::<PrisonerCard>(card) {
                c.points = 0;
            }
        }
        PrisonerAct::Implant {
            implant,
            action,
            amount,
        } => {
            // `locate(params["implant_ref"]) in GLOB.tracked_implants`, shown on this console.
            let (mob, kind) = implant_host(world, implant).ok_or("no such implant")?;
            if !shown(world, console, mob, kind) {
                return Err("not shown");
            }
            let question = match (action.as_str(), kind) {
                (SELF_DESTRUCT, _) => Some((
                    Question::SelfDestruct { console, implant },
                    "You sure?",
                    "Activation will harmlessly self-destruct this implant. Proceed?".to_owned(),
                    PromptKind::Alert {
                        buttons: vec!["Yes".to_owned(), "No".to_owned()],
                        autofocus: true,
                    },
                )),
                (WARN, ImplantKind::Tracking) => Some((
                    Question::Warn { console, implant },
                    "Messaging",
                    format!(
                        "What warning do you want to send to {}?",
                        name_of(world, mob)
                    ),
                    PromptKind::Text {
                        default: String::new(),
                        max_length: MAX_MESSAGE_LEN,
                        multiline: false,
                    },
                )),
                (INJECT, ImplantKind::Chem) => {
                    let amount = amount
                        .filter(|a| IMPLANT_SIZES.contains(a))
                        .ok_or("bad amount")?;
                    chem_inject(world, mob, implant, amount as f32);
                    None
                }
                _ => return Err("unknown implant action"),
            };
            if let Some((question, title, message, kind)) = question {
                let prompt = ask_world(world, Ask::new(client, user, title, message, kind));
                world.entity_mut(prompt).insert(question);
            }
        }
    }
    Ok(())
}

/// `id_insert(user, new_id)`: a prisoner ID from the active hand into the empty slot.
pub(super) fn id_insert(
    world: &mut World,
    client: Entity,
    user: Entity,
    console: Entity,
    card: Entity,
) {
    if world.get::<PrisonerCard>(card).is_none() {
        return;
    }
    if world
        .get::<PrisonerConsole>(console)
        .is_some_and(|c| c.contained_id.is_some())
    {
        crate::tools::tell(world, client, Span::Warning, "Нет свободного слота!");
        return;
    }
    {
        let Some(mut inventory) = world.get_mut::<Inventory>(user) else {
            return;
        };
        let Some(hand) = inventory.hands.iter().position(|h| *h == Some(card)) else {
            return;
        };
        inventory.hands[hand] = None;
    }
    world.entity_mut(card).remove::<CarriedBy>();
    crate::storage::put_inside(world, card, console);
    if let Some(mut c) = world.get_mut::<PrisonerConsole>(console) {
        c.contained_id = Some(card);
    }
    crate::tools::tell(world, client, Span::Notice, "ID-карта вставлена.");
    sound(world, console, INSERT_SOUND);
}

/// `id_eject(user)`: the card into a free hand of an adjacent user, else onto the tile.
pub(super) fn id_eject(world: &mut World, client: Entity, user: Entity, console: Entity) {
    let Some(card) = world
        .get::<PrisonerConsole>(console)
        .and_then(|c| c.contained_id)
    else {
        crate::tools::tell(world, client, Span::Warning, "Нет ID-карты!");
        return;
    };
    if let Some(mut c) = world.get_mut::<PrisonerConsole>(console) {
        c.contained_id = None;
    }
    if world.get::<StoredIn>(card).is_none_or(|s| s.0 != console) {
        return;
    }
    let Some(pos) = world.get::<TilePos>(console).copied() else {
        return;
    };
    let hand = crate::storage::in_reach(world, user, console)
        .then(|| world.get::<Inventory>(user))
        .flatten()
        .and_then(|i| {
            [i.active, 1 - i.active]
                .into_iter()
                .find(|&h| i.hands[h].is_none())
        });
    match hand {
        Some(h) => {
            world
                .entity_mut(card)
                .remove::<StoredIn>()
                .insert(CarriedBy(user));
            if let Some(mut inventory) = world.get_mut::<Inventory>(user) {
                inventory.hands[h] = Some(card);
            }
        }
        None => crate::storage::take_out(world, card, pos),
    }
    crate::tools::tell(world, client, Span::Notice, "ID-карта извлечена.");
    sound(world, console, INSERT_SOUND);
}

/// Prompt answers, re-checked as tg does after its sleeps.
fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answered: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, user, answer) in answered {
        let Some(question) = world.get::<Question>(prompt).copied() else {
            continue;
        };
        let console = match question {
            Question::Goal { console, .. }
            | Question::SelfDestruct { console, .. }
            | Question::Warn { console, .. } => console,
        };
        if !can_still_use(world, user, console) {
            continue;
        }
        match (question, answer) {
            (Question::Goal { card, .. }, Some(PromptAnswer::Number(n))) => {
                let inside = world
                    .get::<PrisonerConsole>(console)
                    .is_some_and(|c| c.contained_id == Some(card));
                if inside && let Some(mut c) = world.get_mut::<PrisonerCard>(card) {
                    c.goal = n.clamp(GOAL_MIN, GOAL_MAX) as u32;
                }
            }
            (Question::SelfDestruct { implant, .. }, Some(PromptAnswer::Button(0))) => {
                let Some((mob, _)) = implant_host(world, implant) else {
                    continue;
                };
                tell_mob(
                    world,
                    mob,
                    "You feel a tiny jolt from inside of you as one of your implants fizzles out.",
                );
                crate::emp::sparks(world, mob);
                remove_implant(world, mob, implant, true);
            }
            (Question::Warn { implant, .. }, Some(PromptAnswer::Text(text))) => {
                let text = crate::records::console::sanitize(&text, MAX_MESSAGE_LEN as usize);
                if text.is_empty() {
                    continue;
                }
                let Some((mob, _)) = implant_host(world, implant) else {
                    continue;
                };
                tell_mob(
                    world,
                    mob,
                    format!("You hear a voice in your head saying: '{text}'"),
                );
            }
            _ => {}
        }
    }
}

fn tell_mob(world: &mut World, mob: Entity, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        crate::tools::tell(world, client, Span::Notice, text);
    }
}

/// `inserted_id` and `implants` of every logged-in console.
fn refresh_views(world: &mut World) {
    let mut q = world.query::<(Entity, &PrisonerConsole)>();
    let consoles: Vec<(Entity, Option<Entity>)> = q
        .iter(world)
        .filter(|(_, c)| c.authenticated)
        .map(|(e, c)| (e, c.contained_id))
        .collect();
    if consoles.is_empty() {
        return;
    }
    let mut hosts: Vec<(Entity, u32, ImplantKind)> = Vec::new();
    for kind in [
        ImplantKind::Tracking,
        ImplantKind::Chem,
        ImplantKind::Beacon,
    ] {
        hosts.extend(
            tracked(world, kind)
                .into_iter()
                .map(|(mob, id)| (mob, id, kind)),
        );
    }
    for (console, card) in consoles {
        // `Exited`: a card that left the console (deleted, moved) is no longer contained.
        let card = card.filter(|&c| world.get::<StoredIn>(c).is_some_and(|s| s.0 == console));
        let inserted_id = card.and_then(|card| {
            let c = world.get::<PrisonerCard>(card)?;
            Some(PrisonerIdInfo {
                name: name_of(world, card),
                points: c.points,
                goal: c.goal,
            })
        });
        let implants: Vec<ImplantInfo> = hosts
            .iter()
            .filter(|&&(mob, _, kind)| shown(world, console, mob, kind))
            .filter_map(|&(mob, id, kind)| implant_info(world, mob, id, kind))
            .collect();
        if let Some(mut c) = world.get_mut::<PrisonerConsole>(console) {
            let view = PrisonerManagementData {
                authorized: false,
                inserted_id,
                implants,
            };
            if c.view != view {
                c.view = view;
            }
            if c.contained_id != card {
                c.contained_id = card;
            }
        }
    }
}

fn button(name: &str, color: &str, tooltip: &str, key: &str) -> ImplantButton {
    ImplantButton {
        name: name.to_owned(),
        color: color.to_owned(),
        tooltip: tooltip.to_owned(),
        action_key: key.to_owned(),
        action_params: Vec::new(),
    }
}

/// The console row of one implant.
fn implant_info(world: &World, mob: Entity, id: u32, kind: ImplantKind) -> Option<ImplantInfo> {
    let implant = world.get::<Implants>(mob)?.0.iter().find(|i| i.id == id)?;
    let mut info = vec![("ID".to_owned(), name_of(world, mob))];
    let mut buttons = vec![button(
        "Self Destruct",
        "bad",
        "Destoys the implant from within the user harmlessly.",
        SELF_DESTRUCT,
    )];
    match kind {
        ImplantKind::Tracking => {
            let area = world
                .get::<TilePos>(mob)
                .map(|&p| crate::bots::area_name(world, p))
                .filter(|a| !a.is_empty())
                .unwrap_or_else(|| "Unknown".to_owned());
            info.push(("Location".to_owned(), area));
            buttons.push(button(
                "Warn",
                "average",
                "Sends a message directly to the target's brain.",
                WARN,
            ));
        }
        // `beacon/get_management_console_data`.
        ImplantKind::Beacon => {
            let status = match crate::explosions::turf_of(world, mob) {
                Some(at)
                    if world.get::<crate::teleport::NoTeleport>(mob).is_none()
                        && crate::teleport::check_teleport_valid(world, at, at) =>
                {
                    if safe_air(world, at) {
                        "Implant carrier is in a safe environment."
                    } else {
                        "Implant carrier is in a hazardous environment!"
                    }
                }
                _ => "Implant carrier teleport signal cannot be reached!",
            };
            info.push(("Status".to_owned(), status.to_owned()));
        }
        ImplantKind::Chem => {
            let volume = implant.reagents.as_ref().map_or(0.0, |r| r.total_volume);
            info.push(("Volume".to_owned(), format!("{volume}u")));
            for size in IMPLANT_SIZES {
                buttons.push(ImplantButton {
                    action_params: vec![("amount".to_owned(), size.to_string())],
                    ..button(&format!("Inject {size}u"), "good", "", INJECT)
                });
            }
        }
        _ => return None,
    }
    Some(ImplantInfo {
        info,
        buttons,
        category: implant.name.clone(),
        implant_ref: id.to_string(),
    })
}

/// `is_safe_turf`'s goldilocks temperature and pressure of the air at `at`.
fn safe_air(world: &World, at: TilePos) -> bool {
    use crate::breath::{
        BODYTEMP_COLD_DAMAGE_LIMIT, BODYTEMP_HEAT_DAMAGE_LIMIT, HAZARD_HIGH_PRESSURE,
        HAZARD_LOW_PRESSURE,
    };
    let Some(atmos) = world.get_resource::<ss13_atmos::Atmos>() else {
        return false;
    };
    let (Some(t), Some(p)) = (atmos.temperature(at), atmos.pressure(at)) else {
        return false;
    };
    t > BODYTEMP_COLD_DAMAGE_LIMIT
        && t < BODYTEMP_HEAT_DAMAGE_LIMIT
        && p > HAZARD_LOW_PRESSURE
        && p < HAZARD_HIGH_PRESSURE
}

/// tg `item_interaction` of a prisoner console with a prisoner ID: `id_insert`. True when the
/// click was the console's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<PrisonerConsole>(target).is_none() || world.get::<PrisonerCard>(held).is_none() {
        return false;
    }
    id_insert(world, client, mob, target, held);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chem_rows_offer_tg_sizes() {
        assert_eq!(IMPLANT_SIZES, [1, 5, 10]);
        let b = button("Warn", "average", "", WARN);
        assert_eq!(b.action_key, "warn");
        assert!(b.action_params.is_empty());
    }
}
