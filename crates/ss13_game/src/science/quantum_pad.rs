//! Quantum pads and quantum keycards (server), after tg `/obj/machinery/quantumpad`
//! (`code/game/machinery/quantum_pad.dm`) and `/obj/item/quantum_keycard`
//! (`code/game/objects/items/devices/quantum_keycard.dm`).
//!
//! - A multitool on a pad with its panel open saves the pad to the multitool's buffer; on a pad
//!   with the panel closed it links that pad to the buffered one (not to itself). The link is
//!   one way.
//! - An empty hand on a powered pad (or a linked keycard on any pad: that pad sends to the
//!   card's pad instead) charges a teleport: refused while recharging (`teleport_cooldown`),
//!   charging, while the target is charging or unpowered. `teleport_speed` later every
//!   unanchored thing on the pad (not a mob buckled to something anchored) goes to the target
//!   pad's tile through [`crate::teleport::do_teleport`] (quantum channel): both pads spark,
//!   flash their beam and hum, the source pad uses `active_power_usage / power_efficiency`.
//! - `RefreshParts`: `power_efficiency` = the capacitor tiers, `teleport_speed` = 5 s - 1 s and
//!   `teleport_cooldown` = 40 s - 10 s per servo tier.
//! - An unlinked keycard on a pad links to it after 4 s; alt-clicking the keycard in hand
//!   unlinks it after 4 s.
// shortcut: no USB circuit component (wiremod has no `usb_port` yet), no mapped `map_pad_id`
// links (no map uses them), no ghost click to the linked pad, no keycard recolouring by the
// pad's department (no greyscale configs at runtime), no "Quantum Pad For Dummies" paper.

use bevy::prelude::*;
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::SpatialIndex;

use crate::construction::{Job, PanelOpen, path_of};
use crate::examine::{Line, name_of};
use crate::obj_damage::is_under;
use crate::stock_parts::PartKind;
use crate::teleport::{Channel, Tele, do_teleport};
use crate::tools::{Actor, ToolKind, do_after, tell, tool_of};

pub const PAD: &str = "/obj/machinery/quantumpad";
pub const KEYCARD: &str = "/obj/item/quantum_keycard";
const FLASH: &str = "sound/items/weapons/flash.ogg";
const EMITTER: &str = "sound/items/weapons/emitter2.ogg";
/// `initial(teleport_speed)`, `initial(teleport_cooldown)`, deciseconds.
const BASE_SPEED_DS: f64 = 50.0;
const BASE_COOLDOWN_DS: f64 = 400.0;
/// `BASE_MACHINE_ACTIVE_CONSUMPTION * 10`, joules.
const ACTIVE_POWER: f64 = 10_000.0;
/// The keycard's link and unlink `do_after`s, deciseconds.
const CARD_DS: f32 = 40.0;
/// `do_sparks(5, TRUE, src, spark_type = quantum)`.
const SPARKS: u32 = 5;

/// A pad's live state (absent: never used).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct QuantumPad {
    /// `linked_pad`.
    pub linked: Option<Entity>,
    /// `last_teleport`, game seconds.
    pub last_teleport: Option<f64>,
    /// `teleporting`: the charging teleport.
    pub charging: Option<Charge>,
}

/// A charging teleport: to `target` at `at` (game seconds), for `client`'s feedback.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Charge {
    pub target: Entity,
    pub at: f64,
    pub client: Option<Entity>,
}

/// A keycard's `qpad`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QuantumKeycard(pub Option<Entity>);

/// The keycard's timed actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadJob {
    /// The card in hand linking to the target pad.
    LinkCard(Entity),
    /// The card's unlink button.
    UnlinkCard(Entity),
}

pub struct QuantumPadPlugin;

impl Plugin for QuantumPadPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // `multitool_act` and the keycard's `item_interaction` (before `machine_frame`).
        app.add_click_handler(ClickPhase::HeldOnTarget, 1597, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
        // `attack_hand` -> `interact`.
        app.add_click_handler(ClickPhase::EmptyHand, 3467, |w, c| {
            attack_hand(w, c.client, c.mob, c.target)
        });
        app.add_systems(
            Update,
            finish_teleports.run_if(any_with_component::<QuantumPad>),
        );
    }
}

pub fn is_pad(world: &World, e: Entity) -> bool {
    world
        .get::<DmObj>(e)
        .is_some_and(|o| is_under(&o.path, PAD))
}

fn is_keycard(world: &World, e: Entity) -> bool {
    world
        .get::<DmItem>(e)
        .is_some_and(|i| is_under(&i.path, KEYCARD))
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn state(world: &World, pad: Entity) -> QuantumPad {
    world.get::<QuantumPad>(pad).copied().unwrap_or_default()
}

fn unpowered(world: &World, pad: Entity) -> bool {
    world.get::<crate::power::Unpowered>(pad).is_some()
}

/// A pad that still exists (`QDELETED`).
fn live_pad(world: &World, pad: Option<Entity>) -> Option<Entity> {
    pad.filter(|&p| world.get_entity(p).is_ok() && is_pad(world, p))
}

/// `RefreshParts`: (`teleport_speed`, `teleport_cooldown`) in seconds, `power_efficiency`.
pub fn ratings(world: &World, pad: Entity) -> (f64, f64, f64) {
    let r = crate::stock_parts::ratings(world, pad);
    let servos = f64::from(r.sum(PartKind::Servo));
    let speed = (BASE_SPEED_DS - servos * 10.0) / 10.0;
    let cooldown = (BASE_COOLDOWN_DS - servos * 100.0) / 10.0;
    (
        speed.max(0.0),
        cooldown.max(0.0),
        f64::from(r.sum(PartKind::Capacitor)),
    )
}

fn attack_hand(world: &mut World, client: Entity, mob: Entity, pad: Entity) -> bool {
    if !is_pad(world, pad) {
        return false;
    }
    // `can_interact`: no `INTERACT_MACHINE_OFFLINE`.
    let broken = world
        .get::<crate::obj_damage::Integrity>(pad)
        .is_some_and(|i| i.broken);
    if !unpowered(world, pad) && !broken {
        let target = state(world, pad).linked;
        interact(world, client, mob, pad, target);
    }
    true
}

/// `interact(user, target_pad)`: starts charging a teleport to `target`.
pub fn interact(
    world: &mut World,
    client: Entity,
    _mob: Entity,
    pad: Entity,
    target: Option<Entity>,
) {
    let Some(target) = live_pad(world, target) else {
        tell(world, client, Span::Warning, "Target pad not found!");
        return;
    };
    let s = state(world, pad);
    let (speed, cooldown, _) = ratings(world, pad);
    let t = now(world);
    if let Some(last) = s.last_teleport
        && t < last + cooldown
    {
        let name = name_of(world, pad);
        let wait = crate::jobs::display_time_text(last + cooldown - t);
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} is recharging power. Please wait {wait}."),
        );
        return;
    }
    if s.charging.is_some() {
        let name = name_of(world, pad);
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} is charging up. Please wait."),
        );
        return;
    }
    if state(world, target).charging.is_some() {
        tell(
            world,
            client,
            Span::Warning,
            "Target pad is busy. Please wait.",
        );
        return;
    }
    if unpowered(world, target) {
        tell(
            world,
            client,
            Span::Warning,
            "Target pad is not responding to ping.",
        );
        return;
    }
    doteleport(world, pad, target, Some(client), speed);
}

/// `doteleport`: the charge-up.
fn doteleport(world: &mut World, pad: Entity, target: Entity, client: Option<Entity>, speed: f64) {
    if let Some(&pos) = world.get::<TilePos>(pad) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, FLASH, 25.0, true));
    }
    let at = now(world) + speed;
    let mut s = state(world, pad);
    s.charging = Some(Charge { target, at, client });
    world.entity_mut(pad).insert(s);
}

fn finish_teleports(world: &mut World) {
    let t = now(world);
    let due: Vec<(Entity, Charge)> = world
        .query::<(Entity, &QuantumPad)>()
        .iter(world)
        .filter_map(|(e, p)| p.charging.filter(|c| c.at <= t).map(|c| (e, c)))
        .collect();
    for (pad, charge) in due {
        teleport_contents(world, pad, charge);
    }
}

/// `teleport_contents`.
fn teleport_contents(world: &mut World, pad: Entity, charge: Charge) {
    let mut s = state(world, pad);
    s.charging = None;
    world.entity_mut(pad).insert(s);
    let say = |world: &mut World, text: String| {
        if let Some(client) = charge.client {
            tell(world, client, Span::Warning, text);
        }
    };
    if unpowered(world, pad) {
        let name = name_of(world, pad);
        say(world, format!("{name} is unpowered!"));
        return;
    }
    let target = live_pad(world, Some(charge.target));
    let Some(target) = target.filter(|&p| !unpowered(&*world, p)) else {
        say(
            world,
            "Linked pad is not responding to ping. Teleport aborted.".to_owned(),
        );
        return;
    };
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(pad), world.get::<TilePos>(target)) else {
        return;
    };
    s.last_teleport = Some(now(world));
    world.entity_mut(pad).insert(s);
    let (_, _, efficiency) = ratings(world, pad);
    crate::power::machine_use_energy(world, pad, ACTIVE_POWER / efficiency.max(1.0), false);
    for (e, at) in [(pad, from), (target, to)] {
        crate::effects::particles::start(
            world,
            crate::effects::particles::Kind::QuantumSparks,
            SPARKS,
            true,
            at,
        );
        crate::animation::flick(world, e, "qpad-beam");
        crate::sound::playsound(world, crate::sound::PlaySound::new(at, EMITTER, 25.0, true));
    }
    let on_pad: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(from).to_vec();
    for e in on_pad {
        if e == pad || world.get_entity(e).is_err() || !teleportable(world, e) {
            continue;
        }
        do_teleport(
            world,
            e,
            to,
            Tele {
                channel: Channel::Quantum,
                no_effects: true,
                ..default()
            },
        );
    }
}

fn anchored(world: &World, e: Entity) -> bool {
    world
        .get::<crate::obj_damage::Anchored>(e)
        .map(|a| a.0)
        .or_else(|| world.get::<DmObj>(e).map(|o| o.anchored))
        .or_else(|| world.get::<DmItem>(e).map(|i| i.anchored))
        .unwrap_or(false)
}

/// A movable on the pad that goes: an object or a mob (not the turf, nor a bare particle), not
/// anchored, a living mob not buckled to something anchored.
fn teleportable(world: &World, e: Entity) -> bool {
    let mob = world.get::<crate::health::Health>(e).is_some()
        || world.get::<crate::ghost::Ghost>(e).is_some();
    if world.get::<ss13_mapload::Turf>(e).is_some() || (!mob && path_of(world, e).is_none()) {
        return false;
    }
    if anchored(world, e) {
        return false;
    }
    !world
        .get::<crate::medical::surgery::Buckled>(e)
        .is_some_and(|b| anchored(world, b.0))
}

/// `multitool_act` and the keycard's `item_interaction` on a pad. True when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    pad: Entity,
) -> bool {
    if !is_pad(world, pad) {
        return false;
    }
    if is_keycard(world, held) {
        let card = world
            .get::<QuantumKeycard>(held)
            .copied()
            .unwrap_or_default();
        let (card_name, pad_name) = (name_of(world, held), name_of(world, pad));
        if let Some(linked) = live_pad(world, card.0) {
            tell(
                world,
                client,
                Span::Notice,
                format!("You insert {card_name} into {pad_name}'s card slot, activating it."),
            );
            interact(world, client, mob, pad, Some(linked));
            return true;
        }
        tell(
            world,
            client,
            Span::Notice,
            format!(
                "You insert {card_name} into {pad_name}'s card slot, initiating the link procedure."
            ),
        );
        do_after(
            world,
            Actor {
                client,
                mob,
                item: Some(held),
            },
            pad,
            CARD_DS,
            0,
            Job::QuantumPad(PadJob::LinkCard(held)),
        );
        return true;
    }
    let combat = world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0);
    if combat || tool_of(world, held).map(|t| t.0) != Some(ToolKind::Multitool) {
        return false;
    }
    if world.get::<PanelOpen>(pad).is_some_and(|p| p.0) {
        crate::tools::set_multitool_buffer(world, held, pad);
        let tool = name_of(world, held);
        tell(
            world,
            client,
            Span::Notice,
            format!(
                "You save the data in {tool} buffer. It can now be saved to pads with closed panels."
            ),
        );
        return true;
    }
    match crate::tools::multitool_buffer(world, held).filter(|&b| is_pad(world, b)) {
        Some(buffer) if buffer == pad => {
            tell(world, client, Span::Warning, "cannot link to self!");
        }
        Some(buffer) => {
            let mut s = state(world, pad);
            s.linked = Some(buffer);
            world.entity_mut(pad).insert(s);
            tell(world, client, Span::Notice, "data uploaded from buffer");
        }
        // tg returns `NONE` here and the multitool goes on to hit the pad; it only says so.
        None => tell(world, client, Span::Warning, "no quantum pad data found!"),
    }
    true
}

/// `click_alt` of a keycard in hand: the unlink button. True when it was a keycard.
pub fn click_alt(world: &mut World, client: Entity, mob: Entity, card: Entity) -> bool {
    if !is_keycard(world, card) {
        return false;
    }
    let name = name_of(world, card);
    tell(
        world,
        client,
        Span::Notice,
        format!("You start pressing {name}'s unlink button..."),
    );
    // `do_after` keeps whatever the active hand held at the start (the card may be in the other).
    let item = world
        .get::<crate::inventory::Inventory>(mob)
        .and_then(crate::inventory::Inventory::active_item);
    do_after(
        world,
        Actor { client, mob, item },
        mob,
        CARD_DS,
        0,
        Job::QuantumPad(PadJob::UnlinkCard(card)),
    );
    true
}

/// The end of a keycard's timed action (the card still in the active hand: `do_after`).
pub fn complete(world: &mut World, client: Entity, target: Entity, job: PadJob) {
    match job {
        PadJob::LinkCard(card) => {
            if !is_pad(world, target) || world.get_entity(card).is_err() {
                return;
            }
            let (card_name, pad_name) = (name_of(world, card), name_of(world, target));
            tell(
                world,
                client,
                Span::Notice,
                format!("You complete the link between {card_name} and {pad_name}."),
            );
            world.entity_mut(card).insert(QuantumKeycard(Some(target)));
        }
        PadJob::UnlinkCard(card) => {
            if world.get_entity(card).is_err() {
                return;
            }
            tell(
                world,
                client,
                Span::Notice,
                "The keycard beeps twice and disconnects the quantum link.",
            );
            world.entity_mut(card).insert(QuantumKeycard(None));
        }
    }
}

/// The pad's link and panel, the keycard's link and its pad's area.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<Line>) {
    if is_pad(world, target) {
        let linked = live_pad(world, state(world, target).linked).is_some();
        lines.push((
            Span::Notice,
            format!(
                "It is {} linked to another pad.",
                if linked { "currently" } else { "not" }
            ),
        ));
        lines.push((
            Span::Notice,
            if world.get::<PanelOpen>(target).is_some_and(|p| p.0) {
                "The linking device is now able to be scanned with a multitool."
            } else {
                "The panel is screwed in, obstructing the linking device."
            }
            .to_owned(),
        ));
        return;
    }
    if !is_keycard(world, target) {
        return;
    }
    let pad = live_pad(world, world.get::<QuantumKeycard>(target).and_then(|c| c.0));
    let Some(pad) = pad else {
        let name = name_of(world, target);
        lines.push((
            Span::Notice,
            format!("Insert {name} into an active quantum pad to link it."),
        ));
        return;
    };
    lines.push((
        Span::Info,
        "It's currently linked to a quantum pad.".to_owned(),
    ));
    let area = world
        .get::<TilePos>(pad)
        .and_then(|&p| world.get_resource::<ss13_mapload::Areas>()?.area_at(p));
    if let Some(area) = area {
        let name = crate::shuttle::area_name(world, area);
        lines.push((Span::Notice, format!("The pad is located in the {name}")));
    }
    lines.push((Span::Notice, "Alt-click to unlink the keycard.".to_owned()));
}
