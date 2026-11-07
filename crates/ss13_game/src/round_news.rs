//! The round's news report, after tg `SSdynamic.set_round_result` (each ruleset's
//! `round_result`) and `SSticker.send_news_report`: at round end the station's fate picks one
//! Nanotrasen News Network story.
// shortcut: tg sends the story to the linked servers (`send2otherserver`, `cross_server` config);
// there is no server link here, so it goes to the server log ([`send_news_report`]). Send it to
// the link once one exists.
// shortcut: no blob (`BLOB_WIN`, `BLOB_DESTROYED`) and no supermatter cascade
// (`SUPERMATTER_CASCADE`) stories: neither is ported. The emergency shuttle keeps no call reason,
// so the evacuation story is the one without a distress beacon.
// Deviation: tg's `GLOB.station_was_nuked` check overrides the nuclear operatives' own story
// ("it wasn't nuke ops", says its comment): here `STATION_NUKED` applies only when no ruleset set
// a story.

use bevy::prelude::*;

use crate::antag::nukeop::server::NukeResult;
use crate::antag::objectives::{RoundEnd, considered_alive, considered_escaped};

/// tg `news_report` values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewsReport {
    NukeSyndicateBase,
    StationDestroyedNuke,
    StationEvacuated,
    CultEscape,
    CultFailure,
    CultSummon,
    NukeMiss,
    OperativesKilled,
    OperativeSkirmish,
    RevsWin,
    RevsLose,
    WizardKilled,
    StationNuked,
    ShuttleHijack,
}

/// `/datum/dynamic_ruleset/roundstart/nukies/round_result`.
pub fn nuke_story(result: NukeResult) -> NewsReport {
    match result {
        NukeResult::Fluke => NewsReport::NukeSyndicateBase,
        NukeResult::NukeWin | NukeResult::NoSurvivors => NewsReport::StationDestroyedNuke,
        NukeResult::WrongStation | NukeResult::WrongStationDead => NewsReport::NukeMiss,
        NukeResult::CrewWinSyndiesDead | NukeResult::CrewWin => NewsReport::OperativesKilled,
        _ => NewsReport::OperativeSkirmish,
    }
}

/// tg `set_round_result`'s news half against the world.
pub fn round_result(world: &mut World, end: RoundEnd) -> Option<NewsReport> {
    // The rulesets' `round_result`: the cult and a dead wizard end the search.
    if let Some(team) = world
        .get_resource::<crate::antag::cult::server::CultTeam>()
        .filter(|t| t.size_at_maximum > 0 || !t.members.is_empty())
    {
        if team.victory() {
            return Some(NewsReport::CultSummon);
        }
        let total = if team.size_at_maximum > 0 {
            team.size_at_maximum
        } else {
            100
        };
        let escaped = team
            .members
            .iter()
            .filter(|&&m| considered_escaped(world, m, end))
            .count() as u32;
        // `ratio_to_be_considered_escaped = 0.5`.
        return Some(if f64::from(escaped) / f64::from(total) >= 0.5 {
            NewsReport::CultEscape
        } else {
            NewsReport::CultFailure
        });
    }
    let wizards: Vec<Entity> = world
        .query_filtered::<Entity, With<crate::antag::wizard::server::Wizard>>()
        .iter(world)
        .collect();
    if !wizards.is_empty() && !wizards.iter().any(|&w| considered_alive(world, w)) {
        return Some(NewsReport::WizardKilled);
    }
    let mut news = crate::antag::nukeop::server::get_result(world).map(nuke_story);
    match world
        .get_resource::<crate::antag::revolution::server::RevolutionHandler>()
        .and_then(|h| h.result)
    {
        Some(crate::antag::revolution::server::RevResult::Station) => {
            news = Some(NewsReport::RevsLose)
        }
        Some(crate::antag::revolution::server::RevResult::Revolution) => {
            news = Some(NewsReport::RevsWin)
        }
        None => {}
    }
    if news.is_none() && crate::antag::nukeop::server::station_destroyed(world) {
        news = Some(NewsReport::StationNuked);
    }
    if news.is_none() && crate::shuttle::evacuated(world) {
        news = Some(if crate::shuttle::hijacked(world) {
            NewsReport::ShuttleHijack
        } else {
            NewsReport::StationEvacuated
        });
    }
    news
}

/// `send_news_report`'s source.
pub const NEWS_SOURCE: &str = "Nanotrasen News Network";

/// `send_news_report`'s story for the station.
pub fn news_message(report: NewsReport, station: &str) -> String {
    match report {
        NewsReport::NukeSyndicateBase => format!(
            "В ходе дерзкого рейда героический экипаж {station} взорвал ядерное устройство в \
             самом сердце базы террористов."
        ),
        NewsReport::StationDestroyedNuke => format!(
            "Мы хотим заверить всех сотрудников, что сообщения о ядерной атаке Синдиката на \
             {station} на самом деле являются розыгрышем. Безопасного вам дня!"
        ),
        NewsReport::StationEvacuated => format!(
            "Экипаж {station} был эвакуирован на фоне неподтверждённых сообщений о вражеской \
             активности."
        ),
        NewsReport::CultEscape => {
            format!("Тревога безопасности: группа религиозных фанатиков сбежала с {station}.")
        }
        NewsReport::CultFailure => format!(
            "В связи с ликвидацией запрещённого культа на {station} напоминаем всем сотрудникам, \
             что поклонение вне часовни строго запрещено и является основанием для увольнения."
        ),
        NewsReport::CultSummon => format!(
            "Представители компании уточняют, что {station} планировалось списать после \
             метеоритных повреждений ранее в этом году. Ранние сообщения о непостижимом \
             древнем ужасе были ошибочными."
        ),
        NewsReport::NukeMiss => format!(
            "Синдикат провалил террористическую атаку на {station}, взорвав ядерное оружие в \
             пустом космосе неподалёку."
        ),
        NewsReport::OperativesKilled => format!(
            "На {station} идёт ремонт после того, как экипаж уничтожил элитный отряд смерти \
             Синдиката."
        ),
        NewsReport::OperativeSkirmish => format!(
            "Стычка сил безопасности с агентами Синдиката на борту {station} закончилась тем, \
             что обе стороны обескровлены, но целы."
        ),
        NewsReport::RevsWin => format!(
            "Представители компании заверили инвесторов, что, несмотря на профсоюзный мятеж на \
             борту {station}, повышения зарплат рабочим не будет."
        ),
        NewsReport::RevsLose => format!(
            "{station} быстро подавила ошибочную попытку мятежа. Помните: профсоюзы незаконны!"
        ),
        NewsReport::WizardKilled => format!(
            "Обострилась напряжённость с Федерацией космических волшебников после гибели одного \
             из её членов на борту {station}."
        ),
        NewsReport::StationNuked => format!(
            "{station} по неизвестным причинам активировала устройство самоуничтожения. Ведутся \
             попытки клонировать капитана для ареста и казни."
        ),
        NewsReport::ShuttleHijack => format!(
            "Во время плановой эвакуации навигационные протоколы эвакуационного шаттла {station} \
             были повреждены, и он сбился с курса, но вскоре был найден."
        ),
    }
}

/// tg `send_news_report`, from `declare_completion`: the story of the round, if any.
pub fn send_news_report(world: &mut World, end: RoundEnd) -> Option<String> {
    let report = round_result(world, end)?;
    let message = news_message(report, &crate::round::station_name(world));
    info!("news report ({NEWS_SOURCE}): {message}");
    Some(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nuke_results_pick_tg_stories() {
        assert_eq!(nuke_story(NukeResult::Fluke), NewsReport::NukeSyndicateBase);
        assert_eq!(
            nuke_story(NukeResult::NoSurvivors),
            NewsReport::StationDestroyedNuke
        );
        assert_eq!(
            nuke_story(NukeResult::WrongStationDead),
            NewsReport::NukeMiss
        );
        assert_eq!(
            nuke_story(NukeResult::CrewWin),
            NewsReport::OperativesKilled
        );
        assert_eq!(
            nuke_story(NukeResult::HijackDisk),
            NewsReport::OperativeSkirmish
        );
        assert_eq!(
            nuke_story(NukeResult::Interrupted),
            NewsReport::OperativeSkirmish
        );
    }

    #[test]
    fn a_quiet_round_has_no_story_and_revolutions_have_theirs() {
        let mut world = World::new();
        assert_eq!(round_result(&mut world, RoundEnd::default()), None);
        world.insert_resource(crate::antag::revolution::server::RevolutionHandler {
            result: Some(crate::antag::revolution::server::RevResult::Station),
            ..default()
        });
        assert_eq!(
            round_result(&mut world, RoundEnd::default()),
            Some(NewsReport::RevsLose)
        );
    }

    #[test]
    fn every_story_names_the_station() {
        use NewsReport::*;
        for report in [
            NukeSyndicateBase,
            StationDestroyedNuke,
            StationEvacuated,
            CultEscape,
            CultFailure,
            CultSummon,
            NukeMiss,
            OperativesKilled,
            OperativeSkirmish,
            RevsWin,
            RevsLose,
            WizardKilled,
            StationNuked,
            ShuttleHijack,
        ] {
            assert!(news_message(report, "Box").contains("Box"), "{report:?}");
        }
    }
}
