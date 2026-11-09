//! Server → client one-shot visuals of a replicated entity: BYOND `flick` and `animate`.
//! Sent only to the clients that have the entity replicated, after the replication that spawned
//! it; a client that sees the entity later does not get animations already running.

use serde::{Deserialize, Serialize};

use crate::IconStateId;
use crate::protocol::NetId;

/// Most steps a [`TransformAnimation`] may have; the client drops longer ones.
pub const MAX_ANIMATION_STEPS: usize = 64;

/// One animation of the entity the client knows as `target`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnimationEvent {
    pub target: NetId,
    pub animation: Animation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Animation {
    /// BYOND `flick(state, atom)`: the base sprite shows `state` from its first frame, then its
    /// appearance's own state again. `duration_ds` `None`: one cycle of the state's frames (as
    /// BYOND); `Some`: that many deciseconds.
    Flick {
        state: IconStateId,
        duration_ds: Option<u16>,
    },
    /// BYOND `animate(atom, ..., flags = ANIMATION_PARALLEL)` of `pixel_*` and `transform`: runs
    /// alongside the entity's other transform animations (their offsets and turns add up, their
    /// scales multiply), relative to its rest transform, and leaves it at rest when done.
    Transform(TransformAnimation),
    /// BYOND `animate(atom)` with no steps: ends every transform animation of the entity.
    StopTransforms,
    /// tg `balloon_alert`: `text` (at most [`MAX_BALLOON_CHARS`] characters, plain) floats up
    /// from the entity and fades. Sent to one client only, never to every viewer.
    Balloon { text: String },
    /// tg `/datum/chatmessage` (runechat): `text` over the entity, for one client. Stacked and
    /// aged by the client (`ss13_game::runechat_client`).
    Runechat {
        text: String,
        /// `chat_color` of the speaker (`colorize_string`), RGB.
        color: [u8; 3],
        style: RunechatStyle,
    },
}

/// The classes of a [`Animation::Runechat`] (`extra_classes`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunechatStyle {
    /// `italics`: whispers and emotes; the color is already darkened.
    pub italics: bool,
    /// `small`: non-mob speakers.
    pub small: bool,
    /// `yell`: the text ends in `!!`.
    pub yell: bool,
    /// BandaStation: the `[LOOC]` prefix.
    pub looc: bool,
}

/// Most characters of a [`Animation::Balloon`]; the server cuts longer texts.
pub const MAX_BALLOON_CHARS: usize = 120;

/// tg `CHAT_MESSAGE_MAX_LENGTH`: a longer [`Animation::Runechat`] is cut and gets `...`.
pub const MAX_RUNECHAT_CHARS: usize = 110;

/// Keyframes from the rest transform: step `i` moves from step `i - 1`'s pose (the first from
/// rest) to its own over its time. The whole list plays `loops` times (0: until stopped), each
/// cycle from rest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransformAnimation {
    pub steps: Vec<AnimationStep>,
    pub loops: u16,
}

/// The pose a step reaches, relative to rest, and how it gets there.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnimationStep {
    /// Pixels, +y up (BYOND `pixel_x`/`pixel_w` and `pixel_y`/`pixel_z`).
    pub offset: [f32; 2],
    /// Clockwise degrees around the icon's center (BYOND `transform.Turn`).
    pub turn: f32,
    /// Around the icon's center (BYOND `transform.Scale`); `[1, 1]` = rest.
    pub scale: [f32; 2],
    pub time_ds: f32,
    pub easing: Easing,
}

impl AnimationStep {
    /// Back to rest over `time_ds`, linearly.
    pub const fn rest(time_ds: f32) -> Self {
        Self {
            offset: [0.0, 0.0],
            turn: 0.0,
            scale: [1.0, 1.0],
            time_ds,
            easing: Easing::LINEAR,
        }
    }
}

/// BYOND `easing`: a curve and the `EASE_IN` / `EASE_OUT` flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Easing {
    pub curve: EaseCurve,
    pub mode: EaseMode,
}

impl Easing {
    pub const LINEAR: Self = Self {
        curve: EaseCurve::Linear,
        mode: EaseMode::InOut,
    };

    /// BYOND's defaults without `EASE_IN`/`EASE_OUT`: `BOUNCE_EASING` and `ELASTIC_EASING` ease
    /// out, the others both ways.
    pub const fn of(curve: EaseCurve) -> Self {
        let mode = match curve {
            EaseCurve::Bounce | EaseCurve::Elastic => EaseMode::Out,
            _ => EaseMode::InOut,
        };
        Self { curve, mode }
    }

    pub const fn with(curve: EaseCurve, mode: EaseMode) -> Self {
        Self { curve, mode }
    }
}

/// BYOND `LINEAR_EASING`, `SINE_EASING`, `QUAD_EASING`, `CUBIC_EASING`, `CIRCULAR_EASING`,
/// `BACK_EASING`, `BOUNCE_EASING`, `ELASTIC_EASING`, `JUMP_EASING`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EaseCurve {
    Linear,
    Sine,
    Quad,
    Cubic,
    Circular,
    Back,
    Bounce,
    Elastic,
    Jump,
}

/// BYOND `EASE_IN` (slow start), `EASE_OUT` (slow end), neither or both (`InOut`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EaseMode {
    InOut,
    In,
    Out,
}
