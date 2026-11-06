//! The ore redemption machine (server), after tg `/obj/machinery/mineral/ore_redemption`
//! (`code/modules/mining/machine_redemption.dm`) with tgui's `OreRedemptionMachine`.
//!
//! - Storage: the station's ORM links to the ore silo on its z at round start, like the lathes
//!   (`crate::science::lathe`); without one (or the off-station subtype, `requires_silo`
//!   unset) it keeps a local store.
//! - Smelting (`smelt`): ore whose type has a `refined_type` goes into the store with its
//!   materials times `ore_multiplier` (sand is glass first, `on_orm_collection`); each unit
//!   taken adds `points * point_upgrade` unclaimed points. Ore arrives by hand, from a held
//!   satchel, or lying on (or in an ore box on) the input tile (`input_dir`, north) while the
//!   machine is powered; what does not fit is unloaded onto the output tile.
//! - The window: `Claim` moves the unclaimed points to the user's ID's account
//!   (`mining_points`; refused without a silo when one is required); `Release` ejects up to 50
//!   sheets of a material onto the output tile and `Smelt` makes up to 50 of a smelter alloy
//!   (`SMELTER` designs of the `initial` category), both behind the silo's ID check and the
//!   machine's `req_access` (mineral storeroom).
//! - A screwdriver opens its panel (it takes no ore then), a crowbar takes it apart
//!   (`crate::machine_frame`); alt-click with the panel open turns the input and output a
//!   quarter clockwise. tg's ORM has no part upgrades: `ore_multiplier` and `point_upgrade`
//!   are its type's.
// shortcut: no request console ore notices, I/O overlays, boulders, silo holds or logs.

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::research::{MAX_STACK_SIZE, SHEET_MATERIAL_AMOUNT, build_type};
use ss13_mapload::{DmItem, DmName, DmOre, DmOreRedemption};
use ss13_shared::chat::Span;
use ss13_shared::{Dir, TilePos};
use ss13_tgui::UiInterface as _;
use ss13_tgui::UiStatus;
use ss13_tgui::interfaces::vending::ProductIcon;
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_world::SpatialIndex;

use super::rocks::{ORE_BOX, is_ore, stored_ores};
use super::{
    CLAIM, MATERIAL_REF, OrmData, OrmMaterial, OrmUi, OrmUser, PATH, RELEASE, SHEETS, SMELT,
};
use crate::cargo::console::machine_say;
use crate::cargo::economy::{BankAccountId, Economy};
use crate::inventory::{CarriedBy, IdCard, IdCards, Inventory, check_access};
use crate::obj_damage::is_under;
use crate::power::Unpowered;
use crate::science::lathe::{LatheSetup, MaterialStore, OreSilo, silo_permitted};
use crate::science::points::ResearchIcons;
use crate::science::techweb::ResearchDb;

/// The local store of an ORM without a silo, in sheets.
// shortcut: tg sizes it from the machine's matter bins; a fixed 50 sheets per material slot.
const LOCAL_SHEETS: f64 = 50.0;
/// `RND_CATEGORY_INITIAL`.
const INITIAL: &str = "initial";

/// An ORM's live state.
#[derive(Component, Clone, Debug, Default)]
pub struct OreRedemption {
    /// `points`.
    pub points: f64,
    /// The linked silo; `None`: its own [`MaterialStore`].
    pub silo: Option<Entity>,
    /// The smelter alloys (`stored_research.researched_designs`): design indices.
    pub alloys: Vec<usize>,
}

pub(super) struct OrmPlugin;

impl Plugin for OrmPlugin {
    fn build(&self, app: &mut App) {
        app.register_ui::<OrmUi>()
            .add_systems(ss13_world::MapInit, setup_orms.after(LatheSetup))
            .add_systems(
                Update,
                (
                    input_tiles.run_if(any_with_component::<OreRedemption>),
                    orm_acts.in_set(UiSystems::Handle),
                    unpowered_windows.in_set(UiSystems::StatusHook),
                ),
            );
    }
}

fn setup_orms(world: &mut World) {
    let mut query =
        world.query_filtered::<Entity, (With<DmOreRedemption>, With<ss13_world::Mapped>)>();
    let orms: Vec<Entity> = query.iter(world).collect();
    for orm in orms {
        setup(world, orm);
    }
}

/// An ORM's live state, at map load or built from a frame (`crate::machine_frame`).
pub fn setup(world: &mut World, orm: Entity) {
    let Some((materials, alloys)) = world.get_resource::<ResearchDb>().map(|db| {
        let alloys: Vec<usize> = db
            .designs
            .iter()
            .enumerate()
            .filter(|(_, d)| {
                d.build_type & build_type::SMELTER != 0
                    && d.build_path.is_some()
                    && !d.material_requirements
                    && d.category.iter().any(|c| c == INITIAL)
            })
            .map(|(i, _)| i)
            .collect();
        (db.materials.len(), alloys)
    }) else {
        return;
    };
    let Some(requires_silo) = world.get::<DmOreRedemption>(orm).map(|d| d.requires_silo) else {
        return;
    };
    let z = world.get::<TilePos>(orm).map(|p| p.z);
    let silo = if requires_silo {
        silo_on(world, z)
    } else {
        None
    };
    let mut entity = world.entity_mut(orm);
    entity.insert(OreRedemption {
        points: 0.0,
        silo,
        alloys,
    });
    if silo.is_none() {
        let size = (LOCAL_SHEETS * SHEET_MATERIAL_AMOUNT) as u32 * materials as u32;
        entity.insert(MaterialStore::new(materials, Some(size)));
    }
}

/// The ore silo on level `z`, if any (`GLOB.ore_silo_default` of the station).
pub fn silo_on(world: &mut World, z: Option<u16>) -> Option<Entity> {
    let mut silos = world.query_filtered::<(Entity, &TilePos), With<OreSilo>>();
    silos
        .iter(world)
        .find(|(_, p)| Some(p.z) == z)
        .map(|(e, _)| e)
}

/// tg `turn(dir, -90)`: a quarter clockwise.
pub fn clockwise(dir: Dir) -> Dir {
    match dir {
        Dir::North => Dir::East,
        Dir::East => Dir::South,
        Dir::South => Dir::West,
        Dir::West => Dir::North,
        other => other,
    }
}

/// tg `dir2text`.
pub fn dir_text(dir: Dir) -> &'static str {
    match dir {
        Dir::North => "north",
        Dir::South => "south",
        Dir::East => "east",
        Dir::West => "west",
        Dir::NorthEast => "northeast",
        Dir::NorthWest => "northwest",
        Dir::SouthEast => "southeast",
        Dir::SouthWest => "southwest",
    }
}

/// `/obj/machinery/mineral/ore_redemption/click_alt`: with the panel open the input and output
/// turn a quarter clockwise. True when `target` is an ORM (closed, it blocks the click).
pub fn click_alt(world: &mut World, client: Entity, target: Entity) -> bool {
    if world.get::<OreRedemption>(target).is_none() {
        return false;
    }
    let open = world
        .get::<crate::construction::PanelOpen>(target)
        .is_some_and(|p| p.0);
    if !open {
        return true;
    }
    let input = clockwise(dirs(world, target).0);
    if let Some(mut dm) = world.get_mut::<DmOreRedemption>(target) {
        dm.input_dir = input as u8;
    }
    let name = world
        .get::<DmName>(target)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    crate::tools::tell(
        world,
        client,
        Span::Notice,
        format!(
            "You change {name}'s I/O settings, setting the input to {} and the output to {}.",
            dir_text(input),
            dir_text(input.reverse())
        ),
    );
    true
}

/// The store `orm` uses: its silo's or its own.
pub fn store_of(world: &World, orm: Entity) -> Option<Entity> {
    match world.get::<OreRedemption>(orm)?.silo {
        Some(silo) if world.get::<MaterialStore>(silo).is_some() => Some(silo),
        Some(_) => None,
        None => world.get::<MaterialStore>(orm).map(|_| orm),
    }
}

fn dirs(world: &World, orm: Entity) -> (Dir, Dir) {
    let input = world
        .get::<DmOreRedemption>(orm)
        .and_then(|d| Dir::from_bits(d.input_dir))
        .unwrap_or(Dir::North);
    (input, input.reverse())
}

/// `unload_mineral`: onto the output tile.
fn unload(world: &mut World, orm: Entity, item: Entity) {
    let Some(&pos) = world.get::<TilePos>(orm) else {
        return;
    };
    let out = pos.step(dirs(world, orm).1);
    world.entity_mut(item).remove::<CarriedBy>();
    crate::storage::take_out(world, item, out);
}

/// `on_orm_collection` + `materials.insert_item(ore, ore_multiplier)` + `local_redeem_points`.
/// True when the ore was taken (some of it).
pub fn smelt(world: &mut World, orm: Entity, ore: Entity) -> bool {
    let Some(dm) = world.get::<DmOre>(ore).cloned() else {
        return false;
    };
    if dm.refined_type.is_empty() {
        return false;
    }
    let (Some(store), Some(config)) = (
        store_of(world, orm),
        world.get::<DmOreRedemption>(orm).cloned(),
    ) else {
        return false;
    };
    let path = world
        .get::<DmItem>(ore)
        .map(|i| i.path.clone())
        .unwrap_or_default();
    let per_unit: Vec<(usize, f64)> = {
        let Some(db) = world.get_resource::<ResearchDb>() else {
            return false;
        };
        // Sand's material is not a silo one: it is smelted into its glass first.
        db.stacks
            .get(&path)
            .or_else(|| db.stacks.get(&dm.refined_type))
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|(m, n)| (m, n * f64::from(config.ore_multiplier)))
            .collect()
    };
    if per_unit.is_empty() {
        return false;
    }
    let amount = crate::tools::stack_amount(world, ore);
    let taken = world
        .get_mut::<MaterialStore>(store)
        .map_or(0, |mut s| s.insert_stack(&per_unit, amount));
    if taken == 0 {
        unload(world, orm, ore);
        return false;
    }
    if let Some(mut machine) = world.get_mut::<OreRedemption>(orm) {
        machine.points += f64::from(dm.points * config.point_upgrade) * f64::from(taken);
    }
    crate::tools::use_stack(world, ore, taken);
    if taken < amount && world.get_entity(ore).is_ok() {
        unload(world, orm, ore);
    }
    true
}

fn working(world: &World, orm: Entity) -> bool {
    world.get::<Unpowered>(orm).is_none()
        && store_of(world, orm).is_some()
        && !world
            .get::<crate::construction::PanelOpen>(orm)
            .is_some_and(|p| p.0)
}

/// tg `pickup_item` on the input turf: ore lying there, or in an ore box there.
// Cost: one tile lookup per ORM per tick (one or two ORMs on a station).
fn input_tiles(world: &mut World) {
    let mut query = world.query_filtered::<(Entity, &TilePos), With<OreRedemption>>();
    let orms: Vec<(Entity, TilePos)> = query.iter(world).map(|(e, p)| (e, *p)).collect();
    for (orm, pos) in orms {
        if !working(world, orm) {
            continue;
        }
        let tile = pos.step(dirs(world, orm).0);
        let here: Vec<Entity> = world.resource::<SpatialIndex>().on_tile(tile).to_vec();
        let mut ores = Vec::new();
        for e in here {
            if is_ore(world, e) {
                ores.push(e);
            } else if crate::construction::path_of(world, e).is_some_and(|p| is_under(&p, ORE_BOX))
            {
                ores.extend(stored_ores(world, e));
            }
        }
        for ore in ores {
            smelt(world, orm, ore);
        }
    }
}

/// tg `base_item_interaction`: ore, or a satchel's ore, into a working ORM. True when it took
/// the click.
pub fn item_interaction(world: &mut World, held: Entity, target: Entity) -> bool {
    if world.get::<OreRedemption>(target).is_none() || !working(world, target) {
        return false;
    }
    let ores = if is_ore(world, held) {
        vec![held]
    } else if world.get::<crate::storage::Storage>(held).is_some() {
        stored_ores(world, held)
    } else {
        Vec::new()
    };
    if ores.is_empty() {
        return false;
    }
    for ore in ores {
        smelt(world, target, ore);
    }
    true
}

/// An empty hand opens the window (`ui_interact`). True when `target` is an ORM.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<OreRedemption>(target).is_none() {
        return false;
    }
    let name = world
        .get::<DmName>(target)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<OrmUi>(client, mob, target, name));
    true
}

fn unpowered_windows(
    mut windows: Query<&mut UiWindow>,
    unpowered: Query<(), (With<OreRedemption>, With<Unpowered>)>,
) {
    for mut window in &mut windows {
        if window.interface == OrmUi::NAME && unpowered.contains(window.source) {
            window.cap_status(UiStatus::Update);
        }
    }
}

/// `can_smelt_alloy`: how many of `design` the store's materials make.
pub fn can_smelt(db: &ResearchDb, store: &MaterialStore, design: usize) -> u32 {
    let Some(d) = db.designs.get(design) else {
        return 0;
    };
    let mut amount: Option<u32> = None;
    for (mat, units) in &d.materials {
        let have = db
            .material_index
            .get(mat)
            .and_then(|&i| store.amounts.get(i))
            .copied()
            .unwrap_or(0);
        if *units <= 0.0 || have == 0 {
            return 0;
        }
        let n = (f64::from(have) / units).floor() as u32;
        if n == 0 {
            return 0;
        }
        amount = Some(amount.map_or(n, |a| a.min(n)));
    }
    amount.unwrap_or(0)
}

type OrmParams = (
    Query<'static, 'static, &'static Inventory>,
    IdCards<'static, 'static>,
    Query<'static, 'static, &'static BankAccountId>,
    Option<Res<'static, Economy>>,
    Query<'static, 'static, &'static MaterialStore, With<OreSilo>>,
    Option<Res<'static, ResearchDb>>,
    Option<Res<'static, ResearchIcons>>,
);

impl ServerUi for OrmUi {
    type Source = (
        &'static OreRedemption,
        &'static DmOreRedemption,
        Option<&'static MaterialStore>,
    );
    type UserParam = OrmParams;
    type Act = OrmAct;

    fn static_data(_: (&OreRedemption, &DmOreRedemption, Option<&MaterialStore>)) {}

    fn data(
        (orm, dm, own): (&OreRedemption, &DmOreRedemption, Option<&MaterialStore>),
        user: Entity,
        (inventories, cards, accounts, economy, silos, db, icons): &SystemParamItem<
            '_,
            '_,
            OrmParams,
        >,
    ) -> OrmData {
        let store = match orm.silo {
            Some(silo) => silos.get(silo).ok(),
            None => own,
        };
        let mut materials = Vec::new();
        if let (Some(store), Some(db)) = (store, db.as_deref()) {
            for (i, m) in db.materials.iter().enumerate() {
                let units = store.amounts.get(i).copied().unwrap_or(0);
                materials.push(OrmMaterial {
                    name: crate::science::lathe::plain(&m.name).to_owned(),
                    id: i as u32,
                    amount: f64::from(units) / SHEET_MATERIAL_AMOUNT,
                    alloy: false,
                    value: dm
                        .ore_values
                        .iter()
                        .find(|(p, _)| *p == m.id)
                        .map_or(0.0, |(_, v)| *v),
                    icon: icons
                        .as_deref()
                        .and_then(|icons| icons.materials.get(i).copied().flatten()),
                });
            }
            for &design in &orm.alloys {
                let d = &db.designs[design];
                let icon: Option<ProductIcon> = icons
                    .as_deref()
                    .and_then(|icons| icons.designs.get(design).copied().flatten());
                materials.push(OrmMaterial {
                    name: d.name.clone(),
                    id: design as u32,
                    amount: f64::from(can_smelt(db, store, design)),
                    alloy: true,
                    value: 0.0,
                    icon,
                });
            }
        }
        let disconnected = if store.is_none() {
            Some("Local mineral storage is unavailable".to_owned())
        } else if orm.silo.is_none() && dm.requires_silo {
            Some("No ore silo connection is available; storing locally".to_owned())
        } else {
            None
        };
        let card = inventories
            .get(user)
            .ok()
            .and_then(|i| i.id_search().find_map(|e| cards.id_of(e)));
        let user = card
            .and_then(|c| accounts.get(c).ok())
            .and_then(|a| economy.as_ref()?.account(a.0))
            .map(|a| OrmUser {
                name: a.holder.clone(),
                cash: a.mining_points,
            });
        OrmData {
            unclaimed_points: orm.points.floor() as i64,
            materials,
            disconnected,
            user,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<OrmAct, UiReject> {
        let index = |key: &str| -> Result<usize, UiReject> {
            let n = params.number(key)?;
            if n < 0.0 || n.fract() != 0.0 || n > f64::from(u16::MAX) {
                return Err(UiReject::BadParams(format!("{key}: not an index")));
            }
            Ok(n as usize)
        };
        Ok(match action {
            CLAIM => OrmAct::Claim,
            RELEASE => OrmAct::Release(index(MATERIAL_REF)?, params.number(SHEETS)?),
            SMELT => OrmAct::Smelt(index(PATH)?, params.number(SHEETS)?),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OrmAct {
    Claim,
    /// Material index, sheets.
    Release(usize, f64),
    /// Alloy design index, sheets.
    Smelt(usize, f64),
}

fn orm_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<OrmUi>>>) {
    let pending: Vec<(Entity, Entity, OrmAct)> = acts
        .read(world.resource::<Messages<UiActed<OrmUi>>>())
        .map(|a| (a.actor, a.source, a.act))
        .collect();
    for (user, orm, act) in pending {
        if world.get::<OreRedemption>(orm).is_none() {
            continue;
        }
        match act {
            OrmAct::Claim => {
                claim(world, orm, user);
            }
            OrmAct::Release(material, sheets) => release(world, orm, user, material, sheets),
            OrmAct::Smelt(design, sheets) => smelt_alloy(world, orm, user, design, sheets),
        }
    }
}

/// `ui_act("Claim")`: the unclaimed points onto the user's ID's account. True when paid.
pub fn claim(world: &mut World, orm: Entity, user: Entity) -> bool {
    let Some(machine) = world.get::<OreRedemption>(orm).cloned() else {
        return false;
    };
    let requires = world
        .get::<DmOreRedemption>(orm)
        .is_some_and(|d| d.requires_silo);
    // `requires_silo && !materials.check_z_level()`.
    if requires && machine.silo.is_none() {
        return false;
    }
    let Some(card) = world.get::<Inventory>(user).and_then(|i| i.id_card(world)) else {
        machine_say(world, orm, "No ID card found.");
        return false;
    };
    let whole = machine.points.floor();
    if whole < 1.0 {
        return false;
    }
    // tg reads `registered_account` unchecked; a card without an account gets nothing.
    let Some(account) = crate::cargo::economy::account_of(world, card) else {
        return false;
    };
    let paid = world
        .get_resource_mut::<Economy>()
        .and_then(|mut e| {
            e.account_mut(account).map(|a| {
                a.mining_points += whole as i64;
            })
        })
        .is_some();
    if paid && let Some(mut m) = world.get_mut::<OreRedemption>(orm) {
        m.points -= whole;
    }
    paid
}

/// `allowed(usr)` against the ORM's `req_access` with the user's ID.
fn allowed(world: &World, orm: Entity, user: Entity) -> bool {
    let req = world
        .get::<DmOreRedemption>(orm)
        .map(|d| d.req_access.clone())
        .unwrap_or_default();
    let access = world
        .get::<Inventory>(user)
        .and_then(|i| i.id_card(world))
        .and_then(|c| world.get::<IdCard>(c))
        .map(|c| c.access.clone())
        .unwrap_or_default();
    check_access(&req, &[], &access)
}

/// `materials.can_use_resource`, then `allowed`.
fn permitted(world: &mut World, orm: Entity, user: Entity) -> Option<Entity> {
    let store = store_of(world, orm);
    let linked = world
        .get::<OreRedemption>(orm)
        .is_some_and(|m| m.silo.is_some());
    if !silo_permitted(world, orm, user, store, linked) {
        return None;
    }
    if !allowed(world, orm, user) {
        if let Some(client) = world
            .get::<crate::server::PlayerMob>(user)
            .map(|p| p.client)
        {
            crate::tools::tell(world, client, Span::Warning, "Required access not found.");
        }
        return None;
    }
    store
}

/// `ui_act("Release")`: `round(min(sheets, 50, stored))` sheets onto the output tile.
pub fn release(world: &mut World, orm: Entity, user: Entity, material: usize, sheets: f64) {
    if !sheets.is_finite() || sheets < 1.0 {
        return;
    }
    let Some(store) = permitted(world, orm, user) else {
        return;
    };
    let Some(sheet_type) = world
        .get_resource::<ResearchDb>()
        .and_then(|db| db.materials.get(material))
        .and_then(|m| m.sheet_type.clone())
    else {
        return;
    };
    let have = world
        .get::<MaterialStore>(store)
        .and_then(|s| s.amounts.get(material).copied())
        .unwrap_or(0);
    let whole = have / SHEET_MATERIAL_AMOUNT as u32;
    let count = (sheets.round() as u32).min(MAX_STACK_SIZE).min(whole);
    if count == 0 {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(orm) else {
        return;
    };
    if let Some(mut s) = world.get_mut::<MaterialStore>(store)
        && let Some(n) = s.amounts.get_mut(material)
    {
        *n -= count * SHEET_MATERIAL_AMOUNT as u32;
    }
    let out = pos.step(dirs(world, orm).1);
    crate::construction::spawn_stack(world, &sheet_type, out, count);
}

/// `ui_act("Smelt")`: up to 50 of an alloy from the store's materials, onto the output tile.
pub fn smelt_alloy(world: &mut World, orm: Entity, user: Entity, design: usize, sheets: f64) {
    if !sheets.is_finite() {
        return;
    }
    let known = world
        .get::<OreRedemption>(orm)
        .is_some_and(|m| m.alloys.contains(&design));
    if !known {
        return;
    }
    let Some(store) = permitted(world, orm, user) else {
        return;
    };
    let Some((cost, build_path, stack)) = world.get_resource::<ResearchDb>().and_then(|db| {
        let d = db.designs.get(design)?;
        let cost: Vec<(usize, f64)> = d
            .materials
            .iter()
            .filter_map(|(m, n)| Some((*db.material_index.get(m)?, *n)))
            .collect();
        Some((cost, d.build_path.clone()?, d.stack))
    }) else {
        return;
    };
    let possible = {
        let db = world.resource::<ResearchDb>();
        world
            .get::<MaterialStore>(store)
            .map_or(0, |s| can_smelt(db, s, design))
    };
    let amount = (sheets.round().max(0.0) as u32)
        .min(MAX_STACK_SIZE)
        .min(possible);
    if amount < 1 {
        return;
    }
    if let Some(mut s) = world.get_mut::<MaterialStore>(store) {
        s.use_materials(&cost, 1.0, amount);
    }
    let Some(&pos) = world.get::<TilePos>(orm) else {
        return;
    };
    let out = pos.step(dirs(world, orm).1);
    if stack {
        crate::construction::spawn_stack(world, &build_path, out, amount);
    } else {
        crate::construction::spawn_obj(world, &build_path, out);
    }
}
