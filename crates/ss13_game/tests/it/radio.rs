//! Radio over real WebTransport on 127.0.0.1: two engineers and an assistant spawn in their
//! outfit headsets far out of earshot of each other. `:e` reaches only engineering headsets, `;`
//! every headset, a broadcasting intercom relays the speech next to it, a listening intercom is
//! heard without a headset, an intercom in an unpowered area neither talks nor listens.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_chat::server::ChatServerPlugin;
use ss13_chat::speech::EarsWorn;
use ss13_game::inventory::Inventory;
use ss13_game::jobs::fallback_jobs;
use ss13_game::power::AreaPower;
use ss13_game::radio::Radio;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::Areas;
use ss13_mapload::convert::DmTypes;
use ss13_mapload::jobs::{JOBS_FILE, JobsData, OutfitSlot};
use ss13_net::chat::client::ChatReceived;
use ss13_shared::TilePos;
use ss13_shared::chat::ChatChannel;
use ss13_shared::protocol::ClientIntent;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable" },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark",
        "vars": { "name": "start" }, "declared_vars": { "name": "start" } },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start",
        "vars": { "name": "Assistant" }, "declared_vars": { "name": "Assistant" } },
    "/obj/effect/landmark/start/station_engineer": { "parent": "/obj/effect/landmark/start",
        "vars": { "name": "Station Engineer" }, "declared_vars": { "name": "Station Engineer" } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/item/encryptionkey": { "parent": "/obj/item", "declared_vars": {
        "channels": { "list": [] }, "special_channels": 0 } },
    "/obj/item/encryptionkey/headset_eng": { "parent": "/obj/item/encryptionkey",
        "declared_vars": { "channels": { "list": [{ "assoc": ["Инженерия", 1] }] } } },
    "/obj/item/radio": { "parent": "/obj/item", "declared_vars": { "name": "station bounced radio",
        "on": 1, "frequency": 1459, "broadcasting": 0, "listening": 1, "canhear_range": 3,
        "subspace_transmission": 0, "freerange": 0, "keyslot": null, "special_channels": 0,
        "slot_flags": 512 } },
    "/obj/item/radio/headset": { "parent": "/obj/item/radio", "declared_vars": {
        "name": "radio headset", "canhear_range": 0, "subspace_transmission": 1,
        "keyslot2": null, "slot_flags": 16 } },
    "/obj/item/radio/headset/headset_eng": { "parent": "/obj/item/radio/headset",
        "declared_vars": { "keyslot": { "path": "/obj/item/encryptionkey/headset_eng" } } },
    "/obj/item/radio/intercom": { "parent": "/obj/item/radio", "declared_vars": {
        "name": "station intercom", "anchored": 1, "canhear_range": 2 } },
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
    "/area/space": { "parent": "/area" },
    "/area/dark": { "parent": "/area", "declared_vars": { "requires_power": 1,
        "power_equip": 0, "power_light": 0, "power_environ": 0 } }
  }
}"##;

/// Rows top-down, the last line is y = 1. Assistant starts `s` (3,3), (15,3), (27,3); engineer
/// starts `E` (3,10) and (15,10). `L` (28,4) a listening intercom, `R` (26,2) a broadcasting one
/// that does not listen, `D` (4,10) a listening intercom in the unpowered area (`d` tiles).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"d" = (/turf/open/floor,/area/dark)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"E" = (/obj/effect/landmark/start/station_engineer,/turf/open/floor,/area/space)
"e" = (/obj/effect/landmark/start/station_engineer,/turf/open/floor,/area/dark)
"L" = (/obj/item/radio/intercom,/turf/open/floor,/area/space)
"R" = (/obj/item/radio/intercom{broadcasting = 1; listening = 0; name = "Station Intercom (Court)"},/turf/open/floor,/area/space)
"D" = (/obj/item/radio/intercom,/turf/open/floor,/area/dark)

(1,1,1) = {"
wwwwwwwwwwwwwwwwwwwwwwwwwwwwww
wdeDdddfffffffEffffffffffffffw
wddddddffffffffffffffffffffffw
wddddddffffffffffffffffffffffw
wddddddffffffffffffffffffffffw
wddddddffffffffffffffffffffffw
wddddddffffffffffffffffffffffw
wffffffffffffffffffffffffffLfw
wfsfffffffffffsfffffffffffsffw
wffffffffffffffffffffffffRfffw
wwwwwwwwwwwwwwwwwwwwwwwwwwwwww
"}
"##;

const ENGINEER: &str = "Station Engineer";

/// Manifest order = `IconStateId`.
const STATES: [(&str, &str); 5] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
];

/// The assistant with a plain headset, an engineer with the engineering one.
fn fixture_jobs() -> JobsData {
    let mut data = fallback_jobs();
    let outfit = data.jobs[0].outfit.as_mut().unwrap();
    outfit
        .items
        .push((OutfitSlot::Ears, "/obj/item/radio/headset".into()));
    let mut engineer = data.jobs[0].clone();
    engineer.path = "/datum/job/station_engineer".into();
    engineer.title = ENGINEER.into();
    let outfit = engineer.outfit.as_mut().unwrap();
    outfit.items.pop();
    outfit.items.push((
        OutfitSlot::Ears,
        "/obj/item/radio/headset/headset_eng".into(),
    ));
    data.jobs.push(engineer);
    data.departments[0].jobs = vec![0, 1];
    data
}

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("radio_fixture")
        .join(test);
    let types = DmTypes::parse(TYPES.as_bytes()).unwrap();
    let data = fixture_jobs();
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/landmark/start/station_engineer",
        "/obj/item/radio/intercom",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
        "/area/dark",
    ];
    // As `ss13-mapload convert` does: types the fixture lacks (the survival box) are skipped.
    used.extend(
        data.outfit_items()
            .into_iter()
            .filter(|p| types.types.contains_key(*p)),
    );
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)]);
    std::fs::write(dir.join(JOBS_FILE), data.to_ron()).unwrap();
    dir
}

/// Chat lines a client received, with their channel.
#[derive(Resource, Default)]
struct Received(Vec<(ChatChannel, String)>);

fn record(mut chat: MessageReader<ChatReceived>, mut received: ResMut<Received>) {
    for ChatReceived(message) in chat.read() {
        received.0.push((message.channel, message.plain_text()));
    }
}

struct Station(testing::Station);

impl Deref for Station {
    type Target = testing::Station;
    fn deref(&self) -> &testing::Station {
        &self.0
    }
}

impl DerefMut for Station {
    fn deref_mut(&mut self) -> &mut testing::Station {
        &mut self.0
    }
}

impl Drive for Station {
    fn station(&mut self) -> &mut testing::Station {
        &mut self.0
    }

    fn context(&self) -> String {
        let chat: Vec<_> = self
            .players
            .iter()
            .map(|p| &p.app.world().resource::<Received>().0)
            .collect();
        format!("; chat: {chat:?}")
    }
}

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

    fn mob(&self, p: usize) -> Entity {
        self.controlled(p)
    }

    fn pos(&self, p: usize) -> TilePos {
        *self.server.world().get::<TilePos>(self.mob(p)).unwrap()
    }

    fn name(&self, p: usize) -> String {
        let world = self.server.world();
        world.get::<Name>(self.mob(p)).unwrap().as_str().to_owned()
    }

    fn say(&mut self, p: usize, text: &str) {
        self.send(p, ClientIntent::Say(text.into()));
    }

    /// A radio line `p` received containing `text`.
    fn radio(&self, p: usize, text: &str) -> bool {
        self.players[p]
            .app
            .world()
            .resource::<Received>()
            .0
            .iter()
            .any(|(channel, line)| *channel == ChatChannel::Radio && line.contains(text))
    }

    fn heard(&self, p: usize, text: &str) -> bool {
        let received = &self.players[p].app.world().resource::<Received>().0;
        received.iter().any(|(_, line)| line.contains(text))
    }

    /// Takes the headset off `p` and deletes it (test setup).
    fn drop_headset(&mut self, p: usize) {
        let mob = self.mob(p);
        let world = self.server.world_mut();
        let ears = world.get::<Inventory>(mob).unwrap().ears.unwrap();
        world.despawn(ears);
    }
}

#[test]
fn department_common_intercoms_and_power() {
    let mut s = Station::start("department_common_intercoms_and_power", 3);
    // P0 and P1 become engineers (tg `AttemptLateSpawn` after the menu).
    let engineer = 1;
    for p in [0, 1] {
        let link = s.players[p].link;
        ss13_game::jobs::join(s.server.world_mut(), link, engineer, false);
    }
    s.run_until("bodies settle", |s| {
        s.server.world().get::<EarsWorn>(s.mob(0)).is_some()
            && s.server.world().get::<EarsWorn>(s.mob(2)).is_some()
    });
    let mut engineers = [s.pos(0), s.pos(1)];
    engineers.sort_by_key(|p| p.x);
    assert_eq!(
        (engineers, s.pos(2)),
        (
            [TilePos::new(3, 10, 1), TilePos::new(15, 10, 1)],
            TilePos::new(27, 3, 1)
        ),
        "fixture spawn layout: everyone out of earshot"
    );
    // A is the engineer in the dark corner (next to the unpowered intercom).
    let (a, b) = if s.pos(0).x == 3 { (0, 1) } else { (1, 0) };
    let names: Vec<String> = (0..3).map(|p| s.name(p)).collect();
    // Outfits put the headsets into the ears slot.
    let ears = |s: &Station, p: usize| {
        let world = s.server.world();
        let headset = world.get::<Inventory>(s.mob(p)).unwrap().ears.unwrap();
        world.get::<Radio>(headset).unwrap().channels.clone()
    };
    assert_eq!(ears(&s, a), vec![("Инженерия".to_owned(), true)]);
    assert!(ears(&s, 2).is_empty(), "the assistant's plain headset");

    // `:e`: the engineers hear it on the engineering channel, the assistant does not.
    s.say(a, ":e Двигатель запущен");
    let line = format!("[Инженерия] {} говорит, \"Двигатель запущен\"", names[a]);
    s.run_until("B hears engineering", |s| s.radio(b, &line));
    s.run_until("A hears itself", |s| s.radio(a, &line));
    // Next to the speaker it is a whisper (`REDUCE_RANGE`); the speaker reads its own.
    s.run_until("A's own whisper", |s| {
        s.heard(a, &format!("{} шепчет, \"Двигатель запущен\"", names[a]))
    });
    s.hold(0.5, "a plain headset has no engineering", |s| {
        !s.heard(2, "Двигатель")
    });
    // Cyrillic layout key, same channel.
    s.say(b, ":у Принято");
    s.run_until("A hears :у", |s| {
        s.radio(a, &format!("[Инженерия] {} говорит, \"Принято\"", names[b]))
    });

    // `;`: every headset on Common.
    s.say(2, "; всем постам");
    let common = format!("[Общий] {} говорит, \"Всем постам\"", names[2]);
    for p in 0..3 {
        s.run_until("everyone with a headset hears Common", |s| {
            s.radio(p, &common)
        });
    }

    // A broadcasting intercom relays what is said next to it (here: by C, 1 tile away).
    s.say(2, "Тест связи");
    let relayed = format!("[Общий] {} говорит, \"Тест связи\"", names[2]);
    s.run_until("the intercom relays C", |s| {
        s.radio(a, &relayed) && s.radio(b, &relayed)
    });

    // Without headsets: C hears Common through the listening intercom at 1 tile, A's intercom
    // has no power.
    s.drop_headset(a);
    s.drop_headset(2);
    s.run_until("the ears slots empty", |s| {
        s.server.world().get::<EarsWorn>(s.mob(a)).is_none()
            && s.server.world().get::<EarsWorn>(s.mob(2)).is_none()
    });
    s.say(b, "; приём");
    let reply = format!("[Общий] {} говорит, \"Приём\"", names[b]);
    s.run_until("C hears it from the intercom", |s| s.radio(2, &reply));
    s.hold(0.5, "the unpowered intercom is silent", |s| {
        !s.heard(a, "Приём")
    });

    // `:i` talks into intercoms within 1 tile: C's two reach B; A's unpowered one does not.
    s.say(2, ":i через интерком");
    let via = format!("[Общий] {} говорит, \"Через интерком\"", names[2]);
    s.run_until("B hears C's intercom", |s| s.radio(b, &via));
    s.say(a, ":i из темноты");
    s.hold(0.5, "an unpowered intercom does not transmit", |s| {
        !s.radio(b, "Из темноты")
    });
    // Without a headset a department key is ordinary speech (nobody is near A).
    s.say(a, ":e никто");
    s.hold(0.5, "no radio without a headset", |s| !s.radio(b, "Никто"));

    // Power back in A's area (as an APC would switch it): the same intercom works both ways.
    let pos = s.pos(a);
    let world = s.server.world_mut();
    let area = world.resource::<Areas>().area_at(pos).unwrap();
    world.get_mut::<AreaPower>(area).unwrap().power = [true; 3];
    s.say(b, "; снова");
    s.run_until("A hears through the powered intercom", |s| {
        s.radio(a, &format!("[Общий] {} говорит, \"Снова\"", names[b]))
    });
    s.say(a, ":i светло");
    s.run_until("and talks into it", |s| {
        s.radio(b, &format!("[Общий] {} говорит, \"Светло\"", names[a]))
    });
}

/// Communications Blackout (`ss13_game::events`, forced as an admin would): the telecomms are
/// down, so headsets neither send nor receive, while a broadcasting intercom still reaches a
/// listening one on the level (tg's mundane `backup_transmission`); once the telecomms are back,
/// headsets work again.
#[test]
fn communications_blackout_silences_headsets() {
    use ss13_game::radio::TelecommsOutage;
    let mut s = Station::start("communications_blackout_silences_headsets", 3);
    s.run_until("bodies settle", |s| {
        (0..3).all(|p| s.server.world().get::<EarsWorn>(s.mob(p)).is_some())
    });
    // P2 stands by both intercoms (27,3).
    let c = (0..3)
        .find(|&p| s.pos(p) == TilePos::new(27, 3, 1))
        .unwrap();
    let (a, b) = match c {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    };
    let names: Vec<String> = (0..3).map(|p| s.name(p)).collect();
    // B also has a silicon's built-in radio (a subspace headset): it goes quiet with them.
    let b_mob = s.mob(b);
    let headset = s
        .server
        .world()
        .get::<Inventory>(b_mob)
        .unwrap()
        .ears
        .unwrap();
    let radio = s.server.world().get::<Radio>(headset).unwrap().clone();
    s.server
        .world_mut()
        .spawn((radio, ss13_game::radio::BuiltInRadio { listener: b_mob }));
    s.say(a, "; до сбоя");
    s.run_until("headsets work before", |s| s.radio(b, "До сбоя"));

    ss13_game::events::force_event(s.server.world_mut(), "Communications Blackout", None).unwrap();
    s.run_until("the telecomms go down", |s| {
        let world = s.server.world();
        world.resource::<TelecommsOutage>().until > world.resource::<Time>().elapsed_secs_f64()
    });
    let world = s.server.world();
    let left =
        world.resource::<TelecommsOutage>().until - world.resource::<Time>().elapsed_secs_f64();
    assert!((296.0..=302.0).contains(&left), "{left} s");

    s.say(a, "; тишина в эфире");
    s.hold(1.0, "no headset hears it", |s| {
        (0..3).all(|p| !s.radio(p, "Тишина в эфире"))
    });
    // Said by the broadcasting intercom: the listening one next to C still hears it.
    s.say(c, "Связь через интерком");
    let relayed = format!("[Общий] {} говорит, \"Связь через интерком\"", names[c]);
    s.run_until("the listening intercom hears the mundane broadcast", |s| {
        s.radio(c, &relayed)
    });
    s.hold(0.5, "headsets do not", |s| {
        !s.radio(a, &relayed) && !s.radio(b, &relayed)
    });

    // The telecomms come back.
    s.server.world_mut().resource_mut::<TelecommsOutage>().until = 0.0;
    s.say(a, "; снова в эфире");
    s.run_until("headsets work again", |s| s.radio(b, "Снова в эфире"));
}
