//! AIs in MODsuits (server), after tg `code/modules/mod/mod_ai.dm`:
//!
//! - With the cover open, a loaded intelliCard on a control unit uploads its AI (conscious and
//!   played) after 5 s (`AI_TRANS_FROM_CARD` -> `ai_enter_mod`): wireless and radio on, power
//!   restored, the AI inside the unit ([`AiInside::Mod`]: its eye on the unit's turf, seeing
//!   around it). An empty card takes it back after 5 s (`ai_exit_mod`: wireless and radio off).
//! - The AI gets the unit's action buttons and window (tg `ai_action`s, `contained_state`).
//! - Its steps move the suit (`relaymove`): with a wearer only while the suit is active and the
//!   boots are sealed, alone only on a turf with gravity; each step costs `CHARGE_PER_STEP` and
//!   waits `movedelay × 2 (×√2 diagonally) (×5 without a wearer) + slowdown_deployed`.
//! - A unit despawned with its AI inside leaves an AI mini-card with the AI dead in it
//!   (tg `atom_destruction`: 150 burn and death); an empty intelliCard recovers it after 5 s.
// shortcut: no pAI assistants (`insert_pai`), no shell recall before the upload, no
// `TRAIT_FORCED_STANDING` for an unconscious wearer, no grab check, no relay into a vehicle the
// wearer is in; units have no integrity, so only a despawn makes the mini-card.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;
use ss13_shared::{Dir, TilePos};
use ss13_world::MoveRequest;

use super::ModJob;
use super::construction::{job_target, reaches};
use super::server::{ModSuit, check_charge, refresh_buttons, subtract_charge, theme_of};
use crate::construction::Job;
use crate::health::{DamageType, Health, Stat};
use crate::inventory::CarriedBy;
use crate::server::PlayerMob;
use crate::silicon::ai::{AiCore, AiEye, AiInside, EyeStep};
use crate::silicon::aicard::IntelliCard;
use crate::tools::{Actor, DoAfter, do_after, now, tell};

pub const MINICARD: &str = "/obj/item/mod/ai_minicard";
/// `do_after(user, 5 SECONDS)` of the transfers, deciseconds.
const TRANSFER_DS: f32 = 50.0;
/// `CHARGE_PER_STEP`: `DEFAULT_CHARGE_DRAIN * 2.5`, joules.
pub const CHARGE_PER_STEP: f64 = 0.005 * 10_000.0 * 2.5;
/// `MOVE_DELAY`, `WEARER_DELAY`, `LONE_DELAY`.
const MOVE_DELAY: f64 = 2.0;
const WEARER_DELAY: f64 = 1.0;
const LONE_DELAY: f64 = 5.0;
const MECHMOVE: &str = "sound/vehicles/mecha/mechmove01.ogg";
/// The mini-card's `apply_damage(150, BURN)`.
const MINICARD_BURN: f32 = 150.0;

/// On a control unit: its AI (tg `ai_assistant`) and the move cooldown (`cooldown_mod_move`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ModAssistant {
    pub ai: Entity,
    pub next_move: f64,
}

/// `/obj/item/mod/ai_minicard`: `stored_ai`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct MiniCard {
    pub ai: Option<Entity>,
}

pub struct ModAiPlugin;

impl Plugin for ModAiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            relay_steps
                .before(ss13_world::WorldSystems::Movement)
                .run_if(any_with_component::<ModAssistant>),
        )
        .add_observer(unit_despawned);
    }
}

fn path_of(world: &World, e: Entity) -> Option<String> {
    world.get::<DmItem>(e).map(|i| i.path.clone())
}

/// A control unit or an AI mini-card: what an intelliCard transfers with here.
pub fn is_target(world: &World, target: Entity) -> bool {
    world.get::<ModSuit>(target).is_some()
        || path_of(world, target).is_some_and(|p| is_under(&p, MINICARD))
}

/// The AI in the unit, if any.
pub fn assistant(world: &World, control: Entity) -> Option<Entity> {
    world.get::<ModAssistant>(control).map(|a| a.ai)
}

/// The unit whose AI `eye` is the eye of (tg `user == ai_assistant`).
pub fn suit_of_eye(world: &World, eye: Entity) -> Option<Entity> {
    let ai = world.get::<AiEye>(eye)?.ai;
    match world.get::<AiInside>(ai)? {
        &AiInside::Mod(control) => (assistant(world, control) == Some(ai)).then_some(control),
        _ => None,
    }
}

fn eye_of(world: &World, ai: Entity) -> Option<Entity> {
    world.get::<AiCore>(ai).map(|c| c.eye)
}

/// tg `transfer_ai` of a control unit or a mini-card with `card` (`card_ai`: its AI).
pub fn transfer(
    world: &mut World,
    client: Entity,
    user: Entity,
    card: Entity,
    card_ai: Option<Entity>,
    target: Entity,
) {
    let actor = Actor {
        client,
        mob: user,
        item: Some(card),
    };
    let on = job_target(world, user, target);
    if world.get::<ModSuit>(target).is_none() {
        // The mini-card: `AI_TRANS_TO_CARD` only.
        if card_ai.is_some() {
            return;
        }
        if world.get::<MiniCard>(target).and_then(|m| m.ai).is_none() {
            tell(world, client, Span::Warning, "нет ИИ!");
            return;
        }
        tell(world, client, Span::Notice, "перенос на карту...");
        let job = ModJob::MinicardToCard {
            minicard: target,
            card,
        };
        do_after(world, actor, on, TRANSFER_DS, 0, Job::ModSuit(job));
        return;
    }
    let open = world.get::<ModSuit>(target).is_some_and(|s| s.open);
    if !open {
        tell(world, client, Span::Warning, "панель закрыта!");
        return;
    }
    match card_ai {
        None => {
            if assistant(world, target).is_none() {
                tell(world, client, Span::Warning, "в блоке нет ИИ!");
                return;
            }
            tell(world, client, Span::Notice, "перенос на карту...");
            let job = ModJob::AiToCard {
                control: target,
                card,
            };
            do_after(world, actor, on, TRANSFER_DS, 0, Job::ModSuit(job));
        }
        Some(ai) => {
            if assistant(world, target).is_some() {
                tell(world, client, Span::Warning, "ИИ уже установлен!");
                return;
            }
            if !responsive(world, ai) {
                tell(world, client, Span::Warning, "ИИ не отвечает!");
                return;
            }
            tell(world, client, Span::Notice, "перенос в блок...");
            let job = ModJob::AiFromCard {
                control: target,
                card,
                ai,
            };
            do_after(world, actor, on, TRANSFER_DS, 0, Job::ModSuit(job));
        }
    }
}

/// `!IS_UNCONSCIOUS_OR_CRIT(intAI) && intAI.client`.
fn responsive(world: &World, ai: Entity) -> bool {
    world.get::<Stat>(ai) == Some(&Stat::Conscious)
        && eye_of(world, ai).is_some_and(|eye| world.get::<PlayerMob>(eye).is_some())
}

/// The end of a transfer `do_after`, rechecked: the card still in hand, the unit still open
/// and in reach, the AI still where it was.
pub(super) fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: ModJob) {
    let client = action.client;
    let held = |world: &World, card: Entity| {
        world.get::<CarriedBy>(card).is_some_and(|c| c.0 == mob)
            && world.get::<IntelliCard>(card).is_some()
    };
    match job {
        ModJob::AiToCard { control, card } => {
            if !held(world, card) || !reaches(world, mob, control) {
                return;
            }
            let Some(ai) = assistant(world, control) else {
                tell(world, client, Span::Warning, "в блоке нет ИИ!");
                return;
            };
            let empty = world
                .get::<IntelliCard>(card)
                .is_some_and(|c| c.ai.is_none());
            let open = world.get::<ModSuit>(control).is_some_and(|s| s.open);
            if !empty || !open {
                return;
            }
            ai_exit_mod(world, control, ai, card);
            tell(world, client, Span::Notice, "ИИ перенесён на карту");
        }
        ModJob::AiFromCard { control, card, ai } => {
            if !held(world, card) || !reaches(world, mob, control) {
                return;
            }
            let still = world.get::<IntelliCard>(card).and_then(|c| c.ai) == Some(ai)
                && world.get::<AiInside>(ai) == Some(&AiInside::Card(card));
            let open = world.get::<ModSuit>(control).is_some_and(|s| s.open);
            if !still || !open || assistant(world, control).is_some() || !responsive(world, ai) {
                return;
            }
            ai_enter_mod(world, control, ai, card);
            tell(world, client, Span::Notice, "ИИ перенесён в блок");
        }
        ModJob::MinicardToCard { minicard, card } => {
            if !held(world, card) || !reaches(world, mob, minicard) {
                return;
            }
            let Some(ai) = world.get::<MiniCard>(minicard).and_then(|m| m.ai) else {
                tell(world, client, Span::Warning, "прервано!");
                return;
            };
            if world
                .get::<IntelliCard>(card)
                .is_some_and(|c| c.ai.is_some())
                || world.get::<AiInside>(ai) != Some(&AiInside::Card(minicard))
            {
                return;
            }
            world.entity_mut(minicard).insert(MiniCard { ai: None });
            crate::obj_damage::set_icon_state(world, minicard, "minicard");
            crate::silicon::ai::house(world, ai, AiInside::Card(card));
            if let Some(mut c) = world.get_mut::<IntelliCard>(card) {
                c.ai = Some(ai);
            }
            crate::silicon::aicard::refresh_look(world, card);
            // `notify_revival`.
            crate::silicon::chassis::tell_ai(
                world,
                ai,
                Span::BoldNotice,
                "Вас извлекли из обломков!",
            );
            tell(world, client, Span::Notice, "ИИ перенесён на карту");
        }
        ModJob::RepairCore(_) | ModJob::WireFlower(_) => {}
    }
}

/// tg `ai_enter_mod` + `on_gained_assistant`.
pub fn ai_enter_mod(world: &mut World, control: Entity, ai: Entity, card: Entity) {
    crate::silicon::aicard::set_wireless(world, ai, true);
    crate::silicon::aicard::set_radio(world, ai, true);
    crate::silicon::ai_power::restore_power(world, ai);
    crate::silicon::ai::house(world, ai, AiInside::Mod(control));
    if let Some(mut c) = world.get_mut::<IntelliCard>(card) {
        c.ai = None;
    }
    crate::silicon::aicard::refresh_look(world, card);
    crate::silicon::aicard::refresh_eye(world, ai, true);
    world
        .entity_mut(control)
        .insert(ModAssistant { ai, next_move: 0.0 });
    crate::silicon::chassis::tell_ai(world, ai, Span::Notice, "перенесён в блок MOD");
    let eye = eye_of(world, ai);
    refresh_buttons(world, eye);
}

/// tg `ai_exit_mod` + `on_removed_assistant`.
pub fn ai_exit_mod(world: &mut World, control: Entity, ai: Entity, card: Entity) {
    crate::silicon::ai_power::restore_power(world, ai);
    crate::silicon::aicard::set_wireless(world, ai, false);
    crate::silicon::aicard::set_radio(world, ai, false);
    crate::silicon::ai::house(world, ai, AiInside::Card(card));
    if let Some(mut c) = world.get_mut::<IntelliCard>(card) {
        c.ai = Some(ai);
    }
    crate::silicon::aicard::refresh_look(world, card);
    crate::silicon::aicard::refresh_eye(world, ai, true);
    world.entity_mut(control).remove::<ModAssistant>();
    crate::silicon::chassis::tell_ai(world, ai, Span::Notice, "перенесён на карту");
    let eye = eye_of(world, ai);
    refresh_buttons(world, eye);
}

/// The AI's steps (`EyeStep`, which `move_eyes` ignores for a housed AI) move its unit.
fn relay_steps(world: &mut World, mut steps: Local<MessageCursor<EyeStep>>) {
    let steps: Vec<EyeStep> = steps
        .read(world.resource::<Messages<EyeStep>>())
        .copied()
        .collect();
    for step in steps {
        let Some(control) = suit_of_eye(world, step.eye) else {
            continue;
        };
        if world
            .get::<AiEye>(step.eye)
            .and_then(|e| world.get::<Stat>(e.ai))
            == Some(&Stat::Dead)
        {
            continue;
        }
        relaymove(world, control, step.dir);
    }
}

/// tg `/obj/item/mod/control/relaymove` by its AI. True when the suit stepped.
pub fn relaymove(world: &mut World, control: Entity, dir: Dir) -> bool {
    let (Some(suit), Some(assist)) = (
        world.get::<ModSuit>(control).cloned(),
        world.get::<ModAssistant>(control).copied(),
    ) else {
        return false;
    };
    let now = now(world);
    if (!suit.active && suit.wearer.is_some())
        || !check_charge(world, control, CHARGE_PER_STEP)
        || now < assist.next_move
    {
        return false;
    }
    if suit.wearer.is_some()
        && !suit
            .parts
            .iter()
            .any(|p| p.slot == InventorySlot::Shoes && p.sealed)
    {
        return false;
    }
    let diagonal = if Dir::CARDINALS.contains(&dir) {
        1.0
    } else {
        std::f64::consts::SQRT_2
    };
    let lone = if suit.wearer.is_some() {
        WEARER_DELAY
    } else {
        LONE_DELAY
    };
    let modifier = MOVE_DELAY * diagonal * lone;
    let mover = match suit.wearer {
        Some(wearer) => {
            if !crate::spacemove::process_spacemove(world, wearer, Some(dir), false) {
                return false;
            }
            wearer
        }
        None => {
            let Some(&pos) = world.get::<TilePos>(control) else {
                return false;
            };
            if !crate::multiz::has_gravity(world, pos) {
                return false;
            }
            control
        }
    };
    // `movedelay = CONFIG_GET(number/movedelay/run_delay)`, deciseconds.
    let movedelay = f64::from(crate::RUN_DELAY_SECS) * 10.0;
    let slowdown = f64::from(theme_of(world, &suit).slowdown_deployed);
    if let Some(mut a) = world.get_mut::<ModAssistant>(control) {
        a.next_move = now + (movedelay * modifier + slowdown) / 10.0;
    }
    subtract_charge(world, control, CHARGE_PER_STEP);
    super::server::sound_at(world, control, MECHMOVE, 25.0, 0.0);
    world.write_message(MoveRequest { entity: mover, dir });
    true
}

/// tg `atom_destruction` with an AI inside: a unit despawned (or its assistant otherwise
/// dropped) while its AI is still housed in it leaves a mini-card with the AI dead in it where
/// the unit was. `ai_exit_mod` rehouses the AI first, so it is not caught here.
fn unit_despawned(
    remove: On<Remove<ModAssistant>>,
    units: Query<&ModAssistant>,
    positions: Query<&TilePos>,
    carried: Query<&CarriedBy>,
    inside: Query<&AiInside>,
    mut commands: Commands,
) {
    let control = remove.entity;
    let Ok(&ModAssistant { ai, .. }) = units.get(control) else {
        return;
    };
    if inside.get(ai).ok() != Some(&AiInside::Mod(control)) {
        return;
    }
    let pos = positions.get(control).ok().copied().or_else(|| {
        carried
            .get(control)
            .ok()
            .and_then(|c| positions.get(c.0).ok().copied())
    });
    let Some(pos) = pos else {
        return;
    };
    commands.queue(move |world: &mut World| {
        if world.get::<AiInside>(ai) == Some(&AiInside::Mod(control)) {
            leave_minicard(world, ai, pos);
        }
    });
}

/// tg `new /obj/item/mod/ai_minicard(drop_location(), ai_assistant)`.
pub fn leave_minicard(world: &mut World, ai: Entity, pos: TilePos) {
    let Some(minicard) = crate::construction::spawn_obj(world, MINICARD, pos) else {
        return;
    };
    world.entity_mut(minicard).insert(MiniCard { ai: Some(ai) });
    crate::obj_damage::set_icon_state(world, minicard, "minicard-filled");
    crate::silicon::ai::house(world, ai, AiInside::Card(minicard));
    if let Ok(mut e) = world.get_entity_mut(ai)
        && let Ok((mut health, mut stat)) = e.get_components_mut::<(&mut Health, &mut Stat)>()
    {
        let mut new = *stat;
        crate::health::apply_damage(&mut health, &mut new, DamageType::Burn, MINICARD_BURN);
        // `INVOKE_ASYNC(ai, death)`.
        stat.set_if_neq(Stat::Dead);
    }
    crate::silicon::aicard::set_wireless(world, ai, false);
    crate::silicon::aicard::set_radio(world, ai, false);
}
