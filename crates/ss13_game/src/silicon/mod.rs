//! Silicons and cameras, after tg `/mob/living/silicon` (`code/modules/mob/living/silicon/`):
//!
//! - [`cameras`] (server): security cameras, `SScameras` chunks, the camera console;
//! - [`ai`] (server): the AI in its core, its eye, AI clicks on doors and APCs;
//! - [`cyborg`] (server): cyborgs, their models, cell and the recharging station;
//! - [`laws`] (server): lawsets, the round default, lawsync;
//! - [`interact`] (server): silicons opening machine windows (`attack_ai`), their window
//!   status; [`airlock`] (server): the airlock's `AiAirlock` window;
//! - [`ui`]: the windows' data both halves share; [`client`] (client) draws the camera view's
//!   static, the camera console and the list input.
//!
//! What every silicon shares (here): no lungs, stomach or reagents (`crate::breath`,
//! `crate::hunger`, `crate::chem` skip [`Silicon`]), brute and burn only, no crit: tg
//! `/mob/living/silicon/robot/update_stat` and the AI's die at `-maxHealth` and are conscious
//! until then; the binary chat (`:b`, tg `robot_talk`); a built-in radio
//! (`crate::radio::BuiltInRadio`).
// shortcut: a dead silicon stays where it died, inert, with the carbon deathgasp text; no
// gibbing, debris or MMI; no EMP.

#[cfg(feature = "server")]
pub mod ai;
#[cfg(feature = "server")]
pub mod airlock;
#[cfg(feature = "server")]
pub mod cameras;
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod cyborg;
#[cfg(feature = "server")]
pub mod interact;
#[cfg(feature = "server")]
pub mod laws;
pub mod ui;

#[cfg(feature = "server")]
pub use server::*;

#[cfg(feature = "server")]
mod server {
    use bevy::prelude::*;
    use ss13_chat::server::{Listeners, SaySystems, Spoke};
    use ss13_chat::speech::{BinaryCapable, EarsWorn, RadioTarget};
    use ss13_mapload::MapLoadPlugin;
    use ss13_mapload::jobs::Job;
    use ss13_mapload::silicon::{SILICON_FILE, SiliconData};
    use ss13_net::chat::server::SendChat;
    use ss13_net::server::{ControlledEntity, SeeInvisible, Sight, Viewer};
    use ss13_shared::TilePos;
    use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};
    use ss13_tgui::server::{AppTguiExt, ServerUi, UiParams, UiReject};

    use super::laws::Laws;
    use super::ui::{ListInputStatic, ListInputUi, SUBMIT};
    use crate::ghost::Ghost;
    use crate::health::{Health, HealthSystems, Stat};
    use crate::radio::{BuiltInRadio, Radio, RadioKind, SiliconRadio};
    use crate::server::{MoveQueue, PlayerMob, Players};

    /// tg `issilicon`: on AIs and cyborgs.
    #[derive(Component, Clone, Copy, Debug, Default)]
    pub struct Silicon;

    /// The lawsets and cyborg models from `ss13-mapload convert` (`<assets>/silicon.ron`).
    #[derive(Resource, Clone, Debug, Default)]
    pub struct Silicons(pub SiliconData);

    pub struct SiliconServerPlugin;

    impl Plugin for SiliconServerPlugin {
        fn build(&self, app: &mut App) {
            app.init_resource::<Silicons>()
                .register_ui::<ListInputWindow>()
                .add_plugins((
                    super::cameras::CamerasPlugin,
                    super::ai::AiPlugin,
                    super::cyborg::CyborgPlugin,
                    super::interact::SiliconInteractPlugin,
                    super::airlock::AiAirlockPlugin,
                ))
                .add_systems(PreStartup, load_data)
                .add_systems(
                    Update,
                    (
                        silicon_stat.before(HealthSystems),
                        binary_chat.after(SaySystems),
                    ),
                );
        }
    }

    fn load_data(world: &mut World) {
        let Some(path) = world
            .get_resource::<MapLoadPlugin>()
            .map(|m| m.assets.join(SILICON_FILE))
        else {
            return;
        };
        match SiliconData::read(&path) {
            Ok(data) => {
                info!(
                    "silicons: {} lawsets, {} cyborg models",
                    data.lawsets.len(),
                    data.models.len()
                );
                world.insert_resource(Silicons(data));
            }
            Err(e) if path.exists() => error!("{e:#}"),
            Err(_) => warn!("no {SILICON_FILE} (rerun `ss13-mapload convert`): default laws only"),
        }
    }

    /// tg `/datum/job/ai`, `/datum/job/cyborg`: their bodies are silicons. `None` for other
    /// jobs.
    pub(crate) fn join(
        world: &mut World,
        client: Entity,
        job: usize,
        def: &Job,
        name: &str,
        latejoin: bool,
    ) -> Option<Entity> {
        match def.path.as_str() {
            "/datum/job/ai" => super::ai::join(world, client, job, name, latejoin),
            "/datum/job/cyborg" => super::cyborg::join(world, client, job, latejoin),
            _ => None,
        }
    }

    /// `client` now controls `mob` at `pos` (the replication viewer and the step numbering
    /// follow); its previous body is despawned.
    pub(crate) fn attach_client(world: &mut World, client: Entity, mob: Entity, pos: TilePos) {
        let old = world.resource::<Players>().mob(client);
        let queue = old
            .and_then(|o| world.get::<MoveQueue>(o))
            .map(MoveQueue::continued)
            .unwrap_or_default();
        world.entity_mut(mob).insert((PlayerMob { client }, queue));
        world
            .entity_mut(client)
            // A ghost's sight does not come along into a body.
            .remove::<(SeeInvisible, Sight)>()
            .insert((Viewer::new(pos), ControlledEntity(mob)));
        world.resource_mut::<Players>().set_mob(client, mob);
        if let Some(old) = old.filter(|&o| o != mob) {
            world.try_despawn(old).ok();
        }
    }

    /// A built-in radio for `listener` (tg `/obj/item/radio/headset/silicon/ai`,
    /// `/obj/item/radio/borg`): Common plus `channels`, all listened to.
    pub(crate) fn give_radio(world: &mut World, listener: Entity, channels: &[String]) {
        let radio = world
            .spawn((
                Radio {
                    kind: RadioKind::Headset,
                    on: true,
                    frequency: ss13_chat::radio::FREQ_COMMON,
                    broadcasting: true,
                    listening: true,
                    canhear_range: 0,
                    special_channels: 0,
                    channels: channels.iter().map(|c| (c.clone(), true)).collect(),
                    tx_cut: false,
                },
                BuiltInRadio { listener },
                Name::new("integrated radio"),
                ChildOf(listener),
            ))
            .id();
        world
            .entity_mut(listener)
            .insert((SiliconRadio(radio), EarsWorn, BinaryCapable));
    }

    /// tg `/mob/living/silicon/proc/show_laws`.
    pub fn laws_message(laws: &Laws) -> ChatMessage {
        let mut text = String::new();
        for line in laws.law_list(true) {
            text.push('\n');
            text.push_str(&line);
        }
        ChatMessage {
            channel: ChatChannel::Local,
            segments: vec![
                ChatSegment::new(Span::Bold, "Подчиняйтесь этим законам:"),
                ChatSegment::new(Span::Plain, text),
            ],
        }
    }

    pub(crate) fn show_laws(world: &mut World, client: Entity, laws: &Laws) {
        world.write_message(SendChat {
            clients: vec![client],
            message: laws_message(laws),
        });
    }

    pub(crate) fn notice(world: &mut World, client: Entity, span: Span, text: impl Into<String>) {
        world.write_message(SendChat {
            clients: vec![client],
            message: ChatMessage::simple(ChatChannel::Local, span, text),
        });
    }

    /// Silicons take no oxygen or toxin damage and have no crit (tg robot and AI
    /// `update_stat`: dead at `-maxHealth`, conscious above).
    fn silicon_stat(
        mut mobs: Query<
            (&mut Health, &mut Stat),
            (With<Silicon>, Or<(Changed<Health>, Changed<Stat>)>),
        >,
    ) {
        for (mut health, mut stat) in &mut mobs {
            if health.tox != 0.0 || health.oxy != 0.0 {
                health.tox = 0.0;
                health.oxy = 0.0;
            }
            let wanted = if *stat == Stat::Dead || health.health() <= -health.max {
                Stat::Dead
            } else {
                Stat::Conscious
            };
            stat.set_if_neq(wanted);
        }
    }

    /// tg `robot_talk`: every binary-capable mob reads `Robotic Talk, Name "..."`, ghosts too.
    fn binary_chat(
        mut spoken: MessageReader<Spoke>,
        hearers: Query<Entity, With<BinaryCapable>>,
        ghosts: Query<Entity, With<Ghost>>,
        designations: Query<&super::ai::AiEye>,
        listeners: Res<Listeners>,
        mut chat: MessageWriter<SendChat>,
    ) {
        for spoke in spoken.read() {
            if spoke.radio != Some(RadioTarget::Binary) {
                continue;
            }
            let clients: Vec<Entity> = hearers
                .iter()
                .chain(ghosts.iter())
                .filter_map(|e| listeners.client_of(e))
                .collect();
            let job = if designations.contains(spoke.speaker) {
                "ИИ"
            } else {
                "Киборг"
            };
            info!(target: "say", "binary {} ({job}): {}", spoke.name, spoke.text);
            chat.write(SendChat {
                clients,
                message: binary_message(&spoke.name, spoke.verb, &spoke.text),
            });
        }
    }

    /// `span_binarysay("Robotic Talk, [name] [messagepart]")`, BandaStation's translation.
    pub fn binary_message(name: &str, verb: &str, text: &str) -> ChatMessage {
        ChatMessage {
            channel: ChatChannel::Radio,
            segments: vec![
                ChatSegment::new(Span::BinarySay, "Робоболтовня, "),
                ChatSegment::new(Span::Name, name),
                ChatSegment::new(Span::BinarySay, format!(" {verb}, \"")),
                ChatSegment::new(Span::BinarySay, text),
                ChatSegment::new(Span::BinarySay, "\""),
            ],
        }
    }

    /// The server side of `ListInputModal` (tg `tgui_input_list`): the source holds the
    /// choices, the act names one of them.
    pub struct ListInputWindow;

    impl ss13_tgui::UiInterface for ListInputWindow {
        const NAME: &'static str = ListInputUi::NAME;
        type Data = ();
        type StaticData = ListInputStatic;
    }

    /// On a window source: the list it offers.
    #[derive(Component, Clone, Debug, Default)]
    pub struct ListInput(pub ListInputStatic);

    impl ServerUi for ListInputWindow {
        type Source = &'static ListInput;
        type UserParam = ();
        type Act = String;

        fn static_data(source: &ListInput) -> ListInputStatic {
            source.0.clone()
        }

        fn data(_: &ListInput, _: Entity, _: &()) {}

        fn parse_act(action: &str, params: &UiParams) -> Result<String, UiReject> {
            if action != SUBMIT {
                return Err(UiReject::UnknownAction);
            }
            params
                .get("entry")
                .map(str::to_owned)
                .ok_or_else(|| UiReject::BadParams("entry: missing".into()))
        }
    }
}
