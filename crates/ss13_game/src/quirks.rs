//! Quirks (server), after tg `code/datums/quirks` (`/datum/quirk`), `SSquirks`
//! (`code/controllers/subsystem/processing/quirks.dm`) and the preferences menu's quirk
//! middleware (`preferences/middleware/quirks.dm`).
//!
//! - The character's quirks are picked by name in the preferences window ([`QuirkPrefs`] on the
//!   client, `give_quirk`/`remove_quirk` acts checked by [`filter_invalid`]: blacklisted pairs,
//!   at most [`MAX_POSITIVE_QUIRKS`] positive ones and a non-negative point balance from
//!   [`DEFAULT_QUIRK_POINTS`]).
//! - A human joining the round gets them (`AssignQuirks` -> `add_quirk`, [`assign`]): a
//!   [`Quirks`] component, the quirk's own effect and, once, its items (`item_quirk`).
//! - Processing quirks act every `SSquirks` fire (1 s) while the holder is below hard crit.
//!
//! Ported quirks: Alcohol Tolerance and Light Drinker (booze power, `ss13_reagents`), Drunken
//! Resilience, Friendly (`crate::mood::on_hug`), Jolly, Voracious (nutrition moodlets),
//! Monochromacy, Blind and Nearsighted (`crate::vision`), Depression, Hypersensitive, Unstable
//! (`crate::mood`), Mute and Pacifist (`crate::medical::traumas::refresh`), Brain Tumor,
//! Bad Back, Paraplegic (an absolute paralysis trauma).
// shortcut: the other tg quirks wait for their mechanics (languages, addictions, allergies,
// phobias, skillchips, species, hallucinations, wheelchairs...); no medical record lines, no
// customization options (blindfold colour, glasses type, paraplegic variant), no hardcore or
// random quirks, no mail goodies, no quirk transfer between bodies; the point settings are tg's
// config defaults, not read from `config/game_options.txt`. Removing Paraplegic keeps its trauma.

use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;

use crate::combat::CombatRng;
use crate::health::{DamageType, Health, Stat, update_stat};
use crate::inventory::{CarriedBy, Inventory};
use crate::medical::organs::{self, Slot};
use crate::medical::traumas::{self, Kind, Paralysis, Resilience};
use crate::mood::{self, Mood, events};
use crate::outfit::OutfitTemplates;
use crate::server::PlayerMob;

/// `default_quirk_points` (config default).
pub const DEFAULT_QUIRK_POINTS: i32 = 2;
/// `max_positive_quirks` (BandaStation `config/game_options.txt`).
pub const MAX_POSITIVE_QUIRKS: usize = 6;
/// `SSquirks` `wait`, seconds.
const QUIRKS_WAIT_SECS: f64 = 1.0;
/// Jolly's and Depression's `SPT_PROB(0.416, seconds_per_tick)`.
const MOOD_SWING_PERCENT: f32 = 0.416;
/// Brain Tumor's `degradation_speed`, brain damage per second.
const TUMOR_DAMAGE_PER_SECOND: f32 = 0.2;
const BACKPACK: &str = "/obj/item/storage/backpack";
const BLINDFOLD: &str = "/obj/item/clothing/glasses/blindfold/white";
const GLASSES: &str = "/obj/item/clothing/glasses/regular";
const MANNITOL_BOTTLE: &str = "/obj/item/storage/pill_bottle/mannitol/braintumor";
/// Items the quirks hand out: `crate::jobs` builds their templates with the outfits'.
pub const ITEM_PATHS: [&str; 3] = [BLINDFOLD, GLASSES, MANNITOL_BOTTLE];

/// The ported tg quirk types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Quirk {
    AlcoholTolerance,
    DrunkHealing,
    Friendly,
    Jolly,
    Voracious,
    BadBack,
    Blind,
    BrainTumor,
    Depression,
    Hypersensitive,
    LightDrinker,
    Mute,
    Nearsighted,
    Pacifist,
    Paraplegic,
    Unstable,
    Monochromacy,
}

/// The static part of a quirk (tg vars).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuirkDef {
    pub quirk: Quirk,
    pub path: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    /// A FontAwesome icon (`FA_ICON_*`).
    pub icon: &'static str,
    /// Points: positive quirks cost them, negative ones give them.
    pub value: i32,
    pub gain: Option<(Span, &'static str)>,
    pub lose: Option<(Span, &'static str)>,
}

const fn q(
    quirk: Quirk,
    path: &'static str,
    name: &'static str,
    desc: &'static str,
    icon: &'static str,
    value: i32,
    gain: Option<(Span, &'static str)>,
    lose: Option<(Span, &'static str)>,
) -> QuirkDef {
    QuirkDef {
        quirk,
        path,
        name,
        desc,
        icon,
        value,
        gain,
        lose,
    }
}

/// Every ported quirk in tg's menu order (`cmp_quirk_asc`: positive, negative, neutral, then by
/// name). BandaStation texts.
pub const QUIRKS: [QuirkDef; 17] = [
    q(
        Quirk::AlcoholTolerance,
        "/datum/quirk/alcohol_tolerance",
        "Alcohol Tolerance",
        "Вы пьянеете медленнее и испытываете меньше проблем от алкоголя.",
        "beer",
        4,
        Some((
            Span::Notice,
            "Вам кажется, что вы можете выпить целую кегу пива!",
        )),
        Some((
            Span::Danger,
            "Вы больше не испытываете особой стойкости к алкоголю.",
        )),
    ),
    q(
        Quirk::DrunkHealing,
        "/datum/quirk/drunkhealing",
        "Drunken Resilience",
        "Ничто так не помогает почувствовать себя на вершине мира, как хорошая выпивка. Когда вы \
         пьяны, вы медленно восстанавливаетесь от повреждений.",
        "wine-bottle",
        8,
        Some((Span::Notice, "Вам кажется, что немного выпить не помешает.")),
        Some((
            Span::Danger,
            "Вам больше не хочется пить, чтобы облегчить боль.",
        )),
    ),
    q(
        Quirk::Friendly,
        "/datum/quirk/friendly",
        "Friendly",
        "Вы обнимаете лучше всех, особенно когда вы в хорошем настроении.",
        "hands-helping",
        2,
        Some((Span::Notice, "Вам хочется обнять кого-то.")),
        Some((
            Span::Danger,
            "Вы больше не чувствуете себя обязанным обнимать других.",
        )),
    ),
    q(
        Quirk::Jolly,
        "/datum/quirk/jolly",
        "Jolly",
        "Иногда вы просто чувствуете себя счастливым без всякой причины.",
        "grin",
        4,
        None,
        None,
    ),
    q(
        Quirk::Voracious,
        "/datum/quirk/voracious",
        "Voracious",
        "Ничто не встает между вами и едой. Вы едите быстрее и можете налегать на нездоровую \
         пищу! Быть толстым вам очень идет.",
        "drumstick-bite",
        4,
        Some((Span::Notice, "ХОЧУ ЖРАААААТЬ!!")),
        Some((Span::Danger, "Кажется вы больше не хотите ЖРАААААТЬ.")),
    ),
    q(
        Quirk::BadBack,
        "/datum/quirk/badback",
        "Bad Back",
        "Из-за плохой осанки рюкзаки и другие сумки никогда не сидят на вашей спине правильно. \
         Впрочем, более равномерно взвешенные предметы вполне подойдут.",
        "hiking",
        -8,
        Some((Span::Danger, "Ваша спина действительно болит!")),
        Some((Span::Notice, "Ваша спина чувствует себя лучше.")),
    ),
    q(
        Quirk::Blind,
        "/datum/quirk/item_quirk/blindness",
        "Blind",
        "Вы полностью слепой, и это необратимо.",
        "blind",
        -16,
        Some((Span::Danger, "Вы ослепли и ничего не видите.")),
        Some((Span::Notice, "Чудесным образом к вам вернулось зрение.")),
    ),
    q(
        Quirk::BrainTumor,
        "/datum/quirk/item_quirk/brainproblems",
        "Brain Tumor",
        "У вас в мозгу есть маленький друг, который медленно разрушает его. Лучше взять с собой \
         немного маннитола!",
        "brain",
        -12,
        Some((Span::Danger, "Вы ощущаете тяжесть мышления.")),
        Some((Span::Notice, "Вы снова здраво мыслите.")),
    ),
    q(
        Quirk::Depression,
        "/datum/quirk/depression",
        "Depression",
        "Иногда вы просто ненавидите жизнь.",
        "frown",
        -3,
        Some((Span::Danger, "У вас начинается депрессия.")),
        Some((Span::Notice, "Вы больше не испытываете депрессию.")),
    ),
    q(
        Quirk::Hypersensitive,
        "/datum/quirk/hypersensitive",
        "Hypersensitive",
        "К сожалению или к счастью, вам кажется, что всё влияет на ваше настроение сильнее \
         обычного.",
        "flushed",
        -2,
        Some((
            Span::Danger,
            "Вы, кажется, делаете из мухи слона где только возможно.",
        )),
        Some((
            Span::Notice,
            "Кажется, вы больше не придаете всему так много значения.",
        )),
    ),
    q(
        Quirk::LightDrinker,
        "/datum/quirk/light_drinker",
        "Light Drinker",
        "Вы просто не умеете обращаться с напитками и очень быстро пьянеете.",
        "cocktail",
        -2,
        Some((
            Span::Notice,
            "От одной мысли об употреблении алкоголя голова идет кругом.",
        )),
        Some((
            Span::Danger,
            "Вы больше не подвержены сильному влиянию алкоголя.",
        )),
    ),
    q(
        Quirk::Mute,
        "/datum/quirk/mute",
        "Mute",
        "По какой-то причине вы совершенно не можете говорить.",
        "volume-xmark",
        -4,
        Some((Span::Danger, "Вы обнаруживаете, что не можете говорить!")),
        Some((
            Span::Notice,
            "Вы чувствуете, как голосовые связки приходят в норму.",
        )),
    ),
    q(
        Quirk::Nearsighted,
        "/datum/quirk/item_quirk/nearsighted",
        "Nearsighted",
        "У вас близорукость, но зато с парой очков выписанных по рецепту.",
        "glasses",
        -4,
        Some((
            Span::Danger,
            "Предметы, находящиеся на большом расстоянии от вас, выглядят размыто.",
        )),
        Some((
            Span::Notice,
            "Вы начинаете видеть вещи на далеком расстоянии как обычно.",
        )),
    ),
    q(
        Quirk::Pacifist,
        "/datum/quirk/nonviolent",
        "Pacifist",
        "Мысли о насилии вызывают у вас тошноту. Настолько, что вы не можете причинить никому \
         вреда.",
        "peace",
        -8,
        Some((
            Span::Danger,
            "Вы чувствуете, как любая мысль о насилии отвращает вас!",
        )),
        Some((
            Span::Notice,
            "Кажется, вы уже и не такой беззащитный и можете дать отпор, как раньше.",
        )),
    ),
    q(
        Quirk::Paraplegic,
        "/datum/quirk/paraplegic",
        "Paraplegic",
        "У вас отказали ноги, и вы больше никогда не сможете ходить, зато у вас есть бесплатная \
         коляска!",
        "wheelchair",
        -12,
        // Handled by the trauma.
        None,
        None,
    ),
    q(
        Quirk::Unstable,
        "/datum/quirk/unstable",
        "Unstable",
        "Из-за прошлых неприятностей вы не сможете восстановить рассудок, если потеряете его. \
         Будьте очень осторожны в управлении своим настроением!",
        "angry",
        -10,
        Some((
            Span::Danger,
            "У вас сейчас довольно много разных мыслей на уме.",
        )),
        Some((Span::Notice, "Ваш разум наконец-то успокоился.")),
    ),
    q(
        Quirk::Monochromacy,
        "/datum/quirk/monochromatic",
        "Monochromacy",
        "Вы страдаете полным дальтонизмом и воспринимаете весь мир в черно-белых тонах.",
        "adjust",
        0,
        None,
        None,
    ),
];

/// `GLOB.quirk_blacklist`, the lists with at least two ported quirks: quirks of one list
/// exclude each other.
pub const BLACKLIST: [&[Quirk]; 3] = [
    &[Quirk::Blind, Quirk::Nearsighted],
    &[Quirk::Jolly, Quirk::Depression, Quirk::Hypersensitive],
    &[Quirk::AlcoholTolerance, Quirk::LightDrinker],
];

impl Quirk {
    pub fn def(self) -> &'static QuirkDef {
        QUIRKS
            .iter()
            .find(|d| d.quirk == self)
            .expect("every quirk has a definition")
    }

    /// `SSquirks.quirks[name]`.
    pub fn by_name(name: &str) -> Option<Self> {
        QUIRKS.iter().find(|d| d.name == name).map(|d| d.quirk)
    }
}

/// The quirks of a mob (tg `quirks`).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct Quirks(pub Vec<Quirk>);

impl Quirks {
    pub fn has(&self, quirk: Quirk) -> bool {
        self.0.contains(&quirk)
    }
}

/// On a client connection: the character's quirks by name (tg `prefs.all_quirks`).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct QuirkPrefs(pub Vec<String>);

impl QuirkPrefs {
    /// tg's menu balance: the free points left after the chosen quirks.
    pub fn balance(&self) -> i32 {
        DEFAULT_QUIRK_POINTS
            - self
                .0
                .iter()
                .filter_map(|n| Quirk::by_name(n))
                .map(|q| q.def().value)
                .sum::<i32>()
    }
}

/// tg `has_quirk`.
pub fn has(world: &World, mob: Entity, quirk: Quirk) -> bool {
    world.get::<Quirks>(mob).is_some_and(|q| q.has(quirk))
}

/// tg `SSquirks.filter_invalid_quirks`: drops unknown names, blacklisted pairs and positive
/// quirks past the limit, then, while the point balance is positive (more spent than given),
/// positive quirks from the first on. Returns the same list when all of it is valid.
pub fn filter_invalid(names: &[String]) -> Vec<String> {
    let mut new: Vec<String> = Vec::new();
    let mut positive: Vec<(String, i32)> = Vec::new();
    let mut balance = -DEFAULT_QUIRK_POINTS;
    for name in names {
        let Some(quirk) = Quirk::by_name(name) else {
            continue;
        };
        let blacklisted = BLACKLIST.iter().any(|list| {
            list.contains(&quirk)
                && list
                    .iter()
                    .any(|other| *other != quirk && new.iter().any(|n| n == other.def().name))
        });
        if blacklisted {
            continue;
        }
        let value = quirk.def().value;
        if value > 0 {
            if positive.len() == MAX_POSITIVE_QUIRKS {
                continue;
            }
            positive.push((name.clone(), value));
        }
        balance += value;
        new.push(name.clone());
    }
    if balance > 0 {
        let mut left = balance;
        for (name, value) in positive {
            left -= value;
            new.retain(|n| *n != name);
            if left <= 0 {
                break;
            }
        }
    }
    new
}

/// tg `give_quirk`/`remove_quirk` of the preferences middleware: the change is kept only when
/// the result is valid as a whole. True if it was applied.
pub fn change_prefs(prefs: &mut QuirkPrefs, name: &str, give: bool) -> bool {
    let current = filter_invalid(&prefs.0);
    let new: Vec<String> = if give {
        let mut new = current.clone();
        if !new.iter().any(|n| n == name) {
            new.push(name.to_owned());
        }
        new
    } else {
        if !current.iter().any(|n| n == name) {
            return false;
        }
        current.iter().filter(|n| *n != name).cloned().collect()
    };
    if filter_invalid(&new) != new {
        return false;
    }
    prefs.0 = new;
    true
}

/// tg `AssignQuirks`: the client's chosen quirks onto its new human, silently, with their items.
pub fn assign(world: &mut World, client: Entity, mob: Entity) {
    let Some(prefs) = world.get::<QuirkPrefs>(client) else {
        return;
    };
    let names = filter_invalid(&prefs.0);
    for name in names {
        if let Some(quirk) = Quirk::by_name(&name) {
            add_quirk(world, mob, quirk, false, true);
        }
    }
}

fn tell_mob(world: &mut World, mob: Entity, (span, text): (Span, &str)) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        crate::tools::tell(world, client, span, text);
    }
}

/// tg `add_quirk` -> `add_to_holder`: false if the mob has it already. `announce`: the gain
/// line; `unique`: the one-time effects (items).
pub fn add_quirk(
    world: &mut World,
    mob: Entity,
    quirk: Quirk,
    announce: bool,
    unique: bool,
) -> bool {
    if world.get_entity(mob).is_err() || has(world, mob, quirk) {
        return false;
    }
    match world.get_mut::<Quirks>(mob) {
        Some(mut quirks) => quirks.0.push(quirk),
        None => {
            world.entity_mut(mob).insert(Quirks(vec![quirk]));
        }
    }
    let def = quirk.def();
    info!("{mob} gained the quirk {}", def.path);
    // `add`.
    match quirk {
        Quirk::Hypersensitive => {
            if let Some(mut mood) = world.get_mut::<Mood>(mob) {
                mood.mood_modifier += 0.5;
                mood.update_mood();
            }
        }
        Quirk::Unstable => {
            if let Some(mut mood) = world.get_mut::<Mood>(mob) {
                mood.unstable = true;
            }
        }
        Quirk::Mute | Quirk::Pacifist => traumas::refresh(world, mob),
        // `/datum/brain_trauma/severe/paralysis/paraplegic`, `TRAUMA_RESILIENCE_ABSOLUTE`.
        Quirk::Paraplegic => {
            if let Some(brain) = organs::organ(world, mob, Slot::Brain) {
                traumas::gain(
                    world,
                    brain,
                    Kind::Paralysis(Paralysis::Legs),
                    Some(Resilience::Absolute),
                );
            }
        }
        _ => {}
    }
    if announce && let Some(gain) = def.gain {
        tell_mob(world, mob, gain);
    }
    if unique {
        match quirk {
            Quirk::Blind => {
                give_item(world, mob, BLINDFOLD, &[Location::Eyes, Location::Hands]);
            }
            Quirk::Nearsighted => {
                give_item(
                    world,
                    mob,
                    GLASSES,
                    &[Location::Eyes, Location::Backpack, Location::Hands],
                );
            }
            Quirk::BrainTumor => {
                if let Some(at) = give_item(
                    world,
                    mob,
                    MANNITOL_BOTTLE,
                    &[
                        Location::LeftPocket,
                        Location::RightPocket,
                        Location::Backpack,
                        Location::Hands,
                    ],
                ) {
                    let text = format!(
                        "У вас есть баночка таблеток маннитола {at}. Они помогут вам выжить, пока \
                         вы не получите запас лекарств. Не полагайтесь на них слишком сильно!"
                    );
                    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
                        crate::tools::tell(world, client, Span::BoldNotice, text);
                    }
                }
            }
            _ => {}
        }
    }
    true
}

/// tg `remove_quirk`: false if the mob does not have it.
pub fn remove_quirk(world: &mut World, mob: Entity, quirk: Quirk) -> bool {
    let Some(mut quirks) = world.get_mut::<Quirks>(mob) else {
        return false;
    };
    let Some(i) = quirks.0.iter().position(|q| *q == quirk) else {
        return false;
    };
    quirks.0.remove(i);
    match quirk {
        Quirk::Hypersensitive => {
            if let Some(mut mood) = world.get_mut::<Mood>(mob) {
                mood.mood_modifier -= 0.5;
                mood.update_mood();
            }
        }
        Quirk::Unstable => {
            if let Some(mut mood) = world.get_mut::<Mood>(mob) {
                mood.unstable = false;
            }
        }
        Quirk::Jolly => mood::clear_event(world, mob, "jolly"),
        Quirk::Depression => mood::clear_event(world, mob, "depression"),
        Quirk::BadBack => mood::clear_event(world, mob, "back_pain"),
        Quirk::Mute | Quirk::Pacifist => traumas::refresh(world, mob),
        _ => {}
    }
    if let Some(lose) = quirk.def().lose {
        tell_mob(world, mob, lose);
    }
    true
}

/// `/datum/quirk/alcohol_tolerance` (×0.7) and `light_drinker` (×2): the booze power of
/// `/datum/reagent/consumable/ethanol/on_mob_life`.
pub fn booze_multiplier(world: &World, mob: Entity) -> f32 {
    let Some(quirks) = world.get::<Quirks>(mob) else {
        return 1.0;
    };
    let mut multiplier = 1.0;
    if quirks.has(Quirk::AlcoholTolerance) {
        multiplier *= 0.7;
    }
    if quirks.has(Quirk::LightDrinker) {
        multiplier *= 2.0;
    }
    multiplier
}

/// tg `LOCATION_*` of `give_item_to_holder`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Location {
    Eyes,
    LeftPocket,
    RightPocket,
    Backpack,
    Hands,
}

/// tg `give_item_to_holder`: a new `path` into the first location that takes it, else at the
/// mob's feet. Where it went (`None`: no template for it).
fn give_item(
    world: &mut World,
    mob: Entity,
    path: &str,
    locations: &[Location],
) -> Option<&'static str> {
    if !world.contains_resource::<OutfitTemplates>() {
        return None;
    }
    let item = world.resource_scope(|world, templates: Mut<OutfitTemplates>| {
        Some(templates.0.get(path)?.spawn(world, None))
    });
    let Some(item) = item else {
        warn!("quirk item {path} has no prototype");
        return None;
    };
    world.entity_mut(item).insert(Networked);
    crate::storage::finish_item(world, item, 0);
    for &location in locations {
        let Some(mut inventory) = world.get::<Inventory>(mob).cloned() else {
            break;
        };
        let (slot, where_) = match location {
            Location::Eyes => (InventorySlot::Eyes, "на глазах"),
            Location::LeftPocket => (InventorySlot::LeftPocket, "в левом кармане"),
            Location::RightPocket => (InventorySlot::RightPocket, "в правом кармане"),
            Location::Hands => {
                let Some(hand) = inventory.hands.iter().position(Option::is_none) else {
                    continue;
                };
                inventory.hands[hand] = Some(item);
                world.entity_mut(item).insert(CarriedBy(mob));
                world.entity_mut(mob).insert(inventory);
                return Some("в руках");
            }
            Location::Backpack => {
                let Some(pack) = inventory.back else {
                    continue;
                };
                if crate::storage::can_insert(world, pack, item).is_err() {
                    continue;
                }
                crate::storage::put_inside(world, item, pack);
                return Some("в рюкзаке");
            }
        };
        // Pockets need a jumpsuit (tg `mob_can_equip`).
        if matches!(slot, InventorySlot::LeftPocket | InventorySlot::RightPocket)
            && inventory.uniform.is_none()
        {
            continue;
        }
        if !crate::inventory::can_equip(world, mob, &inventory, item, slot) {
            continue;
        }
        match slot {
            InventorySlot::Eyes => inventory.eyes = Some(item),
            InventorySlot::LeftPocket => inventory.pockets[0] = Some(item),
            _ => inventory.pockets[1] = Some(item),
        }
        world.entity_mut(item).insert(CarriedBy(mob));
        world.entity_mut(mob).insert(inventory);
        return Some(where_);
    }
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::inventory::put_down(world, item, pos);
    }
    Some("у ваших ног")
}

pub struct QuirksServerPlugin;

impl Plugin for QuirksServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            quirk_life
                .before(crate::health::HealthSystems)
                .run_if(any_with_component::<Quirks>),
        );
    }
}

/// `SSquirks`: the processing quirks of every holder, every second.
// Cost: 1 Hz, one pass over mobs with quirks (a few players), a few lookups each.
fn quirk_life(world: &mut World, mut clock: Local<f64>) {
    *clock += world.resource::<Time>().delta_secs_f64();
    let mut fires = 0;
    while *clock >= QUIRKS_WAIT_SECS {
        *clock -= QUIRKS_WAIT_SECS;
        fires += 1;
    }
    if fires == 0 {
        return;
    }
    let mut query = world.query::<(Entity, &Quirks)>();
    let holders: Vec<(Entity, Vec<Quirk>)> =
        query.iter(world).map(|(e, q)| (e, q.0.clone())).collect();
    let secs = QUIRKS_WAIT_SECS as f32;
    for (mob, quirks) in holders {
        for _ in 0..fires {
            for &quirk in &quirks {
                process(world, mob, quirk, secs);
            }
        }
    }
}

/// One quirk's `process` (and Bad Back's equip signals, followed here every fire).
fn process(world: &mut World, mob: Entity, quirk: Quirk, secs: f32) {
    let Some(&stat) = world.get::<Stat>(mob) else {
        return;
    };
    if quirk == Quirk::BadBack {
        let backpack = world
            .get::<Inventory>(mob)
            .and_then(|i| i.back)
            .and_then(|b| world.get::<DmItem>(b))
            .is_some_and(|dm| ss13_mapload::fidelity::is_under(&dm.path, BACKPACK));
        let has_pain = world
            .get::<Mood>(mob)
            .is_some_and(|m| m.get("back_pain").is_some());
        if backpack && !has_pain {
            mood::add_event(world, mob, "back_pain", &events::BACK_PAIN);
        } else if !backpack && has_pain {
            mood::clear_event(world, mob, "back_pain");
        }
        return;
    }
    // `maximum_process_stat = HARD_CRIT`.
    if stat >= Stat::HardCrit {
        return;
    }
    match quirk {
        Quirk::Jolly => {
            if spt_prob(world, MOOD_SWING_PERCENT, secs) {
                mood::add_event(world, mob, "jolly", &events::JOLLY);
            }
        }
        Quirk::Depression => {
            if spt_prob(world, MOOD_SWING_PERCENT, secs) {
                mood::add_event(world, mob, "depression", &events::DEPRESSION);
            }
        }
        Quirk::DrunkHealing => {
            let drunk = world
                .get::<crate::chem::MobReagents>(mob)
                .map_or(0.0, |r| r.drunk);
            let (brute, burn) = if drunk >= 61.0 {
                (0.8, 0.4)
            } else if drunk >= 41.0 {
                (0.4, 0.2)
            } else if drunk >= 6.0 {
                (0.1, 0.05)
            } else {
                return;
            };
            let Some(mut health) = world.get::<Health>(mob).cloned() else {
                return;
            };
            if health.brute <= 0.0 && health.burn <= 0.0 {
                return;
            }
            health.adjust(DamageType::Brute, -brute * secs);
            health.adjust(DamageType::Burn, -burn * secs);
            let mut new_stat = stat;
            update_stat(&health, &mut new_stat);
            let mut e = world.entity_mut(mob);
            e.insert(health);
            if new_stat != stat {
                e.insert(new_stat);
            }
        }
        Quirk::BrainTumor => {
            organs::adjust_organ_loss(world, mob, Slot::Brain, TUMOR_DAMAGE_PER_SECOND * secs);
        }
        _ => {}
    }
}

fn spt_prob(world: &mut World, percent: f32, secs: f32) -> bool {
    let mut rng = world.get_resource_or_init::<CombatRng>();
    crate::medical::wounds::spt_prob(&mut rng, percent, secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|n| (*n).to_owned()).collect()
    }

    #[test]
    fn every_quirk_has_one_definition_and_name() {
        for def in &QUIRKS {
            assert_eq!(def.quirk.def().path, def.path);
            assert_eq!(Quirk::by_name(def.name), Some(def.quirk));
        }
    }

    #[test]
    fn filter_follows_points_blacklist_and_limits() {
        // 2 free points: Friendly (2) alone is fine, Jolly (4) is not.
        let ok = names(&["Friendly"]);
        assert_eq!(filter_invalid(&ok), ok);
        assert_eq!(filter_invalid(&names(&["Jolly"])), Vec::<String>::new());
        // Negative quirks pay for positive ones.
        let paid = names(&["Jolly", "Mute"]);
        assert_eq!(filter_invalid(&paid), paid);
        // Blacklisted: the later one is dropped.
        assert_eq!(
            filter_invalid(&names(&["Blind", "Nearsighted"])),
            names(&["Blind"])
        );
        // Unknown names are dropped.
        assert_eq!(filter_invalid(&names(&["Nonsense"])), Vec::<String>::new());
        // Over budget: positive quirks go from the first until the balance holds.
        assert_eq!(
            filter_invalid(&names(&["Friendly", "Jolly", "Light Drinker"])),
            names(&["Jolly", "Light Drinker"])
        );
    }

    #[test]
    fn prefs_change_only_to_valid_sets() {
        let mut prefs = QuirkPrefs::default();
        assert_eq!(prefs.balance(), 2);
        assert!(!change_prefs(&mut prefs, "Jolly", true));
        assert!(change_prefs(&mut prefs, "Mute", true));
        assert!(change_prefs(&mut prefs, "Jolly", true));
        assert_eq!(prefs.balance(), 2);
        // Removing Mute would leave Jolly unpaid.
        assert!(!change_prefs(&mut prefs, "Mute", false));
        assert!(change_prefs(&mut prefs, "Jolly", false));
        assert!(!change_prefs(&mut prefs, "Jolly", false));
        assert_eq!(prefs.0, names(&["Mute"]));
    }
}
