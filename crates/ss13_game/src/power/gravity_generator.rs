//! The gravity generator (server): tg `/obj/machinery/gravity_generator`,
//! `code/modules/power/gravitygenerator.dm`.
//!
//! - The main part sits at the bottom middle of a 3x3 machine; the other eight are
//!   [`GravityGeneratorPart`]s (the top row is not dense) that pass clicks, tools and breakage on
//!   to it.
//! - The breaker (the `GravityGenerator` window, `gentoggle`) and area power (environ) decide
//!   whether it charges up or down: 2% per machine tick, with an EMP crackle; at 100% it is on,
//!   at 0% off. On, it gives its level — every station level for a station generator — gravity
//!   ([`crate::gravity::LevelGravity`]) and crushes everything within 2 tiles with 6 G
//!   ([`crate::gravity::GravityFields`]); the level's gravity coming or going shakes every
//!   screen there with an alarm and refreshes the weightlessness of every mob.
//! - A devastating explosion breaks it (and its parts); the repair is a screwdriver, a welder,
//!   10 plasteel sheets and a wrench, in that order. [`blackout`] (the gravity generator
//!   blackout event, a gravity anomaly's detonation) drains it and opens the breaker.
// shortcut: no investigate log or admin messages, no tool sounds, no shuttle moves,
// no nebula shielding, no silicon access, no `blob_act`/`zap_act`; the blackout event is not
// scheduled by the events controller (see `docs/parity/events`).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmItem, DmPower};
use ss13_shared::appearance::Overlay;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::gravity_generator::{GravityGeneratorData, GravityGeneratorUi};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject};
use ss13_world::Density;

use super::{EngineRng, MachineTicks, Unpowered};
use crate::gravity::{GravityField, GravityFields, LevelGravity};
use crate::tools::{ToolKind, tell};

pub const GRAVGEN_MAIN: &str = "/obj/machinery/gravity_generator/main";
pub const GRAVGEN_OFF: &str = "/obj/machinery/gravity_generator/main/off";
pub const GRAVGEN_PART: &str = "/obj/machinery/gravity_generator/part";
const PLASTEEL: &str = "/obj/item/stack/sheet/plasteel";
/// `gravity_field = new(src, 2, TRUE, 6)`.
const FIELD_RANGE: u32 = 2;
const FIELD_GRAVITY: f32 = 6.0;
/// The plasteel the repair takes.
const PLATING_SHEETS: u32 = 10;
const CHARGE_SOUND: &str = "sound/effects/empulse.ogg";
const ALERT_SOUND: &str = "sound/effects/alert.ogg";
const CLICK_SOUND: &str = "sound/machines/click.ogg";

/// `charging_state`: `POWER_IDLE`, `POWER_UP`, `POWER_DOWN`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Charging {
    #[default]
    Idle,
    Up,
    Down,
}

/// What a machine tick of [`GravityGenerator::process`] does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Nothing,
    Enable,
    Disable,
    /// Charged up or down by 2; `crackle`: the EMP sound plays.
    Charged {
        crackle: bool,
    },
}

/// tg `/obj/machinery/gravity_generator/main`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct GravityGenerator {
    pub on: bool,
    pub breaker: bool,
    pub charging: Charging,
    /// 0..100.
    pub charge_count: i32,
    /// `machine_stat & BROKEN`.
    pub broken: bool,
    /// `GRAV_NEEDS_SCREWDRIVER` 0, `GRAV_NEEDS_WELDING` 1, `GRAV_NEEDS_PLASTEEL` 2,
    /// `GRAV_NEEDS_WRENCH` 3.
    pub broken_state: u8,
    pub parts: Vec<Entity>,
    /// The fifth part, which carries the charge overlay.
    pub center: Option<Entity>,
    /// Its parts were spawned (`setup_parts`).
    set_up: bool,
    /// The area power last seen, for `power_change`.
    last_powered: Option<bool>,
}

impl GravityGenerator {
    /// A mapped generator: `/main` starts on and charged, `/main/off` off and empty.
    pub fn of_path(path: &str) -> Self {
        let on = !is_under(path, GRAVGEN_OFF);
        Self {
            on,
            breaker: on,
            charging: Charging::Idle,
            charge_count: if on { 100 } else { 0 },
            broken: false,
            broken_state: 0,
            parts: Vec::new(),
            center: None,
            set_up: false,
            last_powered: None,
        }
    }

    /// `set_power`: charge up with power and the breaker on, else down.
    pub fn set_power(&mut self, powered: bool) {
        self.charging = if powered && !self.broken && self.breaker {
            Charging::Up
        } else {
            Charging::Down
        };
    }

    /// `process()`; `roll` is `prob(75)` for the crackle.
    pub fn process(&mut self, roll: impl FnOnce() -> bool) -> Step {
        if self.broken || self.charging == Charging::Idle {
            return Step::Nothing;
        }
        if self.charging == Charging::Up && self.charge_count >= 100 {
            return Step::Enable;
        }
        if self.charging == Charging::Down && self.charge_count <= 0 {
            return Step::Disable;
        }
        self.charge_count += if self.charging == Charging::Up { 2 } else { -2 };
        Step::Charged {
            crackle: self.charge_count % 4 == 0 && roll(),
        }
    }

    /// `get_status`: the icon state's prefix.
    pub fn status(&self) -> String {
        if self.broken {
            format!("fix{}", self.broken_state.min(3))
        } else if self.on || self.charging != Charging::Idle {
            "on".to_owned()
        } else {
            "off".to_owned()
        }
    }

    /// The center part's overlay by charge (`process`'s `overlay_state`).
    pub fn overlay(&self) -> Option<&'static str> {
        if self.broken {
            return None;
        }
        match self.charge_count {
            ..=20 => None,
            21..=40 => Some("startup"),
            41..=60 => Some("idle"),
            61..=80 => Some("activating"),
            _ => Some("activated"),
        }
    }

    /// `set_broken`: drained, breaker open (the caller turns it off).
    fn break_down(&mut self) {
        self.broken = true;
        self.charge_count = 0;
        self.breaker = false;
    }
}

/// tg `/obj/machinery/gravity_generator/part`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GravityGeneratorPart {
    pub main: Entity,
    /// 1..9 without 8 (the main part's).
    pub sprite_number: u8,
}

pub struct GravityGeneratorWindow;

impl UiInterface for GravityGeneratorWindow {
    const NAME: &'static str = GravityGeneratorUi::NAME;
    type Data = GravityGeneratorData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gentoggle;

impl ServerUi for GravityGeneratorWindow {
    type Source = &'static GravityGenerator;
    type UserParam = ();
    type Act = Gentoggle;

    fn static_data(_: &GravityGenerator) {}

    fn data(
        g: &GravityGenerator,
        _: Entity,
        _: &SystemParamItem<'_, '_, ()>,
    ) -> GravityGeneratorData {
        GravityGeneratorData {
            breaker: g.breaker,
            charge_count: g.charge_count.clamp(0, 100) as u32,
            charging_state: match g.charging {
                Charging::Idle => 0,
                Charging::Up => 1,
                Charging::Down => 2,
            },
            on: g.on,
            operational: !g.broken,
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<Gentoggle, UiReject> {
        match action {
            "gentoggle" => Ok(Gentoggle),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

pub struct GravityGeneratorPlugin;

impl Plugin for GravityGeneratorPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `attack_hand` / `ui_interact` (the parts pass it on).
        app.add_click_handler(ClickPhase::EmptyHand, 1981, |w, c| {
            interact(w, c.client, c.mob, c.target)
        });
        // tg `item_interaction` (plasteel), `screwdriver_act`, `welder_act`, `wrench_act`.
        app.add_click_handler(ClickPhase::HeldOnTarget, 2871, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, h, c.target))
        });
        app.register_ui::<GravityGeneratorWindow>()
            .add_systems(
                ss13_world::MapInit,
                init_generators.after(super::init_power),
            )
            .add_systems(
                Update,
                (
                    gravgen_acts.in_set(ss13_tgui::server::UiSystems::Handle),
                    (setup_parts, process_generators, looks)
                        .chain()
                        .after(super::PowerSystems),
                ),
            );
    }
}

fn init_generators(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmPower), With<ss13_world::Mapped>>();
    let found: Vec<(Entity, GravityGenerator)> = query
        .iter(world)
        .filter(|(_, dm)| is_under(&dm.path, GRAVGEN_MAIN))
        .map(|(e, dm)| (e, GravityGenerator::of_path(&dm.path)))
        .collect();
    info!("gravity generators: {}", found.len());
    world.insert_batch(found);
}

/// `Initialize`: `setup_parts`, then `enable()` when it starts on, else it registers off (its
/// level is a generator's level either way).
fn setup_parts(world: &mut World) {
    let mut query = world.query::<(Entity, &GravityGenerator, &TilePos)>();
    let pending: Vec<(Entity, TilePos, bool)> = query
        .iter(world)
        .filter(|(_, g, _)| !g.set_up)
        .map(|(e, g, &pos)| (e, pos, g.on))
        .collect();
    for (e, pos, on) in pending {
        let mut parts = Vec::new();
        let mut center = None;
        // `CORNER_BLOCK_OFFSET(our_turf, 3, 3, -1, 0)`: row by row from the bottom left, the
        // main part's tile (8) skipped.
        let mut count = 10u8;
        for dy in 0..3 {
            for dx in -1..=1 {
                count -= 1;
                if (dx, dy) == (0, 0) {
                    continue;
                }
                let at = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
                let Some(part) = crate::construction::spawn_obj(world, GRAVGEN_PART, at) else {
                    continue;
                };
                if count <= 3 {
                    world.entity_mut(part).remove::<Density>();
                } else {
                    world.entity_mut(part).insert(Density);
                }
                world.entity_mut(part).insert(GravityGeneratorPart {
                    main: e,
                    sprite_number: count,
                });
                if count == 5 {
                    center = Some(part);
                }
                parts.push(part);
            }
        }
        if let Some(mut g) = world.get_mut::<GravityGenerator>(e) {
            g.parts = parts;
            g.center = center;
            g.set_up = true;
        }
        if on {
            enable(world, e, false);
        } else {
            update_list(world, e);
        }
    }
}

/// The levels a generator at `z` gives gravity to (`update_list`): every station level for one
/// on the station.
fn levels_of(world: &World, z: u16) -> Vec<u16> {
    let station = crate::events::controller::station_levels(world);
    if station.contains(&z) {
        station.collect()
    } else {
        vec![z]
    }
}

/// `update_list`: the generator into or out of its levels' lists, its field with it.
fn update_list(world: &mut World, e: Entity) {
    let (Some(&pos), Some(on)) = (
        world.get::<TilePos>(e),
        world.get::<GravityGenerator>(e).map(|g| g.on),
    ) else {
        return;
    };
    let levels = levels_of(world, pos.z);
    world.resource_mut::<LevelGravity>().set(&levels, e, on);
    let field = on.then_some(GravityField {
        source: e,
        center: pos,
        range: FIELD_RANGE,
        gravity: FIELD_GRAVITY,
    });
    world.resource_mut::<GravityFields>().set(e, field);
}

fn gravity_in_level(world: &World, e: Entity) -> usize {
    world.get::<TilePos>(e).map_or(0, |pos| {
        world.resource::<LevelGravity>().generators_on(pos.z)
    })
}

/// `enable()`; `shake`: the level's gravity coming back shakes everyone (not at map load).
fn enable(world: &mut World, e: Entity, shake: bool) {
    let Some(mut g) = world.get_mut::<GravityGenerator>(e) else {
        return;
    };
    g.charging = Charging::Idle;
    g.on = true;
    super::update_use_power(world, e, true);
    crate::looping_sound::start(world, e, &crate::looping_sound::defs::GRAVGEN);
    let old = gravity_in_level(world, e);
    update_list(world, e);
    if old == 0 && shake {
        shake_everyone(world, e);
    }
}

/// `disable()`.
fn disable(world: &mut World, e: Entity) {
    let Some(mut g) = world.get_mut::<GravityGenerator>(e) else {
        return;
    };
    g.charging = Charging::Idle;
    g.on = false;
    super::update_use_power(world, e, false);
    crate::looping_sound::stop(world, e, crate::looping_sound::defs::GRAVGEN.path);
    let old = gravity_in_level(world, e);
    update_list(world, e);
    if old > 0 {
        shake_everyone(world, e);
    }
}

/// `shake_everyone`: every mob on the generator's levels has its gravity refreshed, a player's
/// screen shakes with the alarm.
fn shake_everyone(world: &mut World, e: Entity) {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let levels = levels_of(world, pos.z);
    let mut q = world.query_filtered::<(Entity, &TilePos), Or<(
        With<crate::health::Health>,
        With<crate::server::PlayerMob>,
    )>>();
    let mobs: Vec<Entity> = q
        .iter(world)
        .filter(|(_, p)| levels.contains(&p.z))
        .map(|(m, _)| m)
        .collect();
    for mob in mobs {
        crate::spacemove::refresh_weightless(world, mob);
        if let Some(client) = crate::tools::client_of(world, mob) {
            crate::events::earthquake::shake_camera(world, client, 15.0, 1.0);
            crate::events::earthquake::play_local(world, client, ALERT_SOUND, 100.0);
        }
    }
}

/// `set_power` against the generator's area power now.
fn set_power(world: &mut World, e: Entity) {
    let powered = world.get::<Unpowered>(e).is_none();
    if let Some(mut g) = world.get_mut::<GravityGenerator>(e) {
        g.last_powered = Some(powered);
        g.set_power(powered);
    }
}

/// `power_change` (area power seen changed, or first seen) and `process()`, once per machine
/// tick.
fn process_generators(world: &mut World, mut last: Local<u64>) {
    let ticks = world.resource::<MachineTicks>().0;
    if ticks == *last {
        return;
    }
    *last = ticks;
    let mut query = world.query::<(Entity, &GravityGenerator, Has<Unpowered>)>();
    let gens: Vec<(Entity, GravityGenerator, bool)> = query
        .iter(world)
        .filter(|(_, g, _)| g.set_up)
        .map(|(e, g, unpowered)| (e, g.clone(), !unpowered))
        .collect();
    for (e, before, powered) in gens {
        let mut g = before.clone();
        if g.last_powered != Some(powered) {
            g.last_powered = Some(powered);
            g.set_power(powered);
        }
        let step = g.process(|| world.resource_mut::<EngineRng>().0.chance(75.0));
        if g != before
            && let Some(mut live) = world.get_mut::<GravityGenerator>(e)
        {
            *live = g;
        }
        match step {
            Step::Enable => enable(world, e, true),
            Step::Disable => disable(world, e),
            Step::Charged { crackle: true } => {
                if let Some(&pos) = world.get::<TilePos>(e) {
                    crate::sound::playsound(
                        world,
                        crate::sound::PlaySound::new(pos, CHARGE_SOUND, 100.0, true),
                    );
                }
            }
            Step::Charged { crackle: false } | Step::Nothing => {}
        }
    }
}

/// `update_icon_state` of the main part and its parts (`<status>_<sprite_number>`), the charge
/// overlay on the center part.
fn looks(world: &mut World) {
    let mut query =
        world.query_filtered::<(Entity, &GravityGenerator), Changed<GravityGenerator>>();
    let changed: Vec<(Entity, GravityGenerator)> =
        query.iter(world).map(|(e, g)| (e, g.clone())).collect();
    for (e, g) in changed {
        let Some(icon) = world.get::<DmAtom>(e).map(|a| a.icon.clone()) else {
            continue;
        };
        let status = g.status();
        let mut sprites = vec![(e, 8u8)];
        sprites.extend(g.parts.iter().filter_map(|&p| {
            world
                .get::<GravityGeneratorPart>(p)
                .map(|part| (p, part.sprite_number))
        }));
        let overlay = g.overlay();
        let Some(index) = world.get_resource::<IconIndex>() else {
            continue;
        };
        let looks: Vec<(Entity, Option<ss13_shared::IconStateId>, Vec<Overlay>)> = sprites
            .into_iter()
            .map(|(part, n)| {
                let state = index.get(&icon, &format!("{status}_{n}"));
                let overlays = overlay
                    .filter(|_| Some(part) == g.center)
                    .and_then(|o| index.get(&icon, o))
                    .map(Overlay::new)
                    .into_iter()
                    .collect();
                (part, state, overlays)
            })
            .collect();
        for (part, state, overlays) in looks {
            let Some(mut appearance) = world.get_mut::<Appearance>(part) else {
                continue;
            };
            if let Some(state) = state
                && appearance.icon_state != state
            {
                appearance.icon_state = state;
            }
            if appearance.overlays != overlays {
                appearance.overlays = overlays;
            }
        }
    }
}

/// The main part of a click on `target` (itself or one of its parts).
fn main_of(world: &World, target: Entity) -> Option<Entity> {
    if world.get::<GravityGenerator>(target).is_some() {
        return Some(target);
    }
    world
        .get::<GravityGeneratorPart>(target)
        .map(|p| p.main)
        .filter(|&m| world.get::<GravityGenerator>(m).is_some())
}

/// An empty hand opens the window (`INTERACT_MACHINE_OFFLINE`: unpowered or broken too).
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(main) = main_of(world, target) else {
        return false;
    };
    let title = crate::security::name_of(world, main);
    world.write_message(OpenUi::new::<GravityGeneratorWindow>(
        client, mob, main, title,
    ));
    true
}

/// The repair steps of a broken generator. True when handled; anything else on a working one
/// goes on down the chain (`NONE`).
pub fn item_interaction(world: &mut World, client: Entity, held: Entity, target: Entity) -> bool {
    let Some(main) = main_of(world, target) else {
        return false;
    };
    let Some(g) = world.get::<GravityGenerator>(main).cloned() else {
        return false;
    };
    if !g.broken {
        return false;
    }
    let is_plasteel = world
        .get::<DmItem>(held)
        .is_some_and(|i| is_under(&i.path, PLASTEEL));
    if is_plasteel {
        if g.broken_state != 2 {
            return false;
        }
        if crate::tools::stack_amount(world, held) < PLATING_SHEETS {
            tell(
                world,
                client,
                Span::Warning,
                format!("Нужно {PLATING_SHEETS} листов пласстали!"),
            );
            return true;
        }
        crate::tools::use_stack(world, held, PLATING_SHEETS);
        tell(
            world,
            client,
            Span::Notice,
            "Вы добавляете обшивку на каркас.",
        );
        if let Some(&pos) = world.get::<TilePos>(main) {
            crate::sound::playsound(
                world,
                crate::sound::PlaySound::new(pos, CLICK_SOUND, 75.0, true),
            );
        }
        advance(world, main);
        return true;
    }
    let Some((tool, _)) = crate::tools::tool_of(world, held) else {
        return false;
    };
    match (tool, g.broken_state) {
        (ToolKind::Screwdriver, 0) => {
            tell(
                world,
                client,
                Span::Notice,
                "Вы закручиваете винты каркаса.",
            );
            advance(world, main);
            true
        }
        (ToolKind::Welder, 1) => {
            if !crate::tools::tool_usable(world, held, 0, 0.0) {
                let text = crate::tools::unusable_message(world, held, 0);
                tell(world, client, Span::Warning, text);
                return true;
            }
            tell(
                world,
                client,
                Span::Notice,
                "Вы завариваете повреждённый каркас.",
            );
            advance(world, main);
            true
        }
        (ToolKind::Wrench, 3) => {
            tell(
                world,
                client,
                Span::Notice,
                "Вы закрепляете обшивку на каркасе.",
            );
            set_fix(world, main);
            true
        }
        _ => false,
    }
}

fn advance(world: &mut World, main: Entity) {
    if let Some(mut g) = world.get_mut::<GravityGenerator>(main) {
        g.broken_state += 1;
    }
}

/// `set_broken` of the generator (any part breaking breaks it).
pub fn set_broken(world: &mut World, e: Entity) {
    let Some(main) = main_of(world, e) else {
        return;
    };
    let Some(mut g) = world.get_mut::<GravityGenerator>(main) else {
        return;
    };
    if g.broken {
        return;
    }
    g.break_down();
    set_power(world, main);
    disable(world, main);
}

/// `set_fix`: working again, charging up if powered with the breaker on.
fn set_fix(world: &mut World, main: Entity) {
    if let Some(mut g) = world.get_mut::<GravityGenerator>(main) {
        g.broken = false;
        g.broken_state = 0;
    }
    set_power(world, main);
}

/// `blackout()`: drained, breaker open, off.
pub fn blackout(world: &mut World, e: Entity) {
    let Some(mut g) = world.get_mut::<GravityGenerator>(e) else {
        return;
    };
    g.charge_count = 0;
    g.breaker = false;
    set_power(world, e);
    disable(world, e);
}

/// `kickstart()`: the admin "fix gravity".
pub fn kickstart(world: &mut World, e: Entity) {
    let Some(mut g) = world.get_mut::<GravityGenerator>(e) else {
        return;
    };
    g.charge_count = 100;
    g.breaker = true;
    set_power(world, e);
    enable(world, e, true);
}

/// `gravity_generator/ex_act`: only a devastating blast breaks it, nothing destroys it. True when
/// `thing` is a generator or a part.
pub fn ex_act(world: &mut World, thing: Entity, severity: crate::explosions::Severity) -> bool {
    if main_of(world, thing).is_none() {
        return false;
    }
    if matches!(severity, crate::explosions::Severity::Devastate) {
        set_broken(world, thing);
    }
    true
}

/// `examine`: the next repair step of a broken generator.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(g) = main_of(world, target).and_then(|m| world.get::<GravityGenerator>(m)) else {
        return;
    };
    if !g.broken {
        return;
    }
    let text = match g.broken_state {
        0 => "Каркас едва держится, нужно закрутить <b>винты</b>.",
        1 => "На каркасе много повреждённых швов, их нужно <b>заварить</b>.",
        2 => {
            "Часть повреждённой обшивки нужно заменить. <b>10 листов пласстали</b> должно \
              хватить."
        }
        _ => "Новую обшивку осталось только <b>прикрутить</b>.",
    };
    lines.push((Span::Notice, text.to_owned()));
}

/// `ui_act` `gentoggle`: the breaker flips, the charging follows.
fn gravgen_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<GravityGeneratorWindow>>>,
) {
    let pending: Vec<Entity> = acts
        .read(world.resource::<Messages<UiActed<GravityGeneratorWindow>>>())
        .map(|a| a.source)
        .collect();
    for source in pending {
        let Some(mut g) = world.get_mut::<GravityGenerator>(source) else {
            continue;
        };
        g.breaker = !g.breaker;
        set_power(world, source);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charges_two_percent_a_tick_and_switches_at_the_ends() {
        let mut g = GravityGenerator::of_path(GRAVGEN_OFF);
        assert!(!g.on && !g.breaker && g.charge_count == 0);
        assert_eq!(g.process(|| true), Step::Nothing, "idle");
        g.breaker = true;
        g.set_power(true);
        assert_eq!(g.charging, Charging::Up);
        assert_eq!(g.process(|| true), Step::Charged { crackle: false });
        assert_eq!(g.process(|| true), Step::Charged { crackle: true });
        assert_eq!(g.charge_count, 4);
        for _ in 0..48 {
            g.process(|| false);
        }
        assert_eq!(g.charge_count, 100);
        assert_eq!(g.overlay(), Some("activated"));
        assert_eq!(g.process(|| false), Step::Enable);
        // No power: it runs down and switches off at 0.
        g.set_power(false);
        assert_eq!(g.charging, Charging::Down);
        for _ in 0..50 {
            g.process(|| false);
        }
        assert_eq!(g.overlay(), None);
        assert_eq!(g.process(|| false), Step::Disable);
    }

    #[test]
    fn broken_it_stops_and_shows_its_repair_step() {
        let mut g = GravityGenerator::of_path(GRAVGEN_MAIN);
        assert!(g.on && g.breaker && g.charge_count == 100);
        assert_eq!(g.status(), "on");
        g.break_down();
        g.set_power(true);
        assert_eq!(
            g.charging,
            Charging::Down,
            "a broken generator never charges up"
        );
        assert_eq!(g.process(|| true), Step::Nothing);
        g.broken_state = 5;
        assert_eq!(g.status(), "fix3");
    }
}
