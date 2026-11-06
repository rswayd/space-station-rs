//! Tank dispensers and tank holders (server), after tg
//! `code/game/objects/structures/tank_dispenser.dm`, `tank_holder.dm` and
//! `code/datums/components/container_item/tank_holder.dm`.
//!
//! - Tank dispenser ([`TankDispenser`]): up to 10 plasma and 10 oxygen tanks (the `/oxygen` and
//!   `/plasma` subtypes start without the other kind). An empty hand takes a plasma tank, a right
//!   click an oxygen one (an inserted tank first, else a new one); a plasma or oxygen tank goes in
//!   while there is room; any other item out of combat mode is refused, in combat mode it hits.
//!   A wrench (2 s) fastens or unfastens it; destroyed, it drops what was put in and 2 iron.
//!   Its overlays show up to 4 oxygen and 5 plasma tanks.
//! - Tank holder ([`TankHolder`]): holds one tank or extinguisher (mapped subtypes start with
//!   theirs). Out of combat mode a tank or extinguisher clicked on it goes in (it turns dense and
//!   shows the item), anything else "does not fit"; an empty hand takes the item out; a wrench
//!   (2 s) fastens or unfastens it; a screwdriver takes an empty one apart into 2 rods.
// shortcut: no thrown items passing through a holder (`CanAllowThrough`), no explosion of the
// held tank with the holder (`contents_explosion`), cyborgs are not told apart from humans, no
// blocked-turf check of the wrench, no fingerprints. Add each with its mechanic.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmItem, DmObj, IconIndex};
use ss13_shared::appearance::Overlay;
use ss13_shared::chat::Span;
use ss13_shared::protocol::ClickModifiers;
use ss13_shared::{Appearance, TilePos};
use ss13_world::Density;

use crate::construction::Job;
use crate::decor::DecorJob;
use crate::examine::Line;
use crate::medical::name_of;
use crate::sound::{PlaySound, playsound};
use crate::storage::{Stored, StoredIn};
use crate::tools::{Actor, ToolKind, tell, tool_of, use_tool};

pub const TANK_DISPENSER: &str = "/obj/structure/tank_dispenser";
pub const TANK_HOLDER: &str = "/obj/structure/tank_holder";
const PLASMA_TANK: &str = "/obj/item/tank/internals/plasma";
const OXYGEN_TANK: &str = "/obj/item/tank/internals/oxygen";
const INTERNALS: &str = "/obj/item/tank/internals";
const EXTINGUISHER: &str = "/obj/item/extinguisher";
const IRON: &str = "/obj/item/stack/sheet/iron";
const RODS: &str = "/obj/item/stack/rods";
const SCREWDRIVER_SOUND: &str = "sound/items/tools/screwdriver.ogg";
/// `TANK_DISPENSER_CAPACITY`.
pub const CAPACITY: u32 = 10;
/// `default_unfasten_wrench` (2 s) and `tank_holder/wrench_act`'s `use_tool(src, user, 20)`.
const WRENCH_DS: f32 = 20.0;

/// `/obj/structure/tank_dispenser`: `oxygentanks`, `plasmatanks`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TankDispenser {
    pub oxygen: u32,
    pub plasma: u32,
}

/// `/obj/structure/tank_holder`: `tank`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TankHolder {
    pub tank: Option<Entity>,
}

pub struct TankStoragePlugin;

impl Plugin for TankStoragePlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::EmptyHand, 3155, |w, c| {
            attack_hand(w, c.client, c.mob, c.target, c.modifiers)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 3440, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
        app.add_observer(add_tank_storage)
            .add_systems(ss13_world::MapInit, fill_mapped_holders);
    }
}

/// `tank_holder/Initialize(mapload)`: a mapped subtype's tank goes in. Runs in the map pass, when
/// the holder's prototype (and its `TilePos`) is complete.
fn fill_mapped_holders(world: &mut World) {
    let holders: Vec<(Entity, String, TilePos)> = world
        .query_filtered::<(Entity, &DmObj, &TilePos, &TankHolder), With<ss13_world::Mapped>>()
        .iter(world)
        .filter(|(.., h)| h.tank.is_none())
        .filter_map(|(e, dm, &pos, _)| mapped_tank(&dm.path).map(|t| (e, t, pos)))
        .collect();
    for (holder, tank, pos) in holders {
        if let Some(item) = crate::construction::spawn_obj(world, &tank, pos) {
            attach(world, holder, item);
        }
    }
}

/// What [`crate::obj_damage`] gives integrity to here.
pub fn breakable(path: &str) -> bool {
    is_under(path, TANK_DISPENSER) || is_under(path, TANK_HOLDER)
}

/// `tank_dispenser` vars and `tank_holder/Initialize` (a mapped holder's tank goes in).
fn add_tank_storage(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(dm) = objs.get(add.entity) else {
        return;
    };
    let entity = add.entity;
    if is_under(&dm.path, TANK_DISPENSER) {
        let oxygen = if is_under(&dm.path, &format!("{TANK_DISPENSER}/plasma")) {
            0
        } else {
            CAPACITY
        };
        let plasma = if is_under(&dm.path, &format!("{TANK_DISPENSER}/oxygen")) {
            0
        } else {
            CAPACITY
        };
        commands
            .entity(entity)
            .insert(TankDispenser { oxygen, plasma });
        commands.queue(move |world: &mut World| refresh_dispenser(world, entity));
    } else if is_under(&dm.path, TANK_HOLDER) {
        commands.entity(entity).insert(TankHolder::default());
    }
}

/// The `tank` path of a mapped holder subtype: `/extinguisher/advanced` holds
/// `/obj/item/extinguisher/advanced`, `/oxygen/red` holds `/obj/item/tank/internals/oxygen/red`.
fn mapped_tank(path: &str) -> Option<String> {
    let rest = path.strip_prefix(TANK_HOLDER)?.strip_prefix('/')?;
    // The mapping helpers of the base type hold nothing.
    let rest = rest.split("/directional").next().unwrap_or_default();
    if rest.is_empty() {
        return None;
    }
    Some(match rest.strip_prefix("extinguisher") {
        Some(sub) => format!("{EXTINGUISHER}{sub}"),
        None => format!("{INTERNALS}/{rest}"),
    })
}

/// `tank_holder_icon_state` and `make_density` of the item's `container_item/tank_holder`
/// component; `None`: the item has none (plasma and plasmaman tanks, everything else).
pub fn holder_look(path: &str) -> Option<(&'static str, bool)> {
    const LOOKS: [(&str, Option<&str>); 14] = [
        ("/obj/item/tank/internals/plasmaman", None),
        ("/obj/item/tank/internals/plasma", None),
        (
            "/obj/item/tank/internals/emergency_oxygen/engi/clown",
            Some("holder_emergency_clown"),
        ),
        (
            "/obj/item/tank/internals/emergency_oxygen/engi",
            Some("holder_emergency_engi"),
        ),
        (
            "/obj/item/tank/internals/emergency_oxygen/double",
            Some("holder_emergency_engi"),
        ),
        (
            "/obj/item/tank/internals/emergency_oxygen",
            Some("holder_emergency"),
        ),
        (
            "/obj/item/tank/internals/oxygen/yellow",
            Some("holder_oxygen_f"),
        ),
        (
            "/obj/item/tank/internals/oxygen/red",
            Some("holder_oxygen_fr"),
        ),
        ("/obj/item/tank/internals/oxygen", Some("holder_oxygen")),
        (
            "/obj/item/tank/internals/anesthetic",
            Some("holder_anesthetic"),
        ),
        ("/obj/item/tank", Some("holder_generic")),
        (
            "/obj/item/extinguisher/advanced",
            Some("holder_foam_extinguisher"),
        ),
        ("/obj/item/extinguisher", Some("holder_extinguisher")),
        // `skub/Initialize`: `AddComponent(..., "holder_skub", FALSE)`.
        ("/obj/item/skub", Some("holder_skub")),
    ];
    let state = LOOKS
        .iter()
        .find(|(root, _)| is_under(path, root))
        .and_then(|&(_, state)| state)?;
    Some((state, state != "holder_skub"))
}

fn item_path(world: &World, item: Entity) -> Option<&str> {
    world.get::<DmItem>(item).map(|i| i.path.as_str())
}

/// The holder's tank, if it is still inside (tg `Exited` clears the var otherwise).
fn held_tank(world: &World, holder: Entity) -> Option<Entity> {
    world
        .get::<TankHolder>(holder)?
        .tank
        .filter(|&t| world.get::<StoredIn>(t).is_some_and(|s| s.0 == holder))
}

/// `container_item/tank_holder/try_attach` past its checks: in, dense, drawn.
fn attach(world: &mut World, holder: Entity, item: Entity) {
    let Some((state, dense)) = item_path(world, item).and_then(holder_look) else {
        return;
    };
    crate::storage::put_inside(world, item, holder);
    world
        .entity_mut(holder)
        .insert(TankHolder { tank: Some(item) });
    if dense {
        world.entity_mut(holder).insert(Density);
    } else {
        world.entity_mut(holder).remove::<Density>();
    }
    crate::obj_damage::set_icon_state(world, holder, state);
}

/// `tank_holder/after_detach_tank`.
fn detach(world: &mut World, holder: Entity) {
    world.entity_mut(holder).insert(TankHolder { tank: None });
    world.entity_mut(holder).remove::<Density>();
    crate::obj_damage::set_icon_state(world, holder, "holder");
}

/// tg `attack_hand` / `attack_hand_secondary` of the dispenser and the holder.
fn attack_hand(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    if let Some(dispenser) = world.get::<TankDispenser>(target).copied() {
        let (count, path, empty) = if modifiers.right {
            (dispenser.oxygen, OXYGEN_TANK, "нет кислородных баллонов!")
        } else {
            (dispenser.plasma, PLASMA_TANK, "нет плазменных баллонов!")
        };
        if count == 0 {
            tell(world, client, Span::Warning, empty);
            return true;
        }
        if dispense(world, target, mob, path) {
            if let Some(mut d) = world.get_mut::<TankDispenser>(target) {
                if modifiers.right {
                    d.oxygen -= 1;
                } else {
                    d.plasma -= 1;
                }
            }
            tell(world, client, Span::Notice, "баллон получен");
            refresh_dispenser(world, target);
        }
        return true;
    }
    if world.get::<TankHolder>(target).is_some() {
        let Some(tank) = held_tank(world, target) else {
            return false;
        };
        let (what, from) = (name_of(world, tank), name_of(world, target));
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы достаёте {what} из {from}."),
        );
        detach(world, target);
        crate::cooking::give_to(world, mob, target, tank);
        return true;
    }
    false
}

/// `tank_dispenser/dispense`: an inserted tank of `path`, else a new one, into the hands.
fn dispense(world: &mut World, dispenser: Entity, mob: Entity, path: &str) -> bool {
    let stored = world.get::<Stored>(dispenser).and_then(|s| {
        s.items()
            .iter()
            .copied()
            .find(|&i| item_path(world, i).is_some_and(|p| is_under(p, path)))
    });
    let Some(tank) = stored.or_else(|| crate::construction::spawn_detached(world, path)) else {
        return false;
    };
    crate::cooking::give_to(world, mob, dispenser, tank);
    true
}

/// tg tool acts and `item_interaction` of the dispenser and the holder.
fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let dispenser = world.get::<TankDispenser>(target).copied();
    let holder = world.get::<TankHolder>(target).is_some();
    if dispenser.is_none() && !holder {
        return false;
    }
    let combat = world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0);
    // `base_item_interaction`: tool acts only out of combat mode.
    if !combat && let Some((tool, _)) = tool_of(world, held) {
        let actor = Actor {
            client,
            mob,
            item: Some(held),
        };
        match tool {
            ToolKind::Wrench => {
                use_tool(
                    world,
                    actor,
                    target,
                    WRENCH_DS,
                    0,
                    Job::Decor(DecorJob::Unfasten),
                );
                return true;
            }
            // `tank_holder/screwdriver_act`: only an empty one.
            ToolKind::Screwdriver if holder && held_tank(world, target).is_none() => {
                if let Some(&pos) = world.get::<TilePos>(target) {
                    playsound(world, PlaySound::new(pos, SCREWDRIVER_SOUND, 50.0, true));
                }
                crate::obj_damage::deconstruct(world, target, true);
                return true;
            }
            _ => {}
        }
    }
    let path = item_path(world, held)
        .map(str::to_owned)
        .unwrap_or_default();
    if let Some(dispenser) = dispenser {
        let plasma = is_under(&path, PLASMA_TANK);
        let oxygen = is_under(&path, OXYGEN_TANK);
        if !plasma && !oxygen {
            // In combat mode it hits the dispenser (`NOBLUDGEON` items are not told apart).
            if combat {
                return false;
            }
            tell(world, client, Span::Warning, "нельзя вставить!");
            return true;
        }
        let count = if plasma {
            dispenser.plasma
        } else {
            dispenser.oxygen
        };
        if count >= CAPACITY {
            tell(world, client, Span::Warning, "заполнено!");
            return true;
        }
        crate::storage::put_inside(world, held, target);
        if let Some(mut d) = world.get_mut::<TankDispenser>(target) {
            if plasma {
                d.plasma += 1;
            } else {
                d.oxygen += 1;
            }
        }
        tell(world, client, Span::Notice, "баллон вставлен");
        refresh_dispenser(world, target);
        return true;
    }
    // The holder: `if(user.combat_mode) return NONE`.
    if combat {
        return false;
    }
    let name = name_of(world, held);
    let from = name_of(world, target);
    if holder_look(&path).is_none() {
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} не помещается в {from}."),
        );
        return true;
    }
    if held_tank(world, target).is_some() {
        tell(
            world,
            client,
            Span::Warning,
            format!("В {from} уже что-то есть."),
        );
        return true;
    }
    attach(world, target, held);
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы помещаете {name} в {from}."),
    );
    true
}

/// `tank_dispenser/update_overlays`.
fn refresh_dispenser(world: &mut World, dispenser: Entity) {
    let Some(d) = world.get::<TankDispenser>(dispenser).copied() else {
        return;
    };
    let mut states = Vec::new();
    match d.oxygen {
        0 => {}
        n @ 1..=3 => states.push(format!("oxygen-{n}")),
        _ => states.push("oxygen-4".to_owned()),
    }
    match d.plasma {
        0 => {}
        n @ 1..=4 => states.push(format!("plasma-{n}")),
        _ => states.push("plasma-5".to_owned()),
    }
    let Some(icon) = world.get::<DmAtom>(dispenser).map(|a| a.icon.clone()) else {
        return;
    };
    let overlays: Vec<Overlay> = {
        let Some(index) = world.get_resource::<IconIndex>() else {
            return;
        };
        states
            .iter()
            .filter_map(|s| index.get(&icon, s))
            .map(Overlay::new)
            .collect()
    };
    if let Some(mut appearance) = world.get_mut::<Appearance>(dispenser)
        && appearance.overlays != overlays
    {
        appearance.overlays = overlays;
    }
}

/// `atom_deconstruct` of the dispenser (inserted tanks, 2 iron) and the holder (2 rods, its
/// tank). True when `dm` was one.
pub(crate) fn deconstruct(world: &mut World, target: Entity, dm: &DmObj, pos: TilePos) -> bool {
    let dispenser = is_under(&dm.path, TANK_DISPENSER);
    if !dispenser && !is_under(&dm.path, TANK_HOLDER) {
        return false;
    }
    let contents: Vec<Entity> = world
        .get::<Stored>(target)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    for item in contents {
        crate::storage::take_out(world, item, pos);
    }
    if dispenser {
        crate::construction::spawn_stack(world, IRON, pos, 2);
    } else {
        crate::construction::spawn_stack(world, RODS, pos, 2);
    }
    true
}

/// `examine` of the dispenser and the holder.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<Line>) {
    if let Some(d) = world.get::<TankDispenser>(target) {
        let text = match (d.plasma, d.oxygen) {
            (0, 0) => return,
            (p, 0) => format!("Осталось плазменных баллонов: {p}."),
            (0, o) => format!("Осталось кислородных баллонов: {o}."),
            (p, o) => format!("Осталось плазменных баллонов: {p}, кислородных: {o}."),
        };
        lines.push((Span::Notice, text));
        return;
    }
    if world.get::<TankHolder>(target).is_none() {
        return;
    }
    let anchored = world
        .get::<crate::obj_damage::Anchored>(target)
        .is_some_and(|a| a.0);
    lines.push((
        Span::Notice,
        if anchored {
            "Он прикручен к полу болтами."
        } else {
            "Болты снизу не закручены."
        }
        .to_owned(),
    ));
    lines.push((
        Span::Notice,
        match held_tank(world, target) {
            Some(tank) => format!("В нём {}.", name_of(world, tank)),
            None => "Он пуст.".to_owned(),
        },
    ));
    lines.push((Span::Notice, "Он держится на винтах.".to_owned()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapped_holders_hold_their_subtype() {
        assert_eq!(
            mapped_tank("/obj/structure/tank_holder/extinguisher/advanced").as_deref(),
            Some("/obj/item/extinguisher/advanced")
        );
        assert_eq!(
            mapped_tank("/obj/structure/tank_holder/oxygen/red").as_deref(),
            Some("/obj/item/tank/internals/oxygen/red")
        );
        assert_eq!(mapped_tank("/obj/structure/tank_holder"), None);
    }

    #[test]
    fn holder_looks_follow_the_most_specific_tank() {
        assert_eq!(
            holder_look("/obj/item/tank/internals/emergency_oxygen/engi/clown"),
            Some(("holder_emergency_clown", true))
        );
        assert_eq!(holder_look("/obj/item/tank/internals/plasma/full"), None);
        assert_eq!(
            holder_look("/obj/item/tank/jetpack"),
            Some(("holder_generic", true))
        );
        assert_eq!(
            holder_look("/obj/item/extinguisher/mini"),
            Some(("holder_extinguisher", true))
        );
        assert_eq!(holder_look("/obj/item/skub"), Some(("holder_skub", false)));
        assert_eq!(holder_look("/obj/item/wrench"), None);
    }
}
