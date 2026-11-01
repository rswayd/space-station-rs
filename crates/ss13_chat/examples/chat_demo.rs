//! The chat panel with every message kind, filled locally (no server).
//!
//! `cargo run -p ss13_chat --features client --example chat_demo`
//! `cargo run -p ss13_chat --features client --example chat_demo -- --screenshot out.png` saves a
//! PNG and exits.

use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use ss13_chat::client::panel::SayInput;
use ss13_chat::client::{ChatLog, ChatPanelPlugin};
use ss13_net::client::SendIntent;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
use ss13_ui_kit::UiKitPlugin;
use ss13_ui_kit::prelude::*;

fn main() {
    let screenshot = std::env::args().skip_while(|a| a != "--screenshot").nth(1);
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins.set(WindowPlugin {
            primary_window: Some(bevy::window::Window {
                title: "ss13_chat demo".into(),
                resolution: (1280, 720).into(),
                ..default()
            }),
            ..default()
        }),
        UiKitPlugin,
        ChatPanelPlugin,
    ))
    .insert_resource(ClearColor(Color::srgb(0.05, 0.06, 0.08)))
    .init_resource::<ChatLog>()
    .add_message::<SendIntent>()
    .add_systems(
        Startup,
        (fill, |mut commands: Commands| {
            commands.spawn(Camera2d);
        }),
    )
    .add_systems(Update, echo_says);
    if let Some(path) = screenshot {
        app.add_systems(
            Update,
            move |mut frames: Local<u32>,
                  mut commands: Commands,
                  mut inputs: Query<&mut Input, With<SayInput>>,
                  mut exit: MessageWriter<AppExit>| {
                *frames += 1;
                if *frames == 5 {
                    for mut input in &mut inputs {
                        input.value = "Кто-нибудь, откройте шлюз!".into();
                    }
                }
                if *frames == 30 {
                    commands
                        .spawn(Screenshot::primary_window())
                        .observe(save_to_disk(path.clone()));
                }
                if *frames == 40 {
                    exit.write(AppExit::Success);
                }
            },
        );
    }
    app.run();
}

fn seg(span: Span, text: &str) -> ChatSegment {
    ChatSegment::new(span, text)
}

fn say(name: &str, verb: &str, text: &str) -> ChatMessage {
    ChatMessage {
        channel: ChatChannel::Local,
        segments: vec![
            seg(Span::Name, name),
            seg(Span::Say, &format!(" {verb}, \"")),
            seg(Span::Say, text),
            seg(Span::Say, "\""),
        ],
    }
}

fn fill(mut log: ResMut<ChatLog>) {
    let simple = ChatMessage::simple;
    let messages = [
        simple(
            ChatChannel::System,
            Span::BoldNotice,
            "Подключено к серверу.",
        ),
        simple(
            ChatChannel::System,
            Span::BoldNotice,
            "Добро пожаловать на станцию! Смена №42, сектор Альфа.",
        ),
        say("Иван Петров", "спрашивает", "Кто-нибудь видел капитана?"),
        say("Боб Смит", "восклицает", "Привет!"),
        say("Боб Смит", "восклицает", "Привет!"),
        say("Боб Смит", "восклицает", "Привет!"),
        ChatMessage {
            channel: ChatChannel::Radio,
            segments: vec![
                seg(Span::Radio, "[Общий] "),
                seg(Span::Name, "Анна Кузнецова"),
                seg(
                    Span::Radio,
                    " говорит, \"Медбей, нужна помощь в коридоре.\"",
                ),
            ],
        },
        ChatMessage {
            channel: ChatChannel::Local,
            segments: vec![seg(Span::Name, "Иван Петров"), seg(Span::Emote, " кивает.")],
        },
        simple(ChatChannel::Info, Span::Info, "Вы чувствуете себя лучше."),
        simple(
            ChatChannel::Info,
            Span::Notice,
            "Вы надеваете изолированные перчатки.",
        ),
        simple(
            ChatChannel::Warning,
            Span::Warning,
            "Вы слишком устали, чтобы бежать!",
        ),
        simple(
            ChatChannel::Combat,
            Span::Danger,
            "Ассистент бьёт вас ящиком с инструментами!",
        ),
        simple(
            ChatChannel::Warning,
            Span::BoldAnnounce,
            "Внимание! Шаттл эвакуации прибудет через 10 минут.",
        ),
        ChatMessage {
            channel: ChatChannel::Ooc,
            segments: vec![
                seg(Span::Ooc, "OOC: Gr1mm: "),
                seg(Span::Plain, "всем удачного раунда"),
            ],
        },
        say(
            "Боб Смит",
            "говорит",
            "<b>это не жирный</b> и [i]не курсив[/i]: разметка игрока показывается как есть, \
             а длинная строка переносится по словам, как в tgui-panel.",
        ),
    ];
    for message in messages {
        log.push(message, 0.0);
    }
}

/// Stand-in for the server: what the player says comes back as a say.
fn echo_says(mut said: MessageReader<SendIntent>, time: Res<Time>, mut log: ResMut<ChatLog>) {
    for SendIntent(intent) in said.read() {
        if let ss13_shared::protocol::ClientIntent::Say(text) = intent {
            log.push(say("Вы", "говорите", text), time.elapsed_secs_f64());
        }
    }
}
