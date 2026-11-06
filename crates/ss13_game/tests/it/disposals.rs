//! Disposals.
//!
//! Over real WebTransport on 127.0.0.1 (fixture station: a bin, a mail sorting junction tagged
//! for medbay, two outlets): an item put into the bin and flushed from the `DisposalUnit` window
//! comes out of the straight outlet; a pen wrapped with the package wrapper and tagged for
//! medbay with the destination tagger (its `DestinationTagger` window) takes the sorted branch;
//! a player dragged into the bin rides to the outlet and comes out.
//!
//! MetaStation (`--ignored`): the network's build time, every bin flushed with an item (where
//! it lands), a parcel tagged for the cargo bay reaching the mail room, the cost of a holder
//! step.
//!
//! `SS13_ASSETS=<dir> cargo test -p ss13_game --all-features --test it disposals:: -- --ignored --nocapture`

use std::collections::{BTreeMap, HashMap};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_atmos::AtmosPlugin;
use ss13_game::closets::InCloset;
use ss13_game::disposals::{
    Delivery, DestTagger, DisposalBin, DisposalHolder, DisposalOutlet, DisposalReport,
    DisposalStats,
};
use ss13_game::inventory::Inventory;
use ss13_game::storage::StoredIn;
use ss13_game::testing::{self, Drive, fixture_map, write_fixture};
use ss13_game::tools::DoAfter;
use ss13_mapload::{Areas, DmItem, DmName, MapLoadPlugin};
use ss13_net::chat::client::ChatReceived;
use ss13_net::ui::client::UiMessageReceived;
use ss13_shared::TilePos;
use ss13_shared::protocol::{ClickModifiers, ClientIntent, DragEnd, InventorySlot, TICK_DURATION};
use ss13_shared::ui::UiServerMessage;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::destination_tagger::DestinationTaggerUi;
use ss13_tgui::interfaces::disposal_unit::{DisposalUnitData, DisposalUnitUi};
use ss13_world::SpatialIndex;

fn metastation_map() -> MapLoadPlugin {
    MapLoadPlugin {
        maps: vec!["MetaStation".into()],
        ..MapLoadPlugin::from_env().unwrap_or(MapLoadPlugin {
            source: env!("SS13_SOURCE").into(),
            maps: vec![],
            assets: std::env::var("SS13_ASSETS")
                .unwrap_or_else(|_| "assets/generated".into())
                .into(),
        })
    }
}

fn server_app(map: MapLoadPlugin) -> App {
    testing::server_app(map, |app| {
        app.add_plugins(AtmosPlugin);
    })
}

fn area_name(world: &World, pos: TilePos) -> String {
    world
        .resource::<Areas>()
        .area_at(pos)
        .and_then(|a| world.get::<DmName>(a))
        .map(|n| n.name.clone())
        .unwrap_or_default()
}

fn holders(server: &mut App) -> usize {
    let world = server.world_mut();
    world.query::<&DisposalHolder>().iter(world).count()
}

/// Runs until no holder is left (at most `secs`).
fn settle(server: &mut App, secs: f64) {
    let frames = (secs / TICK_DURATION.as_secs_f64()) as usize;
    for i in 0..frames {
        server.update();
        // Flushes start on the 2 s process and take 1.5 s.
        if i > 100 && holders(server) == 0 {
            // Thrown things land.
            for _ in 0..60 {
                server.update();
            }
            return;
        }
    }
}

#[test]
#[ignore = "needs generated assets and MetaStation; run with --ignored"]
fn metastation() {
    let mut server = server_app(metastation_map());
    server.update();
    let report = server.world().resource::<DisposalReport>().clone();
    println!("MetaStation disposals: {report:#?}");
    assert!(report.pipes > 1000 && report.bins >= 50 && report.outlets >= 10);
    assert!(report.sorting >= 30, "mail sorting junctions");
    // Wall clock: the gate runs this next to station_maps, so 3x the ~0.05 s idle build time.
    assert!(
        report.build_secs < 0.15,
        "network build {} s",
        report.build_secs
    );

    // Every bin gets an item and its handle pulled.
    let world = server.world_mut();
    let mut bins: Vec<(Entity, TilePos, bool)> = world
        .query::<(Entity, &DisposalBin, &TilePos)>()
        .iter(world)
        .map(|(e, b, &p)| (e, p, b.chute))
        .collect();
    bins.sort_by_key(|b| b.0);
    let outlets: Vec<TilePos> = world
        .query_filtered::<&TilePos, With<DisposalOutlet>>()
        .iter(world)
        .copied()
        .collect();
    for &o in &outlets {
        println!("outlet {o:?} in {}", area_name(world, o));
    }
    let sorting: Vec<(TilePos, Vec<u8>)> = world
        .query::<(&ss13_game::disposals::DisposalPipe, &TilePos)>()
        .iter(world)
        .filter_map(|(p, &pos)| match &p.pipe.kind {
            ss13_game::disposals::PipeKind::Sorting(ss13_game::disposals::SortFilter::Mail(t)) => {
                Some((pos, t.clone()))
            }
            _ => None,
        })
        .collect();
    println!("mail junctions: {sorting:?}");
    let mut items = Vec::new();
    for &(bin, pos, chute) in &bins {
        if chute {
            continue;
        }
        let item = ss13_game::construction::spawn_obj(world, "/obj/item/pen", pos).unwrap();
        ss13_game::storage::put_inside(world, item, bin);
        world.get_mut::<DisposalBin>(bin).unwrap().flush = true;
        items.push((item, pos));
    }
    settle(&mut server, 120.0);
    let world = server.world_mut();
    let mut landed: BTreeMap<String, usize> = BTreeMap::new();
    let mut lost = Vec::new();
    for &(item, from) in &items {
        let Some(&at) = world.get::<TilePos>(item) else {
            lost.push((from, None));
            continue;
        };
        let near_outlet = outlets
            .iter()
            .any(|o| o.chebyshev(at).is_some_and(|d| d <= 3));
        if !near_outlet {
            lost.push((from, Some(at)));
        }
        *landed.entry(area_name(world, at)).or_default() += 1;
    }
    let stats = world.resource::<DisposalStats>().clone();
    println!("{} items flushed, landed by area: {landed:#?}", items.len());
    println!("not at an outlet: {lost:?}");
    println!(
        "{} flushes, {} holder steps, {:.2} µs per step",
        stats.flushes,
        stats.steps,
        stats.move_secs * 1e6 / stats.steps.max(1) as f64
    );
    assert!(stats.flushes as usize >= items.len());
    assert!(lost.is_empty(), "every item reaches an outlet: {lost:?}");
    // The station loop ends in the delivery office (the mail room), which sorts by hand and
    // sends the rest on to waste disposal through its chute.
    assert!(landed.get("Delivery Office").copied().unwrap_or(0) * 10 >= items.len() * 8);

    // Each delivery chute flushes a pen; the delivery office's goes to waste disposal.
    let mut chuted = Vec::new();
    for &(chute, pos, is_chute) in &bins {
        if !is_chute {
            continue;
        }
        let item = ss13_game::construction::spawn_obj(world, "/obj/item/pen", pos).unwrap();
        ss13_game::storage::put_inside(world, item, chute);
        ss13_game::disposals::start_flush(world, chute);
        chuted.push((item, area_name(world, pos)));
    }
    settle(&mut server, 60.0);
    let world = server.world_mut();
    let chuted: Vec<(String, String)> = chuted
        .into_iter()
        .map(|(item, from)| {
            let to = world
                .get::<TilePos>(item)
                .map(|&at| area_name(world, at))
                .unwrap_or_default();
            (from, to)
        })
        .collect();
    println!("chutes (from, landed in): {chuted:?}");
    assert!(
        chuted
            .iter()
            .any(|(from, to)| from == "Delivery Office" && to == "Waste Disposal")
    );

    // A parcel tagged for the cargo bay (2), flushed from the first bin, reaches the mail room.
    let (bin, pos, _) = *bins.iter().find(|b| !b.2).unwrap();
    let parcel =
        ss13_game::construction::spawn_obj(world, "/obj/item/delivery/small", pos).unwrap();
    world.entity_mut(parcel).insert(Delivery { sort_tag: 2 });
    ss13_game::storage::put_inside(world, parcel, bin);
    world.get_mut::<DisposalBin>(bin).unwrap().flush = true;
    // The bin recharges first (it was flushed above).
    for _ in 0..(180.0 / TICK_DURATION.as_secs_f64()) as usize {
        server.update();
        if server.world().get::<TilePos>(parcel).is_some() {
            break;
        }
    }
    for _ in 0..60 {
        server.update();
    }
    let world = server.world();
    let at = *world.get::<TilePos>(parcel).expect("the parcel came out");
    println!(
        "cargo parcel from {pos:?} landed at {at:?} in {}",
        area_name(world, at)
    );
    // Its sort junction sends it up to the cargo bay's bin, which throws it around.
    assert_eq!(area_name(world, at), "Cargo Bay");
}

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
    "/obj/effect/mapping_helpers": { "parent": "/obj/effect", "declared_vars": {
        "icon": { "resource": "l.dmi" }, "icon_state": "helper" } },
    "/obj/effect/mapping_helpers/mail_sorting": { "parent": "/obj/effect/mapping_helpers",
        "declared_vars": { "sort_type": 0 } },
    "/obj/effect/mapping_helpers/mail_sorting/medbay": {
        "parent": "/obj/effect/mapping_helpers/mail_sorting", "declared_vars": { "sort_type": 9 } },
    "/obj/item": { "parent": "/obj", "declared_vars": {
        "icon": { "resource": "i.dmi" }, "icon_state": "toy", "interaction_flags_item": 1,
        "slot_flags": 0, "inhand_icon_state": null, "w_class": 3,
        "lefthand_file": { "resource": "l.dmi" }, "righthand_file": { "resource": "r.dmi" },
        "worn_icon": null, "worn_icon_state": null } },
    "/obj/item/pen": { "parent": "/obj/item", "declared_vars": {
        "name": "pen", "icon_state": "pen", "w_class": 1 } },
    "/obj/item/stack": { "parent": "/obj/item", "declared_vars": { "amount": 1,
        "max_amount": 50, "merge_type": null, "singular_name": null, "turf_type": null } },
    "/obj/item/stack/package_wrap": { "parent": "/obj/item/stack", "declared_vars": {
        "name": "package wrapper", "icon_state": "deliveryPaper", "amount": 25,
        "max_amount": 25, "merge_type": { "path": "/obj/item/stack/package_wrap" } } },
    "/obj/item/dest_tagger": { "parent": "/obj/item", "declared_vars": {
        "name": "destination tagger", "icon_state": "cargo tagger", "w_class": 1 } },
    "/obj/item/delivery": { "parent": "/obj/item", "declared_vars": {
        "icon": { "resource": "w.dmi" } } },
    "/obj/item/delivery/small": { "parent": "/obj/item/delivery", "declared_vars": {
        "name": "parcel", "icon_state": "deliverypackage3" } },
    "/obj/item/card": { "parent": "/obj/item" },
    "/obj/item/card/id": { "parent": "/obj/item/card", "declared_vars": {
        "access": { "list": [] }, "icon_state": "card_grey", "slot_flags": 256 } },
    "/obj/item/card/id/advanced": { "parent": "/obj/item/card/id" },
    "/obj/item/clothing": { "parent": "/obj/item" },
    "/obj/item/clothing/under": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 2 } },
    "/obj/item/clothing/under/color": { "parent": "/obj/item/clothing/under" },
    "/obj/item/clothing/under/color/grey": { "parent": "/obj/item/clothing/under/color" },
    "/obj/item/clothing/shoes": { "parent": "/obj/item/clothing", "declared_vars": { "slot_flags": 128 } },
    "/obj/item/clothing/shoes/sneakers": { "parent": "/obj/item/clothing/shoes" },
    "/obj/item/clothing/shoes/sneakers/black": { "parent": "/obj/item/clothing/shoes/sneakers" },
    "/obj/structure": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/structure/disposalpipe": { "parent": "/obj/structure", "declared_vars": {
        "name": "disposal pipe", "icon": { "resource": "p.dmi" }, "icon_state": "pipe",
        "dir": 0, "plane": -6 } },
    "/obj/structure/disposalpipe/segment": { "parent": "/obj/structure/disposalpipe" },
    "/obj/structure/disposalpipe/trunk": { "parent": "/obj/structure/disposalpipe",
        "declared_vars": { "icon_state": "pipe-t" } },
    "/obj/structure/disposalpipe/sorting": { "parent": "/obj/structure/disposalpipe",
        "declared_vars": { "icon_state": "pipe-j1s" } },
    "/obj/structure/disposalpipe/sorting/mail": { "parent": "/obj/structure/disposalpipe/sorting",
        "declared_vars": { "sortType": 0 } },
    "/obj/structure/disposaloutlet": { "parent": "/obj/structure", "declared_vars": {
        "name": "disposal outlet", "icon": { "resource": "d.dmi" }, "icon_state": "outlet",
        "density": 1 } },
    "/obj/machinery": { "parent": "/obj", "declared_vars": { "anchored": 1 } },
    "/obj/machinery/disposal": { "parent": "/obj/machinery", "declared_vars": {
        "icon": { "resource": "d.dmi" }, "density": 1 } },
    "/obj/machinery/disposal/bin": { "parent": "/obj/machinery/disposal", "declared_vars": {
        "name": "disposal unit", "icon_state": "disposal" } },
    "/mob": { "parent": "/atom/movable", "declared_vars": { "layer": 4, "density": 1 } },
    "/mob/living": { "parent": "/mob" },
    "/mob/living/carbon": { "parent": "/mob/living" },
    "/mob/living/carbon/human": { "parent": "/mob/living/carbon", "declared_vars": {
        "icon": { "resource": "h.dmi" }, "icon_state": "human_basic" } },
    "/turf": { "parent": "/atom", "declared_vars": { "icon": { "resource": "t.dmi" },
        "blocks_air": 0, "can_atmos_pass": 0, "initial_gas_mix": "o2=22;n2=82;TEMP=293.15" } },
    "/turf/open": { "parent": "/turf", "declared_vars": { "can_atmos_pass": -1,
        "planetary_atmos": 0 } },
    "/turf/open/floor": { "parent": "/turf/open", "declared_vars": {
        "name": "floor", "icon_state": "floor" } },
    "/turf/open/space": { "parent": "/turf/open", "declared_vars": { "icon_state": "space",
        "initial_gas_mix": "TEMP=2.7" } },
    "/turf/open/space/basic": { "parent": "/turf/open/space" },
    "/turf/closed": { "parent": "/turf", "declared_vars": { "density": 1, "opacity": 1,
        "blocks_air": 1 } },
    "/turf/closed/wall": { "parent": "/turf/closed", "declared_vars": { "icon_state": "wall" } },
    "/area": { "parent": "/atom" },
    "/area/space": { "parent": "/area" }
  },
  "element_calls": { "/datum/element/undertile": [
    { "type": "/obj/structure/disposalpipe", "proc": "Initialize", "call": "AddElement",
      "line": "AddElement(/datum/element/undertile, TRAIT_T_RAY_VISIBLE)" } ] }
}"##;

/// Rows top-down, the last line is y = 1. Spawns `A` (2,5) and (2,4); the bin `B` (3,5) on a
/// trunk east into a pipe run past the mail junction `J` (5,5, tagged medbay by its helper):
/// straight on to outlet `O` (10,5), sorted down to outlet `Q` (5,3); both outlets throw east.
/// A pen (3,4), the package wrapper `W` (3,6), the destination tagger `T` (2,6).
const MAP: &str = r##""w" = (/turf/closed/wall,/area/space)
"f" = (/turf/open/floor,/area/space)
"A" = (/obj/effect/landmark/start/assistant,/turf/open/floor,/area/space)
"B" = (/obj/structure/disposalpipe/trunk{dir = 4},/obj/machinery/disposal/bin,/turf/open/floor,/area/space)
"h" = (/obj/structure/disposalpipe/segment{dir = 4},/turf/open/floor,/area/space)
"J" = (/obj/structure/disposalpipe/sorting/mail{dir = 4},/obj/effect/mapping_helpers/mail_sorting/medbay,/turf/open/floor,/area/space)
"v" = (/obj/structure/disposalpipe/segment,/turf/open/floor,/area/space)
"Q" = (/obj/structure/disposalpipe/trunk{dir = 1},/obj/structure/disposaloutlet{dir = 4},/turf/open/floor,/area/space)
"O" = (/obj/structure/disposalpipe/trunk{dir = 8},/obj/structure/disposaloutlet{dir = 4},/turf/open/floor,/area/space)
"p" = (/obj/item/pen,/turf/open/floor,/area/space)
"W" = (/obj/item/stack/package_wrap,/turf/open/floor,/area/space)
"T" = (/obj/item/dest_tagger,/turf/open/floor,/area/space)

(1,1,1) = {"
wwwwwwwwwwwwwwww
wffffffffffffffw
wffffffffffffffw
wTWffffffffffffw
wABhJhhhhOfffffw
wApfvffffffffffw
wfffQffffffffffw
wffffffffffffffw
wwwwwwwwwwwwwwww
"}
"##;

const BIN: TilePos = TilePos::new(3, 5, 1);
const PEN: TilePos = TilePos::new(3, 4, 1);
const WRAP: TilePos = TilePos::new(3, 6, 1);
const TAGGER: TilePos = TilePos::new(2, 6, 1);
const STRAIGHT_OUTLET: TilePos = TilePos::new(10, 5, 1);
const SORTED_OUTLET: TilePos = TilePos::new(5, 3, 1);

const STATES: [(&str, &str); 19] = [
    ("t.dmi", "floor"),
    ("t.dmi", "wall"),
    ("l.dmi", "x2"),
    ("h.dmi", "human_basic"),
    ("i.dmi", "pen"),
    ("i.dmi", "deliveryPaper"),
    ("i.dmi", "cargo tagger"),
    ("i.dmi", "card_grey"),
    ("w.dmi", "deliverypackage1"),
    ("w.dmi", "deliverypackage3"),
    ("d.dmi", "disposal"),
    ("d.dmi", "disposal-flush"),
    ("d.dmi", "dispover-charge"),
    ("d.dmi", "dispover-ready"),
    ("d.dmi", "dispover-full"),
    ("d.dmi", "dispover-handle"),
    ("d.dmi", "outlet"),
    ("d.dmi", "outlet-open"),
    ("p.dmi", "pipe"),
];

fn fixture_assets(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("disposals_fixture")
        .join(test);
    let mut used = vec![
        "/obj/effect/landmark/start/assistant",
        "/obj/effect/mapping_helpers/mail_sorting/medbay",
        "/obj/structure/disposalpipe/segment",
        "/obj/structure/disposalpipe/trunk",
        "/obj/structure/disposalpipe/sorting/mail",
        "/obj/structure/disposaloutlet",
        "/obj/machinery/disposal/bin",
        "/obj/item/pen",
        "/obj/item/stack/package_wrap",
        "/obj/item/dest_tagger",
        "/turf/open/floor",
        "/turf/closed/wall",
        "/turf/open/space/basic",
        "/area/space",
    ];
    used.extend(ss13_mapload::DISPOSAL_RUNTIME_TYPES);
    write_fixture(&dir, TYPES, &used, &STATES, &[("map.dmm", MAP)])
}

/// Every open window of a client (interface, last data) and its chat.
#[derive(Resource, Default)]
struct Received {
    windows: HashMap<u32, (String, Vec<u8>)>,
    chat: Vec<String>,
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
        match message {
            UiServerMessage::Open {
                window,
                interface,
                data,
                ..
            } => {
                received
                    .windows
                    .insert(*window, (interface.clone(), data.clone()));
            }
            UiServerMessage::Update {
                window,
                data: Some(data),
                ..
            } => {
                if let Some(w) = received.windows.get_mut(window) {
                    w.1 = data.clone();
                }
            }
            UiServerMessage::Close { window } => {
                received.windows.remove(window);
            }
            _ => {}
        }
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
            .map(|p| p.app.world().resource::<Received>().chat.clone())
            .collect();
        format!("; chat {chat:?}")
    }
}

impl Station {
    fn start(test: &str, clients: usize) -> Self {
        let assets = fixture_assets(test);
        let mut s = testing::Station::start(fixture_map(&assets, &["map.dmm"]), |app| {
            app.add_plugins(AtmosPlugin);
        });
        s.join_all(clients, |app| {
            app.init_resource::<Received>().add_systems(Update, record);
        });
        Self(s)
    }

    fn get<T: Component>(&self, e: Entity) -> Option<&T> {
        self.server.world().get::<T>(e)
    }

    /// `entity`'s id for `p`, once replicated to it.
    fn wait_net_id(&mut self, p: usize, entity: Entity) -> ss13_shared::protocol::NetId {
        self.run_until("replicated to the player", |s| {
            s.net_id(p, entity).is_some()
        });
        self.net_id(p, entity).unwrap()
    }

    /// A click once `entity` is replicated to `p`.
    fn click(&mut self, p: usize, entity: Entity) {
        self.wait_net_id(p, entity);
        self.0.click(p, entity);
    }

    fn on(&self, pos: TilePos, filter: impl Fn(EntityRef) -> bool) -> Option<Entity> {
        let world = self.server.world();
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| filter(world.entity(e)))
    }

    fn item(&self, pos: TilePos) -> Entity {
        self.on(pos, |e| e.contains::<DmItem>()).unwrap()
    }

    fn bin(&self) -> Entity {
        self.on(BIN, |e| e.contains::<DisposalBin>()).unwrap()
    }

    fn inventory(&self, e: Entity) -> Inventory {
        self.get::<Inventory>(e).unwrap().clone()
    }

    fn hand_slot(&self, p: usize, hand: usize) -> InventorySlot {
        let _ = p;
        if hand == 0 {
            InventorySlot::LeftHand
        } else {
            InventorySlot::RightHand
        }
    }

    /// The newest window of `interface` on `p`'s client: its id and data.
    fn window<T: UiInterface>(&self, p: usize) -> Option<(u32, T::Data)> {
        let received = self.players[p].app.world().resource::<Received>();
        received
            .windows
            .iter()
            .filter(|(_, (name, _))| name == T::NAME)
            .max_by_key(|(id, _)| **id)
            .map(|(id, (_, data))| (*id, ss13_tgui::decode(data).unwrap()))
    }

    fn said(&self, p: usize, text: &str) -> bool {
        self.players[p]
            .app
            .world()
            .resource::<Received>()
            .chat
            .iter()
            .any(|line| line.contains(text))
    }

    /// A lone player may spawn on either landmark: it steps onto (2,5), west of the bin.
    fn step_west_of_bin(&mut self, p: usize) {
        let mob = self.players[p].mob;
        if self.get::<TilePos>(mob) == Some(&TilePos::new(2, 4, 1)) {
            self.send(
                p,
                ClientIntent::Move {
                    dir: ss13_shared::Dir::North,
                    seq: 1,
                },
            );
        }
        self.run_until("west of the bin", |s| {
            s.get::<TilePos>(mob) == Some(&TilePos::new(2, 5, 1))
        });
    }

    /// Opens the bin's window with an empty hand, waits for "Ready", engages the handle.
    fn flush_bin(&mut self, p: usize) {
        let bin = self.bin();
        self.click(p, bin);
        self.run_until("the bin is ready", |s| {
            s.window::<DisposalUnitUi>(p)
                .is_some_and(|(_, d): (u32, DisposalUnitData)| d.full_pressure && d.per >= 1.0)
        });
        let (window, _) = self.window::<DisposalUnitUi>(p).unwrap();
        self.act(p, window, "handle-1", &[]);
        self.run_until("the handle is engaged", |s| {
            s.get::<DisposalBin>(bin).unwrap().flush
                || s.get::<DisposalBin>(bin).unwrap().flushing_until.is_some()
        });
    }
}

/// Where an item thrown out of an outlet facing east lands: `eject_range` 2 tiles on.
fn thrown_from(outlet: TilePos) -> TilePos {
    TilePos::new(outlet.x + 2, outlet.y, outlet.z)
}

#[test]
fn flushed_item_reaches_the_outlet() {
    let mut s = Station::start("flushed_item_reaches_the_outlet", 1);
    let a = s.players[0].mob;
    s.step_west_of_bin(0);
    let (pen, bin) = (s.item(PEN), s.bin());
    s.click(0, pen);
    s.run_until("A holds the pen", |s| {
        s.inventory(a).active_item() == Some(pen)
    });
    // With the pen in hand, a click puts it into the bin.
    s.click(0, bin);
    s.run_until("the pen is in the bin", |s| {
        s.get::<StoredIn>(pen).is_some_and(|i| i.0 == bin)
    });
    s.run_until("A reads it", |s| s.said(0, "Вы кладёте pen в"));
    s.flush_bin(0);
    let flushed = s.now();
    s.run_until("the pen lands past the straight outlet", |s| {
        s.get::<TilePos>(pen) == Some(&thrown_from(STRAIGHT_OUTLET))
    });
    // 1.5 s flush, 7 pipes at 20 per second, 2 s at the outlet.
    assert!(s.now() - flushed >= 3.5, "{}", s.now() - flushed);
    // The bin recharges from the room.
    let (_, data) = s.window::<DisposalUnitUi>(0).unwrap();
    assert!(data.pressure_charging && !data.full_pressure && data.per < 0.5);
    assert!(!data.flush);
}

#[test]
fn tagged_parcel_takes_its_sort_branch() {
    let mut s = Station::start("tagged_parcel_takes_its_sort_branch", 1);
    let a = s.players[0].mob;
    s.step_west_of_bin(0);
    let (pen, wrap, tagger) = (s.item(PEN), s.item(WRAP), s.item(TAGGER));
    s.click(0, wrap);
    s.run_until("A holds the wrapper", |s| {
        s.inventory(a).active_item() == Some(wrap)
    });
    // The wrapper on the pen: a parcel with the pen inside, in A's other hand.
    s.click(0, pen);
    s.run_until("the pen is wrapped", |s| s.get::<StoredIn>(pen).is_some());
    let parcel = s.get::<StoredIn>(pen).unwrap().0;
    let other = 1 - s.inventory(a).active;
    assert_eq!(s.inventory(a).hands[other], Some(parcel));
    assert!(s.get::<Delivery>(parcel).is_some());
    assert_eq!(
        s.get::<DmName>(parcel).unwrap().name,
        "крохотного размера parcel"
    );
    // Drop the wrapper, take the tagger, set it to Medbay in its window.
    s.send(0, ClientIntent::DropActive);
    s.run_until("the wrapper is dropped", |s| {
        s.inventory(a).active_item().is_none()
    });
    s.click(0, tagger);
    s.run_until("A holds the tagger", |s| {
        s.inventory(a).active_item() == Some(tagger)
    });
    let active = s.inventory(a).active;
    let slot = s.hand_slot(0, active);
    s.send(0, ClientIntent::ClickSlot(slot));
    s.run_until("the tagger window opens", |s| {
        s.window::<DestinationTaggerUi>(0).is_some()
    });
    let (window, _) = s.window::<DestinationTaggerUi>(0).unwrap();
    s.act(0, window, "change", &[("index", "9")]);
    s.run_until("the tagger is set to Medbay", |s| {
        s.get::<DestTagger>(tagger).unwrap().curr_tag == 9
    });
    // Out-of-range tags are refused.
    s.act(0, window, "change", &[("index", "31")]);
    // The tagger on the parcel in the other hand.
    let slot = s.hand_slot(0, other);
    s.send(0, ClientIntent::ClickSlot(slot));
    s.run_until("the parcel is tagged", |s| {
        s.get::<Delivery>(parcel).unwrap().sort_tag == 9
    });
    assert_eq!(s.get::<DestTagger>(tagger).unwrap().curr_tag, 9);
    s.run_until("A reads the tag", |s| s.said(0, "*MEDBAY*"));
    // Into the bin and flushed: the junction sends it down to the sorted outlet.
    s.send(0, ClientIntent::DropActive);
    s.send(0, ClientIntent::SwapHands);
    s.run_until("A holds the parcel", |s| {
        s.inventory(a).active_item() == Some(parcel)
    });
    let bin = s.bin();
    s.click(0, bin);
    s.run_until("the parcel is in the bin", |s| {
        s.get::<StoredIn>(parcel).is_some_and(|i| i.0 == bin)
    });
    s.flush_bin(0);
    s.run_until("the parcel lands past the sorted outlet", |s| {
        s.get::<TilePos>(parcel) == Some(&thrown_from(SORTED_OUTLET))
    });
}

#[test]
fn dragged_mob_rides_to_the_outlet() {
    let mut s = Station::start("dragged_mob_rides_to_the_outlet", 2);
    // A stands west of the bin, B below A.
    let (mut a, mut b) = (s.players[0].mob, s.players[1].mob);
    let mut pa = 0;
    if s.get::<TilePos>(a) != Some(&TilePos::new(2, 5, 1)) {
        std::mem::swap(&mut a, &mut b);
        pa = 1;
    }
    let pb = 1 - pa;
    assert_eq!(s.get::<TilePos>(b), Some(&TilePos::new(2, 4, 1)));
    let bin = s.bin();
    let (from, to) = (s.wait_net_id(pa, b), s.wait_net_id(pa, bin));
    s.send(
        pa,
        ClientIntent::Drag {
            from: DragEnd::Atom(from),
            to: DragEnd::Atom(to),
            modifiers: ClickModifiers::default(),
        },
    );
    s.run_until("stuffing starts", |s| s.get::<DoAfter>(a).is_some());
    let started = s.now();
    s.run_until("B reads it", |s| s.said(pb, "начинает запихивать вас"));
    s.run_until("B is in the bin", |s| {
        s.get::<InCloset>(b).is_some_and(|c| c.closet == bin)
    });
    assert!(s.now() - started >= 1.9, "tg stuffs after 2 s");
    assert_eq!(s.get::<TilePos>(b), Some(&BIN));
    // B's own window of the bin closes while it is inside (`notcontained_state`).
    s.flush_bin(pa);
    // B rides the pipes (its view follows) and is thrown out of the straight outlet.
    s.run_until("B is in a pipe", |s| {
        s.get::<InCloset>(b)
            .is_some_and(|c| s.get::<DisposalHolder>(c.closet).is_some())
    });
    s.run_until("B passes the junction", |s| {
        s.get::<TilePos>(b).is_some_and(|p| p.x > 5)
    });
    s.run_until("B is out", |s| s.get::<InCloset>(b).is_none());
    s.run_until("B lands past the outlet", |s| {
        s.get::<TilePos>(b) == Some(&thrown_from(STRAIGHT_OUTLET))
    });
}

/// A holder out of its 1000 steps with a mob inside expels it where it is (tg would stop it
/// until the mob struggles out); nothing stays stuck in the pipes.
#[test]
fn holder_out_of_steps_lets_the_mob_out() {
    let mut s = Station::start("holder_out_of_steps_lets_the_mob_out", 2);
    // The player west of the bin stuffs the other one in.
    let first = s.players[0].mob;
    let pa = usize::from(s.get::<TilePos>(first) != Some(&TilePos::new(2, 5, 1)));
    let b = s.players[1 - pa].mob;
    let bin = s.bin();
    let (from, to) = (s.wait_net_id(pa, b), s.wait_net_id(pa, bin));
    s.send(
        pa,
        ClientIntent::Drag {
            from: DragEnd::Atom(from),
            to: DragEnd::Atom(to),
            modifiers: ClickModifiers::default(),
        },
    );
    s.run_until("B is in the bin", |s| {
        s.get::<InCloset>(b).is_some_and(|c| c.closet == bin)
    });
    s.flush_bin(pa);
    s.run_until("B is in a pipe", |s| {
        s.get::<InCloset>(b)
            .is_some_and(|c| s.get::<DisposalHolder>(c.closet).is_some())
    });
    let holder = s.get::<InCloset>(b).unwrap().closet;
    s.server
        .world_mut()
        .get_mut::<DisposalHolder>(holder)
        .unwrap()
        .count = 0;
    s.run_until("B is out", |s| s.get::<InCloset>(b).is_none());
    assert!(
        s.server.world().get_entity(holder).is_err(),
        "the holder is gone"
    );
    let x = s.get::<TilePos>(b).unwrap().x;
    assert!(x < 10, "out before the outlet, at {x}");
}
