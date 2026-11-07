use super::*;

/// A duct of `layer` or a plumbing machine of that layer on `pos` (`ducting_layer_check`).
pub(super) fn layer_taken(world: &World, pos: TilePos, layer: u8, except: Option<Entity>) -> bool {
    world
        .resource::<SpatialIndex>()
        .on_tile(pos)
        .iter()
        .filter(|&&e| Some(e) != except)
        .any(|&e| {
            world.get::<Duct>(e).is_some_and(|d| d.layer & layer != 0)
                || (anchored(world, e)
                    && world
                        .get::<Plumber>(e)
                        .is_some_and(|p| p.ports.iter().any(|p| p.layer & layer != 0)))
        })
}

/// `stack/ducts/attack_self`: the next layer (shortcut: a cycle instead of the two lists).
pub fn attack_self(world: &mut World, client: Entity, item: Entity) -> bool {
    let Some(layer) = duct_stack_layer(world, item) else {
        return false;
    };
    let next = if layer >= 1 << 4 { 1 } else { layer << 1 };
    world.entity_mut(item).insert(DuctStack { layer: next });
    tell(
        world,
        client,
        Span::Notice,
        format!("Слой дукта: {}.", layer_index(next)),
    );
    true
}

/// Duct stacks, wrenches on ducts and plumbing machines, screwdrivers on buffers. True when
/// handled.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
    modifiers: ClickModifiers,
) -> bool {
    let _ = modifiers;
    if let Some(layer) = duct_stack_layer(world, held) {
        // `stack/ducts/interact_with_atom`: a duct back into the stack.
        if world.get::<Duct>(target).is_some() {
            let amount = world.get::<Stack>(held).map_or(0, |s| s.amount);
            if amount >= DUCT_STACK_MAX {
                tell(world, client, Span::Warning, "Стопка заполнена!");
                return true;
            }
            world.despawn(target);
            if let Some(mut s) = world.get_mut::<Stack>(held) {
                s.amount += 1;
            }
            return true;
        }
        // `check_attach_turf`.
        let open = world.get::<Turf>(target).is_some()
            && path_of(world, target).is_some_and(|p| is_under(&p, "/turf/open"));
        if !open {
            return false;
        }
        let Some(&pos) = world.get::<TilePos>(target) else {
            return true;
        };
        if layer_taken(world, pos, layer, None) {
            tell(
                world,
                client,
                Span::Warning,
                "Здесь уже есть дукт или машина!",
            );
            return true;
        }
        if !use_stack(world, held, 1) {
            return true;
        }
        if let Some(duct) = crate::construction::spawn_obj(world, DUCT, pos) {
            world
                .entity_mut(duct)
                .insert((Duct { layer }, PlumbingLayer(layer_index(layer))));
        }
        sound(world, target, CLICK, 50.0);
        return true;
    }
    let Some((tool, _)) = tool_of(world, held) else {
        // `grinder_chemical/item_interaction`: a non-tool item in hand is ground at once.
        if matches!(kind(world, target), Some(Kind::Grinder)) {
            let combat = world
                .get::<crate::combat::CombatMode>(mob)
                .is_some_and(|c| c.0);
            if combat {
                return false;
            }
            if !anchored(world, target) {
                tell(world, client, Span::Warning, "Сначала прикрутите его.");
                return true;
            }
            let name = crate::chem::name_of(world, held);
            crate::storage::put_inside(world, held, target);
            blend(world, target, held);
            tell(
                world,
                client,
                Span::Notice,
                format!("Вы перемалываете {name}."),
            );
            return true;
        }
        return false;
    };
    // `duct/wrench_act`: `deconstruct` into a stack of one.
    if tool == ToolKind::Wrench
        && let Some(duct) = world.get::<Duct>(target).copied()
    {
        let Some(&pos) = world.get::<TilePos>(target) else {
            return true;
        };
        sound(world, target, RATCHET, 50.0);
        world.despawn(target);
        if let Some(stack) = spawn_stack(world, DUCT_STACK, pos, 1) {
            world
                .entity_mut(stack)
                .insert(DuctStack { layer: duct.layer });
        }
        tell(world, client, Span::Notice, "Вы откручиваете дукт.");
        return true;
    }
    if world.get::<Plumber>(target).is_none() {
        return false;
    }
    let combat = world
        .get::<crate::combat::CombatMode>(mob)
        .is_some_and(|c| c.0);
    match tool {
        // `plumbing/wrench_act` -> `default_unfasten_wrench`.
        ToolKind::Wrench if !combat => {
            let was = anchored(world, target);
            if !was
                && let Some(&pos) = world.get::<TilePos>(target)
                && world.get::<Plumber>(target).is_some_and(|p| {
                    p.ports
                        .iter()
                        .any(|p| layer_taken(world, pos, p.layer, Some(target)))
                })
            {
                tell(
                    world,
                    client,
                    Span::Warning,
                    "Здесь уже есть дукт или машина!",
                );
                return true;
            }
            let actor = Actor {
                client,
                mob,
                item: Some(held),
            };
            sound(world, target, RATCHET, 50.0);
            do_after(
                world,
                actor,
                target,
                UNFASTEN_DS,
                0,
                Job::Unfasten { anchored: was },
            );
            reset_on_unfasten(world, target);
            true
        }
        // `buffer/screwdriver_act`: links every buffer reached through adjacent buffers.
        ToolKind::Screwdriver if matches!(kind(world, target), Some(Kind::Buffer { .. })) => {
            let group = buffer_group(world, target);
            for &b in &group {
                if let Some(mut p) = world.get_mut::<Plumber>(b)
                    && let Kind::Buffer { links, .. } = &mut p.kind
                {
                    *links = group.clone();
                }
            }
            tell(
                world,
                client,
                Span::Notice,
                format!("Связано буферов: {}.", group.len()),
            );
            true
        }
        // `receiver/multitool_act`: into the multitool's buffer.
        ToolKind::Multitool if matches!(kind(world, target), Some(Kind::Receiver { .. })) => {
            crate::tools::set_multitool_buffer(world, held, target);
            tell(world, client, Span::Notice, "Сохранено в буфер мультитула.");
            true
        }
        // `sender/multitool_act`: linked to the buffered receiver.
        ToolKind::Multitool if matches!(kind(world, target), Some(Kind::Sender { .. })) => {
            let receiver = crate::tools::multitool_buffer(world, held)
                .filter(|&r| matches!(kind(world, r), Some(Kind::Receiver { .. })));
            let Some(receiver) = receiver else {
                tell(world, client, Span::Warning, "Неверный буфер.");
                return true;
            };
            link_sender(world, target, receiver);
            tell(world, client, Span::Notice, "Маяк связан с приёмником.");
            true
        }
        _ => false,
    }
}

/// `sender/set_teleport_target` (after `lose_teleport_target`).
pub(super) fn link_sender(world: &mut World, sender: Entity, receiver: Entity) {
    if let Some(Kind::Sender {
        target: Some(old), ..
    }) = kind(world, sender).cloned()
        && let Some(mut p) = world.get_mut::<Plumber>(old)
        && let Kind::Receiver { senders, .. } = &mut p.kind
    {
        senders.retain(|&s| s != sender);
    }
    set_kind(
        world,
        sender,
        Kind::Sender {
            target: Some(receiver),
        },
    );
    if let Some(mut p) = world.get_mut::<Plumber>(receiver)
        && let Kind::Receiver { senders, .. } = &mut p.kind
        && !senders.contains(&sender)
    {
        senders.push(sender);
    }
}

/// `acclimator/wrench_act`: back to filling.
pub(super) fn reset_on_unfasten(world: &mut World, target: Entity) {
    if let Some(mut p) = world.get_mut::<Plumber>(target)
        && let Kind::Acclimator { state, .. } = &mut p.kind
    {
        *state = Acclimate::Filling;
    }
}

pub(super) fn buffer_group(world: &World, start: Entity) -> Vec<Entity> {
    let mut group = vec![start];
    let mut i = 0;
    while i < group.len() {
        let Some(&pos) = world.get::<TilePos>(group[i]) else {
            i += 1;
            continue;
        };
        for d in Dir::CARDINALS {
            for &e in world.resource::<SpatialIndex>().on_tile(pos.step(d)) {
                if matches!(kind(world, e), Some(Kind::Buffer { .. })) && !group.contains(&e) {
                    group.push(e);
                }
            }
        }
        i += 1;
    }
    group
}

/// `simple_rotation` of an unanchored plumbing machine (alt-click, counterclockwise). True when
/// `target` is one.
pub fn rotate_machine(world: &mut World, target: Entity) -> bool {
    if world.get::<Plumber>(target).is_none() || anchored(world, target) {
        return false;
    }
    let dir = dir_of(world, target);
    let ccw = CW[(cw_index(dir) + 3) % 4];
    if let Some(facing) = Dir::from_bits(ccw) {
        world.entity_mut(target).insert(Facing(facing));
    }
    true
}

/// An empty hand on a configurable machine: its settings menu (tg `ui_interact`).
pub fn interact(world: &mut World, client: Entity, mob: Entity, target: Entity) -> bool {
    let Some(k) = kind(world, target).cloned() else {
        return false;
    };
    let choices = match k {
        Kind::Acclimator { .. } => vec![
            Choice::Number(Setting::AcclimatorTarget),
            Choice::Number(Setting::AcclimatorVolume),
        ],
        Kind::Filter { left, right } => {
            let mut c = vec![Choice::FilterAdd(true), Choice::FilterAdd(false)];
            c.extend(left.iter().map(|&id| Choice::FilterRemove(true, id)));
            c.extend(right.iter().map(|&id| Choice::FilterRemove(false, id)));
            c
        }
        Kind::Splitter { .. } => vec![
            Choice::Number(Setting::Splitter(S)),
            Choice::Number(Setting::Splitter(E)),
            Choice::Number(Setting::Splitter(W)),
        ],
        Kind::Buffer { .. } => {
            // `buffer/attack_hand`: `tgui_input_number` of the activation volume.
            ask(
                world,
                client,
                mob,
                target,
                "Порог активации",
                Step::Number(Setting::BufferActivation),
            );
            return true;
        }
        Kind::ReactionChamber {
            required,
            catalysts,
            ph,
            ..
        } => {
            let mut c = vec![Choice::ChamberAdd, Choice::Number(Setting::ChamberTarget)];
            if ph.is_some() {
                c.push(Choice::Number(Setting::AcidicLimit));
                c.push(Choice::Number(Setting::AlkalineLimit));
            }
            for &(id, _) in required.iter().chain(&catalysts) {
                c.push(Choice::ChamberCatalyst(id));
                c.push(Choice::ChamberRemove(id));
            }
            c
        }
        Kind::Synthesizer { .. } => {
            vec![Choice::SynthReagent, Choice::Number(Setting::SynthAmount)]
        }
        Kind::PillPress { .. } => vec![
            Choice::PressPackage(Package::Pill),
            Choice::PressPackage(Package::Patch),
            Choice::PressPackage(Package::Bottle),
            Choice::Number(Setting::PressVolume),
            Choice::Number(Setting::PillDuration),
            Choice::PressName,
        ],
        Kind::Bottler { .. } => {
            ask(
                world,
                client,
                mob,
                target,
                "Объём заполнения",
                Step::Number(Setting::BottlerAmount),
            );
            return true;
        }
        Kind::Disposer { .. } => vec![
            Choice::DisposerToggle,
            Choice::Number(Setting::DisposerRate),
        ],
        _ => return false,
    };
    let title = crate::chem::name_of(world, target);
    ask(world, client, mob, target, &title, Step::Menu(choices));
    true
}

/// `ui_status`: the user still next to the machine, conscious.
pub(super) fn can_use(world: &World, mob: Entity, machine: Entity) -> bool {
    let near = match (world.get::<TilePos>(mob), world.get::<TilePos>(machine)) {
        (Some(a), Some(b)) => a.chebyshev(*b).is_some_and(|d| d <= 1),
        _ => false,
    };
    near && world.get::<crate::health::Stat>(mob) == Some(&crate::health::Stat::Conscious)
}

/// Examine lines of a duct, duct stack or plumbing machine (`duct/examine`,
/// `stack/ducts/examine`, `plumbing/examine`, the machines' own `examine`).
pub fn examine_lines(world: &World, target: Entity, lines: &mut Vec<crate::examine::Line>) {
    let mut note = |text: String| lines.push((Span::Notice, text));
    if let Some(d) = world.get::<Duct>(target) {
        note(format!("Слой дукта: {}.", layer_index(d.layer)));
        note("Его можно открутить гаечным ключом.".to_owned());
        return;
    }
    if let Some(d) = world.get::<DuctStack>(target) {
        note(format!(
            "Слой: {}. Используйте в руке, чтобы сменить.",
            layer_index(d.layer)
        ));
        note("Кликните по полу, чтобы проложить дукт.".to_owned());
        return;
    }
    let Some(p) = world.get::<Plumber>(target) else {
        return;
    };
    if let Some(h) = holder(world, target) {
        note(format!(
            "Дисплей максимального объёма: {} u. Содержит:",
            crate::chem::dm_num(h.maximum_volume)
        ));
        if h.reagents.is_empty() {
            note("Ничего.".to_owned());
        }
        for r in &h.reagents {
            note(format!(
                "{} u {}",
                crate::chem::dm_num(round_to(r.volume, CHEMICAL_VOLUME_ROUNDING)),
                reagent_name(world, r.id)
            ));
        }
    }
    if anchored(world, target) {
        note("Прикручено на месте.".to_owned());
    } else {
        lines.push((
            Span::Warning,
            "Нужно прикрутить гаечным ключом, чтобы заработало.".to_owned(),
        ));
    }
    let mut note = |text: String| lines.push((Span::Notice, text));
    if let Some(cap) = p.cap {
        note(format!("Принимает не больше {cap} разных реагентов."));
    }
    match &p.kind {
        Kind::Buffer {
            activation, mode, ..
        } => {
            note(format!("Срабатывает при {activation} u."));
            note(
                match mode {
                    BufferMode::Unready => "Набирает реагенты.",
                    BufferMode::Idle => "Ждёт другие буферы.",
                    BufferMode::Ready => "Выпускает реагенты.",
                }
                .to_owned(),
            );
        }
        Kind::Disposer { rate, .. } => note(format!("Уничтожает {rate} u реагентов в секунду.")),
        Kind::Bottler { wanted } => note(format!("Наполняет по {wanted} u.")),
        Kind::ReactionChamber {
            ph: Some(ph),
            target: temp,
            ..
        } => note(format!(
            "Держит {temp} K и pH {}–{}.",
            ph.acidic_limit, ph.alkaline_limit
        )),
        Kind::Sender { target: None } => note("Не связан с приёмником.".to_owned()),
        _ => {}
    }
}
