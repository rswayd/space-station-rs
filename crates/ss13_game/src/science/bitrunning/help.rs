//! The avatar's domain information and the host monitor (server), after tg
//! `code/modules/bitrunning/abilities.dm` (`/datum/avatar_help_text`,
//! `/datum/action/avatar_domain_info`) and `objects/host_monitor.dm`.
//!
//! - A connected avatar gets the "Open Domain Information" button; it opens tgui `AvatarHelp`
//!   with the domain's `help_text`. The button is granted on every connection (a mind leaving
//!   the avatar takes its buttons along, `crate::mind`).
//! - The host monitor, used in hand by an avatar, tells the health of its body in the pod.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmItem, IconIndex};
use ss13_shared::chat::Span;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed};

use super::avatar::AvatarConnection;
use super::{AvatarHelpStatic, AvatarHelpUi};
use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::health::Health;
use crate::obj_damage::is_under;
use crate::server::PlayerMob;
use crate::tools::tell;

pub const HOST_MONITOR: &str = "/obj/item/bitrunning_host_monitor";
/// The button's `trigger` id.
const BUTTON: &str = "avatar_domain_info";

/// `/datum/avatar_help_text` behind the avatar's button.
#[derive(Component, Clone, Debug, Default)]
pub struct AvatarHelp {
    pub help_text: String,
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    app.register_ui::<AvatarHelpUi>()
        .add_click_handler(ClickPhase::AttackSelf, 3367, |w, c| {
            host_monitor(w, c.client, c.mob, c.target)
        })
        .add_systems(Update, trigger_buttons);
}

/// `avatar_connection/Initialize`: the help datum and its action granted to the avatar.
pub fn grant(world: &mut World, avatar: Entity, help_text: &str) {
    world.entity_mut(avatar).insert(AvatarHelp {
        help_text: help_text.to_owned(),
    });
    let icon = |dmi: &str, state: &str| {
        world
            .get_resource::<IconIndex>()
            .and_then(|icons| icons.get(dmi, state))
    };
    let button = ActionButton {
        id: BUTTON.to_owned(),
        name: "Open Domain Information".to_owned(),
        icon: icon("icons/hud/actions.dmi", "round_end"),
        background: icon("icons/mob/actions/backgrounds.dmi", "bg_default"),
        border: None,
        available: true,
        cooldown: 0.0,
        active: false,
        maptext: None,
    };
    let mut buttons: Vec<(ActionButton, f64)> = world
        .get::<ActionButtons>(avatar)
        .map(|b| {
            b.0.iter()
                .filter(|(b, _)| b.id != BUTTON)
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    buttons.push((button, 0.0));
    set_buttons(world, avatar, buttons);
}

/// `avatar_domain_info/Trigger` -> `ui_interact(owner)`, checked at the moment of effect: the
/// client controls the mob and it still has the help.
fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .filter(|a| a.act == BUTTON)
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world.get::<AvatarHelp>(actor).is_none()
        {
            continue;
        }
        world.write_message(OpenUi::new::<AvatarHelpUi>(
            client,
            actor,
            actor,
            "Информация о домене",
        ));
    }
}

impl ServerUi for AvatarHelpUi {
    type Source = &'static AvatarHelp;
    type UserParam = ();
    type Act = ();

    fn static_data(help: &AvatarHelp) -> AvatarHelpStatic {
        AvatarHelpStatic {
            help_text: help.help_text.clone(),
        }
    }

    fn data(_: &AvatarHelp, _: Entity, _: &()) {}
}

/// `/obj/item/bitrunning_host_monitor/attack_self`.
fn host_monitor(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if !world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, HOST_MONITOR))
    {
        return false;
    }
    let Some(body) = world.get::<AvatarConnection>(mob).map(|c| c.body) else {
        crate::balloon::balloon_alert(world, mob, item, "данные не распознаны");
        return true;
    };
    let Some(percent) = world
        .get::<Health>(body)
        .map(|h| h.health() / h.max * 100.0)
    else {
        crate::balloon::balloon_alert(world, mob, item, "хост не распознан");
        return true;
    };
    tell(
        world,
        client,
        Span::Notice,
        format!("Current host health: {}%", crate::chem::dm_num(percent)),
    );
    true
}
