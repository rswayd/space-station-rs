//! Emergency shield projectors and shield wall generators (server): tg
//! `code/game/machinery/shieldgen.dm` (`/obj/machinery/shieldgen`, `/obj/structure/
//! emergency_shield`, `/obj/machinery/power/shieldwallgen`, `/obj/machinery/shieldwall`).
//!
//! - Projector ([`ShieldGen`]): a wrench anchors it (not in space, not while locked; unanchored
//!   it shuts off), a screwdriver opens its panel, an ID with engineering access locks its
//!   controls, an emag shorts the lock for good. An empty hand on an anchored, unlocked one with
//!   its panel shut switches it: on, every space tile within 4 gets an emergency shield (dense,
//!   airtight, 200 integrity; a broken projector makes each at 33%); off, they are gone. A broken
//!   active projector drops one of them at 2.5%/s; 1 cable with the panel open (3 s) repairs it.
//!   An EMP deletes a shield (heavy) or deals it 50 (light).
//! - Wall generator ([`ShieldWallGen`]): anchored, it is a node on its tile's cable. An ID with
//!   its access (teleporter; xenobiology for the xenobiology one) locks it, an emag shorts the
//!   lock. Unlocked, wired, panel shut, an empty hand turns it on: every machine tick it draws
//!   150 W from its net (`processing_early`) or shuts down, and while it has no walls yet it looks
//!   up to 8 tiles along each cardinal for another active generator and fills the tiles between
//!   with shield walls (gibbing whatever lives there). A wall drains 10 J per tick from its two
//!   generators and goes away when either is off or gone. Off, its walls go. A wrench (instant,
//!   not while on) anchors it, a screwdriver (unlocked) opens the panel, a crowbar with the panel
//!   open takes it apart (`crate::machine_frame` family `ShieldWallGen`).
//!
//! Cost: projectors and generators act on clicks; the per-machine-tick pass is over the
//! generators, the walls and the broken active projectors only.
// shortcut: no shield generator wires (`/datum/wires/shieldwallgen`), no light while on, no
// singularity or containment-field traits, walls are dense to every projectile and thrown item
// (tg lets 10% / `PASSGLASS` 20% through) and take no damage (tg drains the generators by it),
// a map's `active` projector does not come up on load, no shield attack sounds or ion impacts.

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_mapload::fidelity::{IconIndex, is_under};
use ss13_mapload::{DmAtom, DmItem, DmObj, DmPower};
use ss13_shared::appearance::Overlay;
use ss13_shared::chat::{ChatChannel, Span};
use ss13_shared::{Appearance, Dir, EMISSIVE_PLANE, Facing, TilePos};
use ss13_world::{Density, SpatialIndex};

use super::{Grid, NodeKind, Powernet};
use crate::construction::{Job, PanelOpen};
use crate::health::Health;
use crate::obj_damage::{Anchored, Integrity, set_anchored, set_icon_state};
use crate::tools::{Actor, DoAfter, ToolKind, tell};

pub const SHIELDGEN: &str = "/obj/machinery/shieldgen";
pub const EMERGENCY_SHIELD: &str = "/obj/structure/emergency_shield";
pub const SHIELD_WALL_GEN: &str = "/obj/machinery/power/shieldwallgen";
const XENOBIO_GEN: &str = "/obj/machinery/power/shieldwallgen/xenobiologyaccess";
const UNLOCKED_GEN: &str = "/obj/machinery/power/shieldwallgen/unlocked";
pub const SHIELD_WALL: &str = "/obj/machinery/shieldwall";
use ss13_mapload::circuit::CABLE_COIL;
const SPARKS: &str = "sparks";

/// `shield_range` of the projector, of the wall generator.
pub const PROJECTOR_RANGE: i32 = 4;
pub const WALL_RANGE: u32 = 8;
/// `active_power_usage` of the wall generator, watts.
pub const WALL_GEN_POWER: f64 = 150.0;
/// `drain_power(10)` of a wall each tick, half from each generator.
pub const WALL_DRAIN: f64 = 10.0;
/// `do_after(user, 3 SECONDS)` of the repair, deciseconds.
const REPAIR_DS: f32 = 30.0;
/// `SPT_PROB(2.5, seconds_per_tick)` of a broken projector losing a shield.
const BROKEN_DROP_PERCENT_PER_SEC: f64 = 2.5;

/// tg `/obj/machinery/shieldgen`.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct ShieldGen {
    pub active: bool,
    pub locked: bool,
    pub emagged: bool,
    pub shields: Vec<Entity>,
}

/// An emergency shield and the projector that made it (none for mapped ones).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct EmergencyShield {
    pub projector: Option<Entity>,
}

/// tg `active`: `FALSE`, `ACTIVE_SETUPFIELDS`, `ACTIVE_HASFIELDS`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WallActive {
    #[default]
    Off,
    SetupFields,
    HasFields,
}

/// tg `/obj/machinery/power/shieldwallgen`.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct ShieldWallGen {
    pub active: WallActive,
    pub locked: bool,
    pub emagged: bool,
    /// `req_access`.
    pub access: Option<&'static str>,
    pub layer: u8,
    /// Could not draw its power this machine tick (`process_early`): shuts down after it.
    pub starved: bool,
}

impl ShieldWallGen {
    pub fn of_path(path: &str, layer: u8) -> Self {
        let unlocked = is_under(path, UNLOCKED_GEN);
        Self {
            active: WallActive::Off,
            locked: !unlocked,
            emagged: false,
            access: if unlocked {
                None
            } else if is_under(path, XENOBIO_GEN) {
                Some("xenobiology")
            } else {
                Some("teleporter")
            },
            layer,
            starved: false,
        }
    }

    pub fn on(&self) -> bool {
        self.active != WallActive::Off
    }

    /// `process_early`'s draw: `surplus() >= active_power_usage` takes it, else starved.
    pub fn draw_power(&mut self, net: Option<&mut Powernet>, dt: f64) {
        if !self.on() {
            return;
        }
        let usage = WALL_GEN_POWER * dt;
        match net {
            Some(net) if net.surplus() >= usage => net.load += usage,
            _ => self.starved = true,
        }
    }
}

/// tg `/obj/machinery/shieldwall` between two generators.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ShieldWall {
    pub gens: [Entity; 2],
}

/// A timed projector step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShieldJob {
    /// Cable on a broken projector with its panel open.
    Repair,
}

pub struct ShieldsPlugin;

impl Plugin for ShieldsPlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        app.add_click_handler(ClickPhase::EmptyHand, 1970, |w, c| {
            interact(w, c.client, c.mob, c.target)
        });
        app.add_click_handler(ClickPhase::HeldOnTarget, 2870, |w, c| {
            c.held.is_some_and(|h| {
                item_interaction(
                    w,
                    Actor {
                        client: c.client,
                        mob: c.mob,
                        item: Some(h),
                    },
                    c.target,
                )
            })
        });
        app.add_systems(Startup, init_shields.after(super::init_power))
            .add_systems(
                Update,
                (
                    machine_tick.after(super::PowerSystems),
                    emp_shields.in_set(crate::emp::EmpSystems::Effects),
                ),
            );
    }
}

fn init_shields(world: &mut World) {
    let mut q = world.query::<(Entity, &DmObj, Option<&DmPower>)>();
    let found: Vec<(Entity, String, u8)> = q
        .iter(world)
        .filter(|(_, o, _)| {
            is_under(&o.path, SHIELDGEN)
                || is_under(&o.path, SHIELD_WALL_GEN)
                || is_under(&o.path, EMERGENCY_SHIELD)
        })
        .map(|(e, o, p)| (e, o.path.clone(), p.map_or(2, super::layer)))
        .collect();
    for (e, path, layer) in found {
        if is_under(&path, SHIELDGEN) {
            world.entity_mut(e).insert(ShieldGen::default());
        } else if is_under(&path, EMERGENCY_SHIELD) {
            world
                .entity_mut(e)
                .insert(EmergencyShield { projector: None });
        } else {
            install_wall_gen(world, e, &path, layer);
        }
    }
}

fn install_wall_gen(world: &mut World, e: Entity, path: &str, layer: u8) {
    world
        .entity_mut(e)
        .insert(ShieldWallGen::of_path(path, layer));
    // `Initialize`: `if(anchored) connect_to_network()`.
    if anchored(world, e)
        && let Some(&pos) = world.get::<TilePos>(e)
    {
        world
            .resource_mut::<Grid>()
            .add_node(e, pos, layer, NodeKind::Machine);
    }
    wall_gen_look(world, e);
}

/// A wall generator built from its board (`crate::machine_frame::setup_built`).
pub fn setup_wall_gen(world: &mut World, e: Entity) {
    let Some(dm) = world.get::<DmPower>(e).cloned() else {
        return;
    };
    install_wall_gen(world, e, &dm.path, super::layer(&dm));
}

/// `Destroy` of a projector or a wall generator taken apart or destroyed (from
/// `crate::obj_damage::deconstruct`): its shields or walls go, a generator leaves the net.
pub fn on_destroy(world: &mut World, e: Entity) {
    if world.get::<ShieldGen>(e).is_some() {
        shields_down(world, e);
    } else if world.get::<ShieldWallGen>(e).is_some() {
        drop_walls_of(world, e);
        world.resource_mut::<Grid>().remove_node(e);
    }
}

/// `crowbar_act` of an active wall generator: blocked. True when it is.
pub fn crowbar_blocked(world: &World, e: Entity) -> bool {
    world.get::<ShieldWallGen>(e).is_some_and(ShieldWallGen::on)
}

fn anchored(world: &World, e: Entity) -> bool {
    world.get::<Anchored>(e).is_some_and(|a| a.0)
}

fn panel_open(world: &World, e: Entity) -> bool {
    world.get::<PanelOpen>(e).is_some_and(|p| p.0)
}

fn broken(world: &World, e: Entity) -> bool {
    world.get::<Integrity>(e).is_some_and(|i| i.broken)
}

fn is_space(world: &World, pos: TilePos) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .filter_map(|&e| world.get::<ss13_atmos::TurfAir>(e))
        .any(|a| a.kind == ss13_atmos::TileKind::Space)
}

fn say_around(world: &mut World, e: Entity, mob: Entity, others: String, own: String) {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    crate::security::show(
        world,
        pos,
        7,
        ChatChannel::Local,
        (Span::Notice, others),
        &[(mob, Span::Notice, own)],
    );
}

/// `update_icon_state` of the projector: `shield<on|off>[br]`.
fn projector_look(world: &mut World, e: Entity) {
    let Some(active) = world.get::<ShieldGen>(e).map(|g| g.active) else {
        return;
    };
    let state = format!(
        "shield{}{}",
        if active { "on" } else { "off" },
        if broken(world, e) { "br" } else { "" }
    );
    set_icon_state(world, e, &state);
}

/// `update_icon_state`, `update_overlays` of the wall generator.
fn wall_gen_look(world: &mut World, e: Entity) {
    let Some(on) = world.get::<ShieldWallGen>(e).map(ShieldWallGen::on) else {
        return;
    };
    set_icon_state(
        world,
        e,
        if on {
            "shield_wall_gen_on"
        } else {
            "shield_wall_gen"
        },
    );
    let wires = panel_open(world, e)
        .then(|| {
            let icon = world.get::<DmAtom>(e)?.icon.clone();
            world
                .get_resource::<IconIndex>()?
                .get(&icon, "shieldgen_wires")
        })
        .flatten();
    let overlays: Vec<Overlay> = wires.map(Overlay::new).into_iter().collect();
    if let Some(mut a) = world.get_mut::<Appearance>(e)
        && a.overlays != overlays
    {
        a.overlays = overlays;
    }
}

// ---------------------------------------------------------------------------------------------
// The projector

/// `shields_up`: a shield on every space tile in range without one.
pub fn shields_up(world: &mut World, e: Entity) {
    let Some(&center) = world.get::<TilePos>(e) else {
        return;
    };
    let is_broken = broken(world, e);
    let mut made = Vec::new();
    for dy in -PROJECTOR_RANGE..=PROJECTOR_RANGE {
        for dx in -PROJECTOR_RANGE..=PROJECTOR_RANGE {
            let pos = TilePos::new(center.x + dx, center.y + dy, center.z);
            let index = world.resource::<SpatialIndex>();
            if !index.in_bounds(pos) || !is_space(world, pos) {
                continue;
            }
            let has_shield = index
                .on_tile(pos)
                .iter()
                .any(|&s| world.get::<EmergencyShield>(s).is_some());
            if has_shield {
                continue;
            }
            if is_broken
                && !world
                    .resource_mut::<crate::combat::CombatRng>()
                    .chance(33.0)
            {
                continue;
            }
            if let Some(shield) = spawn_shield(world, pos, Some(e)) {
                made.push(shield);
            }
        }
    }
    if let Some(mut g) = world.get_mut::<ShieldGen>(e) {
        g.active = true;
        g.shields.extend(made);
    }
    projector_look(world, e);
}

fn spawn_shield(world: &mut World, pos: TilePos, projector: Option<Entity>) -> Option<Entity> {
    let shield = crate::construction::spawn_obj(world, EMERGENCY_SHIELD, pos)?;
    // `setDir(pick(GLOB.cardinals))`.
    let dir = Dir::CARDINALS[world.resource_mut::<crate::combat::CombatRng>().range(0, 3) as usize];
    world.entity_mut(shield).insert((
        EmergencyShield { projector },
        Density,
        Facing(dir),
        Anchored(true),
        ss13_atmos::AtmosBlocker(ss13_atmos::AtmosPass::No),
    ));
    Some(shield)
}

/// `shields_down`: inactive, its shields gone.
pub fn shields_down(world: &mut World, e: Entity) {
    let shields = world
        .get_mut::<ShieldGen>(e)
        .map(|mut g| {
            g.active = false;
            std::mem::take(&mut g.shields)
        })
        .unwrap_or_default();
    for s in shields {
        remove_shield(world, s);
    }
    projector_look(world, e);
}

fn remove_shield(world: &mut World, shield: Entity) {
    let Some(&pos) = world.get::<TilePos>(shield) else {
        return;
    };
    if world.get::<EmergencyShield>(shield).is_none() {
        return;
    }
    world.despawn(shield);
    crate::construction::tile_changed(world, pos);
}

fn card_access(world: &World, card: Entity, access: Option<&str>) -> bool {
    let Some(access) = access else {
        return true;
    };
    world
        .get::<crate::inventory::IdCard>(card)
        .is_some_and(|c| c.access.iter().any(|a| a == access))
}

/// Empty hand: `interact` of a projector or a wall generator. True when handled.
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    if let Some(g) = world.get::<ShieldGen>(target).cloned() {
        if g.locked && !crate::silicon::interact::has_silicon_access(world, mob) {
            tell(
                world,
                client,
                Span::Warning,
                "Машина заблокирована, вы не можете ею воспользоваться!",
            );
            return true;
        }
        if panel_open(world, target) {
            tell(
                world,
                client,
                Span::Warning,
                "Перед работой с машиной нужно закрыть панель!",
            );
            return true;
        }
        let name = crate::security::name_of(world, target);
        let user = crate::security::name_of(world, mob);
        if g.active {
            say_around(
                world,
                target,
                mob,
                format!("{user} выключает {name}."),
                format!("Вы выключаете {name}."),
            );
            shields_down(world, target);
        } else if anchored(world, target) {
            say_around(
                world,
                target,
                mob,
                format!("{user} включает {name}."),
                format!("Вы включаете {name}."),
            );
            shields_up(world, target);
        } else {
            tell(
                world,
                client,
                Span::Warning,
                "Сначала устройство нужно закрепить на полу!",
            );
        }
        return true;
    }
    let Some(g) = world.get::<ShieldWallGen>(target).cloned() else {
        return false;
    };
    let refusal = if !anchored(world, target) {
        Some("не закреплён!")
    } else if g.locked && !crate::silicon::interact::has_silicon_access(world, mob) {
        Some("заблокирован!")
    } else if world.resource::<Grid>().node_net(target).is_none() {
        Some("нужно питание от провода!")
    } else if panel_open(world, target) {
        Some("панель открыта!")
    } else {
        None
    };
    if let Some(why) = refusal {
        tell(world, client, Span::Warning, why);
        return true;
    }
    let name = crate::security::name_of(world, target);
    let user = crate::security::name_of(world, mob);
    if g.on() {
        say_around(
            world,
            target,
            mob,
            format!("{user} выключает {name}."),
            format!("Вы выключаете {name}."),
        );
        deactivate(world, target);
    } else {
        say_around(
            world,
            target,
            mob,
            format!("{user} включает {name}."),
            format!("Вы включаете {name}."),
        );
        if let Some(mut g) = world.get_mut::<ShieldWallGen>(target) {
            g.active = WallActive::SetupFields;
            g.starved = false;
        }
        wall_gen_look(world, target);
    }
    info!(target: "engine", "shield {target:?} toggled by {mob:?}");
    true
}

/// Tools, IDs and cable on a projector or a wall generator. True when handled.
pub fn item_interaction(world: &mut World, actor: Actor, target: Entity) -> bool {
    let Some(held) = actor.item else {
        return false;
    };
    let projector = world.get::<ShieldGen>(target).is_some();
    let wall_gen = world.get::<ShieldWallGen>(target).is_some();
    if !projector && !wall_gen {
        return false;
    }
    let client = actor.client;
    if projector
        && world
            .get::<DmItem>(held)
            .is_some_and(|i| is_under(&i.path, CABLE_COIL))
        && broken(world, target)
        && panel_open(world, target)
    {
        tell(
            world,
            client,
            Span::Notice,
            "Вы начинаете менять проводку...",
        );
        crate::tools::do_after(
            world,
            actor,
            target,
            REPAIR_DS,
            1,
            Job::Shield(ShieldJob::Repair),
        );
        return true;
    }
    if let Some(card) = crate::inventory::get_id(world, held) {
        return id_act(world, client, card, target);
    }
    let Some((tool, _)) = crate::tools::tool_of(world, held) else {
        return false;
    };
    match tool {
        ToolKind::Screwdriver => screwdriver(world, client, target),
        ToolKind::Wrench => wrench(world, client, target),
        _ => false,
    }
}

/// `GetID()`: the lock flips with access, never once emagged.
fn id_act(world: &mut World, client: Entity, card: Entity, target: Entity) -> bool {
    let (emagged, access) = if let Some(g) = world.get::<ShieldGen>(target) {
        (g.emagged, Some("engineering"))
    } else if let Some(g) = world.get::<ShieldWallGen>(target) {
        (g.emagged, g.access)
    } else {
        return false;
    };
    if emagged {
        tell(
            world,
            client,
            Span::Danger,
            "Ошибка, контроллер доступа повреждён!",
        );
        return true;
    }
    if !card_access(world, card, access) {
        tell(world, client, Span::Danger, "Доступ запрещён.");
        return true;
    }
    let locked = if let Some(mut g) = world.get_mut::<ShieldGen>(target) {
        g.locked = !g.locked;
        g.locked
    } else if let Some(mut g) = world.get_mut::<ShieldWallGen>(target) {
        g.locked = !g.locked;
        g.locked
    } else {
        return true;
    };
    let text = if locked {
        "Вы блокируете управление."
    } else {
        "Вы разблокируете управление."
    };
    tell(world, client, Span::Notice, text);
    true
}

fn screwdriver(world: &mut World, client: Entity, target: Entity) -> bool {
    let open = panel_open(world, target);
    // `shieldwallgen/screwdriver_act`: `if(!panel_open && locked)`.
    if !open && world.get::<ShieldWallGen>(target).is_some_and(|g| g.locked) {
        tell(world, client, Span::Warning, "сначала разблокируйте!");
        return true;
    }
    world.entity_mut(target).insert(PanelOpen(!open));
    let text = if open {
        "Вы закрываете панель."
    } else {
        "Вы открываете панель и обнажаете проводку."
    };
    tell(world, client, Span::Notice, text);
    wall_gen_look(world, target);
    true
}

fn wrench(world: &mut World, client: Entity, target: Entity) -> bool {
    let Some(&pos) = world.get::<TilePos>(target) else {
        return true;
    };
    let was = anchored(world, target);
    if let Some(g) = world.get::<ShieldGen>(target).cloned() {
        if g.locked {
            tell(
                world,
                client,
                Span::Warning,
                "Болты закрыты! Разблокировка сняла бы крышки.",
            );
            return true;
        }
        if !was && is_space(world, pos) {
            return true;
        }
        if was && g.active {
            let name = crate::security::name_of(world, target);
            tell(world, client, Span::Notice, format!("{name} отключается!"));
            shields_down(world, target);
        }
        set_anchored(world, target, !was);
        tell(
            world,
            client,
            Span::Notice,
            if was {
                "откреплено"
            } else {
                "закреплено"
            },
        );
        return true;
    }
    let Some(g) = world.get::<ShieldWallGen>(target).cloned() else {
        return false;
    };
    // `can_be_unfasten_wrench`.
    if g.on() {
        tell(
            world,
            client,
            Span::Warning,
            "Сначала выключите генератор щита!",
        );
        return true;
    }
    if !was && is_space(world, pos) {
        return true;
    }
    set_anchored(world, target, !was);
    {
        let mut grid = world.resource_mut::<Grid>();
        if was {
            grid.remove_node(target);
        } else {
            grid.add_node(target, pos, g.layer, NodeKind::Machine);
        }
    }
    let text = if was {
        "Вы откручиваете генератор от пола."
    } else {
        "Вы прикручиваете генератор к полу."
    };
    tell(world, client, Span::Notice, text);
    true
}

/// The end of a [`ShieldJob`].
pub fn complete(
    world: &mut World,
    action: &DoAfter,
    job: ShieldJob,
    consume: impl FnOnce(&mut World) -> bool,
) {
    let target = action.target;
    match job {
        ShieldJob::Repair => {
            if world.get::<ShieldGen>(target).is_none()
                || !broken(world, target)
                || !panel_open(world, target)
                || !consume(world)
            {
                return;
            }
            let max = world.get::<DmObj>(target).map_or(0.0, |o| o.max_integrity);
            world.entity_mut(target).insert(Integrity {
                current: max,
                broken: false,
            });
            let name = crate::security::name_of(world, target);
            tell(
                world,
                action.client,
                Span::Notice,
                format!("Вы чините {name}."),
            );
            projector_look(world, target);
        }
    }
}

/// Emag: the lock shorted for good. True when it took (from `crate::emag`).
pub fn emag_act(world: &mut World, client: Entity, target: Entity) -> bool {
    let already = if let Some(mut g) = world.get_mut::<ShieldGen>(target) {
        let was = g.emagged;
        g.emagged = true;
        g.locked = false;
        was
    } else if let Some(mut g) = world.get_mut::<ShieldWallGen>(target) {
        let was = g.emagged;
        g.emagged = true;
        g.locked = false;
        was
    } else {
        return false;
    };
    if already {
        tell(
            world,
            client,
            Span::Warning,
            "Контроллер доступа повреждён!",
        );
        return false;
    }
    if let Some(&pos) = world.get::<TilePos>(target) {
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, SPARKS, 100.0, true)
                .extrarange(crate::sound::SHORT_RANGE_SOUND_EXTRARANGE),
        );
    }
    tell(world, client, Span::Notice, "контроллер доступа закорочен");
    true
}

/// The families with an `emag_act` here.
pub fn takes_emag(world: &World, target: Entity) -> bool {
    world.get::<ShieldGen>(target).is_some() || world.get::<ShieldWallGen>(target).is_some()
}

/// `emergency_shield/emp_act`: heavy deletes it, light deals 50.
fn emp_shields(world: &mut World, mut cursor: Local<MessageCursor<crate::emp::EmpAct>>) {
    for ev in crate::emp::emp_acts(world, &mut cursor) {
        if !ev.hits_self() || world.get::<EmergencyShield>(ev.target).is_none() {
            continue;
        }
        if ev.severity <= 1 {
            forget_shield(world, ev.target);
            remove_shield(world, ev.target);
        } else {
            crate::obj_damage::take_damage(
                world,
                ev.target,
                50.0,
                crate::health::DamageType::Brute,
            );
        }
    }
}

fn forget_shield(world: &mut World, shield: Entity) {
    if let Some(p) = world
        .get::<EmergencyShield>(shield)
        .and_then(|s| s.projector)
        && let Some(mut g) = world.get_mut::<ShieldGen>(p)
    {
        g.shields.retain(|&s| s != shield);
    }
}

// ---------------------------------------------------------------------------------------------
// The wall generator

fn wall_gen_on(world: &World, pos: TilePos) -> Option<Entity> {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<ShieldWallGen>(e).is_some())
}

/// `setup_field(direction)`: walls between this generator and the next active one along `dir`.
fn setup_field(world: &mut World, gen_e: Entity, dir: Dir) -> bool {
    let Some(&start) = world.get::<TilePos>(gen_e) else {
        return false;
    };
    let mut cur = start;
    let mut other = None;
    let mut steps = 0u32;
    for _ in 0..WALL_RANGE {
        cur = cur.step(dir);
        if let Some(g) = wall_gen_on(world, cur) {
            if !world.get::<ShieldWallGen>(g).is_some_and(ShieldWallGen::on) {
                return false;
            }
            cleanup_field(world, g, dir.reverse());
            other = Some(g);
            break;
        }
        steps += 1;
    }
    let Some(other) = other.filter(|_| steps > 0) else {
        return false;
    };
    let mut cur = start;
    for _ in 0..steps {
        cur = cur.step(dir);
        spawn_wall(world, cur, [gen_e, other], dir);
    }
    true
}

/// `new /obj/machinery/shieldwall(T, src, G)`: whatever lives there is gibbed.
fn spawn_wall(world: &mut World, pos: TilePos, gens: [Entity; 2], dir: Dir) {
    let wall = crate::construction::spawn_obj(world, SHIELD_WALL, pos)
        .unwrap_or_else(|| world.spawn((Name::new("shield wall"), pos)).id());
    let emissive = world.get::<Appearance>(wall).map(|a| Overlay {
        plane: Some(EMISSIVE_PLANE),
        ..Overlay::new(a.icon_state)
    });
    world
        .entity_mut(wall)
        .insert((ShieldWall { gens }, Density, Facing(dir)));
    if let Some(overlay) = emissive
        && let Some(mut a) = world.get_mut::<Appearance>(wall)
    {
        a.overlays = vec![overlay];
    }
    let living: Vec<Entity> = world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&e| world.get::<Health>(e).is_some())
        .collect();
    for mob in living {
        let name = crate::security::name_of(world, mob);
        crate::security::show(
            world,
            pos,
            7,
            ChatChannel::Local,
            (
                Span::Danger,
                format!("Щитовая стена внезапно занимает место, где стоит {name}!"),
            ),
            &[],
        );
        info!(target: "engine", "{mob:?} gibbed by a shield wall at {pos:?}");
        crate::health::gib(world, mob);
    }
}

/// `cleanup_field(direction)`: this generator's walls along `dir`, up to an inactive generator.
fn cleanup_field(world: &mut World, gen_e: Entity, dir: Dir) {
    let Some(&start) = world.get::<TilePos>(gen_e) else {
        return;
    };
    let mut cur = start;
    for _ in 0..WALL_RANGE {
        cur = cur.step(dir);
        if let Some(g) = wall_gen_on(world, cur)
            && !world.get::<ShieldWallGen>(g).is_some_and(ShieldWallGen::on)
        {
            break;
        }
        let walls: Vec<Entity> = world
            .resource::<SpatialIndex>()
            .on_tile(cur)
            .iter()
            .copied()
            .filter(|&w| {
                world
                    .get::<ShieldWall>(w)
                    .is_some_and(|f| f.gens.contains(&gen_e))
            })
            .collect();
        for w in walls {
            remove_wall(world, w);
        }
    }
}

fn remove_wall(world: &mut World, wall: Entity) {
    let Some(&pos) = world.get::<TilePos>(wall) else {
        return;
    };
    world.despawn(wall);
    crate::construction::tile_changed(world, pos);
}

/// Every wall of `gen_e` (its `Destroy`).
fn drop_walls_of(world: &mut World, gen_e: Entity) {
    let mut q = world.query::<(Entity, &ShieldWall)>();
    let walls: Vec<Entity> = q
        .iter(world)
        .filter(|(_, w)| w.gens.contains(&gen_e))
        .map(|(e, _)| e)
        .collect();
    for w in walls {
        remove_wall(world, w);
    }
}

/// `deactivate`: off, its walls along every cardinal gone.
pub fn deactivate(world: &mut World, gen_e: Entity) {
    if let Some(mut g) = world.get_mut::<ShieldWallGen>(gen_e) {
        g.active = WallActive::Off;
        g.starved = false;
    }
    for dir in Dir::CARDINALS {
        cleanup_field(world, gen_e, dir);
    }
    wall_gen_look(world, gen_e);
}

/// Once per machine tick, after the power tick drew the generators' power: starved generators
/// shut down, generators setting up look for partners, walls check and drain their generators;
/// broken active projectors lose shields.
fn machine_tick(world: &mut World, mut last_tick: Local<u64>) {
    let ticks = world.resource::<super::MachineTicks>().0;
    if ticks == *last_tick {
        return;
    }
    let elapsed = ticks.saturating_sub(*last_tick).min(3);
    *last_tick = ticks;
    let mut gens = world.query::<(Entity, &ShieldWallGen)>();
    let gens: Vec<(Entity, WallActive, bool)> = gens
        .iter(world)
        .map(|(e, g)| (e, g.active, g.starved))
        .collect();
    for (e, active, starved) in gens {
        if starved {
            let name = crate::security::name_of(world, e);
            if let Some(&pos) = world.get::<TilePos>(e) {
                crate::security::show(
                    world,
                    pos,
                    7,
                    ChatChannel::Local,
                    (
                        Span::Danger,
                        format!("{name} отключается из-за нехватки энергии!"),
                    ),
                    &[],
                );
            }
            info!(target: "engine", "shield wall generator {e:?} deactivated due to lack of power");
            deactivate(world, e);
            continue;
        }
        if active == WallActive::SetupFields {
            let mut fields = 0;
            for dir in Dir::CARDINALS {
                if setup_field(world, e, dir) {
                    fields += 1;
                }
            }
            if fields > 0
                && let Some(mut g) = world.get_mut::<ShieldWallGen>(e)
            {
                g.active = WallActive::HasFields;
            }
        }
    }
    // `shieldwall/process`: gone with a generator off, else `drain_power(10)`.
    let mut walls = world.query::<(Entity, &ShieldWall)>();
    let walls: Vec<(Entity, [Entity; 2])> = walls.iter(world).map(|(e, w)| (e, w.gens)).collect();
    for (wall, pair) in walls {
        let live = pair
            .iter()
            .all(|&g| world.get::<ShieldWallGen>(g).is_some_and(ShieldWallGen::on));
        if !live {
            remove_wall(world, wall);
            continue;
        }
        let mut grid = world.resource_mut::<Grid>();
        for g in pair {
            let net = grid.node_net(g);
            if let Some(net) = net.and_then(|n| grid.net_mut(n)) {
                net.load += WALL_DRAIN * 0.5;
            }
        }
    }
    // `shieldgen/process`: a broken active projector loses a shield at 2.5%/s.
    let dt = super::MACHINE_TICK_SECS * elapsed as f64;
    let percent = (1.0 - (1.0 - BROKEN_DROP_PERCENT_PER_SEC / 100.0).powf(dt)) * 100.0;
    let mut projectors = world.query::<(Entity, &ShieldGen, &Integrity)>();
    let failing: Vec<(Entity, Vec<Entity>)> = projectors
        .iter(world)
        .filter(|(_, g, i)| g.active && i.broken && !g.shields.is_empty())
        .map(|(e, g, _)| (e, g.shields.clone()))
        .collect();
    for (p, shields) in failing {
        let pick = {
            let mut rng = world.resource_mut::<crate::combat::CombatRng>();
            if !rng.chance(percent as f32) {
                continue;
            }
            shields[rng.range(0, shields.len() as u32 - 1) as usize]
        };
        if let Some(mut g) = world.get_mut::<ShieldGen>(p) {
            g.shields.retain(|&s| s != pick);
        }
        remove_shield(world, pick);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wall_gen_kinds_and_power_draw() {
        let g = ShieldWallGen::of_path(XENOBIO_GEN, 2);
        assert_eq!((g.locked, g.access), (true, Some("xenobiology")));
        let g = ShieldWallGen::of_path("/obj/machinery/power/shieldwallgen/unlocked/anchored", 2);
        assert_eq!((g.locked, g.access), (false, None));
        let mut g = ShieldWallGen::of_path(SHIELD_WALL_GEN, 2);
        assert_eq!(g.access, Some("teleporter"));
        // Off: draws nothing.
        let mut net = Powernet::default();
        g.draw_power(Some(&mut net), 2.0);
        assert!(!g.starved);
        assert_eq!(net.load, 0.0);
        // On without a net or surplus: starved.
        g.active = WallActive::SetupFields;
        g.draw_power(None, 2.0);
        assert!(g.starved);
        g.starved = false;
        g.draw_power(Some(&mut net), 2.0);
        assert!(g.starved, "an empty net has no surplus");
    }
}
