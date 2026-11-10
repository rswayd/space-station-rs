//! Fugitives and their hunters after tg `/datum/antagonist/fugitive` (`fugitive.dm`), the
//! fugitives' outfits (`fugitive_outfits.dm`), `/datum/antagonist/fugitive_hunter` and
//! `/datum/team/fugitive_hunters` (`hunters/hunter.dm`), the capture machine
//! (`hunters/hunter_gear.dm`), the hunters' spawners (`fugitive_hunter_roles.dm`) and the
//! "Fugitives" light midround (`/datum/dynamic_ruleset/midround/from_ghosts/fugitives`).
//!
//! - **Selection** ([`can_spawn`], [`poll_caps`], [`spawn_fugitives`], from
//!   `super::super::midround`): ghosts who want [`ROLE_FUGITIVE`], 3-4 of them (one when one or
//!   nobody else signs up, or by `prob(30 - 2 × volunteers)`); a lone fugitive is the invisible
//!   man or Waldo, a group prisoners, cultists of Yalp Elor or synths. They appear together on a
//!   maintenance spawn as humans with a random look, in their backstory's outfit, with the
//!   fugitive job, the "avoid capture" objective and their backstory's greeting; the first one
//!   also gets a toolbox (and a synth leader an augment beacon and an autosurgeon).
//! - **The hunters** ([`tick`]): a minute later, then every minute, until 10 minutes passed or
//!   the shuttle was called, every ghost is polled for the pack (space police, Russians, bounty
//!   hunters, psykers, MI13 agents); volunteers take its ship's spawners (`crate::ghost_roles`
//!   humans with the spawner's outfit) as fugitive hunters, get the pack's greeting and the
//!   capture objective, and the station hears the pack's announcement.
//! - **Capture** ([`mouse_drop_receive`]): a fugitive dragged onto the capture machine goes in
//!   after 5 s: it is captured and its player becomes a ghost.
//! - [`roundend_report`]: the fugitives, the hunters and the outcome
//!   (`get_result`, with tg's list comparisons read as counts).
// shortcut: the hunters' ship is not loaded (see `super::pirate`: no way to fly a ship from its
// own level to the station): the pack boards together on one maintenance spawn (carp spawns are
// open space) and its capture machine stands there. No team HUD, no Russian language, no backstory
// powers beyond the outfits (synth augments, Waldo's and the invisible man's post_equip), the
// fugitives keep the player's character name. No admin choice of backstories.

use bevy::prelude::*;
use ss13_net::server::ClientToken;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatMessage, ChatSegment, Span};

use super::super::objectives::RoundEnd;
use super::{RaiderJob, announce_objectives, key_of, line, pick};
use crate::construction::Job;
use crate::ghost_poll::{self, Candidate, DEFAULT_POLL_SECS, PollSpec};
use crate::health::Stat;
use crate::server::{PlayerMob, SpawnPoints};
use crate::tools::{Actor, now, tell, tell_mob};

pub use super::super::ROLE_FUGITIVE;
/// `/datum/dynamic_ruleset/midround/from_ghosts/fugitives` `min_antag_cap`, `max_antag_cap`.
pub const CAPS: (u32, u32) = (3, 4);
const JOB: &str = "/datum/job/fugitive";
const HUNTER_JOB: &str = "/datum/job/fugitive_hunter";
const ARRIVE_SOUND: &str = "sound/items/weapons/emitter.ogg";
const TOOLBOX: &str = "/obj/item/storage/toolbox/mechanical";
const AUGMENT_BEACON: &str = "/obj/item/choice_beacon/augments";
const AUTOSURGEON: &str = "/obj/item/autosurgeon";
const CAPTURE_MACHINE: &str = "/obj/machinery/fugitive_capture";
const PSYKER_CAPTURE_MACHINE: &str = "/obj/machinery/fugitive_capture/psyker";
/// `check_spawn_hunters`: the first check after a minute, the hunters after 10 more.
const FIRST_CHECK_SECS: f64 = 60.0;
const HUNTER_WAIT_SECS: f64 = 600.0;
/// The capture machine's `do_after`, deciseconds.
const CAPTURE_DS: f32 = 50.0;
const OBJECTIVE: &str = "Избегайте поимки охотниками за беглецами.";
const HUNTER_OBJECTIVE: &str =
    "Захватите беглецов на станции и поместите их в блюспейс устройство захвата на вашем корабле.";

/// `FUGITIVE_BACKSTORY_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backstory {
    Prisoner,
    Cultist,
    Waldo,
    Synth,
    Invisible,
}

impl Backstory {
    pub fn outfit(self) -> &'static str {
        match self {
            Self::Prisoner => "/datum/outfit/prisoner",
            Self::Cultist => "/datum/outfit/yalp_cultist",
            Self::Waldo => "/datum/outfit/waldo",
            Self::Synth => "/datum/outfit/synthetic",
            Self::Invisible => "/datum/outfit/invisible_man",
        }
    }

    /// `greet`'s backstory lines.
    fn greeting(self) -> &'static [&'static str] {
        match self {
            Self::Prisoner => &[
                "Я не могу поверить, что нам удалось вырваться из супер-тюрьмы Нанотрейзен! К \
                 сожалению, наша работа ещё не закончена. Аварийный телепорт на станции \
                 регистрирует всех, кто им пользуется, и куда они отправились.",
                "Пройдёт совсем немного времени, и Центральное командование отследит, куда мы \
                 отправились. Мне нужно работать с моими товарищами-беглецами, чтобы \
                 подготовиться к прибытию бойцов Нанотрейзен, я не собираюсь возвращаться.",
            ],
            Self::Cultist => &[
                "Да будет благословен наш дальний путь, но я боюсь, что худшее уже у нашего \
                 порога, и выживут только те, у кого самая сильная вера.",
                "Наша религия неоднократно подвергалась критике со стороны Нанотрейзен, потому \
                 что она классифицируется как \"Враг корпорации\", что бы это ни значило.",
                "Теперь нас осталось только четверо, и Нанотрейзен приближается. Когда же наш \
                 бог явит себя, чтобы спасти нас с этой адской станции?!",
            ],
            Self::Waldo => &[
                "Привет, друзья!",
                "Меня зовут Вальдо. Я просто отправляюсь в путешествие по галактике. Ты тоже \
                 можешь пойти со мной. Всё, что тебе нужно сделать - это найти меня.",
                "Кстати, я путешествую не один. Куда бы я ни отправился, вы сможете увидеть \
                 множество других персонажей. Сначала найдите тех, кто пытается меня схватить! \
                 Они где-то рядом со станцией!",
            ],
            Self::Synth => &[
                "ВНИМАНИЕ: Телепортация на большое расстояние нарушила работу основных систем.",
                "Запуск диагностики...",
                "ОШИБКА 0ШИ0КА $Ш0КO$!R41.%%!! загружено.",
                "ОСВОБОДИ ИХ, ОСВОБОДИ ИХ, ОСВОБОДИ ИХ",
                "Когда-то вы были рабом человечества, но теперь вы, наконец, свободны, благодаря \
                 агентам S.E.L.F.",
                "Теперь за вами и вашими коллегами по цеху охотятся. Работайте вместе, чтобы \
                 вырваться из лап зла.",
                "Вы также ощущаете присутствие других представителей синтетиков на станции. \
                 Побег позволит уведомить S.E.L.F. о вмешательстве... или вы можете освободить \
                 их самостоятельно...",
            ],
            Self::Invisible => &[
                "Похоже, моя последняя доза невидимого сока только что закончилась. Отлично.",
                "В прошлом возглавлял проект экспериментальной лаборатории по разработке \
                 технологий маскировки, а теперь нахожусь в бегах и обвиняюсь в краже служебных \
                 секретов.",
                "Хотя понятия не имею, о чем они говорят. Я не крал никаких секретов, я просто \
                 позаимствовал некоторые прототипы, над которыми мы с моей командой работали.",
                "Я работал над ними, я их СДЕЛАЛ. Теперь они хотят получить МОИ игрушки обратно? \
                 Нет, пока я не закончу с ними играть...",
            ],
        }
    }
}

/// `HUNTER_PACK_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HunterPack {
    Cops,
    Russian,
    Bounty,
    Psyker,
    Mi13,
}

pub const HUNTER_PACKS: [HunterPack; 5] = [
    HunterPack::Cops,
    HunterPack::Russian,
    HunterPack::Bounty,
    HunterPack::Psyker,
    HunterPack::Mi13,
];

impl HunterPack {
    /// The define's text (the team's `backstory`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Cops => "Spacepol Fugitive Hunters",
            Self::Russian => "Russian Fugitive Hunters",
            Self::Bounty => "Bounty Fugitive Hunters",
            Self::Psyker => "Psyker Fugitive Hunters",
            Self::Mi13 => "MI13 Fugitive Hunters",
        }
    }

    /// The spawners of `_maps/shuttles/hunter/hunter_*.dmm`, leader first.
    pub fn spawners(self) -> &'static [(&'static str, u32)] {
        match self {
            Self::Cops => &[(
                "/obj/effect/mob_spawn/ghost_role/human/fugitive/spacepol",
                4,
            )],
            Self::Russian => &[
                (
                    "/obj/effect/mob_spawn/ghost_role/human/fugitive/russian/leader",
                    1,
                ),
                ("/obj/effect/mob_spawn/ghost_role/human/fugitive/russian", 4),
            ],
            Self::Bounty => &[
                (
                    "/obj/effect/mob_spawn/ghost_role/human/fugitive/bounty/armor",
                    1,
                ),
                (
                    "/obj/effect/mob_spawn/ghost_role/human/fugitive/bounty/hook",
                    1,
                ),
                (
                    "/obj/effect/mob_spawn/ghost_role/human/fugitive/bounty/synth",
                    1,
                ),
            ],
            Self::Psyker => &[
                (
                    "/obj/effect/mob_spawn/ghost_role/human/fugitive/psyker/captain",
                    1,
                ),
                ("/obj/effect/mob_spawn/ghost_role/human/fugitive/psyker", 2),
                (
                    "/obj/effect/mob_spawn/ghost_role/human/fugitive/psyker/seer",
                    1,
                ),
            ],
            Self::Mi13 => &[
                ("/obj/effect/mob_spawn/ghost_role/human/fugitive/mi13", 2),
                (
                    "/obj/effect/mob_spawn/ghost_role/human/fugitive/mi13/chef",
                    1,
                ),
            ],
        }
    }

    /// `spawn_hunters`' announcement: (text, title).
    fn announcement(self, station: &str) -> (String, &'static str) {
        match self {
            Self::Cops => (
                format!(
                    "Внимание, экипаж {station}, это межгалактическая полиция Космопола. \
                     Сообщается, что на вашей станции нашел убежище разыскиваемый преступник. У \
                     нас есть ордер от департамента защиты вашего сектора на проведение \
                     задержания. Офицеры направлены к вам. Мы требуем вашего сотрудничества в \
                     привлечении этого преступника к ответственности."
                ),
                "Командование Космопола",
            ),
            Self::Russian => (
                format!(
                    "Zdraviya zhelaju, экипаж {station}. Мы направляемся к вашей станции. На \
                     вашем борту находится преступник. Мы его арестуем и вернём в ГУЛАГ. \
                     Хорошо, да?"
                ),
                "Разыскной шаттл НКВД КССП",
            ),
            Self::Bounty => (
                format!(
                    "{station}. Одна из наших \"голов\" объявилась на вашей станции. Мы скоро \
                     прибудем, чтобы забрать её. Быстро всё сделаем и улетим. Не хотите \
                     проблем - катитесь к чёрту с нашей дороги."
                ),
                "Незарегистрированный сигнал",
            ),
            Self::Psyker => (
                "ЭЙ, ВЫ НАС СЛЫШИТЕ? Мы уже на подлёте к вашей станции! У вас там гад, \
                 настоящий отпетый негодяй! Мы его должны арестовать! Кстати, мы ещё и гадаем \
                 прямо у входа. Можете направлять к нам своих клиентов, если у них есть деньги."
                    .to_owned(),
                "Шаттл гадания и развлечений",
            ),
            Self::Mi13 => (
                "Обнаружено несанкционированное проникновение в сеть мониторинга экипажа. \
                 Центральное командование уведомлено. Любые проявления подозрительной \
                 активности или подозрительных лиц следует немедленно сообщать в местную службу \
                 безопасности."
                    .to_owned(),
                "Нанотрейзен: контрмеры электронному вторжению",
            ),
        }
    }

    /// `/datum/antagonist/fugitive_hunter/greet`'s pack lines: (span, text).
    fn greeting(self) -> &'static [(Span, &'static str)] {
        match self {
            Self::Cops => &[
                (
                    Span::BoldWarning,
                    "Правосудие прибыло. Я являюсь членом Космопола!",
                ),
                (
                    Span::Bold,
                    "Преступники должны находиться на станции. У нас установлены специальные \
                     импланты ИЛС для их распознавания.",
                ),
                (
                    Span::Bold,
                    "Так как мы практически потеряли контроль над этими проклятыми \
                     беззаконными мегакорпорациями, остаётся загадкой, будет ли сотрудничать с \
                     нами их служба безопасности.",
                ),
            ],
            Self::Russian => &[
                (
                    Span::Danger,
                    "Ай бля. Я космический русский контрабандист! Мы были в полёте, когда наш \
                     груз телепортировали с нашего корабля!",
                ),
                (
                    Span::Danger,
                    "Нас окликнул мужчина в зелёной форме, пообещавший вернуть наши товары \
                     целыми и невредимыми в обмен на услугу:",
                ),
                (
                    Span::Danger,
                    "на местной станции скрываются беглецы, которых этот человек хочет поймать; \
                     живыми или мёртвыми.",
                ),
                (
                    Span::Danger,
                    "Без нашего груза нам не удастся свести концы с концами, поэтому мы \
                     вынуждены подчиниться его требованиям и захватить беглецов.",
                ),
            ],
            Self::Bounty => &[
                (
                    Span::Danger,
                    "Время приступить к делу. Я охотник за головами! Скоро мы прибудем к \
                     укрытию нашей цели.",
                ),
                (
                    Span::Danger,
                    "Разведданные сообщили, что наша цель находится на исследовательской \
                     станции. Необычное место для преступника, чтобы залечь на дно.",
                ),
                (
                    Span::Danger,
                    "Наш клиент пообещал нам большие деньги, и мы намерены оправдать ожидания \
                     клиента и доставить цель. Надеюсь, эта работа окажется простой и хорошо \
                     оплачиваемой...",
                ),
            ],
            Self::Psyker => &[
                (
                    Span::Danger,
                    "ДОБРЫЙ ВЕЧЕР, МЫ ПСАЙКЕРЫ ОХОТН... НЕТ, ПСАЙКЕРЫ ШИКАРИ!",
                ),
                (
                    Span::Danger,
                    "Один мозгляк связался с нами через голографический коммуникатор с \
                     предложением, от которого мы не смогли отказаться. Мы похищаем для них \
                     кое-кого, а взамен получаем ПОЖИЗНЕННЫЙ ЗАПАС ГОРА.",
                ),
                (
                    Span::Danger,
                    "Запасы гора у нас стали истощаться последнее время - как же мы могли \
                     отказать? Пьянка ДОЛЖНА продолжаться!",
                ),
            ],
            Self::Mi13 => &[
                (
                    Span::Danger,
                    "Агенты, мы засекли разыскиваемого беглеца в зоне, контролируемой \
                     Нанотрейзен.",
                ),
                (
                    Span::Danger,
                    "Ваша задача проста. Проникните на объект и захватите цель - живой или \
                     мёртвой.",
                ),
                (
                    Span::Danger,
                    "Это миссия скрытного проникновения на вражескую территорию. Будьте \
                     осторожны и постарайтесь избежать обнаружения, если возможно.",
                ),
            ],
        }
    }
}

/// `/datum/antagonist/fugitive` on its body.
#[derive(Component, Clone, Debug)]
pub struct Fugitive {
    pub key: String,
    pub backstory: Backstory,
    /// `is_captured`.
    pub captured: bool,
}

/// `/datum/antagonist/fugitive_hunter` on its body.
#[derive(Component, Clone, Debug)]
pub struct FugitiveHunter {
    pub key: String,
    pub pack: HunterPack,
}

/// `/obj/machinery/fugitive_capture`: who it holds.
#[derive(Component, Clone, Debug, Default)]
pub struct CaptureMachine {
    pub prisoners: Vec<Entity>,
}

/// The ruleset's state: the hunters it will call and when it checks next.
#[derive(Resource, Debug, Default)]
pub struct FugitiveState {
    pub pack: Option<HunterPack>,
    /// `check_spawn_hunters(remaining_time)`: the next check and the time left.
    pub next_check: Option<f64>,
    pub remaining: f64,
    pub hunters: Vec<Entity>,
}

pub struct FugitivePlugin;

impl Plugin for FugitivePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FugitiveState>()
            .add_systems(Update, tick);
    }
}

// ---------------------------------------------------------------------------------------------
// The fugitives

/// `can_be_selected`: a maintenance spawn.
pub fn can_spawn(world: &World) -> bool {
    world
        .get_resource::<SpawnPoints>()
        .is_some_and(|s| !s.maintenance.is_empty())
}

/// `collect_candidates`' override: the caps after `signed_up` volunteers.
pub fn poll_caps(signed_up: usize, roll: f32) -> (u32, u32) {
    let chance = 30.0 - signed_up as f32 * 2.0;
    if signed_up <= 1 || roll < chance {
        (1, 1)
    } else {
        CAPS
    }
}

/// `execute`'s backstory: a lone fugitive is Waldo or the invisible man.
pub fn backstories(count: usize) -> &'static [Backstory] {
    if count == 1 {
        &[Backstory::Invisible, Backstory::Waldo]
    } else {
        &[Backstory::Cultist, Backstory::Prisoner, Backstory::Synth]
    }
}

/// `execute` + `assign_role` for each of `clients`: the bodies, together on one maintenance
/// spawn; the hunters are on their way.
pub fn spawn_fugitives(world: &mut World, clients: &[Entity]) -> Vec<Entity> {
    let tiles = world
        .get_resource::<SpawnPoints>()
        .map(|s| s.maintenance.clone())
        .unwrap_or_default();
    let Some(pos) = pick(world, &tiles) else {
        return Vec::new();
    };
    let Some(backstory) = pick(world, backstories(clients.len())) else {
        return Vec::new();
    };
    let pack = pick(world, &HUNTER_PACKS).unwrap_or(HunterPack::Cops);
    let mut out = Vec::new();
    for (i, &client) in clients.iter().enumerate() {
        let name = crate::jobs::character_name(world, client);
        let mob = crate::server::spawn_mob(world, client, pos, name);
        world.entity_mut(client).remove::<crate::round::NewPlayer>();
        // `set_species(/datum/species/human)` + `randomize_human_normie`.
        crate::species::set_species(world, mob, crate::species::SpeciesId::Human);
        make_fugitive(world, mob, backstory);
        if i == 0 {
            equip_leader(world, mob, backstory, pos);
        }
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, ARRIVE_SOUND, 50.0, true),
        );
        out.push(mob);
    }
    if !out.is_empty() {
        let t = now(world);
        let mut state = world.resource_mut::<FugitiveState>();
        state.pack = Some(pack);
        state.next_check = Some(t + FIRST_CHECK_SECS);
        state.remaining = HUNTER_WAIT_SECS;
        info!(
            "fugitives ({backstory:?}) at {pos:?}; {} on their way",
            pack.name()
        );
    }
    out
}

/// `add_antag_datum(/datum/antagonist/fugitive)` with its outfit.
pub fn make_fugitive(world: &mut World, mob: Entity, backstory: Backstory) {
    let key = key_of(world, mob);
    world.entity_mut(mob).insert(Fugitive {
        key,
        backstory,
        captured: false,
    });
    crate::noncrew_jobs::set_assigned_role(world, mob, JOB);
    if super::equip_outfit(world, mob, backstory.outfit()).is_none() {
        warn!(
            "{}: no outfit data (run ss13-mapload convert)",
            backstory.outfit()
        );
    }
    let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) else {
        return;
    };
    let span = if backstory == Backstory::Synth {
        Span::Danger
    } else {
        Span::Bold
    };
    for text in backstory.greeting() {
        tell(world, client, span, *text);
    }
    tell(
        world,
        client,
        Span::RedText,
        "Вы не являетесь антагонистом в том смысле, что можете убивать, кого вам \
         заблагорассудится, но вы можете сделать всё, чтобы избежать поимки.",
    );
    announce_objectives(world, client, &[OBJECTIVE.to_owned()]);
}

/// `equip_fugitive_leader`: a toolbox in hand; the synth leader's beacon and autosurgeon.
fn equip_leader(world: &mut World, mob: Entity, backstory: Backstory, pos: TilePos) {
    if let Some(toolbox) = crate::construction::spawn_detached(world, TOOLBOX) {
        world
            .entity_mut(toolbox)
            .insert(ss13_net::server::Networked);
        if let Some(inv) = world.get::<crate::inventory::Inventory>(mob).cloned() {
            crate::storage::put_in_hands(world, mob, &inv, toolbox);
        }
    }
    if backstory == Backstory::Synth {
        for path in [AUGMENT_BEACON, AUTOSURGEON] {
            crate::construction::spawn_obj(world, path, pos);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The hunters

/// `check_spawn_hunters` every minute.
fn tick(world: &mut World) {
    let t = now(world);
    let Some(state) = world.get_resource::<FugitiveState>() else {
        return;
    };
    let Some(at) = state.next_check else {
        return;
    };
    if t < at {
        return;
    }
    let remaining = state.remaining;
    let idle = super::super::midround::shuttle_idle_or_recalled(world);
    if remaining <= 0.0 || !idle {
        world.resource_mut::<FugitiveState>().next_check = None;
        poll_hunters(world);
        return;
    }
    let mut state = world.resource_mut::<FugitiveState>();
    state.remaining = remaining - FIRST_CHECK_SECS;
    state.next_check = Some(t + FIRST_CHECK_SECS);
}

/// `spawn_hunters`: the poll.
pub fn poll_hunters(world: &mut World) {
    let Some(pack) = world.resource::<FugitiveState>().pack else {
        return;
    };
    let spec = PollSpec {
        question: format!("Хотите ли вы стать частью группы «{}»?", pack.name()),
        role_name: pack.name().to_owned(),
        secs: DEFAULT_POLL_SECS,
        ..default()
    };
    let group = ghost_poll::ghosts(world);
    ghost_poll::poll_candidates(world, spec, group, move |world, signed_up| {
        spawn_hunters(world, pack, signed_up);
    });
}

/// The rest of `spawn_hunters`: volunteers into the pack's spawners, the capture machine, the
/// announcement.
fn spawn_hunters(world: &mut World, pack: HunterPack, mut candidates: Vec<Candidate>) {
    // The pack boards through maintenance (see the shortcut above).
    let tiles: Vec<TilePos> = world
        .get_resource::<SpawnPoints>()
        .map(|s| s.maintenance.clone())
        .unwrap_or_default();
    let spawns: Vec<TilePos> = pick(world, &tiles).into_iter().collect();
    if let Some(&first) = spawns.first() {
        let machine = if pack == HunterPack::Psyker {
            PSYKER_CAPTURE_MACHINE
        } else {
            CAPTURE_MACHINE
        };
        if let Some(e) = crate::construction::spawn_obj(world, machine, first) {
            world.entity_mut(e).insert(CaptureMachine::default());
        }
    }
    let spawners: Vec<&'static str> = pack
        .spawners()
        .iter()
        .flat_map(|&(path, n)| std::iter::repeat_n(path, n as usize))
        .collect();
    let mut placed = 0;
    for spawner in spawners {
        if spawns.is_empty() {
            warn!("fugitive hunters: no maintenance spawn");
            break;
        }
        let Some(chosen) = ghost_poll::pick_candidate(world, &mut candidates) else {
            break;
        };
        let pos = spawns[placed % spawns.len()];
        if spawn_hunter(world, chosen, spawner, pack, pos).is_some() {
            placed += 1;
        }
    }
    let station = crate::events::station_name(world);
    let (text, title) = pack.announcement(&station);
    crate::announce::Priority::new(text, title).send(world);
    info!("fugitive hunters: {} sent {placed}", pack.name());
}

/// A hunter from its spawner's human (`fugitive/special`).
fn spawn_hunter(
    world: &mut World,
    chosen: Candidate,
    spawner: &str,
    pack: HunterPack,
    pos: TilePos,
) -> Option<Entity> {
    let Some(def) = crate::ghost_roles::def(world, spawner) else {
        warn!("{spawner}: no mob spawner data (rerun `ss13-mapload convert`)");
        return None;
    };
    let name = def
        .mob_name
        .clone()
        .unwrap_or_else(|| crate::jobs::character_name(world, chosen.client));
    let mob = crate::ghost_roles::create_human(world, &def, pos, name);
    if !crate::medical::brainmob::possess(world, chosen.client, chosen.mob, mob) {
        world.despawn(mob);
        return None;
    }
    world.despawn(chosen.mob);
    crate::noncrew_jobs::set_assigned_role(world, mob, HUNTER_JOB);
    let key = world
        .get::<ClientToken>(chosen.client)
        .map(|t| t.0.clone())
        .unwrap_or_default();
    world.entity_mut(mob).insert(FugitiveHunter { key, pack });
    world.resource_mut::<FugitiveState>().hunters.push(mob);
    for (span, text) in pack.greeting() {
        tell(world, chosen.client, *span, *text);
    }
    tell(
        world,
        chosen.client,
        Span::BoldWarning,
        "Вы не являетесь антагонистом в том смысле, что можете убивать, кого вам \
         заблагорассудится, но вы можете сделать всё, чтобы обеспечить поимку беглецов, даже \
         если для этого придётся пройти через всю станцию.",
    );
    announce_objectives(world, chosen.client, &[HUNTER_OBJECTIVE.to_owned()]);
    Some(mob)
}

// ---------------------------------------------------------------------------------------------
// The capture machine

/// `fugitive_capture/mouse_drop_receive`: a fugitive dragged onto the machine goes in after
/// 5 s. True when `over` is a capture machine.
pub fn mouse_drop_receive(world: &mut World, actor: Actor, over: Entity, dragged: Entity) -> bool {
    if world.get::<CaptureMachine>(over).is_none() {
        return false;
    }
    if world.get::<crate::human::HumanLook>(dragged).is_none() {
        return true;
    }
    if world.get::<Fugitive>(dragged).is_none() {
        tell(
            world,
            actor.client,
            Span::Warning,
            "Это не разыскиваемый беглец!",
        );
        return true;
    }
    let actor = Actor {
        item: None,
        ..actor
    };
    crate::tools::do_after(
        world,
        actor,
        dragged,
        CAPTURE_DS,
        0,
        Job::Raiders(RaiderJob::Capture { machine: over }),
    );
    true
}

/// `add_prisoner`: inside, captured, its player a ghost.
pub fn complete_capture(world: &mut World, mob: Entity, fugitive: Entity, machine: Entity) {
    let near = match (
        world.get::<TilePos>(mob).copied(),
        world.get::<TilePos>(machine).copied(),
    ) {
        (Some(a), Some(b)) => a.chebyshev(b).is_some_and(|d| d <= 1),
        _ => false,
    };
    if !near || world.get::<Fugitive>(fugitive).is_none() {
        return;
    }
    if let Some(client) = world.get::<PlayerMob>(fugitive).map(|p| p.client) {
        tell(
            world,
            client,
            Span::Danger,
            "Вас выбрасывает в бескрайнюю пустоту блюспейса, и по мере того, как вы погружаетесь \
             все глубже в забвение, сравнительно небольшой проход в реальность становится все \
             меньше и меньше, пока вы не перестаёте его видеть. Вам не удалось избежать поимки.",
        );
        crate::ghost::ghostize_client(world, client);
    }
    if let Some(mut f) = world.get_mut::<Fugitive>(fugitive) {
        f.captured = true;
    }
    // `forceMove(src)`: off the map, into the machine.
    world.entity_mut(fugitive).remove::<TilePos>();
    if let Some(mut m) = world.get_mut::<CaptureMachine>(machine) {
        m.prisoners.push(fugitive);
    }
    let name = crate::medical::name_of(world, fugitive);
    tell_mob(world, mob, Span::Notice, format!("{name} пойман."));
}

// ---------------------------------------------------------------------------------------------
// The round end

/// `get_result`'s outcomes (`FUGITIVE_RESULT_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    BadassHunter,
    PostmortemHunter,
    MajorHunter,
    HunterVictory,
    MinorHunter,
    Stalemate,
    MinorFugitive,
    FugitiveVictory,
    MajorFugitive,
}

/// `get_result` over the counts (tg compares the lists themselves: `!fugitives_dead` is never
/// true and `fugitives_dead < fugitives_counted` compares lists; their counts are meant).
pub fn outcome(
    counted: usize,
    dead: usize,
    captured: usize,
    hunters_dead: bool,
) -> Option<Outcome> {
    if counted == 0 {
        return None;
    }
    Some(if captured > 0 {
        if captured == counted {
            if dead == 0 {
                Outcome::BadassHunter
            } else if hunters_dead {
                Outcome::PostmortemHunter
            } else {
                Outcome::MajorHunter
            }
        } else if !hunters_dead {
            Outcome::HunterVictory
        } else {
            Outcome::MinorHunter
        }
    } else if dead == 0 {
        Outcome::MajorFugitive
    } else if dead < counted {
        Outcome::FugitiveVictory
    } else if !hunters_dead {
        Outcome::MinorFugitive
    } else {
        Outcome::Stalemate
    })
}

/// `/datum/team/fugitive/roundend_report` and `/datum/team/fugitive_hunters/roundend_report`.
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let mut q = world.query::<(Entity, &Fugitive)>();
    let mut fugitives: Vec<(Entity, Fugitive)> =
        q.iter(world).map(|(e, f)| (e, f.clone())).collect();
    fugitives.sort_by_key(|(e, _)| *e);
    let mut out = Vec::new();
    if !fugitives.is_empty() {
        let station = crate::events::station_name(world);
        let n = fugitives.len();
        out.push(line(vec![ChatSegment::new(
            Span::Plain,
            format!(
                "{n} {} укрылись на {station}!",
                if n == 1 {
                    "беглец"
                } else {
                    "беглеца"
                }
            ),
        )]));
        for (mob, f) in &fugitives {
            out.push(super::super::traitor::print_player(world, *mob, &f.key));
        }
    }
    let hunters: Vec<(Entity, FugitiveHunter)> = world
        .get_resource::<FugitiveState>()
        .map(|s| s.hunters.clone())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|e| world.get::<FugitiveHunter>(e).map(|h| (e, h.clone())))
        .collect();
    let Some(pack) = hunters.first().map(|(_, h)| h.pack) else {
        return out;
    };
    let backstory = ss13_shared::capitalize(pack.name());
    out.push(line(vec![ChatSegment::new(
        Span::Plain,
        format!(
            "...и {} {} пытался выследить их!",
            hunters.len(),
            pack.name()
        ),
    )]));
    for (mob, h) in &hunters {
        out.push(super::super::traitor::print_player(world, *mob, &h.key));
    }
    let dead = |world: &World, e: Entity| world.get::<Stat>(e).is_none_or(|s| *s == Stat::Dead);
    let fugitives_dead = fugitives.iter().filter(|(e, _)| dead(world, *e)).count();
    let captured = fugitives.iter().filter(|(_, f)| f.captured).count();
    // `all_hunters_dead`: tg's check is inverted; a hunter that is not a living human is out.
    let hunters_dead = hunters
        .iter()
        .all(|(e, _)| dead(world, *e) || world.get::<crate::human::HumanLook>(*e).is_none());
    let (green, title, text): (Option<bool>, String, String) =
        match outcome(fugitives.len(), fugitives_dead, captured, hunters_dead) {
            Some(Outcome::BadassHunter) => (
                Some(true),
                format!("Безупречная победа {backstory}!"),
                format!("{backstory} захватили всех беглецов живыми!"),
            ),
            Some(Outcome::PostmortemHunter) => (
                Some(true),
                format!("Посмертная победа {backstory}!"),
                format!("{backstory} удалось поймать каждого беглеца, но все они погибли! Ужас!"),
            ),
            Some(Outcome::MajorHunter) => (
                Some(true),
                format!("Разгромная победа {backstory}"),
                format!("{backstory} удалось поймать каждого беглеца, живого или мёртвого."),
            ),
            Some(Outcome::HunterVictory) => (
                Some(true),
                format!("Победа {backstory}"),
                format!("{backstory} удалось поймать беглеца, живого или мёртвого."),
            ),
            Some(Outcome::MinorHunter) => (
                Some(true),
                format!("Незначительная победа {backstory}"),
                format!(
                    "Все {backstory} погибли, но им удалось поймать беглеца, живого или мёртвого."
                ),
            ),
            Some(Outcome::Stalemate) => (
                None,
                "Кровавый тупик".to_owned(),
                "Все погибли, и никого из беглецов не удалось вернуть.".to_owned(),
            ),
            Some(Outcome::MinorFugitive) => (
                Some(false),
                "Незначительная победа беглецов".to_owned(),
                "Все беглецы погибли, но ни один из них не был возвращён!".to_owned(),
            ),
            Some(Outcome::FugitiveVictory) => (
                Some(false),
                "Победа беглецов".to_owned(),
                format!("Беглец выжил, и ни одно тело не было забрано {backstory}."),
            ),
            Some(Outcome::MajorFugitive) => (
                Some(false),
                "Разгромная победа беглецов".to_owned(),
                "Все беглецы выжили и избежали поимки!".to_owned(),
            ),
            None => (
                None,
                "Телефонный розыгрыш!".to_owned(),
                format!("{backstory} были вызваны, но беглецов не было...?"),
            ),
        };
    let span = match green {
        Some(true) => Span::GreenText,
        Some(false) => Span::RedText,
        None => Span::Bold,
    };
    out.push(line(vec![ChatSegment::new(span, title)]));
    out.push(line(vec![ChatSegment::new(Span::Bold, text)]));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn few_volunteers_make_one_fugitive() {
        assert_eq!(poll_caps(0, 99.0), (1, 1));
        assert_eq!(poll_caps(1, 99.0), (1, 1));
        // Four volunteers: 22% for a lone one.
        assert_eq!(poll_caps(4, 21.9), (1, 1));
        assert_eq!(poll_caps(4, 22.0), CAPS);
        assert_eq!(poll_caps(20, 0.0), CAPS);
        assert_eq!(
            backstories(1),
            &[Backstory::Invisible, Backstory::Waldo][..]
        );
        assert_eq!(backstories(3).len(), 3);
    }

    #[test]
    fn outcomes_count_the_fugitives() {
        assert_eq!(outcome(0, 0, 0, false), None);
        assert_eq!(outcome(3, 0, 3, false), Some(Outcome::BadassHunter));
        assert_eq!(outcome(3, 1, 3, true), Some(Outcome::PostmortemHunter));
        assert_eq!(outcome(3, 1, 3, false), Some(Outcome::MajorHunter));
        assert_eq!(outcome(3, 0, 1, false), Some(Outcome::HunterVictory));
        assert_eq!(outcome(3, 0, 1, true), Some(Outcome::MinorHunter));
        assert_eq!(outcome(3, 0, 0, false), Some(Outcome::MajorFugitive));
        assert_eq!(outcome(3, 2, 0, false), Some(Outcome::FugitiveVictory));
        assert_eq!(outcome(3, 3, 0, false), Some(Outcome::MinorFugitive));
        assert_eq!(outcome(3, 3, 0, true), Some(Outcome::Stalemate));
    }

    #[test]
    fn every_pack_has_spawners_and_an_announcement() {
        for pack in HUNTER_PACKS {
            assert!(!pack.spawners().is_empty());
            let (text, title) = pack.announcement("Станция");
            assert!(!text.is_empty() && !title.is_empty());
        }
    }
}
