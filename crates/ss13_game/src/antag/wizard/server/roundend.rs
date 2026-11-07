//! The round end report.

use super::*;

/// `/datum/antagonist/wizard/roundend_report` of every wizard and apprentice.
pub fn roundend_report(world: &mut World, end: RoundEnd) -> Vec<ChatMessage> {
    let mut query = world.query::<(Entity, &Wizard)>();
    let mut wizards: Vec<(Entity, Wizard)> =
        query.iter(world).map(|(e, w)| (e, w.clone())).collect();
    if wizards.is_empty() {
        return Vec::new();
    }
    wizards.sort_by_key(|(e, w)| (w.master.is_some(), *e));
    let line = |segments: Vec<ChatSegment>| ChatMessage {
        channel: ChatChannel::System,
        segments,
    };
    let mut out = vec![line(vec![ChatSegment::new(
        Span::Bold,
        format!("{ROUNDEND_CATEGORY} были:"),
    )])];
    for (mob, wizard) in wizards {
        out.push(crate::antag::traitor::print_player(world, mob, &wizard.key));
        let mut won = true;
        for (i, objective) in wizard.objectives.iter().enumerate() {
            let done = objective.check_completion(world, mob, end);
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
        if wizard.master.is_some() {
            continue;
        }
        out.push(line(vec![if won {
            ChatSegment::new(Span::GreenText, "Волшебник был успешен!")
        } else {
            ChatSegment::new(Span::RedText, "Волшебник провалился!")
        }]));
        let name = name_of(world, mob);
        if wizard.purchases.is_empty() {
            out.push(line(vec![ChatSegment::new(
                Span::Bold,
                format!("{name} не покупал никаких заклинаний!"),
            )]));
        } else {
            let bought: Vec<String> = wizard
                .purchases
                .iter()
                .map(|(n, k)| {
                    if *k > 1 {
                        format!("{k}x {n}")
                    } else {
                        (*n).to_owned()
                    }
                })
                .collect();
            out.push(line(vec![ChatSegment::new(
                Span::Bold,
                format!("{name} использовал следующие заклинания:"),
            )]));
            out.push(line(vec![ChatSegment::new(Span::Plain, bought.join(", "))]));
        }
    }
    out
}
