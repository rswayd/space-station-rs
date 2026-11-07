//! The vampire's windows and the round end report.

use super::*;

impl ServerUi for AntagInfoVampireUi {
    type Source = &'static Vampire;
    type UserParam = ();
    type Act = Subclass;

    fn static_data(_: &Vampire) -> VampireInfoStatic {
        VampireInfoStatic {
            subclasses: Subclass::ALL.into_iter().map(Subclass::ui).collect(),
        }
    }

    fn data(vampire: &Vampire, _: Entity, _: &()) -> VampireInfoData {
        VampireInfoData {
            total_blood: vampire.bloodtotal,
            specialization_blood_required: 150,
            selected_subclass: vampire.subclass.map(|s| s.id().to_owned()),
            can_select_subclass: vampire.has(Power::UnlockSpecialization),
            objectives: vampire
                .objectives
                .iter()
                .enumerate()
                .map(|(i, o)| ObjectiveInfo {
                    count: i as u32 + 1,
                    explanation: o.explanation(),
                    complete: false,
                })
                .collect(),
        }
    }

    fn parse_act(action: &str, _: &UiParams) -> Result<Subclass, UiReject> {
        Subclass::from_id(action).ok_or(UiReject::UnknownAction)
    }
}

impl ServerUi for VampireHudUi {
    type Source = &'static VampireHudData;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &VampireHudData) {}

    fn data(source: &VampireHudData, _: Entity, _: &()) -> VampireHudData {
        *source
    }
}

/// `/datum/action/antag_info/Trigger` of a vampire.
pub(super) fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if world.get::<Vampire>(source).is_some() {
            world.write_message(OpenUi::new::<AntagInfoVampireUi>(
                client,
                actor,
                source,
                VAMPIRE_NAME,
            ));
        }
    }
}

/// `ui_act` of the info panel: the specialization, once, after 150 blood.
pub(super) fn pick_subclass(
    world: &mut World,
    mut acts: Local<MessageCursor<UiActed<AntagInfoVampireUi>>>,
) {
    let pending: Vec<(Entity, Entity, Entity, Subclass)> = acts
        .read(world.resource::<Messages<UiActed<AntagInfoVampireUi>>>())
        .map(|a| (a.window, a.actor, a.source, a.act))
        .collect();
    for (window, actor, source, subclass) in pending {
        if actor != source {
            continue;
        }
        let allowed = world
            .get::<Vampire>(source)
            .is_some_and(|v| v.subclass.is_none() && v.has(Power::UnlockSpecialization));
        if !allowed {
            continue;
        }
        add_subclass(world, source, subclass);
        // `ui.close()`.
        if let Some(mut window) = world.get_mut::<ss13_tgui::server::UiWindow>(window) {
            window.cap_status(ss13_shared::ui::UiStatus::Close);
        }
    }
}

/// `add_subclass` after the pick: the unlock power is spent, the specialization's powers
/// granted and its objective written.
pub fn add_subclass(world: &mut World, mob: Entity, subclass: Subclass) {
    let mut rng = rng_of(world, mob.to_bits());
    let departments = [
        "службу безопасности",
        "отдел обслуживания",
        "научный отдел",
        "медицинский отдел",
        "инженерный отдел",
        "отдел снабжения",
    ];
    let mut pick = |n: usize| rng.range(0, n as i64 - 1) as usize;
    let objectives = subclass.unique_objectives();
    let text = objectives[pick(objectives.len())]
        .replace("%DEPARTMENT", departments[pick(departments.len())]);
    world.insert_resource(DynamicRng(rng));
    if let Some(mut vampire) = world.get_mut::<Vampire>(mob) {
        vampire.subclass = Some(subclass);
        vampire.spent.push(Power::UnlockSpecialization);
        vampire.powers.retain(|p| *p != Power::UnlockSpecialization);
        for objective in &mut vampire.objectives {
            if let VampireObjective::Specialization { text: t @ None } = objective {
                *t = Some(text.clone());
            }
        }
    }
    info!("{mob} chose the {subclass:?} specialization (vampire)");
    check_upgrades(world, mob, true);
    announce_objectives(world, mob);
    refresh(world, mob);
}

// ---------------------------------------------------------------------------------------------
// Round end

/// tg `antag_report` for vampires: the header, then each vampire's `roundend_report` with the
/// blood it drank.
pub fn roundend_report(world: &mut World, end: RoundEnd) -> Vec<ChatMessage> {
    let mut query = world.query::<(Entity, &Vampire)>();
    let mut vampires: Vec<(Entity, Vampire)> =
        query.iter(world).map(|(e, v)| (e, v.clone())).collect();
    if vampires.is_empty() {
        return Vec::new();
    }
    vampires.sort_by_key(|(e, _)| *e);
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = vec![line(vec![ChatSegment::new(
        Span::Bold,
        format!("{ROUNDEND_CATEGORY} были:"),
    )])];
    for (mob, vampire) in vampires {
        out.push(crate::antag::traitor::print_player(
            world,
            mob,
            &vampire.key,
        ));
        // shortcut: the fork's report has no blood line; its admin panel's
        // (`antag_panel_data`) is shown here.
        let class = vampire.subclass.map_or("N/A", Subclass::name);
        out.push(line(vec![ChatSegment::new(
            Span::Plain,
            format!(
                "Класс: {class} | Всего крови: {} | Жертв: {}",
                blood_text(vampire.bloodtotal),
                vampire.drained.len()
            ),
        )]));
        let mut won = true;
        for (i, objective) in vampire.objectives.iter().enumerate() {
            let done = objective.check_completion(world, mob, &vampire, end);
            won &= done;
            out.push(line(vec![
                ChatSegment::new(Span::Bold, format!("{OBJECTIVE_NAME} #{}", i + 1)),
                ChatSegment::new(Span::Plain, format!(": {} ", objective.explanation())),
                if done {
                    ChatSegment::new(Span::GreenText, "Успех!")
                } else {
                    ChatSegment::new(Span::RedText, "Неудача.")
                },
            ]));
        }
        out.push(line(vec![if won {
            ChatSegment::new(Span::GreenText, format!("{VAMPIRE_NAME} был успешен!"))
        } else {
            ChatSegment::new(Span::RedText, format!("{VAMPIRE_NAME} провалился!"))
        }]));
    }
    out
}
