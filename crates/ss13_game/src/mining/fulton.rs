//! Fulton extraction (server), after tg `code/modules/mining/fulton.dm`.
//!
//! - The beacon assembly kit (`/obj/item/fulton_core`) used in hand unfolds after 1.5 s into a
//!   recovery beacon on the user's tile (not in a `NOTELEPORT` area), named
//!   `fulton recovery beacon (NNN) (<area>)`; its maker is asked for a name. An empty hand
//!   folds a beacon back into its kit after 1.5 s.
//! - The pack used in hand links to a beacon of its networks (`station`; the syndicate pack
//!   also `syndicate`), chosen from a list.
//! - The pack on an unanchored thing on a tile, outdoors (unless the syndicate pack), not in a
//!   `NOTELEPORT` area or across a `LOCAL_TELEPORT` boundary: after 5 s attaching, a use is
//!   spent (the last one goes with the thing), a living thing is paralyzed (32 s, networked 18
//!   s), an object is anchored; 9.4 s later the thing comes down on a free tile around the
//!   beacon and 7.9 s after that it is released.
// shortcut: no balloon holder or animation (the thing waits on its tile, then lands), no
// `move_resist` limit; the networked pack and beacon only use the plain beacon list (their
// Candela network is ../marker beacons).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{Areas, DmArea, DmItem, DmName, DmObj, DmPower, Turf};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_world::{Density, SpatialIndex};

use super::MiningJob;
use crate::construction::Job;
use crate::examine::Line;
use crate::health::Health;
use crate::obj_damage::Anchored;
use crate::sound::{PlaySound, playsound};
use crate::status_effects::Incap;
use crate::tools::{Actor, DoAfter, do_after, tell};

pub const PACK: &str = "/obj/item/extraction_pack";
const NETWORKED_PACK: &str = "/obj/item/extraction_pack/networked";
const SYNDICATE_PACK: &str = "/obj/item/extraction_pack/syndicate";
pub const CORE: &str = "/obj/item/fulton_core";
pub const POINT: &str = "/obj/structure/extraction_point";
const ZIP: &str = "sound/items/zip/zip.ogg";
const DEPLOY: &str = "sound/items/fulton/fultext_deploy.ogg";
const LAUNCH: &str = "sound/items/fulton/fultext_launch.ogg";
const DECONSTRUCT: &str = "sound/items/deconstruct.ogg";
/// `NOTELEPORT`, `LOCAL_TELEPORT` of `area_flags`.
const NOTELEPORT: u32 = 1 << 2;
const LOCAL_TELEPORT: u32 = 1 << 15;
/// `MAX_LABEL_LEN`.
const MAX_LABEL_LEN: u32 = 64;
/// From the end of the attach to the landing (0.4 + 6 + 3 s) and to the release (+ 7 + 0.4 +
/// 0.5 s).
const FLIGHT: f64 = 9.4;
const SETTLE: f64 = 7.9;

/// A recovery beacon: its `beacon_network` and the kit it folds into.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ExtractionPoint {
    pub network: &'static str,
    pub core: &'static str,
}

/// A pack's `beacon_ref` and `uses_left`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FultonPack {
    pub beacon: Option<Entity>,
    pub uses_left: u32,
}

impl Default for FultonPack {
    fn default() -> Self {
        Self {
            beacon: None,
            uses_left: 3,
        }
    }
}

/// A thing on its way to `to`: it lands at `land_at` and is released at `release_at`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Extracting {
    pub to: TilePos,
    pub land_at: f64,
    pub release_at: f64,
    pub landed: bool,
    /// The pack to delete on release (its last use).
    pub spent_pack: Option<Entity>,
}

/// On a prompt: the pack it links, or the beacon it names.
#[derive(Component, Clone, Copy, Debug)]
enum FultonPrompt {
    Link(Entity),
    Name(Entity),
}

pub(super) struct FultonPlugin;

impl Plugin for FultonPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(ss13_world::MapInit, setup_mapped)
            .add_systems(Update, flights)
            .add_systems(
                Update,
                answers
                    .in_set(ss13_tgui::server::UiSystems::Handle)
                    .after(ss13_tgui::prompt::PromptSystems::Answer),
            );
    }
}

fn now(world: &World) -> f64 {
    world
        .get_resource::<Time>()
        .map_or(0.0, |t| t.elapsed_secs_f64())
}

fn path_of(world: &World, e: Entity) -> Option<String> {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.clone())
        .or_else(|| world.get::<DmObj>(e).map(|o| o.path.clone()))
}

/// The beacon type's network and kit.
pub fn point_kind(path: &str) -> Option<ExtractionPoint> {
    if !is_under(path, POINT) {
        return None;
    }
    Some(
        if is_under(path, "/obj/structure/extraction_point/syndicate") {
            ExtractionPoint {
                network: "syndicate",
                core: "/obj/item/fulton_core/syndicate",
            }
        } else if is_under(path, "/obj/structure/extraction_point/networked") {
            ExtractionPoint {
                network: "station",
                core: "/obj/item/fulton_core/networked",
            }
        } else {
            ExtractionPoint {
                network: "station",
                core: CORE,
            }
        },
    )
}

/// The beacon a kit type unfolds into (`point_type`).
pub fn core_point(path: &str) -> Option<&'static str> {
    if !is_under(path, CORE) {
        return None;
    }
    Some(if is_under(path, "/obj/item/fulton_core/syndicate") {
        "/obj/structure/extraction_point/syndicate"
    } else if is_under(path, "/obj/item/fulton_core/networked") {
        "/obj/structure/extraction_point/networked"
    } else {
        POINT
    })
}

/// A pack type's `beacon_networks`, `can_use_indoors` and `paralyze_duration`.
pub fn pack_kind(path: &str) -> Option<(&'static [&'static str], bool, f32)> {
    if !is_under(path, PACK) {
        return None;
    }
    Some(if is_under(path, SYNDICATE_PACK) {
        (&["station", "syndicate"], true, 32.0)
    } else if is_under(path, NETWORKED_PACK) {
        (&["station"], false, 18.0)
    } else {
        (&["station"], false, 32.0)
    })
}

fn setup_mapped(world: &mut World) {
    let mapped: Vec<(Entity, String)> = world
        .query_filtered::<(Entity, &DmObj), (With<ss13_world::Mapped>, Without<ExtractionPoint>)>()
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, POINT))
        .map(|(e, o)| (e, o.path.clone()))
        .collect();
    for (e, path) in mapped {
        if let Some(kind) = point_kind(&path) {
            world.entity_mut(e).insert(kind);
        }
    }
}

fn area_of(world: &World, pos: TilePos) -> Option<Entity> {
    world.resource::<Areas>().area_at(pos)
}

fn area_flags(world: &World, area: Option<Entity>) -> u32 {
    area.and_then(|a| world.get::<DmPower>(a))
        .map_or(0, |p| p.area_flags as u32)
}

fn beacons(world: &mut World, networks: &[&str]) -> Vec<(Entity, String)> {
    let mut out: Vec<(Entity, String)> = world
        .query::<(Entity, &ExtractionPoint)>()
        .iter(world)
        .filter(|(_, p)| networks.contains(&p.network))
        .map(|(e, _)| e)
        .collect::<Vec<_>>()
        .into_iter()
        .map(|e| (e, crate::examine::name_of(world, e)))
        .collect();
    // `sort_names`.
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out
}

/// `attack_self` of a pack (link it) or a kit (unfold it). True when handled.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(path) = world.get::<DmItem>(item).map(|i| i.path.clone()) else {
        return false;
    };
    if let Some((networks, ..)) = pack_kind(&path) {
        let list = beacons(world, networks);
        if list.is_empty() {
            tell(world, client, Span::Warning, "no beacons");
            return true;
        }
        let prompt = ask_world(
            world,
            Ask::new(
                client,
                mob,
                "Balloon Extraction Pack",
                "Beacon to connect to",
                PromptKind::List {
                    items: list.into_iter().map(|(_, n)| n).collect(),
                    default: None,
                },
            ),
        );
        world.entity_mut(prompt).insert(FultonPrompt::Link(item));
        return true;
    }
    if core_point(&path).is_none() {
        return false;
    }
    let pos = world.get::<TilePos>(mob).copied();
    let flags = area_flags(world, pos.and_then(|p| area_of(world, p)));
    if flags & NOTELEPORT != 0 {
        tell(world, client, Span::Warning, "unable to deploy!");
        return true;
    }
    do_after(
        world,
        Actor {
            client,
            mob,
            item: Some(item),
        },
        mob,
        15.0,
        0,
        Job::Mining(MiningJob::UnfoldFultonBeacon),
    );
    true
}

/// The pack on `target` (`interact_with_atom`), or nothing. True when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some((networks, indoors, _)) = path_of(world, held).and_then(|p| pack_kind(&p)) else {
        return false;
    };
    // `ismovable`, `isturf(loc)`, not anchored.
    let Some(&pos) = world.get::<TilePos>(target) else {
        return false;
    };
    if world.get::<Turf>(target).is_some()
        || world.get::<crate::inventory::CarriedBy>(target).is_some()
        || world.get::<Extracting>(target).is_some()
    {
        return false;
    }
    let anchored = world
        .get::<Anchored>(target)
        .map(|a| a.0)
        .or_else(|| world.get::<DmObj>(target).map(|o| o.anchored))
        .or_else(|| world.get::<DmItem>(target).map(|i| i.anchored))
        .unwrap_or(false);
    if anchored {
        return false;
    }
    let Some(beacon) = linked_beacon(world, held, networks) else {
        tell(world, client, Span::Warning, "not linked!");
        return true;
    };
    let area = area_of(world, pos);
    if !indoors
        && !area
            .and_then(|a| world.get::<DmArea>(a))
            .is_some_and(|a| a.outdoors)
    {
        tell(world, client, Span::Warning, "not outdoors!");
        return true;
    }
    let flags = area_flags(world, area);
    let beacon_area = world
        .get::<TilePos>(beacon)
        .copied()
        .and_then(|p| area_of(world, p));
    let target_flags = area_flags(world, beacon_area);
    if flags & NOTELEPORT != 0
        || (area != beacon_area && (flags | target_flags) & LOCAL_TELEPORT != 0)
    {
        tell(world, client, Span::Warning, "unable to activate!");
        return true;
    }
    tell(world, client, Span::Notice, "attaching...");
    playsound(world, PlaySound::new(pos, ZIP, 50.0, true));
    if let Some(victim) = world
        .get::<crate::server::PlayerMob>(target)
        .map(|p| p.client)
    {
        tell(
            world,
            victim,
            Span::Danger,
            "You are being extracted! Stand still to proceed.",
        );
    }
    do_after(
        world,
        Actor {
            client,
            mob,
            item: Some(held),
        },
        target,
        50.0,
        0,
        Job::Mining(MiningJob::Fulton),
    );
    true
}

/// `beacon_ref?.resolve()` of a pack, still a beacon of its networks.
fn linked_beacon(world: &mut World, pack: Entity, networks: &[&str]) -> Option<Entity> {
    let beacon = world.get::<FultonPack>(pack).and_then(|p| p.beacon)?;
    let ok = world
        .get::<ExtractionPoint>(beacon)
        .is_some_and(|p| networks.contains(&p.network));
    if !ok {
        if let Some(mut p) = world.get_mut::<FultonPack>(pack) {
            p.beacon = None;
        }
        return None;
    }
    Some(beacon)
}

/// The end of a fulton `job`: unfold a kit, fold a beacon, or send the thing off.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: MiningJob) {
    match job {
        MiningJob::UnfoldFultonBeacon => unfold(world, mob, action),
        MiningJob::FoldFultonBeacon => fold(world, action),
        MiningJob::Fulton => launch(world, action),
        _ => {}
    }
}

fn unfold(world: &mut World, mob: Entity, action: &DoAfter) {
    let Some(kit) = action.item.filter(|&k| world.get_entity(k).is_ok()) else {
        return;
    };
    let Some(point) = path_of(world, kit).and_then(|p| core_point(&p)) else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let Some(beacon) = crate::construction::spawn_obj(world, point, pos) else {
        return;
    };
    if let Some(kind) = point_kind(point) {
        world.entity_mut(beacon).insert(kind);
    }
    let number = world
        .resource_mut::<crate::combat::CombatRng>()
        .range(100, 999);
    let area = area_of(world, pos)
        .map(|a| crate::shuttle::area_name(world, a))
        .unwrap_or_default();
    let base = crate::examine::name_of(world, beacon);
    let name = format!("{base} ({number}) ({area})");
    set_name(world, beacon, &name);
    playsound(world, PlaySound::new(pos, DECONSTRUCT, 50.0, true));
    world.despawn(kit);
    // `poll_name`.
    let prompt = ask_world(
        world,
        Ask::new(
            action.client,
            mob,
            "Fulton Beacon",
            "What would you like to name the beacon?",
            PromptKind::Text {
                default: name,
                max_length: MAX_LABEL_LEN,
                multiline: false,
            },
        ),
    );
    world.entity_mut(prompt).insert(FultonPrompt::Name(beacon));
}

fn set_name(world: &mut World, e: Entity, name: &str) {
    if let Some(mut dm) = world.get_mut::<DmName>(e) {
        dm.name = name.to_owned();
    }
    world.entity_mut(e).insert(Name::new(name.to_owned()));
}

/// The beacon's `attack_hand`: start folding it. True when `target` is a beacon.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<ExtractionPoint>(target).is_none() {
        return false;
    }
    tell(world, client, Span::Notice, "undeploying...");
    do_after(
        world,
        Actor {
            client,
            mob,
            item: None,
        },
        target,
        15.0,
        0,
        Job::Mining(MiningJob::FoldFultonBeacon),
    );
    true
}

fn fold(world: &mut World, action: &DoAfter) {
    let beacon = action.target;
    let Some(point) = world.get::<ExtractionPoint>(beacon).cloned() else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(beacon) else {
        return;
    };
    crate::construction::spawn_obj(world, point.core, pos);
    playsound(world, PlaySound::new(pos, DECONSTRUCT, 50.0, true));
    world.despawn(beacon);
}

/// A free tile around `center` (`RANGE_TURFS(1)` not `is_blocked_turf`), else `center`.
fn landing(world: &mut World, center: TilePos) -> TilePos {
    let free: Vec<TilePos> = (-1i32..=1)
        .flat_map(|dy| (-1i32..=1).map(move |dx| (dx, dy)))
        .map(|(dx, dy)| TilePos {
            x: center.x + dx,
            y: center.y + dy,
            z: center.z,
        })
        .filter(|&p| {
            let index = world.resource::<SpatialIndex>();
            index.in_bounds(p) && index.dense_on_tile(p).is_empty()
        })
        .collect();
    if free.is_empty() {
        return center;
    }
    let i = world
        .resource_mut::<crate::combat::CombatRng>()
        .range(0, free.len() as u32 - 1) as usize;
    free[i]
}

fn launch(world: &mut World, action: &DoAfter) {
    let thing = action.target;
    let Some(pack) = action.item.filter(|&p| world.get_entity(p).is_ok()) else {
        return;
    };
    let Some((networks, _, paralyze)) = path_of(world, pack).and_then(|p| pack_kind(&p)) else {
        return;
    };
    let Some(beacon) = linked_beacon(world, pack, networks) else {
        tell(world, action.client, Span::Warning, "beacon lost!");
        return;
    };
    let (Some(&from), Some(&beacon_pos)) =
        (world.get::<TilePos>(thing), world.get::<TilePos>(beacon))
    else {
        return;
    };
    if world.get::<Extracting>(thing).is_some() {
        return;
    }
    tell(world, action.client, Span::Notice, "extracting!");
    let mut state = world.get::<FultonPack>(pack).copied().unwrap_or_default();
    state.uses_left = state.uses_left.saturating_sub(1);
    world.entity_mut(pack).insert(state);
    let spent_pack = (state.uses_left == 0).then_some(pack);
    if world.get::<Health>(thing).is_some() {
        crate::status_effects::apply(world, thing, Incap::Paralyze, paralyze);
    } else {
        world
            .entity_mut(thing)
            .insert(Anchored(true))
            .remove::<Density>();
    }
    if let Some(pack) = spent_pack
        && world.get::<crate::inventory::CarriedBy>(pack).is_some()
    {
        // `transferItemToLoc(src, thing)`: the empty pack goes along, out of the hands.
        crate::storage::take_out(world, pack, from);
    }
    playsound(world, PlaySound::new(from, DEPLOY, 50.0, true));
    let t = now(world);
    world.entity_mut(thing).insert(Extracting {
        to: beacon_pos,
        land_at: t + FLIGHT,
        release_at: t + FLIGHT + SETTLE,
        landed: false,
        spent_pack,
    });
}

fn flights(world: &mut World) {
    let t = now(world);
    let due: Vec<(Entity, Extracting)> = world
        .query::<(Entity, &Extracting)>()
        .iter(world)
        .filter(|(_, x)| (!x.landed && x.land_at <= t) || x.release_at <= t)
        .map(|(e, x)| (e, *x))
        .collect();
    for (thing, mut x) in due {
        if !x.landed {
            if let Some(&from) = world.get::<TilePos>(thing) {
                playsound(world, PlaySound::new(from, LAUNCH, 50.0, true));
            }
            let to = landing(world, x.to);
            world.entity_mut(thing).insert(to);
            if let Some(pack) = x.spent_pack
                && world.get::<TilePos>(pack).is_some()
            {
                world.entity_mut(pack).insert(to);
            }
            x.landed = true;
            world.entity_mut(thing).insert(x);
            continue;
        }
        world.entity_mut(thing).remove::<Extracting>();
        if world.get::<Health>(thing).is_none() {
            // `set_density(initial(density))`.
            let dense = world
                .get::<ss13_mapload::DmAtom>(thing)
                .is_some_and(|a| a.density);
            let mut e = world.entity_mut(thing);
            e.insert(Anchored(false));
            if dense {
                e.insert(Density);
            }
        }
        if let Some(pack) = x.spent_pack
            && world.get_entity(pack).is_ok()
        {
            world.despawn(pack);
        }
    }
}

/// The link and name answers, revalidated.
fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, actor, answer) in answers {
        let Some(&kind) = world.get::<FultonPrompt>(prompt) else {
            continue;
        };
        match (kind, answer) {
            (FultonPrompt::Link(pack), Some(PromptAnswer::Item(i))) => {
                let Some((networks, ..)) = path_of(world, pack).and_then(|p| pack_kind(&p)) else {
                    continue;
                };
                let Some((beacon, _)) = beacons(world, networks).into_iter().nth(i) else {
                    continue;
                };
                let mut state = world.get::<FultonPack>(pack).copied().unwrap_or_default();
                state.beacon = Some(beacon);
                world.entity_mut(pack).insert(state);
                if let Some(client) = world
                    .get::<crate::server::PlayerMob>(actor)
                    .map(|p| p.client)
                {
                    tell(world, client, Span::Notice, "linked!");
                }
            }
            (FultonPrompt::Name(beacon), Some(PromptAnswer::Text(name))) => {
                // `creator.Adjacent(src)`.
                let adjacent = world.get_entity(beacon).is_ok()
                    && crate::storage::in_reach(world, actor, beacon);
                if adjacent && !name.trim().is_empty() {
                    set_name(world, beacon, name.trim());
                }
            }
            _ => {}
        }
    }
}

/// The pack's uses and link.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<Line>) {
    if !path_of(world, target).is_some_and(|p| is_under(&p, PACK)) {
        return;
    }
    let state = world.get::<FultonPack>(target).copied().unwrap_or_default();
    let s = if state.uses_left == 1 { "" } else { "s" };
    lines.push((
        Span::Info,
        format!("It has {} use{s} remaining.", state.uses_left),
    ));
    let beacon = state
        .beacon
        .filter(|&b| world.get::<ExtractionPoint>(b).is_some());
    lines.push((
        Span::Info,
        match beacon {
            Some(b) => format!("It is linked to {}.", crate::examine::name_of(world, b)),
            None => "It is not linked to a beacon.".to_owned(),
        },
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_follow_the_types() {
        assert_eq!(point_kind(POINT).unwrap().core, CORE);
        assert_eq!(
            point_kind("/obj/structure/extraction_point/syndicate")
                .unwrap()
                .network,
            "syndicate"
        );
        assert_eq!(
            core_point("/obj/item/fulton_core/networked"),
            Some("/obj/structure/extraction_point/networked")
        );
        let (nets, indoors, _) = pack_kind(SYNDICATE_PACK).unwrap();
        assert!(indoors && nets.contains(&"syndicate"));
        let (nets, indoors, paralyze) = pack_kind(NETWORKED_PACK).unwrap();
        assert!(!indoors && nets == ["station"] && paralyze == 18.0);
        assert!(pack_kind(CORE).is_none());
    }

    #[test]
    fn a_pack_links_only_to_its_networks() {
        let mut world = World::new();
        let station = world
            .spawn((
                ExtractionPoint {
                    network: "station",
                    core: CORE,
                },
                Name::new("b"),
            ))
            .id();
        world.spawn((
            ExtractionPoint {
                network: "syndicate",
                core: CORE,
            },
            Name::new("a"),
        ));
        assert_eq!(beacons(&mut world, &["station"]).len(), 1);
        let both = beacons(&mut world, &["station", "syndicate"]);
        assert_eq!(both[0].1, "a");
        let pack = world
            .spawn(FultonPack {
                beacon: Some(station),
                uses_left: 3,
            })
            .id();
        assert_eq!(linked_beacon(&mut world, pack, &["station"]), Some(station));
        assert_eq!(linked_beacon(&mut world, pack, &["syndicate"]), None);
        assert_eq!(world.get::<FultonPack>(pack).unwrap().beacon, None);
    }
}
