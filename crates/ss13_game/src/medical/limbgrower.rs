//! The limb grower (server): tg `/obj/machinery/limbgrower` (`code/game/machinery/limbgrower.dm`,
//! designs in `code/modules/research/designs/limbgrower_designs.dm` and BandaStation's
//! `modular_bandastation/species/code/surgery/organs/limbgrower_designs.dm`) and its
//! `Limbgrower` window (`ss13_tgui::interfaces::limbgrower`).
//!
//! - Its open 100 u holder (the volume of the beakers among its parts) is filled by pouring
//!   (`crate::chem`); the window lists its reagents (each can be emptied) and the designs by
//!   species tab with their reagents times `production_coefficient` (1.25 - 0.25 per servo
//!   tier, 0.25 to 1).
//! - The designs (`build_type` `LIMBGROWER`) are the `initial` ones, those imported from limb
//!   design disks (a 2 s load) and, emagged, the `hacked` ones; the `fullupgrade` grower has
//!   every one that is not hacked.
//! - `make_limb` of an available design and tab, with the reagents in the holder, takes
//!   `max(active_power_usage, units)` and after `3 s × coefficient` uses the reagents and makes
//!   the item on its tile: a bodypart of the tab's species (`<build_path>/<tab>`, the human one
//!   for humans), named synthetic, or the organ. A busy grower refuses everything.
// shortcut: no sounds, flick animations or synthetic green color of the limbs; no plumbing demand
// or rotation; a species bodypart type without a prototype is made as the human one; a load or
// print runs on the grower's own timer (the user's move or a lost disk cancels a load).

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::prelude::*;
use ss13_mapload::research::build_type;
use ss13_mapload::{DmItem, DmObj};
use ss13_reagents::{Holder, Owner, flags};
use ss13_shared::TilePos;
use ss13_shared::chat::Span;
use ss13_shared::ui::UiStatus;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::limbgrower::{
    LimbgrowerCategory, LimbgrowerData, LimbgrowerDesign, LimbgrowerReagent, LimbgrowerStatic,
    LimbgrowerUi,
};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiKind, UiParams, UiReject, UiSystems, UiWindow,
};

use super::pods::{operational, panel_open};
use crate::chem::{Chem, Reagents};
use crate::inventory::Inventory;
use crate::obj_damage::is_under;
use crate::science::techweb::ResearchDb;
use crate::stock_parts::PartKind;
use crate::tools::tell;

pub const LIMBGROWER_ROOT: &str = "/obj/machinery/limbgrower";
const FULLUPGRADE: &str = "/obj/machinery/limbgrower/fullupgrade";
pub const LIMB_DISK_ROOT: &str = "/obj/item/disk/design_disk/limbs";
const BODYPART_ROOT: &str = "/obj/item/bodypart";
/// `RND_CATEGORY_INITIAL`, `RND_CATEGORY_HACKED`.
const INITIAL: &str = "initial";
const HACKED: &str = "hacked";
/// `SPECIES_HUMAN`.
const HUMAN: &str = "human";
/// `categories`: the species tabs, `RND_CATEGORY_LIMBS_OTHER`, `RND_CATEGORY_LIMBS_DIGITIGRADE`.
pub const CATEGORIES: [&str; 8] = [
    HUMAN,
    "lizard",
    "moth",
    "plasmaman",
    "ethereal",
    "cerulean",
    "Other Limbs",
    "digitigrade",
];
/// `create_reagents(100, OPENCONTAINER)`.
const VOLUME: f32 = 100.0;
/// `production_speed`.
const PRODUCTION_SECS: f64 = 3.0;
/// The disk's load `do_after`.
const LOAD_SECS: f64 = 2.0;

/// A limb grower's live state; its reagents are its [`Reagents`].
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct Limbgrower {
    pub busy: bool,
    pub emagged: bool,
    /// `selected_category` of the print in progress.
    pub selected_category: String,
    /// `imported_designs`: design types.
    pub imported: Vec<String>,
    /// The print in progress: (design type, reagent type -> units, ends at in game seconds).
    pub building: Option<(String, Vec<(String, f32)>, f64)>,
    /// A disk load in progress: (user, disk, ends at, user's tile).
    pub loading: Option<(Entity, Entity, f64, TilePos)>,
    view: LimbgrowerData,
    static_view: LimbgrowerStatic,
}

pub(super) fn build(app: &mut App) {
    use crate::click_handlers::{AddClickHandler, ClickPhase};
    // tg `limbgrower/ui_interact`.
    app.add_click_handler(ClickPhase::EmptyHand, 2670, |w, c| {
        crate::medical::limbgrower::interact(w, c.client, c.mob, c.target)
    });
    // tg `limbgrower/item_interaction`: limb design disks.
    app.add_click_handler(ClickPhase::HeldOnTarget, 2630, |w, c| {
        c.held.is_some_and(|h| {
            crate::medical::limbgrower::item_interaction(w, c.client, c.mob, h, c.target)
        })
    });
    app.register_ui::<LimbgrowerWindow>()
        .add_observer(attach)
        .add_systems(
            Update,
            (
                cap_windows.in_set(UiSystems::StatusHook),
                acts.in_set(UiSystems::Handle),
                refresh_views
                    .after(UiSystems::Handle)
                    .before(UiSystems::Send),
                timers.run_if(any_with_component::<Limbgrower>),
            ),
        );
}

/// `Initialize`: the open holder.
fn attach(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    let Ok(o) = objs.get(add.entity) else {
        return;
    };
    if !is_under(&o.path, LIMBGROWER_ROOT) {
        return;
    }
    let holder = Holder::new(
        VOLUME,
        flags::OPENCONTAINER,
        Owner {
            path: o.path.clone(),
            living: false,
        },
    );
    commands
        .entity(add.entity)
        .insert_if_new((Limbgrower::default(), Reagents(holder)));
}

/// `RefreshParts`: `production_coefficient` from the servos' tiers.
pub fn production_coefficient(servo_tiers: u32) -> f32 {
    (1.25 - 0.25 * servo_tiers as f32).clamp(0.0, 1.0)
}

fn coefficient(world: &World, e: Entity) -> f32 {
    let r = crate::stock_parts::ratings(world, e);
    production_coefficient(r.sum(PartKind::Servo))
}

/// `RefreshParts`: the holder holds what the beakers among the parts hold.
fn max_volume(world: &World, e: Entity) -> f32 {
    let volume: f32 = world
        .get::<crate::stock_parts::ComponentParts>(e)
        .map(|p| {
            p.parts()
                .iter()
                .filter(|&&part| {
                    world
                        .get::<DmItem>(part)
                        .is_some_and(|i| is_under(&i.path, "/obj/item/reagent_containers/cup"))
                })
                .filter_map(|&part| world.get::<Reagents>(part))
                .map(|r| r.0.maximum_volume)
                .sum()
        })
        .unwrap_or(0.0);
    if volume > 0.0 { volume } else { VOLUME }
}

/// The design indices it can print: `researched_designs` (`initial`), the imported ones and,
/// emagged, `hacked_designs`. The flag says whether a design is hacked.
fn available(world: &World, e: Entity) -> Vec<(usize, bool)> {
    let (Some(db), Some(lg)) = (
        world.get_resource::<ResearchDb>(),
        world.get::<Limbgrower>(e),
    ) else {
        return Vec::new();
    };
    let full = world
        .get::<DmObj>(e)
        .is_some_and(|o| is_under(&o.path, FULLUPGRADE));
    db.designs
        .iter()
        .enumerate()
        .filter(|(_, d)| d.build_type & build_type::LIMBGROWER != 0)
        .filter_map(|(i, d)| {
            let hacked = d.category.iter().any(|c| c == HACKED);
            let shown = d.category.iter().any(|c| c == INITIAL)
                || (!hacked && full)
                || lg.imported.contains(&d.id)
                || (hacked && lg.emagged);
            shown.then_some((i, hacked))
        })
        .collect()
}

fn reagent_name(world: &World, path: &str) -> String {
    world
        .get_resource::<Chem>()
        .and_then(|c| c.0.id(path).map(|id| c.0.def(id).name.clone()))
        .unwrap_or_else(|| path.to_owned())
}

/// `ui_static_data`.
fn compute_static(world: &World, e: Entity) -> LimbgrowerStatic {
    let coefficient = coefficient(world, e);
    let designs = available(world, e);
    let Some(db) = world.get_resource::<ResearchDb>() else {
        return LimbgrowerStatic::default();
    };
    let categories = CATEGORIES
        .iter()
        .map(|&category| LimbgrowerCategory {
            name: category.to_owned(),
            designs: designs
                .iter()
                .map(|&(i, _)| &db.designs[i])
                .filter(|d| d.category.iter().any(|c| c == category))
                .map(|d| LimbgrowerDesign {
                    parent_category: category.to_owned(),
                    name: d.name.clone(),
                    path: d.id.clone(),
                    needed_reagents: d
                        .reagents
                        .iter()
                        .map(|(r, n)| (reagent_name(world, r), *n as f32 * coefficient))
                        .collect(),
                })
                .collect(),
        })
        .collect();
    LimbgrowerStatic { categories }
}

/// `ui_data`; the holder's volume follows the parts.
fn compute_view(world: &mut World, e: Entity) -> LimbgrowerData {
    let max = max_volume(world, e);
    if let Some(mut r) = world.get_mut::<Reagents>(e)
        && r.0.maximum_volume != max
    {
        r.0.maximum_volume = max;
    }
    let busy = world.get::<Limbgrower>(e).is_some_and(|l| l.busy);
    let chem = world.get_resource::<Chem>().map(|c| c.0.clone());
    let Some(holder) = world.get::<Reagents>(e).map(|r| &r.0) else {
        return LimbgrowerData::default();
    };
    let reagents = holder
        .reagents
        .iter()
        .map(|r| {
            let def = chem.as_ref().map(|c| c.def(r.id));
            LimbgrowerReagent {
                reagent_name: def.map_or_else(String::new, |d| d.name.clone()),
                reagent_amount: r.volume,
                reagent_type: def.map_or_else(String::new, |d| d.path.clone()),
            }
        })
        .collect();
    LimbgrowerData {
        reagents,
        total_reagents: holder.total_volume,
        max_reagents: holder.maximum_volume,
        busy,
    }
}

fn refresh_view(world: &mut World, e: Entity) {
    if world.get::<Limbgrower>(e).is_none() {
        return;
    }
    let view = compute_view(world, e);
    if let Some(mut l) = world.get_mut::<Limbgrower>(e)
        && l.view != view
    {
        l.view = view;
    }
}

/// `update_static_data`: the designs again, for the open windows too.
fn refresh_static(world: &mut World, e: Entity) {
    let st = compute_static(world, e);
    let changed = world
        .get::<Limbgrower>(e)
        .is_some_and(|l| l.static_view != st);
    if !changed {
        return;
    }
    if let Some(mut l) = world.get_mut::<Limbgrower>(e) {
        l.static_view = st;
    }
    let mut q = world.query_filtered::<&mut UiWindow, With<UiKind<LimbgrowerWindow>>>();
    for mut w in q.iter_mut(world) {
        if w.source == e {
            w.refresh_static();
        }
    }
}

/// `update_icon_state`: `limbgrower_idleon` while busy, `_panelopen`, else `_idleoff`.
fn refresh_look(world: &mut World, e: Entity) {
    let state = if world.get::<Limbgrower>(e).is_some_and(|l| l.busy) {
        "limbgrower_idleon"
    } else if panel_open(world, e) {
        "limbgrower_panelopen"
    } else {
        "limbgrower_idleoff"
    };
    crate::obj_damage::set_icon_state(world, e, state);
}

/// `check_busy`.
fn check_busy(world: &mut World, client: Entity, e: Entity) -> bool {
    let busy = world.get::<Limbgrower>(e).is_some_and(|l| l.busy);
    if busy {
        tell(
            world,
            client,
            Span::Warning,
            "Выращиватель конечностей занят. Дождитесь завершения предыдущей операции.",
        );
    }
    busy
}

/// `ui_interact`. True when `target` is a limb grower.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if world.get::<Limbgrower>(target).is_none() {
        return false;
    }
    if !operational(world, target) {
        return true;
    }
    refresh_static(world, target);
    refresh_view(world, target);
    world.write_message(OpenUi::new::<LimbgrowerWindow>(
        client,
        mob,
        target,
        "Limb Grower",
    ));
    true
}

/// `emag_act`: the hacked designs (synthetic arm blades). `None` for another target.
pub fn emag_act(world: &mut World, client: Entity, e: Entity) -> Option<bool> {
    let lg = world.get::<Limbgrower>(e)?;
    if lg.emagged {
        return Some(false);
    }
    if let Some(mut l) = world.get_mut::<Limbgrower>(e) {
        l.emagged = true;
    }
    refresh_static(world, e);
    tell(
        world,
        client,
        Span::Notice,
        "производство нелегальных конечностей разблокировано",
    );
    Some(true)
}

/// `item_interaction`: a limb design disk starts loading. True when the click was the grower's.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if world.get::<Limbgrower>(target).is_none()
        || world
            .get::<crate::combat::CombatMode>(mob)
            .is_some_and(|c| c.0)
    {
        return false;
    }
    let disk = world
        .get::<DmItem>(held)
        .is_some_and(|i| is_under(&i.path, LIMB_DISK_ROOT));
    if !disk {
        return false;
    }
    if check_busy(world, client, target) {
        return true;
    }
    let Some(&pos) = world.get::<TilePos>(mob) else {
        return true;
    };
    let ends = world.resource::<Time>().elapsed_secs_f64() + LOAD_SECS;
    if let Some(mut l) = world.get_mut::<Limbgrower>(target) {
        l.busy = true;
        l.loading = Some((mob, held, ends, pos));
    }
    let (who, what, grower) = (
        super::name_of(world, mob),
        super::name_of(world, held),
        super::name_of(world, target),
    );
    super::visible(
        world,
        mob,
        &format!("Вы начинаете загружать чертежи с {what}..."),
        &format!("{who} начинает загружать {what} в {grower}..."),
        Span::Notice,
    );
    refresh_look(world, target);
    true
}

/// The loads and prints that are due.
fn timers(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let due: Vec<(Entity, Limbgrower)> = world
        .query::<(Entity, &Limbgrower)>()
        .iter(world)
        .filter(|(_, l)| {
            l.loading.is_some_and(|(_, _, t, _)| now >= t)
                || l.building.as_ref().is_some_and(|(_, _, t)| now >= *t)
        })
        .map(|(e, l)| (e, l.clone()))
        .collect();
    for (e, lg) in due {
        if let Some((user, disk, ends, pos)) = lg.loading
            && now >= ends
        {
            finish_load(world, e, user, disk, pos);
        }
        if let Some((design, reagents, ends)) = lg.building
            && now >= ends
        {
            build_item(world, e, &design, &reagents, &lg.selected_category);
        }
        refresh_look(world, e);
        refresh_view(world, e);
    }
}

/// The end of the disk's `do_after`: the user stayed and still holds it.
fn finish_load(world: &mut World, e: Entity, user: Entity, disk: Entity, pos: TilePos) {
    if let Some(mut l) = world.get_mut::<Limbgrower>(e) {
        l.loading = None;
        l.busy = false;
    }
    let stayed = world.get::<TilePos>(user) == Some(&pos)
        && world.get::<Inventory>(user).is_some_and(|i| i.holds(disk));
    if !stayed {
        return;
    }
    let Some(path) = world.get::<DmItem>(disk).map(|i| i.path.clone()) else {
        return;
    };
    let blueprints = world
        .get_resource::<ResearchDb>()
        .and_then(|db| db.limb_disks.get(&path).cloned())
        .unwrap_or_default();
    if let Some(mut l) = world.get_mut::<Limbgrower>(e) {
        for design in blueprints {
            if !l.imported.contains(&design) {
                l.imported.push(design);
            }
        }
    }
    refresh_static(world, e);
}

/// `build_item`: the reagents are used, the item made on the grower's tile.
// tg builds even when a reagent went missing meanwhile (it only breaks out of the removal loop);
// here the print is cancelled instead.
fn build_item(
    world: &mut World,
    e: Entity,
    design: &str,
    reagents: &[(String, f32)],
    category: &str,
) {
    if let Some(mut l) = world.get_mut::<Limbgrower>(e) {
        l.building = None;
        l.busy = false;
    }
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return;
    };
    let ids: Option<Vec<_>> = reagents
        .iter()
        .map(|(path, n)| chem.id(path).map(|id| (id, *n)))
        .collect();
    let enough = ids.as_ref().is_some_and(|ids| {
        world
            .get::<Reagents>(e)
            .is_some_and(|r| ids.iter().all(|&(id, n)| r.0.amount(id) + 0.0001 >= n))
    });
    let Some(ids) = ids.filter(|_| enough) else {
        crate::atmos_machinery::machine_say(world, e, "*жужжит*");
        return;
    };
    if let Some(mut r) = world.get_mut::<Reagents>(e) {
        for (id, n) in ids {
            r.0.remove_reagent(id, n);
        }
    }
    let Some(def) = world
        .get_resource::<ResearchDb>()
        .and_then(|db| db.design_index.get(design).map(|&i| db.designs[i].clone()))
    else {
        return;
    };
    let (Some(build), Some(&pos)) = (def.build_path.clone(), world.get::<TilePos>(e)) else {
        return;
    };
    if is_under(&build, BODYPART_ROOT) {
        // `create_buildpath`: humans use the parent type.
        let species = format!("{build}/{category}");
        let path = if category != HUMAN && def.variants.contains(&species) {
            species
        } else {
            build.clone()
        };
        let Some(limb) = crate::construction::spawn_obj(world, &path, pos) else {
            return;
        };
        if let Some(zone) = super::bodyparts::zone_of_path(&build) {
            let name = format!(
                "{} (синтетическая, {category})",
                ss13_chat::speech::capitalize(zone.ru().0)
            );
            world.entity_mut(limb).insert(Name::new(name));
        }
    } else {
        crate::construction::spawn_obj(world, &build, pos);
    }
}

/// Windows of a broken or unpowered grower close.
fn cap_windows(world: &mut World) {
    let windows: Vec<(Entity, Entity)> = world
        .query_filtered::<(Entity, &UiWindow), With<UiKind<LimbgrowerWindow>>>()
        .iter(world)
        .map(|(w, window)| (w, window.source))
        .collect();
    for (window, e) in windows {
        if !operational(world, e)
            && let Some(mut w) = world.get_mut::<UiWindow>(window)
        {
            w.cap_status(UiStatus::Close);
        }
    }
}

/// Growers with an open window get fresh data.
// Cost: per tick, one pass over the open windows; the data is the holder's few reagents.
fn refresh_views(world: &mut World) {
    let mut q = world.query_filtered::<&UiWindow, With<UiKind<LimbgrowerWindow>>>();
    let mut growers: Vec<Entity> = q.iter(world).map(|w| w.source).collect();
    growers.sort();
    growers.dedup();
    for e in growers {
        refresh_view(world, e);
    }
}

/// The server side of the `Limbgrower` interface.
pub struct LimbgrowerWindow;

impl UiInterface for LimbgrowerWindow {
    const NAME: &'static str = LimbgrowerUi::NAME;
    type Data = LimbgrowerData;
    type StaticData = LimbgrowerStatic;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LimbgrowerAct {
    /// `reagent_type`.
    EmptyReagent(String),
    /// `design_path`, `active_tab`.
    MakeLimb(String, String),
}

impl ServerUi for LimbgrowerWindow {
    type Source = &'static Limbgrower;
    type UserParam = ();
    type Act = LimbgrowerAct;

    fn static_data(lg: &Limbgrower) -> LimbgrowerStatic {
        lg.static_view.clone()
    }

    fn data(lg: &Limbgrower, _: Entity, _: &()) -> LimbgrowerData {
        lg.view.clone()
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<LimbgrowerAct, UiReject> {
        let param = |key: &str| {
            params
                .get(key)
                .filter(|v| v.len() <= 128)
                .map(str::to_owned)
                .ok_or_else(|| UiReject::BadParams(key.into()))
        };
        Ok(match action {
            "empty_reagent" => LimbgrowerAct::EmptyReagent(param("reagent_type")?),
            "make_limb" => LimbgrowerAct::MakeLimb(param("design_path")?, param("active_tab")?),
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

fn acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<LimbgrowerWindow>>>) {
    let pending: Vec<(Entity, Entity, Entity, LimbgrowerAct)> = acts
        .read(world.resource::<Messages<UiActed<LimbgrowerWindow>>>())
        .map(|a| (a.source, a.client, a.actor, a.act.clone()))
        .collect();
    for (e, client, _, act) in pending {
        apply_act(world, client, e, &act);
    }
}

/// `ui_act`, checked at the moment of effect. True when a print started or a reagent went.
pub fn apply_act(world: &mut World, client: Entity, e: Entity, act: &LimbgrowerAct) -> bool {
    if world.get::<Limbgrower>(e).is_none() || check_busy(world, client, e) {
        return false;
    }
    let done = match act {
        LimbgrowerAct::EmptyReagent(path) => {
            let id = world.get_resource::<Chem>().and_then(|c| c.0.id(path));
            match (id, world.get_mut::<Reagents>(e)) {
                (Some(id), Some(mut r)) => {
                    let amount = r.0.amount(id);
                    r.0.remove_reagent(id, amount) > 0.0
                }
                _ => false,
            }
        }
        LimbgrowerAct::MakeLimb(design, tab) => make_limb(world, e, design, tab),
    };
    refresh_look(world, e);
    refresh_view(world, e);
    done
}

/// `ui_act("make_limb")`.
fn make_limb(world: &mut World, e: Entity, design: &str, tab: &str) -> bool {
    if !CATEGORIES.contains(&tab) {
        return false;
    }
    let Some(index) = world
        .get_resource::<ResearchDb>()
        .and_then(|db| db.design_index.get(design).copied())
    else {
        return false;
    };
    if !available(world, e).iter().any(|&(i, _)| i == index) {
        return false;
    }
    let coefficient = coefficient(world, e);
    let needed: Vec<(String, f32)> = world.resource::<ResearchDb>().designs[index]
        .reagents
        .iter()
        .map(|(r, n)| (r.clone(), *n as f32 * coefficient))
        .collect();
    let Some(chem) = world.get_resource::<Chem>().map(|c| c.0.clone()) else {
        return false;
    };
    let watts = world
        .get::<ss13_mapload::DmPower>(e)
        .map_or(0.0, |p| p.active_power_usage);
    let mut power = 0.0_f64;
    for (path, amount) in &needed {
        let has = chem.id(path).is_some_and(|id| {
            world
                .get::<Reagents>(e)
                .is_some_and(|r| r.0.amount(id) + 0.0001 >= *amount)
        });
        if !has {
            let name = super::name_of(world, e);
            super::visible(world, e, "", &format!("{name} жужжит."), Span::Notice);
            return false;
        }
        power = watts.max(power + f64::from(*amount));
    }
    crate::kitchen::machine_use_energy(world, e, power);
    let ends =
        world.resource::<Time>().elapsed_secs_f64() + PRODUCTION_SECS * f64::from(coefficient);
    if let Some(mut l) = world.get_mut::<Limbgrower>(e) {
        l.busy = true;
        l.selected_category = tab.to_owned();
        l.building = Some((design.to_owned(), needed, ends));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn servos_cut_the_reagent_cost() {
        assert_eq!(production_coefficient(0), 1.0);
        assert_eq!(production_coefficient(1), 1.0);
        assert_eq!(production_coefficient(2), 0.75);
        assert_eq!(production_coefficient(4), 0.25);
        assert_eq!(production_coefficient(9), 0.0);
    }
}
