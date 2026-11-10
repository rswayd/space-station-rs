//! The intelliCard (server), after tg `/obj/item/aicard` (`code/game/objects/items/devices/
//! aicard.dm`) and the `transfer_ai` procs of the AI (`ai.dm`), the AI core (`_core.dm`) and the
//! APC (`apc_malf.dm`).
//!
//! - An empty card on an AI with a mind cards it (tg `AI_TRANS_TO_CARD`): an empty finished core
//!   stays on its tile (`super::ai_core`), the AI's power is restored, its wireless (remote
//!   control) and radio go off, and it is inside the card: its eye sits on the card's turf and it
//!   sees around it ([`super::ai`]).
//! - A loaded card on an empty finished core uploads the AI there (`AI_TRANS_FROM_CARD`): the
//!   core is replaced by the AI, wireless and radio are back.
//! - An empty card on an APC with a shunted AI that has no linked core asks the AI; if it agrees
//!   and the user stays 5 s, the AI moves into the card.
//! - Used in hand with an AI inside: the card's menu (tg `Intellicard` window) — wipe the AI
//!   (5 oxygen damage every 0.5 s until it dies, after a "Yes"; again stops it), switch its
//!   wireless and its radio. The card shows the AI's face and an on/off light.
// shortcut: the menu is a list prompt instead of the `Intellicard` window (laws are not shown);
// no intelliTater or intelliLantern looks, no light; a destroyed card
// kills its AI and ghosts its player (tg deletes it); no transfers into mechs (MODsuits:
// `crate::modsuit::ai`; the AI fixer: `super::ai_fixer`);
// no `can_be_carded` (doomsday); messages are chat lines (tg balloon alerts).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_chat::server::MaskedSenses;
use ss13_mapload::{DmItem, DmName, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems};
use ss13_tgui::server::UiSystems;

use super::ai::{AiCore, AiEye, AiInside, LinkedCore};
use super::cameras::CAMERANET_MASK;
use super::chassis::{name_of, tell_ai};
use crate::construction::Job;
use crate::health::{DamageType, Health, Stat};
use crate::inventory::CarriedBy;
use crate::medical::brainmob;
use crate::obj_damage::is_under;
use crate::server::PlayerMob;
use crate::tools::{Actor, DoAfter, tell};

pub const AICARD: &str = "/obj/item/aicard";
const AICARDS_DMI: &str = "icons/obj/aicards.dmi";
/// `wipe_ai`: `adjust_oxy_loss(5)` every `sleep(0.5 SECONDS)`.
pub const WIPE_STEP_SECS: f64 = 0.5;
pub const WIPE_OXY: f32 = 5.0;
/// tg `do_after(user, 5 SECONDS, target = src)` of the APC transfer, deciseconds.
const APC_TRANSFER_DS: f32 = 50.0;
const CONSENT: [&str; 2] = ["Да - Переместить", "Нет - Остаться"];

/// tg `AI`, `flush` of the card; made on first use.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct IntelliCard {
    pub ai: Option<Entity>,
    pub flush: bool,
    next_wipe: f64,
}

/// The APC transfer's 5 s.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardJob {
    FromApc { ai: Entity },
}

/// The card's prompts, on their prompt entity.
#[derive(Component, Clone, Copy, Debug)]
enum CardPrompt {
    Menu {
        card: Entity,
        user: Entity,
    },
    ConfirmWipe {
        card: Entity,
        user: Entity,
    },
    /// Asked to the shunted AI.
    Consent {
        card: Entity,
        user: Entity,
        user_client: Entity,
        apc: Entity,
        ai: Entity,
    },
}

pub struct AiCardPlugin;

impl Plugin for AiCardPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // The card's menu.
        app.add_click_handler(ClickPhase::AttackSelf, 160, |w, c| {
            attack_self(w, c.client, c.mob, c.target)
        });
        // Carding an AI, uploading into a core, taking a shunted AI out of an APC.
        app.add_click_handler(ClickPhase::HeldOnTarget, 1420, |w, c| {
            c.held
                .is_some_and(|h| interact_with_atom(w, c.client, c.mob, h, c.target))
        });
        app.add_systems(
            Update,
            (
                answered
                    .in_set(UiSystems::Handle)
                    .after(PromptSystems::Answer),
                wipe.run_if(any_with_component::<IntelliCard>),
            ),
        )
        .add_observer(card_removed);
    }
}

fn is_card(world: &World, e: Entity) -> bool {
    world
        .get::<DmItem>(e)
        .is_some_and(|i| is_under(&i.path, AICARD))
}

/// The card's clicks on mobs are its own: `crate::combat` leaves them (tg `NOBLUDGEON`).
pub fn interacts_with_mobs(path: &str) -> bool {
    is_under(path, AICARD)
}

fn card(world: &mut World, e: Entity) -> Option<IntelliCard> {
    if !is_card(world, e) {
        return None;
    }
    if world.get::<IntelliCard>(e).is_none() {
        world.entity_mut(e).insert(IntelliCard::default());
    }
    world.get::<IntelliCard>(e).copied()
}

fn holds(world: &World, user: Entity, item: Entity) -> bool {
    world.get::<CarriedBy>(item).is_some_and(|c| c.0 == user)
}

fn eye_of(world: &World, ai: Entity) -> Option<Entity> {
    world.get::<AiCore>(ai).map(|c| c.eye)
}

fn ai_client(world: &World, ai: Entity) -> Option<Entity> {
    world.get::<PlayerMob>(eye_of(world, ai)?).map(|p| p.client)
}

fn exe_number(world: &mut World) -> u32 {
    world
        .get_resource_mut::<crate::combat::CombatRng>()
        .map_or(1000, |mut r| r.range(1000, 9999))
}

/// tg `set_control_disabled` and `radio_enabled`.
pub(crate) fn set_wireless(world: &mut World, ai: Entity, on: bool) {
    if let Some(mut c) = world.get_mut::<AiCore>(ai)
        && c.control_disabled == on
    {
        c.control_disabled = !on;
    }
}

pub(super) fn radio_on(world: &World, ai: Entity) -> bool {
    eye_of(world, ai)
        .and_then(|eye| world.get::<crate::radio::SiliconRadio>(eye))
        .and_then(|r| world.get::<crate::radio::Radio>(r.0))
        .is_some_and(|r| !r.tx_cut)
}

pub(crate) fn set_radio(world: &mut World, ai: Entity, on: bool) {
    let Some(radio) = eye_of(world, ai)
        .and_then(|eye| world.get::<crate::radio::SiliconRadio>(eye))
        .map(|r| r.0)
    else {
        return;
    };
    if let Some(mut r) = world.get_mut::<crate::radio::Radio>(radio) {
        r.tx_cut = !on;
    }
}

/// The eye's senses and view follow its new housing; tracking stops (tg `cancel_camera`).
pub(crate) fn refresh_eye(world: &mut World, ai: Entity, carded: bool) {
    let Some(eye) = eye_of(world, ai) else {
        return;
    };
    let mut e = world.entity_mut(eye);
    e.remove::<super::ai_track::AiTracking>();
    if carded {
        e.remove::<MaskedSenses>();
    } else {
        e.insert(MaskedSenses(CAMERANET_MASK));
    }
    if let Some(mut eye) = world.get_mut::<AiEye>(eye) {
        eye.set_changed();
    }
}

/// tg `interact_with_atom`: capture with an empty card, upload with a loaded one. False when the
/// held item is no card or the target nothing it transfers with.
pub fn interact_with_atom(
    world: &mut World,
    client: Entity,
    user: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(state) = card(world, held) else {
        return false;
    };
    // `/obj/item/mod/control/transfer_ai`, `/obj/item/mod/ai_minicard/transfer_ai`.
    if crate::modsuit::ai::is_target(world, target) {
        if state.flush {
            tell(
                world,
                client,
                Span::Warning,
                "ERROR: AI flush is in progress, cannot execute transfer protocol.",
            );
        } else {
            crate::modsuit::ai::transfer(world, client, user, held, state.ai, target);
        }
        return true;
    }
    let ai_target = world.get::<AiCore>(target).is_some();
    let core_target = super::ai_core::is_core(world, target);
    let apc_target = world.get::<crate::power::Apc>(target).is_some();
    let fixer_target = super::ai_fixer::is_fixer(world, target);
    if !ai_target && !core_target && !apc_target && !fixer_target {
        return false;
    }
    // tg `/atom/proc/transfer_ai`: no transfer while flushing.
    if state.flush {
        tell(
            world,
            client,
            Span::Warning,
            "ERROR: AI flush is in progress, cannot execute transfer protocol.",
        );
        return true;
    }
    if fixer_target {
        super::ai_fixer::transfer(world, client, held, state.ai, target);
        return true;
    }
    match state.ai {
        None if ai_target => capture(world, client, held, target),
        None if core_target => tell(
            world,
            client,
            Span::Warning,
            "There is no AI loaded on this terminal.",
        ),
        None => from_apc(world, client, user, held, target),
        Some(ai) if core_target => upload(world, client, held, ai, target),
        Some(_) if apc_target => {
            let name = name_of(world, held);
            tell(world, client, Span::Warning, format!("{name} уже занята!"));
        }
        // A loaded card on an AI: nothing (tg `NOBLUDGEON`, no attack either).
        Some(_) => {}
    }
    true
}

/// tg `/mob/living/silicon/ai/transfer_ai(AI_TRANS_TO_CARD)`.
fn capture(world: &mut World, client: Entity, card: Entity, ai: Entity) {
    if world.get::<AiInside>(ai).is_some() {
        return;
    }
    let Some(eye) = eye_of(world, ai) else {
        return;
    };
    if brainmob::mind_of(world, eye).is_none() {
        tell(world, client, Span::Warning, "no intelligence detected!");
        return;
    }
    let Some(&pos) = world.get::<TilePos>(ai) else {
        return;
    };
    super::ai_power::restore_power(world, ai);
    set_wireless(world, ai, false);
    set_radio(world, ai, false);
    super::ai::house(world, ai, AiInside::Card(card));
    super::ai_core::spawn_empty_core(world, pos);
    crate::construction::tile_changed(world, pos);
    if let Some(mut c) = world.get_mut::<IntelliCard>(card) {
        c.ai = Some(ai);
    }
    refresh_eye(world, ai, true);
    refresh_look(world, card);
    tell_ai(
        world,
        ai,
        Span::Plain,
        "You have been downloaded to a mobile storage device. Remote device connection severed.",
    );
    let name = name_of(world, ai);
    let n = exe_number(world);
    tell(
        world,
        client,
        Span::BoldNotice,
        format!(
            "Transfer successful: {name} ({n}.exe) removed from host terminal and stored within \
             local memory."
        ),
    );
    info!("{client} carded the AI {ai} into {card}");
}

/// tg `/obj/structure/ai_core/transfer_ai(AI_TRANS_FROM_CARD)`.
fn upload(world: &mut World, client: Entity, card: Entity, ai: Entity, core: Entity) {
    if world.get::<AiInside>(ai) != Some(&AiInside::Card(card)) {
        return;
    }
    match super::ai_core::upload_refusal(world, core) {
        None => {}
        Some("") => return,
        Some(why) => {
            let name = name_of(world, core);
            tell(world, client, Span::Warning, format!("{name} {why}"));
            return;
        }
    }
    let Some(&pos) = world.get::<TilePos>(core) else {
        return;
    };
    super::ai_core::despawn_core(world, core);
    super::ai::unhouse(world, ai, pos);
    set_wireless(world, ai, true);
    set_radio(world, ai, true);
    if let Some(mut c) = world.get_mut::<IntelliCard>(card) {
        c.ai = None;
    }
    if let Some(eye) = eye_of(world, ai) {
        world.entity_mut(eye).insert(pos);
    }
    refresh_eye(world, ai, false);
    refresh_look(world, card);
    tell_ai(
        world,
        ai,
        Span::Notice,
        "You have been uploaded to a stationary terminal. Remote device connection restored.",
    );
    let name = name_of(world, ai);
    let n = exe_number(world);
    tell(
        world,
        client,
        Span::BoldNotice,
        format!(
            "Transfer successful: {name} ({n}.exe) installed and executed successfully. Local \
             copy has been removed."
        ),
    );
    info!("{client} uploaded the AI {ai} from {card} at {pos:?}");
}

/// The shunted AI of `apc` that may go into an empty card, else why not (empty: silently).
fn apc_refusal(world: &mut World, apc: Entity) -> Result<Entity, String> {
    let apc_name = name_of(world, apc);
    let Some(ai) = world.get::<super::malf::ShuntOccupier>(apc).map(|o| o.0) else {
        return Err(format!("В {apc_name} ничего нет для передачи!"));
    };
    let ai_name = name_of(world, ai);
    if ai_client(world, ai).is_none() {
        return Err(format!("{ai_name} неактивен или уничтожен!"));
    }
    if world.get::<LinkedCore>(ai).is_some() {
        return Err(format!("{ai_name} блокирует все попытки передачи!"));
    }
    if world.get::<Stat>(ai) != Some(&Stat::Conscious) {
        return Err(String::new());
    }
    Ok(ai)
}

/// tg `/obj/machinery/power/apc/transfer_ai`: the AI is asked first.
fn from_apc(world: &mut World, client: Entity, user: Entity, card: Entity, apc: Entity) {
    let ai = match apc_refusal(world, apc) {
        Ok(ai) => ai,
        Err(why) => {
            if !why.is_empty() {
                tell(world, client, Span::Warning, why);
            }
            return;
        }
    };
    let (Some(ai_client), Some(eye)) = (ai_client(world, ai), eye_of(world, ai)) else {
        return;
    };
    tell(
        world,
        client,
        Span::Notice,
        "Процесс передачи запущен. Отправлен запрос на подтверждение ИИ...",
    );
    if let Some(&pos) = world.get::<TilePos>(apc) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/machines/click.ogg", 50.0, true),
        );
    }
    let user_name = name_of(world, user);
    let card_name = name_of(world, card);
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        Ask::new(
            ai_client,
            eye,
            "Перемещение ИИ",
            format!("{user_name} пытается переместить вас в {card_name}. Вы согласны с этим?"),
            PromptKind::Alert {
                buttons: CONSENT.map(str::to_owned).to_vec(),
                autofocus: true,
            },
        ),
    );
    world.entity_mut(prompt).insert(CardPrompt::Consent {
        card,
        user,
        user_client: client,
        apc,
        ai,
    });
}

/// The APC transfer's end: the same AI still in the APC, the card still empty in the hand.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: CardJob) {
    let CardJob::FromApc { ai } = job;
    let apc = action.target;
    let Some(card) = action.item.filter(|&c| holds(world, mob, c)) else {
        return;
    };
    let empty = world
        .get::<IntelliCard>(card)
        .is_some_and(|c| c.ai.is_none());
    if !empty
        || world.get::<AiInside>(ai) != Some(&AiInside::Apc(apc))
        || apc_refusal(world, apc) != Ok(ai)
    {
        return;
    }
    super::ai::house(world, ai, AiInside::Card(card));
    super::malf::left_apc(world, ai, apc);
    if let Some(mut c) = world.get_mut::<IntelliCard>(card) {
        c.ai = Some(ai);
    }
    refresh_eye(world, ai, true);
    refresh_look(world, card);
    let ai_name = name_of(world, ai);
    let card_name = name_of(world, card);
    tell(
        world,
        action.client,
        Span::Notice,
        format!("Передача завершена! {ai_name} теперь сохранён в {card_name}."),
    );
    let user_name = name_of(world, mob);
    tell_ai(
        world,
        ai,
        Span::Notice,
        format!("Передача завершена! Вы были сохранены в {card_name} у {user_name}."),
    );
}

/// tg `ui_interact`: the menu, with an AI inside.
pub fn attack_self(world: &mut World, client: Entity, user: Entity, item: Entity) -> bool {
    let Some(state) = card(world, item) else {
        return false;
    };
    let Some(ai) = state.ai else {
        tell(world, client, Span::Notice, "The card is empty.");
        return true;
    };
    let wireless = world.get::<AiCore>(ai).is_some_and(|c| !c.control_disabled);
    let on_off = |on: bool| if on { "Enabled" } else { "Disabled" };
    let items = vec![
        if state.flush { "Stop Wiping" } else { "Wipe" }.to_owned(),
        format!("Wireless Activity: {}", on_off(wireless)),
        format!("Subspace Radio: {}", on_off(radio_on(world, ai))),
    ];
    let name = name_of(world, ai);
    let dead = world.get::<Stat>(ai) == Some(&Stat::Dead);
    let health = world
        .get::<Health>(ai)
        .map_or(0.0, |h| (h.health() + 100.0) / 2.0);
    let title = name_of(world, item);
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        Ask::new(
            client,
            user,
            title,
            format!(
                "{name}: {}",
                if dead {
                    "offline".to_owned()
                } else {
                    format!("system integrity {health:.0}%")
                }
            ),
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(CardPrompt::Menu { card: item, user });
    true
}

fn answered(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.answer.clone()))
        .collect();
    for (prompt, client, answer) in pending {
        let Some(asked) = world.get::<CardPrompt>(prompt).copied() else {
            continue;
        };
        match asked {
            CardPrompt::Menu { card, user } => {
                if let Some(PromptAnswer::Item(i)) = answer {
                    menu_choice(world, client, user, card, i);
                }
            }
            CardPrompt::ConfirmWipe { card, user } => {
                // tg `if(confirm == "Yes" && !..())`: the card still in the hand.
                if answer == Some(PromptAnswer::Button(0)) && holds(world, user, card) {
                    start_wipe(world, card);
                }
            }
            CardPrompt::Consent {
                card,
                user,
                user_client,
                apc,
                ai,
            } => consent(world, user_client, user, card, apc, ai, answer),
        }
    }
}

/// The menu's acts (tg `ui_act` `wipe`, `wireless`, `radio`), the card in the hand.
fn menu_choice(world: &mut World, client: Entity, user: Entity, card: Entity, choice: usize) {
    if !holds(world, user, card) {
        return;
    }
    let Some(state) = world.get::<IntelliCard>(card).copied() else {
        return;
    };
    let Some(ai) = state.ai else {
        return;
    };
    match choice {
        0 if state.flush => {
            if let Some(mut c) = world.get_mut::<IntelliCard>(card) {
                c.flush = false;
            }
        }
        0 => {
            let name = name_of(world, card);
            let prompt = ss13_tgui::prompt::ask_world(
                world,
                Ask::new(
                    client,
                    user,
                    name,
                    "Are you sure you want to wipe this card's memory?",
                    PromptKind::Alert {
                        buttons: vec!["Yes".to_owned(), "No".to_owned()],
                        autofocus: true,
                    },
                ),
            );
            world
                .entity_mut(prompt)
                .insert(CardPrompt::ConfirmWipe { card, user });
        }
        1 => {
            let on = world.get::<AiCore>(ai).is_some_and(|c| c.control_disabled);
            set_wireless(world, ai, on);
            let name = name_of(world, card);
            let state = if on { "enabled" } else { "disabled" };
            tell_ai(
                world,
                ai,
                Span::Warning,
                format!("{name}'s wireless port has been {state}!"),
            );
        }
        2 => {
            let on = !radio_on(world, ai);
            set_radio(world, ai, on);
            let state = if on { "enabled" } else { "disabled" };
            tell_ai(
                world,
                ai,
                Span::Warning,
                format!("Your Subspace Transceiver has been {state}!"),
            );
        }
        _ => {}
    }
    refresh_look(world, card);
}

/// tg `wipe_ai`'s start.
fn start_wipe(world: &mut World, card: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(ai) = world.get::<IntelliCard>(card).and_then(|c| c.ai) else {
        return;
    };
    if world.get::<AiInside>(ai) != Some(&AiInside::Card(card)) {
        return;
    }
    if let Some(mut c) = world.get_mut::<IntelliCard>(card) {
        c.flush = true;
        c.next_wipe = now;
    }
    tell_ai(
        world,
        ai,
        Span::Danger,
        "YOUR SYSTEM FILES ARE BEING WIPED!",
    );
}

/// tg `wipe_ai`'s loop: 5 oxygen damage every 0.5 s until the AI is dead or the wipe stopped.
// Cost: the few cards of a round, a component read each per tick.
fn wipe(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &IntelliCard)>();
    let due: Vec<(Entity, Option<Entity>)> = q
        .iter(world)
        .filter(|(_, c)| c.flush && now >= c.next_wipe)
        .map(|(e, c)| (e, c.ai))
        .collect();
    for (card, ai) in due {
        let alive = ai.filter(|&ai| {
            world.get::<AiInside>(ai) == Some(&AiInside::Card(card))
                && world.get::<Stat>(ai).is_some_and(|s| *s != Stat::Dead)
        });
        let Some(ai) = alive else {
            if let Some(mut c) = world.get_mut::<IntelliCard>(card) {
                c.flush = false;
            }
            refresh_look(world, card);
            continue;
        };
        if let Some(mut h) = world.get_mut::<Health>(ai) {
            h.adjust(DamageType::Oxy, WIPE_OXY);
        }
        if let Some(mut c) = world.get_mut::<IntelliCard>(card) {
            c.next_wipe = now + WIPE_STEP_SECS;
        }
    }
}

/// The AI's consent to the APC transfer, then the user's 5 s.
fn consent(
    world: &mut World,
    user_client: Entity,
    user: Entity,
    card: Entity,
    apc: Entity,
    ai: Entity,
    answer: Option<PromptAnswer>,
) {
    if answer != Some(PromptAnswer::Button(0)) {
        tell(
            world,
            user_client,
            Span::Danger,
            "ИИ отклонил запрос на передачу. Процесс прерван.",
        );
        if let Some(&pos) = world.get::<TilePos>(apc) {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(pos, "sound/machines/buzz/buzz-sigh.ogg", 50.0, true),
            );
        }
        return;
    }
    let empty = world
        .get::<IntelliCard>(card)
        .is_some_and(|c| c.ai.is_none());
    if !holds(world, user, card) || !empty || apc_refusal(world, apc) != Ok(ai) {
        return;
    }
    let card_name = name_of(world, card);
    tell(
        world,
        user_client,
        Span::Notice,
        format!("ИИ принял запрос. Передача сохранённого сознания в {card_name}..."),
    );
    tell_ai(
        world,
        ai,
        Span::Notice,
        format!("Начало передачи. Вы будете перемещены в {card_name} в ближайшее время."),
    );
    crate::tools::do_after(
        world,
        Actor {
            client: user_client,
            mob: user,
            item: Some(card),
        },
        apc,
        APC_TRANSFER_DS,
        0,
        Job::AiCard(CardJob::FromApc { ai }),
    );
}

/// tg `/obj/item/aicard/Destroy`: the AI inside dies and its player ghosts.
fn card_removed(
    remove: On<Remove<IntelliCard>>,
    cards: Query<&IntelliCard>,
    mut commands: Commands,
) {
    let Some(ai) = cards.get(remove.entity).ok().and_then(|c| c.ai) else {
        return;
    };
    let card = remove.entity;
    commands.queue(move |world: &mut World| {
        if world.get::<AiInside>(ai) != Some(&AiInside::Card(card)) {
            return;
        }
        if let Some(mut stat) = world.get_mut::<Stat>(ai) {
            *stat = Stat::Dead;
        }
        if let Some(client) = ai_client(world, ai) {
            crate::ghost::ghostize_client(world, client);
        }
    });
}

/// tg `update_name` ("<initial name> - <AI>") and `update_overlays`: the AI's face (`ai`, dead
/// `ai_dead`) and the wireless light.
pub(crate) fn refresh_look(world: &mut World, card: Entity) {
    let ai = world.get::<IntelliCard>(card).and_then(|c| c.ai);
    let initial = world.get::<DmItem>(card).and_then(|i| {
        world
            .get_resource::<ss13_proto::Prototypes>()?
            .get(&i.path)?
            .get::<DmName>()
            .map(|n| n.name.clone())
    });
    if let Some(initial) = initial {
        let name = match ai.and_then(|a| world.get::<Name>(a)) {
            Some(ai_name) => format!("{initial} - {}", ai_name.as_str()),
            None => initial,
        };
        if let Some(mut n) = world.get_mut::<DmName>(card)
            && n.name != name
        {
            n.name = name;
        }
    }
    let states: Vec<&str> = match ai {
        None => Vec::new(),
        Some(ai) => {
            let dead = world.get::<Stat>(ai) == Some(&Stat::Dead);
            let wireless = world.get::<AiCore>(ai).is_some_and(|c| !c.control_disabled);
            vec![
                if dead { "ai_dead" } else { "ai" },
                if wireless { "aicard-on" } else { "aicard-off" },
            ]
        }
    };
    let Some(icons) = world.get_resource::<IconIndex>() else {
        return;
    };
    let ours: Vec<_> = ["ai", "ai_dead", "aicard-on", "aicard-off"]
        .iter()
        .filter_map(|s| icons.get(AICARDS_DMI, s))
        .collect();
    let wanted: Vec<_> = states
        .iter()
        .filter_map(|s| icons.get(AICARDS_DMI, s))
        .collect();
    let Some(mut look) = world.get_mut::<Appearance>(card) else {
        return;
    };
    let mut overlays = look.overlays.clone();
    overlays.retain(|o| !ours.contains(&o.icon_state));
    overlays.extend(wanted.into_iter().map(Overlay::new));
    if look.overlays != overlays {
        look.overlays = overlays;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<Time>();
        world
    }

    #[test]
    fn a_wipe_drains_the_carded_ai_every_half_second() {
        let mut world = world();
        let card = world
            .spawn(DmItem {
                path: AICARD.into(),
                ..default()
            })
            .id();
        let ai = world
            .spawn((AiInside::Card(card), crate::health::living_bundle()))
            .id();
        world.entity_mut(card).insert(IntelliCard {
            ai: Some(ai),
            ..default()
        });
        start_wipe(&mut world, card);
        assert!(world.get::<IntelliCard>(card).unwrap().flush);
        wipe(&mut world);
        assert_eq!(world.get::<Health>(ai).unwrap().oxy, WIPE_OXY);
        // Not yet half a second later.
        wipe(&mut world);
        assert_eq!(world.get::<Health>(ai).unwrap().oxy, WIPE_OXY);
        world.entity_mut(ai).insert(Stat::Dead);
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs(1));
        wipe(&mut world);
        assert!(!world.get::<IntelliCard>(card).unwrap().flush);
    }

    #[test]
    fn only_cards_interact_with_mobs() {
        assert!(interacts_with_mobs("/obj/item/aicard/aitater"));
        assert!(!interacts_with_mobs("/obj/item/aicardigan"));
    }
}
