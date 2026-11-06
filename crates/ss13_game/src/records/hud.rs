//! The security and medical data HUDs (server) after tg `code/game/data_huds.dm`: a mob wearing
//! security HUD glasses (`TRAIT_SECURITY_HUD`) sees over every human it sees the job icon of
//! the worn ID (`update_ID_card`) and the wanted status of its crew record
//! (`sec_hud_set_security_status`); medical HUD glasses (`TRAIT_MEDICAL_HUD`) show the health
//! bar (`med_hud_set_health`, `RoundHealth`) and the status (`med_hud_set_status`). The images
//! go to the wearer only, as the headless `DataHud` interface keyed by its net ids.
//!
//! Cost: every [`REFRESH_SECS`], per wearer, one pass over the humans (an interest lookup
//! each): wearers × humans; a few wearers among 100 players is a few hundred lookups per
//! refresh.
// shortcut: glasses give their HUD by type (`/obj/item/clothing/glasses/hud/security`,
// `.../health`, `.../medsechud` and subtypes), not by `clothing_traits` (no toggling HUD
// glasses or silicon HUDs; HUD implants come from `crate::medical::cyberimps`); the record is
// found by the mob's real name (tg: the face name, which a mask hides); no second implant slot
// offset, virus or defib states; refreshed every 0.5 s rather
// than on each change.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_net::server::Interest;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiParams, UiReject, UiSystems};

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

    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
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

/// Which data HUDs the glasses of type `path` give: (security, medical).
pub fn glasses_huds(path: &str) -> (bool, bool) {
    (
        SECURITY_GLASSES.iter().any(|r| is_under(path, r)),
        MEDICAL_GLASSES.iter().any(|r| is_under(path, r)),
    )
}

fn refresh(world: &mut World, mut next: Local<f64>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + REFRESH_SECS;
    let mut humans = world.query_filtered::<(
        Entity,
        &Name,
        Option<&Inventory>,
        Option<&Health>,
        Option<&Stat>,
    ), With<HumanLook>>();
    let mut wearers = world.query::<(Entity, &Inventory, &PlayerMob, Option<&DataHudData>)>();
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
    let shown: Vec<(Entity, Vec<crate::hud::HudIcon>, Vec<crate::hud::HudIcon>)> = humans
        .iter(world)
        .map(|(e, name, inventory, health, stat)| {
            let job = inventory
                .and_then(|i| i.id)
                .and_then(|id| crate::inventory::get_id(world, id))
                .and_then(|card| world.get::<crate::id_access::CardAccess>(card))
                .and_then(|c| c.trim.as_ref())
                .map(|t| t.sechud_icon_state.as_str())
                .filter(|s| !s.is_empty());
            let mut sec: Vec<_> = icon(job.unwrap_or("hudno_id")).into_iter().collect();
            // `sec_hud_set_implants`: the security implants, then the mindshield.
            for imp in crate::implants::implants_of(world, e) {
                match imp.kind {
                    crate::implants::ImplantKind::Chem => sec.extend(icon("hud_imp_chem")),
                    crate::implants::ImplantKind::Tracking => {
                        sec.extend(icon("hud_imp_tracking"));
                    }
                    _ => {}
                }
            }
            if crate::antag::revolution::server::mindshielded(world, e) {
                sec.extend(icon("hud_imp_loyal"));
            }
            if let Some(state) = manifest
                .find(name.as_str())
                .and_then(|r| wanted_state(&r.wanted_status))
            {
                sec.extend(icon(state));
            }
            let dead = stat.is_some_and(|s| *s == Stat::Dead);
            let med = health
                .map(|h| {
                    let bar = format!("hud{}", round_health(h.health(), h.max, dead));
                    let status = if dead { "huddead" } else { "hudhealthy" };
                    icon(&bar).into_iter().chain(icon(status)).collect()
                })
                .unwrap_or_default();
            (e, sec, med)
        })
        .collect();
    let Some(interest) = world.get_resource::<Interest>() else {
        return;
    };
    let mut updates = Vec::new();
    for (mob, inventory, player, old) in wearers.iter(world) {
        let (sec, med) = inventory
            .eyes
            .and_then(|e| world.get::<DmItem>(e))
            .map_or((false, false), |i| glasses_huds(&i.path));
        let (implant_sec, implant_med) = crate::medical::cyberimps::implant_huds(world, mob);
        // MOD security and medical visors (`crate::modsuit::modules::VisorHud`).
        let (visor_sec, visor_med) = crate::modsuit::modules::visor_huds(world, mob);
        // The ablative hood's `TRAIT_SECURITY_HUD` (`crate::clothing::ablative`).
        let hood_sec = crate::clothing::ablative::hood_hud(world, inventory.head);
        let (sec, med) = (
            sec || implant_sec || visor_sec || hood_sec,
            med || implant_med || visor_med,
        );
        if !sec && !med && old.is_none_or(|d| d.entries.is_empty()) {
            continue;
        }
        let entries: Vec<DataHudEntry> = shown
            .iter()
            .filter_map(|(e, s, m)| {
                let id = interest.net_id(player.client, *e)?;
                let mut icons = Vec::new();
                if med {
                    icons.extend(m.iter().copied());
                }
                if sec {
                    icons.extend(s.iter().copied());
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
