//! The changeling's windows and the round end report.

use super::*;

impl ServerUi for AntagInfoChangelingUi {
    type Source = &'static Changeling;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &Changeling) {}

    fn data(ling: &Changeling, _: Entity, _: &()) -> ChangelingInfo {
        ChangelingInfo {
            true_name: ling.id_name.clone(),
            hive_name: ling.hive_name.clone(),
            stolen_antag_info: ling.memory.clone(),
            memories: Vec::new(),
            objectives: ling
                .objectives
                .iter()
                .enumerate()
                .map(|(i, o)| ObjectiveInfo {
                    count: i as u32 + 1,
                    explanation: o.explanation(),
                    complete: false,
                })
                .collect(),
            can_change_objective: false,
            absorbed_dna: ling.absorbed_count,
        }
    }
}

impl ServerUi for ChangelingHudUi {
    type Source = &'static ChangelingHudData;
    type UserParam = ();
    type Act = ();

    fn static_data(_: &ChangelingHudData) {}

    fn data(source: &ChangelingHudData, _: Entity, _: &()) -> ChangelingHudData {
        *source
    }
}

/// `/datum/action/antag_info/Trigger` of a changeling.
pub(super) fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if world.get::<Changeling>(source).is_some() {
            world.write_message(OpenUi::new::<AntagInfoChangelingUi>(
                client,
                actor,
                source,
                CHANGELING_NAME,
            ));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Round end

/// tg `antag_report` for changelings: the header, then each one's `roundend_report`.
pub fn roundend_report(world: &mut World, end: RoundEnd) -> Vec<ChatMessage> {
    let mut query = world.query::<(Entity, &Changeling)>();
    let mut lings: Vec<(Entity, Changeling)> =
        query.iter(world).map(|(e, c)| (e, c.clone())).collect();
    if lings.is_empty() {
        return Vec::new();
    }
    lings.sort_by_key(|(e, _)| *e);
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = vec![line(vec![ChatSegment::new(
        Span::Bold,
        format!("{ROUNDEND_CATEGORY} были:"),
    )])];
    for (mob, ling) in lings {
        out.push(crate::antag::traitor::print_player(world, mob, &ling.key));
        out.push(line(vec![
            ChatSegment::new(Span::Bold, "Извлеченных генов: "),
            ChatSegment::new(Span::Plain, ling.absorbed_count.to_string()),
        ]));
        let mut won = world.get_entity(mob).is_ok();
        for (i, objective) in ling.objectives.iter().enumerate() {
            let done = objective.check_completion(world, mob, &ling, end);
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
            ChatSegment::new(Span::GreenText, "Генокрад был успешен!")
        } else {
            ChatSegment::new(Span::RedText, "Генокрад провалился.")
        }]));
    }
    out
}
