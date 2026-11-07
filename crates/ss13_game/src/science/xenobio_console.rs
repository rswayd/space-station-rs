//! The slime management console (server), after tg `/obj/machinery/computer/camera_advanced/xenobio`
//! (`code/modules/research/xenobiology/xenobio_camera.dm`).
//!
//! - A plain click on the console takes its controls (clicking it again lets go); while the
//!   operator stays next to it and conscious, the console's click shortcuts work on whatever
//!   the operator sees in the console's area (or an area with `XENOBIOLOGY_COMPATIBLE`):
//!   shift-click a slime to suck it up (at most 5, a player slime wiggles free, a feeding one
//!   lets go), shift-click the floor to spit every stored slime out there; ctrl-click a slime
//!   to scan it, ctrl-click a dead monkey to recycle it into 0.2 cubes (a monkey recycler must
//!   be in the console's area), ctrl-click the floor to recycle the dead monkeys there, else
//!   to place a new monkey from a stored cube (64 console monkeys at most); alt-click a slime
//!   to feed it the stored potion. Shift on anything else examines as usual.
//! - Stored slimes are hidden in the console ([`crate::closets::InCloset`]) on its tile, in
//!   stasis (`TRAIT_STASIS`: no hunger, no hunting); a gone console lets them out.
//! - Monkey cubes and slime potions go into the console by hand; a new potion swaps the old
//!   one out onto the operator's tile.
// shortcut: no remote camera eye (`/mob/eye/camera/remote/xenobio`), no action buttons and no
// HUD counter: the operator works the shortcuts on what it sees from the console (no camera
// coverage check); the counts are told in chat. No loading cubes from bags or boxes, no
// multitool linking (the recycler in the console's area is linked, as at roundstart), no
// recycler part tiers (`cube_production` 0.2), no power use, no suction animation.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmObj, Turf};
use ss13_reagents::is_under;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::protocol::ClickModifiers;
use ss13_world::Density;

use super::xenobio::{SLIME, Slime, say_around, slime_type, stop_feeding};
use crate::closets::InCloset;
use crate::health::{Health, Stat};
use crate::species::{Species, SpeciesId};
use crate::tools::tell;

pub const CONSOLE: &str = "/obj/machinery/computer/camera_advanced/xenobio";
pub const RECYCLER: &str = "/obj/machinery/monkey_recycler";
const MONKEY_CUBE: &str = "/obj/item/food/monkeycube";
const SLIME_POTION: &str = "/obj/item/slimepotion/slime";
/// `max_slimes`.
pub const MAX_SLIMES: usize = 5;
/// `/obj/machinery/monkey_recycler::cube_production` with tier 1 parts.
pub const CUBE_PRODUCTION: f32 = 0.2;
/// `CONFIG_GET(number/monkeycap)` default.
const MONKEY_CAP: usize = 64;
/// `XENOBIOLOGY_COMPATIBLE`.
const XENOBIOLOGY_COMPATIBLE: u32 = 1 << 6;
const SUCK_SOUND: &str = "sound/effects/compressed_air/air_suck.ogg";
const SHOOT_SOUND: &str = "sound/effects/compressed_air/air_shoot.ogg";

/// The console's storage.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct XenoConsole {
    pub slimes: Vec<Entity>,
    /// `stored_monkeys` (fractions from recycling).
    pub monkeys: f32,
    /// The stored potion's type.
    pub potion: Option<String>,
}

/// On a mob working a console's controls.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct XenoOperator {
    pub console: Entity,
}

/// A monkey the console placed (`TRAIT_SPAWNED_MOB`, `SSmobs.cubemonkeys`).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct ConsoleMonkey;

/// A shift, ctrl or alt click of an operator ([`crate::click`] sends it instead of the usual
/// modifier click): tg `user_shift_click`, `user_ctrl_click`, `user_alt_click`.
#[derive(Message, Clone, Copy, Debug)]
pub struct XenoConsoleClick {
    pub client: Entity,
    pub mob: Entity,
    pub target: Entity,
    pub target_pos: TilePos,
    pub modifiers: ClickModifiers,
}

pub struct XenobioConsolePlugin;

impl Plugin for XenobioConsolePlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::EmptyHand, 2663, |w, c| {
            take_controls(w, c.client, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 1773, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
        app.add_message::<XenoConsoleClick>()
            .add_observer(on_console_added)
            .add_observer(release_on_remove)
            .add_systems(Update, operator_clicks.after(crate::click::ResolveClicks));
    }
}

fn on_console_added(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    if objs
        .get(add.entity)
        .is_ok_and(|o| is_under(&o.path, CONSOLE))
    {
        commands
            .entity(add.entity)
            .try_insert(XenoConsole::default());
    }
}

/// `Destroy()`: the stored slimes drop out where the console stood.
fn release_on_remove(
    remove: On<Remove<XenoConsole>>,
    consoles: Query<&XenoConsole>,
    mut commands: Commands,
) {
    let Ok(console) = consoles.get(remove.entity) else {
        return;
    };
    for &slime in &console.slimes {
        commands.queue(move |world: &mut World| {
            if world.get_entity(slime).is_ok() {
                crate::closets::let_out(world, slime);
            }
        });
    }
}

fn tell_counts(world: &mut World, client: Entity, console: Entity) {
    let Some(c) = world.get::<XenoConsole>(console) else {
        return;
    };
    let text = format!(
        "Слаймов: {}/{MAX_SLIMES}, обезьян: {}, зелье: {}.",
        c.slimes.len(),
        (c.monkeys * 10.0).round() / 10.0,
        if c.potion.is_some() {
            "есть"
        } else {
            "нет"
        }
    );
    tell(world, client, Span::Notice, text);
}

/// A plain click on the console: take or give up its controls (tg `attack_hand` ->
/// `give_eye_control`, and leaving it).
fn take_controls(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<XenoConsole>(target).is_none() {
        return false;
    }
    if world
        .get::<XenoOperator>(mob)
        .is_some_and(|o| o.console == target)
    {
        world.entity_mut(mob).remove::<XenoOperator>();
        tell(world, client, Span::Notice, "Вы отходите от консоли.");
        return true;
    }
    world
        .entity_mut(mob)
        .insert(XenoOperator { console: target });
    // `hotkey_help`.
    tell(
        world,
        client,
        Span::Notice,
        "Управление консолью:\n\
         • Shift-клик по слайму — забрать его, по полу — выпустить всех слаймов.\n\
         • Ctrl-клик по слайму — просканировать его.\n\
         • Alt-клик по слайму — скормить ему зелье.\n\
         • Ctrl-клик по мёртвой обезьяне — переработать её, по полу — выпустить обезьяну.",
    );
    tell_counts(world, client, target);
    true
}

/// `item_interaction`: slime potions and monkey cubes into the console.
fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<XenoConsole>(target).is_none() {
        return false;
    }
    let Some(path) = world.get::<DmItem>(held).map(|i| i.path.clone()) else {
        return false;
    };
    if is_under(&path, SLIME_POTION) {
        world.despawn(held);
        let old = world
            .get_mut::<XenoConsole>(target)
            .and_then(|mut c| c.potion.replace(path));
        let text = if let Some(old) = old {
            // shortcut: the old potion lands on the operator's tile, not in its hand.
            if let Some(&pos) = world.get::<TilePos>(mob) {
                crate::construction::spawn_obj(world, &old, pos);
            }
            "Зелье заменено."
        } else {
            "Зелье загружено."
        };
        tell(world, client, Span::Notice, text);
        return true;
    }
    if is_under(&path, MONKEY_CUBE) {
        world.despawn(held);
        let stored = world.get_mut::<XenoConsole>(target).map_or(0.0, |mut c| {
            c.monkeys += 1.0;
            c.monkeys
        });
        tell(
            world,
            client,
            Span::Notice,
            format!("Кубиков в консоли: {stored}."),
        );
        return true;
    }
    false
}

fn operator_clicks(world: &mut World, mut cursor: Local<MessageCursor<XenoConsoleClick>>) {
    let clicks: Vec<XenoConsoleClick> = cursor
        .read(world.resource::<Messages<XenoConsoleClick>>())
        .copied()
        .collect();
    for click in clicks {
        operator_click(world, click);
    }
}

/// Whether `mob` may still work `console`: the console is there and next to the conscious mob
/// (tg `can_use` -> `check_eye`).
fn can_use(world: &World, mob: Entity, console: Entity) -> bool {
    let (Some(a), Some(b)) = (world.get::<TilePos>(mob), world.get::<TilePos>(console)) else {
        return false;
    };
    world.get::<XenoConsole>(console).is_some()
        && world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && a.z == b.z
        && (a.x - b.x).abs() <= 1
        && (a.y - b.y).abs() <= 1
}

/// `validate_turf`: the console's own area or a xenobiology compatible one.
pub fn valid_turf(world: &World, console: Entity, pos: TilePos) -> bool {
    let Some(&home) = world.get::<TilePos>(console) else {
        return false;
    };
    let (_, flags) = crate::teleport::area_flags(world, pos);
    flags & XENOBIOLOGY_COMPATIBLE != 0
        || crate::bots::area_name(world, pos) == crate::bots::area_name(world, home)
}

fn is_slime(world: &World, e: Entity) -> bool {
    world.get::<Slime>(e).is_some()
        && world
            .get::<ss13_mapload::DmMob>(e)
            .is_some_and(|m| is_under(&m.path, SLIME))
}

fn is_monkey(world: &World, e: Entity) -> bool {
    world.get::<Species>(e).map(|s| s.0) == Some(SpeciesId::Monkey)
}

fn is_open_turf(world: &World, e: Entity) -> bool {
    world.get::<Turf>(e).is_some() && !world.entity(e).contains::<Density>()
}

fn operator_click(world: &mut World, click: XenoConsoleClick) {
    let XenoConsoleClick {
        client,
        mob,
        target,
        target_pos,
        modifiers,
    } = click;
    let examine = |world: &mut World| {
        if modifiers.shift {
            world.write_message(crate::click::ExamineClick {
                client,
                examiner: mob,
                target,
            });
        }
    };
    let Some(console) = world.get::<XenoOperator>(mob).map(|o| o.console) else {
        return examine(world);
    };
    if !can_use(world, mob, console) {
        world.entity_mut(mob).remove::<XenoOperator>();
        tell(
            world,
            client,
            Span::Warning,
            "Вы слишком далеко от консоли.",
        );
        return examine(world);
    }
    let slime = is_slime(world, target);
    let monkey = is_monkey(world, target);
    let turf = is_open_turf(world, target);
    let handled = (modifiers.shift && (slime || turf))
        || (modifiers.ctrl && (slime || turf || monkey))
        || (modifiers.alt && slime);
    if !handled {
        return examine(world);
    }
    if !valid_turf(world, console, target_pos) {
        tell(world, client, Span::Warning, "Недопустимая зона!");
        return;
    }
    if modifiers.shift && slime {
        slime_pickup(world, client, console, target);
    } else if modifiers.shift {
        slime_place(world, console, target_pos);
    } else if modifiers.ctrl && slime {
        slime_scan(world, client, target);
    } else if modifiers.ctrl && monkey {
        if can_recycle(world, client, console) {
            recycle_monkey(world, client, console, target);
        }
    } else if modifiers.ctrl {
        if !recycle_monkeys_on_turf(world, client, console, target_pos) {
            feed_slime(world, client, console, target_pos);
        }
    } else if modifiers.alt {
        feed_potion(world, client, mob, console, target);
    }
    tell_counts(world, client, console);
}

/// `slime_pickup`.
pub fn slime_pickup(world: &mut World, client: Entity, console: Entity, slime: Entity) {
    let Some(stored) = world.get::<XenoConsole>(console).map(|c| c.slimes.clone()) else {
        return;
    };
    if stored.contains(&slime) || world.get::<InCloset>(slime).is_some() {
        return;
    }
    if stored.len() >= MAX_SLIMES {
        tell(world, client, Span::Warning, "Хранилище слаймов заполнено.");
        return;
    }
    if world.get::<crate::server::PlayerMob>(slime).is_some() {
        tell(world, client, Span::Warning, "Слайм вырвался!");
        return;
    }
    let (Some(&from), Some(&home)) = (world.get::<TilePos>(slime), world.get::<TilePos>(console))
    else {
        return;
    };
    if world
        .get::<Slime>(slime)
        .is_some_and(|s| s.feeding.is_some())
    {
        stop_feeding(world, slime, true);
    }
    say_around(world, from, Span::Notice, "Слайма засасывает!");
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(from, SUCK_SOUND, 50.0, true),
    );
    world.entity_mut(slime).insert(home);
    crate::closets::put_mob_inside(world, slime, console);
    if let Some(mut c) = world.get_mut::<XenoConsole>(console) {
        c.slimes.push(slime);
    }
}

/// `slime_place`: every stored slime out onto `pos`.
pub fn slime_place(world: &mut World, console: Entity, pos: TilePos) {
    let slimes = world
        .get_mut::<XenoConsole>(console)
        .map(|mut c| std::mem::take(&mut c.slimes))
        .unwrap_or_default();
    let slimes: Vec<Entity> = slimes
        .into_iter()
        .filter(|&s| world.get_entity(s).is_ok())
        .collect();
    if slimes.is_empty() {
        return;
    }
    let text = if slimes.len() == 1 {
        "Слайма выплёвывает!".to_owned()
    } else {
        format!("Выплёвывает слаймов: {}!", slimes.len())
    };
    for slime in slimes {
        crate::closets::let_out(world, slime);
        world.entity_mut(slime).insert(pos);
    }
    say_around(world, pos, Span::Notice, &text);
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, SHOOT_SOUND, 50.0, true),
    );
}

/// `feed_slime`: a new monkey from a stored cube.
pub fn feed_slime(world: &mut World, client: Entity, console: Entity, pos: TilePos) {
    let stored = world.get::<XenoConsole>(console).map_or(0.0, |c| c.monkeys);
    if stored < 1.0 {
        tell(
            world,
            client,
            Span::Warning,
            format!(
                "В консоли должна быть хотя бы 1 обезьяна. Сейчас: {}.",
                (stored * 10.0).round() / 10.0
            ),
        );
        return;
    }
    let alive = world
        .query_filtered::<(), With<ConsoleMonkey>>()
        .iter(world)
        .count();
    if alive >= MONKEY_CAP {
        tell(
            world,
            client,
            Span::Warning,
            format!(
                "Блюспейс-гармоники не позволяют создать на станции больше {MONKEY_CAP} обезьян одновременно!"
            ),
        );
        return;
    }
    let seed = world
        .resource_mut::<crate::combat::CombatRng>()
        .range(0, u32::MAX);
    let mut look = crate::human::AppearancePrefs::random(u64::from(seed)).body();
    look.species = SpeciesId::Monkey;
    let monkey = crate::server::spawn_body(world, pos, "monkey".to_owned(), look);
    world.entity_mut(monkey).insert(ConsoleMonkey);
    if let Some(mut c) = world.get_mut::<XenoConsole>(console) {
        c.monkeys = ((c.monkeys - 1.0) * 10.0).round() / 10.0;
    }
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, SHOOT_SOUND, 50.0, true),
    );
}

/// `can_recycle_monkeys`: a monkey recycler in the console's area.
fn can_recycle(world: &mut World, client: Entity, console: Entity) -> bool {
    let Some(&home) = world.get::<TilePos>(console) else {
        return false;
    };
    let area = crate::bots::area_name(world, home);
    let recyclers: Vec<TilePos> = world
        .query::<(&DmObj, &TilePos)>()
        .iter(world)
        .filter(|(o, _)| is_under(&o.path, RECYCLER))
        .map(|(_, &p)| p)
        .collect();
    if recyclers
        .into_iter()
        .any(|p| crate::bots::area_name(world, p) == area)
    {
        return true;
    }
    tell(
        world,
        client,
        Span::Warning,
        "Нет подключённого переработчика обезьян.",
    );
    false
}

/// `can_recycle_target_monkey`: a dead monkey.
fn recyclable(world: &World, e: Entity) -> bool {
    is_monkey(world, e)
        && world.get::<Health>(e).is_some()
        && world.get::<Stat>(e) == Some(&Stat::Dead)
}

/// `recycle_monkey`.
pub fn recycle_monkey(world: &mut World, client: Entity, console: Entity, monkey: Entity) -> bool {
    if !recyclable(world, monkey) {
        tell(world, client, Span::Warning, "Это не мёртвая обезьяна!");
        return false;
    }
    if let Some(&pos) = world.get::<TilePos>(monkey) {
        say_around(
            world,
            pos,
            Span::Notice,
            "Обезьяну засасывает на переработку!",
        );
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, SUCK_SOUND, 50.0, true),
        );
    }
    if let Some(mut c) = world.get_mut::<XenoConsole>(console) {
        c.monkeys = ((c.monkeys + CUBE_PRODUCTION) * 10.0).round() / 10.0;
    }
    world.despawn(monkey);
    true
}

/// `try_recycle_monkeys_on_turf`: whether a monkey was found.
fn recycle_monkeys_on_turf(
    world: &mut World,
    client: Entity,
    console: Entity,
    pos: TilePos,
) -> bool {
    let monkeys: Vec<Entity> = world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| recyclable(world, e))
        .collect();
    if monkeys.is_empty() || !can_recycle(world, client, console) {
        return false;
    }
    for m in monkeys {
        recycle_monkey(world, client, console, m);
    }
    true
}

/// `slime_scan` (`slime_scanner.dm`).
pub fn slime_scan(world: &mut World, client: Entity, slime: Entity) {
    let Some(s) = world.get::<Slime>(slime).cloned() else {
        return;
    };
    let health = world
        .get::<Health>(slime)
        .map_or(0.0, |h| (h.health() / h.max.max(1.0) * 100.0).round());
    let stage = if s.adult { "adult" } else { "baby" };
    let mut text = format!(
        "Результаты сканирования слайма:\n{} {stage} slime\nПитание: {}/200",
        s.colour, s.nutrition
    );
    if s.nutrition < 10.0 {
        text += "\nВнимание: слайм голодает!";
    } else if s.nutrition < 50.0 {
        text += "\nВнимание: слайм голоден";
    }
    let mutations: Vec<&str> = slime_type(s.colour).mutations.iter().map(|m| m.0).collect();
    text += &format!(
        "\nЭлектрический заряд: {}\nЗдоровье: {health}%\nВозможные мутации: {}\n\
         Генетическая нестабильность: {}% шанс мутации при делении.",
        s.powerlevel,
        mutations.join(", "),
        s.mutation_chance
    );
    if s.cores > 1 {
        text += "\nОбнаружено несколько ядер";
    }
    text += &format!("\nПрогресс роста: {}/10", s.amount_grown);
    if let Some(m) = s.crossbreed {
        text += &format!(
            "\nМутация ядра: {m}\nПрогресс мутации ядра: {} / 10",
            s.crossbreed_amount
        );
    }
    tell(world, client, Span::Notice, text);
}

/// `alt_click_slime`: the stored potion is shot onto the slime's tile and used on it; one that
/// does not take stays on the floor (tg `spit_atom` then `attack`).
pub fn feed_potion(world: &mut World, client: Entity, mob: Entity, console: Entity, slime: Entity) {
    let Some(path) = world
        .get_mut::<XenoConsole>(console)
        .and_then(|mut c| c.potion.take())
    else {
        tell(world, client, Span::Warning, "Зелье не загружено.");
        return;
    };
    let Some(&pos) = world.get::<TilePos>(slime) else {
        return;
    };
    let Some(potion) = crate::construction::spawn_obj(world, &path, pos) else {
        return;
    };
    world.flush();
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, SHOOT_SOUND, 50.0, true),
    );
    super::xenobio::item_interaction(world, client, mob, potion, slime);
}
