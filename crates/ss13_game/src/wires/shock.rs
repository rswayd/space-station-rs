use super::*;

/// The area's power for a shock: tg `get_powernet_info_from_source(get_area(src))`, the APC's
/// cell charge and powernet. `None`: no APC, or one without a cell and a powernet.
pub(super) fn area_power_source(
    world: &World,
    holder: Entity,
) -> Option<(Entity, Option<f64>, bool)> {
    let pos = *world.get::<TilePos>(holder)?;
    let area = world.get_resource::<Areas>()?.area_at(pos)?;
    let apc = world.get::<AreaPower>(area)?.apc?;
    apc_source(world, apc)
}

pub(super) fn apc_source(world: &World, apc: Entity) -> Option<(Entity, Option<f64>, bool)> {
    let cell = world.get::<Apc>(apc)?.cell.map(|c| c.charge);
    let net = world
        .get_resource::<Grid>()
        .and_then(|g| g.node_net(apc))
        .is_some();
    (cell.is_some() || net).then_some((apc, cell, net))
}

/// `should_electrocute(get_area(holder))` of a carbon.
pub(super) fn should_electrocute(world: &World, holder: Entity, user: Entity) -> bool {
    world.get::<Inventory>(user).is_some() && area_power_source(world, holder).is_some()
}

/// The holder's `interactable(user)`.
pub fn interactable(world: &World, holder: Entity, user: Entity) -> bool {
    let Some(wires) = world.get::<Wires>(holder) else {
        return false;
    };
    let electrified = world.get::<Electrified>(holder).is_some();
    match wires.holder {
        WireHolder::Airlock => {
            let Some(door) = world.get::<Door>(holder) else {
                return false;
            };
            let powered = door.has_power(world.get::<Unpowered>(holder).is_none());
            if electrified && powered && should_electrocute(world, holder, user) {
                return false;
            }
            door.panel_open
        }
        WireHolder::Apc | WireHolder::Autolathe => panel_open(world, holder),
        WireHolder::Mulebot => crate::bots::mule::interactable(world, holder),
        WireHolder::ChemGrenade => {
            crate::assemblies::chem_grenade::wires_interactable(world, holder, user)
        }
        WireHolder::Machine(kind) => crate::machine_wires::interactable(world, holder, kind),
        WireHolder::Vending => {
            if electrified && should_electrocute(world, holder, user) {
                return false;
            }
            panel_open(world, holder)
        }
    }
}

/// tg `ELECTROCUTE_DAMAGE(energy)`.
pub(super) fn electrocute_damage(energy: f64, rng: &mut CombatRng) -> f32 {
    if energy < 1000.0 {
        return 0.0;
    }
    let base = (20.0 + (energy / JOULES_PER_DAMAGE).round()).clamp(20.0, 195.0) as f32;
    base + rng.range(0, 10) as f32 - 5.0
}

/// The holder's `shock(user, chance)`: per holder checks, then `/obj/shock` ->
/// `electrocute_mob`. True when a shock went through.
pub fn shock(world: &mut World, holder: Entity, mob: Entity, chance: f32) -> bool {
    let Some(kind) = world.get::<Wires>(holder).map(|w| w.holder) else {
        return false;
    };
    let now = world.resource::<Time>().elapsed_secs_f64();
    let unpowered = world.get::<Unpowered>(holder).is_some();
    let (source, siemens) = match kind {
        WireHolder::Airlock => {
            let Some(door) = world.get::<Door>(holder) else {
                return false;
            };
            // tg also gives the mob `TRAIT_AIRLOCK_SHOCKIMMUNE` for 1 s, which the 1 s
            // cooldown of the airlock already covers.
            if !door.has_power(!unpowered) || now < door.shock_ready_at {
                return false;
            }
            (area_power_source(world, holder), 1.0)
        }
        // `autolathe/shock`: `siemens_coeff` 0.7 too.
        WireHolder::Vending | WireHolder::Autolathe => {
            if unpowered {
                return false;
            }
            (area_power_source(world, holder), VENDING_SIEMENS)
        }
        // MULEbots and grenades never shock (no `shock` override, no electrified state).
        WireHolder::Mulebot | WireHolder::ChemGrenade => return false,
        // `/obj/machinery/shock`: not without power, the area's power.
        WireHolder::Machine(_) => {
            if !crate::machine_wires::can_shock(world, holder) {
                return false;
            }
            (area_power_source(world, holder), 1.0)
        }
        // `get_cell()`: the APC's own cell.
        WireHolder::Apc => (
            world
                .get::<Apc>(holder)
                .and_then(|a| a.cell)
                .map(|c| (holder, Some(c.charge), false)),
            1.0,
        ),
    };
    if world.get::<Stat>(mob).is_none() || !world.resource_mut::<CombatRng>().chance(chance) {
        return false;
    }
    // `do_sparks(5, TRUE, src)`.
    if let Some(pos) = crate::explosions::turf_of(world, holder) {
        crate::effects::particles::do_sparks(world, 5, true, pos);
    }
    let delivered = electrocute_mob(world, mob, source, holder, siemens);
    if delivered
        && kind == WireHolder::Airlock
        && let Some(mut door) = world.get_mut::<Door>(holder)
    {
        door.shock_ready_at = now + SHOCK_COOLDOWN_SECS;
    }
    delivered
}

/// tg `electrocute_mob(victim, node.powernet, source, 1, TRUE)`: from the net of a power node
/// (a SMES terminal).
pub(crate) fn electrocute_from_node(
    world: &mut World,
    victim: Entity,
    node: Entity,
    source: Entity,
) -> bool {
    electrocute_mob(world, victim, Some((node, None, true)), source, 1.0)
}

/// tg `electrocute_mob(user, get_area(source), source, siemens)`: from the APC of `source`'s
/// area.
pub(crate) fn electrocute_from_area(
    world: &mut World,
    victim: Entity,
    source: Entity,
    siemens: f32,
) -> bool {
    let power = area_power_source(world, source);
    electrocute_mob(world, victim, power, source, siemens)
}

/// tg `electrocute_mob(victim, power_source, source, siemens_coeff, dist_check = TRUE)`.
pub(super) fn electrocute_mob(
    world: &mut World,
    victim: Entity,
    power: Option<(Entity, Option<f64>, bool)>,
    source: Entity,
    siemens: f32,
) -> bool {
    let (Some(&vpos), Some(&spos)) = (world.get::<TilePos>(victim), world.get::<TilePos>(source))
    else {
        return false;
    };
    // `iscarbon` and `in_range`.
    if world.get::<Inventory>(victim).is_none()
        || vpos.z != spos.z
        || (vpos.x - spos.x).abs() > 1
        || (vpos.y - spos.y).abs() > 1
    {
        return false;
    }
    let gloves = world
        .get::<Inventory>(victim)
        .and_then(|i| i.gloves)
        .map(|g| {
            world
                .get::<DmClothing>(g)
                .map_or(1.0, |c| c.siemens_coefficient)
        });
    if gloves == Some(0.0) {
        return false;
    }
    let Some((apc, cell, net)) = power else {
        return false;
    };
    let net_id = net
        .then(|| world.resource::<Grid>().node_net(apc))
        .flatten();
    let avail = net_id
        .and_then(|n| world.resource::<Grid>().net(n))
        .map_or(0.0, |n| n.avail);
    let (net_damage, cell_damage) = {
        let mut rng = world.resource_mut::<CombatRng>();
        (
            net_id.map_or(0.0, |_| {
                electrocute_damage(avail / MACHINE_TICK_SECS, &mut rng)
            }),
            cell.map_or(0.0, |c| {
                electrocute_damage(c / (0.001 * STANDARD_CELL_CHARGE).max(1.0), &mut rng)
            }),
        )
    };
    let from_net = net_damage >= cell_damage;
    let damage = net_damage.max(cell_damage);
    let drained = electrocute_act(
        world,
        victim,
        damage,
        source,
        siemens * gloves.unwrap_or(1.0),
    );
    let energy = f64::from(drained) * 20.0;
    if energy > 0.0 {
        if from_net {
            if let Some(net) =
                net_id.and_then(|n| world.resource_mut::<Grid>().into_inner().net_mut(n))
            {
                net.delayedload += energy.min((net.newavail - net.delayedload).max(0.0));
            }
        } else if let Some(cell) = world
            .get_mut::<Apc>(apc)
            .and_then(|a| a.into_inner().cell.as_mut())
        {
            cell.take(energy, false);
        }
    }
    drained > 0.0
}

/// tg `/mob/living/carbon/human/electrocute_act` (the gloves are already in `siemens`): burn
/// over the body, stun (paralysis when already incapacitated), paralysis 2 s later. Returns the
/// damage dealt (0: blocked or too weak).
// shortcut: no `force_say`, heart restarts, shocks passed on through pulls or the human's
// skeleton flash (`electrocution_animation`).
pub(crate) fn electrocute_act(
    world: &mut World,
    mob: Entity,
    damage: f32,
    source: Entity,
    siemens: f32,
) -> f32 {
    // `TRAIT_SHOCKIMMUNE` (grounded nerves).
    if crate::medical::bioware::shock_immune(world, mob) {
        return 0.0;
    }
    // `physiology.siemens_coeff` of a human: its species' `siemens_coeff`.
    let damage = damage * siemens * crate::species::def_of(world, mob).siemens_coeff;
    if damage < 1.0 {
        return 0.0;
    }
    let Some(mut stat) = world.get::<Stat>(mob).copied() else {
        return 0.0;
    };
    if let Some(mut health) = world.get_mut::<Health>(mob) {
        health.adjust(DamageType::Burn, damage);
        let health = health.clone();
        if update_stat(&health, &mut stat).is_some() {
            *world.get_mut::<Stat>(mob).expect("checked above") = stat;
        }
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let incapacitated = world
        .get::<crate::status_effects::StatusEffects>(mob)
        .is_some_and(|e| e.incapacitated());
    let kind = if incapacitated {
        Incap::Paralyze
    } else {
        Incap::Stun
    };
    crate::status_effects::apply(world, mob, kind, SHOCK_STUN_SECS);
    // `do_jitter_animation(300)`, `adjust_jitter(jitter_time)`, `adjust_stutter(stutter_time)`
    // (20 s, 4 s by default).
    crate::animation::do_jitter_animation(world, mob, 300.0);
    {
        use crate::status_effects::{Timed, adjust_timed};
        adjust_timed(world, mob, Timed::Jitter, 20.0, None);
        adjust_timed(world, mob, Timed::Stutter, 4.0, None);
    }
    world.entity_mut(mob).insert(SecondaryShock {
        at: now + SECONDARY_SHOCK_DELAY,
        secs: SHOCK_STUN_SECS * SECONDARY_SHOCK_FACTOR,
    });
    if let Some(&pos) = world.get::<TilePos>(mob) {
        let (source_name, mob_name) = (name_of(world, source), name_of(world, mob));
        crate::security::show(
            world,
            pos,
            DEFAULT_MESSAGE_RANGE,
            ChatChannel::Combat,
            (
                Span::Danger,
                format!("{source_name} ударяет током {mob_name}!"),
            ),
            &[(
                mob,
                Span::Danger,
                "Вы чувствуете мощный разряд, проходящий через всё ваше тело!".to_owned(),
            )],
        );
    }
    damage
}

/// Bare-hand clicks and bumps on wire holders: an open panel opens the wires
/// (`attempt_wire_interaction`), an electrified holder shocks (`attack_hand`, `bumpopen`,
/// vending `interact`); a shock that does not go through lets the touch do what it does
/// otherwise.
pub(super) fn touches(
    world: &mut World,
    mut bumps: Local<MessageCursor<Bumped>>,
    mut clicks: Local<MessageCursor<ClickResolved>>,
) {
    let bumped: Vec<(Entity, Entity)> = bumps
        .read(world.resource::<Messages<Bumped>>())
        .map(|b| (b.entity, b.obstacle))
        .collect();
    for (mob, obstacle) in bumped {
        let door = world
            .get::<crate::doors::DoorFiller>(obstacle)
            .map_or(obstacle, |f| f.0);
        let powered = world.get::<Unpowered>(door).is_none();
        let Some(has_power) = world
            .get::<Door>(door)
            .filter(|_| world.get::<Electrified>(door).is_some())
            .map(|d| d.has_power(powered))
        else {
            continue;
        };
        // `bumpopen`: unpowered airlocks ignore bumps; non-carbons skip the shock.
        if !has_power || world.get::<Stat>(mob).is_none() {
            continue;
        }
        if world.get::<Inventory>(mob).is_some() && shock(world, door, mob, 100.0) {
            continue;
        }
        crate::doors::try_activate(world, door, mob, true);
    }
    let clicked: Vec<ClickResolved> = clicks
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in clicked {
        let Some(kind) = world.get::<Wires>(click.target).map(|w| w.holder) else {
            continue;
        };
        let empty = world
            .get::<Inventory>(click.mob)
            .is_some_and(|i| i.active_item().is_none());
        // The machines' open panels: `crate::machine_wires`' click handler.
        if !empty || matches!(kind, WireHolder::Machine(_)) {
            continue;
        }
        if panel_open(world, click.target) {
            interact(world, click.client, click.mob, click.target);
            continue;
        }
        if world.get::<Electrified>(click.target).is_none() {
            continue;
        }
        // A door's BandaStation knock (`crate::doors`) comes before the airlock's shock.
        if kind == WireHolder::Airlock && click.modifiers.right {
            continue;
        }
        match kind {
            WireHolder::Airlock => {
                if !shock(world, click.target, click.mob, 100.0) {
                    crate::doors::try_activate(world, click.target, click.mob, false);
                }
            }
            WireHolder::Vending => {
                if world.get::<Unpowered>(click.target).is_some()
                    || shock(world, click.target, click.mob, 100.0)
                {
                    continue;
                }
                let title = world
                    .get::<DmName>(click.target)
                    .map(|n| crate::vending::plain_name(&n.name).to_owned())
                    .unwrap_or_default();
                world.write_message(OpenUi::new::<crate::vending::VendingWindow>(
                    click.client,
                    click.mob,
                    click.target,
                    title,
                ));
            }
            WireHolder::Apc
            | WireHolder::Autolathe
            | WireHolder::Mulebot
            | WireHolder::ChemGrenade
            | WireHolder::Machine(_) => {}
        }
    }
}
