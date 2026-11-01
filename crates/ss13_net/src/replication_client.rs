//! Client side of replication: applies server batches to local entities.
//!
//! Every replicated entity is a plain local entity with [`NetEntity`], `TilePos` and, when the
//! server has them, `Facing`, `Appearance` and `Optics` — drawable as is. Server `Entity` values never
//! reach the client; [`NetEntityMap`] resolves the [`NetId`] used in intents such as
//! `ClientIntent::Click`. All replicated entities are despawned when the connection is lost.
//!
//! Light blocks are mirrored in [`LightBlocks`] and passed on as `LightUpdate` messages for the
//! client lighting.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use ss13_shared::protocol::NetId;
use ss13_shared::{
    Appearance, Facing, LIGHT_BLOCK, LightSource, LightUpdate, Optics, TilePos, light_block,
};

use crate::client::NetClientState;
use crate::replication::{
    AppearanceId, BlockLights, Look, MotionBatch, ReplicationBatch, ReplicationOp, Spawned,
};

/// The server id of a replicated entity.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NetEntity(pub NetId);

/// On the entity this client controls (its mob).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct LocalControl;

/// [`NetId`] -> local entity for everything currently replicated.
#[derive(Resource, Default)]
pub struct NetEntityMap {
    entities: HashMap<NetId, Replicated>,
    controlled: Option<Entity>,
    /// Appearance cache mirrored from the server (rule on `AppearanceId`): value and the number
    /// of replicated entities using it.
    appearances: HashMap<AppearanceId, (Look, u32)>,
    unused: Vec<AppearanceId>,
}

struct Replicated {
    entity: Entity,
    /// Server tick of the state the entity shows.
    tick: u32,
    appearance: Option<AppearanceId>,
}

impl NetEntityMap {
    pub fn get(&self, id: NetId) -> Option<Entity> {
        self.entities.get(&id).map(|r| r.entity)
    }

    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    /// The local entity with [`LocalControl`].
    pub fn controlled(&self) -> Option<Entity> {
        self.controlled
    }

    /// Number of cached appearance values.
    pub fn cached_appearances(&self) -> usize {
        self.appearances.len()
    }

    /// The cached value of `id`, counted as used by one more entity.
    fn use_appearance(&mut self, id: Option<AppearanceId>) -> Option<Look> {
        let (appearance, count) = self.appearances.get_mut(&id?).or_else(|| {
            warn!("server used appearance {id:?} without sending it");
            None
        })?;
        *count += 1;
        Some(appearance.clone())
    }

    fn drop_appearance(&mut self, id: Option<AppearanceId>) {
        if let Some(id) = id
            && let Some((_, count)) = self.appearances.get_mut(&id)
        {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.unused.push(id);
            }
        }
    }

    fn remove(&mut self, id: NetId, commands: &mut Commands) {
        if let Some(old) = self.entities.remove(&id) {
            commands.entity(old.entity).try_despawn();
            if self.controlled == Some(old.entity) {
                self.controlled = None;
            }
            self.drop_appearance(old.appearance);
        }
    }
}

/// The light blocks the server streams to this client: opacity and light sources around the
/// view, wider than the replicated entities.
#[derive(Resource, Default)]
pub struct LightBlocks {
    blocks: HashMap<TilePos, KnownBlock>,
}

struct KnownBlock {
    opaque: [u8; 32],
    lights: HashMap<u8, Vec<LightSource>>,
}

impl LightBlocks {
    /// Number of known blocks.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Number of known light sources.
    pub fn sources(&self) -> usize {
        (self.blocks.values())
            .flat_map(|b| b.lights.values())
            .map(Vec::len)
            .sum()
    }

    /// Whether `pos` blocks light; `None` when its block is not known.
    pub fn opaque(&self, pos: TilePos) -> Option<bool> {
        let (origin, i) = light_block(pos);
        let block = self.blocks.get(&origin)?;
        Some(block.opaque[i / 8] & (1 << (i % 8)) != 0)
    }

    /// The light sources on `pos`; `None` when its block is not known.
    pub fn lights(&self, pos: TilePos) -> Option<&[LightSource]> {
        let (origin, i) = light_block(pos);
        let block = self.blocks.get(&origin)?;
        Some(block.lights.get(&(i as u8)).map_or(&[], Vec::as_slice))
    }

    fn forget(&mut self, origin: TilePos, out: &mut MessageWriter<LightUpdate>) {
        if self.blocks.remove(&origin).is_some() {
            out.write(LightUpdate::Forget { origin });
        }
    }

    /// Applies one light op. Ops for blocks that are not aligned, or not known when they must
    /// be, are dropped: the server sends a block's state before its changes.
    fn apply(&mut self, op: ReplicationOp, out: &mut MessageWriter<LightUpdate>) {
        let aligned = |origin: TilePos| light_block(origin).0 == origin;
        match op {
            ReplicationOp::LightBlock {
                origin,
                opaque,
                lights,
            } if aligned(origin) => {
                self.forget(origin, out);
                self.blocks.insert(
                    origin,
                    KnownBlock {
                        opaque,
                        lights: HashMap::default(),
                    },
                );
                out.write(LightUpdate::Opacity {
                    origin,
                    bits: opaque,
                });
                self.set_lights(origin, lights, out);
            }
            ReplicationOp::Opacity { origin, opaque } => {
                if let Some(block) = self.blocks.get_mut(&origin) {
                    block.opaque = opaque;
                    out.write(LightUpdate::Opacity {
                        origin,
                        bits: opaque,
                    });
                }
            }
            ReplicationOp::Lights { origin, lights } => self.set_lights(origin, lights, out),
            ReplicationOp::ForgetLight(origins) => {
                for origin in origins {
                    self.forget(origin, out);
                }
            }
            _ => {}
        }
    }

    fn set_lights(
        &mut self,
        origin: TilePos,
        BlockLights { palette, tiles }: BlockLights,
        out: &mut MessageWriter<LightUpdate>,
    ) {
        let Some(block) = self.blocks.get_mut(&origin) else {
            return;
        };
        for (tile, indexes) in tiles {
            let lights: Vec<LightSource> = (indexes.iter())
                .filter_map(|&i| palette.get(usize::from(i)).copied())
                .collect();
            let pos = TilePos::new(
                origin.x + i32::from(tile) % LIGHT_BLOCK,
                origin.y + i32::from(tile) / LIGHT_BLOCK,
                origin.z,
            );
            if lights.is_empty() {
                block.lights.remove(&tile);
            } else {
                block.lights.insert(tile, lights.clone());
            }
            out.write(LightUpdate::Lights { pos, lights });
        }
    }
}

/// Batches received this frame, in arrival order per kind; filled by the client's receive system.
#[derive(Resource, Default)]
pub(crate) struct ReplicationInbox {
    pub batches: Vec<ReplicationBatch>,
    pub motion: Vec<MotionBatch>,
}

pub(crate) fn receive_replication(
    mut inbox: ResMut<ReplicationInbox>,
    mut map: ResMut<NetEntityMap>,
    mut light: ResMut<LightBlocks>,
    mut light_out: MessageWriter<LightUpdate>,
    mut commands: Commands,
) {
    // Reliable first: a motion of the same frame must not be dropped as "unknown entity".
    for batch in inbox.batches.drain(..) {
        for (id, appearance) in batch.appearances {
            let count = map.appearances.get(&id).map_or(0, |(_, count)| *count);
            map.appearances.insert(id, (appearance, count));
        }
        for op in batch.ops {
            if op.is_light() {
                light.apply(op, &mut light_out);
            } else {
                apply_op(&mut map, &mut commands, batch.tick, op);
            }
        }
        let map = &mut *map;
        for id in map.unused.drain(..) {
            if map
                .appearances
                .get(&id)
                .is_some_and(|(_, count)| *count == 0)
            {
                map.appearances.remove(&id);
            }
        }
    }
    for batch in inbox.motion.drain(..) {
        for m in batch.moves {
            let Some(replicated) = map.entities.get_mut(&m.id) else {
                continue; // Spawn still in flight; its settle or next motion repairs it.
            };
            if batch.tick <= replicated.tick {
                continue;
            }
            replicated.tick = batch.tick;
            set_motion(&mut commands, replicated.entity, m.pos, m.facing);
        }
    }
}

fn apply_op(map: &mut NetEntityMap, commands: &mut Commands, tick: u32, op: ReplicationOp) {
    match op {
        ReplicationOp::Spawn { pos, entities } => {
            for Spawned {
                id,
                facing,
                appearance,
            } in entities
            {
                map.remove(id, commands);
                let mut spawned = commands.spawn((NetEntity(id), pos));
                if let Some(dir) = facing {
                    spawned.insert(Facing(dir));
                }
                if let Some(Look { appearance, optics }) = map.use_appearance(appearance) {
                    spawned.insert(appearance);
                    if let Some(optics) = optics {
                        spawned.insert(optics);
                    }
                }
                let entity = spawned.id();
                map.entities.insert(
                    id,
                    Replicated {
                        entity,
                        tick,
                        appearance,
                    },
                );
            }
        }
        ReplicationOp::Despawn(ids) => {
            for id in ids {
                map.remove(id, commands);
            }
        }
        ReplicationOp::Appearance(id, appearance) => {
            let Some(old) = map.entities.get(&id).map(|r| r.appearance) else {
                return;
            };
            let value = map.use_appearance(appearance);
            map.drop_appearance(old);
            let replicated = map.entities.get_mut(&id).expect("checked above");
            replicated.appearance = appearance;
            let mut entity = commands.entity(replicated.entity);
            match value {
                Some(Look {
                    appearance,
                    optics: Some(optics),
                }) => entity.try_insert((appearance, optics)),
                Some(Look {
                    appearance,
                    optics: None,
                }) => entity.try_insert(appearance).try_remove::<Optics>(),
                None => entity.try_remove::<(Appearance, Optics)>(),
            };
        }
        ReplicationOp::Settle { id, pos, facing } => {
            if let Some(replicated) = map.entities.get_mut(&id)
                && tick >= replicated.tick
            {
                replicated.tick = tick;
                set_motion(commands, replicated.entity, pos, facing);
            }
        }
        ReplicationOp::Control(id) => {
            if let Some(old) = map.controlled.take() {
                commands.entity(old).try_remove::<LocalControl>();
            }
            if let Some(entity) = id.and_then(|id| map.get(id)) {
                commands.entity(entity).try_insert(LocalControl);
                map.controlled = Some(entity);
            }
        }
        ReplicationOp::LightBlock { .. }
        | ReplicationOp::Opacity { .. }
        | ReplicationOp::Lights { .. }
        | ReplicationOp::ForgetLight(_) => {}
    }
}

fn set_motion(
    commands: &mut Commands,
    entity: Entity,
    pos: TilePos,
    facing: Option<ss13_shared::Dir>,
) {
    let mut entity = commands.entity(entity);
    entity.try_insert(pos);
    match facing {
        Some(dir) => entity.try_insert(Facing(dir)),
        None => entity.try_remove::<Facing>(),
    };
}

/// A new connection starts from an empty world.
pub(crate) fn clear_on_disconnect(
    state: Res<NetClientState>,
    mut map: ResMut<NetEntityMap>,
    mut light: ResMut<LightBlocks>,
    mut light_out: MessageWriter<LightUpdate>,
    mut commands: Commands,
) {
    if !state.is_changed() || !matches!(*state, NetClientState::Disconnected { .. }) {
        return;
    }
    let origins: Vec<TilePos> = light.blocks.keys().copied().collect();
    for origin in origins {
        light.forget(origin, &mut light_out);
    }
    if map.entities.is_empty() && map.appearances.is_empty() {
        return;
    }
    for (_, replicated) in map.entities.drain() {
        commands.entity(replicated.entity).try_despawn();
    }
    map.controlled = None;
    map.appearances.clear();
    map.unused.clear();
}
