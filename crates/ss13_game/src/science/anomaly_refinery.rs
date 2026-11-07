//! Raw anomaly cores and the anomaly refinery, after tg `/obj/item/raw_anomaly_core`
//! (`code/modules/research/anomaly/raw_anomaly.dm`) and `/obj/machinery/research/anomaly_refinery`
//! (`anomaly_refinery.dm`) with its tgui `AnomalyRefinery` window.
//!
//! - A raw core (any type but the random spawner) goes in while the refinery is idle and has
//!   no core, if a finished core of its type can still be made this round; a transfer valve with
//!   both tanks goes in while it has no bomb.
//! - The window shows both tanks and a simulated merge of their gas ("react" steps it), the
//!   core with the explosion radius it needs, and ejects the core and the bomb; "swap" picks
//!   which tank is shown as the target.
//! - "start_implosion" opens the valve. A tank rupturing inside it is an internal explosion:
//!   nothing blows up (`COMSIG_CANCEL_EXPLOSION`), the gas is lost, and a light radius of at
//!   least the required one (4 tiles, up to 20 as the round's cores of that type run out:
//!   `4 + 16 / limit × made`) turns the raw core into a finished core of its type, counting
//!   towards the round's limit. After 10 s the test ends, the result is said and what is inside
//!   drops out.
// shortcut: the valve always merges into its secondary tank (the port's `toggle_valve`), so
// "swap" only changes what the window lists first; no emag firmware fault, no room shake, no
// circuit board or deconstruction; the window lists each gas mixture's pressure, temperature and
// moles (tg's `gas_mixture_parser` also lists the gases).

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// tgui `AnomalyRefinery`.
pub struct AnomalyRefineryUi;

impl UiInterface for AnomalyRefineryUi {
    const NAME: &'static str = "AnomalyRefinery";
    type Data = AnomalyRefineryData;
    type StaticData = ();
}

/// One parsed gas mixture (`gas_mixture_parser`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct RefineryGas {
    pub name: String,
    pub pressure: f64,
    pub temperature: f64,
    pub total_moles: f64,
}

/// `ui_data`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AnomalyRefineryData {
    /// The target tank, the other tank, the simulated merge (absent ones are skipped).
    pub gas_list: Vec<RefineryGas>,
    pub valve_present: bool,
    pub valve_ready: bool,
    pub reaction_increment: u32,
    pub core: Option<String>,
    pub required_radius: Option<u32>,
    pub active: bool,
}

/// Act names.
pub mod acts {
    pub const REACT: &str = "react";
    pub const EJECT_BOMB: &str = "eject_bomb";
    pub const EJECT_CORE: &str = "eject_core";
    pub const START: &str = "start_implosion";
    pub const SWAP: &str = "swap";
}

#[cfg(feature = "server")]
pub use server::*;

#[cfg(feature = "server")]
mod server {
    use bevy::ecs::message::{MessageCursor, Messages};
    use bevy::prelude::*;
    use ss13_shared::chat::Span;
    use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

    use super::*;
    use crate::assemblies::ttv::{toggle_valve, valve};
    use crate::construction::path_of;
    use crate::events::anomalies::{AnomalyKind, CoresMade};
    use crate::internals::TankAir;
    use crate::obj_damage::is_under;
    use crate::storage::StoredIn;

    pub const REFINERY: &str = "/obj/machinery/research/anomaly_refinery";
    pub const RAW_CORE: &str = "/obj/item/raw_anomaly_core";
    const RAW_RANDOM: &str = "/obj/item/raw_anomaly_core/random";
    const CORE: &str = "/obj/item/assembly/signaler/anomaly";
    /// `MAX_RADIUS_REQUIRED`, `MIN_RADIUS_REQUIRED`.
    const MAX_RADIUS: f64 = 20.0;
    const MIN_RADIUS: f64 = 4.0;
    /// `COMPRESSION_TEST_TIME`: `SSOBJ_DT` (2 s) × 5.
    const TEST_SECS: f64 = 10.0;

    /// The refinery's state (`inserted_core`, `inserted_bomb`, `tank_to_target`, `active`,
    /// `test_status`, the timeout, `combined_gasmix`, `reaction_increment`).
    #[derive(Component, Clone, Debug, Default)]
    pub struct Refinery {
        pub core: Option<Entity>,
        pub bomb: Option<Entity>,
        /// `tank_to_target == tank_two`.
        pub target_two: bool,
        pub active: bool,
        pub test_status: Option<String>,
        pub timeout_at: Option<f64>,
        pub combined: Option<TankAir>,
        pub reaction_increment: u32,
    }

    /// What the window shows.
    #[derive(Component, Clone, Debug, Default, PartialEq)]
    pub struct RefineryView(pub AnomalyRefineryData);

    pub struct AnomalyRefineryPlugin;

    impl Plugin for AnomalyRefineryPlugin {
        fn build(&self, app: &mut App) {
            use crate::click_handlers::{AddClickHandler, ClickPhase};
            app.add_click_handler(ClickPhase::EmptyHand, 3481, |w, c| {
                !c.modifiers.right && interact(w, c.client, c.mob, c.target)
            });
            app.add_click_handler(ClickPhase::HeldOnTarget, 1581, |w, c| {
                c.held
                    .is_some_and(|h| item_interaction(w, c.client, c.mob, h, c.target))
            });
            app.register_ui::<AnomalyRefineryUi>().add_systems(
                Update,
                (refinery_acts.in_set(UiSystems::Handle), process_refineries),
            );
        }
    }

    fn inside(world: &World, e: Option<Entity>, container: Entity) -> Option<Entity> {
        e.filter(|&e| world.get::<StoredIn>(e).is_some_and(|s| s.0 == container))
    }

    /// The refinery state of `e` (given on first use), with what left it forgotten (`Exited`).
    pub fn refinery(world: &mut World, e: Entity) -> Option<Refinery> {
        if let Some(r) = world.get::<Refinery>(e) {
            let mut r = r.clone();
            r.core = inside(world, r.core, e);
            r.bomb = inside(world, r.bomb, e);
            if r.bomb.is_none() {
                r.combined = None;
                r.reaction_increment = 0;
            }
            return Some(r);
        }
        if !path_of(world, e).is_some_and(|p| is_under(&p, REFINERY)) {
            return None;
        }
        let r = Refinery::default();
        world
            .entity_mut(e)
            .insert((r.clone(), RefineryView::default()));
        Some(r)
    }

    fn tanks(world: &mut World, r: &Refinery) -> Option<(Entity, Entity)> {
        let v = valve(world, r.bomb?)?;
        Some((v.tank_one?, v.tank_two?))
    }

    fn parse(name: String, t: &TankAir) -> RefineryGas {
        RefineryGas {
            name,
            pressure: t.pressure(),
            temperature: t.air.temperature,
            total_moles: t.air.total_moles(),
        }
    }

    fn set(world: &mut World, e: Entity, r: Refinery) {
        let mut gas_list = Vec::new();
        let pair = tanks(world, &r);
        if let Some((one, two)) = pair {
            let (target, other) = if r.target_two { (two, one) } else { (one, two) };
            for t in [target, other] {
                if let Some(air) = world.get::<TankAir>(t) {
                    gas_list.push(parse(crate::chem::name_of(world, t), air));
                }
            }
        }
        if let Some(combined) = &r.combined {
            gas_list.push(parse("Combined Gasmix".to_owned(), combined));
        }
        let core = r.core.map(|c| crate::chem::name_of(world, c));
        let required_radius = r.core.and_then(|c| required_radius(world, c));
        let view = AnomalyRefineryData {
            gas_list,
            valve_present: r.bomb.is_some(),
            valve_ready: pair.is_some(),
            reaction_increment: r.reaction_increment,
            core,
            required_radius,
            active: r.active,
        };
        world.entity_mut(e).insert((r, RefineryView(view)));
    }

    /// The finished core type of a raw core (its `anomaly_type`: the same subtype name).
    pub fn core_type(raw_path: &str) -> Option<String> {
        let kind = raw_path.strip_prefix(RAW_CORE)?.strip_prefix('/')?;
        (!kind.is_empty() && !is_under(raw_path, RAW_RANDOM)).then(|| format!("{CORE}/{kind}"))
    }

    /// `get_required_radius(anomaly_type)`: `None` once the round's cores of the type are used
    /// up (or the type has no ported anomaly).
    pub fn required_radius(world: &World, raw: Entity) -> Option<u32> {
        let kind = path_of(world, raw)
            .and_then(|p| core_type(&p))
            .and_then(|c| AnomalyKind::of_core(&c))?;
        let made = world
            .get_resource::<CoresMade>()
            .and_then(|m| m.0.get(&kind).copied())
            .unwrap_or(0);
        radius_for(made, kind.max_cores())
    }

    /// The linear autoscale: `clamp(round(4 + 16 / limit × made), 4, 20)`.
    pub fn radius_for(made: u32, limit: u32) -> Option<u32> {
        if made >= limit || limit == 0 {
            return None;
        }
        let radius = MIN_RADIUS + (MAX_RADIUS - MIN_RADIUS) / f64::from(limit) * f64::from(made);
        Some(radius.round().clamp(MIN_RADIUS, MAX_RADIUS) as u32)
    }

    fn say(world: &mut World, e: Entity, text: &str) {
        crate::atmos_machinery::machine_say(world, e, text);
    }

    fn tell(world: &mut World, client: Entity, span: Span, text: impl Into<String>) {
        crate::tools::tell(world, client, span, text);
    }

    /// `attack_hand`: the window.
    pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
        let Some(r) = refinery(world, target) else {
            return false;
        };
        set(world, target, r);
        let title = crate::chem::name_of(world, target);
        world.write_message(OpenUi::new::<AnomalyRefineryUi>(client, mob, target, title));
        true
    }

    /// `item_interaction`: a raw core or a ready transfer valve in.
    pub fn item_interaction(
        world: &mut World,
        client: Entity,
        _mob: Entity,
        held: Entity,
        target: Entity,
    ) -> bool {
        let Some(path) = path_of(world, held) else {
            return false;
        };
        let raw = is_under(&path, RAW_CORE);
        let is_valve = is_under(&path, crate::assemblies::ttv::VALVE);
        if !raw && !is_valve {
            return false;
        }
        let Some(mut r) = refinery(world, target) else {
            return false;
        };
        let (name, machine) = (
            crate::chem::name_of(world, held),
            crate::chem::name_of(world, target),
        );
        if r.active {
            tell(
                world,
                client,
                Span::Warning,
                format!("Вы не можете вставить {name} в {machine}, пока он активен."),
            );
            return true;
        }
        if raw {
            if r.core.is_some() {
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!("Уже есть ядро в {machine}."),
                );
                return true;
            }
            // tg puts the core in first and leaves it inside unreferenced: it stays out here.
            if required_radius(world, held).is_none() {
                say(
                    world,
                    target,
                    &format!(
                        "К сожалению, из-за уменьшения запасов сжатой аномальной материи, {name} \
                         и любые ядра такого типа больше не имеют достаточного уровня качества \
                         для сжатия в рабочее ядро."
                    ),
                );
                return true;
            }
            crate::storage::put_inside(world, held, target);
            r.core = Some(held);
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы вставляете {name} в {machine}."),
            );
        } else {
            if r.bomb.is_some() {
                tell(
                    world,
                    client,
                    Span::Warning,
                    format!("Уже есть бомба в {machine}."),
                );
                return true;
            }
            let ready =
                valve(world, held).is_some_and(|v| v.tank_one.is_some() && v.tank_two.is_some());
            if !ready {
                tell(world, client, Span::Warning, format!("{name} не завершён."));
                return true;
            }
            crate::storage::put_inside(world, held, target);
            r.bomb = Some(held);
            r.target_two = true;
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы вставляете {name} в {machine}."),
            );
        }
        set(world, target, r);
        true
    }

    /// `start_test()`.
    pub fn start_test(world: &mut World, e: Entity) {
        let Some(mut r) = refinery(world, e) else {
            return;
        };
        if r.active {
            say(world, e, "ОШИБКА: уже выполняется тест сжатия.");
            return;
        }
        if r.core.is_none() || r.bomb.is_none() {
            end_test(
                world,
                e,
                Some("ОШИБКА: отсутствует оборудование. Предметы извлечены."),
            );
            return;
        }
        if tanks(world, &r).is_none() {
            end_test(
                world,
                e,
                Some("ОШИБКА: неисправность клапана перекачки. Предметы извлечены."),
            );
            return;
        }
        say(
            world,
            e,
            "Начинается проверка на сжатие. Открытие клапана перекачки.",
        );
        r.active = true;
        r.test_status = None;
        r.timeout_at = Some(world.resource::<Time>().elapsed_secs_f64() + TEST_SECS);
        let bomb = r.bomb;
        set(world, e, r);
        if let Some(bomb) = bomb
            && !valve(world, bomb).is_some_and(|v| v.open)
        {
            toggle_valve(world, bomb);
        }
    }

    /// `end_test(message)`: inactive, both items out onto the floor.
    pub fn end_test(world: &mut World, e: Entity, message: Option<&str>) {
        let Some(mut r) = refinery(world, e) else {
            return;
        };
        r.active = false;
        r.test_status = None;
        r.timeout_at = None;
        for item in [r.core.take(), r.bomb.take()].into_iter().flatten() {
            crate::assemblies::server::drop_out(world, item, e);
        }
        r.combined = None;
        r.reaction_increment = 0;
        set(world, e, r);
        if let Some(message) = message {
            say(world, e, message);
        }
    }

    /// `COMSIG_ATOM_INTERNAL_EXPLOSION` of a refinery holding the valve `tank` is in: the
    /// `check_test` result. True when the explosion is cancelled (the tank was in one).
    pub fn internal_explosion(world: &mut World, tank: Entity, power: f32) -> bool {
        let Some(bomb) = world.get::<StoredIn>(tank).map(|s| s.0) else {
            return false;
        };
        let Some(e) = world.get::<StoredIn>(bomb).map(|s| s.0) else {
            return false;
        };
        let Some(mut r) = refinery(world, e) else {
            return false;
        };
        let Some(core) = r.core else {
            r.test_status = Some("ОШИБКА: во время детонации отсутствует ядро.".to_owned());
            set(world, e, r);
            return true;
        };
        let (_, range) = crate::explosions::wave::dyn_ranges(power);
        let range = range as u32;
        let name = crate::chem::name_of(world, core);
        let Some(required) = required_radius(world, core) else {
            r.test_status = Some(format!(
                "В результате взрыва мощность не была достаточной для сжатия {name}. Предметы \
                 извлечены."
            ));
            set(world, e, r);
            return true;
        };
        if range < required {
            r.test_status = Some(format!(
                "В результате взрыва мощность не была достаточной для сжатия {name}. Предметы \
                 извлечены."
            ));
            set(world, e, r);
            return true;
        }
        if r.test_status.is_some() {
            return true;
        }
        r.core = create_core(world, core, e);
        r.test_status = Some(format!(
            "Успех. Теоретический диапазон взрыва составляет {range}. Требуемый диапазон \
             составлял {required}. Производство ядра завершено."
        ));
        set(world, e, r);
        true
    }

    /// `create_core(src, del_self = TRUE, count_towards_limit = TRUE)`.
    fn create_core(world: &mut World, raw: Entity, refinery: Entity) -> Option<Entity> {
        let path = path_of(world, raw).and_then(|p| core_type(&p))?;
        let kind = AnomalyKind::of_core(&path)?;
        let pos = crate::explosions::turf_of(world, refinery)?;
        // Without the core's prototype (no generated assets) a bare core item.
        let core = crate::events::anomalies::spawn_core(world, kind, pos).unwrap_or_else(|| {
            world
                .spawn(ss13_mapload::DmItem {
                    path: path.clone(),
                    ..default()
                })
                .id()
        });
        crate::storage::put_inside(world, core, refinery);
        *world
            .get_resource_or_insert_with(CoresMade::default)
            .0
            .entry(kind)
            .or_default() += 1;
        world.despawn(raw);
        Some(core)
    }

    /// `simulate_valve()`: the first step merges copies of both tanks, the next ones react.
    fn simulate_valve(world: &mut World, e: Entity) {
        let Some(mut r) = refinery(world, e) else {
            return;
        };
        let Some((one, two)) = tanks(world, &r) else {
            eject_bomb(world, e, None);
            return;
        };
        if r.reaction_increment == 0 {
            let (Some(a), Some(b)) = (
                world.get::<TankAir>(one).cloned(),
                world.get::<TankAir>(two).cloned(),
            ) else {
                return;
            };
            let mut combined = TankAir {
                air: a.air.clone(),
                volume: a.volume + b.volume,
                distribute_pressure: 0.0,
            };
            combined.air.merge(&b.air);
            r.combined = Some(combined);
        } else if let Some(c) = r.combined.as_mut() {
            let volume = c.volume;
            c.air.react(
                ss13_atmos::reactions::Holder::Other,
                volume,
                &mut ss13_atmos::reactions::Exposures::default(),
            );
        }
        r.reaction_increment += 1;
        set(world, e, r);
    }

    fn eject_bomb(world: &mut World, e: Entity, user: Option<Entity>) {
        let Some(mut r) = refinery(world, e) else {
            return;
        };
        let Some(bomb) = r.bomb.filter(|_| !r.active) else {
            return;
        };
        match user {
            Some(mob) => crate::assemblies::server::to_hands(world, mob, bomb),
            None => crate::assemblies::server::drop_out(world, bomb, e),
        }
        r.bomb = None;
        r.combined = None;
        r.reaction_increment = 0;
        set(world, e, r);
    }

    fn eject_core(world: &mut World, e: Entity, user: Entity) {
        let Some(mut r) = refinery(world, e) else {
            return;
        };
        let Some(core) = r.core.filter(|_| !r.active) else {
            return;
        };
        crate::assemblies::server::to_hands(world, user, core);
        r.core = None;
        set(world, e, r);
    }

    /// The timeout (`timeout_test`), and the window kept in step with the tanks.
    fn process_refineries(world: &mut World) {
        let now = world.resource::<Time>().elapsed_secs_f64();
        let mut q = world.query::<(Entity, &Refinery)>();
        let due: Vec<(Entity, bool)> = q
            .iter(world)
            .filter(|(_, r)| r.active)
            .map(|(e, r)| (e, r.timeout_at.is_some_and(|t| t <= now)))
            .collect();
        for (e, timed_out) in due {
            if !timed_out {
                if let Some(r) = refinery(world, e) {
                    set(world, e, r);
                }
                continue;
            }
            let status = world
                .get::<Refinery>(e)
                .and_then(|r| r.test_status.clone())
                .unwrap_or_else(|| {
                    "Клапан перекачки привел к незначительной мощности взрыва. Предметы извлечены."
                        .to_owned()
                });
            end_test(world, e, Some(&status));
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum RefineryAct {
        React,
        EjectBomb,
        EjectCore,
        Start,
        Swap,
    }

    impl ServerUi for AnomalyRefineryUi {
        type Source = &'static RefineryView;
        type UserParam = ();
        type Act = RefineryAct;

        fn static_data(_: &RefineryView) {}

        fn data(view: &RefineryView, _: Entity, _: &()) -> AnomalyRefineryData {
            view.0.clone()
        }

        fn parse_act(action: &str, _: &UiParams) -> Result<RefineryAct, UiReject> {
            Ok(match action {
                acts::REACT => RefineryAct::React,
                acts::EJECT_BOMB => RefineryAct::EjectBomb,
                acts::EJECT_CORE => RefineryAct::EjectCore,
                acts::START => RefineryAct::Start,
                acts::SWAP => RefineryAct::Swap,
                _ => return Err(UiReject::UnknownAction),
            })
        }
    }

    /// `ui_act`, every check at the moment of effect.
    pub fn apply_act(world: &mut World, actor: Entity, e: Entity, act: RefineryAct) {
        match act {
            RefineryAct::React => simulate_valve(world, e),
            RefineryAct::EjectBomb => eject_bomb(world, e, Some(actor)),
            RefineryAct::EjectCore => eject_core(world, e, actor),
            RefineryAct::Start => start_test(world, e),
            RefineryAct::Swap => {
                let Some(mut r) = refinery(world, e) else {
                    return;
                };
                if tanks(world, &r).is_none() {
                    eject_bomb(world, e, None);
                    return;
                }
                r.target_two = !r.target_two;
                set(world, e, r);
            }
        }
    }

    fn refinery_acts(
        world: &mut World,
        mut cursor: Local<MessageCursor<UiActed<AnomalyRefineryUi>>>,
    ) {
        let pending: Vec<(Entity, Entity, RefineryAct)> = cursor
            .read(world.resource::<Messages<UiActed<AnomalyRefineryUi>>>())
            .map(|a| (a.actor, a.source, a.act))
            .collect();
        for (actor, e, act) in pending {
            apply_act(world, actor, e, act);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn radius_scales_with_cores_made() {
            assert_eq!(radius_for(0, 8), Some(4));
            assert_eq!(radius_for(4, 8), Some(12));
            assert_eq!(radius_for(7, 8), Some(18));
            assert_eq!(radius_for(8, 8), None);
            assert_eq!(radius_for(2, 3), Some(15));
        }

        #[test]
        fn raw_cores_map_to_their_finished_type() {
            assert_eq!(
                core_type("/obj/item/raw_anomaly_core/grav").as_deref(),
                Some("/obj/item/assembly/signaler/anomaly/grav")
            );
            assert_eq!(core_type("/obj/item/raw_anomaly_core/random"), None);
            assert_eq!(core_type("/obj/item/raw_anomaly_core"), None);
        }
    }
}

#[cfg(feature = "client")]
pub mod client {
    use ss13_tgui::client::ClientUi;
    use ss13_ui_kit::prelude::*;

    use super::*;

    fn button(
        icon: &'static str,
        content: String,
        action: &str,
        disabled: bool,
    ) -> impl Scene + use<> {
        let action = action.to_owned();
        let params = ActParams(Vec::new());
        bsn! {
            @ss13_ui_kit::Button { @icon: icon, @content: content }
            ss13_ui_kit::Button { disabled: disabled, selected: false }
            Act(action)
            params
        }
    }

    fn gas_line(gas: &RefineryGas) -> impl Scene + use<> {
        let label = gas.name.clone();
        let content = format!(
            "{:.2} kPa, {:.2} K, {:.2} mol",
            gas.pressure, gas.temperature, gas.total_moles
        );
        bsn! { @LabeledListItem { @label: label, @content: content } }
    }

    impl ClientUi for AnomalyRefineryUi {
        const WIDTH: f32 = 550.0;
        const HEIGHT: f32 = 350.0;

        fn content(_: &(), data: &AnomalyRefineryData) -> impl Scene {
            let core = data.core.clone().unwrap_or_else(|| "None".to_owned());
            let radius = data
                .required_radius
                .map_or_else(|| "-".to_owned(), |r| r.to_string());
            let gases: Vec<_> = data.gas_list.iter().map(gas_line).collect();
            let idle = !data.active;
            let buttons = vec![
                button(
                    "atom",
                    "Simulate".to_owned(),
                    acts::REACT,
                    !data.valve_ready,
                ),
                button(
                    "exchange-alt",
                    "Swap".to_owned(),
                    acts::SWAP,
                    !data.valve_ready,
                ),
                button(
                    "eject",
                    "Eject Bomb".to_owned(),
                    acts::EJECT_BOMB,
                    !data.valve_present || !idle,
                ),
                button(
                    "eject",
                    "Eject Core".to_owned(),
                    acts::EJECT_CORE,
                    data.core.is_none() || !idle,
                ),
                button(
                    "bomb",
                    "Implode".to_owned(),
                    acts::START,
                    !data.valve_ready || data.core.is_none() || !idle,
                ),
            ];
            let steps = format!("{}", data.reaction_increment);
            bsn! {
                @WindowContent
                Children [
                    @Section { @title: "Core" }
                    Children [
                        @LabeledList
                        Children [
                            @LabeledListItem { @label: "Core", @content: core }
                            --
                            @LabeledListItem { @label: "Required Radius", @content: radius }
                            --
                            @LabeledListItem { @label: "Reactions", @content: steps }
                        ]
                    ]
                    --
                    @Section { @title: "Gases" }
                    Children [
                        @LabeledList
                        Children [ {gases} ]
                    ]
                    --
                    Node { flex_direction: FlexDirection::Row }
                    Children [ {buttons} ]
                ]
            }
        }
    }
}
