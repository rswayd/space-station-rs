//! The rapid pipe dispenser: tg `/obj/item/pipe_dispenser` (`game/objects/items/rcd/RPD.dm`)
//! with its `RapidPipeDispenser` window, atmos piping only.
//!
//! - In hand it opens its window: category, modes (Dispense, Connect, Destroy, Reprogram), paint
//!   color, layers (one, or several with Multi), smart pipe directions, and the recipe with its
//!   direction from the previews.
//! - Used on a turf (or a lattice, girder, window, grille, fitting; an atmos machine means its
//!   turf), in Dispense mode: per chosen layer 0.4 s, then the fitting (or meter) of the recipe,
//!   wrenched into place in Connect mode. Recipes that are not `all_layers` refuse layers 1 and
//!   5 (Multi skips them).
//! - Destroy mode deletes fittings and meter items; with the unwrench upgrade disk it wrenches
//!   atmos machines off instead. Reprogram mode sets a smart pipe's free directions (never its
//!   live ones, never fewer than two), or a smart pipe fitting's.
//! - A right click on an atmos machine copies its color and layer.
// shortcut: no mouse wheel layer change (the client sends no scroll), no disposal or transit
// tube categories (their construction is not ported), no sparks or switch sounds, no
// `ISADVANCEDTOOLUSER` check (only humans hold it in practice).

use std::sync::Arc;

use bevy::ecs::message::{MessageCursor, Messages};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_atmos::pipenet::{DeviceKind, Pipenet};
use ss13_chat::Span;
use ss13_mapload::atmos_machine::ATMOS_PIPE_RECIPES;
use ss13_mapload::{DmAtmosDevice, DmItem, DmName, DmObj, IconIndex};
use ss13_shared::TilePos;
use ss13_shared::protocol::ClickModifiers;
use ss13_tgui::UiInterface;
use ss13_tgui::interfaces::pipe_dispenser::InitDirections;
use ss13_tgui::interfaces::rapid_pipe_dispenser::{
    RapidPipeDispenserUi, RpdCategory, RpdData, RpdPreview, RpdRecipe, RpdStatic, text_dir,
};
use ss13_tgui::server::{AlwaysState, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiWindow};

use super::AtmosDevice;
use super::fitting::{FittingKind, MeterFitting, PipeFitting};
use super::thermo::PIPE_PAINT_COLORS;
use crate::inventory::CarriedBy;
use crate::obj_damage::is_under;
use crate::tools::{Actor, tell};

pub const RPD: &str = "/obj/item/pipe_dispenser";
const UNWRENCH_DISK: &str = "/obj/item/rpd_upgrade/unwrench";
const PIPE_ITEM_DMI: &str = "icons/obj/pipes_n_cables/pipe_item.dmi";
const USE_SOUND: &str = "sound/items/deconstruct.ogg";
/// `atmos_build_speed`.
const BUILD_DS: f32 = 4.0;
const MAX_PIPE_LAYERS: u8 = 5;
const ALL_CARDINALS: u8 = 15;

/// `BUILD_MODE`, `WRENCH_MODE`, `DESTROY_MODE`, `REPROGRAM_MODE`.
pub const BUILD: u8 = 1;
pub const WRENCH: u8 = 2;
pub const DESTROY: u8 = 4;
pub const REPROGRAM: u8 = 8;

/// `/obj/item/pipe_dispenser`'s settings.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct Rpd {
    /// The recipe: category and index in `GLOB.atmos_pipe_recipes`.
    pub recipe: (usize, usize),
    pub p_dir: u8,
    pub p_flipped: bool,
    /// `p_init_dir`.
    pub init_dir: u8,
    /// Index into [`PIPE_PAINT_COLORS`].
    pub color: usize,
    /// `PIPE_LAYER(n)` bits.
    pub pipe_layers: u8,
    pub multi_layer: bool,
    pub mode: u8,
    /// `RPD_UPGRADE_UNWRENCH`.
    pub unwrench: bool,
}

impl Default for Rpd {
    fn default() -> Self {
        Self {
            recipe: (0, 0),
            p_dir: 1,
            p_flipped: false,
            init_dir: ALL_CARDINALS,
            color: PIPE_PAINT_COLORS
                .iter()
                .position(|(n, _)| *n == "green")
                .unwrap_or(0),
            pipe_layers: layer_bit(3),
            multi_layer: false,
            mode: BUILD | WRENCH | DESTROY | REPROGRAM,
            unwrench: false,
        }
    }
}

/// `PIPE_LAYER(num)`.
fn layer_bit(layer: u8) -> u8 {
    1 << (layer - 1)
}

/// `get_active_pipe_layers`.
fn active_layers(bits: u8) -> impl Iterator<Item = u8> {
    (1..=MAX_PIPE_LAYERS).filter(move |&l| bits & layer_bit(l) != 0)
}

/// The recipe list with icons and previews, made once per server and shared by every RPD (its
/// window's static data).
#[derive(Component, Clone, Debug)]
pub struct RpdCatalog(pub Arc<Vec<RpdCategory>>);

/// `get_preview`'s directions per `RPD_type`.
fn previews(kind: Option<FittingKind>, name: &str) -> Vec<RpdPreview> {
    let row = |dir: u8, dir_name: &str, flipped: bool| RpdPreview {
        dir,
        dir_name: dir_name.to_owned(),
        flipped,
    };
    let plain = |list: &[(u8, &str)]| list.iter().map(|&(d, n)| row(d, n, false)).collect();
    match kind {
        Some(FittingKind::Binary) => plain(&[(1, "Vertical"), (4, "Horizontal")]),
        Some(FittingKind::Bendable) => plain(&[
            (1, "Vertical"),
            (4, "Horizontal"),
            (9, "West to North"),
            (5, "North to East"),
            (10, "South to West"),
            (6, "East to South"),
        ]),
        Some(FittingKind::Trinary) => plain(&[
            (1, "West South East"),
            (2, "East North West"),
            (4, "North West South"),
            (8, "South East North"),
        ]),
        Some(FittingKind::Flippable) => [
            (1, "North East South"),
            (10, "North West South"),
            (5, "South East North"),
            (2, "South West North"),
            (8, "West North East"),
            (6, "West South East"),
            (9, "East North West"),
            (4, "East South West"),
        ]
        .iter()
        .map(|&(d, n)| row(d, n, d & d.wrapping_sub(1) != 0))
        .collect(),
        Some(FittingKind::Directional) => {
            plain(&[(1, "North"), (2, "South"), (8, "West"), (4, "East")])
        }
        // `PIPE_ONEDIR`: smart pipes, 4-way manifolds, the meter.
        Some(FittingKind::Quaternary) | None => vec![row(2, name, false)],
    }
}

fn catalog(world: &World) -> Vec<RpdCategory> {
    let protos = world.get_resource::<ss13_proto::Prototypes>();
    let icons = world.get_resource::<IconIndex>();
    ATMOS_PIPE_RECIPES
        .iter()
        .map(|(cat, recipes)| RpdCategory {
            cat_name: (*cat).to_owned(),
            recipes: recipes
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let dm = r
                        .path
                        .and_then(|p| protos?.get(p)?.get::<DmAtmosDevice>().cloned());
                    let kind = dm.as_ref().map(|d| FittingKind::of(&d.construction_type));
                    let state = dm.as_ref().map_or("meter", |d| d.pipe_state.as_str());
                    let icon = icons.and_then(|i| {
                        i.0.get(&(PIPE_ITEM_DMI.to_owned(), state.to_owned()))
                            .copied()
                    });
                    RpdRecipe {
                        pipe_index: i as u32 + 1,
                        pipe_name: r.name.to_owned(),
                        pipe_desc: r.desc.to_owned(),
                        icon,
                        previews: previews(kind, r.name),
                    }
                })
                .collect(),
        })
        .collect()
}

/// The catalog, built when a window first needs it: RPDs placed by the map appear while the
/// loader holds the prototypes.
#[derive(Resource, Clone)]
struct CatalogCache(Arc<Vec<RpdCategory>>);

/// `Initialize`: RPDs get their settings however they appear.
pub fn attach(add: On<Add<DmItem>>, items: Query<&DmItem>, mut commands: Commands) {
    let Ok(item) = items.get(add.entity) else {
        return;
    };
    if is_under(&item.path, RPD) {
        commands
            .entity(add.entity)
            .insert_if_new((Rpd::default(), AlwaysState));
    }
}

/// The RPD's catalog, given to it on first use.
pub fn catalog_of(world: &mut World, item: Entity) -> Arc<Vec<RpdCategory>> {
    if let Some(c) = world.get::<RpdCatalog>(item) {
        return c.0.clone();
    }
    let list = match world.get_resource::<CatalogCache>() {
        Some(c) => c.0.clone(),
        None => {
            let list = Arc::new(catalog(world));
            world.insert_resource(CatalogCache(list.clone()));
            list
        }
    };
    world.entity_mut(item).insert(RpdCatalog(list.clone()));
    list
}

pub struct RpdWindow;

impl UiInterface for RpdWindow {
    const NAME: &'static str = RapidPipeDispenserUi::NAME;
    type Data = RpdData;
    type StaticData = RpdStatic;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RpdAct {
    Color(usize),
    /// Only atmos (0) is ported.
    Category,
    Layers(u8),
    ToggleMulti,
    /// Category, index, then the preview's direction and flip when a preview was clicked.
    PipeType(usize, usize, Option<(u8, bool)>),
    SetDir(u8, bool),
    Mode(u8),
    InitDir(u8),
    InitReset,
}

fn bad(what: &str) -> UiReject {
    UiReject::BadParams(what.to_owned())
}

/// `dir` (`text2dir`) and `flipped` (`text2num`).
fn dir_params(params: &UiParams) -> Result<(u8, bool), UiReject> {
    let dir = params
        .get("dir")
        .and_then(text_dir)
        .ok_or_else(|| bad("dir"))?;
    let flipped = params.number("flipped").unwrap_or(0.0) != 0.0;
    Ok((dir, flipped))
}

impl ServerUi for RpdWindow {
    type Source = (&'static Rpd, &'static RpdCatalog);
    type UserParam = ();
    type Act = RpdAct;

    fn static_data((_, catalog): (&Rpd, &RpdCatalog)) -> RpdStatic {
        RpdStatic {
            paint_colors: PIPE_PAINT_COLORS
                .iter()
                .map(|(n, [r, g, b])| ((*n).to_owned(), format!("#{r:02X}{g:02X}{b:02X}")))
                .collect(),
            max_pipe_layers: MAX_PIPE_LAYERS,
            categories: catalog.0.as_ref().clone(),
        }
    }

    fn data((rpd, _): (&Rpd, &RpdCatalog), _: Entity, _: &SystemParamItem<'_, '_, ()>) -> RpdData {
        RpdData {
            category: 0,
            mode: rpd.mode,
            multi_layer: rpd.multi_layer,
            pipe_layers: rpd.pipe_layers,
            selected_category: ATMOS_PIPE_RECIPES
                .get(rpd.recipe.0)
                .map_or("", |c| c.0)
                .to_owned(),
            selected_recipe: rpd.recipe.1 as u32 + 1,
            p_dir: rpd.p_dir,
            selected_color: PIPE_PAINT_COLORS
                .get(rpd.color)
                .map_or("", |c| c.0)
                .to_owned(),
            init_directions: InitDirections::from_bits(rpd.init_dir),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<RpdAct, UiReject> {
        match action {
            "color" => {
                let name = params
                    .get("paint_color")
                    .ok_or_else(|| bad("paint_color"))?;
                PIPE_PAINT_COLORS
                    .iter()
                    .position(|(n, _)| *n == name)
                    .map(RpdAct::Color)
                    .ok_or_else(|| bad("paint_color"))
            }
            "category" => match params.number("category")? {
                0.0 => Ok(RpdAct::Category),
                _ => Err(bad("category: only atmos is ported")),
            },
            "pipe_layers" => {
                let bits = params.number("pipe_layers")?;
                if bits.fract() != 0.0 || !(1.0..32.0).contains(&bits) {
                    return Err(bad("pipe_layers"));
                }
                Ok(RpdAct::Layers(bits as u8))
            }
            "toggle_multi_layer" => Ok(RpdAct::ToggleMulti),
            "pipe_type" => {
                let category = params.get("category").ok_or_else(|| bad("category"))?;
                let c = ATMOS_PIPE_RECIPES
                    .iter()
                    .position(|(n, _)| *n == category)
                    .ok_or_else(|| bad("category"))?;
                let index = params.number("pipe_type")?;
                if index.fract() != 0.0
                    || index < 1.0
                    || index > ATMOS_PIPE_RECIPES[c].1.len() as f64
                {
                    return Err(bad("pipe_type"));
                }
                let dir = params.get("dir").map(|_| dir_params(params)).transpose()?;
                Ok(RpdAct::PipeType(c, index as usize - 1, dir))
            }
            "setdir" => dir_params(params).map(|(d, f)| RpdAct::SetDir(d, f)),
            "mode" => match params.number("mode")? {
                m if [BUILD, WRENCH, DESTROY, REPROGRAM]
                    .iter()
                    .any(|&b| f64::from(b) == m) =>
                {
                    Ok(RpdAct::Mode(m as u8))
                }
                _ => Err(bad("mode")),
            },
            "init_dir_setting" => match params.get("dir_flag") {
                Some("north") => Ok(RpdAct::InitDir(1)),
                Some("south") => Ok(RpdAct::InitDir(2)),
                Some("east") => Ok(RpdAct::InitDir(4)),
                Some("west") => Ok(RpdAct::InitDir(8)),
                _ => Err(bad("dir_flag")),
            },
            "init_reset" => Ok(RpdAct::InitReset),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// `ui_act` on the settings; `Err` carries what the user is told, if anything.
pub fn apply_act(rpd: &mut Rpd, act: RpdAct) -> Result<(), Option<&'static str>> {
    match act {
        RpdAct::Color(i) => rpd.color = i,
        RpdAct::Category => {
            rpd.recipe = (0, 0);
            rpd.p_dir = 1;
        }
        RpdAct::Layers(bits) => {
            if bits & 0b1_1111 == 0 {
                return Err(None);
            }
            if rpd.multi_layer {
                if rpd.pipe_layers != bits {
                    rpd.pipe_layers ^= bits;
                }
            } else {
                rpd.pipe_layers = bits;
            }
        }
        RpdAct::ToggleMulti => {
            if rpd.multi_layer {
                let top = active_layers(rpd.pipe_layers).max().unwrap_or(3);
                rpd.pipe_layers = layer_bit(top);
            }
            rpd.multi_layer = !rpd.multi_layer;
        }
        RpdAct::PipeType(c, i, dir) => {
            rpd.recipe = (c, i);
            rpd.p_dir = 1;
            if let Some((d, f)) = dir {
                rpd.p_dir = d;
                rpd.p_flipped = f;
            }
        }
        RpdAct::SetDir(d, f) => {
            rpd.p_dir = d;
            rpd.p_flipped = f;
        }
        RpdAct::Mode(bit) => rpd.mode ^= bit,
        RpdAct::InitDir(bit) => {
            let target = rpd.init_dir ^ bit;
            if target & target.wrapping_sub(1) == 0 {
                return Err(Some(
                    "Экран RPD мигает предупреждением: нельзя настроить трубу на соединение \
                     только в одном направлении.",
                ));
            }
            rpd.init_dir = target;
        }
        RpdAct::InitReset => rpd.init_dir = ALL_CARDINALS,
    }
    Ok(())
}

pub fn rpd_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<RpdWindow>>>) {
    let pending: Vec<(Entity, Entity, RpdAct)> = acts
        .read(world.resource::<Messages<UiActed<RpdWindow>>>())
        .map(|a| (a.client, a.source, a.act.clone()))
        .collect();
    for (client, source, act) in pending {
        let Some(mut rpd) = world.get_mut::<Rpd>(source) else {
            continue;
        };
        if let Err(Some(why)) = apply_act(&mut rpd, act) {
            tell(world, client, Span::Warning, why);
        }
    }
}

/// The window closes when the RPD leaves its user's hands or bag (`default_state`).
pub fn window_status(mut windows: Query<&mut UiWindow>, carried: Query<&CarriedBy>) {
    for mut window in &mut windows {
        if window.interface == RpdWindow::NAME
            && !carried
                .get(window.source)
                .is_ok_and(|c| c.0 == window.actor)
        {
            window.cap_status(ss13_tgui::UiStatus::Close);
        }
    }
}

/// `attack_self`: the window.
pub fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world.get::<Rpd>(item).is_none() {
        return false;
    }
    catalog_of(world, item);
    let title = world
        .get::<DmName>(item)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default();
    world.write_message(OpenUi::new::<RpdWindow>(client, mob, item, title));
    true
}

fn path_of(world: &World, e: Entity) -> Option<String> {
    world
        .get::<DmObj>(e)
        .map(|o| o.path.clone())
        .or_else(|| world.get::<DmItem>(e).map(|i| i.path.clone()))
}

/// `check_can_make_pipe`: a turf, or what pipes are laid through.
fn can_make_pipe(world: &World, target: Entity) -> bool {
    if world.get::<ss13_mapload::Turf>(target).is_some() {
        return true;
    }
    world.get::<PipeFitting>(target).is_some()
        || path_of(world, target).is_some_and(|p| {
            [
                "/obj/structure/lattice",
                "/obj/structure/girder",
                "/obj/structure/window",
                "/obj/structure/grille",
            ]
            .iter()
            .any(|root| is_under(&p, root))
        })
}

fn play(world: &mut World, pos: TilePos) {
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, USE_SOUND, 50.0, true),
    );
}

/// The RPD used on `target` (`interact_with_atom`, `interact_with_atom_secondary`). True when
/// the click was its.
pub fn item_interaction(
    world: &mut World,
    actor: Actor,
    rpd_item: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    let Some(rpd) = world.get::<Rpd>(rpd_item).cloned() else {
        return false;
    };
    let client = actor.client;
    if modifiers.right {
        return copy_settings(world, client, rpd_item, target);
    }
    // An upgrade disk goes in.
    if path_of(world, target).is_some_and(|p| is_under(&p, UNWRENCH_DISK)) {
        if rpd.unwrench {
            tell(world, client, Span::Warning, "Уже установлено!");
        } else {
            if let Some(mut r) = world.get_mut::<Rpd>(rpd_item) {
                r.unwrench = true;
            }
            world.despawn(target);
            tell(world, client, Span::Notice, "Улучшение установлено.");
        }
        return true;
    }
    let machine = world.get::<AtmosDevice>(target).is_some();
    if rpd.mode & DESTROY != 0 && rpd.unwrench && machine {
        // tg builds nothing after the unwrench (`wrench_act` returns a flag, not an atom).
        return super::fitting::wrench_off(world, actor, target);
    }
    if rpd.mode & DESTROY != 0
        && (world.get::<PipeFitting>(target).is_some()
            || world.get::<MeterFitting>(target).is_some())
    {
        let pos = world.get::<TilePos>(target).copied();
        world.despawn(target);
        if let Some(pos) = pos {
            play(world, pos);
        }
        return true;
    }
    if rpd.mode & REPROGRAM != 0 && reprogram(world, client, &rpd, target) {
        return true;
    }
    if rpd.mode & BUILD == 0 {
        return false;
    }
    let target = if machine {
        let Some(turf) = world
            .get::<TilePos>(target)
            .and_then(|&p| crate::construction::turf_at(world, p))
        else {
            return true;
        };
        turf
    } else {
        target
    };
    if !can_make_pipe(world, target) {
        return false;
    }
    let job = RpdJob {
        layers: rpd.pipe_layers & 0b1_1111,
        category: rpd.recipe.0 as u8,
        index: rpd.recipe.1 as u8,
        dir: rpd.p_dir,
        flipped: rpd.p_flipped,
    };
    start(world, actor, target, job);
    true
}

/// The layers still to lay and the recipe as it was when the click came (`queued_pipe_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RpdJob {
    pub layers: u8,
    pub category: u8,
    pub index: u8,
    pub dir: u8,
    pub flipped: bool,
}

fn start(world: &mut World, actor: Actor, target: Entity, job: RpdJob) {
    if job.layers == 0 {
        return;
    }
    crate::tools::use_tool(
        world,
        actor,
        target,
        BUILD_DS,
        0,
        crate::construction::Job::Atmos(super::windows::AtmosJob::Rpd(job)),
    );
}

/// One layer of `do_pipe_build` after its `do_after`, then the next.
pub fn complete(world: &mut World, mob: Entity, action: &crate::tools::DoAfter, job: RpdJob) {
    let Some(item) = action.item else { return };
    let Some(rpd) = world.get::<Rpd>(item).cloned() else {
        return;
    };
    let Some(layer) = active_layers(job.layers).next() else {
        return;
    };
    let rest = RpdJob {
        layers: job.layers & !layer_bit(layer),
        ..job
    };
    let Some(recipe) = ATMOS_PIPE_RECIPES
        .get(usize::from(job.category))
        .and_then(|c| c.1.get(usize::from(job.index)))
    else {
        return;
    };
    let client = action.client;
    let pos = action.target_pos;
    if !recipe.all_layers && (layer == 1 || layer == MAX_PIPE_LAYERS) {
        tell(
            world,
            client,
            Span::Warning,
            format!("Нельзя строить на слое {layer}!"),
        );
        if !rpd.multi_layer {
            return;
        }
    } else {
        play(world, pos);
        let built = match recipe.path {
            None => crate::construction::spawn_obj(world, super::fitting::PIPE_METER_ITEM, pos)
                .inspect(|&m| {
                    world.entity_mut(m).insert(MeterFitting { layer });
                }),
            Some(path) => {
                let kind = world
                    .get_resource::<ss13_proto::Prototypes>()
                    .and_then(|p| p.get(path))
                    .and_then(|p| p.get::<DmAtmosDevice>())
                    .map(|d| FittingKind::of(&d.construction_type));
                kind.and_then(|kind| {
                    super::fitting::spawn_fitting(
                        world,
                        pos,
                        PipeFitting {
                            pipe_type: path.to_owned(),
                            kind,
                            dir: job.dir,
                            init_dir: if is_under(path, super::SMART_PIPE) {
                                rpd.init_dir
                            } else {
                                ALL_CARDINALS
                            },
                            layer,
                            color: PIPE_PAINT_COLORS.get(rpd.color).map_or([0xEE; 3], |c| c.1),
                            flipped: job.flipped && kind == FittingKind::Flippable,
                        },
                    )
                })
            }
        };
        if rpd.mode & WRENCH != 0
            && let Some(built) = built
        {
            super::fitting::wrench_on(world, client, built);
        }
    }
    if rest.layers != 0 && world.get_entity(action.target).is_ok() {
        start(
            world,
            Actor {
                client,
                mob,
                item: Some(item),
            },
            action.target,
            rest,
        );
    }
}

/// `interact_with_atom_secondary`: the machine's color and layer.
fn copy_settings(world: &mut World, client: Entity, rpd_item: Entity, target: Entity) -> bool {
    let Some(device) = world.get::<AtmosDevice>(target).copied() else {
        return false;
    };
    let Some(color) = world
        .resource::<Pipenet>()
        .device(device.id)
        .map(|d| d.color)
    else {
        return false;
    };
    // `GLOB.pipe_color_name[pipe_color]`: a color outside the paint list keeps the RPD's.
    let index = PIPE_PAINT_COLORS.iter().position(|(_, c)| *c == color);
    if let Some(mut rpd) = world.get_mut::<Rpd>(rpd_item) {
        if let Some(i) = index {
            rpd.color = i;
        }
        rpd.pipe_layers = layer_bit(device.layer.clamp(1, MAX_PIPE_LAYERS));
    }
    tell(world, client, Span::Notice, "Цвет и слой скопированы.");
    true
}

/// `REPROGRAM_MODE` on a placed or unplaced smart pipe. True when `target` is one.
fn reprogram(world: &mut World, client: Entity, rpd: &Rpd, target: Entity) -> bool {
    if let Some(mut fitting) = world.get_mut::<PipeFitting>(target) {
        if !is_under(&fitting.pipe_type, super::SMART_PIPE) {
            return false;
        }
        // An unplaced pipe has no connections to keep.
        fitting.init_dir = rpd.init_dir;
        return true;
    }
    let Some(device) = world.get::<AtmosDevice>(target).copied() else {
        return false;
    };
    let new = {
        let net = world.resource::<Pipenet>();
        let Some(d) = net.device(device.id) else {
            return false;
        };
        if !matches!(d.kind, DeviceKind::Pipe { smart: true, .. }) {
            return false;
        }
        reprogrammed(d.init_dirs, d.connections(net), rpd.init_dir)
    };
    match new {
        Ok(new) => {
            super::fitting::narrow_smart_pipe(world, device.id, new);
            let name = world
                .get::<DmName>(target)
                .map(|n| crate::vending::plain_name(&n.name).to_owned())
                .unwrap_or_default();
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы перепрограммируете {name}."),
            );
        }
        Err(why) => tell(world, client, Span::Warning, why),
    }
    true
}

/// A placed smart pipe's new `initialize_directions`: `p_init_dir` on its free directions.
pub fn reprogrammed(old: u8, connections: u8, p_init_dir: u8) -> Result<u8, &'static str> {
    if connections == ALL_CARDINALS {
        return Err("Свободных направлений нет!");
    }
    if old == p_init_dir {
        return Err("Уже настроена!");
    }
    let differences = (p_init_dir ^ old) & !connections;
    if differences == 0 {
        return Err("Уже настроена для своих направлений!");
    }
    let new = (old & !differences) | (p_init_dir & differences);
    if new & new.wrapping_sub(1) == 0 {
        return Err("Трубы с одним направлением нельзя!");
    }
    Ok(new)
}

/// `examine`.
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<(Span, String)>) {
    if world.get::<Rpd>(target).is_some() {
        lines.push((
            Span::Notice,
            "Правый клик по трубе переносит в RPD её цвет и слой.".to_owned(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_follow_single_and_multi_mode() {
        let mut rpd = Rpd::default();
        assert_eq!(rpd.pipe_layers, 0b100);
        apply_act(&mut rpd, RpdAct::Layers(0b1)).unwrap();
        assert_eq!(rpd.pipe_layers, 0b1);
        apply_act(&mut rpd, RpdAct::ToggleMulti).unwrap();
        apply_act(&mut rpd, RpdAct::Layers(0b1000)).unwrap();
        assert_eq!(rpd.pipe_layers, 0b1001);
        // The last layer of a set cannot be clicked away.
        apply_act(&mut rpd, RpdAct::Layers(0b1001)).unwrap();
        assert_eq!(rpd.pipe_layers, 0b1001);
        // Multi off keeps the top layer.
        apply_act(&mut rpd, RpdAct::ToggleMulti).unwrap();
        assert_eq!((rpd.pipe_layers, rpd.multi_layer), (0b1000, false));
        assert!(apply_act(&mut rpd, RpdAct::Layers(0b10_0000)).is_err());
        assert!(apply_act(&mut rpd, RpdAct::InitDir(1)).is_ok());
        assert_eq!(rpd.init_dir, 2 | 4 | 8);
    }

    #[test]
    fn reprogramming_keeps_live_connections() {
        // A pipe connected east-west allowing all four, set to east-west-north.
        assert_eq!(reprogrammed(15, 4 | 8, 1 | 4 | 8), Ok(1 | 4 | 8));
        // Asking to drop east, which is connected: nothing free changes.
        assert!(reprogrammed(15, 4 | 8, 1 | 2 | 8).is_err());
        // Dropping south while keeping the live east-west.
        assert_eq!(reprogrammed(15, 4 | 8, 1 | 8), Ok(1 | 4 | 8));
        assert_eq!(reprogrammed(4 | 8, 4 | 8, 1 | 4 | 8), Ok(1 | 4 | 8));
        assert!(reprogrammed(15, 15, 3).is_err());
        assert!(reprogrammed(4 | 8, 4 | 8, 4 | 8).is_err());
    }

    #[test]
    fn previews_follow_get_preview() {
        assert_eq!(previews(Some(FittingKind::Binary), "x").len(), 2);
        assert_eq!(previews(Some(FittingKind::Bendable), "x").len(), 6);
        let filter = previews(Some(FittingKind::Flippable), "x");
        assert_eq!(filter.len(), 8);
        assert!(
            filter
                .iter()
                .all(|p| p.flipped == matches!(p.dir, 5 | 6 | 9 | 10))
        );
        let pipe = previews(Some(FittingKind::Quaternary), "Pipe");
        assert_eq!((pipe[0].dir, pipe[0].dir_name.as_str()), (2, "Pipe"));
    }
}
