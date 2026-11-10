//! Space pirates after tg `/datum/antagonist/pirate`, `/datum/team/pirate`,
//! `/datum/objective/loot` (`pirate.dm`), `/datum/pirate_gang` (`pirate_gangs.dm`), the pirate
//! spawners (`pirate_roles.dm`) and the "Light Pirates" / "Heavy Pirates" midrounds
//! (`/datum/dynamic_ruleset/midround/pirates`).
//!
//! - **The threat** ([`send_threat`]): a gang not seen this round is taken from the light or
//!   heavy pool; the station is told of an incoming channel and every communications console
//!   gets the gang's message demanding `max(20000, 80% of the cargo budget, floored to 1000)`
//!   credits. Answered within 2 minutes ([`answered`]): paying takes the money from the cargo
//!   budget (not enough: "not enough"), refusing or answering late gets the gang's reply. The
//!   Siren Shimai pose as another gang and reveal themselves in their replies.
//! - **The raid** ([`tick`], 2 minutes after the threat, unless paid off): every ghost is polled
//!   for the crew; the volunteers take the gang's ship spawners, leader first ([`GANGS`]: the
//!   spawners of each `_maps/shuttles/pirate/pirate_*.dmm`), on one maintenance spawn, as the
//!   spawner's human (species,
//!   outfit, faction: `crate::ghost_roles`) named "[rank] [name]", the space pirate job, the crew
//!   and its loot objective; then the gang announces its arrival.
//! - [`roundend_report`]: the crew, the loot and whether it reached 50000 credits.
// shortcut: the pirate ship is not loaded. `ss13_mapload::load_template` could put it on its
// own level, but it could not fly to the station: the pirate shuttle console only reaches
// `pirate_custom`, set by the navigation computer, whose port (`crate::shuttle_docker`, no camera
// eye) only picks a landing spot among the tiles its operator sees from the ship's level. So the
// crew boards together on one station maintenance spawn (`find_maintenance_spawn`: carp spawns
// are open space, where crews without space suits die), without the ship's gear, shuttle scrambler, loot locator and the cargo hold's pad: the loot objective has no
// hold to count and always reads 0. Unfilled spawners are lost (tg keeps them on the ship);
// no pirate language, no medieval footmen's dash and unbreakable, no warlord mutations, no
// siren cloning look; silverscales take human first names (no lizard name lists). With a
// docker that sees other levels: load `pirate_<ship_template_id>` with `load_template`, set it
// up with `crate::shuttle::setup_template_shuttle`, and fill its spawners instead.

use bevy::prelude::*;
use ss13_net::server::ClientToken;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatMessage, ChatSegment, Span};

use super::super::objectives::RoundEnd;
use super::{line, pick, with_rng};
use crate::cargo::economy::{ACCOUNT_CAR, Economy};
use crate::comms::CommsMessage;
use crate::ghost_poll::{self, Candidate, DEFAULT_POLL_SECS, PollSpec};
use crate::server::{PlayerMob, SpawnPoints};
use crate::tools::{now, tell};

/// `PAYOFF_MIN`.
pub const PAYOFF_MIN: i64 = 20_000;
/// `RESPONSE_MAX_TIME`, seconds.
pub const RESPONSE_MAX_SECS: f64 = 120.0;
/// `/datum/objective/loot` `target_value`.
pub const LOOT_TARGET: i64 = 50_000;
const JOB: &str = "/datum/job/space_pirate";
const SPAWNER_ROOT: &str = "/obj/effect/mob_spawn/ghost_role/human/pirate";
const LOOT_TEXT: &str = "Acquire valuable loot and store it in the designated area.";

/// One `/datum/pirate_gang`.
#[derive(Debug)]
pub struct Gang {
    pub name: &'static str,
    pub heavy: bool,
    pub ship_template_id: &'static str,
    pub ship_name_pool: &'static str,
    pub threat_title: &'static str,
    /// `%SHIPNAME` and `%PAYOFF` are replaced.
    pub threat_content: &'static str,
    pub arrival: &'static str,
    pub answers: [&'static str; 2],
    pub received: &'static str,
    pub rejected: &'static str,
    pub too_late: &'static str,
    pub not_enough: &'static str,
    /// The ship's spawners, leader first: (type, count).
    pub spawners: &'static [(&'static str, u32)],
}

#[allow(clippy::too_many_arguments)]
const fn gang(
    name: &'static str,
    heavy: bool,
    ship_template_id: &'static str,
    ship_name_pool: &'static str,
    threat_title: &'static str,
    threat_content: &'static str,
    arrival: &'static str,
    answers: [&'static str; 2],
    responses: [&'static str; 4],
    spawners: &'static [(&'static str, u32)],
) -> Gang {
    Gang {
        name,
        heavy,
        ship_template_id,
        ship_name_pool,
        threat_title,
        threat_content,
        arrival,
        answers,
        received: responses[0],
        rejected: responses[1],
        too_late: responses[2],
        not_enough: responses[3],
        spawners,
    }
}

pub const SIREN: &str = "Siren Shimai";

/// Every `/datum/pirate_gang` subtype (all `can_roll`).
pub static GANGS: [Gang; 9] = [
    gang(
        "Rogues",
        false,
        "default",
        "rogue_names",
        "Предложение по защите сектора",
        "Привет, приятель! Это %SHIPNAME. Не могу не заметить, что у тебя дикий и сумасшедший \
         шаттл без страховки! Безумец. Что, если с ним что-то случится?! Мы провели быструю \
         оценку ваших доходов в этом секторе и просим %PAYOFF кредитов, чтобы покрыть вашу \
         страховку шаттла на случай стихийного бедствия.",
        "Не хотите ли вы рассмотреть наше предложение? К сожалению, время для переговоров \
         прошло. Открывайте, мы скоро прибудем на борт.",
        ["Приобрести страховку.", "Отклонить предложение."],
        [
            "Халявные деньги. Съебываем, ребята.",
            "Не платить было ошибкой, теперь вам нужно пройти курс по экономике.",
            "Платите или нет, но игнорирование нас стало вопросом нашей гордости. Теперь пришло \
             время научить вас уважению.",
            "Вы думали, мы не заметим, если вы недоплатите? Смешно. Скоро мы с вами увидимся.",
        ],
        &[
            ("/obj/effect/mob_spawn/ghost_role/human/pirate/captain", 1),
            ("/obj/effect/mob_spawn/ghost_role/human/pirate", 2),
        ],
    ),
    gang(
        "Silverscales",
        false,
        "silverscale",
        "silverscale_names",
        "Запрос на выплату дани",
        "Это %SHIPNAME. Серебряные Чешуи желают получить кое-какую дань от ваших плебейских \
         ящеров. %PAYOFF кредитов должно хватить.",
        "Разумеется, вы не заслуживаете всего этого на борту своего судна. Оно подойдет нам \
         гораздо лучше.",
        ["Мы заплатим.", "Дань? Серьёзно? Проваливайте."],
        [
            "Щедрое пожертвование. Пусть когти Тизиры дотянутся до самых дальних точек космоса.",
            "Не зря же первое правило охоты гласит: не уходи без добычи.",
            "Я вижу, что вы пытаетесь заплатить, но охота уже началась.",
            "Вы отправили оскорбительное \"пожертвование\". За вами начата охота.",
        ],
        &[
            (
                "/obj/effect/mob_spawn/ghost_role/human/pirate/silverscale/captain",
                1,
            ),
            (
                "/obj/effect/mob_spawn/ghost_role/human/pirate/silverscale",
                1,
            ),
            (
                "/obj/effect/mob_spawn/ghost_role/human/pirate/silverscale/gunner",
                1,
            ),
        ],
    ),
    gang(
        "Skeleton Pirates",
        true,
        "dutchman",
        "skeleton_names",
        "Передача имущества",
        "Эгегей! Это %SHIPNAME. Выплачивайте %PAYOFF кредитов или пойдете по доске.",
        "Веселый Роджер не будет ждать вечно, дружки; мы тут распластались рядом и готовы \
         отправить вам подарки.",
        ["Мы заплатим.", "Мы не потерпим вымогательства."],
        [
            "Спасибо за кредиты, аборигены.",
            "Черт бы побрал! Все на палубу, мы собираемся забрать их богатства.",
            "Слишком поздно просить пощады!",
            "Пытаетесь одурачить нас? Пожалеете об этом.",
        ],
        &[
            (
                "/obj/effect/mob_spawn/ghost_role/human/pirate/skeleton/captain",
                1,
            ),
            ("/obj/effect/mob_spawn/ghost_role/human/pirate/skeleton", 1),
            (
                "/obj/effect/mob_spawn/ghost_role/human/pirate/skeleton/gunner",
                1,
            ),
        ],
    ),
    gang(
        "Restless Ex-Pharmacists",
        true,
        "ex_interdyne",
        "interdyne_names",
        "Финансирование исследований",
        "Приветствую вас, это %SHIPNAME. Нам требуется некоторое финансирование для наших \
         фармацевтических операций. %PAYOFF кредитов должно быть достаточно.",
        "Мы скромно просим вас о значительном поступлении средств для будущих исследований \
         нашего дела. Конечно, было бы неприятно, если бы вы заболели, но мы можем это \
         исправить.",
        ["Ладно.", "Найдите себе работу!"],
        [
            "Благодарим вас за щедрость. Ваши деньги не будут потрачены понапрасну.",
            "О, ты не станция, ты опухоль. Что ж, придется ее вырезать.",
            "Поздно. Надеемся, вам понравится рак кожи!",
            "Этого недостаточно для наших операций. Боюсь, нам придется одолжить немного.",
        ],
        &[
            (
                "/obj/effect/mob_spawn/ghost_role/human/pirate/interdyne/senior",
                1,
            ),
            (
                "/obj/effect/mob_spawn/ghost_role/human/pirate/interdyne/junior",
                2,
            ),
        ],
    ),
    gang(
        "The Grey Tide",
        false,
        "grey",
        "grey_names",
        "Ограбление",
        "Это %SHIPNAME. Отдавайте свои деньги. %PAYOFF кредитов может быть достаточно.",
        "Отличные у вас вещи, теперь они наши.",
        ["Пожалуйста, не бейте.", "ВЫ ОТВЕТИТЕ ПЕРЕД ЗАКОНОМ!!!"],
        [
            "Подождите, вы действительно отдали свои деньги? Спасибо, но мы все равно придем за \
             остальным!",
            "Ответим перед законом? Мы и есть закон! И вы понесете ответственность!",
            "Ничего, да? Похоже, \"Тайд\" прибывает на борт!",
            "Пытаетесь наебать? Ничего страшного, мы возьмем вашу станцию в качестве залога.",
        ],
        &[(
            "/obj/effect/mob_spawn/ghost_role/human/pirate/grey/shitter",
            6,
        )],
    ),
    gang(
        "Space IRS Agents",
        true,
        "irs",
        "irs_names",
        "Пропущеные налоговые платежи",
        "%SHIPNAME на месте. Мы заметили, что ваша станция не платит налоги... Давайте исправим \
         это. Ваши недостающие налоговые отчисления составляют %PAYOFF кредитов Мы настоятельно \
         рекомендуем выплатить сумму налога, чтобы нам не пришлось отправлять команду на вашу \
         станцию, для разрешения ситуации.",
        "Это команда по разрешению налоговых конфликтов, готовьтесь к тому, что ваши активы \
         будут ликвидированы, а вас обвинят в налоговом мошенничестве, если вы не заплатите \
         налоги вовремя.",
        [
            "Знаете, я как раз собирался заплатить. Спасибо за напоминание!",
            "Мне все равно, кого присылает налоговая служба, я не буду платить налоги!",
        ],
        [
            "Оплата произведена, мы приветствуем вас как законопослушных граждан, платящих \
             налоги.",
            "Понимаем, я отправляю команду на вашу станцию, чтобы решить этот вопрос.",
            "Слишком поздно, уже отправлена команда, которая решает этот вопрос напрямую.",
            "Вы неправильно заполнили налоговую декларацию. Мы направили команду для ликвидации \
             активов и ареста вас за налоговое мошенничество. Ничего личного, парень.",
        ],
        &[
            (
                "/obj/effect/mob_spawn/ghost_role/human/pirate/irs/auditor",
                1,
            ),
            ("/obj/effect/mob_spawn/ghost_role/human/pirate/irs", 2),
        ],
    ),
    gang(
        "Geode Scavengers",
        false,
        "geode",
        "geode_names",
        "Странное сообщение",
        "Кристалл матери-пустоты трескается, и на свет появляется %SHIPNAME. Мы - Блестящие, \
         руки хрустального короля.Наши сундуки с блюспейс пылью иссякли, поэтому наш синтез \
         прекратился. %PAYOFF кредитов исправят это!",
        "Мы прибыли, мы всегда были здесь, и мы уже исчезли.",
        [
            "Аа, эм, окей? Хорошо.",
            "У нас нет времени на бредни сумасшедших, проваливайте.",
        ],
        [
            "Отличная посылка, синтез будет продолжен.",
            "Грубость в вашей речи необходимо нейтрализовать. И мы можем помочь вам в этом прямо \
             сейчас.",
            "Тогда вы не были готовы, а теперь это время прошло. Мы можем идти только вперед, \
             никогда не возвращаясь назад.",
            "Вы оскорбили нас, но вражды не будет, только стремительное правосудие!",
        ],
        &[
            (
                "/obj/effect/mob_spawn/ghost_role/human/pirate/lustrous/captain",
                1,
            ),
            ("/obj/effect/mob_spawn/ghost_role/human/pirate/lustrous", 1),
            (
                "/obj/effect/mob_spawn/ghost_role/human/pirate/lustrous/gunner",
                1,
            ),
        ],
    ),
    gang(
        "Medieval Warmongers",
        true,
        "medieval",
        "medieval_names",
        "ЗАЯВЛЕНИЕ НА ВЫПЛАТУ ГОНОРАРА",
        "ПРИВЕТСТВУЮ, ЭТО %SHIPNAME И МЫ СОБИРАЕМ ДАНЬ ОТ ВАССАЛОВ НА НАШЕЙ ТЕРРИТОРИИ, ТАК \
         ПОЛУЧИЛОСЬ, ЧТО ВЫ ТОЖЕ НА НЕЙ!!! ОБЫЧНО МЫ УБИВАЕМ ТАКИХ СЛАБАКОВ, КАК ВЫ, ЗА ВТОРЖЕНИЕ \
         НА НАШУ ЗЕМЛЮ, НО МЫ ГОТОВЫ ПРИВЕТСТВОВАТЬ ВАС В НАШЕМ ПРОСТРАНСТВЕ, ЕСЛИ ВЫ ЗАПЛАТИТЕ \
         %PAYOFF КРЕДИТОВ КАК ДАНЬ НАШЕМУ ПРАВУ. БУДЬТЕ МУДРЫ В СВОЕМ ВЫБОРЕ!!! (отправить \
         сообщение. отправить сообщение. почему сообщение не отправляется?).",
        "Я ПОНЯЛ, КАК УПРАВЛЯТЬ СВОИМ КОРАБЛЁМ, МЫ ПРИЧАЛИМ К ВАМ ЧЕРЕЗ МИНУТУ!!!",
        [
            "Ладно, мне нравится, когда мой череп цел.",
            "Болваны, свалите играть куда подальше.",
        ],
        [
            "ЭТОГО БУДЕТ ДОСТАТОЧНО, ПОМНИТЕ, КТО ЯВЛЯЕТСЯ ВАШИМ ВЛАДЕЛЬЦЕМ!!!",
            "ГЛУПОЕ РЕШЕНИЕ, Я СДЕЛАЮ ИЗ ТВОЕЙ ТУШКИ ОБРАЗЕЦ ДЛЯ ПОДРАЖАНИЯ!!! (кто-нибудь \
             помнит, как управлять нашим кораблём?)",
            "ТЫ УЖЕ В ОСАДЕ, ШУТ, ТЫ БЕЗМОЗГЛЫЙ ИЛИ НЕВЕЖЕСТВЕННЫЙ?!",
            "ТЫ СЧИТАЕШЬ МЕНЯ ШУТОМ? ТЫ, ТУХЛОЕ МЯСО!!! (я забыл, как управлять кораблем, зараза)",
        ],
        &[
            (
                "/obj/effect/mob_spawn/ghost_role/human/pirate/medieval/warlord",
                1,
            ),
            ("/obj/effect/mob_spawn/ghost_role/human/pirate/medieval", 4),
        ],
    ),
    // `false_flag` takes the threat from another gang; `reveal_yourself` writes the replies.
    gang(
        SIREN,
        false,
        "siren",
        "siren_names",
        "",
        "",
        "",
        ["", ""],
        ["", "", "", ""],
        &[
            (
                "/obj/effect/mob_spawn/ghost_role/human/pirate/siren/vocalist",
                1,
            ),
            ("/obj/effect/mob_spawn/ghost_role/human/pirate/siren", 1),
        ],
    ),
];

/// A spawner's `rank` and how it names (`generate_pirate_name`).
fn rank_of(spawner: &str) -> (&'static str, NameStyle) {
    let p = |s: &str| format!("{SPAWNER_ROOT}{s}");
    let table: [(&str, &'static str, NameStyle); 19] = [
        ("/captain", "Лидер ренегатов", NameStyle::Generic),
        ("/gunner", "Отступник", NameStyle::Generic),
        ("/skeleton/captain", "Капитан", NameStyle::Generic),
        ("/skeleton/gunner", "Канонир", NameStyle::Generic),
        ("/skeleton", "Боцман", NameStyle::Generic),
        (
            "/silverscale/captain",
            "Старая гвардия",
            NameStyle::Silverscale,
        ),
        (
            "/silverscale/gunner",
            "Первого класса",
            NameStyle::Silverscale,
        ),
        ("/silverscale", "Знатного рода", NameStyle::Silverscale),
        (
            "/interdyne/senior",
            "Директор-фармацевт",
            NameStyle::FirstName,
        ),
        ("/interdyne", "Фармацевт", NameStyle::FirstName),
        ("/grey/shitter", "Тайд-мастер", NameStyle::Generic),
        ("/grey", "Тайдер", NameStyle::Generic),
        ("/irs/auditor", "Главный аудитор", NameStyle::FirstName),
        ("/irs", "Агент", NameStyle::FirstName),
        ("/lustrous/captain", "Сияющий", NameStyle::Generic),
        ("/lustrous/gunner", "Мерцающий", NameStyle::Generic),
        ("/lustrous", "Сверкающий", NameStyle::Generic),
        ("/medieval/warlord", "Полководец", NameStyle::Generic),
        ("/medieval", "Пехотинец", NameStyle::Generic),
    ];
    if spawner == p("/siren/vocalist") {
        return ("Bookaru", NameStyle::FemaleFirst);
    }
    if spawner == p("/siren") {
        return ("Gitarisuto", NameStyle::FemaleFirst);
    }
    table
        .iter()
        .find(|(suffix, ..)| crate::obj_damage::is_under(spawner, &p(suffix)))
        .map_or(
            ("Дезертир", NameStyle::Generic),
            |&(_, rank, style)| (rank, style),
        )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NameStyle {
    /// `[rank] [beginning][ending]` from `generic_beginnings` / `generic_endings`.
    Generic,
    /// `[rank] [first name]-Серебрянная чешуя`.
    Silverscale,
    /// `[rank] [first name]`.
    FirstName,
    /// `[rank] [female first name]`.
    FemaleFirst,
}

/// One threat sent (the ruleset's callbacks' arguments).
#[derive(Clone, Debug, PartialEq)]
pub struct Threat {
    pub gang: usize,
    pub ship_name: String,
    pub payoff: i64,
    pub sent_at: f64,
    pub paid_off: bool,
    pub answered: bool,
    pub spawned: bool,
    pub received: String,
    pub rejected: String,
    pub too_late: String,
    pub not_enough: String,
    pub arrival: String,
}

/// The gangs taken this round (`GLOB.light_pirate_gangs` / `heavy_pirate_gangs` minus them),
/// the threats and the crew.
#[derive(Resource, Debug, Default)]
pub struct PirateState {
    pub taken: Vec<usize>,
    pub threats: Vec<Threat>,
    /// `/datum/team/pirate` members.
    pub crew: Vec<Entity>,
}

/// `/datum/antagonist/pirate` on its body.
#[derive(Component, Clone, Debug)]
pub struct Pirate {
    pub key: String,
}

pub struct PiratePlugin;

impl Plugin for PiratePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PirateState>().add_systems(Update, tick);
    }
}

/// The gangs of a pool still free.
pub fn pool(world: &World, heavy: bool) -> Vec<usize> {
    let taken = world
        .get_resource::<PirateState>()
        .map(|s| s.taken.clone())
        .unwrap_or_default();
    GANGS
        .iter()
        .enumerate()
        .filter(|(i, g)| g.heavy == heavy && !taken.contains(i))
        .map(|(i, _)| i)
        .collect()
}

/// `can_be_selected`: a gang left in the pool (and here a maintenance spawn to board through).
pub fn can_spawn(world: &World, heavy: bool) -> bool {
    !pool(world, heavy).is_empty()
        && world
            .get_resource::<SpawnPoints>()
            .is_some_and(|s| !s.maintenance.is_empty())
}

/// `max(PAYOFF_MIN, FLOOR(cargo × 0.80, 1000))`.
pub fn payoff(cargo_balance: i64) -> i64 {
    let floored = (cargo_balance as f64 * 0.8 / 1000.0).floor() as i64 * 1000;
    PAYOFF_MIN.max(floored)
}

/// `generate_message`: `%SHIPNAME` and `%PAYOFF` filled in.
pub fn threat_text(template: &str, ship_name: &str, payoff: i64) -> String {
    template
        .replace("%SHIPNAME", ship_name)
        .replace("%PAYOFF", &payoff.to_string())
}

fn ship_name(world: &mut World, pool_key: &str) -> String {
    let names = world
        .get_resource::<super::RaiderData>()
        .map(|d| d.pirate_pool(pool_key).to_vec())
        .unwrap_or_default();
    pick(world, &names).unwrap_or_else(|| "Безымянный корабль".to_owned())
}

/// `send_pirate_threat`: false when the pool is empty.
pub fn send_threat(world: &mut World, heavy: bool) -> bool {
    let gangs = pool(world, heavy);
    let Some(gang_i) = pick(world, &gangs) else {
        warn!("pirates: no gang left in the pool");
        return false;
    };
    world.resource_mut::<PirateState>().taken.push(gang_i);
    let gang = &GANGS[gang_i];
    let balance = world
        .get_resource::<Economy>()
        .and_then(|e| e.department(ACCOUNT_CAR).map(|a| a.balance));
    let payoff = balance.map_or(0, payoff);
    let mut name = ship_name(world, gang.ship_name_pool);
    let (title, content, answers) = if gang.name == SIREN {
        // `false_flag`: the message of another gang, under one of its ship names.
        let others: Vec<usize> = (0..GANGS.len()).filter(|&i| i != gang_i).collect();
        let imitated = &GANGS[pick(world, &others).unwrap_or(0)];
        let fake_ship = ship_name(world, imitated.ship_name_pool);
        (
            imitated.threat_title,
            threat_text(imitated.threat_content, &fake_ship, payoff),
            imitated.answers,
        )
    } else {
        (
            gang.threat_title,
            threat_text(gang.threat_content, &name, payoff),
            gang.answers,
        )
    };
    let mut threat = Threat {
        gang: gang_i,
        ship_name: name.clone(),
        payoff,
        sent_at: now(world),
        paid_off: false,
        answered: false,
        spawned: false,
        received: gang.received.to_owned(),
        rejected: gang.rejected.to_owned(),
        too_late: gang.too_late.to_owned(),
        not_enough: gang.not_enough.to_owned(),
        arrival: gang.arrival.to_owned(),
    };
    if gang.name == SIREN {
        // `reveal_yourself`.
        name = ship_name(world, gang.ship_name_pool);
        threat.ship_name = name.clone();
        siren_replies(world, &mut threat);
    }
    let mut announcement = crate::announce::Priority::new(
        "Входящий подпространственный вызов. Защищенный канал открыт на всех коммуникационных \
         консолях.",
        "Входящее сообщение",
    );
    if let Some(sound) = crate::announce::report_sound(world) {
        announcement = announcement.sound(sound);
    }
    announcement.send(world);
    world.resource_mut::<PirateState>().threats.push(threat);
    crate::announce::send_unique_message(
        world,
        CommsMessage {
            title: title.to_owned(),
            content,
            possible_answers: answers.iter().map(|a| (*a).to_owned()).collect(),
            answered: 0,
        },
        answered,
    );
    info!("pirates: {} threatened the station for {payoff}", gang.name);
    true
}

/// `reveal_yourself`'s replies.
fn siren_replies(world: &mut World, threat: &mut Threat) {
    const NOTES: [&str; 3] = ["\u{266A}", "\u{2669}", "\u{266B}"];
    let ship = threat.ship_name.clone();
    let station = crate::events::station_name(world);
    let mut n = || pick(world, &NOTES).unwrap_or(NOTES[0]);
    let (a, b, c, d) = (n(), n(), n(), n());
    threat.received = format!(
        "{a} Fufu~ You fell for it. We're not {ship} at all! {b} \n{c} Our next show is funded. \
         Sayōnara! {d}"
    );
    let (a, b, c, d) = (n(), n(), n(), n());
    threat.rejected = format!(
        "{a} You must think you're so clever. {b} \n{c} {station} seems like the perfect venue \
         for us anyway!~ {d}"
    );
    let (a, b, c, d) = (n(), n(), n(), n());
    threat.too_late =
        format!("{a} Surprise!~ {b} \n{c} ...Awh- don't worry, the autographs are free! {d}");
    let (a, b, c, d) = (n(), n(), n(), n());
    threat.not_enough = format!(
        "{a}Got you good!~ B-but this is hardly enough, baka! {b} \n{c} We'll discuss payment \
         after the show. {d}"
    );
    let (a, b, c, d) = (n(), n(), n(), n());
    threat.arrival = format!(
        "{a} Ready to play in the greatest performance of your life? {b}\n{c} Ankōru suru hima \
         wa nai! {d}"
    );
}

fn announce_as(world: &mut World, ship: &str, text: &str) {
    crate::announce::Priority::new(text, "")
        .sender(ship.to_owned())
        .send(world);
}

/// `pirates_answered` of the oldest unanswered threat.
// shortcut: the console's answer callback carries no threat, so with two threats out at once
// the first answer goes to the older one.
pub fn answered(world: &mut World, answer: u32) {
    let t = now(world);
    let Some(i) = world
        .resource::<PirateState>()
        .threats
        .iter()
        .position(|th| !th.answered)
    else {
        return;
    };
    let threat = {
        let mut state = world.resource_mut::<PirateState>();
        state.threats[i].answered = true;
        state.threats[i].clone()
    };
    if t > threat.sent_at + RESPONSE_MAX_SECS {
        announce_as(world, &threat.ship_name, &threat.too_late);
        return;
    }
    if answer == 2 {
        announce_as(world, &threat.ship_name, &threat.rejected);
        return;
    }
    if answer != 1 {
        return;
    }
    let paid = world.get_resource_mut::<Economy>().is_some_and(|mut e| {
        e.department_mut(ACCOUNT_CAR)
            .is_some_and(|a| a.adjust_money(-threat.payoff))
    });
    if paid {
        world.resource_mut::<PirateState>().threats[i].paid_off = true;
        announce_as(world, &threat.ship_name, &threat.received);
    } else {
        announce_as(world, &threat.ship_name, &threat.not_enough);
    }
}

/// `addtimer(spawn_pirates, RESPONSE_MAX_TIME)` of every threat.
fn tick(world: &mut World) {
    let t = now(world);
    let due: Vec<usize> = match world.get_resource::<PirateState>() {
        Some(s) => s
            .threats
            .iter()
            .enumerate()
            .filter(|(_, th)| !th.spawned && t >= th.sent_at + RESPONSE_MAX_SECS)
            .map(|(i, _)| i)
            .collect(),
        None => return,
    };
    for i in due {
        let threat = {
            let mut state = world.resource_mut::<PirateState>();
            state.threats[i].spawned = true;
            state.threats[i].clone()
        };
        if threat.paid_off {
            continue;
        }
        poll_crew(world, threat);
    }
}

/// `spawn_pirates`: the poll, then the crew.
fn poll_crew(world: &mut World, threat: Threat) {
    let gang = &GANGS[threat.gang];
    let spec = PollSpec {
        question: format!(
            "Хотите ли вы стать членом пиратской команды «{}»?",
            gang.name
        ),
        role_name: "пиратская команда".to_owned(),
        secs: DEFAULT_POLL_SECS,
        ..default()
    };
    let group = ghost_poll::ghosts(world);
    ghost_poll::poll_candidates(world, spec, group, move |world, signed_up| {
        spawn_crew(world, &threat, signed_up);
    });
}

/// The rest of `spawn_pirates`: volunteers into the spawners, leader first, and the arrival.
fn spawn_crew(world: &mut World, threat: &Threat, mut candidates: Vec<Candidate>) {
    let gang = &GANGS[threat.gang];
    // The boarding party comes in through maintenance (see the shortcut above).
    let tiles: Vec<TilePos> = world
        .get_resource::<SpawnPoints>()
        .map(|s| s.maintenance.clone())
        .unwrap_or_default();
    let spawns: Vec<TilePos> = pick(world, &tiles).into_iter().collect();
    let spawners: Vec<&'static str> = gang
        .spawners
        .iter()
        .flat_map(|&(path, n)| std::iter::repeat_n(path, n as usize))
        .collect();
    let mut spawned = 0;
    for spawner in spawners {
        if spawns.is_empty() {
            warn!("pirates: no maintenance spawn to put the crew on");
            break;
        }
        let Some(chosen) = ghost_poll::pick_candidate(world, &mut candidates) else {
            break;
        };
        let pos = spawns[spawned % spawns.len()];
        if spawn_pirate(world, chosen, spawner, pos).is_some() {
            spawned += 1;
        }
    }
    info!("pirates: {} spawned {spawned} crew", gang.name);
    announce_as(world, &threat.ship_name, &threat.arrival);
}

/// `create_from_ghost` of a pirate spawner at `pos`: the human, its name, the antagonist.
pub fn spawn_pirate(
    world: &mut World,
    chosen: Candidate,
    spawner: &str,
    pos: TilePos,
) -> Option<Entity> {
    let Some(def) = crate::ghost_roles::def(world, spawner) else {
        warn!("{spawner}: no mob spawner data (rerun `ss13-mapload convert`)");
        return None;
    };
    let name = pirate_name(world, spawner);
    let mob = crate::ghost_roles::create_human(world, &def, pos, name);
    if !crate::medical::brainmob::possess(world, chosen.client, chosen.mob, mob) {
        world.despawn(mob);
        return None;
    }
    world.despawn(chosen.mob);
    crate::noncrew_jobs::set_assigned_role(world, mob, JOB);
    let key = world
        .get::<ClientToken>(chosen.client)
        .map(|t| t.0.clone())
        .unwrap_or_default();
    world.entity_mut(mob).insert(Pirate { key });
    world.resource_mut::<PirateState>().crew.push(mob);
    // The spawner's `special` box, then `/datum/antagonist/pirate/greet`.
    for (span, text) in [
        (Span::BoldNotice, def.you_are_text.clone()),
        (Span::Notice, def.flavour_text.clone()),
        (Span::Danger, def.important_text.clone()),
    ] {
        if !text.is_empty() {
            tell(world, chosen.client, span, text);
        }
    }
    tell(
        world,
        chosen.client,
        Span::Bold,
        "Станция отказалась платить за вашу защиту. Защищайте корабль, выкачивайте кредиты со \
         станции и совершайте набеги ради ещё большей добычи.",
    );
    super::announce_objectives(world, chosen.client, &[LOOT_TEXT.to_owned()]);
    info!("{mob} joined the pirate crew as {spawner}");
    Some(mob)
}

/// `generate_pirate_name` of the spawner type.
fn pirate_name(world: &mut World, spawner: &str) -> String {
    let (rank, style) = rank_of(spawner);
    let names = world
        .get_resource::<crate::jobs::Jobs>()
        .map(|j| j.data.names.clone())
        .unwrap_or_default();
    match style {
        NameStyle::Generic => {
            let (begin, end) = world
                .get_resource::<super::RaiderData>()
                .map(|d| {
                    (
                        d.pirate_pool("generic_beginnings").to_vec(),
                        d.pirate_pool("generic_endings").to_vec(),
                    )
                })
                .unwrap_or_default();
            let b = pick(world, &begin).unwrap_or_default();
            let e = pick(world, &end).unwrap_or_default();
            format!("{rank} {b}{e}")
        }
        NameStyle::Silverscale => {
            let all: Vec<String> = names
                .first_male
                .iter()
                .chain(&names.first_female)
                .cloned()
                .collect();
            let first = pick(world, &all).unwrap_or_default();
            format!("{rank} {first}-Серебрянная чешуя")
        }
        NameStyle::FirstName => {
            let female = with_rng(world, |rng| rng.prob(50.0));
            let pool = if female {
                names.first_female
            } else {
                names.first_male
            };
            let first = pick(world, &pool).unwrap_or_default();
            format!("{rank} {first}")
        }
        NameStyle::FemaleFirst => {
            let first = pick(world, &names.first_female).unwrap_or_default();
            format!("{rank} {first}")
        }
    }
}

/// `/datum/team/pirate/roundend_report`.
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let crew = world
        .get_resource::<PirateState>()
        .map(|s| s.crew.clone())
        .unwrap_or_default();
    let crew: Vec<Entity> = crew
        .into_iter()
        .filter(|&m| world.get::<Pirate>(m).is_some())
        .collect();
    if crew.is_empty() {
        return Vec::new();
    }
    let mut out = vec![line(vec![ChatSegment::new(
        Span::Bold,
        "Космическими пиратами были:",
    )])];
    let mut all_dead = true;
    for &mob in &crew {
        let key = world
            .get::<Pirate>(mob)
            .map(|p| p.key.clone())
            .unwrap_or_default();
        out.push(super::super::traitor::print_player(world, mob, &key));
        if world
            .get::<crate::health::Stat>(mob)
            .is_some_and(|s| *s != crate::health::Stat::Dead)
            && world.get::<PlayerMob>(mob).is_some()
        {
            all_dead = false;
        }
    }
    let loot = 0;
    out.push(line(vec![ChatSegment::new(
        Span::Plain,
        "Похищенная добыча: ничего",
    )]));
    out.push(line(vec![ChatSegment::new(
        Span::Plain,
        format!("Общая стоимость добычи: {loot}/{LOOT_TARGET} кредитов"),
    )]));
    out.push(line(vec![if loot >= LOOT_TARGET && !all_dead {
        ChatSegment::new(Span::GreenText, "Пиратская команда преуспела!")
    } else {
        ChatSegment::new(Span::RedText, "Пиратская команда провалилась.")
    }]));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_payoff_is_four_fifths_of_cargo_floored_with_a_minimum() {
        assert_eq!(payoff(0), PAYOFF_MIN);
        assert_eq!(payoff(30_000), PAYOFF_MIN);
        assert_eq!(payoff(100_000), 80_000);
        assert_eq!(payoff(101_999), 81_000);
    }

    #[test]
    fn gangs_split_into_tgs_pools() {
        let light: Vec<&str> = GANGS.iter().filter(|g| !g.heavy).map(|g| g.name).collect();
        let heavy: Vec<&str> = GANGS.iter().filter(|g| g.heavy).map(|g| g.name).collect();
        assert_eq!(
            light,
            [
                "Rogues",
                "Silverscales",
                "The Grey Tide",
                "Geode Scavengers",
                SIREN
            ]
        );
        assert_eq!(
            heavy,
            [
                "Skeleton Pirates",
                "Restless Ex-Pharmacists",
                "Space IRS Agents",
                "Medieval Warmongers"
            ]
        );
        // The leader's spawner comes first and every spawner type is a pirate one.
        for g in &GANGS {
            assert!(g.spawners.iter().all(|(p, _)| p.starts_with(SPAWNER_ROOT)));
        }
        assert!(GANGS[0].spawners[0].0.ends_with("/captain"));
    }

    #[test]
    fn threats_fill_in_ship_and_payoff() {
        let text = threat_text(GANGS[2].threat_content, "Летучий Голландец", 25_000);
        assert!(text.contains("Летучий Голландец"));
        assert!(text.contains("25000"));
        assert!(!text.contains('%'));
    }

    #[test]
    fn ranks_follow_the_spawner_types() {
        assert_eq!(
            rank_of("/obj/effect/mob_spawn/ghost_role/human/pirate/skeleton/captain").0,
            "Капитан"
        );
        assert_eq!(
            rank_of("/obj/effect/mob_spawn/ghost_role/human/pirate").0,
            "Дезертир"
        );
        assert_eq!(
            rank_of("/obj/effect/mob_spawn/ghost_role/human/pirate/grey/shitter").0,
            "Тайд-мастер"
        );
    }

    #[test]
    fn a_late_or_refused_answer_pays_nothing() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let mut economy = Economy::new(1);
        economy.department_mut(ACCOUNT_CAR).unwrap().balance = 100_000;
        world.insert_resource(economy);
        let threat = |sent_at| Threat {
            gang: 0,
            ship_name: "Корабль".into(),
            payoff: 80_000,
            sent_at,
            paid_off: false,
            answered: false,
            spawned: false,
            received: String::new(),
            rejected: String::new(),
            too_late: String::new(),
            not_enough: String::new(),
            arrival: String::new(),
        };
        world.insert_resource(PirateState {
            taken: vec![0],
            threats: vec![threat(0.0), threat(0.0)],
            crew: Vec::new(),
        });
        // Refused: the first threat, no money moves.
        answered(&mut world, 2);
        let balance = |w: &World| {
            w.resource::<Economy>()
                .department(ACCOUNT_CAR)
                .unwrap()
                .balance
        };
        assert_eq!(balance(&world), 100_000);
        assert!(world.resource::<PirateState>().threats[0].answered);
        // Paid: the second one.
        answered(&mut world, 1);
        assert_eq!(balance(&world), 20_000);
        assert!(world.resource::<PirateState>().threats[1].paid_off);
    }
}
