//! Curtains, mirrors, signs, plaques and statues (server), after tg
//! `code/game/objects/structures/curtains.dm`, `mirror.dm`, `signs/_signs.dm`,
//! `plaques/_plaques.dm` and `code/modules/art/statues.dm` (with BandaStation's station map
//! statues, `modular_bandastation/objects/code/structures/statues.dm`).
//!
//! - Curtains ([`Curtain`]): an empty hand draws or opens them (the shower curtain stays
//!   see-through, cloth and bounty curtains block sight when closed; closed ones hang above
//!   mobs); a wrench (5 s) fastens or unfastens them, wirecutters (5 s) cut an unfastened one into
//!   cloth, plastic and a rod.
//! - Mirrors ([`Mirror`]): a human's empty hand opens the mirror's options
//!   (`crate::service::barber::open_mirror`: hair and beard, a magic mirror's race, sex, eyes
//!   and name); a mirror breaks at half integrity ("mirror_broke", the shatter), a lit welder (1 s)
//!   mends a broken one, a wrench (right click, 3 s) takes it off the wall as its frame
//!   (broken or not), destroyed it leaves a shard.
//! - Signs and plaques: an empty hand examines them; a wrench (4 s) takes one off the wall as an
//!   item with its name, description and look (`/obj/item/sign`, `/obj/item/plaque`; flags,
//!   logos, posters, paintings and picture frames stay); a lit welder (4 s) repairs a damaged one.
//! - Statues: a wrench (2 s) fastens or unfastens them, a lit welder (4 s) slices one into its
//!   material's sheets (5, two fewer when destroyed); a station map statue takes the rest of the
//!   map with it and cannot be unfastened.
//! - Posters (`effects/posters/poster.dm`): an empty hand rips one (a ripped poster stays in its
//!   place); wirecutters take a whole one off as a rolled poster ([`RolledPoster`]) or clear a
//!   ripped one away; a rolled poster clicked on a wall in line with its user (3.7 s) hangs on
//!   that wall's face of the user's tile, one poster per face.
//! - Showcases (`showcase.dm`): a screwdriver loosens or tightens an unanchored one's screws, a
//!   wrench fastens or unfastens a screwed one (2 s), a crowbar (2 s) pries one into 4 iron.
// shortcut: no crayon recolouring of curtains, no mechanical (shuttle button) curtains; mirrors:
// no reflections, no revenant prisons, no omen for whoever breaks one; sign items
// are not hung back on walls (no wallframes) and keep no integrity, no pen redesign of blank
// signs, no fountain pen engraving of plaques (no text prompt for items here); statues: no
// carving from material blocks, no custom material statues, no beauty or art elements, no
// rotation, no emissive eyes. Posters: no glass shard traps, no random rolled posters from loot
// (only posters taken off a wall), no diagonal-wall check, no hanging animation (the poster
// appears when the 3.7 s are over). Showcases: no examine line of the screws' state. Add each with
// its mechanic.

use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmAtom, DmName, DmObj, DmTurfBuild, IconIndex, Opaque};
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::Span;
use ss13_shared::protocol::ClickModifiers;
use ss13_shared::{Appearance, Dir, IconStateId, Layer, TilePos};

use crate::construction::Job;
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
/// BandaStation's flags (`objects/code/structures/flags.dm`) and their folded items.
const FLAG: &str = "/obj/structure/sign/flag";
const FOLDED_FLAG: &str = "/obj/item/sign/flag";
const POSTER_RIPPED: &str = "/obj/structure/sign/poster/ripped";
const POSTER_OFFICIAL: &str = "/obj/structure/sign/poster/official";
const POSTER_ITEM: &str = "/obj/item/poster";
/// `/obj/item/poster/traitor`: its `poster_type` is the random seditious poster.
pub const TRAITOR_POSTER_ITEM: &str = "/obj/item/poster/traitor";
const POSTER_TRAITOR: &str = "/obj/structure/sign/poster/traitor";
const POSTER_TRAITOR_RANDOM: &str = "/obj/structure/sign/poster/traitor/random";
/// `random_basetype = /obj/structure/sign/poster/traitor`: its subtypes without `never_random`.
pub const TRAITOR_POSTERS: [&str; 9] = [
    "/obj/structure/sign/poster/traitor/small_brain",
    "/obj/structure/sign/poster/traitor/lick_supermatter",
    "/obj/structure/sign/poster/traitor/cloning",
    "/obj/structure/sign/poster/traitor/ai_rights",
    "/obj/structure/sign/poster/traitor/metroid",
    "/obj/structure/sign/poster/traitor/low_pay",
    "/obj/structure/sign/poster/traitor/look_up",
    "/obj/structure/sign/poster/traitor/accidents",
    "/obj/structure/sign/poster/traitor/starve",
];
const POSTER_RIP_SOUND: &str = "sound/items/poster/poster_ripped.ogg";
const POSTER_HANG_SOUND: &str = "sound/items/poster/poster_being_created.ogg";
const WIRECUTTER_SOUND: &str = "sound/items/tools/wirecutter.ogg";
const SCREWDRIVER_SOUND: &str = "sound/items/tools/screwdriver.ogg";
const CROWBAR_SOUND: &str = "sound/items/tools/crowbar.ogg";
const CLOSED_TURF: &str = "/turf/closed";
/// `PLACE_SPEED`: the poster hanging `do_after`, deciseconds.
const POSTER_PLACE_DS: f32 = 37.0;
const SHOWCASE: &str = "/obj/structure/showcase";
const IRON: &str = "/obj/item/stack/sheet/iron";
/// `showcase/crowbar_act`: `use_tool(src, user, 2 SECONDS)`; its iron.
const SHOWCASE_PRY_DS: f32 = 20.0;
const SHOWCASE_IRON: u32 = 4;
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

/// A poster rolled up off a wall: tg `/obj/item/poster` holding its `poster_structure` (the
/// structure's type, its rolled look and its name).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct RolledPoster {
    pub structure: String,
    pub icon_state: IconStateId,
    pub name: DmName,
}

/// `/obj/structure/showcase` at `SHOWCASE_SCREWDRIVERED` (absent: `SHOWCASE_CONSTRUCTED`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShowcaseUnscrewed;

/// The timed actions of decor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecorJob {
    /// `showcase/crowbar_act`.
    PryShowcase,
    /// `poster/interact_with_atom`: hanging a rolled poster on a wall.
    HangPoster,
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

pub struct DecorPlugin;

impl Plugin for DecorPlugin {
    fn build(&self, app: &mut App) {
        // Click chain hooks (`crate::click_handlers`).
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // tg `attack_hand` of curtains, mirrors, signs and plaques.
        app.add_click_handler(ClickPhase::EmptyHand, 3150, |w, c| {
            (c.modifiers.right && crate::decor::fold_flag(w, c.mob, c.target))
                || crate::decor::interact(w, c.client, c.mob, c.target)
        });
        // tg tool acts and `item_interaction` of curtains, mirrors, signs, plaques, statues.
        app.add_click_handler(ClickPhase::HeldOnTarget, 3450, |w, c| {
            c.held.is_some_and(|h| {
                crate::decor::item_interaction(w, c.client, c.mob, h, c.target, c.modifiers)
            })
        });
        app.add_observer(add_decor).add_observer(add_traitor_poster);
    }
}

/// `/obj/item/poster/traitor`: a rolled random seditious poster.
fn add_traitor_poster(
    add: On<Add<ss13_mapload::DmItem>>,
    items: Query<&ss13_mapload::DmItem>,
    mut commands: Commands,
) {
    if items
        .get(add.entity)
        .is_ok_and(|i| i.path == TRAITOR_POSTER_ITEM)
    {
        commands.entity(add.entity).insert_if_new(RolledPoster {
            structure: POSTER_TRAITOR_RANDOM.to_owned(),
            icon_state: IconStateId(0),
            name: DmName::default(),
        });
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
        // `if(. || !ishuman(user) || broken) return TRUE`, then the options
        // (`crate::service::barber`).
        if !mirror.broken {
            crate::service::barber::open_mirror(world, client, mob, target);
        }
        return true;
    }
    if path(world, target).is_some_and(|p| is_under(p, POSTER)) {
        tear_poster(world, client, mob, target);
        return true;
    }
    if path(world, target).is_some_and(is_sign) {
        // `user.examinate(src)`.
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

/// BandaStation `flag/attack_hand_secondary` -> `fold`: the flag comes off the wall as its
/// `foldable_type` (`/obj/item/sign/flag/<kind>`, the same subpath) in the hands. True when
/// `target` was a flag.
pub fn fold_flag(world: &mut World, mob: Entity, target: Entity) -> bool {
    let Some(path) = path(world, target).map(str::to_owned) else {
        return false;
    };
    let Some(kind) = path.strip_prefix(FLAG) else {
        return false;
    };
    if !(kind.is_empty() || kind.starts_with('/')) {
        return false;
    }
    // `can_perform_action(NEED_HANDS | NEED_DEXTERITY)`.
    if world.get::<crate::restraints::Handcuffed>(mob).is_some()
        || crate::status_effects::incapacitation(world, mob).incapacitated
    {
        return true;
    }
    let Some(&pos) = world.get::<TilePos>(target) else {
        return true;
    };
    // `MAPPING_DIRECTIONAL_HELPERS` subtypes fold like their flag; an unknown kind like the base.
    let kind = kind.split("/directional").next().unwrap_or_default();
    let mut item_path = format!("{FOLDED_FLAG}{kind}");
    let item = loop {
        if let Some(item) = crate::construction::spawn_obj(world, &item_path, pos) {
            break Some(item);
        }
        match item_path.rfind('/') {
            Some(cut) if item_path.len() > FOLDED_FLAG.len() => item_path.truncate(cut),
            _ => break None,
        }
    };
    let Some(item) = item else {
        return true;
    };
    let (name, user) = (name_of(world, target), name_of(world, mob));
    visible(
        world,
        mob,
        &format!("Вы складываете {name}."),
        &format!("{user} складывает {name}."),
        Span::Notice,
    );
    crate::cooking::give_to(world, mob, target, item);
    remove(world, target);
    true
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
    if world.get::<RolledPoster>(held).is_some() && closed_turf(world, target) {
        start_hanging(world, client, mob, held, target);
        return true;
    }
    let Some(path) = path(world, target).map(str::to_owned) else {
        return false;
    };
    if !breakable(&path) && !is_under(&path, SHOWCASE) {
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
    if is_under(&path, SHOWCASE) {
        if modifiers.right {
            return false;
        }
        let unscrewed = world.get::<ShowcaseUnscrewed>(target).is_some();
        return match tool {
            ToolKind::Screwdriver => {
                if anchored(world, target) {
                    return false;
                }
                sound(world, target, SCREWDRIVER_SOUND, 100.0);
                if unscrewed {
                    world.entity_mut(target).remove::<ShowcaseUnscrewed>();
                    tell(
                        world,
                        client,
                        Span::Notice,
                        "Вы закручиваете винты витрины.",
                    );
                } else {
                    world.entity_mut(target).insert(ShowcaseUnscrewed);
                    tell(world, client, Span::Notice, "Вы откручиваете винты.");
                }
                true
            }
            ToolKind::Crowbar => {
                // tg pries a showcase apart in any state.
                sound(world, target, CROWBAR_SOUND, 100.0);
                start(world, SHOWCASE_PRY_DS, 0, DecorJob::PryShowcase);
                true
            }
            ToolKind::Wrench if !unscrewed => {
                sound(world, target, RATCHET, 50.0);
                start(world, STATUE_WRENCH_DS, 0, DecorJob::Unfasten);
                true
            }
            _ => false,
        };
    }
    if is_under(&path, POSTER) && tool == ToolKind::Wirecutters && !modifiers.right {
        // `poster/wirecutter_act`.
        sound(world, target, WIRECUTTER_SOUND, 100.0);
        if poster_ruined(world, target, &path) {
            tell(world, client, Span::Notice, "Вы убираете остатки плаката.");
            remove(world, target);
        } else {
            tell(
                world,
                client,
                Span::Notice,
                "Вы аккуратно снимаете плакат со стены.",
            );
            roll_up(world, mob, target, &path);
        }
        return true;
    }
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
            // BandaStation `flag/welder_act`: `return` (no repair).
            ToolKind::Welder if is_under(&path, FLAG) => false,
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
    // Its target is the wall, not a decor object.
    if job == DecorJob::HangPoster {
        finish_hanging(world, mob, action);
        return;
    }
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
            // `showcase/wrench_act` only while its screws are in.
            if world.get::<ShowcaseUnscrewed>(target).is_some() {
                return;
            }
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
        DecorJob::PryShowcase => {
            if !is_under(&path, SHOWCASE) {
                return;
            }
            tell(world, client, Span::Notice, "Вы разбираете витрину ломом.");
            if let Some(&pos) = world.get::<TilePos>(target) {
                crate::construction::spawn_stack(world, IRON, pos, SHOWCASE_IRON);
            }
            remove(world, target);
        }
        DecorJob::HangPoster => {}
    }
}

/// `qdel(src)` of a structure: gone, its tile updated.
fn remove(world: &mut World, target: Entity) {
    let Some(&pos) = world.get::<TilePos>(target) else {
        return;
    };
    world.despawn(target);
    crate::construction::tile_changed(world, pos);
}

/// `ruined`: a ripped poster, mapped or rolled from the random pool.
fn poster_ruined(world: &World, poster: Entity, path: &str) -> bool {
    if is_under(path, POSTER_RIPPED) {
        return true;
    }
    let (Some(atom), Some(appearance)) =
        (world.get::<DmAtom>(poster), world.get::<Appearance>(poster))
    else {
        return false;
    };
    world
        .get_resource::<IconIndex>()
        .and_then(|i| i.get(&atom.icon, "poster_ripped"))
        == Some(appearance.icon_state)
}

/// `poster/attack_hand` -> `tear_poster`: a ripped poster in its place.
fn tear_poster(world: &mut World, client: Entity, mob: Entity, poster: Entity) {
    let Some(path) = path(world, poster).map(str::to_owned) else {
        return;
    };
    if poster_ruined(world, poster, &path) {
        tell(world, client, Span::Warning, "уже порван!");
        return;
    }
    let (Some(&pos), Some(offset)) = (
        world.get::<TilePos>(poster),
        world.get::<Appearance>(poster).map(|a| a.pixel_offset),
    ) else {
        return;
    };
    let text = format!(
        "{} срывает {} одним решительным движением!",
        name_of(world, mob),
        name_of(world, poster)
    );
    visible(world, mob, &text, &text, Span::Notice);
    sound(world, poster, POSTER_RIP_SOUND, 100.0);
    remove(world, poster);
    if let Some(torn) = crate::construction::spawn_obj(world, POSTER_RIPPED, pos)
        && let Some(mut appearance) = world.get_mut::<Appearance>(torn)
    {
        appearance.pixel_offset = offset;
    }
}

/// `poster/roll_and_drop`: the rolled poster into the user's hands (or at their feet), the
/// structure gone.
fn roll_up(world: &mut World, mob: Entity, poster: Entity, path: &str) {
    let (Some(&at), Some(icon_state)) = (
        world.get::<TilePos>(mob),
        world.get::<Appearance>(poster).map(|a| a.icon_state),
    ) else {
        return;
    };
    let name = world.get::<DmName>(poster).cloned().unwrap_or(DmName {
        name: name_of(world, poster),
        desc: String::new(),
    });
    // A wanted poster's photo (`crate::records::posters`) goes with it.
    let photo = world.get::<crate::service::Photo>(poster).cloned();
    let rolled_state = if is_under(path, POSTER_OFFICIAL) {
        "rolled_legit"
    } else if is_under(path, POSTER_TRAITOR) {
        "rolled_traitor"
    } else {
        "rolled_poster"
    };
    remove(world, poster);
    let Some(item) = crate::construction::spawn_obj(world, POSTER_ITEM, at) else {
        return;
    };
    let item_name = format!("плакат - {}", crate::obj_damage::strip(&name.name));
    if let Some(mut n) = world.get_mut::<Name>(item) {
        *n = Name::new(item_name.clone());
    }
    world.entity_mut(item).insert((
        DmName {
            name: item_name,
            desc: "Свёрнутый плакат. Его можно повесить на стену.".to_owned(),
        },
        RolledPoster {
            structure: path.to_owned(),
            icon_state,
            name,
        },
    ));
    if let Some(photo) = photo {
        world.entity_mut(item).insert(photo);
    }
    crate::obj_damage::set_icon_state(world, item, rolled_state);
    if let Some(inventory) = world.get::<crate::inventory::Inventory>(mob).cloned() {
        crate::storage::put_in_hands(world, mob, &inventory, item);
    }
}

fn closed_turf(world: &World, e: Entity) -> bool {
    world
        .get::<DmTurfBuild>(e)
        .is_some_and(|t| is_under(&t.path, CLOSED_TURF))
}

/// The pixel offset of a poster hung on the `dir` wall of its tile.
fn poster_offset(dir: Dir) -> [i16; 2] {
    let (dx, dy) = dir.offset();
    [(dx * 32) as i16, (dy * 32) as i16]
}

/// The direction from `mob` to the wall it may hang a poster on: cardinal, the face free (tg's
/// "no room!" check of the wall's posters, here the posters drawn on that face of the user's
/// tile). `Err` is the refusal.
fn hanging_dir(world: &World, mob: Entity, wall: Entity) -> Result<(Dir, TilePos), &'static str> {
    let (Some(&from), Some(&to)) = (world.get::<TilePos>(mob), world.get::<TilePos>(wall)) else {
        return Err("встаньте на одной линии со стеной!");
    };
    let dir = crate::pull::dir_between(from, to)
        .filter(|d| Dir::CARDINALS.contains(d) && from.chebyshev(to) == Some(1))
        .ok_or("встаньте на одной линии со стеной!")?;
    let offset = poster_offset(dir);
    let taken = world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(from)
        .iter()
        .any(|&e| {
            path(world, e).is_some_and(|p| is_under(p, POSTER))
                && world
                    .get::<Appearance>(e)
                    .is_some_and(|a| a.pixel_offset == offset)
        });
    if taken {
        return Err("нет места!");
    }
    Ok((dir, from))
}

/// `poster/interact_with_atom` on a wall: the checks, then the 3.7 s `do_after`.
fn start_hanging(world: &mut World, client: Entity, mob: Entity, held: Entity, wall: Entity) {
    if let Err(refusal) = hanging_dir(world, mob, wall) {
        tell(world, client, Span::Warning, refusal);
        return;
    }
    tell(world, client, Span::Notice, "вешаем плакат...");
    if let Some(&pos) = world.get::<TilePos>(mob) {
        playsound(world, PlaySound::new(pos, POSTER_HANG_SOUND, 100.0, true));
    }
    crate::tools::do_after(
        world,
        Actor {
            client,
            mob,
            item: Some(held),
        },
        wall,
        POSTER_PLACE_DS,
        0,
        Job::Decor(DecorJob::HangPoster),
    );
}

/// The end of the hanging `do_after`: the wall still a wall, the face still free, the rolled
/// poster still in hand; the poster's structure on the user's tile, on that wall.
fn finish_hanging(world: &mut World, mob: Entity, action: &DoAfter) {
    let wall = action.target;
    let Some(item) = action.item else {
        return;
    };
    let Some(rolled) = world.get::<RolledPoster>(item).cloned() else {
        return;
    };
    if !closed_turf(world, wall) {
        return;
    }
    let Ok((dir, pos)) = hanging_dir(world, mob, wall) else {
        return;
    };
    // `poster_type`'s `random_basetype`: one of its subtypes, with its own look and name.
    let random = rolled.structure == POSTER_TRAITOR_RANDOM;
    let structure = if random {
        let i = world
            .get_resource_mut::<crate::combat::CombatRng>()
            .map_or(0, |mut r| {
                r.range(0, TRAITOR_POSTERS.len() as u32 - 1) as usize
            });
        TRAITOR_POSTERS[i].to_owned()
    } else {
        rolled.structure.clone()
    };
    let Some(poster) = crate::construction::spawn_obj(world, &structure, pos) else {
        return;
    };
    if let Some(mut appearance) = world.get_mut::<Appearance>(poster) {
        if !random {
            appearance.icon_state = rolled.icon_state;
        }
        appearance.pixel_offset = poster_offset(dir);
    }
    if !random {
        if let Some(mut n) = world.get_mut::<Name>(poster) {
            *n = Name::new(crate::obj_damage::strip(&rolled.name.name).to_owned());
        }
        world.entity_mut(poster).insert(rolled.name);
    }
    // `traitor/on_placed_poster`.
    if is_under(&structure, POSTER_TRAITOR) {
        world
            .entity_mut(poster)
            .insert(crate::syndie_spraycan::Demoraliser(
                &crate::syndie_spraycan::POSTER,
            ));
    }
    if let Some(photo) = world.get::<crate::service::Photo>(item).cloned() {
        world.entity_mut(poster).insert(photo);
    }
    world.despawn(item);
    tell(world, action.client, Span::Notice, "Вы вешаете плакат!");
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

#[cfg(test)]
mod tests {
    use super::*;

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
