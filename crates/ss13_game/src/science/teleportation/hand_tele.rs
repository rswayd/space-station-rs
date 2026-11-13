//! The hand teleporter, its portals and the bluespace locator (server): tg `/obj/item/hand_tele`,
//! `/obj/effect/portal` (`create_portal_pair`) and `/obj/item/locator` (tgui
//! `BluespaceLocator`).
//!
//! - Used in hand (where teleporting is allowed) the hand tele lists the teleporter consoles
//!   with a target, a working station and hub ("<area> (Active)" / "(Inactive)") and
//!   "None (Dangerous)" (a random tile within 10, away from the map edge). The choice (the tele
//!   still in the active hand) opens a portal pair for 30 s: one next to the user (in the
//!   direction faced, else any free one), the other within a tile of the target. At most three
//!   pairs at once ("is recharging!"). Right click on anything reopens a portal to the last
//!   choice; the tele clicked on one of its portals dispels the pair. A console that changes
//!   its target is forgotten as the last choice.
//! - Whatever bumps into a portal, or an empty hand or an item next to one, goes onto the other
//!   portal's tile (bluespace channel, no sparks); anchored things and effects stay. A portal
//!   whose twin is gone closes.
//! - The locator's window lists the living (or dead for under 10 minutes) mobs with a tracking
//!   implant on its level within 35 tiles: name, direction and distance.
// shortcut: no ranged clicks on portals (only adjacent), no portal light or wobble, no
// suicide act, no projectiles through portals; the locator draws a plain list (no compass, no
// search). CentCom and away levels are not modelled (`away_restricted`).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmName, IconIndex};
use ss13_net::server::Networked;
use ss13_shared::appearance::{Layer, MouseOpacity};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, Facing, IconStateId, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
use ss13_tgui::server::{AlwaysState, ServerUi, UiWindow};
use ss13_tgui::{UiInterface as _, UiStatus};
use ss13_world::{Density, SpatialIndex};

use super::server::{area_name_at, operational};
use super::teleporter::{self, CONSOLE};
use super::{BluespaceLocatorUi, LocatorData, Trackable};
use crate::click_handlers::ClickCtx;
use crate::construction::panel_open;
use crate::examine::name_of;
use crate::health::{Health, Stat};
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::teleport::{Channel, Tele, check_teleport_valid, do_teleport, valid_teleport_turf};
use crate::tools::{now, tell};

pub const HAND_TELE: &str = "/obj/item/hand_tele";
pub const LOCATOR: &str = "/obj/item/locator";
/// `max_portal_pairs`.
const MAX_PAIRS: usize = 3;
/// `create_portal_pair(..., 300, 1)`: seconds, accuracy.
const PORTAL_LIFESPAN: f64 = 30.0;
const PORTAL_ACCURACY: i32 = 1;
/// `urange(10, ...)` and `PORTAL_DANGEROUS_EDGE_LIMIT`.
const DANGEROUS_RANGE: i32 = 10;
const EDGE_LIMIT: i32 = 8;
const DANGEROUS: &str = "None (Dangerous)";
/// `tracking_range`, `lifespan_postmortem` (seconds).
const TRACKING_RANGE: i32 = 35;
const LIFESPAN_POSTMORTEM: f64 = 600.0;
const ICON: &str = "icons/obj/anomaly.dmi";
const ICON_STATE: &str = "portal";
/// `HIGH_OBJ_LAYER` on `GAME_PLANE`.
const HIGH_OBJ_LAYER: f32 = 3.5;
const GAME_PLANE: i16 = -4;
const SFX_CREATED: &str = "portal_created";
const SFX_ENTER: &str = "portal_enter";
const SFX_CLOSE: &str = "portal_close";
const SHORT_RANGE_EXTRARANGE: f32 = -7.0;
const MECHA: &str = "/obj/vehicle/sealed/mecha";

/// A hand tele's live state.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct HandTele {
    /// `active_portal_pairs`.
    pub pairs: Vec<(Entity, Entity)>,
    /// `last_portal_location`.
    pub last: Option<Dest>,
}

/// A place the hand tele opens portals to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dest {
    Dangerous,
    /// A teleporter console and the target it had when chosen.
    Console(Entity, Entity),
}

/// `/obj/effect/portal`: its twin and when it closes.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Portal {
    pub linked: Option<Entity>,
    pub expires_at: f64,
}

/// What the locator's window shows.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct LocatorView(pub LocatorData);

/// An open destination list of a hand tele.
#[derive(Component, Clone, Debug)]
struct DestPrompt {
    tele: Entity,
    options: Vec<Dest>,
}

fn item_is(world: &World, e: Entity, root: &str) -> bool {
    world
        .get::<DmItem>(e)
        .is_some_and(|i| is_under(&i.path, root))
}

fn tele_state(world: &World, tele: Entity) -> HandTele {
    world.get::<HandTele>(tele).cloned().unwrap_or_default()
}

/// The pairs whose portals still exist (`on_portal_destroy`).
fn live_pairs(world: &World, tele: Entity) -> Vec<(Entity, Entity)> {
    tele_state(world, tele)
        .pairs
        .into_iter()
        .filter(|&(a, b)| world.get::<Portal>(a).is_some() || world.get::<Portal>(b).is_some())
        .collect()
}

/// `attack_self` of a hand tele or a locator. True when `item` is one.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if item_is(world, item, LOCATOR) {
        let Some(v) = locator_view(world, item) else {
            return true;
        };
        world.entity_mut(item).insert(AlwaysState);
        super::server::open::<BluespaceLocatorUi, _>(world, client, mob, item, v);
        return true;
    }
    if !item_is(world, item, HAND_TELE) {
        return false;
    }
    if !can_teleport_notifies(world, client, mob, item) {
        return true;
    }
    let Some(&user_at) = world.get::<TilePos>(mob) else {
        return true;
    };
    let consoles: Vec<Entity> = world
        .query::<(Entity, &ss13_mapload::DmObj)>()
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, CONSOLE))
        .map(|(e, _)| e)
        .collect();
    let mut locations: Vec<(String, Dest)> = Vec::new();
    for console in consoles {
        let Some(target) = teleporter::resolve_target(world, console) else {
            continue;
        };
        let Some(&console_at) = world.get::<TilePos>(console) else {
            continue;
        };
        if !check_teleport_valid(world, user_at, console_at) {
            continue;
        }
        let Some(station) = teleporter::station_of(world, console) else {
            continue;
        };
        let Some(hub) = teleporter::hub_of(world, station) else {
            continue;
        };
        if !operational(world, station)
            || panel_open(world, station)
            || !operational(world, hub)
            || panel_open(world, hub)
        {
            continue;
        }
        let area = crate::explosions::turf_of(world, target)
            .map(|p| area_name_at(world, p))
            .unwrap_or_default();
        let state = if teleporter::station_state(world, station).engaged {
            "Active"
        } else {
            "Inactive"
        };
        locations.push((format!("{area} ({state})"), Dest::Console(console, target)));
    }
    locations.push((DANGEROUS.to_owned(), Dest::Dangerous));
    // `sort_list(locations)`.
    locations.sort_by(|a, b| a.0.cmp(&b.0));
    let (items, options): (Vec<String>, Vec<Dest>) = locations.into_iter().unzip();
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Hand Teleporter",
            "Teleporter to lock on",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(DestPrompt {
        tele: item,
        options,
    });
    true
}

/// `can_teleport_notifies(user)`: on a tile where teleporting is allowed.
fn can_teleport_notifies(world: &mut World, client: Entity, mob: Entity, tele: Entity) -> bool {
    let ok = match world.get::<TilePos>(mob).copied() {
        Some(at) => check_teleport_valid(world, at, at),
        None => false,
    };
    if !ok {
        let name = name_of(world, tele);
        tell(
            world,
            client,
            Span::Notice,
            format!("{name} is malfunctioning."),
        );
    }
    ok
}

/// The chosen destination: the tele still in the active hand, the user able.
pub(super) fn destination_answers(
    world: &mut World,
    mut cursor: Local<MessageCursor<PromptAnswered>>,
) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in answers {
        let Some(DestPrompt { tele, options }) = world.get::<DestPrompt>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(index)) = answer else {
            continue;
        };
        let Some(&dest) = options.get(index) else {
            continue;
        };
        let in_hand = world.get::<Inventory>(mob).and_then(Inventory::active_item) == Some(tele);
        if !in_hand || crate::status_effects::incapacitation(world, mob).incapacitated {
            continue;
        }
        if try_create_portal_to(world, client, mob, tele, dest) {
            let mut s = tele_state(world, tele);
            s.last = Some(dest);
            world.entity_mut(tele).insert(s);
        }
    }
}

/// The last choice, forgotten when its console has another target now
/// (`on_teleporter_new_target`).
fn last_location(world: &mut World, tele: Entity) -> Option<Dest> {
    let mut s = tele_state(world, tele);
    let last = s.last?;
    if let Dest::Console(console, target) = last
        && teleporter::console_state(world, console).target != Some(target)
    {
        s.last = None;
        world.entity_mut(tele).insert(s);
        return None;
    }
    Some(last)
}

/// `try_create_portal_to(user, location)`.
pub fn try_create_portal_to(
    world: &mut World,
    client: Entity,
    mob: Entity,
    tele: Entity,
    dest: Dest,
) -> bool {
    let name = name_of(world, tele);
    let pairs = live_pairs(world, tele);
    if pairs.len() >= MAX_PAIRS {
        tell(
            world,
            client,
            Span::Notice,
            format!("{name} is recharging!"),
        );
        return false;
    }
    let Some(here) = crate::explosions::turf_of(world, tele) else {
        return false;
    };
    let target = match dest {
        Dest::Dangerous => dangerous_turf(world, here),
        Dest::Console(console, _) => teleporter::resolve_target(world, console)
            .and_then(|t| crate::explosions::turf_of(world, t)),
    };
    let Some(target) = target else {
        tell(
            world,
            client,
            Span::Notice,
            format!("{name} vibrates, then stops. Maybe you should try something else."),
        );
        return false;
    };
    if !check_teleport_valid(world, here, target) {
        tell(
            world,
            client,
            Span::Notice,
            format!("{name} is malfunctioning."),
        );
        return false;
    }
    if !can_teleport_notifies(world, client, mob, tele) {
        return false;
    }
    let Some(&user_at) = world.get::<TilePos>(mob) else {
        return false;
    };
    let Some((p1, p2)) = create_portal_pair(world, user_at, target) else {
        return false;
    };
    let facing = world.get::<Facing>(mob).map_or(Dir::South, |f| f.0);
    try_move_adjacent(world, p1, facing);
    let mut s = tele_state(world, tele);
    s.pairs = pairs;
    s.pairs.push((p1, p2));
    world.entity_mut(tele).insert(s);
    tell(world, client, Span::Notice, "Locked in.");
    true
}

/// "None (Dangerous)": a random tile within 10 that is not near the map edge and allows
/// teleporting there.
fn dangerous_turf(world: &mut World, here: TilePos) -> Option<TilePos> {
    let (w, h) = world.resource::<SpatialIndex>().level_size(here.z)?;
    let (maxx, maxy) = (w as i32, h as i32);
    let mut tiles = Vec::new();
    for dy in -DANGEROUS_RANGE..=DANGEROUS_RANGE {
        for dx in -DANGEROUS_RANGE..=DANGEROUS_RANGE {
            if dx == 0 && dy == 0 {
                continue;
            }
            let p = TilePos::new(here.x + dx, here.y + dy, here.z);
            if p.x > maxx - EDGE_LIMIT
                || p.x < EDGE_LIMIT
                || p.y > maxy - EDGE_LIMIT
                || p.y < EDGE_LIMIT
            {
                continue;
            }
            if !world.resource::<SpatialIndex>().in_bounds(p)
                || !check_teleport_valid(world, here, p)
            {
                continue;
            }
            tiles.push(p);
        }
    }
    if tiles.is_empty() {
        return None;
    }
    let i = world
        .resource_mut::<crate::combat::CombatRng>()
        .range(0, tiles.len() as u32 - 1) as usize;
    tiles.get(i).copied()
}

/// `create_portal_pair(source, destination, 300, 1)`: two hardlinked portals.
pub fn create_portal_pair(
    world: &mut World,
    source: TilePos,
    destination: TilePos,
) -> Option<(Entity, Entity)> {
    let actual = valid_teleport_turf(world, source, destination, PORTAL_ACCURACY, false)?;
    let expires_at = now(world) + PORTAL_LIFESPAN;
    let p1 = spawn_portal(world, source, expires_at);
    let p2 = spawn_portal(world, actual, expires_at);
    world.entity_mut(p1).insert(Portal {
        linked: Some(p2),
        expires_at,
    });
    world.entity_mut(p2).insert(Portal {
        linked: Some(p1),
        expires_at,
    });
    for at in [source, actual] {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(at, SFX_CREATED, 50.0, true)
                .extrarange(SHORT_RANGE_EXTRARANGE),
        );
    }
    Some((p1, p2))
}

fn spawn_portal(world: &mut World, pos: TilePos, expires_at: f64) -> Entity {
    let icon = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(ICON, ICON_STATE))
        .unwrap_or(IconStateId(u32::MAX));
    world
        .spawn((
            Name::new("/obj/effect/portal"),
            DmName {
                name: "portal".to_owned(),
                desc: "Looks unstable. Best to test it with the clown.".to_owned(),
            },
            pos,
            Appearance {
                icon_state: icon,
                layer: Layer(HIGH_OBJ_LAYER),
                plane: GAME_PLANE,
                color: None,
                pixel_offset: [0, 0],
                rotation: 0,
                overlays: vec![],
                mouse_opacity: MouseOpacity::Icon,
                glide: 0,
            },
            Density,
            Networked,
            Portal {
                linked: None,
                expires_at,
            },
        ))
        .id()
}

/// `try_move_adjacent(portal, dir)`: the facing tile, else the first free tile around.
fn try_move_adjacent(world: &mut World, portal: Entity, facing: Dir) {
    let Some(&at) = world.get::<TilePos>(portal) else {
        return;
    };
    let dirs = std::iter::once(facing).chain(Dir::DMI_ORDER.into_iter().filter(|&d| d != facing));
    let free = {
        let index = world.resource::<SpatialIndex>();
        dirs.map(|d| at.step(d)).find(|&p| {
            index.in_bounds(p)
                && index.dense_on_tile(p).is_empty()
                && !index
                    .on_tile(p)
                    .iter()
                    .any(|&e| world.get::<Portal>(e).is_some())
        })
    };
    if let Some(p) = free {
        world.entity_mut(portal).insert(p);
    }
}

/// `expire()`: the closing sound; its twin goes with it (`hardlinked`).
pub fn expire(world: &mut World, portal: Entity) {
    let Some(p) = world.get::<Portal>(portal).copied() else {
        return;
    };
    if let Some(&at) = world.get::<TilePos>(portal) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(at, SFX_CLOSE, 50.0, false)
                .extrarange(SHORT_RANGE_EXTRARANGE),
        );
    }
    world.despawn(portal);
    if let Some(linked) = p.linked
        && world.get::<Portal>(linked).is_some()
    {
        world.despawn(linked);
    }
}

pub(super) fn expire_portals(world: &mut World) {
    let t = now(world);
    let due: Vec<Entity> = world
        .query::<(Entity, &Portal)>()
        .iter(world)
        .filter(|(_, p)| p.expires_at <= t)
        .map(|(e, _)| e)
        .collect();
    for portal in due {
        expire(world, portal);
    }
}

/// `/obj/effect/portal/teleport(moving)`: onto the twin's tile. True when it went.
pub fn portal_teleport(world: &mut World, portal: Entity, moving: Entity) -> bool {
    let Some(p) = world.get::<Portal>(portal).copied() else {
        return false;
    };
    let path = crate::construction::path_of(world, moving);
    let mob = world.get::<Health>(moving).is_some();
    let thing = path
        .as_deref()
        .is_some_and(|p| is_under(p, "/obj") && !is_under(p, "/obj/effect") && !is_under(p, MECHA));
    if !mob && !thing {
        return false;
    }
    // `get_link_target_turf`: a hardlinked portal without its twin closes.
    let Some(target) = p
        .linked
        .filter(|&l| world.get::<Portal>(l).is_some())
        .and_then(|l| world.get::<TilePos>(l).copied())
    else {
        world.despawn(portal);
        return false;
    };
    if crate::obj_damage::anchored(world, moving) {
        return false;
    }
    let Some(start) = crate::explosions::turf_of(world, moving) else {
        return false;
    };
    let tele = Tele {
        channel: Channel::Bluespace,
        no_effects: true,
        ..default()
    };
    if !do_teleport(world, moving, target, tele) {
        return false;
    }
    for at in [start, target] {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(at, SFX_ENTER, 50.0, true)
                .extrarange(SHORT_RANGE_EXTRARANGE),
        );
    }
    true
}

/// `Bumped` of a portal. True when `obstacle` is one.
pub fn bumped(world: &mut World, mover: Entity, obstacle: Entity) -> bool {
    if world.get::<Portal>(obstacle).is_none() {
        return false;
    }
    portal_teleport(world, obstacle, mover);
    true
}

fn adjacent_to(world: &World, mob: Entity, target: Entity) -> bool {
    match (world.get::<TilePos>(mob), world.get::<TilePos>(target)) {
        (Some(&a), Some(&b)) => a.chebyshev(b).is_some_and(|d| d <= 1),
        _ => false,
    }
}

/// `attack_hand` of a portal: next to it, through it.
pub fn portal_hand(world: &mut World, mob: Entity, target: Entity) -> bool {
    if world.get::<Portal>(target).is_none() {
        return false;
    }
    if adjacent_to(world, mob, target) {
        portal_teleport(world, target, mob);
    }
    true
}

/// The hand tele's `interact_with_atom` (dispel its portal) and right click (a portal to the
/// last choice); any other item on an adjacent portal goes through (`item_interaction`).
pub fn item_interaction(world: &mut World, c: &ClickCtx, held: Entity) -> bool {
    if item_is(world, held, HAND_TELE) {
        let is_ours = world.get::<Portal>(c.target).is_some()
            && live_pairs(world, held)
                .iter()
                .any(|&(a, b)| a == c.target || b == c.target);
        if is_ours && !c.modifiers.right {
            let (portal, tele) = (name_of(world, c.target), name_of(world, held));
            tell(
                world,
                c.client,
                Span::Notice,
                format!("You dispel {portal} with {tele}!"),
            );
            expire(world, c.target);
            return true;
        }
        if !c.modifiers.right {
            return false;
        }
        match last_location(world, held) {
            None => {
                let name = name_of(world, held);
                tell(
                    world,
                    c.client,
                    Span::Warning,
                    format!("{name} flashes briefly. No target is locked in."),
                );
            }
            Some(dest) => {
                try_create_portal_to(world, c.client, c.mob, held, dest);
            }
        }
        return true;
    }
    if world.get::<Portal>(c.target).is_none() || !adjacent_to(world, c.mob, c.target) {
        return false;
    }
    portal_teleport(world, c.target, c.mob);
    true
}

/// `/obj/item/locator/ui_data`.
pub(super) fn locator_view(world: &mut World, locator: Entity) -> Option<LocatorView> {
    if !item_is(world, locator, LOCATOR) {
        return None;
    }
    let mut data = LocatorData {
        trackingrange: TRACKING_RANGE,
        trackimplants: Vec::new(),
    };
    let Some(here) = crate::explosions::turf_of(world, locator) else {
        return Some(LocatorView(data));
    };
    let t = now(world);
    let tracked: Vec<(Entity, TilePos)> = world
        .query::<(Entity, &crate::implants::Implants, &TilePos)>()
        .iter(world)
        .filter(|(_, imps, _)| {
            imps.0
                .iter()
                .any(|i| i.kind == crate::implants::ImplantKind::Tracking)
        })
        .map(|(e, _, &p)| (e, p))
        .collect();
    for (mob, at) in tracked {
        if world.get::<Health>(mob).is_none() {
            continue;
        }
        if world.get::<Stat>(mob) == Some(&Stat::Dead)
            && world
                .get::<crate::death::TimeOfDeath>(mob)
                .is_some_and(|d| d.0 + LIFESPAN_POSTMORTEM < t)
        {
            continue;
        }
        let Some(distance) = here.chebyshev(at) else {
            continue;
        };
        if distance > TRACKING_RANGE as u32 {
            continue;
        }
        let direction = crate::mobs::ai::movement::dir_towards(here, at)
            .map(|d| crate::mining::orm::dir_text(d).to_owned())
            .unwrap_or_default();
        data.trackimplants.push(Trackable {
            name: name_of(world, mob),
            direction,
            distance,
        });
    }
    Some(LocatorView(data))
}

/// The locator's window: held, or within reach (`default_state` of an item).
pub(super) fn locator_status(
    mut windows: Query<&mut UiWindow>,
    units: Query<Option<&TilePos>, With<LocatorView>>,
    users: Query<(&Inventory, Option<&Stat>, Option<&TilePos>)>,
) {
    for mut window in &mut windows {
        if window.interface != BluespaceLocatorUi::NAME {
            continue;
        }
        let Ok(at) = units.get(window.source) else {
            window.cap_status(UiStatus::Close);
            continue;
        };
        let status =
            crate::devices::item_ui_status(window.source, at, users.get(window.actor).ok());
        window.cap_status(status);
    }
}

impl ServerUi for BluespaceLocatorUi {
    type Source = &'static LocatorView;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &LocatorView) {}

    fn data(view: &LocatorView, _: Entity, _: &()) -> LocatorData {
        view.0.clone()
    }
}
