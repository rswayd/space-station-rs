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
// shortcut: the voice spans are bold text (tg `SPAN_COMMAND` is bold and large, the clown's
// `SPAN_CLOWN`, an emagged one `SPAN_REALLYBIG`): the chat has no larger span yet; no emag, no
// TTS filter (no TTS), no sign language check; only local speech is loud, not what goes to a
// radio.
// shortcut: no crew (no suit sensors yet), bounty shuttle or material sniffer pinpointers.
// Cost: one pass over the megaphones (a handful per map) per tick, one lookup per spoken line;
// the active pair pinpointers every 0.2 s.

use bevy::prelude::*;
use ss13_chat::server::{SaySystems, Spoke};
use ss13_chat::speech::VoiceSpan;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};

use crate::antag::nukeop::server::{pinpointer_state, set_pinpointer_overlay};
use crate::inventory::{CarriedBy, Inventory};
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

pub struct DevicesPlugin;

impl Plugin for DevicesPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // A pair pinpointer toggles.
        app.add_click_handler(ClickPhase::AttackSelf, 1700, |w, c| {
            crate::devices::attack_self(w, c.client, c.mob, c.target)
        });
        app.add_message::<Spoke>()
            .add_message::<SendChat>()
            .add_observer(add_megaphone)
            .add_observer(add_pair_pinpointer)
            .add_systems(
                Update,
                (
                    megaphone_voices.before(SaySystems),
                    megaphone_spoke.after(SaySystems),
                    link_pairs,
                    tick_pair_pinpointers,
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

fn add_pair_pinpointer(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if items
        .get(add.entity)
        .is_ok_and(|i| is_under(&i.path, PAIR_PINPOINTER))
    {
        commands
            .entity(add.entity)
            .try_insert_if_new(PairPinpointer::default());
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

/// `/obj/item/pinpointer/attack_self` -> `toggle_on` of a pair pinpointer. True when `item` is
/// one.
pub fn attack_self(world: &mut World, _client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(mut pinpointer) = world.get_mut::<PairPinpointer>(item) else {
        return false;
    };
    pinpointer.active = !pinpointer.active;
    let active = pinpointer.active;
    if let Some(pos) = crate::modpc::pda::turf_of(world, item) {
        playsound(world, PlaySound::new(pos, PINPOINTER_TOGGLE, 50.0, true));
    }
    if active {
        refresh_pair(world, item);
    } else {
        set_pinpointer_overlay(world, item, None);
    }
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
    true
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

/// `process` of the active pair pinpointers (`SSfastprocess`).
fn tick_pair_pinpointers(world: &mut World, mut next: Local<f64>) {
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
}

/// `/obj/item/pinpointer/pair/examine`: who holds the other half.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
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
