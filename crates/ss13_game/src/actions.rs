//! Action buttons, after tg `/datum/action` and its `/atom/movable/screen/movable/action_button`
//! (`code/datums/actions/action.dm`, `cooldown_action.dm`, `code/_onclick/hud/action_group.dm`).
//!
//! A mob's buttons are the headless tgui interface [`ActionButtonsUi`] on the mob itself: the
//! server keeps [`ActionButtons`] (each button with the time it is ready again), sends them as
//! [`ActionButtonsData`] with the cooldown left, and a click on a button is the `trigger` act
//! with the button's `id`. Whoever granted the buttons reads `UiActed<ActionButtonsUi>` and checks
//! at the moment of effect that the id is still one of the mob's buttons (tg `Trigger`).
//! `actions_client` draws them in tg's top-left action row.
// shortcut: no dragging buttons around, no palette, no keybinds and no hover tooltips.

use serde::{Deserialize, Serialize};
use ss13_shared::IconStateId;
use ss13_tgui::UiInterface;

/// The buttons of the own mob (no window).
pub struct ActionButtonsUi;

impl UiInterface for ActionButtonsUi {
    const NAME: &'static str = "ActionButtons";
    type Data = ActionButtonsData;
    type StaticData = ();
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ActionButtonsData {
    pub buttons: Vec<ActionButton>,
}

/// One button as the client draws it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ActionButton {
    /// The `trigger` act's `id`.
    pub id: String,
    pub name: String,
    pub icon: Option<IconStateId>,
    /// `background_icon_state` and the `overlay_icon_state` border on top of the icon.
    pub background: Option<IconStateId>,
    pub border: Option<IconStateId>,
    /// tg `IsAvailable()` apart from the cooldown: false tints the button red.
    pub available: bool,
    /// Seconds of cooldown left when sent (tg `next_use_time - world.time`): red, with the
    /// seconds as maptext.
    pub cooldown: f32,
    /// tg `is_action_active`: a pointed spell waiting for its target.
    pub active: bool,
    /// Maptext of the button's own (charges), shown when there is no cooldown.
    pub maptext: Option<String>,
}

/// `act('trigger', { id })`.
pub const TRIGGER: &str = "trigger";

#[cfg(feature = "server")]
pub use server::*;

#[cfg(feature = "server")]
mod server {
    use bevy::prelude::*;
    use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiParams, UiReject};

    use super::{ActionButton, ActionButtonsData, ActionButtonsUi, TRIGGER};
    use crate::server::PlayerMob;

    /// A mob's buttons; `ready_at` is `Time::elapsed_secs_f64` of the end of each cooldown.
    #[derive(Component, Clone, Debug, Default, PartialEq)]
    pub struct ActionButtons(pub Vec<(ActionButton, f64)>);

    pub struct ActionButtonsPlugin;

    impl Plugin for ActionButtonsPlugin {
        fn build(&self, app: &mut App) {
            app.register_ui::<ActionButtonsUi>();
        }
    }

    /// Replaces the mob's buttons and opens the HUD for its client (tg `GiveAction`/`Remove`).
    pub fn set_buttons(world: &mut World, mob: Entity, buttons: Vec<(ActionButton, f64)>) {
        let Ok(mut entity) = world.get_entity_mut(mob) else {
            return;
        };
        match entity.get_mut::<ActionButtons>() {
            Some(mut old) => {
                old.set_if_neq(ActionButtons(buttons));
            }
            None => {
                entity.insert(ActionButtons(buttons));
                if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
                    world.write_message(OpenUi::new::<ActionButtonsUi>(client, mob, mob, ""));
                }
            }
        }
    }

    impl ServerUi for ActionButtonsUi {
        type Source = &'static ActionButtons;
        type UserParam = Res<'static, Time>;
        /// The button's id.
        type Act = String;

        fn static_data(_: &ActionButtons) {}

        fn data(source: &ActionButtons, _: Entity, time: &Res<Time>) -> ActionButtonsData {
            let now = time.elapsed_secs_f64();
            ActionButtonsData {
                buttons: source
                    .0
                    .iter()
                    .map(|(button, ready_at)| ActionButton {
                        cooldown: (ready_at - now).max(0.0) as f32,
                        ..button.clone()
                    })
                    .collect(),
            }
        }

        fn parse_act(action: &str, params: &UiParams) -> Result<String, UiReject> {
            match action {
                TRIGGER => params
                    .get("id")
                    .map(str::to_owned)
                    .ok_or_else(|| UiReject::BadParams("id: missing".into())),
                _ => Err(UiReject::UnknownAction),
            }
        }
    }
}
