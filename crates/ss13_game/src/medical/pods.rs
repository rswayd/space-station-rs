//! Enclosed medical machines (server): tg `/obj/machinery` `state_open`/`occupant`
//! (`open_machine`, `close_machine`, `relaymove`, `container_resist_act`) shared by the sleeper
//! ([`super::sleeper`]), the lipid extractor (`/obj/machinery/fat_sucker`,
//! `code/game/machinery/fat_sucker.dm`) and the organ harvester (`/obj/machinery/harvester`,
//! `code/game/machinery/harvester.dm`).
//!
//! - Closing a machine ([`close_pod`]) shuts in the given mob or a living mob on its tile that
//!   is not buckled or inside something; it turns dense. Opening lets the occupant out.
//! - Lipid extractor: an empty hand opens or closes it (with the safety hatch locked a running
//!   one stays shut); alt-click locks or unlocks the hatch (kitchen access, or emagged; not from
//!   inside). Two seconds after a carbon is shut in it starts when the occupant is fatter than
//!   `start_at` (well fed), then every machine tick takes `bite_size` nutrition a second (7.5 +
//!   2.5 per micro-laser tier) into its store, telling a fat fact every 10 ticks, until the
//!   occupant reaches `stop_at` (starving) or power fails. Opening turns the store into slabs of
//!   meat (`nutrient_to_meat`: 90 - 5 per micro-laser tier), cutlets of the rest and a cookie for
//!   a double helping. An emag lowers the thresholds to 100 and 0.
//! - Organ harvester: an empty hand closes it, or opens it when it is not harvesting; alt-click
//!   with the panel open turns the output direction, otherwise starts the harvest of a dead,
//!   undressed carbon inside (an emag allows the living and clothes). Every `interval` (40 - the
//!   micro-laser tiers, deciseconds) one bodypart goes to the output tile, from the legs up to
//!   the head (with its organs) and the chest's organs; then it opens. Power loss or opening
//!   aborts.
//! - A step from inside opens the sleeper, an extractor with its hatch unlocked or a harvester
//!   that is not running.
// shortcut: no sounds, sound loops, lights or flick animations; no breaking out of a locked
// extractor (tg's 60 s `do_after`: the step only tells it is locked); the extractor's cookie
// lands on its tile instead of in the occupant's hands, meat is always human meat with no
// material; the harvester does not notify ghosts, every carbon counts as organic.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmObj, DmPower, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Dir, Overlay, TilePos};
use ss13_world::{Density, SpatialIndex};

use crate::closets::{ClosetRelayMove, InCloset};
use crate::health::{Health, Stat};
use crate::obj_damage::is_under;
use crate::power::{MACHINE_TICK_SECS, MachineTicks, Unpowered};
use crate::stock_parts::PartKind;
use crate::tools::tell;
use crate::zone::BodyZone;

pub const FAT_SUCKER_ROOT: &str = "/obj/machinery/fat_sucker";
pub const HARVESTER_ROOT: &str = "/obj/machinery/harvester";
/// `ACCESS_KITCHEN`.
const ACCESS_KITCHEN: &str = "kitchen";
const HUMAN_MEAT: &str = "/obj/item/food/meat/slab/human";
const CUTLET: &str = "/obj/item/food/meat/rawcutlet/plain";
const COOKIE: &str = "/obj/item/food/cookie";
/// `addtimer(start_extracting, 20)`: deciseconds.
const START_DELAY_SECS: f64 = 2.0;
/// `next_fact`: machine ticks between facts.
const FACT_TICKS: u32 = 10;
const FAT_FACTS: [&str; 6] = [
    "Жиры — это триглицериды, состоящие из глицерина и жирных кислот.",
    "Взрослым рекомендуется получать 20-35% энергии из жиров.",
    "Лишний вес и ожирение повышают риск хронических болезней: сердечно-сосудистых, \
     метаболического синдрома, диабета второго типа и некоторых видов рака.",
    "Не все жиры вредны. Определённое количество жира — необходимая часть здорового питания.",
    "Насыщенные жиры должны давать не больше 11% дневных калорий.",
    "Ненасыщенные жиры, то есть мононенасыщенные, полиненасыщенные и омега-3 жирные кислоты, \
     содержатся в растительной пище и рыбе.",
];

/// A machine with a door and room for one mob (tg `state_open`, `occupant`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pod {
    pub open: bool,
}

/// `/obj/machinery/fat_sucker` vars.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct FatSucker {
    pub processing: bool,
    pub start_at: f32,
    pub stop_at: f32,
    /// `free_exit`: the safety hatch is unlocked.
    pub free_exit: bool,
    /// `nutrients` taken so far.
    pub nutrients: f32,
    pub next_fact: u32,
    pub emagged: bool,
    /// The pending `start_extracting` (game seconds).
    pub starts_at: Option<f64>,
}

impl Default for FatSucker {
    fn default() -> Self {
        Self {
            processing: false,
            start_at: crate::hunger::NUTRITION_LEVEL_WELL_FED,
            stop_at: crate::hunger::NUTRITION_LEVEL_STARVING,
            free_exit: true,
            nutrients: 0.0,
            next_fact: FACT_TICKS,
            emagged: false,
            starts_at: None,
        }
    }
}

/// `/obj/machinery/harvester` vars.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Harvester {
    pub harvesting: bool,
    pub warming_up: bool,
    /// `operation_order`: the bodyparts still to take.
    pub order: Vec<BodyZone>,
    pub output_dir: Dir,
    pub allow_clothing: bool,
    pub allow_living: bool,
    pub emagged: bool,
    /// The next `harvest` step (game seconds).
    pub next_at: Option<f64>,
}

impl Default for Harvester {
    fn default() -> Self {
        Self {
            harvesting: false,
            warming_up: false,
            order: Vec::new(),
            output_dir: Dir::South,
            allow_clothing: false,
            allow_living: false,
            emagged: false,
            next_at: None,
        }
    }
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // tg `attack_hand`/`interact` of the sleeper, the extractor and the harvester.
    app.add_click_handler(ClickPhase::EmptyHand, 2650, |w, c| {
        crate::medical::pods::interact(w, c.client, c.mob, c.target)
    });
    app.add_observer(attach)
        .add_observer(release_on_despawn)
        .add_systems(
            Update,
            (
                relay_moves.after(ss13_world::WorldSystems::Movement),
                process_fat_suckers
                    .after(crate::power::PowerSystems)
                    .run_if(any_with_component::<FatSucker>),
                timers.run_if(any_with_component::<Pod>),
            ),
        );
}

fn attach(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(o) = objs.get(add.entity) else {
        return;
    };
    let e = add.entity;
    if is_under(&o.path, super::sleeper::SLEEPER_ROOT) {
        commands.entity(e).insert_if_new((
            Pod { open: true },
            super::sleeper::Sleeper::of_path(&o.path),
        ));
    } else if is_under(&o.path, FAT_SUCKER_ROOT) {
        commands
            .entity(e)
            .insert_if_new((Pod { open: false }, FatSucker::default()));
    } else if is_under(&o.path, HARVESTER_ROOT) {
        commands
            .entity(e)
            .insert_if_new((Pod { open: false }, Harvester::default()));
        // `prob(1)`: "auto-autopsy".
        commands.queue(move |world: &mut World| {
            if world.resource_mut::<crate::combat::CombatRng>().chance(1.0) {
                world.entity_mut(e).insert(Name::new("авто-аутопсия"));
            }
        });
    } else {
        return;
    }
    commands.queue(move |world: &mut World| refresh_look(world, e));
}

/// A despawned machine lets its occupant out where it stood.
fn release_on_despawn(
    remove: On<Remove<Pod>>,
    inside: Query<(Entity, &InCloset)>,
    mut commands: Commands,
) {
    for (mob, c) in &inside {
        if c.closet == remove.entity {
            commands.queue(move |world: &mut World| crate::closets::let_out(world, mob));
        }
    }
}

/// The mob shut in `pod` (`occupant`).
pub fn occupant(world: &mut World, pod: Entity) -> Option<Entity> {
    let mut q = world.query::<(Entity, &InCloset)>();
    q.iter(world).find(|(_, c)| c.closet == pod).map(|(e, _)| e)
}

/// `is_operational`: powered and not broken.
pub fn operational(world: &World, e: Entity) -> bool {
    world.get::<Unpowered>(e).is_none()
        && !world
            .get::<crate::obj_damage::Integrity>(e)
            .is_some_and(|i| i.broken)
}

pub fn panel_open(world: &World, e: Entity) -> bool {
    world
        .get::<crate::construction::PanelOpen>(e)
        .is_some_and(|p| p.0)
}

fn is_open(world: &World, pod: Entity) -> bool {
    world.get::<Pod>(pod).is_some_and(|p| p.open)
}

fn tell_mob(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<crate::server::PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

/// `open_machine` (base): the occupant out, undense.
pub fn open_pod(world: &mut World, pod: Entity) {
    if let Some(mob) = occupant(world, pod) {
        crate::closets::let_out(world, mob);
    }
    if let Some(mut p) = world.get_mut::<Pod>(pod) {
        p.open = true;
    }
    world.entity_mut(pod).remove::<Density>();
    refresh_look(world, pod);
}

/// `close_machine(target)` (base): `target`, else a living mob on the tile that is not buckled
/// and not inside something, goes in; the machine turns dense. The occupant.
pub fn close_pod(world: &mut World, pod: Entity, target: Option<Entity>) -> Option<Entity> {
    let &pos = world.get::<TilePos>(pod)?;
    if let Some(mut p) = world.get_mut::<Pod>(pod) {
        p.open = false;
    }
    world.entity_mut(pod).insert(Density);
    let free = |world: &World, e: Entity| {
        world.get::<Health>(e).is_some()
            && world.get::<InCloset>(e).is_none()
            && world.get::<crate::medical::surgery::Buckled>(e).is_none()
    };
    let target = target.filter(|&t| free(world, t)).or_else(|| {
        world
            .resource::<SpatialIndex>()
            .on_tile(pos)
            .iter()
            .copied()
            .find(|&e| e != pod && free(world, e))
    });
    let occupied = occupant(world, pod).is_some();
    let mob = target.filter(|_| !occupied);
    if let Some(mob) = mob {
        world.entity_mut(mob).insert(pos);
        crate::closets::put_mob_inside(world, mob, pod);
    }
    refresh_look(world, pod);
    mob
}

/// The machines' `update_icon_state`/`update_overlays`.
pub fn refresh_look(world: &mut World, pod: Entity) {
    if world.get::<super::sleeper::Sleeper>(pod).is_some() {
        super::sleeper::refresh_look(world, pod);
    } else if world.get::<FatSucker>(pod).is_some() {
        fat_look(world, pod);
    } else if let Some(h) = world.get::<Harvester>(pod).cloned() {
        let state = if panel_open(world, pod) {
            "harvester-o"
        } else if is_open(world, pod) {
            "harvester-open"
        } else if h.warming_up {
            "harvester-charging"
        } else if h.harvesting {
            "harvester-active"
        } else {
            "harvester"
        };
        crate::obj_damage::set_icon_state(world, pod, state);
    }
}

/// `attack_hand`/`interact` of the three machines. True when `target` is one of them.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<super::sleeper::Sleeper>(target).is_some() {
        super::sleeper::interact(world, client, mob, target);
        return true;
    }
    if world.get::<FatSucker>(target).is_some() {
        fat_interact(world, client, target);
        return true;
    }
    if let Some(h) = world.get::<Harvester>(target).cloned() {
        if is_open(world, target) {
            close_pod(world, target, None);
        } else if !h.harvesting {
            harvester_open(world, target);
        }
        return true;
    }
    false
}

/// `mouse_drop_receive` (from `crate::drag`): a carbon dragged onto an open, empty sleeper is
/// shut in.
pub fn mouse_drop_receive(world: &mut World, target: Entity, dragged: Entity) -> bool {
    if world.get::<super::sleeper::Sleeper>(target).is_none() {
        return false;
    }
    if is_open(world, target)
        && world
            .get::<crate::medical::wounds::Limbs>(dragged)
            .is_some()
        && occupant(world, target).is_none()
    {
        super::sleeper::close(world, target, Some(dragged));
    }
    true
}

/// `click_alt` of the three machines. True when `target` is one of them.
pub fn click_alt(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<super::sleeper::Sleeper>(target).is_some() {
        if is_open(world, target) {
            super::sleeper::close(world, target, None);
        } else {
            super::sleeper::open(world, target);
        }
        return true;
    }
    if let Some(fat) = world.get::<FatSucker>(target).cloned() {
        if world
            .get::<InCloset>(mob)
            .is_some_and(|c| c.closet == target)
        {
            tell(
                world,
                client,
                Span::Warning,
                "Вы не можете дотянуться до управления изнутри!",
            );
            return true;
        }
        let access = crate::bots::user_access(world, mob);
        let allowed = fat.emagged
            || crate::silicon::interact::has_silicon_access(world, mob)
            || crate::inventory::check_access(&[ACCESS_KITCHEN.to_owned()], &[], &access);
        if !allowed {
            tell(
                world,
                client,
                Span::Warning,
                "У вас нет необходимого доступа.",
            );
            return true;
        }
        let free_exit = !fat.free_exit;
        if let Some(mut f) = world.get_mut::<FatSucker>(target) {
            f.free_exit = free_exit;
        }
        let text = if free_exit {
            "Защитный люк разблокирован."
        } else {
            "Защитный люк заблокирован."
        };
        tell(world, client, Span::Notice, text);
        return true;
    }
    if let Some(h) = world.get::<Harvester>(target).cloned() {
        if panel_open(world, target) {
            let dir = clockwise(h.output_dir);
            if let Some(mut h) = world.get_mut::<Harvester>(target) {
                h.output_dir = dir;
            }
            tell(
                world,
                client,
                Span::Notice,
                format!(
                    "Вы меняете настройки вывода: теперь органы выходят на {}.",
                    dir_name(dir)
                ),
            );
            return true;
        }
        if !h.harvesting && !is_open(world, target) && can_harvest(world, target) {
            start_harvest(world, target);
        }
        return true;
    }
    false
}

/// `emag_act` of the three machines; `None` when `target` is none of them.
pub fn emag_act(world: &mut World, client: Entity, target: Entity) -> Option<bool> {
    if world.get::<super::sleeper::Sleeper>(target).is_some() {
        return Some(super::sleeper::emag_act(world, client, target));
    }
    if let Some(fat) = world.get::<FatSucker>(target).cloned() {
        if fat.emagged {
            return Some(false);
        }
        world.entity_mut(target).insert(FatSucker {
            emagged: true,
            start_at: 100.0,
            stop_at: 0.0,
            ..fat
        });
        tell(
            world,
            client,
            Span::Notice,
            "Вы снимаете ограничения доступа и понижаете порог автоматического извлечения!",
        );
        return Some(true);
    }
    if let Some(h) = world.get::<Harvester>(target).cloned() {
        if h.emagged {
            return Some(false);
        }
        world.entity_mut(target).insert(Harvester {
            emagged: true,
            allow_living: true,
            allow_clothing: true,
            ..h
        });
        tell(
            world,
            client,
            Span::Notice,
            "сканеры жизненных показателей перегружены",
        );
        return Some(true);
    }
    None
}

/// `relaymove` -> `container_resist_act`: a conscious occupant's step.
fn relay_moves(world: &mut World, mut moves: Local<MessageCursor<ClosetRelayMove>>) {
    let mobs: Vec<Entity> = moves
        .read(world.resource::<Messages<ClosetRelayMove>>())
        .map(|m| m.mob)
        .collect();
    for mob in mobs {
        if world.get::<Stat>(mob) != Some(&Stat::Conscious) {
            continue;
        }
        let Some(pod) = world.get::<InCloset>(mob).map(|c| c.closet) else {
            continue;
        };
        if world.get::<Pod>(pod).is_none_or(|p| p.open) {
            continue;
        }
        if world.get::<super::sleeper::Sleeper>(pod).is_some() {
            emerges(world, mob, pod);
            super::sleeper::open(world, pod);
        } else if let Some(fat) = world.get::<FatSucker>(pod).cloned() {
            if fat.free_exit {
                fat_open(world, pod);
            } else {
                tell_mob(
                    world,
                    mob,
                    Span::Notice,
                    "Аварийный выход не отвечает! Вы начинаете давить на корпус!",
                );
            }
        } else if let Some(h) = world.get::<Harvester>(pod).cloned() {
            if h.harvesting {
                let name = super::name_of(world, pod);
                tell_mob(
                    world,
                    mob,
                    Span::Warning,
                    format!("{name} работает и не может быть открыт!"),
                );
            } else {
                emerges(world, mob, pod);
                harvester_open(world, pod);
            }
        }
    }
}

fn emerges(world: &mut World, mob: Entity, pod: Entity) {
    let (who, what) = (super::name_of(world, mob), super::name_of(world, pod));
    super::visible(
        world,
        mob,
        &format!("Вы выбираетесь из {what}."),
        &format!("{who} выбирается из {what}!"),
        Span::Notice,
    );
}

/// The extractor's `start_extracting` and the harvester's `harvest` timers.
// Cost: per tick one pass over the medical pods (a handful per map), a field read each.
fn timers(world: &mut World) {
    let now = now(world);
    let mut q =
        world.query_filtered::<(Entity, Option<&FatSucker>, Option<&Harvester>), With<Pod>>();
    let due: Vec<(Entity, bool)> = q
        .iter(world)
        .filter_map(|(e, f, h)| {
            if f.and_then(|f| f.starts_at).is_some_and(|t| now >= t) {
                Some((e, true))
            } else if h.and_then(|h| h.next_at).is_some_and(|t| now >= t) {
                Some((e, false))
            } else {
                None
            }
        })
        .collect();
    for (e, fat) in due {
        if fat {
            if let Some(mut f) = world.get_mut::<FatSucker>(e) {
                f.starts_at = None;
            }
            start_extracting(world, e);
        } else {
            if let Some(mut h) = world.get_mut::<Harvester>(e) {
                h.next_at = None;
            }
            harvest(world, e);
        }
    }
}

// The lipid extractor.

/// `RefreshParts`: (`bite_size`, `nutrient_to_meat`) from the micro-lasers' tiers.
pub fn fat_ratings(laser_tiers: u32) -> (f32, f32) {
    let rating = laser_tiers as f32;
    (7.5 + rating * 2.5, 90.0 - rating * 5.0)
}

fn fat_numbers(world: &World, e: Entity) -> (f32, f32) {
    let r = crate::stock_parts::ratings(world, e);
    fat_ratings(r.sum(PartKind::MicroLaser))
}

fn fat_interact(world: &mut World, client: Entity, e: Entity) {
    let Some(fat) = world.get::<FatSucker>(e).cloned() else {
        return;
    };
    if is_open(world, e) {
        fat_close(world, client, e);
    } else if !fat.processing || fat.free_exit {
        fat_open(world, e);
    } else {
        tell(world, client, Span::Warning, "Защитный люк заблокирован!");
    }
}

/// `fat_sucker/close_machine`.
fn fat_close(world: &mut World, client: Entity, e: Entity) {
    if panel_open(world, e) {
        tell(
            world,
            client,
            Span::Warning,
            "Сначала нужно закрыть техническую панель!",
        );
        return;
    }
    let Some(mob) = close_pod(world, e, None) else {
        return;
    };
    // `iscarbon(occupant)`: other mobs are put back out.
    if world.get::<crate::medical::wounds::Limbs>(mob).is_none() {
        crate::closets::let_out(world, mob);
        refresh_look(world, e);
        return;
    }
    let name = super::name_of(world, e);
    tell_mob(
        world,
        mob,
        Span::Notice,
        format!("Вы забираетесь в {name}."),
    );
    let at = now(world) + START_DELAY_SECS;
    if let Some(mut f) = world.get_mut::<FatSucker>(e) {
        f.starts_at = Some(at);
    }
    refresh_look(world, e);
}

/// `fat_sucker/open_machine`.
fn fat_open(world: &mut World, e: Entity) {
    make_meat(world, e);
    if let Some(mut f) = world.get_mut::<FatSucker>(e) {
        f.processing = false;
        f.starts_at = None;
    }
    open_pod(world, e);
}

/// `start_extracting`.
fn start_extracting(world: &mut World, e: Entity) {
    let Some(fat) = world.get::<FatSucker>(e).cloned() else {
        return;
    };
    if is_open(world, e) || fat.processing || world.get::<Unpowered>(e).is_some() {
        return;
    }
    let Some(mob) = occupant(world, e) else {
        return;
    };
    let nutrition = world
        .get::<crate::hunger::Hunger>(mob)
        .map_or(0.0, |h| h.nutrition);
    if nutrition > fat.start_at {
        if let Some(mut f) = world.get_mut::<FatSucker>(e) {
            f.processing = true;
        }
        refresh_look(world, e);
    } else {
        crate::atmos_machinery::machine_say(world, e, "Объект недостаточно толстый.");
    }
}

/// `process(seconds_per_tick)` every machine tick.
// Cost: one pass over the extractors (a few per map) per machine tick (2 s).
fn process_fat_suckers(world: &mut World, mut last: Local<Option<u64>>) {
    let ticks = world.resource::<MachineTicks>().0;
    let from = last.unwrap_or(ticks);
    *last = Some(ticks);
    if from == ticks {
        return;
    }
    let mut q = world.query::<(Entity, &FatSucker)>();
    let running: Vec<Entity> = q
        .iter(world)
        .filter(|(_, f)| f.processing)
        .map(|(e, _)| e)
        .collect();
    for _ in from..ticks {
        for &e in &running {
            fat_tick(world, e, MACHINE_TICK_SECS as f32);
        }
    }
}

fn fat_tick(world: &mut World, e: Entity, secs: f32) {
    let Some(fat) = world.get::<FatSucker>(e).cloned() else {
        return;
    };
    if !fat.processing {
        return;
    }
    let mob = occupant(world, e);
    let carbon = mob.filter(|&m| world.get::<crate::medical::wounds::Limbs>(m).is_some());
    let (Some(mob), true) = (carbon, world.get::<Unpowered>(e).is_none()) else {
        fat_open(world, e);
        return;
    };
    let nutrition = world
        .get::<crate::hunger::Hunger>(mob)
        .map_or(0.0, |h| h.nutrition);
    if nutrition <= fat.stop_at {
        fat_open(world, e);
        return;
    }
    let (bite, _) = fat_numbers(world, e);
    if let Some(mut h) = world.get_mut::<crate::hunger::Hunger>(mob) {
        h.adjust(-bite * secs);
    }
    let fact = fat.next_fact == 0;
    if let Some(mut f) = world.get_mut::<FatSucker>(e) {
        f.nutrients += bite * secs;
        f.next_fact = if fact { FACT_TICKS } else { f.next_fact - 1 };
    }
    if fact {
        let i = crate::medical::fork_rng(world).range(0, FAT_FACTS.len() as u32 - 1) as usize;
        crate::atmos_machinery::machine_say(world, e, FAT_FACTS[i]);
    }
    let watts = world
        .get::<DmPower>(e)
        .map_or(0.0, |p| p.active_power_usage);
    crate::kitchen::machine_use_energy(world, e, watts * f64::from(secs));
}

/// What `make_meat` gives for `nutrients`: (cookie, slabs, cutlets).
pub fn meat_yield(nutrients: f32, nutrient_to_meat: f32) -> (bool, u32, u32) {
    if nutrient_to_meat <= 0.0 {
        return (false, 0, 0);
    }
    let cookie = nutrients >= nutrient_to_meat * 2.0;
    let mut left = nutrients;
    let mut slabs = 0;
    while left >= nutrient_to_meat {
        left -= nutrient_to_meat;
        slabs += 1;
    }
    let mut cutlets = 0;
    while left >= nutrient_to_meat / 3.0 {
        left -= nutrient_to_meat / 3.0;
        cutlets += 1;
    }
    (cookie, slabs, cutlets)
}

/// `make_meat`: the store becomes food on the extractor's tile.
fn make_meat(world: &mut World, e: Entity) {
    let Some(nutrients) = world.get::<FatSucker>(e).map(|f| f.nutrients) else {
        return;
    };
    let carbon =
        occupant(world, e).is_some_and(|m| world.get::<crate::medical::wounds::Limbs>(m).is_some());
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    if !carbon {
        return;
    }
    let (_, n2m) = fat_numbers(world, e);
    let (cookie, slabs, cutlets) = meat_yield(nutrients, n2m);
    if cookie {
        crate::construction::spawn_obj(world, COOKIE, pos);
    }
    for _ in 0..slabs {
        crate::construction::spawn_obj(world, HUMAN_MEAT, pos);
    }
    for _ in 0..cutlets {
        crate::construction::spawn_obj(world, CUTLET, pos);
    }
    if let Some(mut f) = world.get_mut::<FatSucker>(e) {
        f.nutrients = 0.0;
    }
}

/// `fat_sucker/update_overlays`.
fn fat_look(world: &mut World, e: Entity) {
    let Some(fat) = world.get::<FatSucker>(e).cloned() else {
        return;
    };
    let open = is_open(world, e);
    let powered = world.get::<Unpowered>(e).is_none();
    let occupied = occupant(world, e).is_some();
    let mut states: Vec<&str> = Vec::new();
    if !open {
        if fat.processing {
            states.extend(["fat_door_on", "fat_stack", "fat_smoke", "fat_green"]);
        } else {
            states.push("fat_door_off");
            if occupied {
                if powered {
                    states.extend(["fat_stack", "fat_yellow"]);
                }
            } else {
                states.push("fat_red");
            }
        }
    } else if powered {
        states.push("fat_red");
    }
    if panel_open(world, e) {
        states.push("fat_panel");
    }
    let overlays: Vec<Overlay> = {
        let icon = world.get::<DmAtom>(e).map(|a| a.icon.clone());
        let icons = world.get_resource::<IconIndex>();
        states
            .iter()
            .filter_map(|s| icons.zip(icon.as_ref()).and_then(|(i, f)| i.get(f, s)))
            .map(Overlay::new)
            .collect()
    };
    if let Some(mut look) = world.get_mut::<Appearance>(e)
        && look.overlays != overlays
    {
        look.overlays = overlays;
    }
}

// The organ harvester.

/// `RefreshParts`: `interval` (deciseconds) from the micro-lasers' tiers.
pub fn harvest_interval(laser_tiers: u32) -> f32 {
    (40.0 - laser_tiers as f32).max(1.0)
}

fn interval_secs(world: &World, e: Entity) -> f64 {
    let r = crate::stock_parts::ratings(world, e);
    f64::from(harvest_interval(r.sum(PartKind::MicroLaser))) / 10.0
}

/// `reverse_range(get_bodyparts())`: the legs first, the chest last.
pub fn harvest_order(present: impl Fn(BodyZone) -> bool) -> Vec<BodyZone> {
    [
        BodyZone::LLeg,
        BodyZone::RLeg,
        BodyZone::RArm,
        BodyZone::LArm,
        BodyZone::Head,
        BodyZone::Chest,
    ]
    .into_iter()
    .filter(|&z| present(z))
    .collect()
}

/// `harvester/open_machine`: not with the panel open.
fn harvester_open(world: &mut World, e: Entity) {
    if panel_open(world, e) {
        return;
    }
    if let Some(mut h) = world.get_mut::<Harvester>(e) {
        h.warming_up = false;
        h.harvesting = false;
        h.next_at = None;
    }
    open_pod(world, e);
}

/// `can_harvest`.
fn can_harvest(world: &mut World, e: Entity) -> bool {
    let Some(h) = world.get::<Harvester>(e).cloned() else {
        return false;
    };
    if world.get::<Unpowered>(e).is_some() || is_open(world, e) {
        return false;
    }
    let Some(mob) = occupant(world, e) else {
        return false;
    };
    if world.get::<crate::medical::wounds::Limbs>(mob).is_none() {
        return false;
    }
    if !h.allow_clothing && crate::gibber::has_abiotic(world, mob) {
        crate::atmos_machinery::machine_say(
            world,
            e,
            "На объекте не должно быть абиотических предметов.",
        );
        return false;
    }
    // `IS_DEAD_OR_FAKING`: a changeling's fake death keeps `Stat::Dead` too.
    let dead = world.get::<Stat>(mob) == Some(&Stat::Dead);
    if !h.allow_living && !dead {
        crate::atmos_machinery::machine_say(world, e, "Объект всё ещё жив.");
        return false;
    }
    true
}

/// `start_harvest`.
fn start_harvest(world: &mut World, e: Entity) {
    let Some(mob) = occupant(world, e) else {
        return;
    };
    let order = {
        let limbs = world.get::<crate::medical::wounds::Limbs>(mob);
        harvest_order(|z| limbs.is_some_and(|l| !l.limb(z).missing))
    };
    let at = now(world) + interval_secs(world, e);
    if let Some(mut h) = world.get_mut::<Harvester>(e) {
        h.order = order;
        h.warming_up = true;
        h.harvesting = true;
        h.next_at = Some(at);
    }
    let name = super::name_of(world, e);
    super::visible(
        world,
        e,
        "",
        &format!("{name} начинает разогреваться!"),
        Span::Notice,
    );
    crate::atmos_machinery::machine_say(world, e, "Запуск протокола извлечения.");
    refresh_look(world, e);
}

/// `harvest`: one bodypart to the output tile, then the next step.
fn harvest(world: &mut World, e: Entity) {
    let Some(h) = world.get::<Harvester>(e).cloned() else {
        return;
    };
    if let Some(mut h) = world.get_mut::<Harvester>(e) {
        h.warming_up = false;
    }
    refresh_look(world, e);
    let mob =
        occupant(world, e).filter(|&m| world.get::<crate::medical::wounds::Limbs>(m).is_some());
    let Some(mob) =
        mob.filter(|_| h.harvesting && !is_open(world, e) && world.get::<Unpowered>(e).is_none())
    else {
        end_harvesting(world, e, false);
        return;
    };
    let Some((&zone, rest)) = h.order.split_first() else {
        end_harvesting(world, e, true);
        return;
    };
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let target = pos.step(h.output_dir);
    if zone == BodyZone::Chest {
        let organs: Vec<Entity> = world
            .get::<crate::medical::organs::Organs>(mob)
            .map(|o| o.iter().collect())
            .unwrap_or_default();
        for organ in organs {
            let in_chest = world
                .get::<crate::medical::organs::Organ>(organ)
                .is_some_and(|o| o.slot.bodypart() == BodyZone::Chest);
            if in_chest {
                crate::medical::organs::remove(world, organ, false);
                crate::storage::take_out(world, organ, target);
            }
        }
    } else if let Some(item) = crate::medical::bodyparts::drop_limb(world, mob, zone) {
        crate::storage::take_out(world, item, target);
        crate::medical::bodyparts::drop_organs(world, item, target);
    }
    super::diseases::emote(world, mob, "scream");
    let watts = world
        .get::<DmPower>(e)
        .map_or(0.0, |p| p.active_power_usage);
    crate::kitchen::machine_use_energy(world, e, watts);
    let at = now(world) + interval_secs(world, e);
    if let Some(mut h) = world.get_mut::<Harvester>(e) {
        h.order = rest.to_vec();
        h.next_at = Some(at);
    }
}

/// `end_harvesting`.
fn end_harvesting(world: &mut World, e: Entity, success: bool) {
    harvester_open(world, e);
    let text = if success {
        "Объект успешно разобран."
    } else {
        "Протокол прерван. Извлечение отменено."
    };
    crate::atmos_machinery::machine_say(world, e, text);
}

/// BYOND `turn(dir, -90)`.
fn clockwise(dir: Dir) -> Dir {
    match dir {
        Dir::North => Dir::East,
        Dir::East => Dir::South,
        Dir::South => Dir::West,
        Dir::West => Dir::North,
        Dir::NorthEast => Dir::SouthEast,
        Dir::SouthEast => Dir::SouthWest,
        Dir::SouthWest => Dir::NorthWest,
        Dir::NorthWest => Dir::NorthEast,
    }
}

fn dir_name(dir: Dir) -> &'static str {
    match dir {
        Dir::North => "север",
        Dir::South => "юг",
        Dir::East => "восток",
        Dir::West => "запад",
        Dir::NorthEast => "северо-восток",
        Dir::NorthWest => "северо-запад",
        Dir::SouthEast => "юго-восток",
        Dir::SouthWest => "юго-запад",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn micro_lasers_speed_up_the_extractor_and_the_harvester() {
        // Two tier 1 lasers (the board's default), then two tier 4.
        assert_eq!(fat_ratings(2), (12.5, 80.0));
        assert_eq!(fat_ratings(8), (27.5, 50.0));
        assert_eq!(harvest_interval(1), 39.0);
        assert_eq!(harvest_interval(100), 1.0);
    }

    #[test]
    fn the_store_becomes_slabs_then_cutlets() {
        // 200 nutrients at 80 per slab: a cookie (>= 160), 2 slabs, 40 left -> 1 cutlet (26.7).
        assert_eq!(meat_yield(200.0, 80.0), (true, 2, 1));
        assert_eq!(meat_yield(20.0, 90.0), (false, 0, 0));
        assert_eq!(meat_yield(60.0, 90.0), (false, 0, 2));
    }

    #[test]
    fn the_harvest_goes_from_the_legs_to_the_chest() {
        let all = harvest_order(|_| true);
        assert_eq!(all.first(), Some(&BodyZone::LLeg));
        assert_eq!(all.last(), Some(&BodyZone::Chest));
        let no_head = harvest_order(|z| z != BodyZone::Head);
        assert!(!no_head.contains(&BodyZone::Head));
        assert_eq!(no_head.len(), 5);
        assert_eq!(clockwise(Dir::South), Dir::West);
    }
}
