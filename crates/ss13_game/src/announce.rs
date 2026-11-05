//! Station announcements (server): tg `priority_announce`, `print_command_report` and
//! `dispatch_announcement_to_players` (`code/__HELPERS/priority_announce.dm`), the announcers
//! (`code/datums/announcers/`, BandaStation's intern in `modular_bandastation/announcers`) and
//! `GLOB.communications_controller` (`code/datums/communications.dm`): the announcement
//! cooldowns, messages to the communications consoles and the round-start report with
//! BandaStation's template (`modular_bandastation/paperwork/templates/station_report.md`) and
//! its blue alert (`ROUNDSTART_BLUE_ALERT`, on in BandaStation's config).
//!
//! - [`Priority`]: one priority announcement: its header by kind and sender, the text (or the
//!   announcer's `custom_alert_message`), the sound (an `ANNOUNCER_*` key of the announcer, a
//!   file, or the announcer's random alert sound) to every player mob that can hear (or the
//!   given ones).
//! - [`Announcer`]: the round's announcer (`SSstation.announcer`), its sounds from
//!   `ss13-mapload convert` (`sfx.ron`).
//! - [`CommsController`]: cooldowns shared by every console, [`send_message`] to the consoles
//!   (`crate::comms_server`), the round-start report ([`queue_roundstart_report`]).
// shortcut: no announcement colors or TTS (chat has no alert divider spans, no TTS service);
// the announcer is always the default one until station
// traits pick another (the intern's sounds and message are converted and selectable by
// [`Announcer::path`]).
// shortcut: the report has no station goals, station traits or footnotes (none are ported); it
// is printed as the paper's description (no paper text yet).

use bevy::prelude::*;
use ss13_mapload::sound::AnnouncerSounds;
use ss13_net::chat::server::SendChat;
use ss13_net::sound::server::SendSound;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_shared::protocol::SoundEvent;

use crate::comms::CommsMessage;
use crate::events::COMMAND_NAME;
use crate::security_level::SecLevel;
use crate::server::PlayerMob;
use crate::sound::{Sfx, SoundRng};

/// `SSstation.announcer`'s default type.
pub const DEFAULT_ANNOUNCER: &str = "/datum/centcom_announcer/default";
/// BandaStation's intern (`modular_bandastation/announcers`).
pub const INTERN_ANNOUNCER: &str = "/datum/centcom_announcer/intern";
/// `code/__DEFINES/sound.dm` `ANNOUNCER_*` keys of `event_sounds`.
pub const ANNOUNCER_ALIENS: &str = "announcer_aliens";
pub const ANNOUNCER_GRANOMALIES: &str = "announcer_granomalies";
pub const ANNOUNCER_INTERCEPT: &str = "announcer_intercept";
pub const ANNOUNCER_METEORS: &str = "announcer_meteors";
pub const ANNOUNCER_POWEROFF: &str = "announcer_poweroff";
pub const ANNOUNCER_RADIATION: &str = "announcer_radiation";
pub const ANNOUNCER_SHUTTLECALLED: &str = "announcer_shuttlecalled";
pub const ANNOUNCER_SHUTTLEDOCK: &str = "announcer_shuttledock";
pub const ANNOUNCER_SHUTTLERECALLED: &str = "announcer_shuttlerecalled";
pub const ANNOUNCER_SPANOMALIES: &str = "announcer_spanomalies";
/// `make_announcement`'s sounds.
pub const ANNOUNCE_SOUND: &str = "sound/announcer/announcement/announce.ogg";
pub const ANNOUNCE_SYNDI_SOUND: &str = "sound/announcer/announcement/announce_syndi.ogg";
/// `COMMUNICATION_COOLDOWN`, `COMMUNICATION_COOLDOWN_AI`.
pub const COMMUNICATION_COOLDOWN: f64 = 30.0;
pub const COMMUNICATION_COOLDOWN_AI: f64 = 30.0;
/// `waittime_l`, `waittime_h` (BandaStation: 1 and 5 s).
const REPORT_WAIT: (f64, f64) = (1.0, 5.0);
const PAPER: &str = "/obj/item/paper";
const REPORTS_PRINTED: &str = "Отчет был скопирован и распечатан на всех консолях связи.";

/// The round's announcer (tg `SSstation.announcer`): a `/datum/centcom_announcer` type.
#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct Announcer {
    pub path: String,
}

impl Default for Announcer {
    fn default() -> Self {
        Self {
            path: DEFAULT_ANNOUNCER.to_owned(),
        }
    }
}

/// tg `ANNOUNCEMENT_TYPE_*`: the header of a priority announcement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnnounceKind {
    /// No type: the sender (or `"<command>: Сообщение"`) and the title.
    #[default]
    Default,
    /// `ANNOUNCEMENT_TYPE_PRIORITY`.
    Priority,
    /// `ANNOUNCEMENT_TYPE_CAPTAIN`.
    Captain,
    /// `ANNOUNCEMENT_TYPE_SYNDICATE`.
    Syndicate,
}

/// tg `priority_announce(text, title, sound, type, sender_override, has_important_message,
/// players)`.
#[derive(Clone, Debug, Default)]
pub struct Priority {
    pub text: String,
    pub title: String,
    /// An `ANNOUNCER_*` key or a file; `None`: the announcer's random alert sound.
    pub sound: Option<String>,
    pub kind: AnnounceKind,
    pub sender: Option<String>,
    /// `has_important_message`: the announcer's `custom_alert_message` does not replace it.
    pub important: bool,
    /// The mobs to tell; `None`: every player mob (`GLOB.player_list`).
    pub players: Option<Vec<Entity>>,
}

impl Priority {
    pub fn new(text: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            title: title.into(),
            ..default()
        }
    }

    pub fn sound(mut self, sound: impl Into<String>) -> Self {
        self.sound = Some(sound.into());
        self
    }

    pub fn kind(mut self, kind: AnnounceKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn sender(mut self, sender: impl Into<String>) -> Self {
        self.sender = Some(sender.into());
        self
    }

    pub fn important(mut self) -> Self {
        self.important = true;
        self
    }

    pub fn players(mut self, players: Vec<Entity>) -> Self {
        self.players = Some(players);
        self
    }

    /// The header lines: the major title and the subheader.
    fn header(&self) -> (String, Option<&str>) {
        let title = (!self.title.is_empty()).then_some(self.title.as_str());
        match self.kind {
            AnnounceKind::Priority => ("Приоритетное оповещение".into(), title),
            AnnounceKind::Captain => ("Оповещение от капитана".into(), None),
            AnnounceKind::Syndicate => ("Оповещение от капитана Синдиката".into(), None),
            AnnounceKind::Default => (
                self.sender
                    .clone()
                    .unwrap_or_else(|| format!("{COMMAND_NAME}: Сообщение")),
                title,
            ),
        }
    }

    /// Sends it; nothing for an empty text.
    pub fn send(self, world: &mut World) {
        if self.text.trim().is_empty() {
            return;
        }
        let sounds = announcer(world);
        let text = match sounds.as_ref().and_then(|s| s.custom_alert_message.clone()) {
            Some(custom) if !self.important => custom,
            _ => self.text.clone(),
        };
        let sound = match &self.sound {
            None => sounds.as_ref().and_then(|s| pick(world, &s.alert)),
            Some(key) => match sounds.as_ref().and_then(|s| s.events.get(key)) {
                Some(file) => Some(file.clone()),
                // An `ANNOUNCER_*` key the announcer lacks (or no announcer data): silent.
                None => key.ends_with(".ogg").then(|| key.clone()),
            },
        };
        let (header, subheader) = self.header();
        info!("announcement: {header}: {}: {text}", self.title);
        let mut segments = vec![ChatSegment::new(Span::BoldAnnounce, format!("{header}\n"))];
        if let Some(sub) = subheader {
            segments.push(ChatSegment::new(Span::Bold, format!("{sub}\n")));
        }
        segments.push(ChatSegment::new(Span::Plain, text));
        let message = ChatMessage {
            channel: ChatChannel::System,
            segments,
        };
        let to_everyone = self.players.is_none();
        dispatch(world, message, self.players, sound);
        // The station's news feed (`GLOB.news_network.submit_article`). tg also posts a
        // captain's announcement a second time as a plain one; it is posted once here.
        use crate::newscaster_server::{STATION_ANNOUNCEMENTS, submit_article};
        if self.kind == AnnounceKind::Captain {
            let author = "Оповещение от капитана";
            submit_article(world, STATION_ANNOUNCEMENTS, &self.text, author, true, true);
        } else if self.sender.is_none() && to_everyone {
            let (body, author) = if self.title.is_empty() {
                (self.text.clone(), format!("{COMMAND_NAME}: Сообщение"))
            } else {
                (
                    format!("{}\n\n{}", self.title, self.text),
                    COMMAND_NAME.to_owned(),
                )
            };
            submit_article(world, STATION_ANNOUNCEMENTS, &body, &author, true, true);
        }
    }
}

/// tg `dispatch_announcement_to_players`: the text and the sound to each player mob that is
/// not deaf.
pub fn dispatch(
    world: &mut World,
    message: ChatMessage,
    players: Option<Vec<Entity>>,
    sound: Option<String>,
) {
    let mut query = world.query::<(Entity, &PlayerMob)>();
    let all: Vec<(Entity, Entity)> = query.iter(world).map(|(e, p)| (e, p.client)).collect();
    let clients: Vec<Entity> = all
        .into_iter()
        .filter(|(mob, _)| players.as_ref().is_none_or(|p| p.contains(mob)))
        .filter(|(mob, _)| !crate::sound::mob_is_deaf(world, *mob))
        .map(|(_, client)| client)
        .collect();
    if clients.is_empty() {
        return;
    }
    if let Some(file) = sound {
        for &client in &clients {
            world.write_message(SendSound {
                client,
                sound: SoundEvent {
                    file: file.clone(),
                    volume: 100.0,
                    speed: 1.0,
                    offset: None,
                },
            });
        }
    }
    world.write_message(SendChat { clients, message });
}

/// The round announcer's sounds (none on fixture assets).
pub fn announcer(world: &World) -> Option<AnnouncerSounds> {
    let path = world
        .get_resource::<Announcer>()
        .map_or(DEFAULT_ANNOUNCER, |a| a.path.as_str());
    world.get_resource::<Sfx>()?.0.announcers.get(path).cloned()
}

/// tg `pick(list)` from the sound stream.
fn pick(world: &mut World, files: &[String]) -> Option<String> {
    if files.is_empty() {
        return None;
    }
    let i = world
        .get_resource_mut::<SoundRng>()
        .map_or(0, |mut r| r.0.range(0, files.len() as u32 - 1) as usize);
    files.get(i).cloned()
}

/// `get_rand_welcome_sound`.
pub fn welcome_sound(world: &mut World) -> Option<String> {
    pick(world, &announcer(world)?.welcome)
}

/// `get_rand_report_sound`.
pub fn report_sound(world: &mut World) -> Option<String> {
    pick(world, &announcer(world)?.command_report)
}

/// tg `GLOB.communications_controller` and the consoles' static cooldowns (game times).
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct CommsController {
    pub silicon_message_until: f64,
    pub nonsilicon_message_until: f64,
    /// `/obj/machinery/computer/communications` `static/important_action_cooldown`.
    pub important_action_until: f64,
    /// `static/emergency_access_cooldown`.
    pub emergency_access_until: f64,
    /// `static/last_status_display`.
    pub last_status_display: Option<(String, String)>,
    /// When the queued round-start report goes out.
    pub report_at: Option<f64>,
}

impl CommsController {
    /// `can_announce(user, is_silicon)`.
    pub fn can_announce(&self, now: f64, silicon: bool) -> bool {
        now >= if silicon {
            self.silicon_message_until
        } else {
            self.nonsilicon_message_until
        }
    }
}

pub struct AnnouncePlugin;

impl Plugin for AnnouncePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Announcer>()
            .init_resource::<CommsController>()
            .add_systems(Update, send_roundstart_report);
    }
}

/// `make_announcement(user, is_silicon, input, syndicate, players)` after `can_announce`: a
/// silicon's minor announcement, a captain's (or the Syndicate's) priority one.
pub fn make_announcement(
    world: &mut World,
    user: Entity,
    name: &str,
    silicon: bool,
    input: &str,
    syndicate: bool,
    players: Option<Vec<Entity>>,
) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    if !world
        .resource::<CommsController>()
        .can_announce(now, silicon)
    {
        return false;
    }
    if silicon {
        crate::security_level::minor_announce_to(
            world,
            &format!("{name} объявляет"),
            input,
            false,
            players,
        );
        world
            .resource_mut::<CommsController>()
            .silicon_message_until = now + COMMUNICATION_COOLDOWN_AI;
    } else {
        let (kind, sound) = if syndicate {
            (AnnounceKind::Syndicate, ANNOUNCE_SYNDI_SOUND)
        } else {
            (AnnounceKind::Captain, ANNOUNCE_SOUND)
        };
        let mut announcement = Priority::new(input, "").sound(sound).kind(kind).important();
        announcement.players = players;
        announcement.send(world);
        world
            .resource_mut::<CommsController>()
            .nonsilicon_message_until = now + COMMUNICATION_COOLDOWN;
    }
    info!("{user} ({name}) made a priority announcement: {input}");
    true
}

/// tg `send_message(message)`: every powered console on a station level gets its own copy and
/// prints it on paper.
pub fn send_message(world: &mut World, message: CommsMessage) {
    let mut query = world.query_filtered::<(Entity, &ss13_shared::TilePos), (
        With<crate::comms_server::CommsConsole>,
        Without<crate::power::Unpowered>,
    )>();
    let consoles: Vec<(Entity, ss13_shared::TilePos)> =
        query.iter(world).map(|(e, p)| (e, *p)).collect();
    for (console, pos) in consoles {
        if !crate::comms_server::on_station_level(world, pos) {
            continue;
        }
        if let Some(mut state) = world.get_mut::<crate::comms_server::CommsConsole>(console) {
            state.messages.push(message.clone());
        }
        // shortcut: no paper text yet; the report's text is the paper's description.
        if let Some(paper) = crate::construction::spawn_obj(world, PAPER, pos) {
            world.entity_mut(paper).insert(ss13_mapload::DmName {
                name: format!("paper - '{}'", message.title),
                desc: message.content.clone(),
            });
        }
    }
}

/// tg `print_command_report(text, title, announce)`.
pub fn print_command_report(world: &mut World, text: &str, title: Option<&str>, announce: bool) {
    let title = title.map_or_else(
        || format!("{COMMAND_NAME}: Засекреченное сообщение"),
        str::to_owned,
    );
    if announce {
        let mut priority = Priority::new(
            "Отчет был загружен и распечатан на всех консолях связи.",
            "Входящее засекреченное сообщение",
        )
        .important();
        priority.sound = report_sound(world);
        priority.send(world);
    }
    send_message(
        world,
        CommsMessage {
            title,
            content: text.to_owned(),
            ..default()
        },
    );
}

/// `queue_roundstart_report`: the report goes out `rand(waittime_l, waittime_h)` later.
pub fn queue_roundstart_report(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let roll = world
        .get_resource_mut::<SoundRng>()
        .map_or(0.0, |mut r| f64::from(r.0.unit()));
    let delay = REPORT_WAIT.0 + roll * (REPORT_WAIT.1 - REPORT_WAIT.0);
    world.resource_mut::<CommsController>().report_at = Some(now + delay);
}

/// BandaStation `send_roundstart_report`: the report printed on the consoles, then the blue
/// alert set quietly and announced with the intercept sound.
fn send_roundstart_report(world: &mut World) {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut controller = world.resource_mut::<CommsController>();
    if !controller.report_at.is_some_and(|at| now >= at) {
        return;
    }
    controller.report_at = None;
    let report = station_report(world);
    print_command_report(
        world,
        &report,
        Some(&format!("{COMMAND_NAME}. Отчет об обстановке")),
        false,
    );
    if crate::security_level::current(world) < SecLevel::Blue {
        crate::security_level::set_level(world, SecLevel::Blue, false);
    }
    let level = crate::security_level::current(world);
    Priority::new(
        format!("{}\n\n{REPORTS_PRINTED}", level.elevating_to()),
        "Уровень угрозы повышен.",
    )
    .sound(ANNOUNCER_INTERCEPT)
    .send(world);
}

/// `station_report.md` with its keys filled: no goals, traits or footnotes (none ported), a
/// random human signing officer; the paper's `[blank_header]`/`[blank_footer]` as plain lines.
// shortcut: `[datetime]` has no station clock here; the line is left out.
pub fn station_report(world: &mut World) -> String {
    let officer = world
        .get_resource_mut::<crate::jobs::Jobs>()
        .and_then(|mut jobs| jobs.random_name(&Default::default()))
        .unwrap_or_else(|| "Неизвестный".to_owned());
    let station = crate::round::station_name(world);
    format!(
        "NT-INTEL-01 (КОНФИДЕНЦИАЛЬНО)\n\
         Отчет департамента разведки и оценки угроз Нанотрейзен\n\
         Станция: {station}\n\
         Категория: Центральное командование\n\
         Перед заполнением прочтите от начала до конца | Высокий приоритет\n\n\
         ---\n\n\
         === Подписи и штампы ===\n\n\
         Должность уполномоченного лица: Младший офицер разведки Нанотрейзен\n\
         Подпись уполномоченного лица: {officer}\n\n\
         {DISCLAIMER}"
    )
}

/// The template's `[blank_footer]`.
const DISCLAIMER: &str = "Отказ от ответственности: Данный факс является конфиденциальным и не \
    может быть прочтен сотрудниками не имеющего доступа. Если вы получили данный факс по ошибке, \
    просим вас сообщить отправителю и удалить его из вашего почтового ящика или любого другого \
    носителя. И Нанотрейзен, и любой её агент не несут ответственности за любые сделанные \
    заявления, они являются исключительно заявлениями отправителя, за исключением случаев, когда \
    отправителем является Нанотрейзен или один из её агентов. Отмечаем, что ни Нанотрейзен, ни \
    один из агентов корпорации не несут ответственности за наличие вирусов, которые могут \
    содержаться в данном факсе или его приложениях, и это только ваша прерогатива — просканировать \
    факс и приложения на их наличие. Никакие контракты не могут быть заключены посредством \
    факсимильной связи.";

#[cfg(test)]
mod tests {
    use super::*;

    fn world(custom: Option<&str>) -> World {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<Messages<SendChat>>();
        world.init_resource::<Messages<SendSound>>();
        let mut sfx = ss13_mapload::sound::SfxData::default();
        sfx.announcers.insert(
            DEFAULT_ANNOUNCER.to_owned(),
            AnnouncerSounds {
                alert: vec!["sound/attention.ogg".into()],
                events: [(ANNOUNCER_METEORS.to_owned(), "sound/meteors.ogg".to_owned())].into(),
                custom_alert_message: custom.map(str::to_owned),
                ..default()
            },
        );
        world.insert_resource(Sfx(sfx));
        world.init_resource::<SoundRng>();
        world
    }

    fn sent(world: &World) -> (Vec<String>, Vec<String>) {
        let chat = world
            .resource::<Messages<SendChat>>()
            .iter_current_update_messages()
            .map(|c| c.message.plain_text())
            .collect();
        let sounds = world
            .resource::<Messages<SendSound>>()
            .iter_current_update_messages()
            .map(|s| s.sound.file.clone())
            .collect();
        (chat, sounds)
    }

    #[test]
    fn headers_sounds_and_the_custom_message() {
        let mut world = world(None);
        let mob = world.spawn(()).id();
        let client = world.spawn(()).id();
        world.entity_mut(mob).insert(PlayerMob { client });
        Priority::new("Метеоры близко.", "Метеоры")
            .sound(ANNOUNCER_METEORS)
            .send(&mut world);
        Priority::new("Слушайте все.", "ignored")
            .kind(AnnounceKind::Captain)
            .send(&mut world);
        let (chat, sounds) = sent(&world);
        assert_eq!(
            chat[0],
            format!("{COMMAND_NAME}: Сообщение\nМетеоры\nМетеоры близко.")
        );
        assert_eq!(chat[1], "Оповещение от капитана\nСлушайте все.");
        // A key of the announcer, then its random alert sound.
        assert_eq!(sounds, ["sound/meteors.ogg", "sound/attention.ogg"]);

        // The intern's message replaces all but important announcements.
        let mut world = self::world(Some("Ждите стажёра."));
        world.spawn(PlayerMob { client });
        Priority::new("Текст.", "").send(&mut world);
        Priority::new("Важно.", "").important().send(&mut world);
        let (chat, _) = sent(&world);
        assert!(chat[0].ends_with("Ждите стажёра."));
        assert!(chat[1].ends_with("Важно."));
    }

    #[test]
    fn players_limit_the_recipients_and_empty_text_is_dropped() {
        let mut world = world(None);
        let a = world.spawn(()).id();
        let b = world.spawn(()).id();
        let ma = world.spawn(PlayerMob { client: a }).id();
        world.spawn(PlayerMob { client: b });
        Priority::new("", "Пусто").send(&mut world);
        Priority::new("Только А.", "")
            .players(vec![ma])
            .send(&mut world);
        let chats: Vec<Vec<Entity>> = world
            .resource::<Messages<SendChat>>()
            .iter_current_update_messages()
            .map(|c| c.clients.clone())
            .collect();
        assert_eq!(chats, [vec![a]]);
    }

    #[test]
    fn announcement_cooldowns_are_per_kind() {
        let mut c = CommsController::default();
        assert!(c.can_announce(0.0, false) && c.can_announce(0.0, true));
        c.nonsilicon_message_until = COMMUNICATION_COOLDOWN;
        assert!(!c.can_announce(29.0, false));
        assert!(c.can_announce(29.0, true));
        assert!(c.can_announce(30.0, false));
    }
}
