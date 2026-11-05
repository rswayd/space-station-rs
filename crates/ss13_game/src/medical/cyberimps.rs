//! Cybernetic implants and autosurgeons (server), after tg
//! `code/modules/surgery/organs/internal/cyberimp` (`augments_internal.dm`: the base implant,
//! the anti-drop and CNS rebooter brain implants, the breathing tube; `augments_chest.dm`: the
//! nutriment pumps and the reviver; `augments_eyes.dm`: the HUD implants; `augments_arms.dm`:
//! the arm toolkits), BandaStation `modular_bandastation/objects/code/items/implants/
//! cyberimp_arm.dm` (the CentCom toolkit) and `code/modules/surgery/organs/autosurgeon.dm`.
//!
//! An implant is an item entity of an `/obj/item/organ/cyberimp` type with [`Implant`]: inside a
//! body it is [`OrganOf`] that mob like an organ (the mob's [`Organs`] hold both; organ code only
//! reads [`organs::Organ`], so implants are invisible to it), at most one per [`ImplantSlot`]. It
//! goes in and out by organ manipulation ([`super::surgery`]) or an autosurgeon; EMPs reach it
//! through the mob's organs (`crate::emp`).
//!
//! - Arm toolkits keep their tools as detached entities ([`ToolOf`]); the implant's action button
//!   extends one into the arm's hand (stuck there, [`NoDrop`]) or retracts it; with several tools
//!   a list picks one, and using the extended tool in hand goes back to the list.
//! - The anti-drop implant's button locks or releases what the hands hold; the HUD implants'
//!   buttons toggle their HUD ([`implant_huds`], read by `crate::records::hud`); the breathing
//!   tube lets internals work without a mask ([`breathing_tube`], `crate::internals`).
//! - Each `SSmobs` fire the nutriment pumps feed a hungry owner and the reviver heals an owner
//!   down in crit (and revives the dead it can); the CNS rebooter clears stuns a second after
//!   they land, then resists them for 6 s, once a minute.
//! - An autosurgeon holds one organ or implant: an organ or implant used on it (other hand)
//!   loads it, a screwdriver takes it out, using it in hand implants yourself at once, on someone
//!   else after 8 s.
// shortcut: no bodypart overlays, sounds, sparks or eye colours; no thrusters, spine (Atlas),
// skillchip connector, surgical processor, strong-arm, shard blades or BandaStation pumps
// (they go in and out but do nothing but their brain-implant EMP stun); no diagnostic HUD (none
// exists yet); no emagging toolkits, no drop-key retract, no custom arm implant; an arm implant
// stays in the body when its arm is cut off (it only cannot extend into a missing hand); no
// autosurgeon on items on the floor, no TK check, no name suffix.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmItem, DmTool};
use ss13_net::server::Networked;
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;
use ss13_tgui::interfaces::list_input::{ListInputData, ListInputUi};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_tgui::{UiInterface, UiStatus};

use super::organs::{self, Organ, OrganHost, OrganOf, Organs, Slot};
use super::surgery::ListAct;
use super::{MedJob, name_of, visible};
use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::emp::{EmpAct, EmpRng, EmpSystems, emp_acts};
use crate::health::{DamageType, Health, Stat};
use crate::inventory::{CarriedBy, Inventory, NoDrop};
use crate::server::PlayerMob;
use crate::status_effects::{Incap, StatusEffects};
use crate::tools::{Actor, do_after, tell};
use crate::zone::{BodyZone, TargetZone};

pub const ROOT: &str = "/obj/item/organ/cyberimp";
pub const AUTOSURGEON_ROOT: &str = "/obj/item/autosurgeon";
/// Action button ids: the prefix and the slot's index.
const BUTTON: &str = "cyberimp:";
const BUTTON_BACKGROUNDS: &str = "icons/mob/actions/backgrounds.dmi";
/// The rebooter's `stun_cap_amount`, `stun_resistance_time`, `implant_cooldown` and EMP reboot.
const REBOOT_DELAY: f64 = 1.0;
const STUN_RESISTANCE_SECS: f64 = 6.0;
const REBOOTER_COOLDOWN: f64 = 60.0;
const REBOOTER_EMP_SECS: f64 = 9.0;
/// The nutriment pump's `synth_cool` timer and feed rate per second.
const PUMP_COOLDOWN: f64 = 5.0;
const PUMP_FEED: f32 = 25.0;
/// The reviver's `defib_cooldown` on starting and after a revival, the revival's extra
/// `revive_cost` (10 minutes) and the heart attack an EMP gives (`50 SECONDS / severity`).
const REVIVER_DEFIB_DELAY: f64 = 8.0;
const REVIVER_DEFIB_AGAIN: f64 = 16.0;
const REVIVER_REVIVE_COST: f64 = 600.0;
const REVIVER_HEART_ATTACK: f64 = 50.0;
const BAD_FOOD: &str = "/datum/reagent/toxin/bad_food";
/// `use_autosurgeon(target, user, 8 SECONDS)` on someone else, deciseconds.
const AUTOSURGEON_DELAY: f32 = 80.0;

/// tg `ORGAN_SLOT_*` of the implants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImplantSlot {
    RightArm,
    LeftArm,
    /// `ORGAN_SLOT_STOMACH_AID`: nutriment pumps.
    StomachAid,
    /// `ORGAN_SLOT_HEART_AID`: the reviver.
    HeartAid,
    Thrusters,
    Spine,
    /// `ORGAN_SLOT_BRAIN_CEREBELLUM`: the anti-drop implant.
    Cerebellum,
    /// `ORGAN_SLOT_BRAIN_CNS`: the CNS rebooter, the skillchip connector.
    Cns,
    /// `ORGAN_SLOT_BRAIN_HIPPOCAMPUS`: the surgical processor.
    Hippocampus,
    /// `ORGAN_SLOT_HUD`.
    Hud,
    BreathingTube,
}

impl ImplantSlot {
    pub const ALL: [Self; 11] = [
        Self::RightArm,
        Self::LeftArm,
        Self::StomachAid,
        Self::HeartAid,
        Self::Thrusters,
        Self::Spine,
        Self::Cerebellum,
        Self::Cns,
        Self::Hippocampus,
        Self::Hud,
        Self::BreathingTube,
    ];

    /// The implant's `zone` in this slot.
    pub fn zone(self) -> TargetZone {
        match self {
            Self::RightArm => TargetZone::RArm,
            Self::LeftArm => TargetZone::LArm,
            Self::StomachAid | Self::HeartAid | Self::Thrusters | Self::Spine => TargetZone::Chest,
            Self::Cerebellum | Self::Cns | Self::Hippocampus => TargetZone::Head,
            Self::Hud => TargetZone::Eyes,
            Self::BreathingTube => TargetZone::Mouth,
        }
    }

    /// The hand an arm slot extends into (`hands` index).
    fn hand(self) -> Option<usize> {
        match self {
            Self::LeftArm => Some(0),
            Self::RightArm => Some(1),
            _ => None,
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|&s| s == self).unwrap_or(0)
    }

    fn brain(self) -> bool {
        matches!(self, Self::Cerebellum | Self::Cns | Self::Hippocampus)
    }

    /// Organ manipulation's "remove" option for the slot.
    pub fn remove_name(self) -> &'static str {
        match self {
            Self::RightArm => "Извлечь имплант правой руки",
            Self::LeftArm => "Извлечь имплант левой руки",
            Self::StomachAid => "Извлечь имплант желудка",
            Self::HeartAid => "Извлечь имплант сердца",
            Self::Thrusters => "Извлечь имплант-двигатели",
            Self::Spine => "Извлечь имплант позвоночника",
            Self::Cerebellum => "Извлечь имплант мозжечка",
            Self::Cns => "Извлечь имплант ЦНС",
            Self::Hippocampus => "Извлечь имплант гиппокампа",
            Self::Hud => "Извлечь HUD-имплант",
            Self::BreathingTube => "Извлечь дыхательную трубку",
        }
    }
}

/// What an implant type does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// `/obj/item/organ/cyberimp/arm/toolkit`: `items_to_create`; `gun` for the arm cannons.
    Toolkit {
        items: &'static [&'static str],
        gun: bool,
    },
    /// `hunger_threshold`, `poison_amount`, the black market pump's overeating.
    Nutriment {
        threshold: f32,
        poison: f32,
        black_market: bool,
    },
    Reviver,
    AntiDrop,
    AntiStun,
    /// `HUD_traits`.
    Hud {
        security: bool,
        medical: bool,
    },
    BreathingTube,
    /// Goes in and out, does nothing here.
    Inert,
}

const SURGERY_TOOLS: [&str; 7] = [
    "/obj/item/retractor/augment",
    "/obj/item/hemostat/augment",
    "/obj/item/cautery/augment",
    "/obj/item/surgicaldrill/augment",
    "/obj/item/scalpel/augment",
    "/obj/item/circular_saw/augment",
    "/obj/item/surgical_drapes",
];

/// `items_to_create` of the toolkits, most specific type first.
const TOOLKITS: [(&str, &[&str]); 13] = [
    (
        "/arm/toolkit/toolset",
        &[
            "/obj/item/screwdriver/cyborg",
            "/obj/item/wrench/cyborg",
            "/obj/item/weldingtool/largetank/cyborg",
            "/obj/item/crowbar/cyborg",
            "/obj/item/wirecutters/cyborg",
            "/obj/item/multitool/cyborg",
        ],
    ),
    (
        "/arm/toolkit/paperwork",
        &[
            "/obj/item/pen/fountain",
            "/obj/item/clipboard",
            "/obj/item/taperecorder",
            "/obj/item/lighter",
            "/obj/item/laser_pointer",
            "/obj/item/stamp/mod/toolkit",
        ],
    ),
    (
        "/arm/toolkit/surgery/emagged",
        &[
            SURGERY_TOOLS[0],
            SURGERY_TOOLS[1],
            SURGERY_TOOLS[2],
            SURGERY_TOOLS[3],
            SURGERY_TOOLS[4],
            SURGERY_TOOLS[5],
            SURGERY_TOOLS[6],
            "/obj/item/knife/combat/cyborg",
        ],
    ),
    (
        "/arm/toolkit/surgery/cruel",
        &[
            "/obj/item/retractor/cruel/augment",
            "/obj/item/hemostat/cruel/augment",
            "/obj/item/cautery/cruel/augment",
            "/obj/item/surgicaldrill/cruel/augment",
            "/obj/item/scalpel/cruel/augment",
            "/obj/item/circular_saw/cruel/augment",
            "/obj/item/surgical_drapes",
        ],
    ),
    ("/arm/toolkit/surgery", &SURGERY_TOOLS),
    (
        "/arm/toolkit/esword",
        &["/obj/item/melee/energy/blade/hardlight"],
    ),
    ("/arm/toolkit/medibeam", &["/obj/item/gun/medbeam"]),
    (
        "/arm/toolkit/flash",
        &["/obj/item/assembly/flash/armimplant"],
    ),
    ("/arm/toolkit/baton", &["/obj/item/borg/stun"]),
    (
        "/arm/toolkit/combat",
        &[
            "/obj/item/melee/energy/blade/hardlight",
            "/obj/item/gun/medbeam",
            "/obj/item/borg/stun",
            "/obj/item/assembly/flash/armimplant",
        ],
    ),
    (
        "/arm/toolkit/centcom",
        &[
            "/obj/item/melee/energy/blade/hardlight",
            "/obj/item/gun/medbeam",
            "/obj/item/borg/stun",
            "/obj/item/gun/energy/pulse/pistol/m1911",
        ],
    ),
    (
        "/arm/toolkit/gun/laser",
        &["/obj/item/gun/energy/laser/mounted/augment"],
    ),
    (
        "/arm/toolkit/gun/taser",
        &["/obj/item/gun/energy/e_gun/advtaser/mounted"],
    ),
];

/// The slot (arm implants: the right arm) and kind of an implant type; `None` for anything
/// else (the abstract eye implant takes the eyes' own slot and is not modelled).
pub fn classify(path: &str) -> Option<(ImplantSlot, Kind)> {
    use crate::hunger::{NUTRITION_LEVEL_FAT, NUTRITION_LEVEL_HUNGRY, NUTRITION_LEVEL_STARVING};
    let under = |sub: &str| is_under(path, &format!("{ROOT}{sub}"));
    if under("/arm") {
        let kind =
            TOOLKITS
                .iter()
                .find(|&&(sub, _)| under(sub))
                .map_or(Kind::Inert, |&(sub, items)| Kind::Toolkit {
                    items,
                    gun: sub.starts_with("/arm/toolkit/gun"),
                });
        // The custom implant and the shard blades start empty.
        let kind = if kind == Kind::Inert && under("/arm/toolkit") {
            Kind::Toolkit {
                items: &[],
                gun: false,
            }
        } else {
            kind
        };
        return Some((ImplantSlot::RightArm, kind));
    }
    let nutriment = |threshold, poison, black_market| Kind::Nutriment {
        threshold,
        poison,
        black_market,
    };
    let found = if under("/chest/nutriment/black_market") {
        (
            ImplantSlot::StomachAid,
            nutriment(NUTRITION_LEVEL_FAT, 15.0, true),
        )
    } else if under("/chest/nutriment/plus") {
        (
            ImplantSlot::StomachAid,
            nutriment(NUTRITION_LEVEL_HUNGRY, 10.0, false),
        )
    } else if under("/chest/nutriment") {
        (
            ImplantSlot::StomachAid,
            nutriment(NUTRITION_LEVEL_STARVING, 5.0, false),
        )
    } else if under("/chest/pump") {
        (ImplantSlot::StomachAid, Kind::Inert)
    } else if under("/chest/reviver") {
        (ImplantSlot::HeartAid, Kind::Reviver)
    } else if under("/chest/thrusters") {
        (ImplantSlot::Thrusters, Kind::Inert)
    } else if under("/chest/spine") {
        (ImplantSlot::Spine, Kind::Inert)
    } else if under("/brain/anti_drop") {
        (ImplantSlot::Cerebellum, Kind::AntiDrop)
    } else if under("/brain/anti_stun") {
        (ImplantSlot::Cns, Kind::AntiStun)
    } else if under("/brain/connector") {
        (ImplantSlot::Cns, Kind::Inert)
    } else if under("/brain/surgical_processor") {
        (ImplantSlot::Hippocampus, Kind::Inert)
    } else if under("/eyes/hud") {
        let hud = Kind::Hud {
            security: under("/eyes/hud/security"),
            medical: under("/eyes/hud/medical"),
        };
        (ImplantSlot::Hud, hud)
    } else if under("/mouth/breathing_tube") {
        (ImplantSlot::BreathingTube, Kind::BreathingTube)
    } else {
        return None;
    };
    Some(found)
}

/// The slot an implant of `path` takes when put into `zone` (tg `zone_check`: arm implants go
/// into either arm, everything else only into its own zone).
pub fn slot_for(path: &str, zone: TargetZone) -> Option<ImplantSlot> {
    let (slot, _) = classify(path)?;
    match slot {
        ImplantSlot::RightArm | ImplantSlot::LeftArm => match zone {
            TargetZone::RArm => Some(ImplantSlot::RightArm),
            TargetZone::LArm => Some(ImplantSlot::LeftArm),
            _ => None,
        },
        _ => (zone == slot.zone()).then_some(slot),
    }
}

/// Some implant slot sits in `zone`.
pub fn takes_implants(zone: TargetZone) -> bool {
    ImplantSlot::ALL.iter().any(|s| s.zone() == zone)
}

/// A cybernetic implant (tg `/obj/item/organ/cyberimp` vars).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Implant {
    pub slot: ImplantSlot,
    pub kind: Kind,
    /// `ORGAN_FAILING` (a rebooter rebooting after an EMP, an arm cannon that blew up).
    pub failing: bool,
    /// When the rebooter's EMP failure ends.
    pub reboot_at: Option<f64>,
    /// The cooldown's end: the pump's `synthesizing`, the rebooter's `implant_cooldown`, the
    /// reviver's `reviver_cooldown`.
    pub cooldown_until: f64,
    /// The HUD's `toggled_on`, the anti-drop's `active`, the reviver's `reviving`.
    pub active: bool,
    /// The anti-drop's `stored_items`.
    pub held: Vec<Entity>,
    /// The toolkit's `active_item` out in a hand.
    pub extended: Option<Entity>,
    /// The toolkit's tools were made (`Initialize`, done on first use).
    pub stocked: bool,
    /// The rebooter's pending `clear_stuns`; the reviver's `defib_cooldown`.
    pub timer: Option<f64>,
    /// The end of the rebooter's stun resistance.
    pub buff_until: f64,
    /// The rebooter tells its owner when its cooldown is over (`implant_ready`).
    pub notify_ready: bool,
    /// The reviver's `revive_cost`, seconds.
    pub revive_cost: f64,
}

impl Implant {
    pub fn new(path: &str) -> Option<Self> {
        let (slot, kind) = classify(path)?;
        Some(Self {
            slot,
            kind,
            failing: false,
            reboot_at: None,
            cooldown_until: 0.0,
            active: matches!(kind, Kind::Hud { .. }),
            held: Vec::new(),
            extended: None,
            stocked: false,
            timer: None,
            buff_until: 0.0,
            notify_ready: false,
            revive_cost: 0.0,
        })
    }
}

/// A toolkit's tool (in the implant, or extended into a hand).
#[derive(Component, Debug)]
#[relationship(relationship_target = ImplantTools)]
pub struct ToolOf(pub Entity);

/// The toolkit's tools; they go with the implant.
#[derive(Component, Debug, Default)]
#[relationship_target(relationship = ToolOf, linked_spawn)]
pub struct ImplantTools(Vec<Entity>);

/// What an autosurgeon holds (tg `stored_organ` in its contents).
#[derive(Component, Debug)]
#[relationship(relationship_target = AutosurgeonLoad)]
pub struct LoadedIn(pub Entity);

#[derive(Component, Debug, Default)]
#[relationship_target(relationship = LoadedIn, linked_spawn)]
pub struct AutosurgeonLoad(Vec<Entity>);

/// An autosurgeon's state.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Autosurgeon {
    /// `uses`; `None`: `INFINITY`.
    pub uses: Option<u32>,
    /// `surgery_speed`.
    pub speed: f32,
    /// `organ_whitelist` (the comms agent one takes tongues only).
    pub whitelist: Option<&'static str>,
}

/// `starting_organ`, `uses` of the autosurgeon types, most specific first.
const AUTOSURGEONS: [(&str, &str, Option<u32>); 13] = [
    (
        "/medical_hud",
        "/obj/item/organ/cyberimp/eyes/hud/medical",
        Some(1),
    ),
    (
        "/syndicate/laser_arm",
        "/obj/item/organ/cyberimp/arm/toolkit/gun/laser",
        Some(1),
    ),
    (
        "/syndicate/thermal_eyes/single_use",
        "/obj/item/organ/eyes/robotic/thermals",
        Some(1),
    ),
    (
        "/syndicate/thermal_eyes",
        "/obj/item/organ/eyes/robotic/thermals",
        None,
    ),
    (
        "/syndicate/xray_eyes/single_use",
        "/obj/item/organ/eyes/robotic/xray",
        Some(1),
    ),
    (
        "/syndicate/xray_eyes",
        "/obj/item/organ/eyes/robotic/xray",
        None,
    ),
    (
        "/syndicate/anti_stun/single_use",
        "/obj/item/organ/cyberimp/brain/anti_stun",
        Some(1),
    ),
    (
        "/syndicate/anti_stun",
        "/obj/item/organ/cyberimp/brain/anti_stun",
        None,
    ),
    (
        "/syndicate/reviver/single_use",
        "/obj/item/organ/cyberimp/chest/reviver",
        Some(1),
    ),
    (
        "/syndicate/reviver",
        "/obj/item/organ/cyberimp/chest/reviver",
        None,
    ),
    ("/syndicate/commsagent", "/obj/item/organ/tongue", None),
    (
        "/syndicate/emaggedsurgerytoolset/single_use",
        "/obj/item/organ/cyberimp/arm/toolkit/surgery/emagged",
        Some(1),
    ),
    (
        "/syndicate/emaggedsurgerytoolset",
        "/obj/item/organ/cyberimp/arm/toolkit/surgery/emagged",
        None,
    ),
];
const CONTRABAND_SECHUD: (&str, &str) = (
    "/syndicate/contraband_sechud",
    "/obj/item/organ/cyberimp/eyes/hud/security/syndicate",
);

/// An autosurgeon type's state and starting organ.
pub fn autosurgeon_of(path: &str) -> Option<(Autosurgeon, Option<&'static str>)> {
    if !is_under(path, AUTOSURGEON_ROOT) {
        return None;
    }
    let under = |sub: &str| is_under(path, &format!("{AUTOSURGEON_ROOT}{sub}"));
    let (start, uses) = if under(CONTRABAND_SECHUD.0) {
        (Some(CONTRABAND_SECHUD.1), Some(1))
    } else {
        AUTOSURGEONS
            .iter()
            .find(|&&(sub, ..)| under(sub))
            .map_or((None, None), |&(_, organ, uses)| (Some(organ), uses))
    };
    let state = Autosurgeon {
        uses,
        speed: if under("/syndicate") { 0.75 } else { 1.0 },
        whitelist: under("/syndicate/commsagent").then_some("/obj/item/organ/tongue"),
    };
    Some((state, start))
}

/// A list of toolkit tools waiting for the owner's pick (tg's radial menu).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ToolChoice {
    pub implant: Entity,
    pub tools: Vec<Entity>,
    pub names: Vec<String>,
}

/// The server side of `ListInputWindow` for the toolkit's pick.
pub struct ToolListWindow;

impl UiInterface for ToolListWindow {
    const NAME: &'static str = ListInputUi::NAME;
    type Data = ListInputData;
    type StaticData = ();
}

impl ServerUi for ToolListWindow {
    type Source = &'static ToolChoice;
    type UserParam = ();
    type Act = ListAct;

    fn static_data(_: &ToolChoice) {}

    fn data(choice: &ToolChoice, _: Entity, _: &()) -> ListInputData {
        ListInputData {
            title: "Имплант".into(),
            message: String::new(),
            init_value: choice.names.first().cloned().unwrap_or_default(),
            items: choice.names.clone(),
            timeout: None,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ListAct, UiReject> {
        match action {
            "submit" => params
                .get("entry")
                .map(|e| ListAct::Submit(e.to_owned()))
                .ok_or_else(|| UiReject::BadParams("entry: expected an item".into())),
            "cancel" => Ok(ListAct::Cancel),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

pub(super) fn build(app: &mut App) {
    app.register_ui::<ToolListWindow>()
        .add_observer(implant_items)
        .add_systems(
            Update,
            (
                (
                    trigger_buttons.run_if(resource_exists::<Messages<UiActed<ActionButtonsUi>>>),
                    pick_tool,
                )
                    .in_set(UiSystems::Handle),
                close_stale_lists.in_set(UiSystems::StatusHook),
                (implants_life, rebooters).before(crate::health::HealthSystems),
                implants_emp
                    .in_set(EmpSystems::Effects)
                    .run_if(resource_exists::<Messages<EmpAct>>),
            ),
        );
}

/// Implant and autosurgeon items get their state when they spawn (mapped, from crates, built).
fn implant_items(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    let entity = add.entity;
    if let Some(implant) = Implant::new(&item.path) {
        commands.entity(entity).try_insert_if_new(implant);
        return;
    }
    let Some((state, start)) = autosurgeon_of(&item.path) else {
        return;
    };
    commands.entity(entity).try_insert_if_new(state);
    if let Some(start) = start {
        commands.queue(move |world: &mut World| {
            if world.get_entity(entity).is_err() || loaded(world, entity).is_some() {
                return;
            }
            if let Some(organ) = spawn_any(world, start) {
                world.entity_mut(organ).insert(LoadedIn(entity));
            }
        });
    }
}

/// A new implant of type `path` outside the world; a bare entity without its prototype.
pub fn spawn_implant(world: &mut World, path: &str) -> Option<Entity> {
    let implant = Implant::new(path)?;
    let entity = crate::construction::spawn_detached(world, path)
        .unwrap_or_else(|| world.spawn(Name::new("имплант")).id());
    world.entity_mut(entity).insert(implant);
    Some(entity)
}

/// An organ or an implant of `path`, outside the world.
fn spawn_any(world: &mut World, path: &str) -> Option<Entity> {
    spawn_implant(world, path).or_else(|| organs::spawn_organ(world, path))
}

/// The implant in `slot` of `mob`.
pub fn implant_in(world: &World, mob: Entity, slot: ImplantSlot) -> Option<Entity> {
    world.get::<Organs>(mob)?.iter().find(|&e| {
        world
            .get::<Implant>(e)
            .is_some_and(|i| i.slot == slot && world.get::<OrganOf>(e).is_some())
    })
}

fn owner(world: &World, implant: Entity) -> Option<Entity> {
    world.get::<OrganOf>(implant).map(|o| o.0)
}

fn tell_owner(world: &mut World, mob: Entity, span: Span, text: impl Into<String>) {
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        tell(world, client, span, text);
    }
}

/// tg `Insert` of an implant into `slot` of `mob` (anywhere before: a hand, an autosurgeon, the
/// floor); an implant already there drops to the floor. False if `mob` cannot take organs.
pub fn insert(world: &mut World, implant: Entity, mob: Entity, slot: ImplantSlot) -> bool {
    if world.get::<OrganHost>(mob).is_none() || world.get::<Implant>(implant).is_none() {
        return false;
    }
    if let Some(old) = implant_in(world, mob, slot) {
        remove(world, old);
        if let Some(&pos) = world.get::<TilePos>(mob) {
            crate::storage::take_out(world, old, pos);
        }
    }
    world
        .entity_mut(implant)
        .remove::<(
            TilePos,
            crate::storage::StoredIn,
            CarriedBy,
            LoadedIn,
            Networked,
        )>()
        .insert(OrganOf(mob));
    if let Some(mut state) = world.get_mut::<Implant>(implant) {
        state.slot = slot;
    }
    refresh_buttons(world, mob);
    true
}

/// tg `Remove`: out of its owner, nowhere yet (the caller places it); an extended tool comes
/// back, the anti-drop lets go, the rebooter's resistance ends.
pub fn remove(world: &mut World, implant: Entity) {
    let Some(mob) = owner(world, implant) else {
        return;
    };
    retract(world, implant);
    release(world, implant);
    world.entity_mut(implant).remove::<OrganOf>();
    if let Some(mut state) = world.get_mut::<Implant>(implant) {
        state.timer = None;
        state.buff_until = 0.0;
        let hud = matches!(state.kind, Kind::Hud { .. });
        state.active &= hud;
    }
    refresh_buttons(world, mob);
}

/// The data HUDs the mob's HUD implant shows: (security, medical).
pub fn implant_huds(world: &World, mob: Entity) -> (bool, bool) {
    implant_in(world, mob, ImplantSlot::Hud)
        .and_then(|i| world.get::<Implant>(i))
        .filter(|i| i.active && !i.failing)
        .map_or((false, false), |i| match i.kind {
            Kind::Hud { security, medical } => (security, medical),
            _ => (false, false),
        })
}

/// tg `can_breathe_tube()`: a breathing tube implant.
pub fn breathing_tube(world: &World, mob: Entity) -> bool {
    implant_in(world, mob, ImplantSlot::BreathingTube).is_some()
}

// --- Action buttons ---

/// The mob's implant buttons after the others it has.
fn refresh_buttons(world: &mut World, mob: Entity) {
    if world.get::<PlayerMob>(mob).is_none() {
        return;
    }
    let mut buttons: Vec<(ActionButton, f64)> = world
        .get::<ActionButtons>(mob)
        .map(|b| {
            b.0.iter()
                .filter(|(b, _)| !b.id.starts_with(BUTTON))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    for slot in ImplantSlot::ALL {
        let Some(implant) = implant_in(world, mob, slot) else {
            continue;
        };
        let has_action = world.get::<Implant>(implant).is_some_and(|i| {
            matches!(
                i.kind,
                Kind::Toolkit { .. } | Kind::AntiDrop | Kind::Hud { .. }
            )
        });
        if has_action {
            buttons.push((button(world, implant, slot), 0.0));
        }
    }
    if buttons.is_empty() && world.get::<ActionButtons>(mob).is_none() {
        return;
    }
    set_buttons(world, mob, buttons);
}

/// tg `/datum/action/item_action/organ_action`: the implant's own icon.
fn button(world: &World, implant: Entity, slot: ImplantSlot) -> ActionButton {
    let icons = world.get_resource::<IconIndex>();
    let icon = world
        .get::<DmAtom>(implant)
        .and_then(|a| icons.and_then(|i| i.get(&a.icon, &a.icon_state)));
    ActionButton {
        id: format!("{BUTTON}{}", slot.index()),
        name: name_of(world, implant),
        icon,
        background: icons.and_then(|i| i.get(BUTTON_BACKGROUNDS, "bg_default")),
        border: None,
        available: true,
        cooldown: 0.0,
        active: false,
        maptext: None,
    }
}

/// A click on an implant button, checked at the moment of effect: the client controls the
/// conscious mob (`AB_CHECK_CONSCIOUS`) and the implant is still in it.
fn trigger_buttons(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .filter(|a| a.act.starts_with(BUTTON))
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world.get::<Stat>(actor) != Some(&Stat::Conscious)
        {
            continue;
        }
        let Some(slot) = id
            .strip_prefix(BUTTON)
            .and_then(|n| n.parse::<usize>().ok())
            .and_then(|n| ImplantSlot::ALL.get(n).copied())
        else {
            continue;
        };
        if let Some(implant) = implant_in(world, actor, slot) {
            ui_action_click(world, client, actor, implant);
        }
    }
}

/// tg `ui_action_click` of the implant.
pub fn ui_action_click(world: &mut World, client: Entity, mob: Entity, implant: Entity) {
    let Some(kind) = world.get::<Implant>(implant).map(|i| i.kind) else {
        return;
    };
    match kind {
        Kind::Toolkit { .. } => toggle_toolkit(world, client, mob, implant),
        Kind::AntiDrop => toggle_anti_drop(world, client, mob, implant),
        Kind::Hud { .. } => {
            let on = world.get_mut::<Implant>(implant).map(|mut i| {
                i.active = !i.active;
                i.active
            });
            let text = if on == Some(true) {
                "HUD включён"
            } else {
                "HUD выключен"
            };
            tell(world, client, Span::Notice, text);
        }
        _ => {}
    }
}

// --- Toolkits ---

fn arm_words(slot: ImplantSlot) -> (&'static str, &'static str) {
    if slot == ImplantSlot::LeftArm {
        ("левой руки", "левую руку")
    } else {
        ("правой руки", "правую руку")
    }
}

/// The toolkit's `Initialize`: its tools made inside it.
fn stock(world: &mut World, implant: Entity) {
    let Some(Kind::Toolkit { items, .. }) = world
        .get::<Implant>(implant)
        .filter(|i| !i.stocked)
        .map(|i| i.kind)
    else {
        return;
    };
    for path in items {
        if let Some(tool) = crate::construction::spawn_detached(world, path) {
            world.entity_mut(tool).insert(ToolOf(implant));
        }
    }
    if let Some(mut state) = world.get_mut::<Implant>(implant) {
        state.stocked = true;
    }
}

fn tools_of(world: &World, implant: Entity) -> Vec<Entity> {
    world
        .get::<ImplantTools>(implant)
        .map(|t| t.0.clone())
        .unwrap_or_default()
}

/// The toolkit's `ui_action_click`: retract the tool out, else extend the only one or list them.
fn toggle_toolkit(world: &mut World, client: Entity, mob: Entity, implant: Entity) {
    stock(world, implant);
    let Some(state) = world.get::<Implant>(implant).cloned() else {
        return;
    };
    if state.extended.is_some() {
        retract(world, implant);
        return;
    }
    let tools = tools_of(world, implant);
    if state.failing || tools.is_empty() {
        tell(
            world,
            client,
            Span::Warning,
            "Имплант не реагирует. Кажется, он сломан...",
        );
        return;
    }
    if let [tool] = tools.as_slice() {
        extend(world, mob, implant, *tool);
        return;
    }
    let names = tools.iter().map(|&t| name_of(world, t)).collect();
    world.entity_mut(mob).insert(ToolChoice {
        implant,
        tools,
        names,
    });
    world.write_message(OpenUi::new::<ToolListWindow>(client, mob, mob, "Имплант"));
}

/// tg `Extend(augment)`: the tool into the arm's hand, what it held dropped first.
pub fn extend(world: &mut World, mob: Entity, implant: Entity, tool: Entity) -> bool {
    let Some(slot) = world.get::<Implant>(implant).map(|i| i.slot) else {
        return false;
    };
    let (Some(hand), Some(&pos)) = (slot.hand(), world.get::<TilePos>(mob)) else {
        return false;
    };
    if world.get::<ToolOf>(tool).map(|t| t.0) != Some(implant)
        || world.get::<CarriedBy>(tool).is_some()
        || !super::bodyparts::has_hand(world, mob, hand)
    {
        return false;
    }
    let (implant_name, tool_name) = (name_of(world, implant), name_of(world, tool));
    let held = world.get::<Inventory>(mob).and_then(|i| i.hands[hand]);
    if let Some(held) = held {
        let held_name = name_of(world, held);
        if crate::inventory::no_drop(world, held) {
            tell_owner(
                world,
                mob,
                Span::Warning,
                format!("{held_name} мешает {implant_name}!"),
            );
            return false;
        }
        crate::inventory::put_down(world, held, pos);
        tell_owner(
            world,
            mob,
            Span::Notice,
            format!("Вы бросаете {held_name}, чтобы активировать {implant_name}!"),
        );
    }
    world
        .entity_mut(tool)
        .remove::<TilePos>()
        .insert((Networked, CarriedBy(mob), NoDrop));
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.hands[hand] = Some(tool);
    }
    if let Some(mut state) = world.get_mut::<Implant>(implant) {
        state.extended = Some(tool);
    }
    let user = name_of(world, mob);
    let arm = arm_words(slot).0;
    visible(
        world,
        mob,
        &format!("Вы выдвигаете {tool_name} из своей {arm}."),
        &format!("{user} выдвигает {tool_name} из своей {arm}."),
        Span::Notice,
    );
    true
}

/// tg `Retract()`: the extended tool back inside. True if one was out.
pub fn retract(world: &mut World, implant: Entity) -> bool {
    let Some(state) = world.get::<Implant>(implant).cloned() else {
        return false;
    };
    let Some(tool) = state.extended else {
        return false;
    };
    if let Some(mut s) = world.get_mut::<Implant>(implant) {
        s.extended = None;
    }
    if world.get_entity(tool).is_err() {
        return true;
    }
    world
        .entity_mut(tool)
        .remove::<(CarriedBy, NoDrop, TilePos, Networked)>();
    if let Some(mob) = owner(world, implant) {
        let (user, tool_name) = (name_of(world, mob), name_of(world, tool));
        let arm = arm_words(state.slot).1;
        visible(
            world,
            mob,
            &format!("{tool_name} возвращается в вашу {arm}."),
            &format!("{user} убирает {tool_name} обратно в свою {arm}."),
            Span::Notice,
        );
    }
    true
}

/// The owner picked a tool from the list (or closed it).
fn pick_tool(world: &mut World, mut acts: Local<MessageCursor<UiActed<ToolListWindow>>>) {
    let picked: Vec<(Entity, ListAct)> = acts
        .read(world.resource::<Messages<UiActed<ToolListWindow>>>())
        .filter(|a| a.actor == a.source)
        .map(|a| (a.actor, a.act.clone()))
        .collect();
    for (mob, act) in picked {
        let Some(choice) = world.get::<ToolChoice>(mob).cloned() else {
            continue;
        };
        world.entity_mut(mob).remove::<ToolChoice>();
        let ListAct::Submit(entry) = act else {
            continue;
        };
        let Some(i) = choice.names.iter().position(|n| *n == entry) else {
            continue;
        };
        // tg: the owner alive, the implant still in it, nothing extended meanwhile.
        let still = owner(world, choice.implant) == Some(mob)
            && world.get::<Stat>(mob) != Some(&Stat::Dead)
            && world
                .get::<Implant>(choice.implant)
                .is_some_and(|s| s.extended.is_none());
        if still {
            extend(world, mob, choice.implant, choice.tools[i]);
        }
    }
}

fn close_stale_lists(
    mut windows: Query<&mut UiWindow, With<ss13_tgui::server::UiKind<ToolListWindow>>>,
    choices: Query<(), With<ToolChoice>>,
) {
    for mut window in &mut windows {
        if !choices.contains(window.source) {
            window.cap_status(UiStatus::Close);
        }
    }
}

// --- Anti-drop ---

fn hand_words(hand: usize) -> &'static str {
    if hand == 0 {
        "левой руки"
    } else {
        "правой руки"
    }
}

/// The anti-drop's `ui_action_click`: the grip on what the hands hold tightens or relaxes.
fn toggle_anti_drop(world: &mut World, client: Entity, mob: Entity, implant: Entity) {
    if world.get::<Implant>(implant).is_some_and(|i| i.active) {
        release(world, implant);
        tell(world, client, Span::Notice, "Ваши руки расслабляются...");
        return;
    }
    let hands = world
        .get::<Inventory>(mob)
        .map(|i| i.hands)
        .unwrap_or_default();
    // Items stuck by something else stay that thing's.
    let held: Vec<(usize, Entity)> = hands
        .iter()
        .enumerate()
        .filter_map(|(i, h)| h.map(|e| (i, e)))
        .filter(|&(_, e)| !crate::inventory::no_drop(world, e))
        .collect();
    if held.is_empty() {
        tell(
            world,
            client,
            Span::Notice,
            "Вы ничего не держите, ваши руки расслабляются...",
        );
        return;
    }
    for &(hand, item) in &held {
        world.entity_mut(item).insert(NoDrop);
        tell(
            world,
            client,
            Span::Notice,
            format!("Хватка вашей {} крепнет.", hand_words(hand)),
        );
    }
    if let Some(mut state) = world.get_mut::<Implant>(implant) {
        state.active = true;
        state.held = held.into_iter().map(|(_, e)| e).collect();
    }
}

/// The anti-drop's `release_items`.
fn release(world: &mut World, implant: Entity) -> Vec<Entity> {
    let Some(mut state) = world.get::<Implant>(implant).cloned() else {
        return Vec::new();
    };
    if state.kind != Kind::AntiDrop || (!state.active && state.held.is_empty()) {
        return Vec::new();
    }
    let held = std::mem::take(&mut state.held);
    state.active = false;
    world.entity_mut(implant).insert(state);
    for &item in &held {
        if let Ok(mut entity) = world.get_entity_mut(item) {
            entity.remove::<NoDrop>();
        }
    }
    held
}

// --- Life ---

/// The implants' `on_life` (`on_death` for the reviver) each `SSmobs` fire.
// Cost: every 2 s one pass over implants inside bodies (a few per round).
fn implants_life(world: &mut World, mut last_fired: Local<u64>) {
    let fired = world.resource::<crate::breath::MobsClock>().times_fired;
    let fires = fired.saturating_sub(*last_fired);
    *last_fired = fired;
    if fires == 0 {
        return;
    }
    let secs = crate::breath::SSMOBS_WAIT_SECS as f32;
    let now = world.resource::<Time>().elapsed_secs_f64();
    let implants: Vec<(Entity, Entity)> = world
        .query::<(Entity, &Implant, &OrganOf)>()
        .iter(world)
        .filter(|(_, i, _)| matches!(i.kind, Kind::Nutriment { .. } | Kind::Reviver))
        .map(|(e, _, o)| (e, o.0))
        .collect();
    for _ in 0..fires {
        for &(implant, mob) in &implants {
            life(world, implant, mob, secs, now);
        }
    }
}

/// One `SSmobs` fire of `implant` in `mob`.
pub fn life(world: &mut World, implant: Entity, mob: Entity, secs: f32, now: f64) {
    let Some(kind) = world.get::<Implant>(implant).map(|i| i.kind) else {
        return;
    };
    let dead = world.get::<Stat>(mob) == Some(&Stat::Dead);
    match kind {
        Kind::Nutriment {
            threshold,
            black_market,
            ..
        } if !dead => nutriment(world, implant, mob, threshold, black_market, secs, now),
        Kind::Reviver => reviver(world, implant, mob, now),
        _ => {}
    }
}

/// The nutriment pump's `on_life`: at or below its threshold it feeds 25 a second, then cools
/// down 5 s; the black market one keeps a fat owner overeating.
fn nutriment(
    world: &mut World,
    implant: Entity,
    mob: Entity,
    threshold: f32,
    black_market: bool,
    secs: f32,
    now: f64,
) {
    use crate::hunger::{Hunger, NUTRITION_LEVEL_FAT, NUTRITION_LEVEL_FED, OVEREAT_TIME_LIMIT};
    let Some(mut hunger) = world.get::<Hunger>(mob).cloned() else {
        return;
    };
    let ready = world
        .get::<Implant>(implant)
        .is_some_and(|i| now >= i.cooldown_until);
    if ready && hunger.nutrition <= threshold {
        let text = if hunger.nutrition >= NUTRITION_LEVEL_FED {
            "Вы чувствуете сильную сытость..."
        } else {
            "Голод понемногу отступает..."
        };
        tell_owner(world, mob, Span::Notice, text);
        hunger.adjust(PUMP_FEED * secs);
        if let Some(mut state) = world.get_mut::<Implant>(implant) {
            state.cooldown_until = now + PUMP_COOLDOWN;
        }
    }
    if black_market
        && hunger.nutrition >= NUTRITION_LEVEL_FAT
        && hunger.overeat_secs <= OVEREAT_TIME_LIMIT
    {
        hunger.overeat_secs = (hunger.overeat_secs + 40.0).min(OVEREAT_TIME_LIMIT);
    }
    if world.get::<Hunger>(mob) != Some(&hunger) {
        world.entity_mut(mob).insert(hunger);
    }
}

/// `IS_UNCONSCIOUS_OR_CRIT`.
fn down(world: &World, mob: Entity) -> bool {
    let stat = world.get::<Stat>(mob).copied().unwrap_or_default();
    stat != Stat::Conscious
        || world
            .get::<Health>(mob)
            .is_some_and(|h| h.knocked_out(stat))
}

/// The reviver's `try_heal`: an owner down in crit (or dead) starts it; while it lasts it heals
/// each fire; once the owner is up it recharges for what the healing cost.
// tg heals 3 s after each `on_life`; here at the fire itself.
fn reviver(world: &mut World, implant: Entity, mob: Entity, now: f64) {
    let Some(mut state) = world.get::<Implant>(implant).cloned() else {
        return;
    };
    let is_down = down(world, mob);
    if state.active {
        if !is_down {
            state.cooldown_until = now + state.revive_cost;
            state.active = false;
            let secs = state.revive_cost.round();
            world.entity_mut(implant).insert(state);
            tell_owner(
                world,
                mob,
                Span::Notice,
                format!(
                    "Ваш имплант-реаниматор отключается и начинает перезарядку. Он снова будет \
                     готов через {secs} с."
                ),
            );
        } else {
            reviver_heal(world, implant, mob, now);
        }
        return;
    }
    if now < state.cooldown_until || !is_down {
        return;
    }
    state.revive_cost = 0.0;
    state.active = true;
    state.timer = Some(now + REVIVER_DEFIB_DELAY);
    world.entity_mut(implant).insert(state);
    tell_owner(
        world,
        mob,
        Span::Notice,
        "Вы чувствуете слабое жужжание: имплант-реаниматор начинает латать ваши раны...",
    );
}

/// The reviver's `heal`: a revival when it is due, then 5 oxygen, 2 brute, 2 burn and 1 toxin
/// damage healed, each adding to the recharge.
fn reviver_heal(world: &mut World, implant: Entity, mob: Entity, now: f64) {
    revive_dead(world, implant, mob, now);
    let Some(mut health) = world.get::<Health>(mob).cloned() else {
        return;
    };
    let mut cost = 0.0;
    let mut patched = false;
    if health.oxy > 0.0 {
        health.adjust(DamageType::Oxy, -5.0);
        cost += 0.5;
    }
    if health.brute > 0.0 {
        health.adjust(DamageType::Brute, -2.0);
        cost += 4.0;
        patched = true;
    }
    if health.burn > 0.0 {
        health.adjust(DamageType::Burn, -2.0);
        cost += 4.0;
        patched = true;
    }
    if health.tox > 0.0 {
        health.adjust(DamageType::Tox, -1.0);
        cost += 4.0;
    }
    if let Some(mut stat) = world.get::<Stat>(mob).copied() {
        crate::health::update_stat(&health, &mut stat);
        world.entity_mut(mob).insert(stat);
    }
    world.entity_mut(mob).insert(health);
    if let Some(mut state) = world.get_mut::<Implant>(implant) {
        state.revive_cost += cost;
    }
    if patched && super::fork_rng(world).chance(35.0) {
        let user = name_of(world, mob);
        visible(
            world,
            mob,
            "Вы чувствуете, как что-то латает ваше израненное тело.",
            &format!("Тело {user} слегка подёргивается."),
            Span::Warning,
        );
    }
}

/// The reviver's `revive_dead`: a corpse that a defibrillator could bring back is shocked back
/// to life (+10 minutes of recharge, the next try 16 s later).
fn revive_dead(world: &mut World, implant: Entity, mob: Entity, now: f64) {
    let Some(mut state) = world.get::<Implant>(implant).cloned() else {
        return;
    };
    let due = state.timer.is_none_or(|t| now >= t);
    if !due || world.get::<Stat>(mob) != Some(&Stat::Dead) {
        return;
    }
    let Some(health) = world.get::<Health>(mob).cloned() else {
        return;
    };
    let heart = super::defib::vital_organ(world, mob, Slot::Heart);
    let brain = super::defib::vital_organ(world, mob, Slot::Brain);
    if super::defib::can_defib(&health, heart, brain).is_err() {
        return;
    }
    state.revive_cost += REVIVER_REVIVE_COST;
    state.timer = Some(state.timer.unwrap_or(now) + REVIVER_DEFIB_AGAIN);
    world.entity_mut(implant).insert(state);
    let user = name_of(world, mob);
    visible(
        world,
        mob,
        "Имплант-реаниматор возвращает вас к жизни!",
        &format!("Тело {user} содрогается в конвульсиях."),
        Span::Warning,
    );
    crate::ghost::return_to_body(world, mob);
    organs::set_heartattack(world, mob, false);
    super::defib::revive(world, mob);
}

/// The CNS rebooters each tick: a stun clears a second after it lands, then 6 s of stun
/// resistance, once a minute; an EMP failure reboots by itself.
// Cost: per tick, one pass over rebooters inside bodies (a handful per round), two component
// reads each; tg reacts to stun signals instead.
fn rebooters(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let rebooters: Vec<(Entity, Entity)> = world
        .query::<(Entity, &Implant, &OrganOf)>()
        .iter(world)
        .filter(|(_, i, _)| i.kind == Kind::AntiStun)
        .map(|(e, _, o)| (e, o.0))
        .collect();
    for (implant, mob) in rebooters {
        rebooter_tick(world, implant, mob, now);
    }
}

fn stuns(effects: &StatusEffects) -> bool {
    effects.stun > 0.0
        || effects.knockdown > 0.0
        || effects.paralyze > 0.0
        || effects.immobilize > 0.0
}

/// One tick of a rebooter in `mob` at `now`.
pub fn rebooter_tick(world: &mut World, implant: Entity, mob: Entity, now: f64) {
    let Some(mut state) = world.get::<Implant>(implant).cloned() else {
        return;
    };
    let before = state.clone();
    let mut ready = false;
    if state.failing && state.reboot_at.is_some_and(|t| now >= t) {
        state.failing = false;
        state.reboot_at = None;
        ready = true;
    }
    if state.notify_ready && now >= state.cooldown_until {
        state.notify_ready = false;
        ready = true;
    }
    // tg reboots on `COMSIG_LIVING_STATUS_STUN`, `_IMMOBILIZE`, `_PARALYZE` and stamcrit.
    let effects = world.get::<StatusEffects>(mob);
    let stunned = effects
        .is_some_and(|e| e.stun > 0.0 || e.paralyze > 0.0 || e.immobilize > 0.0 || e.stamcrit);
    let knocked = effects.is_some_and(stuns);
    if now < state.buff_until {
        // `TRAIT_STUNIMMUNE`: nothing new holds.
        if knocked {
            clear_stuns(world, mob, false);
        }
    } else if !state.failing && stunned && state.timer.is_none() {
        state.timer = Some(now + REBOOT_DELAY);
    }
    if state.timer.is_some_and(|t| now >= t) {
        state.timer = None;
        if !state.failing && now >= state.cooldown_until {
            clear_stuns(world, mob, true);
            state.buff_until = now + STUN_RESISTANCE_SECS;
            state.cooldown_until = now + REBOOTER_COOLDOWN;
            state.notify_ready = true;
        }
    }
    if state != before {
        world.entity_mut(implant).insert(state);
    }
    if ready {
        tell_owner(
            world,
            mob,
            Span::Notice,
            "Ваш имплант-перезагрузчик готов к работе.",
        );
    }
}

/// `clear_stuns`: `SetStun(0)`, `SetKnockdown(0)`, `SetImmobilized(0)`, `SetParalyzed(0)` and,
/// on the reboot itself, `set_stamina_loss(0)`.
fn clear_stuns(world: &mut World, mob: Entity, stamina: bool) {
    if world.get::<StatusEffects>(mob).is_none() {
        return;
    }
    if let Some(mut effects) = crate::status_effects::effects_mut(world, mob) {
        for kind in [
            Incap::Stun,
            Incap::Knockdown,
            Incap::Paralyze,
            Incap::Immobilize,
        ] {
            if effects.remaining(kind) > 0.0 {
                effects.set(kind, 0.0);
            }
        }
        if stamina {
            effects.stamina = 0.0;
            effects.stamcrit = false;
        }
    }
}

// --- EMP ---

fn implants_emp(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    for ev in emp_acts(world, &mut cursor) {
        if !ev.hits_self() || world.get::<Implant>(ev.target).is_none() {
            continue;
        }
        let Some(mob) = owner(world, ev.target) else {
            continue;
        };
        emp(world, ev.target, mob, f64::from(ev.severity), now);
    }
}

fn prob(world: &mut World, percent: f64) -> bool {
    world.get_resource_or_init::<EmpRng>().prob(percent)
}

/// The implants' `emp_act` at `severity` (1 heavy, 2 light).
pub fn emp(world: &mut World, implant: Entity, mob: Entity, severity: f64, now: f64) {
    let Some(state) = world.get::<Implant>(implant).cloned() else {
        return;
    };
    // `/obj/item/organ/cyberimp/brain/emp_act`: `emp_stun_duration` 20 s (the surgical
    // processor immobilizes for 4 s instead).
    if state.slot.brain() {
        if state.slot == ImplantSlot::Hippocampus {
            crate::status_effects::apply(world, mob, Incap::Immobilize, (4.0 / severity) as f32);
        } else {
            crate::status_effects::apply(world, mob, Incap::Stun, (20.0 / severity) as f32);
            tell_owner(world, mob, Span::Warning, "Ваше тело сводит судорогой!");
        }
    }
    match state.kind {
        Kind::AntiDrop => throw_held(world, implant, mob, severity),
        Kind::AntiStun if !state.failing => {
            if let Some(mut s) = world.get_mut::<Implant>(implant) {
                s.failing = true;
                s.reboot_at = Some(now + REBOOTER_EMP_SECS / severity);
                s.timer = None;
            }
        }
        Kind::Nutriment { poison, .. } => {
            add_blood_reagent(world, mob, BAD_FOOD, (poison / severity as f32).max(0.0));
            tell_owner(
                world,
                mob,
                Span::Warning,
                "Вы чувствуете, как горят ваши внутренности.",
            );
        }
        Kind::Reviver => reviver_emp(world, implant, mob, severity, now),
        Kind::BreathingTube => {
            if prob(world, 60.0 / severity) {
                tell_owner(
                    world,
                    mob,
                    Span::Warning,
                    "Ваша дыхательная трубка внезапно закрывается!",
                );
                if let Some(mut r) = world.get_mut::<crate::breath::Respiration>(mob) {
                    r.losebreath += 2.0;
                }
            }
        }
        Kind::Toolkit { gun, .. } => {
            if prob(world, 15.0 / severity) {
                let name = name_of(world, implant);
                tell_owner(
                    world,
                    mob,
                    Span::Warning,
                    format!("Электромагнитный импульс вызывает сбой {name}!"),
                );
                retract(world, implant);
            }
            if gun && !state.failing && prob(world, 30.0 / severity) {
                gun_blowout(world, implant, mob, state.slot);
            }
        }
        _ => {}
    }
}

/// The anti-drop's EMP: the hands let go and throw what they held up to `10 / severity` tiles.
// tg empties `stored_items` in `release_items` before its throwing loop, so nothing is ever
// thrown; ported as meant.
fn throw_held(world: &mut World, implant: Entity, mob: Entity, severity: f64) {
    let held = release(world, implant);
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    let range = (10.0 / severity) as i32;
    for item in held {
        let hand = world
            .get::<Inventory>(mob)
            .and_then(|i| i.slot_of(item))
            .filter(|s| matches!(s, InventorySlot::LeftHand | InventorySlot::RightHand))
            .map(|s| usize::from(s == InventorySlot::RightHand));
        let Some(hand) = hand else {
            continue;
        };
        let name = name_of(world, item);
        crate::inventory::put_down(world, item, pos);
        let (dx, dy) = {
            let mut rng = world.get_resource_or_init::<EmpRng>();
            (
                rng.range(-i64::from(range), i64::from(range)) as i32,
                rng.range(-i64::from(range), i64::from(range)) as i32,
            )
        };
        let target = TilePos {
            x: pos.x + dx,
            y: pos.y + dy,
            ..pos
        };
        crate::throw::throw_at(world, item, target, range.max(1) as u32, 2.0);
        tell_owner(
            world,
            mob,
            Span::Warning,
            format!(
                "Ваша {} сокращается и швыряет {name}!",
                if hand == 0 {
                    "левая рука"
                } else {
                    "правая рука"
                }
            ),
        );
    }
}

/// The reviver's EMP: a longer recharge, and maybe a heart attack for `50 s / severity`.
// tg adds 20 s to the `reviver_cooldown` timestamp, which does nothing once it is in the past;
// the recharge runs from now here.
fn reviver_emp(world: &mut World, implant: Entity, mob: Entity, severity: f64, now: f64) {
    if let Some(mut state) = world.get_mut::<Implant>(implant) {
        if state.active {
            state.revive_cost += 20.0 / severity;
        } else {
            state.cooldown_until = state.cooldown_until.max(now) + 20.0 / severity;
        }
    }
    let alive = world.get::<Stat>(mob).is_some_and(|s| *s != Stat::Dead);
    if !alive || !prob(world, 50.0 / severity) || !organs::set_heartattack(world, mob, true) {
        return;
    }
    tell_owner(
        world,
        mob,
        Span::UserDanger,
        "Вы чувствуете ужасную боль в груди!",
    );
    if let Some(heart) = organs::organ(world, mob, Slot::Heart) {
        world
            .entity_mut(heart)
            .insert(crate::emp::mobs::HeartRestart(
                now + REVIVER_HEART_ATTACK / severity,
            ));
    }
}

/// The arm cannon's EMP: the cannon retracts and blows up inside the arm (fire, 25 burn), the
/// implant broken.
fn gun_blowout(world: &mut World, implant: Entity, mob: Entity, slot: ImplantSlot) {
    retract(world, implant);
    if let Some(mut state) = world.get_mut::<Implant>(implant) {
        state.failing = true;
    }
    let user = name_of(world, mob);
    let arm = arm_words(slot).0;
    visible(
        world,
        mob,
        &format!("Вы чувствуете взрыв внутри своей {arm}: имплант ломается!"),
        &format!("Громкий хлопок раздаётся из {arm} {user}!"),
        Span::Danger,
    );
    crate::fire::adjust_fire_stacks(world, mob, 20.0);
    crate::fire::ignite_mob(world, mob);
    let zone = if slot == ImplantSlot::LeftArm {
        BodyZone::LArm
    } else {
        BodyZone::RArm
    };
    super::hurt_limb(world, mob, zone, DamageType::Burn, 25.0);
}

/// `owner.reagents.add_reagent(path, amount)`.
fn add_blood_reagent(world: &mut World, mob: Entity, path: &str, amount: f32) {
    if let (Some(chem), Some(mut reagents)) = (
        world
            .get_resource::<crate::chem::Chem>()
            .map(|c| c.0.clone()),
        world.get::<crate::chem::MobReagents>(mob).cloned(),
    ) && let Some(id) = chem.id(path)
    {
        reagents
            .blood
            .add_reagent(&chem, id, amount, 310.15, None, None, false, false);
        world.entity_mut(mob).insert(reagents);
    }
}

// --- Autosurgeon ---

/// The organ or implant loaded in `autosurgeon`.
pub fn loaded(world: &World, autosurgeon: Entity) -> Option<Entity> {
    world
        .get::<AutosurgeonLoad>(autosurgeon)
        .and_then(|l| l.0.first().copied())
}

fn is_organ(world: &World, item: Entity) -> bool {
    world.get::<Organ>(item).is_some() || world.get::<Implant>(item).is_some()
}

/// tg `attack_self` of an extended multi-tool (`swap_tools`: back in and the list again) and of
/// an autosurgeon (implants the user at once). True when `item` is one.
pub fn attack_self(world: &mut World, client: Entity, item: Entity) -> bool {
    let Some(mob) = world.get::<CarriedBy>(item).map(|c| c.0) else {
        return false;
    };
    if let Some(implant) = world.get::<ToolOf>(item).map(|t| t.0) {
        let swaps = owner(world, implant) == Some(mob) && tools_of(world, implant).len() > 1;
        if swaps {
            retract(world, implant);
            toggle_toolkit(world, client, mob, implant);
        }
        return swaps;
    }
    if world.get::<Autosurgeon>(item).is_none() {
        return false;
    }
    let actor = Actor {
        client,
        mob,
        item: Some(item),
    };
    use_autosurgeon(world, actor, item, mob, false);
    true
}

/// tg `item_interaction` of an autosurgeon in the other hand: an organ or implant loads, a
/// screwdriver takes the load out. True when it was one of these.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<Autosurgeon>(target).is_none() {
        return false;
    }
    if is_organ(world, held) {
        load(world, client, target, held);
        return true;
    }
    if world
        .get::<DmTool>(held)
        .is_some_and(|t| t.behaviour == "screwdriver")
    {
        unload(world, client, mob, target);
        return true;
    }
    false
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

/// tg `load_organ(organ, user)`.
fn load(world: &mut World, client: Entity, autosurgeon: Entity, organ: Entity) {
    let Some(state) = world.get::<Autosurgeon>(autosurgeon).cloned() else {
        return;
    };
    let name = capitalize(&name_of(world, autosurgeon));
    let text = if loaded(world, autosurgeon).is_some() {
        Some(format!("{name} уже содержит имплант."))
    } else if state.uses == Some(0) {
        Some(format!(
            "{name} достигает предела использования и не может загружать новые импланты."
        ))
    } else if let Some(root) = state.whitelist
        && !world
            .get::<DmItem>(organ)
            .is_some_and(|i| is_under(&i.path, root))
    {
        Some(format!("{name} несовместим с {}.", name_of(world, organ)))
    } else if crate::inventory::no_drop(world, organ) {
        Some(format!(
            "{} застревает в вашей руке!",
            capitalize(&name_of(world, organ))
        ))
    } else {
        None
    };
    if let Some(text) = text {
        tell(world, client, Span::Warning, text);
        return;
    }
    world
        .entity_mut(organ)
        .remove::<(CarriedBy, TilePos, Networked)>()
        .insert(LoadedIn(autosurgeon));
}

/// tg `screwdriver_act`: the load drops at the user's feet, one use spent.
fn unload(world: &mut World, client: Entity, mob: Entity, autosurgeon: Entity) {
    let Some(organ) = loaded(world, autosurgeon) else {
        let name = name_of(world, autosurgeon);
        tell(
            world,
            client,
            Span::Warning,
            format!("Внутри {name} нет имплантов!"),
        );
        return;
    };
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return;
    };
    world.entity_mut(organ).remove::<LoadedIn>();
    crate::storage::take_out(world, organ, pos);
    let (organ_name, name) = (name_of(world, organ), name_of(world, autosurgeon));
    tell(
        world,
        client,
        Span::Notice,
        format!("Вы извлекаете {organ_name} из {name}."),
    );
    spend_use(world, autosurgeon);
}

fn spend_use(world: &mut World, autosurgeon: Entity) {
    if let Some(mut state) = world.get_mut::<Autosurgeon>(autosurgeon)
        && let Some(uses) = state.uses.as_mut()
    {
        *uses = uses.saturating_sub(1);
    }
}

/// An autosurgeon in hand clicked on a living mob (tg `attack`: `use_autosurgeon` after 8 s).
pub fn autosurgeon_attack(world: &mut World, actor: Actor, target: Entity) -> bool {
    let Some(item) = actor
        .item
        .filter(|&i| world.get::<Autosurgeon>(i).is_some())
    else {
        return false;
    };
    use_autosurgeon(world, actor, item, target, true);
    true
}

/// tg `use_autosurgeon(target, user, implant_time)` up to its `do_after`.
fn use_autosurgeon(world: &mut World, actor: Actor, item: Entity, target: Entity, timed: bool) {
    let Some(state) = world.get::<Autosurgeon>(item).cloned() else {
        return;
    };
    let name = capitalize(&name_of(world, item));
    if loaded(world, item).is_none() {
        tell(
            world,
            actor.client,
            Span::Warning,
            format!("{name} в данный момент не хранит имплантов."),
        );
        return;
    }
    if state.uses == Some(0) {
        tell(
            world,
            actor.client,
            Span::Warning,
            format!(
                "{name} не имеет больше зарядов. Устройства сложены и не могут быть \
                 реактивированы."
            ),
        );
        return;
    }
    if !timed {
        finish_autosurgeon(world, actor, target);
        return;
    }
    let (user, patient, tool) = (
        name_of(world, actor.mob),
        name_of(world, target),
        name_of(world, item),
    );
    visible(
        world,
        actor.mob,
        &format!("Вы собираетесь использовать {tool} на {patient}."),
        &format!("{user} собирается использовать {tool} на {patient}."),
        Span::Notice,
    );
    do_after(
        world,
        actor,
        target,
        AUTOSURGEON_DELAY * state.speed,
        0,
        crate::construction::Job::Medical(MedJob::Autosurgeon),
    );
}

/// The rest of tg `use_autosurgeon`: the load goes into `target` (an arm implant into the
/// selected arm, else the arm of the hand holding the autosurgeon), one use spent.
pub fn finish_autosurgeon(world: &mut World, actor: Actor, target: Entity) {
    let Some(item) = actor
        .item
        .filter(|&i| world.get::<Autosurgeon>(i).is_some())
    else {
        return;
    };
    let Some(organ) = loaded(world, item) else {
        return;
    };
    if world
        .get::<Autosurgeon>(item)
        .is_some_and(|s| s.uses == Some(0))
    {
        return;
    }
    let (user, patient, tool) = (
        name_of(world, actor.mob),
        name_of(world, target),
        name_of(world, item),
    );
    let inserted = if world.get::<Implant>(organ).is_some() {
        let path = world
            .get::<DmItem>(organ)
            .map(|i| i.path.clone())
            .unwrap_or_default();
        let slot = autosurgeon_slot(world, actor.mob, item, &path, organ);
        world.entity_mut(organ).remove::<LoadedIn>();
        let done = slot.is_some_and(|slot| insert(world, organ, target, slot));
        if !done {
            world.entity_mut(organ).insert(LoadedIn(item));
        }
        done
    } else {
        world.entity_mut(organ).remove::<LoadedIn>();
        let done = organs::insert(world, organ, target);
        if !done {
            world.entity_mut(organ).insert(LoadedIn(item));
        }
        done
    };
    if !inserted {
        tell(world, actor.client, Span::Warning, "установка не удалась!");
        return;
    }
    let (own, others) = if target == actor.mob {
        (
            format!("Вы нажимаете на кнопку на {tool}, и тот врезается в ваше тело."),
            format!("{user} нажимает на кнопку на {tool}, и тот врезается в его тело."),
        )
    } else {
        (
            format!("Вы нажимаете на кнопку на {tool}, и тот врезается в тело {patient}."),
            format!("{user} нажимает на кнопку на {tool}, и тот врезается в тело {patient}."),
        )
    };
    visible(world, actor.mob, &own, &others, Span::Notice);
    spend_use(world, item);
}

/// The slot an autosurgeon puts an implant in: tg checks the selected zone, then the arm on the
/// side of the hand holding it.
fn autosurgeon_slot(
    world: &World,
    user: Entity,
    autosurgeon: Entity,
    path: &str,
    implant: Entity,
) -> Option<ImplantSlot> {
    let base = world.get::<Implant>(implant)?.slot;
    if !matches!(base, ImplantSlot::RightArm | ImplantSlot::LeftArm) {
        return Some(base);
    }
    let selected = world
        .get::<crate::health::ZoneSelected>(user)
        .copied()
        .unwrap_or_default()
        .0;
    let side = match world
        .get::<Inventory>(user)
        .and_then(|i| i.slot_of(autosurgeon))
    {
        Some(InventorySlot::LeftHand) => TargetZone::LArm,
        _ => TargetZone::RArm,
    };
    slot_for(path, selected).or_else(|| slot_for(path, side))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implant_types_take_their_slots() {
        let slot = |p: &str| classify(p).map(|(s, _)| s);
        assert_eq!(
            slot("/obj/item/organ/cyberimp/chest/reviver"),
            Some(ImplantSlot::HeartAid)
        );
        assert_eq!(
            slot("/obj/item/organ/cyberimp/brain/anti_stun"),
            Some(ImplantSlot::Cns)
        );
        assert_eq!(slot("/obj/item/organ/cyberimp/eyes"), None);
        assert_eq!(slot("/obj/item/organ/heart"), None);
        let (_, kind) = classify("/obj/item/organ/cyberimp/arm/toolkit/surgery/emagged").unwrap();
        assert!(matches!(kind, Kind::Toolkit { items, gun: false } if items.len() == 8));
        let (_, kind) = classify("/obj/item/organ/cyberimp/arm/toolkit/gun/laser").unwrap();
        assert!(matches!(kind, Kind::Toolkit { gun: true, .. }));
        let (_, kind) = classify("/obj/item/organ/cyberimp/eyes/hud/security/syndicate").unwrap();
        assert_eq!(
            kind,
            Kind::Hud {
                security: true,
                medical: false
            }
        );
        // Arm implants go into either arm, nothing else leaves its zone.
        let arm = "/obj/item/organ/cyberimp/arm/toolkit/toolset";
        assert_eq!(slot_for(arm, TargetZone::LArm), Some(ImplantSlot::LeftArm));
        assert_eq!(slot_for(arm, TargetZone::Chest), None);
        let tube = "/obj/item/organ/cyberimp/mouth/breathing_tube";
        assert_eq!(
            slot_for(tube, TargetZone::Mouth),
            Some(ImplantSlot::BreathingTube)
        );
        assert_eq!(slot_for(tube, TargetZone::Head), None);
    }

    #[test]
    fn autosurgeon_types_start_loaded() {
        let (state, start) =
            autosurgeon_of("/obj/item/autosurgeon/syndicate/reviver/single_use").unwrap();
        assert_eq!(start, Some("/obj/item/organ/cyberimp/chest/reviver"));
        assert_eq!((state.uses, state.speed), (Some(1), 0.75));
        let (state, start) = autosurgeon_of("/obj/item/autosurgeon").unwrap();
        assert_eq!((state.uses, start), (None, None));
        let (state, _) = autosurgeon_of("/obj/item/autosurgeon/syndicate/commsagent").unwrap();
        assert_eq!(state.whitelist, Some("/obj/item/organ/tongue"));
        assert!(autosurgeon_of("/obj/item/scalpel").is_none());
    }

    /// A conscious human with organs and no prototypes.
    fn human(world: &mut World) -> Entity {
        world.init_resource::<Time>();
        world
            .spawn((Health::default(), Stat::Conscious, OrganHost))
            .id()
    }

    #[test]
    fn implants_go_in_and_out_by_slot() {
        let mut world = World::new();
        let mob = human(&mut world);
        let tube =
            spawn_implant(&mut world, "/obj/item/organ/cyberimp/mouth/breathing_tube").unwrap();
        assert!(!breathing_tube(&world, mob));
        assert!(insert(&mut world, tube, mob, ImplantSlot::BreathingTube));
        assert!(breathing_tube(&world, mob));
        // Organ code does not see it.
        assert!(organs::organ(&world, mob, Slot::Tongue).is_none());
        remove(&mut world, tube);
        assert!(!breathing_tube(&world, mob));
        // A HUD implant shows its HUD until toggled off.
        let hud = spawn_implant(&mut world, "/obj/item/organ/cyberimp/eyes/hud/medical").unwrap();
        insert(&mut world, hud, mob, ImplantSlot::Hud);
        assert_eq!(implant_huds(&world, mob), (false, true));
        world.get_mut::<Implant>(hud).unwrap().active = false;
        assert_eq!(implant_huds(&world, mob), (false, false));
    }

    #[test]
    fn nutriment_pump_feeds_the_starving() {
        use crate::hunger::Hunger;
        let mut world = World::new();
        let mob = human(&mut world);
        world.entity_mut(mob).insert(Hunger::new(100.0));
        let pump = spawn_implant(&mut world, "/obj/item/organ/cyberimp/chest/nutriment").unwrap();
        insert(&mut world, pump, mob, ImplantSlot::StomachAid);
        life(&mut world, pump, mob, 2.0, 10.0);
        assert_eq!(world.get::<Hunger>(mob).unwrap().nutrition, 150.0);
        // At the threshold it feeds again, but only after its 5 s cooldown.
        life(&mut world, pump, mob, 2.0, 12.0);
        assert_eq!(world.get::<Hunger>(mob).unwrap().nutrition, 150.0);
        life(&mut world, pump, mob, 2.0, 15.0);
        assert_eq!(world.get::<Hunger>(mob).unwrap().nutrition, 200.0);
        // Above the starving threshold the basic pump stops.
        life(&mut world, pump, mob, 2.0, 30.0);
        assert_eq!(world.get::<Hunger>(mob).unwrap().nutrition, 200.0);
    }

    #[test]
    fn rebooter_clears_stuns_once_a_minute() {
        let mut world = World::new();
        let mob = human(&mut world);
        let cns = spawn_implant(&mut world, "/obj/item/organ/cyberimp/brain/anti_stun").unwrap();
        insert(&mut world, cns, mob, ImplantSlot::Cns);
        crate::status_effects::apply(&mut world, mob, Incap::Paralyze, 10.0);
        rebooter_tick(&mut world, cns, mob, 0.0);
        assert!(world.get::<StatusEffects>(mob).unwrap().paralyze > 0.0);
        // A second later it is gone, and new stuns do not hold for 6 s.
        rebooter_tick(&mut world, cns, mob, 1.0);
        assert_eq!(world.get::<StatusEffects>(mob).unwrap().paralyze, 0.0);
        crate::status_effects::apply(&mut world, mob, Incap::Stun, 5.0);
        rebooter_tick(&mut world, cns, mob, 3.0);
        assert_eq!(world.get::<StatusEffects>(mob).unwrap().stun, 0.0);
        // After the resistance, the cooldown: a stun stays.
        crate::status_effects::apply(&mut world, mob, Incap::Stun, 5.0);
        rebooter_tick(&mut world, cns, mob, 10.0);
        rebooter_tick(&mut world, cns, mob, 11.5);
        assert!(world.get::<StatusEffects>(mob).unwrap().stun > 0.0);
        // An EMP breaks it for 9 s / severity, with the brain implant's stun.
        world.init_resource::<EmpRng>();
        emp(&mut world, cns, mob, 1.0, 70.0);
        assert!(world.get::<Implant>(cns).unwrap().failing);
        assert!(world.get::<StatusEffects>(mob).unwrap().stun >= 20.0);
        rebooter_tick(&mut world, cns, mob, 79.0);
        assert!(!world.get::<Implant>(cns).unwrap().failing);
    }

    #[test]
    fn reviver_heals_its_owner_in_crit() {
        let mut world = World::new();
        let mob = human(&mut world);
        let reviver = spawn_implant(&mut world, "/obj/item/organ/cyberimp/chest/reviver").unwrap();
        insert(&mut world, reviver, mob, ImplantSlot::HeartAid);
        let mut health = world.get::<Health>(mob).unwrap().clone();
        health.adjust(DamageType::Brute, 120.0);
        world.entity_mut(mob).insert((health, Stat::SoftCrit));
        // The first fire starts it, the next heals.
        life(&mut world, reviver, mob, 2.0, 0.0);
        assert!(world.get::<Implant>(reviver).unwrap().active);
        life(&mut world, reviver, mob, 2.0, 2.0);
        assert_eq!(world.get::<Health>(mob).unwrap().brute, 118.0);
        // Up again: it recharges for what it cost.
        world.entity_mut(mob).insert(Stat::Conscious);
        let mut health = world.get::<Health>(mob).unwrap().clone();
        health.adjust(DamageType::Brute, -118.0);
        world.entity_mut(mob).insert(health);
        life(&mut world, reviver, mob, 2.0, 4.0);
        let state = world.get::<Implant>(reviver).unwrap();
        assert!(!state.active && state.cooldown_until == 8.0);
    }
}
