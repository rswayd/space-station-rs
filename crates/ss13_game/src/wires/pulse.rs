use super::*;

/// tg `cut(wire, source)`: cuts it, or mends a cut one, then `on_cut`.
pub fn cut(world: &mut World, holder: Entity, wire: &str, source: Option<Entity>) {
    let Some(mut wires) = world.get_mut::<Wires>(holder) else {
        return;
    };
    let mend = wires.is_cut(wire);
    if mend {
        wires.cut.retain(|w| w != wire);
    } else {
        wires.cut.push(wire.to_owned());
    }
    let kind = wires.holder;
    match kind {
        WireHolder::Airlock => airlock_on_cut(world, holder, wire, mend, source),
        WireHolder::Apc => apc_on_cut(world, holder, wire, mend, source),
        WireHolder::Vending => vending_on_cut(world, holder, wire, mend, source),
        WireHolder::Autolathe => autolathe_on_cut(world, holder, wire, mend, source),
        WireHolder::Mulebot => crate::bots::mule::on_cut(world, holder, wire, source),
        WireHolder::ChemGrenade => crate::assemblies::chem_grenade::wire_explode(world, holder),
        // `/datum/wires/collar_bomb` has no `on_cut`.
        WireHolder::CollarBomb => {}
        WireHolder::Machine(m) => {
            crate::machine_wires::on_cut(world, holder, m, wire, mend, source)
        }
    }
}

/// tg `pulse(wire, user)`: nothing on a cut wire.
pub fn pulse(world: &mut World, holder: Entity, wire: &str, user: Option<Entity>) {
    let Some(wires) = world.get::<Wires>(holder) else {
        return;
    };
    if wires.is_cut(wire) {
        return;
    }
    on_pulse(world, holder, wire, user);
}

/// The holder's `on_pulse(wire, user)`.
pub(super) fn on_pulse(world: &mut World, holder: Entity, wire: &str, user: Option<Entity>) {
    let Some(kind) = world.get::<Wires>(holder).map(|w| w.holder) else {
        return;
    };
    match kind {
        WireHolder::Airlock => airlock_on_pulse(world, holder, wire, user),
        WireHolder::Apc => apc_on_pulse(world, holder, wire, user),
        WireHolder::Vending => vending_on_pulse(world, holder, wire, user),
        WireHolder::Autolathe => autolathe_on_pulse(world, holder, wire),
        WireHolder::Mulebot => crate::bots::mule::on_pulse(world, holder, wire),
        WireHolder::ChemGrenade => crate::assemblies::chem_grenade::wire_explode(world, holder),
        WireHolder::CollarBomb => crate::clothing::collar_bomb::wire_pulse(world, holder),
        WireHolder::Machine(m) => crate::machine_wires::on_pulse(world, holder, m, wire, user),
    }
}

/// tg `emp_pulse()`: in a shuffled order each wire is pulsed at 33%, at most
/// `MAXIMUM_EMP_WIRES` (3) of them.
pub fn emp_pulse(world: &mut World, holder: Entity) {
    // `/datum/wires/collar_bomb/emp_pulse`: nothing ("I'd rather not get people killed by EMP").
    let Some(mut wires) = world
        .get::<Wires>(holder)
        .filter(|w| w.holder != WireHolder::CollarBomb)
        .map(|w| w.colors.iter().map(|(_, w)| w.clone()).collect::<Vec<_>>())
    else {
        return;
    };
    let mut pulsed = Vec::new();
    {
        let mut rng = world.resource_mut::<crate::emp::EmpRng>();
        for i in (1..wires.len()).rev() {
            let j = rng.range(0, i as i64) as usize;
            wires.swap(i, j);
        }
        for wire in wires {
            if pulsed.len() == MAXIMUM_EMP_WIRES {
                break;
            }
            if rng.prob(33.0) {
                pulsed.push(wire);
            }
        }
    }
    for wire in pulsed {
        pulse(world, holder, &wire, None);
    }
}

/// `cut_color`/`pulse_color` by color.
pub fn cut_color(world: &mut World, holder: Entity, color: &str, source: Option<Entity>) {
    if let Some(wire) = world
        .get::<Wires>(holder)
        .and_then(|w| w.get_wire(color))
        .map(str::to_owned)
    {
        cut(world, holder, &wire, source);
    }
}

pub fn pulse_color(world: &mut World, holder: Entity, color: &str, user: Option<Entity>) {
    if let Some(wire) = world
        .get::<Wires>(holder)
        .and_then(|w| w.get_wire(color))
        .map(str::to_owned)
    {
        pulse(world, holder, &wire, user);
    }
}

pub(super) fn shock_user(world: &mut World, holder: Entity, user: Option<Entity>, chance: f32) {
    if let Some(user) = user {
        shock(world, holder, user, chance);
    }
}

pub(crate) fn schedule_reset(world: &mut World, holder: Entity, secs: f64, wire: &'static str) {
    let at = world.resource::<Time>().elapsed_secs_f64() + secs;
    match world.get_mut::<WireResets>(holder) {
        Some(mut resets) => resets.0.push((at, wire)),
        None => {
            world
                .entity_mut(holder)
                .insert(WireResets(vec![(at, wire)]));
        }
    }
}

pub(crate) fn set_electrified(world: &mut World, holder: Entity, value: Option<Electrified>) {
    let mut e = world.entity_mut(holder);
    match value {
        Some(v) => {
            e.insert(v);
        }
        None => {
            e.remove::<Electrified>();
        }
    }
}

/// tg airlock `bolt()` / `unbolt()`.
pub(crate) fn set_bolts(world: &mut World, door: Entity, bolt: bool) {
    let changed = with_door(world, door, |d, appearance, _| {
        let changed = d.locked != bolt;
        d.locked = bolt;
        d.apply_look(appearance);
        (changed, false)
    });
    if changed == Some(true)
        && let Some(&pos) = world.get::<TilePos>(door)
    {
        let sounds = world.get::<Door>(door).map(|d| d.sounds.clone());
        if let Some(sounds) = sounds {
            let file = if bolt {
                &sounds.bolt_down
            } else {
                &sounds.bolt_up
            };
            let sound = crate::sound::PlaySound::new(pos, file.as_str(), 30.0, false);
            crate::sound::playsound(world, sound.extrarange(3.0));
        }
        hear(world, pos, "Вы слышите щелчок у основания двери.");
    }
}

/// tg `loseMainPower` / `loseBackupPower` / `regain*`.
pub(super) fn set_outage(world: &mut World, door: Entity, f: impl FnOnce(&mut Door)) {
    let pending = with_door(world, door, |d, appearance, _| {
        f(d);
        d.apply_look(appearance);
        (d.main_outage > 0.0 || d.backup_outage > 0.0, false)
    });
    if pending == Some(true) {
        world.entity_mut(door).insert(PowerOutage);
    }
}

/// tg airlock `loseMainPower()` / `loseBackupPower()` (the AI's "Disrupt" buttons).
pub(crate) fn lose_power(world: &mut World, door: Entity, main: bool) {
    set_outage(world, door, |d| {
        if main {
            lose_main_power(d);
        } else {
            lose_backup_power(d);
        }
    });
}

pub(super) fn lose_main_power(d: &mut Door) {
    if d.main_outage <= 0.0 {
        d.main_outage = MAIN_OUTAGE_SECS;
        if d.backup_outage < BACKUP_AFTER_MAIN_SECS {
            d.backup_outage = BACKUP_AFTER_MAIN_SECS;
        }
    }
}

pub(super) fn lose_backup_power(d: &mut Door) {
    if d.backup_outage < BACKUP_OUTAGE_SECS {
        d.backup_outage = BACKUP_OUTAGE_SECS;
    }
}

pub(super) fn ai_cut(state: i8, mend: bool) -> i8 {
    match (mend, state) {
        (true, AI_WIRE_DISABLED) => AI_WIRE_NORMAL,
        (true, AI_WIRE_HACKED) => AI_WIRE_DISABLED_HACKED,
        (false, AI_WIRE_NORMAL) => AI_WIRE_DISABLED,
        (false, AI_WIRE_DISABLED_HACKED) => AI_WIRE_HACKED,
        (_, s) => s,
    }
}

/// tg vending `throw_item()`: a random stocked product flies at the first living mob in view.
pub(super) fn throw_item(world: &mut World, vendor: Entity) {
    let Some(&pos) = world.get::<TilePos>(vendor) else {
        return;
    };
    let target = world
        .resource::<SpatialIndex>()
        .in_radius(pos, SHOOT_VIEW)
        .find(|&e| world.get::<Health>(e).is_some());
    let Some(target) = target else {
        return;
    };
    let Some(&target_pos) = world.get::<TilePos>(target) else {
        return;
    };
    let Some(machine) = world.get::<Vendor>(vendor) else {
        return;
    };
    let mut stocked: Vec<usize> = machine
        .records
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RecordKind::Product && r.amount > 0)
        .map(|(i, _)| i)
        .collect();
    if stocked.is_empty() {
        return;
    }
    let pick = world
        .resource_mut::<CombatRng>()
        .range(0, stocked.len() as u32 - 1) as usize;
    let record = stocked.swap_remove(pick);
    // "Always throw new stuff that costs before free returned stuff".
    let only_returned = world
        .get::<Vendor>(vendor)
        .is_some_and(|v| v.records[record].amount as usize <= v.records[record].returned.len());
    let Some(item) = crate::vending::dispense(world, vendor, record, pos, true, only_returned)
    else {
        return;
    };
    let now_ds = world.resource::<Time>().elapsed_secs_f64() * 10.0;
    let Some(path) =
        crate::throw::Trajectory::new(pos, target_pos, SHOOT_RANGE, SHOOT_SPEED, now_ds)
    else {
        return;
    };
    world.entity_mut(item).insert((
        ss13_net::server::Networked,
        pos,
        crate::throw::Thrown {
            path,
            thrower: None,
            target: Some(target),
            last_dir: None,
        },
    ));
    let text = format!(
        "{} швыряет {} в {}!",
        name_of(world, vendor),
        name_of(world, item),
        name_of(world, target)
    );
    crate::security::show(
        world,
        pos,
        DEFAULT_MESSAGE_RANGE,
        ChatChannel::Combat,
        (Span::Danger, text),
        &[],
    );
}

/// Countdowns: airlock power outages (paused while their wires are cut, tg
/// `handle_main_power`), electrification (airlocks per second, vendors per machine tick while
/// powered), timed resets, shooting vendors, second shocks.
pub(super) fn tick_wires(world: &mut World) {
    let dt = f64::from(world.resource::<Time>().delta_secs());
    let now = world.resource::<Time>().elapsed_secs_f64();

    let mut outages =
        world.query_filtered::<(Entity, &mut Door, &Wires, &mut Appearance), With<PowerOutage>>();
    let mut done = Vec::new();
    for (e, mut door, wires, mut appearance) in outages.iter_mut(world) {
        let was = door.wire_power();
        if door.main_outage > 0.0 && !wires.is_cut(WIRE_POWER1) && !wires.is_cut(WIRE_POWER2) {
            door.main_outage = (door.main_outage - dt).max(0.0);
        }
        if door.backup_outage > 0.0 && !wires.is_cut(WIRE_BACKUP1) && !wires.is_cut(WIRE_BACKUP2) {
            door.backup_outage = (door.backup_outage - dt).max(0.0);
        }
        if was != door.wire_power() {
            door.apply_look(&mut appearance);
        }
        if door.main_outage <= 0.0 && door.backup_outage <= 0.0 {
            done.push(e);
        }
    }
    for e in done {
        world.entity_mut(e).remove::<PowerOutage>();
    }

    let mut electrified = world.query::<(Entity, &Electrified, Option<&Door>, Has<Unpowered>)>();
    let timed: Vec<(Entity, f64, bool, bool)> = electrified
        .iter(world)
        .filter_map(|(e, el, door, unpowered)| match el {
            Electrified::Timed(left) => Some((e, *left, door.is_some(), unpowered)),
            Electrified::Permanent => None,
        })
        .collect();
    for (e, left, is_door, unpowered) in timed {
        // Vendors count `process()` calls, which stop without power (`is_operational`).
        let step = if is_door {
            dt
        } else if unpowered {
            0.0
        } else {
            dt / MACHINE_TICK_SECS
        };
        let left = left - step;
        set_electrified(world, e, (left > 0.0).then_some(Electrified::Timed(left)));
    }

    let mut resets = world.query::<(Entity, &WireResets)>();
    let due: Vec<(Entity, Vec<&'static str>, bool)> = resets
        .iter(world)
        .filter(|(_, r)| r.0.iter().any(|(at, _)| *at <= now))
        .map(|(e, r)| {
            let wires =
                r.0.iter()
                    .filter(|(at, _)| *at <= now)
                    .map(|(_, w)| *w)
                    .collect();
            (e, wires, r.0.iter().all(|(at, _)| *at <= now))
        })
        .collect();
    for (e, wires, all) in due {
        if all {
            world.entity_mut(e).remove::<WireResets>();
        } else if let Some(mut r) = world.get_mut::<WireResets>(e) {
            r.0.retain(|(at, _)| *at > now);
        }
        for wire in wires {
            reset(world, e, wire);
        }
    }

    let mut shooters =
        world.query_filtered::<Entity, (With<ShootsInventory>, Without<Unpowered>)>();
    let shooters: Vec<Entity> = shooters.iter(world).collect();
    // `SPT_PROB(shoot_inventory_chance, seconds_per_tick)` per machine tick, as a rate.
    let chance = (1.0 - (1.0 - SHOOT_CHANCE / 100.0).powf(dt / MACHINE_TICK_SECS)) * 100.0;
    for vendor in shooters {
        if world.resource_mut::<CombatRng>().chance(chance as f32) {
            throw_item(world, vendor);
        }
    }

    let mut seconds = world.query::<(Entity, &SecondaryShock)>();
    let due: Vec<(Entity, f32)> = seconds
        .iter(world)
        .filter(|(_, s)| s.at <= now)
        .map(|(e, s)| (e, s.secs))
        .collect();
    for (mob, secs) in due {
        world.entity_mut(mob).remove::<SecondaryShock>();
        crate::status_effects::apply(world, mob, Incap::Paralyze, secs);
    }
}
