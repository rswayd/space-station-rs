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
        app.init_resource::<ChatLinks>()
            .add_message::<ChatLinkClicked>()
            .add_systems(
                Update,
                (
                    examine.after(ResolveClicks),
                    forward_links.in_set(ChatLinkSystems),
                ),
            );
    }
}

/// Links older than this are forgotten (tg keeps hrefs forever, the HUD links refuse after a
/// minute; this only bounds the table).
const LINK_FORGET_SECS: f64 = 600.0;

/// The chat links given out (tg hrefs in `to_chat`): by id, the client it was sent to, what it
/// means to the code that made it, and when (game seconds).
#[derive(Resource, Default)]
pub struct ChatLinks {
    next: u32,
    links: std::collections::HashMap<u32, (Entity, String, f64)>,
}

/// A client clicked a link it was given (BYOND `Topic(href)`): `topic` is what [`chat_link`]
/// was given, `age` the seconds since (tg `examine_time`). The owner of the topic checks
/// everything again.
#[derive(Message, Clone, Debug, PartialEq)]
pub struct ChatLinkClicked {
    pub client: Entity,
    pub topic: String,
    pub age: f64,
}

/// [`ChatLinkClicked`] is written here; readers run after it.
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChatLinkSystems;

/// A link for `client` meaning `topic`: put it in a chat segment as `Span::Link(id)`.
pub fn chat_link(world: &mut World, client: Entity, topic: impl Into<String>) -> u32 {
    let now = world
        .get_resource::<Time>()
        .map_or(0.0, Time::elapsed_secs_f64);
    let mut links = world.get_resource_or_insert_with(ChatLinks::default);
    links
        .links
        .retain(|_, (_, _, at)| now - *at < LINK_FORGET_SECS);
    links.next = links.next.wrapping_add(1);
    let id = links.next;
    links.links.insert(id, (client, topic.into(), now));
    id
}

/// `ClientIntent::ChatLink`: only a link given to that client counts.
fn forward_links(
    mut intents: MessageReader<ss13_net::server::IntentReceived>,
    links: Res<ChatLinks>,
    time: Res<Time>,
    mut clicked: MessageWriter<ChatLinkClicked>,
) {
    let now = time.elapsed_secs_f64();
    for i in intents.read() {
        let ss13_shared::protocol::ClientIntent::ChatLink(id) = i.intent else {
            continue;
        };
        if let Some((client, topic, at)) = links.links.get(&id)
            && *client == i.client
        {
            clicked.write(ChatLinkClicked {
                client: *client,
                topic: topic.clone(),
                age: now - at,
            });
        }
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

pub(crate) fn name_of(world: &World, entity: Entity) -> String {
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
        crate::restaurant::examine_lines(world, target, &mut lines);
        return (name, lines);
    }
    if world.get::<Health>(target).is_some() {
        // `get_visible_name`: a hidden face shows the worn ID's name or "Неизвестный".
        if let Some(visible) = crate::clothing::cover::hidden_face_name(world, target) {
            name = visible;
        }
        if let Some(seen) = crate::clothing::cover::unrecognized_name(world, examiner, target) {
            name = seen;
        }
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
        // `/datum/embedding/examined_tags`.
        if let Some(embed) = crate::embedding::embed_of(world, target) {
            lines.push((
                Span::Plain,
                if embed.is_harmless(false) {
                    "Этот предмет, вероятно, прилипнет к кому-нибудь, если его правильно бросить!"
                } else {
                    "У него тонкое остриё, и вероятно, он может вонзиться в кого-нибудь, если \
                     правильно бросить!"
                }
                .to_owned(),
            ));
        }
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
    crate::tools::examine_lines(world, target, &mut lines);
    crate::smoking::examine_lines(world, target, &mut lines);
    crate::kitchen_ware::examine_lines(world, target, &mut lines);
    crate::fancy::examine_lines(world, target, &mut lines);
    crate::toys::examine_lines(world, target, &mut lines);
    crate::cryosleep::examine_lines(world, target, &mut lines);
    crate::extinguisher::examine_lines(world, target, &mut lines);
    crate::chem::examine_lines(world, examiner, target, &mut lines);
    crate::chem_dispenser::examine_lines(world, target, &mut lines);
    crate::chem_master::examine_lines(world, target, &mut lines);
    crate::chem_heater::examine_lines(world, target, &mut lines);
    crate::chem_machines::examine_lines(world, target, &mut lines);
    crate::teapot::examine_lines(world, target, &mut lines);
    crate::cooking::examine_lines(world, target, &mut lines);
    crate::gibber::examine_lines(world, target, &mut lines);
    crate::food::examine_lines(world, target, examiner, &mut lines);
    crate::service::cards::examine_lines(world, examiner, target, &mut lines);
    crate::decay::examine_lines(world, target, &mut lines);
    crate::coffee::examine_lines(world, target, &mut lines);
    crate::cargo::examine_lines(world, target, &mut lines);
    crate::smartfridge::examine_lines(world, target, &mut lines);
    crate::botany::examine_lines(world, target, &mut lines);
    crate::atmos_machinery::examine_lines(world, target, &mut lines);
    crate::machine_frame::examine_lines(world, target, &mut lines);
    crate::reinforced::examine_lines(world, target, &mut lines);
    crate::vending::examine_lines(world, target, &mut lines);
    crate::wallmount::examine_lines(world, target, &mut lines);
    crate::power::lighting::examine_lines(world, target, &mut lines);
    crate::brig::timer::examine_lines(world, target, &mut lines);
    crate::brig::sentence::examine_lines(world, target, &mut lines);
    crate::furniture::examine_lines(world, target, &mut lines);
    crate::vehicles::examine_lines(world, target, &mut lines);
    crate::mecha::panel::examine_lines(world, target, &mut lines);
    crate::devices::examine_lines(world, target, &mut lines);
    crate::defenses::cameras::examine_lines(world, target, &mut lines);
    crate::silicon::cameras::examine_lines(world, target, &mut lines);
    crate::airlock_cycle::examine_lines(world, target, &mut lines);
    crate::airlock_security::examine_lines(world, target, &mut lines);
    crate::plumbing::examine_lines(world, target, &mut lines);
    crate::plumbing_net::examine_lines(world, target, &mut lines);
    crate::tank_storage::examine_lines(world, target, &mut lines);
    crate::safes::examine_lines(world, target, &mut lines);
    crate::secure_safes::examine_lines(world, target, &mut lines);
    crate::closets::picklock::examine_lines(world, target, &mut lines);
    crate::displaycase::examine_lines(world, target, &mut lines);
    crate::security_level::examine_lines(world, target, &mut lines);
    crate::emag::examine_lines(world, target, &mut lines);
    crate::grenades::examine_lines(world, target, &mut lines);
    crate::crayons::examine_lines(world, target, &mut lines);
    crate::stickers::examine_lines(world, target, &mut lines);
    crate::assemblies::server::examine_lines(world, target, &mut lines);
    crate::wiremod::server::examine_lines(world, target, &mut lines);
    crate::forensics::server::examine_lines(world, target, &mut lines);
    crate::radiation::examine_lines(world, target, &mut lines);
    crate::clothing::examine_lines(world, target, &mut lines);
    crate::fishing::fish::examine_lines(world, examiner, target, &mut lines);
    crate::fishing::server::spot_examine_lines(world, examiner, target, &mut lines);
    crate::fishing::mount::examine_lines(world, target, &mut lines);
    crate::mining::examine_lines(world, target, &mut lines);
    crate::science::quantum_pad::examine_lines(world, target, &mut lines);
    crate::science::teleportation::teleporter::examine_lines(world, target, &mut lines);
    crate::science::cell_lines::examine_lines(world, target, &mut lines);
    crate::silicon::ai_slipper::examine_lines(world, target, &mut lines);
    crate::misc_items::flatpacks::examine_lines(world, target, &mut lines);
    crate::antag::heretic::server::examine_lines(world, examiner, target, &mut lines);
    (name, lines)
}

/// `/mob/living/carbon/human/examine` for what this server models.
fn carbon_lines(world: &World, examiner: Entity, mob: Entity, lines: &mut Vec<Line>) {
    // `ru_p_they(TRUE)`, `ru_p_them()`, `ru_p_theirs()` of a male mob.
    let (he, his, them) = ("Он", "его", "него");
    let thing = |e: Option<Entity>| e.map(|e| name_of(world, e)).filter(|n| !n.is_empty());
    if let Some(inventory) = world.get::<Inventory>(mob) {
        // `get_clothing_examine_info`, in its order; what the clothing hides is left out
        // (`obscured_slots`).
        // shortcut: no blood or cuffs.
        let hidden = crate::clothing::cover::obscured(world, mob);
        let inventory = &crate::clothing::cover::shown(inventory, hidden);
        let worn = |slot: Option<Entity>, where_: &str, lines: &mut Vec<Line>| {
            if let Some(item) = thing(slot) {
                lines.push((Span::Plain, format!("{he} носит {item}{where_}.")));
            }
        };
        let accessories = inventory
            .uniform
            .map(|u| crate::clothing::accessories::examine_suffix(world, u))
            .unwrap_or_default();
        worn(inventory.uniform, &accessories, lines);
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
        let unconscious = crate::status_effects::knocked_out(world, mob);
        if unconscious {
            lines.push((
                Span::Notice,
                format!("{he} не реагирует на {his} окружение и, кажется, спит."),
            ));
        } else if stat == Stat::SoftCrit {
            lines.push((Span::Notice, format!("{he} едва находится в сознании.")));
        }
        crate::status_effects::examine_lines(world, mob, lines);
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
        // `get_hud_examine_info`: the security HUD lines and their links.
        for message in crate::records::sechud::examine_messages(
            world,
            click.client,
            click.examiner,
            click.target,
        ) {
            world.write_message(SendChat {
                clients: vec![click.client],
                message,
            });
        }
        // What is lodged in a carbon, with the links to pull it out.
        for message in
            crate::embedding::examine_messages(world, click.client, click.examiner, click.target)
        {
            world.write_message(SendChat {
                clients: vec![click.client],
                message,
            });
        }
        // Hidden prints for admins only (`R_ADMIN` checked inside).
        crate::forensics::server::admin_examine(world, click.client, click.target);
        crate::quirks::examined(world, click.examiner, click.target);
        // `photo/examine` and `picture_frame/examine` show the picture.
        crate::service::photo_examined(world, click.client, click.examiner, click.target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chat_link_answers_only_the_client_it_was_sent_to() {
        use bevy::ecs::system::RunSystemOnce;
        use ss13_net::server::IntentReceived;
        use ss13_shared::protocol::ClientIntent;
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<Messages<IntentReceived>>();
        world.init_resource::<Messages<ChatLinkClicked>>();
        let (a, b) = (world.spawn_empty().id(), world.spawn_empty().id());
        let id = chat_link(&mut world, a, "sechud:view:1");
        for client in [b, a] {
            world.write_message(IntentReceived {
                client,
                intent: ClientIntent::ChatLink(id),
            });
        }
        world.write_message(IntentReceived {
            client: a,
            intent: ClientIntent::ChatLink(id + 1),
        });
        world.run_system_once(forward_links).unwrap();
        let clicked: Vec<ChatLinkClicked> = world
            .resource_mut::<Messages<ChatLinkClicked>>()
            .drain()
            .collect();
        assert_eq!(clicked.len(), 1);
        assert_eq!(clicked[0].client, a);
        assert_eq!(clicked[0].topic, "sechud:view:1");
    }

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
