//! Building a turret from its frame (server), after tg `/obj/machinery/porta_turret_construct`
//! (`portable_turret_construct.dm`).
//!
//! A frame ([`TurretFrame`], made from iron through the stack recipes) goes through tg's steps:
//! wrench (bolted), 2 iron (welder takes it back out), wrench, an energy gun (an empty hand takes
//! it out), a prox sensor (likewise), screwdriver (reversible), 2 iron (a crowbar pries it
//! off), welder: the finished turret (`setup(installed_gun)`, unlocked) replaces the frame. The
//! crowbar takes an unbolted frame apart. Lasertag guns make a lasertag turret of that team.
//!
//! A pen renames the frame (`UNIQUE_RENAME | RENAME_NO_DESC`, `tool_renaming`): the finished
//! turret takes that name (`finish_name`); "Reset" gives both their own back.
//!
//! A broken turret's crowbar salvage ends here too ([`TurretBuildJob::Salvage`]).
// The pen rename is the frame's own: tg's generic `tool_renaming` of every `UNIQUE_RENAME`
// object belongs to the items group.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::guns::install_gun;
use ss13_mapload::{DmGun, DmItem, DmTurret};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::server::{Turret, setup_turret};
use crate::construction::{Job, spawn_obj, spawn_stack};
use crate::inventory::no_drop;
use crate::obj_damage::{set_anchored, set_icon_state};
use crate::sound::{PlaySound, playsound};
use crate::storage::{StoredIn, put_inside, take_out};
use crate::tools::{Actor, ToolKind, tell, tool_of, use_stack, use_tool};

pub const FRAME: &str = "/obj/machinery/porta_turret_construct";
const IRON: &str = "/obj/item/stack/sheet/iron";
const PROX_SENSOR: &str = "/obj/item/assembly/prox_sensor";
const GUN_ENERGY: &str = "/obj/item/gun/energy";
/// `gun_flags = TURRET_INCOMPATIBLE`.
const DUELING_GUN: &str = "/obj/item/gun/energy/dueling";
const BLUETAG_GUN: &str = "/obj/item/gun/energy/laser/bluetag";
const REDTAG_GUN: &str = "/obj/item/gun/energy/laser/redtag";
const TURRET: &str = "/obj/machinery/porta_turret";
const LASERTAG_RED: &str = "/obj/machinery/porta_turret/lasertag/red";
const LASERTAG_BLUE: &str = "/obj/machinery/porta_turret/lasertag/blue";
const PEN: &str = "/obj/item/pen";
/// `MAX_NAME_LEN`.
const MAX_NAME_LEN: u32 = 42;
const OPTION_RENAME: &str = "Rename";
const OPTION_RESET: &str = "Reset";

/// `PTURRET_*`.
const UNSECURED: u8 = 0;
const BOLTED: u8 = 1;
const START_INTERNAL_ARMOUR: u8 = 2;
const INTERNAL_ARMOUR_ON: u8 = 3;
const GUN_EQUIPPED: u8 = 4;
const SENSORS_ON: u8 = 5;
const CLOSED: u8 = 6;
const START_EXTERNAL_ARMOUR: u8 = 7;

/// `use_tool` of the two welder steps, deciseconds and fuel.
const REMOVE_ARMOUR_DS: f32 = 20.0;
const WELD_ARMOUR_DS: f32 = 30.0;
const WELD_FUEL: u32 = 5;
const SHEETS: u32 = 2;
const FRAME_SHEETS: u32 = 5;

const WRENCH: &str = "sound/items/tools/ratchet.ogg";
const CROWBAR: &str = "sound/items/tools/crowbar.ogg";
const SCREWDRIVER: &str = "sound/items/tools/screwdriver.ogg";

/// `/obj/machinery/porta_turret_construct` live state (`build_step`, `installed_gun`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TurretFrame {
    pub step: u8,
    pub gun: Option<Entity>,
}

/// The welder steps of a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurretBuildJob {
    /// The interior armour comes out.
    RemoveInterior,
    /// The exterior armour is welded: the turret is done.
    Finish,
    /// A crowbar on a broken turret (`porta_turret/crowbar_act`, `super::server`).
    Salvage,
}

/// The frame's pen name (`finish_name`) and the name it had before.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct FinishName {
    pub name: String,
    pub original: String,
}

/// On a prompt: the frame being renamed, and whether the name itself is asked.
#[derive(Component, Clone, Copy, Debug)]
struct RenameAsk {
    frame: Entity,
    naming: bool,
}

/// The rename prompts of [`item_interaction`] answered, after their sleep: the frame is still
/// in reach.
pub fn rename_answers(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<ss13_tgui::prompt::PromptAnswered>>,
) {
    use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};
    let answered: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in answered {
        let Some(RenameAsk { frame, naming }) = world.get::<RenameAsk>(prompt).copied() else {
            continue;
        };
        if world.get::<TurretFrame>(frame).is_none() || !crate::storage::in_reach(world, mob, frame)
        {
            continue;
        }
        let current = crate::examine::name_of(world, frame);
        match (naming, answer) {
            (false, Some(PromptAnswer::Item(0))) => {
                let prompt = ask_world(
                    world,
                    Ask::new(
                        client,
                        mob,
                        "Object Name",
                        format!("What do you want to name {current}?"),
                        PromptKind::Text {
                            default: current,
                            max_length: MAX_NAME_LEN,
                            multiline: false,
                        },
                    ),
                );
                world.entity_mut(prompt).insert(RenameAsk {
                    frame,
                    naming: true,
                });
            }
            (false, Some(PromptAnswer::Item(1))) => {
                if let Some(old) = world.get::<FinishName>(frame).cloned() {
                    set_name(world, frame, &old.original);
                    world.entity_mut(frame).remove::<FinishName>();
                }
                tell(
                    world,
                    client,
                    Span::Notice,
                    format!("You have successfully reset {current}'s name."),
                );
            }
            (true, Some(PromptAnswer::Text(text))) => {
                let input = crate::records::console::sanitize(&text, MAX_NAME_LEN as usize);
                if input.is_empty() || input == current {
                    tell(
                        world,
                        client,
                        Span::Notice,
                        format!("You changed {current} to... well... {current}."),
                    );
                    continue;
                }
                let original = world
                    .get::<FinishName>(frame)
                    .map_or_else(|| current.clone(), |f| f.original.clone());
                world.entity_mut(frame).insert(FinishName {
                    name: input.clone(),
                    original,
                });
                set_name(world, frame, &input);
                tell(
                    world,
                    client,
                    Span::Notice,
                    format!("You have successfully renamed the {current} to {input}."),
                );
            }
            _ => {}
        }
    }
}

/// The atom's shown name, its description kept.
fn set_name(world: &mut World, e: Entity, name: &str) {
    let desc = world
        .get::<ss13_mapload::DmName>(e)
        .map(|n| n.desc.clone())
        .unwrap_or_default();
    world.entity_mut(e).insert(ss13_mapload::DmName {
        name: name.to_owned(),
        desc,
    });
}

fn frame_of(world: &mut World, target: Entity) -> Option<TurretFrame> {
    if let Some(&f) = world.get::<TurretFrame>(target) {
        return Some(f);
    }
    if !crate::construction::path_of(world, target).is_some_and(|p| is_under(&p, FRAME)) {
        return None;
    }
    world.entity_mut(target).insert(TurretFrame::default());
    Some(TurretFrame::default())
}

fn set_step(world: &mut World, frame: Entity, step: u8, gun: Option<Entity>) {
    world.entity_mut(frame).insert(TurretFrame { step, gun });
}

fn tool_sound(world: &mut World, frame: Entity, sound: &str, vol: f32) {
    if let Some(&pos) = world.get::<TilePos>(frame) {
        playsound(world, PlaySound::new(pos, sound, vol, true));
    }
}

/// `item_interaction`, `wrench_act`, `crowbar_act`, `welder_act`, `screwdriver_act`. True when
/// the click was the frame's (a tool that does nothing here falls through to a plain attack).
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(frame) = frame_of(world, target) else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(target) else {
        return false;
    };
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    if let Some((kind, _)) = tool_of(world, held) {
        match kind {
            ToolKind::Wrench => return wrench(world, client, target, frame),
            ToolKind::Crowbar => return crowbar(world, client, target, frame, pos),
            ToolKind::Screwdriver => return screwdriver(world, client, target, frame),
            ToolKind::Welder => return welder(world, actor, target, frame),
            _ => {}
        }
    }
    let path = world
        .get::<DmItem>(held)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    // `tool_renaming`: rename or reset (`RENAME_NO_DESC`: no description).
    if is_under(&path, PEN) {
        let prompt = ss13_tgui::prompt::ask_world(
            world,
            ss13_tgui::prompt::Ask::new(
                client,
                mob,
                "Customization",
                "What would you like to edit?",
                ss13_tgui::prompt::PromptKind::List {
                    items: vec![OPTION_RENAME.to_owned(), OPTION_RESET.to_owned()],
                    default: None,
                },
            ),
        );
        world.entity_mut(prompt).insert(RenameAsk {
            frame: target,
            naming: false,
        });
        return true;
    }
    match frame.step {
        BOLTED if is_under(&path, IRON) => sheets(
            world,
            client,
            held,
            target,
            frame,
            START_INTERNAL_ARMOUR,
            "внутреннюю",
        ),
        CLOSED if is_under(&path, IRON) => sheets(
            world,
            client,
            held,
            target,
            frame,
            START_EXTERNAL_ARMOUR,
            "внешнюю",
        ),
        INTERNAL_ARMOUR_ON if is_under(&path, GUN_ENERGY) => {
            if is_under(&path, DUELING_GUN) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы не думаете, что будет правильно ставить это в турель.",
                );
                return true;
            }
            if no_drop(world, held) {
                return true;
            }
            take_from_hands(world, mob, held);
            put_inside(world, held, target);
            set_step(world, target, GUN_EQUIPPED, Some(held));
            tell(
                world,
                client,
                Span::Notice,
                "Вы устанавливаете оружие в турель.",
            );
            true
        }
        GUN_EQUIPPED if is_under(&path, PROX_SENSOR) => {
            if no_drop(world, held) {
                return true;
            }
            world.entity_mut(held).despawn();
            set_step(world, target, SENSORS_ON, frame.gun);
            tell(
                world,
                client,
                Span::Notice,
                "Вы устанавливаете датчик движения в турель.",
            );
            true
        }
        _ => false,
    }
}

/// Out of the mob's hands, whichever holds it.
fn take_from_hands(world: &mut World, mob: Entity, item: Entity) {
    if let Some(mut inventory) = world.get_mut::<crate::inventory::Inventory>(mob) {
        for hand in inventory.hands.iter_mut() {
            if *hand == Some(item) {
                *hand = None;
            }
        }
    }
}

fn sheets(
    world: &mut World,
    client: Entity,
    held: Entity,
    target: Entity,
    frame: TurretFrame,
    step: u8,
    which: &str,
) -> bool {
    if !use_stack(world, held, SHEETS) {
        tell(
            world,
            client,
            Span::Warning,
            "Нужно два листа железа, чтобы продолжить сборку!",
        );
        return true;
    }
    set_step(world, target, step, frame.gun);
    if step == START_INTERNAL_ARMOUR {
        set_icon_state(world, target, "turret_frame2");
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы добавляете {which} броню к раме."),
    );
    true
}

fn wrench(world: &mut World, client: Entity, target: Entity, frame: TurretFrame) -> bool {
    let (step, vol, text) = match frame.step {
        UNSECURED => (BOLTED, 100.0, "Вы затягиваете внешние болты."),
        BOLTED => (UNSECURED, 75.0, "Вы откручиваете внешние болты."),
        START_INTERNAL_ARMOUR => (
            INTERNAL_ARMOUR_ON,
            100.0,
            "Вы прикручиваете броню на место.",
        ),
        INTERNAL_ARMOUR_ON => (START_INTERNAL_ARMOUR, 100.0, "Вы откручиваете болты брони."),
        _ => return false,
    };
    tool_sound(world, target, WRENCH, vol);
    set_step(world, target, step, frame.gun);
    if frame.step == UNSECURED {
        set_anchored(world, target, true);
    } else if frame.step == BOLTED {
        set_anchored(world, target, false);
    }
    tell(world, client, Span::Notice, text);
    true
}

fn crowbar(
    world: &mut World,
    client: Entity,
    target: Entity,
    frame: TurretFrame,
    pos: TilePos,
) -> bool {
    match frame.step {
        UNSECURED => {
            tool_sound(world, target, CROWBAR, 75.0);
            tell(world, client, Span::Notice, "Вы разбираете раму турели.");
            spawn_stack(world, IRON, pos, FRAME_SHEETS);
            world.entity_mut(target).despawn();
            crate::construction::tile_changed(world, pos);
            true
        }
        START_EXTERNAL_ARMOUR => {
            tool_sound(world, target, CROWBAR, 75.0);
            tell(
                world,
                client,
                Span::Notice,
                "Вы снимаете внешнюю броню турели.",
            );
            spawn_stack(world, IRON, pos, SHEETS);
            set_step(world, target, CLOSED, frame.gun);
            true
        }
        _ => false,
    }
}

fn screwdriver(world: &mut World, client: Entity, target: Entity, frame: TurretFrame) -> bool {
    let (step, text) = match frame.step {
        SENSORS_ON => (CLOSED, "Вы закрываете внутренний люк."),
        CLOSED => (SENSORS_ON, "Вы открываете внутренний люк."),
        _ => return false,
    };
    tool_sound(world, target, SCREWDRIVER, 100.0);
    set_step(world, target, step, frame.gun);
    tell(world, client, Span::Notice, text);
    true
}

fn welder(world: &mut World, actor: Actor, target: Entity, frame: TurretFrame) -> bool {
    let (delay, job, text) = match frame.step {
        START_INTERNAL_ARMOUR => (
            REMOVE_ARMOUR_DS,
            TurretBuildJob::RemoveInterior,
            "Вы начинаете снимать внутреннюю броню турели...",
        ),
        START_EXTERNAL_ARMOUR => (
            WELD_ARMOUR_DS,
            TurretBuildJob::Finish,
            "Вы начинаете приваривать броню турели...",
        ),
        _ => return false,
    };
    if use_tool(
        world,
        actor,
        target,
        delay,
        WELD_FUEL,
        Job::TurretBuild(job),
    ) {
        tell(world, actor.client, Span::Notice, text);
    }
    true
}

/// End of a welder step; the frame's state is checked again here.
pub fn complete(world: &mut World, action: &crate::tools::DoAfter, job: TurretBuildJob) {
    let target = action.target;
    if job == TurretBuildJob::Salvage {
        super::server::finish_salvage(world, action.client, target);
        return;
    }
    let Some(&frame) = world.get::<TurretFrame>(target) else {
        return;
    };
    match job {
        TurretBuildJob::RemoveInterior => {
            if frame.step != START_INTERNAL_ARMOUR {
                return;
            }
            set_step(world, target, BOLTED, frame.gun);
            set_icon_state(world, target, "turret_frame");
            tell(
                world,
                action.client,
                Span::Notice,
                "Вы снимаете внутреннюю броню турели.",
            );
            spawn_stack(world, IRON, action.target_pos, SHEETS);
        }
        TurretBuildJob::Salvage => {}
        TurretBuildJob::Finish => {
            if frame.step != START_EXTERNAL_ARMOUR {
                return;
            }
            tell(
                world,
                action.client,
                Span::Notice,
                "Вы привариваете броню турели.",
            );
            finish(world, target, frame, action.target_pos);
        }
    }
}

/// `new /obj/machinery/porta_turret(loc)`, `setup(installed_gun)`, `locked = FALSE`.
fn finish(world: &mut World, frame_entity: Entity, frame: TurretFrame, pos: TilePos) {
    let Some(gun) = frame.gun else {
        return;
    };
    let gun_path = world
        .get::<DmItem>(gun)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    let lasertag = [(BLUETAG_GUN, LASERTAG_BLUE), (REDTAG_GUN, LASERTAG_RED)]
        .into_iter()
        .find(|(g, _)| is_under(&gun_path, g));
    let path = lasertag.map_or(TURRET, |(_, turret)| turret);
    let Some(turret) = spawn_obj(world, path, pos) else {
        return;
    };
    // The team presets carry their gun in the prototype.
    if lasertag.is_none()
        && let Some(gun_data) = world.get::<DmGun>(gun).cloned()
        && let Some(mut dm) = world.get_mut::<DmTurret>(turret)
    {
        install_gun(&mut dm, &gun_data);
    }
    if world.get::<StoredIn>(gun).is_some() {
        put_inside(world, gun, turret);
    }
    setup_turret(world, turret, 0.0);
    if let Some(mut t) = world.get_mut::<Turret>(turret) {
        t.locked = false;
    }
    // `turret.name = finish_name`.
    if let Some(name) = world
        .get::<FinishName>(frame_entity)
        .map(|f| f.name.clone())
    {
        set_name(world, turret, &name);
    }
    world.entity_mut(frame_entity).despawn();
    crate::construction::tile_changed(world, pos);
}

/// `attack_hand`: an empty hand takes the gun or the sensor back out. True when the click was
/// the frame's.
pub fn attack_hand(world: &mut World, client: Entity, target: Entity) -> bool {
    let Some(frame) = frame_of(world, target) else {
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(target) else {
        return false;
    };
    match frame.step {
        GUN_EQUIPPED => {
            if let Some(gun) = frame.gun {
                take_out(world, gun, pos);
            }
            set_step(world, target, INTERNAL_ARMOUR_ON, None);
            tell(
                world,
                client,
                Span::Notice,
                "Вы вынимаете оружие из рамы турели.",
            );
        }
        SENSORS_ON => {
            tell(
                world,
                client,
                Span::Notice,
                "Вы вынимаете датчик движения из рамы турели.",
            );
            spawn_obj(world, PROX_SENSOR, pos);
            set_step(world, target, GUN_EQUIPPED, frame.gun);
        }
        _ => {}
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_frame_is_unsecured_without_a_gun() {
        let f = TurretFrame::default();
        assert_eq!(f.step, UNSECURED);
        assert!(f.gun.is_none());
    }
}
