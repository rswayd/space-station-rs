//! Brain processors and cyborg construction (server), after tg `/obj/item/brain_processor`
//! (`code/modules/mob/living/brain/processor/`: `_processor.dm`, `organic.dm` — the MMI,
//! `positronic.dm`), `/obj/item/robot_suit` (`code/game/objects/items/robot/robot_parts.dm`) and
//! the robot chest and head (`robot_bodyparts.dm`).
//!
//! - MMI: a brain goes in from a hand (the MMI on the floor or in the other hand); its brainmob
//!   (`crate::medical::brainmob`) comes along and is alive in there, hears, speaks and talks on
//!   the MMI's radio (`;`, tg `/obj/item/radio/brain_processor`: listening, not relaying what
//!   it hears). A ghosted mind comes back into it (tg `notify_revival`). The brain does not
//!   decay inside (`ORGAN_FROZEN`). Used in hand it gives the brain back (its brainmob dead
//!   again, tg `remove_brain`), or, empty, switches its radio.
//! - Positronic brain: used in hand it asks ghosts for a personality for 60 s (every ghost reads
//!   it); a ghost clicking an unoccupied one becomes its brainmob ("[name]-[id]" from
//!   `strings/names/posibrain.txt`), once per player. The display one never takes a ghost.
//! - Endoskeleton: robot arms, legs, a wired chest with a cell (cable, cell) and a head with two
//!   working flashes go on; iron on a bare one is the ED-209 (`crate::bots::construction`), on a
//!   built one nothing. A brain processor with a live, present mind on a complete endoskeleton
//!   lying on the floor builds a cyborg there ("Default Cyborg-NNN", "Default Android-NNN" from a
//!   posibrain) with the chest's cell: the mind goes into it, the processor and the
//!   endoskeleton go inside (tg `mmi`, `robot_suit`), and it starts as the Cyborg job's does
//!   (`crate::silicon::cyborg::activate`).
// shortcut: no AI core construction, AI shells (B.O.R.I.S.), multitool debug window
// (`CyborgBootDebug`: name, locomotion, panel, forced AI, lawsync), wrench disassembly,
// screwdriver cell swap, crowbar flash removal or wirecutters; no syndicate MMI laws, MMI
// EMPs, sphere posibrains, personality seeds, posibrain bans or the ghost's confirmation
// alert; a cyborg never gives its MMI back (no deconstruction, gibbing or debris); a brain is
// put into an MMI without asking when it is inactive; messages are chat lines (tg balloon
// alerts), no sounds.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_chat::speech::EarsWorn;
use ss13_mapload::{DmItem, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};

use super::chassis::{CELL, Chassis};
use super::cyborg::Cyborg;
use crate::ghost::Ghost;
use crate::health::Stat;
use crate::inventory::{CarriedBy, Inventory};
use crate::mecha::core::ItemCell;
use crate::medical::brainmob::{self, BrainMob, Mind, Occupant};
use crate::medical::organs::{Organ, OrganFrozen, OrganOf, Slot};
use crate::multiz::GhostClick;
use crate::obj_damage::is_under;
use crate::radio::{BuiltInRadio, Radio, RadioKind, SiliconRadio};
use crate::server::{PlayerMob, Players};
use crate::storage::{StoredIn, put_inside};
use crate::tools::tell;

pub const PROCESSOR: &str = "/obj/item/brain_processor";
pub const MMI: &str = "/obj/item/brain_processor/organic";
pub const POSIBRAIN: &str = "/obj/item/brain_processor/positronic";
const POSIBRAIN_DISPLAY: &str = "/obj/item/brain_processor/positronic/display";
pub const ROBOT_SUIT: &str = "/obj/item/robot_suit";
const IRON: &str = "/obj/item/stack/sheet/iron";
const CABLE: &str = "/obj/item/stack/cable_coil";
const FLASH: &str = "/obj/item/assembly/flash/handheld";
/// `robot_suit` `l_arm`, `r_arm`, `l_leg`, `r_leg`, `chest`, `head` and their `icon_state`.
const PARTS: [(&str, &str); 6] = [
    ("/obj/item/bodypart/arm/left/robot", "borg_l_arm"),
    ("/obj/item/bodypart/arm/right/robot", "borg_r_arm"),
    ("/obj/item/bodypart/leg/left/robot", "borg_l_leg"),
    ("/obj/item/bodypart/leg/right/robot", "borg_r_leg"),
    ("/obj/item/bodypart/chest/robot", "borg_chest"),
    ("/obj/item/bodypart/head/robot", "borg_head"),
];
const CHEST: usize = 4;
const HEAD: usize = 5;
/// `update_overlays` order: l_arm, r_arm, chest, l_leg, r_leg, head.
const OVERLAY_ORDER: [usize; 6] = [0, 1, CHEST, 2, 3, HEAD];
const ASSEMBLIES_DMI: &str = "icons/obj/devices/assemblies.dmi";
const AUGMENTS_DMI: &str = "icons/mob/augmentation/augments.dmi";
/// `POSIBRAIN_NAG_COOLDOWN`: how long a posibrain asks for a ghost.
pub const POSIBRAIN_SEARCH_SECS: f64 = 60.0;
/// `strings/names/posibrain.txt`.
const POSIBRAIN_NAMES: [&str; 63] = [
    "ACE", "ANCL", "ANTR", "ARC", "ARMA", "BGB", "BLOX", "CBOS", "CDB", "CHOC", "CHRI", "COI",
    "CYBR", "DOM", "DRSD", "DUNC", "FIK", "FIRC", "FRE", "FXMC", "GOOF", "GUN", "HBL", "HG", "HIU",
    "HOG", "INC", "JACQ", "JJR", "JLLO", "JLS", "JNLG", "JRD", "KOR", "KOS", "MEL", "MET", "MIW",
    "MOTH", "MRPR", "MSO", "NOVA", "ORNG", "OSI", "PBU", "PHL", "PKP", "RR", "RZH", "SANS", "SCR",
    "SINA", "SLI", "STLP", "TAT", "TIM", "TKRG", "WJ", "WLRD", "XAL", "XIS", "XSI", "ZEPH",
];

/// A brain processor's state (tg `/obj/item/brain_processor` vars); its brainmob is its
/// [`Occupant`]. Made on first use.
#[derive(Component, Clone, Debug, Default)]
pub struct BrainProcessor {
    /// The MMI's brain (inside it).
    pub brain: Option<Entity>,
    /// The built-in radio (a child; heard by the brainmob).
    pub radio: Option<Entity>,
    /// A posibrain asks ghosts until then (tg `ghost_ping_timer`).
    pub searching_until: Option<f64>,
    /// Clients that entered this posibrain once (tg `ckeys_entered`).
    pub entered: Vec<Entity>,
    /// A posibrain's brainmob name.
    pub name: String,
}

/// An endoskeleton's parts, in [`PARTS`] order (inside it).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct RobotSuit {
    pub parts: [Option<Entity>; 6],
}

/// tg `/obj/item/bodypart/chest/robot` `wired`, `cell` (inside it).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct RobotChest {
    pub wired: bool,
    pub cell: Option<Entity>,
}

/// tg `/obj/item/bodypart/head/robot` `flash1`, `flash2` (inside it).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct RobotHead {
    pub flashes: Vec<Entity>,
}

/// On a cyborg built around a brain processor: the processor inside it (tg `mmi`).
#[derive(Component, Clone, Copy, Debug)]
pub struct BorgBrain(pub Entity);

pub struct MmiPlugin;

impl Plugin for MmiPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // MMI: the brain out or the radio switched; posibrain: the ghost request.
        app.add_click_handler(ClickPhase::AttackSelf, 150, |w, c| {
            attack_self(w, c.client, c.mob, c.target)
        });
        // Before the bot assemblies: iron on a built endoskeleton is not an ED-209.
        app.add_click_handler(ClickPhase::OtherHand, 150, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 1450, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
        app.add_systems(
            Update,
            (
                ghost_clicks.after(crate::click::ResolveClicks),
                stop_searching.run_if(any_with_component::<BrainProcessor>),
            ),
        );
    }
}

fn path(world: &World, e: Entity) -> String {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

fn now(world: &World) -> f64 {
    world
        .get_resource::<Time>()
        .map_or(0.0, |t| t.elapsed_secs_f64())
}

fn name_of(world: &World, e: Entity) -> String {
    crate::security::name_of(world, e)
}

/// A loose brain (not in a body).
fn is_brain(world: &World, e: Entity) -> bool {
    world.get::<Organ>(e).is_some_and(|o| o.slot == Slot::Brain)
        && world.get::<OrganOf>(e).is_none()
}

/// The processor's state, made with its radio on first use (tg `Initialize`). `None` for
/// anything else.
pub fn processor(world: &mut World, e: Entity) -> Option<BrainProcessor> {
    let path = path(world, e);
    if !is_under(&path, PROCESSOR) {
        return None;
    }
    if let Some(state) = world.get::<BrainProcessor>(e) {
        return Some(state.clone());
    }
    // `radio.set_broadcasting(FALSE)`: it does not relay what it hears; talking into it works.
    let radio = world
        .spawn((
            Radio {
                kind: RadioKind::Other,
                on: true,
                frequency: ss13_chat::radio::FREQ_COMMON,
                broadcasting: false,
                listening: true,
                canhear_range: 0,
                special_channels: 0,
                channels: Vec::new(),
                tx_cut: false,
            },
            BuiltInRadio { listener: e },
            Name::new("radio"),
            ChildOf(e),
        ))
        .id();
    let name = if is_under(&path, POSIBRAIN) {
        let (pick, id) = match world.get_resource_mut::<crate::combat::CombatRng>() {
            Some(mut rng) => (
                rng.range(0, POSIBRAIN_NAMES.len() as u32 - 1) as usize,
                rng.range(1, 999),
            ),
            None => (0, 1),
        };
        format!("{}-{id}", POSIBRAIN_NAMES[pick])
    } else {
        String::new()
    };
    let state = BrainProcessor {
        radio: Some(radio),
        name,
        ..default()
    };
    world.entity_mut(e).insert(state.clone());
    Some(state)
}

fn store(world: &mut World, e: Entity, state: BrainProcessor) {
    world.entity_mut(e).insert(state);
}

/// tg `set_brainmob`: the radio follows the processor's brainmob.
pub fn set_brainmob(world: &mut World, processor: Entity, new: Option<Entity>) {
    let radio = world.get::<BrainProcessor>(processor).and_then(|p| p.radio);
    if let Some(Occupant(old)) = world.get::<Occupant>(processor).copied() {
        if Some(old) != new
            && let Ok(mut e) = world.get_entity_mut(old)
        {
            e.remove::<(SiliconRadio, EarsWorn)>();
        }
        world.entity_mut(processor).remove::<Occupant>();
    }
    if let Some(radio) = radio {
        world.entity_mut(radio).insert(BuiltInRadio {
            listener: new.unwrap_or(processor),
        });
    }
    let Some(mob) = new else {
        return;
    };
    world.entity_mut(processor).insert(Occupant(mob));
    if let Some(mut b) = world.get_mut::<BrainMob>(mob) {
        b.container = processor;
    }
    if let Some(radio) = radio {
        world
            .entity_mut(mob)
            .insert((SiliconRadio(radio), EarsWorn));
    }
}

fn set_stat(world: &mut World, mob: Entity, stat: Stat) {
    if let Some(mut s) = world.get_mut::<Stat>(mob) {
        s.set_if_neq(stat);
    }
}

/// tg `insert_brain`: `brain` (from anywhere) into the MMI; its brainmob lives again unless the
/// brain is failing, and a ghosted mind comes back.
pub fn insert_brain(world: &mut World, mmi: Entity, brain: Entity) {
    let Some(mut state) = processor(world, mmi) else {
        return;
    };
    if let Some(old) = state.brain {
        let at = crate::explosions::turf_of(world, mmi);
        remove_brain(world, mmi);
        if let Some(at) = at {
            crate::storage::take_out(world, old, at);
        }
        state = processor(world, mmi).expect("a processor");
    }
    put_inside(world, brain, mmi);
    world.entity_mut(brain).insert(OrganFrozen);
    state.brain = Some(brain);
    store(world, mmi, state);
    if let Some(Occupant(mob)) = world.get::<Occupant>(brain).copied() {
        world.entity_mut(brain).remove::<Occupant>();
        set_brainmob(world, mmi, Some(mob));
        let failing = world.get::<Organ>(brain).is_some_and(|o| o.failing);
        if !failing {
            set_stat(world, mob, Stat::Conscious);
        }
        crate::ghost::return_to_body(world, mob);
    }
    refresh_look(world, mmi);
}

/// tg `remove_brain`: the brain leaves the MMI (nowhere yet: the caller places it) with its
/// brainmob, dead again.
pub fn remove_brain(world: &mut World, mmi: Entity) -> Option<Entity> {
    let mut state = world.get::<BrainProcessor>(mmi)?.clone();
    let brain = state.brain.take()?;
    if let Some(Occupant(mob)) = world.get::<Occupant>(mmi).copied() {
        set_brainmob(world, mmi, None);
        if let Some(mut b) = world.get_mut::<BrainMob>(mob) {
            b.container = brain;
        }
        world.entity_mut(brain).insert(Occupant(mob));
        set_stat(world, mob, Stat::Dead);
    }
    world.entity_mut(brain).remove::<(OrganFrozen, StoredIn)>();
    store(world, mmi, state);
    refresh_look(world, mmi);
    Some(brain)
}

/// A posibrain has a player, sits in a cyborg, or is a display one (tg `is_occupied`).
fn occupied(world: &World, processor: Entity) -> bool {
    is_under(&path(world, processor), POSIBRAIN_DISPLAY)
        || world
            .get::<Occupant>(processor)
            .is_some_and(|o| world.get::<PlayerMob>(o.0).is_some())
        || world
            .get::<StoredIn>(processor)
            .is_some_and(|s| world.get::<Cyborg>(s.0).is_some())
}

fn searching(world: &World, processor: Entity) -> bool {
    world
        .get::<BrainProcessor>(processor)
        .is_some_and(|p| p.searching_until.is_some())
}

/// `update_icon_state`, `update_overlays` of the processors.
fn refresh_look(world: &mut World, e: Entity) {
    let path = path(world, e);
    let (state, overlay) = if is_under(&path, POSIBRAIN) {
        let suffix = if searching(world, e) {
            "-searching"
        } else if occupied(world, e) {
            "-occupied"
        } else {
            ""
        };
        (format!("posibrain{suffix}"), None)
    } else {
        let brain = world.get::<BrainProcessor>(e).and_then(|p| p.brain);
        match brain {
            None => ("mmi_off".to_owned(), None),
            Some(_) => {
                // tg compares `stat <= DEAD`, always true: a dead brainmob is meant to flatline.
                let alive = world
                    .get::<Occupant>(e)
                    .and_then(|o| world.get::<Stat>(o.0))
                    .is_some_and(|s| *s != Stat::Dead);
                let overlay = if alive { "mmi_alive" } else { "mmi_dead" };
                ("mmi_brain".to_owned(), Some(overlay))
            }
        }
    };
    let Some(icons) = world.get_resource::<IconIndex>() else {
        return;
    };
    let base = icons.get(ASSEMBLIES_DMI, &state);
    let ours: Vec<_> = ["mmi_alive", "mmi_dead"]
        .iter()
        .filter_map(|s| icons.get(ASSEMBLIES_DMI, s))
        .collect();
    let overlay = overlay.and_then(|s| icons.get(ASSEMBLIES_DMI, s));
    let Some(mut look) = world.get_mut::<Appearance>(e) else {
        return;
    };
    if let Some(base) = base
        && look.icon_state != base
    {
        look.icon_state = base;
    }
    let mut overlays = look.overlays.clone();
    overlays.retain(|o| !ours.contains(&o.icon_state));
    overlays.extend(overlay.map(Overlay::new));
    if look.overlays != overlays {
        look.overlays = overlays;
    }
}

/// A held `tool` used on `target` (on the floor or in the other hand): a brain into an MMI,
/// parts onto an endoskeleton, a cell or cable into a robot chest, flashes into a robot head,
/// a processor into a finished endoskeleton. True when it was one of these (or refused as one).
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    tool: Entity,
    target: Entity,
) -> bool {
    let (tool_path, target_path) = (path(world, tool), path(world, target));
    if target_path.is_empty() || tool == target {
        return false;
    }
    if is_under(&target_path, MMI) && is_brain(world, tool) {
        brain_into_mmi(world, client, mob, tool, target);
        return true;
    }
    if is_under(&target_path, ROBOT_SUIT) {
        return suit_interaction(world, client, tool, &tool_path, target);
    }
    if is_under(&target_path, PARTS[CHEST].0) {
        return chest_interaction(world, client, tool, &tool_path, target);
    }
    if is_under(&target_path, PARTS[HEAD].0) && is_under(&tool_path, FLASH) {
        head_interaction(world, client, tool, target);
        return true;
    }
    false
}

/// tg `/obj/item/brain_processor/organic/item_interaction`.
fn brain_into_mmi(world: &mut World, client: Entity, mob: Entity, brain: Entity, mmi: Entity) {
    let Some(state) = processor(world, mmi) else {
        return;
    };
    if state.brain.is_some() {
        tell(world, client, Span::Warning, "Он уже заполнен!");
        return;
    }
    let (brain_name, mmi_name) = (name_of(world, brain), name_of(world, mmi));
    let user = name_of(world, mob);
    let failing = world.get::<Organ>(brain).is_some_and(|o| o.failing);
    insert_brain(world, mmi, brain);
    crate::medical::visible(
        world,
        mob,
        &format!("Вы вставляете {brain_name} в {mmi_name}..."),
        &format!("{user} вставляет {brain_name} в {mmi_name}..."),
        Span::Notice,
    );
    if failing {
        tell(
            world,
            client,
            Span::Warning,
            format!("Индикатор {mmi_name} загорается жёлтым, тихо пищит сигнал целостности мозга."),
        );
    } else {
        tell(
            world,
            client,
            Span::Notice,
            format!("Индикатор {mmi_name} загорается зелёным!"),
        );
    }
    info!("{mob} put {brain} into the MMI {mmi}");
}

/// tg `attack_self` of the processors. True when `item` is one.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(mut state) = processor(world, item) else {
        return false;
    };
    let name = name_of(world, item);
    if is_under(&path(world, item), POSIBRAIN) {
        if occupied(world, item) {
            tell(world, client, Span::Warning, "уже активен!");
        } else if state.searching_until.is_some() {
            tell(world, client, Span::Warning, "уже ищет!");
        } else {
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы нажимаете кнопку ручной активации и запускаете загрузку {name}."),
            );
            state.searching_until = Some(now(world) + POSIBRAIN_SEARCH_SECS);
            let label = state.name.clone();
            store(world, item, state);
            refresh_look(world, item);
            ping_ghosts(world, &label);
        }
        return true;
    }
    if state.brain.is_some() {
        let Some(brain) = remove_brain(world, item) else {
            return true;
        };
        if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
            crate::storage::put_in_hands(world, mob, &inventory, brain);
        }
        let caught = world.get::<CarriedBy>(brain).is_some();
        let fate = if caught {
            format!("аккуратно забирая {} в руку", name_of(world, brain))
        } else {
            format!("и {} падает на пол", name_of(world, brain))
        };
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы открываете и переворачиваете {name}, {fate}."),
        );
        return true;
    }
    if let Some(radio) = state.radio
        && let Some(mut r) = world.get_mut::<Radio>(radio)
    {
        r.on = !r.on;
        let on = r.on;
        tell(
            world,
            client,
            Span::Notice,
            format!(
                "Вы переключаете радиосистему {name} в положение {}.",
                if on { "«вкл»" } else { "«выкл»" }
            ),
        );
    }
    true
}

/// tg `ping_ghosts`: every ghost reads that the posibrain wants a personality.
fn ping_ghosts(world: &mut World, name: &str) {
    let clients: Vec<Entity> = world
        .query_filtered::<&PlayerMob, With<Ghost>>()
        .iter(world)
        .map(|p| p.client)
        .collect();
    for client in clients {
        tell(
            world,
            client,
            Span::BoldNotice,
            format!(
                "Позитронный мозг {name} запрашивает личность! Нажмите на него, чтобы вселиться."
            ),
        );
    }
}

/// tg `stop_requesting_ghost` when the timer runs out.
// Cost: one pass over the processors (a handful) per tick.
fn stop_searching(world: &mut World) {
    let t = now(world);
    let expired: Vec<Entity> = world
        .query::<(Entity, &BrainProcessor)>()
        .iter(world)
        .filter(|(_, p)| p.searching_until.is_some_and(|until| t >= until))
        .map(|(e, _)| e)
        .collect();
    for e in expired {
        if let Some(mut state) = world.get_mut::<BrainProcessor>(e) {
            state.searching_until = None;
        }
        refresh_look(world, e);
        if !occupied(world, e) {
            let name = name_of(world, e);
            around(
                world,
                e,
                Span::Notice,
                &format!("Огоньки {name} медленно гаснут, он засыпает..."),
            );
        }
    }
}

/// A message to the players around `e` (tg `visible_message` of an object).
fn around(world: &mut World, e: Entity, span: Span, text: &str) {
    let Some(pos) = crate::explosions::turf_of(world, e) else {
        return;
    };
    let clients = crate::obj_damage::clients_in_range(world, pos, 7, &[]);
    for client in clients {
        tell(world, client, span, text);
    }
}

fn ghost_clicks(world: &mut World, mut clicks: Local<MessageCursor<GhostClick>>) {
    let Some(messages) = world.get_resource::<Messages<GhostClick>>() else {
        return;
    };
    let pending: Vec<GhostClick> = clicks.read(messages).copied().collect();
    for click in pending {
        if is_under(&path(world, click.target), POSIBRAIN) {
            ghost_enters(world, click.client, click.ghost, click.target);
        }
    }
}

/// tg posibrain `attack_ghost` -> `insert_ghost`: the ghost becomes the brainmob (leaving its
/// body for good).
pub fn ghost_enters(world: &mut World, client: Entity, ghost: Entity, posibrain: Entity) {
    // Checked at the moment of effect: still this client's ghost.
    if world.get::<Ghost>(ghost).is_none()
        || world.get_resource::<Players>().and_then(|p| p.mob(client)) != Some(ghost)
    {
        return;
    }
    let Some(mut state) = processor(world, posibrain) else {
        return;
    };
    if occupied(world, posibrain) {
        return;
    }
    let name = name_of(world, posibrain);
    if state.entered.contains(&client) {
        tell(
            world,
            client,
            Span::Warning,
            format!("Вы не можете войти в {name} второй раз!"),
        );
        return;
    }
    let Some(pos) = crate::explosions::turf_of(world, posibrain) else {
        return;
    };
    let mob = match world.get::<Occupant>(posibrain).copied() {
        Some(Occupant(mob)) => mob,
        None => brainmob::spawn_brainmob(world, posibrain, pos, &state.name, Stat::Conscious),
    };
    set_stat(world, mob, Stat::Conscious);
    set_brainmob(world, posibrain, Some(mob));
    if world.get::<TilePos>(mob) != Some(&pos) {
        world.entity_mut(mob).insert(pos);
    }
    // The ghost goes (`PossessByPlayer`).
    super::attach_client(world, client, mob, pos);
    state.entered.push(client);
    state.searching_until = None;
    store(world, posibrain, state);
    refresh_look(world, posibrain);
    around(
        world,
        posibrain,
        Span::Notice,
        &format!("{name} пищит, его огоньки начинают мигать!"),
    );
    info!("client {client} entered the posibrain {posibrain}");
}

/// tg robot chest `item_interaction`: a cell goes in, a cable wires it.
fn chest_interaction(
    world: &mut World,
    client: Entity,
    tool: Entity,
    tool_path: &str,
    chest: Entity,
) -> bool {
    let mut state = world.get::<RobotChest>(chest).cloned().unwrap_or_default();
    let chest_name = name_of(world, chest);
    if is_under(tool_path, CELL) {
        if state.cell.is_some() {
            tell(
                world,
                client,
                Span::Warning,
                format!("В {chest_name} уже есть батарея!"),
            );
            return true;
        }
        let cell_name = name_of(world, tool);
        put_inside(world, tool, chest);
        state.cell = Some(tool);
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы вставляете {cell_name} в {chest_name}."),
        );
    } else if is_under(tool_path, CABLE) {
        if state.wired {
            tell(
                world,
                client,
                Span::Warning,
                format!("{chest_name}: проводка уже подключена!"),
            );
            return true;
        }
        if !crate::tools::use_stack(world, tool, 1) {
            tell(
                world,
                client,
                Span::Warning,
                "Нужен один отрезок кабеля, чтобы подключить проводку!",
            );
            return true;
        }
        state.wired = true;
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы подключаете батарею внутри {chest_name}."),
        );
    } else {
        return false;
    }
    world.entity_mut(chest).insert(state);
    true
}

/// tg robot head `item_interaction` with a handheld flash.
fn head_interaction(world: &mut World, client: Entity, flash: Entity, head: Entity) {
    let mut state = world.get::<RobotHead>(head).cloned().unwrap_or_default();
    let head_name = name_of(world, head);
    if state.flashes.len() >= 2 {
        tell(
            world,
            client,
            Span::Warning,
            format!("В {head_name} уже есть обе вспышки-глаза!"),
        );
        return;
    }
    if world
        .get::<crate::security::Flash>(flash)
        .is_some_and(|f| f.burnt_out)
    {
        tell(
            world,
            client,
            Span::Warning,
            "Нельзя использовать сломанную вспышку!",
        );
        return;
    }
    put_inside(world, flash, head);
    state.flashes.push(flash);
    world.entity_mut(head).insert(state);
    tell(
        world,
        client,
        Span::Notice,
        "Вы вставляете вспышку в глазницу.",
    );
}

/// tg `check_completion`.
pub fn complete(world: &World, suit: &RobotSuit) -> bool {
    let chest = suit.parts[CHEST].and_then(|c| world.get::<RobotChest>(c));
    let head = suit.parts[HEAD].and_then(|h| world.get::<RobotHead>(h));
    suit.parts.iter().all(Option::is_some)
        && chest.is_some_and(|c| c.wired && c.cell.is_some())
        && head.is_some_and(|h| h.flashes.len() >= 2)
}

/// tg `robot_suit/item_interaction`.
fn suit_interaction(
    world: &mut World,
    client: Entity,
    tool: Entity,
    tool_path: &str,
    suit: Entity,
) -> bool {
    let mut state = world.get::<RobotSuit>(suit).cloned().unwrap_or_default();
    // Iron starts an ED-209 from a bare endoskeleton (`crate::bots::construction`).
    if is_under(tool_path, IRON) {
        return state.parts.iter().any(Option::is_some);
    }
    if let Some(i) = PARTS.iter().position(|(p, _)| is_under(tool_path, p)) {
        if state.parts[i].is_some() {
            return true;
        }
        if i == CHEST {
            let chest = world.get::<RobotChest>(tool).cloned().unwrap_or_default();
            if !chest.wired {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Сначала к нему нужно подключить проводку!",
                );
                return true;
            }
            if chest.cell.is_none() {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Сначала в него нужно вставить батарею!",
                );
                return true;
            }
        }
        if i == HEAD
            && world
                .get::<RobotHead>(tool)
                .is_none_or(|h| h.flashes.len() < 2)
        {
            tell(
                world,
                client,
                Span::Warning,
                "Сначала в него нужно вставить вспышки!",
            );
            return true;
        }
        put_inside(world, tool, suit);
        state.parts[i] = Some(tool);
        world.entity_mut(suit).insert(state.clone());
        refresh_suit_look(world, suit, &state);
        return true;
    }
    if is_under(tool_path, PROCESSOR) {
        build_cyborg(world, client, tool, suit, &state);
        return true;
    }
    false
}

/// `robot_suit/update_overlays`: a `[part]+o` overlay per attached part.
fn refresh_suit_look(world: &mut World, suit: Entity, state: &RobotSuit) {
    let Some(icons) = world.get_resource::<IconIndex>() else {
        return;
    };
    let ids: Vec<_> = OVERLAY_ORDER
        .iter()
        .map(|&i| (i, icons.get(AUGMENTS_DMI, &format!("{}+o", PARTS[i].1))))
        .collect();
    let Some(mut look) = world.get_mut::<Appearance>(suit) else {
        return;
    };
    let mut overlays = look.overlays.clone();
    overlays.retain(|o| !ids.iter().any(|(_, id)| *id == Some(o.icon_state)));
    overlays.extend(
        ids.iter()
            .filter(|(i, _)| state.parts[*i].is_some())
            .filter_map(|(_, id)| id.map(Overlay::new)),
    );
    if look.overlays != overlays {
        look.overlays = overlays;
    }
}

/// tg `brain_check`: the processor's brainmob and its client, when the mind is there, alive and
/// (for an MMI) the brain whole; else why not, to `client`.
fn brain_check(world: &mut World, client: Entity, processor: Entity) -> Option<(Entity, Entity)> {
    let name = name_of(world, processor);
    let refuse = |world: &mut World, why: &str| {
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} сообщает, что {why}"),
        );
        None
    };
    let Some(Occupant(mob)) = world.get::<Occupant>(processor).copied() else {
        return refuse(world, "разум отсутствует!");
    };
    let player = match brainmob::mind_of(world, mob) {
        None => return refuse(world, "разум совершенно не отвечает!"),
        Some(Mind::Ghost(_)) => return refuse(world, "разум сейчас неактивен."),
        Some(Mind::Client(player)) => player,
    };
    if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return refuse(world, "мозг мёртв!");
    }
    let brain = world.get::<BrainProcessor>(processor).and_then(|p| p.brain);
    if brain.is_some_and(|b| world.get::<Organ>(b).is_some_and(|o| o.failing)) {
        return refuse(world, "мозг повреждён!");
    }
    Some((mob, player))
}

/// tg `robot_suit/item_interaction` with a brain processor: the cyborg.
fn build_cyborg(
    world: &mut World,
    client: Entity,
    processor: Entity,
    suit: Entity,
    state: &RobotSuit,
) {
    if world.get::<BrainProcessor>(processor).is_none()
        && self::processor(world, processor).is_none()
    {
        return;
    }
    if !complete(world, state) {
        tell(
            world,
            client,
            Span::Warning,
            "MMI вставляется после всего остального!",
        );
        return;
    }
    let Some(&pos) = world.get::<TilePos>(suit) else {
        let name = name_of(world, processor);
        tell(
            world,
            client,
            Span::Warning,
            format!(
                "Не получается вставить {}: каркас должен стоять на полу, чтобы всё встало точно!",
                name
            ),
        );
        return;
    };
    let Some((mob, player)) = brain_check(world, client, processor) else {
        return;
    };
    let braintype = if is_under(&path(world, processor), POSIBRAIN) {
        "Android"
    } else {
        "Cyborg"
    };
    let Some(borg) = super::cyborg::spawn_borg(world, pos, braintype) else {
        warn!(
            "no {} prototype: no cyborg from {processor}",
            ss13_mapload::silicon::ROBOT_TYPE
        );
        return;
    };
    // tg `robot/nocell`: the chest's cell powers it.
    let chest = state.parts[CHEST].expect("checked by complete");
    if let Some(cell) = world.get::<RobotChest>(chest).and_then(|c| c.cell) {
        install_cell(world, borg, cell);
        if let Some(mut c) = world.get_mut::<RobotChest>(chest) {
            c.cell = None;
        }
    }
    // The brainmob's mind goes into the cyborg; the brainmob is gone with it (no client left).
    set_brainmob(world, processor, None);
    put_inside(world, processor, borg);
    put_inside(world, suit, borg);
    world.entity_mut(borg).insert(BorgBrain(processor));
    if let Some(mut b) = world.get_mut::<BrainMob>(mob) {
        b.container = processor;
    }
    super::cyborg::activate(world, borg, player, pos);
    refresh_look(world, processor);
    info!("{client} built the cyborg {borg} around {processor}");
}

/// The cyborg's stock cell gives way to `cell`, whose charge it now runs on.
fn install_cell(world: &mut World, borg: Entity, cell: Entity) {
    if let Some(old) = world.get::<Chassis>(borg).and_then(|c| c.cell) {
        world.try_despawn(old).ok();
    }
    let charge = world.get::<ItemCell>(cell).map(|c| c.0);
    put_inside(world, cell, borg);
    if let Some(mut chassis) = world.get_mut::<Chassis>(borg) {
        chassis.cell = Some(cell);
    }
    if let Some(mut cyborg) = world.get_mut::<Cyborg>(borg) {
        cyborg.charge = charge.map_or(0.0, |c| c.charge);
        cyborg.max_charge = charge.map_or(0.0, |c| c.maxcharge);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::medical::organs::spawn_organ;
    use crate::server::MoveQueue;

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<Players>();
        world
    }

    fn item(world: &mut World, path: &str, pos: TilePos) -> Entity {
        world
            .spawn((
                DmItem {
                    path: path.into(),
                    ..default()
                },
                pos,
            ))
            .id()
    }

    /// A loose brain with a brainmob controlled by a new client.
    fn brain_with_player(world: &mut World, pos: TilePos) -> (Entity, Entity, Entity) {
        let brain = spawn_organ(world, "/obj/item/organ/brain").unwrap();
        world.entity_mut(brain).insert(pos);
        let mob = brainmob::spawn_brainmob(world, brain, pos, "Иван Иванов", Stat::Dead);
        let client = world.spawn_empty().id();
        world
            .entity_mut(mob)
            .insert((PlayerMob { client }, MoveQueue::default()));
        world.resource_mut::<Players>().set_mob(client, mob);
        (brain, mob, client)
    }

    #[test]
    fn a_brain_in_an_mmi_lives_and_talks_on_its_radio() {
        let mut world = world();
        let pos = TilePos::new(3, 3, 1);
        let mmi = item(&mut world, MMI, pos);
        let (brain, mob, _) = brain_with_player(&mut world, pos);
        insert_brain(&mut world, mmi, brain);
        let state = world.get::<BrainProcessor>(mmi).unwrap().clone();
        assert_eq!(state.brain, Some(brain));
        assert_eq!(world.get::<StoredIn>(brain).map(|s| s.0), Some(mmi));
        assert!(world.get::<OrganFrozen>(brain).is_some());
        assert_eq!(world.get::<Occupant>(mmi), Some(&Occupant(mob)));
        assert!(world.get::<Occupant>(brain).is_none());
        assert_eq!(world.get::<Stat>(mob), Some(&Stat::Conscious));
        let radio = state.radio.unwrap();
        assert_eq!(world.get::<SiliconRadio>(mob).map(|r| r.0), Some(radio));
        assert!(world.get::<EarsWorn>(mob).is_some());
        assert_eq!(world.get::<BuiltInRadio>(radio).unwrap().listener, mob);
        let r = world.get::<Radio>(radio).unwrap();
        assert!(r.on && r.listening && !r.broadcasting);
        // Taken out again: the brainmob is in the brain, dead, without the radio.
        assert_eq!(remove_brain(&mut world, mmi), Some(brain));
        assert_eq!(world.get::<Occupant>(brain), Some(&Occupant(mob)));
        assert_eq!(world.get::<BrainMob>(mob).unwrap().container, brain);
        assert_eq!(world.get::<Stat>(mob), Some(&Stat::Dead));
        assert!(world.get::<SiliconRadio>(mob).is_none());
        assert_eq!(world.get::<BuiltInRadio>(radio).unwrap().listener, mmi);
        assert!(world.get::<OrganFrozen>(brain).is_none());
        assert!(world.get::<BrainProcessor>(mmi).unwrap().brain.is_none());
    }

    #[test]
    fn a_failing_brain_stays_dead_in_the_mmi() {
        let mut world = world();
        let pos = TilePos::new(3, 3, 1);
        let mmi = item(&mut world, MMI, pos);
        let (brain, mob, _) = brain_with_player(&mut world, pos);
        world.get_mut::<Organ>(brain).unwrap().failing = true;
        insert_brain(&mut world, mmi, brain);
        assert_eq!(world.get::<Stat>(mob), Some(&Stat::Dead));
    }

    #[test]
    fn a_ghost_enters_a_posibrain_once() {
        let mut world = world();
        let pos = TilePos::new(4, 4, 1);
        let posi = item(&mut world, POSIBRAIN, pos);
        let client = world.spawn_empty().id();
        let ghost = world
            .spawn((
                Ghost { body: None },
                PlayerMob { client },
                MoveQueue::default(),
                pos,
            ))
            .id();
        world.resource_mut::<Players>().set_mob(client, ghost);
        ghost_enters(&mut world, client, ghost, posi);
        let Occupant(mob) = *world.get::<Occupant>(posi).expect("a brainmob");
        assert_eq!(world.resource::<Players>().mob(client), Some(mob));
        assert!(world.get_entity(ghost).is_err());
        assert_eq!(world.get::<Stat>(mob), Some(&Stat::Conscious));
        assert!(occupied(&world, posi));
        let state = world.get::<BrainProcessor>(posi).unwrap();
        assert_eq!(state.entered, vec![client]);
        let name = world.get::<Name>(mob).unwrap().as_str().to_owned();
        assert!(name.contains('-'), "{name}");
        // Another ghost finds it taken.
        let other = world.spawn_empty().id();
        let other_ghost = world
            .spawn((Ghost { body: None }, PlayerMob { client: other }, pos))
            .id();
        world.resource_mut::<Players>().set_mob(other, other_ghost);
        ghost_enters(&mut world, other, other_ghost, posi);
        assert_eq!(world.resource::<Players>().mob(other), Some(other_ghost));
        // A display posibrain takes nobody.
        let display = item(&mut world, POSIBRAIN_DISPLAY, pos);
        ghost_enters(&mut world, other, other_ghost, display);
        assert!(world.get::<Occupant>(display).is_none());
    }

    #[test]
    fn an_endoskeleton_is_complete_with_every_part() {
        let mut world = world();
        let pos = TilePos::new(1, 1, 1);
        let client = world.spawn_empty().id();
        let suit = item(&mut world, ROBOT_SUIT, pos);
        let mut parts = Vec::new();
        for (path, _) in PARTS {
            parts.push(item(&mut world, path, pos));
        }
        // A bare chest and head are refused.
        assert!(suit_interaction(
            &mut world,
            client,
            parts[CHEST],
            PARTS[CHEST].0,
            suit
        ));
        assert!(suit_interaction(
            &mut world,
            client,
            parts[HEAD],
            PARTS[HEAD].0,
            suit
        ));
        assert_eq!(world.get::<RobotSuit>(suit), None);
        // Iron on a bare endoskeleton is the ED-209's, not ours.
        assert!(!suit_interaction(&mut world, client, parts[0], IRON, suit));
        let cell = item(
            &mut world,
            "/obj/item/stock_parts/power_store/cell/high",
            pos,
        );
        assert!(chest_interaction(
            &mut world,
            client,
            cell,
            CELL,
            parts[CHEST]
        ));
        world.entity_mut(parts[CHEST]).insert(RobotChest {
            wired: true,
            cell: Some(cell),
        });
        for _ in 0..2 {
            let flash = item(&mut world, FLASH, pos);
            head_interaction(&mut world, client, flash, parts[HEAD]);
        }
        assert_eq!(
            world.get::<RobotHead>(parts[HEAD]).unwrap().flashes.len(),
            2
        );
        for (i, &part) in parts.iter().enumerate() {
            assert!(suit_interaction(&mut world, client, part, PARTS[i].0, suit));
            assert_eq!(world.get::<StoredIn>(part).map(|s| s.0), Some(suit));
        }
        let state = world.get::<RobotSuit>(suit).unwrap().clone();
        assert!(complete(&world, &state));
        // Now iron is refused.
        let iron = item(&mut world, IRON, pos);
        assert!(suit_interaction(&mut world, client, iron, IRON, suit));
        // An MMI without a mind does not make a cyborg.
        let mmi = item(&mut world, MMI, pos);
        processor(&mut world, mmi);
        assert!(brain_check(&mut world, client, mmi).is_none());
    }
}
