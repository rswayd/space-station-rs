//! Scripted client: drives the real client from a script (see [`script`]) by injecting Bevy
//! input messages, and takes screenshots through Bevy's `Screenshot`, so visual checks never
//! touch OS input or screen capture.
//!
//! Enabled by `SS13_SCRIPT=<file>` ([`ScriptPlugin::from_env`]); without it nothing is added.
//!
//! - Keys: `KeyboardInput` messages, the same ones winit writes. `ButtonInput<KeyCode>` and
//!   `ButtonInput<Key>` are updated from them by `bevy_input`, focused text fields receive them
//!   through `bevy_input_focus` dispatch.
//! - Clicks: `PointerInput` for the mouse pointer, the same messages `bevy_picking` makes from
//!   winit cursor events, on the world camera's render target.
//! - `SS13_OFFSCREEN=1`: no winit, no OS window; the cameras render into an image of the window's
//!   size ([`OffscreenPlugin`]) and screenshots read that image.

pub mod script;

use std::path::PathBuf;
use std::time::Duration;

use bevy::app::{PluginGroupBuilder, ScheduleRunnerPlugin};
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::ecs::system::SystemParam;
use bevy::input::ButtonState;
use bevy::input::InputSystems;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::MouseScrollUnit;
use bevy::input::touch::TouchPhase;
use bevy::input_focus::InputFocus;
use bevy::picking::PickingSystems;
use bevy::picking::pointer::{Location, PointerAction, PointerButton, PointerId, PointerInput};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::text::EditableText;
use bevy::ui::{ComputedNode, UiGlobalTransform};
use bevy::window::PrimaryWindow;
use bevy::winit::{WinitPlugin, WinitSettings};
use ss13_chat::client::ChatLog;
use ss13_net::client::{NetClientState, NetEntityMap};
use ss13_render::sprites::tile_translation;
use ss13_render::{IconAtlas, WorldCamera};
use ss13_shared::TilePos;

pub use script::{Condition, KeyName, Line, Step};

/// Longest wait for pending screenshots before exiting.
const SCREENSHOT_TIMEOUT_SECS: f64 = 10.0;
/// The frame rate of the offscreen loop (no vsync without a window).
const OFFSCREEN_FPS: f64 = 60.0;

pub struct ScriptPlugin {
    pub lines: Vec<Line>,
    /// Screenshots go here.
    pub out_dir: PathBuf,
    pub offscreen: bool,
}

impl ScriptPlugin {
    /// `SS13_SCRIPT` (the script file), `SS13_SCRIPT_OUT` (screenshot directory, default `.`),
    /// `SS13_OFFSCREEN=1`. `None` without `SS13_SCRIPT`; an unreadable or invalid script exits
    /// the process with code 2 before the app starts.
    pub fn from_env() -> Option<Self> {
        let path = std::env::var_os("SS13_SCRIPT")?;
        let out_dir = std::env::var_os("SS13_SCRIPT_OUT").map_or_else(|| ".".into(), PathBuf::from);
        let offscreen = std::env::var("SS13_OFFSCREEN").is_ok_and(|v| v == "1");
        let fail = |e: String| -> ! {
            eprintln!("script {}: {e}", PathBuf::from(&path).display());
            std::process::exit(2)
        };
        let source = std::fs::read_to_string(&path).unwrap_or_else(|e| fail(e.to_string()));
        let lines = script::parse(&source).unwrap_or_else(|e| fail(e));
        std::fs::create_dir_all(&out_dir)
            .unwrap_or_else(|e| fail(format!("{}: {e}", out_dir.display())));
        Some(Self {
            lines,
            out_dir,
            offscreen,
        })
    }

    /// Offscreen: `DefaultPlugins` without winit, driven by a fixed-rate loop instead.
    pub fn adjust_defaults(&self, defaults: PluginGroupBuilder) -> PluginGroupBuilder {
        if !self.offscreen {
            return defaults;
        }
        defaults
            .disable::<WinitPlugin>()
            .add(ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(
                1.0 / OFFSCREEN_FPS,
            )))
    }
}

impl Plugin for ScriptPlugin {
    fn build(&self, app: &mut App) {
        if self.offscreen {
            app.add_plugins(OffscreenPlugin);
        } else {
            // The game default throttles unfocused windows; a script must not depend on focus.
            app.insert_resource(WinitSettings::continuous());
        }
        app.insert_resource(Runner::new(self.lines.clone(), self.out_dir.clone()))
            .add_systems(
                PreUpdate,
                run_script
                    .before(InputSystems)
                    .before(PickingSystems::ProcessInput),
            );
    }
}

/// Renders every camera into one image instead of a window.
pub struct OffscreenPlugin;

#[derive(Resource, Clone)]
pub struct OffscreenTarget(pub Handle<Image>);

impl Plugin for OffscreenPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, create_offscreen_target)
            .add_observer(retarget_camera);
    }
}

/// The image has the primary window's size, so layout and zoom match the windowed client.
fn create_offscreen_target(
    windows: Query<&Window, With<PrimaryWindow>>,
    mut images: ResMut<Assets<Image>>,
    mut commands: Commands,
) {
    let size = windows
        .single()
        .map_or(UVec2::new(1280, 720), Window::physical_size);
    let image = Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8UnormSrgb, None);
    commands.insert_resource(OffscreenTarget(images.add(image)));
}

/// Cameras that draw to the window draw to the offscreen image; those with their own image target
/// (the lighting's emissive mask) keep it.
fn retarget_camera(
    add: On<Add<Camera>>,
    target: Option<Res<OffscreenTarget>>,
    targets: Query<&RenderTarget>,
    mut commands: Commands,
) {
    if matches!(targets.get(add.entity), Ok(RenderTarget::Image(_))) {
        return;
    }
    if let Some(target) = target {
        commands
            .entity(add.entity)
            .insert(RenderTarget::Image(ImageRenderTarget {
                handle: target.0.clone(),
                scale_factor: 1.0,
            }));
    }
}

#[derive(Debug, PartialEq)]
enum Finish {
    Running,
    /// Waiting for pending screenshots, then exit with this code.
    Exiting {
        code: u8,
        since: f64,
    },
}

#[derive(Resource)]
pub struct Runner {
    lines: Vec<Line>,
    out_dir: PathBuf,
    pc: usize,
    /// When the current step started; `None` until it runs for the first time.
    started: Option<f64>,
    /// Frame of a multi-frame step (tap, click).
    phase: u32,
    click_at: Option<Location>,
    pending_screenshots: usize,
    finish: Finish,
    fps: f64,
}

impl Runner {
    fn new(lines: Vec<Line>, out_dir: PathBuf) -> Self {
        Self {
            lines,
            out_dir,
            pc: 0,
            started: None,
            phase: 0,
            click_at: None,
            pending_screenshots: 0,
            finish: Finish::Running,
            fps: 0.0,
        }
    }

    /// Exit code once the script has ended, for tests.
    pub fn exit_code(&self) -> Option<u8> {
        match self.finish {
            Finish::Running => None,
            Finish::Exiting { code, .. } => Some(code),
        }
    }
}

enum Outcome {
    /// Done; the next step may run in this frame.
    Next,
    /// Done; the next step runs next frame (input needs a frame to reach its readers).
    NextFrame,
    Wait,
    Fail(String),
    Quit,
}

/// What the script reads.
#[derive(SystemParam)]
struct Observed<'w, 's> {
    state: Res<'w, NetClientState>,
    map: Res<'w, NetEntityMap>,
    chat: Option<Res<'w, ChatLog>>,
    atlas: Option<Res<'w, IconAtlas>>,
    positions: Query<'w, 's, &'static TilePos>,
    cameras: Query<
        'w,
        's,
        (
            &'static Camera,
            &'static GlobalTransform,
            &'static RenderTarget,
        ),
        With<WorldCamera>,
    >,
    primary: Query<'w, 's, Entity, With<PrimaryWindow>>,
    nodes: Query<
        'w,
        's,
        (
            Option<&'static Text>,
            Option<&'static Name>,
            &'static ComputedNode,
            &'static UiGlobalTransform,
            &'static InheritedVisibility,
        ),
    >,
    sprites: Query<'w, 's, &'static ViewVisibility, With<Sprite>>,
    windows: Query<'w, 's, Entity, With<ss13_ui_kit::window::Window>>,
    children: Query<'w, 's, &'static Children>,
    texts: Query<'w, 's, &'static Text>,
    focus: Option<Res<'w, InputFocus>>,
    editable: Query<'w, 's, (), With<EditableText>>,
}

impl Observed<'_, '_> {
    fn own_pos(&self) -> Option<TilePos> {
        self.positions.get(self.map.controlled()?).ok().copied()
    }

    fn holds(&self, cond: &Condition) -> bool {
        match cond {
            Condition::Controlled => self.own_pos().is_some(),
            Condition::Entities(n) => self.map.len() >= *n,
            Condition::Chat(text) => self.chat.as_ref().is_some_and(|log| {
                log.entries()
                    .iter()
                    .any(|e| e.message.plain_text().contains(text.as_str()))
            }),
        }
    }

    fn window(&self) -> Entity {
        self.primary.single().unwrap_or(Entity::PLACEHOLDER)
    }

    /// The world camera's target: where clicks land and what screenshots capture.
    fn target(&self) -> RenderTarget {
        self.cameras.iter().next().map_or(
            RenderTarget::Window(bevy::window::WindowRef::Primary),
            |c| c.2.clone(),
        )
    }

    fn location(&self, position: Vec2) -> Result<Location, String> {
        let target = self
            .target()
            .normalize(self.primary.single().ok())
            .ok_or("no render target")?;
        Ok(Location { target, position })
    }

    /// The window point over icon pixel `pixel` (from the bottom-left corner) of `tile`.
    fn tile_location(&self, tile: TilePos, [px, py]: [i32; 2]) -> Result<Location, String> {
        let (camera, transform, _) = self.cameras.iter().next().ok_or("no world camera")?;
        let point = tile_translation(tile) + Vec2::new(px as f32 + 0.5, py as f32 + 0.5);
        let position = camera
            .world_to_viewport(transform, point.extend(0.0))
            .map_err(|e| format!("tile {tile:?} not on screen: {e:?}"))?;
        self.location(position)
    }

    fn ui_location(&self, needle: &str) -> Result<Location, String> {
        let mut found = self.nodes.iter().filter(|(text, name, node, _, visible)| {
            visible.get()
                && node.size.x > 0.0
                && (text.is_some_and(|t| t.0.contains(needle))
                    || name.is_some_and(|n| n.as_str() == needle))
        });
        let (_, _, node, transform, _) = found
            .next()
            .ok_or_else(|| format!("no visible UI node with text or name {needle:?}"))?;
        if found.next().is_some() {
            warn!("script: several UI nodes match {needle:?}, clicking the first");
        }
        self.location(transform.translation * node.inverse_scale_factor)
    }

    fn describe(&self, fps: f64) -> String {
        let pos = self
            .own_pos()
            .map_or("none".into(), |p| format!("({}, {}, {})", p.x, p.y, p.z));
        let visible = self.sprites.iter().filter(|v| v.get()).count();
        let windows: Vec<String> = self
            .windows
            .iter()
            .map(|w| {
                self.children
                    .iter_descendants(w)
                    .find_map(|e| self.texts.get(e).ok())
                    .map_or_else(|| "?".into(), |t| t.0.clone())
            })
            .collect();
        let typing = self
            .focus
            .as_ref()
            .and_then(|f| f.get())
            .is_some_and(|e| self.editable.contains(e));
        let chat: Vec<String> = self.chat.as_ref().map_or_else(Vec::new, |log| {
            let entries = log.entries();
            entries
                .range(entries.len().saturating_sub(5)..)
                .map(|e| e.message.plain_text())
                .collect()
        });
        let atlas = self.atlas.as_ref().map(|a| a.stats()).unwrap_or_default();
        format!(
            "connection={:?} own_tile={pos} replicated={} visible_sprites={visible} \
             atlas_pages={}/{} atlas_mib={} windows={windows:?} typing={typing} fps={fps:.0} \
             chat_last={chat:?}",
            *self.state,
            self.map.len(),
            atlas.loaded,
            atlas.pages,
            atlas.resident_bytes >> 20,
        )
    }
}

#[derive(SystemParam)]
struct Inject<'w> {
    keys: MessageWriter<'w, KeyboardInput>,
    pointer: MessageWriter<'w, PointerInput>,
    exit: MessageWriter<'w, AppExit>,
}

impl Inject<'_> {
    fn key(&mut self, key: &KeyName, state: ButtonState, window: Entity) {
        self.keys.write(KeyboardInput {
            key_code: key.code,
            logical_key: key.logical.clone(),
            state,
            text: key.text.as_deref().map(Into::into),
            repeat: false,
            window,
        });
    }

    fn pointer(&mut self, location: &Location, action: PointerAction) {
        self.pointer.write(PointerInput::new(
            PointerId::Mouse,
            location.clone(),
            action,
        ));
    }
}

fn run_script(
    mut runner: ResMut<Runner>,
    time: Res<Time<Real>>,
    observed: Observed,
    mut inject: Inject,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    let dt = time.delta_secs_f64();
    if dt > 0.0 {
        runner.fps = if runner.fps == 0.0 {
            1.0 / dt
        } else {
            runner.fps * 0.9 + 0.1 / dt
        };
    }
    if let Finish::Exiting { code, since } = runner.finish {
        if runner.pending_screenshots == 0 || now - since > SCREENSHOT_TIMEOUT_SECS {
            if runner.pending_screenshots > 0 {
                error!(
                    "script: {} screenshot(s) not captured",
                    runner.pending_screenshots
                );
            }
            inject.exit.write(AppExit::from_code(code));
        }
        return;
    }
    loop {
        let Some(line) = runner.lines.get(runner.pc).cloned() else {
            info!("script: end");
            runner.finish = Finish::Exiting {
                code: 0,
                since: now,
            };
            return;
        };
        let started = *runner.started.get_or_insert(now);
        let elapsed = now - started;
        let outcome = step(
            &mut runner,
            &line.step,
            elapsed,
            &observed,
            &mut inject,
            &mut commands,
        );
        match outcome {
            Outcome::Wait => return,
            Outcome::Next | Outcome::NextFrame => {
                runner.pc += 1;
                runner.started = None;
                runner.phase = 0;
                if matches!(outcome, Outcome::NextFrame) {
                    return;
                }
            }
            Outcome::Quit => {
                runner.finish = Finish::Exiting {
                    code: 0,
                    since: now,
                };
                return;
            }
            Outcome::Fail(reason) => {
                error!(
                    "script: FAILED at line {} `{}`: {reason}; state: {}",
                    line.number,
                    line.text,
                    observed.describe(runner.fps)
                );
                screenshot(&mut runner, &observed, "failure.png", &mut commands);
                runner.finish = Finish::Exiting {
                    code: 1,
                    since: now,
                };
                return;
            }
        }
    }
}

fn step(
    runner: &mut Runner,
    step: &Step,
    elapsed: f64,
    observed: &Observed,
    inject: &mut Inject,
    commands: &mut Commands,
) -> Outcome {
    let window = observed.window();
    let timed_out = |timeout: f64, what: &str| {
        if elapsed > timeout {
            Outcome::Fail(format!("timed out after {timeout} s waiting for {what}"))
        } else {
            Outcome::Wait
        }
    };
    match step {
        Step::WaitConnected { timeout } => {
            if *observed.state == NetClientState::Connected {
                Outcome::Next
            } else {
                timed_out(*timeout, &format!("connection ({:?})", *observed.state))
            }
        }
        Step::WaitSecs(secs) => {
            if elapsed >= *secs {
                Outcome::Next
            } else {
                Outcome::Wait
            }
        }
        Step::WaitUntil { cond, timeout } => {
            if observed.holds(cond) {
                Outcome::Next
            } else {
                timed_out(*timeout, &format!("{cond:?}"))
            }
        }
        Step::KeyDown(key) => {
            inject.key(key, ButtonState::Pressed, window);
            Outcome::NextFrame
        }
        Step::KeyUp(key) => {
            inject.key(key, ButtonState::Released, window);
            Outcome::NextFrame
        }
        Step::Tap(key) => {
            let state = if runner.phase == 0 {
                ButtonState::Pressed
            } else {
                ButtonState::Released
            };
            inject.key(key, state, window);
            runner.phase += 1;
            if runner.phase == 2 {
                Outcome::NextFrame
            } else {
                Outcome::Wait
            }
        }
        Step::Type(text) => {
            for c in text.chars() {
                let key = KeyName::for_char(c);
                inject.key(&key, ButtonState::Pressed, window);
                inject.key(&key, ButtonState::Released, window);
            }
            Outcome::NextFrame
        }
        Step::ClickWorld { .. }
        | Step::ClickTile { .. }
        | Step::RightClickTile { .. }
        | Step::ClickUi(_) => click(runner, step, observed, inject),
        Step::DragTile { from, to } => drag(runner, *from, *to, observed, inject),
        Step::ScrollUi { needle, lines } => {
            // Move first, so the wheel goes to the hovered node.
            let location = match observed.ui_location(needle) {
                Ok(location) => location,
                Err(e) => return Outcome::Fail(e),
            };
            let action = if runner.phase == 0 {
                PointerAction::Move { delta: Vec2::ZERO }
            } else {
                PointerAction::Scroll {
                    unit: MouseScrollUnit::Line,
                    x: 0.0,
                    y: -(*lines as f32),
                    phase: TouchPhase::Moved,
                }
            };
            inject.pointer(&location, action);
            runner.phase += 1;
            if runner.phase == 2 {
                Outcome::NextFrame
            } else {
                Outcome::Wait
            }
        }
        Step::Screenshot(name) => {
            // Pages load on demand: wait for the ones requested so far, or sprites are missing.
            let loading = observed.atlas.as_ref().map_or(0, |a| a.stats().loading);
            if runner.phase == 0 && loading > 0 {
                return timed_out(SCREENSHOT_TIMEOUT_SECS, "atlas pages");
            }
            if runner.phase == 0 {
                screenshot(runner, observed, name, commands);
                runner.phase = 1;
            }
            if runner.pending_screenshots == 0 {
                Outcome::Next
            } else {
                timed_out(SCREENSHOT_TIMEOUT_SECS, &format!("screenshot {name}"))
            }
        }
        Step::LogState => {
            info!("script: state {}", observed.describe(runner.fps));
            Outcome::Next
        }
        Step::Quit => {
            info!("script: quit");
            Outcome::Quit
        }
    }
}

/// Move, press, release: one frame each, so hover is known before the press.
fn click(runner: &mut Runner, step: &Step, observed: &Observed, inject: &mut Inject) -> Outcome {
    if runner.phase == 0 {
        let location = match step {
            Step::ClickUi(needle) => observed.ui_location(needle),
            Step::ClickTile { dx, dy, pixel } => observed
                .own_pos()
                .ok_or_else(|| "no controlled mob".to_string())
                .and_then(|p| {
                    let tile = TilePos::new(p.x + dx, p.y + dy, p.z);
                    observed.tile_location(tile, *pixel)
                }),
            Step::RightClickTile { dx, dy } => observed
                .own_pos()
                .ok_or_else(|| "no controlled mob".to_string())
                .and_then(|p| {
                    observed.tile_location(TilePos::new(p.x + dx, p.y + dy, p.z), [16, 16])
                }),
            Step::ClickWorld { x, y, pixel } => observed
                .own_pos()
                .ok_or_else(|| "no controlled mob".to_string())
                .and_then(|p| observed.tile_location(TilePos::new(*x, *y, p.z), *pixel)),
            _ => unreachable!("click() gets click steps only"),
        };
        match location {
            Ok(location) => runner.click_at = Some(location),
            Err(e) => return Outcome::Fail(e),
        }
    }
    let Some(location) = runner.click_at.clone() else {
        return Outcome::Fail("click lost its location".into());
    };
    let button = if matches!(step, Step::RightClickTile { .. }) {
        PointerButton::Secondary
    } else {
        PointerButton::Primary
    };
    let action = match runner.phase {
        0 => PointerAction::Move { delta: Vec2::ZERO },
        1 => PointerAction::Press(button),
        _ => PointerAction::Release(button),
    };
    inject.pointer(&location, action);
    runner.phase += 1;
    if runner.phase == 3 {
        runner.click_at = None;
        Outcome::NextFrame
    } else {
        Outcome::Wait
    }
}

/// Move to `from`, press, move to `to` (two frames: hover follows the cursor), release; one
/// frame each.
fn drag(
    runner: &mut Runner,
    from: [i32; 2],
    to: [i32; 2],
    observed: &Observed,
    inject: &mut Inject,
) -> Outcome {
    let tile = |d: [i32; 2]| {
        observed
            .own_pos()
            .ok_or_else(|| "no controlled mob".to_string())
            .and_then(|p| {
                observed.tile_location(TilePos::new(p.x + d[0], p.y + d[1], p.z), [16, 16])
            })
    };
    let (start, end) = match (tile(from), tile(to)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => return Outcome::Fail(e),
    };
    let (location, action) = match runner.phase {
        0 => (&start, PointerAction::Move { delta: Vec2::ZERO }),
        1 => (&start, PointerAction::Press(PointerButton::Primary)),
        2 | 3 => (&end, PointerAction::Move { delta: Vec2::ZERO }),
        _ => (&end, PointerAction::Release(PointerButton::Primary)),
    };
    inject.pointer(location, action);
    runner.phase += 1;
    if runner.phase == 5 {
        Outcome::NextFrame
    } else {
        Outcome::Wait
    }
}

fn screenshot(runner: &mut Runner, observed: &Observed, name: &str, commands: &mut Commands) {
    let path = runner.out_dir.join(name);
    info!("script: screenshot {}", path.display());
    runner.pending_screenshots += 1;
    commands
        .spawn(Screenshot(observed.target()))
        .observe(save_to_disk(path))
        .observe(|_: On<ScreenshotCaptured>, mut runner: ResMut<Runner>| {
            runner.pending_screenshots = runner.pending_screenshots.saturating_sub(1);
        });
}
