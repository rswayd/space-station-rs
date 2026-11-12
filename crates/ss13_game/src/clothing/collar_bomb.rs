//! The collar bomb and its button (server), after tg `code/modules/clothing/neck/collar_bomb.dm`
//! and `code/datums/wires/collar_bomb.dm`.
//!
//! - Worn on the neck it cannot be taken off (`TRAIT_NODROP` while in the neck slot).
//! - Its big yellow button (pressed for 1.2 s) or a pulse of its one wire (a multitool in the
//!   wires window, an assembly on the wire) starts a 5 s countdown (`explosive_countdown`): a
//!   tick sound and the seconds left each second; at zero a human still wearing it takes 200
//!   brute to the head and loses it, otherwise it is a dud.
//! - The wires window opens with a wire tool or an assembly; the wearer cannot reach it.
//!   EMPs do not pulse it.
//! - The collar bomb box (`/obj/item/storage/box/collar_bomb`) holds a button linked to its
//!   collar; a collar without a button (the dead money kit's) only goes off by its wire.
// shortcut: no equip delays (`equip_delay_self` 6 s, `equip_delay_other` 8 s), no
// `cuffable_item`, no red screen flash; the countdown sounds where the collar is. Add with
// equip delays.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::chat::Span;
use ss13_tgui::server::AlwaysState;

use crate::assemblies::server::Assembly;
use crate::click_handlers::{AddClickHandler, ClickPhase};
use crate::health::{DamageType, Health, Stat};
use crate::human::HumanLook;
use crate::inventory::{CarriedBy, Inventory, NoDrop};
use crate::sound::{PlaySound, playsound};
use crate::storage::StoredIn;
use crate::tools::{Actor, ToolKind, tool_of};
use crate::wires::{WireHolder, WireSet, Wires};
use crate::zone::BodyZone;

pub const COLLAR: &str = "/obj/item/clothing/neck/collar_bomb";
pub const BUTTON: &str = "/obj/item/collar_bomb_button";
pub const BOX: &str = "/obj/item/storage/box/collar_bomb";
/// `explosive_countdown(ticks_left = 5)`.
const COUNTDOWN: u32 = 5;
/// The button's `do_after(user, 1.2 SECONDS)`.
const PRESS_DS: f32 = 12.0;
const TIMER_SOUND: &str = "sound/items/timer.ogg";
const SNAP_SOUND: &str = "sound/effects/snap.ogg";
const CLICK_SOUND: &str = "sound/machines/click.ogg";
/// `SFX_EXPLOSION`.
const EXPLOSION_SOUND: &str = "sound/effects/explosion/explosion1.ogg";
/// `brian.apply_damage(200, BRUTE, BODY_ZONE_HEAD)`.
const HEAD_DAMAGE: f32 = 200.0;

/// `/obj/item/clothing/neck/collar_bomb`: its `button`, `active` and the running countdown.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct CollarBomb {
    pub button: Option<Entity>,
    pub active: bool,
    pub ticks_left: u32,
    /// Game time of the next countdown step.
    pub next_tick: f64,
}

/// `/obj/item/collar_bomb_button`: its `collar`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct CollarButton {
    pub collar: Option<Entity>,
}

/// `/datum/wires/collar_bomb`: one wire, colours per collar (`randomize = TRUE`).
pub fn collar_set() -> WireSet {
    WireSet {
        key: COLLAR,
        proper_name: "Collar Bomb",
        wires: vec![crate::machine_wires::WIRE_ACTIVATE.to_owned()],
    }
}

pub(super) fn build(app: &mut App) {
    app.add_observer(add_parts)
        .add_systems(
            Update,
            (neck_grip, countdown).run_if(any_with_component::<CollarBomb>),
        )
        .add_click_handler(ClickPhase::AttackSelf, 3357, |w, c| {
            press(w, c.client, c.mob, c.target)
        })
        // Before the assemblies: a wire tool or an assembly opens the collar's wires.
        .add_click_handler(ClickPhase::OtherHand, 697, |w, c| {
            c.held
                .is_some_and(|held| open_wires(w, c.client, c.mob, held, c.target))
        })
        .add_click_handler(ClickPhase::HeldOnTarget, 497, |w, c| {
            c.held
                .is_some_and(|held| open_wires(w, c.client, c.mob, held, c.target))
        });
}

fn add_parts(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    let e = add.entity;
    if is_under(&item.path, COLLAR) {
        let set = collar_set();
        commands.entity(e).insert_if_new((
            CollarBomb::default(),
            Wires {
                holder: WireHolder::CollarBomb,
                proper_name: set.proper_name,
                colors: crate::wires::colors_for(e.to_bits(), &set),
                cut: Vec::new(),
            },
            // The window of a held collar follows `interactable` ([`wires_interactable`]).
            AlwaysState,
        ));
    } else if is_under(&item.path, BUTTON) {
        commands.entity(e).insert_if_new(CollarButton::default());
    }
}

/// `collar_bomb/PopulateContents`: the box's button goes with its collar.
pub fn populated(world: &mut World, storage: Entity) {
    let is_box = world.get::<DmItem>(storage).is_some_and(|i| i.path == BOX);
    if !is_box {
        return;
    }
    let inside = |world: &mut World, path: &str| {
        let mut q = world.query::<(Entity, &DmItem, &StoredIn)>();
        q.iter(world)
            .find(|(_, i, s)| s.0 == storage && is_under(&i.path, path))
            .map(|(e, ..)| e)
    };
    let (Some(button), Some(collar)) = (inside(world, BUTTON), inside(world, COLLAR)) else {
        return;
    };
    link(world, button, collar);
}

/// `collar_bomb/Initialize(mapload, button)`: `button.collar = src`.
pub fn link(world: &mut World, button: Entity, collar: Entity) {
    let mut collar_state = world.get::<CollarBomb>(collar).copied().unwrap_or_default();
    collar_state.button = Some(button);
    world.entity_mut(collar).insert(collar_state);
    world.entity_mut(button).insert(CollarButton {
        collar: Some(collar),
    });
}

/// The human wearing `collar` on the neck.
fn wearer(world: &World, collar: Entity) -> Option<Entity> {
    let mob = world.get::<CarriedBy>(collar)?.0;
    (world.get::<Inventory>(mob)?.neck == Some(collar)).then_some(mob)
}

/// `equipped`/`dropped`: `TRAIT_NODROP` while in the neck slot.
fn neck_grip(world: &mut World) {
    let mut q = world.query_filtered::<Entity, With<CollarBomb>>();
    let collars: Vec<Entity> = q.iter(world).collect();
    for collar in collars {
        let worn = wearer(world, collar).is_some();
        let gripping = world.get::<NoDrop>(collar).is_some();
        if worn && !gripping {
            world.entity_mut(collar).insert(NoDrop);
        } else if !worn && gripping {
            world.entity_mut(collar).remove::<NoDrop>();
        }
    }
}

/// Starts `explosive_countdown(5)` unless it already runs.
pub fn trigger(world: &mut World, collar: Entity) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(mut state) = world.get_mut::<CollarBomb>(collar) else {
        return false;
    };
    if state.active {
        return false;
    }
    state.active = true;
    state.ticks_left = COUNTDOWN;
    state.next_tick = now;
    if let Some(mob) = wearer(world, collar) {
        info!("collar bomb {collar:?} on {mob:?} triggered");
    }
    true
}

/// `explosive_countdown`: a step each second, the end at zero.
fn countdown(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &CollarBomb)>();
    let due: Vec<(Entity, CollarBomb)> = q
        .iter(world)
        .filter(|(_, c)| c.active && now >= c.next_tick)
        .map(|(e, c)| (e, *c))
        .collect();
    for (collar, state) in due {
        let at = crate::chem::map_pos(world, collar);
        if state.ticks_left > 0 {
            if let Some(at) = at {
                playsound(world, PlaySound::new(at, TIMER_SOUND, 30.0, false));
            }
            crate::balloon::balloon_alert_to_viewers(
                world,
                collar,
                &state.ticks_left.to_string(),
                None,
                &[],
            );
            if let Some(mut c) = world.get_mut::<CollarBomb>(collar) {
                c.ticks_left -= 1;
                c.next_tick = now + 1.0;
            }
            continue;
        }
        if let Some(at) = at {
            playsound(world, PlaySound::new(at, SNAP_SOUND, 75.0, true));
        }
        let victim = wearer(world, collar).filter(|&m| world.get::<HumanLook>(m).is_some());
        let Some(victim) = victim else {
            crate::balloon::balloon_alert_to_viewers(world, collar, "осечка...", None, &[]);
            if let Some(mut c) = world.get_mut::<CollarBomb>(collar) {
                c.active = false;
            }
            continue;
        };
        detonate(world, collar, victim);
    }
}

/// The end of the countdown on a wearer: the head goes, the collar with it.
fn detonate(world: &mut World, collar: Entity, victim: Entity) {
    let collar_name = ss13_shared::capitalize(&crate::chem::name_of(world, collar));
    let victim_name = crate::chem::name_of(world, victim);
    let text = format!("{collar_name} срабатывает, напрочь отрывая голову {victim_name}!");
    crate::chem::visible(world, victim, &text, &text, Span::Warning);
    if let Some(at) = crate::chem::map_pos(world, victim) {
        playsound(world, PlaySound::new(at, EXPLOSION_SOUND, 30.0, true));
    }
    if let (Some(mut health), Some(&stat)) = (
        world.get::<Health>(victim).cloned(),
        world.get::<Stat>(victim),
    ) {
        let mut new = stat;
        crate::health::apply_damage_zone(
            &mut health,
            &mut new,
            BodyZone::Head,
            DamageType::Brute,
            HEAD_DAMAGE,
        );
        world.entity_mut(victim).insert(health);
        if new != stat {
            world.entity_mut(victim).insert(new);
        }
    }
    crate::medical::bodyparts::dismember(world, victim, BodyZone::Head, false);
    info!("{victim:?} has been decapitated by collar bomb {collar:?}");
    if let Some(button) = world.get::<CollarBomb>(collar).and_then(|c| c.button)
        && let Some(mut b) = world.get_mut::<CollarButton>(button)
    {
        b.collar = None;
    }
    // Despawning empties the neck slot (`crate::inventory`'s `forget_carried`).
    world.despawn(collar);
}

/// `collar_bomb_button/attack_self`: the button is pressed for 1.2 s. True when `item` is one.
fn press(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world.get::<CollarButton>(item).is_none() {
        return false;
    }
    if world.get::<crate::tools::DoAfter>(mob).is_some() {
        return true;
    }
    crate::balloon::balloon_alert_to_viewers(world, item, "нажатие кнопки...", None, &[]);
    let actor = Actor {
        client,
        mob,
        item: Some(item),
    };
    let job = crate::construction::Job::Clothing(super::ClothingJob::CollarButton { button: item });
    crate::tools::do_after(world, actor, mob, PRESS_DS, 0, job);
    true
}

/// The end of the press: the click, then the countdown of a linked collar that is not running.
pub fn complete_press(world: &mut World, mob: Entity, button: Entity) {
    let held = world.get::<Inventory>(mob).is_some_and(|i| i.holds(button));
    if !held {
        return;
    }
    if let Some(at) = crate::chem::map_pos(world, mob) {
        playsound(world, PlaySound::new(at, CLICK_SOUND, 25.0, true));
    }
    let collar = world.get::<CollarButton>(button).and_then(|b| b.collar);
    if let Some(collar) = collar.filter(|&c| world.get_entity(c).is_ok()) {
        trigger(world, collar);
    }
}

/// `/datum/wires/collar_bomb/on_pulse`: the countdown.
pub fn wire_pulse(world: &mut World, collar: Entity) {
    trigger(world, collar);
}

/// `/datum/wires/collar_bomb/interactable`: not by its wearer; held, next to the user or on a
/// mob next to them.
pub fn wires_interactable(world: &World, collar: Entity, user: Entity) -> bool {
    if wearer(world, collar) == Some(user) {
        return false;
    }
    match world.get::<CarriedBy>(collar).map(|c| c.0) {
        Some(holder) => holder == user || crate::storage::in_reach(world, user, holder),
        None => crate::assemblies::server::within_reach(world, user, collar),
    }
}

/// `collar_bomb/item_interaction`: a wire tool (`is_wire_tool`) opens the wires window. True
/// when it was the collar's.
fn open_wires(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<CollarBomb>(target).is_none() || held == target {
        return false;
    }
    let wire_tool = matches!(
        tool_of(world, held).map(|t| t.0),
        Some(ToolKind::Wirecutters | ToolKind::Multitool)
    ) || world.get::<Assembly>(held).is_some();
    if !wire_tool {
        return false;
    }
    crate::wires::interact(world, client, mob, target);
    true
}

/// `collar_bomb/examine`: the panel hint, unless worn by the examiner.
pub fn examine_lines(world: &World, item: Entity, lines: &mut Vec<crate::examine::Line>) {
    if world.get::<CollarBomb>(item).is_some() && wearer(world, item).is_none() {
        lines.push((
            Span::Notice,
            "У него есть панель с <b>проводами</b>, с которой можно повозиться...".to_owned(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_wire() {
        let set = collar_set();
        assert_eq!(set.wires, vec!["Activate".to_owned()]);
        assert_eq!(crate::wires::colors_for(1, &set).len(), 1);
    }
}
