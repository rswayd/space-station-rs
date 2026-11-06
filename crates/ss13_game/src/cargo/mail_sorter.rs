//! The mail sorter (server), after tg `/obj/machinery/mailsorter` (`code/modules/vending/mail.dm`).
//!
//! - A letter, or a mail bag's letters and papers, go inside (at most `MAIL_CAPACITY`, 100).
//! - An empty hand opens tg's radial menu as a list: "Eject" (pick a letter: it comes out on the
//!   output tile after 5 s), "Dump" (everything onto the output tile at once), "Sort" (pick a
//!   department: after 5 s the letters of recipients whose primary department it is come out;
//!   1 s later it reports what it could not sort, 1 s later it is idle again). Nothing while
//!   busy.
//! - Built from its board and taken apart as other machines (`crate::machine_frame`); taken apart,
//!   the mail drops on its tile.
// shortcut: no alt-click rotation of the output (south), sounds, state overlays or throwing the
// dumped mail; the papers it takes are counted but never sorted (tg: the same).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmName, DmObj};
use ss13_shared::chat::Span;
use ss13_shared::{Dir, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};

use super::console::machine_say;
use super::mail::Mail;
use crate::inventory::{CarriedBy, Inventory};
use crate::jobs::{Jobs, PlayerJob};
use crate::obj_damage::is_under;
use crate::storage::{Stored, put_inside, take_out};
use crate::tools::tell;

pub const MAIL_SORTER: &str = "/obj/machinery/mailsorter";
pub const MAIL_BAG: &str = "/obj/item/storage/bag/mail";
const PAPER: &str = "/obj/item/paper";
const MAIL: &str = "/obj/item/mail";
/// `MAIL_CAPACITY`.
pub const MAIL_CAPACITY: usize = 100;
/// The sorting and ejecting delay, and the report steps.
const WORK_SECS: f64 = 5.0;
const STEP_SECS: f64 = 1.0;
/// `sorting_departments`.
pub const SORTING_DEPARTMENTS: [&str; 9] = [
    "Engineering",
    "Security",
    "Justice",
    "Medical",
    "Science",
    "Cargo",
    "Service",
    "Command",
    "NT Representation",
];

/// What the sorter does next, and when.
#[derive(Clone, Debug, PartialEq)]
enum Step {
    Sort { client: Entity, department: String },
    Eject { client: Entity, letter: Entity },
    Report { unable: u32, total: u32 },
    Idle,
}

/// A mail sorter's live state (`currentstate`; the mail is its `Stored` contents).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct MailSorter {
    pub output: Dir,
    next: Option<(f64, Step)>,
}

impl MailSorter {
    pub fn busy(&self) -> bool {
        self.next.is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Menu {
    Eject,
    Dump,
    Sort,
}

const MENU: [(Menu, &str); 3] = [
    (Menu::Eject, "Eject"),
    (Menu::Dump, "Dump"),
    (Menu::Sort, "Sort"),
];

/// On a prompt: the sorter and what is being chosen.
#[derive(Component, Clone, Debug)]
enum SorterPrompt {
    Menu(Entity),
    Department(Entity),
    Letter(Entity, Vec<Entity>),
}

pub struct MailSorterPlugin;

impl Plugin for MailSorterPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::EmptyHand, 3458, |w, c| {
            crate::cargo::mail_sorter::interact(w, c.client, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 2970, |w, c| {
            c.held.is_some_and(|h| {
                crate::cargo::mail_sorter::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        app.add_systems(ss13_world::MapInit, setup_mapped)
            .add_systems(
                Update,
                (
                    work.run_if(any_with_component::<MailSorter>),
                    prompt_answers
                        .in_set(ss13_tgui::server::UiSystems::Handle)
                        .after(ss13_tgui::prompt::PromptSystems::Answer),
                ),
            );
    }
}

fn setup_mapped(world: &mut World) {
    let mut q = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let found: Vec<Entity> = q
        .iter(world)
        .filter(|(_, o)| is_under(&o.path, MAIL_SORTER))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup(world, e);
    }
}

/// Map load and `crate::machine_frame::setup_built`.
pub fn setup(world: &mut World, e: Entity) {
    world.entity_mut(e).insert_if_new(MailSorter::default());
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn item_path(world: &World, e: Entity) -> String {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

fn contents(world: &World, e: Entity) -> Vec<Entity> {
    world
        .get::<Stored>(e)
        .map(|s| s.items().to_vec())
        .unwrap_or_default()
}

fn name_of(world: &World, e: Entity) -> String {
    world
        .get::<DmName>(e)
        .map(|n| {
            n.name
                .trim_start_matches("\\improper ")
                .trim_start_matches("\\proper ")
                .to_owned()
        })
        .unwrap_or_default()
}

fn panel_open(world: &World, e: Entity) -> bool {
    world
        .get::<crate::construction::PanelOpen>(e)
        .is_some_and(|p| p.0)
}

/// Out of `mob`'s hands into `into`.
fn from_hand(world: &mut World, mob: Entity, item: Entity, into: Entity) {
    world.entity_mut(item).remove::<CarriedBy>();
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob)
        && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(item))
    {
        *slot = None;
    }
    put_inside(world, item, into);
}

/// `accept_check`: letters and papers.
fn accepted(world: &World, item: Entity) -> bool {
    let path = item_path(world, item);
    is_under(&path, MAIL) || is_under(&path, PAPER)
}

/// tg `item_interaction`: a mail bag empties its mail and papers in, a letter goes in.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    e: Entity,
) -> bool {
    if world.get::<MailSorter>(e).is_none() || panel_open(world, e) {
        return false;
    }
    let path = item_path(world, held);
    let name = name_of(world, e);
    let full = |world: &World| contents(world, e).len() >= MAIL_CAPACITY;
    if is_under(&path, MAIL_BAG) {
        let bag_contents = contents(world, held);
        if bag_contents.is_empty() {
            let bag = name_of(world, held);
            tell(world, client, Span::Warning, format!("The {bag} is empty!"));
            return true;
        }
        let mut loaded = 0;
        for item in bag_contents {
            if !accepted(world, item) {
                continue;
            }
            if full(world) {
                let text = format!("There is no space for more mail in {name}!");
                tell(world, client, Span::Warning, text);
                return true;
            }
            put_inside(world, item, e);
            loaded += 1;
        }
        let bag = name_of(world, held);
        if loaded > 0 {
            let text = format!("You load the {name} with the {bag}.");
            tell(world, client, Span::Notice, text);
            if !contents(world, held).is_empty() {
                tell(world, client, Span::Warning, "Some items are refused.");
            }
        } else {
            let text = format!("There is nothing in the {bag} to put in the {name}!");
            tell(world, client, Span::Warning, text);
        }
        return true;
    }
    if world.get::<Mail>(held).is_some() {
        if full(world) {
            let text = format!("There is no space for more mail in {name}!");
            tell(world, client, Span::Warning, text);
        } else {
            let letter = name_of(world, held);
            from_hand(world, mob, held, e);
            let text = format!("The {name} whizzles as it accepts the {letter}.");
            tell(world, client, Span::Notice, text);
        }
        return true;
    }
    false
}

/// tg `interact`: the radial menu as a list.
pub fn interact(world: &mut World, client: Entity, mob: Entity, e: Entity) -> bool {
    let Some(sorter) = world.get::<MailSorter>(e) else {
        return false;
    };
    if sorter.busy() {
        return true;
    }
    if contents(world, e).is_empty() {
        tell(world, client, Span::Warning, "There's no mail inside!");
        return true;
    }
    let items = MENU.iter().map(|(_, label)| (*label).to_owned()).collect();
    let title = name_of(world, e);
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            title,
            "",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(SorterPrompt::Menu(e));
    true
}

fn ask(world: &mut World, client: Entity, mob: Entity, title: &str, items: Vec<String>) -> Entity {
    ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Mail Sorting",
            title,
            PromptKind::List {
                items,
                default: None,
            },
        ),
    )
}

/// The prompts' answers, revalidated: the sorter still exists, is in reach and idle.
fn prompt_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answers {
        let Some(kind) = world.get::<SorterPrompt>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let e = match &kind {
            SorterPrompt::Menu(e) | SorterPrompt::Department(e) | SorterPrompt::Letter(e, _) => *e,
        };
        let able = world.get_entity(e).is_ok()
            && world.get::<MailSorter>(e).is_some_and(|s| !s.busy())
            && world.get::<crate::health::Stat>(actor) == Some(&crate::health::Stat::Conscious)
            && !crate::status_effects::incapacitation(world, actor).incapacitated
            && crate::storage::in_reach(world, actor, e);
        if !able {
            continue;
        }
        match kind {
            SorterPrompt::Menu(_) => match MENU.get(i).map(|(m, _)| *m) {
                Some(Menu::Eject) => {
                    let letters = contents(world, e);
                    if letters.is_empty() {
                        continue;
                    }
                    let names = letters.iter().map(|&l| name_of(world, l)).collect();
                    let p = ask(world, client, actor, "Choose the envelope to eject", names);
                    world.entity_mut(p).insert(SorterPrompt::Letter(e, letters));
                }
                Some(Menu::Dump) => {
                    let n = contents(world, e).len();
                    let name = name_of(world, e);
                    let text = format!("{name} dumps {n} envelope{} on the floor.", plural(n));
                    tell(world, client, Span::Notice, text);
                    dump_all(world, e);
                }
                Some(Menu::Sort) => {
                    let items = SORTING_DEPARTMENTS
                        .iter()
                        .map(|d| (*d).to_owned())
                        .collect();
                    let p = ask(
                        world,
                        client,
                        actor,
                        "Choose the department to sort mail for",
                        items,
                    );
                    world.entity_mut(p).insert(SorterPrompt::Department(e));
                }
                None => {}
            },
            SorterPrompt::Department(_) => {
                let Some(department) = SORTING_DEPARTMENTS.get(i) else {
                    continue;
                };
                let at = now(world) + WORK_SECS;
                set_next(
                    world,
                    e,
                    at,
                    Step::Sort {
                        client,
                        department: (*department).to_owned(),
                    },
                );
            }
            SorterPrompt::Letter(_, letters) => {
                let Some(&letter) = letters.get(i) else {
                    continue;
                };
                if !contents(world, e).contains(&letter) {
                    continue;
                }
                let at = now(world) + WORK_SECS;
                set_next(world, e, at, Step::Eject { client, letter });
            }
        }
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn set_next(world: &mut World, e: Entity, at: f64, step: Step) {
    if let Some(mut sorter) = world.get_mut::<MailSorter>(e) {
        sorter.next = Some((at, step));
    }
}

/// `get_unload_turf`.
fn unload_turf(world: &World, e: Entity) -> Option<TilePos> {
    let pos = *world.get::<TilePos>(e)?;
    let dir = world.get::<MailSorter>(e).map_or(Dir::South, |s| s.output);
    Some(pos.step(dir))
}

/// `dump_all_mail`.
pub fn dump_all(world: &mut World, e: Entity) {
    let Some(out) = unload_turf(world, e) else {
        return;
    };
    for item in contents(world, e) {
        take_out(world, item, out);
    }
}

/// The primary department (`departments_list[1].department_name`) of a recipient's job; a head
/// is listed in Command and their own department, the own one comes first in tg.
fn primary_department(world: &World, mob: Entity) -> Option<String> {
    let job = world.get::<PlayerJob>(mob)?.0;
    let jobs = world.get_resource::<Jobs>()?;
    let of: Vec<&str> = jobs
        .data
        .departments
        .iter()
        .filter(|d| d.jobs.iter().any(|&j| usize::from(j) == job))
        .map(|d| d.name.as_str())
        .collect();
    of.iter()
        .find(|d| **d != "Command")
        .or(of.first())
        .map(|d| (*d).to_owned())
}

/// `continue_sort`: the letters for `department` out; (unable to sort, total).
fn sort(world: &mut World, e: Entity, client: Entity, department: &str) -> (u32, u32) {
    let inside = contents(world, e);
    let total = inside.len() as u32;
    let mut unable = 0;
    let mut sorted = Vec::new();
    for item in inside {
        // Papers are not `/obj/item/mail`: tg's typed loop skips them.
        let Some(mail) = world.get::<Mail>(item).copied() else {
            continue;
        };
        let primary = mail
            .recipient
            .filter(|r| world.get_entity(*r).is_ok())
            .and_then(|r| primary_department(world, r));
        match primary {
            None => unable += 1,
            Some(d) if d == department => sorted.push(item),
            Some(_) => {}
        }
    }
    if sorted.is_empty() {
        let text = format!("No mail for the following department: {department}.");
        machine_say(world, e, &text);
    } else {
        let n = sorted.len();
        machine_say(
            world,
            e,
            &format!("{n} envelope{} sorted successfully.", plural(n)),
        );
        let name = name_of(world, e);
        let text = format!("{name} ejects {n} envelope{}.", plural(n));
        tell(world, client, Span::Notice, text);
        if let Some(out) = unload_turf(world, e) {
            for item in sorted {
                take_out(world, item, out);
            }
        }
    }
    (unable, total)
}

/// The sorter's timers.
fn work(world: &mut World) {
    let t = now(world);
    let mut q = world.query::<(Entity, &MailSorter)>();
    let due: Vec<(Entity, Step)> = q
        .iter(world)
        .filter_map(|(e, s)| {
            s.next
                .clone()
                .filter(|(at, _)| *at <= t)
                .map(|(_, s)| (e, s))
        })
        .collect();
    for (e, step) in due {
        let next = match step {
            Step::Sort { client, department } => {
                let (unable, total) = sort(world, e, client, &department);
                Some((t + STEP_SECS, Step::Report { unable, total }))
            }
            Step::Eject { client, letter } => {
                if contents(world, e).contains(&letter)
                    && let Some(out) = unload_turf(world, e)
                {
                    let text = format!(
                        "{} reluctantly spits out {}.",
                        name_of(world, e),
                        name_of(world, letter)
                    );
                    tell(world, client, Span::Notice, text);
                    take_out(world, letter, out);
                }
                None
            }
            Step::Report { unable, total } => {
                let text = if unable > 0 {
                    format!(
                        "Couldn't sort {unable} envelope{}.",
                        plural(unable as usize)
                    )
                } else {
                    format!("{total} envelope{} processed.", plural(total as usize))
                };
                machine_say(world, e, &text);
                Some((t + STEP_SECS, Step::Idle))
            }
            Step::Idle => None,
        };
        if let Some(mut sorter) = world.get_mut::<MailSorter>(e) {
            sorter.next = next;
        }
    }
}

/// tg `examine`.
pub fn examine_lines(world: &World, e: Entity, lines: &mut Vec<(Span, String)>) {
    if world.get::<MailSorter>(e).is_none() {
        return;
    }
    let n = contents(world, e).len();
    let space = if n < MAIL_CAPACITY {
        format!("There is space for {} envelopes inside.", MAIL_CAPACITY - n)
    } else {
        "There is no more space for envelopes inside.".to_owned()
    };
    let inside = match n {
        0 => "There is no envelopes inside.".to_owned(),
        1 => "There is 1 envelope inside.".to_owned(),
        n => format!("There are {n} envelopes inside."),
    };
    lines.push((Span::Notice, space));
    lines.push((Span::Notice, inside));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busy_follows_the_timer() {
        let mut s = MailSorter::default();
        assert!(!s.busy());
        assert_eq!(s.output, Dir::South, "tg output_dir");
        s.next = Some((1.0, Step::Idle));
        assert!(s.busy());
    }

    #[test]
    fn sorting_departments_are_tgs() {
        assert_eq!(SORTING_DEPARTMENTS.len(), 9);
        assert!(SORTING_DEPARTMENTS.contains(&"NT Representation"));
    }
}
