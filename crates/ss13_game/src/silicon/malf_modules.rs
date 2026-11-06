//! The malfunctioning AI's module picker and modules (server), after tg `/datum/module_picker`
//! (`malf_ai_module_picker.dm`), `/datum/ai_module/malf` and `/datum/action/innate/ai`
//! (`malf_ai_modules.dm`).
//!
//! - A malfunctioning AI gets the "Malfunction Modules" button: a list of the modules still for
//!   sale with their price in processing time. Buying one (`purchase_module`): not dead, enough
//!   time; the module's action button appears, or a bought-again module adds its uses; a
//!   one-purchase module leaves the list for good. Each use of an action spends one use
//!   (`adjust_uses`, "Осталось использований"); out of uses, the button goes.
//! - Blackout (15, 3 uses): every APC overloads its lighting with a 30% × its overload chance
//!   (each miss raises its overload; tg `overload` starts at 1).
//! - Hostile Station Lockdown (30, once): every powered door of the station closes; airlocks the
//!   AI may control are unbolted, made unsafe, closed, bolted and electrified; 90 s later they are
//!   unbolted, un-electrified, opened and safe again and the other doors open; two minor
//!   announcements.
//! - Disable Emergency Lights (10, once): the station's lights lose their emergency cells
//!   (`no_low_power`).
//! - Air Alarm Safety Override (50, once): the station's air alarms are emagged (their dangerous
//!   modes are offered).
// shortcut: a list prompt instead of the `MalfunctionModulePicker` window (no descriptions or
// categories shown); no unlock or activation sounds; no status display lockdown alert; only these
// four modules (the doomsday device, machine override and overload, RCD destruction, megahonk,
// robotic factory, fire alarm override, camera reactivation and upgrades, remote emag, core
// tilt, vendor tipping, turret upgrade, eavesdropping, mech domination and voice changer are not
// ported).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::{DoorKind, IconIndex};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems};
use ss13_tgui::server::{UiActed, UiSystems};

use super::ai::AiEye;
use super::chassis::tell_ai;
use super::malf::{CORE_RETURN, MalfAi};
use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::doors::Door;
use crate::health::Stat;
use crate::server::PlayerMob;

/// The picker's button (tg `/datum/action/innate/choose_modules`).
pub const PICKER: &str = "malf_picker";
pub const ACTIONS_AI: &str = "icons/mob/actions/actions_AI.dmi";
/// `_malf_ai_undo_lockdown` after `90 SECONDS`.
pub const LOCKDOWN_SECS: f64 = 90.0;

/// What a bought module's action does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Power {
    Blackout,
    Lockdown,
    EmergencyLights,
    AirAlarms,
}

/// A `/datum/ai_module/malf` and its `/datum/action/innate/ai`.
pub struct ModuleDef {
    pub name: &'static str,
    pub cost: u32,
    pub one_purchase: bool,
    /// The action's `name`, `button_icon`, `button_icon_state` and `uses`.
    pub action: &'static str,
    pub icon: (&'static str, &'static str),
    pub uses: u32,
    pub unlock_text: &'static str,
    pub power: Power,
}

pub static MODULES: [ModuleDef; 4] = [
    ModuleDef {
        name: "Blackout",
        cost: 15,
        one_purchase: false,
        action: "Blackout",
        icon: (ACTIONS_AI, "blackout"),
        uses: 3,
        unlock_text: "You hook into the powernet and route bonus power towards the station's \
                      lighting.",
        power: Power::Blackout,
    },
    ModuleDef {
        name: "Hostile Station Lockdown",
        cost: 30,
        one_purchase: true,
        action: "Lockdown",
        icon: (ACTIONS_AI, "lockdown"),
        uses: 1,
        unlock_text: "Вы загружаете спящий троян в систему управления дверями. Вы можете \
                      отправить сигнал, чтобы активировать его в любое время",
        power: Power::Lockdown,
    },
    ModuleDef {
        name: "Disable Emergency Lights",
        cost: 10,
        one_purchase: true,
        action: "Disable Emergency Lights",
        icon: ("icons/obj/lighting.dmi", "floor_emergency"),
        uses: 1,
        unlock_text: "Вы подключаетесь к энергосети и находите связи между лампами и резервными \
                      источниками.",
        power: Power::EmergencyLights,
    },
    ModuleDef {
        name: "Air Alarm Safety Override",
        cost: 50,
        one_purchase: true,
        action: "Override Air Alarm Safeties",
        icon: ("icons/obj/machines/wallmounts.dmi", "alarmx"),
        uses: 1,
        unlock_text: "Вы отключаете протоколы безопасности на всех атмосферных датчиках, но \
                      оставляете окно подтверждения открытым. Вы можете нажать 'Да' в любой \
                      момент... ублюдок.",
        power: Power::AirAlarms,
    },
];

/// On a malfunctioning AI: its actions' uses by module index, the one-purchase modules bought
/// (tg `possible_modules` without them).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct MalfModules {
    pub uses: Vec<(usize, u32)>,
    pub sold_out: Vec<usize>,
}

/// On an APC: tg `overload`, the blackout's chance factor.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApcOverload(pub u32);

/// Lockdowns waiting for their reset.
#[derive(Resource, Default)]
pub struct Lockdowns(pub Vec<f64>);

/// The picker's list, on its prompt entity: the module index of each item.
#[derive(Component, Clone, Debug)]
struct PickerPrompt {
    ai: Entity,
    offered: Vec<usize>,
}

pub struct MalfModulesPlugin;

impl Plugin for MalfModulesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Lockdowns>().add_systems(
            Update,
            (
                buttons.in_set(UiSystems::Handle),
                picked
                    .in_set(UiSystems::Handle)
                    .after(PromptSystems::Answer),
                end_lockdowns.run_if(|l: Res<Lockdowns>| !l.0.is_empty()),
            ),
        );
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn button_of(world: &World, id: &str, name: &str, icon: (&str, &str)) -> (ActionButton, f64) {
    let icon = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(icon.0, icon.1));
    (
        ActionButton {
            id: id.to_owned(),
            name: name.to_owned(),
            icon,
            background: None,
            border: None,
            available: true,
            cooldown: 0.0,
            active: false,
            maptext: None,
        },
        0.0,
    )
}

/// One more button on the eye (tg `Grant`); nothing if it has one with this id.
pub fn add_button(world: &mut World, eye: Entity, id: &str, name: &str, icon: (&str, &str)) {
    let mut list = world
        .get::<ActionButtons>(eye)
        .map(|b| b.0.clone())
        .unwrap_or_default();
    if list.iter().any(|(b, _)| b.id == id) {
        return;
    }
    list.push(button_of(world, id, name, icon));
    set_buttons(world, eye, list);
}

/// The eye's button `id` goes (tg `Remove`).
pub fn remove_button(world: &mut World, eye: Entity, id: &str) {
    let Some(mut list) = world.get::<ActionButtons>(eye).map(|b| b.0.clone()) else {
        return;
    };
    let before = list.len();
    list.retain(|(b, _)| b.id != id);
    if list.len() != before {
        set_buttons(world, eye, list);
    }
}

fn set_maptext(world: &mut World, eye: Entity, id: &str, text: Option<String>) {
    let Some(mut list) = world.get::<ActionButtons>(eye).map(|b| b.0.clone()) else {
        return;
    };
    for (b, _) in &mut list {
        if b.id == id {
            b.maptext = text.clone();
        }
    }
    set_buttons(world, eye, list);
}

fn power_id(index: usize) -> String {
    format!("malf_module_{index}")
}

fn eye_of(world: &World, ai: Entity) -> Option<Entity> {
    world.get::<super::ai::AiCore>(ai).map(|c| c.eye)
}

/// tg `add_malf_picker`: the button and the hints.
pub fn give_picker(world: &mut World, ai: Entity) {
    world.entity_mut(ai).insert(MalfModules::default());
    let Some(eye) = eye_of(world, ai) else {
        return;
    };
    add_button(
        world,
        eye,
        PICKER,
        "Malfunction Modules",
        (ACTIONS_AI, "modules_menu"),
    );
    tell_ai(
        world,
        ai,
        Span::Plain,
        "In the top left corner of the screen you will find the Malfunction Modules button, where \
         you can purchase various abilities, from upgraded surveillance to station ending \
         doomsday devices.",
    );
}

/// The modules still for sale (tg `possible_modules`).
pub fn for_sale(modules: &MalfModules) -> Vec<usize> {
    (0..MODULES.len())
        .filter(|i| !modules.sold_out.contains(i))
        .collect()
}

fn incapacitated(world: &World, ai: Entity) -> bool {
    world.get::<Stat>(ai) != Some(&Stat::Conscious) || super::ai_power::depowered(world, ai)
}

fn buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client) {
            continue;
        }
        let Some(ai) = world.get::<AiEye>(actor).map(|e| e.ai) else {
            continue;
        };
        // tg `Trigger`: the button must still be the mob's.
        let has = world
            .get::<ActionButtons>(actor)
            .is_some_and(|b| b.0.iter().any(|(b, _)| b.id == id));
        if !has || world.get::<MalfAi>(ai).is_none() || incapacitated(world, ai) {
            continue;
        }
        if id == PICKER {
            open_picker(world, client, actor, ai);
        } else if id == CORE_RETURN {
            super::malf::vacate(world, ai);
        } else if let Some(index) = (0..MODULES.len()).find(|&i| power_id(i) == id) {
            activate(world, ai, index);
        }
    }
}

fn open_picker(world: &mut World, client: Entity, eye: Entity, ai: Entity) {
    let modules = world.get::<MalfModules>(ai).cloned().unwrap_or_default();
    let points = world.get::<MalfAi>(ai).map_or(0, |m| m.processing_time);
    let hacked = world.get::<MalfAi>(ai).map_or(0, |m| m.hacked_apcs.len());
    let offered = for_sale(&modules);
    let items = offered
        .iter()
        .map(|&i| format!("{} ({})", MODULES[i].name, MODULES[i].cost))
        .collect();
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        Ask::new(
            client,
            eye,
            "Malfunction Modules Menu",
            format!("Processing time: {points}. Hacked APCs: {hacked}."),
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(PickerPrompt { ai, offered });
}

fn picked(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.answer.clone()))
        .collect();
    for (prompt, answer) in pending {
        let Some(asked) = world.get::<PickerPrompt>(prompt).cloned() else {
            continue;
        };
        if let Some(PromptAnswer::Item(i)) = answer
            && let Some(&index) = asked.offered.get(i)
        {
            purchase(world, asked.ai, index);
        }
    }
}

/// tg `purchase_module`. False when it was refused (dead, too expensive, sold out).
pub fn purchase(world: &mut World, ai: Entity, index: usize) -> bool {
    let Some(def) = MODULES.get(index) else {
        return false;
    };
    let Some(points) = world.get::<MalfAi>(ai).map(|m| m.processing_time) else {
        return false;
    };
    let mut modules = world.get::<MalfModules>(ai).cloned().unwrap_or_default();
    if world.get::<Stat>(ai) == Some(&Stat::Dead)
        || def.cost > points
        || modules.sold_out.contains(&index)
    {
        return false;
    }
    let eye = eye_of(world, ai);
    match modules.uses.iter_mut().find(|(i, _)| *i == index) {
        // tg: `action.uses += initial(action.uses)`.
        Some((_, uses)) => *uses += def.uses,
        None => {
            modules.uses.push((index, def.uses));
            if def.one_purchase {
                modules.sold_out.push(index);
            }
            if let Some(eye) = eye {
                add_button(world, eye, &power_id(index), def.action, def.icon);
            }
            tell_ai(world, ai, Span::Notice, def.unlock_text);
        }
    }
    let uses = modules
        .uses
        .iter()
        .find(|(i, _)| *i == index)
        .map_or(0, |(_, u)| *u);
    world.entity_mut(ai).insert(modules);
    if let Some(mut m) = world.get_mut::<MalfAi>(ai) {
        m.processing_time -= def.cost;
    }
    if let Some(eye) = eye
        && def.uses > 1
    {
        set_maptext(world, eye, &power_id(index), Some(uses.to_string()));
    }
    info!("{ai} purchased the malf module {}", def.name);
    true
}

/// tg `adjust_uses(-1)`: the message, the button gone at zero.
fn spend_use(world: &mut World, ai: Entity, index: usize) {
    let def = &MODULES[index];
    let Some(mut modules) = world.get::<MalfModules>(ai).cloned() else {
        return;
    };
    let Some(slot) = modules.uses.iter().position(|(i, _)| *i == index) else {
        return;
    };
    modules.uses[slot].1 = modules.uses[slot].1.saturating_sub(1);
    let left = modules.uses[slot].1;
    if left == 0 {
        modules.uses.remove(slot);
    }
    world.entity_mut(ai).insert(modules);
    let Some(eye) = eye_of(world, ai) else {
        return;
    };
    if left > 0 {
        tell_ai(
            world,
            ai,
            Span::Notice,
            format!(
                "Использовано {}. Осталось использований: {left}.",
                def.action
            ),
        );
        set_maptext(world, eye, &power_id(index), Some(left.to_string()));
    } else {
        if def.uses > 1 {
            tell_ai(
                world,
                ai,
                Span::Warning,
                format!("Закончились использования {}!", def.action),
            );
        }
        remove_button(world, eye, &power_id(index));
    }
}

fn activate(world: &mut World, ai: Entity, index: usize) {
    let owned = world
        .get::<MalfModules>(ai)
        .is_some_and(|m| m.uses.iter().any(|&(i, u)| i == index && u > 0));
    if !owned {
        return;
    }
    match MODULES[index].power {
        Power::Blackout => {
            blackout(world);
            tell_ai(
                world,
                ai,
                Span::Notice,
                "Излишние напряжение наполняет энергосеть станции.",
            );
        }
        Power::Lockdown => {
            lockdown(world);
            tell_ai(
                world,
                ai,
                Span::Danger,
                "Инициирована блокировка. Система перезагрузится через 90 секунд.",
            );
        }
        Power::EmergencyLights => {
            disable_emergency_lights(world);
            tell_ai(world, ai, Span::Notice, "Отключение аварийного освещения.");
        }
        Power::AirAlarms => {
            break_air_alarms(world);
            tell_ai(
                world,
                ai,
                Span::Notice,
                "Предохранители на всех атмосферных датчиках были отключены. Теперь у вас есть \
                 возможность использовать чрезвычайно опасные режимы окружающей среды.",
            );
        }
    }
    spend_use(world, ai, index);
}

/// `/datum/action/innate/ai/blackout/Activate`: `prob(30 * overload)` overloads, else
/// `overload++`.
// Cost: once per use, every APC (~150 on MetaStation).
pub fn blackout(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, Option<&ApcOverload>), With<crate::power::Apc>>();
    let apcs: Vec<(Entity, u32)> = q
        .iter(world)
        .map(|(e, o)| (e, o.map_or(1, |o| o.0)))
        .collect();
    for (apc, overload) in apcs {
        let roll = world
            .get_resource_mut::<crate::combat::CombatRng>()
            .map_or(100, |mut r| r.range(1, 100));
        if roll <= 30 * overload {
            crate::events::electrical_storm::overload_lighting(world, apc);
        } else {
            world.entity_mut(apc).insert(ApcOverload(overload + 1));
        }
    }
}

/// `/datum/action/innate/ai/lockdown/Activate` (`hostile_lockdown` of every station door).
// Cost: once, every door of the station (~1000 on MetaStation).
pub fn lockdown(world: &mut World) {
    for (door, _) in station_doors(world) {
        crate::doors::hostile_lockdown(world, door);
    }
    let at = now(world) + LOCKDOWN_SECS;
    world.resource_mut::<Lockdowns>().0.push(at);
    crate::security_level::minor_announce(
        world,
        "Тревога сети",
        "Вредоносное программное обеспечение обнаружено в системе контроля шлюзов. Задействованы \
         протоколы изоляции. Пожалуйста, сохраняйте спокойствие.",
        true,
    );
}

fn station_doors(world: &mut World) -> Vec<(Entity, bool)> {
    let levels = crate::events::controller::station_levels(world);
    let mut q = world.query::<(Entity, &Door, &TilePos)>();
    q.iter(world)
        .filter(|(.., p)| levels.contains(&p.z))
        .map(|(e, d, _)| (e, d.kind == DoorKind::Airlock))
        .collect()
}

/// `_malf_ai_undo_lockdown` and the reset announcement, 90 s after a lockdown.
fn end_lockdowns(world: &mut World) {
    let t = now(world);
    let due = world.resource::<Lockdowns>().0.iter().any(|&at| t >= at);
    if !due {
        return;
    }
    world.resource_mut::<Lockdowns>().0.retain(|&at| t < at);
    undo_lockdown(world);
    crate::security_level::minor_announce(
        world,
        "Сеть перезапущена:",
        "Автоматическая перезагрузка системы завершена. Безопасного вам дня.",
        false,
    );
}

/// `disable_lockdown` of every station door.
pub fn undo_lockdown(world: &mut World) {
    for (door, _) in station_doors(world) {
        crate::doors::disable_lockdown(world, door);
    }
}

/// `/datum/action/innate/ai/emergency_lights/Activate`: `no_low_power` on every station light.
pub fn disable_emergency_lights(world: &mut World) {
    let levels = crate::events::controller::station_levels(world);
    let mut q = world.query::<(&mut crate::power::LightFixture, &TilePos)>();
    for (mut light, pos) in q.iter_mut(world) {
        if levels.contains(&pos.z) && light.emergency.is_some() {
            light.emergency = None;
        }
    }
}

/// `/datum/action/innate/ai/break_air_alarms/Activate`: every station air alarm `EMAGGED`.
pub fn break_air_alarms(world: &mut World) {
    let levels = crate::events::controller::station_levels(world);
    let mut q = world.query::<(&mut crate::atmos_machinery::AirAlarm, &TilePos)>();
    for (mut alarm, pos) in q.iter_mut(world) {
        if levels.contains(&pos.z) && !alarm.emagged {
            alarm.emagged = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn malf(world: &mut World, points: u32) -> Entity {
        world
            .spawn((
                MalfAi {
                    employer: String::new(),
                    flavor: default(),
                    objectives: Vec::new(),
                    processing_time: points,
                    hacked_apcs: Vec::new(),
                    hacking: None,
                },
                MalfModules::default(),
                Stat::Conscious,
            ))
            .id()
    }

    #[test]
    fn modules_cost_processing_time_and_one_purchase_ones_sell_out() {
        let mut world = World::new();
        let ai = malf(&mut world, 50);
        // Lockdown (30): bought, gone from the list.
        assert!(purchase(&mut world, ai, 1));
        assert_eq!(world.get::<MalfAi>(ai).unwrap().processing_time, 20);
        let modules = world.get::<MalfModules>(ai).unwrap().clone();
        assert_eq!(modules.uses, [(1, 1)]);
        assert!(!for_sale(&modules).contains(&1));
        assert!(!purchase(&mut world, ai, 1));
        // Blackout (15) twice: 3 + 3 uses; then nothing is affordable.
        assert!(purchase(&mut world, ai, 0));
        assert!(!purchase(&mut world, ai, 0));
        world.get_mut::<MalfAi>(ai).unwrap().processing_time = 15;
        assert!(purchase(&mut world, ai, 0));
        assert_eq!(world.get::<MalfModules>(ai).unwrap().uses, [(1, 1), (0, 6)]);
        assert_eq!(world.get::<MalfAi>(ai).unwrap().processing_time, 0);
        assert!(for_sale(world.get::<MalfModules>(ai).unwrap()).contains(&0));
    }

    #[test]
    fn a_dead_ai_buys_nothing() {
        let mut world = World::new();
        let ai = malf(&mut world, 400);
        world.entity_mut(ai).insert(Stat::Dead);
        assert!(!purchase(&mut world, ai, 2));
    }

    #[test]
    fn the_last_use_removes_the_action() {
        let mut world = World::new();
        let ai = malf(&mut world, 10);
        assert!(purchase(&mut world, ai, 2));
        spend_use(&mut world, ai, 2);
        assert!(world.get::<MalfModules>(ai).unwrap().uses.is_empty());
    }
}
