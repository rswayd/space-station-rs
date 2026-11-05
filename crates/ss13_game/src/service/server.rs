//! The service items' plugin (server): their state on spawn, the click hooks the inventory chain
//! calls ([`attack_self`], [`interact`], [`item_interaction`]), the prompts' answers and the
//! `PaperSheet` / `MarkdownViewer` windows. The mechanics live in the sibling modules.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::service::{BOOK_ROOT, DICE_ROOT, PAPER_ROOT};
use ss13_mapload::{DmItem, DmName, DmObj, DmService};

use crate::obj_damage::is_under;
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::prompt::{PromptAnswer, PromptAnswered, PromptSystems};
use ss13_tgui::server::{
    AppTguiExt, ServerUi, UiParams, UiReject, UiSystems, UiWindow, distance_status,
};
use ss13_tgui::{UiInterface as _, UiStatus};

use super::library::Book;
use super::paper::Paper;
use super::{MarkdownViewerData, MarkdownViewerUi, PaperSheetData, PaperSheetUi};
use crate::inventory::Inventory;
use crate::server::PlayerMob;

pub struct ServiceServerPlugin;

impl Plugin for ServiceServerPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // Paper and books are read, dice rolled, horns honked.
        app.add_click_handler(ClickPhase::AttackSelf, 3000, |w, c| {
            crate::service::server::attack_self(w, c.client, c.mob, c.target)
        });
        // A pen or stamp on the paper in the other hand.
        app.add_click_handler(ClickPhase::OtherHand, 900, |w, c| {
            c.held.is_some_and(|h| {
                crate::service::server::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        // tg `attack_hand` of paper bins, bookcases and desk bells.
        app.add_click_handler(ClickPhase::EmptyHand, 3200, |w, c| {
            crate::service::server::interact(w, c.client, c.mob, c.target)
        });
        // Pens, stamps, the labeler, books, the bible, horns.
        app.add_click_handler(ClickPhase::HeldOnTarget, 1700, |w, c| {
            c.held.is_some_and(|h| {
                crate::service::server::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        app.register_ui::<PaperSheetUi>()
            .register_ui::<MarkdownViewerUi>()
            .init_resource::<super::religion::Religion>()
            .add_systems(Startup, super::library::shelve_mapped_books)
            .add_systems(
                Update,
                (
                    prompt_answers
                        .in_set(UiSystems::Handle)
                        .after(PromptSystems::Answer),
                    window_status.in_set(UiSystems::StatusHook),
                    super::clown::squeak_on_moves.after(ss13_world::WorldSystems::Movement),
                ),
            )
            .add_observer(add_item_parts)
            .add_observer(add_service_parts)
            .add_observer(add_obj_parts)
            .add_observer(super::clown::mime_takes_the_vow);
    }
}

/// A question waiting for its answer, on the prompt entity.
#[derive(Component, Clone, Debug)]
pub(super) enum PendingAsk {
    /// A pen's text for a paper.
    Write { paper: Entity, pen: Entity },
    /// The hand labeler's label.
    Label { labeler: Entity },
    /// A book off a bookcase; the list as it was asked.
    TakeBook { case: Entity, books: Vec<Entity> },
    /// The chaplain's names, the null rod, the beacon, the bible's skin, the altar.
    Religion(super::religion::ReligionAsk),
}

/// `Initialize` of the ported items, once all their prototype components are in.
fn add_item_parts(add: On<Add<DmItem>>, mut commands: Commands) {
    let e = add.entity;
    commands.queue(move |world: &mut World| {
        let Some(path) = world.get::<DmItem>(e).map(|i| i.path.clone()) else {
            return;
        };
        super::clown::add_parts(world, e, &path);
        add_data_parts(world, e);
    });
}

/// Prototype components arrive one insert at a time, so `DmItem` may come before or after
/// `DmService`: the parts that read `DmService` wait for both (whichever is added second).
fn add_service_parts(add: On<Add<DmService>>, mut commands: Commands) {
    let e = add.entity;
    commands.queue(move |world: &mut World| add_data_parts(world, e));
}

fn add_data_parts(world: &mut World, e: Entity) {
    let Some(path) = world.get::<DmItem>(e).map(|i| i.path.clone()) else {
        return;
    };
    let reads_data = [PAPER_ROOT, BOOK_ROOT, DICE_ROOT]
        .iter()
        .any(|root| is_under(&path, root));
    if reads_data && world.get::<DmService>(e).is_none() {
        return;
    }
    super::paper::add_parts(world, e, &path);
    super::library::add_parts(world, e, &path);
    super::games::add_parts(world, e, &path);
}

fn add_obj_parts(add: On<Add<DmObj>>, mut commands: Commands) {
    let e = add.entity;
    commands.queue(move |world: &mut World| {
        let Some(path) = world.get::<DmObj>(e).map(|o| o.path.clone()) else {
            return;
        };
        super::clown::add_obj_parts(world, e, &path);
        super::library::add_obj_parts(world, e, &path);
    });
}

/// tg `attack_self` of the held service item. True when it was one.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    super::paper::attack_self(world, client, mob, item)
        || super::religion::attack_self(world, client, mob, item)
        || super::library::attack_self(world, client, mob, item)
        || super::games::attack_self(world, client, mob, item)
        || super::clown::attack_self(world, mob, item)
}

/// tg `attack_hand` of paper bins, bookcases and desk bells. True when it was one.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    super::paper::take_from_bin(world, client, mob, target)
        || super::library::bookcase_hand(world, client, mob, target)
        || super::clown::ring_hand(world, client, mob, target)
}

/// tg `item_interaction` / `attack` of the service items in hand (and of what they are used
/// on). True when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    // `COMSIG_ITEM_ATTACK` of squeaky things and `attackby` of desk bells: a sound, then the
    // click goes on.
    super::clown::used_on(world, mob, held, target);
    super::paper::item_interaction(world, client, mob, held, target)
        || super::religion::item_interaction(world, client, mob, held, target)
        || super::library::item_interaction(world, client, mob, held, target)
}

fn prompt_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, actor, answer) in answers {
        let Some(pending) = world.get::<PendingAsk>(prompt).cloned() else {
            continue;
        };
        // The tg `tgui_input_*` callers all bail on a closed prompt; the actor must still be up.
        let conscious =
            world.get::<crate::health::Stat>(actor) == Some(&crate::health::Stat::Conscious);
        if !conscious {
            continue;
        }
        match (pending, answer) {
            (PendingAsk::Write { paper, pen }, Some(PromptAnswer::Text(text))) => {
                super::paper::write_answer(world, actor, paper, pen, &text);
            }
            (PendingAsk::Label { labeler }, Some(PromptAnswer::Text(text))) => {
                super::paper::label_answer(world, actor, labeler, &text);
            }
            (PendingAsk::Label { labeler }, None) => {
                super::paper::label_answer(world, actor, labeler, "");
            }
            (PendingAsk::TakeBook { case, books }, Some(PromptAnswer::Item(i))) => {
                if let Some(&book) = books.get(i) {
                    super::library::take_book(world, actor, case, book);
                }
            }
            (PendingAsk::Religion(ask), answer) => {
                super::religion::answer(world, actor, ask, answer);
            }
            _ => {}
        }
    }
}

impl ServerUi for PaperSheetUi {
    type Source = (&'static Paper, Option<&'static DmName>);
    type UserParam = ();
    type Act = ();

    fn static_data((paper, name): (&Paper, Option<&DmName>)) -> PaperSheetData {
        PaperSheetData {
            paper_name: name.map(|n| n.name.clone()).unwrap_or_default(),
            raw_text_input: paper.inputs.clone(),
            raw_stamp_input: paper.stamps.clone(),
        }
    }

    fn data(_: (&Paper, Option<&DmName>), _: Entity, _: &()) {}

    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

impl ServerUi for MarkdownViewerUi {
    type Source = &'static Book;
    type UserParam = ();
    type Act = ();

    fn static_data(book: &Book) -> MarkdownViewerData {
        MarkdownViewerData {
            title: book.title.clone(),
            author: book.author.clone(),
            content: book.content.clone(),
        }
    }

    fn data(_: &Book, _: Entity, _: &()) {}

    fn parse_act(_: &str, _: &UiParams) -> Result<(), UiReject> {
        Err(UiReject::UnknownAction)
    }
}

/// tg `paper/ui_status`: readable while held or near (the source has `AlwaysState`, so the
/// distance check is here); blind readers lose the window.
fn window_status(
    mut windows: Query<&mut UiWindow>,
    users: Query<(&Inventory, Option<&TilePos>)>,
    positions: Query<&TilePos>,
) {
    for mut window in &mut windows {
        if window.interface != PaperSheetUi::NAME && window.interface != MarkdownViewerUi::NAME {
            continue;
        }
        let status = match users.get(window.actor) {
            Ok((inventory, _)) if inventory.holds(window.source) => UiStatus::Interactive,
            Ok((_, Some(at))) => positions
                .get(window.source)
                .map_or(UiStatus::Close, |pos| distance_status(*at, *pos)),
            _ => UiStatus::Close,
        };
        window.cap_status(status);
    }
}

/// The DM type of an item or object (`""` for anything else).
pub(super) fn item_path(world: &World, e: Entity) -> String {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.clone())
        .or_else(|| world.get::<DmObj>(e).map(|o| o.path.clone()))
        .unwrap_or_default()
}

pub(super) fn client_of(world: &World, mob: Entity) -> Option<Entity> {
    world.get::<PlayerMob>(mob).map(|p| p.client)
}

/// The job title of a player's mob (tg `mind.assigned_role.title`).
pub(super) fn job_title(world: &World, mob: Entity) -> Option<String> {
    let job = world.get::<crate::jobs::PlayerJob>(mob)?;
    let jobs = world.get_resource::<crate::jobs::Jobs>()?;
    Some(jobs.data.jobs.get(job.0)?.title.clone())
}

/// tg `visible_message` around `source` (its tile, or its holder's), with personal lines.
pub(super) fn say_around(
    world: &mut World,
    source: Entity,
    span: Span,
    others: String,
    personal: &[(Entity, String)],
) {
    let Some(at) = crate::chem::map_pos(world, source) else {
        return;
    };
    let personal: Vec<(Entity, Span, String)> = personal
        .iter()
        .map(|(mob, text)| (*mob, span, text.clone()))
        .collect();
    let lines: Vec<SendChat> =
        crate::pull::visible_message(world, at, ChatChannel::Local, (span, others), &personal);
    for line in lines {
        world.write_message(line);
    }
}

/// `playsound(source, file, vol, vary)`.
pub(super) fn sound_at(world: &mut World, source: Entity, file: &str, vol: f32, vary: bool) {
    if let Some(at) = crate::chem::map_pos(world, source) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(at, file, vol, vary));
    }
}

/// The sprite `state` of `icon`, when the manifest has it.
pub(super) fn set_icon_state(world: &mut World, e: Entity, icon: &str, state: &str) {
    let id = world
        .get_resource::<ss13_mapload::IconIndex>()
        .and_then(|i| i.get(icon, state));
    if let Some(id) = id
        && let Some(mut look) = world.get_mut::<Appearance>(e)
        && look.icon_state != id
    {
        look.icon_state = id;
    }
}

/// tg `update_static_data_for_all_viewers`.
pub(super) fn refresh_windows(world: &mut World, source: Entity) {
    let mut windows = world.query::<&mut UiWindow>();
    for mut window in windows.iter_mut(world) {
        if window.source == source {
            window.refresh_static();
        }
    }
}

/// The text a paper or book was spawned with, for the item kinds that read it.
pub(super) fn service_data(world: &World, e: Entity) -> DmService {
    world.get::<DmService>(e).cloned().unwrap_or_default()
}
