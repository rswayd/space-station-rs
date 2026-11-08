//! The paradox clone after tg `/datum/antagonist/paradox_clone` (`paradox_clone.dm`), its
//! objectives (`/datum/objective/accept_no_substitutes` from `evil_clone.dm`,
//! `/datum/objective/paradox_clone_replace`) and the "Paradox Clone" midround ruleset
//! (`/datum/dynamic_ruleset/midround/from_ghosts/paradox_clone`).
//!
//! - **Selection** ([`can_spawn`], [`spawn_clones`], from `super::midround`): a light midround
//!   from ghosts who want [`ROLE_PARADOX_CLONE`]; one clone, a second one 20% of the time
//!   (`bonus_clone_chance`), all of the same original: a random crew member with a client who is
//!   conscious (`find_clone_candidates`). The ruleset needs such a crew member and a maintenance
//!   spawn (`/obj/effect/landmark/generic_maintenance_landmark`).
//! - **The body** ([`spawn_clone`], `make_full_human_copy`): a fresh human on a maintenance
//!   landmark with the original's name, species and body (skin, hair, eyes, colours; a whole
//!   body, as a copy of the DNA), dressed in the original's job outfit (`dress_up_as_job`: ID,
//!   PDA, backpack), its PDA messenger hidden from the lists, a mechanical toolbox in hand "so
//!   they dont get stuck in maints".
//! - **The antagonist** ([`add_clone`]): no station job (`set_assigned_role(paradox_clone)`), the
//!   objectives "nobody named so stays alive" and "take their place" (done while the clone
//!   lives), the greeting, the `paradox_clone` antag HUD icon and tg's default `AntagInfoGeneric`
//!   panel (reopened by the antag info button).
//! - [`roundend_report`]: the header, each clone and its objectives.
// shortcut: no admin pick of the original (`configure_ruleset`), no `is_safe_turf` check of the
// landmark, no zapbang sound; quirks, personalities, age, voice and memories are not copied, and
// there are no suit sensors to switch off.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::IconIndex;
use ss13_mapload::spawner::Rng;
use ss13_net::chat::server::SendChat;
use ss13_net::server::{ClientToken, Networked};
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_tgui::UiInterface;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiSystems};

use super::dynamic::DynamicRng;
use super::objectives::{RoundEnd, all_contents, considered_alive, target_text};
use super::{AntagHudData, AntagHudUi, BrotherInfo, ObjectiveInfo};
use crate::ghost::Ghost;
use crate::health::Stat;
use crate::human::HumanLook;
use crate::inventory::{CarriedBy, Inventory};
use crate::jobs::{Jobs, PlayerJob};
use crate::server::{PlayerMob, SpawnPoints};
use crate::tools::tell;

pub use super::ROLE_PARADOX_CLONE;
/// `name` ("\improper Paradox Clone").
pub const ANTAG_NAME: &str = "Парадокс-клон";
/// `roundend_category`.
pub const ROUNDEND_CATEGORY: &str = "Парадокс-клоны";
/// `antag_hud_name`.
const HUD_STATE: &str = "paradox_clone";
const ANTAG_HUD_DMI: &str = "icons/mob/huds/antag_hud.dmi";
/// `bad_version.put_in_hands(new /obj/item/storage/toolbox/mechanical())`.
pub const TOOLBOX: &str = "/obj/item/storage/toolbox/mechanical";
/// The ruleset's `bonus_clone_chance`: `max_antag_cap += prob(20)`.
pub const BONUS_CLONE_CHANCE: f32 = 20.0;
const OBJECTIVE_NAME: &str = "Задача";

/// A paradox clone's objective.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloneObjective {
    /// `/datum/objective/accept_no_substitutes`: nobody else named `target_name` stays alive.
    AcceptNoSubstitutes { target_name: String },
    /// `/datum/objective/paradox_clone_replace`: `completed || considered_alive(owner)`.
    Replace { name_and_job: String },
}

impl CloneObjective {
    /// `update_explanation_text` / `set_target_name`.
    pub fn explanation(&self) -> String {
        match self {
            Self::AcceptNoSubstitutes { target_name } => {
                format!("Убедитесь, что никто с именем {target_name} не останется в живых.")
            }
            Self::Replace { name_and_job } => format!(
                "Займите место {name_and_job}. Избегайте лишних жертв - помните, ваша миссия - \
                 слиться с толпой!"
            ),
        }
    }

    /// `check_completion` for the clone `owner`.
    pub fn check_completion(&self, world: &mut World, owner: Entity) -> bool {
        match self {
            Self::AcceptNoSubstitutes { target_name } => {
                if target_name.is_empty() {
                    return false;
                }
                // `GLOB.player_list` living mobs (logged out or ghosted ones do not count).
                let mut q = world
                    .query_filtered::<(Entity, &Name, &Stat), (With<PlayerMob>, Without<Ghost>)>();
                !q.iter(world).any(|(e, name, stat)| {
                    e != owner && *stat != Stat::Dead && name.as_str() == target_name.as_str()
                })
            }
            Self::Replace { .. } => considered_alive(world, owner),
        }
    }
}

/// On the clone's body (`/datum/antagonist/paradox_clone`).
#[derive(Component, Clone, Debug)]
pub struct ParadoxClone {
    /// The player's key, for the round-end report.
    pub key: String,
    /// The original's body (`original_mind.current`).
    pub original: Entity,
    pub objectives: Vec<CloneObjective>,
}

impl ParadoxClone {
    fn info(&self) -> BrotherInfo {
        BrotherInfo {
            antag_name: ANTAG_NAME.to_owned(),
            objectives: self
                .objectives
                .iter()
                .enumerate()
                .map(|(i, o)| ObjectiveInfo {
                    count: i as u32 + 1,
                    explanation: o.explanation(),
                    complete: false,
                })
                .collect(),
        }
    }
}

/// The clone's info panel (tg's default `AntagInfoGeneric`).
pub struct ParadoxCloneInfoWindow;

impl UiInterface for ParadoxCloneInfoWindow {
    const NAME: &'static str = crate::antag::cult::AntagInfoGenericUi::NAME;
    type Data = ();
    type StaticData = BrotherInfo;
}

impl ServerUi for ParadoxCloneInfoWindow {
    type Source = &'static ParadoxClone;
    type UserParam = ();
    type Act = ();

    fn static_data(clone: &ParadoxClone) -> BrotherInfo {
        clone.info()
    }

    fn data(_: &ParadoxClone, _: Entity, _: &()) {}
}

pub struct ParadoxClonePlugin;

impl Plugin for ParadoxClonePlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<ParadoxCloneInfoWindow>()
            .add_systems(Update, open_info.in_set(UiSystems::Handle));
    }
}

fn with_rng<R>(world: &mut World, f: impl FnOnce(&mut Rng) -> R) -> R {
    if !world.contains_resource::<DynamicRng>() {
        world.insert_resource(DynamicRng(Rng::new(0)));
    }
    f(&mut world.resource_mut::<DynamicRng>().0)
}

/// `find_clone_candidates`: crew humans with a client who are conscious (not unconscious or in
/// crit), sorted for a deterministic pick.
pub fn clone_candidates(world: &mut World) -> Vec<Entity> {
    let mut q = world.query_filtered::<(Entity, &Stat), (
        With<PlayerMob>,
        With<PlayerJob>,
        With<HumanLook>,
        Without<Ghost>,
        Without<ParadoxClone>,
    )>();
    let mut out: Vec<Entity> = q
        .iter(world)
        .filter(|(_, s)| **s == Stat::Conscious)
        .map(|(e, _)| e)
        .collect();
    out.sort();
    out
}

/// `can_be_selected`: someone to clone and a maintenance spawn.
pub fn can_spawn(world: &mut World) -> bool {
    !clone_candidates(world).is_empty()
        && world
            .get_resource::<SpawnPoints>()
            .is_some_and(|s| !s.maintenance.is_empty())
}

/// `find_clone`: a random crew member to copy (`clone_target_ref`).
pub fn pick_original(world: &mut World) -> Option<Entity> {
    let candidates = clone_candidates(world);
    if candidates.is_empty() {
        return None;
    }
    let i = with_rng(world, |rng| rng.range(0, candidates.len() as i64 - 1)) as usize;
    Some(candidates[i])
}

/// `find_maintenance_spawn(atmos_sensitive = TRUE)`: a random maintenance landmark.
fn maintenance_spawn(world: &mut World) -> Option<TilePos> {
    let tiles = world.get_resource::<SpawnPoints>()?.maintenance.clone();
    if tiles.is_empty() {
        return None;
    }
    let i = with_rng(world, |rng| rng.range(0, tiles.len() as i64 - 1)) as usize;
    Some(tiles[i])
}

/// The ruleset's `execute`: one original (`find_clone`), then a clone of it for each of
/// `clients` (`assign_role`). The clones' bodies; empty when nobody can be cloned or there is no
/// maintenance spawn.
pub fn spawn_clones(
    world: &mut World,
    clients: &[Entity],
    original: Option<Entity>,
) -> Vec<Entity> {
    // The original of the poll, if still there (tg: the ruleset bails when its target is gone).
    let original = match original {
        Some(o) if world.get_entity(o).is_ok() => o,
        Some(_) => return Vec::new(),
        None => {
            let Some(o) = pick_original(world) else {
                return Vec::new();
            };
            o
        }
    };
    let mut out = Vec::new();
    for &client in clients {
        let Some(pos) = maintenance_spawn(world) else {
            break;
        };
        out.push(spawn_clone(world, client, original, pos));
    }
    out
}

/// `make_full_human_copy(location)` + `transfer_to` + `add_antag_datum` + `setup_clone` + the
/// toolbox: `client`'s new body at `pos`, a paradox clone of `original`.
pub fn spawn_clone(world: &mut World, client: Entity, original: Entity, pos: TilePos) -> Entity {
    let name = world
        .get::<Name>(original)
        .map(|n| n.to_string())
        .unwrap_or_default();
    let look = world.get::<HumanLook>(original).cloned();
    let species = crate::species::species_of(world, original);
    let job = world.get::<PlayerJob>(original).map(|j| j.0);
    let name_and_job = target_text(world, original);
    let mob = crate::server::spawn_mob(world, client, pos, name.clone());
    world.entity_mut(client).remove::<crate::round::NewPlayer>();
    // `copy_dna(COPY_DNA_SPECIES)`.
    if species != crate::species::species_of(world, mob) {
        crate::species::set_species(world, mob, species);
    }
    // The DNA's body: a whole one (no husk, lost or robotic limbs), the clothes its own.
    if let (Some(look), Some(mut body)) = (look, world.get_mut::<HumanLook>(mob)) {
        *body = HumanLook {
            species: body.species,
            uniform: body.uniform.clone(),
            shoes: body.shoes.clone(),
            husk: false,
            missing: 0,
            robotic: 0,
            eyeless: false,
            ..look
        };
    }
    dress_up_as_job(world, mob, job, &name);
    let toolbox = crate::construction::spawn_detached(world, TOOLBOX);
    if let Some(toolbox) = toolbox {
        put_in_hands(world, mob, toolbox);
    }
    crate::inventory::refresh_look(world, mob);
    add_clone(world, mob, original, &name, name_and_job);
    info!("{mob} ({client}) is a paradox clone of {original} ({name}) at {pos:?}");
    mob
}

/// `dress_up_as_job(SSjob.get_job(job))`: the job's outfit with its ID and PDA, the backpack;
/// the PDA's messenger is hidden ("clone doesnt show up on message lists").
fn dress_up_as_job(world: &mut World, mob: Entity, job: Option<usize>, name: &str) {
    let def = job.and_then(|j| world.get_resource::<Jobs>()?.data.jobs.get(j).cloned());
    let mut inventory = match &def {
        Some(def) => crate::outfit::equip(world, mob, def, name),
        None => Inventory::default(),
    };
    crate::species::equip_species_outfit(world, mob, &mut inventory);
    inventory.back = crate::storage::spawn_backpack(world, mob);
    if let (Some(pack), Some(outfit)) =
        (inventory.back, def.as_ref().and_then(|d| d.outfit.as_ref()))
    {
        crate::outfit::fill_backpack(world, pack, outfit);
    }
    world.entity_mut(mob).insert(inventory);
    if let Some(def) = &def {
        crate::modpc::pda::post_equip(world, mob, name, &def.assignment);
    }
    for item in all_contents(world, mob) {
        if let Some(mut messenger) =
            world.get_mut::<crate::modpc::programs::messenger::Messenger>(item)
        {
            messenger.invisible = true;
        }
    }
}

/// `put_in_hands`, else on the floor under `mob`.
fn put_in_hands(world: &mut World, mob: Entity, item: Entity) {
    let hand = world.get::<Inventory>(mob).and_then(|i| {
        [i.active, 1 - i.active]
            .into_iter()
            .find(|&h| i.hands[h].is_none())
    });
    match hand {
        Some(h) => {
            world.entity_mut(item).insert((Networked, CarriedBy(mob)));
            if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
                inventory.hands[h] = Some(item);
            }
        }
        None => {
            let pos = world.get::<TilePos>(mob).copied().unwrap_or_default();
            world.entity_mut(item).insert((Networked, pos));
            crate::construction::tile_changed(world, pos);
        }
    }
}

/// `add_antag_datum(/datum/antagonist/paradox_clone)` + `setup_clone(original_mind)`: no station
/// job, the objectives, the greeting, the HUD icon and the info panel. `false` if `mob` already
/// is one.
pub fn add_clone(
    world: &mut World,
    mob: Entity,
    original: Entity,
    target_name: &str,
    name_and_job: String,
) -> bool {
    if world.get::<ParadoxClone>(mob).is_some() || world.get_entity(mob).is_err() {
        return false;
    }
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    // `set_assigned_role(/datum/job/paradox_clone)`: not a crew member.
    crate::noncrew_jobs::set_assigned_role(world, mob, crate::noncrew_jobs::PARADOX_CLONE);
    let objectives = vec![
        CloneObjective::AcceptNoSubstitutes {
            target_name: if target_name.is_empty() {
                "Unknown".to_owned()
            } else {
                target_name.to_owned()
            },
        },
        CloneObjective::Replace { name_and_job },
    ];
    let hud_icon = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(ANTAG_HUD_DMI, HUD_STATE));
    let clone = ParadoxClone {
        key,
        original,
        objectives,
    };
    world.entity_mut(mob).insert((
        clone.clone(),
        AntagHudData {
            hud_icon,
            ..default()
        },
    ));
    if let Some(client) = client {
        // `greet`, then `announce_objectives`.
        // shortcut: `span_big` is drawn bold (no big span in the chat).
        tell(world, client, Span::Bold, format!("Вы - {ANTAG_NAME}."));
        tell(world, client, Span::Notice, "Ваши текущие задачи:");
        for (i, objective) in clone.objectives.iter().enumerate() {
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
        world.write_message(OpenUi::new::<ParadoxCloneInfoWindow>(
            client, mob, mob, ANTAG_NAME,
        ));
    }
    true
}

/// `/datum/action/antag_info/Trigger` of a clone.
fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if world.get::<ParadoxClone>(source).is_some() {
            world.write_message(OpenUi::new::<ParadoxCloneInfoWindow>(
                client, actor, source, ANTAG_NAME,
            ));
        }
    }
}

/// `roundend_report_header` and each clone with its objectives.
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let mut query = world.query::<(Entity, &ParadoxClone)>();
    let mut clones: Vec<(Entity, ParadoxClone)> =
        query.iter(world).map(|(e, c)| (e, c.clone())).collect();
    if clones.is_empty() {
        return Vec::new();
    }
    clones.sort_by_key(|(e, _)| *e);
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = vec![
        line(vec![ChatSegment::new(
            Span::Bold,
            "На станции появился парадокс-клон!",
        )]),
        line(vec![ChatSegment::new(
            Span::Bold,
            format!("{ROUNDEND_CATEGORY} были:"),
        )]),
    ];
    for (mob, clone) in clones {
        out.push(super::traitor::print_player(world, mob, &clone.key));
        let mut won = true;
        for (i, objective) in clone.objectives.iter().enumerate() {
            let done = objective.check_completion(world, mob);
            won &= done;
            out.push(line(vec![
                ChatSegment::new(Span::Bold, format!("{OBJECTIVE_NAME} #{}", i + 1)),
                ChatSegment::new(Span::Plain, format!(": {} ", objective.explanation())),
                if done {
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
    fn no_substitutes_counts_only_other_living_players() {
        let mut world = World::new();
        let client = world.spawn_empty().id();
        let named = |world: &mut World, stat: Stat| {
            world
                .spawn((Name::new("Аарон Адамс"), stat, PlayerMob { client }))
                .id()
        };
        let clone = named(&mut world, Stat::Conscious);
        let objective = CloneObjective::AcceptNoSubstitutes {
            target_name: "Аарон Адамс".into(),
        };
        // Only the clone itself carries the name.
        assert!(objective.check_completion(&mut world, clone));
        let original = named(&mut world, Stat::Conscious);
        assert!(!objective.check_completion(&mut world, clone));
        // Dead, or logged out: done.
        world.entity_mut(original).insert(Stat::Dead);
        assert!(objective.check_completion(&mut world, clone));
        world
            .entity_mut(original)
            .insert(Stat::Conscious)
            .remove::<PlayerMob>();
        assert!(objective.check_completion(&mut world, clone));
        // A ghost named so does not count either.
        world.spawn((
            Name::new("Аарон Адамс"),
            Stat::Conscious,
            PlayerMob { client },
            Ghost { body: None },
        ));
        assert!(objective.check_completion(&mut world, clone));
        // The replacement holds while the clone lives.
        let replace = CloneObjective::Replace {
            name_and_job: "Аарон Адамс, Ассистент".into(),
        };
        assert!(replace.check_completion(&mut world, clone));
        world.entity_mut(clone).insert(Stat::Dead);
        assert!(!replace.check_completion(&mut world, clone));
        assert!(
            replace
                .explanation()
                .starts_with("Займите место Аарон Адамс, Ассистент.")
        );
    }
}
