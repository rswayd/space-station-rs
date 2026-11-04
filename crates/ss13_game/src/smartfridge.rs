//! Smart fridges (server): tg `/obj/machinery/smartfridge` (`food_and_drinks/machinery/
//! smartfridge.dm`) and its `SmartVend` window.
//!
//! - A fridge starts with its `initial_contents`. Out of combat mode, powered, an item it accepts
//!   (`accept_check` of its kind: plants, drinks, food, slime extracts, petri dishes, organs,
//!   medicine, disks) goes in, a bag loads every accepted item it holds, until
//!   `max_n_of_items * matter bin tier` (organ fridges: `20 * tier`).
//! - An empty-hand click (powered) opens the window: the contents grouped by type and name; the
//!   `Release` act hands out up to `amount` items of a group (the hands, else the floor), each
//!   using `active_power_usage`; everything is checked on the server at the moment of effect.
//! - The sprite shows how full it is (`[base]-[overlay]-1..3`), the glass and the powered screen.
// shortcut: no drying (racks and dehydrators accept nothing), organ preservation, welding to the
// floor, prying, breaking, emissive light or vend sound; the window has no search.

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::{DmAtom, DmFood, DmItem, DmName, DmPower, DmSmartFridge, DmWeight, IconIndex};
use ss13_reagents::is_under;
use ss13_shared::chat::Span;
use ss13_shared::{Appearance, Overlay, TilePos};
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::smart_vend::{FridgeItem, SmartVendData, SmartVendUi};
use ss13_tgui::interfaces::vending::ProductIcon;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use crate::chem::{Reagents, name_of, visible};
use crate::combat::CombatMode;
use crate::inventory::{CarriedBy, Inventory};
use crate::power::Unpowered;
use crate::storage::{Stored, put_inside, take_out};
use crate::tools::tell;

pub const FRIDGE_ROOT: &str = ss13_mapload::smartfridge::FRIDGE_ROOT;
const BAG_ROOT: &str = "/obj/item/storage/bag";
const PILL_BOTTLE: &str = "/obj/item/storage/pill_bottle";
const CONTAINER: &str = "/obj/item/reagent_containers";
const BOWL: &str = "/obj/item/reagent_containers/cup/bowl";
/// `WEIGHT_CLASS_BULKY`.
const BULKY: u8 = 5;

/// What a fridge takes (`accept_check` of its type).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FridgeKind {
    Plants,
    Drying,
    Drinks,
    Food,
    Extract,
    Petri,
    Organ,
    Chemistry,
    Disks,
}

pub fn kind(path: &str) -> FridgeKind {
    const KINDS: [(&str, FridgeKind); 8] = [
        ("/obj/machinery/smartfridge/drying", FridgeKind::Drying),
        ("/obj/machinery/smartfridge/drinks", FridgeKind::Drinks),
        ("/obj/machinery/smartfridge/food", FridgeKind::Food),
        ("/obj/machinery/smartfridge/extract", FridgeKind::Extract),
        ("/obj/machinery/smartfridge/petri", FridgeKind::Petri),
        ("/obj/machinery/smartfridge/organ", FridgeKind::Organ),
        (
            "/obj/machinery/smartfridge/chemistry",
            FridgeKind::Chemistry,
        ),
        ("/obj/machinery/smartfridge/disks", FridgeKind::Disks),
    ];
    KINDS
        .iter()
        .find(|(root, _)| is_under(path, root))
        .map_or(FridgeKind::Plants, |&(_, k)| k)
}

/// A fridge's live state.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmartFridge {
    pub kind: FridgeKind,
}

pub struct SmartFridgePlugin;

impl Plugin for SmartFridgePlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<FridgeWindow>()
            .add_systems(
                Startup,
                setup_mapped
                    .after(crate::power::init_power)
                    .after(crate::storage::StorageStartup),
            )
            .add_systems(Update, fridge_acts.in_set(UiSystems::Handle));
    }
}

fn path_of(world: &World, e: Entity) -> String {
    world
        .get::<DmItem>(e)
        .map(|i| i.path.clone())
        .or_else(|| world.get::<DmPower>(e).map(|p| p.path.clone()))
        .unwrap_or_default()
}

fn setup_mapped(world: &mut World) {
    let mut query = world.query_filtered::<Entity, With<DmSmartFridge>>();
    let found: Vec<Entity> = query.iter(world).collect();
    for e in found {
        setup(world, e);
    }
}

/// `Initialize`: the state and the `initial_contents`. Also for a fridge built from a frame.
pub fn setup(world: &mut World, e: Entity) {
    let Some(dm) = world.get::<DmSmartFridge>(e).cloned() else {
        return;
    };
    let kind = kind(&path_of(world, e));
    world.entity_mut(e).insert(SmartFridge { kind });
    if let Some(&pos) = world.get::<TilePos>(e) {
        for (path, amount) in &dm.initial_contents {
            for _ in 0..*amount {
                if let Some(item) = crate::construction::spawn_obj(world, path, pos) {
                    put_inside(world, item, e);
                }
            }
        }
        crate::construction::tile_changed(world, pos);
    }
    refresh_look(world, e);
}

/// `RefreshParts`: `max_n_of_items * matter bin tier` (organ fridges: `20 * tier`).
pub fn max_items(world: &World, e: Entity) -> usize {
    let tier = crate::stock_parts::ratings(world, e)
        .last(crate::stock_parts::PartKind::MatterBin)
        .unwrap_or(1) as usize;
    let organ = world
        .get::<SmartFridge>(e)
        .is_some_and(|f| f.kind == FridgeKind::Organ);
    if organ {
        return 20 * tier;
    }
    world
        .get::<DmSmartFridge>(e)
        .map_or(1500, |d| d.max_items as usize)
        * tier
}

fn contents(world: &World, e: Entity) -> Vec<Entity> {
    world
        .get::<Stored>(e)
        .map(|s| s.items().to_vec())
        .unwrap_or_default()
}

fn has_reagents(world: &World, item: Entity) -> bool {
    world
        .get::<Reagents>(item)
        .is_some_and(|r| !r.0.reagents.is_empty())
}

/// tg `accept_check` of each kind.
pub fn accepts(world: &World, kind: FridgeKind, item: Entity) -> bool {
    let path = path_of(world, item);
    let under = |root: &str| is_under(&path, root);
    match kind {
        FridgeKind::Plants => [
            "/obj/item/food/grown",
            "/obj/item/seeds",
            "/obj/item/grown",
            "/obj/item/graft",
        ]
        .iter()
        .any(|r| under(r)),
        // shortcut: no `dryable` items yet.
        FridgeKind::Drying => false,
        FridgeKind::Drinks => {
            under(CONTAINER)
                && !under(BOWL)
                && has_reagents(world, item)
                && (under("/obj/item/reagent_containers/cup")
                    || under("/obj/item/reagent_containers/condiment"))
        }
        FridgeKind::Food => {
            let w_class = world.get::<DmWeight>(item).map_or(0, |w| w.w_class);
            if w_class >= BULKY {
                return false;
            }
            world.get::<DmFood>(item).is_some()
                || (under(BOWL)
                    && world
                        .get::<Reagents>(item)
                        .is_some_and(|r| r.0.total_volume > 0.0))
        }
        FridgeKind::Extract => under("/obj/item/slime_extract") || under("/obj/item/slime_scanner"),
        FridgeKind::Petri => under("/obj/item/petri_dish"),
        FridgeKind::Organ => under("/obj/item/organ") || under("/obj/item/bodypart"),
        FridgeKind::Chemistry => chemistry_accepts(world, item, &path),
        FridgeKind::Disks => under("/obj/item/disk"),
    }
}

fn chemistry_accepts(world: &World, item: Entity, path: &str) -> bool {
    // A pill bottle full of what it accepts.
    if is_under(path, PILL_BOTTLE) {
        let inside = contents(world, item);
        return !inside.is_empty()
            && inside
                .iter()
                .all(|&i| chemistry_accepts(world, i, &path_of(world, i)));
    }
    if !is_under(path, CONTAINER) {
        return false;
    }
    if is_under(path, "/obj/item/reagent_containers/applicator") {
        return true;
    }
    has_reagents(world, item)
        && [
            "/obj/item/reagent_containers/syringe",
            "/obj/item/reagent_containers/cup/tube",
            "/obj/item/reagent_containers/cup/bottle",
            "/obj/item/reagent_containers/cup/beaker",
            "/obj/item/reagent_containers/spray",
            "/obj/item/reagent_containers/medigel",
            "/obj/item/reagent_containers/chem_pack",
        ]
        .iter()
        .any(|r| is_under(path, r))
}

/// tg `item_interaction`. True when the click was the fridge's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(fridge) = world.get::<SmartFridge>(target).copied() else {
        return false;
    };
    if world.get::<CombatMode>(mob).is_some_and(|c| c.0) {
        return false;
    }
    let name = name_of(world, target);
    if world.get::<Unpowered>(target).is_some() {
        tell(
            world,
            client,
            Span::Warning,
            format!("Магнитная дверца {name} не открывается без питания!"),
        );
        return true;
    }
    let max = max_items(world, target);
    let mut count = contents(world, target).len();
    if count >= max {
        tell(world, client, Span::Warning, "нет места!");
        return true;
    }
    if accepts(world, fridge.kind, held) {
        // tg's merge loads it twice with two messages; once is meant.
        load(world, mob, held, target);
        let (user, what) = (name_of(world, mob), name_of(world, held));
        visible(
            world,
            mob,
            &format!("Вы переместили {what} в {name}."),
            &format!("{user} перемещает {what} в {name}."),
            Span::Notice,
        );
        refresh_look(world, target);
        return true;
    }
    if is_under(&path_of(world, held), BAG_ROOT) {
        let mut loaded = 0;
        for item in contents(world, held) {
            if count >= max {
                break;
            }
            if !accepts(world, fridge.kind, item) {
                continue;
            }
            put_inside(world, item, target);
            loaded += 1;
            count += 1;
        }
        let bag = name_of(world, held);
        if loaded == 0 {
            tell(
                world,
                client,
                Span::Warning,
                format!("В {bag} нет ничего, что можно положить в {name}!"),
            );
            return true;
        }
        let user = name_of(world, mob);
        visible(
            world,
            mob,
            &format!("Вы перемещаете содержимое из {bag} в {name}."),
            &format!("{user} перекладывает предметы из {bag} в {name}."),
            Span::Notice,
        );
        if !contents(world, held).is_empty() {
            tell(
                world,
                client,
                Span::Warning,
                "Некоторые предметы не влазят.",
            );
        }
        refresh_look(world, target);
        return true;
    }
    tell(
        world,
        client,
        Span::Warning,
        format!(
            "{name} умно отказывается принимать {}.",
            name_of(world, held)
        ),
    );
    true
}

/// `load`: out of the user's hands into the fridge.
fn load(world: &mut World, mob: Entity, item: Entity, fridge: Entity) {
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob)
        && let Some(slot) = inventory.hands.iter_mut().find(|h| **h == Some(item))
    {
        *slot = None;
    }
    world.entity_mut(item).remove::<CarriedBy>();
    put_inside(world, item, fridge);
}

/// An empty hand opens the window (powered).
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<SmartFridge>(target).is_none() {
        return false;
    }
    if world.get::<Unpowered>(target).is_none() {
        let title = world
            .get::<DmName>(target)
            .map(|n| crate::vending::plain_name(&n.name).to_owned())
            .unwrap_or_default();
        world.write_message(OpenUi::new::<FridgeWindow>(client, mob, target, title));
    }
    true
}

/// `"[type]-[name]"`, the key `ui_data` groups by and `Release` names.
fn key(world: &World, item: Entity) -> String {
    let name = world
        .get::<DmName>(item)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    format!("{}-{name}", path_of(world, item))
}

/// `update_icon_state` and `update_overlays`.
pub fn refresh_look(world: &mut World, e: Entity) {
    let (Some(atom), Some(dm)) = (
        world.get::<DmAtom>(e).cloned(),
        world.get::<DmSmartFridge>(e).cloned(),
    ) else {
        return;
    };
    let Some(icons) = world.get_resource::<IconIndex>() else {
        return;
    };
    // `base_icon_state`: the mapped `icon_state` is a preview (`smartfridge-icon`).
    let base = atom
        .icon_state
        .strip_suffix("-icon")
        .unwrap_or(&atom.icon_state)
        .to_owned();
    let state = icons.get(&atom.icon, &base);
    let mut names = Vec::new();
    if world
        .get::<crate::construction::PanelOpen>(e)
        .is_some_and(|p| p.0)
    {
        names.push(format!("{base}-panel"));
    }
    let shown = contents(world, e).len();
    if dm.visible_contents && shown > 0 {
        let level = match shown {
            1..=25 => 1,
            26..=50 => 2,
            _ => 3,
        };
        names.push(format!("{base}-{}-{level}", dm.overlay));
    }
    names.push(format!("{base}-glass"));
    if dm.has_emissive && world.get::<Unpowered>(e).is_none() {
        names.push(format!("{base}-powered"));
    }
    let overlays: Vec<Overlay> = names
        .iter()
        .filter_map(|n| icons.get(&atom.icon, n).map(Overlay::new))
        .collect();
    let Some(mut look) = world.get_mut::<Appearance>(e) else {
        return;
    };
    if let Some(id) = state
        && look.icon_state != id
    {
        look.icon_state = id;
    }
    if look.overlays != overlays {
        look.overlays = overlays;
    }
}

pub struct FridgeWindow;

impl UiInterface for FridgeWindow {
    const NAME: &'static str = SmartVendUi::NAME;
    type Data = SmartVendData;
    type StaticData = ();
}

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub key: String,
    pub amount: u32,
}

impl ServerUi for FridgeWindow {
    type Source = (
        Entity,
        &'static SmartFridge,
        Option<&'static Stored>,
        &'static DmName,
    );
    type UserParam = (
        Query<'static, 'static, (&'static DmItem, Option<&'static DmName>)>,
        Query<'static, 'static, &'static Appearance>,
    );
    type Act = Release;

    fn static_data(_: (Entity, &SmartFridge, Option<&Stored>, &DmName)) {}

    fn data(
        (_, _, stored, name): (Entity, &SmartFridge, Option<&Stored>, &DmName),
        _: Entity,
        (items, looks): &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> SmartVendData {
        let mut groups: Vec<FridgeItem> = Vec::new();
        for &item in stored.map(|s| s.items()).unwrap_or_default() {
            let Ok((dm, item_name)) = items.get(item) else {
                continue;
            };
            let plain = item_name
                .map(|n| crate::vending::plain_name(&n.name).to_owned())
                .unwrap_or_default();
            let key = format!("{}-{plain}", dm.path);
            match groups.iter_mut().find(|g| g.path == key) {
                Some(g) => g.amount += 1,
                None => groups.push(FridgeItem {
                    path: key,
                    name: ss13_tgui::interfaces::vending::capitalize_all(&plain),
                    amount: 1,
                    icon: looks.get(item).ok().map(|a| ProductIcon {
                        icon_state: a.icon_state,
                        color: a.color,
                    }),
                }),
            }
        }
        groups.sort_by(|a, b| a.path.cmp(&b.path));
        SmartVendData {
            contents: groups,
            name: crate::vending::plain_name(&name.name).to_owned(),
            isdryer: false,
            drying: false,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<Release, UiReject> {
        match action {
            "Release" => Ok(Release {
                key: params.text("path")?,
                amount: params.number("amount")?.clamp(0.0, 10_000.0) as u32,
            }),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

fn fridge_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<FridgeWindow>>>) {
    let pending: Vec<(Entity, Entity, Release)> = acts
        .read(world.resource::<Messages<UiActed<FridgeWindow>>>())
        .map(|a| (a.actor, a.source, a.act.clone()))
        .collect();
    for (actor, source, act) in pending {
        release(world, actor, source, &act);
    }
}

/// tg `ui_act("Release")`: up to `amount` items of the group, each into the hands or onto the
/// floor, each using `active_power_usage`.
pub fn release(world: &mut World, actor: Entity, fridge: Entity, act: &Release) -> u32 {
    if world.get::<SmartFridge>(fridge).is_none() {
        return 0;
    }
    let Some(&pos) = world.get::<TilePos>(fridge) else {
        return 0;
    };
    let mut left = act.amount;
    let mut given = 0;
    for item in contents(world, fridge) {
        if left == 0 {
            break;
        }
        if key(world, item) != act.key {
            continue;
        }
        take_out(world, item, pos);
        let hand = world.get::<Inventory>(actor).and_then(|i| {
            [i.active, 1 - i.active]
                .into_iter()
                .find(|&h| i.hands[h].is_none())
        });
        if let Some(hand) = hand {
            crate::inventory::pick_up(world, actor, item, hand);
        }
        let joules = world
            .get::<DmPower>(fridge)
            .map_or(0.0, |p| p.active_power_usage);
        crate::kitchen::machine_use_energy(world, fridge, joules);
        left -= 1;
        given += 1;
    }
    if given > 0 {
        crate::construction::tile_changed(world, pos);
        refresh_look(world, fridge);
    }
    given
}

/// `examine`: the capacity.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    if world.get::<SmartFridge>(target).is_none() {
        return;
    }
    lines.push((
        Span::Notice,
        format!(
            "На дисплее состояния отображается: Это устройство может вмещать максимум {} предметов.",
            max_items(world, target)
        ),
    ));
}
