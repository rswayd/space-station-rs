//! The librarian's console window (server), after tg
//! `/obj/machinery/computer/libraryconsole/bookmanagement` (`code/modules/library/lib_machines.dm`):
//! tgui `LibraryConsole` over the console's [`Ledger`].
//!
//! - Inventory: the books of the area's bookcases (read on the first open), the barcode
//!   scanner's additions; a row can be removed.
//! - Checkout: the book set for check-out (by the scanner or by title from the inventory) is lent
//!   to a name for some minutes; loans show their time left and whether they are overdue, and
//!   are checked in by hand or by the scanner.
//! - Archive and upload: the in-memory archive (`super::archive`): print an archived book, set
//!   the nearby scanner's cached title and author, upload it under a category.
//! - Print: a bible of the station's religion (6 s printer cooldown, 4.1 s later).
//! - Emagged, once: the forbidden lore (a ritual dagger on the console's tile) or turning it down.
// shortcut: no newscaster book club post, no poster printing, no pages (every list is sent
// whole), no dropdown toggle; the forbidden lore does not age the reader (no age on mobs). The
// archive itself is SQL in tg (deferred: `super::archive`).

use bevy::ecs::message::MessageReader;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::server::{OpenUi, ServerUi, UiActed, UiParams, UiReject};

use super::archive::{
    BookScanner, CATEGORIES, LibraryArchive, MAX_NAME_LEN, MAX_TITLE, attempt_print_with, clean,
    powered, scanner_near, truncate, upload,
};
use super::barcode::Ledger;
use super::library::{Book, Bookcase};
use super::religion::Religion;
use super::server::{TimerJob, now, say_around};
use super::{
    LIBRARY_INVENTORY, LIBRARY_TOP_SNEAKY, LibraryArchiveRow, LibraryBookRow, LibraryConsoleData,
    LibraryConsoleUi, LibraryLoanRow,
};
use crate::chem::name_of;
use crate::tools::tell;

const BIBLE: &str = "/obj/item/book/bible";
const DAGGER: &str = "/obj/item/melee/cultblade/dagger";
/// `get_scanner()`'s default range (`range(null)`: the console's view is not asked for).
const SCANNER_VIEW: u32 = 7;
/// `MINUTES`.
const MINUTE: f64 = 60.0;

/// A parsed `ui_act`.
#[derive(Clone, Debug, PartialEq)]
pub enum ConsoleAct {
    SetScreen(u8),
    /// A 1-based inventory row.
    InventoryRemove(usize),
    Checkout {
        loaned_to: String,
        minutes: f64,
    },
    SetCheckout(String),
    /// A 1-based loan row.
    Checkin(usize),
    SetCacheTitle(String),
    SetCacheAuthor(String),
    Upload(String),
    PrintBook(u32),
    PrintBible,
    LoreSpawn,
    LoreDeny,
}

/// `attack_hand` -> `ui_interact`; the first open reads the area's bookcases
/// (`load_nearby_books`).
pub(super) fn open(world: &mut World, client: Entity, mob: Entity, console: Entity) {
    let Some(ledger) = world.get::<Ledger>(console) else {
        return;
    };
    if !ledger.loaded {
        let books = area_books(world, console);
        if let Some(mut ledger) = world.get_mut::<Ledger>(console) {
            ledger.loaded = true;
            ledger.inventory.extend(books);
        }
    }
    let title = name_of(world, console);
    world.write_message(OpenUi::new::<LibraryConsoleUi>(client, mob, console, title));
}

/// `SSlibrary.get_area_books`: the books on the bookcases of the console's area.
fn area_books(world: &mut World, console: Entity) -> Vec<Book> {
    let area_of = |world: &World, pos: TilePos| {
        world
            .get_resource::<ss13_mapload::Areas>()
            .and_then(|a| a.area_at(pos))
    };
    let Some(area) = world
        .get::<TilePos>(console)
        .and_then(|&p| area_of(world, p))
    else {
        return Vec::new();
    };
    let mut cases = world.query_filtered::<(&TilePos, &crate::storage::Stored), With<Bookcase>>();
    let items: Vec<Entity> = cases
        .iter(world)
        .filter(|(pos, _)| area_of(world, **pos) == Some(area))
        .flat_map(|(_, stored)| stored.items().to_vec())
        .collect();
    items
        .into_iter()
        .filter_map(|b| world.get::<Book>(b).cloned())
        .collect()
}

fn text(params: &UiParams, key: &str) -> Result<String, UiReject> {
    params.text(key)
}

/// A 1-based row number.
fn row(params: &UiParams, key: &str) -> Result<usize, UiReject> {
    let n = params.number(key)?.round();
    if n < 1.0 {
        return Err(UiReject::BadParams(format!("{key}: 1 or more")));
    }
    Ok(n as usize)
}

impl ServerUi for LibraryConsoleUi {
    type Source = (Entity, &'static Ledger, Option<&'static TilePos>);
    type UserParam = (
        Option<Res<'static, LibraryArchive>>,
        Option<Res<'static, Religion>>,
        Res<'static, Time>,
        Query<'static, 'static, (&'static TilePos, &'static BookScanner)>,
    );
    type Act = ConsoleAct;

    fn static_data(_: (Entity, &Ledger, Option<&TilePos>)) {}

    fn data(
        (_, ledger, pos): (Entity, &Ledger, Option<&TilePos>),
        _: Entity,
        (archive, religion, time, scanners): &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> LibraryConsoleData {
        let t = time.elapsed_secs_f64();
        let book_row = |(i, b): (usize, &Book)| LibraryBookRow {
            id: i as u32 + 1,
            title: or_na(&b.title),
            author: or_na(&b.author),
        };
        let checkouts = ledger
            .checkouts
            .iter()
            .enumerate()
            .map(|(i, loan)| {
                // `max(round(timedue, 0.1), 0)`.
                let due = (((loan.due - t) / MINUTE * 10.0).round() / 10.0).max(0.0);
                LibraryLoanRow {
                    id: i as u32 + 1,
                    borrower: if loan.loaned_to.is_empty() {
                        "N/A".into()
                    } else {
                        loan.loaned_to.clone()
                    },
                    overdue: due <= 0.0,
                    due_in_minutes: due as f32,
                    title: or_na(&loan.book.title),
                    author: or_na(&loan.book.author),
                }
            })
            .collect();
        // `get_scanner()`: the nearest one in range.
        let cache = pos.and_then(|at| {
            scanners
                .iter()
                .filter_map(|(p, s)| Some((p.chebyshev(*at)?, s)))
                .filter(|(d, _)| *d <= SCANNER_VIEW)
                .min_by_key(|(d, _)| *d)
                .map(|(_, s)| s.cache.clone())
        });
        let (deity, religion, bible_name) =
            super::religion::print_names(&religion.as_deref().cloned().unwrap_or_default());
        LibraryConsoleData {
            screen_state: ledger.screen.clamp(LIBRARY_INVENTORY, LIBRARY_TOP_SNEAKY),
            display_lore: ledger.emagged && !ledger.lore_taken,
            inventory: ledger.inventory.iter().enumerate().map(book_row).collect(),
            checkouts,
            checkout_title: ledger.checking_out.as_ref().map(|b| or_na(&b.title)),
            pages: archive
                .as_ref()
                .map(|a| {
                    a.books
                        .iter()
                        .map(|b| LibraryArchiveRow {
                            id: b.id,
                            title: b.title.clone(),
                            author: b.author.clone(),
                            category: b.category.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            upload_categories: CATEGORIES.map(str::to_owned).to_vec(),
            has_scanner: cache.is_some(),
            has_cache: cache.as_ref().is_some_and(Option::is_some),
            cache_title: cache
                .as_ref()
                .and_then(|c| c.as_ref())
                .map(|c| or_na(&c.title))
                .unwrap_or_default(),
            cache_author: cache
                .as_ref()
                .and_then(|c| c.as_ref())
                .map(|c| or_na(&c.author))
                .unwrap_or_default(),
            deity,
            religion,
            bible_name,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ConsoleAct, UiReject> {
        Ok(match action {
            "set_screen" => {
                let n = params.number("screen_index")?.round();
                // `set_screen_state`: clamped.
                ConsoleAct::SetScreen(
                    n.clamp(LIBRARY_INVENTORY as f64, LIBRARY_TOP_SNEAKY as f64) as u8
                )
            }
            "inventory_remove" => ConsoleAct::InventoryRemove(row(params, "book_id")?),
            "checkout" => ConsoleAct::Checkout {
                loaned_to: text(params, "loaned_to").unwrap_or_default(),
                // `max(params["checkout_time"], 1)`.
                minutes: params.number("checkout_time").unwrap_or(1.0).max(1.0),
            },
            "set_checkout" => ConsoleAct::SetCheckout(text(params, "book_name")?),
            "checkin" => ConsoleAct::Checkin(row(params, "checked_out_id")?),
            "set_cache_title" => ConsoleAct::SetCacheTitle(text(params, "title")?),
            "set_cache_author" => ConsoleAct::SetCacheAuthor(text(params, "author")?),
            "upload" => ConsoleAct::Upload(text(params, "category").unwrap_or_default()),
            "print_book" => {
                let id = params.number("book_id")?.round();
                if id < 0.0 || id > u32::MAX as f64 {
                    return Err(UiReject::BadParams("book_id".into()));
                }
                ConsoleAct::PrintBook(id as u32)
            }
            "print_bible" => ConsoleAct::PrintBible,
            "lore_spawn" => ConsoleAct::LoreSpawn,
            "lore_deny" => ConsoleAct::LoreDeny,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

/// `get_title` / `get_author`.
fn or_na(text: &str) -> String {
    if text.is_empty() { "N/A" } else { text }.to_owned()
}

pub(super) fn console_acts(
    mut acts: MessageReader<UiActed<LibraryConsoleUi>>,
    mut commands: Commands,
) {
    for acted in acts.read() {
        let (actor, client, console, act) =
            (acted.actor, acted.client, acted.source, acted.act.clone());
        commands.queue(move |world: &mut World| apply(world, client, actor, console, act));
    }
}

fn with_ledger(world: &mut World, console: Entity, f: impl FnOnce(&mut Ledger)) {
    if let Some(mut ledger) = world.get_mut::<Ledger>(console) {
        f(&mut ledger);
    }
}

/// The acts, at the moment of effect: the console still a powered librarian's console.
pub fn apply(world: &mut World, client: Entity, actor: Entity, console: Entity, act: ConsoleAct) {
    if world.get::<Ledger>(console).is_none() || !powered(world, console) {
        return;
    }
    let spawn_lore = act == ConsoleAct::LoreSpawn;
    match act {
        ConsoleAct::SetScreen(screen) => with_ledger(world, console, |l| l.screen = screen),
        ConsoleAct::InventoryRemove(i) => with_ledger(world, console, |l| {
            if i <= l.inventory.len() {
                l.inventory.remove(i - 1);
            }
        }),
        ConsoleAct::Checkout { loaned_to, minutes } => {
            // `copytext(sanitize(loaned_to), 1, MAX_NAME_LEN)`.
            let name = clean(&loaned_to, false)
                .map(|n| truncate(&n, MAX_NAME_LEN - 1))
                .unwrap_or_default();
            let due = now(world) + minutes * MINUTE;
            with_ledger(world, console, |l| {
                l.lend(&name, due);
            });
        }
        ConsoleAct::SetCheckout(title) => with_ledger(world, console, |l| {
            // `available[book_infos.title]`: the last copy of that title.
            if let Some(book) = l.inventory.iter().rev().find(|b| b.title == title) {
                l.checking_out = Some(book.clone());
            }
        }),
        ConsoleAct::Checkin(i) => with_ledger(world, console, |l| {
            if i <= l.checkouts.len() {
                l.checkouts.remove(i - 1);
            }
        }),
        ConsoleAct::SetCacheTitle(title) => {
            let title = clean(&title, false).map(|t| truncate(&t, MAX_TITLE));
            set_cache(world, console, |c| {
                if let Some(t) = title {
                    c.title = t;
                }
            });
        }
        ConsoleAct::SetCacheAuthor(author) => {
            let author = clean(&author, false).map(|a| truncate(&a, MAX_NAME_LEN));
            set_cache(world, console, |c| {
                if let Some(a) = author {
                    c.author = a;
                }
            });
        }
        ConsoleAct::Upload(category) => {
            // "Nice try": an unknown category is the default one.
            let category = CATEGORIES
                .iter()
                .find(|c| **c == category)
                .copied()
                .unwrap_or(CATEGORIES[0]);
            upload(world, console, category);
        }
        ConsoleAct::PrintBook(id) => {
            attempt_print_with(world, console, TimerJob::PrintBook { console, id });
        }
        ConsoleAct::PrintBible => {
            attempt_print_with(world, console, TimerJob::PrintBible { console });
        }
        ConsoleAct::LoreSpawn | ConsoleAct::LoreDeny => {
            let ledger = world.get::<Ledger>(console).cloned().unwrap_or_default();
            if ledger.emagged && !ledger.lore_taken {
                with_ledger(world, console, |l| l.lore_taken = true);
                if spawn_lore {
                    forbidden_lore(world, client, actor, console);
                } else {
                    tell(
                        world,
                        client,
                        Span::Warning,
                        "You click off the page in a rush, and the machine hums back to normal, \
                         the tab gone...",
                    );
                }
            }
            with_ledger(world, console, |l| l.screen = LIBRARY_INVENTORY);
        }
    }
}

fn set_cache(world: &mut World, console: Entity, f: impl FnOnce(&mut Book)) {
    let Some(scanner) = scanner_near(world, console) else {
        return;
    };
    if let Some(mut state) = world.get_mut::<BookScanner>(scanner)
        && let Some(cache) = state.cache.as_mut()
    {
        f(cache);
    }
}

/// `print_forbidden_lore`.
fn forbidden_lore(world: &mut World, client: Entity, actor: Entity, console: Entity) {
    if let Some(&pos) = world.get::<TilePos>(console) {
        crate::construction::spawn_obj(world, DAGGER, pos);
    }
    tell(
        world,
        client,
        Span::Warning,
        "Your sanity barely endures the seconds spent in the vault's browsing window. The only \
         thing to remind you of this when you stop browsing is a sinister dagger sitting on the \
         desk. You don't even remember where it came from...",
    );
    let name = name_of(world, actor);
    say_around(
        world,
        actor,
        Span::Warning,
        format!(
            "{name} stares at the blank screen for a few moments, their expression frozen in \
             fear. When they finally awaken from it, they look a lot older."
        ),
        &[],
    );
}

/// `bookmanagement/emag_act`: once.
pub fn emag_act(world: &mut World, client: Entity, console: Entity) -> bool {
    let Some(ledger) = world.get::<Ledger>(console) else {
        return false;
    };
    if ledger.emagged {
        return false;
    }
    with_ledger(world, console, |l| l.emagged = true);
    tell(world, client, Span::Notice, "forbidden knowledge unlocked");
    true
}

/// `print_bible`.
pub(super) fn print_bible(world: &mut World, console: Entity) {
    let Some(&pos) = world.get::<TilePos>(console) else {
        return;
    };
    if let Some(bible) = crate::construction::spawn_obj(world, BIBLE, pos) {
        super::religion::dress_printed_bible(world, bible);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acts_parse_and_clamp() {
        let p = |v: &[(&str, &str)]| -> Vec<(String, String)> {
            v.iter().map(|(a, b)| ((*a).into(), (*b).into())).collect()
        };
        let screen = p(&[("screen_index", "9")]);
        assert_eq!(
            LibraryConsoleUi::parse_act("set_screen", &UiParams::new(&screen)),
            Ok(ConsoleAct::SetScreen(LIBRARY_TOP_SNEAKY))
        );
        let checkout = p(&[("loaned_to", "Bob"), ("checkout_time", "0")]);
        assert_eq!(
            LibraryConsoleUi::parse_act("checkout", &UiParams::new(&checkout)),
            Ok(ConsoleAct::Checkout {
                loaned_to: "Bob".into(),
                minutes: 1.0
            })
        );
        let bad = p(&[("book_id", "0")]);
        assert!(LibraryConsoleUi::parse_act("inventory_remove", &UiParams::new(&bad)).is_err());
        assert!(LibraryConsoleUi::parse_act("news_post", &UiParams::new(&[])).is_err());
    }
}
