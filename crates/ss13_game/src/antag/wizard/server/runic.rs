//! The Scepter of Vendormancy and its Runic Vending Machines (server), after tg
//! `/obj/item/runic_vendor_scepter` (`equipment/artefact.dm`) and
//! `/obj/machinery/vending/runic_vendor` (`code/modules/vending/runic_vendor.dm`).
//!
//! - **Scepter** ([`RunicScepter`]): three charges. Used on a clear tile within 7 tiles (not its
//!   own) it channels 1 s and raises a vendor there; on a vendor (or a tile with one) it blows
//!   it up; right click throws it 4 tiles. Used in hand: 5 s to recharge fully. The Charge
//!   spell refills it too.
//! - **Vendor** ([`RunicVendor`]): appears with one of three wizard clothing sets
//!   (`pick(1,3)` in tg never gave the second, a typo not carried over), repels everyone within
//!   2 tiles, decays after 30 s; a screwdriver or the scepter makes it explode (light 2).
// shortcut: the scepter's busy messages are chat, not balloons; `is_blocked_turf` is only
// `dense_on_tile`.

use super::*;
use crate::click_handlers::ClickCtx;
use crate::tools::{Actor, ToolKind, do_after, now};

pub const SCEPTER: &str = "/obj/item/runic_vendor_scepter";
pub const RUNIC_VENDOR: &str = "/obj/machinery/vending/runic_vendor";
const MAX_CHARGES: u32 = 3;
const MAX_RANGE: u32 = 7;
/// `summoning_time`, the recharge's `do_after`, deciseconds.
const SUMMON_DS: f32 = 10.0;
const RECHARGE_DS: f32 = 50.0;
const DECAY_SECS: f64 = 30.0;
const PULSE_RANGE: u32 = 2;
const FOAM: &str = "/obj/item/toy/foam_runic_scepter";
const VENDOR_SETS: [[&str; 2]; 3] = [
    [
        "/obj/item/clothing/head/wizard",
        "/obj/item/clothing/suit/wizrobe",
    ],
    [
        "/obj/item/clothing/head/wizard/red",
        "/obj/item/clothing/suit/wizrobe/red",
    ],
    [
        "/obj/item/clothing/head/wizard/yellow",
        "/obj/item/clothing/suit/wizrobe/yellow",
    ],
];
const VENDOR_PRICE: u32 = 25;

/// `/obj/item/runic_vendor_scepter` `summon_vendor_charges`.
#[derive(Component, Clone, Copy, Debug)]
pub struct RunicScepter {
    pub charges: u32,
}

/// A runic vendor's decay.
#[derive(Component, Clone, Copy, Debug)]
pub struct RunicVendor {
    pub decay_at: f64,
}

pub(super) fn adopt(new: Query<(Entity, &DmItem), Added<DmItem>>, mut commands: Commands) {
    for (e, item) in &new {
        if item.path == SCEPTER {
            commands.entity(e).insert(RunicScepter {
                charges: MAX_CHARGES,
            });
        }
    }
}

fn vendor_on(world: &World, pos: TilePos) -> Option<Entity> {
    world
        .resource::<ss13_world::SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .find(|&e| world.get::<RunicVendor>(e).is_some())
}

/// `screwdriver_act` of a vendor; the scepter on the world.
pub(super) fn held_on_target(world: &mut World, c: &ClickCtx) -> bool {
    let Some(held) = c.held else {
        return false;
    };
    if world.get::<RunicVendor>(c.target).is_some()
        && crate::tools::tool_of(world, held).is_some_and(|(k, _)| k == ToolKind::Screwdriver)
    {
        explode(world, c.target);
        return true;
    }
    if world.get::<RunicScepter>(held).is_none() {
        return false;
    }
    use_scepter(
        world,
        c.client,
        c.mob,
        held,
        c.target,
        c.pos,
        c.modifiers.right,
    );
    true
}

/// The scepter on a tile past reach.
pub(super) fn ranged_clicks(
    world: &mut World,
    mut clicks: Local<MessageCursor<crate::click::RangedClick>>,
) {
    let clicks: Vec<crate::click::RangedClick> = clicks
        .read(world.resource::<Messages<crate::click::RangedClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(held) = world
            .get::<Inventory>(click.mob)
            .and_then(Inventory::active_item)
            .filter(|&i| world.get::<RunicScepter>(i).is_some())
        else {
            continue;
        };
        use_scepter(
            world,
            click.client,
            click.mob,
            held,
            click.target,
            click.target_pos,
            click.modifiers.right,
        );
    }
}

/// `ranged_interact_with_atom` and its secondary.
fn use_scepter(
    world: &mut World,
    client: Entity,
    mob: Entity,
    scepter: Entity,
    target: Entity,
    pos: TilePos,
    right: bool,
) {
    if world.get::<crate::tools::DoAfter>(mob).is_some() {
        chat_to(world, mob, Span::Warning, "busy!");
        return;
    }
    let vendor = if world.get::<RunicVendor>(target).is_some() {
        Some(target)
    } else {
        vendor_on(world, pos)
    };
    if right {
        if let Some(vendor) = vendor {
            push_vendor(world, mob, vendor, pos);
        }
        return;
    }
    if let Some(vendor) = vendor {
        explode(world, vendor);
        return;
    }
    let charges = world.get::<RunicScepter>(scepter).map_or(0, |s| s.charges);
    if charges == 0 {
        chat_to(world, mob, Span::Warning, "no charges!");
        return;
    }
    let Some(from) = crate::explosions::turf_of(world, mob) else {
        return;
    };
    if from.chebyshev(pos).is_none_or(|d| d > MAX_RANGE) {
        chat_to(world, mob, Span::Warning, "too far!");
        return;
    }
    if from == pos {
        chat_to(world, mob, Span::Warning, "too close!");
        return;
    }
    if !world
        .resource::<ss13_world::SpatialIndex>()
        .dense_on_tile(pos)
        .is_empty()
    {
        chat_to(world, mob, Span::Warning, "blocked!");
        return;
    }
    chat_to(world, mob, Span::Notice, "summoning...");
    do_after(
        world,
        Actor {
            client,
            mob,
            item: Some(scepter),
        },
        target,
        SUMMON_DS,
        0,
        crate::construction::Job::Wizard(WizardJob::SummonVendor(pos)),
    );
}

/// `throw_at` of the vendor, right click.
fn push_vendor(world: &mut World, mob: Entity, vendor: Entity, pos: TilePos) {
    let Some(from) = crate::explosions::turf_of(world, mob) else {
        return;
    };
    let (dx, dy) = ((pos.x - from.x).signum(), (pos.y - from.y).signum());
    let target = TilePos::new(pos.x + dx * 4, pos.y + dy * 4, pos.z);
    crate::throw::throw_at(world, vendor, target, 4, 20.0);
}

/// `attack_self`: the 5 s recharge.
pub(super) fn attack_self(world: &mut World, client: Entity, mob: Entity, item: Entity) -> bool {
    if world.get::<RunicScepter>(item).is_none() {
        return false;
    }
    chat_to(world, mob, Span::Notice, "recharging...");
    do_after(
        world,
        Actor {
            client,
            mob,
            item: Some(item),
        },
        mob,
        RECHARGE_DS,
        0,
        crate::construction::Job::Wizard(WizardJob::RechargeScepter(item)),
    );
    true
}

/// `COMSIG_ITEM_MAGICALLY_CHARGED`: true when `item` is a scepter (now full).
pub fn magic_recharge(world: &mut World, item: Entity) -> bool {
    match world.get_mut::<RunicScepter>(item) {
        Some(mut scepter) => {
            scepter.charges = MAX_CHARGES;
            true
        }
        None => false,
    }
}

/// The scepter's `do_after`s end.
pub(super) fn complete_job(world: &mut World, mob: Entity, scepter: Entity, job: WizardJob) {
    match job {
        WizardJob::SummonVendor(pos) => {
            let charges = world.get::<RunicScepter>(scepter).map_or(0, |s| s.charges);
            if charges == 0 {
                return;
            }
            summon_vendor(world, mob, pos);
            if let Some(mut s) = world.get_mut::<RunicScepter>(scepter) {
                s.charges -= 1;
            }
        }
        WizardJob::RechargeScepter(item) => {
            if magic_recharge(world, item) {
                chat_to(world, mob, Span::Notice, "fully charged");
            }
        }
        _ => {}
    }
}

/// `new /obj/machinery/vending/runic_vendor(tile)`.
pub fn summon_vendor(world: &mut World, mob: Entity, pos: TilePos) -> Option<Entity> {
    if !world
        .resource::<ss13_world::SpatialIndex>()
        .dense_on_tile(pos)
        .is_empty()
    {
        return None;
    }
    let e = crate::construction::spawn_obj(world, RUNIC_VENDOR, pos)?;
    world.flush();
    let user = crate::medical::name_of(world, mob);
    crate::medical::visible(
        world,
        mob,
        "You summon a runic vendor!",
        &format!("{user} summons a runic vendor!"),
        Span::Warning,
    );
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, "sound/items/weapons/resonator_fire.ogg", 50.0, true),
    );
    let t = now(world);
    let set = world.resource_mut::<crate::combat::CombatRng>().range(0, 2) as usize;
    let mut dm = world
        .get::<ss13_mapload::DmVending>(e)
        .cloned()
        .unwrap_or_default();
    dm.products = VENDOR_SETS[set]
        .iter()
        .chain(&["/obj/item/clothing/shoes/sandal/magic", FOAM])
        .map(|&path| ss13_mapload::VendProduct {
            path: path.to_owned(),
            amount: 1,
            price: VENDOR_PRICE,
            category: 0,
            age_restricted: false,
        })
        .collect();
    if world.contains_resource::<crate::vending::ProductTemplates>() {
        let vendor = world.resource_scope::<crate::vending::ProductTemplates, _>(|_, templates| {
            crate::vending::vendor(&dm, &templates.0)
        });
        world.entity_mut(e).insert(vendor);
        crate::vending::refresh_look(world, e);
    }
    world.entity_mut(e).insert((
        dm,
        RunicVendor {
            decay_at: t + DECAY_SECS,
        },
    ));
    pulse(world, e, pos);
    Some(e)
}

/// `runic_pulse`: everyone close is thrown away.
fn pulse(world: &mut World, vendor: Entity, pos: TilePos) {
    let mobs: Vec<(Entity, TilePos)> = world
        .resource::<ss13_world::SpatialIndex>()
        .in_radius(pos, PULSE_RANGE)
        .filter(|&e| e != vendor && world.get::<Health>(e).is_some())
        .filter_map(|e| world.get::<TilePos>(e).map(|&p| (e, p)))
        .collect();
    for (mob, at) in mobs {
        let (dx, dy) = ((at.x - pos.x).signum(), (at.y - pos.y).signum());
        if (dx, dy) == (0, 0) {
            continue;
        }
        chat_to(
            world,
            mob,
            Span::Danger,
            "The field repels you with tremendous force!",
        );
        crate::sound::playsound(
            world,
            crate::sound::PlaySound::new(pos, "sound/effects/gravhit.ogg", 50.0, true),
        );
        let target = TilePos::new(at.x + dx * 4, at.y + dy * 4, at.z);
        crate::throw::throw_at(world, mob, target, 4, 4.0);
    }
}

/// `runic_explosion`.
pub fn explode(world: &mut World, vendor: Entity) {
    if let Some(&pos) = world.get::<TilePos>(vendor) {
        crate::explosions::explosion(
            world,
            pos,
            crate::explosions::Explosion::new(-1, 0, 2).cause("runic vendor"),
        );
    }
    decay(world, vendor);
}

/// `decay` / `handle_deconstruct`.
fn decay(world: &mut World, vendor: Entity) {
    let Some(&pos) = world.get::<TilePos>(vendor) else {
        return;
    };
    let name = crate::security::name_of(world, vendor);
    for m in crate::pull::visible_message(
        world,
        pos,
        ChatChannel::Warning,
        (Span::Warning, format!("{name} flickers and disappears!")),
        &[],
    ) {
        world.write_message(m);
    }
    crate::sound::playsound(
        world,
        crate::sound::PlaySound::new(pos, "sound/items/weapons/resonator_blast.ogg", 25.0, true),
    );
    crate::vending::on_deconstruction(world, vendor, pos, false);
    world.despawn(vendor);
}

/// The vendors' 30 s.
pub(super) fn tick(world: &mut World) {
    let t = now(world);
    let due: Vec<Entity> = world
        .query::<(Entity, &RunicVendor)>()
        .iter(world)
        .filter(|(_, v)| t >= v.decay_at)
        .map(|(e, _)| e)
        .collect();
    for e in due {
        decay(world, e);
    }
}
