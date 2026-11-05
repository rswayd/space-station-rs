//! Client half: keyboard movement (predicted, see [`crate::prediction`]), mouse clicks on the
//! world, camera on the controlled mob, gliding between tiles.
//!
//! Needs `ss13_net::client::NetClientPlugin` and `ss13_render::RenderPlugin`.

use bevy::camera::visibility::RenderLayers;
use bevy::input_focus::InputFocus;
use bevy::picking::hover::HoverMap;
use bevy::picking::pointer::{PointerAction, PointerButton, PointerId, PointerInput};
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::text::EditableText;
use bevy::ui::IsDefaultUiCamera;
use ss13_net::client::{LocalControl, NetClientConfig, NetEntity, NetEntityMap, SendIntent};
use ss13_render::sprites::{sync_tile_pos, tile_translation};
use ss13_render::{
    ActiveZ, AlphaMask, CameraTarget, IconAtlas, IconSprite, RenderSystems, WorldCamera,
};
use ss13_shared::protocol::{ClickModifiers, ClientIntent, DragEnd, InventorySlot, NetId};
use ss13_shared::{Appearance, Dir, MouseOpacity, TILE_SIZE, TilePos};

use crate::RUN_DELAY_SECS;
use crate::prediction::{MoveInput, PredictionPlugin, PredictionSystems};

/// Moves longer than this (teleports, a lost motion repaired by a settle) snap instead of gliding.
const MAX_GLIDE_PX: f32 = 2.0 * TILE_SIZE as f32;

pub struct GameClientPlugin;

impl Plugin for GameClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(crate::keycard_client::KeycardClientPlugin);
        app.add_plugins(crate::cargo::bounty_client::BountyClientPlugin);
        app.add_plugins(crate::defenses::client::DefensesClientPlugin);
        app.add_plugins(crate::assemblies::client::AssembliesClientPlugin);
        app.add_plugins(crate::wiremod::client::WiremodClientPlugin);
        app.add_plugins(crate::operating_client::OperatingClientPlugin);
        app.add_plugins(crate::forensics::client::ForensicsClientPlugin);
        app.add_plugins(crate::service::client::ServiceClientPlugin);
        app.add_plugins(crate::brig::client::BrigClientPlugin);
        app.add_plugins(crate::mining::client::MiningClientPlugin);
        app.add_plugins(crate::animation_client::AnimationClientPlugin);
        app.add_plugins(crate::language_menu_client::LanguageMenuClientPlugin);
        app.add_plugins((
            PredictionPlugin,
            crate::ui_sprites::UiSpritesPlugin,
            crate::lobby_client::LobbyClientPlugin,
            crate::comms_client::CommsClientPlugin,
            crate::cargo::client::CargoClientPlugin,
            crate::antag::client::AntagClientPlugin,
            crate::modpc::client::ModpcClientPlugin,
            crate::records::client::RecordsClientPlugin,
            crate::science::client::ScienceClientPlugin,
            crate::silicon::client::SiliconClientPlugin,
            crate::panels_client::PanelsClientPlugin,
            crate::mecha::client::MechaClientPlugin,
            crate::actions_client::ActionHudPlugin,
            crate::antag::vampire::client::VampireClientPlugin,
            crate::antag::changeling::client::ChangelingClientPlugin,
        ))
        .add_plugins(crate::antag::nukeop::client::NukeopClientPlugin)
        .add_plugins(crate::shuttle_console_client::ShuttleConsoleClientPlugin)
        .add_plugins(crate::bots::client::BotsClientPlugin)
        .add_plugins(crate::antag::cult::client::CultClientPlugin)
        .add_plugins(crate::antag::revolution::client::RevolutionClientPlugin)
        .add_plugins(crate::antag::heretic::client::HereticClientPlugin)
        .add_plugins(crate::antag::wizard::client::WizardClientPlugin)
        .add_plugins(crate::modsuit::client::ModSuitClientPlugin)
        .add_plugins(crate::clothing::client::ClothingClientPlugin)
        .add_systems(PreStartup, dev_cert_fallback)
        .add_systems(Startup, spawn_ui_camera)
        .add_observer(follow_controlled)
        .add_observer(unfollow_controlled)
        .add_systems(
            Update,
            (read_move_input.before(PredictionSystems), send_clicks),
        )
        .add_systems(
            PostUpdate,
            (
                start_glides.before(sync_tile_pos),
                apply_glides
                    .after(sync_tile_pos)
                    .before(ss13_render::camera::follow_target),
                apply_camera_effects.before(ss13_render::camera::follow_target),
            )
                .in_set(RenderSystems),
        );
    }
}

/// Dev only: a local server's digest from [`crate::dev_cert_digest_file`] when `SS13_CERT_DIGEST`
/// is unset. Runs before `NetClientPlugin` connects in `Startup`.
/// A loopback (in-memory) client has no certificate to pin, and no filesystem in the browser.
fn dev_cert_fallback(
    mut config: ResMut<NetClientConfig>,
    loopback: Option<Res<ss13_net::loopback::LoopbackConnector>>,
) {
    if loopback.is_some()
        || !config.certificate_digest.is_empty()
        || !config.server_addr.ip().is_loopback()
    {
        return;
    }
    let read = crate::dev_cert_digest_file(config.server_addr.port())
        .and_then(|path| crate::read_dev_cert_digest(&path).map(|digest| (path, digest)));
    match read {
        Ok((path, digest)) => {
            warn!(
                "dev: SS13_CERT_DIGEST unset, using the local server's digest from {}",
                path.display()
            );
            config.certificate_digest = digest.trim().to_owned();
        }
        Err(e) => warn!("dev: SS13_CERT_DIGEST unset and no usable digest file: {e}"),
    }
}

fn follow_controlled(add: On<Add<LocalControl>>, mut commands: Commands) {
    commands.entity(add.entity).try_insert(CameraTarget);
}

fn unfollow_controlled(remove: On<Remove<LocalControl>>, mut commands: Commands) {
    commands.entity(remove.entity).try_remove::<CameraTarget>();
}

/// UI gets its own full-window camera: on the world camera it would be laid out inside the map
/// viewport. It draws no sprites and keeps the world it is drawn over; `IsDefaultUiCamera` keeps
/// UI on it even if another camera is added later.
fn spawn_ui_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        RenderLayers::none(),
        IsDefaultUiCamera,
    ));
}

/// Arrows or WASD; two perpendicular keys make a diagonal, opposite keys cancel.
pub fn held_dir(keys: &ButtonInput<KeyCode>) -> Option<Dir> {
    let any = |a, b| keys.pressed(a) || keys.pressed(b);
    let mut bits = 0u8;
    for (pressed, bit) in [
        (any(KeyCode::KeyW, KeyCode::ArrowUp), Dir::North),
        (any(KeyCode::KeyS, KeyCode::ArrowDown), Dir::South),
        (any(KeyCode::KeyD, KeyCode::ArrowRight), Dir::East),
        (any(KeyCode::KeyA, KeyCode::ArrowLeft), Dir::West),
    ] {
        if pressed {
            bits |= bit as u8;
        }
    }
    // Opposite pairs cancel.
    if bits & 3 == 3 {
        bits &= !3;
    }
    if bits & 12 == 12 {
        bits &= !12;
    }
    Dir::from_bits(bits)
}

/// The held direction, none while typing in a text field.
fn read_move_input(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Option<Res<InputFocus>>,
    text_fields: Query<(), With<EditableText>>,
    mut input: ResMut<MoveInput>,
) {
    let typing = focus
        .and_then(|f| f.get())
        .is_some_and(|e| text_fields.contains(e));
    input.set_if_neq(MoveInput(held_dir(&keys).filter(|_| !typing)));
}

/// The tile under a world-space point (sprites sit with their bottom-left corner on
/// [`tile_translation`]).
pub fn tile_at(world: Vec2, z: u16) -> TilePos {
    let t = (world / TILE_SIZE as f32).floor();
    TilePos::new(t.x as i32, t.y as i32, z)
}

/// The cursor's offset from the center of `pos`, pixels (tg `ICON_X - 16`, `ICON_Y - 16`);
/// `None` off the world.
pub fn click_pixel(world: Vec2, pos: TilePos) -> Option<[i8; 2]> {
    let half = TILE_SIZE as f32 / 2.0;
    let d = world - tile_translation(pos) - Vec2::splat(half);
    d.is_finite().then(|| {
        [
            d.x.round().clamp(-128.0, 127.0) as i8,
            d.y.round().clamp(-128.0, 127.0) as i8,
        ]
    })
}

/// One drawn sprite of a clickable atom: its base icon or one of its overlays (BYOND counts
/// overlays as part of the atom).
#[derive(Clone, Copy, Debug)]
pub struct DrawnSprite<'a> {
    pub owner: NetId,
    /// World z: plane, layer, overlay order (`ss13_render::order`).
    pub z: f32,
    /// Where it is drawn: tile, glide, rotation; pixel offsets are in `anchor`.
    pub transform: GlobalTransform,
    /// The frame's rect in its atlas page (current dir and animation frame).
    pub rect: Rect,
    /// Bevy `Anchor`.
    pub anchor: Vec2,
    /// `Some`: clicked only where this page mask is drawn (`MOUSE_OPACITY_ICON`). `None`: the
    /// whole rect (`MOUSE_OPACITY_OPAQUE`, or the magenta tile of a missing state).
    pub mask: Option<&'a AlphaMask>,
}

/// The pixel of a `size` sprite under the world `point`: column from the left, row from the top.
pub fn pixel_under(
    point: Vec2,
    transform: &GlobalTransform,
    size: Vec2,
    anchor: Vec2,
) -> Option<UVec2> {
    let local = transform
        .affine()
        .inverse()
        .transform_point3(point.extend(0.0))
        .xy();
    // Bevy draws a sprite centered on `-anchor * size`.
    let uv = local / size + anchor + Vec2::splat(0.5);
    if !(0.0..1.0).contains(&uv.x) || !(0.0..1.0).contains(&uv.y) {
        return None;
    }
    let px = (uv * size).floor();
    Some(UVec2::new(px.x as u32, (size.y - 1.0 - px.y) as u32))
}

/// BYOND clicks the topmost atom drawn under the cursor, whichever tile it stands on: with
/// `mouse_opacity = MOUSE_OPACITY_ICON` (the default) only where a pixel of its icon or of an
/// overlay is not fully transparent, in the frame shown now, with its pixel offset, facing and
/// turn; `MOUSE_OPACITY_OPAQUE` atoms anywhere in their icons. Transparent atoms (gas, fire,
/// effects) are left out by the caller. Topmost = the highest z of the sprite hit.
pub fn topmost_at<'a>(
    point: Vec2,
    drawn: impl IntoIterator<Item = DrawnSprite<'a>>,
) -> Option<NetId> {
    drawn
        .into_iter()
        .filter(|s| {
            let Some(px) = pixel_under(point, &s.transform, s.rect.size(), s.anchor) else {
                return false;
            };
            s.mask.is_none_or(|mask| {
                let at = s.rect.min.as_uvec2() + px;
                mask.opaque(at.x, at.y)
            })
        })
        .max_by(|a, b| a.z.total_cmp(&b.z))
        .map(|s| s.owner)
}

type SpriteItem<'a> = (
    Entity,
    &'a IconSprite,
    &'a Sprite,
    &'a Anchor,
    &'a GlobalTransform,
    &'a InheritedVisibility,
    Option<&'a ChildOf>,
    Option<&'a RenderLayers>,
);

/// tg `LENIENCY_DISTANCE` (16 screen pixels of BYOND's map, icon pixels here) and
/// `LENIENCY_TIME` (0.1 s): a drag released this close to where it started, this soon, is a
/// click on what was pressed (`check_drag_proximity`).
pub const DRAG_LENIENCY_PX: f32 = 16.0;
pub const DRAG_LENIENCY_SECS: f64 = 0.1;

/// What the mouse is over, for clicks and drags: a world atom, or a HUD inventory slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseTarget {
    Atom(NetId),
    Slot(InventorySlot),
}

impl MouseTarget {
    fn drag_end(self) -> DragEnd {
        match self {
            Self::Atom(id) => DragEnd::Atom(id),
            Self::Slot(slot) => DragEnd::Slot(slot),
        }
    }
}

/// A primary press waiting for its release: what it was on, where (world, icon pixels) and when.
#[derive(Clone, Copy, Debug)]
pub struct Press {
    pub target: MouseTarget,
    pub at: Vec2,
    pub time: f64,
}

/// What a primary release does, BYOND `Click` / `MouseDrop` and tg `check_drag_proximity`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Release {
    Click(NetId),
    Drag(MouseTarget, MouseTarget),
    /// A HUD slot clicked (its own `PointerClick` handles it), or released over plain UI.
    Nothing,
}

/// `over`: what the release is over (`None`: UI that is not a slot); `own`: the client's mob.
/// A drag onto the own mob is always a drag (tg: "dragging them onto you").
// shortcut: tg also clicks the target instead when a turf is dragged a little onto something
// else; the client does not know which atoms are turfs, so the pressed atom is clicked.
pub fn on_release(
    press: Press,
    over: Option<MouseTarget>,
    at: Vec2,
    now: f64,
    own: Option<NetId>,
) -> Release {
    let Some(over) = over else {
        return Release::Nothing;
    };
    match press.target {
        MouseTarget::Atom(id) if over == press.target => Release::Click(id),
        MouseTarget::Atom(id)
            if now - press.time <= DRAG_LENIENCY_SECS
                && press.at.distance(at) <= DRAG_LENIENCY_PX
                && over != MouseTarget::Atom(own.unwrap_or(NetId(u64::MAX))) =>
        {
            Release::Click(id)
        }
        MouseTarget::Slot(_) if over == press.target => Release::Nothing,
        _ => Release::Drag(press.target, over),
    }
}

/// Left (middle, right) click on the world -> `Click` on the topmost replicated entity under the
/// cursor ([`topmost_at`]). The left button acts on release, as BYOND does: released over what
/// it was pressed on it clicks, over something else (a world atom or a HUD slot, also when
/// pressed on a HUD slot) it drags (`ClientIntent::Drag`, [`on_release`]). Clicks over UI nodes
/// are the UI's. Reads `bevy_picking`'s pointer input, not the raw mouse, so injected input
/// (`ss13_devtools` scripts) clicks the world too.
// Cost: one pass over the drawn sprites (base icons and overlays in view, a few thousand), one
// affine inverse and at most one mask bit each, per press and release, i.e. per human click
// rate; nothing per frame without one.
#[allow(clippy::too_many_arguments)]
fn send_clicks(
    mut pointer: MessageReader<PointerInput>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    cameras: Query<(&Camera, &GlobalTransform), With<WorldCamera>>,
    hover: Option<Res<HoverMap>>,
    ui_nodes: Query<(), With<Node>>,
    hud_slots: Query<&crate::inventory_client::HudSlot>,
    parents: Query<&ChildOf>,
    (active_z, stack): (Res<ActiveZ>, Option<Res<ss13_shared::LevelStack>>),
    map: Res<NetEntityMap>,
    atlas: Option<Res<IconAtlas>>,
    sprites: Query<SpriteItem>,
    owners: Query<(&NetEntity, &TilePos, Option<&Appearance>)>,
    mut pressed: Local<Option<Press>>,
    mut intents: MessageWriter<SendIntent>,
) {
    let inputs: Vec<(Vec2, PointerAction)> = pointer
        .read()
        .filter(|input| input.pointer_id == PointerId::Mouse)
        .map(|input| (input.location.position, input.action))
        .collect();
    if inputs.is_empty() {
        return;
    }
    let Some(own) = map.controlled() else {
        *pressed = None;
        return;
    };
    let own_id = owners.get(own).ok().map(|(id, ..)| id.0);
    let to_world = |cursor: Vec2| {
        cameras
            .iter()
            .find_map(|(camera, at)| camera.viewport_to_world_2d(at, cursor).ok())
    };
    // The HUD slot under the cursor, if the cursor is over UI at all.
    let over_ui = || -> Option<Option<InventorySlot>> {
        let hits = hover.as_ref()?.get(&PointerId::Mouse)?;
        if !hits.keys().any(|&e| ui_nodes.contains(e)) {
            return None;
        }
        Some(hits.keys().find_map(|&e| {
            std::iter::once(e)
                .chain(parents.iter_ancestors(e))
                .find_map(|a| hud_slots.get(a).ok().map(|s| s.0))
        }))
    };
    let atom_at = |world: Vec2| {
        let drawn = sprites.iter().filter_map(
            |(entity, icon, sprite, anchor, transform, visible, parent, layers)| {
                // Emissive masks are drawn by the lighting only.
                if !visible.get() || layers.is_some_and(|l| !l.intersects(&RenderLayers::default()))
                {
                    return None;
                }
                let owner = if owners.contains(entity) {
                    entity
                } else {
                    parent?.parent()
                };
                let (id, pos, appearance) = owners.get(owner).ok()?;
                let opacity = appearance.map_or(MouseOpacity::Icon, |a| a.mouse_opacity);
                // The camera's level and, through open space, the one below it (tg z-mimic:
                // seen, so examined and pointed at; out of reach for the rest).
                let below = stack.as_deref().and_then(|s| s.below(active_z.0));
                if (pos.z != active_z.0 && Some(pos.z) != below)
                    || opacity == MouseOpacity::Transparent
                {
                    return None;
                }
                let known = atlas
                    .as_ref()
                    .filter(|a| a.manifest.get(icon.state).is_some());
                let (rect, mask) = match known {
                    // A page still loading draws nothing yet.
                    Some(atlas) => (
                        sprite.rect?,
                        match opacity {
                            MouseOpacity::Icon => Some(atlas.mask(icon.state)?),
                            _ => None,
                        },
                    ),
                    None => (Rect::from_corners(Vec2::ZERO, sprite.custom_size?), None),
                };
                Some(DrawnSprite {
                    owner: id.0,
                    z: transform.translation().z,
                    transform: *transform,
                    rect,
                    anchor: anchor.0,
                    mask,
                })
            },
        );
        topmost_at(world, drawn)
    };
    // What the cursor is over and where in the world: `None` over UI that is not a slot.
    let target_at = |cursor: Vec2| -> (Option<MouseTarget>, Vec2) {
        let world = to_world(cursor).unwrap_or(Vec2::NAN);
        let target = match over_ui() {
            Some(slot) => slot.map(MouseTarget::Slot),
            None if world.is_nan() => None,
            None => atom_at(world).map(MouseTarget::Atom),
        };
        (target, world)
    };
    let any = |a, b| keys.pressed(a) || keys.pressed(b);
    let modifiers = |button: PointerButton| ClickModifiers {
        shift: any(KeyCode::ShiftLeft, KeyCode::ShiftRight),
        ctrl: any(KeyCode::ControlLeft, KeyCode::ControlRight),
        alt: any(KeyCode::AltLeft, KeyCode::AltRight),
        middle: button == PointerButton::Middle,
        right: button == PointerButton::Secondary,
        pixel: None,
    };
    // tg `ICON_X`/`ICON_Y`: where on the clicked atom's tile the cursor was.
    let with_pixel = |mut m: ClickModifiers, target: NetId, at: Vec2| {
        let pos = map
            .get(target)
            .and_then(|e| owners.get(e).ok())
            .map(|(_, p, _)| *p);
        m.pixel = pos.and_then(|pos| click_pixel(at, pos));
        m
    };
    let now = time.elapsed_secs_f64();
    for (cursor, action) in inputs {
        match action {
            PointerAction::Press(PointerButton::Primary) => {
                let (target, at) = target_at(cursor);
                *pressed = target.map(|target| Press {
                    target,
                    at,
                    time: now,
                });
            }
            // shortcut: middle and right clicks act on press and never drag (BYOND also drags
            // with them, tg only uses it for the middle-drag click delay).
            PointerAction::Press(button @ (PointerButton::Middle | PointerButton::Secondary)) => {
                if let (Some(MouseTarget::Atom(target)), at) = target_at(cursor) {
                    intents.write(SendIntent(ClientIntent::Click {
                        target,
                        modifiers: with_pixel(modifiers(button), target, at),
                    }));
                }
            }
            PointerAction::Release(PointerButton::Primary) => {
                let Some(press) = pressed.take() else {
                    continue;
                };
                let (over, at) = target_at(cursor);
                let intent = match on_release(press, over, at, now, own_id) {
                    Release::Click(target) => ClientIntent::Click {
                        target,
                        modifiers: with_pixel(modifiers(PointerButton::Primary), target, at),
                    },
                    Release::Drag(from, to) => ClientIntent::Drag {
                        from: from.drag_end(),
                        to: to.drag_end(),
                        modifiers: modifiers(PointerButton::Primary),
                    },
                    Release::Nothing => continue,
                };
                intents.write(SendIntent(intent));
            }
            _ => {}
        }
    }
}

/// A running [`CameraEffect::Recoil`]: out to `offset`, then back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kick {
    pub offset: Vec2,
    pub out: f32,
    pub back: f32,
    pub started: f32,
}

impl Kick {
    /// The view offset `t` seconds after the start; `None` once it is back.
    pub fn at(&self, t: f32) -> Option<Vec2> {
        let t = t - self.started;
        if t < self.out {
            return Some(self.offset * (t / self.out.max(1e-3)));
        }
        let t = (t - self.out) / self.back.max(1e-3);
        if t >= 1.0 {
            return None;
        }
        // BYOND `BACK_EASING` (ease-out): it overshoots past the rest point a little.
        const S: f32 = 1.70158;
        let u = t - 1.0;
        let eased = u * u * ((S + 1.0) * u + S) + 1.0;
        Some(self.offset * (1.0 - eased))
    }
}

/// A running [`CameraEffect::Shake`](ss13_shared::protocol::CameraEffect::Shake): tg
/// `shake_camera` animates the view between random points of a box `max` pixels wide, each leg
/// `max(round(min(|dx|, |dy|) * 0.7 / 32), 1)` deciseconds, then back to rest in 3.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shake {
    pub max: f32,
    /// Seconds of jumping left (tg may overshoot it by one leg).
    pub left: f32,
    pub from: Vec2,
    pub to: Vec2,
    pub leg: f32,
    pub leg_started: f32,
    pub rng: u32,
}

/// tg `shake_camera`'s `animate` back to rest, deciseconds.
const SHAKE_BACK_DS: f32 = 3.0;

impl Shake {
    pub fn new(duration_ds: f32, strength: f32, now: f32, seed: u32) -> Self {
        Self {
            max: strength.clamp(0.0, 8.0) * ss13_shared::TILE_SIZE as f32,
            left: duration_ds.clamp(0.0, 100.0) / 10.0,
            from: Vec2::ZERO,
            to: Vec2::ZERO,
            leg: 0.0,
            leg_started: now,
            rng: seed | 1,
        }
    }

    fn roll(&mut self) -> f32 {
        // xorshift32: a visual jitter, no need for the game's RNG streams.
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng as f32 / u32::MAX as f32 * 2.0 - 1.0) * self.max
    }

    /// The view offset at `now`; `None` once it is back at rest.
    pub fn at(&mut self, now: f32) -> Option<Vec2> {
        while now - self.leg_started >= self.leg {
            self.leg_started += self.leg;
            self.from = self.to;
            if self.left <= 0.0 {
                if self.to == Vec2::ZERO {
                    return None;
                }
                self.to = Vec2::ZERO;
                self.leg = SHAKE_BACK_DS / 10.0;
                continue;
            }
            self.to = Vec2::new(self.roll(), self.roll()).round();
            let d = (self.to - self.from).abs();
            let ds = (d.x.min(d.y) * 0.7 / ss13_shared::TILE_SIZE as f32)
                .round()
                .max(1.0);
            self.leg = ds / 10.0;
            self.left -= self.leg;
        }
        let t = ((now - self.leg_started) / self.leg.max(1e-3)).clamp(0.0, 1.0);
        Some(self.from.lerp(self.to, t))
    }
}

/// tg `recoil_camera` and `shake_camera` on the client: animate
/// [`ss13_render::camera::CameraOffset`].
fn apply_camera_effects(
    mut effects: MessageReader<ss13_net::camera::client::CameraEffectReceived>,
    time: Res<Time>,
    mut kick: Local<Option<Kick>>,
    mut shake: Local<Option<Shake>>,
    mut offset: ResMut<ss13_render::camera::CameraOffset>,
) {
    use ss13_shared::protocol::CameraEffect;
    let now = time.elapsed_secs();
    for effect in effects.read() {
        match effect.0 {
            CameraEffect::Recoil {
                offset: [x, y],
                out_ds,
                back_ds,
            } => {
                // shortcut: a new kick restarts from the rest point (tg animates from where
                // the view is); kicks come a fire delay apart, after most of the previous one.
                *kick = Some(Kick {
                    offset: Vec2::new(f32::from(x), f32::from(y)),
                    out: out_ds.clamp(0.0, 20.0) / 10.0,
                    back: back_ds.clamp(0.0, 40.0) / 10.0,
                    started: now,
                });
            }
            CameraEffect::Shake {
                duration_ds,
                strength,
            } => {
                // tg `shake_camera` ignores durations under a decisecond.
                if duration_ds >= 1.0 && strength > 0.0 {
                    *shake = Some(Shake::new(duration_ds, strength, now, now.to_bits()));
                }
            }
        }
    }
    let at = kick.and_then(|k| k.at(now));
    if at.is_none() {
        *kick = None;
    }
    let shaken = shake.as_mut().and_then(|s| s.at(now));
    if shaken.is_none() {
        *shake = None;
    }
    let total = at.unwrap_or(Vec2::ZERO) + shaken.unwrap_or(Vec2::ZERO);
    offset.set_if_neq(ss13_render::camera::CameraOffset(total));
}

/// A sprite gliding from where it was drawn to its tile, BYOND `glide_size`. Visual only:
/// `TilePos` is the server's and changes at once.
#[derive(Component, Clone, Copy, Debug)]
pub struct Glide {
    from: Vec2,
    to: Vec2,
    start: f64,
    secs: f32,
}

impl Glide {
    /// Drawn position at `now`, whole icon pixels (BYOND glides by whole pixels too).
    pub fn at(&self, now: f64) -> (Vec2, bool) {
        let t = if self.secs > 0.0 {
            ((now - self.start) as f32 / self.secs).clamp(0.0, 1.0)
        } else {
            1.0
        };
        (self.from.lerp(self.to, t).round(), t >= 1.0)
    }
}

/// Runs before `sync_tile_pos` snaps the sprite, so the drawn (old) position is still known.
// shortcut: every mover glides at human run speed; replicate a per-entity glide rate (BYOND sets
// `glide_size` from the mover's delay) when slower or faster movers appear.
fn start_glides(
    time: Res<Time>,
    moved: Query<(Entity, &TilePos, &Transform, Option<&Glide>), Changed<TilePos>>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for (entity, pos, transform, glide) in &moved {
        let from = glide.map_or(transform.translation.xy(), |g| g.at(now).0);
        let to = tile_translation(*pos);
        let distance = from.distance(to);
        if distance == 0.0 || distance > MAX_GLIDE_PX {
            if glide.is_some() {
                commands.entity(entity).remove::<Glide>();
            }
            continue;
        }
        commands.entity(entity).insert(Glide {
            from,
            to,
            start: now,
            secs: RUN_DELAY_SECS * distance / TILE_SIZE as f32,
        });
    }
}

fn apply_glides(
    time: Res<Time>,
    mut gliding: Query<(Entity, &Glide, &mut Transform)>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    for (entity, glide, mut transform) in &mut gliding {
        let (xy, done) = glide.at(now);
        transform.translation.x = xy.x;
        transform.translation.y = xy.y;
        if done {
            commands.entity(entity).remove::<Glide>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_combine_into_diagonals_and_cancel() {
        let mut keys = ButtonInput::<KeyCode>::default();
        assert_eq!(held_dir(&keys), None);
        keys.press(KeyCode::KeyW);
        assert_eq!(held_dir(&keys), Some(Dir::North));
        keys.press(KeyCode::ArrowRight);
        assert_eq!(held_dir(&keys), Some(Dir::NorthEast));
        keys.press(KeyCode::KeyS);
        assert_eq!(held_dir(&keys), Some(Dir::East));
        keys.press(KeyCode::KeyA);
        assert_eq!(held_dir(&keys), None);
    }

    /// A 96x32 page: frame 0 (x 0..32) a small item, opaque only at x 12..20, y 12..20; frame 1
    /// (x 32..64) fully opaque (a floor); frame 2 (x 64..96) a thin standing mob, opaque at
    /// x 14..18 in every row.
    fn page() -> AlphaMask {
        let (w, h) = (96u32, 32u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let drawn = match x / 32 {
                    0 => (12..20).contains(&x) && (12..20).contains(&y),
                    1 => true,
                    _ => (14..18).contains(&(x % 32)),
                };
                if drawn {
                    rgba[((y * w + x) * 4 + 3) as usize] = 255;
                }
            }
        }
        AlphaMask::from_rgba8(w, h, &rgba).unwrap()
    }

    const ITEM: u32 = 0;
    const FLOOR: u32 = 1;
    const MOB: u32 = 2;

    /// Frame `frame` of [`page`] drawn on `tile` the way `ss13_render` draws it: the tile corner
    /// as translation, the pixel offset in the anchor, a BYOND turn around the tile center.
    fn sprite(
        mask: &AlphaMask,
        owner: u64,
        z: f32,
        tile: TilePos,
        frame: u32,
        pixel_offset: Vec2,
        degrees: f32,
    ) -> DrawnSprite<'_> {
        let size = Vec2::splat(32.0);
        let rotation = Quat::from_rotation_z(-degrees.to_radians());
        let center = size / 2.0;
        let offset = (rotation.inverse() * (center + pixel_offset).extend(0.0)).xy() - center;
        DrawnSprite {
            owner: NetId(owner),
            z,
            transform: GlobalTransform::from(
                Transform::from_translation(tile_translation(tile).extend(z))
                    .with_rotation(rotation),
            ),
            rect: Rect::new(frame as f32 * 32.0, 0.0, frame as f32 * 32.0 + 32.0, 32.0),
            anchor: Anchor::BOTTOM_LEFT.0 - offset / size,
            mask: Some(mask),
        }
    }

    #[test]
    fn click_hits_drawn_pixels_of_the_topmost_sprite() {
        let mask = page();
        let z = 2;
        assert_eq!(tile_at(Vec2::new(64.0, 95.9), z), TilePos::new(2, 2, z));
        assert_eq!(tile_at(Vec2::new(63.9, 96.0), z), TilePos::new(1, 3, z));
        let tile = TilePos::new(2, 2, z);
        let at = |x: f32, y: f32| tile_translation(tile) + Vec2::new(x, y);
        let floor = |x| {
            sprite(
                &mask,
                10 + x as u64,
                1.0,
                TilePos::new(x, 2, z),
                FLOOR,
                Vec2::ZERO,
                0.0,
            )
        };
        let item = sprite(&mask, 1, 5.0, tile, ITEM, Vec2::ZERO, 0.0);

        // A small item: its drawn pixels take the click, its transparent corner goes to the
        // floor beneath.
        let drawn = [floor(2), item];
        assert_eq!(topmost_at(at(16.0, 16.0), drawn), Some(NetId(1)));
        assert_eq!(topmost_at(at(12.0, 12.0), drawn), Some(NetId(1)));
        assert_eq!(topmost_at(at(11.9, 16.0), drawn), Some(NetId(12)));
        assert_eq!(topmost_at(at(1.0, 30.0), drawn), Some(NetId(12)));
        assert_eq!(topmost_at(at(200.0, 200.0), drawn), None);

        // The highest z wins where both are drawn.
        let mut above = item;
        above.owner = NetId(2);
        above.z = 9.0;
        assert_eq!(
            topmost_at(at(16.0, 16.0), [floor(2), item, above]),
            Some(NetId(2))
        );

        // `MOUSE_OPACITY_OPAQUE`: anywhere in its icon.
        let mut opaque = item;
        opaque.mask = None;
        assert_eq!(
            topmost_at(at(1.0, 30.0), [floor(2), opaque]),
            Some(NetId(1))
        );

        // The frame shown now (animation frame or facing pick the rect): the full frame is hit
        // where the small one is transparent.
        let mut other_frame = item;
        other_frame.rect = Rect::new(32.0, 0.0, 64.0, 32.0);
        assert_eq!(
            topmost_at(at(1.0, 30.0), [floor(2), other_frame]),
            Some(NetId(1))
        );

        // `pixel_x = 16`: the item's pixels are 16 to the right, over the next tile's floor.
        let shifted = sprite(&mask, 1, 5.0, tile, ITEM, Vec2::new(16.0, 0.0), 0.0);
        let drawn = [floor(2), floor(3), shifted];
        assert_eq!(topmost_at(at(16.0, 16.0), drawn), Some(NetId(12)));
        assert_eq!(topmost_at(at(30.0, 16.0), drawn), Some(NetId(1)));
        assert_eq!(topmost_at(at(35.0, 16.0), drawn), Some(NetId(1)));
        assert_eq!(topmost_at(at(37.0, 16.0), drawn), Some(NetId(13)));

        // An overlay is part of its atom: a click on its pixels is the atom's.
        let overlay = sprite(&mask, 1, 5.5, tile, ITEM, Vec2::new(0.0, 10.0), 0.0);
        let drawn = [floor(2), item, overlay];
        assert_eq!(topmost_at(at(16.0, 25.0), drawn), Some(NetId(1)));
        assert_eq!(
            topmost_at(at(16.0, 25.0), [floor(2), item]),
            Some(NetId(12))
        );
    }

    #[test]
    fn a_lying_mob_is_clicked_where_its_turned_icon_is_drawn() {
        let mask = page();
        let tile = TilePos::new(0, 0, 1);
        let floor = sprite(&mask, 10, 1.0, tile, FLOOR, Vec2::ZERO, 0.0);
        let standing = sprite(&mask, 1, 5.0, tile, MOB, Vec2::ZERO, 0.0);
        let lying = sprite(&mask, 1, 5.0, tile, MOB, Vec2::ZERO, 90.0);
        // Standing: a vertical bar at x 14..18. Lying (turned 90 clockwise): a horizontal bar.
        assert_eq!(
            topmost_at(Vec2::new(16.0, 2.0), [floor, standing]),
            Some(NetId(1))
        );
        assert_eq!(
            topmost_at(Vec2::new(2.0, 16.0), [floor, standing]),
            Some(NetId(10))
        );
        assert_eq!(
            topmost_at(Vec2::new(16.0, 2.0), [floor, lying]),
            Some(NetId(10))
        );
        assert_eq!(
            topmost_at(Vec2::new(2.0, 16.0), [floor, lying]),
            Some(NetId(1))
        );
    }

    #[test]
    fn a_release_clicks_or_drags_like_byond_and_tg() {
        let (bag, floor, bob, me) = (NetId(1), NetId(2), NetId(3), NetId(4));
        let press = |target, time| Press {
            target,
            at: Vec2::new(100.0, 100.0),
            time,
        };
        let atom = MouseTarget::Atom;
        let far = Vec2::new(140.0, 100.0);
        let near = Vec2::new(108.0, 100.0);
        // Released over what was pressed: a click, whenever and wherever.
        assert_eq!(
            on_release(press(atom(bag), 0.0), Some(atom(bag)), far, 5.0, Some(me)),
            Release::Click(bag)
        );
        // Over something else, far: a drag; near and quick: a click on the pressed atom.
        assert_eq!(
            on_release(
                press(atom(bag), 0.0),
                Some(atom(floor)),
                far,
                0.05,
                Some(me)
            ),
            Release::Drag(atom(bag), atom(floor))
        );
        assert_eq!(
            on_release(
                press(atom(bag), 0.0),
                Some(atom(floor)),
                near,
                0.05,
                Some(me)
            ),
            Release::Click(bag)
        );
        assert_eq!(
            on_release(
                press(atom(bag), 0.0),
                Some(atom(floor)),
                near,
                0.5,
                Some(me)
            ),
            Release::Drag(atom(bag), atom(floor))
        );
        // Onto the own mob is always a drag (the strip menu, opening a bag).
        assert_eq!(
            on_release(press(atom(bob), 0.0), Some(atom(me)), near, 0.05, Some(me)),
            Release::Drag(atom(bob), atom(me))
        );
        // HUD slots: a click on the same slot is the slot's own; onto a slot or from one, a drag.
        let hand = MouseTarget::Slot(InventorySlot::LeftHand);
        assert_eq!(
            on_release(press(hand, 0.0), Some(hand), near, 0.05, Some(me)),
            Release::Nothing
        );
        assert_eq!(
            on_release(press(hand, 0.0), Some(atom(floor)), far, 1.0, Some(me)),
            Release::Drag(hand, atom(floor))
        );
        assert_eq!(
            on_release(press(atom(bag), 0.0), Some(hand), far, 1.0, Some(me)),
            Release::Drag(atom(bag), hand)
        );
        // Released over plain UI: nothing.
        assert_eq!(
            on_release(press(atom(bag), 0.0), None, far, 1.0, Some(me)),
            Release::Nothing
        );
    }

    #[test]
    fn glide_reaches_the_tile_in_whole_pixels() {
        let glide = Glide {
            from: Vec2::ZERO,
            to: Vec2::new(32.0, 0.0),
            start: 1.0,
            secs: 0.2,
        };
        assert_eq!(glide.at(1.0), (Vec2::ZERO, false));
        assert_eq!(glide.at(1.1), (Vec2::new(16.0, 0.0), false));
        let (xy, _) = glide.at(1.13);
        assert_eq!(xy, xy.round());
        assert_eq!(glide.at(1.5), (Vec2::new(32.0, 0.0), true));
    }
}
