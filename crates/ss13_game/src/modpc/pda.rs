//! The PDA (server) after tg `/obj/item/modular_computer/pda` and `/obj/item/modular_computer`.
//!
//! Every `/obj/item/modular_computer/pda` item gets a [`ModularComputer`] and the messenger
//! program's state ([`Messenger`]). Using it in hand (`attack_self` -> `interact`) turns it on
//! and opens the `NtosMain` window; an unlocked uplink hidden in it opens instead. The window
//! follows `default_state` for an item: interactive while the user carries it (or stands next
//! to it on the floor) and is conscious, closed when the computer is off.
//!
//! The ID slot (`stored_id`): an ID card in hand used on the PDA goes in (swapping out the old
//! one), alt-click or "Вытащить ID" takes it out. tg's `GetID()`/`GetAccess()` of a computer
//! return the stored card: every ID lookup goes through `crate::inventory::get_id`, which
//! resolves a PDA to the card entity in its slot, so a change to the card (the ID console)
//! reaches the PDA at once. The PDA's name follows its imprint (`imprint_id` ->
//! `UpdateDisplay`). The flashlight lights the holder (tg's overlay light moves with whoever
//! carries it) or the PDA on the floor.
// shortcut: infinite battery (`internal_cell`, `handle_power`), NTNet always at good signal
// (no relays), no inserted pen, disks, pAI, paper or chunky fingers; the light is
// omnidirectional (tg: a directional overlay light) and gives way to any other light on its
// holder (one `Optics` per entity).

use bevy::ecs::message::MessageCursor;
use bevy::ecs::query::QueryItem;
use bevy::ecs::system::{SystemParam, SystemParamItem};
use bevy::prelude::*;
use ss13_atmos::Atmos;
use ss13_atmos::gas::CELL_VOLUME;
use ss13_mapload::construct::DmObj;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmName};
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};
use ss13_shared::{Dir, LightSource, Optics, TilePos};
use ss13_tgui::UiStatus;
use ss13_tgui::server::{
    AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
    distance_status,
};

use super::programs::messenger::{self, Messenger, Peer, ViewRef};
use super::programs::{ProgramKind, pda_programs};
use super::*;
use crate::health::Stat;
use crate::inventory::{CarriedBy, Inventory};
use crate::jobs::Jobs;
use crate::outfit::IdRegistration;
use crate::server::PlayerMob;
use crate::storage::StoredIn;

/// `/obj/item/modular_computer/pda` and subtypes.
pub const PDA_ROOT: &str = "/obj/item/modular_computer/pda";
/// `max_idle_programs` of a PDA.
const MAX_IDLE_PROGRAMS: usize = 2;
/// `comp_light_luminosity` of a PDA, `light_power` of a modular computer, `COLOR_WHITE`.
const LIGHT_RANGE: f32 = 2.3;
const LIGHT_POWER: f32 = 1.2;
/// `NTNET_GOOD_SIGNAL`.
const NTNET_GOOD_SIGNAL: u8 = 2;
/// `NtosMain`'s title.
const MAIN_TITLE: &str = "NtOS Главное Меню";

/// tg `/obj/item/modular_computer` state, on the item.
#[derive(Component, Clone, Debug)]
pub struct ModularComputer {
    /// The item's type path (`starting_programs` by type).
    pub path: String,
    /// `enabled`: powered on.
    pub enabled: bool,
    /// `stored_files`, in install order.
    pub programs: Vec<ProgramKind>,
    pub active: Option<ProgramKind>,
    /// `idle_threads`: programs in the background.
    pub idle: Vec<ProgramKind>,
    pub stored_id: Option<Entity>,
    /// `alt_stored_id`: the second ID slot (`TRAIT_MODPC_TWO_ID_SLOTS`, the ID console).
    pub alt_stored_id: Option<Entity>,
    pub saved_identification: Option<String>,
    pub saved_job: Option<String>,
    pub has_light: bool,
    pub light_on: bool,
    /// The notepad program's `written_note`.
    pub note: String,
    /// `initial(name)`, restored when the imprint is reset.
    pub initial_name: String,
    /// `obj_flags & EMAGGED`.
    pub emagged: bool,
}

impl ModularComputer {
    /// A fresh PDA of type `path` (`Initialize` + `install_default_programs`).
    pub fn pda(path: &str, name: &str) -> Self {
        Self {
            path: path.to_owned(),
            enabled: false,
            programs: pda_programs(path),
            active: None,
            idle: Vec::new(),
            stored_id: None,
            alt_stored_id: None,
            saved_identification: None,
            saved_job: None,
            has_light: true,
            light_on: false,
            note: NOTEPAD_DEFAULT.to_owned(),
            initial_name: name.to_owned(),
            emagged: false,
        }
    }

    /// A stationary console of type `path` (`/obj/machinery/modular_computer`) with `programs`.
    pub fn console(path: &str, name: &str, programs: Vec<ProgramKind>) -> Self {
        Self {
            programs,
            has_light: false,
            ..Self::pda(path, name)
        }
    }

    /// A PDA (`/obj/item/modular_computer/pda`), not a console.
    pub fn is_pda(&self) -> bool {
        is_under(&self.path, PDA_ROOT)
    }

    /// `open_program`: the program comes to the front; the one in front goes to the background.
    /// `false` when it is not installed or the background is full.
    pub fn open_program(&mut self, program: ProgramKind) -> bool {
        if !self.programs.contains(&program) || self.active == Some(program) {
            return false;
        }
        let resumed = self.idle.contains(&program);
        if !resumed && self.idle.len() > MAX_IDLE_PROGRAMS {
            return false;
        }
        self.idle.retain(|&p| p != program);
        if let Some(front) = self.active.replace(program)
            && !self.idle.contains(&front)
        {
            self.idle.push(front);
        }
        true
    }

    /// `kill_program`: closed, in front or in the background.
    pub fn kill_program(&mut self, program: ProgramKind) {
        self.idle.retain(|&p| p != program);
        if self.active == Some(program) {
            self.active = None;
        }
    }

    /// `background_program` of the program in front (`PC_minimize`).
    pub fn minimize(&mut self) {
        if let Some(front) = self.active.take()
            && !self.idle.contains(&front)
        {
            self.idle.push(front);
        }
    }

    /// `shutdown_computer`.
    pub fn shutdown(&mut self) {
        self.active = None;
        self.idle.clear();
        self.enabled = false;
    }

    /// `UpdateDisplay`: `КПК «name» (job)`, or the type's name without an imprint.
    pub fn display_name(&self) -> String {
        match (&self.saved_identification, &self.saved_job) {
            (None, None) => self.initial_name.clone(),
            (name, job) => format!(
                "КПК «{}» ({})",
                name.as_deref().unwrap_or_default(),
                job.as_deref().unwrap_or_default()
            ),
        }
    }
}

/// The notepad program's default `written_note`.
const NOTEPAD_DEFAULT: &str = "Congratulations on your station upgrading to the new NtOS and \
    Thinktronic based collaboration effort, bringing you the best in electronics and software \
    since 2467!\nTo help with navigation, we have provided the following definitions:\nFore - \
    Toward front of ship\nAft - Toward back of ship\nPort - Left side of ship\nStarboard - Right \
    side of ship\nQuarter - Either sides of Aft\nBow - Either sides of Fore";

/// On a PDA whose light this module put on `.0` (its holder or itself), with that light.
#[derive(Component, Clone, Copy, Debug)]
struct LitOn(Entity, LightSource);

pub struct ModpcServerPlugin;

impl Plugin for ModpcServerPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // A PDA turns on and opens its NtOS window.
        app.add_click_handler(ClickPhase::AttackSelf, 2600, |w, c| {
            crate::modpc::pda::attack_self(w, c.client, c.mob, c.target)
        });
        // An ID card in the active hand goes into the PDA in the other.
        app.add_click_handler(ClickPhase::OtherHand, 1200, |w, c| {
            c.held.is_some_and(|h| {
                crate::modpc::pda::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        // tg `attack_hand` of a stationary modular computer (the ID console).
        app.add_click_handler(ClickPhase::EmptyHand, 2400, |w, c| {
            crate::modpc::pda::console_interact(w, c.client, c.mob, c.target)
        });
        // tg `item_interaction_secondary` of the ID console: an ID into the second slot.
        app.add_click_handler(ClickPhase::HeldOnTarget, 4100, |w, c| {
            c.modifiers.right
                && c.held.is_some_and(|h| {
                    crate::modpc::pda::item_interaction_secondary(w, c.client, c.mob, h, c.target)
                })
        });
        // tg `item_interaction` of a PDA with an ID card.
        app.add_click_handler(ClickPhase::HeldOnTarget, 5200, |w, c| {
            c.held.is_some_and(|h| {
                crate::modpc::pda::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        // tg `item_interaction` of an emag on the PDA in the other hand.
        app.add_click_handler(ClickPhase::OtherHand, 1150, |w, c| {
            c.held.is_some_and(|h| {
                w.get::<ModularComputer>(c.target).is_some()
                    && crate::emag::interact_with_atom(
                        w,
                        crate::tools::Actor {
                            client: c.client,
                            mob: c.mob,
                            item: Some(h),
                        },
                        c.target,
                    )
            })
        });
        super::programs::minesweeper::build(app);
        super::programs::downloader::build(app);
        super::programs::chat::build(app);
        super::programs::station::build(app);
        app.register_ui::<NtosUi>()
            .add_observer(attach)
            .add_observer(attach_console)
            .add_observer(card_left)
            .add_observer(unlight)
            .add_systems(
                Update,
                (
                    ntos_status.in_set(UiSystems::StatusHook),
                    ntos_acts.in_set(UiSystems::Handle),
                    sync_lights.after(UiSystems::Handle),
                ),
            );
    }
}

/// `/obj/item/modular_computer/emag_act` (and BandaStation's PDA override, which installs the
/// minesweeper). True when it took.
// shortcut: no device themes (`PDA_THEME_SYNDICATE`) and no syndicate programs to unlock.
pub fn emag_act(world: &mut World, client: Entity, pc: Entity) -> bool {
    let Some(computer) = world.get::<ModularComputer>(pc) else {
        return false;
    };
    if !computer.enabled {
        chat(world, Some(client), Span::Warning, "сначала включите!");
        return false;
    }
    if computer.emagged {
        chat(world, Some(client), Span::Warning, "уже емагнут!");
        chat(
            world,
            Some(client),
            Span::Notice,
            "Вы проводите картой по устройству. Окно консоли включается, но оно быстро \
             закрывается после того, как в него записывается всего несколько строк.",
        );
        return false;
    }
    let is_pda = computer.is_pda();
    world
        .get_mut::<ModularComputer>(pc)
        .expect("checked above")
        .emagged = true;
    chat(world, Some(client), Span::Notice, "syndieOS загружен");
    chat(
        world,
        Some(client),
        Span::Notice,
        "Вы проводите картой по устройству. Окно консоли на мгновение заполняет белый текст, \
         быстро прокручиваемый по экрану.",
    );
    if is_pda {
        super::programs::minesweeper::install(world, pc);
    }
    true
}

/// `Initialize` of a PDA item, once its components are all in.
fn attach(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let entity = add.entity;
    if !items.get(entity).is_ok_and(|i| is_under(&i.path, PDA_ROOT)) {
        return;
    }
    commands.queue(move |world: &mut World| {
        let Ok(e) = world.get_entity(entity) else {
            return;
        };
        if e.contains::<ModularComputer>() {
            return;
        }
        let path = e
            .get::<DmItem>()
            .map(|i| i.path.clone())
            .unwrap_or_default();
        let name = e
            .get::<DmName>()
            .map(|n| n.name.clone())
            .unwrap_or_default();
        world.entity_mut(entity).insert((
            ModularComputer::pda(&path, &name),
            Messenger::default(),
            // The window follows `ntos_status`, not the map distance: the PDA is carried.
            AlwaysState,
        ));
    });
}

/// `Initialize` of a stationary console whose programs this port has
/// (`super::programs::console_programs`).
fn attach_console(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let entity = add.entity;
    let Some(programs) = objs
        .get(entity)
        .ok()
        .and_then(|o| super::programs::console_programs(&o.path).map(|p| (o.path.clone(), p)))
    else {
        return;
    };
    commands.queue(move |world: &mut World| {
        let Ok(e) = world.get_entity(entity) else {
            return;
        };
        if e.contains::<ModularComputer>() {
            return;
        }
        let name = e
            .get::<DmName>()
            .map(|n| crate::vending::plain_name(&n.name).to_owned())
            .unwrap_or_default();
        let (path, programs) = programs;
        world.entity_mut(entity).insert((
            ModularComputer::console(&path, &name, programs),
            super::programs::card_mod::CardModState::default(),
        ));
    });
}

/// tg `attack_hand` of a stationary console (`interact` of its computer): a powered console
/// turns on and opens its window. `false` for anything else.
pub fn console_interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let is_console = world
        .get::<ModularComputer>(target)
        .is_some_and(|c| !c.is_pda());
    if !is_console {
        return false;
    }
    if world.get::<crate::power::Unpowered>(target).is_some() {
        return true;
    }
    if let Some(mut computer) = world.get_mut::<ModularComputer>(target) {
        computer.enabled = true;
    }
    open_window(world, client, mob, target);
    true
}

/// The mob carrying `item` in a hand or slot (tg `computer.loc` when it is a mob).
pub fn holder_mob(world: &World, item: Entity) -> Option<Entity> {
    world.get::<CarriedBy>(item).map(|c| c.0)
}

/// The tile `item` is on, through whoever or whatever holds it (tg `get_turf`).
pub fn turf_of(world: &World, item: Entity) -> Option<TilePos> {
    let mut at = item;
    for _ in 0..8 {
        if let Some(pos) = world.get::<TilePos>(at) {
            return Some(*pos);
        }
        at = world
            .get::<CarriedBy>(at)
            .map(|c| c.0)
            .or_else(|| world.get::<StoredIn>(at).map(|s| s.0))?;
    }
    None
}

fn chat(world: &mut World, client: Option<Entity>, span: Span, text: impl Into<String>) {
    if let Some(client) = client {
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage::simple(ChatChannel::System, span, text),
        });
    }
}

fn client_of(world: &World, mob: Entity) -> Option<Entity> {
    world.get::<PlayerMob>(mob).map(|p| p.client)
}

/// tg `attack_self` of a held PDA: an unlocked uplink opens (it "blocks opening the PDA menu"),
/// else `interact`: a computer that is off turns on, and its window opens. `false` for items
/// that are not computers.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(computer) = world.get::<ModularComputer>(item) else {
        return false;
    };
    if world
        .get::<crate::antag::uplink::Uplink>(item)
        .is_some_and(|u| !u.locked)
    {
        crate::antag::uplink::open_uplink(world, client, mob, item);
        return true;
    }
    if !computer.enabled {
        let name = computer.display_name();
        world
            .get_mut::<ModularComputer>(item)
            .expect("checked above")
            .enabled = true;
        chat(
            world,
            Some(client),
            Span::Notice,
            format!("Вы нажимаете кнопку питания, включая {name}."),
        );
    }
    open_window(world, client, mob, item);
    true
}

fn open_window(world: &mut World, client: Entity, mob: Entity, item: Entity) {
    let title = world
        .get::<ModularComputer>(item)
        .map(ModularComputer::display_name)
        .unwrap_or_default();
    world.write_message(OpenUi::new::<NtosUi>(client, mob, item, title));
}

/// tg `item_interaction` of a computer with `held`: an ID card goes into the card slot.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let is_card = world.get::<DmItem>(held).is_some_and(DmItem::is_id_card);
    if !is_card || world.get::<ModularComputer>(target).is_none() {
        return false;
    }
    insert_id(world, client, mob, held, target, false);
    true
}

/// tg `item_interaction_secondary` of a computer with two ID slots (the ID console): a right
/// click with an ID card puts it into the secondary slot.
pub fn item_interaction_secondary(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let is_card = world.get::<DmItem>(held).is_some_and(DmItem::is_id_card);
    let two_slots = world
        .get::<ModularComputer>(target)
        .is_some_and(|c| c.programs.contains(&ProgramKind::CardMod));
    if !is_card || !two_slots {
        return false;
    }
    insert_id(world, client, mob, held, target, true);
    true
}

/// tg `insert_id` (`alt`: `insert_secondary_id`): from `mob`'s hands into the slot; a card
/// already there goes to the hands.
pub fn insert_id(
    world: &mut World,
    client: Entity,
    mob: Entity,
    card: Entity,
    pda: Entity,
    alt: bool,
) {
    let in_hands = world
        .get::<Inventory>(mob)
        .is_some_and(|inv| inv.hands.contains(&Some(card)));
    if !in_hands || world.get::<ModularComputer>(card).is_some() {
        return;
    }
    // Out of the hand first (`transferItemToLoc`), so the old card can take that hand.
    world.entity_mut(card).remove::<CarriedBy>();
    remove_id(world, pda, Some(mob), true, alt);
    crate::storage::put_inside(world, card, pda);
    let mut computer = world
        .get_mut::<ModularComputer>(pda)
        .expect("checked by the caller");
    if alt {
        computer.alt_stored_id = Some(card);
    } else {
        computer.stored_id = Some(card);
    }
    let name = world
        .get::<DmName>(card)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    chat(
        world,
        Some(client),
        Span::Notice,
        if alt {
            format!("Вы вставляете {name} в слот дополнительной карты.")
        } else {
            format!("Вы вставляете {name} в слот карты.")
        },
    );
}

/// tg `remove_id`: into `user`'s hands if it carries the computer or stands next to it, else
/// onto the computer's tile. `false` with an empty slot.
pub fn remove_id(
    world: &mut World,
    pda: Entity,
    user: Option<Entity>,
    silent: bool,
    alt: bool,
) -> bool {
    let Some(card) = world
        .get::<ModularComputer>(pda)
        .and_then(|c| if alt { c.alt_stored_id } else { c.stored_id })
    else {
        return false;
    };
    let at = turf_of(world, pda);
    let near = user.filter(|&u| {
        holder_mob(world, pda) == Some(u)
            || matches!((at, world.get::<TilePos>(u)), (Some(a), Some(b)) if a.chebyshev(*b).is_some_and(|d| d <= 1))
    });
    let hand = near.and_then(|u| {
        let inv = world.get::<Inventory>(u)?;
        [inv.active, 1 - inv.active]
            .into_iter()
            .find(|&h| inv.hands[h].is_none())
            .map(|h| (u, h))
    });
    // `card_left` empties the slot.
    match (hand, at) {
        (Some((mob, h)), _) => {
            world
                .entity_mut(card)
                .remove::<StoredIn>()
                .insert(CarriedBy(mob));
            world
                .get_mut::<Inventory>(mob)
                .expect("checked above")
                .hands[h] = Some(card);
        }
        (None, Some(pos)) => crate::storage::take_out(world, card, pos),
        // Nowhere to put it: it stays in.
        (None, None) => return false,
    }
    if !silent && let Some(user) = user {
        let name = world
            .get::<DmName>(card)
            .map(|n| n.name.clone())
            .unwrap_or_default();
        chat(
            world,
            client_of(world, user),
            Span::Notice,
            if alt {
                format!("Вы вытаскиваете {name} из слота дополнительной карты.")
            } else {
                format!("Вы вытаскиваете {name} со слота карты.")
            },
        );
    }
    true
}

/// tg `click_alt`: the ID comes out.
pub fn alt_click(world: &mut World, mob: Entity, item: Entity) -> bool {
    if world.get::<ModularComputer>(item).is_none() {
        return false;
    }
    remove_id(world, item, Some(mob), false, false);
    true
}

/// tg `Exited`: a stored card left the computer by any path.
fn card_left(
    remove: On<Remove<StoredIn>>,
    stored_in: Query<&StoredIn>,
    mut computers: Query<&mut ModularComputer>,
) {
    let Ok(&StoredIn(container)) = stored_in.get(remove.entity) else {
        return;
    };
    if let Ok(mut computer) = computers.get_mut(container) {
        if computer.stored_id == Some(remove.entity) {
            computer.stored_id = None;
        }
        if computer.alt_stored_id == Some(remove.entity) {
            computer.alt_stored_id = None;
        }
    }
}

/// tg `imprint_id(name, job)`: the given name and job, else the stored card's, else the old
/// imprint; the name follows (`UpdateDisplay`).
pub fn imprint_id(world: &mut World, pda: Entity, name: Option<&str>, job: Option<&str>) {
    let registration = world
        .get::<ModularComputer>(pda)
        .and_then(|c| c.stored_id)
        .and_then(|card| world.get::<IdRegistration>(card).cloned());
    let Some(mut computer) = world.get_mut::<ModularComputer>(pda) else {
        return;
    };
    computer.saved_identification = name
        .map(str::to_owned)
        .or_else(|| registration.as_ref().map(|r| r.registered_name.clone()))
        .or(computer.saved_identification.take());
    computer.saved_job = job
        .map(str::to_owned)
        .or_else(|| registration.as_ref().map(|r| r.assignment.clone()))
        .or(computer.saved_job.take());
    let display = computer.display_name();
    match world.get_mut::<DmName>(pda) {
        Some(mut dm) => dm.name = display,
        None => {
            world.entity_mut(pda).insert(DmName {
                name: display,
                desc: String::new(),
            });
        }
    }
}

/// tg `/datum/outfit/job/post_equip` for the PDA: the first computer in `mob`'s slots is
/// imprinted with its name and job (BandaStation shows the assignment, like the ID card).
// shortcut: no `job_tone` ringtone and no PDA preferences (ringtone, theme) yet.
pub fn post_equip(world: &mut World, mob: Entity, name: &str, job: &str) {
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return;
    };
    let pda = inventory
        .equipped()
        .chain(inventory.hands.iter().flatten().copied())
        .find(|&e| {
            world
                .get::<DmItem>(e)
                .is_some_and(|i| is_under(&i.path, PDA_ROOT))
        });
    if let Some(pda) = pda {
        if world.get::<ModularComputer>(pda).is_none() {
            let path = world
                .get::<DmItem>(pda)
                .map(|i| i.path.clone())
                .unwrap_or_default();
            let initial = world
                .get::<DmName>(pda)
                .map(|n| n.name.clone())
                .unwrap_or_default();
            world.entity_mut(pda).insert((
                ModularComputer::pda(&path, &initial),
                Messenger::default(),
                AlwaysState,
            ));
        }
        imprint_id(world, pda, Some(name), Some(job));
    }
}

/// tg `default_state` for a computer: the user carries it or stands next to it, it is on, the
/// user is conscious.
fn ntos_status(
    mut windows: Query<&mut UiWindow>,
    computers: Query<(&ModularComputer, Option<&TilePos>)>,
    users: Query<(&Inventory, Option<&Stat>, Option<&TilePos>)>,
    unpowered: Query<(), With<crate::power::Unpowered>>,
) {
    use ss13_tgui::UiInterface as _;
    for mut window in &mut windows {
        let ntos = window.interface == NtosUi::NAME;
        // The window of a launcher program on a computer (`super::programs::apps`, `station`).
        let launcher = if window.interface == crate::newscaster::NewscasterUi::NAME {
            Some(ProgramKind::Newscaster)
        } else {
            super::programs::station::launcher_of(window.interface)
        };
        if !ntos && launcher.is_none() {
            continue;
        }
        let Ok((computer, at)) = computers.get(window.source) else {
            if ntos {
                window.cap_status(UiStatus::Close);
            }
            continue;
        };
        let Ok((inventory, stat, user_at)) = users.get(window.actor) else {
            window.cap_status(UiStatus::Close);
            continue;
        };
        let app_gone = launcher.is_some_and(|p| !computer.programs.contains(&p));
        if !computer.enabled || app_gone || unpowered.contains(window.source) {
            window.cap_status(UiStatus::Close);
            continue;
        }
        if !inventory.holds(window.source) {
            let reach = match (at, user_at) {
                (Some(&at), Some(&user)) => distance_status(user, at),
                _ => UiStatus::Close,
            };
            window.cap_status(reach);
        }
        if stat.is_some_and(|s| *s != Stat::Conscious) {
            window.cap_status(UiStatus::Disabled);
        }
    }
}

/// What `ui_data` reads besides the computer.
#[derive(SystemParam)]
pub struct NtosParams<'w, 's> {
    peers: Query<'w, 's, (Entity, &'static ModularComputer, &'static Messenger)>,
    records: Option<Res<'w, crate::records::manifest::Manifest>>,
    positions: Query<'w, 's, &'static TilePos>,
    registrations: Query<'w, 's, &'static IdRegistration>,
    cards: Query<'w, 's, super::programs::card_mod::CardParts<'static>>,
    card_mods: Query<'w, 's, &'static super::programs::card_mod::CardModState>,
    inventories: Query<'w, 's, &'static Inventory>,
    items: Query<'w, 's, &'static DmItem>,
    jobs: Option<Res<'w, Jobs>>,
    atmos: Option<Res<'w, Atmos>>,
    alarms: Option<Res<'w, crate::silicon::alarms::StationAlarms>>,
    app_states: Query<'w, 's, &'static super::programs::apps::AppState>,
    minesweepers: Query<'w, 's, &'static super::programs::minesweeper::Minesweeper>,
    minesweeper_board: Option<Res<'w, super::programs::minesweeper::MinesweeperLeaderboard>>,
    downloads: Query<'w, 's, &'static super::programs::downloader::Download>,
    id_cards: Query<'w, 's, &'static crate::inventory::IdCard>,
    computers: Query<'w, 's, &'static ModularComputer>,
    chat_clients: Query<'w, 's, &'static super::programs::chat::ChatClient>,
    chat_channels: Option<Res<'w, super::programs::chat::ChatChannels>>,
    station_apps: Query<'w, 's, &'static super::programs::station::StationApps>,
    ticker: Option<Res<'w, crate::round::Ticker>>,
    time: Res<'w, Time>,
}

impl NtosParams<'_, '_> {
    fn round_secs(&self) -> u64 {
        let now = self.time.elapsed_secs_f64();
        self.ticker
            .as_ref()
            .map_or(0.0, |t| t.round_duration(now))
            .max(0.0) as u64
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NtosAct {
    Exit,
    Shutdown,
    Minimize,
    Kill(ProgramKind),
    Run(ProgramKind),
    ToggleLight,
    EjectId,
    ImprintId,
    RingSet(String),
    ToggleAlerts,
    ToggleSending,
    View(Option<ViewRef>),
    CloseChat(u32),
    ClearChats(Option<u32>),
    ChangeSort,
    Send(ViewRef, String),
    ClearUnreads(u32),
    UpdateNote(String),
    Card(super::programs::card_mod::CardAct),
    App(super::programs::apps::AppAct),
    Minesweeper(super::programs::minesweeper::MinesweeperAct),
    Downloader(super::programs::downloader::DownloaderAct),
    Chat(super::programs::chat::ChatAct),
    Station(super::programs::station::StationAct),
}

fn program_param(params: &UiParams) -> Result<ProgramKind, UiReject> {
    params
        .get("name")
        .and_then(ProgramKind::from_filename)
        .ok_or_else(|| UiReject::BadParams("name: expected an installed program".into()))
}

fn ref_param(params: &UiParams) -> Result<Option<ViewRef>, UiReject> {
    match params.get("ref").filter(|r| !r.is_empty()) {
        None => Ok(None),
        Some(r) => ViewRef::parse(r)
            .map(Some)
            .ok_or_else(|| UiReject::BadParams("ref: expected a chat or messenger".into())),
    }
}

fn chat_param(params: &UiParams) -> Result<u32, UiReject> {
    match ref_param(params)? {
        Some(ViewRef::Chat(id)) => Ok(id),
        _ => Err(UiReject::BadParams("ref: expected a chat".into())),
    }
}

impl ServerUi for NtosUi {
    type Source = (
        Entity,
        &'static ModularComputer,
        Option<&'static Messenger>,
        Option<&'static CarriedBy>,
        Option<&'static TilePos>,
    );
    type UserParam = NtosParams<'static, 'static>;
    type Act = NtosAct;

    fn static_data(_: QueryItem<'_, '_, Self::Source>) {}

    fn data(
        (entity, computer, messenger, carried, at): QueryItem<'_, '_, Self::Source>,
        user: Entity,
        params: &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> NtosData {
        let secs = params.round_secs();
        let header = NtosHeader {
            title: computer
                .active
                .map_or(MAIN_TITLE, ProgramKind::filedesc)
                .to_owned(),
            station_time: format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60),
            battery_percent: Some(100),
            ntnet: NTNET_GOOD_SIGNAL,
            program_headers: computer
                .idle
                .iter()
                .filter_map(|&p| match p {
                    ProgramKind::AlarmMonitor
                        if super::programs::apps::has_alert(params.alarms.as_deref()) =>
                    {
                        Some("alarm_red.gif")
                    }
                    ProgramKind::Downloader => params.downloads.get(entity).ok().and_then(|d| {
                        if d.file.is_some() {
                            Some("downloader_running.gif")
                        } else {
                            d.finished.then_some("downloader_finished.gif")
                        }
                    }),
                    ProgramKind::ChatClient => super::programs::chat::header(
                        params.chat_clients.get(entity).ok(),
                        params.chat_channels.as_deref(),
                    ),
                    p => p.ui_header(),
                })
                .map(str::to_owned)
                .collect(),
            show_exit: computer.active.is_some(),
        };
        let pos = at
            .copied()
            .or_else(|| carried.and_then(|c| params.positions.get(c.0).ok().copied()));
        let screen = match computer.active {
            None => NtosScreen::Main(main_data(
                computer,
                computer
                    .stored_id
                    .and_then(|c| params.registrations.get(c).ok()),
            )),
            Some(ProgramKind::Messenger) => {
                let peers = params.peers.iter().filter_map(|(e, c, m)| {
                    Some(Peer {
                        entity: e,
                        name: c.saved_identification.as_deref()?,
                        job: c.saved_job.as_deref()?,
                        invisible: m.invisible,
                    })
                });
                let fallback = Messenger::default();
                NtosScreen::Messenger(messenger::ui_data(
                    entity,
                    computer,
                    messenger.unwrap_or(&fallback),
                    peers,
                ))
            }
            Some(ProgramKind::Notepad) => NtosScreen::Notepad(NotepadData {
                note: computer.note.clone(),
            }),
            Some(ProgramKind::CrewManifest) => NtosScreen::CrewManifest(manifest(params)),
            Some(ProgramKind::Atmosscan) => {
                NtosScreen::GasAnalyzer(gas_mixes(params.atmos.as_deref(), pos))
            }
            Some(ProgramKind::CardMod) => {
                use super::programs::card_mod::{card_ui, ui_data};
                let card =
                    |e: Option<Entity>| e.and_then(|e| params.cards.get(e).ok()).map(card_ui);
                let holding = params
                    .inventories
                    .get(user)
                    .ok()
                    .and_then(Inventory::active_item)
                    .and_then(|i| params.items.get(i).ok())
                    .is_some_and(DmItem::is_id_card);
                NtosScreen::Card(Box::new(ui_data(
                    params.card_mods.get(entity).ok(),
                    card(computer.stored_id),
                    card(computer.alt_stored_id),
                    holding,
                    params.jobs.as_deref(),
                )))
            }
            Some(ProgramKind::AlarmMonitor) => {
                use super::programs::apps::alarm_listener;
                NtosScreen::Alarms(match params.alarms.as_deref() {
                    Some(alarms) => crate::silicon::alarms::alert_data(alarms, &alarm_listener()),
                    None => Default::default(),
                })
            }
            Some(ProgramKind::SecRecords) => NtosScreen::SecRecords(
                super::programs::apps::security_rows(params.records.as_deref()),
            ),
            Some(ProgramKind::MedRecords) => NtosScreen::MedRecords(
                super::programs::apps::medical_rows(params.records.as_deref()),
            ),
            Some(ProgramKind::Signaler) => NtosScreen::Signaler(
                super::programs::apps::signaler_data(params.app_states.get(entity).ok()),
            ),
            Some(ProgramKind::StatusDisplay) => NtosScreen::Status(
                super::programs::apps::status_data(params.app_states.get(entity).ok()),
            ),
            Some(ProgramKind::Minesweeper) => {
                NtosScreen::Minesweeper(Box::new(super::programs::minesweeper::ui_data(
                    params.minesweepers.get(entity).ok(),
                    params.minesweeper_board.as_deref(),
                )))
            }
            Some(ProgramKind::Downloader) => {
                let access = computer
                    .stored_id
                    .and_then(|c| params.id_cards.get(c).ok())
                    .map(|c| c.access.as_slice());
                NtosScreen::Downloader(Box::new(super::programs::downloader::ui_data(
                    computer,
                    params.downloads.get(entity).ok(),
                    access,
                )))
            }
            Some(ProgramKind::ChatClient) => {
                let can_admin = computer
                    .stored_id
                    .and_then(|c| params.id_cards.get(c).ok())
                    .is_some_and(|c| c.access.iter().any(|a| a == "network"));
                let status = |pc: Entity| {
                    params.computers.get(pc).map_or((false, false), |c| {
                        (
                            c.active == Some(ProgramKind::ChatClient),
                            c.idle.contains(&ProgramKind::ChatClient),
                        )
                    })
                };
                let names = |pc: Entity| {
                    params
                        .chat_clients
                        .get(pc)
                        .map(|c| c.username.clone())
                        .unwrap_or_default()
                };
                NtosScreen::Chat(Box::new(super::programs::chat::ui_data(
                    entity,
                    params.chat_clients.get(entity).ok(),
                    params.chat_channels.as_deref(),
                    can_admin,
                    &status,
                    &names,
                )))
            }
            Some(ProgramKind::Station(_)) => match params
                .station_apps
                .get(entity)
                .ok()
                .and_then(|s| s.view.clone())
            {
                Some(view) => NtosScreen::Station(Box::new(view)),
                // The first tick, before `station::refresh` built it.
                None => NtosScreen::Main(NtosMainData::default()),
            },
            // A launcher never comes to the front.
            Some(ProgramKind::Newscaster) => NtosScreen::Main(main_data(
                computer,
                computer
                    .stored_id
                    .and_then(|c| params.registrations.get(c).ok()),
            )),
        };
        NtosData { header, screen }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<NtosAct, UiReject> {
        Ok(match action {
            PC_EXIT => NtosAct::Exit,
            PC_SHUTDOWN => NtosAct::Shutdown,
            PC_MINIMIZE => NtosAct::Minimize,
            PC_KILLPROGRAM => NtosAct::Kill(program_param(params)?),
            PC_RUNPROGRAM => NtosAct::Run(program_param(params)?),
            PC_TOGGLE_LIGHT => NtosAct::ToggleLight,
            PC_EJECT_DISK if params.get("name") == Some("ID") => NtosAct::EjectId,
            PC_EJECT_DISK => return Err(UiReject::BadParams("name: only ID".into())),
            PC_IMPRINT_ID => NtosAct::ImprintId,
            PDA_RING_SET => NtosAct::RingSet(params.text("ringtone")?),
            PDA_TOGGLE_ALERTS => NtosAct::ToggleAlerts,
            PDA_TOGGLE_SENDING => NtosAct::ToggleSending,
            PDA_VIEW_MESSAGES => NtosAct::View(ref_param(params)?),
            PDA_CLOSE_MESSAGES => NtosAct::CloseChat(chat_param(params)?),
            PDA_CLEAR_MESSAGES => NtosAct::ClearChats(match ref_param(params)? {
                None => None,
                Some(ViewRef::Chat(id)) => Some(id),
                Some(ViewRef::Messenger(_)) => {
                    return Err(UiReject::BadParams("ref: expected a chat".into()));
                }
            }),
            PDA_CHANGE_SORT => NtosAct::ChangeSort,
            PDA_SEND_MESSAGE => {
                let target =
                    ref_param(params)?.ok_or_else(|| UiReject::BadParams("ref: missing".into()))?;
                NtosAct::Send(target, params.text("message")?)
            }
            PDA_CLEAR_UNREADS => NtosAct::ClearUnreads(chat_param(params)?),
            UPDATE_NOTE => NtosAct::UpdateNote(params.text("newnote")?),
            other => {
                if let Some(act) = super::programs::apps::parse(other, params) {
                    return act.map(NtosAct::App);
                }
                if let Some(act) = super::programs::minesweeper::parse(other, params) {
                    return act.map(NtosAct::Minesweeper);
                }
                if let Some(act) = super::programs::downloader::parse(other, params) {
                    return act.map(NtosAct::Downloader);
                }
                if let Some(act) = super::programs::chat::parse(other, params) {
                    return act.map(NtosAct::Chat);
                }
                if let Some(act) = super::programs::station::parse(other, params) {
                    return act.map(NtosAct::Station);
                }
                match super::programs::card_mod::parse(other, |k| params.get(k).map(str::to_owned))
                {
                    Some(Ok(act)) => NtosAct::Card(act),
                    Some(Err(e)) => return Err(UiReject::BadParams(e)),
                    None => return Err(UiReject::UnknownAction),
                }
            }
        })
    }
}

fn main_data(computer: &ModularComputer, card: Option<&IdRegistration>) -> NtosMainData {
    NtosMainData {
        has_light: computer.has_light,
        light_on: computer.light_on,
        login: Login {
            name: computer.saved_identification.clone(),
            job: computer.saved_job.clone(),
        },
        proposed_login: Login {
            name: card.map(|c| c.registered_name.clone()),
            job: card.map(|c| c.assignment.clone()),
        },
        id_inserted: computer.stored_id.is_some(),
        show_imprint: computer.is_pda(),
        programs: computer
            .programs
            .iter()
            .map(|&p| ProgramEntry {
                name: p.filename().to_owned(),
                desc: p.filedesc().to_owned(),
                header_program: p.header_program(),
                running: computer.idle.contains(&p),
                icon: p.icon().to_owned(),
                alert: false,
            })
            .collect(),
    }
}

/// `GLOB.manifest.get_manifest()`: the crew records by department.
fn manifest(params: &NtosParams) -> Vec<ManifestDepartment> {
    match (params.jobs.as_deref(), params.records.as_deref()) {
        (Some(jobs), Some(records)) => records.departments(&jobs.data),
        _ => Vec::new(),
    }
}

/// `gas_mixture_parser(turf.return_air(), "Location Reading")`.
fn gas_mixes(atmos: Option<&Atmos>, pos: Option<TilePos>) -> Vec<GasMix> {
    let Some(air) = pos.and_then(|p| atmos?.air(p)) else {
        return Vec::new();
    };
    vec![GasMix {
        name: "Location Reading".to_owned(),
        gases: ss13_atmos::GASES
            .iter()
            .zip(&air.moles)
            .filter(|&(_, &m)| m > 0.0)
            .map(|(g, &m)| (g.name.to_owned(), m))
            .collect(),
        temperature: air.temperature,
        volume: CELL_VOLUME,
        pressure: air.pressure(),
        total_moles: air.total_moles(),
    }]
}

/// `ui_act`: the computer's own actions, then the program in front's.
fn ntos_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<NtosUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, NtosAct)> = acts
        .read(world.resource::<Messages<UiActed<NtosUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, pda, act) in pending {
        let Some(computer) = world.get::<ModularComputer>(pda).cloned() else {
            continue;
        };
        if !computer.enabled {
            continue;
        }
        if let Err(reason) = apply_act(world, client, actor, pda, &computer, act.clone()) {
            debug!("NtOS act {act:?} on {pda} refused: {reason}");
        }
    }
}

fn apply_act(
    world: &mut World,
    client: Entity,
    actor: Entity,
    pda: Entity,
    computer: &ModularComputer,
    act: NtosAct,
) -> Result<(), &'static str> {
    macro_rules! pc {
        () => {
            world.get_mut::<ModularComputer>(pda).expect("read above")
        };
    }
    let card_mod_running = computer.active == Some(ProgramKind::CardMod)
        || computer.idle.contains(&ProgramKind::CardMod);
    match act {
        NtosAct::Exit => {
            if let Some(active) = computer.active {
                if active == ProgramKind::CardMod {
                    super::programs::card_mod::on_kill(world, pda);
                }
                pc!().kill_program(active);
            }
        }
        NtosAct::Shutdown => {
            if card_mod_running {
                super::programs::card_mod::on_kill(world, pda);
            }
            pc!().shutdown();
        }
        NtosAct::Minimize => pc!().minimize(),
        NtosAct::Kill(program) => {
            if program == ProgramKind::CardMod && card_mod_running {
                super::programs::card_mod::on_kill(world, pda);
            }
            pc!().kill_program(program);
        }
        NtosAct::Card(act) => {
            if computer.active != Some(ProgramKind::CardMod) {
                return Err("ID writer not in front");
            }
            return super::programs::card_mod::apply(world, client, actor, pda, act);
        }
        NtosAct::App(act) => {
            return super::programs::apps::apply(world, client, actor, pda, computer, act);
        }
        NtosAct::Minesweeper(act) => {
            return super::programs::minesweeper::apply(world, client, actor, pda, computer, act);
        }
        NtosAct::Downloader(act) => {
            return super::programs::downloader::apply(world, pda, computer, act);
        }
        NtosAct::Chat(act) => {
            return super::programs::chat::apply(world, actor, pda, computer, act);
        }
        NtosAct::Station(act) => {
            return super::programs::station::apply(world, client, actor, pda, computer, act);
        }
        NtosAct::Run(program) if program.launcher() => {
            if !computer.programs.contains(&program) {
                return Err("not installed");
            }
            match program {
                ProgramKind::Station(p) => {
                    super::programs::station::open_launcher(world, client, actor, pda, p)
                }
                _ => super::programs::apps::open_newscaster(world, client, actor, pda),
            }
        }
        NtosAct::Run(program) => {
            if !pc!().open_program(program) {
                chat(
                    world,
                    Some(client),
                    Span::Danger,
                    format!(
                        "{} показывает ошибку \"Максимальная нагрузка на ЦП. Не удаётся запустить \
                         другую программу.\".",
                        computer.display_name()
                    ),
                );
                return Err("cannot open");
            }
        }
        NtosAct::ToggleLight => {
            if computer.has_light {
                let mut pc = pc!();
                pc.light_on = !pc.light_on;
            }
        }
        NtosAct::EjectId => {
            remove_id(world, pda, Some(actor), false, false);
        }
        NtosAct::ImprintId => imprint_id(world, pda, None, None),
        NtosAct::UpdateNote(note) => {
            if computer.active != Some(ProgramKind::Notepad) {
                return Err("notepad not in front");
            }
            pc!().note = note;
        }
        act => {
            if computer.active != Some(ProgramKind::Messenger) {
                return Err("messenger not in front");
            }
            return messenger_act(world, client, actor, pda, act);
        }
    }
    Ok(())
}

/// `/datum/computer_file/program/messenger/ui_act`.
fn messenger_act(
    world: &mut World,
    client: Entity,
    actor: Entity,
    pda: Entity,
    act: NtosAct,
) -> Result<(), &'static str> {
    // A messenger ref must name a registered messenger, not any entity.
    if let NtosAct::View(Some(ViewRef::Messenger(e))) | NtosAct::Send(ViewRef::Messenger(e), _) =
        act
        && messenger::registered(world, e).is_none()
    {
        return Err("unknown messenger");
    }
    if let NtosAct::RingSet(raw) = &act {
        return set_ringtone(world, client, actor, pda, raw);
    }
    if let NtosAct::Send(target, text) = act {
        return messenger::send_message(world, pda, actor, target, &text)
            .map_err(|_| "message not sent");
    }
    let Some(mut m) = world.get_mut::<Messenger>(pda) else {
        return Err("no messenger");
    };
    match act {
        NtosAct::ToggleAlerts => m.alert_silenced = !m.alert_silenced,
        NtosAct::ToggleSending => m.sending_and_receiving = !m.sending_and_receiving,
        NtosAct::ChangeSort => m.sort_by_job = !m.sort_by_job,
        NtosAct::View(target) => {
            if let Some(ViewRef::Chat(id)) = target
                && m.chat(id).is_none()
            {
                return Err("unknown chat");
            }
            m.view(target);
        }
        NtosAct::CloseChat(id) => m.close(id),
        NtosAct::ClearChats(id) => m.clear(id),
        NtosAct::ClearUnreads(id) => m.clear_unreads(id),
        _ => {}
    }
    Ok(())
}

/// `set_ringtone` with the uplink's `COMSIG_TABLET_CHANGE_ID` (`new_ringtone`): the unlock code
/// opens the hidden uplink instead of becoming the ringtone.
fn set_ringtone(
    world: &mut World,
    client: Entity,
    actor: Entity,
    pda: Entity,
    raw: &str,
) -> Result<(), &'static str> {
    let ringtone = messenger::sanitize_ringtone(raw).ok_or("empty ringtone")?;
    if crate::antag::uplink::unlock_by_ringtone(world, client, actor, pda, &ringtone) {
        return Ok(());
    }
    if let Some(mut m) = world.get_mut::<Messenger>(pda) {
        m.ringtone = ringtone;
    }
    Ok(())
}

/// The light follows the PDA: on its holder while carried, on itself on the floor, nowhere in
/// a container. Runs for computers whose light, holder or position changed this tick.
#[allow(clippy::type_complexity)]
fn sync_lights(
    computers: Query<
        Entity,
        (
            With<ModularComputer>,
            Or<(
                Changed<ModularComputer>,
                Changed<CarriedBy>,
                Added<TilePos>,
                Added<StoredIn>,
            )>,
        ),
    >,
    mut dropped: RemovedComponents<CarriedBy>,
    all: Query<(
        Entity,
        &ModularComputer,
        Option<&CarriedBy>,
        Has<TilePos>,
        Option<&LitOn>,
    )>,
    mut optics: Query<&mut Optics>,
    mut commands: Commands,
) {
    let light = light();
    let mut changed: Vec<Entity> = computers.iter().chain(dropped.read()).collect();
    changed.sort_unstable();
    changed.dedup();
    for (entity, computer, carried, on_map, lit) in
        changed.into_iter().filter_map(|e| all.get(e).ok())
    {
        let target = if !computer.light_on {
            None
        } else if let Some(c) = carried {
            Some(c.0)
        } else {
            on_map.then_some(entity)
        };
        if lit.map(|l| l.0) == target {
            continue;
        }
        if lit.is_some() {
            commands.entity(entity).remove::<LitOn>();
        }
        let Some(target) = target else {
            continue;
        };
        match optics.get_mut(target) {
            Ok(mut o) if o.light.is_none() => o.light = Some(light),
            Ok(_) => continue,
            Err(_) => {
                commands.entity(target).insert(Optics {
                    light: Some(light),
                    ..default()
                });
            }
        }
        commands.entity(entity).insert(LitOn(target, light));
    }
}

/// The PDA's light: `comp_light_luminosity`, `light_power`, white.
fn light() -> LightSource {
    LightSource {
        range: LIGHT_RANGE,
        power: LIGHT_POWER,
        color: [255, 255, 255],
        height: 1.0,
        angle: 360,
        dir: Dir::South,
        offset: [0, 0],
    }
}

/// The light goes out where [`sync_lights`] put it (moved, switched off, PDA gone).
fn unlight(
    remove: On<Remove<LitOn>>,
    lit: Query<&LitOn>,
    mut optics: Query<&mut Optics>,
    mut commands: Commands,
) {
    let Ok(&LitOn(target, light)) = lit.get(remove.entity) else {
        return;
    };
    if let Ok(mut o) = optics.get_mut(target)
        && o.light == Some(light)
    {
        if !o.opaque && o.tile.is_none() {
            commands.entity(target).try_remove::<Optics>();
        } else {
            o.light = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_open_minimize_and_close_like_ntos() {
        let mut pc = ModularComputer::pda("/obj/item/modular_computer/pda/crew/atmos", "pda");
        // The role table (tg `starting_programs`) adds apps on top of the PDA defaults.
        assert!(pc.programs.len() >= 4);
        assert!(pc.open_program(ProgramKind::Messenger));
        assert!(!pc.open_program(ProgramKind::Messenger));
        // Another program pushes the messenger to the background.
        assert!(pc.open_program(ProgramKind::Notepad));
        assert_eq!(pc.idle, [ProgramKind::Messenger]);
        // Resuming swaps them.
        assert!(pc.open_program(ProgramKind::Messenger));
        assert_eq!(pc.active, Some(ProgramKind::Messenger));
        assert_eq!(pc.idle, [ProgramKind::Notepad]);
        pc.minimize();
        assert_eq!(pc.active, None);
        assert_eq!(pc.idle, [ProgramKind::Notepad, ProgramKind::Messenger]);
        // `idle_threads.len > max_idle_programs`: a third background program still fits.
        assert!(pc.open_program(ProgramKind::CrewManifest));
        assert!(pc.open_program(ProgramKind::Atmosscan));
        assert_eq!(pc.idle.len(), 3);
        pc.kill_program(ProgramKind::Atmosscan);
        assert_eq!(pc.active, None);
        pc.kill_program(ProgramKind::Notepad);
        assert_eq!(pc.idle.len(), 2);
        pc.shutdown();
        assert!(!pc.enabled && pc.idle.is_empty());
    }

    #[test]
    fn the_light_follows_the_pda() {
        let mut app = App::new();
        app.add_systems(Update, sync_lights).add_observer(unlight);
        let world = app.world_mut();
        let mob = world.spawn(TilePos::new(1, 1, 1)).id();
        let mut pc = ModularComputer::pda(PDA_ROOT, "pda");
        pc.light_on = true;
        let pda = world.spawn((pc, CarriedBy(mob))).id();
        app.update();
        let lit = |app: &App, e| app.world().get::<Optics>(e).and_then(|o| o.light);
        assert!(lit(&app, mob).is_some(), "the holder carries the light");
        // Dropped: the PDA on the floor shines, the holder does not.
        app.world_mut()
            .entity_mut(pda)
            .remove::<CarriedBy>()
            .insert(TilePos::new(1, 1, 1));
        app.update();
        assert!(app.world().get::<Optics>(mob).is_none());
        assert!(lit(&app, pda).is_some());
        // Switched off.
        app.world_mut()
            .get_mut::<ModularComputer>(pda)
            .unwrap()
            .light_on = false;
        app.update();
        assert!(app.world().get::<Optics>(pda).is_none());
        // Another light on the holder stays untouched.
        let fire = Optics {
            light: Some(LightSource {
                range: 4.0,
                ..light()
            }),
            ..default()
        };
        app.world_mut().entity_mut(mob).insert(fire.clone());
        let mut pc = app.world_mut().get_mut::<ModularComputer>(pda).unwrap();
        pc.light_on = true;
        app.world_mut()
            .entity_mut(pda)
            .remove::<TilePos>()
            .insert(CarriedBy(mob));
        app.update();
        assert_eq!(app.world().get::<Optics>(mob), Some(&fire));
    }

    #[test]
    fn name_follows_the_imprint() {
        let mut world = World::new();
        let mut pc = ModularComputer::pda(PDA_ROOT, "pda");
        assert_eq!(pc.display_name(), "pda");
        let card = world
            .spawn(IdRegistration {
                registered_name: "Боб".into(),
                assignment: "Инженер".into(),
                job: "Station Engineer".into(),
            })
            .id();
        pc.stored_id = Some(card);
        let pda = world.spawn(pc).id();
        imprint_id(&mut world, pda, Some("Алиса"), Some("Ассистент"));
        assert_eq!(
            world.get::<DmName>(pda).unwrap().name,
            "КПК «Алиса» (Ассистент)"
        );
        // "Привязать ID": the inserted card's registration.
        imprint_id(&mut world, pda, None, None);
        assert_eq!(
            world.get::<DmName>(pda).unwrap().name,
            "КПК «Боб» (Инженер)"
        );
    }

    #[test]
    fn an_emag_installs_the_minesweeper() {
        use super::super::programs::minesweeper::{Minesweeper, MinesweeperAct, Mode};
        let mut world = World::new();
        world.init_resource::<Messages<SendChat>>();
        world.init_resource::<Time>();
        let client = world.spawn_empty().id();
        let pda = world.spawn(ModularComputer::pda(PDA_ROOT, "pda")).id();
        // Off: refused.
        assert!(!emag_act(&mut world, client, pda));
        world.get_mut::<ModularComputer>(pda).unwrap().enabled = true;
        assert!(emag_act(&mut world, client, pda));
        assert!(!emag_act(&mut world, client, pda), "already emagged");
        let pc = world.get::<ModularComputer>(pda).unwrap().clone();
        assert!(pc.emagged);
        assert_eq!(
            pc.programs
                .iter()
                .filter(|&&p| p == ProgramKind::Minesweeper)
                .count(),
            1
        );
        // Only the program in front takes its clicks.
        let square = MinesweeperAct::Square(4, 4, Mode::Open);
        assert!(
            super::super::programs::minesweeper::apply(
                &mut world,
                client,
                client,
                pda,
                &pc,
                square.clone()
            )
            .is_err()
        );
        let mut pc = pc;
        assert!(pc.open_program(ProgramKind::Minesweeper));
        super::super::programs::minesweeper::apply(&mut world, client, client, pda, &pc, square)
            .unwrap();
        assert!(!world.get::<Minesweeper>(pda).unwrap().first_touch);
    }

    #[test]
    fn acts_parse_hostile_params() {
        let p = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect()
        };
        let parse = |a: &str, pairs: &[(&str, &str)]| {
            let params = p(pairs);
            NtosUi::parse_act(a, &UiParams::new(&params))
        };
        assert_eq!(
            parse(PC_RUNPROGRAM, &[("name", "nt_messenger")]),
            Ok(NtosAct::Run(ProgramKind::Messenger))
        );
        assert!(parse(PC_RUNPROGRAM, &[("name", "../../etc")]).is_err());
        assert!(parse(PC_EJECT_DISK, &[("name", "Eject Disk")]).is_err());
        assert!(parse(PDA_SEND_MESSAGE, &[("message", "x")]).is_err());
        assert!(parse(PDA_SEND_MESSAGE, &[("ref", "pda:abc"), ("message", "x")]).is_err());
        assert!(parse(PDA_CLOSE_MESSAGES, &[("ref", "pda:5")]).is_err());
        assert_eq!(parse(PDA_VIEW_MESSAGES, &[]), Ok(NtosAct::View(None)));
        assert_eq!(parse("PC_hack", &[]), Err(UiReject::UnknownAction));
        assert_eq!(
            parse(
                UPDATE_NOTE,
                &[("newnote", "ab"), ("newnote.1", "cd"), ("newnote.3", "x")]
            ),
            Ok(NtosAct::UpdateNote("abcd".into()))
        );
    }
}
