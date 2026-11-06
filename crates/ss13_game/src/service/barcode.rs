//! The librarian's barcode scanner (server), after tg `code/modules/library/barcode_scanner.dm`
//! and `bookmanagement/item_interaction`.
//!
//! - Used on the librarian's console it connects to it ("already connected!" the second time).
//! - In hand it cycles check-in -> check-out -> inventory (only while connected).
//! - On a book: check-in drops the console's loan of that book, check-out picks an inventory
//!   copy of it for the console's next loan, inventory adds a copy to the console's inventory.
//!   Books compare by title, author and contents (`book_info/compare`).
// shortcut: the console's ledger has no tgui pages yet (`super::archive` lists), so loans are
// only made through `Ledger::lend`; upgrade with the `LibraryConsole` window.

use bevy::prelude::*;
use ss13_shared::chat::Span;

use super::library::Book;
use super::server::{say_around, sound_at};
use crate::chem::name_of;
use crate::obj_damage::is_under;
use crate::tools::tell;

const SCANNER: &str = "/obj/item/barcodescanner";
const MANAGEMENT: &str = "/obj/machinery/computer/libraryconsole/bookmanagement";
const BEEP: &str = "sound/items/barcodebeep.ogg";
const CLICK: &str = "sound/items/click.ogg";

/// `scan_mode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScanMode {
    #[default]
    CheckIn,
    CheckOut,
    Inventory,
}

/// `/obj/item/barcodescanner`: `computer_ref`, `scan_mode`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct BarcodeScanner {
    pub computer: Option<Entity>,
    pub mode: ScanMode,
}

/// One `/datum/borrowbook`.
#[derive(Clone, Debug, PartialEq)]
pub struct Loan {
    pub book: Book,
    pub loaned_to: String,
    /// `duedate`, game seconds.
    pub due: f64,
}

/// The librarian console's `inventory`, `checkouts` and `checking_out_book`, and its window's
/// state (`super::library_console`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Ledger {
    pub inventory: Vec<Book>,
    pub checkouts: Vec<Loan>,
    pub checking_out: Option<Book>,
    /// `screen_state` (0 before the first open: the inventory).
    pub screen: u8,
    /// `obj_flags & EMAGGED`.
    pub emagged: bool,
    /// `!can_spawn_lore`.
    pub lore_taken: bool,
    /// `!dynamic_inv_load`: the area's bookcases were read into the inventory.
    pub loaded: bool,
}

impl Ledger {
    /// The `checkout` act: lends the book set for check-out.
    pub fn lend(&mut self, loaned_to: &str, due: f64) -> bool {
        let Some(book) = self.checking_out.clone() else {
            return false;
        };
        self.checkouts.push(Loan {
            book,
            loaned_to: loaned_to.to_owned(),
            due,
        });
        true
    }
}

pub(super) fn add_parts(world: &mut World, e: Entity, path: &str) {
    if is_under(path, SCANNER) {
        world.entity_mut(e).insert(BarcodeScanner::default());
    }
}

pub(super) fn add_obj_parts(world: &mut World, e: Entity, path: &str) {
    if is_under(path, MANAGEMENT) {
        world.entity_mut(e).insert(Ledger::default());
    }
}

/// The console the scanner is connected to, while it still is one.
fn connected(world: &World, scanner: Entity) -> Option<Entity> {
    world
        .get::<BarcodeScanner>(scanner)?
        .computer
        .filter(|&c| world.get::<Ledger>(c).is_some())
}

/// `barcodescanner/attack_self`: the next mode.
pub(super) fn attack_self(world: &mut World, client: Entity, item: Entity) -> bool {
    let Some(scanner) = world.get::<BarcodeScanner>(item).copied() else {
        return false;
    };
    if connected(world, item).is_none() {
        tell(world, client, Span::Warning, "connect to computer!");
        return true;
    }
    let (mode, text) = match scanner.mode {
        ScanMode::CheckIn => (ScanMode::CheckOut, "check-out mode"),
        ScanMode::CheckOut => (ScanMode::Inventory, "inventory adding mode"),
        ScanMode::Inventory => (ScanMode::CheckIn, "check-in mode"),
    };
    if let Some(mut s) = world.get_mut::<BarcodeScanner>(item) {
        s.mode = mode;
    }
    tell(world, client, Span::Notice, text);
    sound_at(world, item, CLICK, 20.0, true);
    true
}

/// The scanner on the librarian's console or on a book. True when it was the scanner's.
pub(super) fn item_interaction(
    world: &mut World,
    client: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<BarcodeScanner>(held).is_none() {
        return false;
    }
    if world.get::<Ledger>(target).is_some() {
        connect(world, client, held, target);
        return true;
    }
    let Some(book) = world.get::<Book>(target).cloned() else {
        return false;
    };
    scan_book(world, client, held, &book);
    true
}

/// `bookmanagement/item_interaction`.
fn connect(world: &mut World, client: Entity, scanner: Entity, console: Entity) {
    if connected(world, scanner) == Some(console) {
        tell(world, client, Span::Warning, "already connected!");
        return;
    }
    if let Some(mut s) = world.get_mut::<BarcodeScanner>(scanner) {
        s.computer = Some(console);
    }
    tell(world, client, Span::Notice, "scanner connected");
    let name = name_of(world, console);
    say_around(
        world,
        console,
        Span::Notice,
        format!("{name} lets out a low, short blip."),
        &[],
    );
}

/// `interact_with_book`.
fn scan_book(world: &mut World, client: Entity, scanner: Entity, book: &Book) {
    let Some(console) = connected(world, scanner) else {
        tell(world, client, Span::Warning, "not connected to computer!");
        return;
    };
    let mode = world
        .get::<BarcodeScanner>(scanner)
        .map(|s| s.mode)
        .unwrap_or_default();
    let Some(mut ledger) = world.get::<Ledger>(console).cloned() else {
        return;
    };
    let result = match mode {
        ScanMode::CheckIn => match ledger.checkouts.iter().position(|l| l.book == *book) {
            Some(i) => {
                ledger.checkouts.remove(i);
                Ok("checked in")
            }
            None => Err("isn't checked out!"),
        },
        ScanMode::CheckOut => {
            if ledger.checkouts.iter().any(|l| l.book == *book) {
                Err("already checked out!")
            } else if let Some(copy) = ledger.inventory.iter().find(|b| *b == book) {
                ledger.checking_out = Some(copy.clone());
                Ok("set for check out")
            } else {
                Err("not in inventory!")
            }
        }
        ScanMode::Inventory => {
            ledger.inventory.push(book.clone());
            Ok("added to inventory")
        }
    };
    match result {
        Ok(text) => {
            world.entity_mut(console).insert(ledger);
            tell(world, client, Span::Notice, text);
            sound_at(world, scanner, BEEP, 20.0, false);
        }
        Err(text) => tell(world, client, Span::Warning, text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_loan_needs_a_book_set_for_check_out() {
        let mut ledger = Ledger::default();
        assert!(!ledger.lend("Bob", 60.0));
        ledger.checking_out = Some(Book {
            title: "Space Law".into(),
            ..Default::default()
        });
        assert!(ledger.lend("Bob", 60.0));
        assert_eq!(ledger.checkouts[0].loaned_to, "Bob");
    }
}
