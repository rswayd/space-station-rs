//! Gaining the vampire: objectives, upgrades, buttons and blood.

use super::*;

pub(super) fn rng_of(world: &mut World, seed: u64) -> Rng {
    world
        .remove_resource::<DynamicRng>()
        .map_or_else(|| Rng::new(seed), |r| r.0)
}

/// `add_antag_datum(/datum/antagonist/vampire)` on `mob`. `false` if it already is one.
pub fn add_vampire(world: &mut World, mob: Entity) -> bool {
    if world.get::<Vampire>(mob).is_some() || world.get::<Blood>(mob).is_none() {
        return false;
    }
    let mut rng = rng_of(world, mob.to_bits());
    let objectives = forge_objectives(world, mob, &mut rng);
    world.insert_resource(DynamicRng(rng));
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let hud_icon = world
        .get_resource::<IconIndex>()
        .and_then(|index| index.get(ANTAG_HUD.0, ANTAG_HUD.1));
    world.entity_mut(mob).insert((
        Vampire {
            key,
            objectives,
            ..default()
        },
        AntagHudData {
            hud_icon,
            ..default()
        },
        VampireHudData::default(),
        MoveSpeedModifier::default(),
    ));
    // `apply_innate_effects`: the powers of 0 blood, silently.
    check_upgrades(world, mob, false);
    // `greet()`, the panel note and the panel.
    chat_to(world, mob, Span::Bold, format!("Вы - {VAMPIRE_NAME}."));
    chat_to(world, mob, Span::Danger, "Вы — вампир!");
    chat_to(
        world,
        mob,
        Span::Notice,
        "Чтобы укусить кого-то, выберите голову, включите боевой режим и используйте пустую руку. \
         Пейте кровь, чтобы получить новые силы. Вы слабы перед святыми предметами, светом звёзд \
         и огнём. Не выходите в космос и избегайте священника, часовни и особенно святой воды.",
    );
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
        world.write_message(OpenUi::new::<VampireHudUi>(client, mob, mob, ""));
        world.write_message(OpenUi::new::<AntagInfoVampireUi>(
            client,
            mob,
            mob,
            VAMPIRE_NAME,
        ));
    }
    refresh(world, mob);
    info!("{mob} is a vampire");
    true
}

/// `announce_objectives`.
pub(super) fn announce_objectives(world: &mut World, mob: Entity) {
    let Some(lines) = world.get::<Vampire>(mob).map(|v| {
        v.objectives
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

/// `forge_objectives`.
pub(super) fn forge_objectives(
    world: &mut World,
    owner: Entity,
    rng: &mut Rng,
) -> Vec<VampireObjective> {
    // `round(rand(150, 400) / 5) * 5`.
    let target = (rng.range(150, 400) as f64 / 5.0).round() as u32 * 5;
    let mut objectives = vec![VampireObjective::Blood { target }];
    let mut protected = None;
    if rng.prob(5.0) {
        let target = find_target(world, &[owner], &[], assassinate, rng);
        protected = target;
        let text = match target {
            Some(t) => format!("Защитите {}.", target_text(world, t)),
            None => "Свободная задача.".to_owned(),
        };
        objectives.push(VampireObjective::Protect { target, text });
    } else if rng.prob(50.0) {
        objectives.push(VampireObjective::Specialization { text: None });
    } else {
        let targets: Vec<StealItemDef> = world
            .get_resource::<UplinkCatalog>()
            .map(|c| c.data.steal_items.clone())
            .unwrap_or_default();
        let item = find_steal_target(world, &[owner], &[], &targets, rng);
        objectives.push(VampireObjective::Generic(objective(
            world,
            ObjectiveKind::Steal { item },
        )));
    }
    // The protected one is never the kill target.
    let blacklist: Vec<Objective> = protected
        .map(|t| objective(world, assassinate(Some(t))))
        .into_iter()
        .collect();
    let target = find_target(world, &[owner], &blacklist, assassinate, rng);
    objectives.push(VampireObjective::Generic(objective(
        world,
        assassinate(target),
    )));
    objectives.push(VampireObjective::Lair);
    objectives.push(if rng.prob(20.0) {
        VampireObjective::Survive
    } else {
        VampireObjective::Generic(objective(world, ObjectiveKind::Escape))
    });
    objectives
}

pub(super) fn assassinate(target: Option<Entity>) -> ObjectiveKind {
    ObjectiveKind::Assassinate { target }
}

pub(super) fn objective(world: &World, kind: ObjectiveKind) -> Objective {
    Objective {
        explanation: kind.explanation(world),
        kind,
    }
}

/// `check_vampire_upgrade(announce)`: grants what the blood (and specialization) unlocked.
pub fn check_upgrades(world: &mut World, mob: Entity, announce: bool) {
    let Some(pending) = world.get::<Vampire>(mob).map(Vampire::pending_powers) else {
        return;
    };
    if pending.is_empty() {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        for &power in &pending {
            vampire.powers.push(power);
            if let Some(spell) = power.spell() {
                vampire.spells.push(SpellState {
                    spell,
                    ready_at: now,
                    charges: if spell == Spell::Glare {
                        GLARE_CHARGES
                    } else {
                        0
                    },
                    recharges: Vec::new(),
                });
            }
        }
    }
    if announce {
        for power in pending {
            chat_to(world, mob, Span::BoldNotice, power.gain_desc());
        }
    }
    refresh(world, mob);
}

/// The action buttons and the blood counter from the vampire's state (`build_all_button_icons`,
/// `update_blood_hud`). Cost: a handful of icon lookups, on blood changes and each `Life()`.
pub fn refresh(world: &mut World, mob: Entity) {
    let Some(vampire) = world.get::<Vampire>(mob).cloned() else {
        return;
    };
    let ctx = cast_context(world, mob);
    let icon = |(dmi, state): (&str, &str)| {
        world
            .get_resource::<IconIndex>()
            .and_then(|index| index.get(dmi, state))
    };
    let (background, border) = (
        icon((ACTIONS_DMI, "bg_vampire")),
        icon((ACTIONS_DMI, "bg_vampire_border")),
    );
    let buttons: Vec<(ActionButton, f64)> = vampire
        .spells
        .iter()
        .map(|state| {
            let def = state.spell.def();
            let available = match vampire.cast_check(
                state.spell,
                CastContext {
                    now: f64::MAX,
                    ..ctx
                },
            ) {
                Ok(()) => true,
                Err(CastFail::Message(_) | CastFail::Unconscious | CastFail::Missing) => false,
                Err(CastFail::Cooldown) => true,
            };
            let maptext = (state.spell == Spell::Glare && state.charges > 0)
                .then(|| state.charges.to_string());
            (
                ActionButton {
                    id: def.id.to_owned(),
                    name: def.name.to_owned(),
                    icon: icon(def.icon),
                    background,
                    border,
                    available,
                    cooldown: 0.0,
                    active: vampire.aiming == Some(state.spell),
                    maptext,
                },
                state.ready_at,
            )
        })
        .collect();
    set_buttons(world, mob, buttons);
    if let Some(mut hud) = world.get_mut::<VampireHudData>(mob) {
        hud.set_if_neq(VampireHudData {
            usable: vampire.bloodusable,
        });
    }
}

pub(super) fn area_path(world: &World, pos: TilePos) -> Option<String> {
    let area = world.get_resource::<Areas>()?.area_at(pos)?;
    world
        .get::<ss13_mapload::power::DmPower>(area)
        .map(|p| p.path.clone())
}

pub(super) fn in_chapel(world: &World, mob: Entity) -> bool {
    world
        .get::<TilePos>(mob)
        .and_then(|&pos| area_path(world, pos))
        .is_some_and(|p| is_under(&p, CHAPEL))
}

pub(super) fn cast_context(world: &World, mob: Entity) -> CastContext {
    let stat = world.get::<Stat>(mob).copied().unwrap_or(Stat::Dead);
    CastContext {
        now: world.resource::<Time>().elapsed_secs_f64(),
        dead: stat == Stat::Dead,
        conscious: stat == Stat::Conscious,
        in_chapel: in_chapel(world, mob),
    }
}

/// Adds blood from `victim` (`adjust_blood`) and grants what it unlocked. The amount gained.
pub fn gain_blood(world: &mut World, mob: Entity, victim: Option<Entity>, amount: f32) -> f32 {
    let Some(gained) = world
        .get_mut::<Vampire>(mob)
        .map(|mut v| v.adjust_blood(victim, amount))
    else {
        return 0.0;
    };
    check_upgrades(world, mob, true);
    refresh(world, mob);
    gained
}

pub(super) fn subtract_usable(world: &mut World, mob: Entity, amount: f32) {
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        vampire.subtract_usable(amount);
    }
    refresh(world, mob);
}
