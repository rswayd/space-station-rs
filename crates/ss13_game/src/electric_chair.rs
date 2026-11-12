//! Electric chairs, shock kits and electropacks (server), after tg
//! `code/game/objects/structures/electricchair.dm`, `code/modules/assembly/shock_kit.dm`,
//! `code/datums/components/electrified_buckle.dm`, `/obj/structure/chair/electrify_self` and
//! `code/game/objects/items/devices/radio/electropack.dm`.
//!
//! - An electropack ([`Electropack`]) listens on its frequency (144.9) for its code (2): worn or
//!   held by a mob and switched on, it shocks the mob (sparks, a step aside, 10 s paralysis,
//!   10 s cooldown); inside a shock kit it pulses the kit.
//! - A helmet used on an electropack makes a shock kit ([`ShockKit`], "electrohelmet
//!   assembly") holding both; a wrench takes it apart. A mapped kit makes its own parts.
//! - A shock kit used on a chair electrifies it ([`ElectrifiedBuckle`]): whoever is buckled
//!   to it is shocked every 5 s from the net of a cable on its tile (`powernet.avail / 5000`
//!   burn, at least 10 available), each pulse of the kit's electropack opens or closes the
//!   circuit, a screwdriver takes the kit out. The mapped electric chair shocks for 50 through
//!   any gloves, needs no power, and a wrench turns it back into a plain chair.
// shortcut: no Electropack window (frequency, code and power stay their defaults; the kit's
// attack_self does nothing), no blind "deep sharp shock" line, no
// tool sounds; area-power and signal-shock modes of the component (used by other atoms) are
// not ported.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmCable, DmClothing, DmItem, DmObj, IconIndex};
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Appearance, Dir, Facing, Overlay, TilePos};

use crate::health::DEFAULT_MESSAGE_RANGE;
use crate::inventory::{CarriedBy, Inventory};
use crate::security::name_of;
use crate::storage::{Stored, StoredIn, put_inside, take_out};
use crate::tools::{ToolKind, tell, tool_of};

const CHAIR: &str = "/obj/structure/chair";
const E_CHAIR: &str = "/obj/structure/chair/e_chair";
const SHOCK_KIT: &str = "/obj/item/assembly/shock_kit";
const ELECTROPACK: &str = "/obj/item/electropack";
const HELMET: &str = "/obj/item/clothing/head/helmet";
const CHAIRS_DMI: &str = "icons/obj/chairs.dmi";
const ECHAIR_OVER: &str = "echair_over";

/// `FREQ_ELECTROPACK`, deci-kHz.
pub const FREQ_ELECTROPACK: u16 = 1449;
/// `/obj/item/electropack::code`.
pub const ELECTROPACK_CODE: i32 = 2;
/// `addtimer(VARSET_CALLBACK(src, shock_cooldown, FALSE), 10 SECONDS)`.
const PACK_COOLDOWN_SECS: f64 = 10.0;
/// `L.Paralyze(100)`.
const PACK_PARALYZE_SECS: f32 = 10.0;
/// `shock_loop_time`.
const SHOCK_LOOP_SECS: f64 = 5.0;
/// `ELECTRIC_BUCKLE_SHOCK_STRENGTH_DIVISOR`.
const SHOCK_STRENGTH_DIVISOR: f64 = 5000.0;
/// `ELECTRIC_BUCKLE_MINUMUM_POWERNET_STRENGTH`.
const MIN_POWERNET: f64 = 10.0;
/// `shock_damage` of the mapped electric chair.
const E_CHAIR_DAMAGE: f32 = 50.0;

/// tg `/obj/item/electropack` state.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Electropack {
    pub on: bool,
    pub frequency: u16,
    pub code: i32,
    /// End of `shock_cooldown`, game seconds.
    pub ready_at: f64,
}

impl Default for Electropack {
    fn default() -> Self {
        Self {
            on: true,
            frequency: FREQ_ELECTROPACK,
            code: ELECTROPACK_CODE,
            ready_at: 0.0,
        }
    }
}

/// A shock kit: its helmet and electropack (in its [`Stored`]).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShockKit {
    pub helmet: Option<Entity>,
    pub pack: Option<Entity>,
}

/// tg `/datum/component/electrified_buckle` on a chair.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ElectrifiedBuckle {
    /// `required_object`: the shock kit inside the chair (`SHOCK_REQUIREMENT_ITEM`,
    /// `LIVE_CABLE`, `SIGNAL_RECEIVED_TOGGLE`); `None`: the mapped electric chair.
    pub kit: Option<Entity>,
    pub shock_on_loop: bool,
    /// `electric_buckle_cooldown`: the next shock while someone is buckled.
    pub next_shock: Option<f64>,
}

pub struct ElectricChairPlugin;

impl Plugin for ElectricChairPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // A helmet on an electropack, a shock kit on a chair, the screwdriver and wrenches.
        app.add_click_handler(ClickPhase::HeldOnTarget, 2959, |w, c| {
            c.held.is_some_and(|h| {
                crate::electric_chair::item_interaction(
                    w,
                    c.client,
                    c.mob,
                    h,
                    c.target,
                    c.modifiers.right,
                )
            })
        });
        // A helmet on the electropack, a wrench on the shock kit in the other hand.
        app.add_click_handler(ClickPhase::OtherHand, 469, |w, c| {
            c.held.is_some_and(|h| {
                crate::electric_chair::item_interaction(
                    w,
                    c.client,
                    c.mob,
                    h,
                    c.target,
                    c.modifiers.right,
                )
            })
        });
        app.add_observer(add_item)
            .add_observer(add_chair)
            .add_systems(
                Update,
                shock_loop
                    .after(crate::click::ResolveClicks)
                    .run_if(any_with_component::<ElectrifiedBuckle>),
            );
    }
}

/// Electropacks get their state, mapped shock kits their parts (`shock_kit/Initialize`); a
/// kit made by hand (spawned without a tile) gets them from [`combine`].
fn add_item(add: On<Add<DmItem>>, items: Query<(&DmItem, Has<TilePos>)>, mut commands: Commands) {
    let e = add.entity;
    let Ok((item, placed)) = items.get(e) else {
        return;
    };
    if is_under(&item.path, ELECTROPACK) {
        commands.entity(e).insert(Electropack::default());
    } else if is_under(&item.path, SHOCK_KIT) && placed {
        commands.queue(move |world: &mut World| {
            let helmet = crate::construction::spawn_detached(world, HELMET);
            let pack = crate::construction::spawn_detached(world, ELECTROPACK);
            for part in [helmet, pack].into_iter().flatten() {
                put_inside(world, part, e);
            }
            world.entity_mut(e).insert(ShockKit { helmet, pack });
        });
    }
}

/// The mapped electric chair: shocks every 5 s through gloves (`SHOCK_NOGLOVES`), unpowered.
fn add_chair(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    if let Ok(o) = objs.get(add.entity)
        && is_under(&o.path, E_CHAIR)
    {
        let e = add.entity;
        commands.entity(e).insert(ElectrifiedBuckle {
            kit: None,
            shock_on_loop: true,
            next_shock: None,
        });
        commands.queue(move |world: &mut World| set_electrified_name(world, e, true));
    }
}

/// The component's `name = "electrified [initial(name)]"`, and back.
fn set_electrified_name(world: &mut World, chair: Entity, on: bool) {
    if world.get_entity(chair).is_err() {
        return;
    }
    if !on {
        world.entity_mut(chair).remove::<Name>();
        return;
    }
    let base = world
        .get::<ss13_mapload::DmName>(chair)
        .map_or_else(|| "стул".to_owned(), |n| n.name.clone());
    world
        .entity_mut(chair)
        .insert(Name::new(format!("электрифицированный {base}")));
}

fn item_path(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmItem>(e).map(|i| i.path.as_str())
}

fn obj_path(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmObj>(e).map(|o| o.path.as_str())
}

/// Held by `mob` or next to it.
fn in_reach(world: &World, mob: Entity, e: Entity) -> bool {
    if world.get::<CarriedBy>(e).is_some_and(|c| c.0 == mob) {
        return true;
    }
    match (world.get::<TilePos>(mob), world.get::<TilePos>(e)) {
        (Some(&a), Some(&b)) => a.chebyshev(b).is_some_and(|d| d <= 1),
        _ => false,
    }
}

/// The clicks of this module. True when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    right: bool,
) -> bool {
    if held == target || !in_reach(world, mob, target) {
        return false;
    }
    let held_path = item_path(world, held).unwrap_or_default().to_owned();
    let tool = tool_of(world, held).map(|t| t.0);
    if let Some(target_path) = item_path(world, target).map(str::to_owned) {
        // `/obj/item/electropack/item_interaction`.
        if is_under(&target_path, ELECTROPACK) && is_under(&held_path, HELMET) {
            combine(world, client, mob, held, target);
            return true;
        }
        // `/obj/item/assembly/shock_kit/wrench_act`.
        if is_under(&target_path, SHOCK_KIT) && tool == Some(ToolKind::Wrench) {
            disassemble(world, client, target);
            return true;
        }
        return false;
    }
    let Some(chair_path) = obj_path(world, target).map(str::to_owned) else {
        return false;
    };
    if !is_under(&chair_path, CHAIR) {
        return false;
    }
    let buckle = world.get::<ElectrifiedBuckle>(target).copied();
    // `/obj/structure/chair/attackby`: a shock kit on a chair not electrified yet.
    if is_under(&held_path, SHOCK_KIT) {
        if buckle.is_some() {
            return false;
        }
        electrify(world, client, held, target);
        return true;
    }
    // `move_required_object_from_contents` on `COMSIG_ATOM_TOOL_ACT(TOOL_SCREWDRIVER)`.
    if tool == Some(ToolKind::Screwdriver) && buckle.is_some_and(|b| b.kit.is_some()) {
        release(world, target);
        return true;
    }
    // `/obj/structure/chair/e_chair/wrench_act` (the left click; the right click takes any
    // chair apart, `crate::furniture`).
    if tool == Some(ToolKind::Wrench) && !right && is_under(&chair_path, E_CHAIR) {
        unelectrify_mapped(world, target);
        return true;
    }
    false
}

/// A helmet attached to an electropack: both go into a new shock kit in the user's hands.
fn combine(world: &mut World, client: Entity, mob: Entity, helmet: Entity, pack: Entity) {
    if crate::inventory::no_drop(world, helmet) {
        let what = name_of(world, helmet);
        tell(
            world,
            client,
            Span::Warning,
            format!("{what} прилип к вашей руке, вы не можете прикрепить его!"),
        );
        return;
    }
    let Some(kit) = crate::construction::spawn_detached(world, SHOCK_KIT) else {
        return;
    };
    put_inside(world, helmet, kit);
    put_inside(world, pack, kit);
    world.entity_mut(kit).insert(ShockKit {
        helmet: Some(helmet),
        pack: Some(pack),
    });
    crate::assemblies::server::to_hands(world, mob, kit);
}

/// The kit's parts on its tile, the kit gone.
fn disassemble(world: &mut World, client: Entity, kit: Entity) {
    let what = name_of(world, kit);
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы разбираете {what}."),
    );
    for part in parts_of(world, kit) {
        crate::assemblies::server::drop_out(world, part, kit);
    }
    // In a hand the kit has no tile of its own: the parts went to the holder's (`drop_out`).
    world.despawn(kit);
}

fn parts_of(world: &World, kit: Entity) -> Vec<Entity> {
    world
        .get::<Stored>(kit)
        .map(|s| s.items().to_vec())
        .unwrap_or_default()
}

/// `electrify_self`: the kit goes into the chair, which gets the component and its overlay.
fn electrify(world: &mut World, client: Entity, kit: Entity, chair: Entity) {
    if crate::inventory::no_drop(world, kit) {
        return;
    }
    put_inside(world, kit, chair);
    // A kit moved in while someone already sits: the cooldown starts now (`on_buckle`).
    let occupied = crate::medical::surgery::buckled_to(world, chair).is_some();
    let now = crate::tools::now(world);
    world.entity_mut(chair).insert(ElectrifiedBuckle {
        kit: Some(kit),
        shock_on_loop: true,
        next_shock: occupied.then_some(now + SHOCK_LOOP_SECS),
    });
    if let Some(id) = echair_overlay(world)
        && let Some(mut appearance) = world.get_mut::<Appearance>(chair)
    {
        appearance.overlays.push(Overlay::new(id));
    }
    let what = name_of(world, chair);
    set_electrified_name(world, chair, true);
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы прикручиваете набор шокера к {what}, электризуя его!"),
    );
}

fn echair_overlay(world: &World) -> Option<ss13_shared::IconStateId> {
    world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(CHAIRS_DMI, ECHAIR_OVER))
}

/// `UnregisterFromParent` of a kit-electrified chair (screwdriver, the chair taken apart or
/// picked up): the kit out onto its tile, the overlay off. Mapped electric chairs keep theirs.
pub fn release(world: &mut World, chair: Entity) {
    let Some(ElectrifiedBuckle { kit: Some(kit), .. }) =
        world.get::<ElectrifiedBuckle>(chair).copied()
    else {
        return;
    };
    world.entity_mut(chair).remove::<ElectrifiedBuckle>();
    set_electrified_name(world, chair, false);
    if world.get::<StoredIn>(kit).is_some_and(|s| s.0 == chair)
        && let Some(&pos) = world.get::<TilePos>(chair)
    {
        take_out(world, kit, pos);
    }
    if let Some(id) = echair_overlay(world)
        && let Some(mut appearance) = world.get_mut::<Appearance>(chair)
        && let Some(i) = appearance.overlays.iter().position(|o| o.icon_state == id)
    {
        appearance.overlays.remove(i);
    }
}

/// `/obj/structure/chair/e_chair/wrench_act`: a plain chair facing the same way.
fn unelectrify_mapped(world: &mut World, e_chair: Entity) {
    let Some(&pos) = world.get::<TilePos>(e_chair) else {
        return;
    };
    let dir = world.get::<Facing>(e_chair).map_or(Dir::South, |f| f.0);
    let Some(chair) = crate::construction::spawn_obj(world, CHAIR, pos) else {
        return;
    };
    world.entity_mut(chair).insert(Facing(dir));
    world.despawn(e_chair);
    crate::construction::tile_changed(world, pos);
}

/// `post_signal` on an electropack's band: every electropack on `frequency` with `code`
/// (within `range` tiles of `from` on the same level, when it is not 0) receives it.
pub fn electropack_signal(
    world: &mut World,
    frequency: u16,
    code: i32,
    from: Option<TilePos>,
    range: u32,
) {
    let packs: Vec<Entity> = world
        .query::<(Entity, &Electropack)>()
        .iter(world)
        .filter(|(_, p)| p.frequency == frequency && p.code == code)
        .map(|(e, _)| e)
        .collect();
    for pack in packs {
        if range > 0 {
            let (Some(from), Some(to)) = (from, crate::explosions::turf_of(world, pack)) else {
                continue;
            };
            if from.chebyshev(to).is_none_or(|d| d > range) {
                continue;
            }
        }
        receive_signal(world, pack);
    }
}

/// `/obj/item/electropack/receive_signal` (the code already matched).
fn receive_signal(world: &mut World, pack: Entity) {
    let Some(state) = world.get::<Electropack>(pack).copied() else {
        return;
    };
    // `isliving(loc)`: worn or held, not inside a bag.
    let wearer = world
        .get::<CarriedBy>(pack)
        .map(|c| c.0)
        .filter(|_| world.get::<StoredIn>(pack).is_none());
    if let Some(mob) = wearer
        && state.on
    {
        let now = crate::tools::now(world);
        if now < state.ready_at {
            return;
        }
        if let Some(mut p) = world.get_mut::<Electropack>(pack) {
            p.ready_at = now + PACK_COOLDOWN_SECS;
        }
        shock_wearer(world, mob);
    }
    // `master.pulsed()`: a kit's electropack pulses the kit, which toggles its chair.
    if let Some(kit) = world.get::<StoredIn>(pack).map(|s| s.0)
        && world.get::<ShockKit>(kit).is_some()
        && let Some(chair) = world.get::<StoredIn>(kit).map(|s| s.0)
        && world
            .get::<ElectrifiedBuckle>(chair)
            .is_some_and(|b| b.kit == Some(kit))
    {
        toggle_shock_loop(world, chair);
    }
}

/// The electropack's own shock: a step aside, sparks, 10 s paralysis.
fn shock_wearer(world: &mut World, mob: Entity) {
    let dir = {
        let mut rng = world.resource_mut::<crate::combat::CombatRng>();
        Dir::CARDINALS[rng.range(0, 3) as usize]
    };
    let _ = crate::pull::step_once(world, mob, dir, None);
    crate::tools::tell_mob(world, mob, Span::Danger, "Вы чувствуете резкий удар током!");
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::effects::particles::do_sparks(world, 3, true, pos);
    }
    crate::status_effects::apply(
        world,
        mob,
        crate::status_effects::Incap::Paralyze,
        PACK_PARALYZE_SECS,
    );
}

/// `toggle_shock_loop`.
fn toggle_shock_loop(world: &mut World, chair: Entity) {
    let on = {
        let Some(mut buckle) = world.get_mut::<ElectrifiedBuckle>(chair) else {
            return;
        };
        buckle.shock_on_loop = !buckle.shock_on_loop;
        buckle.shock_on_loop
    };
    let Some(&pos) = world.get::<TilePos>(chair) else {
        return;
    };
    let what = name_of(world, chair);
    let text = if on {
        format!("Вы слышите, как в {what} замыкается электрическая цепь!")
    } else {
        format!("{what} щёлкает размыкающейся цепью — пока что он безопасен.")
    };
    crate::security::show(
        world,
        pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Local,
        (Span::Notice, ss13_shared::capitalize(&text)),
        &[],
    );
}

/// `on_buckle` starts the cooldown, `process` shocks whoever is buckled each time it runs out
/// while the circuit is closed.
// Cost: one pass over the buckled mobs per tick, only while an electrified chair exists.
fn shock_loop(world: &mut World) {
    let now = crate::tools::now(world);
    let occupied: Vec<(Entity, Entity)> = world
        .query::<(Entity, &crate::medical::surgery::Buckled)>()
        .iter(world)
        .filter(|(_, b)| world.get::<ElectrifiedBuckle>(b.0).is_some())
        .map(|(mob, b)| (b.0, mob))
        .collect();
    let chairs: Vec<(Entity, ElectrifiedBuckle)> = world
        .query::<(Entity, &ElectrifiedBuckle)>()
        .iter(world)
        .map(|(e, b)| (e, *b))
        .collect();
    for (chair, buckle) in chairs {
        let victims: Vec<Entity> = occupied
            .iter()
            .filter(|(c, _)| *c == chair)
            .map(|(_, m)| *m)
            .collect();
        let next = match (victims.is_empty(), buckle.next_shock) {
            (true, None) => continue,
            (true, Some(_)) => None,
            (false, None) => Some(now + SHOCK_LOOP_SECS),
            (false, Some(at)) if now < at || !buckle.shock_on_loop => Some(at),
            (false, Some(_)) => {
                do_electrocution(world, chair, buckle.kit, &victims);
                Some(now + SHOCK_LOOP_SECS)
            }
        };
        if let Some(mut b) = world.get_mut::<ElectrifiedBuckle>(chair) {
            b.next_shock = next;
        }
    }
}

/// `do_electrocution` and the loop's message.
fn do_electrocution(world: &mut World, chair: Entity, kit: Option<Entity>, victims: &[Entity]) {
    let Some(&pos) = world.get::<TilePos>(chair) else {
        return;
    };
    let (damage, through_gloves) = match kit {
        None => (E_CHAIR_DAMAGE, true),
        Some(kit) => {
            if world.get::<StoredIn>(kit).is_none_or(|s| s.0 != chair) {
                return;
            }
            // `get_cable_node()` of the chair's turf, its net's `avail`.
            let avail = live_cable_avail(world, pos);
            if avail < MIN_POWERNET {
                return;
            }
            ((avail / SHOCK_STRENGTH_DIVISOR).round() as f32, false)
        }
    };
    for &mob in victims {
        let siemens = if through_gloves {
            1.0
        } else {
            gloves_siemens(world, mob)
        };
        crate::wires::electrocute_act(world, mob, damage, chair, siemens);
    }
    // `print_message`.
    let what = ss13_shared::capitalize(&name_of(world, chair));
    crate::security::show(
        world,
        pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (Span::Danger, format!("{what} бьёт мощным разрядом!")),
        &[],
    );
}

/// The `avail` of the net of a cable on `pos` (0 without one).
fn live_cable_avail(world: &World, pos: TilePos) -> f64 {
    let Some(grid) = world.get_resource::<crate::power::Grid>() else {
        return 0.0;
    };
    world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .filter(|&&e| world.get::<DmCable>(e).is_some())
        .filter_map(|&e| grid.cable_net(e))
        .filter_map(|n| grid.net(n))
        .map(|n| n.avail)
        .fold(0.0, f64::max)
}

/// The `siemens_coefficient` of the mob's gloves (1 without).
fn gloves_siemens(world: &World, mob: Entity) -> f32 {
    world
        .get::<Inventory>(mob)
        .and_then(|i| i.gloves)
        .map_or(1.0, |g| {
            world
                .get::<DmClothing>(g)
                .map_or(1.0, |c| c.siemens_coefficient)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn electropack_defaults() {
        let p = Electropack::default();
        assert!(p.on);
        assert_eq!((p.frequency, p.code), (1449, 2));
        assert_eq!(crate::assemblies::format_frequency(p.frequency), "144.9");
    }
}
