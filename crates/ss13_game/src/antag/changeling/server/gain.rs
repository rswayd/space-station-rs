//! Gaining the changeling: objectives, buttons, chemicals and the power buttons.

use super::*;

pub(super) fn rng_of(world: &mut World, seed: u64) -> Rng {
    world
        .remove_resource::<DynamicRng>()
        .map_or_else(|| Rng::new(seed), |r| r.0)
}

pub(super) fn icon(world: &World, (dmi, state): (&str, &str)) -> Option<ss13_shared::IconStateId> {
    world
        .get_resource::<IconIndex>()
        .and_then(|index| index.get(dmi, state))
}

/// The mob's DNA as a profile sees it: its name and body.
pub(super) fn dna_of(world: &World, mob: Entity) -> (String, Option<HumanLook>) {
    let look = world.get::<HumanLook>(mob).map(|l| HumanLook {
        uniform: None,
        shoes: None,
        husk: false,
        hulk: false,
        missing: 0,
        robotic: 0,
        eyeless: false,
        ..l.clone()
    });
    (name_of(world, mob), look)
}

/// `get_id_name`: the registered name of the worn ID.
pub(super) fn id_name(world: &World, mob: Entity) -> Option<String> {
    let card = world
        .get::<Inventory>(mob)?
        .id
        .and_then(|e| crate::inventory::get_id(world, e))?;
    world
        .get::<crate::outfit::IdRegistration>(card)
        .map(|r| r.registered_name.clone())
}

/// `add_antag_datum(/datum/antagonist/changeling)` on `mob`. `false` if it already is one.
pub fn add_changeling(world: &mut World, mob: Entity) -> bool {
    if world.get::<Changeling>(mob).is_some() || world.get::<Health>(mob).is_none() {
        return false;
    }
    let mut rng = rng_of(world, mob.to_bits());
    // `generate_name`: an honorific and a greek letter, then numbers.
    let honorific = match world.get::<HumanLook>(mob).map(|l| l.female) {
        Some(true) => "Ms.",
        Some(false) => "Mr.",
        None => "Mx.",
    };
    let letter = {
        let mut ids = world.resource_mut::<ChangelingIds>();
        (!ids.0.is_empty()).then(|| {
            let i = rng.range(0, ids.0.len() as i64 - 1) as usize;
            ids.0.remove(i)
        })
    };
    let id_name = match letter {
        Some(letter) => format!("{honorific} {letter}"),
        None => format!("{honorific} {}", rng.range(1, 999)),
    };
    let hive_name = format!(
        "{}-hive",
        HIVE_NAMES[rng.range(0, HIVE_NAMES.len() as i64 - 1) as usize]
    );
    let objectives = forge_objectives(world, mob, &mut rng);
    world.insert_resource(DynamicRng(rng));
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let (name, look) = dna_of(world, mob);
    let mut ling = Changeling {
        key,
        id_name,
        hive_name,
        objectives,
        ..default()
    };
    // `create_initial_profile`.
    ling.add_profile(Profile {
        name,
        look,
        protected: false,
    });
    let catalog = emporium_catalog(world);
    let hud_icon = icon(world, ANTAG_HUD);
    world.entity_mut(mob).insert((
        ling,
        catalog,
        AntagHudData {
            hud_icon,
            ..default()
        },
        ChangelingHudData::default(),
        ChangelingHivemind,
    ));
    // `greet()`, the panel note and the panel.
    chat_to(world, mob, Span::Bold, format!("Вы - {CHANGELING_NAME}."));
    announce_objectives(world, mob);
    chat_to(
        world,
        mob,
        Span::BoldNotice,
        "Для большей информации - прочтите панель. Вы можете вернуться снова, нажав кнопку \
         сверху слева.",
    );
    if let Some(client) = client {
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
        world.write_message(OpenUi::new::<ChangelingHudUi>(client, mob, mob, ""));
        world.write_message(OpenUi::new::<AntagInfoChangelingUi>(
            client,
            mob,
            mob,
            CHANGELING_NAME,
        ));
    }
    refresh(world, mob);
    info!("{mob} is a changeling");
    true
}

/// `announce_objectives`.
pub(super) fn announce_objectives(world: &mut World, mob: Entity) {
    let Some(lines) = world.get::<Changeling>(mob).map(|c| {
        c.objectives
            .iter()
            .enumerate()
            .map(|(i, o)| format!("{OBJECTIVE_NAME} #{}: {}", i + 1, o.explanation()))
            .collect::<Vec<_>>()
    }) else {
        return;
    };
    chat_to(world, mob, Span::Notice, "Ваши текущие задачи:");
    for line in lines {
        chat_to(world, mob, Span::Plain, line);
    }
}

pub(super) fn objective(world: &World, kind: ObjectiveKind) -> Objective {
    Objective {
        explanation: kind.explanation(world),
        kind,
    }
}

pub(super) fn assassinate(target: Option<Entity>) -> ObjectiveKind {
    ObjectiveKind::Assassinate { target }
}

pub(super) fn maroon(target: Option<Entity>) -> ObjectiveKind {
    ObjectiveKind::Maroon { target }
}

/// `escape_with_identity/update_explanation_text` for `target`.
pub(super) fn identity_theft(world: &World, target: Option<Entity>) -> LingObjective {
    let Some(target) = target else {
        return LingObjective::EscapeIdentity {
            target: None,
            name: String::new(),
            text: ObjectiveKind::Escape.explanation(world),
            missing_id: false,
        };
    };
    let name = name_of(world, target);
    let mut text = format!(
        "Сбегите на эвакуационном шаттле или челноке под личиной {}",
        target_text(world, target)
    );
    let missing_id = id_name(world, target).as_deref() != Some(name.as_str());
    if !missing_id {
        text.push_str(", при этом нося ID карту цели");
    }
    text.push('.');
    LingObjective::EscapeIdentity {
        target: Some(target),
        name,
        text,
        missing_id,
    }
}

/// `forge_objectives`.
pub(super) fn forge_objectives(
    world: &mut World,
    owner: Entity,
    rng: &mut Rng,
) -> Vec<LingObjective> {
    let competitive = {
        let mut q = world.query::<(Entity, &Changeling)>();
        q.iter(world).any(|(e, _)| e != owner)
    };
    let mut objectives = Vec::new();
    match if competitive { rng.range(1, 3) } else { 1 } {
        2 => objectives.push(LingObjective::AbsorbMost),
        3 => objectives.push(LingObjective::AbsorbChangeling),
        _ => {
            // `gen_amount_goal(6, 8)`: at most the other humans with a player.
            let goal = rng.range(6, 8) as u32;
            let mut q = world.query_filtered::<Entity, (With<PlayerMob>, With<HumanLook>)>();
            let others = q
                .iter(world)
                .filter(|&e| e != owner && world.get::<Changeling>(e).is_none())
                .count() as u32;
            objectives.push(LingObjective::Absorb {
                target: goal.min(others + 1),
            });
        }
    }
    let mut existing: Vec<Objective> = Vec::new();
    if rng.prob(60.0) {
        let targets: Vec<StealItemDef> = world
            .get_resource::<UplinkCatalog>()
            .map(|c| c.data.steal_items.clone())
            .unwrap_or_default();
        let item = find_steal_target(world, &[owner], &existing, &targets, rng);
        let o = objective(world, ObjectiveKind::Steal { item });
        existing.push(o.clone());
        objectives.push(LingObjective::Generic(o));
    }
    // shortcut: no AIs to destroy (`active_ais()` is empty here).
    let mut escape_possible = true;
    if rng.prob(70.0) {
        let target = find_target(world, &[owner], &existing, assassinate, rng);
        objectives.push(LingObjective::Generic(objective(
            world,
            assassinate(target),
        )));
    } else {
        let identity_target = find_target(world, &[owner], &existing, maroon, rng);
        let identity = identity_theft(world, identity_target);
        escape_possible = false;
        let target =
            identity_target.or_else(|| find_target(world, &[owner], &existing, maroon, rng));
        objectives.push(LingObjective::Generic(objective(world, maroon(target))));
        objectives.push(identity);
    }
    if escape_possible {
        if rng.prob(50.0) {
            objectives.push(LingObjective::Generic(objective(
                world,
                ObjectiveKind::Escape,
            )));
        } else {
            let target = find_target(world, &[owner], &existing, maroon, rng);
            objectives.push(identity_theft(world, target));
        }
    }
    objectives
}

/// `/datum/cellular_emporium/ui_static_data`: the sellable powers by name.
pub(super) fn emporium_catalog(world: &World) -> EmporiumCatalog {
    let background = icon(world, (ACTIONS_DMI, "bg_changeling"));
    let mut abilities: Vec<Ability> = Power::ALL
        .into_iter()
        .filter_map(|p| {
            let def = p.def();
            Some(Ability {
                name: def.name.to_owned(),
                desc: def.desc.to_owned(),
                icon: def.icon.to_owned(),
                icon_state: icon(world, (ACTIONS_DMI, def.icon)),
                background,
                helptext: def.helptext.to_owned(),
                path: def.path.to_owned(),
                genetic_point_required: def.dna_cost?,
                absorbs_required: def.req_absorbs,
                dna_required: def.req_dna,
                category: def.category.to_owned(),
            })
        })
        .collect();
    abilities.sort_by(|a, b| a.name.cmp(&b.name));
    EmporiumCatalog(abilities)
}

/// The action buttons and the chemical counter from the changeling's state
/// (`build_all_button_icons`, `update_chemical_hud`).
pub fn refresh(world: &mut World, mob: Entity) {
    let Some(ling) = world.get::<Changeling>(mob).cloned() else {
        return;
    };
    let (background, border) = (
        icon(world, (ACTIONS_DMI, "bg_changeling")),
        icon(world, (ACTIONS_DMI, "bg_changeling_border")),
    );
    let button = |id: &str, name: &str, icon: Option<ss13_shared::IconStateId>, active: bool| {
        (
            ActionButton {
                id: id.to_owned(),
                name: name.to_owned(),
                icon,
                background,
                border,
                available: true,
                cooldown: 0.0,
                active,
                maptext: None,
            },
            0.0,
        )
    };
    let mut buttons = Vec::new();
    if !ling.fallen {
        buttons.push(button(
            EMPORIUM_ID,
            "Cellular Emporium",
            icon(world, EMPORIUM_ICON),
            false,
        ));
        let powers = Power::ALL
            .into_iter()
            .filter(|p| p.innate())
            .chain(ling.purchased.iter().copied());
        for power in powers {
            let def = power.def();
            let ready = matches!(ling.stasis, Stasis::Ready { .. });
            let (name, state) = if power == Power::Fakedeath && ready {
                ("Revive", "revive")
            } else {
                (def.name, def.icon)
            };
            // The chosen sting's button shows it (tg the HUD's sting icon).
            let active = ling.chosen_sting == Some(power);
            buttons.push(button(
                def.id,
                name,
                icon(world, (ACTIONS_DMI, state)),
                active,
            ));
        }
    }
    set_buttons(world, mob, buttons);
    if let Some(mut hud) = world.get_mut::<ChangelingHudData>(mob) {
        hud.set_if_neq(ChangelingHudData {
            chem_charges: ling.chem_charges.round() as u32,
            chem_max: ling.total_chem_storage.round() as u32,
        });
    }
}

pub(super) fn use_context(world: &World, mob: Entity) -> UseContext {
    let stat = world.get::<Stat>(mob).copied().unwrap_or(Stat::Dead);
    let knocked_out = world.get::<DeathComa>(mob).is_some()
        || world
            .get::<Health>(mob)
            .is_some_and(|h| h.knocked_out(stat));
    UseContext {
        stat,
        knocked_out,
        human: world.get::<HumanLook>(mob).is_some(),
        on_fire: crate::fire::on_fire(world, mob),
    }
}

/// The use checks with their feedback (tg balloon alerts).
pub(super) fn checked(world: &mut World, client: Entity, mob: Entity, power: Power) -> bool {
    let ctx = use_context(world, mob);
    match world
        .get::<Changeling>(mob)
        .map(|c| c.check_use(power, ctx))
    {
        Some(Ok(())) => true,
        Some(Err(text)) => {
            if !text.is_empty() {
                tell(world, client, Span::Warning, text);
            }
            false
        }
        None => false,
    }
}

/// `try_to_sting` after `sting_action` succeeded: the chemicals and the click cooldown.
pub(super) fn spend(world: &mut World, mob: Entity, power: Power) {
    let cost = power.def().chemical_cost as f32;
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut ling) = world.get_mut::<Changeling>(mob) {
        ling.adjust_chemicals(-cost, None);
    }
    world.entity_mut(mob).insert(NextMove(now + CLICK_CD_MELEE));
    info!("{mob} used {power:?} (changeling)");
    refresh(world, mob);
}

// ---------------------------------------------------------------------------------------------
// Buttons

/// Button clicks: tg `Trigger` of the changeling's actions and the emporium's.
pub(super) fn trigger_powers(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>,
) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        // The client must still control the changeling (not its ghost through a stale window).
        if actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world.get::<Changeling>(actor).is_none_or(|c| c.fallen)
        {
            continue;
        }
        if id == EMPORIUM_ID {
            world.write_message(OpenUi::new::<CellularEmporiumUi>(
                client,
                actor,
                actor,
                "cellular emporium",
            ));
            continue;
        }
        let Some(power) = Power::from_id(&id) else {
            continue;
        };
        try_power(world, client, actor, power);
    }
}

/// tg `try_to_sting` of `power`, with the powers' own overrides.
pub(super) fn try_power(world: &mut World, client: Entity, mob: Entity, power: Power) {
    // `sting/Trigger`: choosing or putting away, no checks until it stings.
    if power.is_sting() {
        toggle_sting(world, client, mob, power);
        return;
    }
    // `weapon/try_to_sting`, `suit/try_to_sting`: a second use takes it back, for free.
    match power {
        Power::ArmBlade if retract_blade(world, mob) => return,
        Power::Armor if cast_off_armor(world, mob) => return,
        _ => {}
    }
    // `fakedeath/can_sting`: a ready revive skips the stasis checks.
    if power == Power::Fakedeath
        && !matches!(
            world.get::<Changeling>(mob).map(|c| c.stasis),
            Some(Stasis::Ready { .. })
        )
        && world
            .get::<Changeling>(mob)
            .is_some_and(Changeling::in_stasis)
    {
        tell(world, client, Span::Warning, "уже возрождаемся!");
        return;
    }
    if !checked(world, client, mob, power) {
        return;
    }
    let done = match power {
        Power::AbsorbDna => start_absorb(world, client, mob),
        Power::Transform => open_transform(world, client, mob),
        Power::Fakedeath => fakedeath(world, mob),
        Power::Regenerate => regenerate(world, mob),
        Power::Adrenaline => adrenaline(world, mob),
        Power::Fleshmend => fleshmend(world, client, mob),
        Power::ArmBlade => form_blade(world, client, mob),
        Power::Armor => form_armor(world, client, mob),
        // Handled above.
        Power::TransformationSting
        | Power::FalseArmbladeSting
        | Power::ExtractDnaSting
        | Power::MuteSting
        | Power::BlindSting
        | Power::LsdSting
        | Power::CryoSting => false,
    };
    if done {
        spend(world, mob, power);
    }
}
