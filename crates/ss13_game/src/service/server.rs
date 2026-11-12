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
use ss13_tgui::server::{AppTguiExt, ServerUi, UiSystems, UiWindow, distance_status};
use ss13_tgui::{UiInterface as _, UiStatus};

use super::library::Book;
use super::paper::Paper;
use super::{MarkdownViewerData, MarkdownViewerUi, PaperInput, PaperSheetData, PaperSheetUi};
use crate::inventory::Inventory;

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
        // `deck/attack_hand` (draw a card; a right click: face up), `singlecard/attack_hand_secondary`.
        app.add_click_handler(ClickPhase::EmptyHand, 3211, |w, c| {
            super::cards::attack_hand(w, c.client, c.mob, c.target, c.modifiers.right)
        });
        // Cards, hands and decks used on each other; pens marking cards.
        app.add_click_handler(ClickPhase::HeldOnTarget, 1691, |w, c| {
            c.held.is_some_and(|h| {
                super::cards::item_interaction(w, c.client, c.mob, h, c.target, c.modifiers.right)
            })
        });
        // The same on the card thing in the other hand; an empty hand draws from a held deck.
        app.add_click_handler(ClickPhase::OtherHand, 901, |w, c| match c.held {
            Some(h) => super::cards::item_interaction(w, c.client, c.mob, h, c.target, false),
            None => super::cards::attack_hand(w, c.client, c.mob, c.target, false),
        });
        // `papercutter/attack_hand_secondary`: a right click cuts its paper.
        app.add_click_handler(ClickPhase::EmptyHand, 3201, |w, c| {
            c.modifiers.right && super::stationery::cutter_right_click(w, c.client, c.mob, c.target)
        });
        // `filingcabinet/wrench_act_secondary` (a left click files the wrench).
        app.add_click_handler(ClickPhase::HeldOnTarget, 1690, |w, c| {
            c.modifiers.right
                && c.held.is_some_and(|h| {
                    super::stationery::cabinet_wrench(w, c.client, c.mob, h, c.target)
                })
        });
        // Pens, stamps, the labeler, books, the bible, horns.
        app.add_click_handler(ClickPhase::HeldOnTarget, 1700, |w, c| {
            c.held.is_some_and(|h| {
                crate::service::server::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        // A camera takes a picture of what it is used on (`camera/interact_with_atom`), after
        // tables and storages took it.
        app.add_click_handler(ClickPhase::HeldOnTarget, 4759, |w, c| {
            !c.modifiers.right
                && c.held.is_some_and(|h| {
                    w.get::<super::photo::Camera>(h).is_some()
                        && super::photo::snap(w, c.client, c.mob, h, c.target)
                })
        });
        // `wallframe/picture/attack_hand` from the other hand: its photo out.
        app.add_click_handler(ClickPhase::OtherHand, 909, |w, c| {
            c.held.is_none() && super::photo::frame_take_photo(w, c.client, c.mob, c.target)
        });
        // `picture_frame/attack_hand` shows its photo (before the signs' examine).
        app.add_click_handler(ClickPhase::EmptyHand, 3149, |w, c| {
            super::photo::frame_hand(w, c.client, c.mob, c.target)
        });
        // Razors, lipstick, the dye spray and mirrors (`super::barber`).
        super::barber::build(app);
        // The slot machine (`super::slots`).
        super::slots::build(app);
        // The punished, festival, dream and sparring gods.
        super::burden::build(app);
        super::festival::build(app);
        super::dream_sect::build(app);
        super::sparring::build(app);
        app.register_ui::<PaperSheetUi>()
            .register_ui::<MarkdownViewerUi>()
            .register_ui::<super::DsmBookUi>()
            .register_ui::<super::IdcBookUi>()
            .register_ui::<super::LibraryConsoleUi>()
            .register_ui::<super::PhotoUi>()
            .init_resource::<super::religion::Religion>()
            .init_resource::<super::archive::LibraryArchive>()
            .init_resource::<super::stationery::RapsheetCount>()
            .add_systems(Startup, super::blanks::load_blanks)
            .add_systems(ss13_world::MapInit, super::library::shelve_mapped_books)
            .add_systems(ss13_world::MapInit, super::stationery::file_mapped_items)
            .add_systems(ss13_world::MapInit, super::stationery::arm_mapped_cutters)
            .add_systems(
                Update,
                (
                    prompt_answers
                        .in_set(UiSystems::Handle)
                        .after(PromptSystems::Answer),
                    window_status.in_set(UiSystems::StatusHook),
                    super::manuals::window_status.in_set(UiSystems::StatusHook),
                    super::manuals::flip_sounds.in_set(UiSystems::Handle),
                    super::library_console::console_acts.in_set(UiSystems::Handle),
                    super::photo::ranged_snaps.after(crate::click::ResolveClicks),
                    run_timers.run_if(any_with_component::<ServiceTimer>),
                    super::office::load_starting_toner
                        .run_if(any_with_component::<super::office::StartingToner>),
                    super::clown::squeak_on_moves.after(ss13_world::WorldSystems::Movement),
                    super::cards::drop_unheld_wield
                        .run_if(any_with_component::<super::cards::DeckWielded>),
                    super::id_stickers::ctrl_shift_clicks.after(crate::click::ResolveClicks),
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
    /// A pen's text for a paper: at the end, or into a field (`(input, field id, fields then)`).
    Write {
        paper: Entity,
        pen: Entity,
        field: Option<(usize, usize, usize)>,
    },
    /// Where a pen writes on a paper with fields: the end (item 0) or `fields[item - 1]`.
    WriteWhere {
        paper: Entity,
        pen: Entity,
        fields: Vec<(usize, usize)>,
    },
    /// Clipboards, folders, the photocopier and the fax.
    Office(super::office::OfficeAsk),
    /// Book alterations and the library's machines.
    Archive(super::archive::ArchiveAsk),
    /// Canvases.
    Art(super::art::ArtAsk),
    /// The hand labeler's label.
    Label { labeler: Entity },
    /// A book off a bookcase; the list as it was asked.
    TakeBook { case: Entity, books: Vec<Entity> },
    /// The chaplain's names, the null rod, the beacon, the bible's skin, the altar.
    Religion(super::religion::ReligionAsk),
    /// Biscuit cards and filing cabinets.
    Stationery(super::stationery::StationeryAsk),
    /// Playing cards.
    Cards(super::cards::CardsAsk),
    /// Cameras and photos.
    Photo(super::photo::PhotoAsk),
    /// The mime's book: which routine.
    Mimery(super::clown::MimeryAsk),
}

/// `Initialize` of the ported items, once all their prototype components are in.
fn add_item_parts(add: On<Add<DmItem>>, mut commands: Commands) {
    let e = add.entity;
    commands.queue(move |world: &mut World| {
        let Some(path) = world.get::<DmItem>(e).map(|i| i.path.clone()) else {
            return;
        };
        super::clown::add_parts(world, e, &path);
        super::office::add_parts(world, e, &path);
        super::art::add_parts(world, e, &path);
        super::statues::add_parts(world, e, &path);
        super::stationery::add_parts(world, e, &path);
        super::cards::add_parts(world, e, &path);
        super::manuals::add_parts(world, e, &path);
        super::barcode::add_parts(world, e, &path);
        super::photo::add_parts(world, e, &path);
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
        super::office::add_obj_parts(world, e, &path);
        super::art::add_obj_parts(world, e, &path);
        super::archive::add_obj_parts(world, e, &path);
        super::barcode::add_obj_parts(world, e, &path);
        super::statues::add_obj_parts(world, e, &path);
        super::stationery::add_obj_parts(world, e, &path);
        super::slots::add_obj_parts(world, e, &path);
        super::photo::add_obj_parts(world, e, &path);
        super::tickets::add_obj_parts(world, e, &path);
    });
}

/// tg `attack_self` of the held service item. True when it was one.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    super::paper::attack_self(world, client, mob, item)
        || super::photo::attack_self(world, client, mob, item)
        || super::stationery::attack_self(world, client, mob, item)
        || super::office::attack_self(world, client, mob, item)
        || super::art::attack_self(world, client, mob, item)
        || super::statues::attack_self(world, client, mob, item)
        || super::religion::attack_self(world, client, mob, item)
        || super::library::attack_self(world, client, mob, item)
        || super::manuals::attack_self(world, client, mob, item)
        || super::barcode::attack_self(world, client, item)
        || super::games::attack_self(world, client, mob, item)
        || super::cards::attack_self(world, client, mob, item)
        || super::clown::attack_self(world, mob, item)
        || super::clown::mimery_attack_self(world, client, mob, item)
}

/// tg `attack_hand` of paper bins, bookcases, desk bells, office and library machines. True
/// when it was one.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    super::paper::take_from_bin(world, client, mob, target)
        || super::library::bookcase_hand(world, client, mob, target)
        || super::clown::ring_hand(world, client, mob, target)
        || super::office::interact(world, client, mob, target)
        || super::archive::interact(world, client, mob, target)
        || super::stationery::interact(world, client, mob, target)
        || super::tickets::take_ticket(world, client, mob, target)
}

/// tg `click_alt` of papers (folded into a plane; a carbon sheet tears its copy off first) and
/// of the paper cutter. True when it was theirs.
pub fn click_alt(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    super::cards::click_alt(world, client, mob, target)
        || super::photo::click_alt(world, client, mob, target)
        || super::stationery::click_alt(world, client, mob, target)
        || super::office::fold_plane(world, client, mob, target)
        || super::barber::click_alt(world, client, mob, target)
        || super::art::frame_click_alt(world, client, mob, target)
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
    super::barcode::item_interaction(world, client, held, target)
        || super::paper::item_interaction(world, client, mob, held, target)
        || super::photo::item_interaction(world, client, mob, held, target)
        || super::stationery::item_interaction(world, client, mob, held, target)
        || super::office::item_interaction(world, client, mob, held, target)
        || super::religion::item_interaction(world, client, mob, held, target)
        || super::library::item_interaction(world, client, mob, held, target)
        || super::archive::item_interaction(world, client, mob, held, target)
        || super::art::item_interaction(world, client, mob, held, target)
        || super::statues::item_interaction(world, client, mob, held, target)
        || super::tickets::item_interaction(world, client, mob, held, target)
        || super::id_stickers::item_interaction(world, client, mob, held, target)
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
            (PendingAsk::Write { paper, pen, field }, Some(PromptAnswer::Text(text))) => {
                super::paper::write_answer(world, actor, paper, pen, field, &text);
            }
            (PendingAsk::WriteWhere { paper, pen, fields }, Some(PromptAnswer::Item(i))) => {
                super::paper::where_answer(world, actor, paper, pen, &fields, i);
            }
            (PendingAsk::Office(ask), Some(answer)) => {
                super::office::answer(world, actor, ask, answer);
            }
            (PendingAsk::Archive(ask), Some(answer)) => {
                super::archive::answer(world, actor, ask, answer);
            }
            (PendingAsk::Art(ask), Some(answer)) => {
                super::art::answer(world, actor, ask, answer);
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
            (PendingAsk::Stationery(ask), answer) => {
                super::stationery::answer(world, actor, ask, answer);
            }
            (PendingAsk::Cards(ask), answer) => {
                super::cards::answer(world, actor, ask, answer);
            }
            (PendingAsk::Photo(ask), answer) => {
                super::photo::answer(world, actor, ask, answer);
            }
            (PendingAsk::Mimery(ask), Some(PromptAnswer::Item(i))) => {
                super::clown::mimery_answer(world, actor, ask, i);
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
            // The fields' children in place (each in the input's look, not its own).
            raw_text_input: paper
                .inputs
                .iter()
                .map(|input| PaperInput {
                    raw_text: input.display_text(),
                    children: Vec::new(),
                    ..input.clone()
                })
                .collect(),
            raw_stamp_input: paper.stamps.clone(),
        }
    }

    fn data(_: (&Paper, Option<&DmName>), _: Entity, _: &()) {}
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
}

/// tg `paper/ui_status`: readable while held or near (the source has `AlwaysState`, so the
/// distance check is here), also inside a clipboard or folder that is; blind readers lose the
/// window.
fn window_status(
    mut windows: Query<&mut UiWindow>,
    users: Query<(&Inventory, Option<&TilePos>)>,
    positions: Query<&TilePos>,
    stored: Query<&crate::storage::StoredIn>,
) {
    for mut window in &mut windows {
        if ![
            PaperSheetUi::NAME,
            MarkdownViewerUi::NAME,
            super::PhotoUi::NAME,
        ]
        .contains(&window.interface)
        {
            continue;
        }
        let source = match stored.get(window.source) {
            Ok(holder) => holder.0,
            Err(_) => window.source,
        };
        let status = match users.get(window.actor) {
            Ok((inventory, _)) if inventory.holds(source) => UiStatus::Interactive,
            Ok((_, Some(at))) => positions
                .get(source)
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

pub(super) use crate::tools::client_of;

/// The job title of a player's mob (tg `mind.assigned_role.title`).
pub(super) fn job_title(world: &World, mob: Entity) -> Option<String> {
    if let Some(title) = crate::noncrew_jobs::title_of(world, mob) {
        return Some(title.to_owned());
    }
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

/// A delayed step of a machine (tg `addtimer` / the sleeps of `do_copy_loop`).
#[derive(Component, Clone, Debug)]
pub(super) struct ServiceTimer {
    at: f64,
    job: TimerJob,
}

#[derive(Clone, Debug)]
pub(super) enum TimerJob {
    /// The photocopier's next copy.
    Copy { copier: Entity },
    /// A received fax comes out of `fax`.
    FaxVend { fax: Entity, item: Entity },
    /// The binder's book is done.
    Bind { binder: Entity, paper: Entity },
    /// The library console's printed book.
    PrintBook { console: Entity, id: u32 },
    /// The library console's printed bible.
    PrintBible { console: Entity },
    /// A filing cabinet's drawer closes.
    CabinetClose { cabinet: Entity },
    /// A camera's flash is charged again.
    CameraReady { camera: Entity },
    /// A camera's flash light goes out.
    FlashEnd { light: Entity },
}

pub(super) use crate::tools::now;

/// `addtimer(job, secs)`.
pub(super) fn after(world: &mut World, secs: f64, job: TimerJob) {
    let at = now(world) + secs;
    world.spawn((Name::new("service timer"), ServiceTimer { at, job }));
}

fn run_timers(world: &mut World) {
    let t = now(world);
    let mut timers = world.query::<(Entity, &ServiceTimer)>();
    let due: Vec<(Entity, TimerJob)> = timers
        .iter(world)
        .filter(|(_, timer)| timer.at <= t)
        .map(|(e, timer)| (e, timer.job.clone()))
        .collect();
    for (e, job) in due {
        world.despawn(e);
        match job {
            TimerJob::Copy { copier } => super::office::next_copy(world, copier),
            TimerJob::FaxVend { fax, item } => super::office::vend(world, fax, item),
            TimerJob::Bind { binder, paper } => super::archive::bind(world, binder, paper),
            TimerJob::PrintBook { console, id } => super::archive::print(world, console, id),
            TimerJob::PrintBible { console } => super::library_console::print_bible(world, console),
            TimerJob::CabinetClose { cabinet } => super::stationery::close_cabinet(world, cabinet),
            TimerJob::CameraReady { camera } => super::photo::camera_ready(world, camera),
            TimerJob::FlashEnd { light } => super::photo::flash_end(world, light),
        }
    }
}

/// Renames an item (its `DmName`, and its `Name` when it has one or no `DmName`).
pub(super) fn set_name(world: &mut World, e: Entity, name: &str) {
    if let Some(mut dm) = world.get_mut::<DmName>(e) {
        name.clone_into(&mut dm.name);
    }
    if world.get::<Name>(e).is_some() || world.get::<DmName>(e).is_none() {
        world.entity_mut(e).insert(Name::new(name.to_owned()));
    }
}
