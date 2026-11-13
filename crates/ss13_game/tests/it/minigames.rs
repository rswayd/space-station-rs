//! The ghosts' basketball (`ss13_game::minigames::basketball`): two ghosts sign up and start
//! a game; a stadium loads, each gets a pacifist body on a team; a team scores only on the other
//! team's hoop; at the end the losers turn to dust, and after the victory lap the bodies go and
//! the ghosts may re-enter the bodies they had left.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::ghost::Ghost;
use ss13_game::medical::traumas::Pacifist;
use ss13_game::minigames::basketball::{
    self, Baller, BasketballLobby, Hoop, LobbyAct, PickupRestriction, Side,
};
use ss13_game::server::Players;
use ss13_game::testing::{Station, fixture_map, write_fixture};
use ss13_mapload::DmItem;
use ss13_shared::TilePos;

const MINIGAME_TYPES: &str = r##"
    "/obj/structure/hoop": { "parent": "/obj/structure", "declared_vars": {
        "name": "basketball hoop", "icon": { "resource": "o.dmi" }, "icon_state": "hoop",
        "density": 1, "anchored": 1 } },
    "/obj/structure/hoop/minigame": { "parent": "/obj/structure/hoop" },
    "/obj/item/toy": { "parent": "/obj/item", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "w_class": 1 } },
    "/obj/item/toy/basketball": { "parent": "/obj/item/toy", "declared_vars": {
        "name": "basketball", "icon_state": "basketball", "w_class": 4 } },
    "/area/centcom": { "parent": "/area" },
    "/area/centcom/basketball": { "parent": "/area/centcom", "declared_vars": {
        "requires_power": 0 } },
    "/mob": {"##;

const MAP: &str = r##""w" = (/turf/closed/wall,/area/station)
"f" = (/turf/open/floor,/area/station)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/station)

(1,1,1) = {"
wwwww
wsffw
wfffw
wwwww
"}
"##;

/// Hoops at both ends, home spawns west, away spawns east, a ball in the middle.
const STADIUM: &str = r##""f" = (/turf/open/floor,/area/centcom/basketball)
"h" = (/obj/effect/landmark/basketball/team_spawn/home,/turf/open/floor,/area/centcom/basketball)
"a" = (/obj/effect/landmark/basketball/team_spawn/away,/turf/open/floor,/area/centcom/basketball)
"H" = (/obj/effect/landmark/basketball/team_spawn/home_hoop,/obj/structure/hoop/minigame,/turf/open/floor,/area/centcom/basketball)
"A" = (/obj/effect/landmark/basketball/team_spawn/away_hoop,/obj/structure/hoop/minigame,/turf/open/floor,/area/centcom/basketball)
"b" = (/obj/item/toy/basketball,/turf/open/floor,/area/centcom/basketball)

(1,1,1) = {"
HfffA
hfbfa
hfffa
"}
"##;

fn fixture() -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("minigames_fixture");
    std::fs::create_dir_all(dir.join(basketball::MAPS_DIR)).unwrap();
    let types = crate::power::TYPES.replacen(r#"    "/mob": {"#, MINIGAME_TYPES, 1);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/structure/hoop/minigame",
        "/obj/item/toy/basketball",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/station",
        "/area/centcom/basketball",
    ];
    let states = [
        ("t.dmi", "floor"),
        ("t.dmi", "wall"),
        ("t.dmi", ""),
        ("l.dmi", "x2"),
        ("l.dmi", "Assistant"),
        ("h.dmi", "human_basic"),
        ("o.dmi", "hoop"),
        ("i.dmi", "toy"),
        ("i.dmi", "basketball"),
    ];
    let stadiums: Vec<(String, &str)> = basketball::TEAMS
        .iter()
        .map(|t| (format!("{}/{}.dmm", basketball::MAPS_DIR, t.map), STADIUM))
        .collect();
    let mut files: Vec<(&str, &str)> = vec![("map.dmm", MAP)];
    files.extend(stadiums.iter().map(|(p, m)| (p.as_str(), *m)));
    write_fixture(&dir, &types, &used, &states, &files)
}

fn run(app: &mut App, frames: usize) {
    for _ in 0..frames {
        app.update();
    }
}

fn game(world: &World, lobby: Entity) -> Option<basketball::Game> {
    world.get::<BasketballLobby>(lobby)?.game.clone()
}

#[test]
fn ghosts_play_basketball_and_the_losers_turn_to_dust() {
    let mut st = Station::start(fixture_map(&fixture(), &["map.dmm"]), |_| {});
    let p0 = st.join("p0", |_| {});
    let p1 = st.join("p1", |_| {});
    let (c0, body0) = (st.players[p0].link, st.players[p0].mob);
    let (c1, body1) = (st.players[p1].link, st.players[p1].mob);
    let world = st.server.world_mut();
    let g0 = ss13_game::ghost::ghostize_client(world, c0).expect("a ghost");
    let g1 = ss13_game::ghost::ghostize_client(world, c1).expect("a ghost");
    let lobby = basketball::lobby(world);

    // One player is not enough; starting needs a sign-up.
    basketball::lobby_act(world, c1, g1, lobby, LobbyAct::Start);
    basketball::lobby_act(world, c0, g0, lobby, LobbyAct::Signup);
    basketball::lobby_act(world, c0, g0, lobby, LobbyAct::Start);
    assert!(game(world, lobby).is_none());
    assert_eq!(
        world.get::<BasketballLobby>(lobby).unwrap().signup,
        vec!["p0"]
    );

    basketball::lobby_act(world, c1, g1, lobby, LobbyAct::Signup);
    basketball::lobby_act(world, c1, g1, lobby, LobbyAct::Start);
    let g = game(world, lobby).expect("a game");
    assert!(
        world
            .get::<BasketballLobby>(lobby)
            .unwrap()
            .signup
            .is_empty()
    );
    assert_eq!(g.players.len(), 2);
    let home = g
        .players
        .iter()
        .find(|p| p.side == Side::Home)
        .cloned()
        .unwrap();
    let away = g
        .players
        .iter()
        .find(|p| p.side == Side::Away)
        .cloned()
        .unwrap();
    for p in [&home, &away] {
        assert_eq!(world.resource::<Players>().mob(p.client), Some(p.mob));
        assert!(world.get::<Baller>(p.mob).is_some());
        assert!(world.get::<Pacifist>(p.mob).is_some());
        assert!(g.levels.contains(&world.get::<TilePos>(p.mob).unwrap().z));
    }
    assert!(world.get_entity(g0).is_err() && world.get_entity(g1).is_err());
    assert_eq!(
        world.get::<Hoop>(g.away_hoop).unwrap().team,
        Some(vec![home.mob])
    );

    // A team scores on the other team's hoop only; the scorer's team keeps off the ball.
    let mut q = world.query::<(Entity, &DmItem)>();
    let ball = q
        .iter(world)
        .find(|(_, i)| i.path == basketball::BALL)
        .map(|(e, _)| e)
        .expect("the stadium's ball");
    assert!(!basketball::score(world, g.away_hoop, ball, away.mob, 2));
    assert!(basketball::score(world, g.away_hoop, ball, home.mob, 3));
    assert_eq!(world.get::<Hoop>(g.away_hoop).unwrap().total_score, 3);
    assert!(
        world
            .get::<PickupRestriction>(ball)
            .unwrap()
            .mobs
            .contains(&home.mob)
    );

    // Time is up: the home team won, the away player is dust and a ghost.
    let now = world.resource::<Time>().elapsed_secs_f64();
    world
        .get_mut::<BasketballLobby>(lobby)
        .unwrap()
        .game
        .as_mut()
        .unwrap()
        .ends_at = now;
    run(&mut st.server, 3);
    let world = st.server.world_mut();
    assert!(world.get_entity(away.mob).is_err());
    let away_ghost = world.resource::<Players>().mob(away.client).unwrap();
    assert!(world.get::<Ghost>(away_ghost).is_some());
    assert_eq!(world.resource::<Players>().mob(home.client), Some(home.mob));
    assert!(game(world, lobby).unwrap().cleanup_at.is_some());

    // The victory lap ends: the bodies go, the ghosts may go back to their old bodies.
    world
        .get_mut::<BasketballLobby>(lobby)
        .unwrap()
        .game
        .as_mut()
        .unwrap()
        .cleanup_at = Some(0.0);
    run(&mut st.server, 3);
    let world = st.server.world_mut();
    assert!(game(world, lobby).is_none());
    assert!(world.get_entity(home.mob).is_err());
    assert!(world.get_entity(ball).is_err());
    for (client, body) in [(c0, body0), (c1, body1)] {
        let ghost = world.resource::<Players>().mob(client).unwrap();
        assert_eq!(world.get::<Ghost>(ghost).and_then(|g| g.body), Some(body));
    }
}
