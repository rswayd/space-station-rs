//! The Syndicate battlecruiser (server) after tg `summon_battlecruiser`
//! (`battlecruiser_starfury.dm`), `/datum/team/battlecruiser`, `/datum/antagonist/battlecruiser`
//! (+ `/captain`) and the ship's ghost role spawners
//! (`/obj/effect/mob_spawn/ghost_role/human/syndicate/battlecruiser`, `allow_spawn`, `special`).
//!
//! [`summon_battlecruiser`] (an admin's call) loads the SBC Starfury
//! (`_maps/templates/battlecruiser_starfury.dmm`) onto a new level, makes the team (its bomb the
//! station's self-destruct terminal, whose code the crew learns; the objective "nuke the
//! station"), puts the ghosts opted in to "Battlecruiser Crew" into its spawners and announces
//! the ship. Whoever takes one of its spawners ([`special`], from
//! `crate::ghost_verbs::take_role`) joins the crew (the captain's spawner: the captain), once per
//! player ([`allow_spawn`]); an assault operative's pocket radio is a nuclear uplink. The round
//! end lists the crew and whether the station was nuked ([`roundend_report`]).
// shortcut: the ship loads on its own level, not at a random spot of an empty space level, and
// its fighters and corvette (`roundstart_template` docks) are not loaded; no poll (the opted-in
// ghosts are taken at once); the traitor's battlecruiser emag card for the communications
// console is not sold.

use bevy::prelude::*;
use ss13_net::server::ClientToken;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};

use super::server::{NUCLEAR_OBJECTIVE, NukeOutcome, chat_to, selfdestruct};
use crate::antag::objectives::RoundEnd;
use crate::antag::{AntagHudData, AntagHudUi};
use crate::ghost_verbs::GhostRoleSpawner;

/// The ship's spawners' root type.
pub const SPAWNER: &str = "/obj/effect/mob_spawn/ghost_role/human/syndicate/battlecruiser";
/// The captain's spawner.
pub const CAPTAIN_SPAWNER: &str =
    "/obj/effect/mob_spawn/ghost_role/human/syndicate/battlecruiser/captain";
/// `/datum/map_template/battlecruiser_starfury` `mappath`.
pub const TEMPLATE: &str = "_maps/templates/battlecruiser_starfury.dmm";
/// `/datum/antagonist/battlecruiser` `name`, the captain's.
pub const CREW_NAME: &str = "Battlecruiser Crewmember";
pub const CAPTAIN_NAME: &str = "Battlecruiser Captain";
const CREW_JOB: &str = "/datum/job/battlecruiser_crew";
const CAPTAIN_JOB: &str = "/datum/job/battlecruiser_captain";
const ANTAG_HUD: &str = "icons/mob/huds/antag_hud.dmi";

/// `/datum/team/battlecruiser`.
#[derive(Resource, Clone, Debug, Default)]
pub struct BattlecruiserTeam {
    /// `nuke`: the self-destruct terminal.
    pub nuke: Option<Entity>,
    pub code: Option<String>,
    pub members: Vec<Entity>,
    /// `players_spawned`: the keys that already took a spawner.
    pub players_spawned: Vec<String>,
    /// The ship's level.
    pub level_z: Option<u16>,
}

/// The battlecruiser antagonist datum, on its body.
#[derive(Component, Clone, Debug)]
pub struct BattlecruiserCrew {
    pub key: String,
    pub captain: bool,
}

/// Whether `def` is one of the ship's spawner types.
pub fn is_spawner(def: Option<&str>) -> bool {
    def.is_some_and(|d| ss13_mapload::fidelity::is_under(d, SPAWNER))
}

/// The spawner's `allow_spawn`: a player takes a battlecruiser spawner once.
pub fn may_spawn(team: Option<&BattlecruiserTeam>, key: &str) -> bool {
    team.is_none_or(|t| !t.players_spawned.iter().any(|k| k == key))
}

fn key_of(world: &World, client: Entity) -> String {
    world
        .get::<ClientToken>(client)
        .map(|t| t.0.clone())
        .unwrap_or_default()
}

/// `allow_spawn(user)` for `crate::ghost_verbs::take_role`; tells the refusal.
pub fn allow_spawn(world: &mut World, client: Entity, def: Option<&str>) -> bool {
    if !is_spawner(def) {
        return true;
    }
    let key = key_of(world, client);
    if may_spawn(world.get_resource::<BattlecruiserTeam>(), &key) {
        return true;
    }
    chat_to(
        world,
        Some(client),
        Span::BoldWarning,
        "Вы уже использовали свой шанс сыграть за экипаж боевого крейсера.",
    );
    false
}

/// The spawner's `special`: the new body joins the crew.
pub fn special(world: &mut World, mob: Entity, client: Entity, def: Option<&str>) {
    if !is_spawner(def) {
        return;
    }
    let captain = def == Some(CAPTAIN_SPAWNER);
    let key = key_of(world, client);
    let job = if captain { CAPTAIN_JOB } else { CREW_JOB };
    crate::noncrew_jobs::set_assigned_role(world, mob, job);
    let code = {
        let mut team = world.get_resource_or_insert_with(BattlecruiserTeam::default);
        team.members.push(mob);
        team.players_spawned.push(key.clone());
        team.code.clone()
    };
    let hud = if captain {
        "battlecruiser_lead"
    } else {
        "battlecruiser_crew"
    };
    let hud_icon = world
        .get_resource::<ss13_mapload::IconIndex>()
        .and_then(|index| index.get(ANTAG_HUD, hud));
    world.entity_mut(mob).insert((
        BattlecruiserCrew { key, captain },
        AntagHudData {
            hud_icon,
            ..default()
        },
    ));
    // The assault operatives' `/obj/item/uplink/nuclear` (`TELECRYSTALS_DEFAULT`).
    let uplink = crate::antag::objectives::all_contents(world, mob)
        .into_iter()
        .find(|&e| {
            world
                .get::<ss13_mapload::DmItem>(e)
                .is_some_and(|i| i.path == "/obj/item/uplink/nuclear")
        });
    if let Some(device) = uplink {
        crate::antag::uplink::give_nuclear_uplink(
            world,
            mob,
            device,
            crate::antag::uplink::TELECRYSTALS_DEFAULT,
        );
    }
    // `greet` + `on_gain`.
    let name = if captain { CAPTAIN_NAME } else { CREW_NAME };
    chat_to(world, Some(client), Span::Bold, format!("Вы - {name}!"));
    chat_to(world, Some(client), Span::Notice, "Ваши текущие задачи:");
    world.write_message(ss13_net::chat::server::SendChat {
        clients: vec![client],
        message: ChatMessage {
            channel: ChatChannel::System,
            segments: vec![
                ChatSegment::new(Span::Bold, "Задача #1"),
                ChatSegment::new(Span::Plain, format!(": {NUCLEAR_OBJECTIVE}")),
            ],
        },
    });
    if let Some(code) = code {
        chat_to(
            world,
            Some(client),
            Span::Plain,
            format!("Код ядерной авторизации: {code}"),
        );
    }
    world.write_message(ss13_tgui::server::OpenUi::new::<AntagHudUi>(
        client, mob, mob, "",
    ));
    info!("{mob} joined the battlecruiser crew (captain: {captain})");
}

/// `summon_battlecruiser(team = null)`: the ship, the team, the willing ghosts in its spawners
/// and the announcement. Returns the ship's level.
pub fn summon_battlecruiser(world: &mut World) -> Result<u16, String> {
    let source = world
        .get_resource::<ss13_mapload::MapLoadPlugin>()
        .map(|m| m.source.clone())
        .ok_or("no map source")?;
    let path = source.join(TEMPLATE);
    let z = match ss13_mapload::load_template(world, &path) {
        Ok(z) => z,
        Err(e) => {
            ss13_world::run_map_init(world);
            return Err(format!("loading {}: {e:#}", path.display()));
        }
    };
    ss13_world::run_map_init(world);
    if !world.contains_resource::<BattlecruiserTeam>() {
        let nuke = selfdestruct(world);
        let code = nuke.and_then(|n| {
            world
                .get::<super::bomb::NuclearBomb>(n)
                .map(|b| b.state.r_code.clone())
        });
        world.insert_resource(BattlecruiserTeam {
            nuke,
            code,
            ..default()
        });
    }
    world.resource_mut::<BattlecruiserTeam>().level_z = Some(z);
    let mut q = world.query::<(Entity, &GhostRoleSpawner, &TilePos)>();
    let mut spawners: Vec<Entity> = q
        .iter(world)
        .filter(|(_, s, p)| p.z == z && is_spawner(s.def.as_deref()))
        .map(|(e, ..)| e)
        .collect();
    spawners.sort();
    for spawner in spawners {
        let Some((ghost, _)) = super::server::willing_ghost(world, super::ROLE_BATTLECRUISER_CREW)
        else {
            break;
        };
        crate::ghost_verbs::take_role(world, ghost, spawner);
    }
    crate::announce::Priority::new(
        "Неопознанный вооруженный корабль обнаружен рядом со станцией.",
        "",
    )
    .send(world);
    info!("the battlecruiser was summoned on z{z}");
    Ok(z)
}

/// The team's round-end report (`roundend_category` "Экипаж боевого крейсера Синдиката").
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let Some(team) = world.get_resource::<BattlecruiserTeam>().cloned() else {
        return Vec::new();
    };
    if team.members.is_empty() {
        return Vec::new();
    }
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = vec![line(vec![ChatSegment::new(
        Span::Bold,
        "Экипаж боевого крейсера Синдиката:",
    )])];
    for &member in &team.members {
        let key = world
            .get::<BattlecruiserCrew>(member)
            .map(|c| c.key.clone())
            .unwrap_or_default();
        out.push(crate::antag::traitor::print_player(world, member, &key));
    }
    let done = world
        .get_resource::<NukeOutcome>()
        .is_some_and(|o| o.station_was_nuked);
    out.push(line(vec![
        ChatSegment::new(Span::Bold, "Задача #1"),
        ChatSegment::new(Span::Plain, format!(": {NUCLEAR_OBJECTIVE} ")),
        if done {
            ChatSegment::new(Span::GreenText, "Успех!")
        } else {
            ChatSegment::new(Span::RedText, "Неудача.")
        },
    ]));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawners_are_the_ships_and_taken_once_per_player() {
        assert!(is_spawner(Some(SPAWNER)));
        assert!(is_spawner(Some(CAPTAIN_SPAWNER)));
        assert!(is_spawner(Some(
            "/obj/effect/mob_spawn/ghost_role/human/syndicate/battlecruiser/assault"
        )));
        assert!(!is_spawner(Some(
            "/obj/effect/mob_spawn/ghost_role/human/beach"
        )));
        assert!(!is_spawner(None));
        let team = BattlecruiserTeam {
            players_spawned: vec!["alice".into()],
            ..default()
        };
        assert!(!may_spawn(Some(&team), "alice"));
        assert!(may_spawn(Some(&team), "bob"));
        assert!(may_spawn(None, "alice"));
    }
}
