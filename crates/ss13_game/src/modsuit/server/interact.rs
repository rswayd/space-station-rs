use super::*;

/// `held` used on a control unit (on the floor or in the other hand): emags, modules, cores,
/// cells, tools and IDs (tg `emag_act`, `tool_act`, `screwdriver_act`, `crowbar_act`,
/// `wrench_act`, the core's `replace_cell`). True when the click was theirs.
pub fn item_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    if charge_plasma(world, client, held, target) {
        return true;
    }
    let Some(suit) = world.get::<ModSuit>(target).cloned() else {
        return false;
    };
    let control = target;
    let user = Some(mob);
    if crate::emag::is_emag(world, held) {
        if let Some(mut s) = world.get_mut::<ModSuit>(control) {
            s.locked = !s.locked;
        }
        let text = if suit.locked {
            "доступ разблокирован"
        } else {
            "доступ заблокирован"
        };
        tell(world, client, Span::Notice, text);
        return true;
    }
    modules::init_module(world, held);
    if world.get::<ModModule>(held).is_some() {
        if !suit.open {
            refuse(world, control, user, "крышка закрыта!");
        } else {
            install(world, control, held, user);
        }
        return true;
    }
    init_core(world, held);
    if world.get::<ModCore>(held).is_some() {
        if !suit.open {
            refuse(world, control, user, "крышка закрыта!");
        } else if suit.core.is_some() {
            refuse(world, control, user, "ядро уже установлено!");
        } else {
            if let Some(slot) = world.get::<Inventory>(mob).and_then(|i| i.slot_of(held))
                && let Some(mut inv) = world.get_mut::<Inventory>(mob)
            {
                *inv.slot_mut(slot) = None;
            }
            world
                .entity_mut(held)
                .remove::<(CarriedBy, TilePos, StoredIn)>()
                .insert(InSuit(control));
            if let Some(mut s) = world.get_mut::<ModSuit>(control) {
                s.core = Some(held);
            }
            tell(world, client, Span::Notice, "ядро установлено");
            sound_at(world, control, CLICK, 50.0, SILENCED_SOUND_EXTRARANGE);
        }
        return true;
    }
    if world.get::<ItemCell>(held).is_some()
        && let Some(core) = suit.core
        && world.get::<ModCore>(core).is_some_and(|c| c.standard)
    {
        if !suit.open {
            refuse(world, control, user, "крышка закрыта!");
        } else if cell_of(world, control).is_some() {
            refuse(world, control, user, "элемент питания уже установлен!");
        } else {
            if let Some(slot) = world.get::<Inventory>(mob).and_then(|i| i.slot_of(held))
                && let Some(mut inv) = world.get_mut::<Inventory>(mob)
            {
                *inv.slot_mut(slot) = None;
            }
            crate::storage::put_inside(world, held, core);
            if let Some(mut c) = world.get_mut::<ModCore>(core) {
                c.cell = Some(held);
            }
            tell(world, client, Span::Notice, "элемент питания установлен");
            sound_at(world, control, CLICK, 50.0, SILENCED_SOUND_EXTRARANGE);
        }
        return true;
    }
    match tool_of(world, held).map(|(kind, _)| kind) {
        Some(ToolKind::Screwdriver) => {
            if suit.active || suit.activating {
                refuse(world, control, user, "блок активен!");
                return true;
            }
            let text = if suit.open {
                "закрываем крышку..."
            } else {
                "открываем крышку..."
            };
            tell(world, client, Span::Notice, text);
            start_pending(
                world,
                control,
                client,
                mob,
                Some(held),
                PendingKind::Cover,
                COVER_SECS,
            );
            return true;
        }
        Some(ToolKind::Crowbar) => {
            if !suit.open {
                refuse(world, control, user, "крышка закрыта!");
                return true;
            }
            if !allowed(world, control, mob) {
                refuse(world, control, user, "недостаточно доступа!");
                return true;
            }
            let removable: Vec<Entity> = suit
                .modules
                .iter()
                .copied()
                .filter(|&m| modules::def_of(world, m).is_some_and(|d| d.removable))
                .collect();
            if removable.is_empty() {
                refuse(world, control, user, "нет модулей!");
                return true;
            }
            let items = removable.iter().map(|&m| name(world, m)).collect();
            let title = "Извлечение модуля".to_owned();
            let prompt = ask_world(
                world,
                Ask::new(
                    client,
                    mob,
                    title,
                    "Какой модуль извлечь?",
                    PromptKind::List {
                        items,
                        default: None,
                    },
                ),
            );
            world.entity_mut(prompt).insert(ModChoice {
                control,
                kind: ChoiceKind::Remove(removable),
            });
            return true;
        }
        Some(ToolKind::Wrench) if suit.open => {
            if suit.core.is_none() {
                refuse(world, control, user, "нет ядра!");
                return true;
            }
            tell(world, client, Span::Notice, "извлекаем ядро...");
            start_pending(
                world,
                control,
                client,
                mob,
                Some(held),
                PendingKind::RemoveCore,
                CORE_SECS,
            );
            return true;
        }
        _ => {}
    }
    if suit.open && crate::inventory::get_id(world, held).is_some() {
        // `update_access`.
        if !allowed(world, control, mob) {
            refuse(world, control, user, "недостаточно доступа!");
        } else {
            let access = crate::inventory::get_id(world, held)
                .and_then(|c| world.get::<IdCard>(c))
                .map(|c| c.access.clone())
                .unwrap_or_default();
            if let Some(mut s) = world.get_mut::<ModSuit>(control) {
                s.req_access = access;
            }
            tell(world, client, Span::Notice, "доступ обновлён");
        }
        return true;
    }
    false
}

/// A click on the control unit in the other hand: with an item, [`item_interaction`]; with an
/// empty hand and the cover open, the standard core's cell comes out (tg `on_attack_hand`).
/// tg `core/plasma/charge_plasma` (also through the unit's `COMSIG_ATOM_ITEM_INTERACTION`):
/// plasma ore or sheets used on a plasma core, or a unit with one, refuel it with as few as fill
/// it up. True when it took some.
pub fn charge_plasma(world: &mut World, client: Entity, held: Entity, target: Entity) -> bool {
    let Some(path) = world.get::<DmItem>(held).map(|i| i.path.clone()) else {
        return false;
    };
    let Some(&(_, given)) = PLASMA_CHARGERS.iter().find(|(p, _)| is_under(&path, p)) else {
        return false;
    };
    let core = match world.get::<ModSuit>(target) {
        Some(suit) => suit.core,
        None => Some(target),
    };
    let Some(core) = core else {
        return false;
    };
    init_core(world, core);
    let Some((charge, max)) = world.get::<ModCore>(core).and_then(|c| c.plasma) else {
        return false;
    };
    let uses = plasma_uses(charge, max, given, crate::tools::stack_amount(world, held));
    if uses == 0 || !crate::tools::use_stack(world, held, uses) {
        return false;
    }
    if let Some(mut c) = world.get_mut::<ModCore>(core)
        && let Some((now, max)) = c.plasma.as_mut()
    {
        *now = (*now + f64::from(uses) * given).min(*max);
    }
    tell(world, client, Span::Notice, "ядро заправлено");
    true
}

/// `min(plasma.amount, ROUND_UP((max_charge_amount() - charge_amount()) / charge_given))`.
pub fn plasma_uses(charge: f64, max: f64, given: f64, amount: u32) -> u32 {
    if given <= 0.0 {
        return 0;
    }
    let needed = ((max - charge) / given).ceil().max(0.0) as u32;
    amount.min(needed)
}

pub fn hand_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    active: Option<Entity>,
    item: Entity,
) -> bool {
    let Some(suit) = world.get::<ModSuit>(item).cloned() else {
        return false;
    };
    if let Some(held) = active {
        return item_interaction(world, client, mob, held, item);
    }
    if !suit.open || !core_state(world, item).is_some_and(|c| c.standard) {
        return false;
    }
    if cell_of(world, item).is_none() {
        tell(world, client, Span::Warning, "нет элемента питания!");
        return true;
    }
    tell(world, client, Span::Notice, "извлекаем элемент питания...");
    start_pending(
        world,
        item,
        client,
        mob,
        None,
        PendingKind::RemoveCell,
        CELL_SECS,
    );
    true
}

pub(super) fn start_pending(
    world: &mut World,
    control: Entity,
    client: Entity,
    mob: Entity,
    held: Option<Entity>,
    kind: PendingKind,
    secs: f64,
) {
    let Some(&mob_pos) = world.get::<TilePos>(mob) else {
        return;
    };
    // `toolspeed` multiplies `use_tool` delays.
    let speed = held
        .and_then(|h| world.get::<ss13_mapload::DmTool>(h))
        .map_or(1.0, |t| f64::from(t.speed));
    let until = now(world) + secs * speed;
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        s.pending = Some(Pending {
            kind,
            client,
            mob,
            mob_pos,
            held,
            until,
        });
    }
}

/// A wearer's middle click with a selected module (tg `on_special_click` -> `on_select_use`).
pub(super) fn module_clicks(world: &mut World, mut cursor: Local<MessageCursor<ModuleClick>>) {
    let clicks: Vec<ModuleClick> = cursor
        .read(world.resource::<Messages<ModuleClick>>())
        .copied()
        .collect();
    for click in clicks {
        let Some(control) = worn_suit(world, click.mob) else {
            continue;
        };
        let Some(module) = world.get::<ModSuit>(control).and_then(|s| s.selected) else {
            continue;
        };
        modules::on_select_use(world, control, module, click.client, click.target);
    }
}

/// `/obj/item/mod/control/emp_act`: an active worn unit warns its wearer and, unless its
/// contents are protected, drops the selected module and burns the wearer `5 / severity`.
pub(super) fn suit_emp(world: &mut World, mut cursor: Local<MessageCursor<EmpAct>>) {
    for ev in emp_acts(world, &mut cursor) {
        let Some(suit) = world.get::<ModSuit>(ev.target).cloned() else {
            continue;
        };
        let Some(wearer) = suit.wearer.filter(|_| suit.active) else {
            continue;
        };
        let text = if ev.severity > 1 {
            "Обнаружен слабый электромагнитный импульс!"
        } else {
            "Обнаружен сильный электромагнитный импульс!"
        };
        tell_mob(world, wearer, Span::Notice, text);
        if !ev.hits_contents() {
            continue;
        }
        if let Some(selected) = suit.selected {
            modules::deactivate(world, ev.target, selected, true);
        }
        if let Ok(mut entity) = world.get_entity_mut(wearer)
            && let Ok((mut health, mut stat)) =
                entity.get_components_mut::<(&mut Health, &mut Stat)>()
        {
            let mut new = *stat;
            crate::health::apply_damage(
                &mut health,
                &mut new,
                DamageType::Burn,
                ev.by_severity(5.0) as f32,
            );
            stat.set_if_neq(new);
        }
        let text = format!(
            "Вы чувствуете, как {} нагревается от ЭМИ, слегка обжигая вас.",
            name(world, ev.target)
        );
        tell_mob(world, wearer, Span::Danger, text);
    }
}

/// `EmpProtection` bits on/off without touching the other sources' bits.
pub(crate) fn set_emp_protection(world: &mut World, e: Entity, bits: u8, on: bool) {
    let old = world.get::<EmpProtection>(e).map_or(0, |p| p.0);
    let new = if on { old | bits } else { old & !bits };
    if new == 0 {
        world.entity_mut(e).remove::<EmpProtection>();
    } else {
        world.entity_mut(e).insert(EmpProtection(new));
    }
}

/// The EMP shield's `empprotection` on the unit.
pub(crate) const SHIELD_PROTECTION: u8 = PROTECT_ALL;
