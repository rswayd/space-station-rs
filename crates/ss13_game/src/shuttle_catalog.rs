//! Emergency shuttle purchases (server), after the communications console's `purchaseShuttle`
//! (`code/game/machinery/computer/communications.dm`: `can_buy_shuttles`,
//! `can_purchase_this_shuttle`), `/datum/map_template/shuttle/emergency`
//! (`code/datums/shuttles/emergency.dm`) and `SSshuttle.action_load(template, replace = TRUE)`.
//!
//! - The catalog ([`CATALOG`]): station emergency shuttles with tg's name, description, price
//!   and occupancy. Each one the checkout has loads with the station onto its own level, ready at
//!   its CentCom stand-in dock ([`ShuttleCatalog`]); the map's own shuttle is not offered.
//! - Buying ([`can_buy_shuttles`], [`purchase`]): a non-silicon login whose access is in a
//!   shuttle's `who_can_purchase` (the captain), the emergency shuttle idle or recalling, nothing
//!   bought yet this round, and the cargo budget holding the price. The bought shuttle becomes
//!   the emergency shuttle and keeps the call state (mode and timer) of the old one, which stays
//!   parked on its own level (tg `jumpToNullSpace`); cargo pays and a minor announcement names
//!   the buyer (scrambled from an emagged console).
// shortcut: no template loads mid-round: every startup system (doors, air, power, storage,
// closets, the emergency console...) initialises map entities once, so a template loaded later
// would come up inert. The catalog's templates (small station shuttles) load with the station
// instead, and a purchase swaps which one is the emergency shuttle. Only the [`CATALOG`] subset
// is offered: plain station shuttles without special machinery, prerequisites, emag lock or
// department lock. Upgrade when map loading gains a post-load init pass. No shuttle sale trait
// discount, no insurance event weight, no `allow_custom_shuttles` map opt-out (no station map
// sets it).

use std::path::PathBuf;

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
    push_template, setup_template_shuttle,
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

/// The shuttles offered (see the module's shortcut).
pub const CATALOG: [CatalogEntry; 9] = [
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
];

/// The catalog's loaded shuttles (tg `SSmapping.shuttle_templates` of the emergency port) and
/// `SSshuttle.shuttle_purchased`.
#[derive(Resource, Debug, Default)]
pub struct ShuttleCatalog {
    /// `(index into CATALOG, the loaded shuttle)`, in catalog order.
    pub shuttles: Vec<(usize, TemplateShuttle)>,
    /// `SHUTTLEPURCHASE_PURCHASED`.
    pub purchased: bool,
}

impl ShuttleCatalog {
    /// `SSshuttle.has_purchase_shuttle_access`: who may buy any of the loaded shuttles.
    pub fn purchase_access(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.shuttles
            .iter()
            .flat_map(|(i, _)| CATALOG[*i].who_can_purchase.iter().copied())
    }
}

#[derive(Resource)]
struct CatalogTemplates(Vec<(usize, PathBuf)>);

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
        let templates: Vec<(usize, PathBuf)> = CATALOG
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let path =
                    shuttle_template_path(&source, "emergency", &format!("emergency_{}", e.id));
                (i, path)
            })
            .filter(|(_, path)| *path != current && ss13_data::exists(path))
            .collect();
        for (_, path) in &templates {
            push_template(app, path.clone());
        }
        info!("emergency shuttle catalog: {} templates", templates.len());
        app.insert_resource(CatalogTemplates(templates))
            .add_systems(Startup, setup_catalog.after(crate::shuttle::setup_shuttle));
    }
}

/// tg `load_template` + `register` of each catalog template, parked at its CentCom dock.
fn setup_catalog(world: &mut World) {
    let Some(CatalogTemplates(templates)) = world.remove_resource::<CatalogTemplates>() else {
        return;
    };
    for (i, path) in templates {
        let id = CATALOG[i].id;
        let Some(shuttle) = setup_template_shuttle(
            world,
            &path,
            EMERGENCY_PORT,
            &format!("emergency_{id}"),
            &format!("emergency_{id}_away"),
            "CentCom Emergency Shuttle Dock",
        ) else {
            continue;
        };
        world
            .resource_mut::<ShuttleCatalog>()
            .shuttles
            .push((i, shuttle));
    }
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
    let Some((entry, shuttle)) = catalog
        .shuttles
        .iter()
        .find(|(i, _)| CATALOG[*i].id == id)
        .map(|(i, s)| (CATALOG[*i], s.clone()))
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
    {
        let mut emergency = world.resource_mut::<EmergencyShuttle>();
        emergency.port = shuttle.port;
        emergency.name = shuttle.name.clone();
        emergency.away = shuttle.away;
        emergency.transit = shuttle.transit;
        emergency.centcom_z = shuttle.level_z;
        emergency.transit_z = shuttle.transit_z;
        emergency.sound_played = false;
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
        let shuttle = TemplateShuttle {
            port: Entity::PLACEHOLDER,
            name: "box".into(),
            away: Entity::PLACEHOLDER,
            transit: Entity::PLACEHOLDER,
            level_z: 5,
            transit_z: 6,
        };
        ShuttleCatalog {
            shuttles: vec![(4, shuttle)],
            purchased: false,
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
