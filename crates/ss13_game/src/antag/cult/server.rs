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
// until basic mobs exist) and summon dagger; no EMP (no EMP effects yet), equipment, horror,
// veiling or blood rites. No cult master abilities (marks, final reckoning, pulse), no
// structures, no Nar'Sie language or TTS, no silence/stutter/slur effects (no speech status
// effects yet: tg's `Unconscious` and silence are a paralyze here). A sacrifice destroys the
// body (its items fall) without a soulstone. Nar'Sie is a sprite that does not move or consume:
// no singularity component. No cinematic, sounds or shuttle lockdown.

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

/// `name = "Культист"`.
pub const CULTIST_NAME: &str = "Культист";
/// `/datum/antagonist/cult/converted` `name` (BandaStation).
pub const CONVERTED_NAME: &str = "Обращенный культист";
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
    pub const ALL: [Self; 5] = [
        Self::Stun,
        Self::Teleport,
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

fn chat_to(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

fn icon(world: &World, dmi: &str, state: &str) -> Option<ss13_shared::IconStateId> {
    world
        .get_resource::<IconIndex>()
        .and_then(|index| index.get(dmi, state))
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

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

// ---------------------------------------------------------------------------------------------
// Gaining and losing the cult

/// The roundstart ruleset's `execute`: a new team, every selected player a cultist with
/// equipment, the first one its leader, then `setup_objectives` and the greetings.
pub fn start_cult(world: &mut World, mobs: &[Entity]) {
    world.insert_resource(CultTeam::default());
    let mut added = Vec::new();
    for &mob in mobs {
        if gain(world, mob, true, false) {
            added.push(mob);
        }
    }
    if let Some(&leader) = added.first() {
        make_cult_leader(world, leader);
    }
    setup_objectives(world);
    for mob in added {
        greet(world, mob);
    }
}

/// `add_antag_datum(/datum/antagonist/cult)` (admins: without equipment; the team and its
/// objectives are created with the first cultist). `false` if it already is one or cannot be.
pub fn add_cultist(world: &mut World, mob: Entity) -> bool {
    join_cult(world, mob, false)
}

/// [`add_cultist`] with the roundstart equipment (`give_equipment`: the dagger and runed metal).
pub fn add_cultist_equipped(world: &mut World, mob: Entity) -> bool {
    join_cult(world, mob, true)
}

fn join_cult(world: &mut World, mob: Entity, equipment: bool) -> bool {
    let fresh = !world.contains_resource::<CultTeam>();
    if fresh {
        world.insert_resource(CultTeam::default());
    }
    if !gain(world, mob, equipment, false) {
        return false;
    }
    if fresh {
        setup_objectives(world);
    }
    greet(world, mob);
    true
}

/// `on_gain` + `apply_innate_effects`.
fn gain(world: &mut World, mob: Entity, equipment: bool, converted: bool) -> bool {
    if world.get::<Cultist>(mob).is_some() || world.get::<Health>(mob).is_none() {
        return false;
    }
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let hud_icon = icon(world, ANTAG_HUD_DMI, "cult");
    world.entity_mut(mob).insert((
        Cultist {
            key,
            converted,
            ..default()
        },
        AntagHudData {
            hud_icon,
            ..default()
        },
    ));
    let (risen, ascendent) = {
        let mut team = world.resource_mut::<CultTeam>();
        team.members.push(mob);
        team.size_at_maximum += 1;
        (team.cult_risen, team.cult_ascendent)
    };
    if equipment {
        equip_cultist(world, mob);
    }
    if risen {
        set_eyes(world, mob, true);
    }
    if ascendent {
        set_halo(world, mob, true);
    }
    if let Some(client) = client {
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
    }
    refresh(world, mob);
    info!("{mob} joined the blood cult");
    true
}

/// `greet()`: the name and the objectives, then the panel.
fn greet(world: &mut World, mob: Entity) {
    let name = antag_name(world, mob);
    chat_to(world, mob, Span::Bold, format!("Вы - {name}."));
    let lines: Vec<String> = world
        .resource::<CultTeam>()
        .objectives
        .iter()
        .enumerate()
        .map(|(i, o)| format!("{OBJECTIVE_NAME} #{}: {}", i + 1, o.explanation()))
        .collect();
    chat_to(world, mob, Span::Notice, "Ваши текущие задачи:");
    for line in lines {
        chat_to(world, mob, Span::Plain, line);
    }
    update_info(world);
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        world.write_message(OpenUi::new::<AntagInfoGenericUi>(client, mob, mob, name));
    }
}

fn antag_name(world: &World, mob: Entity) -> &'static str {
    if world.get::<Cultist>(mob).is_some_and(|c| c.converted) {
        CONVERTED_NAME
    } else {
        CULTIST_NAME
    }
}

/// `equip_cultist(metal = TRUE)`.
fn equip_cultist(world: &mut World, mob: Entity) {
    for path in [DAGGER, "/obj/item/stack/sheet/runed_metal/ten"] {
        let name = path.rsplit('/').next().unwrap_or_default().to_owned();
        match crate::antag::brother::equip_conspicuous_item(world, mob, path) {
            Some(slot) => {
                let item = match path {
                    DAGGER => "ritual dagger",
                    _ => "runed metal",
                };
                chat_to(
                    world,
                    mob,
                    Span::Danger,
                    format!("{item} был помещен в ваш {slot}."),
                );
            }
            None => chat_to(
                world,
                mob,
                Span::BoldWarning,
                format!(
                    "К сожалению, вы не смогли получить {name}. Это очень плохо и вам стоит \
                     написать админхелп (нажмите F1)."
                ),
            ),
        }
    }
    chat_to(
        world,
        mob,
        Span::Notice,
        "Это поможет вам организовать культ на станции. Используйте их с пользой, и запомните - \
         вы не одни.",
    );
}

/// `make_cult_leader`.
fn make_cult_leader(world: &mut World, mob: Entity) {
    if world.resource::<CultTeam>().leader.is_some() {
        return;
    }
    world.resource_mut::<CultTeam>().leader = Some(mob);
    let master = icon(world, ANTAG_HUD_DMI, "cultmaster");
    if let Some(mut hud) = world.get_mut::<AntagHudData>(mob) {
        hud.hud_icon = master;
    }
    let name = name_of(world, mob);
    let members = world.resource::<CultTeam>().members.clone();
    for m in members.into_iter().filter(|&m| m != mob) {
        chat_to(
            world,
            m,
            Span::CultLarge,
            format!(
                "{name} ваш Мастер культа! Выполняйте приказы Мастера насколько это в ваших силах!"
            ),
        );
    }
    chat_to(
        world,
        mob,
        Span::CultLarge,
        "Вы - Мастер культа. Как Мастер культа у вас громче голос при коммуникации и вы можете \
         помечать цели, такие как локацию и не культиста, чтобы направить культ туда. Вы можете \
         призывать живых культистов к вашей локации единожды. Используйте свои способности для \
         победы любой ценой.",
    );
}

/// `setup_objectives`: the sacrifice target and the summoning areas.
fn setup_objectives(world: &mut World) {
    let mut rng = rng(world, 0x0c17);
    let target = find_sacrifice_target(world, &mut rng);
    let sacrifice = match target {
        Some(t) => CultObjective::Sacrifice {
            target: Some(t),
            name: name_of(world, t),
            job: job_name_ru(world, t),
            sacced: false,
        },
        // "Could not find unconvertible or convertible target. WELP!"
        None => CultObjective::Sacrifice {
            target: None,
            name: String::new(),
            job: String::new(),
            sacced: true,
        },
    };
    let spots = summon_spots(world, &mut rng);
    world.insert_resource(DynamicRng(rng));
    world.resource_mut::<CultTeam>().objectives = vec![
        sacrifice,
        CultObjective::Eldergod {
            spots,
            summoned: false,
        },
    ];
}

/// `sacrifice/find_target`: a living non-cultist human player, unconvertible ones first.
fn find_sacrifice_target(world: &mut World, rng: &mut Rng) -> Option<Entity> {
    let mut query = world.query_filtered::<(Entity, &Stat), (With<PlayerMob>, With<HumanLook>)>();
    let mut alive: Vec<Entity> = query
        .iter(world)
        .filter(|(e, s)| **s != Stat::Dead && world.get::<Cultist>(*e).is_none())
        .map(|(e, _)| e)
        .collect();
    alive.sort();
    let unconvertible: Vec<Entity> = alive
        .iter()
        .copied()
        .filter(|&e| !is_convertable(world, e))
        .collect();
    let pool = if unconvertible.is_empty() {
        alive
    } else {
        unconvertible
    };
    (!pool.is_empty()).then(|| pool[rng.range(0, pool.len() as i64 - 1) as usize])
}

/// `eldergod/New`: three station areas with `VALID_TERRITORY`.
fn summon_spots(world: &mut World, rng: &mut Rng) -> Vec<(Entity, String)> {
    let Some(areas) = world.get_resource::<Areas>() else {
        return Vec::new();
    };
    let mut candidates: Vec<(Entity, String)> = areas
        .entities()
        .iter()
        .copied()
        .filter(|&a| {
            world.get::<DmPower>(a).is_some_and(|p| {
                p.path.starts_with("/area/station") && (p.area_flags as u32) & VALID_TERRITORY != 0
            })
        })
        .map(|a| {
            let name = world
                .get::<DmName>(a)
                .map(|n| {
                    n.name
                        .trim_start_matches("\\improper ")
                        .trim_start_matches("\\proper ")
                        .to_owned()
                })
                .unwrap_or_default();
            (a, name)
        })
        .collect();
    candidates.sort_by_key(|(a, _)| *a);
    let mut spots = Vec::new();
    while spots.len() < SUMMON_POSSIBILITIES && !candidates.is_empty() {
        let i = rng.range(0, candidates.len() as i64 - 1) as usize;
        spots.push(candidates.remove(i));
    }
    spots
}

/// `on_removal` + `remove_innate_effects`: the old faith returns.
pub fn remove_cultist(world: &mut World, mob: Entity, silent: bool) {
    let Some(cultist) = world.get::<Cultist>(mob).cloned() else {
        return;
    };
    if !silent {
        let name = name_of(world, mob);
        visible(
            world,
            mob,
            "Незнакомый белый свет вспыхивает в твоем сознании, очищая от скверны Геометра и всех \
             твоих воспоминаний о том, что ты был её слугой.",
            &format!("Старая вера вернулась к {name}!"),
            Span::BoldWarning,
        );
    }
    drop_auras(world, mob);
    if cultist.own_eyes.is_some() {
        set_eyes(world, mob, false);
    }
    set_halo(world, mob, false);
    world
        .entity_mut(mob)
        .remove::<(Cultist, AntagHudData, CultPrompt, CultInfo)>();
    set_buttons(world, mob, Vec::new());
    if let Some(mut team) = world.get_resource_mut::<CultTeam>() {
        team.members.retain(|&m| m != mob);
        if team.leader == Some(mob) {
            team.leader = None;
        }
    }
    info!("{mob} renounced the cult of Nar'Sie");
}

/// `/datum/element/cult_eyes`: red eyes (or back to the own color).
fn set_eyes(world: &mut World, mob: Entity, on: bool) {
    let Some(look) = world.get::<HumanLook>(mob).cloned() else {
        return;
    };
    let restore = world.get::<Cultist>(mob).and_then(|c| c.own_eyes);
    let color = if on {
        if let Some(mut c) = world.get_mut::<Cultist>(mob)
            && c.own_eyes.is_none()
        {
            c.own_eyes = Some(look.eye_color);
        }
        BLOODCULT_EYE
    } else {
        match restore {
            Some(c) => c,
            None => return,
        }
    };
    if let Some(mut l) = world.get_mut::<HumanLook>(mob) {
        l.eye_color = color;
    }
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.set_changed();
    }
}

/// `/datum/status_effect/cult_halo` on or off.
fn set_halo(world: &mut World, mob: Entity, on: bool) {
    let had = world.get::<CultHalo>(mob).is_some();
    if on == had {
        return;
    }
    if on {
        let n = world
            .get_resource_mut::<crate::combat::CombatRng>()
            .map_or(1, |mut r| r.range(1, 6) as u8);
        world.entity_mut(mob).insert(CultHalo(n));
    } else {
        world.entity_mut(mob).remove::<CultHalo>();
    }
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.set_changed();
    }
}

/// The halo overlay for `crate::inventory`'s look refresh.
pub fn halo_overlay(icons: &IconIndex, halo: CultHalo) -> Option<(f32, ss13_shared::IconStateId)> {
    // `-HALO_LAYER`: above everything else on the mob.
    icons
        .get(HALO_DMI, &format!("halo{}", halo.0))
        .map(|id| (1.1, id))
}

/// `check_size`: eyes at the risen ratio, halos at the ascendent one.
pub fn check_size(world: &mut World) {
    let Some(team) = world.get_resource::<CultTeam>() else {
        return;
    };
    if team.cult_ascendent {
        return;
    }
    let mut query = world.query_filtered::<(Entity, &Stat), With<PlayerMob>>();
    let (mut cultists, mut alive) = (0u32, 0u32);
    for (e, stat) in query.iter(world) {
        if *stat == Stat::Dead {
            continue;
        }
        if world.get::<Cultist>(e).is_some() {
            cultists += 1;
        } else {
            alive += 1;
        }
    }
    if cultists == 0 {
        return;
    }
    let (risen, ascendent) = cult_stage(cultists, alive);
    let team = world.resource::<CultTeam>().clone();
    if risen && !team.cult_risen {
        world.resource_mut::<CultTeam>().cult_risen = true;
        for &m in &team.members {
            chat_to(
                world,
                m,
                Span::CultLarge,
                "Покров слабеет с ростом культа - ваши глаза начинают светиться...",
            );
            set_eyes(world, m, true);
        }
        info!("the blood cult has risen with {cultists} players");
    }
    if ascendent && !team.cult_ascendent {
        world.resource_mut::<CultTeam>().cult_ascendent = true;
        for &m in &team.members {
            chat_to(
                world,
                m,
                Span::CultLarge,
                "Ваш культ набирает силу, и приближается красная жатва - вы не сможете долго \
                 скрывать свою истинную природу!!",
            );
            set_halo(world, m, true);
        }
        let percent = (cultists as f32 / alive.max(1) as f32 * 100.0).floor() as u32;
        crate::shuttle::priority_announce(
            world,
            &format!(
                "{}: Отдел паранормальных явлений",
                crate::events::COMMAND_NAME
            ),
            "",
            &format!(
                "Мы фиксируем активность из другого измерения, связаную с культом \"Нар'Си\" на \
                 вашей станции. Согласно нашей информации, {percent}% экипажа станции были \
                 порабощены культом. Сотрудники службы безопасности наделены правом \
                 беспрепятственно применять летальную силу против культистов. Остальному экипажу \
                 надлежит приготовиться защищать себя и свои отделы, не ведя охоту на культистов. \
                 Погибшие члены экипажа должны быть реанимированы и деконвертированы, как только \
                 ситуация будет взята под контроль."
            ),
        );
        info!("the blood cult has ascended with {cultists} players");
    }
}

// ---------------------------------------------------------------------------------------------
// Buttons

const COMM_ID: &str = "cult_comm";
const MAGIC_ID: &str = "cult_blood_magic";
const SPELL_PREFIX: &str = "cult_spell_";

/// The cultist's buttons: Communion, Prepare Blood Magic and the prepared spells.
pub fn refresh(world: &mut World, mob: Entity) {
    let Some(cultist) = world.get::<Cultist>(mob).cloned() else {
        return;
    };
    let (background, border) = (
        icon(world, BACKGROUNDS_DMI, "bg_demon"),
        icon(world, BACKGROUNDS_DMI, "bg_demon_border"),
    );
    let in_hand: Vec<u32> = world
        .get::<Inventory>(mob)
        .map(|i| {
            i.hands
                .iter()
                .flatten()
                .filter_map(|&h| world.get::<BloodAura>(h).map(|a| a.spell))
                .collect()
        })
        .unwrap_or_default();
    let button = |id: String, name: &str, icon, maptext: Option<String>, active: bool| {
        (
            ActionButton {
                id,
                name: name.to_owned(),
                icon,
                background,
                border,
                available: true,
                cooldown: 0.0,
                active,
                maptext,
            },
            0.0,
        )
    };
    let mut buttons = vec![
        button(
            COMM_ID.into(),
            "Communion",
            icon(world, ACTIONS_DMI, "cult_comms"),
            None,
            false,
        ),
        button(
            MAGIC_ID.into(),
            "Prepare Blood Magic",
            icon(world, ACTIONS_DMI, "carve"),
            None,
            cultist.carving.is_some(),
        ),
    ];
    for spell in &cultist.spells {
        let def = spell.spell.def();
        buttons.push(button(
            format!("{SPELL_PREFIX}{}", spell.id),
            def.name,
            icon(world, ACTIONS_DMI, def.icon),
            (def.charges > 1).then(|| spell.charges.to_string()),
            in_hand.contains(&spell.id),
        ));
    }
    set_buttons(world, mob, buttons);
}

fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world.get::<Cultist>(actor).is_none()
        {
            continue;
        }
        // `AB_CHECK_CONSCIOUS|AB_CHECK_INCAPACITATED`.
        if !can_act(world, actor) {
            continue;
        }
        if id == COMM_ID {
            open_text(world, client, actor, TextKind::Communion);
        } else if id == MAGIC_ID {
            prepare_magic(world, client, actor);
        } else if let Some(n) = id
            .strip_prefix(SPELL_PREFIX)
            .and_then(|n| n.parse::<u32>().ok())
        {
            activate_spell(world, client, actor, n);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Prompts

fn open_list(world: &mut World, client: Entity, mob: Entity, prompt: CultPrompt) {
    let title = prompt.title.clone();
    world.entity_mut(mob).insert(prompt);
    world.write_message(OpenUi::new::<CultListWindow>(client, mob, mob, &title));
}

fn open_text(world: &mut World, client: Entity, mob: Entity, kind: TextKind) {
    let (title, message, max_length) = match kind {
        TextKind::Communion => (
            "Voice of Blood",
            "Message to tell to the other acolytes",
            ss13_chat::MAX_MESSAGE_LEN as u32,
        ),
        // `MAX_NAME_LEN`.
        TextKind::Keyword => ("Words of Power", "Keyword for the new rune", 42),
    };
    let ask = Ask::new(
        client,
        mob,
        title,
        message,
        prompt::PromptKind::Text {
            default: String::new(),
            max_length,
            multiline: false,
        },
    );
    let prompt = ask_world(world, ask);
    world.entity_mut(prompt).insert(CultText(kind));
}

impl ServerUi for CultListWindow {
    type Source = &'static CultPrompt;
    type UserParam = ();
    type Act = Option<String>;

    fn static_data(_: &CultPrompt) {}

    fn data(prompt: &CultPrompt, _: Entity, _: &()) -> ListInputData {
        ListInputData {
            title: prompt.title.clone(),
            message: prompt.message.clone(),
            init_value: prompt.items.first().cloned().unwrap_or_default(),
            items: prompt.items.clone(),
            timeout: None,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<Option<String>, UiReject> {
        parse_entry(action, params)
    }
}

fn parse_entry(action: &str, params: &UiParams) -> Result<Option<String>, UiReject> {
    match action {
        SUBMIT => params.text("entry").map(Some),
        CANCEL => Ok(None),
        _ => Err(UiReject::UnknownAction),
    }
}

fn close_window(world: &mut World, window: Entity) {
    if let Some(mut window) = world.get_mut::<UiWindow>(window) {
        window.cap_status(ss13_shared::ui::UiStatus::Close);
    }
}

fn list_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<CultListWindow>>>) {
    let pending: Vec<(Entity, Entity, Entity, Entity, Option<String>)> = acts
        .read(world.resource::<Messages<UiActed<CultListWindow>>>())
        .map(|a| (a.window, a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (window, client, actor, source, pick) in pending {
        close_window(world, window);
        let Some(prompt) = world.entity_mut(source).take::<CultPrompt>() else {
            continue;
        };
        let Some(pick) = pick else {
            continue;
        };
        if actor != source || world.get::<Cultist>(actor).is_none() || !can_act(world, actor) {
            continue;
        }
        let Some(i) = prompt.items.iter().position(|it| *it == pick) else {
            continue;
        };
        match prompt.kind {
            PromptKind::Rune => {
                if let Some(kind) = RuneKind::from_name(&pick) {
                    chose_rune(world, client, actor, kind);
                }
            }
            PromptKind::Teleport { from_rune, target } => {
                teleport_to(world, client, actor, from_rune, target, prompt.values[i]);
            }
            PromptKind::Summon { rune } => summon_cultist(world, actor, rune, prompt.values[i]),
            PromptKind::PrepareSpell { empowered } => {
                if let Some(spell) = Spell::from_name(&pick) {
                    start_carving(world, client, actor, spell, empowered);
                } else if pick == "(REMOVE SPELL)" {
                    open_remove_spell(world, client, actor);
                }
            }
            PromptKind::RemoveSpell => {
                if let Some(&id) = prompt.spell_ids.get(i) {
                    remove_spell(world, actor, id);
                }
            }
        }
    }
}

fn text_acts(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in pending {
        let Some(&CultText(kind)) = world.get::<CultText>(prompt) else {
            continue;
        };
        let Some(PromptAnswer::Text(entry)) = answer else {
            continue;
        };
        if world.get::<Cultist>(actor).is_none() || !can_act(world, actor) {
            continue;
        }
        let Ok(entry) = ss13_chat::server::sanitize_say(&entry) else {
            continue;
        };
        match kind {
            TextKind::Communion => commune(world, actor, &entry),
            TextKind::Keyword => {
                let entry: String = entry.chars().take(42).collect();
                begin_scribe(world, client, actor, RuneKind::Teleport, Some(entry));
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Communion

/// `cultist_commune`: whispered aloud, read by every cultist and the dead.
pub fn commune(world: &mut World, mob: Entity, message: &str) {
    let invocation = if world
        .get_resource_mut::<crate::combat::CombatRng>()
        .is_some_and(|mut r| r.chance(50.0))
    {
        "O bidai nabora se'sma!"
    } else {
        "O bidai nabora se`sma!"
    };
    whisper(world, mob, invocation);
    whisper(world, mob, message);
    let leader = world.resource::<CultTeam>().leader == Some(mob);
    let (title, span) = if leader {
        ("Master", Span::CultLarge)
    } else if world.get::<HumanLook>(mob).is_some() {
        ("Acolyte", Span::CultItalic)
    } else {
        ("Construct", Span::CultItalic)
    };
    let name = name_of(world, mob);
    let members = world.resource::<CultTeam>().members.clone();
    let mut ghosts = world.query_filtered::<Entity, With<Ghost>>();
    let ghosts: Vec<Entity> = ghosts.iter(world).collect();
    let listeners = world.resource::<Listeners>();
    let clients: Vec<Entity> = members
        .into_iter()
        .chain(ghosts)
        .filter_map(|e| listeners.client_of(e))
        .collect();
    info!(target: "say", "cult {name}: {message}");
    world.write_message(SendChat {
        clients,
        message: commune_message(title, &name, message, span),
    });
}

/// `<span class='cult italic'><b>Acolyte Name:</b> message</span>`.
pub fn commune_message(title: &str, name: &str, text: &str, span: Span) -> ChatMessage {
    ChatMessage {
        channel: ChatChannel::Radio,
        segments: vec![
            ChatSegment::new(Span::Bold, format!("{title} {name}: ")),
            ChatSegment::new(span, text),
        ],
    }
}

// ---------------------------------------------------------------------------------------------
// The ritual dagger and scribing

/// `attack_self` of the dagger (the rune list) or of a blood magic aura (cast on the user).
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if let Some(aura) = world.get::<BloodAura>(item).copied() {
        cast_aura(world, client, mob, item, aura, mob);
        return true;
    }
    if path_of(world, item).as_deref() != Some(DAGGER) {
        return false;
    }
    if world.get::<Cultist>(mob).is_none() {
        let name = name_of(world, item);
        tell(
            world,
            client,
            Span::Warning,
            format!("{name} is covered in unintelligible shapes and markings."),
        );
        return true;
    }
    if world
        .get::<Cultist>(mob)
        .is_some_and(|c| c.scribing.is_some())
    {
        tell(
            world,
            client,
            Span::Warning,
            "You are already drawing a rune.",
        );
        return true;
    }
    if !can_scribe(world, client, mob) {
        return true;
    }
    let items: Vec<String> = RuneKind::ALL
        .iter()
        .map(|k| k.def().cultist_name.to_owned())
        .collect();
    let values = vec![mob; items.len()];
    open_list(
        world,
        client,
        mob,
        CultPrompt {
            kind: PromptKind::Rune,
            title: "Sigils of Power".into(),
            message: "Choose a rite to scribe".into(),
            items,
            values,
            spell_ids: Vec::new(),
        },
    );
    true
}

/// `can_scribe_rune` + `check_rune_turf`.
fn can_scribe(world: &mut World, client: Entity, mob: Entity) -> bool {
    if held_dagger(world, mob).is_none() && !held_any_dagger(world, mob) {
        return false;
    }
    if !can_act(world, mob) {
        tell(
            world,
            client,
            Span::Warning,
            "You can't draw a rune right now.",
        );
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return false;
    };
    let turf = crate::construction::turf_at(world, pos).and_then(|t| path_of(world, t));
    if turf
        .as_deref()
        .is_some_and(|p| p.starts_with("/turf/open/space"))
    {
        tell(
            world,
            client,
            Span::Warning,
            "You cannot scribe runes in space!",
        );
        return false;
    }
    if rune_at(world, pos).is_some() {
        tell(world, client, Span::Cult, "There is already a rune here.");
        return false;
    }
    let permitted = area_flags(world, pos).is_none_or(|f| f & CULT_PERMITTED != 0);
    if !permitted {
        tell(
            world,
            client,
            Span::Warning,
            "The veil is not weak enough here.",
        );
        return false;
    }
    true
}

fn held_any_dagger(world: &World, mob: Entity) -> bool {
    world.get::<Inventory>(mob).is_some_and(|i| {
        i.hands
            .iter()
            .flatten()
            .any(|&h| path_of(world, h).as_deref() == Some(DAGGER))
    })
}

fn area_of(world: &World, pos: TilePos) -> Option<Entity> {
    world.get_resource::<Areas>()?.area_at(pos)
}

fn area_flags(world: &World, pos: TilePos) -> Option<u32> {
    let area = area_of(world, pos)?;
    world.get::<DmPower>(area).map(|p| p.area_flags as u32)
}

fn area_name(world: &World, pos: TilePos) -> String {
    area_of(world, pos)
        .and_then(|a| world.get::<DmName>(a))
        .map(|n| {
            n.name
                .trim_start_matches("\\improper ")
                .trim_start_matches("\\proper ")
                .to_owned()
        })
        .unwrap_or_default()
}

pub fn rune_at(world: &World, pos: TilePos) -> Option<Entity> {
    world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<Rune>(e).is_some())
}

/// The picked rune: a keyword first for the teleport rune; the Nar'Sie rune's checks.
fn chose_rune(world: &mut World, client: Entity, mob: Entity, kind: RuneKind) {
    if !can_scribe(world, client, mob) {
        return;
    }
    if kind.def().req_keyword {
        open_text(world, client, mob, TextKind::Keyword);
        return;
    }
    begin_scribe(world, client, mob, kind, None);
}

/// `do_scribe_rune` after the choices: the arm is cut and the `do_after` starts.
fn begin_scribe(
    world: &mut World,
    client: Entity,
    mob: Entity,
    kind: RuneKind,
    keyword: Option<String>,
) {
    if !can_scribe(world, client, mob) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    if kind == RuneKind::Summon && area_of(world, pos).is_none() {
        tell(
            world,
            client,
            Span::CultItalic,
            "The veil is not weak enough here to summon a cultist, you must be on station!",
        );
        return;
    }
    if kind == RuneKind::NarSie && !scribe_narsie_rune(world, client, mob, pos) {
        return;
    }
    let def = kind.def();
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        "You slice open your arm and begin drawing a sigil of the Geometer.",
        &format!("{user} cuts open their arm and begins writing in their own blood!"),
        Span::Warning,
    );
    let zone = arm(world);
    hurt(world, mob, Some(zone), def.scribe_damage);
    let item = world.get::<Inventory>(mob).and_then(Inventory::active_item);
    if let Some(mut c) = world.get_mut::<Cultist>(mob) {
        c.scribing = Some((kind, keyword, pos));
    }
    let actor = Actor { client, mob, item };
    if !do_after(
        world,
        actor,
        mob,
        def.scribe_delay,
        0,
        Job::Cult(CultJob::Scribe),
    ) && let Some(mut c) = world.get_mut::<Cultist>(mob)
    {
        c.scribing = None;
    }
}

/// `scribe_narsie_rune`: in a ritual site, after the sacrifice, before the summoning; CentCom
/// is told.
fn scribe_narsie_rune(world: &mut World, client: Entity, mob: Entity, pos: TilePos) -> bool {
    let team = world.resource::<CultTeam>().clone();
    let spots = team.summon_spots();
    let here = area_of(world, pos);
    if spots.is_empty() {
        tell(
            world,
            client,
            Span::Warning,
            "There are no ritual sites on this station to scribe this rune!",
        );
        return false;
    }
    if !spots.iter().any(|(a, _)| Some(*a) == here) {
        let names = english_list(spots.iter().map(|(_, n)| n.as_str()));
        tell(
            world,
            client,
            Span::Warning,
            format!("The ritual can only begin where the veil is weak - in {names}!"),
        );
        return false;
    }
    if !team.sacrifice_done() {
        tell(
            world,
            client,
            Span::Warning,
            "The sacrifice is not complete. The portal would lack the power to open if you tried!",
        );
        return false;
    }
    if team.narsie_summoned {
        tell(
            world,
            client,
            Span::CultLarge,
            "\"I am already here. There is no need to try to summon me now.\"",
        );
        return false;
    }
    let name = name_of(world, mob);
    let area = area_name(world, pos);
    crate::shuttle::priority_announce(
        world,
        &format!(
            "{}: Отдел паранормальных явлений",
            crate::events::COMMAND_NAME
        ),
        "",
        &format!(
            "Зафиксирован призыв древнего божества культистом {name} в {area}. Прервите ритуал \
             любой ценой!"
        ),
    );
    true
}

/// [`Job::Cult`] completions.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: CultJob) {
    match job {
        CultJob::Scribe => finish_scribe(world, mob, action),
        CultJob::Erase => finish_erase(world, mob, action),
        CultJob::Carve => finish_carving(world, mob),
        CultJob::Shackle => finish_shackle(world, mob, action),
    }
}

fn finish_scribe(world: &mut World, mob: Entity, action: &DoAfter) {
    let Some((kind, keyword, pos)) = world
        .get_mut::<Cultist>(mob)
        .and_then(|mut c| c.scribing.take())
    else {
        return;
    };
    if !can_scribe(world, action.client, mob) || world.get::<TilePos>(mob) != Some(&pos) {
        return;
    }
    let Some(rune) = spawn_rune(world, kind, pos, keyword) else {
        return;
    };
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        "You finish drawing the arcane markings of the Geometer.",
        &format!("{user} creates a strange circle in their own blood."),
        Span::Warning,
    );
    let def = kind.def();
    chat_to(
        world,
        mob,
        Span::Cult,
        format!(
            "The {} rune {}",
            def.cultist_name.to_lowercase(),
            def.cultist_desc
        ),
    );
    info!(
        "{mob} scribed a {} rune ({rune}) at {pos:?}",
        def.cultist_name
    );
}

/// A new rune of `kind` at `pos` (`new rune_to_scribe(turf, keyword)`).
pub fn spawn_rune(
    world: &mut World,
    kind: RuneKind,
    pos: TilePos,
    keyword: Option<String>,
) -> Option<Entity> {
    let rune = crate::construction::spawn_obj(world, kind.def().path, pos)?;
    let area = area_name(world, pos);
    let listkey = match &keyword {
        Some(k) => format!("{k} {area}"),
        None => area,
    };
    world.entity_mut(rune).insert(Rune {
        kind,
        keyword,
        listkey,
        in_use: false,
        used: false,
        barrier: None,
    });
    Some(rune)
}

/// An admin's spawn: a rune gets its state like a scribed one (`new rune_type(loc)`).
// shortcut: runes placed on maps get no rune state (none of the station maps place any).
pub fn spawn_obj_or_rune(world: &mut World, path: &str, pos: TilePos) -> Option<Entity> {
    match RuneKind::from_path(path) {
        Some(kind) => spawn_rune(world, kind, pos, None),
        None => crate::construction::spawn_obj(world, path, pos),
    }
}

/// The dagger on a rune (scrape it away) — `item_interaction` before the inventory's own.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if let Some(aura) = world.get::<BloodAura>(held).copied() {
        if world.get::<Health>(target).is_none() {
            cast_aura(world, client, mob, held, aura, target);
        }
        return true;
    }
    if world.get::<Rune>(target).is_none() || path_of(world, held).as_deref() != Some(DAGGER) {
        return false;
    }
    if world.get::<Cultist>(mob).is_none() {
        return false;
    }
    let erase = world.get::<Rune>(target).map(|r| r.kind.def().erase_time);
    let item = Some(held);
    let actor = Actor { client, mob, item };
    do_after(
        world,
        actor,
        target,
        erase.unwrap_or(15.0),
        0,
        Job::Cult(CultJob::Erase),
    );
    true
}

fn finish_erase(world: &mut World, mob: Entity, action: &DoAfter) {
    let rune = action.target;
    let Some(kind) = world.get::<Rune>(rune).map(|r| r.kind) else {
        return;
    };
    if world.get::<Cultist>(mob).is_none() || !can_act(world, mob) {
        return;
    }
    chat_to(
        world,
        mob,
        Span::Notice,
        format!(
            "You carefully erase the {} rune.",
            kind.def().cultist_name.to_lowercase()
        ),
    );
    delete_rune(world, rune);
}

fn delete_rune(world: &mut World, rune: Entity) {
    if let Some(barrier) = world.get::<Rune>(rune).and_then(|r| r.barrier)
        && world.get_entity(barrier).is_ok()
    {
        world.despawn(barrier);
    }
    if world.get_entity(rune).is_ok() {
        world.despawn(rune);
    }
}

// ---------------------------------------------------------------------------------------------
// Invoking runes

/// An empty hand on a rune (`attack_hand`).
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(rune) = world.get::<Rune>(target).cloned() else {
        return false;
    };
    if world.get::<Cultist>(mob).is_none() {
        let name = name_of(world, target);
        tell(
            world,
            client,
            Span::Warning,
            format!("You aren't able to understand the words of {name}."),
        );
        return true;
    }
    let invokers = can_invoke(world, target, mob, rune.kind);
    let req = rune.kind.def().req_cultists;
    if invokers.len() >= req {
        invoke(world, client, target, &invokers);
    } else {
        tell(
            world,
            client,
            Span::Danger,
            format!(
                "You need {} more adjacent cultists to use this rune in such a manner.",
                req - invokers.len()
            ),
        );
        fail_invoke(world, target);
    }
    true
}

/// `can_invoke`: the user, then (for multi-invoker runes and Offer) every conscious cultist
/// next to the rune.
pub fn can_invoke(world: &mut World, rune: Entity, user: Entity, kind: RuneKind) -> Vec<Entity> {
    let mut invokers = vec![user];
    if kind.def().req_cultists <= 1 && kind != RuneKind::Offer {
        return invokers;
    }
    let Some(&at) = world.get::<TilePos>(rune) else {
        return invokers;
    };
    let mut query = world.query_filtered::<(Entity, &TilePos, &Stat), With<Cultist>>();
    let mut others: Vec<Entity> = query
        .iter(world)
        .filter(|(e, p, s)| {
            *e != user
                && p.z == at.z
                && (p.x - at.x).abs() <= 1
                && (p.y - at.y).abs() <= 1
                && **s == Stat::Conscious
        })
        .map(|(e, ..)| e)
        .collect();
    others.sort();
    invokers.extend(others);
    invokers
}

/// The base `invoke`: every invoker says the words and takes `invoke_damage`.
fn chant(world: &mut World, kind: RuneKind, invokers: &[Entity], invocation: &str) {
    let def = kind.def();
    for &mob in invokers {
        say(world, mob, invocation);
        if def.invoke_damage > 0.0 {
            hurt(world, mob, None, def.invoke_damage);
            chat_to(world, mob, Span::CultItalic, "rune saps your strength!");
        }
    }
}

/// `fail_invoke`.
fn fail_invoke(world: &mut World, rune: Entity) {
    let Some(&pos) = world.get::<TilePos>(rune) else {
        return;
    };
    let clients = crate::chem::clients_near(world, pos, None);
    world.write_message(SendChat {
        clients,
        message: crate::chem::line(
            Span::Warning,
            "The markings pulse with a small flash of red light, then fall dark.",
        ),
    });
}

fn invoke(world: &mut World, client: Entity, rune: Entity, invokers: &[Entity]) {
    let Some(kind) = world.get::<Rune>(rune).map(|r| r.kind) else {
        return;
    };
    match kind {
        RuneKind::Offer => invoke_offer(world, rune, invokers),
        RuneKind::Empower => {
            chant(world, kind, invokers, kind.def().invocation);
            prepare_magic(world, client, invokers[0]);
        }
        RuneKind::Teleport => invoke_teleport(world, client, rune, invokers[0]),
        RuneKind::Revive => invoke_revive(world, rune, invokers),
        RuneKind::Barrier => invoke_barrier(world, rune, invokers),
        RuneKind::Summon => invoke_summon(world, client, rune, invokers),
        RuneKind::NarSie => invoke_narsie(world, rune, invokers),
    }
}

/// `convert/invoke`: a non-cultist on the rune is converted (alive, convertible, two invokers)
/// or sacrificed.
fn invoke_offer(world: &mut World, rune: Entity, invokers: &[Entity]) {
    let Some(&pos) = world.get::<TilePos>(rune) else {
        return;
    };
    if world.get::<Rune>(rune).is_some_and(|r| r.in_use) {
        return;
    }
    let mut targets: Vec<Entity> = world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| world.get::<Health>(e).is_some() && world.get::<Cultist>(e).is_none())
        .collect();
    targets.sort();
    let Some(&target) = targets.first() else {
        fail_invoke(world, rune);
        return;
    };
    let alive = world.get::<Stat>(target) != Some(&Stat::Dead);
    if alive && is_convertable(world, target) {
        chant(
            world,
            RuneKind::Offer,
            invokers,
            "Mah'weyh pleggh at e'ntrath!",
        );
        do_convert(world, rune, target, invokers);
    } else {
        chant(world, RuneKind::Offer, invokers, "Barhah hra zar'garis!");
        do_sacrifice(world, target, invokers);
    }
    check_size(world);
}

fn do_convert(world: &mut World, rune: Entity, target: Entity, invokers: &[Entity]) -> bool {
    let victim = name_of(world, target);
    if invokers.len() < offer_invokers_needed(OfferTarget::Convert) {
        for &i in invokers {
            chat_to(
                world,
                i,
                Span::Warning,
                format!("You need at least two invokers to convert {victim}!"),
            );
        }
        return false;
    }
    if has_holy_water(world, target) {
        for &i in invokers {
            chat_to(
                world,
                i,
                Span::Warning,
                format!("Something is shielding {victim}'s mind!"),
            );
        }
        return false;
    }
    let health = world.get::<Health>(target).cloned().unwrap_or_default();
    let (brute, burn) = (health.brute, health.burn);
    if brute > 0.0 || burn > 0.0 {
        let mut stat = world.get::<Stat>(target).copied().unwrap_or_default();
        let mut health = health;
        health.adjust(DamageType::Brute, -(brute * 0.75));
        health.adjust(DamageType::Burn, -(burn * 0.75));
        crate::health::update_stat(&health, &mut stat);
        world.entity_mut(target).insert((health, stat));
    }
    let wounds = brute > 0.0 || burn > 0.0;
    visible(
        world,
        target,
        "AAAAAAAAAAAAAA-",
        &format!(
            "{victim} writhes in pain {}!",
            if wounds {
                "even as their wounds heal and close"
            } else {
                "as the markings below them glow a bloody red"
            }
        ),
        Span::Warning,
    );
    if let Some(&pos) = world.get::<TilePos>(target) {
        crate::restraints::uncuff(world, target, pos);
    }
    // `Unconscious(10 SECONDS)`.
    crate::status_effects::apply(world, target, Incap::Paralyze, 10.0);
    if let Some(&pos) = world.get::<TilePos>(rune) {
        crate::construction::spawn_obj(world, DAGGER, pos);
    }
    gain(world, target, false, true);
    greet(world, target);
    chat_to(
        world,
        target,
        Span::CultItalic,
        "Your blood pulses. Your head throbs. The world goes red. All at once you are aware of a \
         horrible, horrible, truth. The veil of reality has been ripped away and something evil \
         takes root.",
    );
    chat_to(
        world,
        target,
        Span::CultItalic,
        "Assist your new compatriots in their dark dealings. Your goal is theirs, and theirs is \
         yours. You serve the Geometer above all else. Bring it back.",
    );
    info!("{target} was converted to the cult by {invokers:?}");
    true
}

fn do_sacrifice(world: &mut World, target: Entity, invokers: &[Entity]) -> bool {
    let victim = name_of(world, target);
    let human = world.get::<HumanLook>(target).is_some();
    let alive = world.get::<Stat>(target) != Some(&Stat::Dead);
    let is_target = world.resource::<CultTeam>().is_sacrifice_target(target);
    let linked = (human && alive) || is_target;
    let needed = offer_invokers_needed(if linked {
        OfferTarget::LinkedSacrifice
    } else {
        OfferTarget::Sacrifice
    });
    if invokers.len() < needed {
        for &i in invokers {
            chat_to(
                world,
                i,
                Span::CultItalic,
                format!("{victim} is too greatly linked to the world! You need three acolytes!"),
            );
        }
        return false;
    }
    {
        let mut team = world.resource_mut::<CultTeam>();
        team.sacrificed += 1;
        for o in &mut team.objectives {
            if let CultObjective::Sacrifice {
                target: t, sacced, ..
            } = o
                && *t == Some(target)
            {
                *sacced = true;
                *t = None;
            }
        }
    }
    for &i in invokers {
        let text = if is_target {
            "\"Yes! This is the one I desire! You have done well.\""
        } else if human {
            "\"I accept this sacrifice.\""
        } else {
            "\"I accept this meager sacrifice.\""
        };
        chat_to(world, i, Span::CultLarge, text);
    }
    gib(world, target);
    update_info(world);
    info!("{target} was sacrificed to Nar'Sie (target: {is_target})");
    true
}

/// `gib(DROP_ALL_REMAINS)`: the body is gone, its items fall; its player becomes a ghost.
fn gib(world: &mut World, mob: Entity) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    if world.get::<Cultist>(mob).is_some() {
        remove_cultist(world, mob, true);
    }
    world.entity_mut(mob).insert(Stat::Dead);
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client)
        && let Some(ghost) = crate::ghost::ghostize_client(world, client)
        && let Some(mut g) = world.get_mut::<Ghost>(ghost)
    {
        g.body = None;
    }
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        let items: Vec<Entity> = crate::inventory::ALL_SLOTS
            .iter()
            .filter_map(|&s| inventory.slot(s))
            .collect();
        for item in items {
            if world.get::<NoDrop>(item).is_some() {
                world.despawn(item);
            } else {
                crate::inventory::put_down(world, item, pos);
            }
        }
    }
    if let Some(cuffs) = world.get::<crate::restraints::Handcuffed>(mob).map(|h| h.0) {
        crate::inventory::put_down(world, cuffs, pos);
    }
    world.despawn(mob);
}

/// `teleport/invoke`: the list of the other teleport runes.
fn invoke_teleport(world: &mut World, client: Entity, rune: Entity, user: Entity) {
    let (items, values) = teleport_runes(world, Some(rune));
    if items.is_empty() {
        tell(
            world,
            client,
            Span::Warning,
            "There are no valid runes to teleport to!",
        );
        fail_invoke(world, rune);
        return;
    }
    open_list(
        world,
        client,
        user,
        CultPrompt {
            kind: PromptKind::Teleport {
                from_rune: Some(rune),
                target: user,
            },
            title: "Teleportation Target".into(),
            message: "Rune to teleport to".into(),
            items,
            values,
            spell_ids: Vec::new(),
        },
    );
}

/// `GLOB.teleport_runes` but `except`, by `listkey` (`avoid_assoc_duplicate_keys`).
fn teleport_runes(world: &mut World, except: Option<Entity>) -> (Vec<String>, Vec<Entity>) {
    let mut query = world.query::<(Entity, &Rune)>();
    let mut runes: Vec<(Entity, String)> = query
        .iter(world)
        .filter(|(e, r)| r.kind == RuneKind::Teleport && Some(*e) != except)
        .map(|(e, r)| (e, r.listkey.clone()))
        .collect();
    runes.sort_by_key(|(e, _)| *e);
    let mut items: Vec<String> = Vec::new();
    let mut values = Vec::new();
    for (e, key) in runes {
        let mut k = key.clone();
        let mut n = 1;
        while items.contains(&k) {
            n += 1;
            k = format!("{key} ({n})");
        }
        items.push(k);
        values.push(e);
    }
    (items, values)
}

/// The teleport pick: from a rune (everything unanchored on it) or by an aura (one cultist).
fn teleport_to(
    world: &mut World,
    client: Entity,
    user: Entity,
    from_rune: Option<Entity>,
    target: Entity,
    dest_rune: Entity,
) {
    let Some(&dest) = world.get::<TilePos>(dest_rune) else {
        fail_rune(world, from_rune);
        return;
    };
    if world.get::<Rune>(dest_rune).is_none() {
        fail_rune(world, from_rune);
        return;
    }
    if world
        .resource::<ss13_world::SpatialIndex>()
        .dense_on_tile(dest)
        .is_empty()
        .not()
    {
        tell(
            world,
            client,
            Span::Warning,
            "The target rune is blocked. Attempting to teleport to it would be massively unwise.",
        );
        fail_rune(world, from_rune);
        return;
    }
    match from_rune {
        Some(rune) => {
            let Some(&here) = world.get::<TilePos>(rune) else {
                return;
            };
            let user_pos = world.get::<TilePos>(user).copied();
            if user_pos.is_none_or(|p| {
                (p.x - here.x).abs() > 1 || (p.y - here.y).abs() > 1 || p.z != here.z
            }) {
                fail_invoke(world, rune);
                return;
            }
            let movables: Vec<Entity> = world
                .resource::<ss13_world::SpatialIndex>()
                .on_tile(here)
                .iter()
                .copied()
                .filter(|&e| {
                    e != rune
                        && world.get::<CarriedBy>(e).is_none()
                        && (world.get::<Health>(e).is_some()
                            || world
                                .get::<ss13_mapload::DmItem>(e)
                                .is_some_and(|i| i.pickable()))
                })
                .collect();
            if movables.is_empty() {
                fail_invoke(world, rune);
                return;
            }
            chant(
                world,
                RuneKind::Teleport,
                &[user],
                RuneKind::Teleport.def().invocation,
            );
            let moved_user = movables.contains(&user);
            for e in movables {
                world.entity_mut(e).insert(dest);
            }
            let clients = crate::chem::clients_near(world, here, None);
            world.write_message(SendChat {
                clients,
                message: crate::chem::line(
                    Span::Warning,
                    "There is a sharp crack of inrushing air, and everything above the rune \
                     disappears!",
                ),
            });
            chat_to(
                world,
                user,
                Span::Cult,
                if moved_user {
                    "Your vision blurs, and you suddenly appear somewhere else."
                } else {
                    "You send everything above the rune away."
                },
            );
        }
        None => {
            // The aura: revalidated, then used.
            if world.get::<Cultist>(target).is_none() {
                return;
            }
            let Some(aura) = held_aura(world, user, Spell::Teleport) else {
                return;
            };
            let Some(&origin) = world.get::<TilePos>(user) else {
                return;
            };
            if let Some(&tp) = world.get::<TilePos>(target)
                && ((tp.x - origin.x).abs() > 1 || (tp.y - origin.y).abs() > 1 || tp.z != origin.z)
            {
                return;
            }
            whisper(world, user, Spell::Teleport.def().invocation);
            world.entity_mut(target).insert(dest);
            let name = name_of(world, user);
            visible(
                world,
                user,
                "You speak the words of the talisman and find yourself somewhere else!",
                &format!("Dust flows from {name}'s hand, and they disappear with a sharp crack!"),
                Span::Warning,
            );
            spend_aura(world, user, aura);
        }
    }
}

fn fail_rune(world: &mut World, rune: Option<Entity>) {
    if let Some(rune) = rune {
        fail_invoke(world, rune);
    }
}

/// `raise_dead/invoke`: a dead cultist on the rune is revived for three sacrifices.
fn invoke_revive(world: &mut World, rune: Entity, invokers: &[Entity]) {
    let user = invokers[0];
    let Some(&pos) = world.get::<TilePos>(rune) else {
        return;
    };
    let mut dead: Vec<Entity> = world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| {
            world.get::<Cultist>(e).is_some()
                && (world.get::<Stat>(e) == Some(&Stat::Dead)
                    || world.get::<PlayerMob>(e).is_none())
        })
        .collect();
    dead.sort();
    // shortcut: the first one on the rune (tg asks which when several lie there).
    let Some(&target) = dead.first() else {
        chat_to(
            world,
            user,
            Span::CultItalic,
            "There are no dead cultists on the rune!",
        );
        fail_invoke(world, rune);
        return;
    };
    if world.get::<Stat>(target) == Some(&Stat::Dead) {
        let team = world.resource::<CultTeam>();
        if !can_revive_dead(team.sacrificed, team.sacrifices_used) {
            let missing = SOULS_TO_REVIVE + team.sacrifices_used - team.sacrificed;
            chat_to(
                world,
                user,
                Span::Warning,
                format!(
                    "Your cult must carry out {missing} more sacrifice{} before it can revive \
                     another cultist!",
                    if missing == 1 { "" } else { "s" }
                ),
            );
            fail_invoke(world, rune);
            return;
        }
        world.resource_mut::<CultTeam>().sacrifices_used += SOULS_TO_REVIVE;
        revive_fully(world, target);
    }
    chant(
        world,
        RuneKind::Revive,
        invokers,
        RuneKind::Revive.def().invocation,
    );
    chat_to(
        world,
        target,
        Span::CultLarge,
        "\"PASNAR SAVRAE YAM'TOTH. Arise.\"",
    );
    let name = name_of(world, target);
    visible(
        world,
        target,
        "You awaken suddenly from the void. You're alive!",
        &format!("{name} draws in a huge breath, red light shining from their eyes."),
        Span::Warning,
    );
}

/// `revive(ADMIN_HEAL_ALL)`.
fn revive_fully(world: &mut World, mob: Entity) {
    crate::medical::heal_all(world, mob);
    let max = world.get::<Health>(mob).map_or(100.0, |h| h.max);
    world.entity_mut(mob).insert((
        Health {
            max,
            ..Health::default()
        },
        Stat::Conscious,
    ));
    if let Some(mut effects) = effects_mut(world, mob) {
        for kind in [
            Incap::Stun,
            Incap::Knockdown,
            Incap::Paralyze,
            Incap::Immobilize,
        ] {
            effects.set(kind, 0.0);
        }
        effects.stamina = 0.0;
        effects.stamcrit = false;
    }
    if let Some(mut alerts) = world.get_mut::<crate::breath::Alerts>(mob) {
        alerts.0.clear();
    }
    // The ghost goes back in (tg `revive` + the ghost reentering).
    let ghost = {
        let mut q = world.query::<(Entity, &Ghost)>();
        q.iter(world)
            .find(|(_, g)| g.body == Some(mob))
            .map(|(e, _)| e)
    };
    if let Some(ghost) = ghost {
        let _ = ghost;
        crate::ghost::return_to_body(world, mob);
    }
}

/// `wall/invoke`: the barrier goes up or down; 2 brute to an arm.
fn invoke_barrier(world: &mut World, rune: Entity, invokers: &[Entity]) {
    let user = invokers[0];
    let Some(&pos) = world.get::<TilePos>(rune) else {
        return;
    };
    chant(
        world,
        RuneKind::Barrier,
        invokers,
        RuneKind::Barrier.def().invocation,
    );
    let barrier = match world.get::<Rune>(rune).and_then(|r| r.barrier) {
        Some(b) if world.get_entity(b).is_ok() => Some(b),
        _ => {
            let b = crate::construction::spawn_obj(world, BARRIER, pos);
            if let Some(b) = b {
                world.entity_mut(b).insert(CultBarrier { rune, up: false });
                if let Some(mut r) = world.get_mut::<Rune>(rune) {
                    r.barrier = Some(b);
                }
            }
            b
        }
    };
    if let Some(b) = barrier {
        let up = !world.get::<CultBarrier>(b).is_some_and(|c| c.up);
        if let Some(mut c) = world.get_mut::<CultBarrier>(b) {
            c.up = up;
        }
        // shortcut: the shield is always drawn; up is dense, down lets everything through.
        if up {
            world.entity_mut(b).insert(ss13_world::Density);
        } else {
            world.entity_mut(b).remove::<ss13_world::Density>();
        }
        crate::construction::tile_changed(world, pos);
    }
    let zone = arm(world);
    hurt(world, user, Some(zone), 2.0);
}

/// `summon/invoke`: two invokers pick a living cultist to call to the rune.
fn invoke_summon(world: &mut World, client: Entity, rune: Entity, invokers: &[Entity]) {
    let user = invokers[0];
    let members = world.resource::<CultTeam>().members.clone();
    let cultists: Vec<Entity> = members
        .into_iter()
        .filter(|m| !invokers.contains(m) && world.get::<Stat>(*m) != Some(&Stat::Dead))
        .collect();
    if cultists.is_empty() {
        tell(
            world,
            client,
            Span::Warning,
            "There are no cultists to summon!",
        );
        fail_invoke(world, rune);
        return;
    }
    let items: Vec<String> = cultists.iter().map(|&c| name_of(world, c)).collect();
    open_list(
        world,
        client,
        user,
        CultPrompt {
            kind: PromptKind::Summon { rune },
            title: "Followers of the Geometer".into(),
            message: "Who do you wish to call to rune?".into(),
            items,
            values: cultists,
            spell_ids: Vec::new(),
        },
    );
}

fn summon_cultist(world: &mut World, user: Entity, rune: Entity, target: Entity) {
    let Some(&at) = world.get::<TilePos>(rune) else {
        return;
    };
    let invokers = can_invoke(world, rune, user, RuneKind::Summon);
    if invokers.len() < RuneKind::Summon.def().req_cultists {
        fail_invoke(world, rune);
        return;
    }
    let name = name_of(world, target);
    if world.get::<Stat>(target) == Some(&Stat::Dead) {
        chat_to(world, user, Span::CultItalic, format!("{name} has died!"));
        fail_invoke(world, rune);
        return;
    }
    if world.get::<Cultist>(target).is_none() {
        chat_to(
            world,
            user,
            Span::CultItalic,
            format!("{name} is not a follower of the Geometer!"),
        );
        fail_invoke(world, rune);
        return;
    }
    let pulled = {
        let mut q = world.query::<&crate::pull::Pulling>();
        q.iter(world).any(|p| p.0 == target)
    };
    if pulled {
        chat_to(
            world,
            user,
            Span::CultItalic,
            format!("{name} is being held in place!"),
        );
        fail_invoke(world, rune);
        return;
    }
    visible(
        world,
        target,
        "Overwhelming vertigo consumes you as you are hurled through the air!",
        &format!("{name} suddenly disappears in a flash of red light!"),
        Span::Warning,
    );
    chant(
        world,
        RuneKind::Summon,
        &invokers,
        RuneKind::Summon.def().invocation,
    );
    world.entity_mut(target).insert(at);
    let clients = crate::chem::clients_near(world, at, None);
    world.write_message(SendChat {
        clients,
        message: crate::chem::line(
            Span::Warning,
            &format!("A foggy shape materializes atop rune and solidifies into {name}!"),
        ),
    });
    delete_rune(world, rune);
}

/// `narsie/invoke`: nine invokers in a summoning area; Nar'Sie rises 4 s later.
fn invoke_narsie(world: &mut World, rune: Entity, invokers: &[Entity]) {
    let user = invokers[0];
    let Some(&pos) = world.get::<TilePos>(rune) else {
        return;
    };
    if world.get::<Rune>(rune).is_some_and(|r| r.used) {
        return;
    }
    let team = world.resource::<CultTeam>().clone();
    let spots = team.summon_spots();
    let here = area_of(world, pos);
    if !spots.iter().any(|(a, _)| Some(*a) == here) {
        let names = english_list(spots.iter().map(|(_, n)| n.as_str()));
        chat_to(
            world,
            user,
            Span::CultLarge,
            format!("The Geometer can only be summoned where the veil is weak - in {names}!"),
        );
        return;
    }
    if team.narsie.is_some() {
        for &i in invokers {
            chat_to(world, i, Span::Warning, "Nar'Sie is already on this plane!");
        }
        return;
    }
    if let Some(mut r) = world.get_mut::<Rune>(rune) {
        r.used = true;
    }
    let true_cultists: Vec<(Entity, String)> = team
        .members
        .iter()
        .map(|&m| {
            (
                m,
                world
                    .get::<Cultist>(m)
                    .map(|c| c.key.clone())
                    .unwrap_or_default(),
            )
        })
        .collect();
    let at = now(world) + 4.0;
    {
        let mut team = world.resource_mut::<CultTeam>();
        team.narsie_summoned = true;
        team.true_cultists = true_cultists;
        team.ending = Some(Ending::Rising { at, pos });
    }
    chant(
        world,
        RuneKind::NarSie,
        invokers,
        RuneKind::NarSie.def().invocation,
    );
    info!("Nar'Sie is being summoned at {pos:?} by {invokers:?}");
}

/// The steps of `begin_the_end` while Nar'Sie stands.
// Cost: one comparison per tick while the cult exists.
fn tick_ending(world: &mut World) {
    let Some(ending) = world.resource::<CultTeam>().ending else {
        return;
    };
    let t = now(world);
    let sender = format!(
        "{}: Отдел паранормальных явлений",
        crate::events::COMMAND_NAME
    );
    let next = match ending {
        Ending::Rising { at, pos } if t >= at => {
            narsie_rises(world, pos);
            Ending::BeginCheck { at: t + 15.0 }
        }
        Ending::BeginCheck { at } if t >= at => {
            crate::shuttle::priority_announce(
                world,
                &sender,
                "",
                "Внимание, это приоритетное оповещение. Сектор Эпсилон Эридани был подвергнут \
                 вторжению древней богоподобной враждебной сущности, у нас есть подтвержденная \
                 информация о массовых порабощениях по всему сектору. Положению присвоен сценарий \
                 \"ПОЛНОЕ ИСТРЕБЛЕНИЕ\". Приказ на принятие мер получен и авторизован. Ожидаемое \
                 время готовности: 60 секунд.",
            );
            Ending::SecondCheck { at: t + 50.0 }
        }
        Ending::SecondCheck { at } if t >= at => {
            crate::shuttle::priority_announce(
                world,
                &sender,
                "",
                "Моделирование беспричинного изменения пространства завершено. Начинаем \
                 развертывание.",
            );
            Ending::DestroyStation { at: t + 5.0 }
        }
        Ending::DestroyStation { at } if t >= at => Ending::Apocalypse { at: t + 60.0 },
        Ending::Apocalypse { at } if t >= at => {
            crate::shuttle::priority_announce(
                world,
                "Оповещение безопасности",
                "",
                "Тревога! Обнаружен запуск орбитального эммитера. Протоколы безопасности станции \
                 обнулены, возобновление нево@#$%^-...",
            );
            Ending::End { at: t + 12.0 }
        }
        Ending::End { at } if t >= at => {
            // `ending_helper`: `SSticker.force_ending`.
            world.resource_mut::<crate::round::Ticker>().force_ending = true;
            info!("Nar'Sie has consumed the station: the round ends");
            Ending::Done
        }
        other => other,
    };
    if next != ending {
        world.resource_mut::<CultTeam>().ending = Some(next);
    }
}

/// `new /obj/narsie(rune_turf)` + `start_ending_the_round`.
fn narsie_rises(world: &mut World, pos: TilePos) {
    let narsie = crate::construction::spawn_obj(world, NARSIE, pos);
    {
        let mut team = world.resource_mut::<CultTeam>();
        team.narsie = narsie;
        for o in &mut team.objectives {
            if let CultObjective::Eldergod { summoned, .. } = o {
                *summoned = true;
            }
        }
    }
    update_info(world);
    let mut q = world.query_filtered::<Entity, With<ClientToken>>();
    let clients: Vec<Entity> = q.iter(world).collect();
    world.write_message(SendChat {
        clients,
        message: ChatMessage::simple(ChatChannel::System, Span::CultLarge, "NAR'SIE HAS RISEN"),
    });
    info!("Nar'Sie has risen at {pos:?}");
}

// ---------------------------------------------------------------------------------------------
// Blood magic

/// `blood_magic/Activate`: the spell list (or a spell to remove first at the limit).
fn prepare_magic(world: &mut World, client: Entity, mob: Entity) {
    let Some(cultist) = world.get::<Cultist>(mob).cloned() else {
        return;
    };
    let empowered = near_empower_rune(world, mob);
    let limit = spell_limit(empowered);
    if cultist.spells.len() >= limit {
        if empowered {
            chat_to(
                world,
                mob,
                Span::CultItalic,
                format!("You cannot store more than {limit} spells. Pick a spell to remove."),
            );
        } else {
            chat_to(
                world,
                mob,
                Span::CultItalic,
                format!(
                    "You cannot store more than {RUNELESS_MAX_BLOODCHARGE} spells without an \
                     empowering rune! Pick a spell to remove."
                ),
            );
        }
        open_remove_spell(world, client, mob);
        return;
    }
    let mut items: Vec<String> = Spell::ALL.iter().map(|s| s.def().name.to_owned()).collect();
    items.push("(REMOVE SPELL)".into());
    let values = vec![mob; items.len()];
    open_list(
        world,
        client,
        mob,
        CultPrompt {
            kind: PromptKind::PrepareSpell { empowered },
            title: "Spell Choices".into(),
            message: "Blood spell to prepare".into(),
            items,
            values,
            spell_ids: Vec::new(),
        },
    );
}

fn open_remove_spell(world: &mut World, client: Entity, mob: Entity) {
    let Some(cultist) = world.get::<Cultist>(mob).cloned() else {
        return;
    };
    if cultist.spells.is_empty() {
        return;
    }
    let items: Vec<String> = cultist
        .spells
        .iter()
        .map(|s| s.spell.def().name.to_owned())
        .collect();
    let values = vec![mob; items.len()];
    let spell_ids = cultist.spells.iter().map(|s| s.id).collect();
    open_list(
        world,
        client,
        mob,
        CultPrompt {
            kind: PromptKind::RemoveSpell,
            title: "Current Spells".into(),
            message: "Spell to remove".into(),
            items,
            values,
            spell_ids,
        },
    );
}

fn near_empower_rune(world: &mut World, mob: Entity) -> bool {
    let Some(&at) = world.get::<TilePos>(mob) else {
        return false;
    };
    let mut q = world.query::<(&Rune, &TilePos)>();
    q.iter(world).any(|(r, p)| {
        r.kind == RuneKind::Empower
            && p.z == at.z
            && (p.x - at.x).abs() <= 1
            && (p.y - at.y).abs() <= 1
    })
}

fn start_carving(world: &mut World, client: Entity, mob: Entity, spell: Spell, empowered: bool) {
    let Some(cultist) = world.get::<Cultist>(mob).cloned() else {
        return;
    };
    // Revalidated: the rune may be gone, the slots filled meanwhile.
    if (empowered && !near_empower_rune(world, mob))
        || cultist.spells.len() >= spell_limit(empowered)
    {
        return;
    }
    if cultist.carving.is_some() {
        chat_to(
            world,
            mob,
            Span::CultItalic,
            "You are already invoking blood magic!",
        );
        return;
    }
    chat_to(
        world,
        mob,
        Span::Warning,
        "You begin to carve unnatural symbols into your flesh!",
    );
    let (delay, _) = carving(empowered);
    if let Some(mut c) = world.get_mut::<Cultist>(mob) {
        c.carving = Some((spell, empowered));
    }
    let item = world.get::<Inventory>(mob).and_then(Inventory::active_item);
    if !do_after(
        world,
        Actor { client, mob, item },
        mob,
        delay,
        0,
        Job::Cult(CultJob::Carve),
    ) && let Some(mut c) = world.get_mut::<Cultist>(mob)
    {
        c.carving = None;
    }
    refresh(world, mob);
}

fn finish_carving(world: &mut World, mob: Entity) {
    let Some((spell, empowered)) = world
        .get_mut::<Cultist>(mob)
        .and_then(|mut c| c.carving.take())
    else {
        return;
    };
    let full = world
        .get::<Cultist>(mob)
        .is_some_and(|c| c.spells.len() >= spell_limit(empowered));
    if full {
        refresh(world, mob);
        return;
    }
    bleed(world, mob, carving(empowered).1);
    if let Some(mut c) = world.get_mut::<Cultist>(mob) {
        let id = c.next_spell;
        c.next_spell += 1;
        c.spells.push(PreparedSpell {
            id,
            spell,
            charges: spell.def().charges,
        });
    }
    chat_to(
        world,
        mob,
        Span::Warning,
        format!(
            "Your wounds glow with power, you have prepared a {} invocation!",
            spell.def().name
        ),
    );
    refresh(world, mob);
}

fn remove_spell(world: &mut World, mob: Entity, id: u32) {
    let aura = world.get::<Inventory>(mob).and_then(|i| {
        i.hands
            .iter()
            .flatten()
            .copied()
            .find(|&h| world.get::<BloodAura>(h).is_some_and(|a| a.spell == id))
    });
    if let Some(aura) = aura {
        despawn_aura(world, mob, aura);
    }
    if let Some(mut c) = world.get_mut::<Cultist>(mob) {
        c.spells.retain(|s| s.id != id);
    }
    refresh(world, mob);
}

/// `blood_spell/Activate`: the aura into an empty hand, or back; the dagger summoned at once.
fn activate_spell(world: &mut World, client: Entity, mob: Entity, id: u32) {
    let Some(spell) = world.get::<Cultist>(mob).and_then(|c| c.spell(id).copied()) else {
        return;
    };
    if spell.charges == 0 {
        return;
    }
    let def = spell.spell.def();
    let Some(aura_path) = def.aura else {
        // `dagger/Activate`.
        whisper(world, mob, def.invocation);
        let Some(&pos) = world.get::<TilePos>(mob) else {
            return;
        };
        let name = name_of(world, mob);
        visible(
            world,
            mob,
            "Your plea for aid is answered, and light begins to shimmer and take form within your \
             hand!",
            &format!("{name}'s hand glows red for a moment."),
            Span::Warning,
        );
        if let Some(dagger) = crate::construction::spawn_obj(world, DAGGER, pos) {
            put_in_hands(world, mob, dagger);
        }
        use_charge(world, mob, id);
        return;
    };
    let held = world.get::<Inventory>(mob).and_then(|i| {
        i.hands
            .iter()
            .flatten()
            .copied()
            .find(|&h| world.get::<BloodAura>(h).is_some_and(|a| a.spell == id))
    });
    if let Some(aura) = held {
        despawn_aura(world, mob, aura);
        tell(
            world,
            client,
            Span::Warning,
            "You snuff out the spell, saving it for later.",
        );
        refresh(world, mob);
        return;
    }
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return;
    };
    let hand = if inventory.hands[inventory.active].is_none() {
        Some(inventory.active)
    } else {
        inventory.hands.iter().position(Option::is_none)
    };
    let Some(hand) = hand else {
        tell(
            world,
            client,
            Span::Warning,
            "You have no empty hand for invoking blood magic!",
        );
        return;
    };
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let Some(aura) = crate::construction::spawn_obj(world, aura_path, pos) else {
        return;
    };
    world.entity_mut(aura).remove::<TilePos>().insert((
        CarriedBy(mob),
        NoDrop,
        BloodAura {
            owner: mob,
            spell: id,
        },
    ));
    if let Some(mut inv) = world.get_mut::<Inventory>(mob) {
        inv.hands[hand] = Some(aura);
    }
    tell(
        world,
        client,
        Span::Notice,
        format!("Your wounds glow as you invoke the {}.", def.name),
    );
    refresh(world, mob);
}

fn put_in_hands(world: &mut World, mob: Entity, item: Entity) {
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return;
    };
    if let Some(hand) = inventory.hands.iter().position(Option::is_none) {
        world
            .entity_mut(item)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        if let Some(mut inv) = world.get_mut::<Inventory>(mob) {
            inv.hands[hand] = Some(item);
        }
    }
}

fn despawn_aura(world: &mut World, mob: Entity, aura: Entity) {
    if let Some(mut inv) = world.get_mut::<Inventory>(mob) {
        for h in &mut inv.hands {
            if *h == Some(aura) {
                *h = None;
            }
        }
    }
    if world.get_entity(aura).is_ok() {
        world.despawn(aura);
    }
}

/// Every aura in the cultist's hands goes away (deconversion, holy water).
fn drop_auras(world: &mut World, mob: Entity) {
    let auras: Vec<Entity> = world
        .get::<Inventory>(mob)
        .map(|i| {
            i.hands
                .iter()
                .flatten()
                .copied()
                .filter(|&h| world.get::<BloodAura>(h).is_some())
                .collect()
        })
        .unwrap_or_default();
    for aura in auras {
        despawn_aura(world, mob, aura);
    }
}

fn held_aura(world: &World, mob: Entity, spell: Spell) -> Option<Entity> {
    let cultist = world.get::<Cultist>(mob)?;
    world
        .get::<Inventory>(mob)?
        .hands
        .iter()
        .flatten()
        .copied()
        .find(|&h| {
            world
                .get::<BloodAura>(h)
                .and_then(|a| cultist.spell(a.spell))
                .is_some_and(|s| s.spell == spell)
        })
}

/// One charge used; the spell goes when it has none left.
fn use_charge(world: &mut World, mob: Entity, id: u32) {
    let left = world.get_mut::<Cultist>(mob).and_then(|mut c| {
        let s = c.spells.iter_mut().find(|s| s.id == id)?;
        s.charges = s.charges.saturating_sub(1);
        Some(s.charges)
    });
    if left == Some(0)
        && let Some(mut c) = world.get_mut::<Cultist>(mob)
    {
        c.spells.retain(|s| s.id != id);
    }
    refresh(world, mob);
}

/// `cast_spell` after the effect: the health cost to the hand's arm, a charge, the aura gone
/// when empty.
fn spend_aura(world: &mut World, mob: Entity, aura: Entity) {
    let Some(a) = world.get::<BloodAura>(aura).copied() else {
        return;
    };
    let Some(spell) = world
        .get::<Cultist>(mob)
        .and_then(|c| c.spell(a.spell).copied())
    else {
        return;
    };
    let def = spell.spell.def();
    if def.health_cost > 0.0 {
        let zone = active_arm(world, mob);
        hurt(world, mob, Some(zone), def.health_cost);
    }
    if spell.charges <= 1 {
        despawn_aura(world, mob, aura);
    }
    use_charge(world, mob, a.spell);
}

/// An aura used on `target` (`cast_spell`).
fn cast_aura(
    world: &mut World,
    client: Entity,
    mob: Entity,
    item: Entity,
    aura: BloodAura,
    target: Entity,
) {
    if aura.owner != mob || world.get::<Cultist>(mob).is_none() {
        despawn_aura(world, mob, item);
        return;
    }
    let Some(spell) = world
        .get::<Cultist>(mob)
        .and_then(|c| c.spell(aura.spell).copied())
    else {
        despawn_aura(world, mob, item);
        return;
    };
    match spell.spell {
        Spell::Stun => cast_stun(world, mob, item, target),
        Spell::Teleport => {
            if world.get::<Cultist>(target).is_none() {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "You can only teleport cultists with this spell!",
                );
                return;
            }
            let (items, values) = teleport_runes(world, None);
            if items.is_empty() {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "There are no valid runes to teleport to!",
                );
                return;
            }
            open_list(
                world,
                client,
                mob,
                CultPrompt {
                    kind: PromptKind::Teleport {
                        from_rune: None,
                        target,
                    },
                    title: "Teleportation Target".into(),
                    message: "Rune to teleport to".into(),
                    items,
                    values,
                    spell_ids: Vec::new(),
                },
            );
        }
        Spell::Shackles => cast_shackles(world, client, mob, item, target),
        Spell::Construction => cast_construction(world, client, mob, item, target),
        Spell::Dagger => {}
    }
}

/// `blood_magic/stun/cast_spell`.
fn cast_stun(world: &mut World, mob: Entity, aura: Entity, target: Entity) {
    if world.get::<Health>(target).is_none() || world.get::<Cultist>(target).is_some() {
        return;
    }
    whisper(world, mob, Spell::Stun.def().invocation);
    let (risen, ascendent) = {
        let t = world.resource::<CultTeam>();
        (t.cult_risen, t.cult_ascendent)
    };
    let coef = stun_coef(risen, ascendent);
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        &format!(
            "You attempt to stun {} with the spell!",
            name_of(world, target)
        ),
        &format!("{user} holds up their hand, which explodes in a flash of red light!"),
        Span::Warning,
    );
    if has_holy_water(world, target) {
        chat_to(world, mob, Span::Warning, "The spell had no effect!");
    } else {
        let victim = name_of(world, target);
        chat_to(
            world,
            mob,
            Span::CultItalic,
            format!("In a brilliant flash of red, {victim} falls to the ground!"),
        );
        crate::status_effects::apply(world, target, Incap::Paralyze, 16.0 * coef);
    }
    let t = now(world);
    world.entity_mut(mob).insert(NextMove(t + CLICK_CD_MELEE));
    spend_aura(world, mob, aura);
}

/// `blood_magic/shackles/cast_spell`: a 3 s `do_after`, then shadow shackles.
fn cast_shackles(world: &mut World, client: Entity, mob: Entity, aura: Entity, target: Entity) {
    if world.get::<HumanLook>(target).is_none() || world.get::<Cultist>(target).is_some() {
        return;
    }
    let victim = name_of(world, target);
    if world.get::<crate::restraints::Handcuffed>(target).is_some() {
        tell(
            world,
            client,
            Span::Warning,
            format!("{victim} is already bound."),
        );
        return;
    }
    whisper(world, mob, Spell::Shackles.def().invocation);
    let user = name_of(world, mob);
    visible(
        world,
        target,
        &format!("{user} begins shaping dark magic shackles around your wrists!"),
        &format!("{user} begins restraining {victim} with dark magic!"),
        Span::Danger,
    );
    do_after(
        world,
        Actor {
            client,
            mob,
            item: Some(aura),
        },
        target,
        30.0,
        0,
        Job::Cult(CultJob::Shackle),
    );
}

fn finish_shackle(world: &mut World, mob: Entity, action: &DoAfter) {
    let target = action.target;
    let Some(aura) = action.item.filter(|&a| world.get::<BloodAura>(a).is_some()) else {
        return;
    };
    let victim = name_of(world, target);
    if world.get::<crate::restraints::Handcuffed>(target).is_some() {
        chat_to(
            world,
            mob,
            Span::Warning,
            format!("{victim} is already bound."),
        );
        return;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    let Some(cuffs) = crate::construction::spawn_obj(world, SHACKLES, pos) else {
        return;
    };
    crate::restraints::apply_cuffs(world, cuffs, target);
    chat_to(world, mob, Span::Notice, format!("You shackle {victim}."));
    spend_aura(world, mob, aura);
}

/// `blood_magic/construction/cast_spell`: plasteel into runed metal.
fn cast_construction(world: &mut World, client: Entity, mob: Entity, aura: Entity, target: Entity) {
    let path = path_of(world, target);
    if path.as_deref().is_some_and(|p| p.starts_with(PLASTEEL)) {
        let Some(&pos) = world.get::<TilePos>(target).or(world.get::<TilePos>(mob)) else {
            return;
        };
        let amount = world
            .get::<crate::tools::Stack>(target)
            .map_or(1, |s| s.amount);
        whisper(world, mob, Spell::Construction.def().invocation);
        if world.get::<CarriedBy>(target).is_some()
            && let Some(mut inv) = world.get_mut::<Inventory>(mob)
        {
            for h in &mut inv.hands {
                if *h == Some(target) {
                    *h = None;
                }
            }
        }
        world.despawn(target);
        crate::construction::spawn_stack(world, RUNED_METAL, pos, amount);
        tell(
            world,
            client,
            Span::Warning,
            "A dark cloud emanates from you hand and swirls around the plasteel, transforming it \
             into runed metal!",
        );
        spend_aura(world, mob, aura);
        return;
    }
    let name = name_of(world, target);
    tell(
        world,
        client,
        Span::Warning,
        format!("The spell will not work on {name}!"),
    );
}

// ---------------------------------------------------------------------------------------------
// Clicks on mobs

/// A blood magic aura or the dagger on a mob, before melee: the spells, and no hurting other
/// cultists with the dagger (it purges their holy water instead).
fn cult_clicks(world: &mut World, mut clicks: Local<MessageCursor<crate::click::ClickResolved>>) {
    let clicks: Vec<crate::click::ClickResolved> = clicks
        .read(world.resource::<Messages<crate::click::ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        let (mob, target) = (click.mob, click.target);
        if world
            .resource::<crate::security::MeleeHandled>()
            .0
            .contains(&mob)
            || world.get::<Health>(target).is_none()
        {
            continue;
        }
        let Some(held) = world.get::<Inventory>(mob).and_then(Inventory::active_item) else {
            continue;
        };
        let t = now(world);
        if world.get::<NextMove>(mob).is_some_and(|n| t < n.0) {
            continue;
        }
        if let Some(aura) = world.get::<BloodAura>(held).copied() {
            world
                .resource_mut::<crate::security::MeleeHandled>()
                .0
                .insert(mob);
            cast_aura(world, click.client, mob, held, aura, target);
            continue;
        }
        if path_of(world, held).as_deref() == Some(DAGGER)
            && world.get::<Cultist>(mob).is_some()
            && world.get::<Cultist>(target).is_some()
        {
            world
                .resource_mut::<crate::security::MeleeHandled>()
                .0
                .insert(mob);
            purge_holy_water(world, mob, target, held);
        }
    }
}

/// `do_purge_holywater`: the holy water in the target's blood turns into unholy water.
fn purge_holy_water(world: &mut World, mob: Entity, target: Entity, dagger: Entity) {
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return;
    };
    let Some(id) = chem.id(HOLY_WATER) else {
        return;
    };
    let Some(mut reagents) = world.get::<crate::chem::MobReagents>(target).cloned() else {
        return;
    };
    let amount = reagents.blood.amount(id) + reagents.stomach.amount(id);
    if amount <= 0.0 {
        return;
    }
    reagents.blood.remove_reagent(id, f32::MAX);
    reagents.stomach.remove_reagent(id, f32::MAX);
    if let Some(unholy) = chem.id("/datum/reagent/fuel/unholywater") {
        reagents
            .blood
            .add_reagent(&chem, unholy, amount, 310.15, None, None, false, false);
    }
    world.entity_mut(target).insert(reagents);
    if let Some(mut c) = world.get_mut::<Cultist>(target) {
        c.holy_secs = 0.0;
    }
    let (victim, tool) = (name_of(world, target), name_of(world, dagger));
    chat_to(
        world,
        mob,
        Span::Cult,
        format!("You remove the taint from {victim} using {tool}."),
    );
}

// ---------------------------------------------------------------------------------------------
// Life: holy water

/// Every `Life()` (2 s): holy water in a cultist's blood (`holywater/on_mob_life`).
fn cult_life(world: &mut World, mut last_fired: Local<Option<u64>>) {
    let fired = world.resource::<crate::breath::MobsClock>().times_fired;
    let fires = fired.saturating_sub(last_fired.unwrap_or(fired));
    *last_fired = Some(fired);
    if fires == 0 {
        return;
    }
    let secs = crate::breath::SSMOBS_WAIT_SECS as f32 * fires as f32;
    let mut q = world.query_filtered::<Entity, With<Cultist>>();
    let mut mobs: Vec<Entity> = q.iter(world).collect();
    mobs.sort();
    for mob in mobs {
        holy_water_tick(world, mob, secs);
    }
}

/// What holy water does to a cultist after `holy_secs` of it: (spells cleared, babbling stage,
/// deconverted).
pub fn holy_water_stage(holy_secs: f32) -> (bool, bool) {
    (
        holy_secs >= HOLY_WATER_STUTTER_SECS,
        holy_secs >= HOLY_WATER_DECONVERT_SECS,
    )
}

// shortcut: the time counts every `Life()` the holy water is in the blood, not by the
// metabolized share (`metabolization_ratio`); a microdose counts like a gulp.
fn holy_water_tick(world: &mut World, mob: Entity, secs: f32) {
    if world.get::<Stat>(mob) == Some(&Stat::Dead) {
        return;
    }
    if !has_holy_water(world, mob) {
        if let Some(mut c) = world.get_mut::<Cultist>(mob)
            && c.holy_secs != 0.0
        {
            c.holy_secs = 0.0;
        }
        return;
    }
    let first = world
        .get::<Cultist>(mob)
        .is_some_and(|c| c.holy_secs == 0.0);
    if first {
        chat_to(
            world,
            mob,
            Span::BoldWarning,
            "A vile holiness begins to spread its shining tendrils through your mind, purging the \
             Geometer of Blood's influence!",
        );
    }
    let had_spells = world
        .get::<Cultist>(mob)
        .is_some_and(|c| !c.spells.is_empty());
    if had_spells {
        drop_auras(world, mob);
        if let Some(mut c) = world.get_mut::<Cultist>(mob) {
            c.spells.clear();
        }
        chat_to(
            world,
            mob,
            Span::CultLarge,
            "Your blood rites falter as holy water scours your body!",
        );
        refresh(world, mob);
    }
    let total = {
        let Some(mut c) = world.get_mut::<Cultist>(mob) else {
            return;
        };
        c.holy_secs += secs;
        c.holy_secs
    };
    let (babbling, deconverted) = holy_water_stage(total);
    if deconverted {
        remove_cultist(world, mob, false);
        crate::status_effects::apply(world, mob, Incap::Paralyze, 10.0);
        if let (Some(chem), Some(mut reagents)) = (
            world
                .get_resource::<crate::chem::Chem>()
                .map(|c| c.0.clone()),
            world.get::<crate::chem::MobReagents>(mob).cloned(),
        ) && let Some(id) = chem.id(HOLY_WATER)
        {
            reagents.blood.remove_reagent(id, f32::MAX);
            world.entity_mut(mob).insert(reagents);
        }
        return;
    }
    // `SPT_PROB(10, seconds_per_tick)`.
    let babble = babbling
        && world
            .get_resource_mut::<crate::combat::CombatRng>()
            .is_some_and(|mut r| r.chance(100.0 * (1.0 - 0.9f32.powf(secs))));
    if babble {
        const WORDS: [&str; 8] = [
            "Av'te Nar'Sie",
            "Pa'lid Mors",
            "INO INO ORA ANA",
            "SAT ANA!",
            "Daim'niodeis Arc'iai Le'eones",
            "R'ge Na'sie",
            "Diabo us Vo'iscum",
            "Eld' Mon Nobis",
        ];
        let i = world
            .get_resource_mut::<crate::combat::CombatRng>()
            .map_or(0, |mut r| r.range(0, 7) as usize);
        say(world, mob, WORDS[i]);
    }
}

// ---------------------------------------------------------------------------------------------
// Windows

impl ServerUi for AntagInfoGenericUi {
    type Source = &'static CultInfo;
    type UserParam = ();
    type Act = ();

    fn static_data(info: &CultInfo) -> BrotherInfo {
        info.0.clone()
    }

    fn data(_: &CultInfo, _: Entity, _: &()) {}

    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

/// `/datum/action/antag_info/Trigger` of a cultist.
fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if world.get::<Cultist>(source).is_some() {
            let name = antag_name(world, source);
            world.write_message(OpenUi::new::<AntagInfoGenericUi>(
                client, actor, source, name,
            ));
        }
    }
}

/// The panels of every cultist follow the cult's objectives.
fn update_info(world: &mut World) {
    let members = world
        .get_resource::<CultTeam>()
        .map(|t| t.members.clone())
        .unwrap_or_default();
    for m in members {
        let info = info_of(world, m);
        if let Ok(mut e) = world.get_entity_mut(m) {
            e.insert(CultInfo(info));
        }
    }
}

/// The panel's static data: the cult's objectives now.
pub fn info_of(world: &World, mob: Entity) -> BrotherInfo {
    let objectives = world
        .get_resource::<CultTeam>()
        .map(|t| {
            t.objectives
                .iter()
                .enumerate()
                .map(|(i, o)| ObjectiveInfo {
                    count: i as u32 + 1,
                    explanation: o.explanation(),
                    complete: o.complete(),
                })
                .collect()
        })
        .unwrap_or_default();
    BrotherInfo {
        antag_name: antag_name(world, mob).to_owned(),
        objectives,
    }
}

// ---------------------------------------------------------------------------------------------
// Round end

/// `/datum/team/cult/roundend_report`.
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let Some(team) = world.get_resource::<CultTeam>().cloned() else {
        return Vec::new();
    };
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = vec![line(vec![if team.victory() {
        ChatSegment::new(
            Span::GreenText,
            "Культ смог призвать своего бога! Нар'Си погасил ещё один факел в этой пустоте!",
        )
    } else {
        ChatSegment::new(
            Span::RedText,
            "Экипаж смог остановить культ! Тёмные речи и ересь не идут ни в какое сравнение с \
             лучшими из Нанотрейзен!",
        )
    }])];
    if !team.objectives.is_empty() {
        out.push(line(vec![ChatSegment::new(
            Span::Bold,
            "Культ имел следующие цели:",
        )]));
        for (i, o) in team.objectives.iter().enumerate() {
            out.push(line(vec![
                ChatSegment::new(Span::Bold, format!("Цель #{}", i + 1)),
                ChatSegment::new(Span::Plain, format!(": {} ", o.explanation())),
                if o.complete() {
                    ChatSegment::new(Span::GreenText, "Успех!")
                } else {
                    ChatSegment::new(Span::RedText, "Провал.")
                },
            ]));
        }
    }
    let members: Vec<(Entity, String)> = if team.true_cultists.is_empty() {
        team.members
            .iter()
            .map(|&m| {
                (
                    m,
                    world
                        .get::<Cultist>(m)
                        .map(|c| c.key.clone())
                        .unwrap_or_default(),
                )
            })
            .collect()
    } else {
        team.true_cultists.clone()
    };
    if !members.is_empty() {
        out.push(line(vec![ChatSegment::new(
            Span::Bold,
            "Культистами были:",
        )]));
        for (mob, key) in members {
            out.push(crate::antag::traitor::print_player(world, mob, &key));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rune_table_follows_tg() {
        let names: Vec<&str> = RuneKind::ALL.iter().map(|k| k.def().cultist_name).collect();
        assert_eq!(
            names,
            [
                "Offer",
                "Empower",
                "Teleport",
                "Nar'Sie",
                "Revive",
                "Barrier",
                "Summon Cultist"
            ]
        );
        for kind in RuneKind::ALL {
            assert_eq!(RuneKind::from_name(kind.def().cultist_name), Some(kind));
            assert_eq!(RuneKind::from_path(kind.def().path), Some(kind));
        }
        let narsie = RuneKind::NarSie.def();
        assert_eq!(
            (
                narsie.req_cultists,
                narsie.scribe_delay,
                narsie.scribe_damage
            ),
            (9, 500.0, 40.1)
        );
        assert_eq!(narsie.erase_time, 50.0);
        let summon = RuneKind::Summon.def();
        assert_eq!((summon.req_cultists, summon.invoke_damage), (2, 10.0));
        let offer = RuneKind::Offer.def();
        assert_eq!(
            (offer.req_cultists, offer.scribe_delay, offer.scribe_damage),
            (1, 40.0, 0.1)
        );
        assert!(RuneKind::Teleport.def().req_keyword);
        assert!(!RuneKind::Offer.def().req_keyword);
    }

    #[test]
    fn offer_needs_two_to_convert_and_three_for_the_linked() {
        assert_eq!(offer_invokers_needed(OfferTarget::Convert), 2);
        assert_eq!(offer_invokers_needed(OfferTarget::LinkedSacrifice), 3);
        assert_eq!(offer_invokers_needed(OfferTarget::Sacrifice), 1);
    }

    #[test]
    fn revives_cost_three_sacrifices_each() {
        assert!(!can_revive_dead(2, 0));
        assert!(can_revive_dead(3, 0));
        assert!(!can_revive_dead(5, 3));
        assert!(can_revive_dead(6, 3));
    }

    #[test]
    fn blood_magic_limits_and_costs() {
        assert_eq!(spell_limit(false), 1);
        assert_eq!(spell_limit(true), 4);
        assert_eq!(carving(false), (100.0, 40.0));
        assert_eq!(carving(true), (40.0, 8.0));
        assert_eq!(Spell::Shackles.def().charges, 4);
        assert_eq!(Spell::Stun.def().health_cost, 10.0);
        assert_eq!(Spell::Teleport.def().health_cost, 7.0);
        assert_eq!(Spell::Construction.def().health_cost, 12.0);
        for spell in Spell::ALL {
            assert_eq!(Spell::from_name(spell.def().name), Some(spell));
        }
    }

    #[test]
    fn cult_stages_follow_bandastation_ratios() {
        // Low pop: 20% risen, 30% ascendent of the other living players.
        assert_eq!(cult_stage(1, 10), (false, false));
        assert_eq!(cult_stage(2, 10), (true, false));
        assert_eq!(cult_stage(3, 10), (true, true));
        // High pop (70+): 10% and 20%.
        assert_eq!(cult_stage(7, 70), (true, false));
        assert_eq!(cult_stage(14, 70), (true, true));
        assert_eq!(cult_stage(6, 70), (false, false));
        // Nobody else alive.
        assert_eq!(cult_stage(1, 0), (true, true));
        assert_eq!(stun_coef(false, false), 1.0);
        assert_eq!(stun_coef(true, false), 0.4);
        assert_eq!(stun_coef(true, true), 0.1);
    }

    #[test]
    fn holy_water_deconverts_after_a_minute() {
        assert_eq!(holy_water_stage(10.0), (false, false));
        assert_eq!(holy_water_stage(26.0), (true, false));
        assert_eq!(holy_water_stage(60.0), (true, true));
    }

    #[test]
    fn objectives_explain_and_complete() {
        let mut team = CultTeam {
            objectives: vec![
                CultObjective::Sacrifice {
                    target: Some(Entity::PLACEHOLDER),
                    name: "Bob".into(),
                    job: "Ассистент".into(),
                    sacced: false,
                },
                CultObjective::Eldergod {
                    spots: vec![
                        (Entity::PLACEHOLDER, "Bridge".into()),
                        (Entity::PLACEHOLDER, "Bar".into()),
                        (Entity::PLACEHOLDER, "Chapel".into()),
                    ],
                    summoned: false,
                },
            ],
            ..default()
        };
        assert!(
            team.objectives[0]
                .explanation()
                .starts_with("Sacrifice Bob, Ассистент,")
        );
        assert!(
            team.objectives[1]
                .explanation()
                .contains("Bridge, Bar and Chapel")
        );
        assert!(team.is_sacrifice_target(Entity::PLACEHOLDER));
        assert!(!team.sacrifice_done() && !team.victory());
        if let CultObjective::Sacrifice { sacced, .. } = &mut team.objectives[0] {
            *sacced = true;
        }
        assert!(team.sacrifice_done() && !team.victory());
        assert!(!team.is_sacrifice_target(Entity::PLACEHOLDER));
        if let CultObjective::Eldergod { summoned, .. } = &mut team.objectives[1] {
            *summoned = true;
        }
        assert!(team.victory());
    }
}
