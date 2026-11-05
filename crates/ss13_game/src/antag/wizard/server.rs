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
//! - [`roundend_report`]: each wizard, its objectives and what it bought.
// shortcut: no Grand Ritual, perks, summons, challenges, loadouts or randomized books, no team
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

/// What an entry gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Spell(Spell),
    /// `/datum/spellbook_entry/item`: summoned into the hands.
    Item(&'static str),
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
    /// `limit`: a spell's `spell_max_level`, items none.
    pub fn limit(&self) -> Option<u32> {
        match self.kind {
            EntryKind::Spell(spell) => Some(u32::from(spell.def().max_level)),
            EntryKind::Item(_) => None,
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
pub const ENTRIES: [EntryDef; 21] = [
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
}

impl Default for Spellbook {
    fn default() -> Self {
        Self {
            owner: None,
            owner_name: String::new(),
            uses: SPELLBOOK_USES,
            refunds_allowed: true,
            times: vec![0; ENTRIES.len()],
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

    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
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

    fn static_data(_: &Spellbook) -> SpellbookStatic {
        let mut categories: Vec<String> = Vec::new();
        for e in &ENTRIES {
            if !categories.iter().any(|c| c == e.category) {
                categories.push(e.category.to_owned());
            }
        }
        SpellbookStatic {
            entries: ENTRIES
                .iter()
                .map(|e| SpellbookEntry {
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
            times: book.times.clone(),
            // `set_spell_info`: the spell's initial cooldown.
            cooldowns: ENTRIES
                .iter()
                .map(|e| match e.kind {
                    EntryKind::Spell(s) => Some(s.def().cooldown as f32),
                    EntryKind::Item(_) => None,
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

/// The types the den places that have no prototype (all of them without prototypes).
fn missing_types(world: &World, den: &std::path::Path) -> Vec<String> {
    let map = match DmmMap::from_file(den) {
        Ok(map) => map,
        Err(e) => return vec![format!("{}: {e}", den.display())],
    };
    let Some(protos) = world.get_resource::<ss13_proto::Prototypes>() else {
        return vec!["(no prototypes)".to_owned()];
    };
    let mut missing: Vec<String> = map
        .prefabs
        .iter()
        .flat_map(|p| p.objects.iter())
        .map(|o| o.path.clone())
        .filter(|p| protos.get(p).is_none())
        .collect();
    missing.sort();
    missing.dedup();
    missing
}

fn load_names(world: &mut World) {
    let seed = world
        .get_resource::<ss13_mapload::MapSeed>()
        .map_or(0, |s| s.0);
    world.insert_resource(WizardRng(Rng::new(seed ^ 0x7769_7a61_7264)));
    let source = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.source.clone());
    let read = |file: &str| -> Vec<String> {
        source
            .as_ref()
            .and_then(|s| ss13_data::read(&s.join("strings/names").join(file)).ok())
            .map(|bytes| {
                String::from_utf8_lossy(&bytes)
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    world.insert_resource(WizardNames {
        first: read("wizardfirst.txt"),
        second: read("wizardsecond.txt"),
    });
}

/// `GLOB.wizardstart`: the den's start landmarks.
fn find_den(world: &mut World) {
    let Some(path) = world.get_resource::<DenTemplate>().map(|d| d.0.clone()) else {
        return;
    };
    let Some(first_z) = world
        .get_resource::<MapLoadReport>()
        .and_then(|r| r.maps.iter().find(|m| m.path == path).map(|m| m.first_z))
    else {
        warn!("wizard den {} was not loaded", path.display());
        return;
    };
    let Ok(map) = DmmMap::from_file(&path) else {
        return;
    };
    let marks: Vec<bool> = map
        .prefabs
        .iter()
        .map(|p| p.objects.iter().any(|o| o.path == WIZARD_START))
        .collect();
    let mut spawns = Vec::new();
    let (sx, sy, sz) = map.size;
    for mz in 1..=sz {
        for y in 1..=sy {
            for x in 1..=sx {
                let Some(i) = map.index(x, y, mz).map(|i| i as usize) else {
                    continue;
                };
                if marks.get(i).copied().unwrap_or(false) {
                    spawns.push(TilePos::new(x as i32, y as i32, first_z + (mz - 1) as u16));
                }
            }
        }
    }
    info!("wizard den at z{first_z}: {} start tiles", spawns.len());
    world.insert_resource(WizardDen {
        z: Some(first_z),
        spawns,
        next: 0,
    });
}

/// Items spawned from the map, bought or summoned get their state.
fn adopt_items(
    mut commands: Commands,
    added: Query<(Entity, &DmItem), Added<DmItem>>,
    books: Query<(), With<Spellbook>>,
    scrolls: Query<(), With<TeleportScroll>>,
    contracts: Query<(), With<ApprenticeContract>>,
) {
    for (e, item) in &added {
        if item.path == SPELLBOOK && books.get(e).is_err() {
            commands.entity(e).insert(Spellbook::default());
        } else if is_under(&item.path, SCROLL) && scrolls.get(e).is_err() {
            let uses = if is_under(&item.path, APPRENTICE_SCROLL) {
                APPRENTICE_SCROLL_USES
            } else {
                SCROLL_USES
            };
            commands.entity(e).insert(TeleportScroll { uses });
        } else if item.path == CONTRACT && contracts.get(e).is_err() {
            commands.entity(e).insert(ApprenticeContract::default());
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Helpers

fn chat_to(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

fn line(world: &mut World, mob: Entity, segments: Vec<ChatSegment>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage {
                channel: ChatChannel::System,
                segments,
            },
        });
    }
}

fn with_rng<R>(world: &mut World, f: impl FnOnce(&mut World, &mut Rng) -> R) -> R {
    let mut rng = world
        .remove_resource::<WizardRng>()
        .map_or_else(|| Rng::new(0), |r| r.0);
    let out = f(world, &mut rng);
    world.insert_resource(WizardRng(rng));
    out
}

/// `pick(GLOB.wizard_first) pick(GLOB.wizard_second)`.
pub fn random_name(world: &mut World) -> String {
    let (first, second) = world
        .get_resource::<WizardNames>()
        .map(|n| (n.first.clone(), n.second.clone()))
        .unwrap_or_default();
    with_rng(world, |_, rng| {
        let mut pick = |list: &[String], fallback: &str| {
            if list.is_empty() {
                fallback.to_owned()
            } else {
                list[rng.range(0, list.len() as i64 - 1) as usize].clone()
            }
        };
        let a = pick(&first, "Мерлин");
        let b = pick(&second, "Мудрый");
        format!("{a} {b}")
    })
}

fn hud_icon(world: &World, state: &str) -> Option<ss13_shared::IconStateId> {
    world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(ANTAG_HUD_DMI, state))
}

fn name_of(world: &World, e: Entity) -> String {
    crate::medical::name_of(world, e)
}

/// `path` spawned on `mob` (tg `equip_to_slot`): replicated and carried. `None` without its
/// prototype.
fn carried(world: &mut World, mob: Entity, path: &str) -> Option<Entity> {
    let item = crate::construction::spawn_detached(world, path)?;
    world.entity_mut(item).insert((Networked, CarriedBy(mob)));
    Some(item)
}

/// `path` spawned into `pack` (`equip_to_storage`).
fn into_pack(world: &mut World, pack: Entity, path: &str) -> Option<Entity> {
    let item = crate::construction::spawn_detached(world, path)?;
    world.entity_mut(item).insert(Networked);
    crate::storage::put_inside(world, item, pack);
    Some(item)
}

/// `put_in_hands`, else on the floor under `mob`; true when in a hand.
fn give(world: &mut World, mob: Entity, item: Entity) -> bool {
    let hand = world.get::<Inventory>(mob).and_then(|i| {
        [i.active, 1 - i.active]
            .into_iter()
            .find(|&h| i.hands[h].is_none())
    });
    match hand {
        Some(h) => {
            world.entity_mut(item).insert((Networked, CarriedBy(mob)));
            if let Some(mut inv) = world.get_mut::<Inventory>(mob) {
                inv.hands[h] = Some(item);
            }
            crate::inventory::refresh_look(world, mob);
            true
        }
        None => {
            let pos = world.get::<TilePos>(mob).copied().unwrap_or_default();
            world.entity_mut(item).insert((Networked, pos));
            crate::construction::tile_changed(world, pos);
            false
        }
    }
}

/// `delete_equipment`: everything the body carries goes.
fn strip(world: &mut World, mob: Entity) {
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        let items: Vec<Entity> = inventory
            .equipped()
            .chain(inventory.hands.iter().flatten().copied())
            .collect();
        for item in items {
            let inside: Vec<Entity> = world
                .get::<Stored>(item)
                .map(|s| s.items().to_vec())
                .unwrap_or_default();
            for e in inside {
                if world.get_entity(e).is_ok() {
                    world.despawn(e);
                }
            }
            if world.get_entity(item).is_ok() {
                world.despawn(item);
            }
        }
        let active = inventory.active;
        world.entity_mut(mob).insert(Inventory {
            active,
            ..Inventory::default()
        });
    }
}

/// `/datum/outfit/wizard` (`apprentice`: `/datum/outfit/wizard/apprentice`) and `post_equip`.
fn equip(world: &mut World, mob: Entity, apprentice: bool) {
    let mut inv = world.get::<Inventory>(mob).cloned().unwrap_or_default();
    inv.uniform = carried(world, mob, LIGHTPURPLE);
    inv.suit = carried(world, mob, WIZROBE);
    inv.ears = carried(world, mob, HEADSET);
    inv.head = carried(world, mob, WIZARD_HAT);
    inv.shoes = carried(world, mob, MAGIC_SANDALS);
    let scroll = carried(
        world,
        mob,
        if apprentice {
            APPRENTICE_SCROLL
        } else {
            SCROLL
        },
    );
    if let Some(scroll) = scroll {
        let uses = if apprentice {
            APPRENTICE_SCROLL_USES
        } else {
            SCROLL_USES
        };
        world.entity_mut(scroll).insert(TeleportScroll { uses });
    }
    inv.pockets[1] = scroll;
    if !apprentice {
        inv.hands[0] = carried(world, mob, STAFF);
    }
    world.entity_mut(mob).insert(inv.clone());
    inv.back = crate::storage::spawn_backpack(world, mob);
    world.entity_mut(mob).insert(inv.clone());
    if let Some(pack) = inv.back {
        into_pack(world, pack, SURVIVAL_BOX);
        if !apprentice && let Some(book) = into_pack(world, pack, SPELLBOOK) {
            // `post_equip`: `new_spellbook.owner = wizard.mind`.
            let owner_name = name_of(world, mob);
            world.entity_mut(book).insert(Spellbook {
                owner: Some(mob),
                owner_name,
                ..default()
            });
        }
    }
    crate::inventory::refresh_look(world, mob);
}

// ---------------------------------------------------------------------------------------------
// Gaining the wizard

/// The next den start tile (`pick(GLOB.wizardstart)`), `None` without a den.
fn den_spawn(world: &mut World) -> Option<TilePos> {
    let den = world.resource::<WizardDen>().clone();
    if den.spawns.is_empty() {
        return None;
    }
    let (i, pos) = crate::server::pick_free(world, &den.spawns, den.next);
    world.resource_mut::<WizardDen>().next = i + 1;
    Some(pos)
}

/// The rulesets' `assign_role` for the players they took: a body in the den each.
pub fn spawn_wizards(world: &mut World, clients: &[Entity]) {
    for &client in clients {
        let (pos, rushed) = match den_spawn(world) {
            Some(pos) => (pos, false),
            None => (crate::server::pick_latejoin(world), true),
        };
        let name = random_name(world);
        let mob = crate::server::spawn_mob(world, client, pos, name);
        world.entity_mut(client).remove::<crate::round::NewPlayer>();
        add_wizard(world, mob);
        if rushed {
            tell(
                world,
                client,
                Span::Plain,
                "ПРЯМО СЕЙЧАС, ПОШЕЛ ПОШЕЛ ПОШЕЛ",
            );
        }
    }
}

/// Dev (`SS13_DEV_TRAITOR=wizard`): a joined crew body becomes a wizard in the den (a dev spawn
/// on the den's level is kept).
pub fn dev_wizard(world: &mut World, client: Entity, mob: Entity) {
    let den_z = world.resource::<WizardDen>().z;
    let kept = crate::server::dev_spawn(world, client).is_some_and(|p| Some(p.z) == den_z);
    if !kept && let Some(pos) = den_spawn(world) {
        world.entity_mut(mob).insert(pos);
    }
    add_wizard(world, mob);
}

/// `add_antag_datum(/datum/antagonist/wizard)`: stripped and dressed, objectives, greeting, HUD,
/// info panel, the rename prompt. `false` if it already is one or is not a living mob.
pub fn add_wizard(world: &mut World, mob: Entity) -> bool {
    if world.get::<Wizard>(mob).is_some() || world.get::<Health>(mob).is_none() {
        return false;
    }
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    strip(world, mob);
    // `owner.set_assigned_role(space_wizard)`, `set_species(/datum/species/human)`.
    world.entity_mut(mob).remove::<crate::jobs::PlayerJob>();
    crate::species::set_species(world, mob, crate::species::SpeciesId::Human);
    equip(world, mob, false);
    let objectives = with_rng(world, |world, rng| forge_objectives(world, mob, rng));
    let hud_icon = hud_icon(world, "wizard");
    world.entity_mut(mob).insert((
        Wizard {
            key,
            master: None,
            school: None,
            objectives,
            purchases: Vec::new(),
        },
        AntagHudData {
            hud_icon,
            ..default()
        },
    ));
    greet(world, mob, None);
    if let Some(client) = client {
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
        world.write_message(OpenUi::new::<WizardInfoWindow>(
            client,
            mob,
            mob,
            WIZARD_NAME,
        ));
        rename_prompt(world, client, mob);
    }
    info!("{mob} is a wizard");
    true
}

/// `create_objectives`.
fn forge_objectives(world: &mut World, owner: Entity, rng: &mut Rng) -> Vec<WizardObjective> {
    let targets: Vec<StealItemDef> = world
        .get_resource::<crate::antag::uplink::UplinkCatalog>()
        .map(|c| c.data.steal_items.clone())
        .unwrap_or_default();
    let common = |world: &World, kind: ObjectiveKind| {
        WizardObjective::Common(Objective {
            explanation: kind.explanation(world),
            kind,
        })
    };
    let mut out = Vec::new();
    match rng.range(1, 100) {
        1..=30 => {
            let target = find_target(world, &[owner], &[], assassinate, rng);
            out.push(common(&*world, assassinate(target)));
            out.push(common(&*world, ObjectiveKind::Escape));
        }
        31..=60 => {
            let item = find_steal_target(world, &[owner], &[], &targets, rng);
            out.push(common(&*world, ObjectiveKind::Steal { item }));
            out.push(common(&*world, ObjectiveKind::Escape));
        }
        61..=85 => {
            let target = find_target(world, &[owner], &[], assassinate, rng);
            out.push(common(&*world, assassinate(target)));
            let item = find_steal_target(world, &[owner], &[], &targets, rng);
            out.push(common(&*world, ObjectiveKind::Steal { item }));
            out.push(WizardObjective::Survive);
        }
        _ => out.push(common(&*world, ObjectiveKind::Hijack)),
    }
    out
}

fn assassinate(target: Option<Entity>) -> ObjectiveKind {
    ObjectiveKind::Assassinate { target }
}

/// `greet` and `announce_objectives`.
fn greet(world: &mut World, mob: Entity, lesson: Option<String>) {
    let Some(wizard) = world.get::<Wizard>(mob).cloned() else {
        return;
    };
    match wizard.master {
        Some(master) => {
            let master = name_of(world, master);
            chat_to(
                world,
                mob,
                Span::Bold,
                format!(
                    "Вы ученик {master}! Вы связаны магическим контрактом, чтобы следовать \
                     приказам и помогать в выполнении целей своего учителя."
                ),
            );
            if let Some(lesson) = lesson {
                chat_to(world, mob, Span::Bold, lesson);
            }
        }
        None => chat_to(
            world,
            mob,
            Span::Bold,
            format!("Вы - {}.", wizard.antag_name()),
        ),
    }
    chat_to(world, mob, Span::Notice, "Ваши текущие задачи:");
    for (i, o) in wizard.objectives.iter().enumerate() {
        line(
            world,
            mob,
            vec![
                ChatSegment::new(Span::Bold, format!("{OBJECTIVE_NAME} #{}", i + 1)),
                ChatSegment::new(Span::Plain, format!(": {}", o.explanation())),
            ],
        );
    }
}

/// `rename_wizard`.
fn rename_prompt(world: &mut World, client: Entity, mob: Entity) {
    let default = name_of(world, mob);
    let ask = Ask::new(
        client,
        mob,
        "Смена имени",
        format!("Ваше имя - {WIZARD_NAME}. Вы хотите изменить его на другое?"),
        PromptKind::Text {
            default: default.clone(),
            max_length: MAX_NAME_LEN,
            multiline: false,
        },
    );
    let prompt = ask_world(world, ask);
    world
        .entity_mut(prompt)
        .insert((WizardPrompt::Rename, RenameDefault(default)));
}

/// `add_antag_datum(/datum/antagonist/wizard/apprentice)` on a fresh body for `master`.
pub fn add_apprentice(world: &mut World, mob: Entity, master: Entity, school: School) -> bool {
    if world.get::<Wizard>(mob).is_some() || world.get::<Health>(mob).is_none() {
        return false;
    }
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    strip(world, mob);
    world.entity_mut(mob).remove::<crate::jobs::PlayerJob>();
    crate::species::set_species(world, mob, crate::species::SpeciesId::Human);
    equip(world, mob, true);
    for &spell in school.spells() {
        crate::spells::learn(world, mob, spell);
    }
    if school == School::Healing
        && let Some(staff) =
            crate::construction::spawn_detached(world, ss13_mapload::antag::HEALING_STAFF)
    {
        crate::spells::magic_guns::adopt_full(world, staff);
        give(world, mob, staff);
    }
    let master_name = name_of(world, master);
    let hud_icon = hud_icon(world, "apprentice");
    world.entity_mut(mob).insert((
        Wizard {
            key,
            master: Some(master),
            school: Some(school),
            objectives: vec![WizardObjective::Protect {
                target: master,
                text: format!("Защищайте волшебника - {master_name}."),
            }],
            purchases: Vec::new(),
        },
        AntagHudData {
            hud_icon,
            ..default()
        },
    ));
    greet(world, mob, Some(school.lesson(&master_name)));
    if let Some(client) = client {
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
    }
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/effects/magic.ogg", 50.0, false),
        );
    }
    info!("{mob} is the {} apprentice of {master}", school.key());
    true
}

// ---------------------------------------------------------------------------------------------
// The spellbook

/// The book is on `mob` (in its hands, worn or in a bag it carries).
fn holds(world: &World, mob: Entity, item: Entity) -> bool {
    let mut e = item;
    for _ in 0..4 {
        if world.get::<CarriedBy>(e).is_some_and(|c| c.0 == mob) {
            return true;
        }
        match world.get::<StoredIn>(e) {
            Some(s) => e = s.0,
            None => return false,
        }
    }
    false
}

/// `can_buy` + `buy_spell` + `purchase_entry` for `mob` with `book`. True when bought.
pub fn purchase(world: &mut World, mob: Entity, book: Entity, id: &str) -> bool {
    let Some((i, def)) = entry(id) else {
        return false;
    };
    let Some(state) = world.get::<Spellbook>(book).cloned() else {
        return false;
    };
    let times = state.times.get(i).copied().unwrap_or(0);
    if state.uses < def.cost || def.limit().is_some_and(|l| times >= l) {
        return false;
    }
    let bought = match def.kind {
        EntryKind::Spell(spell) => {
            let before = world
                .get::<Spells>(mob)
                .and_then(|s| s.get(spell))
                .map(|k| k.name());
            match crate::spells::learn(world, mob, spell) {
                Learned::New => {
                    chat_to(
                        world,
                        mob,
                        Span::Notice,
                        format!("Вы выучили - {}.", spell.def().name),
                    );
                    true
                }
                Learned::Leveled(_) => {
                    let after = world
                        .get::<Spells>(mob)
                        .and_then(|s| s.get(spell))
                        .map(|k| k.name())
                        .unwrap_or_default();
                    chat_to(
                        world,
                        mob,
                        Span::Notice,
                        format!(
                            "Вы улучшили {} в {after}.",
                            before.unwrap_or_else(|| spell.def().name.to_owned())
                        ),
                    );
                    true
                }
                Learned::MaxLevel => {
                    chat_to(
                        world,
                        mob,
                        Span::Warning,
                        "Это заклинание не может быть болeе улучшено!",
                    );
                    false
                }
            }
        }
        EntryKind::Item(path) => summon_item(world, mob, path),
    };
    if !bought {
        return false;
    }
    if let Some(mut book) = world.get_mut::<Spellbook>(book) {
        if let Some(t) = book.times.get_mut(i) {
            *t += 1;
        }
        book.uses -= def.cost;
    }
    if let Some(mut wizard) = world.get_mut::<Wizard>(mob) {
        match wizard.purchases.iter_mut().find(|(n, _)| *n == def.name) {
            Some((_, n)) => *n += 1,
            None => wizard.purchases.push((def.name, 1)),
        }
    }
    info!("{mob} bought {} from the spellbook", def.name);
    true
}

/// `/datum/spellbook_entry/item/buy_spell` + `try_equip_item`.
fn summon_item(world: &mut World, mob: Entity, path: &str) -> bool {
    let Some(item) = crate::construction::spawn_detached(world, path) else {
        warn!("no prototype for {path}");
        return false;
    };
    let name = name_of(world, item);
    if path == WAND_BELT {
        fill_wand_belt(world, item);
        let free = world
            .get::<Inventory>(mob)
            .is_some_and(|i| i.belt.is_none());
        if free {
            world.entity_mut(item).insert((Networked, CarriedBy(mob)));
            if let Some(mut inv) = world.get_mut::<Inventory>(mob) {
                inv.belt = Some(item);
            }
            crate::inventory::refresh_look(world, mob);
        } else {
            let pos = world.get::<TilePos>(mob).copied().unwrap_or_default();
            world.entity_mut(item).insert((Networked, pos));
            crate::construction::tile_changed(world, pos);
        }
        let place = if free {
            "на ваш пояс"
        } else {
            "у ваших ног"
        };
        chat_to(
            world,
            mob,
            Span::Notice,
            format!("{} призывается {place}.", capitalize(&name)),
        );
        return true;
    }
    crate::spells::magic_guns::adopt_full(world, item);
    if path == CONTRACT {
        world.entity_mut(item).insert(ApprenticeContract::default());
    }
    let in_hands = give(world, mob, item);
    let place = if in_hands {
        "в ваши руки"
    } else {
        "у ваших ног"
    };
    chat_to(
        world,
        mob,
        Span::Notice,
        format!("Приобретенный {name} призывается {place}."),
    );
    true
}

/// `wands/full/PopulateContents`: the wands, "in the best possible condition".
fn fill_wand_belt(world: &mut World, belt: Entity) {
    let inside: Vec<Entity> = world
        .get::<Stored>(belt)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    let wands = if inside.is_empty() {
        BELT_WANDS
            .iter()
            .filter_map(|&p| into_pack(world, belt, p))
            .collect()
    } else {
        inside
    };
    for wand in wands {
        crate::spells::magic_guns::adopt_full(world, wand);
    }
}

/// `can_refund` + `refund_spell`: only spells, only in the den. True when refunded.
pub fn refund(world: &mut World, mob: Entity, book: Entity, id: &str) -> bool {
    let Some((i, def)) = entry(id) else {
        return false;
    };
    let EntryKind::Spell(spell) = def.kind else {
        return false;
    };
    let allowed = world
        .get::<Spellbook>(book)
        .is_some_and(|b| b.refunds_allowed);
    if !allowed || !crate::spells::knows(world, mob, spell) {
        return false;
    }
    let in_den = world
        .get::<TilePos>(mob)
        .is_some_and(|&p| crate::spells::in_wizard_den(world, p));
    if !in_den {
        chat_to(
            world,
            mob,
            Span::Warning,
            "Вы можете вернуть очки за заклинания только в логове волшебника!",
        );
        return false;
    }
    let level = crate::spells::forget(world, mob, spell);
    let amount = u32::from(level) * def.cost;
    if amount == 0 {
        return false;
    }
    if let Some(mut book) = world.get_mut::<Spellbook>(book) {
        if let Some(t) = book.times.get_mut(i) {
            *t = 0;
        }
        book.uses += amount;
    }
    if let Some(mut wizard) = world.get_mut::<Wizard>(mob) {
        wizard.purchases.retain(|(n, _)| *n != def.name);
    }
    info!("{mob} refunded {} for {amount} points", def.name);
    true
}

fn book_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<SpellbookUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, SpellbookAct)> = acts
        .read(world.resource::<Messages<UiActed<SpellbookUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, book, act) in pending {
        // At the moment of effect: the owner's own body, holding the book.
        if world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world.get::<Spellbook>(book).and_then(|b| b.owner) != Some(actor)
            || !holds(world, actor, book)
            || world.get::<Stat>(actor) != Some(&Stat::Conscious)
        {
            continue;
        }
        if world.get::<HumanLook>(actor).is_none() {
            tell(
                world,
                client,
                Span::Warning,
                "Книга, похоже, не слушает низшие формы жизни.",
            );
            continue;
        }
        match act {
            SpellbookAct::Purchase(id) => purchase(world, actor, book, &id),
            SpellbookAct::Refund(id) => refund(world, actor, book, &id),
        };
    }
}

/// `attack_self` of the wizard's items: the spellbook binds or opens, the scroll and the
/// contract ask. `false` for other items.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if let Some(book) = world.get::<Spellbook>(item).cloned() {
        let name = name_of(world, item);
        match book.owner {
            None => {
                tell(
                    world,
                    client,
                    Span::Notice,
                    format!("Вы привязываете {name} к себе."),
                );
                let owner_name = name_of(world, mob);
                if let Some(mut b) = world.get_mut::<Spellbook>(item) {
                    b.owner = Some(mob);
                    b.owner_name = owner_name;
                }
            }
            Some(owner) if owner != mob => {
                let apprentice = world.get::<Wizard>(mob).is_some_and(|w| w.master.is_some());
                let text = if apprentice {
                    "Если бы вас поймают за подглядыванием в книгу заклинаний вашего учителя, то, \
                     скорее всего, вас отчислять из Академии волшебников. Лучше не стоит."
                        .to_owned()
                } else {
                    format!("Вы не признаетесь владельцем {name}, и не собирается открываться!")
                };
                tell(world, client, Span::Warning, text);
            }
            Some(_) => {
                world.write_message(OpenUi::new::<SpellbookUi>(
                    client,
                    mob,
                    item,
                    "Книга заклинаний",
                ));
            }
        }
        return true;
    }
    if let Some(scroll) = world.get::<TeleportScroll>(item).copied() {
        if scroll.uses > 0 {
            crate::spells::open_teleport_prompt(world, client, mob, Some(item));
        }
        return true;
    }
    if let Some(contract) = world.get::<ApprenticeContract>(item).copied() {
        if !contract.used {
            let items: Vec<String> = School::ALL.iter().map(|s| s.key().to_owned()).collect();
            let ask = Ask::new(
                client,
                mob,
                "Контракт",
                "Выберите школу ученика.",
                PromptKind::List {
                    items,
                    default: None,
                },
            );
            let prompt = ask_world(world, ask);
            world
                .entity_mut(prompt)
                .insert(WizardPrompt::Contract { contract: item });
        }
        return true;
    }
    false
}

fn prompt_answers(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in pending {
        let Some(kind) = world.get::<WizardPrompt>(prompt).copied() else {
            continue;
        };
        if world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client) {
            continue;
        }
        match kind {
            WizardPrompt::Rename => {
                let default = world
                    .get::<RenameDefault>(prompt)
                    .map(|d| d.0.clone())
                    .unwrap_or_default();
                // `sanitize_name(reject_bad_text(...))`, else the random name.
                let entered = match answer {
                    Some(PromptAnswer::Text(text)) => {
                        ss13_chat::server::sanitize_say(&text).unwrap_or_default()
                    }
                    _ => String::new(),
                };
                let name: String = entered.trim().chars().take(MAX_NAME_LEN as usize).collect();
                let name = if name.is_empty() { default } else { name };
                if world.get::<Wizard>(actor).is_some() && !name.is_empty() {
                    world.entity_mut(actor).insert(Name::new(name));
                }
            }
            WizardPrompt::Contract { contract } => {
                let Some(PromptAnswer::Item(i)) = answer else {
                    continue;
                };
                let Some(&school) = School::ALL.get(i) else {
                    continue;
                };
                call_apprentice(world, actor, contract, school);
            }
        }
    }
}

/// `poll_for_student` + `spawn_antag` of the contract: the first ghost who wants to be a
/// wizard (midround) becomes the apprentice beside its teacher.
fn call_apprentice(world: &mut World, teacher: Entity, contract: Entity, school: School) {
    let usable = world
        .get::<ApprenticeContract>(contract)
        .is_some_and(|c| !c.used)
        && holds(world, teacher, contract)
        && world.get::<Stat>(teacher) == Some(&Stat::Conscious);
    if !usable {
        return;
    }
    let mut ghosts: Vec<(Entity, Entity)> = world
        .query_filtered::<(Entity, &PlayerMob), With<crate::ghost::Ghost>>()
        .iter(world)
        .map(|(g, p)| (g, p.client))
        .collect();
    ghosts.sort();
    let chosen = ghosts.into_iter().map(|(_, c)| c).find(|&c| {
        world
            .get::<crate::antag::dynamic::AntagPreferences>(c)
            .is_some_and(|p| p.wants(ROLE_WIZARD_MIDROUND))
    });
    let Some(client) = chosen else {
        chat_to(
            world,
            teacher,
            Span::Warning,
            "Unable to reach your apprentice! You can either attack the spellbook with the \
             contract to refund your points, or wait and try again later.",
        );
        return;
    };
    let Some(&pos) = world.get::<TilePos>(teacher) else {
        return;
    };
    if let Some(mut c) = world.get_mut::<ApprenticeContract>(contract) {
        c.used = true;
    }
    let name = random_name(world);
    let mob = crate::server::spawn_mob(world, client, pos, name);
    add_apprentice(world, mob, teacher, school);
}

/// `/datum/action/antag_info/Trigger` of a wizard.
fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if let Some(wizard) = world.get::<Wizard>(source) {
            let title = wizard.antag_name();
            world.write_message(OpenUi::new::<WizardInfoWindow>(
                client, actor, source, title,
            ));
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().collect::<String>() + chars.as_str()
    })
}

// ---------------------------------------------------------------------------------------------
// Round end

/// `/datum/antagonist/wizard/roundend_report` of every wizard and apprentice.
pub fn roundend_report(world: &mut World, end: RoundEnd) -> Vec<ChatMessage> {
    let mut query = world.query::<(Entity, &Wizard)>();
    let mut wizards: Vec<(Entity, Wizard)> =
        query.iter(world).map(|(e, w)| (e, w.clone())).collect();
    if wizards.is_empty() {
        return Vec::new();
    }
    wizards.sort_by_key(|(e, w)| (w.master.is_some(), *e));
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = vec![line(vec![ChatSegment::new(
        Span::Bold,
        format!("{ROUNDEND_CATEGORY} были:"),
    )])];
    for (mob, wizard) in wizards {
        out.push(crate::antag::traitor::print_player(world, mob, &wizard.key));
        let mut won = true;
        for (i, objective) in wizard.objectives.iter().enumerate() {
            let done = objective.check_completion(world, mob, end);
            won &= done;
            out.push(line(vec![
                ChatSegment::new(Span::Bold, format!("{OBJECTIVE_NAME} #{}", i + 1)),
                ChatSegment::new(Span::Plain, format!(": {} ", objective.explanation())),
                if done {
                    ChatSegment::new(Span::GreenText, "Успех!")
                } else {
                    ChatSegment::new(Span::RedText, "Неудача.")
                },
            ]));
        }
        if wizard.master.is_some() {
            continue;
        }
        out.push(line(vec![if won {
            ChatSegment::new(Span::GreenText, "Волшебник был успешен!")
        } else {
            ChatSegment::new(Span::RedText, "Волшебник провалился!")
        }]));
        let name = name_of(world, mob);
        if wizard.purchases.is_empty() {
            out.push(line(vec![ChatSegment::new(
                Span::Bold,
                format!("{name} не покупал никаких заклинаний!"),
            )]));
        } else {
            let bought: Vec<String> = wizard
                .purchases
                .iter()
                .map(|(n, k)| {
                    if *k > 1 {
                        format!("{k}x {n}")
                    } else {
                        (*n).to_owned()
                    }
                })
                .collect();
            out.push(line(vec![ChatSegment::new(
                Span::Bold,
                format!("{name} использовал следующие заклинания:"),
            )]));
            out.push(line(vec![ChatSegment::new(Span::Plain, bought.join(", "))]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_unique_and_cost_what_tg_says() {
        for (i, e) in ENTRIES.iter().enumerate() {
            assert_eq!(entry(e.id).map(|(j, _)| j), Some(i), "{}", e.id);
        }
        let cost = |id: &str| entry(id).unwrap().1.cost;
        assert_eq!(cost("/datum/spellbook_entry/fireball"), 2);
        assert_eq!(cost("/datum/spellbook_entry/blind"), 1);
        assert_eq!(cost("/datum/spellbook_entry/item/contract"), 2);
        let teleport = entry("/datum/spellbook_entry/teleport").unwrap().1;
        assert_eq!(teleport.limit(), Some(3));
        assert!(teleport.refundable());
        let staff = entry("/datum/spellbook_entry/item/staffhealing").unwrap().1;
        assert_eq!((staff.limit(), staff.refundable()), (None, false));
        assert_eq!(staff.buy_word(), "Призвать");
    }

    #[test]
    fn static_data_lists_categories_in_book_order() {
        let st = SpellbookUi::static_data(&Spellbook::default());
        assert_eq!(st.categories, [OFFENSIVE, DEFENSIVE, MOBILITY, ASSISTANCE]);
        assert_eq!(st.entries.len(), ENTRIES.len());
        let smoke = st.entries.iter().find(|e| e.name == "Smoke").unwrap();
        assert!(!smoke.requires_wizard_garb);
        let fireball = st.entries.iter().find(|e| e.name == "Fireball").unwrap();
        // `SPELL_REQUIRES_NO_ANTIMAGIC` only.
        assert!(!fireball.requires_wizard_garb);
        let missile = st
            .entries
            .iter()
            .find(|e| e.name == "Magic Missile")
            .unwrap();
        assert!(missile.requires_wizard_garb);
    }

    #[test]
    fn apprentice_schools_teach_their_spells() {
        assert_eq!(
            School::Destruction.spells(),
            &[Spell::MagicMissile, Spell::Fireball]
        );
        assert_eq!(
            School::ALL.map(School::key),
            ["destruction", "bluespace", "healing", "robeless"]
        );
    }
}
