//! Reinforced walls and windows (server): tg `/turf/closed/wall/r_wall/try_decon`
//! (`code/game/turfs/closed/wall/reinf_walls.dm`) and the `RWINDOW_*` steps of
//! `/obj/structure/window/reinforced` (`code/game/objects/structures/window.dm`).
//!
//! - A reinforced wall comes apart in seven reversible steps (`d_state`, [`RWallState`]):
//!   wirecutters (instant), screwdriver 4 s, welder 6 s, crowbar 10 s, wrench 4 s, welder 10 s,
//!   crowbar 10 s -> `dismantle_wall` (its plasteel sheet and reinforced girder). Every welder
//!   step needs a lit welder of 1500 K with 2 fuel at its start (the last weld back: 0 fuel).
//!   A wall out of `INTACT` stops smoothing and shows `r_wall-<d_state>` of the fork's
//!   `reinforced_states.dmi`.
//! - A reinforced window is taken out of its frame with right clicks: welder 15 s heats the
//!   security screws (they cool back after 30 s), screwdriver 5 s, crowbar 4 s, wirecutters 2 s,
//!   wrench 4 s -> out of the frame and unanchored; from there the plain window steps apply
//!   (screwdriver unanchors/anchors, wrench takes it into its glass), and a crowbar on an
//!   anchored one levers it back to secure (10 s).

use bevy::prelude::*;
use ss13_mapload::smooth::DmSmooth;
use ss13_mapload::{DmObj, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};

use crate::construction::{Job, path_of};
use crate::obj_damage::{
    INDESTRUCTIBLE, ObjState, RWINDOW_SECURE, WINDOW, WINDOW_OUT_OF_FRAME, is_under,
};
use crate::tools::{Actor, HIGH_TEMPERATURE_REQUIRED, ToolKind, tell, tool_usable, use_tool};

pub const R_WALL: &str = "/turf/closed/wall/r_wall";
/// BandaStation's sprites of the deconstruction steps (`update_icon_state`).
const DECON_DMI: &str = "modular_bandastation/aesthetics/walls/icons/reinforced_states.dmi";
/// `base_decon_state`.
const DECON_STATE: &str = "r_wall";

/// `d_state` values.
pub const INTACT: u8 = 0;
pub const SUPPORT_LINES: u8 = 1;
pub const COVER: u8 = 2;
pub const CUT_COVER: u8 = 3;
pub const ANCHOR_BOLTS: u8 = 4;
pub const SUPPORT_RODS: u8 = 5;
pub const SHEATH: u8 = 6;
/// Not a tg state: the last crowbar step's target, the wall comes down.
const DISMANTLED: u8 = u8::MAX;

/// `RWINDOW_*` window states below `RWINDOW_SECURE`.
pub const RWINDOW_FRAME_BOLTED: i32 = 3;
pub const RWINDOW_BARS_CUT: i32 = 4;
pub const RWINDOW_POPPED: i32 = 5;
pub const RWINDOW_BOLTS_OUT: i32 = 6;
pub const RWINDOW_BOLTS_HEATED: i32 = 7;
/// `addtimer(cool_bolts, 30 SECONDS)`.
const BOLTS_COOL_SECS: f64 = 30.0;

/// A reinforced wall's `d_state` out of `INTACT`, with the smoothing flags it had (restored
/// when it is intact again). Absent: intact.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RWallState {
    pub d_state: u8,
    smooth_flags: u8,
}

/// Heated security screws of a reinforced window cool back at `at` (tg `cool_bolts`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct BoltsCooling {
    pub at: f64,
}

/// A timed reinforced wall or window step: the state it started from and the one it leads to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReinforcedJob {
    Wall { from: u8, to: u8 },
    Window { from: i32, to: i32 },
}

pub fn d_state(world: &World, wall: Entity) -> u8 {
    world.get::<RWallState>(wall).map_or(INTACT, |s| s.d_state)
}

/// tg `deconstruction_hints` of a reinforced wall, `examine` of a reinforced window.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(path) = path_of(world, target) else {
        return;
    };
    let hint = if is_under(&path, R_WALL) {
        Some(wall_hint(d_state(world, target)))
    } else if is_under(&path, WINDOW)
        && world
            .get::<DmObj>(target)
            .is_some_and(|o| o.reinf && o.resistance_flags & INDESTRUCTIBLE == 0)
    {
        window_hint(world.get::<ObjState>(target).map_or(0, |s| s.0))
    } else {
        None
    };
    if let Some(hint) = hint {
        lines.push((Span::Notice, hint.to_owned()));
    }
}

fn window_hint(state: i32) -> Option<&'static str> {
    Some(match state {
        RWINDOW_SECURE => {
            "Окно закреплено односторонними винтами — чтобы их выкрутить, их нужно нагреть."
        }
        RWINDOW_BOLTS_HEATED => "Винты раскалены добела, теперь их, вероятно, можно выкрутить.",
        RWINDOW_BOLTS_OUT => "Винты выкручены, открылась небольшая щель, куда можно вставить лом.",
        RWINDOW_POPPED => {
            "Основная панель окна выскочила из рамы, открыв прутья, которые можно перекусить."
        }
        RWINDOW_BARS_CUT => "Основную панель можно отодвинуть, открыв болты, держащие раму.",
        _ => return None,
    })
}

fn wall_hint(d_state: u8) -> &'static str {
    match d_state {
        SUPPORT_LINES => {
            "Внешняя решётка разрезана, но поддержки всё ещё надёжно привинчены к внешней обшивке."
        }
        COVER => "Поддержки откручены. Внешняя обшивка накрепко приварена к балке.",
        CUT_COVER => "Внешняя обшивка срезана и едва держится на паре заклёпок.",
        ANCHOR_BOLTS => {
            "Внешняя обшивка отодрана. Болты, крепящие опорные стержни, плотно затянуты."
        }
        SUPPORT_RODS => {
            "Болты, крепящие опорные стержни, ослаблены, но они всё ещё приварены к балке."
        }
        SHEATH => {
            "Опорные стержни перерезаны, и внутренний слой обшивки вот-вот отвалится от балки."
        }
        _ => "Внешняя решётка выглядит нетронутой.",
    }
}

/// A tool on a reinforced wall (`try_decon`). True when it was a tool (the click is used up
/// even when no step matched or the welder could not start).
pub fn wall_tool(world: &mut World, actor: Actor, wall: Entity) -> bool {
    let Some(item) = actor.item else {
        return false;
    };
    let Some((kind, _)) = crate::tools::tool_of(world, item) else {
        return false;
    };
    let client = actor.client;
    let state = d_state(world, wall);
    // (delay ds, fuel needed at the start for a welder, next state, start text).
    let step: Option<(f32, u32, u8, &'static str)> = match (state, kind) {
        (INTACT, ToolKind::Wirecutters) => {
            set_wall_state(world, wall, SUPPORT_LINES);
            tell(
                world,
                client,
                Span::Notice,
                "Вы перекусываете внешнюю решётку.",
            );
            return true;
        }
        (SUPPORT_LINES, ToolKind::Wirecutters) => {
            set_wall_state(world, wall, INTACT);
            tell(world, client, Span::Notice, "Вы чините внешнюю решётку.");
            return true;
        }
        (SUPPORT_LINES, ToolKind::Screwdriver) => {
            Some((40.0, 0, COVER, "Вы начинаете откручивать поддержки..."))
        }
        (COVER, ToolKind::Welder) => {
            Some((60.0, 2, CUT_COVER, "Вы начинаете резать внешнюю обшивку..."))
        }
        (COVER, ToolKind::Screwdriver) => Some((
            40.0,
            0,
            SUPPORT_LINES,
            "Вы начинаете закреплять поддержки...",
        )),
        (CUT_COVER, ToolKind::Crowbar) => Some((
            100.0,
            0,
            ANCHOR_BOLTS,
            "Вы с трудом отдираете лист внешней обшивки...",
        )),
        (CUT_COVER, ToolKind::Welder) => Some((
            60.0,
            2,
            COVER,
            "Вы начинаете приваривать лист обшивки обратно к каркасу...",
        )),
        (ANCHOR_BOLTS, ToolKind::Wrench) => Some((
            40.0,
            0,
            SUPPORT_RODS,
            "Вы начинаете ослаблять болты, фиксирующие опорные стержни на каркасе...",
        )),
        (ANCHOR_BOLTS, ToolKind::Crowbar) => Some((
            20.0,
            0,
            CUT_COVER,
            "Вы поддеваете лист обшивки, вправляя его на место...",
        )),
        (SUPPORT_RODS, ToolKind::Welder) => Some((
            100.0,
            2,
            SHEATH,
            "Вы начинаете разрезать опорные стержни...",
        )),
        (SUPPORT_RODS, ToolKind::Wrench) => Some((
            40.0,
            0,
            ANCHOR_BOLTS,
            "Вы начинаете затягивать болты, удерживающие опорные стержни...",
        )),
        (SHEATH, ToolKind::Crowbar) => Some((
            100.0,
            0,
            DISMANTLED,
            "Вы просовываете монтировку в щель с краю внутренней обшивки...",
        )),
        (SHEATH, ToolKind::Welder) => Some((
            100.0,
            0,
            SUPPORT_RODS,
            "Вы начинаете сваривать опорные стержни друг с другом...",
        )),
        _ => None,
    };
    let Some((delay, fuel, to, text)) = step else {
        return true;
    };
    // `tool_start_check(amount, heat_required = HIGH_TEMPERATURE_REQUIRED)`; `use_tool` itself
    // burns no extra fuel.
    if kind == ToolKind::Welder && !tool_usable(world, item, fuel, HIGH_TEMPERATURE_REQUIRED) {
        let text = crate::tools::unusable_message(world, item, fuel);
        tell(world, client, Span::Warning, text);
        return true;
    }
    let job = Job::Reinforced(ReinforcedJob::Wall { from: state, to });
    if use_tool(world, actor, wall, delay, 0, job) {
        tell(world, client, Span::Notice, text);
    }
    true
}

/// `d_state = to; update_appearance()`: out of `INTACT` the wall stops smoothing and shows its
/// step's sprite; back to `INTACT` it smooths again.
pub fn set_wall_state(world: &mut World, wall: Entity, to: u8) {
    let Some(&pos) = world.get::<TilePos>(wall) else {
        return;
    };
    let previous = world.get::<RWallState>(wall).copied();
    if to == INTACT {
        let Some(previous) = previous else {
            return;
        };
        world.entity_mut(wall).remove::<RWallState>();
        if let Some(mut smooth) = world.get_mut::<DmSmooth>(wall) {
            smooth.flags = previous.smooth_flags;
        }
        crate::construction::tile_changed(world, pos);
        return;
    }
    let smooth_flags = previous.map_or_else(
        || world.get::<DmSmooth>(wall).map_or(0, |s| s.flags),
        |p| p.smooth_flags,
    );
    world.entity_mut(wall).insert(RWallState {
        d_state: to,
        smooth_flags,
    });
    if let Some(mut smooth) = world.get_mut::<DmSmooth>(wall)
        && smooth.flags != 0
    {
        smooth.flags = 0;
    }
    let id = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(DECON_DMI, &format!("{DECON_STATE}-{to}")));
    if let (Some(id), Some(mut appearance)) = (id, world.get_mut::<Appearance>(wall))
        && appearance.icon_state != id
    {
        appearance.icon_state = id;
    }
}

/// A right click with a tool on a reinforced window (its `*_act_secondary` steps, else
/// `item_interaction_secondary`'s hint). True when handled.
pub fn window_secondary(world: &mut World, actor: Actor, kind: ToolKind, window: Entity) -> bool {
    let Some(obj) = world.get::<DmObj>(window).cloned() else {
        return false;
    };
    if !obj.reinf || !is_under(&obj.path, WINDOW) {
        return false;
    }
    let client = actor.client;
    if obj.resistance_flags & INDESTRUCTIBLE != 0 {
        tell(world, client, Span::Warning, "Слишком прочное!");
        return true;
    }
    let state = world.get::<ObjState>(window).map_or(0, |s| s.0);
    // (delay ds, next state, start text).
    let step = match (state, kind) {
        (RWINDOW_SECURE, ToolKind::Welder) => {
            let item = actor.item.expect("a tool act has an item");
            if !tool_usable(world, item, 0, HIGH_TEMPERATURE_REQUIRED) {
                let text = crate::tools::unusable_message(world, item, 0);
                tell(world, client, Span::Warning, text);
                return true;
            }
            Some((
                150.0,
                RWINDOW_BOLTS_HEATED,
                "Вы начинаете нагревать защитные винты окна...",
            ))
        }
        (RWINDOW_BOLTS_HEATED, ToolKind::Screwdriver) => Some((
            50.0,
            RWINDOW_BOLTS_OUT,
            "Вы с силой вкручиваетесь в раскалённые винты, и они начинают поддаваться...",
        )),
        (RWINDOW_BOLTS_OUT, ToolKind::Crowbar) => Some((
            40.0,
            RWINDOW_POPPED,
            "Вы вставляете инструмент в щель рамы и начинаете поддевать...",
        )),
        (RWINDOW_POPPED, ToolKind::Wirecutters) => Some((
            20.0,
            RWINDOW_BARS_CUT,
            "Вы начинаете перекусывать открывшиеся прутья...",
        )),
        (RWINDOW_BARS_CUT, ToolKind::Wrench) => Some((
            40.0,
            WINDOW_OUT_OF_FRAME,
            "Вы начинаете откручивать болты рамы...",
        )),
        _ => None,
    };
    let Some((delay, to, text)) = step else {
        let hint = match state {
            RWINDOW_SECURE => Some("Сначала нужно нагреть защитные винты!"),
            RWINDOW_BOLTS_HEATED => Some("Сначала нужно выкрутить защитные винты!"),
            RWINDOW_BOLTS_OUT => Some("Сначала нужно поддеть щель!"),
            RWINDOW_POPPED => Some("Сначала нужно перекусить прутья!"),
            RWINDOW_BARS_CUT => Some("Сначала нужно открутить болты!"),
            _ => None,
        };
        if let Some(hint) = hint {
            tell(world, client, Span::Warning, hint);
        }
        return true;
    };
    let job = Job::Reinforced(ReinforcedJob::Window { from: state, to });
    if use_tool(world, actor, window, delay, 0, job) {
        tell(world, client, Span::Notice, text);
    }
    true
}

/// `/obj/structure/window/reinforced/crowbar_act`: an anchored window out of its frame is
/// levered back to secure (10 s). False when it does not apply (the crowbar hits it).
pub fn window_crowbar(world: &mut World, actor: Actor, window: Entity) -> bool {
    let anchored = world
        .get::<crate::obj_damage::Anchored>(window)
        .is_some_and(|a| a.0);
    let state = world.get::<ObjState>(window).map_or(0, |s| s.0);
    if !anchored || state != WINDOW_OUT_OF_FRAME {
        return false;
    }
    let job = Job::Reinforced(ReinforcedJob::Window {
        from: WINDOW_OUT_OF_FRAME,
        to: RWINDOW_SECURE,
    });
    if use_tool(world, actor, window, 100.0, 0, job) {
        tell(
            world,
            actor.client,
            Span::Notice,
            "Вы начинаете вдвигать окно обратно в раму...",
        );
    }
    true
}

/// The step's effect if its starting state still holds (tg `extra_checks`; tg's window steps
/// have none, so a second do_after started meanwhile would repeat the step there).
pub fn complete(world: &mut World, client: Entity, target: Entity, job: ReinforcedJob) {
    let path = path_of(world, target).unwrap_or_default();
    match job {
        ReinforcedJob::Wall { from, to } => {
            if !is_under(&path, R_WALL) || d_state(world, target) != from {
                return;
            }
            if to == DISMANTLED {
                let Some(&pos) = world.get::<TilePos>(target) else {
                    return;
                };
                tell(
                    world,
                    client,
                    Span::Notice,
                    "Вы отдираете внутреннюю обшивку.",
                );
                crate::construction::dismantle_wall(world, target, pos);
                return;
            }
            set_wall_state(world, target, to);
            let text = match (from, to) {
                (SUPPORT_LINES, COVER) => "Вы откручиваете болтики, крепящие поддержки.",
                (COVER, SUPPORT_LINES) => "Поддержки закреплены.",
                (COVER, CUT_COVER) => {
                    "Вы сильно нажимаете на лист обшивки, отцепляя его от крепления."
                }
                (CUT_COVER, ANCHOR_BOLTS) => "Лист обшивки с треском отходит от каркаса.",
                (CUT_COVER, COVER) => "Лист обшивки накрепко приварен.",
                (ANCHOR_BOLTS, SUPPORT_RODS) => "Вы откручиваете болты опорных стержней.",
                (ANCHOR_BOLTS, CUT_COVER) => "Лист обшивки встаёт в свой паз.",
                (SUPPORT_RODS, SHEATH) => "Опорные стержни разрезаны.",
                (SUPPORT_RODS, ANCHOR_BOLTS) => "Болты опорных стержней подтянуты.",
                _ => "Опорные стержни зафиксированы.",
            };
            tell(world, client, Span::Notice, text);
        }
        ReinforcedJob::Window { from, to } => {
            let reinforced = world.get::<DmObj>(target).is_some_and(|o| o.reinf);
            let state = world.get::<ObjState>(target).map_or(0, |s| s.0);
            if !is_under(&path, WINDOW) || !reinforced || state != from {
                return;
            }
            let anchored = world
                .get::<crate::obj_damage::Anchored>(target)
                .is_some_and(|a| a.0);
            if from == WINDOW_OUT_OF_FRAME && !anchored {
                return;
            }
            world.entity_mut(target).insert(ObjState(to));
            let text = match to {
                RWINDOW_BOLTS_HEATED => {
                    let at = world.resource::<Time>().elapsed_secs_f64() + BOLTS_COOL_SECS;
                    world.entity_mut(target).insert(BoltsCooling { at });
                    "Защитные винты раскалились добела, теперь их можно выкрутить."
                }
                RWINDOW_BOLTS_OUT => "Винты выходят, и вокруг края стекла появляется щель.",
                RWINDOW_POPPED => {
                    "Панель выскакивает из рамы, открывая тонкие металлические прутья, которые можно перекусить."
                }
                RWINDOW_BARS_CUT => "Панель отходит в сторону, открывая болты рамы.",
                WINDOW_OUT_OF_FRAME => {
                    crate::obj_damage::set_anchored(world, target, false);
                    "Вы откручиваете болты рамы, и окно выходит из неё."
                }
                _ => "Вы вдвигаете окно обратно в раму.",
            };
            tell(world, client, Span::Notice, text);
        }
    }
}

/// tg `cool_bolts`: heated screws still not out go back to secure.
pub(crate) fn cool_bolts(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &BoltsCooling)>();
    let due: Vec<Entity> = query
        .iter(world)
        .filter(|(_, c)| now >= c.at)
        .map(|(e, _)| e)
        .collect();
    for window in due {
        world.entity_mut(window).remove::<BoltsCooling>();
        let heated = world
            .get::<ObjState>(window)
            .is_some_and(|s| s.0 == RWINDOW_BOLTS_HEATED);
        let Some(&pos) = world.get::<TilePos>(window) else {
            continue;
        };
        if !heated {
            continue;
        }
        world.entity_mut(window).insert(ObjState(RWINDOW_SECURE));
        for client in crate::construction::clients_near(world, pos, 7, &[]) {
            tell(
                world,
                client,
                Span::Notice,
                "Болты на окне, похоже, остыли...",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use ss13_mapload::DmTurfBuild;
    use ss13_net::chat::server::SendChat;
    use ss13_world::WorldPlugin;

    use super::*;
    use crate::obj_damage::Anchored;
    use crate::tools::DoAfter;

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, WorldPlugin))
            .add_message::<SendChat>();
        app.world_mut()
            .resource_mut::<ss13_world::SpatialIndex>()
            .create_level(1, 5, 5)
            .unwrap();
        app
    }

    fn run(world: &mut World, target: Entity, job: ReinforcedJob) {
        let mob = world.spawn(TilePos::new(1, 1, 1)).id();
        let action = DoAfter {
            client: mob,
            target,
            target_pos: TilePos::new(2, 2, 1),
            user_pos: TilePos::new(1, 1, 1),
            item: None,
            amount: 0,
            ends_at: 0.0,
            job: Job::Reinforced(job),
        };
        crate::construction::complete(world, mob, &action);
    }

    /// Each step of `try_decon` needs the state it started from; leaving `INTACT` turns the
    /// wall's smoothing off and coming back restores it.
    #[test]
    fn wall_steps_need_their_state_and_restore_smoothing() {
        let mut app = app();
        let world = app.world_mut();
        let wall = world
            .spawn((
                DmTurfBuild {
                    path: R_WALL.into(),
                    ..default()
                },
                DmSmooth {
                    flags: 1,
                    ..default()
                },
                TilePos::new(2, 2, 1),
            ))
            .id();
        set_wall_state(world, wall, SUPPORT_LINES);
        assert_eq!(d_state(world, wall), SUPPORT_LINES);
        assert_eq!(world.get::<DmSmooth>(wall).unwrap().flags, 0);
        // A screwdriver started on COVER does nothing on SUPPORT_LINES.
        run(
            world,
            wall,
            ReinforcedJob::Wall {
                from: COVER,
                to: SUPPORT_LINES,
            },
        );
        assert_eq!(d_state(world, wall), SUPPORT_LINES);
        run(
            world,
            wall,
            ReinforcedJob::Wall {
                from: SUPPORT_LINES,
                to: COVER,
            },
        );
        assert_eq!(d_state(world, wall), COVER);
        run(
            world,
            wall,
            ReinforcedJob::Wall {
                from: COVER,
                to: SUPPORT_LINES,
            },
        );
        set_wall_state(world, wall, INTACT);
        assert!(world.get::<RWallState>(wall).is_none());
        assert_eq!(world.get::<DmSmooth>(wall).unwrap().flags, 1);
    }

    /// The right-click chain of a reinforced window ends out of its frame and unanchored; a
    /// heated window cools back to secure.
    #[test]
    fn window_chain_and_cooling() {
        let mut app = app();
        let world = app.world_mut();
        let window = world
            .spawn((
                DmObj {
                    path: "/obj/structure/window/reinforced".into(),
                    reinf: true,
                    ..default()
                },
                ObjState(RWINDOW_SECURE),
                Anchored(true),
                TilePos::new(2, 2, 1),
            ))
            .id();
        let state = |world: &World| world.get::<ObjState>(window).unwrap().0;
        run(
            world,
            window,
            ReinforcedJob::Window {
                from: RWINDOW_SECURE,
                to: RWINDOW_BOLTS_HEATED,
            },
        );
        assert_eq!(state(world), RWINDOW_BOLTS_HEATED);
        assert!(world.get::<BoltsCooling>(window).is_some());
        world.entity_mut(window).insert(BoltsCooling { at: 0.0 });
        cool_bolts(world);
        assert_eq!(state(world), RWINDOW_SECURE, "the screws cooled");
        for (from, to) in [
            (RWINDOW_SECURE, RWINDOW_BOLTS_HEATED),
            (RWINDOW_BOLTS_HEATED, RWINDOW_BOLTS_OUT),
            (RWINDOW_BOLTS_OUT, RWINDOW_POPPED),
            (RWINDOW_POPPED, RWINDOW_BARS_CUT),
        ] {
            run(world, window, ReinforcedJob::Window { from, to });
            assert_eq!(state(world), to);
        }
        // Stale: started from another state.
        run(
            world,
            window,
            ReinforcedJob::Window {
                from: RWINDOW_POPPED,
                to: RWINDOW_BARS_CUT,
            },
        );
        assert_eq!(state(world), RWINDOW_BARS_CUT);
        run(
            world,
            window,
            ReinforcedJob::Window {
                from: RWINDOW_BARS_CUT,
                to: WINDOW_OUT_OF_FRAME,
            },
        );
        assert_eq!(state(world), WINDOW_OUT_OF_FRAME);
        assert!(!world.get::<Anchored>(window).unwrap().0);
        // Levering back needs it anchored.
        let back = ReinforcedJob::Window {
            from: WINDOW_OUT_OF_FRAME,
            to: RWINDOW_SECURE,
        };
        run(world, window, back);
        assert_eq!(state(world), WINDOW_OUT_OF_FRAME);
        crate::obj_damage::set_anchored(world, window, true);
        run(world, window, back);
        assert_eq!(state(world), RWINDOW_SECURE);
    }
}
