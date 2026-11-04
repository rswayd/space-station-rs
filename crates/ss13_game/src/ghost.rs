//! Ghosts (server), after tg `/mob/dead/observer` and `/client/Move`.
//!
//! A dead mob keeps its client until the player tries to move: then, as tg's `client/Move` calls
//! `ghostize()`, the client gets a ghost at the body. The ghost:
//! - is drawn as `icons/mob/simple/mob.dmi` `ghost` on `GHOST_PLANE`;
//! - has `INVISIBILITY_OBSERVER` and its client `SEE_INVISIBLE_OBSERVER`, so only ghosts are
//!   replicated it (living clients never learn it exists) and ghosts see each other;
//! - sees through walls (tg observer `sight`: `SEE_TURFS | SEE_MOBS | SEE_OBJS`), while a living
//!   mob's client gets only what BYOND `view()` shows it;
//! - moves one tile per tick in any direction, through walls (`abstract_move`), within the map;
//! - has no hands, no health and no clicks (it is not a living mob).
//!
//! The body stays where it died, without a client. A client that disconnects as a ghost loses the
//! ghost; its body stays. The windows the body had open close (tg `SStgui.on_logout`): a ghost
//! works none of its body's windows or buttons; back in the body, its HUD windows reopen.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_net::server::{ControlledEntity, Invisibility, Networked, SeeInvisible, Sight, Viewer};
use ss13_shared::protocol::TICK_DURATION;
use ss13_shared::ui::UiStatus;
use ss13_shared::{Appearance, Dir, Facing, IconStateId, Layer, TilePos};
use ss13_tgui::server::{OpenUi, UiSystems, UiWindow};
use ss13_world::{SpatialIndex, WorldSystems};

use crate::hud::{
    AlertHudData, AlertHudUi, HealthHudData, HealthHudUi, HudData, HungerHudData, HungerHudUi,
};
use crate::server::{MoveQueue, PlayerMob, Players};

/// `INVISIBILITY_OBSERVER` and `SEE_INVISIBLE_OBSERVER`.
pub const INVISIBILITY_OBSERVER: u8 = 60;
pub const SEE_INVISIBLE_OBSERVER: u8 = 60;
/// `GHOST_PLANE`.
pub const GHOST_PLANE: i16 = 5;
/// `/mob/dead/observer` `icon`, `icon_state`; `MOB_LAYER` (inherited).
pub const GHOST_ICON: (&str, &str) = ("icons/mob/simple/mob.dmi", "ghost");
const MOB_LAYER: f32 = 4.0;

/// On a ghost entity.
#[derive(Component, Clone, Copy, Debug)]
pub struct Ghost {
    /// The body it left; `None` for an observer from the lobby (tg `started_as_observer`).
    pub body: Option<Entity>,
}

/// The client's dead mob asked to move: it becomes a ghost (tg `ghostize`).
#[derive(Message, Clone, Copy, Debug)]
pub struct Ghostize {
    pub client: Entity,
}

/// One step of a ghost, already taken from its client's queue.
#[derive(Message, Clone, Copy, Debug)]
pub struct GhostStep {
    pub ghost: Entity,
    pub dir: Dir,
}

pub struct GhostServerPlugin;

impl Plugin for GhostServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<Ghostize>()
            .add_message::<GhostStep>()
            .add_systems(
                Update,
                (ghostize, move_ghosts)
                    .chain()
                    .in_set(WorldSystems::Movement),
            )
            .add_systems(Update, close_foreign_windows.in_set(UiSystems::StatusHook));
    }
}

fn ghostize(world: &mut World, mut requests: Local<MessageCursor<Ghostize>>) {
    let clients: Vec<Entity> = requests
        .read(world.resource::<Messages<Ghostize>>())
        .map(|g| g.client)
        .collect();
    for client in clients {
        ghostize_client(world, client);
    }
}

/// tg `ghostize(can_reenter_corpse = TRUE)`: the client's mob (dead, or alive for an admin
/// ghost) keeps its place without a client and the client gets a ghost at it. `None` when the
/// client controls no body (already a ghost, in the lobby).
pub fn ghostize_client(world: &mut World, client: Entity) -> Option<Entity> {
    let body = world.resource::<Players>().mob(client)?;
    let entity = world.get_entity(body).ok()?;
    // Checked again here: the request may be stale (already a ghost, another body).
    if entity.contains::<Ghost>() || entity.get::<PlayerMob>().map(|p| p.client) != Some(client) {
        return None;
    }
    let (Some(&pos), Some(queue)) = (entity.get::<TilePos>(), entity.get::<MoveQueue>()) else {
        return None;
    };
    let queue = queue.continued();
    let facing = entity.get::<Facing>().copied().unwrap_or_default();
    let name = entity
        .get::<Name>()
        .cloned()
        .unwrap_or_else(|| Name::new("призрак"));
    let ghost = spawn_ghost(world, client, Some(body), pos, facing, name, queue);
    // The body has no client any more: its HUD windows close with these.
    world
        .entity_mut(body)
        .remove::<(PlayerMob, MoveQueue, HudData, HealthHudData)>();
    info!("client {client} ghosted from {body} at {pos:?} as {ghost}");
    Some(ghost)
}

/// A ghost for `client` at `pos`, which the client now controls and sees through (its view box
/// moves there, it sees other ghosts).
pub(crate) fn spawn_ghost(
    world: &mut World,
    client: Entity,
    body: Option<Entity>,
    pos: TilePos,
    facing: Facing,
    name: Name,
    queue: MoveQueue,
) -> Entity {
    let icon = world
        .get_resource::<IconIndex>()
        .and_then(|icons| icons.get(GHOST_ICON.0, GHOST_ICON.1));
    if icon.is_none() {
        warn!("no {GHOST_ICON:?} icon state; the ghost is drawn as a missing state");
    }
    let ghost = world
        .spawn((
            Ghost { body },
            pos,
            facing,
            Appearance {
                icon_state: icon.unwrap_or(IconStateId(u32::MAX)),
                layer: Layer(MOB_LAYER),
                plane: GHOST_PLANE,
                color: None,
                pixel_offset: [0, 0],
                rotation: 0,
                mouse_opacity: Default::default(),
                overlays: vec![],
            },
            Invisibility(INVISIBILITY_OBSERVER),
            Networked,
            PlayerMob { client },
            queue,
            name,
            HealthHudData {
                health: None,
                combat_mode: false,
                move_delay: TICK_DURATION.as_secs_f32(),
                phasing: true,
                ..default()
            },
        ))
        .id();
    world.entity_mut(client).insert((
        ControlledEntity(ghost),
        SeeInvisible(SEE_INVISIBLE_OBSERVER),
        // tg observer `sight`: through walls.
        Sight::XRAY,
        Viewer::new(pos),
    ));
    world.resource_mut::<Players>().set_mob(client, ghost);
    world.write_message(OpenUi::new::<HealthHudUi>(client, ghost, ghost, ""));
    ghost
}

/// tg `grab_ghost` -> `reenter_corpse`: the ghost that left `body` goes back into it (its
/// client controls and sees through the body again). Nothing when nobody ghosted from it.
pub fn return_to_body(world: &mut World, body: Entity) {
    let Some((ghost, client)) = world
        .query::<(Entity, &Ghost, &PlayerMob)>()
        .iter(world)
        .find(|(_, g, _)| g.body == Some(body))
        .map(|(e, _, p)| (e, p.client))
    else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(body) else {
        return;
    };
    if world.get_entity(client).is_err() {
        return;
    }
    let queue = world
        .get::<MoveQueue>(ghost)
        .map(MoveQueue::continued)
        .unwrap_or_default();
    world.entity_mut(body).insert((
        PlayerMob { client },
        queue,
        HudData::default(),
        HealthHudData::default(),
    ));
    if let Some(mut inventory) = world.get_mut::<crate::inventory::Inventory>(body) {
        inventory.set_changed();
    }
    world
        .entity_mut(client)
        .remove::<(SeeInvisible, Sight)>()
        .insert((ControlledEntity(body), Viewer::new(pos)));
    world.resource_mut::<Players>().set_mob(client, body);
    world.despawn(ghost);
    reopen_body_huds(world, client, body);
    info!("client {client} re-entered {body}");
}

/// tgui `on_logout` / `on_transfer`: a window whose actor is no longer the client's mob (the body
/// it ghosted from) closes. Lobby windows act as the client itself.
// Cost: one pass over the open windows per tick, like tgui's default status.
fn close_foreign_windows(mut windows: Query<&mut UiWindow>, players: Res<Players>) {
    for mut window in &mut windows {
        if window.actor != window.client && players.mob(window.client) != Some(window.actor) {
            window.cap_status(UiStatus::Close);
        }
    }
}

/// The body's HUD windows that closed with the ghostize and do not reopen by themselves (opened
/// once, or only when their data changes); the inventory is marked changed above and the health
/// HUD follows its data.
fn reopen_body_huds(world: &mut World, client: Entity, body: Entity) {
    let Ok(e) = world.get_entity(body) else {
        return;
    };
    let mut open = Vec::new();
    if e.contains::<crate::actions::ActionButtons>() {
        open.push(OpenUi::new::<crate::actions::ActionButtonsUi>(
            client, body, body, "",
        ));
    }
    if e.contains::<crate::antag::AntagHudData>() {
        open.push(OpenUi::new::<crate::antag::AntagHudUi>(
            client, body, body, "",
        ));
    }
    if e.contains::<crate::antag::vampire::VampireHudData>() {
        open.push(OpenUi::new::<crate::antag::vampire::VampireHudUi>(
            client, body, body, "",
        ));
    }
    if e.contains::<crate::antag::changeling::ChangelingHudData>() {
        open.push(OpenUi::new::<crate::antag::changeling::ChangelingHudUi>(
            client, body, body, "",
        ));
    }
    if e.contains::<crate::records::DataHudData>() {
        open.push(OpenUi::new::<crate::records::DataHudUi>(
            client, body, body, "",
        ));
    }
    if e.contains::<AlertHudData>() {
        open.push(OpenUi::new::<AlertHudUi>(client, body, body, ""));
    }
    if e.contains::<HungerHudData>() {
        open.push(OpenUi::new::<HungerHudUi>(client, body, body, ""));
    }
    if e.contains::<crate::hud::FullscreenHudData>() {
        open.push(OpenUi::new::<crate::hud::FullscreenHudUi>(
            client, body, body, "",
        ));
    }
    for message in open {
        world.write_message(message);
    }
}

/// tg `/mob/dead/observer/Move` -> `abstract_move`: straight to the next tile, density ignored,
/// diagonals in one step, never off the map.
fn move_ghosts(
    mut steps: MessageReader<GhostStep>,
    index: Res<SpatialIndex>,
    mut ghosts: Query<(&TilePos, &mut Facing), With<Ghost>>,
    mut commands: Commands,
) {
    for &GhostStep { ghost, dir } in steps.read() {
        let Ok((&pos, mut facing)) = ghosts.get_mut(ghost) else {
            continue;
        };
        facing.set_if_neq(Facing(dir));
        let to = pos.step(dir);
        if index.in_bounds(to) {
            commands.entity(ghost).insert(to);
        }
    }
}
