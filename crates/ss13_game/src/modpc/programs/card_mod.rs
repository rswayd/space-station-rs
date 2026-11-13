//! Plexagon Access Management (server) after tg `/datum/computer_file/program/card_mod`
//! (`code/modules/modular_computers/file_system/programs/card.dm`) on the identification
//! console: the main ID slot authenticates, the secondary slot holds the card being edited.
//!
//! Every edit is checked at the moment of effect against the authorizing card as it is then:
//! it must still be in the main slot, and its current access is authenticated again (tg keeps
//! the login after the card leaves and never re-reads its access); a card that left or lost its
//! rights logs the program out.
// shortcut: no access report printing (`PRG_print`), no admin alerts or ID change logs, no
// CentCom variant (`is_centcom`) or departmental consoles (`target_dept`); job templates are the
// joinable jobs (tg: every station job trim with `template_access`).

use bevy::prelude::*;
use ss13_mapload::{DmItem, DmName};
use ss13_shared::chat::Span;

use super::super::*;
use crate::id_access::{
    ACCESS_FLAG_NAMES, AddMode, CardAccess, CardModAuth, STATION_REGIONS, Trim, WILDCARD_FLAGS,
    access_desc, access_flag, add_access, apply_template, authenticate, remove_access, remove_trim,
};
use crate::inventory::{IdCard, Inventory};
use crate::jobs::Jobs;
use crate::modpc::pda::{ModularComputer, insert_id, play, remove_id};
use crate::outfit::{IdRegistration, id_label};

/// `MAX_NAME_LEN`.
const MAX_NAME_LEN: usize = 42;
/// `card_mod/ui_act` sounds, all at 50 without variation.
const TERMINAL_ON: &str = "sound/machines/terminal/terminal_on.ogg";
const TERMINAL_OFF: &str = "sound/machines/terminal/terminal_off.ogg";
const PROMPT_DENY: &str = "sound/machines/terminal/terminal_prompt_deny.ogg";
/// `SFX_TERMINAL_TYPE`.
const TERMINAL_TYPE: &str = "terminal_type";

/// The program's login: `authenticated_card`, `authenticated_user`, `minor`, `region_access`,
/// `valid_access`, `job_templates`, and which card logged in.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct CardModState {
    pub login: Option<CardModLogin>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardModLogin {
    pub card: Entity,
    pub authenticated_card: String,
    pub authenticated_user: String,
    pub auth: CardModAuth,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardAct {
    Authenticate,
    Logout,
    Terminate,
    Edit(String),
    Age(u32),
    Assign(String),
    Access {
        target: String,
        wildcard: Option<String>,
    },
    Template(String),
    InsertMain,
    RemoveMain,
    InsertAlt,
    RemoveAlt,
}

/// The `NtosCard` acts; `None` for other actions.
pub fn parse(
    action: &str,
    get: impl Fn(&str) -> Option<String>,
) -> Option<Result<CardAct, String>> {
    let text = |k: &str| get(k).unwrap_or_default();
    Some(Ok(match action {
        PRG_AUTHENTICATE => CardAct::Authenticate,
        PRG_LOGOUT => CardAct::Logout,
        PRG_TERMINATE => CardAct::Terminate,
        PRG_EDIT => CardAct::Edit(text("name")),
        PRG_AGE => match text("id_age").parse() {
            Ok(age) => CardAct::Age(age),
            Err(_) => return Some(Err("id_age: expected a number".into())),
        },
        PRG_ASSIGN => CardAct::Assign(text("assignment")),
        PRG_ACCESS => match get("access_target") {
            Some(target) => CardAct::Access {
                target,
                wildcard: get("access_wildcard").filter(|w| !w.is_empty()),
            },
            None => return Some(Err("access_target: missing".into())),
        },
        PRG_TEMPLATE => CardAct::Template(text("name")),
        PRG_INSERT_MAIN_ID => CardAct::InsertMain,
        PRG_REMOVE_MAIN_ID => CardAct::RemoveMain,
        PRG_INSERT_ALT_ID => CardAct::InsertAlt,
        PRG_REMOVE_ALT_ID => CardAct::RemoveAlt,
        _ => return None,
    }))
}

/// What the window needs of one card.
pub type CardParts<'a> = (
    &'a IdCard,
    Option<&'a CardAccess>,
    Option<&'a IdRegistration>,
    Option<&'a DmName>,
);

/// `get_id_ui_data(card)`.
pub fn card_ui((card, data, reg, name): CardParts) -> CardUi {
    let trim = data.and_then(|d| d.trim.as_ref());
    CardUi {
        id_name: name.map(|n| n.name.clone()).unwrap_or_default(),
        id_rank: reg
            .map(|r| r.assignment.clone())
            .filter(|a| !a.is_empty())
            .unwrap_or_else(|| "Unassigned".to_owned()),
        id_owner: reg
            .map(|r| r.registered_name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "-----".to_owned()),
        access_on_card: card.access.clone(),
        wildcard_slots: data
            .map(|d| {
                d.wildcard_slots
                    .iter()
                    .map(|s| WildcardSlotUi {
                        name: s.name.clone(),
                        limit: s.limit,
                        usage: s.usage.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        id_age: data.map_or(30, |d| d.registered_age),
        has_trim: trim.is_some(),
        trim_assignment: trim.map(|t| t.assignment.clone()).unwrap_or_default(),
        trim_access: trim.map(|t| t.access.clone()).unwrap_or_default(),
    }
}

/// `ui_data` + `ui_static_data`.
pub fn ui_data(
    state: Option<&CardModState>,
    auth_card: Option<CardUi>,
    modified_card: Option<CardUi>,
    is_holding_id: bool,
    jobs: Option<&Jobs>,
) -> CardModData {
    let login = state.and_then(|s| s.login.as_ref());
    let regions = STATION_REGIONS
        .iter()
        .filter(|(name, _)| {
            login.is_some_and(|l| !l.auth.minor || l.auth.region_access.contains(name))
        })
        .map(|(name, access)| RegionUi {
            name: (*name).to_owned(),
            accesses: access
                .iter()
                .filter_map(|a| {
                    Some(AccessUi {
                        desc: access_desc(a)?.to_owned(),
                        access: (*a).to_owned(),
                        flag: access_flag(a),
                    })
                })
                .collect(),
        })
        .collect();
    CardModData {
        authed_user: login.map(|l| l.authenticated_card.clone()),
        auth_card,
        modified_card,
        is_holding_id,
        regions,
        wildcard_flags: WILDCARD_FLAGS
            .iter()
            .map(|&(n, f)| (n.to_owned(), f))
            .collect(),
        access_flag_names: ACCESS_FLAG_NAMES
            .iter()
            .map(|&(f, n)| (f, n.to_owned()))
            .collect(),
        show_basic: true,
        templates: login
            .zip(jobs)
            .map(|(l, jobs)| {
                l.auth
                    .job_templates
                    .iter()
                    .filter_map(|&j| jobs.data.jobs.get(j).map(|j| j.assignment.clone()))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn chat(world: &mut World, client: Entity, text: &str) {
    crate::tools::tell(world, client, Span::Notice, text);
}

/// The label of a card after an edit (`update_label`).
fn relabel(world: &mut World, card: Entity) {
    let Some(reg) = world.get::<IdRegistration>(card).cloned() else {
        return;
    };
    let label = id_label(&reg.registered_name, &reg.assignment);
    match world.get_mut::<DmName>(card) {
        Some(mut name) => name.name = label,
        None => {
            world.entity_mut(card).insert(DmName {
                name: label,
                desc: String::new(),
            });
        }
    }
}

fn registration_mut(world: &mut World, card: Entity) -> Mut<'_, IdRegistration> {
    if world.get::<IdRegistration>(card).is_none() {
        world.entity_mut(card).insert(IdRegistration {
            registered_name: String::new(),
            assignment: String::new(),
            job: String::new(),
        });
    }
    world
        .get_mut::<IdRegistration>(card)
        .expect("inserted above")
}

/// The login checked again now: the card that logged in is still in the main slot and its
/// current access still authenticates. A login that no longer holds is dropped.
fn current_login(world: &mut World, console: Entity) -> Option<CardModLogin> {
    let login = world.get::<CardModState>(console)?.login.clone()?;
    let slot = world
        .get::<ModularComputer>(console)
        .and_then(|c| c.stored_id);
    let auth = (slot == Some(login.card))
        .then(|| world.get::<IdCard>(login.card))
        .flatten()
        .zip(world.get_resource::<Jobs>())
        .and_then(|(card, jobs)| authenticate(&card.access, &jobs.data));
    match auth {
        Some(auth) => Some(CardModLogin { auth, ..login }),
        None => {
            if let Some(mut state) = world.get_mut::<CardModState>(console) {
                state.login = None;
            }
            None
        }
    }
}

/// `ui_act`.
pub fn apply(
    world: &mut World,
    client: Entity,
    user: Entity,
    console: Entity,
    act: CardAct,
) -> Result<(), &'static str> {
    let computer = world
        .get::<ModularComputer>(console)
        .cloned()
        .ok_or("no computer")?;
    let held_card = || -> Option<Entity> {
        let held = world.get::<Inventory>(user)?.active_item()?;
        world
            .get::<DmItem>(held)
            .is_some_and(DmItem::is_id_card)
            .then_some(held)
    };
    match act {
        CardAct::InsertMain => {
            let card = held_card().ok_or("no ID in hand")?;
            insert_id(world, client, user, card, console, false);
            return Ok(());
        }
        CardAct::InsertAlt => {
            let Some(card) = held_card() else {
                chat(
                    world,
                    client,
                    "Чтобы вставить ID-карту в дополнительный слот, её нужно держать в руке.",
                );
                return Err("no ID in hand");
            };
            insert_id(world, client, user, card, console, true);
            return Ok(());
        }
        CardAct::RemoveMain => {
            remove_id(world, console, Some(user), false, false);
            return Ok(());
        }
        CardAct::RemoveAlt => {
            remove_id(world, console, Some(user), false, true);
            return Ok(());
        }
        CardAct::Authenticate => {
            let Some(card) = computer.stored_id else {
                play(world, console, PROMPT_DENY, 50.0, false);
                return Err("no auth card");
            };
            let access = world
                .get::<IdCard>(card)
                .map(|c| c.access.clone())
                .ok_or("not a card")?;
            let auth = world
                .get_resource::<Jobs>()
                .and_then(|jobs| authenticate(&access, &jobs.data))
                .ok_or("no rights")?;
            let name = world
                .get::<DmName>(card)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            let user_name = world
                .get::<IdRegistration>(card)
                .map(|r| r.registered_name.clone())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| "Unknown".to_owned());
            let login = CardModLogin {
                card,
                authenticated_card: if auth.minor {
                    format!("{name} [LIMITED ACCESS]")
                } else {
                    name
                },
                authenticated_user: user_name,
                auth,
            };
            world
                .entity_mut(console)
                .insert(CardModState { login: Some(login) });
            play(world, console, TERMINAL_ON, 50.0, false);
            return Ok(());
        }
        CardAct::Logout => {
            world.entity_mut(console).insert(CardModState::default());
            play(world, console, TERMINAL_OFF, 50.0, false);
            return Ok(());
        }
        _ => {}
    }
    let login = current_login(world, console).ok_or("not logged in")?;
    let target = computer.alt_stored_id.ok_or("no card to modify")?;
    if world.get::<IdCard>(target).is_none() {
        return Err("not a card");
    }
    if world.get::<CardAccess>(target).is_none() {
        let path = world
            .get::<DmItem>(target)
            .map(|i| i.path.clone())
            .unwrap_or_default();
        world.entity_mut(target).insert(CardAccess::of_type(&path));
    }
    match act {
        CardAct::Terminate => {
            let job = world
                .get::<CardAccess>(target)
                .and_then(|d| d.trim.as_ref())
                .map(|t| t.job.clone());
            if login.auth.minor {
                let allowed = world.get_resource::<Jobs>().is_some_and(|jobs| {
                    login
                        .auth
                        .job_templates
                        .iter()
                        .any(|&j| jobs.data.jobs.get(j).map(|j| &j.title) == job.as_ref())
                });
                if !allowed {
                    chat(
                        world,
                        client,
                        "Software error: You do not have the necessary permissions to demote this card.",
                    );
                    return Err("cannot demote this card");
                }
            }
            registration_mut(world, target).assignment = "Demoted".to_owned();
            with_card(world, target, remove_trim);
            relabel(world, target);
            play(world, console, PROMPT_DENY, 50.0, false);
        }
        CardAct::Edit(name) => {
            let name = crate::records::console::sanitize(&name, MAX_NAME_LEN);
            registration_mut(world, target).registered_name = name;
            relabel(world, target);
            play(world, console, TERMINAL_TYPE, 50.0, false);
        }
        CardAct::Age(age) => {
            if !(crate::records::AGE_MIN..=crate::records::AGE_MAX).contains(&age) {
                return Err("age out of range");
            }
            world
                .get_mut::<CardAccess>(target)
                .expect("inserted above")
                .registered_age = age;
            play(world, console, TERMINAL_TYPE, 50.0, false);
        }
        CardAct::Assign(assignment) => {
            registration_mut(world, target).assignment =
                crate::records::console::sanitize(&assignment, MAX_NAME_LEN);
            relabel(world, target);
            play(world, console, TERMINAL_TYPE, 50.0, false);
        }
        CardAct::Access {
            target: access,
            wildcard,
        } => {
            play(world, console, TERMINAL_TYPE, 50.0, false);
            if !login.auth.valid_access.contains(&access) {
                return Err("access outside this login");
            }
            let on_card = world
                .get::<IdCard>(target)
                .is_some_and(|c| c.access.contains(&access));
            if on_card {
                with_card(world, target, |card, data| {
                    remove_access(card, data, std::slice::from_ref(&access));
                });
                return Ok(());
            }
            let added = with_card(world, target, |card, data| {
                add_access(
                    card,
                    data,
                    std::slice::from_ref(&access),
                    wildcard.as_deref(),
                    AddMode::ErrorOnFail,
                )
            });
            if !added {
                chat(
                    world,
                    client,
                    "ID error: ID card rejected your attempted access modification.",
                );
                return Err("card rejected the access");
            }
        }
        CardAct::Template(name) => {
            play(world, console, TERMINAL_TYPE, 50.0, false);
            let trim = world.get_resource::<Jobs>().and_then(|jobs| {
                login
                    .auth
                    .job_templates
                    .iter()
                    .filter_map(|&j| jobs.data.jobs.get(j))
                    .find(|j| j.assignment == name)
                    .map(Trim::of_job)
            });
            let trim = trim.ok_or("not a template of this login")?;
            with_card(world, target, |card, data| {
                apply_template(card, data, &trim)
            });
        }
        _ => unreachable!("handled above"),
    }
    Ok(())
}

fn with_card<R>(
    world: &mut World,
    card: Entity,
    f: impl FnOnce(&mut IdCard, &mut CardAccess) -> R,
) -> R {
    let mut entity = world.entity_mut(card);
    let mut data = entity.take::<CardAccess>().expect("checked by the caller");
    let mut access = entity.get_mut::<IdCard>().expect("checked by the caller");
    let out = f(&mut access, &mut data);
    entity.insert(data);
    out
}

/// `kill_program`: the edited card's name, assignment and trim go into its crew record
/// (`GLOB.manifest.modify`).
pub fn on_kill(world: &mut World, console: Entity) {
    let Some(card) = world
        .get::<ModularComputer>(console)
        .and_then(|c| c.alt_stored_id)
    else {
        return;
    };
    let Some(reg) = world.get::<IdRegistration>(card).cloned() else {
        return;
    };
    let trim = world.get::<CardAccess>(card).and_then(|d| d.trim.clone());
    let trim_assignment = trim
        .as_ref()
        .map(|t| t.assignment.clone())
        .unwrap_or_default();
    let job = trim.and_then(|t| {
        world
            .get_resource::<Jobs>()
            .and_then(|jobs| jobs.data.job(&t.job))
    });
    world
        .resource_mut::<crate::records::manifest::Manifest>()
        .modify(&reg.registered_name, &reg.assignment, &trim_assignment, job);
}
