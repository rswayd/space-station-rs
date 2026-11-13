//! Guardians (holoparasites), after tg `/mob/living/basic/guardian` (`code/modules/mob/living/
//! basic/guardian/`: `guardian.dm`, `guardian_creator.dm`, `guardian_verbs.dm`,
//! `guardian_fluff.dm`), `/datum/component/life_link` and `/datum/component/leash`.
//!
//! - A creator (`/obj/item/guardian_creator`: the tarot deck, the holoparasite injector, the
//!   holocarp fishsticks, the dusty shard) used in hand: refused to a guardian, to someone who
//!   already has one (unless it allows several), to a vampire (BandaStation) and, for the
//!   injector, to a changeling; a radial menu of its types (the shard picks one at random), then
//!   ghosts are polled for 10 s. A volunteer becomes the guardian, hidden inside the user; a
//!   failed poll leaves the creator unused.
//! - The guardian is bound to its summoner ([`Guardian`]): it manifests on the summoner's tile and
//!   recalls into them (1 s cooldown each), never strays further than its type's range (it is
//!   pulled back to the summoner, `leash`), attacks only while manifested, and is invincible:
//!   damage it takes goes to the summoner (`life_link`, "X is under attack!"). The summoner's
//!   death dusts them and ends the guardian; the guardian's death dusts the summoner.
//! - Buttons: the guardian's manifest, recall, communicate and check type; the summoner's
//!   communicate and recall. A message goes to the summoner, every guardian of theirs and the
//!   ghosts.
//! - Look by theme (magic, tech, miner, carp): the theme's base sprite and its overlay, the
//!   theme's name; a random name of `guardian_descriptions` and the theme's surnames.
// shortcut: no type abilities (assassin stealth, charger charge, dextrous hands, explosive
// traps, gaseous gases, gravitokinetic gravity, lightning chains, protector shield, ranged
// shots and scout mode, support healing and beacon): every type fights with its own melee
// numbers only; no name and colour prompts (a random name, the overlay stays white), no
// "Reset Guardian Consciousness", light toggle, phase effects, health HUD of the summoner's
// health or brain damage to a summoner in hard crit; damage passes as brute; the summoning line
// is the magic theme's for every theme. Add with each type's mechanism.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::{UiActed, UiSystems};

use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::click::{ClickResolved, ResolveClicks};
use crate::closets::InCloset;
use crate::ghost_poll::{self, PollSpec};
use crate::health::{DamageType, Health, Stat};
use crate::server::PlayerMob;
use crate::tools::tell_mob;

pub const GUARDIAN: &str = "/mob/living/basic/guardian";
pub const CREATOR: &str = "/obj/item/guardian_creator";
const TECH_CREATOR: &str = "/obj/item/guardian_creator/tech";
const WIZARD_CREATOR: &str = "/obj/item/guardian_creator/wizard";
const CARP_CREATOR: &str = "/obj/item/guardian_creator/carp";
const MINER_CREATOR: &str = "/obj/item/guardian_creator/miner";
const GUARDIAN_DMI: &str = "icons/mob/nonhuman-player/guardian.dmi";
const HUD_DMI: &str = "icons/hud/guardian.dmi";
/// The guardian's own health (tg `INFINITY`): damage never kills it, it passes on.
const GUARDIAN_HEALTH: f32 = 1.0e9;
/// `manifest_cooldown`.
const MANIFEST_COOLDOWN: f64 = 1.0;
/// The creator's `poll_time`.
const POLL_SECS: f64 = 10.0;
/// `POLL_IGNORE_HOLOPARASITE`.
const POLL_IGNORE: &str = "holoparasite";
/// Button ids.
const BUTTON: &str = "guardian:";
const MANIFEST: &str = "guardian:manifest";
const RECALL: &str = "guardian:recall";
const COMMUNICATE: &str = "guardian:communicate";
const CHECK_TYPE: &str = "guardian:check_type";
const SUMMONER_COMMS: &str = "guardian:summoner_comms";
const RECALL_GUARDIAN: &str = "guardian:recall_guardian";

/// `guardian_descriptions.txt`, `guardian_tarot.txt`, `guardian_gamepieces.txt`.
const FIRST_NAMES: [&str; 24] = [
    "Black",
    "Blazing",
    "Bloody",
    "Blue",
    "Bronze",
    "Dawn",
    "Dusk",
    "Gold",
    "Green",
    "Grey",
    "Iron",
    "Midnight",
    "Orange",
    "Pink",
    "Plastitanium",
    "Purple",
    "Red",
    "Shimmering",
    "Shining",
    "Silver",
    "Sparkling",
    "Steel",
    "White",
    "Yellow",
];
const FANTASY_SURNAMES: [&str; 23] = [
    "Chariot",
    "Death",
    "Devil",
    "Emperor",
    "Empress",
    "Fool",
    "Fortune",
    "Hangman",
    "Hermit",
    "Hierophant",
    "Judgement",
    "Justice",
    "Lover",
    "Magician",
    "Moon",
    "Priestess",
    "Star",
    "Strength",
    "Sun",
    "Temperance",
    "Tower",
    "Wheel",
    "World",
];
const TECH_SURNAMES: [&str; 13] = [
    "Ace", "Bishop", "Club", "Diamond", "Heart", "Jack", "Joker", "King", "Knight", "Pawn",
    "Queen", "Rook", "Spade",
];

/// The guardian types (`guardian_type`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Assassin,
    Charger,
    Dextrous,
    Explosive,
    Gaseous,
    Gravitokinetic,
    Lightning,
    Protector,
    Ranged,
    Standard,
    Support,
}

impl Kind {
    pub const ALL: [Self; 11] = [
        Self::Assassin,
        Self::Charger,
        Self::Dextrous,
        Self::Explosive,
        Self::Gaseous,
        Self::Gravitokinetic,
        Self::Lightning,
        Self::Protector,
        Self::Ranged,
        Self::Standard,
        Self::Support,
    ];

    /// `creator_icon` (also the subtype's name).
    pub fn key(self) -> &'static str {
        match self {
            Self::Assassin => "assassin",
            Self::Charger => "charger",
            Self::Dextrous => "dextrous",
            Self::Explosive => "explosive",
            Self::Gaseous => "gaseous",
            Self::Gravitokinetic => "gravitokinetic",
            Self::Lightning => "lightning",
            Self::Protector => "protector",
            Self::Ranged => "ranged",
            Self::Standard => "standard",
            Self::Support => "support",
        }
    }

    pub fn path(self) -> String {
        format!("{GUARDIAN}/{}", self.key())
    }

    pub fn of(path: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|k| is_under(path, &format!("{GUARDIAN}/{}", k.key())))
    }

    /// `creator_name`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Assassin => "Assassin",
            Self::Charger => "Charger",
            Self::Dextrous => "Dextrous",
            Self::Explosive => "Explosive",
            Self::Gaseous => "Gaseous",
            Self::Gravitokinetic => "Gravitokinetic",
            Self::Lightning => "Lightning",
            Self::Protector => "Protector",
            Self::Ranged => "Ranged",
            Self::Standard => "Standard",
            Self::Support => "Support",
        }
    }

    /// `range`: how far from the summoner it may go.
    pub fn range(self) -> u32 {
        match self {
            Self::Explosive | Self::Ranged => 13,
            Self::Gaseous | Self::Lightning => 7,
            Self::Protector => 5,
            _ => 10,
        }
    }

    /// The `/datum/guardian_fluff` (magic) line on summoning.
    fn fluff(self) -> &'static str {
        match self {
            Self::Assassin => {
                "...И вытягиваете Космического ниндзя, смертоносного и невидимого убийцу."
            }
            Self::Charger => {
                "...И вытягиваете Охотника, инопланетного мастера стремительной атаки."
            }
            Self::Dextrous => {
                "...И вытягиваете Обезьяну, зверя, научившегося пользоваться инструментами."
            }
            Self::Explosive => "...И вытягиваете Учёного, вестника взрывной смерти.",
            Self::Gaseous => "...И вытягиваете Атмосферного техника, окутанного фиолетовой дымкой.",
            Self::Gravitokinetic => "...И вытягиваете Сингулярность, ужасную непреодолимую силу.",
            Self::Lightning => "...И вытягиваете Суперматерию, смертельный источник энергии.",
            Self::Protector => {
                "...И вытягиваете Корги, стойкого защитника, никогда не покидающего хозяина."
            }
            Self::Ranged => "...И вытягиваете Наблюдателя, пронзающего добычу издалека.",
            Self::Standard => {
                "...И вытягиваете Ассистента, безликого, но не стоит его недооценивать."
            }
            Self::Support => "...И вытягиваете Парамедика, вершителя жизни и смерти.",
        }
    }

    /// The `playstyle_string`, short.
    fn playstyle(self) -> &'static str {
        match self {
            Self::Standard => "Как стандартный тип, вы наносите сильные удары в ближнем бою.",
            Self::Protector => "Как тип защитника, вы держитесь рядом с призывателем.",
            Self::Ranged => "Как дальнобойный тип, вы держитесь дальше от призывателя.",
            Self::Support => "Как тип поддержки, вы помогаете призывателю.",
            _ => "Особые способности вашего типа ещё не пробудились: вы сражаетесь в ближнем бою.",
        }
    }
}

/// `/datum/guardian_fluff` subtypes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Magic,
    Tech,
    Miner,
    Carp,
}

impl Theme {
    fn of_creator(path: &str) -> Self {
        if is_under(path, TECH_CREATOR) {
            Self::Tech
        } else if is_under(path, MINER_CREATOR) {
            Self::Miner
        } else if is_under(path, CARP_CREATOR) {
            Self::Carp
        } else {
            Self::Magic
        }
    }

    /// `name`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Magic => "Guardian Spirit",
            Self::Tech => "Holoparasite",
            Self::Miner => "Power Miner",
            Self::Carp => "Holocarp",
        }
    }

    /// `icon_state` and `overlay_state`.
    fn states(self) -> (Option<&'static str>, &'static str) {
        match self {
            Self::Magic => (Some("magicbase"), "magic"),
            Self::Tech => (Some("techbase"), "tech"),
            Self::Miner => (Some("minerbase"), "miner"),
            Self::Carp => (None, "carp"),
        }
    }

    fn surnames(self) -> &'static [&'static str] {
        match self {
            Self::Tech | Self::Carp => &TECH_SURNAMES,
            _ => &FANTASY_SURNAMES,
        }
    }
}

/// A guardian's bond (tg `summoner`, `range`, `theme`, `manifest_cooldown`).
#[derive(Component, Clone, Debug)]
pub struct Guardian {
    pub summoner: Option<Entity>,
    pub kind: Kind,
    pub theme: Theme,
    pub range: u32,
    pub ready_at: f64,
}

/// A creator that was used (or is being used: `used`).
#[derive(Component, Clone, Copy, Debug)]
pub struct CreatorUsed;

/// On a radial prompt: the creator and its offered types.
#[derive(Component, Clone, Debug)]
struct PickPrompt {
    creator: Entity,
    kinds: Vec<Kind>,
}

/// On a text prompt: who speaks to their bond.
#[derive(Component, Clone, Copy, Debug)]
struct CommsPrompt;

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    app.add_click_handler(ClickPhase::AttackSelf, 1787, |w, c| {
        attack_self(w, c.client, c.mob, c.target)
    });
    app.add_systems(
        Update,
        (
            picks.in_set(UiSystems::Handle).after(PromptSystems::Answer),
            messages
                .in_set(UiSystems::Handle)
                .after(PromptSystems::Answer),
            buttons.in_set(UiSystems::Handle),
            guardian_clicks.after(ResolveClicks),
            bonds.before(crate::health::HealthSystems),
            pass_damage.before(crate::health::HealthSystems),
        ),
    );
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn name(world: &World, e: Entity) -> String {
    crate::security::name_of(world, e)
}

/// The guardians bound to `summoner` (`get_all_linked_holoparasites`).
pub fn guardians_of(world: &mut World, summoner: Entity) -> Vec<Entity> {
    let mut q = world.query::<(Entity, &Guardian)>();
    let mut out: Vec<Entity> = q
        .iter(world)
        .filter(|(_, g)| g.summoner == Some(summoner))
        .map(|(e, _)| e)
        .collect();
    out.sort();
    out
}

/// Out and about (`is_deployed`): not inside its summoner.
pub fn deployed(world: &World, guardian: Entity) -> bool {
    let summoner = world.get::<Guardian>(guardian).and_then(|g| g.summoner);
    summoner.is_none() || world.get::<InCloset>(guardian).map(|c| c.closet) != summoner
}

/// What a creator offers (`possible_guardians`): all but dextrous; the wizard's all but support;
/// the shard's five.
fn offered(path: &str) -> Vec<Kind> {
    if is_under(path, WIZARD_CREATOR) {
        return Kind::ALL
            .into_iter()
            .filter(|k| *k != Kind::Support)
            .collect();
    }
    if is_under(path, MINER_CREATOR) {
        return vec![
            Kind::Charger,
            Kind::Protector,
            Kind::Ranged,
            Kind::Standard,
            Kind::Support,
        ];
    }
    Kind::ALL
        .into_iter()
        .filter(|k| *k != Kind::Dextrous)
        .collect()
}

/// `allow_multiple`: the wizard's deck and the fishsticks.
fn allows_multiple(path: &str) -> bool {
    is_under(path, WIZARD_CREATOR) || is_under(path, CARP_CREATOR)
}

/// The messages of a creator (`use_message`, `used_message`, `failure_message`, `ling_failure`,
/// `success_message`).
struct CreatorText {
    used: &'static str,
    using: &'static str,
    failure: &'static str,
    refused: &'static str,
    success: &'static str,
}

fn texts(theme: Theme) -> CreatorText {
    match theme {
        Theme::Tech => CreatorText {
            used: "Инжектор уже использован.",
            using: "Вы начинаете включать инжектор...",
            failure: "...ОШИБКА. ЗАГРУЗКА ПРЕРВАНА. ИИ НЕ ИНИЦИАЛИЗИРОВАН. ОБРАТИТЕСЬ В ПОДДЕРЖКУ \
                      ИЛИ ПОПРОБУЙТЕ ПОЗЖЕ.",
            refused: "Голопаразиты отшатываются в ужасе. Они не хотят иметь ничего общего с таким \
                      существом, как вы.",
            success: "теперь в сети!",
        },
        Theme::Carp => CreatorText {
            used: "Кто-то уже откусил от этих рыбных палочек! Фу.",
            using: "Вы кладёте рыбные палочки в рот...",
            failure: "Вам не удалось поймать ни одного карпового духа. Может, их нет, а может, \
                      вы что-то напутали.",
            refused: "Карпси, похоже, не избрал вас. Может, из-за вашего ужасающего \
                      происхождения.",
            success: "пойман!",
        },
        Theme::Miner => CreatorText {
            used: "Этот осколок, похоже, утратил всю силу...",
            using: "Вы прокалываете кожу осколком...",
            failure: "Осколок никак не отреагировал. Может, попробовать позже...",
            refused: "Сила осколка не откликается на ваше ужасающее мутировавшее тело.",
            success: "явился!",
        },
        Theme::Magic => CreatorText {
            used: "Все карты теперь будто пустые.",
            using: "Вы тасуете колоду...",
            failure: "...И тянете карту! Она... пустая? Может, стоит попробовать позже.",
            refused: "Колода не откликается на бездушное существо вроде вас.",
            success: "призван!",
        },
    }
}

/// Why `user` may not use `creator` now (the balloon or chat line), if anything.
fn refusal(world: &mut World, user: Entity, creator_path: &str) -> Option<(bool, &'static str)> {
    let theme = Theme::of_creator(creator_path);
    if world.get::<Guardian>(user).is_some() {
        return Some((true, "нельзя!"));
    }
    if !allows_multiple(creator_path) && !guardians_of(world, user).is_empty() {
        return Some((true, "уже есть!"));
    }
    let changeling = world
        .get::<crate::antag::changeling::server::Changeling>(user)
        .is_some();
    let vampire = world
        .get::<crate::antag::vampire::server::Vampire>(user)
        .is_some();
    // `allow_changeling` is off on the injector only; BandaStation refuses vampires everywhere.
    if (changeling && theme == Theme::Tech) || vampire {
        return Some((false, texts(theme).refused));
    }
    None
}

/// `/obj/item/guardian_creator/attack_self`.
pub fn attack_self(world: &mut World, client: Entity, user: Entity, item: Entity) -> bool {
    let Some(path) = world.get::<DmItem>(item).map(|i| i.path.clone()) else {
        return false;
    };
    if !is_under(&path, CREATOR) {
        return false;
    }
    let theme = Theme::of_creator(&path);
    if let Some((balloon, text)) = refusal(world, user, &path) {
        if balloon {
            crate::balloon::balloon_alert(world, user, user, text);
        } else {
            tell_mob(world, user, Span::BoldNotice, text);
        }
        return true;
    }
    if world.get::<CreatorUsed>(item).is_some() {
        tell_mob(world, user, Span::Notice, texts(theme).used);
        return true;
    }
    let kinds = offered(&path);
    // `random`: the shard.
    if is_under(&path, MINER_CREATOR) {
        let i = world
            .resource_mut::<crate::combat::CombatRng>()
            .range(0, kinds.len() as u32 - 1) as usize;
        start_poll(world, user, item, kinds[i], true);
        return true;
    }
    let choices = kinds
        .iter()
        .map(|k| ss13_tgui::prompt::RadialChoice {
            name: k.name().to_owned(),
            icon: crate::radial::icon_of(world, HUD_DMI, k.key()),
        })
        .collect();
    if let Some(prompt) = crate::radial::show(
        world,
        crate::radial::RadialMenu {
            client,
            actor: user,
            anchor: item,
            title: theme.name().to_owned(),
            choices,
            require_near: true,
        },
    ) {
        world.entity_mut(prompt).insert(PickPrompt {
            creator: item,
            kinds,
        });
    }
    true
}

/// The radial's answer: `check_menu` again, then the poll.
fn picks(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, user, answer) in answers {
        let Some(pick) = world.get::<PickPrompt>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let Some(&kind) = pick.kinds.get(i) else {
            continue;
        };
        let held = world
            .get::<crate::inventory::CarriedBy>(pick.creator)
            .is_some_and(|c| c.0 == user);
        if !held
            || world.get::<CreatorUsed>(pick.creator).is_some()
            || !crate::radial::can_still_use(world, user, pick.creator)
        {
            continue;
        }
        start_poll(world, user, pick.creator, kind, false);
    }
}

/// `used = TRUE`, `use_message`, then `poll_ghost_candidates`.
fn start_poll(world: &mut World, user: Entity, creator: Entity, kind: Kind, random: bool) {
    let Some(path) = world.get::<DmItem>(creator).map(|i| i.path.clone()) else {
        return;
    };
    let theme = Theme::of_creator(&path);
    world.entity_mut(creator).insert(CreatorUsed);
    tell_mob(world, user, Span::Notice, texts(theme).using);
    if !world.contains_resource::<ghost_poll::Polls>() {
        world.entity_mut(creator).remove::<CreatorUsed>();
        return;
    }
    let type_name = if random { "Random" } else { kind.name() };
    let user_name = name(world, user);
    let spec = PollSpec {
        question: format!(
            "Вы хотите играть за {type_name} {} игрока {user_name}?",
            theme.name()
        ),
        role_name: type_name.to_owned(),
        secs: POLL_SECS,
        ignore: Some(POLL_IGNORE),
        target: Some(creator),
        ..default()
    };
    let group = ghost_poll::ghosts(world);
    ghost_poll::poll_candidates(world, spec, group, move |world, mut signed_up| {
        if world.get_entity(creator).is_err() {
            return;
        }
        // BandaStation: a vampire is refused again at the moment of effect.
        if refusal(world, user, &path).is_some_and(|(balloon, _)| !balloon) {
            tell_mob(world, user, Span::BoldNotice, texts(theme).refused);
            world.entity_mut(creator).remove::<CreatorUsed>();
            return;
        }
        let spawned = ghost_poll::pick_candidate(world, &mut signed_up)
            .and_then(|chosen| spawn_guardian(world, user, &path, kind, chosen));
        if spawned.is_none() {
            tell_mob(world, user, Span::BoldNotice, texts(theme).failure);
            world.entity_mut(creator).remove::<CreatorUsed>();
        }
    });
}

/// `spawn_guardian`: the user still alive and without one (unless allowed), the guardian made,
/// bound and handed to the ghost. `None` when it did not happen.
fn spawn_guardian(
    world: &mut World,
    user: Entity,
    creator_path: &str,
    kind: Kind,
    chosen: ghost_poll::Candidate,
) -> Option<Entity> {
    if world.get_entity(user).is_err() || world.get::<Stat>(user) == Some(&Stat::Dead) {
        return None;
    }
    if !allows_multiple(creator_path) && !guardians_of(world, user).is_empty() {
        crate::balloon::balloon_alert(world, user, user, "уже есть!");
        return None;
    }
    let pos = *world.get::<TilePos>(user)?;
    let theme = Theme::of_creator(creator_path);
    let guardian = super::spawn::spawn_mob(world, &kind.path(), pos)?;
    world.entity_mut(guardian).insert(Guardian {
        summoner: None,
        kind,
        theme,
        range: kind.range(),
        ready_at: 0.0,
    });
    if let Some(mut health) = world.get_mut::<Health>(guardian) {
        health.max = GUARDIAN_HEALTH;
    }
    apply_theme(world, guardian, theme);
    set_summoner(world, guardian, user);
    if !crate::medical::brainmob::possess(world, chosen.client, chosen.mob, guardian) {
        world.despawn(guardian);
        refresh_buttons(world, user);
        return None;
    }
    world.despawn(chosen.mob);
    refresh_buttons(world, guardian);
    let guardian_name = name(world, guardian);
    let user_name = name(world, user);
    tell_mob(
        world,
        guardian,
        Span::Notice,
        format!("Вы — {guardian_name}, связанный с {user_name}."),
    );
    tell_mob(
        world,
        guardian,
        Span::Notice,
        "Кнопками на HUD вы материализуетесь, возвращаетесь к хозяину и говорите с ним наедине.",
    );
    tell_mob(
        world,
        guardian,
        Span::Notice,
        format!(
            "Вы неуязвимы, но умрёте, если умрёт {user_name}, а урон по вам передаётся хозяину, \
             ведь вы питаетесь им."
        ),
    );
    tell_mob(world, guardian, Span::BoldNotice, kind.playstyle());
    tell_mob(world, user, Span::Notice, kind.fluff());
    tell_mob(
        world,
        user,
        Span::Notice,
        format!("{} {}", theme.name(), texts(theme).success),
    );
    Some(guardian)
}

/// `/datum/guardian_fluff/apply`: name, sprite and overlay; a random name (`generate_random_name`).
fn apply_theme(world: &mut World, guardian: Entity, theme: Theme) {
    let (base, overlay) = theme.states();
    if let Some(base) = base {
        super::set_icon_state(world, guardian, GUARDIAN_DMI, base);
    }
    let overlay = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(GUARDIAN_DMI, overlay));
    if let (Some(id), Some(mut look)) = (overlay, world.get_mut::<Appearance>(guardian)) {
        look.overlays.push(Overlay::new(id));
    }
    let surnames = theme.surnames();
    let (first, last) = {
        let mut rng = world.resource_mut::<crate::combat::CombatRng>();
        let first = rng.range(0, FIRST_NAMES.len() as u32 - 1) as usize;
        let last = rng.range(0, surnames.len() as u32 - 1) as usize;
        (first, last)
    };
    world.entity_mut(guardian).insert(Name::new(format!(
        "{} {}",
        FIRST_NAMES[first], surnames[last]
    )));
}

/// `set_summoner`: bound, the summoner's buttons, recalled into them.
fn set_summoner(world: &mut World, guardian: Entity, summoner: Entity) {
    if let Some(mut g) = world.get_mut::<Guardian>(guardian) {
        g.summoner = Some(summoner);
    }
    refresh_buttons(world, summoner);
    recall(world, guardian, true);
}

/// `manifest(forced)`: out onto the summoner's tile.
pub fn manifest(world: &mut World, guardian: Entity, forced: bool) -> bool {
    let Some(g) = world.get::<Guardian>(guardian).cloned() else {
        return false;
    };
    let Some(summoner) = g.summoner else {
        return false;
    };
    if deployed(world, guardian) || (!forced && now(world) < g.ready_at) {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(summoner) else {
        return false;
    };
    crate::closets::let_out(world, guardian);
    set_pos(world, guardian, pos);
    let ready = now(world) + MANIFEST_COOLDOWN;
    if let Some(mut g) = world.get_mut::<Guardian>(guardian) {
        g.ready_at = ready;
    }
    true
}

/// `recall(forced)`: back inside the summoner.
pub fn recall(world: &mut World, guardian: Entity, forced: bool) -> bool {
    let Some(g) = world.get::<Guardian>(guardian).cloned() else {
        return false;
    };
    let Some(summoner) = g.summoner else {
        return false;
    };
    if !deployed(world, guardian) || (!forced && now(world) < g.ready_at) {
        return false;
    }
    crate::closets::put_mob_inside(world, guardian, summoner);
    if let Some(&pos) = world.get::<TilePos>(summoner) {
        set_pos(world, guardian, pos);
    }
    let ready = now(world) + MANIFEST_COOLDOWN;
    if let Some(mut g) = world.get_mut::<Guardian>(guardian) {
        g.ready_at = ready;
    }
    true
}

fn set_pos(world: &mut World, mob: Entity, pos: TilePos) {
    if world.get::<TilePos>(mob) != Some(&pos) {
        world.entity_mut(mob).insert(pos);
    }
}

/// `cut_summoner` and the end of the bond (`on_summoner_death`, `on_summoner_deletion`).
fn unbind(world: &mut World, guardian: Entity) -> Option<Entity> {
    let summoner = world.get_mut::<Guardian>(guardian)?.summoner.take()?;
    crate::closets::let_out(world, guardian);
    if world.get_entity(summoner).is_ok() {
        refresh_buttons(world, summoner);
    }
    Some(summoner)
}

/// Every tick: a recalled guardian rides along with its summoner, a manifested one is pulled
/// back past its range (`leash`); a summoner gone ends it, a dead summoner is dusted with it.
fn bonds(world: &mut World) {
    let mut q = world.query::<(Entity, &Guardian)>();
    let bonded: Vec<(Entity, Entity, u32)> = q
        .iter(world)
        .filter_map(|(e, g)| Some((e, g.summoner?, g.range)))
        .collect();
    for (guardian, summoner, range) in bonded {
        if world.get_entity(summoner).is_err() {
            // `on_summoner_deletion`.
            unbind(world, guardian);
            tell_mob(
                world,
                guardian,
                Span::Danger,
                "Ваш призыватель исчез, вы чувствуете, как угасаете!",
            );
            crate::health::ghostize_gone(world, guardian);
            world.despawn(guardian);
            continue;
        }
        if world.get::<Stat>(summoner) == Some(&Stat::Dead) {
            summoner_died(world, guardian, summoner);
            continue;
        }
        let Some(&home) = world.get::<TilePos>(summoner) else {
            continue;
        };
        if !deployed(world, guardian) {
            set_pos(world, guardian, home);
            continue;
        }
        let far = world
            .get::<TilePos>(guardian)
            .is_none_or(|p| p.chebyshev(home).is_none_or(|d| d > range));
        if far {
            set_pos(world, guardian, home);
        }
    }
}

/// `on_summoner_death` (the life link's `on_linked_death`), then the guardian's own death.
fn summoner_died(world: &mut World, guardian: Entity, summoner: Entity) {
    unbind(world, guardian);
    if let Some(&pos) = world.get::<TilePos>(summoner) {
        set_pos(world, guardian, pos);
    }
    tell_mob(world, guardian, Span::Danger, "Ваш призыватель умер!");
    let guardian_name = name(world, guardian);
    let summoner_name = name(world, summoner);
    crate::chem::visible(
        world,
        guardian,
        &format!("{guardian_name} умирает вместе с хозяином!"),
        &format!("{guardian_name} умирает вместе с хозяином!"),
        Span::Danger,
    );
    crate::chem::visible(
        world,
        summoner,
        &format!("Ваше тело поглощено напряжением от поддержания {guardian_name}!"),
        &format!(
            "Тело {summoner_name} полностью поглощено напряжением от поддержания {guardian_name}!"
        ),
        Span::Danger,
    );
    crate::health::dust_with(world, summoner, false, true, true);
    crate::health::ghostize_gone(world, guardian);
    world.despawn(guardian);
}

/// The guardian died (`/mob/living/basic/guardian/death`, from `crate::mobs`): its summoner is
/// dusted.
pub(super) fn on_death(world: &mut World, guardian: Entity) {
    let Some(summoner) = unbind(world, guardian) else {
        return;
    };
    if world.get_entity(summoner).is_err() {
        return;
    }
    let guardian_name = name(world, guardian);
    tell_mob(
        world,
        summoner,
        Span::Danger,
        format!("Ваш {guardian_name} каким-то образом умер!"),
    );
    crate::health::dust(world, summoner);
}

/// `life_link`: what hurts the guardian hurts its summoner; `on_harm` tells them.
fn pass_damage(world: &mut World) {
    let mut q = world.query::<(Entity, &Guardian, &Health)>();
    let hurt: Vec<(Entity, Option<Entity>, f32)> = q
        .iter(world)
        .map(|(e, g, h)| (e, g.summoner, h.brute + h.burn + h.tox + h.oxy))
        .filter(|&(_, _, amount)| amount > 0.0)
        .collect();
    for (guardian, summoner, amount) in hurt {
        if let Some(mut health) = world.get_mut::<Health>(guardian) {
            for kind in [
                DamageType::Brute,
                DamageType::Burn,
                DamageType::Tox,
                DamageType::Oxy,
            ] {
                let had = match kind {
                    DamageType::Brute => health.brute,
                    DamageType::Burn => health.burn,
                    DamageType::Tox => health.tox,
                    _ => health.oxy,
                };
                if had > 0.0 {
                    health.adjust(kind, -had);
                }
            }
        }
        let Some(summoner) = summoner.filter(|&s| world.get_entity(s).is_ok()) else {
            continue;
        };
        if let Some(mut health) = world.get::<Health>(summoner).cloned() {
            health.adjust(DamageType::Brute, amount);
            let mut stat = world.get::<Stat>(summoner).copied().unwrap_or_default();
            let changed = crate::health::update_stat(&health, &mut stat).is_some();
            let mut e = world.entity_mut(summoner);
            e.insert(health);
            if changed {
                e.insert(stat);
            }
        }
        if amount > 2.0 {
            let guardian_name = name(world, guardian);
            let summoner_name = name(world, summoner);
            tell_mob(
                world,
                summoner,
                Span::Danger,
                format!("{guardian_name} атакован! Вы получаете урон!"),
            );
            let seen =
                format!("Из {summoner_name} брызжет кровь, когда {guardian_name} получает урон!");
            crate::chem::visible(world, summoner, &seen, &seen, Span::Danger);
        }
    }
}

/// A manifested guardian's click on a living mob is its melee attack (`melee_attack`); recalled
/// it is "not tangible!".
fn guardian_clicks(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        let (mob, target) = (click.mob, click.target);
        if world.get::<Guardian>(mob).is_none() || target == mob {
            continue;
        }
        if world.get::<Stat>(mob) != Some(&Stat::Conscious) {
            continue;
        }
        if !deployed(world, mob) {
            crate::balloon::balloon_alert(world, mob, mob, "не осязаем!");
            continue;
        }
        if world.get::<Health>(target).is_some() {
            super::defense::melee_attack(world, mob, target);
        }
    }
}

fn button(world: &World, id: &str, name: &str, state: &str) -> (ActionButton, f64) {
    let icon = |dmi: &str, s: &str| {
        world
            .get_resource::<IconIndex>()
            .and_then(|index| index.get(dmi, s))
    };
    (
        ActionButton {
            id: id.to_owned(),
            name: name.to_owned(),
            icon: icon(HUD_DMI, state),
            background: icon(HUD_DMI, "base"),
            border: None,
            available: true,
            cooldown: 0.0,
            active: false,
            maptext: None,
        },
        0.0,
    )
}

/// The guardian buttons of `mob` after its others: a guardian's own, a summoner's controls.
fn refresh_buttons(world: &mut World, mob: Entity) {
    let mut buttons: Vec<(ActionButton, f64)> = world
        .get::<ActionButtons>(mob)
        .map(|b| {
            b.0.iter()
                .filter(|(b, _)| !b.id.starts_with(BUTTON))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    if world
        .get::<Guardian>(mob)
        .is_some_and(|g| g.summoner.is_some())
    {
        for (id, label, state) in [
            (CHECK_TYPE, "Проверить тип", "check_type"),
            (COMMUNICATE, "Связь", "communicate"),
            (MANIFEST, "Материализоваться", "manifest"),
            (RECALL, "Вернуться", "recall"),
        ] {
            buttons.push(button(world, id, label, state));
        }
    }
    if !guardians_of(world, mob).is_empty() {
        for (id, label, state) in [
            (SUMMONER_COMMS, "Связь со стражем", "communicate"),
            (RECALL_GUARDIAN, "Отозвать стража", "recall"),
        ] {
            buttons.push(button(world, id, label, state));
        }
    }
    if buttons.is_empty() && world.get::<ActionButtons>(mob).is_none() {
        return;
    }
    set_buttons(world, mob, buttons);
}

/// A click on a guardian button, checked at the moment of effect: the client still controls the
/// mob and the bond still holds.
fn buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .filter(|a| a.act.starts_with(BUTTON))
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client) {
            continue;
        }
        match id.as_str() {
            MANIFEST | RECALL | COMMUNICATE | CHECK_TYPE => {
                let Some(g) = world.get::<Guardian>(actor).cloned() else {
                    continue;
                };
                if g.summoner.is_none() {
                    continue;
                }
                match id.as_str() {
                    MANIFEST => {
                        manifest(world, actor, false);
                    }
                    RECALL => {
                        recall(world, actor, false);
                    }
                    COMMUNICATE => ask_message(world, client, actor, "Сообщение для призывателя"),
                    _ => tell_mob(world, actor, Span::BoldNotice, g.kind.playstyle()),
                }
            }
            SUMMONER_COMMS => {
                if !guardians_of(world, actor).is_empty() {
                    ask_message(world, client, actor, "Сообщение для стража");
                }
            }
            RECALL_GUARDIAN => {
                for guardian in guardians_of(world, actor) {
                    recall(world, guardian, false);
                }
            }
            _ => {}
        }
    }
}

fn ask_message(world: &mut World, client: Entity, actor: Entity, title: &str) {
    let ask = Ask::new(
        client,
        actor,
        title,
        "",
        PromptKind::Text {
            default: String::new(),
            max_length: 1024,
            multiline: false,
        },
    );
    let prompt = ask_world(world, ask);
    world.entity_mut(prompt).insert(CommsPrompt);
}

/// `communicate` and `guardian_comms`: to the summoner, all their guardians and the ghosts.
fn messages(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, speaker, answer) in answers {
        if world.get::<CommsPrompt>(prompt).is_none() {
            continue;
        }
        let Some(PromptAnswer::Text(text)) = answer else {
            continue;
        };
        let text = text.trim().to_owned();
        if text.is_empty() {
            continue;
        }
        let summoner = match world.get::<Guardian>(speaker) {
            Some(g) => g.summoner,
            None => Some(speaker),
        };
        let Some(summoner) = summoner.filter(|&s| world.get_entity(s).is_ok()) else {
            continue;
        };
        let mut hearers = guardians_of(world, summoner);
        if hearers.is_empty() {
            continue;
        }
        hearers.push(summoner);
        hearers.extend(ghost_poll::ghosts(world));
        let line = format!("{}: {text}", name(world, speaker));
        for hearer in hearers {
            tell_mob(world, hearer, Span::BoldNotice, line.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_follow_their_types() {
        assert_eq!(
            Kind::of("/mob/living/basic/guardian/standard"),
            Some(Kind::Standard)
        );
        assert_eq!(Kind::of("/mob/living/basic/guardian"), None);
        for kind in Kind::ALL {
            assert_eq!(Kind::of(&kind.path()), Some(kind));
        }
        assert_eq!(Kind::Protector.range(), 5);
        assert_eq!(Kind::Ranged.range(), 13);
        assert_eq!(Kind::Standard.range(), 10);
    }

    #[test]
    fn creators_offer_their_lists() {
        let tech = offered("/obj/item/guardian_creator/tech");
        assert_eq!(tech.len(), 10);
        assert!(!tech.contains(&Kind::Dextrous));
        let wizard = offered(WIZARD_CREATOR);
        assert!(wizard.contains(&Kind::Dextrous) && !wizard.contains(&Kind::Support));
        assert_eq!(offered(MINER_CREATOR).len(), 5);
        assert!(allows_multiple(CARP_CREATOR) && !allows_multiple(TECH_CREATOR));
        assert_eq!(Theme::of_creator(CREATOR), Theme::Magic);
        assert_eq!(Theme::of_creator(TECH_CREATOR), Theme::Tech);
    }
}
