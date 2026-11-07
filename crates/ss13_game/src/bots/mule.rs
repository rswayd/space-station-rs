//! The MULEbot (server), after tg `code/modules/mob/living/basic/bots/mulebot/` (`mulebot.dm`,
//! `mulebot_ai.dm`, `mulebot_control.dm`, `mulebot_movement.dm`, `mulebot_tool_interactions.dm`)
//! and `/datum/wires/mulebot`.
//!
//! - [`Mule`]: its id (named "MULEbot <id>"), the load (a crate in it, off the map, drawn on its
//!   platform; or a rider), the delivery flags, the home and destination beacon tags and its
//!   power cell (`/obj/item/stock_parts/power_store/cell/upgraded` inside it). Every step uses
//!   5 J; out of charge, or with a power wire cut, it turns off and stays off.
//! - Delivery ([`super::ai`]): "go" heads for the destination's delivery beacon (`codes_txt`
//!   `delivery`), "home" for the home one; at the beacon it unloads in the beacon's `dir`
//!   (reporting on the supply channel), or loads the crate there (auto-pickup), then returns home
//!   (auto-return) or idles.
//! - Loading: a crate dragged onto it (anything unanchored once the load check wire is cut; a
//!   living mob rides it). Unloading: the window, a delivery, the rider stepping off.
//! - Wires (`/datum/wires/mulebot`, colors random per bot): power 1/2 (cut: no power), avoidance
//!   (cut: bumps knock people over, it drives over the lying), load check, motors 1/2 (speed),
//!   receive (cut: silicons cannot use it), transmit, beacon (cut: it stops finding beacons).
//! - Driving over a lying human: emagged or with avoidance cut, `rand(5, 15)` brute weighted by
//!   zone; otherwise it slows down for 5 s (`careful_driving`).
//! - The `Mule` window: power, the load, destination, home and id, the flags, go/stop/home.
// shortcut: no GHOULbot replacement at load (0.666%), no PDA control (no bot control program),
// no frustration buzzing (a blocked move fails the delivery leaf, which retries), no loading
// animation, no rider pixel offset, no blood on the bot or tracks after running someone over;
// a rider stepping away gets off one tile over (tg unloads it onto the bot's tile).

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmCloset, DmItem};
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Appearance, Dir, Facing, Overlay, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::{Bumped, Moved, Mover, SpatialIndex, WorldSystems};

use super::{
    BOT_BLOCKED, BOT_DELIVER, BOT_GO_HOME, BOT_IDLE, Bot, BotKind, COVER_EMAGGED, COVER_LOCKED,
    COVER_MAINTS_OPEN, MuleData, MuleUi, NavBeacons, bot_name, set_mode,
};
use crate::health::{BodyPosition, DEFAULT_MESSAGE_RANGE, DamageType, Health, Stat};
use crate::inventory::Inventory;
use crate::mecha::core::ItemCell;
use crate::security::show;
use crate::storage::{StoredIn, put_inside, take_out};
use crate::tools::{Actor, ToolKind, tell};
use crate::wires::{WireHolder, WireSet, Wires};
use crate::zone::BodyZone;

pub const WIRE_AVOIDANCE: &str = "Avoidance";
pub const WIRE_LOADCHECK: &str = "Load Check";
pub const WIRE_MOTOR1: &str = "Motor 1";
pub const WIRE_MOTOR2: &str = "Motor 2";
pub const WIRE_RX: &str = "Receive";
pub const WIRE_TX: &str = "Transmit";
pub const WIRE_BEACON: &str = "Beacon";

/// `MULEBOT_*` delivery flags.
pub const RETURN_MODE: u32 = 1 << 0;
pub const AUTO_PICKUP_MODE: u32 = 1 << 1;
pub const REPORT_DELIVERY_MODE: u32 = 1 << 2;

const CELL: &str = "/obj/item/stock_parts/power_store/cell/upgraded";
const CRATE: &str = "/obj/structure/closet/crate";
/// `cell/upgraded` `maxcharge`: `STANDARD_CELL_CHARGE * 2.5`.
const UPGRADED_CELL_CHARGE: f64 = 25_000.0;
/// `cell_move_power_usage`: `0.0005 * STANDARD_CELL_CHARGE` (10 kJ).
const MOVE_ENERGY: f64 = 5.0;
/// `/datum/status_effect/careful_driving`: 5 s, +3 deciseconds per tile.
const CAREFUL_SECS: f64 = 5.0;
const CAREFUL_SLOWDOWN: f32 = 0.3;
/// `FAST_MOTOR_SPEED`, `AVERAGE_MOTOR_SPEED`, `SLOW_MOTOR_SPEED` (deciseconds per tile).
const MOTOR_SPEEDS: [f32; 3] = [1.0, 2.0, 3.0];
/// `load_overlay.pixel_y = initial(load.pixel_y) + 11`.
const LOAD_OFFSET: i16 = 11;
/// `MobBump` with the avoidance wire cut: `Knockdown(8 SECONDS)`.
const BUMP_KNOCKDOWN: f32 = 8.0;

const BUZZ_SIGH: &str = "sound/machines/buzz/buzz-sigh.ogg";
const BUZZ_ANNOYED: &str = "sound/machines/buzz/buzz-two.ogg";
const PING: &str = "sound/machines/ping.ogg";
const CHIME: &str = "sound/machines/chime.ogg";
const SPLAT: &str = "sound/effects/splat.ogg";
const HEAVY_DROP: &str = "heavy_drop";

/// The MULEbot's own state (tg `/mob/living/basic/bot/mulebot` vars and its blackboard tags).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Mule {
    pub id: String,
    /// What is on the platform: a crate (inside the bot) or the rider.
    pub load: Option<Entity>,
    /// `mulebot_delivery_flags`.
    pub flags: u32,
    pub cell: Option<Entity>,
    /// `BB_MULEBOT_HOME_BEACON`, `BB_MULEBOT_DESTINATION_BEACON` (beacon `location` tags).
    pub home: String,
    pub destination: Option<String>,
    /// `careful_driving` until this game time.
    pub careful_until: f64,
}

/// A mob riding a MULEbot (`buckle_mob` with the mulebot riding component).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Riding(pub Entity);

/// `/datum/wires/mulebot`.
pub fn wire_set() -> WireSet {
    WireSet {
        key: "/mob/living/basic/bot/mulebot",
        proper_name: "Mulebot",
        wires: [
            crate::wires::WIRE_POWER1,
            crate::wires::WIRE_POWER2,
            WIRE_AVOIDANCE,
            WIRE_LOADCHECK,
            WIRE_MOTOR1,
            WIRE_MOTOR2,
            WIRE_RX,
            WIRE_TX,
            WIRE_BEACON,
        ]
        .map(str::to_owned)
        .to_vec(),
    }
}

pub(super) fn build(app: &mut App) {
    app.register_ui::<MuleWindow>().add_systems(
        Update,
        (
            mule_moves.after(WorldSystems::Movement),
            mule_acts.in_set(UiSystems::Handle),
            mule_prompts
                .in_set(UiSystems::Handle)
                .after(PromptSystems::Answer),
        ),
    );
}

/// tg `Initialize`: the wires (`randomize = TRUE`: its own colors), the upgraded cell, the
/// id and name, the home beacon under it.
pub fn attach(world: &mut World, mob: Entity) {
    if world.get::<Mule>(mob).is_some() {
        return;
    }
    let (id, seed) = {
        let mut rng = world.resource_mut::<crate::combat::CombatRng>();
        // `assign_random_name()`: five of these.
        const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let id: String = (0..5)
            .map(|_| CHARS[rng.range(0, CHARS.len() as u32 - 1) as usize] as char)
            .collect();
        let seed = u64::from(rng.range(0, u32::MAX - 1));
        (id, seed)
    };
    let wires = Wires {
        holder: WireHolder::Mulebot,
        proper_name: "Mulebot",
        colors: crate::wires::colors_for(seed, &wire_set()),
        cut: Vec::new(),
    };
    let home = world
        .get::<TilePos>(mob)
        .copied()
        .and_then(|pos| beacon_at(world, pos))
        .unwrap_or_default();
    world.entity_mut(mob).insert((
        wires,
        Mule {
            id: id.clone(),
            flags: RETURN_MODE | AUTO_PICKUP_MODE | REPORT_DELIVERY_MODE,
            home,
            ..Mule::default()
        },
    ));
    let cell = crate::mecha::core::spawn_inside(world, CELL, mob);
    world.flush();
    // The cell's charge comes from `mecha.ron`; without it (fixtures) the upgraded cell's.
    if let Some(cell) = cell
        && world.get::<ItemCell>(cell).is_none()
    {
        world.entity_mut(cell).insert(ItemCell(crate::power::Cell {
            charge: UPGRADED_CELL_CHARGE,
            maxcharge: UPGRADED_CELL_CHARGE,
            chargerate: 0.0,
        }));
    }
    if let Some(mut mule) = world.get_mut::<Mule>(mob) {
        mule.cell = cell;
    }
    // `if(name == "\improper MULEbot") name = "\improper MULEbot [id]"`: not a mapped name.
    let default = default_name(world, mob);
    let current = bot_name(world, mob);
    if current == default {
        world
            .entity_mut(mob)
            .insert(Name::new(format!("{default} {id}")));
    }
}

/// The type's name ("MULEbot", as its prototype names it).
fn default_name(world: &World, mob: Entity) -> String {
    let Some(path) = world
        .get::<crate::mobs::BasicMob>(mob)
        .map(|b| b.def.path.clone())
    else {
        return String::new();
    };
    world
        .get_resource::<ss13_proto::Prototypes>()
        .and_then(|p| p.get(&path))
        .and_then(|p| p.get::<ss13_mapload::DmName>())
        .map(|n| {
            n.name
                .trim_start_matches("\\improper ")
                .trim_start_matches("\\proper ")
                .to_owned()
        })
        .unwrap_or_default()
}

/// `set_home(get_turf(src))` of the mapped MULEbots, once the map's beacons are known.
pub fn assign_homes(world: &mut World) {
    let homeless: Vec<(Entity, TilePos)> = world
        .query::<(Entity, &Mule, &TilePos)>()
        .iter(world)
        .filter(|(_, m, _)| m.home.is_empty())
        .map(|(e, _, p)| (e, *p))
        .collect();
    for (mob, pos) in homeless {
        if let Some(home) = beacon_at(world, pos)
            && let Some(mut m) = world.get_mut::<Mule>(mob)
        {
            m.home = home;
        }
    }
}

/// The location tag of a navigation beacon on `pos`.
fn beacon_at(world: &World, pos: TilePos) -> Option<String> {
    world
        .resource::<NavBeacons>()
        .0
        .values()
        .find(|b| b.pos == pos)
        .map(|b| b.location.clone())
        .or_else(|| {
            world
                .get_resource::<SpatialIndex>()?
                .on_tile(pos)
                .iter()
                .find_map(|&e| world.get::<ss13_mapload::DmNavBeacon>(e))
                .map(|b| b.location.clone())
        })
}

pub fn wire_cut(world: &World, mob: Entity, wire: &str) -> bool {
    world.get::<Wires>(mob).is_some_and(|w| w.is_cut(wire))
}

/// The cell still inside.
pub fn cell_of(world: &World, mob: Entity) -> Option<Entity> {
    let cell = world.get::<Mule>(mob)?.cell?;
    world
        .get::<StoredIn>(cell)
        .is_some_and(|s| s.0 == mob)
        .then_some(cell)
}

fn cell_charge(world: &World, mob: Entity) -> Option<crate::power::Cell> {
    cell_of(world, mob)
        .and_then(|c| world.get::<ItemCell>(c))
        .map(|c| c.0)
}

/// tg `has_power`: a charged cell and both power wires intact.
pub fn has_power(world: &World, mob: Entity) -> bool {
    cell_charge(world, mob).is_some_and(|c| c.charge > 0.0)
        && !wire_cut(world, mob, crate::wires::WIRE_POWER1)
        && !wire_cut(world, mob, crate::wires::WIRE_POWER2)
}

/// `mulebot/turn_on` refusals: the panel open, no power. True when it may turn on.
pub fn can_turn_on(world: &mut World, mob: Entity, user: Option<Entity>) -> bool {
    let Some(bot) = world.get::<Bot>(mob) else {
        return false;
    };
    if bot.kind != BotKind::Mule {
        return true;
    }
    let name = bot_name(world, mob);
    let refusal = if bot.cover & COVER_MAINTS_OPEN != 0 {
        Some(format!("Панель техобслуживания {name} открыта!"))
    } else if !has_power(world, mob) {
        Some(format!("У {name} нет питания!"))
    } else {
        None
    };
    match refusal {
        Some(text) => {
            if let Some(user) = user {
                tell_mob(world, user, Span::Warning, text);
            }
            false
        }
        None => true,
    }
}

use crate::tools::tell_mob;

/// `buzz(type)`: the message and the sound.
pub fn buzz(world: &mut World, mob: Entity, mood: Mood) {
    let name = bot_name(world, mob);
    let (text, file) = match mood {
        Mood::Sigh => (format!("{name} разочарованно жужжит."), BUZZ_SIGH),
        Mood::Annoyed => (format!("{name} раздражённо жужжит."), BUZZ_ANNOYED),
        Mood::Delight => (format!("{name} радостно пищит!"), PING),
        Mood::Chime => (format!("{name} издаёт звон!"), CHIME),
    };
    super::play(world, mob, file, 50.0);
    if let Some(&pos) = world.get::<TilePos>(mob) {
        show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Local,
            (Span::Italics, text),
            &[],
        );
    }
}

/// `MULEBOT_MOOD_*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Sigh,
    Annoyed,
    Delight,
    Chime,
}

/// The load's look on the platform (`update_overlays`): a thing's sprite 11 px up; a rider is
/// drawn as itself.
pub fn load_overlays(world: &World, mob: Entity) -> Vec<Overlay> {
    let Some(load) = world.get::<Mule>(mob).and_then(|m| m.load) else {
        return Vec::new();
    };
    if world.get::<Riding>(load).is_some() {
        return Vec::new();
    }
    let Some(look) = world.get::<Appearance>(load) else {
        return Vec::new();
    };
    let lift = |mut o: Overlay| {
        o.pixel_offset[1] += LOAD_OFFSET;
        o
    };
    let mut base = Overlay::new(look.icon_state);
    base.pixel_offset = [look.pixel_offset[0], look.pixel_offset[1] + LOAD_OFFSET];
    std::iter::once(base)
        .chain(look.overlays.iter().cloned().map(lift))
        .collect()
}

/// tg `load(atom_to_load)`: a crate (closed first), or anything unanchored with the load check
/// wire cut; a living mob rides. True when loaded.
pub fn load(world: &mut World, mob: Entity, thing: Entity) -> bool {
    let Some(mule) = world.get::<Mule>(mob) else {
        return false;
    };
    if mule.load.is_some() || thing == mob {
        return false;
    }
    let Some(&at) = world.get::<TilePos>(thing) else {
        // `if(!isturf(prev_loc))`: not from an inventory or a container.
        return false;
    };
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return false;
    };
    let living = world.get::<Health>(thing).is_some();
    if living && world.get::<Stat>(thing) == Some(&Stat::Dead) {
        return false;
    }
    let path = crate::construction::path_of(world, thing).unwrap_or_default();
    let is_crate = world.get::<DmCloset>(thing).is_some() && is_under(&path, CRATE);
    let anchored = world.get::<DmItem>(thing).is_some_and(|i| i.anchored)
        || world
            .get::<crate::obj_damage::Anchored>(thing)
            .is_some_and(|a| a.0)
        || (!living && !path.starts_with("/obj/"));
    if anchored {
        return false;
    }
    if !is_crate && !wire_cut(world, mob, WIRE_LOADCHECK) {
        buzz(world, mob, Mood::Sigh);
        return false;
    }
    if living {
        // `load_mob` -> `buckle_mob`.
        if world.get::<Riding>(thing).is_some() || world.get::<Bot>(thing).is_some() {
            return false;
        }
        world.entity_mut(thing).insert((pos, Riding(mob)));
    } else {
        // Nothing with someone inside (`locate(/mob) in atom_to_load`).
        let occupied = world
            .query::<&crate::closets::InCloset>()
            .iter(world)
            .any(|c| c.closet == thing);
        if occupied {
            buzz(world, mob, Mood::Sigh);
            return false;
        }
        if is_crate {
            crate::closets::close(world, thing);
        }
        put_inside(world, thing, mob);
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, HEAVY_DROP, 60.0, true),
        );
    }
    let _ = at;
    if let Some(mut m) = world.get_mut::<Mule>(mob) {
        m.load = Some(thing);
    }
    super::refresh_look(world, mob);
    true
}

/// tg `unload(dirn)`: the load onto the bot's tile, then one step in `dir`.
pub fn unload(world: &mut World, mob: Entity, dir: Option<Dir>) {
    let Some(load) = world.get::<Mule>(mob).and_then(|m| m.load) else {
        return;
    };
    if let Some(mut m) = world.get_mut::<Mule>(mob) {
        m.load = None;
    }
    if world.get::<Bot>(mob).is_some_and(|b| b.mode != BOT_IDLE) {
        set_mode(world, mob, BOT_IDLE);
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        super::refresh_look(world, mob);
        return;
    };
    if world.get_entity(load).is_err() {
        // A deleted load leaves only the overlay to clear.
        super::refresh_look(world, mob);
        return;
    }
    let riding = world.get::<Riding>(load).is_some();
    if riding {
        world.entity_mut(load).remove::<Riding>();
    } else if world.get::<StoredIn>(load).is_some_and(|s| s.0 == mob) {
        take_out(world, load, pos);
    }
    if let Some(dir) = dir {
        let to = pos.step(dir);
        let free = {
            let index = world.resource::<SpatialIndex>();
            index.in_bounds(to) && crate::mobs::ai::movement::can_step(index, load, pos, dir)
        };
        if free {
            world.entity_mut(load).insert(to);
        }
    }
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, HEAVY_DROP, 50.0, true),
    );
    super::refresh_look(world, mob);
}

/// tg `attack_hand` of a MULEbot: the wires with the panel open (not silicons); silicons get
/// nothing with the receive wire cut; otherwise the window. True when handled.
pub fn attack_hand(world: &mut World, client: Entity, user: Entity, mob: Entity) -> bool {
    let Some(bot) = world.get::<Bot>(mob) else {
        return false;
    };
    if bot.kind != BotKind::Mule {
        return false;
    }
    let silicon = crate::silicon::interact::has_silicon_access(world, user);
    if bot.cover & COVER_MAINTS_OPEN != 0 && !silicon {
        crate::wires::interact(world, client, user, mob);
        return true;
    }
    if silicon && wire_cut(world, mob, WIRE_RX) {
        return true;
    }
    open(world, client, user, mob);
    true
}

/// The `Mule` window (an emagged bot answers silicons only once they hacked it, as the base
/// bot's `attack_ai`).
pub fn open(world: &mut World, client: Entity, user: Entity, mob: Entity) {
    let Some(bot) = world.get::<Bot>(mob) else {
        return;
    };
    let silicon = crate::silicon::interact::has_silicon_access(world, user);
    if silicon && bot.emagged() && bot.cover & super::COVER_HACKED == 0 {
        let name = bot_name(world, mob);
        tell_mob(
            world,
            user,
            Span::Warning,
            format!("Интерфейс {name} не отвечает!"),
        );
        return;
    }
    let title = bot_name(world, mob);
    world.write_message(OpenUi::new::<MuleWindow>(client, user, mob, title));
}

/// `mulebot/item_interaction` and its `crowbar_act`, before the base bot's. `None`: not the
/// MULEbot's to handle.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    user: Entity,
    item: Entity,
    mob: Entity,
    combat: bool,
) -> Option<bool> {
    let bot = world.get::<Bot>(mob)?;
    if bot.kind != BotKind::Mule {
        return None;
    }
    let open = bot.cover & COVER_MAINTS_OPEN != 0;
    let name = bot_name(world, mob);
    if world.get::<ItemCell>(item).is_some() && open {
        if cell_of(world, mob).is_some() {
            tell(
                world,
                client,
                Span::Warning,
                format!("В {name} уже есть элемент питания!"),
            );
            return Some(true);
        }
        put_inside(world, item, mob);
        if let Some(mut m) = world.get_mut::<Mule>(mob) {
            m.cell = Some(item);
        }
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы вставляете элемент питания в {name}."),
        );
        return Some(true);
    }
    match crate::tools::tool_of(world, item).map(|t| t.0) {
        Some(ToolKind::Wirecutters | ToolKind::Multitool) if open => {
            attack_hand(world, client, user, mob);
            Some(true)
        }
        Some(ToolKind::Crowbar) if open && !combat => {
            let Some(cell) = cell_of(world, mob) else {
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!("В {name} нет элемента питания!"),
                );
                return Some(true);
            };
            if let Some(mut m) = world.get_mut::<Mule>(mob) {
                m.cell = None;
            }
            match world.get::<Inventory>(user).cloned() {
                Some(inventory) => crate::storage::put_in_hands(world, user, &inventory, cell),
                None => {
                    if let Some(&pos) = world.get::<TilePos>(mob) {
                        take_out(world, cell, pos);
                    }
                }
            }
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы вытаскиваете элемент питания из {name}."),
            );
            // `Exited(cell)`: it turns off.
            if world.get::<Bot>(mob).is_some_and(Bot::on) {
                super::turn_off(world, mob);
            }
            Some(true)
        }
        _ => None,
    }
}

/// `mouse_drop_receive` (from `crate::drag`): something dragged onto a MULEbot is loaded. True
/// when the target was a MULEbot.
pub fn mouse_drop_receive(world: &mut World, actor: Actor, over: Entity, dragged: Entity) -> bool {
    if world.get::<Mule>(over).is_none() {
        return false;
    }
    if world.get::<Health>(actor.mob).is_none()
        || world.get::<crate::ghost::Ghost>(dragged).is_some()
    {
        return true;
    }
    load(world, over, dragged);
    true
}

/// `on_cut` of `/datum/wires/mulebot`.
pub fn on_cut(world: &mut World, mob: Entity, wire: &str, source: Option<Entity>) {
    let name = bot_name(world, mob);
    let cut = |w: &str, world: &World| wire_cut(world, mob, w);
    let mut lines = Vec::new();
    match wire {
        WIRE_MOTOR1 | WIRE_MOTOR2 => {
            let (m1, m2) = (cut(WIRE_MOTOR1, world), cut(WIRE_MOTOR2, world));
            let speed = if m1 && m2 {
                lines.push(format!("Моторы {name} затихают."));
                None
            } else {
                Some(if m1 {
                    lines.push(format!("Моторы {name} ускоряются!"));
                    MOTOR_SPEEDS[0]
                } else if m2 {
                    lines.push(format!("Моторы {name} жужжат."));
                    MOTOR_SPEEDS[1]
                } else {
                    lines.push(format!("Моторы {name} мягко урчат."));
                    MOTOR_SPEEDS[2]
                })
            };
            set_speed(world, mob, speed);
        }
        WIRE_AVOIDANCE if source.is_some() => {
            lines.push(format!("Что-то внутри {name} зловеще щёлкает!"));
        }
        // `update_able_to_run`: without power the AI stops; the bot turns off.
        crate::wires::WIRE_POWER1 | crate::wires::WIRE_POWER2
            if !has_power(world, mob) && world.get::<Bot>(mob).is_some_and(Bot::on) =>
        {
            super::turn_off(world, mob);
        }
        _ => {}
    }
    super::refresh_look(world, mob);
    hear(world, mob, lines);
}

/// `on_pulse` of `/datum/wires/mulebot`: lights and noises, nothing changes.
pub fn on_pulse(world: &mut World, mob: Entity, wire: &str) {
    if cell_charge(world, mob).is_none_or(|c| c.charge <= 0.0) {
        return;
    }
    let text = match wire {
        crate::wires::WIRE_POWER1 | crate::wires::WIRE_POWER2 => "Индикатор заряда мигает.",
        WIRE_AVOIDANCE => "Внешние предупреждающие огни коротко вспыхивают.",
        WIRE_LOADCHECK => "Платформа загрузки лязгает.",
        WIRE_MOTOR1 | WIRE_MOTOR2 => "Ходовой мотор коротко взвывает.",
        _ => "Вы слышите треск радио.",
    };
    hear(world, mob, vec![text.to_owned()]);
}

fn hear(world: &mut World, mob: Entity, lines: Vec<String>) {
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    for text in lines {
        show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Local,
            (Span::Italics, text),
            &[],
        );
    }
}

/// `set_varspeed` (`None`: both motors cut, `TRAIT_IMMOBILIZED`).
fn set_speed(world: &mut World, mob: Entity, deciseconds: Option<f32>) {
    match deciseconds {
        Some(ds) => {
            let careful = world
                .get::<Mule>(mob)
                .is_some_and(|m| m.careful_until > world.resource::<Time>().elapsed_secs_f64());
            let delay = ds / 10.0 + if careful { CAREFUL_SLOWDOWN } else { 0.0 };
            match world.get_mut::<Mover>(mob) {
                Some(mut mover) => mover.delay = delay,
                None => {
                    world.entity_mut(mob).insert(Mover::new(delay));
                }
            }
        }
        None => {
            world.entity_mut(mob).remove::<Mover>();
        }
    }
}

/// The motor speed the wires give, deciseconds per tile (`None`: no motor).
fn motor_speed(world: &World, mob: Entity) -> Option<f32> {
    let (m1, m2) = (
        wire_cut(world, mob, WIRE_MOTOR1),
        wire_cut(world, mob, WIRE_MOTOR2),
    );
    match (m1, m2) {
        (true, true) => None,
        (true, false) => Some(MOTOR_SPEEDS[0]),
        (false, true) => Some(MOTOR_SPEEDS[1]),
        (false, false) => Some(MOTOR_SPEEDS[2]),
    }
}

/// `get_status` of the MULEbot's wires: none in tg (the window shows only the wires).
pub fn interactable(world: &World, mob: Entity) -> bool {
    world
        .get::<Bot>(mob)
        .is_some_and(|b| b.cover & COVER_MAINTS_OPEN != 0)
}

/// Steps of MULEbots and their riders: power, running over, careful driving, bumps.
// Cost: per tick, per `Moved` of a MULEbot or a rider a few lookups; bumps by MULEbots only.
fn mule_moves(
    world: &mut World,
    mut moves: Local<MessageCursor<Moved>>,
    mut bumps: Local<MessageCursor<Bumped>>,
) {
    let moved: Vec<Moved> = moves
        .read(world.resource::<Messages<Moved>>())
        .copied()
        .collect();
    let bumped: Vec<Bumped> = bumps
        .read(world.resource::<Messages<Bumped>>())
        .copied()
        .collect();
    let now = world.resource::<Time>().elapsed_secs_f64();
    for step in moved {
        if world.get::<Mule>(step.entity).is_some() {
            mule_stepped(world, step.entity, step.to, now);
        } else if let Some(&Riding(mule)) = world.get::<Riding>(step.entity)
            && world.get::<TilePos>(mule) != Some(&step.to)
        {
            // The rider moved off on its own (`relaymove` -> `unload`).
            if let Some(mut m) = world.get_mut::<Mule>(mule) {
                m.load = None;
            }
            world.entity_mut(step.entity).remove::<Riding>();
            set_mode(world, mule, BOT_IDLE);
            super::refresh_look(world, mule);
        }
    }
    for bump in bumped {
        if world.get::<Mule>(bump.entity).is_some() {
            bump_mob(world, bump.entity, bump.obstacle);
        }
    }
}

/// `on_bot_movement` + `on_pre_move`: the step's energy (off when it runs out), the rider
/// follows, lying humans on the new tile are run over.
fn mule_stepped(world: &mut World, mob: Entity, to: TilePos, now: f64) {
    let cell = cell_of(world, mob);
    let spent = cell
        .and_then(|c| world.get_mut::<ItemCell>(c))
        .is_some_and(|mut c| c.0.take(MOVE_ENERGY, false) > 0.0);
    if !spent || !has_power(world, mob) {
        super::turn_off(world, mob);
    }
    if let Some(load) = world.get::<Mule>(mob).and_then(|m| m.load)
        && world.get::<Riding>(load).is_some()
    {
        world.entity_mut(load).insert(to);
    }
    if world
        .get::<Mule>(mob)
        .is_some_and(|m| m.careful_until <= now)
    {
        set_speed(world, mob, motor_speed(world, mob));
    }
    if world.get::<crate::pull::PulledBy>(mob).is_some() {
        return;
    }
    let lying: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(to)
        .iter()
        .copied()
        .filter(|&e| e != mob)
        .filter(|&e| world.get::<Riding>(e).is_none())
        .filter(|&e| world.get::<Inventory>(e).is_some())
        .filter(|&e| {
            world
                .get::<BodyPosition>(e)
                .is_some_and(|b| b.lying_angle != 0)
        })
        .collect();
    for victim in lying {
        run_over(world, mob, victim, now);
    }
}

/// tg `run_over`.
pub fn run_over(world: &mut World, mob: Entity, victim: Entity, now: f64) {
    let reckless = world
        .get::<Bot>(mob)
        .is_some_and(|b| b.cover & COVER_EMAGGED != 0)
        || wire_cut(world, mob, WIRE_AVOIDANCE);
    let (name, who) = (bot_name(world, mob), bot_name(world, victim));
    let Some(&pos) = world.get::<TilePos>(victim) else {
        return;
    };
    if !reckless {
        let careful = world
            .get::<Mule>(mob)
            .is_some_and(|m| m.careful_until > now);
        if !careful {
            show(
                world,
                pos,
                DEFAULT_MESSAGE_RANGE,
                ChatChannel::Info,
                (
                    Span::Notice,
                    format!("{name} притормаживает, чтобы не раздавить {who}."),
                ),
                &[],
            );
        }
        if let Some(mut m) = world.get_mut::<Mule>(mob) {
            m.careful_until = now + CAREFUL_SECS;
        }
        set_speed(world, mob, motor_speed(world, mob));
        return;
    }
    show(
        world,
        pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (Span::Danger, format!("{name} переезжает {who}!")),
        &[(victim, Span::BoldWarning, format!("{name} переезжает вас!"))],
    );
    super::play(world, mob, SPLAT, 50.0);
    let damage = world
        .resource_mut::<crate::combat::CombatRng>()
        .range(5, 15) as f32;
    for (zone, weight) in RUN_OVER_ZONES {
        super::hurt_zone(world, victim, zone, DamageType::Brute, weight * damage);
    }
}

/// `run_over`'s `zone_damages`.
const RUN_OVER_ZONES: [(BodyZone, f32); 6] = [
    (BodyZone::Head, 2.0),
    (BodyZone::Chest, 2.0),
    (BodyZone::LLeg, 0.5),
    (BodyZone::RLeg, 0.5),
    (BodyZone::LArm, 0.5),
    (BodyZone::RArm, 0.5),
];

/// `MobBump`: with the avoidance wire cut a bumped human is knocked over (a cyborg only bumped).
fn bump_mob(world: &mut World, mob: Entity, other: Entity) {
    if !wire_cut(world, mob, WIRE_AVOIDANCE) || world.get::<Health>(other).is_none() {
        return;
    }
    if world.get::<Stat>(other).is_none_or(|s| *s == Stat::Dead) {
        return;
    }
    let (name, who) = (bot_name(world, mob), bot_name(world, other));
    let Some(&pos) = world.get::<TilePos>(other) else {
        return;
    };
    let text = if world.get::<crate::silicon::cyborg::Cyborg>(other).is_some() {
        format!("{name} врезается в {who}!")
    } else {
        crate::status_effects::knockdown(world, other, BUMP_KNOCKDOWN, 0.0);
        format!("{name} сбивает с ног {who}!")
    };
    show(
        world,
        pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (Span::Danger, text),
        &[],
    );
}

/// `GLOB.deliverybeacontags`: every delivery beacon's tag, in a stable order.
pub fn delivery_tags(world: &World) -> Vec<String> {
    let mut tags: Vec<String> = world
        .resource::<NavBeacons>()
        .0
        .values()
        .filter(|b| b.delivery)
        .map(|b| b.location.clone())
        .collect();
    tags.sort();
    tags.dedup();
    tags
}

/// `find_delivery_beacon`: the delivery beacon with `tag`.
pub fn delivery_beacon(world: &World, tag: &str) -> Option<Entity> {
    world
        .resource::<NavBeacons>()
        .0
        .values()
        .filter(|b| b.delivery && b.location == tag)
        .min_by_key(|b| b.entity.to_bits())
        .map(|b| b.entity)
}

/// The direction a beacon loads and unloads (`codes["dir"]`, else its own `dir`).
pub fn delivery_dir(world: &World, beacon: Entity) -> Option<Dir> {
    let from_codes = world
        .resource::<NavBeacons>()
        .0
        .get(&beacon)
        .and_then(|b| b.dir);
    match from_codes {
        Some(bits) => Dir::from_bits(bits),
        None => world.get::<Facing>(beacon).map(|f| f.0),
    }
}

/// `handle_delivery/perform` at `beacon` (the AI leaf, [`super::ai`]): unload or pick up, then
/// home or idle. Returns whether it heads home.
pub fn handle_delivery(world: &mut World, mob: Entity, beacon: Entity) -> bool {
    let Some(mule) = world.get::<Mule>(mob).cloned() else {
        return false;
    };
    let location = world
        .resource::<NavBeacons>()
        .0
        .get(&beacon)
        .map(|b| b.location.clone())
        .unwrap_or_default();
    let dir = delivery_dir(world, beacon);
    if let Some(load) = mule.load {
        // tg unloads only when reporting (a bug: without reports nothing is ever unloaded).
        if mule.flags & REPORT_DELIVERY_MODE != 0 {
            buzz(world, mob, Mood::Chime);
            let what = bot_name(world, load);
            super::speak(
                world,
                mob,
                &format!("Пункт назначения {location} достигнут. Выгружаю {what}."),
                true,
            );
        }
        unload(world, mob, dir);
    } else if mule.flags & AUTO_PICKUP_MODE != 0
        && let (Some(dir), Some(&pos)) = (dir, world.get::<TilePos>(mob))
    {
        let hacked = wire_cut(world, mob, WIRE_LOADCHECK);
        let next = pos.step(dir);
        let pick = world
            .resource::<SpatialIndex>()
            .on_tile(next)
            .iter()
            .copied()
            .filter(|&e| e != mob)
            .find(|&e| {
                let path = crate::construction::path_of(world, e).unwrap_or_default();
                if hacked {
                    !path.is_empty()
                        && world
                            .get::<crate::obj_damage::Anchored>(e)
                            .is_none_or(|a| !a.0)
                } else {
                    world.get::<DmCloset>(e).is_some() && is_under(&path, CRATE)
                }
            });
        if let Some(thing) = pick
            && load(world, mob, thing)
            && mule.flags & REPORT_DELIVERY_MODE != 0
        {
            let (what, area) = (bot_name(world, thing), super::area_name(world, pos));
            super::speak(world, mob, &format!("Загружаю {what} в {area}."), true);
        }
    }
    let home = !mule.home.is_empty() && mule.home != location && mule.flags & RETURN_MODE != 0;
    if home {
        set_mode(world, mob, BOT_GO_HOME);
    } else {
        reset_mule(world, mob);
    }
    home
}

/// The MULEbot's part of `bot_reset`: idle, no destination (`reset_keys`).
pub fn reset_mule(world: &mut World, mob: Entity) {
    if let Some(mut m) = world.get_mut::<Mule>(mob) {
        m.destination = None;
    }
    set_mode(world, mob, BOT_IDLE);
}

// The window.

/// The server half of `Mule`.
pub struct MuleWindow;

impl UiInterface for MuleWindow {
    const NAME: &'static str = MuleUi::NAME;
    type Data = MuleData;
    type StaticData = ();
}

#[derive(Clone, Debug, PartialEq)]
pub enum MuleAct {
    Power,
    Stop,
    Go,
    Home,
    Destination(String),
    SetId,
    SetHome,
    Unload,
    AutoReturn,
    AutoPickup,
    Report,
    Rename,
}

/// What `ui_data` reads.
#[derive(bevy::ecs::query::QueryData)]
pub struct MuleSource {
    bot: &'static Bot,
    mule: &'static Mule,
}

impl ServerUi for MuleWindow {
    type Source = MuleSource;
    type UserParam = (
        crate::silicon::interact::SiliconUsers<'static, 'static>,
        Query<'static, 'static, &'static ItemCell>,
        Res<'static, NavBeacons>,
        Query<'static, 'static, &'static Name>,
        Query<'static, 'static, &'static ss13_mapload::DmName>,
    );
    type Act = MuleAct;

    fn static_data(_: MuleSourceItem<'_, '_>) {}

    fn data(
        s: MuleSourceItem<'_, '_>,
        user: Entity,
        (silicons, cells, beacons, names, dm_names): &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> MuleData {
        let cell = s.mule.cell.and_then(|c| cells.get(c).ok()).map(|c| c.0);
        let load_name = s.mule.load.map(|l| {
            names
                .get(l)
                .map(|n| n.as_str().to_owned())
                .or_else(|_| {
                    dm_names
                        .get(l)
                        .map(|n| crate::vending::plain_name(&n.name).to_owned())
                })
                .unwrap_or_default()
        });
        let mut tags: Vec<String> = beacons
            .0
            .values()
            .filter(|b| b.delivery)
            .map(|b| b.location.clone())
            .collect();
        tags.sort();
        tags.dedup();
        mule_data(
            s.bot,
            s.mule,
            silicons.contains(user),
            cell,
            load_name,
            tags,
        )
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<MuleAct, UiReject> {
        Ok(match action {
            // `Mule.tsx` sends "on" (the base bot's "power" is accepted too).
            "on" | "power" => MuleAct::Power,
            "stop" => MuleAct::Stop,
            "go" => MuleAct::Go,
            "home" => MuleAct::Home,
            "destination" => MuleAct::Destination(
                params
                    .get("value")
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| UiReject::BadParams("value: expected a tag".into()))?
                    .to_owned(),
            ),
            "setid" => MuleAct::SetId,
            "sethome" => MuleAct::SetHome,
            "unload" => MuleAct::Unload,
            // `Mule.tsx` sends "autored" for the DM's "autoret".
            "autoret" | "autored" => MuleAct::AutoReturn,
            "autopick" => MuleAct::AutoPickup,
            "report" => MuleAct::Report,
            "rename" => MuleAct::Rename,
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

/// `mulebot/ui_data`.
pub fn mule_data(
    bot: &Bot,
    mule: &Mule,
    silicon: bool,
    cell: Option<crate::power::Cell>,
    load_name: Option<String>,
    destinations: Vec<String>,
) -> MuleData {
    let status = match bot.mode {
        BOT_IDLE | BOT_DELIVER | BOT_GO_HOME => "good",
        BOT_BLOCKED => "average",
        _ => "",
    };
    MuleData {
        power: bot.on(),
        cell_percent: cell.map(|c| c.percent() as f32),
        load_name,
        locked: bot.locked(),
        silicon,
        mode: bot.mode.to_owned(),
        mode_status: status.to_owned(),
        auto_return: mule.flags & RETURN_MODE != 0,
        auto_pickup: mule.flags & AUTO_PICKUP_MODE != 0,
        report_delivery: mule.flags & REPORT_DELIVERY_MODE != 0,
        destination: mule.destination.clone(),
        destinations,
        home: (!mule.home.is_empty()).then(|| mule.home.clone()),
        bot_id: mule.id.clone(),
    }
}

fn mule_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<MuleWindow>>>) {
    let pending: Vec<(Entity, Entity, MuleAct)> = acts
        .read(world.resource::<Messages<UiActed<MuleWindow>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (user, mob, act) in pending {
        act_on(world, user, mob, act);
    }
}

/// `mulebot/ui_act` -> `bot_control`, checked at the moment of effect: the user's access
/// (`allowed`), and the controls unlocked unless a silicon acts.
pub fn act_on(world: &mut World, user: Entity, mob: Entity, act: MuleAct) {
    let Some(bot) = world.get::<Bot>(mob) else {
        return;
    };
    let silicon = crate::silicon::interact::has_silicon_access(world, user);
    if !super::allowed(world, mob, user) {
        tell_mob(world, user, Span::Warning, "В доступе отказано!");
        return;
    }
    if bot.cover & COVER_LOCKED != 0 && !silicon {
        return;
    }
    let mode = bot.mode;
    let Some(mule) = world.get::<Mule>(mob).cloned() else {
        return;
    };
    match act {
        MuleAct::Power => {
            if world.get::<Bot>(mob).is_some_and(Bot::on) {
                super::turn_off(world, mob);
            } else if can_turn_on(world, mob, Some(user)) {
                super::turn_on(world, mob);
            }
        }
        MuleAct::Stop => {
            if mode != BOT_IDLE {
                super::bot_reset(world, mob);
            }
        }
        MuleAct::Go => {
            if mode == BOT_IDLE && world.get::<Bot>(mob).is_some_and(Bot::on) {
                start(world, mob);
            }
        }
        MuleAct::Home => {
            if mode == BOT_IDLE || mode == BOT_DELIVER {
                start_home(world, mob);
            }
        }
        MuleAct::Destination(tag) => {
            if delivery_tags(world).contains(&tag)
                && let Some(mut m) = world.get_mut::<Mule>(mob)
            {
                m.destination = Some(tag);
            }
        }
        MuleAct::Unload => {
            if mule.load.is_some() && mode != super::BOT_HUNT {
                unload(world, mob, None);
            }
        }
        MuleAct::AutoReturn => toggle(world, mob, RETURN_MODE),
        MuleAct::AutoPickup => toggle(world, mob, AUTO_PICKUP_MODE),
        MuleAct::Report => toggle(world, mob, REPORT_DELIVERY_MODE),
        MuleAct::SetId | MuleAct::SetHome | MuleAct::Rename => ask(world, user, mob, act, &mule),
    }
}

fn toggle(world: &mut World, mob: Entity, flag: u32) {
    if let Some(mut m) = world.get_mut::<Mule>(mob) {
        m.flags ^= flag;
    }
}

/// `start()`: towards the destination, or home when that is home.
pub fn start(world: &mut World, mob: Entity) {
    let Some(mule) = world.get::<Mule>(mob) else {
        return;
    };
    let mode = if mule.destination.as_deref() == Some(mule.home.as_str()) {
        BOT_GO_HOME
    } else {
        BOT_DELIVER
    };
    set_mode(world, mob, mode);
}

/// `start_home()`.
pub fn start_home(world: &mut World, mob: Entity) {
    if let Some(mut m) = world.get_mut::<Mule>(mob) {
        m.destination = Some(m.home.clone());
    }
    set_mode(world, mob, BOT_GO_HOME);
}

/// What a MULEbot prompt was for.
#[derive(Component, Clone, Copy, Debug)]
struct MulePrompt {
    mob: Entity,
    act: PromptFor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PromptFor {
    Id,
    Home,
    Rename,
}

/// `tgui_input_text`/`tgui_input_list` of "setid", "sethome" and "rename".
fn ask(world: &mut World, user: Entity, mob: Entity, act: MuleAct, mule: &Mule) {
    let Some(client) = world
        .get::<crate::server::PlayerMob>(user)
        .map(|p| p.client)
    else {
        return;
    };
    let (what, ask) = match act {
        MuleAct::SetId => (
            PromptFor::Id,
            Ask::new(
                client,
                user,
                "ID Assignment",
                "Enter ID",
                PromptKind::Text {
                    default: mule.id.clone(),
                    max_length: super::MAX_NAME_LEN,
                    multiline: false,
                },
            ),
        ),
        MuleAct::SetHome => (
            PromptFor::Home,
            Ask::new(
                client,
                user,
                "Mulebot Settings",
                "Enter Home",
                PromptKind::List {
                    items: delivery_tags(world),
                    default: (!mule.home.is_empty()).then(|| mule.home.clone()),
                },
            ),
        ),
        _ => (
            PromptFor::Rename,
            super::rename_ask(world, client, user, mob),
        ),
    };
    let prompt = ask_world(world, ask);
    world
        .entity_mut(prompt)
        .insert(MulePrompt { mob, act: what });
}

/// The answers to [`ask`], revalidated: the bot is still there, the user may still use it.
fn mule_prompts(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, user, answer) in answers {
        let Some(MulePrompt { mob, act }) = world.get::<MulePrompt>(prompt).copied() else {
            continue;
        };
        let Some(answer) = answer else {
            continue;
        };
        if world.get::<Mule>(mob).is_none() || !super::allowed(world, mob, user) {
            continue;
        }
        let silicon = crate::silicon::interact::has_silicon_access(world, user);
        let near = silicon || crate::storage::in_reach(world, user, mob);
        if !near || (world.get::<Bot>(mob).is_some_and(Bot::locked) && !silicon) {
            continue;
        }
        match (act, answer) {
            (PromptFor::Id, PromptAnswer::Text(id)) => {
                let id = super::sanitize_name(&id);
                if id.is_empty() {
                    continue;
                }
                // `name = "\improper MULEbot [new_id]"`.
                let base = default_name(world, mob);
                world
                    .entity_mut(mob)
                    .insert(Name::new(format!("{base} {id}")));
                if let Some(mut m) = world.get_mut::<Mule>(mob) {
                    m.id = id;
                }
            }
            (PromptFor::Home, PromptAnswer::Item(i)) => {
                if let Some(tag) = delivery_tags(world).get(i).cloned()
                    && let Some(mut m) = world.get_mut::<Mule>(mob)
                {
                    m.home = tag;
                }
            }
            (PromptFor::Rename, PromptAnswer::Text(name)) => super::rename(world, mob, &name),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mule_window_data_follows_the_bot() {
        let bot = Bot {
            kind: BotKind::Mule,
            mode: BOT_DELIVER,
            cover: COVER_LOCKED,
            ..Bot::default()
        };
        let mule = Mule {
            id: "AbC12".into(),
            flags: RETURN_MODE | REPORT_DELIVERY_MODE,
            home: "QM #1".into(),
            destination: Some("Medbay".into()),
            ..Mule::default()
        };
        let cell = crate::power::Cell {
            charge: 5_000.0,
            maxcharge: 25_000.0,
            chargerate: 0.0,
        };
        let data = mule_data(&bot, &mule, false, Some(cell), None, vec!["Medbay".into()]);
        assert_eq!(data.mode_status, "good");
        assert!(data.locked && !data.silicon && data.power);
        assert_eq!(data.cell_percent, Some(20.0));
        assert!(data.auto_return && !data.auto_pickup && data.report_delivery);
        assert_eq!(data.home.as_deref(), Some("QM #1"));
        assert_eq!(data.bot_id, "AbC12");
    }

    #[test]
    fn acts_parse_tg_and_tsx_names() {
        let params: Vec<(String, String)> = vec![("value".into(), "Medbay".into())];
        let p = UiParams::new(&params);
        assert_eq!(MuleWindow::parse_act("on", &p), Ok(MuleAct::Power));
        assert_eq!(
            MuleWindow::parse_act("autored", &p),
            Ok(MuleAct::AutoReturn)
        );
        assert_eq!(
            MuleWindow::parse_act("destination", &p),
            Ok(MuleAct::Destination("Medbay".into()))
        );
        let empty: Vec<(String, String)> = Vec::new();
        assert!(MuleWindow::parse_act("destination", &UiParams::new(&empty)).is_err());
        assert!(MuleWindow::parse_act("eject_pai", &p).is_err());
    }

    #[test]
    fn mule_wires_have_no_duds() {
        let set = wire_set();
        assert_eq!(set.wires.len(), 9);
        assert!(set.wires.iter().all(|w| !Wires::is_dud(w)));
        assert_eq!(crate::wires::colors_for(7, &set).len(), 9);
    }
}
