//! The atmos pipe dispenser: tg `/obj/machinery/pipedispenser` (`game/machinery/pipe/
//! pipe_dispenser.dm`) with `GLOB.atmos_pipe_recipes` and its `PipeDispenser` window.
//!
//! - The window picks a paint color, a piping layer and the directions a smart pipe may connect
//!   in (never fewer than two); a recipe button drops its fitting (facing north, in that color,
//!   on that layer, with those directions) or a meter item on the dispenser's tile, at most one
//!   a second. It works without power (`INTERACT_MACHINE_OFFLINE`).
//! - A fitting or a meter item used on it goes back in (is deleted); a wrench (4 s) unfastens or
//!   fastens it (`default_unfasten_wrench`).
// shortcut: the disposal and transit tube dispensers are not set up (their pipes and tubes are
// not ported: docs/parity/engineering/recycling.md); no fingerprints.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_chat::Span;
use ss13_mapload::DmObj;
use ss13_mapload::atmos_machine::{ATMOS_PIPE_RECIPES, PIPE_DISPENSER};
use ss13_shared::TilePos;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::pipe_dispenser::{
    InitDirections, PipeCategory, PipeDispenserData, PipeDispenserStatic, PipeDispenserUi,
    PipeRecipeEntry,
};
use ss13_tgui::server::{OpenUi, ServerUi, UiActed, UiParams, UiReject};

use super::fitting::{FittingKind, MeterFitting, PipeFitting};
use super::thermo::PIPE_PAINT_COLORS;
use crate::obj_damage::is_under;
use crate::tools::{Actor, ToolKind, tell};

/// `ALL_CARDINALS`.
const ALL_CARDINALS: u8 = 15;
/// `wait = world.time + 1 SECONDS`.
const DISPENSE_COOLDOWN: f64 = 1.0;
/// `default_unfasten_wrench(user, tool, time = 4 SECONDS)`.
const UNFASTEN_DS: f32 = 40.0;
const PIPING_LAYER_DEFAULT: u8 = 3;

/// `/obj/machinery/pipedispenser` (atmos).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct PipeDispenser {
    pub layer: u8,
    /// Index into [`PIPE_PAINT_COLORS`] (`paint_color`, `"green"` at first).
    pub color: usize,
    /// `p_init_dir`.
    pub init_dir: u8,
    /// `wait`: `Time::elapsed_secs_f64` from which it dispenses again.
    pub ready_at: f64,
}

impl Default for PipeDispenser {
    fn default() -> Self {
        Self {
            layer: PIPING_LAYER_DEFAULT,
            color: PIPE_PAINT_COLORS
                .iter()
                .position(|(n, _)| *n == "green")
                .unwrap_or(0),
            init_dir: ALL_CARDINALS,
            ready_at: 0.0,
        }
    }
}

/// Atmos dispensers get their state as they appear (map load or a later spawn); the disposal
/// and transit tube ones are other machines.
pub fn init_dispensers(mut commands: Commands, added: Query<(Entity, &DmObj), Added<DmObj>>) {
    for (e, obj) in &added {
        if is_atmos_dispenser(&obj.path) {
            commands.entity(e).insert(PipeDispenser::default());
        }
    }
}

pub struct PipeDispenserWindow;

impl UiInterface for PipeDispenserWindow {
    const NAME: &'static str = PipeDispenserUi::NAME;
    type Data = PipeDispenserData;
    type StaticData = PipeDispenserStatic;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DispenserAct {
    Color(usize),
    /// Category index, recipe index (both 0-based), the fitting's dir.
    Dispense(usize, usize, u8),
    Layer(u8),
    /// One direction bit to toggle.
    InitDir(u8),
    InitReset,
}

fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

/// `ui_data`'s categories: every recipe offered facing north (bent ones are disposals only).
pub fn categories() -> Vec<PipeCategory> {
    ATMOS_PIPE_RECIPES
        .iter()
        .map(|(name, recipes)| PipeCategory {
            cat_name: (*name).to_owned(),
            recipes: recipes
                .iter()
                .enumerate()
                .map(|(i, r)| PipeRecipeEntry {
                    pipe_name: r.name.to_owned(),
                    pipe_index: i as u32 + 1,
                    all_layers: r.all_layers,
                    dir: 1,
                })
                .collect(),
        })
        .collect()
}

/// `params["dir_flag"]` (`text2dir`).
fn dir_flag(text: &str) -> Option<u8> {
    match text {
        "north" => Some(1),
        "south" => Some(2),
        "east" => Some(4),
        "west" => Some(8),
        _ => None,
    }
}

fn bad(what: &str) -> UiReject {
    UiReject::BadParams(what.to_owned())
}

impl ServerUi for PipeDispenserWindow {
    type Source = &'static PipeDispenser;
    type UserParam = ();
    type Act = DispenserAct;

    fn static_data(_: &PipeDispenser) -> PipeDispenserStatic {
        PipeDispenserStatic {
            paint_colors: PIPE_PAINT_COLORS
                .iter()
                .map(|(n, rgb)| ((*n).to_owned(), hex(*rgb)))
                .collect(),
            categories: categories(),
        }
    }

    fn data(d: &PipeDispenser, _: Entity, _: &SystemParamItem<'_, '_, ()>) -> PipeDispenserData {
        PipeDispenserData {
            piping_layer: d.layer,
            selected_color: PIPE_PAINT_COLORS
                .get(d.color)
                .map_or("", |(n, _)| n)
                .to_owned(),
            init_directions: InitDirections::from_bits(d.init_dir),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<DispenserAct, UiReject> {
        match action {
            "color" => {
                let name = params
                    .get("paint_color")
                    .ok_or_else(|| bad("paint_color"))?;
                PIPE_PAINT_COLORS
                    .iter()
                    .position(|(n, _)| *n == name)
                    .map(DispenserAct::Color)
                    .ok_or_else(|| bad("paint_color"))
            }
            "pipe_type" => {
                let category = params.get("category").ok_or_else(|| bad("category"))?;
                let c = ATMOS_PIPE_RECIPES
                    .iter()
                    .position(|(n, _)| *n == category)
                    .ok_or_else(|| bad("category"))?;
                // "No spawning arbitrary paths": an index into that category only.
                let index = params.number("pipe_type")?;
                let len = ATMOS_PIPE_RECIPES[c].1.len();
                if index.fract() != 0.0 || index < 1.0 || index > len as f64 {
                    return Err(bad("pipe_type"));
                }
                let dir = params.number("pipe_dir")?;
                let dir = [1, 2, 4, 8, 5, 6, 9, 10]
                    .into_iter()
                    .find(|&d| f64::from(d) == dir)
                    .ok_or_else(|| bad("pipe_dir"))?;
                Ok(DispenserAct::Dispense(c, index as usize - 1, dir))
            }
            "piping_layer" => {
                let layer = params.number("piping_layer")?;
                (1..=5)
                    .find(|&l| f64::from(l) == layer)
                    .map(DispenserAct::Layer)
                    .ok_or_else(|| bad("piping_layer"))
            }
            "init_dir_setting" => params
                .get("dir_flag")
                .and_then(dir_flag)
                .map(DispenserAct::InitDir)
                .ok_or_else(|| bad("dir_flag")),
            "init_reset" => Ok(DispenserAct::InitReset),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

pub fn dispenser_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<PipeDispenserWindow>>>,
) {
    let pending: Vec<(Entity, Entity, DispenserAct)> = acts
        .read(world.resource::<Messages<UiActed<PipeDispenserWindow>>>())
        .map(|a| (a.client, a.source, a.act.clone()))
        .collect();
    for (client, source, act) in pending {
        apply(world, client, source, act);
    }
}

fn apply(world: &mut World, client: Entity, source: Entity, act: DispenserAct) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(mut d) = world.get_mut::<PipeDispenser>(source) else {
        return;
    };
    match act {
        DispenserAct::Color(i) => d.color = i,
        DispenserAct::Layer(l) => d.layer = l,
        DispenserAct::InitReset => d.init_dir = ALL_CARDINALS,
        DispenserAct::InitDir(bit) => {
            let target = d.init_dir ^ bit;
            // `ISNOTSTUB`: a smart pipe needs two directions at least.
            if target & target.wrapping_sub(1) != 0 {
                d.init_dir = target;
            } else {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Экран раздатчика труб мигает предупреждением: нельзя настроить трубу \
                     на соединение только в одном направлении.",
                );
            }
        }
        DispenserAct::Dispense(c, i, dir) => {
            if now < d.ready_at {
                return;
            }
            d.ready_at = now + DISPENSE_COOLDOWN;
            let d = d.clone();
            let Some(&pos) = world.get::<TilePos>(source) else {
                return;
            };
            dispense(world, pos, &d, c, i, dir);
        }
    }
}

/// The recipe's fitting (or meter item) on `pos`.
pub fn dispense(
    world: &mut World,
    pos: TilePos,
    d: &PipeDispenser,
    category: usize,
    index: usize,
    dir: u8,
) -> Option<Entity> {
    let recipe = ATMOS_PIPE_RECIPES.get(category)?.1.get(index)?;
    let Some(path) = recipe.path else {
        let item = crate::construction::spawn_obj(world, super::fitting::PIPE_METER_ITEM, pos)?;
        world.entity_mut(item).insert(MeterFitting {
            layer: PIPING_LAYER_DEFAULT,
        });
        return Some(item);
    };
    let kind = world
        .get_resource::<ss13_proto::Prototypes>()
        .and_then(|p| p.get(path))
        .and_then(|p| p.get::<ss13_mapload::DmAtmosDevice>())
        .map(|dm| FittingKind::of(&dm.construction_type))?;
    super::fitting::spawn_fitting(
        world,
        pos,
        PipeFitting {
            pipe_type: path.to_owned(),
            kind,
            dir,
            init_dir: d.init_dir,
            layer: d.layer,
            color: PIPE_PAINT_COLORS.get(d.color).map_or([0xEE; 3], |c| c.1),
            flipped: false,
        },
    )
}

/// A click with an empty hand: the window.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<PipeDispenser>(target).is_none() {
        return false;
    }
    let title = world
        .get::<ss13_mapload::DmName>(target)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<PipeDispenserWindow>(
        client, mob, target, title,
    ));
    true
}

/// `item_interaction` (fittings back in) and `wrench_act`. True when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    actor: Actor,
    held: Entity,
    target: Entity,
    tool: Option<ToolKind>,
) -> bool {
    if world.get::<PipeDispenser>(target).is_none() {
        return false;
    }
    if world.get::<PipeFitting>(held).is_some() || world.get::<MeterFitting>(held).is_some() {
        let name = world
            .get::<ss13_mapload::DmName>(held)
            .map(|n| crate::vending::plain_name(&n.name).to_owned())
            .unwrap_or_default();
        world.despawn(held);
        tell(
            world,
            actor.client,
            Span::Notice,
            format!("Вы кладёте {name} обратно в раздатчик труб."),
        );
        return true;
    }
    if tool != Some(ToolKind::Wrench) {
        return false;
    }
    let anchored = world
        .get::<crate::obj_damage::Anchored>(target)
        .is_some_and(|a| a.0);
    if !anchored && crate::construction::blocked_for(world, target) {
        tell(
            world,
            actor.client,
            Span::Notice,
            "Вам не удалось прикрутить раздатчик труб.",
        );
        return true;
    }
    if crate::tools::use_tool(
        world,
        actor,
        target,
        UNFASTEN_DS,
        0,
        crate::construction::Job::Atmos(super::windows::AtmosJob::Unfasten { anchored }),
    ) {
        let text = if anchored {
            "Вы начинаете откреплять раздатчик труб..."
        } else {
            "Вы начинаете прикреплять раздатчик труб..."
        };
        tell(world, actor.client, Span::Notice, text);
    }
    true
}

/// Whether `path` is the atmos dispenser (not its disposal or transit tube subtypes).
pub fn is_atmos_dispenser(path: &str) -> bool {
    is_under(path, PIPE_DISPENSER) && !is_under(path, "/obj/machinery/pipedispenser/disposal")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ss13_tgui::server::UiParams;

    fn act(action: &str, params: &[(&str, &str)]) -> Result<DispenserAct, UiReject> {
        let owned: Vec<(String, String)> = params
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        PipeDispenserWindow::parse_act(action, &UiParams::new(&owned))
    }

    #[test]
    fn recipes_follow_tg_and_acts_stay_inside_them() {
        let cats = categories();
        let names: Vec<&str> = cats.iter().map(|c| c.cat_name.as_str()).collect();
        assert_eq!(names, ["Pipes", "Binary", "Devices", "Heat Exchange"]);
        assert_eq!(cats[2].recipes[7].pipe_name, "Meter");
        assert_eq!(
            act(
                "pipe_type",
                &[
                    ("category", "Devices"),
                    ("pipe_type", "8"),
                    ("pipe_dir", "1")
                ]
            ),
            Ok(DispenserAct::Dispense(2, 7, 1))
        );
        for bad in [
            [
                ("category", "Devices"),
                ("pipe_type", "9"),
                ("pipe_dir", "1"),
            ],
            [
                ("category", "Devices"),
                ("pipe_type", "0"),
                ("pipe_dir", "1"),
            ],
            [
                ("category", "Disposals"),
                ("pipe_type", "1"),
                ("pipe_dir", "1"),
            ],
            [("category", "Pipes"), ("pipe_type", "1"), ("pipe_dir", "3")],
        ] {
            assert!(act("pipe_type", &bad).is_err(), "{bad:?}");
        }
        assert_eq!(
            act("piping_layer", &[("piping_layer", "5")]),
            Ok(DispenserAct::Layer(5))
        );
        assert!(act("piping_layer", &[("piping_layer", "6")]).is_err());
        assert!(act("color", &[("paint_color", "chartreuse")]).is_err());
        assert_eq!(
            act("init_dir_setting", &[("dir_flag", "west")]),
            Ok(DispenserAct::InitDir(8))
        );
    }
}
