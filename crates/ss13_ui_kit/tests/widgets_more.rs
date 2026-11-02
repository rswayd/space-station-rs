//! Wave-2 widgets driven through a real headless Bevy app: UI layout, text measuring and the UI
//! picking backend run (no GPU, no OS window), and the pointer is driven with `PointerInput`
//! messages, so hover, clicks, wheel and hit blocking go through the same code as in the client.

use std::time::Duration;

use bevy::camera::NormalizedRenderTarget;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::MouseScrollUnit;
use bevy::input::touch::TouchPhase;
use bevy::input_focus::FocusedInput;
use bevy::picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput};
use bevy::render::RenderPlugin;
use bevy::render::settings::{RenderCreation, WgpuSettings};
use bevy::time::TimeUpdateStrategy;
use bevy::ui::{ComputedNode, UiGlobalTransform};
use bevy::ui_widgets::ScrollbarThumb;
use bevy::window::PrimaryWindow;
use bevy::winit::WinitPlugin;
use ss13_ui_kit::UiKitPlugin;
use ss13_ui_kit::dropdown::{DropdownControl, DropdownEntry, DropdownMenu};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::scroll::ScrollContent;
use ss13_ui_kit::tooltip::TooltipPopup;
use ss13_ui_kit::virtual_list::VirtualListRow;

/// 50 ms per `update()`.
const FRAME: f32 = 0.05;

fn headless() -> App {
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins
            .build()
            .disable::<WinitPlugin>()
            .disable::<bevy::log::LogPlugin>()
            .set(RenderPlugin {
                render_creation: RenderCreation::Automatic(Box::new(WgpuSettings {
                    backends: None,
                    ..default()
                })),
                ..default()
            }),
        UiKitPlugin,
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        FRAME,
    )))
    .init_resource::<Acts>();
    app.world_mut().spawn(Camera2d);
    app
}

#[derive(Resource, Default)]
struct Acts(Vec<(String, Vec<(String, String)>)>);

fn record_acts(world: &mut World, window: Entity) {
    world
        .entity_mut(window)
        .observe(|act: On<UiAct>, mut acts: ResMut<Acts>| {
            acts.0.push((act.action.clone(), act.params.clone()));
        });
}

fn spawn(app: &mut App, scene: impl Scene) -> Entity {
    let window = app.world_mut().spawn_scene(scene).unwrap().id();
    record_acts(app.world_mut(), window);
    settle(app, 4);
    window
}

fn settle(app: &mut App, frames: usize) {
    for _ in 0..frames {
        app.update();
    }
}

fn single<C: Component>(app: &mut App) -> Entity {
    app.world_mut()
        .query_filtered::<Entity, With<C>>()
        .single(app.world())
        .unwrap()
}

fn count<C: Component>(app: &mut App) -> usize {
    app.world_mut()
        .query_filtered::<(), With<C>>()
        .iter(app.world())
        .count()
}

/// Center of a laid out node, in window (logical) px.
fn center(app: &App, entity: Entity) -> Vec2 {
    let node = app.world().get::<ComputedNode>(entity).unwrap();
    let transform = app.world().get::<UiGlobalTransform>(entity).unwrap();
    transform.translation * node.inverse_scale_factor
}

fn pointer(app: &mut App, position: Vec2, action: PointerAction) {
    let window = single::<PrimaryWindow>(app);
    let target = NormalizedRenderTarget::Window(
        bevy::window::WindowRef::Primary
            .normalize(Some(window))
            .unwrap(),
    );
    app.world_mut().write_message(PointerInput::new(
        PointerId::Mouse,
        Location { target, position },
        action,
    ));
    app.update();
}

fn move_to(app: &mut App, position: Vec2) {
    pointer(app, position, PointerAction::Move { delta: Vec2::ZERO });
}

fn click(app: &mut App, position: Vec2) {
    move_to(app, position);
    pointer(app, position, PointerAction::Press(PointerButton::Primary));
    pointer(
        app,
        position,
        PointerAction::Release(PointerButton::Primary),
    );
}

fn wheel(app: &mut App, position: Vec2, lines: f32) {
    move_to(app, position);
    pointer(
        app,
        position,
        PointerAction::Scroll {
            unit: MouseScrollUnit::Line,
            x: 0.0,
            y: lines,
            phase: TouchPhase::Moved,
        },
    );
}

#[derive(Component, Default, Clone)]
struct Marker;

#[test]
fn virtual_list_instantiates_only_visible_rows_and_follows_scroll() {
    let mut app = headless();
    let row = VirtualRow::new(|i| {
        let label = format!("row {i}");
        bsn! { Children [ Text(label) ] }
    });
    spawn(
        &mut app,
        bsn! {
            @Window { @width: 300.0, @height: 300.0 }
            Children [
                @WindowContent { @fitted: true }
                Children [
                    @VirtualList { @item_height: 20.0, @overscan: 2, @row: row }
                    VirtualList { len: 10_000 }
                    Node { height: px(200) }
                ]
            ]
        },
    );
    let rows = |app: &mut App| {
        let mut indices: Vec<usize> = app
            .world_mut()
            .query::<&VirtualListRow>()
            .iter(app.world())
            .map(|r| r.0)
            .collect();
        indices.sort();
        indices
    };

    // 200 px view / 20 px rows = 10 rows, + 2 overscan below; none above the top.
    assert_eq!(rows(&mut app), (0..12).collect::<Vec<_>>());

    // Scrolling deep into the list keeps the count and moves the window of rows.
    let content = single::<ScrollContent>(&mut app);
    for y in [5_000.0, 100_000.0, 199_800.0] {
        app.world_mut()
            .get_mut::<ScrollPosition>(content)
            .unwrap()
            .y = y;
        settle(&mut app, 1);
        let first = (y / 20.0) as usize - 2;
        let expected: Vec<usize> = (first..(first + 14).min(10_000)).collect();
        assert_eq!(rows(&mut app), expected, "scroll y = {y}");
    }

    // The whole list is scrollable: the spacer gives the content its full height.
    let node = app.world().get::<ComputedNode>(content).unwrap();
    let content_height = node.content_size().y * node.inverse_scale_factor;
    assert_eq!(content_height, 200_000.0);
}

#[test]
fn wheel_scrolls_a_scrollable_section_and_clamps_at_both_ends() {
    let mut app = headless();
    let lines: Vec<_> = (0..40)
        .map(|i| {
            let t = format!("line {i}");
            bsn! { Text(t) }
        })
        .collect();
    spawn(
        &mut app,
        bsn! {
            @Window { @width: 300.0, @height: 300.0 }
            Children [
                @WindowContent
                Children [
                    @Section { @title: "Log", @fill: true, @scrollable: true }
                    Children [ {lines} ]
                ]
            ]
        },
    );
    let content = single::<ScrollContent>(&mut app);
    let at = center(&app, content);
    let max = {
        let node = app.world().get::<ComputedNode>(content).unwrap();
        (node.content_size().y - node.size().y) * node.inverse_scale_factor
    };
    assert!(max > 100.0, "content must overflow, max = {max}");

    wheel(&mut app, at, -3.0);
    let y = app.world().get::<ScrollPosition>(content).unwrap().y;
    assert!(y > 0.0 && y < max, "one wheel step scrolls a bit: {y}");

    wheel(&mut app, at, -1000.0);
    assert_eq!(app.world().get::<ScrollPosition>(content).unwrap().y, max);

    wheel(&mut app, at, 1000.0);
    assert_eq!(app.world().get::<ScrollPosition>(content).unwrap().y, 0.0);

    // Dragging the thumb to the bottom of the track scrolls to the end, clamped.
    let thumb = single::<ScrollbarThumb>(&mut app);
    let grab = center(&app, thumb);
    move_to(&mut app, grab);
    pointer(&mut app, grab, PointerAction::Press(PointerButton::Primary));
    for step in 1..=10 {
        let to = grab + Vec2::new(0.0, 40.0 * step as f32);
        pointer(
            &mut app,
            to,
            PointerAction::Move {
                delta: Vec2::new(0.0, 40.0),
            },
        );
    }
    pointer(
        &mut app,
        grab + Vec2::new(0.0, 400.0),
        PointerAction::Release(PointerButton::Primary),
    );
    assert_eq!(app.world().get::<ScrollPosition>(content).unwrap().y, max);
}

/// Not an assertion on speed (debug builds, shared CI machines); prints the cost of jumping
/// through a 10k-row list so a regression is visible in `--nocapture` output.
#[test]
fn virtual_list_jumps_through_ten_thousand_rows() {
    let mut app = headless();
    let row = VirtualRow::new(|i| {
        let label = format!("row {i}");
        bsn! { Children [ Text(label) ] }
    });
    spawn(
        &mut app,
        bsn! {
            @Window { @width: 300.0, @height: 500.0 }
            Children [
                @WindowContent { @fitted: true }
                Children [
                    @VirtualList { @item_height: 18.0, @row: row }
                    VirtualList { len: 10_000 }
                    Node { height: px(400) }
                ]
            ]
        },
    );
    let content = single::<ScrollContent>(&mut app);
    let start = std::time::Instant::now();
    let frames = 100;
    for i in 0..frames {
        app.world_mut()
            .get_mut::<ScrollPosition>(content)
            .unwrap()
            .y = i as f32 * 1_789.0;
        app.update();
        assert!(count::<VirtualListRow>(&mut app) <= 400 / 18 + 1 + 2 * 4 + 1);
    }
    let per_frame = start.elapsed() / frames;
    println!("10k-row list, full row turnover every frame: {per_frame:?} per frame");
}

#[test]
fn clicking_tabs_sends_their_acts() {
    let mut app = headless();
    spawn(
        &mut app,
        bsn! {
            @Window
            Children [
                @WindowContent
                Children [
                    @Tabs
                    Children [
                        @TabsTab { @content: "One" } TabsTab { selected: true } Act("tab_one")
                        --
                        @TabsTab { @icon: "cog", @content: "Two" } Act("tab_two") Marker
                    ]
                ]
            ]
        },
    );
    let tab = single::<Marker>(&mut app);
    let at = center(&app, tab);
    click(&mut app, at);
    let acts = &app.world().resource::<Acts>().0;
    assert_eq!(acts, &[("tab_two".to_string(), vec![])]);
}

#[test]
fn dropdown_selects_by_click_and_keyboard_and_closes_on_outside_click() {
    let mut app = headless();
    let options: Vec<String> = ["Alpha", "Beta", "Gamma"].map(String::from).to_vec();
    let window = spawn(
        &mut app,
        bsn! {
            @Window { @width: 400.0, @height: 300.0 }
            Children [
                @WindowContent
                Children [
                    @Dropdown { @value_param: "destination" }
                    Dropdown { options: options, selected: "Alpha" }
                    Act("set_destination")
                ]
            ]
        },
    );
    let control = single::<DropdownControl>(&mut app);

    // Click opens a menu over everything, one entry per option.
    let at = center(&app, control);
    click(&mut app, at);
    settle(&mut app, 2);
    assert_eq!(count::<DropdownMenu>(&mut app), 1);
    assert_eq!(count::<DropdownEntry>(&mut app), 3);
    // Opaque: the window under the menu must not show through.
    let menu = single::<DropdownMenu>(&mut app);
    let background = app.world().get::<BackgroundColor>(menu).unwrap().0;
    assert_eq!(background.alpha(), 1.0);

    // Clicking "Gamma" sends the act with the value and closes the menu.
    let gamma = app
        .world_mut()
        .query::<(Entity, &DropdownEntry)>()
        .iter(app.world())
        .find(|(_, e)| e.index == 2)
        .unwrap()
        .0;
    let at = center(&app, gamma);
    click(&mut app, at);
    assert_eq!(
        app.world().resource::<Acts>().0,
        [(
            "set_destination".to_string(),
            vec![("destination".to_string(), "Gamma".to_string())]
        )]
    );
    assert_eq!(count::<DropdownMenu>(&mut app), 0);

    // A press outside an open menu closes it without selecting.
    let at = center(&app, control);
    click(&mut app, at);
    settle(&mut app, 2);
    assert_eq!(count::<DropdownMenu>(&mut app), 1);
    click(&mut app, Vec2::new(1000.0, 600.0));
    assert_eq!(count::<DropdownMenu>(&mut app), 0);
    assert_eq!(app.world().resource::<Acts>().0.len(), 1);

    // Keyboard: ArrowDown opens and highlights the option after "Alpha", Enter picks it.
    let key = |app: &mut App, code: KeyCode, logical: Key| {
        let input = KeyboardInput {
            key_code: code,
            logical_key: logical,
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window,
        };
        app.world_mut()
            .trigger(FocusedInput::new(control, input, window));
        app.update();
    };
    key(&mut app, KeyCode::ArrowDown, Key::ArrowDown);
    assert_eq!(count::<DropdownMenu>(&mut app), 1);
    key(&mut app, KeyCode::Enter, Key::Enter);
    assert_eq!(count::<DropdownMenu>(&mut app), 0);
    let acts = &app.world().resource::<Acts>().0;
    assert_eq!(acts.len(), 2);
    assert_eq!(acts[1].1, [("destination".to_string(), "Beta".to_string())]);
}

#[test]
fn tooltip_appears_after_hover_delay_and_hides_on_leave() {
    let mut app = headless();
    spawn(
        &mut app,
        bsn! {
            @Window
            Children [
                @WindowContent
                Children [
                    @Button { @content: "Hover me", @tooltip: "Helpful text" } Marker
                ]
            ]
        },
    );
    let button = single::<Marker>(&mut app);
    let at = center(&app, button);
    move_to(&mut app, at);
    // floating-ui's 200 ms rest delay: nothing after 100 ms.
    settle(&mut app, 1);
    assert_eq!(count::<TooltipPopup>(&mut app), 0);
    settle(&mut app, 4);
    let popup = single::<TooltipPopup>(&mut app);
    settle(&mut app, 2);
    let world = app.world();
    let text: Vec<String> = world
        .get::<Children>(popup)
        .unwrap()
        .iter()
        .filter_map(|c| world.get::<Text>(c).map(|t| t.0.clone()))
        .collect();
    assert_eq!(text, ["Helpful text"]);
    // Placed below the button (default `bottom`) and shown.
    let popup_center = center(&app, popup);
    assert!(popup_center.y > at.y);
    assert_eq!(
        app.world().get::<Visibility>(popup),
        Some(&Visibility::Inherited)
    );

    move_to(&mut app, Vec2::new(1200.0, 700.0));
    settle(&mut app, 1);
    assert_eq!(count::<TooltipPopup>(&mut app), 0);
}

#[test]
fn tooltip_is_never_drawn_off_its_place_and_survives_a_rerender() {
    let mut app = headless();
    spawn(
        &mut app,
        bsn! {
            @Window
            Children [
                @WindowContent
                Children [
                    @Button { @content: "Hover me", @tooltip: "Helpful text" } Marker
                ]
            ]
        },
    );
    let button = single::<Marker>(&mut app);
    let at = center(&app, button);
    move_to(&mut app, at);
    // Every frame it is visible, it is below the button, never at its spawn corner.
    let mut popup = None;
    for _ in 0..8 {
        settle(&mut app, 1);
        if count::<TooltipPopup>(&mut app) == 1 {
            let p = single::<TooltipPopup>(&mut app);
            if app.world().get::<Visibility>(p) == Some(&Visibility::Inherited) {
                assert!(center(&app, p).y > at.y, "drawn off its place");
            }
            popup = Some(p);
        }
    }
    let popup = popup.expect("shown");
    assert_eq!(app.world().get::<Visibility>(popup), Some(&Visibility::Inherited));

    // A tgui re-render: the button is replaced by an equal one in the same place.
    let parent = app.world().get::<ChildOf>(button).unwrap().parent();
    app.world_mut().despawn(button);
    let twin = app
        .world_mut()
        .spawn_scene(bsn! { @Button { @content: "Hover me", @tooltip: "Helpful text" } })
        .unwrap()
        .id();
    app.world_mut().entity_mut(twin).insert(ChildOf(parent));
    settle(&mut app, 3);
    assert_eq!(single::<TooltipPopup>(&mut app), popup, "the same popup, not a new one");
    assert_eq!(app.world().get::<Visibility>(popup), Some(&Visibility::Inherited));
}

fn modal_window(with_modal: bool) -> impl Scene {
    let modal = with_modal.then(|| {
        bsn! {
            Children [ @Modal Children [ Text("Locked") ] ]
        }
    });
    bsn! {
        @Window { @width: 300.0, @height: 200.0 }
        Children [
            @WindowContent
            Children [ @Button { @content: "Under the modal" } Act("under") Marker ]
            @{modal}
        ]
    }
}

#[test]
fn modal_blocks_clicks_underneath() {
    for with_modal in [false, true] {
        let mut app = headless();
        spawn(&mut app, modal_window(with_modal));
        let button = single::<Marker>(&mut app);
        let at = center(&app, button);
        click(&mut app, at);
        let acts = app.world().resource::<Acts>().0.len();
        assert_eq!(acts, usize::from(!with_modal), "with_modal = {with_modal}");
    }
}

#[test]
fn consecutive_buttons_flow_inline_and_wrap() {
    let mut app = headless();
    spawn(
        &mut app,
        bsn! {
            @Window { @width: 200.0, @height: 200.0 }
            Children [
                @WindowContent
                Children [
                    @Button { @content: "One" } Marker -- @Button { @content: "Two" }
                    -- @Button { @content: "Three" } -- @Button { @content: "Four, a long one" }
                ]
            ]
        },
    );
    let mut buttons: Vec<Vec2> = app
        .world_mut()
        .query_filtered::<Entity, With<Button>>()
        .iter(app.world())
        .collect::<Vec<_>>()
        .into_iter()
        .map(|b| center(&app, b))
        .collect();
    buttons.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    // The first three share a line; the long one wraps below.
    assert_eq!(buttons[0].y, buttons[1].y);
    assert_eq!(buttons[1].y, buttons[2].y);
    assert!(buttons[0].x < buttons[1].x && buttons[1].x < buttons[2].x);
    assert!(buttons[3].y > buttons[0].y);
}

#[test]
fn table_columns_line_up_across_rows() {
    let mut app = headless();
    spawn(
        &mut app,
        bsn! {
            @Window { @width: 400.0, @height: 200.0 }
            Children [
                @WindowContent
                Children [
                    @Table
                    Children [
                        @TableRow { @header: true }
                        Children [ @TableCell { @content: "Name" } -- @TableCell { @content: "Qty", @collapsing: true } ]
                        --
                        @TableRow
                        Children [ @TableCell { @content: "A much longer name" } -- @TableCell { @content: "12345", @collapsing: true } ]
                    ]
                ]
            ]
        },
    );
    settle(&mut app, 2);
    let rows: Vec<Vec<(f32, f32)>> = {
        let world = app.world_mut();
        let rows: Vec<Entity> = world
            .query_filtered::<Entity, With<TableRow>>()
            .iter(world)
            .collect();
        rows.iter()
            .map(|r| {
                let cells: Vec<Entity> = world.get::<Children>(*r).unwrap().iter().collect();
                cells
                    .iter()
                    .map(|c| {
                        let n = world.get::<ComputedNode>(*c).unwrap();
                        let t = world.get::<UiGlobalTransform>(*c).unwrap();
                        let left = (t.translation.x - n.size.x / 2.0) * n.inverse_scale_factor;
                        (left, n.size.x * n.inverse_scale_factor)
                    })
                    .collect()
            })
            .collect()
    };
    assert_eq!(rows[0], rows[1], "same left and width per column");
    // Full-width table: the auto column takes the spare width, the collapsing one does not.
    let (_, qty) = rows[0][1];
    assert!(qty < 60.0, "collapsing column hugs its content: {qty}");
    assert!(rows[0][0].1 > 250.0);
}
