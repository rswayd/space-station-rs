//! The statpanel, after tg's stat browser (`SSstatpanels`, `code/modules/client/verbs/
//! stat_panel.dm`, `html/statbrowser.js`): a window of tabs, the Status tab first, then one tab
//! per verb category of the client's mob, each verb a button.
//!
//! - Every client, in the lobby or in a body, has a host off the map ([`StatPanelHost`], tgui
//!   `always_state`) whose window [`StatPanelUi`] opens on connect.
//! - The data is rebuilt once a second (tg `SSstatpanels` fires every 0.4 s): the Status lines
//!   (station, round time, players) and the verbs of the client's controlled mob.
//! - A verb button sends [`VERB`] with its tab and name; the server looks the verb up again in
//!   the mob's current verbs before running it, so only verbs the mob has run.
//! - Verb tabs so far: BandaStation's «Эмоции» (`modular_bandastation/emote_panel/code/
//!   emote_verbs.dm`) on humans, with the species verbs (tajaran, vulpkanin, moth, tails).
// shortcut: no MC/admin tabs, tickets, turf listing (alt-click "objects on the tile") or other
// verb categories (the port has no verb objects yet); the window can be closed and only comes
// back on reconnect.

use serde::{Deserialize, Serialize};
use ss13_tgui::UiInterface;

/// The stat browser window.
pub struct StatPanelUi;

impl UiInterface for StatPanelUi {
    const NAME: &'static str = "StatPanel";
    type Data = StatPanelData;
    type StaticData = ();
}

/// A verb button: `act('verb', { tab, name })`.
pub const VERB: &str = "verb";
pub const TAB: &str = "tab";
pub const NAME: &str = "name";

/// The Status tab (tg `"Status"`).
pub const STATUS_TAB: &str = "Status";
/// BandaStation's emote verb category.
pub const EMOTES_TAB: &str = "Эмоции";
/// tg's `"OOC"` verb category: the buttons `ss13_admin::ooc` answers.
pub const OOC_TAB: &str = "OOC";
pub const OOC_VERB_IGNORE: &str = "Игнорировать";
pub const OOC_VERB_UNIGNORE: &str = "Перестать игнорировать";
pub const OOC_VERB_MOTD: &str = "MOTD";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatPanelData {
    pub tabs: Vec<StatTab>,
}

/// One tab: text lines (Status) and verb buttons, by name.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatTab {
    pub name: String,
    pub lines: Vec<String>,
    pub verbs: Vec<String>,
}

/// Which mobs have an emote verb (the verb's owner type, or the organ that adds it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerbOwner {
    /// `/mob/living/carbon/human`.
    Human,
    /// `/mob/living/carbon/human/species/tajaran`.
    Tajaran,
    /// `/mob/living/carbon/human/species/vulpkanin` (its tongue adds them).
    Vulpkanin,
    /// `/mob/living/carbon/human/species/moth`.
    Moth,
    /// `emote_wag`: added by the tajaran and vulpkanin tails.
    Tail,
}

/// BandaStation's emote verbs: name and emote key. The `GAME_VERB_PROC` verbs no organ adds
/// (`emote_flap`, `emote_aflap`, `emote_ping`, `emote_beep`, `emote_buzz`, `emote_buzz2`) are
/// left out: no mob has them.
pub const EMOTE_VERBS: &[(&str, &str, VerbOwner)] = &[
    ("▷ Смеяться", "laugh", VerbOwner::Human),
    ("▷ Плакать", "cry", VerbOwner::Human),
    ("▷ Хихикать", "giggle", VerbOwner::Human),
    ("▷ Кашлять", "cough", VerbOwner::Human),
    ("▷ Кричать", "scream", VerbOwner::Human),
    ("▷ Подавиться", "choke", VerbOwner::Human),
    ("▷ Стонать", "moan", VerbOwner::Human),
    ("▷ Задыхаться", "gasp", VerbOwner::Human),
    ("▷ Вздыхать", "sigh", VerbOwner::Human),
    ("▷ Чихнуть", "sneeze", VerbOwner::Human),
    ("▷ Шмыгать носом", "sniff", VerbOwner::Human),
    ("▷ Храпеть", "snore", VerbOwner::Human),
    ("▷ Свистеть", "whistle", VerbOwner::Human),
    ("▷ Зевать", "yawn", VerbOwner::Human),
    ("▷ Салютовать", "salute", VerbOwner::Human),
    ("▷ Щёлкнуть пальцами", "snap", VerbOwner::Human),
    ("▷ Хрустеть пальцами", "crack", VerbOwner::Human),
    ("▷ Хлопать", "clap", VerbOwner::Human),
    ("○ Рухнуть", "collapse", VerbOwner::Human),
    ("○ Прыгнуть", "jump", VerbOwner::Human),
    ("○ Потерять сознание", "faint", VerbOwner::Human),
    ("○ Пожать руку", "handshake", VerbOwner::Human),
    ("○ Сделать кувырок", "flip", VerbOwner::Human),
    ("○ Покрутиться", "spin", VerbOwner::Human),
    ("○ Сделать кольцо из пальцев", "circle", VerbOwner::Human),
    ("○ Приподнять очки", "glasses", VerbOwner::Human),
    ("○ Приготовить руку", "hand", VerbOwner::Human),
    ("○ Поцеловать", "kiss", VerbOwner::Human),
    ("○ Дать леща", "noogie", VerbOwner::Human),
    ("○ Украсть обувь", "shoesteal", VerbOwner::Human),
    ("○ Сдаться", "surrender", VerbOwner::Human),
    ("○ Качаться на месте", "sway", VerbOwner::Human),
    ("◦ Ругнуться", "swear", VerbOwner::Human),
    ("○ Дразнить", "taunt", VerbOwner::Human),
    ("○ Танцевать", "dance", VerbOwner::Human),
    ("○ Дать пощёчину", "slap", VerbOwner::Human),
    ("◦ Почесаться", "scratch", VerbOwner::Human),
    ("◦ Краснеть", "blush", VerbOwner::Human),
    ("◦ Моргать", "blink", VerbOwner::Human),
    ("◦ Моргать быстро", "blink_r", VerbOwner::Human),
    ("◦ Поклониться", "bow", VerbOwner::Human),
    ("◦ Усмехнуться", "chuckle", VerbOwner::Human),
    ("◦ Прочистить горло", "clear", VerbOwner::Human),
    ("◦ Скрестить руки", "cross", VerbOwner::Human),
    ("◦ Особое рукопожатие", "dap", VerbOwner::Human),
    ("◦ Подпевать", "tunesing", VerbOwner::Human),
    ("◦ Сесть", "sit", VerbOwner::Human),
    ("◦ Пускать слюни", "drool", VerbOwner::Human),
    ("◦ Выдохнуть", "exhale", VerbOwner::Human),
    ("◦ Потужиться", "gag", VerbOwner::Human),
    ("◦ Скорчить рожицу", "grimace", VerbOwner::Human),
    ("◦ Смотреть в недоумении", "frown", VerbOwner::Human),
    ("◦ Сверлить взглядом", "glare", VerbOwner::Human),
    ("◦ Ухмыльнуться", "grin", VerbOwner::Human),
    ("◦ Мотать головой", "shake", VerbOwner::Human),
    ("◦ Наклонить голову", "tilt", VerbOwner::Human),
    ("◦ Улыбнуться", "smile", VerbOwner::Human),
    ("◦ Самодовольно улыбнуться", "smug", VerbOwner::Human),
    ("◦ Ворчать", "grumble", VerbOwner::Human),
    ("◦ Пожать плечами", "shrug", VerbOwner::Human),
    ("◦ Пялиться", "stare", VerbOwner::Human),
    ("◦ Обидеться", "sulk", VerbOwner::Human),
    ("◦ Дрожать в ужасе", "tremble", VerbOwner::Human),
    ("◦ Сильно дёргаться", "twitch", VerbOwner::Human),
    ("◦ Дёргаться", "twitch_s", VerbOwner::Human),
    ("◦ Приподнять бровь", "eyebrow", VerbOwner::Human),
    ("◦ Побулькать", "gurgle", VerbOwner::Human),
    ("◦ Обнимать", "hug", VerbOwner::Human),
    ("◦ Вдохнуть", "inhale", VerbOwner::Human),
    ("◦ Воображаемая гитара", "airguitar", VerbOwner::Human),
    ("◦ Рыгнуть", "burp", VerbOwner::Human),
    ("◦ Бормотать", "mumble", VerbOwner::Human),
    ("◦ Поднять руку", "raise", VerbOwner::Human),
    ("◦ Бледнеть", "pale", VerbOwner::Human),
    ("◦ Надуть губы", "pout", VerbOwner::Human),
    ("◦ Хмуриться", "scowl", VerbOwner::Human),
    ("◦ Махать хвостом", "wag", VerbOwner::Tail),
    ("◦ Махать", "wave", VerbOwner::Human),
    ("◦ Хныкать", "whimper", VerbOwner::Human),
    ("◦ Посмотреть", "look", VerbOwner::Human),
    ("◦ Кивнуть", "nod", VerbOwner::Human),
    ("◦ Подмигнуть", "wink", VerbOwner::Human),
    ("◦ Дрожать", "shiver", VerbOwner::Human),
    ("▷ Смеяться злорадно", "laughevil", VerbOwner::Human),
    ("▷ Мяукнуть", "meow_t", VerbOwner::Tajaran),
    ("▷ Мяукнуть раздражённо", "mow", VerbOwner::Tajaran),
    ("▷ Мурчать", "purr_t", VerbOwner::Tajaran),
    ("▷ Мурчать кратко", "pur", VerbOwner::Tajaran),
    ("▷ Мурчать дольше", "purrr", VerbOwner::Tajaran),
    ("▷ Шипеть", "hiss", VerbOwner::Tajaran),
    ("▷ Шипеть", "hiss_t", VerbOwner::Tajaran),
    ("▷ Выть", "howl", VerbOwner::Vulpkanin),
    ("▷ Рычать", "growl", VerbOwner::Vulpkanin),
    ("▷ Урчать", "purr", VerbOwner::Vulpkanin),
    ("▷ Гавкнуть", "bark", VerbOwner::Vulpkanin),
    ("▷ Дважды гавкнуть", "wbark", VerbOwner::Vulpkanin),
    ("○ Расправить крылья", "wing", VerbOwner::Moth),
];

/// The emote verbs of a human of this kind, sorted by name as the stat browser lists them.
/// A verb whose emote the port does not have yet is hidden (it would only answer "does not
/// exist").
pub fn emote_verbs(tajaran: bool, vulpkanin: bool, moth: bool) -> Vec<&'static str> {
    let mut verbs: Vec<&'static str> = EMOTE_VERBS
        .iter()
        .filter(|(_, _, owner)| match owner {
            VerbOwner::Human => true,
            VerbOwner::Tajaran => tajaran,
            VerbOwner::Vulpkanin => vulpkanin,
            VerbOwner::Moth => moth,
            VerbOwner::Tail => tajaran || vulpkanin,
        })
        .filter(|(_, key, _)| emote_exists(key))
        .map(|(name, _, _)| *name)
        .collect();
    verbs.sort_unstable();
    verbs.dedup();
    verbs
}

#[cfg(feature = "server")]
fn emote_exists(key: &str) -> bool {
    ss13_chat::emote::find(key).is_some()
}

#[cfg(not(feature = "server"))]
fn emote_exists(_: &str) -> bool {
    true
}

/// The emote key of a verb shown in `verbs`; `None` when the name is not among them. Two verbs
/// may share a name (the tajaran's two "▷ Шипеть"): the first one with an existing emote runs.
pub fn emote_of_verb(verbs: &[&str], name: &str) -> Option<&'static str> {
    if !verbs.contains(&name) {
        return None;
    }
    EMOTE_VERBS
        .iter()
        .find(|(n, key, _)| *n == name && emote_exists(key))
        .map(|(_, key, _)| *key)
}

#[cfg(feature = "server")]
pub use server::*;

#[cfg(feature = "server")]
mod server {
    use std::collections::HashMap;

    use bevy::ecs::message::{MessageCursor, Messages};
    use bevy::prelude::*;
    use ss13_net::server::{
        ClientConnected, ClientDisconnected, ClientToken, ControlledEntity, IntentReceived,
    };
    use ss13_shared::protocol::ClientIntent;
    use ss13_tgui::server::{
        AlwaysState, AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems,
    };

    use super::*;
    use crate::ghost::Ghost;
    use crate::human::HumanLook;
    use crate::lobby::GameState;
    use crate::species::{Species, SpeciesId};

    /// How often the panels are rebuilt.
    const REFRESH_SECS: f64 = 1.0;

    /// A client's statpanel host, off the map.
    #[derive(Component, Clone, Debug)]
    pub struct StatPanelHost {
        pub client: Entity,
        pub data: StatPanelData,
    }

    /// Client -> its [`StatPanelHost`]; `fresh`: a host was added since the last refresh.
    #[derive(Resource, Default, Debug)]
    pub struct StatPanelHosts {
        pub hosts: HashMap<Entity, Entity>,
        fresh: bool,
    }

    /// A parsed [`VERB`] act.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct VerbAct {
        pub tab: String,
        pub name: String,
    }

    pub struct StatPanelServerPlugin;

    impl Plugin for StatPanelServerPlugin {
        fn build(&self, app: &mut App) {
            app.register_ui::<StatPanelUi>()
                .init_resource::<StatPanelHosts>()
                .add_systems(Update, (hosts, refresh).chain().before(UiSystems::Open))
                .add_systems(Update, run_verbs.in_set(UiSystems::Handle));
        }
    }

    /// A client that passed the handshake gets its host and window; a gone client's host goes.
    fn hosts(
        world: &mut World,
        mut connected: Local<MessageCursor<ClientConnected>>,
        mut disconnected: Local<MessageCursor<ClientDisconnected>>,
    ) {
        let joined: Vec<Entity> = connected
            .read(world.resource::<Messages<ClientConnected>>())
            .map(|c| c.client)
            .collect();
        let left: Vec<Entity> = disconnected
            .read(world.resource::<Messages<ClientDisconnected>>())
            .map(|d| d.client)
            .collect();
        for client in left {
            if let Some(host) = world.resource_mut::<StatPanelHosts>().hosts.remove(&client) {
                world.try_despawn(host).ok();
            }
        }
        for client in joined {
            if world.get_entity(client).is_err()
                || world
                    .resource::<StatPanelHosts>()
                    .hosts
                    .contains_key(&client)
            {
                continue;
            }
            let host = world
                .spawn((
                    Name::new("statpanel host"),
                    StatPanelHost {
                        client,
                        data: StatPanelData::default(),
                    },
                    AlwaysState,
                ))
                .id();
            let mut hosts = world.resource_mut::<StatPanelHosts>();
            hosts.hosts.insert(client, host);
            hosts.fresh = true;
            // Actor = the client, as the escape menu: windows of other actors close on ghosting.
            world.write_message(OpenUi::new::<StatPanelUi>(
                client,
                client,
                host,
                "Статпанель",
            ));
        }
    }

    /// The verb tabs of a mob: name, verbs (sorted).
    pub fn verb_tabs(world: &World, mob: Entity) -> Vec<(&'static str, Vec<&'static str>)> {
        let mut tabs = Vec::new();
        if world.get::<HumanLook>(mob).is_some() && world.get::<Ghost>(mob).is_none() {
            let species = world.get::<Species>(mob).map(|s| s.0);
            let verbs = emote_verbs(
                species == Some(SpeciesId::Tajaran),
                species == Some(SpeciesId::Vulpkanin),
                species == Some(SpeciesId::Moth),
            );
            if !verbs.is_empty() {
                tabs.push((EMOTES_TAB, verbs));
            }
        }
        tabs.push((
            OOC_TAB,
            vec![OOC_VERB_IGNORE, OOC_VERB_UNIGNORE, OOC_VERB_MOTD],
        ));
        tabs
    }

    /// tg `SSstatpanels` `global_data`: what every Status tab shows.
    fn status_lines(world: &World, players: usize) -> Vec<String> {
        let started = world
            .get_resource::<crate::round::Ticker>()
            .is_some_and(|t| t.state() >= GameState::Playing);
        let round_time = if started {
            crate::records::manifest::round_timestamp(world)
        } else {
            "Pre-Game".to_owned()
        };
        vec![
            format!("Станция: {}", crate::round::station_name(world)),
            format!("Время раунда: {round_time}"),
            format!("Игроков: {players}"),
        ]
    }

    /// Rebuilds every panel once a second (and at once for a new host).
    // Cost: once a second, one pass over the hosts, a few component reads each.
    fn refresh(world: &mut World, mut next: Local<f64>) {
        let now = world.resource::<Time>().elapsed_secs_f64();
        let fresh = std::mem::take(&mut world.resource_mut::<StatPanelHosts>().fresh);
        if now < *next && !fresh {
            return;
        }
        *next = now + REFRESH_SECS;
        let players = world.query::<&ClientToken>().iter(world).count();
        let lines = status_lines(world, players);
        let mut query = world.query::<(Entity, &StatPanelHost)>();
        let hosts: Vec<(Entity, Entity)> = query.iter(world).map(|(e, h)| (e, h.client)).collect();
        for (host, client) in hosts {
            let mob = world.get::<ControlledEntity>(client).map(|c| c.0);
            let mut status = lines.clone();
            if let Some(mob) = mob {
                status.extend(crate::silicon::ai_status::status_lines(world, mob));
            }
            let mut tabs = vec![StatTab {
                name: STATUS_TAB.to_owned(),
                lines: status,
                verbs: Vec::new(),
            }];
            if let Some(mob) = mob {
                tabs.extend(
                    verb_tabs(world, mob)
                        .into_iter()
                        .map(|(name, verbs)| StatTab {
                            name: name.to_owned(),
                            lines: Vec::new(),
                            verbs: verbs.into_iter().map(str::to_owned).collect(),
                        }),
                );
            }
            let data = StatPanelData { tabs };
            if let Some(mut host) = world.get_mut::<StatPanelHost>(host)
                && host.data != data
            {
                host.data = data;
            }
        }
    }

    impl ServerUi for StatPanelUi {
        type Source = &'static StatPanelHost;
        type UserParam = ();
        type Act = VerbAct;

        fn static_data(_: &StatPanelHost) {}

        fn data(host: &StatPanelHost, _: Entity, _: &()) -> StatPanelData {
            host.data.clone()
        }

        fn parse_act(action: &str, params: &UiParams) -> Result<VerbAct, UiReject> {
            if action != VERB {
                return Err(UiReject::UnknownAction);
            }
            Ok(VerbAct {
                tab: params.text(TAB)?,
                name: params.text(NAME)?,
            })
        }
    }

    /// A verb button: the verb must be one the client's mob has now (looked up again here);
    /// an emote verb runs `emote(key, intentional = TRUE)`, which is what `*key` in say does,
    /// so it goes through the say path (rate, mute, stat and cooldown checks).
    fn run_verbs(world: &mut World, mut acts: Local<MessageCursor<UiActed<StatPanelUi>>>) {
        let pending: Vec<(Entity, Entity, VerbAct)> = acts
            .read(world.resource::<Messages<UiActed<StatPanelUi>>>())
            .map(|a| (a.client, a.source, a.act.clone()))
            .collect();
        for (client, source, act) in pending {
            if world.get::<StatPanelHost>(source).map(|h| h.client) != Some(client) {
                continue;
            }
            let Some(mob) = world.get::<ControlledEntity>(client).map(|c| c.0) else {
                continue;
            };
            let key = verb_tabs(world, mob)
                .into_iter()
                .find(|(tab, _)| *tab == act.tab)
                .and_then(|(tab, verbs)| {
                    (tab == EMOTES_TAB)
                        .then(|| emote_of_verb(&verbs, &act.name))
                        .flatten()
                });
            if let Some(key) = key {
                world.write_message(IntentReceived {
                    client,
                    intent: ClientIntent::Say(format!("*{key}")),
                });
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn species_get_their_emote_verbs() {
            let human = emote_verbs(false, false, false);
            assert!(human.contains(&"▷ Смеяться") && human.contains(&"◦ Кивнуть"));
            assert!(!human.contains(&"▷ Мяукнуть") && !human.contains(&"▷ Выть"));
            // Sorted as the stat browser lists them.
            assert!(human.windows(2).all(|w| w[0] <= w[1]));
            let tajaran = emote_verbs(true, false, false);
            assert!(tajaran.contains(&"▷ Мяукнуть"));
            let vulpkanin = emote_verbs(false, true, false);
            assert!(vulpkanin.contains(&"▷ Выть") && vulpkanin.contains(&"▷ Урчать"));
            // Verbs of emotes the port lacks are hidden.
            assert!(!human.contains(&"▷ Храпеть"));
        }

        #[test]
        fn only_shown_verbs_run() {
            let human = emote_verbs(false, false, false);
            assert_eq!(emote_of_verb(&human, "▷ Смеяться"), Some("laugh"));
            assert_eq!(emote_of_verb(&human, "▷ Мяукнуть"), None);
            assert_eq!(emote_of_verb(&human, "что угодно"), None);
            // The tajaran's second "▷ Шипеть" is its own `hiss_t` (no carbon `hiss` yet).
            let tajaran = emote_verbs(true, false, false);
            assert_eq!(emote_of_verb(&tajaran, "▷ Шипеть"), Some("hiss_t"));
        }
    }
}
