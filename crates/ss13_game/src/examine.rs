//! Examine (server), after tg `run_examinate` (`mob.dm`), `atom_examine.dm` (`examine`,
//! `examine_title`), `items.dm` (`examine_tags`: the weight class) and `carbon/examine.dm`
//! (clothes, held items, damage, grab, crit, death, SSD), with BandaStation's strings.
//!
//! A shift-click (`crate::click::ExamineClick`, from any client that sees the target, also in crit
//! or as a ghost) sends the examiner's client the title line ("Это <name>.") and one chat line per
//! examine entry. The examiner's living mob turns to face the target (tg `face_atom`).
// shortcut: no darkness rule (tg refuses to examine unlit tiles farther than one), no
// double-examine (`examine_more`), no pronouns but the male ones (player mobs have no gender
// yet), no species, wounds, blood or scars; add with lighting on the server and characters.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmName, DmWeight};
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::{Facing, TilePos};

use crate::click::{ExamineClick, ResolveClicks};
use crate::ghost::Ghost;
use crate::health::{Health, Stat};
use crate::inventory::Inventory;
use crate::pull::{GrabState, PulledBy, dir_between};
use crate::server::PlayerMob;

/// BYOND widescreen `view` of tg (19x15): what a client can see around its mob.
const VIEW_HALF: (i32, i32) = (9, 7);

pub struct ExamineServerPlugin;

impl Plugin for ExamineServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, examine.after(ResolveClicks));
    }
}

/// tg `weight_class_to_text`.
pub fn weight_class_text(w_class: u8) -> &'static str {
    match w_class {
        1 => "крохотного размера",
        2 => "маленького размера",
        3 => "обычного размера",
        4 => "громоздкого размера",
        5 => "огромного размера",
        6 => "гигантского размера",
        _ => "",
    }
}

/// One examine entry: a styled line.
pub type Line = (Span, String);

fn name_of(world: &World, entity: Entity) -> String {
    if let Some(name) = world.get::<Name>(entity) {
        return name.as_str().to_owned();
    }
    world
        .get::<DmName>(entity)
        .map_or_else(String::new, |n| n.name.clone())
}

/// tg `examine()` of `target` as `examiner` sees it: the title, then the entries.
pub fn examine_lines(world: &World, examiner: Entity, target: Entity) -> (String, Vec<Line>) {
    let mut name = name_of(world, target);
    // The blood decal's `get_examine_name`.
    if crate::forensics::server::blood_stained(world, target) {
        name = format!("blood-stained {name}");
    }
    let mut lines = Vec::new();
    if world.get::<crate::mobs::BasicMob>(target).is_some() {
        crate::mobs::examine_lines(world, target, &mut lines);
        return (name, lines);
    }
    if world.get::<Health>(target).is_some() {
        carbon_lines(world, examiner, target, &mut lines);
        return (name, lines);
    }
    let desc = world.get::<DmName>(target).map(|n| n.desc.clone());
    if let Some(desc) = desc.filter(|d| !d.is_empty()) {
        lines.push((Span::Italics, desc));
    }
    // `/obj/item/examine_tags`: the size first, then `examine_descriptor` "предмет".
    if world.get::<DmItem>(target).is_some() {
        let w_class = world.get::<DmWeight>(target).copied().unwrap_or_default();
        lines.push((
            Span::Plain,
            format!("Это {} предмет.", weight_class_text(w_class.w_class)),
        ));
    }
    // `multitool/examine`.
    if matches!(
        crate::tools::tool_of(world, target),
        Some((crate::tools::ToolKind::Multitool, _))
    ) {
        let line = match crate::tools::multitool_buffer(world, target) {
            Some(b) => {
                let name = world
                    .get::<DmName>(b)
                    .map(|n| crate::vending::plain_name(&n.name).to_owned())
                    .unwrap_or_default();
                format!("В буфере: {name}.")
            }
            None => "Буфер пуст.".to_owned(),
        };
        lines.push((Span::Notice, line));
    }
    crate::chem::examine_lines(world, examiner, target, &mut lines);
    crate::chem_dispenser::examine_lines(world, target, &mut lines);
    crate::chem_master::examine_lines(world, target, &mut lines);
    crate::chem_heater::examine_lines(world, target, &mut lines);
    crate::cooking::examine_lines(world, target, &mut lines);
    crate::gibber::examine_lines(world, target, &mut lines);
    crate::food::examine_lines(world, target, examiner, &mut lines);
    crate::decay::examine_lines(world, target, &mut lines);
    crate::coffee::examine_lines(world, target, &mut lines);
    crate::smartfridge::examine_lines(world, target, &mut lines);
    crate::botany::examine_lines(world, target, &mut lines);
    crate::atmos_machinery::examine_lines(world, target, &mut lines);
    crate::machine_frame::examine_lines(world, target, &mut lines);
    crate::vending::examine_lines(world, target, &mut lines);
    crate::wallmount::examine_lines(world, target, &mut lines);
    crate::security_level::examine_lines(world, target, &mut lines);
    crate::emag::examine_lines(world, target, &mut lines);
    crate::grenades::examine_lines(world, target, &mut lines);
    crate::forensics::server::examine_lines(world, target, &mut lines);
    (name, lines)
}

/// `/mob/living/carbon/human/examine` for what this server models.
fn carbon_lines(world: &World, examiner: Entity, mob: Entity, lines: &mut Vec<Line>) {
    // `ru_p_they(TRUE)`, `ru_p_them()`, `ru_p_theirs()` of a male mob.
    let (he, his, them) = ("Он", "его", "него");
    let thing = |e: Option<Entity>| e.map(|e| name_of(world, e)).filter(|n| !n.is_empty());
    if let Some(inventory) = world.get::<Inventory>(mob) {
        // `get_clothing_examine_info`, in its order.
        // shortcut: no `obscured_slots` (a helmet does not hide the mask), no blood or cuffs.
        let worn = |slot: Option<Entity>, where_: &str, lines: &mut Vec<Line>| {
            if let Some(item) = thing(slot) {
                lines.push((Span::Plain, format!("{he} носит {item}{where_}.")));
            }
        };
        worn(inventory.uniform, "", lines);
        worn(inventory.head, " на голове", lines);
        worn(inventory.mask, " на лице", lines);
        worn(inventory.neck, " на шее", lines);
        worn(inventory.eyes, " на глазах", lines);
        worn(inventory.ears, " на ушах", lines);
        worn(inventory.suit, "", lines);
        if let Some(suit) = thing(inventory.suit) {
            worn(inventory.suit_storage, &format!(" на {suit}"), lines);
        }
        worn(inventory.back, " на спине", lines);
        worn(inventory.id, "", lines);
        for (hand, side) in ["левой руке", "правой руке"].into_iter().enumerate()
        {
            if let Some(held) = thing(inventory.hands[hand]) {
                // BandaStation writes the capitalised `t_His` here; lower case reads right.
                lines.push((Span::Plain, format!("{he} держит {held} в {his} {side}.")));
            }
        }
        worn(inventory.gloves, " на руках", lines);
        worn(inventory.belt, " на поясе", lines);
        worn(inventory.shoes, " на ногах", lines);
    }
    let stat = world.get::<Stat>(mob).copied().unwrap_or_default();
    let has_client = world.get::<PlayerMob>(mob).is_some();
    if stat == Stat::Dead {
        if !lines.is_empty() {
            lines.push((Span::Plain, String::new()));
        }
        // `generate_death_examine_text`: a soul that can come back (the client in the body or
        // its ghost connected) or none.
        let mut ghosts = world.try_query::<(&Ghost, &PlayerMob)>();
        let ghost_connected = ghosts
            .as_mut()
            .is_some_and(|q| q.iter(world).any(|(g, _)| g.body == Some(mob)));
        let text = if has_client || ghost_connected {
            format!("{he} выглядит обмякло и не реагирует; нет признаков жизни...")
        } else {
            format!(
                "{he} выглядит обмякло и не реагирует; нет признаков жизни, и {his} душа ушла..."
            )
        };
        lines.push((Span::Danger, text));
    }
    if let Some(health) = world.get::<Health>(mob) {
        for (damage, kind) in [(health.brute, "ушибы"), (health.burn, "ожоги")] {
            if damage <= 0.0 {
                continue;
            }
            lines.push(if damage < 25.0 {
                (Span::Danger, format!("У {them} незначительные {kind}."))
            } else if damage < 50.0 {
                (Span::Danger, format!("У {them} умеренные {kind}!"))
            } else {
                (Span::Danger, format!("У {them} тяжелые {kind}!"))
            });
        }
    }
    if world.get::<crate::human::Husk>(mob).is_some() {
        lines.push((
            Span::Warning,
            "Это тело превратилось в гротескную шелуху.".to_owned(),
        ));
    }
    if let Some(puller) = world.get::<PulledBy>(mob).map(PulledBy::puller)
        && world
            .get::<GrabState>(puller)
            .is_some_and(|g| *g > GrabState::Passive)
    {
        let by = name_of(world, puller);
        lines.push((Span::Warning, format!("Его держит в захвате {by}.")));
    }
    if stat != Stat::Dead {
        let unconscious = world
            .get::<Health>(mob)
            .is_some_and(|h| h.knocked_out(stat));
        if unconscious {
            lines.push((
                Span::Notice,
                format!("{he} не реагирует на {his} окружение и, кажется, спит."),
            ));
        } else if stat == Stat::SoftCrit {
            lines.push((Span::Notice, format!("{he} едва находится в сознании.")));
        }
        // A body without its player (tg: no `client`) stares blankly.
        if !has_client && examiner != mob {
            lines.push((
                Span::Danger,
                format!(
                    "У {them} пустой и рассеянный взгляд и, кажется, он совершенно ни на что не \
                     реагирует. {he}, возможно, скоро опомнится."
                ),
            ));
        }
    }
}

/// The examine block as chat messages: the title line, then one message per entry.
pub fn examine_messages(name: &str, lines: Vec<Line>) -> Vec<ChatMessage> {
    let mut out = vec![ChatMessage {
        channel: ChatChannel::Info,
        segments: vec![
            ChatSegment::new(Span::Plain, "Это "),
            ChatSegment::new(Span::Bold, name),
            ChatSegment::new(Span::Plain, "."),
        ],
    }];
    out.extend(
        lines
            .into_iter()
            .filter(|(_, text)| !text.is_empty())
            .map(|(span, text)| ChatMessage::simple(ChatChannel::Info, span, text)),
    );
    out
}

/// tg `run_examinate`, checked at the moment of effect: the target still exists on the map within
/// the examiner's view.
fn examine(world: &mut World, mut clicks: Local<MessageCursor<ExamineClick>>) {
    let clicks: Vec<ExamineClick> = clicks
        .read(world.resource::<Messages<ExamineClick>>())
        .copied()
        .collect();
    for click in clicks {
        let (Some(&from), Some(&to)) = (
            world.get::<TilePos>(click.examiner),
            world.get::<TilePos>(click.target),
        ) else {
            continue;
        };
        let in_view = from.z == to.z
            && (to.x - from.x).abs() <= VIEW_HALF.0
            && (to.y - from.y).abs() <= VIEW_HALF.1;
        if !in_view {
            continue;
        }
        if world.get::<Stat>(click.examiner).is_some()
            && let Some(dir) = dir_between(from, to)
            && let Some(mut facing) = world.get_mut::<Facing>(click.examiner)
        {
            facing.set_if_neq(Facing(dir));
        }
        let (name, lines) = examine_lines(world, click.examiner, click.target);
        for message in examine_messages(&name, lines) {
            world.write_message(SendChat {
                clients: vec![click.client],
                message,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_follow_tg() {
        assert_eq!(weight_class_text(3), "обычного размера");
        assert_eq!(weight_class_text(4), "громоздкого размера");
        assert_eq!(weight_class_text(9), "");
    }

    #[test]
    fn a_dead_body_without_a_soul_and_a_toolbox() {
        let mut world = World::new();
        let examiner = world.spawn(Name::new("A")).id();
        let body = world
            .spawn((Name::new("B"), Health::default(), Stat::Dead))
            .id();
        let (name, lines) = examine_lines(&world, examiner, body);
        assert_eq!(name, "B");
        assert!(
            lines
                .iter()
                .any(|(_, l)| l.contains("нет признаков жизни, и его душа ушла")),
            "{lines:?}"
        );
        let toolbox = world
            .spawn((
                DmName {
                    name: "toolbox".into(),
                    desc: "Danger. Very robust.".into(),
                },
                DmItem::default(),
                DmWeight { w_class: 4 },
            ))
            .id();
        let (name, lines) = examine_lines(&world, examiner, toolbox);
        let messages = examine_messages(&name, lines);
        let text: Vec<String> = messages.iter().map(ChatMessage::plain_text).collect();
        assert_eq!(
            text,
            [
                "Это toolbox.",
                "Danger. Very robust.",
                "Это громоздкого размера предмет."
            ]
        );
    }
}
