//! BandaStation roundstart logout report (`modular_bandastation/_helpers220`,
//! `SSticker.display_roundstart_logout_report`): 10 ± 3 minutes after the round starts, the
//! admins with `R_ADMIN` (and the admin log) get the list of players who already left their
//! character: disconnected, in hard crit, dead.
// shortcut: no "Connected, Inactive" (AFK) line, clients keep no inactivity time; no "Suicide"
// line, there is no suicide verb; no "Ghosted" line, every ghost here can re-enter its body
// (tg `can_reenter_corpse`). Add them with those mechanics.
// shortcut: the blackbox record of the report is skipped (SQL feedback, deferred).
// Deviation: a dead body whose ghost disconnected reads "Disconnected" (tg keeps the observer and
// says "Dead"; here the ghost is gone with its client).

use bevy::prelude::*;
use ss13_game::antag::AntagHudData;
use ss13_game::combat::CombatRng;
use ss13_game::ghost::Ghost;
use ss13_game::health::{Health, Stat};
use ss13_game::jobs::{Jobs, PlayerJob, display_time_text};
use ss13_game::lobby::GameState;
use ss13_game::round::Ticker;
use ss13_game::server::PlayerMob;
use ss13_net::server::ClientToken;
use ss13_shared::admin::rights::R_ADMIN;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};

use crate::rights::ckey;
use crate::server::{admins_with, chat, client_by_ckey, log_admin};

/// `roundstart_logout_report_time_average`, `_variance` (config defaults), seconds.
pub const REPORT_TIME_AVERAGE: u32 = 600;
pub const REPORT_TIME_VARIANCE: u32 = 180;

/// On a living mob a player ever controlled (tg `mind`/`last_mind`): that player's ckey.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct PlayedBy(pub String);

/// The report's timer (tg `GLOB.logout_timer_set`): rolled when the round is first seen playing.
/// Insert before the plugin to fix the delay (tests).
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct LogoutReport {
    pub delay_secs: Option<f64>,
    pub sent: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogoutStatus {
    Disconnected,
    Dying,
    Dead,
}

/// tg `/datum/roundstart_logout_report_entry`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogoutEntry {
    pub name: String,
    pub ckey: String,
    pub job: String,
    pub status: LogoutStatus,
    pub is_antag: bool,
}

pub(crate) fn build(app: &mut App) {
    app.init_resource::<LogoutReport>()
        .add_observer(remember_player)
        .add_systems(Update, report_when_due);
}

/// tg `mind.key` of a body: kept when its player leaves it.
fn remember_player(
    add: On<Add<PlayerMob>>,
    mobs: Query<&PlayerMob, (With<Health>, Without<Ghost>)>,
    tokens: Query<&ClientToken>,
    mut commands: Commands,
) {
    let Ok(player) = mobs.get(add.entity) else {
        return;
    };
    let Ok(token) = tokens.get(player.client) else {
        return;
    };
    let key = ckey(&token.0);
    if !key.is_empty() {
        commands.entity(add.entity).try_insert(PlayedBy(key));
    }
}

fn report_when_due(world: &mut World) {
    let report = *world.resource::<LogoutReport>();
    let Some(ticker) = world.get_resource::<Ticker>() else {
        return;
    };
    if report.sent || ticker.state() != GameState::Playing {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let elapsed = ticker.round_duration(now);
    let delay = match report.delay_secs {
        Some(delay) => delay,
        None => {
            // `rand(average - variance, average + variance)`.
            let delay = world.get_resource_mut::<CombatRng>().map_or(
                f64::from(REPORT_TIME_AVERAGE),
                |mut rng| {
                    f64::from(rng.range(
                        REPORT_TIME_AVERAGE - REPORT_TIME_VARIANCE,
                        REPORT_TIME_AVERAGE + REPORT_TIME_VARIANCE,
                    ))
                },
            );
            world.resource_mut::<LogoutReport>().delay_secs = Some(delay);
            delay
        }
    };
    if elapsed < delay {
        return;
    }
    world.resource_mut::<LogoutReport>().sent = true;
    let entries = entries(world);
    let text = formatted_text(&entries, delay);
    log_admin(world, "ADMIN", &text);
    let admins = admins_with(world, R_ADMIN);
    chat(world, admins, message(&entries, delay));
}

/// tg `display_roundstart_logout_report`'s scan of `GLOB.mob_living_list`.
// Cost: once per round, one pass over the bodies players ever controlled.
pub fn entries(world: &mut World) -> Vec<LogoutEntry> {
    let bodies: Vec<(
        Entity,
        String,
        Stat,
        bool,
        Option<String>,
        Option<usize>,
        bool,
    )> = world
        .query_filtered::<(
            Entity,
            &PlayedBy,
            Option<&Stat>,
            Has<PlayerMob>,
            Option<&Name>,
            Option<&PlayerJob>,
            Has<AntagHudData>,
        ), (With<Health>, Without<Ghost>)>()
        .iter(world)
        .map(|(e, by, stat, played, name, job, antag)| {
            (
                e,
                by.0.clone(),
                stat.copied().unwrap_or_default(),
                played,
                name.map(|n| n.to_string()),
                job.map(|j| j.0),
                antag,
            )
        })
        .collect();
    let ghosted: Vec<Entity> = world
        .query_filtered::<&Ghost, With<PlayerMob>>()
        .iter(world)
        .filter_map(|g| g.body)
        .collect();
    let mut out = Vec::new();
    for (body, key, stat, played, name, job, is_antag) in bodies {
        let status = if played {
            // A connected client in its body: only the dying and the dead.
            match stat {
                Stat::Dead => LogoutStatus::Dead,
                Stat::HardCrit => LogoutStatus::Dying,
                _ => continue,
            }
        } else if ghosted.contains(&body) {
            // A ghost that can re-enter: listed only once its body is dead.
            if stat != Stat::Dead {
                continue;
            }
            LogoutStatus::Dead
        } else if client_by_ckey(world, &key).is_none() {
            LogoutStatus::Disconnected
        } else {
            // The player moved to another body (tg: the ckey left this one).
            continue;
        };
        let job = job
            .and_then(|j| {
                world
                    .get_resource::<Jobs>()
                    .and_then(|jobs| jobs.data.jobs.get(j))
                    .map(|job| job.title.clone())
            })
            .unwrap_or_default();
        out.push(LogoutEntry {
            name: name.unwrap_or_default(),
            ckey: key,
            job,
            status,
            is_antag,
        });
    }
    out.sort_by(|a, b| a.ckey.cmp(&b.ckey));
    out
}

fn status_text(status: LogoutStatus) -> (Span, &'static str) {
    match status {
        LogoutStatus::Disconnected => (Span::Warning, "Disconnected"),
        LogoutStatus::Dying => (Span::Plain, "Dying"),
        LogoutStatus::Dead => (Span::Plain, "Dead"),
    }
}

/// tg `to_formatted_text` without the markup (the admin log line).
pub fn formatted_text(entries: &[LogoutEntry], delay: f64) -> String {
    let mut msg = String::from("Roundstart logout report\n\n");
    for entry in entries {
        let (_, status) = status_text(entry.status);
        msg += &format!(
            "{} ({}), the {} ({status})\n",
            entry.name, entry.ckey, entry.job
        );
    }
    msg += &format!(
        "Roundstart logout reported at: {}\n",
        display_time_text(delay)
    );
    msg
}

/// tg `to_formatted_text`: the chat message, statuses in their colours.
fn message(entries: &[LogoutEntry], delay: f64) -> ChatMessage {
    let mut segments = vec![ChatSegment::new(
        Span::BoldNotice,
        "Roundstart logout report\n\n",
    )];
    for entry in entries {
        let (span, status) = status_text(entry.status);
        segments.push(ChatSegment::new(Span::Bold, entry.name.clone()));
        segments.push(ChatSegment::new(
            Span::Plain,
            format!(" ({}), the {} (", entry.ckey, entry.job),
        ));
        segments.push(ChatSegment::new(span, status));
        segments.push(ChatSegment::new(Span::Plain, ")\n"));
    }
    segments.push(ChatSegment::new(
        Span::BoldNotice,
        format!(
            "Roundstart logout reported at: {}\n",
            display_time_text(delay)
        ),
    ));
    ChatMessage {
        channel: ChatChannel::AdminLog,
        segments,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatted_text_lists_every_entry_and_the_time() {
        let entries = [
            LogoutEntry {
                name: "Bob".into(),
                ckey: "bob".into(),
                job: "Assistant".into(),
                status: LogoutStatus::Disconnected,
                is_antag: false,
            },
            LogoutEntry {
                name: "Ann".into(),
                ckey: "ann".into(),
                job: "Medical Doctor".into(),
                status: LogoutStatus::Dead,
                is_antag: true,
            },
        ];
        let text = formatted_text(&entries, 630.0);
        assert_eq!(
            text,
            "Roundstart logout report\n\n\
             Bob (bob), the Assistant (Disconnected)\n\
             Ann (ann), the Medical Doctor (Dead)\n\
             Roundstart logout reported at: 10 минут и 30 секунд\n"
        );
    }
}
