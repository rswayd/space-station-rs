//! The AI's foam dispenser (server), after tg `/obj/machinery/ai_slipper`
//! (`code/game/machinery/ai_slipper.dm`).
//!
//! - Mapped dispensers get their `uses` (20, map edits from `DmAiSlipper`).
//! - `interact`: an AI or cyborg (`attack_ai`, [`super::interact`]) or anyone with AI upload
//!   access (an empty-handed click) floods foam around it (`fluid_spread/foam`, range 4), one
//!   use, then 10 s of cooldown; unpowered it does nothing (`can_interact` of a machine).
//! - Its sprite is `ai-slipper1` while it can be used, `ai-slipper0` otherwise; examine shows the
//!   uses left.
// shortcut: no armor or breaking: the dispenser takes the generic machine damage only.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::silicon::AI_SLIPPER_ROOT;
use ss13_mapload::{DmAiSlipper, DmName, DmObj};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::effects::foam::{FoamKind, FoamOptions, do_foam};
use crate::power::Unpowered;
use crate::tools::tell;

/// `uses` default.
const DEFAULT_USES: u32 = 20;
/// `cooldown_time`.
pub const COOLDOWN_SECS: f64 = 10.0;
/// `new /datum/effect_system/fluid_spread/foam(loc, 4)`.
const FOAM_RANGE: u32 = 4;
/// `req_access = list(ACCESS_AI_UPLOAD)`.
const REQ_ACCESS: &str = "ai_upload";

/// A foam dispenser's state.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct AiSlipper {
    pub uses: u32,
    /// `foam_cooldown`: usable again at this time.
    pub ready_at: f64,
    /// Whether `ai-slipper1` is shown.
    shown_ready: Option<bool>,
}

impl AiSlipper {
    pub fn new(uses: u32) -> Self {
        Self {
            uses,
            ready_at: 0.0,
            shown_ready: None,
        }
    }
}

pub struct AiSlipperPlugin;

impl Plugin for AiSlipperPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::EmptyHand, 3463, |w, c| {
            interact(w, c.client, c.mob, c.target)
        })
        .add_systems(ss13_world::MapInitLate, init_slippers)
        .add_systems(
            Update,
            slipper_looks.run_if(any_with_component::<AiSlipper>),
        );
    }
}

fn init_slippers(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &DmObj, Option<&DmAiSlipper>), (
        Without<AiSlipper>,
        With<ss13_world::Mapped>,
    )>();
    let found: Vec<(Entity, u32)> = q
        .iter(world)
        .filter(|(_, o, _)| is_under(&o.path, AI_SLIPPER_ROOT))
        .map(|(e, _, d)| {
            let uses = d.map_or(DEFAULT_USES, |d| d.uses.max(0.0) as u32);
            (e, uses)
        })
        .collect();
    for (e, uses) in found {
        world.entity_mut(e).insert(AiSlipper::new(uses));
    }
}

fn name_of(world: &World, e: Entity) -> String {
    world.get::<DmName>(e).map_or_else(
        || "foam dispenser".to_owned(),
        |n| crate::vending::plain_name(&n.name).to_owned(),
    )
}

/// tg `allowed(user)`: silicons, or an ID with AI upload access.
fn allowed(world: &World, user: Entity) -> bool {
    super::interact::has_silicon_access(world, user)
        || crate::inventory::check_access(
            &[REQ_ACCESS.to_owned()],
            &[],
            &crate::bots::user_access(world, user),
        )
}

/// tg `/obj/machinery/ai_slipper/interact`. True when `target` is a dispenser.
pub fn interact(world: &mut World, client: Entity, user: Entity, target: Entity) -> bool {
    let Some(slipper) = world.get::<AiSlipper>(target).copied() else {
        return false;
    };
    // `can_interact`: a machine without power ignores the user.
    if world.get::<Unpowered>(target).is_some() {
        return true;
    }
    let name = name_of(world, target);
    if !allowed(world, user) {
        tell(world, client, Span::Danger, "Access denied.");
        return true;
    }
    if slipper.uses == 0 {
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} is out of foam and cannot be activated!"),
        );
        return true;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < slipper.ready_at {
        let left = crate::jobs::display_time_text((slipper.ready_at - now).ceil());
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} cannot be activated for {left}!"),
        );
        return true;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return true;
    };
    do_foam(
        world,
        FOAM_RANGE,
        pos,
        None,
        None,
        FoamKind::Plain,
        FoamOptions::default(),
    );
    let uses = slipper.uses - 1;
    if let Some(mut s) = world.get_mut::<AiSlipper>(target) {
        s.uses = uses;
        s.ready_at = now + COOLDOWN_SECS;
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("You activate {name}. It now has {uses} uses of foam remaining."),
    );
    true
}

/// tg `examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    if let Some(s) = world.get::<AiSlipper>(target) {
        lines.push((
            Span::Notice,
            format!("It has {} uses of foam remaining.", s.uses),
        ));
    }
}

/// tg `update_icon_state`: `ai-slipper1` when powered, cooled down and not empty. Runs over the
/// few dispensers of the map; the sprite changes only when the state flips.
fn slipper_looks(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &AiSlipper, Has<Unpowered>)>();
    let flips: Vec<(Entity, bool)> = q
        .iter(world)
        .filter_map(|(e, s, unpowered)| {
            let ready = !unpowered && now >= s.ready_at && s.uses > 0;
            (s.shown_ready != Some(ready)).then_some((e, ready))
        })
        .collect();
    for (e, ready) in flips {
        if let Some(mut s) = world.get_mut::<AiSlipper>(e) {
            s.shown_ready = Some(ready);
        }
        let state = if ready { "ai-slipper1" } else { "ai-slipper0" };
        crate::obj_damage::set_icon_state(world, e, state);
    }
}
