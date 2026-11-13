//! Hand gestures (server), after tg `/obj/item/hand_item` (`code/game/objects/items/hand_items.dm`)
//! and the emotes that make them (`living/carbon/emote.dm`, `living/emote.dm` `kiss`).
//!
//! - `*circle`, `*slap`, `*shoesteal`, `*noogie`, `*hand`, `*kiss`
//!   ([`ss13_chat::emote::HAND_GESTURES`]) put the gesture in a free hand (the active one
//!   first). A gesture is the hand itself (`DROPDEL`): out of the hand it is gone; offered, it
//!   never changes hands (`crate::buffs::take` asks [`on_offer_taken`]).
//! - The slapper slaps a mob: by the selected zone a slap in the face (a facepalm on oneself,
//!   waking an unconscious mob: −7.5 s sleep, −5 s unconsciousness), on the wrist or plain, with
//!   `slap.ogg`; one who offers the slapper's user a kiss is smacked to the ground instead (2.5 s
//!   paralysis, 7 s confusion, 40 stamina). On a table a click slaps it (three times, then the
//!   hand is spent), a right click slams the fist down (only a fresh hand) and spends it.
//!   Offered, it is a high five (`high_fiver`): to everyone around, the alert's click high-fives
//!   (two hand gestures held: a high ten) with moodlets and `slap.ogg`, a taker without a free
//!   hand only gets a sad moodlet; a slapper put away meanwhile makes the taker "too slow": 0.5 s
//!   later they fall for 1 s, the giver gloats, the rube's moodlet grows with the onlookers.
//! - The circled hand: a mob next to its owner who examines the owner falls for it — 0.4 s, then
//!   0.6 s later the owner bops them (15 stamina, `hit_punch.ogg`) unless they moved away or the
//!   owner cannot act. The owner examining themselves bops themselves 1 s later (100 stamina,
//!   1 s knockdown). The hand is spent either way.
//! - The stealing hand: lying down next to a human with shoes, the user takes them off in the
//!   shoes' strip delay and holds them.
//! - The noogie: on a carbon with a head held in an aggressive grab, 1.5 s, then a moodlet
//!   (harsh on a moth's antennae or from a hulk) and rounds of 1-5 brute to the head (+3-7 each
//!   for antennae and hulk, a scream from 5) costing the user `round + 5` stamina, each round
//!   `1 s + 2 ds × round` long, until the grab, the head or the user's stamina (80) gives out.
//! - The hand: clicked on a carbon (or offered) it is held out: to a lying carbon next to the
//!   user a help up (the taker is shaken up and stands; moodlets for compassionate and callous
//!   ones), else everyone around may take it and be pulled along.
//! - The kiss ([`Kiss`]: garlic breath, the chef's kiss chip, kiss of death and syndie lipstick
//!   pick it) is blown at what is clicked, at any range, as a projectile; offered to everyone
//!   around (a cheek kiss unless aimed at the mouth) it is given point blank. A harmless kiss
//!   on a mob fakes its hit: `kiss.ogg`, the kiss moodlet, a fluster roll of the socially
//!   anxious (confusion, stutter or a stun); the french kiss leaves 1 u of garlic. The kiss of
//!   death hits for 35 brute and breaks the heart, the syndie kiss burns like a laser; a chef's
//!   kiss on adjacent handmade food adds love by its nutriment.
// shortcut: no attack animation, tail unwag, hulk circle-game variant, "gottem" or "too slow"
// achievements, memories, screen shake or the ring in the ears of the kiss slap, no fake-out
// warning on examining a high-fiver. The helping hand ignores buckles; the anxious "Uhhh..." line
// is not said (no say from code). The ink kiss is not made (no ink spit action), and the chef's
// kiss accepts any handmade food (`TRAIT_HANDMADE` has no maker here). Antennae are the moth's
// (no external organs).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_chat::server::Emoted;
use ss13_mapload::ProjectileData;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::protocol::InventorySlot;
use ss13_shared::{Facing, TilePos};

use super::DeviceJob;
use crate::buffs::{OfferKind, Offering};
use crate::click::{ExamineClick, RangedClick};
use crate::combat::CombatRng;
use crate::health::{
    BodyPosition, COMBAT_MESSAGE_RANGE, DEFAULT_MESSAGE_RANGE, DamageType, Health, Stat,
    ZoneSelected,
};
use crate::inventory::{CarriedBy, Inventory};
use crate::medical::wounds::Limbs;
use crate::mood::{ActiveMood, Mood, MoodEvent};
use crate::projectile::Projectile;
use crate::security::{name_of, show};
use crate::sound::{PlaySound, playsound};
use crate::status_effects::{Incap, StatusEffects, Timed};
use crate::tools::{Actor, do_after, tell};
use crate::zone::{BodyZone, TargetZone};

pub const CIRCLE: &str = "/obj/item/hand_item/circlegame";
pub const SLAPPER: &str = "/obj/item/hand_item/slapper";
pub const STEALER: &str = "/obj/item/hand_item/stealer";
pub const NOOGIE: &str = "/obj/item/hand_item/noogie";
pub const HAND: &str = "/obj/item/hand_item/hand";
const SLAP_SOUND: &str = "sound/items/weapons/slap.ogg";
const TABLE_SOUND: &str = "sound/effects/tableslam.ogg";
const BOP_SOUND: &str = "sound/effects/hit_punch.ogg";
const KISS_SOUND: &str = "sound/effects/emotes/kiss.ogg";
const THUDSWOOSH: &str = "sound/items/weapons/thudswoosh.ogg";
const GARLIC: &str = "/datum/reagent/consumable/garlic";
const LOVE: &str = "/datum/reagent/love";
const NUTRIMENT: &str = "/datum/reagent/consumable/nutriment";
const FRENCH_BERET: &str = "/obj/item/clothing/head/beret/frenchberet";
/// `table_smacks_left`.
const TABLE_SMACKS: u8 = 3;
/// The noogie gives out past this much of the user's `staminaloss`.
const NOOGIE_MAX_STAMINA: f32 = 80.0;
/// `too_slow_p1` -> `too_slow_p2`, seconds.
const TOO_SLOW_SECS: f64 = 0.5;

const fn mood(id: &'static str, description: &'static str, change: f32, timeout: f32) -> MoodEvent {
    MoodEvent {
        id,
        description,
        mood_change: change,
        timeout,
        hidden: false,
    }
}

// BandaStation's `/datum/mood_event/*` texts.
const NOOGIE_MOOD: MoodEvent = mood(
    "noogie",
    "Ай! Как будто вернулся в космошколу...",
    -2.0,
    60.0,
);
const NOOGIE_HARSH: MoodEvent = mood(
    "noogie_harsh",
    "АЙ!! Это было даже хуже обычной терки!",
    -4.0,
    60.0,
);
const HIGH_FIVE: MoodEvent = mood("high_five", "Я люблю получать пятюни!", 2.0, 45.0);
const HIGH_TEN: MoodEvent = mood("high_ten", "ВЕЛИКОЛЕПНО! ДВОЙНАЯ ПЯТЮНЯ!", 3.0, 45.0);
const HIGH_FIVE_FULL_HAND: MoodEvent = mood(
    "high_five_full_hand",
    "О боже, я даже не умею правильно давать пятюню...",
    -1.0,
    45.0,
);
/// Times the onlookers (`too_slow/add_effects`).
const TOO_SLOW: MoodEvent = mood("too_slow", "НЕТ! КАК Я МОГ... НЕ УСПЕТЬ???", -2.0, 120.0);
const DOWN_LOW: MoodEvent = mood(
    "down_low",
    "ХА! Вот глупыш, у них не было и шанса...",
    4.0,
    90.0,
);
/// Its change and text come from the personality (`helped_up/add_effects`).
const HELPED_UP: MoodEvent = mood("helped_up", "Мне понравилось помогать им!", 0.0, 45.0);
/// Its text names the kisser (`kiss/add_effects`).
const KISS_MOOD: MoodEvent = mood(
    "kiss",
    "Воздушный поцелуй от кого-то, я настоящая находка!",
    1.5,
    120.0,
);

/// On a gesture in a hand: what it is and its uses.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct HandItem {
    pub kind: Gesture,
    /// The slapper's `table_smacks_left`.
    pub smacks_left: u8,
    /// The kiss's `cheek_kiss`: offered aimed anywhere but the mouth.
    pub cheek: bool,
    /// The circled hand's suckers: (sucker, due time, stage).
    pending: Vec<(Entity, f64, CircleStage)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    Circle,
    Slapper,
    Stealer,
    Noogie,
    Hand,
    Kisser(Kiss),
}

/// `/obj/item/hand_item/kisser` and its subtypes, with their `kiss_type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kiss {
    Plain,
    /// `TRAIT_KISS_OF_DEATH`.
    Death,
    /// `TRAIT_SYNDIE_KISS`.
    Syndie,
    /// `TRAIT_GARLIC_BREATH`.
    French,
    /// `TRAIT_CHEF_KISS`.
    Chef,
}

impl Kiss {
    fn item_path(self) -> &'static str {
        match self {
            Self::Plain => "/obj/item/hand_item/kisser",
            Self::Death => "/obj/item/hand_item/kisser/death",
            Self::Syndie => "/obj/item/hand_item/kisser/syndie",
            Self::French => "/obj/item/hand_item/kisser/french",
            Self::Chef => "/obj/item/hand_item/kisser/chef",
        }
    }

    /// The projectile's name (BandaStation's `ru_names`).
    fn name(self) -> &'static str {
        match self {
            Self::Plain => "поцелуй",
            Self::Death => "поцелуй смерти",
            Self::Syndie => "поцелуй синди",
            Self::French => "французский поцелуй (с привкусом чеснока?)",
            Self::Chef => "поцелуй повара",
        }
    }

    /// `/obj/projectile/kiss` and its subtypes.
    pub fn projectile(self) -> ProjectileData {
        let base = ProjectileData {
            path: "/obj/projectile/kiss".into(),
            damage: 0.0,
            damage_type: "brute".into(),
            armor_flag: "bullet".into(),
            armour_penetration: 100.0,
            speed: 0.66,
            range: 50.0,
            pass_flags: crate::projectile::PASSTABLE
                | crate::projectile::PASSGLASS
                | crate::projectile::PASSGRILLE,
            accurate_range: 100.0,
            accuracy_falloff: 7.0,
            ricochet_decay_chance: 0.7,
            ricochet_decay_damage: 0.7,
            ricochet_incidence_leeway: 40.0,
            wound_bonus: crate::medical::wounds::CANT_WOUND,
            ..default()
        };
        match self {
            Self::Plain => base,
            Self::Death => ProjectileData {
                path: "/obj/projectile/kiss/death".into(),
                damage: 35.0,
                wound_bonus: 0.0,
                // `SHARP_POINTY`.
                sharpness: 2,
                ..base
            },
            // "Based on energy gun characteristics".
            Self::Syndie => ProjectileData {
                path: "/obj/projectile/kiss/syndie".into(),
                damage: 25.0,
                damage_type: "burn".into(),
                armor_flag: "laser".into(),
                armour_penetration: 0.0,
                wound_bonus: -20.0,
                exposed_wound_bonus: 40.0,
                ..base
            },
            Self::French => ProjectileData {
                path: "/obj/projectile/kiss/french".into(),
                ..base
            },
            Self::Chef => ProjectileData {
                path: "/obj/projectile/kiss/chef".into(),
                ..base
            },
        }
    }
}

/// The `lipstick_trait` of painted lips (`crate::service::barber`); read only while
/// [`crate::human::HumanLook::lip_style`] is set.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LipstickKiss(pub Kiss);

/// On a blown kiss in flight (`/obj/projectile/kiss`): what it is and whether it was given
/// point blank (`suppressed`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlownKiss {
    pub kind: Kiss,
    pub direct: bool,
}

/// On a high-fiver whose slapper is gone: the rubes reaching for it, and when the hand is
/// pulled away (`too_slowing_this_guy`, `too_slow_p2`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
struct TooSlow(Vec<(Entity, f64)>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CircleStage {
    /// `waitASecond`.
    Noticed,
    /// `selfGottem`.
    SelfBop,
    /// `GOTTEM`.
    Bop,
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // The slapper on a mob or a table, the stealing hand on a human, the noogie, the hand on a
    // carbon, a kiss blown at what is next to the user.
    app.add_click_handler(ClickPhase::HeldOnTarget, 233, |w, c| {
        c.held.is_some_and(|h| {
            crate::devices::hand_items::held_on_target(
                w,
                c.client,
                c.mob,
                h,
                c.target,
                c.pos,
                c.modifiers.right,
            )
        })
    });
    app.add_message::<Emoted>()
        .add_message::<ExamineClick>()
        .add_message::<RangedClick>()
        .add_systems(
            Update,
            (
                make_gestures,
                examined,
                circle_timers,
                ranged_kisses,
                too_slow_timers,
                drop_del,
            )
                .chain(),
        );
}

/// Gestures never attack (`crate::combat`): their `attack` is their own.
pub fn interacts_with_mobs(path: &str) -> bool {
    ss13_mapload::fidelity::is_under(path, "/obj/item/hand_item")
}

fn gesture_of(world: &World, item: Entity) -> Option<Gesture> {
    world.get::<HandItem>(item).map(|h| h.kind)
}

fn client_of(world: &World, mob: Entity) -> Option<Entity> {
    world.get::<crate::server::PlayerMob>(mob).map(|p| p.client)
}

fn tell_to(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(c) = client_of(world, mob) {
        tell(world, c, span, text);
    }
}

/// The gesture's emote: it goes into a free hand, or the emote says why not.
fn make_gestures(world: &mut World, mut cursor: Local<MessageCursor<Emoted>>) {
    let emotes: Vec<(Entity, &'static str)> = cursor
        .read(world.resource::<Messages<Emoted>>())
        .filter(|e| ss13_chat::emote::HAND_GESTURES.contains(&e.key))
        .map(|e| (e.speaker, e.key))
        .collect();
    for (mob, key) in emotes {
        let (kind, path) = match key {
            "circle" => (Gesture::Circle, CIRCLE),
            "slap" => (Gesture::Slapper, SLAPPER),
            "noogie" => (Gesture::Noogie, NOOGIE),
            "hand" => (Gesture::Hand, HAND),
            "kiss" => {
                let kiss = kiss_of(world, mob);
                (Gesture::Kisser(kiss), kiss.item_path())
            }
            _ => (Gesture::Stealer, STEALER),
        };
        let given = give(world, mob, kind, path);
        let (ready, unable) = match kind {
            Gesture::Stealer => {
                let text = if given {
                    "готовимся красть обувь..."
                } else {
                    "не украсть обувь!"
                };
                crate::balloon::balloon_alert(world, mob, mob, text);
                continue;
            }
            Gesture::Circle => (
                "Вы делаете кольцо из пальцев.",
                "У вас нет свободной руки, чтобы сделать кольцо.",
            ),
            Gesture::Slapper => (
                "Вы заносите руку для пощёчины.",
                "Вы не в состоянии дать пощёчину.",
            ),
            Gesture::Noogie => (
                "Вы готовите руку для тёрки.",
                "Вы не в состоянии устроить тёрку.",
            ),
            Gesture::Hand => (
                "Вы готовите руку.",
                "Вы не можете воспользоваться рукой в вашем состоянии.",
            ),
            Gesture::Kisser(_) => (
                "Вы готовите руку для воздушного поцелуя.",
                "Вы не в состоянии послать воздушный поцелуй.",
            ),
        };
        if given {
            tell_to(world, mob, Span::Notice, ready);
        } else {
            tell_to(world, mob, Span::Warning, unable);
        }
    }
}

/// `kiss/run_emote`'s `kiss_type`: the last trait that applies wins.
fn kiss_of(world: &World, mob: Entity) -> Kiss {
    let mut kiss = Kiss::Plain;
    if garlic_breath(world, mob) {
        kiss = Kiss::French;
    }
    if crate::skillchips::has_trait(world, mob, "chefs_kiss") {
        kiss = Kiss::Chef;
    }
    let painted = world
        .get::<crate::human::HumanLook>(mob)
        .is_some_and(|l| l.lip_style.is_some());
    if painted && let Some(&LipstickKiss(lips)) = world.get::<LipstickKiss>(mob) {
        kiss = lips;
    }
    kiss
}

/// `TRAIT_GARLIC_BREATH`: garlic in the blood (`added_traits`) or a french beret worn.
fn garlic_breath(world: &World, mob: Entity) -> bool {
    let beret = world
        .get::<Inventory>(mob)
        .and_then(|i| i.head)
        .and_then(|h| crate::construction::path_of(world, h))
        .is_some_and(|p| ss13_mapload::fidelity::is_under(&p, FRENCH_BERET));
    beret || reagent_amount(world, mob, GARLIC) > 0.0
}

/// The amount of the reagent `path` in a mob's blood.
fn reagent_amount(world: &World, mob: Entity, path: &str) -> f32 {
    let Some(chem) = world.get_resource::<crate::chem::Chem>() else {
        return 0.0;
    };
    let Some(id) = chem.0.id(path) else {
        return 0.0;
    };
    world
        .get::<crate::chem::MobReagents>(mob)
        .map_or(0.0, |m| m.blood.amount(id))
}

/// `new(user)` + `put_in_hands`: the active hand first, then the other.
fn give(world: &mut World, mob: Entity, kind: Gesture, path: &str) -> bool {
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return false;
    };
    let Some(hand) = [inventory.active, 1 - inventory.active]
        .into_iter()
        .find(|&h| {
            inventory.hands[h].is_none() && crate::medical::bodyparts::has_hand(world, mob, h)
        })
    else {
        return false;
    };
    let Some(item) = crate::construction::spawn_detached(world, path) else {
        return false;
    };
    world.entity_mut(item).remove::<TilePos>().insert((
        CarriedBy(mob),
        HandItem {
            kind,
            smacks_left: TABLE_SMACKS,
            cheek: false,
            pending: Vec::new(),
        },
    ));
    if let Some(mut inv) = world.get_mut::<Inventory>(mob) {
        inv.hands[hand] = Some(item);
    }
    crate::inventory::refresh_look(world, mob);
    true
}

/// `qdel(src)` of a gesture: out of the hand and gone.
fn spend(world: &mut World, item: Entity) {
    let holder = world.get::<CarriedBy>(item).map(|c| c.0);
    if let Some(mob) = holder
        && let Some(mut inv) = world.get_mut::<Inventory>(mob)
    {
        for hand in inv.hands.iter_mut().filter(|h| **h == Some(item)) {
            *hand = None;
        }
    }
    if world.get_entity(item).is_ok() {
        world.despawn(item);
    }
    if let Some(mob) = holder {
        crate::inventory::refresh_look(world, mob);
    }
}

/// `DROPDEL`: a gesture that is not in its holder's hands any more is gone.
fn drop_del(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, Option<&CarriedBy>), With<HandItem>>();
    let items: Vec<(Entity, Option<Entity>)> =
        q.iter(world).map(|(e, c)| (e, c.map(|c| c.0))).collect();
    for (item, holder) in items {
        let in_hand = holder.is_some_and(|mob| {
            world
                .get::<Inventory>(mob)
                .is_some_and(|i| i.hands.contains(&Some(item)))
        });
        if !in_hand {
            spend(world, item);
        }
    }
}

fn sound(world: &mut World, at: Entity, file: &str, volume: f32) {
    if let Some(pos) = crate::explosions::turf_of(world, at) {
        playsound(
            world,
            PlaySound::new(pos, file, volume, true).extrarange(-1.0),
        );
    }
}

/// The gesture's `pre_attack`/`attack` on what was clicked. True when it was one of ours.
pub fn held_on_target(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    pos: TilePos,
    right: bool,
) -> bool {
    match gesture_of(world, held) {
        Some(Gesture::Noogie) => {
            noogie(world, client, mob, held, target);
            true
        }
        Some(Gesture::Hand) => {
            hand_attack(world, mob, target);
            true
        }
        // `kisser/interact_with_atom` is its ranged one; a right click does nothing.
        Some(Gesture::Kisser(kind)) => {
            if !right {
                blow_kiss(world, mob, held, kind, target, pos);
            }
            true
        }
        Some(Gesture::Slapper) if crate::tables::is_table(world, target) => {
            if right {
                slam_table(world, mob, held, target);
            } else {
                slap_table(world, mob, held, target);
            }
            true
        }
        Some(Gesture::Slapper) if world.get::<Health>(target).is_some() => {
            slap(world, mob, target);
            true
        }
        Some(Gesture::Stealer) => {
            steal(world, client, mob, held, target);
            true
        }
        // The circled hand bops nothing by hand (`force = 0`).
        Some(_) => true,
        None => false,
    }
}

/// `slapper/attack`.
fn slap(world: &mut World, user: Entity, slapped: Entity) {
    // A kiss offered to the user is answered with a smack to the ground.
    let kissing = world.get::<Offering>(slapped).is_some_and(|o| {
        o.takers.contains(&user) && matches!(gesture_of(world, o.item), Some(Gesture::Kisser(_)))
    });
    if kissing {
        kiss_smack(world, user, slapped);
        return;
    }
    let zone = world
        .get::<ZoneSelected>(user)
        .map_or(TargetZone::Chest, |z| z.0);
    let them = crate::security::name_of(world, slapped);
    let me = crate::security::name_of(world, user);
    let (own, others, span) = match zone {
        TargetZone::Head | TargetZone::Mouth if user == slapped => (
            "Вы бьёте себя ладонью по лицу.".to_owned(),
            format!("{me} бьёт себя ладонью по лицу!"),
            Span::Notice,
        ),
        TargetZone::Head | TargetZone::Mouth
            if crate::status_effects::knocked_out(world, slapped) =>
        {
            crate::status_effects::adjust(world, slapped, Incap::Sleeping, -7.5);
            crate::status_effects::adjust(world, slapped, Incap::Unconscious, -5.0);
            (
                format!("Вы бьёте {them} по лицу, пытаясь разбудить!"),
                format!("{me} бьёт {them} по лицу, пытаясь разбудить!"),
                Span::Notice,
            )
        }
        TargetZone::Head | TargetZone::Mouth => (
            format!("Вы даёте {them} пощёчину!"),
            format!("{me} даёт {them} пощёчину!"),
            Span::Danger,
        ),
        TargetZone::LArm | TargetZone::RArm => (
            format!("Вы шлёпаете {them} по запястью!"),
            format!("{me} шлёпает {them} по запястью!"),
            Span::Danger,
        ),
        _ => (
            format!("Вы шлёпаете {them}!"),
            format!("{me} шлёпает {them}!"),
            Span::Danger,
        ),
    };
    crate::medical::visible(world, user, &own, &others, span);
    sound(world, slapped, SLAP_SOUND, 50.0);
}

/// `slapper/attack` on a mob offering the user a kiss.
fn kiss_smack(world: &mut World, user: Entity, slapped: Entity) {
    let (me, them) = (name_of(world, user), name_of(world, slapped));
    if let Some(&pos) = world.get::<TilePos>(user) {
        show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            (
                Span::Danger,
                format!(
                    "{me} с презрением отвергает заигрывания {them}, замахивается и со всей силы \
                     сбивает {them} с ног!"
                ),
            ),
            &[
                (
                    user,
                    Span::Notice,
                    format!(
                        "Какая наглость! Вы замахиваетесь и бьёте {them} так сильно, что сбиваете \
                         с ног!"
                    ),
                ),
                (
                    slapped,
                    Span::BoldWarning,
                    format!(
                        "Вы видите, как {me} презрительно фыркает и отводит руку, — и вдруг вы \
                         уже на полу, а в ушах стоит невыносимый звон!"
                    ),
                ),
            ],
        );
    }
    sound(world, slapped, SLAP_SOUND, 120.0);
    crate::status_effects::apply(world, slapped, Incap::Paralyze, 2.5);
    crate::status_effects::adjust_timed(world, slapped, Timed::Confusion, 7.0, None);
    crate::status_effects::adjust_stamina(world, slapped, 40.0);
}

/// `slap_table`: three slaps, then the hand is spent.
fn slap_table(world: &mut World, user: Entity, hand: Entity, table: Entity) {
    sound(world, table, TABLE_SOUND, 40.0);
    let me = crate::security::name_of(world, user);
    let name = crate::security::name_of(world, table);
    crate::medical::visible(
        world,
        user,
        &format!("Вы хлопаете ладонью по {name}."),
        &format!("{me} хлопает ладонью по {name}."),
        Span::Notice,
    );
    let left = world.get_mut::<HandItem>(hand).map(|mut h| {
        h.smacks_left = h.smacks_left.saturating_sub(1);
        h.smacks_left
    });
    if left == Some(0) {
        spend(world, hand);
    }
}

/// `slam_table`: a fresh hand slams the fist down and is spent; a used one only slaps.
fn slam_table(world: &mut World, user: Entity, hand: Entity, table: Entity) {
    if world
        .get::<HandItem>(hand)
        .is_some_and(|h| h.smacks_left < TABLE_SMACKS)
    {
        slap_table(world, user, hand, table);
        return;
    }
    sound(world, table, TABLE_SOUND, 110.0);
    let me = crate::security::name_of(world, user);
    let name = crate::security::name_of(world, table);
    crate::medical::visible(
        world,
        user,
        &format!("Вы с размаху бьёте кулаком по {name}!"),
        &format!("{me} с размаху бьёт кулаком по {name}!"),
        Span::BoldWarning,
    );
    spend(world, hand);
}

/// `stealer/attack`: lying next to a human with shoes, the strip starts.
fn steal(world: &mut World, client: Entity, user: Entity, hand: Entity, target: Entity) {
    if world.get::<crate::human::HumanLook>(target).is_none() {
        return;
    }
    if target == user {
        tell(
            world,
            client,
            Span::Notice,
            "Зачем пытаться украсть собственную обувь?",
        );
        return;
    }
    let Some(shoes) = world.get::<Inventory>(target).and_then(|i| i.shoes) else {
        return;
    };
    if !world.get::<BodyPosition>(user).is_some_and(|b| b.lying()) {
        return;
    }
    let actor = Actor {
        client,
        mob: user,
        item: Some(hand),
    };
    if !do_after(
        world,
        actor,
        target,
        crate::strip::STRIP_DELAY_DS,
        0,
        crate::construction::Job::Device(DeviceJob::StealShoes(shoes)),
    ) {
        return;
    }
    let (me, them) = (
        crate::security::name_of(world, user),
        crate::security::name_of(world, target),
    );
    let shoe_name = crate::security::name_of(world, shoes);
    crate::medical::visible(
        world,
        user,
        &format!("Вы начинаете красть {shoe_name} у {them}..."),
        &format!("{me} начинает красть {shoe_name} у {them}!"),
        Span::Warning,
    );
    if let Some(c) = world
        .get::<crate::server::PlayerMob>(target)
        .map(|p| p.client)
    {
        tell(
            world,
            c,
            Span::Danger,
            format!("{me} начинает красть вашу обувь!"),
        );
    }
}

/// The end of the theft: the same shoes still worn, the user still next to the target; they
/// drop and go into a free hand of the user.
pub fn complete_steal(world: &mut World, user: Entity, target: Entity, shoes: Entity) {
    if world.get::<Inventory>(target).and_then(|i| i.shoes) != Some(shoes) {
        return;
    }
    let (Some(&pos), Some(&user_pos)) = (world.get::<TilePos>(target), world.get::<TilePos>(user))
    else {
        return;
    };
    if !user_pos.chebyshev(pos).is_some_and(|d| d <= 1) {
        return;
    }
    crate::inventory::unequip(world, target, pos, InventorySlot::Shoes);
    world.entity_mut(shoes).remove::<CarriedBy>().insert(pos);
    crate::inventory::refresh_look(world, target);
    if let Some(inventory) = world.get::<Inventory>(user).cloned()
        && let Some(hand) = [inventory.active, 1 - inventory.active]
            .into_iter()
            .find(|&h| inventory.hands[h].is_none())
    {
        crate::inventory::pick_up(world, user, shoes, hand);
        crate::inventory::refresh_look(world, user);
    }
    let (me, them) = (
        crate::security::name_of(world, user),
        crate::security::name_of(world, target),
    );
    let shoe_name = crate::security::name_of(world, shoes);
    crate::medical::visible(
        world,
        user,
        &format!("Вы украли {shoe_name} у {them}!"),
        &format!("{me} украл {shoe_name} у {them}!"),
        Span::Warning,
    );
    if let Some(c) = world
        .get::<crate::server::PlayerMob>(target)
        .map(|p| p.client)
    {
        tell(world, c, Span::Danger, format!("{me} украл вашу обувь!"));
    }
}

/// The circled hand of `owner`, if one is in its hands.
fn circle_of(world: &World, owner: Entity) -> Option<Entity> {
    world
        .get::<Inventory>(owner)?
        .hands
        .iter()
        .flatten()
        .copied()
        .find(|&h| gesture_of(world, h) == Some(Gesture::Circle))
}

fn adjacent(world: &World, a: Entity, b: Entity) -> bool {
    match (world.get::<TilePos>(a), world.get::<TilePos>(b)) {
        (Some(x), Some(y)) => x.chebyshev(*y).is_some_and(|d| d <= 1),
        _ => false,
    }
}

/// `ownerExamined`: a living examiner within reach falls for it in 0.4 s.
fn examined(world: &mut World, mut cursor: Local<MessageCursor<ExamineClick>>) {
    let clicks: Vec<ExamineClick> = cursor
        .read(world.resource::<Messages<ExamineClick>>())
        .copied()
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for click in clicks {
        let Some(hand) = circle_of(world, click.target) else {
            continue;
        };
        if world.get::<Health>(click.examiner).is_none()
            || !adjacent(world, click.target, click.examiner)
        {
            continue;
        }
        if let Some(mut h) = world.get_mut::<HandItem>(hand) {
            h.pending
                .push((click.examiner, now + 0.4, CircleStage::Noticed));
        }
    }
}

/// The circle game's timers (`addtimer` stages).
fn circle_timers(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &HandItem)>();
    let due: Vec<(Entity, Entity, CircleStage)> = q
        .iter(world)
        .flat_map(|(e, h)| {
            h.pending
                .iter()
                .filter(move |p| p.1 <= now)
                .map(move |p| (e, p.0, p.2))
        })
        .collect();
    for (hand, sucker, stage) in due {
        if let Some(mut h) = world.get_mut::<HandItem>(hand) {
            h.pending
                .retain(|p| !(p.0 == sucker && p.2 == stage && p.1 <= now));
        }
        let Some(owner) = world.get::<CarriedBy>(hand).map(|c| c.0) else {
            continue;
        };
        if world.get_entity(sucker).is_err() {
            continue;
        }
        circle_stage(world, hand, owner, sucker, stage, now);
    }
}

fn circle_stage(
    world: &mut World,
    hand: Entity,
    owner: Entity,
    sucker: Entity,
    stage: CircleStage,
    now: f64,
) {
    let name = crate::security::name_of(world, hand);
    match stage {
        CircleStage::Noticed => {
            let (text, next, delay) = if owner == sucker {
                (
                    format!("Секундочку... вы только что посмотрели на свою {name}!"),
                    CircleStage::SelfBop,
                    1.0,
                )
            } else {
                ("Секундочку... это что, -".to_owned(), CircleStage::Bop, 0.6)
            };
            if let Some(c) = client_of(world, sucker) {
                tell(world, c, Span::Danger, text);
            }
            if let Some(mut h) = world.get_mut::<HandItem>(hand) {
                h.pending.push((sucker, now + delay, next));
            }
        }
        CircleStage::SelfBop => {
            sound(world, owner, BOP_SOUND, 50.0);
            let me = crate::security::name_of(world, owner);
            crate::medical::visible(
                world,
                owner,
                &format!("Вы стыдливо стукаете себя своей {name}."),
                &format!("{me} стыдливо стукает себя своей {name}."),
                Span::Danger,
            );
            crate::status_effects::adjust_stamina(world, owner, 100.0);
            crate::status_effects::apply(world, owner, Incap::Knockdown, 1.0);
            spend(world, hand);
        }
        CircleStage::Bop => {
            let me = crate::security::name_of(world, owner);
            let can_act = !crate::status_effects::incapacitation(world, owner).incapacitated
                && world.get::<Stat>(owner) == Some(&Stat::Conscious);
            if !adjacent(world, owner, sucker) || !can_act {
                if let Some(c) = client_of(world, sucker) {
                    tell(
                        world,
                        c,
                        Span::Notice,
                        format!(
                            "Фух... вы отошли раньше, чем {me} заметил, что вы увидели {name}..."
                        ),
                    );
                }
                return;
            }
            let them = crate::security::name_of(world, sucker);
            if let Some(c) = client_of(world, owner) {
                tell(
                    world,
                    c,
                    Span::Warning,
                    format!("{them} смотрит на вашу {name} и пытается отвести взгляд, но поздно!"),
                );
            }
            if let Some(c) = client_of(world, sucker) {
                tell(
                    world,
                    c,
                    Span::Danger,
                    format!("{me} стукает вас своей {name}!"),
                );
            }
            sound(world, owner, BOP_SOUND, 50.0);
            crate::medical::visible(
                world,
                owner,
                &format!("Вы стукаете {them} своей {name}!"),
                &format!("{me} стукает {them} своей {name}!"),
                Span::Danger,
            );
            crate::status_effects::adjust_stamina(world, sucker, 15.0);
            spend(world, hand);
        }
    }
}

// --- Shared ----------------------------------------------------------------------------------

/// `iscarbon` (as `crate::screentips`).
fn carbon(world: &World, mob: Entity) -> bool {
    world.get::<Health>(mob).is_some()
        && world.get::<Inventory>(mob).is_some()
        && world.get::<crate::silicon::Silicon>(mob).is_none()
        && world.get::<crate::mobs::BasicMob>(mob).is_none()
}

fn lying(world: &World, mob: Entity) -> bool {
    world.get::<BodyPosition>(mob).is_some_and(|b| b.lying())
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

/// The carbons in `orange(1, mob)`.
fn carbons_around(world: &World, mob: Entity) -> Vec<Entity> {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return Vec::new();
    };
    world
        .resource::<ss13_world::SpatialIndex>()
        .in_radius(pos, 1)
        .filter(|&m| m != mob && carbon(world, m) && world.get::<TilePos>(m) != Some(&pos))
        .collect()
}

/// `face_atom`.
fn face(world: &mut World, mob: Entity, at: Entity) {
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(mob), world.get::<TilePos>(at)) else {
        return;
    };
    if let Some(dir) = crate::pull::dir_between(from, to)
        && let Some(mut facing) = world.get_mut::<Facing>(mob)
    {
        facing.set_if_neq(Facing(dir));
    }
}

fn add_mood(world: &mut World, mob: Entity, event: ActiveMood) {
    let now = now(world);
    if let Some(mut mood) = world.get_mut::<Mood>(mob) {
        mood.add_instance(now, event);
    }
}

// --- Noogie ----------------------------------------------------------------------------------

fn has_head(world: &World, mob: Entity) -> bool {
    world
        .get::<Limbs>(mob)
        .is_some_and(|l| !l.limb(BodyZone::Head).missing)
}

/// `user.pulling == target`.
fn holds(world: &World, user: Entity, target: Entity) -> bool {
    world
        .get::<crate::pull::Pulling>(user)
        .is_some_and(|p| p.0 == target)
}

fn stamina_loss(world: &World, mob: Entity) -> f32 {
    world.get::<StatusEffects>(mob).map_or(0.0, |e| e.stamina)
}

/// `TRAIT_ANTENNAE`.
fn antennae(world: &World, mob: Entity) -> bool {
    crate::species::species_of(world, mob) == crate::species::SpeciesId::Moth
}

/// `noogie/attack`: a carbon with a head held in an aggressive grab.
fn noogie(world: &mut World, client: Entity, user: Entity, hand: Entity, target: Entity) {
    // Only mobs are `attack`ed.
    if world.get::<Health>(target).is_none() {
        return;
    }
    if !carbon(world, target) {
        tell(
            world,
            client,
            Span::Warning,
            "Вы не думаете, что этому можно устроить тёрку!",
        );
        return;
    }
    let (me, them) = (name_of(world, user), name_of(world, target));
    if world
        .get::<crate::medical::traumas::Pacifist>(user)
        .is_some()
    {
        tell(
            world,
            client,
            Span::Warning,
            format!(
                "Вы не можете заставить себя устроить тёрку {them}! Вы не хотите никому навредить..."
            ),
        );
        return;
    }
    let grab = world
        .get::<crate::pull::GrabState>(user)
        .copied()
        .unwrap_or_default();
    if !has_head(world, target)
        || !holds(world, user, target)
        || grab < crate::pull::GrabState::Aggressive
        || stamina_loss(world, user) > NOOGIE_MAX_STAMINA
    {
        return;
    }
    // `biological_state & (BIO_FLESH|BIO_CHITIN)`.
    if world
        .get::<Limbs>(target)
        .is_some_and(|l| l.limb(BodyZone::Head).robotic)
    {
        tell(
            world,
            client,
            Span::Warning,
            format!("Вы не можете устроить тёрку {them}: у него нет кожи на голове!"),
        );
        return;
    }
    let antennae = antennae(world, target);
    let prefix = if crate::genetics::effects::is_hulk(world, user) {
        "тошнотворно зверскую"
    } else if antennae {
        "жестокую"
    } else {
        "грубую"
    };
    let (affix, affix_target) = if antennae {
        (
            " по его чувствительным усикам",
            " по вашим очень чувствительным усикам",
        )
    } else {
        ("", "")
    };
    if let Some(&pos) = world.get::<TilePos>(user) {
        show(
            world,
            pos,
            COMBAT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Danger,
                format!("{me} начинает устраивать {them} {prefix} тёрку{affix}!"),
            ),
            &[
                (
                    user,
                    Span::Warning,
                    format!("Вы начинаете устраивать {them} {prefix} тёрку{affix}!"),
                ),
                (
                    target,
                    Span::BoldWarning,
                    format!("{me} начинает устраивать вам {prefix} тёрку{affix_target}!"),
                ),
            ],
        );
    }
    noogie_wait(world, client, user, hand, target, 15.0, 0, true);
}

/// The noogie's `do_after`s; a failed start fails the noogie too.
#[allow(clippy::too_many_arguments)]
fn noogie_wait(
    world: &mut World,
    client: Entity,
    user: Entity,
    hand: Entity,
    target: Entity,
    delay_ds: f32,
    iteration: u32,
    first: bool,
) {
    let actor = Actor {
        client,
        mob: user,
        item: Some(hand),
    };
    let job = crate::construction::Job::Device(DeviceJob::Noogie { iteration, first });
    if !do_after(world, actor, target, delay_ds, 0, job) {
        noogie_interrupted(world, user, target);
    }
}

/// A noogie's `do_after` failed.
pub fn noogie_interrupted(world: &mut World, user: Entity, target: Entity) {
    let (me, them) = (name_of(world, user), name_of(world, target));
    tell_to(
        world,
        user,
        Span::Warning,
        format!("Вам не удалось устроить {them} тёрку!"),
    );
    tell_to(
        world,
        target,
        Span::Danger,
        format!("{me} не удалось устроить вам тёрку!"),
    );
}

/// The end of a noogie's `do_after`: the moodlet after the first one, then `noogie_loop`.
pub fn noogie_round(
    world: &mut World,
    client: Entity,
    user: Entity,
    target: Entity,
    iteration: u32,
    first: bool,
) {
    let Some(hand) = world
        .get::<Inventory>(user)
        .and_then(Inventory::active_item)
        .filter(|&h| gesture_of(world, h) == Some(Gesture::Noogie))
    else {
        return;
    };
    let antennae = antennae(world, target);
    let hulk = crate::genetics::effects::is_hulk(world, user);
    if first {
        let (category, event) = if antennae || hulk {
            ("noogie_harsh", &NOOGIE_HARSH)
        } else {
            ("noogie", &NOOGIE_MOOD)
        };
        crate::mood::add_event(world, target, category, event);
    }
    if !has_head(world, target) || !holds(world, user, target) {
        return;
    }
    let (me, them) = (name_of(world, user), name_of(world, target));
    if stamina_loss(world, user) > NOOGIE_MAX_STAMINA {
        tell(
            world,
            client,
            Span::Warning,
            format!("Вы слишком устали, чтобы продолжать тёрку {them}!"),
        );
        tell_to(
            world,
            target,
            Span::Danger,
            format!("{me} слишком устал, чтобы продолжать вам тёрку!"),
        );
        return;
    }
    let (damage, continues) = {
        let mut rng = world.resource_mut::<CombatRng>();
        let mut damage = rng.range(1, 5);
        if antennae {
            damage += rng.range(3, 7);
        }
        if hulk {
            damage += rng.range(3, 7);
        }
        (damage, rng.chance(33.0))
    };
    if damage >= 5 {
        crate::speech::run_emote(world, target, "scream");
    }
    crate::mobs::defense::hurt(
        world,
        target,
        DamageType::Brute,
        damage as f32,
        Some(BodyZone::Head),
    );
    crate::status_effects::adjust_stamina(world, user, (iteration + 5) as f32);
    if let Some(&pos) = world.get::<TilePos>(user) {
        playsound(world, PlaySound::new(pos, "rustle", 50.0, false));
        if continues {
            show(
                world,
                pos,
                COMBAT_MESSAGE_RANGE,
                ChatChannel::Combat,
                (Span::Danger, format!("{me} продолжает тёрку {them}!")),
                &[
                    (
                        user,
                        Span::Warning,
                        format!("Вы продолжаете устраивать {them} тёрку!"),
                    ),
                    (
                        target,
                        Span::BoldWarning,
                        format!("{me} продолжает устраивать вам тёрку!"),
                    ),
                ],
            );
        }
    }
    let delay = 10.0 + 2.0 * iteration as f32;
    noogie_wait(
        world,
        client,
        user,
        hand,
        target,
        delay,
        iteration + 1,
        false,
    );
}

// --- Hand ------------------------------------------------------------------------------------

const NOT_LYING: &str = "Вы не можете вести себя по-джентльменски, лёжа на полу!";

/// `hand/pre_attack` + `attack`: held out to a carbon (`give`).
fn hand_attack(world: &mut World, user: Entity, target: Entity) {
    if !carbon(world, user) || !carbon(world, target) {
        return;
    }
    if world
        .get::<crate::rest::Rest>(user)
        .is_some_and(|r| r.resting)
    {
        tell_to(world, user, Span::Warning, NOT_LYING);
        return;
    }
    crate::buffs::give(world, user, Some(target));
}

/// `on_offered` of a gesture (`crate::buffs::give`): true when it made its own offer (or
/// refused one).
pub fn on_offered(world: &mut World, giver: Entity, item: Entity, taker: Option<Entity>) -> bool {
    match gesture_of(world, item) {
        Some(Gesture::Slapper) => offer_high_five(world, giver, item),
        Some(Gesture::Hand) => offer_hand(world, giver, item, taker),
        Some(Gesture::Kisser(_)) => offer_kiss(world, giver, item),
        _ => return false,
    }
    true
}

/// `high_fiver/on_offer`: to everyone around.
fn offer_high_five(world: &mut World, giver: Entity, item: Entity) {
    let me = name_of(world, giver);
    crate::medical::visible_range(
        world,
        giver,
        "Вы поднимаете руку, ожидая пятюню!",
        &format!("{me} поднимает руку, ожидая пятюню!"),
        Span::Notice,
        2,
    );
    let name = format!("{me} предлагает пятюню");
    let desc = format!("{name}. Нажмите на уведомление чтобы дать пять!");
    crate::buffs::start_offer(world, giver, item, None, OfferKind::HighFive, name, desc);
}

/// `hand/on_offered`: a help up for a lying carbon next to the giver, else a hand to hold.
fn offer_hand(world: &mut World, giver: Entity, item: Entity, taker: Option<Entity>) {
    if lying(world, giver) {
        tell_to(world, giver, Span::Warning, NOT_LYING);
        return;
    }
    let offered = taker.or_else(|| carbons_around(world, giver).first().copied());
    let me = name_of(world, giver);
    let name = format!("{me} предлагает свою руку");
    if let Some(offered) = offered.filter(|&o| carbon(world, o) && lying(world, o)) {
        let them = name_of(world, offered);
        crate::medical::visible_range(
            world,
            giver,
            &format!("Вы протягиваете руку {them}, чтобы помочь подняться!"),
            &format!("{me} протягивает руку {them}, чтобы помочь подняться!"),
            Span::Notice,
            2,
        );
        let desc = format!("{name}. Нажмите на это уведомление, чтобы вам помогли подняться!");
        crate::buffs::start_offer(
            world,
            giver,
            item,
            Some(offered),
            OfferKind::HelpUp,
            name,
            desc,
        );
        return;
    }
    crate::medical::visible_range(
        world,
        giver,
        "Вы протягиваете руку.",
        &format!("{me} протягивает руку."),
        Span::Notice,
        2,
    );
    let desc = format!("{name}. Нажмите на это уведомление, чтобы вас могли таскать!");
    crate::buffs::start_offer(world, giver, item, None, OfferKind::Gesture, name, desc);
}

/// `kisser/on_offered`: to everyone around, if a carbon is there.
fn offer_kiss(world: &mut World, giver: Entity, item: Entity) {
    if carbons_around(world, giver).is_empty() {
        return;
    }
    let zone = world
        .get::<ZoneSelected>(giver)
        .map_or(TargetZone::Chest, |z| z.0);
    let cheek = zone != TargetZone::Mouth;
    if let Some(mut hand) = world.get_mut::<HandItem>(item) {
        hand.cheek = cheek;
    }
    let on_cheek = if cheek { " в щёчку" } else { "" };
    let me = name_of(world, giver);
    crate::medical::visible_range(
        world,
        giver,
        &format!("Вы слегка наклоняетесь, предлагая поцелуй{on_cheek}!"),
        &format!("{me} слегка наклоняется, предлагая поцелуй{on_cheek}!"),
        Span::Notice,
        2,
    );
    let name = format!("{me} предлагает {}", name_of(world, item));
    let desc = format!("{name}. Нажмите на уведомление, чтобы взять.");
    crate::buffs::start_offer(world, giver, item, None, OfferKind::Gesture, name, desc);
}

/// `on_offer_taken` (`crate::buffs::take`): a gesture never changes hands. True for every
/// gesture.
pub fn on_offer_taken(world: &mut World, giver: Entity, taker: Entity, item: Entity) -> bool {
    let Some(kind) = gesture_of(world, item) else {
        return false;
    };
    match kind {
        Gesture::Hand => hand_taken(world, giver, taker, item),
        Gesture::Kisser(kiss) => kiss_taken(world, giver, taker, item, kiss),
        _ => {}
    }
    true
}

/// `hand/on_offer_taken`: a lying taker is helped up, a standing one is pulled along.
fn hand_taken(world: &mut World, giver: Entity, taker: Entity, item: Entity) {
    let (me, them) = (name_of(world, giver), name_of(world, taker));
    let Some(&pos) = world.get::<TilePos>(taker) else {
        return;
    };
    if lying(world, taker) {
        show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            (
                Span::Notice,
                format!("{me} трясет {them}, пытаясь его поднять!"),
            ),
            &[
                (
                    giver,
                    Span::Notice,
                    format!("Вы трясете {them}, пытаясь его поднять!"),
                ),
                (
                    taker,
                    Span::Notice,
                    format!("{me} трясет вас, чтобы вас поднять!"),
                ),
            ],
        );
        // That didn't help them. Awkwaaaaard.
        if !shake_up(world, taker) {
            return;
        }
        show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            (Span::Notice, format!("{me} помогает {them} подняться!")),
            &[
                (
                    giver,
                    Span::Notice,
                    format!("Вы помогаете {them} подняться!"),
                ),
                (
                    taker,
                    Span::Notice,
                    format!("Вы берёте {me} за руку, и вам помогают подняться! Как мило!"),
                ),
            ],
        );
        helped_up_mood(world, giver, taker, true);
        helped_up_mood(world, taker, giver, false);
        spend(world, item);
        return;
    }
    if !crate::pull::can_be_pulled(world, giver, taker) {
        return;
    }
    world.entity_mut(giver).insert((
        crate::pull::Pulling(taker),
        crate::pull::GrabState::Passive,
        crate::pull::PullSlowdown(0.0),
    ));
    tell_to(
        world,
        giver,
        Span::Notice,
        format!("{them} берёт вас за руку, позволяя вести себя за собой."),
    );
    tell_to(
        world,
        taker,
        Span::Notice,
        format!("Вы берёте {me} за руку, позволяя вести вас за собой. Как вежливо!"),
    );
    spend(world, item);
}

/// `help_shake_act`'s end: `adjust_status_effects_on_shake_up`, `set_resting(FALSE)` and,
/// unless floored otherwise, `get_up(instant)`. True when the mob stands.
fn shake_up(world: &mut World, mob: Entity) -> bool {
    crate::status_effects::shake_up(world, mob);
    if let Some(mut rest) = world.get_mut::<crate::rest::Rest>(mob) {
        rest.resting = false;
        rest.down = false;
        rest.up_at = None;
    }
    world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && !world
            .get::<StatusEffects>(mob)
            .is_some_and(StatusEffects::floored)
}

/// `/datum/mood_event/helped_up`: only the compassionate, callous and misanthropic feel it.
fn helped_up_mood(world: &mut World, mob: Entity, other: Entity, helper: bool) {
    use crate::personality::Personality as P;
    let Some(mood) = world.get::<Mood>(mob) else {
        return;
    };
    let other_name = name_of(world, other);
    let (change, text) =
        if mood.has_personality(P::Callous) || mood.has_personality(P::Misanthropic) {
            let text = if helper {
                "Они должны были сами достать свою задницу из своих проблем.".to_owned()
            } else {
                "Я мог встать сам.".to_owned()
            };
            (-2.0, text)
        } else if mood.has_personality(P::Compassionate) {
            let text = if helper {
                format!("Мне понравилось помогать {other_name}!")
            } else {
                format!(
                    "{} помог мне, как мило с их стороны!",
                    ss13_chat::capitalize(&other_name)
                )
            };
            (2.0, text)
        } else {
            return;
        };
    // Different categories: one can be helped up and then help someone else up.
    let category = if helper { "helping_up" } else { "helped_up" };
    let mut event = ActiveMood::new(category, &HELPED_UP);
    event.mood_change = change;
    event.description = text;
    add_mood(world, mob, event);
}

// --- High five -------------------------------------------------------------------------------

/// `alert/give/highfive/handle_transfer`: the slapper still held, a high five; gone, "too slow".
pub fn high_five_click(world: &mut World, giver: Entity, taker: Entity, slapper: Entity) {
    if world
        .get::<TooSlow>(giver)
        .is_some_and(|t| t.0.iter().any(|(r, _)| *r == taker))
    {
        return;
    }
    let held = world.get_entity(slapper).is_ok()
        && world
            .get::<Inventory>(giver)
            .is_some_and(|i| i.hands.contains(&Some(slapper)));
    if held {
        high_five(world, giver, taker);
    } else {
        too_slow(world, giver, taker);
    }
}

/// `high_fiver/on_offer_taken`: as many fives as hand gestures held.
fn high_five(world: &mut World, giver: Entity, taker: Entity) {
    let slappers = world.get::<Inventory>(giver).map_or(0, |i| {
        i.hands
            .iter()
            .flatten()
            .filter(|&&h| world.get::<HandItem>(h).is_some())
            .count()
    });
    let high_ten = slappers >= 2;
    let (what, category, event) = if high_ten {
        ("десять", "high-ten", &HIGH_TEN)
    } else {
        ("пять", "high-five", &HIGH_FIVE)
    };
    let (me, them) = (name_of(world, giver), name_of(world, taker));
    let free = world
        .get::<Inventory>(taker)
        .is_some_and(|i| i.hands.iter().any(Option::is_none));
    if !free {
        tell_to(
            world,
            taker,
            Span::Warning,
            format!("Вы не можете дать {what} {me} без свободных рук!"),
        );
        crate::mood::add_event(world, taker, category, &HIGH_FIVE_FULL_HAND);
        return;
    }
    let Some(&pos) = world.get::<TilePos>(giver) else {
        return;
    };
    let volume = (50.0 * slappers as f32).min(300.0);
    playsound(
        world,
        PlaySound::new(pos, SLAP_SOUND, volume, true).extrarange(1.0),
    );
    let (others, own, theirs) = if high_ten {
        (
            format!("{them} с энтузиазмом даёт {me} десять!"),
            format!("Ух ты! {them} даёт вам десять!"),
            format!("Вы даёте {me} десять, вкладывая всё, что у вас есть!"),
        )
    } else {
        (
            format!("{them} даёт {me} пять!"),
            format!("Отлично! {them} даёт вам пять!"),
            format!("Вы даёте {me} пять!"),
        )
    };
    show(
        world,
        pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Info,
        (Span::Notice, others),
        &[(giver, Span::Notice, own), (taker, Span::Notice, theirs)],
    );
    crate::mood::add_event(world, giver, category, event);
    crate::mood::add_event(world, taker, category, event);
    crate::buffs::end_offer(world, giver);
}

/// `too_slow_p1`: the rube reaches for a hand that is not there.
fn too_slow(world: &mut World, giver: Entity, rube: Entity) {
    let (me, them) = (name_of(world, giver), name_of(world, rube));
    if let Some(&pos) = world.get::<TilePos>(giver) {
        show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            (
                Span::Notice,
                format!("{them} спешит, чтобы дать пять {me}, но..."),
            ),
            &[
                (
                    giver,
                    Span::Notice,
                    format!(
                        "{them} попадается на ваш трюк, как и планировалось, бросившись за \
                         несуществующей пятюней! Классика!"
                    ),
                ),
                (
                    rube,
                    Span::Notice,
                    format!("Вы тянетесь, чтобы дать {me} пять, но..."),
                ),
            ],
        );
    }
    let at = now(world) + TOO_SLOW_SECS;
    if let Some(mut pending) = world.get_mut::<TooSlow>(giver) {
        pending.0.push((rube, at));
    } else {
        world.entity_mut(giver).insert(TooSlow(vec![(rube, at)]));
    }
}

fn too_slow_timers(world: &mut World) {
    let now = now(world);
    let mut q = world.query::<(Entity, &TooSlow)>();
    let due: Vec<(Entity, Entity)> = q
        .iter(world)
        .flat_map(|(giver, t)| {
            t.0.iter()
                .filter(move |(_, at)| *at <= now)
                .map(move |(rube, _)| (giver, *rube))
        })
        .collect();
    for (giver, rube) in due {
        let empty = world.get_mut::<TooSlow>(giver).map(|mut t| {
            t.0.retain(|(r, _)| *r != rube);
            t.0.is_empty()
        });
        if empty == Some(true) {
            world.entity_mut(giver).remove::<TooSlow>();
        }
        too_slow_p2(world, giver, rube);
    }
}

/// `too_slow_p2`: the hand is pulled away, the rube falls; the rube's alert goes either way.
fn too_slow_p2(world: &mut World, giver: Entity, rube: Entity) {
    let here = |e: Entity| world.get::<TilePos>(e).copied();
    if let (Some(pos), Some(_)) = (here(giver), here(rube)) {
        let (me, them) = (name_of(world, giver), name_of(world, rube));
        show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            (
                Span::Danger,
                format!(
                    "{me} убирает руку от шлепка {them} в последнюю секунду, полностью уклоняясь \
                     от пятюни!"
                ),
            ),
            &[
                (
                    giver,
                    Span::Notice,
                    format!("{them} не удаётся коснуться вашей руки, выставляя себя на посмешище!"),
                ),
                (
                    rube,
                    Span::BoldWarning,
                    format!(
                        "НЕТ! {} ОТДЁРГИВАЕТ СВОЮ РУКУ! ТЫ ДЕЙСТВУЕШЬ СЛИШКОМ МЕДЛЕННО!",
                        me.to_uppercase()
                    ),
                ),
            ],
        );
        playsound(
            world,
            PlaySound::new(pos, THUDSWOOSH, 100.0, true).extrarange(1.0),
        );
        crate::status_effects::knockdown(world, rube, 1.0, 0.0);
        crate::mood::add_event(world, giver, "high_five", &DOWN_LOW);
        let mut event = ActiveMood::new("high_five", &TOO_SLOW);
        event.mood_change *= onlookers(world, rube) as f32;
        add_mood(world, rube, event);
        crate::buffs::end_offer(world, giver);
    }
    crate::buffs::remove_taker(world, giver, rube);
}

/// `too_slow/add_effects`: the rube and the conscious carbons in view, at most 8.
// shortcut: range only, no line of sight, as `crate::health::clients_in_view`.
fn onlookers(world: &World, rube: Entity) -> u32 {
    let Some(&pos) = world.get::<TilePos>(rube) else {
        return 1;
    };
    let seen = world
        .resource::<ss13_world::SpatialIndex>()
        .in_radius(pos, 7)
        .filter(|&m| {
            m != rube && carbon(world, m) && world.get::<Stat>(m) == Some(&Stat::Conscious)
        })
        .count() as u32;
    1 + seen.min(7)
}

// --- Kiss ------------------------------------------------------------------------------------

/// `kisser/ranged_interact_with_atom`: blown at what was clicked, the hand spent.
fn blow_kiss(
    world: &mut World,
    user: Entity,
    hand: Entity,
    kind: Kiss,
    target: Entity,
    at: TilePos,
) {
    let (me, them, kiss) = (name_of(world, user), name_of(world, target), kind.name());
    crate::medical::visible(
        world,
        user,
        &format!("Вы отправляете {kiss} {them}!"),
        &format!("{me} отправляет {kiss} {them}!"),
        Span::Notice,
    );
    // `impacted = list(user)`: never at oneself.
    fire_kiss(
        world,
        user,
        kind,
        (target != user).then_some(target),
        at,
        false,
    );
    spend(world, hand);
}

/// The kiss projectile from `firer` at `target`/`at`; `direct` when given point blank
/// (`suppressed`).
fn fire_kiss(
    world: &mut World,
    firer: Entity,
    kind: Kiss,
    target: Option<Entity>,
    at: TilePos,
    direct: bool,
) {
    let Some(&from) = world.get::<TilePos>(firer) else {
        return;
    };
    let data = kind.projectile();
    crate::projectile::fire_with(
        world,
        crate::projectile::Shot {
            data: &data,
            firer,
            from,
            target,
            target_pos: at,
            // "let's keep it PG, people".
            zone: BodyZone::Head,
            hit_prone: false,
            pixel: [0, 0],
            deviation: 0.0,
            fired_from: None,
        },
        BlownKiss { kind, direct },
    );
}

/// A left click past reach with a kiss in the active hand.
fn ranged_kisses(world: &mut World, mut cursor: Local<MessageCursor<RangedClick>>) {
    let clicks: Vec<RangedClick> = cursor
        .read(world.resource::<Messages<RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        if click.modifiers.right {
            continue;
        }
        let Some(hand) = world
            .get::<Inventory>(click.mob)
            .and_then(Inventory::active_item)
        else {
            continue;
        };
        if let Some(Gesture::Kisser(kind)) = gesture_of(world, hand) {
            blow_kiss(world, click.mob, hand, kind, click.target, click.target_pos);
        }
    }
}

/// `kisser/on_offer_taken`: given point blank (on the cheek unless aimed at the mouth).
fn kiss_taken(world: &mut World, giver: Entity, taker: Entity, item: Entity, kind: Kiss) {
    let cheek = world.get::<HandItem>(item).is_some_and(|h| h.cheek);
    let on_cheek = if cheek { " в щёчку" } else { "" };
    let (me, them, kiss) = (name_of(world, giver), name_of(world, taker), kind.name());
    let (Some(&pos), Some(&at)) = (world.get::<TilePos>(giver), world.get::<TilePos>(taker)) else {
        return;
    };
    show(
        world,
        pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Info,
        (
            Span::Notice,
            format!("{me} дарит {them} {kiss}{on_cheek}!!"),
        ),
        &[
            (
                giver,
                Span::Notice,
                format!("Вы дарите {them} {kiss}{on_cheek}!"),
            ),
            (
                taker,
                Span::Notice,
                format!("{me} дарит вам {kiss}{on_cheek}!"),
            ),
        ],
    );
    face(world, giver, taker);
    face(world, taker, giver);
    fire_kiss(world, giver, kind, Some(taker), at, true);
    spend(world, item);
}

/// `kiss/fire`'s name: "blown by" its firer, but for the syndie kiss (`silent_blown`).
fn blown_name(world: &World, kind: Kiss, firer: Entity) -> String {
    if kind == Kiss::Syndie || world.get_entity(firer).is_err() {
        return kind.name().to_owned();
    }
    format!("воздушный {} от {}", kind.name(), name_of(world, firer))
}

/// `kiss/impact` on a living mob of a kiss without damage: `harmless_on_hit` instead of a hit.
/// True when it was one.
pub fn harmless_kiss(world: &mut World, e: Entity, p: &Projectile, target: Entity) -> bool {
    let Some(kiss) = world.get::<BlownKiss>(e).copied() else {
        return false;
    };
    if p.data.damage > 0.0 || world.get::<Health>(target).is_none() {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return true;
    };
    playsound(world, PlaySound::new(pos, KISS_SOUND, 100.0, true));
    if !kiss.direct {
        let name = ss13_chat::capitalize(&blown_name(world, kiss.kind, p.firer));
        let them = name_of(world, target);
        show(
            world,
            pos,
            COMBAT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (Span::Danger, format!("{name} попадает по {them}.")),
            &[(
                target,
                Span::BoldWarning,
                format!("{name} попадает по вам!"),
            )],
        );
    }
    kiss_mood(world, target, p.firer, kiss.direct);
    try_fluster(world, target, p.firer);
    if kiss.kind == Kiss::French {
        garlic_aftertaste(world, target);
    }
    true
}

/// A kiss's `on_hit` after a real hit: the moodlet and fluster on a mob, the kiss of death's
/// broken heart, the chef's kiss on food.
pub fn kiss_on_hit(world: &mut World, e: Entity, p: &Projectile, target: Entity) {
    let Some(kiss) = world.get::<BlownKiss>(e).copied() else {
        return;
    };
    if world.get::<Health>(target).is_some() {
        kiss_mood(world, target, p.firer, kiss.direct);
        try_fluster(world, target, p.firer);
    }
    match kiss.kind {
        // `kiss/death/on_hit`: don't go breaking my heart.
        Kiss::Death if carbon(world, target) => {
            crate::medical::organs::adjust_organ_loss(
                world,
                target,
                crate::medical::organs::Slot::Heart,
                999.0,
            );
        }
        Kiss::Chef => chef_kiss(world, p.firer, target),
        _ => {}
    }
}

/// A chef's kiss hits the food it was blown at ([`crate::projectile`]'s `select_target`).
pub fn kisses_food(world: &World, projectile: Entity, original: Entity) -> bool {
    world
        .get::<BlownKiss>(projectile)
        .is_some_and(|k| k.kind == Kiss::Chef)
        && world.get::<crate::food::Food>(original).is_some()
}

/// `/datum/mood_event/kiss` with its kisser.
fn kiss_mood(world: &mut World, target: Entity, firer: Entity, direct: bool) {
    let mut event = ActiveMood::new("kiss", &KISS_MOOD);
    if world.get_entity(firer).is_ok() {
        let beau = name_of(world, firer);
        event.description = if direct {
            format!("Поцелуй от {beau}, ахх!!")
        } else {
            format!("Воздушный поцелуй от {beau}, я настоящая находка!")
        };
    }
    add_mood(world, target, event);
}

/// `kiss/try_fluster`: the socially anxious may get flustered (half the time).
// shortcut: no `TRAIT_FEARLESS` (no source here) and no flustered "Uhhh..." line.
fn try_fluster(world: &mut World, target: Entity, firer: Entity) {
    let anxious = world
        .get::<crate::quirks::Quirks>(target)
        .is_some_and(|q| q.has(crate::quirks::Quirk::SocialAnxiety));
    if !anxious
        || world.get::<Stat>(target) != Some(&Stat::Conscious)
        || crate::vision::is_blind(world, target)
    {
        return;
    }
    let (spared, roll, ds) = {
        let mut rng = world.resource_mut::<CombatRng>();
        let spared = rng.chance(50.0);
        let roll = rng.range(1, 3);
        let ds = match roll {
            1 => rng.range(50, 100),
            2 => rng.range(100, 300),
            _ => rng.range(30, 80),
        };
        (spared, roll, ds)
    };
    if spared {
        return;
    }
    let secs = ds as f32 / 10.0;
    let them = name_of(world, target);
    let staring = if world.get_entity(firer).is_ok() {
        name_of(world, firer)
    } else {
        "потолок".to_owned()
    };
    let (others, own) = match roll {
        1 => {
            crate::status_effects::adjust_timed(world, target, Timed::Confusion, secs, None);
            (
                format!("{them} слегка спотыкается, заливаясь ярким румянцем!"),
                "Вы на мгновение теряете контроль над конечностями: кровь приливает к лицу, и оно \
                 становится ярко-красным!"
                    .to_owned(),
            )
        }
        2 => {
            crate::status_effects::adjust_timed(world, target, Timed::Stutter, secs, None);
            (
                format!("{them} что-то тихо бормочет и вдруг давится словами!"),
                "Язык будто проваливается в горло, и вы изо всех сил пытаетесь вспомнить, как \
                 говорить слова!"
                    .to_owned(),
            )
        }
        _ => {
            face(world, target, firer);
            crate::status_effects::apply(world, target, Incap::Stun, secs);
            (
                format!("{them} застывает с ошеломлённым видом, уставившись на {staring}!"),
                format!(
                    "Ваш мозг совершенно не понимает, что только что произошло, и вы застываете, \
                     уставившись на {staring}, будто на целую вечность!"
                ),
            )
        }
    };
    if let Some(&pos) = world.get::<TilePos>(target) {
        show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            (Span::Plain, others),
            &[(target, Span::BoldWarning, format!("Ого! {own}"))],
        );
    }
}

/// `kiss/french/harmless_on_hit`: 1 u of garlic in the blood (not stacked).
fn garlic_aftertaste(world: &mut World, target: Entity) {
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return;
    };
    if world.get::<crate::chem::MobReagents>(target).is_none() {
        return;
    }
    if let Some(id) = chem.id(GARLIC)
        && reagent_amount(world, target, GARLIC) <= 0.0
        && let Some(mut m) = world.get_mut::<crate::chem::MobReagents>(target)
    {
        m.blood.add_reagent_list(&chem, &[(id, 1.0)], None, false);
    }
    let them = name_of(world, target);
    crate::medical::visible_range(
        world,
        target,
        "Ух, какое сильное чесночное послевкусие!",
        &format!("{them} как-то странно морщится."),
        Span::Plain,
        COMBAT_MESSAGE_RANGE,
    );
}

/// `kiss/chef/on_hit`: handmade food next to the chef gets love by its nutriment.
fn chef_kiss(world: &mut World, chef: Entity, food: Entity) {
    if world.get::<crate::food::Food>(food).is_none()
        || world.get::<crate::chem::Reagents>(food).is_none()
        || !adjacent(world, chef, food)
    {
        return;
    }
    let name = name_of(world, food);
    if world.get::<crate::food::Handmade>(food).is_none() {
        tell_to(
            world,
            chef,
            Span::Warning,
            format!(
                "Секундочку, это не вы приготовили {name}. Как вы можете выдавать это за своё?"
            ),
        );
        return;
    }
    let Some(chem) = world
        .get_resource::<crate::chem::Chem>()
        .map(|c| c.0.clone())
    else {
        return;
    };
    let Some(love) = chem.id(LOVE) else {
        return;
    };
    let (loved, nutriment) = {
        let holder = &world.get::<crate::chem::Reagents>(food).expect("checked").0;
        let nutriment: f32 = holder
            .reagents
            .iter()
            .filter(|r| chem.is_type(r.id, NUTRIMENT))
            .map(|r| r.volume)
            .sum();
        (holder.amount(love) > 0.0, nutriment)
    };
    if loved {
        tell_to(
            world,
            chef,
            Span::Warning,
            format!("Вы уже благословили {name} всем своим сердцем и душой."),
        );
        return;
    }
    if nutriment <= 0.0 {
        tell_to(
            world,
            chef,
            Span::Warning,
            format!("В {name} недостаточно питательных веществ, чтобы это было настоящим блюдом."),
        );
        return;
    }
    if let Some(&pos) = world.get::<TilePos>(food) {
        let me = name_of(world, chef);
        show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Info,
            (
                Span::Notice,
                format!("{me} посылает поцелуй повара над {name}."),
            ),
            &[(
                chef,
                Span::Notice,
                format!("Вы посылаете поцелуй повара над {name}, объявляя это блюдо идеальным."),
            )],
        );
    }
    if let Some(mut r) = world.get_mut::<crate::chem::Reagents>(food) {
        r.0.add_reagent_list(
            &chem,
            &[(love, (nutriment / 4.0).clamp(1.0, 10.0))],
            None,
            false,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A table slap spends one of three smacks; the third spends the hand.
    #[test]
    fn three_table_slaps_spend_the_hand() {
        let mut world = World::new();
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        world.init_resource::<Messages<PlaySound>>();
        let hand = world
            .spawn(HandItem {
                kind: Gesture::Slapper,
                smacks_left: TABLE_SMACKS,
                cheek: false,
                pending: Vec::new(),
            })
            .id();
        let mob = world
            .spawn(Inventory {
                hands: [Some(hand), None],
                ..default()
            })
            .id();
        world.entity_mut(hand).insert(CarriedBy(mob));
        let table = world.spawn_empty().id();
        slap_table(&mut world, mob, hand, table);
        slap_table(&mut world, mob, hand, table);
        assert_eq!(world.get::<HandItem>(hand).unwrap().smacks_left, 1);
        // A used hand cannot slam: it slaps instead, and that was the last smack.
        slam_table(&mut world, mob, hand, table);
        assert!(world.get_entity(hand).is_err());
        assert_eq!(world.get::<Inventory>(mob).unwrap().hands[0], None);
    }
}
