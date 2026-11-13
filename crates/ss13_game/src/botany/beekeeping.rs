//! Beekeeping (tg `code/modules/hydroponics/beekeeping/`): the apiary, its frames and
//! honeycombs, the queen bee item and the bee smoker. The bees themselves are mobs
//! (`crate::mobs::bees`).
//!
//! - An apiary (`/obj/structure/beebox`) holds up to 3 honey frames (10 honeycombs each), a
//!   queen and its bees (`bees`: every bee that calls it home, inside or out). Every `SSobj`
//!   fire (2 s) with a queen: 100 resources make a honeycomb while there is room (with the
//!   queen's reagent), and while there are fewer bees than half the honeycomb room a new bee
//!   comes out (always when at most one is left, else 20% for 50 resources). Pollinating bees
//!   bring the resources. Bees inside leave 35%/s (the queen 5%/s).
//! - A frame or a queen bee item goes in by hand; a queen with another reagent drives the
//!   incompatible bees out. An empty hand of someone who is not `bee_friendly` lets every
//!   worker out ("disturbs the bees"); a bee-friendly one picks a frame (scraping up to 10
//!   honeycombs onto the floor) or the queen (back into a queen bee item) out.
//! - Premade apiaries come with a queen, 3 frames and 15 bees (the random one: of a random
//!   reagent). A bought queen bee item holds a queen.
//! - The bee smoker toggles in hand (needs fuel), burns 1 fuel per `SSobj` fire, and for 5 fuel
//!   befriends the bees on the clicked tile, or every bee of a clicked apiary, with the user.
//!   Cannabis refuels it by a fifth of its `wine_power`.
// shortcut: no wrench anchoring or deconstruction into wood, no royal bee jelly or reagent
// splicing of the queen item by syringe, no smoker particles, sound loop or dust effect; the
// queen in an item is kept as her reagent (a new queen mob comes out at full health).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmObj, IconIndex};
use ss13_reagents::is_under;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};

use crate::chem::{Chem, Reagents, name_of, visible};
use crate::combat::CombatRng;
use crate::mobs::bees::{Bee, bee_friendly};
use crate::storage::{put_inside, take_out};
use crate::tools::tell;

pub const BEEBOX: &str = "/obj/structure/beebox";
const BEEBOX_PREMADE: &str = "/obj/structure/beebox/premade";
const BEEBOX_RANDOM: &str = "/obj/structure/beebox/premade/random";
pub const HONEY_FRAME: &str = "/obj/item/honey_frame";
pub const HONEYCOMB: &str = "/obj/item/food/honeycomb";
pub const QUEEN_ITEM: &str = "/obj/item/queen_bee";
const QUEEN_ITEM_BOUGHT: &str = "/obj/item/queen_bee/bought";
pub const QUEEN_MOB: &str = "/mob/living/basic/bee/queen";
pub const SMOKER: &str = "/obj/item/bee_smoker";
const CANNABIS: &str = "/obj/item/food/grown/cannabis";
const HARVEST_DMI: &str = "icons/obj/service/hydroponics/harvest.dmi";
const SPRAY_SOUND: &str = "sound/effects/spray2.ogg";
const TOGGLE_SOUND: &str = "sound/items/tools/welderdeactivate.ogg";
/// `BEEBOX_MAX_FRAMES`, `honeycomb_capacity`, `BEES_RATIO`.
const MAX_FRAMES: usize = 3;
const HONEYCOMB_CAPACITY: usize = 10;
const BEES_RATIO: f32 = 0.5;
/// `BEE_PROB_NEW_BEE`, `BEE_RESOURCE_HONEYCOMB_COST`, `BEE_RESOURCE_NEW_BEE_COST`.
const NEW_BEE_CHANCE: f32 = 20.0;
const HONEYCOMB_COST: f32 = 100.0;
const NEW_BEE_COST: f32 = 50.0;
/// `enter_exit_hive` `exit_chance` (worker, queen), percent per second.
const EXIT: f32 = 35.0;
const EXIT_QUEEN: f32 = 5.0;
/// `set_reagent`: units of the queen's reagent in a honeycomb.
const COMB_REAGENT: f32 = 5.0;
/// The bee smoker: `max_herb_fuel`, `single_use_cost`, `WEED_WINE_MULTIPLIER`.
const SMOKER_MAX_FUEL: f32 = 50.0;
const SMOKER_USE_COST: f32 = 5.0;
const WEED_WINE_MULTIPLIER: f32 = 0.2;
/// `SSobj` `wait`, seconds.
const SSOBJ_WAIT: f64 = 2.0;

/// An apiary: `queen_bee`, `bees` (home of, inside or out), `honeycombs`, `honey_frames`
/// (both inside it) and `bee_resources`.
#[derive(Component, Clone, Debug, Default)]
pub struct Beebox {
    pub queen: Option<Entity>,
    pub bees: Vec<Entity>,
    pub honeycombs: Vec<Entity>,
    pub frames: Vec<Entity>,
    pub resources: f32,
}

impl Beebox {
    /// `get_max_honeycomb`.
    pub fn max_honeycomb(&self) -> usize {
        self.frames.len() * HONEYCOMB_CAPACITY
    }

    /// `get_max_bees`.
    pub fn max_bees(&self) -> usize {
        (self.max_honeycomb() as f32 * BEES_RATIO) as usize
    }
}

/// A queen bee item's queen (`queen`): her reagent. An item without it holds none (unless it
/// is a bought one, whose queen is plain).
#[derive(Component, Clone, Debug, Default)]
pub struct QueenBeeItem {
    pub beegent: Option<String>,
}

/// The bee smoker's `current_herb_fuel` and `activated`.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct BeeSmoker {
    pub fuel: f32,
    pub active: bool,
}

impl Default for BeeSmoker {
    fn default() -> Self {
        Self {
            fuel: SMOKER_MAX_FUEL,
            active: false,
        }
    }
}

/// The "Honey Frame or Queen Bee" question of a bee-friendly hand on an apiary.
#[derive(Component, Clone, Copy, Debug)]
struct ApiaryAsk {
    hive: Entity,
}

pub(super) fn build(app: &mut App) {
    app.add_observer(add_beebox)
        .add_observer(remove_beebox)
        .add_systems(
            Update,
            (process_hives, hive_exits, process_smokers, apiary_answers),
        );
}

fn is_path(world: &World, e: Entity, root: &str) -> bool {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.as_str())
        .or_else(|| world.get::<DmObj>(e).map(|o| o.path.as_str()))
        .is_some_and(|p| is_under(p, root))
}

fn alive(world: &World, e: Entity) -> bool {
    world.get_entity(e).is_ok()
}

/// `Initialize` of an apiary, mapped or built.
fn add_beebox(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(obj) = objs.get(add.entity) else {
        return;
    };
    if !is_under(&obj.path, BEEBOX) {
        return;
    }
    let premade = is_under(&obj.path, BEEBOX_PREMADE);
    let random = is_under(&obj.path, BEEBOX_RANDOM);
    let hive = add.entity;
    commands.queue(move |world: &mut World| {
        if world.get_entity(hive).is_err() {
            return;
        }
        world.entity_mut(hive).insert(Beebox::default());
        if premade {
            fill_premade(world, hive, random);
        }
    });
}

/// `/obj/structure/beebox/premade/Initialize`: a queen, full frames and as many bees as fit,
/// all of one reagent (a random one for the random apiary).
fn fill_premade(world: &mut World, hive: Entity, random: bool) {
    let reagent = random.then(|| random_reagent(world)).flatten();
    for _ in 0..MAX_FRAMES {
        if let Some(frame) = crate::construction::spawn_detached(world, HONEY_FRAME) {
            put_inside(world, frame, hive);
            if let Some(mut h) = world.get_mut::<Beebox>(hive) {
                h.frames.push(frame);
            }
        }
    }
    let Some(&pos) = world.get::<TilePos>(hive) else {
        return;
    };
    let Some(queen) = crate::mobs::spawn::spawn_mob(world, QUEEN_MOB, pos) else {
        return;
    };
    if let Some(r) = &reagent {
        crate::mobs::bees::assign_reagent(world, queen, r);
    }
    adopt(world, hive, queen);
    put_bee_in(world, hive, queen);
    let max = world.get::<Beebox>(hive).map_or(0, Beebox::max_bees);
    for _ in 0..max {
        let Some(bee) = crate::mobs::spawn::spawn_mob(world, crate::mobs::bees::BEE, pos) else {
            break;
        };
        if let Some(r) = &reagent {
            crate::mobs::bees::assign_reagent(world, bee, r);
        }
        if let Some(mut b) = world.get_mut::<Bee>(bee) {
            b.home = Some(hive);
        }
        if let Some(mut h) = world.get_mut::<Beebox>(hive) {
            h.bees.push(bee);
        }
        put_bee_in(world, hive, bee);
    }
}

/// `get_random_reagent_id()`.
fn random_reagent(world: &mut World) -> Option<String> {
    let pool: Vec<String> = world
        .get_resource::<super::Botany>()
        .map(|b| b.0.data.random_reagents.clone())
        .unwrap_or_default();
    if pool.is_empty() {
        return None;
    }
    let i = world
        .resource_mut::<CombatRng>()
        .range(0, pool.len() as u32 - 1) as usize;
    Some(pool[i].clone())
}

/// A gone apiary: its bees are homeless and come out (`atom_deconstruct`); frames and
/// honeycombs went with it.
fn remove_beebox(remove: On<Remove<Beebox>>, hives: Query<&Beebox>, mut commands: Commands) {
    let Ok(hive) = hives.get(remove.entity) else {
        return;
    };
    let bees = hive.bees.clone();
    let id = remove.entity;
    commands.queue(move |world: &mut World| {
        for bee in bees {
            if !alive(world, bee) {
                continue;
            }
            if let Some(mut b) = world.get_mut::<Bee>(bee)
                && b.home == Some(id)
            {
                b.home = None;
            }
            if world
                .get::<crate::closets::InCloset>(bee)
                .is_some_and(|c| c.closet == id)
            {
                crate::closets::let_out(world, bee);
            }
        }
    });
}

/// `habitable(target)`: a bee of the queen's reagent while there is room.
pub fn habitable(world: &World, hive: Entity, bee: Entity) -> bool {
    let (Some(h), Some(_)) = (world.get::<Beebox>(hive), world.get::<Bee>(bee)) else {
        return false;
    };
    !crate::mobs::bees::reagent_incompatible(world, bee, h.queen) && h.bees.len() < h.max_bees()
}

/// The end of `handle_habitation` for a homeless bee: this apiary is its home now.
pub fn adopt(world: &mut World, hive: Entity, bee: Entity) {
    let queen = world.get::<Bee>(bee).is_some_and(|b| b.queen);
    if let Some(mut b) = world.get_mut::<Bee>(bee) {
        b.home = Some(hive);
    }
    if let Some(mut h) = world.get_mut::<Beebox>(hive) {
        if !h.bees.contains(&bee) {
            h.bees.push(bee);
        }
        if queen {
            h.queen = Some(bee);
        }
    }
}

/// `beehome.bees -= src` (and the queen slot) of a dying bee.
pub fn forget_bee(world: &mut World, bee: Entity) {
    let Some(home) = world.get::<Bee>(bee).and_then(|b| b.home) else {
        return;
    };
    if let Some(mut h) = world.get_mut::<Beebox>(home) {
        h.bees.retain(|&b| b != bee);
        if h.queen == Some(bee) {
            h.queen = None;
        }
    }
}

/// `bee_resources = min(bee_resources + growth, 100)`.
pub fn add_resources(world: &mut World, hive: Entity, amount: f32) {
    if let Some(mut h) = world.get_mut::<Beebox>(hive) {
        h.resources = (h.resources + amount).min(100.0);
    }
}

/// `forceMove(beehome)`: hidden inside, on its tile.
pub fn put_bee_in(world: &mut World, hive: Entity, bee: Entity) {
    if let Some(&pos) = world.get::<TilePos>(hive)
        && world.get::<TilePos>(bee) != Some(&pos)
    {
        world.entity_mut(bee).insert(pos);
    }
    crate::closets::put_mob_inside(world, bee, hive);
}

/// `forceMove(get_turf(beehome))`.
pub fn let_bee_out(world: &mut World, hive: Entity, bee: Entity) {
    if world
        .get::<crate::closets::InCloset>(bee)
        .is_some_and(|c| c.closet == hive)
    {
        crate::closets::let_out(world, bee);
    }
}

fn inside(world: &World, hive: Entity, bee: Entity) -> bool {
    world
        .get::<crate::closets::InCloset>(bee)
        .is_some_and(|c| c.closet == hive)
}

/// Drops the bees that are gone or moved out, and a queen that is gone.
fn prune(world: &mut World, hive: Entity) {
    let Some(mut h) = world.get::<Beebox>(hive).cloned() else {
        return;
    };
    let lives_here = |world: &World, b: Entity| {
        world
            .get::<Bee>(b)
            .is_some_and(|bee| bee.home == Some(hive))
    };
    h.bees.retain(|&b| lives_here(world, b));
    h.queen = h.queen.filter(|&q| lives_here(world, q));
    h.frames.retain(|&f| alive(world, f));
    h.honeycombs.retain(|&c| alive(world, c));
    world.entity_mut(hive).insert(h);
}

/// `/obj/structure/beebox/process` every `SSobj` fire.
// Cost: one pass over the apiaries (a handful per map) every 2 s.
fn process_hives(world: &mut World, mut next: Local<f64>) {
    let now = crate::tools::now(world);
    if now < *next {
        return;
    }
    *next = now + SSOBJ_WAIT;
    let hives: Vec<Entity> = world
        .query_filtered::<Entity, With<Beebox>>()
        .iter(world)
        .collect();
    for hive in hives {
        prune(world, hive);
        process_hive(world, hive);
    }
}

fn process_hive(world: &mut World, hive: Entity) {
    let Some(h) = world.get::<Beebox>(hive).cloned() else {
        return;
    };
    let Some(queen) = h.queen else {
        return;
    };
    let beegent = world.get::<Bee>(queen).and_then(|b| b.beegent.clone());
    if h.resources >= HONEYCOMB_COST
        && h.honeycombs.len() < h.max_honeycomb()
        && let Some(comb) = crate::construction::spawn_detached(world, HONEYCOMB)
    {
        put_inside(world, comb, hive);
        world.flush();
        if let Some(r) = &beegent {
            set_comb_reagent(world, comb, r);
        }
        if let Some(mut h) = world.get_mut::<Beebox>(hive) {
            h.resources = (h.resources - HONEYCOMB_COST).max(0.0);
            h.honeycombs.push(comb);
        }
    }
    let Some(h) = world.get::<Beebox>(hive).cloned() else {
        return;
    };
    if h.bees.len() >= h.max_bees() {
        return;
    }
    // "a freebee, geddit?": there is always one set of worker bees.
    let freebee = h.bees.len() <= 1;
    let paid = !freebee
        && h.resources >= NEW_BEE_COST
        && world.resource_mut::<CombatRng>().chance(NEW_BEE_CHANCE);
    if !freebee && !paid {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(hive) else {
        return;
    };
    let Some(bee) = crate::mobs::spawn::spawn_mob(world, crate::mobs::bees::BEE, pos) else {
        return;
    };
    if let Some(r) = &beegent {
        crate::mobs::bees::assign_reagent(world, bee, r);
    }
    if let Some(mut b) = world.get_mut::<Bee>(bee) {
        b.home = Some(hive);
    }
    if let Some(mut h) = world.get_mut::<Beebox>(hive) {
        if paid {
            h.resources = (h.resources - NEW_BEE_COST).max(0.0);
        }
        h.bees.push(bee);
    }
}

/// `honeycomb/set_reagent`: named after the reagent, 5 u of it, the honey overlay in its colour.
fn set_comb_reagent(world: &mut World, comb: Entity, path: &str) {
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let Some(id) = chem.id(path) else {
        return;
    };
    let def = chem.def(id);
    let base = name_of(world, comb);
    world
        .entity_mut(comb)
        .insert(Name::new(format!("{base} ({})", def.name)));
    if let Some(mut r) = world.get_mut::<Reagents>(comb) {
        r.0.add_reagent(&chem, id, COMB_REAGENT, 300.0, None, None, false, false);
    }
    let color = ss13_reagents::parse_color(&def.color);
    let state = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(HARVEST_DMI, "greyscale_honey"));
    if let (Some(state), Some(mut appearance)) = (state, world.get_mut::<Appearance>(comb)) {
        appearance.overlays.push(ss13_shared::Overlay {
            icon_state: state,
            dir: None,
            color,
            pixel_offset: [0, 0],
            plane: None,
            only_dirs: 0,
        });
    }
}

/// `enter_exit_hive` of the bees inside (their AI is off in there): they fly out.
fn hive_exits(world: &mut World, mut next: Local<f64>) {
    let now = crate::tools::now(world);
    if now < *next {
        return;
    }
    *next = now + 1.0;
    let hives: Vec<(Entity, Vec<Entity>)> = world
        .query::<(Entity, &Beebox)>()
        .iter(world)
        .map(|(e, h)| (e, h.bees.clone()))
        .collect();
    for (hive, bees) in hives {
        for bee in bees {
            if !inside(world, hive, bee) {
                continue;
            }
            let queen = world.get::<Bee>(bee).is_some_and(|b| b.queen);
            let chance = if queen { EXIT_QUEEN } else { EXIT };
            if world.resource_mut::<CombatRng>().chance(chance) {
                let_bee_out(world, hive, bee);
            }
        }
    }
}

/// `item_interaction` of an apiary: a frame or a queen bee item. True when it was one.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(h) = world.get::<Beebox>(target).cloned() else {
        return false;
    };
    let user = name_of(world, mob);
    if is_path(world, held, HONEY_FRAME) {
        if h.frames.len() >= MAX_FRAMES {
            tell(
                world,
                client,
                Span::Warning,
                "В улье больше нет места для рамок!",
            );
            return true;
        }
        put_inside(world, held, target);
        if let Some(mut h) = world.get_mut::<Beebox>(target) {
            h.frames.push(held);
        }
        visible(
            world,
            mob,
            "Вы вставляете рамку в улей.",
            &format!("{user} вставляет рамку в улей."),
            Span::Notice,
        );
        return true;
    }
    if !is_path(world, held, QUEEN_ITEM) {
        return false;
    }
    if h.queen.is_some() {
        tell(
            world,
            client,
            Span::Warning,
            "В этом улье уже есть королева!",
        );
        return true;
    }
    let held_queen = queen_of(world, held);
    world.entity_mut(held).remove::<QueenBeeItem>();
    world.despawn(held);
    let Some(beegent) = held_queen else {
        tell(
            world,
            client,
            Span::Warning,
            "Пчелиная королева исчезла! В последнее время в новостях часто пишут о пропаже пчёл...",
        );
        return true;
    };
    let Some(&pos) = world.get::<TilePos>(target) else {
        return true;
    };
    let Some(queen) = crate::mobs::spawn::spawn_mob(world, QUEEN_MOB, pos) else {
        return true;
    };
    if let Some(r) = &beegent {
        crate::mobs::bees::assign_reagent(world, queen, r);
    }
    adopt(world, target, queen);
    put_bee_in(world, target, queen);
    let queen_name = name_of(world, queen);
    visible(
        world,
        mob,
        &format!("Вы сажаете {queen_name} в улей, теперь это её новый дом."),
        &format!("{user} сажает {queen_name} в улей, теперь это её новый дом."),
        Span::Notice,
    );
    let bees = world
        .get::<Beebox>(target)
        .map(|h| h.bees.clone())
        .unwrap_or_default();
    let mut relocated = 0;
    for bee in bees {
        if !crate::mobs::bees::reagent_incompatible(world, bee, Some(queen)) {
            continue;
        }
        if let Some(mut h) = world.get_mut::<Beebox>(target) {
            h.bees.retain(|&b| b != bee);
        }
        if let Some(mut b) = world.get_mut::<Bee>(bee) {
            b.home = None;
        }
        let_bee_out(world, target, bee);
        relocated += 1;
    }
    if relocated > 0 {
        tell(
            world,
            client,
            Span::Warning,
            "У этой королевы другой реагент, чем у некоторых живущих здесь пчёл, и они не вернутся в этот улей!",
        );
    }
    true
}

/// The queen a queen bee item holds: `Some(reagent)`, or `None` for an empty one.
fn queen_of(world: &World, item: Entity) -> Option<Option<String>> {
    if let Some(q) = world.get::<QueenBeeItem>(item) {
        return Some(q.beegent.clone());
    }
    is_path(world, item, QUEEN_ITEM_BOUGHT).then_some(None)
}

/// `interact` of an apiary with an empty hand. True when `target` is one.
pub fn attack_hand(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(h) = world.get::<Beebox>(target).cloned() else {
        return false;
    };
    if bee_friendly(world, mob) {
        let ask = Ask::new(
            client,
            mob,
            "Работа с ульем",
            "Что вы хотите достать?",
            PromptKind::Alert {
                buttons: vec!["Медовую рамку".into(), "Пчелиную королеву".into()],
                autofocus: true,
            },
        );
        let prompt = ask_world(world, ask);
        world.entity_mut(prompt).insert(ApiaryAsk { hive: target });
        return true;
    }
    // "everyone who's ever lived here now instantly hates you".
    let mut attack = false;
    for bee in h.bees {
        if world.get::<Bee>(bee).is_none_or(|b| b.queen) {
            continue;
        }
        let_bee_out(world, target, bee);
        attack = true;
    }
    let user = name_of(world, mob);
    let hive = name_of(world, target);
    let text = if attack {
        format!("{user} тревожит пчёл!")
    } else {
        format!("{user} безрезультатно тревожит {hive}!")
    };
    visible(world, mob, &text, &text, Span::Danger);
    true
}

/// The apiary answer: a frame (and its honeycombs) or the queen, checked again after the wait.
fn apiary_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let Some(messages) = world.get_resource::<Messages<PromptAnswered>>() else {
        return;
    };
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(messages)
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in pending {
        let Some(ask) = world.get::<ApiaryAsk>(prompt).copied() else {
            continue;
        };
        let Some(PromptAnswer::Button(choice)) = answer else {
            continue;
        };
        // `can_perform_action(src, NEED_DEXTERITY)`.
        let near = world
            .get::<TilePos>(mob)
            .zip(world.get::<TilePos>(ask.hive))
            .and_then(|(a, b)| a.chebyshev(*b))
            .is_some_and(|d| d <= 1);
        let conscious =
            world.get::<crate::health::Stat>(mob) == Some(&crate::health::Stat::Conscious);
        if !near || !conscious || world.get::<Beebox>(ask.hive).is_none() {
            continue;
        }
        prune(world, ask.hive);
        if choice == 0 {
            take_frame(world, client, mob, ask.hive);
        } else {
            take_queen(world, client, mob, ask.hive);
        }
    }
}

fn give(world: &mut World, mob: Entity, item: Entity, pos: TilePos) {
    match world.get::<crate::inventory::Inventory>(mob).cloned() {
        Some(inventory) => crate::storage::put_in_hands(world, mob, &inventory, item),
        None => take_out(world, item, pos),
    }
}

/// "Honey Frame": a random frame into the hand, as many honeycombs as it holds onto the floor.
fn take_frame(world: &mut World, client: Entity, mob: Entity, hive: Entity) {
    let Some(&pos) = world.get::<TilePos>(hive) else {
        return;
    };
    let Some(mut h) = world.get::<Beebox>(hive).cloned() else {
        return;
    };
    if h.frames.is_empty() {
        tell(world, client, Span::Warning, "В улье нет медовых рамок!");
        return;
    }
    let mut rng = CombatRng(world.resource::<CombatRng>().0);
    let frame = h
        .frames
        .remove(rng.range(0, h.frames.len() as u32 - 1) as usize);
    let mut combs = Vec::new();
    while !h.honeycombs.is_empty() && combs.len() < HONEYCOMB_CAPACITY {
        let i = rng.range(0, h.honeycombs.len() as u32 - 1) as usize;
        combs.push(h.honeycombs.remove(i));
    }
    world.resource_mut::<CombatRng>().0 = rng.0;
    world.entity_mut(hive).insert(h);
    give(world, mob, frame, pos);
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        "Вы достаёте рамку из улья.",
        &format!("{user} достаёт рамку из улья."),
        Span::Notice,
    );
    for &comb in &combs {
        take_out(world, comb, pos);
    }
    if !combs.is_empty() {
        crate::construction::tile_changed(world, pos);
        let what = if combs.len() > 1 {
            format!("{} сот", combs.len())
        } else {
            "соты".to_owned()
        };
        visible(
            world,
            mob,
            &format!("Вы соскребаете {what} с рамки."),
            &format!("{user} соскребает {what} с рамки."),
            Span::Notice,
        );
    }
}

/// "Queen Bee": the queen inside into a queen bee item.
fn take_queen(world: &mut World, client: Entity, mob: Entity, hive: Entity) {
    let Some(&pos) = world.get::<TilePos>(hive) else {
        return;
    };
    let queen = world
        .get::<Beebox>(hive)
        .and_then(|h| h.queen)
        .filter(|&q| inside(world, hive, q));
    let Some(queen) = queen else {
        tell(world, client, Span::Warning, "Здесь нет пчелиной королевы!");
        return;
    };
    let Some(item) = crate::construction::spawn_detached(world, QUEEN_ITEM) else {
        return;
    };
    let beegent = world.get::<Bee>(queen).and_then(|b| b.beegent.clone());
    let queen_name = name_of(world, queen);
    if let Some(mut h) = world.get_mut::<Beebox>(hive) {
        h.bees.retain(|&b| b != queen);
        h.queen = None;
    }
    world.despawn(queen);
    world
        .entity_mut(item)
        .insert((QueenBeeItem { beegent }, Name::new(queen_name)));
    give(world, mob, item, pos);
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        "Вы достаёте королеву из улья.",
        &format!("{user} достаёт королеву из улья."),
        Span::Notice,
    );
}

/// tg `/obj/structure/beebox/examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(h) = world.get::<Beebox>(target) else {
        return;
    };
    if h.queen.is_none() {
        lines.push((
            Span::Warning,
            "Здесь нет пчелиной королевы! Без королевы сот не будет!".to_owned(),
        ));
    }
    let half = h.max_bees() as f32 * 0.5;
    if half > 0.0 && h.bees.len() as f32 >= half {
        lines.push((
            Span::Notice,
            "Здесь всё ЖУЖЖИТ от активности... тут полно пчёл!".to_owned(),
        ));
    }
    let r = crate::chem::dm_num(h.resources);
    lines.push((Span::Notice, format!("Запас ресурсов: {r}/100.")));
    lines.push((Span::Notice, format!("{r}% до новых сот.")));
    lines.push((
        Span::Notice,
        format!(
            "{}% до новой пчелы.",
            crate::chem::dm_num(h.resources * 2.0)
        ),
    ));
    if !h.honeycombs.is_empty() {
        lines.push((
            Span::Notice,
            format!("Несобранных сот в улье: {}.", h.honeycombs.len()),
        ));
    }
    if h.honeycombs.len() >= h.max_honeycomb() {
        lines.push((Span::Warning, "Для новых сот нет места!".to_owned()));
    }
}

fn smoker(world: &World, item: Entity) -> Option<BeeSmoker> {
    is_path(world, item, SMOKER).then(|| world.get::<BeeSmoker>(item).copied().unwrap_or_default())
}

fn sound_at(world: &mut World, e: Entity, sound: &str, vol: f32) {
    if let Some(pos) = crate::explosions::turf_of(world, e) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, sound, vol, true));
    }
}

/// `bee_smoker/attack_self`. True when `item` is a smoker.
pub fn smoker_attack_self(world: &mut World, mob: Entity, item: Entity) -> bool {
    let Some(mut s) = smoker(world, item) else {
        return false;
    };
    if !s.active && s.fuel <= 0.0 {
        crate::balloon::balloon_alert(world, mob, mob, "нет топлива!");
        return true;
    }
    s.active = !s.active;
    world.entity_mut(item).insert(s);
    sound_at(world, item, TOGGLE_SOUND, 50.0);
    let text = if s.active {
        "включён"
    } else {
        "выключен"
    };
    crate::balloon::balloon_alert(world, mob, mob, text);
    true
}

/// `bee_smoker/interact_with_atom` on an apiary, a bee or a floor: the bees on that tile
/// (or of the apiary) befriend the user. True when the click was the smoker's.
pub fn smoker_use(world: &mut World, mob: Entity, held: Entity, target: Entity) -> bool {
    let Some(mut s) = smoker(world, held) else {
        return false;
    };
    let smokable = world.get::<Beebox>(target).is_some()
        || world.get::<Bee>(target).is_some()
        || world.get::<ss13_mapload::Turf>(target).is_some();
    if !smokable {
        return false;
    }
    if !s.active {
        crate::balloon::balloon_alert(world, mob, mob, "не включён!");
        return true;
    }
    if s.fuel < SMOKER_USE_COST {
        crate::balloon::balloon_alert(world, mob, mob, "недостаточно топлива!");
        return true;
    }
    s.fuel -= SMOKER_USE_COST;
    world.entity_mut(held).insert(s);
    sound_at(world, held, SPRAY_SOUND, 100.0);
    let Some(pos) = crate::explosions::turf_of(world, target) else {
        return true;
    };
    let mut bees: Vec<Entity> = world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| {
            world.get::<Bee>(e).is_some() && world.get::<crate::closets::InCloset>(e).is_none()
        })
        .collect();
    if let Some(h) = world.get::<Beebox>(target) {
        bees.extend(h.bees.iter().copied());
    }
    for bee in bees {
        if alive(world, bee) {
            crate::mobs::pets::befriend(world, bee, mob);
        }
    }
    true
}

/// `bee_smoker/item_interaction` with cannabis: fuel by its `wine_power`. True when it was.
pub fn smoker_refuel(world: &mut World, mob: Entity, held: Entity, target: Entity) -> bool {
    let Some(mut s) = smoker(world, target) else {
        return false;
    };
    let Some(power) = world
        .get::<DmItem>(held)
        .and_then(|i| cannabis_wine_power(&i.path))
    else {
        return false;
    };
    if s.fuel >= SMOKER_MAX_FUEL {
        crate::balloon::balloon_alert(world, mob, mob, "топливо уже на максимуме!");
        return true;
    }
    s.fuel = (s.fuel + power * WEED_WINE_MULTIPLIER).min(SMOKER_MAX_FUEL);
    world.entity_mut(target).insert(s);
    crate::balloon::balloon_alert(world, mob, mob, "топливо добавлено");
    world.despawn(held);
    true
}

/// `wine_power` of the cannabis kinds (`grown/cannabis.dm`).
fn cannabis_wine_power(path: &str) -> Option<f32> {
    if !is_under(path, CANNABIS) {
        return None;
    }
    let sub = |s: &str| is_under(path, &format!("{CANNABIS}/{s}"));
    Some(if sub("rainbow") {
        60.0
    } else if sub("death") {
        40.0
    } else if sub("white") {
        10.0
    } else if sub("ultimate") {
        90.0
    } else {
        20.0
    })
}

/// `bee_smoker/process`: 1 fuel per `SSobj` fire while on; off when it runs out.
fn process_smokers(world: &mut World, mut next: Local<f64>) {
    let now = crate::tools::now(world);
    if now < *next {
        return;
    }
    *next = now + SSOBJ_WAIT;
    let lit: Vec<Entity> = world
        .query::<(Entity, &BeeSmoker)>()
        .iter(world)
        .filter(|(_, s)| s.active)
        .map(|(e, _)| e)
        .collect();
    for e in lit {
        let Some(mut s) = world.get::<BeeSmoker>(e).copied() else {
            continue;
        };
        s.fuel -= 1.0;
        if s.fuel <= 0.0 {
            s.active = false;
            world.entity_mut(e).insert(s);
            sound_at(world, e, TOGGLE_SOUND, 50.0);
        } else {
            world.entity_mut(e).insert(s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_follows_frames() {
        let mut world = World::new();
        let frames: Vec<Entity> = (0..3).map(|_| world.spawn_empty().id()).collect();
        let h = Beebox {
            frames,
            ..default()
        };
        assert_eq!(h.max_honeycomb(), 30);
        assert_eq!(h.max_bees(), 15);
        assert_eq!(Beebox::default().max_bees(), 0);
    }

    fn bee(world: &mut World, beegent: Option<&str>, queen: bool) -> Entity {
        let mut b = Bee::default();
        b.beegent = beegent.map(str::to_owned);
        b.queen = queen;
        world.spawn(b).id()
    }

    #[test]
    fn hive_takes_bees_of_its_queen_while_there_is_room() {
        let mut world = World::new();
        let frame = world.spawn_empty().id();
        let hive = world
            .spawn(Beebox {
                frames: vec![frame],
                ..default()
            })
            .id();
        let queen = bee(&mut world, Some("/datum/reagent/water"), true);
        adopt(&mut world, hive, queen);
        assert_eq!(world.get::<Beebox>(hive).unwrap().queen, Some(queen));
        assert_eq!(world.get::<Bee>(queen).unwrap().home, Some(hive));
        let plain = bee(&mut world, None, false);
        assert!(!habitable(&world, hive, plain), "another reagent");
        let workers: Vec<Entity> = (0..4)
            .map(|_| bee(&mut world, Some("/datum/reagent/water"), false))
            .collect();
        for &w in &workers {
            assert!(habitable(&world, hive, w));
            adopt(&mut world, hive, w);
        }
        // One frame: room for 5 bees, the queen and four workers.
        let late = bee(&mut world, Some("/datum/reagent/water"), false);
        assert!(!habitable(&world, hive, late));
        forget_bee(&mut world, queen);
        let h = world.get::<Beebox>(hive).unwrap();
        assert_eq!(h.queen, None);
        assert_eq!(h.bees, workers);
    }

    #[test]
    fn cannabis_refuels_the_smoker() {
        let mut world = World::new();
        let user = world.spawn_empty().id();
        let smoker = world
            .spawn((
                DmItem {
                    path: SMOKER.to_owned(),
                    ..default()
                },
                BeeSmoker {
                    fuel: 10.0,
                    active: false,
                },
            ))
            .id();
        let weed = world
            .spawn(DmItem {
                path: "/obj/item/food/grown/cannabis/ultimate".to_owned(),
                ..default()
            })
            .id();
        assert!(smoker_refuel(&mut world, user, weed, smoker));
        assert_eq!(world.get::<BeeSmoker>(smoker).unwrap().fuel, 28.0);
        assert!(world.get_entity(weed).is_err());
    }

    #[test]
    fn cannabis_fuel() {
        assert_eq!(cannabis_wine_power(CANNABIS), Some(20.0));
        assert_eq!(
            cannabis_wine_power("/obj/item/food/grown/cannabis/ultimate"),
            Some(90.0)
        );
        assert_eq!(cannabis_wine_power("/obj/item/food/grown/apple"), None);
    }
}
