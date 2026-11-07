//! Chameleon clothing after tg `code/modules/clothing/chameleon/`: clothing with a
//! `/datum/action/item_action/chameleon/change` action ([`Chameleon`]) gets a "Change ...
//! Appearance" button wherever it is carried (`action_slots = ALL`); the button lists every
//! item of its family (`chameleon_type`, minus the blacklist) and the pick copies that type's
//! look: name, description, icon (GAGS icons are already the rendered ones), worn and in-hand
//! sprites, `flags_inv`, `transparent_protection` and `flags_cover` ([`update_look`]). Each item
//! starts as its `active_type` (chameleon shoes look like black sneakers).
//!
//! - `/datum/action/chameleon_outfit` ("Select Chameleon Outfit"): one button for a mob with
//!   any chameleon item; picking a job's outfit turns every carried chameleon item into the
//!   first outfit item of its family at once (`apply_outfit`).
//! - EMP (`emp_randomise`): a random look now and every second for 30 s; `/broken` items
//!   (`BREAK_CHAMELEON_ACTION`) randomise forever.
//!
//! - Non-clothing chameleon items (backpack, belt and holster, headset, PDA, stamp, agent ID,
//!   the chameleon guns, the scanner) are found by type ([`Kind::items`]) and change looks the
//!   same way; the chameleon gun ignores EMP.
//! - The chameleon scanner ([`scanner_click`]): a human (or a turf with one) within 5 tiles, a
//!   0.5 s scan, a 5 s cooldown; the worn types (a chameleon item's look) become a custom outfit
//!   ("Custom Outfit #n", 10 slots, listed first by the outfit button); right click also puts it
//!   on.
//!
//! Not ported: saving the current setup with a right click on the outfit button (action
//! buttons have no right click), the hood a chameleon hat becomes with a hooded suit, mask
//! voice filters; the agent ID's trim, assignment and name (`crate::id_access`), the PDA's job
//! imprint, and the chameleon gun's copied projectiles (`crate::guns`).

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmClothing, DmItem, DmName};
use ss13_proto::Prototypes;
use ss13_shared::chat::Span;
use ss13_shared::protocol::InventorySlot;
use ss13_shared::{Appearance, TilePos};
use ss13_tgui::interfaces::list_input::{ListInputData, ListInputUi};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_tgui::{UiInterface, UiStatus};

use crate::actions::{ActionButton, ActionButtons, ActionButtonsUi, set_buttons};
use crate::emp::{EmpAct, EmpRng, EmpSystems, emp_acts};
use crate::health::Stat;
use crate::inventory::{ALL_SLOTS, CarriedBy, Inventory};
use crate::medical::surgery::ListAct;
use crate::server::PlayerMob;
use crate::tools::tell;

/// `/datum/action/item_action/chameleon/change`.
pub const CHANGE_ACTION: &str = "/datum/action/item_action/chameleon/change";
/// `EMP_RANDOMISE_TIME`.
pub const EMP_RANDOMISE_SECS: f64 = 30.0;
/// `SSprocessing` `wait`: how often an EMP'd item picks a new look.
const RANDOMISE_PERIOD: f64 = 1.0;
/// `item_flags` `ABSTRACT`.
const ABSTRACT: u32 = 1 << 7;
/// `/obj/item/chameleon_scanner`.
pub const SCANNER: &str = "/obj/item/chameleon_scanner";
/// The scanner's `scan_range`, tiles.
pub const SCAN_RANGE: i32 = 5;
/// `COOLDOWN_START(src, scan_cooldown, 5 SECONDS)`.
pub const SCAN_COOLDOWN_SECS: f64 = 5.0;
/// `do_after(scanner, 0.5 SECONDS, ...)`, deciseconds.
const SCAN_DS: f32 = 5.0;
/// `max_custom_oufits`.
pub const MAX_CUSTOM_OUTFITS: usize = 10;
const SCAN_SOUND: &str = "sound/machines/ping.ogg";
/// `SILENCED_SOUND_EXTRARANGE`.
const SILENCED_SOUND_EXTRARANGE: f32 = -11.0;
/// The outfit button's id.
const OUTFIT_BUTTON: &str = "chameleon_outfit";
/// [`ChameleonChoice::values`] of a custom outfit: the slot index after it (job titles never
/// start with it).
const CUSTOM_PREFIX: &str = "custom:";
const BUTTON_BACKGROUNDS: &str = "icons/mob/actions/backgrounds.dmi";

/// One `/datum/action/item_action/chameleon/change` subtype of clothing.
pub struct Kind {
    pub action: &'static str,
    /// `chameleon_type` and the `add_chameleon_items` extras.
    pub roots: &'static [&'static str],
    /// Exact types added besides the trees (`add_chameleon_items(..., only_root = TRUE)`).
    pub include_exact: &'static [&'static str],
    /// The chameleon item types of a non-clothing family (their `actions_types` are not in the
    /// data: [`DmClothing::actions`] exists on clothing only); empty for clothing.
    pub items: &'static [&'static str],
    /// `chameleon_name`.
    pub name: &'static str,
    pub active_type: Option<&'static str>,
    /// `initialize_blacklist`: whole trees, and exact types (`only_root_path`).
    pub blacklist_trees: &'static [&'static str],
    pub blacklist_exact: &'static [&'static str],
}

/// `chameleon_action_subtypes.dm`; more specific actions (and item types) first.
pub const KINDS: [Kind; 18] = [
    Kind {
        action: "/datum/action/item_action/chameleon/change/glasses/no_preset",
        roots: &["/obj/item/clothing/glasses"],
        include_exact: &[],
        items: &[],
        name: "Glasses",
        active_type: None,
        blacklist_trees: &[],
        blacklist_exact: &[
            "/obj/item/clothing/glasses/changeling",
            "/obj/item/clothing/glasses/hud/security/chameleon",
            "/obj/item/clothing/glasses/thermal/syndi",
        ],
    },
    Kind {
        action: "/datum/action/item_action/chameleon/change/glasses",
        roots: &["/obj/item/clothing/glasses"],
        include_exact: &[],
        items: &[],
        name: "Glasses",
        active_type: Some("/obj/item/clothing/glasses/meson"),
        blacklist_trees: &[],
        blacklist_exact: &[
            "/obj/item/clothing/glasses/changeling",
            "/obj/item/clothing/glasses/hud/security/chameleon",
            "/obj/item/clothing/glasses/thermal/syndi",
        ],
    },
    Kind {
        action: "/datum/action/item_action/chameleon/change/jumpsuit",
        roots: &["/obj/item/clothing/under"],
        include_exact: &[],
        items: &[],
        name: "Jumpsuit",
        active_type: Some("/obj/item/clothing/under/color/black"),
        blacklist_trees: &[],
        blacklist_exact: &[
            "/obj/item/clothing/under",
            "/obj/item/clothing/under/color",
            "/obj/item/clothing/under/rank",
            "/obj/item/clothing/under/changeling",
        ],
    },
    Kind {
        action: "/datum/action/item_action/chameleon/change/suit",
        roots: &["/obj/item/clothing/suit"],
        include_exact: &[],
        items: &[],
        name: "Suit",
        active_type: Some("/obj/item/clothing/suit/armor/vest"),
        blacklist_trees: &[],
        blacklist_exact: &[
            "/obj/item/clothing/suit/armor/abductor",
            "/obj/item/clothing/suit/changeling",
        ],
    },
    Kind {
        action: "/datum/action/item_action/chameleon/change/gloves",
        roots: &["/obj/item/clothing/gloves"],
        include_exact: &[],
        items: &[],
        name: "Gloves",
        active_type: Some("/obj/item/clothing/gloves/color/yellow"),
        blacklist_trees: &[],
        blacklist_exact: &[
            "/obj/item/clothing/gloves",
            "/obj/item/clothing/gloves/color",
            "/obj/item/clothing/gloves/changeling",
        ],
    },
    Kind {
        action: "/datum/action/item_action/chameleon/change/hat",
        roots: &["/obj/item/clothing/head"],
        include_exact: &[],
        items: &[],
        name: "Hat",
        active_type: Some("/obj/item/clothing/head/soft/black"),
        blacklist_trees: &[],
        blacklist_exact: &["/obj/item/clothing/head/changeling"],
    },
    Kind {
        action: "/datum/action/item_action/chameleon/change/mask",
        roots: &[
            "/obj/item/clothing/mask",
            "/obj/item/cigarette",
            "/obj/item/vape",
        ],
        include_exact: &[],
        items: &[],
        name: "Mask",
        active_type: Some("/obj/item/clothing/mask/gas"),
        blacklist_trees: &[],
        blacklist_exact: &["/obj/item/clothing/mask/changeling"],
    },
    Kind {
        action: "/datum/action/item_action/chameleon/change/shoes",
        roots: &["/obj/item/clothing/shoes"],
        include_exact: &[],
        items: &[],
        name: "Shoes",
        active_type: Some("/obj/item/clothing/shoes/sneakers/black"),
        blacklist_trees: &[],
        blacklist_exact: &["/obj/item/clothing/shoes/changeling"],
    },
    Kind {
        action: "/datum/action/item_action/chameleon/change/neck",
        roots: &["/obj/item/clothing/neck"],
        include_exact: &[],
        items: &[],
        name: "Neck Accessory",
        active_type: Some("/obj/item/clothing/neck/tie/black"),
        blacklist_trees: &["/obj/item/clothing/neck/cloak/skill_reward"],
        blacklist_exact: &[],
    },
    Kind {
        action: "/datum/action/item_action/chameleon/change/backpack",
        roots: &["/obj/item/storage/backpack"],
        include_exact: &[],
        items: &["/obj/item/storage/backpack/chameleon"],
        name: "Backpack",
        active_type: Some("/obj/item/storage/backpack"),
        blacklist_trees: &[],
        blacklist_exact: &[],
    },
    Kind {
        action: "/datum/action/item_action/chameleon/change/belt",
        roots: &["/obj/item/storage/belt"],
        include_exact: &[],
        items: &[
            "/obj/item/storage/belt/chameleon",
            "/obj/item/storage/belt/holster/chameleon",
        ],
        name: "Belt",
        active_type: Some("/obj/item/storage/belt/utility"),
        blacklist_trees: &[],
        blacklist_exact: &[],
    },
    Kind {
        action: "/datum/action/item_action/chameleon/change/headset",
        roots: &["/obj/item/radio/headset"],
        include_exact: &[],
        items: &["/obj/item/radio/headset/chameleon"],
        name: "Headset",
        active_type: Some("/obj/item/radio/headset"),
        blacklist_trees: &[],
        blacklist_exact: &[],
    },
    Kind {
        action: "/datum/action/item_action/chameleon/change/tablet",
        roots: &["/obj/item/modular_computer/pda"],
        include_exact: &[],
        items: &["/obj/item/modular_computer/pda/chameleon"],
        name: "Tablet",
        active_type: None,
        blacklist_trees: &[],
        blacklist_exact: &["/obj/item/modular_computer/pda/crew/heads"],
    },
    Kind {
        action: "/datum/action/item_action/chameleon/change/stamp",
        roots: &["/obj/item/stamp"],
        include_exact: &[],
        items: &["/obj/item/stamp/chameleon"],
        name: "Stamp",
        active_type: None,
        blacklist_trees: &[],
        blacklist_exact: &[],
    },
    Kind {
        action: "/datum/action/item_action/chameleon/change/id",
        roots: &[
            "/obj/item/card/id/advanced/black",
            "/obj/item/card/id/advanced/centcom",
        ],
        include_exact: &[
            "/obj/item/card/id/advanced",
            "/obj/item/card/id/advanced/bountyhunter",
            "/obj/item/card/id/advanced/gold",
            "/obj/item/card/id/advanced/platinum",
            "/obj/item/card/id/advanced/prisoner",
            "/obj/item/card/id/advanced/rainbow",
            "/obj/item/card/id/advanced/robotic",
            "/obj/item/card/id/advanced/silver",
            "/obj/item/card/id/advanced/gold/captains_spare",
        ],
        items: &["/obj/item/card/id/advanced/chameleon"],
        name: "ID Card",
        active_type: None,
        blacklist_trees: &[],
        blacklist_exact: &[],
    },
    Kind {
        action: "/datum/action/item_action/chameleon/change/gun/ballistic",
        roots: &["/obj/item/gun/ballistic"],
        include_exact: &[],
        items: &["/obj/item/gun/energy/laser/chameleon/ballistic_only"],
        name: "Gun",
        // `default_look`.
        active_type: Some("/obj/item/gun/ballistic/automatic/mini_uzi"),
        blacklist_trees: &["/obj/item/gun/energy/minigun"],
        blacklist_exact: &[],
    },
    Kind {
        action: "/datum/action/item_action/chameleon/change/gun",
        roots: &["/obj/item/gun"],
        include_exact: &[],
        items: &["/obj/item/gun/energy/laser/chameleon"],
        name: "Gun",
        active_type: Some("/obj/item/gun/energy/laser"),
        blacklist_trees: &["/obj/item/gun/energy/minigun"],
        blacklist_exact: &[],
    },
    Kind {
        action: "/datum/action/item_action/chameleon/change/scanner",
        roots: &[
            "/obj/item/pen",
            "/obj/item/analyzer",
            "/obj/item/assembly/flash/handheld",
            "/obj/item/assembly/signaler",
            "/obj/item/autopsy_scanner",
            "/obj/item/camera",
            "/obj/item/cane",
            "/obj/item/detective_scanner",
            "/obj/item/door_remote",
            "/obj/item/flashlight",
            "/obj/item/geiger_counter",
            "/obj/item/healthanalyzer",
            "/obj/item/mining_scanner",
            "/obj/item/multitool",
            "/obj/item/plant_analyzer",
            "/obj/item/sensor_device",
            "/obj/item/storage/fancy/cigarettes",
            "/obj/item/taperecorder",
            "/obj/item/toy/crayon",
        ],
        include_exact: &[],
        items: &[SCANNER],
        name: "Chameleon Scanner",
        active_type: Some("/obj/item/storage/fancy/cigarettes/cigpack_robustgold"),
        blacklist_trees: &[
            "/obj/item/assembly/signaler/anomaly",
            "/obj/item/camera/siliconcam",
            "/obj/item/door_remote/omni",
            "/obj/item/flashlight/emp/debug",
            "/obj/item/flashlight/flare",
            "/obj/item/flashlight/lamp",
            "/obj/item/healthanalyzer/rad_laser",
            "/obj/item/multitool/ai_detect",
            "/obj/item/multitool/cyborg",
            "/obj/item/multitool/drone",
            "/obj/item/multitool/field_debug",
            "/obj/item/storage/fancy/cigarettes/cigars",
        ],
        blacklist_exact: &[],
    },
];

/// On a chameleon item: its family ([`KINDS`]) and look.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Chameleon {
    pub kind: usize,
    /// `active_type`: the type it looks like.
    pub active: Option<String>,
    /// The `active_type` look was applied (prototypes are absent during map load).
    pub ready: bool,
    /// `emp_timer`: random looks until then (`INFINITY` when broken).
    pub emp_until: f64,
    pub next_random: f64,
}

/// A mob's open chameleon list.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ChameleonChoice {
    /// The item, or `None` for the outfit list.
    pub item: Option<Entity>,
    pub names: Vec<String>,
    /// Type paths (item list) or job titles (outfit list), by `names` index.
    pub values: Vec<String>,
}

/// The server side of `ListInputWindow` for both chameleon lists.
pub struct ChameleonListWindow;

impl UiInterface for ChameleonListWindow {
    const NAME: &'static str = ListInputUi::NAME;
    type Data = ListInputData;
    type StaticData = ();
}

impl ServerUi for ChameleonListWindow {
    type Source = &'static ChameleonChoice;
    type UserParam = ();
    type Act = ListAct;

    fn static_data(_: &ChameleonChoice) {}

    fn data(choice: &ChameleonChoice, _: Entity, _: &()) -> ListInputData {
        ListInputData {
            title: "Chameleon Settings".into(),
            message: if choice.item.is_some() {
                "Select the look to change into".into()
            } else {
                "Select outfit to change into".into()
            },
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
    app.register_ui::<ChameleonListWindow>()
        .add_observer(chameleon_items)
        .add_observer(chameleon_non_clothing)
        .add_systems(
            Update,
            (
                (init_looks, randomise).chain(),
                refresh_outfit_button.after(crate::inventory::inventory_intents),
                (
                    trigger_outfit.run_if(resource_exists::<Messages<UiActed<ActionButtonsUi>>>),
                    pick,
                )
                    .in_set(UiSystems::Handle),
                close_stale_lists.in_set(UiSystems::StatusHook),
                chameleon_emp
                    .in_set(EmpSystems::Effects)
                    .run_if(resource_exists::<Messages<EmpAct>>),
            ),
        );
}

/// The [`KINDS`] index of clothing with a chameleon change action.
pub fn kind_of(clothing: &DmClothing) -> Option<usize> {
    let action = clothing
        .actions
        .iter()
        .find(|a| is_under(a, CHANGE_ACTION))?;
    KINDS.iter().position(|k| is_under(action, k.action))
}

fn chameleon_items(add: On<Add<DmClothing>>, items: Query<&DmClothing>, mut commands: Commands) {
    let Some(kind) = items.get(add.entity).ok().and_then(kind_of) else {
        return;
    };
    commands.entity(add.entity).try_insert_if_new(Chameleon {
        kind,
        active: KINDS[kind].active_type.map(str::to_owned),
        ready: false,
        emp_until: 0.0,
        next_random: 0.0,
    });
}

/// The [`KINDS`] index of a non-clothing chameleon item type: its most specific item root.
pub fn kind_of_item(path: &str) -> Option<usize> {
    KINDS
        .iter()
        .enumerate()
        .flat_map(|(i, k)| k.items.iter().map(move |root| (i, *root)))
        .filter(|(_, root)| is_under(path, root))
        .max_by_key(|(_, root)| root.len())
        .map(|(i, _)| i)
}

fn chameleon_non_clothing(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Some(kind) = items
        .get(add.entity)
        .ok()
        .and_then(|i| kind_of_item(&i.path))
    else {
        return;
    };
    commands.entity(add.entity).try_insert_if_new(Chameleon {
        kind,
        active: KINDS[kind].active_type.map(str::to_owned),
        ready: false,
        emp_until: 0.0,
        next_random: 0.0,
    });
}

/// `path` is one of `kind`'s disguises before the blacklist (`chameleon_typecache`).
fn in_family(kind: &Kind, path: &str) -> bool {
    kind.roots.iter().any(|r| is_under(path, r)) || kind.include_exact.contains(&path)
}

/// The "Change ... Appearance" button name of `item` (`name = "Change [chameleon_name]
/// Appearance"`).
pub fn button_name(world: &World, item: Entity) -> Option<&'static str> {
    let kind = world.get::<Chameleon>(item)?.kind;
    Some(match KINDS[kind].name {
        "Glasses" => "Change Glasses Appearance",
        "Jumpsuit" => "Change Jumpsuit Appearance",
        "Suit" => "Change Suit Appearance",
        "Gloves" => "Change Gloves Appearance",
        "Hat" => "Change Hat Appearance",
        "Mask" => "Change Mask Appearance",
        "Shoes" => "Change Shoes Appearance",
        "Neck Accessory" => "Change Neck Accessory Appearance",
        "Backpack" => "Change Backpack Appearance",
        "Belt" => "Change Belt Appearance",
        "Headset" => "Change Headset Appearance",
        "Tablet" => "Change Tablet Appearance",
        "Stamp" => "Change Stamp Appearance",
        "ID Card" => "Change ID Card Appearance",
        "Gun" => "Change Gun Appearance",
        _ => "Change Chameleon Scanner Appearance",
    })
}

/// `add_chameleon_items` minus the blacklist (`typecacheof(target.type)` too), abstract items
/// and types without an icon state: (readable name, type), by type path.
pub fn disguises(world: &World, item: Entity) -> Vec<(String, String)> {
    let (Some(state), Some(own), Some(protos)) = (
        world.get::<Chameleon>(item),
        world.get::<DmItem>(item).map(|d| d.path.clone()),
        world.get_resource::<Prototypes>(),
    ) else {
        return Vec::new();
    };
    let kind = &KINDS[state.kind];
    let mut out: Vec<(String, String)> = protos
        .ids()
        .filter(|id| in_family(kind, id))
        .filter(|id| !is_under(id, &own))
        .filter(|id| !kind.blacklist_exact.contains(id))
        .filter(|id| !kind.blacklist_trees.iter().any(|t| is_under(id, t)))
        .filter_map(|id| {
            let proto = protos.get(id)?;
            let dm = proto.get::<DmItem>()?;
            let atom = proto.get::<DmAtom>()?;
            if dm.item_flags & ABSTRACT != 0 || atom.icon_state.is_empty() {
                return None;
            }
            let name = proto
                .get::<DmName>()
                .map(|n| n.name.clone())
                .unwrap_or_default();
            // `format_readable_name`.
            Some((
                format!("{name} ({})", atom.icon_state.replace('_', " ")),
                id.to_owned(),
            ))
        })
        .collect();
    // `cmp_typepaths_asc`.
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// `update_look`/`update_item`: `item` takes the look of type `path`. False without its
/// prototype.
pub fn update_look(world: &mut World, item: Entity, path: &str) -> bool {
    let Some(proto) = world
        .get_resource::<Prototypes>()
        .and_then(|p| p.get(path).cloned())
    else {
        return false;
    };
    let atom = proto.get::<DmAtom>().cloned().unwrap_or_default();
    let name = proto.get::<DmName>().cloned().unwrap_or_default();
    let picked = proto.get::<DmItem>().cloned().unwrap_or_default();
    let clothing = proto.get::<DmClothing>().cloned();
    let icon = world.get_resource::<IconIndex>().and_then(|i| {
        i.get(&atom.icon, &atom.icon_state)
            .or_else(|| i.get(&atom.icon, ""))
    });
    let Ok(mut e) = world.get_entity_mut(item) else {
        return false;
    };
    if let Some(mut n) = e.get_mut::<Name>() {
        *n = Name::new(name.name.clone());
    }
    e.insert(name);
    // tg edits the item's own `icon`/`icon_state` vars: the port keeps them in `DmAtom`.
    if let Some(mut a) = e.get_mut::<DmAtom>() {
        a.icon = atom.icon.clone();
        a.icon_state = atom.icon_state.clone();
    }
    if let (Some(id), Some(mut look)) = (icon, e.get_mut::<Appearance>()) {
        look.icon_state = id;
    }
    if let Some(mut dm) = e.get_mut::<DmItem>() {
        dm.worn_icon = picked.worn_icon;
        dm.lefthand_file = picked.lefthand_file;
        dm.righthand_file = picked.righthand_file;
        dm.worn_icon_state = picked.worn_icon_state;
        dm.inhand_icon_state = picked.inhand_icon_state;
    }
    if let Some(mut c) = e.get_mut::<DmClothing>() {
        let picked = clothing.unwrap_or_default();
        c.flags_inv = picked.flags_inv;
        c.transparent_protection = picked.transparent_protection;
        c.flags_cover = picked.flags_cover;
    }
    if let Some(mut state) = e.get_mut::<Chameleon>() {
        state.active = Some(path.to_owned());
    }
    if let Some(mob) = world.get::<CarriedBy>(item).map(|c| c.0) {
        crate::inventory::refresh_look(world, mob);
    }
    true
}

/// `random_look`.
pub fn random_look(world: &mut World, item: Entity) {
    let list = disguises(world, item);
    if list.is_empty() {
        return;
    }
    let i = world
        .get_resource_or_init::<EmpRng>()
        .range(0, list.len() as i64 - 1) as usize;
    update_look(world, item, &list[i.min(list.len() - 1)].1);
}

/// `emp_randomise(amount)`.
pub fn emp_randomise(world: &mut World, item: Entity, secs: f64) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut state) = world.get_mut::<Chameleon>(item) {
        state.emp_until = state.emp_until.max(now + secs);
        state.next_random = now + RANDOMISE_PERIOD;
    }
    random_look(world, item);
}

fn init_looks(world: &mut World) {
    if !world.contains_resource::<Prototypes>() {
        return;
    }
    let mut q = world.query::<(Entity, &Chameleon)>();
    let pending: Vec<(Entity, Option<String>)> = q
        .iter(world)
        .filter(|(_, c)| !c.ready)
        .map(|(e, c)| (e, c.active.clone()))
        .collect();
    for (item, active) in pending {
        // `BREAK_CHAMELEON_ACTION`: `emp_randomise(INFINITY)`.
        let broken = world
            .get::<DmItem>(item)
            .is_some_and(|d| d.path.ends_with("/broken"));
        if let Some(mut c) = world.get_mut::<Chameleon>(item) {
            c.ready = true;
        }
        if let Some(active) = active {
            update_look(world, item, &active);
        }
        if broken {
            emp_randomise(world, item, f64::INFINITY);
        }
    }
}

/// `process()`: an EMP'd item's next random look.
fn randomise(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut q = world.query::<(Entity, &Chameleon)>();
    let due: Vec<Entity> = q
        .iter(world)
        .filter(|(_, c)| c.ready && now < c.emp_until && now >= c.next_random)
        .map(|(e, _)| e)
        .collect();
    for item in due {
        if let Some(mut c) = world.get_mut::<Chameleon>(item) {
            c.next_random = now + RANDOMISE_PERIOD;
        }
        random_look(world, item);
    }
}

fn chameleon_emp(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        // The chameleon gun has `EMP_PROTECT_SELF`.
        let affected = world
            .get::<Chameleon>(ev.target)
            .is_some_and(|c| KINDS[c.kind].name != "Gun");
        if ev.hits_self() && affected {
            emp_randomise(world, ev.target, EMP_RANDOMISE_SECS);
        }
    }
}

/// `AB_CHECK_CONSCIOUS | AB_CHECK_INCAPACITATED | AB_CHECK_HANDS_BLOCKED`.
fn can_use(world: &World, mob: Entity) -> bool {
    let incap = crate::status_effects::incapacitation(world, mob);
    world.get::<Stat>(mob) == Some(&Stat::Conscious) && !incap.incapacitated && !incap.hands_blocked
}

/// `select_look`: the item's list opens for `mob`.
pub fn select_look(world: &mut World, client: Entity, mob: Entity, item: Entity) {
    if !can_use(world, mob) {
        tell(
            world,
            client,
            Span::Warning,
            "Сейчас вы не можете этого сделать!",
        );
        return;
    }
    let list = disguises(world, item);
    if list.is_empty() {
        return;
    }
    let (names, values) = list.into_iter().unzip();
    world.entity_mut(mob).insert(ChameleonChoice {
        item: Some(item),
        names,
        values,
    });
    world.write_message(OpenUi::new::<ChameleonListWindow>(
        client,
        mob,
        mob,
        "Chameleon Settings",
    ));
}

/// The chameleon items `mob` carries, in slot order.
fn carried(world: &World, mob: Entity) -> Vec<Entity> {
    let Some(inventory) = world.get::<Inventory>(mob) else {
        return Vec::new();
    };
    ALL_SLOTS
        .iter()
        .filter_map(|&s| inventory.slot(s))
        .filter(|&i| world.get::<Chameleon>(i).is_some())
        .collect()
}

/// `apply_outfit`: each carried chameleon item takes the first outfit item of its family, and
/// that item is used up.
pub fn apply_outfit(world: &mut World, mob: Entity, outfit: &ss13_mapload::jobs::Outfit) {
    let mut paths: Vec<String> = outfit.items.iter().map(|(_, p)| p.clone()).collect();
    paths.extend(outfit.accessory.clone());
    apply_paths(world, mob, paths);
}

/// `/datum/action/chameleon_outfit/apply_outfit(outfit, outfit_types)` over a type list.
pub fn apply_paths(world: &mut World, mob: Entity, mut paths: Vec<String>) {
    for item in carried(world, mob) {
        let kind = world.get::<Chameleon>(item).map_or(0, |c| c.kind);
        let Some(i) = paths.iter().position(|p| in_family(&KINDS[kind], p)) else {
            continue;
        };
        let path = paths.remove(i);
        update_look(world, item, &path);
    }
}

/// On a mob: `custom_outfits` of its outfit action (the chameleon scanner fills them).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct CustomOutfits {
    /// "Custom Outfit #n" by slot index `n - 1`.
    pub slots: Vec<Vec<String>>,
    /// `next_custom_outfit - 1`.
    pub next: usize,
}

/// `save_outfit`: a new custom outfit unless the same one is saved; the slot number, or the
/// reason it was not saved.
pub fn save_outfit(
    world: &mut World,
    mob: Entity,
    paths: Vec<String>,
) -> Result<usize, &'static str> {
    if paths.is_empty() {
        return Err("наряд не сохранён!");
    }
    let mut saved = world.get::<CustomOutfits>(mob).cloned().unwrap_or_default();
    // `~=`: the same paths in the same order.
    if saved.slots.contains(&paths) {
        return Err("наряд уже сохранён!");
    }
    if saved.next >= MAX_CUSTOM_OUTFITS {
        saved.next = 0;
    }
    let slot = saved.next;
    if slot < saved.slots.len() {
        saved.slots[slot] = paths;
    } else {
        saved.slots.push(paths);
    }
    saved.next += 1;
    world.entity_mut(mob).insert(saved);
    Ok(slot + 1)
}

/// The outfit button for a mob with any chameleon item (`chameleon/change/Grant`).
fn refresh_outfit_button(world: &mut World) {
    let mut q = world.query_filtered::<Entity, (Changed<Inventory>, With<PlayerMob>)>();
    let mobs: Vec<Entity> = q.iter(world).collect();
    for mob in mobs {
        let want = !carried(world, mob).is_empty();
        let current = world.get::<ActionButtons>(mob);
        let has = current.is_some_and(|b| b.0.iter().any(|(b, _)| b.id == OUTFIT_BUTTON));
        if want == has {
            continue;
        }
        let mut buttons: Vec<(ActionButton, f64)> = current
            .map(|b| {
                b.0.iter()
                    .filter(|(b, _)| b.id != OUTFIT_BUTTON)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        if want {
            let icons = world.get_resource::<IconIndex>();
            buttons.push((
                ActionButton {
                    id: OUTFIT_BUTTON.into(),
                    name: "Select Chameleon Outfit".into(),
                    icon: icons.and_then(|i| {
                        i.get("icons/mob/actions/actions_clothing.dmi", "chameleon_outfit")
                    }),
                    background: icons.and_then(|i| i.get(BUTTON_BACKGROUNDS, "bg_default")),
                    border: None,
                    available: true,
                    cooldown: 0.0,
                    active: false,
                    maptext: None,
                },
                0.0,
            ));
        }
        set_buttons(world, mob, buttons);
    }
}

/// A click on the outfit button: the job outfits' list.
fn trigger_outfit(world: &mut World, mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .filter(|a| a.act == OUTFIT_BUTTON)
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || carried(world, actor).is_empty()
        {
            continue;
        }
        if !can_use(world, actor) {
            tell(
                world,
                client,
                Span::Warning,
                "Сейчас вы не можете этого сделать!",
            );
            continue;
        }
        let mut titles: Vec<String> = world
            .get_resource::<crate::jobs::Jobs>()
            .map(|j| {
                j.data
                    .jobs
                    .iter()
                    .filter(|j| j.outfit.is_some())
                    .map(|j| j.title.clone())
                    .collect()
            })
            .unwrap_or_default();
        titles.sort();
        // `select_outfit`: the custom outfits first.
        let custom = world
            .get::<CustomOutfits>(actor)
            .map_or(0, |c| c.slots.len());
        let mut names: Vec<String> = (1..=custom)
            .map(|n| format!("Custom Outfit #{n}"))
            .collect();
        let mut values: Vec<String> = (0..custom).map(|i| format!("{CUSTOM_PREFIX}{i}")).collect();
        names.extend(titles.iter().cloned());
        values.extend(titles);
        if names.is_empty() {
            continue;
        }
        world.entity_mut(actor).insert(ChameleonChoice {
            item: None,
            names,
            values,
        });
        world.write_message(OpenUi::new::<ChameleonListWindow>(
            client,
            actor,
            actor,
            "Chameleon Outfit",
        ));
    }
}

/// The pick, checked again at the moment of effect: the owner can still act and still
/// carries the item.
fn pick(world: &mut World, mut acts: Local<MessageCursor<UiActed<ChameleonListWindow>>>) {
    let picked: Vec<(Entity, ListAct)> = acts
        .read(world.resource::<Messages<UiActed<ChameleonListWindow>>>())
        .filter(|a| a.actor == a.source)
        .map(|a| (a.actor, a.act.clone()))
        .collect();
    for (mob, act) in picked {
        let Some(choice) = world.get::<ChameleonChoice>(mob).cloned() else {
            continue;
        };
        world.entity_mut(mob).remove::<ChameleonChoice>();
        let ListAct::Submit(entry) = act else {
            continue;
        };
        let Some(i) = choice.names.iter().position(|n| *n == entry) else {
            continue;
        };
        if !can_use(world, mob) {
            continue;
        }
        match choice.item {
            Some(item) => {
                let still = world.get::<CarriedBy>(item).is_some_and(|c| c.0 == mob)
                    && world.get::<Chameleon>(item).is_some();
                if still {
                    update_look(world, item, &choice.values[i]);
                }
            }
            None if choice.values[i].starts_with(CUSTOM_PREFIX) => {
                let paths = choice.values[i][CUSTOM_PREFIX.len()..]
                    .parse::<usize>()
                    .ok()
                    .and_then(|n| world.get::<CustomOutfits>(mob)?.slots.get(n).cloned());
                if let Some(paths) = paths {
                    apply_paths(world, mob, paths);
                }
            }
            None => {
                let outfit = world.get_resource::<crate::jobs::Jobs>().and_then(|j| {
                    j.data
                        .jobs
                        .iter()
                        .find(|j| j.title == choice.values[i])
                        .and_then(|j| j.outfit.clone())
                });
                if let Some(outfit) = outfit {
                    apply_outfit(world, mob, &outfit);
                }
            }
        }
    }
}

fn close_stale_lists(
    mut windows: Query<&mut UiWindow, With<ss13_tgui::server::UiKind<ChameleonListWindow>>>,
    choices: Query<(), With<ChameleonChoice>>,
) {
    for mut window in &mut windows {
        if !choices.contains(window.source) {
            window.cap_status(UiStatus::Close);
        }
    }
}

// --- The chameleon scanner -------------------------------------------------------------------

/// On a chameleon scanner: `scan_cooldown` ends then.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct ScanCooldown(pub f64);

fn is_scanner(world: &World, item: Entity) -> bool {
    world
        .get::<DmItem>(item)
        .is_some_and(|d| is_under(&d.path, SCANNER))
}

/// `scan_target`'s pick: a human, or a human on the clicked turf (`locate() in scanned`).
fn scan_subject(world: &World, target: Entity, pos: TilePos) -> Option<Entity> {
    let human = |e: Entity| world.get::<crate::human::HumanLook>(e).is_some();
    if human(target) {
        return Some(target);
    }
    world.get::<ss13_mapload::Turf>(target)?;
    world
        .get_resource::<ss13_world::SpatialIndex>()?
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| human(e))
}

/// `get_dist(scanner, mob_copying) > scan_range`.
fn in_scan_range(world: &World, mob: Entity, subject: Entity) -> bool {
    let (Some(a), Some(b)) = (world.get::<TilePos>(mob), world.get::<TilePos>(subject)) else {
        return false;
    };
    a.chebyshev(*b).is_some_and(|d| d <= SCAN_RANGE as u32)
}

/// The scanner's click on an atom in view (`ranged_interact_with_atom`, right click:
/// `_secondary`, which also puts the outfit on): checks, then the 0.5 s scan.
pub fn scanner_click(world: &mut World, c: &crate::click_handlers::ClickCtx) -> bool {
    let Some(held) = c.held.filter(|&h| is_scanner(world, h)) else {
        return false;
    };
    let Some(subject) = scan_subject(world, c.target, c.pos) else {
        return false;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    if world.get::<ScanCooldown>(held).is_some_and(|cd| now < cd.0) {
        tell(world, c.client, Span::Warning, "ещё не готов!");
        return true;
    }
    if !in_scan_range(world, c.mob, subject) {
        tell(world, c.client, Span::Warning, "слишком далеко!");
        return true;
    }
    let actor = crate::tools::Actor {
        client: c.client,
        mob: c.mob,
        item: Some(held),
    };
    crate::tools::do_after(
        world,
        actor,
        subject,
        SCAN_DS,
        0,
        crate::construction::Job::Clothing(super::damage::ClothingJob::ChameleonScan {
            target: subject,
            equip: c.modifiers.right,
        }),
    );
    true
}

/// The end of the scan, checked again: the scanner still in the active hand and ready, the
/// subject still in range. The subject's worn types (a chameleon item's look, not its type:
/// "prevent counter spying") become a custom outfit; `equip` also applies it.
pub fn complete_scan(
    world: &mut World,
    mob: Entity,
    action: &crate::tools::DoAfter,
    subject: Entity,
    equip: bool,
) {
    let client = action.client;
    let Some(scanner) = action.item.filter(|&s| is_scanner(world, s)) else {
        return;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    let held = world
        .get::<Inventory>(mob)
        .is_some_and(|i| i.active_item() == Some(scanner));
    if !held
        || world
            .get::<ScanCooldown>(scanner)
            .is_some_and(|cd| now < cd.0)
        || !in_scan_range(world, mob, subject)
    {
        return;
    }
    let Some(inventory) = world.get::<Inventory>(subject).cloned() else {
        return;
    };
    // `get_equipped_items()`: worn, without pockets and suit storage.
    let mut scanned: Vec<String> = Vec::new();
    for slot in ALL_SLOTS {
        if matches!(
            slot,
            InventorySlot::LeftHand
                | InventorySlot::RightHand
                | InventorySlot::LeftPocket
                | InventorySlot::RightPocket
                | InventorySlot::SuitStorage
        ) {
            continue;
        }
        let Some(item) = inventory.slot(slot) else {
            continue;
        };
        let path = world
            .get::<Chameleon>(item)
            .and_then(|c| c.active.clone())
            .or_else(|| world.get::<DmItem>(item).map(|d| d.path.clone()));
        if let Some(path) = path.filter(|p| !scanned.contains(p)) {
            scanned.push(path);
        }
    }
    if scanned.is_empty() {
        tell(world, client, Span::Warning, "нечего сканировать!");
        return;
    }
    if let Some(&pos) = world.get::<TilePos>(mob) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, SCAN_SOUND, 30.0, true)
                .extrarange(SILENCED_SOUND_EXTRARANGE),
        );
    }
    world
        .entity_mut(scanner)
        .insert(ScanCooldown(now + SCAN_COOLDOWN_SECS));
    // The outfit action exists while the scanner (itself a chameleon item) is carried.
    match save_outfit(world, mob, scanned.clone()) {
        Ok(slot) => tell(
            world,
            client,
            Span::Notice,
            format!("наряд сохранён в слот {slot}"),
        ),
        Err(why) => tell(world, client, Span::Warning, why),
    }
    if equip {
        apply_paths(world, mob, scanned);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_pick_their_family() {
        let with = |action: &str| DmClothing {
            actions: vec![action.into()],
            ..default()
        };
        let kind = |a| kind_of(&with(a)).map(|k| KINDS[k].name);
        assert_eq!(
            kind("/datum/action/item_action/chameleon/change/jumpsuit"),
            Some("Jumpsuit")
        );
        assert_eq!(
            kind_of(&with(
                "/datum/action/item_action/chameleon/change/glasses/no_preset"
            ))
            .map(|k| KINDS[k].active_type),
            Some(None)
        );
        assert_eq!(kind("/datum/action/item_action/adjust"), None);
        assert_eq!(
            kind("/datum/action/item_action/chameleon/change/stamp"),
            None
        );
    }

    #[test]
    fn non_clothing_items_pick_their_family() {
        let name = |p: &str| kind_of_item(p).map(|k| (KINDS[k].name, KINDS[k].active_type));
        assert_eq!(
            name("/obj/item/gun/energy/laser/chameleon/ballistic_only"),
            Some(("Gun", Some("/obj/item/gun/ballistic/automatic/mini_uzi")))
        );
        assert_eq!(
            name("/obj/item/gun/energy/laser/chameleon"),
            Some(("Gun", Some("/obj/item/gun/energy/laser")))
        );
        assert_eq!(
            name("/obj/item/storage/belt/holster/chameleon").map(|n| n.0),
            Some("Belt")
        );
        assert_eq!(
            name("/obj/item/storage/backpack/chameleon/broken").map(|n| n.0),
            Some("Backpack")
        );
        assert_eq!(name(SCANNER).map(|n| n.0), Some("Chameleon Scanner"));
        assert_eq!(name("/obj/item/storage/backpack"), None);
        // The ID card's list: the exact basic cards and the black and centcom trees.
        let id = &KINDS[kind_of_item("/obj/item/card/id/advanced/chameleon").unwrap()];
        assert!(in_family(id, "/obj/item/card/id/advanced/gold"));
        assert!(in_family(
            id,
            "/obj/item/card/id/advanced/black/syndicate_command"
        ));
        assert!(!in_family(id, "/obj/item/card/id/advanced/gold/hos"));
    }

    #[test]
    fn custom_outfits_dedupe_and_wrap() {
        let mut world = World::new();
        let mob = world.spawn_empty().id();
        let outfit = |n: usize| vec![format!("/obj/item/clothing/under/n{n}")];
        assert_eq!(
            save_outfit(&mut world, mob, Vec::new()),
            Err("наряд не сохранён!")
        );
        assert_eq!(save_outfit(&mut world, mob, outfit(0)), Ok(1));
        assert_eq!(
            save_outfit(&mut world, mob, outfit(0)),
            Err("наряд уже сохранён!")
        );
        for n in 1..MAX_CUSTOM_OUTFITS {
            assert_eq!(save_outfit(&mut world, mob, outfit(n)), Ok(n + 1));
        }
        // The 11th overwrites slot 1.
        assert_eq!(save_outfit(&mut world, mob, outfit(99)), Ok(1));
        let saved = world.get::<CustomOutfits>(mob).unwrap();
        assert_eq!(saved.slots.len(), MAX_CUSTOM_OUTFITS);
        assert_eq!(saved.slots[0], outfit(99));
    }
}
