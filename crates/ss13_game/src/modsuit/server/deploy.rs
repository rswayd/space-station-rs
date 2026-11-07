use super::*;

/// tg `deploy(user, part, instant)`. True when the part went on.
pub fn deploy(
    world: &mut World,
    control: Entity,
    index: usize,
    user: Option<Entity>,
    instant: bool,
) -> bool {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return false;
    };
    let Some(part) = suit.parts.get(index).cloned() else {
        return false;
    };
    let Some(wearer) = suit.wearer.filter(|&w| world.get_entity(w).is_ok()) else {
        buzz(world, control);
        return false;
    };
    if world.get::<CarriedBy>(part.item).is_some() {
        if user.is_some() {
            refuse(world, control, user, "уже развёрнуто!");
        }
        return false;
    }
    let Some(inventory) = world.get::<Inventory>(wearer).cloned() else {
        return false;
    };
    // The worn item under it (`CAN_BE_OVERSLOT`) goes inside the part.
    let mut overslot = None;
    if let Some(worn) = inventory.slot(part.slot)
        && world
            .get::<DmItem>(worn)
            .is_some_and(|i| i.item_flags & CAN_BE_OVERSLOT != 0)
        && !crate::inventory::no_drop(world, worn)
    {
        world
            .entity_mut(worn)
            .remove::<CarriedBy>()
            .insert(InSuit(control));
        overslot = Some(worn);
    }
    let inventory = world.get::<Inventory>(wearer).cloned().unwrap_or_default();
    if crate::inventory::can_equip(world, wearer, &inventory, part.item, part.slot) {
        world
            .entity_mut(part.item)
            .insert((CarriedBy(wearer), NoDrop));
        *world
            .get_mut::<Inventory>(wearer)
            .expect("checked above")
            .slot_mut(part.slot) = Some(part.item);
        if let Some(mut s) = world.get_mut::<ModSuit>(control) {
            s.parts[index].overslot = overslot;
        }
        if user.is_some() {
            hiss(world, control, wearer, &part, true);
        }
        if !suit.active || part.sealed {
            return true;
        }
        if instant {
            seal_part(world, control, index, true);
        } else {
            queue_steps(
                world,
                control,
                [Step::Seal {
                    part: index,
                    seal: true,
                    retract: false,
                }],
            );
        }
        return true;
    }
    if let Some(worn) = overslot {
        rewear(world, wearer, worn, part.slot);
    }
    if user.is_some() {
        refuse(world, control, user, "часть тела занята!");
    }
    false
}

/// An overslotted item back into its slot, else at the wearer's feet.
pub(super) fn rewear(world: &mut World, wearer: Entity, item: Entity, slot: InventorySlot) {
    world.entity_mut(item).remove::<InSuit>();
    let inventory = world.get::<Inventory>(wearer).cloned().unwrap_or_default();
    if world.get_entity(wearer).is_ok()
        && crate::inventory::can_equip(world, wearer, &inventory, item, slot)
    {
        world.entity_mut(item).insert(CarriedBy(wearer));
        if let Some(mut inv) = world.get_mut::<Inventory>(wearer) {
            *inv.slot_mut(slot) = Some(item);
        }
    } else if let Some(pos) = world.get::<TilePos>(wearer).copied() {
        crate::storage::take_out(world, item, pos);
    }
}

/// The "deploys/retracts with a mechanical hiss" message and sound.
pub(super) fn hiss(
    world: &mut World,
    control: Entity,
    wearer: Entity,
    part: &ModPart,
    deployed: bool,
) {
    let part_name = name(world, part.item);
    let wearer_name = name(world, wearer);
    let (own, others) = if deployed {
        (
            format!("{part_name} разворачивается с механическим шипением."),
            format!("{part_name} {wearer_name} разворачивается с механическим шипением."),
        )
    } else {
        (
            format!(
                "{part_name} убирается в {} с механическим шипением.",
                name(world, control)
            ),
            format!("{part_name} {wearer_name} убирается с механическим шипением."),
        )
    };
    if let Some(pos) = world.get::<TilePos>(wearer).copied() {
        crate::security::show(
            world,
            pos,
            crate::health::DEFAULT_MESSAGE_RANGE,
            ChatChannel::Local,
            (Span::Notice, others),
            &[(wearer, Span::Notice, own)],
        );
    }
    sound_at(world, control, MECHMOVE, 25.0, SHORT_RANGE_SOUND_EXTRARANGE);
}

/// tg `retract(user, part, instant)`. True when it retracts (now or after unsealing).
pub fn retract(
    world: &mut World,
    control: Entity,
    index: usize,
    user: Option<Entity>,
    instant: bool,
) -> bool {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return false;
    };
    let Some(part) = suit.parts.get(index) else {
        return false;
    };
    if world.get::<CarriedBy>(part.item).is_none() {
        if user.is_some() {
            refuse(world, control, user, "уже свёрнуто!");
        }
        return false;
    }
    if suit.active && part.sealed {
        if !instant {
            queue_steps(
                world,
                control,
                [Step::Seal {
                    part: index,
                    seal: false,
                    retract: true,
                }],
            );
            return true;
        }
        seal_part(world, control, index, false);
    }
    do_retract(world, control, index, user);
    true
}

/// The retraction itself: the part back into the unit, the overslotted item back on.
pub(super) fn do_retract(world: &mut World, control: Entity, index: usize, user: Option<Entity>) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    let Some(part) = suit.parts.get(index).cloned() else {
        return;
    };
    let Some(wearer) = world.get::<CarriedBy>(part.item).map(|c| c.0) else {
        return;
    };
    world.entity_mut(part.item).remove::<(CarriedBy, NoDrop)>();
    if let Some(worn) = part.overslot {
        if let Some(mut s) = world.get_mut::<ModSuit>(control) {
            s.parts[index].overslot = None;
        }
        if world.get_entity(worn).is_ok() {
            rewear(world, wearer, worn, part.slot);
        }
    }
    crate::inventory::refresh_look(world, wearer);
    if user.is_some() {
        hiss(world, control, wearer, &part, false);
    }
}

/// tg `quick_deploy`: every part out, or every part in when all are out.
pub fn quick_deploy(world: &mut World, control: Entity, user: Entity) -> bool {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return false;
    };
    if suit.activating || !suit.steps.is_empty() {
        refuse(world, control, Some(user), "идёт герметизация!");
        return false;
    }
    let Some(wearer) = suit.wearer else {
        refuse(world, control, Some(user), "не надет!");
        return false;
    };
    let out = suit
        .parts
        .iter()
        .filter(|p| world.get::<CarriedBy>(p.item).is_some())
        .count();
    let deploying = out == 0;
    let text = if deploying {
        "разворачивает"
    } else {
        "сворачивает"
    };
    let (own, others) = (
        format!(
            "{} {text} свои части с механическим шипением.",
            name(world, control)
        ),
        format!(
            "{} {} {text} свои части с механическим шипением.",
            name(world, control),
            name(world, wearer)
        ),
    );
    if let Some(pos) = world.get::<TilePos>(wearer).copied() {
        crate::security::show(
            world,
            pos,
            crate::health::DEFAULT_MESSAGE_RANGE,
            ChatChannel::Local,
            (Span::Notice, others),
            &[(wearer, Span::Notice, own)],
        );
    }
    sound_at(world, control, MECHMOVE, 25.0, SHORT_RANGE_SOUND_EXTRARANGE);
    for i in 0..suit.parts.len() {
        let is_out = world.get::<CarriedBy>(suit.parts[i].item).is_some();
        if deploying && !is_out {
            deploy(world, control, i, None, false);
        } else if !deploying && is_out {
            retract(world, control, i, None, false);
        }
    }
    true
}

/// tg `seal_part(part, is_sealed)`.
pub fn seal_part(world: &mut World, control: Entity, index: usize, sealed: bool) {
    let Some(part) = world
        .get::<ModSuit>(control)
        .and_then(|s| s.parts.get(index).cloned())
    else {
        return;
    };
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        s.parts[index].sealed = sealed;
    }
    if let Some(mut c) = world.get_mut::<DmClothing>(part.item) {
        if sealed {
            c.clothing_flags |= part.def.sealed_clothing;
            c.flags_inv |= part.def.sealed_inv;
            c.flags_cover |= part.def.sealed_cover;
            c.heat_protection = part.heat_protection;
            c.cold_protection = part.cold_protection;
        } else {
            c.clothing_flags &= !part.def.sealed_clothing;
            c.flags_inv &= !part.def.sealed_inv;
            c.flags_cover &= !part.def.sealed_cover;
            c.heat_protection = 0;
            c.cold_protection = 0;
        }
    }
    update_icons(world, control);
    module_part_activation(world, control, sealed);
}

/// The end of `seal_part` and `control_activation`: modules whose required parts became all
/// sealed activate their parts, those that lost one deactivate.
pub(super) fn module_part_activation(world: &mut World, control: Entity, sealing: bool) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    let theme = theme_of(world, &suit);
    let sealed = suit.part_slots(&theme, true);
    for &module in &suit.modules {
        let (Some(def), Some(state)) = (
            modules::def_of(world, module),
            world.get::<ModModule>(module).cloned(),
        ) else {
            continue;
        };
        let has = suit.active && modules::has_required_parts(&def, sealed);
        if sealing {
            if !state.part_activated && has {
                modules::on_part_activation(world, control, module);
                if let Some(mut m) = world.get_mut::<ModModule>(module) {
                    m.part_activated = true;
                }
            }
        } else if state.part_activated && !has {
            modules::on_part_deactivation(world, control, module);
            if let Some(mut m) = world.get_mut::<ModModule>(module) {
                m.part_activated = false;
            }
            if state.active && def.allow_flags & modules::MODULE_ALLOW_INACTIVE == 0 {
                modules::deactivate(world, control, module, false);
            }
        }
    }
}

/// tg `control_activation(is_on)`.
pub fn control_activation(world: &mut World, control: Entity, on: bool) {
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        s.active = on;
    }
    if on {
        module_part_activation(world, control, true);
    } else {
        let modules_list = world
            .get::<ModSuit>(control)
            .map(|s| s.modules.clone())
            .unwrap_or_default();
        for module in modules_list {
            if world
                .get::<ModModule>(module)
                .is_some_and(|m| m.part_activated)
            {
                modules::on_part_deactivation(world, control, module);
                if let Some(mut m) = world.get_mut::<ModModule>(module) {
                    m.part_activated = false;
                }
            }
        }
    }
    update_icons(world, control);
}

pub(super) fn queue_steps(
    world: &mut World,
    control: Entity,
    steps: impl IntoIterator<Item = Step>,
) {
    let now = now(world);
    let step_secs = world
        .get::<ModSuit>(control)
        .map(|s| theme_of(world, s).activation_step_time / 10.0)
        .unwrap_or(1.0);
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        if s.steps.is_empty() {
            s.step_at = now + f64::from(step_secs);
        }
        s.steps.extend(steps);
    }
}

/// tg `toggle_activate(user, force_deactivate)`: starts the sealing sequence. True when it
/// started.
pub fn toggle_activate(
    world: &mut World,
    control: Entity,
    user: Option<Entity>,
    force: bool,
) -> bool {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return false;
    };
    let Some(wearer) = suit.wearer else {
        if !force {
            refuse(world, control, user, "не надет!");
        }
        return false;
    };
    if !force {
        if suit.locked && !suit.active && !user.is_some_and(|u| allowed(world, control, u)) {
            refuse(world, control, user, "доступ запрещён!");
            return false;
        }
        if charge(world, control) <= 0.0 {
            refuse(world, control, user, "нет источника питания!");
            return false;
        }
        if suit.open {
            refuse(world, control, user, "панель открыта!");
            return false;
        }
    }
    if suit.activating || !suit.steps.is_empty() {
        if !force {
            let text = if suit.active {
                "уже отключается!"
            } else {
                "уже запускается!"
            };
            refuse(world, control, user, text);
        }
        return false;
    }
    for &module in &suit.modules {
        let keeps = modules::def_of(world, module)
            .is_some_and(|d| d.allow_flags & modules::MODULE_ALLOW_INACTIVE != 0);
        if world.get::<ModModule>(module).is_some_and(|m| m.active) && !keeps {
            modules::deactivate(world, control, module, false);
        }
    }
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        s.activating = true;
    }
    let text = if suit.active {
        "MODsuit отключается."
    } else {
        "MODsuit запускается."
    };
    tell_mob(world, wearer, Span::Notice, text);
    let deployed: Vec<usize> = (0..suit.parts.len())
        .filter(|&i| world.get::<CarriedBy>(suit.parts[i].item).is_some())
        .collect();
    let seal = !suit.active;
    let seals = deployed.into_iter().map(move |part| Step::Seal {
        part,
        seal,
        retract: false,
    });
    let steps: Vec<Step> = if suit.active {
        std::iter::once(Step::Control { on: false })
            .chain(seals)
            .collect()
    } else {
        seals.chain([Step::Control { on: true }]).collect()
    };
    queue_steps(world, control, steps);
    true
}

/// The next due step of the sequence (tg's do_after finishing): stops when the wearer is gone
/// (`extra_checks = get_wearer`).
pub(super) fn run_steps(world: &mut World, control: Entity) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    if suit.steps.is_empty() || now(world) < suit.step_at {
        return;
    }
    let Some(wearer) = suit.wearer else {
        if let Some(mut s) = world.get_mut::<ModSuit>(control) {
            s.steps.clear();
            s.activating = false;
        }
        return;
    };
    let step = world
        .get_mut::<ModSuit>(control)
        .and_then(|mut s| s.steps.pop_front())
        .expect("checked above");
    match step {
        Step::Seal {
            part,
            seal,
            retract,
        } => {
            if let Some(p) = suit.parts.get(part).cloned() {
                let message = if seal {
                    &p.def.sealed_message
                } else {
                    &p.def.unsealed_message
                };
                let text = format!("{} {message}.", name(world, p.item));
                tell_mob(world, wearer, Span::Notice, text);
                sound_at(world, control, MECHMOVE, 25.0, SHORT_RANGE_SOUND_EXTRARANGE);
                seal_part(world, control, part, seal);
                if retract {
                    do_retract(world, control, part, Some(wearer));
                }
            }
        }
        Step::Control { on } => {
            control_activation(world, control, on);
            if on {
                sound_at(
                    world,
                    control,
                    SYNTH_YES,
                    50.0,
                    SHORT_RANGE_SOUND_EXTRARANGE,
                );
                // BandaStation: the theme's `activation_sound` to the wearer only.
                let sound = theme_of(world, &suit).activation_sound;
                if !sound.is_empty() {
                    let pos = crate::chem::map_pos(world, control);
                    crate::hallucination::play_local(world, wearer, pos, &sound, 50.0, false);
                }
            } else {
                sound_at(world, control, SYNTH_NO, 50.0, SHORT_RANGE_SOUND_EXTRARANGE);
                tell_mob(
                    world,
                    wearer,
                    Span::Notice,
                    "Блок управления отключён. Модули недоступны.",
                );
            }
        }
    }
    let Some(s) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    if !s.steps.is_empty() {
        let step_secs = f64::from(theme_of(world, &s).activation_step_time / 10.0);
        let now = now(world);
        if let Some(mut s) = world.get_mut::<ModSuit>(control) {
            s.step_at = now + step_secs;
        }
    } else if s.activating {
        if let Some(mut s) = world.get_mut::<ModSuit>(control) {
            s.activating = false;
        }
        let who = name(world, wearer);
        let text = if s.active {
            format!("Системы запущены. Части загерметизированы. Добро пожаловать, {who}.")
        } else {
            format!("Системы отключены. Части разгерметизированы. До свидания, {who}.")
        };
        tell_mob(world, wearer, Span::Notice, text);
    }
}

/// Every unit, every tick: its wearer and holder, its sequence and tool delays, power
/// (`process`), the wearer's slowdown, whether it can be taken off and its window data.
// Cost: one pass over the control units (a few dozen per round at most) per tick.
pub fn tick(world: &mut World) {
    let mut q = world.query_filtered::<Entity, With<ModSuit>>();
    let suits: Vec<Entity> = q.iter(world).collect();
    let dt = world.resource::<Time>().delta_secs_f64();
    for control in suits {
        track_wearer(world, control);
        run_steps(world, control);
        run_pending(world, control);
        process(world, control, dt);
        update_wearer(world, control);
        if modules::refresh_overlays(world, control)
            && let Some(mob) = world.get::<ModSuit>(control).and_then(|s| s.wearer)
        {
            crate::inventory::refresh_look(world, mob);
        }
        refresh_view(world, control);
    }
}

/// tg `visual_equipped`/`dropped`/`Moved`: the wearer is who wears it in its theme's slot.
pub(super) fn track_wearer(world: &mut World, control: Entity) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    let holder = world.get::<CarriedBy>(control).map(|c| c.0);
    let slot_flags = theme_of(world, &suit).slot_flags;
    let wearer = holder.filter(|&mob| {
        world
            .get::<Inventory>(mob)
            .and_then(|i| i.slot_of(control))
            .is_some_and(|slot| slot_bit(slot) & slot_flags != 0)
    });
    if wearer != suit.wearer {
        if suit.wearer.is_some() {
            clean_up(world, control);
        }
        if let Some(mob) = wearer {
            set_wearer(world, control, mob);
        }
    }
    if holder != suit.holder {
        if let Some(mut s) = world.get_mut::<ModSuit>(control) {
            s.holder = holder;
        }
        refresh_buttons(world, suit.holder);
        refresh_buttons(world, holder);
    } else if wearer != suit.wearer {
        refresh_buttons(world, holder);
    }
}

/// A tool delay that ran out: done if the mob stayed, still holds what it held and still
/// reaches the unit (tg `use_tool`/`do_after` checks).
pub(super) fn run_pending(world: &mut World, control: Entity) {
    let Some(pending) = world.get::<ModSuit>(control).and_then(|s| s.pending) else {
        return;
    };
    if now(world) < pending.until {
        return;
    }
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        s.pending = None;
    }
    let mob = pending.mob;
    let stayed = world.get::<TilePos>(mob) == Some(&pending.mob_pos)
        && world.get::<Inventory>(mob).map(Inventory::active_item) == Some(pending.held)
        && world.get::<Stat>(mob) == Some(&Stat::Conscious);
    let reach = world.get::<CarriedBy>(control).is_some_and(|c| c.0 == mob)
        || crate::storage::in_reach(world, mob, control);
    if !stayed || !reach {
        tell(world, pending.client, Span::Warning, "прервано!");
        return;
    }
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    match pending.kind {
        PendingKind::Cover => {
            if suit.active || suit.activating {
                tell(world, pending.client, Span::Warning, "блок активен!");
                return;
            }
            if let Some(mut s) = world.get_mut::<ModSuit>(control) {
                s.open = !s.open;
            }
            let text = if suit.open {
                "крышка закрыта"
            } else {
                "крышка открыта"
            };
            tell(world, pending.client, Span::Notice, text);
        }
        PendingKind::RemoveCore => {
            let Some(core) = suit.core.filter(|_| suit.open) else {
                tell(world, pending.client, Span::Warning, "прервано!");
                return;
            };
            if let Some(mut s) = world.get_mut::<ModSuit>(control) {
                s.core = None;
            }
            if let Some(pos) = drop_pos(world, control) {
                place_on(world, core, pos);
            }
            tell(world, pending.client, Span::Notice, "ядро извлечено");
        }
        PendingKind::RemoveCell => {
            let Some(cell) = cell_of(world, control).filter(|_| suit.open) else {
                tell(
                    world,
                    pending.client,
                    Span::Warning,
                    "нет элемента питания!",
                );
                return;
            };
            if let Some(core) = suit.core
                && let Some(mut c) = world.get_mut::<ModCore>(core)
            {
                c.cell = None;
            }
            world
                .entity_mut(cell)
                .remove::<StoredIn>()
                .insert(Networked);
            if let Some(inventory) = world.get::<Inventory>(mob).cloned() {
                crate::storage::put_in_hands(world, mob, &inventory, cell);
            }
            sound_at(world, control, CLICK, 50.0, SILENCED_SOUND_EXTRARANGE);
            tell(
                world,
                pending.client,
                Span::Notice,
                "элемент питания извлечён",
            );
        }
    }
}

/// tg `process(seconds_per_tick)`: an active unit without charge shuts down; otherwise it pays
/// `charge_drain` and each module runs.
pub(super) fn process(world: &mut World, control: Entity, dt: f64) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    if !suit.active {
        return;
    }
    if charge(world, control) <= 0.0 && !suit.activating {
        tell_opt(world, suit.wearer, Span::Warning, "нет питания!");
        toggle_activate(world, control, suit.wearer, true);
        return;
    }
    let drain = theme_of(world, &suit).charge_drain;
    subtract_charge(world, control, drain * dt);
    for &module in &suit.modules {
        modules::on_process(world, control, module, dt);
    }
}

/// The wearer's slowdown (tg `update_speed`: the total split over every part, unsealed parts
/// never faster), and whether the unit comes off (`can_mob_unequip`).
pub(super) fn update_wearer(world: &mut World, control: Entity) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    let locked_on = suit.wearer.is_some()
        && (suit.active
            || suit.activating
            || suit
                .parts
                .iter()
                .any(|p| world.get::<CarriedBy>(p.item).is_some()));
    let has = world.get::<NoDrop>(control).is_some();
    if locked_on && !has {
        world.entity_mut(control).insert(NoDrop);
    } else if !locked_on && has {
        world.entity_mut(control).remove::<NoDrop>();
    }
    let Some(wearer) = suit.wearer.filter(|&w| world.get_entity(w).is_ok()) else {
        return;
    };
    let theme = theme_of(world, &suit);
    let mut total = theme.slowdown_deployed;
    for &m in &suit.modules {
        total += modules::slowdown(world, m);
    }
    let share = total / (suit.parts.len() + 1) as f32;
    let mut slowdown = share;
    // `/datum/movespeed_modifier/jetpack/full_speed` (`movetypes = FLOATING`).
    if world
        .get::<modules::Jetpack>(wearer)
        .is_some_and(|j| suit.modules.contains(&j.module))
        && world.get::<crate::spacemove::Weightless>(wearer).is_some()
    {
        slowdown += modules::JETPACK_FULL_SPEED;
    }
    for p in &suit.parts {
        if world
            .get::<CarriedBy>(p.item)
            .is_some_and(|c| c.0 == wearer)
        {
            slowdown += if p.sealed { share } else { share.max(0.0) };
        }
    }
    let new = SuitSlowdown(slowdown);
    match world.get_mut::<SuitSlowdown>(wearer) {
        Some(mut old) => {
            old.set_if_neq(new);
        }
        None => {
            world.entity_mut(wearer).insert(new);
        }
    }
}
