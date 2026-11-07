use super::*;

/// The machines' own `process` and the reagent-change hooks (`acclimator/clear`,
/// `reaction_chamber/on_reagent_change`, `buffer/on_reagent_change`, `disposer/process`).
pub(super) fn process_machines(world: &mut World, secs: f32) {
    let chem = world.resource::<Chem>().0.clone();
    let machines: Vec<Entity> = world
        .query_filtered::<Entity, With<Plumber>>()
        .iter(world)
        .collect();
    for e in machines {
        if !anchored(world, e) {
            continue;
        }
        let Some(current) = kind(world, e).cloned() else {
            continue;
        };
        match current {
            Kind::Acclimator {
                target,
                max_volume,
                state,
            } => {
                let Some(mut h) = take_holder(world, e) else {
                    continue;
                };
                let mut state = state;
                if state == Acclimate::Emptying && h.total_volume <= 0.0 {
                    state = Acclimate::Filling;
                    h.flags |= flags::NO_REACT;
                } else if h.total_volume > 0.0
                    && matches!(state, Acclimate::Heating | Acclimate::Cooling)
                {
                    state = if h.chem_temp > target {
                        Acclimate::Cooling
                    } else {
                        Acclimate::Heating
                    };
                    let energy =
                        (target - h.chem_temp) * ACCLIMATOR_COEFF * secs * h.heat_capacity(&chem);
                    h.adjust_thermal_energy(
                        &chem,
                        energy,
                        0.0,
                        ss13_reagents::CHEMICAL_MAXIMUM_TEMPERATURE,
                    );
                    h.handle_reactions(&chem);
                    let temp = round_to(h.chem_temp, 0.01);
                    if !h.is_reacting
                        && temp >= target - ACCLIMATOR_TEMP_DIFF
                        && temp <= target + ACCLIMATOR_TEMP_DIFF
                    {
                        h.set_temperature(target);
                        state = Acclimate::Emptying;
                    }
                }
                put_holder(world, e, h);
                set_kind(
                    world,
                    e,
                    Kind::Acclimator {
                        target,
                        max_volume,
                        state,
                    },
                );
            }
            Kind::ReactionChamber { .. } => chamber_process(world, e, secs),
            Kind::Synthesizer {
                reagent: Some(id),
                amount,
                ..
            } => {
                // `synthesizer/process`: `amount` more whenever it holds less.
                if amount > 0.0
                    && let Some(mut r) = world.get_mut::<Reagents>(e)
                    && r.0.total_volume < amount
                {
                    r.0.add_reagent(
                        &chem,
                        id,
                        amount,
                        ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
                        None,
                        None,
                        false,
                        false,
                    );
                }
            }
            Kind::PillPress {
                volume,
                product,
                package,
                duration,
            } => press(world, e, volume, &product, package, duration),
            Kind::Bottler { wanted } => bottle(world, e, wanted),
            Kind::Fermenter => ferment_tile(world, e),
            Kind::Grinder => {
                for item in loose_items(world, e) {
                    blend(world, e, item);
                }
            }
            Kind::Receiver { senders, next } => {
                // `receiver/process`: one sender a tick, in turn.
                let senders: Vec<Entity> = senders
                    .into_iter()
                    .filter(|&s| matches!(kind(world, s), Some(Kind::Sender { target: Some(t) }) if *t == e))
                    .collect();
                if senders.is_empty() {
                    set_kind(world, e, Kind::Receiver { senders, next: 0 });
                    continue;
                }
                let index = if next >= senders.len() { 0 } else { next };
                let sender = senders[index];
                if let Some((mut a, mut b)) = take_pair(world, sender, e) {
                    a.trans_to(&chem, &mut b, RECEIVER_PULL, 1.0, None, false, false);
                    put_holder(world, sender, a);
                    put_holder(world, e, b);
                }
                set_kind(
                    world,
                    e,
                    Kind::Receiver {
                        senders,
                        next: index + 1,
                    },
                );
            }
            Kind::Disposer { on: true, rate } => {
                if let Some(mut r) = world.get_mut::<Reagents>(e) {
                    r.0.remove_all(rate * secs);
                }
            }
            Kind::Buffer { .. } => buffer_changed(world, e),
            _ => {}
        }
    }
}

/// `reaction_chamber/process`: holds the target temperature while filling or reacting, and
/// the chemistry chamber's `handle_reagents` keeps the pH between its limits from its
/// buffers; `on_reagent_change` ends the emptying.
pub(super) fn chamber_process(world: &mut World, e: Entity, secs: f32) {
    let chem = world.resource::<Chem>().0.clone();
    let Some(Kind::ReactionChamber {
        catalysts,
        emptying,
        target,
        ph,
        ..
    }) = kind(world, e).cloned()
    else {
        return;
    };
    let Some(mut h) = take_holder(world, e) else {
        return;
    };
    let mut emptying = emptying;
    let mut ph = ph;
    if emptying && catalyst_excluded_volume(&h, &catalysts) <= 0.0 {
        emptying = false;
        h.flags |= flags::NO_REACT;
    }
    if h.total_volume > 0.0 && (!emptying || h.is_reacting) {
        let energy = (target - h.chem_temp) * CHAMBER_COEFF * secs * h.heat_capacity(&chem);
        h.adjust_thermal_energy(
            &chem,
            energy,
            0.0,
            ss13_reagents::CHEMICAL_MAXIMUM_TEMPERATURE,
        );
        if let Some(ph) = ph.as_mut() {
            let n = h.reagents.len();
            let too_basic = h.ph > ph.alkaline_limit;
            if n > 0 && (h.ph < ph.acidic_limit || too_basic) {
                let change = if too_basic {
                    h.ph - ph.alkaline_limit
                } else {
                    ph.acidic_limit - h.ph
                }
                .max(0.25);
                let amount = change * h.total_volume / (BUFFER_IONIZING_STRENGTH * n as f32) * secs;
                let buffer = if too_basic {
                    &mut ph.acid
                } else {
                    &mut ph.base
                };
                buffer.trans_to(&chem, &mut h, amount, 1.0, None, false, false);
            }
        }
    }
    put_holder(world, e, h);
    if let Some(mut p) = world.get_mut::<Plumber>(e)
        && let Kind::ReactionChamber {
            emptying: em,
            ph: p_ph,
            ..
        } = &mut p.kind
    {
        *em = emptying;
        *p_ph = ph;
    }
}

/// Items lying loose on the machine's tile.
pub(super) fn loose_items(world: &World, e: Entity) -> Vec<Entity> {
    let Some(&pos) = world.get::<TilePos>(e) else {
        return Vec::new();
    };
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .copied()
        .filter(|&i| i != e && world.get::<ss13_mapload::DmItem>(i).is_some())
        .filter(|&i| !anchored(world, i))
        .collect()
}

/// `pill_press/process`: a product of `volume` units on the floor.
pub(super) fn press(
    world: &mut World,
    e: Entity,
    volume: f32,
    product: &str,
    package: Package,
    layers: f32,
) {
    if holder(world, e).is_none_or(|h| h.total_volume < volume) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let (path, _) = package.container();
    let Some(item) = crate::construction::spawn_obj(world, path, pos) else {
        warn!("pill press: no prototype for {path}");
        return;
    };
    if let Some(mut n) = world.get_mut::<ss13_mapload::DmName>(item) {
        n.name = format!("{product} {}", package.suffix());
    }
    let chem = world.resource::<Chem>().0.clone();
    if let Some((mut a, mut b)) = take_pair(world, e, item) {
        b.clear();
        a.trans_to(&chem, &mut b, volume, 1.0, None, false, false);
        put_holder(world, e, a);
        put_holder(world, item, b);
    }
    if package == Package::Pill {
        world
            .entity_mut(item)
            .insert(crate::chem::PillShell(layers));
    }
}

/// `bottler/process`: a cup or dart from behind is filled and put in front, or to the side
/// when it has no room.
// shortcut: the first item behind instead of `pick` among the tile's contents; slime extracts
// are not filled (→ ../xenobiology).
pub(super) fn bottle(world: &mut World, e: Entity, wanted: f32) {
    if holder(world, e).is_none_or(|h| h.total_volume < wanted) {
        return;
    }
    let Some(&pos) = world.get::<TilePos>(e) else {
        return;
    };
    let facing = dir_of(world, e);
    let step = |d: u8| Dir::from_bits(d).map(|d| pos.step(d));
    // `setDir`: front, behind, and the side clockwise of the front.
    let (Some(good), Some(input), Some(bad)) = (
        step(facing),
        step(reverse(facing)),
        step(CW[(cw_index(facing) + 1) % 4]),
    ) else {
        return;
    };
    let item = world
        .resource::<SpatialIndex>()
        .on_tile(input)
        .iter()
        .copied()
        .find(|&i| world.get::<ss13_mapload::DmItem>(i).is_some() && !anchored(world, i));
    let Some(item) = item else {
        return;
    };
    let path = path_of(world, item).unwrap_or_default();
    if !is_under(&path, CUP) && !is_under(&path, DART) {
        return;
    }
    let fits = holder(world, item).is_some_and(|h| h.total_volume + wanted <= h.maximum_volume);
    if !fits {
        world.entity_mut(item).insert(bad);
        return;
    }
    let chem = world.resource::<Chem>().0.clone();
    if let Some((mut a, mut b)) = take_pair(world, e, item) {
        a.trans_to(&chem, &mut b, wanted, 1.0, None, false, false);
        put_holder(world, e, a);
        put_holder(world, item, b);
    }
    world.entity_mut(item).insert(good);
}

/// `fermenter/ferment` of the produce on its tile: `potency × 0.25` of its `distill_reagent`.
// shortcut: the tile is checked every plumbing tick (no `COMSIG_ATOM_ENTERED`), and the
// fermenter does not let items through its eating side.
pub(super) fn ferment_tile(world: &mut World, e: Entity) {
    let chem = world.resource::<Chem>().0.clone();
    for item in loose_items(world, e) {
        if holder(world, e).is_none_or(Holder::holder_full) {
            return;
        }
        let path = path_of(world, item).unwrap_or_default();
        if !is_under(&path, GROWN) {
            continue;
        }
        let distill = world
            .get_resource::<crate::botany::Botany>()
            .and_then(|b| b.0.grown_def(&path).map(|d| d.distill_reagent.clone()))
            .unwrap_or_default();
        let Some(id) = chem.id(&distill) else {
            continue;
        };
        let potency = world
            .get::<crate::botany::seeds::Seed>(item)
            .map_or(0.0, |s| s.potency);
        if let Some(mut r) = world.get_mut::<Reagents>(e) {
            r.0.add_reagent(
                &chem,
                id,
                potency * 0.25,
                ss13_reagents::DEFAULT_REAGENT_TEMPERATURE,
                None,
                None,
                false,
                false,
            );
        }
        world.despawn(item);
    }
}

/// `grinder_chemical/blend`: the item's reagents into the grinder (`/obj/item/proc/grind`),
/// and it is gone.
// shortcut: no juicing mode and no `grind_results` (as `crate::kitchen`'s grinder).
pub(super) fn blend(world: &mut World, grinder: Entity, item: Entity) {
    if holder(world, grinder).is_none_or(Holder::holder_full) {
        return;
    }
    let chem = world.resource::<Chem>().0.clone();
    if let Some((mut a, mut b)) = take_pair(world, item, grinder) {
        let total = a.total_volume;
        a.trans_to(&chem, &mut b, total, 1.0, None, false, false);
        put_holder(world, item, a);
        put_holder(world, grinder, b);
    }
    world.despawn(item);
}

/// `buffer/on_reagent_change`: fill to the threshold, wait for the linked ones, release
/// together, and start over once all are empty.
pub(super) fn buffer_changed(world: &mut World, e: Entity) {
    let Some(Kind::Buffer {
        activation,
        mode,
        links,
    }) = kind(world, e).cloned()
    else {
        return;
    };
    let volume = |w: &World, b: Entity| holder(w, b).map_or(0.0, |h| h.total_volume);
    let mode_of = |w: &World, b: Entity| match kind(w, b) {
        Some(Kind::Buffer { mode, .. }) => Some(*mode),
        _ => None,
    };
    let mut group: Vec<Entity> = links
        .iter()
        .copied()
        .filter(|&b| mode_of(world, b).is_some())
        .collect();
    if !group.contains(&e) {
        group.push(e);
    }
    let mut mode = mode;
    if mode == BufferMode::Unready && volume(world, e) >= activation {
        mode = BufferMode::Idle;
        set_buffer_mode(world, e, mode);
    }
    if mode == BufferMode::Idle {
        if group
            .iter()
            .any(|&b| mode_of(world, b) == Some(BufferMode::Unready))
        {
            return;
        }
        for &b in &group {
            set_buffer_mode(world, b, BufferMode::Ready);
        }
        mode = BufferMode::Ready;
    }
    if mode == BufferMode::Ready {
        if group.iter().any(|&b| volume(world, b) > 0.0) {
            return;
        }
        for &b in &group {
            set_buffer_mode(world, b, BufferMode::Unready);
        }
    }
}

pub(super) fn set_buffer_mode(world: &mut World, e: Entity, new: BufferMode) {
    if let Some(mut p) = world.get_mut::<Plumber>(e)
        && let Kind::Buffer { mode, .. } = &mut p.kind
    {
        *mode = new;
    }
}

/// `SSplumbing`: every second the networks, the machines and the requests.
pub(super) fn run_plumbing(world: &mut World, mut since: Local<f32>) {
    if !world.contains_resource::<Chem>() {
        return;
    }
    *since += world.resource::<Time>().delta_secs();
    if *since < PLUMBING_WAIT {
        return;
    }
    let secs = *since;
    *since = 0.0;
    tick(world, secs);
}

/// One `SSplumbing` fire: the networks, the machines' `process`, every demand connect's
/// request.
pub fn tick(world: &mut World, secs: f32) {
    let nets = rebuild(world);
    process_machines(world, secs);
    for net in &nets {
        for &d in &net.demanders {
            // A buffered input checks its own beaker.
            let buffered = world
                .get::<Plumber>(d.machine)
                .and_then(|p| p.ports.get(d.port))
                .is_some_and(|p| p.buffer.is_some());
            if !buffered && holder(world, d.machine).is_none_or(Holder::holder_full) {
                continue;
            }
            send_request(world, net, d);
        }
    }
    if let Some(mut r) = world.get_resource_mut::<DuctNets>() {
        r.nets = nets;
    }
}

pub(super) fn sound(world: &mut World, e: Entity, file: &str, vol: f32) {
    if let Some(&pos) = world.get::<TilePos>(e) {
        playsound(world, PlaySound::new(pos, file, vol, true));
    }
}
