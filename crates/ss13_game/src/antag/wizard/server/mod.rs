//! The space wizard (server) after tg `/datum/antagonist/wizard` (`wizard.dm`), its spellbook
//! (`equipment/wizard_spellbook.dm`, `spellbook_entries/`), the scroll of teleportation
//! (`code/game/objects/items/scrolls.dm`), the contract of apprenticeship
//! (`_common/antag_spawner.dm`) and `/datum/outfit/wizard`.
//!
//! - **The den.** tg lazy-loads `_maps/templates/lazy_templates/wizard_den.dmm`; here it loads
//!   with the station onto its own level (like the nuclear operatives' infiltrator) when every
//!   type it places has a prototype, and its `/obj/effect/landmark/start/wizard` tiles are where
//!   wizards appear (`GLOB.wizardstart`). Without it a wizard appears at a latejoin spot ("ПРЯМО
//!   СЕЙЧАС, ПОШЕЛ ПОШЕЛ ПОШЕЛ").
//! - **Roundstart and midround** ([`spawn_wizards`]): the "Roundstart Wizard" ruleset's player gets
//!   no station job (`forced_occupations`), the "Midround Wizard" ruleset takes a ghost; either
//!   gets a fresh body in the den.
//! - **[`add_wizard`]** (`on_gain`): a human body stripped and dressed in `/datum/outfit/wizard`
//!   (robe, hat, magic sandals, light purple jumpsuit, headset, the staff in the left hand, the
//!   scroll in the right pocket, a backpack with the survival box and the spellbook bound to the
//!   wizard), the objectives (`create_objectives`: kill and escape 30%, steal and escape 30%,
//!   kill, steal and survive 25%, hijack 15%), the greeting, the wizard HUD icon, the info panel
//!   and the rename prompt (a random name from `strings/names/wizard*.txt` by default).
//! - **The spellbook** ([`Spellbook`]): 10 points; the entries below (the ported spells and
//!   artefacts), each spell up to its `spell_max_level`, buying a known spell levels it up; spells
//!   are refunded (`level × cost`) only in the den. The book opens only for its owner (an unbound
//!   book binds to whoever uses it first).
//! - **The scroll** ([`crate::spells::effects::TeleportScroll`]): used in hand, the area
//!   teleport without invocation or robes, 4 uses (an apprentice's: 1), then it crumbles.
//! - **The contract** ([`ApprenticeContract`]): used in hand, the school (destruction,
//!   bluespace, healing, robeless); a ghost who wants `ROLE_WIZARD_MIDROUND` becomes the
//!   apprentice next to the wizard ([`add_apprentice`]): the apprentice outfit (no book, no staff,
//!   a one-use scroll), the school's spells, "protect the wizard".
//! - **Rituals** (`spellbook_entries/summons.dm`), only in books prepared in a high chaos round
//!   (dynamic tier 4): Summon Guns (once; a gun for everyone, survivalists 10% of the time,
//!   `crate::antag::survivalist`) and Summon Events (each cast turns on or intensifies wizard
//!   mode, `crate::events::controller::summon_events`).
//! - [`roundend_report`]: each wizard, its objectives and what it bought.
// shortcut: no Grand Ritual, perks, Summon Magic or Ghosts or other summons, challenges, loadouts or randomized books, no team
// HUD between a wizard and its apprentices, no imposters or academy wizards; the book refunds
// no contract or bottles; the apprentice is the first willing ghost (tg polls them for 15 s);
// the robeless school's mindswap is not ported (only Knock). Entries whose mechanics are missing
// (mutate, rod form, mindswap, time stop, shapeshift, lichdom, the hammers, ...) are not in the
// book.

use std::path::PathBuf;

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_assets::dmm::DmmMap;
use ss13_mapload::antag::StealItemDef;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::spawner::Rng;
use ss13_mapload::{DmItem, IconIndex, MapLoadPlugin, MapLoadReport, ShuttleTemplates};
use ss13_net::chat::server::SendChat;
use ss13_net::server::{ClientToken, Networked};
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::UiInterface;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::{
    PURCHASE, REFUND, ROLE_WIZARD_MIDROUND, SpellbookData, SpellbookEntry, SpellbookStatic,
    SpellbookUi,
};
use crate::antag::objectives::{
    Objective, ObjectiveKind, RoundEnd, considered_alive, find_steal_target, find_target,
};
use crate::antag::{AntagHudData, AntagHudUi, BrotherInfo, ObjectiveInfo};
use crate::health::{Health, Stat};
use crate::human::HumanLook;
use crate::inventory::{CarriedBy, Inventory};
use crate::server::PlayerMob;
use crate::spells::effects::TeleportScroll;
use crate::spells::{Learned, Spell, Spells};
use crate::storage::{Stored, StoredIn};
use crate::tools::tell;

mod den;
mod gain;
mod roundend;
mod spellbook;
use den::*;
pub use gain::*;
pub use roundend::*;
pub use spellbook::*;

/// `name = "\proper Космический Волшебник"`, the apprentice's `name`.
pub const WIZARD_NAME: &str = "Космический Волшебник";
pub const APPRENTICE_NAME: &str = "Ученик мага";
/// `roundend_category`.
pub const ROUNDEND_CATEGORY: &str = "Волшебники/Ведьмы";
const OBJECTIVE_NAME: &str = "Задача";
/// `/obj/item/spellbook` `uses`.
pub const SPELLBOOK_USES: u32 = 10;
/// `/obj/item/teleportation_scroll` `uses`, the apprentice's.
pub const SCROLL_USES: u32 = 4;
pub const APPRENTICE_SCROLL_USES: u32 = 1;
/// `MAX_NAME_LEN`.
const MAX_NAME_LEN: u32 = 42;

pub const DEN_MAP: &str = "_maps/templates/lazy_templates/wizard_den.dmm";
pub const WIZARD_START: &str = "/obj/effect/landmark/start/wizard";
pub const SPELLBOOK: &str = "/obj/item/spellbook";
pub const SCROLL: &str = "/obj/item/teleportation_scroll";
pub const APPRENTICE_SCROLL: &str = "/obj/item/teleportation_scroll/apprentice";
pub const CONTRACT: &str = "/obj/item/antag_spawner/contract";
const WIZROBE: &str = "/obj/item/clothing/suit/wizrobe";
const WIZARD_HAT: &str = "/obj/item/clothing/head/wizard";
const MAGIC_SANDALS: &str = "/obj/item/clothing/shoes/sandal/magic";
const LIGHTPURPLE: &str = "/obj/item/clothing/under/color/lightpurple";
const HEADSET: &str = "/obj/item/radio/headset";
const STAFF: &str = "/obj/item/staff";
const SURVIVAL_BOX: &str = "/obj/item/storage/box/survival";
const WAND_BELT: &str = "/obj/item/storage/belt/wands/full";
/// `wands/full/create_wands`.
const BELT_WANDS: [&str; 7] = [
    "/obj/item/gun/magic/wand/death",
    "/obj/item/gun/magic/wand/resurrection",
    "/obj/item/gun/magic/wand/polymorph",
    "/obj/item/gun/magic/wand/teleport",
    "/obj/item/gun/magic/wand/door",
    "/obj/item/gun/magic/wand/fireball",
    "/obj/item/gun/magic/wand/shrink",
];
const ANTAG_HUD_DMI: &str = "icons/mob/huds/antag_hud.dmi";

const OFFENSIVE: &str = "Наступление";
const DEFENSIVE: &str = "Защита";
const MOBILITY: &str = "Мобильность";
const ASSISTANCE: &str = "Поддержка";
const RITUALS: &str = "Ритуалы";
/// `DYNAMIC_TIER_HIGH`: Summon Events needs a high chaos round.
const DYNAMIC_TIER_HIGH: u8 = 4;

/// What an entry gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Spell(Spell),
    /// `/datum/spellbook_entry/item`: summoned into the hands.
    Item(&'static str),
    /// `/datum/spellbook_entry/summon`: a station-wide ritual.
    Ritual(Ritual),
}

/// The ported `/datum/spellbook_entry/summon` rituals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ritual {
    /// `/datum/spellbook_entry/summon/guns` (`crate::antag::survivalist`).
    SummonGuns,
    SummonEvents,
}

/// A `/datum/spellbook_entry`.
#[derive(Clone, Copy, Debug)]
pub struct EntryDef {
    pub id: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub category: &'static str,
    pub cost: u32,
    pub kind: EntryKind,
}

impl EntryDef {
    /// `limit`: a spell's `spell_max_level`, items none, Summon Events 5 (each cast
    /// intensifies it).
    pub fn limit(&self) -> Option<u32> {
        match self.kind {
            EntryKind::Spell(spell) => Some(u32::from(spell.def().max_level)),
            EntryKind::Item(_) => None,
            EntryKind::Ritual(Ritual::SummonEvents) => Some(5),
            EntryKind::Ritual(Ritual::SummonGuns) => Some(1),
        }
    }

    /// Items are not refundable in the book.
    pub fn refundable(&self) -> bool {
        matches!(self.kind, EntryKind::Spell(_))
    }

    pub fn buy_word(&self) -> &'static str {
        match self.kind {
            EntryKind::Spell(_) => "Выучить",
            EntryKind::Item(_) => "Призвать",
            EntryKind::Ritual(_) => "Провести",
        }
    }
}

const fn spell(
    id: &'static str,
    name: &'static str,
    category: &'static str,
    cost: u32,
    spell: Spell,
    desc: &'static str,
) -> EntryDef {
    EntryDef {
        id,
        name,
        desc,
        category,
        cost,
        kind: EntryKind::Spell(spell),
    }
}

const fn item(
    id: &'static str,
    name: &'static str,
    category: &'static str,
    cost: u32,
    path: &'static str,
    desc: &'static str,
) -> EntryDef {
    EntryDef {
        id,
        name,
        desc,
        category,
        cost,
        kind: EntryKind::Item(path),
    }
}

/// The book's entries (`subtypesof(/datum/spellbook_entry)` that are ported), tg's order.
pub const ENTRIES: [EntryDef; 23] = [
    spell(
        "/datum/spellbook_entry/fireball",
        "Fireball",
        OFFENSIVE,
        2,
        Spell::Fireball,
        "Выпускает в цель взрывной огненный шар. Считается классическим среди всех волшебников.",
    ),
    spell(
        "/datum/spellbook_entry/disintegrate",
        "Smite",
        OFFENSIVE,
        2,
        Spell::Smite,
        "Заряжает вашу руку нечестивой энергией, которая может быть использована для того, \
         чтобы заставить тронутую жертву жестоко взорваться.",
    ),
    spell(
        "/datum/spellbook_entry/blind",
        "Blind",
        OFFENSIVE,
        1,
        Spell::Blind,
        "Временно ослепляет одну цель.",
    ),
    spell(
        "/datum/spellbook_entry/lightningbolt",
        "Lightning Bolt",
        OFFENSIVE,
        1,
        Spell::LightningBolt,
        "Выпустите молнию в своих врагов! Она перепрыгивает между целями, но не может сбить их \
         с ног.",
    ),
    item(
        "/datum/spellbook_entry/item/staffchaos",
        "Staff of Chaos",
        OFFENSIVE,
        2,
        "/obj/item/gun/magic/staff/chaos",
        "Капризный инструмент, который может стрелять всеми видами магии без всякой причины. \
         Не рекомендуется использовать его на дорогих вам людях.",
    ),
    spell(
        "/datum/spellbook_entry/magicm",
        "Magic Missile",
        DEFENSIVE,
        2,
        Spell::MagicMissile,
        "Выпускает несколько медленно движущихся магических снарядов по ближайшим целям.",
    ),
    spell(
        "/datum/spellbook_entry/disabletech",
        "Disable Tech",
        DEFENSIVE,
        1,
        Spell::DisableTech,
        "Отключает все оружие, камеры и большинство других устройств в радиусе действия.",
    ),
    spell(
        "/datum/spellbook_entry/repulse",
        "Repulse",
        DEFENSIVE,
        2,
        Spell::Repulse,
        "Отбрасывает все вокруг пользователя.",
    ),
    spell(
        "/datum/spellbook_entry/smoke",
        "Smoke",
        DEFENSIVE,
        1,
        Spell::Smoke,
        "Порождает облако удушливого дыма на вашем месте.",
    ),
    spell(
        "/datum/spellbook_entry/forcewall",
        "Force Wall",
        DEFENSIVE,
        1,
        Spell::Forcewall,
        "Создайте магический барьер, через который сможете пройти только вы.",
    ),
    item(
        "/datum/spellbook_entry/item/staffhealing",
        "Staff of Healing",
        DEFENSIVE,
        1,
        "/obj/item/gun/magic/staff/healing",
        "Альтруистический посох, способный исцелять хромых и воскрешать мертвых.",
    ),
    item(
        "/datum/spellbook_entry/item/wands",
        "Wand Assortment",
        DEFENSIVE,
        2,
        WAND_BELT,
        "Коллекция палочек, позволяющих использовать их в самых разных целях. У палочек \
         ограниченное количество зарядов, поэтому будьте осторожны при их использовании. \
         Поставляется в удобном поясе или модной бандольере, если пояс уже экипирован.",
    ),
    spell(
        "/datum/spellbook_entry/knock",
        "Knock",
        MOBILITY,
        1,
        Spell::Knock,
        "Открывает ближайшие двери и шкафы.",
    ),
    spell(
        "/datum/spellbook_entry/blink",
        "Blink",
        MOBILITY,
        2,
        Spell::Blink,
        "Случайным образом телепортирует вас на небольшое расстояние.",
    ),
    spell(
        "/datum/spellbook_entry/teleport",
        "Teleport",
        MOBILITY,
        2,
        Spell::Teleport,
        "Телепортирует вас в выбранную вами область.",
    ),
    spell(
        "/datum/spellbook_entry/jaunt",
        "Ethereal Jaunt",
        MOBILITY,
        2,
        Spell::EtherealJaunt,
        "Превращает вас в эфирную форму, временно делая вас невидимым и способным проходить \
         сквозь стены.",
    ),
    spell(
        "/datum/spellbook_entry/swap",
        "Swap",
        MOBILITY,
        1,
        Spell::Swap,
        "Поменяйтесь местами с любой живой целью в пределах девяти плиток. Нажмите ПКМ, чтобы \
         отметить вторую цель. Вы всегда будете меняться местами с основной целью.",
    ),
    item(
        "/datum/spellbook_entry/item/staffdoor",
        "Staff of Door Creation",
        MOBILITY,
        1,
        "/obj/item/gun/magic/staff/door",
        "Особый посох, способный превращать твердые стены в украшенные двери. Пригодится для \
         передвижения при отсутствии другого вида передвижения. Не работает со стеклом.",
    ),
    spell(
        "/datum/spellbook_entry/summonitem",
        "Summon Item",
        ASSISTANCE,
        1,
        Spell::SummonItem,
        "Возвращает в вашу руку ранее отмеченный предмет из любой точки вселенной.",
    ),
    spell(
        "/datum/spellbook_entry/charge",
        "Charge",
        ASSISTANCE,
        1,
        Spell::Charge,
        "Это заклинание можно использовать для подзарядки самых разных предметов в ваших руках, \
         от магических артефактов до электрических компонентов. Креативный волшебник может даже \
         использовать его, чтобы наделить магической силой своего товарища.",
    ),
    item(
        "/datum/spellbook_entry/item/contract",
        "Contract of Apprenticeship",
        ASSISTANCE,
        2,
        CONTRACT,
        "Магический контракт, привязывающий ученика волшебника к вашей службе, при \
         использовании вызовет его на вашу сторону.",
    ),
    EntryDef {
        id: "/datum/spellbook_entry/summon/guns",
        name: "Summon Guns",
        desc: "Нет ничего плохого в том, чтобы вооружить команду сумасшедших, которые только и \
               ждут повода, чтобы убить вас. Но велика вероятность, что они сперва перестреляют \
               друг друга.",
        category: RITUALS,
        cost: 2,
        kind: EntryKind::Ritual(Ritual::SummonGuns),
    },
    EntryDef {
        id: "/datum/spellbook_entry/summon/events",
        name: "Summon Events",
        desc: "Подтолкните закон Мерфи и замените все события на специальные магические, которые \
               запутают и собьют с толку всех. Многократное использование увеличивает частоту \
               этих событий.",
        category: RITUALS,
        cost: 2,
        kind: EntryKind::Ritual(Ritual::SummonEvents),
    },
];

pub fn entry(id: &str) -> Option<(usize, &'static EntryDef)> {
    ENTRIES.iter().enumerate().find(|(_, e)| e.id == id)
}

/// The apprentice schools (`APPRENTICE_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum School {
    Destruction,
    Bluespace,
    Healing,
    Robeless,
}

impl School {
    pub const ALL: [School; 4] = [
        School::Destruction,
        School::Bluespace,
        School::Healing,
        School::Robeless,
    ];

    /// The `APPRENTICE_*` key.
    pub fn key(self) -> &'static str {
        match self {
            School::Destruction => "destruction",
            School::Bluespace => "bluespace",
            School::Healing => "healing",
            School::Robeless => "robeless",
        }
    }

    /// `apprentice/equip_wizard`: the spells taught.
    pub fn spells(self) -> &'static [Spell] {
        match self {
            School::Destruction => &[Spell::MagicMissile, Spell::Fireball],
            School::Bluespace => &[Spell::Teleport, Spell::EtherealJaunt],
            School::Healing => &[Spell::Charge, Spell::Forcewall],
            // shortcut: mindswap is not ported.
            School::Robeless => &[Spell::Knock],
        }
    }

    fn lesson(self, master: &str) -> String {
        match self {
            School::Destruction => format!(
                "Однако ваша служба не прошла даром. Учась у {master}, вы выучили мощные, \
                 разрушительные заклинания. Вы можете использовать заклинания «Magic Missile» и \
                 «Fireball»."
            ),
            School::Bluespace => format!(
                "Однако ваша служба не прошла даром. Учась у {master}, вы изучили заклинания \
                 перемещения, изменяющие реальность. Вы можете использовать заклинания «Teleport» \
                 и «Ethereal Jaunt»."
            ),
            School::Healing => format!(
                "Однако ваша служба не прошла даром. Учась у {master}, вы изучили \
                 жизненно-спасительные заклинания выживания. Вы можете использовать заклинания \
                 «Charge» и «Forcewall», а также владеете посохом исцеления."
            ),
            School::Robeless => format!(
                "Однако ваша служба не прошла даром. Учась у {master}, Вы выучили скрытные \
                 заклинания, не нуждающиеся в магических одеяниях. Вы можете использовать \
                 заклинание «Knock»."
            ),
        }
    }
}

/// A wizard objective.
#[derive(Clone, Debug, PartialEq)]
pub enum WizardObjective {
    Common(Objective),
    /// `/datum/objective/survive`.
    Survive,
    /// The apprentice's `/datum/objective/protect` of its master.
    Protect {
        target: Entity,
        text: String,
    },
}

impl WizardObjective {
    pub fn explanation(&self) -> String {
        match self {
            Self::Common(o) => o.explanation.clone(),
            Self::Survive => "Оставайтесь в живых до самого конца.".to_owned(),
            Self::Protect { text, .. } => text.clone(),
        }
    }

    pub fn check_completion(&self, world: &World, owner: Entity, end: RoundEnd) -> bool {
        match self {
            Self::Common(o) => o.check_completion(world, &[owner], end),
            Self::Survive => considered_alive(world, owner),
            Self::Protect { target, .. } => considered_alive(world, *target),
        }
    }
}

/// The wizard (or apprentice) antagonist datum, on its body.
#[derive(Component, Clone, Debug)]
pub struct Wizard {
    pub key: String,
    /// The apprentice's `master`.
    pub master: Option<Entity>,
    pub school: Option<School>,
    pub objectives: Vec<WizardObjective>,
    /// `GLOB.wizard_spellbook_purchases_by_key`: entry name and times.
    pub purchases: Vec<(&'static str, u32)>,
}

impl Wizard {
    pub fn antag_name(&self) -> &'static str {
        if self.master.is_some() {
            APPRENTICE_NAME
        } else {
            WIZARD_NAME
        }
    }

    fn info(&self) -> BrotherInfo {
        BrotherInfo {
            antag_name: self.antag_name().to_owned(),
            objectives: self
                .objectives
                .iter()
                .enumerate()
                .map(|(i, o)| ObjectiveInfo {
                    count: i as u32 + 1,
                    explanation: o.explanation(),
                    complete: false,
                })
                .collect(),
        }
    }
}

/// `/obj/item/spellbook`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Spellbook {
    /// `owner`: the mind (body) it is bound to.
    pub owner: Option<Entity>,
    pub owner_name: String,
    pub uses: u32,
    pub refunds_allowed: bool,
    /// Each entry's `times`, in [`ENTRIES`] order.
    pub times: Vec<u32>,
    /// `prepare_spells`' `can_be_purchased()` of the rituals: the round was high chaos when the
    /// book was made (and `NO_SUMMON_EVENTS` is off, as in BandaStation's config).
    pub rituals: bool,
}

impl Spellbook {
    /// `entries`: what the book offers, with each one's index in [`ENTRIES`].
    pub fn entries(&self) -> impl Iterator<Item = (usize, &'static EntryDef)> + '_ {
        ENTRIES
            .iter()
            .enumerate()
            .filter(|(_, e)| self.rituals || !matches!(e.kind, EntryKind::Ritual(_)))
    }

    pub fn offers(&self, i: usize) -> bool {
        self.entries().any(|(j, _)| j == i)
    }
}

impl Default for Spellbook {
    fn default() -> Self {
        Self {
            owner: None,
            owner_name: String::new(),
            uses: SPELLBOOK_USES,
            refunds_allowed: true,
            times: vec![0; ENTRIES.len()],
            rituals: false,
        }
    }
}

/// `/obj/item/antag_spawner/contract`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ApprenticeContract {
    pub used: bool,
}

/// Insert before `GameServerPlugin` to say where the den template comes from.
#[derive(Resource, Clone, Debug, Default)]
pub struct WizardConfig {
    /// `None`: [`DEN_MAP`] of the BYOND checkout, with a real station map.
    pub den: Option<PathBuf>,
}

/// The den loaded with the station: its level and its wizard start tiles.
#[derive(Resource, Clone, Debug, Default)]
pub struct WizardDen {
    pub z: Option<u16>,
    pub spawns: Vec<TilePos>,
    next: usize,
}

#[derive(Resource, Clone)]
struct DenCandidate(PathBuf);

#[derive(Resource, Clone)]
struct DenTemplate(PathBuf);

/// `GLOB.wizard_first`, `GLOB.wizard_second`.
#[derive(Resource, Default)]
struct WizardNames {
    first: Vec<String>,
    second: Vec<String>,
}

#[derive(Resource)]
struct WizardRng(Rng);

/// A prompt of a wizard waiting for its answer.
#[derive(Component, Clone, Copy, Debug)]
enum WizardPrompt {
    /// `rename_wizard`: the default name.
    Rename,
    /// The contract's school.
    Contract { contract: Entity },
}

#[derive(Component, Clone, Debug)]
struct RenameDefault(String);

/// The wizard's info panel (tg's default `AntagInfoGeneric`).
pub struct WizardInfoWindow;

impl UiInterface for WizardInfoWindow {
    const NAME: &'static str = crate::antag::cult::AntagInfoGenericUi::NAME;
    type Data = ();
    type StaticData = BrotherInfo;
}

impl ServerUi for WizardInfoWindow {
    type Source = &'static Wizard;
    type UserParam = ();
    type Act = ();

    fn static_data(wizard: &Wizard) -> BrotherInfo {
        wizard.info()
    }

    fn data(_: &Wizard, _: Entity, _: &()) {}
}

/// A parsed spellbook act.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpellbookAct {
    Purchase(String),
    Refund(String),
}

impl ServerUi for SpellbookUi {
    type Source = &'static Spellbook;
    type UserParam = ();
    type Act = SpellbookAct;

    fn static_data(book: &Spellbook) -> SpellbookStatic {
        let mut categories: Vec<String> = Vec::new();
        for (_, e) in book.entries() {
            if !categories.iter().any(|c| c == e.category) {
                categories.push(e.category.to_owned());
            }
        }
        SpellbookStatic {
            entries: book
                .entries()
                .map(|(_, e)| SpellbookEntry {
                    id: e.id.to_owned(),
                    name: e.name.to_owned(),
                    desc: e.desc.to_owned(),
                    category: e.category.to_owned(),
                    cost: e.cost,
                    refundable: e.refundable(),
                    buyword: e.buy_word().to_owned(),
                    requires_wizard_garb: matches!(e.kind, EntryKind::Spell(s)
                        if s.def().requirements & crate::spells::SPELL_REQUIRES_WIZARD_GARB != 0),
                    limit: e.limit(),
                })
                .collect(),
            categories,
        }
    }

    fn data(book: &Spellbook, _: Entity, _: &()) -> SpellbookData {
        SpellbookData {
            owner: book.owner_name.clone(),
            points: book.uses,
            times: book
                .entries()
                .map(|(i, _)| book.times.get(i).copied().unwrap_or(0))
                .collect(),
            // `set_spell_info`: the spell's initial cooldown.
            cooldowns: book
                .entries()
                .map(|(_, e)| match e.kind {
                    EntryKind::Spell(s) => Some(s.def().cooldown as f32),
                    EntryKind::Item(_) | EntryKind::Ritual(_) => None,
                })
                .collect(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<SpellbookAct, UiReject> {
        let spellref = || {
            params
                .get("spellref")
                .filter(|r| entry(r).is_some())
                .map(str::to_owned)
                .ok_or_else(|| UiReject::BadParams("spellref: expected an entry".into()))
        };
        match action {
            PURCHASE => spellref().map(SpellbookAct::Purchase),
            REFUND => spellref().map(SpellbookAct::Refund),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

pub struct WizardPlugin;

impl Plugin for WizardPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // The spellbook opens, the scroll of teleportation and the contract ask.
        app.add_click_handler(ClickPhase::AttackSelf, 700, |w, c| {
            crate::antag::wizard::server::attack_self(w, c.client, c.mob, c.target)
        });
        if !app.world().contains_resource::<WizardConfig>() {
            app.init_resource::<WizardConfig>();
        }
        let config = app.world().resource::<WizardConfig>().clone();
        let den = config.den.or_else(|| {
            let map = app.world().get_resource::<MapLoadPlugin>()?;
            // Only with a real station map: fixture maps name `.dmm` files.
            map.maps.first().filter(|m| !m.ends_with(".dmm"))?;
            let path = map.source.join(DEN_MAP);
            ss13_data::exists(&path).then_some(path)
        });
        if let Some(den) = den {
            app.insert_resource(DenCandidate(den));
        }
        app.register_ui::<SpellbookUi>()
            .register_ui::<WizardInfoWindow>()
            .init_resource::<WizardDen>()
            .add_systems(Startup, (load_names, find_den))
            .add_systems(Update, adopt_items)
            .add_systems(
                Update,
                (
                    open_info,
                    book_acts,
                    prompt_answers.after(PromptSystems::Answer),
                )
                    .in_set(UiSystems::Handle),
            );
    }

    /// The den loads with the station only when every type on it has a prototype (assets
    /// converted with it); prototypes exist from `ProtoPlugin::finish` on.
    fn finish(&self, app: &mut App) {
        let Some(den) = app
            .world()
            .get_resource::<DenCandidate>()
            .map(|d| d.0.clone())
        else {
            return;
        };
        let missing = missing_types(app.world(), &den);
        if !missing.is_empty() {
            warn!(
                "wizard den {} not loaded: no prototypes for {missing:?} (rerun `ss13-mapload \
                 convert`)",
                den.display()
            );
            return;
        }
        info!("wizard den template {}", den.display());
        let world = app.world_mut();
        match world.get_resource_mut::<ShuttleTemplates>() {
            Some(mut templates) => templates.0.push(den.clone()),
            None => {
                world.insert_resource(ShuttleTemplates(vec![den.clone()]));
            }
        }
        app.insert_resource(DenTemplate(den));
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers

use crate::tools::tell_mob as chat_to;

#[cfg(test)]
mod tests;
