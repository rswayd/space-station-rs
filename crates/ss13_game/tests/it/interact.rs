//! Core interactions over real WebTransport on 127.0.0.1: A pulls B's dead body across the room,
//! throws a toolbox at it; grabs and resisting; examine text; crit whispers fade with distance,
//! emotes, `*me`, the radio prefix and deadchat.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_chat::server::ChatServerPlugin;
use ss13_chat::speech::SpeakerStat;
use ss13_game::combat::NextMove;
use ss13_game::health::{DamageType, Health, Stat, apply_damage};
use ss13_game::hud::{HealthHudData, HealthHudUi};
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::pull::{GrabState, HeldByGrab, PulledBy, Pulling};
use ss13_game::server::MoveQueue;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::throw::{ThrowMode, Thrown};
use ss13_mapload::DmItem;
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, PanelEmote};
use ss13_shared::ui::UiServerMessage;
use ss13_shared::{Dir, TilePos};
use ss13_tgui::UiInterface;
use ss13_world::SpatialIndex;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0,
        "throwforce": 0, "throw_speed": 2, "throw_range": 7 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute" } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8, "w_class": 3 } },
    "/obj/item/toy": { "parent": "/obj/item" },
    "/obj/item/storage": { "parent": "/obj/item" },
    "/obj/item/storage/toolbox": { "parent": "/obj/item/storage", "declared_vars": {
        "name": "toolbox", "desc": "Danger. Very robust.", "icon_state": "toolbox",
        "force": 13, "throwforce": 13, "w_class": 4 } },
    "/obj/item/toy/bolted": { "parent": "/obj/item/toy", "declared_vars": { "anchored": 1 } },
    "/obj/item/card": { "parent": "/obj/item", "declared_vars": { "inhand_icon_state": "card-id" } },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256,
        "worn_icon_state": "nothing" } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/card/id/brig": { "parent": "/obj/item/card/id", "declared_vars": {
        "access": { "list": ["brig"] } } },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color",
        "declared_vars": { "name": "grey jumpsuit" } },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/rack": { "parent": "/obj/structure", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "rack" } },
    "/obj/machinery": { "parent": "/obj" },
    "/obj/machinery/door": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "d.dmi" }, "icon_state": "door_closed", "density": 1, "opacity": 1,
        "layer": 2.7, "base_icon_state": "door", "autoclose": 0, "can_open_with_hands": 1,
        "closingLayer": 3.1, "glass": 0, "locked": 0, "safe": 1, "visible": 1, "welded": 0 } },
    "/obj/machinery/door/airlock": { "parent": "/obj/machinery/door", "declared_vars": {
        "icon": { "resource": "a.dmi" }, "icon_state": "closed", "autoclose": 1,
        "base_icon_state": null, "normalspeed": 1, "overlays_file": { "resource": "o.dmi" },
        "airlock_material": null } },
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

/// Rows top-down, the last line is y = 1. Spawns in map order: (3,3), (4,3), (5,3), (7,3), then
/// (3,4); `T` a toolbox (force and throwforce 13, bulky).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"T" = (/obj/item/storage/toolbox,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwwwww
wffffffffffw
wfsfffffTffw
wfsssfsffffw
wffffffffffw
wwwwwwwwwwww
"}
"##;

const TOOLBOX: TilePos = TilePos::new(9, 4, 1);

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 7] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toolbox"),
    ("icons/mob/simple/mob.dmi", "ghost"),
    ("l.dmi", "card-id"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("interact_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/storage/toolbox",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

/// What a client received: chat lines and its last health HUD data.
#[derive(Resource, Default)]
struct Received {
    chat: Vec<String>,
    hud: Option<HealthHudData>,
    hud_window: Option<u32>,
}

fn record(
    mut chat: MessageReader<ChatReceived>,
    mut ui: MessageReader<UiMessageReceived>,
    mut received: ResMut<Received>,
) {
    for ChatReceived(message) in chat.read() {
        received.chat.push(message.plain_text());
    }
    for UiMessageReceived(message) in ui.read() {
        let data = match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } if interface == HealthHudUi::NAME => {
                received.hud_window = Some(*window);
                data
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } if received.hud_window == Some(*window) => data,
            _ => continue,
        };
        if !data.is_empty() {
            received.hud = Some(ss13_tgui::decode(data).unwrap());
        }
    }
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str, clients: usize) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            app.add_plugins(ChatServerPlugin);
        });
        s.join_all(clients, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        Self(s)
    }

    fn get<T: Component>(&self, e: Entity) -> Option<&T> {
        self.server.world().get::<T>(e)
    }

    fn mob(&self, p: usize) -> Entity {
        self.players[p].mob
    }

    fn pos(&self, e: Entity) -> TilePos {
        *self.get::<TilePos>(e).unwrap()
    }

    fn health(&self, p: usize) -> f32 {
        self.get::<Health>(self.mob(p)).unwrap().health()
    }

    /// Damage straight through the health API on the server (test setup).
    fn hurt(&mut self, p: usize, brute: f32) {
        let mob = self.mob(p);
        let world = self.server.world_mut();
        let mut health = world.get::<Health>(mob).unwrap().clone();
        let mut stat = *world.get::<Stat>(mob).unwrap();
        apply_damage(&mut health, &mut stat, DamageType::Brute, brute);
        world.entity_mut(mob).insert((health, stat));
    }

    /// One step of `p`'s mob, waiting until the server applied it.
    fn walk(&mut self, p: usize, dir: Dir) {
        let mob = self.mob(p);
        self.players[p].seq += 1;
        let seq = self.players[p].seq;
        self.send(p, ClientIntent::Move { dir, seq });
        self.run_until("the step is applied", |s| {
            s.get::<MoveQueue>(mob).unwrap().applied() >= seq
        });
    }

    fn ctrl_click(&mut self, p: usize, entity: Entity) {
        self.wait_click_cooldown(p);
        let modifiers = ClickModifiers {
            ctrl: true,
            ..default()
        };
        self.click_with(p, entity, modifiers);
    }

    fn wait_click_cooldown(&mut self, p: usize) {
        let mob = self.mob(p);
        self.run_until("off click cooldown", |s| {
            s.now() >= s.get::<NextMove>(mob).unwrap().0
        });
    }

    fn chat(&self, p: usize) -> &[String] {
        &self.players[p].app.world().resource::<Received>().chat
    }

    fn said(&self, p: usize, text: &str) -> bool {
        self.chat(p).iter().any(|line| line.contains(text))
    }

    fn name(&self, p: usize) -> String {
        self.get::<Name>(self.mob(p)).unwrap().as_str().to_owned()
    }

    fn say(&mut self, p: usize, text: &str) {
        self.send(p, ClientIntent::Say(text.into()));
    }
}

#[test]
fn drag_a_body_then_throw_a_toolbox_at_it() {
    let mut s = Station::start("drag_a_body_then_throw_a_toolbox_at_it", 2);
    let (a, b) = (s.mob(0), s.mob(1));
    assert_eq!(
        s.pos(a).chebyshev(s.pos(b)),
        Some(1),
        "spawned next to each other"
    );
    // B dies (setup through the health API) and lies.
    s.hurt(1, 200.0);
    s.run_until("B is dead and lying", |s| {
        s.get::<Stat>(b) == Some(&Stat::Dead)
            && s.get::<ss13_game::health::BodyPosition>(b)
                .is_some_and(|p| p.lying())
    });
    let name_a = s.name(0);

    // Ctrl+click: a passive pull.
    s.ctrl_click(0, b);
    s.run_until("A pulls B", |s| s.get::<Pulling>(a) == Some(&Pulling(b)));
    assert_eq!(s.get::<PulledBy>(b).map(PulledBy::puller), Some(a));
    assert_eq!(s.get::<GrabState>(a), Some(&GrabState::Passive));
    s.run_until("B reads it", |s| {
        s.said(1, &format!("{name_a} пассивно хватает вас!"))
    });
    s.run_until("A's speed includes dragging a lying body", |s| {
        s.players[0]
            .app
            .world()
            .resource::<Received>()
            .hud
            .as_ref()
            .is_some_and(|h| (h.move_delay - 0.37).abs() < 1e-4)
    });

    // North, then three tiles east: B steps into the tile A left whenever it would end up
    // diagonal or two tiles behind.
    let mut dragged = 0;
    for dir in [Dir::North, Dir::East, Dir::East, Dir::East, Dir::East] {
        let (from_a, from_b) = (s.pos(a), s.pos(b));
        s.walk(0, dir);
        let to_a = s.pos(a);
        assert_eq!(to_a, from_a.step(dir), "A walked {dir:?}");
        let follows = ss13_game::pull::pulled_follows(to_a, from_b, false);
        let expected = if follows { from_a } else { from_b };
        assert_eq!(s.pos(b), expected, "B after A's step {dir:?}");
        dragged += usize::from(follows);
    }
    assert!(dragged >= 3, "B was dragged {dragged} tiles");
    assert_eq!(s.get::<Pulling>(a), Some(&Pulling(b)));

    // The stop pulling key.
    s.send(0, ClientIntent::StopPulling);
    s.run_until("A lets go", |s| s.get::<Pulling>(a).is_none());
    assert!(s.get::<PulledBy>(b).is_none());

    // One more step east (B stays), the toolbox next to A is picked up.
    s.walk(0, Dir::East);
    let body_at = s.pos(b);
    let toolbox = {
        let world = s.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(TOOLBOX)
            .iter()
            .copied()
            .find(|&e| world.get::<DmItem>(e).is_some())
            .unwrap()
    };
    assert_eq!(s.get::<DmItem>(toolbox).unwrap().throwforce, 13.0);
    assert_eq!(s.pos(a).chebyshev(TOOLBOX), Some(1));
    s.wait_click_cooldown(0);
    s.click(0, toolbox);
    s.run_until("A holds the toolbox", |s| {
        s.get::<Inventory>(a).unwrap().active_item() == Some(toolbox)
    });

    // Throw mode (R), a click on the body three tiles away: the toolbox flies, hits it for its
    // throwforce and lies on its tile.
    s.send(0, ClientIntent::ToggleThrowMode);
    s.run_until("throw mode on", |s| {
        s.get::<ThrowMode>(a) == Some(&ThrowMode(true))
    });
    let before = s.health(1);
    s.wait_click_cooldown(0);
    s.click(0, b);
    s.run_until("the toolbox flies", |s| {
        s.get::<Thrown>(toolbox).is_some() || s.health(1) < before
    });
    s.run_until("the toolbox hits B", |s| s.health(1) == before - 13.0);
    assert!(s.get::<Thrown>(toolbox).is_none());
    assert!(s.get::<CarriedBy>(toolbox).is_none());
    assert_eq!(s.pos(toolbox), body_at, "it drops where it hit");
    assert_eq!(s.pos(b), body_at, "a lying body is not pushed");
    assert_eq!(
        s.get::<ThrowMode>(a),
        Some(&ThrowMode(false)),
        "a throw ends throw mode"
    );
    s.run_until("everyone reads it", |s| {
        s.said(1, "В вас врезается toolbox!") && s.said(0, "toolbox врезается в")
    });
    assert!(
        s.chat(0)
            .iter()
            .any(|l| l.starts_with("Вы ") && l.contains("toolbox!"))
    );
}

#[test]
fn grab_upgrade_and_resist() {
    let mut s = Station::start("grab_upgrade_and_resist", 2);
    let (a, b) = (s.mob(0), s.mob(1));
    s.ctrl_click(0, b);
    s.run_until("passive", |s| {
        s.get::<GrabState>(a) == Some(&GrabState::Passive)
    });
    // A standing conscious mob is not held by a passive pull.
    s.hold(0.2, "not held", |s| s.get::<HeldByGrab>(b).is_none());
    s.ctrl_click(0, b);
    s.run_until("aggressive", |s| {
        s.get::<GrabState>(a) == Some(&GrabState::Aggressive)
    });
    s.run_until("B is held", |s| s.get::<HeldByGrab>(b).is_some());
    let name_a = s.name(0);
    s.run_until("B reads it", |s| {
        s.said(1, &format!("{name_a} агрессивно хватает вас!"))
    });
    // A step from an aggressive grab is a resist roll: either B got out and moved, or B stays
    // held and its steps are refused for 4 s.
    let at = s.pos(b);
    let away = if s.pos(a).x < at.x {
        Dir::East
    } else {
        Dir::West
    };
    s.walk(1, away);
    s.run_until("the roll is done", |s| {
        s.get::<Pulling>(a).is_none() || s.said(1, "неуспешно пытаетесь вырваться")
    });
    if s.get::<Pulling>(a).is_some() {
        s.walk(1, away);
        s.hold(0.5, "held: refused without a new roll", |s| s.pos(b) == at);
        // The resist key: 60% per try, `CLICK_CD_RESIST` between tries.
        for _ in 0..12 {
            if s.get::<Pulling>(a).is_none() {
                break;
            }
            s.wait_click_cooldown(1);
            let before = s.get::<NextMove>(b).unwrap().0;
            s.send(1, ClientIntent::Resist);
            s.run_until("the resist is handled", |s| {
                s.get::<NextMove>(b).unwrap().0 > before
            });
        }
        assert!(s.get::<Pulling>(a).is_none(), "B never broke free");
        // Chat reaches the client after the server already dropped the pull.
        s.run_until("B reads the escape", |s| {
            s.said(1, "Вы вырываетесь из хватки")
        });
    } else {
        s.run_until("B moved away", |s| s.pos(b) == at.step(away));
    }
    assert!(s.get::<HeldByGrab>(b).is_none());
}

#[test]
fn examine_speech_emotes_and_deadchat() {
    let mut s = Station::start("examine_speech_emotes_and_deadchat", 4);
    let names: Vec<String> = (0..4).map(|p| s.name(p)).collect();
    let d = |s: &Station, p: usize| s.pos(s.mob(0)).chebyshev(s.pos(s.mob(p))).unwrap();
    assert_eq!(
        (d(&s, 1), d(&s, 2), d(&s, 3)),
        (1, 2, 4),
        "fixture spawn layout"
    );

    // Examine a toolbox-less human and the toolbox: shift+click from P1.
    let mob0 = s.mob(0);
    s.click_with(
        1,
        mob0,
        ClickModifiers {
            shift: true,
            ..default()
        },
    );
    s.run_until("the examine title arrives", |s| {
        s.said(1, &format!("Это {}.", names[0]))
    });
    s.run_until("with the clothes", |s| s.said(1, "Он носит"));

    // Soft crit: say becomes a whisper, heard in full at 1 tile, starred at 2, seen at 4.
    s.hurt(0, 100.0);
    let p0 = s.mob(0);
    s.run_until("P0 is in soft crit for speech", |s| {
        s.get::<SpeakerStat>(p0) == Some(&SpeakerStat::SoftCrit)
    });
    s.say(0, "помогите мне");
    s.run_until("P1 hears the whisper", |s| {
        s.said(1, &format!("{} шепчет, \"Помогите мне\"", names[0]))
    });
    s.run_until("P2 hears it starred", |s| {
        s.said(2, &format!("{} шепчет, \"", names[0]))
    });
    s.run_until("P3 only sees it", |s| {
        s.said(
            3,
            &format!("{} шепчет что-то, но вы слишком далеко", names[0]),
        )
    });
    assert!(!s.said(3, "мне"), "too far to hear words");
    // Examine sees the crit (past `EXAMINE_MORE_WINDOW`, so not the closer look).
    s.run_for(1.1);
    let mob0 = s.mob(0);
    s.click_with(
        1,
        mob0,
        ClickModifiers {
            shift: true,
            ..default()
        },
    );
    s.run_until("examine shows the crit", |s| {
        s.said(1, "едва находится в сознании")
    });
    // Emotes from crit: only the crit ones.
    s.say(0, "*scream");
    s.run_until("refused", |s| {
        s.said(0, "You cannot scream while in a critical condition!")
    });
    s.say(0, "*gasp");
    s.run_until("a crit emote", |s| {
        s.said(1, &format!("{} задыхается!", names[0]))
    });

    // Emotes and `*me` from a conscious mob reach everyone in view.
    s.say(1, "*scream");
    s.run_until("P3 sees P1 scream", |s| {
        s.said(3, &format!("{} кричит!", names[1]))
    });
    s.say(1, "*me чешет затылок");
    s.run_until("a custom emote", |s| {
        s.said(2, &format!("{} чешет затылок", names[1]))
    });
    s.say(1, "*nod Бобу");
    s.run_until("an emote with a parameter", |s| {
        s.said(0, &format!("{} кивает Бобу.", names[1]))
    });
    // The radio prefix: a local whisper (this fixture gives no headsets).
    s.say(2, "; всем постам");
    s.run_until("P1 next to P2 hears the whisper", |s| {
        s.said(1, &format!("{} шепчет, \"Всем постам\"", names[2]))
    });

    // Death: speech goes to deadchat only.
    s.hurt(0, 200.0);
    s.run_until("P0 speaks dead", |s| {
        s.get::<SpeakerStat>(p0) == Some(&SpeakerStat::Dead)
    });
    s.say(0, "я умер");
    s.run_until("P0 reads deadchat", |s| s.said(0, "МЕРТВ:"));
    s.hold(0.5, "the living never hear the dead", |s| {
        (1..4).all(|p| !s.said(p, "умер"))
    });
}

/// BandaStation's chat emote panel: a standard emote, one with its own text (the emote's checks,
/// the panel's text) and `*me`; panel texts cool down 0.4 s (short) between them.
#[test]
fn panel_emotes_run_with_their_texts_and_cool_down() {
    let mut s = Station::start("panel_emotes_run_with_their_texts_and_cool_down", 2);
    let name = s.name(1);
    s.send(
        1,
        ClientIntent::PanelEmote(PanelEmote::Custom {
            key: "scream".into(),
            text: "вопит на свой лад!".into(),
        }),
    );
    s.send(
        1,
        ClientIntent::PanelEmote(PanelEmote::Me {
            text: "чешет нос".into(),
        }),
    );
    s.run_until("the custom text replaces the message", |s| {
        s.said(0, &format!("{name} вопит на свой лад!"))
    });
    assert!(!s.said(0, &format!("{name} кричит!")));
    s.run_until("the second panel text is too soon", |s| {
        s.said(1, "Не так быстро!")
    });
    assert!(!s.said(0, "чешет нос"));
    s.run_for(0.5);
    s.send(
        1,
        ClientIntent::PanelEmote(PanelEmote::Me {
            text: "чешет нос".into(),
        }),
    );
    s.run_until("after the cooldown", |s| {
        s.said(0, &format!("{name} чешет нос"))
    });
    s.send(
        1,
        ClientIntent::PanelEmote(PanelEmote::Default { key: "nod".into() }),
    );
    s.run_until("a standard emote", |s| {
        s.said(0, &format!("{name} кивает."))
    });
}
