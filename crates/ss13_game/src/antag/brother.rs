//! Blood brothers (server) after tg `/datum/antagonist/brother` and `/datum/team/brother_team`
//! (BandaStation strings).
//!
//! [`add_brother`] is `add_antag_datum(/datum/antagonist/brother, team)` on a body. Without a team
//! (the roundstart ruleset, one per selected player) a new [`BrotherTeam`] starts with two
//! conversions (three at 10%); `add_member` forges the team's objectives when it has none and
//! spends one conversion, so a roundstart brother may convert one more (two at 10%). `on_gain`:
//! the team is renamed after its members' last names; while conversions are left the brother gets
//! a plain flash (`equip_conspicuous_item`: a pocket, else the backpack) and may flash from behind
//! (`/datum/component/can_flash_from_behind`); then the greeting, the objectives, the info panel
//! (`AntagInfoBrother`) and the team HUD.
//!
//! Conversion ([`on_successful_flash`], tg `on_mob_successful_flashed_mob`): a successful targeted
//! flash by a brother with conversions left on a conscious crew member with a client, who is not
//! the target of one of the team's objectives, not a brother already and not mindshielded, makes
//! them a brother of the team (`add_brother`), tells the team and burns the flash out. When the
//! last conversion is spent nobody in the team can convert any more.
// shortcut: no stinger sound, moodlet, memory, admin panel commands, removal from the team or
// hijack speed; the greeting uses `.danger` (no `.alertsyndie` chat class); everyone is male
// (no genders yet). Mindshields: `crate::antag::revolution::server::mindshielded`.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_mapload::spawner::Rng;
use ss13_net::server::ClientToken;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::InventorySlot;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::dynamic::DynamicRng;
use super::objectives::{Objective, ObjectiveKind, RoundEnd, forge_brother_objectives};
use super::traitor::{chat, print_player};
use super::uplink::UplinkCatalog;
use super::{AntagHudData, AntagHudUi, AntagInfoBrotherUi, BrotherInfo, ObjectiveInfo};
use crate::health::Stat;
use crate::inventory::{CarriedBy, Inventory, can_equip};
#[cfg(test)]
use crate::jobs::{Jobs, PlayerJob};
use crate::server::PlayerMob;

/// `name = "\improper Brother"` (untranslated in BandaStation).
pub const BROTHER_NAME: &str = "Brother";
/// `/datum/team/brother_team` `name` and `member_name`.
pub const TEAM_NAME: &str = "Кровные братья";
/// `antag_hud_name` in `hud_icon`.
const ANTAG_HUD: (&str, &str) = ("icons/mob/huds/antag_hud.dmi", "brother");
/// `equip_conspicuous_item(new /obj/item/assembly/flash)`.
pub const BROTHER_FLASH: &str = "/obj/item/assembly/flash";
/// `brothers_left = 2`, and `New()`: `if (prob(10)) brothers_left += 1`.
pub const BROTHERS_LEFT: u32 = 2;
const EXTRA_BROTHER_PROB: f32 = 10.0;
/// Jobs whose tg/BandaStation outfit has `implants = list(/obj/item/implant/mindshield)`
/// (`TRAIT_UNCONVERTABLE`).
pub(crate) const MINDSHIELD_JOBS: [&str; 9] = [
    "Captain",
    "Head of Security",
    "Warden",
    "Detective",
    "Security Officer",
    "Veteran Security Advisor",
    "Magistrate",
    "Nanotrasen Representative",
    "Blueshield",
];

/// The brother antagonist datum, on the body it was given to.
#[derive(Component, Clone, Debug)]
pub struct Brother {
    /// The player's key (tg `owner.key`).
    pub key: String,
    pub team: Entity,
    /// `objectives += team.objectives`: the team's, as they were when this brother joined.
    pub objectives: Vec<Objective>,
}

/// `/datum/team/brother_team`: a non-replicated entity.
#[derive(Component, Clone, Debug)]
pub struct BrotherTeam {
    pub name: String,
    /// Bodies, in joining order.
    pub members: Vec<Entity>,
    /// How many more may be converted.
    pub brothers_left: u32,
    pub objectives: Vec<Objective>,
}

pub struct BrotherPlugin;

impl Plugin for BrotherPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<AntagInfoBrotherUi>()
            .add_systems(Update, open_info.in_set(UiSystems::Handle));
    }
}

/// `add_antag_datum(/datum/antagonist/brother, team)` on `mob`; `None` makes a new team.
/// `false` if it already is a brother.
pub fn add_brother(world: &mut World, mob: Entity, team: Option<Entity>) -> bool {
    if world.get::<Brother>(mob).is_some() || world.get_entity(mob).is_err() {
        return false;
    }
    let mut rng = world
        .remove_resource::<DynamicRng>()
        .map_or_else(|| Rng::new(mob.to_bits()), |r| r.0);
    let team = match team.filter(|&t| world.get::<BrotherTeam>(t).is_some()) {
        Some(team) => team,
        None => {
            let brothers_left = BROTHERS_LEFT + u32::from(rng.prob(EXTRA_BROTHER_PROB));
            world
                .spawn(BrotherTeam {
                    name: TEAM_NAME.to_owned(),
                    members: Vec::new(),
                    brothers_left,
                    objectives: Vec::new(),
                })
                .id()
        }
    };
    // `add_member`: the first member forges the objectives; joining spends a conversion.
    let (mut members, brothers_left, mut objectives) = {
        let t = world.get::<BrotherTeam>(team).expect("checked above");
        (t.members.clone(), t.brothers_left, t.objectives.clone())
    };
    members.push(mob);
    if objectives.is_empty() {
        let targets = world
            .get_resource::<UplinkCatalog>()
            .map(|c| c.data.steal_items.clone())
            .unwrap_or_default();
        objectives = forge_brother_objectives(world, &members, brothers_left, &targets, &mut rng);
    }
    world.insert_resource(DynamicRng(rng));
    let brothers_left = brothers_left.saturating_sub(1);
    let first = members.len() == 1;
    {
        let mut t = world.get_mut::<BrotherTeam>(team).expect("checked above");
        t.members = members;
        t.brothers_left = brothers_left;
        t.objectives.clone_from(&objectives);
    }
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let hud_icon = world
        .get_resource::<IconIndex>()
        .and_then(|index| index.get(ANTAG_HUD.0, ANTAG_HUD.1));
    world.entity_mut(mob).insert((
        Brother {
            key,
            team,
            objectives: objectives.clone(),
        },
        AntagHudData {
            hud_icon,
            ..default()
        },
    ));
    // `on_gain`.
    update_name(world, team);
    if brothers_left > 0 && world.get::<Inventory>(mob).is_some() {
        equip_conspicuous_item(world, mob, BROTHER_FLASH);
        if !first {
            chat(
                world,
                client,
                Span::BoldWarning,
                "Синдикат возлагает на вас большие надежды, чем на других. Они предоставили вам \
                 вспышку, чтобы вы могли пробудить еще одного агента.",
            );
        }
    }
    // `greet()`: the greeting and `announce_objectives`; then the base `on_gain`.
    chat(world, client, Span::Danger, "Вы - кровный брат.");
    chat(world, client, Span::Notice, "Ваши текущие задачи:");
    for (i, objective) in objectives.iter().enumerate() {
        if let Some(client) = client {
            world.write_message(ss13_net::chat::server::SendChat {
                clients: vec![client],
                message: ChatMessage {
                    channel: ChatChannel::System,
                    segments: vec![
                        ChatSegment::new(Span::Bold, format!("Задача #{}", i + 1)),
                        ChatSegment::new(Span::Plain, format!(": {}", objective.explanation)),
                    ],
                },
            });
        }
    }
    chat(
        world,
        client,
        Span::BoldNotice,
        "Для большей информации - прочтите панель. Вы можете вернуться снова, нажав кнопку \
         сверху слева.",
    );
    if let Some(client) = client {
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
        world.write_message(OpenUi::new::<AntagInfoBrotherUi>(
            client,
            mob,
            mob,
            BROTHER_NAME,
        ));
    }
    info!("{mob} is a blood brother of team {team} ({brothers_left} conversions left)");
    true
}

/// `update_name`: "Кровные братья <last names joined by и>", or "<last name> одинокий бунтарь".
fn update_name(world: &mut World, team: Entity) {
    let Some(members) = world.get::<BrotherTeam>(team).map(|t| t.members.clone()) else {
        return;
    };
    let last_names: Vec<String> = members
        .iter()
        .map(|&m| {
            world
                .get::<Name>(m)
                .and_then(|n| n.as_str().split(' ').next_back().map(str::to_owned))
                .unwrap_or_default()
        })
        .collect();
    let name = match &last_names[..] {
        [one] => format!("{one} одинокий бунтарь"),
        _ => format!("{TEAM_NAME} {}", last_names.join(" и ")),
    };
    if let Some(mut t) = world.get_mut::<BrotherTeam>(team) {
        t.name = name;
    }
}

/// tg `equip_conspicuous_item`: a new `path` into a free pocket, else into the backpack, else
/// deleted.
/// Where it went (tg's `where`), `None` when it was deleted.
pub(crate) fn equip_conspicuous_item(
    world: &mut World,
    mob: Entity,
    path: &str,
) -> Option<&'static str> {
    let &at = world.get::<TilePos>(mob)?;
    let Some(item) = crate::construction::spawn_obj(world, path, at) else {
        warn!("antag: no prototype for {path}");
        return None;
    };
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        world.despawn(item);
        return None;
    };
    for (i, slot) in [InventorySlot::LeftPocket, InventorySlot::RightPocket]
        .into_iter()
        .enumerate()
    {
        if can_equip(world, &inventory, item, slot) {
            world
                .entity_mut(item)
                .remove::<TilePos>()
                .insert(CarriedBy(mob));
            world
                .get_mut::<Inventory>(mob)
                .expect("checked above")
                .pockets[i] = Some(item);
            return Some(if i == 0 {
                "left pocket"
            } else {
                "right pocket"
            });
        }
    }
    if let Some(pack) = inventory.back
        && crate::storage::can_insert(world, pack, item).is_ok()
    {
        crate::storage::put_inside(world, item, pack);
        return Some("backpack");
    }
    world.despawn(item);
    None
}

/// `/datum/component/can_flash_from_behind` while the brother may still convert: a flash from
/// behind is at least a partial one.
pub fn can_flash_from_behind(world: &World, user: Entity) -> bool {
    conversions_left(world, user) > 0
}

fn conversions_left(world: &World, user: Entity) -> u32 {
    world
        .get::<Brother>(user)
        .and_then(|b| world.get::<BrotherTeam>(b.team))
        .map_or(0, |t| t.brothers_left)
}

/// tg `on_mob_successful_flashed_mob` (`COMSIG_MOB_SUCCESSFUL_FLASHED_MOB`, registered while the
/// team has conversions left): `user` flashed `flashed` with `flash`.
pub fn on_successful_flash(world: &mut World, user: Entity, flashed: Entity, flash: Entity) {
    if conversions_left(world, user) == 0 {
        return;
    }
    let Some(team) = world.get::<Brother>(user).map(|b| b.team) else {
        return;
    };
    // Dead, silicons and drones: nothing (only humans have an inventory here).
    if world.get::<Stat>(flashed).is_none_or(|s| *s == Stat::Dead)
        || world.get::<Inventory>(flashed).is_none()
    {
        return;
    }
    let user_client = world.get::<PlayerMob>(user).map(|p| p.client);
    let refuse = |world: &mut World, text: String| {
        if let Some(client) = user_client {
            crate::tools::tell(world, client, Span::Warning, text);
        }
    };
    let name = world
        .get::<Name>(flashed)
        .map(|n| n.to_string())
        .unwrap_or_default();
    // `stat != STABLE`; there is no separate unconsciousness here (hard crit is a stat).
    if world.get::<Stat>(flashed) != Some(&Stat::Conscious) {
        return refuse(world, "в критическом состоянии!".to_owned());
    }
    if world.get::<PlayerMob>(flashed).is_none() {
        return refuse(world, format!("у {name} нет сознания!"));
    }
    let objectives = world
        .get::<Brother>(user)
        .map(|b| b.objectives.clone())
        .unwrap_or_default();
    let is_target = objectives.iter().any(|o| match o.kind {
        ObjectiveKind::Assassinate { target } | ObjectiveKind::Maroon { target } => {
            target == Some(flashed)
        }
        _ => false,
    });
    if is_target {
        return refuse(world, "это ваша цель!".to_owned());
    }
    if world.get::<Brother>(flashed).is_some() {
        return refuse(world, "он предан кому-то другому!".to_owned());
    }
    if crate::antag::revolution::server::mindshielded(world, flashed) {
        return refuse(world, "он сопротивляется!".to_owned());
    }
    if !team_add_brother(world, team, flashed) {
        return refuse(world, "неудача!".to_owned());
    }
    info!("{flashed} was made into a blood brother by {user}");
    crate::security::burn_out(world, flash);
    refuse(world, "converted".to_owned());
}

/// `/datum/team/brother_team/add_brother`: `new_brother` joins the team (spending a conversion)
/// and the other members are told.
fn team_add_brother(world: &mut World, team: Entity, new_brother: Entity) -> bool {
    if world.get::<PlayerMob>(new_brother).is_none() || !add_brother(world, new_brother, Some(team))
    {
        return false;
    }
    let Some((members, brothers_left)) = world
        .get::<BrotherTeam>(team)
        .map(|t| (t.members.clone(), t.brothers_left))
    else {
        return false;
    };
    let name = world
        .get::<Name>(new_brother)
        .map(|n| n.to_string())
        .unwrap_or_default();
    for member in members.into_iter().filter(|&m| m != new_brother) {
        let client = world.get::<PlayerMob>(member).map(|p| p.client);
        let Some(client) = client else {
            continue;
        };
        world.write_message(ss13_net::chat::server::SendChat {
            clients: vec![client],
            message: ChatMessage {
                channel: ChatChannel::System,
                segments: vec![
                    ChatSegment::new(Span::Bold, name.clone()),
                    ChatSegment::new(
                        Span::Notice,
                        " пробужден, чтобы помогать вам как ваш кровный брат!",
                    ),
                ],
            },
        });
        if brothers_left == 0 {
            chat(
                world,
                Some(client),
                Span::Notice,
                "Вы больше не можете пробуждать кровных братьев.",
            );
        }
    }
    true
}

impl ServerUi for AntagInfoBrotherUi {
    type Source = &'static Brother;
    type UserParam = ();
    type Act = ();

    fn static_data(brother: &Brother) -> BrotherInfo {
        BrotherInfo {
            antag_name: BROTHER_NAME.to_owned(),
            objectives: brother
                .objectives
                .iter()
                .enumerate()
                .map(|(i, o)| ObjectiveInfo {
                    count: i as u32 + 1,
                    explanation: o.explanation.clone(),
                    complete: false,
                })
                .collect(),
        }
    }

    fn data(_: &Brother, _: Entity, _: &()) {}

    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

/// `/datum/action/antag_info/Trigger` of the brother's button.
fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if world.get::<Brother>(source).is_some() {
            world.write_message(OpenUi::new::<AntagInfoBrotherUi>(
                client,
                actor,
                source,
                BROTHER_NAME,
            ));
        }
    }
}

/// tg `antag_report` for brother teams: each team's `/datum/team/roundend_report`.
pub fn roundend_report(world: &mut World, end: RoundEnd) -> Vec<ChatMessage> {
    let mut query = world.query::<(Entity, &BrotherTeam)>();
    let mut teams: Vec<(Entity, BrotherTeam)> =
        query.iter(world).map(|(e, t)| (e, t.clone())).collect();
    teams.sort_by_key(|(e, _)| *e);
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = Vec::new();
    for (_, team) in teams {
        out.push(line(vec![ChatSegment::new(
            Span::Bold,
            format!("{}:", team.name),
        )]));
        out.push(line(vec![ChatSegment::new(
            Span::Plain,
            format!("{TEAM_NAME} были:"),
        )]));
        for &member in &team.members {
            let key = world
                .get::<Brother>(member)
                .map(|b| b.key.clone())
                .unwrap_or_default();
            out.push(print_player(world, member, &key));
        }
        if team.objectives.is_empty() {
            continue;
        }
        out.push(line(vec![ChatSegment::new(
            Span::Bold,
            "Команда имела следующие цели:",
        )]));
        let mut win = true;
        for (i, objective) in team.objectives.iter().enumerate() {
            let done = objective.check_completion(world, &team.members, end);
            win &= done;
            out.push(line(vec![
                ChatSegment::new(Span::Bold, format!("Цель #{}", i + 1)),
                ChatSegment::new(Span::Plain, format!(": {} ", objective.explanation)),
                if done {
                    ChatSegment::new(Span::GreenText, "Успех!")
                } else {
                    ChatSegment::new(Span::RedText, "Неудача.")
                },
            ]));
        }
        out.push(line(vec![if win {
            ChatSegment::new(
                Span::GreenText,
                format!("{} выполнил свои задачи!", team.name),
            )
        } else {
            ChatSegment::new(Span::RedText, format!("{} провалился!", team.name))
        }]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A conscious crew body with a client, a name and an inventory.
    fn crew(world: &mut World, name: &str) -> Entity {
        let client = world.spawn_empty().id();
        world
            .spawn((
                Stat::Conscious,
                PlayerJob(0),
                Name::new(name.to_owned()),
                Inventory::default(),
                PlayerMob { client },
            ))
            .id()
    }

    fn world() -> World {
        let mut world = World::new();
        world.insert_resource(Jobs::new(crate::jobs::fallback_jobs(), 0));
        world.init_resource::<Messages<ss13_net::chat::server::SendChat>>();
        world.init_resource::<Messages<OpenUi>>();
        world
    }

    #[test]
    fn a_roundstart_brother_gets_a_team_and_shared_objectives() {
        let mut world = world();
        let first = crew(&mut world, "Иван Петров");
        crew(&mut world, "Пётр Сидоров");
        assert!(add_brother(&mut world, first, None));
        assert!(!add_brother(&mut world, first, None));
        let team = world.get::<Brother>(first).unwrap().team;
        let t = world.get::<BrotherTeam>(team).unwrap().clone();
        assert_eq!(t.members, [first]);
        // Two conversions (three at 10%), one spent by joining.
        assert!((1..=2).contains(&t.brothers_left), "{}", t.brothers_left);
        assert_eq!(t.name, "Петров одинокий бунтарь");
        // Convert, kill or steal objectives, then escape or hijack.
        assert_eq!(t.objectives[0].kind, ObjectiveKind::ConvertBrother);
        assert!(matches!(
            t.objectives.last().unwrap().kind,
            ObjectiveKind::Escape | ObjectiveKind::Hijack
        ));
        assert_eq!(
            world.get::<Brother>(first).unwrap().objectives,
            t.objectives
        );
        assert!(can_flash_from_behind(&world, first));
    }

    #[test]
    fn forging_follows_the_team_size_and_hijack() {
        let mut world = world();
        let owner = crew(&mut world, "Иван Петров");
        crew(&mut world, "Пётр Сидоров");
        let (mut hijacks, mut escapes) = (0, 0);
        for seed in 0..200 {
            let mut rng = Rng::new(seed);
            for brothers_left in [2, 3] {
                let objectives =
                    forge_brother_objectives(&mut world, &[owner], brothers_left, &[], &mut rng);
                let hijack = objectives.last().unwrap().kind == ObjectiveKind::Hijack;
                hijacks += usize::from(hijack);
                escapes += usize::from(!hijack);
                // `max(1, 2 + (brothers_left > 2) - is_hijacker)` between convert and ending.
                let middle = 2 + usize::from(brothers_left > 2) - usize::from(hijack);
                assert_eq!(objectives.len(), middle + 2);
                for o in &objectives[1..=middle] {
                    match &o.kind {
                        ObjectiveKind::Assassinate { target }
                        | ObjectiveKind::Maroon { target } => {
                            assert_ne!(*target, Some(owner))
                        }
                        ObjectiveKind::Steal { .. } => {}
                        other => panic!("{other:?}"),
                    }
                }
            }
        }
        assert!((10..80).contains(&hijacks), "{hijacks} hijacks, {escapes}");
    }

    #[test]
    fn conversion_checks_eligibility_and_spends_the_slot() {
        let mut world = world();
        let first = crew(&mut world, "Иван Петров");
        let target = crew(&mut world, "Пётр Сидоров");
        let officer = crew(&mut world, "Олег Щитов");
        let ghost = world
            .spawn((Stat::Conscious, PlayerJob(0), Inventory::default()))
            .id();
        let mut jobs = crate::jobs::fallback_jobs();
        let mut sec = jobs.jobs[0].clone();
        sec.title = "Security Officer".into();
        jobs.jobs.push(sec);
        world.insert_resource(Jobs::new(jobs, 0));
        world.entity_mut(officer).insert(PlayerJob(1));
        assert!(add_brother(&mut world, first, None));
        let team = world.get::<Brother>(first).unwrap().team;
        // One conversion left, and the target is the one of a kill objective.
        world.get_mut::<BrotherTeam>(team).unwrap().brothers_left = 1;
        let kill = Objective {
            kind: ObjectiveKind::Assassinate {
                target: Some(target),
            },
            explanation: String::new(),
        };
        let convert = world.get::<BrotherTeam>(team).unwrap().objectives[0].clone();
        world.get_mut::<Brother>(first).unwrap().objectives = vec![convert.clone(), kill];
        let flash = world.spawn_empty().id();
        // No client, mindshielded, in crit: refused.
        on_successful_flash(&mut world, first, ghost, flash);
        on_successful_flash(&mut world, first, officer, flash);
        world.entity_mut(target).insert(Stat::SoftCrit);
        on_successful_flash(&mut world, first, target, flash);
        assert_eq!(world.get::<BrotherTeam>(team).unwrap().members, [first]);
        // Not the brothers' target; a conscious crew member then joins, spending the last
        // conversion.
        world.entity_mut(target).insert(Stat::Conscious);
        on_successful_flash(&mut world, first, target, flash);
        assert_eq!(world.get::<BrotherTeam>(team).unwrap().members, [first]);
        world.get_mut::<Brother>(first).unwrap().objectives = vec![convert];
        world
            .get_mut::<BrotherTeam>(team)
            .unwrap()
            .objectives
            .truncate(1);
        on_successful_flash(&mut world, first, target, flash);
        let t = world.get::<BrotherTeam>(team).unwrap().clone();
        assert_eq!(t.members, [first, target]);
        assert_eq!(t.brothers_left, 0);
        assert_eq!(t.name, "Кровные братья Петров и Сидоров");
        assert_eq!(world.get::<Brother>(target).unwrap().team, team);
        assert_eq!(
            world.get::<Brother>(target).unwrap().objectives,
            t.objectives
        );
        // Nobody can convert any more.
        assert!(!can_flash_from_behind(&world, first));
        assert!(!can_flash_from_behind(&world, target));
        let third = crew(&mut world, "Сергей Иванов");
        on_successful_flash(&mut world, target, third, flash);
        assert!(world.get::<Brother>(third).is_none());
        // The convert objective is done with two members.
        let end = RoundEnd { force_ending: true };
        assert!(t.objectives[0].check_completion(&world, &t.members, end));
        assert!(!t.objectives[0].check_completion(&world, &[first], end));
    }

    #[test]
    fn report_lists_the_team_and_its_objectives() {
        let mut world = world();
        let a = crew(&mut world, "Иван Петров");
        let b = crew(&mut world, "Пётр Сидоров");
        let victim = world.spawn(Stat::Conscious).id();
        let objectives = vec![
            Objective {
                kind: ObjectiveKind::ConvertBrother,
                explanation: "Пробудите.".into(),
            },
            Objective {
                kind: ObjectiveKind::Assassinate {
                    target: Some(victim),
                },
                explanation: "Убейте Боба.".into(),
            },
        ];
        let team = world
            .spawn(BrotherTeam {
                name: "Кровные братья Петров и Сидоров".into(),
                members: vec![a, b],
                brothers_left: 0,
                objectives: objectives.clone(),
            })
            .id();
        for (mob, key) in [(a, "p0"), (b, "p1")] {
            world.entity_mut(mob).insert(Brother {
                key: key.into(),
                team,
                objectives: objectives.clone(),
            });
        }
        let report = roundend_report(&mut world, RoundEnd { force_ending: true });
        let text: Vec<String> = report.iter().map(ChatMessage::plain_text).collect();
        assert_eq!(text[0], "Кровные братья Петров и Сидоров:");
        assert_eq!(text[1], "Кровные братья были:");
        assert_eq!(text[2], "p0 был Иван Петров, Ассистент, и выжил");
        assert_eq!(text[3], "p1 был Пётр Сидоров, Ассистент, и выжил");
        assert_eq!(text[4], "Команда имела следующие цели:");
        assert_eq!(text[5], "Цель #1: Пробудите. Успех!");
        assert_eq!(report[5].segments[2].span, Span::GreenText);
        assert_eq!(text[6], "Цель #2: Убейте Боба. Неудача.");
        assert_eq!(report[6].segments[2].span, Span::RedText);
        assert_eq!(text[7], "Кровные братья Петров и Сидоров провалился!");
        world.entity_mut(victim).insert(Stat::Dead);
        let report = roundend_report(&mut world, RoundEnd { force_ending: true });
        assert_eq!(
            report[7].plain_text(),
            "Кровные братья Петров и Сидоров выполнил свои задачи!"
        );
    }
}
