//! Photography over a loopback server: a camera takes a picture of the tiles around what it is
//! used on and prints it into the hands, with the player in its description; film reloads an
//! empty camera; a photo goes into a wall frame, which then shows it.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_game::inventory::{CarriedBy, Inventory};
use ss13_game::service::{Camera, Photo, PictureFrame};
use ss13_game::storage::StoredIn;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_mapload::{DmItem, DmObj};
use ss13_shared::TilePos;
use ss13_tgui::interfaces::alert::AlertUi;
use ss13_tgui::server::UiWindow;

const TYPES: &str = r##"{
  "schema": 3,
  "types": {
    "/datum": { "parent": null },
    "/atom": { "parent": "/datum", "vars": {
        "name": null, "desc": null, "icon": null, "icon_state": null, "layer": 2, "plane": -4,
        "alpha": 255, "color": null, "dir": 2, "density": 0, "opacity": 0, "pixel_x": 0,
        "pixel_y": 0, "invisibility": 0 } },
    "/atom/movable": { "parent": "/atom", "declared_vars": { "layer": 3, "anchored": 0 } },
    "/obj": { "parent": "/atom/movable", "declared_vars": { "force": 0, "damtype": "brute" } },
    "/obj/effect": { "parent": "/obj" },
    "/obj/effect/landmark": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "x2", "invisibility": 101 } },
    "/obj/effect/landmark/start": { "parent": "/obj/effect/landmark" },
    "/obj/effect/landmark/start/assistant": { "parent": "/obj/effect/landmark/start" },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "card_grey", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 1,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null, "attack_speed": 8 } },
    "/obj/item/camera": { "parent": "/obj/item", "declared_vars": { "name": "camera" } },
    "/obj/item/camera_film": { "parent": "/obj/item" },
    "/obj/item/photo": { "parent": "/obj/item", "declared_vars": { "name": "photo" } },
    "/obj/item/wallframe": { "parent": "/obj/item" },
    "/obj/item/wallframe/picture": { "parent": "/obj/item/wallframe" },
    "/obj/item/stack": { "parent": "/obj/item" },
    "/obj/item/stack/sheet": { "parent": "/obj/item/stack" },
    "/obj/item/stack/sheet/mineral": { "parent": "/obj/item/stack/sheet" },
    "/obj/item/stack/sheet/mineral/wood": { "parent": "/obj/item/stack/sheet/mineral" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "density": 1, "anchored": 1,
        "icon": { "resource": "i.dmi" }, "icon_state": "card_grey" } },
    "/obj/structure/sign": { "parent": "/obj/structure", "declared_vars": { "density": 0 } },
    "/obj/structure/sign/picture_frame": { "parent": "/obj/structure/sign" },
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
    "/area": { "parent": "/atom", "declared_vars": { "requires_power": 0 } },
    "/area/space": { "parent": "/area" }
  }
}"##;

/// The player starts at (2,3); the loose items lie east of it on (3,3), the wall frame hangs
/// on (3,4).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"s" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"o" = (/obj/item/camera,/obj/item/camera_film,/obj/item/photo,/turf/open/floor,/area/space)
"P" = (/obj/structure/sign/picture_frame,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwww
wfPfffw
wsofffw
wfffffw
wwwwwww
"}
"##;

const STATES: [(&str, &str); 5] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "card_grey"),
];

const CAMERA: &str = "/obj/item/camera";
const FILM: &str = "/obj/item/camera_film";
const PHOTO: &str = "/obj/item/photo";
const FRAME: &str = "/obj/structure/sign/picture_frame";

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("photography_fixture")
        .join(test);
    let used = [
        "/obj/effect/landmark/start/assistant",
        CAMERA,
        FILM,
        PHOTO,
        FRAME,
        "/obj/item/wallframe/picture",
        "/obj/item/stack/sheet/mineral/wood",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

struct Station(testing::Station);
ss13_game::drive_station!(Station, 0);

impl Station {
    fn start(test: &str) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            app.add_plugins(ss13_chat::server::ChatServerPlugin);
        });
        s.join_all(1, |_| {});
        let mut s = Self(s);
        let mob = s.players[0].mob;
        s.server
            .world_mut()
            .entity_mut(mob)
            .insert(TilePos::new(2, 3, 1));
        for _ in 0..5 {
            s.tick();
        }
        s
    }

    fn item(&mut self, path: &str) -> Entity {
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &DmItem)>();
        q.iter(world)
            .find(|(_, i)| i.path == path)
            .map(|(e, _)| e)
            .unwrap_or_else(|| panic!("{path} on the map"))
    }

    fn obj(&mut self, path: &str) -> Entity {
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &DmObj)>();
        q.iter(world)
            .find(|(_, o)| o.path == path)
            .map(|(e, _)| e)
            .unwrap_or_else(|| panic!("{path} on the map"))
    }

    /// `item` into the active hand (out of the other one), whatever was there back onto (3,3).
    fn put_in_hand(&mut self, item: Option<Entity>) {
        let mob = self.players[0].mob;
        let world = self.server.world_mut();
        let hand = world.get::<Inventory>(mob).unwrap().active;
        for slot in &mut world.get_mut::<Inventory>(mob).unwrap().hands {
            if item.is_some() && *slot == item {
                *slot = None;
            }
        }
        if let Some(old) = world.get::<Inventory>(mob).unwrap().hands[hand] {
            world
                .entity_mut(old)
                .remove::<CarriedBy>()
                .insert(TilePos::new(3, 3, 1));
        }
        if let Some(item) = item {
            world
                .entity_mut(item)
                .remove::<(TilePos, StoredIn)>()
                .insert(CarriedBy(mob));
        }
        world.get_mut::<Inventory>(mob).unwrap().hands[hand] = item;
    }

    fn window(&mut self, interface: &str) -> Option<u32> {
        let link = self.players[0].link;
        let world = self.server.world_mut();
        let mut q = world.query::<&UiWindow>();
        q.iter(world)
            .filter(|w| w.client == link && w.interface == interface)
            .map(|w| w.id)
            .max()
    }

    /// The printed photos (with a picture).
    fn printed(&mut self) -> Vec<Entity> {
        let world = self.server.world_mut();
        let mut q = world.query::<(Entity, &Photo)>();
        q.iter(world)
            .filter(|(_, p)| p.picture.is_some())
            .map(|(e, _)| e)
            .collect()
    }
}

#[test]
fn a_camera_prints_a_photo_of_who_it_sees() {
    let mut s = Station::start("a_camera_prints_a_photo_of_who_it_sees");
    let camera = s.item(CAMERA);
    let film = s.item(FILM);
    s.run_until("the camera is loaded", |s| {
        s.try_get::<Camera>(camera)
            .is_some_and(|c| c.pictures_left == 10)
    });
    s.put_in_hand(Some(camera));
    // The film lying next to the player: the 3x3 picture around it has the player in it.
    s.click(0, film);
    s.run_until("the customisation question", |s| {
        s.window(AlertUi::NAME).is_some()
    });
    let alert = s.window(AlertUi::NAME).unwrap();
    s.act(0, alert, "choose", &[("choice", "Нет")]);
    s.run_until("a photo is printed", |s| !s.printed().is_empty());
    let photo = s.printed()[0];
    let mob = s.players[0].mob;
    assert!(
        s.get::<Inventory>(mob).holds(photo),
        "the photo is in the hands"
    );
    assert_eq!(s.get::<Camera>(camera).pictures_left, 9);
    let picture = s.get::<Photo>(photo).picture.clone().unwrap();
    assert_eq!((picture.width, picture.height), (3, 3));
    assert!(
        picture.sprites.len() >= 9,
        "nine floor tiles at least: {}",
        picture.sprites.len()
    );
    assert!(
        picture
            .desc
            .starts_with("Это фотография области размером 3 на 3 метра."),
        "{}",
        picture.desc
    );
    assert!(
        picture.desc.contains("На фотографии также имеется"),
        "the player is in the picture: {}",
        picture.desc
    );

    // Empty, then reloaded by the film.
    s.server
        .world_mut()
        .get_mut::<Camera>(camera)
        .unwrap()
        .pictures_left = 0;
    s.put_in_hand(None);
    s.server
        .world_mut()
        .entity_mut(camera)
        .insert(TilePos::new(3, 3, 1));
    s.put_in_hand(Some(film));
    s.click(0, camera);
    s.run_until("the film reloads the camera", |s| {
        s.get::<Camera>(camera).pictures_left == 10
    });
}

#[test]
fn a_wall_frame_shows_its_photo() {
    let mut s = Station::start("a_wall_frame_shows_its_photo");
    let frame = s.obj(FRAME);
    let camera = s.item(CAMERA);
    s.run_until("the frame is set up", |s| {
        s.try_get::<PictureFrame>(frame).is_some() && s.try_get::<Camera>(camera).is_some()
    });
    s.put_in_hand(Some(camera));
    s.click(0, frame);
    s.run_until("the customisation question", |s| {
        s.window(AlertUi::NAME).is_some()
    });
    let alert = s.window(AlertUi::NAME).unwrap();
    s.act(0, alert, "choose", &[("choice", "Нет")]);
    s.run_until("a photo is printed", |s| !s.printed().is_empty());
    let photo = s.printed()[0];

    s.put_in_hand(Some(photo));
    s.click(0, frame);
    s.run_until("the photo is framed", |s| {
        s.get::<PictureFrame>(frame).photo == Some(photo)
    });
    assert_eq!(
        s.try_get::<StoredIn>(photo).map(|i| i.0),
        Some(frame),
        "the photo is inside the frame"
    );
    s.put_in_hand(None);
    s.click(0, frame);
    s.run_until("the photo window opens", |s| s.window("Photo").is_some());
}
