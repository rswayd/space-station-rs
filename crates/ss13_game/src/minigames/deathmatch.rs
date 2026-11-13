//! Deathmatch (server), after tg `code/modules/deathmatch`: ghosts' lobbies and the fight to
//! the last one standing.
//!
//! - The panel ([`DeathmatchController`], tg `/datum/deathmatch_controller`, `DeathmatchPanel`):
//!   the open lobbies; a ghost hosts a new one (after the round start), joins one as a player
//!   (as an observer once it is full) or spectates it.
//! - A lobby ([`DeathmatchLobby`], tg `/datum/deathmatch_lobby`, `DeathmatchLobby` window), keyed
//!   by its host's key: players with a ready flag and a loadout of the map's ([`MAPS`],
//!   [`LOADOUTS`]), observers. The host picks the map, kicks, hands the lobby over, moves
//!   players to observers and back, starts it; it starts by itself once everyone is ready. A
//!   lobby not started within 5 minutes closes; a host who leaves passes the lobby on.
//! - The game: the map loads onto a new level; 8 seconds later (tg's lighting pause) every
//!   still-ghost player gets a human with their character at a random spawn in their loadout,
//!   observers are put in the arena. A dead, ghosted or deleted player is out (a living ghosted
//!   body turns to ash) and watches; with one player left, or when the map's time runs out, the
//!   game ends: the remaining bodies go, the arena is scrubbed, the lobby closes and the players'
//!   ghosts may go back to the bodies they left (tg `temporary_body`).
// shortcut: 6 of the 25 maps (those whose loadouts are plain outfits: assistant, chef,
// samurai); the others need their loadouts' spells, mutations, species and guns. Add per map.
// shortcut: no modifiers (`/datum/deathmatch_modifier`), no admin functions (force start,
// closing lobbies), no `GHOSTROLE_MINIGAME` switch, no clickable join link in the dead chat;
// loadouts' `backpack_contents`/`box` are not packed. Add with the backpack-contents helper.
// shortcut: each game leaves its emptied level behind (as basketball does).

use std::cell::RefCell;
use std::path::Path;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_assets::dmm::DmmMap;
use ss13_mapload::MapLoadPlugin;
use ss13_mapload::fidelity::is_under;
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};
use ss13_shared::protocol::InventorySlot;
use ss13_shared::ui::UiStatus;
use ss13_tgui::server::{AlwaysState, OpenUi, UiActed, UiWindow};

use super::basketball::{Kit, client_of, is_observer, key_of, now, rand_index, return_mind};
use crate::ghost::Ghost;
use crate::health::Stat;
use crate::inventory::{CarriedBy, Inventory};
use crate::minigames_ui::*;
use crate::server::{PlayerMob, Players};
use crate::tools::tell;

/// `/datum/lazy_template/deathmatch` `min_players`.
pub const MIN_PLAYERS: usize = 2;
/// `start_time`: the pause between the load and the spawn.
pub const START_DELAY_SECS: f64 = 8.0;
/// `lobby_afk_probably` timer.
pub const AFK_SECS: f64 = 300.0;
pub const MAPS_DIR: &str = "_maps/minigame/deathmatch";
const SPAWN_MARK: &str = "/obj/effect/landmark/deathmatch_player_spawn";
const RANDOMIZE: &str = "Случайно";

/// `/datum/outfit/deathmatch_loadout`.
pub struct Loadout {
    pub display_name: &'static str,
    pub desc: &'static str,
    pub kit: Kit,
}

const SNEAKERS: (InventorySlot, &str) = (
    InventorySlot::Shoes,
    "/obj/item/clothing/shoes/sneakers/black",
);
const BACKPACK: (InventorySlot, &str) = (InventorySlot::Back, "/obj/item/storage/backpack");

pub const LOADOUTS: [Loadout; 3] = [
    // `/datum/outfit/deathmatch_loadout/assistant`
    Loadout {
        display_name: "Ассистент",
        desc: "Простое снаряжение ассистента: серый комбинезон и ящик с инструментами.",
        kit: &[
            SNEAKERS,
            (
                InventorySlot::LeftHand,
                "/obj/item/storage/toolbox/mechanical",
            ),
            (
                InventorySlot::Uniform,
                "/obj/item/clothing/under/color/grey",
            ),
            BACKPACK,
            (InventorySlot::Belt, "/obj/item/flashlight"),
        ],
    },
    // `/datum/outfit/deathmatch_loadout/chef`
    Loadout {
        display_name: "Шеф-повар",
        desc: "Он любит пиццу.",
        kit: &[
            SNEAKERS,
            (
                InventorySlot::Uniform,
                "/obj/item/clothing/under/costume/buttondown/slacks/service",
            ),
            (InventorySlot::Suit, "/obj/item/clothing/suit/toggle/chef"),
            (
                InventorySlot::Head,
                "/obj/item/clothing/head/utility/chefhat",
            ),
            (
                InventorySlot::Mask,
                "/obj/item/clothing/mask/fakemoustache/italian",
            ),
            (
                InventorySlot::Gloves,
                "/obj/item/clothing/gloves/the_sleeping_carp",
            ),
            BACKPACK,
        ],
    },
    // `/datum/outfit/deathmatch_loadout/samurai`
    Loadout {
        display_name: "Самурай",
        desc: "Босоногий жаждет крови.",
        kit: &[
            SNEAKERS,
            (InventorySlot::LeftHand, "/obj/item/katana"),
            (
                InventorySlot::Uniform,
                "/obj/item/clothing/under/costume/gi",
            ),
        ],
    },
];

/// `/datum/lazy_template/deathmatch/*`.
pub struct ArenaMap {
    pub name: &'static str,
    pub desc: &'static str,
    /// The `.dmm` in [`MAPS_DIR`].
    pub file: &'static str,
    pub max_players: usize,
    /// `automatic_gameend_time`.
    pub secs: f64,
    /// Indexes into [`LOADOUTS`] (`allowed_loadouts`).
    pub loadouts: &'static [usize],
}

/// Sorted by name, as tg's `sort_list(maps)`.
pub const MAPS: [ArenaMap; 6] = [
    ArenaMap {
        name: "Maint Mania",
        desc: "Тёмные туннели техобслуживания, таблетки на полу, подручное оружие и кровавая драка. Добро пожаловать в утопию ассистентов.",
        file: "maint_mania",
        max_players: 8,
        secs: 480.0,
        loadouts: &[0],
    },
    ArenaMap {
        name: "Meat Tower",
        desc: "Шеф-повара делят мясную башню.",
        file: "meat_tower",
        max_players: 8,
        secs: 480.0,
        loadouts: &[1],
    },
    ArenaMap {
        name: "OSHA Violator",
        desc: "Чем был бы инженерный отдел без переусложнённого двигателя с конвейерами, эмиттерами и генераторами щитов? Правильно, не инженерным.",
        // tg's `map_name = "OSHA_violator"` differs from the file's case.
        file: "OSHA_Violator",
        max_players: 10,
        secs: 480.0,
        loadouts: &[0],
    },
    ArenaMap {
        name: "Ragecage",
        desc: "Веселье для всей семьи — классическая клетка.",
        file: "ragecage",
        max_players: 4,
        secs: 240.0,
        loadouts: &[0],
    },
    ArenaMap {
        name: "Sunrise",
        desc: "Самураи сходятся на рассвете.",
        file: "sunrise",
        max_players: 8,
        secs: 480.0,
        loadouts: &[2],
    },
    ArenaMap {
        name: "The Brig",
        desc: "Копия брига MetaStation.",
        file: "meta_brig",
        max_players: 12,
        secs: 480.0,
        loadouts: &[0],
    },
];

pub fn kit_paths() -> impl Iterator<Item = &'static str> {
    LOADOUTS.iter().flat_map(|l| l.kit.iter()).map(|&(_, p)| p)
}

/// The panel's source (tg `GLOB.deathmatch_game`), one per server, without a place.
#[derive(Component, Default, Debug)]
pub struct DeathmatchController;

/// `DEATHMATCH_NOT_PLAYING`, `DEATHMATCH_PRE_PLAYING`, `DEATHMATCH_PLAYING`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Playing {
    #[default]
    Not = 0,
    Pre = 1,
    Playing = 2,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub key: String,
    pub ready: bool,
    /// Index into [`LOADOUTS`].
    pub loadout: usize,
}

/// A spawned player.
#[derive(Clone, Debug)]
pub struct Fighter {
    pub key: String,
    pub client: Entity,
    pub mob: Entity,
    pub old_body: Option<Entity>,
}

/// The loaded arena.
#[derive(Clone, Debug)]
pub struct Arena {
    pub levels: Vec<u16>,
    pub spawns: Vec<TilePos>,
    pub start_at: f64,
    pub ends_at: f64,
    pub fighters: Vec<Fighter>,
    /// Every body spawned, for `return_mind`.
    pub spawned: Vec<(Entity, Option<Entity>, Entity)>,
}

/// tg `/datum/deathmatch_lobby`.
#[derive(Component, Debug)]
pub struct DeathmatchLobby {
    pub host: String,
    pub players: Vec<Member>,
    pub observers: Vec<String>,
    /// Index into [`MAPS`].
    pub map: usize,
    pub playing: Playing,
    pub created: f64,
    pub arena: Option<Arena>,
}

impl DeathmatchLobby {
    pub fn new(host: String, map: usize, now: f64) -> Self {
        let loadout = MAPS[map].loadouts[0];
        Self {
            players: vec![Member {
                key: host.clone(),
                ready: false,
                loadout,
            }],
            host,
            observers: Vec::new(),
            map,
            playing: Playing::Not,
            created: now,
            arena: None,
        }
    }

    pub fn is_in(&self, key: &str) -> bool {
        self.players.iter().any(|m| m.key == key) || self.observers.iter().any(|k| k == key)
    }

    fn player_mut(&mut self, key: &str) -> Option<&mut Member> {
        self.players.iter_mut().find(|m| m.key == key)
    }

    /// `remove_ckey_from_play`.
    pub fn remove(&mut self, key: &str) {
        self.players.retain(|m| m.key != key);
        self.observers.retain(|k| k != key);
    }

    /// `add_player`; a full map makes it an observer instead (`join`).
    pub fn add(&mut self, key: String) {
        if self.is_in(&key) {
            return;
        }
        if self.players.len() >= MAPS[self.map].max_players {
            self.observers.push(key);
        } else {
            let loadout = MAPS[self.map].loadouts[0];
            self.players.push(Member {
                key,
                ready: false,
                loadout,
            });
        }
    }

    /// `observe` / host "Toggle observe": a player becomes an observer and back while there
    /// is room.
    pub fn toggle_observe(&mut self, key: &str) -> bool {
        if self.players.iter().any(|m| m.key == key) {
            self.remove(key);
            self.observers.push(key.to_owned());
            return true;
        }
        if self.observers.iter().any(|k| k == key)
            && self.players.len() < MAPS[self.map].max_players
        {
            self.remove(key);
            let loadout = MAPS[self.map].loadouts[0];
            self.players.push(Member {
                key: key.to_owned(),
                ready: false,
                loadout,
            });
            return true;
        }
        false
    }

    /// `leave`: a leaving host hands the lobby to a player (to an observer when they were the
    /// only player). False when the lobby is now empty and closes.
    pub fn leave(&mut self, key: &str) -> bool {
        if self.host == key {
            if self.players.len() + self.observers.len() <= 1 {
                self.remove(key);
                return false;
            }
            let was_last_player =
                self.players.iter().any(|m| m.key == key) && self.players.len() <= 1;
            let next = if was_last_player {
                self.observers.iter().find(|k| *k != key).cloned()
            } else {
                self.players
                    .iter()
                    .find(|m| m.key != key)
                    .map(|m| m.key.clone())
            };
            if let Some(next) = next {
                self.host = next;
            }
        }
        self.remove(key);
        !(self.players.is_empty() && self.observers.is_empty())
    }

    /// `change_map`: players over the new map's cap become observers, loadouts it does not
    /// allow fall back to its first one.
    pub fn change_map(&mut self, map: usize) {
        self.map = map;
        let max = MAPS[map].max_players;
        while self.players.len() > max {
            let m = self.players.pop().expect("over the cap");
            self.observers.push(m.key);
        }
        for m in &mut self.players {
            if !MAPS[map].loadouts.contains(&m.loadout) {
                m.loadout = MAPS[map].loadouts[0];
            }
        }
    }

    pub fn all_ready(&self) -> bool {
        self.players.len() >= MIN_PLAYERS && self.players.iter().all(|m| m.ready)
    }
}

fn controller(world: &mut World) -> Entity {
    let mut q = world.query_filtered::<Entity, With<DeathmatchController>>();
    if let Some(e) = q.iter(world).next() {
        return e;
    }
    world
        .spawn((DeathmatchController, AlwaysState, Name::new("deathmatch")))
        .id()
}

fn lobby_of_host(world: &mut World, host: &str) -> Option<Entity> {
    let mut q = world.query::<(Entity, &DeathmatchLobby)>();
    q.iter(world).find(|(_, l)| l.host == host).map(|(e, _)| e)
}

fn lobby_with(world: &mut World, key: &str) -> Option<Entity> {
    let mut q = world.query::<(Entity, &DeathmatchLobby)>();
    q.iter(world).find(|(_, l)| l.is_in(key)).map(|(e, _)| e)
}

/// `/datum/minigames_menu/proc/deathmatch`: the panel for the ghost.
pub fn open_panel(world: &mut World, client: Entity, ghost: Entity) {
    let source = controller(world);
    world.write_message(OpenUi::new::<DeathmatchPanelUi>(
        client,
        ghost,
        source,
        "Дезматч",
    ));
}

fn open_lobby(world: &mut World, client: Entity, ghost: Entity, lobby: Entity) {
    world.write_message(OpenUi::new::<DeathmatchLobbyUi>(
        client,
        ghost,
        lobby,
        "Лобби дезматча",
    ));
}

/// `ui.close()` of the client's windows on `source`.
fn close_windows(world: &mut World, client: Entity, source: Entity) {
    let mut q = world.query::<&mut UiWindow>();
    for mut w in q.iter_mut(world) {
        if w.client == client && w.source == source {
            w.cap_status(UiStatus::Close);
        }
    }
}

/// `announce`: to every player and observer still connected.
fn announce(world: &mut World, lobby: Entity, span: Span, text: &str) {
    let Some(keys) = world.get::<DeathmatchLobby>(lobby).map(|l| {
        let mut keys: Vec<String> = l.players.iter().map(|m| m.key.clone()).collect();
        keys.extend(l.observers.iter().cloned());
        if let Some(a) = &l.arena {
            keys.extend(a.fighters.iter().map(|f| f.key.clone()));
        }
        keys.sort();
        keys.dedup();
        keys
    }) else {
        return;
    };
    for key in keys {
        if let Some(client) = client_of(world, &key) {
            tell(world, client, span, text);
        }
    }
}

fn notify_ghosts(world: &mut World, text: String) {
    let ghosts = crate::ghost_poll::ghosts(world);
    let clients: Vec<Entity> = ghosts
        .iter()
        .filter_map(|&g| world.get::<PlayerMob>(g).map(|p| p.client))
        .collect();
    if !clients.is_empty() {
        world.write_message(ss13_net::chat::server::SendChat {
            clients,
            message: ChatMessage::simple(ChatChannel::System, Span::Notice, text),
        });
    }
}

/// `/datum/deathmatch_controller/ui_data`.
pub fn panel_rows<'a>(
    lobbies: impl Iterator<Item = &'a DeathmatchLobby>,
    viewer: Option<&str>,
) -> DeathmatchPanelData {
    let mut data = DeathmatchPanelData::default();
    for l in lobbies {
        if viewer == Some(l.host.as_str()) {
            data.hosting = true;
        }
        if viewer.is_some_and(|v| l.is_in(v)) {
            data.playing = Some(l.host.clone());
        }
        data.lobbies.push(DeathmatchLobbyRow {
            name: l.host.clone(),
            players: l.players.len() as u32,
            max_players: MAPS[l.map].max_players as u32,
            map: MAPS[l.map].name.to_owned(),
            playing: l.playing as u8,
        });
    }
    data.lobbies.sort_by(|a, b| a.name.cmp(&b.name));
    data
}

/// `/datum/deathmatch_lobby/ui_data`.
pub fn lobby_data(l: &DeathmatchLobby, viewer: Option<&str>) -> DeathmatchLobbyData {
    let map = &MAPS[l.map];
    let me = viewer.and_then(|v| l.players.iter().find(|m| m.key == v));
    let mut loadouts = vec![RANDOMIZE.to_owned()];
    loadouts.extend(
        map.loadouts
            .iter()
            .map(|&i| LOADOUTS[i].display_name.to_owned()),
    );
    DeathmatchLobbyData {
        self_key: viewer.unwrap_or_default().to_owned(),
        host: viewer == Some(l.host.as_str()),
        playing: l.playing as u8,
        map: DeathmatchMapInfo {
            name: map.name.to_owned(),
            desc: map.desc.to_owned(),
            time: map.secs as u32,
            min_players: MIN_PLAYERS as u32,
            max_players: map.max_players as u32,
        },
        loadouts,
        loadoutdesc: match me {
            Some(m) => LOADOUTS[m.loadout].desc.to_owned(),
            None => "Вы наблюдатель! Наблюдатели слышат объявления лобби.".to_owned(),
        },
        players: l
            .players
            .iter()
            .map(|m| DeathmatchLobbyPlayer {
                key: m.key.clone(),
                host: m.key == l.host,
                ready: m.ready,
                loadout: LOADOUTS[m.loadout].display_name.to_owned(),
            })
            .collect(),
        observers: l
            .observers
            .iter()
            .map(|k| DeathmatchLobbyObserver {
                key: k.clone(),
                host: *k == l.host,
            })
            .collect(),
    }
}

/// The checked actor of an act: a ghost owned by `client`, and its key.
fn ghost_key(world: &World, client: Entity, actor: Entity) -> Option<String> {
    (world.get::<Ghost>(actor).is_some()
        && world.get::<PlayerMob>(actor).map(|p| p.client) == Some(client))
    .then(|| key_of(world, client))
    .flatten()
}

fn round_started(world: &World) -> bool {
    world
        .get_resource::<crate::round::Ticker>()
        .is_none_or(|t| t.state() >= crate::lobby::GameState::Playing)
}

/// The controller's `ui_act`.
pub fn panel_act(world: &mut World, client: Entity, actor: Entity, act: PanelAct) {
    let Some(key) = ghost_key(world, client, actor) else {
        return;
    };
    match act {
        PanelAct::Host => {
            if lobby_of_host(world, &key).is_some() {
                return;
            }
            if !round_started(world) {
                tell(world, client, Span::Warning, "Раунд ещё не начался!");
                return;
            }
            // tg: a host already in another lobby stays listed there too; here they leave it.
            if let Some(other) = lobby_with(world, &key) {
                leave(world, other, &key);
            }
            let map = rand_index(world, MAPS.len());
            let now = now(world);
            let source = controller(world);
            close_windows(world, client, source);
            let lobby = world
                .spawn((
                    DeathmatchLobby::new(key.clone(), map, now),
                    AlwaysState,
                    Name::new("deathmatch lobby"),
                ))
                .id();
            info!("deathmatch: {key} opened a lobby");
            notify_ghosts(world, format!("{key} открыл новое лобби дезматча."));
            open_lobby(world, client, actor, lobby);
        }
        PanelAct::Join(id) => {
            let Some(chosen) = lobby_of_host(world, &id) else {
                return;
            };
            let current = lobby_with(world, &key);
            if let Some(cur) = current.filter(|&c| c != chosen) {
                leave(world, cur, &key);
            }
            if current != Some(chosen) {
                let mut l = world.get_mut::<DeathmatchLobby>(chosen).expect("found");
                // `join`: not while it plays.
                if l.playing != Playing::Not {
                    return;
                }
                l.add(key.clone());
                info!("deathmatch: {key} joined lobby {id}");
            }
            open_lobby(world, client, actor, chosen);
        }
        PanelAct::Spectate(id) => {
            let Some(chosen) = lobby_of_host(world, &id) else {
                return;
            };
            let current = lobby_with(world, &key);
            if current == Some(chosen) {
                open_lobby(world, client, actor, chosen);
                return;
            }
            if let Some(cur) = current {
                leave(world, cur, &key);
            }
            let mut l = world.get_mut::<DeathmatchLobby>(chosen).expect("found");
            l.observers.push(key);
            let playing = l.playing != Playing::Not;
            let arena_pos = l.arena.as_ref().and_then(|a| a.spawns.first().copied());
            if !playing {
                open_lobby(world, client, actor, chosen);
            } else if let Some(pos) = arena_pos {
                world.entity_mut(actor).insert(pos);
            }
        }
    }
}

/// `leave` on the lobby entity; an emptied lobby closes.
fn leave(world: &mut World, lobby: Entity, key: &str) {
    let Some(mut l) = world.get_mut::<DeathmatchLobby>(lobby) else {
        return;
    };
    if !l.leave(key) && l.playing == Playing::Not {
        remove_lobby(world, lobby);
    }
}

/// `remove_lobby`: the lobby goes (its windows close with their source).
fn remove_lobby(world: &mut World, lobby: Entity) {
    if world
        .get::<DeathmatchLobby>(lobby)
        .is_some_and(|l| l.arena.is_some())
    {
        end_game(world, lobby, false);
        return;
    }
    world.try_despawn(lobby).ok();
}

/// The lobby window's `ui_act`.
pub fn lobby_act(world: &mut World, client: Entity, actor: Entity, lobby: Entity, act: LobbyAct) {
    let Some(key) = ghost_key(world, client, actor) else {
        return;
    };
    let Some(l) = world.get::<DeathmatchLobby>(lobby) else {
        return;
    };
    let is_host = l.host == key;
    let not_playing = l.playing == Playing::Not;
    match act {
        LobbyAct::Start => {
            if !is_host || !not_playing {
                return;
            }
            if l.players.len() < MIN_PLAYERS {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Пока недостаточно игроков для начала.",
                );
                return;
            }
            start_game(world, lobby);
        }
        LobbyAct::Leave => {
            if !not_playing {
                return;
            }
            close_windows(world, client, lobby);
            leave(world, lobby, &key);
            open_panel(world, client, actor);
        }
        LobbyAct::ChangeLoadout { player, loadout } => {
            if !not_playing || (player != key && !is_host) {
                return;
            }
            let allowed = MAPS[l.map].loadouts;
            let pick = if loadout == RANDOMIZE {
                Some(allowed[rand_index(world, allowed.len())])
            } else {
                allowed
                    .iter()
                    .copied()
                    .find(|&i| LOADOUTS[i].display_name == loadout)
            };
            if let Some(pick) = pick
                && let Some(mut l) = world.get_mut::<DeathmatchLobby>(lobby)
                && let Some(m) = l.player_mut(&player)
            {
                m.loadout = pick;
            }
        }
        LobbyAct::Observe => {
            if not_playing && let Some(mut l) = world.get_mut::<DeathmatchLobby>(lobby) {
                l.toggle_observe(&key);
            }
        }
        LobbyAct::Ready => {
            if !not_playing {
                return;
            }
            let mut l = world.get_mut::<DeathmatchLobby>(lobby).expect("checked");
            let Some(m) = l.player_mut(&key) else {
                return;
            };
            m.ready = !m.ready;
            if l.all_ready() {
                start_game(world, lobby);
            }
        }
        LobbyAct::Host(func) => {
            if !is_host || !not_playing {
                return;
            }
            match func {
                HostFunc::Kick(target) => {
                    if !l.is_in(&target) {
                        return;
                    }
                    leave(world, lobby, &target);
                    if let Some(tc) = client_of(world, &target) {
                        close_windows(world, tc, lobby);
                        if let Some(mob) = world
                            .resource::<Players>()
                            .mob(tc)
                            .filter(|&m| world.get::<Ghost>(m).is_some())
                        {
                            open_panel(world, tc, mob);
                        }
                    }
                }
                HostFunc::TransferHost(target) => {
                    if target != key && l.is_in(&target) {
                        world
                            .get_mut::<DeathmatchLobby>(lobby)
                            .expect("checked")
                            .host = target;
                    }
                }
                HostFunc::ToggleObserve(target) => {
                    world
                        .get_mut::<DeathmatchLobby>(lobby)
                        .expect("checked")
                        .toggle_observe(&target);
                }
                HostFunc::ChangeMap(name) => {
                    if let Some(i) = MAPS.iter().position(|m| m.name == name) {
                        world
                            .get_mut::<DeathmatchLobby>(lobby)
                            .expect("checked")
                            .change_map(i);
                    }
                }
            }
        }
    }
}

/// The spawn landmarks of the parsed map (1-based DMM cells) are noted and taken out.
fn take_spawns(map: &mut DmmMap) -> Vec<(u32, u32, u32)> {
    let (sx, sy, sz) = map.size;
    let mut out = Vec::new();
    for z in 1..=sz {
        for y in 1..=sy {
            for x in 1..=sx {
                let Some(prefab) = map.get(x, y, z) else {
                    continue;
                };
                for o in &prefab.objects {
                    if is_under(&o.path, SPAWN_MARK) {
                        out.push((x, y, z));
                    }
                }
            }
        }
    }
    for prefab in &mut map.prefabs {
        prefab.objects.retain(|o| !is_under(&o.path, SPAWN_MARK));
    }
    out
}

/// `map.lazy_load()`: the levels and the spawn points.
fn load_arena(world: &mut World, map: &ArenaMap) -> Option<(Vec<u16>, Vec<TilePos>)> {
    let source = world.get_resource::<MapLoadPlugin>()?.source.clone();
    let path = source.join(MAPS_DIR).join(format!("{}.dmm", map.file));
    if !ss13_data::exists(&path) {
        warn!("deathmatch {}: no {}", map.name, path.display());
        return None;
    }
    let found = RefCell::new(Vec::new());
    let prepare = |_: &Path, m: &mut DmmMap| {
        *found.borrow_mut() = take_spawns(m);
    };
    let first_z = match ss13_mapload::load_template_with(world, &path, &prepare) {
        Ok(z) => z,
        Err(e) => {
            warn!("deathmatch {}: {e:#}", map.name);
            ss13_world::run_map_init(world);
            return None;
        }
    };
    ss13_world::run_map_init(world);
    let found = found.into_inner();
    let levels_n = found.iter().map(|m| m.2).max().unwrap_or(1).max(1);
    let levels = (0..levels_n).map(|i| first_z + i as u16).collect();
    let spawns = found
        .into_iter()
        .map(|(x, y, z)| TilePos::new(x as i32, y as i32, first_z + (z - 1) as u16))
        .collect();
    Some((levels, spawns))
}

/// `start_game` + `find_spawns_and_start_delay`: the arena loads; the players come in after
/// [`START_DELAY_SECS`].
pub fn start_game(world: &mut World, lobby: Entity) {
    let Some(l) = world.get::<DeathmatchLobby>(lobby) else {
        return;
    };
    if l.playing != Playing::Not {
        return;
    }
    let (host, map) = (l.host.clone(), l.map);
    let Some((levels, spawns)) = load_arena(world, &MAPS[map]) else {
        if let Some(c) = client_of(world, &host) {
            tell(
                world,
                c,
                Span::Warning,
                "Не удалось загрузить карту. Попробуйте позже или обратитесь к кодерам.",
            );
        }
        return;
    };
    let start_at = now(world) + START_DELAY_SECS;
    let mut l = world.get_mut::<DeathmatchLobby>(lobby).expect("checked");
    l.playing = Playing::Pre;
    l.arena = Some(Arena {
        levels,
        spawns,
        start_at,
        ends_at: f64::INFINITY,
        fighters: Vec::new(),
        spawned: Vec::new(),
    });
}

/// `start_game_after_delay`: each connected ghost player gets a body at a random spawn; the
/// observers are put in the arena. Too few spawns: the arena goes, the lobby is back.
fn start_after_delay(world: &mut World, lobby: Entity) {
    let l = world.get::<DeathmatchLobby>(lobby).expect("caller checked");
    let arena = l.arena.clone().expect("pre-playing");
    let (players, observers, map) = (l.players.clone(), l.observers.clone(), l.map);
    if arena.spawns.len() < players.len() {
        warn!(
            "deathmatch {}: {} spawns for {} players",
            MAPS[map].name,
            arena.spawns.len(),
            players.len()
        );
        crate::science::bitrunning::server::scrub_levels(world, &arena.levels);
        let mut l = world.get_mut::<DeathmatchLobby>(lobby).expect("checked");
        l.arena = None;
        l.playing = Playing::Not;
        return;
    }
    let mut spawns = arena.spawns.clone();
    let mut fighters = Vec::new();
    let mut spawned = Vec::new();
    for m in &players {
        // Checked again: the ghost may have left during the pause.
        let ghost = client_of(world, &m.key).and_then(|c| {
            world
                .resource::<Players>()
                .mob(c)
                .filter(|&g| world.get::<Ghost>(g).is_some())
                .map(|g| (c, g))
        });
        let Some((client, ghost)) = ghost else {
            info!("deathmatch: {} left before the start", m.key);
            continue;
        };
        let pos = spawns.swap_remove(rand_index(world, spawns.len()));
        let loadout = if MAPS[map].loadouts.contains(&m.loadout) {
            m.loadout
        } else {
            MAPS[map].loadouts[0]
        };
        let old_body = world.get::<Ghost>(ghost).and_then(|g| g.body);
        let Some(mob) = spawn_fighter(world, client, ghost, pos, LOADOUTS[loadout].kit) else {
            continue;
        };
        spawned.push((client, old_body, mob));
        fighters.push(Fighter {
            key: m.key.clone(),
            client,
            mob,
            old_body,
        });
    }
    let fallback = arena.spawns[0];
    for key in &observers {
        if let Some(c) = client_of(world, key)
            && let Some(g) = world
                .resource::<Players>()
                .mob(c)
                .filter(|&g| world.get::<Ghost>(g).is_some())
        {
            world.entity_mut(g).insert(fallback);
        }
    }
    let now = now(world);
    let mut l = world.get_mut::<DeathmatchLobby>(lobby).expect("checked");
    l.players
        .retain(|m| fighters.iter().any(|f| f.key == m.key));
    l.playing = Playing::Playing;
    if let Some(a) = l.arena.as_mut() {
        a.fighters = fighters;
        a.spawned = spawned;
        a.ends_at = now + MAPS[map].secs;
    }
    info!("deathmatch {}: started", MAPS[map].name);
    announce(world, lobby, Span::BoldAnnounce, "ВПЕРЁД!");
}

/// `spawn_observer_as_player`: a human with the client's character at `pos` in `kit`,
/// controlled by the client; the ghost goes.
fn spawn_fighter(
    world: &mut World,
    client: Entity,
    ghost: Entity,
    pos: TilePos,
    kit: Kit,
) -> Option<Entity> {
    let name = crate::jobs::character_name(world, client);
    let look =
        crate::human::player_body(client, world.get::<crate::human::AppearancePrefs>(client));
    let mob = crate::server::spawn_body(world, pos, name, look);
    let mut inventory = world.get::<Inventory>(mob).cloned().unwrap_or_default();
    for &(slot, path) in kit {
        if inventory.slot(slot).is_some() {
            continue;
        }
        let Some(item) = crate::construction::spawn_detached(world, path) else {
            warn!("deathmatch loadout: no prototype for {path}");
            continue;
        };
        world.entity_mut(item).insert((Networked, CarriedBy(mob)));
        *inventory.slot_mut(slot) = Some(item);
    }
    crate::species::equip_species_outfit(world, mob, &mut inventory);
    world.entity_mut(mob).insert(inventory);
    crate::inventory::refresh_look(world, mob);
    if !crate::medical::brainmob::possess(world, client, ghost, mob) {
        world.despawn(mob);
        return None;
    }
    world.despawn(ghost);
    Some(mob)
}

/// Why a fighter is out (`player_died` on `COMSIG_LIVING_DEATH`, `COMSIG_MOB_GHOSTIZED`,
/// `COMSIG_QDELETING`).
pub fn is_out(world: &World, f: &Fighter) -> bool {
    world.get_entity(f.mob).is_err()
        || world.get::<Stat>(f.mob) == Some(&Stat::Dead)
        || world.resource::<Players>().mob(f.client) != Some(f.mob)
}

/// `player_died`: the fighter watches as an observer; a living body left behind turns to
/// ash; with one fighter left the game ends.
fn player_died(world: &mut World, lobby: Entity, f: &Fighter) {
    let alive = world.get_entity(f.mob).is_ok() && world.get::<Stat>(f.mob) != Some(&Stat::Dead);
    let name = world
        .get::<Name>(f.mob)
        .map(|n| n.as_str().to_owned())
        .unwrap_or_else(|| f.key.clone());
    if world.resource::<Players>().mob(f.client) == Some(f.mob) {
        let _ = crate::ghost::ghostize_client(world, f.client);
    }
    if alive {
        crate::health::dust_with(world, f.mob, true, true, true);
    }
    let Some(mut l) = world.get_mut::<DeathmatchLobby>(lobby) else {
        return;
    };
    l.players.retain(|m| m.key != f.key);
    if !l.observers.contains(&f.key) {
        l.observers.push(f.key.clone());
    }
    let left = l.arena.as_mut().map_or(0, |a| {
        a.fighters.retain(|x| x.mob != f.mob);
        a.fighters.len()
    });
    announce(
        world,
        lobby,
        Span::BoldAnnounce,
        &format!("{name} ПОГИБАЕТ. ОСТАЛОСЬ: {left}."),
    );
    if left <= 1 {
        end_game(world, lobby, false);
    }
}

/// `end_game` (`game_took_too_long` with `timeout`): the winner is announced, the remaining
/// bodies go, the arena is scrubbed, the lobby closes, the ghosts may go back to their bodies.
pub fn end_game(world: &mut World, lobby: Entity, timeout: bool) {
    if timeout {
        announce(
            world,
            lobby,
            Span::BoldAnnounce,
            "Игроки слишком затянули! Игра окончена!",
        );
    }
    let Some(arena) = world
        .get_mut::<DeathmatchLobby>(lobby)
        .and_then(|mut l| l.arena.take())
    else {
        world.try_despawn(lobby).ok();
        return;
    };
    let winner = arena
        .fighters
        .first()
        .and_then(|f| world.get::<Name>(f.mob))
        .map(|n| n.as_str().to_owned())
        .unwrap_or_else(|| "никто".to_owned());
    announce(
        world,
        lobby,
        Span::BoldAnnounce,
        &format!("ИГРА ОКОНЧЕНА. ПОБЕДИТЕЛЬ: {winner}."),
    );
    for f in &arena.fighters {
        if world.get_entity(f.mob).is_ok() {
            crate::health::ghostize_gone(world, f.mob);
            let items = crate::health::carried_items(world, f.mob);
            crate::health::despawn_all(world, items);
            world.try_despawn(f.mob).ok();
        }
    }
    let mobs: Vec<Entity> = arena.spawned.iter().map(|s| s.2).collect();
    for &(client, old_body, _) in &arena.spawned {
        return_mind(world, client, old_body, &mobs);
    }
    crate::science::bitrunning::server::scrub_levels(world, &arena.levels);
    info!("deathmatch: game over, levels {:?} scrubbed", arena.levels);
    world.try_despawn(lobby).ok();
}

/// The lobbies' timers: the start pause, the fighters' deaths, the time limit, the AFK close.
// Cost: per tick, one pass over the lobbies (a handful) and their fighters.
pub(super) fn tick(world: &mut World) {
    let now = now(world);
    let mut q = world.query::<(Entity, &DeathmatchLobby)>();
    let lobbies: Vec<(Entity, Playing, f64, Option<(f64, f64)>)> = q
        .iter(world)
        .map(|(e, l)| {
            (
                e,
                l.playing,
                l.created,
                l.arena.as_ref().map(|a| (a.start_at, a.ends_at)),
            )
        })
        .collect();
    for (lobby, playing, created, times) in lobbies {
        match (playing, times) {
            (Playing::Not, _) if now >= created + AFK_SECS => {
                let host = world.get::<DeathmatchLobby>(lobby).map(|l| l.host.clone());
                announce(
                    world,
                    lobby,
                    Span::Warning,
                    &format!(
                        "Лобби ({}) закрыто: оно не стартовало за 5 минут. В следующий раз будьте быстрее.",
                        host.unwrap_or_default()
                    ),
                );
                remove_lobby(world, lobby);
            }
            (Playing::Pre, Some((start_at, _))) if now >= start_at => {
                start_after_delay(world, lobby);
            }
            (Playing::Playing, Some((_, ends_at))) => {
                if now >= ends_at {
                    end_game(world, lobby, true);
                    continue;
                }
                let fighters = world
                    .get::<DeathmatchLobby>(lobby)
                    .and_then(|l| l.arena.as_ref())
                    .map(|a| a.fighters.clone())
                    .unwrap_or_default();
                for f in fighters {
                    if world.get::<DeathmatchLobby>(lobby).is_none() {
                        break;
                    }
                    if is_out(world, &f) {
                        player_died(world, lobby, &f);
                    }
                }
                // Everyone left before the start, or the last one went unnoticed.
                let left = world
                    .get::<DeathmatchLobby>(lobby)
                    .and_then(|l| l.arena.as_ref())
                    .map(|a| a.fighters.len());
                if left.is_some_and(|n| n == 0) {
                    end_game(world, lobby, false);
                }
            }
            _ => {}
        }
    }
}

/// tg drops disconnected and non-ghost members from the lists on each `ui_data`; here once a
/// second for lobbies not playing.
// Cost: once a second, one client lookup per lobby member (a few dozen at most).
pub(super) fn refresh_members(world: &mut World, mut next: Local<f64>) {
    let now = now(world);
    if now < *next {
        return;
    }
    *next = now + 1.0;
    let mut q = world.query::<(Entity, &DeathmatchLobby)>();
    let lobbies: Vec<(Entity, Vec<String>)> = q
        .iter(world)
        .filter(|(_, l)| l.playing == Playing::Not)
        .map(|(e, l)| {
            let mut keys: Vec<String> = l.players.iter().map(|m| m.key.clone()).collect();
            keys.extend(l.observers.iter().cloned());
            (e, keys)
        })
        .collect();
    for (lobby, keys) in lobbies {
        for key in keys {
            let gone = client_of(world, &key).is_none_or(|c| !is_observer(world, c));
            if gone {
                leave(world, lobby, &key);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PanelAct {
    Host,
    Join(String),
    Spectate(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostFunc {
    Kick(String),
    TransferHost(String),
    ToggleObserve(String),
    ChangeMap(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LobbyAct {
    Start,
    Leave,
    ChangeLoadout { player: String, loadout: String },
    Observe,
    Ready,
    Host(HostFunc),
}

pub(super) fn panel_acts(
    world: &mut World,
    mut cursor: Local<MessageCursor<UiActed<DeathmatchPanelUi>>>,
) {
    let acts: Vec<(Entity, Entity, PanelAct)> = cursor
        .read(world.resource::<Messages<UiActed<DeathmatchPanelUi>>>())
        .map(|a| (a.client, a.actor, a.act.clone()))
        .collect();
    for (client, actor, act) in acts {
        panel_act(world, client, actor, act);
    }
}

pub(super) fn lobby_acts(
    world: &mut World,
    mut cursor: Local<MessageCursor<UiActed<DeathmatchLobbyUi>>>,
) {
    let acts: Vec<(Entity, Entity, Entity, LobbyAct)> = cursor
        .read(world.resource::<Messages<UiActed<DeathmatchLobbyUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, act) in acts {
        lobby_act(world, client, actor, source, act);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(key: &str) -> Member {
        Member {
            key: key.to_owned(),
            ready: false,
            loadout: 0,
        }
    }

    #[test]
    fn every_map_allows_a_loadout_and_maps_are_sorted() {
        for m in &MAPS {
            assert!(!m.loadouts.is_empty(), "{}", m.name);
            assert!(m.loadouts.iter().all(|&i| i < LOADOUTS.len()));
            assert!(m.max_players >= MIN_PLAYERS);
        }
        assert!(MAPS.windows(2).all(|w| w[0].name < w[1].name));
        let registry = ss13_mapload::runtime::runtime_types();
        for path in kit_paths() {
            assert!(registry.contains(path), "{path} is not a runtime type");
        }
    }

    #[test]
    fn a_leaving_host_hands_the_lobby_over() {
        let mut l = DeathmatchLobby::new("a".into(), 3, 0.0);
        l.add("b".into());
        l.observers.push("c".into());
        assert!(l.leave("a"));
        assert_eq!(l.host, "b");
        // The last player leaving hands it to an observer.
        assert!(l.leave("b"));
        assert_eq!(l.host, "c");
        assert!(!l.leave("c"));
    }

    #[test]
    fn a_full_map_takes_observers_and_a_smaller_map_moves_players_out() {
        // Ragecage: 4 players.
        let mut l = DeathmatchLobby::new("a".into(), 3, 0.0);
        for k in ["b", "c", "d", "e"] {
            l.add(k.into());
        }
        assert_eq!(l.players.len(), 4);
        assert_eq!(l.observers, vec!["e".to_owned()]);
        assert!(!l.toggle_observe("e"));
        l.players.push(member("f"));
        l.change_map(3);
        assert_eq!(l.players.len(), 4);
        assert!(l.observers.contains(&"f".to_owned()));
        // Sunrise allows only the samurai.
        l.change_map(4);
        assert!(l.players.iter().all(|m| m.loadout == 2));
    }

    #[test]
    fn everyone_ready_needs_two_players() {
        let mut l = DeathmatchLobby::new("a".into(), 0, 0.0);
        l.players[0].ready = true;
        assert!(!l.all_ready());
        l.add("b".into());
        assert!(!l.all_ready());
        l.players[1].ready = true;
        assert!(l.all_ready());
    }
}
