//! Forensics (server), after tg `code/modules/forensics/_forensics.dm`, `forensics_helpers.dm`,
//! `_onclick/item_attack.dm` (`attack_effects`), `detectivework/scanner.dm` and `evidence.dm`.
//!
//! - [`Forensics`] on an atom (`/datum/forensics`): fingerprints, hidden prints (ckey and name
//!   of every player who touched it, first and last time, for admins), blood DNA and fibers.
//! - A living human touching something ([`add_fingerprint`]: an empty-hand click, a pickup, an
//!   item it attacks with) leaves its fingerprint (`md5(dna.unique_identity)`, here
//!   [`fingerprint_of`]) unless gloves that cover the hands are worn: then the gloves get the
//!   print. Fingerless, cut, tackler and rapid gloves let prints through. Worn clothes may leave
//!   fibers (10-20 % per piece and touch, ×1.2 on items).
//! - Blood DNA is `unique_enzymes` → blood type: a brute hit with an item has a
//!   `25 + 2 × damage` % chance to bloody the weapon, the floor under the victim and the
//!   attacker's gloves (and clothes on a heavy hit); a bleeding mob's drips and pools carry its
//!   DNA. Items with blood are "blood-stained" on examine.
//! - `wash` (`crate::janitor::wash`) with `CLEAN_TYPE_FINGERPRINTS`, `CLEAN_TYPE_BLOOD` and
//!   `CLEAN_TYPE_FIBERS` wipes the matching lists ([`clean_act`]); hidden prints stay.
//! - The forensic scanner (`/obj/item/detective_scanner`) scans an adjacent atom, an item in the
//!   other hand, or one in sight within 8 tiles: fibers, blood, prints (a human's own when it
//!   wears no gloves), reagents and an ID card's access by region. The log entry is added
//!   after 3 s of analysis; the tgui `ForensicScanner` window (use in hand) lists, deletes,
//!   clears (also alt-click) and prints the log as an `FR-n 'Forensic Record'` paper after 3 s,
//!   which clears it.
//! - An evidence bag holds one item up to normal size: clicked on an item it bags it; its sprite
//!   shows the item, its weight class becomes the item's; used in hand it empties.
//! - Worn clothes with blood draw tg's `<state>blood` overlay of `icons/effects/blood.dmi` in the
//!   blood's color ([`blood_overlays`], `get_blood_overlay`); [`transfer_fingerprints_to`] moves
//!   prints when a thing becomes another (a wall frame mounted); doors take the prints of who
//!   bumps or clicks them (`try_to_activate_door`).
//! - An admin (`R_ADMIN`, checked here on the server) examining an atom also gets its hidden
//!   prints ([`admin_examine`]; tg shows them in the view-variables panel).
// shortcut: no blood overlay on items on the floor or in hands (tg draws `itemblood` inset into
// the item's sprite, a blend mode the renderer lacks), every suit bleeds as `suitblood` (no
// `blood_overlay_type` armor/coat in the converted data), no bloody hands or gloves
// transferring blood to what they touch (tg's `/atom/add_fibers` that would do it is never
// called), no bloody soles, no blood DNA from blood reagents (they carry only the type), no
// prints from strip menus or the AI; prints move only on the transfers ported here (most tg
// `transfer_fingerprints_to` callers build things not yet ported); `md5` is a 128-bit FNV pair
// (same length, never shown as md5).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmClothing, DmItem, DmName, DmWeight, IconIndex};
use ss13_net::server::ClientToken;
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::server::{
    AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_tgui::{UiInterface, UiStatus};

use super::{
    CLEAR, DELETE, ForensicScannerData, ForensicScannerUi, INDEX, PRINT, ScanCategory, ScanLog,
};
use crate::click::{RangedClick, ResolveClicks};
use crate::combat::CombatRng;
use crate::health::{Health, Stat};
use crate::inventory::{CarriedBy, IdCard, Inventory, slot_bit};
use crate::medical::blood::{Blood, BloodType};
use crate::server::PlayerMob;
use crate::storage::{Refusal, Storage, Stored};
use crate::tools::tell;

/// `CLEAN_TYPE_FINGERPRINTS`, `CLEAN_TYPE_FIBERS` (`code/__DEFINES/cleaning.dm`).
pub const CLEAN_TYPE_FINGERPRINTS: u32 = 1 << 1;
pub const CLEAN_TYPE_FIBERS: u32 = 1 << 2;
/// Body part bits `CHEST`, `HANDS` (`code/__DEFINES/inventory.dm`).
const CHEST: u32 = 1 << 0;
const HANDS: u32 = (1 << 9) | (1 << 10);
/// Gloves with `TRAIT_FINGERPRINT_PASSTHROUGH` in their `clothing_traits`.
const FINGERPRINT_PASSTHROUGH: [&str; 4] = [
    "/obj/item/clothing/gloves/fingerless",
    "/obj/item/clothing/gloves/cut",
    "/obj/item/clothing/gloves/tackler",
    "/obj/item/clothing/gloves/rapid",
];
pub const SCANNER: &str = "/obj/item/detective_scanner";
pub const EVIDENCE_BAG: &str = "/obj/item/evidencebag";
const PAPER: &str = "/obj/item/paper";
const STORAGE_DMI: &str = "icons/obj/storage/storage.dmi";
/// `stoplag(3 SECONDS)` of `scan`, `addtimer(..., 3 SECONDS)` of printing.
pub const SCAN_SECS: f64 = 3.0;
pub const PRINT_SECS: f64 = 3.0;
/// The scanner's `range`.
pub const SCAN_RANGE: u32 = 8;
/// `WEIGHT_CLASS_TINY`, `WEIGHT_CLASS_NORMAL`; the default `/datum/storage` `max_total_storage`.
const WEIGHT_CLASS_TINY: u8 = 1;
const WEIGHT_CLASS_NORMAL: u8 = 3;
const DEFAULT_MAX_TOTAL_STORAGE: u32 = 14;
/// `DETSCAN_ACCESS_ORDER()` (regions the station has).
const ACCESS_ORDER: [&str; 9] = [
    "Security",
    "Justice",
    "Engineering",
    "Supply",
    "General",
    "Medbay",
    "Command",
    "NT Representation",
    "Research",
];

/// `/datum/forensics`: the traces on an atom, in the order they were left.
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct Forensics {
    pub fingerprints: Vec<String>,
    /// ckey → `First: [time] "name"(gloves). Ckey: ckey` and the `Last:` line.
    pub hiddenprints: Vec<(String, String)>,
    /// `unique_enzymes` → blood type.
    pub blood_dna: Vec<(String, BloodType)>,
    pub fibers: Vec<String>,
    /// `fingerprintslast`: the ckey of the last player who touched it.
    pub last_ckey: Option<String>,
}

/// `/obj/item/detective_scanner`: its log, the scan being analysed and the report printing.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct DetectiveScanner {
    pub logs: Vec<ScanLog>,
    /// Game time the scan finishes analysing, and its entry.
    pub pending: Option<(f64, ScanLog)>,
    /// Game time the report comes out.
    pub printing: Option<f64>,
    /// `forensicPrintCount`.
    pub print_count: u32,
}

impl DetectiveScanner {
    /// `scanner_busy`.
    pub fn busy(&self) -> bool {
        self.pending.is_some() || self.printing.is_some()
    }
}

/// `/obj/item/evidencebag`: the item its sprite shows.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EvidenceBag {
    pub shown: Option<Entity>,
}

pub struct ForensicsServerPlugin;

impl Plugin for ForensicsServerPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // The forensic scanner's window, an evidence bag empties.
        app.add_click_handler(ClickPhase::AttackSelf, 400, |w, c| {
            crate::forensics::server::attack_self(w, c.client, c.mob, c.target)
        });
        // The forensic scanner scans the item in the other hand.
        app.add_click_handler(ClickPhase::OtherHand, 300, |w, c| {
            c.held.is_some_and(|h| {
                crate::forensics::server::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        // The forensic scanner scans, an evidence bag takes the item.
        app.add_click_handler(ClickPhase::HeldOnTarget, 1000, |w, c| {
            c.held.is_some_and(|h| {
                crate::forensics::server::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        super::spoofer::build(app);
        super::board::build(app);
        app.register_ui::<ForensicScannerUi>()
            .add_observer(add_item_parts)
            .add_systems(
                Update,
                (
                    ranged_scans.after(ResolveClicks),
                    scanner_timers,
                    evidence_bag_looks.after(crate::inventory::inventory_intents),
                    scanner_acts.in_set(UiSystems::Handle),
                    scanner_status.in_set(UiSystems::StatusHook),
                ),
            );
    }
}

/// The scanner's state (its window works while it is held: `AlwaysState` plus
/// [`scanner_status`]), the bag's `create_storage(max_slots = 1, WEIGHT_CLASS_NORMAL)`.
fn add_item_parts(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    if is_under(&item.path, SCANNER) {
        commands
            .entity(add.entity)
            .insert_if_new((DetectiveScanner::default(), AlwaysState));
    } else if is_under(&item.path, EVIDENCE_BAG) {
        commands.entity(add.entity).insert_if_new((
            EvidenceBag::default(),
            Storage {
                max_slots: 1,
                max_specific_storage: WEIGHT_CLASS_NORMAL,
                max_total_storage: DEFAULT_MAX_TOTAL_STORAGE,
            },
        ));
    }
}

// ---------------------------------------------------------------------------------------------
// Traces.

/// tg `md5(dna.unique_identity)`: 32 hex digits.
pub fn fingerprint_of(unique_identity: &str) -> String {
    let fnv = |seed: u64| {
        unique_identity.bytes().fold(seed, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    };
    format!(
        "{:016x}{:016x}",
        fnv(0xcbf2_9ce4_8422_2325),
        fnv(0x8422_2325_cbf2_9ce4)
    )
}

/// The fingerprint of a body with DNA.
pub fn mob_fingerprint(world: &World, mob: Entity) -> Option<String> {
    world
        .get::<crate::genetics::Dna>(mob)
        .map(|dna| fingerprint_of(&dna.unique_identity))
}

/// `get_blood_dna_list()`: the body's `unique_enzymes` and blood type.
pub fn blood_dna_of(world: &World, mob: Entity) -> Option<(String, BloodType)> {
    let dna = world.get::<crate::genetics::Dna>(mob)?;
    let blood = world.get::<Blood>(mob)?;
    Some((dna.unique_enzymes.clone(), blood.blood_type))
}

fn is_human(world: &World, mob: Entity) -> bool {
    world.get::<crate::human::HumanLook>(mob).is_some()
}

fn covers(world: &World, item: Option<Entity>, parts: u32) -> bool {
    item.and_then(|i| world.get::<DmClothing>(i))
        .is_some_and(|c| c.body_parts_covered & parts != 0)
}

fn path_of(world: &World, e: Entity) -> String {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

fn plain_name(world: &World, e: Entity) -> String {
    crate::vending::plain_name(&crate::chem::name_of(world, e)).to_owned()
}

/// BYOND `\a [thing]`: an article before an improper latin name.
fn a_name(world: &World, e: Entity) -> String {
    let raw = crate::chem::name_of(world, e);
    let name = crate::vending::plain_name(&raw).to_owned();
    match name.chars().next() {
        Some(c) if raw.starts_with("\\proper") || c.is_uppercase() || !c.is_ascii_alphabetic() => {
            name
        }
        Some(c) if "aeiou".contains(c) => format!("an {name}"),
        Some(_) => format!("a {name}"),
        None => name,
    }
}

/// Reads, changes and writes back `target`'s traces (creating them).
fn edit(world: &mut World, target: Entity, f: impl FnOnce(&mut Forensics)) {
    let Ok(mut entity) = world.get_entity_mut(target) else {
        return;
    };
    let mut forensics = entity.get::<Forensics>().cloned().unwrap_or_default();
    f(&mut forensics);
    entity.insert(forensics);
}

fn push_unique(list: &mut Vec<String>, value: String) {
    if !list.contains(&value) {
        list.push(value);
    }
}

/// BYOND `ckey()`.
fn ckey(key: &str) -> String {
    key.chars()
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '@')
        .collect()
}

/// `/datum/forensics/add_fingerprint(suspect, ignoregloves)`: only living mobs leave traces;
/// a human leaves fibers, and its print unless gloves covering the hands take it.
pub fn add_fingerprint(world: &mut World, target: Entity, suspect: Entity, ignore_gloves: bool) {
    if world.get_entity(target).is_err() || world.get::<Health>(suspect).is_none() {
        return;
    }
    add_hiddenprint(world, target, suspect);
    if !is_human(world, suspect) {
        return;
    }
    add_fibers(world, target, suspect);
    let gloves = world.get::<Inventory>(suspect).and_then(|i| i.gloves);
    if let Some(gloves) = gloves {
        let passthrough = !covers(world, Some(gloves), HANDS)
            || FINGERPRINT_PASSTHROUGH
                .iter()
                .any(|p| is_under(&path_of(world, gloves), p))
            || crate::clothing::traits::has_trait(
                world,
                suspect,
                crate::clothing::traits::FINGERPRINT_PASSTHROUGH,
            );
        if !ignore_gloves && !passthrough {
            if gloves != target {
                add_fingerprint(world, gloves, suspect, true);
            }
            return;
        }
    }
    let Some(print) = mob_fingerprint(world, suspect) else {
        return;
    };
    edit(world, target, |f| push_unique(&mut f.fingerprints, print));
}

/// `/datum/forensics/add_hiddenprint`: a player's ckey, name and the time, first and last.
pub fn add_hiddenprint(world: &mut World, target: Entity, suspect: Entity) {
    let Some(client) = world.get::<PlayerMob>(suspect).map(|p| p.client) else {
        return;
    };
    let key = world
        .get::<ClientToken>(client)
        .map(|t| ckey(&t.0))
        .unwrap_or_default();
    if key.is_empty() {
        return;
    }
    let gloves = if world
        .get::<Inventory>(suspect)
        .is_some_and(|i| i.gloves.is_some())
    {
        "(gloves)"
    } else {
        ""
    };
    let name = world
        .get::<Name>(suspect)
        .map(|n| n.as_str().to_owned())
        .unwrap_or_default();
    let time = crate::records::manifest::round_timestamp(world);
    let stamp = format!("[{time}] \"{name}\"{gloves}. Ckey: {key}");
    edit(world, target, |f| {
        match f.hiddenprints.iter_mut().find(|(k, _)| *k == key) {
            None => f
                .hiddenprints
                .push((key.clone(), format!("First: {stamp}"))),
            Some((_, text)) => {
                if let Some(cut) = text.find("\nLast: ") {
                    text.truncate(cut);
                }
                text.push_str("\nLast: ");
                text.push_str(&stamp);
            }
        }
        f.last_ckey = Some(key);
    });
}

/// `/datum/forensics/add_fibers`: each worn layer that shows may leave its fibers.
pub fn add_fibers(world: &mut World, target: Entity, suspect: Entity) {
    let Some(inventory) = world.get::<Inventory>(suspect).cloned() else {
        return;
    };
    let multiplier = if world.get::<DmItem>(target).is_some() {
        1.2
    } else {
        1.0
    };
    let gloves_text = |world: &World, g: Entity| {
        format!(
            "Material from a pair of {}.",
            crate::vending::plain_name(&crate::chem::name_of(world, g))
        )
    };
    // (text, percent) candidates in tg's order.
    let mut candidates: Vec<(String, f32)> = Vec::new();
    if let Some(suit) = inventory.suit {
        candidates.push((format!("Material from {}.", a_name(world, suit)), 10.0));
        if !covers(world, Some(suit), CHEST)
            && let Some(uniform) = inventory.uniform
        {
            candidates.push((format!("Fibers from {}.", a_name(world, uniform)), 12.0));
        }
        if !covers(world, Some(suit), HANDS)
            && let Some(gloves) = inventory.gloves
        {
            candidates.push((gloves_text(world, gloves), 20.0));
        }
    } else if let Some(uniform) = inventory.uniform {
        candidates.push((format!("Fibers from {}.", a_name(world, uniform)), 15.0));
        if let Some(gloves) = inventory.gloves {
            candidates.push((gloves_text(world, gloves), 20.0));
        }
    } else if let Some(gloves) = inventory.gloves {
        candidates.push((gloves_text(world, gloves), 20.0));
    }
    let Some(mut rng) = world.get_resource_mut::<CombatRng>() else {
        return;
    };
    let left: Vec<String> = candidates
        .into_iter()
        .filter(|(_, percent)| rng.chance(percent * multiplier))
        .map(|(text, _)| text)
        .collect();
    if left.is_empty() {
        return;
    }
    edit(world, target, |f| {
        for text in left {
            push_unique(&mut f.fibers, text);
        }
    });
}

/// `/obj/add_blood_DNA` and `/mob/living/add_blood_DNA`: merges the DNA into the atom's traces;
/// a human gets it on its clothes instead ([`add_blood_dna_to_items`]).
pub fn add_blood_dna(world: &mut World, target: Entity, dna: &[(String, BloodType)]) -> bool {
    if dna.is_empty() || world.get_entity(target).is_err() {
        return false;
    }
    if is_human(world, target) {
        return add_blood_dna_to_items(world, target, dna, DEFAULT_BLOODIED_SLOTS);
    }
    merge_blood(world, target, dna);
    true
}

fn merge_blood(world: &mut World, target: Entity, dna: &[(String, BloodType)]) {
    edit(world, target, |f| {
        for (enzymes, blood_type) in dna {
            match f.blood_dna.iter_mut().find(|(k, _)| k == enzymes) {
                Some(slot) => slot.1 = *blood_type,
                None => f.blood_dna.push((enzymes.clone(), *blood_type)),
            }
        }
    });
    redraw_wearer(world, target);
}

/// A worn or held item's blood changed: its wearer is drawn again (the worn blood overlay).
fn redraw_wearer(world: &mut World, item: Entity) {
    if let Some(&CarriedBy(mob)) = world.get::<CarriedBy>(item) {
        crate::inventory::refresh_look(world, mob);
    }
}

/// `/datum/blood_type` `color` (`get_color`) of the ported blood types.
pub fn blood_type_color(blood_type: BloodType) -> [u8; 4] {
    match blood_type {
        BloodType::Lizard => [0x03, 0x5a, 0x00, 0xff],
        // `/datum/reagent/consumable/liquidelectricity::color`.
        BloodType::Ethereal => [0x97, 0xee, 0x63, 0xff],
        BloodType::Oil => [0x2c, 0x2c, 0x2c, 0xff],
        // `/datum/reagent/toxin/slimejelly::color`.
        BloodType::Slime => [0xa6, 0x95, 0x9d, 0xff],
        // `/datum/reagent/water::color` `#AAAAAA77`.
        BloodType::Water => [0xaa, 0xaa, 0xaa, 0x77],
        BloodType::Skrell => [0x1e, 0x40, 0xff, 0xff],
        BloodType::Moth => [0xb9, 0xae, 0x9c, 0xff],
        _ => [0xaa, 0x10, 0x10, 0xff],
    }
}

/// `get_color_from_blood_list`: one blood type its own color, several the average of theirs.
pub fn blood_color(dna: &[(String, BloodType)]) -> [u8; 4] {
    match dna {
        [] => blood_type_color(BloodType::OPlus),
        [(_, only)] => blood_type_color(*only),
        _ => {
            let n = dna.len() as u32;
            let mut sum = [0u32; 3];
            for (_, blood_type) in dna {
                let c = blood_type_color(*blood_type);
                for (s, v) in sum.iter_mut().zip(c) {
                    *s += u32::from(v);
                }
            }
            let avg = |v: u32| ((v + n / 2) / n) as u8;
            [avg(sum[0]), avg(sum[1]), avg(sum[2]), 0xff]
        }
    }
}

/// `icons/effects/blood.dmi`, the worn blood overlays.
const BLOOD_DMI: &str = "icons/effects/blood.dmi";

/// tg `get_blood_overlay(blood_state)` of the worn clothes in `shown` (the inventory as drawn):
/// `(layer, overlay)` pairs just above each bloody piece, tinted with its blood's color.
pub fn blood_overlays(
    shown: &Inventory,
    forensics: &Query<&Forensics>,
    icons: &IconIndex,
) -> Vec<(f32, Overlay)> {
    use crate::human::layers::{
        FACEMASK_LAYER, GLOVES_LAYER, HEAD_LAYER, NECK_LAYER, SHOES_LAYER, SUIT_LAYER,
        UNIFORM_LAYER,
    };
    let worn = [
        (shown.uniform, UNIFORM_LAYER, "uniform"),
        (shown.suit, SUIT_LAYER, "suit"),
        (shown.gloves, GLOVES_LAYER, "glove"),
        (shown.shoes, SHOES_LAYER, "shoe"),
        (shown.head, HEAD_LAYER, "helmet"),
        (shown.mask, FACEMASK_LAYER, "mask"),
        (shown.neck, NECK_LAYER, "mask"),
    ];
    worn.into_iter()
        .filter_map(|(item, layer, state)| {
            let f = forensics
                .get(item?)
                .ok()
                .filter(|f| !f.blood_dna.is_empty())?;
            let id = icons.get(BLOOD_DMI, &format!("{state}blood"))?;
            Some((
                layer - 0.1,
                Overlay {
                    color: Some(blood_color(&f.blood_dna)),
                    ..Overlay::new(id)
                },
            ))
        })
        .collect()
}

/// `/atom/proc/transfer_fingerprints_to`: `from`'s prints and hidden prints (and who touched it
/// last) onto `to`, when a thing becomes another.
pub fn transfer_fingerprints_to(world: &mut World, from: Entity, to: Entity) {
    let Some(f) = world.get::<Forensics>(from).cloned() else {
        return;
    };
    edit(world, to, |t| {
        for print in f.fingerprints {
            push_unique(&mut t.fingerprints, print);
        }
        // `add_hiddenprint_list`: assoc assignment, the newer entry wins.
        for (key, text) in f.hiddenprints {
            match t.hiddenprints.iter_mut().find(|(k, _)| *k == key) {
                Some(slot) => slot.1 = text,
                None => t.hiddenprints.push((key, text)),
            }
        }
        t.last_ckey = f.last_ckey;
    });
}

/// `add_fingerprint_list` / `add_fiber_list` of one print or fiber (the forensics spoofer).
pub fn add_trace(world: &mut World, target: Entity, fiber: bool, value: String) {
    edit(world, target, |f| {
        push_unique(
            if fiber {
                &mut f.fibers
            } else {
                &mut f.fingerprints
            },
            value,
        );
    });
}

/// The admin's view of an atom's hidden prints (tg's view-variables `forensics`), sent on
/// examine to a client with `R_ADMIN` only.
pub fn admin_examine(world: &mut World, client: Entity, target: Entity) {
    let admin = world
        .get::<crate::votes::VoteRights>(client)
        .is_some_and(|r| r.admin);
    if !admin {
        return;
    }
    let Some(f) = world.get::<Forensics>(target) else {
        return;
    };
    if f.hiddenprints.is_empty() {
        return;
    }
    let mut text = String::from("Скрытые отпечатки (видны только администраторам):");
    for (_, entry) in &f.hiddenprints {
        text.push('\n');
        text.push_str(entry);
    }
    if let Some(last) = &f.last_ckey {
        text.push_str("\nПоследним касался: ");
        text.push_str(last);
    }
    tell(world, client, Span::Info, text);
}

/// `ITEM_SLOT_ICLOTHING|ITEM_SLOT_OCLOTHING|ITEM_SLOT_GLOVES|ITEM_SLOT_HEAD|ITEM_SLOT_MASK`.
const DEFAULT_BLOODIED_SLOTS: u32 = (1 << 1) | (1 << 0) | (1 << 2) | (1 << 6) | (1 << 5);

/// `add_blood_DNA_to_items`: the clothes worn in `slots` (`ITEM_SLOT_*`) get the blood; the
/// jumpsuit stays clean under a coat; bare hands (or a naked body) take it on the mob.
pub fn add_blood_dna_to_items(
    world: &mut World,
    mob: Entity,
    dna: &[(String, BloodType)],
    mut slots: u32,
) -> bool {
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return false;
    };
    if dna.is_empty() {
        return false;
    }
    if slots & slot_bit(InventorySlot::Suit) != 0 && covers(world, inventory.suit, CHEST) {
        slots &= !slot_bit(InventorySlot::Uniform);
    }
    let hands_bit = slot_bit(InventorySlot::LeftHand);
    let mut dirty_hands = slots & (slot_bit(InventorySlot::Gloves) | hands_bit) != 0;
    let worn: Vec<(InventorySlot, Entity)> = crate::inventory::ALL_SLOTS
        .into_iter()
        .filter(|s| !matches!(s, InventorySlot::LeftHand | InventorySlot::RightHand))
        .filter_map(|s| Some((s, inventory.slot(s)?)))
        .collect();
    for &(slot, item) in &worn {
        if slot_bit(slot) & slots != 0 {
            merge_blood(world, item, dna);
        }
        if covers(world, Some(item), HANDS) {
            dirty_hands = false;
        }
    }
    if slots & hands_bit != 0 {
        for held in inventory.hands.into_iter().flatten() {
            merge_blood(world, held, dna);
        }
    }
    if dirty_hands || worn.is_empty() {
        merge_blood(world, mob, dna);
    }
    true
}

/// `/datum/forensics/clean_act`: wipes the lists `clean_types` covers. True when something was
/// there to wipe.
pub fn clean_act(world: &mut World, target: Entity, clean_types: u32) -> bool {
    let Some(f) = world.get::<Forensics>(target) else {
        return false;
    };
    let prints = clean_types & CLEAN_TYPE_FINGERPRINTS != 0 && !f.fingerprints.is_empty();
    let blood = clean_types & crate::janitor::CLEAN_TYPE_BLOOD != 0 && !f.blood_dna.is_empty();
    let fibers = clean_types & CLEAN_TYPE_FIBERS != 0 && !f.fibers.is_empty();
    if !(prints || blood || fibers) {
        return false;
    }
    let mut f = world.get_mut::<Forensics>(target).expect("checked above");
    if prints {
        f.fingerprints.clear();
    }
    if blood {
        f.blood_dna.clear();
    }
    if fibers {
        f.fibers.clear();
    }
    if blood {
        redraw_wearer(world, target);
    }
    true
}

/// The decal element's `get_examine_name`: an item with blood on it.
pub fn blood_stained(world: &World, target: Entity) -> bool {
    world.get::<DmItem>(target).is_some()
        && world
            .get::<Forensics>(target)
            .is_some_and(|f| !f.blood_dna.is_empty())
}

/// A bleeding mob's drip or pool (`crate::janitor::bleed_splatter`) takes its blood DNA.
pub fn bloody_splatter(world: &mut World, mob: Entity, splat: Entity) {
    if let Some(dna) = blood_dna_of(world, mob) {
        merge_blood(world, splat, &[dna]);
    }
}

/// tg `/obj/item/attack` and `attacked_by` -> `attack_effects` of a hit with `weapon` that did
/// `damage` (after armor): the weapon gets the attacker's prints, a brute hit may spread the
/// victim's blood.
pub fn item_attack(
    world: &mut World,
    attacker: Entity,
    target: Entity,
    weapon: Entity,
    damage: f32,
    brute: bool,
) {
    attack_effects(world, attacker, target, weapon, damage, brute);
    add_fingerprint(world, weapon, attacker, false);
}

/// `/mob/living/attack_effects`.
fn attack_effects(
    world: &mut World,
    attacker: Entity,
    target: Entity,
    weapon: Entity,
    damage: f32,
    brute: bool,
) {
    if damage <= 0.0 || !brute {
        return;
    }
    // `can_bleed`: a body with blood (and DNA to leave).
    let Some(dna) = blood_dna_of(world, target) else {
        return;
    };
    if !world
        .resource_mut::<CombatRng>()
        .chance(25.0 + damage * 2.0)
    {
        return;
    }
    let dna = [dna];
    merge_blood(world, weapon, &dna);
    let target_pos = world.get::<TilePos>(target).copied();
    if let Some(pos) = target_pos
        && let Some(splat) = crate::janitor::add_splatter_floor(world, pos, false)
    {
        merge_blood(world, splat, &dna);
    }
    let near = match (world.get::<TilePos>(attacker).copied(), target_pos) {
        (Some(a), Some(t)) => a.chebyshev(t).is_some_and(|d| d <= 1),
        _ => false,
    };
    if !near {
        return;
    }
    if !is_human(world, attacker) {
        merge_blood(world, attacker, &dna);
        return;
    }
    let mut slots = slot_bit(InventorySlot::Gloves);
    let mut rng = world.resource_mut::<CombatRng>();
    if damage >= 20.0 || (damage >= 15.0 && rng.chance(25.0)) {
        slots |= slot_bit(InventorySlot::Uniform) | slot_bit(InventorySlot::Suit);
        if rng.chance(33.0) && damage >= 10.0 {
            slots |= slot_bit(InventorySlot::Shoes);
        }
        if rng.chance(33.0) && damage >= 24.0 {
            slots |= slot_bit(InventorySlot::Mask);
        }
        if rng.chance(33.0) && damage >= 30.0 {
            slots |= slot_bit(InventorySlot::Head);
        }
    }
    add_blood_dna_to_items(world, attacker, &dna, slots);
}

// ---------------------------------------------------------------------------------------------
// The forensic scanner.

/// Items whose click on a mob is theirs, not an attack (`NOBLUDGEON`, `interact_with_atom`).
pub fn interacts_with_mobs(path: &str) -> bool {
    is_under(path, SCANNER) || is_under(path, EVIDENCE_BAG)
}

/// What a scan of `target` finds (`scan` before its `stoplag`).
pub fn scan_log(world: &World, target: Entity) -> ScanLog {
    let mut log = ScanLog {
        scan_target: plain_name(world, target),
        scan_time: crate::records::manifest::round_timestamp(world),
        data_entries: Vec::new(),
    };
    let traces = world.get::<Forensics>(target);
    if let Some(f) = traces.filter(|f| !f.fibers.is_empty()) {
        log.add(
            ScanCategory::Fiber,
            f.fibers.iter().map(|t| (t.clone(), t.clone())),
        );
    }
    if let Some(f) = traces.filter(|f| !f.blood_dna.is_empty()) {
        log.add(
            ScanCategory::Blood,
            f.blood_dna
                .iter()
                .map(|(dna, t)| (dna.clone(), t.name().to_owned())),
        );
    }
    if is_human(world, target) {
        let gloved = world
            .get::<Inventory>(target)
            .is_some_and(|i| i.gloves.is_some());
        if !gloved && let Some(print) = mob_fingerprint(world, target) {
            log.add(ScanCategory::Fingers, [(print.clone(), print)]);
        }
    } else if world.get::<Health>(target).is_none() {
        if let Some(f) = traces.filter(|f| !f.fingerprints.is_empty()) {
            log.add(
                ScanCategory::Fingers,
                f.fingerprints.iter().map(|p| (p.clone(), p.clone())),
            );
        }
        if let (Some(reagents), Some(chem)) = (
            world.get::<crate::chem::Reagents>(target),
            world.get_resource::<crate::chem::Chem>(),
        ) {
            for r in &reagents.0.reagents {
                log.add(
                    ScanCategory::Reagents,
                    [(chem.0.def(r.id).name.clone(), crate::chem::dm_num(r.volume))],
                );
            }
        }
    }
    if let Some(card) = world.get::<IdCard>(target) {
        for region in ACCESS_ORDER {
            let Some((_, accesses)) = crate::id_access::STATION_REGIONS
                .iter()
                .find(|(name, _)| *name == region)
            else {
                continue;
            };
            let names: Vec<String> = accesses
                .iter()
                .filter(|a| card.access.iter().any(|c| c == *a))
                .map(|a| crate::id_access::access_desc(a).unwrap_or(*a).to_owned())
                .collect();
            if !names.is_empty() {
                log.add(
                    ScanCategory::Access,
                    [(region.to_owned(), english_list(&names))],
                );
            }
        }
    }
    log
}

/// DM `english_list`: `a, b and c`.
fn english_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// `safe_scan` -> `scan(user, atom)`: the scanner in `mob`'s hands starts analysing `target`.
pub fn scan(world: &mut World, client: Entity, mob: Entity, scanner: Entity, target: Entity) {
    let Some(busy) = world.get::<DetectiveScanner>(scanner).map(|s| s.busy()) else {
        return;
    };
    if busy {
        tell(world, client, Span::Warning, "scanner busy!");
        return;
    }
    // `loc != user`.
    if !world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.holds(scanner))
    {
        return;
    }
    if let Some(pos) = world.get::<TilePos>(mob).copied() {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "industrial_scan", 20.0, true).extrarange(-2.0),
        );
    }
    let (user, tool, thing) = (
        plain_name(world, mob),
        plain_name(world, scanner),
        plain_name(world, target),
    );
    crate::chem::visible(
        world,
        mob,
        &format!("You scan {thing}. The scanner is now analysing the results..."),
        &format!("{user} points {tool} at {thing} and performs a forensic scan."),
        Span::Notice,
    );
    let log = scan_log(world, target);
    let done = world.resource::<Time>().elapsed_secs_f64() + SCAN_SECS;
    if let Some(mut s) = world.get_mut::<DetectiveScanner>(scanner) {
        s.pending = Some((done, log));
    }
}

/// `interact_with_atom` of a held scanner or evidence bag on an adjacent `target`. A storage
/// takes the scanner instead (`SHOULD_SKIP_INTERACTION`). True when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<DetectiveScanner>(held).is_some() {
        if world.get::<Storage>(target).is_some() {
            return false;
        }
        scan(world, client, mob, held, target);
        return true;
    }
    if world.get::<EvidenceBag>(held).is_some() {
        return bag_item(world, client, held, target);
    }
    super::spoofer::item_interaction(world, client, mob, held, target)
}

/// A click past reach: the scanner scans what is in sight within its range.
fn ranged_scans(world: &mut World, mut cursor: Local<MessageCursor<RangedClick>>) {
    let clicks: Vec<RangedClick> = cursor
        .read(world.resource::<Messages<RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(scanner) = world
            .get::<Inventory>(click.mob)
            .and_then(Inventory::active_item)
            .filter(|&i| world.get::<DetectiveScanner>(i).is_some())
        else {
            continue;
        };
        if crate::mobs::ai::behaviors::can_see(world, click.mob_pos, click.target_pos, SCAN_RANGE) {
            scan(world, click.client, click.mob, scanner, click.target);
        }
    }
}

/// The scans and reports that are due.
pub fn scanner_timers(
    time: Res<Time>,
    mut scanners: Query<(Entity, &mut DetectiveScanner)>,
    mut commands: Commands,
) {
    let now = time.elapsed_secs_f64();
    // Cost: one check per scanner (a few per map) per tick.
    for (scanner, mut state) in &mut scanners {
        if state.pending.as_ref().is_some_and(|(at, _)| now >= *at)
            && let Some((_, log)) = state.pending.take()
        {
            state.logs.push(log);
        }
        if state.printing.is_some_and(|at| now >= at) {
            state.printing = None;
            commands.queue(move |world: &mut World| print_report(world, scanner));
        }
    }
}

/// `print_report`: the log on paper at the scanner, into its holder's hands; the log clears.
// The report is also the paper's description, as before the paper had text.
pub fn print_report(world: &mut World, scanner: Entity) {
    let Some(mut state) = world.get_mut::<DetectiveScanner>(scanner) else {
        return;
    };
    state.print_count += 1;
    let number = state.print_count;
    let logs = std::mem::take(&mut state.logs);
    let mut text = format!("Forensic Record - (FR-{number})\n---\n");
    for log in &logs {
        text.push_str(&log.report_text());
    }
    text.push_str("Notes:\n");
    let Some(pos) = crate::chem::map_pos(world, scanner) else {
        return;
    };
    let Some(paper) = crate::construction::spawn_obj(world, PAPER, pos) else {
        return;
    };
    world.entity_mut(paper).insert(DmName {
        name: format!("FR-{number} 'Forensic Record'"),
        desc: text.clone(),
    });
    crate::service::write_printed(world, paper, text);
    if let Some(&CarriedBy(mob)) = world.get::<CarriedBy>(scanner)
        && let Some(inventory) = world.get::<Inventory>(mob).cloned()
    {
        crate::storage::put_in_hands(world, mob, &inventory, paper);
        if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
            tell(world, client, Span::Notice, "logs cleared");
        }
    }
}

/// `click_alt` / the `clear` act: `clear_logs`. True when `item` is a scanner.
pub fn clear_logs(world: &mut World, client: Entity, item: Entity) -> bool {
    let Some(state) = world.get::<DetectiveScanner>(item) else {
        return false;
    };
    let text = if state.logs.is_empty() {
        "no logs!"
    } else if state.busy() {
        "scanner busy!"
    } else {
        if let Some(mut s) = world.get_mut::<DetectiveScanner>(item) {
            s.logs.clear();
        }
        "logs cleared"
    };
    tell(world, client, Span::Notice, text);
    true
}

/// `attack_self`: the scanner opens its window, the bag empties. True when `item` is one.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world.get::<DetectiveScanner>(item).is_some() {
        // `IS_UNCONSCIOUS_OR_CRIT(user)`: the click comes from a conscious mob.
        world.write_message(OpenUi::new::<ForensicScannerUi>(
            client,
            mob,
            item,
            "Forensic Scanner",
        ));
        return true;
    }
    if super::spoofer::attack_self(world, client, mob, item) {
        return true;
    }
    if world.get::<EvidenceBag>(item).is_some() {
        empty_bag(world, client, mob, item);
        return true;
    }
    false
}

impl ServerUi for ForensicScannerUi {
    type Source = &'static DetectiveScanner;
    type UserParam = ();
    type Act = ScannerAct;

    fn static_data(_: &DetectiveScanner) {}

    fn data(scanner: &DetectiveScanner, _: Entity, _: &()) -> ForensicScannerData {
        ForensicScannerData {
            logs: scanner.logs.clone(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ScannerAct, UiReject> {
        Ok(match action {
            CLEAR => ScannerAct::Clear,
            PRINT => ScannerAct::Print,
            DELETE => {
                let index = params.number(INDEX)?;
                if index < 0.0 || index.fract() != 0.0 || index > f64::from(u32::MAX) {
                    return Err(UiReject::BadParams(format!("{INDEX}: {index}")));
                }
                ScannerAct::Delete(index as usize)
            }
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScannerAct {
    Clear,
    Delete(usize),
    Print,
}

/// The window stays open while the actor holds the scanner; disabled out of consciousness.
/// The spoofer's window needs it in a hand (`hands_state`).
fn scanner_status(mut windows: Query<&mut UiWindow>, users: Query<(&Inventory, Option<&Stat>)>) {
    for mut window in &mut windows {
        if window.interface == super::ForensicsSpooferUi::NAME {
            let (holding, conscious) = users.get(window.actor).map_or((false, false), |(i, s)| {
                (
                    i.hands.contains(&Some(window.source)),
                    s.is_none_or(|s| *s == Stat::Conscious),
                )
            });
            if let Some(status) = super::spoofer::window_status(holding, conscious) {
                window.cap_status(status);
            }
            continue;
        }
        if window.interface != ForensicScannerUi::NAME {
            continue;
        }
        match users.get(window.actor) {
            Ok((inventory, stat)) if inventory.holds(window.source) => {
                if stat.is_some_and(|s| *s != Stat::Conscious) {
                    window.cap_status(UiStatus::Disabled);
                }
            }
            _ => window.cap_status(UiStatus::Close),
        }
    }
}

/// `ui_act`, checked again at the moment of effect.
fn scanner_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ForensicScannerUi>>>) {
    let pending: Vec<(Entity, Entity, ScannerAct)> = acts
        .read(world.resource::<Messages<UiActed<ForensicScannerUi>>>())
        .map(|a| (a.client, a.source, a.act))
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for (client, scanner, act) in pending {
        let Some(state) = world.get::<DetectiveScanner>(scanner) else {
            continue;
        };
        let (busy, count) = (state.busy(), state.logs.len());
        match act {
            ScannerAct::Clear => {
                clear_logs(world, client, scanner);
            }
            ScannerAct::Delete(index) if index < count => {
                if busy {
                    tell(world, client, Span::Warning, "scanner busy!");
                    continue;
                }
                if let Some(mut s) = world.get_mut::<DetectiveScanner>(scanner) {
                    s.logs.remove(index);
                }
                tell(world, client, Span::Notice, "log deleted");
            }
            ScannerAct::Delete(_) => {}
            ScannerAct::Print => {
                if count == 0 {
                    tell(world, client, Span::Warning, "no logs!");
                    continue;
                }
                if busy {
                    tell(world, client, Span::Warning, "scanner busy!");
                    continue;
                }
                if let Some(mut s) = world.get_mut::<DetectiveScanner>(scanner) {
                    s.printing = Some(now + PRINT_SECS);
                }
                if let Some(pos) = crate::chem::map_pos(world, scanner) {
                    crate::sound::playsound(
                        world,
                        crate::sound::PlaySound::new(
                            pos,
                            "sound/machines/printer.ogg",
                            50.0,
                            false,
                        ),
                    );
                }
                tell(world, client, Span::Notice, "printing report...");
            }
        }
    }
}

/// Examine lines of the scanner (`examine`) and the "blood-stained" prefix is in
/// `crate::examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    if let Some(state) = world.get::<DetectiveScanner>(target)
        && !state.logs.is_empty()
        && !state.busy()
    {
        lines.push((Span::Notice, "Alt-click to clear scanner logs.".to_owned()));
    }
}

// ---------------------------------------------------------------------------------------------
// Evidence bags.

/// The bag's storage `COLLECT_ONE` quick gather: the clicked item on the floor goes in. A
/// storage on the floor takes the bag instead (its `attackby`).
fn bag_item(world: &mut World, client: Entity, bag: Entity, target: Entity) -> bool {
    if world.get::<DmItem>(target).is_none()
        || world.get::<TilePos>(target).is_none()
        || world.get::<Storage>(target).is_some()
    {
        return false;
    }
    if !world.get::<DmItem>(target).is_some_and(DmItem::pickable) {
        return false;
    }
    match crate::storage::can_insert(world, bag, target) {
        Ok(()) => crate::storage::put_inside(world, target, bag),
        Err(Refusal::TooBig) => tell(world, client, Span::Warning, "не помещается!"),
        Err(Refusal::Full) => tell(world, client, Span::Warning, "нет места!"),
        Err(_) => {}
    }
    true
}

/// `attack_self`: `atom_storage.remove_all()` onto the user's tile.
fn empty_bag(world: &mut World, client: Entity, mob: Entity, bag: Entity) {
    let name = plain_name(world, bag);
    let items: Vec<Entity> = world
        .get::<Stored>(bag)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    if items.is_empty() {
        tell(world, client, Span::Notice, format!("{name} is empty."));
        return;
    }
    let Some(pos) = world.get::<TilePos>(mob).copied() else {
        return;
    };
    for item in items {
        crate::storage::take_out(world, item, pos);
    }
    let user = plain_name(world, mob);
    crate::chem::visible(
        world,
        mob,
        &format!("You empty {name}."),
        &format!("{user} empties {name}."),
        Span::Notice,
    );
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(
            pos,
            "sound/items/evidence_bag/evidence_bag_unzip.ogg",
            50.0,
            true,
        ),
    );
}

/// `update_icon_state`, `update_overlays`, `update_desc` and the weight class of a bag whose
/// contents changed.
// Cost: one comparison per evidence bag (a few dozen per map) per tick.
fn evidence_bag_looks(
    mut bags: Query<(Entity, &mut EvidenceBag, Option<&Stored>)>,
    looks: Query<&Appearance>,
    weights: Query<&DmWeight>,
    names: Query<&DmName>,
    icons: Option<Res<IconIndex>>,
    mut commands: Commands,
) {
    let Some(icons) = icons else {
        return;
    };
    let (Some(empty), Some(full)) = (
        icons.get(STORAGE_DMI, "evidenceobj"),
        icons.get(STORAGE_DMI, "evidence"),
    ) else {
        return;
    };
    for (bag, mut state, stored) in &mut bags {
        let inside = stored.and_then(|s| s.items().first().copied());
        if state.shown == inside {
            continue;
        }
        state.shown = inside;
        let Ok(look) = looks.get(bag) else {
            continue;
        };
        let mut look = look.clone();
        look.overlays.clear();
        let (w_class, desc) = match inside {
            None => {
                look.icon_state = empty;
                (WEIGHT_CLASS_TINY, "An empty evidence bag.".to_owned())
            }
            Some(item) => {
                look.icon_state = full;
                if let Ok(inner) = looks.get(item) {
                    look.overlays.push(Overlay {
                        color: inner.color,
                        ..Overlay::new(inner.icon_state)
                    });
                    look.overlays.extend(inner.overlays.iter().cloned());
                }
                look.overlays.push(Overlay::new(full));
                let (name, inner_desc) = names.get(item).map_or_else(
                    |_| (String::new(), String::new()),
                    |n| {
                        (
                            crate::vending::plain_name(&n.name).to_owned(),
                            n.desc.clone(),
                        )
                    },
                );
                (
                    weights.get(item).map_or(WEIGHT_CLASS_TINY, |w| w.w_class),
                    format!("An evidence bag containing {name}. {inner_desc}"),
                )
            }
        };
        let name = names.get(bag).map(|n| n.name.clone()).unwrap_or_default();
        commands
            .entity(bag)
            .insert((look, DmWeight { w_class }, DmName { name, desc }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blood_colors_mix_by_average() {
        let red = [0xaa, 0x10, 0x10, 0xff];
        assert_eq!(blood_color(&[]), red, "no DNA: O+ red");
        assert_eq!(
            blood_color(&[("a".into(), BloodType::Water)]),
            [0xaa, 0xaa, 0xaa, 0x77],
            "one type keeps its alpha"
        );
        let mixed = blood_color(&[
            ("a".into(), BloodType::APlus),
            ("b".into(), BloodType::Lizard),
        ]);
        assert_eq!(mixed, [0x57, 0x35, 0x08, 0xff]);
    }

    #[test]
    fn prints_move_with_the_newer_hidden_print_winning() {
        let mut world = World::new();
        let from = world
            .spawn(Forensics {
                fingerprints: vec!["p1".into()],
                hiddenprints: vec![("ck".into(), "new".into())],
                last_ckey: Some("ck".into()),
                ..default()
            })
            .id();
        let to = world
            .spawn(Forensics {
                fingerprints: vec!["p1".into(), "p0".into()],
                hiddenprints: vec![("ck".into(), "old".into())],
                ..default()
            })
            .id();
        transfer_fingerprints_to(&mut world, from, to);
        let f = world.get::<Forensics>(to).unwrap();
        assert_eq!(f.fingerprints, vec!["p1".to_owned(), "p0".to_owned()]);
        assert_eq!(f.hiddenprints, vec![("ck".to_owned(), "new".to_owned())]);
        assert_eq!(f.last_ckey.as_deref(), Some("ck"));
    }
}
