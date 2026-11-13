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
//! - The mail bag holds only letters, small parcels, papers and bounty cubes ([`bag_refuses`]);
//!   the mail sorter is [`super::mail_sorter`].
//! - The GLA-2 counterfeit device (`/obj/item/storage/mail_counterfeit_device`, from the uplink):
//!   in hand it asks mail or envelope, armed or not, and a recipient (a crew member, or "Anyone"
//!   with a free title); the device becomes a letter holding what was put in it. Opening armed
//!   counterfeit mail puts its contents in the hands and uses them (`attack_self`: a grenade
//!   primes).
// shortcut: no mail strike letters (the event is ../events), quirk or holiday goodies, stamps and
// postmark overlays, Sunday mail holiday, destination tags on letters, round end "mail opened"
// count, multitool disarming of counterfeit mail; junk mail names have no random adjective (no
// `GLOB.adjectives` here) and the ash letter is empty (letters hold items only).

use bevy::prelude::*;
use ss13_mapload::cargo::{JUNK_MAIL, MAIL, MAIL_BAG, MAIL_CRATE, MAIL_ENVELOPE};
use ss13_mapload::spawner::Rng;
use ss13_mapload::{DmCloset, DmItem, DmName, MapSeed};
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use super::supply::SupplyPacks;
use crate::health::Stat;
use crate::jobs::{Jobs, PlayerJob};
use crate::obj_damage::is_under;
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
        // The counterfeit device asks how to dress its contents.
        app.add_click_handler(ClickPhase::AttackSelf, 3165, |w, c| {
            crate::cargo::mail::counterfeit_attack_self(w, c.client, c.mob, c.target)
        });
        app.init_resource::<MailGoodies>()
            .add_systems(Startup, init)
            .add_systems(
                Update,
                counterfeit_answers
                    .in_set(ss13_tgui::server::UiSystems::Handle)
                    .after(ss13_tgui::prompt::PromptSystems::Answer),
            );
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
            Some((mob, job)) => {
                initialize_for_recipient(world, mail, mob, job, if letter { 1 } else { 2 })
            }
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
    goodie_count: u32,
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
    for _ in 0..goodie_count {
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

/// What `path` makes: itself, or a spawner's loot.
fn loot_of(world: &mut World, path: &str) -> Vec<String> {
    let mut out = Vec::new();
    world.resource_scope(|world, mut state: Mut<MailState>| {
        if let Some(packs) = world.get_resource::<SupplyPacks>() {
            out = super::supply::loot_of(packs, path, &mut state.rng);
        }
    });
    out
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

/// `/datum/storage/bag/mail` `set_holdable`: what a mail bag refuses.
pub fn bag_refuses(world: &World, storage: Entity, item: Entity) -> bool {
    const HOLDS: [&str; 4] = [
        "/obj/item/mail",
        "/obj/item/delivery/small",
        "/obj/item/paper",
        ss13_mapload::cargo::BOUNTY_CUBE,
    ];
    let is_bag = world
        .get::<DmItem>(storage)
        .is_some_and(|i| is_under(&i.path, MAIL_BAG));
    is_bag
        && !world
            .get::<DmItem>(item)
            .is_some_and(|i| HOLDS.iter().any(|root| is_under(&i.path, root)))
}

/// On counterfeit mail (`/obj/item/mail/traitor`): `armed` makes its contents go off on opening.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraitorMail {
    pub armed: bool,
}

/// From `crate::disposals` after a letter is unwrapped into `mob`'s hands: armed counterfeit
/// mail uses each item (`after_unwrap`: `attack_self`).
pub(crate) fn after_unwrap(world: &mut World, mob: Entity, armed: bool, items: &[Entity]) {
    if !armed {
        return;
    }
    let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) else {
        return;
    };
    let Some(&mob_pos) = world.get::<TilePos>(mob) else {
        return;
    };
    for &item in items {
        let Some(inventory) = world.get::<crate::inventory::Inventory>(mob) else {
            return;
        };
        let Some(hand) = inventory.hands.iter().position(|h| *h == Some(item)) else {
            continue;
        };
        info!("cargo: armed counterfeit mail opened by {mob}, activating {item}");
        let ctx = crate::click_handlers::ClickCtx {
            client,
            mob,
            mob_pos,
            held: Some(item),
            target: item,
            pos: mob_pos,
            modifiers: ss13_shared::protocol::ClickModifiers::default(),
            active_hand: hand,
        };
        crate::click_handlers::run(world, crate::click_handlers::ClickPhase::AttackSelf, &ctx);
    }
}

/// `/obj/item/mail/traitor` `armed` of `parcel`.
pub(crate) fn is_armed(world: &World, parcel: Entity) -> bool {
    world.get::<TraitorMail>(parcel).is_some_and(|t| t.armed)
}

const COUNTERFEIT_DEVICE: &str = "/obj/item/storage/mail_counterfeit_device";
/// `MAX_LABEL_LEN`.
const MAX_LABEL_LEN: u32 = 64;

/// The counterfeit device's questions, on their prompt.
#[derive(Component, Clone, Debug)]
enum Counterfeit {
    Kind(Entity),
    Armed(Entity, bool),
    /// The recipients offered after "Anyone".
    Recipient(Entity, bool, bool, Vec<(Entity, usize)>),
    Title(Entity, bool, bool),
}

impl Counterfeit {
    fn device(&self) -> Entity {
        match self {
            Self::Kind(d) | Self::Armed(d, _) | Self::Recipient(d, ..) | Self::Title(d, ..) => *d,
        }
    }
}

fn mob_name(world: &World, mob: Entity) -> String {
    world
        .get::<Name>(mob)
        .map(|n| n.as_str().to_owned())
        .or_else(|| world.get::<DmName>(mob).map(|n| n.name.clone()))
        .unwrap_or_default()
}

/// `mail_counterfeit_device/attack_self`: the first question.
pub fn counterfeit_attack_self(
    world: &mut World,
    client: Entity,
    mob: Entity,
    item: Entity,
) -> bool {
    if !world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, COUNTERFEIT_DEVICE))
    {
        return false;
    }
    let buttons = vec!["Mail".to_owned(), "Envelope".to_owned()];
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        ss13_tgui::prompt::Ask::new(
            client,
            mob,
            "Mail Counterfeiting",
            "Make it look like an envelope or like normal mail?",
            ss13_tgui::prompt::PromptKind::Alert {
                buttons,
                autofocus: true,
            },
        ),
    );
    world.entity_mut(prompt).insert(Counterfeit::Kind(item));
    true
}

fn counterfeit_ask(
    world: &mut World,
    client: Entity,
    mob: Entity,
    message: &str,
    kind: ss13_tgui::prompt::PromptKind,
    next: Counterfeit,
) {
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        ss13_tgui::prompt::Ask::new(client, mob, "Mail Counterfeiting", message, kind),
    );
    world.entity_mut(prompt).insert(next);
}

/// The device's answers, each checked against the device still being in the asker's hands
/// (tg `loc != user`).
fn counterfeit_answers(
    world: &mut World,
    mut cursor: Local<bevy::ecs::message::MessageCursor<ss13_tgui::prompt::PromptAnswered>>,
) {
    use ss13_tgui::prompt::{PromptAnswer, PromptAnswered, PromptKind};
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<bevy::ecs::message::Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answers {
        let Some(stage) = world.get::<Counterfeit>(prompt).cloned() else {
            continue;
        };
        let device = stage.device();
        let held = world
            .get::<crate::inventory::CarriedBy>(device)
            .is_some_and(|c| c.0 == actor);
        let Some(answer) = answer.filter(|_| held) else {
            continue;
        };
        match (stage, answer) {
            (Counterfeit::Kind(_), PromptAnswer::Button(i)) => counterfeit_ask(
                world,
                client,
                actor,
                "Arm it?",
                PromptKind::Alert {
                    buttons: vec!["Yes".to_owned(), "No".to_owned()],
                    autofocus: true,
                },
                Counterfeit::Armed(device, i == 1),
            ),
            (Counterfeit::Armed(_, envelope), PromptAnswer::Button(i)) => {
                let crew = living_crew(world);
                let mut items = vec!["Anyone".to_owned()];
                items.extend(crew.iter().map(|&(mob, _)| mob_name(world, mob)));
                counterfeit_ask(
                    world,
                    client,
                    actor,
                    "Choose a recipient",
                    PromptKind::List {
                        items,
                        default: None,
                    },
                    Counterfeit::Recipient(device, envelope, i == 0, crew),
                );
            }
            (Counterfeit::Recipient(_, envelope, armed, crew), PromptAnswer::Item(i)) => {
                if i == 0 {
                    counterfeit_ask(
                        world,
                        client,
                        actor,
                        "Enter mail title, or leave it blank",
                        PromptKind::Text {
                            default: String::new(),
                            max_length: MAX_LABEL_LEN,
                            multiline: false,
                        },
                        Counterfeit::Title(device, envelope, armed),
                    );
                } else if let Some(&(mob, job)) = crew.get(i - 1)
                    && world.get_entity(mob).is_ok()
                {
                    counterfeit(world, actor, device, envelope, armed, Ok((mob, job)));
                }
            }
            (Counterfeit::Title(_, envelope, armed), PromptAnswer::Text(title)) => {
                counterfeit(world, actor, device, envelope, armed, Err(title));
            }
            _ => {}
        }
    }
}

/// The end of `attack_self`: the device becomes a counterfeit letter in the hands, for a
/// recipient (`Ok`) or anyone under a title (`Err`, blank: "mail"/"envelope").
fn counterfeit(
    world: &mut World,
    user: Entity,
    device: Entity,
    envelope: bool,
    armed: bool,
    recipient: Result<(Entity, usize), String>,
) {
    use ss13_mapload::cargo::{TRAITOR_ENVELOPE, TRAITOR_MAIL};
    let path = if envelope {
        TRAITOR_ENVELOPE
    } else {
        TRAITOR_MAIL
    };
    let Some(mail) = crate::construction::spawn_detached(world, path) else {
        warn!("no {path} prototype (rerun `ss13-mapload convert`)");
        return;
    };
    world
        .entity_mut(mail)
        .insert((Networked, TraitorMail { armed }));
    match recipient {
        Ok((mob, job)) => initialize_for_recipient(world, mail, mob, job, 0),
        Err(title) => {
            let title = title.trim();
            let name = if title.is_empty() {
                if envelope { "envelope" } else { "mail" }.to_owned()
            } else {
                title.to_owned()
            };
            let desc = world
                .get::<DmName>(mail)
                .map(|n| n.desc.clone())
                .unwrap_or_default();
            world
                .entity_mut(mail)
                .insert((DmName { name, desc }, Mail { recipient: None }));
        }
    }
    let inside: Vec<Entity> = world
        .get::<Stored>(device)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    for item in inside {
        crate::storage::put_inside(world, item, mail);
    }
    if let Some(mut inventory) = world.get_mut::<crate::inventory::Inventory>(user)
        && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(device))
    {
        *slot = None;
    }
    world.despawn(device);
    if let Some(inventory) = world.get::<crate::inventory::Inventory>(user).cloned() {
        crate::storage::put_in_hands(world, user, &inventory, mail);
    }
    info!("cargo: {user} made counterfeit mail (armed: {armed})");
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
