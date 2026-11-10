//! Raiders: the antagonists that come to the station from space (tg
//! `code/modules/antagonists/{space_ninja,pirate,fugitive}`): the space ninja ([`ninja`]), the
//! pirates ([`pirate`]) and the fugitives with their hunters ([`fugitive`]). This module holds
//! what they share: the converted outfits and names ([`RaiderData`]), equipping an outfit with
//! its back item ([`equip_outfit`]), the objective printout and the timed steps
//! ([`RaiderJob`]).

pub mod fugitive;
pub mod ninja;
pub mod pirate;

use bevy::prelude::*;
use ss13_mapload::jobs::{Outfit, OutfitSlot};
use ss13_mapload::spawner::Rng;
use ss13_mapload::{MapLoadPlugin, Template};
use ss13_net::chat::server::SendChat;
use ss13_net::server::Networked;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};

use super::dynamic::DynamicRng;
use crate::inventory::{CarriedBy, Inventory};
use crate::outfit::OutfitTemplates;
use crate::tools::{DoAfter, tell};

/// The converted outfits and name lists ([`ss13_mapload::antag::AntagData`]).
#[derive(Resource, Default, Debug)]
pub struct RaiderData {
    pub outfits: Vec<Outfit>,
    pub ninja_titles: Vec<String>,
    pub ninja_names: Vec<String>,
    pub ninja_lines: Vec<String>,
    pub pirate_names: Vec<(String, Vec<String>)>,
}

impl RaiderData {
    pub fn outfit(&self, path: &str) -> Option<&Outfit> {
        self.outfits.iter().find(|o| o.path == path)
    }

    /// `strings(PIRATE_NAMES_FILE, key)`.
    pub fn pirate_pool(&self, key: &str) -> &[String] {
        self.pirate_names
            .iter()
            .find(|(k, _)| k == key)
            .map_or(&[], |(_, v)| v.as_slice())
    }
}

pub struct RaidersPlugin;

impl Plugin for RaidersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RaiderData>()
            .add_systems(Startup, load_data)
            .add_plugins((
                ninja::NinjaPlugin,
                pirate::PiratePlugin,
                fugitive::FugitivePlugin,
            ));
    }
}

fn load_data(world: &mut World) {
    let Some(assets) = world
        .get_resource::<MapLoadPlugin>()
        .map(|m| m.assets.clone())
    else {
        return;
    };
    let Ok(data) =
        ss13_mapload::antag::AntagData::read(&assets.join(ss13_mapload::antag::ANTAG_FILE))
    else {
        return;
    };
    let manifest = ss13_mapload::read_manifest(&assets).unwrap_or_default();
    let index = manifest.index();
    let mut added = Vec::new();
    {
        let have = world.get_resource::<OutfitTemplates>();
        let protos = world.get_resource::<ss13_proto::Prototypes>();
        for path in data.raider_types() {
            if have.is_some_and(|t| t.0.contains_key(path)) {
                continue;
            }
            if let Some(proto) = protos.and_then(|p| p.get(path).cloned()) {
                added.push((path.to_owned(), Template::new(proto, &index)));
            }
        }
    }
    if !world.contains_resource::<OutfitTemplates>() {
        world.init_resource::<OutfitTemplates>();
    }
    world.resource_mut::<OutfitTemplates>().0.extend(added);
    world.insert_resource(RaiderData {
        outfits: data.raider_outfits,
        ninja_titles: data.ninja_titles,
        ninja_names: data.ninja_names,
        ninja_lines: data.ninja_lines,
        pirate_names: data.pirate_names,
    });
}

/// Dynamic's generator (the raiders are its rulesets).
pub(crate) fn with_rng<R>(world: &mut World, f: impl FnOnce(&mut Rng) -> R) -> R {
    if !world.contains_resource::<DynamicRng>() {
        world.insert_resource(DynamicRng(Rng::new(0)));
    }
    f(&mut world.resource_mut::<DynamicRng>().0)
}

/// `pick(list)`; `None` for an empty list.
pub(crate) fn pick<T: Clone>(world: &mut World, list: &[T]) -> Option<T> {
    if list.is_empty() {
        return None;
    }
    let i = with_rng(world, |rng| rng.range(0, list.len() as i64 - 1)) as usize;
    list.get(i).cloned()
}

/// `/datum/outfit/equip` of a raider outfit: its slots (the back item too: a backpack gets
/// `backpack_contents`, anything else, like the ninja's MODsuit, is spawned as itself) and the
/// species outfit. The mob's inventory is replaced; `None` without the outfit's data.
pub fn equip_outfit(world: &mut World, mob: Entity, path: &str) -> Option<Inventory> {
    let outfit = world.get_resource::<RaiderData>()?.outfit(path)?.clone();
    Some(equip_with(world, mob, &outfit))
}

/// [`equip_outfit`] of an outfit already in hand.
pub fn equip_with(world: &mut World, mob: Entity, outfit: &Outfit) -> Inventory {
    if !world.contains_resource::<OutfitTemplates>() {
        world.init_resource::<OutfitTemplates>();
    }
    let mut inventory = world.resource_scope(|world, templates: Mut<OutfitTemplates>| {
        crate::outfit::equip_outfit(world, mob, outfit, &templates.0)
    });
    crate::species::equip_species_outfit(world, mob, &mut inventory);
    match outfit.item(OutfitSlot::Back) {
        Some(back) if is_backpack(back) => {
            inventory.back = crate::storage::spawn_backpack(world, mob);
            if let Some(pack) = inventory.back {
                crate::outfit::fill_backpack(world, pack, outfit);
            }
        }
        Some(back) => {
            if let Some(item) = crate::construction::spawn_detached(world, back) {
                world.entity_mut(item).insert((Networked, CarriedBy(mob)));
                // A MODsuit is set up now: `post_equip` reads its modules.
                crate::modsuit::server::init_suit(world, item);
                inventory.back = Some(item);
            }
        }
        None => {}
    }
    world.entity_mut(mob).insert(inventory.clone());
    crate::inventory::refresh_look(world, mob);
    inventory
}

fn is_backpack(path: &str) -> bool {
    crate::obj_damage::is_under(path, "/obj/item/storage/backpack")
}

/// `announce_objectives`: "Ваши текущие задачи:" and each numbered objective.
pub(crate) fn announce_objectives(world: &mut World, client: Entity, objectives: &[String]) {
    tell(world, client, Span::Notice, "Ваши текущие задачи:");
    for (i, text) in objectives.iter().enumerate() {
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage {
                channel: ChatChannel::System,
                segments: vec![
                    ChatSegment::new(Span::Bold, format!("Задача #{}", i + 1)),
                    ChatSegment::new(Span::Plain, format!(": {text}")),
                ],
            },
        });
    }
}

/// One line of a report.
pub(crate) fn line(segments: Vec<ChatSegment>) -> ChatMessage {
    ChatMessage {
        channel: ChatChannel::System,
        segments,
    }
}

/// A round-end objective line: "Задача #n: text Успех!/Неудача.".
pub(crate) fn objective_line(n: usize, text: &str, done: bool) -> ChatMessage {
    line(vec![
        ChatSegment::new(Span::Bold, format!("Задача #{n}")),
        ChatSegment::new(Span::Plain, format!(": {text} ")),
        if done {
            ChatSegment::new(Span::GreenText, "Успех!")
        } else {
            ChatSegment::new(Span::RedText, "Неудача.")
        },
    ])
}

/// The player's key of `mob` (for round-end reports).
pub(crate) fn key_of(world: &World, mob: Entity) -> String {
    world
        .get::<crate::server::PlayerMob>(mob)
        .and_then(|p| world.get::<ss13_net::server::ClientToken>(p.client))
        .map(|t| t.0.clone())
        .unwrap_or_default()
}

/// The timed steps of the raiders (`crate::construction::Job::Raiders`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RaiderJob {
    /// A hacker module's drain or hack of the target ([`ninja`]).
    Hack(ninja::HackJob),
    /// A fugitive dragged into the capture machine `machine` ([`fugitive`]).
    Capture { machine: Entity },
}

/// The end of a [`RaiderJob`]'s `do_after`.
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: RaiderJob) {
    match job {
        RaiderJob::Hack(job) => ninja::complete_hack(world, mob, action, job),
        RaiderJob::Capture { machine } => {
            fugitive::complete_capture(world, mob, action.target, machine)
        }
    }
}

/// Every raider team's round-end lines.
pub fn roundend_report(world: &mut World, end: super::objectives::RoundEnd) -> Vec<ChatMessage> {
    let mut out = ninja::roundend_report(world, end);
    out.extend(pirate::roundend_report(world, end));
    out.extend(fugitive::roundend_report(world, end));
    out
}
