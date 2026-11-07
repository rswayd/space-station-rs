//! Sleepers (server): tg `/obj/machinery/sleeper` and its subtypes
//! (`code/game/machinery/sleepers.dm`) and their `Sleeper` window (`ss13_tgui::interfaces::
//! sleeper`). The door is [`super::pods`]'.
//!
//! - Open, a mob can step in; alt-click (or the window's `door`) closes it on the living mob on
//!   its tile, a carbon dragged onto an open sleeper is shut in, a step from inside opens it.
//! - An empty hand on a working sleeper opens the window: the occupant's state, health, damage,
//!   brain and the medicines. The occupant cannot use it (`notcontained_state`) except in the
//!   syndicate, self-control and party sleepers (`controls_inside`), whose occupant clicks it
//!   from inside ([`crate::click::ControlsFromInside`]).
//! - `inject` puts 10 u of a medicine into the occupant's blood when the sleeper works, the chem
//!   is offered and allowed: the occupant has at most `20 * efficiency - 10` u of it and more
//!   health than `min_health` (epinephrine always). `efficiency` is the matter bins' tiers,
//!   `min_health` -25 per tier; each servo of tier N offers the first N medicine groups.
//! - An emag scrambles which medicine each button injects; the party pod also doses lead
//!   acetate (4 u emagged, else 1-3 u one time in five).
// shortcut: no sounds or flick animations; the party pod's spray chems
// are injected instead of a vapor exposure; no combat log; no screwdriver/crowbar/wrench acts
// (the machine frame ones apply).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_reagents::Chemistry;
use ss13_shared::chat::Span;
use ss13_shared::ui::UiStatus;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::sleeper::{SleeperChem, SleeperData, SleeperOccupant, SleeperUi};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiKind, UiParams, UiReject, UiSystems, UiWindow,
};

use super::pods::{Pod, close_pod, occupant, open_pod, operational, panel_open};
use crate::chem::{Chem, MobReagents};
use crate::closets::InCloset;
use crate::health::{HEALTH_THRESHOLD_DEAD, Health, Stat};
use crate::obj_damage::is_under;
use crate::stock_parts::PartKind;
use crate::tools::tell;

pub const SLEEPER_ROOT: &str = "/obj/machinery/sleeper";
const SYNDIE: &str = "/obj/machinery/sleeper/syndie";
const NT_FULLUPGRADE: &str = "/obj/machinery/sleeper/syndie/fullupgrade/nt";
const SELF_CONTROL: &str = "/obj/machinery/sleeper/self_control";
const OLD: &str = "/obj/machinery/sleeper/old";
const PARTY: &str = "/obj/machinery/sleeper/party";
pub const EPINEPHRINE: &str = "/datum/reagent/medicine/epinephrine";
const LEAD_ACETATE: &str = "/datum/reagent/toxin/leadacetate";
/// `REAGENT_INVISIBLE`.
const REAGENT_INVISIBLE: u32 = 1 << 1;
/// `DEFAULT_REAGENT_TEMPERATURE`.
const REAGENT_TEMPERATURE: f32 = 300.0;
/// `inject_chem`: units per injection.
pub const INJECT_UNITS: f32 = 10.0;
/// `initial(min_health)` per matter bin tier.
const MIN_HEALTH: f32 = -25.0;

/// `possible_chems`, by servo tier.
pub const POSSIBLE_CHEMS: [&[&str]; 4] = [
    &[
        EPINEPHRINE,
        "/datum/reagent/medicine/morphine",
        "/datum/reagent/medicine/c2/convermol",
        "/datum/reagent/medicine/c2/libital",
        "/datum/reagent/medicine/c2/aiuri",
    ],
    &[
        "/datum/reagent/medicine/oculine",
        "/datum/reagent/medicine/inacusiate",
    ],
    &[
        "/datum/reagent/medicine/c2/multiver",
        "/datum/reagent/medicine/mutadone",
        "/datum/reagent/medicine/mannitol",
        "/datum/reagent/medicine/salbutamol",
        "/datum/reagent/medicine/pen_acid",
    ],
    &["/datum/reagent/medicine/omnizine"],
];

/// `/obj/machinery/sleeper/party` `possible_chems`.
const PARTY_CHEMS: [&[&str]; 4] = [
    &[
        "/datum/reagent/consumable/ethanol/beer",
        "/datum/reagent/consumable/laughter",
    ],
    &["/datum/reagent/spraytan", "/datum/reagent/barbers_aid"],
    &["/datum/reagent/colorful_reagent", "/datum/reagent/hair_dye"],
    &["/datum/reagent/drug/space_drugs", "/datum/reagent/baldium"],
];

/// A sleeper's live state; its door is its [`Pod`].
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Sleeper {
    pub controls_inside: bool,
    pub party: bool,
    /// The `base_icon_state`.
    pub base_icon: &'static str,
    /// `chem_buttons` of an emagged sleeper: (button, injected) pairs; empty: every button
    /// injects its own chem.
    pub scrambled: Vec<(String, String)>,
    view: SleeperData,
}

impl Sleeper {
    pub fn of_path(path: &str) -> Self {
        let base_icon = if is_under(path, NT_FULLUPGRADE) {
            "sleeper"
        } else if is_under(path, SYNDIE) {
            "sleeper_s"
        } else if is_under(path, OLD) {
            "oldpod"
        } else if is_under(path, PARTY) {
            "partypod"
        } else {
            "sleeper"
        };
        Self {
            controls_inside: is_under(path, SYNDIE)
                || is_under(path, SELF_CONTROL)
                || is_under(path, PARTY),
            party: is_under(path, PARTY),
            base_icon,
            ..Self::default()
        }
    }

    pub fn emagged(&self) -> bool {
        !self.scrambled.is_empty()
    }
}

pub(super) fn build(app: &mut App) {
    app.register_ui::<SleeperWindow>().add_systems(
        Update,
        (
            cap_windows.in_set(UiSystems::StatusHook),
            acts.in_set(UiSystems::Handle),
            refresh_views
                .after(UiSystems::Handle)
                .before(UiSystems::Send),
        ),
    );
}

/// `RefreshParts`: the chems the servos offer (each of tier N the first N groups, no repeats).
pub fn available_chems(
    servo_tiers: impl IntoIterator<Item = u32>,
    party: bool,
) -> Vec<&'static str> {
    let groups = if party { &PARTY_CHEMS } else { &POSSIBLE_CHEMS };
    let mut out: Vec<&'static str> = Vec::new();
    for tier in servo_tiers {
        for group in groups.iter().take(tier.min(4) as usize) {
            for &chem in group.iter() {
                if !out.contains(&chem) {
                    out.push(chem);
                }
            }
        }
    }
    out
}

/// `RefreshParts`: (`efficiency`, `min_health`, `available_chems`).
pub fn parts(world: &World, e: Entity) -> (f32, f32, Vec<&'static str>) {
    let party = world.get::<Sleeper>(e).is_some_and(|s| s.party);
    let r = crate::stock_parts::ratings(world, e);
    let bins = r.sum(PartKind::MatterBin) as f32;
    let tiers: Vec<u32> = r.tiers(PartKind::Servo).collect();
    (bins, MIN_HEALTH * bins, available_chems(tiers, party))
}

/// `chem_allowed`: room for another injection under `20 * efficiency`, and the occupant above
/// `min_health` (epinephrine is always allowed).
pub fn chem_allowed(amount: f32, efficiency: f32, health: f32, min_health: f32, epi: bool) -> bool {
    amount + INJECT_UNITS <= 20.0 * efficiency && (health > min_health || epi)
}

fn amount_of(chem: &Chemistry, reagents: Option<&MobReagents>, path: &str) -> f32 {
    match (chem.id(path), reagents) {
        (Some(id), Some(r)) => r.blood.amount(id),
        _ => 0.0,
    }
}

/// `close_machine` of the sleeper: the occupant is told it went numb.
pub fn close(world: &mut World, e: Entity, target: Option<Entity>) {
    if panel_open(world, e) {
        return;
    }
    if let Some(mob) = close_pod(world, e, target)
        && world.get::<Stat>(mob) != Some(&Stat::Dead)
    {
        tell_mob(
            world,
            mob,
            Span::BoldNotice,
            "Вас окутывает прохладный воздух. Вы цепенеете, ваши чувства обращаются внутрь.",
        );
    }
    refresh_view(world, e);
}

/// `open_machine` of the sleeper.
pub fn open(world: &mut World, e: Entity) {
    open_pod(world, e);
    refresh_view(world, e);
}

use crate::tools::tell_mob;

/// `update_icon_state`: `[base]-open`, `[base]-o` (panel), `[base]`.
pub fn refresh_look(world: &mut World, e: Entity) {
    let Some(base) = world.get::<Sleeper>(e).map(|s| s.base_icon) else {
        return;
    };
    let open = world.get::<Pod>(e).is_some_and(|p| p.open);
    let state = if open {
        format!("{base}-open")
    } else if panel_open(world, e) {
        format!("{base}-o")
    } else {
        base.to_owned()
    };
    crate::obj_damage::set_icon_state(world, e, &state);
}

/// `interact` -> `ui_interact`: the window of a working sleeper.
pub fn interact(world: &mut World, client: Entity, mob: Entity, e: Entity) {
    if !operational(world, e) {
        return;
    }
    refresh_view(world, e);
    let title = super::name_of(world, e);
    world.write_message(OpenUi::new::<SleeperWindow>(client, mob, e, title));
}

/// `emag_act`: the buttons inject each other's chems. False when it was emagged already.
pub fn emag_act(world: &mut World, client: Entity, e: Entity) -> bool {
    if world.get::<Sleeper>(e).is_none_or(Sleeper::emagged) {
        return false;
    }
    let (_, _, available) = parts(world, e);
    let mut pool: Vec<&str> = available.clone();
    let mut rng = super::fork_rng(world);
    let mut scrambled = Vec::new();
    for chem in available {
        let i = rng.range(0, pool.len() as u32 - 1) as usize;
        scrambled.push((chem.to_owned(), pool.remove(i).to_owned()));
    }
    if let Some(mut s) = world.get_mut::<Sleeper>(e) {
        s.scrambled = scrambled;
    }
    tell(world, client, Span::Notice, "интерфейс перемешан");
    true
}

/// `ui_data`.
fn compute_view(world: &mut World, e: Entity) -> SleeperData {
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return SleeperData::default();
    };
    let open = world.get::<Pod>(e).is_some_and(|p| p.open);
    let mob = occupant(world, e);
    let (efficiency, min_health, available) = parts(world, e);
    let reagents = mob.and_then(|m| world.get::<MobReagents>(m));
    let health = mob.and_then(|m| world.get::<Health>(m));
    let chems = available
        .iter()
        .filter_map(|&path| {
            let id = chem.id(path)?;
            let allowed = health.is_some_and(|h| {
                chem_allowed(
                    amount_of(&chem, reagents, path),
                    efficiency,
                    h.health(),
                    min_health,
                    path == EPINEPHRINE,
                )
            });
            Some(SleeperChem {
                name: chem.def(id).name.clone(),
                id: path.to_owned(),
                allowed,
            })
        })
        .collect();
    let occupant = mob.map(|m| occupant_data(world, &chem, m));
    SleeperData {
        occupied: mob.is_some(),
        open,
        chems,
        occupant,
    }
}

/// `ui_data["occupant"]`.
fn occupant_data(world: &World, chem: &Chemistry, mob: Entity) -> SleeperOccupant {
    let stat = world.get::<Stat>(mob).copied().unwrap_or_default();
    let knocked_out = crate::status_effects::incapacitation(world, mob).knocked_out;
    let (text, state) = match stat {
        Stat::Dead => ("Dead", "bad"),
        Stat::HardCrit => ("Critical", "bad"),
        Stat::SoftCrit => ("Critical", "average"),
        Stat::Conscious if knocked_out => ("Unconscious", "average"),
        Stat::Conscious => ("Stable", "good"),
    };
    let health = world.get::<Health>(mob).cloned().unwrap_or_default();
    let brain =
        super::organs::state(world, mob, super::organs::Slot::Brain).map_or(0.0, |o| o.damage);
    let reagents = world
        .get::<MobReagents>(mob)
        .map(|r| {
            r.blood
                .reagents
                .iter()
                .filter(|r| chem.def(r.id).chemical_flags & REAGENT_INVISIBLE == 0)
                .map(|r| (chem.def(r.id).name.clone(), r.volume))
                .collect()
        })
        .unwrap_or_default();
    SleeperOccupant {
        name: super::name_of(world, mob),
        stat: text.to_owned(),
        statstate: state.to_owned(),
        health: health.health(),
        max_health: health.max,
        min_health: HEALTH_THRESHOLD_DEAD,
        brute_loss: health.brute,
        oxy_loss: health.oxy,
        tox_loss: health.tox,
        fire_loss: health.burn,
        brain_loss: brain,
        reagents,
    }
}

fn refresh_view(world: &mut World, e: Entity) {
    if world.get::<Sleeper>(e).is_none() {
        return;
    }
    let view = compute_view(world, e);
    if let Some(mut s) = world.get_mut::<Sleeper>(e)
        && s.view != view
    {
        s.view = view;
    }
}

/// Sleepers with an open window get fresh data.
// Cost: per tick, one pass over the open windows; a sleeper's data is a dozen short entries.
fn refresh_views(world: &mut World) {
    let mut q = world.query_filtered::<&UiWindow, With<UiKind<SleeperWindow>>>();
    let mut sleepers: Vec<Entity> = q.iter(world).map(|w| w.source).collect();
    sleepers.sort();
    sleepers.dedup();
    for e in sleepers {
        refresh_view(world, e);
    }
}

/// `ui_status`: closed for a broken or unpowered sleeper and for its occupant
/// (`notcontained_state`) unless the controls are inside.
fn cap_windows(world: &mut World) {
    let windows: Vec<(Entity, Entity, Entity)> = world
        .query_filtered::<(Entity, &UiWindow), With<UiKind<SleeperWindow>>>()
        .iter(world)
        .map(|(e, w)| (e, w.actor, w.source))
        .collect();
    for (window, user, e) in windows {
        let inside = world.get::<InCloset>(user).is_some_and(|c| c.closet == e);
        let controls_inside = world.get::<Sleeper>(e).is_some_and(|s| s.controls_inside);
        if (!operational(world, e) || (inside && !controls_inside))
            && let Some(mut w) = world.get_mut::<UiWindow>(window)
        {
            w.cap_status(UiStatus::Close);
        }
    }
}

/// The server side of the `Sleeper` interface.
pub struct SleeperWindow;

impl UiInterface for SleeperWindow {
    const NAME: &'static str = SleeperUi::NAME;
    type Data = SleeperData;
    type StaticData = ();
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SleeperAct {
    Door,
    /// `chem`: a reagent type path.
    Inject(String),
}

impl ServerUi for SleeperWindow {
    type Source = &'static Sleeper;
    type UserParam = ();
    type Act = SleeperAct;

    fn static_data(_: &Sleeper) {}

    fn data(sleeper: &Sleeper, _: Entity, _: &()) -> SleeperData {
        sleeper.view.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<SleeperAct, UiReject> {
        Ok(match action {
            "door" => SleeperAct::Door,
            "inject" => SleeperAct::Inject(
                params
                    .get("chem")
                    .filter(|c| c.len() <= 128)
                    .map(str::to_owned)
                    .ok_or_else(|| UiReject::BadParams("chem".into()))?,
            ),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

fn acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<SleeperWindow>>>) {
    let pending: Vec<(Entity, Entity, SleeperAct)> = acts
        .read(world.resource::<Messages<UiActed<SleeperWindow>>>())
        .map(|a| (a.source, a.actor, a.act.clone()))
        .collect();
    for (e, actor, act) in pending {
        if world.get::<Sleeper>(e).is_none() {
            continue;
        }
        match act {
            SleeperAct::Door => {
                if world.get::<Pod>(e).is_some_and(|p| p.open) {
                    close(world, e, None);
                } else {
                    open(world, e);
                }
            }
            SleeperAct::Inject(chem) => {
                inject(world, e, actor, &chem);
            }
        }
        refresh_view(world, e);
    }
}

/// `ui_act("inject")` -> `inject_chem`, revalidated at the moment of effect: the sleeper works,
/// has an occupant, offers the chem and allows it. True when it injected.
pub fn inject(world: &mut World, e: Entity, user: Entity, path: &str) -> bool {
    let Some(state) = world.get::<Sleeper>(e).cloned() else {
        return false;
    };
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return false;
    };
    if !operational(world, e) {
        return false;
    }
    let Some(mob) = occupant(world, e) else {
        return false;
    };
    let (efficiency, min_health, available) = parts(world, e);
    let Some(&offered) = available.iter().find(|&&c| c == path) else {
        return false;
    };
    let health = world.get::<Health>(mob).map_or(0.0, Health::health);
    let amount = amount_of(&chem, world.get::<MobReagents>(mob), offered);
    if !chem_allowed(
        amount,
        efficiency,
        health,
        min_health,
        offered == EPINEPHRINE,
    ) {
        return false;
    }
    // The emag picks what goes in after the checks of the intended chem.
    let injected = state
        .scrambled
        .iter()
        .find(|(button, _)| button == offered)
        .map_or(offered.to_owned(), |(_, real)| real.clone());
    if state.party {
        // `party/inject_chem`: lead acetate, more when emagged.
        let mut rng = super::fork_rng(world);
        let lead = if state.emagged() {
            4.0
        } else if rng.chance(20.0) {
            rng.range(1, 3) as f32
        } else {
            0.0
        };
        if lead > 0.0 {
            add_blood(world, &chem, mob, LEAD_ACETATE, lead);
        }
    }
    add_blood(world, &chem, mob, &injected, INJECT_UNITS);
    if state.emagged()
        && super::fork_rng(world).chance(5.0)
        && let Some(client) = world
            .get::<crate::server::PlayerMob>(user)
            .map(|p| p.client)
    {
        tell(
            world,
            client,
            Span::Warning,
            "Обнаружено перенаправление химической системы, результаты могут отличаться от \
             ожидаемых!",
        );
    }
    true
}

fn add_blood(world: &mut World, chem: &Chemistry, mob: Entity, path: &str, amount: f32) {
    let Some(id) = chem.id(path) else {
        return;
    };
    if let Some(mut reagents) = world.get_mut::<MobReagents>(mob) {
        reagents.blood.add_reagent(
            chem,
            id,
            amount,
            REAGENT_TEMPERATURE,
            None,
            None,
            false,
            false,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn servos_offer_the_groups_of_their_tier() {
        assert!(available_chems([], false).is_empty());
        let one = available_chems([1], false);
        assert_eq!(one.len(), 5);
        assert_eq!(one[0], EPINEPHRINE);
        // A tier 1 and a tier 3 servo: groups 1-3 once each.
        let mixed = available_chems([1, 3], false);
        assert_eq!(mixed.len(), 12);
        assert!(!mixed.contains(&"/datum/reagent/medicine/omnizine"));
        assert_eq!(available_chems([4], false).len(), 13);
        assert_eq!(available_chems([9], true).len(), 8);
    }

    #[test]
    fn injections_stop_at_the_matter_bins_limit_and_low_health() {
        // Efficiency 1: 20 u at most, 10 per injection.
        assert!(chem_allowed(0.0, 1.0, 50.0, -25.0, false));
        assert!(chem_allowed(10.0, 1.0, 50.0, -25.0, false));
        assert!(!chem_allowed(10.5, 1.0, 50.0, -25.0, false));
        // Below `min_health` only epinephrine.
        assert!(!chem_allowed(0.0, 1.0, -30.0, -25.0, false));
        assert!(chem_allowed(0.0, 1.0, -30.0, -25.0, true));
        // No matter bins: nothing.
        assert!(!chem_allowed(0.0, 0.0, 100.0, 0.0, true));
    }

    #[test]
    fn subtypes_pick_their_icon_and_controls() {
        let s = Sleeper::of_path("/obj/machinery/sleeper/syndie/fullupgrade");
        assert!(s.controls_inside && s.base_icon == "sleeper_s");
        let nt = Sleeper::of_path(NT_FULLUPGRADE);
        assert!(nt.controls_inside && nt.base_icon == "sleeper");
        let plain = Sleeper::of_path(SLEEPER_ROOT);
        assert!(!plain.controls_inside && !plain.party && !plain.emagged());
        let party = Sleeper::of_path(PARTY);
        assert!(party.party && party.base_icon == "partypod");
    }
}
