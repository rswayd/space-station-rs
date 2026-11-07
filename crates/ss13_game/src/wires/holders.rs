use super::*;

/// `/datum/wires/airlock/on_cut`.
pub(super) fn airlock_on_cut(
    world: &mut World,
    door: Entity,
    wire: &str,
    mend: bool,
    source: Option<Entity>,
) {
    let wires = world.get::<Wires>(door).cloned().expect("caller checked");
    match wire {
        WIRE_POWER1 | WIRE_POWER2 => {
            let both = !wires.is_cut(WIRE_POWER1) && !wires.is_cut(WIRE_POWER2);
            set_outage(world, door, |d| {
                if mend && both {
                    d.main_outage = 0.0;
                } else {
                    lose_main_power(d);
                }
            });
            shock_user(world, door, source, 50.0);
        }
        WIRE_BACKUP1 | WIRE_BACKUP2 => {
            let both = !wires.is_cut(WIRE_BACKUP1) && !wires.is_cut(WIRE_BACKUP2);
            set_outage(world, door, |d| {
                if mend && both {
                    d.backup_outage = 0.0;
                } else {
                    lose_backup_power(d);
                }
            });
            shock_user(world, door, source, 50.0);
        }
        WIRE_BOLTS if !mend => set_bolts(world, door, true),
        WIRE_AI => {
            if let Some(mut d) = world.get_mut::<Door>(door) {
                d.ai_control_disabled = ai_cut(d.ai_control_disabled, mend);
            }
        }
        WIRE_SHOCK => {
            if let Some(source) = source {
                info!(
                    "{} {} shocking for airlock {door}",
                    name_of(world, source),
                    if mend { "disabled" } else { "enabled" }
                );
            }
            if mend {
                set_electrified(world, door, None);
            } else {
                set_electrified(world, door, Some(Electrified::Permanent));
                shock_user(world, door, source, 100.0);
            }
        }
        WIRE_SAFETY => {
            if let Some(mut d) = world.get_mut::<Door>(door) {
                d.safe = mend;
            }
        }
        WIRE_TIMING => {
            with_door(world, door, |d, appearance, ctx| {
                d.autoclose = mend;
                if !mend {
                    // tg `autoclose()` checks `autoclose` when its timer fires.
                    d.close_at_clear();
                    return ((), false);
                }
                let closing = d.state == DoorState::Open && ctx.close(d, appearance, false);
                ((), closing)
            });
        }
        WIRE_FEEDBACK => {
            with_door(world, door, |d, appearance, _| {
                d.feedback = mend;
                d.apply_look(appearance);
                ((), false)
            });
        }
        WIRE_ZAP1 | WIRE_ZAP2 => shock_user(world, door, source, 50.0),
        WIRE_UNRESTRICTED_EXIT => {
            let side = if mend {
                [1u8, 2, 4, 8][world.resource_mut::<CombatRng>().range(0, 3) as usize]
            } else {
                0
            };
            if let Some(mut d) = world.get_mut::<Door>(door)
                && d.unres_latch
            {
                d.unres_sides = side;
            }
        }
        _ => {}
    }
}

/// `/datum/wires/airlock/on_pulse`.
pub(super) fn airlock_on_pulse(world: &mut World, door: Entity, wire: &str, user: Option<Entity>) {
    match wire {
        WIRE_POWER1 | WIRE_POWER2 => set_outage(world, door, lose_main_power),
        WIRE_BACKUP1 | WIRE_BACKUP2 => set_outage(world, door, lose_backup_power),
        WIRE_OPEN => {
            // Only without an ID check (cut ID scan) or on a door that needs no access.
            with_door(world, door, |d, appearance, ctx| {
                let free = !d.requires_id(ctx.env.wires) || d.allows(None);
                let moved = free
                    && match d.state {
                        DoorState::Closed => ctx.open(d, appearance, true),
                        DoorState::Open => ctx.close(d, appearance, true),
                        _ => false,
                    };
                ((), moved)
            });
        }
        WIRE_BOLTS => {
            let Some((locked, has_power)) = world.get::<Door>(door).map(|d| {
                (
                    d.locked,
                    d.has_power(world.get::<Unpowered>(door).is_none()),
                )
            }) else {
                return;
            };
            // Drops them always, raises them only with power.
            if !locked {
                set_bolts(world, door, true);
            } else if has_power {
                set_bolts(world, door, false);
            }
        }
        WIRE_IDSCAN => {
            with_door(world, door, |d, appearance, ctx| {
                let flash = d.has_power(ctx.env.powered) && d.state == DoorState::Closed;
                let active = flash && ctx.deny(d, appearance);
                ((), active)
            });
        }
        WIRE_AI => {
            if let Some(mut d) = world.get_mut::<Door>(door) {
                d.ai_control_disabled = ai_cut(d.ai_control_disabled, false);
            }
            schedule_reset(world, door, AI_RESET_SECS, WIRE_AI);
        }
        WIRE_SHOCK => {
            if world.get::<Electrified>(door).is_none() {
                set_electrified(
                    world,
                    door,
                    Some(Electrified::Timed(MACHINE_DEFAULT_ELECTRIFY_TIME)),
                );
            }
            shock_user(world, door, user, 100.0);
        }
        WIRE_SAFETY => {
            with_door(world, door, |d, appearance, ctx| {
                d.safe = !d.safe;
                let closing = d.state == DoorState::Open && ctx.close(d, appearance, false);
                ((), closing)
            });
        }
        WIRE_TIMING => {
            if let Some(mut d) = world.get_mut::<Door>(door) {
                d.normalspeed = !d.normalspeed;
            }
        }
        WIRE_FEEDBACK => {
            with_door(world, door, |d, appearance, _| {
                d.feedback = !d.feedback;
                d.apply_look(appearance);
                ((), false)
            });
        }
        WIRE_UNRESTRICTED_EXIT => {
            if let Some(mut d) = world.get_mut::<Door>(door)
                && d.unres_latch
            {
                d.unres_sides = reverse_dir(d.unres_sides);
            }
        }
        _ => {}
    }
}

/// tg APC `update()` after a wire changed its state: the area follows at once.
pub(crate) fn apc_update(world: &mut World, apc: Entity) {
    let Some(state) = world.get::<Apc>(apc).cloned() else {
        return;
    };
    let changed = world
        .get_mut::<AreaPower>(state.area)
        .is_some_and(|mut area| state.update(&mut area));
    if changed {
        world.write_message(AreaPowerChanged(state.area));
    }
}

pub(crate) fn apc_channel(
    world: &mut World,
    apc: Entity,
    channel: Channel,
    value: impl FnOnce(u8) -> u8,
) {
    if let Some(mut a) = world.get_mut::<Apc>(apc) {
        let i = channel as usize;
        a.channels[i] = value(a.channels[i]);
    }
    apc_update(world, apc);
}

pub(super) fn apc_wire_channel(wire: &str) -> Option<Channel> {
    match wire {
        WIRE_EQUIPMENT => Some(Channel::Equip),
        WIRE_LIGHT => Some(Channel::Light),
        WIRE_ENVIRONMENT => Some(Channel::Environ),
        _ => None,
    }
}

/// `/datum/wires/apc/on_cut`.
pub(super) fn apc_on_cut(
    world: &mut World,
    apc: Entity,
    wire: &str,
    mend: bool,
    source: Option<Entity>,
) {
    if let Some(channel) = apc_wire_channel(wire) {
        let value = if mend { CHANNEL_AUTO_ON } else { CHANNEL_OFF };
        apc_channel(world, apc, channel, |_| value);
        return;
    }
    let wires = world.get::<Wires>(apc).cloned().expect("caller checked");
    match wire {
        WIRE_POWER1 | WIRE_POWER2 => {
            let shorted = !(mend && !wires.is_cut(WIRE_POWER1) && !wires.is_cut(WIRE_POWER2));
            if let Some(mut a) = world.get_mut::<Apc>(apc) {
                a.shorted = shorted;
            }
            apc_update(world, apc);
            shock_user(world, apc, source, 50.0);
        }
        WIRE_INTERFACE => {
            if let Some(mut a) = world.get_mut::<Apc>(apc) {
                a.locked = !mend;
            }
        }
        WIRE_AI => {
            if let Some(mut a) = world.get_mut::<Apc>(apc) {
                a.aidisabled = !mend;
            }
        }
        _ => {}
    }
}

/// `/datum/wires/apc/on_pulse`.
pub(super) fn apc_on_pulse(world: &mut World, apc: Entity, wire: &str, _user: Option<Entity>) {
    if let Some(channel) = apc_wire_channel(wire) {
        apc_channel(world, apc, channel, |v| {
            if v > CHANNEL_OFF {
                CHANNEL_OFF
            } else {
                CHANNEL_AUTO_ON
            }
        });
        return;
    }
    match wire {
        WIRE_POWER1 | WIRE_POWER2 => {
            let was = world.get::<Apc>(apc).is_some_and(|a| a.shorted);
            if !was {
                if let Some(mut a) = world.get_mut::<Apc>(apc) {
                    a.shorted = true;
                }
                apc_update(world, apc);
                schedule_reset(world, apc, APC_SHORT_SECS, WIRE_POWER1);
            }
        }
        // tg schedules `reset(WIRE_INTERFACE)`, which no case of `reset()` matches: the APC
        // stays unlocked until something locks it.
        WIRE_INTERFACE => {
            if let Some(mut a) = world.get_mut::<Apc>(apc) {
                a.locked = false;
            }
        }
        WIRE_AI => {
            let was = world.get::<Apc>(apc).is_some_and(|a| a.aidisabled);
            if !was {
                if let Some(mut a) = world.get_mut::<Apc>(apc) {
                    a.aidisabled = true;
                }
                schedule_reset(world, apc, AI_RESET_SECS, WIRE_AI);
            }
        }
        _ => {}
    }
}

pub(super) fn hack_mut(world: &mut World, vendor: Entity, f: impl FnOnce(&mut VendorHack)) {
    let Some(mut hack) = world.get_mut::<VendorHack>(vendor) else {
        return;
    };
    f(&mut hack);
    let shoots = hack.shoot_inventory;
    let mut e = world.entity_mut(vendor);
    if shoots {
        e.insert(ShootsInventory);
    } else {
        e.remove::<ShootsInventory>();
    }
}

/// `/datum/wires/vending/on_cut`.
pub(super) fn vending_on_cut(
    world: &mut World,
    vendor: Entity,
    wire: &str,
    mend: bool,
    source: Option<Entity>,
) {
    match wire {
        WIRE_THROW => hack_mut(world, vendor, |h| h.shoot_inventory = !mend),
        WIRE_CONTRABAND => {
            if let Some(mut v) = world.get_mut::<Vendor>(vendor) {
                v.extended_inventory = false;
            }
        }
        WIRE_SHOCK => {
            if mend {
                set_electrified(world, vendor, None);
            } else {
                set_electrified(world, vendor, Some(Electrified::Permanent));
                shock_user(world, vendor, source, 100.0);
            }
        }
        WIRE_IDSCAN => hack_mut(world, vendor, |h| h.scan_id = mend),
        WIRE_SPEAKER => hack_mut(world, vendor, |h| h.shut_up = mend),
        _ => {}
    }
}

/// `/datum/wires/vending/on_pulse`.
pub(super) fn vending_on_pulse(
    world: &mut World,
    vendor: Entity,
    wire: &str,
    user: Option<Entity>,
) {
    match wire {
        WIRE_THROW => hack_mut(world, vendor, |h| h.shoot_inventory = !h.shoot_inventory),
        WIRE_CONTRABAND => {
            if let Some(mut v) = world.get_mut::<Vendor>(vendor) {
                v.extended_inventory = !v.extended_inventory;
            }
        }
        WIRE_SHOCK => {
            set_electrified(
                world,
                vendor,
                Some(Electrified::Timed(MACHINE_DEFAULT_ELECTRIFY_TIME)),
            );
            shock_user(world, vendor, user, 100.0);
        }
        WIRE_IDSCAN => hack_mut(world, vendor, |h| h.scan_id = !h.scan_id),
        // Speaker: the language cycle (module shortcut in `status`).
        _ => {}
    }
}

/// `/datum/wires/autolathe/on_cut`: the wire's state follows the cut; zap shocks.
pub(super) fn autolathe_on_cut(
    world: &mut World,
    lathe: Entity,
    wire: &str,
    mend: bool,
    source: Option<Entity>,
) {
    match wire {
        WIRE_HACK => crate::science::autolathe::set_hacked(world, lathe, !mend),
        WIRE_SHOCK | WIRE_DISABLE => {
            if let Some(mut a) = world.get_mut::<crate::science::autolathe::Autolathe>(lathe) {
                if wire == WIRE_SHOCK {
                    a.shocked = !mend;
                } else {
                    a.disabled = !mend;
                }
            }
        }
        WIRE_ZAP => shock_user(world, lathe, source, 50.0),
        _ => {}
    }
}

/// `/datum/wires/autolathe/on_pulse`: hack, shock and disable flip for 6 s.
pub(super) fn autolathe_on_pulse(world: &mut World, lathe: Entity, wire: &str) {
    let Some(mut a) = world.get_mut::<crate::science::autolathe::Autolathe>(lathe) else {
        return;
    };
    let wire: &'static str = match wire {
        WIRE_HACK => {
            a.hacked = !a.hacked;
            WIRE_HACK
        }
        WIRE_SHOCK => {
            a.shocked = !a.shocked;
            WIRE_SHOCK
        }
        WIRE_DISABLE => {
            a.disabled = !a.disabled;
            WIRE_DISABLE
        }
        _ => return,
    };
    schedule_reset(world, lathe, AUTOLATHE_RESET_SECS, wire);
}

/// `addtimer(CALLBACK(A, reset, wire), 6 SECONDS)`.
pub(super) const AUTOLATHE_RESET_SECS: f64 = 6.0;

/// tg airlock `reset_ai_wire`, APC and autolathe `reset(wire)`.
pub(super) fn reset(world: &mut World, holder: Entity, wire: &'static str) {
    let Some(wires) = world.get::<Wires>(holder).cloned() else {
        return;
    };
    if let WireHolder::Machine(kind) = wires.holder {
        crate::machine_wires::reset(world, holder, kind, wire);
        return;
    }
    if wires.holder == WireHolder::Autolathe {
        // `reset(wire)`: back to off unless the wire is cut.
        if !wires.is_cut(wire)
            && let Some(mut a) = world.get_mut::<crate::science::autolathe::Autolathe>(holder)
        {
            match wire {
                WIRE_HACK => a.hacked = false,
                WIRE_SHOCK => a.shocked = false,
                WIRE_DISABLE => a.disabled = false,
                _ => {}
            }
        }
        return;
    }
    match (wires.holder, wire) {
        (WireHolder::Airlock, WIRE_AI) => {
            if let Some(mut d) = world.get_mut::<Door>(holder) {
                d.ai_control_disabled = match d.ai_control_disabled {
                    AI_WIRE_DISABLED => AI_WIRE_NORMAL,
                    AI_WIRE_HACKED => AI_WIRE_DISABLED_HACKED,
                    s => s,
                };
            }
        }
        (WireHolder::Apc, WIRE_POWER1) => {
            if !wires.is_cut(WIRE_POWER1) && !wires.is_cut(WIRE_POWER2) {
                if let Some(mut a) = world.get_mut::<Apc>(holder) {
                    a.shorted = false;
                }
                apc_update(world, holder);
            }
        }
        (WireHolder::Apc, WIRE_AI) => {
            if !wires.is_cut(WIRE_AI)
                && let Some(mut a) = world.get_mut::<Apc>(holder)
            {
                a.aidisabled = false;
            }
        }
        _ => {}
    }
}
