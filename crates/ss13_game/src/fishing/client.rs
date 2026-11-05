//! The fishing windows (client): the minigame (tg's fishing HUD as a window: the bar with the
//! bait and the fish, the completion meter) simulated here at `SSfishing`'s rate while the
//! left mouse button or Space reels, and tgui `Aquarium`.
// shortcut: plain coloured boxes instead of `fishing_hud.dmi` (backgrounds, frames, the fish
// icon) and no flipped HUD; the aquarium has no props tab, fish icons, petting or renaming.

use bevy::scene::EntityScene;
use ss13_net::client::SendIntent;
use ss13_shared::protocol::ClientIntent;
use ss13_tgui::client::{ActOnChange, AppTguiClientExt, ClientUi, ClientWindow};
use ss13_ui_kit::prelude::*;

use super::minigame::{AREA, Effect, Outcome, STEP, Sim};
use super::{
    AquariumData, AquariumStatic, AquariumUi, CHANGE_MODE, FEEDING_INTERVAL, FLUID,
    FishingMinigameUi, ITEM_REFERENCE, LOSE, MinigameParams, NEW_MODE, REMOVE_ITEM, TEMPERATURE,
    WIN,
};

/// The bar's height on screen.
const TRACK_PX: f32 = 228.0;
/// The most simulated time one frame catches up (a stalled frame does not replay seconds).
const MAX_CATCH_UP: f32 = 0.25;

pub struct FishingClientPlugin;

impl Plugin for FishingClientPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui_view::<FishingMinigameUi>()
            .register_ui_view::<AquariumUi>()
            .add_systems(Update, run_minigames);
    }
}

/// On the minigame view's root: the challenge as the server set it up.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct MinigameRoot(pub MinigameParams);

/// The running simulation, next to [`MinigameRoot`].
#[derive(Component)]
struct MinigameState {
    sim: Sim,
    carry: f32,
    done: bool,
}

#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct MinigameBait;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct MinigameFish;

#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct MinigameCompletion;

impl ClientUi for FishingMinigameUi {
    const WIDTH: f32 = 220.0;
    const HEIGHT: f32 = 300.0;

    fn content(params: &MinigameParams, _: &()) -> impl Scene {
        minigame_view(params)
    }
}

fn share(value: f32) -> f32 {
    (value / AREA * 100.0).clamp(0.0, 100.0)
}

fn minigame_view(params: &MinigameParams) -> impl Scene + use<> {
    let root = MinigameRoot(params.clone());
    let absolute = |left: Val, width: Val, bottom: f32, height: f32| Node {
        position_type: PositionType::Absolute,
        left,
        width,
        bottom: percent(bottom),
        height: percent(height),
        ..default()
    };
    let bait = absolute(
        px(2),
        px(44),
        share(params.bait_position),
        share(params.bait_height),
    );
    let fish = absolute(
        px(12),
        px(24),
        share(params.fish_position),
        share(params.fish_height),
    );
    let completion = absolute(px(0), px(12), 0.0, params.completion.clamp(0.0, 100.0));
    let water = match params.background.as_str() {
        "background_lavaland" => Color::srgb(0.25, 0.1, 0.08),
        "background_ice" => Color::srgb(0.35, 0.5, 0.6),
        _ => Color::srgb(0.08, 0.2, 0.35),
    };
    let hint = "Hold the left mouse button or Space to reel.".to_owned();
    bsn! {
        @WindowContent
        Children [
            Node { flex_direction: FlexDirection::Column, align_items: AlignItems::Center }
            root
            Children [
                Node { flex_direction: FlexDirection::Row, margin: UiRect::all(px(6)) }
                Children [
                    Node { width: px(48), height: px(TRACK_PX), margin: UiRect::right(px(6)) }
                    BackgroundColor(water)
                    Children [
                        bait
                        BackgroundColor(Color::srgba(0.4, 0.9, 0.4, 0.45))
                        MinigameBait
                        --
                        fish
                        BackgroundColor(Color::srgb(1.0, 0.6, 0.2))
                        MinigameFish
                    ]
                    --
                    Node { width: px(12), height: px(TRACK_PX) }
                    BackgroundColor(Color::srgb(0.15, 0.15, 0.15))
                    Children [
                        completion
                        BackgroundColor(Color::srgb(0.3, 0.8, 0.3))
                        MinigameCompletion
                    ]
                ]
                --
                Node Children [ Text(hint) ]
            ]
        ]
    }
}

/// Steps every shown minigame at [`STEP`] and reports its outcome once.
#[allow(clippy::too_many_arguments)]
fn run_minigames(
    time: Res<Time>,
    mouse: Option<Res<ButtonInput<MouseButton>>>,
    keys: Option<Res<ButtonInput<KeyCode>>>,
    mut roots: Query<(
        Entity,
        &MinigameRoot,
        Option<&mut MinigameState>,
        &InheritedVisibility,
    )>,
    mut marks: Query<
        (
            &mut Node,
            &mut BackgroundColor,
            Has<MinigameBait>,
            Has<MinigameFish>,
        ),
        Or<(
            With<MinigameBait>,
            With<MinigameFish>,
            With<MinigameCompletion>,
        )>,
    >,
    children: Query<&Children>,
    parents: Query<&ChildOf>,
    windows: Query<&ClientWindow>,
    mut intents: MessageWriter<SendIntent>,
    mut commands: Commands,
) {
    let reeling = mouse.is_some_and(|m| m.pressed(MouseButton::Left))
        || keys.is_some_and(|k| k.pressed(KeyCode::Space));
    for (root, params, state, visible) in &mut roots {
        // A render waiting to be swapped in is hidden: only the shown one plays.
        if !visible.get() {
            continue;
        }
        let Some(mut state) = state else {
            commands.entity(root).insert(MinigameState {
                sim: Sim::new(params.0.clone()),
                carry: 0.0,
                done: false,
            });
            continue;
        };
        if state.done {
            continue;
        }
        state.carry = (state.carry + time.delta_secs()).min(MAX_CATCH_UP);
        let mut outcome = Outcome::Running;
        while state.carry >= STEP && outcome == Outcome::Running {
            state.carry -= STEP;
            outcome = state.sim.step(reeling);
        }
        if outcome != Outcome::Running {
            state.done = true;
            let window = parents
                .iter_ancestors(root)
                .find_map(|e| windows.get(e).ok())
                .map(|w| w.id);
            if let Some(window) = window {
                let action = if outcome == Outcome::Won { WIN } else { LOSE };
                intents.write(SendIntent(ClientIntent::UiAct {
                    window,
                    action: action.to_owned(),
                    params: Vec::new(),
                }));
            }
        }
        let sim = &state.sim;
        let camo = sim.effect == Some(Effect::Camo);
        for e in children.iter_descendants(root) {
            let Ok((mut node, mut color, bait, fish)) = marks.get_mut(e) else {
                continue;
            };
            if bait {
                node.bottom = percent(share(sim.bait_position));
                node.height = percent(share(sim.p.bait_height));
            } else if fish {
                node.bottom = percent(share(sim.fish_position));
                color.0 = color.0.with_alpha(if camo { 0.03 } else { 1.0 });
            } else {
                node.height = percent(sim.completion);
            }
        }
    }
}

impl ClientUi for AquariumUi {
    const WIDTH: f32 = 520.0;
    const HEIGHT: f32 = 480.0;

    fn content(static_data: &AquariumStatic, data: &AquariumData) -> impl Scene {
        aquarium_view(static_data, data)
    }
}

fn aquarium_view(s: &AquariumStatic, data: &AquariumData) -> impl Scene + use<> {
    let fish: Vec<_> = data
        .fish_data
        .iter()
        .map(|f| {
            let mood = match f.fish_happiness {
                i32::MIN..=0 => "sad",
                1 => "content",
                2 => "happy",
                _ => "very happy",
            };
            let label = if f.fish_alive {
                format!("{} ({mood})", f.fish_name)
            } else {
                format!("{} (dead)", f.fish_name)
            };
            let params = ActParams(vec![(ITEM_REFERENCE.to_owned(), f.fish_ref.clone())]);
            bsn! {
                Node { flex_direction: FlexDirection::Row, align_items: AlignItems::Center }
                Children [
                    Node { flex_grow: 1.0 } Children [ Text(label) ]
                    --
                    @Button { @icon: "eject", @content: "Remove" }
                    Act({REMOVE_ITEM.to_owned()})
                    params
                ]
            }
        })
        .collect();
    let empty = data
        .fish_data
        .is_empty()
        .then(|| EntityScene(bsn! { Node Children [ Text({"No fish inside.".to_owned()}) ] }));
    let locked = data.locked_fluid_temp;
    let temperature = NumberInput {
        value: f64::from(data.temperature.round()),
        min_value: f64::from(s.min_temperature.round()),
        max_value: f64::from(s.max_temperature.round()),
        step: 1.0,
        step_pixel_size: 3.0,
        unit: "K".into(),
        ..Default::default()
    };
    let on_temperature = ActOnChange {
        action: TEMPERATURE.into(),
        param: TEMPERATURE.into(),
    };
    let interval = NumberInput {
        value: f64::from(data.feeding_interval.round()),
        min_value: 1.0,
        max_value: 60.0,
        step: 1.0,
        step_pixel_size: 5.0,
        unit: "min".into(),
        ..Default::default()
    };
    let on_interval = ActOnChange {
        action: FEEDING_INTERVAL.into(),
        param: FEEDING_INTERVAL.into(),
    };
    let fluids: Vec<_> = s
        .fluid_types
        .iter()
        .map(|fluid| {
            let selected = *fluid == data.fluid_type;
            let label = fluid.clone();
            let params = ActParams(vec![(FLUID.to_owned(), fluid.clone())]);
            bsn! {
                @Button { @content: label }
                Button { selected: selected, disabled: locked }
                Act({FLUID.to_owned()})
                params
            }
        })
        .collect();
    let modes: Vec<_> = s
        .aquarium_modes
        .iter()
        .map(|mode| {
            let selected = *mode == data.current_mode;
            let label = mode.clone();
            let params = ActParams(vec![(NEW_MODE.to_owned(), mode.clone())]);
            bsn! {
                @Button { @content: label }
                Button { selected: selected }
                Act({CHANGE_MODE.to_owned()})
                params
            }
        })
        .collect();
    let lock_note = locked.then(|| {
        EntityScene(bsn! {
            @NoticeBox { @color: UiColor::Yellow }
            Children [ Text({"LOCKED: Assisted mode sets the fluid and temperature.".to_owned()}) ]
        })
    });
    bsn! {
        @WindowContent
        Children [
            @Section { @title: "Fish", @fill: true, @scrollable: true }
            Children [
                Node { flex_direction: FlexDirection::Column }
                Children [ {empty} -- {fish} ]
            ]
            --
            {lock_note}
            --
            @Section { @title: "Temperature" }
            Children [ Node { width: px(90) } temperature on_temperature ]
            --
            @Section { @title: "Fluid" }
            Children [ Node { flex_wrap: FlexWrap::Wrap } Children [ {fluids} ] ]
            --
            @Section { @title: "Settings" }
            Children [
                @LabeledList
                Children [
                    @LabeledListItem { @label: "Aquarium Mode" }
                    Children [ Node { flex_wrap: FlexWrap::Wrap } Children [ {modes} ] ]
                    --
                    @LabeledListItem { @label: "Feeding Interval" }
                    Children [ Node { width: px(90) } interval on_interval ]
                ]
            ]
        ]
    }
}
