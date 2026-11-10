//! The admin fishing calculator (server), after tg `/datum/fishing_calculator`
//! (`code/modules/fishing/admin.dm`): for a fish source, a rod and a bait, every outcome of the
//! source's table with its modified weight, difficulty and limited count.
//!
//! `ss13_admin` opens it from the `R_DEBUG` verb ([`open`]); the window sits on a host off the
//! map, one per opening, gone with its window. Every act checks `R_DEBUG` again
//! ([`crate::votes::VoteRights`]`::debug`) and the window closes without it (tg
//! `ADMIN_STATE(R_DEBUG)`).
// shortcut: the setup is chosen with acts kept on the server (tgui keeps it in `useState` and
// sends it with `recalc`); no hook or line choice (the port's rods carry their own); the bait is
// any food type path.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::DmFood;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::fishing::{DUD, DmFish, DmFishingRod, FISH_SOURCES, ROD_ROOT, fish_source};
use ss13_shared::chat::Span;
use ss13_shared::ui::UiStatus;
use ss13_tgui::server::{
    AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};

use super::server::{BaitInfo, FishCounts, Reward, Row, difficulty, modified_table};
use super::{
    CALC_RECALC, CALC_SET_BAIT, CALC_SET_ROD, CALC_SET_SPOT, CALC_VALUE, CalculatorData,
    CalculatorRow, CalculatorStatic, FishingCalculatorUi,
};
use crate::votes::VoteRights;

/// `/datum/fishing_challenge::difficulty` (`FISHING_DEFAULT_DIFFICULTY`), added to the table's.
const CHALLENGE_DIFFICULTY: f32 = 15.0;
/// The tgui default bait.
const DEFAULT_BAIT: &str = "/obj/item/food/bait/worm";
const MAX_PATH_LEN: usize = 200;
/// How long a host waits for its window to open.
const OPEN_GRACE_SECS: f64 = 5.0;

/// A calculator's host: the static lists and the setup.
#[derive(Component, Clone, Debug, Default)]
pub struct FishingCalculator {
    pub lists: CalculatorStatic,
    pub data: CalculatorData,
    /// When it was opened: a host whose window has not opened yet is kept for a while.
    pub opened_at: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CalcAct {
    Spot(String),
    Rod(String),
    Bait(String),
    Recalc,
}

pub(super) fn plugin(app: &mut App) {
    app.register_ui::<FishingCalculatorUi>().add_systems(
        Update,
        (
            close_without_rights.in_set(UiSystems::StatusHook),
            calculator_acts.in_set(UiSystems::Handle),
            drop_closed_hosts.after(UiSystems::Send),
        ),
    );
}

fn has_debug(world: &World, client: Entity) -> bool {
    world.get::<VoteRights>(client).is_some_and(|r| r.debug)
}

/// The admin verb: a new calculator for `client`, whose body is `mob`.
pub fn open(world: &mut World, client: Entity, mob: Entity) {
    if !has_debug(world, client) {
        return;
    }
    let mut rod_types: Vec<String> = world
        .get_resource::<ss13_proto::Prototypes>()
        .map(|p| {
            p.ids()
                .filter(|id| is_under(id, ROD_ROOT))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    rod_types.sort_unstable();
    let spot_types: Vec<String> = FISH_SOURCES.iter().map(|s| s.path.to_owned()).collect();
    let data = CalculatorData {
        spot: spot_types.first().cloned().unwrap_or_default(),
        rod: rod_types.first().cloned().unwrap_or_default(),
        bait: DEFAULT_BAIT.to_owned(),
        info: Vec::new(),
    };
    let opened_at = crate::tools::now(world);
    let host = world
        .spawn((
            Name::new("fishing calculator"),
            AlwaysState,
            FishingCalculator {
                lists: CalculatorStatic {
                    rod_types,
                    spot_types,
                },
                data,
                opened_at,
            },
        ))
        .id();
    world.write_message(OpenUi::new::<FishingCalculatorUi>(
        client,
        mob,
        host,
        "Fishing Calculator",
    ));
}

impl ServerUi for FishingCalculatorUi {
    type Source = &'static FishingCalculator;
    type UserParam = ();
    type Act = CalcAct;

    fn static_data(calc: &FishingCalculator) -> CalculatorStatic {
        calc.lists.clone()
    }

    fn data(calc: &FishingCalculator, _: Entity, _: &()) -> CalculatorData {
        calc.data.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<CalcAct, UiReject> {
        let value = || {
            params
                .text(CALC_VALUE)
                .map(|v| v.chars().take(MAX_PATH_LEN).collect::<String>())
        };
        match action {
            CALC_SET_SPOT => Ok(CalcAct::Spot(value()?)),
            CALC_SET_ROD => Ok(CalcAct::Rod(value()?)),
            CALC_SET_BAIT => Ok(CalcAct::Bait(value()?)),
            CALC_RECALC => Ok(CalcAct::Recalc),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// `ui_state`: `ADMIN_STATE(R_DEBUG)`.
fn close_without_rights(
    mut windows: Query<&mut UiWindow>,
    rights: Query<&VoteRights>,
    hosts: Query<(), With<FishingCalculator>>,
) {
    for mut window in &mut windows {
        if hosts.contains(window.source) && !rights.get(window.client).is_ok_and(|r| r.debug) {
            window.cap_status(UiStatus::Close);
        }
    }
}

/// `ui_close`: `qdel(src)`.
fn drop_closed_hosts(
    hosts: Query<(Entity, &FishingCalculator)>,
    windows: Query<&UiWindow>,
    time: Res<Time>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for (host, calc) in &hosts {
        if now > calc.opened_at + OPEN_GRACE_SECS && !windows.iter().any(|w| w.source == host) {
            commands.entity(host).despawn();
        }
    }
}

fn calculator_acts(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<FishingCalculatorUi>>>,
) {
    let pending: Vec<(Entity, Entity, CalcAct)> = acts
        .read(world.resource::<Messages<UiActed<FishingCalculatorUi>>>())
        .map(|a| (a.client, a.source, a.act.clone()))
        .collect();
    for (client, host, act) in pending {
        if !has_debug(world, client) {
            continue;
        }
        let Some(calc) = world.get::<FishingCalculator>(host).cloned() else {
            continue;
        };
        let mut data = calc.data;
        match act {
            CalcAct::Spot(spot) if calc.lists.spot_types.contains(&spot) => data.spot = spot,
            CalcAct::Rod(rod) if calc.lists.rod_types.contains(&rod) => data.rod = rod,
            CalcAct::Bait(bait) => data.bait = bait,
            CalcAct::Recalc => match calculate(world, &data) {
                Ok(info) => data.info = info,
                Err(text) => crate::tools::tell(world, client, Span::Warning, text),
            },
            _ => continue,
        }
        if let Some(mut c) = world.get_mut::<FishingCalculator>(host) {
            c.data = data;
        }
    }
}

/// `recalc`: the source's table with the rod and bait, every outcome (0-weight ones too).
fn calculate(world: &World, setup: &CalculatorData) -> Result<Vec<CalculatorRow>, &'static str> {
    let protos = world.get_resource::<ss13_proto::Prototypes>();
    let rod = protos
        .and_then(|p| p.get(&setup.rod))
        .and_then(|p| p.get::<DmFishingRod>())
        .cloned()
        .ok_or("A fishing rod is needed in order to fish.")?;
    if !rod.has_hook {
        return Err("A fishing hook is needed in order to fish.");
    }
    let source = fish_source(&setup.spot).ok_or("No such fish source.")?;
    let bait = (!setup.bait.is_empty())
        .then(|| protos.and_then(|p| p.get(&setup.bait)))
        .flatten()
        .map(|proto| bait_of(&setup.bait, proto.get::<DmFood>()));
    let counts = world.resource::<FishCounts>();
    let traits_of = |path: &str| {
        protos
            .and_then(|p| p.get(path))
            .and_then(|p| p.get::<DmFish>())
            .cloned()
    };
    let rows: Vec<Row> = source
        .table
        .iter()
        .map(|&(path, weight)| Row {
            reward: if path == DUD {
                Reward::Dud
            } else {
                Reward::Path(path)
            },
            weight: f64::from(weight),
            traits: traits_of(path).map(|f| f.fish_traits),
        })
        .collect();
    let modified = modified_table(rows.clone(), bait.as_ref());
    Ok(rows
        .iter()
        .map(|row| {
            let path = match row.reward {
                Reward::Path(p) => p,
                _ => DUD,
            };
            let weight = modified
                .iter()
                .find(|m| m.reward == row.reward)
                .map_or(0.0, |m| m.weight);
            let fish = traits_of(path);
            let traits = fish
                .as_ref()
                .map(|f| f.fish_traits.clone())
                .unwrap_or_default();
            // `calculate_difficulty` has no skill term.
            let d = difficulty(
                source,
                &rod,
                fish.as_ref().map(|f| (f, traits.as_slice())),
                0.0,
            ) + CHALLENGE_DIFFICULTY;
            CalculatorRow {
                result: path.to_owned(),
                weight,
                difficulty: d,
                count: counts
                    .left(source, path)
                    .map_or_else(|| "Infinite".to_owned(), |n| n.to_string()),
            }
        })
        .collect())
}

/// The bait facts of a food type (`super::server::bait_info` for a prototype).
fn bait_of(path: &str, food: Option<&DmFood>) -> BaitInfo {
    super::server::bait_info_of(path, food.map_or(0, |f| f.foodtypes))
}
