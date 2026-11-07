//! Admin channel (`ss13_admin`): tg `/datum/admins` verbs and BandaStation's ticket manager.
//! Separate from the debug channel: it works in every build, and the server applies a request
//! only when the asking client's admin rights allow it at that moment. Players use it for
//! `adminhelp` only. Everything here is untrusted on the server.

use serde::{Deserialize, Serialize};

use crate::TilePos;

/// tg `R_*` rights flags (`code/__DEFINES/admin.dm`), same bits.
pub mod rights {
    pub const R_BUILD: u32 = 1 << 0;
    pub const R_ADMIN: u32 = 1 << 1;
    pub const R_BAN: u32 = 1 << 2;
    pub const R_FUN: u32 = 1 << 3;
    pub const R_SERVER: u32 = 1 << 4;
    pub const R_DEBUG: u32 = 1 << 5;
    pub const R_POSSESS: u32 = 1 << 6;
    pub const R_PERMISSIONS: u32 = 1 << 7;
    pub const R_STEALTH: u32 = 1 << 8;
    pub const R_POLL: u32 = 1 << 9;
    pub const R_VAREDIT: u32 = 1 << 10;
    pub const R_SOUND: u32 = 1 << 11;
    pub const R_SPAWN: u32 = 1 << 12;
    pub const R_AUTOADMIN: u32 = 1 << 13;
    pub const R_DBRANKS: u32 = 1 << 14;
    /// BandaStation.
    pub const R_MENTOR: u32 = 1 << 15;
    pub const R_EVERYTHING: u32 = (1 << 16) - 1;
}

/// Client → server.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AdminRequest {
    /// Any player (BandaStation `adminhelp`, F1): opens a ticket, or writes into the open one.
    Adminhelp(String),
    /// Listed admins: BandaStation `readmin`/`deadmin`.
    Readmin,
    Deadmin,
    /// `R_ADMIN`: admin chat.
    Asay(String),
    /// `R_ADMIN`: answered by [`AdminResponse::Players`].
    Players,
    /// `R_ADMIN`: a message into a ticket (links the admin to it).
    TicketReply {
        ticket: u32,
        text: String,
    },
    /// `R_ADMIN`: close, resolve or reopen a ticket.
    SetTicketState {
        ticket: u32,
        state: TicketState,
    },
    /// `R_ADMIN`: admin PM to a player: into the player's open ticket, else a new PM ticket.
    Pm {
        ckey: String,
        text: String,
    },
    /// `R_ADMIN`: the own mob to the player's tile (player panel "JMP").
    JumpTo(String),
    /// `R_ADMIN`: the player's mob to the own tile (tg "Get Mob").
    Get(String),
    /// `R_ADMIN`: admin ghost (if needed) that follows the player (tg "FLW").
    Follow(String),
    /// `R_ADMIN`.
    Kick {
        ckey: String,
        reason: String,
    },
    /// `R_ADMIN`: mutes (or unmutes) IC speech, deadchat and adminhelp (tg `MUTE_ALL`).
    Mute {
        ckey: String,
        mute: bool,
    },
    /// `R_BAN`.
    Ban {
        ckey: String,
        reason: String,
    },
    /// `R_BAN`.
    Unban(String),
    /// `R_ADMIN`: become an admin ghost, or go back into the body.
    Aghost,
    /// `R_SPAWN`: prototype ids containing the text, answered by [`AdminResponse::Prototypes`].
    FindPrototypes(String),
    /// `R_SPAWN`: a new object of the prototype on the own tile (tg "Spawn").
    Spawn(String),
    /// `R_ADMIN`: full heal and revive (tg "Rejuvenate").
    Rejuvenate(String),
    /// `R_ADMIN | R_FUN`.
    Smite {
        ckey: String,
        smite: Smite,
    },
    /// `R_FUN`: event names, answered by [`AdminResponse::Events`].
    Events,
    /// `R_FUN`: tg "Trigger Event"; `announce: None` uses the event's own chance.
    ForceEvent {
        name: String,
        announce: Option<bool>,
    },
    /// `R_SERVER`: tg "Start Now": start the round at once; again before it starts, cancel and
    /// count down 3 minutes.
    StartNow,
    /// `R_SERVER`: tg "End Round".
    EndRound,
    /// `R_SERVER`: tg "Reboot World".
    Restart,
    /// `R_DEBUG`: speed of the server's virtual time (1.0 = normal).
    TimeScale(f32),
    /// `R_ADMIN`: a Central Command priority announcement (tg "Create Command Report").
    Announce {
        title: String,
        text: String,
    },
    /// `R_ADMIN`: tg traitor panel "make antag".
    MakeAntag {
        ckey: String,
        role: AntagRole,
    },
    /// `R_ADMIN`: the player's mind (tg `traitor_panel`), answered by [`AdminResponse::Mind`].
    MindPanel(String),
    /// `R_ADMIN`: a change from the mind panel (tg `/datum/mind/Topic`), answered by the
    /// updated [`AdminResponse::Mind`].
    MindAct {
        ckey: String,
        act: MindAct,
    },
    /// `R_ADMIN`: view-only round and antagonist summary (tg "Dynamic Panel").
    Dynamic,
    /// `R_ADMIN`: run a midround ruleset now, by config tag (tg `force_run_midround`).
    ForceMidround(String),
    /// `R_ADMIN`: tg "Set Security Level", by level name (`green` ... `delta`, BandaStation's
    /// `gamma` and `epsilon`).
    SetSecurityLevel(String),
    /// `R_FUN`: tg "Drop Bomb": an explosion on the own tile, ignoring the bomb cap.
    DropBomb(BombSize),
    /// `R_ADMIN | R_FUN`: tg "EM Pulse": `empulse()` on the own tile with these ranges.
    Emp {
        heavy: u8,
        light: u8,
    },
    /// `R_PERMISSIONS`: the admins and ranks, answered by [`AdminResponse::AdminList`] (tg
    /// "Permissions Panel").
    AdminList,
    /// `R_PERMISSIONS`: lists `ckey` as an admin of `rank` (`Rank1+Rank2` allowed) or changes
    /// the rank, for this round only (tg "temporary" change; no database yet).
    SetAdminRank {
        ckey: String,
        rank: String,
    },
    /// `R_PERMISSIONS`: removes `ckey` from the admins for this round.
    RemoveAdmin(String),
    /// `R_ADMIN`: BandaStation "Man Up" to a living player.
    ManUp(String),
    /// `R_ADMIN`: BandaStation "Global Man Up" to every connected player.
    GlobalManUp,
    /// `R_SOUND`: BandaStation "Play Z-level Sound" / "Play Sound in View": an asset sound path
    /// (`sound/...` or `modular_bandastation/...`, `.ogg`) at `volume` (1-100).
    PlaySound {
        file: String,
        volume: u8,
        scope: SoundScope,
    },
    /// `R_DEBUG`: tg "Debug Air Status" (analyzer readout of the own tile) and the SSair
    /// counters of the Atmos Control Panel.
    AirStatus,
    /// `R_DEBUG`: tg `atmos_spawn_air`: a gas string (`o2=22;n2=82;TEMP=293.15`) into the own
    /// tile.
    SpawnAir(String),
}

/// Who hears an admin sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SoundScope {
    /// Every player on the admin's z-level.
    ZLevel,
    /// Every player the admin's tile sees (tg `get_hearers_in_view`).
    View,
}

/// A row of the permissions panel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdminEntry {
    pub ckey: String,
    /// The rank names joined by `+`.
    pub rank: String,
    pub rights: u32,
    pub online: bool,
}

/// tg "Drop Bomb" choices: `(devastation, heavy, light, flash)` ranges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BombSize {
    /// `(1, 2, 3, 3)`.
    Small,
    /// `(2, 3, 4, 4)`.
    Medium,
    /// `(3, 5, 7, 5)`.
    Big,
    /// The bomb cap's ranges.
    Maxcap,
    Custom {
        devastation: u8,
        heavy: u8,
        light: u8,
        flash: u8,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TicketState {
    Open,
    Closed,
    Resolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Smite {
    /// tg `/datum/smite/lightning`.
    Lightning,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AntagRole {
    Traitor,
    Brother,
    Changeling,
    Vampire,
    Cultist,
    Heretic,
    /// `/datum/antagonist/rev/head`.
    HeadRev,
    /// `/datum/antagonist/rev`.
    Revolutionary,
}

/// A mind panel change (tg `/datum/mind/Topic` hrefs).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MindAct {
    /// `remove_antag`: only the traitor and the blood brother.
    RemoveAntag(AntagRole),
    /// `obj_delete`: the objective by its index in [`MindPanel::objectives`].
    RemoveObjective(u32),
    /// `obj_announce`.
    AnnounceObjectives,
}

/// tg `traitor_panel`: what the mind holds.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MindPanel {
    pub ckey: String,
    /// Mind name, current mob, role, deaths and the like, one per line.
    pub lines: Vec<String>,
    /// Antagonist role names.
    pub antag: Vec<String>,
    /// The traitor's objectives, then the blood brother's.
    pub objectives: Vec<String>,
}

/// Server → client.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AdminResponse {
    /// The client's active admin rights; `rights == 0` after a deadmin. Sent only to listed admins.
    Rights {
        rank: String,
        rights: u32,
    },
    Players(Vec<PlayerEntry>),
    /// Every ticket the client may see: an admin all of the round's, a player the own current
    /// or last one. Pushed whenever one changes.
    Tickets(Vec<Ticket>),
    Prototypes {
        query: String,
        ids: Vec<String>,
    },
    Events(Vec<String>),
    Dynamic(Vec<String>),
    Mind(MindPanel),
    /// A request was applied; what happened.
    Done(String),
    /// Why a request was not applied.
    Refused(String),
    /// The permissions panel: every listed admin and the rank names from `admin_ranks.txt`.
    AdminList {
        admins: Vec<AdminEntry>,
        ranks: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerEntry {
    pub ckey: String,
    /// The mob's name (empty in the lobby).
    pub name: String,
    pub job: String,
    /// Antagonist role names.
    pub antag: Vec<String>,
    pub state: PlayerState,
    pub pos: Option<TilePos>,
    pub muted: bool,
    /// Rank name of an admin, empty otherwise.
    pub rank: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerState {
    Lobby,
    Alive,
    Dead,
    Ghost,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ticket {
    pub id: u32,
    /// The player's ckey.
    pub initiator: String,
    pub state: TicketState,
    /// Opened by an admin PM (BandaStation `TICKET_TYPE_HIDDEN_PM`).
    pub pm: bool,
    /// Ckey of the admin answering it.
    pub linked_admin: Option<String>,
    pub messages: Vec<TicketMessage>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TicketMessage {
    /// Ckey of the sender.
    pub sender: String,
    pub text: String,
    /// Seconds since the server started.
    pub time: u32,
}
