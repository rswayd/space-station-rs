//! The ghost's verbs and menus (server), after tg `/mob/dead/observer` verbs, `/datum/hud/ghost`,
//! `/datum/orbit_menu`, `/datum/ghost_menu`, `/datum/spawners_menu` and `/mob/verb/abandon_mob`.
//!
//! - Every ghost with a client gets the HUD buttons ([`GhostHudUi`]): Orbit, the ghost menu,
//!   spawners, re-enter corpse, respawn.
//! - Orbit ([`OrbitUi`]): the ghost follows a mob or another ghost onto its tile
//!   ([`Orbiting`]); with `auto_observe` it also observes it ([`Observing`]: tg `do_observe`):
//!   the client sees the observed mob's HUD (alerts, hunger, mood, fullscreen overlays), which
//!   it cannot work. The ghost's own step ends both.
//! - The ghost menu ([`GhostMenuUi`]): re-enter corpse, DNR, darkness (tg `ghost_lightings`,
//!   the client's lighting cutoff), ghost vision (`GHOST_VISION`: other ghosts and ghost-only
//!   things), the health scan on click (`GHOST_HEALTH`), a T-ray scan, and with tg `fun_verbs` ([`FunVerbs`]) Boo (a light within a tile flickers, once a
//!   minute) and Possess (a living mob without a mind, not megafauna).
//! - Spawners ([`SpawnersMenuUi`]): the ghost roles other modules place ([`GhostRoleSpawner`]);
//!   taking one gives the client a new human at it.
//! - Respawn ([`HUD_RESPAWN`]): back to the lobby when [`RespawnConfig`] allows it, after its
//!   delay since death.
// shortcut: no view range, data HUDs or chem/gas scans in the ghost menu, no
// notification (poll ignore) list, no pAI sign-up, no crew manifest button; settings do not
// outlive the ghost (tg keeps darkness in the preferences). Spawners hold no outfit and the map's
// `/obj/effect/mob_spawn/ghost_role` objects are not converted into them yet. No minigames menu:
// the port has no minigames.

use std::collections::HashSet;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::query::QueryItem;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_net::chat::server::SendChat;
use ss13_net::server::{ControlledEntity, SeeInvisible, Sight, Viewer, VisibleToMobs};
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};
use ss13_tgui::server::UiWindow;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::{SpatialIndex, WorldSystems};

use crate::ghost::{Ghost, Observing, Orbiting, SEE_INVISIBLE_OBSERVER};
use crate::ghost_menu::*;
use crate::health::Stat;
use crate::server::{PlayerMob, Players};

/// tg `SEE_INVISIBLE_LIVING`: a ghost without ghost vision.
pub const SEE_INVISIBLE_LIVING: u8 = 25;
/// tg `boo`'s `COOLDOWN_START(src, bootime, 60 SECONDS)`.
pub const BOO_COOLDOWN_SECS: f64 = 60.0;
/// How often the Orbit and possess lists are rebuilt while one is open (tgui autoupdate).
const POI_REFRESH_SECS: f64 = 1.0;

/// On a ghost: its settings (tg `lighting_cutoff`, `ghost_hud_flags & GHOST_VISION`, `bootime`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct GhostSettings {
    /// Index into [`GHOST_LIGHTINGS`].
    pub darkness: u8,
    pub vision: bool,
    /// `ghost_hud_flags & GHOST_HEALTH`.
    pub health_scan: bool,
    pub boo_ready_at: f64,
    /// tg `persistent_client.time_of_death`: the body's death, 0 for a lobby observer.
    pub died_at: f64,
}

impl Default for GhostSettings {
    fn default() -> Self {
        Self {
            darkness: 0,
            vision: true,
            health_scan: false,
            boo_ready_at: 0.0,
            died_at: 0.0,
        }
    }
}

/// On a ghost: tg `fun_verbs` (Boo and Possess), given by the wizard's "Possessing G-G-G-Ghosts!"
/// event or an admin.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct FunVerbs;

/// A ghost role spawner (tg `/obj/effect/mob_spawn/ghost_role`), placed by the module that owns
/// the role; it needs a `TilePos`. Listed in the spawners menu while `uses` is above 0.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct GhostRoleSpawner {
    /// tg `name`: the role, groups spawners in the menu.
    pub name: String,
    /// tg `you_are_text` / `flavour_text`.
    pub desc: String,
    /// tg `uses`.
    pub uses: u32,
    /// The map spawner type ([`ss13_mapload::mob_spawn`]) whose human it makes; `None`: a
    /// plain human with the player's look.
    pub def: Option<String>,
}

/// tg `CONFIG_GET(flag/allow_respawn)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RespawnMode {
    /// `RESPAWN_FLAG_DISABLED` (the default).
    #[default]
    Disabled,
    /// `RESPAWN_FLAG_FREE`.
    Free,
    /// `RESPAWN_FLAG_NEW_CHARACTER`: the player is told to come back as another character.
    // shortcut: not enforced (the port has one character per connection); enforce once there
    // are character slots.
    NewCharacter,
}

/// tg `allow_respawn` and `respawn_delay`: `SS13_ALLOW_RESPAWN` (0, 1, 2 as tg's config) and
/// `SS13_RESPAWN_DELAY` (seconds).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct RespawnConfig {
    pub mode: RespawnMode,
    pub delay_secs: f64,
}

impl Default for RespawnConfig {
    fn default() -> Self {
        let mode = match std::env::var("SS13_ALLOW_RESPAWN")
            .ok()
            .as_deref()
            .map(str::trim)
        {
            Some("1") => RespawnMode::Free,
            Some("2") => RespawnMode::NewCharacter,
            _ => RespawnMode::Disabled,
        };
        let delay_secs = std::env::var("SS13_RESPAWN_DELAY")
            .ok()
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v >= 0.0)
            .unwrap_or(0.0);
        Self { mode, delay_secs }
    }
}

/// The points of interest (tg `SSpoints_of_interest.get_mob_pois`), rebuilt while a menu that
/// shows them is open.
#[derive(Resource, Default, Debug)]
pub struct PoiList {
    pub orbit: OrbitData,
    pub possessable: Vec<OrbitEntry>,
}

pub struct GhostVerbsPlugin;

impl Plugin for GhostVerbsPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<GhostHudUi>()
            .register_ui::<OrbitUi>()
            .register_ui::<GhostMenuUi>()
            .register_ui::<SpawnersMenuUi>()
            .init_resource::<RespawnConfig>()
            .init_resource::<PoiList>()
            .add_observer(setup_ghost)
            .add_systems(
                Update,
                follow_orbits
                    .after(WorldSystems::Movement)
                    .before(UiSystems::Open),
            )
            .add_systems(Update, refresh_pois.before(UiSystems::Open))
            .add_systems(Update, ghost_acts.in_set(UiSystems::Handle))
            .add_systems(Update, ghost_scans);
    }
}

/// A new ghost: its settings, its own sprite visible to it whatever its vision, and with a
/// client the HUD buttons.
fn setup_ghost(
    add: On<Add<Ghost>>,
    ghosts: Query<(&Ghost, Option<&PlayerMob>)>,
    deaths: Query<&crate::death::TimeOfDeath>,
    mut open: MessageWriter<OpenUi>,
    mut commands: Commands,
) {
    let ghost = add.entity;
    let Ok((g, player)) = ghosts.get(ghost) else {
        return;
    };
    let died_at = g.body.and_then(|b| deaths.get(b).ok()).map_or(0.0, |t| t.0);
    commands.entity(ghost).try_insert((
        GhostSettings {
            died_at,
            ..default()
        },
        VisibleToMobs(vec![ghost]),
    ));
    if let Some(player) = player {
        open.write(OpenUi::new::<GhostHudUi>(player.client, ghost, ghost, ""));
    }
}

fn chat(world: &mut World, client: Entity, span: Span, text: impl Into<String>) {
    world.write_message(SendChat {
        clients: vec![client],
        message: ChatMessage::simple(ChatChannel::System, span, text),
    });
}

/// The ghost `client` controls now, if `ghost` is it.
fn own_ghost(world: &World, client: Entity, ghost: Entity) -> bool {
    world.get::<Ghost>(ghost).is_some()
        && world.get::<PlayerMob>(ghost).map(|p| p.client) == Some(client)
        && world.resource::<Players>().mob(client) == Some(ghost)
}

/// A mob with a mind: a client, or a ghost that left it.
fn minded(world: &World, mob: Entity, ghost_bodies: &HashSet<Entity>) -> bool {
    world.get::<PlayerMob>(mob).is_some() || ghost_bodies.contains(&mob)
}

fn ghost_bodies(world: &mut World) -> HashSet<Entity> {
    world
        .query::<&Ghost>()
        .iter(world)
        .filter_map(|g| g.body)
        .collect()
}

/// tg `is_valid_poi` for mobs: a mob on the map, or a ghost with a player.
pub fn is_poi(world: &World, e: Entity) -> bool {
    let Ok(entity) = world.get_entity(e) else {
        return false;
    };
    entity.contains::<TilePos>()
        && entity.contains::<Name>()
        && (entity.contains::<Stat>()
            || (entity.contains::<Ghost>() && entity.contains::<PlayerMob>()))
}

/// tg `possess`'s list entry: alive, no mind, not megafauna (`ismegafauna`), on the map.
fn possessable(world: &World, mob: Entity, ghost_bodies: &HashSet<Entity>) -> bool {
    let Ok(entity) = world.get_entity(mob) else {
        return false;
    };
    entity.get::<Stat>().is_some_and(|s| *s != Stat::Dead)
        && entity.contains::<TilePos>()
        && !entity.contains::<Ghost>()
        && !entity.contains::<crate::mobs::megafauna::Megafauna>()
        && !minded(world, mob, ghost_bodies)
}

/// Rebuilds [`PoiList`] once a second while an Orbit or ghost menu window is open.
// Cost: one pass over the open windows per tick; with a menu open, one pass over the mobs a
// second.
fn refresh_pois(world: &mut World, mut next: Local<f64>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < *next {
        return;
    }
    let open = world.query::<&UiWindow>().iter(world).any(|w| {
        w.interface == <OrbitUi as ss13_tgui::UiInterface>::NAME
            || w.interface == <GhostMenuUi as ss13_tgui::UiInterface>::NAME
    });
    if !open {
        return;
    }
    *next = now + POI_REFRESH_SECS;
    let bodies = ghost_bodies(world);
    let mut q = world.query_filtered::<(Entity, &Name, Option<&Stat>, Has<PlayerMob>, Has<Ghost>), With<TilePos>>();
    let found: Vec<(Entity, String, Option<Stat>, bool, bool)> = q
        .iter(world)
        .map(|(e, n, s, p, g)| (e, n.to_string(), s.copied(), p, g))
        .collect();
    let mut list = PoiList::default();
    for (e, full_name, stat, player, ghost) in found {
        let entry = OrbitEntry {
            full_name,
            key: e.to_bits(),
        };
        if ghost {
            if player {
                list.orbit.ghosts.push(entry);
            }
            continue;
        }
        let Some(stat) = stat else {
            continue;
        };
        if player || bodies.contains(&e) {
            if stat == Stat::Dead {
                list.orbit.dead.push(entry);
            } else {
                list.orbit.alive.push(entry);
            }
        } else {
            if possessable(world, e, &bodies) {
                list.possessable.push(entry.clone());
            }
            list.orbit.npcs.push(entry);
        }
    }
    for l in [
        &mut list.orbit.alive,
        &mut list.orbit.dead,
        &mut list.orbit.ghosts,
        &mut list.orbit.npcs,
        &mut list.possessable,
    ] {
        l.sort_by(|a, b| a.full_name.cmp(&b.full_name).then(a.key.cmp(&b.key)));
    }
    *world.resource_mut::<PoiList>() = list;
}

/// tg `ManualFollow` (+ `do_observe` with `observe`): the ghost follows `target` onto its tile;
/// observing also shows the client the target's HUD. False when the target is no POI.
pub fn orbit(world: &mut World, ghost: Entity, target: Entity, observe: bool) -> bool {
    if target == ghost || !is_poi(world, target) || world.get::<Ghost>(ghost).is_none() {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return false;
    };
    let before = world.get::<Observing>(ghost).copied();
    world.entity_mut(ghost).insert((Orbiting { target }, pos));
    // tg `reset_perspective(null)` first: the old target's HUD closes (`ghost::
    // close_foreign_windows`).
    if !observe || world.get::<Stat>(target).is_none() {
        world.entity_mut(ghost).remove::<Observing>();
        return true;
    }
    world.entity_mut(ghost).insert(Observing { target });
    if before.map(|o| o.target) == Some(target) {
        return true;
    }
    if let Some(client) = world.get::<PlayerMob>(ghost).map(|p| p.client) {
        open_observed_huds(world, client, target);
    }
    true
}

/// `mob_eye.hud_used.show_hud(..., src)`: the observed mob's display-only HUD windows.
// shortcut: not its inventory, health doll or action buttons (they share the client's own HUD
// windows); mirror them when the client HUD can show a second mob.
fn open_observed_huds(world: &mut World, client: Entity, target: Entity) {
    let Ok(e) = world.get_entity(target) else {
        return;
    };
    let mut open = Vec::new();
    if e.contains::<crate::hud::AlertHudData>() {
        open.push(OpenUi::new::<crate::hud::AlertHudUi>(
            client, target, target, "",
        ));
    }
    if e.contains::<crate::hud::HungerHudData>() {
        open.push(OpenUi::new::<crate::hud::HungerHudUi>(
            client, target, target, "",
        ));
    }
    if e.contains::<crate::hud::MoodHudData>() {
        open.push(OpenUi::new::<crate::hud::MoodHudUi>(
            client, target, target, "",
        ));
    }
    if e.contains::<crate::hud::FullscreenHudData>() {
        open.push(OpenUi::new::<crate::hud::FullscreenHudUi>(
            client, target, target, "",
        ));
    }
    for message in open {
        world.write_message(message);
    }
}

/// The orbiters follow their targets; a gone target (or one off the map) ends the orbit.
// Cost: per tick, one lookup per orbiting ghost.
fn follow_orbits(
    ghosts: Query<(Entity, &Orbiting, &TilePos)>,
    targets: Query<&TilePos>,
    mut commands: Commands,
) {
    for (ghost, orbit, pos) in &ghosts {
        match targets.get(orbit.target) {
            Ok(to) if to != pos => {
                commands.entity(ghost).insert(*to);
            }
            Ok(_) => {}
            Err(_) => {
                commands.entity(ghost).remove::<(Orbiting, Observing)>();
            }
        }
    }
}

/// The ghost menu's darkness: `level` is a name of [`GHOST_LIGHTINGS`]. False when unknown or
/// already set.
pub fn set_darkness(world: &mut World, ghost: Entity, level: &str) -> bool {
    let Some(i) = GHOST_LIGHTINGS.iter().position(|(n, _)| *n == level) else {
        return false;
    };
    let Some(mut settings) = world.get_mut::<GhostSettings>(ghost) else {
        return false;
    };
    if usize::from(settings.darkness) == i {
        return false;
    }
    settings.darkness = i as u8;
    true
}

/// tg `toggle_ghostsee` -> `update_sight`: without ghost vision the client sees only what the
/// living see (`SEE_INVISIBLE_LIVING`), its own ghost still (`VisibleToMobs`).
pub fn toggle_vision(world: &mut World, ghost: Entity) -> Option<bool> {
    let client = world.get::<PlayerMob>(ghost)?.client;
    let mut settings = world.get_mut::<GhostSettings>(ghost)?;
    settings.vision = !settings.vision;
    let vision = settings.vision;
    let see = if vision {
        SEE_INVISIBLE_OBSERVER
    } else {
        SEE_INVISIBLE_LIVING
    };
    if let Ok(mut c) = world.get_entity_mut(client) {
        c.insert(SeeInvisible(see));
    }
    let text = if vision {
        "You now have ghost vision."
    } else {
        "You no longer have ghost vision."
    };
    chat(world, client, Span::BoldNotice, text);
    Some(vision)
}

/// tg `toggle_health_scan`.
pub fn toggle_health_scan(world: &mut World, ghost: Entity) -> Option<bool> {
    let client = world.get::<PlayerMob>(ghost)?.client;
    let mut settings = world.get_mut::<GhostSettings>(ghost)?;
    settings.health_scan = !settings.health_scan;
    let on = settings.health_scan;
    let text = if on {
        "Health scan enabled."
    } else {
        "Health scan disabled."
    };
    chat(world, client, Span::Notice, text);
    Some(on)
}

/// `/mob/living/attack_ghost` with `GHOST_HEALTH`: the clicked mob's health scan to the ghost.
// shortcut: tg scans with `SCANPOWER_SUPER`; this is the basic analyzer's readout.
fn ghost_scans(world: &mut World, mut clicks: Local<MessageCursor<crate::multiz::GhostClick>>) {
    let Some(messages) = world.get_resource::<Messages<crate::multiz::GhostClick>>() else {
        return;
    };
    let pending: Vec<crate::multiz::GhostClick> = clicks.read(messages).copied().collect();
    for click in pending {
        // tg `mob_spawn/ghost_role/attack_ghost`: the spawner's role, through the menu.
        if world.get::<GhostRoleSpawner>(click.target).is_some()
            && world.get::<PlayerMob>(click.ghost).map(|p| p.client) == Some(click.client)
        {
            world.write_message(OpenUi::new::<SpawnersMenuUi>(
                click.client,
                click.ghost,
                click.ghost,
                "Spawners Menu",
            ));
            continue;
        }
        let scanning = world
            .get::<GhostSettings>(click.ghost)
            .is_some_and(|s| s.health_scan)
            && world.get::<PlayerMob>(click.ghost).map(|p| p.client) == Some(click.client);
        if !scanning {
            continue;
        }
        if let Some(message) = crate::health_scan::healthscan(world, click.target) {
            world.write_message(SendChat {
                clients: vec![click.client],
                message,
            });
        }
    }
}

/// tg `boo`: with fun verbs and the cooldown over, a light fixture within a tile flickers once;
/// the cooldown starts only when it did.
pub fn boo(world: &mut World, ghost: Entity) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let (Some(settings), Some(&pos)) = (
        world.get::<GhostSettings>(ghost).copied(),
        world.get::<TilePos>(ghost),
    ) else {
        return false;
    };
    if world.get::<FunVerbs>(ghost).is_none() || now < settings.boo_ready_at {
        return false;
    }
    // `locate(/obj/machinery/light) in view(1, src)`.
    let light = world
        .resource::<SpatialIndex>()
        .in_radius(pos, 1)
        .find(|&e| world.get::<crate::power::LightFixture>(e).is_some());
    let Some(light) = light else {
        return false;
    };
    if !crate::power::lighting::flicker(world, light, 1) {
        return false;
    }
    if let Some(mut s) = world.get_mut::<GhostSettings>(ghost) {
        s.boo_ready_at = now + BOO_COOLDOWN_SECS;
    }
    true
}

/// tg `possess`: with fun verbs, the client takes a living mob without a mind (checked again
/// now: someone may have taken it meanwhile); the ghost is gone and its old body has no mind.
pub fn possess(world: &mut World, ghost: Entity, target: Entity) -> bool {
    if world.get::<FunVerbs>(ghost).is_none() {
        return false;
    }
    let Some(client) = world.get::<PlayerMob>(ghost).map(|p| p.client) else {
        return false;
    };
    let bodies = ghost_bodies(world);
    if !possessable(world, target, &bodies) {
        chat(
            world,
            client,
            Span::Warning,
            "Someone has taken this body while you were choosing!",
        );
        return false;
    }
    if !crate::medical::brainmob::possess(world, client, ghost, target) {
        return false;
    }
    // `set_faction(list(FACTION_NEUTRAL))`.
    if let Some(mut faction) = world.get_mut::<crate::mobs::Faction>(target) {
        faction.0 = vec![crate::mobs::Faction::NEUTRAL.to_owned()];
    }
    world.despawn(ghost);
    info!("client {client} possessed {target}");
    true
}

/// tg `reenter_corpse`: back into the body it left, if that body still has no other mind.
pub fn reenter_corpse(world: &mut World, ghost: Entity) -> bool {
    let Some(body) = world.get::<Ghost>(ghost).and_then(|g| g.body) else {
        if let Some(client) = world.get::<PlayerMob>(ghost).map(|p| p.client) {
            chat(
                world,
                client,
                Span::Warning,
                "You cannot re-enter your body.",
            );
        }
        return false;
    };
    if world.get_entity(body).is_err() || world.get::<PlayerMob>(body).is_some() {
        return false;
    }
    crate::ghost::return_to_body(world, body);
    true
}

/// tg `stay_dead` (DNR): the ghost may never re-enter its body.
pub fn do_not_resuscitate(world: &mut World, ghost: Entity) -> bool {
    let Some(mut g) = world.get_mut::<Ghost>(ghost) else {
        return false;
    };
    let Some(body) = g.body.take() else {
        return false;
    };
    // The mind still knows its body: a forced `grab_ghost` (admin revive) brings it back.
    world.entity_mut(ghost).insert(crate::mind::DnrBody(body));
    if let Some(client) = world.get::<PlayerMob>(ghost).map(|p| p.client) {
        chat(
            world,
            client,
            Span::BoldNotice,
            "You can no longer be brought back into your body.",
        );
    }
    true
}

/// tg `abandon_mob` (the "Respawn" verb) of a ghost: back to the lobby as a new player when the
/// config allows it and its delay since death is over.
// shortcut: no admin bypass of a disabled respawn or of the delay (tg asks admins); add it with
// the admin rights check in this crate.
pub fn respawn(world: &mut World, client: Entity, ghost: Entity) -> bool {
    if !own_ghost(world, client, ghost) {
        return false;
    }
    let config = *world.resource::<RespawnConfig>();
    if config.mode == RespawnMode::Disabled {
        chat(world, client, Span::BoldNotice, "Возврат в лобби отключен!");
        return false;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let died_at = world.get::<GhostSettings>(ghost).map_or(0.0, |s| s.died_at);
    if config.delay_secs > 0.0 && now - died_at < config.delay_secs {
        let dead = crate::jobs::display_time_text(now - died_at);
        let need = crate::jobs::display_time_text(config.delay_secs);
        chat(
            world,
            client,
            Span::Notice,
            format!("Вы были мертвы {dead}."),
        );
        chat(
            world,
            client,
            Span::Warning,
            format!("Вы должны ждать {need}, чтобы вернуться в лобби!"),
        );
        return false;
    }
    if config.mode == RespawnMode::NewCharacter {
        chat(
            world,
            client,
            Span::Notice,
            "Заметьте, что возвращаться в игру с лобби необходимо другим персонажем.",
        );
    }
    chat(
        world,
        client,
        Span::BoldNotice,
        "Пожалуйста, соблюдайте ролевую игру!",
    );
    world.despawn(ghost);
    world.resource_mut::<Players>().remove(client);
    if let Ok(mut c) = world.get_entity_mut(client) {
        c.remove::<(ControlledEntity, Viewer, SeeInvisible, Sight)>();
    }
    crate::round::return_to_lobby(world, client);
    info!("client {client} respawned to the lobby");
    true
}

/// The spawners of the menu, grouped by role name: the first spawner with uses left speaks for
/// its group.
pub fn spawner_groups<'a>(
    spawners: impl Iterator<Item = (Entity, &'a GhostRoleSpawner)>,
) -> Vec<SpawnerEntry> {
    let mut groups: Vec<SpawnerEntry> = Vec::new();
    let mut all: Vec<(Entity, &GhostRoleSpawner)> = spawners.filter(|(_, s)| s.uses > 0).collect();
    all.sort_by_key(|(e, _)| e.to_bits());
    for (e, s) in all {
        match groups.iter_mut().find(|g| g.name == s.name) {
            Some(g) => g.amount_left += s.uses,
            None => groups.push(SpawnerEntry {
                name: s.name.clone(),
                desc: s.desc.clone(),
                amount_left: s.uses,
                key: e.to_bits(),
            }),
        }
    }
    groups.sort_by(|a, b| a.name.cmp(&b.name));
    groups
}

/// tg `mob_spawn/attack_ghost` -> `create(mob_possessor)`: a human at the spawner for the
/// ghost's client; a spawner without uses left goes.
pub fn take_role(world: &mut World, ghost: Entity, spawner: Entity) -> Option<Entity> {
    let client = world.get::<PlayerMob>(ghost)?.client;
    let s = world.get::<GhostRoleSpawner>(spawner)?.clone();
    let &pos = world.get::<TilePos>(spawner)?;
    let def = s
        .def
        .as_deref()
        .and_then(|p| crate::ghost_roles::def(world, p));
    let infinite = def.as_ref().is_some_and(|d| d.infinite_use);
    if s.uses == 0 && !infinite {
        chat(
            world,
            client,
            Span::Warning,
            "This spawner is out of charges!",
        );
        return None;
    }
    // `SSticker.HasRoundStarted()`.
    if world
        .get_resource::<crate::round::Ticker>()
        .is_some_and(|t| t.state() < crate::lobby::GameState::Playing)
    {
        return None;
    }
    // The spawner type's `allow_spawn` (the battlecruiser's: once per player).
    if !crate::antag::nukeop::battlecruiser::allow_spawn(world, client, s.def.as_deref()) {
        return None;
    }
    let mob = match &def {
        Some(d) => {
            let name = match crate::golems::forced_name(world, spawner) {
                Some(name) => name,
                None => d
                    .mob_name
                    .clone()
                    .unwrap_or_else(|| crate::jobs::character_name(world, client)),
            };
            crate::ghost_roles::create_human(world, d, pos, name)
        }
        None => {
            let name = crate::jobs::character_name(world, client);
            let look = crate::human::player_body(
                client,
                world.get::<crate::human::AppearancePrefs>(client),
            );
            crate::server::spawn_body(world, pos, name, look)
        }
    };
    if !crate::medical::brainmob::possess(world, client, ghost, mob) {
        world.despawn(mob);
        return None;
    }
    world.despawn(ghost);
    // `create`: `set_assigned_role_with_greeting(spawner_job_path)`, `/datum/job/ghost_role`.
    crate::noncrew_jobs::set_assigned_role(world, mob, crate::noncrew_jobs::GHOST_ROLE);
    // The spawner type's `special` (the battlecruiser's crew).
    crate::antag::nukeop::battlecruiser::special(world, mob, client, s.def.as_deref());
    crate::golems::special(world, mob, spawner);
    if !infinite {
        let left = s.uses - 1;
        let deletes = def.as_ref().is_none_or(|d| d.deletes_on_zero_uses_left);
        if left == 0 && deletes {
            world.despawn(spawner);
        } else if let Some(mut sp) = world.get_mut::<GhostRoleSpawner>(spawner) {
            sp.uses = left;
        }
    }
    // `special`'s boxed message: who they are, the context, the rules.
    match &def {
        Some(d) => {
            let lines = [
                (Span::BoldNotice, &d.you_are_text),
                (Span::Notice, &d.flavour_text),
                (Span::Danger, &d.important_text),
            ];
            for (span, text) in lines {
                if !text.is_empty() {
                    chat(world, client, span, text.clone());
                }
            }
        }
        None => chat(world, client, Span::BoldNotice, format!("Вы — {}.", s.name)),
    }
    info!("client {client} took the ghost role {} at {pos:?}", s.name);
    Some(mob)
}

fn parse_ref(params: &UiParams, key: &str) -> Result<Entity, UiReject> {
    params
        .get(key)
        .and_then(|k| k.trim().parse::<u64>().ok())
        .and_then(Entity::try_from_bits)
        .ok_or_else(|| UiReject::BadParams(format!("{key}: expected a listed thing")))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudAct {
    Orbit,
    GhostMenu,
    Spawners,
    Reenter,
    Respawn,
}

impl ServerUi for GhostHudUi {
    type Source = (&'static Ghost, &'static GhostSettings);
    type UserParam = ();
    type Act = HudAct;

    fn static_data(_: QueryItem<'_, '_, Self::Source>) {}

    fn data((ghost, settings): QueryItem<'_, '_, Self::Source>, _: Entity, _: &()) -> GhostHudData {
        GhostHudData {
            lighting_cutoff: GHOST_LIGHTINGS
                .get(usize::from(settings.darkness))
                .map_or(0, |l| l.1),
            has_body: ghost.body.is_some(),
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<HudAct, UiReject> {
        Ok(match action {
            HUD_ORBIT => HudAct::Orbit,
            HUD_GHOST_MENU => HudAct::GhostMenu,
            HUD_SPAWNERS => HudAct::Spawners,
            HUD_REENTER => HudAct::Reenter,
            HUD_RESPAWN => HudAct::Respawn,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrbitAct {
    Orbit { target: Entity, observe: bool },
    Refresh,
}

impl ServerUi for OrbitUi {
    type Source = (Option<&'static Orbiting>, Option<&'static Observing>);
    type UserParam = Res<'static, PoiList>;
    type Act = OrbitAct;

    fn static_data(_: QueryItem<'_, '_, Self::Source>) {}

    fn data(
        (orbiting, observing): QueryItem<'_, '_, Self::Source>,
        _: Entity,
        pois: &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> OrbitData {
        OrbitData {
            orbiting: orbiting.map(|o| o.target.to_bits()),
            observing: observing.map(|o| o.target.to_bits()),
            ..pois.orbit.clone()
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<OrbitAct, UiReject> {
        match action {
            ORBIT => Ok(OrbitAct::Orbit {
                target: parse_ref(params, ORBIT_REF)?,
                observe: params
                    .get(ORBIT_AUTO_OBSERVE)
                    .is_some_and(|v| v == "1" || v == "true"),
            }),
            ORBIT_REFRESH => Ok(OrbitAct::Refresh),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuAct {
    Dnr,
    Return,
    Darkness(String),
    ToggleVision,
    ToggleHealth,
    TRay,
    Boo,
    Possess(Entity),
}

impl ServerUi for GhostMenuUi {
    type Source = (&'static Ghost, &'static GhostSettings, Has<FunVerbs>);
    type UserParam = (
        Res<'static, Time>,
        Res<'static, PoiList>,
        Query<'static, 'static, &'static Name>,
    );
    type Act = MenuAct;

    fn static_data(_: QueryItem<'_, '_, Self::Source>) {}

    fn data(
        (ghost, settings, fun): QueryItem<'_, '_, Self::Source>,
        _: Entity,
        (time, pois, names): &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> GhostMenuData {
        GhostMenuData {
            can_boo: time.elapsed_secs_f64() >= settings.boo_ready_at,
            has_fun: fun,
            body_name: ghost
                .body
                .and_then(|b| names.get(b).ok())
                .map(|n| n.to_string()),
            current_darkness: settings.darkness,
            ghost_vision: settings.vision,
            health_scan: settings.health_scan,
            possessable: if fun {
                pois.possessable.clone()
            } else {
                Vec::new()
            },
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<MenuAct, UiReject> {
        match action {
            MENU_DNR => Ok(MenuAct::Dnr),
            MENU_RETURN => Ok(MenuAct::Return),
            MENU_DARKNESS => params
                .get(MENU_DARKNESS_LEVEL)
                .map(|l| MenuAct::Darkness(l.to_owned()))
                .ok_or_else(|| UiReject::BadParams("darkness_level: missing".into())),
            MENU_TRAY => Ok(MenuAct::TRay),
            MENU_TOGGLE => match params.get(MENU_TOGGLING) {
                Some(GHOST_VISION) => Ok(MenuAct::ToggleVision),
                Some(GHOST_HEALTH) => Ok(MenuAct::ToggleHealth),
                _ => Err(UiReject::BadParams("toggling: unknown flag".into())),
            },
            MENU_BOO => Ok(MenuAct::Boo),
            MENU_POSSESS => Ok(MenuAct::Possess(parse_ref(params, ORBIT_REF)?)),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnerAct {
    Jump(Entity),
    Spawn(Entity),
}

impl ServerUi for SpawnersMenuUi {
    type Source = &'static Ghost;
    type UserParam = Query<'static, 'static, (Entity, &'static GhostRoleSpawner)>;
    type Act = SpawnerAct;

    fn static_data(_: &Ghost) {}

    // Cost: one pass over the spawners (a few per map) per data build of an open menu.
    fn data(
        _: &Ghost,
        _: Entity,
        spawners: &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> SpawnersData {
        SpawnersData {
            spawners: spawner_groups(spawners.iter()),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<SpawnerAct, UiReject> {
        match action {
            SPAWNER_JUMP => Ok(SpawnerAct::Jump(parse_ref(params, SPAWNER_REF)?)),
            SPAWNER_SPAWN => Ok(SpawnerAct::Spawn(parse_ref(params, SPAWNER_REF)?)),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// The acts of all ghost windows, on the client's own ghost only, checked when applied.
fn ghost_acts(
    world: &mut World,
    mut huds: Local<MessageCursor<UiActed<GhostHudUi>>>,
    mut orbits: Local<MessageCursor<UiActed<OrbitUi>>>,
    mut menus: Local<MessageCursor<UiActed<GhostMenuUi>>>,
    mut spawners: Local<MessageCursor<UiActed<SpawnersMenuUi>>>,
) {
    let hud: Vec<(Entity, Entity, Entity, HudAct)> = huds
        .read(world.resource::<Messages<UiActed<GhostHudUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act))
        .collect();
    let orbit_acts: Vec<(Entity, Entity, Entity, OrbitAct)> = orbits
        .read(world.resource::<Messages<UiActed<OrbitUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act))
        .collect();
    let menu: Vec<(Entity, Entity, Entity, MenuAct)> = menus
        .read(world.resource::<Messages<UiActed<GhostMenuUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    let spawn: Vec<(Entity, Entity, Entity, SpawnerAct)> = spawners
        .read(world.resource::<Messages<UiActed<SpawnersMenuUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act))
        .collect();
    let mine = |world: &World, client: Entity, actor: Entity, source: Entity| {
        actor == source && own_ghost(world, client, actor)
    };
    for (client, ghost, source, act) in hud {
        if !mine(world, client, ghost, source) {
            continue;
        }
        match act {
            HudAct::Orbit => {
                world.write_message(OpenUi::new::<OrbitUi>(client, ghost, ghost, "Orbit"));
            }
            HudAct::GhostMenu => {
                world.write_message(OpenUi::new::<GhostMenuUi>(
                    client,
                    ghost,
                    ghost,
                    "Ghost Menu",
                ));
            }
            HudAct::Spawners => {
                world.write_message(OpenUi::new::<SpawnersMenuUi>(
                    client,
                    ghost,
                    ghost,
                    "Spawners Menu",
                ));
            }
            HudAct::Reenter => {
                reenter_corpse(world, ghost);
            }
            HudAct::Respawn => {
                respawn(world, client, ghost);
            }
        }
    }
    for (client, ghost, source, act) in orbit_acts {
        if !mine(world, client, ghost, source) {
            continue;
        }
        match act {
            OrbitAct::Orbit { target, observe } => {
                orbit(world, ghost, target, observe);
            }
            OrbitAct::Refresh => {}
        }
    }
    for (client, ghost, source, act) in menu {
        if !mine(world, client, ghost, source) {
            continue;
        }
        match act {
            MenuAct::Dnr => {
                do_not_resuscitate(world, ghost);
            }
            MenuAct::Return => {
                reenter_corpse(world, ghost);
            }
            MenuAct::Darkness(level) => {
                set_darkness(world, ghost, &level);
            }
            MenuAct::ToggleVision => {
                toggle_vision(world, ghost);
            }
            MenuAct::ToggleHealth => {
                toggle_health_scan(world, ghost);
            }
            // `tray_view` -> `t_ray_scan(src)` on the next pulse.
            MenuAct::TRay => {
                if let Some(mut requests) =
                    world.get_resource_mut::<crate::t_scanner::TRayRequests>()
                {
                    requests.0.push(ghost);
                }
            }
            MenuAct::Boo => {
                boo(world, ghost);
            }
            MenuAct::Possess(target) => {
                possess(world, ghost, target);
            }
        }
    }
    for (client, ghost, source, act) in spawn {
        if !mine(world, client, ghost, source) {
            continue;
        }
        match act {
            SpawnerAct::Jump(spawner) => {
                let to = world
                    .get::<GhostRoleSpawner>(spawner)
                    .and(world.get::<TilePos>(spawner).copied());
                if let Some(to) = to {
                    world
                        .entity_mut(ghost)
                        .remove::<(Orbiting, Observing)>()
                        .insert(to);
                }
            }
            SpawnerAct::Spawn(spawner) => {
                take_role(world, ghost, spawner);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::ecs::system::RunSystemOnce;
    use ss13_shared::Facing;

    use super::*;
    use crate::server::MoveQueue;

    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<Players>();
        world.init_resource::<Time>();
        world.init_resource::<Messages<OpenUi>>();
        world.init_resource::<Messages<SendChat>>();
        world.insert_resource(RespawnConfig {
            mode: RespawnMode::Free,
            delay_secs: 0.0,
        });
        let mut index = SpatialIndex::default();
        index.create_level(1, 16, 16).unwrap();
        world.insert_resource(index);
        world.add_observer(setup_ghost);
        world
    }

    fn ghost(world: &mut World, at: TilePos) -> (Entity, Entity) {
        let client = world.spawn_empty().id();
        let ghost = crate::ghost::spawn_ghost(
            world,
            client,
            None,
            at,
            Facing::default(),
            Name::new("ghost"),
            MoveQueue::default(),
        );
        world.flush();
        (client, ghost)
    }

    fn mob(world: &mut World, at: TilePos, stat: Stat) -> Entity {
        world.spawn((Name::new("mob"), stat, at)).id()
    }

    #[test]
    fn an_orbiting_ghost_follows_its_target_and_observes_it() {
        let mut world = world();
        let (_, g) = ghost(&mut world, TilePos::new(1, 1, 1));
        assert!(world.get::<GhostSettings>(g).is_some(), "settings on spawn");
        let target = mob(&mut world, TilePos::new(5, 5, 1), Stat::Conscious);
        assert!(!orbit(&mut world, g, g, false), "not itself");
        assert!(orbit(&mut world, g, target, true));
        assert_eq!(world.get::<TilePos>(g), Some(&TilePos::new(5, 5, 1)));
        assert_eq!(world.get::<Observing>(g).map(|o| o.target), Some(target));
        world.entity_mut(target).insert(TilePos::new(6, 5, 1));
        world.run_system_once(follow_orbits).unwrap();
        world.flush();
        assert_eq!(world.get::<TilePos>(g), Some(&TilePos::new(6, 5, 1)));
        world.despawn(target);
        world.run_system_once(follow_orbits).unwrap();
        world.flush();
        assert!(world.get::<Orbiting>(g).is_none() && world.get::<Observing>(g).is_none());
    }

    #[test]
    fn darkness_and_ghost_vision_toggle() {
        let mut world = world();
        let (client, g) = ghost(&mut world, TilePos::new(1, 1, 1));
        assert!(set_darkness(&mut world, g, "Ночное зрение"));
        assert!(!set_darkness(&mut world, g, "Ночное зрение"), "already set");
        assert!(!set_darkness(&mut world, g, "nope"));
        assert_eq!(world.get::<GhostSettings>(g).unwrap().darkness, 2);
        assert_eq!(toggle_vision(&mut world, g), Some(false));
        assert_eq!(
            world.get::<SeeInvisible>(client),
            Some(&SeeInvisible(SEE_INVISIBLE_LIVING))
        );
        assert!(world.get::<VisibleToMobs>(g).is_some_and(|v| v.0 == [g]));
        assert_eq!(toggle_health_scan(&mut world, g), Some(true));
        assert_eq!(toggle_vision(&mut world, g), Some(true));
        assert_eq!(
            world.get::<SeeInvisible>(client),
            Some(&SeeInvisible(SEE_INVISIBLE_OBSERVER))
        );
    }

    #[test]
    fn possess_needs_fun_verbs_and_a_mindless_living_mob() {
        let mut world = world();
        let (client, g) = ghost(&mut world, TilePos::new(1, 1, 1));
        let dead = mob(&mut world, TilePos::new(2, 2, 1), Stat::Dead);
        let alive = mob(&mut world, TilePos::new(3, 3, 1), Stat::Conscious);
        assert!(!possess(&mut world, g, alive), "no fun verbs");
        world.entity_mut(g).insert(FunVerbs);
        assert!(!possess(&mut world, g, dead));
        assert!(possess(&mut world, g, alive));
        assert!(world.get_entity(g).is_err(), "the ghost is gone");
        assert_eq!(world.resource::<Players>().mob(client), Some(alive));
        assert_eq!(
            world.get::<PlayerMob>(alive).map(|p| p.client),
            Some(client)
        );
    }

    #[test]
    fn boo_is_refused_without_fun_verbs_or_a_light() {
        let mut world = world();
        let (_, g) = ghost(&mut world, TilePos::new(1, 1, 1));
        assert!(!boo(&mut world, g), "no fun verbs");
        world.entity_mut(g).insert(FunVerbs);
        assert!(!boo(&mut world, g), "no light near");
        assert_eq!(world.get::<GhostSettings>(g).unwrap().boo_ready_at, 0.0);
    }

    #[test]
    fn respawn_follows_the_config_and_its_delay() {
        let mut world = world();
        let (client, g) = ghost(&mut world, TilePos::new(1, 1, 1));
        world.insert_resource(RespawnConfig {
            mode: RespawnMode::Disabled,
            delay_secs: 0.0,
        });
        assert!(!respawn(&mut world, client, g));
        world.insert_resource(RespawnConfig {
            mode: RespawnMode::Free,
            delay_secs: 300.0,
        });
        assert!(!respawn(&mut world, client, g), "within the delay");
        assert!(world.get_entity(g).is_ok());
        assert_eq!(world.resource::<Players>().mob(client), Some(g));
        world.resource_mut::<RespawnConfig>().delay_secs = 0.0;
        // `character_name` keeps a name the client already has (no `Jobs` here).
        world
            .entity_mut(client)
            .insert(crate::jobs::CharacterName("Ghost".into()));
        assert!(respawn(&mut world, client, g));
        assert!(world.get_entity(g).is_err(), "the ghost is gone");
        assert_eq!(world.resource::<Players>().mob(client), None);
        assert!(world.get::<crate::round::NewPlayer>(client).is_some());
        assert!(world.get::<Viewer>(client).is_none());
        assert!(!respawn(&mut world, client, g), "only once");
    }

    #[test]
    fn spawners_group_by_role_and_skip_used_ones() {
        let mut world = World::new();
        let [a, b, c] = [(); 3].map(|()| world.spawn_empty().id());
        let role = |name: &str, uses| GhostRoleSpawner {
            name: name.into(),
            desc: String::new(),
            uses,
            def: None,
        };
        let (x, y, z) = (role("Hermit", 1), role("Hermit", 2), role("Ash walker", 0));
        let groups = spawner_groups([(a, &x), (b, &y), (c, &z)].into_iter());
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "Hermit");
        assert_eq!(groups[0].amount_left, 3);
        // The group jumps to one of its spawners.
        assert!([a.to_bits(), b.to_bits()].contains(&groups[0].key));
    }

    #[test]
    fn acts_parse_their_params() {
        let key = World::new().spawn_empty().id().to_bits().to_string();
        let p = [
            (ORBIT_REF.to_owned(), key.clone()),
            (ORBIT_AUTO_OBSERVE.to_owned(), "1".to_owned()),
        ];
        assert!(matches!(
            OrbitUi::parse_act(ORBIT, &UiParams::new(&p)),
            Ok(OrbitAct::Orbit { observe: true, .. })
        ));
        assert!(OrbitUi::parse_act(ORBIT, &UiParams::new(&[])).is_err());
        let p = [(MENU_TOGGLING.to_owned(), "nope".to_owned())];
        assert!(GhostMenuUi::parse_act(MENU_TOGGLE, &UiParams::new(&p)).is_err());
        assert!(GhostHudUi::parse_act("teleport", &UiParams::new(&[])).is_err());
    }
}
