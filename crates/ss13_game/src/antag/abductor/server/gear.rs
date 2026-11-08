//! Abductor gear (server) after tg `equipment/gear/abductor_items.dm`, `abductor_clothing.dm`
//! and `equipment/abduction_outfits.dm`.
//!
//! - **Outfits** ([`equip`]): the alien jumpsuit, combat boots, alien headset and a backpack;
//!   the agent (and the solo scientist) adds the helmet, the agent vest, the advanced baton in
//!   the suit storage, the abductor belt, the alien pistol and the radio silencer; the scientist
//!   a science tool. [`link_to_console`]: the vest (locked on) and the science tool are linked to
//!   the team's console.
//! - **Science tool** ([`ScienceTool`]): only a scientist can use it, and only when linked; in hand
//!   it switches SCAN/MARK. Scan (any range): a human goes into the console's disguises. Mark: an
//!   abductor or a cow at once; anyone else must be next to the user for a 10 s preparation.
//! - **Advanced baton** ([`AbductorBaton`]): in hand an abductor cycles stun, sleep, restrain,
//!   probe. A hit by an abductor: stun (jitter 40 s, confusion 10 s, stutter 16 s, paralysis
//!   14 s), sleep (2 minutes on an incapacitated target, else drowsiness), restrain (3 s, then
//!   hard-light energy cuffs) or probe (the species and whether it has a gland or a heart).
//!   Others just hit with it.
//! - **Radio silencer** ([`Silencer`]): an abductor shuts the radios of every human within 2
//!   tiles of the target off (headsets keep transmitting).
//! - **Mental interface device** ([`MindDevice`]): a scientist switches TRANSMISSION/COMMAND;
//!   a message goes into a living mind, a command to the gland of the target
//!   ([`super::glands::mind_control`]).
//! - **Agent vest** ([`AgentVest`]): the console flips stealth/combat and locks it on.
// shortcut: the vest's stealth disguise does not change the wearer's look or name, its armor
// does not change with the mode and it has no combat stimulant action; the energy cuffs do not
// break on removal; the alien pistol, shrink ray, omnitool, alien tools and the reagent
// synthesizer are their generic types (guns, tools) without abductor-only behaviour.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::UiSystems;

use super::*;
use crate::antag::abductor::{VEST_COMBAT, VEST_STEALTH};
use crate::combat::NextMove;
use crate::health::Stat;
use crate::inventory::{Inventory, NoDrop};
use crate::status_effects::{Incap, Timed};
use crate::tools::{Actor, now, tell};

pub const UNIFORM: &str = "/obj/item/clothing/under/abductor";
pub const SHOES: &str = "/obj/item/clothing/shoes/combat";
pub const HEADSET: &str = "/obj/item/radio/headset/abductor";
pub const HELMET: &str = "/obj/item/clothing/head/helmet/abductor";
pub const VEST: &str = "/obj/item/clothing/suit/armor/abductor/vest";
pub const BATON: &str = "/obj/item/melee/baton/abductor";
pub const BELT: &str = "/obj/item/storage/belt/military/abductor/full";
pub const ALIEN_PISTOL: &str = "/obj/item/gun/energy/alien";
pub const SILENCER: &str = "/obj/item/abductor/silencer";
pub const GIZMO: &str = "/obj/item/abductor/gizmo";
pub const MIND_DEVICE: &str = "/obj/item/abductor/mind_device";
pub const ENERGY_CUFFS: &str = "/obj/item/restraints/handcuffs/energy";
const COW_ROOT: &str = "/mob/living/basic/cow";
/// `prepare`: `do_after(user, 10 SECONDS)`.
const PREPARE_DS: f32 = 100.0;
/// The baton's `time_to_cuff`, `sleep_time`, `knockdown_time`.
const CUFF_DS: f32 = 30.0;
const SLEEP_SECS: f32 = 120.0;
const KNOCKDOWN_SECS: f32 = 14.0;
/// `MAX_MESSAGE_LEN`.
const MAX_MESSAGE_LEN: u32 = 1024;

/// `GIZMO_SCAN`, `GIZMO_MARK`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolMode {
    #[default]
    Scan,
    Mark,
}

/// `/obj/item/abductor/gizmo`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScienceTool {
    pub mode: ToolMode,
    /// `marked_target_weakref`.
    pub marked: Option<Entity>,
    pub console: Option<Entity>,
}

/// `BATON_STUN`, `BATON_SLEEP`, `BATON_CUFF`, `BATON_PROBE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BatonMode {
    #[default]
    Stun,
    Sleep,
    Cuff,
    Probe,
}

impl BatonMode {
    /// `mode = (mode + 1) % BATON_MODES`.
    pub fn next(self) -> Self {
        match self {
            Self::Stun => Self::Sleep,
            Self::Sleep => Self::Cuff,
            Self::Cuff => Self::Probe,
            Self::Probe => Self::Stun,
        }
    }

    fn icon_state(self) -> &'static str {
        match self {
            Self::Stun => "wonderprodStun",
            Self::Sleep => "wonderprodSleep",
            Self::Cuff => "wonderprodCuff",
            Self::Probe => "wonderprodProbe",
        }
    }

    fn text(self) -> &'static str {
        match self {
            Self::Stun => "оглушения",
            Self::Sleep => "усыпления",
            Self::Cuff => "сковывания",
            Self::Probe => "зондирования",
        }
    }
}

/// `/obj/item/melee/baton/abductor`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AbductorBaton {
    pub mode: BatonMode,
}

/// `/obj/item/abductor/silencer`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Silencer;

/// `/obj/item/abductor/mind_device`: `MIND_DEVICE_CONTROL` when `control`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MindDevice {
    pub control: bool,
}

/// `/obj/item/clothing/suit/armor/abductor/vest`.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct AgentVest {
    /// [`VEST_STEALTH`] or [`VEST_COMBAT`].
    pub mode: u8,
    /// The scanned name it disguises as.
    pub disguise: Option<String>,
}

impl Default for AgentVest {
    fn default() -> Self {
        Self {
            mode: VEST_STEALTH,
            disguise: None,
        }
    }
}

/// A mental interface device's question waiting for its answer.
#[derive(Component, Clone, Copy, Debug)]
struct MindPrompt {
    device: Entity,
    target: Entity,
    control: bool,
}

/// The gear component of an item type, inserted when missing.
fn ensure_gear(world: &mut World, item: Entity) {
    let Some(path) = world.get::<DmItem>(item).map(|i| i.path.clone()) else {
        return;
    };
    let is = |root: &str| crate::obj_damage::is_under(&path, root);
    let mut e = world.entity_mut(item);
    if is(GIZMO) && !e.contains::<ScienceTool>() {
        e.insert(ScienceTool::default());
    } else if is(BATON) && !e.contains::<AbductorBaton>() {
        e.insert(AbductorBaton::default());
    } else if is(SILENCER) && !e.contains::<Silencer>() {
        e.insert(Silencer);
    } else if is(MIND_DEVICE) && !e.contains::<MindDevice>() {
        e.insert(MindDevice::default());
    } else if is(VEST) && !e.contains::<AgentVest>() {
        e.insert(AgentVest::default());
    }
}

fn adopt(add: On<Add<DmItem>>, mut commands: Commands) {
    let item = add.entity;
    commands.queue(move |world: &mut World| {
        if world.get_entity(item).is_ok() {
            ensure_gear(world, item);
        }
    });
}

/// The items of the abductor gear tools that act on mobs instead of hitting them.
pub fn interacts_with_mobs(path: &str) -> bool {
    [GIZMO, SILENCER, MIND_DEVICE]
        .iter()
        .any(|root| crate::obj_damage::is_under(path, root))
}

// ---------------------------------------------------------------------------------------------
// Outfit

fn carried(world: &mut World, mob: Entity, path: &str) -> Option<Entity> {
    let item = crate::antag::wizard::server::carried(world, mob, path)?;
    ensure_gear(world, item);
    Some(item)
}

fn into_pack(world: &mut World, pack: Entity, path: &str) {
    if let Some(item) = crate::antag::wizard::server::into_pack(world, pack, path) {
        ensure_gear(world, item);
    }
}

/// `/datum/outfit/abductor/agent`, `/scientist`, `/scientist/onemanteam`.
pub fn equip(world: &mut World, mob: Entity, role: AbductorRole) {
    let mut inv = world.get::<Inventory>(mob).cloned().unwrap_or_default();
    inv.uniform = carried(world, mob, UNIFORM);
    inv.shoes = carried(world, mob, SHOES);
    inv.ears = carried(world, mob, HEADSET);
    let agent_gear = role != AbductorRole::Scientist;
    if agent_gear {
        inv.head = carried(world, mob, HELMET);
        inv.suit = carried(world, mob, VEST);
        inv.suit_storage = carried(world, mob, BATON);
        inv.belt = carried(world, mob, BELT);
    }
    world.entity_mut(mob).insert(inv.clone());
    inv.back = crate::storage::spawn_backpack(world, mob);
    world.entity_mut(mob).insert(inv.clone());
    if let Some(pack) = inv.back {
        if role.scientist() {
            into_pack(world, pack, GIZMO);
        }
        if agent_gear {
            into_pack(world, pack, ALIEN_PISTOL);
            into_pack(world, pack, SILENCER);
        }
    }
    crate::inventory::refresh_look(world, mob);
}

/// `link_to_console`: the worn vest (locked on) and the science tools in the backpack.
pub fn link_to_console(world: &mut World, mob: Entity, team: u32) {
    let team = if team == 0 { 1 } else { team };
    let Some(console) = console_of(world, team) else {
        return;
    };
    let Some(inv) = world.get::<Inventory>(mob).cloned() else {
        return;
    };
    if let Some(vest) = inv.suit.filter(|&v| world.get::<AgentVest>(v).is_some()) {
        add_vest(world, console, vest);
        world.entity_mut(vest).insert(NoDrop);
    }
    let packed: Vec<Entity> = inv
        .back
        .and_then(|b| world.get::<crate::storage::Stored>(b))
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    for item in packed {
        if world.get::<ScienceTool>(item).is_some() {
            add_gizmo(world, console, item);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Checks

/// `AbductorCheck`: "no idea how this works!".
fn abductor_check(world: &mut World, client: Entity, mob: Entity) -> bool {
    if has_training(world, mob) {
        return true;
    }
    tell(
        world,
        client,
        Span::Warning,
        "понятия не имею, как это работает!",
    );
    false
}

/// `ScientistCheck`.
fn scientist_check(world: &mut World, client: Entity, mob: Entity) -> bool {
    match (has_training(world, mob), has_scientist_training(world, mob)) {
        (_, true) => true,
        (true, false) => {
            tell(
                world,
                client,
                Span::Warning,
                "Вас не учили этим пользоваться!",
            );
            false
        }
        (false, false) => {
            tell(
                world,
                client,
                Span::Warning,
                "Вы не можете понять, как это работает!",
            );
            false
        }
    }
}

/// `TRAIT_ABDUCTOR_QUICK_SCAN`: abductor bodies and cows.
fn quick_scan(world: &World, target: Entity) -> bool {
    is_abductor_species(world, target)
        || world
            .get::<ss13_mapload::DmObj>(target)
            .is_some_and(|o| crate::obj_damage::is_under(&o.path, COW_ROOT))
}

// ---------------------------------------------------------------------------------------------
// In hand

/// `attack_self` of the science tool, the baton and the mental interface device.
fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if let Some(g) = world.get::<ScienceTool>(item).copied() {
        if !scientist_check(world, client, mob) {
            return true;
        }
        if g.console.is_none_or(|c| world.get_entity(c).is_err()) {
            tell(
                world,
                client,
                Span::Warning,
                "Устройство не привязано к консоли!",
            );
            return true;
        }
        let mode = match g.mode {
            ToolMode::Scan => ToolMode::Mark,
            ToolMode::Mark => ToolMode::Scan,
        };
        if let Some(mut g) = world.get_mut::<ScienceTool>(item) {
            g.mode = mode;
        }
        let (state, text) = match mode {
            ToolMode::Scan => ("gizmo_scan", "СКАНИРОВАНИЯ"),
            ToolMode::Mark => ("gizmo_mark", "МЕТКИ"),
        };
        crate::obj_damage::set_icon_state(world, item, state);
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы переключаете устройство в режим {text}."),
        );
        return true;
    }
    if let Some(b) = world.get::<AbductorBaton>(item).copied() {
        if !abductor_check(world, client, mob) {
            return true;
        }
        let mode = b.mode.next();
        if let Some(mut b) = world.get_mut::<AbductorBaton>(item) {
            b.mode = mode;
        }
        crate::obj_damage::set_icon_state(world, item, mode.icon_state());
        crate::inventory::refresh_look(world, mob);
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы переключаете дубинку в режим {}.", mode.text()),
        );
        return true;
    }
    if let Some(d) = world.get::<MindDevice>(item).copied() {
        if !scientist_check(world, client, mob) {
            return true;
        }
        let control = !d.control;
        if let Some(mut d) = world.get_mut::<MindDevice>(item) {
            d.control = control;
        }
        let (state, text) = if control {
            ("mind_device_control", "КОМАНДЫ")
        } else {
            ("mind_device_message", "ПЕРЕДАЧИ")
        };
        crate::obj_damage::set_icon_state(world, item, state);
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы переключаете устройство в режим {text}."),
        );
        return true;
    }
    false
}

// ---------------------------------------------------------------------------------------------
// On mobs

/// The science tool, silencer and mental interface device on `target` (adjacent or ranged).
/// True when handled.
fn use_on(
    world: &mut World,
    client: Entity,
    mob: Entity,
    item: Entity,
    target: Entity,
    adjacent: bool,
) -> bool {
    if let Some(g) = world.get::<ScienceTool>(item).copied() {
        // Only mobs are scanned or marked.
        if world.get::<Health>(target).is_none() {
            return false;
        }
        if !has_scientist_training(world, mob) {
            scientist_check(world, client, mob);
            return false;
        }
        let Some(console) = g.console.filter(|&c| world.get_entity(c).is_ok()) else {
            tell(
                world,
                client,
                Span::Warning,
                "Устройство не привязано к консоли!",
            );
            return true;
        };
        let name = name_of(world, target);
        match g.mode {
            ToolMode::Scan => {
                if world.get::<crate::human::HumanLook>(target).is_some() {
                    add_snapshot(world, console, target);
                    tell(
                        world,
                        client,
                        Span::Notice,
                        format!("Вы сканируете {name} и добавляете в базу данных."),
                    );
                }
            }
            ToolMode::Mark => mark(world, client, mob, item, target),
        }
        return true;
    }
    if world.get::<Silencer>(item).is_some() {
        if world.get::<Health>(target).is_none() && adjacent {
            return false;
        }
        if !abductor_check(world, client, mob) {
            return false;
        }
        radio_off(world, client, mob, target);
        return true;
    }
    if let Some(d) = world.get::<MindDevice>(item).copied() {
        if world.get::<Health>(target).is_none() {
            return false;
        }
        if !scientist_check(world, client, mob) {
            return true;
        }
        if d.control {
            command_prompt(world, client, mob, item, target);
        } else {
            message_prompt(world, client, mob, item, target);
        }
        return true;
    }
    false
}

/// `gizmo/mark`, `prepare`.
fn mark(world: &mut World, client: Entity, mob: Entity, gizmo: Entity, target: Entity) {
    let name = name_of(world, target);
    if world.get::<ScienceTool>(gizmo).and_then(|g| g.marked) == Some(target) {
        tell(world, client, Span::Warning, "Этот образец уже помечен!");
        return;
    }
    if quick_scan(world, target) {
        if let Some(mut g) = world.get_mut::<ScienceTool>(gizmo) {
            g.marked = Some(target);
        }
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы помечаете {name} для последующего извлечения."),
        );
        return;
    }
    let near = match (world.get::<TilePos>(mob), world.get::<TilePos>(target)) {
        (Some(a), Some(b)) => a.chebyshev(*b).is_some_and(|d| d <= 1),
        _ => false,
    };
    if !near {
        tell(
            world,
            client,
            Span::Warning,
            "Нужно находиться рядом с образцом, чтобы подготовить его к транспортировке!",
        );
        return;
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы начинаете готовить {name} к транспортировке..."),
    );
    crate::tools::do_after(
        world,
        Actor {
            client,
            mob,
            item: Some(gizmo),
        },
        target,
        PREPARE_DS,
        0,
        crate::construction::Job::Abductor(AbductorJob::PrepareMark),
    );
}

/// The end of `prepare`'s `do_after`.
pub fn finish_prepare(world: &mut World, _mob: Entity, action: &crate::tools::DoAfter) {
    let Some(gizmo) = action.item else {
        return;
    };
    let name = name_of(world, action.target);
    if let Some(mut g) = world.get_mut::<ScienceTool>(gizmo) {
        g.marked = Some(action.target);
    } else {
        return;
    }
    tell(
        world,
        action.client,
        Span::Notice,
        format!("Вы заканчиваете готовить {name} к транспортировке."),
    );
}

/// `silencer/radio_off`: the radios of the humans within 2 tiles of the target.
fn radio_off(world: &mut World, client: Entity, user: Entity, target: Entity) {
    let (Some(&at), Some(&from)) = (world.get::<TilePos>(target), world.get::<TilePos>(user))
    else {
        return;
    };
    // `user in viewers(7, target)`.
    if at.chebyshev(from).is_none_or(|d| d > 7) {
        return;
    }
    let mut q = world.query_filtered::<(Entity, &TilePos), With<crate::human::HumanLook>>();
    let humans: Vec<Entity> = q
        .iter(world)
        .filter(|(e, p)| *e != user && p.chebyshev(at).is_some_and(|d| d <= 2))
        .map(|(e, _)| e)
        .collect();
    for human in humans {
        let name = name_of(world, human);
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы глушите радиоустройства {name}."),
        );
        for item in crate::antag::objectives::all_contents(world, human) {
            if let Some(mut radio) = world.get_mut::<crate::radio::Radio>(item) {
                radio.listening = false;
                if radio.kind != crate::radio::RadioKind::Headset {
                    radio.broadcasting = false;
                }
            }
        }
    }
}

fn command_prompt(world: &mut World, client: Entity, user: Entity, device: Entity, target: Entity) {
    let gland = crate::medical::organs::organ(world, target, crate::medical::organs::Slot::Heart)
        .filter(|&h| world.get::<Gland>(h).is_some());
    let Some(gland) = gland else {
        tell(
            world,
            client,
            Span::Warning,
            "У цели нет экспериментальной железы!",
        );
        return;
    };
    let Some(g) = world.get::<Gland>(gland).cloned() else {
        return;
    };
    if g.mind_control_uses == 0 {
        tell(world, client, Span::Warning, "Железа цели истощена!");
        return;
    }
    if g.mind_control_until.is_some() {
        tell(
            world,
            client,
            Span::Warning,
            "Цель уже под ментальным контролем!",
        );
        return;
    }
    let secs = g.def().mind_control_secs as u32;
    let ask = Ask::new(
        client,
        user,
        "Введите команду",
        format!(
            "Введите команду, которую должна выполнить цель. Осталось использований: {}, \
             длительность: {} мин. {} сек.",
            g.mind_control_uses,
            secs / 60,
            secs % 60
        ),
        PromptKind::Text {
            default: String::new(),
            max_length: MAX_MESSAGE_LEN,
            multiline: false,
        },
    );
    let prompt = ask_world(world, ask);
    world.entity_mut(prompt).insert(MindPrompt {
        device,
        target,
        control: true,
    });
}

fn message_prompt(world: &mut World, client: Entity, user: Entity, device: Entity, target: Entity) {
    if world.get::<Stat>(target) == Some(&Stat::Dead) {
        tell(world, client, Span::Warning, "Цель мертва!");
        return;
    }
    let ask = Ask::new(
        client,
        user,
        "Введите сообщение",
        "Сообщение, которое услышит мозг цели",
        PromptKind::Text {
            default: String::new(),
            max_length: MAX_MESSAGE_LEN,
            multiline: false,
        },
    );
    let prompt = ask_world(world, ask);
    world.entity_mut(prompt).insert(MindPrompt {
        device,
        target,
        control: false,
    });
}

/// The answers to the mental interface device, rechecked at the moment of effect.
fn mind_answers(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in pending {
        let Some(p) = world.get::<MindPrompt>(prompt).copied() else {
            continue;
        };
        if world.get::<PlayerMob>(actor).map(|m| m.client) != Some(client) {
            continue;
        }
        let Some(PromptAnswer::Text(text)) = answer else {
            continue;
        };
        let text = ss13_chat::server::sanitize_say(&text).unwrap_or_default();
        if text.trim().is_empty() || world.get_entity(p.target).is_err() {
            continue;
        }
        if p.control {
            // `user.get_active_held_item() != src`.
            let held = world
                .get::<Inventory>(actor)
                .and_then(Inventory::active_item)
                == Some(p.device);
            let gland =
                crate::medical::organs::organ(world, p.target, crate::medical::organs::Slot::Heart)
                    .filter(|&h| world.get::<Gland>(h).is_some());
            let (true, Some(gland)) = (held, gland) else {
                continue;
            };
            if mind_control(world, gland, &text, actor) {
                tell(world, client, Span::Notice, "Вы отправляете команду цели.");
            }
        } else {
            if world.get::<Stat>(p.target).is_none_or(|s| *s == Stat::Dead) {
                continue;
            }
            tell_mob(
                world,
                p.target,
                Span::Plain,
                format!("Вы слышите голос в своей голове: {text}"),
            );
            tell(
                world,
                client,
                Span::Notice,
                "Вы отправляете сообщение цели.",
            );
            info!("{actor} sent an abductor whisper to {}: {text}", p.target);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The advanced baton

/// The baton's hit on a living mob (from `crate::security`): true when the mode's effect
/// replaced the hit (an abductor's), false for anyone else's ordinary hit.
pub fn baton_attack(world: &mut World, actor: Actor, baton: Entity, target: Entity) -> bool {
    let Some(mode) = world.get::<AbductorBaton>(baton).map(|b| b.mode) else {
        return false;
    };
    if !has_training(world, actor.mob) {
        return false;
    }
    let speed = world.get::<DmItem>(baton).map_or(8.0, |i| i.attack_speed);
    let t = now(world);
    world
        .entity_mut(actor.mob)
        .insert(NextMove(t + f64::from(speed) / 10.0));
    let (user, victim, item) = (
        name_of(world, actor.mob),
        name_of(world, target),
        name_of(world, baton),
    );
    match mode {
        BatonMode::Stun => {
            crate::medical::visible(
                world,
                target,
                &format!("{user} оглушает вас с помощью {item}!"),
                &format!("{user} оглушает {victim} с помощью {item}!"),
                Span::Danger,
            );
            crate::status_effects::set_timed(world, target, Timed::Jitter, 40.0, true);
            crate::status_effects::set_timed(world, target, Timed::Confusion, 10.0, true);
            crate::status_effects::set_timed(world, target, Timed::Stutter, 16.0, true);
            crate::status_effects::apply(world, target, Incap::Paralyze, KNOCKDOWN_SECS);
        }
        BatonMode::Sleep => {
            let incapacitated = crate::status_effects::incapacitation(world, target).incapacitated
                || world
                    .get::<Stat>(target)
                    .is_some_and(|s| *s != Stat::Conscious);
            if incapacitated {
                crate::medical::visible(
                    world,
                    target,
                    "Вас внезапно клонит в сон!",
                    &format!("{user} усыпляет {victim} с помощью {item}!"),
                    Span::Danger,
                );
                crate::status_effects::apply(world, target, Incap::Sleeping, SLEEP_SECS);
            } else {
                crate::status_effects::adjust_timed(world, target, Timed::Drowsiness, 2.0, None);
                tell(
                    world,
                    actor.client,
                    Span::Warning,
                    "Усыпление полностью работает только на оглушённых образцах!",
                );
                crate::medical::visible(
                    world,
                    target,
                    "Вас внезапно клонит в сон!",
                    &format!("{user} пытается усыпить {victim} с помощью {item}!"),
                    Span::Danger,
                );
            }
        }
        BatonMode::Cuff => {
            if world
                .get::<crate::medical::organs::OrganHost>(target)
                .is_none()
                || world.get::<crate::restraints::Handcuffed>(target).is_some()
            {
                return true;
            }
            if let Some(&pos) = world.get::<TilePos>(target) {
                crate::sound::playsound(
                    world,
                    crate::sound::PlaySound::new(
                        pos,
                        "sound/items/weapons/cablecuff.ogg",
                        30.0,
                        true,
                    ),
                );
            }
            crate::medical::visible(
                world,
                target,
                &format!("{user} начинает формировать энергетическое поле вокруг ваших рук!"),
                &format!("{user} начинает сковывать {victim} с помощью {item}!"),
                Span::Danger,
            );
            crate::tools::do_after(
                world,
                actor,
                target,
                CUFF_DS,
                0,
                crate::construction::Job::Abductor(AbductorJob::Cuff),
            );
        }
        BatonMode::Probe => probe(world, actor.client, &user, &victim, target),
    }
    true
}

/// The end of the restraining `do_after`: hard-light cuffs.
pub fn finish_cuff(world: &mut World, _mob: Entity, action: &crate::tools::DoAfter) {
    let target = action.target;
    if world.get::<crate::restraints::Handcuffed>(target).is_some() {
        return;
    }
    let Some(cuffs) = crate::construction::spawn_detached(world, ENERGY_CUFFS) else {
        tell(
            world,
            action.client,
            Span::Warning,
            "Не удалось сковать цель.",
        );
        return;
    };
    crate::restraints::apply_cuffs(world, cuffs, target);
    let name = name_of(world, target);
    tell(
        world,
        action.client,
        Span::Notice,
        format!("Вы сковываете {name}."),
    );
}

/// `ProbeAttack`.
fn probe(world: &mut World, client: Entity, user: &str, victim: &str, target: Entity) {
    crate::medical::visible(
        world,
        target,
        &format!("{user} зондирует вас!"),
        &format!("{user} зондирует {victim}!"),
        Span::Danger,
    );
    let (species, help) = if world.get::<crate::human::HumanLook>(target).is_some() {
        let species = if world
            .get::<crate::antag::changeling::server::Changeling>(target)
            .is_some()
        {
            (Span::Warning, "Форма жизни генокрада".to_owned())
        } else {
            (
                Span::Notice,
                crate::species::def_of(world, target).name.to_owned(),
            )
        };
        let help = if !glands_of(world, target).is_empty() {
            (Span::Warning, "Обнаружена экспериментальная железа!")
        } else if crate::medical::organs::organ(world, target, crate::medical::organs::Slot::Heart)
            .is_some()
        {
            (Span::Notice, "Субъект пригоден для экспериментов.")
        } else {
            (Span::Warning, "Субъект непригоден для экспериментов.")
        };
        (species, help)
    } else {
        (
            (Span::Warning, "Неизвестный вид".to_owned()),
            (Span::Warning, "Вид непригоден для экспериментов."),
        )
    };
    tell(
        world,
        client,
        species.0,
        format!("Результат зондирования: {}", species.1),
    );
    tell(world, client, help.0, help.1);
}

// ---------------------------------------------------------------------------------------------
// The vest

/// `flip_mode`.
pub fn flip_vest(world: &mut World, vest: Entity) {
    let Some(mode) = world.get::<AgentVest>(vest).map(|v| v.mode) else {
        return;
    };
    let (mode, state) = if mode == VEST_STEALTH {
        (VEST_COMBAT, "vest_combat")
    } else {
        (VEST_STEALTH, "vest_stealth")
    };
    if let Some(mut v) = world.get_mut::<AgentVest>(vest) {
        v.mode = mode;
    }
    crate::obj_damage::set_icon_state(world, vest, state);
    if let Some(wearer) = world.get::<crate::inventory::CarriedBy>(vest).map(|c| c.0) {
        crate::inventory::refresh_look(world, wearer);
    }
}

/// `toggle_nodrop`.
pub fn toggle_nodrop(world: &mut World, vest: Entity) {
    let locked = world.get::<NoDrop>(vest).is_none();
    if locked {
        world.entity_mut(vest).insert(NoDrop);
    } else {
        world.entity_mut(vest).remove::<NoDrop>();
    }
    if let Some(wearer) = world.get::<crate::inventory::CarriedBy>(vest).map(|c| c.0) {
        let text = if locked {
            "Ваш жилет теперь заблокирован."
        } else {
            "Ваш жилет теперь разблокирован."
        };
        tell_mob(world, wearer, Span::Notice, text);
    }
}

// ---------------------------------------------------------------------------------------------
// Ranged

/// `ranged_interact_with_atom` of the held science tool, silencer and mental interface device.
fn ranged(world: &mut World, mut clicks: Local<MessageCursor<crate::click::RangedClick>>) {
    let clicks: Vec<crate::click::RangedClick> = clicks
        .read(world.resource::<Messages<crate::click::RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(held) = world
            .get::<Inventory>(click.mob)
            .and_then(Inventory::active_item)
        else {
            continue;
        };
        if world.get::<Stat>(click.mob) != Some(&Stat::Conscious) {
            continue;
        }
        if world.get::<ScienceTool>(held).is_some()
            || world.get::<Silencer>(held).is_some()
            || world.get::<MindDevice>(held).is_some()
        {
            use_on(world, click.client, click.mob, held, click.target, false);
        }
    }
}

pub(super) fn build_gear(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // The science tool, the baton and the mental interface device switch modes.
    app.add_click_handler(ClickPhase::AttackSelf, 893, |w, c| {
        attack_self(w, c.client, c.mob, c.target)
    });
    // The science tool, silencer and mental interface device on a mob next to the user.
    app.add_click_handler(ClickPhase::HeldOnTarget, 1813, |w, c| {
        c.held
            .is_some_and(|held| use_on(w, c.client, c.mob, held, c.target, true))
    });
    app.add_observer(adopt).add_systems(
        Update,
        (
            ranged.after(crate::click::ResolveClicks),
            mind_answers
                .after(PromptSystems::Answer)
                .in_set(UiSystems::Handle),
        ),
    );
}
