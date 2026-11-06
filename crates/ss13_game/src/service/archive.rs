//! The library's archive and machines (server), after tg `code/modules/library/lib_machines.dm`
//! and `book.dm` (`writing_utensil_act`).
//!
//! - [`LibraryArchive`] is the book database (tg's `library` SQL table), kept in memory.
//! - A pen on a book (not a manual, bible or the Codex Gigas: `unique`) changes its title (30
//!   characters), contents or author.
//! - The scanner takes a book (giving back the one inside); an empty hand lists scan (into its
//!   cache), clear the cache and eject.
//! - The binder takes an ID card (the author) and then a paper: 4.1 s later the paper is a book
//!   "Print Job #NNN" with the paper's text. Without a scanned ID the first paper only sets the
//!   author to "unknown author".
//! - The librarian's console (`bookmanagement`) is the `LibraryConsole` window
//!   (`super::library_console`); its upload of the nearest scanner's cache (in view) and its
//!   printing (6 s printer cooldown, the book 4.1 s later) are here. The public console only
//!   browses.
// shortcut: the archive lives as long as the server process (tg keeps it in SQL across rounds),
// so random bookcases stay empty; no search filters or pages, poster printing, newscaster book
// club or admin deletion (the console's ledger and window are `super::library_console` and
// `super::barcode`).

use bevy::prelude::*;
use ss13_mapload::DmAtom;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptKind, ask_world};

use super::library::{BOOK, Book};
use super::paper::{MAX_PAPER_LENGTH, Paper, Writing};
use super::server::{
    PendingAsk, TimerJob, after, client_of, item_path, now, say_around, set_icon_state, set_name,
    sound_at,
};
use crate::chem::{map_pos, name_of};
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::storage::{StoredIn, in_reach, put_in_hands, put_inside, take_out};
use crate::tools::tell;

const CONSOLE: &str = "/obj/machinery/computer/libraryconsole";
const MANAGEMENT: &str = "/obj/machinery/computer/libraryconsole/bookmanagement";
const SCANNER: &str = "/obj/machinery/libraryscanner";
const BINDER: &str = "/obj/machinery/bookbinder";
/// Book types with `unique = TRUE`.
const UNIQUE_BOOKS: [&str; 3] = [
    "/obj/item/book/manual",
    "/obj/item/book/bible",
    "/obj/item/book/codex_gigas",
];
/// `SSlibrary.upload_categories`.
pub(super) const CATEGORIES: [&str; 5] =
    ["Fiction", "Non-fiction", "Adult", "Reference", "Religion"];
/// `set_title`'s trim, `MAX_NAME_LEN`, `MAX_BOOK_LENGTH`.
pub(super) const MAX_TITLE: usize = 30;
pub(super) const MAX_NAME_LEN: usize = 42;
const MAX_BOOK_LENGTH: usize = MAX_PAPER_LENGTH * 2;
/// `PRINTER_COOLDOWN`, `attempt_print`'s timer, `bind_book`'s timer.
const PRINTER_COOLDOWN: f64 = 6.0;
const PRINT_SECS: f64 = 4.1;
const BIND_SECS: f64 = 4.1;
/// `get_scanner()`: `range(world.view)`.
const SCANNER_RANGE: u32 = 7;
/// `maximum_book_state`.
const BOOK_STATES: u32 = 8;
const PRINTER_SOUND: &str = "sound/machines/printer.ogg";
const SCAN_SOUND: &str = "sound/machines/scanner/scanner.ogg";
const EJECT_SOUND: &str = "sound/machines/eject.ogg";

/// One archived book (a row of tg's `library` table).
#[derive(Clone, Debug, PartialEq)]
pub struct ArchivedBook {
    pub id: u32,
    pub title: String,
    pub author: String,
    pub content: String,
    pub category: String,
}

/// The library database.
#[derive(Resource, Default, Debug)]
pub struct LibraryArchive {
    pub books: Vec<ArchivedBook>,
}

/// `/obj/machinery/libraryscanner`: `held_book`, `cache`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct BookScanner {
    pub book: Option<Entity>,
    pub cache: Option<Book>,
}

/// `/obj/machinery/bookbinder`: `busy`, `scanned_name`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct BookBinder {
    pub busy: bool,
    pub scanned_name: Option<String>,
}

/// A library console; `management` for the librarian's.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct LibraryConsole {
    pub management: bool,
    /// `printer_cooldown`: game seconds.
    pub printer_ready: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BookField {
    Title,
    Contents,
    Author,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScannerAct {
    Scan,
    Clear,
    Eject,
}

/// A question of a book or a library machine.
#[derive(Clone, Debug)]
pub enum ArchiveAsk {
    /// "What would you like to change?"
    Alter { book: Entity, pen: Entity },
    AlterText {
        book: Entity,
        pen: Entity,
        field: BookField,
    },
    Scanner {
        scanner: Entity,
        acts: Vec<ScannerAct>,
    },
}

pub(super) fn add_obj_parts(world: &mut World, e: Entity, path: &str) {
    if is_under(path, CONSOLE) {
        world.entity_mut(e).insert(LibraryConsole {
            management: is_under(path, MANAGEMENT),
            printer_ready: 0.0,
        });
    } else if is_under(path, SCANNER) {
        world.entity_mut(e).insert(BookScanner::default());
    } else if is_under(path, BINDER) {
        world.entity_mut(e).insert(BookBinder::default());
    }
}

pub(super) fn powered(world: &World, machine: Entity) -> bool {
    world.get::<crate::power::Unpowered>(machine).is_none()
}

fn inside(world: &World, item: Entity, container: Entity) -> bool {
    world
        .get::<StoredIn>(item)
        .is_some_and(|s| s.0 == container)
}

fn to_hands(world: &mut World, mob: Entity, item: Entity) {
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        put_in_hands(world, mob, &inventory, item);
    } else if let Some(pos) = map_pos(world, mob) {
        take_out(world, item, pos);
    }
}

fn say(world: &mut World, machine: Entity, text: &str) {
    crate::atmos_machinery::machine_say(world, machine, text);
}

fn ask(
    world: &mut World,
    client: Entity,
    mob: Entity,
    title: &str,
    msg: &str,
    kind: PromptKind,
    q: ArchiveAsk,
) {
    let prompt = ask_world(world, Ask::new(client, mob, title, msg, kind));
    world.entity_mut(prompt).insert(PendingAsk::Archive(q));
}

fn list(items: Vec<String>) -> PromptKind {
    PromptKind::List {
        items,
        default: None,
    }
}

fn text(max_length: usize, multiline: bool) -> PromptKind {
    PromptKind::Text {
        default: String::new(),
        max_length: max_length as u32,
        multiline,
    }
}

/// `reject_bad_text`: no control characters (line breaks too unless `multiline`), trimmed.
pub(super) fn clean(text: &str, multiline: bool) -> Option<String> {
    let text = text.trim();
    let bad = |c: char| c.is_control() && !(multiline && (c == '\n' || c == '\t'));
    (!text.is_empty() && !text.chars().any(bad)).then(|| text.to_owned())
}

pub(super) fn truncate(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// tg `item_interaction` of books (a pen), the scanner (a book) and the binder (a paper, an
/// ID). True when it was theirs.
pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<Book>(target).is_some()
        && matches!(super::paper::writing(world, held), Some(Writing::Pen(_)))
    {
        if can_alter(world, client, mob, target) {
            let name = name_of(world, target);
            let items = ["Title", "Contents", "Author", "Cancel"]
                .map(str::to_owned)
                .to_vec();
            ask(
                world,
                client,
                mob,
                &name,
                "What would you like to change?",
                list(items),
                ArchiveAsk::Alter {
                    book: target,
                    pen: held,
                },
            );
        }
        return true;
    }
    if world.get::<BookScanner>(target).is_some() && world.get::<Book>(held).is_some() {
        let old = world.get::<BookScanner>(target).and_then(|s| s.book);
        put_inside(world, held, target);
        if let Some(old) = old.filter(|&o| inside(world, o, target)) {
            to_hands(world, mob, old);
        }
        if let Some(mut scanner) = world.get_mut::<BookScanner>(target) {
            scanner.book = Some(held);
        }
        sound_at(world, target, EJECT_SOUND, 70.0, false);
        return true;
    }
    if world.get::<BookBinder>(target).is_some() {
        if world.get::<Paper>(held).is_some() {
            prebind(world, mob, held, target);
            return true;
        }
        if world.get::<crate::inventory::IdCard>(held).is_some() {
            let name = world
                .get::<crate::outfit::IdRegistration>(held)
                .map(|r| r.registered_name.clone());
            if let Some(mut binder) = world.get_mut::<BookBinder>(target) {
                binder.scanned_name = name;
            }
            tell(world, client, Span::Notice, "scanned");
            return true;
        }
    }
    false
}

/// `can_vandalize` (the pen's own checks are `writing`'s).
fn can_alter(world: &mut World, client: Entity, mob: Entity, book: Entity) -> bool {
    if crate::vision::is_blind(world, mob) {
        tell(
            world,
            client,
            Span::Warning,
            "As you are trying to write on the book, you suddenly feel very stupid!",
        );
        return false;
    }
    let path = item_path(world, book);
    if UNIQUE_BOOKS.iter().any(|root| is_under(&path, root)) {
        tell(
            world,
            client,
            Span::Warning,
            "These pages don't seem to take the ink well! Looks like you can't modify it.",
        );
        return false;
    }
    true
}

/// tg `attack_hand` -> `ui_interact` of the consoles and the scanner. True when it was one.
pub(super) fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if let Some(console) = world.get::<LibraryConsole>(target).copied() {
        if !powered(world, target) {
            return true;
        }
        if !console.management {
            browse(world, client);
            return true;
        }
        super::library_console::open(world, client, mob, target);
        return true;
    }
    let Some(scanner) = world.get::<BookScanner>(target).cloned() else {
        return false;
    };
    if !powered(world, target) {
        return true;
    }
    let book = scanner.book.filter(|&b| inside(world, b, target));
    let name = name_of(world, target);
    let cached = match &scanner.cache {
        Some(cache) => format!(
            "{name}: cached \"{}\" by {}.",
            or_na(&cache.title),
            or_na(&cache.author)
        ),
        None => format!("{name}: nothing cached."),
    };
    tell(world, client, Span::Notice, cached);
    let mut acts = Vec::new();
    if book.is_some() {
        acts.push(ScannerAct::Scan);
    }
    if scanner.cache.is_some() {
        acts.push(ScannerAct::Clear);
    }
    if book.is_some() {
        acts.push(ScannerAct::Eject);
    }
    if acts.is_empty() {
        return true;
    }
    let items = acts
        .iter()
        .map(|a| {
            match a {
                ScannerAct::Scan => "Scan",
                ScannerAct::Clear => "Clear the cache",
                ScannerAct::Eject => "Eject the book",
            }
            .to_owned()
        })
        .collect();
    ask(
        world,
        client,
        mob,
        &name,
        "",
        list(items),
        ArchiveAsk::Scanner {
            scanner: target,
            acts,
        },
    );
    true
}

/// `get_title` / `get_author`: "N/A" when empty.
fn or_na(text: &str) -> &str {
    if text.is_empty() { "N/A" } else { text }
}

/// The public console's listing (tg's search with no filters).
fn browse(world: &mut World, client: Entity) {
    let lines: Vec<String> = world
        .resource::<LibraryArchive>()
        .books
        .iter()
        .map(|b| format!("#{} \"{}\" by {} ({})", b.id, b.title, b.author, b.category))
        .collect();
    if lines.is_empty() {
        tell(world, client, Span::Notice, "The archive is empty.");
        return;
    }
    tell(world, client, Span::Notice, lines.join("\n"));
}

/// The answers, checked again: the actor next to the machine (holding the pen and next to the
/// book), the machine powered, the things still inside.
pub(super) fn answer(world: &mut World, actor: Entity, q: ArchiveAsk, answer: PromptAnswer) {
    let Some(client) = client_of(world, actor) else {
        return;
    };
    match (q, answer) {
        (ArchiveAsk::Alter { book, pen }, PromptAnswer::Item(i)) => {
            let field = [BookField::Title, BookField::Contents, BookField::Author]
                .get(i)
                .copied();
            let Some(field) = field else {
                return;
            };
            if !pen_on_book(world, actor, pen, book) || !can_alter(world, client, actor, book) {
                return;
            }
            let (msg, kind) = match field {
                BookField::Title => ("Write a new title", text(MAX_TITLE, false)),
                BookField::Contents => (
                    "Write your book's contents (HTML NOT allowed)",
                    text(MAX_PAPER_LENGTH, true),
                ),
                BookField::Author => ("Write the author's name", text(MAX_NAME_LEN, false)),
            };
            let title = name_of(world, book);
            ask(
                world,
                client,
                actor,
                &title,
                msg,
                kind,
                ArchiveAsk::AlterText { book, pen, field },
            );
        }
        (ArchiveAsk::AlterText { book, pen, field }, PromptAnswer::Text(entry)) => {
            if !pen_on_book(world, actor, pen, book) || !can_alter(world, client, actor, book) {
                return;
            }
            alter(world, client, book, field, &entry);
        }
        (ArchiveAsk::Scanner { scanner, acts }, PromptAnswer::Item(i)) => {
            if let Some(&act) = acts.get(i)
                && in_reach(world, actor, scanner)
                && powered(world, scanner)
            {
                scanner_act(world, actor, scanner, act);
            }
        }
        _ => {}
    }
}

fn pen_on_book(world: &World, actor: Entity, pen: Entity, book: Entity) -> bool {
    world
        .get::<Inventory>(actor)
        .is_some_and(|i| i.active_item() == Some(pen))
        && super::paper::reachable(world, actor, book)
}

/// `vandalize_title` / `_contents` / `_author`.
fn alter(world: &mut World, client: Entity, book: Entity, field: BookField, entry: &str) {
    let multiline = field == BookField::Contents;
    let Some(entry) = clean(entry, multiline) else {
        tell(world, client, Span::Warning, "invalid input!");
        return;
    };
    match field {
        BookField::Title => {
            if entry.chars().count() > MAX_TITLE {
                tell(world, client, Span::Warning, "too long!");
                return;
            }
            set_name(world, book, &entry);
            if let Some(mut data) = world.get_mut::<Book>(book) {
                data.title = entry;
            }
        }
        BookField::Contents => {
            if let Some(mut data) = world.get_mut::<Book>(book) {
                data.content = truncate(&entry, MAX_PAPER_LENGTH);
            }
        }
        BookField::Author => {
            if let Some(mut data) = world.get_mut::<Book>(book) {
                data.author = truncate(&entry, MAX_NAME_LEN);
            }
        }
    }
    sound_at(world, book, "writing_pen", 50.0, true);
    super::server::refresh_windows(world, book);
}

fn scanner_act(world: &mut World, actor: Entity, scanner: Entity, act: ScannerAct) {
    let Some(state) = world.get::<BookScanner>(scanner).cloned() else {
        return;
    };
    let book = state.book.filter(|&b| inside(world, b, scanner));
    match act {
        ScannerAct::Scan => {
            let Some(data) = book.and_then(|b| world.get::<Book>(b)).cloned() else {
                return;
            };
            if state.cache.as_ref() == Some(&data) {
                say(world, scanner, "This book is already in my internal cache");
                return;
            }
            if let Some(mut s) = world.get_mut::<BookScanner>(scanner) {
                s.cache = Some(data);
            }
            sound_at(world, scanner, SCAN_SOUND, 50.0, true);
        }
        ScannerAct::Clear => {
            if let Some(mut s) = world.get_mut::<BookScanner>(scanner) {
                s.cache = None;
            }
        }
        ScannerAct::Eject => {
            let Some(book) = book else {
                return;
            };
            to_hands(world, actor, book);
            if let Some(mut s) = world.get_mut::<BookScanner>(scanner) {
                s.book = None;
            }
            sound_at(world, scanner, EJECT_SOUND, 70.0, false);
        }
    }
}

/// `get_scanner()`: the nearest scanner in view of the console.
pub(super) fn scanner_near(world: &mut World, console: Entity) -> Option<Entity> {
    let at = *world.get::<TilePos>(console)?;
    let mut scanners = world.query_filtered::<(Entity, &TilePos), With<BookScanner>>();
    scanners
        .iter(world)
        .filter_map(|(e, pos)| Some((pos.chebyshev(at)?, e)))
        .filter(|(d, _)| *d <= SCANNER_RANGE)
        .min_by_key(|(d, e)| (*d, e.to_bits()))
        .map(|(_, e)| e)
}

/// `upload_from_scanner`.
pub(super) fn upload(world: &mut World, console: Entity, category: &str) {
    let Some(scanner) = scanner_near(world, console) else {
        say(world, console, "No nearby scanner detected.");
        return;
    };
    let Some(book) = world
        .get::<BookScanner>(scanner)
        .and_then(|s| s.cache.clone())
    else {
        say(world, console, "No cached book found. Aborting upload.");
        return;
    };
    for (empty, what) in [
        (book.title.is_empty(), "No title detected. Aborting"),
        (book.author.is_empty(), "No author detected. Aborting"),
        (book.content.is_empty(), "No content detected. Aborting"),
    ] {
        if empty {
            say(world, console, what);
            return;
        }
    }
    let mut archive = world.resource_mut::<LibraryArchive>();
    let id = archive.books.iter().map(|b| b.id).max().unwrap_or(0) + 1;
    archive.books.push(ArchivedBook {
        id,
        title: book.title,
        author: book.author,
        content: truncate(&book.content, MAX_BOOK_LENGTH),
        category: category.to_owned(),
    });
    say(
        world,
        console,
        "Upload Complete. Uploaded title will be available for printing in a moment",
    );
}

/// `attempt_print(call_after)`: the printer cooldown, then `job` after the timer.
pub(super) fn attempt_print_with(world: &mut World, console: Entity, job: TimerJob) {
    let t = now(world);
    let ready = world
        .get::<LibraryConsole>(console)
        .is_some_and(|c| c.printer_ready <= t);
    if !ready {
        say(
            world,
            console,
            "Printer currently unavailable, please wait a moment.",
        );
        return;
    }
    if let Some(mut state) = world.get_mut::<LibraryConsole>(console) {
        state.printer_ready = t + PRINTER_COOLDOWN;
    }
    sound_at(world, console, PRINTER_SOUND, 50.0, false);
    after(world, PRINT_SECS, job);
}

/// A new book at `pos` with `data`, named `name`, a random cover (`gen_random_icon_state`).
fn new_book(world: &mut World, pos: TilePos, name: &str, data: Book) -> Option<Entity> {
    let book = crate::construction::spawn_obj(world, BOOK, pos)?;
    if world.get::<Book>(book).is_none() {
        super::library::add_parts(world, book, BOOK);
    }
    world.entity_mut(book).insert(data);
    set_name(world, book, name);
    let cover = world
        .resource_mut::<crate::combat::CombatRng>()
        .range(1, BOOK_STATES);
    if let Some(icon) = world.get::<DmAtom>(book).map(|a| a.icon.clone()) {
        set_icon_state(world, book, &icon, &format!("book{cover}"));
    }
    Some(book)
}

/// `print_book`: the archived book, if it is still there.
pub(super) fn print(world: &mut World, console: Entity, id: u32) {
    let Some(entry) = world
        .resource::<LibraryArchive>()
        .books
        .iter()
        .find(|b| b.id == id)
        .cloned()
    else {
        say(
            world,
            console,
            "PRINTER ERROR! Failed to print document (0x0000000F)",
        );
        return;
    };
    let Some(pos) = world.get::<TilePos>(console).copied() else {
        return;
    };
    let data = Book {
        title: entry.title.clone(),
        author: entry.author,
        content: entry.content,
    };
    if new_book(world, pos, &format!("Book: {}", entry.title), data).is_some() {
        let name = name_of(world, console);
        say_around(
            world,
            console,
            Span::Notice,
            format!(
                "{name}'s printer hums as it produces a completely bound book. How did it do that?"
            ),
            &[],
        );
    }
}

/// `prebind_book`.
fn prebind(world: &mut World, mob: Entity, paper: Entity, binder: Entity) {
    let Some(state) = world.get::<BookBinder>(binder).cloned() else {
        return;
    };
    if !powered(world, binder) {
        return;
    }
    let client = client_of(world, mob);
    if state.busy {
        if let Some(client) = client {
            tell(
                world,
                client,
                Span::Warning,
                "The book binder is busy. Please wait for completion of previous operation.",
            );
        }
        return;
    }
    if state.scanned_name.is_none() {
        if let Some(mut b) = world.get_mut::<BookBinder>(binder) {
            b.scanned_name = Some("unknown author".to_owned());
        }
        say(
            world,
            binder,
            "No ID detected. Please scan your ID if you would like to be credited for this book. Otherwise please enter your paper again.",
        );
        return;
    }
    put_inside(world, paper, binder);
    let (user, name) = (name_of(world, mob), name_of(world, binder));
    say_around(
        world,
        binder,
        Span::Notice,
        format!("{user} loads some paper into {name}."),
        &[(mob, format!("You load some paper into {name}."))],
    );
    if let Some(mut b) = world.get_mut::<BookBinder>(binder) {
        b.busy = true;
    }
    sound_at(world, binder, PRINTER_SOUND, 50.0, false);
    after(world, BIND_SECS, TimerJob::Bind { binder, paper });
}

/// `bind_book`: the paper becomes a book (back out without power).
pub(super) fn bind(world: &mut World, binder: Entity, paper: Entity) {
    let Some(state) = world.get::<BookBinder>(binder).cloned() else {
        return;
    };
    if let Some(mut b) = world.get_mut::<BookBinder>(binder) {
        b.busy = false;
    }
    if !inside(world, paper, binder) {
        return;
    }
    let Some(pos) = world.get::<TilePos>(binder).copied() else {
        return;
    };
    if !powered(world, binder) {
        take_out(world, paper, pos);
        return;
    }
    let content = world
        .get::<Paper>(paper)
        .map(|p| truncate(&p.raw_html(), MAX_BOOK_LENGTH))
        .unwrap_or_default();
    let author = truncate(&state.scanned_name.unwrap_or_default(), MAX_NAME_LEN);
    let job = world
        .resource_mut::<crate::combat::CombatRng>()
        .range(100, 999);
    let data = Book {
        title: String::new(),
        author,
        content,
    };
    let name = name_of(world, binder);
    say_around(
        world,
        binder,
        Span::Notice,
        format!("{name} whirs as it prints and binds a new book."),
        &[],
    );
    new_book(world, pos, &format!("Print Job #{job}"), data);
    if let Some(mut b) = world.get_mut::<BookBinder>(binder) {
        b.scanned_name = None;
    }
    world.despawn(paper);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_text_is_rejected() {
        assert_eq!(clean("  Space Law ", false).as_deref(), Some("Space Law"));
        assert_eq!(clean("a\nb", false), None);
        assert_eq!(clean("a\nb", true).as_deref(), Some("a\nb"));
        assert_eq!(clean("   ", true), None);
        assert_eq!(truncate("abcdef", 3), "abc");
    }
}
