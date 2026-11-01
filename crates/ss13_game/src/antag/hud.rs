//! The antag HUD window (server): one headless `AntagHud` window per antagonist body, the source
//! of the antag info button and of the HUD icons its client draws (tg `antag_hud` on the own mob,
//! and the team's `has_antagonist` alternate appearance on the team mates it sees).
// shortcut: the team mates' ids follow the window's autoupdate (0.9 s), so a mate who just came
// into view gets the icon up to a refresh later; push on interest changes if that shows.

use bevy::prelude::*;
use ss13_net::server::Interest;
use ss13_tgui::server::{AppTguiExt, ServerUi, UiParams, UiReject};

use super::brother::{Brother, BrotherTeam};
use super::cult::server::{CultTeam, Cultist};
use super::revolution::server::{RevolutionHandler, Revolutionary};
use super::{AntagHudData, AntagHudUi, OPEN_INFO};
use crate::server::PlayerMob;

pub struct AntagHudPlugin;

impl Plugin for AntagHudPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<AntagHudUi>();
    }
}

impl ServerUi for AntagHudUi {
    type Source = (
        &'static AntagHudData,
        Option<&'static Brother>,
        Has<Cultist>,
        Has<Revolutionary>,
    );
    type UserParam = (
        Option<Res<'static, Interest>>,
        Query<'static, 'static, &'static PlayerMob>,
        Query<'static, 'static, &'static BrotherTeam>,
        Option<Res<'static, CultTeam>>,
        Option<Res<'static, RevolutionHandler>>,
        Query<'static, 'static, &'static AntagHudData>,
    );
    type Act = ();

    fn static_data(_: (&AntagHudData, Option<&Brother>, bool, bool)) {}

    fn data(
        (hud, brother, cultist, rev): (&AntagHudData, Option<&Brother>, bool, bool),
        user: Entity,
        (interest, players, teams, cult, revs, huds): &(
            Option<Res<Interest>>,
            Query<&PlayerMob>,
            Query<&BrotherTeam>,
            Option<Res<CultTeam>>,
            Option<Res<RevolutionHandler>>,
            Query<&AntagHudData>,
        ),
    ) -> AntagHudData {
        let members: Option<&[Entity]> = match (brother, cult) {
            (Some(brother), _) => teams.get(brother.team).ok().map(|t| t.members.as_slice()),
            (None, Some(cult)) if cultist => Some(cult.members.as_slice()),
            _ => None,
        };
        let team = match (interest, players.get(user), members) {
            (Some(interest), Ok(player), Some(members)) => members
                .iter()
                .filter(|&&m| m != user)
                .filter_map(|&m| interest.net_id(player.client, m))
                .collect(),
            _ => Vec::new(),
        };
        // tg `add_team_hud(/datum/antagonist/rev)`: every revolutionary with its own icon.
        let team_icons = match (interest, players.get(user), revs) {
            (Some(interest), Ok(player), Some(revs)) if rev => revs
                .members
                .iter()
                .filter(|&&m| m != user)
                .filter_map(|&m| {
                    let icon = huds.get(m).ok()?.hud_icon?;
                    Some((interest.net_id(player.client, m)?, icon))
                })
                .collect(),
            _ => Vec::new(),
        };
        AntagHudData {
            hud_icon: hud.hud_icon,
            team,
            team_icons,
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<(), UiReject> {
        match action {
            OPEN_INFO => Ok(()),
            _ => Err(UiReject::UnknownAction),
        }
    }
}
