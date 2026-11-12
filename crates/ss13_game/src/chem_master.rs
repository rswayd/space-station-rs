//! ChemMaster and CondiMaster (server): tg `/obj/machinery/chem_master`
//! (`reagents/chemistry/machinery/chem_master.dm`) and its `ChemMaster` window.
//!
//! - An open container clicked on it goes in (the old one to the user's hands) and opens the
//!   window; a right click with an empty hand takes it out. The buffer holds the volume of the
//!   beakers among its parts (`RefreshParts`; 100 without any), each servo tier cuts 0.25 s off
//!   the 1 s printing step (0.25 s at least).
//! - `transfer` moves one reagent between the beaker and the buffer (1, 5, 10, the custom
//!   amount, all, or an amount asked with `tgui_input_number`); from the buffer in "destroying"
//!   mode it is deleted instead. Every transfer uses `active_power_usage`; nothing moves into a
//!   holder that is reacting.
//! - `create` asks for a name (tubes and bottles default to "<main reagent> <container>", closed
//!   containers add the volume) and prints up to 50 of the selected container, one per printing
//!   step, each with an equal share of the buffer and pills with the chosen sugar layers.
//!   Printing locks the buffer and the beaker; losing power or `stopPrinting` ends it.
//! - Every act is checked at the moment of effect: power, the beaker still inside, the reagent
//!   type, the amounts, the container among the printable ones; after a prompt, the user must
//!   still be next to the machine.
// shortcut: no overlays (beaker, screen, buffer fill, extruder), no light, no `adjust_item_
// drop_location` pixel offsets; printable containers are listed by type path (tg: declaration
// order); `addictionTypes` is always empty (addictions are not ported); the board's
// screwdriver mode switch (ChemMaster/CondiMaster) is not ported; transfers need the machine
// powered but do not fail on a short `use_energy`.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::{DmName, DmObj, DmPower, DmReagents, IconIndex, Template};
use ss13_proto::Prototypes;
use ss13_reagents::{Holder, Owner, round_to};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::chem_master::{
    ASK_AMOUNT, AnalyzableBeaker, AnalyzableReagent, ChemMasterData, ChemMasterStatic,
    ChemMasterUi, ContainerCategory, PrintableContainer,
};
use ss13_tgui::interfaces::vending::ProductIcon;
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems, ask_world};
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use crate::chem::{Chem, Reagents};
use crate::chem_dispenser::{InsertedBeaker, beaker_of, chem_container, replace_beaker};
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::power::Unpowered;
use crate::stock_parts::{ComponentParts, PartKind};
use crate::storage::StoredIn;

pub const MASTER_ROOT: &str = "/obj/machinery/chem_master";
const CONDIMASTER: &str = "/obj/machinery/chem_master/condimaster";
const BEAKER: &str = "/obj/item/reagent_containers/cup/beaker";
const PILL_ROOT: &str = "/obj/item/reagent_containers/applicator/pill";
/// `MAX_CONTAINER_PRINT_AMOUNT`.
pub const MAX_PRINTABLE: u32 = 50;
/// `PILL_MAX_PRINTABLE_LAYERS`.
pub const MAX_PILL_LAYERS: f32 = 30.0;
/// `/obj/item/reagent_containers/applicator/pill::layers_remaining`.
pub const PILL_LAYERS: f32 = 3.0;
/// `create_reagents(100)` before `RefreshParts`.
const BUFFER_VOLUME: f32 = 100.0;
/// `MAX_NAME_LEN`.
const MAX_NAME_LEN: u32 = 42;
/// Seconds: `printing_speed` with no servo, the cut per servo tier and the floor.
const PRINT_STEP: f64 = 1.0;
const SERVO_CUT: f64 = 0.25;
const MIN_PRINT_STEP: f64 = 0.25;

/// `GLOB.reagent_containers` categories: (category, the types under these roots, the root
/// itself included as `typecacheof` does).
const CHEM_CATEGORIES: [(&str, &str); 3] = [
    ("tubes", "/obj/item/reagent_containers/cup/tube"),
    (
        "pills",
        "/obj/item/reagent_containers/applicator/pill/style",
    ),
    (
        "patches",
        "/obj/item/reagent_containers/applicator/patch/style",
    ),
];
/// `CAT_CONDIMENTS`: an explicit list, in its order.
const CONDIMENTS: [&str; 23] = [
    "/obj/item/reagent_containers/cup/bottle",
    "/obj/item/reagent_containers/condiment/flour",
    "/obj/item/reagent_containers/condiment/sugar",
    "/obj/item/reagent_containers/condiment/rice",
    "/obj/item/reagent_containers/condiment/cornmeal",
    "/obj/item/reagent_containers/condiment/milk",
    "/obj/item/reagent_containers/condiment/soymilk",
    "/obj/item/reagent_containers/condiment/yoghurt",
    "/obj/item/reagent_containers/condiment/saltshaker",
    "/obj/item/reagent_containers/condiment/peppermill",
    "/obj/item/reagent_containers/condiment/soysauce",
    "/obj/item/reagent_containers/condiment/bbqsauce",
    "/obj/item/reagent_containers/condiment/enzyme",
    "/obj/item/reagent_containers/condiment/hotsauce",
    "/obj/item/reagent_containers/condiment/coldsauce",
    "/obj/item/reagent_containers/condiment/mayonnaise",
    "/obj/item/reagent_containers/condiment/ketchup",
    "/obj/item/reagent_containers/condiment/olive_oil",
    "/obj/item/reagent_containers/condiment/vegetable_oil",
    "/obj/item/reagent_containers/condiment/peanut_butter",
    "/obj/item/reagent_containers/condiment/cherryjelly",
    "/obj/item/reagent_containers/condiment/honey",
    "/obj/item/reagent_containers/condiment/pack",
];

/// A ChemMaster's live state; its buffer is its [`Reagents`].
#[derive(Component, Clone, Debug)]
#[require(InsertedBeaker)]
pub struct ChemMaster {
    pub is_transfering: bool,
    /// The selected printable type and the one `selectContainer` falls back to.
    pub selected: String,
    pub default_container: String,
    pub pill_layers: f32,
    pub custom_transfer_amount: f32,
    /// Seconds per printed container.
    pub printing_speed: f64,
    pub printing: Option<PrintJob>,
    pub printables: Arc<ChemMasterStatic>,
}

/// Containers being printed (`create_containers` and its timers).
#[derive(Clone, Debug, PartialEq)]
pub struct PrintJob {
    pub progress: u32,
    pub total: u32,
    pub name: String,
    pub volume_each: f32,
    pub container: String,
    /// `Time::elapsed_secs_f64` of the next one.
    pub next_at: f64,
}

/// The printable categories of both machine kinds, built once from the prototypes.
#[derive(Resource, Default)]
struct Printables(HashMap<bool, Arc<ChemMasterStatic>>);

/// What a prompt opened by the window was for.
#[derive(Component, Clone, Debug)]
enum MasterPrompt {
    /// `tgui_input_number` of a `transfer` with amount -1.
    Amount {
        machine: Entity,
        reagent: String,
        target: String,
    },
    /// `tgui_input_text` of `create`.
    Name {
        machine: Entity,
        count: u32,
        volume_each: f32,
        container: String,
    },
}

pub struct ChemMasterPlugin;

impl Plugin for ChemMasterPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::EmptyHand, 1400, |w, c| {
            crate::chem_master::interact(w, c.client, c.mob, c.target, c.modifiers.right)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 2500, |w, c| {
            c.held.is_some_and(|h| {
                crate::chem_master::item_interaction(w, c.client, c.mob, h, c.target)
            })
        });
        app.init_resource::<Printables>()
            .register_ui::<MasterWindow>()
            .add_systems(
                ss13_world::MapInit,
                setup_mapped
                    .after(crate::power::init_power)
                    .after(crate::mecha::core::MechaLoaded),
            )
            .add_systems(
                Update,
                (
                    master_acts.in_set(UiSystems::Handle),
                    prompt_answers
                        .in_set(UiSystems::Handle)
                        .after(PromptSystems::Answer),
                    print_containers,
                ),
            );
    }
}

fn path_of(world: &World, e: Entity) -> Option<String> {
    world.get::<DmObj>(e).map(|o| o.path.clone())
}

/// A ChemMaster or CondiMaster type.
pub fn is_master(path: &str) -> bool {
    is_under(path, MASTER_ROOT)
}

fn setup_mapped(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &DmObj), With<ss13_world::Mapped>>();
    let found: Vec<Entity> = query
        .iter(world)
        .filter(|(_, o)| is_master(&o.path))
        .map(|(e, _)| e)
        .collect();
    for e in found {
        setup(world, e);
    }
}

/// `Initialize`: the buffer, the printable containers, then `RefreshParts`.
pub fn setup(world: &mut World, e: Entity) {
    let Some(path) = path_of(world, e) else {
        return;
    };
    let condi = is_under(&path, CONDIMASTER);
    let printables = printables(world, condi);
    let default_container = printables
        .categories
        .first()
        .and_then(|c| c.containers.first())
        .map(|c| c.r#ref.clone())
        .unwrap_or_default();
    crate::stock_parts::ensure_parts(world, e);
    world.entity_mut(e).insert((
        ChemMaster {
            is_transfering: true,
            selected: default_container.clone(),
            default_container,
            pill_layers: PILL_LAYERS,
            custom_transfer_amount: 15.0,
            printing_speed: 0.75,
            printing: None,
            printables,
        },
        Reagents(Holder::new(
            BUFFER_VOLUME,
            0,
            Owner {
                path,
                living: false,
            },
        )),
    ));
    refresh_parts(world, e);
}

/// tg `chem_master/RefreshParts`.
pub fn refresh_parts(world: &mut World, e: Entity) {
    if world.get::<ChemMaster>(e).is_none() {
        return;
    }
    let beakers: Vec<f32> = world
        .get::<ComponentParts>(e)
        .map(|p| p.parts().to_vec())
        .unwrap_or_default()
        .into_iter()
        .filter(|&p| {
            world
                .get::<ss13_mapload::DmItem>(p)
                .is_some_and(|i| is_under(&i.path, BEAKER))
        })
        .filter_map(|p| world.get::<Reagents>(p).map(|r| r.0.maximum_volume))
        .collect();
    let volume = if beakers.is_empty() {
        BUFFER_VOLUME
    } else {
        beakers.iter().sum()
    };
    let r = crate::stock_parts::ratings(world, e);
    let speed = r
        .tiers(PartKind::Servo)
        .fold(PRINT_STEP, |s, t| s - f64::from(t) * SERVO_CUT)
        .max(MIN_PRINT_STEP);
    if let Some(mut m) = world.get_mut::<ChemMaster>(e) {
        m.printing_speed = speed;
    }
    if let Some(mut r) = world.get_mut::<Reagents>(e) {
        r.0.maximum_volume = volume;
    }
}

/// `load_printable_containers` with each type's name, volume and picture.
fn printables(world: &mut World, condi: bool) -> Arc<ChemMasterStatic> {
    if let Some(p) = world.resource::<Printables>().0.get(&condi) {
        return p.clone();
    }
    let index: HashMap<(&str, &str), ss13_shared::IconStateId> = world
        .get_resource::<IconIndex>()
        .map(|i| {
            i.0.iter()
                .map(|((dmi, state), id)| ((dmi.as_str(), state.as_str()), *id))
                .collect()
        })
        .unwrap_or_default();
    let protos = world.get_resource::<Prototypes>();
    let container = |path: &str| {
        let proto = protos?.get(path)?.clone();
        let name = proto
            .get::<DmName>()
            .map(|n| crate::vending::plain_name(&n.name).to_owned())
            .unwrap_or_default();
        let volume = proto.get::<DmReagents>()?.volume;
        let icon = Template::new(proto, &index)
            .appearance
            .map(|a| ProductIcon {
                icon_state: a.icon_state,
                color: a.color,
            });
        Some(PrintableContainer {
            r#ref: path.to_owned(),
            name,
            icon,
            volume,
        })
    };
    let categories = if condi {
        vec![ContainerCategory {
            name: "condiments".into(),
            containers: CONDIMENTS.iter().filter_map(|p| container(p)).collect(),
        }]
    } else {
        CHEM_CATEGORIES
            .iter()
            .map(|(name, root)| {
                let mut paths: Vec<&str> = protos
                    .map(|p| p.ids().filter(|id| is_under(id, root)).collect())
                    .unwrap_or_default();
                paths.sort_unstable();
                ContainerCategory {
                    name: (*name).into(),
                    containers: paths.into_iter().filter_map(container).collect(),
                }
            })
            .collect()
    };
    let st = Arc::new(ChemMasterStatic {
        max_printable: MAX_PRINTABLE,
        max_pill_duration: MAX_PILL_LAYERS as u32,
        categories,
    });
    world
        .resource_mut::<Printables>()
        .0
        .insert(condi, st.clone());
    st
}

fn operational(world: &World, e: Entity) -> bool {
    world.get::<Unpowered>(e).is_none()
}

/// `use_energy(active_power_usage)`.
fn use_active_power(world: &mut World, e: Entity) {
    let active = world
        .get::<DmPower>(e)
        .map_or(0.0, |p| p.active_power_usage);
    crate::kitchen::machine_use_energy(world, e, active);
}

/// `item_interaction`: a container goes in and the window opens. True when the click was the
/// machine's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<ChemMaster>(target).is_none() || !chem_container(world, held) {
        return false;
    }
    replace_beaker(world, mob, target, Some(held));
    open_window(world, client, mob, target);
    true
}

/// `attack_hand` (the window) and `attack_hand_secondary` (the beaker out).
pub fn interact(
    world: &mut World,
    client: Entity,
    mob: Entity,
    target: Entity,
    right: bool,
) -> bool {
    if world.get::<ChemMaster>(target).is_none() {
        return false;
    }
    if right {
        replace_beaker(world, mob, target, None);
    } else {
        open_window(world, client, mob, target);
    }
    true
}

fn open_window(world: &mut World, client: Entity, mob: Entity, target: Entity) {
    let title = world
        .get::<DmName>(target)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<MasterWindow>(client, mob, target, title));
}

pub struct MasterWindow;

impl UiInterface for MasterWindow {
    const NAME: &'static str = ChemMasterUi::NAME;
    type Data = ChemMasterData;
    type StaticData = ChemMasterStatic;
}

#[derive(Clone, Debug, PartialEq)]
pub enum MasterAct {
    Eject,
    Insert,
    SetCustomTransfer(f32),
    Transfer {
        reagent: String,
        amount: f32,
        target: String,
    },
    ToggleTransferMode,
    StopPrinting,
    SetPillDuration(f32),
    SelectContainer(String),
    Create(f32),
}

/// One reagent of a holder as the window lists it.
fn analyzable(chem: &ss13_reagents::Chemistry, h: &Holder) -> Vec<AnalyzableReagent> {
    h.reagents
        .iter()
        .map(|r| {
            let def = chem.def(r.id);
            AnalyzableReagent {
                r#ref: def.path.clone(),
                name: def.name.clone(),
                volume: round_to(r.volume, ss13_reagents::CHEMICAL_VOLUME_ROUNDING),
                ph: r.ph,
                color: def.color.clone(),
                description: def.description.clone(),
                purity: r.purity,
                meta_rate: r.metabolization_rate,
                overdose: r.overdose_threshold,
                addiction_types: crate::addictions::describe(def),
            }
        })
        .collect()
}

/// The category of a printable type.
fn category_of<'a>(st: &'a ChemMasterStatic, path: &str) -> Option<&'a ContainerCategory> {
    st.categories
        .iter()
        .find(|c| c.containers.iter().any(|x| x.r#ref == path))
}

fn printable<'a>(st: &'a ChemMasterStatic, path: &str) -> Option<&'a PrintableContainer> {
    st.categories
        .iter()
        .flat_map(|c| &c.containers)
        .find(|x| x.r#ref == path)
}

impl ServerUi for MasterWindow {
    type Source = (
        &'static ChemMaster,
        &'static Reagents,
        &'static InsertedBeaker,
        Entity,
    );
    type UserParam = (
        Query<'static, 'static, &'static Reagents>,
        Query<'static, 'static, &'static DmReagents>,
        Query<'static, 'static, &'static Inventory>,
        Query<'static, 'static, &'static StoredIn>,
        Option<Res<'static, Chem>>,
    );
    type Act = MasterAct;

    fn static_data((m, ..): (&ChemMaster, &Reagents, &InsertedBeaker, Entity)) -> ChemMasterStatic {
        (*m.printables).clone()
    }

    fn data(
        (m, buffer, inserted, e): (&ChemMaster, &Reagents, &InsertedBeaker, Entity),
        user: Entity,
        (reagents, dm_reagents, inventories, stored, chem): &SystemParamItem<
            '_,
            '_,
            Self::UserParam,
        >,
    ) -> ChemMasterData {
        let empty = Vec::new;
        let list = |h: &Holder| chem.as_ref().map_or_else(empty, |c| analyzable(&c.0, h));
        let beaker = inserted
            .0
            .filter(|&b| stored.get(b).is_ok_and(|s| s.0 == e))
            .and_then(|b| {
                let h = &reagents.get(b).ok()?.0;
                Some(AnalyzableBeaker {
                    max_volume: dm_reagents.get(b).map_or(h.maximum_volume, |d| d.volume),
                    current_volume: h.total_volume,
                    contents: list(h),
                })
            });
        let b = &buffer.0;
        let suggested = if b.total_volume > 0.0 {
            chem.as_ref()
                .zip(b.master_reagent())
                .map(|(c, r)| c.0.def(r.id).default_container.clone())
                .filter(|p| printable(&m.printables, p).is_some())
                .unwrap_or_else(|| m.default_container.clone())
        } else {
            m.default_container.clone()
        };
        let job = m.printing.as_ref();
        ChemMasterData {
            is_printing: job.is_some(),
            printing_progress: job.map_or(0, |j| j.progress),
            printing_total: job.map_or(0, |j| j.total),
            selected_pill_duration: m.pill_layers,
            has_beaker_in_hand: crate::chem_dispenser::beaker_in_hand(
                user,
                inventories,
                dm_reagents,
                reagents,
            ),
            custom_transfer_amount: m.custom_transfer_amount,
            beaker,
            buffer: AnalyzableBeaker {
                max_volume: b.maximum_volume,
                current_volume: b.total_volume,
                contents: list(b),
            },
            is_transfering: m.is_transfering,
            suggested_container_ref: suggested,
            selected_container_ref: m.selected.clone(),
            selected_container_volume: printable(&m.printables, &m.selected)
                .map_or(0.0, |c| c.volume),
            selected_container_category: category_of(&m.printables, &m.selected)
                .map(|c| c.name.clone()),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<MasterAct, UiReject> {
        let number = |key: &str| params.number(key).map(|n| n as f32);
        Ok(match action {
            "eject" => MasterAct::Eject,
            "insert" => MasterAct::Insert,
            "setCustomTransfer" => MasterAct::SetCustomTransfer(number("target")?),
            "transfer" => MasterAct::Transfer {
                reagent: params.text("reagentRef")?,
                amount: number("amount")?,
                target: params.text("target")?,
            },
            "toggleTransferMode" => MasterAct::ToggleTransferMode,
            "stopPrinting" => MasterAct::StopPrinting,
            "setPillDuration" => MasterAct::SetPillDuration(number("duration")?),
            "selectContainer" => MasterAct::SelectContainer(params.text("ref")?),
            "create" => MasterAct::Create(number("itemCount")?),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

fn master_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<MasterWindow>>>) {
    let pending: Vec<(Entity, Entity, Entity, MasterAct)> = acts
        .read(world.resource::<Messages<UiActed<MasterWindow>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, act) in pending {
        apply_act(world, client, actor, source, act);
    }
}

fn say(world: &mut World, e: Entity, text: &str) {
    crate::cargo::console::machine_say(world, e, text);
}

/// tg `ui_act`, every check at the moment of effect.
pub fn apply_act(world: &mut World, client: Entity, actor: Entity, e: Entity, act: MasterAct) {
    let Some(m) = world.get::<ChemMaster>(e).cloned() else {
        return;
    };
    let printing = m.printing.is_some();
    match act {
        MasterAct::Eject => {
            if printing {
                say(world, e, "The buffer is locked while printing.");
                return;
            }
            replace_beaker(world, actor, e, None);
        }
        MasterAct::Insert => crate::chem_dispenser::insert_held(world, actor, e),
        MasterAct::SetCustomTransfer(target) => {
            let Some(max) = beaker_of(world, e)
                .and_then(|b| world.get::<DmReagents>(b))
                .map(|d| d.volume)
            else {
                return;
            };
            if target.is_finite()
                && let Some(mut m) = world.get_mut::<ChemMaster>(e)
            {
                m.custom_transfer_amount = target.clamp(0.0, max);
            }
        }
        MasterAct::Transfer {
            reagent,
            amount,
            target,
        } => {
            if printing {
                say(world, e, "The buffer is locked while printing.");
                return;
            }
            if amount == ASK_AMOUNT {
                let prompt = ask_world(
                    world,
                    Ask::new(
                        client,
                        actor,
                        "Transfer amount",
                        "Enter amount to transfer",
                        PromptKind::Number {
                            default: 0.0,
                            min: 0.0,
                            max: 10_000.0,
                            round: false,
                        },
                    ),
                );
                world.entity_mut(prompt).insert(MasterPrompt::Amount {
                    machine: e,
                    reagent,
                    target,
                });
                return;
            }
            transfer(world, e, &reagent, amount, &target);
        }
        MasterAct::ToggleTransferMode => {
            if let Some(mut m) = world.get_mut::<ChemMaster>(e) {
                m.is_transfering = !m.is_transfering;
            }
        }
        MasterAct::StopPrinting => {
            if let Some(mut m) = world.get_mut::<ChemMaster>(e) {
                m.printing = None;
            }
        }
        MasterAct::SetPillDuration(duration) => {
            if duration.is_finite()
                && let Some(mut m) = world.get_mut::<ChemMaster>(e)
            {
                m.pill_layers = duration.clamp(0.0, MAX_PILL_LAYERS);
            }
        }
        MasterAct::SelectContainer(path) => {
            if printable(&m.printables, &path).is_some()
                && let Some(mut m) = world.get_mut::<ChemMaster>(e)
            {
                m.selected = path;
            }
        }
        MasterAct::Create(count) => {
            let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
                return;
            };
            let Some(buffer) = world.get::<Reagents>(e).map(|r| r.0.clone()) else {
                return;
            };
            if buffer.total_volume == 0.0 || printing || !count.is_finite() || count <= 0.0 {
                return;
            }
            let Some(container) = printable(&m.printables, &m.selected).cloned() else {
                return;
            };
            let count = (count.min(MAX_PRINTABLE as f32) as u32).max(1);
            let volume_each = volume_each(buffer.total_volume, count, container.volume);
            let flags = world
                .get_resource::<Prototypes>()
                .and_then(|p| p.get(&container.r#ref))
                .and_then(|p| p.get::<DmReagents>().map(|d| d.initial_reagent_flags))
                .unwrap_or(0);
            let master = buffer
                .master_reagent()
                .map(|r| chem.def(r.id).name.clone())
                .unwrap_or_default();
            let default = default_name(
                &container.name,
                &master,
                container.r#ref == m.default_container,
                flags & ss13_reagents::flags::OPENCONTAINER == ss13_reagents::flags::OPENCONTAINER,
                volume_each,
            );
            let prompt = ask_world(
                world,
                Ask::new(
                    client,
                    actor,
                    "Name",
                    "Container name",
                    PromptKind::Text {
                        default,
                        max_length: MAX_NAME_LEN,
                        multiline: false,
                    },
                ),
            );
            world.entity_mut(prompt).insert(MasterPrompt::Name {
                machine: e,
                count,
                volume_each,
                container: container.r#ref,
            });
        }
    }
}

/// `volume_in_each`: `min(round(total / count, 0.01), container volume)`.
pub fn volume_each(total: f32, count: u32, container_volume: f32) -> f32 {
    round_to(
        total / count.max(1) as f32,
        ss13_reagents::CHEMICAL_VOLUME_ROUNDING,
    )
    .min(container_volume)
}

/// `item_name_default`: the default container (tubes, bottles) gains the main reagent's name;
/// a closed one (pills, patches) gains it and the volume.
pub fn default_name(
    container: &str,
    master: &str,
    is_default: bool,
    open: bool,
    volume_each: f32,
) -> String {
    let mut name = container.to_owned();
    if is_default {
        name = format!("{master} {name}");
    }
    if !open {
        name = format!("{master} {name} ({}u)", crate::chem::dm_num(volume_each));
    }
    name
}

/// tg `transfer_reagent` of a `transfer` act.
fn transfer(world: &mut World, e: Entity, reagent: &str, amount: f32, target: &str) {
    if !operational(world, e) || !amount.is_finite() || amount <= 0.0 {
        return;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let Some(id) = chem.id(reagent) else {
        return;
    };
    let Some(m) = world.get::<ChemMaster>(e) else {
        return;
    };
    let to_buffer = match target {
        "buffer" => true,
        "beaker" => false,
        _ => return,
    };
    let should_transfer = m.is_transfering || to_buffer;
    let beaker = beaker_of(world, e);
    if should_transfer && beaker.is_none() {
        say(world, e, "No reagent container is inserted.");
        return;
    }
    use_active_power(world, e);
    if !should_transfer {
        if let Some(mut r) = world.get_mut::<Reagents>(e) {
            r.0.remove_reagent(id, amount);
        }
        return;
    }
    let beaker = beaker.expect("checked above");
    let (from, to) = if to_buffer { (beaker, e) } else { (e, beaker) };
    let (Some(mut a), Some(mut b)) = (
        world.get::<Reagents>(from).map(|r| r.0.clone()),
        world.get::<Reagents>(to).map(|r| r.0.clone()),
    ) else {
        return;
    };
    if b.is_reacting {
        return;
    }
    a.trans_to(&chem, &mut b, amount, 1.0, Some(id), false, false);
    world.entity_mut(from).insert(Reagents(a));
    world.entity_mut(to).insert(Reagents(b));
}

/// The answers to the window's prompts, checked again now: the user next to the machine.
fn prompt_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.actor, a.answer.clone()))
        .collect();
    for (prompt, actor, answer) in answers {
        let Some(asked) = world.get::<MasterPrompt>(prompt).cloned() else {
            continue;
        };
        let machine = match &asked {
            MasterPrompt::Amount { machine, .. } | MasterPrompt::Name { machine, .. } => *machine,
        };
        if world.get::<ChemMaster>(machine).is_none()
            || !crate::storage::in_reach(world, actor, machine)
        {
            continue;
        }
        match (asked, answer) {
            (
                MasterPrompt::Amount {
                    reagent, target, ..
                },
                Some(PromptAnswer::Number(n)),
            ) => {
                let amount = round_to(n as f32, ss13_reagents::CHEMICAL_VOLUME_ROUNDING);
                let printing = world
                    .get::<ChemMaster>(machine)
                    .is_some_and(|m| m.printing.is_some());
                if amount > 0.0 && !printing {
                    transfer(world, machine, &reagent, amount, &target);
                }
            }
            (
                MasterPrompt::Name {
                    count,
                    volume_each,
                    container,
                    ..
                },
                Some(PromptAnswer::Text(name)),
            ) => start_printing(world, machine, name, count, volume_each, container),
            _ => {}
        }
    }
}

/// After the name prompt: `if(!item_name || is_printing) return`, then printing starts.
fn start_printing(
    world: &mut World,
    e: Entity,
    name: String,
    count: u32,
    volume_each: f32,
    container: String,
) {
    let name = name.trim().to_owned();
    if name.is_empty() || !operational(world, e) {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(mut m) = world.get_mut::<ChemMaster>(e) else {
        return;
    };
    if m.printing.is_some() {
        return;
    }
    m.printing = Some(PrintJob {
        progress: 0,
        total: count,
        name,
        volume_each,
        container,
        next_at: now,
    });
}

/// `create_containers` and its timers: one container every `printing_speed` seconds.
// Cost: one pass over the ChemMasters each tick (a handful per map).
fn print_containers(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &ChemMaster)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, m)| m.printing.as_ref().is_some_and(|j| now >= j.next_at))
        .map(|(e, _)| e)
        .collect();
    for e in due {
        print_one(world, e, now);
    }
}

fn print_one(world: &mut World, e: Entity, now: f64) {
    // `on_set_is_operational`: losing power stops it.
    if !operational(world, e) {
        if let Some(mut m) = world.get_mut::<ChemMaster>(e) {
            m.printing = None;
        }
        return;
    }
    let Some(m) = world.get::<ChemMaster>(e).cloned() else {
        return;
    };
    let Some(job) = m.printing.clone() else {
        return;
    };
    use_active_power(world, e);
    if let Some(&pos) = world.get::<TilePos>(e) {
        print_container(world, e, pos, &job, m.pill_layers);
    }
    let done = job.progress + 1 >= job.total;
    if let Some(mut m) = world.get_mut::<ChemMaster>(e) {
        m.printing = (!done).then(|| PrintJob {
            progress: job.progress + 1,
            next_at: now + m.printing_speed,
            ..job
        });
    }
}

/// One printed container: named, emptied, filled with `volume_each` of the buffer; a pill
/// gets the chosen sugar layers.
fn print_container(world: &mut World, e: Entity, pos: TilePos, job: &PrintJob, layers: f32) {
    let Some(item) = crate::construction::spawn_obj(world, &job.container, pos) else {
        warn!("chem master: no prototype for {}", job.container);
        return;
    };
    if let Some(mut n) = world.get_mut::<DmName>(item) {
        n.name = job.name.clone();
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let (Some(mut buffer), Some(mut holder)) = (
        world.get::<Reagents>(e).map(|r| r.0.clone()),
        world.get::<Reagents>(item).map(|r| r.0.clone()),
    ) else {
        return;
    };
    holder.clear();
    buffer.trans_to(&chem, &mut holder, job.volume_each, 1.0, None, false, false);
    world.entity_mut(e).insert(Reagents(buffer));
    world.entity_mut(item).insert(Reagents(holder));
    if is_under(&job.container, PILL_ROOT) {
        world
            .entity_mut(item)
            .insert(crate::chem::PillShell(layers));
    }
}

/// `examine`: the buffer, the printing speed and the beaker.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    let (Some(m), Some(r)) = (
        world.get::<ChemMaster>(target),
        world.get::<Reagents>(target),
    ) else {
        return;
    };
    lines.push((
        Span::Notice,
        format!(
            "Дисплей состояния: ёмкость буфера — {} ед. Скорость печати — {}%.",
            crate::chem::dm_num(r.0.maximum_volume),
            crate::chem::dm_num((0.75 / m.printing_speed * 100.0) as f32)
        ),
    ));
    match beaker_of(world, target) {
        Some(b) => {
            let name = crate::chem::name_of(world, b);
            let volume = world.get::<Reagents>(b).map_or(0.0, |r| r.0.maximum_volume);
            lines.push((
                Span::Notice,
                format!(
                    "Вставлен {name} на {} ед. ПКМ пустой рукой — достать.",
                    crate::chem::dm_num(volume)
                ),
            ));
        }
        None => lines.push((Span::Warning, "Нет ёмкости для реагентов.".to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_each_splits_and_caps_at_the_container() {
        assert!((volume_each(100.0, 3, 50.0) - 33.33).abs() < 1e-4);
        assert_eq!(volume_each(100.0, 1, 50.0), 50.0);
        assert_eq!(volume_each(10.0, 4, 30.0), 2.5);
    }

    #[test]
    fn default_names_follow_the_container_kind() {
        // A tube (the default container, open): the main reagent's name in front.
        assert_eq!(
            default_name("tube", "Water", true, true, 10.0),
            "Water tube"
        );
        // A pill (closed): the name and the volume.
        assert_eq!(
            default_name("pill", "Bicaridine", false, false, 12.5),
            "Bicaridine pill (12.5u)"
        );
        // A condiment bottle that is the CondiMaster's default and open.
        assert_eq!(
            default_name("bottle", "Ketchup", true, true, 50.0),
            "Ketchup bottle"
        );
        // Another open condiment: its own name.
        assert_eq!(
            default_name("honey pot", "Honey", false, true, 50.0),
            "honey pot"
        );
    }
}
