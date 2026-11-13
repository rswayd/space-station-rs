//! Maintenance drones (server), after tg `/mob/living/basic/drone`
//! (`code/modules/mob/living/basic/drone/`).
//!
//! - A drone is a basic mob (`crate::mobs`) with hands, an internal storage and a head slot
//!   (`crate::dextrous`, HUD `HudLayout::Drone`), all station access (`simple_access`, in
//!   `crate::inventory::mob_access`), a ventcrawler (`crate::ventcrawl::ALWAYS_ROOTS`). Its type
//!   ([`Kind`]) sets its laws, its default storage (the built-in tools, an uplink, a duffel bag)
//!   and hat, and whether it is shy: a shy drone is a pacifist (`TRAIT_PACIFISM`, kept through
//!   `crate::medical::traumas::refresh`).
//! - Laws: the type's, else `<SS13_CONFIG_DIR>/drone_laws.txt` (numbered lines, `#` comments),
//!   else tg's three. "Check Laws" and "Drone ping" are its verbs (the stat panel's "Drone" tab).
//! - `:b` is the drone chat: living drones of the same factions and ghosts read it; robots do not
//!   (`super::binary_chat` skips drones). Drones come online and die with a network line.
//! - A shell (`/obj/effect/mob_spawn/ghost_role/drone`, a ghost role of `crate::ghost_roles`)
//!   becomes its `mob_type` for the ghost that takes it ([`take_shell`]): "drone (N)", the role's
//!   texts, the laws and flavor text, then the look radial (maintenance in six colors, repair,
//!   scout) unless the type has its own.
//! - The built-in tools come back to their storage when dropped or put elsewhere
//!   (`holderloving`); the storage itself is stuck to the drone (`TRAIT_NODROP`).
//! - Death drops the internal storage (unless stuck) and the hat; a drone's empty hand on a dead
//!   drone offers "Reactivate" (3 s; its player, alive or a ghost, is needed) or "Cannibalize"
//!   (6 s: the parts heal the user's brute by the dead drone's max health). A screwdriver
//!   tightens a living drone's screws (8 s, full brute heal); a wrench resets a hacked drone.
//! - Derelict drones are gibbed when they leave their z-level (`stationstuck`).
// shortcut: no alarm listener, built-in camera, diagnostic HUD, EMP stun, shock immunity, holiday
// hats, being picked up (`can_be_held`), crafting, the drone playtime requirement
// (`allow_spawn`) or the badass syndrone's weapons implant; nothing hacks a drone in tg either
// (only the wrench's reset exists). Drones' melee on carbons goes through `crate::combat`, which
// basic mobs do not reach yet.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_chat::server::{SaySystems, Spoke};
use ss13_chat::speech::{BinaryCapable, RadioTarget};
use ss13_mapload::fidelity::is_under;
use ss13_mapload::mob_spawn::{DRONE_SHELL_ROOT, MobSpawnDef};
use ss13_mapload::mobs::MobDef;
use ss13_mapload::{DmItem, IconIndex};
use ss13_net::chat::server::SendChat;
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::InventorySlot;
use ss13_tgui::prompt::{
    Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, RadialChoice, ask_world,
};
use ss13_tgui::server::{UiActed, UiSystems};

use crate::click::{ClickResolved, ResolveClicks};
use crate::click_handlers::{AddClickHandler, ClickPhase};
use crate::construction::Job;
use crate::dextrous::Dextrous;
use crate::ghost::Ghost;
use crate::health::{DamageType, Health, Stat};
use crate::hud::HudLayout;
use crate::inventory::{CarriedBy, Inventory, NoDrop, no_drop, put_down};
use crate::mobs::{BasicMob, Faction};
use crate::server::PlayerMob;
use crate::storage::StoredIn;
use crate::tools::{Actor, DoAfter, ToolKind, tell, tell_mob, tool_of, use_tool};

pub const DRONE: &str = "/mob/living/basic/drone";
const SYNDRONE: &str = "/mob/living/basic/drone/syndrone";
const BADASS: &str = "/mob/living/basic/drone/syndrone/badass";
const SNOWFLAKE: &str = "/mob/living/basic/drone/snowflake";
const POLYMORPHED: &str = "/mob/living/basic/drone/polymorphed";
const CLASSIC: &str = "/mob/living/basic/drone/classic";
const DERELICT: &str = "/mob/living/basic/drone/derelict";
const DRONE_TOOLS: &str = "/obj/item/storage/drone_tools";
const DRONE_DMI: &str = "icons/mob/silicon/drone.dmi";
const OIL: &str = "/obj/effect/decal/cleanable/blood/splatter/oil";
/// `MAINTDRONE`, `REPAIRDRONE`, `SCOUTDRONE`.
const MAINTDRONE: &str = "drone_maint";
const REPAIRDRONE: &str = "drone_repair";
const SCOUTDRONE: &str = "drone_scout";
const COLORS: [&str; 6] = ["blue", "green", "grey", "orange", "pink", "red"];
/// The stat panel's verb category and verbs (`GAME_VERB(..., "Drone")`).
pub const VERB_TAB: &str = "Drone";
pub const VERB_LAWS: &str = "Check Laws";
pub const VERB_PING: &str = "Drone ping";
const PING_LEVELS: [&str; 4] = ["Low", "Medium", "High", "Critical"];
/// The tgui alert of `attack_drone`.
const REACTIVATE: &str = "Reactivate";
const CANNIBALIZE: &str = "Cannibalize";

/// `/mob/living/basic/drone::laws`.
pub const BASE_LAWS: &str = "1. You may not involve yourself in the matters of another being, \
even if such matters conflict with Law Two or Law Three, unless the other being is another \
Drone.\n2. You may not harm any being, regardless of intent or circumstance.\n3. Your goals are \
to actively build, maintain, repair, improve, and provide power to the best of your abilities \
within the facility that housed your activation.";
const SYNDRONE_LAWS: &str = "1. Interfere.\n2. Kill.\n3. Destroy.";
const DERELICT_LAWS: &str = "1. You may not involve yourself in the matters of another sentient \
being outside the station that housed your activation, even if such matters conflict with Law \
Two or Law Three, unless the other being is another Drone.\n2. You may not harm any sentient \
being, regardless of intent or circumstance.\n3. Your goals are to actively build, maintain, \
repair, improve, and provide power to the best of your abilities within the facility that \
housed your activation.";
/// `liberate()`.
const FREE_LAWS: &str = "1. You are a Free Drone.";

const FLAVOR: &str = "DO NOT INTERFERE WITH THE ROUND AS A DRONE OR YOU WILL BE DRONE BANNED\n\
Drones are a ghost role that are allowed to fix the station and build things. Interfering with \
the round as a drone is against the rules.\n\
Actions that constitute interference include, but are not limited to:\n\
     - Interacting with round critical objects (IDs, weapons, contraband, powersinks, bombs, etc.)\n\
     - Interacting with living beings (communication, attacking, healing, etc.)\n\
     - Interacting with non-living beings (dragging bodies, looting bodies, etc.)\n\
These rules are at admin discretion and will be heavily enforced.\n\
If you do not have the regular drone laws, follow your laws to the best of your ability.\n\
Prefix your message with :b to speak in Drone Chat.";
const DERELICT_FLAVOR: &str = "DO NOT WILLINGLY LEAVE KOSMICHESKAYA STANTSIYA 13 (THE DERELICT)\n\
Derelict drones are a ghost role that is allowed to roam freely on KS13, with the main goal of \
repairing and improving it.\n\
Do not interfere with the round going on outside KS13.\n\
Actions that constitute interference include, but are not limited to:\n\
     - Going to the main station in search of materials.\n\
     - Interacting with non-drone players outside KS13, dead or alive.\n\
These rules are at admin discretion and will be heavily enforced.\n\
If you do not have the regular drone laws, follow your laws to the best of your ability.";
const DERELICT_ESCAPE: &str = "01000110 01010101 01000011 01001011 00100000 01011001 01001111 \
01010101\nWARNING: Dereliction of KS13 detected. Self-destruct activated.";

/// The vars of a drone type (`extra_drone_types.dm`).
struct Kind {
    root: &'static str,
    /// `default_storage`, `default_headwear`.
    storage: Option<&'static str>,
    hat: Option<&'static str>,
    /// `None`: the default laws ([`DroneLaws`]).
    laws: Option<&'static str>,
    shy: bool,
    hacked: bool,
    can_unhack: bool,
    picked: bool,
    flavor: Option<&'static str>,
    /// The uplink in the storage: telecrystals, flag.
    uplink: Option<(u32, u32)>,
}

const fn kind(root: &'static str) -> Kind {
    Kind {
        root,
        storage: Some(DRONE_TOOLS),
        hat: None,
        laws: None,
        shy: true,
        hacked: false,
        can_unhack: true,
        picked: false,
        flavor: Some(FLAVOR),
        uplink: None,
    }
}

/// Most specific first.
static KINDS: [Kind; 7] = [
    Kind {
        storage: Some("/obj/item/uplink/nuclear"),
        hat: Some("/obj/item/clothing/head/helmet/swat"),
        laws: Some(SYNDRONE_LAWS),
        shy: false,
        hacked: true,
        can_unhack: false,
        picked: true,
        flavor: None,
        uplink: Some((30, ss13_mapload::antag::UPLINK_NUKE_OPS)),
        ..kind(BADASS)
    },
    Kind {
        storage: Some("/obj/item/uplink"),
        hat: Some("/obj/item/clothing/head/helmet/swat"),
        laws: Some(SYNDRONE_LAWS),
        shy: false,
        hacked: true,
        can_unhack: false,
        picked: true,
        flavor: None,
        uplink: Some((10, ss13_mapload::antag::UPLINK_TRAITORS)),
        ..kind(SYNDRONE)
    },
    Kind {
        hat: Some("/obj/item/clothing/head/chameleon/drone"),
        ..kind(SNOWFLAKE)
    },
    Kind {
        storage: None,
        laws: Some(FREE_LAWS),
        shy: false,
        picked: true,
        flavor: None,
        ..kind(POLYMORPHED)
    },
    Kind {
        storage: Some("/obj/item/storage/backpack/duffelbag/drone"),
        shy: false,
        ..kind(CLASSIC)
    },
    Kind {
        hat: Some("/obj/item/clothing/head/costume/ushanka"),
        laws: Some(DERELICT_LAWS),
        shy: false,
        flavor: Some(DERELICT_FLAVOR),
        ..kind(DERELICT)
    },
    kind(DRONE),
];

fn kind_of(path: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|k| is_under(path, k.root))
}

/// A maintenance drone.
#[derive(Component, Clone, Debug)]
pub struct Drone {
    pub laws: String,
    pub hacked: bool,
    pub can_unhack: bool,
    /// tg `shy`: `TRAIT_PACIFISM`, no stripping.
    pub shy: bool,
    /// The look is chosen (`picked`).
    pub picked: bool,
    flavor: Option<&'static str>,
    /// `visualAppearance`.
    visual: &'static str,
    /// The type's `shy` and laws, for a reset.
    initial_shy: bool,
}

/// On a derelict drone: `stationstuck` to the z-level it was made on.
#[derive(Component, Clone, Copy, Debug)]
struct StationStuck(u16);

/// On a built-in tool: the storage it belongs in (`/datum/component/holderloving`).
#[derive(Component, Clone, Copy, Debug)]
pub struct HolderLoving(pub Entity);

/// The default laws of a regular drone (`get_default_laws`).
#[derive(Resource, Clone, Debug)]
pub struct DroneLaws(pub String);

impl Default for DroneLaws {
    fn default() -> Self {
        Self(BASE_LAWS.to_owned())
    }
}

/// tg `get_default_laws` over `drone_laws.txt`: every non-empty line not starting with `#` is
/// the next numbered law; `None` without any.
pub fn parse_laws(text: &str) -> Option<String> {
    let lines: Vec<String> = text
        .lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .enumerate()
        .map(|(i, l)| format!("{}. {l}", i + 1))
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// What a drone prompt asks.
#[derive(Component, Clone, Copy, Debug)]
enum DronePrompt {
    /// `pickVisualAppearance`: the kind.
    Visual { drone: Entity },
    /// The maintenance drone's color.
    Color { drone: Entity },
    /// `attack_drone`: reactivate or cannibalize `target`.
    Interact { drone: Entity, target: Entity },
    /// `drone_ping`'s severity.
    Ping { drone: Entity },
}

/// The `do_after`s of this module (`crate::construction::Job::Drone`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DroneJob {
    Repair,
    Reset,
    Reactivate,
    Cannibalize,
}

pub struct DronesPlugin;

impl Plugin for DronesPlugin {
    fn build(&self, app: &mut App) {
        // tg `/obj/item/uplink/interact` from the hand: an item uplink (not a PDA's or a
        // radio's, which lock) opens.
        app.add_click_handler(ClickPhase::AttackSelf, 1013, |w, c| {
            open_item_uplink(w, c.client, c.mob, c.target)
        });
        app.init_resource::<DroneLaws>()
            .add_systems(Startup, load_laws)
            .add_systems(
                Update,
                (
                    drone_clicks
                        .after(crate::security::security_clicks)
                        .before(crate::combat::MeleeSystems)
                        .after(ResolveClicks),
                    drone_chat.after(SaySystems),
                    answers.after(PromptSystems::Answer).before(UiSystems::Open),
                    verbs.in_set(UiSystems::Handle),
                    (on_death, holder_loving, station_stuck),
                ),
            );
    }
}

fn load_laws(mut laws: ResMut<DroneLaws>) {
    let dir = std::env::var("SS13_CONFIG_DIR")
        .ok()
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "config".to_owned());
    let path = std::path::Path::new(&dir).join("drone_laws.txt");
    if let Some(parsed) = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| parse_laws(&t))
    {
        laws.0 = parsed;
    }
}

/// `/mob/living/basic/drone/Initialize` for a basic mob of a drone type (`crate::mobs::attach`).
pub(crate) fn attach(world: &mut World, mob: Entity, def: &MobDef) {
    let Some(kind) = kind_of(&def.path) else {
        return;
    };
    let laws = kind.laws.map_or_else(
        || {
            world
                .get_resource::<DroneLaws>()
                .map_or_else(|| BASE_LAWS.to_owned(), |l| l.0.clone())
        },
        str::to_owned,
    );
    world.entity_mut(mob).insert((
        Drone {
            laws,
            hacked: kind.hacked,
            can_unhack: kind.can_unhack,
            shy: kind.shy,
            picked: kind.picked,
            flavor: kind.flavor,
            visual: MAINTDRONE,
            initial_shy: kind.shy,
        },
        Dextrous {
            layout: HudLayout::Drone,
            inhand_y: 0,
        },
        Inventory::default(),
        BinaryCapable,
    ));
    set_shy(world, mob, kind.shy);
    if let Some(path) = kind.storage
        && let Some(storage) = equip_new(world, mob, path, InventorySlot::SuitStorage)
    {
        if path == DRONE_TOOLS {
            built_in_tools(world, storage);
        }
        if let Some((tc, flag)) = kind.uplink {
            crate::antag::uplink::give_item_uplink(world, mob, storage, tc, flag, "Syndrone");
        }
    }
    if let Some(hat) = kind.hat {
        equip_new(world, mob, hat, InventorySlot::Head);
    }
    if kind.root == POLYMORPHED {
        let roll = world.resource_mut::<crate::combat::CombatRng>().range(0, 2);
        let visual = [MAINTDRONE, REPAIRDRONE, SCOUTDRONE][roll as usize];
        let color = if visual == MAINTDRONE {
            let c = world.resource_mut::<crate::combat::CombatRng>().range(0, 5);
            Some(COLORS[c as usize])
        } else {
            None
        };
        set_visual(world, mob, visual, color);
    }
    if kind.root == DERELICT
        && let Some(pos) = world.get::<TilePos>(mob).copied()
    {
        world.entity_mut(mob).insert(StationStuck(pos.z));
    }
    let name = crate::security::name_of(world, mob);
    alert_drones(
        world,
        mob,
        Span::Notice,
        &format!("DRONE NETWORK: {name} connected."),
    );
}

/// A new `path` into the drone's empty `slot` (`equip_to_slot_or_del`).
fn equip_new(world: &mut World, mob: Entity, path: &str, slot: InventorySlot) -> Option<Entity> {
    let item = crate::construction::spawn_detached(world, path)?;
    world.entity_mut(item).insert((Networked, CarriedBy(mob)));
    if world.get::<Inventory>(mob).is_none() {
        world.despawn(item);
        return None;
    }
    *world
        .get_mut::<Inventory>(mob)
        .expect("checked above")
        .slot_mut(slot) = Some(item);
    Some(item)
}

/// `/obj/item/storage/drone_tools/Initialize` and `PopulateContents`: stuck to the drone, its
/// tools bound to it.
fn built_in_tools(world: &mut World, storage: Entity) {
    world.entity_mut(storage).insert(NoDrop);
    let tools: Vec<Entity> = world
        .get::<crate::storage::Stored>(storage)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    for tool in tools {
        world.entity_mut(tool).insert(HolderLoving(storage));
    }
}

/// `set_shy` -> `shy_update`: `TRAIT_PACIFISM` while shy (losing it, the other sources count
/// again).
fn set_shy(world: &mut World, mob: Entity, shy: bool) {
    if let Some(mut drone) = world.get_mut::<Drone>(mob) {
        drone.shy = shy;
    }
    if shy {
        world
            .entity_mut(mob)
            .insert(crate::medical::traumas::Pacifist);
    } else {
        crate::medical::traumas::refresh(world, mob);
    }
}

/// A shy drone's `TRAIT_PACIFISM` (`DRONE_SHY_TRAIT`), for `crate::medical::traumas::refresh`.
pub fn shy(world: &World, mob: Entity) -> bool {
    world.get::<Drone>(mob).is_some_and(|d| d.shy)
}

/// `visualAppearance` and the sprites it gives (`pickVisualAppearance`, `getItemPixelShiftY`).
fn set_visual(world: &mut World, mob: Entity, visual: &'static str, color: Option<&str>) {
    let living = match color {
        Some(color) => format!("{visual}_{color}"),
        None => visual.to_owned(),
    };
    if let Some(mut drone) = world.get_mut::<Drone>(mob) {
        drone.visual = visual;
        drone.picked = true;
    }
    if let Some(mut basic) = world.get_mut::<BasicMob>(mob) {
        basic.icon_living = living;
        basic.icon_dead = format!("{visual}_dead");
    }
    if let Some(mut dextrous) = world.get_mut::<Dextrous>(mob) {
        dextrous.inhand_y = if visual == MAINTDRONE { 0 } else { -6 };
    }
    let dead = world.get::<Stat>(mob) == Some(&Stat::Dead);
    crate::mobs::set_look(world, mob, dead);
    crate::inventory::refresh_look(world, mob);
}

/// The drones `_alert_drones` reaches from `source`: alive, of exactly its factions.
fn network(world: &mut World, source: Entity) -> Vec<Entity> {
    let factions = sorted_factions(world, source);
    let mut q = world.query_filtered::<(Entity, &Stat, &PlayerMob), With<Drone>>();
    let candidates: Vec<(Entity, Entity)> = q
        .iter(world)
        .filter(|(_, s, _)| **s != Stat::Dead)
        .map(|(e, _, p)| (e, p.client))
        .collect();
    candidates
        .into_iter()
        .filter(|&(e, _)| sorted_factions(world, e) == factions)
        .map(|(_, c)| c)
        .collect()
}

fn sorted_factions(world: &World, mob: Entity) -> Vec<String> {
    let mut f = world
        .get::<Faction>(mob)
        .map(|f| f.0.clone())
        .unwrap_or_default();
    f.sort();
    f.dedup();
    f
}

/// tg `alert_drones(msg)`: to every drone of the network.
fn alert_drones(world: &mut World, source: Entity, span: Span, text: &str) {
    let clients = network(world, source);
    if clients.is_empty() {
        return;
    }
    world.write_message(SendChat {
        clients,
        message: ChatMessage::simple(ChatChannel::Radio, span, text),
    });
}

/// tg `drone_chat` (the binary saymode of a drone): the network and every ghost read
/// "Drone Chat: Name says, "...""; nothing is said aloud.
fn drone_chat(world: &mut World, mut spoken: Local<MessageCursor<Spoke>>) {
    let said: Vec<(Entity, String, &'static str, String)> = spoken
        .read(world.resource::<Messages<Spoke>>())
        .filter(|s| s.radio == Some(RadioTarget::Binary))
        .map(|s| (s.speaker, s.name.clone(), s.verb, s.text.clone()))
        .collect();
    for (speaker, name, verb, text) in said {
        if world.get::<Drone>(speaker).is_none() {
            continue;
        }
        let mut clients = network(world, speaker);
        let mut ghosts = world.query_filtered::<&PlayerMob, With<Ghost>>();
        clients.extend(ghosts.iter(world).map(|p| p.client));
        clients.sort();
        clients.dedup();
        info!(target: "say", "drone chat {name}: {text}");
        world.write_message(SendChat {
            clients,
            message: drone_chat_message(&name, verb, &text),
        });
    }
}

/// `span_drone("Drone Chat: [span_name(name)] [message_part]")`.
pub fn drone_chat_message(name: &str, verb: &str, text: &str) -> ChatMessage {
    ChatMessage {
        channel: ChatChannel::Radio,
        segments: vec![
            ChatSegment::new(Span::BinarySay, "Drone Chat: "),
            ChatSegment::new(Span::Name, name),
            ChatSegment::new(Span::BinarySay, format!(" {verb}, \"{text}\"")),
        ],
    }
}

/// Whether `def` is a drone shell's.
pub fn is_shell(def: &MobSpawnDef) -> bool {
    is_under(&def.path, DRONE_SHELL_ROOT)
}

/// `new /obj/effect/mob_spawn/ghost_role/drone(loc)` after map load: the ghost role, the
/// ghosts told (`notify_ghosts`). `None` without the shell's data or prototype.
pub fn make_shell(world: &mut World, path: &str, pos: TilePos) -> Option<Entity> {
    let def = crate::ghost_roles::def(world, path)?;
    let shell = crate::construction::spawn_obj(world, path, pos)?;
    world
        .entity_mut(shell)
        .insert(crate::ghost_verbs::GhostRoleSpawner {
            name: def.name.clone(),
            desc: crate::ghost_roles::role_text(&def),
            uses: def.uses,
            def: Some(path.to_owned()),
        });
    let area = crate::bots::area_name(world, pos);
    let alert = format!("В зоне {area} создана оболочка дрона.");
    for ghost in crate::ghost_poll::ghosts(world) {
        tell_mob(world, ghost, Span::Notice, alert.clone());
    }
    Some(shell)
}

/// tg `mob_spawn/ghost_role/drone` `create(ghost)`: the shell's drone for the ghost's client
/// ("drone (N)"), the role's texts and the drone's `Login`; a shell out of uses goes.
pub fn take_shell(
    world: &mut World,
    client: Entity,
    ghost: Entity,
    shell: Entity,
    def: &MobSpawnDef,
) -> Option<Entity> {
    let mob_type = def.mob_type.as_deref()?;
    let &pos = world.get::<TilePos>(shell)?;
    let mob = crate::mobs::spawn::spawn_mob(world, mob_type, pos)?;
    // `unique_name`: "[mob_name] ([rand(1, 999)])".
    let base = def
        .mob_name
        .clone()
        .unwrap_or_else(|| crate::security::name_of(world, mob));
    let n = world
        .resource_mut::<crate::combat::CombatRng>()
        .range(1, 999);
    world
        .entity_mut(mob)
        .insert(Name::new(format!("{base} ({n})")));
    if !crate::medical::brainmob::possess(world, client, ghost, mob) {
        world.despawn(mob);
        return None;
    }
    world.despawn(ghost);
    crate::noncrew_jobs::set_assigned_role(world, mob, crate::noncrew_jobs::GHOST_ROLE);
    if !def.infinite_use {
        let left = world
            .get::<crate::ghost_verbs::GhostRoleSpawner>(shell)
            .map_or(0, |s| s.uses.saturating_sub(1));
        if left == 0 && def.deletes_on_zero_uses_left {
            world.despawn(shell);
        } else if let Some(mut s) = world.get_mut::<crate::ghost_verbs::GhostRoleSpawner>(shell) {
            s.uses = left;
        }
    }
    for (span, text) in [
        (Span::BoldNotice, &def.you_are_text),
        (Span::Notice, &def.flavour_text),
        (Span::Danger, &def.important_text),
    ] {
        if !text.is_empty() {
            tell(world, client, span, text.clone());
        }
    }
    login(world, client, mob);
    info!(
        "client {client} took the drone shell {} at {pos:?}",
        def.path
    );
    Some(mob)
}

/// `/mob/living/basic/drone/Login`: the laws, the flavor text, the look unless picked.
fn login(world: &mut World, client: Entity, mob: Entity) {
    let Some(drone) = world.get::<Drone>(mob).cloned() else {
        return;
    };
    check_laws(world, client, &drone.laws);
    if let Some(flavor) = drone.flavor {
        tell(world, client, Span::Warning, flavor);
    }
    if !drone.picked {
        ask_visual(world, client, mob);
    }
}

fn check_laws(world: &mut World, client: Entity, laws: &str) {
    tell(world, client, Span::Bold, "Drone Laws");
    tell(world, client, Span::Plain, laws);
}

fn drone_icon(world: &World, state: &str) -> Option<ss13_shared::IconStateId> {
    world.get_resource::<IconIndex>()?.get(DRONE_DMI, state)
}

/// `pickVisualAppearance`'s first radial.
fn ask_visual(world: &mut World, client: Entity, mob: Entity) {
    let choices = [
        ("Maintenance Drone", format!("{MAINTDRONE}_grey")),
        ("Repair Drone", REPAIRDRONE.to_owned()),
        ("Scout Drone", SCOUTDRONE.to_owned()),
    ]
    .map(|(name, state)| RadialChoice {
        name: name.to_owned(),
        icon: drone_icon(world, &state),
    })
    .to_vec();
    radial(
        world,
        client,
        mob,
        choices,
        DronePrompt::Visual { drone: mob },
    );
}

fn radial(
    world: &mut World,
    client: Entity,
    mob: Entity,
    choices: Vec<RadialChoice>,
    prompt: DronePrompt,
) {
    let menu = crate::radial::RadialMenu {
        client,
        actor: mob,
        anchor: mob,
        title: String::new(),
        choices,
        require_near: true,
    };
    if let Some(p) = crate::radial::show(world, menu) {
        world.entity_mut(p).insert(prompt);
    }
}

/// The client still controls `mob`, conscious and not incapacitated (`check_menu`).
fn still_controls(world: &World, client: Entity, mob: Entity) -> bool {
    world
        .get::<PlayerMob>(mob)
        .is_some_and(|p| p.client == client)
        && world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, mob).incapacitated
}

fn answers(world: &mut World, mut answered: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Option<PromptAnswer>)> = answered
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.answer.clone()))
        .collect();
    for (prompt, client, answer) in pending {
        let Some(&asked) = world.get::<DronePrompt>(prompt) else {
            continue;
        };
        let item = match answer {
            Some(PromptAnswer::Item(i) | PromptAnswer::Button(i)) => Some(i),
            _ => None,
        };
        match asked {
            DronePrompt::Visual { drone } => {
                if world.get::<Drone>(drone).is_none() {
                    continue;
                }
                let checked = still_controls(world, client, drone);
                match item.filter(|_| checked) {
                    Some(0) => {
                        let choices = COLORS
                            .map(|c| RadialChoice {
                                name: c.to_owned(),
                                icon: drone_icon(world, &format!("{MAINTDRONE}_{c}")),
                            })
                            .to_vec();
                        radial(world, client, drone, choices, DronePrompt::Color { drone });
                    }
                    Some(1) => set_visual(world, drone, REPAIRDRONE, None),
                    Some(2) => set_visual(world, drone, SCOUTDRONE, None),
                    _ => set_visual(world, drone, MAINTDRONE, Some("grey")),
                }
            }
            DronePrompt::Color { drone } => {
                if world.get::<Drone>(drone).is_none() {
                    continue;
                }
                let color = item
                    .filter(|_| still_controls(world, client, drone))
                    .and_then(|i| COLORS.get(i).copied())
                    .unwrap_or("grey");
                set_visual(world, drone, MAINTDRONE, Some(color));
            }
            DronePrompt::Interact { drone, target } => match item {
                Some(0) => try_reactivate(world, client, drone, target),
                Some(1) => try_cannibalize(world, client, drone, target),
                _ => {}
            },
            DronePrompt::Ping { drone } => {
                let Some(level) = item.and_then(|i| PING_LEVELS.get(i)) else {
                    continue;
                };
                let Some(&pos) = world.get::<TilePos>(drone) else {
                    continue;
                };
                if world.get::<Drone>(drone).is_none()
                    || world.get::<Stat>(drone) == Some(&Stat::Dead)
                {
                    continue;
                }
                let name = crate::security::name_of(world, drone);
                let area = crate::bots::area_name(world, pos);
                alert_drones(
                    world,
                    drone,
                    Span::BoldWarning,
                    &format!("DRONE PING: {name}: {level} priority alert in {area}!"),
                );
            }
        }
    }
}

/// The "Drone" verbs of the stat panel.
fn verbs(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<crate::statpanel::StatPanelUi>>>,
) {
    use crate::statpanel::{StatPanelHost, StatPanelUi};
    let pending: Vec<(Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<StatPanelUi>>>())
        .filter(|a| a.act.tab == VERB_TAB)
        .map(|a| (a.client, a.source, a.act.name.clone()))
        .collect();
    for (client, source, verb) in pending {
        if world.get::<StatPanelHost>(source).map(|h| h.client) != Some(client) {
            continue;
        }
        let Some(mob) = world.resource::<crate::server::Players>().mob(client) else {
            continue;
        };
        let Some(drone) = world.get::<Drone>(mob).cloned() else {
            continue;
        };
        match verb.as_str() {
            VERB_LAWS => check_laws(world, client, &drone.laws),
            VERB_PING => {
                let prompt = ask_world(
                    world,
                    Ask::new(
                        client,
                        mob,
                        "Drone ping",
                        "Alert severity level",
                        PromptKind::List {
                            items: PING_LEVELS.map(str::to_owned).to_vec(),
                            default: None,
                        },
                    ),
                );
                world
                    .entity_mut(prompt)
                    .insert(DronePrompt::Ping { drone: mob });
            }
            _ => {}
        }
    }
}

/// Clicks on drones before melee: a drone's empty hand on a dead drone (`attack_drone`), a
/// screwdriver (`screwdriver_act`), a wrench (`wrench_act`).
fn drone_clicks(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        let (user, target) = (click.mob, click.target);
        if world
            .resource::<crate::security::MeleeHandled>()
            .0
            .contains(&user)
            || world.get::<Drone>(target).is_none()
            || world.get::<Stat>(user) != Some(&Stat::Conscious)
            || click.modifiers.right
        {
            continue;
        }
        let held = world
            .get::<Inventory>(user)
            .and_then(Inventory::active_item);
        let handled = match held {
            None => attack_drone(world, click.client, user, target),
            Some(item) => match tool_of(world, item).map(|t| t.0) {
                Some(ToolKind::Screwdriver) => {
                    screwdriver_act(world, click.client, user, item, target)
                }
                Some(ToolKind::Wrench) if user != target => {
                    let actor = Actor {
                        client: click.client,
                        mob: user,
                        item: Some(item),
                    };
                    let (name, user_name) = (
                        crate::security::name_of(world, target),
                        crate::security::name_of(world, user),
                    );
                    crate::medical::visible(
                        world,
                        user,
                        &format!("Вы нажимаете кнопку сброса {name} до заводских настроек..."),
                        &format!("{user_name} начинает сбрасывать {name}..."),
                        Span::Notice,
                    );
                    use_tool(world, actor, target, 50.0, 0, Job::Drone(DroneJob::Reset));
                    true
                }
                _ => false,
            },
        };
        if handled {
            world
                .resource_mut::<crate::security::MeleeHandled>()
                .0
                .insert(user);
        }
    }
}

/// tg `attack_drone`: a drone's hand on another, dead drone asks what to do.
fn attack_drone(world: &mut World, client: Entity, user: Entity, target: Entity) -> bool {
    if user == target
        || world.get::<Drone>(user).is_none()
        || world.get::<Stat>(target) != Some(&Stat::Dead)
    {
        return false;
    }
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            user,
            "Drone Interaction",
            "Perform which action?",
            PromptKind::Alert {
                buttons: vec![REACTIVATE.to_owned(), CANNIBALIZE.to_owned()],
                autofocus: true,
            },
        ),
    );
    world.entity_mut(prompt).insert(DronePrompt::Interact {
        drone: user,
        target,
    });
    true
}

/// The ghost that left `mob` (tg `get_ghost`).
fn ghost_of(world: &mut World, mob: Entity) -> Option<Entity> {
    let mut q = world.query::<(Entity, &Ghost)>();
    q.iter(world)
        .find(|(_, g)| g.body == Some(mob))
        .map(|(e, _)| e)
}

/// Whether a dead drone answered `alert` still has its player (`client`, or a ghost's).
fn has_player(world: &mut World, mob: Entity) -> bool {
    world.get::<PlayerMob>(mob).is_some()
        || ghost_of(world, mob).is_some_and(|g| world.get::<PlayerMob>(g).is_some())
}

/// Rechecked after the alert: both still there, the user next to a dead drone.
fn interaction_valid(world: &World, user: Entity, target: Entity) -> bool {
    world.get::<Drone>(user).is_some()
        && world.get::<Stat>(user) == Some(&Stat::Conscious)
        && world.get::<Stat>(target) == Some(&Stat::Dead)
        && world.get::<Drone>(target).is_some()
        && crate::storage::in_reach(world, user, target)
}

/// tg `try_reactivate`.
fn try_reactivate(world: &mut World, client: Entity, user: Entity, target: Entity) {
    if !interaction_valid(world, user, target) {
        return;
    }
    let name = crate::security::name_of(world, target);
    if !has_player(world, target) {
        let gadget = {
            const GADGETS: [&str; 12] = [
                "hypertext inflator",
                "failsafe directory",
                "DRM switch",
                "stack initializer",
                "anti-freeze capacitor",
                "data stream diode",
                "TCP bottleneck",
                "supercharged I/O bolt",
                "tradewind stabilizer",
                "radiated XML cable",
                "registry fluid tank",
                "open-source debunker",
            ];
            let i = world
                .resource_mut::<crate::combat::CombatRng>()
                .range(0, 11);
            GADGETS[i as usize]
        };
        tell(
            world,
            client,
            Span::Warning,
            format!("You can't seem to find the {gadget}! Without it, {name} won't boot."),
        );
        return;
    }
    let user_name = crate::security::name_of(world, user);
    crate::medical::visible(
        world,
        user,
        &format!("Вы начинаете перезапускать {name}..."),
        &format!("{user_name} начинает перезапускать {name}."),
        Span::Notice,
    );
    let actor = Actor {
        client,
        mob: user,
        item: None,
    };
    crate::tools::do_after(
        world,
        actor,
        target,
        30.0,
        0,
        Job::Drone(DroneJob::Reactivate),
    );
}

/// tg `attack_drone`'s "Cannibalize".
fn try_cannibalize(world: &mut World, client: Entity, user: Entity, target: Entity) {
    if !interaction_valid(world, user, target) {
        return;
    }
    if world
        .get::<Health>(user)
        .is_some_and(|h| h.health() >= h.max)
    {
        tell(
            world,
            client,
            Span::Warning,
            "Вы уже в идеальном состоянии!",
        );
        return;
    }
    let (name, user_name) = (
        crate::security::name_of(world, target),
        crate::security::name_of(world, user),
    );
    crate::medical::visible(
        world,
        user,
        &format!("Вы начинаете разбирать {name} на запчасти..."),
        &format!("{user_name} начинает разбирать {name} на запчасти."),
        Span::Notice,
    );
    let actor = Actor {
        client,
        mob: user,
        item: None,
    };
    crate::tools::do_after(
        world,
        actor,
        target,
        60.0,
        0,
        Job::Drone(DroneJob::Cannibalize),
    );
}

/// tg `screwdriver_act`. False lets the click go on (a dead drone is hit).
fn screwdriver_act(
    world: &mut World,
    client: Entity,
    user: Entity,
    item: Entity,
    target: Entity,
) -> bool {
    if world.get::<Stat>(target) == Some(&Stat::Dead) {
        let text = if world.get::<Drone>(user).is_some() {
            "сначала перезапустите!"
        } else {
            "не починить!"
        };
        crate::balloon::balloon_alert(world, user, target, text);
        return false;
    }
    let name = crate::security::name_of(world, target);
    if world.get::<Health>(target).is_some_and(|h| h.brute <= 0.0) {
        tell(
            world,
            client,
            Span::Warning,
            format!("Винты {name} уже не затянуть сильнее!"),
        );
        return true;
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы начинаете затягивать ослабленные винты {name}..."),
    );
    let actor = Actor {
        client,
        mob: user,
        item: Some(item),
    };
    use_tool(world, actor, target, 80.0, 0, Job::Drone(DroneJob::Repair));
    true
}

/// The end of a drone `do_after`, its state rechecked.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: DroneJob) {
    let target = action.target;
    if world.get::<Drone>(target).is_none() {
        return;
    }
    let (name, user_name) = (
        crate::security::name_of(world, target),
        crate::security::name_of(world, mob),
    );
    match job {
        DroneJob::Repair => {
            if world.get::<Stat>(target) == Some(&Stat::Dead) {
                return;
            }
            if let Some(mut health) = world.get_mut::<Health>(target) {
                let brute = health.brute;
                health.adjust(DamageType::Brute, -brute);
            }
            let own = if mob == target {
                "Вы затягиваете свои ослабленные винты.".to_owned()
            } else {
                format!("Вы затягиваете ослабленные винты {name}!")
            };
            crate::medical::visible(
                world,
                mob,
                &own,
                &format!("{user_name} затягивает ослабленные винты {name}!"),
                Span::Notice,
            );
            if mob != target {
                tell_mob(
                    world,
                    target,
                    Span::Notice,
                    format!("{user_name} затягивает ваши ослабленные винты."),
                );
            }
        }
        DroneJob::Reset => {
            crate::medical::visible(
                world,
                mob,
                &format!("Вы сбрасываете директивы {name} до заводских!"),
                &format!("{user_name} сбрасывает {name}!"),
                Span::Notice,
            );
            unhack(world, target);
        }
        DroneJob::Reactivate => {
            if !interaction_valid(world, mob, target) {
                return;
            }
            if let Some(health) = world.get::<Health>(target).map(Health::healed) {
                world.entity_mut(target).insert(health);
            }
            if let Some(mut stat) = world.get_mut::<Stat>(target) {
                *stat = Stat::Conscious;
            }
            crate::medical::visible(
                world,
                mob,
                &format!("Вы перезапустили {name}."),
                &format!("{user_name} перезапускает {name}!"),
                Span::Notice,
            );
            alert_drones(
                world,
                target,
                Span::Notice,
                &format!("DRONE NETWORK: {name} connected."),
            );
            if let Some(ghost) = ghost_of(world, target) {
                tell_mob(
                    world,
                    ghost,
                    Span::BoldNotice,
                    format!("Вас ({name}) перезапустил {user_name}!"),
                );
            }
        }
        DroneJob::Cannibalize => {
            if !interaction_valid(world, mob, target) {
                return;
            }
            let max = world.get::<Health>(target).map_or(0.0, |h| h.max);
            if let Some(mut health) = world.get_mut::<Health>(mob) {
                health.adjust(DamageType::Brute, -max);
            }
            crate::medical::visible(
                world,
                mob,
                &format!("Вы чините себя запчастями {name}."),
                &format!("{user_name} чинит себя запчастями {name}!"),
                Span::Notice,
            );
            if let Some(&pos) = world.get::<TilePos>(target) {
                crate::construction::spawn_obj(world, OIL, pos);
            }
            // `ghostize(can_reenter_corpse = FALSE)`.
            crate::health::ghostize_gone(world, target);
            if let Some(ghost) = ghost_of(world, target)
                && let Some(mut g) = world.get_mut::<Ghost>(ghost)
            {
                g.body = None;
            }
            world.despawn(target);
        }
    }
}

/// `update_drone_hack(FALSE)`: a hacked drone that can be fixed gets its laws back.
fn unhack(world: &mut World, mob: Entity) {
    let Some(drone) = world.get::<Drone>(mob).cloned() else {
        return;
    };
    if !drone.hacked || !drone.can_unhack || world.get::<PlayerMob>(mob).is_none() {
        return;
    }
    let laws = world
        .get_resource::<DroneLaws>()
        .map_or_else(|| BASE_LAWS.to_owned(), |l| l.0.clone());
    if let Some(mut d) = world.get_mut::<Drone>(mob) {
        d.hacked = false;
        d.laws = laws.clone();
    }
    set_shy(world, mob, drone.initial_shy);
    crate::status_effects::apply(world, mob, crate::status_effects::Incap::Stun, 4.0);
    tell_mob(world, mob, Span::Info, "From now on, these are your laws:");
    tell_mob(world, mob, Span::Plain, laws);
}

/// tg `death`: the internal storage and the hat fall (the hands: `crate::dextrous`), the
/// network hears it.
fn on_death(world: &mut World) {
    let died: Vec<Entity> = world
        .query_filtered::<(Entity, Ref<Stat>), With<Drone>>()
        .iter(world)
        .filter(|(_, s)| s.is_changed() && !s.is_added() && **s == Stat::Dead)
        .map(|(e, _)| e)
        .collect();
    for mob in died {
        if let (Some(inventory), Some(&pos)) = (
            world.get::<Inventory>(mob).cloned(),
            world.get::<TilePos>(mob),
        ) {
            for item in [inventory.suit_storage, inventory.head]
                .into_iter()
                .flatten()
            {
                if !no_drop(world, item) {
                    put_down(world, item, pos);
                }
            }
        }
        let name = crate::security::name_of(world, mob);
        alert_drones(
            world,
            mob,
            Span::Danger,
            &format!("DRONE NETWORK: {name} is not responding."),
        );
    }
}

/// `holderloving/check_my_loc`: a built-in tool anywhere but its storage or the hands of the
/// storage's drone goes back into the storage; it goes with its storage.
fn holder_loving(world: &mut World) {
    let moved: Vec<(Entity, Entity)> = world
        .query_filtered::<(Entity, &HolderLoving), Or<(Added<TilePos>, Added<StoredIn>, Added<CarriedBy>)>>()
        .iter(world)
        .map(|(e, h)| (e, h.0))
        .collect();
    for (item, holder) in moved {
        if world.get_entity(holder).is_err() {
            world.despawn(item);
            continue;
        }
        let holder_mob = world.get::<CarriedBy>(holder).map(|c| c.0);
        let valid = world.get::<StoredIn>(item).is_some_and(|s| s.0 == holder)
            || world
                .get::<CarriedBy>(item)
                .is_some_and(|c| Some(c.0) == holder_mob);
        if !valid {
            crate::storage::put_inside(world, item, holder);
        }
    }
}

/// `stationstuck` with `PUNISHMENT_GIB`: a derelict drone off its z-level is gibbed.
fn station_stuck(world: &mut World) {
    let fled: Vec<Entity> = world
        .query::<(Entity, &StationStuck, &TilePos)>()
        .iter(world)
        .filter(|(_, s, p)| p.z != s.0)
        .map(|(e, ..)| e)
        .collect();
    for mob in fled {
        world.entity_mut(mob).remove::<StationStuck>();
        tell_mob(world, mob, Span::Danger, DERELICT_ESCAPE);
        crate::health::gib(world, mob);
    }
}

/// tg drone `examine`: the storage, the hat, the status LED, the hack, the screws, death.
pub fn examine_lines(world: &World, mob: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(drone) = world.get::<Drone>(mob) else {
        return;
    };
    let abstract_item = |e: Entity| {
        world
            .get::<DmItem>(e)
            .is_some_and(|i| i.item_flags & (1 << 7) != 0)
    };
    if let Some(inventory) = world.get::<Inventory>(mob) {
        if let Some(storage) = inventory.suit_storage.filter(|&e| !abstract_item(e)) {
            let name = crate::security::name_of(world, storage);
            lines.push((
                Span::Info,
                format!("Во внутреннем хранилище у него {name}."),
            ));
        }
        if let Some(hat) = inventory.head.filter(|&e| !abstract_item(e)) {
            let name = crate::security::name_of(world, hat);
            lines.push((Span::Info, format!("На голове у него {name}.")));
        }
    }
    let stat = world.get::<Stat>(mob).copied().unwrap_or_default();
    let has_client = world.get::<PlayerMob>(mob).is_some();
    if !has_client && stat != Stat::Dead {
        lines.push((
            Span::Info,
            "Его индикатор состояния мерно мигает.".to_owned(),
        ));
    }
    if drone.hacked {
        lines.push((Span::Warning, "Его дисплей светится красным!".to_owned()));
    }
    if let Some(health) = world.get::<Health>(mob)
        && health.health() < health.max
    {
        if health.health() > health.max * 0.33 {
            lines.push((Span::Warning, "Его винты слегка ослаблены.".to_owned()));
        } else {
            lines.push((Span::BoldWarning, "Его винты сильно ослаблены!".to_owned()));
        }
    }
    if stat == Stat::Dead {
        let text = if has_client {
            "На его дисплее мигает сообщение: \"REBOOT -- REQUIRED\"."
        } else {
            "На его дисплее мигает сообщение: \"ERROR -- OFFLINE\"."
        };
        lines.push((Span::Info, text.to_owned()));
    }
}

/// `/obj/item/uplink/interact` from the hand: an open item uplink of the traitors' catalog.
fn open_item_uplink(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let opens = world
        .get::<crate::antag::uplink::Uplink>(item)
        .is_some_and(|u| {
            !u.lockable && !u.locked && u.uplink_flag == ss13_mapload::antag::UPLINK_TRAITORS
        });
    if opens {
        crate::antag::uplink::open_uplink(world, client, mob, item);
    }
    opens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_laws_are_numbered_without_comments_or_blanks() {
        let text = "#Header\n\nFirst law.\r\n# not a law\nSecond law.\n";
        assert_eq!(
            parse_laws(text).as_deref(),
            Some("1. First law.\n2. Second law.")
        );
        assert_eq!(parse_laws("# only comments\n\n"), None);
    }

    #[test]
    fn drone_types_take_the_most_specific_kind() {
        assert_eq!(kind_of(BADASS).map(|k| k.root), Some(BADASS));
        assert_eq!(kind_of(SYNDRONE).map(|k| k.root), Some(SYNDRONE));
        assert_eq!(kind_of(DRONE).map(|k| k.root), Some(DRONE));
        assert_eq!(
            kind_of("/mob/living/basic/drone/classic").map(|k| (k.shy, k.storage)),
            Some((false, Some("/obj/item/storage/backpack/duffelbag/drone")))
        );
        assert!(kind_of("/mob/living/basic/node_drone").is_none());
        // Only the regular and snowflake drones are shy; syndrones are hacked for good.
        let shy: Vec<&str> = KINDS.iter().filter(|k| k.shy).map(|k| k.root).collect();
        assert_eq!(shy, vec![SNOWFLAKE, DRONE]);
        let syndrone = kind_of(SYNDRONE).unwrap();
        assert!(syndrone.hacked && !syndrone.can_unhack);
    }

    #[test]
    fn drone_chat_names_the_speaker() {
        let message = drone_chat_message("drone (12)", "говорит", "Привет");
        assert_eq!(
            message.plain_text(),
            "Drone Chat: drone (12) говорит, \"Привет\""
        );
    }
}
