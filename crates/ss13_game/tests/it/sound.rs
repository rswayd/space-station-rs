//! Positional sound over real WebTransport on 127.0.0.1: a pistol shot (tg `fire_sounds`) is
//! heard by the shooter and a listener next to it (at tg's falloff), not by one 20 tiles away,
//! not by one in a vacuum room behind a wall, not by one wearing earmuffs. Footsteps (tg
//! `/datum/element/footstep`) come from the turf's table every second step, and a sound that
//! does not carry through walls (`ignore_walls = FALSE`) stops at one. A sound takes the
//! listener's room reverb (`/area/space`: `SOUND_AREA_SPACE`), its channel and volume preference,
//! and `stop_sound_channel` stops it.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_atmos::AtmosPlugin;
use ss13_game::combat::NextMove;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_net::sound::client::{ChannelSoundReceived, SoundReceived};
use ss13_shared::TilePos;
use ss13_shared::protocol::{
    CHANNEL_CHARGED_SPELL, ChannelSound, SOUND_ENVIRONMENT_NONE, SOUND_ENVIRONMENT_UNDERWATER,
    SoundCategory, SoundEvent,
};
use ss13_world::SpatialIndex;

const SHOT: &str = "sound/items/weapons/gun/pistol/shot.ogg";

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 }, "declared_vars": { "can_atmos_pass": 1 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute" } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/projectile": { "parent": "/obj", "declared_vars": { "name": "projectile",
        "icon": { "resource": "p.dmi" }, "icon_state": "bullet", "anchored": 1, "damage": 10,
        "damage_type": "brute", "armor_flag": "bullet", "speed": 1.25, "range": 50,
        "pass_flags": 1, "accurate_range": 100, "accuracy_falloff": 7 } },
    "/obj/projectile/bullet": { "parent": "/obj/projectile" },
    "/obj/projectile/bullet/c9mm": { "parent": "/obj/projectile/bullet", "declared_vars": {
        "name": "9mm bullet", "damage": 30 } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8, "hitsound": null } },
    "/obj/item/firing_pin": { "parent": "/obj/item", "declared_vars": {
        "default_pin_auth": 1, "fail_message": "недопустимый пользователь!" } },
    "/obj/item/ammo_casing": { "parent": "/obj/item", "declared_vars": { "name": "casing",
        "icon_state": "s-casing", "caliber": null, "projectile_type": null, "harmful": 1,
        "pellets": 1, "fire_sound": null } },
    "/obj/item/ammo_casing/c9mm": { "parent": "/obj/item/ammo_casing", "declared_vars": {
        "caliber": "9mm", "projectile_type": { "path": "/obj/projectile/bullet/c9mm" } } },
    "/obj/item/ammo_box": { "parent": "/obj/item", "declared_vars": { "max_ammo": 7,
        "start_empty": 0, "caliber": null, "ammo_type": { "path": "/obj/item/ammo_casing" } } },
    "/obj/item/ammo_box/magazine": { "parent": "/obj/item/ammo_box" },
    "/obj/item/ammo_box/magazine/m9mm": { "parent": "/obj/item/ammo_box/magazine",
        "declared_vars": { "name": "pistol magazine (9mm)", "icon_state": "9x19p",
        "max_ammo": 12, "caliber": "9mm", "ammo_type": { "path": "/obj/item/ammo_casing/c9mm" } } },
    "/obj/item/gun": { "parent": "/obj/item", "declared_vars": { "name": "gun",
        "fire_delay": 0, "burst_size": 1, "burst_delay": 2, "pinless": 0,
        "pin": { "path": "/obj/item/firing_pin" }, "spread": 0, "randomspread": 1,
        "dual_wield_spread": 24, "weapon_weight": 1, "recoil": 0,
        "fire_sound": { "resource": "sound/items/weapons/gun/pistol/shot.ogg" },
        "vary_fire_sound": 1, "fire_sound_volume": 50,
        "dry_fire_sound": { "resource": "sound/items/weapons/gun/general/dry_fire.ogg" },
        "dry_fire_sound_volume": 30 } },
    "/obj/item/gun/ballistic": { "parent": "/obj/item/gun", "declared_vars": {
        "bolt_type": 1, "casing_ejector": 1, "semi_auto": 1, "spawnwithmagazine": 1,
        "internal_magazine": 0, "spawn_magazine_type": null, "click_on_low_ammo": 1,
        "accepted_magazine_type": { "path": "/obj/item/ammo_box/magazine/m9mm" } } },
    "/obj/item/gun/ballistic/automatic": { "parent": "/obj/item/gun/ballistic" },
    "/obj/item/gun/ballistic/automatic/pistol": { "parent": "/obj/item/gun/ballistic/automatic",
        "declared_vars": { "name": "Makarov pistol", "icon_state": "pistol", "bolt_type": 4 } },
    "/obj/item/clothing": { "parent": "/obj/item", "declared_vars": { "flash_protect": 0 } },
    "/obj/item/clothing/ears": { "parent": "/obj/item/clothing", "declared_vars": {
        "slot_flags": 64 } },
    "/obj/item/clothing/ears/earmuffs": { "parent": "/obj/item/clothing/ears",
        "declared_vars": { "name": "earmuffs", "icon_state": "earmuffs" } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": {
        "slot_flags": 128, "body_parts_covered": 96 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" },
        "blocks_air": 0, "can_atmos_pass": 0, "initial_gas_mix": "o2=22;n2=82;TEMP=293.15" } },
    "/turf/open": { "parent": "/turf", "declared_vars": { "can_atmos_pass": -1,
        "planetary_atmos": 0 } },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": { "icon_state": "floor",
        "footstep": "floor", "barefootstep": "hardbarefoot" } },
    "/turf/open/space": { "parent": "/turf/open", "declared_vars": { "icon_state": "space",
        "initial_gas_mix": "TEMP=2.7" } },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1,
        "blocks_air": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom", "declared_vars": { "sound_environment": -1 } },
    "/area/space": { "parent": "/area", "declared_vars": { "sound_environment": 22 } }
  }
}"##;

/// Rows top-down, the last line is y = 1. A corridor of air with spawns at (2,4), (3,4),
/// (4,4) and (22,4); behind the wall below, a vacuum room with a spawn at (2,2).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"A" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"s" = (/turf/open/space/basic,/area/space)
"V" = (/obj/effect/landmark/start/assistant,/turf/open/space/basic,/area/space)

(1,1,1) = {"
wwwwwwwwwwwwwwwwwwwwwww
wAAAfffffffffffffffffAw
wwwwwwwwwwwwwwwwwwwwwww
wVsswwwwwwwwwwwwwwwwwww
wwwwwwwwwwwwwwwwwwwwwww
"}
"##;

const STATES: [(&str, &str); 11] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("t.dmi", "space"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "toy"),
    ("i.dmi", "pistol"),
    ("i.dmi", "9x19p"),
    ("i.dmi", "s-casing"),
    ("i.dmi", "earmuffs"),
    ("p.dmi", "bullet"),
];

const SHOOTER: TilePos = TilePos::new(2, 4, 1);
const NEAR: TilePos = TilePos::new(3, 4, 1);
const DEAF: TilePos = TilePos::new(4, 4, 1);
const FAR: TilePos = TilePos::new(22, 4, 1);
const VACUUM: TilePos = TilePos::new(2, 2, 1);

/// A corridor with a walker at (2,4) and a listener at (9,4); behind the wall below, an air-filled
/// room with a listener at (2,2).
const ROOMS: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"A" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwwww
wAffffffAfw
wwwwwwwwwww
wAfffffffww
wwwwwwwwwww
"}
"##;

const WALKER: TilePos = TilePos::new(2, 4, 1);
const CORRIDOR: TilePos = TilePos::new(9, 4, 1);
const BEHIND_WALL: TilePos = TilePos::new(2, 2, 1);

/// `GLOB.footstep[FOOTSTEP_FLOOR]` as the converter writes it.
fn sfx_ron() -> String {
    let mut data = ss13_mapload::sound::SfxData::default();
    let floor = ss13_mapload::sound::StepSound {
        files: (1..=5)
            .map(|i| format!("sound/effects/footstep/floor{i}.ogg"))
            .collect(),
        volume: 75.0,
        range: -1.0,
    };
    data.footsteps
        .entry("footstep".into())
        .or_default()
        .insert("floor".into(), floor);
    data.to_ron()
}

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("sound_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        "/obj/item/gun/ballistic/automatic/pistol",
        "/obj/item/ammo_box/magazine/m9mm",
        "/obj/item/clothing/ears/earmuffs",
        "/obj/item/clothing/shoes/sneakers/black",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    let sfx = sfx_ron();
    let files = [
        ("map.dmm", MAP),
        ("rooms.dmm", ROOMS),
        ("sfx.ron", sfx.as_str()),
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &files)
}

#[derive(Resource, Default)]
struct Heard(Vec<SoundEvent>);

fn record(
    mut sounds: MessageReader<SoundReceived>,
    mut channel_sounds: MessageReader<ChannelSoundReceived>,
    mut out: ResMut<Heard>,
) {
    out.0.extend(sounds.read().map(|s| s.0.clone()));
    // `playsound` comes as a sound datum (`ChannelSound`).
    out.0
        .extend(channel_sounds.read().filter_map(|s| s.0.sound.clone()));
}

/// Every sound datum as it came, for its channel and effects.
#[derive(Resource, Default)]
struct HeardDatums(Vec<ChannelSound>);

fn record_datums(
    mut channel_sounds: MessageReader<ChannelSoundReceived>,
    mut out: ResMut<HeardDatums>,
) {
    out.0.extend(channel_sounds.read().map(|s| s.0.clone()));
}

struct Station(testing::Station);

ss13_game::drive_station!(Station, 0);

impl Station {
    /// The player standing on `pos`.
    fn at(&self, pos: TilePos) -> usize {
        (0..self.players.len())
            .find(|&p| *self.get::<TilePos>(self.players[p].mob) == pos)
            .unwrap_or_else(|| panic!("nobody on {pos:?}"))
    }

    fn heard(&self, p: usize) -> Vec<SoundEvent> {
        self.players[p].app.world().resource::<Heard>().0.clone()
    }

    fn datums(&self, p: usize) -> Vec<ChannelSound> {
        self.players[p]
            .app
            .world()
            .resource::<HeardDatums>()
            .0
            .clone()
    }

    fn spawn_on(&mut self, p: usize, path: &str) -> Entity {
        let mob = self.players[p].mob;
        let world = self.0.server.world_mut();
        let pos = *world.get::<TilePos>(mob).unwrap();
        let item = ss13_game::construction::spawn_obj(world, path, pos).unwrap();
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        item
    }
}

#[test]
fn a_gunshot_reaches_near_listeners_only() {
    let assets = fixture_assets("a_gunshot_reaches_near_listeners_only");
    let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
        app.add_plugins(AtmosPlugin);
    });
    s.join_all(5, |app| {
        app.init_resource::<Heard>().add_systems(Update, record);
    });
    let mut s = Station(s);
    let (shooter, near, deaf, far, vacuum) = (
        s.at(SHOOTER),
        s.at(NEAR),
        s.at(DEAF),
        s.at(FAR),
        s.at(VACUUM),
    );
    let muffs = s.spawn_on(deaf, "/obj/item/clothing/ears/earmuffs");
    let deaf_mob = s.players[deaf].mob;
    s.0.server
        .world_mut()
        .get_mut::<Inventory>(deaf_mob)
        .unwrap()
        .ears = Some(muffs);
    let pistol = s.spawn_on(shooter, "/obj/item/gun/ballistic/automatic/pistol");
    let mob = s.players[shooter].mob;
    {
        let world = s.0.server.world_mut();
        let mut inventory = world.get_mut::<Inventory>(mob).unwrap();
        let hand = inventory.active;
        inventory.hands = [None, None];
        inventory.hands[hand] = Some(pistol);
    }
    let target = {
        let world = s.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(TilePos::new(10, 4, 1))
            .iter()
            .copied()
            .find(|&e| world.get::<ss13_mapload::Turf>(e).is_some())
            .unwrap()
    };
    s.run_until("the target is replicated", |s| {
        s.net_id(shooter, target).is_some()
    });
    s.0.click(shooter, target);
    s.run_until("the shot is fired", |s| s.now() < s.get::<NextMove>(mob).0);
    s.run_until("the listener next to the shooter hears it", |s| {
        s.heard(near).iter().any(|e| e.file == SHOT)
    });
    let shot = |e: &&SoundEvent| e.file == SHOT;
    // Distance 1 of 15: 50 - (1/15)^(1/2.5) * 45 = 34.77, in 1 atm of air; one tile west.
    let heard = s.heard(near);
    let near_shot = heard.iter().find(shot).unwrap();
    assert!((near_shot.volume - 34.77).abs() < 0.1, "{near_shot:?}");
    assert_eq!(near_shot.offset, Some([-1, 0]));
    // `vary`: one rand(32000, 55000) Hz for everyone.
    assert!((32000.0 / 44100.0..=55000.0 / 44100.0).contains(&near_shot.speed));
    s.run_until("the shooter hears it at full volume", |s| {
        s.heard(shooter)
            .iter()
            .any(|e| e.file == SHOT && e.volume == 50.0)
    });
    s.hold(1.0, "nobody else hears it", |s| {
        [far, vacuum, deaf]
            .iter()
            .all(|&p| s.heard(p).iter().all(|e| e.file != SHOT))
    });
}

#[test]
fn footsteps_and_sounds_that_walls_stop() {
    let assets = fixture_assets("footsteps_and_sounds_that_walls_stop");
    let mut s = testing::Station::start(fixture_map(&assets, &["rooms.dmm"]), |app| {
        app.add_plugins(AtmosPlugin);
    });
    s.join_all(3, |app| {
        app.init_resource::<Heard>().add_systems(Update, record);
    });
    let mut s = Station(s);
    let (walker, corridor, behind) = (s.at(WALKER), s.at(CORRIDOR), s.at(BEHIND_WALL));
    let shoes = s.spawn_on(walker, "/obj/item/clothing/shoes/sneakers/black");
    let mob = s.players[walker].mob;
    s.0.server
        .world_mut()
        .get_mut::<Inventory>(mob)
        .unwrap()
        .shoes = Some(shoes);
    // Two running steps east: the second one sounds, from the floor's shoe table.
    let step = |e: &SoundEvent| e.file.starts_with("sound/effects/footstep/floor");
    s.run_until("the walker takes two steps", |s| {
        s.0.try_step(walker, ss13_shared::Dir::East);
        *s.get::<TilePos>(mob) == TilePos::new(4, 4, 1)
    });
    s.run_until("the corridor listener hears the footstep", |s| {
        s.heard(corridor).iter().any(step)
    });
    // Volume 75, extrarange -1 - 6: max 8 tiles, falloff from 1 tile; 5 tiles away:
    // 75 - ((5 - 1) / 7)^(1/2.5) * (75 - 5) = 19.04. Footsteps do not vary.
    let heard = s.heard(corridor);
    let footstep = heard.iter().find(|e| step(e)).unwrap();
    assert!((footstep.volume - 19.04).abs() < 0.05, "{footstep:?}");
    assert_eq!(footstep.speed, 1.0);
    assert_eq!(heard.iter().filter(|e| step(e)).count(), 1);

    // From the walker's tile, 2 tiles from the room behind the wall.
    let at = TilePos::new(2, 4, 1);
    let through = ss13_game::sound::PlaySound::new(at, "through.ogg", 50.0, false);
    let blocked = through.clone().ignore_walls(false);
    let blocked = ss13_game::sound::PlaySound {
        sound: "blocked.ogg".into(),
        ..blocked
    };
    let world = s.0.server.world_mut();
    ss13_game::sound::playsound(world, through);
    ss13_game::sound::playsound(world, blocked);
    s.run_until(
        "the room behind the wall hears the sound that carries",
        |s| s.heard(behind).iter().any(|e| e.file == "through.ogg"),
    );
    s.run_until("the walker hears the blocked one", |s| {
        s.heard(walker).iter().any(|e| e.file == "blocked.ogg")
    });
    s.hold(0.5, "the wall stops the other", |s| {
        s.heard(behind).iter().all(|e| e.file != "blocked.ogg")
    });
}

#[test]
fn sounds_carry_the_room_their_channel_and_preference() {
    use ss13_game::sound::{PlaySound, playsound, stop_sound_channel};
    let assets = fixture_assets("sounds_carry_the_room_their_channel_and_preference");
    let mut s = testing::Station::start(fixture_map(&assets, &["rooms.dmm"]), |app| {
        app.add_plugins(AtmosPlugin);
    });
    s.join_all(3, |app| {
        app.init_resource::<HeardDatums>()
            .add_systems(Update, record_datums);
    });
    let mut s = Station(s);
    let corridor = s.at(CORRIDOR);
    let world = s.0.server.world_mut();
    let room = PlaySound::new(WALKER, "room.ogg", 50.0, false)
        .channel(CHANNEL_CHARGED_SPELL)
        .volume_preference(SoundCategory::Jukebox);
    playsound(world, room);
    playsound(
        world,
        PlaySound::new(WALKER, "dry.ogg", 50.0, false).use_reverb(false),
    );
    let file = |c: &ChannelSound, f: &str| c.sound.as_ref().is_some_and(|s| s.file == f);
    s.run_until("the corridor hears both", |s| {
        let heard = s.datums(corridor);
        heard.iter().any(|c| file(c, "room.ogg")) && heard.iter().any(|c| file(c, "dry.ogg"))
    });
    let heard = s.datums(corridor);
    let room = heard.iter().find(|c| file(c, "room.ogg")).unwrap();
    assert_eq!(room.channel, CHANNEL_CHARGED_SPELL);
    assert_eq!(room.fx.environment, SOUND_ENVIRONMENT_UNDERWATER);
    assert_eq!(room.fx.category, SoundCategory::Jukebox);
    let dry = heard.iter().find(|c| file(c, "dry.ogg")).unwrap();
    assert_eq!(
        (dry.channel, dry.fx.environment),
        (0, SOUND_ENVIRONMENT_NONE)
    );
    assert_eq!(dry.fx.category, SoundCategory::Game);

    let mob = s.players[corridor].mob;
    let world = s.0.server.world_mut();
    let client = world
        .get::<ss13_game::server::PlayerMob>(mob)
        .unwrap()
        .client;
    stop_sound_channel(world, client, CHANNEL_CHARGED_SPELL);
    s.run_until("the channel is stopped", |s| {
        s.datums(corridor)
            .iter()
            .any(|c| c.channel == CHANNEL_CHARGED_SPELL && c.sound.is_none())
    });
}
