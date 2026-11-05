//! Chairs and beds (server), after tg `code/game/objects/structures/beds_chairs/chair.dm`,
//! `bed.dm` and `code/game/objects/buckling.dm`. Buckling itself (dragging a mob onto the
//! furniture, the 2 s for someone else, an empty hand or resisting to get up, the buckled mob
//! held in place) is `crate::medical::surgery` (`Buckled`), shared with operating tables; this
//! module adds what chairs and beds do on top:
//!
//! - A chair seats its mob upright ([`Seated`]): the mob faces the chair's way and cannot turn
//!   (tg `face_atom` returns while `buckled`), a chair facing north is drawn above it
//!   (`handle_layer`), a stool lifts it 4 px, a bar stool 7 (`add_offsets`).
//! - Alt-click turns a chair counterclockwise with whoever sits on it (`simple_rotation`,
//!   anchored or not); a pew needs a wrench in the active hand.
//! - A wrench (right click) takes a chair or a bed apart into its `buildstacktype` /
//!   `build_stack_type` (`wrench_act_secondary`).
//! - A chair with an `item_chair` dragged onto its user is picked up as that item
//!   (`mouse_drop_dragged`); the item used in hand sets the chair down facing the user's way
//!   (`/obj/item/chair/plant`).
//! - Beds draw mobs on them `elevation` pixels up (`/datum/element/elevation`, through
//!   `crate::tables::Elevating`).
//! - The furniture's `buckle_sound` / `unbuckle_sound` (shuttle seats, medical beds).
// shortcut: no armrest overlays (`[icon_state]_armrest` above the mob: overlays have no layer of
// their own here), no electric chairs (shock kits), no office chairs rolling under their mob, no
// smashing item chairs on people, no greyscale material chairs (they leave no stack), no
// spinning bronze chairs, no ghost rotation; alt+right click (clockwise) is not told apart from
// alt-click.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmItem, DmObj};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, Facing, Layer, TilePos};
use ss13_world::{Border, Density, SpatialIndex};

use crate::health::Stat;
use crate::inventory::Inventory;
use crate::medical::surgery::Buckled;
use crate::medical::{name_of, visible};
use crate::sound::{PlaySound, SHORT_RANGE_SOUND_EXTRARANGE, playsound};
use crate::tools::{Actor, ToolKind, tell, tool_of};

const CHAIR: &str = "/obj/structure/chair";
const ITEM_CHAIR: &str = "/obj/item/chair";
const BED: &str = "/obj/structure/bed";
const PEW: &str = "/obj/structure/chair/pew";
const BRONZE: &str = "/obj/structure/chair/bronze";
const STOOL: &str = "/obj/structure/chair/stool";
const BAR_STOOL: &str = "/obj/structure/chair/stool/bar";
/// `/obj/structure/chair/stool/post_buckle_mob`: `add_offsets(type, z_add = 4)`; the bar stool's
/// own call with the same key replaces it with 7.
const STOOL_UP: i16 = 4;
const BAR_STOOL_UP: i16 = 7;
/// `ABOVE_MOB_LAYER`.
const ABOVE_MOB_LAYER: f32 = 4.1;
const RATCHET: &str = "sound/items/tools/ratchet.ogg";

/// On a mob buckled to a chair (tg `buckle_lying = 0`): it sits upright facing the chair's way,
/// drawn `up` pixels higher.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seated {
    pub chair: Entity,
    pub up: i16,
}

pub struct FurniturePlugin;

impl Plugin for FurniturePlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // An item chair is set down.
        app.add_click_handler(ClickPhase::AttackSelf, 2200, |w, c| {
            crate::furniture::attack_self(w, c.client, c.mob, c.target)
        });
        app.add_observer(add_bed).add_systems(
            Update,
            (seated_facing, unseat_orphans).after(crate::click::ResolveClicks),
        );
    }
}

/// `/obj/structure/chair` and its subtypes (sofas, pews, stools, benches).
pub fn is_chair(path: &str) -> bool {
    is_under(path, CHAIR)
}

/// `can_buckle` of the furniture (and ridden vehicles, `crate::vehicles`) the drag-and-drop
/// buckling covers.
pub fn can_buckle(path: &str) -> bool {
    is_chair(path)
        || crate::medical::surgery::buckles_lying(path)
        || crate::vehicles::is_ridable(path)
}

fn obj(world: &World, e: Entity) -> Option<&DmObj> {
    world.get::<DmObj>(e)
}

fn facing(world: &World, e: Entity) -> Dir {
    world.get::<Facing>(e).map_or(Dir::South, |f| f.0)
}

fn set_facing(world: &mut World, e: Entity, dir: Dir) {
    match world.get_mut::<Facing>(e) {
        Some(mut f) => {
            f.set_if_neq(Facing(dir));
        }
        None => {
            world.entity_mut(e).insert(Facing(dir));
        }
    }
}

fn sound(world: &mut World, at: Entity, file: &str) {
    if file.is_empty() {
        return;
    }
    if let Some(&pos) = world.get::<TilePos>(at) {
        // `playsound(src, buckle_sound, 50, TRUE, SHORT_RANGE_SOUND_EXTRARANGE)`.
        playsound(
            world,
            PlaySound::new(pos, file, 50.0, true).extrarange(SHORT_RANGE_SOUND_EXTRARANGE),
        );
    }
}

/// The mob seated on `chair`, if any.
fn sitter(world: &mut World, chair: Entity) -> Option<Entity> {
    world
        .query::<(Entity, &Seated)>()
        .iter(world)
        .find(|(_, s)| s.chair == chair)
        .map(|(e, _)| e)
}

/// `handle_layer`: a chair facing north with someone on it is drawn above them.
fn refresh_layer(world: &mut World, chair: Entity) {
    let Some(base) = world.get::<DmAtom>(chair).map(|a| a.layer) else {
        return;
    };
    let occupied = sitter(world, chair).is_some();
    let layer = if occupied && facing(world, chair) == Dir::North {
        ABOVE_MOB_LAYER
    } else {
        base
    };
    if let Some(mut appearance) = world.get_mut::<Appearance>(chair)
        && appearance.layer != Layer(layer)
    {
        appearance.layer = Layer(layer);
    }
}

/// After `crate::medical::surgery::buckle` put `mob` on `furniture`: the buckle sound, and for a
/// chair the seat (`buckle_mob` -> `setDir(dir)`, `post_buckle_mob`) and `buckle_feedback`.
/// True when the furniture is a chair (its feedback is given here).
pub(crate) fn post_buckle(world: &mut World, actor: Actor, mob: Entity, furniture: Entity) -> bool {
    let Some(dm) = obj(world, furniture).cloned() else {
        return false;
    };
    sound(world, furniture, &dm.buckle_sound);
    if crate::vehicles::is_ridable(&dm.path) {
        crate::vehicles::post_buckle(world, actor, mob, furniture);
        return true;
    }
    if !is_chair(&dm.path) {
        return false;
    }
    let up = if is_under(&dm.path, BAR_STOOL) {
        BAR_STOOL_UP
    } else if is_under(&dm.path, STOOL) {
        STOOL_UP
    } else {
        0
    };
    world.entity_mut(mob).insert(Seated {
        chair: furniture,
        up,
    });
    let dir = facing(world, furniture);
    set_facing(world, mob, dir);
    if let Some(&pos) = world.get::<TilePos>(furniture) {
        crate::tables::reelevate(world, pos);
    }
    refresh_layer(world, furniture);
    let (user, target, what) = (
        name_of(world, actor.mob),
        name_of(world, mob),
        name_of(world, furniture),
    );
    if mob == actor.mob {
        visible(
            world,
            mob,
            &format!("Вы садитесь на {what}."),
            &format!("{user} садится на {what}."),
            Span::Notice,
        );
    } else {
        visible(
            world,
            actor.mob,
            &format!("Вы усаживаете {target} на {what}."),
            &format!("{user} усаживает {target} на {what}."),
            Span::Notice,
        );
    }
    true
}

/// After `crate::medical::surgery::unbuckle` freed `mob` from `furniture`: the unbuckle sound,
/// and for a chair `post_unbuckle_mob` and `unbuckle_feedback`. True for a chair.
pub(crate) fn post_unbuckle(
    world: &mut World,
    actor: Actor,
    mob: Entity,
    furniture: Entity,
) -> bool {
    let Some(dm) = obj(world, furniture).cloned() else {
        return false;
    };
    sound(world, furniture, &dm.unbuckle_sound);
    if crate::vehicles::is_ridable(&dm.path) {
        crate::vehicles::post_unbuckle(world, actor, mob, furniture);
        return true;
    }
    if !is_chair(&dm.path) {
        return false;
    }
    unseat(world, mob, furniture);
    let (user, target, what) = (
        name_of(world, actor.mob),
        name_of(world, mob),
        name_of(world, furniture),
    );
    if mob == actor.mob {
        visible(
            world,
            mob,
            &format!("Вы встаёте с {what}."),
            &format!("{user} встаёт с {what}."),
            Span::Notice,
        );
    } else {
        visible(
            world,
            actor.mob,
            &format!("Вы помогаете {target} встать с {what}."),
            &format!("{user} помогает {target} встать с {what}."),
            Span::Notice,
        );
    }
    true
}

/// `post_unbuckle_mob`: the stool's offset off, the chair's layer back.
fn unseat(world: &mut World, mob: Entity, chair: Entity) {
    if world.get_entity(mob).is_ok() {
        world.entity_mut(mob).remove::<Seated>();
        if let Some(&pos) = world.get::<TilePos>(mob) {
            crate::tables::reelevate(world, pos);
        }
    }
    if world.get_entity(chair).is_ok() {
        refresh_layer(world, chair);
    }
}

/// tg `face_atom` returns while buckled: a seated mob keeps facing its chair's way, whatever
/// turned it.
fn seated_facing(
    mut mobs: Query<(&Seated, &mut Facing), Changed<Facing>>,
    chairs: Query<Option<&Facing>, Without<Seated>>,
) {
    for (seat, mut facing) in &mut mobs {
        if let Ok(chair) = chairs.get(seat.chair) {
            facing.set_if_neq(chair.copied().unwrap_or(Facing(Dir::South)));
        }
    }
}

/// A seated mob whose buckle ended some other way (the chair gone or moved, the mob dragged off:
/// `crate::medical::surgery`'s checks) gets up.
fn unseat_orphans(world: &mut World) {
    let orphans: Vec<(Entity, Entity)> = world
        .query_filtered::<(Entity, &Seated), Without<Buckled>>()
        .iter(world)
        .map(|(e, s)| (e, s.chair))
        .collect();
    for (mob, chair) in orphans {
        unseat(world, mob, chair);
    }
}

/// `/datum/element/elevation` of beds.
fn add_bed(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    if let Ok(o) = objs.get(add.entity)
        && is_under(&o.path, BED)
        && o.elevation > 0
    {
        commands
            .entity(add.entity)
            .insert(crate::tables::Elevating(o.elevation as i16));
    }
}

/// BYOND `turn(dir, 90)`: counterclockwise.
fn turn_ccw(dir: Dir) -> Dir {
    match dir {
        Dir::North => Dir::West,
        Dir::West => Dir::South,
        Dir::South => Dir::East,
        Dir::East => Dir::North,
        Dir::NorthEast => Dir::NorthWest,
        Dir::NorthWest => Dir::SouthWest,
        Dir::SouthWest => Dir::SouthEast,
        Dir::SouthEast => Dir::NorthEast,
    }
}

/// `simple_rotation/rotate_left` (alt-click) of a chair, the click already in reach: turns it
/// and whoever sits on it (`handle_rotation`). True when `target` is a rotatable chair.
pub fn click_alt(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(path) = obj(world, target).map(|o| o.path.clone()) else {
        return false;
    };
    // The bronze chair spins on its own click_alt; not ported.
    if !is_chair(&path) || is_under(&path, BRONZE) {
        return false;
    }
    // `can_user_rotate`: `can_perform_action(NEED_DEXTERITY)`.
    if world.get::<Stat>(mob) != Some(&Stat::Conscious)
        || crate::status_effects::incapacitation(world, mob).incapacitated
    {
        return true;
    }
    if is_under(&path, PEW) {
        // `ROTATION_REQUIRE_WRENCH`.
        let wrench = world
            .get::<Inventory>(mob)
            .and_then(Inventory::active_item)
            .and_then(|i| tool_of(world, i))
            .is_some_and(|(kind, _)| kind == ToolKind::Wrench);
        if !wrench {
            tell(world, client, Span::Warning, "Нужен гаечный ключ!");
            return true;
        }
        if let Some(&pos) = world.get::<TilePos>(target) {
            playsound(world, PlaySound::new(pos, RATCHET, 50.0, true));
        }
    }
    let dir = turn_ccw(facing(world, target));
    set_facing(world, target, dir);
    if let Some(sitter) = sitter(world, target) {
        set_facing(world, sitter, dir);
    }
    refresh_layer(world, target);
    true
}

/// `wrench_act_secondary` of chairs and beds (a bed only with `can_deconstruct`): taken apart
/// at once. True when it was theirs.
pub(crate) fn wrench_secondary(world: &mut World, target: Entity) -> bool {
    let Some(dm) = obj(world, target).cloned() else {
        return false;
    };
    let bed = is_under(&dm.path, BED) && dm.can_deconstruct;
    if !is_chair(&dm.path) && !bed {
        return false;
    }
    if let Some(&pos) = world.get::<TilePos>(target) {
        playsound(world, PlaySound::new(pos, RATCHET, 50.0, true));
    }
    crate::obj_damage::deconstruct(world, target, true);
    true
}

/// `atom_deconstruct` of chairs and beds: their build stack on the tile. True when `path` is
/// one of them.
pub(crate) fn deconstruct(world: &mut World, dm: &DmObj, pos: TilePos) -> bool {
    if !is_chair(&dm.path) && !is_under(&dm.path, BED) {
        return false;
    }
    if !dm.build_stack.is_empty() && dm.build_stack_amount > 0 {
        crate::construction::spawn_stack(world, &dm.build_stack, pos, dm.build_stack_amount);
    }
    true
}

/// `/obj/structure/chair/mouse_drop_dragged` onto its user: a chair with an `item_chair` and
/// nobody on it is picked up as that item. True when the drag was a chair's.
pub(crate) fn mouse_drop_dragged(world: &mut World, actor: Actor, chair: Entity) -> bool {
    let Some(dm) = obj(world, chair).cloned() else {
        return false;
    };
    if !is_chair(&dm.path) {
        return false;
    }
    if dm.item_chair.is_empty() || crate::medical::surgery::buckled_to(world, chair).is_some() {
        return true;
    }
    let Some(&pos) = world.get::<TilePos>(chair) else {
        return true;
    };
    let Some(item) = crate::construction::spawn_obj(world, &dm.item_chair, pos) else {
        return true;
    };
    let (user, what) = (name_of(world, actor.mob), name_of(world, chair));
    visible(
        world,
        actor.mob,
        &format!("Вы хватаете {what}."),
        &format!("{user} хватает {what}."),
        Span::Notice,
    );
    world.despawn(chair);
    crate::construction::tile_changed(world, pos);
    if let Some(inventory) = world.get::<Inventory>(actor.mob).cloned() {
        crate::storage::put_in_hands(world, actor.mob, &inventory, item);
    }
    true
}

/// The chair structure an item chair is set down as (tg `origin_type`: the same path under
/// `/obj/structure/chair`).
pub fn origin_type(item_path: &str) -> Option<String> {
    is_under(item_path, ITEM_CHAIR).then(|| item_path.replacen(ITEM_CHAIR, CHAIR, 1))
}

/// `/obj/item/chair/attack_self` -> `plant`: the chair stands on the user's tile, facing the
/// user's way, unless a chair or something dense (not a border) is there. True for item chairs.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(origin) = world.get::<DmItem>(item).and_then(|i| origin_type(&i.path)) else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return true;
    };
    let what = name_of(world, item);
    let w: &World = world;
    let blocker = w
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter_map(|e| Some((e, w.get::<DmObj>(e)?)))
        .find_map(|(e, o)| {
            if is_chair(&o.path) {
                Some(format!("Здесь уже есть {}!", name_of(w, e)))
            } else if w.get::<Density>(e).is_some() && w.get::<Border>(e).is_none() {
                Some("Здесь уже что-то есть!".to_owned())
            } else {
                None
            }
        });
    if let Some(text) = blocker {
        tell(world, client, Span::Warning, text);
        return true;
    }
    if crate::inventory::no_drop(world, item) {
        tell(
            world,
            client,
            Span::Warning,
            format!("{what} прилип к вашей руке!"),
        );
        return true;
    }
    let Some(chair) = crate::construction::spawn_obj(world, &origin, pos) else {
        return true;
    };
    let dir = facing(world, mob);
    set_facing(world, chair, dir);
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        &format!("Вы ставите {what}."),
        &format!("{user} ставит {what}."),
        Span::Notice,
    );
    world.despawn(item);
    crate::construction::tile_changed(world, pos);
    true
}

/// tg `examine()` of chairs and beds.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(dm) = obj(world, target) else {
        return;
    };
    if is_chair(&dm.path) {
        lines.push((Span::Notice, "Скреплено парой болтов.".to_owned()));
        let occupied = world
            .try_query::<&Buckled>()
            .is_some_and(|mut q| q.iter(world).any(|b| b.0 == target));
        if !occupied {
            lines.push((
                Span::Notice,
                "Перетащите на него своего персонажа, чтобы сесть.".to_owned(),
            ));
        }
    } else if is_under(&dm.path, BED) && dm.can_deconstruct {
        lines.push((Span::Notice, "Скреплено парой болтов.".to_owned()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_chairs_stand_as_their_structure() {
        assert_eq!(
            origin_type("/obj/item/chair").as_deref(),
            Some("/obj/structure/chair")
        );
        assert_eq!(
            origin_type("/obj/item/chair/stool/bar/dark").as_deref(),
            Some("/obj/structure/chair/stool/bar/dark")
        );
        assert_eq!(origin_type("/obj/item/chairs"), None);
        assert_eq!(origin_type("/obj/item/toy"), None);
    }

    #[test]
    fn alt_click_turns_counterclockwise() {
        let mut dir = Dir::South;
        let mut seen = Vec::new();
        for _ in 0..4 {
            dir = turn_ccw(dir);
            seen.push(dir);
        }
        assert_eq!(seen, [Dir::East, Dir::North, Dir::West, Dir::South]);
    }

    #[test]
    fn chairs_and_beds_buckle() {
        assert!(can_buckle("/obj/structure/chair/stool/bar"));
        assert!(can_buckle("/obj/structure/chair/sofa/left"));
        assert!(can_buckle("/obj/structure/bed/medical"));
        assert!(can_buckle("/obj/structure/table/optable"));
        assert!(!can_buckle("/obj/structure/table"));
        assert!(!can_buckle("/obj/structure/chairs"));
    }
}
