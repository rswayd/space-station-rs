//! Cyborgs (server), after tg `/mob/living/silicon/robot` (`code/modules/mob/living/silicon/robot/`),
//! `/obj/item/robot_model` and `/obj/machinery/recharge_station`
//! (`code/game/machinery/rechargestation.dm`).
//!
//! - The Cyborg job spawns a "Default Cyborg-NNN" on a cyborg start with a high-capacity cell,
//!   the round's laws, and links to the AI with the fewest cyborgs (tg `try_connect_to_ai`),
//!   whose laws it then follows (`lawsync`: `ai_to_cyborg`).
//! - Model choice (tg `pick_model`'s radial menu, here tgui's `tgui_input_list` window): the
//!   models tg offers with BandaStation's config (no security), giving the model's sprite, radio
//!   channels and `basic_modules`. The modules sit in the model, worn in the back slot as a
//!   storage: clicking it opens it, a module goes to a hand from there, dropping one (Q) puts it
//!   back (tg `uneq_active`). A cyborg picks nothing up and puts no module down or into other
//!   storages.
//! - The cell (tg `handle_robot_cell`): every second `BORG_MINIMUM_POWER_CONSUMPTION` while
//!   conscious; at 1% of a standard cell the modules drop back into the model, at 0 the cyborg
//!   is in low power mode (no modules) until charged. Silicon access opens every door
//!   (`allowed()` is TRUE for silicons): an internal ID with every job's access.
//! - The recharging station: a click with an empty hand opens or closes it (tg `toggle_open`);
//!   closing takes the cyborg on its tile in, and while closed and powered it charges the cell
//!   by tg's `recharge_speed` (two tier 1 capacitors, a high-capacity cell: 1 kW). Walking out
//!   opens it (tg `relaymove`).
// shortcut: no cyborg HUD (the human slots are shown; only the hands and the back slot work),
// no lamp, ion thrusters, model transform animation, skins, upgrades, emag modules, lockdown,
// tipping or MMI; consumables are not respawned at the station (nor their 0.5% of the charge
// drawn, which tg takes only when something was restocked).
// shortcut: cyborgs are not EMP-able (no EMP effects exist yet).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::silicon::{ROBOT_TYPE, RobotModel};
use ss13_mapload::{DmObj, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::server::{OpenUi, UiActed, UiSystems};
use ss13_world::Mover;

use super::ai::AiCore;
use super::laws::{Laws, round_default};
use super::ui::ListInputStatic;
use super::{
    ListInput, ListInputWindow, Silicon, Silicons, attach_client, give_radio, notice, show_laws,
};
use crate::click::{ClickResolved, ResolveClicks};
use crate::health::{HealthSystems, Stat, living_bundle};
use crate::hud::HudData;
use crate::inventory::{CarriedBy, IdCard, Inventory};
use crate::jobs::{Jobs, PlayerJob};
use crate::power::Unpowered;
use crate::radio::{Radio, SiliconRadio};
use crate::server::SpawnPoints;
use crate::storage::Storage;

/// tg `STANDARD_CELL_CHARGE`, joules.
pub const STANDARD_CELL_CHARGE: f64 = 10_000.0;
/// `/obj/item/stock_parts/power_store/cell/high` `maxcharge`.
pub const HIGH_CELL_CHARGE: f64 = STANDARD_CELL_CHARGE * 10.0;
/// tg `BORG_MINIMUM_POWER_CONSUMPTION`, watts.
pub const BORG_MINIMUM_POWER_CONSUMPTION: f64 = 1.0;
/// tg `recharge_speed` of a stock station: `5e-3 * 2 capacitors * high cell maxcharge`.
pub const RECHARGE_SPEED: f64 = 5e-3 * 2.0 * HIGH_CELL_CHARGE;
const ROBOTS_DMI: &str = "icons/mob/silicon/robots.dmi";
const STATION: &str = "/obj/machinery/recharge_station";
const STATION_DMI: &str = "icons/obj/machines/borg_charger.dmi";
/// tg `get_radio_information`.
const RADIO_INFO: &str = "Prefix your message with :b to speak with other cyborgs and AI.";

/// A cyborg's state (tg `/mob/living/silicon/robot` vars).
#[derive(Component, Clone, Debug)]
pub struct Cyborg {
    /// Index into [`Silicons`] models; `None`: the default model, not chosen yet.
    pub model: Option<usize>,
    /// The model item holding the modules (worn in the back slot).
    pub model_item: Option<Entity>,
    /// Cell charge and capacity, joules.
    pub charge: f64,
    pub max_charge: f64,
    pub low_power: bool,
    pub connected_ai: Option<Entity>,
    /// tg `lawupdate`.
    pub lawupdate: bool,
}

/// On a module item (tg `basic_modules` entries): it belongs to its cyborg's model.
#[derive(Component, Clone, Copy, Debug)]
pub struct RobotModule;

/// tg `/obj/machinery/recharge_station`: `state_open` and its occupant.
#[derive(Component, Clone, Copy, Debug)]
pub struct RechargeStation {
    pub open: bool,
    pub occupant: Option<Entity>,
}

pub struct CyborgPlugin;

impl Plugin for CyborgPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, init_stations)
            .add_systems(
                Update,
                (
                    choose_model.in_set(UiSystems::Handle),
                    station_clicks.after(ResolveClicks),
                    (cell_drain, charge_occupants, station_looks)
                        .chain()
                        .after(station_clicks),
                    sync_laws.after(HealthSystems),
                ),
            )
            .add_observer(forget_borg)
            .add_observer(forget_ai);
    }
}

/// A deleted cyborg leaves its AI's `connected_robots` (tg `Destroy` → `set_connected_ai(null)`).
fn forget_borg(remove: On<Remove<Cyborg>>, borgs: Query<&Cyborg>, mut cores: Query<&mut AiCore>) {
    let Some(ai) = borgs.get(remove.entity).ok().and_then(|b| b.connected_ai) else {
        return;
    };
    if let Ok(mut core) = cores.get_mut(ai) {
        core.connected_robots.retain(|&b| b != remove.entity);
    }
}

/// A deleted AI unlinks its cyborgs (tg `/mob/living/silicon/ai/Destroy`).
fn forget_ai(remove: On<Remove<AiCore>>, cores: Query<&AiCore>, mut borgs: Query<&mut Cyborg>) {
    let Ok(core) = cores.get(remove.entity) else {
        return;
    };
    for &borg in &core.connected_robots {
        if let Ok(mut cyborg) = borgs.get_mut(borg)
            && cyborg.connected_ai == Some(remove.entity)
        {
            cyborg.connected_ai = None;
        }
    }
}

/// tg `select_active_ai_with_fewest_borgs`.
fn fewest_borgs_ai(world: &mut World) -> Option<Entity> {
    let mut ais = world.query::<(Entity, &AiCore, &Stat)>();
    ais.iter(world)
        .filter(|(_, _, s)| **s != Stat::Dead)
        .min_by_key(|(_, core, _)| core.connected_robots.len())
        .map(|(e, ..)| e)
}

/// tg `set_connected_ai`, then `lawsync`.
pub(crate) fn connect(world: &mut World, borg: Entity, ai: Entity) {
    let Some(mut cyborg) = world.get_mut::<Cyborg>(borg) else {
        return;
    };
    let old = cyborg.connected_ai.replace(ai);
    if let Some(old) = old.filter(|&o| o != ai)
        && let Some(mut core) = world.get_mut::<AiCore>(old)
    {
        core.connected_robots.retain(|&b| b != borg);
    }
    if let Some(mut core) = world.get_mut::<AiCore>(ai)
        && !core.connected_robots.contains(&borg)
    {
        core.connected_robots.push(borg);
    }
    lawsync(world, borg);
}

/// tg `lawsync`: a linked cyborg with `lawupdate` takes its AI's laws.
fn lawsync(world: &mut World, borg: Entity) {
    let Some(cyborg) = world.get::<Cyborg>(borg) else {
        return;
    };
    let Some(ai) = cyborg.connected_ai.filter(|_| cyborg.lawupdate) else {
        return;
    };
    let Some(laws) = world.get::<Laws>(ai).map(Laws::for_cyborg) else {
        return;
    };
    if world.get::<Laws>(borg) != Some(&laws) {
        world.entity_mut(borg).insert(laws);
    }
}

/// Cyborgs without an AI link to `ai` (tg `/datum/job/ai/after_spawn`).
pub(crate) fn link_orphans(world: &mut World, ai: Entity) {
    let mut borgs = world.query::<(Entity, &Cyborg)>();
    let orphans: Vec<Entity> = borgs
        .iter(world)
        .filter(|(_, c)| c.connected_ai.is_none())
        .map(|(e, _)| e)
        .collect();
    for borg in orphans {
        connect(world, borg, ai);
        if let Some(player) = world.get::<crate::server::PlayerMob>(borg).copied()
            && let Some(laws) = world.get::<Laws>(borg).cloned()
        {
            show_laws(world, player.client, &laws);
        }
    }
}

/// The Cyborg job's body. `None` without the cyborg's prototype.
pub(crate) fn join(
    world: &mut World,
    client: Entity,
    job: usize,
    latejoin: bool,
) -> Option<Entity> {
    let forced = crate::server::dev_spawn(world, client).or(world.resource::<SpawnPoints>().forced);
    let pos = forced
        .or_else(|| crate::jobs::job_spawn(world, "Cyborg"))
        .unwrap_or_else(|| crate::server::pick_latejoin(world));
    let borg = crate::construction::spawn_obj(world, ROBOT_TYPE, pos)?;
    // tg `get_standard_name`: "[designation] [braintype]-[identifier]".
    let id = world
        .resource_mut::<crate::combat::CombatRng>()
        .range(1, 999);
    let name = format!("Default Cyborg-{id}");
    let card = all_access_card(world, borg);
    let laws = round_default(&world.resource::<Silicons>().0);
    let choices = model_choices(world);
    world.entity_mut(borg).insert((
        Name::new(name.clone()),
        Silicon,
        ss13_tgui::server::RemoteActor,
        Cyborg {
            model: None,
            model_item: None,
            charge: HIGH_CELL_CHARGE,
            max_charge: HIGH_CELL_CHARGE,
            low_power: false,
            connected_ai: None,
            lawupdate: true,
        },
        laws,
        living_bundle(),
        Mover::new(crate::RUN_DELAY_SECS),
        HudData::default(),
        Inventory {
            id: Some(card),
            ..default()
        },
        PlayerJob(job),
        ListInput(choices),
    ));
    world.resource_mut::<Jobs>().add_position(job);
    attach_client(world, client, borg, pos);
    give_radio(world, borg, &[]);
    if let Some(ai) = fewest_borgs_ai(world) {
        connect(world, borg, ai);
    }
    world.write_message(OpenUi::new::<ListInputWindow>(
        client,
        borg,
        borg,
        "Выбор модели",
    ));
    if let Some(laws) = world.get::<Laws>(borg).cloned() {
        show_laws(world, client, &laws);
    }
    notice(world, client, Span::BoldNotice, RADIO_INFO);
    info!(
        "{name} ({client}) joined as Cyborg at {pos:?}{}",
        if latejoin { " (latejoin)" } else { "" }
    );
    Some(borg)
}

/// tg silicons pass `allowed()`: an internal card with every job's access.
fn all_access_card(world: &mut World, borg: Entity) -> Entity {
    let mut access: Vec<String> = world
        .get_resource::<Jobs>()
        .map(|j| {
            j.data
                .jobs
                .iter()
                .flat_map(|job| job.access.clone())
                .collect()
        })
        .unwrap_or_default();
    access.sort();
    access.dedup();
    world
        .spawn((IdCard { access }, CarriedBy(borg), ChildOf(borg)))
        .id()
}

fn model_choices(world: &World) -> ListInputStatic {
    ListInputStatic {
        title: "Выбор модели".into(),
        message: "Выберите модель киборга".into(),
        items: world
            .resource::<Silicons>()
            .0
            .models
            .iter()
            .map(|m| m.name.clone())
            .collect(),
    }
}

/// tg `pick_model` -> `transform_to`: only from the default model.
fn choose_model(world: &mut World, mut acts: Local<MessageCursor<UiActed<ListInputWindow>>>) {
    let acts: Vec<(Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ListInputWindow>>>())
        .map(|a| (a.client, a.source, a.act.clone()))
        .collect();
    for (client, borg, choice) in acts {
        let Some(cyborg) = world.get::<Cyborg>(borg) else {
            continue;
        };
        if cyborg.model.is_some() || world.get::<Stat>(borg) == Some(&Stat::Dead) {
            continue;
        }
        let Some((index, model)) = world
            .resource::<Silicons>()
            .0
            .models
            .iter()
            .enumerate()
            .find(|(_, m)| m.name == choice)
            .map(|(i, m)| (i, m.clone()))
        else {
            continue;
        };
        transform_to(world, borg, index, &model);
        world.entity_mut(borg).remove::<ListInput>();
        notice(
            world,
            client,
            Span::Notice,
            format!("Выбрана модель: {}.", model.name),
        );
    }
}

/// tg `transform_to`: the model's sprite, modules and radio channels.
pub fn transform_to(world: &mut World, borg: Entity, index: usize, model: &RobotModel) {
    let Some(&pos) = world.get::<TilePos>(borg) else {
        return;
    };
    if let Some(id) = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(ROBOTS_DMI, &model.cyborg_base_icon))
        && let Some(mut appearance) = world.get_mut::<Appearance>(borg)
    {
        appearance.icon_state = id;
    }
    let item = crate::construction::spawn_obj(world, &model.path, pos);
    if let Some(item) = item {
        world.entity_mut(item).remove::<TilePos>().insert((
            CarriedBy(borg),
            Storage {
                max_slots: u32::MAX,
                max_specific_storage: u8::MAX,
                max_total_storage: u32::MAX,
            },
        ));
        for path in &model.basic_modules {
            if let Some(module) = crate::construction::spawn_obj(world, path, pos) {
                world.entity_mut(module).insert(RobotModule);
                crate::storage::put_inside(world, module, item);
            }
        }
        if let Some(mut inventory) = world.get_mut::<Inventory>(borg) {
            inventory.back = Some(item);
        }
    }
    if let Some(&SiliconRadio(radio)) = world.get::<SiliconRadio>(borg)
        && let Some(mut radio) = world.get_mut::<Radio>(radio)
    {
        radio.channels = model
            .radio_channels
            .iter()
            .map(|c| (c.clone(), true))
            .collect();
    }
    if let Some(mut cyborg) = world.get_mut::<Cyborg>(borg) {
        cyborg.model = Some(index);
        cyborg.model_item = item;
    }
}

/// tg `uneq_active` / `drop_all_held_items` for a cyborg: held modules go back into the model.
pub fn stow_held(world: &mut World, borg: Entity, only_active: bool) {
    let Some(model) = world.get::<Cyborg>(borg).and_then(|c| c.model_item) else {
        return;
    };
    let Some(inventory) = world.get::<Inventory>(borg).cloned() else {
        return;
    };
    let hands: Vec<usize> = if only_active {
        vec![inventory.active]
    } else {
        vec![0, 1]
    };
    for hand in hands {
        let Some(item) = inventory.hands[hand] else {
            continue;
        };
        if let Some(mut inv) = world.get_mut::<Inventory>(borg) {
            inv.hands[hand] = None;
        }
        crate::storage::put_inside(world, item, model);
    }
}

/// What a cyborg's inventory action does instead of a human's (`crate::inventory`). `true`:
/// handled (nothing else happens).
pub fn inventory_action(
    world: &mut World,
    client: Entity,
    borg: Entity,
    action: CyborgAction,
) -> bool {
    if world.get::<Cyborg>(borg).is_none() {
        return false;
    }
    let low_power = world.get::<Cyborg>(borg).is_some_and(|c| c.low_power);
    match action {
        CyborgAction::Drop => stow_held(world, borg, true),
        CyborgAction::Back => {
            let model = world.get::<Inventory>(borg).and_then(|i| i.back);
            if let Some(model) = model {
                let held = world
                    .get::<Inventory>(borg)
                    .and_then(Inventory::active_item);
                match held {
                    Some(_) => stow_held(world, borg, true),
                    // tg `activate_module` needs charge.
                    None if !low_power => crate::storage::click_storage(world, client, borg, model),
                    None => {}
                }
            }
        }
        CyborgAction::OtherSlot | CyborgAction::EmptyHandClick => {}
    }
    true
}

/// The inventory actions a cyborg handles itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CyborgAction {
    /// Q: the active module back into the model.
    Drop,
    /// The back slot: the model's storage.
    Back,
    /// Any other equipment slot: a cyborg has none.
    OtherSlot,
    /// An empty-handed click on something: a cyborg picks nothing up (tg `attack_robot`).
    EmptyHandClick,
}

/// tg `handle_robot_cell` -> `use_energy`, per second of the tick.
fn cell_drain(
    time: Res<Time>,
    mut borgs: Query<(Entity, &mut Cyborg, &Stat)>,
    mut stow: Local<Vec<Entity>>,
    mut commands: Commands,
) {
    let dt = time.delta_secs_f64();
    for (borg, mut cyborg, stat) in &mut borgs {
        if *stat == Stat::Dead {
            continue;
        }
        if cyborg.low_power {
            if cyborg.charge > 0.0 {
                cyborg.low_power = false;
            }
            continue;
        }
        if *stat != Stat::Conscious {
            continue;
        }
        if cyborg.charge > 0.0 {
            if cyborg.charge <= 0.01 * STANDARD_CELL_CHARGE {
                stow.push(borg);
            }
            cyborg.charge = (cyborg.charge - BORG_MINIMUM_POWER_CONSUMPTION * dt).max(0.0);
        } else {
            stow.push(borg);
            cyborg.low_power = true;
        }
    }
    for borg in stow.drain(..) {
        commands.queue(move |world: &mut World| stow_held(world, borg, false));
    }
}

fn init_stations(world: &mut World) {
    let mut q = world.query::<(Entity, &DmObj)>();
    let stations: Vec<Entity> = q
        .iter(world)
        .filter(|(_, o)| crate::obj_damage::is_under(&o.path, STATION))
        .map(|(e, _)| e)
        .collect();
    for e in stations {
        setup_station(world, e);
    }
}

/// A recharging station's state (map load and `crate::machine_frame`).
pub(crate) fn setup_station(world: &mut World, e: Entity) {
    world.entity_mut(e).insert(RechargeStation {
        open: true,
        occupant: None,
    });
}

/// tg `recharge_station/interact` -> `toggle_open` on an empty-handed click; `close_machine`
/// takes in a cyborg standing on it.
fn station_clicks(world: &mut World, mut clicks: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(station) = world.get::<RechargeStation>(click.target).copied() else {
            continue;
        };
        if world
            .get::<Inventory>(click.mob)
            .is_some_and(|i| i.active_item().is_some())
        {
            continue;
        }
        let new = if station.open {
            let occupant = world
                .resource::<ss13_world::SpatialIndex>()
                .on_tile(click.target_pos)
                .iter()
                .copied()
                .find(|&e| world.get::<Cyborg>(e).is_some());
            RechargeStation {
                open: false,
                occupant,
            }
        } else {
            RechargeStation {
                open: true,
                occupant: None,
            }
        };
        let mut e = world.entity_mut(click.target);
        e.insert(new);
        if new.open {
            e.remove::<ss13_world::Density>();
        } else {
            e.insert(ss13_world::Density);
        }
    }
}

/// tg `recharge_station/process` -> `robot/charge`: the cell fills by `recharge_speed`; an
/// occupant who left opens the station (`relaymove`).
fn charge_occupants(
    time: Res<Time>,
    mut stations: Query<(&mut RechargeStation, &TilePos, Has<Unpowered>)>,
    mut borgs: Query<(&mut Cyborg, &TilePos)>,
) {
    let dt = time.delta_secs_f64();
    for (mut station, pos, unpowered) in &mut stations {
        let Some(occupant) = station.occupant else {
            continue;
        };
        let Ok((mut cyborg, borg_pos)) = borgs.get_mut(occupant) else {
            station.occupant = None;
            continue;
        };
        if borg_pos != pos {
            *station = RechargeStation {
                open: true,
                occupant: None,
            };
            continue;
        }
        if unpowered {
            continue;
        }
        cyborg.charge = (cyborg.charge + RECHARGE_SPEED * dt).min(cyborg.max_charge);
    }
}

/// tg `recharge_station/update_icon_state` and its density.
fn station_looks(
    mut stations: Query<
        (Entity, &RechargeStation, &mut Appearance, Has<Unpowered>),
        Or<(Changed<RechargeStation>, Added<Unpowered>)>,
    >,
    icons: Option<Res<IconIndex>>,
    mut commands: Commands,
) {
    let Some(icons) = icons else {
        return;
    };
    for (e, station, mut appearance, unpowered) in &mut stations {
        let state = if unpowered {
            format!("borgcharger-u{}", u8::from(!station.open))
        } else if station.open {
            "borgcharger0".into()
        } else if station.occupant.is_some() {
            "borgcharger1".into()
        } else {
            "borgcharger2".into()
        };
        if let Some(id) = icons.get(STATION_DMI, &state) {
            appearance.icon_state = id;
        }
        if station.open {
            commands.entity(e).remove::<ss13_world::Density>();
        }
    }
}

/// Laws follow the AI (tg `lawsync` whenever the AI's laws change).
fn sync_laws(world: &mut World, mut changed: Local<Vec<Entity>>) {
    let mut ais = world.query_filtered::<&AiCore, Changed<Laws>>();
    changed.extend(ais.iter(world).flat_map(|c| c.connected_robots.clone()));
    for borg in std::mem::take(&mut *changed) {
        lawsync(world, borg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deleted_borgs_and_ais_unlink() {
        let mut world = World::new();
        world.add_observer(forget_borg);
        world.add_observer(forget_ai);
        let ai = world
            .spawn(AiCore {
                eye: Entity::PLACEHOLDER,
                control_disabled: false,
                connected_robots: Vec::new(),
            })
            .id();
        let borg = |world: &mut World| {
            let b = world
                .spawn(Cyborg {
                    model: None,
                    model_item: None,
                    charge: 0.0,
                    max_charge: 0.0,
                    low_power: false,
                    connected_ai: Some(ai),
                    lawupdate: true,
                })
                .id();
            world
                .get_mut::<AiCore>(ai)
                .unwrap()
                .connected_robots
                .push(b);
            b
        };
        let (gone, kept) = (borg(&mut world), borg(&mut world));
        world.despawn(gone);
        assert_eq!(world.get::<AiCore>(ai).unwrap().connected_robots, [kept]);
        world.despawn(ai);
        assert_eq!(world.get::<Cyborg>(kept).unwrap().connected_ai, None);
    }
}
