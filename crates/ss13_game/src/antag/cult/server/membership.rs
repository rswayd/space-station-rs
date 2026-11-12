//! Gaining and losing the cult, the eyes and halos, and the action buttons.

use super::*;

/// The roundstart ruleset's `execute`: a new team, every selected player a cultist with
/// equipment, the first one its leader, then `setup_objectives` and the greetings.
pub fn start_cult(world: &mut World, mobs: &[Entity]) {
    world.insert_resource(CultTeam::default());
    let mut added = Vec::new();
    for &mob in mobs {
        if gain(world, mob, true, false) {
            added.push(mob);
        }
    }
    if let Some(&leader) = added.first() {
        make_cult_leader(world, leader);
    }
    setup_objectives(world);
    for mob in added {
        greet(world, mob);
    }
}

/// `add_antag_datum(/datum/antagonist/cult)` (admins: without equipment; the team and its
/// objectives are created with the first cultist). `false` if it already is one or cannot be.
pub fn add_cultist(world: &mut World, mob: Entity) -> bool {
    join_cult(world, mob, false, CultForm::Cultist)
}

/// [`add_cultist`] with the roundstart equipment (`give_equipment`: the dagger and runed metal).
pub fn add_cultist_equipped(world: &mut World, mob: Entity) -> bool {
    join_cult(world, mob, true, CultForm::Cultist)
}

/// `add_antag_datum(/datum/antagonist/cult/shade)` or `/construct`.
pub fn add_cult_minion(world: &mut World, mob: Entity, form: CultForm) -> bool {
    join_cult(world, mob, false, form)
}

pub(super) fn join_cult(world: &mut World, mob: Entity, equipment: bool, form: CultForm) -> bool {
    let fresh = !world.contains_resource::<CultTeam>();
    if fresh {
        world.insert_resource(CultTeam::default());
    }
    if !gain(world, mob, equipment, false) {
        return false;
    }
    if let Some(mut cultist) = world.get_mut::<Cultist>(mob) {
        cultist.form = form;
    }
    if fresh {
        setup_objectives(world);
    }
    greet(world, mob);
    true
}

/// `on_gain` + `apply_innate_effects`.
pub(super) fn gain(world: &mut World, mob: Entity, equipment: bool, converted: bool) -> bool {
    if world.get::<Cultist>(mob).is_some() || world.get::<Health>(mob).is_none() {
        return false;
    }
    let client = world.get::<PlayerMob>(mob).map(|p| p.client);
    let key = client
        .and_then(|c| world.get::<ClientToken>(c))
        .map(|t| t.0.clone())
        .unwrap_or_default();
    let hud_icon = icon(world, ANTAG_HUD_DMI, "cult");
    world.entity_mut(mob).insert((
        Cultist {
            key,
            converted,
            ..default()
        },
        AntagHudData {
            hud_icon,
            ..default()
        },
    ));
    let (risen, ascendent) = {
        let mut team = world.resource_mut::<CultTeam>();
        team.members.push(mob);
        team.size_at_maximum += 1;
        (team.cult_risen, team.cult_ascendent)
    };
    if equipment {
        equip_cultist(world, mob);
    }
    ss13_chat::language::edit_holder(world, mob, |h| {
        h.grant(
            ss13_chat::language::LanguageId::Narsie,
            ss13_chat::language::ALL,
            ss13_chat::language::LanguageSource::Cultist,
        );
    });
    if risen {
        set_eyes(world, mob, true);
    }
    // `cult_team.rise`/`ascend` mark humans only.
    if ascendent && world.get::<HumanLook>(mob).is_some() {
        set_halo(world, mob, true);
    }
    if let Some(client) = client {
        world.write_message(OpenUi::new::<AntagHudUi>(client, mob, mob, ""));
    }
    refresh(world, mob);
    info!("{mob} joined the blood cult");
    true
}

/// `greet()`: the name and the objectives, then the panel.
pub(super) fn greet(world: &mut World, mob: Entity) {
    let name = antag_name(world, mob);
    chat_to(world, mob, Span::Bold, format!("Вы - {name}."));
    let lines: Vec<String> = world
        .resource::<CultTeam>()
        .objectives
        .iter()
        .enumerate()
        .map(|(i, o)| format!("{OBJECTIVE_NAME} #{}: {}", i + 1, o.explanation()))
        .collect();
    chat_to(world, mob, Span::Notice, "Ваши текущие задачи:");
    for line in lines {
        chat_to(world, mob, Span::Plain, line);
    }
    update_info(world);
    if let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) {
        world.write_message(OpenUi::new::<AntagInfoGenericUi>(client, mob, mob, name));
    }
}

pub(super) fn antag_name(world: &World, mob: Entity) -> &'static str {
    match world.get::<Cultist>(mob) {
        Some(c) if c.form == CultForm::Construct => CONSTRUCT_NAME,
        Some(c) if matches!(c.form, CultForm::Shade { .. }) => SHADE_NAME,
        Some(c) if c.converted => CONVERTED_NAME,
        _ => CULTIST_NAME,
    }
}

/// `equip_cultist(metal = TRUE)`.
pub(super) fn equip_cultist(world: &mut World, mob: Entity) {
    for path in [DAGGER, "/obj/item/stack/sheet/runed_metal/ten"] {
        let name = path.rsplit('/').next().unwrap_or_default().to_owned();
        match crate::antag::brother::equip_conspicuous_item(world, mob, path) {
            Some(slot) => {
                let item = match path {
                    DAGGER => "ritual dagger",
                    _ => "runed metal",
                };
                chat_to(
                    world,
                    mob,
                    Span::Danger,
                    format!("{item} был помещен в ваш {slot}."),
                );
            }
            None => chat_to(
                world,
                mob,
                Span::BoldWarning,
                format!(
                    "К сожалению, вы не смогли получить {name}. Это очень плохо и вам стоит \
                     написать админхелп (нажмите F1)."
                ),
            ),
        }
    }
    chat_to(
        world,
        mob,
        Span::Notice,
        "Это поможет вам организовать культ на станции. Используйте их с пользой, и запомните - \
         вы не одни.",
    );
}

/// `make_cult_leader`.
pub(super) fn make_cult_leader(world: &mut World, mob: Entity) {
    if world.resource::<CultTeam>().leader.is_some() {
        return;
    }
    world.resource_mut::<CultTeam>().leader = Some(mob);
    let master = icon(world, ANTAG_HUD_DMI, "cultmaster");
    if let Some(mut hud) = world.get_mut::<AntagHudData>(mob) {
        hud.hud_icon = master;
    }
    let name = name_of(world, mob);
    let members = world.resource::<CultTeam>().members.clone();
    for m in members.into_iter().filter(|&m| m != mob) {
        chat_to(
            world,
            m,
            Span::CultLarge,
            format!(
                "{name} ваш Мастер культа! Выполняйте приказы Мастера насколько это в ваших силах!"
            ),
        );
    }
    chat_to(
        world,
        mob,
        Span::CultLarge,
        "Вы - Мастер культа. Как Мастер культа у вас громче голос при коммуникации и вы можете \
         помечать цели, такие как локацию и не культиста, чтобы направить культ туда. Вы можете \
         призывать живых культистов к вашей локации единожды. Используйте свои способности для \
         победы любой ценой.",
    );
}

/// `setup_objectives`: the sacrifice target and the summoning areas.
pub(super) fn setup_objectives(world: &mut World) {
    let mut rng = rng(world, 0x0c17);
    let target = find_sacrifice_target(world, &mut rng);
    let sacrifice = match target {
        Some(t) => CultObjective::Sacrifice {
            target: Some(t),
            name: name_of(world, t),
            job: job_name_ru(world, t),
            sacced: false,
        },
        // "Could not find unconvertible or convertible target. WELP!"
        None => CultObjective::Sacrifice {
            target: None,
            name: String::new(),
            job: String::new(),
            sacced: true,
        },
    };
    let spots = summon_spots(world, &mut rng);
    world.insert_resource(DynamicRng(rng));
    world.resource_mut::<CultTeam>().objectives = vec![
        sacrifice,
        CultObjective::Eldergod {
            spots,
            summoned: false,
        },
    ];
}

/// `sacrifice/find_target`: a living non-cultist human player, unconvertible ones first.
pub(super) fn find_sacrifice_target(world: &mut World, rng: &mut Rng) -> Option<Entity> {
    let mut query = world.query_filtered::<(Entity, &Stat), (With<PlayerMob>, With<HumanLook>)>();
    let mut alive: Vec<Entity> = query
        .iter(world)
        .filter(|(e, s)| **s != Stat::Dead && world.get::<Cultist>(*e).is_none())
        .map(|(e, _)| e)
        .collect();
    alive.sort();
    let unconvertible: Vec<Entity> = alive
        .iter()
        .copied()
        .filter(|&e| !is_convertable(world, e))
        .collect();
    let pool = if unconvertible.is_empty() {
        alive
    } else {
        unconvertible
    };
    (!pool.is_empty()).then(|| pool[rng.range(0, pool.len() as i64 - 1) as usize])
}

/// `eldergod/New`: three station areas with `VALID_TERRITORY`.
pub(super) fn summon_spots(world: &mut World, rng: &mut Rng) -> Vec<(Entity, String)> {
    let Some(areas) = world.get_resource::<Areas>() else {
        return Vec::new();
    };
    let mut candidates: Vec<(Entity, String)> = areas
        .entities()
        .iter()
        .copied()
        .filter(|&a| {
            world.get::<DmPower>(a).is_some_and(|p| {
                p.path.starts_with("/area/station") && (p.area_flags as u32) & VALID_TERRITORY != 0
            })
        })
        .map(|a| {
            let name = world
                .get::<DmName>(a)
                .map(|n| {
                    n.name
                        .trim_start_matches("\\improper ")
                        .trim_start_matches("\\proper ")
                        .to_owned()
                })
                .unwrap_or_default();
            (a, name)
        })
        .collect();
    candidates.sort_by_key(|(a, _)| *a);
    let mut spots = Vec::new();
    while spots.len() < SUMMON_POSSIBILITIES && !candidates.is_empty() {
        let i = rng.range(0, candidates.len() as i64 - 1) as usize;
        spots.push(candidates.remove(i));
    }
    spots
}

/// `on_removal` + `remove_innate_effects`: the old faith returns.
pub fn remove_cultist(world: &mut World, mob: Entity, silent: bool) {
    let Some(cultist) = world.get::<Cultist>(mob).cloned() else {
        return;
    };
    if !silent {
        let name = name_of(world, mob);
        visible(
            world,
            mob,
            "Незнакомый белый свет вспыхивает в твоем сознании, очищая от скверны Геометра и всех \
             твоих воспоминаний о том, что ты был её слугой.",
            &format!("Старая вера вернулась к {name}!"),
            Span::BoldWarning,
        );
    }
    drop_auras(world, mob);
    if let Some(mut holder) = world.get_mut::<ss13_chat::language::LanguageHolder>(mob) {
        holder.remove(
            ss13_chat::language::LanguageId::Narsie,
            ss13_chat::language::ALL,
            Some(ss13_chat::language::LanguageSource::Cultist),
        );
    }
    if cultist.own_eyes.is_some() {
        set_eyes(world, mob, false);
    }
    set_halo(world, mob, false);
    world
        .entity_mut(mob)
        .remove::<(Cultist, AntagHudData, CultPrompt, CultInfo)>();
    set_buttons(world, mob, Vec::new());
    if let Some(mut team) = world.get_resource_mut::<CultTeam>() {
        team.members.retain(|&m| m != mob);
        if team.leader == Some(mob) {
            team.leader = None;
        }
    }
    info!("{mob} renounced the cult of Nar'Sie");
}

/// `/datum/element/cult_eyes`: red eyes (or back to the own color).
pub(super) fn set_eyes(world: &mut World, mob: Entity, on: bool) {
    let Some(look) = world.get::<HumanLook>(mob).cloned() else {
        return;
    };
    let restore = world.get::<Cultist>(mob).and_then(|c| c.own_eyes);
    let color = if on {
        if let Some(mut c) = world.get_mut::<Cultist>(mob)
            && c.own_eyes.is_none()
        {
            c.own_eyes = Some(look.eye_color);
        }
        BLOODCULT_EYE
    } else {
        match restore {
            Some(c) => c,
            None => return,
        }
    };
    if let Some(mut l) = world.get_mut::<HumanLook>(mob) {
        l.eye_color = color;
    }
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.set_changed();
    }
}

/// `/datum/status_effect/cult_halo` on or off.
pub(super) fn set_halo(world: &mut World, mob: Entity, on: bool) {
    let had = world.get::<CultHalo>(mob).is_some();
    if on == had {
        return;
    }
    if on {
        let n = world
            .get_resource_mut::<crate::combat::CombatRng>()
            .map_or(1, |mut r| r.range(1, 6) as u8);
        world.entity_mut(mob).insert(CultHalo(n));
    } else {
        world.entity_mut(mob).remove::<CultHalo>();
    }
    if let Some(mut inventory) = world.get_mut::<Inventory>(mob) {
        inventory.set_changed();
    }
}

/// The halo overlay for `crate::inventory`'s look refresh.
pub fn halo_overlay(icons: &IconIndex, halo: CultHalo) -> Option<(f32, ss13_shared::IconStateId)> {
    // `-HALO_LAYER`: above everything else on the mob.
    icons
        .get(HALO_DMI, &format!("halo{}", halo.0))
        .map(|id| (1.1, id))
}

/// `check_size`: eyes at the risen ratio, halos at the ascendent one.
pub fn check_size(world: &mut World) {
    let Some(team) = world.get_resource::<CultTeam>() else {
        return;
    };
    if team.cult_ascendent {
        return;
    }
    let mut query = world.query_filtered::<(Entity, &Stat), With<PlayerMob>>();
    let (mut cultists, mut alive) = (0u32, 0u32);
    for (e, stat) in query.iter(world) {
        if *stat == Stat::Dead {
            continue;
        }
        if world.get::<Cultist>(e).is_some() {
            cultists += 1;
        } else {
            alive += 1;
        }
    }
    if cultists == 0 {
        return;
    }
    let (risen, ascendent) = cult_stage(cultists, alive);
    let team = world.resource::<CultTeam>().clone();
    if risen && !team.cult_risen {
        world.resource_mut::<CultTeam>().cult_risen = true;
        for &m in &team.members {
            chat_to(
                world,
                m,
                Span::CultLarge,
                "Покров слабеет с ростом культа - ваши глаза начинают светиться...",
            );
            set_eyes(world, m, true);
        }
        info!("the blood cult has risen with {cultists} players");
    }
    if ascendent && !team.cult_ascendent {
        world.resource_mut::<CultTeam>().cult_ascendent = true;
        for &m in &team.members {
            chat_to(
                world,
                m,
                Span::CultLarge,
                "Ваш культ набирает силу, и приближается красная жатва - вы не сможете долго \
                 скрывать свою истинную природу!!",
            );
            set_halo(world, m, true);
        }
        let percent = (cultists as f32 / alive.max(1) as f32 * 100.0).floor() as u32;
        crate::shuttle::priority_announce(
            world,
            &format!(
                "{}: Отдел паранормальных явлений",
                crate::events::COMMAND_NAME
            ),
            "",
            &format!(
                "Мы фиксируем активность из другого измерения, связаную с культом \"Нар'Си\" на \
                 вашей станции. Согласно нашей информации, {percent}% экипажа станции были \
                 порабощены культом. Сотрудники службы безопасности наделены правом \
                 беспрепятственно применять летальную силу против культистов. Остальному экипажу \
                 надлежит приготовиться защищать себя и свои отделы, не ведя охоту на культистов. \
                 Погибшие члены экипажа должны быть реанимированы и деконвертированы, как только \
                 ситуация будет взята под контроль."
            ),
        );
        info!("the blood cult has ascended with {cultists} players");
    }
}

// ---------------------------------------------------------------------------------------------
// Buttons

pub(super) const COMM_ID: &str = "cult_comm";
pub(super) const MAGIC_ID: &str = "cult_blood_magic";
pub(super) const SPELL_PREFIX: &str = "cult_spell_";

/// The cultist's buttons: Communion, Prepare Blood Magic and the prepared spells.
pub fn refresh(world: &mut World, mob: Entity) {
    let Some(cultist) = world.get::<Cultist>(mob).cloned() else {
        return;
    };
    let (background, border) = (
        icon(world, BACKGROUNDS_DMI, "bg_demon"),
        icon(world, BACKGROUNDS_DMI, "bg_demon_border"),
    );
    let in_hand: Vec<u32> = world
        .get::<Inventory>(mob)
        .map(|i| {
            i.hands
                .iter()
                .flatten()
                .filter_map(|&h| world.get::<BloodAura>(h).map(|a| a.spell))
                .collect()
        })
        .unwrap_or_default();
    let button = |id: String, name: &str, icon, maptext: Option<String>, active: bool| {
        (
            ActionButton {
                id,
                name: name.to_owned(),
                icon,
                background,
                border,
                available: true,
                cooldown: 0.0,
                active,
                maptext,
            },
            0.0,
        )
    };
    let mut buttons = vec![button(
        COMM_ID.into(),
        "Communion",
        icon(world, ACTIONS_DMI, "cult_comms"),
        None,
        false,
    )];
    // `apply_innate_effects`: blood magic for humans only.
    if world.get::<HumanLook>(mob).is_some() {
        buttons.push(button(
            MAGIC_ID.into(),
            "Prepare Blood Magic",
            icon(world, ACTIONS_DMI, "carve"),
            None,
            cultist.carving.is_some(),
        ));
    }
    for spell in &cultist.spells {
        let def = spell.spell.def();
        buttons.push(button(
            format!("{SPELL_PREFIX}{}", spell.id),
            def.name,
            icon(world, ACTIONS_DMI, def.icon),
            (def.charges > 1).then(|| spell.charges.to_string()),
            in_hand.contains(&spell.id),
        ));
    }
    set_buttons(world, mob, buttons);
}

pub(super) fn trigger_buttons(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<ActionButtonsUi>>>,
) {
    let pending: Vec<(Entity, Entity, Entity, String)> = acts
        .read(world.resource::<Messages<UiActed<ActionButtonsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, source, id) in pending {
        if actor != source
            || world.get::<PlayerMob>(actor).map(|p| p.client) != Some(client)
            || world.get::<Cultist>(actor).is_none()
        {
            continue;
        }
        // `AB_CHECK_CONSCIOUS|AB_CHECK_INCAPACITATED`.
        if !can_act(world, actor) {
            continue;
        }
        if id == COMM_ID {
            open_text(world, client, actor, TextKind::Communion);
        } else if id == MAGIC_ID {
            prepare_magic(world, client, actor);
        } else if let Some(n) = id
            .strip_prefix(SPELL_PREFIX)
            .and_then(|n| n.parse::<u32>().ok())
        {
            activate_spell(world, client, actor, n);
        }
    }
}
