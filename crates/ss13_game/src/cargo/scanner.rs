//! The universal scanner (server), after tg `/obj/item/universal_scanner`
//! (`code/modules/cargo/universal_scanner.dm`) in its export scanner mode: used on an object, it
//! tells its user what the object and its contents would sell for on the supply shuttle now
//! (`export_scan`, a dry run of [`super::export::export_value`]: elasticity, boosted exports and
//! the stock market included).
// shortcut: only the export scanner mode; the price tagger and sales tagger modes (custom vendor
// prices, barcodes for parcels), the paper supply and the bounty cube handling tip are not
// ported.

use bevy::prelude::*;
use ss13_mapload::{DmItem, DmName};
use ss13_net::chat::server::SendChat;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};

use super::MONEY_SYMBOL;
use super::export::{Exports, export_value, path_of};
use crate::obj_damage::is_under;

/// `/obj/item/universal_scanner`.
pub const UNIVERSAL_SCANNER: &str = "/obj/item/universal_scanner";
const SCAN_SOUND: &str = "sound/machines/terminal/terminal_select.ogg";

pub fn is_scanner(world: &World, item: Entity) -> bool {
    world
        .get::<DmItem>(item)
        .is_some_and(|i| is_under(&i.path, UNIVERSAL_SCANNER))
}

/// tg `export_scan`'s message: `(text, warning)`; the value is bold.
pub fn scan_message(target: &str, price: i64, has_contents: bool) -> (Vec<(bool, String)>, bool) {
    let money = |price: i64| (true, price.to_string());
    let mut parts = Vec::new();
    let mut warning = false;
    if has_contents {
        parts.push((false, format!("Scanned {target} and its contents")));
        if price != 0 {
            parts.push((false, ", total value: ".to_owned()));
            parts.push(money(price));
            parts.push((false, format!("{MONEY_SYMBOL}.")));
        } else {
            parts.push((false, ", no export values.".to_owned()));
            warning = true;
        }
    } else {
        parts.push((false, format!("Scanned {target}")));
        if price != 0 {
            parts.push((false, ", value: ".to_owned()));
            parts.push(money(price));
            parts.push((false, format!("{MONEY_SYMBOL}.")));
        } else {
            parts.push((false, ", no export value.".to_owned()));
            warning = true;
        }
    }
    (parts, warning)
}

/// tg `interact_with_atom` of the scanner: scans an object (`isobj`). `false` when `held` is not
/// a scanner or `target` not an object, so the click goes on.
pub(crate) fn item_interaction(
    world: &mut World,
    client: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if !is_scanner(world, held) || path_of(world, target).is_none() {
        return false;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let (price, has_contents) = match world.get_resource::<Exports>() {
        Some(exports) => export_value(world, exports, target, now),
        None => (0, false),
    };
    let name = world
        .get::<DmName>(target)
        .map(|n| {
            n.name
                .trim_start_matches("\\improper ")
                .trim_start_matches("\\proper ")
                .to_owned()
        })
        .unwrap_or_default();
    let (parts, warning) = scan_message(&name, price, has_contents);
    let (plain, bold) = if warning {
        (Span::Warning, Span::BoldWarning)
    } else {
        (Span::Notice, Span::BoldNotice)
    };
    let message = ChatMessage {
        channel: ChatChannel::Info,
        segments: parts
            .into_iter()
            .map(|(is_bold, text)| ChatSegment::new(if is_bold { bold } else { plain }, text))
            .collect(),
    };
    world.write_message(SendChat {
        clients: vec![client],
        message,
    });
    if price != 0
        && let Some(&pos) = world
            .get::<TilePos>(held)
            .or_else(|| {
                super::economy::holder_of(world, held).and_then(|m| world.get::<TilePos>(m))
            })
    {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, SCAN_SOUND, 50.0, true),
        );
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(parts: &[(bool, String)]) -> String {
        parts.iter().map(|(_, t)| t.as_str()).collect()
    }

    #[test]
    fn messages() {
        let (parts, warning) = scan_message("wrench", 2, false);
        assert_eq!(text(&parts), "Scanned wrench, value: 2¢.");
        assert!(!warning && parts[2].0, "the value is bold");
        let (parts, warning) = scan_message("crate", 227, true);
        assert_eq!(
            text(&parts),
            "Scanned crate and its contents, total value: 227¢."
        );
        assert!(!warning);
        let (parts, warning) = scan_message("toy", 0, false);
        assert_eq!(text(&parts), "Scanned toy, no export value.");
        assert!(warning);
        let (parts, warning) = scan_message("box", 0, true);
        assert_eq!(text(&parts), "Scanned box and its contents, no export values.");
        assert!(warning);
    }
}
