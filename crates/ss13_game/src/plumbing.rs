//! Sinks (server), after tg `code/game/objects/structures/water_structures/sink.dm`.
//!
//! A sink holds `capacity` units of its `dispensedreagent` (water; a gas station's plasma) and,
//! with a water recycler, refills `reclaim_rate` units a second.
//! - An empty hand washes the hands (or the face, aiming at the head) in 4 s for 5 units: worn
//!   gloves or the hands lose their blood and prints (`wash_hands`, `CLEAN_WASH`).
//! - A reagent container fills from it (left click, `crate::chem`'s drain: the sink is a
//!   drainable holder) or empties down the drain (right click); a mop gets 5 units.
//! - Any other item (out of combat mode) is washed in 4 s for 5 units.
//! - A wrench takes it apart (its stack and the recycler), a crowbar pries the recycler out.
//! - "Someone's already washing here": one wash at a time per sink.
// shortcut: no washing the face clean of makeup (no lipstick), no shocking yourself with an
// active baton, no reagents' touch effects on the washer (`expose` is not ported for mobs), no
// pickle jars, no installing a recycler back, no sink frames, no plumbing ducts; showers,
// toilets, urinals and water sources are not ported.
// Cost: one pass over the sinks below capacity per tick (a few dozen per map).

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmObj};
use ss13_reagents::{Holder, Owner, flags};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::protocol::{ClickModifiers, InventorySlot};

use crate::chem::{Chem, Reagents};
use crate::construction::Job;
use crate::health::ZoneSelected;
use crate::inventory::Inventory;
use crate::medical::{name_of, visible};
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, DoAfter, ToolKind, do_after, tell, tool_of};
use crate::zone::BodyZone;

const SINK: &str = "/obj/structure/sink";
const MOP: &str = "/obj/item/mop";
const WATER_RECYCLER: &str = "/obj/item/stock_parts/water_recycler";
const FAUCET: &str = "sound/machines/sink-faucet.ogg";
const SLOSH: &str = "sound/effects/slosh.ogg";
const CLICK: &str = "sound/machines/click.ogg";
/// `do_after(user, 4 SECONDS, target = src)`, deciseconds.
const WASH_DS: f32 = 40.0;
/// What one wash uses.
const WASH_UNITS: f32 = 5.0;

/// `/obj/structure/sink`: `has_water_reclaimer`, `reclaim_rate`, `dispensedreagent`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Sink {
    pub reclaimer: bool,
    pub reclaim_rate: f32,
    pub reagent: String,
}

/// The `do_after`s of a sink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SinkJob {
    /// The washer's hands, or its face.
    Hands { face: bool },
    /// The item in the washer's hand.
    Item,
}

pub struct PlumbingPlugin;

impl Plugin for PlumbingPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `attack_hand` of a sink: washing the hands or face.
        app.add_click_handler(ClickPhase::EmptyHand, 3100, |w, c| {
            crate::plumbing::interact(w, c.client, c.mob, c.target)
        });
        // tg `sink/item_interaction(_secondary)`: washing, emptying, a mop wetted.
        app.add_click_handler(ClickPhase::HeldOnTarget, 3400, |w, c| {
            c.held.is_some_and(|h| {
                crate::plumbing::item_interaction(w, c.client, c.mob, h, c.target, c.modifiers)
            })
        });
        app.add_observer(add_sink)
            .add_systems(Update, reclaim_water);
    }
}

fn is_sink(world: &World, e: Entity) -> bool {
    world.get::<Sink>(e).is_some()
}

/// `sink/Initialize`: `create_reagents(capacity, NO_REACT)`, full with a recycler.
fn add_sink(
    add: On<Add<DmObj>>,
    objs: Query<&DmObj>,
    chem: Option<Res<Chem>>,
    mut commands: Commands,
) {
    let Ok(dm) = objs.get(add.entity) else {
        return;
    };
    if !is_under(&dm.path, SINK) {
        return;
    }
    let owner = Owner {
        path: dm.path.clone(),
        living: false,
    };
    // Drainable: a held container's `interact_with_atom` fills from it (`crate::chem`).
    let mut holder = Holder::new(dm.capacity, flags::NO_REACT | flags::DRAINABLE, owner);
    if dm.water_reclaimer
        && let Some(chem) = chem
        && let Some(id) = chem.0.id(&dm.dispensed_reagent)
    {
        holder.add_reagent(
            &chem.0,
            id,
            dm.capacity,
            ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
            None,
            None,
            true,
            false,
        );
    }
    commands.entity(add.entity).insert((
        Sink {
            reclaimer: dm.water_reclaimer,
            reclaim_rate: dm.reclaim_rate,
            reagent: dm.dispensed_reagent.clone(),
        },
        Reagents(holder),
    ));
}

/// `sink/process(seconds_per_tick)`: the recycler tops the sink up.
fn reclaim_water(
    time: Res<Time>,
    chem: Option<Res<Chem>>,
    mut sinks: Query<(&Sink, &mut Reagents)>,
) {
    let Some(chem) = chem else {
        return;
    };
    let secs = time.delta_secs();
    for (sink, mut reagents) in &mut sinks {
        if !sink.reclaimer || reagents.0.holder_full() {
            continue;
        }
        let Some(id) = chem.0.id(&sink.reagent) else {
            continue;
        };
        reagents.0.add_reagent(
            &chem.0,
            id,
            sink.reclaim_rate * secs,
            ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
            None,
            None,
            true,
            false,
        );
    }
}

fn volume(world: &World, sink: Entity) -> f32 {
    world
        .get::<Reagents>(sink)
        .map_or(0.0, |r| r.0.total_volume)
}

/// `busy`: someone's wash at this sink is under way.
fn busy(world: &mut World, sink: Entity) -> bool {
    world
        .query::<&DoAfter>()
        .iter(world)
        .any(|d| d.target == sink && matches!(d.job, Job::Sink(_)))
}

fn sound(world: &mut World, sink: Entity, file: &str, vol: f32) {
    if let Some(&pos) = world.get::<TilePos>(sink) {
        playsound(world, PlaySound::new(pos, file, vol, true));
    }
}

/// `sink/attack_hand`: washing the hands, or the face when aiming at the head. True when
/// `target` is a sink.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if !is_sink(world, target) {
        return false;
    }
    // `iscarbon(user)`.
    if world.get::<crate::chem::MobReagents>(mob).is_none() {
        return true;
    }
    if volume(world, target) < WASH_UNITS {
        tell(world, client, Span::Warning, "Раковина пуста!");
        return true;
    }
    if busy(world, target) {
        tell(world, client, Span::Warning, "Здесь уже кто-то моется!");
        return true;
    }
    let face = world
        .get::<ZoneSelected>(mob)
        .is_some_and(|z| z.0.check_zone() == BodyZone::Head);
    sound(world, target, FAUCET, 50.0);
    let actor = Actor {
        client,
        mob,
        item: None,
    };
    if do_after(
        world,
        actor,
        target,
        WASH_DS,
        0,
        Job::Sink(SinkJob::Hands { face }),
    ) {
        let user = name_of(world, mob);
        let what = if face { "лицо" } else { "руки" };
        visible(
            world,
            mob,
            &format!("Вы начинаете мыть {what}..."),
            &format!("{user} начинает мыть {what}..."),
            Span::Notice,
        );
    }
    true
}

/// `sink/item_interaction(_secondary)` of `held`. True when the click was the sink's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    if !is_sink(world, target) {
        return false;
    }
    let path = world
        .get::<DmItem>(held)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    let container = is_under(&path, crate::chem::CONTAINER_ROOT);
    let tool = tool_of(world, held).map(|(kind, _)| kind);
    if !modifiers.right && tool == Some(ToolKind::Wrench) {
        // `wrench_act`: taken apart at once.
        sound(world, target, "sound/items/tools/ratchet.ogg", 50.0);
        crate::obj_damage::deconstruct(world, target, true);
        return true;
    }
    if !modifiers.right && tool == Some(ToolKind::Crowbar) {
        crowbar(world, client, target);
        return true;
    }
    if busy(world, target) {
        tell(world, client, Span::Warning, "Здесь уже кто-то моется!");
        return true;
    }
    if modifiers.right {
        // `item_interaction_secondary`: a container (or a mop) is emptied down the drain.
        if !container && !is_under(&path, MOP) {
            return false;
        }
        let Some(mut reagents) = world.get_mut::<Reagents>(held) else {
            return true;
        };
        if reagents.0.total_volume <= 0.0 {
            tell(world, client, Span::Warning, "Уже пусто!");
            return true;
        }
        reagents.0.clear();
        sound(world, target, SLOSH, 25.0);
        let (user, what, sink) = (
            name_of(world, mob),
            name_of(world, held),
            name_of(world, target),
        );
        visible(
            world,
            mob,
            &format!("Вы выливаете {what} в {sink}, смывая содержимое в слив."),
            &format!("{user} выливает {what} в {sink}."),
            Span::Notice,
        );
        return true;
    }
    if container {
        // Filled by `crate::chem` (the sink is drainable).
        return false;
    }
    if is_under(&path, MOP) {
        let name = name_of(world, target);
        if volume(world, target) <= 0.0 {
            tell(world, client, Span::Notice, format!("{name} пуста."));
            return true;
        }
        crate::chem::transfer(world, target, held, WASH_UNITS);
        sound(world, target, SLOSH, 25.0);
        let what = name_of(world, held);
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы смачиваете {what} в {name}."),
        );
        return true;
    }
    if world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
    {
        return false;
    }
    if volume(world, target) < WASH_UNITS {
        tell(world, client, Span::Warning, "Раковина пуста!");
        return true;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    if do_after(world, actor, target, WASH_DS, 0, Job::Sink(SinkJob::Item)) {
        let what = name_of(world, held);
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы начинаете мыть {what}..."),
        );
        sound(world, target, FAUCET, 50.0);
    }
    true
}

/// `sink/crowbar_act`: the recycler out.
fn crowbar(world: &mut World, client: Entity, sink: Entity) {
    let Some(reclaimer) = world.get::<Sink>(sink).map(|s| s.reclaimer) else {
        return;
    };
    if !reclaimer {
        tell(world, client, Span::Warning, "Здесь нет утилизатора воды.");
        return;
    }
    if let Some(mut s) = world.get_mut::<Sink>(sink) {
        s.reclaimer = false;
    }
    sound(world, sink, CLICK, 20.0);
    if let Some(&pos) = world.get::<TilePos>(sink) {
        crate::construction::spawn_obj(world, WATER_RECYCLER, pos);
    }
    let name = name_of(world, sink);
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы снимаете утилизатор воды с {name}."),
    );
}

/// `atom_deconstruct` of a sink: its stack and its recycler. True when `dm` is a sink.
pub(crate) fn deconstruct(world: &mut World, sink: Entity, dm: &DmObj, pos: TilePos) -> bool {
    if !is_under(&dm.path, SINK) {
        return false;
    }
    if !dm.build_stack.is_empty() && dm.build_stack_amount > 0 {
        crate::construction::spawn_stack(world, &dm.build_stack, pos, dm.build_stack_amount);
    }
    if world.get::<Sink>(sink).is_some_and(|s| s.reclaimer) {
        crate::construction::spawn_obj(world, WATER_RECYCLER, pos);
    }
    true
}

/// End of a [`SinkJob`] (`crate::tools` checked nobody moved): 5 units go, then the wash.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: SinkJob) {
    let sink = action.target;
    if volume(world, sink) < WASH_UNITS {
        tell(world, action.client, Span::Warning, "Раковина пуста!");
        return;
    }
    if let Some(mut reagents) = world.get_mut::<Reagents>(sink) {
        reagents.0.remove_all(WASH_UNITS);
    }
    let (user, name) = (name_of(world, mob), name_of(world, sink));
    match job {
        SinkJob::Hands { face } => {
            if !face {
                // `wash_hands`: the gloves, else the hands.
                let gloves = world
                    .get::<Inventory>(mob)
                    .and_then(|i| i.slot(InventorySlot::Gloves));
                crate::janitor::wash(world, gloves.unwrap_or(mob), crate::janitor::CLEAN_WASH);
            }
            let what = if face { "лицо" } else { "руки" };
            visible(
                world,
                mob,
                &format!("Вы моете {what} в {name}."),
                &format!("{user} моет {what} в {name}."),
                Span::Notice,
            );
        }
        SinkJob::Item => {
            let Some(item) = action.item else {
                return;
            };
            crate::janitor::wash(world, item, crate::janitor::CLEAN_WASH);
            let what = name_of(world, item);
            visible(
                world,
                mob,
                &format!("Вы моете {what} в {name}."),
                &format!("{user} моет {what} в {name}."),
                Span::Notice,
            );
        }
    }
}

/// tg `sink/examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(sink) = world.get::<Sink>(target) else {
        return;
    };
    if sink.reclaimer {
        lines.push((
            Span::Notice,
            "Установлен утилизатор воды. Похоже, его можно поддеть ломом.".to_owned(),
        ));
    }
    if let Some(r) = world.get::<Reagents>(target) {
        lines.push((
            Span::Notice,
            format!(
                "Осталось жидкости: {}/{}.",
                r.0.total_volume.floor(),
                r.0.maximum_volume.floor()
            ),
        ));
    }
    lines.push((
        Span::Notice,
        "Контейнер можно опустошить в слив правым кликом.".to_owned(),
    ));
}
