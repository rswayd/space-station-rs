//! Books, bookcases and the bible (server), after tg `code/modules/library/book.dm`,
//! `bookcase.dm` and `bibles.dm`.
//!
//! - A book (`/obj/item/book`) has its `starting_title`/`starting_author`/`starting_content`
//!   (`ss13_mapload::DmService`). Read in hand it opens the `MarkdownViewer` window ("book is
//!   blank!" without content), tells those around, and the first read of a title is a
//!   `book_nerd` mood event.
//! - A mapped bookcase takes the books on its tile; an empty hand picks one from the list into
//!   the empty active hand, a book goes onto the shelf; the sprite shows up to five books.
//! - The bible in a chaplain's hand heals another living human (10 brute and 10 burn on each
//!   hurt bodypart, a `blessing` mood event) 40% of the time, else beats them over the head (5
//!   brain damage without a helmet); others get burnt. It blesses the water (and unholy water)
//!   in a container into holy water.
// shortcut: no library database (random bookcases stay empty, no library computers, scanner or
// binder), no carving, vandalizing or wiki manuals; bookcases cannot be built, wrenched or
// pried; the bible has no skins, sects, altar making, rune revealing, dead blessings, clumsy
// fumbles or bullet blocking.

use std::collections::HashSet;

use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmItem, DmObj};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptKind, ask_world};
use ss13_tgui::server::{AlwaysState, OpenUi};
use ss13_world::SpatialIndex;

use super::server::{
    PendingAsk, client_of, item_path, job_title, say_around, service_data, set_icon_state, sound_at,
};
use crate::chem::{Chem, Reagents, name_of};
use crate::health::{DamageType, Health, Stat, apply_damage, update_stat};
use crate::inventory::Inventory;
use crate::mood::MoodEvent;
use crate::obj_damage::is_under;
use crate::storage::{Stored, StoredIn};
use crate::tools::tell;
use crate::zone::BodyZone;

pub const BOOK: &str = ss13_mapload::service::BOOK_ROOT;
const BIBLE: &str = "/obj/item/book/bible";
const SYNDICATE_BIBLE: &str = "/obj/item/book/bible/syndicate";
/// Martial arts manuals and other granters read themselves (`crate::martial`).
const GRANTER: &str = "/obj/item/book/granter";
const BOOKCASE: &str = "/obj/structure/bookcase";
const HELMET: &str = "/obj/item/clothing/head/helmet";
const WATER: &str = "/datum/reagent/water";
const HOLY_WATER: &str = "/datum/reagent/water/holywater";
const UNHOLY_WATER: &str = "/datum/reagent/fuel/unholywater";
const PAGE_SOUND: &str = "sound/items/handling/paper_pickup.ogg";
/// `SFX_PUNCH`.
const PUNCH: &str = "punch";
/// `DEFAULT_SMACK_CHANCE`.
const SMACK_CHANCE: f32 = 60.0;
/// The bible's `deity_name` without a religion.
const DEITY: &str = "Christ";
/// The chaplain is tg's `HOLY_ROLE_HIGHPRIEST`.
const CHAPLAIN: &str = "Chaplain";

/// `/datum/mood_event/book_nerd`, `/datum/mood_event/blessing`.
const BOOK_NERD: MoodEvent = MoodEvent {
    id: "book_nerd",
    description: "Я недавно прочитал книгу.",
    mood_change: 1.0,
    timeout: 300.0,
    hidden: false,
};
const BLESSING: MoodEvent = MoodEvent {
    id: "blessing",
    description: "Меня благословили.",
    mood_change: 1.0,
    timeout: 480.0,
    hidden: false,
};

/// `book_data` (`/datum/book_info`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Book {
    pub title: String,
    pub author: String,
    pub content: String,
}

/// `mind.book_titles_read`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct BooksRead(pub HashSet<String>);

/// A bookcase that keeps books (`contents`).
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct Bookcase;

pub(super) fn add_parts(world: &mut World, e: Entity, path: &str) {
    if !is_under(path, BOOK) || is_under(path, GRANTER) || world.get::<Book>(e).is_some() {
        return;
    }
    let data = service_data(world, e);
    world.entity_mut(e).insert((
        Book {
            title: data.title,
            author: data.author,
            content: data.text,
        },
        AlwaysState,
    ));
}

pub(super) fn add_obj_parts(world: &mut World, e: Entity, path: &str) {
    if is_under(path, BOOKCASE) {
        world.entity_mut(e).insert(Bookcase);
    }
}

/// `bookcase/Initialize(mapload)`: the books on its tile go onto the shelf.
pub(super) fn shelve_mapped_books(world: &mut World) {
    let mut cases = world.query::<(Entity, &TilePos, &DmObj)>();
    let cases: Vec<(Entity, TilePos)> = cases
        .iter(world)
        .filter(|(_, _, o)| is_under(&o.path, BOOKCASE))
        .map(|(e, p, _)| (e, *p))
        .collect();
    for (case, pos) in cases {
        world.entity_mut(case).insert(Bookcase);
        let books: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .filter(|&e| {
                world
                    .get::<DmItem>(e)
                    .is_some_and(|i| is_under(&i.path, BOOK))
            })
            .collect();
        for book in books {
            crate::storage::put_inside(world, book, case);
        }
        if world
            .get::<Stored>(case)
            .is_some_and(|s| !s.items().is_empty())
        {
            refresh_case_look(world, case);
        }
    }
}

fn books_in(world: &World, case: Entity) -> Vec<Entity> {
    world
        .get::<Stored>(case)
        .map(|s| s.items().to_vec())
        .unwrap_or_default()
}

/// `update_icon_state`: `book-<books, at most 5>`.
fn refresh_case_look(world: &mut World, case: Entity) {
    let count = books_in(world, case).len().min(5);
    if let Some(atom) = world.get::<DmAtom>(case).cloned() {
        set_icon_state(world, case, &atom.icon, &format!("book-{count}"));
    }
}

/// `book/attack_self`: read it; the bible too (its skin menu is not ported).
pub(super) fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(book) = world.get::<Book>(item).cloned() else {
        return false;
    };
    // `can_read_book`.
    if crate::vision::is_blind(world, mob) {
        tell(
            world,
            client,
            Span::Warning,
            "You are blind and can't read anything!",
        );
        return true;
    }
    if book.content.is_empty() {
        tell(world, client, Span::Warning, "book is blank!");
        return true;
    }
    if let Some(at) = crate::chem::map_pos(world, item) {
        let sound = crate::sound::PlaySound::new(at, PAGE_SOUND, 50.0, true)
            .extrarange(crate::sound::SHORT_RANGE_SOUND_EXTRARANGE);
        crate::sound::playsound(world, sound);
    }
    let user = name_of(world, mob);
    say_around(
        world,
        mob,
        Span::Notice,
        format!(
            "{user} opens a book titled \"{}\" and begins reading intently.",
            book.title
        ),
        &[],
    );
    // `credit_book_to_reader`.
    let first = match world.get_mut::<BooksRead>(mob) {
        Some(mut read) => read.0.insert(book.title.clone()),
        None => {
            let read = BooksRead(HashSet::from([book.title.clone()]));
            world.entity_mut(mob).insert(read);
            true
        }
    };
    if first {
        crate::mood::add_event(world, mob, "book_nerd", &BOOK_NERD);
    }
    let title = name_of(world, item);
    world.write_message(OpenUi::new::<super::MarkdownViewerUi>(
        client, mob, item, title,
    ));
    true
}

/// `bookcase/attack_hand`: which book to take. True when `target` is a bookcase.
pub(super) fn bookcase_hand(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
) -> bool {
    if world.get::<Bookcase>(target).is_none() {
        return false;
    }
    let mut books: Vec<(String, Entity)> = books_in(world, target)
        .into_iter()
        .map(|b| (name_of(world, b), b))
        .collect();
    if books.is_empty() {
        return true;
    }
    // `sort_names`.
    books.sort_by(|a, b| a.0.cmp(&b.0));
    let items = books.iter().map(|(n, _)| n.clone()).collect();
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Remove Book",
            "Book to remove from the shelf",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(PendingAsk::TakeBook {
        case: target,
        books: books.into_iter().map(|(_, b)| b).collect(),
    });
    true
}

/// The chosen book, checked again: still on that shelf, the shelf in reach, the active hand
/// empty (tg leaves the book on the shelf otherwise).
pub(super) fn take_book(world: &mut World, actor: Entity, case: Entity, book: Entity) {
    let on_shelf = world.get::<StoredIn>(book).is_some_and(|s| s.0 == case);
    if !on_shelf || !crate::storage::in_reach(world, actor, case) {
        return;
    }
    let Some(inventory) = world.get::<Inventory>(actor).cloned() else {
        return;
    };
    if inventory.active_item().is_some() {
        return;
    }
    crate::storage::put_in_hands(world, actor, &inventory, book);
    refresh_case_look(world, case);
}

/// `bookcase/item_interaction` with a book, the bible on mobs and containers.
pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<Bookcase>(target).is_some() && world.get::<Book>(held).is_some() {
        crate::storage::put_inside(world, held, target);
        refresh_case_look(world, target);
        return true;
    }
    let path = item_path(world, held);
    if !is_under(&path, BIBLE) || is_under(&path, SYNDICATE_BIBLE) {
        return false;
    }
    if world.get::<Health>(target).is_some() {
        bible_attack(world, client, mob, held, target);
        return true;
    }
    bless_container(world, client, mob, target)
}

fn hurt(world: &mut World, mob: Entity, kind: DamageType, amount: f32) {
    if let (Some(mut health), Some(&stat)) =
        (world.get::<Health>(mob).cloned(), world.get::<Stat>(mob))
    {
        let mut new_stat = stat;
        apply_damage(&mut health, &mut new_stat, kind, amount);
        world.entity_mut(mob).insert(health);
        if new_stat != stat {
            world.entity_mut(mob).insert(new_stat);
        }
    }
}

/// `bible/attack` in heal mode.
fn bible_attack(world: &mut World, client: Entity, user: Entity, bible: Entity, target: Entity) {
    if job_title(world, user).as_deref() != Some(CHAPLAIN) {
        tell(
            world,
            client,
            Span::Danger,
            "The book sizzles in your hands.",
        );
        hurt(world, user, DamageType::Burn, 10.0);
        return;
    }
    let (name, user_name) = (name_of(world, target), name_of(world, user));
    if world.get::<Stat>(target) == Some(&Stat::Dead) {
        let bible_name = name_of(world, bible);
        say_around(
            world,
            target,
            Span::Danger,
            format!("{user_name} smacks {name}'s lifeless corpse with {bible_name}."),
            &[],
        );
        sound_at(world, target, PUNCH, 25.0, true);
        return;
    }
    if user == target {
        tell(world, client, Span::Warning, "can't heal yourself!");
        return;
    }
    let smack = world
        .resource_mut::<crate::combat::CombatRng>()
        .chance(SMACK_CHANCE);
    if !smack && bless(world, user, target) {
        return;
    }
    let helmet = world
        .get::<Inventory>(target)
        .and_then(|i| i.head)
        .is_some_and(|h| is_under(&item_path(world, h), HELMET));
    if !helmet {
        crate::medical::organs::adjust_organ_loss_up_to(
            world,
            target,
            crate::medical::organs::Slot::Brain,
            5.0,
            60.0,
        );
        if let Some(victim) = client_of(world, target) {
            tell(world, victim, Span::Warning, "you feel dumber!");
        }
    }
    let bible_name = name_of(world, bible);
    say_around(
        world,
        target,
        Span::Danger,
        format!("{user_name} beats {name} over the head with {bible_name}!"),
        &[(
            target,
            format!("{user_name} beats {name} over the head with {bible_name}!"),
        )],
    );
    sound_at(world, target, PUNCH, 25.0, true);
}

/// `bible/bless` without a sect: true unless the blessing failed (not a human). A human without
/// hurt bodyparts is "ignored": no heal, no smack either.
fn bless(world: &mut World, user: Entity, target: Entity) -> bool {
    let Some(mut health) = world.get::<Health>(target).cloned() else {
        return false;
    };
    if health.basic.is_some() {
        return false;
    }
    let hurt: Vec<BodyZone> = BodyZone::ALL
        .into_iter()
        .filter(|z| health.parts[z.index()].damage() > 0.0)
        .collect();
    if hurt.is_empty() {
        return true;
    }
    for zone in hurt {
        health.heal_zone(zone, 10.0, 10.0);
    }
    let mut stat = world.get::<Stat>(target).copied().unwrap_or_default();
    let before = stat;
    update_stat(&health, &mut stat);
    world.entity_mut(target).insert(health);
    if stat != before {
        world.entity_mut(target).insert(stat);
    }
    let (name, user_name) = (name_of(world, target), name_of(world, user));
    say_around(
        world,
        target,
        Span::Notice,
        format!("{user_name} heals {name} with the power of {DEITY}!"),
        &[],
    );
    if let Some(client) = client_of(world, target) {
        tell(
            world,
            client,
            Span::BoldNotice,
            format!("May the power of {DEITY} compel you to be healed!"),
        );
    }
    sound_at(world, target, PUNCH, 25.0, true);
    crate::mood::add_event(world, target, "blessing", &BLESSING);
    true
}

/// `bible/interact_with_atom` on a reagent holder: water and unholy water become holy water.
fn bless_container(world: &mut World, client: Entity, user: Entity, target: Entity) -> bool {
    if job_title(world, user).as_deref() != Some(CHAPLAIN) {
        return false;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return false;
    };
    let (Some(holy), Some(mut holder)) = (
        chem.id(HOLY_WATER),
        world.get::<Reagents>(target).map(|r| r.0.clone()),
    ) else {
        return false;
    };
    for (source, text) in [(WATER, "blessed"), (UNHOLY_WATER, "purified")] {
        let Some(id) = chem.id(source) else {
            continue;
        };
        let amount = holder.amount(id);
        if amount <= 0.0 {
            continue;
        }
        holder.remove_reagent(id, amount);
        let temp = holder.chem_temp;
        holder.add_reagent(&chem, holy, amount, temp, None, None, false, false);
        world.entity_mut(target).insert(Reagents(holder));
        tell(world, client, Span::Notice, text);
        return true;
    }
    false
}
