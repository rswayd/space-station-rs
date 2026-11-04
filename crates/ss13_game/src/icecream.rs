//! The ice cream vat (server): tg `/obj/machinery/icecream_vat` (`icecream_vat.dm`),
//! `/datum/component/ice_cream_holder` and `/datum/ice_cream_flavour`
//! (`datums/components/food/ice_cream_holder.dm`).
//!
//! - The vat holds 300 units (`NO_REACT`, 273.15 K) and starts with 6 units of each of tg's
//!   `icecream_vat_reagents`.
//! - An empty hand opens tg's radial menu as a list: in ice cream mode it picks the flavour, in
//!   cone mode it makes a cone (1 unit of each of the cone's ingredients) into the hands. A
//!   right click swaps the modes; an alt click takes the custom beaker out
//!   (`crate::loot`'s alt clicks).
//! - An open container goes in as the custom beaker (swapping the old one into the hands); a right
//!   click with one pours its reagents that the vat stocks into the vat. A spoon or a ladle asks
//!   which reagent to spill out.
//! - An ice cream holder (a cone, space freezy, sundae, honkdae) used on the vat gets a scoop of
//!   the selected flavour if the vat has 1 unit of each of its ingredients (up to 3 scoops): its
//!   `reagent_type`, 10 units of the holder's sweetener, 20 more units of room, a colored scoop
//!   overlay and a new name ("double vanilla ice cream"). The custom flavour takes 4 units from
//!   the beaker (bland without 4).
// shortcut: no Ice Cream Day, no `crafted_food_buff`/`TRAIT_HANDMADE`, no restaurant orders,
// descriptions keep their type's text; the names of the flavours and cones are tg's English.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::{DmItem, DmPower, IconIndex};
use ss13_reagents::{Holder, Owner, flags, is_under};
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, ask_world};

use crate::chem::{Chem, Reagents, name_of};
use crate::inventory::CarriedBy;
use crate::storage::put_inside;
use crate::tools::tell;

pub const VAT_ROOT: &str = "/obj/machinery/icecream_vat";
const NO_PRESET: &str = "/obj/machinery/icecream_vat/no_preinstalled_reagents";
/// `T0C`.
const T0C: f32 = 273.15;
const VAT_VOLUME: f32 = 300.0;
/// `CONE_REAGENT_NEEDED`, `SWEETENER_PER_SCOOP`, `EXTRA_MAX_VOLUME_PER_SCOOP`,
/// `DEFAULT_MAX_ICE_CREAM_SCOOPS`, `ICE_CREAM_SCOOP_OFFSET`.
const CONE_REAGENT_NEEDED: f32 = 1.0;
const SWEETENER_PER_SCOOP: f32 = 10.0;
const EXTRA_VOLUME_PER_SCOOP: f32 = 20.0;
const MAX_SCOOPS: usize = 3;
const SCOOP_OFFSET: i16 = 4;
const SCOOP_ICON: &str = "icons/obj/service/kitchen.dmi";
const SCOOP_STATE: &str = "icecream_custom";
const SPOON: &str = "/obj/item/kitchen/spoon";

const MILK: &str = "/datum/reagent/consumable/milk";
const KORTA_MILK: &str = "/datum/reagent/consumable/korta_milk";
const FLOUR: &str = "/datum/reagent/consumable/flour";
const KORTA_FLOUR: &str = "/datum/reagent/consumable/korta_flour";
const SUGAR: &str = "/datum/reagent/consumable/sugar";
const ICE: &str = "/datum/reagent/consumable/ice";
const COCO: &str = "/datum/reagent/consumable/coco";
const VANILLA: &str = "/datum/reagent/consumable/vanilla";
const BERRY: &str = "/datum/reagent/consumable/berryjuice";
const SINGULO: &str = "/datum/reagent/consumable/ethanol/singulo";
const LEMON: &str = "/datum/reagent/consumable/lemonjuice";
const CARAMEL: &str = "/datum/reagent/consumable/caramel";
const BANANA: &str = "/datum/reagent/consumable/banana";
const ORANGE: &str = "/datum/reagent/consumable/orangejuice";
const CREAM: &str = "/datum/reagent/consumable/cream";
const PEACH: &str = "/datum/reagent/consumable/peachjuice";
const CHERRY: &str = "/datum/reagent/consumable/cherryjelly";

/// `icecream_vat_reagents`: 6 units each.
const VAT_REAGENTS: [&str; 17] = [
    MILK,
    KORTA_MILK,
    FLOUR,
    KORTA_FLOUR,
    SUGAR,
    ICE,
    COCO,
    VANILLA,
    BERRY,
    SINGULO,
    LEMON,
    CARAMEL,
    BANANA,
    ORANGE,
    CREAM,
    PEACH,
    CHERRY,
];
const VAT_REAGENT_AMOUNT: f32 = 6.0;

/// One `/datum/ice_cream_flavour` (tg's subtypes, `GLOB.ice_cream_flavours` order).
pub struct Flavour {
    pub name: &'static str,
    pub color: [u8; 4],
    pub ingredients: &'static [&'static str],
    pub reagent: Option<&'static str>,
    pub hidden: bool,
    pub custom: bool,
}

const fn rgb(c: u32) -> [u8; 4] {
    [(c >> 16) as u8, (c >> 8) as u8, c as u8, 0xFF]
}

const BASE: [&str; 3] = [MILK, ICE, VANILLA];

/// `subtypesof(/datum/ice_cream_flavour)` in type order.
pub const FLAVOURS: [Flavour; 15] = [
    Flavour {
        name: "banana",
        color: rgb(0xFFFF00),
        ingredients: &[MILK, ICE, BANANA],
        reagent: Some(BANANA),
        hidden: false,
        custom: false,
    },
    Flavour {
        name: "bland",
        color: rgb(0xF3F3F3),
        ingredients: &BASE,
        reagent: None,
        hidden: true,
        custom: false,
    },
    Flavour {
        name: "blue",
        color: rgb(0xCBD5F4),
        ingredients: &[MILK, ICE, SINGULO],
        reagent: Some(SINGULO),
        hidden: false,
        custom: false,
    },
    Flavour {
        name: "caramel",
        color: rgb(0xD98736),
        ingredients: &[MILK, ICE, CARAMEL],
        reagent: Some(CARAMEL),
        hidden: false,
        custom: false,
    },
    Flavour {
        name: "cherry chocolate chip",
        color: rgb(0x800000),
        ingredients: &[MILK, ICE, COCO, CHERRY],
        reagent: Some(CHERRY),
        hidden: false,
        custom: false,
    },
    Flavour {
        name: "chocolate",
        color: rgb(0x93683C),
        ingredients: &[MILK, ICE, COCO],
        reagent: Some(COCO),
        hidden: false,
        custom: false,
    },
    Flavour {
        name: "custom",
        color: rgb(0x8589FA),
        ingredients: &[MILK, ICE],
        reagent: None,
        hidden: false,
        custom: true,
    },
    Flavour {
        name: "korta custom",
        color: rgb(0x8589FA),
        ingredients: &[KORTA_MILK, ICE],
        reagent: None,
        hidden: false,
        custom: true,
    },
    Flavour {
        name: "lemon sorbet",
        color: rgb(0xFFFF9F),
        ingredients: &[ICE, LEMON],
        reagent: Some(LEMON),
        hidden: false,
        custom: false,
    },
    Flavour {
        name: "mob",
        color: rgb(0xF4CBCB),
        ingredients: &BASE,
        reagent: Some("/datum/reagent/consumable/liquidgibs"),
        hidden: true,
        custom: false,
    },
    Flavour {
        name: "orangesicle",
        color: rgb(0xFFA980),
        ingredients: &[CREAM, ICE, ORANGE],
        reagent: Some(ORANGE),
        hidden: false,
        custom: false,
    },
    Flavour {
        name: "peach",
        color: rgb(0xFFCC66),
        ingredients: &[MILK, ICE, PEACH],
        reagent: Some(PEACH),
        hidden: false,
        custom: false,
    },
    Flavour {
        name: "strawberry",
        color: rgb(0xF4CBCB),
        ingredients: &[MILK, ICE, BERRY],
        reagent: Some(BERRY),
        hidden: false,
        custom: false,
    },
    Flavour {
        name: "vanilla",
        color: rgb(0xF2EEDE),
        ingredients: &BASE,
        reagent: Some(VANILLA),
        hidden: false,
        custom: false,
    },
    Flavour {
        name: "korta vanilla",
        color: rgb(0xF2EEDE),
        ingredients: &[KORTA_MILK, ICE, VANILLA],
        reagent: Some(VANILLA),
        hidden: false,
        custom: false,
    },
];

/// `typesof(/obj/item/food/icecream)` with their `ingredients`.
pub const CONES: [(&str, &[&str]); 3] = [
    ("/obj/item/food/icecream", &[FLOUR, SUGAR]),
    ("/obj/item/food/icecream/chocolate", &[FLOUR, SUGAR, COCO]),
    ("/obj/item/food/icecream/korta", &[KORTA_FLOUR, SUGAR]),
];

/// The `ice_cream_holder` arguments of the foods that have one: (type, `filled_name`,
/// `y_offset`, sweetener).
const HOLDERS: [(&str, Option<&str>, i16, &str); 4] = [
    ("/obj/item/food/icecream", Some("ice cream"), 0, SUGAR),
    ("/obj/item/food/spacefreezy", None, 0, SUGAR),
    ("/obj/item/food/sundae", None, -2, CARAMEL),
    ("/obj/item/food/honkdae", None, -2, SUGAR),
];

/// A vat's live state.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct IceCreamVat {
    pub cones_mode: bool,
    /// Index into [`FLAVOURS`] (`selected_flavour`, vanilla to start).
    pub flavour: usize,
    pub beaker: Option<Entity>,
}

/// The scoops on an ice cream holder (`scoops`), by flavour name.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Scoops(pub Vec<String>);

/// On a prompt: what it asks for.
#[derive(Component, Clone, Debug)]
enum Question {
    /// The radial menu: flavours (by [`FLAVOURS`] index) or cones (by [`CONES`] index).
    Menu { vat: Entity, cones: bool, options: Vec<usize> },
    /// `spill_reagents`: which reagent (by path) to pour out.
    Spill { vat: Entity, reagents: Vec<String> },
}

pub struct IceCreamPlugin;

impl Plugin for IceCreamPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_mapped).add_systems(
            Update,
            answers
                .in_set(ss13_tgui::server::UiSystems::Handle)
                .after(ss13_tgui::prompt::PromptSystems::Answer),
        );
    }
}

fn setup_mapped(world: &mut World) {
    let mut q = world.query::<(Entity, &DmPower)>();
    let found: Vec<(Entity, String)> = q
        .iter(world)
        .filter(|(_, p)| is_under(&p.path, VAT_ROOT))
        .map(|(e, p)| (e, p.path.clone()))
        .collect();
    for (e, path) in found {
        setup(world, e, &path);
    }
}

/// `Initialize`: the reagents (`create_reagents(300, NO_REACT|TRANSPARENT)` at `T0C`).
pub fn setup(world: &mut World, e: Entity, path: &str) {
    let mut holder = Holder::new(
        VAT_VOLUME,
        flags::NO_REACT | flags::TRANSPARENT,
        Owner {
            path: path.to_owned(),
            living: false,
        },
    );
    holder.set_temperature(T0C);
    if !is_under(path, NO_PRESET)
        && let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone())
    {
        for reagent in VAT_REAGENTS {
            if let Some(id) = chem.id(reagent) {
                holder.add_reagent(&chem, id, VAT_REAGENT_AMOUNT, T0C, None, None, true, false);
            }
        }
    }
    world.entity_mut(e).insert((
        IceCreamVat {
            cones_mode: false,
            flavour: FLAVOURS.iter().position(|f| f.name == "vanilla").unwrap_or(0),
            beaker: None,
        },
        Reagents(holder),
    ));
}

fn item_path(world: &World, e: Entity) -> String {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.clone())
        .unwrap_or_default()
}

/// The `ice_cream_holder` of `item`'s type, if it has one.
fn holder_of(world: &World, item: Entity) -> Option<(Option<&'static str>, i16, &'static str)> {
    let path = item_path(world, item);
    HOLDERS
        .iter()
        .find(|(root, ..)| is_under(&path, root))
        .map(|&(_, name, y, sweet)| (name, y, sweet))
}

fn has_all(holder: &Holder, chem: &ss13_reagents::Chemistry, needed: &[&str]) -> bool {
    needed.iter().all(|path| {
        chem.id(path)
            .is_some_and(|id| holder.amount(id) >= CONE_REAGENT_NEEDED)
    })
}

fn remove_each(holder: &mut Holder, chem: &ss13_reagents::Chemistry, used: &[&str]) {
    for path in used {
        if let Some(id) = chem.id(path) {
            holder.remove_reagent(id, CONE_REAGENT_NEEDED);
        }
    }
}

/// tg `item_interaction` (left) and `item_interaction_secondary` (right) of the vat. True when
/// the click was the vat's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    e: Entity,
    right: bool,
) -> bool {
    let Some(vat) = world.get::<IceCreamVat>(e).cloned() else {
        return false;
    };
    let path = item_path(world, held);
    if holder_of(world, held).is_some() {
        add_scoop(world, client, held, e);
        return true;
    }
    if !right && is_under(&path, SPOON) {
        ask_spill(world, client, mob, e);
        return true;
    }
    let open = world
        .get::<Reagents>(held)
        .is_some_and(|r| r.0.is_open_container());
    if !open {
        return false;
    }
    if right {
        pour_stock(world, client, held, e);
        return true;
    }
    world.entity_mut(held).remove::<CarriedBy>();
    if let Some(mut inventory) = world.get_mut::<crate::inventory::Inventory>(mob)
        && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(held))
    {
        *slot = None;
    }
    put_inside(world, held, e);
    if let Some(old) = vat.beaker {
        crate::cooking::give_to(world, mob, e, old);
        tell(world, client, Span::Notice, "мензурки заменены");
    } else {
        tell(world, client, Span::Notice, "мензурка вставлена");
    }
    if let Some(mut v) = world.get_mut::<IceCreamVat>(e) {
        v.beaker = Some(held);
    }
    true
}

/// `item_interaction_secondary`: the beaker's reagents the vat stocks go into it.
fn pour_stock(world: &mut World, client: Entity, beaker: Entity, e: Entity) {
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let (Some(mut from), Some(mut into)) = (
        world.get::<Reagents>(beaker).map(|r| r.0.clone()),
        world.get::<Reagents>(e).map(|r| r.0.clone()),
    ) else {
        return;
    };
    let stocked: Vec<(ss13_reagents::ReagentId, f32)> = from
        .reagents
        .iter()
        .filter(|x| VAT_REAGENTS.contains(&chem.def(x.id).path.as_str()))
        .map(|x| (x.id, x.volume))
        .collect();
    if stocked.is_empty() {
        tell(world, client, Span::Warning, "нет реагентов для передачи!");
        return;
    }
    for (id, volume) in stocked {
        from.trans_to(&chem, &mut into, volume, 1.0, Some(id), false, false);
    }
    world.entity_mut(beaker).insert(Reagents(from));
    world.entity_mut(e).insert(Reagents(into));
    tell(world, client, Span::Notice, "перезарядка реагентов");
}

/// `click_alt`: the custom beaker into the hands. True when the click was the vat's.
pub fn click_alt(world: &mut World, client: Entity, mob: Entity, e: Entity) -> bool {
    let Some(vat) = world.get::<IceCreamVat>(e).cloned() else {
        return false;
    };
    if let Some(beaker) = vat.beaker {
        if let Some(mut v) = world.get_mut::<IceCreamVat>(e) {
            v.beaker = None;
        }
        crate::cooking::give_to(world, mob, e, beaker);
        tell(world, client, Span::Notice, "мензурка убрана");
    }
    true
}

/// An empty-hand click: the menu (left), swapping modes (right). True when the click was the
/// vat's.
pub fn interact(world: &mut World, client: Entity, mob: Entity, e: Entity, right: bool) -> bool {
    let Some(vat) = world.get::<IceCreamVat>(e).cloned() else {
        return false;
    };
    if right {
        if let Some(mut v) = world.get_mut::<IceCreamVat>(e) {
            v.cones_mode = !v.cones_mode;
        }
        let mode = if vat.cones_mode {
            "мороженное"
        } else {
            "рожок"
        };
        tell(world, client, Span::Notice, format!("выдается {mode}"));
        return true;
    }
    let (options, items): (Vec<usize>, Vec<String>) = if vat.cones_mode {
        CONES
            .iter()
            .enumerate()
            .map(|(i, (path, _))| (i, cone_name(world, path)))
            .unzip()
    } else {
        FLAVOURS
            .iter()
            .enumerate()
            .filter(|(_, f)| !f.hidden)
            .map(|(i, f)| (i, f.name.to_owned()))
            .unzip()
    };
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            name_of(world, e),
            "",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(Question::Menu {
        vat: e,
        cones: vat.cones_mode,
        options,
    });
    true
}

fn cone_name(world: &World, path: &str) -> String {
    world
        .get_resource::<ss13_proto::Prototypes>()
        .and_then(|p| p.get(path))
        .and_then(|p| p.get::<ss13_mapload::DmName>().map(|n| n.name.clone()))
        .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path).to_owned())
}

/// `spill_reagents`: the vat's reagents as a list.
fn ask_spill(world: &mut World, client: Entity, mob: Entity, e: Entity) {
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let Some(holder) = world.get::<Reagents>(e).map(|r| r.0.clone()) else {
        return;
    };
    let (reagents, items): (Vec<String>, Vec<String>) = holder
        .reagents
        .iter()
        .map(|x| {
            let def = chem.def(x.id);
            (def.path.clone(), def.name.clone())
        })
        .unzip();
    if reagents.is_empty() {
        return;
    }
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            "Remove reagent",
            "Select a reagent to purge from the vat.",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world
        .entity_mut(prompt)
        .insert(Question::Spill { vat: e, reagents });
}

/// The answers, revalidated: the actor conscious and next to the vat (`require_near`,
/// `can_perform_action`).
fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answers {
        let Some(question) = world.get::<Question>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let vat = match &question {
            Question::Menu { vat, .. } | Question::Spill { vat, .. } => *vat,
        };
        let able = world.get::<crate::health::Stat>(actor)
            == Some(&crate::health::Stat::Conscious)
            && world.get_entity(vat).is_ok()
            && world.get::<IceCreamVat>(vat).is_some()
            && crate::storage::in_reach(world, actor, vat);
        if !able {
            continue;
        }
        match question {
            Question::Menu {
                cones: false,
                options,
                ..
            } => {
                if let Some(&f) = options.get(i) {
                    if let Some(mut v) = world.get_mut::<IceCreamVat>(vat) {
                        v.flavour = f;
                    }
                    let name = FLAVOURS[f].name;
                    tell(world, client, Span::Notice, format!("делается {name}"));
                }
            }
            Question::Menu {
                cones: true,
                options,
                ..
            } => {
                if let Some(&c) = options.get(i) {
                    make_cone(world, client, actor, vat, c);
                }
            }
            Question::Spill { reagents, .. } => {
                let Some(path) = reagents.get(i) else {
                    continue;
                };
                let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
                    continue;
                };
                if let Some(id) = chem.id(path)
                    && let Some(mut r) = world.get_mut::<Reagents>(vat)
                {
                    let volume = r.0.amount(id);
                    r.0.remove_reagent(id, volume);
                    let name = chem.def(id).name.clone();
                    tell(world, client, Span::Notice, format!("{name} разливается"));
                }
            }
        }
    }
}

/// `make_cone`: 1 unit of each ingredient, the cone into the hands.
fn make_cone(world: &mut World, client: Entity, mob: Entity, e: Entity, cone: usize) {
    let (path, ingredients) = CONES[cone];
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let Some(mut holder) = world.get::<Reagents>(e).map(|r| r.0.clone()) else {
        return;
    };
    if !has_all(&holder, &chem, ingredients) {
        tell(world, client, Span::Warning, "не хватает ингредиентов!");
        return;
    }
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let Some(made) = crate::construction::spawn_obj(world, path, pos) else {
        return;
    };
    remove_each(&mut holder, &chem, ingredients);
    world.entity_mut(e).insert(Reagents(holder));
    let name = name_of(world, made);
    tell(world, client, Span::Notice, format!("делается {name}"));
    crate::cooking::give_to(world, mob, e, made);
}

/// `add_flavor_to_cone` -> `add_flavour`.
fn add_scoop(world: &mut World, client: Entity, cone: Entity, e: Entity) {
    let Some((filled_name, y_offset, sweetener)) = holder_of(world, cone) else {
        return;
    };
    let scoops = world.get::<Scoops>(cone).cloned().unwrap_or_default();
    if scoops.0.len() >= MAX_SCOOPS {
        tell(world, client, Span::Warning, "too many scoops!");
        return;
    }
    let Some(vat) = world.get::<IceCreamVat>(e).cloned() else {
        return;
    };
    let mut flavour = &FLAVOURS[vat.flavour];
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let Some(mut stock) = world.get::<Reagents>(e).map(|r| r.0.clone()) else {
        return;
    };
    if !has_all(&stock, &chem, flavour.ingredients) {
        tell(world, client, Span::Warning, "не хватает ингредиентов!");
        return;
    }
    let mut beaker = vat
        .beaker
        .and_then(|b| world.get::<Reagents>(b).map(|r| (b, r.0.clone())))
        .filter(|(_, r)| r.total_volume > 0.0);
    let ingredients = flavour.ingredients;
    // `custom/add_flavour`: under 4 units it is the bland flavour.
    let mut custom_color = None;
    if flavour.custom {
        match beaker.as_ref() {
            Some((_, r)) if r.total_volume >= 4.0 => {
                custom_color = r
                    .reagents
                    .iter()
                    .max_by(|a, b| a.volume.total_cmp(&b.volume))
                    .and_then(|m| ss13_reagents::parse_color(&chem.def(m.id).color));
            }
            _ => {
                flavour = FLAVOURS.iter().find(|f| f.name == "bland").unwrap_or(flavour);
                beaker = None;
            }
        }
    } else {
        beaker = None;
    }
    let Some(mut food) = world.get::<Reagents>(cone).map(|r| r.0.clone()) else {
        return;
    };
    food.maximum_volume += EXTRA_VOLUME_PER_SCOOP;
    if let Some(reagent) = flavour.reagent.and_then(|p| chem.id(p)) {
        food.add_reagent(&chem, reagent, 3.0, T0C, None, None, false, false);
    }
    if let Some(sweet) = chem.id(sweetener) {
        food.add_reagent(&chem, sweet, SWEETENER_PER_SCOOP, T0C, None, None, false, false);
    }
    if let Some((b, mut from)) = beaker {
        from.trans_to(&chem, &mut food, 4.0, 1.0, None, false, false);
        world.entity_mut(b).insert(Reagents(from));
    }
    world.entity_mut(cone).insert(Reagents(food));
    remove_each(&mut stock, &chem, ingredients);
    world.entity_mut(e).insert(Reagents(stock));
    let mut scoops = scoops;
    scoops.0.push(flavour.name.to_owned());
    let color = custom_color.unwrap_or(flavour.color);
    // The scoop overlays stack `ICE_CREAM_SCOOP_OFFSET` pixels apart.
    let scoop_overlay = world
        .get_resource::<IconIndex>()
        .and_then(|icons| icons.get(SCOOP_ICON, SCOOP_STATE))
        .map(|id| {
            let mut o = Overlay::new(id);
            o.color = Some(color);
            o.pixel_offset = [0, y_offset + SCOOP_OFFSET * (scoops.0.len() as i16 - 1)];
            o
        });
    if let (Some(o), Some(mut look)) = (scoop_overlay, world.get_mut::<Appearance>(cone)) {
        look.overlays.push(o);
    }
    let base = world
        .get::<ss13_mapload::DmName>(cone)
        .map(|n| n.name.clone())
        .unwrap_or_default();
    let name = scoop_name(&scoops.0, filled_name.unwrap_or(&base));
    world.entity_mut(cone).insert((scoops, Name::new(name)));
    let flavour_name = flavour.name;
    tell(
        world,
        client,
        Span::Notice,
        format!("набирает шарик с {flavour_name}"),
    );
}

/// `on_update_name`: "double vanilla ice cream", or the flavours listed.
pub fn scoop_name(scoops: &[String], name: &str) -> String {
    let mut unique: Vec<&str> = Vec::new();
    for s in scoops {
        if !unique.contains(&s.as_str()) {
            unique.push(s);
        }
    }
    if scoops.len() > 1 && unique.len() == 1 {
        let tuple = match scoops.len() {
            2 => "double",
            3 => "triple",
            4 => "quadruple",
            _ => "multiple",
        };
        return format!("{tuple} {} {name}", unique[0]);
    }
    let list = match unique.len() {
        0 => String::new(),
        1 => unique[0].to_owned(),
        2 => format!("{} and {}", unique[0], unique[1]),
        n => format!("{}, and {}", unique[..n - 1].join(", "), unique[n - 1]),
    };
    format!("{list} {name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_the_scoops() {
        let s = |v: &[&str]| v.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
        assert_eq!(scoop_name(&s(&["vanilla"]), "ice cream"), "vanilla ice cream");
        assert_eq!(
            scoop_name(&s(&["vanilla", "vanilla"]), "ice cream"),
            "double vanilla ice cream"
        );
        assert_eq!(
            scoop_name(&s(&["vanilla", "peach", "blue"]), "sundae"),
            "vanilla, peach, and blue sundae"
        );
    }
}
