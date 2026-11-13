//! The blood cult (server) after tg `/datum/antagonist/cult` (`datums/cultist.dm`),
//! `/datum/team/cult` (`datums/cult_team.dm`), the runes (`runes.dm`), the ritual dagger
//! (`/datum/component/cult_ritual_item`), blood magic (`blood_magic.dm`), Communion
//! (`cult_comms.dm`), the objectives (`cult_objectives.dm`) and Nar'Sie's round end
//! (`code/modules/power/singularity/narsie.dm`).
//!
//! - [`add_cultist`] is `add_antag_datum(/datum/antagonist/cult)`: one team per round
//!   ([`CultTeam`]), the greeting and objectives, the `AntagInfoGeneric` panel, the cult HUD icon
//!   (the team mates' too), the Communion and blood magic buttons, a ritual dagger and ten runed
//!   metal for the roundstart cultists (`equip_cultist`). The first roundstart cultist leads
//!   (`make_cult_leader`, the `cultmaster` icon).
//! - The dagger in hand opens the rune list ("Sigils of Power"); the rune is drawn in the
//!   cultist's blood (`scribe_damage` brute to an arm, a `scribe_delay` `do_after`) on the
//!   cultist's tile. Hitting a rune with the dagger erases it; hitting a cultist purges their holy
//!   water. An empty hand on a rune invokes it with the cultists around it ([`RuneKind`]).
//! - Blood magic: a spell is carved into the flesh (10 s and 40 blood, 4 s and 8 blood next to an
//!   Empower rune; one spell without it, four with it) and its aura is called into the hand.
//! - Counterplay: holy water in the blood clears the spells and deconverts after a minute;
//!   mindshields (the jobs whose outfit implants one), the chaplain, holy water and the
//!   sacrifice target cannot be converted (they can only be sacrificed, by three).
//! - The team grows red eyes and then halos at BandaStation's ratios of cultists to the living.
//! - Nar'Sie's rune, scribed in one of the three summoning areas after the sacrifice, called by
//!   nine, summons her: the cult's objectives are done and the round ends after CentCom's
//!   countdown (tg `begin_the_end`).
// shortcut: the runes are Offer, Empower, Teleport, Revive, Barrier, Summon Cultist and Nar'Sie;
// Boil Blood, Spirit Realm, Apocalypse and Manifest need projectiles, ghosts as players and
// hallucinations (not ported). Blood magic is stun, teleport, shadow shackles, twisted
// construction (plasteel into runed metal only: no construct shells, constructs or soulstones
// until basic mobs exist), EMP and summon dagger; no equipment, horror,
// veiling or blood rites. No cult master abilities (marks, final reckoning, pulse), no TTS.
// Structures: [`structures`]; soulstones, shades and shells: [`soulstone`]. A sacrifice leaves
// a soulstone (with the shade of a body with a mind) and gibs what is left. Nar'Sie is a sprite that does not move or consume:
// no singularity component. No cinematic or shuttle lockdown; its death sound (`demon_dies`) waits
// for a killable Nar'Sie.

use bevy::ecs::message::MessageCursor;
use std::ops::Not;

use bevy::prelude::*;
use ss13_chat::server::{Listeners, say_message, whisper_message};
use ss13_mapload::spawner::Rng;
use ss13_mapload::{Areas, DmName, DmPower, IconIndex};
use ss13_net::chat::server::SendChat;
use ss13_net::server::ClientToken;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::list_input::{ListInputData, ListInputUi};
use ss13_tgui::prompt::{self, Ask, PromptAnswer, PromptAnswered, PromptSystems, ask_world};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};

use super::{AntagInfoGenericUi, CANCEL, SUBMIT};
use crate::actions::{ActionButton, ActionButtonsUi, set_buttons};
use crate::antag::dynamic::DynamicRng;
use crate::antag::objectives::RoundEnd;
use crate::antag::{AntagHudData, AntagHudUi, BrotherInfo, ObjectiveInfo};
use crate::combat::{CLICK_CD_MELEE, NextMove};
use crate::construction::{Job, path_of};
use crate::ghost::Ghost;
use crate::health::{DamageType, Health, Stat, apply_damage, apply_damage_zone};
use crate::human::HumanLook;
use crate::inventory::{CarriedBy, Inventory, NoDrop};
use crate::jobs::{Jobs, PlayerJob};
use crate::medical::blood::Blood;
use crate::medical::{name_of, visible};
use crate::server::PlayerMob;
use crate::status_effects::{Incap, effects_mut};
use crate::tools::{Actor, DoAfter, do_after, tell};
use crate::zone::BodyZone;

mod life;
mod magic;
mod membership;
mod prompts;
mod runes;
mod scribe;
mod soulstone;
mod structures;
mod windows;
pub use life::*;
use magic::*;
pub use membership::*;
pub use prompts::*;
pub use runes::*;
pub use scribe::*;
pub use soulstone::*;
pub use structures::*;
pub use windows::*;

/// `name = "Культист"`.
pub const CULTIST_NAME: &str = "Культист";
/// `/datum/antagonist/cult/converted` `name` (BandaStation).
pub const CONVERTED_NAME: &str = "Обращенный культист";
/// `/datum/antagonist/cult/shade` and `/construct` `name`.
pub const SHADE_NAME: &str = "Тень культа";
pub const CONSTRUCT_NAME: &str = "Конструкт культа";
pub const ROUNDEND_CATEGORY: &str = "Культисты";
const OBJECTIVE_NAME: &str = "Задача";

pub const DAGGER: &str = "/obj/item/melee/cultblade/dagger";
const RUNED_METAL: &str = "/obj/item/stack/sheet/runed_metal";
const PLASTEEL: &str = "/obj/item/stack/sheet/plasteel";
const SHACKLES: &str = "/obj/item/restraints/handcuffs/cult";
const BARRIER: &str = "/obj/structure/emergency_shield/cult/barrier";
pub const NARSIE: &str = "/obj/narsie";
pub const HOLY_WATER: &str = "/datum/reagent/water/holywater";

const ANTAG_HUD_DMI: &str = "icons/mob/huds/antag_hud.dmi";
const ACTIONS_DMI: &str = "icons/mob/actions/actions_cult.dmi";
const BACKGROUNDS_DMI: &str = "icons/mob/actions/backgrounds.dmi";
const HALO_DMI: &str = "icons/mob/effects/halo.dmi";
/// `BLOODCULT_EYE` (`COLOR_RED`).
const BLOODCULT_EYE: [u8; 3] = [255, 0, 0];

/// BandaStation `CULT_HIGHPOP_THRESHOLD` and the risen/ascendent ratios below and above it.
pub const CULT_HIGHPOP_THRESHOLD: u32 = 70;
pub const CULT_RISEN_LOWPOP: f32 = 0.2;
pub const CULT_ASCENDENT_LOWPOP: f32 = 0.3;
pub const CULT_RISEN_HIGHPOP: f32 = 0.1;
pub const CULT_ASCENDENT_HIGHPOP: f32 = 0.2;
/// `MAX_BLOODCHARGE`, `RUNELESS_MAX_BLOODCHARGE`.
pub const MAX_BLOODCHARGE: usize = 4;
pub const RUNELESS_MAX_BLOODCHARGE: usize = 1;
/// `SOULS_TO_REVIVE`.
pub const SOULS_TO_REVIVE: u32 = 3;
/// `SUMMON_POSSIBILITIES`.
pub const SUMMON_POSSIBILITIES: usize = 3;
/// `VALID_TERRITORY`, `CULT_PERMITTED` (`area_flags`).
const VALID_TERRITORY: u32 = 1 << 0;
const CULT_PERMITTED: u32 = 1 << 7;
/// Holy water: `deciseconds_metabolized` for the stutter and seizures (25 s) and for the
/// deconversion (1 min).
pub const HOLY_WATER_STUTTER_SECS: f32 = 25.0;
pub const HOLY_WATER_DECONVERT_SECS: f32 = 60.0;

// ---------------------------------------------------------------------------------------------
// Runes

/// A rune the dagger can scribe (`GLOB.rune_types`, in tg's `subtypesof` order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RuneKind {
    Offer,
    Empower,
    Teleport,
    NarSie,
    Revive,
    Barrier,
    Summon,
}

/// A rune type's vars.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RuneDef {
    pub path: &'static str,
    pub cultist_name: &'static str,
    pub cultist_desc: &'static str,
    pub invocation: &'static str,
    pub req_cultists: usize,
    /// Deciseconds.
    pub scribe_delay: f32,
    pub scribe_damage: f32,
    pub invoke_damage: f32,
    pub req_keyword: bool,
    /// Deciseconds.
    pub erase_time: f32,
    pub log_when_erased: bool,
}

const BASE_RUNE: RuneDef = RuneDef {
    path: "/obj/effect/rune",
    cultist_name: "basic rune",
    cultist_desc: "a basic rune with no function.",
    invocation: "Aiy ele-mayo!",
    req_cultists: 1,
    scribe_delay: 40.0,
    scribe_damage: 0.1,
    invoke_damage: 0.0,
    req_keyword: false,
    erase_time: 15.0,
    log_when_erased: false,
};

impl RuneKind {
    pub const ALL: [Self; 7] = [
        Self::Offer,
        Self::Empower,
        Self::Teleport,
        Self::NarSie,
        Self::Revive,
        Self::Barrier,
        Self::Summon,
    ];

    pub fn def(self) -> RuneDef {
        match self {
            Self::Offer => RuneDef {
                path: "/obj/effect/rune/convert",
                cultist_name: "Offer",
                cultist_desc: "offers a noncultist above it to Nar'Sie, either converting them or \
                               sacrificing them.",
                invocation: "Mah'weyh pleggh at e'ntrath!",
                ..BASE_RUNE
            },
            Self::Empower => RuneDef {
                path: "/obj/effect/rune/empower",
                cultist_name: "Empower",
                cultist_desc: "allows cultists to prepare greater amounts of blood magic at far \
                               less of a cost.",
                invocation: "H'drak v'loso, mir'kanas verbot!",
                ..BASE_RUNE
            },
            Self::Teleport => RuneDef {
                path: "/obj/effect/rune/teleport",
                cultist_name: "Teleport",
                cultist_desc: "warps everything above it to another chosen teleport rune.",
                invocation: "Sas'so c'arta forbici!",
                req_keyword: true,
                ..BASE_RUNE
            },
            Self::NarSie => RuneDef {
                path: "/obj/effect/rune/narsie",
                cultist_name: "Nar'Sie",
                cultist_desc: "tears apart dimensional barriers, calling forth the Geometer. \
                               Requires 9 invokers.",
                invocation: "TOK-LYR RQA-NAP G'OLT-ULOFT!!",
                req_cultists: 9,
                scribe_delay: 500.0,
                scribe_damage: 40.1,
                erase_time: 50.0,
                log_when_erased: true,
                ..BASE_RUNE
            },
            Self::Revive => RuneDef {
                path: "/obj/effect/rune/raise_dead",
                cultist_name: "Revive",
                cultist_desc: "requires a dead, mindless, or inactive cultist placed upon the \
                               rune. For each three bodies sacrificed to the dark patron, one \
                               body will be mended and their mind awoken",
                invocation: "Pasnar val'keriam usinar. Savrae ines amutan. Yam'toth remium \
                             il'tarat!",
                ..BASE_RUNE
            },
            Self::Barrier => RuneDef {
                path: "/obj/effect/rune/wall",
                cultist_name: "Barrier",
                cultist_desc: "when invoked, makes a temporary invisible wall to block passage. \
                               Can be invoked again to reverse this.",
                invocation: "Khari'd! Eske'te tannin!",
                ..BASE_RUNE
            },
            Self::Summon => RuneDef {
                path: "/obj/effect/rune/summon",
                cultist_name: "Summon Cultist",
                cultist_desc: "summons a single cultist to the rune. Requires 2 invokers.",
                invocation: "N'ath reth sh'yro eth d'rekkathnor!",
                req_cultists: 2,
                invoke_damage: 10.0,
                ..BASE_RUNE
            },
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.def().cultist_name == name)
    }

    pub fn from_path(path: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.def().path == path)
    }
}

/// Who the Offer rune works on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfferTarget {
    /// Alive and `is_convertable_to_cult`.
    Convert,
    /// A living human or the sacrifice target (tg "too greatly linked to the world").
    LinkedSacrifice,
    /// Anything else (the dead, mindless animals).
    Sacrifice,
}

/// The invokers the Offer rune needs for its target: two to convert, three for a living human
/// or the sacrifice target, else one.
pub fn offer_invokers_needed(target: OfferTarget) -> usize {
    match target {
        OfferTarget::Convert => 2,
        OfferTarget::LinkedSacrifice => 3,
        OfferTarget::Sacrifice => 1,
    }
}

/// `raise_dead`: a dead cultist is revived for every [`SOULS_TO_REVIVE`] sacrifices not used yet.
pub fn can_revive_dead(sacrificed: u32, used: u32) -> bool {
    sacrificed >= SOULS_TO_REVIVE + used
}

/// `blood_magic/Activate`: how many spells fit (one without an empowering rune) and how long a
/// spell is carved (deciseconds) for how much blood.
pub fn spell_limit(empowered: bool) -> usize {
    if empowered {
        MAX_BLOODCHARGE
    } else {
        RUNELESS_MAX_BLOODCHARGE
    }
}

pub fn carving(empowered: bool) -> (f32, f32) {
    if empowered {
        (40.0, 8.0)
    } else {
        (100.0, 40.0)
    }
}

/// BandaStation `check_size`: (risen, ascendent) for this many living cultists and other living
/// players.
pub fn cult_stage(cultists: u32, alive: u32) -> (bool, bool) {
    let highpop = alive >= CULT_HIGHPOP_THRESHOLD;
    let (risen, ascendent) = if highpop {
        (CULT_RISEN_HIGHPOP, CULT_ASCENDENT_HIGHPOP)
    } else {
        (CULT_RISEN_LOWPOP, CULT_ASCENDENT_LOWPOP)
    };
    let ratio = if alive > 0 {
        cultists as f32 / alive as f32
    } else {
        1.0
    };
    (ratio >= risen, ratio >= ascendent)
}

/// The stun aura's `effect_coef`: weaker as the cult shows itself.
pub fn stun_coef(risen: bool, ascendent: bool) -> f32 {
    if ascendent {
        0.1
    } else if risen {
        0.4
    } else {
        1.0
    }
}

/// A rune on the floor.
#[derive(Component, Clone, Debug)]
pub struct Rune {
    pub kind: RuneKind,
    pub keyword: Option<String>,
    /// Teleport: `listkey` (the keyword and the area's name).
    pub listkey: String,
    pub in_use: bool,
    /// Nar'Sie: invoked already.
    pub used: bool,
    /// Barrier: its shield.
    pub barrier: Option<Entity>,
}

/// The barrier rune's shield (`/obj/structure/emergency_shield/cult/barrier`).
#[derive(Component, Clone, Copy, Debug)]
pub struct CultBarrier {
    pub rune: Entity,
    pub up: bool,
}

// ---------------------------------------------------------------------------------------------
// Blood magic

/// A blood spell (`/datum/action/innate/cult/blood_spell/*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Spell {
    Stun,
    Teleport,
    Emp,
    Shackles,
    Construction,
    Dagger,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpellDef {
    pub name: &'static str,
    pub icon: &'static str,
    pub charges: u32,
    pub health_cost: f32,
    /// The aura (`magic_path`); `None`: used from the button.
    pub aura: Option<&'static str>,
    pub invocation: &'static str,
}

impl Spell {
    pub const ALL: [Self; 6] = [
        Self::Stun,
        Self::Teleport,
        Self::Emp,
        Self::Shackles,
        Self::Construction,
        Self::Dagger,
    ];

    pub fn def(self) -> SpellDef {
        match self {
            Self::Stun => SpellDef {
                name: "Stun",
                icon: "hand",
                charges: 1,
                health_cost: 10.0,
                aura: Some("/obj/item/melee/blood_magic/stun"),
                invocation: "Fuu ma'jin!",
            },
            Self::Teleport => SpellDef {
                name: "Teleport",
                icon: "tele",
                charges: 1,
                health_cost: 7.0,
                aura: Some("/obj/item/melee/blood_magic/teleport"),
                invocation: "Sas'so c'arta forbici!",
            },
            Self::Emp => SpellDef {
                name: "Electromagnetic Pulse",
                icon: "emp",
                charges: 1,
                health_cost: 10.0,
                aura: None,
                invocation: "Ta'gh fara'qha fel d'amar det!",
            },
            Self::Shackles => SpellDef {
                name: "Shadow Shackles",
                icon: "cuff",
                charges: 4,
                health_cost: 0.0,
                aura: Some("/obj/item/melee/blood_magic/shackles"),
                invocation: "In'totum Lig'abis!",
            },
            Self::Construction => SpellDef {
                name: "Twisted Construction",
                icon: "transmute",
                charges: 1,
                health_cost: 12.0,
                aura: Some("/obj/item/melee/blood_magic/construction"),
                invocation: "Ethra p'ni dedol!",
            },
            Self::Dagger => SpellDef {
                name: "Summon Ritual Dagger",
                icon: "equip",
                charges: 1,
                health_cost: 0.0,
                aura: None,
                invocation: "Wur d'dai leev'mai k'sagan!",
            },
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.def().name == name)
    }
}

/// A prepared spell.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedSpell {
    pub id: u32,
    pub spell: Spell,
    pub charges: u32,
}

/// A blood magic aura in a cultist's hand (`/obj/item/melee/blood_magic`).
#[derive(Component, Clone, Copy, Debug)]
pub struct BloodAura {
    pub owner: Entity,
    pub spell: u32,
}

// ---------------------------------------------------------------------------------------------
// The antagonist and the team

/// The cultist antagonist datum, on its body.
#[derive(Component, Clone, Debug, Default)]
pub struct Cultist {
    pub key: String,
    /// BandaStation's `/datum/antagonist/cult/converted`.
    pub converted: bool,
    pub spells: Vec<PreparedSpell>,
    next_spell: u32,
    /// `blood_magic.channeling`: a spell being carved.
    pub carving: Option<(Spell, bool)>,
    /// The rune being drawn: its kind, keyword and tile.
    pub scribing: Option<(RuneKind, Option<String>, TilePos)>,
    /// Seconds of holy water in the blood (tg `deciseconds_metabolized`).
    pub holy_secs: f32,
    /// The eye color under the cult's red.
    pub own_eyes: Option<[u8; 3]>,
    /// `/datum/antagonist/cult/shade`, `/datum/antagonist/cult/construct`.
    pub form: CultForm,
}

/// Which cult datum: the cultist, a shade or a construct.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum CultForm {
    #[default]
    Cultist,
    /// `release_time`: when it last left a soulstone (`None` inside one).
    Shade {
        released_at: Option<f64>,
    },
    Construct,
}

impl Cultist {
    fn spell(&self, id: u32) -> Option<&PreparedSpell> {
        self.spells.iter().find(|s| s.id == id)
    }
}

/// The panel's contents (`ui_static_data`), kept current by [`update_info`].
#[derive(Component, Clone, Debug, Default)]
pub struct CultInfo(pub BrotherInfo);

/// Glowing halo (`/datum/status_effect/cult_halo`): `halo1`..`halo6`.
#[derive(Component, Clone, Copy, Debug)]
pub struct CultHalo(pub u8);

/// A cult objective (`/datum/objective/sacrifice`, `/datum/objective/eldergod`).
#[derive(Clone, Debug, PartialEq)]
pub enum CultObjective {
    Sacrifice {
        target: Option<Entity>,
        name: String,
        job: String,
        sacced: bool,
    },
    Eldergod {
        /// The summoning areas: entity and name.
        spots: Vec<(Entity, String)>,
        summoned: bool,
    },
}

impl CultObjective {
    /// `update_explanation_text`.
    pub fn explanation(&self) -> String {
        match self {
            Self::Sacrifice {
                target: Some(_),
                name,
                job,
                sacced: false,
            } => format!(
                "Sacrifice {name}, {job}, via invoking an Offer rune with them on it and three \
                 acolytes around it."
            ),
            Self::Sacrifice { .. } => {
                "The veil has already been weakened here, proceed to the final objective.".into()
            }
            Self::Eldergod { spots, .. } => format!(
                "Summon Nar'Sie by invoking the rune 'Summon Nar'Sie'. The summoning can only be \
                 accomplished in {} - where the veil is weak enough for the ritual to begin.",
                english_list(spots.iter().map(|(_, n)| n.as_str()))
            ),
        }
    }

    pub fn complete(&self) -> bool {
        match self {
            Self::Sacrifice { sacced, .. } => *sacced,
            Self::Eldergod { summoned, .. } => *summoned,
        }
    }
}

/// `english_list`: "a, b and c".
fn english_list<'a>(items: impl Iterator<Item = &'a str>) -> String {
    let items: Vec<&str> = items.collect();
    match items.as_slice() {
        [] => "nothing".into(),
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Nar'Sie's round end (`begin_the_end`): the step and when it runs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Ending {
    /// `sleep(4 SECONDS)` after the invocation, then Nar'Sie appears.
    Rising {
        at: f64,
        pos: TilePos,
    },
    /// `narsie_end_begin_check`, 15 s after she rose (BandaStation).
    BeginCheck {
        at: f64,
    },
    /// `narsie_end_second_check`, 50 s later.
    SecondCheck {
        at: f64,
    },
    /// `narsie_start_destroy_station`, 5 s later.
    DestroyStation {
        at: f64,
    },
    /// `narsie_apocalypse`, a minute later.
    Apocalypse {
        at: f64,
    },
    /// `cult_ending_helper`, 12 s later: the round ends.
    End {
        at: f64,
    },
    Done,
}

/// `/datum/team/cult`: one per round.
#[derive(Resource, Clone, Debug, Default)]
pub struct CultTeam {
    pub members: Vec<Entity>,
    pub leader: Option<Entity>,
    pub objectives: Vec<CultObjective>,
    pub cult_risen: bool,
    pub cult_ascendent: bool,
    pub narsie_summoned: bool,
    pub size_at_maximum: u32,
    /// `GLOB.sacrificed`, `GLOB.sacrifices_used`.
    pub sacrificed: u32,
    pub sacrifices_used: u32,
    /// `true_cultists`: the members (bodies, keys) when Nar'Sie was summoned.
    pub true_cultists: Vec<(Entity, String)>,
    pub narsie: Option<Entity>,
    pub ending: Option<Ending>,
}

impl CultTeam {
    pub fn is_sacrifice_target(&self, mob: Entity) -> bool {
        self.objectives.iter().any(|o| {
            matches!(o, CultObjective::Sacrifice { target: Some(t), sacced: false, .. } if *t == mob)
        })
    }

    pub fn sacrifice_done(&self) -> bool {
        self.objectives
            .iter()
            .all(|o| !matches!(o, CultObjective::Sacrifice { .. }) || o.complete())
    }

    pub fn summon_spots(&self) -> Vec<(Entity, String)> {
        self.objectives
            .iter()
            .find_map(|o| match o {
                CultObjective::Eldergod { spots, .. } => Some(spots.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// `check_cult_victory`.
    pub fn victory(&self) -> bool {
        !self.objectives.is_empty() && self.objectives.iter().all(CultObjective::complete)
    }
}

// ---------------------------------------------------------------------------------------------
// Prompts

/// What a list prompt picks.
#[derive(Clone, Debug, PartialEq)]
pub enum PromptKind {
    /// The dagger's rune list.
    Rune,
    /// A teleport rune's target, for the rune's tile or the aura's target.
    Teleport {
        from_rune: Option<Entity>,
        target: Entity,
    },
    /// The summon rune's cultist.
    Summon {
        rune: Entity,
    },
    PrepareSpell {
        empowered: bool,
    },
    RemoveSpell,
}

/// A list prompt on the cultist (`tgui_input_list`); `values[i]` belongs to `items[i]`.
#[derive(Component, Clone, Debug)]
pub struct CultPrompt {
    pub kind: PromptKind,
    pub title: String,
    pub message: String,
    pub items: Vec<String>,
    pub values: Vec<Entity>,
    /// The spell ids of [`PromptKind::RemoveSpell`].
    pub spell_ids: Vec<u32>,
}

/// What a text prompt is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextKind {
    Communion,
    Keyword,
}

/// On a `tgui_input_text` prompt entity: what the cultist is typing.
#[derive(Component, Clone, Copy, Debug)]
pub struct CultText(pub TextKind);

/// The list prompt window (`ListInputWindow`).
pub struct CultListWindow;

impl UiInterface for CultListWindow {
    const NAME: &'static str = ListInputUi::NAME;
    type Data = ListInputData;
    type StaticData = ();
}

/// Timed actions of the cult.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CultJob {
    Scribe,
    Erase,
    Carve,
    Shackle,
}

pub struct CultPlugin;

impl Plugin for CultPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // The ritual dagger's rune list, a blood magic aura cast on the user.
        app.add_click_handler(ClickPhase::AttackSelf, 500, |w, c| {
            crate::antag::cult::server::attack_self(w, c.client, c.mob, c.target)
        });
        // tg `attack_hand` of a cult rune: invoked.
        app.add_click_handler(ClickPhase::EmptyHand, 300, |w, c| {
            crate::antag::cult::server::interact(w, c.client, c.mob, c.target)
        });
        // An altar, forge or archives lists its items to a cultist.
        app.add_click_handler(ClickPhase::EmptyHand, 303, |w, c| {
            crate::antag::cult::server::dispenser_hand(w, c.client, c.mob, c.target)
        });
        // Runed metal refuses a non-cultist or a place where the veil is strong (before the
        // stack's recipes).
        app.add_click_handler(ClickPhase::AttackSelf, 3233, |w, c| {
            crate::antag::cult::server::runed_metal_refused(w, c.client, c.mob, c.target)
        });
        // The ritual dagger scrapes a rune away; a blood magic aura on an object.
        app.add_click_handler(ClickPhase::HeldOnTarget, 1300, |w, c| {
            c.held.is_some_and(|h| {
                crate::antag::cult::server::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        // A soulstone used in hand releases its shade.
        app.add_click_handler(ClickPhase::AttackSelf, 509, |w, c| {
            crate::antag::cult::server::soulstone_attack_self(w, c.mob, c.target)
        });
        // A soulstone on a human, a shade or a construct shell.
        app.add_click_handler(ClickPhase::HeldOnTarget, 1309, |w, c| {
            c.held.is_some_and(|h| {
                crate::antag::cult::server::soulstone_interaction(w, c.mob, h, c.target)
            })
        });
        app.add_observer(stone_destroyed);
        app.register_ui::<AntagInfoGenericUi>()
            .register_ui::<CultListWindow>()
            .add_systems(
                Update,
                cult_clicks
                    .after(crate::security::security_clicks)
                    .before(crate::combat::MeleeSystems),
            )
            .add_systems(
                Update,
                (
                    open_info,
                    trigger_buttons,
                    list_acts,
                    text_acts.after(PromptSystems::Answer),
                )
                    .in_set(UiSystems::Handle),
            )
            .add_systems(
                Update,
                (
                    dispenser_picked
                        .in_set(UiSystems::Handle)
                        .after(PromptSystems::Answer),
                    construct_picked
                        .in_set(UiSystems::Handle)
                        .after(PromptSystems::Answer),
                    follow_stones.run_if(any_with_component::<Soulstone>),
                    (tag_pylons, pylons.run_if(any_with_component::<Pylon>))
                        .chain()
                        .before(crate::health::HealthSystems),
                ),
            )
            .add_systems(
                Update,
                (cult_life, tick_ending)
                    .chain()
                    .run_if(resource_exists::<CultTeam>)
                    .before(crate::health::HealthSystems)
                    .before(UiSystems::Open),
            );
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers

use crate::tools::tell_mob as chat_to;

fn icon(world: &World, dmi: &str, state: &str) -> Option<ss13_shared::IconStateId> {
    world
        .get_resource::<IconIndex>()
        .and_then(|index| index.get(dmi, state))
}

use crate::tools::now;

pub fn is_cultist(world: &World, mob: Entity) -> bool {
    world.get::<Cultist>(mob).is_some()
}

/// `say(invocation, forced = "cult invocation")`: heard around like speech.
fn say(world: &mut World, mob: Entity, text: &str) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let name = name_of(world, mob);
    let clients = crate::chem::clients_near(world, pos, None);
    world.write_message(SendChat {
        clients,
        message: say_message(&name, text),
    });
}

/// `whisper(text)`: heard next to the speaker.
fn whisper(world: &mut World, mob: Entity, text: &str) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let name = name_of(world, mob);
    let clients = crate::obj_damage::clients_in_range(world, pos, 1, &[]);
    world.write_message(SendChat {
        clients,
        message: whisper_message(&name, text),
    });
}

fn hurt(world: &mut World, mob: Entity, zone: Option<BodyZone>, amount: f32) {
    let (Some(mut health), Some(mut stat)) = (
        world.get::<Health>(mob).cloned(),
        world.get::<Stat>(mob).copied(),
    ) else {
        return;
    };
    let changed = match zone {
        Some(zone) => apply_damage_zone(&mut health, &mut stat, zone, DamageType::Brute, amount),
        None => apply_damage(&mut health, &mut stat, DamageType::Brute, amount),
    };
    world.entity_mut(mob).insert(health);
    if changed.is_some() {
        world.entity_mut(mob).insert(stat);
    }
}

/// `pick(GLOB.arm_zones)`.
fn arm(world: &mut World) -> BodyZone {
    let left = world
        .get_resource_mut::<crate::combat::CombatRng>()
        .is_some_and(|mut r| r.chance(50.0));
    if left { BodyZone::LArm } else { BodyZone::RArm }
}

fn active_arm(world: &World, mob: Entity) -> BodyZone {
    match world.get::<Inventory>(mob).map(|i| i.active) {
        Some(0) => BodyZone::LArm,
        _ => BodyZone::RArm,
    }
}

fn bleed(world: &mut World, mob: Entity, amount: f32) {
    if let Some(mut blood) = world.get_mut::<Blood>(mob) {
        blood.volume = (blood.volume - amount).max(0.0);
    }
}

fn held_dagger(world: &World, mob: Entity) -> Option<Entity> {
    world
        .get::<Inventory>(mob)
        .and_then(Inventory::active_item)
        .filter(|&i| path_of(world, i).as_deref() == Some(DAGGER))
}

/// `incapacitated` or dead: cannot act.
fn can_act(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, mob).incapacitated
}

fn job_title(world: &World, mob: Entity) -> Option<String> {
    let job = world.get::<PlayerJob>(mob)?.0;
    Some(
        world
            .get_resource::<Jobs>()?
            .data
            .jobs
            .get(job)?
            .title
            .clone(),
    )
}

fn job_name_ru(world: &World, mob: Entity) -> String {
    job_title(world, mob)
        .map(|t| ss13_tgui::interfaces::ru_jobs::job_ru(&t).map_or(t, str::to_owned))
        .unwrap_or_default()
}

/// Holy water in the blood: tg `TRAIT_HOLY` while it metabolizes (`can_block_magic(HOLY)`).
fn has_holy_water(world: &World, mob: Entity) -> bool {
    crate::chem::blood_amount(world, mob, HOLY_WATER) > 0.0
}

/// `is_convertable_to_cult`: a player body that is not holy (the chaplain), not the sacrifice
/// target and not mindshielded.
pub fn is_convertable(world: &World, mob: Entity) -> bool {
    if world.get::<Health>(mob).is_none() || world.get::<PlayerMob>(mob).is_none() {
        return false;
    }
    let title = job_title(world, mob);
    if title.as_deref() == Some("Chaplain") {
        return false;
    }
    if world
        .get_resource::<CultTeam>()
        .is_some_and(|t| t.is_sacrifice_target(mob))
    {
        return false;
    }
    !crate::antag::revolution::server::mindshielded(world, mob)
}

fn rng(world: &mut World, seed: u64) -> Rng {
    world
        .remove_resource::<DynamicRng>()
        .map_or_else(|| Rng::new(seed), |r| r.0)
}

#[cfg(test)]
mod tests;
