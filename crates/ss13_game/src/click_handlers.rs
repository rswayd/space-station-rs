//! Ordered click handler registries of [`crate::inventory`]'s click chain (tg's `attack_self`,
//! `attack_hand`, `item_interaction`/`interact_with_atom` chains).
//!
//! Each module registers its hooks from its own plugin with
//! [`AddClickHandler::add_click_handler`]; the inventory runs a phase's handlers by ascending
//! priority and stops at the first that returns `true` (handled). Nothing handled: the
//! inventory's own fallback (selecting the hand, picking up, placing the item).
//!
//! Priorities are multiples of 100 that reproduce the order of the former single `match`; a new
//! hook takes a free number between its neighbours. Two handlers of one phase never share a
//! priority (registration panics), so the order never depends on plugin order. `core` marks
//! handlers registered by `crate::inventory` itself.
//!
//! [`ClickPhase::AttackSelf`] (the active hand's item clicked, tg `attack_self`):
//!
//! | prio | handler |
//! | ---- | ------- |
//! | 100 | `atmos` analyzer scans the user's turf |
//! | 150 | `silicon::mmi::attack_self` |
//! | 160 | `silicon::aicard::attack_self` |
//! | 200 | `tools::attack_self` |
//! | 300 | `holosign::attack_self` |
//! | 400 | `forensics::server::attack_self` |
//! | 500 | `antag::cult::server::attack_self` |
//! | 600 | `antag::heretic::server::attack_self` |
//! | 700 | `antag::wizard::server::attack_self` |
//! | 800 | `vision::visor_attack_self` |
//! | 900 | `security::attack_self` |
//! | 1000 | `guns::attack_self` |
//! | 1100 | `chem::attack_self` |
//! | 1200 | `medical::attack_self` |
//! | 1300 | `disposals::attack_self` |
//! | 1400 | `cargo::mail::attack_self` |
//! | 1500 | `antag::nukeop::server::attack_self` |
//! | 1600 | `antag::syndie_items::attack_self` |
//! | 1700 | `devices::attack_self` |
//! | 1750 | `radiation::geiger_attack_self` |
//! | 1800 | `explosions::sources::c4_attack_self` |
//! | 1900 | `assemblies::server::attack_self` |
//! | 2000 | `grenades::attack_self` |
//! | 2100 | `tables::attack_self` |
//! | 2200 | `furniture::attack_self` |
//! | 2300 | `atmos_machinery::rpd::attack_self` |
//! | 2400 | `science::experi_scanner::attack_self` |
//! | 2500 | `atmos_machinery::fitting::attack_self` |
//! | 2600 | `modpc::pda::attack_self` |
//! | 2700 | `radio::attack_self` |
//! | 2800 | `martial::attack_self` |
//! | 2900 | `botany::attack_self` |
//! | 3000 | `service::server::attack_self` |
//! | 3100 | `silicon::law_rack::attack_self` |
//!
//! [`ClickPhase::OtherHand`] (the other hand's item clicked; `held` is the active hand's):
//!
//! | prio | handler |
//! | ---- | ------- |
//! | 100 | `modsuit::server::hand_interaction` |
//! | 150 | `silicon::mmi::item_interaction` |
//! | 200 | `bots::construction::item_interaction` |
//! | 300 | `forensics::server::item_interaction` |
//! | 400 | `medical::cyberimps::item_interaction` |
//! | 500 | core: a storage opens or takes the held item |
//! | 600 | `implants::item_interaction` |
//! | 700 | `assemblies::server::item_interaction` |
//! | 800 | `grenades::item_interaction` |
//! | 900 | `service::server::item_interaction` |
//! | 1000 | `antag::uplink::item_interaction` |
//! | 1100 | `antag::syndie_items::item_interaction` |
//! | 1200 | `modpc::pda::item_interaction` |
//! | 1250 | `radiation::geiger_interact` |
//! | 1300 | `disposals::mail_interaction` |
//! | 1400 | `chem::item_interaction` |
//! | 1500 | `guns::item_interaction` |
//! | 1600 | `guns::attack_hand` (empty active hand) |
//! | 1700 | `construction::split_into_hand` (empty active hand) |
//!
//! [`ClickPhase::EmptyHand`] (a world click with the active hand empty; tg `attack_hand`):
//!
//! | prio | handler |
//! | ---- | ------- |
//! | 100 | `silicon::interact::attack_robot` |
//! | 200 | `silicon::cyborg::inventory_action` (`EmptyHandClick`) |
//! | 300 | `antag::cult::server::interact` |
//! | 400 | `antag::heretic::server::interact` |
//! | 500 | `kitchen::interact` |
//! | 600 | `cooking::interact` |
//! | 700 | `smartfridge::interact` |
//! | 800 | `gibber::interact` |
//! | 900 | `coffee::interact` |
//! | 1000 | `stove::interact` |
//! | 1100 | `barsign::interact` |
//! | 1200 | `icecream::interact` |
//! | 1300 | `chem_dispenser::interact` |
//! | 1400 | `chem_master::interact` |
//! | 1500 | `chem_heater::interact` |
//! | 1600 | `botany::interact` |
//! | 1700 | `multiz::interact` |
//! | 1800 | `atmos_machinery::windows::interact` |
//! | 1900 | `power::chargers::interact` |
//! | 2000 | `wallmount::interact` |
//! | 2100 | `power::apc_ui::interact` |
//! | 2200 | `mecha::bay::interact` |
//! | 2300 | `mecha::core::attack_hand` |
//! | 2400 | `modpc::pda::console_interact` |
//! | 2500 | `disposals::interact` |
//! | 2600 | `genetics::console::interact` |
//! | 2700 | `tables::attack_hand` |
//! | 2800 | `falsewall::attack_hand` |
//! | 2900 | `mining::interact` |
//! | 3000 | `holosign::attack_hand` |
//! | 3100 | `plumbing::interact` |
//! | 3200 | `service::server::interact` |
//! | 3300 | `defenses::server::interact` |
//! | 3400 | `silicon::law_rack::interact` |
//!
//! [`ClickPhase::HeldOnTarget`] (a world click with the active hand's item; tg
//! `item_interaction`, `interact_with_atom`, `pre_attack`, `attackby`):
//!
//! | prio | handler |
//! | ---- | ------- |
//! | 100 | `antag::uplink::item_interaction` |
//! | 200 | `antag::syndie_items::item_interaction` |
//! | 300 | `emag::interact_with_atom` (left click) |
//! | 400 | `modsuit::server::item_interaction` |
//! | 500 | `assemblies::server::item_interaction` |
//! | 600 | `grenades::item_interaction` |
//! | 700 | `implants::item_interaction` |
//! | 800 | `defenses::server::item_interaction` |
//! | 900 | `silicon::law_rack::item_interaction` |
//! | 1000 | `forensics::server::item_interaction` |
//! | 1100 | `explosions::sources::c4_interact` |
//! | 1200 | `explosions::sources::fueltank_welder_act` |
//! | 1300 | `antag::cult::server::item_interaction` |
//! | 1400 | `antag::heretic::server::item_interaction` |
//! | 1420 | `silicon::aicard::interact_with_atom` |
//! | 1450 | `silicon::mmi::item_interaction` |
//! | 1460 | `silicon::ai_core::item_interaction` |
//! | 1500 | `bots::construction::item_interaction` |
//! | 1600 | `machine_frame::item_interaction` |
//! | 1650 | `radiation::geiger_interact` |
//! | 1700 | `service::server::item_interaction` |
//! | 1750 | `medical::bodyparts::item_interaction` |
//! | 1800 | `kitchen::item_interaction` |
//! | 1900 | `cooking::item_interaction` |
//! | 2000 | `smartfridge::item_interaction` |
//! | 2100 | `coffee::item_interaction` |
//! | 2200 | `stove::item_interaction` |
//! | 2300 | `icecream::item_interaction` |
//! | 2400 | `chem_dispenser::item_interaction` |
//! | 2500 | `chem_master::item_interaction` |
//! | 2600 | `chem_heater::item_interaction` |
//! | 2700 | `wallmount::item_interaction` |
//! | 2800 | `power::chargers::item_interaction` |
//! | 2900 | `mecha::item_interaction` |
//! | 3000 | `mining::item_interaction` |
//! | 3100 | `science::item_interaction` |
//! | 3200 | `botany::item_interaction` |
//! | 3300 | `food::item_interaction` |
//! | 3400 | `plumbing::item_interaction` |
//! | 3500 | `janitor::item_interaction` |
//! | 3600 | `genetics::console::item_interaction` |
//! | 3700 | `chem::item_interaction` |
//! | 3800 | `medical::item_interaction` |
//! | 3900 | `multiz::interact` (out of combat mode) |
//! | 4000 | `atmos_machinery::windows::item_interaction` |
//! | 4100 | `modpc::pda::item_interaction_secondary` (right click) |
//! | 4200 | `disposals::item_interaction` |
//! | 4300 | `tables::item_interaction` |
//! | 4400 | `science::experi_scanner::pre_attack` (not on living mobs) |
//! | 4500 | `construction::item_interaction` |
//! | 4600 | core: a cyborg module or `NoDrop` item stops here |
//! | 4650 | `lockbox::item_interaction` (`closets` plugin) |
//! | 4700 | core: a storage takes the held item |
//! | 4800 | `cargo::bounty::item_interaction` |
//! | 4900 | `brig::prisoner::item_interaction` |
//! | 5000 | `cargo::scanner::item_interaction` |
//! | 5100 | `atmos` analyzer scans the clicked atom |
//! | 5200 | `modpc::pda::item_interaction` |

use bevy::prelude::*;
use ss13_shared::TilePos;
use ss13_shared::protocol::ClickModifiers;

/// A step of the click chain with its own ordered handlers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickPhase {
    /// The active hand's item clicked in its hand slot (tg `attack_self`); `target` = `held`.
    AttackSelf,
    /// The other hand's item (`target`) clicked in its hand slot, `held` the active hand's item
    /// (may be empty).
    OtherHand,
    /// A world click on `target` with the active hand empty (tg `attack_hand`, `interact`).
    EmptyHand,
    /// A world click on `target` with `held` (tg `item_interaction`, `attackby`).
    HeldOnTarget,
}

impl ClickPhase {
    const COUNT: usize = 4;

    fn index(self) -> usize {
        match self {
            Self::AttackSelf => 0,
            Self::OtherHand => 1,
            Self::EmptyHand => 2,
            Self::HeldOnTarget => 3,
        }
    }
}

/// What a click handler sees.
#[derive(Clone, Copy, Debug)]
pub struct ClickCtx {
    pub client: Entity,
    pub mob: Entity,
    pub mob_pos: TilePos,
    /// The active hand's item.
    pub held: Option<Entity>,
    /// The clicked entity: the item in the clicked hand slot, or the world click's target.
    pub target: Entity,
    /// The target's tile (the mob's own for hand slot clicks).
    pub pos: TilePos,
    /// Default for hand slot clicks.
    pub modifiers: ClickModifiers,
    /// The active hand's index (`Inventory::active`).
    pub active_hand: usize,
}

/// Handles the click (`true`) or passes it on (`false`).
pub type ClickHandler = fn(&mut World, &ClickCtx) -> bool;

/// The handlers of every phase, sorted by ascending priority.
#[derive(Resource, Default)]
pub struct ClickHandlers {
    phases: [Vec<(u32, ClickHandler)>; ClickPhase::COUNT],
}

impl ClickHandlers {
    /// Inserts `handler` in priority order. Panics on a priority already taken in `phase`.
    pub fn add(&mut self, phase: ClickPhase, priority: u32, handler: ClickHandler) {
        let list = &mut self.phases[phase.index()];
        let at = list.partition_point(|&(p, _)| p < priority);
        assert!(
            list.get(at).is_none_or(|&(p, _)| p != priority),
            "click handler priority {priority} of {phase:?} is taken"
        );
        list.insert(at, (priority, handler));
    }

    fn get(&self, phase: ClickPhase, index: usize) -> Option<ClickHandler> {
        self.phases[phase.index()].get(index).map(|&(_, h)| h)
    }
}

/// Runs `phase`'s handlers in priority order until one returns `true`; whether one did.
pub fn run(world: &mut World, phase: ClickPhase, ctx: &ClickCtx) -> bool {
    // By index: a handler gets the whole world, the registry included.
    for index in 0.. {
        let Some(handler) = world
            .get_resource::<ClickHandlers>()
            .and_then(|h| h.get(phase, index))
        else {
            break;
        };
        if handler(world, ctx) {
            return true;
        }
    }
    false
}

pub trait AddClickHandler {
    /// Registers `handler` in `phase` at `priority` (see the tables in [`crate::click_handlers`]).
    fn add_click_handler(
        &mut self,
        phase: ClickPhase,
        priority: u32,
        handler: ClickHandler,
    ) -> &mut Self;
}

impl AddClickHandler for App {
    fn add_click_handler(
        &mut self,
        phase: ClickPhase,
        priority: u32,
        handler: ClickHandler,
    ) -> &mut Self {
        self.world_mut()
            .get_resource_or_init::<ClickHandlers>()
            .add(phase, priority, handler);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Calls(Vec<u32>);

    fn ctx(world: &mut World) -> ClickCtx {
        let e = world.spawn_empty().id();
        ClickCtx {
            client: e,
            mob: e,
            mob_pos: TilePos::default(),
            held: None,
            target: e,
            pos: TilePos::default(),
            modifiers: ClickModifiers::default(),
            active_hand: 0,
        }
    }

    fn call(world: &mut World, n: u32, handled: bool) -> bool {
        world.resource_mut::<Calls>().0.push(n);
        handled
    }

    #[test]
    fn runs_in_priority_order_until_handled() {
        let mut app = App::new();
        app.init_resource::<Calls>();
        // Registered out of order.
        app.add_click_handler(ClickPhase::HeldOnTarget, 300, |w, _| call(w, 300, true))
            .add_click_handler(ClickPhase::HeldOnTarget, 100, |w, _| call(w, 100, false))
            .add_click_handler(ClickPhase::HeldOnTarget, 400, |w, _| call(w, 400, true))
            .add_click_handler(ClickPhase::HeldOnTarget, 200, |w, _| call(w, 200, false))
            .add_click_handler(ClickPhase::EmptyHand, 50, |w, _| call(w, 50, true));
        let world = app.world_mut();
        let ctx = ctx(world);
        assert!(run(world, ClickPhase::HeldOnTarget, &ctx));
        assert_eq!(world.resource::<Calls>().0, [100, 200, 300]);
        // A phase without handlers handles nothing.
        assert!(!run(world, ClickPhase::AttackSelf, &ctx));
    }

    #[test]
    fn nothing_handled() {
        let mut app = App::new();
        app.init_resource::<Calls>();
        app.add_click_handler(ClickPhase::OtherHand, 200, |w, _| call(w, 200, false))
            .add_click_handler(ClickPhase::OtherHand, 100, |w, _| call(w, 100, false));
        let world = app.world_mut();
        let ctx = ctx(world);
        assert!(!run(world, ClickPhase::OtherHand, &ctx));
        assert_eq!(world.resource::<Calls>().0, [100, 200]);
    }

    #[test]
    #[should_panic(expected = "is taken")]
    fn duplicate_priority_panics() {
        let mut app = App::new();
        app.add_click_handler(ClickPhase::EmptyHand, 100, |_, _| false)
            .add_click_handler(ClickPhase::EmptyHand, 100, |_, _| true);
    }
}
