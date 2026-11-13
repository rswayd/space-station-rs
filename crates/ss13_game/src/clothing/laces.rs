//! Shoelaces (server), after tg `/obj/item/clothing/shoes` (`code/modules/clothing/shoes/
//! _shoes.dm`: `adjust_laces`, `handle_tying`, `check_trip`), the strip menu's knot actions
//! (`human_stripping.dm`) and `/atom/movable/screen/alert/shoes`.
//!
//! - Shoes that are not slip-ons ([`Fastening`]) start tied. Used in hand they are untied or
//!   tied after their `lace_time` (5 s; boots and costumes their own).
//! - The wearer clicking their worn untied or knotted shoes (or the alert) works towards tied:
//!   knotted -> untied -> tied. Anyone else (lying on the floor next to the wearer) works
//!   towards knotted from the strip menu: tied -> untied -> knotted, a quarter faster for the
//!   clumsy; if the wearer moves in the meantime, they stamp on the prankster's hand (10 brute
//!   to the active arm, 40 stamina, 1 s paralysis, a scream).
//! - Every step of a standing wearer in gravity: knotted laces always trip (0.5 s paralysis, 1 s
//!   knockdown, the "tripped" moodlet and the knotted alert); untied ones per mille trip (1),
//!   lurch three tiles forward (2-5), fling what the hands hold (6-13) or stagger (14-25), and
//!   any of those shows the untied alert and moodlet.
// shortcut: the alert uses the base `template` state without the sneakers overlay; the flung
// items only drop or fly ahead (no hit on oneself, `accident`'s first two outcomes); no
// cerulean shoes-as-hats, no spell or smite knots, no examine of someone else's laces through
// the HUD; add with those features.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::chat::Span;
use ss13_shared::{Facing, TilePos};
use ss13_tgui::server::{UiActed, UiSystems};
use ss13_world::{Moved, WorldSystems};

use crate::breath::{AlertCategory, Alerts};
use crate::combat::CombatRng;
use crate::health::{BodyPosition, DamageType, Stat};
use crate::hud::AlertHudUi;
use crate::inventory::{CarriedBy, Inventory};
use crate::mood::MoodEvent;
use crate::security::name_of;
use crate::status_effects::Incap;
use crate::tools::{Actor, do_after, tell_mob};
use crate::zone::BodyZone;

pub const SHOES: &str = "/obj/item/clothing/shoes";
/// `lace_time`, deciseconds.
const LACE_TIME_DS: f32 = 50.0;
/// The `template` state of `/atom/movable/screen/alert` (see the shortcut above).
const ALERT_STATE: &str = "template";
/// `STAGGERED_SLOWDOWN_LENGTH`, `adjust_staggered_up_to(..., 10 SECONDS)`.
const STAGGER_SECS: f32 = 3.0;
const STAGGER_MAX: f32 = 10.0;

/// `/datum/mood_event/tripped`.
const TRIPPED: MoodEvent = MoodEvent {
    id: "tripped",
    description: "Не могу поверить, что я попался на самый старый прикол!",
    mood_change: -5.0,
    timeout: 120.0,
    hidden: false,
};
/// `/datum/mood_event/untied`.
const UNTIED: MoodEvent = MoodEvent {
    id: "untied",
    description: "Ненавижу, когда у меня развязываются шнурки!",
    mood_change: -3.0,
    timeout: 60.0,
    hidden: false,
};

/// `fastening_type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fastening {
    Laced,
    Velcro,
    Straps,
    SlipOn,
}

impl Fastening {
    /// `[fastening_type]`.
    fn noun(self) -> &'static str {
        match self {
            Self::Laced => "шнурки",
            Self::Velcro => "липучки",
            Self::Straps => "ремешки",
            Self::SlipOn => "застёжки",
        }
    }

    /// `untied_adjective()`.
    fn untied(self) -> &'static str {
        match self {
            Self::Laced => "развязаны",
            _ => "расстёгнуты",
        }
    }

    /// `fastening_verb()` ("tying"), `fasten_verb()` ("tie") and their "un" forms.
    fn verbs(self, undo: bool) -> (&'static str, &'static str) {
        match (self, undo) {
            (Self::Laced, false) => ("завязывать", "завязываете"),
            (Self::Laced, true) => ("развязывать", "развязываете"),
            (_, false) => ("застёгивать", "застёгиваете"),
            (_, true) => ("расстёгивать", "расстёгиваете"),
        }
    }
}

/// `tied`: `SHOES_UNTIED`, `SHOES_TIED`, `SHOES_KNOTTED`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tied {
    Untied,
    Tied,
    Knotted,
}

/// The lace state of a pair of shoes.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Laces {
    pub fastening: Fastening,
    pub tied: Tied,
    pub lace_time_ds: f32,
}

impl Laces {
    pub fn of_path(path: &str) -> Option<Self> {
        if !is_under(path, SHOES) {
            return None;
        }
        let under = |p: &str| is_under(path, p);
        let fastening = if under("/obj/item/clothing/shoes/cowboy/laced")
            || under("/obj/item/clothing/shoes/cowboy/white/laced")
            || under("/obj/item/clothing/shoes/cowboy/black/laced")
        {
            Fastening::Laced
        } else if under("/obj/item/clothing/shoes/sandal/velcro")
            || under("/obj/item/clothing/shoes/sandal/alt/velcro")
        {
            Fastening::Velcro
        } else if [
            "/obj/item/clothing/shoes/jackboots",
            "/obj/item/clothing/shoes/bronze",
            "/obj/item/clothing/shoes/cookflops",
            "/obj/item/clothing/shoes/sneakers/marisa",
            "/obj/item/clothing/shoes/mod",
            "/obj/item/clothing/shoes/cowboy",
            "/obj/item/clothing/shoes/galoshes",
            "/obj/item/clothing/shoes/sandal",
            "/obj/item/clothing/shoes/divine_archer",
        ]
        .iter()
        .any(|p| under(p))
        {
            Fastening::SlipOn
        } else if under("/obj/item/clothing/shoes/roman") {
            Fastening::Straps
        } else {
            Fastening::Laced
        };
        let secs = if under("/obj/item/clothing/shoes/clown_shoes") {
            20.0
        } else if under("/obj/item/clothing/shoes/combat")
            || under("/obj/item/clothing/shoes/pirate/armored")
        {
            12.0
        } else if under("/obj/item/clothing/shoes/cult") {
            10.0
        } else if under("/obj/item/clothing/shoes/griffin")
            || under("/obj/item/clothing/shoes/winterboots")
            || under("/obj/item/clothing/shoes/workboots")
            || under("/obj/item/clothing/shoes/russian")
        {
            8.0
        } else if under("/obj/item/clothing/shoes/sandal") {
            3.0
        } else {
            LACE_TIME_DS / 10.0
        };
        Some(Self {
            fastening,
            tied: Tied::Tied,
            lace_time_ds: secs * 10.0,
        })
    }
}

/// The timed lacing of [`crate::clothing::ClothingJob::Laces`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaceJob {
    /// `attack_self`: the shoes in the user's hand.
    InHand,
    /// `handle_tying` by the wearer.
    Own,
    /// `handle_tying` by someone else (the prank).
    Other,
}

/// On a human: the worn shoes the last inventory change saw (tg `equipped`/`dropped` of shoes).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LastShoes(pub Option<Entity>);

pub fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // Used in hand: tie or untie.
    app.add_click_handler(ClickPhase::AttackSelf, 3387, |w, c| {
        crate::clothing::laces::attack_self(w, c.client, c.mob, c.target)
    });
    app.add_observer(add_laces).add_systems(
        Update,
        (
            check_trip.after(WorldSystems::Movement),
            equip_alerts,
            alert_clicks.in_set(UiSystems::Handle),
        ),
    );
}

fn add_laces(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if let Some(laces) = items
        .get(add.entity)
        .ok()
        .and_then(|i| Laces::of_path(&i.path))
    {
        commands.entity(add.entity).try_insert_if_new(laces);
    }
}

/// The human whose `loc` the shoes are (tg `ishuman(loc)`): worn or held.
fn human_of(world: &World, shoes: Entity) -> Option<Entity> {
    let mob = world.get::<CarriedBy>(shoes)?.0;
    world
        .get::<crate::human::HumanLook>(mob)
        .is_some()
        .then_some(mob)
}

fn worn_by(world: &World, shoes: Entity, mob: Entity) -> bool {
    world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.shoes == Some(shoes))
}

/// `ALERT_SHOES_KNOT`: one alert, `/atom/movable/screen/alert/shoes/untied` or `knotted`.
fn set_alert(alerts: &mut Alerts, knotted: Option<bool>) {
    let keep = knotted.map(|k| {
        if k {
            AlertCategory::ShoesKnotted
        } else {
            AlertCategory::ShoesUntied
        }
    });
    for category in [AlertCategory::ShoesUntied, AlertCategory::ShoesKnotted] {
        if Some(category) != keep && alerts.get(category).is_some() {
            alerts.clear(category);
        }
    }
    if let Some(category) = keep {
        alerts.throw(category, ALERT_STATE, 0);
    }
}

fn throw_alert(world: &mut World, mob: Entity, knotted: bool) {
    if let Some(mut alerts) = world.get_mut::<Alerts>(mob) {
        set_alert(&mut alerts, Some(knotted));
    }
}

fn clear_alert(world: &mut World, mob: Entity) {
    let shown = world.get::<Alerts>(mob).is_some_and(|a| {
        a.get(AlertCategory::ShoesUntied).is_some() || a.get(AlertCategory::ShoesKnotted).is_some()
    });
    if shown && let Some(mut alerts) = world.get_mut::<Alerts>(mob) {
        set_alert(&mut alerts, None);
    }
}

/// tg `adjust_laces(state, user)`.
pub fn adjust_laces(world: &mut World, shoes: Entity, state: Tied, user: Option<Entity>) {
    let Some(mut laces) = world.get_mut::<Laces>(shoes) else {
        return;
    };
    if laces.fastening == Fastening::SlipOn {
        return;
    }
    laces.tied = state;
    let Some(guy) = human_of(world, shoes) else {
        return;
    };
    match state {
        Tied::Tied => clear_alert(world, guy),
        // If we're the ones unknotting our own laces, of course we know they're untied.
        Tied::Untied if user == Some(guy) => throw_alert(world, guy, false),
        _ => {}
    }
}

/// The lace state of the shoes `mob` wears, for the strip menu's actions (`None`: no laces).
pub fn worn_laces(world: &World, shoes: Entity) -> Option<Tied> {
    world
        .get::<Laces>(shoes)
        .filter(|l| l.fastening != Fastening::SlipOn)
        .map(|l| l.tied)
}

/// tg `attack_self`: the shoes in hand are tied or untied after `lace_time`.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, shoes: Entity) -> bool {
    let Some(laces) = world.get::<Laces>(shoes).copied() else {
        return false;
    };
    if laces.fastening == Fastening::SlipOn {
        return false;
    }
    let undo = laces.tied != Tied::Untied;
    let name = name_of(world, shoes);
    let actor = Actor {
        client,
        mob,
        item: Some(shoes),
    };
    let job = crate::construction::Job::Clothing(crate::clothing::ClothingJob::Laces {
        shoes,
        job: LaceJob::InHand,
    });
    // The shoes are in a hand (no tile of their own): the do_after is on the user.
    if do_after(world, actor, mob, laces.lace_time_ds, 0, job) {
        let (verb, _) = laces.fastening.verbs(undo);
        tell_mob(
            world,
            mob,
            Span::Notice,
            format!(
                "Вы начинаете {verb} {} на {name}...",
                laces.fastening.noun()
            ),
        );
    } else {
        tell_mob(world, mob, Span::Warning, format!("Вы уже заняты {name}!"));
    }
    true
}

/// tg `attack_hand` of the worn shoes: the wearer works on their untied or knotted laces.
pub fn attack_hand_worn(world: &mut World, mob: Entity, shoes: Entity) -> bool {
    let Some(laces) = world.get::<Laces>(shoes).copied() else {
        return false;
    };
    if laces.tied == Tied::Tied || !worn_by(world, shoes, mob) || !can_use(world, mob) {
        return false;
    }
    handle_tying(world, mob, shoes);
    true
}

/// `mobility_flags & MOBILITY_USE`.
fn can_use(world: &World, mob: Entity) -> bool {
    world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, mob).incapacitated
}

/// tg `handle_tying(user)`: the wearer ties, anyone else knots.
pub fn handle_tying(world: &mut World, user: Entity, shoes: Entity) {
    let Some(guy) = human_of(world, shoes) else {
        return;
    };
    let Some(laces) = world.get::<Laces>(shoes).copied() else {
        return;
    };
    if !can_use(world, user) {
        return;
    }
    let name = name_of(world, shoes);
    let noun = laces.fastening.noun();
    let near = match (world.get::<TilePos>(user), world.get::<TilePos>(guy)) {
        (Some(a), Some(b)) => a.chebyshev(*b).is_some_and(|d| d <= 1),
        _ => false,
    };
    if !near {
        tell_mob(
            world,
            user,
            Span::Warning,
            format!("Вы недостаточно близко, чтобы добраться до {noun} {name}!"),
        );
        return;
    }
    let Some(client) = crate::tools::client_of(world, user) else {
        return;
    };
    let actor = Actor {
        client,
        mob: user,
        item: world
            .get::<Inventory>(user)
            .and_then(Inventory::active_item),
    };
    let busy = |world: &mut World| {
        tell_mob(world, user, Span::Warning, format!("Вы уже заняты {name}!"));
    };
    if user == guy && laces.tied != Tied::Tied {
        let job = crate::construction::Job::Clothing(crate::clothing::ClothingJob::Laces {
            shoes,
            job: LaceJob::Own,
        });
        if !do_after(world, actor, guy, laces.lace_time_ds, 0, job) {
            busy(world);
            return;
        }
        let verb = if laces.tied == Tied::Knotted {
            "распутывать"
        } else {
            laces.fastening.verbs(false).0
        };
        let user_name = name_of(world, user);
        crate::medical::visible(
            world,
            user,
            &format!("Вы начинаете {verb} {noun} на своих {name}..."),
            &format!("{user_name} начинает {verb} {noun} на своих {name}."),
            Span::Notice,
        );
        return;
    }
    if world.get::<BodyPosition>(user).is_none_or(|b| !b.lying()) {
        tell_mob(
            world,
            user,
            Span::Warning,
            format!("Нужно быть на полу, чтобы добраться до {name}!"),
        );
        return;
    }
    let guy_name = name_of(world, guy);
    if laces.tied == Tied::Knotted {
        tell_mob(
            world,
            user,
            Span::Warning,
            format!(
                "{} на {name} {guy_name} уже безнадёжно запутаны!",
                capitalize(noun)
            ),
        );
        return;
    }
    let mut time = laces.lace_time_ds;
    // Based clowns trained their whole lives for this.
    if crate::genetics::is_clumsy(world, user) {
        time *= 0.75;
    }
    let job = crate::construction::Job::Clothing(crate::clothing::ClothingJob::Laces {
        shoes,
        job: LaceJob::Other,
    });
    if !do_after(world, actor, guy, time, 0, job) {
        busy(world);
        return;
    }
    let verb = if laces.tied == Tied::Tied {
        laces.fastening.verbs(true).0
    } else {
        "связывать"
    };
    tell_mob(
        world,
        user,
        Span::Notice,
        format!("Вы тихонько начинаете {verb} {noun} на {name} {guy_name}..."),
    );
}

fn capitalize(s: &str) -> String {
    ss13_shared::capitalize(s)
}

/// The end of a [`LaceJob`] (`do_after` succeeded; the shoes must still be where they were:
/// `still_shoed`).
pub fn complete(world: &mut World, user: Entity, shoes: Entity, job: LaceJob) {
    let Some(laces) = world.get::<Laces>(shoes).copied() else {
        return;
    };
    let Some(guy) = human_of(world, shoes) else {
        return;
    };
    let name = name_of(world, shoes);
    let noun = laces.fastening.noun();
    match job {
        LaceJob::InHand => {
            if guy != user {
                return;
            }
            let undo = laces.tied != Tied::Untied;
            let (_, done) = laces.fastening.verbs(undo);
            tell_mob(
                world,
                user,
                Span::Notice,
                format!("Вы {done} {noun} на {name}."),
            );
            let state = if undo { Tied::Untied } else { Tied::Tied };
            adjust_laces(world, shoes, state, Some(user));
        }
        LaceJob::Own => {
            if guy != user || laces.tied == Tied::Tied {
                return;
            }
            let done = if laces.tied == Tied::Knotted {
                "распутываете"
            } else {
                laces.fastening.verbs(false).1
            };
            tell_mob(
                world,
                user,
                Span::Notice,
                format!("Вы {done} {noun} на своих {name}."),
            );
            let state = if laces.tied == Tied::Untied {
                Tied::Tied
            } else {
                Tied::Untied
            };
            adjust_laces(world, shoes, state, Some(user));
        }
        LaceJob::Other => {
            if laces.tied == Tied::Knotted {
                return;
            }
            let guy_name = name_of(world, guy);
            let done = if laces.tied == Tied::Tied {
                laces.fastening.verbs(true).1
            } else {
                "связываете"
            };
            tell_mob(
                world,
                user,
                Span::Notice,
                format!("Вы {done} {noun} на {name} {guy_name}."),
            );
            let state = if laces.tied == Tied::Untied {
                Tied::Knotted
            } else {
                Tied::Untied
            };
            adjust_laces(world, shoes, state, Some(user));
        }
    }
}

/// tg `handle_tying`'s failed `do_after` of a prankster: the wearer stamps on their hand.
pub fn interrupted(world: &mut World, user: Entity, shoes: Entity, job: LaceJob) {
    if job != LaceJob::Other {
        return;
    }
    let (Some(laces), Some(guy)) = (world.get::<Laces>(shoes).copied(), human_of(world, shoes))
    else {
        return;
    };
    let (user_name, guy_name) = (name_of(world, user), name_of(world, guy));
    let noun = laces.fastening.noun();
    let doing = if laces.tied == Tied::Tied {
        laces.fastening.verbs(true).0
    } else {
        "связывать"
    };
    crate::medical::visible(
        world,
        user,
        &format!("Ай! {guy_name} наступает вам на руку!"),
        &format!("{guy_name} наступает на руку {user_name}, пытавшегося {doing} {noun}!"),
        Span::Danger,
    );
    tell_mob(
        world,
        guy,
        Span::BoldWarning,
        format!("Вы наступаете на руку {user_name}! Что за... Он пытался {doing} ваши {noun}!"),
    );
    if let Some(emote) = ss13_chat::emote::find("scream") {
        crate::mobs::defense::emote(world, user, emote.message);
    }
    let arm = match world.get::<Inventory>(user).map(|i| i.active) {
        Some(0) => BodyZone::LArm,
        _ => BodyZone::RArm,
    };
    crate::medical::hurt_limb(world, user, arm, DamageType::Brute, 10.0);
    crate::status_effects::adjust_stamina(world, user, 40.0);
    crate::status_effects::apply(world, user, Incap::Paralyze, 1.0);
}

/// What one step on untied laces does (`rand(1, 1000)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stumble {
    Fall,
    Lurch,
    Fling,
    Stagger,
    Nothing,
}

pub fn untied_step(roll: u32) -> Stumble {
    match roll {
        1 => Stumble::Fall,
        2..=5 => Stumble::Lurch,
        6..=13 => Stumble::Fling,
        14..=25 => Stumble::Stagger,
        _ => Stumble::Nothing,
    }
}

/// `COMSIG_SHOES_STEP_ACTION` -> `check_trip` on every step of a standing human in gravity.
// Cost: per tick, one lookup per moved thing.
fn check_trip(
    world: &mut World,
    mut cursor: Local<MessageCursor<Moved>>,
    mut seen: Local<Vec<Entity>>,
) {
    let moved: Vec<Entity> = cursor
        .read(world.resource::<Messages<Moved>>())
        .map(|m| m.entity)
        .collect();
    seen.clear();
    for mob in moved {
        // A diagonal move is one step (two `Moved`).
        if seen.contains(&mob) {
            continue;
        }
        seen.push(mob);
        let Some(shoes) = world.get::<Inventory>(mob).and_then(|i| i.shoes) else {
            continue;
        };
        let Some(laces) = world.get::<Laces>(shoes).copied() else {
            continue;
        };
        if laces.tied == Tied::Tied
            || world.get::<crate::human::HumanLook>(mob).is_none()
            || world.get::<BodyPosition>(mob).is_some_and(|b| b.lying())
        {
            continue;
        }
        let Some(&pos) = world.get::<TilePos>(mob) else {
            continue;
        };
        if !crate::multiz::has_gravity(world, pos) {
            continue;
        }
        trip(world, mob, laces, pos);
    }
}

fn fall(world: &mut World, mob: Entity) {
    crate::status_effects::apply(world, mob, Incap::Paralyze, 0.5);
    crate::status_effects::knockdown(world, mob, 1.0, 0.0);
    crate::mood::add_event(world, mob, "trip", &TRIPPED);
}

fn trip(world: &mut World, mob: Entity, laces: Laces, pos: TilePos) {
    let noun = laces.fastening.noun();
    let name = name_of(world, mob);
    if laces.tied == Tied::Knotted {
        fall(world, mob);
        crate::medical::visible(
            world,
            mob,
            &format!("Вы спотыкаетесь о связанные {noun} и падаете!"),
            &format!("{name} спотыкается о связанные {noun} и падает! Вот растяпа!"),
            Span::Danger,
        );
        throw_alert(world, mob, true);
        return;
    }
    let roll = world.get_resource_or_init::<CombatRng>().range(1, 1000);
    let adjective = laces.fastening.untied();
    match untied_step(roll) {
        Stumble::Nothing => return,
        Stumble::Fall => {
            fall(world, mob);
            crate::medical::visible(
                world,
                mob,
                &format!("Вы спотыкаетесь о свои {noun}, которые {adjective}, и падаете!"),
                &format!(
                    "{name} спотыкается о свои {noun}, которые {adjective}, и падает! Вот растяпа!"
                ),
                Span::Danger,
            );
        }
        Stumble::Lurch => {
            let dir = world
                .get::<Facing>(mob)
                .map_or(ss13_shared::Dir::South, |f| f.0);
            crate::throw::throw_at(world, mob, pos.step(dir), 3, 2.0);
            tell_mob(
                world,
                mob,
                Span::Danger,
                format!(
                    "Вы спотыкаетесь о свои {noun}, которые {adjective}, и вас бросает вперёд!"
                ),
            );
        }
        Stumble::Fling => {
            let flung = fling_held(world, mob, pos);
            let tail = if flung {
                ", роняя то, что держали"
            } else {
                ""
            };
            tell_mob(
                world,
                mob,
                Span::Danger,
                format!("Вы немного спотыкаетесь о свои {noun}{tail}!"),
            );
        }
        Stumble::Stagger => {
            tell_mob(
                world,
                mob,
                Span::Danger,
                format!("Вы немного спотыкаетесь о свои {noun}, которые {adjective}!"),
            );
            let dead = world.get::<Stat>(mob) == Some(&Stat::Dead);
            if let Some(mut e) = crate::status_effects::effects_mut(world, mob) {
                e.adjust_staggered_up_to(STAGGER_SECS, STAGGER_MAX, dead);
            }
        }
    }
    crate::mood::add_event(world, mob, "untied", &UNTIED);
    throw_alert(world, mob, false);
}

/// `accident` of each held item: dropped, then (31-60) thrown ahead or (61-90) left at the
/// feet. True when the hands held anything.
fn fling_held(world: &mut World, mob: Entity, pos: TilePos) -> bool {
    let held: Vec<Entity> = world
        .get::<Inventory>(mob)
        .map(|i| i.hands.iter().flatten().copied().collect())
        .unwrap_or_default();
    let dir = world
        .get::<Facing>(mob)
        .map_or(ss13_shared::Dir::South, |f| f.0);
    let clumsy = crate::genetics::is_clumsy(world, mob);
    for &item in &held {
        if crate::inventory::no_drop(world, item) {
            continue;
        }
        crate::inventory::put_down(world, item, pos);
        let mut rng = world.get_resource_or_init::<CombatRng>();
        let roll = rng.range(1, 100) as i32 - if clumsy { 40 } else { 0 };
        let range = world
            .get::<DmItem>(item)
            .map_or(7, |d| d.throw_range.max(2.0) as u32);
        let speed = world.get::<DmItem>(item).map_or(2.0, |d| d.throw_speed);
        if (31..=60).contains(&roll) {
            let flight = world.get_resource_or_init::<CombatRng>().range(2, range);
            let mut target = pos;
            for _ in 1..flight {
                target = target.step(dir);
            }
            crate::throw::throw_at(world, item, target, range, speed);
        }
    }
    !held.is_empty()
}

/// tg `equipped`/`dropped` of shoes: untied laces put on show the untied alert, the alert goes
/// with the shoes.
fn equip_alerts(
    mut mobs: Query<
        (
            Entity,
            &Inventory,
            Option<&mut LastShoes>,
            Option<&mut Alerts>,
        ),
        Changed<Inventory>,
    >,
    laces: Query<&Laces>,
    mut commands: Commands,
) {
    for (mob, inventory, last, alerts) in &mut mobs {
        let shoes = inventory.shoes;
        let before = last.as_deref().copied().unwrap_or_default().0;
        if before == shoes {
            continue;
        }
        match last {
            Some(mut last) => last.0 = shoes,
            None => {
                commands.entity(mob).insert(LastShoes(shoes));
            }
        }
        let Some(mut alerts) = alerts else {
            continue;
        };
        let untied = shoes
            .and_then(|s| laces.get(s).ok())
            .is_some_and(|l| l.fastening != Fastening::SlipOn && l.tied == Tied::Untied);
        let shown = alerts.get(AlertCategory::ShoesUntied).is_some()
            || alerts.get(AlertCategory::ShoesKnotted).is_some();
        if untied {
            set_alert(&mut alerts, Some(false));
        } else if before.is_some() && shown {
            set_alert(&mut alerts, None);
        }
    }
}

/// `/atom/movable/screen/alert/shoes/Click`: the wearer works on their laces.
fn alert_clicks(world: &mut World, mut acts: Local<MessageCursor<UiActed<AlertHudUi>>>) {
    let acts: Vec<(Entity, Entity, crate::breath::AlertClick)> = acts
        .read(world.resource::<Messages<UiActed<AlertHudUi>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, source, click) in acts {
        if actor != source
            || !matches!(
                crate::breath::clicked_alert(world, actor, &click),
                Some(AlertCategory::ShoesUntied | AlertCategory::ShoesKnotted)
            )
        {
            continue;
        }
        let now = world.resource::<Time>().elapsed_secs_f64();
        if world
            .get::<crate::combat::NextMove>(actor)
            .is_some_and(|n| now < n.0)
        {
            continue;
        }
        let Some(shoes) = world.get::<Inventory>(actor).and_then(|i| i.shoes) else {
            continue;
        };
        if world.get::<Laces>(shoes).is_none() || !can_use(world, actor) {
            continue;
        }
        handle_tying(world, actor, shoes);
    }
}

/// tg `examine` of shoes on a human.
pub fn examine_lines(world: &World, item: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(laces) = world.get::<Laces>(item) else {
        return;
    };
    if human_of(world, item).is_none() {
        return;
    }
    let noun = ss13_shared::capitalize(laces.fastening.noun());
    match laces.tied {
        Tied::Untied => lines.push((Span::Plain, format!("{noun} {}.", laces.fastening.untied()))),
        Tied::Knotted => lines.push((Span::Plain, format!("{noun} связаны между собой."))),
        Tied::Tied => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fastening_and_lace_time_follow_the_type() {
        let sneakers = Laces::of_path("/obj/item/clothing/shoes/sneakers/black").unwrap();
        assert_eq!(
            (sneakers.fastening, sneakers.tied, sneakers.lace_time_ds),
            (Fastening::Laced, Tied::Tied, 50.0)
        );
        let jackboots = Laces::of_path("/obj/item/clothing/shoes/jackboots/sec").unwrap();
        assert_eq!(jackboots.fastening, Fastening::SlipOn);
        let laced = Laces::of_path("/obj/item/clothing/shoes/cowboy/laced").unwrap();
        assert_eq!(laced.fastening, Fastening::Laced);
        assert_eq!(
            Laces::of_path("/obj/item/clothing/shoes/cowboy/white")
                .unwrap()
                .fastening,
            Fastening::SlipOn
        );
        let velcro = Laces::of_path("/obj/item/clothing/shoes/sandal/velcro").unwrap();
        assert_eq!(
            (velcro.fastening, velcro.lace_time_ds),
            (Fastening::Velcro, 30.0)
        );
        assert_eq!(
            Laces::of_path("/obj/item/clothing/shoes/clown_shoes")
                .unwrap()
                .lace_time_ds,
            200.0
        );
        assert!(Laces::of_path("/obj/item/clothing/gloves/color/black").is_none());
    }

    #[test]
    fn untied_steps_follow_the_per_mille_table() {
        assert_eq!(untied_step(1), Stumble::Fall);
        assert_eq!(untied_step(5), Stumble::Lurch);
        assert_eq!(untied_step(13), Stumble::Fling);
        assert_eq!(untied_step(25), Stumble::Stagger);
        assert_eq!(untied_step(26), Stumble::Nothing);
        assert_eq!(untied_step(1000), Stumble::Nothing);
    }
}
