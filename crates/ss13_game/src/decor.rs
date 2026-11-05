//! Curtains, mirrors, signs, plaques and statues (server), after tg
//! `code/game/objects/structures/curtains.dm`, `mirror.dm`, `signs/_signs.dm`,
//! `plaques/_plaques.dm` and `code/modules/art/statues.dm` (with BandaStation's station map
//! statues, `modular_bandastation/objects/code/structures/statues.dm`).
//!
//! - Curtains ([`Curtain`]): an empty hand draws or opens them (the shower curtain stays
//!   see-through, cloth and bounty curtains block sight when closed; closed ones hang above
//!   mobs); a wrench (5 s) fastens or unfastens them, wirecutters (5 s) cut an unfastened one into
//!   cloth, plastic and a rod.
//! - Mirrors ([`Mirror`]): a human's empty hand opens the hairstyle list (tg's radial "Change
//!   Hair"), the pick is applied at the moment of effect if the mirror is still whole and in
//!   reach; a mirror breaks at half integrity ("mirror_broke", the shatter), a lit welder (1 s)
//!   mends a broken one, a wrench (right click, 3 s) takes it off the wall as its frame
//!   (broken or not), destroyed it leaves a shard.
//! - Signs and plaques: an empty hand examines them; a wrench (4 s) takes one off the wall as an
//!   item with its name, description and look (`/obj/item/sign`, `/obj/item/plaque`; flags,
//!   logos, posters, paintings and picture frames stay); a lit welder (4 s) repairs a damaged one.
//! - Statues: a wrench (2 s) fastens or unfastens them, a lit welder (4 s) slices one into its
//!   material's sheets (5, two fewer when destroyed); a station map statue takes the rest of the
//!   map with it and cannot be unfastened.
// shortcut: no crayon recolouring of curtains, no mechanical (shuttle button) curtains; mirrors
// offer hair only (no beards: the body has no facial hair; magic mirrors' race, sex, eyes and
// name changes are not offered), no reflections, no revenant prisons, no bald trait; sign items
// are not hung back on walls (no wallframes) and keep no integrity, no pen redesign of blank
// signs, no fountain pen engraving of plaques (no text prompt for items here); statues: no
// carving from material blocks, no custom material statues, no beauty or art elements, no
// rotation, no emissive eyes. Add each with its mechanic.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmName, DmObj, Opaque};
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::Span;
use ss13_shared::protocol::ClickModifiers;
use ss13_shared::{Appearance, Layer, TilePos};
use ss13_tgui::interfaces::list_input::{ListInputData, ListInputUi};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_tgui::{UiInterface, UiStatus};

use crate::construction::Job;
use crate::health::Stat;
use crate::human::{HAIR_STYLE_NAMES, HAIR_STYLES, HumanLook};
use crate::medical::surgery::ListAct;
use crate::medical::{name_of, visible};
use crate::obj_damage::{Integrity, set_anchored};
use crate::sound::{PlaySound, playsound};
use crate::tools::{Actor, DoAfter, ToolKind, tell, tool_of, use_tool};

const CURTAIN: &str = "/obj/structure/curtain";
const CURTAIN_CLOTH: &str = "/obj/structure/curtain/cloth";
const CURTAIN_BOUNTY: &str = "/obj/structure/curtain/bounty";
const CURTAIN_START_CLOSED: &str = "/obj/structure/curtain/bounty/start_closed";
const MIRROR: &str = "/obj/structure/mirror";
const MIRROR_BROKEN: &str = "/obj/structure/mirror/broken";
const HERETIC_MIRROR: &str = "/obj/structure/mirror/magic/lesser/heretic";
const MIRROR_FRAME: &str = "/obj/item/wallframe/mirror";
const MIRROR_FRAME_BROKEN: &str = "/obj/item/wallframe/mirror/broken";
const SHARD: &str = "/obj/item/shard";
const SIGN: &str = "/obj/structure/sign";
const SIGN_BLANK: &str = "/obj/structure/sign/blank";
const SIGN_ITEM: &str = "/obj/item/sign";
/// `buildable_sign = FALSE` (and the poster, painting and picture frame branches of
/// `/obj/structure/sign`, which come off the wall their own way).
const FIXED_SIGNS: [&str; 5] = [
    "/obj/structure/sign/flag",
    "/obj/structure/sign/logo",
    "/obj/structure/sign/poster",
    "/obj/structure/sign/painting",
    "/obj/structure/sign/picture_frame",
];
const POSTER: &str = "/obj/structure/sign/poster";
const PLAQUE: &str = "/obj/structure/plaque";
const PLAQUE_ITEM: &str = "/obj/item/plaque";
const STATUE: &str = "/obj/structure/statue";
const STATION_MAP: &str = "/obj/structure/statue/station_map";
const CLOTH: &str = "/obj/item/stack/sheet/cloth";
const PLASTIC: &str = "/obj/item/stack/sheet/plastic";
const RODS: &str = "/obj/item/stack/rods";
const CURTAIN_SOUND: &str = "sound/effects/curtain.ogg";
const RATCHET: &str = "sound/items/tools/ratchet.ogg";
const DECONSTRUCT: &str = "sound/items/deconstruct.ogg";
/// `SFX_SHATTER`.
const SHATTER: &str = "shatter";
/// `SIGN_LAYER`, `WALL_OBJ_LAYER`.
const SIGN_LAYER: f32 = 3.4;
const WALL_OBJ_LAYER: f32 = 4.25;
/// `default_unfasten_wrench(user, tool, time = 5 SECONDS)`, `wirecutter_act`'s
/// `use_tool(src, user, 50)`, deciseconds.
const CURTAIN_DS: f32 = 50.0;
/// `mirror/wrench_act_secondary`: 3 s; `welder_act`: `use_tool(src, user, 10)`.
const MIRROR_DETACH_DS: f32 = 30.0;
const MIRROR_REPAIR_DS: f32 = 10.0;
/// `sign`/`plaque` `wrench_act` and `welder_act`: 4 s.
const SIGN_DS: f32 = 40.0;
/// `statue/wrench_act`: `default_unfasten_wrench` (2 s); `welder_act`: 4 s.
const STATUE_WRENCH_DS: f32 = 20.0;
const STATUE_WELD_DS: f32 = 40.0;
/// `SHEET_MATERIAL_AMOUNT * 5` of the material statues, in sheets.
const STATUE_SHEETS: u32 = 5;
/// The material statues (`custom_materials`), by subtype, and the sheets of their material.
const STATUE_MATERIALS: [(&str, &[(&str, u32)]); 10] = [
    (
        "/uranium",
        &[("/obj/item/stack/sheet/mineral/uranium", STATUE_SHEETS)],
    ),
    (
        "/plasma",
        &[("/obj/item/stack/sheet/mineral/plasma", STATUE_SHEETS)],
    ),
    (
        "/gold",
        &[("/obj/item/stack/sheet/mineral/gold", STATUE_SHEETS)],
    ),
    (
        "/silver",
        &[("/obj/item/stack/sheet/mineral/silver", STATUE_SHEETS)],
    ),
    (
        "/diamond",
        &[("/obj/item/stack/sheet/mineral/diamond", STATUE_SHEETS)],
    ),
    (
        "/bananium",
        &[("/obj/item/stack/sheet/mineral/bananium", STATUE_SHEETS)],
    ),
    (
        "/sandstone",
        &[("/obj/item/stack/sheet/mineral/sandstone", STATUE_SHEETS)],
    ),
    (
        "/snow",
        &[("/obj/item/stack/sheet/mineral/snow", STATUE_SHEETS)],
    ),
    (
        "/bronze",
        &[("/obj/item/stack/sheet/bronze", STATUE_SHEETS)],
    ),
    (
        "/elder_atmosian",
        &[
            ("/obj/item/stack/sheet/iron", 30),
            ("/obj/item/stack/sheet/mineral/metal_hydrogen", 20),
            ("/obj/item/stack/sheet/mineral/zaukerite", 15),
        ],
    ),
];
/// The hairstyle list's "Bald" (`SSaccessories.hairstyles_list`), then [`HAIR_STYLE_NAMES`].
const BALD: &str = "Без волос";

/// `/obj/structure/curtain`: `open`, `opaque_closed` (`icon_type` is its map sprite's prefix).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Curtain {
    pub open: bool,
    pub opaque_closed: bool,
}

/// `/obj/structure/mirror`: `broken`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mirror {
    pub broken: bool,
}

/// The timed actions of decor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecorJob {
    /// `curtain/wrench_act`, `statue/wrench_act` (`default_unfasten_wrench`).
    Unfasten,
    /// `curtain/wirecutter_act`.
    CutCurtain,
    /// `mirror/wrench_act_secondary`.
    DetachMirror,
    /// `mirror/welder_act`.
    MendMirror,
    /// `sign`/`plaque` `wrench_act`.
    Unmount,
    /// `sign`/`plaque` `welder_act`.
    Repair,
    /// `statue/welder_act`.
    SliceStatue,
}

/// A hairstyle list waiting for a mirror user's pick (tg `tgui_input_list` of `change_hair`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct MirrorChoice {
    pub mirror: Entity,
}

/// The server side of `ListInputWindow` for a mirror's hairstyles.
pub struct MirrorListWindow;

impl UiInterface for MirrorListWindow {
    const NAME: &'static str = ListInputUi::NAME;
    type Data = ListInputData;
    type StaticData = ();
}

impl ServerUi for MirrorListWindow {
    type Source = &'static MirrorChoice;
    type UserParam = ();
    type Act = ListAct;

    fn static_data(_: &MirrorChoice) {}

    fn data(_: &MirrorChoice, _: Entity, _: &()) -> ListInputData {
        let items = hair_names();
        ListInputData {
            title: "Уход за собой".into(),
            message: "Выберите причёску".into(),
            init_value: items.first().cloned().unwrap_or_default(),
            items,
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

fn hair_names() -> Vec<String> {
    std::iter::once(BALD)
        .chain(HAIR_STYLE_NAMES)
        .map(str::to_owned)
        .collect()
}

/// The hairstyle a list entry names: `Some(None)` bald, `None` not an entry.
fn hair_of(entry: &str) -> Option<Option<&'static str>> {
    if entry == BALD {
        return Some(None);
    }
    let i = HAIR_STYLE_NAMES.iter().position(|n| *n == entry)?;
    Some(Some(HAIR_STYLES[i]))
}

pub struct DecorPlugin;

impl Plugin for DecorPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `attack_hand` of curtains, mirrors, signs and plaques.
        app.add_click_handler(ClickPhase::EmptyHand, 3150, |w, c| {
            crate::decor::interact(w, c.client, c.mob, c.target)
        });
        // tg tool acts and `item_interaction` of curtains, mirrors, signs, plaques, statues.
        app.add_click_handler(ClickPhase::HeldOnTarget, 3450, |w, c| {
            c.held.is_some_and(|h| {
                crate::decor::item_interaction(w, c.client, c.mob, h, c.target, c.modifiers)
            })
        });
        app.register_ui::<MirrorListWindow>()
            .add_observer(add_decor)
            .add_systems(
                Update,
                (
                    pick_hair.in_set(UiSystems::Handle),
                    close_stale_lists.in_set(UiSystems::StatusHook),
                ),
            );
    }
}

/// `curtain/Initialize` (a `start_closed` one toggles shut), `mirror/Initialize`.
fn add_decor(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(dm) = objs.get(add.entity) else {
        return;
    };
    let entity = add.entity;
    if is_under(&dm.path, CURTAIN) {
        let opaque_closed = is_under(&dm.path, CURTAIN_CLOTH) || is_under(&dm.path, CURTAIN_BOUNTY);
        commands.entity(entity).insert(Curtain {
            open: true,
            opaque_closed,
        });
        if is_under(&dm.path, CURTAIN_START_CLOSED) {
            commands.queue(move |world: &mut World| toggle_curtain(world, entity));
        }
    } else if is_under(&dm.path, MIRROR) {
        commands.entity(entity).insert(Mirror {
            broken: is_under(&dm.path, MIRROR_BROKEN) || dm.broken,
        });
    }
}

/// The structures [`crate::obj_damage`] gives integrity to here: they can be hit, mirrors
/// break, signs and plaques are repaired.
pub fn breakable(path: &str) -> bool {
    [CURTAIN, MIRROR, SIGN, PLAQUE, STATUE]
        .iter()
        .any(|root| is_under(path, root))
}

fn path(world: &World, e: Entity) -> Option<&str> {
    world.get::<DmObj>(e).map(|o| o.path.as_str())
}

fn sound(world: &mut World, at: Entity, file: &str, vol: f32) {
    if let Some(&pos) = world.get::<TilePos>(at) {
        playsound(world, PlaySound::new(pos, file, vol, true));
    }
}

fn is_sign(path: &str) -> bool {
    is_under(path, SIGN) || is_under(path, PLAQUE)
}

fn buildable_sign(path: &str) -> bool {
    is_under(path, PLAQUE)
        || (is_under(path, SIGN) && !FIXED_SIGNS.iter().any(|r| is_under(path, r)))
}

/// `curtain/toggle`: open is see-through on the sign layer; closed hangs on the wall-object layer
/// and blocks sight if `opaque_closed`.
fn toggle_curtain(world: &mut World, curtain: Entity) {
    let Some(state) = world.get::<Curtain>(curtain).copied() else {
        return;
    };
    // `icon_type`: "bathroom", "bounty", "cur_fancy" of "<icon_type>-open".
    let icon_type = world
        .get::<DmAtom>(curtain)
        .map(|a| {
            let s = a.icon_state.as_str();
            s.rsplit_once('-').map_or(s, |(base, _)| base).to_owned()
        })
        .unwrap_or_else(|| "bathroom".to_owned());
    let open = !state.open;
    if let Some(mut c) = world.get_mut::<Curtain>(curtain) {
        c.open = open;
    }
    let layer = if open { SIGN_LAYER } else { WALL_OBJ_LAYER };
    if let Some(mut appearance) = world.get_mut::<Appearance>(curtain)
        && appearance.layer != Layer(layer)
    {
        appearance.layer = Layer(layer);
    }
    if open {
        world.entity_mut(curtain).remove::<Opaque>();
    } else if state.opaque_closed {
        world.entity_mut(curtain).insert(Opaque);
    }
    let icon_state = format!("{icon_type}-{}", if open { "open" } else { "closed" });
    crate::obj_damage::set_icon_state(world, curtain, &icon_state);
}

/// tg `attack_hand` of the decor. True when handled.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<Curtain>(target).is_some() {
        sound(world, target, CURTAIN_SOUND, 50.0);
        toggle_curtain(world, target);
        return true;
    }
    if let Some(mirror) = world.get::<Mirror>(target).copied() {
        // `if(. || !ishuman(user) || broken) return TRUE`.
        if !mirror.broken && world.get::<HumanLook>(mob).is_some() {
            world
                .entity_mut(mob)
                .insert(MirrorChoice { mirror: target });
            world.write_message(OpenUi::new::<MirrorListWindow>(
                client,
                mob,
                mob,
                "Уход за собой",
            ));
        }
        return true;
    }
    if path(world, target).is_some_and(|p| is_sign(p) && !is_under(p, POSTER)) {
        // `user.examinate(src)`; posters are torn down instead (their own group).
        let (name, lines) = crate::examine::examine_lines(world, mob, target);
        for message in crate::examine::examine_messages(&name, lines) {
            world.write_message(SendChat {
                clients: vec![client],
                message,
            });
        }
        return true;
    }
    false
}

/// tg tool acts and `item_interaction` of the decor. True when handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    let Some(path) = path(world, target).map(str::to_owned) else {
        return false;
    };
    if !breakable(&path) {
        return false;
    }
    // Tool acts run only out of combat mode.
    if world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0)
    {
        return false;
    }
    let Some(tool) = tool_of(world, held).map(|(kind, _)| kind) else {
        return false;
    };
    let actor = Actor {
        client,
        mob,
        item: Some(held),
    };
    let name = name_of(world, target);
    let user = name_of(world, mob);
    let start = |world: &mut World, ds: f32, amount: u32, job: DecorJob| {
        use_tool(world, actor, target, ds, amount, Job::Decor(job));
    };
    if is_under(&path, CURTAIN) {
        return match tool {
            ToolKind::Wrench if !modifiers.right => {
                sound(world, target, RATCHET, 50.0);
                start(world, CURTAIN_DS, 0, DecorJob::Unfasten);
                true
            }
            ToolKind::Wirecutters if !modifiers.right => {
                if anchored(world, target) {
                    return true;
                }
                visible(
                    world,
                    mob,
                    &format!("Вы начинаете разрезать {name}."),
                    &format!("{user} разрезает {name}."),
                    Span::Warning,
                );
                start(world, CURTAIN_DS, 0, DecorJob::CutCurtain);
                true
            }
            _ => false,
        };
    }
    if is_under(&path, MIRROR) {
        let broken = world.get::<Mirror>(target).is_some_and(|m| m.broken);
        return match tool {
            ToolKind::Wrench if modifiers.right => {
                // `deconstructable`: the heretic's mirror is held by magic.
                if is_under(&path, HERETIC_MIRROR) {
                    tell(world, client, Span::Warning, "магия не даёт снять!");
                    return true;
                }
                visible(
                    world,
                    mob,
                    &format!("Вы начинаете снимать {name}..."),
                    &format!("{user} начинает снимать {name}..."),
                    Span::Notice,
                );
                sound(world, target, RATCHET, 50.0);
                start(world, MIRROR_DETACH_DS, 0, DecorJob::DetachMirror);
                true
            }
            ToolKind::Welder if !modifiers.right => {
                if broken && !is_under(&path, HERETIC_MIRROR) {
                    tell(world, client, Span::Notice, "ремонт...");
                    start(world, MIRROR_REPAIR_DS, 1, DecorJob::MendMirror);
                }
                true
            }
            _ => false,
        };
    }
    if is_sign(&path) {
        return match tool {
            ToolKind::Wrench if !modifiers.right => {
                if !buildable_sign(&path) {
                    return true;
                }
                visible(
                    world,
                    mob,
                    &format!("Вы начинаете откручивать {name}."),
                    &format!("{user} начинает снимать {name}..."),
                    Span::Notice,
                );
                sound(world, target, RATCHET, 50.0);
                start(world, SIGN_DS, 0, DecorJob::Unmount);
                true
            }
            ToolKind::Welder if !modifiers.right => {
                if !damaged(world, target) {
                    tell(
                        world,
                        client,
                        Span::Warning,
                        format!("{name} в идеальном состоянии."),
                    );
                    return true;
                }
                visible(
                    world,
                    mob,
                    &format!("Вы начинаете чинить {name}."),
                    &format!("{user} начинает чинить {name}..."),
                    Span::Notice,
                );
                start(world, SIGN_DS, 1, DecorJob::Repair);
                true
            }
            _ => false,
        };
    }
    // Statues.
    match tool {
        ToolKind::Wrench if !modifiers.right => {
            // BandaStation's station map: `wrench_act` returns FALSE.
            if is_under(&path, STATION_MAP) {
                return false;
            }
            sound(world, target, RATCHET, 50.0);
            start(world, STATUE_WRENCH_DS, 0, DecorJob::Unfasten);
            true
        }
        ToolKind::Welder if !modifiers.right => {
            tell(world, client, Span::Notice, "разрезание...");
            start(world, STATUE_WELD_DS, 1, DecorJob::SliceStatue);
            true
        }
        _ => false,
    }
}

fn anchored(world: &World, e: Entity) -> bool {
    world.get::<crate::obj_damage::Anchored>(e).map_or_else(
        || world.get::<DmObj>(e).is_some_and(|o| o.anchored),
        |a| a.0,
    )
}

fn damaged(world: &World, e: Entity) -> bool {
    let max = world.get::<DmObj>(e).map_or(0.0, |o| o.max_integrity);
    world.get::<Integrity>(e).is_some_and(|i| i.current < max)
}

/// End of a [`DecorJob`] (`crate::tools` checked nobody moved).
pub fn complete(world: &mut World, mob: Entity, action: &DoAfter, job: DecorJob) {
    let target = action.target;
    let client = action.client;
    let Some(path) = path(world, target).map(str::to_owned) else {
        return;
    };
    let consume = |world: &mut World| {
        action
            .item
            .is_none_or(|i| crate::tools::consume(world, i, action.amount))
    };
    let name = name_of(world, target);
    let user = name_of(world, mob);
    match job {
        DecorJob::Unfasten => {
            // `default_unfasten_wrench`.
            let to = !anchored(world, target);
            set_anchored(world, target, to);
            let text = if to {
                format!("Вы закрепляете {name}.")
            } else {
                format!("Вы откручиваете {name}.")
            };
            tell(world, client, Span::Notice, text);
        }
        DecorJob::CutCurtain => {
            if anchored(world, target) {
                return;
            }
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы разрезаете {name}."),
            );
            crate::obj_damage::deconstruct(world, target, true);
        }
        DecorJob::DetachMirror => {
            visible(
                world,
                mob,
                &format!("Вы снимаете {name} со стены."),
                &format!("{user} снимает {name}!"),
                Span::Notice,
            );
            sound(world, target, DECONSTRUCT, 50.0);
            crate::obj_damage::deconstruct(world, target, true);
        }
        DecorJob::MendMirror => {
            if !world.get::<Mirror>(target).is_some_and(|m| m.broken) || !consume(world) {
                return;
            }
            world.entity_mut(target).insert(Mirror { broken: false });
            if let Some(mut integrity) = world.get_mut::<Integrity>(target) {
                integrity.broken = false;
            }
            let initial = world
                .get::<DmAtom>(target)
                .map(|a| a.icon_state.clone())
                .filter(|s| s != "mirror_broke")
                .unwrap_or_else(|| "mirror".to_owned());
            crate::obj_damage::set_icon_state(world, target, &initial);
            tell(world, client, Span::Notice, "отремонтировано");
        }
        DecorJob::Unmount => {
            sound(world, target, DECONSTRUCT, 50.0);
            visible(
                world,
                mob,
                &format!("Вы откручиваете {name}."),
                &format!("{user} откручивает {name}."),
                Span::Notice,
            );
            unmount(world, target, &path);
        }
        DecorJob::Repair => {
            if !damaged(world, target) || !consume(world) {
                return;
            }
            let max = world.get::<DmObj>(target).map_or(0.0, |o| o.max_integrity);
            if let Some(mut integrity) = world.get_mut::<Integrity>(target) {
                integrity.current = max;
            }
            visible(
                world,
                mob,
                &format!("Вы заканчиваете чинить {name}."),
                &format!("{user} заканчивает чинить {name}."),
                Span::Notice,
            );
        }
        DecorJob::SliceStatue => {
            if !consume(world) {
                return;
            }
            crate::obj_damage::deconstruct(world, target, true);
        }
    }
}

/// `sign/wrench_act` (`deconstruct(TRUE)`) and `plaque/wrench_act`: the item version, the
/// structure gone.
fn unmount(world: &mut World, target: Entity, path: &str) {
    if !is_under(path, PLAQUE) {
        crate::obj_damage::deconstruct(world, target, true);
        return;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    sign_item(world, target, path, pos);
    world.despawn(target);
    crate::construction::tile_changed(world, pos);
}

/// The item a sign or plaque comes off the wall as, with the structure's name, description and
/// sprite (a blank one stays blank), on `pos`.
fn sign_item(world: &mut World, target: Entity, path: &str, pos: TilePos) {
    let plaque = is_under(path, PLAQUE);
    let item_path = if plaque { PLAQUE_ITEM } else { SIGN_ITEM };
    let blank = path == SIGN_BLANK || path == PLAQUE;
    let dm_name = world.get::<DmName>(target).cloned();
    let icon_state = world.get::<Appearance>(target).map(|a| a.icon_state);
    let Some(item) = crate::construction::spawn_obj(world, item_path, pos) else {
        return;
    };
    if !blank && let Some(dm_name) = dm_name {
        let desc = if plaque {
            dm_name.desc.clone()
        } else {
            format!("{} Это можно повесить на стену.", dm_name.desc)
        };
        if let Some(mut name) = world.get_mut::<Name>(item) {
            *name = Name::new(crate::obj_damage::strip(&dm_name.name).to_owned());
        }
        world.entity_mut(item).insert(DmName {
            name: dm_name.name,
            desc,
        });
    }
    if let Some(state) = icon_state
        && let Some(mut appearance) = world.get_mut::<Appearance>(item)
    {
        appearance.icon_state = state;
    }
}

/// tg `atom_break` of a mirror: "mirror_broke", the shatter. True when `target` is a mirror.
pub fn atom_break(world: &mut World, target: Entity) -> bool {
    let Some(mirror) = world.get::<Mirror>(target).copied() else {
        return false;
    };
    if mirror.broken {
        return true;
    }
    crate::obj_damage::set_icon_state(world, target, "mirror_broke");
    sound(world, target, SHATTER, 70.0);
    world.entity_mut(target).insert(Mirror { broken: true });
    true
}

/// `atom_deconstruct` of the decor: what it leaves. True when `dm` was decor.
pub(crate) fn deconstruct(
    world: &mut World,
    target: Entity,
    dm: &DmObj,
    pos: TilePos,
    disassembled: bool,
) -> bool {
    let path = dm.path.as_str();
    if is_under(path, CURTAIN) {
        // `curtain/atom_deconstruct`; `cloth/atom_deconstruct`.
        if is_under(path, CURTAIN_CLOTH) {
            crate::construction::spawn_stack(world, CLOTH, pos, 4);
        } else {
            crate::construction::spawn_stack(world, CLOTH, pos, 2);
            crate::construction::spawn_stack(world, PLASTIC, pos, 2);
        }
        crate::construction::spawn_stack(world, RODS, pos, 1);
        return true;
    }
    if is_under(path, MIRROR) {
        let broken = world.get::<Mirror>(target).is_some_and(|m| m.broken);
        // `heretic/atom_deconstruct` never gives the frame back.
        let leaves = if !disassembled || is_under(path, HERETIC_MIRROR) {
            SHARD
        } else if broken {
            MIRROR_FRAME_BROKEN
        } else {
            MIRROR_FRAME
        };
        crate::construction::spawn_obj(world, leaves, pos);
        return true;
    }
    if is_under(path, STATUE) {
        if is_under(path, STATION_MAP) {
            // BandaStation: the map's other pieces within 3 tiles go with it.
            let others: Vec<Entity> = world
                .query::<(Entity, &DmObj, &TilePos)>()
                .iter(world)
                .filter(|(e, o, p)| {
                    *e != target
                        && is_under(&o.path, STATION_MAP)
                        && p.chebyshev(pos).is_some_and(|d| d <= 3)
                })
                .map(|(e, ..)| e)
                .collect();
            for e in others {
                if let Some(&p) = world.get::<TilePos>(e) {
                    world.despawn(e);
                    crate::construction::tile_changed(world, p);
                }
            }
            return true;
        }
        // `statue/atom_deconstruct`: two sheets fewer when destroyed.
        let sheets = STATUE_MATERIALS
            .iter()
            .find(|(sub, _)| is_under(path, &format!("{STATUE}{sub}")))
            .map_or(&[][..], |(_, sheets)| *sheets);
        for &(stack, amount) in sheets {
            let amount = if disassembled {
                amount
            } else {
                amount.saturating_sub(2)
            };
            if amount > 0 {
                crate::construction::spawn_stack(world, stack, pos, amount);
            }
        }
        return true;
    }
    if is_under(path, PLAQUE) {
        // A plaque has no `atom_deconstruct`: destroyed, it leaves nothing.
        return true;
    }
    if is_under(path, SIGN) {
        // `sign/atom_deconstruct`, wrenched or destroyed: the sign item (posters, paintings and
        // the rest of `FIXED_SIGNS` leave nothing here).
        if buildable_sign(path) {
            sign_item(world, target, path, pos);
        }
        return true;
    }
    false
}

/// A mirror user picked a hairstyle (or closed the list): tg `change_hair` after
/// `can_use_mirror` at the moment of effect.
fn pick_hair(world: &mut World, mut acts: Local<MessageCursor<UiActed<MirrorListWindow>>>) {
    let picked: Vec<(Entity, ListAct)> = acts
        .read(world.resource::<Messages<UiActed<MirrorListWindow>>>())
        .filter(|a| a.actor == a.source)
        .map(|a| (a.actor, a.act.clone()))
        .collect();
    for (mob, act) in picked {
        let Some(choice) = world.get::<MirrorChoice>(mob).copied() else {
            continue;
        };
        world.entity_mut(mob).remove::<MirrorChoice>();
        let ListAct::Submit(entry) = act else {
            continue;
        };
        let Some(style) = hair_of(&entry) else {
            continue;
        };
        if !can_use_mirror(world, mob, choice.mirror) {
            continue;
        }
        let Some(mut look) = world.get_mut::<HumanLook>(mob) else {
            continue;
        };
        let style = style.map(str::to_owned);
        if look.hair_style == style {
            continue;
        }
        look.hair_style = style;
        crate::inventory::refresh_look(world, mob);
    }
}

/// `can_use_mirror`: the mirror whole and in reach, the user conscious and able to act.
fn can_use_mirror(world: &World, mob: Entity, mirror: Entity) -> bool {
    let whole = world.get::<Mirror>(mirror).is_some_and(|m| !m.broken);
    let near = match (world.get::<TilePos>(mob), world.get::<TilePos>(mirror)) {
        (Some(a), Some(b)) => a.chebyshev(*b).is_some_and(|d| d <= 1),
        _ => false,
    };
    whole
        && near
        && world.get::<Stat>(mob) == Some(&Stat::Conscious)
        && !crate::status_effects::incapacitation(world, mob).incapacitated
}

fn close_stale_lists(
    mut windows: Query<&mut UiWindow, With<ss13_tgui::server::UiKind<MirrorListWindow>>>,
    choices: Query<(), With<MirrorChoice>>,
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
    fn hair_entries_map_back_to_styles() {
        let names = hair_names();
        assert_eq!(names[0], BALD);
        assert_eq!(names.len(), HAIR_STYLES.len() + 1);
        assert_eq!(hair_of(BALD), Some(None));
        assert_eq!(hair_of(HAIR_STYLE_NAMES[2]), Some(Some(HAIR_STYLES[2])));
        assert_eq!(hair_of("Ирокез из космоса"), None);
    }

    #[test]
    fn fixed_signs_stay_on_the_wall() {
        assert!(buildable_sign(
            "/obj/structure/sign/warning/directional/north"
        ));
        assert!(buildable_sign("/obj/structure/plaque/static_plaque"));
        assert!(!buildable_sign("/obj/structure/sign/flag/nanotrasen"));
        assert!(!buildable_sign("/obj/structure/sign/poster/random"));
        assert!(breakable("/obj/structure/statue/gold/hos"));
        assert!(!breakable("/obj/structure/table"));
    }
}
