//! Subdermal implants (server), after tg `code/game/objects/items/implants/`: the implant
//! framework (`implant.dm`: `implant()`, `removed()`, `activate()`), the implanter
//! (`implanter.dm`), the implant case (`implantcase.dm`), the implant pad ([`pad`]), the implant
//! chairs ([`chair`]) and the implants the port has: mindshield, tracking (and the .38 TRAC
//! one), chem, exile and anti-teleportation, bluespace grounding, beacon, microbomb, macrobomb,
//! fake macrobomb and tactical deniability, EMP, smoke, freedom, firearms authentication, sad
//! trombone, deathrattle, spell, the abductor's recall, the MOD pathfinder's
//! (`crate::modsuit::pathfinder`) and BandaStation's CQC, death alarm and EMP absorber.
//!
//! - An implant is data ([`Implant`]), not an entity: it lives in an implanter or a case
//!   ([`ImplantHolder`], filled from the container's type, tg `imp_type`) or in a mob
//!   ([`Implants`], tg `implants`). A chem implant's reagents are the case's [`Reagents`] while it
//!   is in a case (tg `reagents = imp.reagents`), so a syringe fills it there.
//! - The implanter on a living mob ([`implant_clicks`], tg `implanter/attack`): yourself at once,
//!   someone else after a 5 s `do_after`; then [`implant`] (tg `implant()`): no silicons, at most
//!   [`SECURITY_IMPLANT_CAP`] security implants, a duplicate (or a subtype of it) tops up its
//!   uses or is refused, a microbomb merges into one already there (the deniability implant
//!   takes the other in; two do not fit), a deathrattle needs a group. The implanter on a case
//!   swaps the implant between them; a case goes into the implant pad.
//! - Activated implants get an action button (tg `/datum/action/item_action/hands_free/activate`):
//!   the EMP implant pulses (3 uses), the smoke implant puffs bad smoke (3 uses), freedom takes
//!   off cuffs that take at most 3 minutes to break (4 uses), the microbomb asks for a
//!   confirmation and explodes, the recall implant brings its host to its abductor team's pad
//!   (once a minute). A bomb also arms on death (the deniability one also below the crit
//!   threshold, and keeps its host out of crit); a timed one beeps (`timer.ogg`), tells the
//!   ghosts, paralyzes a quarter in (not the deniability one, whose beeps speed up) and explodes;
//!   the fake macrobomb honks first. A chem implant injects everything on death ([`chem_inject`]
//!   is the management console's injection). The CQC implant's button teaches or forgets CQC.
//! - Passive implants: firearms authentication (the syndicate firing pin, `crate::guns`), the
//!   sad trombone (a sound on death), the death alarm (the host's death and area on the
//!   command, medical and security channels), the deathrattle (the death and area to the other
//!   hosts of its group), the EMP absorber (`EMP_PROTECT_ALL`), the exile implant (the home
//!   gateway and the mining shuttle consoles refuse its host; the anti-teleportation one adds
//!   `crate::teleport::NoTeleport`), the bluespace grounding implant (teleports and jaunts fail
//!   and paralyze: [`teleport_blocked`], [`jaunt_blocked`]), the beacon (a teleporter and
//!   prisoner console target), the spell implant (its spell, robeless; the base type is inert)
//!   and the TRAC implant (gone after 5 minutes).
//! - [`remove_implant`] is tg `removed()` (surgery and the console use it); the mindshield's
//!   removal tells the mob unless dead.
// shortcut: no implant entities (no implant item to drop, no `ABSTRACT` item in contents), so no
// storage implant (its bluespace pocket lives on the implant), internal radio implant or
// tactical map; an implanter or case of a type the port has no implant for is empty; no
// renaming with a pen, no signaler sync of the chem implant, no admin logs; the ghosts get a
// chat line instead of a `notify_ghosts` alert.
// shortcut: the chem implant's injection does not expose blood reagents
// (`crate::medical::blood::expose_inject`): an implant full of blood only adds blood reagent.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmHandcuffs, DmItem};
use ss13_reagents::{Holder, Owner, flags};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems};
use ss13_tgui::server::{UiActed, UiSystems};

use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::chem::Reagents;
use crate::construction::Job;
use crate::health::{Health, HealthSystems, Stat};
use crate::inventory::Inventory;
use crate::medical::{name_of, visible};
use crate::server::PlayerMob;
use crate::status_effects::Incap;
use crate::tools::{Actor, DoAfter, do_after, tell};

pub mod chair;
pub mod pad;

pub const IMPLANTER_ROOT: &str = "/obj/item/implanter";
pub const CASE_ROOT: &str = "/obj/item/implantcase";
pub const PAD_ROOT: &str = "/obj/item/implantpad";
pub const MINDSHIELD: &str = "/obj/item/implant/mindshield";
pub const TRACKING: &str = "/obj/item/implant/tracking";
pub const CHEM: &str = "/obj/item/implant/chem";
pub const MICROBOMB: &str = "/obj/item/implant/explosive";
pub const MACROBOMB: &str = "/obj/item/implant/explosive/macro";
pub const EMP: &str = "/obj/item/implant/emp";
pub const FREEDOM: &str = "/obj/item/implant/freedom";
pub const WEAPONS_AUTH: &str = "/obj/item/implant/weapons_auth";
pub const SAD_TROMBONE: &str = "/obj/item/implant/sad_trombone";
/// BandaStation `modular_bandastation/implants/code/implant_cqc.dm`.
pub const CQC: &str = "/obj/item/implant/cqc";
/// BandaStation `modular_bandastation/objects/code/items/implants/death_alarm.dm`.
pub const DEATH_ALARM: &str = "/obj/item/implant/death_alarm";
/// BandaStation `modular_bandastation/objects/code/items/implants/empprotection.dm`.
pub const EMP_PROTECTION: &str = "/obj/item/implant/empprotection";
/// The MOD pathfinder module's implant (`crate::modsuit::pathfinder`).
pub const MOD_PATHFINDER: &str = "/obj/item/implant/mod";
/// `security/implant_exile.dm`: the exile implant and its anti-teleportation subtype.
pub const EXILE: &str = "/obj/item/implant/exile";
pub const NOTELEPORT: &str = "/obj/item/implant/exile/noteleport";
/// `security/implant_noteleport.dm`.
pub const TELEPORT_BLOCKER: &str = "/obj/item/implant/teleport_blocker";
/// `security/implant_beacon.dm`.
pub const BEACON: &str = "/obj/item/implant/beacon";
/// `implant_spell.dm`.
pub const SPELL: &str = "/obj/item/implant/spell";
/// `implant_abductor.dm`: the abductor scientist's recall implant.
pub const ABDUCTOR: &str = "/obj/item/implant/abductor";
/// `implant_misc.dm`.
pub const SMOKE: &str = "/obj/item/implant/smoke";
/// `implant_track.dm`: the .38 TRAC bullet's tracking implant.
pub const TRAC: &str = "/obj/item/implant/tracking/c38";
/// `implant_explosive.dm`.
pub const DENIABILITY: &str = "/obj/item/implant/explosive/deniability";
pub const FAKE_MACROBOMB: &str = "/obj/item/implant/explosive/macro/fake";
/// `implant_deathrattle.dm`.
pub const DEATHRATTLE: &str = "/obj/item/implant/deathrattle";
pub const DEATHRATTLE_LAVALAND: &str = "/obj/item/implant/deathrattle/lavaland";
/// The beacon's and the tracking implant's `lifespan_postmortem`.
pub const POSTMORTEM_SECS: f64 = 10.0 * 60.0;
/// The TRAC implant's `lifespan`.
const TRAC_LIFESPAN_SECS: f64 = 5.0 * 60.0;
/// The teleport blocker's `Paralyze(5 SECONDS)`.
const BLOCKER_PARALYZE_SECS: f32 = 5.0;
/// The recall implant's `cooldown`.
const RECALL_COOLDOWN_SECS: f64 = 60.0;
/// The smoke implant's `uses` and `do_smoke(6, ...)`.
const SMOKE_USES: i32 = 3;
const SMOKE_RANGE: u32 = 6;
/// The deniability implant's `delay`.
const DENIABILITY_DELAY: f32 = 10.0;
/// `timed_explosion`'s beeps.
const TIMER_SOUND: &str = "sound/items/timer.ogg";
const KNELLS: [&str; 4] = [
    "sound/items/knell/knell1.ogg",
    "sound/items/knell/knell2.ogg",
    "sound/items/knell/knell3.ogg",
    "sound/items/knell/knell4.ogg",
];
/// The sad trombone's `death_noise`.
const SAD_TROMBONE_SOUND: &str = "sound/misc/sadtrombone.ogg";
/// The death alarm's `channels_for_alert`.
const DEATH_ALARM_FREQS: [u16; 3] = [
    ss13_chat::radio::FREQ_COMMAND,
    ss13_chat::radio::FREQ_MEDICAL,
    ss13_chat::radio::FREQ_SECURITY,
];
/// `implanter/attack`: `do_after(user, 5 SECONDS, target)` on someone else, deciseconds.
const IMPLANT_DELAY_DS: f32 = 50.0;
/// `SECURITY_IMPLANT_CAP`.
pub const SECURITY_IMPLANT_CAP: usize = 2;
/// `FREEDOM_IMPLANT_CHARGES`; the freedom implant's `resist_strength` (3 minutes), deciseconds.
const FREEDOM_CHARGES: i32 = 4;
const FREEDOM_RESIST_DS: f32 = 1800.0;
/// The EMP implant's `uses` and `empulse(imp_in, 3, 5)`.
const EMP_USES: i32 = 3;
const EMP_HEAVY: i32 = 3;
const EMP_LIGHT: i32 = 5;
/// `MICROBOMB_DELAY` (seconds) and `MICROBOMB_EXPLOSION_DEVASTATE`/`_HEAVY`/`_LIGHT`.
pub const MICROBOMB_DELAY: f32 = 0.7;
const MICROBOMB_BLAST: [f32; 3] = [0.4, 0.8, 2.0];
/// `merge_implants`: `min(delay + other.delay, 30 SECONDS)`.
const MAX_BOMB_DELAY: f32 = 30.0;
/// `timed_explosion`: `Paralyze(14 SECONDS)` a quarter of the delay in.
const BOMB_PARALYZE_SECS: f32 = 14.0;
/// The chem implant's `create_reagents(50, OPENCONTAINER)`.
pub const CHEM_VOLUME: f32 = 50.0;
const IMPLANTS_DMI: &str = "icons/hud/implants.dmi";
const BUTTON: &str = "implant:";
const SYRINGE_DMI: &str = "icons/obj/medical/syringe.dmi";

/// The implants the port has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImplantKind {
    Mindshield,
    Tracking,
    Chem,
    /// The microbomb and the macrobomb (ten microbombs in one).
    Explosive,
    Emp,
    Freedom,
    /// `weapons_auth`: the syndicate firing pin's `req_implant`.
    WeaponsAuth,
    /// `sad_trombone`: a sad trombone on the deathgasp.
    SadTrombone,
    /// BandaStation `cqc`: its button teaches or forgets CQC.
    Cqc,
    /// BandaStation `death_alarm`: the host's death on the command, medical and security
    /// channels.
    DeathAlarm,
    /// BandaStation `empprotection`: `EMP_PROTECT_ALL` on the host.
    EmpProtection,
    /// `/obj/item/implant/mod`: its button recalls the linked MODsuit
    /// (`crate::modsuit::pathfinder`).
    ModPathfinder,
    /// `exile`: refused by the home gateway and the mining shuttle consoles; the `noteleport`
    /// subtype also gives [`crate::teleport::NoTeleport`].
    Exile,
    /// `teleport_blocker`: teleports and jaunts fail and paralyze.
    TeleportBlocker,
    /// `beacon`: a teleporter target.
    Beacon,
    /// `spell`: grants its spell (the base type has none: inert).
    Spell,
    /// `abductor`: its button recalls the host onto its team's pad.
    Abductor,
    Smoke,
    /// `deathrattle`: the deaths of its group's other hosts.
    Deathrattle,
}

impl ImplantKind {
    /// The kind of the implant type `path` (most specific first).
    pub fn of(path: &str) -> Option<Self> {
        [
            (MINDSHIELD, Self::Mindshield),
            (TRACKING, Self::Tracking),
            (CHEM, Self::Chem),
            (EMP, Self::Emp),
            (FREEDOM, Self::Freedom),
            (WEAPONS_AUTH, Self::WeaponsAuth),
            (SAD_TROMBONE, Self::SadTrombone),
            (CQC, Self::Cqc),
            (DEATH_ALARM, Self::DeathAlarm),
            (EMP_PROTECTION, Self::EmpProtection),
            (MOD_PATHFINDER, Self::ModPathfinder),
            (EXILE, Self::Exile),
            (TELEPORT_BLOCKER, Self::TeleportBlocker),
            (BEACON, Self::Beacon),
            (SPELL, Self::Spell),
            (ABDUCTOR, Self::Abductor),
            (SMOKE, Self::Smoke),
            (DEATHRATTLE, Self::Deathrattle),
        ]
        .into_iter()
        .find(|(root, _)| is_under(path, root))
        .map(|(_, kind)| kind)
        .or_else(|| {
            // Not the deathmatch and battle royale bombs (their minigames are not ported).
            [MICROBOMB, MACROBOMB, FAKE_MACROBOMB, DENIABILITY]
                .contains(&path)
                .then_some(Self::Explosive)
        })
    }

    /// Has the activation button (`actions_types`).
    fn activated(self) -> bool {
        matches!(
            self,
            Self::Explosive
                | Self::Emp
                | Self::Freedom
                | Self::Cqc
                | Self::ModPathfinder
                | Self::Abductor
                | Self::Smoke
        )
    }

    /// `implant_color` of its case.
    fn color(self) -> &'static str {
        match self {
            Self::Explosive | Self::Emp | Self::Freedom => "r",
            _ => "b",
        }
    }

    /// `implant_info` (the implant pad).
    pub fn info(self) -> &'static str {
        match self {
            Self::Mindshield => "Срабатывает при имплантации. Защищает от промывки мозгов.",
            Self::Tracking => {
                "Срабатывает при имплантации. Маячок слежения и односторонняя связь для \
                 консоли управления заключёнными. Отключается через десять минут после \
                 смерти носителя."
            }
            Self::Chem => {
                "Заполняется шприцем, пока лежит в кейсе. Срабатывает при имплантации. \
                 Позволяет удалённо вводить реагенты носителю."
            }
            Self::Explosive => {
                "Срабатывает при смерти или по сигналу. Подрывает микровзрывчатку. Может \
                 синхронизироваться с другими микробомбами для увеличения мощности."
            }
            Self::Emp => "Активируется вручную. Создаёт электромагнитный импульс средней силы.",
            Self::Freedom => {
                "Активируется вручную. Снимает оковы с рук и ног, но не смирительные рубашки."
            }
            Self::WeaponsAuth => {
                "Срабатывает при имплантации. Подтверждает личность для оружия с подходящей \
                 системой защиты, обычно в ударнике."
            }
            Self::SadTrombone => "Срабатывает при смерти. Играет грустный тромбон.",
            Self::Cqc => {
                "Обучает даже самого безнадёжного носителя навыкам боевого искусства Close \
                 Quarters Combat."
            }
            Self::DeathAlarm => {
                "Автоматически отправляет сигнал тревоги в случае фатального состояния \
                 пользователя. Сигнал будет содержать информацию о возможном местоположении \
                 пользователя."
            }
            Self::EmpProtection => {
                "Обеспечивает защиту нейроинтерфейсов и встроенных компонентов пользователя от \
                 электромагнитных импульсов высокой мощности."
            }
            Self::ModPathfinder => {
                "Активируется вручную. Позволяет носителю импланта в любой момент вызвать \
                 модульный костюм."
            }
            Self::Exile => {
                "Срабатывает при имплантации. Не даёт вернуться через врата Nanotrasen и \
                 блокирует пульты шахтёрских шаттлов Nanotrasen."
            }
            Self::TeleportBlocker => {
                "Срабатывает при имплантации. Заземляет блюспейс-сигнатуру носителя, не давая \
                 ему телепортироваться или уходить в астрал."
            }
            Self::Beacon => {
                "Срабатывает при имплантации. Служит блюспейс-маячком только для телепортации. \
                 Отключается через десять минут после смерти носителя, но остаётся в теле."
            }
            Self::Spell => {
                "Срабатывает при имплантации. Если бы не был инертен, теоретически давал бы \
                 способность к заклинаниям. К сожалению, инертен."
            }
            Self::Abductor => "Возвращает носителя на материнский корабль.",
            Self::Smoke => {
                "Активируется вручную. Создаёт облако дыма среднего размера при срабатывании."
            }
            Self::Deathrattle => {
                "Требует настройки перед имплантацией. Срабатывает при имплантации. Сообщает \
                 носителю о смертях других носителей имплантов той же группы."
            }
        }
    }
}

/// One implant (tg `/obj/item/implant` vars).
#[derive(Clone, Debug, PartialEq)]
pub struct Implant {
    pub kind: ImplantKind,
    /// The implant type.
    pub path: String,
    pub name: String,
    /// `uses`; -1 is unlimited.
    pub uses: i32,
    /// The type's `initial(uses)`.
    pub initial_uses: i32,
    /// A chem implant's reagents outside a case (in an implanter or a mob).
    pub reagents: Option<Holder>,
    /// Explosives: `explosion_devastate`, `explosion_heavy`, `explosion_light`.
    pub blast: [f32; 3],
    /// Explosives: `delay`, seconds.
    pub delay: f32,
    /// Explosives: `active` (armed).
    pub active: bool,
    /// The spell implant's `spell_to_give` (robeless, `make_robeless`); `None`: inert.
    pub spell: Option<crate::spells::Spell>,
    /// The spell implant taught its spell (a host that already knew it keeps its own).
    pub granted: bool,
    /// The recall implant's `on_cooldown` end, `Time::elapsed_secs_f64`.
    pub ready_at: f64,
    /// The deathrattle's `current_group` ([`DeathrattleGroups`]).
    pub group: Option<u32>,
    /// Tells the implant's button apart in its mob.
    pub id: u32,
}

impl Implant {
    /// `new imp_type()`.
    pub fn new(path: &str, id: u32) -> Option<Self> {
        let kind = ImplantKind::of(path)?;
        let (name, uses) = match kind {
            ImplantKind::Mindshield => ("имплант защиты разума", -1),
            ImplantKind::Tracking if path == TRAC => ("TRAC имплант", -1),
            ImplantKind::Tracking => ("имплант слежения", -1),
            ImplantKind::Chem => ("химический имплант", -1),
            ImplantKind::Explosive if path == DENIABILITY => ("имплант тактического отрицания", -1),
            // The fake macrobomb looks like the real one.
            ImplantKind::Explosive if path == MACROBOMB || path == FAKE_MACROBOMB => {
                ("имплант-макробомба", -1)
            }
            ImplantKind::Explosive => ("имплант-микробомба", -1),
            ImplantKind::Exile if is_under(path, NOTELEPORT) => ("имплант анти-телепортации", -1),
            ImplantKind::Exile => ("имплант изгнания", -1),
            ImplantKind::TeleportBlocker => ("имплант блюспейс-заземления", -1),
            ImplantKind::Beacon => ("имплант-маячок", -1),
            ImplantKind::Spell => ("имплант заклинания", -1),
            ImplantKind::Abductor => ("имплант возврата", -1),
            ImplantKind::Smoke => ("дымовой имплант", SMOKE_USES),
            ImplantKind::Deathrattle if path == DEATHRATTLE_LAVALAND => {
                ("экспедиционный имплант предсмертного хрипа", -1)
            }
            ImplantKind::Deathrattle => ("имплант предсмертного хрипа", -1),
            ImplantKind::Emp => ("ЭМИ-имплант", EMP_USES),
            ImplantKind::Freedom => ("имплант свободы", FREEDOM_CHARGES),
            ImplantKind::WeaponsAuth => ("имплант авторизации оружия", -1),
            ImplantKind::SadTrombone => ("имплант грустного тромбона", -1),
            ImplantKind::Cqc => ("CQC имплант", -1),
            ImplantKind::DeathAlarm => ("имплант оповещения о смерти", -1),
            ImplantKind::EmpProtection => ("поглотитель ЭМИ", -1),
            ImplantKind::ModPathfinder => ("имплант МОД-навигатора", -1),
        };
        let scale = if path == MACROBOMB { 10.0 } else { 1.0 };
        let delay = match path {
            MACROBOMB | FAKE_MACROBOMB => MICROBOMB_DELAY * 10.0,
            DENIABILITY => DENIABILITY_DELAY,
            _ => MICROBOMB_DELAY,
        };
        Some(Self {
            kind,
            path: path.to_owned(),
            name: name.to_owned(),
            uses,
            initial_uses: uses,
            reagents: (kind == ImplantKind::Chem).then(chem_holder),
            blast: MICROBOMB_BLAST.map(|b| b * scale),
            delay,
            active: false,
            spell: None,
            granted: false,
            ready_at: 0.0,
            group: None,
            id,
        })
    }

    /// `IMPLANT_TYPE_SECURITY` (not the TRAC implant's).
    pub fn security(&self) -> bool {
        match self.kind {
            ImplantKind::Tracking => self.path != TRAC,
            ImplantKind::Chem
            | ImplantKind::Exile
            | ImplantKind::TeleportBlocker
            | ImplantKind::Beacon => true,
            _ => false,
        }
    }

    /// `implant_info` of the type (the implant pad).
    pub fn info(&self) -> String {
        match self.path.as_str() {
            NOTELEPORT => "Срабатывает при имплантации. Повторяет имплант изгнания (не даёт \
                           вернуться через врата и блокирует пульты шахтёрских шаттлов), а также \
                           не даёт телепортироваться."
                .to_owned(),
            TRAC => "Срабатывает при имплантации, обычно выстрелом. Маячок слежения и \
                     односторонняя связь для консоли управления заключёнными. Растворяется через \
                     5 минут."
                .to_owned(),
            DENIABILITY => format!(
                "{} Пока имплантирован, не даёт потерять сознание от тяжёлых травм. Не \
                 защищает от смерти и оглушения. Откладывает подрыв других микробомб на десять \
                 секунд.",
                self.kind.info()
            ),
            DEATHRATTLE_LAVALAND => format!(
                "СРАБАТЫВАЕТ ТОЛЬКО ПРИ СМЕРТИ ВНЕ СТАНЦИИ. {}",
                self.kind.info()
            ),
            _ if self.kind == ImplantKind::Spell => match self.spell {
                Some(spell) => format!(
                    "Срабатывает при имплантации. Позволяет носителю применять {} без \
                     подходящего облачения волшебника.",
                    spell.def().name
                ),
                None => self.kind.info().to_owned(),
            },
            _ => self.kind.info().to_owned(),
        }
    }

    /// The deniability implant's `master_implant`, `panic_beep_sound`, `no_paralyze`.
    fn deniability(&self) -> bool {
        self.path == DENIABILITY
    }
}

/// `create_reagents(50, OPENCONTAINER)`.
fn chem_holder() -> Holder {
    Holder::new(
        CHEM_VOLUME,
        flags::OPENCONTAINER,
        Owner {
            path: CHEM.to_owned(),
            living: false,
        },
    )
}

/// A mob's implants (tg `implants`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Implants(pub Vec<Implant>);

/// An implanter's or a case's `imp`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ImplantHolder {
    pub imp: Option<Implant>,
    /// An implant case (else an implanter).
    pub case: bool,
}

/// Hands out [`Implant::id`]s.
#[derive(Resource, Default, Debug)]
struct NextImplantId(u32);

/// A microbomb counting down on its mob (`timed_explosion`): the time it paralyzes (if not
/// yet) and explodes, and its next `timer.ogg` beep.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Detonation {
    pub implant: u32,
    pub paralyze_at: Option<f64>,
    pub at: f64,
    pub next_beep: Option<f64>,
    /// The time to the beep after `next_beep`: a quarter of the delay, or (`panic_beep_sound`)
    /// a fifth of what is left, shrinking each beep.
    pub step: f64,
    pub volume: f32,
    pub panic: bool,
}

/// The TRAC implant's `QDEL_IN_STOPPABLE(src, lifespan)`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct TracExpiry {
    pub implant: u32,
    pub at: f64,
}

/// The deathrattle groups (`/datum/deathrattle_group`): their names and whether a group only
/// reports deaths in the wastes (`/datum/deathrattle_group/lavaland`).
#[derive(Resource, Default, Debug)]
pub struct DeathrattleGroups {
    pub groups: Vec<DeathrattleGroup>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeathrattleGroup {
    pub id: u32,
    pub name: String,
    pub lavaland: bool,
}

impl DeathrattleGroups {
    pub fn get(&self, id: u32) -> Option<&DeathrattleGroup> {
        self.groups.iter().find(|g| g.id == id)
    }
}

/// The microbomb's "Are you sure?" prompt (`tgui_alert` of `activate("action_button")`).
#[derive(Component, Clone, Copy, Debug)]
struct BombConfirm {
    mob: Entity,
    implant: u32,
}

pub struct ImplantsPlugin;

impl Plugin for ImplantsPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // An implanter and an implant case in the hands swap the implant.
        app.add_click_handler(ClickPhase::OtherHand, 600, |w, c| {
            c.held
                .is_some_and(|h| crate::implants::item_interaction(w, c.client, c.mob, h, c.target))
        });
        // An implanter on an implant case, a case on the implant pad.
        app.add_click_handler(ClickPhase::HeldOnTarget, 700, |w, c| {
            c.held
                .is_some_and(|h| crate::implants::item_interaction(w, c.client, c.mob, h, c.target))
        });
        app.init_resource::<NextImplantId>()
            .init_resource::<DeathrattleGroups>()
            .add_observer(add_holder_state)
            .add_systems(
                Update,
                (
                    implant_clicks
                        .after(crate::security::security_clicks)
                        .before(crate::combat::MeleeSystems),
                    on_death.after(HealthSystems),
                    deniability_crit.after(HealthSystems),
                    run_detonations.run_if(any_with_component::<Detonation>),
                    expire_trac.run_if(any_with_component::<TracExpiry>),
                    trigger_buttons.in_set(UiSystems::Handle),
                    bomb_confirmed
                        .in_set(UiSystems::Handle)
                        .after(PromptSystems::Answer),
                ),
            );
        chair::build(app);
        pad::build(app);
    }
}

pub(crate) fn next_id(world: &mut World) -> u32 {
    let mut next = world.get_resource_or_insert_with(NextImplantId::default);
    next.0 += 1;
    next.0
}

/// The implant type an implanter or case of `path` starts with (tg `imp_type`).
pub fn imp_type(path: &str) -> Option<&'static str> {
    const TYPES: &[(&str, &str)] = &[
        ("/obj/item/implanter/mindshield", MINDSHIELD),
        ("/obj/item/implanter/tracking", TRACKING),
        ("/obj/item/implanter/explosive", MICROBOMB),
        ("/obj/item/implanter/explosive_macro", MACROBOMB),
        ("/obj/item/implanter/emp", EMP),
        ("/obj/item/implanter/freedom", FREEDOM),
        ("/obj/item/implantcase/mindshield", MINDSHIELD),
        ("/obj/item/implantcase/tracking", TRACKING),
        ("/obj/item/implantcase/chem", CHEM),
        ("/obj/item/implantcase/explosive", MICROBOMB),
        ("/obj/item/implantcase/freedom", FREEDOM),
        ("/obj/item/implantcase/weapons_auth", WEAPONS_AUTH),
        ("/obj/item/implanter/sad_trombone", SAD_TROMBONE),
        ("/obj/item/implantcase/sad_trombone", SAD_TROMBONE),
        ("/obj/item/implanter/cqc", CQC),
        ("/obj/item/implantcase/cqc", CQC),
        ("/obj/item/implanter/death_alarm", DEATH_ALARM),
        ("/obj/item/implantcase/death_alarm", DEATH_ALARM),
        ("/obj/item/implanter/empprotection", EMP_PROTECTION),
        ("/obj/item/implantcase/empprotection", EMP_PROTECTION),
        ("/obj/item/implanter/exile", EXILE),
        ("/obj/item/implanter/exile/noteleport", NOTELEPORT),
        ("/obj/item/implantcase/exile", EXILE),
        ("/obj/item/implantcase/exile/noteleport", NOTELEPORT),
        ("/obj/item/implantcase/teleport_blocker", TELEPORT_BLOCKER),
        ("/obj/item/implanter/beacon", BEACON),
        ("/obj/item/implantcase/beacon", BEACON),
        ("/obj/item/implanter/spell", SPELL),
        ("/obj/item/implantcase/spell", SPELL),
        ("/obj/item/implanter/smoke", SMOKE),
        ("/obj/item/implanter/explosive_macro/fake", FAKE_MACROBOMB),
        ("/obj/item/implanter/tactical_deniability", DENIABILITY),
        ("/obj/item/implantcase/deathrattle", DEATHRATTLE),
        (
            "/obj/item/implantcase/deathrattle/lavaland",
            DEATHRATTLE_LAVALAND,
        ),
    ];
    // Exact types: `/obj/item/implanter/tracking/gps` holds a GPS (tg cannot implant it).
    TYPES.iter().find(|(p, _)| *p == path).map(|(_, imp)| *imp)
}

/// `implanter/Initialize` and `implantcase/Initialize`: the `imp_type` implant inside.
fn add_holder_state(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    let case = is_under(&item.path, CASE_ROOT);
    if !case && !is_under(&item.path, IMPLANTER_ROOT) {
        return;
    }
    let (entity, path) = (add.entity, item.path.clone());
    commands.queue(move |world: &mut World| {
        if world.get_entity(entity).is_err() || world.get::<ImplantHolder>(entity).is_some() {
            return;
        }
        let imp = match imp_type(&path) {
            Some(imp) => {
                let id = next_id(world);
                Implant::new(imp, id)
            }
            None => None,
        };
        world
            .entity_mut(entity)
            .insert(ImplantHolder { imp: None, case });
        put_in_holder(world, entity, imp);
    });
}

/// Puts `imp` into the holder (replacing what it held), with the case's reagents and sprite.
fn put_in_holder(world: &mut World, holder: Entity, mut imp: Option<Implant>) {
    let case = world.get::<ImplantHolder>(holder).is_some_and(|h| h.case);
    if case {
        match imp.as_mut().and_then(|i| i.reagents.take()) {
            Some(reagents) => {
                world.entity_mut(holder).insert(Reagents(reagents));
            }
            None => {
                world.entity_mut(holder).remove::<Reagents>();
            }
        }
    }
    if let Some(mut h) = world.get_mut::<ImplantHolder>(holder) {
        h.imp = imp;
    }
    refresh_holder_look(world, holder);
}

/// Takes the holder's implant out, with a case's reagents.
fn take_from_holder(world: &mut World, holder: Entity) -> Option<Implant> {
    let mut imp = world.get_mut::<ImplantHolder>(holder)?.imp.take()?;
    if imp.kind == ImplantKind::Chem
        && let Some(Reagents(reagents)) = world.entity_mut(holder).take::<Reagents>()
    {
        imp.reagents = Some(reagents);
    }
    refresh_holder_look(world, holder);
    Some(imp)
}

/// `implanter/update_icon_state` (`implanter0`/`implanter1`), `implantcase/update_icon_state`
/// (`implantcase-0`, `-b`, `-r`).
fn refresh_holder_look(world: &mut World, holder: Entity) {
    let Some(h) = world.get::<ImplantHolder>(holder) else {
        return;
    };
    let state = match (h.case, &h.imp) {
        (false, imp) => format!("implanter{}", u8::from(imp.is_some())),
        (true, Some(imp)) => format!("implantcase-{}", imp.kind.color()),
        (true, None) => "implantcase-0".to_owned(),
    };
    let dmi = world
        .get::<DmAtom>(holder)
        .map(|a| a.icon.clone())
        .filter(|i| !i.is_empty())
        .unwrap_or_else(|| SYRINGE_DMI.to_owned());
    let id = world
        .get_resource::<IconIndex>()
        .and_then(|icons| icons.get(&dmi, &state));
    if let (Some(id), Some(mut look)) = (id, world.get_mut::<Appearance>(holder)) {
        look.icon_state = id;
    }
}

use crate::tools::tell_mob;

// ---------------------------------------------------------------------------------------------
// Implanting

/// An implanter's click on a living mob: `implanter/attack`.
pub fn implant_clicks(
    world: &mut World,
    mut clicks: Local<MessageCursor<crate::click::ClickResolved>>,
) {
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
        let Some(holder) = world.get::<ImplantHolder>(held) else {
            continue;
        };
        if holder.case {
            continue;
        }
        let loaded = holder.imp.is_some();
        world
            .resource_mut::<crate::security::MeleeHandled>()
            .0
            .insert(mob);
        if !loaded {
            continue;
        }
        let delay = if target == mob {
            0.0
        } else {
            let (user, victim) = (name_of(world, mob), name_of(world, target));
            let text = format!("{user} пытается имплантировать {victim}.");
            visible(world, target, &text, &text, Span::Warning);
            IMPLANT_DELAY_DS
        };
        let actor = Actor {
            client: click.client,
            mob,
            item: Some(held),
        };
        do_after(world, actor, target, delay, 0, Job::Implant);
    }
}

/// The implanter's `do_after` ended: `imp.implant(target, user)`.
pub fn complete_implant(world: &mut World, user: Entity, action: &DoAfter) {
    let target = action.target;
    let Some(implanter) = action.item else {
        return;
    };
    if world
        .get::<ImplantHolder>(implanter)
        .is_none_or(|h| h.case || h.imp.is_none())
        || world.get::<Health>(target).is_none()
    {
        return;
    }
    let imp = take_from_holder(world, implanter).expect("checked above");
    let (user_name, name) = (name_of(world, user), name_of(world, target));
    let implanter_name = name_of(world, implanter);
    match implant(world, target, Some(user), imp) {
        Ok(()) => {
            if target == user {
                tell(world, action.client, Span::Notice, "Вы имплантируете себя.");
            } else {
                visible(
                    world,
                    target,
                    &format!("{user_name} имплантирует вас."),
                    &format!("{user_name} имплантирует {name}."),
                    Span::Notice,
                );
            }
        }
        Err((imp, reason)) => {
            put_in_holder(world, implanter, Some(imp));
            let text = reason
                .unwrap_or_else(|| format!("{implanter_name} не удаётся имплантировать {name}."));
            tell(world, action.client, Span::Warning, text);
        }
    }
}

/// tg `can_be_implanted_in`: living non-silicons; a deathrattle goes into any living mob once
/// it has a group.
fn can_be_implanted_in(world: &World, target: Entity, imp: &Implant) -> bool {
    if world.get::<Health>(target).is_none() {
        return false;
    }
    if imp.kind == ImplantKind::Deathrattle {
        return imp.group.is_some();
    }
    world.get::<crate::silicon::Silicon>(target).is_none()
}

/// `explosive/merge_implants(kept, stat)`.
fn merge_bombs(kept: &mut Implant, other: &Implant) {
    for (k, added) in kept.blast.iter_mut().zip(other.blast) {
        *k += added;
    }
    kept.delay = (kept.delay + other.delay).min(MAX_BOMB_DELAY);
}

/// tg `implant(target, user)`: Ok when the implant went in (or was used up merging into one
/// there); else the implant back with the refusal's text, if any.
#[allow(clippy::result_large_err)]
pub fn implant(
    world: &mut World,
    target: Entity,
    user: Option<Entity>,
    mut imp: Implant,
) -> Result<(), (Implant, Option<String>)> {
    if !can_be_implanted_in(world, target, &imp) {
        let reason = (imp.kind == ImplantKind::Deathrattle && imp.group.is_none())
            .then(|| "Имплант предсмертного хрипа требует настройки!".to_owned());
        return Err((imp, reason));
    }
    let mut implants = world.get::<Implants>(target).cloned().unwrap_or_default();
    // `explosive/implant`: a bomb merges into one already there, a master implant (the
    // deniability one) takes the other in instead; two masters do not fit.
    if imp.kind == ImplantKind::Explosive
        && let Some(index) = implants
            .0
            .iter()
            .position(|i| i.kind == ImplantKind::Explosive)
    {
        if imp.deniability() && implants.0[index].deniability() {
            return Err((imp, Some("Имплант не помещается!".to_owned())));
        }
        if imp.deniability() {
            let other = implants.0.remove(index);
            merge_bombs(&mut imp, &other);
            world.entity_mut(target).remove::<Detonation>();
        } else {
            merge_bombs(&mut implants.0[index], &imp);
            world.entity_mut(target).insert(implants);
            return Ok(());
        }
    }
    // `freedom/implant`: only carbons can be cuffed.
    if imp.kind == ImplantKind::Freedom && world.get::<crate::mobs::BasicMob>(target).is_some() {
        return Err((imp, Some("Это было бы пустой тратой!".to_owned())));
    }
    // tg counts the security implants already there for any new implant, so two of them block
    // every other implant too; the cap is meant for security implants (deviation).
    let security = implants.0.iter().filter(|i| i.security()).count();
    if imp.security() && security >= SECURITY_IMPLANT_CAP {
        return Err((imp, Some("Слишком много имплантов охраны!".to_owned())));
    }
    // A duplicate (`!allow_multiple`; the MOD pathfinder's and the deathrattle allow several):
    // its uses top up the old one's, up to twice the initial. tg's `istype(other, type)` counts
    // a subtype as a duplicate too.
    if !matches!(
        imp.kind,
        ImplantKind::ModPathfinder | ImplantKind::Deathrattle
    ) && let Some(other) = implants.0.iter_mut().find(|i| is_under(&i.path, &imp.path))
    {
        if other.uses < other.initial_uses * 2 {
            other.uses = if imp.uses == -1 {
                -1
            } else {
                (other.uses + imp.uses).min(other.initial_uses * 2)
            };
            world.entity_mut(target).insert(implants);
            return Ok(());
        }
        return Err((imp, None));
    }
    // `mindshield/implant`: `COMSIG_PRE_MINDSHIELD_IMPLANT` (a head revolutionary resists and
    // the implant is gone), `COMSIG_MINDSHIELD_IMPLANTED` (deconversion).
    if imp.kind == ImplantKind::Mindshield
        && crate::antag::revolution::server::on_mindshield_implant(world, target, user)
    {
        return Ok(());
    }
    if imp.kind == ImplantKind::Chem && imp.reagents.is_none() {
        imp.reagents = Some(chem_holder());
    }
    let (kind, id, spell) = (imp.kind, imp.id, imp.spell);
    let path = imp.path.clone();
    implants.0.push(imp);
    world.entity_mut(target).insert(implants);
    if kind == ImplantKind::Mindshield {
        world
            .entity_mut(target)
            .insert(crate::antag::revolution::server::Mindshield);
        tell_mob(
            world,
            target,
            Span::Notice,
            "Вы чувствуете умиротворение и безопасность. Теперь вы защищены от промывки мозгов.",
        );
    }
    // `empprotection/implant`: `AddElement(/datum/element/empprotection, EMP_PROTECT_ALL)`.
    if kind == ImplantKind::EmpProtection {
        crate::modsuit::server::set_emp_protection(world, target, crate::emp::PROTECT_ALL, true);
    }
    // `exile/noteleport/implant`: `TRAIT_NO_TELEPORT`.
    if is_under(&path, NOTELEPORT) {
        world.entity_mut(target).insert(crate::teleport::NoTeleport);
    }
    // `spell/implant`: `spell_to_give.Grant(target)`, robeless.
    if let Some(spell) = spell
        && !crate::spells::knows(world, target, spell)
    {
        crate::spells::learn(world, target, spell);
        if let Some(mut spells) = world.get_mut::<crate::spells::Spells>(target)
            && let Some(known) = spells.get_mut(spell)
        {
            known.robeless = true;
        }
        if let Some(mut implants) = world.get_mut::<Implants>(target)
            && let Some(imp) = implants.0.iter_mut().find(|i| i.id == id)
        {
            imp.granted = true;
        }
    }
    // `deniability/implant`: `TRAIT_NOSOFTCRIT`, `TRAIT_NOHARDCRIT`.
    if path == DENIABILITY {
        crate::medical::traumas::refresh(world, target);
    }
    // `tracking/c38/implant`: `QDEL_IN_STOPPABLE(src, lifespan)`.
    if path == TRAC {
        let at = world.resource::<Time>().elapsed_secs_f64() + TRAC_LIFESPAN_SECS;
        world
            .entity_mut(target)
            .insert(TracExpiry { implant: id, at });
    }
    refresh_buttons(world, target);
    Ok(())
}

/// tg `removed(source, silent)`: the implant out of `mob` and its effects gone.
pub fn remove_implant(world: &mut World, mob: Entity, id: u32, silent: bool) -> Option<Implant> {
    let mut implants = world.get_mut::<Implants>(mob)?;
    let index = implants.0.iter().position(|i| i.id == id)?;
    let imp = implants.0.remove(index);
    world.entity_mut(mob).remove::<Detonation>();
    if imp.kind == ImplantKind::Mindshield {
        world
            .entity_mut(mob)
            .remove::<crate::antag::revolution::server::Mindshield>();
        if world.get::<Stat>(mob) != Some(&Stat::Dead) && !silent {
            tell_mob(
                world,
                mob,
                Span::Notice,
                "Ваш разум внезапно чувствует себя ужасно уязвимым. Вы больше не защищены от \
                 промывки мозгов.",
            );
        }
    }
    if imp.kind == ImplantKind::EmpProtection {
        crate::modsuit::server::set_emp_protection(world, mob, crate::emp::PROTECT_ALL, false);
    }
    // The CQC implant's style goes with it (BandaStation keeps it until the removed implant is
    // destroyed; here the removed implant is gone at once).
    if imp.kind == ImplantKind::Cqc {
        crate::martial::unlearn(world, mob, crate::martial::Origin::Implant(imp.id));
    }
    // `exile/noteleport/removed`: the trait goes unless another such implant stays.
    if is_under(&imp.path, NOTELEPORT)
        && !implants_of(world, mob)
            .iter()
            .any(|i| is_under(&i.path, NOTELEPORT))
    {
        world
            .entity_mut(mob)
            .remove::<crate::teleport::NoTeleport>();
    }
    // `spell/removed`: `spell_to_give.Remove(source)`.
    if let Some(spell) = imp.spell.filter(|_| imp.granted) {
        crate::spells::forget(world, mob, spell);
        if world.get::<Stat>(mob) != Some(&Stat::Dead) && !silent {
            tell_mob(
                world,
                mob,
                Span::BoldNotice,
                format!(
                    "Знание о том, как применять {}, ускользает из вашего разума.",
                    spell.def().name
                ),
            );
        }
    }
    if imp.deniability() {
        crate::medical::traumas::refresh(world, mob);
    }
    if world
        .get::<TracExpiry>(mob)
        .is_some_and(|t| t.implant == imp.id)
    {
        world.entity_mut(mob).remove::<TracExpiry>();
    }
    refresh_buttons(world, mob);
    Some(imp)
}

/// The implants of `mob`.
pub fn implants_of(world: &World, mob: Entity) -> &[Implant] {
    world.get::<Implants>(mob).map_or(&[], |i| i.0.as_slice())
}

/// Every implanted mob's implants of `kind` (tg `GLOB.tracked_implants` filtered by type).
pub fn tracked(world: &mut World, kind: ImplantKind) -> Vec<(Entity, u32)> {
    let mut query = world.query::<(Entity, &Implants)>();
    query
        .iter(world)
        .flat_map(|(mob, implants)| {
            implants
                .0
                .iter()
                .filter(move |i| i.kind == kind)
                .map(move |i| (mob, i.id))
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Cases and the pad

/// tg `implantcase/item_interaction` with an implanter, and `implantpad` with a case: true when
/// the click was theirs.
pub fn item_interaction(
    world: &mut World,
    _client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let (Some(tool), Some(other)) = (
        world.get::<ImplantHolder>(held).cloned(),
        world.get::<ImplantHolder>(target).cloned(),
    ) else {
        return pad::insert_case(world, mob, held, target);
    };
    // The implanter on a case (and, as tg's pad is not ported, a case on an implanter).
    let (implanter, case) = match (tool.case, other.case) {
        (false, true) => (held, target),
        (true, false) => (target, held),
        _ => return false,
    };
    let in_implanter = world
        .get::<ImplantHolder>(implanter)
        .is_some_and(|h| h.imp.is_some());
    let in_case = world
        .get::<ImplantHolder>(case)
        .is_some_and(|h| h.imp.is_some());
    if in_implanter && !in_case {
        let imp = take_from_holder(world, implanter);
        put_in_holder(world, case, imp);
    } else if !in_implanter && in_case {
        let imp = take_from_holder(world, case);
        put_in_holder(world, implanter, imp);
    }
    true
}

// ---------------------------------------------------------------------------------------------
// Activation

fn button(world: &World, imp: &Implant) -> (ActionButton, f64) {
    let icon = |dmi: &str, state: &str| {
        world
            .get_resource::<IconIndex>()
            .and_then(|index| index.get(dmi, state))
    };
    let (dmi, state) = match imp.kind {
        ImplantKind::Explosive => (IMPLANTS_DMI, "explosive"),
        ImplantKind::Emp => (IMPLANTS_DMI, "emp"),
        ImplantKind::Freedom => (IMPLANTS_DMI, "freedom"),
        ImplantKind::Smoke => (IMPLANTS_DMI, "smoke"),
        // The recall implant's own `icon`/`icon_state`.
        ImplantKind::Abductor => ("icons/obj/antags/abductor.dmi", "implant"),
        // The CQC implant's own `icon`/`icon_state`.
        ImplantKind::Cqc => ("icons/obj/scrolls.dmi", "scroll2"),
        // `/datum/action/item_action/mod_recall`.
        ImplantKind::ModPathfinder => ("icons/mob/actions/actions_mod.dmi", "recall"),
        _ => (IMPLANTS_DMI, "generic"),
    };
    let name = if imp.kind == ImplantKind::ModPathfinder {
        "Вызвать МОД".to_owned()
    } else {
        format!("Активировать {}", imp.name)
    };
    (
        ActionButton {
            id: format!("{BUTTON}{}", imp.id),
            name,
            icon: icon(dmi, state),
            background: icon("icons/mob/actions/backgrounds.dmi", "bg_default"),
            border: None,
            available: true,
            cooldown: 0.0,
            active: false,
            maptext: None,
        },
        0.0,
    )
}

/// The mob's implant buttons after the others it has (tg `implant_action.Grant`/`Remove`).
fn refresh_buttons(world: &mut World, mob: Entity) {
    if world.get::<PlayerMob>(mob).is_none() {
        return;
    }
    let mut buttons: Vec<(ActionButton, f64)> = world
        .get::<ActionButtons>(mob)
        .map(|b| {
            b.0.iter()
                .filter(|(b, _)| !b.id.starts_with(BUTTON))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let own: Vec<(ActionButton, f64)> = implants_of(world, mob)
        .iter()
        .filter(|i| i.kind.activated())
        .map(|i| button(world, i))
        .collect();
    buttons.extend(own);
    if buttons.is_empty() && world.get::<ActionButtons>(mob).is_none() {
        return;
    }
    set_buttons(world, mob, buttons);
}

/// A click on an implant's button (tg `ui_action_click` -> `activate("action_button")`),
/// checked at the moment of effect: the client controls the mob and the implant is still in it.
fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .filter(|a| a.act.starts_with(BUTTON))
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client) {
            continue;
        }
        let Some(id) = id.strip_prefix(BUTTON).and_then(|n| n.parse::<u32>().ok()) else {
            continue;
        };
        // `item_action_slot_check`/`IsAvailable`: a conscious, unrestrained user.
        if world.get::<Stat>(actor) != Some(&Stat::Conscious) {
            continue;
        }
        let Some(imp) = implants_of(world, actor)
            .iter()
            .find(|i| i.id == id)
            .cloned()
        else {
            continue;
        };
        match imp.kind {
            ImplantKind::Explosive => ask_bomb(world, client, actor, &imp),
            ImplantKind::Emp => activate_emp(world, actor, id),
            ImplantKind::Freedom => activate_freedom(world, actor, id),
            ImplantKind::Cqc => activate_cqc(world, actor, id),
            ImplantKind::ModPathfinder => {
                crate::modsuit::pathfinder::recall_action(world, actor, id)
            }
            ImplantKind::Smoke => activate_smoke(world, actor, id),
            ImplantKind::Abductor => activate_recall(world, actor, id),
            _ => {}
        }
    }
}

/// `smoke/activate`: `do_smoke(6, ...)` of the bad smoke; gone after the last use.
fn activate_smoke(world: &mut World, mob: Entity, id: u32) {
    let Some(pos) = crate::explosions::turf_of(world, mob) else {
        return;
    };
    let Some(left) = use_charge(world, mob, id) else {
        return;
    };
    crate::effects::smoke::do_smoke(
        world,
        SMOKE_RANGE,
        pos,
        None,
        crate::effects::smoke::SmokeKind::Bad,
    );
    if left == 0 {
        remove_implant(world, mob, id, true);
    }
}

/// `abductor/activate`: the host onto its home pad (its team's, else a random ship's), once a
/// minute.
fn activate_recall(world: &mut World, mob: Entity, id: u32) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(ready_at) = implants_of(world, mob)
        .iter()
        .find(|i| i.id == id)
        .map(|i| i.ready_at)
    else {
        return;
    };
    if now < ready_at {
        tell_mob(
            world,
            mob,
            Span::Warning,
            format!(
                "Нужно подождать {:.1} секунд, прежде чем снова использовать имплант возврата!",
                ready_at - now
            ),
        );
        return;
    }
    let Some(pad) = recall_pad(world, mob) else {
        tell_mob(
            world,
            mob,
            Span::Warning,
            "телепорт-площадки не обнаружены!",
        );
        return;
    };
    crate::antag::abductor::server::pad_retrieve(world, pad, mob);
    if let Some(mut implants) = world.get_mut::<Implants>(mob)
        && let Some(imp) = implants.0.iter_mut().find(|i| i.id == id)
    {
        imp.ready_at = now + RECALL_COOLDOWN_SECS;
    }
}

/// `abductor/link_pad`: the pad of the host's abductor team, else the pad of the first ship
/// console found.
// shortcut: the pad is looked up at each use instead of linked once (`home`); tg picks a random
// console where this takes the first.
fn recall_pad(world: &mut World, mob: Entity) -> Option<Entity> {
    use crate::antag::abductor::server::{Abductor, pad_of};
    if let Some(team) = world.get::<Abductor>(mob).map(|a| a.team)
        && let Some(pad) = pad_of(world, team)
    {
        return Some(pad);
    }
    let mut pads =
        world.query_filtered::<Entity, With<crate::antag::abductor::server::AbductorPad>>();
    let mut all: Vec<Entity> = pads.iter(world).collect();
    all.sort();
    all.first().copied()
}

/// `/datum/outfit/abductor/scientist/post_equip`: the scientist's recall implant.
pub fn give_recall(world: &mut World, mob: Entity) {
    let id = next_id(world);
    if let Some(imp) = Implant::new(ABDUCTOR, id) {
        let _ = implant(world, mob, None, imp);
    }
}

/// `emp/activate`: `empulse(imp_in, 3, 5)`; gone after the last use.
fn activate_emp(world: &mut World, mob: Entity, id: u32) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let Some(left) = use_charge(world, mob, id) else {
        return;
    };
    world.write_message(crate::emp::Empulse {
        epicenter: pos,
        heavy: EMP_HEAVY,
        light: EMP_LIGHT,
    });
    if left == 0 {
        remove_implant(world, mob, id, true);
    }
}

/// One use of the implant; the uses left.
fn use_charge(world: &mut World, mob: Entity, id: u32) -> Option<i32> {
    let mut implants = world.get_mut::<Implants>(mob)?;
    let imp = implants.0.iter_mut().find(|i| i.id == id)?;
    if imp.uses > 0 {
        imp.uses -= 1;
    }
    Some(imp.uses)
}

/// `freedom/can_trigger`: cuffs that take at most `resist_strength` to break out of.
pub fn freedom_can_trigger(world: &World, mob: Entity) -> bool {
    world
        .get::<crate::restraints::Handcuffed>(mob)
        .and_then(|c| world.get::<DmHandcuffs>(c.0))
        .is_some_and(|c| c.breakouttime <= FREEDOM_RESIST_DS)
        || freedom_breaks_legcuffs(world, mob)
}

/// `uncuff(resist_strength)` also takes off leg restraints it can break.
fn freedom_breaks_legcuffs(world: &World, mob: Entity) -> bool {
    world
        .get::<crate::legcuffs::Legcuffed>(mob)
        .and_then(|c| world.get::<ss13_mapload::DmLegcuffs>(c.0))
        .is_some_and(|c| c.breakouttime <= FREEDOM_RESIST_DS)
}

/// `freedom/activate`: `uncuff(resist_strength)`; gone after the last use.
fn activate_freedom(world: &mut World, mob: Entity, id: u32) {
    if !freedom_can_trigger(world, mob) {
        tell_mob(world, mob, Span::Warning, "На вас нет оков!");
        return;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let Some(left) = use_charge(world, mob, id) else {
        return;
    };
    if world
        .get::<crate::restraints::Handcuffed>(mob)
        .and_then(|c| world.get::<DmHandcuffs>(c.0))
        .is_some_and(|c| c.breakouttime <= FREEDOM_RESIST_DS)
    {
        crate::restraints::uncuff(world, mob, pos);
    }
    if freedom_breaks_legcuffs(world, mob) {
        crate::legcuffs::remove(world, mob, pos);
    }
    if left == 0 {
        tell_mob(world, mob, Span::Warning, "Имплант деградировал!");
        remove_implant(world, mob, id, true);
    }
}

/// BandaStation `cqc/activate`: forgets the implant's CQC if the host knows it, else teaches it.
fn activate_cqc(world: &mut World, mob: Entity, id: u32) {
    let origin = crate::martial::Origin::Implant(id);
    if crate::martial::unlearn(world, mob, origin) {
        return;
    }
    crate::martial::teach(world, mob, crate::martial::Style::Cqc, origin);
}

/// `explosive/activate("action_button")`: asks first (one prompt at a time, tg `popup`).
fn ask_bomb(world: &mut World, client: Entity, mob: Entity, imp: &Implant) {
    let mut asking = world.query::<&BombConfirm>();
    if imp.active || asking.iter(world).any(|c| c.mob == mob) {
        return;
    }
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        Ask::new(
            client,
            mob,
            format!("Подтверждение: {}", imp.name),
            format!(
                "Вы уверены, что хотите активировать {}? Вы взорвётесь!",
                imp.name
            ),
            PromptKind::Alert {
                buttons: vec!["Да".to_owned(), "Нет".to_owned()],
                autofocus: true,
            },
        ),
    );
    world.entity_mut(prompt).insert(BombConfirm {
        mob,
        implant: imp.id,
    });
}

/// The answer, re-checked: the implant is still in the conscious mob.
fn bomb_confirmed(world: &mut World, mut answers: Local<MessageCursor<PromptAnswered>>) {
    let pending: Vec<(Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.answer.clone()))
        .collect();
    for (prompt, answer) in pending {
        let Some(confirm) = world.get::<BombConfirm>(prompt).copied() else {
            continue;
        };
        if answer != Some(PromptAnswer::Button(0))
            || world.get::<Stat>(confirm.mob) == Some(&Stat::Dead)
        {
            continue;
        }
        activate_bomb(world, confirm.mob, confirm.implant, true);
    }
}

/// `explosive/activate(cause)`: arms the bomb; at most [`MICROBOMB_DELAY`] it explodes at once,
/// else it beeps, paralyzes its host a quarter of the delay in and explodes at the delay.
pub fn activate_bomb(world: &mut World, mob: Entity, id: u32, announce: bool) {
    let Some(mut implants) = world.get_mut::<Implants>(mob) else {
        return;
    };
    let Some(imp) = implants.0.iter_mut().find(|i| i.id == id) else {
        return;
    };
    if imp.active {
        return;
    }
    imp.active = true;
    let (name, delay, panic) = (imp.name.clone(), imp.delay, imp.deniability());
    if announce {
        tell_mob(world, mob, Span::Notice, format!("Вы активируете {name}."));
    }
    if delay <= MICROBOMB_DELAY {
        explode(world, mob, id);
        return;
    }
    let who = name_of(world, mob);
    let text = format!("{who} начинает зловеще пищать!");
    visible(world, mob, &text, &text, Span::Warning);
    // `notify_ghosts(...)`, as a line to every ghost.
    let alert = format!("{who} вот-вот взорвёт свой взрывной имплант!");
    for ghost in crate::ghost_poll::ghosts(world) {
        tell_mob(world, ghost, Span::Danger, alert.clone());
    }
    beep(world, mob, 30.0);
    let now = world.resource::<Time>().elapsed_secs_f64();
    let delay = f64::from(delay);
    // A panicking bomb skips the first sleep and never paralyzes (`no_paralyze`); its beeps
    // start at once, each a fifth of the delay left after the last.
    let detonation = if panic {
        Detonation {
            implant: id,
            paralyze_at: None,
            at: now + delay,
            next_beep: Some(now),
            step: delay * 0.2,
            volume: 35.0,
            panic: true,
        }
    } else {
        Detonation {
            implant: id,
            paralyze_at: Some(now + delay * 0.25),
            at: now + delay,
            next_beep: Some(now + delay * 0.25),
            step: delay * 0.25,
            volume: 35.0,
            panic: false,
        }
    };
    world.entity_mut(mob).insert(detonation);
}

/// `playsound(loc, 'sound/items/timer.ogg', volume, FALSE)`.
fn beep(world: &mut World, mob: Entity, volume: f32) {
    if let Some(pos) = crate::explosions::turf_of(world, mob) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, TIMER_SOUND, volume, false),
        );
    }
}

/// `timed_explosion` after its sleeps.
fn run_detonations(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &Detonation)>();
    let due: Vec<(Entity, Detonation)> = query.iter(world).map(|(e, d)| (e, *d)).collect();
    for (mob, d) in due {
        if d.paralyze_at.is_some_and(|t| now >= t) {
            if let Some(mut det) = world.get_mut::<Detonation>(mob) {
                det.paralyze_at = None;
            }
            // `!IS_UNCONSCIOUS_OR_CRIT(imp_in)`.
            if world.get::<Stat>(mob) == Some(&Stat::Conscious) {
                let who = name_of(world, mob);
                let text = format!("{who} сгибается пополам от боли!");
                visible(world, mob, &text, &text, Span::Warning);
                crate::status_effects::apply(world, mob, Incap::Paralyze, BOMB_PARALYZE_SECS);
            }
        }
        if let Some(at) = d.next_beep.filter(|&t| now >= t && now < d.at) {
            beep(world, mob, d.volume);
            let next = next_beep(&d, at);
            if let Some(mut det) = world.get_mut::<Detonation>(mob) {
                (det.next_beep, det.step, det.volume) = next;
            }
        }
        if now >= d.at {
            world.entity_mut(mob).remove::<Detonation>();
            explode(world, mob, d.implant);
        }
    }
}

/// The beep after the one at `at`: (time, step, volume). A regular countdown beeps at a
/// quarter, half and three quarters of its delay, louder each time; a panicking one every fifth
/// of the delay left (`delay -= delay * 0.2`) until that is a decisecond.
fn next_beep(d: &Detonation, at: f64) -> (Option<f64>, f64, f32) {
    if d.panic {
        let step = d.step * 0.8;
        // `while(delay > 1)`: the delay left is five steps, in deciseconds.
        let next = (step * 5.0 > 0.1).then_some(at + d.step);
        (next, step, d.volume)
    } else {
        let next = Some(at + d.step).filter(|&t| t < d.at - 1e-6);
        (next, d.step, d.volume + 5.0)
    }
}

/// `explosive/explode`: `explosion(src, floor(devastate), floor(heavy), floor(light),
/// flame = light, flash = light)`, then the host is gibbed.
fn explode(world: &mut World, mob: Entity, id: u32) {
    let Some(imp) = remove_implant(world, mob, id, true) else {
        return;
    };
    let Some(pos) = crate::explosions::turf_of(world, mob) else {
        return;
    };
    // `macro/fake/explode`: a honkerblast of half the macrobomb's ranges first.
    if imp.path == FAKE_MACROBOMB {
        honkerblast(world, pos, [10.0, 4.0, 2.0]);
    }
    let [dev, heavy, light] = imp.blast.map(|b| b.floor() as i32);
    crate::explosions::explosion(
        world,
        pos,
        crate::explosions::Explosion::new(dev, heavy, light)
            .flame(light)
            .flash(light)
            .cause(&imp.name),
    );
    if world.get_entity(mob).is_ok() {
        crate::health::gib(world, mob);
    }
}

/// tg `honkerblast(origin, light, medium, heavy)`: an airhorn, then the carbons around are
/// knocked down, paralyzed or knocked out by their distance (`[light, medium, heavy]`).
// shortcut: everyone in range hears it (no `hearers`, no `TRAIT_DEAF`), no ear damage, no
// knotted laces, the HONK is plain text.
fn honkerblast(world: &mut World, at: TilePos, ranges: [f32; 3]) {
    use crate::status_effects::{Timed, apply, set_timed};
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(at, "sound/items/airhorn/airhorn.ogg", 100.0, true),
    );
    let [light, medium, heavy] = ranges;
    let reach = light.max(medium).max(heavy) as u32;
    let victims: Vec<(Entity, f32)> = world
        .resource::<ss13_world::SpatialIndex>()
        .in_radius(at, reach)
        .filter(|&e| world.get::<crate::medical::wounds::Limbs>(e).is_some())
        .filter_map(|e| Some((e, world.get::<TilePos>(e)?.chebyshev(at)? as f32)))
        .collect();
    for (victim, distance) in victims {
        if distance <= heavy {
            apply(world, victim, Incap::Unconscious, 4.0);
            apply(world, victim, Incap::Stun, 10.0);
            crate::status_effects::adjust_timed(world, victim, Timed::Stutter, 30.0, None);
            set_timed(world, victim, Timed::Jitter, 1000.0, true);
            tell_mob(world, victim, Span::BoldWarning, "ХОНК");
        } else if distance <= medium {
            apply(world, victim, Incap::Paralyze, 2.0);
            apply(world, victim, Incap::Stun, 5.0);
            set_timed(world, victim, Timed::Jitter, 500.0, true);
            tell_mob(world, victim, Span::Warning, "ХОНК");
        } else if distance <= light {
            crate::status_effects::knockdown(world, victim, 2.0, 0.0);
            set_timed(world, victim, Timed::Jitter, 200.0, true);
            tell_mob(world, victim, Span::Warning, "хонк");
        }
    }
}

/// tg `chem/activate(amount)`: `amount` units into the host's blood; an empty implant is gone.
/// The amount injected.
pub fn chem_inject(world: &mut World, mob: Entity, id: u32, amount: f32) -> f32 {
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return 0.0;
    };
    let Some(mut holder) = world
        .get_mut::<Implants>(mob)
        .and_then(|mut i| i.0.iter_mut().find(|i| i.id == id)?.reagents.take())
    else {
        return 0.0;
    };
    let moved = world
        .get_mut::<crate::chem::MobReagents>(mob)
        .map_or(0.0, |mut m| {
            holder.trans_to(&chem, &mut m.blood, amount, 1.0, None, false, false)
        });
    let empty = holder.total_volume <= 0.0;
    if let Some(mut implants) = world.get_mut::<Implants>(mob)
        && let Some(imp) = implants.0.iter_mut().find(|i| i.id == id)
    {
        imp.reagents = Some(holder);
    }
    tell_mob(world, mob, Span::Notice, "Вы слышите тихий писк.");
    if empty {
        tell_mob(
            world,
            mob,
            Span::Notice,
            "Вы слышите тихий щелчок из своей груди.",
        );
        remove_implant(world, mob, id, true);
    }
    moved
}

/// BandaStation `death_alarm/activate`: the host and its area (on the station or mining levels)
/// on the command, medical and security channels, from the implant's internal radio.
fn death_alarm(world: &mut World, mob: Entity) {
    let Some(pos) = crate::explosions::turf_of(world, mob) else {
        return;
    };
    let name = name_of(world, mob);
    let sensed = crate::events::controller::station_levels(world).contains(&pos.z)
        || ss13_mapload::mining_z(world) == Some(pos.z);
    let text = if sensed {
        format!("{name} умер в {}.", crate::bots::area_name(world, pos))
    } else {
        format!("{name} умер за пределами действия сенсоров.")
    };
    for freq in DEATH_ALARM_FREQS {
        world.write_message(crate::radio::MachineRadio {
            freq,
            z: pos.z,
            name: "имплант оповещения о смерти".to_owned(),
            text: text.clone(),
        });
    }
}

/// `COMSIG_LIVING_DEATH`: bombs arm (`activate("death")`), chem implants inject everything, the
/// death alarm reports and the sad trombone plays.
fn on_death(dead: Query<(Entity, &Stat, &Implants), Changed<Stat>>, mut commands: Commands) {
    for (mob, stat, implants) in &dead {
        if *stat != Stat::Dead {
            continue;
        }
        let hits: Vec<(ImplantKind, u32, f32)> = implants
            .0
            .iter()
            .map(|i| {
                (
                    i.kind,
                    i.id,
                    i.reagents.as_ref().map_or(0.0, |r| r.total_volume),
                )
            })
            .collect();
        commands.queue(move |world: &mut World| {
            for (kind, id, volume) in hits {
                match kind {
                    ImplantKind::Explosive => activate_bomb(world, mob, id, true),
                    ImplantKind::Chem => {
                        chem_inject(world, mob, id, volume);
                    }
                    ImplantKind::DeathAlarm => death_alarm(world, mob),
                    ImplantKind::Deathrattle => deathrattle(world, mob, id),
                    // `COMSIG_MOB_EMOTED("deathgasp")`: the port's deathgasp comes with death
                    // (`crate::health`).
                    ImplantKind::SadTrombone => {
                        if let Some(pos) = crate::explosions::turf_of(world, mob) {
                            crate::sound::playsound(
                                world,
                                crate::sound::PlaySound::new(pos, SAD_TROMBONE_SOUND, 50.0, false),
                            );
                        }
                    }
                    _ => {}
                }
            }
        });
    }
}

/// `strings/phonetic_alphabet.txt`.
const PHONETIC: [&str; 26] = [
    "Альфа",
    "Браво",
    "Чарли",
    "Дельта",
    "Эхо",
    "Фокстрот",
    "Гольф",
    "Отель",
    "Индиа",
    "Джульет",
    "Кило",
    "Лима",
    "Майк",
    "Новембер",
    "Оскар",
    "Папа",
    "Квебек",
    "Ромео",
    "Сиерра",
    "Танго",
    "Униформ",
    "Виктор",
    "Виски",
    "Икс-рей",
    "Янки",
    "Зулу",
];

/// `new deathrattle_group_type()`: "STD-<100..999> <letter>" (`MIN-` for the expeditionary
/// group). Its id.
pub fn new_deathrattle_group(world: &mut World, lavaland: bool) -> u32 {
    let (number, letter) = {
        let mut rng = world.resource_mut::<crate::combat::CombatRng>();
        (
            rng.range(100, 999),
            rng.range(0, PHONETIC.len() as u32 - 1) as usize,
        )
    };
    let prefix = if lavaland { "MIN" } else { "STD" };
    let mut groups = world.resource_mut::<DeathrattleGroups>();
    let id = groups.groups.iter().map(|g| g.id).max().unwrap_or(0) + 1;
    groups.groups.push(DeathrattleGroup {
        id,
        name: format!("{prefix}-{number} {}", PHONETIC[letter]),
        lavaland,
    });
    id
}

/// The area type path at `pos`.
fn area_path(world: &World, pos: TilePos) -> Option<String> {
    let area = world.get_resource::<ss13_mapload::Areas>()?.area_at(pos)?;
    world
        .get::<ss13_mapload::DmPower>(area)
        .map(|p| p.path.clone())
}

/// `deathrattle_group/on_user_statchange(DEAD)`: the other hosts of the dead host's group hear
/// who died where (an expeditionary group only for deaths in the wastes).
fn deathrattle(world: &mut World, dead: Entity, id: u32) {
    let Some(group) = implants_of(world, dead)
        .iter()
        .find(|i| i.id == id)
        .and_then(|i| i.group)
        .and_then(|g| world.resource::<DeathrattleGroups>().get(g).cloned())
    else {
        return;
    };
    let Some(pos) = crate::explosions::turf_of(world, dead) else {
        return;
    };
    if group.lavaland
        && !area_path(world, pos)
            .is_some_and(|p| is_under(&p, "/area/lavaland") || is_under(&p, "/area/icemoon"))
    {
        return;
    }
    let name = name_of(world, dead);
    let area = crate::bots::area_name(world, pos);
    let sound = {
        let mut rng = world.resource_mut::<crate::combat::CombatRng>();
        KNELLS[rng.range(0, KNELLS.len() as u32 - 1) as usize]
    };
    let mut query = world.query::<(Entity, &Implants)>();
    let recipients: Vec<Entity> = query
        .iter(world)
        .filter(|&(mob, imps)| {
            mob != dead
                && imps
                    .0
                    .iter()
                    .any(|i| i.kind == ImplantKind::Deathrattle && i.group == Some(group.id))
        })
        .map(|(mob, _)| mob)
        .collect();
    for mob in recipients {
        tell_mob(
            world,
            mob,
            Span::Italics,
            format!(
                "Вы слышите странный механический голос в своей голове... \"{name} умер в \
                 {area}.\""
            ),
        );
        crate::hallucination::play_local(world, mob, None, sound, 75.0, false);
    }
}

/// `deniability/check_health`: below the crit threshold the bomb arms
/// (`activate("deniability")`).
fn deniability_crit(
    mobs: Query<(Entity, &Health, &Implants), Changed<Health>>,
    mut commands: Commands,
) {
    for (mob, health, implants) in &mobs {
        let Some(bomb) = implants.0.iter().find(|i| i.deniability() && !i.active) else {
            continue;
        };
        if health.health() >= crate::health::HEALTH_THRESHOLD_CRIT {
            continue;
        }
        let id = bomb.id;
        commands.queue(move |world: &mut World| activate_bomb(world, mob, id, true));
    }
}

/// The deniability implant's `TRAIT_NOSOFTCRIT` and `TRAIT_NOHARDCRIT`
/// (`crate::medical::traumas` syncs them with the others).
pub fn denies_crit(world: &World, mob: Entity) -> bool {
    implants_of(world, mob).iter().any(Implant::deniability)
}

/// The TRAC implant dissolves after its `lifespan`.
fn expire_trac(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut query = world.query::<(Entity, &TracExpiry)>();
    let due: Vec<(Entity, u32)> = query
        .iter(world)
        .filter(|(_, t)| now >= t.at)
        .map(|(e, t)| (e, t.implant))
        .collect();
    for (mob, id) in due {
        world.entity_mut(mob).remove::<TracExpiry>();
        remove_implant(world, mob, id, true);
    }
}

/// `/obj/projectile/bullet/c38/trac/on_hit`: a carbon without a TRAC implant gets one.
pub fn trac_hit(world: &mut World, projectile: &str, target: Entity) {
    if projectile != "/obj/projectile/bullet/c38/trac"
        || world.get::<crate::medical::wounds::Limbs>(target).is_none()
        || implants_of(world, target).iter().any(|i| i.path == TRAC)
    {
        return;
    }
    let id = next_id(world);
    if let Some(imp) = Implant::new(TRAC, id) {
        let _ = implant(world, target, None, imp);
    }
}

pub fn has_kind(world: &World, mob: Entity, kind: ImplantKind) -> bool {
    implants_of(world, mob).iter().any(|i| i.kind == kind)
}

/// The home gateway's `check_exile_implant`: an exile implant (or its anti-teleportation
/// subtype) tells its host it is refused.
pub fn check_exile(world: &mut World, mob: Entity) -> bool {
    if !has_kind(world, mob, ImplantKind::Exile) {
        return false;
    }
    tell_mob(
        world,
        mob,
        Span::Danger,
        "Врата станции обнаружили ваш имплант изгнания и блокируют вход.",
    );
    true
}

/// `teleport_blocker/on_teleport` (`COMSIG_MOVABLE_TELEPORTING`): true when the teleport is
/// blocked; the host is flung back and paralyzed.
pub fn teleport_blocked(world: &mut World, mob: Entity) -> bool {
    if !has_kind(world, mob, ImplantKind::TeleportBlocker) {
        return false;
    }
    tell_mob(
        world,
        mob,
        Span::Warning,
        "Вы чувствуете, что телепортируетесь, но вас внезапно отбрасывает обратно!",
    );
    grounding_jolt(world, mob);
    true
}

/// `teleport_blocker/on_jaunt` (`COMSIG_MOB_PRE_JAUNT`): true when the jaunt is blocked.
pub fn jaunt_blocked(world: &mut World, mob: Entity) -> bool {
    if !has_kind(world, mob, ImplantKind::TeleportBlocker) {
        return false;
    }
    tell_mob(
        world,
        mob,
        Span::Warning,
        "Пытаясь уйти в астрал, вы врезаетесь прямо в барьер между реальностями и с грохотом \
         возвращаетесь в телесность!",
    );
    grounding_jolt(world, mob);
    true
}

/// `Paralyze(5 SECONDS)` and `do_sparks(5, TRUE, ..., quantum)`.
fn grounding_jolt(world: &mut World, mob: Entity) {
    crate::status_effects::apply(world, mob, Incap::Paralyze, BLOCKER_PARALYZE_SECS);
    if let Some(pos) = crate::explosions::turf_of(world, mob) {
        crate::effects::particles::start(
            world,
            crate::effects::particles::Kind::QuantumSparks,
            5,
            true,
            pos,
        );
    }
}

/// The beacon implant of `mob` is a target: it has one, and its host died at most
/// [`POSTMORTEM_SECS`] ago (`lifespan_postmortem`).
pub fn beacon_live(world: &World, mob: Entity) -> bool {
    if !has_kind(world, mob, ImplantKind::Beacon) {
        return false;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    !(world.get::<Stat>(mob) == Some(&Stat::Dead)
        && world
            .get::<crate::brig::prisoner::DiedAt>(mob)
            .is_some_and(|d| d.0 + POSTMORTEM_SECS < now))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.init_resource::<NextImplantId>();
        world
    }

    fn mob(world: &mut World) -> Entity {
        world
            .spawn((Health::default(), Stat::Conscious, TilePos::new(1, 1, 1)))
            .id()
    }

    #[test]
    fn holders_start_with_their_imp_type() {
        assert_eq!(imp_type("/obj/item/implanter/mindshield"), Some(MINDSHIELD));
        assert_eq!(imp_type("/obj/item/implantcase/chem"), Some(CHEM));
        assert_eq!(imp_type("/obj/item/implanter/tracking/gps"), None);
        assert_eq!(imp_type("/obj/item/implanter"), None);
        assert_eq!(ImplantKind::of(MACROBOMB), Some(ImplantKind::Explosive));
        assert_eq!(ImplantKind::of(DENIABILITY), Some(ImplantKind::Explosive));
        assert_eq!(
            ImplantKind::of("/obj/item/implant/explosive/deathmatch"),
            None
        );
        assert_eq!(ImplantKind::of(NOTELEPORT), Some(ImplantKind::Exile));
        assert_eq!(
            ImplantKind::of("/obj/item/implant/tracking/c38"),
            Some(ImplantKind::Tracking)
        );
    }

    /// tg `implant()`: two security implants at most, a duplicate tops up its uses up to twice
    /// the initial ones, a second microbomb merges into the first.
    #[test]
    fn implanting_caps_duplicates_and_merges_bombs() {
        let mut world = world();
        let target = mob(&mut world);
        let mut id = 0;
        let mut new = |path: &str| {
            id += 1;
            Implant::new(path, id).unwrap()
        };
        assert!(implant(&mut world, target, None, new(TRACKING)).is_ok());
        assert!(implant(&mut world, target, None, new(CHEM)).is_ok());
        let refused = implant(&mut world, target, None, new(TRACKING));
        assert!(refused.is_err(), "a third security implant");
        assert!(implant(&mut world, target, None, new(EMP)).is_ok());
        assert!(implant(&mut world, target, None, new(EMP)).is_ok());
        let emp = implants_of(&world, target)
            .iter()
            .find(|i| i.kind == ImplantKind::Emp)
            .unwrap();
        assert_eq!(emp.uses, 6, "3 + 3, capped at 6");
        assert!(implant(&mut world, target, None, new(EMP)).is_err());
        assert!(implant(&mut world, target, None, new(MICROBOMB)).is_ok());
        assert!(implant(&mut world, target, None, new(MACROBOMB)).is_ok());
        let bombs: Vec<&Implant> = implants_of(&world, target)
            .iter()
            .filter(|i| i.kind == ImplantKind::Explosive)
            .collect();
        assert_eq!(bombs.len(), 1);
        assert!(
            (bombs[0].blast[0] - 4.4).abs() < 1e-4,
            "{:?}",
            bombs[0].blast
        );
        assert!((bombs[0].delay - 7.7).abs() < 1e-4);
    }

    #[test]
    fn silicons_take_no_implants() {
        let mut world = world();
        let borg = mob(&mut world);
        world.entity_mut(borg).insert(crate::silicon::Silicon);
        assert!(implant(&mut world, borg, None, Implant::new(EMP, 1).unwrap()).is_err());
    }

    #[test]
    fn emp_implant_is_gone_after_its_uses() {
        let mut world = world();
        world.init_resource::<Messages<crate::emp::Empulse>>();
        let target = mob(&mut world);
        implant(&mut world, target, None, Implant::new(EMP, 7).unwrap()).unwrap();
        for left in (0..EMP_USES).rev() {
            activate_emp(&mut world, target, 7);
            let still = implants_of(&world, target).iter().any(|i| i.id == 7);
            assert_eq!(still, left > 0);
        }
    }

    #[test]
    fn a_macrobomb_counts_down_and_a_microbomb_does_not() {
        let mut world = world();
        let target = mob(&mut world);
        implant(
            &mut world,
            target,
            None,
            Implant::new(MACROBOMB, 3).unwrap(),
        )
        .unwrap();
        activate_bomb(&mut world, target, 3, false);
        let d = *world.get::<Detonation>(target).unwrap();
        assert!((d.at - 7.0).abs() < 1e-6 && (d.paralyze_at.unwrap() - 1.75).abs() < 1e-6);
        // Armed once: a second activation does nothing.
        activate_bomb(&mut world, target, 3, false);
        assert_eq!(*world.get::<Detonation>(target).unwrap(), d);
    }

    #[test]
    fn fork_and_passive_implants_come_from_their_holders() {
        assert_eq!(imp_type("/obj/item/implanter/cqc"), Some(CQC));
        assert_eq!(
            imp_type("/obj/item/implantcase/death_alarm"),
            Some(DEATH_ALARM)
        );
        assert_eq!(
            imp_type("/obj/item/implanter/empprotection"),
            Some(EMP_PROTECTION)
        );
        assert_eq!(
            imp_type("/obj/item/implantcase/weapons_auth"),
            Some(WEAPONS_AUTH)
        );
        assert_eq!(
            ImplantKind::of(SAD_TROMBONE),
            Some(ImplantKind::SadTrombone)
        );
    }

    /// BandaStation `empprotection`: `EMP_PROTECT_ALL` while implanted, gone on removal.
    #[test]
    fn emp_absorber_protects_its_host_while_implanted() {
        let mut world = world();
        let target = mob(&mut world);
        let imp = Implant::new(EMP_PROTECTION, 4).unwrap();
        implant(&mut world, target, None, imp).unwrap();
        assert_eq!(
            world.get::<crate::emp::EmpProtection>(target).map(|p| p.0),
            Some(crate::emp::PROTECT_ALL)
        );
        remove_implant(&mut world, target, 4, true).unwrap();
        assert!(world.get::<crate::emp::EmpProtection>(target).is_none());
    }

    /// BandaStation `cqc/activate`: the button teaches CQC, again forgets it; removal forgets.
    #[test]
    fn cqc_implant_toggles_the_style() {
        let mut world = world();
        let target = mob(&mut world);
        implant(&mut world, target, None, Implant::new(CQC, 5).unwrap()).unwrap();
        let cqc = |w: &World| crate::martial::active_style(w, target);
        assert_eq!(cqc(&world), None);
        activate_cqc(&mut world, target, 5);
        assert_eq!(cqc(&world), Some(crate::martial::Style::Cqc));
        activate_cqc(&mut world, target, 5);
        assert_eq!(cqc(&world), None);
        activate_cqc(&mut world, target, 5);
        remove_implant(&mut world, target, 5, true).unwrap();
        assert_eq!(cqc(&world), None);
    }

    /// The exile family and the grounding and beacon implants count against the cap; the TRAC
    /// implant does not, and dissolves after five minutes.
    #[test]
    fn security_flags_and_the_trac_lifespan() {
        let new = |path: &str| Implant::new(path, 1).unwrap();
        assert!(new(EXILE).security() && new(TELEPORT_BLOCKER).security());
        assert!(new(BEACON).security() && new(TRACKING).security());
        assert!(!new(TRAC).security() && !new(SMOKE).security());
        let mut world = world();
        let target = mob(&mut world);
        implant(&mut world, target, None, Implant::new(TRAC, 9).unwrap()).unwrap();
        let expiry = *world.get::<TracExpiry>(target).unwrap();
        assert_eq!(expiry.implant, 9);
        assert!((expiry.at - TRAC_LIFESPAN_SECS).abs() < 1e-6);
        remove_implant(&mut world, target, 9, true).unwrap();
        assert!(world.get::<TracExpiry>(target).is_none());
    }

    /// `exile/noteleport`: `TRAIT_NO_TELEPORT` while implanted; a plain exile implant after it
    /// is a duplicate of nothing, an exile implant after a noteleport one is.
    #[test]
    fn anti_teleportation_implant_grounds_its_host() {
        let mut world = world();
        let target = mob(&mut world);
        implant(
            &mut world,
            target,
            None,
            Implant::new(NOTELEPORT, 2).unwrap(),
        )
        .unwrap();
        assert!(world.get::<crate::teleport::NoTeleport>(target).is_some());
        assert!(has_kind(&world, target, ImplantKind::Exile));
        // `istype(noteleport, exile)`: refused as a duplicate with no uses to top up.
        assert!(implant(&mut world, target, None, Implant::new(EXILE, 3).unwrap()).is_err());
        remove_implant(&mut world, target, 2, true).unwrap();
        assert!(world.get::<crate::teleport::NoTeleport>(target).is_none());
    }

    /// A microbomb merges into a deniability implant already there; a second deniability
    /// implant does not fit.
    #[test]
    fn deniability_is_the_master_bomb() {
        let mut world = world();
        let target = mob(&mut world);
        world
            .entity_mut(target)
            .insert(Implants(vec![Implant::new(DENIABILITY, 4).unwrap()]));
        assert!(
            implant(
                &mut world,
                target,
                None,
                Implant::new(DENIABILITY, 5).unwrap()
            )
            .is_err()
        );
        implant(
            &mut world,
            target,
            None,
            Implant::new(MICROBOMB, 6).unwrap(),
        )
        .unwrap();
        let bombs = implants_of(&world, target);
        assert_eq!(bombs.len(), 1);
        assert_eq!(bombs[0].path, DENIABILITY);
        assert!((bombs[0].delay - (DENIABILITY_DELAY + MICROBOMB_DELAY)).abs() < 1e-4);
        assert!((bombs[0].blast[2] - 4.0).abs() < 1e-4);
        // The fake macrobomb has the macrobomb's delay and a microbomb's blast.
        let fake = Implant::new(FAKE_MACROBOMB, 7).unwrap();
        assert!((fake.delay - 7.0).abs() < 1e-4 && (fake.blast[2] - 2.0).abs() < 1e-4);
    }

    /// `timed_explosion`: beeps at a quarter, half and three quarters, louder each time; a
    /// panicking countdown beeps every fifth of what is left until a decisecond is left.
    #[test]
    fn countdown_beeps() {
        let regular = Detonation {
            implant: 1,
            paralyze_at: Some(2.5),
            at: 10.0,
            next_beep: Some(2.5),
            step: 2.5,
            volume: 35.0,
            panic: false,
        };
        let (next, _, volume) = next_beep(&regular, 2.5);
        assert_eq!((next, volume), (Some(5.0), 40.0));
        let last = Detonation {
            volume: 45.0,
            ..regular
        };
        assert_eq!(next_beep(&last, 7.5).0, None);
        let mut panic = Detonation {
            paralyze_at: None,
            next_beep: Some(0.0),
            step: 2.0,
            volume: 35.0,
            panic: true,
            ..regular
        };
        let mut at = 0.0;
        let mut beeps = 1;
        while let (Some(next), step, _) = next_beep(&panic, at) {
            assert!(next < panic.at);
            panic.step = step;
            at = next;
            beeps += 1;
        }
        // 10 s shrinking by a fifth each beep while over a decisecond: 20 more beeps.
        assert_eq!(beeps, 21);
    }

    /// A deathrattle needs a group before it goes in; several fit in one host.
    #[test]
    fn deathrattle_needs_a_group() {
        let mut world = world();
        let target = mob(&mut world);
        let (_, reason) = implant(
            &mut world,
            target,
            None,
            Implant::new(DEATHRATTLE, 1).unwrap(),
        )
        .unwrap_err();
        assert!(reason.is_some());
        for id in [2, 3] {
            let mut imp = Implant::new(DEATHRATTLE, id).unwrap();
            imp.group = Some(1);
            implant(&mut world, target, None, imp).unwrap();
        }
        assert_eq!(implants_of(&world, target).len(), 2);
    }
}
