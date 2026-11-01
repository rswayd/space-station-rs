use std::path::{Path, PathBuf};
use std::time::Instant;

use bevy::prelude::*;
use ss13_proto::{
    ErrorKind, LoadError, ProtoAppExt, ProtoCommandsExt, ProtoComponents, ProtoError, ProtoPlugin,
    ProtoWorldExt, Prototypes,
};

// Dummy components: real game components live in game crates, not here.

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
struct Name(String);

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
struct Density(bool);

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
struct Anchored;

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
struct Door {
    open: bool,
    open_time: f32,
    access: Vec<String>,
}

#[derive(Reflect, Default, Clone, Copy, Debug, PartialEq)]
enum Facing {
    North,
    #[default]
    South,
    East,
    West,
}

#[derive(Reflect, Default, Clone, Copy, Debug, PartialEq)]
struct Offset {
    x: i32,
    y: i32,
}

#[derive(Component, Reflect, Default, Clone, Debug, PartialEq)]
struct Icon {
    file: String,
    state: String,
    layer: f32,
    dir: Facing,
    tint: Option<(u8, u8, u8, u8)>,
    offset: Offset,
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/station")
}

fn app() -> App {
    let mut app = App::new();
    app.register_proto_component::<Name>("name")
        .register_proto_component::<Density>("density")
        .register_proto_component::<Anchored>("anchored")
        .register_proto_component::<Door>("door")
        .register_proto_component::<Icon>("icon");
    app
}

fn load_fixtures(app: &App) -> Prototypes {
    let world = app.world();
    Prototypes::load_dir(
        &fixtures(),
        world.resource::<ProtoComponents>(),
        world.resource::<AppTypeRegistry>(),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

fn load_str(files: &[(&str, &str)]) -> Result<Prototypes, LoadError> {
    let app = app();
    let world = app.world();
    let sources: Vec<(PathBuf, String)> = files
        .iter()
        .map(|(p, t)| (PathBuf::from(p), t.to_string()))
        .collect();
    Prototypes::from_sources(
        &sources,
        world.resource::<ProtoComponents>(),
        world.resource::<AppTypeRegistry>(),
    )
}

fn single_error(files: &[(&str, &str)]) -> ProtoError {
    let mut errors = match load_str(files) {
        Ok(_) => panic!("expected a load error"),
        Err(LoadError(errors)) => errors,
    };
    assert_eq!(errors.len(), 1, "{errors:#?}");
    errors.remove(0)
}

#[test]
fn loads_directory_tree() {
    let protos = load_fixtures(&app());
    let mut ids: Vec<_> = protos.ids().collect();
    ids.sort_unstable();
    assert_eq!(
        ids,
        [
            "airlock",
            "airlock_engineering",
            "atom",
            "door",
            "item",
            "machinery",
            "obj",
            "toolbox"
        ]
    );
    let airlock = protos.get("airlock").unwrap();
    assert_eq!(airlock.parent(), Some("door"));
    assert!(airlock.file().unwrap().ends_with("doors/airlocks.ron"));
}

#[test]
fn multi_level_inheritance() {
    let protos = load_fixtures(&app());
    // atom -> obj -> machinery -> door -> airlock -> airlock_engineering
    let eng = protos.get("airlock_engineering").unwrap();
    assert_eq!(eng.get::<Name>(), Some(&Name("engineering airlock".into())));
    assert_eq!(eng.get::<Density>(), Some(&Density(true)), "from door");
    assert_eq!(eng.get::<Anchored>(), Some(&Anchored), "from machinery");
    assert_eq!(
        eng.get::<Icon>(),
        Some(&Icon {
            file: "icons/obj/doors/airlock.dmi".into(), // airlock
            state: "closed".into(),                     // door
            layer: 3.0,                                 // obj
            dir: Facing::East,                          // door
            tint: Some((255, 200, 0, 255)),             // airlock_engineering
            offset: Offset { x: 0, y: 4 },              // airlock_engineering
        })
    );
    // Siblings are independent: item gets nothing from machinery.
    let toolbox = protos.get("toolbox").unwrap();
    assert_eq!(toolbox.get::<Anchored>(), None);
    assert_eq!(toolbox.get::<Icon>().unwrap().layer, 3.1);
    assert_eq!(toolbox.get::<Icon>().unwrap().state, "red");
}

#[test]
fn field_level_override() {
    let protos = load_fixtures(&app());
    let door = protos.get("door").unwrap().get::<Door>().unwrap();
    let airlock = protos.get("airlock").unwrap().get::<Door>().unwrap();
    assert_eq!(door.open_time, 1.0);
    assert_eq!(airlock.open_time, 0.5, "overridden");
    assert!(!airlock.open, "inherited");
    assert_eq!(airlock.access, ["general"], "inherited");
    // A listed field replaces the whole value: lists are not merged.
    let eng = protos
        .get("airlock_engineering")
        .unwrap()
        .get::<Door>()
        .unwrap();
    assert_eq!(eng.access, ["engineering", "atmos"]);
    assert_eq!(eng.open_time, 0.5);
    // Parents are not affected by children.
    assert_eq!(
        protos.get("atom").unwrap().get::<Icon>().unwrap().file,
        "icons/misc.dmi"
    );
}

#[test]
fn per_instance_override() {
    let mut app = app();
    let protos = load_fixtures(&app);
    let inst = protos
        .instance(
            "airlock",
            Some(r#"{"door": (open: true, access: []), "density": (false), "icon": (dir: North)}"#),
        )
        .unwrap();
    let door = inst.get::<Door>().unwrap();
    assert!(door.open);
    assert!(door.access.is_empty());
    assert_eq!(door.open_time, 0.5, "kept from the prototype");
    assert_eq!(inst.get::<Density>(), Some(&Density(false)));
    let icon = inst.get::<Icon>().unwrap();
    assert_eq!(icon.dir, Facing::North);
    assert_eq!(icon.file, "icons/obj/doors/airlock.dmi");
    // The shared prototype is untouched.
    assert!(!protos.get("airlock").unwrap().get::<Door>().unwrap().open);

    app.insert_resource(protos);
    let world = app.world_mut();
    let e = world
        .spawn_proto("airlock", Some(r#"{"name": ("custom")}"#), Density(false))
        .unwrap();
    let e = world.entity(e);
    assert_eq!(e.get::<Name>(), Some(&Name("custom".into())));
    assert_eq!(
        e.get::<Density>(),
        Some(&Density(false)),
        "extra bundle wins"
    );
    assert_eq!(e.get::<Door>().unwrap().open_time, 0.5);
    assert!(e.contains::<Anchored>());
}

#[test]
fn spawn_through_commands() {
    #[derive(Resource)]
    struct Spawned(Vec<Entity>);

    let mut app = app();
    let protos = load_fixtures(&app);
    app.insert_resource(protos);
    app.add_systems(Update, |mut commands: Commands| {
        let a = commands.spawn_proto("toolbox", None, ());
        let b = commands.spawn_proto("door", Some(r#"{"door": (open: true)}"#.into()), ());
        commands.insert_resource(Spawned(vec![a, b]));
    });
    app.update();
    let world = app.world();
    let spawned = &world.resource::<Spawned>().0;
    assert_eq!(world.get::<Name>(spawned[0]), Some(&Name("toolbox".into())));
    assert!(world.get::<Door>(spawned[1]).unwrap().open);
}

#[test]
fn plugin_loads_configured_dir() {
    let mut app = app();
    app.add_plugins(ProtoPlugin { dir: fixtures() });
    app.finish();
    assert_eq!(app.world().resource::<Prototypes>().len(), 8);
}

// --- validation: one test per error kind ---

const DOOR: &str = r#"[(id: "door", components: {"door": (open_time: 1.0)})]"#;

#[test]
fn error_syntax() {
    let e = single_error(&[("bad.ron", "[(id: \"a\",\n  components: {")]);
    assert!(matches!(e.kind, ErrorKind::Syntax(_)), "{e}");
    assert_eq!(e.file.as_deref(), Some(Path::new("bad.ron")));
    assert!(e.pos.is_some());
    // Unknown top-level keys are syntax errors too, so typos do not pass silently.
    let e = single_error(&[("bad.ron", r#"[(id: "a", parnet: "b")]"#)]);
    assert!(matches!(e.kind, ErrorKind::Syntax(_)), "{e}");
}

#[test]
fn error_duplicate_id() {
    let e = single_error(&[("a.ron", DOOR), ("b.ron", DOOR)]);
    assert!(
        matches!(&e.kind, ErrorKind::DuplicateId(first) if first == Path::new("a.ron")),
        "{e}"
    );
    assert_eq!(e.file.as_deref(), Some(Path::new("b.ron")));
    assert_eq!(e.proto.as_deref(), Some("door"));
}

#[test]
fn error_unknown_parent() {
    let e = single_error(&[("a.ron", r#"[(id: "a", parent: "nope")]"#)]);
    assert!(
        matches!(&e.kind, ErrorKind::UnknownParent(p) if p == "nope"),
        "{e}"
    );
    assert_eq!(e.proto.as_deref(), Some("a"));
}

#[test]
fn error_cycle() {
    let e = single_error(&[(
        "a.ron",
        r#"[(id: "a", parent: "c"), (id: "b", parent: "a"), (id: "c", parent: "b"), (id: "d", parent: "c")]"#,
    )]);
    assert!(
        matches!(&e.kind, ErrorKind::Cycle(chain) if chain == &["a", "c", "b", "a"]),
        "{e}"
    );
    let e = single_error(&[("a.ron", r#"[(id: "self", parent: "self")]"#)]);
    assert!(
        matches!(&e.kind, ErrorKind::Cycle(chain) if chain.len() == 2),
        "{e}"
    );
}

#[test]
fn error_unknown_component() {
    let e = single_error(&[("a.ron", r#"[(id: "a", components: {"dencity": (true)})]"#)]);
    assert!(matches!(e.kind, ErrorKind::UnknownComponent), "{e}");
    assert_eq!(e.component.as_deref(), Some("dencity"));
}

#[test]
fn error_unknown_field_points_to_file_line() {
    let text = "[\n  (\n    id: \"a\",\n    components: {\n      \"door\": (open: true,\n        opne_time: 2.0),\n    },\n  ),\n]";
    let e = single_error(&[("doors.ron", text)]);
    assert!(
        matches!(&e.kind, ErrorKind::BadValue(m) if m.contains("opne_time")),
        "{e}"
    );
    assert_eq!(e.proto.as_deref(), Some("a"));
    assert_eq!(e.component.as_deref(), Some("door"));
    assert_eq!(e.pos.map(|p| p.0), Some(6), "{e}");
    let msg = e.to_string();
    assert!(
        msg.starts_with("doors.ron:6:") && msg.contains("prototype `a`, component `door`"),
        "{msg}"
    );
}

#[test]
fn error_wrong_type() {
    let e = single_error(&[(
        "a.ron",
        r#"[(id: "a", components: {"door": (open: "yes")})]"#,
    )]);
    assert!(matches!(e.kind, ErrorKind::BadValue(_)), "{e}");
    let e = single_error(&[("a.ron", r#"[(id: "a", components: {"icon": (dir: Up)})]"#)]);
    assert!(matches!(e.kind, ErrorKind::BadValue(_)), "{e}");
}

#[test]
fn error_incomplete_nested_value() {
    let e = single_error(&[(
        "a.ron",
        r#"[(id: "a", components: {"icon": (offset: (x: 1))})]"#,
    )]);
    assert!(
        matches!(&e.kind, ErrorKind::Incomplete(Some(f)) if f == "offset"),
        "{e}"
    );
}

#[test]
fn errors_are_collected_and_children_of_broken_parents_are_quiet() {
    let errors = match load_str(&[
        (
            "a.ron",
            r#"[(id: "a", components: {"x": ()}), (id: "b", parent: "a")]"#,
        ),
        ("b.ron", r#"[(id: "c", parent: "missing")]"#),
    ]) {
        Ok(_) => panic!("expected errors"),
        Err(LoadError(e)) => e,
    };
    assert_eq!(errors.len(), 2, "{errors:#?}");
}

#[test]
fn error_bad_instance_override() {
    let protos = load_fixtures(&app());
    let err = |id, o| {
        protos
            .instance(id, o)
            .err()
            .expect("expected an error")
            .kind
    };
    assert!(matches!(err("nope", None), ErrorKind::UnknownPrototype));
    assert!(matches!(err("door", Some("{")), ErrorKind::Syntax(_)));
    assert!(matches!(
        err("door", Some(r#"{"lock": ()}"#)),
        ErrorKind::UnknownComponent
    ));
    assert!(matches!(
        err("door", Some(r#"{"door": (shut: true)}"#)),
        ErrorKind::BadValue(_)
    ));
}

/// Acceptance: spawn 100k entities from prototypes, report the time.
/// `cargo test -p ss13_proto --release spawn_100k -- --nocapture` for release numbers.
#[test]
fn spawn_100k() {
    const N: usize = 100_000;
    let mut app = app();
    let protos = load_fixtures(&app);
    let kinds: Vec<_> = ["airlock_engineering", "toolbox", "machinery", "door"]
        .iter()
        .map(|id| protos.get(id).unwrap().clone())
        .collect();
    app.insert_resource(protos);
    let world = app.world_mut();

    let start = Instant::now();
    for i in 0..N {
        kinds[i % kinds.len()].spawn(world, ());
    }
    let resolved = start.elapsed();

    let start = Instant::now();
    for i in 0..N {
        let id = ["airlock_engineering", "toolbox", "machinery", "door"][i % 4];
        world.spawn_proto(id, None, ()).unwrap();
    }
    let by_id = start.elapsed();

    assert_eq!(world.query::<&Name>().iter(world).count(), 2 * N);
    println!(
        "spawn {N}: resolved prototype {resolved:?} ({:.0} ns/entity), by id {by_id:?} ({:.0} ns/entity)",
        resolved.as_nanos() as f64 / N as f64,
        by_id.as_nanos() as f64 / N as f64,
    );
}
