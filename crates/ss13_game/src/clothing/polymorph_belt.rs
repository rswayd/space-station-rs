//! The polymorphic field inverter (`code/modules/clothing/belts/polymorph_belt.dm`,
//! `/obj/item/polymorph_belt`).
//!
//! - A bioscrambler anomaly core goes in (3 s, `item_interaction`): the belt is active
//!   (`polybelt` sprite), the core is used up. `/functioning` starts active.
//! - The belt used on a living basic mob of organic sentience scans it in 5 s (`attack`): its
//!   type is stored, one at a time; humans and other complex mobs, smart mobs and the type
//!   already stored are refused.
//! - Worn on the belt, active and with a type stored, it gives the wearer the
//!   "Инверсия полиморфного поля" button (`/datum/action/cooldown/spell/shapeshift/
//!   polymorph_belt`): 3 s of crunching (`channel_time`), then the wearer turns into the stored
//!   mob (`crate::spells::shapeshift`); the same button on the form turns it back. 30 s cooldown
//!   after each change. Scanning a new type turns a wearer in the old form back first.
// shortcut: the shrinking animation of the channel and the pop after it and the mob biotype
// check (`MOB_HUMANOID`, `MOB_ROBOTIC`, ...: only the sentience
// type and basic mobs are checked) are missing; a form that ages into another type is not
// followed (`on_type_change`).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::mobs::SENTIENCE_ORGANIC;
use ss13_mapload::{DmItem, IconIndex};
use ss13_shared::chat::{ChatChannel, Span};
use ss13_tgui::server::{UiActed, UiSystems};

use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::balloon::balloon_alert;
use crate::construction::Job;
use crate::health::{DEFAULT_MESSAGE_RANGE, Stat};
use crate::inventory::{CarriedBy, Inventory};
use crate::mobs::BasicMob;
use crate::security::{name_of, show};
use crate::server::PlayerMob;
use crate::sound::{PlaySound, playsound};
use crate::spells::shapeshift::{Shapeshifted, do_shapeshift, unshift};
use crate::tools::{Actor, DoAfter, do_after};

pub const BELT: &str = "/obj/item/polymorph_belt";
const FUNCTIONING: &str = "/obj/item/polymorph_belt/functioning";
pub const BIOSCRAMBLER_CORE: &str = "/obj/item/assembly/signaler/anomaly/bioscrambler";
const BUTTON: &str = "polybelt:";
const BUTTON_NAME: &str = "Инверсия полиморфного поля";
const BUTTON_BACKGROUNDS: &str = "icons/mob/actions/backgrounds.dmi";
const INSERT_DS: f32 = 30.0;
const SCAN_DS: f32 = 50.0;
/// `channel_time`.
const CHANNEL_DS: f32 = 30.0;
/// `cooldown_time`.
const COOLDOWN_SECS: f64 = 30.0;

/// The belt's state.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct PolymorphBelt {
    /// `stored_mob_type` and its name.
    pub stored: Option<(String, String)>,
    pub active: bool,
    /// The action's `next_use_time`.
    pub ready_at: f64,
}

/// On a form the belt made: the belt (on the waiting caster).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BeltForm {
    pub belt: Entity,
}

/// The belt's do_afters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeltJob {
    Insert { belt: Entity },
    Scan { belt: Entity, target: Entity },
    Channel { belt: Entity },
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    app.add_click_handler(ClickPhase::HeldOnTarget, 489, |w, c| {
        c.held
            .is_some_and(|held| item_interaction(w, c.client, c.mob, held, c.target))
    })
    .add_click_handler(ClickPhase::OtherHand, 449, |w, c| {
        c.held
            .is_some_and(|held| item_interaction(w, c.client, c.mob, held, c.target))
    })
    .add_systems(
        Update,
        (init_belts, refresh_buttons)
            .chain()
            .after(crate::inventory::inventory_intents),
    )
    .add_systems(
        Update,
        trigger_buttons
            .in_set(UiSystems::Handle)
            .run_if(resource_exists::<Messages<UiActed<ActionButtonsUi>>>),
    );
}

fn init_belts(added: Query<(Entity, &DmItem), Added<DmItem>>, mut commands: Commands) {
    for (e, dm) in &added {
        if is_under(&dm.path, BELT) {
            commands.entity(e).insert(PolymorphBelt {
                active: is_under(&dm.path, FUNCTIONING),
                ..default()
            });
        }
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

/// `update_icon_state`: `polybelt` or `polybelt_inactive`, in hand and worn.
fn update_icon(world: &mut World, belt: Entity, active: bool) {
    let state = if active {
        "polybelt"
    } else {
        "polybelt_inactive"
    };
    super::toggles::set_states(world, belt, state, state.to_owned());
}

/// `item_interaction` with a core and `attack` with the belt.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let held_path = world.get::<DmItem>(held).map(|d| d.path.clone());
    let Some(held_path) = held_path else {
        return false;
    };
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    if is_under(&held_path, BIOSCRAMBLER_CORE) {
        let Some(belt) = world.get::<PolymorphBelt>(target) else {
            return false;
        };
        if belt.active {
            balloon_alert(world, mob, target, "ядро уже установлено!");
            return true;
        }
        balloon_alert(world, mob, target, "вставка...");
        let on = if world.get::<ss13_shared::TilePos>(target).is_some() {
            target
        } else {
            mob
        };
        do_after(
            world,
            actor,
            on,
            INSERT_DS,
            0,
            Job::Clothing(super::ClothingJob::Belt(BeltJob::Insert { belt: target })),
        );
        return true;
    }
    if world.get::<PolymorphBelt>(held).is_none() || target == mob {
        return false;
    }
    let Some(stat) = world.get::<Stat>(target).copied() else {
        return false;
    };
    if stat == Stat::Dead {
        return false;
    }
    // `isanimal_or_basicmob`.
    let Some(def) = world.get::<BasicMob>(target).map(|b| b.def.clone()) else {
        balloon_alert(world, mob, target, "сложная цель для оцифровки!");
        return true;
    };
    if def.sentience_type != SENTIENCE_ORGANIC {
        balloon_alert(world, mob, target, "слишком умная цель для оцифровки!");
        return true;
    }
    let stored = world
        .get::<PolymorphBelt>(held)
        .and_then(|b| b.stored.as_ref())
        .is_some_and(|(path, _)| *path == def.path);
    if stored {
        balloon_alert(world, mob, target, "уже просканировано!");
        return true;
    }
    if world.get::<DoAfter>(mob).is_some() {
        balloon_alert(world, mob, target, "занят!");
        return true;
    }
    balloon_alert(world, mob, target, "сканирование...");
    let (user, belt, what) = (
        name_of(world, mob),
        name_of(world, held),
        name_of(world, target),
    );
    if let Some(pos) = crate::explosions::turf_of(world, mob) {
        show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            (
                Span::Notice,
                format!("{user} начинает сканировать {what} через {belt}."),
            ),
            &[],
        );
    }
    do_after(
        world,
        actor,
        target,
        SCAN_DS,
        0,
        Job::Clothing(super::ClothingJob::Belt(BeltJob::Scan {
            belt: held,
            target,
        })),
    );
    true
}

/// The end of a [`BeltJob`], revalidated.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: BeltJob) {
    match job {
        BeltJob::Insert { belt } => {
            let Some(core) = action.item else {
                return;
            };
            let holds = world.get::<Inventory>(mob).is_some_and(|i| i.holds(core));
            let Some(state) = world.get::<PolymorphBelt>(belt) else {
                return;
            };
            if state.active {
                balloon_alert(world, mob, belt, "ядро уже установлено!");
                return;
            }
            if !holds || !crate::storage::in_reach(world, mob, belt) {
                return;
            }
            world.despawn(core);
            if let Some(mut state) = world.get_mut::<PolymorphBelt>(belt) {
                state.active = true;
            }
            update_icon(world, belt, true);
            if let Some(pos) = crate::explosions::turf_of(world, belt) {
                playsound(
                    world,
                    PlaySound::new(pos, "sound/machines/crate/crate_open.ogg", 50.0, false),
                );
            }
            touch_wearer(world, belt);
        }
        BeltJob::Scan { belt, target } => {
            let held = world.get::<Inventory>(mob).is_some_and(|i| i.holds(belt));
            let Some(path) = world.get::<BasicMob>(target).map(|b| b.def.path.clone()) else {
                return;
            };
            if !held || world.get::<Stat>(target) == Some(&Stat::Dead) {
                return;
            }
            let (user, belt_name, what) = (
                name_of(world, mob),
                name_of(world, belt),
                name_of(world, target),
            );
            if let Some(pos) = crate::explosions::turf_of(world, mob) {
                show(
                    world,
                    pos,
                    DEFAULT_MESSAGE_RANGE,
                    ChatChannel::Info,
                    (
                        Span::Notice,
                        format!("{user} просканировал {what} через {belt_name}."),
                    ),
                    &[],
                );
                playsound(
                    world,
                    PlaySound::new(pos, "sound/machines/ping.ogg", 50.0, false),
                );
            }
            // `update_type`: `unshift_owner` first.
            unshift_wearer(world, belt);
            let name = world
                .get::<BasicMob>(target)
                .map(|b| b.def.name.clone())
                .unwrap_or(what);
            if let Some(mut state) = world.get_mut::<PolymorphBelt>(belt) {
                state.stored = Some((path, name));
            }
            touch_wearer(world, belt);
        }
        BeltJob::Channel { belt } => {
            if !can_use(world, mob, belt) {
                return;
            }
            let name = name_of(world, mob);
            if let Some(pos) = crate::explosions::turf_of(world, mob) {
                show(
                    world,
                    pos,
                    DEFAULT_MESSAGE_RANGE,
                    ChatChannel::Info,
                    (
                        Span::Warning,
                        format!("Тело {name} перестраивается с ужасным хрустящим звуком!"),
                    ),
                    &[],
                );
                playsound(
                    world,
                    PlaySound::new(pos, "sound/effects/magic/demon_consume.ogg", 50.0, true),
                );
            }
            cast(world, mob, belt);
        }
    }
}

/// Re-runs the wearer's button refresh (the inventory "changed").
fn touch_wearer(world: &mut World, belt: Entity) {
    if let Some(mob) = world.get::<CarriedBy>(belt).map(|c| c.0) {
        crate::inventory::refresh_look(world, mob);
    }
}

/// The form a belt's wearer is in now, if any.
fn form_of(world: &mut World, belt: Entity) -> Option<Entity> {
    let mut q = world.query::<(Entity, &BeltForm)>();
    q.iter(world).find(|(_, f)| f.belt == belt).map(|(e, _)| e)
}

fn unshift_wearer(world: &mut World, belt: Entity) {
    if let Some(form) = form_of(world, belt) {
        unshift(world, form);
    }
}

/// tg `IsAvailable` + `item_action_slot_check`: the belt in the caster's belt slot, active, a
/// type stored, off cooldown, and `mob` is the caster or the belt's form of it.
fn can_use(world: &mut World, mob: Entity, belt: Entity) -> bool {
    let Some(state) = world.get::<PolymorphBelt>(belt).cloned() else {
        return false;
    };
    let caster = match world.get::<BeltForm>(mob) {
        Some(form) if form.belt == belt => world.get::<Shapeshifted>(mob).map(|s| s.caster),
        Some(_) => None,
        None => Some(mob),
    };
    let worn = caster.is_some_and(|c| {
        world
            .get::<Inventory>(c)
            .is_some_and(|i| i.belt == Some(belt))
    });
    worn && state.active
        && state.stored.is_some()
        && state.ready_at <= now(world)
        && world.get::<Stat>(mob) == Some(&Stat::Conscious)
}

/// `cast`: into the stored mob, or back; then the cooldown.
fn cast(world: &mut World, mob: Entity, belt: Entity) {
    let ready_at = now(world) + COOLDOWN_SECS;
    if world.get::<BeltForm>(mob).is_some() {
        unshift(world, mob);
    } else {
        let Some((path, _)) = world
            .get::<PolymorphBelt>(belt)
            .and_then(|b| b.stored.clone())
        else {
            return;
        };
        // tg `TRAIT_NO_TRANSFORM`.
        if crate::transform::is_transforming(world, mob) || world.get::<Shapeshifted>(mob).is_some()
        {
            return;
        }
        let Some(form) = do_shapeshift(world, mob, None, &path) else {
            return;
        };
        world.entity_mut(form).insert(BeltForm { belt });
    }
    if let Some(mut state) = world.get_mut::<PolymorphBelt>(belt) {
        state.ready_at = ready_at;
    }
    touch_wearer(world, belt);
}

/// The belt whose button `mob` should see: its own worn one, or the one that made its form.
fn button_belt(world: &World, mob: Entity) -> Option<Entity> {
    if let Some(form) = world.get::<BeltForm>(mob) {
        return Some(form.belt);
    }
    let belt = world.get::<Inventory>(mob)?.belt?;
    let state = world.get::<PolymorphBelt>(belt)?;
    (state.active && state.stored.is_some()).then_some(belt)
}

/// The button after the mob's other buttons, kept in step every tick: forms and their spells'
/// own refreshes may replace the row.
// Cost: one pass over the players a tick, a lookup each; the row is only written on a change.
fn refresh_buttons(world: &mut World) {
    let mut q = world.query_filtered::<Entity, With<PlayerMob>>();
    let mobs: Vec<Entity> = q.iter(world).collect();
    for mob in mobs {
        let belt = button_belt(world, mob);
        let current = world.get::<ActionButtons>(mob);
        let ours: Vec<(ActionButton, f64)> = match belt {
            Some(belt) => {
                let desc = world
                    .get::<PolymorphBelt>(belt)
                    .and_then(|b| b.stored.as_ref())
                    .map(|(_, name)| format!("{BUTTON_NAME}: {name}"))
                    .unwrap_or_else(|| BUTTON_NAME.to_owned());
                let icons = world.get_resource::<IconIndex>();
                vec![(
                    ActionButton {
                        id: format!("{BUTTON}{}", belt.to_bits()),
                        name: desc,
                        icon: world
                            .get::<ss13_shared::Appearance>(belt)
                            .map(|a| a.icon_state),
                        background: icons.and_then(|i| i.get(BUTTON_BACKGROUNDS, "bg_default")),
                        border: None,
                        available: true,
                        cooldown: 0.0,
                        active: false,
                        maptext: None,
                    },
                    world.get::<PolymorphBelt>(belt).map_or(0.0, |b| b.ready_at),
                )]
            }
            None => Vec::new(),
        };
        let had: Vec<(ActionButton, f64)> = current
            .map(|b| {
                b.0.iter()
                    .filter(|(b, _)| b.id.starts_with(BUTTON))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if had == ours {
            continue;
        }
        let mut buttons: Vec<(ActionButton, f64)> = current
            .map(|b| {
                b.0.iter()
                    .filter(|(b, _)| !b.id.starts_with(BUTTON))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        buttons.extend(ours);
        set_buttons(world, mob, buttons);
    }
}

/// The button pressed, checked now: `before_cast`'s 3 s channel on the caster or its form.
fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .filter(|a| a.act.starts_with(BUTTON))
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client) {
            continue;
        }
        let Some(belt) = button_belt(world, actor) else {
            continue;
        };
        if id != format!("{BUTTON}{}", belt.to_bits()) || !can_use(world, actor, belt) {
            continue;
        }
        if world.get::<DoAfter>(actor).is_some() {
            continue;
        }
        if let Some(pos) = crate::explosions::turf_of(world, actor) {
            playsound(
                world,
                PlaySound::new(pos, "sound/effects/wounds/crack1.ogg", 50.0, false),
            );
        }
        balloon_alert(world, actor, actor, "трансформация...");
        do_after(
            world,
            Actor {
                client,
                mob: actor,
                item: None,
            },
            actor,
            CHANNEL_DS,
            0,
            Job::Clothing(super::ClothingJob::Belt(BeltJob::Channel { belt })),
        );
    }
}

/// `examine`: the stored DNA, the missing core.
pub fn examine_lines(world: &World, item: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(state) = world.get::<PolymorphBelt>(item) else {
        return;
    };
    if let Some((_, name)) = &state.stored {
        lines.push((Span::Notice, format!("Содержит оцифрованное ДНК {name}.")));
    }
    if !state.active {
        lines.push((
            Span::Warning,
            "Для работы требуется аномальное ядро биоскремблер.".to_owned(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_button_needs_an_active_belt_with_a_type() {
        let mut world = World::new();
        let belt = world.spawn(PolymorphBelt::default()).id();
        let mob = world
            .spawn(Inventory {
                belt: Some(belt),
                ..default()
            })
            .id();
        assert_eq!(button_belt(&world, mob), None);
        world.get_mut::<PolymorphBelt>(belt).unwrap().active = true;
        assert_eq!(button_belt(&world, mob), None);
        world.get_mut::<PolymorphBelt>(belt).unwrap().stored =
            Some(("/mob/living/basic/cockroach".into(), "cockroach".into()));
        assert_eq!(button_belt(&world, mob), Some(belt));
        // A form sees its belt's button.
        let form = world.spawn(BeltForm { belt }).id();
        assert_eq!(button_belt(&world, form), Some(belt));
    }

    #[test]
    fn examine_tells_the_missing_core() {
        let mut world = World::new();
        let belt = world.spawn(PolymorphBelt::default()).id();
        let mut lines = Vec::new();
        examine_lines(&world, belt, &mut lines);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].0, Span::Warning);
    }
}
