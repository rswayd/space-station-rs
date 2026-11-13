//! Shooting range targets (tg `/obj/item/target`, `code/game/objects/items/shooting_range.dm`):
//! a target shot at is hit on its tile though it is not dense (`CAN_BE_HIT`,
//! `crate::projectile`); a hostile brute or burn projectile (or a practice laser) leaves a dent
//! or a scorch where it was aimed, three times in four, up to 35 marks. A lit welder clears
//! them; the clown target honks when hit.
// shortcut: the hit point is not checked against the target's icon pixels (`GetPixel`): the
// server has no icon masks, and a client click only picks the target on an opaque pixel of it
// (`mouse_opacity`), so an aimed shot is on the sprite. No target stakes.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, Overlay, TilePos};

use crate::combat::CombatRng;
use crate::projectile::{AimPixel, Projectile};
use crate::sound::{PlaySound, playsound};
use crate::tools::{ToolKind, tell, tool_of, tool_usable, unusable_message};

pub const TARGET: &str = "/obj/item/target";
const CLOWN_TARGET: &str = "/obj/item/target/clown";
const EFFECTS: &str = "icons/effects/effects.dmi";
const BIKEHORN: &str = "sound/items/bikehorn.ogg";
/// `length(overlays) > 35`: no more marks.
const MAX_MARKS: usize = 35;
/// `always_leave_marks`.
const PRACTICE: [&str; 2] = [
    "/obj/projectile/beam/practice",
    "/obj/projectile/beam/laser/rapid/practice",
];

/// `bullethole_overlays`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct BulletHoles(pub Vec<Overlay>);

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // `target/welder_act`.
    app.add_click_handler(ClickPhase::HeldOnTarget, 4257, |w, c| {
        c.held.is_some_and(|h| welder_act(w, c.client, h, c.target))
    });
}

pub fn is_target(world: &World, e: Entity) -> bool {
    world
        .get::<DmItem>(e)
        .is_some_and(|i| is_under(&i.path, TARGET))
}

/// `target/bullet_act` after the hit (`..()` damaged it already): the mark of projectile `e`
/// aimed at `target`, then the clown target's honk.
pub fn bullet_act(world: &mut World, e: Entity, p: &Projectile, target: Entity) {
    if !is_target(world, target) || p.original != Some(target) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    let clown = world
        .get::<DmItem>(target)
        .is_some_and(|i| is_under(&i.path, CLOWN_TARGET));
    let pixel = world.get::<AimPixel>(e).copied().unwrap_or_default();
    add_mark(world, p, target, pixel.0);
    if clown {
        playsound(world, PlaySound::new(pos, BIKEHORN, 50.0, true));
    }
}

/// The dent or scorch of `p`, aimed `pixel` from the tile's center.
fn add_mark(world: &mut World, p: &Projectile, target: Entity, pixel: [i16; 2]) {
    let (skip, jx, jy, scorch_dir) = {
        let mut rng = world.resource_mut::<CombatRng>();
        // RNG chance to just not leave a mark, like walls.
        let skip = rng.chance(25.0);
        // `pick(0, 0, 0, 0, 0, -1, 1)`.
        let mut jitter = || match rng.range(0, 6) {
            5 => -1,
            6 => 1,
            _ => 0,
        };
        let (jx, jy) = (jitter(), jitter());
        (skip, jx, jy, Dir::CARDINALS[rng.range(0, 3) as usize])
    };
    if skip {
        return;
    }
    let holes = world.get::<BulletHoles>(target).map_or(0, |h| h.0.len());
    if holes > MAX_MARKS {
        return;
    }
    let data = &p.data;
    let brute = data.damage_type == "brute";
    let valid_damage = brute || data.damage_type == "burn";
    let hostile = data.damage > 0.0
        || data.stamina > 0.0
        || data.paralyze + data.stun + data.immobilize + data.knockdown > 0.0;
    let generic = !PRACTICE.iter().any(|path| is_under(&data.path, path));
    if generic && (!valid_damage || !hostile) {
        return;
    }
    let state = if brute {
        "dent"
    } else if data.damage < 20.0 && generic {
        "light_scorch"
    } else {
        "scorch"
    };
    let Some(id) = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(EFFECTS, state))
    else {
        return;
    };
    // `p_x`/`p_y` are 1-based icon pixels, 16 at the center; the image sits at `p - 1`.
    let mut hole = Overlay::new(id);
    hole.pixel_offset = [pixel[0] + 16 + jx - 1, pixel[1] + 16 + jy - 1];
    if !brute {
        // A random scorch design.
        hole.dir = Some(scorch_dir);
    }
    match world.get_mut::<BulletHoles>(target) {
        Some(mut h) => h.0.push(hole.clone()),
        None => {
            world
                .entity_mut(target)
                .insert(BulletHoles(vec![hole.clone()]));
        }
    }
    if let Some(mut look) = world.get_mut::<Appearance>(target) {
        look.overlays.push(hole);
    }
}

/// `target/welder_act`: a usable welder clears the marks. True when it was a welder on a
/// target.
fn welder_act(world: &mut World, client: Entity, held: Entity, target: Entity) -> bool {
    if !is_target(world, target) || tool_of(world, held).map(|(k, _)| k) != Some(ToolKind::Welder) {
        return false;
    }
    if !tool_usable(world, held, 0, 0.0) {
        let text = unusable_message(world, held, 0);
        tell(world, client, Span::Warning, text);
        return true;
    }
    let holes = world
        .entity_mut(target)
        .take::<BulletHoles>()
        .unwrap_or_default();
    if let Some(mut look) = world.get_mut::<Appearance>(target) {
        look.overlays.retain(|o| !holes.0.contains(o));
    }
    tell(world, client, Span::Notice, "мишень отремонтирована");
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;
    use ss13_mapload::ProjectileData;

    fn shot(
        world: &mut World,
        damage_type: &str,
        damage: f32,
        target: Entity,
    ) -> (Entity, Projectile) {
        let p = Projectile {
            data: ProjectileData {
                path: "/obj/projectile/bullet".into(),
                damage,
                damage_type: damage_type.into(),
                ..default()
            },
            firer: Entity::PLACEHOLDER,
            original: Some(target),
            zone: crate::zone::BodyZone::Chest,
            hit_prone: false,
            start: TilePos::new(1, 1, 1),
            x: 1.0,
            y: 1.0,
            dx: 0.0,
            dy: 1.0,
            moved: 0.0,
            angle: 0.0,
            fired_from: None,
            ricochets: 0,
        };
        let e = world.spawn(AimPixel([4, -2])).id();
        (e, p)
    }

    #[test]
    fn hostile_hits_leave_marks_and_harmless_ones_do_not() {
        let mut world = World::new();
        world.insert_resource(CombatRng(7));
        let mut icons = IconIndex::default();
        for (n, state) in ["dent", "scorch", "light_scorch"].into_iter().enumerate() {
            icons.0.insert(
                (EFFECTS.into(), state.into()),
                ss13_shared::IconStateId(n as u32),
            );
        }
        world.insert_resource(icons);
        world.init_resource::<Messages<PlaySound>>();
        let target = world
            .spawn((
                DmItem {
                    path: TARGET.into(),
                    ..default()
                },
                TilePos::new(1, 3, 1),
                Appearance {
                    icon_state: ss13_shared::IconStateId(9),
                    layer: ss13_shared::Layer(3.0),
                    plane: -4,
                    color: None,
                    pixel_offset: [0, 0],
                    rotation: 0,
                    mouse_opacity: Default::default(),
                    glide: 0,
                    overlays: Vec::new(),
                },
            ))
            .id();
        for _ in 0..40 {
            let (e, p) = shot(&mut world, "brute", 20.0, target);
            bullet_act(&mut world, e, &p, target);
        }
        let holes = world.get::<BulletHoles>(target).unwrap().0.clone();
        assert!(
            holes.len() > 15 && holes.len() <= MAX_MARKS + 1,
            "{}",
            holes.len()
        );
        assert!(
            holes
                .iter()
                .all(|h| h.icon_state == ss13_shared::IconStateId(0))
        );
        // Aimed 4 px right and 2 down of the center: the dent sits there, give or take a pixel.
        assert!(holes.iter().all(|h| (h.pixel_offset[0] - 19).abs() <= 1
            && (h.pixel_offset[1] - 13).abs() <= 1));
        assert_eq!(world.get::<Appearance>(target).unwrap().overlays, holes);

        // A harmless or toxin shot leaves nothing on a fresh target.
        world.entity_mut(target).remove::<BulletHoles>();
        for (kind, damage) in [("brute", 0.0), ("toxin", 30.0)] {
            for _ in 0..10 {
                let (e, p) = shot(&mut world, kind, damage, target);
                bullet_act(&mut world, e, &p, target);
            }
        }
        assert!(world.get::<BulletHoles>(target).is_none());
        // A weak laser scorches lightly.
        for _ in 0..10 {
            let (e, p) = shot(&mut world, "burn", 10.0, target);
            bullet_act(&mut world, e, &p, target);
        }
        let holes = &world.get::<BulletHoles>(target).unwrap().0;
        assert!(!holes.is_empty());
        assert!(
            holes
                .iter()
                .all(|h| h.icon_state == ss13_shared::IconStateId(2) && h.dir.is_some())
        );
    }
}
