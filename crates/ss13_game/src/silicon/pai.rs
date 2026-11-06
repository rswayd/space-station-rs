//! Personal AIs (server), after tg `code/modules/pai/` (`card.dm`, `candidate.dm`, `pai.dm`,
//! `shell.dm`, `defense.dm`, `software.dm`, `door_jack.dm`, `death.dm`) and the data cable
//! (`code/game/objects/items/paiwire.dm`).
//!
//! - The card: used in hand without a pAI, a list of "Request pAI" and the ready candidates.
//!   A request pings every ghost (once per 40 s, tg `PAI_SPAM_TIME`); a ghost clicking a pAI
//!   card fills in its candidacy (name, description, OOC comments: tg `PaiSubmit`), then every
//!   used card without a pAI flashes. Downloading a candidate whose player is still a ghost
//!   makes the pAI: the ghost becomes it, it is in the card, it reads its laws.
//! - With a pAI the card's menu (tg `PaiCard`): imprint the user's DNA as master, set the
//!   supplemental directive (with a master), holomatrix on/off, leash on/off and its range
//!   (3..9), radio transmit/receive, reset the software, wipe the pAI (after a "Yes").
//! - The pAI (`/mob/living/silicon/pai`): folded it is invisible, stands where its card is and
//!   cannot move. Its buttons: the software menu, unfold/fold, the chassis list, rest, laws.
//!   Unfolded (40 s after the download or at once after a DNA imprint, then 5 s between
//!   changes) it is a visible holo-animal on the card's turf; the leash brings it back to the
//!   card when it strays farther than the range. Brute and burn hit the holochassis instead
//!   (20 health, 1.25 back per second); below 0 it folds back (10 s before it may unfold).
//! - Software: 100 RAM to spend on tg's programs at tg's prices; the host scan (health scan of
//!   the carrier or the master), the universal translator (every language), the encryption
//!   slot, the door jack, the DNA check of the carrier.
//! - Door jack: the cable goes into the carrier's hand (or on the floor); used on an airlock it
//!   plugs in; the pAI's hack then takes 15 s and opens it. The cable retracts when it is more
//!   than 2 tiles from the pAI, or on a failed hack.
//! - Laws: `pAI Directives`, law 0 "Serve your master."; an emag on the pAI or its card (on the
//!   floor) makes the emagger its master, "The Syndicate", with a Syndicate directive.
// shortcut: windows are list prompts (no `PaiCard`, `PaiSubmit`, `PaiInterface` windows); the
// candidacy lives for the round (no `pai.sav` savefile: persistence comes with the SQL wave);
// no screen images, card overlays or light (pickups: `crate::mob_holder`); the
// medical/security HUD, atmos sensor, GPS, music synthesizer, newscaster, signaler, camera,
// messenger, crew manifest and crew monitor programs can be bought but tell their device is
// missing (their devices' groups); no EMP or explosion effects, no encryption keys in the
// card; messages are chat lines (tg balloon alerts).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_chat::speech::BinaryCapable;
use ss13_mapload::{DmItem, IconIndex};
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems};
use ss13_tgui::server::{UiActed, UiSystems};
use ss13_world::Mover;

use super::Silicon;
use super::chassis::{name_of, tell_mob};
use super::laws::Laws;
use crate::actions::{ActionButton, ActionButtonsUi, set_buttons};
use crate::ghost::Ghost;
use crate::health::{Health, Stat};
use crate::inventory::{CarriedBy, Inventory};
use crate::multiz::GhostClick;
use crate::obj_damage::is_under;
use crate::radio::{Radio, SiliconRadio};
use crate::server::{PlayerMob, Players};
use crate::storage::StoredIn;
use crate::tools::{Actor, DoAfter, tell};

pub const PAI_CARD: &str = "/obj/item/pai_card";
pub const PAI_MOB: &str = "/mob/living/silicon/pai";
pub const PAI_CABLE: &str = "/obj/item/pai_cable";
const PAI_DMI: &str = "icons/mob/silicon/pai.dmi";
const SCREEN_PAI: &str = "icons/hud/screen_pai.dmi";
const PING: &str = "sound/machines/ping.ogg";
const BOOT: &str = "sound/effects/pai_boot.ogg";
const BUZZ: &str = "sound/machines/buzz/buzz-two.ogg";
const PRYING: &str = "sound/machines/airlock/airlock_alien_prying.ogg";
/// `PAI_SPAM_TIME`.
pub const REQUEST_COOLDOWN_SECS: f64 = 40.0;
/// `HOLOCHASSIS_INIT_TIME`, `HOLOCHASSIS_COOLDOWN`, `HOLOCHASSIS_OVERLOAD_COOLDOWN`.
pub const HOLO_INIT_SECS: f64 = 40.0;
pub const HOLO_COOLDOWN_SECS: f64 = 5.0;
pub const HOLO_OVERLOAD_SECS: f64 = 10.0;
/// `HOLOCHASSIS_MAX_HEALTH`, `HOLOCHASSIS_REGEN_PER_SECOND`.
pub const HOLO_MAX_HEALTH: f32 = 20.0;
pub const HOLO_REGEN: f32 = 1.25;
/// `HOLOFORM_MIN_RANGE`, `HOLOFORM_MAX_RANGE` (= `HOLOFORM_DEFAULT_RANGE`).
pub const RANGE_MIN: u32 = 3;
pub const RANGE_MAX: u32 = 9;
/// door_jack.dm `CABLE_LENGTH`; the hack's `do_after(15 SECONDS)` in deciseconds.
pub const CABLE_LENGTH: u32 = 2;
const HACK_DS: f32 = 150.0;
/// `ram`.
pub const START_RAM: u32 = 100;
const MAX_NAME_LEN: u32 = 42;
const MAX_BROADCAST_LEN: u32 = 512;
const ZEROTH: &str = "Serve your master.";
const SYNDICATE_LAW: &str = "Do not interfere with the operations of the Syndicate.";

/// `available_software` with its RAM costs.
pub const SOFTWARE: [(&str, u32); 15] = [
    ("Atmospheric Sensor", 5),
    ("Crew Manifest", 5),
    ("Digital Messenger", 5),
    ("Photography Module", 5),
    ("Encryption Slot", 10),
    ("Music Synthesizer", 10),
    ("Newscaster", 10),
    ("Remote Signaler", 10),
    ("Host Scan", 20),
    ("Medical HUD", 20),
    ("Security HUD", 20),
    // BandaStation: the crew monitor program.
    ("Crew Monitor", 35),
    ("Door Jack", 35),
    ("Internal GPS", 35),
    ("Universal Translator", 35),
];

/// `possible_chassis`, sorted.
pub const CHASSIS: [&str; 18] = [
    "bat",
    "butterfly",
    "cat",
    "chicken",
    "corgi",
    "crow",
    "duffel",
    "fox",
    "frog",
    "hawk",
    "kitten",
    "lizard",
    "monkey",
    "mouse",
    "puppy",
    "rabbit",
    "repairbot",
    "spider",
];

/// A pAI card's state (tg `pai`, `request_spam`); made on first use.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PaiCard {
    pub pai: Option<Entity>,
    /// A new request waits until then.
    pub request_until: f64,
}

/// tg `/mob/living/silicon/pai` vars.
#[derive(Component, Clone, Debug)]
pub struct Pai {
    pub card: Entity,
    pub holoform: bool,
    pub can_holo: bool,
    pub chassis: &'static str,
    pub resting: bool,
    pub holo_health: f32,
    /// `holochassis_ready` from then on.
    pub ready_at: f64,
    pub master: Option<Entity>,
    pub master_name: Option<String>,
    pub master_dna: Option<String>,
    pub emagged: bool,
    pub ram: u32,
    pub installed: Vec<&'static str>,
    /// The leash's range, `None` without a leash.
    pub leash: Option<u32>,
    pub can_transmit: bool,
    pub can_receive: bool,
    pub encrypt_mod: bool,
    pub languages_granted: bool,
    pub cable: Option<Entity>,
}

/// tg `/obj/item/pai_cable`: its pAI and the airlock it is plugged into (`hacking_machine`).
#[derive(Component, Clone, Copy, Debug)]
pub struct PaiCable {
    pub pai: Entity,
    pub door: Option<Entity>,
}

/// tg `/datum/pai_candidate`.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub client: Entity,
    pub name: String,
    pub description: String,
    pub comments: String,
}

/// tg `SSpai.candidates`: the submitted candidates.
#[derive(Resource, Clone, Debug, Default)]
pub struct PaiCandidates(pub Vec<Candidate>);

/// The pAI's hack of an airlock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaiJob {
    Hack { cable: Entity },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardAct {
    Request,
    Download(Entity),
    SetDna,
    SetLaws,
    ToggleHolo,
    ToggleLeash,
    RangeUp,
    RangeDown,
    ToggleTransmit,
    ToggleReceive,
    Reset,
    Wipe,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SoftAct {
    Buy(&'static str),
    ScanHolder,
    ScanMaster,
    Translator,
    Encryption,
    CableExtend,
    CableHack,
    CableRetract,
    CheckDna,
    Missing(&'static str),
}

/// The prompts, on their prompt entity.
#[derive(Component, Clone, Debug)]
enum PaiPrompt {
    CardMenu {
        card: Entity,
        user: Entity,
        acts: Vec<CardAct>,
    },
    ConfirmWipe {
        card: Entity,
        user: Entity,
    },
    SetLaws {
        card: Entity,
        user: Entity,
    },
    /// The ghost's candidacy, field by field (0 name, 1 description, 2 comments).
    Submit {
        step: u8,
        draft: Candidate,
    },
    Software {
        pai: Entity,
        acts: Vec<SoftAct>,
    },
    Chassis {
        pai: Entity,
    },
    DnaConsent {
        pai: Entity,
        holder: Entity,
    },
}

const B_SOFTWARE: &str = "pai_software";
const B_SHELL: &str = "pai_shell";
const B_CHASSIS: &str = "pai_chassis";
const B_REST: &str = "pai_rest";
const B_LAWS: &str = "pai_laws";

pub struct PaiPlugin;

impl Plugin for PaiPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // The card's menu.
        app.add_click_handler(ClickPhase::AttackSelf, 165, |w, c| {
            attack_self(w, c.client, c.mob, c.target)
        });
        // The data cable into an airlock.
        app.add_click_handler(ClickPhase::HeldOnTarget, 1425, |w, c| {
            c.held
                .is_some_and(|h| cable_interaction(w, c.client, c.mob, h, c.target))
        });
        app.init_resource::<PaiCandidates>().add_systems(
            Update,
            (
                answered
                    .in_set(UiSystems::Handle)
                    .after(PromptSystems::Answer),
                buttons.in_set(UiSystems::Handle),
                ghost_clicks.after(crate::click::ResolveClicks),
                follow_cards.run_if(any_with_component::<Pai>),
                shell_damage.run_if(any_with_component::<Pai>),
                cable_reach.run_if(any_with_component::<PaiCable>),
            ),
        );
    }
}

fn now(world: &World) -> f64 {
    world
        .get_resource::<Time>()
        .map_or(0.0, |t| t.elapsed_secs_f64())
}

fn is_card(world: &World, e: Entity) -> bool {
    world
        .get::<DmItem>(e)
        .is_some_and(|i| is_under(&i.path, PAI_CARD))
}

/// The card's clicks on mobs are its own (tg `pai_card` has no attack).
pub fn interacts_with_mobs(path: &str) -> bool {
    is_under(path, PAI_CARD) || is_under(path, PAI_CABLE)
}

fn card(world: &mut World, e: Entity) -> Option<PaiCard> {
    if !is_card(world, e) {
        return None;
    }
    if world.get::<PaiCard>(e).is_none() {
        world.entity_mut(e).insert(PaiCard::default());
    }
    world.get::<PaiCard>(e).copied()
}

/// tg `user in get_nested_locs(src)`: `user` carries the card, maybe in a bag.
fn carries(world: &World, user: Entity, item: Entity) -> bool {
    let mut e = item;
    for _ in 0..8 {
        let up = world
            .get::<CarriedBy>(e)
            .map(|c| c.0)
            .or_else(|| world.get::<StoredIn>(e).map(|s| s.0));
        match up {
            Some(up) if up == user => return true,
            Some(up) => e = up,
            None => return false,
        }
    }
    false
}

/// tg `get_holder`: the carbon carrying the card (in a hand, a pocket or a bag).
pub fn holder(world: &World, card: Entity) -> Option<Entity> {
    let mut e = card;
    for _ in 0..8 {
        let up = world
            .get::<CarriedBy>(e)
            .map(|c| c.0)
            .or_else(|| world.get::<StoredIn>(e).map(|s| s.0))?;
        if world.get::<crate::genetics::Dna>(up).is_some() {
            return Some(up);
        }
        e = up;
    }
    None
}

fn around(world: &mut World, e: Entity, span: Span, text: &str) {
    let Some(pos) = crate::explosions::turf_of(world, e) else {
        return;
    };
    for client in crate::obj_damage::clients_in_range(world, pos, 7, &[]) {
        tell(world, client, span, text);
    }
}

fn sound(world: &mut World, e: Entity, path: &str, vol: f32) {
    if let Some(pos) = crate::explosions::turf_of(world, e) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, path, vol, true));
    }
}

fn pai_of(world: &World, pai: Entity) -> Option<Pai> {
    world.get::<Pai>(pai).cloned()
}

fn with_pai(world: &mut World, pai: Entity, f: impl FnOnce(&mut Pai)) {
    if let Some(mut p) = world.get_mut::<Pai>(pai) {
        f(&mut p);
    }
}

/// tg `/datum/ai_laws/pai`.
pub fn pai_laws() -> Laws {
    Laws {
        name: "pAI Directives".into(),
        zeroth: Some(ZEROTH.into()),
        ..default()
    }
}

/// A ready candidate: still a ghost (tg `check_ready`; one gone is forgotten).
fn ready(world: &World, client: Entity) -> bool {
    world
        .get_resource::<Players>()
        .and_then(|p| p.mob(client))
        .is_some_and(|m| world.get::<Ghost>(m).is_some())
}

fn ready_candidates(world: &mut World) -> Vec<Candidate> {
    let all = world
        .get_resource::<PaiCandidates>()
        .map(|c| c.0.clone())
        .unwrap_or_default();
    let (keep, gone): (Vec<_>, Vec<_>) = all.into_iter().partition(|c| ready(world, c.client));
    if !gone.is_empty()
        && let Some(mut c) = world.get_resource_mut::<PaiCandidates>()
    {
        c.0.retain(|c| !gone.iter().any(|g| g.client == c.client));
    }
    keep
}

fn ask(world: &mut World, ask: Ask, prompt: PaiPrompt) {
    let e = ss13_tgui::prompt::ask_world(world, ask);
    world.entity_mut(e).insert(prompt);
}

fn list(items: Vec<String>) -> PromptKind {
    PromptKind::List {
        items,
        default: None,
    }
}

// ---- The card --------------------------------------------------------------------------------

/// tg `attack_self` -> `ui_interact`: the card's menu.
pub fn attack_self(world: &mut World, client: Entity, user: Entity, item: Entity) -> bool {
    let Some(state) = card(world, item) else {
        return false;
    };
    let title = name_of(world, item);
    let (items, acts): (Vec<String>, Vec<CardAct>) = match state.pai.and_then(|p| pai_of(world, p))
    {
        None => {
            let mut items = vec!["Request pAI".to_owned()];
            let mut acts = vec![CardAct::Request];
            for c in ready_candidates(world) {
                items.push(format!("Download: {} — {}", c.name, c.description));
                acts.push(CardAct::Download(c.client));
            }
            (items, acts)
        }
        Some(p) => {
            let on_off = |on: bool| if on { "on" } else { "off" };
            let mut rows = vec![
                ("Imprint master DNA".to_owned(), CardAct::SetDna),
                ("Set directive".to_owned(), CardAct::SetLaws),
                (
                    format!("Holomatrix: {}", on_off(p.can_holo)),
                    CardAct::ToggleHolo,
                ),
                (
                    format!("Leash: {}", on_off(p.leash.is_some())),
                    CardAct::ToggleLeash,
                ),
            ];
            if let Some(range) = p.leash {
                rows.push((format!("Range {range}: increase"), CardAct::RangeUp));
                rows.push((format!("Range {range}: decrease"), CardAct::RangeDown));
            }
            rows.extend([
                (
                    format!("Transmit: {}", on_off(p.can_transmit)),
                    CardAct::ToggleTransmit,
                ),
                (
                    format!("Receive: {}", on_off(p.can_receive)),
                    CardAct::ToggleReceive,
                ),
                ("Reset software".to_owned(), CardAct::Reset),
                ("Wipe pAI".to_owned(), CardAct::Wipe),
            ]);
            rows.into_iter().unzip()
        }
    };
    let message = match state.pai {
        None => "No personality installed.".to_owned(),
        Some(pai) => {
            let p = pai_of(world, pai);
            let master = p
                .as_ref()
                .and_then(|p| p.master_name.clone())
                .unwrap_or_else(|| "none".into());
            let dna = p
                .as_ref()
                .and_then(|p| p.master_dna.clone())
                .unwrap_or_else(|| "none".into());
            let laws = world
                .get::<Laws>(pai)
                .map(|l| {
                    l.inherent
                        .iter()
                        .map(|(l, _)| l.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
                .join(" ");
            format!(
                "{}: master {master} (DNA {dna}). Directive: {}",
                name_of(world, pai),
                if laws.is_empty() {
                    "none"
                } else {
                    laws.as_str()
                }
            )
        }
    };
    ask(
        world,
        Ask::new(client, user, title, message, list(items)),
        PaiPrompt::CardMenu {
            card: item,
            user,
            acts,
        },
    );
    true
}

fn card_act(world: &mut World, client: Entity, user: Entity, card: Entity, act: CardAct) {
    // Checked again after the prompt: the card still carried.
    if !carries(world, user, card) {
        return;
    }
    let Some(state) = world.get::<PaiCard>(card).copied() else {
        return;
    };
    match (act, state.pai) {
        (CardAct::Request, None) => find_pai(world, client, user, card),
        (CardAct::Download(candidate), None) => {
            download(world, client, card, candidate);
        }
        (_, None) => {}
        (CardAct::Request | CardAct::Download(_), Some(_)) => {}
        (CardAct::SetDna, Some(pai)) => {
            set_dna(world, client, user, pai);
        }
        (CardAct::SetLaws, Some(pai)) => {
            if pai_of(world, pai).is_none_or(|p| p.master.is_none()) {
                tell(world, client, Span::Warning, "access denied: no master");
                return;
            }
            let default = world
                .get::<Laws>(pai)
                .and_then(|l| l.inherent.first().map(|(l, _)| l.clone()))
                .unwrap_or_default();
            ask(
                world,
                Ask::new(
                    client,
                    user,
                    "pAI Directive Configuration",
                    "Enter any additional directives you would like your pAI personality to \
                     follow. Note that these directives will not override the personality's \
                     allegiance to its imprinted master. Conflicting directives will be ignored.",
                    PromptKind::Text {
                        default,
                        max_length: 300,
                        multiline: false,
                    },
                ),
                PaiPrompt::SetLaws { card, user },
            );
        }
        (CardAct::ToggleHolo, Some(pai)) => {
            let mut on = false;
            with_pai(world, pai, |p| {
                p.can_holo = !p.can_holo;
                on = p.can_holo;
            });
            let text = format!("holomatrix {}", if on { "enabled" } else { "disabled" });
            tell_mob(world, pai, Span::Notice, text);
        }
        (CardAct::ToggleLeash, Some(pai)) => with_pai(world, pai, |p| {
            p.leash = match p.leash {
                Some(_) => None,
                None => Some(RANGE_MAX),
            }
        }),
        (CardAct::RangeUp, Some(pai)) => increment_range(world, pai, 1),
        (CardAct::RangeDown, Some(pai)) => increment_range(world, pai, -1),
        (CardAct::ToggleTransmit, Some(pai)) => toggle_radio(world, pai, true),
        (CardAct::ToggleReceive, Some(pai)) => toggle_radio(world, pai, false),
        (CardAct::Reset, Some(pai)) => {
            reset_software(world, pai);
        }
        (CardAct::Wipe, Some(_)) => ask(
            world,
            Ask::new(
                client,
                user,
                "Personality Wipe",
                "Are you certain you wish to delete the current personality? This action cannot \
                 be undone.",
                PromptKind::Alert {
                    buttons: vec!["Yes".into(), "No".into()],
                    autofocus: true,
                },
            ),
            PaiPrompt::ConfirmWipe { card, user },
        ),
    }
}

/// tg `find_pai`: pings the ghosts, once per [`REQUEST_COOLDOWN_SECS`].
pub fn find_pai(world: &mut World, client: Entity, user: Entity, card: Entity) {
    let t = now(world);
    let Some(state) = world.get::<PaiCard>(card).copied() else {
        return;
    };
    if state.pai.is_some() {
        return;
    }
    if t < state.request_until {
        tell(world, client, Span::Warning, "request sent too recently");
        return;
    }
    if let Some(mut c) = world.get_mut::<PaiCard>(card) {
        c.request_until = t + REQUEST_COOLDOWN_SECS;
    }
    sound(world, card, PING, 20.0);
    tell(world, client, Span::Notice, "pAI assistance requested");
    let name = name_of(world, user);
    let ghosts: Vec<Entity> = world
        .query_filtered::<&PlayerMob, With<Ghost>>()
        .iter(world)
        .map(|p| p.client)
        .collect();
    for ghost in ghosts {
        tell(
            world,
            ghost,
            Span::BoldNotice,
            format!(
                "pAI Request! {name} is requesting a pAI companion! Click them or a pAI card to \
                 submit yourself as one."
            ),
        );
    }
}

/// tg `download_candidate` -> `new /mob/living/silicon/pai(card)`, `PossessByPlayer`.
pub fn download(world: &mut World, client: Entity, card: Entity, candidate: Entity) -> bool {
    if world.get::<PaiCard>(card).is_none_or(|c| c.pai.is_some()) {
        return false;
    }
    let found = world
        .get_resource::<PaiCandidates>()
        .and_then(|c| c.0.iter().find(|c| c.client == candidate).cloned());
    let Some(found) = found.filter(|c| ready(world, c.client)) else {
        tell(world, client, Span::Warning, "download interrupted");
        return false;
    };
    let Some(pos) = crate::explosions::turf_of(world, card) else {
        return false;
    };
    // Detached: the folded pAI is not replicated (`install`).
    let mob = crate::construction::spawn_detached(world, PAI_MOB)
        .unwrap_or_else(|| world.spawn_empty().id());
    world.entity_mut(mob).insert(pos);
    let name = if found.name.is_empty() {
        "pAI".to_owned()
    } else {
        found.name.clone()
    };
    install(world, card, mob, &name);
    super::attach_client(world, candidate, mob, pos);
    super::give_radio(world, mob, &[]);
    // tg `pai/binarycheck`: the pAI headset has no binary key.
    world.entity_mut(mob).remove::<BinaryCapable>();
    give_buttons(world, mob);
    let laws = world.get::<Laws>(mob).cloned().unwrap_or_default();
    super::show_laws(world, candidate, &laws);
    if let Some(mut c) = world.get_resource_mut::<PaiCandidates>() {
        c.0.retain(|c| c.client != candidate);
    }
    sound(world, card, BOOT, 50.0);
    let card_name = name_of(world, card);
    around(
        world,
        card,
        Span::Notice,
        &format!("{card_name} plays a cheerful startup noise!"),
    );
    info!("client {candidate} downloaded as the pAI {mob} into {card}");
    true
}

/// tg `/mob/living/silicon/pai/Initialize` and `set_personality`: `mob` becomes the folded pAI
/// of `card`, named `name`.
pub fn install(world: &mut World, card: Entity, mob: Entity, name: &str) {
    let t = now(world);
    world.entity_mut(mob).insert((
        Name::new(name.to_owned()),
        Silicon,
        pai_laws(),
        crate::health::living_bundle(),
        Pai {
            card,
            holoform: false,
            can_holo: true,
            chassis: "repairbot",
            resting: false,
            holo_health: HOLO_MAX_HEALTH,
            ready_at: t + HOLO_INIT_SECS,
            master: None,
            master_name: None,
            master_dna: None,
            emagged: false,
            ram: START_RAM,
            installed: Vec::new(),
            leash: Some(RANGE_MAX),
            can_transmit: true,
            can_receive: true,
            encrypt_mod: false,
            languages_granted: false,
            cable: None,
        },
    ));
    world.entity_mut(mob).remove::<(Networked, Mover)>();
    if let Some(mut c) = world.get_mut::<PaiCard>(card) {
        c.pai = Some(mob);
    } else {
        world.entity_mut(card).insert(PaiCard {
            pai: Some(mob),
            request_until: 0.0,
        });
    }
    refresh_look(world, mob);
}

fn give_buttons(world: &mut World, pai: Entity) {
    let icon = |world: &World, state: &str| {
        world
            .get_resource::<IconIndex>()
            .and_then(|i| i.get(SCREEN_PAI, state))
    };
    let rows = [
        (B_SOFTWARE, "Software Interface", "pai"),
        (B_SHELL, "Toggle Holoform", "pai_holoform"),
        (B_CHASSIS, "Holochassis Appearance Composite", "pai_chassis"),
        (B_REST, "Rest", "pai_rest"),
        (B_LAWS, "State Laws", "state_laws"),
    ];
    let buttons = rows
        .iter()
        .map(|&(id, name, state)| {
            (
                ActionButton {
                    id: id.to_owned(),
                    name: name.to_owned(),
                    icon: icon(world, state),
                    background: None,
                    border: None,
                    available: true,
                    cooldown: 0.0,
                    active: false,
                    maptext: None,
                },
                0.0,
            )
        })
        .collect();
    set_buttons(world, pai, buttons);
}

/// tg `wipe_pai` after its "Yes": the pAI is gone, its player a ghost.
pub fn wipe(world: &mut World, client: Entity, pai: Entity) {
    for (span, text) in [
        (
            Span::Warning,
            "You feel yourself slipping away from reality.",
        ),
        (Span::Danger, "Byte by byte you lose your sense of self."),
        (Span::Danger, "Your mental faculties leave you."),
        (Span::Plain, "oblivion... "),
    ] {
        tell_mob(world, pai, span, text);
    }
    tell(world, client, Span::Notice, "personality wiped");
    sound(world, pai, BUZZ, 30.0);
    despawn_pai(world, pai);
}

/// tg `death` / `Destroy`: the player ghosts, the card is free again, the cable is gone.
pub fn despawn_pai(world: &mut World, pai: Entity) {
    let state = pai_of(world, pai);
    // The ghost cannot come back: the pAI is gone (tg `qdel`).
    if let Some(client) = world.get::<PlayerMob>(pai).map(|p| p.client)
        && let Some(ghost) = crate::ghost::ghostize_client(world, client)
        && let Some(mut g) = world.get_mut::<Ghost>(ghost)
    {
        g.body = None;
    }
    if let Some(state) = state {
        if let Some(cable) = state.cable {
            world.try_despawn(cable).ok();
        }
        if let Some(mut c) = world.get_mut::<PaiCard>(state.card)
            && c.pai == Some(pai)
        {
            c.pai = None;
        }
    }
    world.try_despawn(pai).ok();
}

fn increment_range(world: &mut World, pai: Entity, by: i32) {
    with_pai(world, pai, |p| {
        if let Some(range) = p.leash {
            let new = range as i32 + by;
            if (RANGE_MIN as i32..=RANGE_MAX as i32).contains(&new) {
                p.leash = Some(new as u32);
            }
        }
    });
}

/// tg `toggle_radio`: cuts or mends the transmit or receive wire of the pAI's radio.
fn toggle_radio(world: &mut World, pai: Entity, transmit: bool) {
    let mut on = false;
    with_pai(world, pai, |p| {
        if transmit {
            p.can_transmit = !p.can_transmit;
            on = p.can_transmit;
        } else {
            p.can_receive = !p.can_receive;
            on = p.can_receive;
        }
    });
    if let Some(radio) = world.get::<SiliconRadio>(pai).map(|r| r.0)
        && let Some(mut r) = world.get_mut::<Radio>(radio)
    {
        if transmit {
            r.tx_cut = !on;
        } else {
            r.listening = on;
        }
    }
    let text = format!(
        "{} radio {}",
        if transmit { "outgoing" } else { "incoming" },
        if on { "enabled" } else { "disabled" }
    );
    tell_mob(world, pai, Span::Notice, text);
}

// ---- Laws, master, emag ----------------------------------------------------------------------

/// tg `set_dna`: a carbon's DNA imprints it as the master; the holochassis is ready at once.
pub fn set_dna(world: &mut World, client: Entity, user: Entity, pai: Entity) -> bool {
    let Some(dna) = world
        .get::<crate::genetics::Dna>(user)
        .map(|d| d.unique_enzymes.clone())
    else {
        tell(world, client, Span::Warning, "incompatible DNA signature");
        tell_mob(world, pai, Span::Warning, "incompatible DNA signature");
        return false;
    };
    if pai_of(world, pai).is_none_or(|p| p.emagged) {
        tell(
            world,
            client,
            Span::Warning,
            "directive system malfunctional",
        );
        return false;
    }
    let name = name_of(world, user);
    let t = now(world);
    with_pai(world, pai, |p| {
        p.master = Some(user);
        p.master_name = Some(name.clone());
        p.master_dna = Some(dna);
        p.ready_at = p.ready_at.min(t);
    });
    tell_mob(
        world,
        pai,
        Span::BoldWarning,
        format!("You have been bound to a new master: {name}!"),
    );
    true
}

/// tg `set_laws` after the prompt: the supplemental directive.
// tg `add_inherent_law` appends every new directive while the window shows and edits only the
// first; here the directive is replaced, as the window means.
pub fn set_directive(world: &mut World, pai: Entity, text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() || pai_of(world, pai).is_none_or(|p| p.master.is_none()) {
        return false;
    }
    if let Some(mut laws) = world.get_mut::<Laws>(pai) {
        laws.inherent.clear();
        laws.add_inherent(text, None, false);
    }
    tell_mob(world, pai, Span::Notice, text.to_owned());
    info!("pAI {pai} directive set: {text}");
    true
}

/// tg `reset_software`: no master, no directive, the leash back; not emagged any more.
pub fn reset_software(world: &mut World, pai: Entity) -> bool {
    let Some(state) = pai_of(world, pai) else {
        return false;
    };
    with_pai(world, pai, |p| p.emagged = false);
    if state.master.is_none() {
        return false;
    }
    with_pai(world, pai, |p| {
        p.master = None;
        p.master_name = None;
        p.master_dna = None;
        p.leash = Some(RANGE_MAX);
    });
    if let Some(mut laws) = world.get_mut::<Laws>(pai) {
        laws.inherent.clear();
    }
    tell_mob(world, pai, Span::Notice, "software rebooted");
    true
}

/// Whether `target` takes an emag here: a pAI, or a card with one (`crate::emag`).
pub fn takes_emag(world: &World, target: Entity) -> bool {
    world.get::<Pai>(target).is_some()
        || world
            .get::<PaiCard>(target)
            .is_some_and(|c| c.pai.is_some())
}

/// tg `emag_act` of the pAI or its card -> `handle_emag`.
pub fn emag_act(world: &mut World, client: Entity, attacker: Entity, target: Entity) -> bool {
    let pai = match world.get::<PaiCard>(target).and_then(|c| c.pai) {
        Some(pai) => pai,
        None if world.get::<Pai>(target).is_some() => target,
        None => return false,
    };
    tell(world, client, Span::Notice, "directive override complete");
    with_pai(world, pai, |p| {
        p.emagged = true;
        p.master = Some(attacker);
        p.master_name = Some("The Syndicate".into());
        p.master_dna = Some("Untraceable Signature".into());
    });
    if let Some(mut laws) = world.get_mut::<Laws>(pai) {
        laws.inherent.clear();
        laws.add_inherent(SYNDICATE_LAW, None, false);
    }
    tell_mob(world, pai, Span::Notice, "directive override detected");
    tell_mob(
        world,
        pai,
        Span::Danger,
        "ALERT: Foreign software detected.",
    );
    info!("{attacker} emagged the pAI {pai}");
    true
}

/// tg `check_dna`: the carrier is asked for a sample.
fn check_dna(world: &mut World, client: Entity, pai: Entity) {
    let Some(state) = pai_of(world, pai) else {
        return;
    };
    if state.emagged {
        tell(
            world,
            client,
            Span::Warning,
            "You are not at liberty to do this! All agents are clandestine.",
        );
        return;
    }
    // tg `if(!isnull(holder))` refuses a carried pAI: inverted, the intended check.
    let Some(holder) = holder(world, state.card) else {
        tell(world, client, Span::Warning, "not being carried");
        return;
    };
    let Some(holder_client) = world.get::<PlayerMob>(holder).map(|p| p.client) else {
        tell(world, client, Span::Warning, "dna sample refused!");
        return;
    };
    tell(world, client, Span::Notice, "requesting dna sample");
    let name = name_of(world, pai);
    ask(
        world,
        Ask::new(
            holder_client,
            holder,
            "Checking DNA",
            format!(
                "{name} is requesting a DNA sample from you. Will you allow it to confirm your \
                 identity?"
            ),
            PromptKind::Alert {
                buttons: vec!["Yes".into(), "No".into()],
                autofocus: true,
            },
        ),
        PaiPrompt::DnaConsent { pai, holder },
    );
}

fn dna_consent(world: &mut World, pai: Entity, holder: Entity, yes: bool) {
    let Some(state) = pai_of(world, pai) else {
        return;
    };
    if !yes {
        tell_mob(world, pai, Span::Warning, "dna sample refused!");
        return;
    }
    // Checked again: the same carrier.
    if self::holder(world, state.card) != Some(holder) {
        return;
    }
    let holder_name = name_of(world, holder);
    let name = name_of(world, pai);
    around(
        world,
        holder,
        Span::Notice,
        &format!("{holder_name} presses their thumb against {name}."),
    );
    let Some(dna) = world
        .get::<crate::genetics::Dna>(holder)
        .map(|d| d.unique_enzymes.clone())
    else {
        tell_mob(world, pai, Span::Warning, "no dna detected!");
        return;
    };
    tell_mob(
        world,
        pai,
        Span::BoldWarning,
        format!("{holder_name}'s UE string: {dna}"),
    );
    let matches = state.master_dna.as_deref() == Some(dna.as_str());
    tell_mob(
        world,
        pai,
        Span::Notice,
        format!(
            "DNA {} our stored Master's DNA.",
            if matches { "matches" } else { "does not match" }
        ),
    );
}

// ---- The holochassis -------------------------------------------------------------------------

fn refresh_look(world: &mut World, pai: Entity) {
    let Some(state) = pai_of(world, pai) else {
        return;
    };
    let icon_state = if state.resting {
        format!("{}_rest", state.chassis)
    } else {
        state.chassis.to_owned()
    };
    let Some(id) = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(PAI_DMI, &icon_state))
    else {
        return;
    };
    if let Some(mut a) = world.get_mut::<ss13_shared::Appearance>(pai)
        && a.icon_state != id
    {
        a.icon_state = id;
    }
}

/// tg `fold_out`: the holochassis on the card's turf. False when it may not.
pub fn fold_out(world: &mut World, pai: Entity, force: bool) -> bool {
    let Some(state) = pai_of(world, pai) else {
        return false;
    };
    if state.holoform {
        return fold_in(world, pai, force);
    }
    let refuse = if state.holo_health < 0.0 {
        Some("emitter repair incomplete")
    } else if !state.can_holo && !force {
        Some("emitters are disabled")
    } else if now(world) < state.ready_at {
        Some("emitters recycling...")
    } else {
        None
    };
    if let Some(why) = refuse {
        tell_mob(world, pai, Span::Warning, why);
        return false;
    }
    let Some(pos) = crate::explosions::turf_of(world, state.card) else {
        return false;
    };
    let t = now(world);
    with_pai(world, pai, |p| {
        p.holoform = true;
        p.ready_at = t + HOLO_COOLDOWN_SECS;
    });
    world
        .entity_mut(pai)
        .insert((pos, Networked, Mover::new(crate::RUN_DELAY_SECS)));
    refresh_look(world, pai);
    let name = name_of(world, pai);
    around(
        world,
        pai,
        Span::BoldNotice,
        &format!("{name} appears in a flash of light!"),
    );
    true
}

/// tg `fold_in`: back into the card (forced: a longer recharge).
pub fn fold_in(world: &mut World, pai: Entity, force: bool) -> bool {
    let Some(state) = pai_of(world, pai) else {
        return false;
    };
    let t = now(world);
    let wait = if force {
        HOLO_OVERLOAD_SECS
    } else {
        HOLO_COOLDOWN_SECS
    };
    if !state.holoform {
        return false;
    }
    let name = name_of(world, pai);
    around(world, pai, Span::Notice, &format!("{name} dematerialises!"));
    with_pai(world, pai, |p| {
        p.holoform = false;
        p.resting = false;
        p.ready_at = t + wait;
    });
    world.entity_mut(pai).remove::<(Networked, Mover)>();
    if let Some(pos) = crate::explosions::turf_of(world, state.card) {
        world.entity_mut(pai).insert(pos);
    }
    refresh_look(world, pai);
    true
}

/// tg `take_holo_damage`.
pub fn take_holo_damage(world: &mut World, pai: Entity, amount: f32) {
    let mut broken = false;
    with_pai(world, pai, |p| {
        p.holo_health = (p.holo_health - amount).clamp(-50.0, HOLO_MAX_HEALTH);
        broken = p.holo_health < 0.0;
    });
    if amount > 0.0 {
        tell_mob(
            world,
            pai,
            Span::Danger,
            "The impact degrades your holochassis!",
        );
    }
    if broken {
        fold_in(world, pai, true);
    }
}

/// Folded: on the card's turf. Unfolded: the leash pulls it back. A card gone takes it along.
// Cost: one pass over the pAIs (a handful) per tick, a short parent walk each.
fn follow_cards(world: &mut World) {
    let pais: Vec<(Entity, Pai)> = world
        .query::<(Entity, &Pai)>()
        .iter(world)
        .map(|(e, p)| (e, p.clone()))
        .collect();
    for (pai, state) in pais {
        if world.get_entity(state.card).is_err() {
            despawn_pai(world, pai);
            continue;
        }
        let Some(card_pos) = crate::explosions::turf_of(world, state.card) else {
            continue;
        };
        let here = world.get::<TilePos>(pai).copied();
        let strayed = match (state.holoform, state.leash, here) {
            (false, _, _) => here != Some(card_pos),
            (true, Some(range), Some(p)) => {
                p.z != card_pos.z || p.x.abs_diff(card_pos.x).max(p.y.abs_diff(card_pos.y)) > range
            }
            _ => false,
        };
        if strayed {
            world.entity_mut(pai).insert(card_pos);
        }
    }
}

/// tg `on_shell_damaged`: brute and burn go to the holochassis; `process`: it mends.
fn shell_damage(world: &mut World, mut last: Local<f64>) {
    let t = now(world);
    let dt = (t - *last).clamp(0.0, 1.0) as f32;
    *last = t;
    let hurt: Vec<(Entity, f32)> = world
        .query_filtered::<(Entity, &Health), With<Pai>>()
        .iter(world)
        .map(|(e, h)| (e, h.brute + h.burn))
        .collect();
    for (pai, amount) in hurt {
        if amount > 0.0 {
            world.entity_mut(pai).insert(Health::default());
            take_holo_damage(world, pai, amount);
        }
        with_pai(world, pai, |p| {
            if p.holo_health < HOLO_MAX_HEALTH {
                p.holo_health = (p.holo_health + HOLO_REGEN * dt).min(HOLO_MAX_HEALTH);
            }
        });
        if world.get::<Stat>(pai) == Some(&Stat::Dead) {
            despawn_pai(world, pai);
        }
    }
}

// ---- Buttons and software --------------------------------------------------------------------

fn buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let Some(messages) = world.get_resource::<Messages<UiActed<ActionButtonsUi>>>() else {
        return;
    };
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(messages)
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source
            || world.get::<Pai>(actor).is_none()
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
        {
            continue;
        }
        press(world, client, actor, &id);
    }
}

/// A pAI button pressed.
pub fn press(world: &mut World, client: Entity, pai: Entity, id: &str) {
    if world.get::<Stat>(pai) != Some(&Stat::Conscious) {
        return;
    }
    match id {
        B_SOFTWARE => software_menu(world, client, pai),
        B_SHELL => {
            fold_out(world, pai, false);
        }
        B_CHASSIS => {
            let items = CHASSIS.iter().map(|&c| c.to_owned()).collect();
            ask(
                world,
                Ask::new(
                    client,
                    pai,
                    "Holochassis",
                    "Choose a composite",
                    list(items),
                ),
                PaiPrompt::Chassis { pai },
            );
        }
        B_REST => {
            let mut text = None;
            if pai_of(world, pai).is_some_and(|p| p.holoform) {
                with_pai(world, pai, |p| {
                    p.resting = !p.resting;
                    text = Some(if p.resting {
                        "lays down for a moment..."
                    } else {
                        "perks up from the ground."
                    });
                });
            }
            refresh_look(world, pai);
            if let Some(text) = text {
                let name = name_of(world, pai);
                around(world, pai, Span::Notice, &format!("{name} {text}"));
            }
        }
        B_LAWS => {
            let laws = world.get::<Laws>(pai).cloned().unwrap_or_default();
            super::show_laws(world, client, &laws);
        }
        _ => {}
    }
}

fn software_menu(world: &mut World, client: Entity, pai: Entity) {
    let Some(state) = pai_of(world, pai) else {
        return;
    };
    let mut rows: Vec<(String, SoftAct)> = Vec::new();
    for &(name, cost) in &SOFTWARE {
        if !state.installed.contains(&name) {
            rows.push((format!("Download {name} ({cost} RAM)"), SoftAct::Buy(name)));
        }
    }
    for &name in &state.installed {
        match name {
            "Host Scan" => {
                rows.push(("Host Scan: carrier".into(), SoftAct::ScanHolder));
                rows.push(("Host Scan: master".into(), SoftAct::ScanMaster));
            }
            "Universal Translator" if !state.languages_granted => {
                rows.push((name.into(), SoftAct::Translator));
            }
            "Universal Translator" => {}
            "Encryption Slot" => rows.push((
                format!(
                    "Encryption Slot: {}",
                    if state.encrypt_mod { "on" } else { "off" }
                ),
                SoftAct::Encryption,
            )),
            "Door Jack" => match state.cable {
                None => rows.push(("Door Jack: extend cable".into(), SoftAct::CableExtend)),
                Some(_) => {
                    rows.push(("Door Jack: hack".into(), SoftAct::CableHack));
                    rows.push(("Door Jack: retract".into(), SoftAct::CableRetract));
                }
            },
            other => rows.push((other.into(), SoftAct::Missing(other))),
        }
    }
    rows.push(("Check DNA".into(), SoftAct::CheckDna));
    let (items, acts): (Vec<String>, Vec<SoftAct>) = rows.into_iter().unzip();
    let name = name_of(world, pai);
    ask(
        world,
        Ask::new(
            client,
            pai,
            name,
            format!("Available RAM: {}", state.ram),
            list(items),
        ),
        PaiPrompt::Software { pai, acts },
    );
}

/// tg `buy_software`: false when unknown, installed or too dear.
pub fn buy_software(world: &mut World, pai: Entity, selection: &str) -> bool {
    let Some(&(name, cost)) = SOFTWARE.iter().find(|(n, _)| *n == selection) else {
        return false;
    };
    let mut bought = false;
    with_pai(world, pai, |p| {
        if !p.installed.contains(&name) && p.ram >= cost {
            p.ram -= cost;
            p.installed.push(name);
            bought = true;
        }
    });
    bought
}

fn soft_act(world: &mut World, client: Entity, pai: Entity, act: SoftAct) {
    let Some(state) = pai_of(world, pai) else {
        return;
    };
    // Checked again: the program is still installed.
    let needs = match act {
        SoftAct::ScanHolder | SoftAct::ScanMaster => Some("Host Scan"),
        SoftAct::Translator => Some("Universal Translator"),
        SoftAct::Encryption => Some("Encryption Slot"),
        SoftAct::CableExtend | SoftAct::CableHack | SoftAct::CableRetract => Some("Door Jack"),
        _ => None,
    };
    if needs.is_some_and(|n| !state.installed.contains(&n)) {
        tell(world, client, Span::Warning, "software unavailable!");
        return;
    }
    match act {
        SoftAct::Buy(name) => {
            if !buy_software(world, pai, name) {
                tell(world, client, Span::Warning, "insufficient RAM");
            }
        }
        SoftAct::ScanHolder | SoftAct::ScanMaster => {
            let target = if act == SoftAct::ScanHolder {
                holder(world, state.card).ok_or("not being carried!")
            } else {
                state
                    .master
                    .filter(|&m| world.get_entity(m).is_ok())
                    .filter(|&m| {
                        world.get::<TilePos>(m).map(|p| p.z)
                            == crate::explosions::turf_of(world, pai).map(|p| p.z)
                    })
                    .ok_or("no master detected!")
            };
            match target {
                Ok(target) => {
                    if let Some(message) = crate::health_scan::healthscan(world, target) {
                        world.write_message(ss13_net::chat::server::SendChat {
                            clients: vec![client],
                            message,
                        });
                    }
                }
                Err(why) => tell(world, client, Span::Warning, why),
            }
        }
        SoftAct::Translator => {
            if let Some(mut h) = world.get_mut::<ss13_chat::language::LanguageHolder>(pai) {
                h.grant_all(ss13_chat::language::LanguageSource::Atom);
            }
            with_pai(world, pai, |p| p.languages_granted = true);
        }
        SoftAct::Encryption => {
            let mut on = false;
            with_pai(world, pai, |p| {
                p.encrypt_mod = !p.encrypt_mod;
                on = p.encrypt_mod;
            });
            let text = format!(
                "radio frequencies {}",
                if on { "enabled" } else { "disabled" }
            );
            tell(world, client, Span::Notice, text);
        }
        SoftAct::CableExtend => extend_cable(world, pai),
        SoftAct::CableHack => {
            hack_door(world, client, pai);
        }
        SoftAct::CableRetract => {
            retract_cable(world, pai);
            around(world, pai, Span::Notice, "The cable retracts into the pAI.");
        }
        SoftAct::CheckDna => check_dna(world, client, pai),
        SoftAct::Missing(name) => tell(
            world,
            client,
            Span::Warning,
            format!("{name}: no device for this program yet."),
        ),
    }
}

// ---- Door jack -------------------------------------------------------------------------------

/// tg `extend_cable`: into the carrier's hand, else on the floor.
pub fn extend_cable(world: &mut World, pai: Entity) {
    retract_cable(world, pai);
    let Some(state) = pai_of(world, pai) else {
        return;
    };
    let Some(pos) = crate::explosions::turf_of(world, pai) else {
        return;
    };
    let cable = crate::construction::spawn_detached(world, PAI_CABLE).unwrap_or_else(|| {
        world
            .spawn((
                DmItem {
                    path: PAI_CABLE.into(),
                    ..default()
                },
                Name::new("data cable"),
            ))
            .id()
    });
    crate::storage::take_out(world, cable, pos);
    world.entity_mut(cable).insert(PaiCable { pai, door: None });
    with_pai(world, pai, |p| p.cable = Some(cable));
    let name = name_of(world, pai);
    if let Some(hacker) = holder(world, state.card)
        && let Some(inventory) = world.get::<Inventory>(hacker).cloned()
        && inventory.hands.iter().any(Option::is_none)
    {
        crate::storage::put_in_hands(world, hacker, &inventory, cable);
        let hacker_name = name_of(world, hacker);
        around(
            world,
            hacker,
            Span::Notice,
            &format!(
                "A port on {name} opens to reveal a cable, which {hacker_name} quickly grabs."
            ),
        );
        return;
    }
    around(
        world,
        cable,
        Span::Notice,
        &format!("A port on {name} opens to reveal a cable, which promptly falls to the floor."),
    );
}

pub fn retract_cable(world: &mut World, pai: Entity) {
    let Some(cable) = pai_of(world, pai).and_then(|p| p.cable) else {
        return;
    };
    with_pai(world, pai, |p| p.cable = None);
    // Out of a hand first, so the inventory forgets it.
    if let Some(mob) = world.get::<CarriedBy>(cable).map(|c| c.0)
        && let Some(mut inventory) = world.get_mut::<Inventory>(mob)
    {
        for hand in inventory.hands.iter_mut() {
            if *hand == Some(cable) {
                *hand = None;
            }
        }
    }
    world.try_despawn(cable).ok();
}

/// tg `pai_cable/plugin` from the airlock's `item_interaction`.
pub fn cable_interaction(
    world: &mut World,
    client: Entity,
    user: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<PaiCable>(held).is_none() {
        return false;
    }
    if world
        .get::<crate::doors::Door>(target)
        .is_none_or(|d| d.kind != ss13_mapload::DoorKind::Airlock)
    {
        return false;
    }
    plug_in(world, client, user, held, target);
    true
}

pub fn plug_in(world: &mut World, client: Entity, user: Entity, cable: Entity, door: Entity) {
    if let Some(mut inventory) = world.get_mut::<Inventory>(user) {
        for hand in inventory.hands.iter_mut() {
            if *hand == Some(cable) {
                *hand = None;
            }
        }
    }
    crate::storage::put_inside(world, cable, door);
    if let Some(mut c) = world.get_mut::<PaiCable>(cable) {
        c.door = Some(door);
    }
    let (name, door_name) = (name_of(world, cable), name_of(world, door));
    tell(
        world,
        client,
        Span::Notice,
        format!("You insert {name} into a data port on {door_name}."),
    );
}

/// tg `hack_door`: 15 s on the plugged airlock.
pub fn hack_door(world: &mut World, client: Entity, pai: Entity) -> bool {
    let Some(cable) = pai_of(world, pai).and_then(|p| p.cable) else {
        return false;
    };
    let Some(door) = world.get::<PaiCable>(cable).and_then(|c| c.door) else {
        tell(world, client, Span::Warning, "nothing connected");
        return false;
    };
    sound(world, pai, PRYING, 50.0);
    tell(world, client, Span::Notice, "overriding...");
    crate::tools::do_after(
        world,
        Actor {
            client,
            mob: pai,
            item: None,
        },
        door,
        HACK_DS,
        0,
        crate::construction::Job::Pai(PaiJob::Hack { cable }),
    )
}

/// The hack's end (`crate::construction::complete`): the same cable still in the same door.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: PaiJob) {
    let PaiJob::Hack { cable } = job;
    let door = action.target;
    if pai_of(world, mob).and_then(|p| p.cable) != Some(cable)
        || world.get::<PaiCable>(cable).and_then(|c| c.door) != Some(door)
    {
        return;
    }
    tell(world, action.client, Span::Notice, "success");
    crate::doors::with_door(world, door, |d, appearance, ctx| {
        let moved = ctx.open(d, appearance, false);
        (moved, moved)
    });
    retract_cable(world, mob);
}

/// tg `handle_move`: a cable farther than [`CABLE_LENGTH`] from its pAI retracts.
// Cost: one pass over the extended cables (a handful) per tick.
fn cable_reach(world: &mut World) {
    let cables: Vec<(Entity, Entity)> = world
        .query::<(Entity, &PaiCable)>()
        .iter(world)
        .map(|(e, c)| (e, c.pai))
        .collect();
    for (cable, pai) in cables {
        let far = match (
            crate::explosions::turf_of(world, cable),
            crate::explosions::turf_of(world, pai),
        ) {
            (Some(a), Some(b)) => {
                a.z != b.z || a.x.abs_diff(b.x).max(a.y.abs_diff(b.y)) > CABLE_LENGTH
            }
            _ => true,
        };
        if far {
            if pai_of(world, pai).and_then(|p| p.cable) == Some(cable) {
                tell_mob(world, pai, Span::Notice, "cable retracted");
                retract_cable(world, pai);
            } else {
                world.try_despawn(cable).ok();
            }
        }
    }
}

// ---- Ghosts and prompts ----------------------------------------------------------------------

fn ghost_clicks(world: &mut World, mut clicks: Local<MessageCursor<GhostClick>>) {
    let Some(messages) = world.get_resource::<Messages<GhostClick>>() else {
        return;
    };
    let pending: Vec<GhostClick> = clicks.read(messages).copied().collect();
    for click in pending {
        // A card, or someone carrying a used one (a card in a hand is not on the map).
        let cards: Vec<Entity> = world
            .query_filtered::<Entity, With<PaiCard>>()
            .iter(world)
            .collect();
        let carrier = cards
            .into_iter()
            .any(|card| carries(world, click.target, card));
        if is_card(world, click.target) || carrier {
            start_submit(world, click.client, click.ghost);
        }
    }
}

/// tg `SSpai.recruit_window`: the candidacy, from its last submission.
pub fn start_submit(world: &mut World, client: Entity, ghost: Entity) {
    if world.get::<Ghost>(ghost).is_none() {
        return;
    }
    let draft = world
        .get_resource::<PaiCandidates>()
        .and_then(|c| c.0.iter().find(|c| c.client == client).cloned())
        .unwrap_or_else(|| Candidate {
            client,
            name: String::new(),
            description: String::new(),
            comments: String::new(),
        });
    ask_submit(world, client, ghost, 0, draft);
}

fn ask_submit(world: &mut World, client: Entity, ghost: Entity, step: u8, draft: Candidate) {
    let (message, default, max_length, multiline) = match step {
        0 => ("Name", draft.name.clone(), MAX_NAME_LEN, false),
        1 => (
            "Description: what your pAI is like",
            draft.description.clone(),
            MAX_BROADCAST_LEN,
            true,
        ),
        _ => (
            "OOC comments",
            draft.comments.clone(),
            MAX_BROADCAST_LEN,
            true,
        ),
    };
    ask(
        world,
        Ask::new(
            client,
            ghost,
            "pAI Candidacy",
            message,
            PromptKind::Text {
                default,
                max_length,
                multiline,
            },
        ),
        PaiPrompt::Submit { step, draft },
    );
}

/// tg `ui_act("submit")`: the candidate is ready; the cards flash (tg `alert_update`).
pub fn submit(world: &mut World, mut candidate: Candidate) {
    let unknown = |s: &str| {
        let s = s.trim();
        if s.is_empty() {
            "Unknown".to_owned()
        } else {
            s.to_owned()
        }
    };
    candidate.name = unknown(&candidate.name);
    candidate.description = unknown(&candidate.description);
    candidate.comments = unknown(&candidate.comments);
    let client = candidate.client;
    {
        let mut all = world.resource_mut::<PaiCandidates>();
        all.0.retain(|c| c.client != client);
        all.0.push(candidate);
    }
    tell(
        world,
        client,
        Span::Notice,
        "Your pAI candidacy has been submitted!",
    );
    let cards: Vec<Entity> = world
        .query::<(Entity, &PaiCard)>()
        .iter(world)
        .filter(|(_, c)| c.pai.is_none())
        .map(|(e, _)| e)
        .collect();
    for card in cards {
        sound(world, card, PING, 30.0);
        let name = name_of(world, card);
        around(
            world,
            card,
            Span::Notice,
            &format!(
                "{name} flashes a message across its screen: New personalities available for \
                 download!"
            ),
        );
    }
}

fn answered(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in pending {
        let Some(asked) = world.get::<PaiPrompt>(prompt).cloned() else {
            continue;
        };
        match (asked, answer) {
            (PaiPrompt::CardMenu { card, user, acts }, Some(PromptAnswer::Item(i))) => {
                if let Some(&act) = acts.get(i) {
                    card_act(world, client, user, card, act);
                }
            }
            (PaiPrompt::ConfirmWipe { card, user }, Some(PromptAnswer::Button(0))) => {
                if carries(world, user, card)
                    && let Some(pai) = world.get::<PaiCard>(card).and_then(|c| c.pai)
                {
                    wipe(world, client, pai);
                }
            }
            (PaiPrompt::SetLaws { card, user }, Some(PromptAnswer::Text(text))) => {
                if carries(world, user, card)
                    && let Some(pai) = world.get::<PaiCard>(card).and_then(|c| c.pai)
                {
                    set_directive(world, pai, &text);
                }
            }
            (PaiPrompt::Submit { step, mut draft }, Some(PromptAnswer::Text(text))) => {
                // Still this client's ghost.
                if world.get::<Ghost>(actor).is_none()
                    || world.get_resource::<Players>().and_then(|p| p.mob(client)) != Some(actor)
                {
                    continue;
                }
                match step {
                    0 => draft.name = text,
                    1 => draft.description = text,
                    _ => draft.comments = text,
                }
                if step < 2 {
                    ask_submit(world, client, actor, step + 1, draft);
                } else {
                    submit(world, draft);
                }
            }
            (PaiPrompt::Software { pai, acts }, Some(PromptAnswer::Item(i))) => {
                if actor == pai
                    && world.get::<Stat>(pai) == Some(&Stat::Conscious)
                    && let Some(&act) = acts.get(i)
                {
                    soft_act(world, client, pai, act);
                }
            }
            (PaiPrompt::Chassis { pai }, Some(PromptAnswer::Item(i))) => {
                if actor == pai
                    && let Some(&chassis) = CHASSIS.get(i)
                {
                    with_pai(world, pai, |p| p.chassis = chassis);
                    refresh_look(world, pai);
                    tell(
                        world,
                        client,
                        Span::Notice,
                        format!("{chassis} composite engaged"),
                    );
                }
            }
            (PaiPrompt::DnaConsent { pai, holder }, answer) if actor == holder => {
                dna_consent(world, pai, holder, answer == Some(PromptAnswer::Button(0)));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::MoveQueue;

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<Players>();
        world.init_resource::<PaiCandidates>();
        world
    }

    fn advance(world: &mut World, secs: f64) {
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f64(secs));
    }

    fn card_at(world: &mut World, pos: TilePos) -> Entity {
        world
            .spawn((
                DmItem {
                    path: PAI_CARD.into(),
                    ..default()
                },
                pos,
            ))
            .id()
    }

    fn ghost(world: &mut World, pos: TilePos) -> (Entity, Entity) {
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
        (client, ghost)
    }

    fn pai_in(world: &mut World, card: Entity) -> Entity {
        let mob = world.spawn_empty().id();
        install(world, card, mob, "Helper");
        mob
    }

    #[test]
    fn a_ghost_candidate_is_downloaded_into_the_card() {
        let mut world = world();
        let pos = TilePos::new(2, 2, 1);
        let card = card_at(&mut world, pos);
        let user = world.spawn(pos).id();
        let (client, _) = ghost(&mut world, pos);
        // A request, then a second one too soon.
        card_act_direct(&mut world, user, card, CardAct::Request);
        let until = world.get::<PaiCard>(card).unwrap().request_until;
        assert_eq!(until, REQUEST_COOLDOWN_SECS);
        submit(
            &mut world,
            Candidate {
                client,
                name: "Buddy".into(),
                description: " ".into(),
                comments: "hi".into(),
            },
        );
        let c = ready_candidates(&mut world);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].description, "Unknown");
        assert!(world.get::<PaiCard>(card).unwrap().pai.is_none());
        // `download` itself spawns from the prototype; here the mob is installed by hand and
        // the candidate's ghost possesses it as `download` does.
        let mob = pai_in(&mut world, card);
        super::super::attach_client(&mut world, client, mob, pos);
        assert_eq!(world.get::<PaiCard>(card).unwrap().pai, Some(mob));
        assert_eq!(world.resource::<Players>().mob(client), Some(mob));
        // No longer a ghost: no longer a ready candidate.
        assert!(ready_candidates(&mut world).is_empty());
        let laws = world.get::<Laws>(mob).unwrap();
        assert_eq!(laws.zeroth.as_deref(), Some(ZEROTH));
        // Wiped: the card is free, the player a ghost.
        despawn_pai(&mut world, mob);
        assert!(world.get_entity(mob).is_err());
        assert!(world.get::<PaiCard>(card).unwrap().pai.is_none());
    }

    fn card_act_direct(world: &mut World, user: Entity, card: Entity, act: CardAct) {
        let client = world.spawn_empty().id();
        if world.get::<PaiCard>(card).is_none() {
            world.entity_mut(card).insert(PaiCard::default());
        }
        world.entity_mut(card).insert(CarriedBy(user));
        card_act(world, client, user, card, act);
    }

    #[test]
    fn the_holochassis_folds_out_follows_the_leash_and_folds_on_damage() {
        let mut world = world();
        let pos = TilePos::new(5, 5, 1);
        let card = card_at(&mut world, pos);
        let pai = pai_in(&mut world, card);
        // Folded: hidden, immobile, on the card's turf.
        assert!(world.get::<Networked>(pai).is_none());
        assert!(world.get::<Mover>(pai).is_none());
        // Too early after the download.
        assert!(!fold_out(&mut world, pai, false));
        advance(&mut world, HOLO_INIT_SECS);
        assert!(fold_out(&mut world, pai, false));
        assert!(world.get::<Networked>(pai).is_some());
        assert!(world.get::<Mover>(pai).is_some());
        assert_eq!(world.get::<TilePos>(pai), Some(&pos));
        // Strays beyond the leash: back on the card.
        world.entity_mut(pai).insert(TilePos::new(5 + 10, 5, 1));
        follow_cards(&mut world);
        assert_eq!(world.get::<TilePos>(pai), Some(&pos));
        // Within the leash it stays.
        world.entity_mut(pai).insert(TilePos::new(8, 5, 1));
        follow_cards(&mut world);
        assert_eq!(world.get::<TilePos>(pai), Some(&TilePos::new(8, 5, 1)));
        // 25 damage breaks the 20-health holochassis: folded, a longer recharge.
        take_holo_damage(&mut world, pai, 25.0);
        let state = world.get::<Pai>(pai).unwrap().clone();
        assert!(!state.holoform);
        assert!(world.get::<Networked>(pai).is_none());
        assert_eq!(state.ready_at, HOLO_INIT_SECS + HOLO_OVERLOAD_SECS);
        follow_cards(&mut world);
        assert_eq!(world.get::<TilePos>(pai), Some(&pos));
        // Below 0 it may not unfold.
        advance(&mut world, HOLO_OVERLOAD_SECS);
        assert!(!fold_out(&mut world, pai, false));
        // The card gone: the pAI too.
        world.entity_mut(card).despawn();
        follow_cards(&mut world);
        assert!(world.get_entity(pai).is_err());
    }

    #[test]
    fn software_costs_ram_once() {
        let mut world = world();
        let card = card_at(&mut world, TilePos::new(1, 1, 1));
        let pai = pai_in(&mut world, card);
        assert!(buy_software(&mut world, pai, "Door Jack"));
        assert!(!buy_software(&mut world, pai, "Door Jack"));
        assert!(buy_software(&mut world, pai, "Universal Translator"));
        assert!(buy_software(&mut world, pai, "Host Scan"));
        // 100 - 35 - 35 - 20 = 10 left: 20 more is too dear, 10 fits.
        assert!(!buy_software(&mut world, pai, "Medical HUD"));
        assert!(buy_software(&mut world, pai, "Encryption Slot"));
        assert!(!buy_software(&mut world, pai, "Nonsense"));
        let state = world.get::<Pai>(pai).unwrap();
        assert_eq!(state.ram, 0);
        assert_eq!(state.installed.len(), 4);
    }

    #[test]
    fn the_door_jack_cable_plugs_in_and_retracts_when_far() {
        let mut world = world();
        let pos = TilePos::new(3, 3, 1);
        let card = card_at(&mut world, pos);
        let pai = pai_in(&mut world, card);
        buy_software(&mut world, pai, "Door Jack");
        world.entity_mut(pai).insert(pos);
        extend_cable(&mut world, pai);
        let cable = world.get::<Pai>(pai).unwrap().cable.expect("a cable");
        assert_eq!(world.get::<TilePos>(cable), Some(&pos));
        let door = world.spawn(TilePos::new(4, 3, 1)).id();
        let user = world.spawn(pos).id();
        let client = world.spawn_empty().id();
        plug_in(&mut world, client, user, cable, door);
        assert_eq!(world.get::<PaiCable>(cable).unwrap().door, Some(door));
        assert_eq!(world.get::<StoredIn>(cable).map(|s| s.0), Some(door));
        // Within 2 tiles: stays.
        cable_reach(&mut world);
        assert!(world.get_entity(cable).is_ok());
        // The card (and folded pAI) carried 5 tiles away: retracted.
        world.entity_mut(card).insert(TilePos::new(9, 3, 1));
        follow_cards(&mut world);
        cable_reach(&mut world);
        assert!(world.get_entity(cable).is_err());
        assert!(world.get::<Pai>(pai).unwrap().cable.is_none());
        // Nothing to hack without a cable.
        assert!(!hack_door(&mut world, client, pai));
    }

    #[test]
    fn the_master_sets_the_directive_and_an_emag_takes_over() {
        let mut world = world();
        let card = card_at(&mut world, TilePos::new(1, 1, 1));
        let pai = pai_in(&mut world, card);
        let client = world.spawn_empty().id();
        // No master: no directive.
        assert!(!set_directive(&mut world, pai, "Be nice."));
        // A mob without DNA is no master.
        let robot = world.spawn_empty().id();
        assert!(!set_dna(&mut world, client, robot, pai));
        let master = world
            .spawn((
                Name::new("Иван"),
                crate::genetics::Dna {
                    unique_enzymes: "abc".into(),
                    unique_identity: String::new(),
                    mutation_index: Vec::new(),
                    default_mutation_genes: Vec::new(),
                    mutations: Vec::new(),
                    stability: 100,
                    melt_until: None,
                },
            ))
            .id();
        assert!(set_dna(&mut world, client, master, pai));
        let state = world.get::<Pai>(pai).unwrap().clone();
        assert_eq!(state.master, Some(master));
        assert_eq!(state.master_dna.as_deref(), Some("abc"));
        assert!(set_directive(&mut world, pai, "Be nice."));
        assert!(set_directive(&mut world, pai, "Be kind."));
        let laws = world.get::<Laws>(pai).unwrap().clone();
        assert_eq!(laws.inherent, vec![("Be kind.".to_owned(), false)]);
        // The emag on the card.
        assert!(takes_emag(&world, card));
        let traitor = world.spawn_empty().id();
        assert!(emag_act(&mut world, client, traitor, card));
        let state = world.get::<Pai>(pai).unwrap().clone();
        assert!(state.emagged);
        assert_eq!(state.master, Some(traitor));
        assert_eq!(state.master_name.as_deref(), Some("The Syndicate"));
        let laws = world.get::<Laws>(pai).unwrap().clone();
        assert_eq!(laws.inherent, vec![(SYNDICATE_LAW.to_owned(), false)]);
        // Emagged: no new DNA; a reset clears it all.
        assert!(!set_dna(&mut world, client, master, pai));
        assert!(reset_software(&mut world, pai));
        let state = world.get::<Pai>(pai).unwrap().clone();
        assert!(!state.emagged && state.master.is_none());
        assert!(world.get::<Laws>(pai).unwrap().inherent.is_empty());
    }
}
