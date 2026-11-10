//! Prison labor (server): the labor camp's point claim console, collection unit and points
//! checker after tg `laborstacker.dm` (tgui `LaborClaimConsole`), the license plate press and
//! the produceporter after `prisonlabor.dm`.
//!
//! - The collection unit (`stacking_machine/laborstacker`) is a stacking machine
//!   ([`crate::mining::stacking`]) that adds `gulag_value * amount` points for every
//!   manufactured sheet it takes ([`crate::construction::Manufactured`]: ORM and lathe output);
//!   a sheet clicked on it is taken at once.
//! - The claim console links to a collection unit within 2 tiles. Its window shows the user's
//!   prisoner ID (`get_idcard(TRUE)`) status and points and the unit's points; "Claim points"
//!   moves them onto the ID; "Move shuttle" — with the goal met (or emagged), and only when the
//!   user is the only prisoner (a human without brig access) in the console's area — flies the
//!   `laborcamp` shuttle to `laborcamp_home`, paroles the user's record and tells security.
//! - The points checker says an inserted prisoner ID's points and goal.
//! - The plate press takes one blank license plate while working (10 points to the loader's
//!   prisoner ID); an empty hand presses it in 4 s (`do_after`) into a finished plate.
//! - Prison gates (`/obj/machinery/prisongate`, `ss13_world::PrisonGate`) let through what is
//!   not a carbon, except a structure with a living mob inside; a carbon only while unpowered,
//!   with brig access, pulled by someone with brig access, or — at a labor camp gate — wearing
//!   or holding a prisoner ID whose goal is met (a set goal not met refuses at once). They say
//!   why, at most every 3 s.
//! - The produceporter holds up to 10 grown items; a right click with an empty hand teleports
//!   them to random produce pads, 2 points each (4 above 50 potency) to the user's prisoner ID,
//!   and tells supply.
// shortcut: prison gates are decided on the server only (`ss13_world::GatePolicy`): the client
// predicts the step through and is corrected once; no timed sentences (`prisoner/timed`), so a
// station gate never lets a prisoner ID out; only worn and held IDs count (not backpacks); no
// flicker on power changes.
// shortcut: no claim console emag act (the flag is honoured), no takeoff abort when a second
// prisoner boards after the launch (`COMSIG_SHUTTLE_SHOULD_MOVE`), no plate press or
// produceporter power use, flick or wrench act, no produceporter shift-click emptying.
// Deviation: tg's `move_shuttle` trusts the window's `can_go_home`; here the goal is checked.

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::construct::DmObj;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{Areas, DmItem};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};
use ss13_world::SpatialIndex;

use super::prisoner::PrisonerCard;
use super::*;
use crate::health::Health;
use crate::inventory::{IdCard, IdCards, Inventory};
use crate::power::Unpowered;
use crate::radio::MachineRadio;
use crate::sound::{PlaySound, playsound};
use crate::tools::tell;

pub const LABOR_STACKER: &str = "/obj/machinery/mineral/stacking_machine/laborstacker";
pub const CLAIM_CONSOLE: &str = "/obj/machinery/mineral/labor_claim_console";
pub const POINTS_CHECKER: &str = "/obj/machinery/mineral/labor_points_checker";
pub const PLATE_PRESS: &str = "/obj/machinery/plate_press";
pub const BLANK_PLATE: &str = "/obj/item/stack/license_plates/empty";
pub const FILLED_PLATE: &str = "/obj/item/stack/license_plates/filled";
pub const PRODUCEPORTER: &str = "/obj/machinery/produceporter";
pub const PRODUCE_PAD: &str = "/obj/item/producepad";
pub const PRISON_GATE: &str = "/obj/machinery/prisongate";
pub const LABOUR_GATE: &str = "/obj/machinery/prisongate/labour";
/// `SPAM_CD`.
const GATE_COOLDOWN: f64 = 3.0;
const CHIME: &str = "sound/machines/chime.ogg";
const BUZZ: &str = "sound/machines/buzz/buzz-two.ogg";
const GROWN: &str = "/obj/item/food/grown";
/// `PRISON_LABOR_PLATE`, `PRISON_LABOR_CROPS`.
pub const PLATE_POINTS: u32 = 10;
pub const CROP_POINTS: u32 = 2;
/// `max_produce`.
pub const MAX_PRODUCE: usize = 10;
/// `do_after(user, 4 SECONDS)`.
const PRESS_DS: f32 = 40.0;
/// `dview(2, get_turf(src))` of `locate_stacking_machine`.
const STACKER_RANGE: i32 = 2;
/// `COOLDOWN_START(src, say_cooldown, 2 SECONDS)`.
const SAY_COOLDOWN: f64 = 2.0;
const ACCESS_BRIG: &str = "brig";
const SHUTTLE_ID: &str = "laborcamp";
const HOME_DOCK: &str = "laborcamp_home";
/// `/obj/item/stack/sheet` subtypes' `gulag_value`.
const GULAG_VALUES: [(&str, i64); 14] = [
    ("/obj/item/stack/sheet/plasmarglass", 5),
    ("/obj/item/stack/sheet/mineral/diamond", 100),
    ("/obj/item/stack/sheet/mineral/uranium", 50),
    ("/obj/item/stack/sheet/mineral/plasma", 5),
    ("/obj/item/stack/sheet/mineral/gold", 20),
    ("/obj/item/stack/sheet/mineral/silver", 10),
    ("/obj/item/stack/sheet/mineral/bananium", -5),
    ("/obj/item/stack/sheet/mineral/titanium", 10),
    ("/obj/item/stack/sheet/mineral/plastitanium", 20),
    ("/obj/item/stack/sheet/mineral/metal_hydrogen", 100),
    ("/obj/item/stack/sheet/mineral/zaukerite", 100),
    ("/obj/item/stack/sheet/bluespace_crystal", 100),
    ("/obj/item/stack/sheet/iron", 5),
    ("/obj/item/stack/sheet/plasteel", 10),
];
const KERCHUNK: &str = "sound/machines/kerchunk.ogg";
const COMPONENT_DROP: &str = "sound/items/handling/component_drop.ogg";
const EMITTER2: &str = "sound/items/weapons/emitter2.ogg";
const WOOSH: &str = "sound/machines/woosh.ogg";

/// `/obj/machinery/mineral/stacking_machine/laborstacker`: `points`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LaborStacker {
    pub points: i64,
}

/// `/obj/machinery/mineral/labor_claim_console`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ClaimConsole {
    pub stacker: Option<Entity>,
    pub emagged: bool,
    /// `say_cooldown`.
    pub say_ready_at: f64,
    /// The linked unit's points and whether the shuttle exists, for the window.
    pub unclaimed: i64,
    pub shuttle_exists: bool,
    /// `initiated_launch`: this console sent the shuttle; its takeoff is checked again.
    pub initiated_launch: bool,
}

/// `/obj/machinery/mineral/labor_points_checker`.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PointsChecker;

/// `/obj/machinery/plate_press`: `current_plate`, and who is pressing it.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct PlatePress {
    pub loaded: bool,
    pub pressing: Option<Entity>,
}

/// `/obj/machinery/produceporter`: the grown items in its bin.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Produceporter {
    pub produce: Vec<Entity>,
}

/// `/obj/machinery/prisongate`: the labor camp variant, `spam_cooldown_time`.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct PrisonGateState {
    pub labour: bool,
    pub ready_at: f64,
}

/// `/obj/item/producepad` (`GLOB.produce_locations`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ProducePad;

pub struct LaborPlugin;

impl Plugin for LaborPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // The claim console's window, the press's `attack_hand`, the produceporter's
        // `attack_hand_secondary`.
        app.add_click_handler(ClickPhase::EmptyHand, 2695, |w, c| {
            crate::brig::labor::interact(w, c.client, c.mob, c.target, c.modifiers.right)
        });
        // Blank plates, grown items, prisoner IDs and sheets onto the labor machines.
        app.add_click_handler(ClickPhase::HeldOnTarget, 4925, |w, c| {
            c.held.is_some_and(|h| {
                crate::brig::labor::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        app.insert_resource(ss13_world::GatePolicy(gate_allows));
        app.register_ui::<LaborClaimUi>()
            .add_observer(attach)
            .add_observer(attach_pad)
            .add_systems(
                Update,
                (
                    claim_acts.in_set(UiSystems::Handle),
                    refresh_consoles
                        .after(UiSystems::Handle)
                        .before(UiSystems::Send)
                        .run_if(any_with_component::<ClaimConsole>),
                    gate_messages
                        .after(ss13_world::WorldSystems::Movement)
                        .run_if(any_with_component::<PrisonGateState>),
                ),
            );
    }
}

fn attach(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(o) = objs.get(add.entity) else {
        return;
    };
    let e = add.entity;
    if is_under(&o.path, LABOR_STACKER) {
        commands.entity(e).insert(LaborStacker::default());
    } else if is_under(&o.path, CLAIM_CONSOLE) {
        commands.entity(e).insert(ClaimConsole::default());
    } else if is_under(&o.path, POINTS_CHECKER) {
        commands.entity(e).insert(PointsChecker);
    } else if is_under(&o.path, PLATE_PRESS) {
        commands.entity(e).insert(PlatePress::default());
        commands.queue(move |world: &mut World| press_look(world, e));
    } else if is_under(&o.path, PRISON_GATE) {
        commands.entity(e).insert((
            ss13_world::PrisonGate,
            PrisonGateState {
                labour: is_under(&o.path, LABOUR_GATE),
                ready_at: 0.0,
            },
        ));
    } else if is_under(&o.path, PRODUCEPORTER) {
        commands.entity(e).insert(Produceporter::default());
    }
}

fn attach_pad(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if items
        .get(add.entity)
        .is_ok_and(|i| is_under(&i.path, PRODUCE_PAD))
    {
        commands.entity(add.entity).insert(ProducePad);
    }
}

/// `gulag_value` of a sheet type.
pub fn gulag_value(path: &str) -> i64 {
    GULAG_VALUES
        .iter()
        .find(|(p, _)| is_under(path, p))
        .map_or(0, |&(_, v)| v)
}

/// `laborstacker/process_stack`: called by the stacking machine for every stack it takes.
pub fn stacker_points(world: &mut World, machine: Entity, item: Entity, path: &str, amount: u32) {
    if world.get::<LaborStacker>(machine).is_none()
        || world
            .get::<crate::construction::Manufactured>(item)
            .is_none()
    {
        return;
    }
    let value = gulag_value(path);
    if value != 0
        && let Some(mut s) = world.get_mut::<LaborStacker>(machine)
    {
        s.points += value * i64::from(amount);
    }
}

fn sound(world: &mut World, at: Entity, file: &str, volume: f32) {
    if let Some(&pos) = world.get::<TilePos>(at) {
        playsound(world, PlaySound::new(pos, file, volume, false));
    }
}

fn say(world: &mut World, e: Entity, text: &str) {
    crate::cargo::console::machine_say(world, e, text);
}

/// The prisoner ID `get_idcard(TRUE)` finds on `user`.
fn prisoner_card(world: &World, user: Entity) -> Option<Entity> {
    world
        .get::<Inventory>(user)
        .and_then(|i| i.id_card(world))
        .filter(|&c| world.get::<PrisonerCard>(c).is_some())
}

/// Adds labor points to `user`'s prisoner ID, telling them. False without one.
fn add_points(world: &mut World, client: Entity, user: Entity, points: u32, text: &str) -> bool {
    let Some(card) = prisoner_card(world, user) else {
        return false;
    };
    if let Some(mut c) = world.get_mut::<PrisonerCard>(card) {
        c.points += points;
    }
    tell(world, client, Span::Notice, text.to_owned());
    true
}

/// The held item out of `mob`'s hands (the caller puts it somewhere).
fn take_from_hands(world: &mut World, mob: Entity, item: Entity) {
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob)
        && let Some(hand) = inventory.hands.iter().position(|h| *h == Some(item))
    {
        inventory.hands[hand] = None;
    }
    world
        .entity_mut(item)
        .remove::<crate::inventory::CarriedBy>();
}

fn operational(world: &World, e: Entity) -> bool {
    crate::medical::pods::operational(world, e)
}

/// The empty-hand clicks of the labor machines. True when `target` is one of them.
pub fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    right: bool,
) -> bool {
    if world.get::<ClaimConsole>(target).is_some() {
        if !right && world.get::<Unpowered>(target).is_none() {
            let title = crate::examine::name_of(world, target);
            world.write_message(OpenUi::new::<LaborClaimUi>(client, mob, target, title));
        }
        return !right;
    }
    if let Some(press) = world.get::<PlatePress>(target).copied() {
        if right {
            return false;
        }
        let busy = press.pressing.is_some_and(|m| {
            world
                .get::<crate::tools::DoAfter>(m)
                .is_some_and(|d| d.target == target)
        });
        if !busy && press.loaded {
            work_press(world, client, mob, target);
        }
        return true;
    }
    if world.get::<Produceporter>(target).is_some() && right {
        send_produce(world, client, mob, target);
        return true;
    }
    false
}

/// The held-item clicks of the labor machines. True when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let path = world
        .get::<DmItem>(held)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    if let Some(press) = world.get::<PlatePress>(target).copied() {
        if !is_under(&path, BLANK_PLATE) {
            return false;
        }
        let name = crate::examine::name_of(world, target);
        if !operational(world, target) {
            tell(
                world,
                client,
                Span::Warning,
                format!("{name} должен быть включён, чтобы его загрузить!"),
            );
        } else if press.loaded {
            tell(
                world,
                client,
                Span::Warning,
                format!("В {name} уже есть номерной знак!"),
            );
        } else if crate::tools::use_stack(world, held, 1) {
            if let Some(mut p) = world.get_mut::<PlatePress>(target) {
                p.loaded = true;
            }
            press_look(world, target);
            // `post_press`.
            add_points(
                world,
                client,
                mob,
                PLATE_POINTS,
                &format!("Добавлено {PLATE_POINTS} очков!"),
            );
        }
        return true;
    }
    if world.get::<Produceporter>(target).is_some() {
        if !is_under(&path, GROWN) {
            return false;
        }
        let count = world
            .get::<Produceporter>(target)
            .map_or(0, |p| p.produce.len());
        if count >= MAX_PRODUCE {
            tell(world, client, Span::Warning, "Корзина полна!");
            return true;
        }
        take_from_hands(world, mob, held);
        crate::storage::put_inside(world, held, target);
        if let Some(mut p) = world.get_mut::<Produceporter>(target) {
            p.produce.push(held);
        }
        sound(world, target, COMPONENT_DROP, 30.0);
        tell(
            world,
            client,
            Span::Notice,
            format!("Внутри {} / {MAX_PRODUCE}", count + 1),
        );
        return true;
    }
    if world.get::<PointsChecker>(target).is_some() {
        let Some(card) = world.get::<PrisonerCard>(held).copied() else {
            return false;
        };
        if card.goal == 0 {
            say(world, target, "No goal required for this ID.");
            return true;
        }
        let name = world
            .get::<crate::outfit::IdRegistration>(held)
            .map(|r| r.registered_name.clone())
            .unwrap_or_default();
        say(world, target, &format!("ID: {name}."));
        say(
            world,
            target,
            &format!("Points Collected: {} / {}.", card.points, card.goal),
        );
        say(
            world,
            target,
            "Collect points by bringing smelted minerals to the Labor Shuttle stacking machine. \
             Reach your quota to earn your release.",
        );
        return true;
    }
    if world.get::<LaborStacker>(target).is_some() && crate::mining::stacking::accepted(world, held)
    {
        // `base_item_interaction`: the sheet is processed from the hand.
        take_from_hands(world, mob, held);
        if let Some(&pos) = world.get::<TilePos>(target) {
            crate::inventory::put_down(world, held, pos);
        }
        crate::mining::stacking::process_stack(world, target, held);
        return true;
    }
    false
}

/// `update_icon_state` of the press.
fn press_look(world: &mut World, e: Entity) {
    let Some(p) = world.get::<PlatePress>(e).copied() else {
        return;
    };
    let state = if !operational(world, e) {
        "offline"
    } else if p.pressing.is_some() {
        "loop"
    } else if p.loaded {
        "online_loaded"
    } else {
        "online"
    };
    crate::obj_damage::set_icon_state(world, e, state);
}

/// `work_press`: the 4 s press.
fn work_press(world: &mut World, client: Entity, mob: Entity, press: Entity) {
    let actor = crate::tools::Actor {
        client,
        mob,
        item: None,
    };
    if !crate::tools::do_after(
        world,
        actor,
        press,
        PRESS_DS,
        0,
        crate::construction::Job::PlatePress,
    ) {
        return;
    }
    if let Some(mut p) = world.get_mut::<PlatePress>(press) {
        p.pressing = Some(mob);
    }
    press_look(world, press);
    tell(
        world,
        client,
        Span::Notice,
        "Вы начинаете штамповать новый номерной знак!",
    );
    sound(world, press, KERCHUNK, 80.0);
}

/// The press's `do_after` succeeded: the plate is done if one is still loaded.
pub fn complete_press(world: &mut World, mob: Entity, press: Entity) {
    let Some(p) = world.get::<PlatePress>(press).copied() else {
        return;
    };
    if let Some(mut p) = world.get_mut::<PlatePress>(press) {
        p.pressing = None;
    }
    if !p.loaded || !operational(world, press) {
        press_look(world, press);
        return;
    }
    if let Some(mut p) = world.get_mut::<PlatePress>(press) {
        p.loaded = false;
    }
    press_look(world, press);
    if let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) {
        tell(
            world,
            client,
            Span::Notice,
            "Вы заканчиваете штамповку номерного знака!",
        );
    }
    if let Some(&pos) = world.get::<TilePos>(press) {
        crate::construction::spawn_stack(world, FILLED_PLATE, pos, 1);
    }
}

/// `labor_points` of one grown item: doubled above 50 potency.
pub fn crop_points(potency: f32) -> u32 {
    if potency > 50.0 {
        CROP_POINTS * 2
    } else {
        CROP_POINTS
    }
}

/// `attack_hand_secondary` of the produceporter: everything in the bin to random pads.
fn send_produce(world: &mut World, client: Entity, user: Entity, porter: Entity) {
    if !operational(world, porter) {
        tell(world, client, Span::Warning, "Ничего не происходит.");
        return;
    }
    let produce: Vec<Entity> = world
        .get::<Produceporter>(porter)
        .map(|p| p.produce.clone())
        .unwrap_or_default()
        .into_iter()
        .filter(|&i| {
            world
                .get::<crate::storage::StoredIn>(i)
                .is_some_and(|s| s.0 == porter)
        })
        .collect();
    if produce.is_empty() {
        if let Some(mut p) = world.get_mut::<Produceporter>(porter) {
            p.produce.clear();
        }
        tell(world, client, Span::Warning, "Корзина пуста!");
        return;
    }
    let pads: Vec<TilePos> = {
        let mut q = world.query_filtered::<&TilePos, With<ProducePad>>();
        q.iter(world).copied().collect()
    };
    if pads.is_empty() {
        tell(world, client, Span::Warning, "Нет точек выгрузки!");
        return;
    }
    sound(world, porter, EMITTER2, 50.0);
    let mut points = 0;
    for &item in &produce {
        let i = world
            .resource_mut::<crate::combat::CombatRng>()
            .range(0, pads.len() as u32 - 1) as usize;
        let Some(&dest) = pads.get(i) else {
            continue;
        };
        points += crop_points(
            world
                .get::<crate::botany::seeds::Seed>(item)
                .map_or(0.0, |s| s.potency),
        );
        crate::storage::take_out(world, item, dest);
        crate::effects::particles::start(
            world,
            crate::effects::particles::Kind::Sparks,
            5,
            true,
            dest,
        );
        playsound(world, PlaySound::new(dest, WOOSH, 60.0, true));
    }
    if let Some(mut p) = world.get_mut::<Produceporter>(porter) {
        p.produce.clear();
    }
    // `post_delivery`.
    if add_points(
        world,
        client,
        user,
        points,
        &format!("Добавлено {points} очков труда!"),
    ) && let Some(&pos) = world.get::<TilePos>(porter)
    {
        let area = crate::bots::area_name(world, pos);
        world.write_message(MachineRadio {
            freq: ss13_chat::radio::FREQ_SUPPLY,
            z: pos.z,
            name: crate::examine::name_of(world, porter),
            text: format!(
                "A shipment of produce ({} items) has been sent to {area} from the Gulag.",
                produce.len()
            ),
        });
    }
}

/// The linked unit's points, the link (`locate_stacking_machine`, retried until found) and
/// the shuttle, for the window.
fn refresh_consoles(world: &mut World) {
    let mut q = world.query::<(Entity, &ClaimConsole)>();
    let consoles: Vec<(Entity, Option<Entity>)> =
        q.iter(world).map(|(e, c)| (e, c.stacker)).collect();
    let shuttle_exists = crate::shuttle_nav::get_shuttle(world, SHUTTLE_ID).is_some();
    for (console, stacker) in consoles {
        let stacker = stacker
            .filter(|&s| world.get::<LaborStacker>(s).is_some())
            .or_else(|| find_stacker(world, console));
        let unclaimed = stacker
            .and_then(|s| world.get::<LaborStacker>(s))
            .map_or(0, |s| s.points);
        if let Some(mut c) = world.get_mut::<ClaimConsole>(console)
            && (c.stacker != stacker
                || c.unclaimed != unclaimed
                || c.shuttle_exists != shuttle_exists)
        {
            c.stacker = stacker;
            c.unclaimed = unclaimed;
            c.shuttle_exists = shuttle_exists;
        }
    }
}

fn find_stacker(world: &World, console: Entity) -> Option<Entity> {
    let &pos = world.get::<TilePos>(console)?;
    let index = world.resource::<SpatialIndex>();
    for dy in -STACKER_RANGE..=STACKER_RANGE {
        for dx in -STACKER_RANGE..=STACKER_RANGE {
            let at = TilePos::new(pos.x + dx, pos.y + dy, pos.z);
            if let Some(&e) = index
                .on_tile(at)
                .iter()
                .find(|&&e| world.get::<LaborStacker>(e).is_some())
            {
                return Some(e);
            }
        }
    }
    None
}

/// `can_go_home` and `status_info` for a prisoner ID (`None`: no prisoner ID).
pub fn status(card: Option<&PrisonerCard>, emagged: bool) -> (bool, String) {
    match card {
        None => (emagged, "No Prisoner ID detected.".to_owned()),
        Some(c) if c.goal == 0 => (emagged, "No goal set!".to_owned()),
        Some(c) if c.points >= c.goal => (true, "Goal met!".to_owned()),
        Some(c) => (
            emagged,
            format!("You are {} points away.", c.goal - c.points),
        ),
    }
}

type ClaimUser = (
    Query<'static, 'static, &'static Inventory>,
    IdCards<'static, 'static>,
    Query<'static, 'static, &'static PrisonerCard>,
);

impl ServerUi for LaborClaimUi {
    type Source = &'static ClaimConsole;
    type UserParam = ClaimUser;
    type Act = ClaimAct;

    fn static_data(_: &ClaimConsole) {}

    fn data(
        console: &ClaimConsole,
        user: Entity,
        (inventories, cards, prisoner_cards): &SystemParamItem<'_, '_, ClaimUser>,
    ) -> LaborClaimData {
        let card = inventories
            .get(user)
            .ok()
            .and_then(|i| i.id_search().find_map(|e| cards.id_of(e)))
            .and_then(|c| prisoner_cards.get(c).ok());
        let (can_go_home, status_info) = status(card, console.emagged);
        LaborClaimData {
            can_go_home,
            id_points: card.map_or(0, |c| c.points),
            status_info,
            unclaimed_points: console.unclaimed,
            shuttle_exists: console.shuttle_exists,
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<ClaimAct, UiReject> {
        match action {
            CLAIM_POINTS => Ok(ClaimAct::Claim),
            MOVE_SHUTTLE => Ok(ClaimAct::MoveShuttle),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// A parsed `LaborClaimConsole` act.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClaimAct {
    Claim,
    MoveShuttle,
}

fn claim_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<LaborClaimUi>>>) {
    let pending: Vec<(Entity, Entity, ClaimAct)> = acts
        .read(world.resource::<Messages<UiActed<LaborClaimUi>>>())
        .map(|a| (a.actor, a.source, a.act))
        .collect();
    for (user, console, act) in pending {
        claim_act(world, user, console, act);
    }
}

/// `say` behind `say_cooldown`.
fn say_cooled(world: &mut World, console: Entity, text: &str) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(mut c) = world.get_mut::<ClaimConsole>(console) else {
        return;
    };
    if c.say_ready_at > now {
        return;
    }
    c.say_ready_at = now + SAY_COOLDOWN;
    say(world, console, text);
}

/// `on_laborshuttle_can_move` (`COMSIG_SHUTTLE_SHOULD_MOVE`) when the `laborcamp` shuttle
/// `port` is about to leave: a console that launched it aborts the takeoff if more than one
/// prisoner is aboard by now. Each launch is checked once (tg never clears `initiated_launch`
/// after a takeoff, so later flights would be checked too).
pub fn takeoff_blocked(world: &mut World, port: Entity) -> bool {
    let labor = world
        .get::<crate::shuttle::MobileDock>(port)
        .is_some_and(|m| m.id == SHUTTLE_ID);
    if !labor {
        return false;
    }
    let mut q = world.query::<(Entity, &ClaimConsole)>();
    let launched: Vec<Entity> = q
        .iter(world)
        .filter(|(_, c)| c.initiated_launch)
        .map(|(e, _)| e)
        .collect();
    let mut blocked = false;
    for console in launched {
        if let Some(mut c) = world.get_mut::<ClaimConsole>(console) {
            c.initiated_launch = false;
        }
        if shuttle_prisoners(world, console).len() > 1 {
            say(
                world,
                console,
                "Takeoff aborted. Prisoners may only be released one at a time.",
            );
            blocked = true;
        }
    }
    blocked
}

fn has_brig_access(world: &World, mob: Entity) -> bool {
    world
        .get::<Inventory>(mob)
        .and_then(|i| i.id_card(world))
        .and_then(|c| world.get::<IdCard>(c))
        .is_some_and(|c| c.access.iter().any(|a| a == ACCESS_BRIG))
}

/// `find_labor_shuttle_mobs`: humans in the console's area without brig access.
// Cost: one pass over the mobs with health, only on a `move_shuttle` act.
fn shuttle_prisoners(world: &mut World, console: Entity) -> Vec<Entity> {
    let Some(&pos) = world.get::<TilePos>(console) else {
        return Vec::new();
    };
    let area = world.get_resource::<Areas>().and_then(|a| a.area_at(pos));
    let mut q = world.query_filtered::<(Entity, &TilePos), (With<Health>, With<Inventory>)>();
    let mobs: Vec<(Entity, TilePos)> = q.iter(world).map(|(e, p)| (e, *p)).collect();
    mobs.into_iter()
        .filter(|&(m, p)| {
            area.is_some()
                && world.get_resource::<Areas>().and_then(|a| a.area_at(p)) == area
                && world.get::<crate::medical::wounds::Limbs>(m).is_some()
                && !has_brig_access(world, m)
        })
        .map(|(m, _)| m)
        .collect()
}

/// `labor_claim_console/ui_act` at the moment of effect.
pub fn claim_act(world: &mut World, user: Entity, console: Entity, act: ClaimAct) {
    let Some(state) = world.get::<ClaimConsole>(console).cloned() else {
        return;
    };
    if world.get::<Unpowered>(console).is_some() {
        return;
    }
    match act {
        ClaimAct::Claim => {
            let stacker = state
                .stacker
                .filter(|&s| world.get::<LaborStacker>(s).is_some());
            let (Some(card), Some(stacker)) = (prisoner_card(world, user), stacker) else {
                say_cooled(world, console, "No valid id for point transfer detected.");
                return;
            };
            let points = world.get::<LaborStacker>(stacker).map_or(0, |s| s.points);
            if let Some(mut c) = world.get_mut::<PrisonerCard>(card) {
                c.points = (i64::from(c.points) + points).max(0) as u32;
            }
            if let Some(mut s) = world.get_mut::<LaborStacker>(stacker) {
                s.points = 0;
            }
            say(world, console, "Points transferred.");
        }
        ClaimAct::MoveShuttle => {
            let card = prisoner_card(world, user).and_then(|c| world.get::<PrisonerCard>(c));
            if !status(card, state.emagged).0 {
                return;
            }
            let Some(port) = crate::shuttle_nav::get_shuttle(world, SHUTTLE_ID) else {
                say_cooled(world, console, "Shuttle not found.");
                return;
            };
            let prisoners = shuttle_prisoners(world, console);
            if prisoners != [user] {
                say_cooled(
                    world,
                    console,
                    "Prisoners may only be released one at a time.",
                );
                return;
            }
            let home = {
                let mut q = world.query::<(Entity, &crate::shuttle::StationaryDock)>();
                q.iter(world)
                    .find(|(_, d)| d.id == HOME_DOCK)
                    .map(|(e, _)| e)
            };
            let Some(home) = home else {
                say_cooled(world, console, "Shuttle not found.");
                return;
            };
            let docked = world
                .get::<crate::shuttle::MobileDock>(port)
                .and_then(|m| m.docked);
            if docked == Some(home) {
                say_cooled(world, console, "Shuttle already at station.");
                return;
            }
            if !crate::shuttle_nav::request(world, port, home) {
                say_cooled(world, console, "No permission to dock could be granted.");
                return;
            }
            if let Some(mut c) = world.get_mut::<ClaimConsole>(console) {
                c.initiated_launch = true;
            }
            let name = world
                .get::<Name>(user)
                .map(|n| n.to_string())
                .unwrap_or_else(|| crate::examine::name_of(world, user));
            if !state.emagged {
                if let Some(mut manifest) =
                    world.get_resource_mut::<crate::records::manifest::Manifest>()
                    && let Some(r) = manifest.general.iter_mut().find(|r| r.name == name)
                {
                    r.wanted_status = crate::records::WANTED_PAROLE.to_owned();
                }
                if let Some(&pos) = world.get::<TilePos>(console) {
                    world.write_message(MachineRadio {
                        freq: ss13_chat::radio::FREQ_SECURITY,
                        z: pos.z,
                        name: crate::examine::name_of(world, console),
                        text: format!(
                            "{name} returned to the station. Minerals and Prisoner ID card \
                             ready for retrieval."
                        ),
                    });
                }
            }
            say(
                world,
                console,
                "Labor sentence finished, shuttle returning.",
            );
        }
    }
}

/// What a gate decides for a mover (`CanAllowThrough`, `allow_prisoner_id`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateVerdict {
    /// Not a carbon, or the gate is off.
    Free,
    Stowaway,
    Clearance,
    Escort,
    /// `ALLOW_EXIT` of a prisoner ID.
    Served,
    /// `BLOCK_EXIT`: points left on the sentence.
    Remaining(u32),
    /// A brig gate and a timed prisoner ID (`crate::brig::sentence`): the sentence starts, has
    /// been served (both pass), or is still running.
    SentenceStart(Entity),
    SentenceServed(Entity),
    SentenceOngoing,
    Denied,
}

impl GateVerdict {
    pub fn passes(self) -> bool {
        matches!(
            self,
            Self::Free
                | Self::Clearance
                | Self::Escort
                | Self::Served
                | Self::SentenceStart(_)
                | Self::SentenceServed(_)
        )
    }
}

fn is_carbon(world: &World, e: Entity) -> bool {
    world.get::<Health>(e).is_some()
        && world.get::<Inventory>(e).is_some()
        && world.get::<crate::silicon::Silicon>(e).is_none()
        && world.get::<crate::mobs::BasicMob>(e).is_none()
}

/// `prisongate/CanAllowThrough` of `gate` for `mover`.
pub fn gate_verdict(world: &World, mover: Entity, gate: Entity) -> GateVerdict {
    if !is_carbon(world, mover) {
        let structure = world
            .get::<DmObj>(mover)
            .is_some_and(|o| is_under(&o.path, "/obj/structure"));
        let stowaway = structure
            && world
                .try_query::<(&crate::closets::InCloset, &Health)>()
                .is_some_and(|mut q| q.iter(world).any(|(c, _)| c.closet == mover));
        return if stowaway {
            GateVerdict::Stowaway
        } else {
            GateVerdict::Free
        };
    }
    if world.get::<Unpowered>(gate).is_some() {
        return GateVerdict::Free;
    }
    if has_brig_access(world, mover) {
        return GateVerdict::Clearance;
    }
    if world
        .get::<crate::pull::PulledBy>(mover)
        .is_some_and(|p| has_brig_access(world, p.puller()))
    {
        return GateVerdict::Escort;
    }
    let labour = world.get::<PrisonGateState>(gate).is_some_and(|g| g.labour);
    let cards = world
        .get::<Inventory>(mover)
        .map(|i| {
            [i.hands[0], i.hands[1], i.id]
                .into_iter()
                .flatten()
                .chain(i.equipped())
                .filter_map(|e| Some((e, world.get::<PrisonerCard>(e).copied()?)))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let now_ms = super::sentence::now_ms(world);
    for (entity, card) in cards {
        if !labour {
            // `prisongate/allow_prisoner_id`: the timed sentence.
            use super::sentence::SentenceVerdict as V;
            match super::sentence::gate_check(&card, now_ms) {
                None => continue,
                Some(V::Start) => return GateVerdict::SentenceStart(entity),
                Some(V::Served) => return GateVerdict::SentenceServed(entity),
                Some(V::Ongoing) => return GateVerdict::SentenceOngoing,
            }
        }
        // `labour/allow_prisoner_id`.
        if card.goal == 0 {
            continue;
        }
        return if card.points >= card.goal {
            GateVerdict::Served
        } else {
            GateVerdict::Remaining(card.goal - card.points)
        };
    }
    GateVerdict::Denied
}

/// [`ss13_world::GatePolicy`].
fn gate_allows(world: &World, mover: Entity, gate: Entity) -> bool {
    gate_verdict(world, mover, gate).passes()
}

/// The gates' words: refusals from `Bumped`, grants from steps onto a gate.
fn gate_messages(
    world: &mut World,
    mut bumps: Local<MessageCursor<ss13_world::Bumped>>,
    mut moves: Local<MessageCursor<ss13_world::Moved>>,
) {
    let mut events: Vec<(Entity, Entity)> = bumps
        .read(world.resource::<Messages<ss13_world::Bumped>>())
        .filter(|b| world.get::<PrisonGateState>(b.obstacle).is_some())
        .map(|b| (b.entity, b.obstacle))
        .collect();
    let moved: Vec<(Entity, TilePos)> = moves
        .read(world.resource::<Messages<ss13_world::Moved>>())
        .map(|m| (m.entity, m.to))
        .collect();
    for (mover, to) in moved {
        if let Some(&gate) = world
            .resource::<SpatialIndex>()
            .on_tile(to)
            .iter()
            .find(|&&g| world.get::<PrisonGateState>(g).is_some())
        {
            events.push((mover, gate));
        }
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    for (mover, gate) in events {
        let verdict = gate_verdict(world, mover, gate);
        // A timed ID's start and release happen on every pass, past the spam cooldown.
        match verdict {
            GateVerdict::SentenceStart(card) => {
                let name = world
                    .get::<crate::outfit::IdRegistration>(card)
                    .map(|r| r.registered_name.clone())
                    .unwrap_or_else(|| crate::examine::name_of(world, card));
                say(
                    world,
                    gate,
                    &format!(
                        "Prison ID with active sentence detected. Please enjoy your stay in our \
                         corporate rehabilitation center, {name}!"
                    ),
                );
                sound(world, gate, CHIME, 50.0);
                super::sentence::start(world, card);
                continue;
            }
            GateVerdict::SentenceServed(card) => {
                say(
                    world,
                    gate,
                    "Prison ID with served sentence detected. Access granted.",
                );
                sound(world, gate, CHIME, 50.0);
                if let Some(mut c) = world.get_mut::<PrisonerCard>(card) {
                    c.timed = false;
                }
                continue;
            }
            _ => {}
        }
        let ready = world
            .get::<PrisonGateState>(gate)
            .is_some_and(|g| g.ready_at <= now);
        if !ready || verdict == GateVerdict::Free {
            continue;
        }
        if let Some(mut g) = world.get_mut::<PrisonGateState>(gate) {
            g.ready_at = now + GATE_COOLDOWN;
        }
        let (text, file) = match verdict {
            GateVerdict::Stowaway => (
                "Stowaway detected in internal contents. Access denied.".to_owned(),
                Some(BUZZ),
            ),
            GateVerdict::Clearance => (
                "Brig clearance detected. Access granted.".to_owned(),
                Some(CHIME),
            ),
            GateVerdict::Escort => (
                "Brig escort detected. Access granted.".to_owned(),
                Some(CHIME),
            ),
            GateVerdict::Served => ("All points served. Access granted.".to_owned(), Some(CHIME)),
            GateVerdict::Remaining(n) => (
                format!("{n} point(s) remaining on sentence. Access denied."),
                Some(BUZZ),
            ),
            GateVerdict::SentenceOngoing => (
                "Prison ID with ongoing sentence detected. Access denied.".to_owned(),
                Some(BUZZ),
            ),
            GateVerdict::SentenceStart(_) | GateVerdict::SentenceServed(_) => continue,
            GateVerdict::Denied | GateVerdict::Free => {
                if let Some(client) = world
                    .get::<crate::server::PlayerMob>(mover)
                    .map(|p| p.client)
                {
                    tell(
                        world,
                        client,
                        Span::Warning,
                        "Вы пытаетесь пройти сквозь световой барьер, но безуспешно.",
                    );
                }
                continue;
            }
        };
        say(world, gate, &text);
        if let Some(file) = file {
            sound(world, gate, file, 50.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sheet_values_and_crop_points() {
        assert_eq!(gulag_value("/obj/item/stack/sheet/iron"), 5);
        assert_eq!(gulag_value("/obj/item/stack/sheet/mineral/diamond"), 100);
        assert_eq!(gulag_value("/obj/item/stack/sheet/mineral/bananium"), -5);
        assert_eq!(gulag_value("/obj/item/stack/sheet/glass"), 0);
        assert_eq!(crop_points(50.0), 2);
        assert_eq!(crop_points(51.0), 4);
    }

    #[test]
    fn claim_status_follows_the_goal() {
        assert_eq!(
            status(None, false),
            (false, "No Prisoner ID detected.".to_owned())
        );
        let away = PrisonerCard {
            goal: 200,
            points: 150,
            ..Default::default()
        };
        assert_eq!(
            status(Some(&away), false),
            (false, "You are 50 points away.".to_owned())
        );
        assert!(status(Some(&away), true).0, "an emag sends anyone home");
        let met = PrisonerCard {
            goal: 200,
            points: 200,
            ..Default::default()
        };
        assert_eq!(status(Some(&met), false), (true, "Goal met!".to_owned()));
        assert_eq!(
            status(Some(&PrisonerCard::default()), false),
            (false, "No goal set!".to_owned())
        );
    }
}
