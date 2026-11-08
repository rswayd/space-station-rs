//! Door remotes (server), after tg `/obj/item/door_remote` (`code/game/objects/items/tools/
//! control_wand.dm`).
//!
//! - Used in hand, the remote steps through its modes: open, bolts, emergency access (emagged
//!   also: shock, depower), with the balloon and the switch sound.
//! - Clicked at a door (from any distance it sees, or at anything on the door's tile), it works
//!   that airlock or windoor when its access fits the door (`check_access_list`), the ID scanner
//!   is on, the door is outside the restricted areas (bridge, security, vault, AI chamber and
//!   upload) unless that area is the remote's domain, and the door has power and the AI wire.
//! - The access is the remote's region plus its owner job's access, wildcards included
//!   (BandaStation fix), worked out on first use.
// shortcut: the access is read on first use, not at `LateInitialize`; no `log_combat`.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{Areas, DmItem, DoorKind};
use ss13_shared::TilePos;
use ss13_world::SpatialIndex;

use crate::click::RangedClick;
use crate::doors::{Door, DoorState, with_door};
use crate::inventory::Inventory;
use crate::sound::{PlaySound, playsound};
use crate::wires::{Electrified, MACHINE_DEFAULT_ELECTRIFY_TIME};

pub const DOOR_REMOTE: &str = "/obj/item/door_remote";
const MODE_SWITCH_SOUND: &str = "remote_mode_switch";
const ACTION_SOUND: &str = "remote_action";
/// `COOLDOWN_START(src, shock_cooldown, 10 SECONDS)`.
const SHOCK_COOLDOWN_SECS: f64 = 10.0;

/// `restricted_areas`.
const RESTRICTED_AREAS: [&str; 5] = [
    "/area/station/command/bridge",
    "/area/station/security",
    "/area/station/command/vault",
    "/area/station/ai/satellite/chamber",
    "/area/station/ai/upload",
];

/// One remote type: `department` (its icon), `region_access` (`None`: `REGION_ALL_STATION`),
/// `owner_trim` and `our_domain`.
struct RemoteKind {
    path: &'static str,
    department: &'static str,
    region: Option<&'static str>,
    owner_trim: Option<&'static str>,
    domain: &'static [&'static str],
}

/// tg's remote subtypes; the root is the civilian one with no trim or domain.
const KINDS: [RemoteKind; 9] = [
    RemoteKind {
        path: "/obj/item/door_remote/omni",
        department: "omni",
        region: None,
        owner_trim: None,
        domain: &["/area/station"],
    },
    RemoteKind {
        path: "/obj/item/door_remote/captain",
        department: "command",
        region: Some(crate::id_access::REGION_COMMAND),
        owner_trim: None,
        domain: &["/area/station"],
    },
    RemoteKind {
        path: "/obj/item/door_remote/chief_engineer",
        department: "engi",
        region: Some("Engineering"),
        owner_trim: Some("/datum/id_trim/job/chief_engineer"),
        domain: &[],
    },
    RemoteKind {
        path: "/obj/item/door_remote/research_director",
        department: "sci",
        region: Some("Research"),
        owner_trim: Some("/datum/id_trim/job/research_director"),
        domain: &[
            "/area/station/ai/satellite/chamber",
            "/area/station/ai/upload",
        ],
    },
    RemoteKind {
        path: "/obj/item/door_remote/head_of_security",
        department: "security",
        region: Some("Security"),
        owner_trim: Some("/datum/id_trim/job/head_of_security"),
        domain: &["/area/station/security"],
    },
    RemoteKind {
        path: "/obj/item/door_remote/quartermaster",
        department: "cargo",
        region: Some("Supply"),
        owner_trim: Some("/datum/id_trim/job/quartermaster"),
        domain: &["/area/station/command/vault"],
    },
    RemoteKind {
        path: "/obj/item/door_remote/chief_medical_officer",
        department: "med",
        region: Some("Medbay"),
        owner_trim: Some("/datum/id_trim/job/chief_medical_officer"),
        domain: &[],
    },
    RemoteKind {
        path: "/obj/item/door_remote/head_of_personnel",
        department: "civilian",
        region: Some(crate::id_access::REGION_GENERAL),
        owner_trim: Some("/datum/id_trim/job/head_of_personnel"),
        domain: &[],
    },
    RemoteKind {
        path: DOOR_REMOTE,
        department: "civilian",
        region: Some(crate::id_access::REGION_GENERAL),
        owner_trim: None,
        domain: &[],
    },
];

fn kind_of(path: &str) -> Option<&'static RemoteKind> {
    KINDS.iter().find(|k| is_under(path, k.path))
}

/// `WAND_*`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Open,
    Bolt,
    Emergency,
    Shock,
    Depower,
}

impl Mode {
    /// `attack_self`: the next mode; shock and depower only when emagged.
    fn next(self, emagged: bool) -> Self {
        match self {
            Self::Open => Self::Bolt,
            Self::Bolt => Self::Emergency,
            Self::Emergency if emagged => Self::Shock,
            Self::Emergency | Self::Depower => Self::Open,
            Self::Shock => Self::Depower,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Open => "открытие двери",
            Self::Bolt => "болты",
            Self::Emergency => "аварийный доступ",
            Self::Shock => "электризация",
            Self::Depower => "обесточивание",
        }
    }
}

/// A remote's live state: `mode`, `obj_flags & EMAGGED`, `shock_cooldown`, `access_list`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct DoorRemote {
    pub mode: Mode,
    pub emagged: bool,
    pub shock_ready_at: f64,
    pub access: Vec<String>,
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    app.add_click_handler(ClickPhase::AttackSelf, 1773, |w, c| {
        attack_self(w, c.mob, c.target)
    });
    // `interact_with_atom`: next to the door or its turf, as from afar.
    app.add_click_handler(ClickPhase::HeldOnTarget, 1593, |w, c| {
        let Some(held) = c.held.filter(|&h| is_remote(w, h)) else {
            return false;
        };
        if w.get::<Door>(c.target).is_none() && w.get::<ss13_mapload::Turf>(c.target).is_none() {
            return false;
        }
        use_on(w, c.mob, held, c.target);
        true
    });
    app.add_systems(Update, ranged_clicks);
}

pub fn is_remote(world: &World, item: Entity) -> bool {
    world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, DOOR_REMOTE))
}

/// The remote's state, made on first use (`LateInitialize`: region access plus the owner
/// trim's access and wildcards).
fn state(world: &mut World, item: Entity) -> Option<DoorRemote> {
    if let Some(remote) = world.get::<DoorRemote>(item) {
        return Some(remote.clone());
    }
    let kind = kind_of(&world.get::<DmItem>(item)?.path)?;
    let mut access = match kind.region {
        Some(region) => crate::id_access::region_access_list(&[region]),
        None => crate::id_access::all_station_access()
            .into_iter()
            .map(str::to_owned)
            .collect(),
    };
    if let Some(trim) = kind.owner_trim
        && let Some(jobs) = world.get_resource::<crate::jobs::Jobs>()
        && let Some(job) = jobs
            .data
            .jobs
            .iter()
            .find(|j| j.trim.as_deref() == Some(trim))
    {
        for a in &job.access {
            if !access.contains(a) {
                access.push(a.clone());
            }
        }
    }
    let remote = DoorRemote {
        access,
        ..default()
    };
    world.entity_mut(item).insert(remote.clone());
    Some(remote)
}

fn save(world: &mut World, item: Entity, remote: DoorRemote) {
    world.entity_mut(item).insert(remote);
    update_icon(world, item);
}

/// `update_icon_state`: `remote_<department>_<mode>`; an emagged one shows emergency.
fn update_icon(world: &mut World, item: Entity) {
    let Some(kind) = world.get::<DmItem>(item).and_then(|i| kind_of(&i.path)) else {
        return;
    };
    let Some(remote) = world.get::<DoorRemote>(item) else {
        return;
    };
    let mode = match remote.mode {
        _ if remote.emagged => "emergency",
        Mode::Open => "open",
        Mode::Bolt => "bolt",
        Mode::Emergency => "emergency",
        Mode::Shock | Mode::Depower => return,
    };
    let state = format!("remote_{}_{mode}", kind.department);
    crate::obj_damage::set_icon_state(world, item, &state);
}

fn sound_at(world: &mut World, source: Entity, sound: &str) {
    if let Some(pos) =
        crate::balloon::atom_on_turf(world, source).and_then(|e| world.get::<TilePos>(e).copied())
    {
        playsound(world, PlaySound::new(pos, sound, 50.0, true));
    }
}

/// `attack_self`: the next mode.
fn attack_self(world: &mut World, mob: Entity, item: Entity) -> bool {
    if !is_remote(world, item) {
        return false;
    }
    let Some(mut remote) = state(world, item) else {
        return false;
    };
    remote.mode = remote.mode.next(remote.emagged);
    let label = remote.mode.label();
    save(world, item, remote);
    crate::balloon::balloon_alert(world, mob, item, &format!("режим: {label}"));
    sound_at(world, item, MODE_SWITCH_SOUND);
    true
}

/// `emag_act`: shock and depower unlocked. False when already emagged.
pub fn emag_act(world: &mut World, mob: Entity, item: Entity) -> bool {
    let Some(mut remote) = state(world, item) else {
        return false;
    };
    if remote.emagged {
        return false;
    }
    remote.emagged = true;
    save(world, item, remote);
    crate::balloon::balloon_alert(world, mob, item, "ограничения сняты");
    true
}

/// A click past reach with a remote in the active hand (`ranged_interact_with_atom`).
fn ranged_clicks(world: &mut World, mut cursor: Local<MessageCursor<RangedClick>>) {
    let clicks: Vec<RangedClick> = cursor
        .read(world.resource::<Messages<RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(held) = world
            .get::<Inventory>(click.mob)
            .and_then(Inventory::active_item)
            .filter(|&h| is_remote(world, h))
        else {
            continue;
        };
        if world.get_entity(click.target).is_err() {
            continue;
        }
        use_on(world, click.mob, held, click.target);
    }
}

/// Why the remote cannot work `door` now (`ranged_interact_with_atom`'s refusals).
fn refusal(
    world: &World,
    remote: &DoorRemote,
    kind: &RemoteKind,
    door: Entity,
) -> Option<&'static str> {
    let d = world.get::<Door>(door)?;
    let wires = world.get::<crate::wires::Wires>(door);
    if !d.allows(Some(&remote.access)) || !d.requires_id(wires) {
        return Some("нет доступа!");
    }
    let area = world
        .get::<TilePos>(door)
        .and_then(|&p| world.resource::<Areas>().area_at(p))
        .and_then(|a| crate::antag::nukeop::bomb::area_path(world, a));
    if let Some(area) = area
        && RESTRICTED_AREAS.iter().any(|r| is_under(area, r))
        && !kind.domain.iter().any(|r| is_under(area, r))
    {
        return Some("нет доступа!");
    }
    let powered = world.get::<crate::power::Unpowered>(door).is_none();
    let airlock = d.kind == DoorKind::Airlock;
    if !d.has_power(powered) || (airlock && !crate::silicon::ai::can_ai_control(world, door)) {
        return Some(if remote.mode == Mode::Open {
            "не поддаётся!"
        } else {
            "ничего не происходит!"
        });
    }
    if !airlock && remote.mode != Mode::Open {
        return Some("только шлюзы!");
    }
    None
}

/// `ranged_interact_with_atom` on `target`: the door clicked, or the first door on its tile
/// that takes remotes (airlocks, windoors).
pub fn use_on(world: &mut World, mob: Entity, item: Entity, target: Entity) {
    sound_at(world, item, ACTION_SOUND);
    let takes_remote = |world: &World, e: Entity| {
        world
            .get::<Door>(e)
            .is_some_and(|d| matches!(d.kind, DoorKind::Airlock | DoorKind::Windoor))
    };
    let door = if world.get::<Door>(target).is_some() {
        Some(target).filter(|&t| takes_remote(world, t))
    } else {
        world.get::<TilePos>(target).copied().and_then(|pos| {
            world
                .resource::<SpatialIndex>()
                .on_tile(pos)
                .iter()
                .copied()
                .find(|&e| takes_remote(world, e))
        })
    };
    let Some(door) = door else {
        return;
    };
    let Some(mut remote) = state(world, item) else {
        return;
    };
    let Some(kind) = world.get::<DmItem>(item).and_then(|i| kind_of(&i.path)) else {
        return;
    };
    if let Some(text) = refusal(world, &remote, kind, door) {
        crate::balloon::balloon_alert(world, mob, target, text);
        return;
    }
    match remote.mode {
        Mode::Open => {
            with_door(world, door, |d, appearance, ctx| match d.state {
                DoorState::Closed => ((), ctx.open(d, appearance, false)),
                DoorState::Open => ((), ctx.close(d, appearance, false)),
                _ => ((), false),
            });
        }
        Mode::Bolt => {
            let locked = world.get::<Door>(door).is_some_and(|d| d.locked);
            crate::wires::set_bolts(world, door, !locked);
            info!(target: "game", "{mob:?} {} airlock {door:?} with a door remote", if locked { "unbolted" } else { "bolted" });
        }
        Mode::Emergency => {
            with_door(world, door, |d, appearance, _| {
                d.emergency = !d.emergency;
                d.apply_look(appearance);
                ((), false)
            });
        }
        Mode::Shock => {
            let now = world.resource::<Time>().elapsed_secs_f64();
            if now < remote.shock_ready_at {
                crate::balloon::balloon_alert(world, mob, target, "импульс перезаряжается!");
                return;
            }
            if world.get::<Electrified>(door).is_some() {
                crate::balloon::balloon_alert(world, mob, target, "уже под напряжением!");
            } else {
                crate::wires::set_electrified(
                    world,
                    door,
                    Some(Electrified::Timed(MACHINE_DEFAULT_ELECTRIFY_TIME)),
                );
                remote.shock_ready_at = now + SHOCK_COOLDOWN_SECS;
                save(world, item, remote);
            }
        }
        // First hit cuts main power, the next once backup came back cuts backup too.
        Mode::Depower => {
            let Some((main_on, backup_on)) = world
                .get::<Door>(door)
                .map(|d| (d.main_outage <= 0.0, d.backup_outage <= 0.0))
            else {
                return;
            };
            if main_on {
                crate::wires::lose_power(world, door, true);
            } else if backup_on {
                crate::wires::lose_power(world, door, false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_cycle_and_emag_adds_two() {
        let mut m = Mode::Open;
        let mut seen = vec![m];
        for _ in 0..3 {
            m = m.next(false);
            seen.push(m);
        }
        assert_eq!(seen, [Mode::Open, Mode::Bolt, Mode::Emergency, Mode::Open]);
        assert_eq!(Mode::Emergency.next(true), Mode::Shock);
        assert_eq!(Mode::Shock.next(true), Mode::Depower);
        assert_eq!(Mode::Depower.next(true), Mode::Open);
    }

    #[test]
    fn subtypes_find_their_kind() {
        assert_eq!(
            kind_of("/obj/item/door_remote/chief_engineer").map(|k| k.department),
            Some("engi")
        );
        assert_eq!(kind_of(DOOR_REMOTE).map(|k| k.department), Some("civilian"));
        assert!(
            kind_of("/obj/item/door_remote/omni")
                .unwrap()
                .region
                .is_none()
        );
    }
}
