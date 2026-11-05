//! The ported `/datum/hallucination` subtypes: their weights and tiers ([`TABLE`]) and what each
//! one's `start()` does ([`start`]).
// shortcut: not ported — delusions (no per-client appearance override of other mobs), the
// in-hand and nearby fake items, stray bullets, xeno and Bubblegum attacks, bolted airlocks,
// hazards, plasma floods, eyes in the dark (no server-side light levels), the hacked APC,
// "your mother", the dusting death and the freezer body; the shock's black body override, the
// fire's stop-drop-roll, the fake death's muting, jitter and stutter. Their weight is missing
// from the pool, so the ported ones come a bit more often than in tg.

use bevy::prelude::*;
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::{Dir, TilePos};

use super::{
    ABOVE_MOB_LAYER, ImageLook, clear_fake_alert, client_of, exists, first_name, hide_image,
    hide_later, humans_in_view, last_name, later, line, pick_line, play_local, pos_of,
    random_far_turf, rng, screwy, show_image, show_on, tell, tell_message, throw_fake_alert,
    view_tiles,
};
use crate::combat::CombatRng;
use crate::health::Stat;
use crate::status_effects::Incap;
use crate::zone::BodyZone;

/// `HALLUCINATION_TIER_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    Common = 1,
    Uncommon,
    Rare,
    VerySpecial,
    /// Never picked at random, only caused.
    Never,
}

/// A ported hallucination type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Sound(Snd),
    Chat,
    Message,
    Telepathy,
    Station(Station),
    Alert(FakeAlert),
    Screwy(Screwy),
    HealthDoll,
    Battle(Battle),
    Shock,
    BloodFlow,
    Ice,
    Fire,
    Death,
    Body(Body),
}

/// `/datum/hallucination/fake_sound/normal/*` and `weird/*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Snd {
    Airlock,
    AirlockPry,
    Console,
    Boom,
    DistantBoom,
    Glass,
    Alarm,
    Beepsky,
    Mech,
    WallDeconstruction,
    DoorHacking,
    Steam,
    Flash,
    Ringtone,
    ChimpEvent,
    Colossus,
    Creepy,
    Curse,
    GameOver,
    Hallelujah,
    Highlander,
    Hyperspace,
    Laugher,
    Phone,
    Spell,
    JustJaunt,
    Summon,
    Tesloose,
    Xeno,
    RadioStatic,
    IceCrack,
}

/// `/datum/hallucination/station_message/*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Station {
    Blob,
    ShuttleDock,
    MalfAi,
    Heretic,
    CultSummon,
    Meteors,
    SupermatterDelam,
}

/// `/datum/hallucination/fake_alert/*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FakeAlert {
    NeedOxygen,
    NeedPlasma,
    NeedCo2,
    BadOxygen,
    BadPlasma,
    BadCo2,
    Gravity,
    Fire,
    Hot,
    Cold,
    Pressure,
    Law,
    Locked,
    Hacked,
    NeedCharge,
}

/// `/datum/hallucination/screwy_hud/*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Screwy {
    Crit,
    Dead,
    Healthy,
}

/// `/datum/hallucination/battle/*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Battle {
    Disabler,
    Laser,
    StunProd,
    HarmBaton,
    ESword,
    Bomb,
}

/// `/datum/hallucination/body/*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Body {
    Husk,
    HuskSideways,
    Ghost,
    Hole,
    StaticGuy,
    Alien,
    MiniBubblegum,
    Chrono,
    God,
    Sling,
    Faceless,
    Bones,
}

/// A type's `random_hallucination_weight` and `hallucination_tier`.
#[derive(Clone, Copy, Debug)]
pub struct KindDef {
    pub kind: Kind,
    pub path: &'static str,
    pub weight: f32,
    pub tier: Tier,
}

const fn k(kind: Kind, path: &'static str, weight: f32, tier: Tier) -> KindDef {
    KindDef {
        kind,
        path,
        weight,
        tier,
    }
}

use Tier::{Common, Rare, Uncommon, VerySpecial};

/// Every ported non-abstract type, in `typesof` order.
pub static TABLE: &[KindDef] = &[
    k(
        Kind::Battle(Battle::Bomb),
        "/datum/hallucination/battle/bomb",
        3.0,
        Common,
    ),
    k(
        Kind::Battle(Battle::ESword),
        "/datum/hallucination/battle/e_sword",
        3.0,
        Common,
    ),
    k(
        Kind::Battle(Battle::Disabler),
        "/datum/hallucination/battle/gun/disabler",
        3.0,
        Common,
    ),
    k(
        Kind::Battle(Battle::Laser),
        "/datum/hallucination/battle/gun/laser",
        3.0,
        Common,
    ),
    k(
        Kind::Battle(Battle::HarmBaton),
        "/datum/hallucination/battle/harm_baton",
        3.0,
        Common,
    ),
    k(
        Kind::Battle(Battle::StunProd),
        "/datum/hallucination/battle/stun_prod",
        3.0,
        Common,
    ),
    k(
        Kind::BloodFlow,
        "/datum/hallucination/blood_flow",
        3.0,
        Common,
    ),
    k(
        Kind::Body(Body::Ghost),
        "/datum/hallucination/body/ghost",
        2.0,
        Common,
    ),
    k(
        Kind::Body(Body::Hole),
        "/datum/hallucination/body/hole",
        1.0,
        Common,
    ),
    k(
        Kind::Body(Body::Husk),
        "/datum/hallucination/body/husk",
        8.0,
        Common,
    ),
    k(
        Kind::Body(Body::HuskSideways),
        "/datum/hallucination/body/husk/sideways",
        4.0,
        Common,
    ),
    k(
        Kind::Body(Body::StaticGuy),
        "/datum/hallucination/body/staticguy",
        1.0,
        Common,
    ),
    k(
        Kind::Body(Body::Alien),
        "/datum/hallucination/body/weird/alien",
        0.1,
        Rare,
    ),
    k(
        Kind::Body(Body::Bones),
        "/datum/hallucination/body/weird/bones",
        0.1,
        VerySpecial,
    ),
    k(
        Kind::Body(Body::Chrono),
        "/datum/hallucination/body/weird/chrono",
        0.1,
        VerySpecial,
    ),
    k(
        Kind::Body(Body::Faceless),
        "/datum/hallucination/body/weird/faceless",
        0.1,
        Rare,
    ),
    k(
        Kind::Body(Body::God),
        "/datum/hallucination/body/weird/god",
        0.1,
        Rare,
    ),
    k(
        Kind::Body(Body::MiniBubblegum),
        "/datum/hallucination/body/weird/mini_bubblegum",
        0.1,
        Rare,
    ),
    k(
        Kind::Body(Body::Sling),
        "/datum/hallucination/body/weird/sling",
        0.1,
        Rare,
    ),
    k(Kind::Chat, "/datum/hallucination/chat", 100.0, Common),
    k(Kind::Death, "/datum/hallucination/death", 1.0, Uncommon),
    k(
        Kind::Alert(FakeAlert::BadCo2),
        "/datum/hallucination/fake_alert/bad_co2",
        1.0,
        Common,
    ),
    k(
        Kind::Alert(FakeAlert::BadOxygen),
        "/datum/hallucination/fake_alert/bad_oxygen",
        1.0,
        Common,
    ),
    k(
        Kind::Alert(FakeAlert::BadPlasma),
        "/datum/hallucination/fake_alert/bad_plasma",
        1.0,
        Common,
    ),
    k(
        Kind::Alert(FakeAlert::Cold),
        "/datum/hallucination/fake_alert/cold",
        1.0,
        Common,
    ),
    k(
        Kind::Alert(FakeAlert::Fire),
        "/datum/hallucination/fake_alert/fire",
        1.0,
        Common,
    ),
    k(
        Kind::Alert(FakeAlert::Gravity),
        "/datum/hallucination/fake_alert/gravity",
        1.0,
        Common,
    ),
    k(
        Kind::Alert(FakeAlert::Hacked),
        "/datum/hallucination/fake_alert/hacked",
        1.0,
        Common,
    ),
    k(
        Kind::Alert(FakeAlert::Hot),
        "/datum/hallucination/fake_alert/hot",
        1.0,
        Common,
    ),
    k(
        Kind::Alert(FakeAlert::Law),
        "/datum/hallucination/fake_alert/law",
        1.0,
        Common,
    ),
    k(
        Kind::Alert(FakeAlert::Locked),
        "/datum/hallucination/fake_alert/locked",
        1.0,
        Common,
    ),
    k(
        Kind::Alert(FakeAlert::NeedCharge),
        "/datum/hallucination/fake_alert/need_charge",
        1.0,
        Common,
    ),
    k(
        Kind::Alert(FakeAlert::NeedCo2),
        "/datum/hallucination/fake_alert/need_co2",
        1.0,
        Common,
    ),
    k(
        Kind::Alert(FakeAlert::NeedOxygen),
        "/datum/hallucination/fake_alert/need_oxygen",
        1.0,
        Common,
    ),
    k(
        Kind::Alert(FakeAlert::NeedPlasma),
        "/datum/hallucination/fake_alert/need_plasma",
        1.0,
        Common,
    ),
    k(
        Kind::Alert(FakeAlert::Pressure),
        "/datum/hallucination/fake_alert/pressure",
        1.0,
        Common,
    ),
    k(
        Kind::HealthDoll,
        "/datum/hallucination/fake_health_doll",
        12.0,
        Common,
    ),
    k(Kind::Message, "/datum/hallucination/message", 60.0, Common),
    k(
        Kind::Sound(Snd::Airlock),
        "/datum/hallucination/fake_sound/normal/airlock",
        5.0,
        Common,
    ),
    k(
        Kind::Sound(Snd::AirlockPry),
        "/datum/hallucination/fake_sound/normal/airlock_pry",
        5.0,
        Common,
    ),
    k(
        Kind::Sound(Snd::Alarm),
        "/datum/hallucination/fake_sound/normal/alarm",
        5.0,
        Common,
    ),
    k(
        Kind::Sound(Snd::Beepsky),
        "/datum/hallucination/fake_sound/normal/beepsky",
        5.0,
        Common,
    ),
    k(
        Kind::Sound(Snd::Boom),
        "/datum/hallucination/fake_sound/normal/boom",
        5.0,
        Common,
    ),
    k(
        Kind::Sound(Snd::Console),
        "/datum/hallucination/fake_sound/normal/console",
        5.0,
        Common,
    ),
    k(
        Kind::Sound(Snd::DistantBoom),
        "/datum/hallucination/fake_sound/normal/distant_boom",
        5.0,
        Common,
    ),
    k(
        Kind::Sound(Snd::DoorHacking),
        "/datum/hallucination/fake_sound/normal/door_hacking",
        5.0,
        Common,
    ),
    k(
        Kind::Sound(Snd::Flash),
        "/datum/hallucination/fake_sound/normal/flash",
        2.0,
        Common,
    ),
    k(
        Kind::Sound(Snd::Glass),
        "/datum/hallucination/fake_sound/normal/glass",
        5.0,
        Common,
    ),
    k(
        Kind::Sound(Snd::Mech),
        "/datum/hallucination/fake_sound/normal/mech",
        5.0,
        Common,
    ),
    k(
        Kind::Sound(Snd::Ringtone),
        "/datum/hallucination/fake_sound/normal/ringtone",
        5.0,
        Common,
    ),
    k(
        Kind::Sound(Snd::Steam),
        "/datum/hallucination/fake_sound/normal/steam",
        5.0,
        Common,
    ),
    k(
        Kind::Sound(Snd::WallDeconstruction),
        "/datum/hallucination/fake_sound/normal/wall_deconstruction",
        5.0,
        Common,
    ),
    k(
        Kind::Sound(Snd::ChimpEvent),
        "/datum/hallucination/fake_sound/weird/chimp_event",
        1.0,
        VerySpecial,
    ),
    k(
        Kind::Sound(Snd::Colossus),
        "/datum/hallucination/fake_sound/weird/colossus",
        1.0,
        VerySpecial,
    ),
    k(
        Kind::Sound(Snd::Creepy),
        "/datum/hallucination/fake_sound/weird/creepy",
        1.0,
        Common,
    ),
    k(
        Kind::Sound(Snd::Curse),
        "/datum/hallucination/fake_sound/weird/curse_sound",
        1.0,
        VerySpecial,
    ),
    k(
        Kind::Sound(Snd::GameOver),
        "/datum/hallucination/fake_sound/weird/game_over",
        1.0,
        Rare,
    ),
    k(
        Kind::Sound(Snd::Hallelujah),
        "/datum/hallucination/fake_sound/weird/hallelujah",
        1.0,
        VerySpecial,
    ),
    k(
        Kind::Sound(Snd::Highlander),
        "/datum/hallucination/fake_sound/weird/highlander",
        1.0,
        VerySpecial,
    ),
    k(
        Kind::Sound(Snd::Hyperspace),
        "/datum/hallucination/fake_sound/weird/hyperspace",
        1.0,
        Common,
    ),
    k(
        Kind::Sound(Snd::IceCrack),
        "/datum/hallucination/fake_sound/weird/ice_crack",
        0.0,
        VerySpecial,
    ),
    k(
        Kind::Sound(Snd::Laugher),
        "/datum/hallucination/fake_sound/weird/laugher",
        1.0,
        Common,
    ),
    k(
        Kind::Sound(Snd::Phone),
        "/datum/hallucination/fake_sound/weird/phone",
        1.0,
        Rare,
    ),
    k(
        Kind::Sound(Snd::RadioStatic),
        "/datum/hallucination/fake_sound/weird/radio_static",
        1.0,
        VerySpecial,
    ),
    k(
        Kind::Sound(Snd::Spell),
        "/datum/hallucination/fake_sound/weird/spell",
        1.0,
        Rare,
    ),
    k(
        Kind::Sound(Snd::JustJaunt),
        "/datum/hallucination/fake_sound/weird/spell/just_jaunt",
        1.0,
        Rare,
    ),
    k(
        Kind::Sound(Snd::Summon),
        "/datum/hallucination/fake_sound/weird/summon_sound",
        1.0,
        Rare,
    ),
    k(
        Kind::Sound(Snd::Tesloose),
        "/datum/hallucination/fake_sound/weird/tesloose",
        1.0,
        Rare,
    ),
    k(
        Kind::Sound(Snd::Xeno),
        "/datum/hallucination/fake_sound/weird/xeno",
        2.0,
        Rare,
    ),
    k(Kind::Fire, "/datum/hallucination/fire", 3.0, Uncommon),
    k(Kind::Ice, "/datum/hallucination/ice", 3.0, Common),
    k(
        Kind::Screwy(Screwy::Crit),
        "/datum/hallucination/screwy_hud/crit",
        4.0,
        Common,
    ),
    k(
        Kind::Screwy(Screwy::Dead),
        "/datum/hallucination/screwy_hud/dead",
        4.0,
        Common,
    ),
    k(
        Kind::Screwy(Screwy::Healthy),
        "/datum/hallucination/screwy_hud/healthy",
        4.0,
        Common,
    ),
    k(Kind::Shock, "/datum/hallucination/shock", 1.0, Common),
    k(
        Kind::Station(Station::Blob),
        "/datum/hallucination/station_message/blob_alert",
        1.0,
        Rare,
    ),
    k(
        Kind::Station(Station::CultSummon),
        "/datum/hallucination/station_message/cult_summon",
        1.0,
        Rare,
    ),
    k(
        Kind::Station(Station::Heretic),
        "/datum/hallucination/station_message/heretic",
        1.0,
        Rare,
    ),
    k(
        Kind::Station(Station::MalfAi),
        "/datum/hallucination/station_message/malf_ai",
        1.0,
        Rare,
    ),
    k(
        Kind::Station(Station::Meteors),
        "/datum/hallucination/station_message/meteors",
        2.0,
        Rare,
    ),
    k(
        Kind::Station(Station::ShuttleDock),
        "/datum/hallucination/station_message/shuttle_dock",
        1.0,
        Rare,
    ),
    k(
        Kind::Station(Station::SupermatterDelam),
        "/datum/hallucination/station_message/supermatter_delam",
        1.0,
        Rare,
    ),
    k(
        Kind::Telepathy,
        "/datum/hallucination/telepathy",
        4.0,
        Common,
    ),
];

impl Kind {
    pub fn def(self) -> &'static KindDef {
        TABLE
            .iter()
            .find(|d| d.kind == self)
            .expect("every kind is in the table")
    }

    /// The type named `path`.
    pub fn by_path(path: &str) -> Option<Self> {
        TABLE.iter().find(|d| d.path == path).map(|d| d.kind)
    }

    /// tg `get_random_valid_hallucination_subtype(parent)`: any ported subtype of `parent` with
    /// a weight.
    pub fn random_subtype(rng: &mut CombatRng, parent: &str) -> Option<Self> {
        let pool: Vec<Self> = TABLE
            .iter()
            .filter(|d| d.weight > 0.0 && crate::obj_damage::is_under(d.path, parent))
            .map(|d| d.kind)
            .collect();
        pick(rng, &pool).copied()
    }
}

/// `hallucination.start()`: false when it cannot happen now.
pub fn start(world: &mut World, mob: Entity, kind: Kind) -> bool {
    match kind {
        Kind::Sound(snd) => fake_sound(world, mob, snd),
        Kind::Chat => chat(world, mob, false, None),
        Kind::Message => message(world, mob),
        Kind::Telepathy => telepathy(world, mob),
        Kind::Station(station) => station_message(world, mob, station),
        Kind::Alert(alert) => fake_alert(world, mob, alert, 15.0),
        Kind::Screwy(hud) => screwy_hud(world, mob, hud),
        Kind::HealthDoll => health_doll(world, mob),
        Kind::Battle(battle) => self::battle(world, mob, battle),
        Kind::Shock => shock(world, mob),
        Kind::BloodFlow => blood_flow(world, mob),
        Kind::Ice => ice(world, mob, 6.0, true),
        Kind::Fire => fire(world, mob),
        Kind::Death => death(world, mob),
        Kind::Body(body) => self::body(world, mob, body),
    }
}

fn pick<'a, T>(rng: &mut CombatRng, items: &'a [T]) -> Option<&'a T> {
    if items.is_empty() {
        return None;
    }
    items.get(rng.range(0, items.len() as u32 - 1) as usize)
}

fn pick_str(world: &mut World, items: &[&'static str]) -> &'static str {
    let mut rng = rng(world);
    pick(&mut rng, items).copied().unwrap_or_default()
}

/// DM `rand(low, high)` on deciseconds, as seconds.
fn rand_secs(world: &mut World, low: f64, high: f64) -> f64 {
    let mut rng = rng(world);
    f64::from(rng.range((low * 10.0) as u32, (high * 10.0) as u32)) / 10.0
}

fn prob(world: &mut World, percent: f32) -> bool {
    rng(world).chance(percent)
}

/// `IS_UNCONSCIOUS`: hard crit or dead.
fn unconscious(world: &World, mob: Entity) -> bool {
    matches!(
        world.get::<Stat>(mob),
        Some(Stat::HardCrit | Stat::Dead) | None
    )
}

fn name_of(world: &World, e: Entity) -> String {
    crate::chem::name_of(world, e)
}

/// `TRAIT_DEAF` (and no ears).
fn deaf(world: &World, mob: Entity) -> bool {
    !crate::sound::can_hear(world, mob)
}

// --- Fake sounds --------------------------------------------------------------------------

/// `GLOB.creepy_ambience`.
const CREEPY: &[&str] = &[
    "sound/effects/ghost.ogg",
    "sound/effects/ghost2.ogg",
    "sound/effects/heart_beat.ogg",
    "sound/effects/screech.ogg",
    "sound/effects/hallucinations/behind_you1.ogg",
    "sound/effects/hallucinations/behind_you2.ogg",
    "sound/effects/hallucinations/far_noise.ogg",
    "sound/effects/hallucinations/growl1.ogg",
    "sound/effects/hallucinations/growl2.ogg",
    "sound/effects/hallucinations/growl3.ogg",
    "sound/effects/hallucinations/i_see_you1.ogg",
    "sound/effects/hallucinations/i_see_you2.ogg",
    "sound/effects/hallucinations/im_here1.ogg",
    "sound/effects/hallucinations/im_here2.ogg",
    "sound/effects/hallucinations/look_up1.ogg",
    "sound/effects/hallucinations/look_up2.ogg",
    "sound/effects/hallucinations/over_here1.ogg",
    "sound/effects/hallucinations/over_here2.ogg",
    "sound/effects/hallucinations/over_here3.ogg",
    "sound/effects/hallucinations/turn_around1.ogg",
    "sound/effects/hallucinations/turn_around2.ogg",
    "sound/effects/hallucinations/veryfar_noise.ogg",
    "sound/effects/hallucinations/wail.ogg",
];

const AIRLOCK_FORCED: &str = "sound/machines/airlock/airlockforced.ogg";
const MULTITOOL: &str = "sound/items/weapons/empty.ogg";
const BODYFALL: &str = "bodyfall";

/// A fake sound's `sound_type`, `volume`, `sound_vary` and `no_source`.
struct SoundDef {
    files: &'static [&'static str],
    volume: f32,
    vary: bool,
    no_source: bool,
}

const fn s(files: &'static [&'static str], volume: f32) -> SoundDef {
    SoundDef {
        files,
        volume,
        vary: true,
        no_source: false,
    }
}

const fn fixed(files: &'static [&'static str], volume: f32, no_source: bool) -> SoundDef {
    SoundDef {
        files,
        volume,
        vary: false,
        no_source,
    }
}

impl Snd {
    fn def(self) -> SoundDef {
        match self {
            Self::Airlock => s(&["sound/machines/airlock/airlock.ogg"], 30.0),
            Self::AirlockPry => s(&["sound/machines/airlock/airlock_alien_prying.ogg"], 100.0),
            Self::Console => s(&["sound/machines/terminal/terminal_prompt.ogg"], 25.0),
            Self::Boom => s(
                &[
                    "sound/effects/explosion/explosion1.ogg",
                    "sound/effects/explosion/explosion2.ogg",
                ],
                50.0,
            ),
            Self::DistantBoom => s(&["sound/effects/explosion/explosionfar.ogg"], 50.0),
            Self::Glass => s(
                &[
                    "sound/effects/glass/glassbr1.ogg",
                    "sound/effects/glass/glassbr2.ogg",
                    "sound/effects/glass/glassbr3.ogg",
                ],
                50.0,
            ),
            Self::Alarm => s(&["sound/announcer/alarm/nuke_alarm.ogg"], 70.0),
            Self::Beepsky => s(&["sound/mobs/non-humanoids/beepsky/freeze.ogg"], 35.0),
            Self::Mech => s(&["sound/vehicles/mecha/mechstep.ogg"], 40.0),
            Self::WallDeconstruction => s(&["sound/items/tools/welder.ogg"], 50.0),
            Self::DoorHacking => s(&["sound/items/tools/screwdriver.ogg"], 30.0),
            Self::Steam => s(&["sound/machines/steam_hiss.ogg"], 75.0),
            Self::Flash => s(&["sound/items/weapons/flash.ogg"], 90.0),
            // shortcut: no `STATION_TRAIT_PDA_GLITCHED` voice beeps (no station traits).
            Self::Ringtone => s(&["sound/machines/beep/twobeep_high.ogg"], 50.0),
            Self::ChimpEvent => fixed(&["sound/music/antag/monkey.ogg"], 90.0, true),
            Self::Colossus => s(&["sound/effects/magic/clockwork/invoke_general.ogg"], 50.0),
            Self::Creepy => s(CREEPY, 50.0),
            Self::Curse => fixed(&["sound/effects/magic/curse.ogg"], 40.0, true),
            Self::GameOver => fixed(
                &["sound/machines/compiler/compiler-failure.ogg"],
                50.0,
                false,
            ),
            Self::Hallelujah => fixed(&["sound/effects/pray_chaplain.ogg"], 50.0, false),
            Self::Highlander => fixed(&["sound/misc/highlander.ogg"], 50.0, true),
            Self::Hyperspace => fixed(
                &["sound/runtime/hyperspace/hyperspace_begin.ogg"],
                50.0,
                true,
            ),
            Self::Laugher => s(
                &[
                    "sound/mobs/humanoids/human/laugh/womanlaugh.ogg",
                    "sound/mobs/humanoids/human/laugh/manlaugh1.ogg",
                    "sound/mobs/humanoids/human/laugh/manlaugh2.ogg",
                ],
                50.0,
            ),
            Self::Phone => fixed(&["sound/items/weapons/ring.ogg"], 15.0, false),
            Self::Spell => s(
                &[
                    "sound/effects/magic/disintegrate.ogg",
                    "sound/effects/magic/ethereal_enter.ogg",
                    "sound/effects/magic/ethereal_exit.ogg",
                    "sound/effects/magic/fireball.ogg",
                    "sound/effects/magic/forcewall.ogg",
                    "sound/effects/magic/teleport_app.ogg",
                    "sound/effects/magic/teleport_diss.ogg",
                ],
                50.0,
            ),
            Self::JustJaunt => s(&["sound/effects/magic/ethereal_enter.ogg"], 50.0),
            Self::Summon => s(&["sound/effects/magic/castsummon.ogg"], 75.0),
            Self::Tesloose => s(&["sound/effects/magic/lightningbolt.ogg"], 35.0),
            Self::Xeno => s(
                &[
                    "sound/mobs/non-humanoids/hiss/lowHiss1.ogg",
                    "sound/mobs/non-humanoids/hiss/lowHiss2.ogg",
                    "sound/mobs/non-humanoids/hiss/lowHiss3.ogg",
                    "sound/mobs/non-humanoids/hiss/lowHiss4.ogg",
                    "sound/mobs/non-humanoids/hiss/hiss1.ogg",
                    "sound/mobs/non-humanoids/hiss/hiss2.ogg",
                    "sound/mobs/non-humanoids/hiss/hiss3.ogg",
                    "sound/mobs/non-humanoids/hiss/hiss4.ogg",
                ],
                25.0,
            ),
            Self::RadioStatic => fixed(
                &["sound/effects/hallucinations/radio_static.ogg"],
                75.0,
                true,
            ),
            Self::IceCrack => SoundDef {
                files: &["sound/effects/ice_shovel.ogg"],
                volume: 100.0,
                vary: true,
                no_source: true,
            },
        }
    }
}

/// Every sound file the ported hallucinations play (for the asset converter's list).
pub fn sound_files() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = TABLE
        .iter()
        .filter_map(|d| match d.kind {
            Kind::Sound(snd) => Some(snd.def().files),
            _ => None,
        })
        .flatten()
        .copied()
        .collect();
    out.extend(EXTRA_SOUNDS);
    out.sort_unstable();
    out.dedup();
    out
}

/// The other files of the ported hallucinations (follow-up sounds, battles, announcements).
const EXTRA_SOUNDS: &[&str] = &[
    AIRLOCK_FORCED,
    MULTITOOL,
    "sound/items/tools/welder2.ogg",
    "sound/items/tools/ratchet.ogg",
    "sound/items/weapons/taser2.ogg",
    "sound/items/weapons/tap.ogg",
    "sound/items/weapons/effects/searwall.ogg",
    "sound/items/weapons/laser.ogg",
    "sound/items/weapons/sear.ogg",
    "sound/items/weapons/egloves.ogg",
    "sound/items/weapons/cablecuff.ogg",
    "sound/items/weapons/saberon.ogg",
    "sound/items/weapons/saberoff.ogg",
    "sound/items/weapons/blade1.ogg",
    "sound/items/timer.ogg",
    "sound/effects/magic/charge.ogg",
    "sound/music/antag/heretic/ascend_blade.ogg",
    "sound/music/antag/heretic/ascend_flesh.ogg",
    "sound/music/antag/heretic/ascend_rust.ogg",
    "sound/music/antag/heretic/ascend_void.ogg",
    CULT_SUMMON_SOUND,
    "sound/effects/wounds/blood1.ogg",
    "sound/effects/wounds/blood2.ogg",
    "sound/effects/wounds/blood3.ogg",
];

/// Plays `file` to `mob` in `secs` (`queue_fake_sound`: a timer on the mob).
fn queue_sound(
    world: &mut World,
    mob: Entity,
    source: Option<TilePos>,
    file: &'static str,
    volume: f32,
    vary: bool,
    secs: f64,
) {
    later(world, secs, move |world| {
        play_local(world, mob, source, file, volume, vary);
    });
}

fn fake_sound(world: &mut World, mob: Entity, snd: Snd) -> bool {
    if deaf(world, mob) {
        return false;
    }
    let def = snd.def();
    let far = random_far_turf(world, mob);
    if snd == Snd::Mech {
        let dir = pick_dir(world);
        let steps = rng(world).range(4, 9);
        mech_walk(world, mob, far, dir, steps);
        return true;
    }
    let mut source = if def.no_source { None } else { far };
    if snd == Snd::Ringtone && prob(world, 33.0) {
        source = pos_of(world, mob);
    }
    let file = pick_str(world, def.files);
    // `phone`: the rings are queued before the first one plays.
    if snd == Snd::Phone {
        for n in 1..=3 {
            queue_sound(
                world,
                mob,
                source,
                file,
                def.volume,
                def.vary,
                2.5 * f64::from(n),
            );
        }
    }
    play_local(world, mob, source, file, def.volume, def.vary);
    match snd {
        Snd::AirlockPry => queue_sound(world, mob, source, AIRLOCK_FORCED, 50.0, true, 5.0),
        Snd::WallDeconstruction => {
            let v = def.volume;
            queue_sound(
                world,
                mob,
                source,
                "sound/items/tools/welder2.ogg",
                v,
                true,
                10.5,
            );
            queue_sound(
                world,
                mob,
                source,
                "sound/items/tools/ratchet.ogg",
                v,
                true,
                12.0,
            );
        }
        Snd::DoorHacking => {
            let v = def.volume;
            let hacking = rand_secs(world, 4.0, 8.0);
            queue_sound(world, mob, source, MULTITOOL, v, true, 0.8);
            if hacking > 4.5 {
                queue_sound(world, mob, source, MULTITOOL, v, true, 3.0);
                if prob(world, 50.0) {
                    queue_sound(world, mob, source, MULTITOOL, v, true, 3.5);
                }
            }
            if hacking > 5.5 {
                queue_sound(world, mob, source, MULTITOOL, v, true, 5.0);
            }
            queue_sound(world, mob, source, AIRLOCK_FORCED, v, true, hacking);
        }
        Snd::Tesloose => {
            let shocks = rng(world).range(2, 4);
            for n in 1..=shocks {
                let volume = def.volume + 15.0 * n as f32;
                queue_sound(world, mob, source, file, volume, true, 3.0 * f64::from(n));
            }
        }
        _ => {}
    }
    true
}

fn pick_dir(world: &mut World) -> Dir {
    let mut rng = rng(world);
    *pick(&mut rng, &Dir::CARDINALS).expect("four cardinals")
}

/// `mech/mech_walk`: a step every second, turning a quarter of the time.
fn mech_walk(world: &mut World, mob: Entity, source: Option<TilePos>, dir: Dir, steps: u32) {
    if !exists(world, mob) {
        return;
    }
    let def = Snd::Mech.def();
    play_local(world, mob, source, def.files[0], def.volume, def.vary);
    let (source, dir) = if prob(world, 75.0) {
        (source.map(|p| p.step(dir)), dir)
    } else {
        (source, pick_dir(world))
    };
    if steps > 1 {
        later(world, 1.0, move |world| {
            mech_walk(world, mob, source, dir, steps - 1);
        });
    }
}

// --- Chat, messages, telepathy ------------------------------------------------------------

/// tg `say_mod` with BandaStation's verbs.
fn say_verb(text: &str) -> &'static str {
    if text.ends_with("!!") {
        "кричит"
    } else if text.ends_with('?') {
        "спрашивает"
    } else if text.ends_with('!') {
        "восклицает"
    } else {
        "говорит"
    }
}

/// Player humans with a job (tg `get_crewmember_minds()`), by entity order.
fn crew(world: &mut World) -> Vec<Entity> {
    let mut query = world.query_filtered::<Entity, (
        With<crate::server::PlayerMob>,
        With<crate::jobs::PlayerJob>,
        With<crate::human::HumanLook>,
    )>();
    let mut out: Vec<Entity> = query.iter(world).collect();
    out.sort();
    out
}

fn shuffle<T>(rng: &mut CombatRng, items: &mut [T]) {
    for i in (1..items.len()).rev() {
        let j = rng.range(0, i as u32) as usize;
        items.swap(i, j);
    }
}

/// `/datum/hallucination/chat`: a nearby human (or a corpse) says something, else a crew
/// member on the common channel.
// shortcut: no languages (everyone speaks and understands the common one), no runechat.
pub fn chat(world: &mut World, mob: Entity, force_radio: bool, specific: Option<String>) -> bool {
    if unconscious(world, mob) {
        return false;
    }
    let mut rng = rng(world);
    let mut speaker = None;
    if !force_radio {
        let nearby = humans_in_view(world, mob, 7);
        let (mut living, corpses): (Vec<Entity>, Vec<Entity>) = nearby
            .iter()
            .map(|(e, _)| *e)
            .partition(|&e| world.get::<Stat>(e) != Some(&Stat::Dead));
        shuffle(&mut rng, &mut living);
        speaker = living
            .first()
            .copied()
            .or_else(|| pick(&mut rng, &corpses).copied());
    }
    let radio = force_radio || speaker.is_none();
    if radio {
        let mut crew = crew(world);
        crew.retain(|&e| e != mob);
        speaker = pick(&mut rng, &crew).copied();
    }
    let Some(speaker) = speaker else {
        return false;
    };
    let target = first_name(&name_of(world, mob)).to_owned();
    let text = match specific {
        Some(text) => text,
        None if radio => {
            let options = [
                "Help!".to_owned(),
                format!(
                    "Help {}{}",
                    pick_line(&mut rng, "location"),
                    if rng.chance(50.0) { "!" } else { "!!" }
                ),
                format!(
                    "{} is {}!",
                    pick_line(&mut rng, "people"),
                    pick_line(&mut rng, "accusations")
                ),
                format!(
                    "{} has {}!",
                    pick_line(&mut rng, "people"),
                    pick_line(&mut rng, "contraband")
                ),
                format!(
                    "{} in {}{}",
                    pick_line(&mut rng, "threat"),
                    pick_line(&mut rng, "location"),
                    if rng.chance(50.0) { "!" } else { "!!" }
                ),
                if rng.chance(50.0) {
                    format!("Where's {target}?")
                } else {
                    format!("Set {target} to arrest!")
                },
                format!(
                    "{}all the shuttle!",
                    pick(&mut rng, &["C", "Ai, c", "Someone c", "Rec"])
                        .copied()
                        .unwrap_or("C")
                ),
                format!(
                    "AI {}!!",
                    if rng.chance(50.0) { "rogue" } else { "is dead" }
                ),
                "Borgs rogue!".to_owned(),
            ];
            ss13_chat::speech::capitalize(&pick(&mut rng, &options).cloned().unwrap_or_default())
        }
        None => {
            let options = [
                pick_line(&mut rng, "suspicion"),
                pick_line(&mut rng, "conversation"),
                format!("{}{target}!", pick_line(&mut rng, "greetings")),
                pick_line(&mut rng, "getout"),
                pick_line(&mut rng, "weird"),
                pick_line(&mut rng, "didyouhearthat"),
                pick_line(&mut rng, "doubt"),
                pick_line(&mut rng, "aggressive"),
                format!("{}!!", pick_line(&mut rng, "help")),
                pick_line(&mut rng, "escape"),
                format!("I'm infected, {}!", pick_line(&mut rng, "infection_advice")),
            ];
            ss13_chat::speech::capitalize(&pick(&mut rng, &options).cloned().unwrap_or_default())
        }
    };
    let text = text.replace("%TARGETNAME%", &target);
    let name = name_of(world, speaker);
    let message = if radio {
        ss13_chat::radio::radio_message(
            ss13_chat::radio::FREQ_COMMON,
            &name,
            say_verb(&text),
            &text,
            false,
        )
    } else {
        // The speech bubble over the speaker (`flick_overlay_global`, 3 s).
        let bubble = show_on(
            world,
            mob,
            speaker,
            ImageLook::new("icons/mob/effects/talk.dmi", "default0", ABOVE_MOB_LAYER),
        );
        hide_later(world, bubble, 3.0);
        ss13_chat::server::say_message(&name, &text)
    };
    tell_message(world, mob, message);
    true
}

/// `/datum/hallucination/message`: a weighted fake line about someone nearby or the body.
fn message(world: &mut World, mob: Entity) -> bool {
    if unconscious(world, mob) {
        return false;
    }
    let Some(at) = pos_of(world, mob) else {
        return false;
    };
    let mut rng = rng(world);
    let nearby = humans_in_view(world, mob, 7);
    let adjacent = nearby
        .iter()
        .find(|(_, p)| p.chebyshev(at).is_some_and(|d| d <= 1))
        .map(|(e, _)| *e);
    let suspect = adjacent.or_else(|| {
        let all: Vec<Entity> = nearby.iter().map(|(e, _)| *e).collect();
        pick(&mut rng, &all).copied()
    });
    let mut pool: Vec<(ChatMessage, u32)> = Vec::new();
    let info = |span, text: String| line(ChatChannel::Info, span, text);
    let warn = |text: String| line(ChatChannel::Warning, Span::Warning, text);
    if let Some(suspect) = suspect {
        let name = name_of(world, suspect);
        if adjacent.is_some() {
            pool.push((warn("You feel a tiny prick!".into()), 5));
        }
        let pack = world
            .get::<crate::inventory::Inventory>(suspect)
            .and_then(|i| i.back)
            .filter(|&b| world.get::<crate::storage::Storage>(b).is_some());
        if let Some(pack) = pack {
            const STASH: &[&str] = &[
                "/obj/item/blueprints",
                "/obj/item/assembly/flash",
                "/obj/item/card/id/advanced/gold/captains_spare",
                "/obj/item/card/emag",
                "/obj/item/circular_saw",
                "/obj/item/codex_cicatrix",
                "/obj/item/grenade/c4",
                "/obj/item/gun/ballistic/revolver",
                "/obj/item/gun/energy/e_gun/hos",
                "/obj/item/gun/energy/laser/captain",
                "/obj/item/gun/energy/recharge/ebow",
                "/obj/item/gun/syringe/syndicate",
                "/obj/item/hand_tele",
                "/obj/item/melee/baton/security",
                "/obj/item/melee/cultblade/dagger",
                "/obj/item/melee/energy",
                "/obj/item/powersink",
                "/obj/item/reagent_containers/hypospray/cmo",
                "/obj/item/spellbook",
            ];
            let item = pick(&mut rng, STASH).copied().unwrap_or(STASH[0]);
            let item = crate::machine_frame::type_name(world, item);
            let pack = name_of(world, pack);
            pool.push((
                info(Span::Notice, format!("{name} puts the {item} into {pack}.")),
                5,
            ));
        }
        let verb = if rng.chance(50.0) {
            "sneezes"
        } else {
            "coughs"
        };
        pool.push((
            ChatMessage {
                channel: ChatChannel::Local,
                segments: vec![
                    ChatSegment::new(Span::Bold, name),
                    ChatSegment::new(Span::Plain, format!(" {verb}.")),
                ],
            },
            1,
        ));
    }
    pool.push((
        info(
            Span::Notice,
            "You hear something squeezing through the ducts...".into(),
        ),
        1,
    ));
    let part = pick(&mut rng, &["arm", "leg", "back", "head"])
        .copied()
        .unwrap_or("arm");
    pool.push((warn(format!("Your {part} itches.")), 1));
    let feel = pick(&mut rng, &["hot", "cold", "dry", "wet", "woozy", "faint"])
        .copied()
        .unwrap_or("hot");
    pool.push((warn(format!("You feel {feel}.")), 1));
    pool.push((warn("Your stomach rumbles.".into()), 1));
    pool.push((warn("Your head hurts.".into()), 1));
    pool.push((warn("You hear a faint buzz in your head.".into()), 1));
    if rng.chance(10.0) {
        pool.push((warn("Behind you.".into()), 1));
        pool.push((warn("You hear a faint laughter.".into()), 1));
        pool.push((warn("You hear skittering on the ceiling.".into()), 1));
        pool.push((
            warn("You see an inhumanly tall silhouette moving in the distance.".into()),
            2,
        ));
    }
    if rng.chance(30.0) {
        pool.push((info(Span::Plain, pick_line(&mut rng, "advice")), 4));
    }
    let total: u32 = pool.iter().map(|p| p.1).sum();
    let mut roll = rng.range(1, total);
    let chosen = pool
        .into_iter()
        .find(|(_, w)| {
            if roll <= *w {
                return true;
            }
            roll -= w;
            false
        })
        .map(|(m, _)| m);
    if let Some(chosen) = chosen {
        tell_message(world, mob, chosen);
    }
    true
}

/// `/datum/hallucination/telepathy`: a voice in the head (the telepathy spell's spans).
fn telepathy(world: &mut World, mob: Entity) -> bool {
    let mut rng = rng(world);
    let memo = if rng.chance(0.001) {
        "horse".to_owned()
    } else {
        let key = pick(
            &mut rng,
            &[
                "advice",
                "aggressive",
                "conversation",
                "didyouhearthat",
                "doubt",
                "escape",
                "getout",
                "greetings",
                "suspicion",
            ],
        )
        .copied()
        .unwrap_or("advice");
        let name = name_of(world, mob);
        let names = [first_name(&name), last_name(&name)];
        let name = pick(&mut rng, &names).copied().unwrap_or_default();
        pick_line(&mut rng, key).replace("%TARGETNAME%", name)
    };
    tell_message(
        world,
        mob,
        ChatMessage {
            channel: ChatChannel::Info,
            segments: vec![
                ChatSegment::new(Span::BoldNotice, "You hear a voice in your head..."),
                ChatSegment::new(Span::Notice, format!(" {memo}")),
            ],
        },
    );
    true
}

// --- Station messages ---------------------------------------------------------------------

const CULT_SUMMON_SOUND: &str =
    "modular_bandastation/cult_overhaul/sound/announcements/cult_summon.ogg";

/// `random_non_sec_crewmember()`: a crew human 8+ tiles away, not in security.
fn random_non_sec_crewmember(world: &mut World, mob: Entity) -> Option<Entity> {
    let at = pos_of(world, mob);
    let crew: Vec<Entity> = crew(world)
        .into_iter()
        .filter(|&e| e != mob && !is_security(world, e))
        .filter(|&e| match (pos_of(world, e), at) {
            (Some(p), Some(at)) => p.chebyshev(at).is_none_or(|d| d >= 8),
            _ => true,
        })
        .collect();
    let mut rng = rng(world);
    pick(&mut rng, &crew).copied()
}

fn is_security(world: &World, mob: Entity) -> bool {
    let (Some(job), Some(jobs)) = (
        world.get::<crate::jobs::PlayerJob>(mob),
        world.get_resource::<crate::jobs::Jobs>(),
    ) else {
        return false;
    };
    jobs.data
        .departments
        .iter()
        .any(|d| d.path == "/datum/job_department/security" && d.jobs.contains(&(job.0 as u16)))
}

/// `generate_heretic_text()`.
fn heretic_text(rng: &mut CombatRng) -> String {
    const CHARS: [char; 10] = ['!', '$', '^', '@', '&', '#', '*', '(', ')', '?'];
    (0..25)
        .map(|_| CHARS[rng.range(0, CHARS.len() as u32 - 1) as usize])
        .collect()
}

fn station_message(world: &mut World, mob: Entity, kind: Station) -> bool {
    use crate::announce::Priority;
    // `require_hearing` (TRUE on the base type too).
    if deaf(world, mob) {
        return false;
    }
    let players = vec![mob];
    match kind {
        Station::Blob => Priority::new(
            format!(
                "Вспышка биологической угрозы 5-го уровня зафиксирована на борту {}. Всему \
                 персоналу надлежит сдержать её распространение любой ценой!",
                crate::events::station_name(world)
            ),
            "Биологическая угроза",
        )
        .sound("announcer_outbreak5")
        .players(players)
        .send(world),
        Station::ShuttleDock => {
            let name = world
                .get_resource::<crate::shuttle::EmergencyShuttle>()
                .map(|s| s.name.clone())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "Эвакуационный шаттл".to_owned());
            let dock = world
                .get_resource::<crate::shuttle::ShuttleConfig>()
                .map_or(180.0, |c| c.dock_secs);
            Priority::new(
                format!(
                    "{name} совершил стыковку со станцией. У вас есть {}, чтобы добраться до \
                     эвакуационного шаттла.",
                    crate::jobs::display_time_text(dock)
                ),
                "Прибытие эвакуационного шаттла",
            )
            .sound(crate::announce::ANNOUNCER_SHUTTLEDOCK)
            .sender("Система оповещения эвакуационного шаттла")
            .players(players)
            .send(world);
        }
        Station::MalfAi => {
            let mut ais = world.query::<&crate::silicon::ai::AiCore>();
            if ais.iter(world).next().is_none() {
                return false;
            }
            Priority::new(
                "Во всех системах станций обнаружены вредоносные процессы. Пожалуйста, \
                 уничтожьте свой ИИ, чтобы предотвратить возможный ущерб его моральному ядру.",
                "Обнаружена аномалия",
            )
            .sound("announcer_aimalf")
            .players(players)
            .send(world);
        }
        Station::Heretic => {
            const ASCENSIONS: [(&str, &str); 5] = [
                (
                    "Бойтесь пламени, ибо Пепельный Лорд %FAKENAME% вознесся! Пламя поглотит всех!",
                    "sound/music/antag/heretic/ascend_blade.ogg",
                ),
                (
                    "Мастер клинков %FAKENAME%, ученик Чемпиона Растерзаний, вознесся! Их сталь — \
                     та, что рассечет реальность в серебряном шторме!",
                    "sound/music/antag/heretic/ascend_blade.ogg",
                ),
                (
                    "Вечно бушующий вихрь. Реальность раскрылась. РУКИ ПРОТЯНУТЫ, ВЛАДЫКА НОЧИ \
                     %FAKENAME% ВОЗНЕССЯ! Бойтесь непрестанно извивающейся руки!",
                    "sound/music/antag/heretic/ascend_flesh.ogg",
                ),
                (
                    "Бойтесь разложения, ибо Предводитель Ржавчины, %FAKENAME%, вознесся! Никто \
                     не уйдет от коррозии!",
                    "sound/music/antag/heretic/ascend_rust.ogg",
                ),
                (
                    "Аристократ пустоты %FAKENAME% ступил в Вальс, завершающий миры!",
                    "sound/music/antag/heretic/ascend_void.ogg",
                ),
            ];
            let Some(heretic) = random_non_sec_crewmember(world, mob) else {
                return false;
            };
            let name = name_of(world, heretic);
            let mut rng = rng(world);
            let (text, sound) = *pick(&mut rng, &ASCENSIONS).expect("five ascensions");
            let text = format!(
                "{} {} {}",
                heretic_text(&mut rng),
                text.replace("%FAKENAME%", &name),
                heretic_text(&mut rng)
            );
            Priority::new(text, heretic_text(&mut rng))
                .sound(sound)
                .players(players)
                .send(world);
        }
        Station::CultSummon => {
            let Some(leader) = random_non_sec_crewmember(world, mob) else {
                return false;
            };
            let Some(area) = fake_summon_area(world, mob) else {
                return false;
            };
            Priority::new(
                format!(
                    "Зафиксирован призыв древнего божества в {area}, культист-призыватель - {}. \
                     Прервите ритуал любой ценой!",
                    name_of(world, leader)
                ),
                format!(
                    "{}: Отдел паранормальных явлений",
                    crate::events::COMMAND_NAME
                ),
            )
            .sound(CULT_SUMMON_SOUND)
            .important()
            .players(players)
            .send(world);
        }
        Station::Meteors => Priority::new(
            "Зафиксировано движение астероидов на встречном со станцией курсе.",
            "Астероиды",
        )
        .sound(crate::announce::ANNOUNCER_METEORS)
        .players(players)
        .send(world),
        Station::SupermatterDelam => {
            play_local(
                world,
                mob,
                None,
                "sound/effects/magic/charge.ogg",
                100.0,
                false,
            );
            tell(
                world,
                mob,
                Span::BoldWarning,
                "You feel reality distort for a moment...",
            );
        }
    }
    true
}

/// A station area of another type than the hallucinator's (`GLOB.the_station_areas -
/// hallucinator_area.type`), by name.
fn fake_summon_area(world: &mut World, mob: Entity) -> Option<String> {
    let own = pos_of(world, mob)
        .and_then(|p| crate::events::area_type_at(world, p))
        .map(str::to_owned);
    let areas = world.get_resource::<ss13_mapload::Areas>()?;
    let mut names: Vec<String> = areas
        .entities()
        .iter()
        .filter(|&&a| {
            crate::events::area_type(world, a).is_some_and(|t| {
                crate::obj_damage::is_under(t, "/area/station") && Some(t) != own.as_deref()
            })
        })
        .map(|&a| crate::shuttle::area_name(world, a))
        .filter(|n| !n.is_empty())
        .collect();
    names.sort();
    names.dedup();
    let mut rng = rng(world);
    pick(&mut rng, &names).cloned()
}

// --- Fake HUD -----------------------------------------------------------------------------

/// The fake alerts' categories (tg's `ALERT_*` of each type).
pub mod alert {
    pub const NOT_ENOUGH_OXYGEN: u8 = 0;
    pub const NOT_ENOUGH_PLASMA: u8 = 1;
    pub const NOT_ENOUGH_CO2: u8 = 2;
    pub const TOO_MUCH_OXYGEN: u8 = 3;
    pub const TOO_MUCH_PLASMA: u8 = 4;
    pub const TOO_MUCH_CO2: u8 = 5;
    pub const GRAVITY: u8 = 6;
    pub const FIRE: u8 = 7;
    pub const TEMPERATURE: u8 = 8;
    pub const PRESSURE: u8 = 9;
    pub const NEW_LAW: u8 = 10;
    pub const LOCKED: u8 = 11;
    pub const HACKED: u8 = 12;
    pub const CHARGE: u8 = 13;
}

impl FakeAlert {
    /// `alert_category`, the alert types' `icon_state`s, `optional_severity`.
    fn def(self) -> (u8, &'static [&'static str], u8) {
        use alert::*;
        match self {
            Self::NeedOxygen => (NOT_ENOUGH_OXYGEN, &["not_enough_oxy"], 0),
            Self::NeedPlasma => (NOT_ENOUGH_PLASMA, &["not_enough_plas"], 0),
            Self::NeedCo2 => (NOT_ENOUGH_CO2, &["not_enough_co2"], 0),
            Self::BadOxygen => (TOO_MUCH_OXYGEN, &["too_much_oxy"], 0),
            Self::BadPlasma => (TOO_MUCH_PLASMA, &["too_much_plas"], 0),
            Self::BadCo2 => (TOO_MUCH_CO2, &["too_much_co2"], 0),
            Self::Gravity => (GRAVITY, &["weightless"], 0),
            Self::Fire => (FIRE, &["fire"], 0),
            Self::Hot => (TEMPERATURE, &["hot"], 3),
            Self::Cold => (TEMPERATURE, &["cold"], 3),
            Self::Pressure => (PRESSURE, &["highpressure", "lowpressure"], 2),
            Self::Law => (NEW_LAW, &["newlaw"], 0),
            Self::Locked => (LOCKED, &["locked"], 0),
            Self::Hacked => (HACKED, &["hacked"], 0),
            Self::NeedCharge => (CHARGE, &["empty_cell"], 0),
        }
    }
}

/// `/datum/hallucination/fake_alert`: the alert for `secs`, then its category is cleared.
pub fn fake_alert(world: &mut World, mob: Entity, kind: FakeAlert, secs: f64) -> bool {
    let (category, states, severity) = kind.def();
    let state = pick_str(world, states);
    throw_fake_alert(world, mob, category, state, severity);
    later(world, secs, move |world| {
        clear_fake_alert(world, mob, category)
    });
    true
}

fn screwy_hud(world: &mut World, mob: Entity, kind: Screwy) -> bool {
    let bump = move |hud: &mut super::ScrewyHud, up: bool| {
        let count = match kind {
            Screwy::Crit => &mut hud.crit,
            Screwy::Dead => &mut hud.dead,
            Screwy::Healthy => &mut hud.healthy,
        };
        *count = if up {
            count.saturating_add(1)
        } else {
            count.saturating_sub(1)
        };
    };
    screwy(world, mob, |hud| bump(hud, true));
    let secs = rand_secs(world, 10.0, 25.0);
    later(world, secs, move |world| {
        screwy(world, mob, |hud| bump(hud, false))
    });
    true
}

/// `/datum/hallucination/fake_health_doll`: one limb looks hurt (severity 1-5) for 50 s.
fn health_doll(world: &mut World, mob: Entity) -> bool {
    if world.get::<crate::human::HumanLook>(mob).is_none() {
        return false;
    }
    let mut rng = rng(world);
    let zone = *pick(&mut rng, &BodyZone::ALL).expect("six zones");
    let severity = rng.range(1, 5) as u8;
    let i = super::doll_index(zone);
    screwy(world, mob, |hud| hud.doll[i] = severity);
    later(world, 50.0, move |world| {
        screwy(world, mob, |hud| hud.doll[i] = 0)
    });
    true
}

// --- Battles ------------------------------------------------------------------------------

/// `CLICK_CD_RANGE`, `CLICK_CD_MELEE` in deciseconds.
const CLICK_CD_RANGE: f64 = 4.0;
const CLICK_CD_MELEE: f64 = 8.0;

/// A gun battle: shots, fire, hit sounds, hits to end, chance to fall.
#[derive(Clone, Copy)]
struct Gun {
    shots: (u32, u32),
    fire: &'static str,
    hit: &'static str,
    wall: &'static str,
    hits_to_end: u32,
    fall: f32,
}

fn battle(world: &mut World, mob: Entity, kind: Battle) -> bool {
    if deaf(world, mob) {
        return false;
    }
    let source = random_far_turf(world, mob);
    match kind {
        Battle::Disabler | Battle::Laser => {
            let gun = if kind == Battle::Disabler {
                Gun {
                    shots: (5, 10),
                    fire: "sound/items/weapons/taser2.ogg",
                    hit: "sound/items/weapons/tap.ogg",
                    wall: "sound/items/weapons/effects/searwall.ogg",
                    hits_to_end: 3,
                    fall: 70.0,
                }
            } else {
                Gun {
                    shots: (5, 10),
                    fire: "sound/items/weapons/laser.ogg",
                    hit: "sound/items/weapons/sear.ogg",
                    wall: "sound/items/weapons/effects/searwall.ogg",
                    hits_to_end: 4,
                    fall: 70.0,
                }
            };
            let shots = rng(world).range(gun.shots.0, gun.shots.1) as i32;
            if let Some(source) = source {
                fire_loop(world, mob, gun, source, shots, 0);
            }
        }
        Battle::StunProd | Battle::HarmBaton => {
            play_local(
                world,
                mob,
                source,
                "sound/items/weapons/egloves.ogg",
                40.0,
                true,
            );
            play_local(world, mob, source, BODYFALL, 25.0, true);
            let Some(source) = source else {
                return true;
            };
            if kind == Battle::StunProd {
                later(world, 2.0, move |world| {
                    let file = "sound/items/weapons/cablecuff.ogg";
                    play_local(world, mob, Some(source), file, 15.0, true);
                });
            } else {
                let hits = rng(world).range(5, 12);
                later(world, 2.0, move |world| harmbaton(world, mob, source, hits));
            }
        }
        Battle::ESword => {
            play_local(
                world,
                mob,
                source,
                "sound/items/weapons/saberon.ogg",
                15.0,
                true,
            );
            if let Some(source) = source {
                let stabs = rng(world).range(4, 8);
                later(world, CLICK_CD_MELEE / 10.0, move |world| {
                    stab_loop(world, mob, source, stabs);
                });
            }
        }
        Battle::Bomb => {
            let ticks = rng(world).range(3, 11);
            if let Some(source) = source {
                later(world, 1.5, move |world| {
                    bomb_tick(world, mob, source, ticks)
                });
            }
        }
    }
    true
}

/// `battle/gun/fire_loop`.
fn fire_loop(world: &mut World, mob: Entity, gun: Gun, source: TilePos, shots: i32, hits: u32) {
    if !exists(world, mob) {
        return;
    }
    let src = Some(source);
    play_local(world, mob, src, gun.fire, 25.0, true);
    let next = rand_secs(world, 0.5, 1.0);
    let mut hits = hits;
    if prob(world, 50.0) {
        queue_sound(world, mob, src, gun.hit, 25.0, true, next);
        hits += 1;
    } else {
        queue_sound(world, mob, src, gun.wall, 25.0, true, next);
    }
    if hits >= gun.hits_to_end && prob(world, gun.fall) {
        queue_sound(world, mob, src, BODYFALL, 25.0, true, next);
    } else if shots >= 0 {
        let delay = rand_secs(world, CLICK_CD_RANGE / 10.0, (CLICK_CD_RANGE + 6.0) / 10.0);
        later(world, delay, move |world| {
            fire_loop(world, mob, gun, source, shots - 1, hits);
        });
    }
}

fn harmbaton(world: &mut World, mob: Entity, source: TilePos, hits: u32) {
    if !exists(world, mob) {
        return;
    }
    play_local(world, mob, Some(source), "swing_hit", 50.0, true);
    if hits > 1 {
        let delay = rand_secs(world, CLICK_CD_MELEE / 10.0, (CLICK_CD_MELEE + 4.0) / 10.0);
        later(world, delay, move |world| {
            harmbaton(world, mob, source, hits - 1)
        });
    }
}

fn stab_loop(world: &mut World, mob: Entity, source: TilePos, stabs: u32) {
    if !exists(world, mob) {
        return;
    }
    let src = Some(source);
    if stabs == 0 {
        play_local(
            world,
            mob,
            src,
            "sound/items/weapons/saberoff.ogg",
            15.0,
            true,
        );
        return;
    }
    play_local(
        world,
        mob,
        src,
        "sound/items/weapons/blade1.ogg",
        50.0,
        true,
    );
    if stabs == 4 {
        play_local(world, mob, src, BODYFALL, 25.0, true);
    }
    let delay = rand_secs(world, CLICK_CD_MELEE / 10.0, (CLICK_CD_MELEE + 6.0) / 10.0);
    later(world, delay, move |world| {
        stab_loop(world, mob, source, stabs - 1)
    });
}

fn bomb_tick(world: &mut World, mob: Entity, source: TilePos, ticks: u32) {
    if !exists(world, mob) {
        return;
    }
    play_local(
        world,
        mob,
        Some(source),
        "sound/items/timer.ogg",
        25.0,
        false,
    );
    if ticks > 1 {
        later(world, 1.5, move |world| {
            bomb_tick(world, mob, source, ticks - 1)
        });
    }
}

// --- The body -----------------------------------------------------------------------------

/// `/datum/hallucination/shock`: a fake electrocution.
fn shock(world: &mut World, mob: Entity) -> bool {
    let human = world.get::<crate::human::HumanLook>(mob).is_some();
    let state = if human {
        "electrocuted_base"
    } else {
        "electrocuted_generic"
    };
    tell(
        world,
        mob,
        Span::BoldWarning,
        "You feel a powerful shock course through your body!",
    );
    if let Some(at) = pos_of(world, mob) {
        let clients = crate::chem::clients_near(world, at, Some(mob));
        if !clients.is_empty() {
            let name = name_of(world, mob);
            world.write_message(SendChat {
                clients,
                message: line(
                    ChatChannel::Warning,
                    Span::Warning,
                    format!("{name} falls to the ground, shaking!"),
                ),
            });
        }
    }
    let skeleton = show_on(
        world,
        mob,
        mob,
        ImageLook::new("icons/mob/human/human.dmi", state, ABOVE_MOB_LAYER),
    );
    hide_later(world, skeleton, 4.0);
    // tg plays it at `get_turf(src)` of the datum (null): the intent is the hallucinator's tile.
    let at = pos_of(world, mob);
    play_local(world, mob, at, "sparks", 100.0, true);
    crate::status_effects::adjust_stamina(world, mob, 50.0);
    crate::status_effects::apply(world, mob, Incap::Stun, 4.0);
    later(world, 2.0, move |world| {
        if exists(world, mob) {
            crate::status_effects::apply(world, mob, Incap::Paralyze, 6.0);
        }
    });
    true
}

/// `plaintext_zone` (tg English).
fn zone_text(zone: BodyZone) -> &'static str {
    match zone {
        BodyZone::Head => "head",
        BodyZone::Chest => "chest",
        BodyZone::LArm => "left arm",
        BodyZone::RArm => "right arm",
        BodyZone::LLeg => "left leg",
        BodyZone::RLeg => "right leg",
    }
}

/// `/datum/hallucination/blood_flow`: a limb seems to spray blood for 16-40 s.
fn blood_flow(world: &mut World, mob: Entity) -> bool {
    if client_of(world, mob).is_none()
        || world.get::<crate::human::HumanLook>(mob).is_none()
        || world.get::<crate::medical::blood::Blood>(mob).is_none()
    {
        return false;
    }
    let mut rng = rng(world);
    let zone = *pick(&mut rng, &BodyZone::ALL).expect("six zones");
    let name = zone_text(zone);
    tell(
        world,
        mob,
        Span::Warning,
        &format!("Your {name} looses a spray of blood!"),
    );
    let duration = f64::from(rng.range(160, 400)) / 10.0;
    if rng.chance(25.0) {
        let at = duration * if rng.chance(50.0) { 0.5 } else { 0.66 };
        later(world, at, move |world| {
            tell(
                world,
                mob,
                Span::Warning,
                &format!("The blood doesn't stop flowing, yet {name} doesn't seem to hurt..."),
            );
        });
    }
    let sound = *pick(
        &mut rng,
        &[
            "sound/effects/wounds/blood1.ogg",
            "sound/effects/wounds/blood2.ogg",
            "sound/effects/wounds/blood3.ogg",
        ],
    )
    .expect("three sounds");
    let at = pos_of(world, mob);
    play_local(world, mob, at, sound, 50.0, true);
    // `"[body_zone]_[pick(2, 3)]"`.
    let states = match zone {
        BodyZone::Head => ["head_2", "head_3"],
        BodyZone::Chest => ["chest_2", "chest_3"],
        BodyZone::LArm => ["l_arm_2", "l_arm_3"],
        BodyZone::RArm => ["r_arm_2", "r_arm_3"],
        BodyZone::LLeg => ["l_leg_2", "l_leg_3"],
        BodyZone::RLeg => ["r_leg_2", "r_leg_3"],
    };
    let state = states[rng.range(0, 1) as usize];
    let mut look = ImageLook::new(
        "icons/mob/effects/bleed_overlays.dmi",
        state,
        ABOVE_MOB_LAYER,
    );
    // `BLOOD_COLOR_RED`.
    look.color = Some([0xaa, 0x10, 0x10, 0xff]);
    let image = show_on(world, mob, mob, look);
    let ends = super::now(world) + duration;
    bleed_stamina(world, mob, ends);
    later(world, duration, move |world| {
        if let Some(image) = image {
            hide_image(world, image);
        }
        if exists(world, mob) {
            tell(
                world,
                mob,
                Span::Warning,
                &format!("Your {name} stops bleeding."),
            );
        }
    });
    true
}

/// `stamina_loop`: 5 stamina damage every 4 s while it bleeds.
fn bleed_stamina(world: &mut World, mob: Entity, ends: f64) {
    if !exists(world, mob) || super::now(world) >= ends {
        return;
    }
    crate::status_effects::adjust_stamina(world, mob, 5.0);
    later(world, 4.0, move |world| bleed_stamina(world, mob, ends));
}

/// `/datum/hallucination/ice`: frozen in a cube for `secs`.
pub fn ice(world: &mut World, mob: Entity, secs: f64, play_sound: bool) -> bool {
    let cube = show_on(
        world,
        mob,
        mob,
        ImageLook::new("icons/effects/freeze.dmi", "ice_cube", ABOVE_MOB_LAYER),
    );
    // shortcut: `TRAIT_IMMOBILIZED` for the duration as an immobilization (no traits by source).
    crate::status_effects::apply(world, mob, Incap::Immobilize, secs as f32);
    tell(
        world,
        mob,
        Span::BoldWarning,
        "You become frozen in a cube!",
    );
    fake_alert(world, mob, FakeAlert::Cold, secs + 6.0);
    if play_sound {
        super::cause_hallucination(world, mob, Kind::Sound(Snd::IceCrack), "ice hallucination");
    }
    hide_later(world, cube, secs);
    true
}

/// `/datum/hallucination/fire` state (`SSfastprocess`, 0.2 s).
#[derive(Clone, Copy)]
struct FakeFire {
    stacks: f32,
    stage: i32,
    next_action: f32,
    stamina_left: u32,
    clearing: bool,
    increasing: bool,
    time_spent: f32,
    active: bool,
    image: Option<Entity>,
    big: bool,
}

const ONFIRE_DMI: &str = "icons/mob/effects/onfire.dmi";
/// `MOB_BIG_FIRE_STACK_THRESHOLD`.
const BIG_FIRE_STACKS: f32 = 3.0;
const FAST_PROCESS: f32 = 0.2;
const FAKE_FIRE_MOOD: crate::mood::MoodEvent = crate::mood::MoodEvent {
    id: "fake_fire",
    description: "Oh... I was just imagining those flames.",
    mood_change: 4.0,
    timeout: 30.0,
    hidden: false,
};

fn fire_look(big: bool) -> ImageLook {
    let state = if big {
        "human_big_fire"
    } else {
        "human_small_fire"
    };
    ImageLook::new(ONFIRE_DMI, state, ABOVE_MOB_LAYER)
}

fn fire(world: &mut World, mob: Entity) -> bool {
    let mut rng = rng(world);
    let stacks = rng.range(5, 15) as f32;
    let big = stacks > BIG_FIRE_STACKS;
    let image = show_on(world, mob, mob, fire_look(big));
    tell(world, mob, Span::BoldWarning, "You're set on fire!");
    crate::mood::add_event(world, mob, "on_fire", &crate::mood::events::ON_FIRE);
    throw_fake_alert(world, mob, alert::FIRE, "fire", 0);
    let state = FakeFire {
        stacks,
        stage: 0,
        next_action: 0.0,
        stamina_left: rng.range(5, 10),
        clearing: false,
        increasing: true,
        time_spent: 0.0,
        active: true,
        image,
        big,
    };
    later(world, 2.0, move |world| fire_step(world, mob, state));
    true
}

/// `fire/process(seconds_per_tick)`.
fn fire_step(world: &mut World, mob: Entity, mut st: FakeFire) {
    if !exists(world, mob) {
        return;
    }
    let dt = FAST_PROCESS;
    st.stacks -= 0.25 * dt;
    if st.stacks <= 0.0 {
        clear_fire(world, mob, &mut st);
    } else if st.active {
        let big = st.stacks > BIG_FIRE_STACKS;
        if big != st.big {
            if let Some(old) = st.image.take() {
                hide_image(world, old);
            }
            st.image = show_on(world, mob, mob, fire_look(big));
            st.big = big;
        }
    }
    st.time_spent += dt;
    if st.clearing {
        st.next_action -= dt;
        if st.next_action < 0.0 {
            st.stage -= 1;
            if !update_temp(world, mob, &st) {
                return;
            }
            st.next_action += 3.0;
        }
    } else if st.increasing {
        let stage = ((st.time_spent / 3.0).floor() as i32).min(3);
        if stage != st.stage {
            st.stage = stage;
            if !update_temp(world, mob, &st) {
                return;
            }
            if stage == 3 {
                st.increasing = false;
            }
        }
    } else if st.stamina_left > 0 {
        st.next_action -= dt;
        if st.next_action < 0.0 {
            crate::status_effects::adjust_stamina(world, mob, 15.0);
            st.next_action += 2.0;
            st.stamina_left -= 1;
        }
    } else {
        clear_fire(world, mob, &mut st);
    }
    later(world, f64::from(dt), move |world| fire_step(world, mob, st));
}

/// `update_temp`: the fake heat alert at the stage's severity; false once it ended.
fn update_temp(world: &mut World, mob: Entity, st: &FakeFire) -> bool {
    clear_fake_alert(world, mob, alert::TEMPERATURE);
    if st.stage > 0 {
        throw_fake_alert(world, mob, alert::TEMPERATURE, "hot", st.stage as u8);
        return true;
    }
    if st.active {
        return true;
    }
    end_fire(world, mob, st);
    false
}

fn clear_fire(world: &mut World, mob: Entity, st: &mut FakeFire) {
    if !st.active {
        return;
    }
    st.active = false;
    clear_fake_alert(world, mob, alert::FIRE);
    if let Some(image) = st.image.take() {
        hide_image(world, image);
    }
    st.clearing = true;
    st.next_action = 0.0;
}

/// `fire/Destroy()`.
fn end_fire(world: &mut World, mob: Entity, st: &FakeFire) {
    clear_fake_alert(world, mob, alert::FIRE);
    clear_fake_alert(world, mob, alert::TEMPERATURE);
    let burning = world
        .get::<crate::fire::FireStacks>(mob)
        .is_some_and(|f| f.on_fire);
    if !burning {
        crate::mood::clear_event(world, mob, "on_fire");
        crate::mood::add_event(world, mob, "fake_fire", &FAKE_FIRE_MOOD);
    }
    if let Some(image) = st.image {
        hide_image(world, image);
    }
}

/// `/datum/hallucination/death`: a fake death for 7-9 s (and a dead chat line, maybe).
fn death(world: &mut World, mob: Entity) -> bool {
    if unconscious(world, mob) {
        return false;
    }
    crate::status_effects::apply(world, mob, Incap::Paralyze, 30.0);
    screwy(world, mob, |hud| hud.dead = hud.dead.saturating_add(1));
    let name = name_of(world, mob);
    let area = pos_of(world, mob)
        .and_then(|p| {
            let area = world.get_resource::<ss13_mapload::Areas>()?.area_at(p)?;
            Some(crate::shuttle::area_name(world, area))
        })
        .unwrap_or_default();
    tell_message(
        world,
        mob,
        ChatMessage {
            channel: ChatChannel::Local,
            segments: vec![
                ChatSegment::new(Span::Bold, name.clone()),
                ChatSegment::new(Span::Emote, " has died at "),
                ChatSegment::new(Span::Bold, area),
                ChatSegment::new(Span::Emote, "."),
            ],
        },
    );
    let mut delay = 0.0;
    if prob(world, 50.0) {
        let mut dead = world
            .query_filtered::<(Entity, Option<&Stat>), Or<(With<crate::ghost::Ghost>, With<Stat>)>>(
            );
        let mut dead: Vec<Entity> = dead
            .iter(world)
            .filter(|(e, stat)| {
                *stat == Some(&Stat::Dead) || world.get::<crate::ghost::Ghost>(*e).is_some()
            })
            .map(|(e, _)| e)
            .collect();
        dead.sort();
        let mut rng = rng(world);
        if let Some(&salter) = pick(&mut rng, &dead) {
            delay = f64::from(rng.range(20, 50)) / 10.0;
            const HATE: &[&str] = &[
                "admins",
                "batons",
                "blood cult",
                "coders",
                "heretics",
                "myself",
                "revenants",
                "revs",
                "sec",
                "ss13",
                "this game",
                "this round",
                "this shift",
                "this shit",
                "this",
                "wizards",
                "you",
            ];
            let hate = format!(
                "i{} hate {}",
                if rng.chance(50.0) { " fucking" } else { "" },
                pick(&mut rng, HATE).copied().unwrap_or("this")
            );
            let salt = [
                "...".to_owned(),
                "FUCK".to_owned(),
                "git gud".to_owned(),
                "god damn it".to_owned(),
                format!("hey {}", first_name(&name)),
                hate,
                "is the AI rogue?".to_owned(),
                "rip".to_owned(),
                "shitsec".to_owned(),
                "why did i just drop dead?".to_owned(),
                "why was i gibbed".to_owned(),
                "wizard?".to_owned(),
                "you too?".to_owned(),
            ];
            let said = pick(&mut rng, &salt).cloned().unwrap_or_default();
            let who = name_of(world, salter);
            later(world, delay, move |world| {
                tell_message(
                    world,
                    mob,
                    ChatMessage {
                        channel: ChatChannel::Local,
                        segments: vec![
                            ChatSegment::new(Span::Bold, format!("DEAD: {who}")),
                            ChatSegment::new(Span::Emote, format!(" says, \"{said}\"")),
                        ],
                    },
                );
            });
        }
    }
    let wake = delay + rand_secs(world, 7.0, 9.0);
    later(world, wake, move |world| {
        if !exists(world, mob) {
            return;
        }
        screwy(world, mob, |hud| hud.dead = hud.dead.saturating_sub(1));
        if let Some(mut effects) = crate::status_effects::effects_mut(world, mob) {
            effects.set(Incap::Paralyze, 0.0);
        }
    });
    true
}

/// `LOW_FLOOR_LAYER`.
const LOW_FLOOR_LAYER: f32 = 2.05;

impl Body {
    fn look(self) -> ImageLook {
        let (dmi, state) = match self {
            Self::Husk | Self::HuskSideways => ("icons/mob/human/human.dmi", "husk"),
            Self::Ghost => ("icons/mob/simple/mob.dmi", "ghost"),
            Self::Hole => ("icons/effects/effects.dmi", "blank"),
            Self::StaticGuy => ("icons/effects/effects.dmi", "static"),
            Self::Alien => ("icons/mob/nonhuman-player/alien.dmi", "alienother"),
            Self::MiniBubblegum => ("icons/mob/simple/mob.dmi", "horror"),
            Self::Chrono => ("icons/mob/simple/mob.dmi", "chronostuck"),
            Self::God => ("icons/mob/simple/mob.dmi", "god"),
            Self::Sling => ("icons/mob/simple/mob.dmi", "shadowling_ascended"),
            Self::Faceless => ("icons/obj/trader_signs.dmi", "faceless"),
            Self::Bones => ("icons/obj/trader_signs.dmi", "mrbones"),
        };
        let mut look = ImageLook::new(dmi, state, LOW_FLOOR_LAYER);
        if self == Self::HuskSideways {
            look.rotation = 90;
        }
        look
    }
}

/// `/datum/hallucination/body`: a body on a free floor in view for 3-5 s.
// shortcut: no floating animation in space.
fn body(world: &mut World, mob: Entity, kind: Body) -> bool {
    if client_of(world, mob).is_none() || unconscious(world, mob) {
        return false;
    }
    let Some(at) = pos_of(world, mob) else {
        return false;
    };
    let mut tiles: Vec<TilePos> = view_tiles(world, at, 7)
        .into_iter()
        .filter(|&p| {
            crate::construction::turf_at(world, p)
                .and_then(|t| crate::construction::path_of(world, t))
                .is_some_and(|path| crate::obj_damage::is_under(&path, "/turf/open"))
                && world
                    .resource::<ss13_world::SpatialIndex>()
                    .dense_on_tile(p)
                    .is_empty()
        })
        .collect();
    tiles.sort_by_key(|p| (p.x, p.y));
    let mut rng = rng(world);
    let Some(&spot) = pick(&mut rng, &tiles) else {
        return false;
    };
    let Some(image) = show_image(world, mob, spot, kind.look(), None) else {
        return false;
    };
    if kind == Body::StaticGuy {
        let until = super::now(world) + rand_secs(world, 120.0, 180.0);
        watch_static_guy(world, mob, image, spot, until);
    } else {
        let secs = rand_secs(world, 3.0, 5.0);
        hide_later(world, Some(image), secs);
    }
    true
}

/// `staticguy/on_move`: it fades when walked into, vanishes out of view, else stays 2-3 min.
// shortcut: polled every 0.5 s instead of on each move.
fn watch_static_guy(world: &mut World, mob: Entity, image: Entity, spot: TilePos, until: f64) {
    if !exists(world, image) {
        return;
    }
    let at = pos_of(world, mob);
    let Some(at) = at.filter(|_| super::now(world) < until) else {
        hide_image(world, image);
        return;
    };
    if at == spot {
        hide_later(world, Some(image), 0.6);
        return;
    }
    if !view_tiles(world, at, 7).contains(&spot) {
        hide_image(world, image);
        return;
    }
    later(world, 0.5, move |world| {
        watch_static_guy(world, mob, image, spot, until);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_one_entry_with_a_unique_path() {
        let mut paths: Vec<&str> = TABLE.iter().map(|d| d.path).collect();
        paths.sort_unstable();
        let before = paths.len();
        paths.dedup();
        assert_eq!(before, paths.len());
        for d in TABLE {
            assert_eq!(Kind::by_path(d.path), Some(d.kind));
            assert!(d.path.starts_with("/datum/hallucination/"));
        }
    }

    #[test]
    fn subtypes_are_picked_under_their_parent() {
        let mut rng = CombatRng(11);
        for _ in 0..50 {
            let kind =
                Kind::random_subtype(&mut rng, "/datum/hallucination/fake_sound/normal").unwrap();
            assert!(kind.def().path.contains("/fake_sound/normal/"));
            let weird =
                Kind::random_subtype(&mut rng, "/datum/hallucination/fake_sound/weird").unwrap();
            // Weightless types (ice crack) are never picked.
            assert_ne!(weird, Kind::Sound(Snd::IceCrack));
        }
        assert!(Kind::random_subtype(&mut rng, "/datum/hallucination/delusion/preset").is_none());
    }

    #[test]
    fn sound_files_are_listed_once() {
        let files = sound_files();
        assert!(files.contains(&"sound/machines/airlock/airlock.ogg"));
        assert!(files.contains(&CULT_SUMMON_SOUND));
        assert!(files.iter().all(|f| f.ends_with(".ogg")));
        // The converter copies them.
        for file in files {
            assert!(
                ss13_mapload::sound::files::HALLUCINATION.contains(&file),
                "{file}"
            );
        }
    }
}
