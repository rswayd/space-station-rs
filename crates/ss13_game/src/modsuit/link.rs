//! MODlink video calls (server), after tg `code/modules/mod/mod_link.dm`:
//!
//! - Every control unit and MODlink scryer (`/obj/item/clothing/neck/link_scryer`) has a
//!   MODlink ([`ModLink`]): a unique numeric id and a frequency (the unit type's
//!   `starting_frequency`; the loaded scryers `NT`, the Charlie one `CHRL`). A right click with
//!   a multitool copies the frequency into its buffer or imprints the buffer's (both set: asked).
//! - Calling (the MODsuit window's `call`, the scryer's action button) lists the links on the
//!   same frequency that can call; the callee gets the ring and a 10 s accept/deny question.
//!   Accepted, each side sees a hologram of the other's user next to its own user (offset by
//!   the way the user faces) and what each user says is said by its hologram at the other end
//!   (range 2 from a unit, 3 from a scryer).
//! - A unit can call while it has charge and a living wearer; a scryer while worn on the neck
//!   by a living mob with charge in its cell. The call ends when either side cannot, the
//!   frequencies differ, a unit starts (de)activating, or a scryer leaves the neck; it costs a
//!   unit 12.5 W and a scryer `0.02 × STANDARD_CELL_RATE`.
//! - Scryers take a power cell (none inside), give it back to a right click with an empty hand,
//!   and take a label used in hand ("MODlink scryer - <label>").
// shortcut: the incoming call is a question window (tg: a clickable screen alert, right click
// denies); the hologram is the user's appearance tinted, without the `modlink` frame, the crop
// filter, the mirroring/shear transforms or overlay updates mid-call; no `modlink_active` worn
// overlay, no examine lines, no TTS on the hologram's speech.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_chat::server::{Listeners, Spoke, Understanding, say_message, tagged};
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmName};
use ss13_net::chat::server::SendChat;
use ss13_net::server::Networked;
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;
use ss13_shared::{Appearance, Dir, Facing, IconStateId, Layer, MouseOpacity, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::UiSystems;
use ss13_world::SpatialIndex;

use super::server::{ModDb, ModSuit, charge, subtract_charge};
use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::health::Stat;
use crate::inventory::{CarriedBy, Inventory};
use crate::mecha::core::ItemCell;
use crate::server::PlayerMob;
use crate::storage::StoredIn;
use crate::tools::{ToolKind, tell, tell_mob, tool_of};

pub const SCRYER: &str = "/obj/item/clothing/neck/link_scryer";
const SCRYER_LOADED: &str = "/obj/item/clothing/neck/link_scryer/loaded";
const SCRYER_CHARLIE: &str = "/obj/item/clothing/neck/link_scryer/loaded/charlie";
/// `MODLINK_FREQ_NANOTRASEN`, `MODLINK_FREQ_CHARLIE`.
const FREQ_NANOTRASEN: &str = "NT";
const FREQ_CHARLIE: &str = "CHRL";
const HIGH_CELL: &str = "/obj/item/stock_parts/power_store/cell/high";
const RING: &str = "sound/items/weapons/ring.ogg";
const TERMINAL: &str = "sound/machines/terminal/terminal_processing.ogg";
/// `/atom/movable/screen/alert/modlink_call` `timeout`.
const CALL_TIMEOUT_SECS: f64 = 10.0;
/// `0.25 * DEFAULT_CHARGE_DRAIN` of a unit in a call, watts.
const SUIT_CALL_DRAIN: f64 = 0.25 * 0.005 * 10_000.0;
/// `0.02 * STANDARD_CELL_RATE` of a scryer in a call, watts.
const SCRYER_CALL_DRAIN: f64 = 0.02 * crate::power::chargers::STANDARD_CELL_RATE;
/// `MAX_NAME_LEN`.
const MAX_NAME_LEN: u32 = 42;
/// `makeHologram(0.75)`: `rgb(125, 180, 225, 0.75 * 255)`.
const HOLO_COLOR: [u8; 4] = [125, 180, 225, 191];
/// `BELOW_MOB_LAYER`, `ABOVE_ALL_MOB_LAYER`.
const BELOW_MOB_LAYER: f32 = 3.7;
const ABOVE_ALL_MOB_LAYER: f32 = 4.6;
/// `GAME_PLANE`.
const GAME_PLANE: i16 = -4;
/// Action button ids: `modlink:<scryer bits>`.
const BUTTON: &str = "modlink:";

/// tg `/datum/mod_link` on its holder.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ModLink {
    pub id: String,
    /// `None`: unset.
    pub frequency: Option<String>,
    /// The other holder in the call (`link_call`).
    pub call: Option<Entity>,
    /// The hologram of this link's user, shown at the other end (`visual`).
    pub visual: Option<Entity>,
    /// `user_in_call_ref`.
    pub user_in_call: Option<Entity>,
}

/// On a MODlink scryer: its cell (inside it) and `label`, and its base name.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Scryer {
    pub cell: Option<Entity>,
    pub label: String,
    base_name: String,
}

/// On a mob in a call (`TRAIT_IN_CALL`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct InCall;

/// On a hologram: the link whose user it shows and the holder at whose user it stands.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkVisual {
    pub link: Entity,
    pub at: Entity,
}

/// The questions of MODlinks, on their prompt entity.
#[derive(Component, Clone, Debug)]
enum LinkPrompt {
    /// `call_link`'s list: the holders listed.
    Choose { from: Entity, options: Vec<Entity> },
    /// The callee's accept/deny (tg's alert).
    Incoming {
        caller: Entity,
        receiver: Entity,
        user: Entity,
    },
    /// The multitool's "Copy" or "Imprint".
    Frequency { holder: Entity, tool: Entity },
    /// The scryer's label.
    Label { scryer: Entity },
}

pub struct ModLinkPlugin;

impl Plugin for ModLinkPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                init_links,
                process_calls
                    .after(crate::inventory::inventory_intents)
                    .before(UiSystems::Open),
                (
                    prompt_answers.after(PromptSystems::Answer),
                    trigger_buttons.run_if(resource_exists::<Messages<UiActed>>),
                )
                    .in_set(UiSystems::Handle),
                scryer_buttons.after(crate::inventory::inventory_intents),
                relay_speech
                    .after(ss13_chat::server::SaySystems)
                    .run_if(resource_exists::<Messages<Spoke>>),
            ),
        )
        .add_observer(holder_removed);
    }
}

type UiActed = ss13_tgui::server::UiActed<ActionButtonsUi>;

fn path_of(world: &World, e: Entity) -> Option<String> {
    world.get::<DmItem>(e).map(|i| i.path.clone())
}

fn name(world: &World, e: Entity) -> String {
    crate::security::name_of(world, e)
}

/// tg `/datum/mod_link/New`: three digits (four after ten collisions, and so on), unique.
fn new_id(world: &mut World) -> String {
    let taken: Vec<String> = {
        let mut q = world.query::<&ModLink>();
        q.iter(world).map(|l| l.id.clone()).collect()
    };
    let mut digits = 3;
    let mut attempts = 0;
    loop {
        if attempts == 10 {
            attempts = 0;
            digits += 1;
        }
        let id: String = (0..digits)
            .map(|_| {
                let d = world
                    .get_resource_mut::<crate::combat::CombatRng>()
                    .map_or(0, |mut r| r.range(0, 9));
                char::from_digit(d, 10).unwrap_or('0')
            })
            .collect();
        attempts += 1;
        if !taken.contains(&id) {
            return id;
        }
    }
}

/// Units and scryers that appeared get their link (and a scryer its state, the loaded ones a
/// high-capacity cell).
fn init_links(
    suits: Query<Entity, (Added<ModSuit>, Without<ModLink>)>,
    items: Query<(Entity, &DmItem), (Added<DmItem>, Without<ModLink>)>,
    mut commands: Commands,
) {
    for control in &suits {
        commands.queue(move |world: &mut World| init_suit_link(world, control));
    }
    for (e, item) in &items {
        if is_under(&item.path, SCRYER) {
            let path = item.path.clone();
            commands.queue(move |world: &mut World| init_scryer(world, e, &path));
        }
    }
}

fn init_suit_link(world: &mut World, control: Entity) {
    if world.get_entity(control).is_err() || world.get::<ModLink>(control).is_some() {
        return;
    }
    let frequency = world
        .get::<ModSuit>(control)
        .and_then(|s| world.resource::<ModDb>().data.suits.get(s.suit))
        .map(|d| d.starting_frequency.clone())
        .filter(|f| !f.is_empty());
    let id = new_id(world);
    world.entity_mut(control).insert(ModLink {
        id,
        frequency,
        ..default()
    });
}

fn init_scryer(world: &mut World, scryer: Entity, path: &str) {
    if world.get_entity(scryer).is_err() || world.get::<ModLink>(scryer).is_some() {
        return;
    }
    let frequency = if is_under(path, SCRYER_CHARLIE) {
        Some(FREQ_CHARLIE.to_owned())
    } else if is_under(path, SCRYER_LOADED) {
        Some(FREQ_NANOTRASEN.to_owned())
    } else {
        None
    };
    let cell = if is_under(path, SCRYER_LOADED) {
        crate::construction::spawn_detached(world, HIGH_CELL)
    } else {
        None
    };
    if let Some(cell) = cell {
        crate::storage::put_inside(world, cell, scryer);
    }
    let id = new_id(world);
    let base_name = world
        .get::<DmName>(scryer)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    world.entity_mut(scryer).insert((
        ModLink {
            id,
            frequency,
            ..default()
        },
        Scryer {
            cell,
            label: String::new(),
            base_name,
        },
    ));
}

/// The scryer's cell, if still inside it.
fn scryer_cell(world: &World, scryer: Entity) -> Option<Entity> {
    let cell = world.get::<Scryer>(scryer)?.cell?;
    world
        .get::<StoredIn>(cell)
        .is_some_and(|s| s.0 == scryer)
        .then_some(cell)
}

fn cell_charge(world: &World, scryer: Entity) -> f64 {
    scryer_cell(world, scryer)
        .and_then(|c| world.get::<ItemCell>(c))
        .map_or(0.0, |c| c.0.charge)
}

/// `get_user_callback`: a unit's wearer, the mob wearing a scryer on its neck.
pub fn user_of(world: &World, holder: Entity) -> Option<Entity> {
    if let Some(suit) = world.get::<ModSuit>(holder) {
        return suit.wearer;
    }
    let mob = world.get::<CarriedBy>(holder)?.0;
    (world.get::<Inventory>(mob)?.slot_of(holder)? == InventorySlot::Neck).then_some(mob)
}

/// `can_call_callback`: a unit with charge and a living wearer; a scryer held or worn by a
/// living mob with charge in its cell.
pub fn can_call(world: &World, holder: Entity) -> bool {
    let alive = |mob: Entity| world.get::<Stat>(mob).is_some_and(|s| *s != Stat::Dead);
    if let Some(suit) = world.get::<ModSuit>(holder) {
        return charge(world, holder) > 0.0 && suit.wearer.is_some_and(alive);
    }
    world.get::<CarriedBy>(holder).is_some_and(|c| alive(c.0)) && cell_charge(world, holder) > 0.0
}

/// tg `/proc/call_link`: the links on the frequency that can call, as a list.
pub fn call_link(world: &mut World, client: Entity, user: Entity, from: Entity) {
    let Some(frequency) = world.get::<ModLink>(from).and_then(|l| l.frequency.clone()) else {
        return;
    };
    let mut q = world.query::<(Entity, &ModLink)>();
    let mut others: Vec<(Entity, String)> = q
        .iter(world)
        .filter(|(e, l)| *e != from && l.frequency.as_deref() == Some(frequency.as_str()))
        .map(|(e, l)| (e, l.id.clone()))
        .collect();
    others.retain(|&(e, _)| can_call(world, e));
    others.sort_by(|a, b| a.1.cmp(&b.1));
    if others.is_empty() {
        tell(
            world,
            client,
            Span::Warning,
            format!("нет целей на частоте {frequency}!"),
        );
        return;
    }
    let items: Vec<String> = others
        .iter()
        .map(|&(e, ref id)| format!("{} ({id})", name(world, e)))
        .collect();
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            user,
            "MODlink",
            format!("Выберите ID для вызова на частоте {frequency}"),
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(LinkPrompt::Choose {
        from,
        options: others.into_iter().map(|(e, _)| e).collect(),
    });
}

/// tg `/datum/mod_link/proc/call_link`: the checks, the ring and the callee's question.
fn ring(world: &mut World, client: Entity, user: Entity, caller: Entity, called: Entity) {
    let refuse = |world: &mut World, text: &str| tell(world, client, Span::Warning, text);
    let Some(from) = world.get::<ModLink>(caller).cloned() else {
        return;
    };
    if world.get::<ModLink>(called).is_none() {
        return refuse(world, "неверная цель!");
    }
    if from.frequency.is_none() {
        return;
    }
    let Some(link_user) = user_of(world, caller) else {
        return;
    };
    if world.get::<InCall>(link_user).is_some() {
        return refuse(world, "уже идёт вызов!");
    }
    let Some(target) = user_of(world, called) else {
        return refuse(world, "неверная цель!");
    };
    if world.get::<InCall>(target).is_some() {
        return refuse(world, "цель уже в вызове!");
    }
    if !can_call(world, caller) || !can_call(world, called) {
        return refuse(world, "вызов невозможен!");
    }
    let Some(target_client) = world.get::<PlayerMob>(target).map(|p| p.client) else {
        return refuse(world, "вызов невозможен!");
    };
    if let Some(pos) = crate::chem::map_pos(world, called) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, RING, 15.0, true));
    }
    let text = format!(
        "{} ({}) вызывает вас! Принять вызов?",
        name(world, caller),
        from.id
    );
    let prompt = ask_world(
        world,
        Ask::new(
            target_client,
            target,
            "Входящий вызов MODlink",
            text,
            PromptKind::Alert {
                buttons: vec!["Принять".to_owned(), "Отклонить".to_owned()],
                autofocus: true,
            },
        )
        .timeout(CALL_TIMEOUT_SECS),
    );
    world.entity_mut(prompt).insert(LinkPrompt::Incoming {
        caller,
        receiver: called,
        user,
    });
}

/// `/datum/mod_link_call/New`: both links enter the call.
pub fn start_call(world: &mut World, caller: Entity, receiver: Entity) {
    for (me, other) in [(caller, receiver), (receiver, caller)] {
        if let Some(mut l) = world.get_mut::<ModLink>(me) {
            l.call = Some(other);
        }
    }
    entered_call(world, caller, receiver);
    entered_call(world, receiver, caller);
}

/// `entered_call`: this side's user is in the call; the other's hologram goes to this user.
fn entered_call(world: &mut World, me: Entity, other: Entity) {
    let user = user_of(world, me);
    if let Some(mut l) = world.get_mut::<ModLink>(me) {
        l.user_in_call = user;
    }
    if let Some(user) = user {
        world.entity_mut(user).insert(InCall);
    }
    make_visual(world, other, me);
    if let Some(pos) = crate::chem::map_pos(world, me) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, TERMINAL, 50.0, true),
        );
    }
}

/// `make_link_visual_generic` + `get_link_visual_generic`: the hologram of `link`'s user,
/// standing with the user of `at`.
fn make_visual(world: &mut World, link: Entity, at: Entity) {
    let Some(user) = user_of(world, link) else {
        return;
    };
    let id = world
        .get::<ModLink>(link)
        .map(|l| l.id.clone())
        .unwrap_or_default();
    // A user without a look of its own still gets an (invisible) hologram to speak from.
    let mut look = world
        .get::<Appearance>(user)
        .cloned()
        .unwrap_or(Appearance {
            icon_state: IconStateId(u32::MAX),
            layer: Layer(ABOVE_ALL_MOB_LAYER),
            plane: GAME_PLANE,
            color: None,
            pixel_offset: [0, 0],
            rotation: 0,
            mouse_opacity: MouseOpacity::Transparent,
            glide: 0,
            overlays: Vec::new(),
        });
    look.color = Some(HOLO_COLOR);
    look.mouse_opacity = MouseOpacity::Transparent;
    let pos = user_of(world, at)
        .and_then(|u| world.get::<TilePos>(u).copied())
        .or_else(|| crate::chem::map_pos(world, at));
    let Some(pos) = pos else {
        return;
    };
    let visual = world
        .spawn((
            LinkVisual { link, at },
            pos,
            Facing(Dir::South),
            look,
            Networked,
            Name::new(format!("holocall ({id})")),
        ))
        .id();
    if let Some(mut l) = world.get_mut::<ModLink>(link) {
        l.visual = Some(visual);
    }
    place_visual(world, visual);
}

/// `on_user_set_dir_generic`: the hologram beside the user it stands with, by the user's
/// facing (north: behind, south: in front, east/west: at the side).
fn place_visual(world: &mut World, visual: Entity) {
    let Some(&LinkVisual { at, .. }) = world.get::<LinkVisual>(visual) else {
        return;
    };
    let Some(user) = user_of(world, at) else {
        return;
    };
    let Some(&pos) = world.get::<TilePos>(user) else {
        return;
    };
    let dir = world.get::<Facing>(user).map_or(Dir::South, |f| f.0);
    let (offset, layer) = visual_offset(dir);
    if world.get::<TilePos>(visual) != Some(&pos) {
        world.entity_mut(visual).insert(pos);
    }
    if let Some(mut look) = world.get_mut::<Appearance>(visual)
        && (look.pixel_offset != offset || look.layer != Layer(layer))
    {
        look.pixel_offset = offset;
        look.layer = Layer(layer);
    }
}

/// The hologram's pixel offset and layer for the facing of the user it stands with.
pub fn visual_offset(dir: Dir) -> ([i16; 2], f32) {
    match dir {
        Dir::North => ([0, 13], BELOW_MOB_LAYER),
        Dir::East => ([14, 0], BELOW_MOB_LAYER),
        Dir::West => ([-14, 0], BELOW_MOB_LAYER),
        _ => ([0, -24], ABOVE_ALL_MOB_LAYER),
    }
}

/// `/datum/mod_link/proc/end_call` (`mod_link_call/Destroy`): both sides leave the call.
pub fn end_call(world: &mut World, holder: Entity) {
    let Some(other) = world.get::<ModLink>(holder).and_then(|l| l.call) else {
        return;
    };
    for side in [holder, other] {
        exiting_call(world, side);
    }
}

/// `exiting_call`: the user leaves the call, this side's hologram goes.
fn exiting_call(world: &mut World, me: Entity) {
    let Some(link) = world.get::<ModLink>(me).cloned() else {
        return;
    };
    if let Some(user) = link.user_in_call
        && let Ok(mut e) = world.get_entity_mut(user)
    {
        e.remove::<InCall>();
    }
    if let Some(visual) = link.visual {
        world.try_despawn(visual).ok();
    }
    if let Some(mut l) = world.get_mut::<ModLink>(me) {
        l.call = None;
        l.visual = None;
        l.user_in_call = None;
    }
    if let Some(other) = link.call
        && let Some(pos) = crate::chem::map_pos(world, other)
    {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, TERMINAL, 50.0, true),
        );
    }
}

/// `mod_link_call/process` (`can_continue_call`), the scryer's `equipped`/`dropped`, the unit's
/// `toggle_activate`; the charge of the call; the holograms follow their users.
fn process_calls(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs_f64();
    let mut q = world.query::<(Entity, &ModLink)>();
    let calls: Vec<(Entity, Entity)> = q
        .iter(world)
        .filter_map(|(e, l)| Some((e, l.call?)))
        .collect();
    for &(me, other) in &calls {
        let Some(link) = world.get::<ModLink>(me).cloned() else {
            continue;
        };
        if link.call != Some(other) {
            continue;
        }
        let same_freq = world
            .get::<ModLink>(other)
            .is_some_and(|o| o.frequency == link.frequency && o.call == Some(me));
        let activating = world.get::<ModSuit>(me).is_some_and(|s| s.activating);
        let same_user = user_of(world, me) == link.user_in_call;
        if !same_freq || activating || !same_user || !can_call(world, me) || !can_call(world, other)
        {
            end_call(world, me);
            continue;
        }
        if world.get::<ModSuit>(me).is_some() {
            subtract_charge(world, me, SUIT_CALL_DRAIN * dt);
        } else if let Some(cell) = scryer_cell(world, me)
            && let Some(mut c) = world.get_mut::<ItemCell>(cell)
        {
            c.0.take(SCRYER_CALL_DRAIN * dt, true);
        }
    }
    let mut visuals = world.query_filtered::<Entity, With<LinkVisual>>();
    let visuals: Vec<Entity> = visuals.iter(world).collect();
    for visual in visuals {
        place_visual(world, visual);
    }
}

/// tg `Hear` of a holder: what its user (or a unit's AI) says, its hologram says at the other
/// end (`message_range` 2 from a unit, 3 from a scryer).
fn relay_speech(
    mut spoken: MessageReader<Spoke>,
    links: Query<(Entity, &ModLink, Option<&Scryer>)>,
    visuals: Query<(&TilePos, &Name), With<LinkVisual>>,
    assistants: Query<&super::ai::ModAssistant>,
    eyes: Query<&crate::silicon::ai::AiEye>,
    index: Res<SpatialIndex>,
    listeners: Res<Listeners>,
    understanding: Understanding,
    mut chat: MessageWriter<SendChat>,
) {
    for spoke in spoken.read() {
        if spoke.radio.is_some() {
            continue;
        }
        let ai = eyes.get(spoke.speaker).ok().map(|e| e.ai);
        for (holder, link, scryer) in &links {
            if link.call.is_none() {
                continue;
            }
            let own_ai =
                ai.is_some() && assistants.get(holder).ok().map(|a| a.ai) == ai && scryer.is_none();
            if link.user_in_call != Some(spoke.speaker) && !own_ai {
                continue;
            }
            let Some((&pos, name)) = link.visual.and_then(|v| visuals.get(v).ok()) else {
                continue;
            };
            let range = if scryer.is_some() { 3 } else { 2 };
            let clients: Vec<Entity> = index
                .in_radius(pos, range)
                .filter_map(|e| listeners.client_of(e))
                .collect();
            for heard in understanding.split(clients, spoke.language, &spoke.text) {
                chat.write(SendChat {
                    clients: heard.clients,
                    message: tagged(say_message(name.as_str(), &heard.text), heard.tag),
                });
            }
        }
    }
}

/// tg `/datum/mod_link/Destroy` with its holder: the call ends.
fn holder_removed(remove: On<Remove<ModLink>>, links: Query<&ModLink>, mut commands: Commands) {
    let holder = remove.entity;
    let Ok(link) = links.get(holder) else {
        return;
    };
    let (other, visual, user) = (link.call, link.visual, link.user_in_call);
    commands.queue(move |world: &mut World| {
        if let Some(visual) = visual {
            world.try_despawn(visual).ok();
        }
        if let Some(user) = user
            && let Ok(mut e) = world.get_entity_mut(user)
        {
            e.remove::<InCall>();
        }
        if let Some(other) = other
            && world
                .get::<ModLink>(other)
                .is_some_and(|l| l.call == Some(holder))
        {
            exiting_call(world, other);
        }
    });
}

/// The MODsuit window's `call`: call, or hang up.
pub fn window_call(world: &mut World, client: Entity, user: Entity, control: Entity) {
    if world
        .get::<ModLink>(control)
        .is_some_and(|l| l.call.is_some())
    {
        end_call(world, control);
    } else {
        call_link(world, client, user, control);
    }
}

/// The `link_id`, `link_freq` and `link_call` of the MODsuit window.
pub fn window_fields(world: &World, control: Entity) -> (String, Option<String>, Option<String>) {
    let Some(link) = world.get::<ModLink>(control) else {
        return (String::new(), None, None);
    };
    let other = link
        .call
        .and_then(|o| world.get::<ModLink>(o))
        .map(|o| o.id.clone());
    (link.id.clone(), link.frequency.clone(), other)
}

/// `multitool_act_secondary` of a unit or a scryer. True when the click was its.
pub fn multitool_act(
    world: &mut World,
    client: Entity,
    user: Entity,
    tool: Entity,
    holder: Entity,
) -> bool {
    if tool_of(world, tool).map(|t| t.0) != Some(ToolKind::Multitool) {
        return false;
    }
    let Some(mine) = world.get::<ModLink>(holder).map(|l| l.frequency.clone()) else {
        return false;
    };
    let buffered = crate::tools::multitool_buffer(world, tool)
        .and_then(|b| world.get::<ModLink>(b))
        .and_then(|l| l.frequency.clone());
    match (buffered, mine) {
        (None, Some(_)) => {
            crate::tools::set_multitool_buffer(world, tool, holder);
            tell(world, client, Span::Notice, "частота скопирована");
        }
        (Some(freq), None) => {
            if let Some(mut l) = world.get_mut::<ModLink>(holder) {
                l.frequency = Some(freq);
            }
            tell(world, client, Span::Notice, "частота установлена");
        }
        (Some(_), Some(_)) => {
            let prompt = ask_world(
                world,
                Ask::new(
                    client,
                    user,
                    "Частота MODlink",
                    "Скопировать или записать частоту?",
                    PromptKind::Alert {
                        buttons: vec!["Копировать".to_owned(), "Записать".to_owned()],
                        autofocus: true,
                    },
                ),
            );
            world
                .entity_mut(prompt)
                .insert(LinkPrompt::Frequency { holder, tool });
        }
        (None, None) => return false,
    }
    true
}

/// Scryer clicks with `held` (on the floor or in the other hand): a cell in. True when the
/// click was its.
pub fn scryer_item_interaction(
    world: &mut World,
    client: Entity,
    held: Entity,
    scryer: Entity,
) -> bool {
    if world.get::<Scryer>(scryer).is_none()
        || world.get::<ItemCell>(held).is_none()
        || scryer_cell(world, scryer).is_some()
        || !path_of(world, held)
            .is_some_and(|p| is_under(&p, "/obj/item/stock_parts/power_store/cell"))
    {
        return false;
    }
    crate::storage::put_inside(world, held, scryer);
    if let Some(mut s) = world.get_mut::<Scryer>(scryer) {
        s.cell = Some(held);
    }
    tell(world, client, Span::Notice, "элемент питания установлен");
    true
}

/// `attack_hand_secondary`: the cell into the hands. True when there was one.
pub fn remove_cell(world: &mut World, client: Entity, mob: Entity, scryer: Entity) -> bool {
    let Some(cell) = scryer_cell(world, scryer) else {
        return false;
    };
    if let Some(mut s) = world.get_mut::<Scryer>(scryer) {
        s.cell = None;
    }
    let Some(pos) = crate::chem::map_pos(world, scryer) else {
        return true;
    };
    crate::storage::take_out(world, cell, pos);
    if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, cell);
    }
    tell(world, client, Span::Notice, "элемент питания извлечён");
    true
}

/// `attack_self`: the label question.
pub fn scryer_attack_self(world: &mut World, client: Entity, mob: Entity, scryer: Entity) -> bool {
    let Some(label) = world.get::<Scryer>(scryer).map(|s| s.label.clone()) else {
        return false;
    };
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Изменить имя",
            "Изменить видимое имя",
            PromptKind::Text {
                default: label,
                max_length: MAX_NAME_LEN,
                multiline: false,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(LinkPrompt::Label { scryer });
    true
}

fn prompt_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answers {
        let Some(question) = world.get::<LinkPrompt>(prompt).cloned() else {
            continue;
        };
        let holds = |world: &World, item: Entity| {
            world.get::<CarriedBy>(item).is_some_and(|c| c.0 == actor)
        };
        match question {
            LinkPrompt::Choose { from, options } => {
                let Some(PromptAnswer::Item(i)) = answer else {
                    continue;
                };
                if let Some(&called) = options.get(i) {
                    ring(world, client, actor, from, called);
                }
            }
            LinkPrompt::Incoming {
                caller,
                receiver,
                user,
            } => {
                let busy = |world: &World, e: Entity| {
                    world.get::<ModLink>(e).is_none_or(|l| l.call.is_some())
                };
                let user_client = world.get::<PlayerMob>(user).map(|p| p.client);
                let end_message = match answer {
                    Some(PromptAnswer::Button(0)) => {
                        // `modlink_call/Click`: both links still free, the callee still the
                        // receiver's user.
                        if busy(world, caller)
                            || busy(world, receiver)
                            || user_of(world, receiver) != Some(actor)
                            || !can_call(world, caller)
                            || !can_call(world, receiver)
                        {
                            continue;
                        }
                        start_call(world, caller, receiver);
                        "вызов принят"
                    }
                    Some(_) => "вызов отклонён!",
                    None => "время вызова истекло!",
                };
                if let Some(c) = user_client {
                    tell(world, c, Span::Notice, end_message);
                }
            }
            LinkPrompt::Frequency { holder, tool } => {
                let Some(PromptAnswer::Button(i)) = answer else {
                    continue;
                };
                if !holds(world, tool) || world.get::<ModLink>(holder).is_none() {
                    continue;
                }
                if i == 0 {
                    crate::tools::set_multitool_buffer(world, tool, holder);
                    tell(world, client, Span::Notice, "частота скопирована");
                } else {
                    let freq = crate::tools::multitool_buffer(world, tool)
                        .and_then(|b| world.get::<ModLink>(b))
                        .and_then(|l| l.frequency.clone());
                    if let Some(freq) = freq
                        && let Some(mut l) = world.get_mut::<ModLink>(holder)
                    {
                        l.frequency = Some(freq);
                        tell(world, client, Span::Notice, "частота установлена");
                    }
                }
            }
            LinkPrompt::Label { scryer } => {
                if !holds(world, scryer) {
                    continue;
                }
                let label = match answer {
                    Some(PromptAnswer::Text(t)) => t.trim().to_owned(),
                    _ => String::new(),
                };
                if label.is_empty() {
                    tell(world, client, Span::Warning, "неверное имя!");
                    continue;
                }
                let base = world
                    .get::<Scryer>(scryer)
                    .map(|s| s.base_name.clone())
                    .unwrap_or_default();
                if let Some(mut s) = world.get_mut::<Scryer>(scryer) {
                    s.label = label.clone();
                }
                if let Some(mut n) = world.get_mut::<DmName>(scryer) {
                    n.name = format!("{base} - {label}");
                }
                tell(world, client, Span::Notice, "имя установлено");
            }
        }
    }
}

/// The scryer's action button (`/datum/action/item_action/call_link`) for the mob wearing it.
pub fn refresh_scryer_buttons(world: &mut World, mob: Entity) {
    if world.get::<PlayerMob>(mob).is_none() {
        return;
    }
    let mut buttons: Vec<(ActionButton, f64)> = world
        .get::<ActionButtons>(mob)
        .map(|b| {
            b.0.iter()
                .filter(|(b, _)| !b.id.starts_with(BUTTON))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let neck = world.get::<Inventory>(mob).and_then(|i| i.neck);
    let worn = neck.filter(|&n| world.get::<Scryer>(n).is_some());
    if let Some(scryer) = worn {
        // An item action shows the item.
        buttons.push((
            ActionButton {
                id: format!("{BUTTON}{}", scryer.to_bits()),
                name: "Вызов MODlink".to_owned(),
                icon: world.get::<Appearance>(scryer).map(|a| a.icon_state),
                background: None,
                border: None,
                available: true,
                cooldown: 0.0,
                active: world
                    .get::<ModLink>(scryer)
                    .is_some_and(|l| l.call.is_some()),
                maptext: None,
            },
            0.0,
        ));
    } else if world
        .get::<ActionButtons>(mob)
        .is_none_or(|b| !b.0.iter().any(|(b, _)| b.id.starts_with(BUTTON)))
    {
        return;
    }
    set_buttons(world, mob, buttons);
}

/// `ui_action_click` of the scryer: hang up, or call.
fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed>>())
        .filter(|a| a.act.starts_with(BUTTON))
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client) {
            continue;
        }
        if world.get::<Stat>(actor) != Some(&Stat::Conscious) {
            continue;
        }
        let Some(scryer) = id
            .trim_start_matches(BUTTON)
            .parse::<u64>()
            .ok()
            .and_then(Entity::try_from_bits)
            .filter(|&s| user_of(world, s) == Some(actor))
            .filter(|&s| world.get::<Scryer>(s).is_some())
        else {
            continue;
        };
        if world
            .get::<ModLink>(scryer)
            .is_some_and(|l| l.call.is_some())
        {
            end_call(world, scryer);
        } else if scryer_cell(world, scryer).is_none() {
            tell_mob(
                world,
                actor,
                Span::Warning,
                "не установлен элемент питания!",
            );
        } else if cell_charge(world, scryer) <= 0.0 {
            tell_mob(world, actor, Span::Warning, "нет заряда!");
        } else {
            call_link(world, client, actor, scryer);
        }
    }
}

/// The scryer's button follows the neck slot of every mob whose inventory changed.
fn scryer_buttons(mobs: Query<Entity, Changed<Inventory>>, mut commands: Commands) {
    for mob in &mobs {
        commands.queue(move |world: &mut World| refresh_scryer_buttons(world, mob));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hologram_sits_by_the_facing() {
        assert_eq!(visual_offset(Dir::North), ([0, 13], BELOW_MOB_LAYER));
        assert_eq!(visual_offset(Dir::South), ([0, -24], ABOVE_ALL_MOB_LAYER));
        assert_eq!(visual_offset(Dir::East).0, [14, 0]);
        assert_eq!(visual_offset(Dir::West).0, [-14, 0]);
    }
}
