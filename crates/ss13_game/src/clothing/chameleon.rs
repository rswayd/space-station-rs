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
//! Not ported: the custom outfit slots (right click), the hood a chameleon hat becomes with a
//! hooded suit, mask voice filters, job data of the PDA and ID card actions; the non-clothing
//! chameleon items (backpack, belt, headset, PDA, stamp, ID, gun, scanner) carry no
//! [`DmClothing`] action list yet.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmClothing, DmItem, DmName};
use ss13_proto::Prototypes;
use ss13_shared::Appearance;
use ss13_shared::chat::Span;
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
/// The outfit button's id.
const OUTFIT_BUTTON: &str = "chameleon_outfit";
const BUTTON_BACKGROUNDS: &str = "icons/mob/actions/backgrounds.dmi";

/// One `/datum/action/item_action/chameleon/change` subtype of clothing.
pub struct Kind {
    pub action: &'static str,
    /// `chameleon_type` and the `add_chameleon_items` extras.
    pub roots: &'static [&'static str],
    /// `chameleon_name`.
    pub name: &'static str,
    pub active_type: Option<&'static str>,
    /// `initialize_blacklist`: whole trees, and exact types (`only_root_path`).
    pub blacklist_trees: &'static [&'static str],
    pub blacklist_exact: &'static [&'static str],
}

/// `chameleon_action_subtypes.dm`, the clothing ones; more specific actions first.
pub const KINDS: [Kind; 9] = [
    Kind {
        action: "/datum/action/item_action/chameleon/change/glasses/no_preset",
        roots: &["/obj/item/clothing/glasses"],
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
        name: "Mask",
        active_type: Some("/obj/item/clothing/mask/gas"),
        blacklist_trees: &[],
        blacklist_exact: &["/obj/item/clothing/mask/changeling"],
    },
    Kind {
        action: "/datum/action/item_action/chameleon/change/shoes",
        roots: &["/obj/item/clothing/shoes"],
        name: "Shoes",
        active_type: Some("/obj/item/clothing/shoes/sneakers/black"),
        blacklist_trees: &[],
        blacklist_exact: &["/obj/item/clothing/shoes/changeling"],
    },
    Kind {
        action: "/datum/action/item_action/chameleon/change/neck",
        roots: &["/obj/item/clothing/neck"],
        name: "Neck Accessory",
        active_type: Some("/obj/item/clothing/neck/tie/black"),
        blacklist_trees: &["/obj/item/clothing/neck/cloak/skill_reward"],
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
        _ => "Change Neck Accessory Appearance",
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
        .filter(|id| kind.roots.iter().any(|r| is_under(id, r)))
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
        if ev.hits_self() && world.get::<Chameleon>(ev.target).is_some() {
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
    for item in carried(world, mob) {
        let kind = world.get::<Chameleon>(item).map_or(0, |c| c.kind);
        let Some(i) = paths
            .iter()
            .position(|p| KINDS[kind].roots.iter().any(|r| is_under(p, r)))
        else {
            continue;
        };
        let path = paths.remove(i);
        update_look(world, item, &path);
    }
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
        if titles.is_empty() {
            continue;
        }
        world.entity_mut(actor).insert(ChameleonChoice {
            item: None,
            names: titles.clone(),
            values: titles,
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
}
