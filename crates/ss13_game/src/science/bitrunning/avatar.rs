//! Netpods, avatars and hololadders (server), after tg `code/modules/bitrunning/netpod/*.dm`,
//! `components/avatar_connection.dm`, `components/netpod_healing.dm` and
//! `objects/hololadder.dm`.
//!
//! - A human dragged onto an open, working netpod is shut in (`close_machine`) and, with a
//!   player, connects (`enter_matrix`): the pod's quantum server (4 tiles) must hold a loaded
//!   domain and be ready; after 2 s the player gets the pod's last avatar if it still stands,
//!   else a new one at a free exit of the domain (its hololadder spawns there; each costs one
//!   of the domain's exits, `retries_spent`). The new avatar is a random human in the domain's
//!   forced outfit (when it is a job's) or the bitrunner's, stripped of ID, belt, ears, glasses,
//!   gloves, pockets, suit and (unforced) hands, with a backpack of a survival box, a medkit and
//!   a flashlight. The occupant heals 4 brute, burn and toxin a second while the pod is closed.
//! - Connected ([`AvatarConnection`]): brute, burn and toxin the avatar takes go to the same
//!   bodyparts of the body in the pod (`on_linked_damage`); a hit worth the body's health plus
//!   100, or one that knocks it out, disconnects. Any damage loses the no-hit bonus.
//! - Disconnects return the mind to the body (`full_avatar_disconnect`): safe ones by the
//!   hololadder (a click or a step onto it, 3 s), forced ones (with 40 brain damage, less with
//!   the server's servos: `disconnect_damage`) when the avatar dies, the body dies, is knocked
//!   out or leaves the pod, the pod or the server loses power or goes away, the server resets,
//!   or the pod is pried open (a crowbar, 15 s, the avatar warned). The pod stays shut a while
//!   (1 s, more when hurt) then opens; a dead or unpowered one opens at once.
// shortcut: no outfit selection window (`NetpodOutfits`) or body scanning (`copy_body`), no
// hacker alias, voice, skills, skillchips or disks loaded onto the avatar (`stock_gear`), no
// avatar ID card or help action (the help text goes to chat), no stasis status
// effect, no red pill, no screen alerts (chat lines instead), no netpod integrity alert; the
// avatar keeps the pod's occupant's name.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::DmTool;
use ss13_mapload::jobs::{Outfit, OutfitSlot};
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::{Density, SpatialIndex, WorldSystems};

use super::server::{self, BitrunJob, HOLOLADDER, MAX_DISTANCE, QuantumServer, operational};
use crate::closets::{ClosetRelayMove, InCloset};
use crate::construction::Job;
use crate::health::{DamageType, Health, Stat};
use crate::inventory::Inventory;
use crate::medical::brainmob;
use crate::medical::stasis::InStasis;
use crate::server::PlayerMob;
use crate::tools::{Actor, ToolKind, tell};
use crate::zone::BodyZone;

pub const NETPOD_ROOT: &str = "/obj/machinery/netpod";
/// `/datum/outfit/job/bitrunner`: the netpod's default `netsuit`.
pub const BITRUNNER_OUTFIT: &str = "/datum/outfit/job/bitrunner";
/// `BASE_DISCONNECT_DAMAGE`.
pub const BASE_DISCONNECT_DAMAGE: f32 = 40.0;
/// `netpod_healing` `BASE_HEAL`, per second.
pub const BASE_HEAL: f32 = 4.0;
/// `MAX_LIVING_HEALTH`.
const MAX_LIVING_HEALTH: f32 = 100.0;
/// Deciseconds: `enter_matrix`'s `do_after`, the hololadder's `travel_time`, prying a pod open.
const CONNECT_DS: f32 = 20.0;
const LADDER_DS: f32 = 30.0;
const PRY_DS: f32 = 150.0;
/// `generate_avatar`'s backpack.
const AVATAR_BACKPACK: [&str; 3] = [
    "/obj/item/storage/box/survival",
    "/obj/item/storage/medkit/regular",
    "/obj/item/flashlight",
];
/// `generate_avatar`'s nulled slots (and the ID: no avatar trim here).
const STRIPPED: [OutfitSlot; 9] = [
    OutfitSlot::Belt,
    OutfitSlot::Ears,
    OutfitSlot::Glasses,
    OutfitSlot::Gloves,
    OutfitSlot::LPocket,
    OutfitSlot::RPocket,
    OutfitSlot::Suit,
    OutfitSlot::SuitStore,
    OutfitSlot::Id,
];

/// `/obj/machinery/netpod`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Netpod {
    /// `state_open`.
    pub open: bool,
    pub connected: bool,
    /// `avatar_ref`: the avatar to reuse.
    pub avatar: Option<Entity>,
    /// `netpod_healing` on the occupant.
    pub healing: bool,
    /// When `auto_disconnect` opens it.
    pub auto_open_at: Option<f64>,
}

impl Default for Netpod {
    fn default() -> Self {
        Self {
            open: true,
            connected: false,
            avatar: None,
            healing: false,
            auto_open_at: None,
        }
    }
}

/// `/obj/structure/hololadder`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Hololadder;

/// The avatar's [`AvatarConnection`] damage as last seen: per bodypart brute and burn, toxin.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Seen {
    pub parts: [(f32, f32); 6],
    pub tox: f32,
}

impl Seen {
    pub fn of(health: &Health) -> Self {
        let mut parts = [(0.0, 0.0); 6];
        for zone in BodyZone::ALL {
            let p = health.part(zone);
            parts[zone.index()] = (p.brute, p.burn);
        }
        Self {
            parts,
            tox: health.tox,
        }
    }
}

/// `/datum/component/avatar_connection` on an avatar.
#[derive(Component, Clone, Debug)]
pub struct AvatarConnection {
    /// `old_body_ref`: the bitrunner in the pod.
    pub body: Entity,
    pub pod: Entity,
    pub server: Entity,
    /// No damage taken yet.
    pub nohit: bool,
    /// The body's name (the console's `pilot`).
    pub pilot: String,
    pub seen: Seen,
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // `hololadder/attack_hand`.
    app.add_click_handler(ClickPhase::EmptyHand, 3464, |w, c| {
        ladder_click(w, c.client, c.mob, c.target)
    });
    // `netpod/crowbar_act` -> `default_pry_open` (before `machine_frame` takes it apart).
    app.add_click_handler(ClickPhase::HeldOnTarget, 1595, |w, c| {
        c.held
            .is_some_and(|held| crowbar_act(w, c.client, c.mob, held, c.target))
    });
    app.add_systems(
        Update,
        (
            relay_moves.after(WorldSystems::Movement),
            ladder_steps.after(WorldSystems::Movement),
            (watch_connections, netpods)
                .chain()
                .after(WorldSystems::Movement)
                .run_if(any_with_component::<Netpod>),
        ),
    );
}

pub(super) fn setup_netpod(world: &mut World, pod: Entity) {
    world.entity_mut(pod).insert(Netpod::default());
    world.entity_mut(pod).remove::<Density>();
    refresh_look(world, pod);
}

use crate::tools::now;

/// The `do_after` actor `mob` with what it holds now (a `do_after` stops when that changes).
pub(super) fn actor(world: &World, client: Entity, mob: Entity) -> Actor {
    Actor {
        client,
        mob,
        item: world.get::<Inventory>(mob).and_then(Inventory::active_item),
    }
}

pub use crate::tools::tell_mob;

/// A hololadder at `pos`.
pub fn spawn_ladder(world: &mut World, pos: TilePos) -> Option<Entity> {
    let ladder = crate::construction::spawn_obj(world, HOLOLADDER, pos)?;
    world.entity_mut(ladder).insert(Hololadder);
    Some(ladder)
}

/// `update_icon_state`.
fn refresh_look(world: &mut World, pod: Entity) {
    let Some(state) = world.get::<Netpod>(pod).copied() else {
        return;
    };
    let base = "netpod";
    let icon = if !operational(world, pod) {
        base.to_owned()
    } else if state.open {
        format!("{base}_open_active")
    } else if crate::medical::pods::panel_open(world, pod) {
        format!("{base}_panel")
    } else if occupant(world, pod).is_some() {
        format!("{base}_closed_active")
    } else {
        format!("{base}_closed")
    };
    crate::obj_damage::set_icon_state(world, pod, &icon);
}

/// The mob shut in `pod` (`occupant`).
pub fn occupant(world: &mut World, pod: Entity) -> Option<Entity> {
    crate::medical::pods::occupant(world, pod)
}

/// The pod's quantum server (`find_server`: 4 tiles).
fn pod_server(world: &World, pod: Entity) -> Option<Entity> {
    let &pos = world.get::<TilePos>(pod)?;
    server::nearby::<QuantumServer>(world, pos, MAX_DISTANCE)
}

/// `open_machine`: the occupant out, undense, its healing over.
pub fn open_netpod(world: &mut World, pod: Entity) {
    if let Some(mob) = occupant(world, pod) {
        if world.get::<InStasis>(mob) == Some(&InStasis(pod)) {
            world.entity_mut(mob).remove::<InStasis>();
        }
        crate::closets::let_out(world, mob);
    }
    if let Some(mut p) = world.get_mut::<Netpod>(pod) {
        p.open = true;
        p.healing = false;
        p.auto_open_at = None;
    }
    world.entity_mut(pod).remove::<Density>();
    refresh_look(world, pod);
}

/// `close_machine(target)` without the connection: `mob` in (if given), dense.
fn shut(world: &mut World, pod: Entity, mob: Option<Entity>) {
    let Some(&pos) = world.get::<TilePos>(pod) else {
        return;
    };
    if let Some(mob) = mob {
        world.entity_mut(mob).insert(pos);
        crate::closets::put_mob_inside(world, mob, pod);
    }
    if let Some(mut p) = world.get_mut::<Netpod>(pod) {
        p.open = false;
    }
    world.entity_mut(pod).insert(Density);
    refresh_look(world, pod);
}

/// `mouse_drop_receive` (from `crate::drag`): a human dragged onto an open, working, empty pod
/// is shut in and connects. True when `target` is a netpod.
pub fn mouse_drop_receive(world: &mut World, target: Entity, dragged: Entity) -> bool {
    let Some(pod) = world.get::<Netpod>(target).copied() else {
        return false;
    };
    let human = world.get::<Inventory>(dragged).is_some() && world.get::<Health>(dragged).is_some();
    if !pod.open
        || !human
        || !operational(world, target)
        || crate::medical::pods::panel_open(world, target)
        || occupant(world, target).is_some()
        || world.get::<InCloset>(dragged).is_some()
        || world
            .get::<crate::medical::surgery::Buckled>(dragged)
            .is_some()
    {
        return true;
    }
    shut(world, target, Some(dragged));
    enter_matrix(world, target);
    true
}

/// `enter_matrix()`: the checks, then the 2 s `do_after` of the occupant.
fn enter_matrix(world: &mut World, pod: Entity) {
    let Some(neo) = occupant(world, pod) else {
        return;
    };
    let Some(client) = world.get::<PlayerMob>(neo).map(|p| p.client) else {
        return;
    };
    if world.get::<Stat>(neo).is_none_or(|&s| s == Stat::Dead) {
        tell(world, client, Span::Warning, "неверный пользователь.");
        return;
    }
    let Some(server) = pod_server(world, pod) else {
        tell(world, client, Span::Warning, "нет подключения к серверу!");
        return;
    };
    if world
        .get::<QuantumServer>(server)
        .is_none_or(|s| s.domain.is_none() || !s.is_ready)
    {
        tell(world, client, Span::Warning, "ничего не загружено!!");
        return;
    }
    tell(world, client, Span::Notice, "установка соединения...");
    let who = actor(world, client, neo);
    let started = crate::tools::do_after(
        world,
        who,
        pod,
        CONNECT_DS,
        0,
        Job::Bitrunning(BitrunJob::Connect { pod }),
    );
    if !started {
        open_netpod(world, pod);
    }
}

/// The end of `enter_matrix`: an avatar (reused or new), `validate_entry`, the connection.
pub(super) fn complete_connect(world: &mut World, neo: Entity, pod: Entity) {
    let valid = |world: &mut World| {
        occupant(world, pod) == Some(neo)
            && operational(world, pod)
            && world.get::<PlayerMob>(neo).is_some()
            && !crate::status_effects::knocked_out(world, neo)
    };
    if !valid(world) {
        open_netpod(world, pod);
        return;
    }
    let Some(server) = pod_server(world, pod).filter(|&s| {
        world
            .get::<QuantumServer>(s)
            .is_some_and(|s| s.domain.is_some() && s.is_ready)
    }) else {
        open_netpod(world, pod);
        return;
    };
    let client = world.get::<PlayerMob>(neo).map(|p| p.client);
    let reuse = world
        .get::<Netpod>(pod)
        .and_then(|p| p.avatar)
        .filter(|&a| {
            world.get::<Stat>(a).is_some_and(|&s| s != Stat::Dead)
                && !crate::status_effects::knocked_out(world, a)
                && world.get::<AvatarConnection>(a).is_none()
        });
    let avatar = match reuse {
        Some(a) => a,
        None => match start_new_connection(world, server, neo) {
            Some(a) => a,
            None => {
                if let Some(client) = client {
                    tell(
                        world,
                        client,
                        Span::Warning,
                        "кончилась пропускная способность!",
                    );
                }
                return;
            }
        },
    };
    if let Some(mut p) = world.get_mut::<Netpod>(pod) {
        p.healing = true;
        p.avatar = Some(avatar);
    }
    // `add_healing`: `/datum/status_effect/embryonic` (`TRAIT_STASIS`): no `Life()`, so the
    // body left behind does not fall asleep as SSD (`crate::ssd`).
    world.entity_mut(neo).insert(InStasis(pod));
    if !valid(world) || world.get::<Stat>(avatar).is_none_or(|&s| s == Stat::Dead) {
        open_netpod(world, pod);
        return;
    }
    connect(world, avatar, neo, pod, server);
}

/// `/datum/component/avatar_connection/Initialize`: the mind into the avatar.
fn connect(world: &mut World, avatar: Entity, neo: Entity, pod: Entity, server: Entity) {
    let Some(seen) = world.get::<Health>(avatar).map(Seen::of) else {
        return;
    };
    let pilot = world
        .get::<Name>(neo)
        .map(|n| n.to_string())
        .unwrap_or_default();
    if !brainmob::transfer_mind(world, neo, avatar) {
        open_netpod(world, pod);
        return;
    }
    world.entity_mut(avatar).insert(AvatarConnection {
        body: neo,
        pod,
        server,
        nohit: true,
        pilot,
        seen,
    });
    if let Some(mut s) = world.get_mut::<QuantumServer>(server)
        && !s.connections.contains(&avatar)
    {
        s.connections.push(avatar);
    }
    if let Some(mut p) = world.get_mut::<Netpod>(pod) {
        p.connected = true;
    }
    let help = world
        .get::<QuantumServer>(server)
        .and_then(|s| s.domain.as_ref())
        .map(|d| d.domain.help_text)
        .unwrap_or_default();
    if !help.is_empty() {
        tell_mob(world, avatar, Span::Notice, help);
    }
    info!("bitrunning: {neo} connected to avatar {avatar}");
}

/// `start_new_connection`: a free exit (`get_avatar_destination`), its hololadder, the avatar.
fn start_new_connection(world: &mut World, server: Entity, neo: Entity) -> Option<Entity> {
    let state = world.get::<QuantumServer>(server)?.clone();
    let domain = state.domain.as_ref()?;
    if state.retries_spent as usize >= domain.exit_turfs.len() {
        return None;
    }
    let exit = domain.exit_turfs.iter().copied().find(|&t| {
        !world
            .resource::<SpatialIndex>()
            .on_tile(t)
            .iter()
            .any(|&e| world.get::<Hololadder>(e).is_some())
    })?;
    spawn_ladder(world, exit)?;
    if let Some(mut s) = world.get_mut::<QuantumServer>(server) {
        s.retries_spent += 1;
    }
    let name = world
        .get::<Name>(neo)
        .map(|n| n.to_string())
        .unwrap_or_default();
    let forced = domain.domain.forced_outfit;
    Some(generate_avatar(world, exit, name, forced))
}

/// The job outfit `path` (a domain's `forced_outfit` that is a job's), else the bitrunner's.
fn avatar_outfit(world: &World, forced: Option<&str>) -> Option<(Outfit, bool)> {
    let jobs = world.get_resource::<crate::jobs::Jobs>()?;
    let find = |path: &str| {
        jobs.data
            .jobs
            .iter()
            .filter_map(|j| j.outfit.as_ref())
            .find(|o| o.path == path)
            .cloned()
    };
    forced
        .and_then(|p| find(p).map(|o| (o, true)))
        .or_else(|| find(BITRUNNER_OUTFIT).map(|o| (o, false)))
}

/// `generate_avatar(destination, netsuit)`.
fn generate_avatar(world: &mut World, pos: TilePos, name: String, forced: Option<&str>) -> Entity {
    let seed = world
        .resource_mut::<crate::combat::CombatRng>()
        .range(0, u32::MAX);
    let look = crate::human::AppearancePrefs::random(u64::from(seed)).body();
    let mob = crate::server::spawn_body(world, pos, name.clone(), look);
    // `AddComponent(/datum/component/simple_bodycam, ..., BITRUNNER_CAMERA_NET, emp_proof)`.
    crate::silicon::mob_cameras::add_simple_bodycam(
        world,
        mob,
        format!("Avatar {name}"),
        "bitrunner",
        true,
    );
    let mut inventory = match avatar_outfit(world, forced) {
        Some((mut outfit, forced)) => {
            outfit.items.retain(|(slot, _)| {
                !STRIPPED.contains(slot)
                    && (forced || !matches!(slot, OutfitSlot::LHand | OutfitSlot::RHand))
            });
            outfit.accessory = None;
            world
                .try_resource_scope(|world, templates: Mut<crate::outfit::OutfitTemplates>| {
                    crate::outfit::equip_outfit(world, mob, &outfit, &templates.0)
                })
                .unwrap_or_default()
        }
        None => Inventory::default(),
    };
    crate::species::equip_species_outfit(world, mob, &mut inventory);
    inventory.back = crate::storage::spawn_backpack(world, mob);
    if let Some(pack) = inventory.back {
        for path in AVATAR_BACKPACK {
            let Some(item) = crate::construction::spawn_detached(world, path) else {
                continue;
            };
            if crate::storage::can_insert(world, pack, item).is_err() {
                world.despawn(item);
                continue;
            }
            world.entity_mut(item).insert(Networked);
            crate::storage::put_inside(world, item, pack);
        }
    }
    world.entity_mut(mob).insert(inventory);
    crate::inventory::refresh_look(world, mob);
    mob
}

/// `full_avatar_disconnect(cause_damage)`: the mind back in the body, the pod's
/// `disconnect_occupant`, the server forgets the connection.
pub fn full_disconnect(world: &mut World, avatar: Entity, cause_damage: bool) {
    let Some(conn) = world
        .get_entity_mut(avatar)
        .ok()
        .and_then(|mut e| e.take::<AvatarConnection>())
    else {
        return;
    };
    if world.get_entity(conn.body).is_ok() {
        brainmob::transfer_mind(world, avatar, conn.body);
    }
    if let Some(mut s) = world.get_mut::<QuantumServer>(conn.server) {
        s.connections.retain(|&a| a != avatar);
    }
    let servo = if world.get_entity(conn.server).is_ok() {
        server::server_parts(world, conn.server).servo_bonus
    } else {
        0.0
    };
    disconnect_occupant(world, conn.pod, cause_damage, servo);
    info!("bitrunning: avatar {avatar} disconnected (forced: {cause_damage})");
}

/// `disconnect_occupant(cause_damage)`.
fn disconnect_occupant(world: &mut World, pod: Entity, cause_damage: bool, servo_bonus: f32) {
    if let Some(mut p) = world.get_mut::<Netpod>(pod) {
        p.connected = false;
    } else {
        return;
    }
    let Some(mob) = occupant(world, pod) else {
        open_netpod(world, pod);
        return;
    };
    if world.get::<Stat>(mob).is_none_or(|&s| s == Stat::Dead) {
        open_netpod(world, pod);
        return;
    }
    crate::status_effects::apply(world, mob, crate::status_effects::Incap::Paralyze, 2.0);
    if !operational(world, pod) {
        open_netpod(world, pod);
        return;
    }
    let hurt = world.get::<Health>(mob).is_some_and(|h| h.health() < h.max);
    let stat = world.get::<Stat>(mob).copied().unwrap_or_default();
    // `(stat + 2) * 5` seconds to heal when hurt.
    let heal_secs = if hurt {
        f64::from((stat as u8 + 2) * 5)
    } else {
        1.0
    };
    let at = now(world) + heal_secs;
    if let Some(mut p) = world.get_mut::<Netpod>(pod) {
        p.auto_open_at = Some(at);
    }
    if !cause_damage {
        return;
    }
    let damage = BASE_DISCONNECT_DAMAGE * (1.0 - servo_bonus);
    crate::medical::organs::adjust_organ_loss(
        world,
        mob,
        crate::medical::organs::Slot::Brain,
        damage,
    );
    tell_mob(
        world,
        mob,
        Span::Danger,
        "Вы были насильно отключены от своего аватара! Ваши мысли в смятении!",
    );
}

/// The connections' forced disconnects and linked damage, every tick.
// Cost: one pass over the connected avatars (a handful) per tick.
fn watch_connections(world: &mut World) {
    let mut query = world.query::<(Entity, &AvatarConnection)>();
    let all: Vec<(Entity, AvatarConnection)> =
        query.iter(world).map(|(e, c)| (e, c.clone())).collect();
    for (avatar, conn) in all {
        let gone = |e: Entity| world.get_entity(e).is_err();
        if gone(conn.body) || gone(conn.pod) || gone(conn.server) {
            full_disconnect(world, avatar, true);
            continue;
        }
        let dead = |world: &World, e: Entity| world.get::<Stat>(e).is_none_or(|&s| s == Stat::Dead);
        let severed = dead(world, avatar)
            || dead(world, conn.body)
            || crate::status_effects::knocked_out(world, conn.body)
            || world.get::<InCloset>(conn.body).map(|c| c.closet) != Some(conn.pod)
            || !operational(world, conn.pod)
            || !operational(world, conn.server);
        if severed {
            full_disconnect(world, avatar, true);
            continue;
        }
        if link_damage(world, avatar, &conn) {
            full_disconnect(world, avatar, true);
        }
    }
}

/// `on_linked_damage`: what the avatar took since last seen goes to the body. True when the
/// connection must break (a killing blow, or the body knocked out).
fn link_damage(world: &mut World, avatar: Entity, conn: &AvatarConnection) -> bool {
    let Some(now_seen) = world.get::<Health>(avatar).map(Seen::of) else {
        return false;
    };
    if now_seen == conn.seen {
        return false;
    }
    let mut hits: Vec<(BodyZone, DamageType, f32)> = Vec::new();
    for zone in BodyZone::ALL {
        let (b0, f0) = conn.seen.parts[zone.index()];
        let (b1, f1) = now_seen.parts[zone.index()];
        if b1 > b0 {
            hits.push((zone, DamageType::Brute, b1 - b0));
        }
        if f1 > f0 {
            hits.push((zone, DamageType::Burn, f1 - f0));
        }
    }
    let tox = (now_seen.tox - conn.seen.tox).max(0.0);
    if let Some(mut c) = world.get_mut::<AvatarConnection>(avatar) {
        c.seen = now_seen;
        if !hits.is_empty() || tox > 0.0 {
            c.nohit = false;
        }
    }
    let total: f32 = hits.iter().map(|h| h.2).sum::<f32>() + tox;
    if total <= 0.0 {
        return false;
    }
    let body_health = world.get::<Health>(conn.body).map_or(0.0, Health::health);
    if total >= body_health + MAX_LIVING_HEALTH {
        return true;
    }
    if let Ok(mut body) = world.get_entity_mut(conn.body)
        && let Ok((mut health, mut stat)) = body.get_components_mut::<(&mut Health, &mut Stat)>()
    {
        for (zone, kind, amount) in hits {
            crate::health::apply_damage_zone(&mut health, &mut stat, zone, kind, amount);
        }
        if tox > 0.0 {
            crate::health::apply_damage(&mut health, &mut stat, DamageType::Tox, tox);
        }
    }
    crate::status_effects::knocked_out(world, conn.body)
}

/// The pods' healing (`netpod_healing/process`) and `auto_disconnect`.
// Cost: one pass over the netpods (a handful) per tick.
fn netpods(world: &mut World) {
    let now = now(world);
    let secs = world.resource::<Time>().delta_secs();
    let mut query = world.query::<(Entity, &Netpod)>();
    let pods: Vec<(Entity, Netpod)> = query.iter(world).map(|(e, p)| (e, *p)).collect();
    for (pod, state) in pods {
        if state.open {
            continue;
        }
        let Some(mob) = occupant(world, pod) else {
            continue;
        };
        if state.healing
            && let Ok(mut body) = world.get_entity_mut(mob)
            && let Ok((mut health, mut stat)) =
                body.get_components_mut::<(&mut Health, &mut Stat)>()
            && (health.brute > 0.0 || health.burn > 0.0 || health.tox > 0.0)
        {
            let heal = -BASE_HEAL * secs;
            for kind in [DamageType::Brute, DamageType::Burn, DamageType::Tox] {
                crate::health::apply_damage(&mut health, &mut stat, kind, heal);
            }
        }
        if !state.connected && state.auto_open_at.is_some_and(|t| t <= now) {
            tell_mob(
                world,
                mob,
                Span::Notice,
                "Машина отключается и начинает сливать воду.",
            );
            open_netpod(world, pod);
        }
    }
}

/// `relaymove` -> `container_resist_act`: a step from inside opens the pod.
fn relay_moves(world: &mut World, mut moves: Local<MessageCursor<ClosetRelayMove>>) {
    let mobs: Vec<Entity> = moves
        .read(world.resource::<Messages<ClosetRelayMove>>())
        .map(|m| m.mob)
        .collect();
    for mob in mobs {
        let Some(pod) = world.get::<InCloset>(mob).map(|c| c.closet) else {
            continue;
        };
        if world.get::<Netpod>(pod).is_none_or(|p| p.open) {
            continue;
        }
        tell_mob(world, mob, Span::Notice, "Вы выбираетесь из нетпода!");
        open_netpod(world, pod);
    }
}

/// `hololadder/attack_hand` -> `disconnect`. True when `target` is a hololadder.
fn ladder_click(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<Hololadder>(target).is_none() {
        return false;
    }
    start_ladder(world, client, mob, target);
    true
}

fn start_ladder(world: &mut World, client: Entity, mob: Entity, ladder: Entity) {
    if world.get::<AvatarConnection>(mob).is_none() {
        tell(world, client, Span::Warning, "соединение не обнаружено");
        return;
    }
    if world.get::<crate::tools::DoAfter>(mob).is_some() {
        return;
    }
    tell(world, client, Span::Notice, "отключение...");
    let who = actor(world, client, mob);
    crate::tools::do_after(
        world,
        who,
        ladder,
        LADDER_DS,
        0,
        Job::Bitrunning(BitrunJob::Ladder),
    );
}

/// `hololadder/on_enter`: an avatar stepping onto a ladder starts disconnecting (a step, not a
/// spawn: BYOND's `new` does not call `Entered`).
fn ladder_steps(
    mut moves: MessageReader<ss13_world::Moved>,
    avatars: Query<&PlayerMob, With<AvatarConnection>>,
    ladders: Query<(), With<Hololadder>>,
    index: Res<SpatialIndex>,
    mut commands: Commands,
) {
    for moved in moves.read() {
        let (mob, pos) = (moved.entity, moved.to);
        let Ok(player) = avatars.get(mob) else {
            continue;
        };
        let Some(ladder) = index
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| ladders.contains(e))
        else {
            continue;
        };
        let client = player.client;
        commands.queue(move |world: &mut World| start_ladder(world, client, mob, ladder));
    }
}

/// The ladder's `do_after` done: `COMSIG_BITRUNNER_LADDER_SEVER`.
pub(super) fn complete_ladder(world: &mut World, mob: Entity, _ladder: Entity) {
    if world.get::<AvatarConnection>(mob).is_some() {
        full_disconnect(world, mob, false);
    }
}

/// `crowbar_act` -> `default_pry_open`. True when the click was the netpod's.
fn crowbar_act(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(pod) = world.get::<Netpod>(target).copied() else {
        return false;
    };
    let crowbar = world
        .get::<DmTool>(held)
        .and_then(|t| ToolKind::from_dm(&t.behaviour))
        == Some(ToolKind::Crowbar);
    if !crowbar {
        return false;
    }
    if occupant(world, target).is_none() {
        if crate::medical::pods::panel_open(world, target) {
            // `default_deconstruction_crowbar`: the frame's business.
            return false;
        }
        if pod.open {
            shut(world, target, None);
        } else {
            open_netpod(world, target);
        }
        return true;
    }
    tell(
        world,
        client,
        Span::Notice,
        "Вы начинаете вскрывать нетпод.",
    );
    if let Some(avatar) = pod.avatar.filter(|_| pod.connected) {
        tell_mob(
            world,
            avatar,
            Span::Danger,
            "Нарушение работы нетпода: кто-то пытается вскрыть нетпод. Найдите выход.",
        );
    }
    crate::tools::use_tool(
        world,
        Actor {
            client,
            mob,
            item: Some(held),
        },
        target,
        PRY_DS,
        0,
        Job::Bitrunning(BitrunJob::PryPod),
    );
    true
}

/// The pry done: `sever_connection` + `open_machine`.
pub(super) fn complete_pry(world: &mut World, pod: Entity) {
    let Some(state) = world.get::<Netpod>(pod).copied() else {
        return;
    };
    if state.open {
        return;
    }
    if state.connected
        && let Some(avatar) = state.avatar
    {
        full_disconnect(world, avatar, true);
    }
    open_netpod(world, pod);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seen_damage_reads_every_bodypart() {
        let mut health = Health::default();
        health.damage_zone(BodyZone::Head, DamageType::Brute, 10.0);
        health.adjust(DamageType::Tox, 5.0);
        let seen = Seen::of(&health);
        assert!(seen.parts[BodyZone::Head.index()].0 > 0.0);
        assert_eq!(seen.parts[BodyZone::Chest.index()], (0.0, 0.0));
        assert!(seen.tox > 0.0);
        assert_eq!(Seen::of(&Health::default()), Seen::default());
    }
}
