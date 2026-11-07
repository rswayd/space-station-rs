//! Mineral and other sheets' own behaviour (server), after tg `stacks/sheets/mineral.dm`,
//! `hot_ice.dm` and the materials they are made of (`datums/materials/basemats.dm`,
//! `datums/components/combustible_flooder.dm`). Their crafting menus: [`crate::stack_recipes`].
//!
//! - Plasma sheets and hot ice are `combustible_flooder`s (`/datum/material/plasma`,
//!   `/hot_ice` `on_applied` with `mats_per_unit` = one sheet): a fire hotter than
//!   `FIRE_MINIMUM_TEMPERATURE_TO_EXIST` on their tile, or a hot tool used on them, floods the tile
//!   with plasma — 5 moles a sheet at the trigger's temperature for plasma, 150 moles a sheet at
//!   `20 × amount + 300` K for hot ice — and the stack is gone.
//! - Coal (`/obj/item/stack/sheet/mineral/coal`): any `fire_act` or a hot tool releases 10 moles of
//!   CO2 a lump at that temperature, and the coal is gone.
//! - Five plain iron rods used on a titanium sheet make five shuttle frame rods on the user's
//!   tile (one sheet used); a titanium sheet used up in the other hand leaves them in the hands.
//! - Tanning (`leather.dm`): a sharp item on an animal hide (5 s) cuts the hair off, the whole
//!   stack becomes hairless hide; water on it (`/datum/reagent/water/expose_obj`) or a washing
//!   machine cycle (`machine_wash`) makes it wet hide; wet hide on a tile hotter than 500 K
//!   loses a point of its 30 `wetness` every air tick (0.5 s, `atmos_expose`) and at 0 one
//!   piece becomes leather.
//! - Cardboard stamped with a clown stamp folds into a clown box, with a chameleon stamp into a
//!   syndicate kit box (one sheet each, not inside a storage).
// shortcut: the only hot tool is a lit welder (no lighters or matches yet); sparks and burn
// projectiles do not ignite sheets; bananium sheets do not squeak or slip; wet hide is not dried
// by drying racks, microwaves, grills or ovens, and a wooden peg leg is not repaired with wood.

use bevy::prelude::*;
use ss13_mapload::{DmItem, DmStack};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;

use crate::construction::Job;
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, Stack, Welder, do_after, stack_amount, tell, use_stack};

const PLASMA_SHEET: &str = "/obj/item/stack/sheet/mineral/plasma";
const HOT_ICE: &str = "/obj/item/stack/sheet/hot_ice";
const COAL: &str = "/obj/item/stack/sheet/mineral/coal";
const TITANIUM: &str = "/obj/item/stack/sheet/mineral/titanium";
const RODS: &str = "/obj/item/stack/rods";
pub const SHUTTLE_RODS: &str = "/obj/item/stack/rods/shuttle/five";
const ANIMAL_HIDE: &str = "/obj/item/stack/sheet/animalhide";
pub const HAIRLESS_HIDE: &str = "/obj/item/stack/sheet/hairlesshide";
pub const WET_HIDE: &str = "/obj/item/stack/sheet/wethide";
pub const LEATHER: &str = "/obj/item/stack/sheet/leather";
const CARDBOARD: &str = "/obj/item/stack/sheet/cardboard";
const CLOWN_STAMP: &str = "/obj/item/stamp/clown";
const CHAMELEON_STAMP: &str = "/obj/item/stamp/chameleon";
pub const CLOWN_BOX: &str = "/obj/item/storage/box/clown";
pub const SYNDIE_BOX: &str = "/obj/item/storage/box/syndie_kit";
const STORAGE: &str = "/obj/item/storage";
const SLICE_SOUND: &str = "sound/items/weapons/slice.ogg";
const BIKEHORN_SOUND: &str = "sound/items/bikehorn.ogg";
/// Dehairing a hide, deciseconds.
const DEHAIR_DS: f32 = 50.0;
/// `wetness`, `drying_threshold_temperature`.
const WETNESS: u32 = 30;
const DRYING_TEMP: f64 = 500.0;
/// SSair's wait: how often wet hide is exposed.
const AIR_TICK_SECS: f64 = 0.5;

/// `/obj/item/stack/sheet/wethide` `wetness`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wetness(pub u32);

/// The timed actions of this module ([`crate::construction::Job::Sheet`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SheetJob {
    /// Cutting the hair off this hide.
    Dehair(Entity),
}
const FIRE_MIN_TEMP: f64 = ss13_atmos::reactions::FIRE_MINIMUM_TEMPERATURE_TO_EXIST;

/// What a burning sheet releases.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flood {
    pub gas: &'static str,
    /// Moles per unit of the stack.
    pub moles_per_unit: f64,
    /// Its own temperature `(base, per unit)`; `None`: the trigger's.
    pub temperature: Option<(f64, f64)>,
    /// `combustible_flooder/flame_react` needs a fire hotter than
    /// `FIRE_MINIMUM_TEMPERATURE_TO_EXIST`; coal's `fire_act` takes any.
    pub hot_fire_only: bool,
}

/// The burning sheets, most specific first.
pub const FLOODS: [(&str, Flood); 3] = [
    (
        PLASMA_SHEET,
        // `mat_amount * 0.05`: one sheet is `SHEET_MATERIAL_AMOUNT` (100).
        Flood {
            gas: "plasma",
            moles_per_unit: 5.0,
            temperature: None,
            hot_fire_only: true,
        },
    ),
    (
        HOT_ICE,
        // `mat_amount * 1.5` moles at `mat_amount * 0.2 + 300`.
        Flood {
            gas: "plasma",
            moles_per_unit: 150.0,
            temperature: Some((300.0, 20.0)),
            hot_fire_only: true,
        },
    ),
    (
        COAL,
        Flood {
            gas: "co2",
            moles_per_unit: 10.0,
            temperature: None,
            hot_fire_only: false,
        },
    ),
];

pub struct SheetsPlugin;

impl Plugin for SheetsPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::HeldOnTarget, 3449, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
        app.add_click_handler(ClickPhase::OtherHand, 1369, |w, c| {
            c.held
                .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
        });
        app.add_observer(wet_hide_state)
            .add_systems(Update, dry_hides.run_if(any_with_component::<Wetness>));
    }
}

fn stack_path(world: &World, e: Entity) -> Option<String> {
    world.get::<Stack>(e)?;
    world.get::<DmItem>(e).map(|i| i.path.clone())
}

/// The flood of a sheet stack, if it burns.
pub fn flood_of(world: &World, e: Entity) -> Option<Flood> {
    let path = stack_path(world, e)?;
    FLOODS
        .iter()
        .find(|(root, _)| is_under(&path, root))
        .map(|&(_, f)| f)
}

/// `get_temperature` of a held item: a lit welder's heat.
fn item_temperature(world: &World, item: Entity) -> f64 {
    if !world.get::<Welder>(item).is_some_and(|w| w.on) {
        return 0.0;
    }
    world
        .get::<ss13_mapload::DmTool>(item)
        .map_or(0.0, |t| f64::from(t.heat))
}

/// `combustible_flooder/flood`, coal's `fire_act`: the gas on the stack's turf, the stack gone.
fn flood(world: &mut World, stack: Entity, flood: Flood, trigger: f64) {
    let amount = stack_amount(world, stack);
    let units = f64::from(amount);
    let temperature = flood
        .temperature
        .map_or(trigger, |(base, per)| base + per * units);
    if let Some(pos) = crate::explosions::turf_of(world, stack)
        && let Some(mut atmos) = world.get_resource_mut::<ss13_atmos::Atmos>()
    {
        let gas = format!(
            "{}={};TEMP={temperature}",
            flood.gas,
            flood.moles_per_unit * units
        );
        if atmos.spawn_air(pos, &gas).is_err() {
            warn!("sheet flood gas {gas:?} does not parse");
        }
    }
    use_stack(world, stack, amount);
}

/// `fire_act` of a burning sheet. True when `thing` burned.
pub fn fire_act(world: &mut World, thing: Entity, temperature: f64) -> bool {
    let Some(f) = flood_of(world, thing) else {
        return false;
    };
    if f.hot_fire_only && temperature <= FIRE_MIN_TEMP {
        return false;
    }
    flood(world, thing, f, temperature);
    true
}

/// `attackby_react`/coal's `item_interaction` (a hot tool), titanium's rods. True when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if let Some(f) = flood_of(world, target) {
        let temperature = item_temperature(world, held);
        if temperature >= FIRE_MIN_TEMP {
            let name = crate::examine::name_of(world, target);
            tell(world, client, Span::Danger, format!("{name} вспыхивает!"));
            flood(world, target, f, temperature);
            return true;
        }
    }
    let target_path = stack_path(world, target).unwrap_or_default();
    if is_under(&target_path, ANIMAL_HIDE)
        && world
            .get::<ss13_mapload::DmWounding>(held)
            .is_some_and(|w| w.sharpness != 0)
    {
        if let Some(pos) = crate::explosions::turf_of(world, target) {
            playsound(world, PlaySound::new(pos, SLICE_SOUND, 50.0, true));
        }
        tell(
            world,
            client,
            Span::Notice,
            "Вы начинаете срезать шерсть со шкуры...",
        );
        // In hand the hide moves with its holder: the timer follows the holder instead.
        let on = if world.get::<TilePos>(target).is_some() {
            target
        } else {
            mob
        };
        let actor = Actor {
            client,
            mob,
            item: Some(held),
        };
        do_after(
            world,
            actor,
            on,
            DEHAIR_DS,
            0,
            Job::Sheet(SheetJob::Dehair(target)),
        );
        return true;
    }
    if is_under(&target_path, CARDBOARD) && stamp_cardboard(world, client, held, target) {
        return true;
    }
    if is_under(&target_path, TITANIUM)
        && stack_path(world, held).is_some_and(|p| is_under(&p, RODS))
    {
        shuttle_rods(world, client, mob, held, target);
        return true;
    }
    false
}

/// `/obj/item/stack/sheet/mineral/titanium/item_interaction` with rods.
fn shuttle_rods(world: &mut World, client: Entity, mob: Entity, rods: Entity, titanium: Entity) {
    // `merge_type != /obj/item/stack/rods`: lava and shuttle rods do not work.
    let plain = world.get::<DmStack>(rods).is_some_and(|s| {
        s.merge_type == RODS
            || (s.merge_type.is_empty()
                && world.get::<DmItem>(rods).is_some_and(|i| i.path == RODS))
    });
    if !plain {
        tell(
            world,
            client,
            Span::Warning,
            "Из такого вида стержней каркасные стержни шаттла не сделать!",
        );
        return;
    }
    // tg tests `rods < 5 && sheets < 1`, letting either shortfall through; both are needed.
    if stack_amount(world, rods) < 5 || stack_amount(world, titanium) < 1 {
        tell(
            world,
            client,
            Span::Warning,
            "Нужно пять стержней и один лист титана, чтобы сделать каркасные стержни шаттла!",
        );
        return;
    }
    let Some(pos) = world.get::<TilePos>(mob).copied() else {
        return;
    };
    let replace = world
        .get::<Inventory>(mob)
        .is_some_and(|inv| inv.hands[1 - inv.active] == Some(titanium));
    let Some(new_rods) = crate::construction::spawn_stack(world, SHUTTLE_RODS, pos, 5) else {
        return;
    };
    use_stack(world, rods, 5);
    use_stack(world, titanium, 1);
    if replace && world.get_entity(titanium).is_err() && world.get_entity(new_rods).is_ok() {
        crate::assemblies::server::to_hands(world, mob, new_rods);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flood_table_is_tg_numbers() {
        let (_, hot_ice) = FLOODS.iter().find(|(p, _)| *p == HOT_ICE).unwrap();
        // 3 sheets: 450 moles at 360 K.
        assert_eq!(hot_ice.moles_per_unit * 3.0, 450.0);
        let (base, per) = hot_ice.temperature.unwrap();
        assert_eq!(base + per * 3.0, 360.0);
        let (_, coal) = FLOODS.iter().find(|(p, _)| *p == COAL).unwrap();
        assert!(!coal.hot_fire_only);
    }
}

/// [`Job::Sheet`] done.
pub fn complete(world: &mut World, client: Entity, mob: Entity, job: SheetJob) {
    match job {
        SheetJob::Dehair(hide) => {
            if stack_path(world, hide).is_none_or(|p| !is_under(&p, ANIMAL_HIDE)) {
                return;
            }
            let Some(pos) = world.get::<TilePos>(mob).copied() else {
                return;
            };
            let amount = stack_amount(world, hide);
            tell(world, client, Span::Notice, "Вы срезаете шерсть со шкуры.");
            crate::construction::spawn_stack(world, HAIRLESS_HIDE, pos, amount);
            use_stack(world, hide, amount);
        }
    }
}

/// `/obj/item/stack/sheet/cardboard/item_interaction` with a clown or chameleon stamp.
fn stamp_cardboard(world: &mut World, client: Entity, stamp: Entity, cardboard: Entity) -> bool {
    let stamp_path = world
        .get::<DmItem>(stamp)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    let (box_path, text) = if is_under(&stamp_path, CLOWN_STAMP) {
        (CLOWN_BOX, "Вы штампуете картон! Это коробка клоуна! Хонк!")
    } else if is_under(&stamp_path, CHAMELEON_STAMP) {
        (SYNDIE_BOX, "Вы зловеще штампуете картон.")
    } else {
        return false;
    };
    let in_storage = world
        .get::<crate::storage::StoredIn>(cardboard)
        .and_then(|s| crate::construction::path_of(world, s.0))
        .is_some_and(|p| is_under(&p, STORAGE));
    if in_storage {
        return false;
    }
    let Some(pos) = crate::explosions::turf_of(world, cardboard) else {
        return true;
    };
    if !use_stack(world, cardboard, 1) {
        return true;
    }
    if box_path == CLOWN_BOX {
        playsound(world, PlaySound::new(pos, BIKEHORN_SOUND, 50.0, true));
    }
    tell(world, client, Span::Notice, text);
    crate::construction::spawn_obj(world, box_path, pos);
    true
}

fn wet_hide_state(add: On<Add<DmStack>>, items: Query<&DmItem>, mut commands: Commands) {
    if items
        .get(add.entity)
        .is_ok_and(|i| is_under(&i.path, WET_HIDE))
    {
        commands.entity(add.entity).insert_if_new(Wetness(WETNESS));
    }
}

/// Hairless hide into wet hide where it is (water's `expose_obj`, a washer's `machine_wash` on
/// the washer's tile). True when `item` was hairless hide.
pub fn soak(world: &mut World, item: Entity) -> bool {
    if stack_path(world, item).is_none_or(|p| !is_under(&p, HAIRLESS_HIDE)) {
        return false;
    }
    let Some(pos) = crate::explosions::turf_of(world, item) else {
        return false;
    };
    let amount = stack_amount(world, item);
    if world.get::<crate::storage::StoredIn>(item).is_some() {
        crate::storage::take_out(world, item, pos);
    }
    crate::construction::spawn_stack(world, WET_HIDE, pos, amount);
    use_stack(world, item, amount);
    true
}

/// `wethide/atmos_expose` every air tick on a tile hotter than `drying_threshold_temperature`.
fn dry_hides(world: &mut World, mut next: Local<f64>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if now < *next {
        return;
    }
    *next = now + AIR_TICK_SECS;
    let mut q = world.query::<(Entity, &TilePos, &Wetness)>();
    let hides: Vec<(Entity, TilePos, u32)> =
        q.iter(world).map(|(e, &pos, w)| (e, pos, w.0)).collect();
    let Some(atmos) = world.get_resource::<ss13_atmos::Atmos>() else {
        return;
    };
    let hot: Vec<(Entity, TilePos, u32)> = hides
        .into_iter()
        .filter(|&(_, pos, _)| atmos.air(pos).is_some_and(|a| a.temperature > DRYING_TEMP))
        .collect();
    for (hide, pos, wetness) in hot {
        if wetness > 1 {
            world.entity_mut(hide).insert(Wetness(wetness - 1));
            continue;
        }
        world.entity_mut(hide).insert(Wetness(WETNESS));
        crate::construction::spawn_stack(world, LEATHER, pos, 1);
        use_stack(world, hide, 1);
    }
}
