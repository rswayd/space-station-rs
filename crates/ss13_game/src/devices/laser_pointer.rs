//! Laser pointers (server), after tg `code/game/objects/items/devices/laserpointer.dm`.
//!
//! - A click with the pointer, near or far (`interact_with_atom` / `ranged_interact_with_atom`
//!   -> `laser_act`): needs a diode, the target within `max_range` (7; the infinite pointer has
//!   none) and the user in sight of it (through walls only with a bluespace crystal lens and
//!   X-ray or thermal sight), and the battery out of its full-recharge lock. Aimed at a living
//!   carbon's eyes it may flash them (`effectchance` 30 × the diode's rating, a random intensity
//!   0-2, an eyepatch halves the odds); a cyborg's sensors overload with the same odds
//!   (`flash_act(affect_silicon)`); a camera's lens takes a heavy EMP.
//! - Each shine uses 1 energy (2 with the lens) of 10; at 0 the pointer locks until fully
//!   recharged; each second it regains 1 with `10 + 10 × rating` % (`SPT_PROB`).
//! - A screwdriver takes the diode out, a micro-laser goes in; a bluespace crystal (one of the
//!   stack) fits as a lens in front of a tier 3+ diode, wirecutters (or a hemostat) take it out.
// shortcut: no laser dot overlay on the target, no cats chasing the light (no
// `TRAIT_CATLIKE_INSTINCT` mobs), no X-ray/thermal sight check (the lens never shines through
// walls: "can't quite make out your target"), no `ISADVANCEDTOOLUSER`/`TRAIT_CHUNKYFINGERS`
// checks (only mobs with hands hold it), no cyborg temporary blindness (no silicon blindness;
// the flash overlay only). Forcing a crystal and a low-tier diode together is refused instead
// of the 2 s struggle that teleports the user or the part away.
// Cost: a pass over the recharging pointers per tick; a shine is a line-of-sight walk.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::DmItem;
use ss13_mapload::fidelity::is_under;
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;

use crate::click::RangedClick;
use crate::combat::CombatRng;
use crate::health::{Stat, ZoneSelected};
use crate::inventory::Inventory;
use crate::medical::name_of;
use crate::sound::{PlaySound, playsound};
use crate::tools::{ToolKind, tell, tool_of};
use crate::zone::TargetZone;

const LASER_POINTER: &str = "/obj/item/laser_pointer";
const UPGRADED: &str = "/obj/item/laser_pointer/upgraded";
const INFINITE: &str = "/obj/item/laser_pointer/infinite_range";
const MICRO_LASER: &str = crate::stock_parts::MICRO_LASER;
const BLUESPACE_CRYSTAL: &str = "/obj/item/stack/ore/bluespace_crystal";
const EYEPATCH: &str = "/obj/item/clothing/glasses/eyepatch";
/// The diode of each tier (`rating`).
const DIODES: [&str; 4] = [
    "/obj/item/stock_parts/micro_laser",
    "/obj/item/stock_parts/micro_laser/high",
    "/obj/item/stock_parts/micro_laser/ultra",
    "/obj/item/stock_parts/micro_laser/quadultra",
];
/// `effectchance`, `max_energy`, `max_range`.
const EFFECT_CHANCE: f32 = 30.0;
pub const MAX_ENERGY: i32 = 10;
const MAX_RANGE: u32 = 7;
/// `flash_act`'s default `length = 2.5 SECONDS`.
const FLASH_SECS: f32 = 2.5;
const SCREWDRIVER: &str = "sound/items/tools/screwdriver.ogg";
const SCREWDRIVER2: &str = "sound/items/tools/screwdriver2.ogg";

/// `/obj/item/laser_pointer`: the diode's path and rating, the lens' path, `energy`,
/// `recharge_locked`, `recharging`, `max_range` (`None`: `INFINITY`).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct LaserPointer {
    pub diode: Option<(String, u8)>,
    pub lens: Option<String>,
    pub energy: i32,
    pub recharge_locked: bool,
    pub recharging: bool,
    pub max_range: Option<u32>,
}

impl LaserPointer {
    fn for_path(path: &str) -> Self {
        let tier = if is_under(path, INFINITE) {
            4
        } else if is_under(path, UPGRADED) {
            3
        } else {
            1
        };
        Self {
            diode: Some((DIODES[tier - 1].to_owned(), tier as u8)),
            lens: None,
            energy: MAX_ENERGY,
            recharge_locked: false,
            recharging: false,
            max_range: (!is_under(path, INFINITE)).then_some(MAX_RANGE),
        }
    }

    fn rating(&self) -> u8 {
        self.diode.as_ref().map_or(0, |d| d.1)
    }

    /// `energy -= crystal_lens ? 2 : 1` and the recharge state after it; true when the battery
    /// just ran dry.
    pub fn drain(&mut self) -> bool {
        self.energy -= if self.lens.is_some() { 2 } else { 1 };
        self.recharging = true;
        if self.energy <= 0 {
            self.recharge_locked = true;
            return true;
        }
        false
    }

    /// One recharged use (`energy += 1`, capped; full ends both states).
    pub fn recharge_one(&mut self) {
        self.energy = (self.energy + 1).min(MAX_ENERGY);
        if self.energy >= MAX_ENERGY {
            self.recharging = false;
            self.recharge_locked = false;
        }
    }
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // Tools and parts onto a pointer held in the other hand.
    app.add_click_handler(ClickPhase::OtherHand, 467, |w, c| {
        c.held
            .is_some_and(|held| item_interaction(w, c.client, c.mob, held, c.target))
    });
    // Onto a pointer on the floor or a table, then the pointer shone at whatever is clicked
    // (after storages take it, as tg's storage `item_interaction` comes first).
    app.add_click_handler(ClickPhase::HeldOnTarget, 1567, |w, c| {
        c.held
            .is_some_and(|held| item_interaction(w, c.client, c.mob, held, c.target))
    });
    app.add_click_handler(ClickPhase::HeldOnTarget, 4753, |w, c| {
        let Some(held) = c.held.filter(|&h| w.get::<LaserPointer>(h).is_some()) else {
            return false;
        };
        laser_act(w, c.client, c.mob, held, c.target);
        true
    });
    app.add_observer(add_pointer)
        .add_systems(Update, (ranged_shines, recharge));
}

fn add_pointer(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    if let Ok(item) = items.get(add.entity)
        && is_under(&item.path, LASER_POINTER)
    {
        commands
            .entity(add.entity)
            .try_insert_if_new(LaserPointer::for_path(&item.path));
    }
}

/// `ranged_interact_with_atom`: a click past reach with a pointer in the active hand.
fn ranged_shines(world: &mut World, mut cursor: Local<MessageCursor<RangedClick>>) {
    let clicks: Vec<RangedClick> = cursor
        .read(world.resource::<Messages<RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(held) = world
            .get::<Inventory>(click.mob)
            .and_then(Inventory::active_item)
            .filter(|&h| world.get::<LaserPointer>(h).is_some())
        else {
            continue;
        };
        if world.get_entity(click.target).is_err() {
            continue;
        }
        laser_act(world, click.client, click.mob, held, click.target);
    }
}

/// `laser_act(target, user)`.
pub fn laser_act(world: &mut World, client: Entity, mob: Entity, pointer: Entity, target: Entity) {
    let Some(state) = world.get::<LaserPointer>(pointer).cloned() else {
        return;
    };
    let item_name = name_of(world, pointer);
    let target_name = name_of(world, target);
    if state.diode.is_none() {
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы направляете {item_name} на {target_name}, но ничего не происходит!"),
        );
        return;
    }
    let (Some(from), Some(to)) = (
        crate::modpc::pda::turf_of(world, mob),
        crate::modpc::pda::turf_of(world, target),
    ) else {
        return;
    };
    if let Some(range) = state.max_range {
        if from.chebyshev(to).is_none_or(|d| d > range) {
            tell(
                world,
                client,
                Span::Warning,
                format!("{target_name} слишком далеко!"),
            );
            return;
        }
        // `user in view(max_range, target)`.
        if !crate::mobs::ai::behaviors::can_see(world, to, from, range) {
            let text = if state.lens.is_none() {
                format!("Вы не можете светить {item_name} сквозь стены!")
            } else {
                "Вы не можете толком разглядеть цель и промахиваетесь.".to_owned()
            };
            tell(world, client, Span::Warning, text);
            return;
        }
    }
    crate::forensics::server::add_fingerprint(world, pointer, mob, false);
    if state.recharge_locked {
        tell(
            world,
            client,
            Span::Notice,
            format!("Вы направляете {item_name} на {target_name}, но он ещё заряжается."),
        );
        return;
    }
    let chance = EFFECT_CHANCE * f32::from(state.rating());
    let outmsg = shine_effect(world, mob, target, chance, &item_name, &target_name);
    let outmsg = outmsg.unwrap_or_else(|| {
        (
            Span::Info,
            format!("Вы направляете {item_name} на {target_name}."),
        )
    });
    tell(world, client, outmsg.0, outmsg.1);
    let dry = world
        .get_mut::<LaserPointer>(pointer)
        .is_some_and(|mut p| p.drain());
    if dry {
        tell(
            world,
            client,
            Span::Warning,
            format!("Батарея {item_name} перегружена, ей нужно время на подзарядку!"),
        );
    }
}

/// The target's reaction (`iscarbon`, `iscyborg`, camera branches) and the user's message.
fn shine_effect(
    world: &mut World,
    mob: Entity,
    target: Entity,
    chance: f32,
    item_name: &str,
    target_name: &str,
) -> Option<(Span, String)> {
    let dead = world.get::<Stat>(target) == Some(&Stat::Dead);
    if world
        .get::<crate::medical::organs::OrganHost>(target)
        .is_some()
    {
        if dead {
            return Some((
                Span::Notice,
                format!("Вы направляете {item_name} на {target_name}, но, похоже, цель мертва!"),
            ));
        }
        let eyes = world
            .get::<ZoneSelected>(mob)
            .is_some_and(|z| z.0 == TargetZone::Eyes);
        if !eyes {
            return None;
        }
        let (severity, eyepatch_fail, hit) = {
            let patched = world
                .get::<Inventory>(target)
                .and_then(|i| i.slot(InventorySlot::Eyes))
                .and_then(|g| world.get::<DmItem>(g))
                .is_some_and(|g| is_under(&g.path, EYEPATCH));
            let mut rng = world.resource_mut::<CombatRng>();
            let severity = rng.range(0, 2) as i32;
            let fail = patched && rng.chance(50.0);
            (severity, fail, rng.chance(chance))
        };
        if hit
            && !eyepatch_fail
            && crate::vision::flash_act(world, target, severity, false, false, FLASH_SECS)
        {
            return Some((
                Span::Notice,
                format!("Вы ослепляете {target_name}, светя {item_name} в глаза."),
            ));
        }
        return Some((
            Span::Warning,
            format!("Вам не удаётся ослепить {target_name}, светя {item_name} в глаза!"),
        ));
    }
    if world
        .get::<crate::silicon::cyborg::Cyborg>(target)
        .is_some()
    {
        if dead {
            return Some((
                Span::Notice,
                format!("Вы направляете {item_name} на {target_name}, но, похоже, он не работает."),
            ));
        }
        let hit = world.resource_mut::<CombatRng>().chance(chance);
        if hit && crate::vision::flash_act(world, target, 1, false, false, FLASH_SECS) {
            if let Some(c) = world
                .get::<crate::server::PlayerMob>(target)
                .map(|p| p.client)
            {
                tell(world, c, Span::Danger, "Ваши сенсоры перегружены лазером!");
            }
            return Some((
                Span::Notice,
                format!("Вы перегружаете {target_name}, светя {item_name} в его сенсоры."),
            ));
        }
        return Some((
            Span::Warning,
            format!("Вам не удаётся перегрузить сенсоры {target_name}, светя {item_name}!"),
        ));
    }
    if let Some(enabled) = world
        .get::<crate::silicon::cameras::Camera>(target)
        .map(|c| c.enabled)
    {
        if !enabled && world.get::<crate::emp::Emped>(target).is_none() {
            return Some((
                Span::Notice,
                format!("Вы направляете {item_name} на {target_name}, но она, похоже, отключена."),
            ));
        }
        if world.resource_mut::<CombatRng>().chance(chance) {
            crate::emp::emp_act(world, target, crate::emp::EMP_HEAVY);
            return Some((
                Span::Notice,
                format!(
                    "Вы попадаете {item_name} в объектив {target_name}, временно отключая камеру!"
                ),
            ));
        }
        return Some((
            Span::Warning,
            format!("Вы промахиваетесь {item_name} мимо объектива {target_name}!"),
        ));
    }
    None
}

/// `process(seconds_per_tick)`: `SPT_PROB(10 + rating * 10)` for one use back.
fn recharge(world: &mut World) {
    let dt = world.resource::<Time>().delta_secs();
    let pointers: Vec<(Entity, u8)> = world
        .query::<(Entity, &LaserPointer)>()
        .iter(world)
        .filter(|(_, p)| p.recharging && p.diode.is_some())
        .map(|(e, p)| (e, p.rating()))
        .collect();
    for (e, rating) in pointers {
        let percent = 10.0 + f32::from(rating) * 10.0;
        let chance = spt_prob(percent, dt);
        if world.resource_mut::<CombatRng>().chance(chance)
            && let Some(mut p) = world.get_mut::<LaserPointer>(e)
        {
            p.recharge_one();
        }
    }
}

/// tg `SPT_PROB`: a per-second `percent` chance over `dt` seconds.
fn spt_prob(percent: f32, dt: f32) -> f32 {
    100.0 * (1.0 - (1.0 - (percent / 100.0).min(1.0)).powf(dt))
}

/// `screwdriver_act`, `tool_act` (wirecutters, hemostat) and `item_interaction` (a diode, a
/// bluespace crystal) with `held` on the pointer `target`. True when handled.
fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(state) = world.get::<LaserPointer>(target).cloned() else {
        return false;
    };
    let Some(drop_at) = crate::modpc::pda::turf_of(world, mob) else {
        return false;
    };
    let held_path = world.get::<DmItem>(held).map(|i| i.path.clone());
    let tool = tool_of(world, held).map(|t| t.0);
    if tool == Some(ToolKind::Screwdriver) {
        let Some((path, _)) = state.diode else {
            return false;
        };
        playsound(world, PlaySound::new(drop_at, SCREWDRIVER, 50.0, true));
        crate::construction::spawn_obj(world, &path, drop_at);
        if let Some(mut p) = world.get_mut::<LaserPointer>(target) {
            p.diode = None;
            p.recharging = false;
        }
        tell(world, client, Span::Notice, "Вы извлекаете диод.");
        return true;
    }
    let hemostat = world
        .get::<ss13_mapload::DmTool>(held)
        .is_some_and(|t| t.behaviour == "hemostat");
    if (tool == Some(ToolKind::Wirecutters) || hemostat)
        && let Some(lens) = state.lens.clone()
    {
        playsound(world, PlaySound::new(drop_at, SCREWDRIVER2, 50.0, true));
        crate::construction::spawn_stack(world, &lens, drop_at, 1);
        if let Some(mut p) = world.get_mut::<LaserPointer>(target) {
            p.lens = None;
        }
        tell(
            world,
            client,
            Span::Notice,
            "Вы извлекаете кристаллическую линзу.",
        );
        return true;
    }
    let Some(held_path) = held_path else {
        return false;
    };
    if is_under(&held_path, MICRO_LASER) {
        if state.diode.is_some() {
            tell(world, client, Span::Warning, "Диод уже установлен!");
            return true;
        }
        let rating = crate::stock_parts::part_rating(world, held).max(1.0) as u8;
        if state.lens.is_some() && rating < 3 {
            // shortcut: refused (see the module note).
            tell(
                world,
                client,
                Span::Warning,
                "Этот диод слишком велик: мешает кристаллическая линза!",
            );
            return true;
        }
        world.despawn(held);
        playsound(world, PlaySound::new(drop_at, SCREWDRIVER, 30.0, true));
        if let Some(mut p) = world.get_mut::<LaserPointer>(target) {
            p.diode = Some((held_path, rating));
            // A pointer charging when its diode came out goes on.
            p.recharging = true;
        }
        tell(world, client, Span::Notice, "Вы устанавливаете диод.");
        return true;
    }
    if is_under(&held_path, BLUESPACE_CRYSTAL) {
        if state.lens.is_some() {
            tell(world, client, Span::Warning, "Линза уже установлена!");
            return true;
        }
        if state.rating() > 0 && state.rating() < 3 {
            tell(
                world,
                client,
                Span::Warning,
                "Кристалл не помещается перед этим диодом!",
            );
            return true;
        }
        // `split_stack(1)`: one crystal of the stack.
        if !crate::tools::use_stack(world, held, 1) {
            return true;
        }
        playsound(world, PlaySound::new(drop_at, SCREWDRIVER2, 30.0, true));
        if let Some(mut p) = world.get_mut::<LaserPointer>(target) {
            p.lens = Some(held_path);
        }
        tell(
            world,
            client,
            Span::Notice,
            "Вы устанавливаете кристаллическую линзу. Теперь указка светит сквозь препятствия \
             ценой двойного расхода энергии.",
        );
        return true;
    }
    false
}

/// `examine` within reach: the diode, the charge, the lens.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let Some(p) = world.get::<LaserPointer>(target) else {
        return;
    };
    match &p.diode {
        None => lines.push((Span::Notice, "Диод отсутствует.".to_owned())),
        Some((_, rating)) => lines.push((
            Span::Notice,
            format!("Установлен лазерный диод класса {rating}. Он прикручен на место."),
        )),
    }
    let locked = if p.recharge_locked {
        " она полностью перезаряжается, и"
    } else {
        ""
    };
    lines.push((
        Span::Notice,
        format!(
            "Маленький дисплей показывает, что{locked} осталось {}% заряда.",
            p.energy.max(0) * 10
        ),
    ));
    if p.lens.is_some() {
        lines.push((
            Span::Notice,
            "Перед фокусирующей линзой аккуратно закреплён кристалл. Его можно вынуть \
             кусачками."
                .to_owned(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_have_their_diodes_and_range() {
        let plain = LaserPointer::for_path("/obj/item/laser_pointer/red");
        assert_eq!(plain.rating(), 1);
        assert_eq!(plain.max_range, Some(7));
        assert_eq!(LaserPointer::for_path(UPGRADED).rating(), 3);
        let infinite = LaserPointer::for_path(INFINITE);
        assert_eq!(infinite.rating(), 4);
        assert_eq!(infinite.max_range, None);
    }

    #[test]
    fn a_dry_battery_locks_until_full() {
        let mut p = LaserPointer::for_path(LASER_POINTER);
        for _ in 0..9 {
            assert!(!p.drain());
        }
        assert!(p.drain());
        assert!(p.recharge_locked);
        for _ in 0..9 {
            p.recharge_one();
            assert!(p.recharge_locked);
        }
        p.recharge_one();
        assert!(!p.recharge_locked && !p.recharging);
        assert_eq!(p.energy, MAX_ENERGY);
        // The lens doubles the drain.
        p.lens = Some(BLUESPACE_CRYSTAL.to_owned());
        p.drain();
        assert_eq!(p.energy, MAX_ENERGY - 2);
    }

    #[test]
    fn spt_prob_is_per_second() {
        assert!((spt_prob(20.0, 1.0) - 20.0).abs() < 1e-3);
        assert!(spt_prob(20.0, 0.5) < 20.0);
        assert!((spt_prob(100.0, 0.1) - 100.0).abs() < 1e-3);
    }
}
