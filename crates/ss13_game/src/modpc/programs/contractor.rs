//! Syndicate Contractor Uplink (`/datum/computer_file/program/contract_uplink`) on the contractor
//! tablet: the screen and acts of `SyndicateContractor`; the hub itself is
//! [`crate::antag::contractor`].
//!
//! The program keeps who logged in (`traitor_data`), the error and the info screen on the
//! computer ([`ContractorTablet`]). Every act checks again that the user is that traitor.
// shortcut: no first-load animation (`first_load`, `PRG_set_first_load_finished`) and no
// program overlay icons.

use bevy::prelude::*;
use ss13_tgui::server::{UiParams, UiReject};

use crate::antag::contractor::{self, ContractStatus, ContractorHub};
use crate::modpc::pda::turf_of;
use crate::modpc::station_ui::*;

/// The program's state on the tablet.
#[derive(Component, Clone, Debug)]
pub struct ContractorTablet {
    /// `traitor_data`: the logged-in traitor's body.
    pub traitor: Option<Entity>,
    pub error: String,
    pub info_screen: bool,
}

impl Default for ContractorTablet {
    fn default() -> Self {
        Self {
            traitor: None,
            error: String::new(),
            info_screen: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContractAct {
    Login,
    Accept(u32),
    Extraction,
    Abort,
    Redeem,
    ClearError,
    ToggleInfo,
}

pub fn parse(action: &str, params: &UiParams) -> Option<Result<ContractAct, UiReject>> {
    Some(Ok(match action {
        CONTRACT_LOGIN => ContractAct::Login,
        CONTRACT_ACCEPT => {
            return Some(
                params
                    .get("contract_id")
                    .and_then(|id| id.parse::<u32>().ok())
                    .map(ContractAct::Accept)
                    .ok_or_else(|| UiReject::BadParams("contract_id: a number".into())),
            );
        }
        CONTRACT_EXTRACTION => ContractAct::Extraction,
        CONTRACT_ABORT => ContractAct::Abort,
        CONTRACT_REDEEM => ContractAct::Redeem,
        CONTRACT_CLEAR_ERROR => ContractAct::ClearError,
        CONTRACT_TOGGLE_INFO => ContractAct::ToggleInfo,
        _ => return None,
    }))
}

/// `ui_data`. The dropoff direction is taken from the tablet's tile (tg: the user's).
pub fn data(world: &World, pc: Entity) -> ContractorData {
    let tablet = world
        .get::<ContractorTablet>(pc)
        .cloned()
        .unwrap_or_default();
    let mut data = ContractorData {
        logged_in: tablet.traitor.is_some(),
        station_name: crate::events::station_name(world),
        info_screen: tablet.info_screen,
        error: tablet.error.clone(),
        ..default()
    };
    let Some(hub) = tablet.traitor.and_then(|t| world.get::<ContractorHub>(t)) else {
        return data;
    };
    if let Some(current) = hub.current.and_then(|i| hub.contracts.get(i)) {
        data.ongoing_contract = true;
        data.extraction_enroute = current.status == ContractStatus::Extracting;
        let here = turf_of(world, pc);
        data.current_location = here
            .map(|p| crate::bots::area_name(world, p))
            .unwrap_or_default();
        data.dropoff_direction = match (here, current.dropoff_pos) {
            (Some(here), Some(drop)) if here.z == drop.z => {
                if current.dropoff.is_some() && contractor::area_of(world, here) == current.dropoff
                {
                    "LOCATION CONFIRMED".to_owned()
                } else {
                    contractor::dir_word(here, drop).to_owned()
                }
            }
            _ => "???".to_owned(),
        };
    }
    data.redeemable_tc = hub.tc_to_redeem;
    data.earned_tc = hub.tc_paid;
    data.contracts_completed = hub.completed;
    data.contracts = hub
        .contracts
        .iter()
        .map(|c| ContractRow {
            id: c.id,
            target: c.target_name.clone(),
            target_rank: c.target_rank.clone(),
            payout: c.payout,
            payout_bonus: c.payout_bonus,
            dropoff: c.dropoff_name.clone(),
            status: c.status as u8,
            message: c.wanted_message.clone(),
        })
        .collect();
    data
}

/// `ui_act`.
pub fn apply(
    world: &mut World,
    _client: Entity,
    actor: Entity,
    pc: Entity,
    act: ContractAct,
) -> Result<(), &'static str> {
    let mut tablet = world
        .get::<ContractorTablet>(pc)
        .cloned()
        .unwrap_or_default();
    let result = match act {
        ContractAct::Login => {
            if contractor::login(world, actor) {
                tablet.traitor = Some(actor);
            } else {
                tablet.error = "UNAUTHORIZED USER".to_owned();
            }
            Ok(())
        }
        ContractAct::ClearError => {
            tablet.error.clear();
            Ok(())
        }
        ContractAct::ToggleInfo => {
            tablet.info_screen = !tablet.info_screen;
            Ok(())
        }
        // The rest act for the logged-in traitor, who must be the one using the tablet.
        _ if tablet.traitor != Some(actor) => Err("not the logged-in contractor"),
        ContractAct::Accept(id) => {
            if contractor::accept(world, actor, id) {
                Ok(())
            } else {
                Err("cannot accept")
            }
        }
        ContractAct::Abort => {
            if contractor::abort(world, actor) {
                Ok(())
            } else {
                Err("no contract")
            }
        }
        ContractAct::Redeem => {
            contractor::redeem(world, actor);
            Ok(())
        }
        ContractAct::Extraction => {
            if let Err(error) = contractor::call_extraction(world, actor) {
                tablet.error = error.to_owned();
            }
            Ok(())
        }
    };
    if world.get_entity(pc).is_ok() {
        world.entity_mut(pc).insert(tablet);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acts_parse_and_reject_bad_ids() {
        let none = UiParams::new(&[]);
        assert_eq!(
            parse(CONTRACT_LOGIN, &none).unwrap().unwrap(),
            ContractAct::Login
        );
        assert!(parse(CONTRACT_ACCEPT, &none).unwrap().is_err());
        let raw = [("contract_id".to_owned(), "3".to_owned())];
        let params = UiParams::new(&raw);
        assert_eq!(
            parse(CONTRACT_ACCEPT, &params).unwrap().unwrap(),
            ContractAct::Accept(3)
        );
        assert!(parse("unrelated", &params).is_none());
    }

    #[test]
    fn only_a_traitor_logs_in() {
        let mut world = World::new();
        let pc = world.spawn_empty().id();
        let crew = world.spawn_empty().id();
        apply(&mut world, crew, crew, pc, ContractAct::Login).unwrap();
        let tablet = world.get::<ContractorTablet>(pc).unwrap();
        assert_eq!(tablet.traitor, None);
        assert_eq!(tablet.error, "UNAUTHORIZED USER");
        // Not logged in: no contract acts.
        assert!(apply(&mut world, crew, crew, pc, ContractAct::Abort).is_err());
    }
}
