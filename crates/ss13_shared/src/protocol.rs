//! Wire-level contract. Game systems never import the network library:
//! they read and write these types, and `ss13_net` moves them.

use core::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{Dir, TilePos};

/// Bumped on every incompatible change; checked during the handshake.
pub const PROTOCOL_VERSION: u32 = 124;

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
    /// BandaStation `pixel_shift` (the key held with a direction): the own mob's sprite one
    /// pixel that way inside its tile. The client sends it instead of a [`Self::Move`] (and
    /// predicts nothing) while the key is held, at most once per server tick; the server checks
    /// the mob may shift and caps the offset.
    PixelShift(Dir),
    /// tg `/client/verb/ooc` (keys O, F2).
    Ooc(String),
    /// BandaStation `/client/proc/looc_message` (key L); `wall_pierce`: heard through walls in
    /// the same range (the "LOOC (Wallpierce)" verb).
    Looc {
        text: String,
        wall_pierce: bool,
    },
    /// tg `/mob/verb/pray` (key P).
    Pray(String),
    /// tg `quick_equip` (key E): the active hand's item into the best free slot.
    QuickEquip,
    /// tg `smart_equip_targeted` (Shift+E, Shift+V, Shift+Q, Alt+1, Alt+2): the active hand's
    /// item into `slot`'s item or the slot, or the last thing out of it into the hand. Only
    /// the belt, back, suit storage and pocket slots are accepted.
    SmartEquip(InventorySlot),
    /// BandaStation `quick_accessory_draw` (Shift+F).
    QuickAccessoryDraw,
    /// tg `/datum/keybinding/living/give` (key V): offers the active hand's item to whoever is
    /// next to the mob.
    GiveItem,
    /// A click on a chat link ([`crate::chat::Span::Link`]): BYOND's `Topic` of an href the
    /// server put in a chat line. The server checks the id was given to this client.
    ChatLink(u32),
    /// tg `/datum/keybinding/robot` `toggle_module` (keys 1, 2, 3): the cyborg's module slot,
    /// from 1.
    ToggleModule(u8),
    /// tg `/datum/keybinding/artificial_intelligence/reconnect` (key -): the AI back into its
    /// last shell.
    ReconnectShell,
    /// tg `/datum/keybinding/robot/undeploy` (key =): the shell's AI back into its core.
    UndeployShell,
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
    /// tg `/datum/component/scope` (`animate(client, pixel_x, pixel_y)`): the view stays
    /// `offset` pixels away until the next one; `[0, 0]` zooms back (over 0.2 s).
    Scope { offset: [i16; 2] },
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

/// tg's reserved BYOND channels (`code/__DEFINES/sound.dm`, BandaStation's TTS ones). BYOND has
/// 1024; the ones below [`CHANNEL_HIGHEST_AVAILABLE`] are handed out by the server's `SSsounds`.
/// tg `CHANNEL_LOBBYMUSIC`: the lobby's title music.
pub const CHANNEL_LOBBYMUSIC: u16 = 1024;
pub const CHANNEL_ADMIN: u16 = 1023;
pub const CHANNEL_VOX: u16 = 1022;
/// tg `CHANNEL_JUKEBOX`: jukeboxes and boomboxes.
pub const CHANNEL_JUKEBOX: u16 = 1021;
pub const CHANNEL_HEARTBEAT: u16 = 1020;
pub const CHANNEL_BOSS_MUSIC: u16 = 1019;
pub const CHANNEL_AMBIENCE: u16 = 1018;
pub const CHANNEL_BUZZ: u16 = 1017;
pub const CHANNEL_TRAITOR: u16 = 1016;
pub const CHANNEL_CHARGED_SPELL: u16 = 1015;
pub const CHANNEL_ELEVATOR: u16 = 1014;
pub const CHANNEL_ESCAPEMENU: u16 = 1013;
pub const CHANNEL_WEATHER: u16 = 1012;
pub const CHANNEL_TTS_RADIO: u16 = 1011;
pub const CHANNEL_TTS_ANNOUNCEMENT: u16 = 1010;
pub const CHANNEL_TTS_TELEPATHY: u16 = 1009;
pub const CHANNEL_HIGHEST_AVAILABLE: u16 = 1008;

/// BYOND `SOUND_ENVIRONMENT_NONE`: no reverb.
pub const SOUND_ENVIRONMENT_NONE: i8 = -1;
/// BYOND's environments (`SOUND_ENVIRONMENT_*`, 0 generic .. 25 psychotic).
pub const SOUND_ENVIRONMENT_UNDERWATER: i8 = 22;
pub const SOUND_ENVIRONMENT_DRUGGED: i8 = 23;
pub const SOUND_ENVIRONMENT_PSYCHOTIC: i8 = 25;

/// tg's sound volume preferences (`/datum/preference/numeric/volume/*`, BandaStation's TTS
/// ones): the player's volume of each kind of sound. [`SoundCategory::Game`] is every other
/// sound (no preference: only the master volume).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SoundCategory {
    #[default]
    Game,
    /// `sound_ambience_volume`.
    Ambience,
    /// `sound_ship_ambience_volume`.
    ShipAmbience,
    /// `sound_lobby_volume`.
    LobbyMusic,
    /// `sound_midi`: admin music and the round end song.
    AdminMusic,
    /// `sound_jukebox`.
    Jukebox,
    /// `sound_instruments`.
    Instruments,
    /// `sound_radio_noise`.
    RadioNoise,
    /// `sound_ai_vox`.
    AiVox,
    /// `sound_tts_volume`.
    Tts,
    /// `sound_tts_radio_volume`.
    TtsRadio,
}

impl SoundCategory {
    /// The preferences, in tg's menu order.
    pub const PREFERENCES: [Self; 10] = [
        Self::Ambience,
        Self::ShipAmbience,
        Self::LobbyMusic,
        Self::AdminMusic,
        Self::Jukebox,
        Self::Instruments,
        Self::RadioNoise,
        Self::AiVox,
        Self::Tts,
        Self::TtsRadio,
    ];

    /// The preference's `savefile_key`.
    pub fn key(self) -> &'static str {
        match self {
            Self::Game => "",
            Self::Ambience => "sound_ambience_volume",
            Self::ShipAmbience => "sound_ship_ambience_volume",
            Self::LobbyMusic => "sound_lobby_volume",
            Self::AdminMusic => "sound_midi",
            Self::Jukebox => "sound_jukebox",
            Self::Instruments => "sound_instruments",
            Self::RadioNoise => "sound_radio_noise",
            Self::AiVox => "sound_ai_vox",
            Self::Tts => "sound_tts_volume",
            Self::TtsRadio => "sound_tts_radio_volume",
        }
    }

    /// The preference's name in BandaStation's preferences menu (`sounds.tsx`).
    pub fn label(self) -> &'static str {
        match self {
            Self::Game => "Игра",
            Self::Ambience => "Громкость окружения",
            Self::ShipAmbience => "Громкость звуков корабля",
            Self::LobbyMusic => "Громкость музыки в лобби",
            Self::AdminMusic => "Громкость админской музыки",
            Self::Jukebox => "Громкость музыкальных автоматов",
            Self::Instruments => "Громкость музыкальных инструментов",
            Self::RadioNoise => "Громкость оповещения рации",
            Self::AiVox => "Громкость оповещения VOX ИИ",
            Self::Tts => "TTS - громкость",
            Self::TtsRadio => "TTS - громкость рации",
        }
    }

    /// `create_default_value`: the maximum, 75 for the radio TTS.
    pub fn default_percent(self) -> u8 {
        match self {
            Self::TtsRadio => 75,
            _ => 100,
        }
    }

    /// The preference of a sound sent on a reserved `channel` without one of its own (tg sends
    /// those at the preference's volume).
    pub fn of_channel(channel: u16) -> Self {
        match channel {
            CHANNEL_LOBBYMUSIC => Self::LobbyMusic,
            CHANNEL_ADMIN => Self::AdminMusic,
            CHANNEL_VOX => Self::AiVox,
            CHANNEL_JUKEBOX => Self::Jukebox,
            CHANNEL_AMBIENCE => Self::Ambience,
            CHANNEL_BUZZ => Self::ShipAmbience,
            CHANNEL_TTS_RADIO => Self::TtsRadio,
            CHANNEL_TTS_ANNOUNCEMENT | CHANNEL_TTS_TELEPATHY => Self::Tts,
            _ => Self::Game,
        }
    }
}

/// What BYOND's `sound` datum adds to a [`SoundEvent`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SoundFx {
    /// `sound.environment` (`SOUND_ENVIRONMENT_*`): the listener's room reverb;
    /// [`SOUND_ENVIRONMENT_NONE`] for none.
    pub environment: i8,
    /// `sound.pitch`: a pitch multiplier that keeps the length (`speed` changes both).
    pub pitch: f32,
    /// tg `volume_preference`; [`SoundCategory::Game`] takes the channel's
    /// ([`SoundCategory::of_channel`]).
    pub category: SoundCategory,
}

impl Default for SoundFx {
    fn default() -> Self {
        Self {
            environment: SOUND_ENVIRONMENT_NONE,
            pitch: 1.0,
            category: SoundCategory::Game,
        }
    }
}

impl SoundFx {
    /// The preference the client scales a sound on `channel` by.
    pub fn category_on(&self, channel: u16) -> SoundCategory {
        match self.category {
            SoundCategory::Game => SoundCategory::of_channel(channel),
            own => own,
        }
    }
}

/// Server → client: a BYOND `sound` datum (tg `SEND_SOUND`). On a numbered channel it replaces
/// whatever plays there, and `None` stops it (`sound(null, channel = N)`, `stop_sound_channel`).
/// Channel 0 is BYOND's "any free channel": the sound plays alongside the others (`playsound`),
/// and `None` on it stops every sound.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct ChannelSound {
    pub channel: u16,
    pub sound: Option<SoundEvent>,
    /// BYOND `SOUND_UPDATE`: only the volume and offset of the sound already playing on the
    /// channel change (nothing restarts); ignored when the channel is silent.
    pub update: bool,
    /// BYOND `sound.repeat`: loops until stopped.
    pub repeat: bool,
    pub fx: SoundFx,
}

/// Server → client system notice (MOTD, admin message, kick reason, ...).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ServerNotice {
    pub text: String,
}
