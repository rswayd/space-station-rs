//! Spawned hierarchy shape and event paths, without a renderer.

use bevy::app::TaskPoolPlugin;
use bevy::asset::AssetPlugin;
use bevy::scene::ScenePlugin;
use bevy::ui_widgets::Activate;
use ss13_ui_kit::UiKitPlugin;
use ss13_ui_kit::labeled_list::LabelCell;
use ss13_ui_kit::layout::{Part, Slot};
use ss13_ui_kit::prelude::*;
use ss13_ui_kit::section::SectionTitle;
use ss13_ui_kit::window::{CloseButton, TitleBar};

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((
        TaskPoolPlugin::default(),
        AssetPlugin::default(),
        ScenePlugin,
        UiKitPlugin,
    ));
    app
}

#[derive(Component, Default, Clone)]
struct Marker;

fn sample() -> impl Scene {
    bsn! {
        @Window { @title: "Teleporter" }
        Children [
            @WindowContent
            Children [
                @Section { @title: "Status" }
                Children [
                    @LabeledList
                    Children [
                        @LabeledListItem {
                            @label: "Regime",
                            @buttons: bsn! { @Button { @content: "Calibrate" } Act("calibrate") },
                        }
                        Children [
                            @Button { @content: "Teleporter" } Act("regimeset") Marker
                        ]
                    ]
                ]
            ]
        ]
    }
}

fn children(world: &World, e: Entity) -> Vec<Entity> {
    world
        .get::<Children>(e)
        .map(|c| c.iter().collect())
        .unwrap_or_default()
}

fn only_with<C: Component>(world: &World, list: &[Entity]) -> Entity {
    let found: Vec<_> = list
        .iter()
        .copied()
        .filter(|e| world.get::<C>(*e).is_some())
        .collect();
    assert_eq!(
        found.len(),
        1,
        "expected exactly one {}",
        std::any::type_name::<C>()
    );
    found[0]
}

fn text_under(world: &World, e: Entity) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(t) = world.get::<Text>(e) {
        out.push(t.0.clone());
    }
    for c in children(world, e) {
        out.extend(text_under(world, c));
    }
    out
}

#[test]
fn user_children_land_in_slots_like_jsx() {
    let mut app = app();
    let world = app.world_mut();
    let window = world.spawn_scene(sample()).unwrap().id();
    world.flush();

    // Window: title bar, content slot, resize grip; the user's Window.Content is in the slot.
    let parts = children(world, window);
    assert!(parts.iter().all(|e| world.get::<Part>(*e).is_some()));
    only_with::<TitleBar>(world, &parts);
    let slot = only_with::<Slot>(world, &parts);
    let content = children(world, slot);
    let content = only_with::<WindowContent>(world, &content);

    // Section: title then content slot.
    let section = only_with::<Section>(world, &children(world, content));
    let section_parts = children(world, section);
    assert_eq!(section_parts.len(), 2);
    assert!(world.get::<SectionTitle>(section_parts[0]).is_some());
    assert_eq!(text_under(world, section_parts[0]), ["Status"]);
    let section_slot = section_parts[1];
    assert!(world.get::<Slot>(section_slot).is_some());

    // LabeledList.Item: label, value slot, buttons, in table column order.
    let list = only_with::<LabeledList>(world, &children(world, section_slot));
    let item = only_with::<LabeledListItem>(world, &children(world, list));
    let cells = children(world, item);
    assert_eq!(cells.len(), 3);
    assert!(world.get::<LabelCell>(cells[0]).is_some());
    assert_eq!(text_under(world, cells[0]), ["Regime:"]);
    assert!(world.get::<Slot>(cells[1]).is_some());
    let value = only_with::<Marker>(world, &children(world, cells[1]));
    assert_eq!(text_under(world, value), ["Teleporter"]);
    assert_eq!(text_under(world, cells[2]), ["Calibrate"]);
}

#[derive(Resource, Default)]
struct Received(Vec<(Entity, Entity, String)>);

#[test]
fn activating_a_button_sends_act_to_its_window() {
    let mut app = app();
    app.init_resource::<Received>();
    let world = app.world_mut();
    let window = world.spawn_scene(sample()).unwrap().id();
    world.flush();
    world
        .entity_mut(window)
        .observe(|act: On<UiAct>, mut got: ResMut<Received>| {
            got.0.push((act.scope, act.source, act.action.clone()));
        });
    let button = world
        .query_filtered::<Entity, With<Marker>>()
        .single(world)
        .unwrap();

    world.trigger(Activate { entity: button });
    world.flush();

    let got = &world.resource::<Received>().0;
    assert_eq!(got, &[(window, button, "regimeset".to_string())]);
}

#[test]
fn close_button_reports_and_despawns_window() {
    let mut app = app();
    app.init_resource::<Received>();
    let world = app.world_mut();
    let window = world.spawn_scene(sample()).unwrap().id();
    world.flush();
    world
        .entity_mut(window)
        .observe(|close: On<WindowClose>, mut got: ResMut<Received>| {
            got.0.push((close.entity, close.entity, "close".into()));
        });
    let close = world
        .query_filtered::<Entity, With<CloseButton>>()
        .single(world)
        .unwrap();

    world.trigger(Activate { entity: close });
    world.flush();

    assert_eq!(world.resource::<Received>().0.len(), 1);
    assert!(world.get_entity(window).is_err());
}

#[test]
fn newer_windows_open_on_top() {
    let mut app = app();
    let world = app.world_mut();
    let first = world.spawn_scene(sample()).unwrap().id();
    let second = world.spawn_scene(sample()).unwrap().id();
    world.flush();
    let z = |e| world.get::<GlobalZIndex>(e).unwrap().0;
    assert!(z(second) > z(first));
}

fn drag(world: &mut World, entity: Entity, delta: Vec2) {
    use bevy::camera::NormalizedRenderTarget;
    use bevy::picking::events::{Pointer, PointerDrag};
    use bevy::picking::pointer::{Location, PointerButton, PointerId};
    let location = Location {
        target: NormalizedRenderTarget::None {
            width: 1280,
            height: 720,
        },
        position: Vec2::ZERO,
    };
    world.trigger(PointerDrag {
        entity,
        pointer: Pointer::new(PointerId::Mouse, location),
        button: PointerButton::Primary,
        distance: delta,
        delta,
    });
    world.flush();
}

#[test]
fn dragging_title_bar_moves_and_grip_resizes() {
    let mut app = app();
    app.insert_resource(UiScale(2.0));
    let world = app.world_mut();
    let window = world.spawn_scene(sample()).unwrap().id();
    world.flush();
    let parts = children(world, window);
    let bar = only_with::<TitleBar>(world, &parts);
    let grip = only_with::<ss13_ui_kit::window::ResizeHandle>(world, &parts);

    // Screen pixels are divided by `UiScale` into logical px.
    drag(world, bar, Vec2::new(20.0, -10.0));
    let node = world.get::<Node>(window).unwrap();
    assert_eq!((node.left, node.top), (Val::Px(60.0), Val::Px(45.0)));

    drag(world, grip, Vec2::new(-2000.0, 40.0));
    let node = world.get::<Node>(window).unwrap();
    // Width is clamped to the minimum; height grows by 20 logical px.
    assert_eq!((node.width, node.height), (Val::Px(150.0), Val::Px(320.0)));
}
