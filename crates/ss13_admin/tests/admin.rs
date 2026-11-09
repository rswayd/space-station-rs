//! Admin tools over real WebTransport on 127.0.0.1: a fixture room, an admin listed in a config
//! directory (tg `admin_ranks.txt` + `admins.txt`) and a player.
//! - a player's admin requests are refused and change nothing;
//! - the admin jumps to the player, spawns an object, makes the player a traitor and kicks them
//!   (the kicked client does not reconnect); everything lands in the admin log file;
//! - an adminhelp reaches the online admin, the admin's reply reaches the player, the player's
//!   answer reaches the admin, and the admin resolves the ticket.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use ss13_admin::server::{AdminConfig, AdminServerPlugin, AdminSession};
use ss13_game::antag::traitor::Traitor;
use ss13_game::antag::vampire::server::Vampire;
use ss13_game::server::Players;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_net::admin::client::{AdminResponseReceived, SendAdminRequest};
use ss13_net::chat::client::ChatReceived;
use ss13_net::client::NetClientState;
use ss13_shared::TilePos;
use ss13_shared::admin::rights::*;
use ss13_shared::admin::{AdminRequest, AdminResponse, AntagRole, Ticket, TicketState};
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start",
        "declared_vars": { "icon_state": "Assistant" } },
    "/obj/item": { "parent": "/obj", "declared_vars": { "interaction_flags_item": 1 } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": { "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing" },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing" },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" } } },
    "/turf/open": { "parent": "/turf" },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor" } },
    "/turf/open/space": { "parent": "/turf/open" },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// Two spawn landmarks far apart: the admin and the player start on different tiles.
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwwwww
wsffffffffsw
wffffffffffw
wwwwwwwwwwww
"}
"##;

const STATES: [(&str, &str); 6] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("t.dmi", ""),
    ("l.dmi", "x2"),
    ("l.dmi", "Assistant"),
    ("h.dmi", "human_basic"),
];

const ID_CARD: &str = "/obj/item/card/id/advanced";

/// BandaStation's rank format; `The Admin` (ckey `theadmin`) is a Game Master.
const RANKS: &str = "Name = Moderator\nInclude = ADMIN\nExclude =\nEdit =\n\n\
                     Name = Game Master\nInclude = EVERYTHING\nExclude =\nEdit = EVERYTHING\n";
const ADMINS: &str = "# admins\nThe Admin = Game Master\n";

fn fixture(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("admin_fixture")
        .join(test);
    let _ = std::fs::remove_dir_all(&dir);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    std::fs::create_dir_all(dir.join("config")).unwrap();
    std::fs::write(dir.join("config/admin_ranks.txt"), RANKS).unwrap();
    std::fs::write(dir.join("config/admins.txt"), ADMINS).unwrap();
    dir
}

#[derive(Resource, Default)]
struct Seen {
    chat: Vec<String>,
    responses: Vec<AdminResponse>,
}

fn record(
    mut chat: MessageReader<ChatReceived>,
    mut admin: MessageReader<AdminResponseReceived>,
    mut seen: ResMut<Seen>,
) {
    seen.chat
        .extend(chat.read().map(|ChatReceived(m)| m.plain_text()));
    seen.responses
        .extend(admin.read().map(|AdminResponseReceived(r)| r.clone()));
}

/// Clients `[admin, player]`.
struct Session {
    dir: PathBuf,
    st: testing::Station,
}

impl core::ops::Deref for Session {
    type Target = testing::Station;
    fn deref(&self) -> &testing::Station {
        &self.st
    }
}

impl core::ops::DerefMut for Session {
    fn deref_mut(&mut self) -> &mut testing::Station {
        &mut self.st
    }
}

impl Drive for Session {
    fn station(&mut self) -> &mut testing::Station {
        &mut self.st
    }

    fn context(&self) -> String {
        format!(
            "; admin chat {:?}, player chat {:?}",
            self.seen(ADMIN).chat,
            self.seen(PLAYER).chat
        )
    }
}

const ADMIN: usize = 0;
const PLAYER: usize = 1;

impl Session {
    fn start(test: &str) -> Self {
        Self::start_trusting(test, true)
    }

    /// `insecure_admins`: [`AdminConfig::insecure_admins`].
    fn start_trusting(test: &str, insecure_admins: bool) -> Self {
        let dir = fixture(test);
        let config = AdminConfig {
            config_dir: Some(dir.join("config")),
            log_file: Some(dir.join("logs/admin.log")),
            // The test clients are trusted: the default validator does not authenticate.
            insecure_admins,
        };
        let mut st = testing::Station::start(fixture_map(&dir, &["map.dmm"]), |app| {
            app.insert_resource(config)
                .add_plugins((
                    ss13_tgui::server::TguiServerPlugin,
                    ss13_chat::server::ChatServerPlugin,
                ))
                .add_plugins(AdminServerPlugin);
        });
        // Joined one after the other: the first client takes the first landmark.
        for token in ["The Admin", "bob"] {
            st.join(token, |client| {
                client.init_resource::<Seen>().add_systems(Update, record);
            });
        }
        Self { dir, st }
    }

    fn seen(&self, client: usize) -> &Seen {
        self.players[client].app.world().resource::<Seen>()
    }

    fn send(&mut self, client: usize, request: AdminRequest) {
        self.players[client]
            .app
            .world_mut()
            .write_message(SendAdminRequest(request));
    }

    /// Sends `request` and returns the next `Done` or `Refused` to it.
    fn ask(&mut self, client: usize, request: AdminRequest) -> AdminResponse {
        let before = self.replies(client).len();
        self.send(client, request.clone());
        self.run_until(&format!("an answer to {request:?}"), |s| {
            s.replies(client).len() > before
        });
        self.replies(client)[before].clone()
    }

    fn replies(&self, client: usize) -> Vec<AdminResponse> {
        self.seen(client)
            .responses
            .iter()
            .filter(|r| matches!(r, AdminResponse::Done(_) | AdminResponse::Refused(_)))
            .cloned()
            .collect()
    }

    fn tickets(&self, client: usize) -> Vec<Ticket> {
        self.seen(client)
            .responses
            .iter()
            .rev()
            .find_map(|r| match r {
                AdminResponse::Tickets(t) => Some(t.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn chat_has(&self, client: usize, text: &str) -> bool {
        self.seen(client).chat.iter().any(|l| l.contains(text))
    }

    fn server_client(&mut self, token: &str) -> Entity {
        self.link_of(token).unwrap()
    }

    fn mob(&mut self, token: &str) -> Entity {
        let client = self.server_client(token);
        self.server
            .world()
            .resource::<Players>()
            .mob(client)
            .unwrap()
    }

    fn pos(&mut self, token: &str) -> TilePos {
        let mob = self.mob(token);
        *self.server.world().get::<TilePos>(mob).unwrap()
    }

    fn admin_log(&self) -> String {
        std::fs::read_to_string(self.dir.join("logs/admin.log")).unwrap_or_default()
    }
}

fn refused(response: AdminResponse) -> String {
    match response {
        AdminResponse::Refused(why) => why,
        other => panic!("expected Refused, got {other:?}"),
    }
}

fn done(response: AdminResponse) -> String {
    match response {
        AdminResponse::Done(text) => text,
        other => panic!("expected Done, got {other:?}"),
    }
}

#[test]
fn a_players_admin_requests_are_refused() {
    let mut s = Session::start("refused");
    // Rights reach the listed admin only.
    assert!(s.seen(ADMIN).responses.iter().any(|r| matches!(
        r,
        AdminResponse::Rights { rank, rights } if rank == "Game Master" && *rights == R_EVERYTHING
    )));
    assert!(
        !s.seen(PLAYER)
            .responses
            .iter()
            .any(|r| matches!(r, AdminResponse::Rights { .. }))
    );
    let admin_client = s.server_client("The Admin");
    assert!(s.server.world().get::<AdminSession>(admin_client).is_some());

    let before = s.pos("bob");
    let why = refused(s.ask(
        PLAYER,
        AdminRequest::Kick {
            ckey: "theadmin".into(),
            reason: String::new(),
        },
    ));
    assert!(why.contains("не администратор"), "{why}");
    refused(s.ask(PLAYER, AdminRequest::Spawn(ID_CARD.into())));
    refused(s.ask(PLAYER, AdminRequest::JumpTo("theadmin".into())));
    refused(s.ask(PLAYER, AdminRequest::EndRound));
    for _ in 0..10 {
        s.step();
    }
    assert_eq!(
        *s.players[ADMIN].app.world().resource::<NetClientState>(),
        NetClientState::Connected
    );
    assert_eq!(s.pos("bob"), before);
    // A deadminned admin is refused too, and readmin gives the rights back.
    done(s.ask(ADMIN, AdminRequest::Deadmin));
    let why = refused(s.ask(ADMIN, AdminRequest::Players));
    assert!(why.contains("Недостаточно прав"), "{why}");
    done(s.ask(ADMIN, AdminRequest::Readmin));
    s.send(ADMIN, AdminRequest::Players);
    s.run_until("the player list", |s| {
        matches!(
            s.seen(ADMIN).responses.last(),
            Some(AdminResponse::Players(_))
        )
    });
    let Some(AdminResponse::Players(players)) = s.seen(ADMIN).responses.last().cloned() else {
        unreachable!()
    };
    let ckeys: Vec<&str> = players.iter().map(|p| p.ckey.as_str()).collect();
    assert_eq!(ckeys, ["bob", "theadmin"]);
    assert_eq!(players[1].rank, "Game Master");
}

#[test]
fn an_admin_jumps_spawns_makes_a_traitor_and_kicks() {
    let mut s = Session::start("verbs");
    let target = s.pos("bob");
    assert_ne!(s.pos("The Admin"), target);
    done(s.ask(ADMIN, AdminRequest::JumpTo("bob".into())));
    assert_eq!(s.pos("The Admin"), target);
    s.run_until("message_admins at the admin", |s| {
        s.chat_has(ADMIN, "jumped to bob")
    });

    let on_tile = |s: &Session| {
        s.server
            .world()
            .resource::<SpatialIndex>()
            .on_tile(target)
            .len()
    };
    let before = on_tile(&s);
    done(s.ask(ADMIN, AdminRequest::Spawn(ID_CARD.into())));
    assert_eq!(on_tile(&s), before + 1, "an ID card under the admin");
    refused(s.ask(ADMIN, AdminRequest::Spawn("/obj/nope".into())));

    done(s.ask(
        ADMIN,
        AdminRequest::MakeAntag {
            ckey: "bob".into(),
            role: AntagRole::Traitor,
        },
    ));
    let bob = s.mob("bob");
    assert!(s.server.world().get::<Traitor>(bob).is_some());
    s.run_until("the traitor greeting", |s| s.chat_has(PLAYER, "Вы - "));

    done(s.ask(
        ADMIN,
        AdminRequest::Kick {
            ckey: "bob".into(),
            reason: "тест".into(),
        },
    ));
    s.run_until("bob kicked", |s| {
        matches!(
            s.players[PLAYER].app.world().resource::<NetClientState>(),
            NetClientState::Disconnected { .. }
        )
    });
    let NetClientState::Disconnected { reason } = s.players[PLAYER]
        .app
        .world()
        .resource::<NetClientState>()
        .clone()
    else {
        unreachable!()
    };
    assert!(reason.contains("кикнуты"), "{reason}");
    // Final: no reconnect. The client's reconnect backoff runs on the wall clock (`Instant`),
    // so the wait is 3 s of wall time as well as of game time.
    let (start, game) = (Instant::now(), s.now());
    while start.elapsed() < Duration::from_secs(3) || s.now() - game < 3.0 {
        s.step();
    }
    assert!(matches!(
        s.players[PLAYER].app.world().resource::<NetClientState>(),
        NetClientState::Disconnected { .. }
    ));
    let log = s.admin_log();
    for line in [
        "admin login: theadmin",
        "jumped to bob",
        "spawned /obj/item/card/id/advanced",
        "has made bob",
        "kicked bob",
    ] {
        assert!(log.contains(line), "{line:?} not in the admin log:\n{log}");
    }
}

#[test]
fn make_antag_vampire_needs_r_admin() {
    let mut s = Session::start("vampire");
    let make = || AdminRequest::MakeAntag {
        ckey: "bob".into(),
        role: AntagRole::Vampire,
    };
    let bob = s.mob("bob");
    refused(s.ask(PLAYER, make()));
    assert!(
        s.server.world().get::<Vampire>(bob).is_none(),
        "no rights, no vampire"
    );
    done(s.ask(ADMIN, make()));
    assert!(s.server.world().get::<Vampire>(bob).is_some());
    refused(s.ask(ADMIN, make()));
    let log = s.admin_log();
    assert!(
        log.contains("has made bob") && log.contains("a vampire."),
        "{log}"
    );
}

#[test]
fn an_adminhelp_reaches_the_admin_and_the_reply_reaches_the_player() {
    let mut s = Session::start("ticket");
    done(s.ask(
        PLAYER,
        AdminRequest::Adminhelp("Меня убили без причины".into()),
    ));
    s.run_until("the ticket at the admin", |s| {
        s.chat_has(ADMIN, "Тикет #1") && s.chat_has(ADMIN, "Меня убили без причины")
    });
    assert!(s.chat_has(PLAYER, "Тикет #1 был создан"));
    s.run_until("the ticket list", |s| s.tickets(ADMIN).len() == 1);
    // Nobody answered yet: the player cannot write more.
    let why = refused(s.ask(PLAYER, AdminRequest::Adminhelp("ау".into())));
    assert!(why.contains("ещё не ответил"), "{why}");

    done(s.ask(
        ADMIN,
        AdminRequest::TicketReply {
            ticket: 1,
            text: "Сейчас разберёмся".into(),
        },
    ));
    s.run_until("the reply at the player", |s| {
        s.chat_has(PLAYER, "Ответ на тикет #1") && s.chat_has(PLAYER, "Сейчас разберёмся")
    });
    s.run_until("the player's ticket", |s| {
        s.tickets(PLAYER)
            .first()
            .is_some_and(|t| t.messages.len() == 2 && t.linked_admin.as_deref() == Some("theadmin"))
    });
    done(s.ask(PLAYER, AdminRequest::Adminhelp("Спасибо".into())));
    s.run_until("the answer at the admin", |s| s.chat_has(ADMIN, "Спасибо"));
    // One message per 5 s.
    let why = refused(s.ask(PLAYER, AdminRequest::Adminhelp("ещё".into())));
    assert!(why.contains("Слишком быстро"), "{why}");

    done(s.ask(
        ADMIN,
        AdminRequest::SetTicketState {
            ticket: 1,
            state: TicketState::Resolved,
        },
    ));
    s.run_until("resolved at the player", |s| {
        s.chat_has(PLAYER, "Ваш тикет #1 был решён")
    });
    s.run_until("the resolved ticket", |s| {
        s.tickets(PLAYER)
            .first()
            .is_some_and(|t| t.state == TicketState::Resolved)
    });
    // A new adminhelp opens a new ticket.
    done(s.ask(PLAYER, AdminRequest::Adminhelp("Снова я".into())));
    s.run_until("ticket #2", |s| s.chat_has(ADMIN, "Тикет #2"));
    let log = s.admin_log();
    assert!(log.contains("Ticket #1: bob"), "{log}");
    assert!(log.contains("решил тикет #1"), "{log}");
}

/// The default token validator proves nothing: a listed ckey gets no admin session (and no
/// rights) unless the server trusts every client.
#[test]
fn unauthenticated_tokens_get_no_admin_session() {
    let mut s = Session::start_trusting("unauthenticated", false);
    let admin_client = s.server_client("The Admin");
    for _ in 0..5 {
        s.step();
    }
    assert!(s.server.world().get::<AdminSession>(admin_client).is_none());
    assert!(
        !s.seen(ADMIN)
            .responses
            .iter()
            .any(|r| matches!(r, AdminResponse::Rights { .. }))
    );
    refused(s.ask(ADMIN, AdminRequest::Spawn(ID_CARD.into())));
    let log = s.admin_log();
    assert!(
        log.contains("admin login refused: theadmin"),
        "not in the admin log:\n{log}"
    );
}

/// OOC is global, stops with the admin switch (not for admins), never reaches an ignorer; LOOC
/// is heard in range only; a mute silences both.
#[test]
fn ooc_is_global_and_looc_is_local() {
    use ss13_admin::ooc::{OocIgnored, OocPolicy};
    use ss13_shared::protocol::ClientIntent;
    let mut s = Session::start("ooc_looc");
    s.st.send(PLAYER, ClientIntent::Ooc("всем привет".into()));
    s.run_until("OOC reaches the admin", |s| {
        s.chat_has(ADMIN, "OOC: bob: всем привет")
    });
    s.run_until("OOC reaches the sender", |s| {
        s.chat_has(PLAYER, "OOC: bob: всем привет")
    });

    // Too long: cut to `MAX_MESSAGE_LEN`, never rejected wholesale by a client limit alone.
    let spam = "я".repeat(5000);
    s.st.send(PLAYER, ClientIntent::Ooc(spam));
    s.run_until("the long OOC is cut", |s| {
        s.seen(ADMIN)
            .chat
            .iter()
            .any(|l| l.starts_with("OOC: bob: я") && l.chars().count() <= 1024 + 12)
    });

    // Nine tiles apart with walls around: LOOC is not heard.
    s.st.send(
        PLAYER,
        ClientIntent::Looc {
            text: "далеко".into(),
            wall_pierce: false,
        },
    );
    s.run_until("the sender reads its LOOC", |s| {
        s.chat_has(PLAYER, "LOOC: ") && s.chat_has(PLAYER, "далеко")
    });
    s.run_until("out of range the admin sees it remotely", |s| {
        s.seen(ADMIN)
            .chat
            .iter()
            .any(|l| l.starts_with("(R)LOOC") && l.contains("далеко"))
    });
    let bob = s.mob("bob");
    let near = s.pos("The Admin");
    s.server.world_mut().entity_mut(bob).insert(near);
    s.tick();
    s.st.send(
        PLAYER,
        ClientIntent::Looc {
            text: "рядом".into(),
            wall_pierce: false,
        },
    );
    s.run_until("the admin hears it near", |s| {
        s.seen(ADMIN)
            .chat
            .iter()
            .any(|l| l.starts_with("LOOC") && l.contains("рядом"))
    });

    // The admin notice reaches everyone online.
    done(s.ask(
        ADMIN,
        AdminRequest::SetAdminNotice("Рестарт в полночь".into()),
    ));
    s.run_until("the notice", |s| s.chat_has(PLAYER, "Рестарт в полночь"));

    // The switch silences players, not admins.
    done(s.ask(ADMIN, AdminRequest::ToggleOoc));
    assert!(!s.server.world().resource::<OocPolicy>().ooc);
    let before = s.seen(ADMIN).chat.len();
    s.st.send(PLAYER, ClientIntent::Ooc("теперь нельзя".into()));
    s.run_until("the refusal", |s| {
        s.chat_has(PLAYER, "OOC глобально заглушён")
    });
    assert!(
        !s.seen(ADMIN).chat[before..]
            .iter()
            .any(|l| l.contains("теперь нельзя"))
    );
    s.st.send(ADMIN, ClientIntent::Ooc("админу можно".into()));
    s.run_until("the admin speaks anyway", |s| {
        s.chat_has(PLAYER, "OOC: theadmin: админу можно")
    });
    done(s.ask(ADMIN, AdminRequest::ToggleOoc));

    // An ignorer never gets it; a muted player cannot send.
    let admin_client = s.server_client("The Admin");
    s.server
        .world_mut()
        .entity_mut(admin_client)
        .insert(OocIgnored(std::iter::once("bob".to_owned()).collect()));
    let before = s.seen(ADMIN).chat.len();
    s.st.send(PLAYER, ClientIntent::Ooc("не для админа".into()));
    s.run_until("bob reads it himself", |s| {
        s.chat_has(PLAYER, "не для админа")
    });
    assert!(
        !s.seen(ADMIN).chat[before..]
            .iter()
            .any(|l| l.contains("не для админа"))
    );
    done(s.ask(
        ADMIN,
        AdminRequest::Mute {
            ckey: "bob".into(),
            mute: true,
        },
    ));
    s.st.send(PLAYER, ClientIntent::Ooc("из мута".into()));
    s.run_until("the mute refusal", |s| {
        s.chat_has(PLAYER, "Вы не можете использовать OOC")
    });
}

/// A mute stays with the ckey across a reconnect; banning an offline admin of a higher rank is
/// refused like banning an online one.
#[test]
fn mutes_follow_the_ckey_and_offline_admins_keep_their_rank() {
    use ss13_admin::server::Admins;
    use ss13_chat::server::Muted;
    let mut s = Session::start("mutes_and_offline_bans");
    done(s.ask(
        ADMIN,
        AdminRequest::Mute {
            ckey: "bob".into(),
            mute: true,
        },
    ));
    let bob = s.server_client("bob");
    assert!(s.server.world().get::<Muted>(bob).is_some());
    // Bob leaves and comes back: a new connection entity, muted again.
    drop(s.st.players.pop());
    s.run_until("the server drops bob", |s| s.link_of("bob").is_none());
    s.st.connect("bob", |client| {
        client.init_resource::<Seen>().add_systems(Update, record);
    });
    let again = s.server_client("bob");
    assert_ne!(again, bob);
    s.run_until("bob is muted again", |s| {
        s.server.world().get::<Muted>(again).is_some()
    });

    // An offline Game Master, and the online admin with ADMIN and BAN only.
    s.server
        .world_mut()
        .resource_mut::<Admins>()
        .grant("Boss", "Game Master", R_EVERYTHING);
    let admin_client = s.server_client("The Admin");
    s.server
        .world_mut()
        .get_mut::<AdminSession>(admin_client)
        .unwrap()
        .rights = R_ADMIN | R_BAN;
    let why = refused(s.ask(
        ADMIN,
        AdminRequest::Ban {
            ckey: "Boss".into(),
            reason: String::new(),
        },
    ));
    assert!(why.contains("больше прав"), "{why}");
    assert!(
        !s.server
            .world()
            .resource::<ss13_admin::server::Bans>()
            .0
            .contains_key("boss")
    );
    done(s.ask(
        ADMIN,
        AdminRequest::Ban {
            ckey: "nobody".into(),
            reason: String::new(),
        },
    ));
}

#[test]
fn the_permissions_panel_lists_changes_and_removes_admins_for_the_round() {
    let mut s = Session::start("permissions");
    assert!(refused(s.ask(PLAYER, AdminRequest::AdminList)).contains("не администратор"));
    let why = refused(s.ask(
        ADMIN,
        AdminRequest::SetAdminRank {
            ckey: "bob".into(),
            rank: "Nope".into(),
        },
    ));
    assert!(why.contains("Nope"), "{why}");
    done(s.ask(
        ADMIN,
        AdminRequest::SetAdminRank {
            ckey: "bob".into(),
            rank: "Moderator".into(),
        },
    ));
    let bob = s.server_client("bob");
    let session = s.server.world().get::<AdminSession>(bob).cloned().unwrap();
    assert_eq!(session.rank, "Moderator");
    assert_eq!(session.active_rights(), R_ADMIN | R_AUTOADMIN);
    s.run_until("bob gets his rights", |s| {
        s.seen(PLAYER).responses.iter().any(|r| {
            matches!(r, AdminResponse::Rights { rank, rights } if rank == "Moderator" && *rights != 0)
        })
    });
    // A Moderator has no PERMISSIONS.
    let why = refused(s.ask(PLAYER, AdminRequest::RemoveAdmin("The Admin".into())));
    assert!(why.contains("PERMISSIONS"), "{why}");
    s.send(ADMIN, AdminRequest::AdminList);
    s.run_until("the admin list shows bob online", |s| {
        s.seen(ADMIN).responses.iter().any(|r| {
            matches!(r, AdminResponse::AdminList { admins, ranks }
                if ranks.len() == 2 && admins.iter().any(|a| a.ckey == "bob" && a.online))
        })
    });
    done(s.ask(ADMIN, AdminRequest::RemoveAdmin("bob".into())));
    assert!(s.server.world().get::<AdminSession>(bob).is_none());
    s.run_until("bob loses the panel", |s| {
        s.seen(PLAYER).responses.iter().any(|r| {
            matches!(r, AdminResponse::Rights { rank, rights } if rank.is_empty() && *rights == 0)
        })
    });
    assert!(
        s.admin_log()
            .contains("removed bob from the admins list temporarily")
    );
}

#[test]
fn man_up_and_admin_sounds_reach_the_players() {
    let mut s = Session::start("man_up");
    done(s.ask(ADMIN, AdminRequest::ManUp("bob".into())));
    s.run_until("bob is told to man up", |s| {
        s.chat_has(PLAYER, "Deal with it")
    });
    let why = refused(s.ask(
        ADMIN,
        AdminRequest::PlaySound {
            file: "../secret.ogg".into(),
            volume: 50,
            scope: ss13_shared::admin::SoundScope::ZLevel,
        },
    ));
    assert!(why.contains(".ogg"), "{why}");
    done(s.ask(
        ADMIN,
        AdminRequest::PlaySound {
            file: "sound/misc/server-ready.ogg".into(),
            volume: 50,
            scope: ss13_shared::admin::SoundScope::ZLevel,
        },
    ));
    assert!(s.admin_log().contains("played a z-level sound"));
    // Bob is no admin: no sounds, no man up.
    assert!(refused(s.ask(PLAYER, AdminRequest::GlobalManUp)).contains("не администратор"));
}

#[test]
fn the_roundstart_logout_report_lists_the_dead_and_the_disconnected() {
    use ss13_admin::logout_report::{LogoutReport, PlayedBy};
    use ss13_game::health::{DamageType, Health, Stat, apply_damage};

    let mut s = Session::start("logout_report");
    // Bob dies in his body; a body whose player left the server stays behind.
    let bob = s.mob("bob");
    let world = s.server.world_mut();
    assert_eq!(
        world.get::<PlayedBy>(bob),
        Some(&PlayedBy("bob".into())),
        "a played body remembers its ckey"
    );
    let mut health = world.get::<Health>(bob).unwrap().clone();
    let mut stat = *world.get::<Stat>(bob).unwrap();
    apply_damage(&mut health, &mut stat, DamageType::Brute, 500.0);
    assert_eq!(stat, Stat::Dead);
    world.entity_mut(bob).insert((health, stat));
    world.spawn((
        Health::default(),
        Stat::Conscious,
        Name::new("Gus"),
        PlayedBy("gone".into()),
    ));
    world.resource_mut::<LogoutReport>().delay_secs = Some(0.0);
    s.run_until("the admins get the report", |s| {
        s.chat_has(ADMIN, "Roundstart logout report")
    });
    assert!(s.chat_has(ADMIN, "(bob), the "), "{:?}", s.seen(ADMIN).chat);
    assert!(s.chat_has(ADMIN, "(Dead)"));
    assert!(s.chat_has(ADMIN, "Gus (gone), the  (Disconnected)"));
    // The admin is alive and connected; players never see the report.
    assert!(!s.chat_has(ADMIN, "(theadmin)"));
    assert!(!s.chat_has(PLAYER, "Roundstart logout report"));
    assert!(s.admin_log().contains("Roundstart logout report"));
    assert!(s.server.world().resource::<LogoutReport>().sent);
}

#[test]
fn view_variables_lists_and_edits_reflected_fields_and_subtypes_list_children() {
    use ss13_shared::admin::VvValue;
    let mut s = Session::start("view_variables");
    // A player gets nothing.
    refused(s.ask(PLAYER, AdminRequest::VvTile));

    let last = |s: &Session, pick: fn(&AdminResponse) -> bool| {
        s.seen(ADMIN)
            .responses
            .iter()
            .rev()
            .find(|r| pick(r))
            .cloned()
    };
    s.send(ADMIN, AdminRequest::VvTile);
    s.run_until("the own tile's entities", |s| {
        last(s, |r| matches!(r, AdminResponse::VvTile(_))).is_some()
    });
    let Some(AdminResponse::VvTile(refs)) = last(&s, |r| matches!(r, AdminResponse::VvTile(_)))
    else {
        unreachable!()
    };
    let own = refs.first().expect("the own mob is listed first").id;

    // Rights are checked before the id is resolved.
    refused(s.ask(PLAYER, AdminRequest::ViewVariables(own)));

    s.send(ADMIN, AdminRequest::ViewVariables(own));
    s.run_until("the variables", |s| {
        last(s, |r| matches!(r, AdminResponse::Variables(_))).is_some()
    });
    let Some(AdminResponse::Variables(entity)) =
        last(&s, |r| matches!(r, AdminResponse::Variables(_)))
    else {
        unreachable!()
    };
    assert_eq!(entity.id, own);
    let (component, field, old) = entity
        .components
        .iter()
        .filter(|c| c.reflected)
        .flat_map(|c| c.fields.iter().map(move |f| (c, f)))
        .find_map(|(c, f)| match f.value {
            VvValue::Bool(v) => Some((c.name.clone(), f.name.clone(), v)),
            _ => None,
        })
        .expect("a reflected boolean field on a human");
    let edit = |value| AdminRequest::EditVariable {
        id: own,
        component: component.clone(),
        field: field.clone(),
        value,
    };
    refused(s.ask(PLAYER, edit(VvValue::Bool(!old))));
    // Wrong kind for the field.
    refused(s.ask(ADMIN, edit(VvValue::Text("x".into()))));

    let before = s.seen(ADMIN).responses.len();
    s.send(ADMIN, edit(VvValue::Bool(!old)));
    s.run_until("the edited variables", |s| {
        s.seen(ADMIN).responses[before..]
            .iter()
            .any(|r| matches!(r, AdminResponse::Variables(_)))
    });
    let edited = s.seen(ADMIN).responses[before..]
        .iter()
        .find_map(|r| match r {
            AdminResponse::Variables(e) => Some(e.clone()),
            _ => None,
        })
        .unwrap();
    let now = edited
        .components
        .iter()
        .find(|c| c.name == component)
        .and_then(|c| c.fields.iter().find(|f| f.name == field))
        .map(|f| f.value.clone());
    assert_eq!(now, Some(VvValue::Bool(!old)));
    assert!(s.admin_log().contains(&format!("{component}.{field}")));

    // The subtype vendor's list.
    refused(s.ask(PLAYER, AdminRequest::Subtypes("/obj/item/clothing".into())));
    s.send(ADMIN, AdminRequest::Subtypes("/obj/item/clothing".into()));
    s.run_until("the subtypes", |s| {
        last(s, |r| matches!(r, AdminResponse::Prototypes { .. })).is_some()
    });
    let Some(AdminResponse::Prototypes { ids, .. }) =
        last(&s, |r| matches!(r, AdminResponse::Prototypes { .. }))
    else {
        unreachable!()
    };
    assert!(
        ids.iter()
            .any(|i| i == "/obj/item/clothing/under/color/grey"),
        "{ids:?}"
    );
    assert!(
        ids.iter()
            .any(|i| i == "/obj/item/clothing/shoes/sneakers/black"),
        "{ids:?}"
    );
    assert!(!ids.iter().any(|i| i == ID_CARD), "{ids:?}");
}
