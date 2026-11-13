//! Drone shell dispensers (server), after tg `/obj/machinery/drone_dispenser`
//! (`code/game/machinery/drone_dispenser.dm`).
//!
//! A mapped dispenser gets [`DroneDispenser`] with its type's vars ([`DEFS`]) and its
//! `starting_amount` of iron and glass. Every tick, powered, whole and anchored, with the
//! materials for one product: ready, it starts (fewer than `maximum_idle` products on its tile;
//! "whirs to life!"), after `production_time` it uses the materials and energy and makes its
//! `dispense_type` (a drone shell: `super::drones::make_shell`, which tells the ghosts), then
//! recharges for `cooldownTime` and pings. Iron and glass stacks go in by hand (their
//! `mats_per_unit`; a stack of anything else is refused), a crowbar takes all out as sheets.
// shortcut: no welder repair of a broken dispenser and no break alarm (`welder_act`,
// `atom_break`); the material container is two counters, not tg's `material_container`.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::mob_spawn::DmMobSpawn;
use ss13_mapload::{DmItem, DmMob, DmName, DmObj};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_world::{MapInitLate, Mapped, SpatialIndex};

use crate::click_handlers::{AddClickHandler, ClickPhase};
use crate::sound::{PlaySound, playsound};
use crate::tools::{ToolKind, tell, tool_of};

pub const DISPENSER: &str = "/obj/machinery/drone_dispenser";
/// `SHEET_MATERIAL_AMOUNT`, `HALF_SHEET_MATERIAL_AMOUNT`, `MAX_STACK_SIZE`.
const SHEET: u32 = 100;
const HALF_SHEET: u32 = 50;
/// The container's `max_amount`: `SHEET_MATERIAL_AMOUNT * MAX_STACK_SIZE * 2`.
const MAX_UNITS: u32 = SHEET * 50 * 2;
const IRON: &str = "/datum/material/iron";
const GLASS: &str = "/datum/material/glass";
const IRON_SHEET: &str = "/obj/item/stack/sheet/iron";
const GLASS_SHEET: &str = "/obj/item/stack/sheet/glass";
const WORK_SOUND: &str = "sound/items/tools/rped.ogg";
const CREATE_SOUND: &str = "sound/items/deconstruct.ogg";
const RECHARGE_SOUND: &str = "sound/machines/ping.ogg";
/// The crowbar's `usesound`.
const CROWBAR_SOUND: &str = "sound/items/tools/crowbar.ogg";
/// `production_time`.
const PRODUCTION_SECS: f64 = 3.0;

/// The vars of a dispenser type.
#[derive(Debug)]
pub struct DispenserDef {
    root: &'static str,
    dispense: &'static [&'static str],
    iron_cost: u32,
    glass_cost: u32,
    /// `energy_used`, joules.
    energy: f64,
    starting: u32,
    cooldown_secs: f64,
    /// `maximum_idle`; 0 is no limit.
    max_idle: u32,
    begin: Option<&'static str>,
    end: &'static str,
    recharge: Option<&'static str>,
    recharge_sound: Option<&'static str>,
    /// `icon_on`, `icon_off`, `icon_recharging`, `icon_creating`.
    icons: [&'static str; 4],
}

const SHELL: &str = "/obj/effect/mob_spawn/ghost_role/drone";

const fn def(root: &'static str, dispense: &'static [&'static str]) -> DispenserDef {
    DispenserDef {
        root,
        dispense,
        iron_cost: HALF_SHEET,
        glass_cost: HALF_SHEET,
        energy: 1000.0,
        starting: 0,
        cooldown_secs: 180.0,
        max_idle: 3,
        begin: Some("оживает с жужжанием!"),
        end: "выдаёт оболочку дрона.",
        recharge: Some("пищит."),
        recharge_sound: Some(RECHARGE_SOUND),
        icons: ["on", "off", "recharge", "make"],
    }
}

/// Most specific first.
pub static DEFS: [DispenserDef; 9] = [
    DispenserDef {
        end: "выдаёт зловещую подозрительную оболочку дрона.",
        cooldown_secs: 10.0,
        starting: SHEET * 25 / 2,
        ..def(
            "/obj/machinery/drone_dispenser/syndrone/badass",
            &["/obj/effect/mob_spawn/ghost_role/drone/syndrone/badass"],
        )
    },
    DispenserDef {
        end: "выдаёт подозрительную оболочку дрона.",
        cooldown_secs: 10.0,
        starting: SHEET * 25 / 2,
        ..def(
            "/obj/machinery/drone_dispenser/syndrone",
            &["/obj/effect/mob_spawn/ghost_role/drone/syndrone"],
        )
    },
    DispenserDef {
        end: "выдаёт оболочку дрона-снежинки.",
        iron_cost: SHEET,
        glass_cost: SHEET,
        energy: 2000.0,
        starting: SHEET * 5,
        ..def(
            "/obj/machinery/drone_dispenser/snowflake",
            &["/obj/effect/mob_spawn/ghost_role/drone/snowflake"],
        )
    },
    DispenserDef {
        end: "выдаёт оболочку заброшенного дрона.",
        iron_cost: SHEET * 5,
        glass_cost: SHEET * 5 / 2,
        cooldown_secs: 60.0,
        ..def(
            "/obj/machinery/drone_dispenser/derelict",
            &["/obj/effect/mob_spawn/ghost_role/drone/derelict"],
        )
    },
    DispenserDef {
        end: "выдаёт оболочку классического дрона.",
        ..def(
            "/obj/machinery/drone_dispenser/classic",
            &["/obj/effect/mob_spawn/ghost_role/drone/classic"],
        )
    },
    DispenserDef {
        iron_cost: 0,
        glass_cost: 0,
        energy: 0.0,
        cooldown_secs: 1.0,
        begin: Some("закрывается и начинает что-то собирать внутри."),
        end: "распахивается, являя хайвбота!",
        recharge: None,
        recharge_sound: None,
        icons: [
            "hivebot_fab",
            "hivebot_fab",
            "hivebot_fab",
            "hivebot_fab_on",
        ],
        ..def(
            "/obj/machinery/drone_dispenser/hivebot",
            &["/mob/living/basic/hivebot"],
        )
    },
    DispenserDef {
        end: "выдаёт бинокль.",
        starting: SHEET * 5 / 2,
        max_idle: 1,
        cooldown_secs: 5.0,
        iron_cost: 0,
        glass_cost: 0,
        energy: 0.0,
        ..def(
            "/obj/machinery/drone_dispenser/binoculars",
            &["/obj/item/binoculars"],
        )
    },
    DispenserDef {
        starting: SHEET * 5 / 2,
        ..def("/obj/machinery/drone_dispenser/preloaded", &[SHELL])
    },
    def(DISPENSER, &[SHELL]),
];

fn def_of(path: &str) -> Option<&'static DispenserDef> {
    DEFS.iter().find(|d| is_under(path, d.root))
}

/// What the dispenser is doing (`DRONE_READY`, `DRONE_PRODUCTION`, `DRONE_RECHARGING`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Ready,
    Production,
    Recharging,
}

/// A drone shell dispenser and its materials (units).
#[derive(Component, Clone, Debug)]
pub struct DroneDispenser {
    def: &'static DispenserDef,
    pub mode: Mode,
    /// `timer`: the game time the current step ends.
    timer: f64,
    pub iron: u32,
    pub glass: u32,
}

impl DroneDispenser {
    fn has_materials(&self) -> bool {
        self.iron >= self.def.iron_cost && self.glass >= self.def.glass_cost
    }
}

/// What `process` does this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Wait,
    Begin,
    Produce,
    Recharged,
}

/// tg `process` once power, integrity and anchoring passed.
fn next_step(dispenser: &DroneDispenser, now: f64, idle: u32) -> Step {
    if !dispenser.has_materials() || dispenser.timer > now {
        return Step::Wait;
    }
    match dispenser.mode {
        Mode::Ready if dispenser.def.max_idle > 0 && idle >= dispenser.def.max_idle => Step::Wait,
        Mode::Ready => Step::Begin,
        Mode::Production => Step::Produce,
        Mode::Recharging => Step::Recharged,
    }
}

pub struct DroneDispenserPlugin;

impl Plugin for DroneDispenserPlugin {
    fn build(&self, app: &mut App) {
        // Stacks in, the crowbar's materials out (`crate::click_handlers`).
        app.add_click_handler(ClickPhase::HeldOnTarget, 263, item_interaction)
            .add_systems(MapInitLate, init_dispensers)
            .add_systems(Update, process.run_if(any_with_component::<DroneDispenser>));
    }
}

/// `Initialize`: the type's vars and its `starting_amount` of each material.
fn init_dispensers(world: &mut World) {
    let found: Vec<(Entity, &'static DispenserDef)> = world
        .query_filtered::<(Entity, &DmObj), (With<Mapped>, Without<DroneDispenser>)>()
        .iter(world)
        .filter_map(|(e, o)| Some((e, def_of(&o.path)?)))
        .collect();
    for (e, def) in found {
        world.entity_mut(e).insert(DroneDispenser {
            def,
            mode: Mode::Ready,
            timer: 0.0,
            iron: def.starting,
            glass: def.starting,
        });
    }
}

/// The type path of whatever `e` is (a spawner, a mob, an item).
fn path_of(world: &World, e: Entity) -> Option<&str> {
    let entity = world.get_entity(e).ok()?;
    entity
        .get::<DmMobSpawn>()
        .map(|s| s.path.as_str())
        .or_else(|| entity.get::<DmMob>().map(|m| m.path.as_str()))
        .or_else(|| entity.get::<DmItem>().map(|i| i.path.as_str()))
}

/// `count_shells`: what it makes, lying on its tile.
fn count_idle(world: &World, def: &DispenserDef, pos: TilePos) -> u32 {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .filter(|&&e| {
            path_of(world, e).is_some_and(|p| def.dispense.iter().any(|d| is_under(p, d)))
        })
        .count() as u32
}

fn machine_name(world: &World, e: Entity) -> String {
    world
        .get::<DmName>(e)
        .map(|n| crate::obj_damage::strip(&n.name).to_owned())
        .unwrap_or_default()
}

/// Powered (`NOPOWER`), not broken (`BROKEN`).
fn working(world: &World, e: Entity) -> bool {
    world.get::<crate::power::Unpowered>(e).is_none()
        && !world
            .get::<crate::obj_damage::Integrity>(e)
            .is_some_and(|i| i.broken)
}

fn anchored(world: &World, e: Entity) -> bool {
    world.get::<crate::obj_damage::Anchored>(e).map_or_else(
        || world.get::<DmObj>(e).is_some_and(|o| o.anchored),
        |a| a.0,
    )
}

/// tg `process` and `update_icon_state`.
// Cost: per tick, the dispensers (about one per station).
fn process(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let all: Vec<Entity> = world
        .query_filtered::<Entity, With<DroneDispenser>>()
        .iter(world)
        .collect();
    for e in all {
        let Some(&pos) = world.get::<TilePos>(e) else {
            continue;
        };
        let Some(dispenser) = world.get::<DroneDispenser>(e).cloned() else {
            continue;
        };
        let def = dispenser.def;
        let on = working(world, e);
        if on && anchored(world, e) {
            let idle = count_idle(world, def, pos);
            match next_step(&dispenser, now, idle) {
                Step::Wait => {}
                Step::Begin => {
                    say(world, e, def.begin);
                    playsound(world, PlaySound::new(pos, WORK_SOUND, 50.0, true));
                    set(world, e, Mode::Production, now + PRODUCTION_SECS);
                }
                Step::Produce => {
                    if let Some(mut d) = world.get_mut::<DroneDispenser>(e) {
                        d.iron -= def.iron_cost;
                        d.glass -= def.glass_cost;
                    }
                    if def.energy > 0.0 {
                        crate::power::machine_use_energy(world, e, def.energy, false);
                    }
                    for &path in def.dispense {
                        if is_under(path, SHELL) {
                            super::drones::make_shell(world, path, pos);
                        } else {
                            crate::construction::spawn_obj(world, path, pos);
                        }
                    }
                    playsound(world, PlaySound::new(pos, CREATE_SOUND, 50.0, true));
                    say(world, e, Some(def.end));
                    set(world, e, Mode::Recharging, now + def.cooldown_secs);
                }
                Step::Recharged => {
                    if let Some(sound) = def.recharge_sound {
                        playsound(world, PlaySound::new(pos, sound, 50.0, true));
                    }
                    say(world, e, def.recharge);
                    set(world, e, Mode::Ready, now);
                }
            }
        }
        let mode = world
            .get::<DroneDispenser>(e)
            .map_or(Mode::Ready, |d| d.mode);
        let [icon_on, icon_off, icon_recharging, icon_creating] = def.icons;
        let state = match mode {
            _ if !on => icon_off,
            Mode::Recharging => icon_recharging,
            Mode::Production => icon_creating,
            Mode::Ready => icon_on,
        };
        crate::obj_damage::set_icon_state(world, e, state);
    }
}

fn set(world: &mut World, e: Entity, mode: Mode, timer: f64) {
    if let Some(mut d) = world.get_mut::<DroneDispenser>(e) {
        d.mode = mode;
        d.timer = timer;
    }
}

/// `visible_message("[src] [text]")`.
fn say(world: &mut World, e: Entity, text: Option<&str>) {
    let Some(text) = text else {
        return;
    };
    let name = ss13_shared::capitalize(&machine_name(world, e));
    crate::medical::visible(world, e, "", &format!("{name} {text}"), Span::Notice);
}

/// The iron and glass units one unit of the stack `path` gives (`mats_per_unit`); `None` when it
/// is made of anything else (the container takes only iron and glass).
fn stack_materials(world: &World, path: &str) -> Option<(u32, u32)> {
    if let Some(db) = world.get_resource::<crate::science::techweb::ResearchDb>()
        && let Some(mats) = db.stacks.get(path)
    {
        let mut out = (0.0, 0.0);
        for &(m, n) in mats {
            match db.materials.get(m).map(|d| d.id.as_str()) {
                Some(IRON) => out.0 += n,
                Some(GLASS) => out.1 += n,
                _ => return None,
            }
        }
        return Some((out.0 as u32, out.1 as u32)).filter(|&(i, g)| i + g > 0);
    }
    // Without the research data: the sheets themselves.
    if is_under(path, IRON_SHEET) {
        Some((SHEET, 0))
    } else if is_under(path, GLASS_SHEET) {
        Some((0, SHEET))
    } else {
        None
    }
}

/// How many of `amount` stack units fit when each gives `per` (iron, glass) units into a
/// container holding `held` units of `MAX_UNITS`.
fn units_that_fit(per: (u32, u32), amount: u32, held: u32) -> u32 {
    let each = per.0 + per.1;
    if each == 0 {
        return 0;
    }
    (MAX_UNITS.saturating_sub(held) / each).min(amount)
}

/// `materials.user_insert` of a held stack, `crowbar_act` (`retrieve_all`).
fn item_interaction(world: &mut World, ctx: &crate::click_handlers::ClickCtx) -> bool {
    let (Some(held), target) = (ctx.held, ctx.target) else {
        return false;
    };
    let Some(dispenser) = world.get::<DroneDispenser>(target).cloned() else {
        return false;
    };
    let name = machine_name(world, target);
    if matches!(tool_of(world, held), Some((ToolKind::Crowbar, _))) {
        let Some(&pos) = world.get::<TilePos>(target) else {
            return true;
        };
        let (iron, glass) = (dispenser.iron / SHEET, dispenser.glass / SHEET);
        if iron > 0 {
            crate::construction::spawn_manufactured(world, IRON_SHEET, pos, iron);
        }
        if glass > 0 {
            crate::construction::spawn_manufactured(world, GLASS_SHEET, pos, glass);
        }
        if let Some(mut d) = world.get_mut::<DroneDispenser>(target) {
            d.iron -= iron * SHEET;
            d.glass -= glass * SHEET;
        }
        playsound(world, PlaySound::new(pos, CROWBAR_SOUND, 50.0, true));
        tell(
            world,
            ctx.client,
            Span::Notice,
            format!("Вы извлекаете материалы из {name}."),
        );
        return true;
    }
    if world.get::<crate::tools::Stack>(held).is_none() {
        return false;
    }
    let Some(path) = world.get::<DmItem>(held).map(|i| i.path.clone()) else {
        return false;
    };
    let item_name = crate::security::name_of(world, held);
    let Some(per) = stack_materials(world, &path) else {
        tell(
            world,
            ctx.client,
            Span::Warning,
            format!("{name} не принимает {item_name}!"),
        );
        return true;
    };
    let amount = crate::tools::stack_amount(world, held);
    let fit = units_that_fit(per, amount, dispenser.iron + dispenser.glass);
    if fit == 0 {
        tell(
            world,
            ctx.client,
            Span::Warning,
            format!("В {name} нет места для {item_name}!"),
        );
        return true;
    }
    if let Some(mut d) = world.get_mut::<DroneDispenser>(target) {
        d.iron += per.0 * fit;
        d.glass += per.1 * fit;
    }
    crate::tools::use_stack(world, held, fit);
    tell(
        world,
        ctx.client,
        Span::Notice,
        format!("Вы загружаете {item_name} ({fit} шт.) в {name}."),
    );
    true
}

/// tg `drone_dispenser/examine`: what one shell costs, the recharge.
pub fn examine_lines(world: &World, e: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(dispenser) = world.get::<DroneDispenser>(e) else {
        return;
    };
    let def = dispenser.def;
    let sheets = |units: u32| f64::from(units) / f64::from(SHEET);
    let needs = match (def.iron_cost > 0, def.glass_cost > 0) {
        (true, true) => Some(format!(
            "{} листов железа и {} листов стекла",
            sheets(def.iron_cost),
            sheets(def.glass_cost)
        )),
        (true, false) => Some(format!("{} листов железа", sheets(def.iron_cost))),
        (false, true) => Some(format!("{} листов стекла", sheets(def.glass_cost))),
        (false, false) => None,
    };
    if let Some(needs) = needs {
        lines.push((
            Span::Notice,
            format!("Для одной оболочки дрона нужно {needs}."),
        ));
    }
    if dispenser.mode == Mode::Recharging && working(world, e) {
        lines.push((
            Span::Warning,
            "Он жужжит и щёлкает. Похоже, перезаряжается.".to_owned(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dispenser(path: &str) -> DroneDispenser {
        let def = def_of(path).unwrap();
        DroneDispenser {
            def,
            mode: Mode::Ready,
            timer: 0.0,
            iron: def.starting,
            glass: def.starting,
        }
    }

    #[test]
    fn types_take_their_own_vars() {
        let syndrone = def_of("/obj/machinery/drone_dispenser/syndrone").unwrap();
        assert_eq!(syndrone.cooldown_secs, 10.0);
        assert_eq!(syndrone.starting, 1250);
        let badass = def_of("/obj/machinery/drone_dispenser/syndrone/badass").unwrap();
        assert!(badass.dispense[0].ends_with("/badass"));
        assert_eq!(def_of(DISPENSER).unwrap().starting, 0);
        assert_eq!(
            def_of("/obj/machinery/drone_dispenser/preloaded")
                .unwrap()
                .starting,
            250
        );
        assert!(def_of("/obj/machinery/autolathe").is_none());
    }

    #[test]
    fn a_cycle_needs_materials_and_room_and_waits_its_timers() {
        let mut d = dispenser("/obj/machinery/drone_dispenser/preloaded");
        // 2.5 sheets of each: five shells' worth.
        assert_eq!(next_step(&d, 0.0, 0), Step::Begin);
        // `maximum_idle` shells on the tile: nothing starts.
        assert_eq!(next_step(&d, 0.0, 3), Step::Wait);
        d.mode = Mode::Production;
        d.timer = 3.0;
        assert_eq!(next_step(&d, 2.0, 0), Step::Wait);
        assert_eq!(next_step(&d, 3.0, 0), Step::Produce);
        d.mode = Mode::Recharging;
        d.timer = 183.0;
        assert_eq!(next_step(&d, 100.0, 0), Step::Wait);
        assert_eq!(next_step(&d, 183.0, 0), Step::Recharged);
        // Out of glass: it waits whatever its step.
        d.glass = 49;
        assert_eq!(next_step(&d, 200.0, 0), Step::Wait);
        // The hivebot fabricator needs nothing.
        let hive = dispenser("/obj/machinery/drone_dispenser/hivebot");
        assert_eq!(next_step(&hive, 0.0, 0), Step::Begin);
    }

    #[test]
    fn stacks_fill_up_to_the_container() {
        assert_eq!(units_that_fit((100, 0), 30, 0), 30);
        assert_eq!(units_that_fit((100, 0), 30, MAX_UNITS - 250), 2);
        assert_eq!(units_that_fit((0, 0), 30, 0), 0);
        assert_eq!(units_that_fit((50, 50), 200, 0), 100);
    }
}
