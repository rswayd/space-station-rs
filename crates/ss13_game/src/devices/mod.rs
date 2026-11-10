//! Small handheld devices (server), after tg `code/game/objects/items/devices`.
//!
//! - Megaphones (`megaphone.dm`): held in the active hand, the holder's spoken words are drawn
//!   in the megaphone's voice span ([`ss13_chat::speech::VoiceSpan`]) with its sound, then it
//!   recharges for 5 s ("нужно перезарядиться" when spoken into too soon).
//! - Pair pinpointers (`pinpointer.dm`, `/obj/item/pinpointer/pair`): two that start in the same
//!   box (`/obj/item/storage/box/pinpointer_pairs`) track each other; used in hand they turn on
//!   or off, and while on their arrow points at the other half (the nuclear operatives'
//!   pinpointer's looks, `crate::antag::nukeop::server::pinpointer_state`), refreshed every
//!   0.2 s (`SSfastprocess`).
//! - Crew pinpointers (`/obj/item/pinpointer/crew`): the first user owns it; used in hand it
//!   lists the crew whose suit sensors are on tracking (the contractor's: any mode) on its level
//!   or another station level, by ID name, and points at the chosen one until their sensors
//!   stop tracking (`scan_for_target`); used again it turns off.
//! - Handheld crew monitors (`/obj/item/sensor_device`): the crew console's window
//!   (`crate::clothing::sensors`) for the level the holder is on.
//! - Flashlights ([`flashlight`]), GPS units ([`gps`]), the RCD ([`rcd`]) and its family
//!   (RLD, RTD, RDD, RPLD, RWD, RSF: [`rhd`]), tape recorders
//!   ([`taperecorder`]), laser pointers ([`laser_pointer`]), tracking and choice beacons
//!   ([`beacons`]), binoculars ([`binoculars`]), broadcast cameras ([`broadcast_camera`]), door remotes ([`door_remote`]), door seals ([`door_seal`]), table clocks
//!   ([`table_clock`]) and hand gestures ([`hand_items`]).
// shortcut: the voice spans are bold text (tg `SPAN_COMMAND` is bold and large, the clown's
// `SPAN_CLOWN`, an emagged one `SPAN_REALLYBIG`): the chat has no larger span yet; no emag, no
// TTS filter (no TTS), no sign language check; only local speech is loud, not what goes to a
// radio.
// shortcut: no bounty shuttle or material sniffer pinpointers; the handheld monitor has no
// drag-to-activate; its window is the console's (List and Map tabs, `crate::nanomap`).
// Cost: one pass over the megaphones (a handful per map) per tick, one lookup per spoken line;
// the active pair and crew pinpointers every 0.2 s (a crew pinpointer's choice list: one pass
// over the mobs with an inventory, on use).

pub mod beacons;
pub mod binoculars;
pub mod broadcast_camera;
pub mod door_remote;
pub mod door_seal;
pub mod flashlight;
pub mod gps;
pub mod hand_items;
pub mod laser_pointer;
pub mod rcd;
pub mod rhd;
pub mod table_clock;
pub mod taperecorder;

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_chat::server::{SaySystems, Spoke};
use ss13_chat::speech::VoiceSpan;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};

use ss13_shared::{Dir, TilePos};
use ss13_tgui::UiStatus;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_tgui::server::{AlwaysState, UiSystems, UiWindow, distance_status};

use crate::antag::nukeop::server::{pinpointer_state, set_pinpointer_overlay};
use crate::clothing::sensors::CrewConsole;
use crate::health::Stat;
use crate::inventory::{CarriedBy, Inventory};
use crate::modpc::pda::turf_of;
use crate::sound::{PlaySound, playsound};
use crate::storage::StoredIn;

const MEGAPHONE: &str = "/obj/item/megaphone";
const MEGAPHONE_SOUND: &str = "sound/items/megaphone.ogg";
/// `spamcheck = world.time + 5 SECONDS`.
const RECHARGE_SECS: f64 = 5.0;
/// tg `voicespan` (see the shortcut above).
const VOICE: Span = Span::Bold;
const PAIR_PINPOINTER: &str = "/obj/item/pinpointer/pair";
/// `toggle_on`: `playsound(src, 'screwdriver2.ogg', 50, TRUE)`.
const PINPOINTER_TOGGLE: &str = "sound/items/tools/screwdriver2.ogg";
/// tg `SSfastprocess` wait.
const PINPOINTER_SECS: f64 = 0.2;
const CREW_PINPOINTER: &str = "/obj/item/pinpointer/crew";
/// `ignore_suit_sensor_level = TRUE`, `minimum_range = 25`.
const CONTRACTOR_PINPOINTER: &str = "/obj/item/pinpointer/crew/contractor";
const CONTRACTOR_MINIMUM_RANGE: f64 = 25.0;
const SENSOR_DEVICE: &str = "/obj/item/sensor_device";

/// `/obj/item/pinpointer/crew`: `active`, `target`, `pinpointer_owner`,
/// `ignore_suit_sensor_level`, `minimum_range`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct CrewPinpointer {
    pub active: bool,
    pub target: Option<Entity>,
    pub owner: Option<Entity>,
    pub ignore_level: bool,
    pub minimum_range: f64,
}

/// The crew pinpointer a choice prompt is about, and the mobs behind its items.
#[derive(Component, Clone, Debug)]
struct CrewPinPrompt {
    item: Entity,
    mobs: Vec<Entity>,
}

/// `/obj/item/sensor_device`: carries a [`CrewConsole`] read for the crew window.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct HandheldMonitor;

/// `/obj/item/pinpointer/pair`: `active`, `other_pair`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PairPinpointer {
    pub active: bool,
    pub other: Option<Entity>,
}

/// `/obj/item/megaphone`: `spamcheck`, and the mob it gives its voice to now.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Megaphone {
    pub ready_at: f64,
    pub voiced: Option<Entity>,
}

/// A timed action of a device ([`crate::construction::Job::Device`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceJob {
    /// Bracing an airlock with this seal ([`door_seal`]).
    SealDoor { seal: Entity },
    /// Taking an airlock's seal off ([`door_seal`]).
    UnsealDoor,
    /// Fixing a broken table clock ([`table_clock`]).
    RepairClock(Entity),
    /// The stealing hand taking these shoes ([`hand_items`]).
    StealShoes(Entity),
}

/// The end of a [`DeviceJob`].
pub fn complete_job(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    job: DeviceJob,
) {
    match job {
        DeviceJob::SealDoor { seal } => door_seal::complete_seal(world, client, mob, target, seal),
        DeviceJob::UnsealDoor => door_seal::complete_unseal(world, mob, target),
        DeviceJob::RepairClock(clock) => table_clock::complete_repair(world, mob, clock),
        DeviceJob::StealShoes(shoes) => hand_items::complete_steal(world, mob, target, shoes),
    }
}

pub struct DevicesPlugin;

impl Plugin for DevicesPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // A pair or crew pinpointer toggles, a handheld monitor opens.
        app.add_click_handler(ClickPhase::AttackSelf, 1700, |w, c| {
            crate::devices::attack_self(w, c.client, c.mob, c.target)
        });
        beacons::build(app);
        binoculars::build(app);
        broadcast_camera::build(app);
        door_remote::build(app);
        hand_items::build(app);
        table_clock::build(app);
        flashlight::build(app);
        gps::build(app);
        laser_pointer::build(app);
        rcd::build(app);
        rhd::build(app);
        taperecorder::build(app);
        app.add_message::<Spoke>()
            .add_message::<SendChat>()
            .add_observer(add_megaphone)
            .add_observer(add_pinpointer)
            .add_observer(add_monitor)
            .add_systems(
                Update,
                (
                    megaphone_voices.before(SaySystems),
                    megaphone_spoke.after(SaySystems),
                    link_pairs,
                    tick_pinpointers,
                    monitor_status.in_set(UiSystems::StatusHook),
                    crew_answers
                        .in_set(UiSystems::Handle)
                        .after(ss13_tgui::prompt::PromptSystems::Answer),
                ),
            );
    }
}

fn add_megaphone(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if items
        .get(add.entity)
        .is_ok_and(|i| is_under(&i.path, MEGAPHONE))
    {
        commands
            .entity(add.entity)
            .try_insert_if_new(Megaphone::default());
    }
}

/// The mob holding `item` in its active hand (tg `get_active_held_item() == src`).
fn active_holder(
    item: Entity,
    carried: Option<&CarriedBy>,
    inventories: &Query<&Inventory>,
) -> Option<Entity> {
    let mob = carried?.0;
    (inventories.get(mob).ok()?.active_item() == Some(item)).then_some(mob)
}

/// `equipped`/`dropped` + `handle_speech`'s checks: a ready megaphone in the active hand gives
/// its holder the voice span, anything else takes it back.
fn megaphone_voices(
    time: Res<Time>,
    mut megaphones: Query<(Entity, &mut Megaphone, Option<&CarriedBy>)>,
    inventories: Query<&Inventory>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    let mut given = Vec::new();
    for (e, mut megaphone, carried) in &mut megaphones {
        let wanted = active_holder(e, carried, &inventories).filter(|_| now >= megaphone.ready_at);
        if wanted == megaphone.voiced {
            continue;
        }
        if let Some(old) = megaphone.voiced {
            commands.entity(old).try_remove::<VoiceSpan>();
        }
        megaphone.voiced = wanted;
        given.extend(wanted);
    }
    // After every removal: a mob swapping one megaphone for another keeps its voice.
    for mob in given {
        commands.entity(mob).try_insert(VoiceSpan(VOICE));
    }
}

/// `handle_speech` after the words went out: the sound and the recharge, or the warning.
fn megaphone_spoke(
    time: Res<Time>,
    mut spoken: MessageReader<Spoke>,
    mut megaphones: Query<(Entity, &mut Megaphone, Option<&CarriedBy>)>,
    inventories: Query<&Inventory>,
    names: Query<&ss13_mapload::DmName>,
    mut chat: MessageWriter<SendChat>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for spoke in spoken.read() {
        if spoke.whisper || spoke.radio.is_some() {
            continue;
        }
        let Some(item) = inventories
            .get(spoke.speaker)
            .ok()
            .and_then(Inventory::active_item)
        else {
            continue;
        };
        let Ok((_, mut megaphone, carried)) = megaphones.get_mut(item) else {
            continue;
        };
        if active_holder(item, carried, &inventories) != Some(spoke.speaker) {
            continue;
        }
        if now < megaphone.ready_at {
            let name = names.get(item).map_or("мегафон", |n| n.name.as_str());
            chat.write(SendChat {
                clients: vec![spoke.client],
                message: ChatMessage::simple(
                    ChatChannel::Info,
                    Span::Warning,
                    format!("{name} нужно перезарядиться!"),
                ),
            });
            continue;
        }
        megaphone.ready_at = now + RECHARGE_SECS;
        let pos = spoke.pos;
        commands.queue(move |world: &mut World| {
            // `playsound(loc, 'megaphone.ogg', 100, FALSE, TRUE)`.
            playsound(world, PlaySound::new(pos, MEGAPHONE_SOUND, 100.0, false));
        });
    }
}

fn add_pinpointer(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    if is_under(&item.path, PAIR_PINPOINTER) {
        commands
            .entity(add.entity)
            .try_insert_if_new(PairPinpointer::default());
    } else if is_under(&item.path, CREW_PINPOINTER) {
        let contractor = is_under(&item.path, CONTRACTOR_PINPOINTER);
        commands
            .entity(add.entity)
            .try_insert_if_new(CrewPinpointer {
                ignore_level: contractor,
                minimum_range: if contractor {
                    CONTRACTOR_MINIMUM_RANGE
                } else {
                    0.0
                },
                ..default()
            });
    }
}

fn add_monitor(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if items
        .get(add.entity)
        .is_ok_and(|i| is_under(&i.path, SENSOR_DEVICE))
    {
        commands.entity(add.entity).try_insert_if_new((
            HandheldMonitor,
            CrewConsole::default(),
            AlwaysState,
        ));
    }
}

/// `/obj/item/storage/box/pinpointer_pairs/PopulateContents`: `A.other_pair = B`,
/// `B.other_pair = A` for two new pair pinpointers in the same container.
fn link_pairs(
    new: Query<(Entity, &StoredIn), Added<PairPinpointer>>,
    all: Query<(Entity, &PairPinpointer, Option<&StoredIn>)>,
    mut commands: Commands,
) {
    let mut linked: Vec<Entity> = Vec::new();
    for (e, &StoredIn(box_)) in &new {
        let unlinked = |x: Entity, p: &PairPinpointer| p.other.is_none() && !linked.contains(&x);
        if all.get(e).is_ok_and(|(_, p, _)| !unlinked(e, p)) {
            continue;
        }
        let Some((other, _, _)) = all.iter().find(|&(o, p, stored)| {
            o != e && unlinked(o, p) && stored.is_some_and(|s| s.0 == box_)
        }) else {
            continue;
        };
        linked.extend([e, other]);
        for (a, b) in [(e, other), (other, e)] {
            commands.entity(a).try_insert(PairPinpointer {
                active: false,
                other: Some(b),
            });
        }
    }
}

/// `attack_self` of a pair or crew pinpointer or a handheld monitor. True when `item` is one.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world.get::<HandheldMonitor>(item).is_some() {
        crate::clothing::sensors::show(world, client, mob, item, false);
        return true;
    }
    if world.get::<CrewPinpointer>(item).is_some() {
        crew_attack_self(world, client, mob, item);
        return true;
    }
    let Some(mut pinpointer) = world.get_mut::<PairPinpointer>(item) else {
        return false;
    };
    pinpointer.active = !pinpointer.active;
    let active = pinpointer.active;
    toggle_sound(world, item);
    if active {
        refresh_pair(world, item);
    } else {
        set_pinpointer_overlay(world, item, None);
    }
    toggle_message(world, mob, active);
    true
}

/// `toggle_on`: `playsound(src, 'screwdriver2.ogg', 50, TRUE)`.
fn toggle_sound(world: &mut World, item: Entity) {
    if let Some(pos) = turf_of(world, item) {
        playsound(world, PlaySound::new(pos, PINPOINTER_TOGGLE, 50.0, true));
    }
}

/// "[user] [active ? "" : "de"]activates [user.p_their()] pinpointer."
fn toggle_message(world: &mut World, mob: Entity, active: bool) {
    let user = crate::medical::name_of(world, mob);
    let (own, others) = if active {
        (
            "Вы активируете пинпоинтер.".to_owned(),
            format!("{user} активирует пинпоинтер."),
        )
    } else {
        (
            "Вы деактивируете пинпоинтер.".to_owned(),
            format!("{user} деактивирует пинпоинтер."),
        )
    };
    crate::medical::visible(world, mob, &own, &others, Span::Notice);
}

/// `/obj/item/pinpointer/crew/trackable(H)`: the device and the mob on the same level or on
/// two station levels, the worn jumpsuit's sensors on tracking (any mode with
/// `ignore_suit_sensor_level`).
pub fn crew_trackable(world: &World, item: Entity, mob: Entity, ignore_level: bool) -> bool {
    let (Some(here), Some(there)) = (turf_of(world, item), turf_of(world, mob)) else {
        return false;
    };
    let station = crate::events::controller::station_levels(world);
    let level = there.z == here.z || (station.contains(&here.z) && station.contains(&there.z));
    level && crate::clothing::sensors::trackable(world, mob, ignore_level)
}

/// The ID name tg lists a mob under (`wear_id.GetID().registered_name`, else "Unknown").
fn id_name(world: &World, mob: Entity) -> String {
    world
        .get::<Inventory>(mob)
        .and_then(|i| i.id)
        .and_then(|id| crate::inventory::get_id(world, id))
        .and_then(|card| world.get::<crate::outfit::IdRegistration>(card))
        .map(|r| r.registered_name.clone())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Unknown".to_owned())
}

/// tg's `name_counts` loop: repeats of a name become "name (2)", "name (3)"...
fn unique_names(names: Vec<(String, Entity)>) -> Vec<(String, Entity)> {
    let mut counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut out = Vec::with_capacity(names.len());
    for (mut name, mob) in names {
        while let Some(count) = counts.get_mut(&name) {
            *count += 1;
            name = format!("{name} ({count})");
        }
        counts.insert(name.clone(), 1);
        out.push((name, mob));
    }
    // `sort_list(names)`.
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// `/obj/item/pinpointer/crew/attack_self`: off when on; else the owner check and the choice.
fn crew_attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) {
    let Some(pinpointer) = world.get::<CrewPinpointer>(item).copied() else {
        return;
    };
    if pinpointer.active {
        if let Some(mut p) = world.get_mut::<CrewPinpointer>(item) {
            p.active = false;
            p.target = None;
        }
        toggle_sound(world, item);
        set_pinpointer_overlay(world, item, None);
        toggle_message(world, mob, false);
        return;
    }
    let owner = pinpointer.owner.unwrap_or(mob);
    if let Some(mut p) = world.get_mut::<CrewPinpointer>(item) {
        p.owner = Some(owner);
    }
    if owner != mob {
        crate::tools::tell(
            world,
            client,
            Span::Notice,
            "Пинпоинтер не реагирует. Похоже, он узнаёт только своего владельца.",
        );
        return;
    }
    let mut q = world.query_filtered::<Entity, With<Inventory>>();
    let mobs: Vec<Entity> = q.iter(world).collect();
    let names: Vec<(String, Entity)> = mobs
        .into_iter()
        .filter(|&m| crew_trackable(world, item, m, pinpointer.ignore_level))
        .map(|m| (id_name(world, m), m))
        .collect();
    if names.is_empty() {
        let user = crate::medical::name_of(world, mob);
        crate::medical::visible(
            world,
            mob,
            "Вашему пинпоинтеру не удаётся обнаружить сигнал.",
            &format!("Пинпоинтеру {user} не удаётся обнаружить сигнал."),
            Span::Notice,
        );
        return;
    }
    let names = unique_names(names);
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Пинпоинтер",
            "Кого отслеживать",
            PromptKind::List {
                items: names.iter().map(|(n, _)| n.clone()).collect(),
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(CrewPinPrompt {
        item,
        mobs: names.into_iter().map(|(_, m)| m).collect(),
    });
}

/// The chosen crew member, if the user still holds the pinpointer and can act
/// (`QDELETED(src) || !user.is_holding(src) || user.incapacitated`).
fn crew_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, mob, answer) in answers {
        let Some(CrewPinPrompt { item, mobs }) = world.get::<CrewPinPrompt>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(index)) = answer else {
            continue;
        };
        let Some(&target) = mobs.get(index) else {
            continue;
        };
        let holding = world
            .get::<Inventory>(mob)
            .is_some_and(|i| i.hands.contains(&Some(item)));
        let able = world.get::<Stat>(mob) == Some(&Stat::Conscious)
            && !crate::status_effects::incapacitation(world, mob).incapacitated;
        if !holding || !able || world.get_entity(target).is_err() {
            continue;
        }
        let Some(mut p) = world.get_mut::<CrewPinpointer>(item) else {
            continue;
        };
        let was_active = p.active;
        p.target = Some(target);
        p.active = true;
        if !was_active {
            toggle_sound(world, item);
            toggle_message(world, mob, true);
        }
        refresh_crew(world, item);
    }
}

/// `scan_for_target` + `update_overlays` of an active crew pinpointer: a target that stopped
/// being trackable is dropped and the pinpointer goes dark.
fn refresh_crew(world: &mut World, item: Entity) {
    let Some(p) = world.get::<CrewPinpointer>(item).copied() else {
        return;
    };
    let target = p
        .target
        .filter(|&t| world.get_entity(t).is_ok() && crew_trackable(world, item, t, p.ignore_level));
    let Some(target) = target else {
        if let Some(mut p) = world.get_mut::<CrewPinpointer>(item) {
            p.active = false;
            p.target = None;
        }
        set_pinpointer_overlay(world, item, None);
        return;
    };
    let (Some(here), Some(there)) = (turf_of(world, item), turf_of(world, target)) else {
        return;
    };
    set_pinpointer_overlay(world, item, Some(crew_state(here, there, p.minimum_range)));
}

/// `get_direction_icon` with `minimum_range`: within it the exact place is hidden ("direct").
fn crew_state(here: TilePos, there: TilePos, minimum_range: f64) -> (String, Option<Dir>) {
    let (dx, dy) = (f64::from(there.x - here.x), f64::from(there.y - here.y));
    if there.z == here.z && dx.hypot(dy) <= minimum_range {
        return ("pinondirect".to_owned(), None);
    }
    pinpointer_state(here, Some(there), false)
}

/// tg `default_state` of the handheld monitor's crew window: interactive while the user carries
/// it (or stands next to it), disabled for an unconscious user.
fn monitor_status(
    mut windows: Query<&mut UiWindow>,
    monitors: Query<Option<&TilePos>, With<HandheldMonitor>>,
    users: Query<(&Inventory, Option<&Stat>, Option<&TilePos>)>,
) {
    for mut window in &mut windows {
        let Ok(at) = monitors.get(window.source) else {
            continue;
        };
        let status = item_ui_status(window.source, at, users.get(window.actor).ok());
        window.cap_status(status);
    }
}

/// tg `default_state` for a carried item's window: interactive while `user` holds `item`,
/// else by distance to it on the floor; disabled for an unconscious user.
pub(crate) fn item_ui_status(
    item: Entity,
    at: Option<&TilePos>,
    user: Option<(&Inventory, Option<&Stat>, Option<&TilePos>)>,
) -> UiStatus {
    let Some((inventory, stat, user_at)) = user else {
        return UiStatus::Close;
    };
    let mut status = if inventory.holds(item) {
        UiStatus::Interactive
    } else {
        match (at, user_at) {
            (Some(&at), Some(&user)) => distance_status(user, at),
            _ => UiStatus::Close,
        }
    };
    if stat.is_some_and(|s| *s != Stat::Conscious) {
        status = status.min(UiStatus::Disabled);
    }
    status
}

/// `update_overlays` of an active pair pinpointer: the arrow at its other half.
fn refresh_pair(world: &mut World, item: Entity) {
    let Some(&PairPinpointer { other, .. }) = world.get::<PairPinpointer>(item) else {
        return;
    };
    let Some(here) = crate::modpc::pda::turf_of(world, item) else {
        return;
    };
    let there = other
        .filter(|&o| world.get_entity(o).is_ok())
        .and_then(|o| crate::modpc::pda::turf_of(world, o));
    let state = pinpointer_state(here, there, false);
    set_pinpointer_overlay(world, item, Some(state));
}

/// `process` of the active pair and crew pinpointers (`SSfastprocess`).
fn tick_pinpointers(world: &mut World, mut next: Local<f64>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + PINPOINTER_SECS;
    let active: Vec<Entity> = world
        .query::<(Entity, &PairPinpointer)>()
        .iter(world)
        .filter(|(_, p)| p.active)
        .map(|(e, _)| e)
        .collect();
    for e in active {
        refresh_pair(world, e);
    }
    let crew: Vec<Entity> = world
        .query::<(Entity, &CrewPinpointer)>()
        .iter(world)
        .filter(|(_, p)| p.active)
        .map(|(e, _)| e)
        .collect();
    for e in crew {
        refresh_crew(world, e);
    }
}

/// `/obj/item/pinpointer/pair/examine`: who holds the other half; `/obj/item/pinpointer/examine`
/// of a crew pinpointer: whom it tracks.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    rcd::examine_lines(world, target, lines);
    binoculars::examine_lines(world, target, lines);
    broadcast_camera::examine_lines(world, target, lines);
    crate::silicon::mob_cameras::examine_lines(world, target, lines);
    door_seal::examine_lines(world, target, lines);
    table_clock::examine_lines(world, target, lines);
    rhd::examine_lines(world, target, lines);
    laser_pointer::examine_lines(world, target, lines);
    taperecorder::examine_lines(world, target, lines);
    if let Some(&CrewPinpointer {
        target: Some(tracked),
        ..
    }) = world.get::<CrewPinpointer>(target)
    {
        let name = crate::medical::name_of(world, tracked);
        lines.push((Span::Plain, format!("Он отслеживает {name}.")));
        return;
    }
    let Some(&PairPinpointer {
        active: true,
        other: Some(other),
    }) = world.get::<PairPinpointer>(target)
    else {
        return;
    };
    if let Some(&CarriedBy(holder)) = world.get::<CarriedBy>(other) {
        let name = crate::medical::name_of(world, holder);
        lines.push((Span::Plain, format!("Его пару держит {name}.")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_names_get_counters_and_the_list_is_sorted() {
        let e = Entity::from_raw_u32;
        let names = vec![
            ("Ян".to_owned(), e(1).unwrap()),
            ("Unknown".to_owned(), e(2).unwrap()),
            ("Ян".to_owned(), e(3).unwrap()),
            ("Ян".to_owned(), e(4).unwrap()),
        ];
        let out: Vec<String> = unique_names(names).into_iter().map(|(n, _)| n).collect();
        assert_eq!(out, ["Unknown", "Ян", "Ян (2)", "Ян (3)"]);
    }

    #[test]
    fn contractor_range_hides_the_direction() {
        let here = TilePos::new(10, 10, 1);
        let near = TilePos::new(30, 10, 1);
        assert_eq!(
            crew_state(here, near, 25.0),
            ("pinondirect".to_owned(), None)
        );
        assert_eq!(
            crew_state(here, near, 0.0),
            ("pinonfar".to_owned(), Some(Dir::East))
        );
        let other_level = TilePos::new(10, 10, 2);
        assert_eq!(crew_state(here, other_level, 25.0).0, "pinonnull");
    }
}
