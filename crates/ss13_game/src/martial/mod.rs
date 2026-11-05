//! Martial arts (server), after tg `code/datums/martial/_martial.dm` (`/datum/martial_art`),
//! `_action.dm` (`swap_arts`), `code/datums/components/martial_art_giver.dm`,
//! `code/game/objects/items/granters/` (`/obj/item/book/granter/martial`) and the styles in
//! [`cqc`], [`carp`], [`boxing`], [`wrestling`] and [`judo`] (BandaStation's corporate judo).
//!
//! - A mob knows styles in [`MartialArts`], the active one first (tg `martial_arts`,
//!   `GET_ACTIVE_MARTIAL_ART`). A style is taught by a manual or scroll read in hand
//!   ([`attack_self`]: three pages and a last read of 5 s each, `uses`), or while gloves or a belt
//!   that give it are worn in their slot ([`Origin::Item`]: gloves on the gloves slot, belts on
//!   the belt slot; taken off, the style is forgotten). A newly taught style becomes the active
//!   one; forgetting the active one activates the next.
//! - The active style takes the mob's empty-hand clicks on living mobs before the shove and
//!   melee ([`unarmed_attack`]: right click -> `disarm_act`, combat mode -> `harm_act`, else
//!   `help_act`) and its ctrl-click grabs ([`grab`]). Each move answers like tg: `Invalid` lets
//!   the normal attack happen, `Fail` and `Success` end the click. Moves add letters to the
//!   style's streak (`H`, `D`, `G`, `E`) for the same target; a combo in the streak fires its
//!   finisher. Styles with a combo display forget the streak `combo_timer` after its first move.
//! - The defender's active style may block a punch, shove, grab or melee hit first
//!   ([`check_block`]: CQC and boxing in throw mode, the sleeping carp's dodge) and the sleeping
//!   carp deflects projectiles ([`deflect_projectile`]).
//! - Action buttons: "Remember the basics" (the active style's moves), "Swap style" with two
//!   styles or more, and the style's own moves (wrestling, judo).
// shortcut: no combo HUD counter (`/atom/movable/screen/combo`), no attack animations, no
// `TRAIT_MARTIAL_ARTS_IMMUNE`, no locked styles (`locked_to_use`), no table smashing, no
// `log_combat`; the swap is its own button (tg's right click on the help button). The martial
// buttons share the mob's action bar: an antagonist's button refresh replaces them until the
// style changes again. Add them with the combo HUD and action groups.

pub mod boxing;
pub mod carp;
pub mod cqc;
pub mod judo;
pub mod wrestling;

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmName, IconIndex};
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::InventorySlot;
use ss13_shared::{Appearance, Dir, Facing, TilePos};
use ss13_tgui::server::UiActed;

use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::click::ClickResolved;
use crate::combat::{CLICK_CD_MELEE, CombatMode, CombatRng, NextMove};
use crate::health::{
    BodyPosition, COMBAT_MESSAGE_RANGE, DEFAULT_MESSAGE_RANGE, DamageType, Health, HealthSystems,
    Stat, ZoneSelected, apply_damage_zone,
};
use crate::inventory::{CarriedBy, Inventory};
use crate::server::PlayerMob;
use crate::sound::PlaySound;
use crate::status_effects::{Incap, StatusEffects, StatusSystems};
use crate::tools::{Actor, tell};
use crate::zone::BodyZone;

/// A martial art (tg `/datum/martial_art` subtypes ported here).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Style {
    /// `/datum/martial_art/cqc`.
    Cqc,
    /// `/datum/martial_art/the_sleeping_carp`.
    SleepingCarp,
    /// `/datum/martial_art/boxing`.
    Boxing,
    /// `/datum/martial_art/boxing/evil`.
    EvilBoxing,
    /// `/datum/martial_art/boxing/hunter`.
    HunterBoxing,
    /// `/datum/martial_art/wrestling`.
    Wrestling,
    /// BandaStation `/datum/martial_art/judo`.
    Judo,
}

impl Style {
    /// tg `name`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Cqc => "CQC",
            Self::SleepingCarp => "Спящий карп",
            Self::Boxing => "Бокс",
            Self::EvilBoxing => "Злой бокс",
            Self::HunterBoxing => "Охотничий бокс",
            Self::Wrestling => "Рестлинг",
            Self::Judo => "Корпоративное дзюдо",
        }
    }

    /// tg `help_verb`: the help button's name.
    fn help_verb(self) -> &'static str {
        match self {
            Self::Cqc => "Вспомнить основы",
            Self::SleepingCarp => "Вспомнить учения",
            Self::Boxing => "Сосредоточиться на стойке",
            Self::EvilBoxing => "Сосредоточиться на драке",
            Self::HunterBoxing => "Сосредоточиться на охоте",
            Self::Wrestling => "SNAP INTO A THIN TIM",
            Self::Judo => "Вспомнить основы",
        }
    }

    /// tg `max_streak_length`.
    fn max_streak(self) -> usize {
        match self {
            Self::Judo => 12,
            _ => 6,
        }
    }

    /// tg `combo_timer` of the styles with `display_combos`, seconds.
    fn combo_timer(self) -> Option<f64> {
        match self {
            Self::Cqc | Self::SleepingCarp => Some(6.0),
            Self::Judo => Some(15.0),
            _ => None,
        }
    }

    /// tg `pacifist_style`: punches a pacifist may still throw.
    fn pacifist_style(self) -> bool {
        self == Self::Boxing
    }

    /// tg `grab_state_modifier`: added to the effective grab state of a mob resisting the user.
    pub fn grab_state_modifier(self) -> u32 {
        u32::from(self == Self::SleepingCarp)
    }

    /// tg `TRAIT_BOXING_READY`.
    pub fn boxing(self) -> bool {
        matches!(self, Self::Boxing | Self::EvilBoxing | Self::HunterBoxing)
    }

    /// tg `can_teach`: boxing only to humans.
    fn can_teach(self, world: &World, mob: Entity) -> bool {
        world.get::<Health>(mob).is_some()
            && (!self.boxing() || world.get::<Inventory>(mob).is_some())
    }

    /// tg `get_style_help`.
    fn help(self) -> &'static [&'static str] {
        match self {
            Self::Cqc => cqc::HELP,
            Self::SleepingCarp => carp::HELP,
            Self::Boxing | Self::EvilBoxing | Self::HunterBoxing => boxing::HELP,
            Self::Wrestling => wrestling::HELP,
            Self::Judo => judo::HELP,
        }
    }
}

/// Where a known style comes from (tg `origin`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Read from a manual or scroll (tg `mindbound_martial_arts`): kept.
    Learned,
    /// Worn gloves or a belt (tg `martial_art_giver`): forgotten when taken off.
    Item(Entity),
}

/// One known style with its own streak (each tg datum keeps its own).
#[derive(Clone, Debug, PartialEq)]
pub struct Art {
    pub style: Style,
    pub origin: Origin,
    /// tg `streak`.
    pub streak: String,
    /// tg `current_target`.
    pub target: Option<Entity>,
    /// When the combo timer resets the streak; 0 = no timer running.
    pub streak_resets_at: f64,
    /// CQC `restraining_mob` and when it lets go (tg's 5 s `VARSET_CALLBACK`).
    pub restraining: Option<(Entity, f64)>,
}

impl Art {
    pub fn new(style: Style, origin: Origin) -> Self {
        Self {
            style,
            origin,
            streak: String::new(),
            target: None,
            streak_resets_at: 0.0,
            restraining: None,
        }
    }

    /// tg `add_to_streak` at `now`.
    pub fn add_to_streak(&mut self, element: &str, defender: Entity, now: f64) {
        // The combo timer's `reset_streak(null)`.
        if self.streak_resets_at > 0.0 && now >= self.streak_resets_at {
            self.reset_streak(None);
        }
        if self.target != Some(defender) {
            self.reset_streak(Some(defender));
        }
        self.streak.push_str(element);
        if self.streak.len() > self.style.max_streak() {
            // `copytext(streak, 1 + length(streak[1]))`: the streak is ASCII.
            self.streak.remove(0);
        }
        // `TIMER_UNIQUE`: a running timer is not restarted.
        if let Some(timer) = self.style.combo_timer()
            && self.streak_resets_at <= 0.0
        {
            self.streak_resets_at = now + timer;
        }
    }

    /// tg `reset_streak(new_target)` (CQC's override lets go of a mob that is not the new
    /// target).
    pub fn reset_streak(&mut self, new_target: Option<Entity>) {
        self.streak_resets_at = 0.0;
        self.target = new_target;
        self.streak.clear();
        if self.restraining.is_some_and(|(m, _)| Some(m) != new_target) {
            self.restraining = None;
        }
    }
}

/// The styles a mob knows, active first (tg `martial_arts`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct MartialArts(pub Vec<Art>);

impl MartialArts {
    pub fn active(&self) -> Option<Style> {
        self.0.first().map(|a| a.style)
    }
}

/// What a move did (tg `MARTIAL_ATTACK_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Not a move: the normal attack follows.
    Invalid,
    /// A move that failed: nothing else happens.
    Fail,
    /// A move: nothing else happens.
    Success,
}

impl Outcome {
    fn from_bool(done: bool) -> Self {
        if done { Self::Success } else { Self::Invalid }
    }
}

/// tg `attack_type` of `check_block`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackKind {
    /// `MELEE_ATTACK`: a held item.
    Melee,
    /// `UNARMED_ATTACK`: a punch, a shove, a grab, a martial move.
    Unarmed,
    /// `PROJECTILE_ATTACK`.
    Projectile,
}

/// A granter's uses left (tg `uses`); added on its first read.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GranterUses(pub u32);

/// A reading step of a martial granter (a `crate::tools::do_after` job): pages 0-2 are tg
/// `turn_page`, page 3 the last `do_after`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadJob {
    pub page: u8,
    pub style: Style,
}

/// tg `pages_to_mastery`, `reading_time` (deciseconds).
const PAGES_TO_MASTERY: u8 = 3;
const READING_TIME_DS: f32 = 50.0;
const PAGE_TURN_SOUNDS: [&str; 3] = [
    "sound/effects/page_turn/pageturn1.ogg",
    "sound/effects/page_turn/pageturn2.ogg",
    "sound/effects/page_turn/pageturn3.ogg",
];

/// Granters: (type, style, `martial_name`).
static GRANTERS: [(&str, Style, &str); 2] = [
    (
        "/obj/item/book/granter/martial/cqc",
        Style::Cqc,
        "основы CQC",
    ),
    (
        "/obj/item/book/granter/martial/carp",
        Style::SleepingCarp,
        "основы Спящего карпа",
    ),
];

/// Worn items that give a style (tg `martial_art_giver`), most specific first, with the slot
/// their `slot_flags` allow.
static GIVERS: [(&str, Style, InventorySlot); 6] = [
    (
        "/obj/item/clothing/gloves/boxing/evil",
        Style::EvilBoxing,
        InventorySlot::Gloves,
    ),
    (
        "/obj/item/clothing/gloves/boxing",
        Style::Boxing,
        InventorySlot::Gloves,
    ),
    (
        "/obj/item/clothing/gloves/fingerless/punch_mitts",
        Style::HunterBoxing,
        InventorySlot::Gloves,
    ),
    (
        "/obj/item/clothing/gloves/the_sleeping_carp",
        Style::SleepingCarp,
        InventorySlot::Gloves,
    ),
    (
        "/obj/item/storage/belt/champion/wrestling",
        Style::Wrestling,
        InventorySlot::Belt,
    ),
    (
        "/obj/item/storage/belt/judobelt",
        Style::Judo,
        InventorySlot::Belt,
    ),
];

/// `/obj/item/clothing/gloves/gauntlets` (Lavaland) also gives hunter boxing.
const GAUNTLETS: &str = "/obj/item/clothing/gloves/gauntlets";

/// The style a worn item gives in `slot`.
pub fn giver_style(path: &str, slot: InventorySlot) -> Option<Style> {
    if is_under(path, GAUNTLETS) && slot == InventorySlot::Gloves {
        return Some(Style::HunterBoxing);
    }
    GIVERS
        .iter()
        .find(|(root, ..)| is_under(path, root))
        .filter(|(.., s)| *s == slot)
        .map(|(_, style, _)| *style)
}

/// The prefix of the martial action buttons' ids.
const BUTTON: &str = "martial:";
const HELP_BUTTON: &str = "martial:help";
const SWAP_BUTTON: &str = "martial:swap";

pub struct MartialPlugin;

impl Plugin for MartialPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // A martial arts manual or scroll is read.
        app.add_click_handler(ClickPhase::AttackSelf, 2800, |w, c| {
            crate::martial::attack_self(w, c.client, c.mob, c.target)
        });
        app.add_systems(
            Update,
            (sync_worn_styles, trigger_buttons)
                .chain()
                .after(crate::click::ResolveClicks)
                .before(StatusSystems)
                .before(HealthSystems),
        );
    }
}

// --- Knowing styles ---

/// The mob's active style.
pub fn active_style(world: &World, mob: Entity) -> Option<Style> {
    world.get::<MartialArts>(mob).and_then(MartialArts::active)
}

/// tg `teach`: false when the mob cannot learn it or already holds this very art.
pub fn teach(world: &mut World, mob: Entity, style: Style, origin: Origin) -> bool {
    if !style.can_teach(world, mob) {
        return false;
    }
    if let Origin::Item(_) = origin
        && world
            .get::<MartialArts>(mob)
            .is_some_and(|m| m.0.iter().any(|a| a.origin == origin))
    {
        return false;
    }
    let old = active_style(world, mob);
    let mut arts = world.get::<MartialArts>(mob).cloned().unwrap_or_default();
    // `LAZYADD`, then `switch_style` to it: the new art goes first.
    arts.0.insert(0, Art::new(style, origin));
    world.entity_mut(mob).insert(arts);
    if let Some(old) = old {
        deactivate(world, mob, old);
    }
    activate(world, mob, style);
    refresh_buttons(world, mob);
    true
}

/// tg `unlearn` of the art from `origin`; false when the mob does not know it.
pub fn unlearn(world: &mut World, mob: Entity, origin: Origin) -> bool {
    let Some(mut arts) = world.get::<MartialArts>(mob).cloned() else {
        return false;
    };
    let Some(index) = arts.0.iter().position(|a| a.origin == origin) else {
        return false;
    };
    let art = arts.0.remove(index);
    let next = arts.active();
    if arts.0.is_empty() {
        world.entity_mut(mob).remove::<MartialArts>();
    } else {
        world.entity_mut(mob).insert(arts);
    }
    if index == 0 {
        deactivate(world, mob, art.style);
        if let Some(next) = next {
            activate(world, mob, next);
        }
    }
    refresh_buttons(world, mob);
    true
}

/// tg `activate_style`.
fn activate(world: &mut World, mob: Entity, style: Style) {
    match style {
        Style::Wrestling => {
            tell_mob(world, mob, Span::BoldWarning, "SNAP INTO A THIN TIM!");
            tell_mob(
                world,
                mob,
                Span::Danger,
                "Наведите курсор на приём вверху экрана, чтобы узнать, что он делает.",
            );
        }
        Style::Judo => {
            tell_mob(
                world,
                mob,
                Span::BoldWarning,
                "Наниты, содержащиеся в поясе, наделяют вас мастерством обладателя черного пояса по корпоративному дзюдо!",
            );
            judo::drop_batons(world, mob);
        }
        _ => {}
    }
}

/// tg `deactivate_style`.
fn deactivate(world: &mut World, mob: Entity, style: Style) {
    match style {
        Style::Wrestling => tell_mob(
            world,
            mob,
            Span::BoldWarning,
            "Вы больше не чувствуете, что башня силы слишком сладка, чтобы быть кислой...",
        ),
        Style::Judo => tell_mob(
            world,
            mob,
            Span::BoldWarning,
            "Вы забываете мастерство корпоративного дзюдо...",
        ),
        _ => {}
    }
}

/// tg `cycle_style`: the active style goes last, the next one becomes active.
pub fn cycle_style(world: &mut World, mob: Entity) {
    let Some(mut arts) = world.get::<MartialArts>(mob).cloned() else {
        return;
    };
    if arts.0.len() < 2 {
        return;
    }
    arts.0.rotate_left(1);
    let (old, new) = (arts.0[arts.0.len() - 1].style, arts.0[0].style);
    world.entity_mut(mob).insert(arts);
    deactivate(world, mob, old);
    activate(world, mob, new);
    tell_mob(
        world,
        mob,
        Span::Notice,
        format!(
            "Вы прекращаете практиковать {} и начинаете практиковать {}.",
            old.name(),
            new.name()
        ),
    );
    refresh_buttons(world, mob);
}

/// Mutable access to the active art.
pub(crate) fn with_art<R>(
    world: &mut World,
    mob: Entity,
    f: impl FnOnce(&mut Art) -> R,
) -> Option<R> {
    let mut arts = world.get_mut::<MartialArts>(mob)?;
    let art = arts.0.first_mut()?;
    Some(f(art))
}

/// The active art's streak.
pub(crate) fn streak(world: &World, mob: Entity) -> String {
    world
        .get::<MartialArts>(mob)
        .and_then(|m| m.0.first())
        .map(|a| a.streak.clone())
        .unwrap_or_default()
}

pub(crate) fn add_to_streak(world: &mut World, mob: Entity, element: &str, defender: Entity) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    with_art(world, mob, |a| a.add_to_streak(element, defender, now));
}

pub(crate) fn reset_streak(world: &mut World, mob: Entity) {
    with_art(world, mob, |a| a.reset_streak(None));
}

// --- Worn styles ---

/// tg `martial_art_giver` `equipped`/`dropped`: the styles of the givers worn in their slot are
/// known, the others forgotten. Runs for the inventories that changed.
// Cost: per tick, the inventories changed this tick; two slots each.
fn sync_worn_styles(changed: Query<Entity, Changed<Inventory>>, mut commands: Commands) {
    for mob in &changed {
        commands.queue(move |world: &mut World| sync_worn(world, mob));
    }
}

fn sync_worn(world: &mut World, mob: Entity) {
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return;
    };
    let worn: Vec<(Entity, Style)> = [InventorySlot::Gloves, InventorySlot::Belt]
        .into_iter()
        .filter_map(|slot| {
            let item = inventory.slot(slot)?;
            let path = &world.get::<DmItem>(item)?.path;
            Some((item, giver_style(path, slot)?))
        })
        .collect();
    let given: Vec<Entity> = world
        .get::<MartialArts>(mob)
        .map(|m| {
            m.0.iter()
                .filter_map(|a| match a.origin {
                    Origin::Item(e) => Some(e),
                    Origin::Learned => None,
                })
                .collect()
        })
        .unwrap_or_default();
    for item in given {
        if !worn.iter().any(|(w, _)| *w == item) {
            unlearn(world, mob, Origin::Item(item));
        }
    }
    for (item, style) in worn {
        teach(world, mob, style, Origin::Item(item));
    }
    if active_style(world, mob) == Some(Style::Judo) {
        judo::drop_batons(world, mob);
    }
}

// --- Granters ---

/// tg `/obj/item/book/granter/attack_self` of a martial manual or scroll in the active hand.
/// False for other items.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some((_, style, martial_name)) = world
        .get::<DmItem>(item)
        .and_then(|i| GRANTERS.iter().find(|(p, ..)| is_under(&i.path, p)))
        .copied()
    else {
        return false;
    };
    if world.get::<crate::tools::DoAfter>(mob).is_some() {
        tell(world, client, Span::Warning, "Вы уже читаете это!");
        return true;
    }
    if crate::vision::is_blind(world, mob) {
        tell(
            world,
            client,
            Span::Warning,
            "Вы слепы и не можете ничего прочесть!",
        );
        return true;
    }
    if !can_learn(world, client, mob, style, martial_name) {
        return true;
    }
    let uses = world.get::<GranterUses>(item).map_or(1, |u| u.0);
    if uses == 0 {
        recoil(world, client, mob, item, style);
        return true;
    }
    world.entity_mut(item).insert(GranterUses(uses));
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы начинаете читать {martial_name}..."),
    );
    start_page(world, client, mob, item, ReadJob { page: 0, style });
    true
}

/// tg `/obj/item/book/granter/martial/can_learn`.
fn can_learn(world: &mut World, client: Entity, mob: Entity, style: Style, martial: &str) -> bool {
    // BANDASTATION ADDITION: vampires cannot learn martial arts.
    if world
        .get::<crate::antag::vampire::server::Vampire>(mob)
        .is_some()
    {
        tell(
            world,
            client,
            Span::Warning,
            "Ваши вампирские силы не позволяют освоить это боевое искусство!",
        );
        return false;
    }
    if world
        .get::<MartialArts>(mob)
        .is_some_and(|m| m.0.iter().any(|a| a.style == style))
    {
        tell(
            world,
            client,
            Span::Warning,
            format!("Вы уже освоили {martial}!"),
        );
        return false;
    }
    true
}

/// tg `turn_page` (its sound, then a `do_after`) or the last read.
fn start_page(world: &mut World, client: Entity, mob: Entity, item: Entity, job: ReadJob) {
    if job.page < PAGES_TO_MASTERY {
        let sound = {
            let mut rng = world.resource_mut::<CombatRng>();
            PAGE_TURN_SOUNDS[rng.range(0, 2) as usize]
        };
        play(world, mob, sound, 30.0);
    }
    let actor = Actor {
        client,
        mob,
        item: Some(item),
    };
    crate::tools::do_after(
        world,
        actor,
        mob,
        READING_TIME_DS,
        0,
        crate::construction::Job::Martial(job),
    );
}

/// A reading step finished: a remark and the next page, or the style learned.
pub fn complete_reading(
    world: &mut World,
    mob: Entity,
    action: &crate::tools::DoAfter,
    job: ReadJob,
) {
    let (client, Some(item)) = (action.client, action.item) else {
        return;
    };
    if job.page < PAGES_TO_MASTERY {
        let remarks = match job.style {
            Style::Cqc => cqc::REMARKS,
            _ => carp::REMARKS,
        };
        let remark = {
            let mut rng = world.resource_mut::<CombatRng>();
            remarks[rng.range(0, remarks.len() as u32 - 1) as usize]
        };
        tell(world, client, Span::Notice, remark);
        start_page(
            world,
            client,
            mob,
            item,
            ReadJob {
                page: job.page + 1,
                ..job
            },
        );
        return;
    }
    // Checked again at the moment of effect: another copy may have been read meanwhile.
    let Some((_, _, martial_name)) = GRANTERS.iter().find(|(_, s, _)| *s == job.style).copied()
    else {
        return;
    };
    let uses = world.get::<GranterUses>(item).map_or(1, |u| u.0);
    if uses == 0 || !can_learn(world, client, mob, job.style, martial_name) {
        return;
    }
    world.entity_mut(item).insert(GranterUses(uses - 1));
    teach(world, mob, job.style, Origin::Learned);
    match job.style {
        Style::Cqc => {
            tell(world, client, Span::Danger, "Вы освоили основы CQC.");
            if uses - 1 == 0 {
                let name = name(world, item);
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!("{} зловеще пищит...", capitalize(&name)),
                );
            }
        }
        _ => {
            tell(world, client, Span::SciRadio, carp::GREET);
            if uses - 1 == 0 {
                carp::blank_scroll(world, item);
            }
        }
    }
}

/// tg `recoil` of a granter with no uses left: the CQC manual explodes.
fn recoil(world: &mut World, client: Entity, mob: Entity, item: Entity, style: Style) {
    if style != Style::Cqc {
        return;
    }
    let name = name(world, item);
    tell(
        world,
        client,
        Span::Warning,
        format!("{} взрывается!", capitalize(&name)),
    );
    play(world, mob, "sound/effects/explosion/explosion1.ogg", 40.0);
    crate::vision::flash_act(world, mob, 1, true, false, crate::vision::FLASH_LENGTH);
    hurt(world, mob, None, DamageType::Brute, 6.0, None);
    hurt(world, mob, None, DamageType::Burn, 6.0, None);
    world.despawn(item);
}

// --- Hooks ---

/// tg `unarmed_strike` (`COMSIG_LIVING_UNARMED_ATTACK`) of the attacker's active style for an
/// empty-hand click on a living mob. True when the style took the click (`Fail`, `Success`).
pub(crate) fn unarmed_attack(world: &mut World, click: &ClickResolved, now: f64) -> bool {
    let (attacker, defender) = (click.mob, click.target);
    let Some(style) = active_style(world, attacker) else {
        return false;
    };
    if world.get::<Health>(defender).is_none() || !can_use(world, attacker, style) {
        return false;
    }
    let combat = world.get::<CombatMode>(attacker).is_some_and(|c| c.0);
    // tg `ClickOn`: `changeNext_move(CLICK_CD_MELEE)` before `UnarmedAttack`; a move may change
    // it. Restored when the style lets the normal attack through, which sets its own.
    let old_next = world.get::<NextMove>(attacker).copied();
    world
        .entity_mut(attacker)
        .insert(NextMove(now + CLICK_CD_MELEE));
    face(world, attacker, defender);
    let outcome = if click.modifiers.right {
        disarm_act(world, style, attacker, defender)
    } else if combat {
        if world
            .get::<crate::medical::traumas::Pacifist>(attacker)
            .is_some()
            && !style.pacifist_style()
        {
            Outcome::Invalid
        } else {
            harm_act(world, style, attacker, defender)
        }
    } else {
        help_act(world, style, attacker, defender)
    };
    if outcome == Outcome::Invalid {
        match old_next {
            Some(n) => {
                world.entity_mut(attacker).insert(n);
            }
            None => {
                world.entity_mut(attacker).remove::<NextMove>();
            }
        }
        return false;
    }
    true
}

/// tg `attempt_grab` (`COMSIG_LIVING_GRAB`) of the puller's style, then the target's
/// `check_block`, for a ctrl-click on a living mob. True when the grab ends here.
pub(crate) fn grab(world: &mut World, attacker: Entity, defender: Entity) -> bool {
    if let Some(style) = active_style(world, attacker)
        && can_use(world, attacker, style)
    {
        face(world, attacker, defender);
        if grab_act(world, style, attacker, defender) != Outcome::Invalid {
            return true;
        }
    }
    let text = format!("захват {}", name(world, attacker));
    check_block(
        world,
        defender,
        Some(attacker),
        AttackKind::Unarmed,
        0.0,
        &text,
    )
}

/// tg `check_block` (`COMSIG_LIVING_CHECK_BLOCK`) of the defender's active style: true when the
/// attack is blocked (the style printed why).
pub(crate) fn check_block(
    world: &mut World,
    defender: Entity,
    attacker: Option<Entity>,
    kind: AttackKind,
    damage: f32,
    attack_text: &str,
) -> bool {
    let Some(style) = active_style(world, defender) else {
        return false;
    };
    if !can_use(world, defender, style) {
        return false;
    }
    match style {
        Style::Cqc => cqc::block(world, defender, attacker, kind, attack_text),
        Style::SleepingCarp => carp::dodge(world, defender, kind, attack_text),
        Style::Boxing | Style::EvilBoxing | Style::HunterBoxing => {
            boxing::block(world, style, defender, attacker, kind, damage, attack_text)
        }
        Style::Wrestling | Style::Judo => false,
    }
}

/// The sleeping carp's `hit_by_projectile`: true when `mob` deflects `projectile` (now its
/// own, flying a random way).
pub(crate) fn deflect_projectile(
    world: &mut World,
    mob: Entity,
    entity: Entity,
    projectile: &mut crate::projectile::Projectile,
) -> bool {
    active_style(world, mob) == Some(Style::SleepingCarp)
        && carp::deflect(world, mob, entity, projectile)
}

/// tg `TRAIT_NOGUNS` (the sleeping carp): the mob cannot fire guns.
pub fn no_guns(world: &World, mob: Entity) -> bool {
    active_style(world, mob) == Some(Style::SleepingCarp)
}

/// tg `TRAIT_PERFECT_ATTACKER` (the sleeping carp): unarmed strikes never miss.
pub fn perfect_attacker(arts: Option<&MartialArts>) -> bool {
    arts.and_then(MartialArts::active) == Some(Style::SleepingCarp)
}

/// tg `grabbed_resisting` of the puller's style: added to the effective grab state.
pub fn grab_state_modifier(world: &World, puller: Entity) -> u32 {
    active_style(world, puller)
        .filter(|&s| can_use(world, puller, s))
        .map_or(0, Style::grab_state_modifier)
}

/// tg `can_use`.
fn can_use(world: &World, mob: Entity, style: Style) -> bool {
    match style {
        Style::Boxing | Style::EvilBoxing | Style::HunterBoxing => {
            world.get::<Inventory>(mob).is_some()
        }
        _ => true,
    }
}

fn harm_act(world: &mut World, style: Style, a: Entity, d: Entity) -> Outcome {
    match style {
        Style::Cqc => cqc::harm(world, a, d),
        Style::SleepingCarp => carp::harm(world, a, d),
        Style::Boxing | Style::EvilBoxing | Style::HunterBoxing => boxing::harm(world, style, a, d),
        Style::Wrestling => wrestling::act(world, a, d),
        Style::Judo => judo::harm(world, a, d),
    }
}

fn disarm_act(world: &mut World, style: Style, a: Entity, d: Entity) -> Outcome {
    match style {
        Style::Cqc => cqc::disarm(world, a, d),
        Style::SleepingCarp => carp::disarm(world, a, d),
        Style::Boxing | Style::EvilBoxing | Style::HunterBoxing => {
            boxing::disarm(world, style, a, d)
        }
        Style::Wrestling => wrestling::act(world, a, d),
        Style::Judo => judo::disarm(world, a, d),
    }
}

fn grab_act(world: &mut World, style: Style, a: Entity, d: Entity) -> Outcome {
    match style {
        Style::Cqc => cqc::grab(world, a, d),
        Style::SleepingCarp => carp::grab(world, a, d),
        Style::Boxing | Style::EvilBoxing | Style::HunterBoxing => boxing::grab(world, style, a),
        Style::Wrestling => wrestling::grab(world, a, d),
        Style::Judo => judo::grab(world, a, d),
    }
}

fn help_act(world: &mut World, style: Style, a: Entity, d: Entity) -> Outcome {
    match style {
        Style::Judo => judo::help(world, a, d),
        _ => Outcome::Invalid,
    }
}

// --- Buttons ---

fn button(world: &World, id: &str, name: &str, dmi: &str, state: &str) -> (ActionButton, f64) {
    let icon = |dmi: &str, state: &str| {
        world
            .get_resource::<IconIndex>()
            .and_then(|index| index.get(dmi, state))
    };
    (
        ActionButton {
            id: id.to_owned(),
            name: name.to_owned(),
            icon: icon(dmi, state),
            background: icon("icons/mob/actions/backgrounds.dmi", "bg_martial_arts"),
            border: None,
            available: true,
            cooldown: 0.0,
            active: false,
            maptext: None,
        },
        0.0,
    )
}

/// The mob's martial buttons after the others it has (tg `swap_arts` and the styles' actions).
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
    let (active, count) = world
        .get::<MartialArts>(mob)
        .map_or((None, 0), |m| (m.active(), m.0.len()));
    if let Some(style) = active {
        buttons.push(button(
            world,
            HELP_BUTTON,
            style.help_verb(),
            "icons/hud/actions.dmi",
            "martial",
        ));
        if count >= 2 {
            buttons.push(button(
                world,
                SWAP_BUTTON,
                "Сменить стиль",
                "icons/hud/actions.dmi",
                "martial",
            ));
        }
        match style {
            Style::Wrestling => {
                for (id, name, state) in wrestling::MOVES {
                    buttons.push(button(world, id, name, "icons/hud/actions.dmi", state));
                }
            }
            Style::Judo => buttons.push(button(
                world,
                judo::DISCOMBOBULATE_BUTTON,
                "Сбить с толку",
                "modular_bandastation/martial_arts/icons/actions.dmi",
                "discombobulate",
            )),
            _ => {}
        }
    }
    if buttons.is_empty() && world.get::<ActionButtons>(mob).is_none() {
        return;
    }
    set_buttons(world, mob, buttons);
}

/// A click on a martial button, checked at the moment of effect: the client controls the mob
/// and its active style still has the button.
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
        let Some(style) = active_style(world, actor) else {
            continue;
        };
        match id.as_str() {
            HELP_BUTTON => {
                for line in style.help() {
                    tell(world, client, Span::Notice, *line);
                }
            }
            SWAP_BUTTON => cycle_style(world, actor),
            _ if style == Style::Wrestling => wrestling::trigger(world, actor, &id),
            judo::DISCOMBOBULATE_BUTTON if style == Style::Judo => judo::trigger(world, actor),
            _ => {}
        }
    }
}

// --- Shared effects ---

pub(crate) fn name(world: &World, e: Entity) -> String {
    crate::security::name_of(world, e)
}

pub(crate) fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

pub(crate) fn tell_mob(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

pub(crate) fn lying(world: &World, mob: Entity) -> bool {
    world.get::<BodyPosition>(mob).is_some_and(|b| b.lying())
}

pub(crate) fn stat(world: &World, mob: Entity) -> Stat {
    world.get::<Stat>(mob).copied().unwrap_or(Stat::Dead)
}

/// tg `IS_UNCONSCIOUS`: hard crit and worse, or knocked out (`crate::status_effects`).
pub(crate) fn unconscious(world: &World, mob: Entity) -> bool {
    stat(world, mob) >= Stat::HardCrit || crate::status_effects::knocked_out(world, mob)
}

/// tg `IS_UNCONSCIOUS_OR_CRIT`.
pub(crate) fn unconscious_or_crit(world: &World, mob: Entity) -> bool {
    stat(world, mob) >= Stat::SoftCrit || crate::status_effects::knocked_out(world, mob)
}

pub(crate) fn stamina_loss(world: &World, mob: Entity) -> f32 {
    world.get::<StatusEffects>(mob).map_or(0.0, |e| e.stamina)
}

pub(crate) fn effects(world: &World, mob: Entity) -> StatusEffects {
    world.get::<StatusEffects>(mob).cloned().unwrap_or_default()
}

/// tg `INCAPACITATED_IGNORING(mob, INCAPABLE_GRAB)`.
pub(crate) fn incapacitated(world: &World, mob: Entity) -> bool {
    stat(world, mob) != Stat::Conscious
        || effects(world, mob).incapacitated()
        || crate::status_effects::incapacitation(world, mob).incapacitated
}

pub(crate) fn throw_mode(world: &World, mob: Entity) -> bool {
    world
        .get::<crate::throw::ThrowMode>(mob)
        .is_some_and(|t| t.0)
}

pub(crate) fn combat_mode(world: &World, mob: Entity) -> bool {
    world.get::<CombatMode>(mob).is_some_and(|c| c.0)
}

pub(crate) fn adjacent(world: &World, a: Entity, b: Entity) -> bool {
    match (world.get::<TilePos>(a), world.get::<TilePos>(b)) {
        (Some(x), Some(y)) => x.chebyshev(*y).is_some_and(|d| d <= 1),
        _ => false,
    }
}

pub(crate) fn chance(world: &mut World, percent: f32) -> bool {
    world.resource_mut::<CombatRng>().chance(percent)
}

pub(crate) fn roll(world: &mut World, low: u32, high: u32) -> u32 {
    world.resource_mut::<CombatRng>().range(low, high)
}

/// The attacker turns to the defender (tg `face_atom` of a click).
fn face(world: &mut World, a: Entity, d: Entity) {
    if let Some(dir) = dir_to(world, a, d)
        && let Some(mut facing) = world.get_mut::<Facing>(a)
    {
        facing.set_if_neq(Facing(dir));
    }
}

/// `get_dir(a, d)`, else the way `a` faces (same tile).
pub(crate) fn dir_to(world: &World, a: Entity, d: Entity) -> Option<Dir> {
    let (from, to) = (world.get::<TilePos>(a)?, world.get::<TilePos>(d)?);
    crate::pull::dir_between(*from, *to).or_else(|| world.get::<Facing>(a).map(|f| f.0))
}

pub(crate) fn play(world: &mut World, at: Entity, file: &str, volume: f32) {
    if let Some(&pos) = world.get::<TilePos>(at) {
        crate::sound::playsound(
            world,
            PlaySound::new(pos, file, volume, true).extrarange(-1.0),
        );
    }
}

/// tg `defender.visible_message(others, to_defender, ..., range, attacker)` and
/// `to_chat(attacker, to_attacker)`.
pub(crate) fn message(
    world: &mut World,
    attacker: Entity,
    defender: Entity,
    range: u32,
    others: (Span, String),
    to_defender: String,
    to_attacker: Option<(Span, String)>,
) {
    let Some(&at) = world.get::<TilePos>(defender) else {
        return;
    };
    let line = |span, text: &str| ChatMessage {
        channel: ChatChannel::Combat,
        segments: vec![ChatSegment::new(span, text)],
    };
    let client = |world: &World, e: Entity| world.get::<PlayerMob>(e).map(|p| p.client);
    let clients = crate::obj_damage::clients_in_range(world, at, range, &[attacker, defender]);
    let mut out = vec![SendChat {
        clients,
        message: line(others.0, &others.1),
    }];
    if let Some(c) = client(world, defender) {
        out.push(SendChat {
            clients: vec![c],
            message: line(Span::BoldWarning, &to_defender),
        });
    }
    // The attacker is in tg's `ignored_mobs`: only its own `to_chat`, if any.
    if let (Some((span, text)), Some(c), true) =
        (to_attacker, client(world, attacker), attacker != defender)
    {
        out.push(SendChat {
            clients: vec![c],
            message: line(span, &text),
        });
    }
    for chat in out {
        world.write_message(chat);
    }
}

/// `range` for messages without one (tg `vision_distance = null`).
pub(crate) const NEAR: u32 = DEFAULT_MESSAGE_RANGE;
/// `COMBAT_MESSAGE_RANGE`.
pub(crate) const COMBAT: u32 = COMBAT_MESSAGE_RANGE;

/// tg `apply_damage(amount, kind, zone)`: no zone hits the chest; stamina goes to
/// `adjust_stamina_loss`. `wound_bonus` `None` is `CANT_WOUND`.
pub(crate) fn hurt(
    world: &mut World,
    mob: Entity,
    zone: Option<BodyZone>,
    kind: DamageType,
    amount: f32,
    wound_bonus: Option<f32>,
) {
    if amount <= 0.0 {
        return;
    }
    if kind == DamageType::Stamina {
        crate::status_effects::adjust_stamina(world, mob, amount);
        return;
    }
    let zone = zone.unwrap_or(BodyZone::Chest);
    let multiplier = world
        .get::<crate::medical::wounds::Limbs>(mob)
        .map_or(1.0, |l| l.limb(zone).damage_multiplier());
    let amount = amount * multiplier;
    let Ok(mut entity) = world.get_entity_mut(mob) else {
        return;
    };
    let Ok((mut health, mut stat)) = entity.get_components_mut::<(&mut Health, &mut Stat)>() else {
        return;
    };
    let before = health.part(zone).damage();
    let mut new_stat = *stat;
    apply_damage_zone(&mut health, &mut new_stat, zone, kind, amount);
    stat.set_if_neq(new_stat);
    if let Some(wound_bonus) = wound_bonus
        && matches!(kind, DamageType::Brute | DamageType::Burn)
    {
        let hit = crate::medical::wounds::Hit {
            kind,
            damage: amount,
            sharpness: 0,
            wound_bonus,
            exposed_wound_bonus: 0.0,
            part_damage_before: before,
        };
        crate::medical::wounds::after_hit(world, mob, zone, hit);
    }
}

/// tg `Stun`/`Paralyze`/`Knockdown`/`Immobilize(secs)`.
pub(crate) fn incap(world: &mut World, mob: Entity, kind: Incap, secs: f32) {
    crate::status_effects::apply(world, mob, kind, secs);
}

/// tg `Unconscious(secs)`.
pub(crate) fn knock_out(world: &mut World, mob: Entity, secs: f32) {
    incap(world, mob, Incap::Unconscious, secs);
}

/// tg `SetSleeping(secs)`.
pub(crate) fn put_to_sleep(world: &mut World, mob: Entity, secs: f32) {
    crate::status_effects::set(world, mob, Incap::Sleeping, secs);
}

/// tg `drop_all_held_items`.
pub(crate) fn drop_held(world: &mut World, mob: Entity) {
    let Some(&at) = world.get::<TilePos>(mob) else {
        return;
    };
    let held = world
        .get::<Inventory>(mob)
        .map(|i| i.hands)
        .unwrap_or_default();
    for item in held.into_iter().flatten() {
        if !crate::inventory::no_drop(world, item) {
            crate::inventory::put_down(world, item, at);
        }
    }
}

/// The defender's active item into the attacker's hands (tg `temporarilyRemoveItemFromInventory`
/// then `put_in_hands`). Returns it.
pub(crate) fn take_active_item(world: &mut World, from: Entity, to: Entity) -> Option<Entity> {
    let item = world
        .get::<Inventory>(from)
        .and_then(Inventory::active_item)?;
    if crate::inventory::no_drop(world, item) {
        return None;
    }
    world.entity_mut(item).remove::<CarriedBy>();
    match world.get::<Inventory>(to).cloned() {
        Some(inventory) => crate::storage::put_in_hands(world, to, &inventory, item),
        None => {
            if let Some(&at) = world.get::<TilePos>(from) {
                world.entity_mut(item).insert(at);
            }
        }
    }
    Some(item)
}

/// tg `CARBON_MAX_IMPACT_SPEED_BONUS`; a mob's `throw_speed`.
const MAX_IMPACT_SPEED_BONUS: f32 = 5.0;
const MOB_THROW_SPEED: f32 = 2.0;

/// tg `throw_at(get_edge_target_turf(mob, dir), range, speed, thrower)` of a carbon, landing
/// with `throw_impact` on what stops it: a wall paralyzes 2 s and hurts `10 + 5 × extra speed`,
/// another carbon is knocked over with it.
// shortcut: the flight is instant (tile steps this tick), no spin, no catching, no thrower
// momentum; add with mob throwing in `crate::throw`.
pub(crate) fn fling(world: &mut World, mob: Entity, dir: Dir, range: u32, speed: f32) {
    let extra = (speed - MOB_THROW_SPEED).clamp(0.0, MAX_IMPACT_SPEED_BONUS);
    for _ in 0..range {
        let Err(blockers) = crate::pull::step_once(world, mob, dir, None) else {
            continue;
        };
        let Some(&hit) = blockers.first() else {
            return;
        };
        let what = name(world, hit);
        let fast = if extra > 0.0 {
            " с большой скоростью"
        } else {
            ""
        };
        let who = name(world, mob);
        let carbon = world.get::<Inventory>(hit).is_some();
        if world.get::<ss13_mapload::Turf>(hit).is_some() {
            incap(world, mob, Incap::Paralyze, 2.0);
            hurt(
                world,
                mob,
                None,
                DamageType::Brute,
                10.0 + 5.0 * extra,
                Some(extra * 5.0),
            );
        } else if carbon {
            hurt(
                world,
                mob,
                None,
                DamageType::Brute,
                10.0 + 5.0 * extra,
                Some(extra * 5.0),
            );
            incap(world, mob, Incap::Paralyze, 2.0);
            incap(world, hit, Incap::Paralyze, 2.0);
        } else if extra > 0.0 {
            let structure = world
                .get::<ss13_mapload::DmObj>(hit)
                .is_some_and(|o| is_under(&o.path, "/obj/structure"));
            if structure {
                incap(world, mob, Incap::Paralyze, 1.0);
                hurt(
                    world,
                    mob,
                    None,
                    DamageType::Brute,
                    5.0 + 5.0 * extra,
                    Some(extra * 5.0),
                );
            } else {
                hurt(
                    world,
                    mob,
                    None,
                    DamageType::Brute,
                    5.0 * extra,
                    Some(extra * 5.0),
                );
            }
        }
        if let Some(&at) = world.get::<TilePos>(mob) {
            crate::security::show(
                world,
                at,
                NEAR,
                ChatChannel::Combat,
                (
                    Span::Danger,
                    format!("{} врезается в {what}{fast}!", capitalize(&who)),
                ),
                &[(
                    mob,
                    Span::BoldWarning,
                    format!("Вы врезаетесь в {what}{fast}!"),
                )],
            );
        }
        return;
    }
}

/// The active hand's arm (tg `get_active_hand`): left for hand 0.
pub(crate) fn active_arm(world: &World, mob: Entity) -> Option<BodyZone> {
    let inventory = world.get::<Inventory>(mob)?;
    let zone = if inventory.active == 0 {
        BodyZone::LArm
    } else {
        BodyZone::RArm
    };
    world
        .get::<Health>(mob)
        .is_some_and(|h| h.has_part(zone))
        .then_some(zone)
}

/// The attacker's selected zone (tg `zone_selected`, checked).
pub(crate) fn zone_selected(world: &World, mob: Entity) -> BodyZone {
    world
        .get::<ZoneSelected>(mob)
        .copied()
        .unwrap_or_default()
        .0
        .check_zone()
}

/// tg `apply_effect(secs, effect, blocked)`: shortened by the armor block.
pub(crate) fn armored(secs: f32, armor: f32) -> f32 {
    secs * (100.0 - armor).max(0.0) / 100.0
}

/// tg `run_armor_check(zone, MELEE, armour_penetration)` against the worn armor.
pub(crate) fn melee_armor(world: &World, mob: Entity, zone: BodyZone, penetration: f32) -> f32 {
    let worn = crate::armor::getarmor(world, mob, zone, "melee");
    crate::armor::run_armor_check(worn, penetration, false, None, None).0
}

/// Sets the item's name and description (an emptied scroll).
pub(crate) fn rename(world: &mut World, item: Entity, name: &str, desc: &str, icon: (&str, &str)) {
    if let Some(mut n) = world.get_mut::<DmName>(item) {
        n.name = name.to_owned();
        n.desc = desc.to_owned();
    }
    let id = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(icon.0, icon.1));
    if let (Some(id), Some(mut look)) = (id, world.get_mut::<Appearance>(item))
        && look.icon_state != id
    {
        look.icon_state = id;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(i: u32) -> Entity {
        Entity::from_raw_u32(i).unwrap()
    }

    #[test]
    fn streak_keeps_the_last_moves_on_one_target() {
        let mut art = Art::new(Style::Cqc, Origin::Learned);
        for c in ["H", "D", "G", "H", "D", "G", "H"] {
            art.add_to_streak(c, e(1), 0.0);
        }
        assert_eq!(art.streak, "DGHDGH", "max_streak_length 6");
        art.add_to_streak("D", e(2), 1.0);
        assert_eq!(art.streak, "D", "a new target starts over");
    }

    #[test]
    fn combo_timer_runs_from_the_first_move() {
        let mut art = Art::new(Style::Cqc, Origin::Learned);
        art.add_to_streak("H", e(1), 0.0);
        art.add_to_streak("H", e(1), 5.0);
        assert_eq!(art.streak, "HH");
        art.add_to_streak("D", e(1), 6.5);
        assert_eq!(
            art.streak, "D",
            "6 s after the first move the streak is gone"
        );
        let mut wrestling = Art::new(Style::Wrestling, Origin::Learned);
        wrestling.add_to_streak("H", e(1), 0.0);
        wrestling.add_to_streak("H", e(1), 100.0);
        assert_eq!(wrestling.streak, "HH", "no combo display, no timer");
    }

    #[test]
    fn givers_need_their_slot() {
        assert_eq!(
            giver_style(
                "/obj/item/clothing/gloves/boxing/evil",
                InventorySlot::Gloves
            ),
            Some(Style::EvilBoxing)
        );
        assert_eq!(
            giver_style(
                "/obj/item/clothing/gloves/boxing/green",
                InventorySlot::Gloves
            ),
            Some(Style::Boxing)
        );
        assert_eq!(
            giver_style("/obj/item/clothing/gloves/boxing", InventorySlot::LeftHand),
            None
        );
        assert_eq!(
            giver_style("/obj/item/storage/belt/judobelt", InventorySlot::Belt),
            Some(Style::Judo)
        );
    }
}
