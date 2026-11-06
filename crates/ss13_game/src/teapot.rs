//! BandaStation's teapot (server): `/obj/machinery/teapot`
//! (`modular_bandastation/objects/code/machinery/teapot.dm`), built from its board.
//!
//! - Anchored, it takes a drinking glass (`/obj/item/reagent_containers/cup/glass`, out of combat
//!   mode; the old one to the hands); a right click with an empty hand takes it out.
//! - An empty hand with a filled glass inside opens tg's radial menu as a list: the target
//!   temperature up or down by 30 K (273 K at first; up while under 333, down while over 213),
//!   or on. On and powered it moves the glass towards the target like the reaction chamber:
//!   `(target - temp) * 0.45 * seconds * heat_capacity` joules every machine tick (or every
//!   reaction step of a reacting glass), and turns itself off at the target.
//! - While on, nothing but heating; a wrench (2 s) or screwdriver refuses with a glass inside;
//!   the screwdriver and crowbar take it apart like other machines (`crate::machine_frame`).
// shortcut: no AI/cyborg radial access or examine option, no explosion passed to the glass;
// add them with silicon machine access.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmItem, DmObj, DmPower, IconIndex};
use ss13_reagents::Holder;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};

use crate::chem::{Chem, Reagents, name_of};
use crate::chem_dispenser::{InsertedBeaker, beaker_of, replace_beaker};
use crate::obj_damage::{Anchored, is_under};
use crate::power::{MACHINE_TICK_SECS, MachineTicks, Unpowered};
use crate::sound::{PlaySound, playsound};
use crate::storage::{StoredIn, put_inside};
use crate::tools::{Actor, ToolKind, tell, tool_of, use_tool};

pub const TEAPOT: &str = "/obj/machinery/teapot";
const GLASS: &str = "/obj/item/reagent_containers/cup/glass";
const DRINKING_GLASS: &str = "/obj/item/reagent_containers/cup/glass/drinkingglass";
/// `heater_coefficient`, `temperature`.
pub const HEATER_COEFFICIENT: f32 = 0.45;
pub const START_TEMPERATURE: f32 = 273.0;
/// The radial menu's step and limits (`temp_up` while `< 333`, `temp_down` while `> 213`).
pub const STEP: f32 = 30.0;
const UP_BELOW: f32 = 333.0;
const DOWN_ABOVE: f32 = 213.0;
/// `default_unfasten_wrench`, deciseconds.
const WRENCH_DS: f32 = 20.0;
const HISS: &str = "sound/machines/hiss.ogg";
const EJECT: &str = "sound/machines/eject.ogg";
const RATCHET: &str = "sound/items/tools/ratchet.ogg";
const BUSY: &str = "Машина выполняет работу!";

/// A teapot's live state; the glass is its [`InsertedBeaker`].
#[derive(Component, Clone, Copy, Debug, PartialEq)]
#[require(InsertedBeaker)]
pub struct Teapot {
    /// `is_operating`.
    pub operating: bool,
    /// The target temperature, kelvin.
    pub temperature: f32,
}

impl Default for Teapot {
    fn default() -> Self {
        Self {
            operating: false,
            temperature: START_TEMPERATURE,
        }
    }
}

/// The radial menu options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Up,
    Down,
    On,
}

impl Choice {
    fn label(self) -> &'static str {
        match self {
            Self::Up => "Повысить температуру",
            Self::Down => "Понизить температуру",
            Self::On => "Включить",
        }
    }
}

/// On a prompt: the teapot it is the menu of.
#[derive(Component, Clone, Copy, Debug)]
struct MenuFor(Entity);

const CHOICES: [Choice; 3] = [Choice::Up, Choice::Down, Choice::On];

pub struct TeapotPlugin;

impl Plugin for TeapotPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // `ui_interact` (the radial menu) and `attack_hand_secondary` (the glass out).
        app.add_click_handler(ClickPhase::EmptyHand, 1510, |w, c| {
            crate::teapot::interact(w, c.client, c.mob, c.target, c.modifiers.right)
        });
        // `item_interaction`, `wrench_act`, the refusals of `screwdriver_act`/`crowbar_act`
        // (before `crate::machine_frame` takes it apart).
        app.add_click_handler(ClickPhase::HeldOnTarget, 1590, |w, c| {
            c.held
                .is_some_and(|h| crate::teapot::item_interaction(w, c.client, c.mob, h, c.target))
        });
        app.add_systems(
            Startup,
            setup_mapped
                .after(crate::power::init_power)
                .after(crate::mecha::core::MechaLoaded),
        )
        .add_systems(
            Update,
            (
                process_teapots.after(crate::power::PowerSystems),
                menu_answers
                    .in_set(ss13_tgui::server::UiSystems::Handle)
                    .after(ss13_tgui::prompt::PromptSystems::Answer),
            ),
        );
    }
}

fn setup_mapped(world: &mut World) {
    let mut q = world.query::<(Entity, &DmObj)>();
    let found: Vec<Entity> = q
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, TEAPOT))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup(world, e, true);
    }
}

/// `Initialize(mapload)`: a mapped teapot starts with a drinking glass. Also for teapots built
/// from a frame (`crate::machine_frame::setup_built`).
pub fn setup(world: &mut World, e: Entity, mapload: bool) {
    world.entity_mut(e).insert(Teapot::default());
    if mapload
        && let Some(&pos) = world.get::<TilePos>(e)
        && let Some(glass) = crate::construction::spawn_obj(world, DRINKING_GLASS, pos)
    {
        put_inside(world, glass, e);
        if let Some(mut b) = world.get_mut::<InsertedBeaker>(e) {
            b.0 = Some(glass);
        }
    }
    update_look(world, e);
}

fn operational(world: &World, e: Entity) -> bool {
    world.get::<Unpowered>(e).is_none()
}

fn anchored(world: &World, e: Entity) -> bool {
    world.get::<Anchored>(e).is_some_and(|a| a.0)
}

fn operating(world: &World, e: Entity) -> bool {
    world.get::<Teapot>(e).is_some_and(|t| t.operating)
}

/// `update_overlays`: `teapot_cup` with a glass.
fn update_look(world: &mut World, e: Entity) {
    let overlays: Vec<Overlay> = {
        let (Some(atom), Some(icons)) = (world.get::<DmAtom>(e), world.get_resource::<IconIndex>())
        else {
            return;
        };
        beaker_of(world, e)
            .and_then(|_| icons.get(&atom.icon, "teapot_cup"))
            .map(Overlay::new)
            .into_iter()
            .collect()
    };
    if let Some(mut look) = world.get_mut::<Appearance>(e)
        && look.overlays != overlays
    {
        look.overlays = overlays;
    }
}

/// `replace_beaker`: only while anchored; the old glass to the hands.
fn swap_glass(world: &mut World, client: Entity, mob: Entity, e: Entity, new: Option<Entity>) {
    if !anchored(world, e) {
        tell(world, client, Span::Warning, "Прикрутите устройство");
        return;
    }
    replace_beaker(world, mob, e, new);
    update_look(world, e);
    if let Some(new) = new {
        let (what, pot) = (name_of(world, new), name_of(world, e));
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы добавили {what} в {pot}."),
        );
    }
}

/// `item_interaction` and the tool acts. True when the click was the teapot's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    e: Entity,
) -> bool {
    if world.get::<Teapot>(e).is_none() {
        return false;
    }
    // `if(user.combat_mode) return ITEM_INTERACT_SKIP_TO_ATTACK`.
    if world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
    {
        return false;
    }
    if world
        .get::<DmItem>(held)
        .is_some_and(|i| is_under(&i.path, GLASS))
    {
        swap_glass(world, client, mob, e, Some(held));
        return true;
    }
    let Some((kind, _)) = tool_of(world, held) else {
        return false;
    };
    let has_glass = beaker_of(world, e).is_some();
    match kind {
        ToolKind::Wrench | ToolKind::Screwdriver | ToolKind::Crowbar if operating(world, e) => {
            tell(world, client, Span::Warning, BUSY);
            true
        }
        ToolKind::Wrench | ToolKind::Screwdriver if has_glass => {
            tell(world, client, Span::Warning, "Сначала вытащите стакан!");
            true
        }
        ToolKind::Wrench => {
            let anchored = anchored(world, e);
            if let Some(&pos) = world.get::<TilePos>(e) {
                playsound(world, PlaySound::new(pos, RATCHET, 50.0, true));
            }
            let actor = Actor {
                client,
                mob,
                item: Some(held),
            };
            use_tool(
                world,
                actor,
                e,
                WRENCH_DS,
                0,
                crate::construction::Job::Unfasten { anchored },
            );
            true
        }
        // `default_deconstruction_screwdriver` / `_crowbar` (`crate::machine_frame`).
        _ => false,
    }
}

/// `check_interactable`: not heating, the user able and next to it.
fn interactable(world: &World, mob: Entity, e: Entity) -> bool {
    !operating(world, e)
        && world.get::<crate::health::Stat>(mob) == Some(&crate::health::Stat::Conscious)
        && !crate::status_effects::incapacitation(world, mob).incapacitated
        && crate::storage::in_reach(world, mob, e)
}

/// An empty-hand click: the radial menu (left), the glass out (right). True when it was ours.
pub fn interact(world: &mut World, client: Entity, mob: Entity, e: Entity, right: bool) -> bool {
    if world.get::<Teapot>(e).is_none() {
        return false;
    }
    if right {
        if interactable(world, mob, e) && beaker_of(world, e).is_some() {
            swap_glass(world, client, mob, e, None);
        }
        return true;
    }
    let Some(glass) = beaker_of(world, e) else {
        return true;
    };
    if world
        .get::<Reagents>(glass)
        .is_none_or(|r| r.0.total_volume <= 0.0)
    {
        tell(world, client, Span::Notice, "Стакан пуст");
        return true;
    }
    if !operational(world, e) || !anchored(world, e) || !interactable(world, mob, e) {
        return true;
    }
    let items = CHOICES.iter().map(|c| c.label().to_owned()).collect();
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            name_of(world, e),
            "",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(MenuFor(e));
    true
}

/// The menu's answers, revalidated (`custom_check = check_interactable`).
fn menu_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answers {
        let Some(MenuFor(e)) = world.get::<MenuFor>(prompt).copied() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let Some(&choice) = CHOICES.get(i) else {
            continue;
        };
        if world.get_entity(e).is_err() || !interactable(world, actor, e) {
            continue;
        }
        choose(world, client, e, choice);
    }
}

fn choose(world: &mut World, client: Entity, e: Entity, choice: Choice) {
    let Some(mut pot) = world.get::<Teapot>(e).copied() else {
        return;
    };
    let text = match choice {
        Choice::On => {
            pot.operating = true;
            None
        }
        Choice::Up if pot.temperature < UP_BELOW => {
            pot.temperature += STEP;
            Some(format!("Температура: {}", pot.temperature))
        }
        Choice::Up => Some("Это максимальная температура".to_owned()),
        Choice::Down if pot.temperature > DOWN_ABOVE => {
            pot.temperature -= STEP;
            Some(format!("Температура: {}", pot.temperature))
        }
        Choice::Down => Some("Это минимальная температура".to_owned()),
    };
    world.entity_mut(e).insert(pot);
    match text {
        Some(text) => tell(world, client, Span::Notice, text),
        None => {
            if let Some(&pos) = world.get::<TilePos>(e) {
                playsound(world, PlaySound::new(pos, HISS, 40.0, false));
            }
        }
    }
}

/// `heat_reagents`' step: the energy moved towards `target`.
pub fn heat(chem: &ss13_reagents::Chemistry, target: f32, glass: &mut Holder, seconds: f32) -> f32 {
    let energy =
        (target - glass.chem_temp) * HEATER_COEFFICIENT * seconds * glass.heat_capacity(chem);
    glass.adjust_thermal_energy(
        chem,
        energy,
        0.0,
        ss13_reagents::CHEMICAL_MAXIMUM_TEMPERATURE,
    );
    energy
}

/// `heat_reagents` on the teapot's glass; `true` when it heated.
fn heat_glass(world: &mut World, e: Entity, glass: Entity, seconds: f32, react: bool) -> bool {
    let Some(pot) = world.get::<Teapot>(e).copied() else {
        return false;
    };
    if !pot.operating || !operational(world, e) {
        return false;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return false;
    };
    let Some(mut holder) = world.get::<Reagents>(glass).map(|r| r.0.clone()) else {
        return false;
    };
    if holder.total_volume <= 0.0 {
        return false;
    }
    let energy = heat(&chem, pot.temperature, &mut holder, seconds);
    // DM stops on `temperature == chem_temp`; the float approach only gets within rounding.
    let done = (pot.temperature - holder.chem_temp).abs() < 0.01;
    if done {
        holder.chem_temp = pot.temperature;
    }
    if react {
        holder.handle_reactions(&chem);
    }
    world.entity_mut(glass).insert(Reagents(holder));
    let active = world
        .get::<DmPower>(e)
        .map_or(0.0, |p| p.active_power_usage);
    crate::kitchen::machine_use_energy(world, e, active + f64::from(energy.ceil().abs() / 120.0));
    if done {
        world.entity_mut(e).insert(Teapot {
            operating: false,
            ..pot
        });
        if let Some(&pos) = world.get::<TilePos>(e) {
            playsound(world, PlaySound::new(pos, EJECT, 40.0, false));
        }
    }
    true
}

/// tg `on_reaction_step`: a reacting glass inside a teapot is heated with every reaction step
/// (from `crate::chem`'s reaction processing).
pub fn reaction_step(world: &mut World, glass: Entity, seconds: f32) {
    let Some(e) = world.get::<StoredIn>(glass).map(|s| s.0) else {
        return;
    };
    if world.get::<Teapot>(e).is_none() || beaker_of(world, e) != Some(glass) {
        return;
    }
    heat_glass(world, e, glass, seconds, false);
}

/// `process`: every machine tick a glass that is not reacting is heated and its reactions start.
// Cost: one pass over the teapots per machine tick (2 s).
fn process_teapots(world: &mut World, mut last: Local<Option<u64>>) {
    let ticks = world.resource::<MachineTicks>().0;
    let from = last.unwrap_or(ticks);
    *last = Some(ticks);
    if from == ticks {
        return;
    }
    let mut q = world.query_filtered::<Entity, With<Teapot>>();
    let pots: Vec<Entity> = q.iter(world).collect();
    for _ in from..ticks {
        for &e in &pots {
            let Some(glass) = beaker_of(world, e) else {
                continue;
            };
            if world
                .get::<Reagents>(glass)
                .is_some_and(|r| r.0.is_reacting)
            {
                continue;
            }
            heat_glass(world, e, glass, MACHINE_TICK_SECS as f32, true);
        }
    }
}

/// tg `examine` (in range).
pub fn examine_lines(world: &World, e: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(pot) = world.get::<Teapot>(e) else {
        return;
    };
    match beaker_of(world, e).and_then(|g| world.get::<Reagents>(g)) {
        Some(r) => {
            lines.push((
                Span::Notice,
                format!("Целевая температура устройства: {}°К", pot.temperature),
            ));
            if r.0.total_volume > 0.0 {
                lines.push((
                    Span::Notice,
                    format!(
                        "Температура содержимого стакана: {}°К",
                        r.0.chem_temp.round()
                    ),
                ));
            }
            lines.push((
                Span::Notice,
                format!(
                    "Стакан размером в {}u вставлен, в нём {}u.",
                    r.0.maximum_volume, r.0.total_volume
                ),
            ));
        }
        None => lines.push((Span::Warning, "Нет стакана.".to_owned())),
    }
    if anchored(world, e) {
        lines.push((Span::Notice, "Машина может быть откручена.".to_owned()));
    } else {
        lines.push((
            Span::Warning,
            "Машина должна быть прикручена к полу для работы.".to_owned(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_reagents::{ChemData, Chemistry, Owner, ReagentDef, flags};

    fn water() -> ReagentDef {
        ReagentDef {
            path: "/datum/reagent/water".into(),
            name: "Water".into(),
            description: String::new(),
            color: "#AAAAAA".into(),
            taste_description: String::new(),
            taste_mult: 1.0,
            metabolization_rate: 0.2,
            overdose_threshold: 0.0,
            ph: 7.0,
            specific_heat: 200.0,
            chemical_flags: 0,
            purity: 1.0,
            creation_purity: 1.0,
            inverse_chem: None,
            inverse_chem_val: 0.25,
            purge_multiplier: 1.0,
            toxpwr: 0.0,
            health_required: 0.0,
            liver_tolerance_multiplier: 1.0,
            boozepwr: 0.0,
            brute_heal: 0.0,
            nutriment_factor: 0.0,
            default_container: String::new(),
        }
    }

    #[test]
    fn heats_by_the_teapot_coefficient() {
        let chem = Chemistry::new(ChemData {
            reagents: vec![water()],
            ..Default::default()
        });
        let mut h = Holder::new(50.0, flags::OPENCONTAINER, Owner::default());
        h.add_reagent(&chem, 0, 50.0, 273.0, None, None, true, false);
        // One machine tick towards 333 K: (333 - 273) * 0.45 * 2 s = 54 K.
        heat(&chem, 333.0, &mut h, 2.0);
        assert!((h.chem_temp - 327.0).abs() < 1e-3, "{}", h.chem_temp);
        // A few more ticks reach the target (within the stop margin).
        for _ in 0..8 {
            heat(&chem, 333.0, &mut h, 2.0);
        }
        assert!((h.chem_temp - 333.0).abs() < 0.01, "{}", h.chem_temp);
    }

    #[test]
    fn menu_limits_follow_the_dm_steps() {
        // DM `if(temperature < 333) += 30`: 273 -> 303 -> 333, then refused; down to 213.
        let mut t = START_TEMPERATURE;
        let mut ups = 0;
        while t < UP_BELOW {
            t += STEP;
            ups += 1;
        }
        assert_eq!((t, ups), (333.0, 2));
        let mut t = START_TEMPERATURE;
        while t > DOWN_ABOVE {
            t -= STEP;
        }
        assert_eq!(t, 213.0);
    }
}
