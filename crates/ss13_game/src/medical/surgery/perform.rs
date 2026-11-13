//! Starting an operation: the pick list, `try_perform`, the preop messages and the `do_after`
//! completion.

use super::*;

/// A list of operations waiting for the surgeon's pick (tg's radial menu).
#[derive(Component, Clone, Debug, PartialEq)]
pub struct SurgeryChoice {
    pub patient: Entity,
    pub tool: Option<Entity>,
    pub zone: TargetZone,
    pub ops: Vec<Op>,
}

/// The server side of `ListInputWindow` for the operation pick.
pub struct SurgeryListWindow;

impl UiInterface for SurgeryListWindow {
    const NAME: &'static str = ListInputUi::NAME;
    type Data = ListInputData;
    type StaticData = ();
}

/// `submit` (`entry`) or `cancel`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ListAct {
    Submit(String),
    Cancel,
}

impl ServerUi for SurgeryListWindow {
    type Source = &'static SurgeryChoice;
    type UserParam = ();
    type Act = ListAct;

    fn static_data(_: &SurgeryChoice) {}

    fn data(choice: &SurgeryChoice, _: Entity, _: &()) -> ListInputData {
        let items: Vec<String> = choice
            .ops
            .iter()
            .map(|op| op.def().name.to_owned())
            .collect();
        ListInputData {
            title: "Операция".into(),
            message: String::new(),
            init_value: items.first().cloned().unwrap_or_default(),
            items,
            timeout: None,
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<ListAct, UiReject> {
        match action {
            "submit" => params
                .get("entry")
                .map(|e| ListAct::Submit(e.to_owned()))
                .ok_or_else(|| UiReject::BadParams("entry: expected an item".into())),
            "cancel" => Ok(ListAct::Cancel),
            _ => Err(UiReject::UnknownAction),
        }
    }
}

/// tg `perform_surgery` from a click of `surgeon` (holding `tool`) on `mob`. True when the click
/// was the surgery's (an operation started, a list opened, or a surgical tool was stopped from
/// stabbing the patient).
pub fn perform_surgery(world: &mut World, actor: Actor, mob: Entity) -> bool {
    if world.get::<DoAfter>(actor.mob).is_some() {
        tell(
            world,
            actor.client,
            Span::Warning,
            "операция уже проводится!",
        );
        return true;
    }
    let zone = world
        .get::<ZoneSelected>(actor.mob)
        .copied()
        .unwrap_or_default()
        .0;
    let tool = ToolInfo::read(world, actor.item);
    let pool = pool_for(world, mob);
    let ops = available_ops(world, &pool, mob, actor.mob, zone, &tool);
    match ops.as_slice() {
        [] => {
            let Some(item) = actor.item else {
                return false;
            };
            if !world
                .get::<DmWounding>(item)
                .is_some_and(|w| w.surgical_tool)
            {
                return false;
            }
            let Some(p) = patient(world, mob) else {
                return false;
            };
            if !world
                .get::<Limbs>(mob)
                .is_some_and(|l| l.limb(zone.check_zone()).prepped)
                && world
                    .get::<super::super::stasis::FreeOperation>(mob)
                    .is_none()
            {
                return false;
            }
            let text = if p.covered & crate::chem::body_part_bit(zone.check_zone()) != 0 {
                "операционная зона перекрыта!"
            } else if !p.lying {
                "не лежит!"
            } else {
                "ничего не сделать с данным инструментом!"
            };
            tell(world, actor.client, Span::Warning, text);
            true
        }
        [op] => {
            try_perform(world, actor, mob, *op, zone);
            true
        }
        _ => {
            world.entity_mut(actor.mob).insert(SurgeryChoice {
                patient: mob,
                tool: actor.item,
                zone,
                ops,
            });
            world.write_message(OpenUi::new::<SurgeryListWindow>(
                actor.client,
                actor.mob,
                actor.mob,
                "Операция",
            ));
            true
        }
    }
}

/// `op` is still offered to `surgeon` (the pool may have changed: the patient left the table)
/// and available: tg `operate_check`.
pub(super) fn offered(
    world: &World,
    op: Op,
    mob: Entity,
    surgeon: Entity,
    zone: TargetZone,
    tool: &ToolInfo,
) -> bool {
    pool_for(world, mob).contains(&op) && available(world, op, mob, surgeon, zone, tool)
}

/// tg `try_perform` up to its `do_after`.
pub(crate) fn try_perform(world: &mut World, actor: Actor, mob: Entity, op: Op, zone: TargetZone) {
    let tool = ToolInfo::read(world, actor.item);
    if !offered(world, op, mob, actor.mob, zone, &tool) {
        return;
    }
    preop(world, actor, mob, op, zone);
    let delay = op.def().time
        * speed(world, op, mob, actor.mob, &tool).min(SURGERY_MODIFIER_FAILURE_THRESHOLD);
    do_after(
        world,
        actor,
        mob,
        delay,
        0,
        crate::construction::Job::Medical(MedJob::Surgery { op, zone }),
    );
}

pub(super) fn speed(world: &World, op: Op, mob: Entity, surgeon: Entity, tool: &ToolInfo) -> f32 {
    let location = world.get::<TilePos>(mob).map_or(2.0, |&pos| {
        let paths: Vec<String> = world
            .resource::<ss13_world::SpatialIndex>()
            .on_tile(pos)
            .iter()
            .filter_map(|&e| crate::construction::path_of(world, e))
            .collect();
        location_modifier(&paths)
    });
    let painless = patient(world, mob).is_some_and(|p| p.painless);
    speed_modifier(
        tool_quality(op, tool),
        surgeon_speed_mod(drunkenness(world, surgeon)),
        location,
        patient_speed_mod(painless),
    )
}

pub(super) fn zone_words(op: Op, zone: TargetZone) -> String {
    match op {
        Op::Amputate | Op::AmputateMechanic => zone.check_zone().ru().1.into(),
        _ => super::super::wounds::zone_prepositional(zone.check_zone()).into(),
    }
}

pub(super) fn item_name(world: &World, item: Option<Entity>) -> String {
    item.map(|i| name_of(world, i)).unwrap_or_default()
}

/// tend wounds' word for what it treats.
pub(super) fn wound_words(heal: Heal) -> &'static str {
    match heal {
        Heal::Combo => "раны",
        Heal::Brute => "ушибы",
        Heal::Burn => "ожоги",
    }
}

/// tg `on_preop` (`display_results` reaches only adjacent mobs) and `display_pain`.
pub(super) fn preop(world: &mut World, actor: Actor, mob: Entity, op: Op, zone: TargetZone) {
    let (user, target) = (name_of(world, actor.mob), name_of(world, mob));
    let tool = item_name(world, actor.item);
    let place = zone_words(op, zone);
    let (own, others, hurt) = match op {
        Op::TendWounds(_, heal) => {
            let what = wound_words(heal);
            (
                format!("Вы пытаетесь наложить швы на {what} у {target}."),
                format!("{user} пытается наложить швы на {what} у {target}."),
                format!("Ваши {what} адски болят!"),
            )
        }
        Op::Revival => (
            format!("Вы готовитесь дать мозгу {target} искру жизни с помощью {tool}."),
            format!("{user} готовится дать мозгу {target} искру жизни с помощью {tool}."),
            String::new(),
        ),
        Op::CoronaryBypass => (
            format!("Вы начинаете устанавливать шунт на сердце {target}..."),
            format!("{user} начинает устанавливать шунт на сердце {target}."),
            "Боль в груди невыносима! Вы едва можете это терпеть!".into(),
        ),
        Op::BrainSurgery => (
            format!("Вы начинаете лечить мозг {target}..."),
            format!("{user} начинает лечить мозг {target}."),
            "Ваша голова пульсирует от невообразимой боли!".into(),
        ),
        Op::Lobotomy => (
            format!("Вы начинаете проводить лоботомию мозга {target}..."),
            format!("{user} начинает проводить лоботомию мозга {target}."),
            "Ваша голова пульсирует от невообразимой боли!".into(),
        ),
        Op::Brainwash | Op::BrainwashSleeper => (
            format!("Вы начинаете промывание мозгов у {target}..."),
            format!("{user} начинает лечить мозг {target}."),
            "Твоя голова раскалывается от невообразимой боли!".into(),
        ),
        Op::Pacify => (
            format!("Вы начинаете пацификацию {target}..."),
            format!("{user} начинает оперировать мозг {target}."),
            "Ваша голова разрывается от невообразимой боли!".into(),
        ),
        Op::RemoveOrgan(slot) => {
            let organ = slot.spec().ru.0;
            (
                format!("Вы начинаете извлекать {organ} у {target}..."),
                format!("{user} начинает извлекать {organ} у {target}."),
                format!("Вы чувствуете тянущее ощущение в {place}!"),
            )
        }
        Op::RemoveImplant(slot) => {
            let implant = cyberimps::implant_in(world, mob, slot)
                .map(|i| name_of(world, i))
                .unwrap_or_default();
            (
                format!("Вы начинаете извлекать {implant} у {target}..."),
                format!("{user} начинает извлекать {implant} у {target}."),
                format!("Вы чувствуете тянущее ощущение в {place}!"),
            )
        }
        Op::InsertOrgan(_) | Op::InsertImplant => (
            format!("Вы начинаете вставлять {tool} в {target}..."),
            format!("{user} начинает вставлять {tool} в {target}."),
            format!(
                "Вы чувствуете, как что-то помещают в вашу {}!",
                zone.check_zone().ru().1
            ),
        ),
        Op::AttachLimb => (
            format!(
                "Вы начинаете заменять отсутствующую {} у {target}...",
                zone.check_zone().ru().1
            ),
            format!(
                "{user} начинает заменять отсутствующую {} у {target}.",
                zone.check_zone().ru().1
            ),
            format!(
                "Вы испытываете неприятное ощущение там, где должна быть ваша {}!",
                zone.check_zone().ru().0
            ),
        ),
        Op::Lobectomy => (
            format!("Вы начинаете делать надрез в легких {target}..."),
            format!("{user} начинает делать надрез у {target}."),
            "Вы чувствуете колющую боль в груди!".into(),
        ),
        Op::Hepatectomy => (
            format!("Вы начинаете вырезать поврежденный фрагмент печени {target}..."),
            format!("{user} начинает делать надрез у {target}."),
            "Ваш живот горит от ужасной колющей боли!".into(),
        ),
        Op::Gastrectomy => (
            format!("Вы начинаете вырезать поврежденный фрагмент желудка {target}..."),
            format!("{user} начинает делать надрез у {target}."),
            "Вы чувствуете ужасную колющую боль в животе!".into(),
        ),
        Op::EarSurgery => (
            format!("Вы начинаете лечить уши {target}..."),
            format!("{user} начинает лечить уши {target}."),
            "Вы чувствуете головокружительную боль в голове!".into(),
        ),
        Op::EyeSurgery => (
            format!("Вы начинаете лечить глаза {target}..."),
            format!("{user} начинает лечить глаза {target}."),
            "Вы чувствуете колющую боль в глазах!".into(),
        ),
        Op::Debride => (
            format!("Вы начинаете удалять зараженную плоть с {place} у {target}..."),
            format!(
                "{user} начинает удалять зараженную плоть с {place} у {target} с помощью {tool}."
            ),
            format!(
                "Инфекция в вашей {place} приносит адскую боль! Такое чувство, что вас режут \
                 ножом!"
            ),
        ),
        Op::RepairPuncture => (
            format!(
                "Вы начинаете соединять разорванные кровеносные сосуды в {place} у {target}..."
            ),
            format!(
                "{user} начинает соединять разорванные кровеносные сосуды в {place} у {target} с \
                 помощью {tool}."
            ),
            format!("Вы чувствуете ужасную колющую боль в вашей {place}!"),
        ),
        Op::SealVeins => (
            format!(
                "Вы начинаете прижигать соединенные кровеносные сосуды в {place} у {target}..."
            ),
            format!(
                "{user} начинает прижигать соединенные кровеносные сосуды в {place} у {target} с \
                 помощью {tool}."
            ),
            format!("Вы чувствуете жжение в вашей {place}!"),
        ),
        Op::Bioware(kind) => {
            let [own, others, _, _, hurt, _] = bioware_lines(kind, &target, &user);
            (own, others, hurt)
        }
        Op::Autopsy | Op::AutopsyMechanic => (
            format!("Вы начинаете проводить вскрытие у {target}..."),
            format!("{user} использует {tool} для проведения вскрытия у {target}."),
            String::new(),
        ),
        Op::Dissection => (
            format!("Вы начинаете вскрывать {target}..."),
            format!("{user} начинает вскрывать {target}."),
            String::new(),
        ),
        Op::ViralBonding => (
            format!("Вы начинаете подогревать костный мозг {target} с помощью {tool}..."),
            format!("{user} начинает подогревать костный мозг {target} с помощью {tool}..."),
            "Вы чувствуете, как жгучее тепло разливается по вашей груди!".into(),
        ),
        Op::Bionecrosis => (
            format!("Вы начинаете выращивать Ромерол-опухоль в мозге у {target}..."),
            format!("{user} начинает копаться в мозге у {target}..."),
            "Ваша голова разрывается от невообразимой боли!".into(),
        ),
        Op::TuneVocalCords => (
            format!("Вы начинаете настраивать голосовые связки {target}..."),
            format!("{user} начинает настраивать голосовые связки {target}."),
            String::new(),
        ),
        Op::FilterBlood => (
            format!("Вы начинаете оперировать {target}..."),
            format!("{user} начинает оперировать {target}."),
            "Вы чувствуете пульсирующую боль в груди!".into(),
        ),
        _ => {
            let verb = op.def().verb;
            (
                format!("Вы начинаете {verb} {place} у {target}..."),
                format!("{user} начинает {verb} {place} у {target}."),
                "Вы чувствуете острую боль, когда вас оперируют!".into(),
            )
        }
    };
    visible_range(world, actor.mob, &own, &others, Span::Notice, 1);
    pain(world, mob, &hurt);
}

/// tg `display_pain`: nothing for the unconscious; a dull tingle under analgesia, and maybe for
/// a drunk patient (its drunkenness as a percent, at most 90).
pub(super) fn pain(world: &mut World, mob: Entity, text: &str) {
    if text.is_empty() {
        return;
    }
    let Some(p) = patient(world, mob) else {
        return;
    };
    if p.unconscious {
        return;
    }
    let Some(client) = world.get::<PlayerMob>(mob).map(|p| p.client) else {
        return;
    };
    let drunk = drunkenness(world, mob);
    let dulled =
        p.painless || (drunk > 0.0 && super::super::fork_rng(world).chance(drunk.clamp(0.0, 90.0)));
    if dulled {
        tell(
            world,
            client,
            Span::Notice,
            "Вы испытываете притупленное покалывание, пока ваше тело оперируют.",
        );
    } else {
        tell(world, client, Span::BoldWarning, text);
    }
}

/// The surgeon picked an entry of the list (or closed it).
pub(super) fn pick_operation(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<SurgeryListWindow>>>,
) {
    let picked: Vec<(Entity, Entity, ListAct)> = acts
        .read(world.resource::<Messages<UiActed<SurgeryListWindow>>>())
        .filter(|a| a.actor == a.source)
        .map(|a| (a.client, a.actor, a.act.clone()))
        .collect();
    for (client, surgeon, act) in picked {
        let Some(choice) = world.get::<SurgeryChoice>(surgeon).cloned() else {
            continue;
        };
        world.entity_mut(surgeon).remove::<SurgeryChoice>();
        let ListAct::Submit(entry) = act else {
            continue;
        };
        // tg `show_radial_menu` `custom_check`: the same tool, still next to the patient.
        let held = world
            .get::<Inventory>(surgeon)
            .and_then(Inventory::active_item);
        let near = match (
            world.get::<TilePos>(surgeon),
            world.get::<TilePos>(choice.patient),
        ) {
            (Some(a), Some(b)) => a.chebyshev(*b).is_some_and(|d| d <= 1),
            _ => false,
        };
        let Some(&op) = choice.ops.iter().find(|op| op.def().name == entry) else {
            continue;
        };
        // tg `can_perform_action` of the radial's check: a surgeon knocked out or stunned
        // while the list was open picks nothing.
        let able = world.get::<Stat>(surgeon) == Some(&Stat::Conscious)
            && !crate::status_effects::incapacitation(world, surgeon).incapacitated;
        if held != choice.tool || !near || !able {
            continue;
        }
        let actor = Actor {
            client,
            mob: surgeon,
            item: held,
        };
        // `brainwash/pre_preop`: the directive is asked first (`crate::antag::brainwashed`).
        if op == Op::Brainwash {
            crate::antag::brainwashed::ask_directive(world, actor, choice.patient, choice.zone);
            continue;
        }
        try_perform(world, actor, choice.patient, op, choice.zone);
    }
}

/// The list closes when the pick is gone (made, or the surgeon moved on).
pub(super) fn close_stale_lists(
    mut windows: Query<&mut UiWindow, With<ss13_tgui::server::UiKind<SurgeryListWindow>>>,
    choices: Query<(), With<SurgeryChoice>>,
) {
    for mut window in &mut windows {
        if !choices.contains(window.source) {
            window.cap_status(UiStatus::Close);
        }
    }
}

/// The end of an operation's `do_after`: `operate_check`, the failure roll, the effects, the
/// stack unit, the loop.
pub fn complete(world: &mut World, surgeon: Entity, action: &DoAfter, op: Op, zone: TargetZone) {
    let mob = action.target;
    let actor = Actor {
        client: action.client,
        mob: surgeon,
        item: action.item,
    };
    let tool = ToolInfo::read(world, actor.item);
    if !offered(world, op, mob, surgeon, zone, &tool) {
        return;
    }
    let speed = speed(world, op, mob, surgeon, &tool);
    let painless = patient(world, mob).is_some_and(|p| p.painless);
    let light = world
        .get::<TilePos>(mob)
        .and_then(|&p| crate::botany::lumcount(world, p));
    let chance = failure_chance(op.def().time, speed, painless, light, tool_mod(op, &tool));
    let failed = super::super::fork_rng(world).chance(chance.clamp(0.0, 99.0));
    if failed {
        failure(world, actor, mob, op, zone, speed);
    } else {
        success(world, actor, mob, op, zone);
    }
    // `isstack(tool)`: one unit per step.
    if let Some(item) = actor.item
        && world.get::<crate::tools::Stack>(item).is_some()
    {
        use_stack(world, item, 1);
    }
    refresh_patient(world, mob);
    // `OPERATION_LOOPING` while `can_loop`.
    let still_holding = actor
        .item
        .is_none_or(|i| world.get_entity(i).is_ok() && stack_amount(world, i) > 0);
    let tool = ToolInfo::read(world, actor.item);
    if op.def().flags & flags::LOOPING != 0
        && still_holding
        && can_loop(world, op, mob)
        && offered(world, op, mob, surgeon, zone, &tool)
    {
        let delay = op.def().time
            * self::speed(world, op, mob, surgeon, &tool).min(SURGERY_MODIFIER_FAILURE_THRESHOLD);
        do_after(
            world,
            actor,
            mob,
            delay,
            0,
            crate::construction::Job::Medical(MedJob::Surgery { op, zone }),
        );
    }
}
