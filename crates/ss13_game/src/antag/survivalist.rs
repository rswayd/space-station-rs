//! Summon Guns and its survivalists after tg `code/modules/spells/spell_types/right_and_wrong.dm`
//! (`summon_guns`, `give_guns`, `/datum/summon_things_controller/item`) and
//! `code/modules/antagonists/survivalist/survivalist.dm` (`/datum/antagonist/survivalist/guns`,
//! `/datum/objective/steal_n_of_type/summon_guns`, `/datum/objective/survive`).
//!
//! - [`summon_guns`] (the wizard's ritual): every crew or antagonist human on the station gets a
//!   random gun from `GLOB.summoned_guns` in hand (or at its feet), unlocked with a standard
//!   pin; a living one with a player who is not an antagonist becomes a survivalist with the
//!   ritual's chance (10% from the spellbook). Wizards and survivalists get nothing. Each cast
//!   gives everyone another gun; latejoiners get one per cast ([`equip_latejoiners`]).
//! - **The survivalist** ([`add_survivalist`]): "steal at least five guns" (toys, practice and
//!   tag lasers, syringe guns and other `NOT_A_REAL_GUN` ones do not count) and "stay alive",
//!   the greeting, the default `AntagInfoGeneric` panel.
//! - [`roundend_report`]: the "Другие антагонисты" section with each survivalist.
// shortcut: no Summon Magic (its wands and artefacts are mostly not ported), no battle royale, no
// admin secret; the summon is not logged to admins. A gun's own special pin (DNA, test range...)
// is replaced by a standard one that answers with the gun's `pin_default_auth`.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::spawner::Rng;
use ss13_net::chat::server::SendChat;
use ss13_net::server::{ClientToken, Networked};
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::UiInterface;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::dynamic::DynamicRng;
use super::objectives::{RoundEnd, all_contents, considered_alive};
use super::{AntagHudData, AntagHudUi, BrotherInfo, ObjectiveInfo};
use crate::health::Stat;
use crate::human::HumanLook;
use crate::jobs::PlayerJob;
use crate::server::PlayerMob;
use crate::tools::tell;

/// `/datum/antagonist/survivalist` `name`.
pub const ANTAG_NAME: &str = "Survivalist";
/// The default `roundend_category`.
pub const ROUNDEND_CATEGORY: &str = "Другие антагонисты";
/// `/datum/antagonist/survivalist/guns` `greet_message`.
pub const GREET: &str = "Your own safety matters above all else, and the only way to ensure your \
                         safety is to stockpile weapons! Grab as many guns as possible, by any \
                         means necessary. Kill anyone who gets in your way.";
/// `/datum/objective/steal_n_of_type/summon_guns` `amount`.
pub const GUNS_TO_STEAL: usize = 5;
const STEAL_GUNS: &str = "Украдите как минимум пять видов огнестрельного оружия!";
const SURVIVE: &str = "Оставайтесь в живых до самого конца.";
const OBJECTIVE_NAME: &str = "Задача";
const GUN: &str = "/obj/item/gun";
/// `new /obj/item/firing_pin` of `unlock()`.
const STANDARD_PIN: &str = "/obj/item/firing_pin";
const SUMMON_SOUND: &str = "sound/effects/magic/summon_guns.ogg";
/// The guns (and their subtypes) with `gun_flags = NOT_A_REAL_GUN`.
pub const NOT_REAL_GUNS: [&str; 15] = [
    "/obj/item/gun/ballistic/automatic/toy",
    "/obj/item/gun/ballistic/automatic/pistol/toy",
    "/obj/item/gun/ballistic/shotgun/toy",
    "/obj/item/gun/ballistic/automatic/c20r/toy",
    "/obj/item/gun/ballistic/automatic/l6_saw/toy",
    "/obj/item/gun/ballistic/revolver/russian",
    "/obj/item/gun/energy/laser/practice",
    "/obj/item/gun/energy/laser/carbine/practice",
    "/obj/item/gun/energy/laser/bluetag",
    "/obj/item/gun/energy/laser/redtag",
    "/obj/item/gun/energy/e_gun/mini/practice_phaser",
    "/obj/item/gun/energy/floragun",
    "/obj/item/gun/energy/plasmacutter",
    "/obj/item/gun/energy/wormhole_projector",
    "/obj/item/gun/energy/gravity_gun",
];
/// More `NOT_A_REAL_GUN` guns outside the families above.
const NOT_REAL_OTHER: [&str; 2] = ["/obj/item/gun/syringe", "/obj/item/gun/energy/wiremod_gun"];
/// `GLOB.summoned_guns`.
pub const SUMMONED_GUNS: [&str; 59] = [
    "/obj/item/gun/ballistic/automatic/ar",
    "/obj/item/gun/ballistic/automatic/battle_rifle",
    "/obj/item/gun/ballistic/automatic/c20r/unrestricted",
    "/obj/item/gun/ballistic/automatic/gyropistol",
    "/obj/item/gun/ballistic/automatic/l6_saw/unrestricted",
    "/obj/item/gun/ballistic/automatic/laser",
    "/obj/item/gun/ballistic/automatic/m90/unrestricted",
    "/obj/item/gun/ballistic/automatic/mini_uzi",
    "/obj/item/gun/ballistic/automatic/pistol",
    "/obj/item/gun/ballistic/automatic/pistol/aps",
    "/obj/item/gun/ballistic/automatic/pistol/clandestine/fisher",
    "/obj/item/gun/ballistic/automatic/pistol/deagle",
    "/obj/item/gun/ballistic/automatic/pistol/deagle/regal",
    "/obj/item/gun/ballistic/automatic/pistol/doohickey",
    "/obj/item/gun/ballistic/automatic/pistol/m1911",
    "/obj/item/gun/ballistic/automatic/proto/unrestricted",
    "/obj/item/gun/ballistic/automatic/smartgun",
    "/obj/item/gun/ballistic/automatic/tommygun",
    "/obj/item/gun/ballistic/automatic/wt550",
    "/obj/item/gun/ballistic/revolver",
    "/obj/item/gun/ballistic/revolver/badass",
    "/obj/item/gun/ballistic/revolver/c38",
    "/obj/item/gun/ballistic/revolver/nagant",
    "/obj/item/gun/ballistic/rifle/boltaction",
    "/obj/item/gun/ballistic/rifle/boltaction/donkrifle",
    "/obj/item/gun/ballistic/rifle/boltaction/harpoon",
    "/obj/item/gun/ballistic/rifle/rebarxbow/syndie",
    "/obj/item/gun/ballistic/rifle/sniper_rifle",
    "/obj/item/gun/ballistic/rocketlauncher/unrestricted",
    "/obj/item/gun/ballistic/shotgun/automatic/combat",
    "/obj/item/gun/ballistic/shotgun/bulldog/unrestricted",
    "/obj/item/gun/ballistic/shotgun/doublebarrel",
    "/obj/item/gun/ballistic/shotgun/hook",
    "/obj/item/gun/ballistic/shotgun/lethal",
    "/obj/item/gun/energy/alien",
    "/obj/item/gun/energy/disabler",
    "/obj/item/gun/energy/disabler/smg",
    "/obj/item/gun/energy/e_gun",
    "/obj/item/gun/energy/e_gun/advtaser",
    "/obj/item/gun/energy/e_gun/nuclear",
    "/obj/item/gun/energy/laser",
    "/obj/item/gun/energy/laser/soul",
    "/obj/item/gun/energy/laser/pistol",
    "/obj/item/gun/energy/laser/assault",
    "/obj/item/gun/energy/laser/carbine",
    "/obj/item/gun/energy/laser/cybersun/unrestricted",
    "/obj/item/gun/energy/laser/hellgun",
    "/obj/item/gun/energy/laser/scatter",
    "/obj/item/gun/energy/laser/thermal",
    "/obj/item/gun/energy/laser/thermal/cryo",
    "/obj/item/gun/energy/laser/thermal/inferno",
    "/obj/item/gun/energy/lasercannon",
    "/obj/item/gun/energy/mindflayer",
    "/obj/item/gun/energy/plasmacutter/adv",
    "/obj/item/gun/energy/pulse",
    "/obj/item/gun/energy/pulse/carbine",
    "/obj/item/gun/energy/recharge/ebow/large",
    "/obj/item/gun/energy/recharge/kinetic_accelerator",
    "/obj/item/gun/medbeam",
];

/// `GLOB.summon_guns`: exists once Summon Guns was cast.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct SummonGuns {
    /// `survivor_probability`, percent.
    pub survivor_probability: f32,
    /// `num_to_give_to_latejoiners`: one per cast.
    pub latejoin_guns: u32,
}

/// On a survivalist's body (`/datum/antagonist/survivalist/guns`).
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Survivalist {
    pub key: String,
}

/// The survivalist's objectives, in order: steal guns, survive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SurvivalistObjective {
    StealGuns,
    Survive,
}

pub const OBJECTIVES: [SurvivalistObjective; 2] = [
    SurvivalistObjective::StealGuns,
    SurvivalistObjective::Survive,
];

impl SurvivalistObjective {
    pub fn explanation(self) -> &'static str {
        match self {
            Self::StealGuns => STEAL_GUNS,
            Self::Survive => SURVIVE,
        }
    }

    pub fn check_completion(self, world: &World, owner: Entity) -> bool {
        match self {
            // `isliving(M.current)`, then the real guns on the body.
            Self::StealGuns => {
                world.get::<Stat>(owner).is_some() && real_guns_on(world, owner) >= GUNS_TO_STEAL
            }
            Self::Survive => considered_alive(world, owner),
        }
    }
}

/// `is_type_in_typecache(item, /obj/item/gun)` without `NOT_A_REAL_GUN`.
pub fn is_real_gun(path: &str) -> bool {
    is_under(path, GUN)
        && !NOT_REAL_GUNS
            .iter()
            .chain(NOT_REAL_OTHER.iter())
            .any(|p| is_under(path, p))
}

/// The real guns `owner` carries (`get_all_contents`).
pub fn real_guns_on(world: &World, owner: Entity) -> usize {
    all_contents(world, owner)
        .into_iter()
        .filter(|&e| world.get::<DmItem>(e).is_some_and(|i| is_real_gun(&i.path)))
        .count()
}

impl Survivalist {
    fn info(&self, world: &World, owner: Entity) -> BrotherInfo {
        BrotherInfo {
            antag_name: ANTAG_NAME.to_owned(),
            objectives: OBJECTIVES
                .iter()
                .enumerate()
                .map(|(i, o)| ObjectiveInfo {
                    count: i as u32 + 1,
                    explanation: o.explanation().to_owned(),
                    complete: o.check_completion(world, owner),
                })
                .collect(),
        }
    }
}

/// The survivalist's info panel (tg's default `AntagInfoGeneric`).
pub struct SurvivalistInfoWindow;

impl UiInterface for SurvivalistInfoWindow {
    const NAME: &'static str = crate::antag::cult::AntagInfoGenericUi::NAME;
    type Data = ();
    type StaticData = BrotherInfo;
}

impl ServerUi for SurvivalistInfoWindow {
    type Source = &'static Survivalist;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &Survivalist) -> BrotherInfo {
        BrotherInfo {
            antag_name: ANTAG_NAME.to_owned(),
            objectives: OBJECTIVES
                .iter()
                .enumerate()
                .map(|(i, o)| ObjectiveInfo {
                    count: i as u32 + 1,
                    explanation: o.explanation().to_owned(),
                    complete: false,
                })
                .collect(),
        }
    }

    fn data(_: &Survivalist, _: Entity, _: &()) {}

    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

pub struct SurvivalistPlugin;

impl Plugin for SurvivalistPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<SurvivalistInfoWindow>().add_systems(
            Update,
            (open_info.in_set(UiSystems::Handle), equip_latejoiners),
        );
    }
}

fn with_rng<R>(world: &mut World, f: impl FnOnce(&mut World, &mut Rng) -> R) -> R {
    let mut rng = world
        .remove_resource::<DynamicRng>()
        .map_or_else(|| Rng::new(0), |r| r.0);
    let out = f(world, &mut rng);
    world.insert_resource(DynamicRng(rng));
    out
}

fn chat_to(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

/// `/proc/summon_guns(user, survivor_probability)`: the controller made or its chance set,
/// then `equip_all_affected`.
pub fn summon_guns(world: &mut World, user: Option<Entity>, survivor_probability: f32) {
    if let Some(user) = user {
        chat_to(world, user, Span::Warning, "You summoned guns!");
    }
    info!("Summon Guns was triggered by {user:?}");
    let mut controller = world
        .get_resource::<SummonGuns>()
        .cloned()
        .unwrap_or_default();
    controller.survivor_probability = survivor_probability;
    controller.latejoin_guns += 1;
    world.insert_resource(controller);
    for mob in affected(world) {
        give_guns(world, mob);
    }
}

/// `get_affected_minds`: crew and antagonist humans on the station levels, sorted.
fn affected(world: &mut World) -> Vec<Entity> {
    let levels = crate::events::controller::station_levels(world);
    let mut q =
        world.query_filtered::<(Entity, Has<PlayerJob>, Has<AntagHudData>), With<HumanLook>>();
    let mut out: Vec<Entity> = q
        .iter(world)
        .filter(|(_, crew, antag)| *crew || *antag)
        .map(|(e, _, _)| e)
        .collect();
    out.retain(|&e| crate::explosions::turf_of(world, e).is_some_and(|t| levels.contains(&t.z)));
    out.sort();
    out
}

/// `/proc/give_guns(to_equip)`. `true` when a gun was given.
pub fn give_guns(world: &mut World, mob: Entity) -> bool {
    let Some(probability) = world
        .get_resource::<SummonGuns>()
        .map(|c| c.survivor_probability)
    else {
        return false;
    };
    if world.get::<Stat>(mob).is_none_or(|s| *s == Stat::Dead)
        || world.get::<PlayerMob>(mob).is_none()
        || world
            .get::<crate::antag::wizard::server::Wizard>(mob)
            .is_some()
        || world.get::<Survivalist>(mob).is_some()
    {
        return false;
    }
    let (survivor, path) = with_rng(world, |_, rng| {
        let survivor = rng.prob(probability);
        let path = SUMMONED_GUNS[rng.range(0, SUMMONED_GUNS.len() as i64 - 1) as usize];
        (survivor, path)
    });
    // `!length(to_equip.mind.antag_datums)`.
    if survivor && world.get::<AntagHudData>(mob).is_none() {
        add_survivalist(world, mob);
        info!("{mob} was made into a survivalist by summon guns, and trusts no one!");
    }
    let Some(gun) = crate::construction::spawn_detached(world, path) else {
        warn!("no prototype for {path}");
        return false;
    };
    world.flush();
    // `spawned_gun.unlock()`.
    if let Some(mut state) = world.get_mut::<crate::guns::Gun>(gun) {
        state.pin = STANDARD_PIN.to_owned();
    }
    world.entity_mut(gun).insert(Networked);
    if let Some(pos) = crate::explosions::turf_of(world, mob) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, SUMMON_SOUND, 50.0, true),
        );
    }
    let in_hand = crate::antag::wizard::server::give(world, mob, gun);
    let name = world
        .get::<Name>(gun)
        .map(|n| n.to_string())
        .unwrap_or_default();
    let place = if in_hand {
        "in your hand"
    } else {
        "at your feet"
    };
    chat_to(
        world,
        mob,
        Span::Warning,
        format!("{} appears {place}!", crate::forensics::capitalize(&name)),
    );
    true
}

/// `COMSIG_GLOB_CREWMEMBER_JOINED` while Summon Guns stands: a gun per cast for each new crew
/// human (`equip_latejoiner`).
fn equip_latejoiners(
    controller: Option<Res<SummonGuns>>,
    joined: Query<Entity, (Added<PlayerJob>, With<HumanLook>)>,
    mut commands: Commands,
) {
    let Some(controller) = controller else {
        return;
    };
    let times = controller.latejoin_guns;
    for mob in &joined {
        commands.queue(move |world: &mut World| {
            for _ in 0..times {
                give_guns(world, mob);
            }
        });
    }
}

/// `add_antag_datum(/datum/antagonist/survivalist/guns)`: the objectives, the greeting and the
/// info panel. `false` if `mob` already is one.
pub fn add_survivalist(world: &mut World, mob: Entity) -> bool {
    if world.get::<Survivalist>(mob).is_some() || world.get_entity(mob).is_err() {
        return false;
    }
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    world.entity_mut(mob).insert(Survivalist { key });
    if world.get::<AntagHudData>(mob).is_none() {
        world.entity_mut(mob).insert(AntagHudData::default());
    }
    let Some(client) = client else {
        return true;
    };
    // `greet`: the default greeting, the message, the objectives.
    // shortcut: `span_big` is drawn bold (no big span in the chat).
    tell(world, client, Span::Bold, format!("Вы - {ANTAG_NAME}."));
    tell(world, client, Span::Bold, GREET);
    tell(world, client, Span::Notice, "Ваши текущие задачи:");
    for (i, objective) in OBJECTIVES.iter().enumerate() {
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage {
                channel: ChatChannel::System,
                segments: vec![
                    ChatSegment::new(Span::Bold, format!("{OBJECTIVE_NAME} #{}", i + 1)),
                    ChatSegment::new(Span::Plain, format!(": {}", objective.explanation())),
                ],
            },
        });
    }
    world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
    world.write_message(OpenUi::new::<SurvivalistInfoWindow>(
        client, mob, mob, ANTAG_NAME,
    ));
    true
}

/// `/datum/action/antag_info/Trigger` of a survivalist.
fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if world.get::<Survivalist>(source).is_some() {
            world.write_message(OpenUi::new::<SurvivalistInfoWindow>(
                client, actor, source, ANTAG_NAME,
            ));
        }
    }
}

/// `roundend_report_header` and each survivalist (`/datum/antagonist/roundend_report`).
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let mut q = world.query::<(Entity, &Survivalist)>();
    let mut survivalists: Vec<(Entity, Survivalist)> =
        q.iter(world).map(|(e, s)| (e, s.clone())).collect();
    if survivalists.is_empty() {
        return Vec::new();
    }
    survivalists.sort_by_key(|(e, _)| *e);
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = vec![line(vec![ChatSegment::new(
        Span::Bold,
        format!("{ROUNDEND_CATEGORY} были:"),
    )])];
    for (mob, survivalist) in survivalists {
        out.push(super::traitor::print_player(world, mob, &survivalist.key));
        let info = survivalist.info(world, mob);
        let won = info.objectives.iter().all(|o| o.complete);
        for objective in info.objectives {
            out.push(line(vec![
                ChatSegment::new(Span::Bold, format!("{OBJECTIVE_NAME} #{}", objective.count)),
                ChatSegment::new(Span::Plain, format!(": {} ", objective.explanation)),
                if objective.complete {
                    ChatSegment::new(Span::GreenText, "Успех!")
                } else {
                    ChatSegment::new(Span::RedText, "Неудача.")
                },
            ]));
        }
        out.push(line(vec![if won {
            ChatSegment::new(Span::GreenText, format!("{ANTAG_NAME} был успешен!"))
        } else {
            ChatSegment::new(Span::RedText, format!("{ANTAG_NAME} провалился!"))
        }]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toys_and_practice_guns_do_not_count() {
        assert!(is_real_gun("/obj/item/gun/energy/laser"));
        assert!(is_real_gun(
            "/obj/item/gun/ballistic/automatic/c20r/unrestricted"
        ));
        assert!(!is_real_gun("/obj/item/gun/ballistic/automatic/c20r/toy"));
        assert!(!is_real_gun("/obj/item/gun/energy/laser/practice"));
        assert!(!is_real_gun("/obj/item/gun/syringe/rapidsyringe"));
        // `plasmacutter/adv` inherits the flag in tg.
        assert!(!is_real_gun("/obj/item/gun/energy/plasmacutter/adv"));
        assert!(!is_real_gun("/obj/item/storage/toolbox"));
        assert!(SUMMONED_GUNS.iter().all(|p| is_under(p, GUN)));
    }

    #[test]
    fn summon_makes_survivalists_of_non_antags_only() {
        let mut world = World::new();
        world.init_resource::<Messages<SendChat>>();
        world.init_resource::<Messages<OpenUi>>();
        world.insert_resource(SummonGuns {
            survivor_probability: 100.0,
            latejoin_guns: 1,
        });
        let client = world.spawn_empty().id();
        let crew = world.spawn((Stat::Conscious, PlayerMob { client })).id();
        let antag = world
            .spawn((
                Stat::Conscious,
                PlayerMob { client },
                AntagHudData::default(),
            ))
            .id();
        let dead = world.spawn((Stat::Dead, PlayerMob { client })).id();
        // No gun prototypes in a bare world: the antagonist datum still comes first, as in tg.
        give_guns(&mut world, crew);
        give_guns(&mut world, antag);
        give_guns(&mut world, dead);
        assert!(world.get::<Survivalist>(crew).is_some());
        assert!(world.get::<Survivalist>(antag).is_none());
        assert!(world.get::<Survivalist>(dead).is_none());
        // A survivalist gets no more guns.
        assert!(!give_guns(&mut world, crew));
        // Alive with fewer than five guns: survives, but fails the theft.
        assert!(SurvivalistObjective::Survive.check_completion(&world, crew));
        assert!(!SurvivalistObjective::StealGuns.check_completion(&world, crew));
        let report = roundend_report(&mut world, RoundEnd::default());
        assert_eq!(report.len(), 5);
    }
}
