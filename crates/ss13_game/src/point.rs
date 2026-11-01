//! Pointing (server): shift+middle click, tg `ShiftMiddleClickOn` -> `/mob/verb/pointed` ->
//! `_pointed` (`code/modules/point/point.dm`, `mob/living/living.dm`).
//!
//! A living mob that is not incapacitated (restraints aside) points at something it sees, at
//! most once a second (BandaStation's `pointed_cooldown`): tg's `/obj/effect/temp_visual/point`
//! arrow (`icons/hud/screen_gen.dmi` `arrow`, `POINT_PLANE`) shows over the target for
//! `POINT_TIME` (2.5 s) and everyone in view reads "<name> указывает на <target>.", the pointer
//! "Вы указываете на <target>.".
// shortcut: the arrow appears over the target at once (tg glides it there from the pointer's
// tile in 0.55 s) and the pointer does not turn to face it; restrained or handless carbons
// point like others (tg makes them `*point` emote); ghosts do not point (tg: a deadchat-only
// arrow); nothing points from inside a closet (tg's thought bubble).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_net::chat::server::SendChat;
use ss13_net::server::Networked;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::{Appearance, IconStateId, Layer, MouseOpacity, TilePos};

use crate::click::{PointClick, ResolveClicks};
use crate::health::{DEFAULT_MESSAGE_RANGE, Stat};
use crate::security::name_of;

/// `POINT_TIME`.
pub const POINT_SECS: f64 = 2.5;
/// BandaStation `TIMER_COOLDOWN_START(src, "pointed_cooldown", 1 SECONDS)`.
pub const POINT_COOLDOWN_SECS: f64 = 1.0;
/// `/obj/effect/temp_visual/point`: `icon`, `icon_state`, `plane = POINT_PLANE`.
pub const POINT_ICON: (&str, &str) = ("icons/hud/screen_gen.dmi", "arrow");
const POINT_PLANE: i16 = 6;
/// `/obj/effect/temp_visual` `layer = ABOVE_MOB_LAYER`.
const ABOVE_MOB_LAYER: f32 = 4.1;

/// A pointing arrow, gone at this time.
#[derive(Component, Clone, Copy, Debug)]
pub struct PointArrow {
    pub until: f64,
}

/// When the mob may point again.
#[derive(Component, Clone, Copy, Debug)]
pub struct PointCooldown(pub f64);

pub struct PointServerPlugin;

impl Plugin for PointServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                points.after(ResolveClicks),
                expire_arrows.run_if(any_with_component::<PointArrow>),
            ),
        );
    }
}

fn points(world: &mut World, mut clicks: Local<MessageCursor<PointClick>>) {
    let clicks: Vec<PointClick> = clicks
        .read(world.resource::<Messages<PointClick>>())
        .copied()
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for click in clicks {
        let (mob, target) = (click.mob, click.target);
        let Ok(entity) = world.get_entity(mob) else {
            continue;
        };
        // `/mob/living/pointed`: `INCAPACITATED_IGNORING(src, INCAPABLE_RESTRAINTS)`.
        if entity.get::<Stat>() != Some(&Stat::Conscious)
            || entity.contains::<crate::closets::InCloset>()
            || entity.get::<PointCooldown>().is_some_and(|c| now < c.0)
            || crate::status_effects::incapacitation(world, mob).incapacitated
            || world.get::<PointArrow>(target).is_some()
        {
            continue;
        }
        let (Some(&from), Some(&at)) = (entity.get::<TilePos>(), world.get::<TilePos>(target))
        else {
            continue;
        };
        if from.z != at.z {
            continue;
        }
        world
            .entity_mut(mob)
            .insert(PointCooldown(now + POINT_COOLDOWN_SECS));
        let offset = world
            .get::<Appearance>(target)
            .map_or([0, 0], |a| a.pixel_offset);
        spawn_arrow(world, at, offset, now);
        announce(world, click.client, mob, target, from);
    }
}

fn spawn_arrow(world: &mut World, at: TilePos, pixel_offset: [i16; 2], now: f64) {
    let icon = world
        .get_resource::<IconIndex>()
        .and_then(|icons| icons.get(POINT_ICON.0, POINT_ICON.1));
    world.spawn((
        Name::new("pointer"),
        at,
        Appearance {
            icon_state: icon.unwrap_or(IconStateId(u32::MAX)),
            layer: Layer(ABOVE_MOB_LAYER),
            plane: POINT_PLANE,
            color: None,
            pixel_offset,
            rotation: 0,
            overlays: vec![],
            // `pointed()` ignores the arrows; nothing clicks them.
            mouse_opacity: MouseOpacity::Transparent,
        },
        Networked,
        PointArrow {
            until: now + POINT_SECS,
        },
    ));
}

/// `visible_message(span_infoplain("[span_name(name)] указывает на [target]."), span_notice(...))`.
fn announce(world: &mut World, client: Entity, mob: Entity, target: Entity, from: TilePos) {
    let (name, what) = (name_of(world, mob), name_of(world, target));
    let others = crate::obj_damage::clients_in_range(world, from, DEFAULT_MESSAGE_RANGE, &[mob]);
    world.write_message(SendChat {
        clients: others,
        message: ChatMessage {
            channel: ChatChannel::Local,
            segments: vec![
                ChatSegment::new(Span::Name, name),
                ChatSegment::new(Span::Plain, format!(" указывает на {what}.")),
            ],
        },
    });
    world.write_message(SendChat {
        clients: vec![client],
        message: ChatMessage {
            channel: ChatChannel::Local,
            segments: vec![ChatSegment::new(
                Span::Notice,
                format!("Вы указываете на {what}."),
            )],
        },
    });
}

/// `temp_visual` `duration`: the arrow despawns.
fn expire_arrows(time: Res<Time>, arrows: Query<(Entity, &PointArrow)>, mut commands: Commands) {
    let now = time.elapsed_secs_f64();
    for (arrow, point) in &arrows {
        if now >= point.until {
            commands.entity(arrow).despawn();
        }
    }
}
