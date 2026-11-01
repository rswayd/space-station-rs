//! Tools (server): tg `tool_behaviour`, `use_tool` and `do_after`
//! (`code/game/objects/items.dm`, `code/__HELPERS/do_after.dm`), welders with fuel
//! (`code/game/objects/items/tools/engineering/weldingtool.dm`) and item stacks used up by
//! construction (`code/game/objects/items/stacks/stack.dm`).
//!
//! A timed action is a [`DoAfter`] on the user's mob. Every tick it is checked like tg's
//! `do_after` loop: the user stays conscious on its tile with the same item in the active hand,
//! the target still exists on its tile, a welder stays lit with the fuel the job needs, a stack
//! keeps enough for it. When the time is up `crate::construction::complete` re-checks the job's
//! own state (the girder is still a girder in the state it had, ...) at the moment of effect.
//! One timed action per mob (tg's default `max_interact_count`); a second one is refused.

use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmItem, DmStack, DmTool, IconIndex};
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_world::WorldSystems;

use crate::construction::Job;
use crate::health::Stat;
use crate::inventory::{CarriedBy, Inventory};

/// `TOOL_FUEL_BURN_INTERVAL`: a lit welder burns one unit of fuel per this many seconds.
pub const TOOL_FUEL_BURN_INTERVAL: f32 = 5.0;
/// `HIGH_TEMPERATURE_REQUIRED` (wall slicing).
pub const HIGH_TEMPERATURE_REQUIRED: f32 = 1500.0;
/// A lit welder hits for 15 burn, an unlit one for 3 brute (`switched_on`, `switched_off`).
const WELDER_LIT_FORCE: f32 = 15.0;
const WELDER_FORCE: f32 = 3.0;

/// tg `TOOL_*` behaviours this server acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    Wrench,
    Screwdriver,
    Crowbar,
    Welder,
    Wirecutters,
    Multitool,
}

impl ToolKind {
    pub fn from_dm(behaviour: &str) -> Option<Self> {
        Some(match behaviour {
            "wrench" => Self::Wrench,
            "screwdriver" => Self::Screwdriver,
            "crowbar" => Self::Crowbar,
            "welder" => Self::Welder,
            "cutters" => Self::Wirecutters,
            "multitool" => Self::Multitool,
            _ => return None,
        })
    }
}

/// The tool behaviour and `toolspeed` of `item`.
pub fn tool_of(world: &World, item: Entity) -> Option<(ToolKind, f32)> {
    let tool = world.get::<DmTool>(item)?;
    Some((ToolKind::from_dm(&tool.behaviour)?, tool.speed))
}

/// A welding tool's live state. Starts full and off (`starting_fuel`, `welding = FALSE`).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Welder {
    pub on: bool,
    pub fuel: f32,
    pub max_fuel: f32,
    /// `burned_fuel_for`, seconds.
    burned: f32,
}

impl Welder {
    pub fn new(max_fuel: f32) -> Self {
        Self {
            on: false,
            fuel: max_fuel,
            max_fuel,
            burned: 0.0,
        }
    }

    /// tg `tool_use_check`: lit, with `amount` fuel.
    pub fn usable(&self, amount: f32) -> bool {
        self.on && self.fuel > 0.0 && self.fuel >= amount
    }

    /// tg `process` of a lit welder over `secs`: one unit per [`TOOL_FUEL_BURN_INTERVAL`];
    /// `check_fuel` turns it off when empty. True when the fuel changed.
    pub fn burn(&mut self, secs: f32) -> bool {
        if !self.on {
            return false;
        }
        self.burned += secs;
        if self.burned < TOOL_FUEL_BURN_INTERVAL {
            return false;
        }
        self.use_fuel(1.0);
        true
    }

    /// tg `use(used)`: only while lit and with enough fuel; turns off when it runs dry.
    pub fn use_fuel(&mut self, used: f32) -> bool {
        if !self.usable(0.0) || self.fuel < used {
            return false;
        }
        if used > 0.0 {
            self.burned = 0.0;
        }
        self.fuel -= used;
        if self.fuel <= 0.0 {
            self.fuel = 0.0;
            self.on = false;
        }
        true
    }

    /// The fuel gauge overlay step of `update_overlays`: `ceil(fuel / max * 4) * 25`.
    pub fn gauge(&self) -> u32 {
        if self.max_fuel <= 0.0 {
            return 0;
        }
        ((self.fuel / self.max_fuel * 4.0).ceil() as u32) * 25
    }
}

/// An item stack's live amount (`/obj/item/stack::amount`). At 0 the stack is deleted.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stack {
    pub amount: u32,
}

/// A timed action in progress (tg `do_after`), on the acting mob.
#[derive(Component, Clone, Debug)]
pub struct DoAfter {
    pub client: Entity,
    pub target: Entity,
    pub target_pos: TilePos,
    pub user_pos: TilePos,
    /// The active hand's item when it started (tg `DO_AFTER_USER_HELD_ITEM` check).
    pub item: Option<Entity>,
    /// Welder fuel or stack amount the job needs at its end (`use_tool` `amount`).
    pub amount: u32,
    /// `Time::elapsed_secs_f64` it completes at.
    pub ends_at: f64,
    pub job: Job,
}

pub struct ToolsServerPlugin;

impl Plugin for ToolsServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                burn_welder_fuel,
                run_do_afters.run_if(any_with_component::<DoAfter>),
            )
                .chain()
                .after(WorldSystems::Movement)
                // A finished do_after can apply effects (cuffs): before their traits refresh.
                .before(crate::status_effects::StatusSystems),
        )
        .add_observer(add_tool_state)
        .add_observer(add_stack_state);
    }
}

fn add_tool_state(add: On<Add<DmTool>>, tools: Query<&DmTool>, mut commands: Commands) {
    let Ok(tool) = tools.get(add.entity) else {
        return;
    };
    if ToolKind::from_dm(&tool.behaviour) == Some(ToolKind::Welder) {
        let entity = add.entity;
        commands
            .entity(entity)
            .insert_if_new(Welder::new(tool.max_fuel));
        commands.queue(move |world: &mut World| refresh_welder(world, entity));
    }
}

fn add_stack_state(add: On<Add<DmStack>>, stacks: Query<&DmStack>, mut commands: Commands) {
    if let Ok(stack) = stacks.get(add.entity) {
        // A spawner that sets the amount itself (`spawn_stack`) wins.
        commands.entity(add.entity).insert_if_new(Stack {
            amount: stack.amount.max(1),
        });
    }
}

/// One line to one client.
pub fn tell(world: &mut World, client: Entity, span: Span, text: impl Into<String>) {
    world.write_message(SendChat {
        clients: vec![client],
        message: ChatMessage {
            channel: if matches!(span, Span::Warning | Span::Danger) {
                ChatChannel::Warning
            } else {
                ChatChannel::Info
            },
            segments: vec![ChatSegment::new(span, text)],
        },
    });
}

/// tg `attack_self` of the active hand's item, for the items that have one here. False when
/// the item has none (the slot click then does what it does for other items).
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(mut welder) = world.get_mut::<Welder>(item) else {
        return false;
    };
    // tg `switched_on`: lighting needs fuel.
    let text = if welder.on {
        welder.on = false;
        "Вы выключаете сварочный аппарат."
    } else if welder.fuel >= 1.0 {
        welder.on = true;
        "Вы включаете сварочный аппарат."
    } else {
        "Нет топлива!"
    };
    tell(world, client, Span::Notice, text);
    refresh_welder(world, item);
    // The in-hand sprite changes (`welder1`).
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.set_changed();
    }
    true
}

/// A lit welder's force, damage type, in-hand state and overlays (`switched_on`,
/// `update_icon_state`, `update_overlays`).
pub fn refresh_welder(world: &mut World, item: Entity) {
    let Some(welder) = world.get::<Welder>(item).cloned() else {
        return;
    };
    let base = world
        .get::<DmAtom>(item)
        .map(|a| (a.icon.clone(), a.icon_state.clone()));
    let overlays: Vec<Overlay> = match (base, world.get_resource::<IconIndex>()) {
        (Some((dmi, state)), Some(icons)) => {
            let gauge = icons.get(&dmi, &format!("{state}{}", welder.gauge()));
            let lit = welder
                .on
                .then(|| icons.get(&dmi, &format!("{state}-on")))
                .flatten();
            [gauge, lit]
                .into_iter()
                .flatten()
                .map(Overlay::new)
                .collect()
        }
        _ => Vec::new(),
    };
    if let Some(mut appearance) = world.get_mut::<Appearance>(item)
        && appearance.overlays != overlays
    {
        appearance.overlays = overlays;
    }
    // Live item vars tg changes on the item itself; combat and the in-hand sprite read them.
    if let Some(mut dm) = world.get_mut::<DmItem>(item) {
        let (force, damtype) = if welder.on {
            (WELDER_LIT_FORCE, "burn")
        } else {
            (WELDER_FORCE, "brute")
        };
        let inhand = dm.inhand_icon_state.trim_end_matches('1').to_owned();
        let inhand = if welder.on {
            format!("{inhand}1")
        } else {
            inhand
        };
        if dm.force != force || dm.damtype != damtype || dm.inhand_icon_state != inhand {
            dm.force = force;
            dm.damtype = damtype.to_owned();
            dm.inhand_icon_state = inhand;
        }
    }
}

/// Lit welders burn fuel (tg `process`, per second). Cost: one pass over the map's welders
/// (dozens) per tick.
fn burn_welder_fuel(
    time: Res<Time>,
    mut welders: Query<(Entity, &mut Welder, Option<&CarriedBy>)>,
    mut inventories: Query<&mut Inventory>,
    mut commands: Commands,
) {
    let secs = time.delta_secs();
    for (item, mut welder, carried) in &mut welders {
        if !welder.on {
            continue;
        }
        let gauge = welder.gauge();
        let was_on = welder.on;
        if welder.burn(secs) && (welder.gauge() != gauge || welder.on != was_on) {
            commands.queue(move |world: &mut World| refresh_welder(world, item));
            if let Some(mut inventory) = carried.and_then(|c| inventories.get_mut(c.0).ok()) {
                inventory.set_changed();
            }
        }
    }
}

/// The stack amount of `item` (1 for non-stacks).
pub fn stack_amount(world: &World, item: Entity) -> u32 {
    world.get::<Stack>(item).map_or(1, |s| s.amount)
}

/// tg `/obj/item/stack/use`: takes `used` from the stack, deletes it when empty. False when it
/// has fewer.
pub fn use_stack(world: &mut World, item: Entity, used: u32) -> bool {
    let Some(mut stack) = world.get_mut::<Stack>(item) else {
        return false;
    };
    if stack.amount < used {
        return false;
    }
    stack.amount -= used;
    if stack.amount == 0 {
        world.despawn(item);
    }
    true
}

/// tg `tool_start_check`/`tool_use_check` of the item for `amount`: a welder must be lit with
/// the fuel and heat, a stack must have the amount; other tools always pass.
pub fn tool_usable(world: &World, item: Entity, amount: u32, heat_required: f32) -> bool {
    if let Some(welder) = world.get::<Welder>(item) {
        let heat = world.get::<DmTool>(item).map_or(0.0, |t| t.heat);
        return welder.usable(amount as f32) && heat >= heat_required;
    }
    if world.get::<Stack>(item).is_some() {
        return stack_amount(world, item) >= amount;
    }
    amount == 0
}

/// The warning tg's `tool_use_check` prints when [`tool_usable`] fails.
pub fn unusable_message(world: &World, item: Entity, amount: u32) -> String {
    if let Some(welder) = world.get::<Welder>(item) {
        return if !welder.on || welder.fuel <= 0.0 {
            "Сварочный аппарат должен быть включён!".to_owned()
        } else {
            "Нужно больше топлива!".to_owned()
        };
    }
    format!("Нужно как минимум {amount} шт.!")
}

/// tg `use(amount)` at the end of `use_tool`: welder fuel or stack units.
pub fn consume(world: &mut World, item: Entity, amount: u32) -> bool {
    if amount == 0 {
        return true;
    }
    if let Some(mut welder) = world.get_mut::<Welder>(item) {
        let used = welder.use_fuel(amount as f32);
        refresh_welder(world, item);
        return used;
    }
    use_stack(world, item, amount)
}

/// Where a timed action starts: who, on what, with which item.
#[derive(Clone, Copy, Debug)]
pub struct Actor {
    pub client: Entity,
    pub mob: Entity,
    pub item: Option<Entity>,
}

/// tg `do_after(user, delay, target)`: starts `job` (`delay` deciseconds); `amount` is what the
/// job uses up at its end (tg `use_tool` `amount`). A zero delay completes at once. False when
/// the mob is already busy.
pub fn do_after(
    world: &mut World,
    actor: Actor,
    target: Entity,
    delay_ds: f32,
    amount: u32,
    job: Job,
) -> bool {
    let (Some(&user_pos), Some(&target_pos)) = (
        world.get::<TilePos>(actor.mob),
        world.get::<TilePos>(target),
    ) else {
        return false;
    };
    if world.get::<DoAfter>(actor.mob).is_some() {
        return false;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let pending = DoAfter {
        client: actor.client,
        target,
        target_pos,
        user_pos,
        item: actor.item,
        amount,
        ends_at: now + f64::from(delay_ds.max(0.0)) / 10.0,
        job,
    };
    if delay_ds <= 0.0 {
        crate::construction::complete(world, actor.mob, &pending);
    } else {
        world.entity_mut(actor.mob).insert(pending);
    }
    true
}

/// tg `use_tool`: the tool's start check (welder lit, stack amount), then a `do_after` of
/// `delay × toolspeed`. False when it did not start.
pub fn use_tool(
    world: &mut World,
    actor: Actor,
    target: Entity,
    delay_ds: f32,
    amount: u32,
    job: Job,
) -> bool {
    let Some(item) = actor.item else {
        return false;
    };
    if !tool_usable(world, item, amount, 0.0) {
        let text = unusable_message(world, item, amount);
        tell(world, actor.client, Span::Warning, text);
        return false;
    }
    let speed = world.get::<DmTool>(item).map_or(1.0, |t| t.speed);
    do_after(world, actor, target, delay_ds * speed, amount, job)
}

/// Every tick: interrupted actions end, finished ones complete.
fn run_do_afters(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &DoAfter)>();
    let pending: Vec<(Entity, DoAfter)> = query.iter(world).map(|(e, d)| (e, d.clone())).collect();
    for (mob, action) in pending {
        if !still_valid(world, mob, &action) {
            world.entity_mut(mob).remove::<DoAfter>();
            if let Job::Heretic(job) = action.job {
                crate::antag::heretic::server::interrupted(world, mob, &action, job);
            }
            continue;
        }
        if now >= action.ends_at {
            world.entity_mut(mob).remove::<DoAfter>();
            crate::construction::complete(world, mob, &action);
        }
    }
}

/// tg `do_after`'s per-tick checks plus `use_tool`'s `tool_check_callback`.
fn still_valid(world: &World, mob: Entity, action: &DoAfter) -> bool {
    let Ok(entity) = world.get_entity(mob) else {
        return false;
    };
    // tg `do_after` stops when the user is incapacitated (stuns, stamcrit).
    let in_place = entity.get::<TilePos>() == Some(&action.user_pos)
        && entity.get::<Stat>() == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, mob).incapacitated;
    let holding = entity.get::<Inventory>().and_then(Inventory::active_item) == action.item;
    let target = world.get::<TilePos>(action.target) == Some(&action.target_pos);
    let tool = action.item.is_none_or(|item| {
        world.get::<Welder>(item).is_none_or(|w| w.usable(0.0))
            && (world.get::<Stack>(item).is_none() || stack_amount(world, item) >= action.amount)
    });
    // `extra_checks` of the jobs that have them.
    let extra = match action.job {
        Job::Medical(crate::medical::MedJob::Defib { .. }) => {
            crate::medical::defib::is_wielded(world, mob)
        }
        // `check_mansus_grasp_offhand`.
        Job::Heretic(crate::antag::heretic::server::HereticJob::DrawRune) => {
            crate::antag::heretic::server::grasp_in_offhand(world, mob)
        }
        _ => true,
    };
    in_place && holding && target && tool && extra
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welder_burns_one_unit_per_five_seconds_and_goes_out() {
        let mut w = Welder::new(2.0);
        assert!(!w.burn(10.0), "an unlit welder burns nothing");
        w.on = true;
        assert_eq!(w.gauge(), 100);
        assert!(!w.burn(4.9));
        assert!(w.burn(0.1));
        assert_eq!(w.fuel, 1.0);
        assert_eq!(w.gauge(), 50);
        assert!(w.usable(1.0) && !w.usable(2.0));
        assert!(w.burn(5.0));
        assert_eq!((w.fuel, w.on), (0.0, false), "check_fuel turns it off");
        assert!(!w.use_fuel(0.0), "unlit: unusable");
        let mut w = Welder::new(20.0);
        w.on = true;
        assert!(w.use_fuel(2.0) && w.fuel == 18.0);
        assert!(!w.use_fuel(19.0));
        // ceil(18 / 20 * 4) * 25
        assert_eq!(w.gauge(), 100);
        w.fuel = 4.0;
        assert_eq!(w.gauge(), 25);
    }

    #[test]
    fn tool_behaviours_follow_tg_defines() {
        assert_eq!(ToolKind::from_dm("cutters"), Some(ToolKind::Wirecutters));
        assert_eq!(ToolKind::from_dm("welder"), Some(ToolKind::Welder));
        assert_eq!(ToolKind::from_dm("mining"), None);
    }
}
