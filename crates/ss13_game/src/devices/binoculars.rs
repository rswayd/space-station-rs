//! Binoculars (server), after tg `/obj/item/binoculars` (`code/game/objects/items/binoculars.dm`)
//! with `/datum/component/scope` (`ZOOM_METHOD_WIELD`, `range_modifier = 4`).
//!
//! - Two-handed (`crate::weapons::two_handed`, force 8/12). Wielding holds them up to the eyes
//!   (`binoculars_wielded` in hand, `/datum/movespeed_modifier/binocs_wielded`: +1.5 ds a step)
//!   and zooms the view out ahead of the user; letting go (in hand again, dropping, the other
//!   hand) lowers them and zooms back.
//! - The zoom is [`crate::gun_attachments::send_scope`]'s: the view moves towards where the user
//!   faces, and turning re-aims it. Walking keeps it (tg ends a held scope only when the item
//!   leaves the hands or the user enters something).
// shortcut: the view follows the facing, not the cursor (tg `cursor_catcher` follows the mouse
// every tick): the client sends no cursor position between clicks. The aim is a cursor halfway
// to the view edge. No scope fullscreen overlay; the zoom sound plays as a normal sound.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::chat::Span;
use ss13_shared::{Dir, Facing, TilePos};

use crate::health::MoveSpeedModifier;
use crate::weapons::two_handed::Wielded;

pub const BINOCULARS: &str = "/obj/item/binoculars";
/// `AddComponent(/datum/component/scope, range_modifier = 4, ...)`.
const RANGE_MODIFIER: f32 = 4.0;
/// `/datum/movespeed_modifier/binocs_wielded` `multiplicative_slowdown`, deciseconds.
const SLOWDOWN_DS: f32 = 1.5;
const SCOPE_SOUND: &str = "sound/items/weapons/scope.ogg";
/// Half of tg's 21x15 view in pixels, halved again: where the shortcut's cursor rests.
const AIM_PX: [f32; 2] = [21.0 * 16.0 / 2.0, 15.0 * 16.0 / 2.0];

/// On a mob looking through `item` towards `facing` (tg `TRAIT_USER_SCOPED`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct BinocularZoom {
    pub item: Entity,
    pub facing: Dir,
}

/// On wielded binoculars: the mob their slowdown went to, so it comes off exactly once.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct BinocsSlowed(pub Entity);

pub(super) fn build(app: &mut App) {
    app.add_systems(
        Update,
        follow_facing.after(crate::inventory::inventory_intents),
    );
}

fn is_binoculars(world: &World, item: Entity) -> bool {
    world
        .get::<DmItem>(item)
        .is_some_and(|d| is_under(&d.path, BINOCULARS))
}

/// The zoom offset for a user facing `dir`.
pub fn aim(dir: Dir) -> [i16; 2] {
    let (dx, dy) = dir.offset();
    crate::gun_attachments::scope_offset(
        RANGE_MODIFIER,
        [dx as f32 * AIM_PX[0], dy as f32 * AIM_PX[1]],
    )
}

/// `on_wield`/`on_unwield` with the scope's `on_wielded`/`on_unwielded`, from
/// `crate::weapons::two_handed`.
pub fn on_wield(world: &mut World, mob: Entity, item: Entity, wielded: bool) {
    if !is_binoculars(world, item) {
        return;
    }
    let name = crate::security::name_of(world, item);
    let user = crate::security::name_of(world, mob);
    let state = if wielded {
        "binoculars_wielded"
    } else {
        "binoculars"
    };
    if let Some(mut dm) = world.get_mut::<DmItem>(item) {
        dm.inhand_icon_state = state.to_owned();
    }
    if wielded {
        crate::medical::visible(
            world,
            mob,
            &format!("Вы подносите {name} к глазам."),
            &format!("{user} подносит {name} к глазам."),
            Span::Notice,
        );
        if world.get::<BinocsSlowed>(item).is_none() {
            add_slowdown(world, mob, SLOWDOWN_DS);
            world.entity_mut(item).insert(BinocsSlowed(mob));
        }
        zoom(world, mob, item);
    } else {
        crate::medical::visible(
            world,
            mob,
            &format!("Вы опускаете {name}."),
            &format!("{user} опускает {name}."),
            Span::Notice,
        );
        if let Some(BinocsSlowed(slowed)) = world.get::<BinocsSlowed>(item).copied() {
            world.entity_mut(item).remove::<BinocsSlowed>();
            add_slowdown(world, slowed, -SLOWDOWN_DS);
        }
        stop_zooming(world, mob);
    }
    crate::inventory::refresh_look(world, mob);
}

fn add_slowdown(world: &mut World, mob: Entity, delta: f32) {
    let Ok(mut e) = world.get_entity_mut(mob) else {
        return;
    };
    let current = e.get::<MoveSpeedModifier>().map_or(0.0, |m| m.0);
    e.insert(MoveSpeedModifier(current + delta));
}

/// The scope's `zoom`: refused while already zoomed (a scoped gun) or without a client.
fn zoom(world: &mut World, mob: Entity, item: Entity) {
    let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) else {
        return;
    };
    if world.get::<crate::gun_attachments::Zoomed>(mob).is_some()
        || world.get::<BinocularZoom>(mob).is_some()
    {
        crate::balloon::balloon_alert(world, mob, mob, "уже приближено!");
        return;
    }
    let facing = world.get::<Facing>(mob).map_or(Dir::South, |f| f.0);
    world.entity_mut(mob).insert(BinocularZoom { item, facing });
    play(world, client, mob, 1.0);
    crate::gun_attachments::send_scope(world, mob, aim(facing));
}

/// The scope's `stop_zooming`.
pub fn stop_zooming(world: &mut World, mob: Entity) {
    if world.get::<BinocularZoom>(mob).is_none() {
        return;
    }
    world.entity_mut(mob).remove::<BinocularZoom>();
    if let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) {
        // `frequency = -1`: the zoom sound backwards.
        play(world, client, mob, -1.0);
    }
    crate::gun_attachments::send_scope(world, mob, [0, 0]);
}

fn play(world: &mut World, client: Entity, mob: Entity, speed: f32) {
    use ss13_net::sound::server::SendSound;
    if world.get::<TilePos>(mob).is_none()
        || !world.contains_resource::<bevy::ecs::message::Messages<SendSound>>()
    {
        return;
    }
    world.write_message(SendSound {
        client,
        sound: ss13_shared::protocol::SoundEvent {
            file: SCOPE_SOUND.into(),
            volume: 75.0,
            speed,
            offset: None,
        },
    });
}

/// The scope's `process` (`face_atom` the other way round: the view follows the facing) and its
/// end when the binoculars are no longer wielded by the user, or the user left its turf for
/// something else (`on_enter_new_loc`). Cost: one pass over the zoomed mobs per tick.
fn follow_facing(world: &mut World) {
    let mut q = world.query::<(Entity, &BinocularZoom)>();
    let zoomed: Vec<(Entity, BinocularZoom)> = q.iter(world).map(|(e, z)| (e, *z)).collect();
    for (mob, zoom) in zoomed {
        let wielded = world
            .get::<Wielded>(zoom.item)
            .is_some_and(|w| w.mob == mob);
        if !wielded || world.get::<TilePos>(mob).is_none() {
            stop_zooming(world, mob);
            continue;
        }
        let facing = world.get::<Facing>(mob).map_or(zoom.facing, |f| f.0);
        if facing != zoom.facing {
            world
                .entity_mut(mob)
                .insert(BinocularZoom { facing, ..zoom });
            crate::gun_attachments::send_scope(world, mob, aim(facing));
        }
    }
}

/// The scope's `on_examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    if is_binoculars(world, target) {
        lines.push((
            Span::Notice,
            "Можно посмотреть вдаль, взяв двумя руками.".to_owned(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// East: 4 × 168 px × 1.1 = 739 px, 23 tiles ahead; north uses the shorter half-view.
    #[test]
    fn the_aim_looks_ahead_of_the_facing() {
        assert_eq!(aim(Dir::East), [739, 0]);
        assert_eq!(aim(Dir::North), [0, 528]);
        assert_eq!(aim(Dir::SouthWest), [-739, -528]);
    }

    /// Wielding slows by 1.5 ds and holds them up; letting go takes exactly that back.
    #[test]
    fn wielding_slows_and_lowering_restores() {
        let mut world = World::new();
        world.init_resource::<bevy::ecs::message::Messages<ss13_net::chat::server::SendChat>>();
        let item = world
            .spawn(DmItem {
                path: BINOCULARS.into(),
                ..default()
            })
            .id();
        let mob = world.spawn(MoveSpeedModifier(1.0)).id();
        on_wield(&mut world, mob, item, true);
        assert_eq!(world.get::<MoveSpeedModifier>(mob).unwrap().0, 2.5);
        assert_eq!(
            world.get::<DmItem>(item).unwrap().inhand_icon_state,
            "binoculars_wielded"
        );
        on_wield(&mut world, mob, item, false);
        assert_eq!(world.get::<MoveSpeedModifier>(mob).unwrap().0, 1.0);
        assert!(world.get::<BinocsSlowed>(item).is_none());
    }
}
