//! Portable turrets and the turret control panel (server), after tg `/obj/machinery/porta_turret`
//! (`portable_turret.dm`) and `/obj/machinery/turretid` (`turret_control.dm`).
//!
//! - A turret ([`Turret`], its type data [`DmTurret`] with the projectiles of its installed gun
//!   resolved by `ss13-mapload`) is processed every 2 s (tg `SSturrets`): while on and powered it
//!   looks at the mobs it can see within `scan_range` (tg `view`). Basic mobs are targets with
//!   `TURRET_FLAG_SHOOT_ANOMALOUS` while conscious and not of its faction; cyborgs with
//!   `TURRET_FLAG_SHOOT_BORGS`; humans when [`assess_perp`] gives at least 4 (in stun mode only
//!   those who can still act, in lethal mode the living). It picks one at random, pops up (0.5 s,
//!   `popUp`), turns to it and fires its stun or lethal projectile once `shot_delay` passed; with
//!   nobody to shoot it pops down unless `always_up`.
//! - [`assess_perp`]: emagged 10; angry (`TURRET_FLAG_SHOOT_ALL`/`_REACT`) at anyone without its
//!   access 10; heads of staff 0 unless `TURRET_FLAG_SHOOT_HEADS`; unauthorized weapons (no access,
//!   a gun or baton in hand 4, on the belt 2); a wanted (arrest) or unknown record 4; no mindshield
//!   4. AI turrets and syndicate turrets shoot everyone not of their faction (10), the
//!   CentCom/auxiliary base ones no human (0), laser tag turrets the other team.
//! - Its window (`PortableTurret`, empty hand) toggles power and the target flags; an ID with
//!   its access (security) locks and unlocks it. An emag shorts its threat assessment (everyone
//!   is a target, panels no longer control it, off for 6 s); an EMP scrambles the flags and
//!   turns it off for 6-20 s.
//! - The control panel ([`TurretControl`], `TurretControl` window) drives every turret of its
//!   `control_area` (its own area by default): on/off, stun/lethal, cyborgs as targets
//!   (`setState`). It opens for anyone; an ID with its access (AI upload) locks and unlocks it,
//!   a silicon toggles the lock from the window; locked, only silicons change it. An emag
//!   unlocks it for good; an EMP may scramble it.
// The windows' settings are refused on the server while locked (tg only greys the buttons out).
// shortcut: no turret construction, wrenching, crowbar salvage, damage or breaking (the
// `_REACT` anger on damage, the broken sprite), wall turrets shooting from the next tile,
// manual (AI) control, mechs and blobs as targets, power drawn per shot; the cover is the
// turret's own sprite (`turretCover` / `openTurretCover` under the gun) without the pop-up
// animation; the perp's record is found by its real name (tg: the face/ID name).
// Cost: every 2 s per turret that is on (~100 on a station), one `in_radius(scan_range)` of the
// spatial index and a line-of-sight check per living mob found.

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::wallmount::TURRETID_ROOT;
use ss13_mapload::{Areas, DmAtom, DmItem, DmName, DmPower, DmTurret, DmWallMount};
use ss13_net::server::Networked;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Facing, Overlay, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::SpatialIndex;

use super::*;
use crate::combat::CombatRng;
use crate::emp::{EmpAct, EmpRng, EmpSystems, emp_acts};
use crate::health::{Health, Stat};
use crate::inventory::{IdCard, Inventory, check_access};
use crate::mobs::{BasicMob, Faction};
use crate::power::Unpowered;
use crate::silicon::cyborg::Cyborg;
use crate::silicon::interact::{SiliconUsers, has_silicon_access};
use crate::tools::tell;
use crate::zone::BodyZone;

/// `SSturrets` `wait`.
pub const PROCESS_SECS: f64 = 2.0;
/// `POPUP_ANIM_TIME`, `POPDOWN_ANIM_TIME`.
pub const POPUP_SECS: f64 = 0.5;
/// `set_disabled()`'s default and the emag's.
const DISABLE_SECS: f64 = 6.0;
const TURRETS_DMI: &str = "icons/obj/weapons/turrets.dmi";
const CHAMELEON_ID: &str = "/obj/item/card/id/advanced/chameleon";
const GUN_ROOT: &str = "/obj/item/gun";
const BATON_ROOT: &str = "/obj/item/melee/baton";
const REDTAG_SUIT: &str = "/obj/item/clothing/suit/redtag";
const BLUETAG_SUIT: &str = "/obj/item/clothing/suit/bluetag";
const REDTAG_GUN: &str = "/obj/item/gun/energy/laser/redtag";
const BLUETAG_GUN: &str = "/obj/item/gun/energy/laser/bluetag";

/// A turret's live state (tg vars).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Turret {
    pub on: bool,
    /// `mode == TURRET_LETHAL`.
    pub lethal: bool,
    pub raised: bool,
    /// `raising`: when the cover finishes moving, and whether it goes up.
    pub raising: Option<(f64, bool)>,
    /// The behaviour controls (window) are locked.
    pub locked: bool,
    /// `controllock`: control panels no longer reach it.
    pub controllock: bool,
    pub emagged: bool,
    pub flags: u32,
    pub last_fired: f64,
    /// `disabled_time`: it cannot be turned on before this.
    pub disabled_until: f64,
    /// `set_disabled(will_restart)`: turned back on at this time.
    pub restart_at: Option<f64>,
    pub next_process: f64,
}

/// The turret control panel (tg `/obj/machinery/turretid`).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct TurretControl {
    pub enabled: bool,
    pub lethal: bool,
    pub locked: bool,
    pub ailock: bool,
    pub shoot_cyborgs: bool,
    pub emagged: bool,
    pub req_access: Vec<String>,
    /// `turrets`.
    pub turrets: Vec<Entity>,
}

pub struct DefensesPlugin;

impl Plugin for DefensesPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `attack_hand` of turrets and the turret control panel: their windows.
        app.add_click_handler(ClickPhase::EmptyHand, 3300, |w, c| {
            crate::defenses::server::interact(w, c.client, c.mob, c.target)
        });
        // Turrets, the turret control panel and cameras: IDs, tools, assemblies.
        app.add_click_handler(ClickPhase::HeldOnTarget, 800, |w, c| {
            c.held.is_some_and(|h| {
                crate::defenses::server::item_interaction(
                    w,
                    c.client,
                    c.mob,
                    h,
                    c.target,
                    c.modifiers,
                )
            })
        });
        app.register_ui::<TurretWindow>()
            .register_ui::<TurretControlWindow>()
            .add_systems(ss13_world::MapInitLate, (init_turrets, init_panels).chain())
            .add_systems(
                Update,
                (
                    process_turrets
                        .after(crate::click::ResolveClicks)
                        .before(crate::projectile::ProjectileSystems),
                    turret_acts.in_set(UiSystems::Handle),
                    panel_acts.in_set(UiSystems::Handle),
                    emp_turrets.in_set(EmpSystems::Effects),
                ),
            )
            .add_plugins(super::cameras::CameraBuildPlugin);
    }
}

/// tg turret `faction` by type.
fn faction_of(path: &str) -> Vec<String> {
    let neutral = || ["neutral", "silicon", "turret"].map(str::to_owned).to_vec();
    if is_under(path, "/obj/machinery/porta_turret/syndicate/energy/raven")
        || is_under(path, "/obj/machinery/porta_turret/aux_base")
        || is_under(path, "/obj/machinery/porta_turret/centcom_shuttle")
    {
        neutral()
    } else if is_under(path, "/obj/machinery/porta_turret/syndicate/irs") {
        vec!["pirate".to_owned()]
    } else if is_under(path, "/obj/machinery/porta_turret/syndicate") {
        vec!["Syndicate".to_owned()]
    } else {
        vec!["turret".to_owned()]
    }
}

use crate::tools::now;

fn init_turrets(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmTurret), With<ss13_world::Mapped>>();
    let turrets: Vec<(Entity, DmTurret)> =
        query.iter(world).map(|(e, dm)| (e, dm.clone())).collect();
    info!("{} turrets", turrets.len());
    for (i, (e, dm)) in turrets.into_iter().enumerate() {
        let up = !dm.has_cover || dm.always_up;
        world.entity_mut(e).insert((
            Turret {
                on: true,
                lethal: dm.lethal_mode,
                raised: up,
                raising: None,
                locked: true,
                controllock: false,
                emagged: false,
                flags: dm.turret_flags,
                last_fired: f64::NEG_INFINITY,
                disabled_until: 0.0,
                restart_at: None,
                // Spread over the period: not every turret on the same tick.
                next_process: (i % 20) as f64 * PROCESS_SECS / 20.0,
            },
            Faction(faction_of(&dm.path)),
            // Map load skips it (`invisibility` while under its cover): the cover is its sprite.
            Networked,
        ));
        refresh_look(world, e);
    }
}

fn init_panels(world: &mut World) {
    let mut query =
        world.query_filtered::<(Entity, &DmWallMount, &TilePos), With<ss13_world::Mapped>>();
    let panels: Vec<(Entity, DmWallMount, TilePos)> = query
        .iter(world)
        .filter(|(_, dm, _)| is_under(&dm.path, TURRETID_ROOT))
        .map(|(e, dm, &p)| (e, dm.clone(), p))
        .collect();
    let mut turrets = world.query_filtered::<(Entity, &TilePos), With<Turret>>();
    let turrets: Vec<(Entity, TilePos)> = turrets.iter(world).map(|(e, &p)| (e, p)).collect();
    for (e, dm, pos) in panels {
        let area = control_area(world, &dm.control_area, pos);
        let linked: Vec<Entity> = turrets
            .iter()
            .filter(|(_, p)| area.is_some() && world.resource::<Areas>().area_at(*p) == area)
            .map(|(t, _)| *t)
            .collect();
        world.entity_mut(e).insert(TurretControl {
            enabled: dm.enabled,
            lethal: dm.lethal,
            locked: dm.locked,
            ailock: dm.ailock,
            shoot_cyborgs: dm.shoot_cyborgs,
            emagged: false,
            req_access: dm.req_access.clone(),
            turrets: linked,
        });
        refresh_panel_look(world, e);
    }
}

/// `get_area_instance_from_text(control_area)`, else the panel's own area.
fn control_area(world: &mut World, path: &str, pos: TilePos) -> Option<Entity> {
    let areas = world.resource::<Areas>().entities().to_vec();
    let named = (!path.is_empty())
        .then(|| {
            areas
                .into_iter()
                .find(|&a| world.get::<DmPower>(a).is_some_and(|p| p.path == path))
        })
        .flatten();
    if named.is_none() && !path.is_empty() {
        warn!("turret control panel at {pos:?}: bad control_area {path}");
    }
    named.or_else(|| world.resource::<Areas>().area_at(pos))
}

// ---------------------------------------------------------------------------------------------
// Looks

fn icon(world: &World, e: Entity, state: &str) -> Option<ss13_shared::IconStateId> {
    let dmi = world
        .get::<DmAtom>(e)
        .map(|a| a.icon.clone())
        .filter(|i| !i.is_empty())
        .unwrap_or_else(|| TURRETS_DMI.to_owned());
    world.get_resource::<IconIndex>()?.get(&dmi, state)
}

/// `update_icon_state` of the turret and its cover: the closed cover while down, the gun over
/// the open cover while up (`_stun`, `_lethal`, `_off` unpowered or off).
pub fn refresh_look(world: &mut World, e: Entity) {
    let (Some(t), Some(dm)) = (world.get::<Turret>(e), world.get::<DmTurret>(e)) else {
        return;
    };
    let powered = world.get::<Unpowered>(e).is_none();
    let gun = format!(
        "{}_{}",
        dm.base_icon_state,
        if !powered || !t.on || !t.raised {
            "off"
        } else if t.lethal {
            "lethal"
        } else {
            "stun"
        }
    );
    let (base, over) = if !dm.has_cover {
        (gun, None)
    } else if t.raised {
        ("openTurretCover".to_owned(), Some(gun))
    } else {
        ("turretCover".to_owned(), None)
    };
    let base = icon(world, e, &base);
    let over = over.and_then(|s| icon(world, e, &s));
    let Some(mut look) = world.get_mut::<Appearance>(e) else {
        return;
    };
    if let Some(base) = base {
        look.icon_state = base;
    }
    look.overlays = over.map(Overlay::new).into_iter().collect();
}

/// `turretid/update_overlays`: the buttons and the screen (`stun`, `kill`, `standby`).
pub fn refresh_panel_look(world: &mut World, e: Entity) {
    let Some(p) = world.get::<TurretControl>(e) else {
        return;
    };
    let powered = world.get::<Unpowered>(e).is_none();
    let mut states: Vec<&str> = Vec::new();
    if p.enabled {
        states.push("button_left");
        if p.lethal {
            states.push("button_right");
        }
        if powered {
            states.push(if p.lethal { "kill" } else { "stun" });
        }
    } else if powered {
        states.push("standby");
    }
    let overlays: Vec<Overlay> = states
        .into_iter()
        .filter_map(|s| icon(world, e, s))
        .map(Overlay::new)
        .collect();
    if let Some(mut look) = world.get_mut::<Appearance>(e) {
        look.overlays = overlays;
    }
}

// ---------------------------------------------------------------------------------------------
// State changes

/// `toggle_on(turn_on)`: refused to turn on while disabled; turning off pops it down.
pub fn toggle_on(world: &mut World, e: Entity, turn_on: bool) {
    let t_now = now(world);
    let Some(mut t) = world.get_mut::<Turret>(e) else {
        return;
    };
    if t.on == turn_on || (turn_on && t_now < t.disabled_until) {
        return;
    }
    t.on = turn_on;
    if !turn_on {
        pop_down(world, e);
    }
    refresh_look(world, e);
}

/// `set_disabled(duration)`: off for the duration, then back on if it was on.
pub fn set_disabled(world: &mut World, e: Entity, secs: f64) {
    let t_now = now(world);
    let Some(mut t) = world.get_mut::<Turret>(e) else {
        return;
    };
    t.disabled_until = t_now + secs;
    if t.on {
        t.restart_at = Some(t_now + secs);
    }
    toggle_on(world, e, false);
}

/// `setState(on, mode, shoot_cyborgs)` from a control panel.
pub fn set_state(world: &mut World, e: Entity, on: bool, lethal: bool, shoot_cyborgs: bool) {
    let Some(mut t) = world.get_mut::<Turret>(e) else {
        return;
    };
    if t.controllock {
        return;
    }
    if shoot_cyborgs {
        t.flags |= TURRET_FLAG_SHOOT_BORGS;
    } else {
        t.flags &= !TURRET_FLAG_SHOOT_BORGS;
    }
    t.lethal = lethal;
    toggle_on(world, e, on);
    refresh_look(world, e);
}

/// `popUp`: the cover opens; raised 0.5 s later.
fn pop_up(world: &mut World, e: Entity) {
    let t_now = now(world);
    if let Some(mut t) = world.get_mut::<Turret>(e)
        && !t.raised
        && t.raising.is_none()
    {
        t.raising = Some((t_now + POPUP_SECS, true));
    }
}

/// `popDown`: the cover closes; down 0.5 s later.
fn pop_down(world: &mut World, e: Entity) {
    let t_now = now(world);
    if let Some(mut t) = world.get_mut::<Turret>(e)
        && t.raised
        && t.raising.is_none()
    {
        t.raising = Some((t_now + POPUP_SECS, false));
    }
}

// ---------------------------------------------------------------------------------------------
// Targets

/// tg `allowed(perp)` against the turret's `req_access`.
fn allowed(world: &World, req_access: &[String], mob: Entity) -> bool {
    let access = world
        .get::<Inventory>(mob)
        .and_then(|i| i.id_card(world))
        .and_then(|c| world.get::<IdCard>(c))
        .map(|c| c.access.clone())
        .unwrap_or_default();
    check_access(req_access, &[], &access)
}

fn item_under(world: &World, item: Option<Entity>, root: &str) -> bool {
    item.and_then(|i| world.get::<DmItem>(i))
        .is_some_and(|i| is_under(&i.path, root))
}

/// tg `assess_perp(perp)` of the turret's type: the threat a human is (4 or more: a target).
pub fn assess_perp(world: &World, turret: &Turret, dm: &DmTurret, perp: Entity) -> i32 {
    let path = dm.path.as_str();
    if is_under(path, "/obj/machinery/porta_turret/syndicate")
        || is_under(path, "/obj/machinery/porta_turret/ai")
    {
        return 10;
    }
    if is_under(path, "/obj/machinery/porta_turret/aux_base")
        || is_under(path, "/obj/machinery/porta_turret/centcom_shuttle")
    {
        return 0;
    }
    let inventory = world.get::<Inventory>(perp);
    let held =
        |root: &str| inventory.is_some_and(|i| i.hands.iter().any(|&h| item_under(world, h, root)));
    let belt = |root: &str| item_under(world, inventory.and_then(|i| i.belt), root);
    if !dm.team_color.is_empty() {
        // `lasertag/assess_perp`: the other team's vest and guns.
        let (suit, gun) = if dm.team_color == "blue" {
            (REDTAG_SUIT, REDTAG_GUN)
        } else {
            (BLUETAG_SUIT, BLUETAG_GUN)
        };
        let mut threat = 0;
        if item_under(world, inventory.and_then(|i| i.suit), suit) {
            threat += 4;
        }
        if held(gun) {
            threat += 4;
        }
        if belt(gun) {
            threat += 2;
        }
        return threat;
    }
    if turret.emagged {
        return 10;
    }
    let flags = turret.flags;
    if flags & (TURRET_FLAG_SHOOT_ALL | TURRET_FLAG_SHOOT_ALL_REACT) != 0
        && !allowed(world, &dm.req_access, perp)
    {
        return 10;
    }
    if flags & TURRET_FLAG_SHOOT_HEADS == 0
        && crate::antag::revolution::server::is_head_of_staff(world, perp)
    {
        return 0;
    }
    let mut threat = 0;
    if flags & TURRET_FLAG_AUTH_WEAPONS != 0
        && !item_under(world, inventory.and_then(|i| i.id), CHAMELEON_ID)
    {
        if allowed(world, &dm.req_access, perp) {
            return 0;
        }
        if held(GUN_ROOT) || held(BATON_ROOT) {
            threat += 4;
        }
        if belt(GUN_ROOT) || belt(BATON_ROOT) {
            threat += 2;
        }
    }
    if flags & TURRET_FLAG_SHOOT_CRIMINALS != 0 {
        let name = world
            .get::<Name>(perp)
            .map(|n| n.as_str().to_owned())
            .unwrap_or_default();
        let record = world
            .get_resource::<crate::records::manifest::Manifest>()
            .and_then(|m| m.find(&name));
        if record.is_none_or(|r| r.wanted_status == crate::records::WANTED_ARREST) {
            threat += 4;
        }
    }
    if flags & TURRET_FLAG_SHOOT_UNSHIELDED != 0
        && !crate::antag::revolution::server::mindshielded(world, perp)
    {
        threat += 4;
    }
    threat
}

/// tg `process()`'s target list: what the turret would shoot among what it sees.
pub fn targets(world: &World, e: Entity) -> Vec<Entity> {
    let (Some(t), Some(dm), Some(&pos)) = (
        world.get::<Turret>(e),
        world.get::<DmTurret>(e),
        world.get::<TilePos>(e),
    ) else {
        return Vec::new();
    };
    let range = dm.scan_range.max(1.0) as u32;
    let index = world.resource::<SpatialIndex>();
    let mut out = Vec::new();
    for mob in index.in_radius(pos, range) {
        if world.get::<Health>(mob).is_none()
            || world.get::<crate::bots::Bot>(mob).is_some()
            || world
                .get::<TilePos>(mob)
                .is_none_or(|p| !crate::mobs::ai::behaviors::can_see(world, pos, *p, range))
        {
            continue;
        }
        let stat = world.get::<Stat>(mob).copied().unwrap_or_default();
        let friend = crate::mobs::same_faction(world, e, mob);
        if world.get::<BasicMob>(mob).is_some() {
            if t.flags & TURRET_FLAG_SHOOT_ANOMALOUS != 0 && stat == Stat::Conscious && !friend {
                out.push(mob);
            }
            continue;
        }
        if world.get::<crate::silicon::Silicon>(mob).is_some() {
            if t.flags & TURRET_FLAG_SHOOT_BORGS != 0
                && world.get::<Cyborg>(mob).is_some()
                && stat == Stat::Conscious
                && !friend
            {
                out.push(mob);
            }
            continue;
        }
        if t.lethal {
            if stat == Stat::Dead {
                continue;
            }
        } else {
            // Stun mode: only those who can still use items (`MOBILITY_USE`, not incapacitated).
            let effects = world.get::<crate::status_effects::StatusEffects>(mob);
            if stat != Stat::Conscious
                || effects.is_some_and(|s| s.incapacitated() || s.floored())
                || world.get::<crate::restraints::Handcuffed>(mob).is_some()
            {
                continue;
            }
        }
        if !friend && assess_perp(world, t, dm, mob) >= 4 {
            out.push(mob);
        }
    }
    out
}

/// Every [`PROCESS_SECS`]: the turrets that are on look for targets; every tick the covers
/// finish moving and disabled turrets come back on.
fn process_turrets(world: &mut World) {
    let t_now = now(world);
    let mut query = world.query::<(Entity, &Turret)>();
    let due: Vec<(Entity, Turret)> = query.iter(world).map(|(e, t)| (e, t.clone())).collect();
    for (e, t) in due {
        if let Some((at, up)) = t.raising
            && t_now >= at
        {
            if let Some(mut t) = world.get_mut::<Turret>(e) {
                t.raising = None;
                t.raised = up;
            }
            refresh_look(world, e);
        }
        if let Some(at) = t.restart_at
            && t_now > at
        {
            if let Some(mut t) = world.get_mut::<Turret>(e) {
                t.restart_at = None;
            }
            toggle_on(world, e, true);
        }
        if t_now < t.next_process {
            continue;
        }
        if let Some(mut t) = world.get_mut::<Turret>(e) {
            t.next_process = t_now + PROCESS_SECS;
        }
        process(world, e);
    }
}

/// tg `process()` of one turret.
fn process(world: &mut World, e: Entity) {
    let Some(t) = world.get::<Turret>(e) else {
        return;
    };
    // Off or unpowered: down, also when it was still popping up as it went off.
    if !t.on || world.get::<Unpowered>(e).is_some() {
        pop_down(world, e);
        return;
    }
    let mut found = targets(world, e);
    if found.is_empty() {
        if !world.get::<DmTurret>(e).is_some_and(|d| d.always_up) {
            pop_down(world, e);
        }
        return;
    }
    let pick = world
        .resource_mut::<CombatRng>()
        .range(0, found.len() as u32 - 1) as usize;
    let target = found.swap_remove(pick);
    pop_up(world, e);
    let (from, to) = (
        world.get::<TilePos>(e).copied(),
        world.get::<TilePos>(target).copied(),
    );
    if let (Some(from), Some(to)) = (from, to)
        && let Some(dir) = crate::pull::dir_between(from, to)
    {
        world.entity_mut(e).insert(Facing(dir));
    }
    shoot_at(world, e, target);
}

/// `shootAt`: raised, after `shot_delay` (emagged: no delay), the mode's projectile.
pub fn shoot_at(world: &mut World, e: Entity, target: Entity) -> bool {
    let t_now = now(world);
    let (Some(t), Some(dm)) = (world.get::<Turret>(e), world.get::<DmTurret>(e)) else {
        return false;
    };
    if !t.raised {
        return false;
    }
    if !t.emagged && t.last_fired + f64::from(dm.shot_delay) / 10.0 > t_now {
        return false;
    }
    let (data, sound) = if t.lethal {
        (dm.lethal.clone(), dm.lethal_sound.clone())
    } else {
        (dm.stun.clone(), dm.stun_sound.clone())
    };
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(e), world.get::<TilePos>(target)) else {
        return false;
    };
    if let Some(mut t) = world.get_mut::<Turret>(e) {
        t.last_fired = t_now;
    }
    crate::sound::playsound(world, crate::sound::PlaySound::new(from, sound, 75.0, true));
    crate::projectile::fire(
        world,
        crate::projectile::Shot {
            data: &data,
            firer: e,
            from,
            target: Some(target),
            target_pos: to,
            zone: BodyZone::Chest,
            hit_prone: true,
            pixel: [0, 0],
            deviation: 0.0,
            fired_from: Some(e),
        },
    );
    true
}

// ---------------------------------------------------------------------------------------------
// Clicks

fn plain_name(world: &World, e: Entity) -> String {
    world.get::<DmName>(e).map_or_else(String::new, |n| {
        crate::vending::plain_name(&n.name).to_owned()
    })
}

/// tg `attack_hand` -> `ui_interact` of a turret or a control panel with an empty hand. True
/// when the click was theirs.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    open(world, client, mob, target)
}

/// tg `attack_ai` of a turret or a control panel (`crate::silicon::interact`).
pub fn attack_ai(world: &mut World, client: Entity, user: Entity, target: Entity) -> bool {
    open(world, client, user, target)
}

fn open(world: &mut World, client: Entity, user: Entity, target: Entity) -> bool {
    if world.get::<Unpowered>(target).is_some()
        && (world.get::<Turret>(target).is_some() || world.get::<TurretControl>(target).is_some())
    {
        return true;
    }
    if world.get::<Turret>(target).is_some() {
        let title = plain_name(world, target);
        world.write_message(OpenUi::new::<TurretWindow>(client, user, target, title));
        return true;
    }
    if let Some(panel) = world.get::<TurretControl>(target) {
        // `is_ai_locked`.
        if panel.ailock && has_silicon_access(world, user) {
            tell(
                world,
                client,
                Span::Warning,
                "Похоже, брандмауэр не даёт вам получить доступ к этому устройству!",
            );
            return true;
        }
        let title = plain_name(world, target);
        world.write_message(OpenUi::new::<TurretControlWindow>(
            client, user, target, title,
        ));
        return true;
    }
    false
}

/// tg `item_interaction` of turrets and control panels (an ID toggles the lock, an emag), and of
/// cameras and camera assemblies ([`super::cameras`]). True when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    modifiers: ss13_shared::protocol::ClickModifiers,
) -> bool {
    if super::cameras::item_interaction(world, client, mob, held, target, modifiers) {
        return true;
    }
    let turret = world.get::<Turret>(target).is_some();
    let panel = world.get::<TurretControl>(target).is_some();
    if !turret && !panel {
        return false;
    }
    if crate::emag::is_emag(world, held) {
        if crate::emag::can_emag(world, mob, held, target) {
            emag_act(world, client, target);
        }
        return true;
    }
    let Some(card) = crate::inventory::get_id(world, held) else {
        return false;
    };
    let access = world
        .get::<IdCard>(card)
        .map(|c| c.access.clone())
        .unwrap_or_default();
    if turret {
        let req = world
            .get::<DmTurret>(target)
            .map(|d| d.req_access.clone())
            .unwrap_or_default();
        if !check_access(&req, &[], &access) {
            tell(world, client, Span::Warning, "Доступ запрещён.");
            return true;
        }
        let Some(mut t) = world.get_mut::<Turret>(target) else {
            return true;
        };
        t.locked = !t.locked;
        let text = if t.locked {
            "Управление заблокировано."
        } else {
            "Управление разблокировано."
        };
        tell(world, client, Span::Notice, text);
        return true;
    }
    let Some(p) = world.get::<TurretControl>(target).cloned() else {
        return true;
    };
    if !check_access(&p.req_access, &[], &access) {
        tell(world, client, Span::Warning, "Доступ запрещён.");
        return true;
    }
    if p.emagged {
        tell(
            world,
            client,
            Span::Warning,
            "Пульт управления турелями не отвечает!",
        );
        return true;
    }
    let locked = !p.locked;
    if let Some(mut p) = world.get_mut::<TurretControl>(target) {
        p.locked = locked;
    }
    let text = if locked {
        "Вы блокируете панель."
    } else {
        "Вы разблокируете панель."
    };
    tell(world, client, Span::Notice, text);
    true
}

/// `porta_turret/emag_act`, `turretid/emag_act`.
fn emag_act(world: &mut World, client: Entity, target: Entity) {
    if let Some(mut t) = world.get_mut::<Turret>(target) {
        if t.emagged {
            return;
        }
        t.emagged = true;
        t.controllock = true;
        tell(
            world,
            client,
            Span::Notice,
            "Цепи оценки угрозы закорочены.",
        );
        set_disabled(world, target, DISABLE_SECS);
        return;
    }
    if let Some(mut p) = world.get_mut::<TurretControl>(target) {
        if p.emagged {
            return;
        }
        p.emagged = true;
        p.locked = false;
        tell(
            world,
            client,
            Span::Notice,
            "Модуль анализа доступа закорочен.",
        );
    }
}

/// `porta_turret/emp_act` (while on: flags scrambled, off for 6-20 s) and `turretid/emp_act`
/// (at `75 / severity` percent: lock, power, mode and cyborgs at random).
fn emp_turrets(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        if !ev.hits_self() {
            continue;
        }
        let e = ev.target;
        if world.get::<Turret>(e).is_some_and(|t| t.on) {
            let mut rng = world.resource_mut::<EmpRng>();
            let mut add = 0;
            if rng.prob(50.0) {
                add |= TURRET_FLAG_SHOOT_CRIMINALS;
            }
            if rng.prob(50.0) {
                add |= TURRET_FLAG_AUTH_WEAPONS;
            }
            if rng.prob(20.0) {
                add |= TURRET_FLAG_SHOOT_ALL;
            }
            let secs = rng.range(60, 200) as f64 / 10.0;
            if let Some(mut t) = world.get_mut::<Turret>(e) {
                t.flags |= add;
            }
            set_disabled(world, e, secs);
            continue;
        }
        if world.get::<TurretControl>(e).is_none() {
            continue;
        }
        let mut rng = world.resource_mut::<EmpRng>();
        if !rng.prob(75.0 / f64::from(ev.severity)) {
            continue;
        }
        let picks = [0; 4].map(|_| rng.prob(50.0));
        if let Some(mut p) = world.get_mut::<TurretControl>(e) {
            [p.locked, p.enabled, p.lethal, p.shoot_cyborgs] = picks;
        }
        update_turrets(world, e);
    }
}

/// `updateTurrets`: every linked turret follows the panel.
pub fn update_turrets(world: &mut World, panel: Entity) {
    let Some(p) = world.get::<TurretControl>(panel).cloned() else {
        return;
    };
    let alive: Vec<Entity> = p
        .turrets
        .iter()
        .copied()
        .filter(|&t| world.get::<Turret>(t).is_some())
        .collect();
    for &t in &alive {
        set_state(world, t, p.enabled, p.lethal, p.shoot_cyborgs);
    }
    if let Some(mut p) = world.get_mut::<TurretControl>(panel) {
        p.turrets = alive;
    }
    refresh_panel_look(world, panel);
}

// ---------------------------------------------------------------------------------------------
// Windows

pub struct TurretWindow;

impl UiInterface for TurretWindow {
    const NAME: &'static str = PortableTurretUi::NAME;
    type Data = PortableTurretData;
    type StaticData = ();
}

impl ServerUi for TurretWindow {
    type Source = (&'static Turret, &'static DmTurret);
    type UserParam = SiliconUsers<'static, 'static>;
    type Act = String;

    fn static_data(_: (&Turret, &DmTurret)) {}

    fn data(
        (t, dm): (&Turret, &DmTurret),
        user: Entity,
        silicons: &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> PortableTurretData {
        let f = |bit: u32| t.flags & bit != 0;
        PortableTurretData {
            locked: t.locked,
            on: t.on,
            check_weapons: f(TURRET_FLAG_AUTH_WEAPONS),
            neutralize_criminals: f(TURRET_FLAG_SHOOT_CRIMINALS),
            neutralize_all: f(TURRET_FLAG_SHOOT_ALL),
            neutralize_unidentified: f(TURRET_FLAG_SHOOT_ANOMALOUS),
            neutralize_nonmindshielded: f(TURRET_FLAG_SHOOT_UNSHIELDED),
            neutralize_cyborgs: f(TURRET_FLAG_SHOOT_BORGS),
            neutralize_heads: f(TURRET_FLAG_SHOOT_HEADS),
            silicon_user: silicons.contains(user),
            lasertag_turret: !dm.team_color.is_empty(),
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<String, UiReject> {
        if action == POWER || act_flag(action).is_some() {
            Ok(action.to_owned())
        } else {
            Err(UiReject::UnknownAction)
        }
    }
}

/// tg `porta_turret/ui_act`, at the moment of effect: unlocked (or a silicon), powered.
fn turret_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<TurretWindow>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<TurretWindow>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (_client, actor, e, act) in pending {
        let Some(t) = world.get::<Turret>(e) else {
            continue;
        };
        if (t.locked && !has_silicon_access(world, actor)) || world.get::<Unpowered>(e).is_some() {
            continue;
        }
        let lasertag = world
            .get::<DmTurret>(e)
            .is_some_and(|d| !d.team_color.is_empty());
        if act == POWER {
            let on = !t.on;
            toggle_on(world, e, on);
        } else if let Some(bit) = act_flag(&act).filter(|_| !lasertag)
            && let Some(mut t) = world.get_mut::<Turret>(e)
        {
            t.flags ^= bit;
        }
    }
}

pub struct TurretControlWindow;

impl UiInterface for TurretControlWindow {
    const NAME: &'static str = TurretControlUi::NAME;
    type Data = TurretControlData;
    type StaticData = ();
}

impl ServerUi for TurretControlWindow {
    type Source = &'static TurretControl;
    type UserParam = SiliconUsers<'static, 'static>;
    type Act = String;

    fn static_data(_: &TurretControl) {}

    fn data(
        p: &TurretControl,
        user: Entity,
        silicons: &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> TurretControlData {
        TurretControlData {
            locked: p.locked,
            silicon_user: silicons.contains(user),
            enabled: p.enabled,
            lethal: p.lethal,
            shoot_cyborgs: p.shoot_cyborgs,
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<String, UiReject> {
        match action {
            LOCK | POWER | MODE | SHOOT_SILICONS => Ok(action.to_owned()),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// tg `turretid/ui_act`, at the moment of effect: not AI-locked for silicons; the lock only by
/// silicons (not emagged); the rest unlocked or by a silicon.
fn panel_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<TurretControlWindow>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<TurretControlWindow>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, e, act) in pending {
        let Some(p) = world.get::<TurretControl>(e).cloned() else {
            continue;
        };
        let silicon = has_silicon_access(world, actor);
        if silicon && p.ailock {
            continue;
        }
        if act == LOCK {
            if !silicon {
                continue;
            }
            if p.emagged {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Пульт управления турелями не отвечает!",
                );
                continue;
            }
            if let Some(mut p) = world.get_mut::<TurretControl>(e) {
                p.locked = !p.locked;
            }
            continue;
        }
        if p.locked && !silicon {
            continue;
        }
        let text = {
            let Some(mut p) = world.get_mut::<TurretControl>(e) else {
                continue;
            };
            match act.as_str() {
                POWER => {
                    p.enabled = !p.enabled;
                    if p.enabled {
                        "включено"
                    } else {
                        "выключено"
                    }
                }
                MODE => {
                    p.lethal = !p.lethal;
                    if p.lethal {
                        "предохранители отключены"
                    } else {
                        "предохранители включены"
                    }
                }
                _ => {
                    p.shoot_cyborgs = !p.shoot_cyborgs;
                    if p.shoot_cyborgs {
                        "стрельба по киборгам"
                    } else {
                        "без стрельбы по киборгам"
                    }
                }
            }
        };
        tell(world, client, Span::Notice, text);
        update_turrets(world, e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dm(path: &str, flags: u32) -> DmTurret {
        DmTurret {
            path: path.to_owned(),
            scan_range: 7.0,
            shot_delay: 15.0,
            turret_flags: flags,
            req_access: vec!["security".to_owned()],
            base_icon_state: "standard".to_owned(),
            ..Default::default()
        }
    }

    fn turret(flags: u32) -> Turret {
        Turret {
            on: true,
            lethal: false,
            raised: false,
            raising: None,
            locked: true,
            controllock: false,
            emagged: false,
            flags,
            last_fired: f64::NEG_INFINITY,
            disabled_until: 0.0,
            restart_at: None,
            next_process: 0.0,
        }
    }

    /// tg `assess_perp`: the type overrides, emag, anger at non-security, mindshields.
    #[test]
    fn threat_assessment_follows_flags_and_type() {
        let mut world = World::new();
        let perp = world.spawn(Name::new("Nobody")).id();
        let base = "/obj/machinery/porta_turret";
        let none = turret(0);
        assert_eq!(assess_perp(&world, &none, &dm(base, 0), perp), 0);
        let ai = dm("/obj/machinery/porta_turret/ai", 0);
        assert_eq!(assess_perp(&world, &none, &ai, perp), 10);
        let centcom = dm("/obj/machinery/porta_turret/centcom_shuttle", 0);
        assert_eq!(assess_perp(&world, &none, &centcom, perp), 0);
        let mut emagged = turret(0);
        emagged.emagged = true;
        assert_eq!(assess_perp(&world, &emagged, &dm(base, 0), perp), 10);
        let angry = turret(TURRET_FLAG_SHOOT_ALL_REACT);
        assert_eq!(assess_perp(&world, &angry, &dm(base, 0), perp), 10);
        let unshielded = turret(TURRET_FLAG_SHOOT_UNSHIELDED);
        assert_eq!(assess_perp(&world, &unshielded, &dm(base, 0), perp), 4);
        world
            .entity_mut(perp)
            .insert(crate::antag::revolution::server::Mindshield);
        assert_eq!(assess_perp(&world, &unshielded, &dm(base, 0), perp), 0);
        // No crew records at all: everyone is unknown to a criminal-checking turret.
        let criminals = turret(TURRET_FLAG_SHOOT_CRIMINALS);
        assert_eq!(assess_perp(&world, &criminals, &dm(base, 0), perp), 4);
    }

    #[test]
    fn panels_set_their_turrets_unless_emagged() {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        let t = world
            .spawn((turret(0), dm("/obj/machinery/porta_turret", 0)))
            .id();
        set_state(&mut world, t, true, true, true);
        let state = world.get::<Turret>(t).unwrap();
        assert!(state.lethal && state.flags & TURRET_FLAG_SHOOT_BORGS != 0);
        set_state(&mut world, t, false, false, false);
        assert!(!world.get::<Turret>(t).unwrap().on);
        world.get_mut::<Turret>(t).unwrap().controllock = true;
        set_state(&mut world, t, true, true, true);
        assert!(!world.get::<Turret>(t).unwrap().on);
    }

    #[test]
    fn a_disabled_turret_stays_off_until_its_time() {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        let t = world
            .spawn((turret(0), dm("/obj/machinery/porta_turret", 0)))
            .id();
        set_disabled(&mut world, t, 6.0);
        let state = world.get::<Turret>(t).unwrap();
        assert!(!state.on && state.restart_at == Some(6.0));
        toggle_on(&mut world, t, true);
        assert!(
            !world.get::<Turret>(t).unwrap().on,
            "refused while disabled"
        );
    }

    #[test]
    fn factions_by_type() {
        assert_eq!(faction_of("/obj/machinery/porta_turret/ai"), vec!["turret"]);
        assert_eq!(
            faction_of("/obj/machinery/porta_turret/syndicate/energy"),
            vec!["Syndicate"]
        );
        assert_eq!(
            faction_of("/obj/machinery/porta_turret/centcom_shuttle/weak").len(),
            3
        );
    }

    #[test]
    fn facing_toward_targets() {
        let from = TilePos::new(5, 5, 1);
        assert_eq!(
            crate::pull::dir_between(from, TilePos::new(5, 9, 1)),
            Some(ss13_shared::Dir::North)
        );
    }
}
