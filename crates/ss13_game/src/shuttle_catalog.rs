//! Emergency shuttle purchases (server), after the communications console's `purchaseShuttle`
//! (`code/game/machinery/computer/communications.dm`: `can_buy_shuttles`,
//! `can_purchase_this_shuttle`), `/datum/map_template/shuttle/emergency`
//! (`code/datums/shuttles/emergency.dm`) and `SSshuttle.action_load(template, replace = TRUE)`.
//!
//! - The catalog ([`CATALOG`]): station emergency shuttles with tg's name, description, price
//!   and occupancy; the ones the checkout has are offered ([`ShuttleCatalog`]), the map's own
//!   shuttle is not. The bought one loads then (tg `action_load`) onto its own level, ready at
//!   its CentCom stand-in dock, and comes up initialized like the station
//!   (`ss13_mapload::load_template`, `ss13_world::run_map_init`).
//! - The Build Your Own Shuttle kits (`construction`, `constructionbig`, captain or chief
//!   engineer) dock at the station as soon as they are bought (`shuttle_build/postregister`);
//!   the call then finds them already docked.
//! - Buying ([`can_buy_shuttles`], [`purchase`]): a non-silicon login whose access is in a
//!   shuttle's `who_can_purchase` (the captain), the emergency shuttle idle or recalling, nothing
//!   bought yet this round, and the cargo budget holding the price. The bought shuttle becomes
//!   the emergency shuttle and keeps the call state (mode and timer) of the old one, which stays
//!   parked on its own level (tg `jumpToNullSpace`); cargo pays and a minor announcement names
//!   the buyer (scrambled from an emagged console).
// shortcut: only the [`CATALOG`] subset is offered: plain station shuttles without special
// machinery, prerequisites, emag lock or department lock. No shuttle sale trait discount, no
// insurance event weight, no `allow_custom_shuttles` map opt-out (no station map sets it).

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use ss13_mapload::MapLoadPlugin;
use ss13_mapload::dock::shuttle_template_path;
use ss13_mapload::spawner::Rng;
use ss13_shared::chat::Span;

use crate::cargo::MONEY_SYMBOL;
use crate::cargo::economy::{ACCOUNT_CAR, Economy};
use crate::comms::{PAGE_MAIN, ShuttleOffer};
use crate::comms_server::CommsConsole;
use crate::shuttle::{
    EMERGENCY_PORT, EmergencyShuttle, EmergencyTemplate, ShuttleMode, TemplateShuttle,
    setup_template_shuttle,
};
use crate::tools::tell;

/// tg `CARGO_CRATE_VALUE`.
pub const CARGO_CRATE_VALUE: i64 = 200;

/// A purchasable `/datum/map_template/shuttle/emergency/<id>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CatalogEntry {
    /// The `suffix`: `_maps/shuttles/emergency/emergency_<id>.dmm`.
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub credit_cost: i64,
    pub occupancy_limit: &'static str,
    pub who_can_purchase: &'static [&'static str],
}

const CAPTAIN: &[&str] = &["captain"];
const CAPTAIN_CE: &[&str] = &["captain", "ce"];
/// `/obj/docking_port/mobile/emergency/shuttle_build`: a BYOS kit's port.
const SHUTTLE_BUILD: &str = "/obj/docking_port/mobile/emergency/shuttle_build";

/// The shuttles offered (see the module's shortcut).
pub const CATALOG: [CatalogEntry; 11] = [
    CatalogEntry {
        id: "birdboat",
        name: "Birdboat Station Emergency Shuttle",
        description: "Though a little on the small side, this shuttle is feature complete, which \
                      is more than can be said for the pattern of station it was commissioned \
                      for.",
        credit_cost: CARGO_CRATE_VALUE * 2,
        occupancy_limit: "25",
        who_can_purchase: CAPTAIN,
    },
    CatalogEntry {
        id: "mini",
        name: "Ministation emergency shuttle",
        description: "Despite its namesake, this shuttle is actually only slightly smaller than \
                      standard, and still complete with a brig and medbay.",
        credit_cost: CARGO_CRATE_VALUE * 2,
        occupancy_limit: "35",
        who_can_purchase: CAPTAIN,
    },
    CatalogEntry {
        id: "pubby",
        name: "Pubby Station Emergency Shuttle",
        description: "A train but in space! Complete with a first, second class, brig and \
                      storage area.",
        credit_cost: CARGO_CRATE_VALUE * 2,
        occupancy_limit: "50",
        who_can_purchase: CAPTAIN,
    },
    CatalogEntry {
        id: "omega",
        name: "Omegastation Emergency Shuttle",
        description: "On the smaller size with a modern design, this shuttle is for the crew who \
                      like the cosier things, while still being able to stretch their legs.",
        credit_cost: CARGO_CRATE_VALUE * 2,
        occupancy_limit: "30",
        who_can_purchase: CAPTAIN,
    },
    CatalogEntry {
        id: "box",
        name: "Box Station Emergency Shuttle",
        description: "The gold standard in emergency exfiltration, this tried and true design is \
                      equipped with everything the crew needs for a safe flight home.",
        credit_cost: CARGO_CRATE_VALUE * 4,
        occupancy_limit: "45",
        who_can_purchase: CAPTAIN,
    },
    CatalogEntry {
        id: "asteroid",
        name: "Asteroid Station Emergency Shuttle",
        description: "A respectable mid-sized shuttle that first saw service shuttling \
                      Nanotrasen crew to and from their asteroid belt embedded facilities.",
        credit_cost: CARGO_CRATE_VALUE * 6,
        occupancy_limit: "50",
        who_can_purchase: CAPTAIN,
    },
    CatalogEntry {
        id: "meta",
        name: "Meta Station Emergency Shuttle",
        description: "A fairly standard shuttle, though larger and slightly better equipped than \
                      the Box Station variant.",
        credit_cost: CARGO_CRATE_VALUE * 8,
        occupancy_limit: "45",
        who_can_purchase: CAPTAIN,
    },
    CatalogEntry {
        id: "kilo",
        name: "Kilo Station Emergency Shuttle",
        description: "A fully functional shuttle including a complete infirmary, storage \
                      facilities and regular amenities.",
        credit_cost: CARGO_CRATE_VALUE * 10,
        occupancy_limit: "55",
        who_can_purchase: CAPTAIN,
    },
    CatalogEntry {
        id: "delta",
        name: "Delta Station Emergency Shuttle",
        description: "A large shuttle for a large station, this shuttle can comfortably fit all \
                      your overpopulation and crowding needs. Complete with all facilities plus \
                      additional equipment.",
        credit_cost: CARGO_CRATE_VALUE * 15,
        occupancy_limit: "75",
        who_can_purchase: CAPTAIN,
    },
    CatalogEntry {
        id: "construction",
        name: "Build your own shuttle kit",
        description: "For the enterprising shuttle engineer! The chassis will dock upon \
                      purchase, but launch will have to be authorized as usual via shuttle call. \
                      Comes stocked with construction materials.",
        credit_cost: CARGO_CRATE_VALUE * 5,
        occupancy_limit: "Flexible",
        who_can_purchase: CAPTAIN_CE,
    },
    CatalogEntry {
        id: "constructionbig",
        name: "Build your own CRUISER kit",
        description: "This is the big brother of the construction kit, with more space for your \
                      shuttle-building ideas! The chassis will dock upon purchase, but launch \
                      will have to be authorized as usual via shuttle call. Comes stocked with \
                      construction materials.",
        credit_cost: CARGO_CRATE_VALUE * 30,
        occupancy_limit: "Flexible and more",
        who_can_purchase: CAPTAIN_CE,
    },
];

/// The catalog's shuttles (tg `SSmapping.shuttle_templates` of the emergency port) and
/// `SSshuttle.shuttle_purchased`.
#[derive(Resource, Debug, Default)]
pub struct ShuttleCatalog {
    /// `(index into CATALOG, its template)`, in catalog order.
    pub shuttles: Vec<(usize, PathBuf)>,
    /// `SHUTTLEPURCHASE_PURCHASED`.
    pub purchased: bool,
    /// `SHUTTLEPURCHASE_FORCED`: an event replaced the shuttle (`crate::events::shuttle`).
    pub forced: bool,
    /// `SSshuttle.shuttle_insurance`, bought from the Shuttle Insurance event.
    pub insurance: bool,
}

impl ShuttleCatalog {
    /// `SSshuttle.has_purchase_shuttle_access`: who may buy any of the loaded shuttles.
    pub fn purchase_access(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.shuttles
            .iter()
            .flat_map(|(i, _)| CATALOG[*i].who_can_purchase.iter().copied())
    }
}

pub struct ShuttleCatalogPlugin;

impl Plugin for ShuttleCatalogPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShuttleCatalog>();
        // Only with a real station map and its emergency shuttle: fixture maps name `.dmm` files.
        let Some(current) = app
            .world()
            .get_resource::<EmergencyTemplate>()
            .map(|t| t.0.clone())
        else {
            return;
        };
        let Some(source) = app
            .world()
            .get_resource::<MapLoadPlugin>()
            .filter(|m| m.maps.first().is_some_and(|m| !m.ends_with(".dmm")))
            .map(|m| m.source.clone())
        else {
            return;
        };
        let shuttles: Vec<(usize, PathBuf)> = CATALOG
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let path =
                    shuttle_template_path(&source, "emergency", &format!("emergency_{}", e.id));
                (i, path)
            })
            .filter(|(_, path)| *path != current && ss13_data::exists(path))
            .collect();
        info!("emergency shuttle catalog: {} templates", shuttles.len());
        app.insert_resource(ShuttleCatalog {
            shuttles,
            ..default()
        });
    }
}

/// tg `action_load`: loads the catalog template mid-round, registers it at its CentCom stand-in
/// dock (`register`) and initializes what it brought (`initTemplateBounds`).
fn load_shuttle(world: &mut World, id: &str, path: &Path) -> Option<TemplateShuttle> {
    if let Err(e) = ss13_mapload::load_template(world, path) {
        warn!("emergency shuttle {}: {e:#}", path.display());
        ss13_world::run_map_init(world);
        return None;
    }
    let shuttle = setup_template_shuttle(
        world,
        path,
        EMERGENCY_PORT,
        &format!("emergency_{id}"),
        &format!("emergency_{id}_away"),
        "CentCom Emergency Shuttle Dock",
    );
    ss13_world::run_map_init(world);
    shuttle
}

/// tg `SSshuttle.action_load(template, replace = TRUE)`: the catalog shuttle `id` loaded from
/// `path` becomes the emergency shuttle, keeping the old one's call state (mode and timer);
/// the old one stays parked on its own level. False when the template fails to load.
pub fn replace_emergency(world: &mut World, id: &str, path: &Path) -> bool {
    let Some(shuttle) = load_shuttle(world, id, path) else {
        return false;
    };
    let mut emergency = world.resource_mut::<EmergencyShuttle>();
    emergency.port = shuttle.port;
    emergency.name = shuttle.name.clone();
    emergency.away = shuttle.away;
    emergency.transit = shuttle.transit;
    emergency.centcom_z = shuttle.level_z;
    emergency.transit_z = shuttle.transit_z;
    emergency.sound_played = false;
    let home = emergency.home;
    // `/obj/docking_port/mobile/emergency/shuttle_build/postregister`: a BYOS kit docks at the
    // station at once, to be built on until the shuttle is called.
    let byos = world
        .get::<ss13_mapload::DmDockingPort>(shuttle.port)
        .is_some_and(|d| crate::obj_damage::is_under(&d.path, SHUTTLE_BUILD));
    if byos && let Some(home) = home {
        let moved = crate::shuttle::initiate_docking(world, shuttle.port, home);
        info!("BYOS kit docked at the station: {moved:?}");
    }
    true
}

/// tg `can_buy_shuttles(user)`: `TRUE`, `FALSE` or the reason it is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuyStatus {
    No,
    Refused(&'static str),
    Yes,
}

/// tg `can_buy_shuttles(user)` (`FALSE` on a Syndicate console or with nothing to buy).
pub fn can_buy_shuttles(
    catalog: Option<&ShuttleCatalog>,
    mode: Option<ShuttleMode>,
    console: &CommsConsole,
    silicon: bool,
) -> BuyStatus {
    let Some(catalog) = catalog.filter(|c| !c.shuttles.is_empty()) else {
        return BuyStatus::No;
    };
    if silicon || console.syndicate {
        return BuyStatus::No;
    }
    let access = catalog
        .purchase_access()
        .any(|a| console.authorize_access.iter().any(|have| have == a));
    let Some(mode) = mode.filter(|_| access) else {
        return BuyStatus::No;
    };
    if !matches!(mode, ShuttleMode::Recall | ShuttleMode::Idle) {
        return BuyStatus::Refused("The shuttle is already in transit.");
    }
    if catalog.purchased {
        return BuyStatus::Refused("A replacement shuttle has already been purchased.");
    }
    if catalog.forced {
        return BuyStatus::Refused(
            "Due to unforseen circumstances, shuttle purchasing is no longer available.",
        );
    }
    BuyStatus::Yes
}

/// tg `can_purchase_this_shuttle`: the login's access is in `who_can_purchase`.
pub fn can_purchase_this_shuttle(entry: &CatalogEntry, console: &CommsConsole) -> bool {
    console
        .authorize_access
        .iter()
        .any(|a| entry.who_can_purchase.contains(&a.as_str()))
}

/// The window's `shuttles`: what this login may buy, by price.
pub fn offers(catalog: &ShuttleCatalog, console: &CommsConsole) -> Vec<ShuttleOffer> {
    let mut out: Vec<ShuttleOffer> = catalog
        .shuttles
        .iter()
        .map(|(i, _)| &CATALOG[*i])
        .filter(|e| can_purchase_this_shuttle(e, console))
        .map(|e| ShuttleOffer {
            id: e.id.to_owned(),
            name: e.name.to_owned(),
            description: e.description.to_owned(),
            occupancy_limit: e.occupancy_limit.to_owned(),
            credit_cost: e.credit_cost,
        })
        .collect();
    out.sort_by_key(|o| o.credit_cost);
    out
}

/// tg `purchaseShuttle`: every check at the moment of the act, then the swap, the payment and
/// the announcement.
pub fn purchase(
    world: &mut World,
    client: Entity,
    actor: Entity,
    console_e: Entity,
    silicon: bool,
    id: &str,
) {
    let Some(console) = world.get::<CommsConsole>(console_e).cloned() else {
        return;
    };
    let mode = world
        .get_resource::<EmergencyShuttle>()
        .map(|s| s.state.mode);
    match can_buy_shuttles(
        world.get_resource::<ShuttleCatalog>(),
        mode,
        &console,
        silicon,
    ) {
        BuyStatus::Yes => {}
        BuyStatus::Refused(why) => {
            tell(world, client, Span::Warning, why);
            return;
        }
        BuyStatus::No => return,
    }
    let catalog = world.resource::<ShuttleCatalog>();
    let Some((entry, path)) = catalog
        .shuttles
        .iter()
        .find(|(i, _)| CATALOG[*i].id == id)
        .map(|(i, p)| (CATALOG[*i], p.clone()))
    else {
        return;
    };
    if !can_purchase_this_shuttle(&entry, &console) {
        return;
    }
    let paid = world
        .get_resource_mut::<Economy>()
        .and_then(|mut e| {
            e.department_mut(ACCOUNT_CAR)
                .map(|a| a.adjust_money(-entry.credit_cost))
        })
        .unwrap_or(false);
    if !paid {
        return;
    }
    world.resource_mut::<ShuttleCatalog>().purchased = true;
    if !replace_emergency(world, entry.id, &path) {
        // tg `action_load` failing: the purchase did not happen.
        if let Some(mut economy) = world.get_resource_mut::<Economy>()
            && let Some(account) = economy.department_mut(ACCOUNT_CAR)
        {
            account.adjust_money(entry.credit_cost);
        }
        world.resource_mut::<ShuttleCatalog>().purchased = false;
        tell(world, client, Span::Warning, "Shuttle purchase failed.");
        return;
    }
    let real_name = world
        .get::<Name>(actor)
        .map(|n| n.to_string())
        .unwrap_or_default();
    let purchaser = if console.emagged {
        let mut rng = Rng::new(world.resource::<Time>().elapsed().as_nanos() as u64 ^ 0x4255_5953);
        crate::shuttle_console_server::scramble(
            "AUTHENTICATION FAILURE: CVE-2018-17107",
            60.0,
            &mut rng,
        )
    } else {
        real_name
    };
    crate::security_level::minor_announce(
        world,
        "Покупка шаттла",
        &format!(
            "{purchaser} купил {} за {}{MONEY_SYMBOL}.",
            entry.name, entry.credit_cost
        ),
        false,
    );
    info!("{actor} purchased the emergency shuttle {}", entry.name);
    if let Some(mut c) = world.get_mut::<CommsConsole>(console_e) {
        c.state = PAGE_MAIN;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn console(access: &[&str]) -> CommsConsole {
        CommsConsole {
            authenticated: true,
            authorize_access: access.iter().map(|a| (*a).to_owned()).collect(),
            ..default()
        }
    }

    fn catalog() -> ShuttleCatalog {
        ShuttleCatalog {
            shuttles: vec![(4, PathBuf::from("emergency_box.dmm"))],
            ..default()
        }
    }

    #[test]
    fn can_buy_follows_access_mode_and_the_purchase() {
        let captain = console(&["captain", "command"]);
        let hop = console(&["hop", "command"]);
        let c = catalog();
        let idle = Some(ShuttleMode::Idle);
        assert_eq!(
            can_buy_shuttles(Some(&c), idle, &captain, false),
            BuyStatus::Yes
        );
        assert_eq!(
            can_buy_shuttles(Some(&c), Some(ShuttleMode::Recall), &captain, false),
            BuyStatus::Yes
        );
        assert_eq!(can_buy_shuttles(Some(&c), idle, &hop, false), BuyStatus::No);
        assert_eq!(
            can_buy_shuttles(Some(&c), idle, &captain, true),
            BuyStatus::No
        );
        assert_eq!(
            can_buy_shuttles(Some(&ShuttleCatalog::default()), idle, &captain, false),
            BuyStatus::No
        );
        assert!(matches!(
            can_buy_shuttles(Some(&c), Some(ShuttleMode::Call), &captain, false),
            BuyStatus::Refused(_)
        ));
        let bought = ShuttleCatalog {
            purchased: true,
            ..catalog()
        };
        assert!(matches!(
            can_buy_shuttles(Some(&bought), idle, &captain, false),
            BuyStatus::Refused(_)
        ));
        let forced = ShuttleCatalog {
            forced: true,
            ..catalog()
        };
        assert!(matches!(
            can_buy_shuttles(Some(&forced), idle, &captain, false),
            BuyStatus::Refused(_)
        ));
        let offers = offers(&c, &captain);
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].id, "box");
        assert_eq!(offers[0].credit_cost, 800);
        assert!(super::offers(&c, &hop).is_empty());
    }

    #[test]
    fn catalog_ids_are_unique() {
        for (i, a) in CATALOG.iter().enumerate() {
            assert!(CATALOG[i + 1..].iter().all(|b| b.id != a.id), "{}", a.id);
        }
    }
}
