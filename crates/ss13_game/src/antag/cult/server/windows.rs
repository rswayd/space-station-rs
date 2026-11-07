//! The antagonist panel and the round end report.

use super::*;

impl ServerUi for AntagInfoGenericUi {
    type Source = &'static CultInfo;
    type UserParam = ();
    type Act = ();

    fn static_data(info: &CultInfo) -> BrotherInfo {
        info.0.clone()
    }

    fn data(_: &CultInfo, _: Entity, _: &()) {}
}

/// `/datum/action/antag_info/Trigger` of a cultist.
pub(super) fn open_info(world: &mut World, mut acts: Local<MessageCursor<UiActed<AntagHudUi>>>) {
    let pending: Vec<(Entity, Entity, Entity)> = acts
        .read(world.resource::<Messages<UiActed<AntagHudUi>>>())
        .map(|a| (a.client, a.actor, a.source))
        .collect();
    for (client, actor, source) in pending {
        if world.get::<Cultist>(source).is_some() {
            let name = antag_name(world, source);
            world.write_message(OpenUi::new::<AntagInfoGenericUi>(
                client, actor, source, name,
            ));
        }
    }
}

/// The panels of every cultist follow the cult's objectives.
pub(super) fn update_info(world: &mut World) {
    let members = world
        .get_resource::<CultTeam>()
        .map(|t| t.members.clone())
        .unwrap_or_default();
    for m in members {
        let info = info_of(world, m);
        if let Ok(mut e) = world.get_entity_mut(m) {
            e.insert(CultInfo(info));
        }
    }
}

/// The panel's static data: the cult's objectives now.
pub fn info_of(world: &World, mob: Entity) -> BrotherInfo {
    let objectives = world
        .get_resource::<CultTeam>()
        .map(|t| {
            t.objectives
                .iter()
                .enumerate()
                .map(|(i, o)| ObjectiveInfo {
                    count: i as u32 + 1,
                    explanation: o.explanation(),
                    complete: o.complete(),
                })
                .collect()
        })
        .unwrap_or_default();
    BrotherInfo {
        antag_name: antag_name(world, mob).to_owned(),
        objectives,
    }
}

// ---------------------------------------------------------------------------------------------
// Round end

/// `/datum/team/cult/roundend_report`.
pub fn roundend_report(world: &mut World, _end: RoundEnd) -> Vec<ChatMessage> {
    let Some(team) = world.get_resource::<CultTeam>().cloned() else {
        return Vec::new();
    };
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = vec![line(vec![if team.victory() {
        ChatSegment::new(
            Span::GreenText,
            "Культ смог призвать своего бога! Нар'Си погасил ещё один факел в этой пустоте!",
        )
    } else {
        ChatSegment::new(
            Span::RedText,
            "Экипаж смог остановить культ! Тёмные речи и ересь не идут ни в какое сравнение с \
             лучшими из Нанотрейзен!",
        )
    }])];
    if !team.objectives.is_empty() {
        out.push(line(vec![ChatSegment::new(
            Span::Bold,
            "Культ имел следующие цели:",
        )]));
        for (i, o) in team.objectives.iter().enumerate() {
            out.push(line(vec![
                ChatSegment::new(Span::Bold, format!("Цель #{}", i + 1)),
                ChatSegment::new(Span::Plain, format!(": {} ", o.explanation())),
                if o.complete() {
                    ChatSegment::new(Span::GreenText, "Успех!")
                } else {
                    ChatSegment::new(Span::RedText, "Провал.")
                },
            ]));
        }
    }
    let members: Vec<(Entity, String)> = if team.true_cultists.is_empty() {
        team.members
            .iter()
            .map(|&m| {
                (
                    m,
                    world
                        .get::<Cultist>(m)
                        .map(|c| c.key.clone())
                        .unwrap_or_default(),
                )
            })
            .collect()
    } else {
        team.true_cultists.clone()
    };
    if !members.is_empty() {
        out.push(line(vec![ChatSegment::new(
            Span::Bold,
            "Культистами были:",
        )]));
        for (mob, key) in members {
            out.push(crate::antag::traitor::print_player(world, mob, &key));
        }
    }
    out
}
