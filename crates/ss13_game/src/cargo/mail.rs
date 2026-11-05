//! Mail (server), after tg `code/game/objects/items/mail.dm`, `SSeconomy.mail_waiting` and
//! `/obj/docking_port/mobile/supply/create_mail`.
//!
//! - Every second economy tick (`ticks_per_mail`) adds `clamp(round(living / 0.5), 1, 6)` letters
//!   to the waiting mail (no inflation here: `inflation_value` is 1).
//! - Leaving CentCom, the supply shuttle loads a mail crate with the waiting mail (at most the
//!   crate's capacity) unless the station is at red or delta alert (`disables_mail`): 70%
//!   letters, else envelopes (two goodies), each for a living crew member picked once
//!   (`initialize_for_recipient`: named for them, tinted by their department, goodies from the
//!   generic list plus their job's `mail_goodies`, or only the job's when exclusive), the rest
//!   junk mail anyone may open.
//! - Only the recipient opens a letter (`unwrap`, 1.5 s); a letter whose recipient is gone opens
//!   for anyone. The contents go to the hands.
//! - Mail aboard keeps the supply shuttle at the station (`check_blacklist`, [`holds_mail`]).
// shortcut: no mail sorter, mail bag, counterfeit or traitor mail, mail strike, quirk or holiday
// goodies, stamps and postmark overlays, Sunday mail holiday, destination tags on letters, round
// end "mail opened" count; junk mail names have no random adjective (no `GLOB.adjectives` here)
// and the ash letter is empty (letters hold items only).

use bevy::prelude::*;
use ss13_mapload::cargo::{JUNK_MAIL, MAIL, MAIL_CRATE, MAIL_ENVELOPE};
use ss13_mapload::spawner::{DmSpawner, Rng, pick};
use ss13_mapload::{DmCloset, DmItem, DmName, MapSeed};
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::supply::SupplyPacks;
use crate::health::Stat;
use crate::jobs::{Jobs, PlayerJob};
use crate::security_level::SecLevel;
use crate::server::PlayerMob;
use crate::storage::{Stored, StoredIn};
use crate::tools::Actor;

/// `MAX_MAIL_PER_MINUTE`.
const MAX_MAIL_PER_MINUTE: u32 = 3;
/// `ticks_per_mail`.
const TICKS_PER_MAIL: u64 = 2;
/// `FULL_CRATE_LETTER_ODDS`.
const LETTER_ODDS: f32 = 70.0;
/// `unwrap`: 1.5 s.
const UNWRAP_DELAY_DS: f32 = 15.0;
/// Nested spawners deeper than this give nothing.
const MAX_LOOT_DEPTH: u32 = 8;

/// tg `department_colors` (`COLOR_*`), by `paycheck_department`.
const DEPARTMENT_COLORS: [(&str, [u8; 4]); 7] = [
    ("CIV", [0xFF, 0xFF, 0xFF, 0xFF]),
    ("ENG", [0xFF, 0xBE, 0x9D, 0xFF]),
    ("SCI", [0xCB, 0xB1, 0xCA, 0xFF]),
    ("MED", [0x98, 0xC5, 0xDF, 0xFF]),
    ("SRV", [0xB7, 0xD9, 0x93, 0xFF]),
    ("CAR", [0xCE, 0xB6, 0x89, 0xFF]),
    ("SEC", [0xD5, 0x99, 0x98, 0xFF]),
];

/// On a letter: who may open it (`recipient_ref`); `None`: junk mail, anyone.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mail {
    pub recipient: Option<Entity>,
}

/// `/obj/item/mail` `generic_goodies` (from `<assets>/cargo.ron`).
#[derive(Resource, Clone, Debug, Default)]
pub struct MailGoodies(pub Vec<(String, u32)>);

/// `SSeconomy.mail_waiting` and the economy ticks counted for it.
#[derive(Resource)]
pub struct MailState {
    pub waiting: u32,
    fires: u64,
    rng: Rng,
}

pub struct MailPlugin;

impl Plugin for MailPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // A letter is opened by its recipient.
        app.add_click_handler(ClickPhase::AttackSelf, 1400, |w, c| {
            crate::cargo::mail::attack_self(w, c.client, c.mob, c.target)
        });
        app.init_resource::<MailGoodies>()
            .add_systems(Startup, init);
    }
}

fn init(world: &mut World) {
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    world.insert_resource(MailState {
        waiting: 0,
        fires: 0,
        rng: Rng::new(seed ^ 0x6d61_696c),
    });
}

/// Living crew with a job (tg: `GLOB.player_list` humans with a mind and `JOB_CREW_MEMBER`).
fn living_crew(world: &mut World) -> Vec<(Entity, usize)> {
    let mut query = world.query_filtered::<(Entity, &PlayerJob, Option<&Stat>), With<PlayerMob>>();
    let mut out: Vec<(Entity, usize)> = query
        .iter(world)
        .filter(|(_, _, stat)| stat.is_none_or(|s| *s != Stat::Dead))
        .map(|(e, job, _)| (e, job.0))
        .collect();
    out.sort();
    out
}

/// The mail part of `SSeconomy.fire` (from `crate::cargo::economy`).
pub(crate) fn economy_tick(world: &mut World) {
    let living = living_crew(world).len() as u32;
    let Some(mut state) = world.get_resource_mut::<MailState>() else {
        return;
    };
    state.fires += 1;
    if !state.fires.is_multiple_of(TICKS_PER_MAIL) {
        return;
    }
    // `round(living_player_count() / (inflation_value - 0.5))`, `seconds_per_tick` is 1.
    let effective = living * 2;
    state.waiting += effective.clamp(1, TICKS_PER_MAIL as u32 * MAX_MAIL_PER_MINUTE);
}

/// `create_mail`: a mail crate on a free tile of the shuttle among `turfs`, filled.
pub(crate) fn create_mail(world: &mut World, turfs: &mut Vec<TilePos>) {
    let waiting = world.get_resource::<MailState>().map_or(0, |s| s.waiting);
    // `SSeconomy.mail_blocked`: the mail strike station trait.
    if waiting == 0
        || crate::station_traits::has(world, crate::station_traits::MAIL_BLOCKED)
        || turfs.is_empty()
        || matches!(
            crate::security_level::current(world),
            SecLevel::Red | SecLevel::Delta
        )
    {
        return;
    }
    let i = {
        let mut state = world.resource_mut::<MailState>();
        (state.rng.next_u64() % turfs.len() as u64) as usize
    };
    let pos = turfs.swap_remove(i);
    let Some(crate_entity) = super::supply::spawn_template(world, MAIL_CRATE, Some(pos)) else {
        warn!("no {MAIL_CRATE} prototype (rerun `ss13-mapload convert`)");
        return;
    };
    world.entity_mut(crate_entity).insert(Networked);
    let closet = {
        let icons = world.get_resource::<ss13_mapload::IconIndex>();
        let entity = world.entity(crate_entity);
        match (
            entity.get::<DmCloset>(),
            entity.get::<ss13_mapload::DmAtom>(),
        ) {
            (Some(dm), Some(atom)) => Some(crate::closets::Closet::from_dm(dm, atom, icons)),
            _ => None,
        }
    };
    let capacity = world
        .get::<DmCloset>(crate_entity)
        .map_or(30, |c| c.storage_capacity);
    if let Some(mut closet) = closet {
        closet.opened = false;
        closet.contents_initialized = true;
        world.entity_mut(crate_entity).insert(closet);
        crate::closets::refresh_look(world, crate_entity);
    }
    populate(world, crate_entity, waiting.min(capacity));
    world.resource_mut::<MailState>().waiting = 0;
}

/// `crate/mail/populate`.
pub fn populate(world: &mut World, container: Entity, amount: u32) {
    let mut recipients = living_crew(world);
    for _ in 0..amount {
        let (letter, pick_index) = {
            let mut state = world.resource_mut::<MailState>();
            let letter = state.rng.prob(LETTER_ODDS);
            let pick_index = (!recipients.is_empty())
                .then(|| (state.rng.next_u64() % recipients.len() as u64) as usize);
            (letter, pick_index)
        };
        let path = if letter { MAIL } else { MAIL_ENVELOPE };
        let Some(mail) = super::supply::spawn_template(world, path, None) else {
            continue;
        };
        world
            .entity_mut(mail)
            .insert((Networked, StoredIn(container)));
        crate::storage::finish_item(world, mail, 1);
        match pick_index.map(|i| recipients.swap_remove(i)) {
            Some((mob, job)) => initialize_for_recipient(world, mail, mob, job, !letter),
            None => junk_mail(world, mail),
        }
    }
}

fn set_color(world: &mut World, mail: Entity, color: [u8; 4]) {
    if let Some(mut look) = world.get_mut::<ss13_shared::Appearance>(mail) {
        look.color = Some(color);
    }
}

/// `initialize_for_recipient`.
fn initialize_for_recipient(
    world: &mut World,
    mail: Entity,
    mob: Entity,
    job: usize,
    envelope: bool,
) {
    let job_def = world
        .get_resource::<Jobs>()
        .and_then(|j| j.data.jobs.get(job).cloned());
    let title = job_def
        .as_ref()
        .map(|j| j.title.clone())
        .unwrap_or_default();
    let department = job_def
        .as_ref()
        .map(|j| j.paycheck_department.clone())
        .unwrap_or_default();
    let name = world
        .get::<Name>(mob)
        .map(|n| n.as_str().to_owned())
        .or_else(|| world.get::<DmName>(mob).map(|n| n.name.clone()))
        .unwrap_or_default();
    let base = world
        .get::<DmName>(mail)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    let desc = world
        .get::<DmName>(mail)
        .map(|n| n.desc.clone())
        .unwrap_or_default();
    let job_ru = ss13_tgui::interfaces::ru_jobs::job_ru(&title).unwrap_or(title.as_str());
    world.entity_mut(mail).insert((
        DmName {
            name: format!("{base} для {name}, ({job_ru})"),
            desc,
        },
        Mail {
            recipient: Some(mob),
        },
    ));
    if let Some((_, color)) = DEPARTMENT_COLORS.iter().find(|(d, _)| *d == department) {
        set_color(world, mail, *color);
    }
    let job_goodies = world
        .get_resource::<super::bounty::Bounties>()
        .and_then(|b| b.jobs.iter().find(|j| j.title == title).cloned());
    let mut goodies: Vec<(String, u32)> = match &job_goodies {
        Some(j) if j.exclusive_mail_goodies && !j.mail_goodies.is_empty() => Vec::new(),
        _ => world.resource::<MailGoodies>().0.clone(),
    };
    if let Some(j) = job_goodies {
        goodies.extend(j.mail_goodies);
    }
    let count = if envelope { 2 } else { 1 };
    for _ in 0..count {
        let Some(path) = pick_weight(world, &goodies) else {
            break;
        };
        for leaf in loot_of(world, &path) {
            spawn_in(world, &leaf, mail);
        }
    }
}

/// `junk_mail`: a junk paper (25%: a special one, named for it), anyone opens it.
fn junk_mail(world: &mut World, mail: Entity) {
    let (special, color) = {
        let mut state = world.resource_mut::<MailState>();
        let special = state
            .rng
            .prob(25.0)
            .then(|| 1 + (state.rng.next_u64() % (JUNK_MAIL.len() as u64 - 1)) as usize);
        let color =
            DEPARTMENT_COLORS[(state.rng.next_u64() % DEPARTMENT_COLORS.len() as u64) as usize].1;
        (special, color)
    };
    let (junk, suffix) = JUNK_MAIL[special.unwrap_or(0)];
    let base = world
        .get::<DmName>(mail)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    let desc = world
        .get::<DmName>(mail)
        .map(|n| n.desc.clone())
        .unwrap_or_default();
    let name = if special.is_some() {
        format!("{base} {}", suffix.replace("%A ", ""))
    } else {
        format!("important {base}")
    };
    world
        .entity_mut(mail)
        .insert((DmName { name, desc }, Mail { recipient: None }));
    set_color(world, mail, color);
    spawn_in(world, junk, mail);
}

fn pick_weight(world: &mut World, list: &[(String, u32)]) -> Option<String> {
    let total: u64 = list.iter().map(|(_, w)| u64::from(*w)).sum();
    if total == 0 {
        return None;
    }
    let mut roll = world.resource_mut::<MailState>().rng.next_u64() % total;
    for (path, w) in list {
        if roll < u64::from(*w) {
            return Some(path.clone());
        }
        roll -= u64::from(*w);
    }
    None
}

/// What `path` makes: itself, or a spawner's loot (`spawn_loot`, as at map load).
fn loot_of(world: &mut World, path: &str) -> Vec<String> {
    let mut out = Vec::new();
    world.resource_scope(|world, mut state: Mut<MailState>| {
        if let Some(packs) = world.get_resource::<SupplyPacks>() {
            loot(packs, path, &mut state.rng, 0, &mut out);
        }
    });
    out
}

fn loot(packs: &SupplyPacks, path: &str, rng: &mut Rng, depth: u32, out: &mut Vec<String>) {
    let spawner = packs
        .templates
        .get(path)
        .and_then(|t| t.proto.get::<DmSpawner>())
        .cloned();
    let Some(s) = spawner else {
        out.push(path.to_owned());
        return;
    };
    if depth > MAX_LOOT_DEPTH || !s.unresolved.is_empty() {
        return;
    }
    if !s.random {
        for e in s
            .loot
            .iter()
            .filter(|e| !e.path.is_empty() && e.parent.is_none())
        {
            loot(packs, &e.path, rng, depth + 1, out);
        }
        return;
    }
    if !rng.prob(s.chance) {
        return;
    }
    let count = if s.all {
        s.loot.len()
    } else {
        s.count as usize
    };
    let mut pool: Vec<usize> = (0..s.loot.len())
        .filter(|&i| s.loot[i].parent.is_none())
        .collect();
    for _ in 0..count {
        if pool.is_empty() {
            break;
        }
        let Some((top, leaf)) = pick(&s.loot, &pool, rng) else {
            continue;
        };
        if !s.double {
            pool.remove(top);
        }
        loot(packs, &s.loot[leaf].path, rng, depth + 1, out);
    }
}

/// An item of `path` inside `mail` (non-items are not put in letters here).
fn spawn_in(world: &mut World, path: &str, mail: Entity) {
    let is_item = world
        .get_resource::<SupplyPacks>()
        .and_then(|p| p.templates.get(path))
        .is_some_and(|t| t.proto.get::<DmItem>().is_some());
    if !is_item {
        return;
    }
    if let Some(item) = super::supply::spawn_template(world, path, None) {
        world.entity_mut(item).insert((Networked, StoredIn(mail)));
        crate::storage::finish_item(world, item, 2);
    }
}

/// `attack_self`: someone else's mail stays shut; otherwise it unwraps in 1.5 s
/// (`crate::disposals` completes it: the contents to the hands, the letter gone).
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(mail) = world.get::<Mail>(item).copied() else {
        return false;
    };
    // A recipient that no longer exists lets anyone open it.
    if let Some(recipient) = mail.recipient
        && recipient != mob
        && world.get_entity(recipient).is_ok()
    {
        crate::tools::tell(
            world,
            client,
            Span::Notice,
            "You can't open somebody else's mail! That's illegal!",
        );
        return true;
    }
    crate::tools::tell(world, client, Span::Notice, "unwrapping...");
    crate::tools::do_after(
        world,
        Actor {
            client,
            mob,
            item: Some(item),
        },
        mob,
        UNWRAP_DELAY_DS,
        0,
        crate::construction::Job::Disposal(crate::disposals::DisposalJob::Unwrap { parcel: item }),
    );
    true
}

/// `check_blacklist` for mail: `thing` is a letter or holds one.
pub fn holds_mail(world: &World, thing: Entity) -> bool {
    let mut stack = vec![thing];
    let mut seen = 0;
    while let Some(e) = stack.pop() {
        if world.get::<Mail>(e).is_some() {
            return true;
        }
        seen += 1;
        if seen > 1024 {
            break;
        }
        if let Some(stored) = world.get::<Stored>(e) {
            stack.extend_from_slice(stored.items());
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mail_waits_every_second_tick() {
        let mut world = World::new();
        world.insert_resource(MailState {
            waiting: 0,
            fires: 0,
            rng: Rng::new(1),
        });
        economy_tick(&mut world);
        assert_eq!(world.resource::<MailState>().waiting, 0, "odd tick");
        economy_tick(&mut world);
        assert_eq!(world.resource::<MailState>().waiting, 1, "at least one");
        for _ in 0..3 {
            world.spawn((
                PlayerMob {
                    client: Entity::PLACEHOLDER,
                },
                PlayerJob(0),
            ));
        }
        world.spawn((
            PlayerMob {
                client: Entity::PLACEHOLDER,
            },
            PlayerJob(0),
            Stat::Dead,
        ));
        economy_tick(&mut world);
        economy_tick(&mut world);
        assert_eq!(
            world.resource::<MailState>().waiting,
            1 + 6,
            "3 living x 2, at most 6"
        );
    }

    #[test]
    fn mail_is_blacklisted_inside_containers() {
        let mut world = World::new();
        let crate_entity = world.spawn_empty().id();
        let letter = world
            .spawn((Mail { recipient: None }, StoredIn(crate_entity)))
            .id();
        assert!(holds_mail(&world, crate_entity));
        assert!(holds_mail(&world, letter));
        let empty = world.spawn_empty().id();
        assert!(!holds_mail(&world, empty));
    }
}
