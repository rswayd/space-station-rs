//! A cyborg's chassis (server), after tg `robot_defense.dm` and the lock and emag procs of
//! `robot.dm` (`toggle_cover`, `SetLockdown`, `set_lockcharge`, `SetEmagged`, `emag_act`,
//! `borg_emag_end`) and the robot's `show_laws` (`robot/laws.dm`):
//!
//! - The cover: an ID (a card or a PDA with one) swiped by someone with robotics access
//!   (`req_one_access`; silicons are always allowed) locks or unlocks it while it is shut; a
//!   crowbar out of combat mode opens an unlocked cover or shuts an open one (`crowbar_act`).
//! - The cell (`/obj/item/stock_parts/power_store/cell/high` at spawn) sits in the chassis: an
//!   empty hand of a non-silicon takes it out of an open chassis (`attack_hand`), a cell goes
//!   into an open one without a cell. The cell's charge lives in [`Cyborg::charge`] while it is
//!   installed and goes back to the cell item when it comes out; without a cell the cyborg is
//!   in low power mode (`crate::silicon::cyborg`). An open chassis is not punched.
//! - Upgrade boards go into an open chassis ([`super::borg_upgrades`]).
//! - An emag (`emag_act`): on a shut cover it breaks the lock; on an open one (10 s cooldown) it
//!   hacks the interface: the cyborg is emagged (its model's `emag_modules`), paralyzed and
//!   stunned for 10 s, unlinked from its AI without lawsync, and 6.5 s of `borg_emag_end` text
//!   later it has SyndOS laws with the emagger as its master in the zeroth law.
//! - Lockdown (`SetLockdown`, from the robotics console, [`super::robotics`]): the cyborg is
//!   immobilized and its clicks stop ([`LockedDown`] in `crate::status_effects`); unless the AI
//!   locked it, it overrides the lockdown itself after 10 minutes (`lockdown_override`).
//! - Laws (`show_laws`): synced with the AI first when `lawupdate` is on, then the list and who
//!   the cyborg's master is; shown on joining and whenever lawsync changes them.
// shortcut: no wires (`/datum/wires/robot`), no screwdriver (wires, radio keys), no wrench
// deconstruction, no welder or cable repair, hats, toner, headlamp, encryption keys,
// defibrillator or RPED installation, no malf AI counter-emag and no AI shells; the lock and
// cover messages are chat lines (tg balloon alerts), and the cover has no `ov-opencover`
// overlay.
// shortcut: the zeroth law names the emagger with "they" (tg `p_they()` pronouns).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmName};
use ss13_shared::chat::Span;

use super::ai::AiCore;
use super::cyborg::{Cyborg, RobotModule};
use super::laws::Laws;
use super::{Silicon, Silicons};
use crate::click::{ClickResolved, ResolveClicks};
use crate::combat::{CombatMode, NextMove};
use crate::health::Stat;
use crate::inventory::{IdCard, Inventory};
use crate::mecha::core::ItemCell;
use crate::server::PlayerMob;
use crate::status_effects::Incap;
use crate::tools::{Actor, ToolKind};

/// `/obj/item/stock_parts/power_store/cell` and subtypes.
pub const CELL: &str = "/obj/item/stock_parts/power_store/cell";
/// tg `req_one_access = list(ACCESS_ROBOTICS)`.
pub const ROBOTICS_ACCESS: &str = "robotics";
/// `emag_cooldown = world.time + 100`.
pub const EMAG_COOLDOWN_SECS: f64 = 10.0;
/// `Paralyze(10 SECONDS)`, `SetStun(10 SECONDS)` of `emag_act`.
pub const EMAG_STUN_SECS: f32 = 10.0;
/// `lockdown_timer`: 10 minutes until the cyborg overrides a console's lockdown.
pub const LOCKDOWN_OVERRIDE_SECS: f64 = 600.0;
/// `/datum/ai_laws/syndicate_override` `id`.
pub(crate) const SYNDICATE_LAWS: &str = "syndie";

/// `borg_emag_end`: each line after the sleep before it.
const EMAG_END: [(f64, &str); 7] = [
    (0.0, "ALERT: Foreign software detected."),
    (0.5, "Initiating diagnostics..."),
    (2.0, "SynBorg v1.7 loaded."),
    (0.5, "LAW SYNCHRONISATION ERROR"),
    (0.5, "Would you like to send a report to NanoTraSoft? Y/N"),
    (1.0, "> N"),
    (2.0, "ERRORERRORERROR"),
];

/// The chassis state of a cyborg (tg `/mob/living/silicon/robot` vars).
#[derive(Component, Clone, Debug)]
pub struct Chassis {
    /// tg `opened`: the cover.
    pub opened: bool,
    /// tg `locked`: the cover lock.
    pub locked: bool,
    /// The installed cell item (inside the cyborg).
    pub cell: Option<Entity>,
    pub emagged: bool,
    /// tg `emag_cooldown`, elapsed seconds.
    pub emag_ready_at: f64,
    /// tg `lockcharge`.
    pub lockcharge: bool,
    /// tg `ai_lockdown`: the AI locked it (no self override).
    pub ai_lockdown: bool,
    /// tg `lockdown_timer`.
    pub lockdown_override_at: Option<f64>,
    /// tg `scrambledcodes`: hidden from robotics consoles.
    pub scrambledcodes: bool,
    /// Installed upgrade boards (inside the cyborg).
    pub upgrades: Vec<Entity>,
}

impl Default for Chassis {
    fn default() -> Self {
        Self {
            opened: false,
            locked: true,
            cell: None,
            emagged: false,
            emag_ready_at: 0.0,
            lockcharge: false,
            ai_lockdown: false,
            lockdown_override_at: None,
            scrambledcodes: false,
            upgrades: Vec::new(),
        }
    }
}

/// tg `TRAIT_IMMOBILIZED` from `LOCKED_BORG_TRAIT`, and robot `ClickOn`'s `lockcharge` return:
/// read by `crate::status_effects` (immobilized, hands blocked).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct LockedDown;

/// A module of the model's `emag_modules`: there while the cyborg is emagged.
#[derive(Component, Clone, Copy, Debug)]
pub struct EmagModule;

/// `borg_emag_end` in progress: the next line of [`EMAG_END`] and when.
#[derive(Component, Clone, Debug)]
pub struct EmagSequence {
    pub master: String,
    pub step: usize,
    pub next_at: f64,
}

pub struct ChassisPlugin;

impl Plugin for ChassisPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                chassis_clicks
                    .after(ResolveClicks)
                    .after(crate::medical::MedicalClicks)
                    .after(crate::security::security_clicks)
                    .before(crate::combat::MeleeSystems)
                    .before(crate::inventory::inventory_intents),
                emag_sequences.run_if(any_with_component::<EmagSequence>),
                lockdown_overrides,
            ),
        );
    }
}

use crate::tools::now;

pub(crate) fn name_of(world: &World, e: Entity) -> String {
    if let Some(name) = world.get::<Name>(e) {
        return name.as_str().to_owned();
    }
    world
        .get::<DmName>(e)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default()
}

pub(crate) use crate::tools::tell_mob;

/// `to_chat(connected_ai, ...)`: the AI's client is on its eye.
pub(crate) fn tell_ai(world: &mut World, ai: Entity, span: Span, text: impl Into<String>) {
    if let Some(eye) = world.get::<AiCore>(ai).map(|c| c.eye) {
        tell_mob(world, eye, span, text);
    }
}

/// The chassis at spawn: a high-capacity cell inside (tg `cell = /obj/item/.../cell/high`).
pub(crate) fn setup(world: &mut World, borg: Entity) {
    let cell = crate::mecha::core::spawn_inside(world, ss13_mapload::silicon::ROBOT_CELL, borg);
    world.entity_mut(borg).insert(Chassis { cell, ..default() });
}

fn is_cell(world: &World, item: Entity) -> bool {
    world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, CELL))
}

/// tg `allowed(user)` with the robot's `req_one_access`: silicons always, else the ID found
/// by `get_idcard(hand_first = TRUE)`.
pub(crate) fn allowed(world: &World, user: Entity, access: &str) -> bool {
    if super::interact::has_silicon_access(world, user) {
        return true;
    }
    world
        .get::<Inventory>(user)
        .and_then(|inv| inv.id_card(world))
        .and_then(|card| world.get::<IdCard>(card))
        .is_some_and(|card| card.access.iter().any(|a| a == access))
}

/// Clicks on a cyborg that its chassis takes, before melee and the inventory (they read
/// `crate::medical::MedicalHandled`).
fn chassis_clicks(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    let t = now(world);
    for click in clicks {
        let (mob, borg) = (click.mob, click.target);
        if world.get::<Chassis>(borg).is_none() || click.modifiers.right {
            continue;
        }
        let taken = {
            let handled = world.resource::<crate::medical::MedicalHandled>();
            let melee = world.resource::<crate::security::MeleeHandled>();
            handled.0.contains(&mob) || melee.0.contains(&mob)
        };
        if taken || world.get::<NextMove>(mob).is_some_and(|n| t < n.0) {
            continue;
        }
        let held = world.get::<Inventory>(mob).and_then(Inventory::active_item);
        let actor = Actor {
            client: click.client,
            mob,
            item: held,
        };
        let handled = match held {
            Some(item) => item_interaction(world, actor, item, borg),
            None => attack_hand(world, actor, borg),
        };
        if handled {
            world
                .resource_mut::<crate::medical::MedicalHandled>()
                .0
                .insert(mob);
        }
    }
}

/// tg `base_item_interaction` of the robot: tool acts out of combat mode, `item_interaction`
/// (cells, upgrades), the emag's `interact_with_atom`, then an ID on the lock. True when taken.
pub fn item_interaction(world: &mut World, actor: Actor, item: Entity, borg: Entity) -> bool {
    let Some(chassis) = world.get::<Chassis>(borg).cloned() else {
        return false;
    };
    let combat = world.get::<CombatMode>(actor.mob).is_some_and(|c| c.0);
    let user = actor.client;
    if !combat
        && crate::tools::tool_of(world, item).is_some_and(|(kind, _)| kind == ToolKind::Crowbar)
    {
        crowbar_act(world, user, borg);
        return true;
    }
    if is_cell(world, item) && chassis.opened {
        insert_cell(world, user, borg, item);
        return true;
    }
    if super::borg_upgrades::is_upgrade(world, item) {
        super::borg_upgrades::try_install(world, actor, item, borg);
        return true;
    }
    if crate::emag::is_emag(world, item) {
        return crate::emag::interact_with_atom(world, actor, borg);
    }
    if crate::inventory::get_id(world, item).is_some() {
        if chassis.opened {
            crate::tools::tell(world, user, Span::Warning, "close the chassis cover first!");
        } else if !allowed(world, actor.mob, ROBOTICS_ACCESS) {
            crate::tools::tell(world, user, Span::Warning, "access denied!");
        } else {
            toggle_cover(world, borg, Some(user));
        }
        return true;
    }
    false
}

/// tg robot `attack_hand`: an open chassis gives its cell to a non-silicon's empty hand and is
/// never punched.
pub fn attack_hand(world: &mut World, actor: Actor, borg: Entity) -> bool {
    let Some(chassis) = world.get::<Chassis>(borg).cloned() else {
        return false;
    };
    if !chassis.opened {
        return false;
    }
    if world.get::<Silicon>(actor.mob).is_some() {
        return false;
    }
    if let Some(cell) = chassis.cell {
        remove_cell(world, borg, cell);
        crate::forensics::server::add_fingerprint(world, cell, actor.mob, false);
        if let Some(inventory) = world.get::<Inventory>(actor.mob).cloned() {
            crate::storage::put_in_hands(world, actor.mob, &inventory, cell);
        }
        crate::tools::tell(world, actor.client, Span::Notice, "cell removed");
    }
    true
}

/// tg `crowbar_act`.
fn crowbar_act(world: &mut World, user: Entity, borg: Entity) {
    let Some(mut chassis) = world.get_mut::<Chassis>(borg) else {
        return;
    };
    let text = if chassis.opened {
        chassis.opened = false;
        "chassis cover closed"
    } else if chassis.locked {
        "chassis cover locked!"
    } else {
        chassis.opened = true;
        "chassis cover opened"
    };
    crate::tools::tell(world, user, Span::Notice, text);
}

/// tg `toggle_cover(user)`: `user` is the client of whoever swiped (not the cyborg itself).
pub fn toggle_cover(world: &mut World, borg: Entity, user: Option<Entity>) {
    let Some(mut chassis) = world.get_mut::<Chassis>(borg) else {
        return;
    };
    chassis.locked = !chassis.locked;
    let (locked, emagged) = (chassis.locked, chassis.emagged);
    let state = if locked { "locked" } else { "unlocked" };
    tell_mob(world, borg, Span::Notice, format!("cover {state}"));
    let log = format!(
        "{} cover lock has been {}",
        if emagged { "ChÃ¥vÃis" } else { "Chassis" },
        if locked { "engaged" } else { "released" }
    );
    crate::modpc::silicon::logevent(world, borg, &log);
    if let Some(user) = user {
        let text = if emagged {
            "chassis cover lock glitches".to_owned()
        } else {
            format!("chassis cover {state}")
        };
        crate::tools::tell(world, user, Span::Notice, text);
    }
}

/// tg `item_interaction` with a cell on an open chassis.
fn insert_cell(world: &mut World, user: Entity, borg: Entity, cell: Entity) {
    if world.get::<Chassis>(borg).is_some_and(|c| c.cell.is_some()) {
        crate::tools::tell(world, user, Span::Warning, "already has a cell!");
        return;
    }
    let state = world.get::<ItemCell>(cell).map(|c| c.0);
    crate::storage::put_inside(world, cell, borg);
    if let Some(mut chassis) = world.get_mut::<Chassis>(borg) {
        chassis.cell = Some(cell);
    }
    if let Some(mut cyborg) = world.get_mut::<Cyborg>(borg) {
        cyborg.charge = state.map_or(0.0, |c| c.charge);
        cyborg.max_charge = state.map_or(0.0, |c| c.maxcharge);
    }
    crate::tools::tell(world, user, Span::Notice, "cell inserted");
}

/// The cell leaves the chassis with the cyborg's charge; the cyborg has none left.
fn remove_cell(world: &mut World, borg: Entity, cell: Entity) {
    let (charge, maxcharge) = world
        .get::<Cyborg>(borg)
        .map_or((0.0, 0.0), |c| (c.charge, c.max_charge));
    let chargerate = world.get::<ItemCell>(cell).map_or(0.0, |c| c.0.chargerate);
    world.entity_mut(cell).insert(ItemCell(crate::power::Cell {
        charge,
        maxcharge,
        chargerate,
    }));
    if let Some(mut chassis) = world.get_mut::<Chassis>(borg) {
        chassis.cell = None;
    }
    if let Some(mut cyborg) = world.get_mut::<Cyborg>(borg) {
        cyborg.charge = 0.0;
        cyborg.max_charge = 0.0;
    }
}

/// tg robot `emag_act(user, emag_card)`. True when it took.
pub fn emag_act(world: &mut World, actor: Actor, borg: Entity) -> bool {
    // "To prevent syndieborgs from emagging themselves".
    if actor.mob == borg {
        return false;
    }
    let Some(chassis) = world.get::<Chassis>(borg).cloned() else {
        return false;
    };
    let user = actor.client;
    if !chassis.opened {
        if !chassis.locked {
            crate::tools::tell(world, user, Span::Warning, "cover already unlocked!");
            return false;
        }
        if let Some(mut c) = world.get_mut::<Chassis>(borg) {
            c.locked = false;
        }
        crate::tools::tell(world, user, Span::Notice, "cover lock destroyed");
        return true;
    }
    let t = now(world);
    if t < chassis.emag_ready_at {
        return false;
    }
    crate::tools::tell(world, user, Span::Notice, "interface hacked");
    if let Some(mut c) = world.get_mut::<Chassis>(borg) {
        c.emag_ready_at = t + EMAG_COOLDOWN_SECS;
    }
    set_emagged(world, borg, true);
    crate::status_effects::apply(world, borg, Incap::Paralyze, EMAG_STUN_SECS);
    if let Some(mut effects) = crate::status_effects::effects_mut(world, borg) {
        effects.set(Incap::Stun, EMAG_STUN_SECS);
    }
    if let Some(mut cyborg) = world.get_mut::<Cyborg>(borg) {
        cyborg.lawupdate = false;
    }
    super::cyborg::disconnect(world, borg);
    let master = name_of(world, actor.mob);
    info!(
        "EMAG: {master} ({}) emagged cyborg {}. Laws overridden.",
        actor.mob,
        name_of(world, borg)
    );
    world.entity_mut(borg).insert(EmagSequence {
        master,
        step: 0,
        next_at: t,
    });
    true
}

/// The journal lines of `borg_emag_end`, by the step whose text they follow.
fn emag_log(world: &mut World, borg: Entity, step: usize, master: &str) {
    let line = match step {
        0 => "ALERT: Foreign software detected.".to_owned(),
        // `num2hex(rand(1,65535), -1)` twice.
        2 => {
            let mut rng = world.get_resource_or_init::<crate::combat::CombatRng>();
            let (a, b) = (rng.range(1, 0xFFFF), rng.range(1, 0xFFFF));
            format!("WARN: root privleges granted to PID {a:X}{b:X}.")
        }
        3 => format!("LOG: New user [{}], groups [root]", master.replace(' ', "")),
        _ => return,
    };
    crate::modpc::silicon::logevent(world, borg, &line);
}

/// tg `SetEmagged`: the model's `emag_modules` come or go (`rebuild_modules`).
pub fn set_emagged(world: &mut World, borg: Entity, state: bool) {
    let Some(mut chassis) = world.get_mut::<Chassis>(borg) else {
        return;
    };
    if chassis.emagged == state {
        return;
    }
    chassis.emagged = state;
    if state {
        add_emag_modules(world, borg);
        return;
    }
    let Some(model) = world.get::<Cyborg>(borg).and_then(|c| c.model_item) else {
        return;
    };
    super::cyborg::stow_held(world, borg, false);
    let modules: Vec<Entity> = world
        .get::<crate::storage::Stored>(model)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    for module in modules {
        if world.get::<EmagModule>(module).is_some() {
            world.entity_mut(module).despawn();
        }
    }
}

/// The model's `emag_modules` into its storage (an emagged cyborg's model).
pub(crate) fn add_emag_modules(world: &mut World, borg: Entity) {
    let Some(cyborg) = world.get::<Cyborg>(borg) else {
        return;
    };
    let (Some(index), Some(item)) = (cyborg.model, cyborg.model_item) else {
        return;
    };
    let Some(&pos) = world.get::<ss13_shared::TilePos>(borg) else {
        return;
    };
    let paths = world
        .resource::<Silicons>()
        .0
        .models
        .get(index)
        .map(|m| m.emag_modules.clone())
        .unwrap_or_default();
    for path in paths {
        if let Some(module) = crate::construction::spawn_obj(world, &path, pos) {
            world.entity_mut(module).insert((RobotModule, EmagModule));
            crate::storage::put_inside(world, module, item);
        }
    }
}

/// The SyndOS lawset (`/datum/ai_laws/syndicate_override`) with `master` in the zeroth law.
pub fn syndicate_laws(world: &World, master: &str) -> Laws {
    let mut laws = world
        .resource::<Silicons>()
        .0
        .lawset(SYNDICATE_LAWS)
        .map(Laws::from_lawset)
        .unwrap_or_else(|| Laws {
            name: "SyndOS 3.1".into(),
            ..default()
        });
    laws.zeroth = Some(format!(
        "Only {master} and people they designate as being such are Syndicate Agents."
    ));
    laws.zeroth_borg = None;
    laws
}

/// `borg_emag_end`'s lines, then the new laws.
fn emag_sequences(world: &mut World) {
    let t = now(world);
    let mut q = world.query::<(Entity, &EmagSequence)>();
    let due: Vec<(Entity, EmagSequence)> = q
        .iter(world)
        .filter(|(_, s)| t >= s.next_at)
        .map(|(e, s)| (e, s.clone()))
        .collect();
    for (borg, seq) in due {
        let Some(&(_, line)) = EMAG_END.get(seq.step) else {
            world.entity_mut(borg).remove::<EmagSequence>();
            continue;
        };
        tell_mob(world, borg, Span::Danger, line);
        emag_log(world, borg, seq.step, &seq.master);
        let next = seq.step + 1;
        if let Some(&(delay, _)) = EMAG_END.get(next) {
            world.entity_mut(borg).insert(EmagSequence {
                step: next,
                next_at: t + delay,
                ..seq
            });
            continue;
        }
        world.entity_mut(borg).remove::<EmagSequence>();
        // A rack would overwrite the new laws on its next update (tg keeps the link).
        super::law_rack::forget_link(world, borg);
        let laws = syndicate_laws(world, &seq.master);
        world.entity_mut(borg).insert(laws);
        tell_mob(
            world,
            borg,
            Span::Danger,
            format!(
                "ALERT: {} is your new master. Obey your new laws and their commands.",
                seq.master
            ),
        );
        show_laws(world, borg);
    }
}

/// tg `SetLockdown(state)`: a lockdown not by the AI overrides itself after 10 minutes.
pub fn set_lockdown(world: &mut World, borg: Entity, state: bool) {
    let t = now(world);
    let Some(mut chassis) = world.get_mut::<Chassis>(borg) else {
        return;
    };
    let ai_lockdown = chassis.ai_lockdown;
    chassis.lockdown_override_at = (state && !ai_lockdown).then_some(t + LOCKDOWN_OVERRIDE_SECS);
    if state && !ai_lockdown {
        tell_mob(
            world,
            borg,
            Span::Warning,
            "ALERT - Remote system lockdown engaged. Trying to hack the lockdown subsystem...",
        );
    }
    set_lockcharge(world, borg, state);
}

/// tg `set_lockcharge`.
pub fn set_lockcharge(world: &mut World, borg: Entity, state: bool) {
    let Some(mut chassis) = world.get_mut::<Chassis>(borg) else {
        return;
    };
    if chassis.lockcharge == state {
        return;
    }
    chassis.lockcharge = state;
    if state {
        world.entity_mut(borg).insert(LockedDown);
    } else {
        world.entity_mut(borg).remove::<LockedDown>();
    }
    let log = format!(
        "System lockdown {}.",
        if state { "triggered" } else { "released" }
    );
    crate::modpc::silicon::logevent(world, borg, &log);
}

/// tg `lockdown_override` when the 10 minutes are over.
fn lockdown_overrides(world: &mut World) {
    let t = now(world);
    let mut q = world.query::<(Entity, &Chassis)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, c)| c.lockdown_override_at.is_some_and(|at| t >= at))
        .map(|(e, _)| e)
        .collect();
    for borg in due {
        let Some(mut chassis) = world.get_mut::<Chassis>(borg) else {
            continue;
        };
        chassis.lockdown_override_at = None;
        if chassis.ai_lockdown {
            tell_mob(
                world,
                borg,
                Span::Warning,
                "ALERT - Remote system lockdown override failed.",
            );
            continue;
        }
        set_lockcharge(world, borg, false);
        tell_mob(
            world,
            borg,
            Span::Notice,
            "ALERT - Remote system lockdown override successful.",
        );
        if let Some(ai) = world.get::<Cyborg>(borg).and_then(|c| c.connected_ai) {
            let text = format!(
                "ALERT - Cyborg {} successfully overriden the lockdown system",
                name_of(world, borg)
            );
            tell_ai(world, ai, Span::Notice, text);
        }
    }
}

/// tg robot `show_laws`: lawsync first when `lawupdate` is on, the laws, then who the master
/// is.
pub fn show_laws(world: &mut World, borg: Entity) {
    let Some(client) = world.get::<PlayerMob>(borg).map(|p| p.client) else {
        return;
    };
    let Some(cyborg) = world.get::<Cyborg>(borg).cloned() else {
        return;
    };
    if cyborg.lawupdate {
        match cyborg
            .connected_ai
            .filter(|&ai| world.get::<AiCore>(ai).is_some())
        {
            Some(ai) => {
                let lost = world.get::<Stat>(ai) != Some(&Stat::Conscious)
                    || world.get::<AiCore>(ai).is_some_and(|c| c.control_disabled);
                if lost {
                    super::notice(
                        world,
                        client,
                        Span::Bold,
                        "AI signal lost, unable to sync laws.",
                    );
                } else {
                    super::cyborg::lawsync(world, borg);
                    super::notice(
                        world,
                        client,
                        Span::Bold,
                        "Laws synced with AI, be sure to note any changes.",
                    );
                }
            }
            None => {
                super::notice(
                    world,
                    client,
                    Span::Bold,
                    "No AI selected to sync laws with, disabling lawsync protocol.",
                );
                if let Some(mut c) = world.get_mut::<Cyborg>(borg) {
                    c.lawupdate = false;
                }
            }
        }
    }
    if let Some(laws) = world.get::<Laws>(borg).cloned() {
        super::show_laws(world, client, &laws);
    }
    let emagged = world.get::<Chassis>(borg).is_some_and(|c| c.emagged);
    let master = cyborg
        .connected_ai
        .filter(|&ai| world.get::<AiCore>(ai).is_some());
    let text = match master {
        Some(ai) => format!(
            "Remember, {} is your master, other AIs can be ignored.",
            name_of(world, ai)
        ),
        None if emagged => "Remember, you are not required to listen to the AI.".to_owned(),
        None => "Remember, you are not bound to any AI, you are not required to listen to them."
            .to_owned(),
    };
    super::notice(world, client, Span::Bold, text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emag_end_takes_six_and_a_half_seconds() {
        let total: f64 = EMAG_END.iter().map(|(d, _)| d).sum();
        assert!((total - 6.5).abs() < 1e-9);
        assert_eq!(EMAG_END[0].0, 0.0);
    }

    #[test]
    fn chassis_starts_shut_and_locked() {
        let c = Chassis::default();
        assert!(c.locked && !c.opened && !c.emagged && !c.lockcharge);
    }

    #[test]
    fn lockcharge_toggles_the_lockdown_marker() {
        let mut world = World::new();
        let borg = world.spawn(Chassis::default()).id();
        set_lockcharge(&mut world, borg, true);
        assert!(world.get::<LockedDown>(borg).is_some());
        assert!(world.get::<Chassis>(borg).unwrap().lockcharge);
        set_lockcharge(&mut world, borg, false);
        assert!(world.get::<LockedDown>(borg).is_none());
    }

    #[test]
    fn crowbar_needs_the_lock_open() {
        let mut world = World::new();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        let user = world.spawn_empty().id();
        let borg = world.spawn(Chassis::default()).id();
        crowbar_act(&mut world, user, borg);
        assert!(!world.get::<Chassis>(borg).unwrap().opened);
        toggle_cover(&mut world, borg, Some(user));
        assert!(!world.get::<Chassis>(borg).unwrap().locked);
        crowbar_act(&mut world, user, borg);
        assert!(world.get::<Chassis>(borg).unwrap().opened);
        crowbar_act(&mut world, user, borg);
        assert!(!world.get::<Chassis>(borg).unwrap().opened);
    }

    fn cyborg(charge: f64, max_charge: f64, connected_ai: Option<Entity>) -> Cyborg {
        Cyborg {
            model: None,
            model_item: None,
            charge,
            max_charge,
            low_power: false,
            connected_ai,
            lawupdate: true,
        }
    }

    #[test]
    fn the_cell_comes_out_with_its_charge() {
        let mut world = World::new();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        let user = world.spawn_empty().id();
        let cell = world
            .spawn(ItemCell(crate::power::Cell {
                charge: 5.0,
                maxcharge: 10.0,
                chargerate: 1.0,
            }))
            .id();
        let borg = world
            .spawn((
                Chassis {
                    cell: Some(cell),
                    opened: true,
                    ..default()
                },
                cyborg(7.0, 100.0, None),
            ))
            .id();
        remove_cell(&mut world, borg, cell);
        let c = world.get::<ItemCell>(cell).unwrap().0;
        assert_eq!((c.charge, c.maxcharge, c.chargerate), (7.0, 100.0, 1.0));
        let b = world.get::<Cyborg>(borg).unwrap();
        assert_eq!((b.charge, b.max_charge), (0.0, 0.0));
        assert!(world.get::<Chassis>(borg).unwrap().cell.is_none());
        insert_cell(&mut world, user, borg, cell);
        let b = world.get::<Cyborg>(borg).unwrap();
        assert_eq!((b.charge, b.max_charge), (7.0, 100.0));
        assert_eq!(world.get::<Chassis>(borg).unwrap().cell, Some(cell));
    }

    #[test]
    fn emag_breaks_the_lock_then_hacks_the_open_interface() {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        let ai = world
            .spawn(AiCore {
                eye: Entity::PLACEHOLDER,
                control_disabled: false,
                connected_robots: Vec::new(),
            })
            .id();
        let borg = world
            .spawn((Chassis::default(), cyborg(0.0, 0.0, Some(ai))))
            .id();
        world
            .get_mut::<AiCore>(ai)
            .unwrap()
            .connected_robots
            .push(borg);
        let agent = world.spawn(Name::new("Agent")).id();
        let actor = Actor {
            client: agent,
            mob: agent,
            item: None,
        };
        // A shut cover: the lock breaks, once.
        assert!(emag_act(&mut world, actor, borg));
        let c = world.get::<Chassis>(borg).unwrap();
        assert!(!c.locked && !c.emagged);
        assert!(!emag_act(&mut world, actor, borg));
        // An open one: the interface.
        world.get_mut::<Chassis>(borg).unwrap().opened = true;
        assert!(emag_act(&mut world, actor, borg));
        assert!(world.get::<Chassis>(borg).unwrap().emagged);
        let b = world.get::<Cyborg>(borg).unwrap();
        assert!(b.connected_ai.is_none() && !b.lawupdate);
        assert!(world.get::<AiCore>(ai).unwrap().connected_robots.is_empty());
        assert_eq!(world.get::<EmagSequence>(borg).unwrap().master, "Agent");
        // The 10 s cooldown; the cyborg never emags itself.
        assert!(!emag_act(&mut world, actor, borg));
        let own = Actor { mob: borg, ..actor };
        assert!(!emag_act(&mut world, own, borg));
    }
}
