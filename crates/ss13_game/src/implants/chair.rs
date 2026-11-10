//! The implant chairs (server), after tg `/obj/machinery/implantchair` and its genetic purifier
//! and neural imprinter subtypes (`implantchair.dm`), with the `ImplantChair` window
//! (`ss13_tgui::interfaces::implant_chair`). The door and the occupant are
//! `crate::medical::pods`'.
//!
//! - It starts open. The window's "door" shuts in the living mob on its tile (or opens it); a
//!   living mob dragged onto an open chair is shut in. The occupant cannot use the window
//!   (`notcontained_state`).
//! - "Implant" works on a living occupant while the chair is ready and has implants left: the
//!   mindshield implanter implants a mindshield, the genetic purifier removes a human's
//!   mutations and makes them human, the neural imprinter brainwashes a player that is not
//!   mindshielded with "Obey the law. Praise Nanotrasen.". A success uses an implant, starts the
//!   injection cooldown (60 s, none, 300 s) and the replenishing of one implant every
//!   `replenish_cooldown` (600 s, 30 s; never for the imprinter) up to five; a failure buzzes.
//! - A step from inside tells the door won't budge (every 5 s) and starts a one minute breakout
//!   that opens it if the occupant is still inside and awake.
// shortcut: no custom imprinter objective (`custom`, admin var), no `purrbation_remove` (cat
// ears and tails are not a human's here), no admin logs.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_shared::chat::Span;
use ss13_shared::ui::UiStatus;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::implant_chair::{
    ImplantChairData, ImplantChairOccupant, ImplantChairUi,
};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiKind, UiParams, UiReject, UiSystems, UiWindow,
};

use super::{Implant, MINDSHIELD, implant, next_id};
use crate::closets::InCloset;
use crate::health::{Health, Stat};
use crate::medical::pods::{Pod, close_pod, occupant, open_pod};
use crate::obj_damage::is_under;
use crate::tools::{now, tell_mob};

pub const CHAIR_ROOT: &str = "/obj/machinery/implantchair";
const GENEPURGE: &str = "/obj/machinery/implantchair/genepurge";
const BRAINWASH: &str = "/obj/machinery/implantchair/brainwash";
const BUZZ: &str = "sound/machines/buzz/buzz-sigh.ogg";
/// `breakout_time` (600 deciseconds).
const BREAKOUT_SECS: f64 = 60.0;
/// `relaymove`'s `message_cooldown`.
const MESSAGE_COOLDOWN_SECS: f64 = 5.0;
/// The imprinter's `objective`.
const OBJECTIVE: &str = "Соблюдайте закон. Восхваляйте Nanotrasen.";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChairKind {
    #[default]
    Mindshield,
    /// `genepurge`.
    Genepurge,
    /// `brainwash`.
    Brainwash,
}

/// `/obj/machinery/implantchair` vars, the pending timers, and the window's data last read.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ImplantChair {
    pub kind: ChairKind,
    pub ready: bool,
    pub replenishing: bool,
    pub ready_implants: u32,
    pub max_implants: u32,
    /// `injection_cooldown`, `replenish_cooldown`: seconds.
    pub injection_cooldown: f64,
    pub replenish_cooldown: f64,
    pub auto_replenish: bool,
    /// `set_ready` and `replenish` timers, game seconds.
    pub ready_at: Option<f64>,
    pub replenish_at: Option<f64>,
    /// `relaymove`'s `message_cooldown`.
    pub message_at: f64,
    /// The breakout under way: who and when it ends.
    pub breakout: Option<(Entity, f64)>,
    view: ImplantChairData,
}

impl ImplantChair {
    pub fn of_path(path: &str) -> Self {
        let (kind, injection_cooldown, replenish_cooldown, auto_replenish) =
            if is_under(path, GENEPURGE) {
                (ChairKind::Genepurge, 0.0, 30.0, true)
            } else if is_under(path, BRAINWASH) {
                (ChairKind::Brainwash, 300.0, 600.0, false)
            } else {
                (ChairKind::Mindshield, 60.0, 600.0, true)
            };
        Self {
            kind,
            ready: true,
            replenishing: false,
            ready_implants: 5,
            max_implants: 5,
            injection_cooldown,
            replenish_cooldown,
            auto_replenish,
            ready_at: None,
            replenish_at: None,
            message_at: 0.0,
            breakout: None,
            view: ImplantChairData::default(),
        }
    }

    /// `special_name` when `special`.
    fn special_name(&self) -> Option<&'static str> {
        match self.kind {
            ChairKind::Mindshield => None,
            ChairKind::Genepurge => Some("Purge genome"),
            ChairKind::Brainwash => Some("Imprint"),
        }
    }
}

pub(super) fn build(app: &mut App) {
    app.register_ui::<ChairWindow>().add_systems(
        Update,
        (
            cap_windows.in_set(UiSystems::StatusHook),
            acts.in_set(UiSystems::Handle),
            refresh_views
                .after(UiSystems::Handle)
                .before(UiSystems::Send),
            timers.run_if(any_with_component::<ImplantChair>),
        ),
    );
}

/// `update_icon_state` (`implantchair`, `_open`, `_occupied`) and the `ready` overlay.
pub fn refresh_look(world: &mut World, e: Entity) {
    let Some(ready) = world.get::<ImplantChair>(e).map(|c| c.ready) else {
        return;
    };
    let open = world.get::<Pod>(e).is_some_and(|p| p.open);
    let occupied = occupant(world, e).is_some();
    let mut state = "implantchair".to_owned();
    if open {
        state.push_str("_open");
    }
    if occupied {
        state.push_str("_occupied");
    }
    crate::obj_damage::set_icon_state(world, e, &state);
    // The chair has no other overlays: the `ready` light is its only one.
    let overlays: Vec<ss13_shared::Overlay> = ready
        .then(|| {
            world
                .get_resource::<ss13_mapload::IconIndex>()
                .and_then(|i| i.get("icons/obj/machines/implant_chair.dmi", "ready"))
        })
        .flatten()
        .map(ss13_shared::Overlay::new)
        .into_iter()
        .collect();
    if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(e)
        && look.overlays != overlays
    {
        look.overlays = overlays;
    }
}

/// `interact` -> `ui_interact`.
pub fn interact(world: &mut World, client: Entity, mob: Entity, e: Entity) {
    refresh_view(world, e);
    let title = crate::examine::name_of(world, e);
    world.write_message(OpenUi::new::<ChairWindow>(client, mob, e, title));
}

/// `close_machine(target)` while open.
pub fn close(world: &mut World, e: Entity, target: Option<Entity>) {
    if world.get::<Pod>(e).is_none_or(|p| !p.open) {
        return;
    }
    close_pod(world, e, target);
    refresh_look(world, e);
    refresh_view(world, e);
}

/// `open_machine`.
pub fn open(world: &mut World, e: Entity) {
    if let Some(mut c) = world.get_mut::<ImplantChair>(e) {
        c.breakout = None;
    }
    open_pod(world, e);
    refresh_look(world, e);
    refresh_view(world, e);
}

/// `mouse_drop_receive`: a living mob dragged onto it is shut in.
pub fn mouse_drop_receive(world: &mut World, e: Entity, dragged: Entity) {
    if world.get::<Health>(dragged).is_some() {
        close(world, e, Some(dragged));
    }
}

/// `relaymove` and `container_resist_act`: "the door won't budge", and a breakout starts.
pub fn relaymove(world: &mut World, mob: Entity, e: Entity) {
    let t = now(world);
    let Some(chair) = world.get::<ImplantChair>(e).cloned() else {
        return;
    };
    let name = crate::examine::name_of(world, e);
    if chair.message_at <= t {
        if let Some(mut c) = world.get_mut::<ImplantChair>(e) {
            c.message_at = t + MESSAGE_COOLDOWN_SECS;
        }
        tell_mob(
            world,
            mob,
            Span::Warning,
            format!("Дверь {name} не поддаётся!"),
        );
    }
    if chair.breakout.is_some() {
        return;
    }
    if let Some(mut c) = world.get_mut::<ImplantChair>(e) {
        c.breakout = Some((mob, t + BREAKOUT_SECS));
    }
    let who = crate::examine::name_of(world, mob);
    crate::medical::visible(
        world,
        mob,
        &format!(
            "Вы упираетесь в спинку {name} и начинаете выдавливать дверь... (это займёт около \
             минуты.)"
        ),
        &format!("{who} пинает дверь {name}!"),
        Span::Notice,
    );
}

/// `implant(occupant, user)`: one use when the action works, then the cooldowns.
pub fn implant_occupant(world: &mut World, e: Entity) {
    let Some(chair) = world.get::<ImplantChair>(e).cloned() else {
        return;
    };
    let Some(mob) = occupant(world, e).filter(|&m| world.get::<Health>(m).is_some()) else {
        return;
    };
    if chair.ready_implants == 0 || !chair.ready {
        return;
    }
    if implant_action(world, e, chair.kind, mob) {
        let t = now(world);
        if let Some(mut c) = world.get_mut::<ImplantChair>(e) {
            c.ready_implants -= 1;
            if !c.replenishing && c.auto_replenish {
                c.replenishing = true;
                c.replenish_at = Some(t + c.replenish_cooldown);
            }
            if c.injection_cooldown > 0.0 {
                c.ready = false;
                c.ready_at = Some(t + c.injection_cooldown);
            }
        }
    } else if let Some(pos) = crate::explosions::turf_of(world, e) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, BUZZ, 25.0, true));
    }
    refresh_look(world, e);
    refresh_view(world, e);
}

/// `implant_action` of the kind.
fn implant_action(world: &mut World, e: Entity, kind: ChairKind, mob: Entity) -> bool {
    let done = match kind {
        ChairKind::Mindshield => {
            let id = next_id(world);
            Implant::new(MINDSHIELD, id).is_some_and(|imp| implant(world, mob, None, imp).is_ok())
        }
        // `genepurge/implant_action`: humans only.
        ChairKind::Genepurge => {
            if world.get::<crate::human::HumanLook>(mob).is_none() {
                return false;
            }
            crate::genetics::remove_all_mutations(world, mob);
            crate::species::set_species(world, mob, crate::species::SpeciesId::Human);
            return true;
        }
        // `brainwash/implant_action`: a mind that is not `TRAIT_UNCONVERTABLE`.
        ChairKind::Brainwash => {
            if world.get::<crate::server::PlayerMob>(mob).is_none()
                || crate::antag::revolution::server::mindshielded(world, mob)
            {
                return false;
            }
            crate::antag::brainwashed::brainwash(world, mob, vec![OBJECTIVE.to_owned()]);
            return true;
        }
    };
    if done {
        let (who, what) = (
            crate::examine::name_of(world, mob),
            crate::examine::name_of(world, e),
        );
        let text = format!("{what} имплантирует {who}.");
        crate::medical::visible(world, e, &text, &text, Span::Warning);
    }
    done
}

/// `set_ready`, `replenish` and the breakouts, revalidated when due.
// Cost: per tick one pass over the implant chairs (a few per map), a field read each.
fn timers(world: &mut World) {
    let t = now(world);
    let chairs: Vec<(Entity, ImplantChair)> = world
        .query::<(Entity, &ImplantChair)>()
        .iter(world)
        .filter(|(_, c)| c.ready_at.is_some() || c.replenish_at.is_some() || c.breakout.is_some())
        .map(|(e, c)| (e, c.clone()))
        .collect();
    for (e, chair) in chairs {
        let mut changed = false;
        if chair.ready_at.is_some_and(|at| t >= at) {
            if let Some(mut c) = world.get_mut::<ImplantChair>(e) {
                c.ready = true;
                c.ready_at = None;
            }
            changed = true;
        }
        if chair.replenish_at.is_some_and(|at| t >= at) {
            if let Some(mut c) = world.get_mut::<ImplantChair>(e) {
                if c.ready_implants < c.max_implants {
                    c.ready_implants += 1;
                }
                if c.ready_implants < c.max_implants {
                    c.replenish_at = Some(t + c.replenish_cooldown);
                } else {
                    c.replenishing = false;
                    c.replenish_at = None;
                }
            }
            changed = true;
        }
        if let Some((mob, ends)) = chair.breakout
            && t >= ends
        {
            if let Some(mut c) = world.get_mut::<ImplantChair>(e) {
                c.breakout = None;
            }
            let inside = world.get::<InCloset>(mob).is_some_and(|c| c.closet == e);
            let awake = world.get::<Stat>(mob) == Some(&Stat::Conscious)
                && !crate::status_effects::incapacitation(world, mob).knocked_out;
            let open_now = world.get::<Pod>(e).is_some_and(|p| p.open);
            if inside && awake && !open_now {
                let (who, what) = (
                    crate::examine::name_of(world, mob),
                    crate::examine::name_of(world, e),
                );
                crate::medical::visible(
                    world,
                    mob,
                    &format!("Вы выбираетесь из {what}!"),
                    &format!("{who} выбирается из {what}!"),
                    Span::Warning,
                );
                open(world, e);
            }
        }
        if changed {
            refresh_look(world, e);
            refresh_view(world, e);
        }
    }
}

/// `ui_data`.
fn compute_view(world: &mut World, e: Entity) -> ImplantChairData {
    let Some(chair) = world.get::<ImplantChair>(e).cloned() else {
        return ImplantChairData::default();
    };
    let mob = occupant(world, e);
    let occupant = mob.map(|m| ImplantChairOccupant {
        name: crate::examine::name_of(world, m),
        stat: match world.get::<Stat>(m) {
            Some(Stat::Dead) => 2,
            Some(Stat::Conscious) | None => 0,
            Some(_) => 1,
        },
    });
    ImplantChairData {
        occupied: mob.is_some(),
        open: world.get::<Pod>(e).is_some_and(|p| p.open),
        occupant,
        special_name: chair.special_name().map(str::to_owned),
        ready_implants: chair.ready_implants,
        ready: chair.ready,
        replenishing: chair.replenishing,
    }
}

fn refresh_view(world: &mut World, e: Entity) {
    if world.get::<ImplantChair>(e).is_none() {
        return;
    }
    let view = compute_view(world, e);
    if let Some(mut c) = world.get_mut::<ImplantChair>(e)
        && c.view != view
    {
        c.view = view;
    }
}

// Cost: per tick, one pass over the open chair windows.
fn refresh_views(world: &mut World) {
    let mut q = world.query_filtered::<&UiWindow, With<UiKind<ChairWindow>>>();
    let mut chairs: Vec<Entity> = q.iter(world).map(|w| w.source).collect();
    chairs.sort();
    chairs.dedup();
    for e in chairs {
        refresh_view(world, e);
    }
}

/// `notcontained_state`: closed for the occupant.
fn cap_windows(world: &mut World) {
    let windows: Vec<(Entity, Entity, Entity)> = world
        .query_filtered::<(Entity, &UiWindow), With<UiKind<ChairWindow>>>()
        .iter(world)
        .map(|(e, w)| (e, w.actor, w.source))
        .collect();
    for (window, user, e) in windows {
        if world.get::<InCloset>(user).is_some_and(|c| c.closet == e)
            && let Some(mut w) = world.get_mut::<UiWindow>(window)
        {
            w.cap_status(UiStatus::Close);
        }
    }
}

/// The server side of the `ImplantChair` interface.
pub struct ChairWindow;

impl UiInterface for ChairWindow {
    const NAME: &'static str = ImplantChairUi::NAME;
    type Data = ImplantChairData;
    type StaticData = ();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChairAct {
    Door,
    Implant,
}

impl ServerUi for ChairWindow {
    type Source = &'static ImplantChair;
    type UserParam = ();
    type Act = ChairAct;

    fn static_data(_: &ImplantChair) {}

    fn data(chair: &ImplantChair, _: Entity, _: &()) -> ImplantChairData {
        chair.view.clone()
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<ChairAct, UiReject> {
        Ok(match action {
            "door" => ChairAct::Door,
            "implant" => ChairAct::Implant,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

fn acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ChairWindow>>>) {
    let pending: Vec<(Entity, Entity, ChairAct)> = acts
        .read(world.resource::<Messages<UiActed<ChairWindow>>>())
        .map(|a| (a.source, a.actor, a.act))
        .collect();
    for (e, actor, act) in pending {
        // `notcontained_state`, checked again at the moment of effect.
        if world.get::<ImplantChair>(e).is_none()
            || world.get::<InCloset>(actor).is_some_and(|c| c.closet == e)
        {
            continue;
        }
        match act {
            ChairAct::Door => {
                if world.get::<Pod>(e).is_some_and(|p| p.open) {
                    close(world, e, None);
                } else {
                    open(world, e);
                }
            }
            ChairAct::Implant => implant_occupant(world, e),
        }
        refresh_view(world, e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtypes_set_their_cooldowns() {
        let plain = ImplantChair::of_path(CHAIR_ROOT);
        assert_eq!(plain.kind, ChairKind::Mindshield);
        assert_eq!(
            (plain.injection_cooldown, plain.replenish_cooldown),
            (60.0, 600.0)
        );
        assert!(plain.auto_replenish && plain.special_name().is_none());
        let purge = ImplantChair::of_path(GENEPURGE);
        assert_eq!(purge.injection_cooldown, 0.0);
        assert_eq!(purge.special_name(), Some("Purge genome"));
        let wash = ImplantChair::of_path(BRAINWASH);
        assert!(!wash.auto_replenish);
        assert_eq!(wash.injection_cooldown, 300.0);
    }
}
