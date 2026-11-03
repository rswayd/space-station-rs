//! The crystallizer: tg `/obj/machinery/atmospherics/components/binary/crystallizer`
//! (`machinery/components/gas_recipe_machines/crystallizer.dm`) with the
//! `/datum/gas_recipe/crystallizer` recipes ([`CRYSTALLIZER_RECIPES`]) and its `Crystallizer`
//! window.
//!
//! - A binary machine: port 1 (behind it) is the coolant, port 2 (in front) the gas input. On,
//!   powered and with a recipe, every SSair fire it draws the recipe's gases from the input into
//!   its internal mixture (`gas_input` of each, until it holds twice the recipe), exchanges heat
//!   with the coolant, and while it holds the whole recipe within 90%-110% of the temperature
//!   range it advances the progress and releases the recipe's energy; out of range the progress
//!   slips back and the quality drops. At 100% it consumes the recipe (more the worse the
//!   quality) and drops the products south, named after their quality.
//! - Ctrl-click turns it on or off (not with the panel open); a screwdriver opens the panel
//!   while it is off; with the panel open a wrench turns it clockwise.
//! - Changing the recipe dumps the internal mixture into the input port.
// shortcut: no crowbar deconstruction (machine frames: docs/parity/engineering/machines.md);
// the supermatter shard product is a plain spawned object (supermatter shards spawned at
// runtime are not set up: docs/parity/power/supermatter.md); dangerous products are logged, not
// reported to admins.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_atmos::gas::TCMB;
use ss13_atmos::pipenet::Pipenet;
use ss13_atmos::reactions::MINIMUM_MOLE_COUNT;
use ss13_atmos::{GasId, GasMixture};
use ss13_chat::Span;
use ss13_mapload::atmos_machine::GasRecipe;
use ss13_mapload::{DmAtmosDevice, DmAtom, IconIndex};
use ss13_shared::{Appearance, Dir, Facing, Overlay, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::crystallizer::{
    CrystallizerData, CrystallizerStatic, CrystallizerUi, InternalGas, RecipeEntry,
};
use ss13_tgui::server::{OpenUi, ServerUi, UiActed, UiParams, UiReject};

use super::AtmosDevice;
use crate::obj_damage::is_under;
use crate::power::Unpowered;
use crate::tools::{ToolKind, tell};

pub use ss13_mapload::atmos_machine::{CRYSTALLIZER, CRYSTALLIZER_RECIPES};

const MIN_PROGRESS_AMOUNT: f64 = 3.0;
const MIN_DEVIATION_RATE: f64 = 0.90;
const MAX_DEVIATION_RATE: f64 = 1.1;
const HIGH_CONDUCTIVITY_RATIO: f64 = 0.95;
/// `gas_input = clamp(params["gas_input"], 0, 250)`.
const MAX_GAS_INPUT: f64 = 250.0;
const UNDERLAYS: &str = "icons/obj/pipes_n_cables/pipe_underlays.dmi";
/// `COLOR_LIME` (the input side) and `COLOR_MOSTLY_PURE_RED` (the coolant side).
const LIME: [u8; 4] = [0x32, 0xCD, 0x32, 255];
const RED: [u8; 4] = [0xFF, 0x33, 0x00, 255];

#[derive(Component, Clone, Debug, PartialEq)]
pub struct Crystallizer {
    pub on: bool,
    pub panel_open: bool,
    /// The share of each recipe gas in the input port drawn in per fire.
    // open question: tg labels it "moles/s" (0-250) in the window and "moles per tick" in the
    // var's comment, yet `inject_gases` takes `moles * gas_input`, so 0.1-0.9 is a share and
    // anything from 1 up takes everything. Ported as tg does until we decide which was meant.
    pub gas_input: f64,
    /// `progress_bar`, 0-100.
    pub progress: f64,
    /// `quality_loss`, -85-100.
    pub quality_loss: f64,
    /// Index into [`CRYSTALLIZER_RECIPES`].
    pub recipe: Option<usize>,
    /// `internal`.
    pub internal: GasMixture,
}

impl Crystallizer {
    pub fn new(on: bool) -> Self {
        Self {
            on,
            panel_open: false,
            gas_input: 0.0,
            progress: 0.0,
            quality_loss: 0.0,
            recipe: None,
            internal: GasMixture::vacuum(),
        }
    }
}

pub fn init_crystallizers(
    mut commands: Commands,
    added: Query<(Entity, &DmAtmosDevice), Added<DmAtmosDevice>>,
) {
    for (e, dm) in &added {
        if is_under(&dm.path, CRYSTALLIZER) {
            commands.entity(e).insert(Crystallizer::new(dm.on));
        }
    }
}

fn gases(recipe: &GasRecipe) -> impl Iterator<Item = (GasId, f64)> + '_ {
    recipe
        .requirements
        .iter()
        .filter_map(|&(g, n)| Some((GasId::parse(g)?, n)))
}

/// `remove_specific`: up to `amount` moles of one gas, at the mixture's temperature.
fn remove_specific(air: &mut GasMixture, gas: GasId, amount: f64) -> Option<GasMixture> {
    let amount = amount.min(air.get(gas));
    if amount <= 0.0 {
        return None;
    }
    let mut removed = GasMixture::vacuum();
    removed.temperature = air.temperature;
    removed.set(gas, amount);
    air.add(gas, -amount);
    air.garbage_collect();
    Some(removed)
}

/// `moles_calculations`: the recipe's total moles.
fn total_moles(recipe: &GasRecipe) -> f64 {
    recipe.requirements.iter().map(|(_, n)| n).sum()
}

/// DM `round(log(10, total * 0.1), 0.01)`, the divisor of the progress and quality steps.
fn step_divisor(recipe: &GasRecipe) -> f64 {
    ((total_moles(recipe) * 0.1).log10() * 100.0).round() / 100.0
}

/// What one fire did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fire {
    /// Some gas mixture changed (`update_parents`).
    pub changed: bool,
    /// The recipe completed with this `total_quality`.
    pub produced: Option<f64>,
}

/// `process_atmos` once the machine is on, operational and has a recipe: `coolant` and `input`
/// are its two ports.
pub fn crystallize(
    c: &mut Crystallizer,
    recipe: &GasRecipe,
    coolant: &mut GasMixture,
    input: &mut GasMixture,
) -> Fire {
    let mut fire = Fire::default();
    // `inject_gases`.
    for (gas, required) in gases(recipe) {
        if input.get(gas) == 0.0 || c.internal.get(gas) >= required * 2.0 {
            continue;
        }
        if let Some(taken) = remove_specific(input, gas, input.get(gas) * c.gas_input) {
            c.internal.merge_raw(&taken);
            fire.changed = true;
        }
    }
    if c.internal.total_moles() == 0.0 {
        return fire;
    }
    // `heat_conduction`.
    if coolant.total_moles() > MINIMUM_MOLE_COUNT {
        let delta = coolant.temperature - c.internal.temperature;
        let coolant_capacity = coolant.heat_capacity_raw();
        let internal_capacity = c.internal.heat_capacity_raw();
        let heat = HIGH_CONDUCTIVITY_RATIO
            * delta
            * (coolant_capacity * internal_capacity / (coolant_capacity + internal_capacity));
        coolant.temperature = (coolant.temperature - heat / coolant_capacity).max(TCMB);
        c.internal.temperature = (c.internal.temperature + heat / internal_capacity).max(TCMB);
        fire.changed = true;
    }
    let divisor = step_divisor(recipe);
    // `internal_check`: every recipe gas, at least the recipe's moles of each.
    let complete = gases(recipe).all(|(gas, required)| c.internal.get(gas) >= required);
    if complete {
        let t = c.internal.temperature;
        let (min, max) = (recipe.min_temp, recipe.max_temp);
        if t >= min * MIN_DEVIATION_RATE && t <= max * MAX_DEVIATION_RATE {
            // `heat_calculations`.
            let step = MIN_PROGRESS_AMOUNT * 4.5 / divisor;
            if (t >= min * MIN_DEVIATION_RATE && t <= min)
                || (t >= max && t <= max * MAX_DEVIATION_RATE)
            {
                c.quality_loss = (c.quality_loss + step).min(100.0);
            }
            let median = (max + min) / 2.0;
            if t >= median * MIN_DEVIATION_RATE && t <= median * MAX_DEVIATION_RATE {
                c.quality_loss = (c.quality_loss - step).max(-85.0);
            }
            c.internal.temperature = (c.internal.temperature
                + recipe.energy_release / c.internal.heat_capacity_raw())
            .max(TCMB);
            fire.changed = true;
            c.progress = (c.progress + MIN_PROGRESS_AMOUNT * 5.0 / divisor).min(100.0);
        } else {
            c.quality_loss = (c.quality_loss + 0.5).min(100.0);
            c.progress = (c.progress - 1.0).max(0.0);
        }
    }
    fire.changed = true;
    if c.progress != 100.0 {
        return fire;
    }
    c.progress = 0.0;
    for (gas, required) in gases(recipe) {
        let consumed = required + required * (c.quality_loss * 0.01);
        if c.internal.get(gas) < consumed {
            c.quality_loss = (c.quality_loss + 10.0).min(100.0);
        }
        remove_specific(&mut c.internal, gas, consumed);
    }
    fire.produced = Some((50.0 - c.quality_loss).clamp(0.0, 100.0));
    c.quality_loss = 0.0;
    fire
}

/// `quality_control` of a `total_quality`. tg switches on whole-number ranges, so a fractional
/// quality (most of them) named its product " crystal" with no word; here each range runs up to
/// the next one (баг не перенесён).
pub fn quality_name(quality: f64) -> &'static str {
    match quality {
        q if q >= 100.0 => "Masterwork",
        q if q >= 95.0 => "Supreme",
        q if q >= 75.0 => "Good",
        q if q >= 65.0 => "Decent",
        q if q >= 55.0 => "Average",
        q if q >= 35.0 => "Ok",
        q if q >= 15.0 => "Poor",
        q if q >= 5.0 => "Ugly",
        q if q >= 1.0 => "Cracked",
        _ => "Oh God why",
    }
}

/// `process_atmos` of every crystallizer.
pub fn process_crystallizers(world: &mut World) {
    let mut q = world.query::<(
        Entity,
        &Crystallizer,
        &AtmosDevice,
        &TilePos,
        Has<Unpowered>,
        Option<&crate::obj_damage::Integrity>,
    )>();
    let running: Vec<(Entity, usize, ss13_atmos::pipenet::DeviceId, TilePos)> = q
        .iter(world)
        .filter(|(_, c, _, _, unpowered, integrity)| {
            c.on && !unpowered && !integrity.is_some_and(|i| i.broken)
        })
        .filter_map(|(e, c, d, &pos, ..)| Some((e, c.recipe?, d.id, pos)))
        .collect();
    for (e, index, id, pos) in running {
        let recipe = &CRYSTALLIZER_RECIPES[index];
        let Some(mut c) = world.get::<Crystallizer>(e).cloned() else {
            continue;
        };
        let fire = world.resource_scope(|_, mut net: Mut<Pipenet>| {
            let Some([coolant, input]) = net.airs_mut(id) else {
                return Fire::default();
            };
            let fire = crystallize(&mut c, recipe, &mut coolant.gas, &mut input.gas);
            if fire.changed {
                net.update_parents(id);
            }
            fire
        });
        if let Some(mut stored) = world.get_mut::<Crystallizer>(e) {
            *stored = c;
        }
        if let Some(quality) = fire.produced {
            produce(world, recipe, pos.step(Dir::South), quality);
        }
    }
}

/// The recipe's products south of the machine, named after their quality.
fn produce(world: &mut World, recipe: &GasRecipe, pos: TilePos, quality: f64) {
    let word = quality_name(quality);
    for &(path, count) in recipe.products {
        for _ in 0..count {
            let Some(item) = crate::construction::spawn_obj(world, path, pos) else {
                warn!("crystallizer: no prototype for {path}");
                continue;
            };
            if let Some(mut name) = world.get_mut::<ss13_mapload::DmName>(item) {
                name.name = format!("{word} {}", name.name);
            }
            if recipe.dangerous {
                warn!("crystallizer: {word} {path} created at {pos:?}");
            }
        }
    }
}

/// `update_icon_state` and the two pipe overlays (lime toward the input, red toward the
/// coolant), drawn under the machine.
pub fn crystallizer_looks(
    icons: Option<Res<IconIndex>>,
    mut q: Query<(
        &Crystallizer,
        &AtmosDevice,
        &DmAtom,
        Has<Unpowered>,
        Option<&crate::obj_damage::Integrity>,
        &mut Appearance,
    )>,
) {
    let Some(icons) = icons else {
        return;
    };
    for (c, device, atom, unpowered, integrity, mut appearance) in &mut q {
        let operational = !unpowered && !integrity.is_some_and(|i| i.broken);
        let state = if c.panel_open {
            "crystallizer-open"
        } else if c.on && operational {
            "crystallizer-on"
        } else {
            "crystallizer-off"
        };
        let Some(sprite) = icons.get(&atom.icon, state) else {
            continue;
        };
        let pipe = |dir: u8| icons.get(UNDERLAYS, &format!("intact_{dir}_{}", device.layer));
        let (Some(input), Some(coolant)) = (pipe(device.dir), pipe(super::reverse_dir(device.dir)))
        else {
            continue;
        };
        let overlay = |icon_state, color: [u8; 4], dir| Overlay {
            color: Some(color),
            dir,
            ..Overlay::new(icon_state)
        };
        let mut new = appearance.clone();
        new.icon_state = input;
        new.color = Some(LIME);
        new.overlays = vec![
            overlay(coolant, RED, Some(Dir::South)),
            overlay(sprite, [255; 4], None),
        ];
        if *appearance != new {
            *appearance = new;
        }
    }
}

fn toggle(world: &mut World, client: Entity, e: Entity) {
    let Some(mut c) = world.get_mut::<Crystallizer>(e) else {
        return;
    };
    c.on = !c.on;
    let text = if c.on {
        "включено"
    } else {
        "выключено"
    };
    tell(world, client, Span::Notice, text);
}

fn operational(world: &World, e: Entity) -> bool {
    world.get::<Unpowered>(e).is_none()
        && !world
            .get::<crate::obj_damage::Integrity>(e)
            .is_some_and(|i| i.broken)
}

/// `click_ctrl`.
pub fn crystallizer_ctrl_clicks(
    world: &mut World,
    mut clicks: Local<MessageCursor<crate::click::CtrlClick>>,
) {
    let pending: Vec<(Entity, Entity)> = clicks
        .read(world.resource::<Messages<crate::click::CtrlClick>>())
        .map(|c| (c.client, c.target))
        .collect();
    for (client, target) in pending {
        let Some(c) = world.get::<Crystallizer>(target) else {
            continue;
        };
        if !operational(world, target) {
            continue;
        }
        if c.panel_open {
            tell(world, client, Span::Warning, "Сначала закройте панель!");
            continue;
        }
        toggle(world, client, target);
    }
}

/// An empty hand: the window. True for a crystallizer.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<Crystallizer>(target).is_none() {
        return false;
    }
    let title = world
        .get::<ss13_mapload::DmName>(target)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<CrystallizerWindow>(
        client, mob, target, title,
    ));
    true
}

/// `screwdriver_act` (off only) and `wrench_act` (`default_change_direction_wrench`, panel
/// open). True when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    target: Entity,
    tool: Option<ToolKind>,
) -> bool {
    let Some(c) = world.get::<Crystallizer>(target) else {
        return false;
    };
    let (on, open) = (c.on, c.panel_open);
    match tool {
        Some(ToolKind::Screwdriver) if !on => {
            if let Some(mut c) = world.get_mut::<Crystallizer>(target) {
                c.panel_open = !open;
            }
            let text = if open {
                "панель обслуживания закрыта"
            } else {
                "панель обслуживания открыта"
            };
            tell(world, client, Span::Notice, text);
            true
        }
        Some(ToolKind::Wrench) if open => {
            if rotate(world, target) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы поворачиваете кристаллизатор.",
                );
            }
            true
        }
        _ => false,
    }
}

/// `setDir(turn(dir, -90))`, then `reconnect_nodes`.
fn rotate(world: &mut World, e: Entity) -> bool {
    let Some(device) = world.get::<AtmosDevice>(e).copied() else {
        return false;
    };
    let dir = super::turn_cw(device.dir);
    let axis = if matches!(dir, 1 | 2) { 1 | 2 } else { 4 | 8 };
    let mut net = world.resource_mut::<Pipenet>();
    let Some(color) = net.device(device.id).map(|d| d.color) else {
        return false;
    };
    let Some(id) = net.reconnect(
        device.id,
        axis,
        vec![super::reverse_dir(dir), dir],
        device.layer,
        color,
    ) else {
        return false;
    };
    let mut entity = world.entity_mut(e);
    entity.insert(AtmosDevice { id, dir, ..device });
    if let Some(facing) = Dir::from_bits(dir) {
        entity.insert(Facing(facing));
    }
    true
}

pub struct CrystallizerWindow;

impl UiInterface for CrystallizerWindow {
    const NAME: &'static str = CrystallizerUi::NAME;
    type Data = CrystallizerData;
    type StaticData = CrystallizerStatic;
}

#[derive(Clone, Debug, PartialEq)]
pub enum CrystallizerAct {
    Power,
    /// A recipe index, `None` for "Nothing".
    Recipe(Option<usize>),
    GasInput(f64),
}

/// DM's `"[number]"`: 6 significant digits, exponent from a million up (`1e+31`).
fn dm_text(n: f64) -> String {
    if n.abs() >= 1e6 {
        let exp = n.abs().log10().floor() as i32;
        let mantissa = crate::chem::dm_num((n / 10f64.powi(exp)) as f32);
        return format!("{mantissa}e+{exp:02}");
    }
    crate::chem::dm_num(n as f32)
}

/// `ui_data["requirements"]`.
pub fn requirements_text(recipe: Option<&GasRecipe>) -> String {
    let Some(recipe) = recipe else {
        return "Select a recipe to see the requirements".into();
    };
    let mut lines = vec![format!("To create {} you will need:", recipe.name)];
    for (gas, moles) in gases(recipe) {
        lines.push(format!("-{} moles of {}", dm_text(moles), gas.info().name));
    }
    lines.push(format!(
        "In a temperature range between {} K and {} K",
        dm_text(recipe.min_temp),
        dm_text(recipe.max_temp)
    ));
    let kind = if recipe.energy_release == 0.0 {
        "thermally neutral"
    } else if recipe.energy_release > 0.0 {
        "exothermic"
    } else {
        "endothermic"
    };
    lines.push(format!("The crystallization reaction will be {kind}"));
    lines.join("\n")
}

impl ServerUi for CrystallizerWindow {
    type Source = &'static Crystallizer;
    type UserParam = ();
    type Act = CrystallizerAct;

    fn static_data(_: &Crystallizer) -> CrystallizerStatic {
        let nothing = RecipeEntry {
            name: "Nothing".into(),
            id: String::new(),
        };
        CrystallizerStatic {
            selected_recipes: std::iter::once(nothing)
                .chain(CRYSTALLIZER_RECIPES.iter().map(|r| RecipeEntry {
                    name: r.name.into(),
                    id: r.id.into(),
                }))
                .collect(),
        }
    }

    fn data(c: &Crystallizer, _: Entity, _: &SystemParamItem<'_, '_, ()>) -> CrystallizerData {
        let recipe = c.recipe.map(|i| &CRYSTALLIZER_RECIPES[i]);
        let has_gas = c.internal.total_moles() > 0.0;
        CrystallizerData {
            on: c.on,
            selected: recipe.map_or("", |r| r.id).to_owned(),
            internal_gas_data: ss13_atmos::GASES
                .iter()
                .zip(c.internal.moles)
                .filter(|(_, m)| *m > 0.0)
                .map(|(g, m)| InternalGas {
                    name: g.name.into(),
                    id: g.id.into(),
                    amount: ((m * 100.0).round() / 100.0) as f32,
                })
                .collect(),
            requirements: requirements_text(recipe),
            internal_temperature: if has_gas {
                c.internal.temperature as f32
            } else {
                0.0
            },
            progress_bar: c.progress as f32,
            gas_input: c.gas_input as f32,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<CrystallizerAct, UiReject> {
        match action {
            "power" => Ok(CrystallizerAct::Power),
            "recipe" => {
                let id = params
                    .get("mode")
                    .ok_or_else(|| UiReject::BadParams("mode".into()))?;
                if id.is_empty() {
                    return Ok(CrystallizerAct::Recipe(None));
                }
                CRYSTALLIZER_RECIPES
                    .iter()
                    .position(|r| r.id == id)
                    .map(|i| CrystallizerAct::Recipe(Some(i)))
                    .ok_or_else(|| UiReject::BadParams("mode".into()))
            }
            "gas_input" => Ok(CrystallizerAct::GasInput(params.number("gas_input")?)),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

pub fn crystallizer_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<CrystallizerWindow>>>,
) {
    let pending: Vec<(Entity, Entity, CrystallizerAct)> = acts
        .read(world.resource::<Messages<UiActed<CrystallizerWindow>>>())
        .map(|a| (a.client, a.source, a.act.clone()))
        .collect();
    for (client, source, act) in pending {
        match act {
            CrystallizerAct::Power => toggle(world, client, source),
            CrystallizerAct::GasInput(v) => {
                if let Some(mut c) = world.get_mut::<Crystallizer>(source) {
                    c.gas_input = v.clamp(0.0, MAX_GAS_INPUT);
                }
            }
            CrystallizerAct::Recipe(recipe) => select_recipe(world, source, recipe),
        }
    }
}

/// `ui_act("recipe")`: the internal mixture back into the input port, a fresh start.
pub fn select_recipe(world: &mut World, e: Entity, recipe: Option<usize>) {
    let Some(id) = world.get::<AtmosDevice>(e).map(|d| d.id) else {
        return;
    };
    let Some(mut c) = world.get::<Crystallizer>(e).cloned() else {
        return;
    };
    if c.internal.total_moles() > 0.0 {
        let all = std::mem::replace(&mut c.internal, GasMixture::vacuum());
        let mut net = world.resource_mut::<Pipenet>();
        if let Some([_, input]) = net.airs_mut(id) {
            input.merge(&all);
        }
        net.update_parents(id);
    }
    c.quality_loss = 0.0;
    c.progress = 0.0;
    c.recipe = recipe;
    if let Some(mut stored) = world.get_mut::<Crystallizer>(e) {
        *stored = c;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipe(id: &str) -> &'static GasRecipe {
        CRYSTALLIZER_RECIPES.iter().find(|r| r.id == id).unwrap()
    }

    fn index(id: &str) -> usize {
        CRYSTALLIZER_RECIPES
            .iter()
            .position(|r| r.id == id)
            .unwrap()
    }

    #[test]
    fn gas_input_is_a_share_and_stops_at_twice_the_recipe() {
        let r = recipe("plasma_sheet");
        let mut c = Crystallizer::new(true);
        c.recipe = Some(index("plasma_sheet"));
        c.gas_input = 0.5;
        let mut input = GasMixture::vacuum();
        input.set(GasId::PLASMA, 400.0);
        input.temperature = 15.0;
        let mut coolant = GasMixture::vacuum();
        crystallize(&mut c, r, &mut coolant, &mut input);
        assert_eq!(c.internal.get(GasId::PLASMA), 200.0);
        assert_eq!(input.get(GasId::PLASMA), 200.0);
        // Anything from 1 up takes everything (tg).
        c.gas_input = 7.0;
        crystallize(&mut c, r, &mut coolant, &mut input);
        assert_eq!(input.get(GasId::PLASMA), 0.0);
        // At twice the recipe (900) nothing more comes in.
        let mut c = Crystallizer::new(true);
        c.internal.set(GasId::PLASMA, 900.0);
        c.internal.temperature = 15.0;
        c.gas_input = 1.0;
        input.set(GasId::PLASMA, 50.0);
        crystallize(&mut c, r, &mut coolant, &mut input);
        assert_eq!(input.get(GasId::PLASMA), 50.0);
    }

    #[test]
    fn in_range_it_progresses_and_completes_with_quality() {
        // Solidified plasma: 450 mol, 10-20 K, median 15 K; log10(45) rounds to 1.65.
        let r = recipe("plasma_sheet");
        assert_eq!(step_divisor(r), 1.65);
        let mut c = Crystallizer::new(true);
        c.internal.set(GasId::PLASMA, 1000.0);
        let (mut coolant, mut input) = (GasMixture::vacuum(), GasMixture::vacuum());
        let mut fires = 0;
        let produced = loop {
            // A perfect coolant: the internal mixture held at the median every fire.
            c.internal.temperature = 15.0;
            fires += 1;
            if let Some(q) = crystallize(&mut c, r, &mut coolant, &mut input).produced {
                break q;
            }
            assert!(fires < 100);
        };
        // 15 / 1.65 = 9.09 per fire: 11 fires to 100.
        assert_eq!(fires, 11);
        // Each fire at the median lowers the loss by 13.5 / 1.65 = 8.18, to the -85 floor: the
        // quality caps at 100 and the recipe costs 450 * (1 - 0.85).
        assert_eq!(produced, 100.0);
        assert!((c.internal.get(GasId::PLASMA) - (1000.0 - 450.0 * 0.15)).abs() < 1e-6);
        assert_eq!((c.progress, c.quality_loss), (0.0, 0.0));
        assert_eq!(quality_name(produced), "Masterwork");
    }

    #[test]
    fn out_of_range_it_slips_and_coolant_exchanges_heat() {
        let r = recipe("plasma_sheet");
        let mut c = Crystallizer::new(true);
        c.internal.set(GasId::PLASMA, 500.0);
        c.internal.temperature = 300.0;
        c.progress = 10.0;
        let mut coolant = GasMixture::vacuum();
        coolant.set(GasId::N2, 1000.0);
        coolant.temperature = 3.0;
        let mut input = GasMixture::vacuum();
        let fire = crystallize(&mut c, r, &mut coolant, &mut input);
        assert!(fire.changed && fire.produced.is_none());
        assert!(c.internal.temperature < 300.0 && coolant.temperature > 3.0);
        assert_eq!((c.progress, c.quality_loss), (9.0, 0.5));
    }

    #[test]
    fn quality_words_cover_fractions() {
        assert_eq!(quality_name(94.5), "Good");
        assert_eq!(quality_name(0.4), "Oh God why");
        assert_eq!(quality_name(50.0), "Ok");
    }

    #[test]
    fn requirements_read_like_tg() {
        assert_eq!(
            requirements_text(Some(recipe("fuel_basic"))),
            "To create Standard Fuel Pellet you will need:\n-50 moles of Oxygen\n\
             -100 moles of Plasma\nIn a temperature range between 2.7 K and 1e+31 K\n\
             The crystallization reaction will be endothermic"
        );
        assert_eq!(
            requirements_text(None),
            "Select a recipe to see the requirements"
        );
        assert_eq!(dm_text(150_000.0), "150000");
        assert_eq!(dm_text(9_500_000.0), "9.5e+06");
    }

    #[test]
    fn acts_stay_inside_the_recipes() {
        let p = |k: &str, v: &str| {
            let owned = vec![(k.to_owned(), v.to_owned())];
            owned
        };
        let parse = |a: &str, owned: Vec<(String, String)>| {
            CrystallizerWindow::parse_act(a, &UiParams::new(&owned))
        };
        assert_eq!(
            parse("recipe", p("mode", "diamond")),
            Ok(CrystallizerAct::Recipe(Some(index("diamond"))))
        );
        assert_eq!(
            parse("recipe", p("mode", "")),
            Ok(CrystallizerAct::Recipe(None))
        );
        assert!(parse("recipe", p("mode", "/obj/item/gun")).is_err());
        assert!(parse("gas_input", p("gas_input", "nan")).is_err());
    }
}
