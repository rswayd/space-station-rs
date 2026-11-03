//! Cutting a stationary tank open and building one from its frame (`components/tank.dm`,
//! `/obj/structure/tank_frame`).
//!
//! - Right-click welder on a tank: 4 s (8 s, with a warning, over 2 atm above the tile); the
//!   group's shared gas goes onto the tile and a plated, unsecured frame is left
//!   (`welder_act_secondary`, `on_deconstruction(disassembled)`).
//! - Frame: a wrench (0.5 s) fastens or loosens it; a right-click screwdriver (1 s) takes a bare
//!   frame apart into 4 plasteel; 12 sheets (3 s) plate it; a right-click crowbar (2 s) pries
//!   the plating back off; a lit welder (2 s) seals a fastened, plated frame into an empty tank
//!   that joins the tanks next to it.
// shortcut: plating takes iron only and builds the iron tank every mapped one is (materials and
// their strength, color and name prefix are not ported, docs/parity/engineering/
// stacks-materials.md); the unsafe cut warns but does not throw the user (mobs are not thrown,
// see `crate::throw`); a new tank leaves its neighbours' ports facing it open (tg closes only
// the merge origin's).

use bevy::prelude::*;
use ss13_atmos::Atmos;
use ss13_atmos::gas::ONE_ATMOSPHERE;
use ss13_atmos::pipenet::{CARDINALS, Pipenet};
use ss13_chat::Span;
use ss13_mapload::{DmAtmosDevice, DmObj, IconIndex};
use ss13_shared::protocol::ClickModifiers;
use ss13_shared::{Appearance, Dir, TilePos};

use super::AtmosDevice;
use super::tank::{StationaryTank, TANK, initial_air, tank_nodes};
use crate::obj_damage::{Anchored, is_under};
use crate::tools::{Actor, ToolKind, tell, tool_of};

pub use ss13_mapload::atmos_machine::TANK_FRAME;
/// `TANK_PLATING_SHEETS`.
pub const PLATING_SHEETS: u32 = 12;
const FRAME_SHEETS: u32 = 4;
const IRON: &str = "/obj/item/stack/sheet/iron";
const PLASTEEL: &str = "/obj/item/stack/sheet/plasteel";
const SHEET: &str = "/obj/item/stack/sheet";
const MISC_DMI: &str = "icons/obj/pipes_n_cables/stationary_canisters_misc.dmi";

/// `construction_state`: `TANK_FRAME` (false) or `TANK_PLATING_UNSECURED` (true).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TankFrame {
    pub plated: bool,
}

/// The timed steps ([`super::windows::AtmosJob::Frame`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameJob {
    /// `welder_act_secondary` of a tank; `unsafe` when it was pressurized.
    Cut {
        unsafe_cut: bool,
    },
    Unfasten {
        anchored: bool,
    },
    Apart,
    Plate,
    Pry,
    Seal,
}

fn anchored(world: &World, e: Entity) -> bool {
    world
        .get::<Anchored>(e)
        .map(|a| a.0)
        .or_else(|| world.get::<DmObj>(e).map(|o| o.anchored))
        .unwrap_or(false)
}

fn set_plated(world: &mut World, frame: Entity, plated: bool) {
    if let Some(mut f) = world.get_mut::<TankFrame>(frame) {
        f.plated = plated;
    }
    let state = if plated { "plated_frame" } else { "frame" };
    let id = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(MISC_DMI, state));
    if let Some(id) = id
        && let Some(mut a) = world.get_mut::<Appearance>(frame)
    {
        a.icon_state = id;
    }
}

/// A new frame on `pos` (unanchored, as `new(location)` leaves it).
pub fn spawn_frame(world: &mut World, pos: TilePos, plated: bool) -> Option<Entity> {
    let frame = crate::construction::spawn_obj(world, TANK_FRAME, pos)?;
    world
        .entity_mut(frame)
        .insert((TankFrame::default(), Anchored(false)));
    set_plated(world, frame, plated);
    Some(frame)
}

/// The tank's shared air pressure minus its tile's (`internal_pressure`).
fn internal_pressure(world: &World, tank: Entity) -> f64 {
    let inside = world
        .get::<StationaryTank>(tank)
        .and_then(|t| world.resource::<Pipenet>().tank_group(t.group))
        .map_or(0.0, |g| g.air.pressure());
    let outside = world
        .get::<TilePos>(tank)
        .and_then(|&p| world.resource::<Atmos>().pressure(p))
        .unwrap_or(0.0);
    inside - outside
}

fn start(world: &mut World, actor: Actor, target: Entity, delay_ds: f32, job: FrameJob) -> bool {
    crate::tools::use_tool(
        world,
        actor,
        target,
        delay_ds,
        0,
        crate::construction::Job::Atmos(super::windows::AtmosJob::Frame(job)),
    )
}

/// A tool or sheets on a tank or a frame. True when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    let tool = tool_of(world, held).map(|t| t.0);
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    let right = modifiers.right;
    if world.get::<StationaryTank>(target).is_some() {
        if !(right && tool == Some(ToolKind::Welder)) {
            return false;
        }
        tell(
            world,
            client,
            Span::Notice,
            "Вы начинаете вскрывать газовый бак...",
        );
        let unsafe_cut = internal_pressure(world, target) > 2.0 * ONE_ATMOSPHERE;
        let delay = if unsafe_cut {
            tell(
                world,
                client,
                Span::Warning,
                "Похоже, бак под давлением. Вы уверены, что это хорошая идея?",
            );
            80.0
        } else {
            40.0
        };
        start(world, actor, target, delay, FrameJob::Cut { unsafe_cut });
        return true;
    }
    let Some(&frame) = world.get::<TankFrame>(target) else {
        return false;
    };
    match (tool, right) {
        // `default_unfasten_wrench(user, tool, time = 0.5 SECONDS)`.
        (Some(ToolKind::Wrench), false) => {
            let was = anchored(world, target);
            if !was && crate::construction::blocked_for(world, target) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вам не удалось прикрутить раму бака.",
                );
                return true;
            }
            let text = if was {
                "Вы начинаете откреплять раму бака..."
            } else {
                "Вы начинаете прикреплять раму бака..."
            };
            tell(world, client, Span::Notice, text);
            start(
                world,
                actor,
                target,
                5.0,
                FrameJob::Unfasten { anchored: was },
            );
            true
        }
        (Some(ToolKind::Screwdriver), true) if !frame.plated => {
            tell(
                world,
                client,
                Span::Notice,
                "Вы начинаете разбирать раму бака.",
            );
            start(world, actor, target, 10.0, FrameJob::Apart);
            true
        }
        (Some(ToolKind::Crowbar), true) if frame.plated => {
            tell(
                world,
                client,
                Span::Notice,
                "Вы начинаете отдирать внешнюю обшивку...",
            );
            start(world, actor, target, 20.0, FrameJob::Pry);
            true
        }
        (Some(ToolKind::Welder), false) if frame.plated => {
            if !anchored(world, target) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Сначала <b>прикрутите</b> раму к полу гаечным ключом.",
                );
                return true;
            }
            if !crate::tools::tool_usable(world, held, 0, crate::tools::HIGH_TEMPERATURE_REQUIRED) {
                return true;
            }
            tell(
                world,
                client,
                Span::Notice,
                "Вы начинаете заваривать внешнюю обшивку...",
            );
            start(world, actor, target, 20.0, FrameJob::Seal);
            true
        }
        // `item_interaction` with a stack on a bare frame -> `add_plating`.
        (None, false) if !frame.plated && world.get::<crate::tools::Stack>(held).is_some() => {
            let path = crate::construction::path_of(world, held).unwrap_or_default();
            if !is_under(&path, SHEET) {
                tell(world, client, Span::Warning, "неподходящий материал!");
            } else if !is_under(&path, IRON) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Из этого материала бак пока не собрать: годится только железо.",
                );
            } else {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы начинаете обшивать раму листами...",
                );
                start(world, actor, target, 30.0, FrameJob::Plate);
            }
            true
        }
        _ => false,
    }
}

/// `add_plating`'s hint at how much more of the stack is needed.
pub fn amount_more(amount: u32) -> &'static str {
    match 100 * amount / PLATING_SHEETS {
        0 => "хоть сколько-нибудь",
        1..=25 => "намного больше",
        26..=50 => "примерно вчетверо больше",
        51..=75 => "примерно вдвое больше",
        76..=100 => "ещё чуть-чуть",
        _ => "неизвестно сколько ещё",
    }
}

/// The end of a [`FrameJob`].
pub fn complete(world: &mut World, action: &crate::tools::DoAfter, job: FrameJob) {
    let (client, target) = (action.client, action.target);
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    match job {
        FrameJob::Cut { unsafe_cut } => {
            if world.get::<StationaryTank>(target).is_none() {
                return;
            }
            if unsafe_cut {
                tell(world, client, Span::Danger, "Давление отбрасывает вас!");
            }
            crate::obj_damage::deconstruct(world, target, true);
            tell(
                world,
                client,
                Span::Notice,
                "Вы вскрываете запаянный газовый бак, обнажая его нутро.",
            );
        }
        FrameJob::Unfasten { anchored: was } => {
            if world.get::<TankFrame>(target).is_none() || anchored(world, target) != was {
                return;
            }
            if !was && crate::construction::blocked_for(world, target) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вам не удалось прикрутить раму бака.",
                );
                return;
            }
            crate::obj_damage::set_anchored(world, target, !was);
            let text = if was {
                "Вы откручиваете раму бака."
            } else {
                "Вы прикручиваете раму бака."
            };
            tell(world, client, Span::Notice, text);
        }
        FrameJob::Apart => {
            if world.get::<TankFrame>(target).is_none_or(|f| f.plated) {
                return;
            }
            world.despawn(target);
            crate::construction::tile_changed(world, pos);
            crate::construction::spawn_stack(world, PLASTEEL, pos, FRAME_SHEETS);
            tell(world, client, Span::Notice, "Рама бака разобрана.");
        }
        FrameJob::Plate => {
            if world.get::<TankFrame>(target).is_none_or(|f| f.plated) {
                return;
            }
            let Some(item) = action.item else {
                return;
            };
            if !crate::tools::use_stack(world, item, PLATING_SHEETS) {
                let have = crate::tools::stack_amount(world, item);
                let text = format!(
                    "Листов не хватает на всю обшивку. Нужно {}.",
                    amount_more(have)
                );
                tell(world, client, Span::Notice, &text);
                return;
            }
            set_plated(world, target, true);
            tell(
                world,
                client,
                Span::Notice,
                "Вы закрепляете листы на раме бака.",
            );
        }
        FrameJob::Pry => {
            if !world.get::<TankFrame>(target).is_some_and(|f| f.plated) {
                return;
            }
            set_plated(world, target, false);
            crate::construction::spawn_stack(world, IRON, pos, PLATING_SHEETS);
            tell(world, client, Span::Notice, "Вы снимаете внешнюю обшивку.");
        }
        FrameJob::Seal => {
            if !world.get::<TankFrame>(target).is_some_and(|f| f.plated) || !anchored(world, target)
            {
                return;
            }
            world.despawn(target);
            if build_tank(world, pos).is_some() {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Бак запаян и готов принимать газ.",
                );
            }
        }
    }
}

/// The new tank of a sealed frame: no open ports, empty, merged with the tanks next to it.
pub fn build_tank(world: &mut World, pos: TilePos) -> Option<Entity> {
    let tank = crate::construction::spawn_obj(world, TANK, pos)?;
    let dm = world.get::<DmAtmosDevice>(tank).cloned()?;
    let (mut spec, dir) = super::device_spec(tank, pos, &dm, 2)?;
    // `on_construction`: not mapped, so no port is opened.
    spec.init_dirs = 0;
    spec.node_dirs = tank_nodes(0);
    let layer = spec.layer;
    let neighbours: Vec<u32> = {
        let index = world.resource::<ss13_world::SpatialIndex>();
        CARDINALS
            .into_iter()
            .filter_map(|d| {
                let (dx, dy) = Dir::from_bits(d)?.offset();
                let p = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
                index
                    .on_tile(p)
                    .iter()
                    .find_map(|&e| world.get::<StationaryTank>(e).map(|t| t.group))
            })
            .collect()
    };
    let group = {
        let mut net = world.resource_mut::<Pipenet>();
        let id = net.add(spec);
        let group = net.join_tank_groups(id, initial_air(&dm), &neighbours);
        (id, group)
    };
    let (id, group) = group;
    let mut q = world.query::<&mut StationaryTank>();
    for mut t in q.iter_mut(world) {
        if neighbours.contains(&t.group) {
            t.group = group;
        }
    }
    world.entity_mut(tank).insert((
        AtmosDevice { id, layer, dir },
        StationaryTank {
            group,
            max_pressure: dm.max_pressure,
        },
    ));
    Some(tank)
}

/// `tank_frame/examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(frame) = world.get::<TankFrame>(target) else {
        return;
    };
    let text = if anchored(world, target) {
        "Рама <b>прикручена</b> к полу."
    } else {
        "Рама ещё не <b>прикручена</b> к полу."
    };
    lines.push((Span::Notice, text.to_owned()));
    let text = if frame.plated {
        "Обшивка плотно прилегает, снять её можно <b>ломом</b>, но её ещё нужно заварить \
         <b>сваркой</b>."
    } else {
        "Рама <b>свинчена</b>, осталось обшить её <b>металлическими листами</b>."
    };
    lines.push((Span::Notice, text.to_owned()));
}
