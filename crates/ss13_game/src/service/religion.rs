//! Religion (server), after tg `code/modules/religion`, `code/modules/jobs/job_types/chaplain`,
//! `code/datums/components/religious_tool.dm`, `subtype_picker.dm` and
//! `/obj/item/choice_beacon/holy`.
//!
//! - The station's religion ([`Religion`], tg `GLOB.religion`, `GLOB.deity`, `GLOB.bible_name`,
//!   `GLOB.religious_sect`, `GLOB.holy_weapon_type`, `GLOB.holy_armor_type`). The first chaplain
//!   is the high priest: they get a bible in their backpack and name the religion, its deity and
//!   its bible; the joke religions of `after_spawn` pick their own books and deities (and brain
//!   damage). A later chaplain is a priest of that religion with its bible, its holy weapon and
//!   the sect's greeting.
//! - Holy roles ([`HolyRole`], tg `mind.holy_role`): deacon < priest < high priest. Bibles,
//!   rites and the armaments beacon need one; a chaplain without the component counts as the
//!   high priest.
//! - The null rod (`/obj/item/nullrod`, the base type, until the station has a holy weapon) asks
//!   which of [`NULLROD_VARIANTS`] to become, sorted by name; the pick is the station's holy
//!   weapon. Antimagic of the rods is `crate::spells::item_antimagic`.
//! - The armaments beacon gives a holy role one of [`HOLY_ARMOR_KITS`] (the station's choice
//!   once made) on its tile and is used up.
//! - The bible: the high priest picks its skin once; on a chapel floor with no altar of the gods
//!   anywhere it unpacks into one in 15 s; it copies itself onto another bible.
//! - The altar of the gods with a bible: without a sect the high priest picks one ([`Sect`]);
//!   with one the favor is shown and a priest invokes a rite ([`Rite`]): the invocations are
//!   said over `ritual_length`, then the effect, then the favor is paid. Other items on the altar
//!   are sacrifices the sect may want (the machine god's charged cells, the pyre god's lit
//!   candles). The sect recolors every altar.
//! - Sects change the bible's blessing: the machine god charges cyborgs and refuses flesh, the
//!   greedy god heals 30 for 50 credits from the priest's account.
//! - The punished ([`super::burden`]), festival ([`super::festival`]), dream
//!   ([`super::dream_sect`]) and sparring ([`super::sparring`]) gods live in their own modules;
//!   this one dispatches to them.
//!
//! The windows are list prompts, not tgui's `ReligiousTool`.
// shortcut: no maintenance sect (BandaStation already hides the honorbound one). No blazing star (no holy
// arrows), no candle lighting (only immortal candles count as lit), no robotic limbs or
// ethereal stomachs for the machine god, no holy faction, no supply pod for the beacon, no
// examine lines, no admin sect reset.

use bevy::prelude::*;
use ss13_chat::server::say_message;
use ss13_mapload::service::{HOLY_ARMOR_KITS, MACHINE_BLESSINGS, NULLROD_VARIANTS};
use ss13_mapload::{Areas, DmAtom, DmClothing, DmItem, DmName, DmObj, DmPower};
use ss13_net::chat::server::SendChat;
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptKind, ask_world};
use ss13_world::SpatialIndex;

use super::festival::SongTune;
use super::server::{
    PendingAsk, client_of, item_path, job_title, say_around, set_icon_state, sound_at,
};
use crate::chem::name_of;
use crate::construction::{Job, path_of};
use crate::health::{Health, Stat};
use crate::inventory::{CarriedBy, Inventory};
use crate::obj_damage::is_under;
use crate::tools::{Actor, DoAfter, do_after, tell};
use crate::zone::BodyZone;

const CHAPLAIN: &str = "Chaplain";
const BIBLE: &str = "/obj/item/book/bible";
const SYNDICATE_BIBLE: &str = "/obj/item/book/bible/syndicate";
/// The chaplain's bible (`/obj/item/book/bible/booze`).
const CHAPLAIN_BIBLE: &str = "/obj/item/book/bible/booze";
const NULLROD: &str = "/obj/item/nullrod";
const ALTAR_OF_GODS: &str = "/obj/structure/altar/of_gods";
const HOLY_BEACON: &str = "/obj/item/choice_beacon/holy";
const CANDLE: &str = "/obj/item/flashlight/flare/candle";
const INFINITE_CANDLE: &str = "/obj/item/flashlight/flare/candle/infinite";
const GREED_VENDOR: &str = "/obj/machinery/vending/custom/greed";
const CELL: &str = "/obj/item/stock_parts/power_store";
const CHAPEL: &str = "/area/station/service/chapel";
const FLOOR: &str = "/turf/open/floor";
const DEFAULT_RELIGION: &str = "Christianity";
const DEFAULT_DEITY: &str = "Space Jesus";
/// tg `MAX_NAME_LEN`.
const MAX_NAME_LEN: usize = 42;
/// `STANDARD_CELL_CHARGE`, joules.
const STANDARD_CELL_CHARGE: f64 = 10_000.0;
/// `FIRE_IMMUNITY_MAX_TEMP_PROTECT`, kelvin.
const FIRE_IMMUNITY_MAX_TEMP_PROTECT: f32 = 35_000.0;
/// `GREEDY_HEAL_COST`, credits.
const GREEDY_HEAL_COST: i64 = 50;
/// `make_new_altar`: 15 s.
const UNPACK_ALTAR_DS: f32 = 150.0;
const PRAY: &str = "sound/effects/pray.ogg";
const PRAY_CHAPLAIN: &str = "sound/effects/pray_chaplain.ogg";
const FIREBALL: &str = "sound/effects/magic/fireball.ogg";
const SUPERMATTER: &str = "sound/effects/supermatter.ogg";
const CASH_REGISTER: &str = "sound/effects/cashregister.ogg";
const BUZZ: &str = "sound/machines/buzz/buzz-sigh.ogg";
const BANG: &str = "sound/effects/bang.ogg";
/// `DEFAULT_SMACK_CHANCE`, percent.
const DEFAULT_SMACK_CHANCE: f32 = 60.0;

/// `GLOB.biblenames` with `GLOB.biblestates` and `GLOB.bibleitemstates`.
pub const BIBLE_SKINS: [(&str, &str, &str); 19] = [
    ("Bible", "bible", "bible"),
    ("Quran", "koran", "koran"),
    ("Scrapbook", "scrapbook", "scrapbook"),
    ("Burning Bible", "burning", "burning"),
    ("Clown Bible", "honk1", "honk1"),
    ("Banana Bible", "honk2", "honk2"),
    ("Creeper Bible", "creeper", "creeper"),
    ("White Bible", "white", "white"),
    ("Holy Light", "holylight", "holylight"),
    ("The God Delusion", "atheist", "atheist"),
    ("Tome", "tome", "tome"),
    ("The King in Yellow", "kingyellow", "kingyellow"),
    ("Ithaqua", "ithaqua", "ithaqua"),
    ("Scientology", "scientology", "scientology"),
    ("Melted Bible", "melted", "melted"),
    ("Necronomicon", "necronomicon", "necronomicon"),
    ("Insulationism", "insuls", "kingyellow"),
    ("Guru Granth Sahib", "gurugranthsahib", "gurugranthsahib"),
    ("Kojiki", "kojiki", "kojiki"),
];

/// tg `HOLY_ROLE_DEACON`, `_PRIEST`, `_HIGHPRIEST` (ordered).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HolyRole {
    Deacon,
    Priest,
    HighPriest,
}

/// The `deity_name` of a bible that has one other than the station's.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct BibleDeity(pub String);

/// The religious sects offered at the altar (`/datum/religion_sect` subtypes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sect {
    Puritanism,
    Mechanical,
    Pyre,
    Greed,
    Burden,
    Spar,
    Music,
    Dreams,
}

pub struct SectDef {
    pub name: &'static str,
    pub quote: &'static str,
    pub desc: &'static str,
    pub max_favor: f32,
    pub altar_icon_state: Option<&'static str>,
    /// `smack_chance`, percent.
    pub smack_chance: f32,
    /// `rites_list` without the deaconize every sect gets (unless it has its own).
    pub rites: &'static [Rite],
}

impl Sect {
    pub const ALL: [Sect; 8] = [
        Sect::Puritanism,
        Sect::Mechanical,
        Sect::Pyre,
        Sect::Greed,
        Sect::Burden,
        Sect::Spar,
        Sect::Music,
        Sect::Dreams,
    ];

    pub fn def(self) -> &'static SectDef {
        match self {
            Sect::Puritanism => &SectDef {
                name: "Nanotrasen Approved God",
                quote: "Nanotrasen Recommends!",
                desc: "Your run-of-the-mill sect, there are no benefits or boons associated.",
                max_favor: 1000.0,
                altar_icon_state: None,
                smack_chance: DEFAULT_SMACK_CHANCE,
                rites: &[],
            },
            Sect::Mechanical => &SectDef {
                name: "Mechanical God",
                quote: "May you find peace in a metal shell.",
                desc: "Bibles now recharge cyborgs and heal robotic limbs if targeted, but they \
                       do not heal organic limbs. You can now sacrifice cells, with favor \
                       depending on their charge.",
                max_favor: 2500.0,
                altar_icon_state: Some("convertaltar-blue"),
                smack_chance: DEFAULT_SMACK_CHANCE,
                rites: &[Rite::SynthConversion, Rite::MachineBlessing],
            },
            Sect::Pyre => &SectDef {
                name: "Pyre God",
                quote: "It must burn! The primal energy must be respected.",
                desc: "Sacrificing burning corpses with a lot of burn damage and candles grants \
                       you favor.",
                max_favor: 10000.0,
                altar_icon_state: Some("convertaltar-red"),
                smack_chance: DEFAULT_SMACK_CHANCE,
                rites: &[
                    Rite::Fireproof,
                    Rite::BurningSacrifice,
                    Rite::InfiniteCandle,
                ],
            },
            Sect::Greed => &SectDef {
                name: "Greedy God",
                quote: "Greed is good.",
                desc: "In the eyes of your mercantile deity, your wealth is your favor. Earn \
                       enough wealth to purchase some more business opportunities.",
                max_favor: 1000.0,
                altar_icon_state: Some("convertaltar-yellow"),
                smack_chance: DEFAULT_SMACK_CHANCE,
                rites: &[Rite::CustomVending],
            },
            Sect::Burden => &SectDef {
                name: "Punished God",
                quote: "To feel the freedom, you must first understand captivity.",
                desc: "Incapacitate yourself in any way possible. Bad mutations, lost limbs, \
                       traumas, even addictions. You will learn the secrets of the universe from \
                       your defeated shell.",
                max_favor: 1000.0,
                altar_icon_state: Some("convertaltar-burden"),
                smack_chance: 0.0,
                rites: &[Rite::NullrodTransformation],
            },
            // shortcut: no "Become Tenacious" (no movespeed modifiers for crawling yet).
            Sect::Spar => &SectDef {
                name: "Sparring God",
                quote: "Your next swing must be faster, neophyte. Steel your heart.",
                desc: "Spar other crewmembers to gain favor or other rewards. Exchange favor to \
                       steel yourself against real battles.",
                max_favor: 1000.0,
                altar_icon_state: Some("convertaltar-orange"),
                smack_chance: DEFAULT_SMACK_CHANCE,
                rites: &[
                    Rite::SparringContract,
                    Rite::CeremonialWeapon,
                    Rite::DeclareArena,
                    Rite::Unbreakable,
                ],
            },
            Sect::Music => &SectDef {
                name: "Festival God",
                quote: "Everything follows a rhythm: the heartbeat of the universe!",
                desc: "Make wonderful music! Sooth or serrate your friends and foes with the beat.",
                max_favor: 1000.0,
                altar_icon_state: Some("convertaltar-festival"),
                smack_chance: DEFAULT_SMACK_CHANCE,
                rites: &[
                    Rite::HolyViolin,
                    Rite::PortableSongTuning,
                    Rite::Tune(SongTune::Evangelism),
                    Rite::Tune(SongTune::Light),
                    Rite::Tune(SongTune::Nullwave),
                    Rite::Tune(SongTune::Pain),
                    Rite::Tune(SongTune::Lullaby),
                ],
            },
            // shortcut: no "Banish Nightmare" (no nightmares) and no "Dream Projection" (no
            // projection eye mob).
            Sect::Dreams => &SectDef {
                name: "Dream God",
                quote: "The dream is a window into the soul.",
                desc: "Dream deeply to gain insights into the universe. Earn favor by dreaming or \
                       blessing dreaming creatures. Your blessings invoke dreams and promote \
                       healing in those who are sound asleep.",
                max_favor: 1000.0,
                altar_icon_state: Some("convertaltar-dream"),
                smack_chance: 20.0,
                rites: &[
                    Rite::DeaconizeDreamers,
                    Rite::DreamPortent,
                    Rite::DreamProtection,
                    Rite::SlumberParty,
                ],
            },
        }
    }
}

/// The rites of the offered sects (`/datum/religion_rites` subtypes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rite {
    Deaconize,
    SynthConversion,
    MachineBlessing,
    Fireproof,
    BurningSacrifice,
    InfiniteCandle,
    CustomVending,
    NullrodTransformation,
    SparringContract,
    CeremonialWeapon,
    DeclareArena,
    Unbreakable,
    HolyViolin,
    PortableSongTuning,
    Tune(SongTune),
    DeaconizeDreamers,
    DreamPortent,
    DreamProtection,
    SlumberParty,
}

/// `/datum/religion_rites` defaults: 10 s, no invocations, free, reusable.
pub(super) const BASE_RITE: RiteDef = RiteDef {
    name: "religious rite",
    desc: "",
    length: 10.0,
    invocations: &[],
    invoke_msg: None,
    favor_cost: 0.0,
    money_cost: 0,
    one_time: false,
};

/// `/datum/religion_rites/deaconize`.
const DEACONIZE: RiteDef = RiteDef {
    name: "Посвящение в диакона",
    desc: "Обращает кого-то в вашу религию. Необходимо добровольное согласие, так \
           как при первом использовании претенденту сначала предложат вступить. Он \
           обретёт те же святые силы, что и вы. Вы можете посвятить только одного \
           человека, так что убедитесь, что он достоин!",
    length: 30.0,
    invocations: &[
        "Хороший, благородный человек был приведен сюда верой ...",
        "С руками, готовыми служить ...",
        "С сердцем, готовым слушать ...",
        "И душой, готовой следовать ...",
        "Да предложим мы свою руку взамен ...",
    ],
    invoke_msg: Some("И использовать их наилучшим образом."),
    favor_cost: 0.0,
    money_cost: 0,
    one_time: true,
};

/// A `&'static RiteDef` from a struct literal that may use `..BASE_RITE`.
macro_rules! rite_def {
    ($($field:tt)*) => {{
        const DEF: RiteDef = RiteDef { $($field)* };
        &DEF
    }};
}
pub(super) use rite_def;

#[derive(Clone, Copy)]
pub struct RiteDef {
    pub name: &'static str,
    pub desc: &'static str,
    /// `ritual_length`, seconds.
    pub length: f32,
    /// `{deity}` stands for `GLOB.deity` (the dream god's invocations name it).
    pub invocations: &'static [&'static str],
    pub invoke_msg: Option<&'static str>,
    pub favor_cost: f32,
    /// The greedy god's rites cost credits instead (`money_cost`).
    pub money_cost: i64,
    /// `RITE_ONE_TIME_USE`.
    pub one_time: bool,
}

impl Rite {
    pub fn def(self) -> &'static RiteDef {
        match self {
            Rite::Deaconize => &DEACONIZE,
            Rite::SynthConversion => &RiteDef {
                name: "Synthetic Conversion",
                desc: "Convert a humanoid individual into a (superior) android. Buckle a human \
                       to convert them, otherwise this rite will convert you.",
                length: 30.0,
                invocations: &[
                    "Внутренними механизмами нашего бога ...",
                    "... Мы взываем к тебе, пред лицом невзгод ...",
                    "... заверши нас, удалив то, что нежеланно ...",
                ],
                invoke_msg: Some(
                    "... Восстань, наш чемпион! Стань тем, чего жаждет твоя душа, обрети жизнь в \
                     этом мире в своём истинном облике!!!",
                ),
                favor_cost: 1000.0,
                money_cost: 0,
                one_time: false,
            },
            Rite::MachineBlessing => &RiteDef {
                name: "Receive Blessing",
                desc: "Receive a blessing from the machine god to further your ascension.",
                length: 5.0,
                invocations: &[
                    "Пусть твоя воля питает наши кузницы.",
                    "... Окажи нам помощь в нашем великом завоевании!",
                ],
                invoke_msg: Some("Конец плоти близок!"),
                favor_cost: 2000.0,
                money_cost: 0,
                one_time: false,
            },
            Rite::Fireproof => &RiteDef {
                name: "Unmelting Protection",
                desc: "Grants fire immunity to any piece of clothing.",
                length: 12.0,
                invocations: &[
                    "Дабы поддержать держателя Вечно-Горящей свечи ...",
                    "... позволь этому непригодному одеянию служить тебе ...",
                    "... сделай его достаточно крепким, чтобы гореть тысячу раз и больше ...",
                ],
                invoke_msg: Some(
                    "... Выйди на свет в своей новой форме и присоединись к нерастворимому воску \
                     едино-истинного пламени!",
                ),
                favor_cost: 700.0,
                money_cost: 0,
                one_time: false,
            },
            Rite::BurningSacrifice => &RiteDef {
                name: "Burning Offering",
                desc: "Sacrifice a buckled burning or husked corpse for favor. The more burn \
                       damage the corpse has the more favor you will receive.",
                length: 15.0,
                invocations: &[
                    "Горящее тело ...",
                    "... очищенное пламенем ...",
                    "... все мы сделаны из огня ...",
                    "... и к нему мы ...",
                ],
                invoke_msg: Some("... ВОЗВРАЩАЕМСЯ! "),
                favor_cost: 0.0,
                money_cost: 0,
                one_time: false,
            },
            Rite::InfiniteCandle => &RiteDef {
                name: "Immortal Candles",
                desc: "Create five unextinguishable candles that never run out of wax.",
                length: 10.0,
                invocations: &[],
                invoke_msg: Some(
                    "Горите ярко, маленькие свечки, ибо вы погаснете вместе со Вселенной.",
                ),
                favor_cost: 200.0,
                money_cost: 0,
                one_time: false,
            },
            Rite::CustomVending => &RiteDef {
                name: "Purchase Personal Vending Machine",
                desc: "Summons a custom vending machine. You can use it to sell MANY items!",
                length: 5.0,
                invocations: &[],
                invoke_msg: Some(
                    "Если я куплю торговый автомат для своих товаров, то я смогу стать БОГАТЫМ!",
                ),
                favor_cost: 0.0,
                money_cost: 1000,
                one_time: false,
            },
            Rite::NullrodTransformation => rite_def! {
                name: "Transmogrify",
                desc: "Your full power needs a firearm to be realized. You may transform your null \
                       rod into one.",
                ..BASE_RITE
            },
            Rite::SparringContract => rite_def! {
                name: "Summon Sparring Contract",
                desc: "Turns some paper into a sparring contract.",
                invoke_msg: Some("Я буду тренироваться во имя моего бога."),
                ..BASE_RITE
            },
            Rite::CeremonialWeapon => rite_def! {
                name: "Forge Ceremonial Gear",
                desc: "Turn some material into ceremonial gear. Ceremonial blades are weak outside \
                       of sparring and quite heavy to lug around.",
                invoke_msg: Some("Оружие во имя твоё! Бои с кровью твоей!"),
                ..BASE_RITE
            },
            Rite::DeclareArena => rite_def! {
                name: "Declare Arena",
                desc: "Declare a new area as fit for sparring. You'll be able to select it in \
                       contracts.",
                length: 6.0,
                invocations: &["Я стремлюсь к новым горизонтам ..."],
                invoke_msg: Some("... и пусть мой подъем будет крут."),
                favor_cost: 1.0,
                ..BASE_RITE
            },
            Rite::Unbreakable => rite_def! {
                name: "Become Unbreakable",
                desc: "Your training has made you unbreakable. In times of crisis, you will \
                       attempt to keep fighting on.",
                invoke_msg: Some("Моя воля должна быть непоколебимой. Даруй мне это благословение!"),
                favor_cost: 4.0,
                ..BASE_RITE
            },
            Rite::HolyViolin => rite_def! {
                name: "Cogitandi Fidis",
                desc: "Creates a holy violin that can analyze songs played from it.",
                length: 6.0,
                invocations: &["Нужен слуга торжества ..."],
                invoke_msg: Some("... Великий разум для музыкальных дел!"),
                favor_cost: 20.0,
                ..BASE_RITE
            },
            Rite::PortableSongTuning => rite_def! {
                name: "Portable Song Tuning",
                desc: "Empowers a provided instrument to work as a portable altar for tuning \
                       songs. It will need to be recharged after five rites.",
                length: 6.0,
                invocations: &["Позвольте мне даровать вам святые вдохновения ..."],
                invoke_msg: Some(
                    "... И пусть их несут ветра, что играют в унисон с моей мелодией!",
                ),
                favor_cost: 10.0,
                ..BASE_RITE
            },
            Rite::Tune(tune) => tune.rite(),
            Rite::DeaconizeDreamers => rite_def! {
                desc: "Converts someone to your sect. They must be willing, so the first \
                       invocation will only prompt them to join. They will gain the same holy \
                       abilities as you. You can deaconize up to three followers, so choose \
                       wisely!",
                one_time: false,
                ..DEACONIZE
            },
            Rite::DreamPortent => rite_def! {
                name: "Dream Portent",
                desc: "Immediately fall into a slumber and receive a portent of the future. The \
                       vision may be difficult to interpret, but will likely come true in some \
                       form. Any form of harm will awaken you and disrupt the vision.",
                length: 6.0,
                invocations: &[
                    "O great shepherd {deity}, grant me a vision of the future!..",
                    "That our flock may persevere through the trials to come...",
                ],
                favor_cost: 50.0,
                ..BASE_RITE
            },
            Rite::DreamProtection => rite_def! {
                name: "Dream Protection",
                desc: "Bless you and all of your followers with protection in their slumber, \
                       granting resistance to damage while asleep, which is further increased \
                       while dreaming.",
                length: 15.0,
                invocations: &[
                    "Protect our flock from harm, great shepherd {deity}!..",
                    "Grant us peaceful slumber, free from nightmares and those who would do us \
                     harm!..",
                    "Our sleepers shall be safe to dream to their heart's desire!..",
                ],
                favor_cost: 200.0,
                one_time: true,
                ..BASE_RITE
            },
            Rite::SlumberParty => rite_def! {
                name: "Slumber Party",
                desc: "Put all nearby creatures to sleep. All affected creatures share the same \
                       dream and heal rapidly while sleeping. You and your followers heal even \
                       faster during the ritual.",
                length: 20.0,
                invocations: &[
                    "Sleep now, flock of {deity}, and share in each other's dreams...",
                    "Our slumber shall rejuvenate us for the trials ahead...",
                    "May we all wake up refreshed and renewed!",
                ],
                favor_cost: 200.0,
                ..BASE_RITE
            },
        }
    }

    /// The do_after steps: one per invocation, or the whole length without any.
    fn steps(self) -> usize {
        self.def().invocations.len().max(1)
    }
}

/// The station's religion (tg's religion globals).
#[derive(Resource, Clone, Debug, Default)]
pub struct Religion {
    /// `GLOB.religion`; `None` until the first chaplain arrives.
    pub name: Option<String>,
    /// `GLOB.deity`.
    pub deity: String,
    /// `GLOB.bible_name`.
    pub bible_name: String,
    /// Index into [`BIBLE_SKINS`] (`GLOB.bible_icon_state`).
    pub bible_skin: Option<usize>,
    pub sect: Option<Sect>,
    pub favor: f32,
    /// One-time rites already performed (`rites_list.Remove`).
    pub used_rites: Vec<Rite>,
    /// `GLOB.holy_weapon_type`.
    pub holy_weapon: Option<String>,
    /// `GLOB.holy_armor_type`.
    pub holy_armor: Option<String>,
    /// The deaconize rite's `potential_deacon`: who accepted the invitation.
    pub deacon_candidate: Option<Entity>,
    /// The sparring god's contract, arenas and record.
    pub spar: super::sparring::SparState,
    /// The dream god's protection, deacons and blessings.
    pub dreams: super::dream_sect::DreamState,
}

impl Religion {
    /// `adjust_favor`: clamped to `[0, max_favor]`; the change actually made.
    // tg writes `clamp(0, max_favor, favor + amount)`, the intent is the clamp of the sum.
    pub fn adjust_favor(&mut self, amount: f32) -> f32 {
        let max = self.sect.map_or(1000.0, |s| s.def().max_favor);
        let before = self.favor;
        self.favor = (self.favor + amount).clamp(0.0, max);
        self.favor - before
    }

    /// The rites the altar offers now: the sect's and deaconize (unless the sect has its own),
    /// minus used one-time rites.
    pub fn rites(&self) -> Vec<Rite> {
        let Some(sect) = self.sect else {
            return Vec::new();
        };
        let own = sect.def().rites;
        let deaconize = (!own.contains(&Rite::DeaconizeDreamers)).then_some(Rite::Deaconize);
        own.iter()
            .copied()
            .chain(deaconize)
            .filter(|r| !self.used_rites.contains(r))
            .collect()
    }
}

/// A rite being performed by this mob at `altar` (tg `performing_rite`).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct PerformingRite {
    pub rite: Rite,
    pub altar: Entity,
    /// Steps done.
    pub step: usize,
    /// The clothing, corpse, convert, offering or area the rite chose when it began.
    pub target: Option<Entity>,
}

/// A religion do_after ([`crate::construction::Job::Religion`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReligionJob {
    /// One step of the [`PerformingRite`].
    RiteStep,
    /// `make_new_altar`.
    UnpackAltar,
    /// The chaplain's revolver's `pray_refill`.
    PrayRefill,
}

/// The religion questions waiting for answers ([`PendingAsk::Religion`]).
#[derive(Clone, Debug)]
pub enum ReligionAsk {
    /// The high priest's religion, then deity, then bible name (tg's preferences).
    ReligionName {
        bible: Entity,
    },
    DeityName {
        bible: Entity,
        religion: String,
    },
    BibleName {
        bible: Entity,
        religion: String,
        deity: String,
    },
    Nullrod {
        rod: Entity,
        variants: Vec<&'static str>,
    },
    Armor {
        beacon: Entity,
        kits: Vec<&'static str>,
    },
    BibleSkin {
        bible: Entity,
    },
    Sect {
        altar: Entity,
    },
    Rite {
        altar: Entity,
        rites: Vec<Rite>,
    },
    /// The deaconize invitation, to the buckled candidate.
    Deacon,
    /// "Declare Arena": the area to declare, before the rite begins.
    Arena {
        altar: Entity,
        areas: Vec<Entity>,
    },
    /// The sparring contract's menus.
    Spar(super::sparring::SparAsk),
}

// ---------------------------------------------------------------------------------------------
// Holy roles and the religion

/// `mind.set_holy_role(NONE)`: excommunicated (the sparring god's third lost holy match).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Excommunicated;

/// `mind.holy_role`; a chaplain without one is the high priest.
pub fn holy_role(world: &World, mob: Entity) -> Option<HolyRole> {
    if world.get::<Excommunicated>(mob).is_some() {
        return None;
    }
    world.get::<HolyRole>(mob).copied().or_else(|| {
        (job_title(world, mob).as_deref() == Some(CHAPLAIN)).then_some(HolyRole::HighPriest)
    })
}

pub(super) fn religion(world: &World) -> Religion {
    world
        .get_resource::<Religion>()
        .cloned()
        .unwrap_or_default()
}

pub(super) fn religion_mut(world: &mut World) -> Mut<'_, Religion> {
    if !world.contains_resource::<Religion>() {
        world.init_resource::<Religion>();
    }
    world.resource_mut::<Religion>()
}

/// The deity a bible speaks for: its own, the station's, or tg's default `deity_name`.
pub fn deity(world: &World, bible: Option<Entity>) -> String {
    if let Some(d) = bible.and_then(|b| world.get::<BibleDeity>(b)) {
        return d.0.clone();
    }
    let station = religion(world).deity;
    if station.is_empty() {
        DEFAULT_DEITY.to_owned()
    } else {
        station
    }
}

/// The station's deity for sect messages (`GLOB.deity`).
pub(super) fn glob_deity(world: &World) -> String {
    deity(world, None)
}

/// The antimagic the sects give a mob: the psyker brain's (`MAGIC_RESISTANCE_MIND`), the nullwave
/// song's (`TRAIT_ANTIMAGIC`) and a protected sleeper's (`TRAIT_HOLY`).
pub fn antimagic_flags(world: &World, mob: Entity) -> u32 {
    let mut flags = 0;
    if world.get::<super::burden::Psyker>(mob).is_some() {
        flags |= crate::spells::MAGIC_RESISTANCE_MIND;
    }
    if super::festival::song_antimagic(world, mob) {
        flags |= crate::spells::MAGIC_RESISTANCE;
    }
    if super::dream_sect::protected_now(world, mob) {
        flags |= crate::spells::MAGIC_RESISTANCE_HOLY;
    }
    flags
}

/// The sect's `smack_chance` (`DEFAULT_SMACK_CHANCE` without one).
pub fn smack_chance(world: &World) -> f32 {
    religion(world)
        .sect
        .map_or(DEFAULT_SMACK_CHANCE, |s| s.def().smack_chance)
}

pub(super) fn rename(world: &mut World, e: Entity, name: &str) {
    if let Some(mut dm) = world.get_mut::<DmName>(e) {
        name.clone_into(&mut dm.name);
    }
    if world.get::<Name>(e).is_some() || world.get::<DmName>(e).is_none() {
        world.entity_mut(e).insert(Name::new(name.to_owned()));
    }
}

pub(super) fn say(world: &mut World, mob: Entity, text: &str) {
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

pub(super) fn tell_mob(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = client_of(world, mob) {
        tell(world, client, span, text);
    }
}

pub(super) fn holding(world: &World, mob: Entity, item: Entity) -> bool {
    world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.hands.contains(&Some(item)))
}

fn holds_bible(world: &World, mob: Entity) -> bool {
    world.get::<Inventory>(mob).is_some_and(|i| {
        i.hands
            .iter()
            .flatten()
            .any(|&h| is_under(&item_path(world, h), BIBLE))
    })
}

pub(super) fn is_human(world: &World, mob: Entity) -> bool {
    world.get::<crate::human::HumanLook>(mob).is_some()
}

pub(super) fn has_prototype(world: &World, path: &str) -> bool {
    world
        .get_resource::<ss13_proto::Prototypes>()
        .is_some_and(|p| p.get(path).is_some())
}

fn clean_name(text: &str) -> String {
    text.trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_NAME_LEN)
        .collect::<String>()
        .trim()
        .to_owned()
}

// ---------------------------------------------------------------------------------------------
// The chaplain

/// `/datum/job/chaplain/after_spawn` for a chaplain who just spawned as a human.
pub fn chaplain_spawned(world: &mut World, client: Entity, mob: Entity) {
    if job_title(world, mob).as_deref() != Some(CHAPLAIN) || !is_human(world, mob) {
        return;
    }
    let bible = spawn_bible(world, mob);
    let station = religion(world);
    if station.name.is_some() {
        world.entity_mut(mob).insert(HolyRole::Priest);
        if let Some(bible) = bible {
            rename(world, bible, &station.bible_name);
            world
                .entity_mut(bible)
                .insert(BibleDeity(station.deity.clone()));
            if let Some(skin) = station.bible_skin {
                set_bible_skin(world, bible, skin);
            }
        }
        tell(
            world,
            client,
            Span::BoldNotice,
            format!(
                "There is already an established religion onboard the station. You are an \
                 acolyte of {}. Defer to the Chaplain.",
                station.deity
            ),
        );
        let weapon = station.holy_weapon.as_deref().unwrap_or(NULLROD);
        if let Some(rod) = crate::construction::spawn_detached(world, weapon)
            && let Some(inventory) = world.get::<Inventory>(mob).cloned()
        {
            world.entity_mut(rod).insert(Networked);
            crate::storage::put_in_hands(world, mob, &inventory, rod);
        }
        if let Some(sect) = station.sect {
            on_conversion(world, mob, sect);
        }
        return;
    }
    world.entity_mut(mob).insert(HolyRole::HighPriest);
    // The religion exists from now on, with tg's defaults until the answers come.
    let Some(bible) = bible else {
        establish(world, mob, None, DEFAULT_RELIGION, DEFAULT_DEITY, "");
        return;
    };
    establish_names(world, bible, DEFAULT_RELIGION, DEFAULT_DEITY, "");
    ask_text(
        world,
        client,
        mob,
        "Религия",
        "Название вашей религии",
        DEFAULT_RELIGION,
        ReligionAsk::ReligionName { bible },
    );
}

/// The chaplain's bible into their backpack (`equip_to_storage(ITEM_SLOT_BACK)`), else hands.
fn spawn_bible(world: &mut World, mob: Entity) -> Option<Entity> {
    let bible = crate::construction::spawn_detached(world, CHAPLAIN_BIBLE)?;
    world.entity_mut(bible).insert(Networked);
    let inventory = world.get::<Inventory>(mob).cloned()?;
    match inventory.back {
        Some(pack) if world.get::<crate::storage::Storage>(pack).is_some() => {
            crate::storage::put_inside(world, bible, pack);
        }
        _ => crate::storage::put_in_hands(world, mob, &inventory, bible),
    }
    Some(bible)
}

fn ask_text(
    world: &mut World,
    client: Entity,
    mob: Entity,
    title: &str,
    message: &str,
    default: &str,
    ask: ReligionAsk,
) {
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            title,
            message,
            PromptKind::Text {
                default: default.to_owned(),
                max_length: MAX_NAME_LEN as u32,
                multiline: false,
            },
        ),
    );
    world.entity_mut(prompt).insert(PendingAsk::Religion(ask));
}

pub(super) fn ask_list(
    world: &mut World,
    client: Entity,
    mob: Entity,
    title: &str,
    message: &str,
    items: Vec<String>,
    ask: ReligionAsk,
) {
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            title,
            message,
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(PendingAsk::Religion(ask));
}

/// The librarian console's print screen: deity, religion and bible name, tg's defaults until
/// the chaplain chose.
pub(super) fn print_names(station: &Religion) -> (String, String, String) {
    let deity = if station.deity.is_empty() {
        DEFAULT_DEITY.to_owned()
    } else {
        station.deity.clone()
    };
    let religion = station
        .name
        .clone()
        .unwrap_or_else(|| DEFAULT_RELIGION.to_owned());
    let bible = default_bible(&religion, &station.bible_name);
    (deity, religion, bible)
}

/// tg's bible name for a religion and the chosen names: `DEFAULT_BIBLE_REPLACE` when none.
fn default_bible(religion: &str, bible: &str) -> String {
    if bible.is_empty() {
        format!("The Holy Book of {religion}")
    } else {
        bible.to_owned()
    }
}

/// The religion, deity and bible as chosen, without the joke religions' picks.
fn establish_names(world: &mut World, bible: Entity, religion: &str, deity: &str, book: &str) {
    let book = default_bible(religion, book);
    rename(world, bible, &book);
    world.entity_mut(bible).insert(BibleDeity(deity.to_owned()));
    let mut station = religion_mut(world);
    station.name = Some(religion.to_owned());
    deity.clone_into(&mut station.deity);
    station.bible_name = book;
}

/// The joke religions of `after_spawn`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Joke {
    Gay,
    Meme,
    Weeaboo,
}

impl Joke {
    fn of(religion: &str) -> Option<Joke> {
        match religion.to_lowercase().as_str() {
            "homosexuality" | "gay" | "penis" | "ass" | "cock" | "cocks" => Some(Joke::Gay),
            "lol" | "wtf" | "poo" | "badmin" | "shitmin" | "deadmin" | "meme" | "memes"
            | "skibidi" => Some(Joke::Meme),
            "weeaboo" | "kawaii" => Some(Joke::Weeaboo),
            _ => None,
        }
    }

    fn books(self) -> &'static [&'static str] {
        match self {
            Joke::Gay => &["Guys Gone Wild", "Coming Out of The Closet", "War of Cocks"],
            Joke::Meme => &[
                "Woody's Got Wood: The Aftermath",
                "Sweet Bro and Hella Jeff: Expanded Edition",
                "F.A.T.A.L. Rulebook",
                "Toilet Humor",
            ],
            Joke::Weeaboo => &[
                "Fanfiction Compendium",
                "Japanese for Dummies",
                "The Manganomicon",
                "Establishing Your O.T.P",
            ],
        }
    }

    fn deities(self, book: &str) -> &'static [&'static str] {
        match (self, book) {
            (Joke::Gay, "War of Cocks") => &["Dick Powers", "King Cock"],
            (Joke::Gay, _) => &["Gay Space Jesus", "Gandalf", "Dumbledore"],
            (Joke::Meme, "Woody's Got Wood: The Aftermath") => {
                &["Woody", "Andy", "Cherry Flavored Lube"]
            }
            (Joke::Meme, "Sweet Bro and Hella Jeff: Expanded Edition") => {
                &["Sweet Bro", "Hella Jeff", "Stairs", "AH"]
            }
            (Joke::Meme, "F.A.T.A.L. Rulebook") => &["Twenty Ten-Sided Dice"],
            (Joke::Meme, _) => &[
                "Skibidi Toilet",
                "Skibidi Wizard",
                "Skibidi Bathtub",
                "John Skibidi",
                "Skibidi Skibidi",
                "G-Toilet 1.0",
                "John Freeman",
            ],
            (Joke::Weeaboo, _) => &["Anime"],
        }
    }

    /// "starts off brain damaged as fuck".
    fn brain_damage(self) -> bool {
        self != Joke::Weeaboo
    }
}

pub(super) fn pick<'a>(world: &mut World, from: &[&'a str]) -> &'a str {
    let i = world
        .resource_mut::<crate::combat::CombatRng>()
        .range(0, from.len() as u32 - 1);
    from[i as usize]
}

/// The end of `after_spawn` for the high priest: the joke religions, then the globals.
fn establish(
    world: &mut World,
    mob: Entity,
    bible: Option<Entity>,
    religion: &str,
    deity: &str,
    book: &str,
) {
    let bible = bible.filter(|&b| world.get_entity(b).is_ok());
    let (mut deity, mut book) = (deity.to_owned(), book.to_owned());
    if let Some(joke) = Joke::of(religion) {
        book = pick(world, joke.books()).to_owned();
        deity = pick(world, joke.deities(&book)).to_owned();
        if joke.brain_damage() {
            crate::medical::organs::adjust_organ_loss(
                world,
                mob,
                crate::medical::organs::Slot::Brain,
                100.0,
            );
        }
    }
    let Some(bible) = bible else {
        let mut station = religion_mut(world);
        station.name = Some(religion.to_owned());
        station.deity = deity;
        station.bible_name = default_bible(religion, &book);
        return;
    };
    establish_names(world, bible, religion, &deity, &book);
    if matches!(
        religion.to_lowercase().as_str(),
        "servicianism" | "partying"
    ) && let Some(mut dm) = world.get_mut::<DmName>(bible)
    {
        dm.desc = "Happy, Full, Clean. Live it and give it.".to_owned();
    }
}

/// `on_conversion`: the sect's greeting, then what the sect gives its converts.
fn on_conversion(world: &mut World, mob: Entity, sect: Sect) {
    let def = sect.def();
    tell_mob(world, mob, Span::BoldNotice, format!("\"{}\"", def.quote));
    tell_mob(world, mob, Span::Notice, def.desc);
    match sect {
        Sect::Burden => super::burden::on_conversion(world, mob),
        Sect::Music => super::festival::on_conversion(world, mob),
        Sect::Dreams => super::dream_sect::on_conversion(world, mob),
        _ => {}
    }
}

// ---------------------------------------------------------------------------------------------
// Items: the null rod, the armaments beacon, the bible's skin

/// `attack_self` of the null rod's picker, the armaments beacon and the bible's skin menu.
pub(super) fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let path = item_path(world, item);
    if path == NULLROD && religion(world).holy_weapon.is_none() {
        let mut variants: Vec<(String, &'static str)> = NULLROD_VARIANTS
            .into_iter()
            .filter(|p| has_prototype(world, p))
            .map(|p| (crate::machine_frame::type_name(world, p), p))
            .collect();
        if variants.is_empty() {
            return false;
        }
        variants.sort_by(|a, b| a.0.cmp(&b.0));
        let names = variants.iter().map(|(n, _)| n.clone()).collect();
        ask_list(
            world,
            client,
            mob,
            "Holy weapon",
            "Choose your holy weapon",
            names,
            ReligionAsk::Nullrod {
                rod: item,
                variants: variants.into_iter().map(|(_, p)| p).collect(),
            },
        );
        return true;
    }
    if is_under(&path, HOLY_BEACON) {
        beacon(world, client, mob, item);
        return true;
    }
    if is_under(&path, BIBLE)
        && !is_under(&path, SYNDICATE_BIBLE)
        && holy_role(world, mob) == Some(HolyRole::HighPriest)
        && religion(world).bible_skin.is_none()
    {
        let names = BIBLE_SKINS.iter().map(|(n, ..)| (*n).to_owned()).collect();
        ask_list(
            world,
            client,
            mob,
            "Bible",
            "Select bible skin",
            names,
            ReligionAsk::BibleSkin { bible: item },
        );
        return true;
    }
    false
}

/// `pick_subtype`: the rod becomes `variant` in the same hand; the station's holy weapon.
fn pick_nullrod(world: &mut World, mob: Entity, rod: Entity, variant: &str) {
    let Some(hand) = world
        .get::<Inventory>(mob)
        .and_then(|i| i.hands.iter().position(|h| *h == Some(rod)))
    else {
        return;
    };
    let Some(new) = crate::construction::spawn_detached(world, variant) else {
        return;
    };
    world.despawn(rod);
    world.entity_mut(new).insert((Networked, CarriedBy(mob)));
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.hands[hand] = Some(new);
    }
    // `on_holy_weapon_picked`: every listed form is a `station_holy_item`.
    religion_mut(world).holy_weapon = Some(variant.to_owned());
}

/// `/obj/item/choice_beacon/holy/interact`.
fn beacon(world: &mut World, client: Entity, mob: Entity, beacon: Entity) {
    if holy_role(world, mob).is_none() {
        sound_at(world, beacon, BUZZ, 40.0, true);
        return;
    }
    if let Some(kit) = religion(world).holy_armor {
        consume_beacon(world, mob, beacon, &kit);
        return;
    }
    let kits: Vec<&'static str> = HOLY_ARMOR_KITS
        .into_iter()
        .filter(|p| has_prototype(world, p))
        .collect();
    if kits.is_empty() {
        return;
    }
    let names = kits
        .iter()
        .map(|p| crate::machine_frame::type_name(world, p))
        .collect();
    ask_list(
        world,
        client,
        mob,
        "Select an Item",
        "Which item would you like to order?",
        names,
        ReligionAsk::Armor { beacon, kits },
    );
}

/// `consume_use` + `spawn_option`: the kit lands on the beacon's tile, the beacon is spent.
// shortcut: no teleporter supply pod, no sparks; the kit appears on the tile.
fn consume_beacon(world: &mut World, mob: Entity, beacon: Entity, kit: &str) {
    let Some(pos) = crate::chem::map_pos(world, beacon) else {
        return;
    };
    tell_mob(
        world,
        mob,
        Span::Plain,
        "You hear something crackle from the beacon for a moment before a voice speaks. \
         \"Please stand by for a message from Interstellar Conservation of Religion Association. \
         Message as follows: A choice has already been made. Message ends.\"",
    );
    sound_at(world, beacon, PRAY_CHAPLAIN, 40.0, true);
    religion_mut(world).holy_armor = Some(kit.to_owned());
    crate::construction::spawn_obj(world, kit, pos);
    world.despawn(beacon);
}

/// `bookmanagement/print_bible`: a printed bible takes the station's skin, name and deity once
/// the chaplain chose a skin (`GLOB.bible_icon_state`).
pub(super) fn dress_printed_bible(world: &mut World, bible: Entity) {
    let station = religion(world);
    let Some(skin) = station.bible_skin else {
        return;
    };
    set_bible_skin(world, bible, skin);
    rename(world, bible, &station.bible_name);
    world.entity_mut(bible).insert(BibleDeity(station.deity));
}

fn set_bible_skin(world: &mut World, bible: Entity, skin: usize) {
    let Some(&(_, state, inhand)) = BIBLE_SKINS.get(skin) else {
        return;
    };
    if let Some(icon) = world.get::<DmAtom>(bible).map(|a| a.icon.clone()) {
        set_icon_state(world, bible, &icon, state);
    }
    if let Some(mut item) = world.get_mut::<DmItem>(bible) {
        inhand.clone_into(&mut item.inhand_icon_state);
    }
    if let Some(holder) = world.get::<CarriedBy>(bible).map(|c| c.0)
        && let Some(mut inventory) = world.get_mut::<Inventory>(holder)
    {
        inventory.set_changed();
    }
}

// ---------------------------------------------------------------------------------------------
// Answers

/// A [`ReligionAsk`] answered by `actor` (conscious, checked by the caller).
pub(super) fn answer(
    world: &mut World,
    actor: Entity,
    ask: ReligionAsk,
    answer: Option<PromptAnswer>,
) {
    let text = match &answer {
        Some(PromptAnswer::Text(t)) => Some(clean_name(t)),
        _ => None,
    };
    let item = match answer {
        Some(PromptAnswer::Item(i)) => Some(i),
        _ => None,
    };
    let client = client_of(world, actor);
    match ask {
        ReligionAsk::ReligionName { bible } => {
            let religion = text
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| DEFAULT_RELIGION.to_owned());
            match client.filter(|_| answer.is_some()) {
                Some(client) => ask_text(
                    world,
                    client,
                    actor,
                    "Божество",
                    "Имя вашего божества",
                    DEFAULT_DEITY,
                    ReligionAsk::DeityName { bible, religion },
                ),
                None => establish(world, actor, Some(bible), &religion, DEFAULT_DEITY, ""),
            }
        }
        ReligionAsk::DeityName { bible, religion } => {
            let deity = text
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| DEFAULT_DEITY.to_owned());
            match client.filter(|_| answer.is_some()) {
                Some(client) => {
                    let default = default_bible(&religion, "");
                    ask_text(
                        world,
                        client,
                        actor,
                        "Библия",
                        "Название вашей библии",
                        &default,
                        ReligionAsk::BibleName {
                            bible,
                            religion,
                            deity,
                        },
                    );
                }
                None => establish(world, actor, Some(bible), &religion, &deity, ""),
            }
        }
        ReligionAsk::BibleName {
            bible,
            religion,
            deity,
        } => {
            let book = text.unwrap_or_default();
            establish(world, actor, Some(bible), &religion, &deity, &book);
        }
        ReligionAsk::Nullrod { rod, variants } => {
            if let Some(&variant) = item.and_then(|i| variants.get(i))
                && holding(world, actor, rod)
            {
                pick_nullrod(world, actor, rod, variant);
            }
        }
        ReligionAsk::Armor { beacon, kits } => {
            if let Some(&kit) = item.and_then(|i| kits.get(i))
                && holding(world, actor, beacon)
                && holy_role(world, actor).is_some()
            {
                // Another beacon may have chosen meanwhile: the station keeps one kit.
                let kit = religion(world).holy_armor.unwrap_or_else(|| kit.to_owned());
                consume_beacon(world, actor, beacon, &kit);
            }
        }
        ReligionAsk::BibleSkin { bible } => {
            if let Some(i) = item.filter(|&i| i < BIBLE_SKINS.len())
                && holding(world, actor, bible)
                && holy_role(world, actor) == Some(HolyRole::HighPriest)
                && religion(world).bible_skin.is_none()
            {
                set_bible_skin(world, bible, i);
                religion_mut(world).bible_skin = Some(i);
            }
        }
        ReligionAsk::Sect { altar } => {
            if let Some(sect) = item.filter(|&i| i < Sect::ALL.len()).map(|i| Sect::ALL[i]) {
                select_sect(world, actor, altar, sect);
            }
        }
        ReligionAsk::Rite { altar, rites } => {
            if let Some(&rite) = item.and_then(|i| rites.get(i)) {
                perform_rite(world, actor, altar, rite);
            }
        }
        ReligionAsk::Deacon => {
            if matches!(answer, Some(PromptAnswer::Button(0))) {
                religion_mut(world).deacon_candidate = Some(actor);
            }
        }
        ReligionAsk::Arena { altar, areas } => {
            if let Some(&area) = item.and_then(|i| areas.get(i)) {
                start_rite(world, actor, altar, Rite::DeclareArena, Some(area));
            }
        }
        ReligionAsk::Spar(ask) => super::sparring::answer(world, actor, ask, answer),
    }
}

// ---------------------------------------------------------------------------------------------
// The altar of the gods

fn is_altar(world: &World, e: Entity) -> bool {
    world
        .get::<DmObj>(e)
        .is_some_and(|o| is_under(&o.path, ALTAR_OF_GODS))
}

fn altars(world: &mut World) -> Vec<Entity> {
    let mut q = world.query::<(Entity, &DmObj)>();
    q.iter(world)
        .filter(|(_, o)| is_under(&o.path, ALTAR_OF_GODS))
        .map(|(e, _)| e)
        .collect()
}

/// `reflect_sect_in_icons` of every altar.
fn refresh_altars(world: &mut World) {
    let state = religion(world)
        .sect
        .and_then(|s| s.def().altar_icon_state)
        .unwrap_or("convertaltar");
    for altar in altars(world) {
        if let Some(icon) = world.get::<DmAtom>(altar).map(|a| a.icon.clone()) {
            set_icon_state(world, altar, &icon, state);
        }
    }
}

/// `religious_tool/AttemptActions` on an altar of the gods: a bible opens the tool, other
/// items are offered. True when `target` is such an altar and the click was handled.
pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if !is_altar(world, target) {
        // `portable_song_tuning`: a bible on an empowered instrument offers its song rites.
        if is_under(&item_path(world, held), BIBLE)
            && world
                .get::<super::festival::PortableAltar>(target)
                .is_some()
        {
            open_tool(world, client, mob, target);
            return true;
        }
        return false;
    }
    if is_under(&item_path(world, held), BIBLE) {
        open_tool(world, client, mob, target);
        return true;
    }
    sacrifice(world, client, mob, held)
}

/// `religious_tool/ui_interact` as prompts: the sects to choose from, or the rites.
fn open_tool(world: &mut World, client: Entity, mob: Entity, altar: Entity) {
    let station = religion(world);
    let deity = glob_deity(world);
    let portable = world.get::<super::festival::PortableAltar>(altar).is_some();
    let Some(sect) = station.sect else {
        if portable {
            return;
        }
        let names = Sect::ALL.iter().map(|s| s.def().name.to_owned()).collect();
        let message = Sect::ALL
            .iter()
            .map(|s| format!("{}: {}", s.def().name, s.def().desc))
            .collect::<Vec<_>>()
            .join("\n");
        ask_list(
            world,
            client,
            mob,
            "Select a sect",
            &message,
            names,
            ReligionAsk::Sect { altar },
        );
        return;
    };
    let def = sect.def();
    let mut rites = station.rites();
    if portable {
        // `rite_types_allowlist`: the song tuners only.
        rites.retain(|r| matches!(r, Rite::Tune(_)));
    }
    let favor = match sect {
        Sect::Greed => format!("In the eyes of {deity}, your wealth is your favor."),
        _ => format!(
            "You are currently at {} favor with {deity}.",
            station.favor.floor()
        ),
    };
    // `tool_examine`.
    let favor = match sect {
        Sect::Burden => format!("{favor}\n{}", super::burden::tool_examine(world, mob)),
        Sect::Spar => format!(
            "{favor}\n{}",
            super::sparring::tool_examine(&station, &deity)
        ),
        _ => favor,
    };
    let names = rites
        .iter()
        .map(|r| {
            let d = r.def();
            if d.money_cost > 0 {
                format!("{} ({} cr)", d.name, d.money_cost)
            } else {
                format!("{} ({} favor)", d.name, d.favor_cost)
            }
        })
        .collect();
    let message = format!("\"{}\"\n{}\n{favor}", def.quote, def.desc);
    ask_list(
        world,
        client,
        mob,
        def.name,
        &message,
        names,
        ReligionAsk::Rite { altar, rites },
    );
}

/// `select_sect` -> `set_new_religious_sect`.
fn select_sect(world: &mut World, actor: Entity, altar: Entity, sect: Sect) {
    if holy_role(world, actor) != Some(HolyRole::HighPriest) {
        tell_mob(
            world,
            actor,
            Span::Warning,
            "You are not the high priest, and therefore cannot select a religious sect.",
        );
        return;
    }
    if !crate::storage::in_reach(world, actor, altar) || !holds_bible(world, actor) {
        tell_mob(
            world,
            actor,
            Span::Warning,
            "You cannot select a sect at this time.",
        );
        return;
    }
    {
        let mut station = religion_mut(world);
        if station.sect.is_some() {
            return;
        }
        station.sect = Some(sect);
        station.favor = 0.0;
        station.used_rites.clear();
    }
    let mut q = world.query::<(Entity, &HolyRole)>();
    let mut holy: Vec<Entity> = q.iter(world).map(|(e, _)| e).collect();
    if !holy.contains(&actor) {
        holy.push(actor);
    }
    for mob in holy {
        on_conversion(world, mob, sect);
    }
    refresh_altars(world);
}

/// `can_sacrifice` + `on_sacrifice` of the sect for `item`. True when the sect took the click.
fn sacrifice(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(sect) = religion(world).sect else {
        return false;
    };
    let path = item_path(world, item);
    let wanted = match sect {
        Sect::Mechanical => is_under(&path, CELL),
        Sect::Pyre => is_under(&path, CANDLE),
        _ => false,
    };
    if !wanted {
        return false;
    }
    let deity = glob_deity(world);
    if holy_role(world, mob) == Some(HolyRole::Deacon) {
        // tg goes on to take the offering anyway; a deacon is meant not to perform rites.
        tell(
            world,
            client,
            Span::Warning,
            format!("You are merely a deacon of {deity}, and therefore cannot perform rites."),
        );
        return true;
    }
    match sect {
        Sect::Mechanical => {
            let charge = world
                .get::<crate::mecha::core::ItemCell>(item)
                .map_or(0.0, |c| c.0.charge);
            let unit = 0.3 * STANDARD_CELL_CHARGE;
            if charge < unit {
                tell(
                    world,
                    client,
                    Span::Notice,
                    format!("{deity} does not accept pity amounts of power."),
                );
                return true;
            }
            religion_mut(world).adjust_favor((charge / unit).floor() as f32);
            let name = name_of(world, item);
            tell(
                world,
                client,
                Span::Notice,
                format!("You offer {name}'s power to {deity}, pleasing them."),
            );
            world.despawn(item);
        }
        Sect::Pyre => {
            // shortcut: candles cannot be lit yet; the immortal ones always are.
            if !is_under(&path, INFINITE_CANDLE) {
                tell(
                    world,
                    client,
                    Span::Notice,
                    "The candle needs to be lit to be offered!",
                );
                return true;
            }
            tell(
                world,
                client,
                Span::Notice,
                format!("{deity} is pleased with your sacrifice."),
            );
            religion_mut(world).adjust_favor(40.0);
            world.despawn(item);
        }
        _ => {}
    }
    true
}

// ---------------------------------------------------------------------------------------------
// Rites

pub(super) fn account_of_mob(world: &World, mob: Entity) -> Option<u32> {
    let inventory = world.get::<Inventory>(mob)?;
    inventory
        .id_search()
        .find_map(|card| crate::cargo::economy::account_of(world, card))
}

pub(super) fn balance(world: &World, mob: Entity) -> Option<i64> {
    let id = account_of_mob(world, mob)?;
    world
        .get_resource::<crate::cargo::economy::Economy>()?
        .account(id)
        .map(|a| a.balance)
}

fn withdraw(world: &mut World, mob: Entity, amount: i64) -> bool {
    let Some(id) = account_of_mob(world, mob) else {
        return false;
    };
    world
        .get_resource_mut::<crate::cargo::economy::Economy>()
        .and_then(|mut e| e.account_mut(id).map(|a| a.adjust_money(-amount)))
        .unwrap_or(false)
}

/// `can_afford`: the favor, or the greedy god's credits.
fn can_afford(world: &mut World, user: Entity, rite: Rite) -> bool {
    let def = rite.def();
    if def.money_cost > 0 {
        return match balance(world, user) {
            None => {
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    "You need a way to pay for the rite!",
                );
                false
            }
            Some(b) if b < def.money_cost => {
                tell_mob(world, user, Span::Warning, "This rite requires more money!");
                false
            }
            Some(_) => true,
        };
    }
    if religion(world).favor < def.favor_cost {
        tell_mob(world, user, Span::Warning, "This rite requires more favor!");
        return false;
    }
    true
}

/// The human buckled to the altar (`buckled_mobs`).
fn buckled_human(world: &mut World, altar: Entity) -> Option<Entity> {
    crate::medical::surgery::buckled_to(world, altar).filter(|&m| is_human(world, m))
}

/// `religious_tool/perform_rite` -> the rite's `perform_rite` checks, then its first step.
fn perform_rite(world: &mut World, user: Entity, altar: Entity, rite: Rite) {
    if !can_begin(world, user, altar, rite) {
        return;
    }
    let Some(target) = rite_target(world, user, altar, rite) else {
        return;
    };
    begin_rite(world, user, altar, rite, target);
}

/// A rite whose target was asked for first ("Declare Arena"): the checks again, then it begins.
fn start_rite(world: &mut World, user: Entity, altar: Entity, rite: Rite, target: Option<Entity>) {
    if can_begin(world, user, altar, rite) {
        begin_rite(world, user, altar, rite, target);
    }
}

/// The altar of the gods, or the instrument of portable song tuning held or next to the user.
fn tool_in_reach(world: &World, user: Entity, tool: Entity) -> bool {
    world.get::<CarriedBy>(tool).map(|c| c.0) == Some(user)
        || crate::storage::in_reach(world, user, tool)
}

/// What the rite's do_afters are on: the tool on the floor, else the user (a held instrument).
fn rite_anchor(world: &World, user: Entity, tool: Entity) -> Entity {
    if world.get::<TilePos>(tool).is_some() {
        tool
    } else {
        user
    }
}

/// The rite's text with `GLOB.deity` in.
fn rite_line(world: &World, line: &str) -> String {
    line.replace("{deity}", &glob_deity(world))
}

/// The holy role, the rite still on offer, the tool in reach with a bible in hand, the price.
fn can_begin(world: &mut World, user: Entity, altar: Entity, rite: Rite) -> bool {
    let deity = glob_deity(world);
    match holy_role(world, user) {
        Some(HolyRole::Priest | HolyRole::HighPriest) => {}
        Some(HolyRole::Deacon) => {
            tell_mob(
                world,
                user,
                Span::Warning,
                format!("You are merely a deacon of {deity}, and therefore cannot perform rites."),
            );
            return false;
        }
        None => {
            tell_mob(
                world,
                user,
                Span::Warning,
                "You are not holy, and therefore cannot perform rites.",
            );
            return false;
        }
    }
    if !religion(world).rites().contains(&rite) {
        return false;
    }
    if world.get_entity(altar).is_err()
        || !tool_in_reach(world, user, altar)
        || !holds_bible(world, user)
    {
        tell_mob(
            world,
            user,
            Span::Warning,
            "You are not close enough to perform the rite.",
        );
        return false;
    }
    can_afford(world, user, rite)
}

fn begin_rite(world: &mut World, user: Entity, altar: Entity, rite: Rite, target: Option<Entity>) {
    let def = rite.def();
    tell_mob(
        world,
        user,
        Span::Notice,
        format!("You begin to perform the rite of {}...", def.name),
    );
    if let Some(first) = def.invocations.first() {
        let line = rite_line(world, first);
        say(world, user, &line);
    }
    world.entity_mut(user).insert(PerformingRite {
        rite,
        altar,
        step: 0,
        target,
    });
    if !rite_step(world, user, altar, rite) {
        world.entity_mut(user).remove::<PerformingRite>();
    }
}

fn rite_step(world: &mut World, user: Entity, altar: Entity, rite: Rite) -> bool {
    let Some(client) = client_of(world, user) else {
        return false;
    };
    let item = world
        .get::<Inventory>(user)
        .and_then(Inventory::active_item);
    let delay = rite.def().length * 10.0 / rite.steps() as f32;
    let anchor = rite_anchor(world, user, altar);
    do_after(
        world,
        Actor {
            client,
            mob: user,
            item,
        },
        anchor,
        delay,
        0,
        Job::Religion(ReligionJob::RiteStep),
    )
}

/// The rite's own `perform_rite` checks: `None` stops it; `Some(target)` is what it works on.
fn rite_target(
    world: &mut World,
    user: Entity,
    altar: Entity,
    rite: Rite,
) -> Option<Option<Entity>> {
    match rite {
        Rite::Deaconize | Rite::DeaconizeDreamers => {
            let Some(candidate) = buckled_human(world, altar) else {
                let what = name_of(world, altar);
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    format!("Ничего не пристегнуто к {what}!"),
                );
                return None;
            };
            let name = name_of(world, candidate);
            if world.get::<Stat>(candidate) != Some(&Stat::Conscious) {
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    format!("{name} должен быть жив и в сознании, чтобы вступить!"),
                );
                return None;
            }
            if world.get::<HolyRole>(candidate).is_some() {
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    format!("{name} уже является членом этой религии!"),
                );
                return None;
            }
            if religion(world).deacon_candidate != Some(candidate) {
                invite_deacon(world, candidate);
                tell_mob(
                    world,
                    user,
                    Span::Notice,
                    "Претенденту была предложена возможность вступить в наши ряды. Подождите, \
                     пока он решит, и попробуйте ещё раз.",
                );
                return None;
            }
            Some(Some(candidate))
        }
        Rite::SynthConversion => {
            if let Some(convert) = buckled_human(world, altar) {
                let what = name_of(world, altar);
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    format!("You're going to convert the one buckled on {what}."),
                );
                return Some(Some(convert));
            }
            if world.get::<crate::species::Species>(user).map(|s| s.0)
                == Some(crate::species::SpeciesId::Android)
            {
                let what = name_of(world, altar);
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    format!(
                        "You've already converted yourself. To convert others, they must be \
                         buckled to {what}."
                    ),
                );
                return None;
            }
            tell_mob(
                world,
                user,
                Span::Warning,
                "You're going to convert yourself with this ritual.",
            );
            Some(Some(user))
        }
        Rite::Fireproof => {
            let pos = *world.get::<TilePos>(altar)?;
            let apparel = world
                .resource::<SpatialIndex>()
                .on_tile(pos)
                .iter()
                .copied()
                .find(|&e| {
                    world.get::<DmItem>(e).is_some()
                        && world.get::<DmClothing>(e).is_some_and(|c| {
                            c.max_heat_protection_temperature < FIRE_IMMUNITY_MAX_TEMP_PROTECT
                        })
                })?;
            Some(Some(apparel))
        }
        Rite::BurningSacrifice => {
            let Some(corpse) = crate::medical::surgery::buckled_to(world, altar) else {
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    "Nothing is buckled to the altar!",
                );
                return None;
            };
            if !is_human(world, corpse) {
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    "Only carbon lifeforms can be properly burned for the sacrifice!",
                );
                return None;
            }
            if world.get::<Stat>(corpse) != Some(&Stat::Dead) {
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    "You can only sacrifice dead bodies, this one is still alive!",
                );
                return None;
            }
            if !burnt(world, corpse) {
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    "This corpse needs to be on fire or husked to be sacrificed!",
                );
                return None;
            }
            Some(Some(corpse))
        }
        Rite::MachineBlessing | Rite::InfiniteCandle | Rite::CustomVending => Some(None),
        Rite::NullrodTransformation => super::burden::rite_target(world, user, altar),
        Rite::SparringContract | Rite::CeremonialWeapon | Rite::Unbreakable => {
            super::sparring::rite_target(world, user, altar, rite)
        }
        Rite::DeclareArena => {
            let client = client_of(world, user)?;
            let areas = super::sparring::declarable_areas(world);
            let names = areas
                .iter()
                .map(|&a| crate::shuttle::area_name(world, a))
                .collect();
            ask_list(
                world,
                client,
                user,
                "Arena Declaration",
                "Choose an area to mark as an arena!",
                names,
                ReligionAsk::Arena { altar, areas },
            );
            None
        }
        Rite::HolyViolin | Rite::Tune(_) => Some(None),
        Rite::PortableSongTuning => super::festival::rite_target(world, user, altar),
        Rite::DreamPortent | Rite::DreamProtection | Rite::SlumberParty => {
            super::dream_sect::rite_target(world, user, rite)
        }
    }
}

/// On fire or husked (`on_fire || HAS_TRAIT_FROM(TRAIT_HUSK, BURN)`).
fn burnt(world: &World, mob: Entity) -> bool {
    world
        .get::<crate::fire::FireStacks>(mob)
        .is_some_and(|f| f.on_fire)
        || world
            .get::<crate::human::HumanLook>(mob)
            .is_some_and(|l| l.husk)
}

/// `invite_deacon`: the candidate is asked (60 s).
fn invite_deacon(world: &mut World, candidate: Entity) {
    let Some(client) = client_of(world, candidate) else {
        return;
    };
    let deity = glob_deity(world);
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            candidate,
            "Приглашение",
            format!(
                "Присоединиться к {deity}? Ожидается, что вы будете следовать указам священника."
            ),
            PromptKind::Alert {
                buttons: vec!["Да".to_owned(), "Нет".to_owned()],
                autofocus: true,
            },
        )
        .timeout(60.0),
    );
    world
        .entity_mut(prompt)
        .insert(PendingAsk::Religion(ReligionAsk::Deacon));
}

/// A religion do_after finished.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: ReligionJob) {
    match job {
        ReligionJob::RiteStep => {
            let Some(mut state) = world.get::<PerformingRite>(mob).copied() else {
                return;
            };
            if rite_anchor(world, mob, state.altar) != action.target {
                return;
            }
            state.step += 1;
            let def = state.rite.def();
            if state.step < state.rite.steps() {
                if let Some(line) = def.invocations.get(state.step) {
                    let line = rite_line(world, line);
                    say(world, mob, &line);
                }
                world.entity_mut(mob).insert(state);
                if !rite_step(world, mob, state.altar, state.rite) {
                    world.entity_mut(mob).remove::<PerformingRite>();
                }
                return;
            }
            world.entity_mut(mob).remove::<PerformingRite>();
            // tg says `invoke_msg` only after invocations; the rites without any have one too.
            if let Some(line) = def.invoke_msg {
                say(world, mob, line);
            }
            finish_rite(world, mob, state);
        }
        ReligionJob::UnpackAltar => {
            if !altars(world).is_empty() {
                return;
            }
            crate::construction::spawn_obj(world, ALTAR_OF_GODS, action.target_pos);
            if let Some(bible) = action.item.filter(|&b| world.get_entity(b).is_ok()) {
                world.despawn(bible);
            }
            refresh_altars(world);
        }
        ReligionJob::PrayRefill => super::burden::refill_complete(world, mob, action),
    }
}

/// `invoke_effect`, then the favor is paid and a one-time rite is gone.
fn finish_rite(world: &mut World, user: Entity, state: PerformingRite) {
    if !religion(world).rites().contains(&state.rite) {
        return;
    }
    // The favor or money may have gone to another rite meanwhile.
    if !can_afford(world, user, state.rite) {
        return;
    }
    if !invoke_effect(world, user, state) {
        return;
    }
    let def = state.rite.def();
    {
        let mut station = religion_mut(world);
        station.adjust_favor(-def.favor_cost);
        if def.one_time {
            station.used_rites.push(state.rite);
        }
    }
    post_invoke_effects(world, user, state);
}

/// `post_invoke_effects` of the rites with one, and the portable altar's charge.
fn post_invoke_effects(world: &mut World, user: Entity, state: PerformingRite) {
    super::festival::use_charge(world, state.altar);
    match state.rite {
        Rite::DeaconizeDreamers
        | Rite::DreamPortent
        | Rite::DreamProtection
        | Rite::SlumberParty => {
            super::dream_sect::post_invoke(world, user, state.rite, state.altar);
        }
        _ => {}
    }
}

fn altar_pos(world: &World, altar: Entity) -> Option<TilePos> {
    world.get::<TilePos>(altar).copied()
}

fn invoke_effect(world: &mut World, user: Entity, state: PerformingRite) -> bool {
    let altar = state.altar;
    // A held instrument (portable song tuning) is where its holder is.
    let Some(pos) = altar_pos(world, altar).or_else(|| crate::chem::map_pos(world, altar)) else {
        return false;
    };
    let deity = glob_deity(world);
    match state.rite {
        Rite::NullrodTransformation => super::burden::transmogrify(world, user, state, pos),
        Rite::SparringContract
        | Rite::CeremonialWeapon
        | Rite::DeclareArena
        | Rite::Unbreakable => super::sparring::invoke_effect(world, user, state, pos),
        Rite::HolyViolin | Rite::PortableSongTuning | Rite::Tune(_) => {
            super::festival::invoke_effect(world, user, state, pos)
        }
        Rite::DreamPortent | Rite::DreamProtection | Rite::SlumberParty => {
            super::dream_sect::invoke_effect(world, user, state.rite)
        }
        Rite::Deaconize | Rite::DeaconizeDreamers => {
            let Some(deacon) = state.target else {
                return false;
            };
            let name = name_of(world, deacon);
            if crate::medical::surgery::buckled_to(world, altar) != Some(deacon) {
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    format!("{name} больше не находится на алтаре!"),
                );
                return false;
            }
            if world.get::<Stat>(deacon) != Some(&Stat::Conscious) {
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    format!("{name} должен быть в сознании, чтобы сработал ритуал!"),
                );
                return false;
            }
            if client_of(world, deacon).is_none() {
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    format!("Разум {name} находится в другом месте!"),
                );
                return false;
            }
            if crate::antag::cult::server::is_cultist(world, deacon) {
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    format!(
                        "{deity} увидел истинное, тёмное зло в сердце {name}, потому он был \
                         поражён!"
                    ),
                );
                sound_at(world, altar, PRAY, 50.0, true);
                crate::health::gib(world, deacon);
                return false;
            }
            tell_mob(
                world,
                user,
                Span::Notice,
                format!(
                    "{deity} связал {name} с кодексом! Теперь он занимает святую роль (пусть и \
                     самого низшего ранга)!"
                ),
            );
            world.entity_mut(deacon).insert(HolyRole::Deacon);
            religion_mut(world).deacon_candidate = None;
            if let Some(sect) = religion(world).sect {
                on_conversion(world, deacon, sect);
            }
            sound_at(world, altar, PRAY, 50.0, true);
            true
        }
        Rite::SynthConversion => {
            let Some(convert) = state.target.filter(|&t| world.get_entity(t).is_ok()) else {
                return false;
            };
            if convert != user && crate::medical::surgery::buckled_to(world, altar) != Some(convert)
            {
                return false;
            }
            crate::species::set_species(world, convert, crate::species::SpeciesId::Android);
            let (name, rite) = (name_of(world, convert), state.rite.def().name);
            say_around(
                world,
                convert,
                Span::Notice,
                format!("{name} has been converted by the rite of {rite}!"),
                &[],
            );
            true
        }
        Rite::MachineBlessing => {
            let blessing = machine_blessing(world);
            crate::construction::spawn_obj(world, blessing, pos);
            true
        }
        Rite::Fireproof => {
            let Some(apparel) = state
                .target
                .filter(|&a| world.get::<TilePos>(a) == Some(&pos))
            else {
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    "The clothing that was chosen for the rite is no longer on the altar!",
                );
                return false;
            };
            // `apply_fireproof`.
            // shortcut: a hood's integrated helmet and `FIRE_PROOF` are not ported.
            if let Some(mut clothing) = world.get_mut::<DmClothing>(apparel) {
                let covered = clothing.body_parts_covered;
                clothing.max_heat_protection_temperature = FIRE_IMMUNITY_MAX_TEMP_PROTECT;
                clothing.heat_protection = covered;
            }
            let name = format!("unmelting {}", name_of(world, apparel));
            rename(world, apparel, &name);
            sound_at(world, altar, FIREBALL, 50.0, true);
            true
        }
        Rite::BurningSacrifice => {
            let Some(corpse) = state.target else {
                return false;
            };
            if crate::medical::surgery::buckled_to(world, altar) != Some(corpse) {
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    "The right sacrifice is no longer on the altar!",
                );
                return false;
            }
            if !burnt(world, corpse) {
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    "The sacrifice has to be on fire or husked to finish the end of the rite!",
                );
                return false;
            }
            if world.get::<Stat>(corpse) != Some(&Stat::Dead) {
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    "The sacrifice has to stay dead for the rite to work!",
                );
                return false;
            }
            let burn = world.get::<Health>(corpse).map_or(0.0, |h| h.burn);
            let gained = 100.0 + burn.floor();
            religion_mut(world).adjust_favor(gained);
            tell_mob(
                world,
                user,
                Span::Notice,
                format!(
                    "{deity} absorbs the charred corpse and any trace of fire with it. {deity} \
                     rewards you with {gained} favor."
                ),
            );
            sound_at(world, altar, SUPERMATTER, 50.0, true);
            // shortcut: `dust(force = TRUE)` is a gib here (no ash; the items fall).
            crate::health::gib(world, corpse);
            true
        }
        Rite::InfiniteCandle => {
            for _ in 0..5 {
                crate::construction::spawn_obj(world, INFINITE_CANDLE, pos);
            }
            sound_at(world, altar, FIREBALL, 50.0, true);
            true
        }
        Rite::CustomVending => {
            let cost = state.rite.def().money_cost;
            if !withdraw(world, user, cost) {
                tell_mob(world, user, Span::Warning, "This rite requires more money!");
                return false;
            }
            if let Some(vendor) = crate::construction::spawn_obj(world, GREED_VENDOR, pos) {
                crate::vending::refill::setup_built(world, vendor);
                rename(world, vendor, &format!("{deity}'s Consecrated Vendor"));
                if let Some(mut dm) = world.get_mut::<DmName>(vendor) {
                    dm.desc = format!("A vending machine created by {deity}.");
                }
            }
            sound_at(world, altar, CASH_REGISTER, 60.0, true);
            true
        }
    }
}

/// `pick_weight_recursive` of `machine_blessing`: (group weight, [(type index, weight)]).
const BLESSING_WEIGHTS: [(f32, &[(usize, f32)]); 5] = [
    (15.0, &[(0, 1.0), (1, 1_000_000.0), (2, 1_500_000.0)]),
    (15.0, &[(3, 1.0), (4, 1.0), (5, 1.0), (6, 2.0)]),
    (9.0, &[(7, 1.0), (8, 2.0)]),
    (10.0, &[(9, 50.0), (10, 50.0), (11, 10.0)]),
    (5.0, &[(12, 1.0)]),
];

fn weighted<T: Copy>(world: &mut World, items: &[(T, f32)]) -> T {
    let total: f32 = items.iter().map(|(_, w)| w).sum();
    let mut roll = world.resource_mut::<crate::combat::CombatRng>().unit() * total;
    for &(item, weight) in items {
        if roll < weight {
            return item;
        }
        roll -= weight;
    }
    items[items.len() - 1].0
}

fn machine_blessing(world: &mut World) -> &'static str {
    let groups: Vec<(usize, f32)> = BLESSING_WEIGHTS
        .iter()
        .enumerate()
        .map(|(i, (w, _))| (i, *w))
        .collect();
    let group = weighted(world, &groups);
    let index = weighted(world, BLESSING_WEIGHTS[group].1);
    MACHINE_BLESSINGS[index]
}

// ---------------------------------------------------------------------------------------------
// The bible

/// What a sect's blessing did (`BLESSING_FAILED`, `_IGNORED`, `_SUCCESS`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blessing {
    Failed,
    Ignored,
    Success,
}

/// Heals every hurt bodypart by `amount` brute and burn; false when none was hurt.
pub(super) fn heal_hurt_parts(world: &mut World, target: Entity, amount: f32) -> bool {
    let Some(mut health) = world.get::<Health>(target).cloned() else {
        return false;
    };
    let hurt: Vec<BodyZone> = BodyZone::ALL
        .into_iter()
        .filter(|z| health.parts[z.index()].damage() > 0.0)
        .collect();
    if hurt.is_empty() {
        return false;
    }
    for zone in hurt {
        health.heal_zone(zone, amount, amount);
    }
    let mut stat = world.get::<Stat>(target).copied().unwrap_or_default();
    let before = stat;
    crate::health::update_stat(&health, &mut stat);
    world.entity_mut(target).insert(health);
    if stat != before {
        world.entity_mut(target).insert(stat);
    }
    true
}

/// The sect's `sect_bless`; `None` leaves it to the bible's own blessing.
pub(super) fn sect_bless(world: &mut World, user: Entity, target: Entity) -> Option<Blessing> {
    let sect = religion(world).sect?;
    let deity = glob_deity(world);
    let (name, user_name) = (name_of(world, target), name_of(world, user));
    match sect {
        Sect::Puritanism | Sect::Pyre | Sect::Spar | Sect::Music => None,
        Sect::Burden => Some(super::burden::sect_bless(world, user, target)),
        Sect::Dreams => Some(super::dream_sect::sect_bless(world, user, target)),
        Sect::Mechanical => {
            // tg adds over the cell's capacity; the cell is meant to fill up.
            let charged = world
                .get_mut::<crate::silicon::cyborg::Cyborg>(target)
                .map(|mut borg| {
                    let max = borg.max_charge;
                    borg.charge = (borg.charge + 0.05 * STANDARD_CELL_CHARGE).min(max);
                })
                .is_some();
            if charged {
                say_around(
                    world,
                    target,
                    Span::Notice,
                    format!("{user_name} charges {name} with the power of {deity}!"),
                    &[],
                );
                tell_mob(
                    world,
                    target,
                    Span::BoldNotice,
                    format!("You are charged by the power of {deity}!"),
                );
                crate::mood::add_event(world, target, "blessing", &super::library::BLESSING);
                sound_at(world, user, BANG, 25.0, true);
                return Some(Blessing::Success);
            }
            if !is_human(world, target) {
                return Some(Blessing::Failed);
            }
            tell_mob(
                world,
                user,
                Span::Warning,
                format!("{deity} scoffs at the idea of healing such fleshy matter!"),
            );
            Some(Blessing::Ignored)
        }
        Sect::Greed => {
            if !is_human(world, target) {
                return Some(Blessing::Failed);
            }
            let Some(balance) = balance(world, user) else {
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    "You need a way to pay for the heal!",
                );
                return Some(Blessing::Ignored);
            };
            if balance < GREEDY_HEAL_COST {
                tell_mob(
                    world,
                    user,
                    Span::Warning,
                    format!("Healing from {deity} costs {GREEDY_HEAL_COST} cr for 30 health!"),
                );
                return Some(Blessing::Ignored);
            }
            withdraw(world, user, GREEDY_HEAL_COST);
            if !heal_hurt_parts(world, target, 30.0) {
                return Some(Blessing::Ignored);
            }
            say_around(
                world,
                target,
                Span::Notice,
                format!("{user_name} barters a heal for {name} from {deity}!"),
                &[],
            );
            tell_mob(
                world,
                target,
                Span::BoldNotice,
                format!(
                    "May the power of {deity} compel you to be healed! Thank you for choosing \
                     {deity}!"
                ),
            );
            sound_at(world, user, CASH_REGISTER, 60.0, true);
            crate::mood::add_event(world, target, "blessing", &super::library::BLESSING);
            Some(Blessing::Success)
        }
    }
}

/// The sect's `sect_dead_bless` on a corpse; `Blessing::Failed` smacks it.
pub(super) fn sect_dead_bless(world: &mut World, user: Entity, target: Entity) -> Blessing {
    match religion(world).sect {
        Some(Sect::Burden) => super::burden::sect_bless(world, user, target),
        Some(Sect::Dreams) => super::dream_sect::sect_dead_bless(world, user, target),
        _ => Blessing::Failed,
    }
}

/// `interact_with_atom` on a floor: in the chapel with no altar of the gods anywhere the bible
/// unpacks into one; elsewhere "floor smacked!". True when `target` is a floor.
// shortcut: no cult rune revealing.
pub(super) fn bible_on_floor(
    world: &mut World,
    client: Entity,
    mob: Entity,
    bible: Entity,
    target: Entity,
) -> bool {
    let Some(path) = path_of(world, target) else {
        return false;
    };
    if !is_under(&path, FLOOR) {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return false;
    };
    let in_chapel = world
        .get_resource::<Areas>()
        .and_then(|a| a.area_at(pos))
        .and_then(|area| world.get::<DmPower>(area))
        .is_some_and(|p| is_under(&p.path, CHAPEL));
    if in_chapel && altars(world).is_empty() {
        tell(world, client, Span::Notice, "unpacking bible...");
        do_after(
            world,
            Actor {
                client,
                mob,
                item: Some(bible),
            },
            target,
            UNPACK_ALTAR_DS,
            0,
            Job::Religion(ReligionJob::UnpackAltar),
        );
        return true;
    }
    tell(world, client, Span::Notice, "floor smacked!");
    true
}

/// `interact_with_atom` on another bible: it becomes this one (name, look, deity).
pub(super) fn bible_on_bible(
    world: &mut World,
    client: Entity,
    bible: Entity,
    target: Entity,
) -> bool {
    let path = item_path(world, target);
    if !is_under(&path, BIBLE) || is_under(&path, SYNDICATE_BIBLE) || target == bible {
        return false;
    }
    tell(world, client, Span::Notice, "converted");
    let name = name_of(world, bible);
    rename(world, target, &name);
    let state = world
        .get::<ss13_shared::Appearance>(bible)
        .map(|a| a.icon_state);
    if let Some(state) = state
        && let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(target)
    {
        look.icon_state = state;
    }
    if let Some(inhand) = world
        .get::<DmItem>(bible)
        .map(|i| i.inhand_icon_state.clone())
        && let Some(mut item) = world.get_mut::<DmItem>(target)
    {
        item.inhand_icon_state = inhand;
    }
    let deity = deity(world, Some(bible));
    world.entity_mut(target).insert(BibleDeity(deity));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favor_stays_within_the_sect_limits() {
        let mut r = Religion {
            sect: Some(Sect::Mechanical),
            ..Default::default()
        };
        assert_eq!(r.adjust_favor(3000.0), 2500.0);
        assert_eq!(r.favor, 2500.0);
        assert_eq!(r.adjust_favor(-3000.0), -2500.0);
        assert_eq!(r.favor, 0.0);
    }

    #[test]
    fn every_sect_offers_deaconize_once() {
        let mut r = Religion {
            sect: Some(Sect::Pyre),
            ..Default::default()
        };
        assert_eq!(
            r.rites(),
            [
                Rite::Fireproof,
                Rite::BurningSacrifice,
                Rite::InfiniteCandle,
                Rite::Deaconize
            ]
        );
        r.used_rites.push(Rite::Deaconize);
        assert!(!r.rites().contains(&Rite::Deaconize));
        assert!(Religion::default().rites().is_empty());
    }

    #[test]
    fn rites_split_their_length_over_the_invocations() {
        assert_eq!(Rite::Deaconize.steps(), 5);
        assert_eq!(Rite::InfiniteCandle.steps(), 1);
        for rite in [
            Rite::Deaconize,
            Rite::SynthConversion,
            Rite::MachineBlessing,
            Rite::Fireproof,
            Rite::BurningSacrifice,
            Rite::InfiniteCandle,
            Rite::CustomVending,
        ] {
            assert!(rite.def().length > 0.0);
        }
    }

    #[test]
    fn the_new_sects_bring_their_rites_and_smack_chances() {
        let music = Religion {
            sect: Some(Sect::Music),
            ..Default::default()
        };
        let rites = music.rites();
        assert!(rites.contains(&Rite::Tune(SongTune::Lullaby)));
        assert!(rites.contains(&Rite::Deaconize));
        let dreams = Religion {
            sect: Some(Sect::Dreams),
            ..Default::default()
        };
        assert!(!dreams.rites().contains(&Rite::Deaconize));
        assert_eq!(Sect::Burden.def().smack_chance, 0.0);
        assert_eq!(Sect::Dreams.def().smack_chance, 20.0);
        assert_eq!(Sect::Spar.def().smack_chance, DEFAULT_SMACK_CHANCE);
        // The dream god's deacon rite is the deaconize rite, reusable.
        let dreamers = Rite::DeaconizeDreamers.def();
        assert_eq!(dreamers.name, Rite::Deaconize.def().name);
        assert!(!dreamers.one_time && Rite::Deaconize.def().one_time);
        assert_eq!(Rite::DeclareArena.def().favor_cost, 1.0);
        assert_eq!(Rite::NullrodTransformation.def().length, 10.0);
        assert!(Rite::DreamProtection.def().one_time);
    }

    #[test]
    fn excommunicated_chaplains_are_not_holy() {
        let mut world = World::new();
        let mob = world.spawn((HolyRole::Priest, Excommunicated)).id();
        assert_eq!(holy_role(&world, mob), None);
    }

    #[test]
    fn dream_invocations_name_the_deity() {
        let mut world = World::new();
        world.insert_resource(Religion {
            deity: "Morpheus".into(),
            ..Default::default()
        });
        let line = rite_line(&world, Rite::DreamPortent.def().invocations[0]);
        assert_eq!(
            line,
            "O great shepherd Morpheus, grant me a vision of the future!.."
        );
    }

    #[test]
    fn joke_religions_pick_their_books() {
        let weeb = Joke::of("Weeaboo").unwrap();
        assert!(weeb.books().contains(&"The Manganomicon"));
        assert_eq!(weeb.deities("The Manganomicon"), ["Anime"]);
        assert!(!weeb.brain_damage());
        let meme = Joke::of("LOL").unwrap();
        assert_eq!(
            meme.deities("F.A.T.A.L. Rulebook"),
            ["Twenty Ten-Sided Dice"]
        );
        assert!(meme.brain_damage());
        assert!(Joke::of("Christianity").is_none());
        assert_eq!(default_bible("Buddhism", ""), "The Holy Book of Buddhism");
        assert_eq!(default_bible("Buddhism", "Sutra"), "Sutra");
        assert_eq!(clean_name("  Space  Jesus \n"), "Space  Jesus");
    }

    #[test]
    fn blessing_weights_name_every_blessing() {
        let mut seen: Vec<usize> = BLESSING_WEIGHTS
            .iter()
            .flat_map(|(_, items)| items.iter().map(|(i, _)| *i))
            .collect();
        seen.sort_unstable();
        assert_eq!(seen, (0..MACHINE_BLESSINGS.len()).collect::<Vec<_>>());
    }
}
