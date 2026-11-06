//! Radiation Leak (`code/modules/events/radiation_leak.dm`): a dense powered machine of the
//! station (or a vent or scrubber) starts to pulse radiation (`radioactive_emitter`: range 5,
//! threshold `RAD_MEDIUM_INSULATION`, every 2 s) and to puff polonium and mutagen smoke, three
//! times over the event's 5 minutes. One to three of eight tool methods, told on examine, stop
//! it early: a 30 s `use_tool` (a welder burns 2 fuel).
// shortcut: no `GLOB.generic_event_spawns` (the event landmarks are not loaded): the machine is
// any fitting one of the station, not one within 3 tiles of a landmark; no ghost alert; no
// "прервано!" balloon on an interrupted repair; balloon alerts are chat lines.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmTool, HiddenUnderTile};
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, Span};

use super::controller::{EventRng, EventState, RoundEvent, station_levels};
use crate::construction::Job;
use crate::radiation::{RAD_MEDIUM_INSULATION, RadioactiveEmitter};
use crate::tools::{Actor, DoAfter, tell};

pub const NAME: &str = "Radiation Leak";
const MACHINE_ROOT: &str = "/obj/machinery";
/// Vents and scrubbers leak too, though not dense.
const UNARY: &str = "/obj/machinery/atmospherics/components/unary";
/// `30 SECONDS`, deciseconds.
const FIX_DS: f32 = 300.0;
const SMOKE_SOUND: &str = "sound/effects/smoke.ogg";
const POLONIUM: &str = "/datum/reagent/toxin/polonium";
const MUTAGEN: &str = "/datum/reagent/toxin/mutagen";

/// `how_do_we_fix_it`: the method shown on examine and the `TOOL_*` it takes. The random picks
/// inside two of the texts are made when the event starts.
const METHODS: [(&str, &str); 8] = [
    ("закрутить несколько клапанов", "wrench"),
    ("затянуть болты", "wrench"),
    ("вправить панель ломом", "crowbar"),
    ("затянуть винты", "screwdriver"),
    ("проверить {}", "multitool"),
    ("{} панель", "welder"),
    ("проанализировать показатели", "analyzer"),
    ("обрезать лишние провода", "cutters"),
];

/// On the leaking machine: the tool behaviours that fix it (tg's `COMSIG_ATOM_TOOL_ACT`
/// registrations). Removed by the fix or the event's end.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct RadiationLeak {
    pub tools: Vec<&'static str>,
}

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Leak {
        machine: None,
        leaking: false,
    })
}

struct Leak {
    /// `picked_machine_ref`.
    machine: Option<Entity>,
    /// `start()` made it leak.
    leaking: bool,
}

impl RoundEvent for Leak {
    fn setup(&mut self, world: &mut World, ev: &mut EventState) {
        ev.start_when = 1;
        ev.announce_when = 10;
        ev.end_when = 150;
        self.machine = pick_machine(world);
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, fake: bool) {
        let area = if fake {
            Some(super::vent_clog::random_area_name(world))
        } else {
            self.machine.and_then(|m| area_name(world, m))
        }
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| "an unknown area".to_owned());
        let who = *world
            .resource_mut::<EventRng>()
            .pick(&[
                "механики",
                "инженеры",
                "ученые",
                "интерны",
                "сенсоры",
                "показатели",
            ])
            .expect("non-empty");
        let text = format!(
            "Утечка радиации обнаружена в {area}. Все члены экипажа должны эвакуироваться из \
             поражённой зоны. Наши {who} указывают, что машинерия в этой локации является \
             причиной проблемы - отремонтируйте её как можно быстрее, чтобы остановить утечку"
        );
        let title = format!("{}: Инженерный отдел", super::COMMAND_NAME);
        super::priority_announce(world, &title, &text);
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let Some(machine) = self.machine.filter(|&m| world.get_entity(m).is_ok()) else {
            return;
        };
        let (methods, tools) = pick_methods(&mut world.resource_mut::<EventRng>());
        let name = crate::security::name_of(world, machine);
        if let Some(&pos) = world.get::<TilePos>(machine) {
            crate::security::show(
                world,
                pos,
                crate::health::DEFAULT_MESSAGE_RANGE,
                ChatChannel::Warning,
                (
                    Span::Danger,
                    format!("{name} начинает испускать ужасный зеленый газ!"),
                ),
                &[],
            );
        }
        let examine = format!(
            "Оно испускает зеленый газ... Возможно, вы сможете исправить это, если сделаете одно \
             из следующего: {}.",
            english_list_or(&methods)
        );
        world.entity_mut(machine).insert((
            RadioactiveEmitter::new(2.0, 5.0, RAD_MEDIUM_INSULATION, Some(examine)),
            RadiationLeak { tools },
        ));
        self.leaking = true;
        puff_some_smoke(world, machine);
    }

    fn tick(&mut self, world: &mut World, ev: &mut EventState) {
        let Some(machine) = self.machine.filter(|&m| world.get_entity(m).is_ok()) else {
            return;
        };
        // `try_remove_radiation` ended the event early (`end()`, `kill()`).
        if self.leaking && world.get::<RadiationLeak>(machine).is_none() {
            self.end(world, ev);
            ev.killed = true;
            return;
        }
        // Roughly three puffs over the event.
        if ev.active_for.is_multiple_of((ev.end_when / 3).max(1)) {
            puff_some_smoke(world, machine);
        }
    }

    fn end(&mut self, world: &mut World, _: &mut EventState) {
        let Some(machine) = self.machine.take() else {
            return;
        };
        if world.get_entity(machine).is_err() {
            return;
        }
        if self.leaking {
            let name = crate::security::name_of(world, machine);
            if let Some(&pos) = world.get::<TilePos>(machine) {
                crate::security::show(
                    world,
                    pos,
                    crate::health::DEFAULT_MESSAGE_RANGE,
                    ChatChannel::Info,
                    (
                        Span::Notice,
                        format!("Газ, исходящий от {name} рассеивается."),
                    ),
                    &[],
                );
            }
        }
        self.leaking = false;
        world
            .entity_mut(machine)
            .remove::<(RadioactiveEmitter, RadiationLeak)>();
    }
}

/// `setup()`'s machine: powered (`use_power`), dense or a vent/scrubber, visible, not under the
/// floor and letting air through, in a station area on a station level.
fn pick_machine(world: &mut World) -> Option<Entity> {
    let levels = station_levels(world);
    let mut q = world.query::<(
        Entity,
        &ss13_mapload::DmPower,
        &TilePos,
        Option<&DmAtom>,
        Has<ss13_world::Density>,
        Has<HiddenUnderTile>,
        Has<ss13_atmos::AtmosBlocker>,
    )>();
    let mut found: Vec<(Entity, TilePos)> = q
        .iter(world)
        .filter(|(_, dm, p, atom, dense, hidden, blocker)| {
            levels.contains(&p.z)
                && is_under(&dm.path, MACHINE_ROOT)
                && dm.use_power > 0.0
                && (*dense || is_under(&dm.path, UNARY))
                && !hidden
                && !blocker
                && atom.is_none_or(|a| a.invisibility == 0 && a.alpha > 0 && a.mouse_opacity > 0)
        })
        .map(|(e, _, &p, ..)| (e, p))
        .collect();
    found.sort_by_key(|&(e, _)| e);
    found.retain(|&(_, p)| {
        super::area_type_at(world, p).is_some_and(|a| is_under(a, "/area/station"))
    });
    world
        .resource_mut::<EventRng>()
        .pick(&found)
        .map(|&(e, _)| e)
}

/// One to three distinct methods (`pick_n_take`) and the tools they take.
fn pick_methods(rng: &mut EventRng) -> (Vec<String>, Vec<&'static str>) {
    let mut keys: Vec<(String, &'static str)> = METHODS
        .iter()
        .map(|&(text, tool)| {
            let text = match tool {
                "multitool" => {
                    text.replace("{}", rng.pick(&["проводку", "микросхемы"]).expect("2"))
                }
                "welder" => text.replace("{}", rng.pick(&["разварить", "заварить"]).expect("2")),
                _ => text.to_owned(),
            };
            (text, tool)
        })
        .collect();
    let count = rng.0.range(1, 3);
    let (mut methods, mut tools) = (Vec::new(), Vec::new());
    for _ in 0..count {
        let i = rng.0.range(0, keys.len() as i64 - 1) as usize;
        let (text, tool) = keys.remove(i);
        methods.push(text);
        if !tools.contains(&tool) {
            tools.push(tool);
        }
    }
    (methods, tools)
}

/// `english_list(methods, and_text = " или ")`.
fn english_list_or(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} или {last}", init.join(", ")),
    }
}

/// `get_area_name` of the machine.
fn area_name(world: &World, machine: Entity) -> Option<String> {
    let pos = *world.get::<TilePos>(machine)?;
    let area = world.resource::<ss13_mapload::Areas>().area_at(pos)?;
    Some(crate::shuttle::area_name(world, area))
}

/// `puff_some_smoke`: range 2 chemical smoke of 10 u polonium and 10 u mutagen, and its sound.
fn puff_some_smoke(world: &mut World, machine: Entity) {
    let Some(pos) = crate::explosions::turf_of(world, machine) else {
        return;
    };
    if let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    {
        let list: Vec<(ss13_reagents::ReagentId, f32)> = [POLONIUM, MUTAGEN]
            .iter()
            .filter_map(|p| Some((chem.id(p)?, 10.0)))
            .collect();
        let mut carry = ss13_reagents::Holder::new(20.0, 0, ss13_reagents::Owner::default());
        carry.add_reagent_list(&chem, &list, None, true);
        crate::effects::smoke::do_chem_smoke(
            world,
            2,
            pos,
            None,
            &carry,
            crate::effects::smoke::SmokeKind::Chem,
        );
    }
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, SMOKE_SOUND, 50.0, true),
    );
}

/// `on_machine_tooled` (left click with one of the leak's tools): starts the 30 s repair.
pub fn tool_act(world: &mut World, actor: Actor, target: Entity) -> bool {
    let Some(item) = actor.item else {
        return false;
    };
    let Some(behaviour) = world.get::<DmTool>(item).map(|t| t.behaviour.clone()) else {
        return false;
    };
    if !world
        .get::<RadiationLeak>(target)
        .is_some_and(|l| l.tools.contains(&behaviour.as_str()))
    {
        return false;
    }
    tell(world, actor.client, Span::Notice, "устраняем протечку...");
    let amount = if behaviour == "welder" { 2 } else { 0 };
    crate::tools::use_tool(world, actor, target, FIX_DS, amount, Job::RadiationLeak);
    // `ITEM_INTERACT_BLOCKING` whether it started or not.
    true
}

/// The end of the repair: the leak stops (the event notices on its next fire and ends).
pub fn complete(world: &mut World, action: &DoAfter, consume: impl FnOnce(&mut World) -> bool) {
    let target = action.target;
    if world.get::<RadiationLeak>(target).is_none() || !consume(world) {
        return;
    }
    world
        .entity_mut(target)
        .remove::<(RadioactiveEmitter, RadiationLeak)>();
    tell(world, action.client, Span::Notice, "протечка исправлена");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn methods_are_distinct_and_named() {
        for seed in 0..50 {
            let mut rng = EventRng(ss13_mapload::spawner::Rng::new(seed));
            let (methods, tools) = pick_methods(&mut rng);
            assert!((1..=3).contains(&methods.len()));
            assert!(!tools.is_empty() && tools.len() <= methods.len());
            assert!(methods.iter().all(|m| !m.contains("{}")));
            let mut sorted = methods.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), methods.len());
        }
    }

    #[test]
    fn english_list_joins_with_or() {
        let l = |v: &[&str]| english_list_or(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(l(&["a"]), "a");
        assert_eq!(l(&["a", "b"]), "a или b");
        assert_eq!(l(&["a", "b", "c"]), "a, b или c");
    }
}
