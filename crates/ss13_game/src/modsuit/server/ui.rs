use super::*;

/// `ui_data`/`ui_static_data` into [`ModSuitView`].
pub(super) fn refresh_view(world: &mut World, control: Entity) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    let theme = theme_of(world, &suit);
    let now = now(world);
    let (color, text) = chargebar(world, control);
    let (charge_now, charge_max) = if core_state(world, control).is_some_and(|c| c.infinite) {
        (1.0, 1.0)
    } else {
        (charge(world, control), max_charge(world, control))
    };
    let module_info = suit
        .modules
        .iter()
        .filter_map(|&m| {
            let def = modules::def_of(world, m)?;
            let state = world.get::<ModModule>(m)?;
            Some(ModuleInfo {
                module_name: name(world, m),
                description: def.desc.clone(),
                module_type: def.module_type,
                module_active: state.active,
                pinned: false,
                idle_power: def.idle_power_cost,
                active_power: modules::active_power(world, m),
                use_energy: def.use_energy_cost,
                module_complexity: def.complexity,
                cooldown_time: def.cooldown_time,
                cooldown: (((state.cooldown_until - now) * 10.0).max(0.0) as f32 / 10.0).round()
                    * 10.0,
                id: def.tgui_id.clone(),
                reference: m.to_bits(),
                configuration_data: modules::configuration(world, m),
            })
        })
        .collect();
    let parts = suit
        .parts
        .iter()
        .map(|p| PartInfo {
            slot: slot_name(p.slot).to_owned(),
            name: name(world, p.item),
            deployed: world.get::<CarriedBy>(p.item).is_some(),
            reference: p.item.to_bits(),
        })
        .collect();
    let core_name = suit.core.map(|c| name(world, c));
    let ai = crate::modsuit::ai::assistant(world, control);
    let ai_eye = ai.and_then(|a| world.get::<crate::silicon::ai::AiCore>(a).map(|c| c.eye));
    let (link_id, link_freq, link_call) = crate::modsuit::link::window_fields(world, control);
    let view = ModSuitView {
        statics: ModSuitStatic {
            ui_theme: theme.ui_theme.clone(),
            complexity_max: theme.complexity_max,
        },
        data: ModSuitData {
            suit_status: SuitStatus {
                core_name,
                charge_current: charge_now,
                charge_max,
                chargebar_color: color,
                chargebar_string: text,
                active: suit.active,
                activating: suit.activating,
                open: suit.open,
                malfunctioning: world
                    .get::<crate::device_wires::WireState>(control)
                    .is_some_and(|w| w.malfunctioning),
                locked: suit.locked,
                complexity: suit.complexity,
                ai_name: ai.map(|a| name(world, a)),
                is_ai: false,
                link_id,
                link_freq,
                link_call,
            },
            user_status: UserStatus {
                user_name: suit.wearer.map(|w| name(world, w)).unwrap_or_default(),
                user_assignment: String::new(),
            },
            control: name(world, control),
            module_info,
            parts,
        },
        pins: suit
            .pins
            .iter()
            .map(|&(user, m)| (user, m.to_bits()))
            .collect(),
        ai_eye,
    };
    if let Some(mut old) = world.get_mut::<ModSuitView>(control) {
        old.set_if_neq(view);
    }
}

/// `parse_slot_flags`.
pub(super) fn slot_name(slot: InventorySlot) -> &'static str {
    match slot {
        InventorySlot::Head => "head",
        InventorySlot::Suit => "suit",
        InventorySlot::Gloves => "gloves",
        InventorySlot::Shoes => "feet",
        InventorySlot::Eyes => "eyes",
        InventorySlot::Neck => "neck",
        InventorySlot::Mask => "mask",
        _ => "back",
    }
}

/// tg `default_state` of a carried item: the window stays while its user carries the unit;
/// for its AI assistant, while the AI is in it (`contained_state`).
pub(super) fn window_status(
    mut windows: Query<&mut UiWindow>,
    carried: Query<&CarriedBy>,
    eyes: Query<&crate::silicon::ai::AiEye>,
    inside: Query<&crate::silicon::ai::AiInside>,
) {
    for mut window in &mut windows {
        if window.interface != ModSuitUi::NAME {
            continue;
        }
        let held = carried
            .get(window.source)
            .is_ok_and(|c| c.0 == window.actor);
        let assistant = eyes
            .get(window.actor)
            .ok()
            .and_then(|e| inside.get(e.ai).ok())
            == Some(&crate::silicon::ai::AiInside::Mod(window.source));
        if !held && !assistant {
            window.cap_status(UiStatus::Close);
        }
    }
}

/// tg `/obj/item/mod/control/proc/ui_interact` (the panel action).
pub fn open_panel(world: &mut World, client: Entity, mob: Entity, control: Entity) {
    let title = name(world, control);
    world.write_message(OpenUi::new::<ModSuitUi>(client, mob, control, title));
}

/// `ui_act`, checked against the unit now (`locate(params["ref"]) in modules`/`get_parts()`).
pub(super) fn suit_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<ModSuitUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, ModAct)> = acts
        .read(world.resource::<Messages<UiActed<ModSuitUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, control, act) in pending {
        let Some(suit) = world.get::<ModSuit>(control).cloned() else {
            continue;
        };
        let module = |bits: u64| Entity::try_from_bits(bits).filter(|e| suit.modules.contains(e));
        match act {
            ModAct::Lock => {
                if !suit.locked || allowed(world, control, actor) {
                    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
                        s.locked = !s.locked;
                    }
                    let text = if suit.locked {
                        "разблокировано"
                    } else {
                        "заблокировано"
                    };
                    tell_mob(world, actor, Span::Notice, text);
                } else {
                    refuse(world, control, Some(actor), "недостаточно доступа!");
                }
            }
            ModAct::Activate => {
                toggle_activate(world, control, Some(actor), false);
            }
            ModAct::Call => {
                crate::modsuit::link::window_call(world, client, actor, control);
            }
            ModAct::Select(bits) => {
                if let Some(m) = module(bits) {
                    modules::on_select(world, control, m, actor);
                }
            }
            ModAct::Configure(bits, key, value) => {
                if let Some(m) = module(bits) {
                    modules::configure_edit(world, control, m, &key, &value);
                }
            }
            ModAct::Pin(bits) => {
                if let Some(m) = module(bits) {
                    pin(world, control, m, actor);
                }
            }
            ModAct::Deploy(bits) => {
                let Some(index) = Entity::try_from_bits(bits).and_then(|e| suit.part_index(e))
                else {
                    continue;
                };
                if world.get::<CarriedBy>(suit.parts[index].item).is_some() {
                    retract(world, control, index, Some(actor), false);
                } else {
                    deploy(world, control, index, Some(actor), false);
                }
            }
        }
    }
}

/// tg `module.pin(user)`: a button of its own for `user`, or the button gone again.
pub fn pin(world: &mut World, control: Entity, module: Entity, user: Entity) {
    let passive = modules::def_of(world, module).is_none_or(|d| d.module_type == MODULE_PASSIVE);
    if passive {
        return;
    }
    let holder = world.get::<ModSuit>(control).and_then(|s| s.holder);
    if let Some(mut s) = world.get_mut::<ModSuit>(control) {
        if let Some(i) = s.pins.iter().position(|&p| p == (user, module)) {
            s.pins.remove(i);
        } else {
            s.pins.push((user, module));
        }
    }
    refresh_buttons(world, holder);
}

pub(super) fn button(
    world: &World,
    id: String,
    name: &str,
    dmi: &str,
    state: &str,
) -> (ActionButton, f64) {
    let icons = world.get_resource::<IconIndex>();
    (
        ActionButton {
            id,
            name: name.to_owned(),
            icon: icons.and_then(|i| i.get(dmi, state)),
            background: icons.and_then(|i| i.get(BACKGROUNDS_DMI, "bg_mod")),
            border: icons.and_then(|i| i.get(BACKGROUNDS_DMI, "bg_mod_border")),
            available: true,
            cooldown: 0.0,
            active: false,
            maptext: None,
        },
        0.0,
    )
}

/// The mob's MOD buttons after the others it has (tg `actions_types` and pinned modules,
/// granted while it carries the unit).
pub fn refresh_buttons(world: &mut World, mob: Option<Entity>) {
    let Some(mob) = mob.filter(|&m| world.get_entity(m).is_ok()) else {
        return;
    };
    if world.get::<PlayerMob>(mob).is_none() {
        return;
    }
    let mut buttons: Vec<(ActionButton, f64)> = world
        .get::<ActionButtons>(mob)
        .map(|b| {
            b.0.iter()
                .filter(|(b, _)| !b.id.starts_with(BUTTON))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let now = now(world);
    for control in carried_suits(world, mob) {
        let Some(suit) = world.get::<ModSuit>(control).cloned() else {
            continue;
        };
        let bits = control.to_bits();
        let ready = if now < suit.ready_until {
            "activate-ready"
        } else {
            "activate"
        };
        buttons.push(button(
            world,
            format!("{BUTTON}{bits}:deploy"),
            "Развернуть MODsuit",
            ACTIONS_DMI,
            "deploy",
        ));
        buttons.push(button(
            world,
            format!("{BUTTON}{bits}:activate"),
            "Активировать MODsuit",
            ACTIONS_DMI,
            ready,
        ));
        buttons.push(button(
            world,
            format!("{BUTTON}{bits}:panel"),
            "Панель MODsuit",
            ACTIONS_DMI,
            "panel",
        ));
        buttons.push(button(
            world,
            format!("{BUTTON}{bits}:module"),
            "Переключить модуль",
            ACTIONS_DMI,
            "module",
        ));
        for &(user, module) in &suit.pins {
            if user != mob || !suit.modules.contains(&module) {
                continue;
            }
            let (dmi, state) = world
                .get::<ss13_mapload::DmAtom>(module)
                .map(|a| (a.icon.clone(), a.icon_state.clone()))
                .unwrap_or_default();
            let module_name = name(world, module);
            let mut b = button(
                world,
                format!("{BUTTON}{bits}:pin:{}", module.to_bits()),
                &module_name,
                &dmi,
                &state,
            );
            b.0.active = world.get::<ModModule>(module).is_some_and(|m| m.active);
            b.1 = world
                .get::<ModModule>(module)
                .map_or(0.0, |m| m.cooldown_until);
            buttons.push(b);
        }
    }
    if buttons.is_empty() && world.get::<ActionButtons>(mob).is_none() {
        return;
    }
    set_buttons(world, mob, buttons);
}

/// A click on a MOD button, checked now: the client controls the conscious mob that carries
/// the unit (`AB_CHECK_CONSCIOUS`).
pub(super) fn trigger_buttons(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>,
) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .filter(|a| a.act.starts_with(BUTTON))
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client) {
            continue;
        }
        if world.get::<Stat>(body_of(world, actor)) != Some(&Stat::Conscious) {
            continue;
        }
        let mut fields = id.trim_start_matches(BUTTON).split(':');
        let Some(control) = fields
            .next()
            .and_then(|b| b.parse::<u64>().ok())
            .and_then(Entity::try_from_bits)
            .filter(|&c| controls(world, actor, c))
            .filter(|&c| world.get::<ModSuit>(c).is_some())
        else {
            continue;
        };
        match fields.next() {
            Some("deploy") => choose_deploy(world, client, actor, control),
            Some("activate") => {
                let now = now(world);
                let ready = world
                    .get::<ModSuit>(control)
                    .is_some_and(|s| now < s.ready_until);
                if let Some(mut s) = world.get_mut::<ModSuit>(control) {
                    s.ready_until = if ready { 0.0 } else { now + READY_SECS };
                }
                if ready {
                    toggle_activate(world, control, Some(actor), false);
                } else {
                    tell(
                        world,
                        client,
                        Span::Notice,
                        "Нажмите ещё раз, чтобы подтвердить.",
                    );
                }
                refresh_buttons(world, Some(actor));
            }
            Some("panel") => open_panel(world, client, actor, control),
            Some("module") => quick_module(world, client, actor, control),
            Some("pin") => {
                let module = fields
                    .next()
                    .and_then(|b| b.parse::<u64>().ok())
                    .and_then(Entity::try_from_bits)
                    .filter(|m| {
                        world
                            .get::<ModSuit>(control)
                            .is_some_and(|s| s.modules.contains(m))
                    });
                if let Some(module) = module {
                    modules::on_select(world, control, module, actor);
                }
            }
            _ => {}
        }
    }
}

/// tg `choose_deploy`'s radial menu, as a list: every part (with its state) and "all".
pub(super) fn choose_deploy(world: &mut World, client: Entity, mob: Entity, control: Entity) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    if suit.parts.is_empty() {
        return;
    }
    let mut items = vec!["Все части".to_owned()];
    let mut options = vec![None];
    for p in &suit.parts {
        let state = if world.get::<CarriedBy>(p.item).is_some() {
            "развёрнуто"
        } else {
            "свёрнуто"
        };
        items.push(format!("{} ({state})", name(world, p.item)));
        options.push(Some(p.item));
    }
    ask(
        world,
        client,
        mob,
        control,
        items,
        ChoiceKind::Deploy(options),
    );
}

/// tg `quick_module`'s radial menu, as a list of the non-passive modules.
pub(super) fn quick_module(world: &mut World, client: Entity, mob: Entity, control: Entity) {
    let Some(suit) = world.get::<ModSuit>(control).cloned() else {
        return;
    };
    let options: Vec<Entity> = suit
        .modules
        .iter()
        .copied()
        .filter(|&m| modules::def_of(world, m).is_some_and(|d| d.module_type != MODULE_PASSIVE))
        .collect();
    if options.is_empty() {
        return;
    }
    let items = options
        .iter()
        .map(|&m| {
            let state = if suit.selected == Some(m) {
                " (выбран)"
            } else if world.get::<ModModule>(m).is_some_and(|s| s.active) {
                " (активен)"
            } else {
                ""
            };
            format!("{}{state}", name(world, m))
        })
        .collect();
    ask(
        world,
        client,
        mob,
        control,
        items,
        ChoiceKind::Module(options),
    );
}

pub(super) fn ask(
    world: &mut World,
    client: Entity,
    mob: Entity,
    control: Entity,
    items: Vec<String>,
    kind: ChoiceKind,
) {
    let title = name(world, control);
    let prompt = ask_world(
        world,
        Ask::new(
            client,
            mob,
            title,
            "",
            PromptKind::List {
                items,
                default: None,
            },
        ),
    );
    world.entity_mut(prompt).insert(ModChoice { control, kind });
}

/// The answers, revalidated: a conscious actor still carrying (deploy, modules) or reaching
/// (removal) the unit, the choice still part of it.
pub(super) fn prompt_answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answers {
        let Some(choice) = world.get::<ModChoice>(prompt).cloned() else {
            continue;
        };
        let Some(PromptAnswer::Item(i)) = answer else {
            continue;
        };
        let control = choice.control;
        let Some(suit) = world.get::<ModSuit>(control).cloned() else {
            continue;
        };
        let body = body_of(world, actor);
        let able = world.get::<Stat>(body) == Some(&Stat::Conscious)
            && !crate::status_effects::incapacitation(world, body).incapacitated;
        let holds = controls(world, actor, control);
        if !able {
            continue;
        }
        match choice.kind {
            ChoiceKind::Deploy(options) if holds => {
                if suit.activating || !suit.steps.is_empty() {
                    let text = if suit.active {
                        "идёт разгерметизация!"
                    } else {
                        "идёт герметизация!"
                    };
                    refuse(world, control, Some(actor), text);
                    continue;
                }
                match options.get(i) {
                    Some(None) => {
                        quick_deploy(world, control, actor);
                    }
                    Some(Some(part)) => {
                        let Some(index) = suit.part_index(*part) else {
                            continue;
                        };
                        let done = if world.get::<CarriedBy>(*part).is_some() {
                            retract(world, control, index, Some(actor), false)
                        } else {
                            deploy(world, control, index, Some(actor), false)
                        };
                        // tg asks again while some part is still on the other side.
                        if done {
                            let all_same = {
                                let s = world.get::<ModSuit>(control).cloned();
                                s.is_some_and(|s| {
                                    let out =
                                        |p: &ModPart| world.get::<CarriedBy>(p.item).is_some();
                                    s.parts.iter().all(out) || !s.parts.iter().any(out)
                                })
                            };
                            if !all_same {
                                choose_deploy(world, client, actor, control);
                            }
                        }
                    }
                    None => {}
                }
            }
            ChoiceKind::Module(options) if holds => {
                if let Some(&m) = options.get(i).filter(|m| suit.modules.contains(m)) {
                    modules::on_select(world, control, m, actor);
                }
            }
            ChoiceKind::Remove(options) => {
                let reach = holds || crate::storage::in_reach(world, actor, control);
                let Some(&m) = options.get(i).filter(|m| suit.modules.contains(m)) else {
                    continue;
                };
                let removable = modules::def_of(world, m).is_some_and(|d| d.removable);
                if !reach || !suit.open || !removable || !allowed(world, control, actor) {
                    continue;
                }
                uninstall(world, control, m);
                if let Some(pos) = drop_pos(world, control) {
                    place_on(world, m, pos);
                }
            }
            _ => {}
        }
    }
}
