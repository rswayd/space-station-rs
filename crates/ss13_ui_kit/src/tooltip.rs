//! tgui `Tooltip`: hover text shown after a short rest, as `Button tooltip="..."` and
//! `<Tooltip content="...">` do.

use bevy::color::Alpha;
use bevy::picking::Pickable;
use bevy::picking::hover::Hovered;
use bevy::prelude::{
    App, BackgroundColor, BorderRadius, BoxShadow, Changed, Children, Color, Commands, Component,
    Entity, GlobalZIndex, IntoScheduleConfigs, Node, PositionType, Query, Res, ResMut, Resource,
    Time, UiRect, Update, px,
};
use bevy::scene::{CommandsSceneExt, bsn};
use bevy::ui::widget::Text;

use crate::overlay::{Anchored, Placement, TOOLTIP_Z};
use crate::theme::{self, root_text_style};

/// Put on any UI node to give it a tooltip: `Tooltip { content: "...".into(), ..default() }`.
/// tgui's `<Tooltip>` wrapper becomes this component on the wrapped node.
#[derive(Component, Clone, Debug, Default, PartialEq)]
#[require(Hovered)]
pub struct Tooltip {
    pub content: String,
    pub position: Placement,
}

/// Seconds the pointer must rest on the anchor: floating-ui `useHover({ restMs: 200 })`.
pub const TOOLTIP_DELAY: f32 = 0.2;
/// `contentOffset` of `Floating`.
const TOOLTIP_OFFSET: f32 = 6.0;
/// `--tooltip-background`: base at 5% lightness, 0.85 alpha.
const TOOLTIP_BACKGROUND: Color = Color::hsla(0.0, 0.0, 0.05, 0.85);
/// `max-width: em(250px)`.
const TOOLTIP_MAX_WIDTH: f32 = 250.0;

/// The floating box of the tooltip being shown.
#[derive(Component, Clone, Debug, Default)]
pub struct TooltipPopup;

/// Seconds a shown tooltip outlives its despawned anchor, waiting for a hovered anchor with the
/// same text to take it over: a tgui window re-renders its content on every data update (the
/// autoupdate, ~1 s), which replaces the button under the pointer with an equal one.
const ORPHAN_GRACE: f32 = 0.25;

/// Which anchor the pointer rests on, for how long, and its popup once shown.
#[derive(Resource, Default, Debug)]
pub struct TooltipState {
    pub anchor: Option<Entity>,
    pub rested: f32,
    pub popup: Option<Entity>,
    /// The shown popup's text, matched against a new anchor taking it over.
    pub content: String,
    /// Seconds since the shown popup's anchor was despawned.
    pub orphaned: f32,
}

fn track_hover(
    changed: Query<(Entity, &Hovered, &Tooltip), Changed<Hovered>>,
    anchors: Query<(), bevy::prelude::With<Tooltip>>,
    mut popups: Query<&mut Anchored, bevy::prelude::With<TooltipPopup>>,
    mut state: ResMut<TooltipState>,
    mut commands: Commands,
) {
    for (entity, hovered, tooltip) in &changed {
        if hovered.get() && state.anchor != Some(entity) {
            // The re-rendered twin of the anchor whose popup is shown: the popup moves over.
            let orphan =
                state.orphaned > 0.0 || state.anchor.is_some_and(|a| !anchors.contains(a));
            if orphan
                && let Some(popup) = state.popup
                && tooltip.content == state.content
                && let Ok(mut anchored) = popups.get_mut(popup)
            {
                anchored.anchor = entity;
                state.anchor = Some(entity);
                state.orphaned = 0.0;
                continue;
            }
            close(&mut state, &mut commands);
            state.anchor = Some(entity);
        } else if !hovered.get() && state.anchor == Some(entity) {
            close(&mut state, &mut commands);
        }
    }
}

fn close(state: &mut TooltipState, commands: &mut Commands) {
    if let Some(popup) = state.popup.take() {
        commands.entity(popup).despawn();
    }
    state.anchor = None;
    state.rested = 0.0;
    state.orphaned = 0.0;
    state.content.clear();
}

fn show_after_delay(
    time: Res<Time>,
    anchors: Query<(&Tooltip, &Hovered)>,
    mut state: ResMut<TooltipState>,
    mut commands: Commands,
) {
    let Some(anchor) = state.anchor else {
        return;
    };
    let Ok((tooltip, hovered)) = anchors.get(anchor) else {
        // The anchor was despawned or lost its tooltip under the pointer; a shown popup waits
        // briefly for a re-rendered twin ([`track_hover`]).
        if state.popup.is_some() && state.orphaned < ORPHAN_GRACE {
            state.orphaned += time.delta_secs().max(f32::EPSILON);
            return;
        }
        close(&mut state, &mut commands);
        return;
    };
    if !hovered.get() {
        close(&mut state, &mut commands);
        return;
    }
    state.rested += time.delta_secs();
    if state.popup.is_some() || state.rested < TOOLTIP_DELAY || tooltip.content.is_empty() {
        return;
    }
    let content = tooltip.content.clone();
    state.content = content.clone();
    let anchored = Anchored {
        anchor,
        placement: tooltip.position,
        offset: TOOLTIP_OFFSET,
        match_width: false,
    };
    let shadow = BoxShadow::new(Color::BLACK.with_alpha(0.5), px(1), px(1), px(-1), px(15));
    let popup = commands
        .spawn_scene(bsn! {
            Node {
                position_type: PositionType::Absolute,
                max_width: px(TOOLTIP_MAX_WIDTH),
                padding: UiRect::axes(px(theme::SPACE_L), px(theme::SPACE_M)),
                border_radius: BorderRadius::all(px(theme::RADIUS_MEDIUM)),
            }
            TooltipPopup
            anchored
            GlobalZIndex(TOOLTIP_Z)
            // `pointer-events: none`: the popup never steals the hover from its anchor.
            Pickable::IGNORE
            BackgroundColor(TOOLTIP_BACKGROUND)
            shadow
            @root_text_style()
            Children [ Text(content) Pickable::IGNORE ]
        })
        .id();
    state.popup = Some(popup);
}

pub(crate) fn plugin(app: &mut App) {
    app.init_resource::<TooltipState>()
        .add_systems(Update, (track_hover, show_after_delay).chain());
}
