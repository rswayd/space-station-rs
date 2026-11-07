use super::*;

pub(super) fn reagent_name(world: &World, id: ReagentId) -> String {
    world.resource::<Chem>().0.def(id).name.clone()
}

pub(super) fn ask(
    world: &mut World,
    client: Entity,
    mob: Entity,
    machine: Entity,
    title: &str,
    step: Step,
) {
    let kind = match &step {
        Step::Menu(choices) => PromptKind::List {
            items: choices
                .iter()
                .map(|c| choice_label(world, machine, *c))
                .collect(),
            default: None,
        },
        Step::Reagent { .. } => PromptKind::List {
            items: world
                .resource::<Chem>()
                .0
                .reagents
                .iter()
                .map(|r| r.name.clone())
                .collect(),
            default: None,
        },
        Step::Number(setting) => {
            let (default, min, max) = setting_range(world, machine, *setting);
            PromptKind::Number {
                default,
                min,
                max,
                round: matches!(
                    setting,
                    Setting::SynthAmount | Setting::PillDuration | Setting::BottlerAmount
                ),
            }
        }
        Step::Synth(ids) => PromptKind::List {
            items: ids.iter().map(|&id| reagent_name(world, id)).collect(),
            default: None,
        },
        Step::Name => PromptKind::Text {
            default: match kind(world, machine) {
                Some(Kind::PillPress { product, .. }) => product.clone(),
                _ => String::new(),
            },
            max_length: MAX_NAME_LEN,
            multiline: false,
        },
    };
    let prompt = ask_world(world, Ask::new(client, mob, title, "", kind));
    world
        .entity_mut(prompt)
        .insert(PlumbPrompt { machine, step });
}

pub(super) fn choice_label(world: &World, machine: Entity, choice: Choice) -> String {
    let k = kind(world, machine);
    match choice {
        Choice::Number(s) => match (s, k) {
            (Setting::AcclimatorTarget, Some(Kind::Acclimator { target, .. })) => {
                format!("Целевая температура: {target} K")
            }
            (Setting::AcclimatorVolume, Some(Kind::Acclimator { max_volume, .. })) => {
                format!("Объём: {max_volume} u")
            }
            (
                Setting::Splitter(d),
                Some(Kind::Splitter {
                    straight,
                    left,
                    right,
                }),
            ) => {
                let (name, v) = match d {
                    S => ("Прямо", straight),
                    E => ("Влево", left),
                    _ => ("Вправо", right),
                };
                format!("{name}: {v} u")
            }
            (Setting::ChamberTarget, Some(Kind::ReactionChamber { target, .. })) => {
                format!("Целевая температура: {target} K")
            }
            (Setting::DisposerRate, Some(Kind::Disposer { rate, .. })) => {
                format!("Скорость: {rate} u/с")
            }
            (Setting::AcclimatorVolume, _) => "Объём".to_owned(),
            (Setting::AcidicLimit, Some(Kind::ReactionChamber { ph: Some(ph), .. })) => {
                format!("Кислотный предел pH: {}", ph.acidic_limit)
            }
            (Setting::AlkalineLimit, Some(Kind::ReactionChamber { ph: Some(ph), .. })) => {
                format!("Щелочной предел pH: {}", ph.alkaline_limit)
            }
            (Setting::SynthAmount, Some(Kind::Synthesizer { amount, .. })) => {
                format!("Количество: {amount} u")
            }
            (Setting::PressVolume, Some(Kind::PillPress { volume, .. })) => {
                format!("Объём продукта: {volume} u")
            }
            (Setting::PillDuration, Some(Kind::PillPress { duration, .. })) => {
                format!("Слои сахара таблетки: {duration}")
            }
            _ => "?".to_owned(),
        },
        Choice::FilterAdd(true) => "Добавить влево".to_owned(),
        Choice::FilterAdd(false) => "Добавить вправо".to_owned(),
        Choice::FilterRemove(left, id) => format!(
            "Убрать {} ({})",
            reagent_name(world, id),
            if left { "влево" } else { "вправо" }
        ),
        Choice::ChamberAdd => "Добавить реагент".to_owned(),
        Choice::ChamberRemove(id) => format!("Убрать {}", reagent_name(world, id)),
        Choice::DisposerToggle => match k {
            Some(Kind::Disposer { on: true, .. }) => "Выключить".to_owned(),
            _ => "Включить".to_owned(),
        },
        Choice::ChamberCatalyst(id) => {
            let catalyst = matches!(k, Some(Kind::ReactionChamber { catalysts, .. })
                if catalysts.iter().any(|&(c, _)| c == id));
            if catalyst {
                format!("{}: катализатор → реагент", reagent_name(world, id))
            } else {
                format!("{}: реагент → катализатор", reagent_name(world, id))
            }
        }
        Choice::SynthReagent => match k {
            Some(Kind::Synthesizer {
                reagent: Some(id), ..
            }) => format!("Реагент: {}", reagent_name(world, *id)),
            _ => "Реагент: нет".to_owned(),
        },
        Choice::PressPackage(p) => {
            let current = matches!(k, Some(Kind::PillPress { package, .. }) if *package == p);
            format!("{}{}", p.suffix(), if current { " (выбрано)" } else { "" })
        }
        Choice::PressName => match k {
            Some(Kind::PillPress { product, .. }) => format!("Название: {product}"),
            _ => "Название".to_owned(),
        },
    }
}

pub(super) fn setting_range(world: &World, machine: Entity, setting: Setting) -> (f64, f64, f64) {
    let max_temp = f64::from(ss13_reagents::CHEMICAL_MAXIMUM_TEMPERATURE);
    match (setting, kind(world, machine)) {
        (Setting::AcclimatorTarget, Some(Kind::Acclimator { target, .. })) => {
            (f64::from(*target), 0.0, max_temp)
        }
        (Setting::AcclimatorVolume, Some(Kind::Acclimator { max_volume, .. })) => {
            let buffer = holder(world, machine).map_or(200.0, |h| h.maximum_volume);
            (f64::from(*max_volume), 1.0, f64::from(buffer))
        }
        (Setting::Splitter(_), _) => (5.0, 1.0, f64::from(SPLITTER_MAX)),
        (Setting::BufferActivation, Some(Kind::Buffer { activation, .. })) => {
            let buffer = holder(world, machine).map_or(200.0, |h| h.maximum_volume);
            (f64::from(*activation), 1.0, f64::from(buffer))
        }
        (Setting::ChamberTarget, Some(Kind::ReactionChamber { target, .. })) => {
            (f64::from(*target), 0.0, 1000.0)
        }
        (Setting::ChamberAmount, _) => (10.0, 0.01, 200.0),
        (Setting::DisposerRate, Some(Kind::Disposer { rate, .. })) => {
            (f64::from(*rate), 0.1, f64::from(DISPOSAL_MAX))
        }
        // `acidic`: `CHEMICAL_MIN_PH..alkaline_limit - 1`; `alkaline`: `acidic_limit + 1..14`.
        (Setting::AcidicLimit, Some(Kind::ReactionChamber { ph: Some(ph), .. })) => (
            f64::from(ph.acidic_limit),
            0.0,
            f64::from(ph.alkaline_limit - 1.0),
        ),
        (Setting::AlkalineLimit, Some(Kind::ReactionChamber { ph: Some(ph), .. })) => (
            f64::from(ph.alkaline_limit),
            f64::from(ph.acidic_limit + 1.0),
            14.0,
        ),
        (Setting::SynthAmount, Some(Kind::Synthesizer { amount, .. })) => {
            (f64::from(*amount), 0.0, f64::from(SYNTH_MAX))
        }
        (
            Setting::PressVolume,
            Some(Kind::PillPress {
                volume, package, ..
            }),
        ) => (
            f64::from(*volume),
            f64::from(PRESS_MIN_VOLUME),
            f64::from(package.container().1),
        ),
        (Setting::PillDuration, Some(Kind::PillPress { duration, .. })) => (
            f64::from(*duration),
            0.0,
            f64::from(crate::chem_master::MAX_PILL_LAYERS),
        ),
        // `bottler/interact`: `tgui_input_number(..., max_value = 100, min_value = 1)`.
        (Setting::BottlerAmount, Some(Kind::Bottler { wanted })) => {
            (f64::from(*wanted), 1.0, 100.0)
        }
        _ => (0.0, 0.0, 0.0),
    }
}

pub(super) fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, mob, answer) in answers {
        let Some(PlumbPrompt { machine, step }) = world.get::<PlumbPrompt>(prompt).cloned() else {
            continue;
        };
        let Some(answer) = answer else {
            continue;
        };
        if world.get::<Plumber>(machine).is_none() || !can_use(world, mob, machine) {
            continue;
        }
        apply_answer(world, client, mob, machine, step, answer);
    }
}

pub(super) fn apply_answer(
    world: &mut World,
    client: Entity,
    mob: Entity,
    machine: Entity,
    step: Step,
    answer: PromptAnswer,
) {
    let Some(mut plumber) = world.get::<Plumber>(machine).cloned() else {
        return;
    };
    match (step, answer) {
        (Step::Menu(choices), PromptAnswer::Item(i)) => {
            let Some(&choice) = choices.get(i) else {
                return;
            };
            match choice {
                Choice::Number(s) => ask(world, client, mob, machine, "Значение", Step::Number(s)),
                Choice::FilterAdd(left) => ask(
                    world,
                    client,
                    mob,
                    machine,
                    "Реагент",
                    Step::Reagent {
                        left: Some(left),
                        amount: 0.0,
                    },
                ),
                Choice::ChamberAdd => ask(
                    world,
                    client,
                    mob,
                    machine,
                    "Количество",
                    Step::Number(Setting::ChamberAmount),
                ),
                Choice::FilterRemove(side, id) => {
                    if let Kind::Filter { left, right } = &mut plumber.kind {
                        let list = if side { left } else { right };
                        list.retain(|&r| r != id);
                    }
                }
                Choice::ChamberRemove(id) => {
                    if let Kind::ReactionChamber { required, .. } = &mut plumber.kind {
                        required.retain(|&(r, _)| r != id);
                    }
                }
                Choice::DisposerToggle => {
                    if let Kind::Disposer { on, .. } = &mut plumber.kind {
                        *on = !*on;
                    }
                }
                Choice::ChamberCatalyst(id) => {
                    if let Kind::ReactionChamber {
                        required,
                        catalysts,
                        ..
                    } = &mut plumber.kind
                    {
                        if let Some(i) = required.iter().position(|&(r, _)| r == id) {
                            catalysts.push(required.remove(i));
                        } else if let Some(i) = catalysts.iter().position(|&(r, _)| r == id) {
                            required.push(catalysts.remove(i));
                        }
                    }
                }
                Choice::SynthReagent => {
                    if let Kind::Synthesizer { list, .. } = &plumber.kind {
                        let chem = world.resource::<Chem>().0.clone();
                        let ids = list.iter().filter_map(|p| chem.id(p)).collect();
                        ask(world, client, mob, machine, "Реагент", Step::Synth(ids));
                    }
                    return;
                }
                Choice::PressPackage(p) => {
                    // `change_product`: the volume clamps to the new container's.
                    if let Kind::PillPress {
                        package, volume, ..
                    } = &mut plumber.kind
                    {
                        *package = p;
                        *volume = volume.clamp(PRESS_MIN_VOLUME, p.container().1);
                    }
                }
                Choice::PressName => {
                    ask(world, client, mob, machine, "Название", Step::Name);
                    return;
                }
            }
        }
        (Step::Number(Setting::ChamberAmount), PromptAnswer::Number(n)) => {
            let amount = round_to(n as f32, CHEMICAL_VOLUME_ROUNDING);
            if amount.is_finite() && amount > 0.0 {
                ask(
                    world,
                    client,
                    mob,
                    machine,
                    "Реагент",
                    Step::Reagent { left: None, amount },
                );
            }
            return;
        }
        (Step::Number(setting), PromptAnswer::Number(n)) => {
            let (_, min, max) = setting_range(world, machine, setting);
            if !n.is_finite() {
                return;
            }
            let v = n.clamp(min, max) as f32;
            match (&mut plumber.kind, setting) {
                (Kind::Acclimator { target, .. }, Setting::AcclimatorTarget) => *target = v.round(),
                (Kind::Acclimator { max_volume, .. }, Setting::AcclimatorVolume) => {
                    *max_volume = v.round();
                }
                (
                    Kind::Splitter {
                        straight,
                        left,
                        right,
                    },
                    Setting::Splitter(d),
                ) => {
                    let v = v.round().clamp(1.0, SPLITTER_MAX);
                    match d {
                        S => *straight = v,
                        E => *left = v,
                        _ => *right = v,
                    }
                }
                (Kind::Buffer { activation, .. }, Setting::BufferActivation) => {
                    *activation = v.round();
                }
                (Kind::ReactionChamber { target, .. }, Setting::ChamberTarget) => {
                    *target = v.round();
                }
                (Kind::Disposer { rate, .. }, Setting::DisposerRate) => {
                    *rate = round_to(v, CHEMICAL_VOLUME_ROUNDING);
                }
                (Kind::ReactionChamber { ph: Some(ph), .. }, Setting::AcidicLimit) => {
                    ph.acidic_limit = v.round();
                }
                (Kind::ReactionChamber { ph: Some(ph), .. }, Setting::AlkalineLimit) => {
                    ph.alkaline_limit = v.round();
                }
                // `possible_amounts`: 0..5.
                (Kind::Synthesizer { amount, .. }, Setting::SynthAmount) => *amount = v.round(),
                (Kind::PillPress { volume, .. }, Setting::PressVolume) => *volume = v,
                (Kind::PillPress { duration, .. }, Setting::PillDuration) => {
                    *duration = v.round();
                }
                (Kind::Bottler { wanted }, Setting::BottlerAmount) => *wanted = v.round(),
                _ => return,
            }
        }
        (Step::Reagent { left, amount }, PromptAnswer::Item(i)) => {
            let chem = world.resource::<Chem>().0.clone();
            let Some(def) = chem.reagents.get(i) else {
                return;
            };
            let Some(id) = chem.id(&def.path) else {
                return;
            };
            match (&mut plumber.kind, left) {
                (Kind::Filter { left, right }, Some(side)) => {
                    let list = if side { left } else { right };
                    if !list.contains(&id) {
                        list.push(id);
                    }
                }
                (Kind::ReactionChamber { required, .. }, None) => {
                    if !required.iter().any(|&(r, _)| r == id) {
                        required.push((id, amount));
                    }
                }
                _ => return,
            }
        }
        // `ui_act("select")`: only a reagent of the machine's list.
        (Step::Synth(ids), PromptAnswer::Item(i)) => {
            let Some(&id) = ids.get(i) else {
                return;
            };
            if let Kind::Synthesizer { list, reagent, .. } = &mut plumber.kind {
                let chem = world.resource::<Chem>().0.clone();
                if !list.iter().any(|p| chem.id(p) == Some(id)) {
                    return;
                }
                *reagent = Some(id);
            }
            // The old reagent is flushed (`reagents.clear_reagents()`).
            if let Some(mut r) = world.get_mut::<Reagents>(machine) {
                r.0.clear();
            }
        }
        (Step::Name, PromptAnswer::Text(name)) => {
            let name: String = name.trim().chars().take(MAX_NAME_LEN as usize).collect();
            if name.is_empty() {
                return;
            }
            if let Kind::PillPress { product, .. } = &mut plumber.kind {
                *product = name;
            }
        }
        _ => return,
    }
    world.entity_mut(machine).insert(plumber);
}
