//! The security and medical data HUDs (server) after tg `code/game/data_huds.dm`: a mob wearing
//! security HUD glasses (`TRAIT_SECURITY_HUD`) sees over every human it sees the job icon of
//! the worn ID (`update_ID_card`) and the wanted status of its crew record
//! (`sec_hud_set_security_status`); medical HUD glasses (`TRAIT_MEDICAL_HUD`) show the health
//! bar (`med_hud_set_health`, `RoundHealth`) and the status (`med_hud_set_status`). The images
//! go to the wearer only, as the headless `DataHud` interface keyed by its net ids.
//!
//! Silicon sensors (`silicon_huds`, toggled by `toggle_sensors`, [`SensorsOff`]): a cyborg has
//! both full HUDs; the AI, through its eye, the lesser ones: only the job icon, and health only
//! over humans whose suit sensors are on tracking (`TRAIT_BASIC_HEALTH_HUD_VISIBLE`).
//!
//! Cost: every [`REFRESH_SECS`], per wearer, one pass over the humans (an interest lookup
//! each): wearers × humans; a few wearers among 100 players is a few hundred lookups per
//! refresh.
// shortcut: glasses give their HUD by type (`/obj/item/clothing/glasses/hud/security`,
// `.../health`, `.../medsechud` and subtypes), not by `clothing_traits` (no toggling HUD
// glasses; HUD implants come from `crate::medical::cyberimps`); no second implant slot offset;
// refreshed every 0.5 s rather than on each change. The record is the one of [`perp_name`]
// (tg `get_face_name(get_id_name(""))`).

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_net::server::Interest;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiSystems};

use super::manifest::Manifest;
use super::*;
use crate::health::{Health, Stat};
use crate::human::HumanLook;
use crate::inventory::Inventory;
use crate::server::PlayerMob;

/// `DEFAULT_HUDS_DMI`.
pub const HUDS_DMI: &str = "icons/mob/huds/hud.dmi";
const REFRESH_SECS: f64 = 0.5;

const SECURITY_GLASSES: [&str; 2] = [
    "/obj/item/clothing/glasses/hud/security",
    "/obj/item/clothing/glasses/hud/medsechud",
];
const MEDICAL_GLASSES: [&str; 2] = [
    "/obj/item/clothing/glasses/hud/health",
    "/obj/item/clothing/glasses/hud/medsechud",
];

pub struct DataHudPlugin;

impl Plugin for DataHudPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<DataHudUi>()
            .add_systems(Update, refresh.before(UiSystems::Open));
    }
}

impl ServerUi for DataHudUi {
    type Source = &'static DataHudData;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &DataHudData) {}

    fn data(source: &DataHudData, _: Entity, _: &()) -> DataHudData {
        source.clone()
    }
}

/// `sec_hud_set_security_status`: the wanted icon of a status.
pub fn wanted_state(status: &str) -> Option<&'static str> {
    Some(match status {
        WANTED_ARREST => "hudwanted",
        WANTED_PRISONER => "hudincarcerated",
        WANTED_SUSPECT => "hudsuspected",
        WANTED_PAROLE => "hudparolled",
        WANTED_DISCHARGED => "huddischarged",
        _ => return None,
    })
}

/// `RoundHealth(M)` (`hud` + it is the health bar's state).
pub fn round_health(health: f32, max: f32, dead: bool) -> &'static str {
    if dead {
        return "health-100";
    }
    // Carbons in crit read against 100 (`maxi_health = 100`).
    let max = if health < 0.0 { 100.0 } else { max };
    let h = health / max * 100.0;
    const STEPS: [(f32, &str); 16] = [
        (100.0, "health100"),
        (90.625, "health93.75"),
        (84.375, "health87.5"),
        (78.125, "health81.25"),
        (71.875, "health75"),
        (65.625, "health68.75"),
        (59.375, "health62.5"),
        (53.125, "health56.25"),
        (46.875, "health50"),
        (40.625, "health43.75"),
        (34.375, "health37.5"),
        (28.125, "health31.25"),
        (21.875, "health25"),
        (15.625, "health18.75"),
        (9.375, "health12.5"),
        (1.0, "health6.25"),
    ];
    if let Some((_, s)) = STEPS.iter().find(|(min, _)| h >= *min) {
        return s;
    }
    if h >= -50.0 {
        "health0"
    } else if h >= -85.0 {
        "health-50"
    } else if h >= -99.0 {
        "health-85"
    } else {
        "health-100"
    }
}

/// The name the HUDs read a human by (tg `get_face_name(get_id_name(""))`): the real name, or
/// with the face hidden the worn ID's name; `None` when neither shows.
pub fn perp_name(world: &World, mob: Entity) -> Option<String> {
    match crate::clothing::cover::hidden_face_name(world, mob) {
        None => world.get::<Name>(mob).map(|n| n.to_string()),
        Some(n) if n == crate::clothing::cover::UNKNOWN_NAME => None,
        Some(n) => Some(n),
    }
}

/// On an AI core or a cyborg whose sensor HUDs are switched off (tg `sensors_on = FALSE`,
/// `toggle_sensors`); they start on (`add_sensors` in `Initialize`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SensorsOff;

/// A cyborg's `silicon_huds` with its sensors on: the full security and medical HUDs.
fn borg_sensors(world: &World, mob: Entity) -> bool {
    world.get::<crate::silicon::cyborg::Cyborg>(mob).is_some()
        && world.get::<SensorsOff>(mob).is_none()
}

/// The AI's `silicon_huds` with its sensors on, seen from its eye: the lesser HUDs
/// (`TRAIT_MEDICAL_HUD_SENSOR_ONLY`, `TRAIT_SECURITY_HUD_ID_ONLY`).
pub fn ai_sensors(world: &World, mob: Entity) -> bool {
    world
        .get::<crate::silicon::ai::AiEye>(mob)
        .is_some_and(|eye| world.get::<SensorsOff>(eye.ai).is_none())
}

/// `TRAIT_BASIC_HEALTH_HUD_VISIBLE` (`update_wearer_status`): worn suit sensors past
/// `SENSOR_VITALS`, what the lesser medical HUD shows.
pub fn basic_health_visible(world: &World, inventory: Option<&Inventory>) -> bool {
    use crate::clothing::sensors::{NO_SENSORS, SENSOR_VITALS, SuitSensor};
    inventory
        .and_then(|i| i.uniform)
        .and_then(|u| world.get::<SuitSensor>(u))
        .is_some_and(|s| s.has > NO_SENSORS && s.mode > SENSOR_VITALS)
}

/// The data HUDs `mob` has (`TRAIT_SECURITY_HUD`, `TRAIT_MEDICAL_HUD`): (security, medical).
pub fn wearer_huds(world: &World, mob: Entity) -> (bool, bool) {
    if borg_sensors(world, mob) {
        return (true, true);
    }
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return (false, false);
    };
    let (sec, med) = inventory
        .eyes
        .and_then(|e| world.get::<DmItem>(e))
        .map_or((false, false), |i| glasses_huds(&i.path));
    let (implant_sec, implant_med) = crate::medical::cyberimps::implant_huds(world, mob);
    // MOD security and medical visors (`crate::modsuit::modules::VisorHud`).
    let (visor_sec, visor_med) = crate::modsuit::modules::visor_huds(world, mob);
    // The ablative hood's `TRAIT_SECURITY_HUD` (`crate::clothing::ablative`).
    let hood_sec = crate::clothing::ablative::hood_hud(world, inventory.head);
    (
        sec || implant_sec || visor_sec || hood_sec,
        med || implant_med || visor_med,
    )
}

/// Which data HUDs the glasses of type `path` give: (security, medical).
pub fn glasses_huds(path: &str) -> (bool, bool) {
    (
        SECURITY_GLASSES.iter().any(|r| is_under(path, r)),
        MEDICAL_GLASSES.iter().any(|r| is_under(path, r)),
    )
}

/// tg `med_hud_set_status` of a human: dead (`huddefib` while a defibrillator could bring
/// back a player), the worst disease a scanner sees (`check_virus`), broken suit sensors,
/// else healthy.
// shortcut: no xenomorph hosts, ghost-role revival or disease-like traits (none ported).
pub fn med_status(
    world: &World,
    mob: Entity,
    inventory: Option<&Inventory>,
    dead: bool,
    ghost_bodies: &[Entity],
) -> &'static str {
    if dead {
        // `can_defib_client`: a player in the body or its ghost.
        let player = world.get::<PlayerMob>(mob).is_some() || ghost_bodies.contains(&mob);
        return if player && crate::medical::defib::revivable(world, mob).is_ok() {
            "huddefib"
        } else {
            "huddead"
        };
    }
    if let Some(state) = virus_state(crate::medical::diseases::of_mob(world, mob)) {
        return state;
    }
    let broken = inventory
        .and_then(|i| i.uniform)
        .and_then(|u| world.get::<crate::clothing::sensors::SuitSensor>(u))
        .is_some_and(|s| s.has == crate::clothing::sensors::BROKEN_SENSORS);
    if broken { "hudnosensor" } else { "hudhealthy" }
}

/// `check_virus`: the icon of the most severe disease not `HIDDEN_SCANNER` (an uncurable one
/// is the lowest value and gives way to any other, as in tg).
pub fn virus_state(diseases: &[crate::medical::diseases::Disease]) -> Option<&'static str> {
    use crate::medical::diseases::{HIDDEN_SCANNER, Severity};
    let mut worst: Option<Severity> = None;
    for d in diseases
        .iter()
        .filter(|d| d.visibility & HIDDEN_SCANNER == 0)
    {
        if worst.is_none_or(|w| w == Severity::Uncurable || d.severity > w) {
            worst = Some(d.severity);
        }
    }
    Some(match worst? {
        Severity::Uncurable => "hudill6",
        Severity::Biohazard => "hudill5",
        Severity::Dangerous => "hudill4",
        Severity::Harmful => "hudill3",
        Severity::Medium => "hudill2",
        Severity::Minor => "hudill1",
        Severity::Nonthreat => "hudill0",
        Severity::Positive => "hudbuff",
    })
}

/// What one human shows on each HUD.
struct Shown {
    mob: Entity,
    /// `DATA_HUD_SECURITY_ADVANCED`: the job, implants and wanted status.
    sec: Vec<crate::hud::HudIcon>,
    /// `DATA_HUD_SECURITY_BASIC`: the job only.
    id_only: Vec<crate::hud::HudIcon>,
    med: Vec<crate::hud::HudIcon>,
    /// Seen on `DATA_HUD_MEDICAL_BASIC` too.
    basic_med: bool,
}

fn refresh(world: &mut World, mut next: Local<f64>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + REFRESH_SECS;
    let mut humans = world.query_filtered::<(
        Entity,
        Option<&Inventory>,
        Option<&Health>,
        Option<&Stat>,
    ), With<HumanLook>>();
    let mut wearers = world.query::<(Entity, &PlayerMob, Option<&DataHudData>)>();
    let ghost_bodies: Vec<Entity> = world
        .query::<&crate::ghost::Ghost>()
        .iter(world)
        .filter_map(|g| g.body)
        .collect();
    let Some(icons) = world.get_resource::<IconIndex>() else {
        return;
    };
    let icon = |state: &str| {
        icons.get(HUDS_DMI, state).map(|id| crate::hud::HudIcon {
            icon_state: id,
            color: None,
        })
    };
    let manifest = world.resource::<Manifest>();
    // What each human shows on each HUD, computed once for all wearers.
    let shown: Vec<Shown> = humans
        .iter(world)
        .map(|(e, inventory, health, stat)| {
            let job = inventory
                .and_then(|i| i.id)
                .and_then(|id| crate::inventory::get_id(world, id))
                .and_then(|card| world.get::<crate::id_access::CardAccess>(card))
                .and_then(|c| c.trim.as_ref())
                .map(|t| t.sechud_icon_state.as_str())
                .filter(|s| !s.is_empty());
            let mut sec: Vec<_> = icon(job.unwrap_or("hudno_id")).into_iter().collect();
            let id_only = sec.clone();
            // `sec_hud_set_implants`: the security implants, then the mindshield.
            for imp in crate::implants::implants_of(world, e) {
                match imp.kind {
                    crate::implants::ImplantKind::Chem => sec.extend(icon("hud_imp_chem")),
                    crate::implants::ImplantKind::Tracking => {
                        sec.extend(icon("hud_imp_tracking"));
                    }
                    crate::implants::ImplantKind::Exile => sec.extend(icon("hud_imp_exile")),
                    crate::implants::ImplantKind::TeleportBlocker => {
                        sec.extend(icon("hud_imp_noteleport"));
                    }
                    crate::implants::ImplantKind::Beacon => sec.extend(icon("hud_imp_beacon")),
                    _ => {}
                }
            }
            if crate::antag::revolution::server::mindshielded(world, e) {
                sec.extend(icon("hud_imp_loyal"));
            }
            if let Some(state) = perp_name(world, e)
                .and_then(|name| manifest.find(&name))
                .and_then(|r| wanted_state(&r.wanted_status))
            {
                sec.extend(icon(state));
            }
            let dead = stat.is_some_and(|s| *s == Stat::Dead);
            let med = health
                .map(|h| {
                    let bar = format!("hud{}", round_health(h.health(), h.max, dead));
                    let status = med_status(world, e, inventory, dead, &ghost_bodies);
                    icon(&bar).into_iter().chain(icon(status)).collect()
                })
                .unwrap_or_default();
            Shown {
                mob: e,
                sec,
                id_only,
                med,
                basic_med: basic_health_visible(world, inventory),
            }
        })
        .collect();
    let Some(interest) = world.get_resource::<Interest>() else {
        return;
    };
    let mut updates = Vec::new();
    for (mob, player, old) in wearers.iter(world) {
        let (sec, med) = wearer_huds(world, mob);
        let lesser = ai_sensors(world, mob);
        if !sec && !med && !lesser && old.is_none_or(|d| d.entries.is_empty()) {
            continue;
        }
        let entries: Vec<DataHudEntry> = shown
            .iter()
            .filter_map(|h| {
                let id = interest.net_id(player.client, h.mob)?;
                let mut icons = Vec::new();
                if med || (lesser && h.basic_med) {
                    icons.extend(h.med.iter().copied());
                }
                if sec {
                    icons.extend(h.sec.iter().copied());
                } else if lesser {
                    icons.extend(h.id_only.iter().copied());
                }
                (!icons.is_empty()).then_some(DataHudEntry {
                    net_id: id.0,
                    icons,
                })
            })
            .collect();
        let data = DataHudData { entries };
        if old != Some(&data) {
            updates.push((mob, player.client, data));
        }
    }
    for (mob, client, data) in updates {
        world.entity_mut(mob).insert(data);
        world.write_message(OpenUi::new::<DataHudUi>(client, mob, mob, ""));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_bar_follows_round_health() {
        assert_eq!(round_health(100.0, 100.0, false), "health100");
        assert_eq!(round_health(95.0, 100.0, false), "health93.75");
        assert_eq!(round_health(50.0, 100.0, false), "health50");
        assert_eq!(round_health(0.5, 100.0, false), "health0");
        assert_eq!(round_health(-60.0, 200.0, false), "health-50");
        assert_eq!(round_health(-100.0, 100.0, false), "health-100");
        assert_eq!(round_health(100.0, 100.0, true), "health-100");
    }

    /// `check_virus`: the worst visible disease; an uncurable one gives way to any other.
    #[test]
    fn virus_icon_is_the_worst_seen_disease() {
        use crate::medical::diseases::{Disease, HIDDEN_SCANNER, Kind, Severity};
        let with = |severity: Severity, hidden: bool| {
            let mut d = Disease::new(Kind::Cold);
            d.severity = severity;
            d.visibility = if hidden { HIDDEN_SCANNER } else { 0 };
            d
        };
        assert_eq!(virus_state(&[]), None);
        assert_eq!(
            virus_state(&[with(Severity::Minor, false), with(Severity::Harmful, false)]),
            Some("hudill3")
        );
        assert_eq!(
            virus_state(&[
                with(Severity::Biohazard, true),
                with(Severity::Minor, false)
            ]),
            Some("hudill1"),
            "hidden from scanners"
        );
        assert_eq!(
            virus_state(&[with(Severity::Uncurable, false)]),
            Some("hudill6")
        );
        assert_eq!(
            virus_state(&[
                with(Severity::Uncurable, false),
                with(Severity::Positive, false)
            ]),
            Some("hudbuff")
        );
    }

    /// `update_wearer_status`: only tracking sensors show on the AI's lesser medical HUD.
    #[test]
    fn basic_health_needs_tracking_sensors() {
        use crate::clothing::sensors::{
            BROKEN_SENSORS, HAS_SENSORS, SENSOR_COORDS, SENSOR_VITALS, SuitSensor,
        };
        let mut world = World::new();
        let mut shown = |has, mode| {
            let suit = world.spawn(SuitSensor { has, mode }).id();
            let inventory = Inventory {
                uniform: Some(suit),
                ..default()
            };
            basic_health_visible(&world, Some(&inventory))
        };
        assert!(shown(HAS_SENSORS, SENSOR_COORDS));
        assert!(!shown(HAS_SENSORS, SENSOR_VITALS));
        assert!(!shown(BROKEN_SENSORS, SENSOR_COORDS));
        assert!(!basic_health_visible(&World::new(), None));
    }

    #[test]
    fn glasses_pick_their_huds() {
        assert_eq!(
            glasses_huds("/obj/item/clothing/glasses/hud/security/sunglasses"),
            (true, false)
        );
        assert_eq!(
            glasses_huds("/obj/item/clothing/glasses/hud/health/night"),
            (false, true)
        );
        assert_eq!(
            glasses_huds("/obj/item/clothing/glasses/hud/medsechud"),
            (true, true)
        );
        assert_eq!(
            glasses_huds("/obj/item/clothing/glasses/sunglasses"),
            (false, false)
        );
        assert_eq!(wanted_state(WANTED_ARREST), Some("hudwanted"));
        assert_eq!(wanted_state(WANTED_NONE), None);
    }
}
