//! Basketball (server), after tg `code/modules/basketball`: the ghost minigame and the hoops and
//! balls it plays with.
//!
//! - The lobby ([`BasketballLobby`], tg `/datum/basketball_controller`, one per server): ghosts
//!   sign up by key in the `BasketballPanel` window; a signed-up ghost starts the game once 2-7
//!   connected ghosts are signed up (tg `try_autostart` -> `basic_setup`). Disconnected keys
//!   stay listed until they come back; keys that are no longer ghosts leave the list.
//! - The game: a random stadium ([`TEAMS`], `_maps/minigame/basketball`) loads onto a new level
//!   (tg `lazy_load`), its hoop landmarks name the two minigame hoops (home: the map's team,
//!   away: another random team). Players alternate home/away; with an odd count the last one is
//!   the referee. Each gets a new human with their character's look and name in the team's
//!   outfit (everything `NODROP`), without breathing, burning, harming or being stunned (tg
//!   `TRAIT_NOBREATH`, `TRAIT_NOFIRE`, `TRAIT_PACIFISM`, `stunmod = 0`). After 3 minutes (the
//!   last 10 seconds counted down by the hoops) the team with more points wins, the losers turn
//!   to dust; 20 seconds later every body goes and the arena is scrubbed. The players' ghosts
//!   may go back to the bodies they left (tg `/datum/component/temporary_body`).
//! - Hoops ([`Hoop`]): an item thrown into a hoop scores by distance (95% from next to it down
//!   to 20% from 6 tiles, halved when the hoop itself was not clicked), 3 points beyond 2 tiles;
//!   any item used on an adjacent hoop is dunked (2 points, 20 stamina with a ball); a mob held
//!   in an aggressive grab is dunked into it (paralyzed 10 s, 30 stamina). Only balls score;
//!   the scorer may not pick the ball up again for 3 s. A minigame hoop counts only the other
//!   team's shots and then keeps that whole team off the ball.
//! - Balls: a ball that hits a carbon lands in its hands (no damage); a ball used on a carbon
//!   out of combat mode is passed into its hands.
// shortcut: no dribbling sounds, no spin (`TRAIT_SPINNING`) or stamina-based steal chance, no
// RMB jump shot over heads (`PASSMOB`) and no knockdown fumble; balls are thrown with the usual
// throw mode. Add with the spin emote and the disarm signal.
// shortcut: no referee whistle action ("Call foul" timestop: the port has no timestop), no
// wheelchair for legless characters, no random height, no scoreboard overlay on the hoops, no
// lusty xenomorph ID names; add with those systems.
// shortcut: each game leaves its emptied stadium level behind (as `crate::science::bitrunning`
// domains do); reuse levels when the map loader can paste onto an existing one.
// shortcut: no admin `GHOSTROLE_MINIGAME` switch (the port has no ghost role flags).

use std::cell::RefCell;
use std::path::Path;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_assets::dmm::DmmMap;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmName, MapLoadPlugin};
use ss13_net::server::{ClientToken, Networked};
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};
use ss13_shared::protocol::InventorySlot;
use ss13_tgui::server::{AlwaysState, OpenUi, UiActed};
use ss13_world::SpatialIndex;

use crate::click_handlers::ClickCtx;
use crate::combat::{CombatMode, CombatRng};
use crate::ghost::Ghost;
use crate::health::{Health, Stat};
use crate::inventory::{CarriedBy, Inventory, NoDrop};
use crate::minigames_ui::*;
use crate::mood::MoodEvent;
use crate::server::{PlayerMob, Players};
use crate::sound::{PlaySound, playsound};
use crate::throw::Thrown;
use crate::tools::tell;

/// `BASKETBALL_MIN_PLAYER_COUNT`, `BASKETBALL_MAX_PLAYER_COUNT`.
pub const MIN_PLAYERS: usize = 2;
pub const MAX_PLAYERS: usize = 7;
/// `game_duration`.
pub const GAME_SECS: f64 = 180.0;
/// `victory()`'s `addtimer(end_game, 20 SECONDS)`: the winners' victory lap.
pub const VICTORY_LAP_SECS: f64 = 20.0;
/// The hoops' countdown before the end.
const COUNTDOWN: u32 = 10;
/// `PICKUP_RESTRICTION_TIME`.
pub const PICKUP_RESTRICTION_SECS: f64 = 3.0;
/// `STAMINA_COST_DUNKING`, `STAMINA_COST_DUNKING_MOB`.
pub const STAMINA_COST_DUNKING: f32 = 20.0;
pub const STAMINA_COST_DUNKING_MOB: f32 = 30.0;
/// `/obj/structure/hoop`'s `throw_range_success`: percent by tiles flown (1 = next to it).
pub const THROW_RANGE_SUCCESS: [f32; 6] = [95.0, 80.0, 65.0, 50.0, 35.0, 20.0];
/// The mob dunk's `Paralyze(100)`.
const MOB_DUNK_PARALYZE_SECS: f32 = 10.0;

pub const MAPS_DIR: &str = "_maps/minigame/basketball";
pub const HOOP: &str = "/obj/structure/hoop";
pub const BALL: &str = "/obj/item/toy/basketball";
const LANDMARK: &str = "/obj/effect/landmark/basketball";
const HOME_SPAWN: &str = "/obj/effect/landmark/basketball/team_spawn/home";
const AWAY_SPAWN: &str = "/obj/effect/landmark/basketball/team_spawn/away";
const REFEREE_SPAWN: &str = "/obj/effect/landmark/basketball/team_spawn/referee";
const HOME_HOOP: &str = "/obj/effect/landmark/basketball/team_spawn/home_hoop";
const AWAY_HOOP: &str = "/obj/effect/landmark/basketball/team_spawn/away_hoop";

const BOUNCE: &str = "sound/items/basketball_bounce.ogg";
const BUZZER: &str = "sound/machines/scanner/scanbuzz.ogg";
const TIMER: &str = "sound/items/timer.ogg";
const WHISTLE: &str = "sound/items/whistle/whistle.ogg";

/// `/datum/mood_event/basketball_score`, `basketball_dunk` (BandaStation texts).
pub const SCORE_MOOD: MoodEvent = MoodEvent {
    id: "basketball_score",
    description: "Вжух! Ничего, кроме сетки.",
    mood_change: 2.0,
    timeout: 300.0,
    hidden: false,
};
pub const DUNK_MOOD: MoodEvent = MoodEvent {
    id: "basketball_dunk",
    description: "Слэм данк! Бум, шакалака!",
    mood_change: 2.0,
    timeout: 300.0,
    hidden: false,
};

/// An outfit's items by slot (tg `/datum/outfit/basketball/*`, resolved through the parent).
pub type Kit = &'static [(InventorySlot, &'static str)];

/// One `/datum/lazy_template/basketball`: the stadium and its home team.
pub struct Team {
    pub key: &'static str,
    /// The map's UI name.
    pub name: &'static str,
    /// `map_name`: the `.dmm` in [`MAPS_DIR`].
    pub map: &'static str,
    pub team_name: &'static str,
    pub uniform: Kit,
}

const ID: (InventorySlot, &str) = (InventorySlot::Id, "/obj/item/card/id/away");
const BLACK: (InventorySlot, &str) = (
    InventorySlot::Uniform,
    "/obj/item/clothing/under/color/black",
);
const WHITE_SNEAKERS: (InventorySlot, &str) = (
    InventorySlot::Shoes,
    "/obj/item/clothing/shoes/sneakers/white",
);

pub const TEAMS: [Team; 8] = [
    Team {
        key: "stadium",
        name: "Stadium",
        map: "stadium",
        team_name: "Nanotrasen Basketball Department",
        uniform: &[
            (
                InventorySlot::Uniform,
                "/obj/item/clothing/under/shorts/blue",
            ),
            (
                InventorySlot::Suit,
                "/obj/item/clothing/suit/jacket/letterman_nanotrasen",
            ),
            (
                InventorySlot::Shoes,
                "/obj/item/clothing/shoes/sneakers/black",
            ),
            ID,
        ],
    },
    Team {
        key: "lusty_xenomorphs",
        name: "Lusty Xenomorphs Stadium",
        map: "lusty_xenomorphs",
        team_name: "Lusty Xenomorphs",
        uniform: &[
            BLACK,
            WHITE_SNEAKERS,
            (InventorySlot::Suit, "/obj/item/clothing/suit/costume/xenos"),
            (InventorySlot::Head, "/obj/item/clothing/head/costume/xenos"),
            (InventorySlot::Mask, "/obj/item/clothing/mask/chameleon"),
            ID,
        ],
    },
    Team {
        key: "space_surfers",
        name: "Space Surfers Stadium",
        map: "space_surfers",
        team_name: "Space Surfers",
        uniform: &[
            BLACK,
            (
                InventorySlot::Shoes,
                "/obj/item/clothing/shoes/magboots/basketball",
            ),
            (
                InventorySlot::Suit,
                "/obj/item/clothing/suit/space/basketball",
            ),
            (InventorySlot::Head, "/obj/item/clothing/head/helmet/space"),
            (InventorySlot::Mask, "/obj/item/clothing/mask/breath"),
            (InventorySlot::Neck, "/obj/item/bedsheet/cosmos"),
            ID,
        ],
    },
    Team {
        key: "greytide_worldwide",
        name: "Greytide Worldwide Stadium",
        map: "greytide_worldwide",
        team_name: "Greytide Worldwide",
        uniform: &[
            (
                InventorySlot::Uniform,
                "/obj/item/clothing/under/color/grey/ancient",
            ),
            (
                InventorySlot::Shoes,
                "/obj/item/clothing/shoes/sneakers/brown",
            ),
            (InventorySlot::Mask, "/obj/item/clothing/mask/gas/prop"),
            (
                InventorySlot::Gloves,
                "/obj/item/clothing/gloves/color/fyellow",
            ),
            (InventorySlot::Belt, "/obj/item/storage/belt/utility"),
            ID,
        ],
    },
    Team {
        key: "ass_blast_usa",
        name: "Ass Blast USA Stadium",
        map: "ass_blast_usa",
        team_name: "Ass Blast USA",
        uniform: &[
            (
                InventorySlot::Uniform,
                "/obj/item/clothing/under/misc/patriotsuit",
            ),
            (
                InventorySlot::Shoes,
                "/obj/item/clothing/shoes/sneakers/red",
            ),
            (InventorySlot::Neck, "/obj/item/bedsheet/patriot"),
            ID,
        ],
    },
    Team {
        key: "soviet_bear",
        name: "Soviet Bears Stadium",
        map: "soviet_bear",
        team_name: "Soviet Bears",
        uniform: &[
            (
                InventorySlot::Uniform,
                "/obj/item/clothing/under/costume/soviet",
            ),
            (InventorySlot::Shoes, "/obj/item/clothing/shoes/winterboots"),
            (
                InventorySlot::Head,
                "/obj/item/clothing/head/costume/ushanka",
            ),
            (
                InventorySlot::Gloves,
                "/obj/item/clothing/gloves/color/brown",
            ),
            ID,
        ],
    },
    Team {
        key: "ash_gladiators",
        name: "Ash Gladiators Stadium",
        map: "ash_gladiators",
        team_name: "Ash Gladiators",
        uniform: &[
            (
                InventorySlot::Uniform,
                "/obj/item/clothing/under/costume/gladiator/ash_walker",
            ),
            (
                InventorySlot::Head,
                "/obj/item/clothing/head/helmet/gladiator",
            ),
            (InventorySlot::Back, "/obj/item/spear"),
            ID,
        ],
    },
    Team {
        key: "beach_bums",
        name: "Beach Bums Stadium",
        map: "beach_bums",
        team_name: "Beach Bums",
        uniform: &[
            (
                InventorySlot::Uniform,
                "/obj/item/clothing/under/shorts/red",
            ),
            (InventorySlot::Eyes, "/obj/item/clothing/glasses/sunglasses"),
            (InventorySlot::Shoes, "/obj/item/clothing/shoes/sandal"),
            ID,
        ],
    },
];

/// `/datum/outfit/basketball/referee`.
pub const REFEREE_KIT: Kit = &[
    (
        InventorySlot::Uniform,
        "/obj/item/clothing/under/costume/referee",
    ),
    (InventorySlot::Shoes, "/obj/item/clothing/shoes/laceup"),
    (
        InventorySlot::Mask,
        "/obj/item/clothing/mask/whistle/minigame",
    ),
    (InventorySlot::Gloves, "/obj/item/clothing/gloves/latex"),
    (InventorySlot::Head, "/obj/item/clothing/head/soft/black"),
    ID,
];

/// Every item path the outfits spawn (`ss13_mapload::runtime`).
pub fn kit_paths() -> impl Iterator<Item = &'static str> {
    TEAMS
        .iter()
        .flat_map(|t| t.uniform.iter())
        .chain(REFEREE_KIT.iter())
        .map(|&(_, p)| p)
}

/// The global lobby and game (tg `GLOB.basketball_signup`, `GLOB.basketball_bad_signup`,
/// `GLOB.basketball_game`), on one entity without a place (the window's source).
#[derive(Component, Default, Debug)]
pub struct BasketballLobby {
    /// Keys of connected ghosts signed up, in sign-up order.
    pub signup: Vec<String>,
    /// Keys signed up whose client is gone.
    pub bad_signup: Vec<String>,
    pub game: Option<Game>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Home,
    Away,
    Referee,
}

#[derive(Clone, Debug)]
pub struct GamePlayer {
    pub client: Entity,
    pub mob: Entity,
    pub side: Side,
    /// The body the ghost had left (tg `temporary_body`'s `old_body`).
    pub old_body: Option<Entity>,
}

/// A running game (tg `current_map` and what `prepare_game` found).
#[derive(Clone, Debug)]
pub struct Game {
    /// Indexes into [`TEAMS`].
    pub home: usize,
    pub away: usize,
    pub levels: Vec<u16>,
    pub home_hoop: Entity,
    pub away_hoop: Entity,
    pub players: Vec<GamePlayer>,
    pub ends_at: f64,
    /// The next countdown second to announce (0: done).
    pub countdown: u32,
    /// Set by the victory: when the bodies and the arena go.
    pub cleanup_at: Option<f64>,
}

/// On a minigame body: tg's `BASKETBALL_MINIGAME_TRAIT` traits.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Baller;

/// A hoop's score (tg `/obj/structure/hoop`), added on its first score.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct Hoop {
    pub total_score: u32,
    /// `/obj/structure/hoop/minigame` `team_ckeys`: the bodies that may score on it (the other
    /// team); `None` on an ordinary hoop.
    pub team: Option<Vec<Entity>>,
}

/// On a ball: tg `pickup_restriction_ckeys` and `pickup_cooldown`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct PickupRestriction {
    pub mobs: Vec<Entity>,
    pub until: f64,
}

pub(super) fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn path_of(world: &World, e: Entity) -> Option<String> {
    crate::construction::path_of(world, e)
}

pub fn is_hoop(world: &World, e: Entity) -> bool {
    path_of(world, e).is_some_and(|p| is_under(&p, HOOP))
}

pub fn is_ball(world: &World, e: Entity) -> bool {
    world
        .get::<DmItem>(e)
        .is_some_and(|i| is_under(&i.path, BALL))
}

pub(super) fn key_of(world: &World, client: Entity) -> Option<String> {
    world.get::<ClientToken>(client).map(|t| t.0.clone())
}

pub(super) fn client_of(world: &mut World, key: &str) -> Option<Entity> {
    let mut q = world.query::<(Entity, &ClientToken)>();
    q.iter(world).find(|(_, t)| t.0 == key).map(|(e, _)| e)
}

/// `isobserver(client.mob)`.
pub(super) fn is_observer(world: &World, client: Entity) -> bool {
    world
        .resource::<Players>()
        .mob(client)
        .is_some_and(|m| world.get::<Ghost>(m).is_some())
}

/// The lobby entity, made on first use (tg `create_basketball_game`).
pub fn lobby(world: &mut World) -> Entity {
    let mut q = world.query_filtered::<Entity, With<BasketballLobby>>();
    if let Some(e) = q.iter(world).next() {
        return e;
    }
    world
        .spawn((
            BasketballLobby::default(),
            AlwaysState,
            Name::new("basketball lobby"),
        ))
        .id()
}

/// `/datum/minigames_menu/proc/basketball`: the lobby window for the ghost.
pub fn open_lobby(world: &mut World, client: Entity, ghost: Entity) {
    let lobby = lobby(world);
    check_signups(world, lobby);
    world.write_message(OpenUi::new::<BasketballPanelUi>(
        client,
        ghost,
        lobby,
        "Basketball",
    ));
}

/// `check_signups`: disconnected keys move to the bad list until they come back; keys that are
/// no longer ghosts leave.
pub fn check_signups(world: &mut World, lobby: Entity) {
    let Some(state) = world.get::<BasketballLobby>(lobby) else {
        return;
    };
    let (mut signup, mut bad) = (state.signup.clone(), state.bad_signup.clone());
    let mut back = Vec::new();
    bad.retain(|key| {
        if client_of(world, key).is_some() {
            back.push(key.clone());
            false
        } else {
            true
        }
    });
    for key in back {
        if !signup.contains(&key) {
            signup.push(key);
        }
    }
    let mut kept = Vec::new();
    for key in signup {
        match client_of(world, &key) {
            None => {
                if !bad.contains(&key) {
                    bad.push(key);
                }
            }
            Some(client) if is_observer(world, client) => kept.push(key),
            Some(_) => {}
        }
    }
    if let Some(mut state) = world.get_mut::<BasketballLobby>(lobby) {
        if state.signup != kept {
            state.signup = kept;
        }
        if state.bad_signup != bad {
            state.bad_signup = bad;
        }
    }
}

/// `ui_data`.
pub fn panel_data(lobby: &BasketballLobby) -> BasketballPanelData {
    let lobbydata = lobby
        .signup
        .iter()
        .map(|k| (k, false))
        .chain(lobby.bad_signup.iter().map(|k| (k, true)))
        .map(|(ckey, disconnected)| LobbyMember {
            ckey: ckey.clone(),
            disconnected,
        })
        .collect();
    BasketballPanelData {
        total_votes: lobby.signup.len() as u32,
        players_min: MIN_PLAYERS as u32,
        players_max: MAX_PLAYERS as u32,
        lobbydata,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LobbyAct {
    Signup,
    Start,
}

/// `ui_act`: only ghosts, only once the round has started.
pub fn lobby_act(world: &mut World, client: Entity, actor: Entity, lobby: Entity, act: LobbyAct) {
    if world.get::<Ghost>(actor).is_none()
        || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
        || world.get::<BasketballLobby>(lobby).is_none()
    {
        return;
    }
    let started = world
        .get_resource::<crate::round::Ticker>()
        .is_none_or(|t| t.state() >= crate::lobby::GameState::Playing);
    if !started {
        tell(world, client, Span::Warning, "Дождитесь начала раунда.");
        return;
    }
    let Some(key) = key_of(world, client) else {
        return;
    };
    match act {
        LobbyAct::Signup => {
            let mut state = world.get_mut::<BasketballLobby>(lobby).expect("checked");
            let signed = state.signup.contains(&key) || state.bad_signup.contains(&key);
            if signed {
                state.signup.retain(|k| *k != key);
                state.bad_signup.retain(|k| *k != key);
            } else {
                state.signup.push(key);
            }
            let text = if signed {
                "Вы отписались от баскетбола."
            } else {
                "Вы записались на баскетбол."
            };
            tell(world, client, Span::Notice, text);
            check_signups(world, lobby);
        }
        LobbyAct::Start => {
            let state = world.get::<BasketballLobby>(lobby).expect("checked");
            let (signed, running) = (state.signup.contains(&key), state.game.is_some());
            if !signed {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Чтобы начать игру, нужно записаться.",
                );
                return;
            }
            if running {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Дождитесь окончания текущей игры в баскетбол.",
                );
                return;
            }
            basic_setup(world, lobby);
        }
    }
}

/// tg `basic_setup`: the connected ghosts among the signed up; with enough of them up to
/// [`MAX_PLAYERS`] picked at random leave the list and play, the rest are told why not.
pub fn basic_setup(world: &mut World, lobby: Entity) -> bool {
    let Some(signup) = world
        .get::<BasketballLobby>(lobby)
        .map(|l| l.signup.clone())
    else {
        return false;
    };
    let mut possible: Vec<(String, Entity)> = Vec::new();
    let mut invalid = Vec::new();
    for key in signup {
        match client_of(world, &key) {
            Some(client) if is_observer(world, client) => possible.push((key, client)),
            _ => invalid.push(key),
        }
    }
    if let Some(mut state) = world.get_mut::<BasketballLobby>(lobby) {
        state.signup.retain(|k| !invalid.contains(k));
    }
    if possible.len() < MIN_PLAYERS {
        return false;
    }
    let wanted = possible.len().min(MAX_PLAYERS);
    let mut picked = Vec::new();
    for _ in 0..wanted {
        let i = world
            .resource_mut::<CombatRng>()
            .range(0, possible.len() as u32 - 1) as usize;
        picked.push(possible.swap_remove(i));
    }
    if let Some(mut state) = world.get_mut::<BasketballLobby>(lobby) {
        state.signup.retain(|k| !picked.iter().any(|(p, _)| p == k));
    }
    for (_, client) in &possible {
        tell(
            world,
            *client,
            Span::Danger,
            "Увы, в начинающейся игре в баскетбол слишком много игроков, и вас не выбрали.",
        );
        tell(
            world,
            *client,
            Span::Warning,
            "Вы остаётесь записаны и сможете сыграть, когда начинающаяся игра закончится.",
        );
    }
    let clients: Vec<Entity> = picked.into_iter().map(|(_, c)| c).collect();
    prepare_game(world, lobby, &clients)
}

/// What the stadium's landmarks marked (tile positions after the load).
#[derive(Default, Debug)]
struct Marks {
    home: Vec<TilePos>,
    away: Vec<TilePos>,
    referee: Vec<TilePos>,
    home_hoop: Option<TilePos>,
    away_hoop: Option<TilePos>,
}

/// The landmarks of the parsed stadium (1-based DMM cells) are noted and taken out.
fn take_landmarks(map: &mut DmmMap) -> Vec<(&'static str, u32, u32, u32)> {
    let (sx, sy, sz) = map.size;
    let mut out = Vec::new();
    for z in 1..=sz {
        for y in 1..=sy {
            for x in 1..=sx {
                let Some(prefab) = map.get(x, y, z) else {
                    continue;
                };
                for o in &prefab.objects {
                    let mark = [HOME_SPAWN, AWAY_SPAWN, REFEREE_SPAWN, HOME_HOOP, AWAY_HOOP]
                        .into_iter()
                        .find(|m| o.path == *m);
                    if let Some(mark) = mark {
                        out.push((mark, x, y, z));
                    }
                }
            }
        }
    }
    for prefab in &mut map.prefabs {
        prefab.objects.retain(|o| !is_under(&o.path, LANDMARK));
    }
    out
}

/// The minigame hoop on `pos`.
fn hoop_on(world: &World, pos: TilePos) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| is_hoop(world, e))
}

/// `current_map.lazy_load()` of `team`'s stadium: the level, its marks.
fn load_stadium(world: &mut World, team: &Team) -> Option<(Vec<u16>, Marks)> {
    let source = world.get_resource::<MapLoadPlugin>()?.source.clone();
    let path = source.join(MAPS_DIR).join(format!("{}.dmm", team.map));
    if !ss13_data::exists(&path) {
        warn!("basketball {}: no {}", team.key, path.display());
        return None;
    }
    let found = RefCell::new(Vec::new());
    let prepare = |_: &Path, map: &mut DmmMap| {
        *found.borrow_mut() = take_landmarks(map);
    };
    let first_z = match ss13_mapload::load_template_with(world, &path, &prepare) {
        Ok(z) => z,
        Err(e) => {
            warn!("basketball {}: {e:#}", team.key);
            ss13_world::run_map_init(world);
            return None;
        }
    };
    ss13_world::run_map_init(world);
    let found = found.into_inner();
    let levels_n = found.iter().map(|m| m.3).max().unwrap_or(1).max(1);
    let levels: Vec<u16> = (0..levels_n).map(|i| first_z + i as u16).collect();
    let mut marks = Marks::default();
    for (mark, x, y, z) in found {
        let pos = TilePos::new(x as i32, y as i32, first_z + (z - 1) as u16);
        match mark {
            HOME_SPAWN => marks.home.push(pos),
            AWAY_SPAWN => marks.away.push(pos),
            REFEREE_SPAWN => marks.referee.push(pos),
            HOME_HOOP => marks.home_hoop = Some(pos),
            AWAY_HOOP => marks.away_hoop = Some(pos),
            _ => {}
        }
    }
    Some((levels, marks))
}

pub(super) fn rand_index(world: &mut World, len: usize) -> usize {
    world.resource_mut::<CombatRng>().range(0, len as u32 - 1) as usize
}

/// tg `pick_n_take` of a spawn point; the last one again when they ran out (tg would get none).
fn take_spawn(world: &mut World, spawns: &mut Vec<TilePos>, last: TilePos) -> TilePos {
    if spawns.is_empty() {
        return last;
    }
    let i = rand_index(world, spawns.len());
    spawns.swap_remove(i)
}

/// tg `prepare_game` + `start_game` + `create_bodies` for the checked `clients`. False when
/// the stadium does not load or lacks its hoops (tg `CRASH`es); the players stay ghosts.
pub fn prepare_game(world: &mut World, lobby: Entity, clients: &[Entity]) -> bool {
    let home = rand_index(world, TEAMS.len());
    let Some((levels, marks)) = load_stadium(world, &TEAMS[home]) else {
        return false;
    };
    let hoops = (
        marks.home_hoop.and_then(|p| hoop_on(world, p)),
        marks.away_hoop.and_then(|p| hoop_on(world, p)),
    );
    let (Some(home_hoop), Some(away_hoop)) = hoops else {
        warn!(
            "basketball {}: no minigame hoops; scrubbed",
            TEAMS[home].key
        );
        crate::science::bitrunning::server::scrub_levels(world, &levels);
        return false;
    };
    let away = {
        let others: Vec<usize> = (0..TEAMS.len()).filter(|&i| i != home).collect();
        others[rand_index(world, others.len())]
    };
    // `home_hoop.name = current_map.team_name`, `away_hoop.name = away_map::team_name`.
    for (hoop, team) in [(home_hoop, home), (away_hoop, away)] {
        if let Some(mut name) = world.get_mut::<DmName>(hoop) {
            name.name = TEAMS[team].team_name.to_owned();
        }
        world.entity_mut(hoop).insert(Hoop {
            total_score: 0,
            team: Some(Vec::new()),
        });
    }
    let now = now(world);
    let fallback = marks
        .home
        .first()
        .or(marks.away.first())
        .copied()
        .unwrap_or(TilePos::new(1, 1, levels[0]));
    let (mut home_spawns, mut away_spawns, mut referee_spawns) = (
        marks.home.clone(),
        marks.away.clone(),
        marks.referee.clone(),
    );
    let has_referee = clients.len() % 2 == 1;
    let mut players = Vec::new();
    for (i, &client) in clients.iter().enumerate() {
        // Checked again: the ghost may have left since the sign-up check.
        let Some(ghost) = world
            .resource::<Players>()
            .mob(client)
            .filter(|&m| world.get::<Ghost>(m).is_some())
        else {
            continue;
        };
        let side = if has_referee && i + 1 == clients.len() {
            Side::Referee
        } else if i % 2 == 0 {
            Side::Home
        } else {
            Side::Away
        };
        let (pos, kit) = match side {
            Side::Referee => (
                take_spawn(world, &mut referee_spawns, fallback),
                REFEREE_KIT,
            ),
            Side::Home => (
                take_spawn(world, &mut home_spawns, fallback),
                TEAMS[home].uniform,
            ),
            Side::Away => (
                take_spawn(world, &mut away_spawns, fallback),
                TEAMS[away].uniform,
            ),
        };
        let old_body = world.get::<Ghost>(ghost).and_then(|g| g.body);
        let Some(mob) = spawn_baller(world, client, ghost, pos, kit) else {
            continue;
        };
        // `away_hoop.team_ckeys |= player_key`: a team scores on the other team's hoop.
        let scores_on = match side {
            Side::Home => Some(away_hoop),
            Side::Away => Some(home_hoop),
            Side::Referee => None,
        };
        if let Some(mut hoop) = scores_on.and_then(|h| world.get_mut::<Hoop>(h))
            && let Some(team) = hoop.team.as_mut()
        {
            team.push(mob);
        }
        let team = if side == Side::Away { away } else { home };
        greet(world, client, pos, side, TEAMS[team].team_name);
        players.push(GamePlayer {
            client,
            mob,
            side,
            old_body,
        });
    }
    // `notify_ghosts`.
    let ghosts = crate::ghost_poll::ghosts(world);
    let clients: Vec<Entity> = ghosts
        .iter()
        .filter_map(|&g| world.get::<PlayerMob>(g).map(|p| p.client))
        .collect();
    if !clients.is_empty() {
        world.write_message(ss13_net::chat::server::SendChat {
            clients,
            message: ChatMessage::simple(
                ChatChannel::System,
                Span::Notice,
                "Начинается мини-игра «Баскетбол»!",
            ),
        });
    }
    info!(
        "basketball: {} vs {} with {} players",
        TEAMS[home].team_name,
        TEAMS[away].team_name,
        players.len()
    );
    if let Some(mut state) = world.get_mut::<BasketballLobby>(lobby) {
        state.game = Some(Game {
            home,
            away,
            levels,
            home_hoop,
            away_hoop,
            players,
            ends_at: now + GAME_SECS,
            countdown: COUNTDOWN,
            cleanup_at: None,
        });
    }
    true
}

/// `create_bodies` for one player: a human with the client's character at `pos`, in `kit`,
/// with the minigame traits, controlled by the client; the ghost goes.
fn spawn_baller(
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
    world.entity_mut(mob).insert(Baller);
    equip(world, mob, kit);
    if !crate::medical::brainmob::possess(world, client, ghost, mob) {
        world.despawn(mob);
        return None;
    }
    world.despawn(ghost);
    // `TRAIT_PACIFISM` (`crate::medical::traumas::refresh` reads [`Baller`]).
    crate::medical::traumas::refresh(world, mob);
    Some(mob)
}

/// `equipOutfit` + `post_equip`: every item `NODROP`.
fn equip(world: &mut World, mob: Entity, kit: Kit) {
    let mut inventory = world.get::<Inventory>(mob).cloned().unwrap_or_default();
    for &(slot, path) in kit {
        if inventory.slot(slot).is_some() {
            continue;
        }
        let Some(item) = crate::construction::spawn_detached(world, path) else {
            warn!("basketball outfit: no prototype for {path}");
            continue;
        };
        world
            .entity_mut(item)
            .insert((Networked, CarriedBy(mob), NoDrop));
        *inventory.slot_mut(slot) = Some(item);
    }
    crate::species::equip_species_outfit(world, mob, &mut inventory);
    world.entity_mut(mob).insert(inventory);
    crate::inventory::refresh_look(world, mob);
}

fn greet(world: &mut World, client: Entity, pos: TilePos, side: Side, team: &str) {
    playsound(world, PlaySound::new(pos, WHISTLE, 30.0, false));
    if side == Side::Referee {
        tell(
            world,
            client,
            Span::Notice,
            "Вы — судья. Следите, чтобы команды играли честно.",
        );
        return;
    }
    tell(
        world,
        client,
        Span::Notice,
        format!(
            "Вы — баскетболист команды «{team}». Набирайте как можно больше очков, пока не вышло время."
        ),
    );
    for line in [
        "Клик мячом по игроку вне боевого режима — передача.",
        "Бросок мяча в кольцо: чем ближе, тем вернее; дальше двух клеток — 3 очка. Кликайте прямо по кольцу, иначе шанс вдвое меньше.",
        "Клик мячом по соседнему кольцу — слэм-данк, 2 очка (затраты выносливости: 20).",
        "В своё кольцо очки не засчитываются.",
    ] {
        tell(world, client, Span::Info, line);
    }
}

/// tg `victory()`: who won by the hoops' scores (the home hoop counts the away team's points).
/// `None`: a draw.
pub fn winner(home_hoop_score: u32, away_hoop_score: u32) -> Option<Side> {
    match home_hoop_score.cmp(&away_hoop_score) {
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Greater => Some(Side::Away),
        std::cmp::Ordering::Less => Some(Side::Home),
    }
}

/// The game's timers: the countdown, the victory, the cleanup.
// Cost: per tick, one lookup of the lobby entity; work only while a game runs.
pub(super) fn tick(world: &mut World) {
    let mut q = world.query::<(Entity, &BasketballLobby)>();
    let Some((lobby, game)) = q
        .iter(world)
        .find_map(|(e, l)| l.game.clone().map(|g| (e, g)))
    else {
        return;
    };
    let now = now(world);
    if let Some(at) = game.cleanup_at {
        if now >= at {
            end_game(world, lobby);
        }
        return;
    }
    if game.countdown > 0 && now >= game.ends_at - f64::from(game.countdown) {
        for hoop in [game.home_hoop, game.away_hoop] {
            if let Some(&pos) = world.get::<TilePos>(hoop) {
                playsound(world, PlaySound::new(pos, TIMER, 75.0, false));
            }
            crate::atmos_machinery::machine_say(
                world,
                hoop,
                &format!("Осталось секунд: {}", game.countdown),
            );
        }
        if let Some(mut l) = world.get_mut::<BasketballLobby>(lobby)
            && let Some(g) = l.game.as_mut()
        {
            g.countdown -= 1;
        }
    }
    if now >= game.ends_at {
        victory(world, lobby, &game);
    }
}

/// The client's mob, if it is still `mob` (tg `get_mob_by_ckey(ckey) in minigame_basketball_mobs`).
fn still_playing(world: &World, p: &GamePlayer) -> bool {
    world.resource::<Players>().mob(p.client) == Some(p.mob)
}

fn victory(world: &mut World, lobby: Entity, game: &Game) {
    let score = |world: &World, hoop| world.get::<Hoop>(hoop).map_or(0, |h| h.total_score);
    let result = winner(score(world, game.home_hoop), score(world, game.away_hoop));
    let text = match result {
        None => "Игра закончилась вничью!".to_owned(),
        Some(side) => {
            let team = if side == Side::Home {
                game.home
            } else {
                game.away
            };
            format!("Побеждает команда «{}»!", TEAMS[team].team_name)
        }
    };
    for p in &game.players {
        if !still_playing(world, p) {
            continue;
        }
        // tg tells the home and away teams, not the referee.
        if p.side == Side::Referee {
            continue;
        }
        tell(world, p.client, Span::Hypnophrase, text.clone());
        let lost = result.is_some_and(|w| w != p.side);
        if lost {
            crate::health::dust(world, p.mob);
        }
    }
    let t = now(world);
    if let Some(mut l) = world.get_mut::<BasketballLobby>(lobby)
        && let Some(g) = l.game.as_mut()
    {
        g.cleanup_at = Some(t + VICTORY_LAP_SECS);
        g.countdown = 0;
    }
}

/// tg `end_game`: the bodies ghostize for good and go, the arena is scrubbed; the players'
/// ghosts get back the bodies they left (`temporary_body`).
pub fn end_game(world: &mut World, lobby: Entity) {
    let Some(game) = world
        .get_mut::<BasketballLobby>(lobby)
        .and_then(|mut l| l.game.take())
    else {
        return;
    };
    let mobs: Vec<Entity> = game.players.iter().map(|p| p.mob).collect();
    for p in &game.players {
        if world.get_entity(p.mob).is_ok() {
            // `ghostize(can_reenter_corpse = FALSE)`, then `qdel`.
            crate::health::ghostize_gone(world, p.mob);
            let items = crate::health::carried_items(world, p.mob);
            crate::health::despawn_all(world, items);
            world.try_despawn(p.mob).ok();
        }
        return_mind(world, p.client, p.old_body, &mobs);
    }
    crate::science::bitrunning::server::scrub_levels(world, &game.levels);
    info!("basketball: game over, levels {:?} scrubbed", game.levels);
}

/// `temporary_body/return_mind`: the client's ghost (body-less, or left in a minigame body)
/// may re-enter the body it had before, when that still has no mind.
pub(super) fn return_mind(
    world: &mut World,
    client: Entity,
    old_body: Option<Entity>,
    mobs: &[Entity],
) {
    let Some(ghost) = world.resource::<Players>().mob(client) else {
        return;
    };
    let Some(body) = old_body.filter(|&b| world.get_entity(b).is_ok()) else {
        return;
    };
    if world.get::<PlayerMob>(body).is_some() {
        return;
    }
    if let Some(mut g) = world.get_mut::<Ghost>(ghost)
        && g.body.is_none_or(|b| mobs.contains(&b))
    {
        g.body = Some(body);
    }
}

/// tg `BASKETBALL_MINIGAME_TRAIT`'s `TRAIT_NOBREATH`, `TRAIT_NOFIRE` (`crate::species::has_trait`).
pub fn grants(world: &World, mob: Entity, t: crate::species::Trait) -> bool {
    matches!(
        t,
        crate::species::Trait::NoBreath | crate::species::Trait::NoFire
    ) && world.get::<Baller>(mob).is_some()
}

/// The basketball outfit's `stunmod = 0` (`crate::status_effects`).
pub fn stun_mod(world: &World, mob: Entity) -> f32 {
    if world.get::<Baller>(mob).is_some() {
        0.0
    } else {
        1.0
    }
}

/// tg `/obj/structure/hoop/score` (+ the minigame override). True when it counted.
pub fn score(world: &mut World, hoop: Entity, item: Entity, baller: Entity, points: u32) -> bool {
    let state = world.get::<Hoop>(hoop).cloned().unwrap_or_default();
    // `is_team_hoop`: no buzzer either.
    if let Some(team) = &state.team
        && !team.contains(&baller)
    {
        crate::balloon::balloon_alert_to_viewers(world, baller, "нельзя в своё кольцо!", None, &[]);
        return false;
    }
    if let Some(&pos) = world.get::<TilePos>(hoop) {
        playsound(world, PlaySound::new(pos, BUZZER, 100.0, false));
    }
    if !is_ball(world, item) {
        return false;
    }
    world.entity_mut(hoop).insert(Hoop {
        total_score: state.total_score + points,
        team: state.team.clone(),
    });
    let mut mobs = vec![baller];
    if let Some(team) = state.team {
        mobs.extend(team.into_iter().filter(|&m| m != baller));
    }
    let until = now(world) + PICKUP_RESTRICTION_SECS;
    world
        .entity_mut(item)
        .insert(PickupRestriction { mobs, until });
    true
}

/// The chance of a throw from `dist_travelled` tiles to score (`hitby`).
pub fn score_chance(dist_travelled: u32, clicked_hoop: bool) -> f32 {
    let distance = (dist_travelled as usize + 1).clamp(1, THROW_RANGE_SUCCESS.len());
    let chance = THROW_RANGE_SUCCESS[distance - 1];
    if clicked_hoop { chance } else { chance * 0.5 }
}

/// tg `/obj/structure/hoop/hitby` of a thrown item (`crate::throw`); true when `hit` is a hoop.
pub fn hoop_hitby(world: &mut World, item: Entity, hit: Entity, thrown: &Thrown) -> bool {
    if !is_hoop(world, hit) {
        return false;
    }
    if is_ball(world, item)
        && let Some(&pos) = world.get::<TilePos>(item)
    {
        playsound(world, PlaySound::new(pos, BOUNCE, 75.0, false));
    }
    let Some(thrower) = thrown.thrower.filter(|&t| world.get::<Health>(t).is_some()) else {
        return true;
    };
    let clicked = thrown.target == Some(hit);
    let distance = (thrown.path.dist_travelled as usize + 1).clamp(1, THROW_RANGE_SUCCESS.len());
    let chance = score_chance(thrown.path.dist_travelled, clicked);
    let thing = crate::security::name_of(world, item);
    let hoop_name = crate::security::name_of(world, hit);
    let Some(&at) = world.get::<TilePos>(hit) else {
        return true;
    };
    let text = if world.resource_mut::<CombatRng>().chance(chance) {
        world.entity_mut(item).insert(at);
        let points = if distance > 2 { 3 } else { 2 };
        crate::mood::add_event(world, thrower, "basketball", &SCORE_MOOD);
        score(world, hit, item, thrower, points);
        if clicked {
            format!("Вжух! {thing} влетает в {hoop_name}.")
        } else {
            format!("{thing} влетает в {hoop_name}.")
        }
    } else {
        let part = if clicked { "обода" } else { "щита" };
        format!("{thing} отскакивает от {part}: {hoop_name}!")
    };
    let chat =
        crate::pull::visible_message(world, at, ChatChannel::Combat, (Span::Warning, text), &[]);
    for message in chat {
        world.write_message(message);
    }
    true
}

/// tg `/obj/item/toy/basketball/throw_impact` on a carbon: it lands in its hands, no hit.
pub fn ball_hit(world: &mut World, item: Entity, mob: Entity) -> bool {
    if !is_ball(world, item) {
        return false;
    }
    if let Some(&pos) = world.get::<TilePos>(item) {
        playsound(world, PlaySound::new(pos, BOUNCE, 75.0, false));
    }
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return false;
    };
    crate::storage::put_in_hands(world, mob, &inventory, item);
    true
}

/// Held-item clicks (`HeldOnTarget`): a ball passed to a carbon out of combat mode (tg
/// `basketball/attack`), any item dunked into an adjacent hoop (tg `hoop/item_interaction`).
pub fn held_on_target(world: &mut World, c: &ClickCtx) -> bool {
    let Some(held) = c.held else {
        return false;
    };
    if is_hoop(world, c.target) {
        return dunk(world, c.client, c.mob, held, c.target);
    }
    let combat = world.get::<CombatMode>(c.mob).is_some_and(|m| m.0);
    if !is_ball(world, held)
        || combat
        || c.target == c.mob
        || world.get::<Inventory>(c.target).is_none()
        || world.get::<Health>(c.target).is_none()
    {
        return false;
    }
    let Some(inventory) = world.get::<Inventory>(c.target).cloned() else {
        return false;
    };
    playsound(world, PlaySound::new(c.mob_pos, BOUNCE, 75.0, false));
    world.entity_mut(held).remove::<CarriedBy>();
    crate::storage::put_in_hands(world, c.target, &inventory, held);
    true
}

/// tg `hoop/item_interaction`: the item goes onto the hoop's tile and scores 2.
fn dunk(world: &mut World, client: Entity, mob: Entity, item: Entity, hoop: Entity) -> bool {
    if crate::inventory::no_drop(world, item) {
        let thing = crate::security::name_of(world, item);
        tell(
            world,
            client,
            Span::Warning,
            format!("{thing} не отпускает вашу руку!"),
        );
        return true;
    }
    let Some(&at) = world.get::<TilePos>(hoop) else {
        return false;
    };
    let ball = is_ball(world, item);
    crate::inventory::put_down(world, item, at);
    let (user, thing, hoop_name) = (
        crate::security::name_of(world, mob),
        crate::security::name_of(world, item),
        crate::security::name_of(world, hoop),
    );
    let chat = crate::pull::visible_message(
        world,
        at,
        ChatChannel::Combat,
        (
            Span::Warning,
            format!("{user} забивает {thing} в {hoop_name}!"),
        ),
        &[],
    );
    for message in chat {
        world.write_message(message);
    }
    crate::mood::add_event(world, mob, "basketball", &DUNK_MOOD);
    score(world, hoop, item, mob, 2);
    if ball {
        crate::status_effects::adjust_stamina(world, mob, STAMINA_COST_DUNKING);
    }
    true
}

/// Empty-hand clicks (`EmptyHand`): a ball its scorer may not pick up yet (tg
/// `basketball/attack_hand`), a grabbed mob dunked into a hoop (tg `hoop/attack_hand`).
pub fn empty_hand(world: &mut World, c: &ClickCtx) -> bool {
    if is_ball(world, c.target) {
        let now = now(world);
        let Some(r) = world.get::<PickupRestriction>(c.target).cloned() else {
            return false;
        };
        if r.mobs.contains(&c.mob) && now < r.until {
            let left = ((r.until - now) * 10.0).ceil() / 10.0;
            crate::balloon::balloon_alert(
                world,
                c.mob,
                c.target,
                &format!("нельзя поднять ещё {left} с!"),
            );
            return true;
        }
        // `reset_pickup_restriction`.
        world.entity_mut(c.target).remove::<PickupRestriction>();
        return false;
    }
    if !is_hoop(world, c.target) {
        return false;
    }
    let Some(loser) = world
        .get::<crate::pull::Pulling>(c.mob)
        .map(|p| p.0)
        .filter(|&p| world.get::<Stat>(p).is_some())
    else {
        return false;
    };
    let grab = world
        .get::<crate::pull::GrabState>(c.mob)
        .copied()
        .unwrap_or_default();
    if grab < crate::pull::GrabState::Aggressive {
        tell(
            world,
            c.client,
            Span::Warning,
            "Нужен более крепкий захват!",
        );
        return true;
    }
    let Some(&at) = world.get::<TilePos>(c.target) else {
        return false;
    };
    world.entity_mut(c.mob).remove::<crate::pull::Pulling>();
    world.entity_mut(loser).insert(at);
    crate::status_effects::apply(
        world,
        loser,
        crate::status_effects::Incap::Paralyze,
        MOB_DUNK_PARALYZE_SECS,
    );
    let (baller, victim, hoop_name) = (
        crate::security::name_of(world, c.mob),
        crate::security::name_of(world, loser),
        crate::security::name_of(world, c.target),
    );
    let chat = crate::pull::visible_message(
        world,
        at,
        ChatChannel::Combat,
        (
            Span::Danger,
            format!("{baller} заталкивает {victim} в {hoop_name}!"),
        ),
        &[],
    );
    for message in chat {
        world.write_message(message);
    }
    playsound(world, PlaySound::new(at, BUZZER, 100.0, false));
    crate::status_effects::adjust_stamina(world, c.mob, STAMINA_COST_DUNKING_MOB);
    true
}

/// The lobby window's acts.
pub(super) fn lobby_acts(
    world: &mut World,
    mut cursor: Local<MessageCursor<UiActed<BasketballPanelUi>>>,
) {
    let acts: Vec<(Entity, Entity, Entity, LobbyAct)> = cursor
        .read(world.resource::<Messages<UiActed<BasketballPanelUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act))
        .collect();
    for (client, actor, source, act) in acts {
        lobby_act(world, client, actor, source, act);
    }
}

/// tg refreshes the sign-ups on every `ui_interact` and act; here once a second while someone
/// is signed up, so a disconnect shows in the open lobby windows.
// Cost: once a second, one pass over the clients per signed-up key (at most a few dozen).
pub(super) fn refresh_signups(world: &mut World, mut next: Local<f64>) {
    let now = now(world);
    if now < *next {
        return;
    }
    *next = now + 1.0;
    let mut q = world.query::<(Entity, &BasketballLobby)>();
    let lobbies: Vec<Entity> = q
        .iter(world)
        .filter(|(_, l)| !l.signup.is_empty() || !l.bad_signup.is_empty())
        .map(|(e, _)| e)
        .collect();
    for lobby in lobbies {
        check_signups(world, lobby);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_home_hoop_counts_the_away_team() {
        assert_eq!(winner(4, 2), Some(Side::Away));
        assert_eq!(winner(0, 3), Some(Side::Home));
        assert_eq!(winner(5, 5), None);
    }

    #[test]
    fn shots_lose_accuracy_with_distance_and_without_aiming() {
        assert_eq!(score_chance(0, true), 95.0);
        assert_eq!(score_chance(2, true), 65.0);
        assert_eq!(score_chance(5, true), 20.0);
        assert_eq!(score_chance(30, true), 20.0);
        assert_eq!(score_chance(0, false), 47.5);
    }

    #[test]
    fn every_team_has_a_map_and_a_uniform() {
        for team in &TEAMS {
            assert!(!team.map.is_empty() && !team.team_name.is_empty());
            assert!(team.uniform.iter().any(|(s, _)| *s == InventorySlot::Id));
        }
        let registry = ss13_mapload::runtime::runtime_types();
        for path in kit_paths() {
            assert!(registry.contains(path), "{path} is not a runtime type");
        }
    }

    #[test]
    fn landmarks_are_noted_and_taken_out() {
        let text = r#""a" = (/obj/effect/landmark/basketball/team_spawn/home,/turf/open/floor,/area/centcom/basketball)
"b" = (/obj/effect/landmark/basketball/team_spawn/away_hoop,/obj/structure/hoop/minigame,/turf/open/floor,/area/centcom/basketball)

(1,1,1) = {"
ab
"}
"#;
        let mut map = DmmMap::parse(text).expect("a map");
        let marks = take_landmarks(&mut map);
        assert_eq!(marks, vec![(HOME_SPAWN, 1, 1, 1), (AWAY_HOOP, 2, 1, 1)]);
        assert!(
            map.prefabs
                .iter()
                .flat_map(|p| p.objects.iter())
                .all(|o| !is_under(&o.path, LANDMARK))
        );
    }
}
