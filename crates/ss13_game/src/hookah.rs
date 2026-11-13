//! Hookahs (server), after BandaStation `modular_bandastation/bar_hookahs/code/bar_hookahs.dm`
//! (`/obj/item/hookah`, `/obj/item/hookah_mouthpiece`, `/obj/item/hookah_coals`). The coals
//! sold by cigarette vendors and the cargo kit are data (`ss13_mapload::vending`, `cargo`).
//!
//! - A hookah has a 50 u bowl (`TRANSPARENT`) and its mouthpiece inside. A right click with an
//!   empty hand on a hookah not carried puts the mouthpiece in the hand; the mouthpiece goes
//!   back when it leaves the hand, when its holder is more than a tile from the hookah or off a
//!   turf, when it is used on the hookah, and when the hookah is picked up.
//! - Used on the hookah: coals (30 fuel, at most 30), food (at most five into the bowl), a
//!   reagent container (its transfer amount, not pills), a hot thing (lights it: coals needed).
//! - Lit: 1 fuel per 30 s (none left: it goes out), the smoke settles by 0.1 a second; light 2.
//! - The mouthpiece used in hand (a human) or on oneself while lit: after 2 s an inhale of 3 u
//!   (`inhale_smoke`): the bowl's food gives its reagents to the bowl, anything not a grown
//!   plant or cheese makes black smoke instead; the bowl's reagents are breathed in (`INHALE`)
//!   by the smoke's quality, a cough and stamina loss when too soon or too much, a taste line,
//!   the rest puffed out as smoke a second later.
//! - Alt+right click (a human): extinguish (2 s), a deep draw (5 s, holding the mouthpiece:
//!   6 u at full strength) and clearing the bowl's liquid (2 s), as tg's radial.
//! - Thrown and landing (not caught) it shatters: a shard, the bowl and the food spilled.
// shortcut: no smoke particles, no beam between the hookah and the mouthpiece, no light while
// carried or stored; the mouthpiece returns by a per-tick check of the few held ones instead
// of `COMSIG_MOVABLE_MOVED` (no move signal here). Integrity damage other than a landing does
// not shatter it.

use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmItem, IconIndex};
use ss13_reagents::{Holder, Owner, flags, is_under};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, EMISSIVE_PLANE, IconStateId, LightSource, Optics, Overlay, TilePos};
use ss13_tgui::prompt::{PromptAnswer, PromptAnswered, PromptSystems, RadialChoice};
use ss13_tgui::server::UiSystems;

use crate::chem::{Reagents, TransferAmount, name_of, tell_mob, visible};
use crate::combat::CombatRng;
use crate::construction::Job;
use crate::inventory::{CarriedBy, Inventory};
use crate::sound::{PlaySound, playsound};
use crate::storage::{StoredIn, put_inside};
use crate::tools::{Actor, DoAfter, do_after};

pub const HOOKAH: &str = "/obj/item/hookah";
pub const MOUTHPIECE: &str = "/obj/item/hookah_mouthpiece";
const COALS: &str = "/obj/item/hookah_coals";
const FOOD_ROOT: &str = "/obj/item/food";
const CONTAINER_ROOT: &str = "/obj/item/reagent_containers";
const PILL_ROOT: &str = "/obj/item/reagent_containers/applicator/pill";
/// `allowed_ingridients`.
const SAFE_FOODS: [&str; 2] = ["/obj/item/food/grown", "/obj/item/food/cheese"];
const SHARD: &str = "/obj/item/shard";
const RADIAL_DMI: &str = "modular_bandastation/bar_hookahs/icons/radial.dmi";
const BUBBLES: &str = "sound/effects/bubbles/bubbles.ogg";
/// `SFX_SHATTER`.
const SHATTER: &str = "shatter";

const INTERNAL_VOLUME: f32 = 50.0;
const MAX_FUEL: u32 = 30;
const FUEL_PER_COAL: u32 = 30;
const FUEL_CONSUME_INTERVAL: f64 = 30.0;
const SMOKE_CONSUME_INTERVAL: f64 = 1.0;
const SMOKE_CONSUME_AMOUNT: f32 = 0.1;
const INHALE_COOLDOWN: f64 = 5.0;
const BASE_COUGH_STAMINA_LOSS: f32 = 5.0;
const BASE_INHALE_VOLUME: f32 = 3.0;
const BASE_INHALE_LIMIT: f32 = 3.0;
const MAX_FOOD_ITEMS: usize = 5;
/// `LIGHT_COLOR_ORANGE`.
const LIGHT_COLOR_ORANGE: [u8; 3] = [0xFA, 0x96, 0x32];

const OPTION_CLEAR: &str = "Очистить чашу";
const OPTION_EXTINGUISH: &str = "Погасить угли";
const OPTION_BLOW: &str = "Раскурить";

/// A hookah's state.
#[derive(Component, Clone, Debug, Default)]
pub struct Hookah {
    pub mouthpiece: Option<Entity>,
    pub fuel: u32,
    pub lit: bool,
    /// `food_items`, inside the hookah.
    pub food: Vec<Entity>,
    /// `smoke_amount`, 0..=100.
    pub smoke: f32,
    next_fuel: f64,
    next_smoke: f64,
    /// `delayed_puff` timers: when, who, how much.
    puffs: Vec<(f64, Entity, f32)>,
}

/// On a mouthpiece: its hookah (`source_hookah`) and `inhale_cooldown`.
#[derive(Component, Clone, Copy, Debug)]
pub struct Mouthpiece {
    pub hookah: Entity,
    inhale_ready: f64,
}

/// The hookah's timed actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookahJob {
    /// `start_inhale` through this mouthpiece.
    Inhale(Entity),
    Extinguish(Entity),
    /// The deep draw of this hookah.
    Blow(Entity),
    Clear(Entity),
}

/// The Alt+right click radial: the options in the slices' order.
#[derive(Component, Clone, Debug)]
struct HookahAsk {
    hookah: Entity,
    options: Vec<&'static str>,
}

pub struct HookahPlugin;

impl Plugin for HookahPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::HeldOnTarget, 239, |w, c| {
            c.held
                .is_some_and(|h| held_on(w, c.client, c.mob, h, c.target))
        });
        app.add_click_handler(ClickPhase::OtherHand, 419, |w, c| {
            c.held
                .is_some_and(|h| held_on(w, c.client, c.mob, h, c.target))
        });
        app.add_click_handler(ClickPhase::AttackSelf, 3369, |w, c| {
            mouthpiece_attack_self(w, c.client, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::EmptyHand, 569, |w, c| {
            c.modifiers.right && attack_hand_secondary(w, c.mob, c.target)
        });
        app.add_observer(setup)
            .add_observer(mouthpiece_dropped)
            .add_observer(hookah_picked_up)
            .add_systems(
                Update,
                (
                    process,
                    keep_in_reach.after(ss13_world::WorldSystems::Movement),
                    radial_answers
                        .in_set(UiSystems::Handle)
                        .after(PromptSystems::Answer),
                ),
            );
    }
}

fn path_of(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmItem>(e).map(|i| i.path.as_str())
}

fn is(world: &World, e: Entity, root: &str) -> bool {
    path_of(world, e).is_some_and(|p| is_under(p, root))
}

fn now(world: &World) -> f64 {
    world.resource::<Time>().elapsed_secs_f64()
}

fn is_human(world: &World, mob: Entity) -> bool {
    world.get::<crate::human::HumanLook>(mob).is_some()
}

/// `Initialize`: the bowl and the mouthpiece inside.
fn setup(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let hookah = add.entity;
    if !items.get(hookah).is_ok_and(|i| is_under(&i.path, HOOKAH)) {
        return;
    }
    commands.queue(move |world: &mut World| {
        if world.get_entity(hookah).is_err() || world.get::<Hookah>(hookah).is_some() {
            return;
        }
        let owner = Owner {
            path: HOOKAH.to_owned(),
            living: false,
        };
        let mouthpiece = crate::construction::spawn_detached(world, MOUTHPIECE);
        world.entity_mut(hookah).insert((
            Reagents(Holder::new(INTERNAL_VOLUME, flags::TRANSPARENT, owner)),
            Hookah {
                mouthpiece,
                ..default()
            },
        ));
        if let Some(m) = mouthpiece {
            world.entity_mut(m).insert(Mouthpiece {
                hookah,
                inhale_ready: 0.0,
            });
            put_inside(world, m, hookah);
        }
        refresh(world, hookah);
    });
}

fn mouthpiece_home(world: &World, hookah: Entity) -> bool {
    world
        .get::<Hookah>(hookah)
        .and_then(|h| h.mouthpiece)
        .is_some_and(|m| world.get::<StoredIn>(m).is_some_and(|s| s.0 == hookah))
}

/// `update_overlays` and the light.
fn refresh(world: &mut World, hookah: Entity) {
    let Some(state) = world.get::<Hookah>(hookah).cloned() else {
        return;
    };
    let home = mouthpiece_home(world, hookah);
    let icon = world.get::<DmAtom>(hookah).map(|a| a.icon.clone());
    let id = |world: &World, s: &str| -> Option<IconStateId> {
        world.get_resource::<IconIndex>()?.get(icon.as_ref()?, s)
    };
    let all: Vec<IconStateId> = ["pipe", "coal", "coal_lit", "lit_overlay"]
        .into_iter()
        .filter_map(|s| id(world, s))
        .collect();
    let mut new = Vec::new();
    for (on, s) in [
        (home, "pipe"),
        (state.fuel > 0, "coal"),
        (state.lit, "coal_lit"),
    ] {
        if on && let Some(i) = id(world, s) {
            new.push(Overlay::new(i));
        }
    }
    if state.lit
        && let Some(i) = id(world, "lit_overlay")
    {
        new.push(Overlay {
            plane: Some(EMISSIVE_PLANE),
            ..Overlay::new(i)
        });
    }
    if let Some(mut look) = world.get_mut::<Appearance>(hookah) {
        look.overlays.retain(|o| !all.contains(&o.icon_state));
        look.overlays.extend(new);
    }
    let light = state.lit.then_some(LightSource {
        range: 2.0,
        power: 1.0,
        color: LIGHT_COLOR_ORANGE,
        height: 1.0,
        angle: 360,
        dir: ss13_shared::Dir::South,
        offset: [0, 0],
    });
    let mut optics = world.get::<Optics>(hookah).cloned().unwrap_or_default();
    if optics.light != light {
        optics.light = light;
        world.entity_mut(hookah).insert(optics);
    }
}

/// `return_mouthpiece`: back inside its hookah. False when it already is or is not this
/// hookah's.
fn return_mouthpiece(world: &mut World, mouthpiece: Entity) -> bool {
    let Some(hookah) = world.get::<Mouthpiece>(mouthpiece).map(|m| m.hookah) else {
        return false;
    };
    if world.get::<Hookah>(hookah).and_then(|h| h.mouthpiece) != Some(mouthpiece)
        || mouthpiece_home(world, hookah)
    {
        return false;
    }
    if let Some(holder) = world.get::<CarriedBy>(mouthpiece).map(|c| c.0)
        && let Some(mut inv) = world.get_mut::<Inventory>(holder)
        && let Some(slot) = inv.slot_of(mouthpiece)
    {
        *inv.slot_mut(slot) = None;
    }
    put_inside(world, mouthpiece, hookah);
    refresh(world, hookah);
    true
}

/// `/obj/item/hookah_mouthpiece/dropped`: out of the hand, it goes home.
fn mouthpiece_dropped(
    remove: On<Remove<CarriedBy>>,
    mouthpieces: Query<&Mouthpiece>,
    mut commands: Commands,
) {
    let mouthpiece = remove.entity;
    if mouthpieces.get(mouthpiece).is_err() {
        return;
    }
    commands.queue(move |world: &mut World| {
        if world.get_entity(mouthpiece).is_ok() && world.get::<CarriedBy>(mouthpiece).is_none() {
            return_mouthpiece(world, mouthpiece);
        }
    });
}

/// `/obj/item/hookah/pickup`: the mouthpiece comes back.
fn hookah_picked_up(
    add: On<Add<CarriedBy>>,
    hookahs: Query<(&Hookah, &CarriedBy)>,
    mut commands: Commands,
) {
    let Ok((state, &CarriedBy(user))) = hookahs.get(add.entity) else {
        return;
    };
    let Some(mouthpiece) = state.mouthpiece else {
        return;
    };
    let hookah = add.entity;
    commands.queue(move |world: &mut World| {
        if world.get_entity(mouthpiece).is_ok() && return_mouthpiece(world, mouthpiece) {
            let text = format!("Мундштук возвращается в {}.", name_of(world, hookah));
            tell_mob(world, user, Span::Notice, &text);
        }
    });
}

/// `check_distance` on the holder's moves: more than a tile from the hookah, or not on a turf,
/// and the mouthpiece is let go.
// Cost: one pass over the mouthpieces (a handful) per tick.
fn keep_in_reach(world: &mut World) {
    let mut q = world.query::<(Entity, &Mouthpiece, &CarriedBy)>();
    let held: Vec<(Entity, Entity, Entity)> =
        q.iter(world).map(|(e, m, c)| (e, m.hookah, c.0)).collect();
    for (mouthpiece, hookah, holder) in held {
        let holder_pos = world.get::<TilePos>(holder).copied();
        let hookah_pos = crate::explosions::turf_of(world, hookah);
        let near = match (holder_pos, hookah_pos) {
            (Some(a), Some(b)) => a.chebyshev(b).is_some_and(|d| d <= 1),
            _ => false,
        };
        if near {
            continue;
        }
        let text = format!("Вы отпускаете {}.", name_of(world, hookah));
        tell_mob(world, holder, Span::Warning, &text);
        // `dropItemToGround`, whose `dropped` sends it home.
        if !return_mouthpiece(world, mouthpiece) {
            world.entity_mut(mouthpiece).remove::<CarriedBy>();
        }
    }
}

/// `attack_hand_secondary`: the mouthpiece into the hand of whoever right-clicks a hookah not
/// carried.
fn attack_hand_secondary(world: &mut World, mob: Entity, hookah: Entity) -> bool {
    let Some(state) = world.get::<Hookah>(hookah) else {
        return false;
    };
    let mouthpiece = state.mouthpiece;
    if world.get::<CarriedBy>(hookah).is_some() {
        return true;
    }
    let Some(mouthpiece) = mouthpiece.filter(|_| mouthpiece_home(world, hookah)) else {
        crate::balloon::balloon_alert(world, mob, hookah, "уже занято!");
        return true;
    };
    let Some(inventory) = world.get::<Inventory>(mob).cloned() else {
        return true;
    };
    crate::storage::put_in_hands(world, mob, &inventory, mouthpiece);
    if world.get::<CarriedBy>(mouthpiece).is_none() {
        // No free hand: `put_in_hands` dropped it, so it goes home.
        return_mouthpiece(world, mouthpiece);
        return true;
    }
    let text = format!("Вы берёте {} в руку.", name_of(world, hookah));
    tell_mob(world, mob, Span::Notice, &text);
    refresh(world, hookah);
    true
}

/// `item_interaction` of `held` on a hookah, or a mouthpiece on its user (`attack`).
fn held_on(world: &mut World, client: Entity, mob: Entity, held: Entity, target: Entity) -> bool {
    if target == mob {
        return match world.get::<Mouthpiece>(held).copied() {
            Some(m) if world.get::<Hookah>(m.hookah).is_some_and(|h| h.lit) => {
                start_inhale(world, client, mob, held);
                true
            }
            _ => false,
        };
    }
    if world.get::<Hookah>(target).is_none() {
        return false;
    }
    let hookah = target;
    let hookah_name = name_of(world, hookah);
    if world
        .get::<Mouthpiece>(held)
        .is_some_and(|m| m.hookah == hookah)
    {
        return_mouthpiece(world, held);
        return true;
    }
    if is(world, held, COALS) {
        let fuel = world.get::<Hookah>(hookah).map_or(0, |h| h.fuel);
        if fuel + FUEL_PER_COAL > MAX_FUEL {
            let text = format!("В {hookah_name} уже достаточно углей!");
            tell_mob(world, mob, Span::Warning, &text);
            return true;
        }
        world.get_mut::<Hookah>(hookah).expect("checked").fuel += FUEL_PER_COAL;
        world.despawn(held);
        let text = format!("Вы добавляете угли в {hookah_name}.");
        tell_mob(world, mob, Span::Notice, &text);
        refresh(world, hookah);
        return true;
    }
    if is(world, held, FOOD_ROOT) {
        if world.get::<Hookah>(hookah).map_or(0, |h| h.food.len()) >= MAX_FOOD_ITEMS {
            let text = format!("В {hookah_name} уже достаточно ингридиентов!");
            tell_mob(world, mob, Span::Warning, &text);
            return true;
        }
        let food_name = name_of(world, held);
        put_inside(world, held, hookah);
        world
            .get_mut::<Hookah>(hookah)
            .expect("checked")
            .food
            .push(held);
        let text = format!("Вы добавляете {food_name} в {hookah_name}.");
        tell_mob(world, mob, Span::Notice, &text);
        return true;
    }
    if is(world, held, CONTAINER_ROOT) {
        add_reagents(world, mob, held, hookah);
        return true;
    }
    try_light(world, mob, held, hookah)
}

/// `add_reagents`.
fn add_reagents(world: &mut World, mob: Entity, container: Entity, hookah: Entity) {
    if is(world, container, PILL_ROOT) {
        return;
    }
    if world
        .get::<Reagents>(container)
        .is_none_or(|r| r.0.total_volume <= 0.0)
    {
        let text = format!("Внутри {} ничего нет!", name_of(world, container));
        tell_mob(world, mob, Span::Warning, &text);
        return;
    }
    let amount = world.get::<TransferAmount>(container).map_or(5.0, |t| t.0);
    let moved = crate::chem::transfer(world, container, hookah, amount);
    let hookah_name = name_of(world, hookah);
    if moved <= 0.0 {
        tell_mob(
            world,
            mob,
            Span::Warning,
            &format!("В {hookah_name} нет места!"),
        );
        return;
    }
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        &format!(
            "Вы переливаете {} единиц жидкости в {hookah_name}.",
            crate::chem::dm_num(moved)
        ),
        &format!("{user} переливает что-то в {hookah_name}."),
        Span::Notice,
    );
}

/// `try_light`: false when `tool` is not hot (the click goes on).
fn try_light(world: &mut World, mob: Entity, tool: Entity, hookah: Entity) -> bool {
    let (user, hookah_name) = (name_of(world, mob), name_of(world, hookah));
    let Some(msg) = crate::smoking::ignition_text(world, tool, &user, &hookah_name) else {
        return false;
    };
    let Some((lit, fuel)) = world.get::<Hookah>(hookah).map(|h| (h.lit, h.fuel)) else {
        return false;
    };
    if lit {
        let text = format!("{} уже зажжён!", ss13_shared::capitalize(&hookah_name));
        tell_mob(world, mob, Span::Warning, &text);
        return true;
    }
    if fuel == 0 {
        tell_mob(
            world,
            mob,
            Span::Warning,
            &format!("В {hookah_name} нет углей!"),
        );
        return true;
    }
    visible(world, mob, &msg, &msg, Span::Notice);
    ignite(world, hookah);
    true
}

/// `ignite`.
pub fn ignite(world: &mut World, hookah: Entity) {
    let t = now(world);
    {
        let Some(mut state) = world.get_mut::<Hookah>(hookah) else {
            return;
        };
        state.lit = true;
        state.smoke = 30.0;
        state.next_fuel = t + FUEL_CONSUME_INTERVAL;
        state.next_smoke = t + SMOKE_CONSUME_INTERVAL;
    }
    let text = format!("Угли внутри {} медленно багровеют.", name_of(world, hookah));
    visible(world, hookah, &text, &text, Span::Notice);
    refresh(world, hookah);
}

/// `put_out`.
pub fn put_out(world: &mut World, hookah: Entity) {
    {
        let Some(mut state) = world.get_mut::<Hookah>(hookah) else {
            return;
        };
        state.lit = false;
        state.smoke = 0.0;
    }
    let text = format!(
        "Угли внутри {} возвращают свой привычный цвет.",
        name_of(world, hookah)
    );
    visible(world, hookah, &text, &text, Span::Notice);
    refresh(world, hookah);
}

/// `process` (fuel and smoke by their cooldowns) and the `delayed_puff` timers.
// Cost: one pass over the hookahs (a handful) per tick.
fn process(world: &mut World) {
    let t = now(world);
    let mut q = world.query::<(Entity, &Hookah)>();
    let hookahs: Vec<Entity> = q
        .iter(world)
        .filter(|(_, h)| h.lit || !h.puffs.is_empty())
        .map(|(e, _)| e)
        .collect();
    for hookah in hookahs {
        let mut out = false;
        let due: Vec<(Entity, f32)> = {
            let mut state = world.get_mut::<Hookah>(hookah).expect("queried");
            if state.lit {
                if t >= state.next_fuel {
                    state.fuel = state.fuel.saturating_sub(1);
                    state.next_fuel = t + FUEL_CONSUME_INTERVAL;
                    out = state.fuel == 0;
                }
                if t >= state.next_smoke && state.smoke - SMOKE_CONSUME_AMOUNT >= 0.0 {
                    state.smoke -= SMOKE_CONSUME_AMOUNT;
                    state.next_smoke = t + SMOKE_CONSUME_INTERVAL;
                }
            }
            let (due, later): (Vec<_>, Vec<_>) =
                state.puffs.drain(..).partition(|(at, ..)| *at <= t);
            state.puffs = later;
            due.into_iter()
                .map(|(_, user, amount)| (user, amount))
                .collect()
        };
        if out {
            put_out(world, hookah);
            refresh(world, hookah);
        }
        for (user, amount) in due {
            delayed_puff(world, hookah, user, amount);
        }
    }
}

/// `delayed_puff`: what was not breathed in leaves as quick chemical smoke at the user.
fn delayed_puff(world: &mut World, hookah: Entity, user: Entity, amount: f32) {
    let (Some(pos), Some(carry)) = (
        world.get::<TilePos>(user).copied(),
        world.get::<Reagents>(hookah).map(|r| r.0.clone()),
    ) else {
        return;
    };
    crate::effects::smoke::do_chem_smoke(
        world,
        (amount / 5.0) as u32,
        pos,
        Some((amount * 0.2) as u32),
        &carry,
        crate::effects::smoke::SmokeKind::ChemQuick,
    );
}

/// `/obj/item/hookah_mouthpiece/attack_self`: a human draws while the hookah is lit.
fn mouthpiece_attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    let Some(m) = world.get::<Mouthpiece>(item).copied() else {
        return false;
    };
    if !world.get::<Hookah>(m.hookah).is_some_and(|h| h.lit) || !is_human(world, mob) {
        return false;
    }
    start_inhale(world, client, mob, item);
    true
}

/// Where a `do_after` on `thing` is anchored: the thing on a turf, else the user.
fn anchor(world: &World, mob: Entity, thing: Entity) -> Entity {
    if world.get::<TilePos>(thing).is_some() {
        thing
    } else {
        mob
    }
}

/// `start_inhale`.
fn start_inhale(world: &mut World, client: Entity, mob: Entity, mouthpiece: Entity) {
    let what = name_of(world, mouthpiece);
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        "Вы затягиваетесь...",
        &format!("{user} затягивается из {what}."),
        Span::Notice,
    );
    let target = anchor(world, mob, mouthpiece);
    do_after(
        world,
        Actor {
            client,
            mob,
            item: Some(mouthpiece),
        },
        target,
        20.0,
        0,
        Job::Hookah(HookahJob::Inhale(mouthpiece)),
    );
}

/// `inhale_smoke(user, amount, skip_calculations)`.
pub fn inhale_smoke(world: &mut World, mob: Entity, mouthpiece: Entity, amount: f32, full: bool) {
    if crate::species::has_trait(world, mob, crate::species::Trait::NoBreath) {
        tell_mob(world, mob, Span::Warning, "Вы не можете сделать и вдоха!");
        return;
    }
    let Some(hookah) = world.get::<Mouthpiece>(mouthpiece).map(|m| m.hookah) else {
        return;
    };
    let Some(food) = world.get::<Hookah>(hookah).map(|h| h.food.clone()) else {
        return;
    };
    let mut safe = true;
    let share = amount / food.len().max(1) as f32;
    for item in food {
        let has_reagents = world.get::<Reagents>(item).is_some();
        if has_reagents {
            if !path_of(world, item).is_some_and(|p| SAFE_FOODS.iter().any(|r| is_under(p, r))) {
                safe = false;
            }
            crate::chem::transfer(world, item, hookah, share);
        }
        let empty = world
            .get::<Reagents>(item)
            .is_none_or(|r| r.0.total_volume <= 0.0);
        if empty {
            if let Some(mut h) = world.get_mut::<Hookah>(hookah) {
                h.food.retain(|&f| f != item);
            }
            world.despawn(item);
        }
    }
    if !safe {
        make_black_smoke(world, mob, mouthpiece, hookah);
        return;
    }
    if world
        .get::<Reagents>(hookah)
        .is_none_or(|r| r.0.total_volume <= 0.0)
    {
        let text = format!("В {} нет жидкости!", name_of(world, mouthpiece));
        tell_mob(world, mob, Span::Warning, &text);
        return;
    }
    let smoke = world.get::<Hookah>(hookah).map_or(0.0, |h| h.smoke);
    let efficiency = smoke.min(100.0) / 100.0;
    let to_transfer = if full { amount } else { amount * efficiency };
    let to_waste = amount - to_transfer;
    let transferred =
        crate::chem::transfer_to_mob_by(world, hookah, mob, to_transfer, crate::janitor::INHALE);
    if let Some(pos) = crate::explosions::turf_of(world, mouthpiece) {
        playsound(world, PlaySound::new(pos, BUBBLES, 20.0, false));
    }
    if transferred <= 0.0 {
        return;
    }
    let t = now(world);
    let what = name_of(world, mouthpiece);
    tell_mob(
        world,
        mob,
        Span::Notice,
        &format!("Вы вдыхаете дым из {what}."),
    );
    crate::mood::add_event(world, mob, "smoked", &crate::drugs::SMOKED);
    let too_soon = world
        .get::<Mouthpiece>(mouthpiece)
        .is_some_and(|m| t < m.inhale_ready);
    if too_soon || transferred > BASE_INHALE_LIMIT {
        let user = name_of(world, mob);
        let others = [
            format!("{user} закашливается!"),
            format!("{user} морщится, откашливаясь."),
            format!("{user} задыхается!"),
        ];
        let own = [
            "Голова кружится...",
            "Вы закашливаетесь, морщась от острого покалывания в горле.",
            "Вы задыхаетесь!",
        ];
        let (a, b) = {
            let mut rng = world.resource_mut::<CombatRng>();
            (rng.range(0, 2) as usize, rng.range(0, 2) as usize)
        };
        visible(world, mob, own[b], &others[a], Span::Warning);
        cough(world, mob);
        crate::status_effects::adjust_stamina(
            world,
            mob,
            BASE_COUGH_STAMINA_LOSS * (transferred / BASE_INHALE_LIMIT),
        );
    }
    let percent = efficiency * 100.0;
    if percent <= 20.0 {
        tell_mob(world, mob, Span::Warning, "Ваше горло словно обжигает...");
        cough(world, mob);
    } else if percent <= 40.0 {
        tell_mob(world, mob, Span::Notice, "Слегка горчит.");
    } else if percent <= 80.0 {
        tell_mob(world, mob, Span::Notice, "Довольно приятный вкус...");
    } else {
        tell_mob(world, mob, Span::Notice, "Неплохой дымок.");
    }
    if let Some(mut m) = world.get_mut::<Mouthpiece>(mouthpiece) {
        m.inhale_ready = t + INHALE_COOLDOWN;
    }
    // `rand(amount * 2, amount)`: DM swaps reversed bounds.
    let gain = world
        .resource_mut::<CombatRng>()
        .range(amount as u32, (amount * 2.0) as u32) as f32;
    if let Some(mut h) = world.get_mut::<Hookah>(hookah) {
        h.smoke = (h.smoke + gain).min(100.0);
        h.puffs.push((t + 1.0, mob, to_waste));
    }
}

/// `emote("cough")`.
fn cough(world: &mut World, mob: Entity) {
    crate::speech::run_emote(world, mob, "cough");
}

/// `make_black_smoke`: what is not a plant or cheese burns foul.
fn make_black_smoke(world: &mut World, mob: Entity, mouthpiece: Entity, hookah: Entity) {
    if let (Some(pos), Some(carry)) = (
        crate::explosions::turf_of(world, hookah),
        world.get::<Reagents>(hookah).map(|r| r.0.clone()),
    ) {
        crate::effects::smoke::do_chem_smoke(
            world,
            2,
            pos,
            None,
            &carry,
            crate::effects::smoke::SmokeKind::Chem,
        );
    }
    let food = world
        .get_mut::<Hookah>(hookah)
        .map(|mut h| std::mem::take(&mut h.food))
        .unwrap_or_default();
    for item in food {
        if world.get_entity(item).is_ok() {
            world.despawn(item);
        }
    }
    if let Some(mut r) = world.get_mut::<Reagents>(hookah) {
        r.0.clear();
    }
    tell_mob(
        world,
        mob,
        Span::Warning,
        "Вы чувствуете резкий неприятный запах!",
    );
    return_mouthpiece(world, mouthpiece);
    cough(world, mob);
    if is_human(world, mob) {
        crate::status_effects::adjust_stamina(world, mob, BASE_COUGH_STAMINA_LOSS * 4.0);
    }
}

/// `click_alt_secondary`: the radial of what can be done now. True when it was a hookah.
pub fn click_alt_secondary(world: &mut World, client: Entity, mob: Entity, hookah: Entity) -> bool {
    let Some(state) = world.get::<Hookah>(hookah) else {
        return false;
    };
    if !is_human(world, mob) {
        return true;
    }
    let filled = !state.food.is_empty()
        || world
            .get::<Reagents>(hookah)
            .is_some_and(|r| r.0.total_volume > 0.0);
    let mut options = Vec::new();
    if state.lit {
        options.push(OPTION_EXTINGUISH);
    }
    if filled && state.lit {
        options.push(OPTION_BLOW);
    }
    if filled {
        options.push(OPTION_CLEAR);
    }
    let choices = options
        .iter()
        .map(|&o| RadialChoice {
            name: o.to_owned(),
            icon: crate::radial::icon_of(world, RADIAL_DMI, radial_state(o)),
        })
        .collect();
    let title = name_of(world, hookah);
    if let Some(prompt) = crate::radial::show(
        world,
        crate::radial::RadialMenu {
            client,
            actor: mob,
            anchor: hookah,
            title,
            choices,
            require_near: true,
        },
    ) {
        world
            .entity_mut(prompt)
            .insert(HookahAsk { hookah, options });
    }
    true
}

fn radial_state(option: &str) -> &'static str {
    match option {
        OPTION_EXTINGUISH => "extinguish",
        OPTION_BLOW => "blow",
        _ => "eject",
    }
}

/// The radial's choice, checked again now.
fn radial_answers(
    world: &mut World,
    mut answers: Local<bevy::ecs::message::MessageCursor<PromptAnswered>>,
) {
    let pending: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = answers
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in pending {
        let Some(ask) = world.get::<HookahAsk>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let Some(&option) = ask.options.get(i) else {
            continue;
        };
        let hookah = ask.hookah;
        if world.get::<Hookah>(hookah).is_none()
            || !crate::radial::can_still_use(world, mob, hookah)
        {
            continue;
        }
        let actor = Actor {
            client,
            mob,
            item: None,
        };
        let target = anchor(world, mob, hookah);
        let lit = world.get::<Hookah>(hookah).is_some_and(|h| h.lit);
        match option {
            OPTION_EXTINGUISH if lit => {
                let text = format!("Вы начинаете тушить {}...", name_of(world, hookah));
                tell_mob(world, mob, Span::Notice, &text);
                do_after(
                    world,
                    actor,
                    target,
                    20.0,
                    0,
                    Job::Hookah(HookahJob::Extinguish(hookah)),
                );
            }
            OPTION_BLOW if lit => {
                if !blow_ready(world, mob, hookah) {
                    continue;
                }
                let user = name_of(world, mob);
                visible(
                    world,
                    mob,
                    "Вы делаете глубокую затяжку...",
                    &format!("{user} глубоко затягивается..."),
                    Span::Notice,
                );
                do_after(
                    world,
                    actor,
                    target,
                    50.0,
                    0,
                    Job::Hookah(HookahJob::Blow(hookah)),
                );
            }
            OPTION_CLEAR => {
                do_after(
                    world,
                    actor,
                    target,
                    20.0,
                    0,
                    Job::Hookah(HookahJob::Clear(hookah)),
                );
            }
            _ => {}
        }
    }
}

/// The deep draw's checks: something in the bowl, the mouthpiece in hand.
fn blow_ready(world: &mut World, mob: Entity, hookah: Entity) -> bool {
    let Some(state) = world.get::<Hookah>(hookah) else {
        return false;
    };
    let mouthpiece = state.mouthpiece;
    let filled = !state.food.is_empty()
        || world
            .get::<Reagents>(hookah)
            .is_some_and(|r| r.0.total_volume > 0.0);
    if !filled {
        let text = format!("В {} нет ингридиентов!", name_of(world, hookah));
        tell_mob(world, mob, Span::Warning, &text);
        return false;
    }
    mouthpiece.is_some_and(|m| {
        world
            .get::<Inventory>(mob)
            .is_some_and(|i| i.hands.contains(&Some(m)))
    })
}

/// The rest of a [`HookahJob`] after its `do_after`, with its checks again.
pub fn complete(world: &mut World, mob: Entity, _action: &DoAfter, job: HookahJob) {
    match job {
        HookahJob::Inhale(mouthpiece) => {
            let held = world
                .get::<Inventory>(mob)
                .is_some_and(|i| i.hands.contains(&Some(mouthpiece)));
            let lit = world
                .get::<Mouthpiece>(mouthpiece)
                .and_then(|m| world.get::<Hookah>(m.hookah))
                .is_some_and(|h| h.lit);
            if held && lit {
                inhale_smoke(world, mob, mouthpiece, BASE_INHALE_VOLUME, false);
            }
        }
        HookahJob::Extinguish(hookah) => {
            if world.get::<Hookah>(hookah).is_some_and(|h| h.lit) {
                put_out(world, hookah);
            }
        }
        HookahJob::Blow(hookah) => {
            if !world.get::<Hookah>(hookah).is_some_and(|h| h.lit)
                || !blow_ready(world, mob, hookah)
            {
                return;
            }
            if let Some(mouthpiece) = world.get::<Hookah>(hookah).and_then(|h| h.mouthpiece) {
                inhale_smoke(world, mob, mouthpiece, BASE_INHALE_VOLUME * 2.0, true);
            }
        }
        HookahJob::Clear(hookah) => {
            if let Some(mut r) = world.get_mut::<Reagents>(hookah) {
                r.0.clear();
            } else {
                return;
            }
            let text = format!("Вы очищаете чашу {}.", name_of(world, hookah));
            tell_mob(world, mob, Span::Notice, &text);
        }
    }
}

/// `examine`: the bowl's food and whether it is lit.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    let Some(state) = world.get::<Hookah>(target) else {
        return;
    };
    let names: Vec<String> = state.food.iter().map(|&f| name_of(world, f)).collect();
    let listed = if names.is_empty() {
        "пусто".to_owned()
    } else {
        crate::clothing::accessories::english_list(&names)
    };
    lines.push((Span::Notice, format!("В чаше {listed}.")));
    if state.lit {
        let text = format!(
            "{} зажжён.",
            ss13_shared::capitalize(&name_of(world, target))
        );
        lines.push((Span::Notice, text));
    }
}

/// `throw_impact` -> `atom_destruction`: a landed hookah shatters. True when it was one.
pub fn thrown(world: &mut World, hookah: Entity) -> bool {
    if world.get::<Hookah>(hookah).is_none() {
        return false;
    }
    let Some(&pos) = world.get::<TilePos>(hookah) else {
        return true;
    };
    crate::construction::spawn_obj(world, SHARD, pos);
    let turf = crate::construction::turf_at(world, pos);
    let state = world.get::<Hookah>(hookah).cloned().expect("checked");
    let mut spilled: Vec<Entity> = vec![hookah];
    spilled.extend(state.food.iter().copied());
    for e in spilled {
        let holder = world
            .get::<Reagents>(e)
            .map(|r| r.0.clone())
            .filter(|h| h.total_volume > 0.0);
        if let (Some(turf), Some(holder)) = (turf, holder) {
            crate::janitor::expose(world, turf, &holder, 1.0);
        }
    }
    for e in state.food.into_iter().chain(state.mouthpiece) {
        if world.get_entity(e).is_ok() {
            world.despawn(e);
        }
    }
    let text = format!(
        "{} с треском разлетается на осколки!",
        ss13_shared::capitalize(&name_of(world, hookah))
    );
    visible(world, hookah, &text, &text, Span::Warning);
    playsound(world, PlaySound::new(pos, SHATTER, 50.0, true));
    world.despawn(hookah);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_fuel_burns_out_and_the_smoke_settles() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let hookah = world
            .spawn(Hookah {
                fuel: 2,
                lit: true,
                smoke: 30.0,
                ..default()
            })
            .id();
        process(&mut world);
        let state = world.get::<Hookah>(hookah).unwrap();
        assert_eq!(state.fuel, 1);
        assert!(state.lit);
        assert!((state.smoke - 29.9).abs() < 1e-4);
        // Nothing more within the cooldowns.
        process(&mut world);
        assert_eq!(world.get::<Hookah>(hookah).unwrap().fuel, 1);
        world.get_mut::<Hookah>(hookah).unwrap().next_fuel = 0.0;
        process(&mut world);
        let state = world.get::<Hookah>(hookah).unwrap();
        assert_eq!(state.fuel, 0);
        assert!(!state.lit, "out of fuel it goes out");
        assert_eq!(state.smoke, 0.0);
    }

    #[test]
    fn radial_states_follow_the_options() {
        assert_eq!(radial_state(OPTION_EXTINGUISH), "extinguish");
        assert_eq!(radial_state(OPTION_BLOW), "blow");
        assert_eq!(radial_state(OPTION_CLEAR), "eject");
    }
}
