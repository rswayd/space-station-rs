//! Wire-level contract. Game systems never import the network library:
//! they read and write these types, and `ss13_net` moves them.

use core::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{Dir, TilePos};

/// Bumped on every incompatible change; checked during the handshake.
pub const PROTOCOL_VERSION: u32 = 82;

/// Simulation tick (20 Hz, as BYOND tg). Server loop and network tick sync must agree on it.
pub const TICK_DURATION: Duration = Duration::from_millis(50);

/// Stable id of a replicated entity as the client sees it. Server `Entity` never leaves the server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NetId(pub u64);

/// Client → server. Intents only: the server validates distance, permissions and state
/// at the moment it applies them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClientIntent {
    /// One step of the own mob. `seq` numbers the steps of one connection from 1 up; the server
    /// applies each `seq` at most once, in order, when the mob's cooldown allows, and answers
    /// with a [`MoveAck`] so the client can reconcile its prediction.
    Move {
        dir: Dir,
        seq: u32,
    },
    Click {
        target: NetId,
        modifiers: ClickModifiers,
    },
    /// TGUI-style action: `window` is a server-issued window id, `action` and `params` are
    /// untrusted and validated by the window owner.
    UiAct {
        window: u32,
        action: String,
        params: Vec<(String, String)>,
    },
    Say(String),
    /// tg `swap_hand` (default key X).
    SwapHands,
    /// tg `dropItemToGround` of the active hand's item at the mob's feet (default key Q).
    DropActive,
    /// A click on a HUD inventory slot (tg `/atom/movable/screen/inventory/Click`): a hand slot
    /// makes that hand active; an equipment slot takes its item into the empty active hand or
    /// equips the active hand's item into it.
    ClickSlot(InventorySlot),
    /// tg `toggle_combat_mode` (default key F): flips the own mob's combat mode.
    ToggleCombatMode,
    /// tg `stop_pulling` (BandaStation keys C, Delete; the HUD pull icon).
    StopPulling,
    /// tg `toggle_throw_mode` (default key R; the HUD throw button).
    ToggleThrowMode,
    /// tg `resist` (default key B; the HUD resist button): break a grab.
    Resist,
    /// tg `toggle_resting` (BandaStation key Shift+B; the HUD rest button): lie down or get up.
    ToggleRest,
    /// tg lobby "Join Game": opens the `JobSelection` window (the latejoin menu), whose
    /// `select_job` act picks the job.
    LateJoinMenu,
    /// Alt+click on a HUD inventory slot: tg `click_alt` of the item in it (a gas tank toggles
    /// internals).
    AltClickSlot(InventorySlot),
    /// tg `MouseDrop` (`/atom/MouseDrop` -> `base_mouse_drop_handler`): the mouse was pressed
    /// on `from` and released over `to`, another thing. Drags short in time and distance are
    /// sent as clicks instead (tg `check_drag_proximity`).
    Drag {
        from: DragEnd,
        to: DragEnd,
        modifiers: ClickModifiers,
    },
    /// tg `toggle_move_intent`: the own mob switches between running and walking.
    ToggleMoveIntent,
    /// tg `/mob/proc/up` (PgUp): the ladder underfoot, flight, or a ghost through the floor.
    MoveUp,
    /// tg `/mob/proc/down` (PgDn).
    MoveDown,
    /// A right click on a HUD inventory slot: on the active hand's item, tg
    /// `attack_self_secondary` (a gas analyzer opens its window).
    RightClickSlot(InventorySlot),
    /// The world atom under the cursor changed (BYOND `MouseEntered` -> tg `on_mouse_enter`),
    /// `None`: nothing (the cursor left the map). Sent only on a change; the server answers with
    /// the screentip (`ss13_game::screentips`).
    Hover(Option<NetId>),
}

/// One end of a [`ClientIntent::Drag`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DragEnd {
    /// A replicated entity: an item, a mob, a structure, a turf (a tile is its turf).
    Atom(NetId),
    /// A HUD inventory slot: the item in it, or the empty slot (tg `/atom/movable/screen/
    /// inventory`, e.g. a hand to drag a storage into).
    Slot(InventorySlot),
}

/// HUD inventory slots (tg `ITEM_SLOT_*`; the hands are tg's held indexes 1 and 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InventorySlot {
    LeftHand,
    RightHand,
    Id,
    Uniform,
    Shoes,
    /// `ITEM_SLOT_BACK` (backpacks).
    Back,
    /// `ITEM_SLOT_HEAD`.
    Head,
    /// `ITEM_SLOT_MASK`.
    Mask,
    /// `ITEM_SLOT_NECK`.
    Neck,
    /// `ITEM_SLOT_EYES`.
    Eyes,
    /// `ITEM_SLOT_EARS`.
    Ears,
    /// `ITEM_SLOT_GLOVES`.
    Gloves,
    /// `ITEM_SLOT_OCLOTHING` (outer suit).
    Suit,
    /// `ITEM_SLOT_SUITSTORE`.
    SuitStorage,
    /// `ITEM_SLOT_BELT`.
    Belt,
    /// `ITEM_SLOT_LPOCKET`.
    LeftPocket,
    /// `ITEM_SLOT_RPOCKET`.
    RightPocket,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClickModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub middle: bool,
    /// tg `RIGHT_CLICK`: secondary interactions (`*_act_secondary`, `interact_with_atom_secondary`).
    pub right: bool,
    /// Where the cursor was, in pixels from the center of the clicked atom's tile (tg `ICON_X` /
    /// `ICON_Y` minus 16, plus the atom's pixel offset): guns aim at that point. Untrusted; the
    /// server clamps it. `None`: the tile's center.
    pub pixel: Option<[i8; 2]>,
}

/// Server → client: the authoritative state of the client's own mob after the server applied
/// every `Move` up to `seq` (0: none yet). Sent when a step was applied (moved or blocked) or the
/// mob moved or turned for another reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveAck {
    pub seq: u32,
    pub pos: TilePos,
    pub facing: Option<Dir>,
}

/// Server → client: a move of the client's view, relative to where it follows the mob.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum CameraEffect {
    /// tg `recoil_camera`: the view slides `offset` pixels away over `out_ds` deciseconds,
    /// then back over `back_ds` (`BACK_EASING`).
    Recoil {
        offset: [i16; 2],
        out_ds: f32,
        back_ds: f32,
    },
    /// tg `shake_camera`: for `duration_ds` deciseconds the view jumps between random offsets
    /// up to `strength` tiles away, then eases back over 3 deciseconds.
    Shake { duration_ds: f32, strength: f32 },
}

/// Server → client: one sound for this client (tg `SEND_SOUND` from `playsound_local`), already
/// attenuated for its listener.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SoundEvent {
    /// The file's tg path (`sound/items/weapons/punch1.ogg`), under the assets' `sound/` dir.
    pub file: String,
    /// BYOND volume, 0-100.
    pub volume: f32,
    /// Playback speed (1: as recorded): tg `frequency` (`vary`).
    pub speed: f32,
    /// Tiles from the listener to the source (x east, y north); `None`: not positional
    /// (`sound_to_playing_players`).
    pub offset: Option<[i16; 2]>,
}

/// Server → client system notice (MOTD, admin message, kick reason, ...).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ServerNotice {
    pub text: String,
}
