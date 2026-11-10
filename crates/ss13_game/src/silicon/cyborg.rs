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
//!   storages. Three modules can be out at once ([`ModuleSlots`]); swap (X) cycles them
//!   (`cycle_modules`). Clicking the back slot before a model is chosen asks again.
//! - Cutting the reset-model wire resets the cyborg ([`reset_model`], `ResetModel`): modules
//!   back, what its bags hold on the floor, boards dropped, the default model and the choice
//!   again; a model is refused while that wire is cut or the cyborg is locked down.
//! - The cell (tg `handle_robot_cell`): every second `BORG_MINIMUM_POWER_CONSUMPTION` while
//!   conscious; at 1% of a standard cell the modules drop back into the model, at 0 the cyborg
//!   is in low power mode (no modules) until charged. Silicon access opens every door
//!   (`allowed()` is TRUE for silicons): an internal ID with every job's access.
//! - The recharging station: a click with an empty hand opens or closes it (tg `toggle_open`);
//!   closing takes the cyborg on its tile in, and while closed and powered it charges the cell
//!   by tg's `recharge_speed` (`5e-3 * capacitor tiers * cell maxcharge`; a mapped one has two
//!   tier 1 capacitors and a high-capacity cell: 1 kW). Walking out
//!   opens it (tg `relaymove`).
//! - A cyborg built around an MMI or a posibrain (`super::mmi`) starts the same way
//!   ([`spawn_borg`], [`activate`]).
// shortcut: no cyborg HUD (the human slots are shown; only the hands and the back slot work;
// the third module slot shares the right hand's cell, see `ModuleSlots`), no broken slots
// (`break_cyborg_slot`), no lamp, ion thrusters, model transform animation, skins, tipping,
// model traits or actions (`borg_meson`, auto-wash); the syndicate models have no thermal
// vision, chameleon projector, syndicate access or factions; consumables but the flashes are
// not respawned at the station (nor their 0.5% of the charge drawn, which tg takes only when
// something was restocked).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::silicon::{ROBOT_SYNDICATE_CELL, ROBOT_TYPE, RobotModel};
use ss13_mapload::{DmObj, IconIndex};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::server::{OpenUi, UiActed, UiSystems};
use ss13_world::Mover;

use super::ai::AiCore;
use super::laws::{Laws, round_default};
use super::ui::ListInputStatic;
use super::{ListInput, ListInputWindow, Silicon, Silicons, attach_client, give_radio, notice};
use crate::click::{ClickResolved, ResolveClicks};
use crate::health::{HealthSystems, Stat, living_bundle};
use crate::hud::HudData;
use crate::inventory::{CarriedBy, IdCard, Inventory};
use crate::jobs::{Jobs, PlayerJob};
use crate::power::Unpowered;
use crate::radio::{Radio, SiliconRadio};
use crate::server::SpawnPoints;
use crate::storage::{Storage, Stored, StoredIn};

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
/// `/obj/item/stock_parts/power_store/cell/hyper` `maxcharge`.
pub const HYPER_CELL_CHARGE: f64 = STANDARD_CELL_CHARGE * 30.0;
/// tg `/obj/item/robot_model/syndicate`, `syndicate_medical`, `saboteur`: what the operatives'
/// beacons build, with the mob's `playstyle_string`; `ResetModel` does nothing for them.
pub(crate) const SYNDICATE_MODELS: [(&str, &str); 3] = [
    (
        "/obj/item/robot_model/syndicate",
        "You are a Syndicate assault cyborg! You are armed with powerful offensive tools to aid \
         you in your mission: help the operatives secure the nuclear authentication disk. Your \
         cyborg LMG will slowly produce ammunition from your power supply, and your operative \
         pinpointer will find and locate fellow nuclear operatives. Help the operatives secure \
         the disk at all costs!",
    ),
    (
        "/obj/item/robot_model/syndicate_medical",
        "You are a Syndicate medical cyborg! You are armed with powerful medical tools to aid \
         you in your mission: help the operatives secure the nuclear authentication disk. Your \
         hypospray will produce Restorative Nanites, a wonder-drug that will heal most types of \
         bodily damages, including brain damage. It also produces morphine for offense. Your \
         defibrillator paddles can revive operatives through their suits, or can be used on \
         harm intent to shock enemies! Your energy saw functions as a circular saw, but can be \
         activated to deal more damage, and your operative pinpointer will find and locate \
         fellow nuclear operatives. Help the operatives secure the disk at all costs!",
    ),
    (
        "/obj/item/robot_model/saboteur",
        "You are a Syndicate saboteur cyborg! You are armed with robust engineering tools to \
         aid you in your mission: help the operatives secure the nuclear authentication disk. \
         Your destination tagger will allow you to stealthily traverse the disposal network \
         across the station. Your welder will allow you to repair the operatives' exosuits, but \
         also yourself and your fellow cyborgs. Your cyborg chameleon projector allows you to \
         assume the appearance and registered name of a Nanotrasen engineering borg, and \
         undertake covert actions on the station. Be aware that almost any physical contact or \
         incidental damage will break your camouflage. Help the operatives secure the disk at \
         all costs!",
    ),
];
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

/// tg `held_items` has three cyborg slots, [`Inventory::hands`] two. Slot 1 is the left hand;
/// slots 2 and 3 are the right hand and `spare`, swapped by [`Self::third_shown`] when slot 3
/// is selected, so everything that reads the active hand just works.
// shortcut: slots 2 and 3 share the right hand's HUD cell and slot 3 is reached by cycling
// only, until a cyborg HUD exists.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModuleSlots {
    pub spare: Option<Entity>,
    pub third_shown: bool,
}

/// tg `/obj/machinery/recharge_station`: `state_open` and its occupant.
#[derive(Component, Clone, Copy, Debug)]
pub struct RechargeStation {
    pub open: bool,
    pub occupant: Option<Entity>,
}

pub struct CyborgPlugin;

impl Plugin for CyborgPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `attack_robot`: a cyborg's empty hand picks nothing up.
        app.add_click_handler(ClickPhase::EmptyHand, 200, |w, c| {
            crate::silicon::cyborg::inventory_action(
                w,
                c.client,
                c.mob,
                crate::silicon::cyborg::CyborgAction::EmptyHandClick,
            )
        });
        app.add_systems(ss13_world::MapInit, init_stations)
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
            .add_observer(forget_ai)
            .add_observer(forget_module);
    }
}

/// A module deleted out of the spare slot (boards remove modules, `SetEmagged(FALSE)`).
fn forget_module(remove: On<Remove<RobotModule>>, mut slots: Query<&mut ModuleSlots>) {
    for mut slots in &mut slots {
        if slots.spare == Some(remove.entity) {
            slots.spare = None;
        }
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
pub(crate) fn fewest_borgs_ai(world: &mut World) -> Option<Entity> {
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

/// tg `lawsync`: a linked cyborg with `lawupdate` takes its AI's laws. True when they changed.
pub(crate) fn lawsync(world: &mut World, borg: Entity) -> bool {
    let Some(cyborg) = world.get::<Cyborg>(borg) else {
        return false;
    };
    let Some(ai) = cyborg.connected_ai.filter(|_| cyborg.lawupdate) else {
        return false;
    };
    let Some(laws) = world.get::<Laws>(ai).map(Laws::for_cyborg) else {
        return false;
    };
    if world.get::<Laws>(borg) == Some(&laws) {
        return false;
    }
    world.entity_mut(borg).insert(laws);
    true
}

/// tg `set_connected_ai(null)`: the cyborg leaves its AI's `connected_robots`.
pub(crate) fn disconnect(world: &mut World, borg: Entity) {
    let Some(ai) = world
        .get_mut::<Cyborg>(borg)
        .and_then(|mut c| c.connected_ai.take())
    else {
        return;
    };
    if let Some(mut core) = world.get_mut::<AiCore>(ai) {
        core.connected_robots.retain(|&b| b != borg);
    }
}

/// Cyborgs without an AI link to `ai` (tg `/datum/job/ai/after_spawn` -> `try_connect_to_ai`:
/// linked, synced, `lawupdate` on). Emagged and scrambled ones stay free.
pub(crate) fn link_orphans(world: &mut World, ai: Entity) {
    let mut borgs = world.query::<(Entity, &Cyborg, Option<&super::chassis::Chassis>)>();
    let orphans: Vec<Entity> = borgs
        .iter(world)
        .filter(|(_, c, chassis)| {
            c.connected_ai.is_none() && !chassis.is_some_and(|ch| ch.emagged || ch.scrambledcodes)
        })
        .map(|(e, ..)| e)
        .collect();
    for borg in orphans {
        if let Some(mut cyborg) = world.get_mut::<Cyborg>(borg) {
            cyborg.lawupdate = true;
        }
        connect(world, borg, ai);
        super::chassis::show_laws(world, borg);
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
    let borg = spawn_borg(world, pos, "Cyborg")?;
    world.entity_mut(borg).insert(PlayerJob(job));
    world.resource_mut::<Jobs>().add_position(job);
    activate(world, borg, client, pos);
    info!(
        "{} ({client}) joined as Cyborg at {pos:?}{}",
        super::chassis::name_of(world, borg),
        if latejoin { " (latejoin)" } else { "" }
    );
    Some(borg)
}

/// tg `/mob/living/silicon/robot/Initialize` on `pos`, without a player: the default model,
/// the round's laws, a high-capacity cell. `braintype` names it (tg `get_standard_name`:
/// "[designation] [braintype]-[identifier]"). `None` without the cyborg's prototype.
pub(crate) fn spawn_borg(world: &mut World, pos: TilePos, braintype: &str) -> Option<Entity> {
    let borg = crate::construction::spawn_obj(world, ROBOT_TYPE, pos)?;
    let id = world
        .resource_mut::<crate::combat::CombatRng>()
        .range(1, 999);
    let name = format!("Default {braintype}-{id}");
    let card = all_access_card(world, borg);
    let laws = round_default(&world.resource::<Silicons>().0);
    let choices = model_choices(world);
    world.entity_mut(borg).insert((
        Name::new(name),
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
        ModuleSlots::default(),
        ListInput(choices),
    ));
    world.entity_mut(borg).insert((
        super::alarms::AlertListener::silicon(false),
        super::alarms::AlarmQueue::default(),
    ));
    super::chassis::setup(world, borg);
    Some(borg)
}

/// `client` takes a new cyborg (the job's, or one built around its MMI): its radio and buttons,
/// the AI with the fewest cyborgs (tg `Initialize` without an AI: no lawsync, the first law
/// rack), the model choice and the laws.
pub(crate) fn activate(world: &mut World, borg: Entity, client: Entity, pos: TilePos) {
    attach_client(world, client, borg, pos);
    give_radio(world, borg, &[]);
    super::ai_hud::give_borg_buttons(world, borg);
    match fewest_borgs_ai(world) {
        Some(ai) => connect(world, borg, ai),
        None => {
            if let Some(mut cyborg) = world.get_mut::<Cyborg>(borg) {
                cyborg.lawupdate = false;
            }
            super::law_rack::link_to_first_rack(world, borg);
        }
    }
    open_model_choice(world, client, borg);
    super::chassis::show_laws(world, borg);
    notice(world, client, Span::BoldNotice, RADIO_INFO);
}

/// `/mob/living/silicon/robot/model/syndicate/Initialize` with `client` in it: no AI, no law
/// sync, SyndOS, a hyper cell, a syndicate radio and the beacon's model from the start.
/// `None` without the prototype or the model.
pub(crate) fn spawn_syndicate(
    world: &mut World,
    client: Entity,
    pos: TilePos,
    model_path: &str,
) -> Option<Entity> {
    let (index, model) = world
        .resource::<Silicons>()
        .0
        .models
        .iter()
        .enumerate()
        .find(|(_, m)| m.path == model_path)
        .map(|(i, m)| (i, m.clone()))?;
    let playstyle = SYNDICATE_MODELS
        .iter()
        .find(|(p, _)| *p == model_path)
        .map(|&(_, text)| text);
    let borg = spawn_borg(world, pos, "Cyborg")?;
    attach_client(world, client, borg, pos);
    give_radio(world, borg, &[]);
    super::ai_hud::give_borg_buttons(world, borg);
    transform_to(world, borg, index, &model);
    world.entity_mut(borg).remove::<ListInput>();
    if let Some(&SiliconRadio(radio)) = world.get::<SiliconRadio>(borg)
        && let Some(mut radio) = world.get_mut::<Radio>(radio)
    {
        // `/obj/item/radio/borg/syndicate`: `syndie`, a syndicate encryption key.
        radio.frequency = ss13_chat::radio::FREQ_SYNDICATE;
        radio.special_channels |= ss13_chat::radio::RADIO_SPECIAL_SYNDIE;
        radio
            .channels
            .push((ss13_chat::radio::RADIO_CHANNEL_SYNDICATE.to_owned(), true));
    }
    let old_cell = world
        .get::<super::chassis::Chassis>(borg)
        .and_then(|c| c.cell);
    if let Some(cell) = old_cell {
        world.entity_mut(cell).despawn();
    }
    let cell = crate::mecha::core::spawn_inside(world, ROBOT_SYNDICATE_CELL, borg);
    if let Some(mut chassis) = world.get_mut::<super::chassis::Chassis>(borg) {
        chassis.cell = cell;
        chassis.scrambledcodes = true;
    }
    if let Some(mut cyborg) = world.get_mut::<Cyborg>(borg) {
        cyborg.lawupdate = false;
        cyborg.charge = HYPER_CELL_CHARGE;
        cyborg.max_charge = HYPER_CELL_CHARGE;
    }
    let laws = world
        .resource::<Silicons>()
        .0
        .lawset(super::chassis::SYNDICATE_LAWS)
        .map(Laws::from_lawset);
    if let Some(laws) = laws {
        world.entity_mut(borg).insert(laws);
    }
    super::chassis::show_laws(world, borg);
    if let Some(text) = playstyle {
        notice(world, client, Span::BoldNotice, text);
    }
    notice(world, client, Span::BoldNotice, RADIO_INFO);
    Some(borg)
}

/// tg `pick_model`'s refusals, told to `client`. True when the model may not be picked now.
fn pick_refused(world: &mut World, client: Entity, borg: Entity) -> bool {
    use crate::device_wires::WIRE_RESET_MODEL;
    if world
        .get::<crate::wires::Wires>(borg)
        .is_some_and(|w| w.is_cut(WIRE_RESET_MODEL))
    {
        notice(
            world,
            client,
            Span::BoldWarning,
            "ERROR: Model installer reply timeout. Please check internal connections.",
        );
        return true;
    }
    if world
        .get::<super::chassis::Chassis>(borg)
        .is_some_and(|c| c.lockcharge)
    {
        notice(
            world,
            client,
            Span::BoldWarning,
            "ERROR: Lockdown is engaged. Please disengage lockdown to pick module.",
        );
        return true;
    }
    false
}

fn open_model_choice(world: &mut World, client: Entity, borg: Entity) {
    world.write_message(OpenUi::new::<ListInputWindow>(
        client,
        borg,
        borg,
        "Выбор модели",
    ));
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
            .filter(|m| !m.hidden)
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
        if cyborg.model.is_some()
            || world.get::<Stat>(borg) == Some(&Stat::Dead)
            || pick_refused(world, client, borg)
        {
            continue;
        }
        let Some((index, model)) = world
            .resource::<Silicons>()
            .0
            .models
            .iter()
            .enumerate()
            .find(|(_, m)| !m.hidden && m.name == choice)
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

/// tg `transform_to`: the model's sprite, modules (and `emag_modules` when emagged) and radio
/// channels; the old model and its modules go.
pub fn transform_to(world: &mut World, borg: Entity, index: usize, model: &RobotModel) {
    let Some(&pos) = world.get::<TilePos>(borg) else {
        return;
    };
    let old_name = designation(world, borg);
    remove_model_item(world, borg);
    set_designation(world, borg, &old_name, &model.name);
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
    if world
        .get::<super::chassis::Chassis>(borg)
        .is_some_and(|c| c.emagged)
    {
        super::chassis::add_emag_modules(world, borg);
    }
}

/// `drop_all_held_items`, then `qdel(src)` with its modules.
fn remove_model_item(world: &mut World, borg: Entity) {
    let Some(old) = world.get::<Cyborg>(borg).and_then(|c| c.model_item) else {
        return;
    };
    stow_held(world, borg, false);
    if let Some(mut inventory) = world.get_mut::<Inventory>(borg)
        && inventory.back == Some(old)
    {
        inventory.back = None;
    }
    world.entity_mut(old).despawn();
}

/// tg `designation`: the current model's name, "Default" before one is chosen.
fn designation(world: &World, borg: Entity) -> String {
    world
        .get::<Cyborg>(borg)
        .and_then(|c| c.model)
        .and_then(|i| world.resource::<Silicons>().0.models.get(i))
        .map_or_else(|| "Default".to_owned(), |m| m.name.clone())
}

/// tg `update_module_innate` -> `updatename`: the new designation replaces the old one in a
/// standard name ("[designation] [braintype]-[identifier]"); another name stays.
fn set_designation(world: &mut World, borg: Entity, old: &str, new: &str) {
    let Some(rest) = world
        .get::<Name>(borg)
        .and_then(|n| n.as_str().strip_prefix(old).map(str::to_owned))
    else {
        return;
    };
    world
        .entity_mut(borg)
        .insert(Name::new(format!("{new}{rest}")));
}

/// tg `ResetModel`: the model's modules go, what its bags hold drops, upgrade boards fall out,
/// and the cyborg is back on the default model, picking again. False when there is nothing to
/// reset or the cyborg is a syndicate one (`/model/syndicate/ResetModel` does nothing).
pub fn reset_model(world: &mut World, borg: Entity) -> bool {
    let Some(&pos) = world.get::<TilePos>(borg) else {
        return false;
    };
    let Some(item) = world.get::<Cyborg>(borg).and_then(|c| c.model_item) else {
        return false;
    };
    let path = world
        .get::<Cyborg>(borg)
        .and_then(|c| c.model)
        .and_then(|i| world.resource::<Silicons>().0.models.get(i))
        .map(|m| m.path.clone())
        .unwrap_or_default();
    if SYNDICATE_MODELS.iter().any(|(p, _)| *p == path) {
        return false;
    }
    stow_held(world, borg, false);
    // `for(var/obj/item/storage/bag in model.contents)`: the bags' contents on the floor.
    let modules: Vec<Entity> = world
        .get::<Stored>(item)
        .map(|s| s.items().to_vec())
        .unwrap_or_default();
    for module in modules {
        let held: Vec<Entity> = world
            .get::<Stored>(module)
            .map(|s| s.items().to_vec())
            .unwrap_or_default();
        for inner in held {
            crate::storage::take_out(world, inner, pos);
        }
    }
    let old_name = designation(world, borg);
    remove_model_item(world, borg);
    set_designation(world, borg, &old_name, "Default");
    if let Some(id) = world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(ROBOTS_DMI, "robot"))
        && let Some(mut appearance) = world.get_mut::<Appearance>(borg)
    {
        appearance.icon_state = id;
    }
    if let Some(&SiliconRadio(radio)) = world.get::<SiliconRadio>(borg)
        && let Some(mut radio) = world.get_mut::<Radio>(radio)
    {
        radio.channels.clear();
    }
    let boards = world
        .get_mut::<super::chassis::Chassis>(borg)
        .map(|mut c| std::mem::take(&mut c.upgrades))
        .unwrap_or_default();
    for board in boards {
        // Installed one-use boards are spent; the others sit inside the cyborg.
        if world.get_entity(board).is_ok() {
            crate::storage::take_out(world, board, pos);
        }
    }
    if let Some(mut cyborg) = world.get_mut::<Cyborg>(borg) {
        cyborg.model = None;
        cyborg.model_item = None;
    }
    let choices = model_choices(world);
    world.entity_mut(borg).insert(ListInput(choices));
    if let Some(client) = world
        .get::<crate::server::PlayerMob>(borg)
        .map(|p| p.client)
    {
        open_model_choice(world, client, borg);
    }
    true
}

/// A cyborg's slot items in slot order: the left hand, then the right hand and the spare slot.
fn slot_items(inventory: &Inventory, slots: ModuleSlots) -> [Option<Entity>; 3] {
    if slots.third_shown {
        [inventory.hands[0], slots.spare, inventory.hands[1]]
    } else {
        [inventory.hands[0], inventory.hands[1], slots.spare]
    }
}

fn has_free_slot(world: &World, borg: Entity) -> bool {
    match (world.get::<Inventory>(borg), world.get::<ModuleSlots>(borg)) {
        (Some(inventory), Some(&slots)) => slot_items(inventory, slots).contains(&None),
        _ => false,
    }
}

/// tg `activate_module`: the module goes to the first free slot of three. False when it is
/// already out or there is no free slot.
pub fn activate_module(world: &mut World, borg: Entity, module: Entity) -> bool {
    let (Some(inventory), Some(&slots)) =
        (world.get::<Inventory>(borg), world.get::<ModuleSlots>(borg))
    else {
        return false;
    };
    let items = slot_items(inventory, slots);
    if items.contains(&Some(module)) {
        super::chassis::tell_mob(
            world,
            borg,
            Span::Warning,
            "That module is already activated.",
        );
        return false;
    }
    let Some(free) = items.iter().position(Option::is_none) else {
        super::chassis::tell_mob(world, borg, Span::Warning, "Deactivate a module first!");
        return false;
    };
    world
        .entity_mut(module)
        .remove::<(StoredIn, TilePos)>()
        .insert(CarriedBy(borg));
    // `free` is a slot number; where that slot physically is depends on `third_shown`.
    let in_right_hand = free == if slots.third_shown { 2 } else { 1 };
    if free == 0 || in_right_hand {
        let hand = usize::from(in_right_hand);
        world.get_mut::<Inventory>(borg).expect("checked").hands[hand] = Some(module);
    } else {
        world.get_mut::<ModuleSlots>(borg).expect("checked").spare = Some(module);
    }
    true
}

/// tg `cycle_modules` (swap): the next slot with a module out; none: no change.
pub fn cycle_modules(world: &mut World, borg: Entity) {
    let (Some(inventory), Some(&slots)) =
        (world.get::<Inventory>(borg), world.get::<ModuleSlots>(borg))
    else {
        return;
    };
    let current = match (inventory.active, slots.third_shown) {
        (0, _) => 0,
        (_, false) => 1,
        (_, true) => 2,
    };
    let items = slot_items(inventory, slots);
    let Some(next) = (1..=2)
        .map(|step| (current + step) % 3)
        .find(|&slot| items[slot].is_some())
    else {
        return;
    };
    let right = inventory.hands[1];
    let spare = slots.spare;
    if next != 0 && (next == 2) != slots.third_shown {
        world.get_mut::<Inventory>(borg).expect("checked").hands[1] = spare;
        let mut slots = world.get_mut::<ModuleSlots>(borg).expect("checked");
        slots.spare = right;
        slots.third_shown = next == 2;
    }
    world.get_mut::<Inventory>(borg).expect("checked").active = usize::from(next != 0);
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
    if only_active {
        return;
    }
    let spare = world.get_mut::<ModuleSlots>(borg).and_then(|mut slots| {
        slots.third_shown = false;
        slots.spare.take()
    });
    if let Some(item) = spare {
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
                    Some(_) if !has_free_slot(world, borg) => stow_held(world, borg, true),
                    // tg `activate_module` needs charge. The module list opens with a module
                    // out too (a held one would go back with `click_storage`).
                    _ if !low_power => crate::storage::open_view(world, client, borg, model),
                    _ => {}
                }
            } else if world.get::<ListInput>(borg).is_some() && !pick_refused(world, client, borg) {
                // tg's module button (`pick_model`) before a model is chosen.
                open_model_choice(world, client, borg);
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
    let mut q = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
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

/// tg `recharge_station/RefreshParts`: `5e-3 * tier` per capacitor, times each cell's
/// `maxcharge`, watts.
pub fn recharge_speed(parts: &crate::stock_parts::PartRatings) -> f64 {
    let caps: f64 = parts
        .tiers(crate::stock_parts::PartKind::Capacitor)
        .map(|t| 5e-3 * f64::from(t))
        .sum();
    parts.cells.iter().fold(caps, |speed, max| speed * max)
}

/// tg `recharge_station/process` -> `robot/charge`: the cell fills by `recharge_speed`; an
/// occupant who left opens the station (`relaymove`).
fn charge_occupants(
    time: Res<Time>,
    mut stations: Query<(
        &mut RechargeStation,
        &TilePos,
        Has<Unpowered>,
        Option<&crate::stock_parts::PartRatings>,
    )>,
    mut borgs: Query<(&mut Cyborg, &TilePos)>,
    mut commands: Commands,
) {
    let dt = time.delta_secs_f64();
    for (mut station, pos, unpowered, parts) in &mut stations {
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
        // A mapped station without refreshed parts has its board's defaults.
        let speed = parts.map_or(RECHARGE_SPEED, recharge_speed);
        cyborg.charge = (cyborg.charge + speed * dt).min(cyborg.max_charge);
        // `respawn_consumable`: the flashes are whole again.
        commands.queue(move |world: &mut World| {
            crate::security::restock_flashes(world, occupant);
        });
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

/// Laws follow the AI (tg `lawsync` whenever the AI's laws change), and the cyborg is shown
/// them (`post_lawchange` -> `show_laws`).
fn sync_laws(world: &mut World, mut changed: Local<Vec<Entity>>) {
    let mut ais = world.query_filtered::<&AiCore, Changed<Laws>>();
    changed.extend(ais.iter(world).flat_map(|c| c.connected_robots.clone()));
    for borg in std::mem::take(&mut *changed) {
        if lawsync(world, borg) {
            super::chassis::show_laws(world, borg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_borg(world: &mut World) -> Entity {
        world
            .spawn((
                Cyborg {
                    model: None,
                    model_item: None,
                    charge: 0.0,
                    max_charge: 0.0,
                    low_power: false,
                    connected_ai: None,
                    lawupdate: true,
                },
                Inventory::default(),
                ModuleSlots::default(),
            ))
            .id()
    }

    #[test]
    fn three_module_slots_fill_in_order_and_cycle() {
        let mut world = World::new();
        let borg = test_borg(&mut world);
        let modules: [Entity; 4] = std::array::from_fn(|_| world.spawn(RobotModule).id());
        let [one, two, three, four] = modules;
        for module in [one, two, three] {
            assert!(activate_module(&mut world, borg, module));
        }
        // tg `activate_module`: the first free slot; none left, the module stays stored.
        assert!(!activate_module(&mut world, borg, four));
        assert!(!activate_module(&mut world, borg, one), "already out");
        assert_eq!(
            world.get::<Inventory>(borg).unwrap().hands,
            [Some(one), Some(two)]
        );
        assert_eq!(world.get::<ModuleSlots>(borg).unwrap().spare, Some(three));

        let active = |w: &World| w.get::<Inventory>(borg).unwrap().active_item();
        assert_eq!(active(&world), Some(one));
        cycle_modules(&mut world, borg);
        assert_eq!(active(&world), Some(two));
        cycle_modules(&mut world, borg);
        assert_eq!(active(&world), Some(three), "the third slot is reachable");
        assert_eq!(world.get::<ModuleSlots>(borg).unwrap().spare, Some(two));
        cycle_modules(&mut world, borg);
        assert_eq!(active(&world), Some(one));
    }

    #[test]
    fn cycling_skips_empty_slots_and_a_deleted_spare_is_forgotten() {
        let mut world = World::new();
        world.add_observer(forget_module);
        let borg = test_borg(&mut world);
        let one = world.spawn(RobotModule).id();
        let three = world.spawn(RobotModule).id();
        {
            let mut inventory = world.get_mut::<Inventory>(borg).unwrap();
            inventory.hands[0] = Some(one);
        }
        world.get_mut::<ModuleSlots>(borg).unwrap().spare = Some(three);
        cycle_modules(&mut world, borg);
        assert_eq!(
            world.get::<Inventory>(borg).unwrap().active_item(),
            Some(three),
            "slot 2 is empty"
        );
        // Now slot 3 is in the right hand and the spare slot is slot 2's.
        let two = world.spawn(RobotModule).id();
        world.get_mut::<ModuleSlots>(borg).unwrap().spare = Some(two);
        world.despawn(two);
        assert_eq!(world.get::<ModuleSlots>(borg).unwrap().spare, None);
    }

    #[test]
    fn the_choice_list_hides_the_other_models() {
        let mut world = World::new();
        let model = |name: &str, hidden| RobotModel {
            name: name.into(),
            hidden,
            ..default()
        };
        world.insert_resource(Silicons(ss13_mapload::silicon::SiliconData {
            models: vec![model("Engineering", false), model("Security", true)],
            ..default()
        }));
        assert_eq!(model_choices(&world).items, ["Engineering"]);
    }

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
