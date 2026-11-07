use super::*;

/// The networks: ducts joined by layer, the anchored machines' connects into them, and
/// machine-to-machine connects (`duct/post_machine_initialize`, `plumbing/enable`). Updates the
/// ducts' icons.
pub(super) fn rebuild(world: &mut World) -> Vec<Net> {
    let ducts: Vec<(Entity, TilePos, u8)> = world
        .query::<(Entity, &TilePos, &Duct)>()
        .iter(world)
        .map(|(e, p, d)| (e, *p, d.layer))
        .collect();
    let mut at: HashMap<(TilePos, u8), usize> = HashMap::new();
    for (i, &(_, pos, layer)) in ducts.iter().enumerate() {
        at.entry((pos, layer)).or_insert(i);
    }
    let mut net_of = vec![usize::MAX; ducts.len()];
    let mut nets: Vec<Net> = Vec::new();
    for start in 0..ducts.len() {
        if net_of[start] != usize::MAX {
            continue;
        }
        let id = nets.len();
        nets.push(Net::default());
        let mut queue = vec![start];
        net_of[start] = id;
        while let Some(i) = queue.pop() {
            let (_, pos, layer) = ducts[i];
            for d in CARDINALS {
                let Some(dir) = Dir::from_bits(d) else {
                    continue;
                };
                if let Some(&j) = at.get(&(pos.step(dir), layer))
                    && net_of[j] == usize::MAX
                {
                    net_of[j] = id;
                    queue.push(j);
                }
            }
        }
    }
    let machines: Vec<(Entity, TilePos, u8, Vec<Port>)> = world
        .query::<(Entity, &TilePos, &Plumber)>()
        .iter(world)
        .map(|(e, p, pl)| (e, *p, 0, pl.ports.clone()))
        .collect::<Vec<_>>()
        .into_iter()
        .filter(|(e, ..)| anchored(world, *e))
        .map(|(e, p, _, ports)| (e, p, dir_of(world, e), ports))
        .collect();
    let mut machine_at: HashMap<TilePos, Vec<usize>> = HashMap::new();
    for (i, m) in machines.iter().enumerate() {
        machine_at.entry(m.1).or_default().push(i);
    }
    let mut duct_links: Vec<u8> = vec![0; ducts.len()];
    for &(machine, pos, facing, ref ports) in &machines {
        for (pi, p) in ports.iter().enumerate() {
            for drawn in CARDINALS {
                let demand = p.demand & drawn != 0;
                let supply = p.supply & drawn != 0;
                if !demand && !supply {
                    continue;
                }
                let world_dir = rotate(drawn, facing);
                let Some(step) = Dir::from_bits(world_dir) else {
                    continue;
                };
                let next = pos.step(step);
                let end = End {
                    machine,
                    port: pi,
                    dir: drawn,
                };
                if let Some(&j) = at.get(&(next, p.layer)) {
                    duct_links[j] |= reverse(world_dir);
                    let net = &mut nets[net_of[j]];
                    if supply {
                        net.suppliers.push(end);
                    }
                    if demand {
                        net.demanders.push(end);
                    }
                    continue;
                }
                // Machine to machine: our supply into their demand makes a two-end network.
                if !supply {
                    continue;
                }
                for &k in machine_at.get(&next).into_iter().flatten() {
                    let (other, _, other_facing, ref other_ports) = machines[k];
                    for (oi, op) in other_ports.iter().enumerate() {
                        if op.layer & p.layer == 0 {
                            continue;
                        }
                        let other_drawn = unrotate(reverse(world_dir), other_facing);
                        if op.demand & other_drawn != 0 {
                            nets.push(Net {
                                suppliers: vec![end],
                                demanders: vec![End {
                                    machine: other,
                                    port: oi,
                                    dir: other_drawn,
                                }],
                            });
                        }
                    }
                }
            }
        }
    }
    // `duct/update_icon_state`: connects to ducts and machines.
    for (i, &(e, pos, layer)) in ducts.iter().enumerate() {
        let mut connects = duct_links[i];
        for d in CARDINALS {
            if let Some(dir) = Dir::from_bits(d)
                && at.contains_key(&(pos.step(dir), layer))
            {
                connects |= d;
            }
        }
        let mut state = "nduct".to_owned();
        for (bit, s) in [(N, "_n"), (S, "_s"), (E, "_e"), (W, "_w")] {
            if connects & bit != 0 {
                state.push_str(s);
            }
        }
        let current = world
            .get::<ss13_mapload::DmAtom>(e)
            .map(|a| a.icon_state.clone());
        if current.as_deref() != Some(state.as_str()) {
            crate::obj_damage::set_icon_state(world, e, &state);
            if let Some(mut atom) = world.get_mut::<ss13_mapload::DmAtom>(e) {
                atom.icon_state = state;
            }
        }
    }
    nets
}

pub(super) fn take_holder(world: &mut World, e: Entity) -> Option<Holder> {
    let mut r = world.get_mut::<Reagents>(e)?;
    Some(std::mem::replace(
        &mut r.0,
        Holder::new(0.0, 0, Owner::default()),
    ))
}

/// Both holders of a transfer, or neither (a missing one leaves the other in place).
pub(super) fn take_pair(world: &mut World, a: Entity, b: Entity) -> Option<(Holder, Holder)> {
    if a == b || world.get::<Reagents>(a).is_none() || world.get::<Reagents>(b).is_none() {
        return None;
    }
    Some((take_holder(world, a)?, take_holder(world, b)?))
}

pub(super) fn put_holder(world: &mut World, e: Entity, holder: Holder) {
    let reacting = holder.is_reacting;
    if let Some(mut r) = world.get_mut::<Reagents>(e) {
        r.0 = holder;
    }
    if reacting {
        world.entity_mut(e).insert(crate::chem::Reacting);
    }
}

pub(super) fn holder(world: &World, e: Entity) -> Option<&Holder> {
    world.get::<Reagents>(e).map(|r| &r.0)
}

pub(super) fn kind(world: &World, e: Entity) -> Option<&Kind> {
    world.get::<Plumber>(e).map(|p| &p.kind)
}

/// `filter/can_give_in_direction`.
pub(super) fn filter_allows(
    left: &[ReagentId],
    right: &[ReagentId],
    dir: u8,
    id: ReagentId,
) -> bool {
    match dir {
        S => !left.contains(&id) && !right.contains(&id),
        E => left.contains(&id),
        W => right.contains(&id),
        _ => false,
    }
}

/// `can_give` of a supplier through its connect `dir`.
pub(super) fn can_give(world: &World, end: End, amount: f32, reagent: Option<ReagentId>) -> bool {
    let Some(h) = holder(world, end.machine) else {
        return false;
    };
    if amount <= 0.0 {
        return false;
    }
    let base = match reagent {
        Some(id) => h.has(id).is_some(),
        None => h.total_volume > 0.0,
    };
    match kind(world, end.machine) {
        Some(Kind::Buffer { mode, .. }) => *mode == BufferMode::Ready && base,
        Some(Kind::Acclimator { state, .. }) => {
            *state == Acclimate::Emptying && !h.is_reacting && base
        }
        // `reaction_chamber/can_give`: only what exceeds the catalysts.
        Some(Kind::ReactionChamber {
            emptying,
            catalysts,
            ..
        }) => {
            *emptying
                && !h.is_reacting
                && h.reagents.iter().any(|r| {
                    reagent.is_none_or(|id| id == r.id)
                        && r.volume > catalyst_amount(catalysts, r.id)
                })
        }
        Some(Kind::Filter { left, right }) => {
            reagent.is_none_or(|id| filter_allows(left, right, end.dir, id)) && base
        }
        Some(Kind::IvDrip) => {
            base && world
                .get::<crate::medical::blood::IvDrip>(end.machine)
                .is_some_and(|d| !d.injecting)
        }
        _ => base,
    }
}

pub(super) fn catalyst_amount(catalysts: &[(ReagentId, f32)], id: ReagentId) -> f32 {
    catalysts
        .iter()
        .find(|&&(c, _)| c == id)
        .map_or(0.0, |&(_, v)| v)
}

/// `get_catalyst_excluded_volume`.
pub(super) fn catalyst_excluded_volume(h: &Holder, catalysts: &[(ReagentId, f32)]) -> f32 {
    let total: f32 = h
        .reagents
        .iter()
        .map(|r| (r.volume - catalyst_amount(catalysts, r.id)).max(0.0))
        .sum();
    round_to(total, CHEMICAL_VOLUME_ROUNDING).min(h.maximum_volume)
}

/// `transfer_to` from `from` into `to`.
pub(super) fn transfer_to(
    world: &mut World,
    from: End,
    to: Entity,
    amount: f32,
    reagent: Option<ReagentId>,
) {
    let chem = world.resource::<Chem>().0.clone();
    let mut amount = amount;
    // The reagents that may leave and how much of each (`remove_blacklisted`, catalysts).
    let mut allowed: Option<Vec<(ReagentId, f32)>> = None;
    match kind(world, from.machine) {
        Some(Kind::Splitter {
            straight,
            left,
            right,
        }) => {
            let limit = match from.dir {
                S => *straight,
                E => *left,
                W => *right,
                _ => f32::INFINITY,
            };
            amount = amount.min(limit);
        }
        Some(Kind::Filter { left, right }) if reagent.is_none() => {
            let h = holder(world, from.machine);
            allowed = Some(
                h.map(|h| {
                    h.reagents
                        .iter()
                        .filter(|r| filter_allows(left, right, from.dir, r.id))
                        .map(|r| (r.id, r.volume))
                        .collect()
                })
                .unwrap_or_default(),
            );
        }
        // `/datum/reagents/plumbing/reaction_chamber/trans_to`: the catalysts stay.
        Some(Kind::ReactionChamber { catalysts, .. }) if !catalysts.is_empty() => {
            let h = holder(world, from.machine);
            allowed = Some(
                h.map(|h| {
                    h.reagents
                        .iter()
                        .filter(|r| reagent.is_none_or(|id| id == r.id))
                        .map(|r| (r.id, r.volume - catalyst_amount(catalysts, r.id)))
                        .filter(|&(_, v)| v > 0.0)
                        .collect()
                })
                .unwrap_or_default(),
            );
        }
        _ => {}
    }
    if from.machine == to {
        return;
    }
    let Some((mut a, mut b)) = take_pair(world, from.machine, to) else {
        return;
    };
    match allowed {
        // `remove_blacklisted`: only the allowed reagents, in proportion.
        Some(ids) => {
            let total: f32 = ids.iter().map(|&(_, v)| v).sum();
            if total > 0.0 {
                let amount = amount.min(total);
                for (id, available) in ids {
                    let part = available / total * amount;
                    a.trans_to(&chem, &mut b, part, 1.0, Some(id), false, false);
                }
            }
        }
        None => {
            a.trans_to(&chem, &mut b, amount, 1.0, reagent, false, false);
        }
    }
    put_holder(world, from.machine, a);
    put_holder(world, to, b);
}

/// `process_request`: an even share from every valid supplier of `net`.
pub(super) fn process_request(
    world: &mut World,
    net: &Net,
    me: Entity,
    amount: f32,
    reagent: Option<ReagentId>,
) {
    let valid: Vec<End> = net
        .suppliers
        .iter()
        .copied()
        .filter(|s| s.machine != me && can_give(world, *s, amount, reagent))
        .collect();
    let Some(current) = holder(world, me).map(|h| h.total_volume) else {
        return;
    };
    let target = current + amount;
    let mut left = valid.len();
    for give in valid {
        let now = holder(world, me).map_or(target, |h| h.total_volume);
        let request = (target - now) / left as f32;
        transfer_to(world, give, me, request, reagent);
        left -= 1;
    }
}

/// `get_all_network_reagents`: ours first, then every supplier's.
pub(super) fn network_reagents(world: &World, net: &Net, me: Entity) -> Vec<ReagentId> {
    let mut out: Vec<ReagentId> = Vec::new();
    let mut add = |h: Option<&Holder>| {
        for r in h.map(|h| h.reagents.as_slice()).unwrap_or_default() {
            if !out.contains(&r.id) {
                out.push(r.id);
            }
        }
    };
    add(holder(world, me));
    for s in &net.suppliers {
        add(holder(world, s.machine));
    }
    out
}

/// `send_request` of the base component (`distinct_reagent_cap`).
pub(super) fn default_request(world: &mut World, net: &Net, me: Entity, cap: Option<usize>) {
    let Some(cap) = cap else {
        process_request(world, net, me, MACHINE_REAGENT_TRANSFER, None);
        return;
    };
    let mut all = network_reagents(world, net, me);
    all.truncate(cap);
    if all.is_empty() {
        return;
    }
    let per = round_to(
        MACHINE_REAGENT_TRANSFER / all.len() as f32,
        CHEMICAL_VOLUME_ROUNDING,
    );
    for id in all {
        process_request(world, net, me, per, Some(id));
    }
}

pub(super) fn set_kind(world: &mut World, e: Entity, kind: Kind) {
    if let Some(mut p) = world.get_mut::<Plumber>(e) {
        p.kind = kind;
    }
}

pub(super) fn set_flag(world: &mut World, e: Entity, flag: u32, on: bool) {
    if let Some(mut r) = world.get_mut::<Reagents>(e) {
        if on {
            r.0.flags |= flag;
        } else {
            r.0.flags &= !flag;
        }
    }
}

/// `send_request(dir)` of a demander's connect.
pub(super) fn send_request(world: &mut World, net: &Net, end: End) {
    let me = end.machine;
    let Some(plumber) = world.get::<Plumber>(me).cloned() else {
        return;
    };
    if let Some(acidic) = plumber.ports.get(end.port).and_then(|p| p.buffer) {
        buffered_request(world, net, me, acidic);
        return;
    }
    match plumber.kind {
        Kind::Disposer { on, rate } => {
            if on {
                process_request(world, net, me, rate * PLUMBING_WAIT, None);
            }
        }
        Kind::Buffer { mode, .. } => {
            if mode == BufferMode::Unready {
                default_request(world, net, me, plumber.cap);
            }
        }
        Kind::Acclimator {
            target,
            max_volume,
            state: Acclimate::Filling,
        } => {
            let Some(h) = holder(world, me) else {
                return;
            };
            if h.total_volume < max_volume {
                default_request(world, net, me, plumber.cap);
                if !holder(world, me).is_some_and(Holder::holder_full) {
                    return;
                }
            }
            set_flag(world, me, flags::NO_REACT, false);
            let temp = holder(world, me).map_or(target, |h| h.chem_temp);
            let state = if temp > target {
                Acclimate::Cooling
            } else {
                Acclimate::Heating
            };
            set_kind(
                world,
                me,
                Kind::Acclimator {
                    target,
                    max_volume,
                    state,
                },
            );
        }
        Kind::Acclimator { .. } => {}
        Kind::ReactionChamber {
            required,
            catalysts,
            emptying,
            ..
        } => {
            if emptying || (required.is_empty() && catalysts.is_empty()) {
                return;
            }
            // `catalysts | required_reagents`: a missing catalyst is asked for without waiting.
            let wanted = catalysts
                .iter()
                .map(|&(id, v)| (id, v, true))
                .chain(required.iter().map(|&(id, v)| (id, v, false)));
            for (id, wanted, catalyst) in wanted.collect::<Vec<_>>() {
                let present = holder(world, me).map_or(0.0, |h| h.amount(id));
                let diff = (wanted - present).min(MACHINE_REAGENT_TRANSFER);
                if diff >= ss13_reagents::CHEMICAL_QUANTISATION_LEVEL {
                    process_request(world, net, me, diff, Some(id));
                    if !catalyst {
                        return;
                    }
                }
            }
            set_flag(world, me, flags::NO_REACT, false);
            let chem = world.resource::<Chem>().0.clone();
            let mut emptying = true;
            if let Some(mut h) = take_holder(world, me) {
                h.handle_reactions(&chem);
                // `on_reagent_change`: some reactions leave nothing to send on.
                if catalyst_excluded_volume(&h, &catalysts) <= 0.0 {
                    emptying = false;
                    h.flags |= flags::NO_REACT;
                }
                put_holder(world, me, h);
            }
            if let Some(mut p) = world.get_mut::<Plumber>(me)
                && let Kind::ReactionChamber { emptying: e, .. } = &mut p.kind
            {
                *e = emptying;
            }
        }
        // `pill_press/send_request`: not past the product's volume, nor with 10 containers on
        // the floor.
        Kind::PillPress { volume, .. } => {
            if holder(world, me).is_some_and(|h| h.total_volume >= volume) {
                return;
            }
            let Some(&pos) = world.get::<TilePos>(me) else {
                return;
            };
            let products = world
                .resource::<SpatialIndex>()
                .on_tile(pos)
                .iter()
                .filter(|&&e| {
                    world
                        .get::<ss13_mapload::DmItem>(e)
                        .is_some_and(|i| is_under(&i.path, REAGENT_CONTAINER))
                })
                .count();
            if products >= MAX_FLOOR_PRODUCTS {
                return;
            }
            default_request(world, net, me, plumber.cap);
        }
        Kind::IvDrip => {
            if world
                .get::<crate::medical::blood::IvDrip>(me)
                .is_some_and(|d| d.injecting)
            {
                default_request(world, net, me, plumber.cap);
            }
        }
        _ => default_request(world, net, me, plumber.cap),
    }
}

/// `buffered/send_request`: the chamber's acidic or alkaline buffer beaker asks for its
/// buffer. The beaker stands in for the chamber's holder while the request runs.
pub(super) fn buffered_request(world: &mut World, net: &Net, me: Entity, acidic: bool) {
    let chem = world.resource::<Chem>().0.clone();
    let Some(id) = chem.id(if acidic { ACIDIC_BUFFER } else { BASIC_BUFFER }) else {
        return;
    };
    let Some(Kind::ReactionChamber { ph: Some(ph), .. }) = kind(world, me).cloned() else {
        return;
    };
    let beaker = if acidic { ph.acid } else { ph.base };
    if beaker.holder_full() {
        return;
    }
    let Some(main) = take_holder(world, me) else {
        return;
    };
    if let Some(mut r) = world.get_mut::<Reagents>(me) {
        r.0 = beaker;
    }
    process_request(world, net, me, MACHINE_REAGENT_TRANSFER, Some(id));
    let beaker = take_holder(world, me);
    put_holder(world, me, main);
    if let Some(beaker) = beaker
        && let Some(mut p) = world.get_mut::<Plumber>(me)
        && let Kind::ReactionChamber { ph: Some(ph), .. } = &mut p.kind
    {
        if acidic {
            ph.acid = beaker;
        } else {
            ph.base = beaker;
        }
    }
}
