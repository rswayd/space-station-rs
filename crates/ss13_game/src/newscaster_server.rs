//! Newscasters and the station's news network (server), after tg `/obj/machinery/newscaster`
//! (`code/game/machinery/newscaster/newscaster_machine.dm`), `GLOB.news_network`
//! (`newscaster_data.dm`) and `/obj/item/newspaper` (`newspaper.dm`), with BandaStation's
//! Russian alerts and channel names.
//!
//! - [`NewsNetwork`]: the feed channels (the station announcements and SpaceBet channels exist
//!   from the start), their stories and comments, the wanted issue, `last_action`.
//!   [`submit_article`] posts a story and has every newscaster announce it; priority
//!   announcements (`crate::announce`) and the economy's earnings report post there too.
//! - A newscaster (mapped `/obj/machinery/newscaster`) opens the `PhysicalNewscaster` window on
//!   an empty-hand click while powered; paper clicked on it is loaded for printing.
//! - The window: pick a channel, submit a story (a text prompt; a locked channel takes its
//!   author's stories only, a censored one none), create a channel (name, description, public or
//!   locked; one per author), comment on a story, censor stories, authors and whole channels
//!   (D-notice: `ACCESS_LIBRARY` on the ID), issue and clear the wanted issue
//!   (`ACCESS_SECURITY`), print a newspaper. The poster's name is the bank account holder of the
//!   ID found hand first (`get_idcard(hand_first = TRUE)`), or a silicon's name.
//! - Every act is checked here at the moment of effect, and again when a prompt is answered:
//!   power, the actor's reach, the ID's access, the channel's lock and censorship.
// shortcut: no photos (no cameras), bounty board, cross-sector channels, screen overlays and
// light, soft/IC chat filters, wall frames or repairs, NTOS newscaster app; the newspaper's
// contents are its description (no newspaper reading window, scribbles or peepholes); a
// silicon cannot open newscasters (no `attack_ai` routing).

use bevy::ecs::message::MessageCursor;
use bevy::ecs::message::Messages;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::fidelity::is_under;
use ss13_mapload::{DmItem, DmName, DmObj};
use ss13_net::chat::server::SendChat;
use ss13_shared::TilePos;
use ss13_shared::chat::{ChatChannel, ChatMessage, Span};
use ss13_tgui::prompt::{Ask, PromptAnswer, PromptAnswered, PromptKind, PromptSystems};
use ss13_tgui::server::{
    AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems, UiWindow,
};
use ss13_tgui::{UiInterface as _, UiStatus};

use crate::cargo::economy::{BankAccountId, Economy};
use crate::click::{ClickResolved, ResolveClicks};
use crate::health::{Health, Stat};
use crate::inventory::{IdCard, IdCards, Inventory};
use crate::newscaster::*;
use crate::power::Unpowered;
use crate::silicon::interact::{SiliconUsers, has_silicon_access};

pub const NEWSCASTER_ROOT: &str = "/obj/machinery/newscaster";
const PAPER: &str = "/obj/item/paper";
const NEWSPAPER: &str = ss13_mapload::runtime::NEWSPAPER;
/// `NEWSCASTER_STATION_ANNOUNCEMENTS`, `NEWSCASTER_SPACE_BETTING`.
pub const STATION_ANNOUNCEMENTS: &str = "Станционные оповещения";
pub const SPACE_BETTING: &str = "SpaceBet";
/// `admin_access`, `security_access`.
const ACCESS_LIBRARY: &str = "library";
const ACCESS_SECURITY: &str = crate::id_access::ACCESS_SECURITY;
/// `MAX_NAME_LEN`, `MAX_BROADCAST_LEN`, `MAX_MESSAGE_LEN`.
const MAX_NAME_LEN: u32 = 42;
const MAX_BROADCAST_LEN: u32 = 512;
const MAX_MESSAGE_LEN: u32 = 1024;
/// `GLOB.news_network.redacted_text`.
pub const REDACTED: &str = "[REDACTED]";
const NEWS_SOUND: &str = "sound/machines/beep/twobeep_high.ogg";
const WANTED_SOUND: &str = "sound/machines/warning-buzzer.ogg";
const PAGE_TURN: &str = "sound/effects/page_turn/pageturn1.ogg";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedComment {
    pub author: String,
    pub body: String,
    pub time: String,
}

/// `/datum/feed_message`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedMessage {
    /// `message_id`: the round's message count when it was posted.
    pub id: u32,
    pub author: String,
    pub body: String,
    pub time: String,
    pub author_censor: bool,
    pub body_censor: bool,
    /// `locked`: no comments.
    pub locked: bool,
    pub comments: Vec<FeedComment>,
}

/// `/datum/feed_channel`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedChannel {
    pub id: u32,
    pub name: String,
    pub desc: String,
    pub author: String,
    pub locked: bool,
    /// D-notice: the channel cannot be read.
    pub censored: bool,
    pub author_censor: bool,
    pub messages: Vec<FeedMessage>,
}

impl FeedChannel {
    /// `return_author(-1)`.
    pub fn shown_author(&self) -> &str {
        if self.author_censor {
            REDACTED
        } else {
            &self.author
        }
    }
}

/// `/datum/wanted_message`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Wanted {
    pub active: bool,
    pub criminal: String,
    pub body: String,
    pub scanned_user: String,
}

/// `GLOB.news_network`.
#[derive(Resource, Clone, Debug)]
pub struct NewsNetwork {
    pub channels: Vec<FeedChannel>,
    pub wanted: Wanted,
    pub last_action: u32,
    next_channel_id: u32,
    message_count: u32,
}

impl Default for NewsNetwork {
    fn default() -> Self {
        let mut net = Self {
            channels: Vec::new(),
            wanted: Wanted::default(),
            last_action: 0,
            next_channel_id: 1,
            message_count: 0,
        };
        net.create_channel(
            STATION_ANNOUNCEMENTS,
            "SS13",
            "Company news, staff announcements, and all the latest information. Have a secure \
             shift!",
            true,
        );
        net.create_channel(
            SPACE_BETTING,
            "NtOS",
            "News from the SpaceBet PDA App! Download now and make your own bets!",
            true,
        );
        net
    }
}

impl NewsNetwork {
    /// `create_feed_channel`: its id.
    pub fn create_channel(&mut self, name: &str, author: &str, desc: &str, locked: bool) -> u32 {
        let id = self.next_channel_id;
        self.next_channel_id += 1;
        self.channels.push(FeedChannel {
            id,
            name: name.to_owned(),
            desc: desc.to_owned(),
            author: author.to_owned(),
            locked,
            censored: false,
            author_censor: false,
            messages: Vec::new(),
        });
        id
    }

    pub fn channel(&self, id: u32) -> Option<&FeedChannel> {
        self.channels.iter().find(|c| c.id == id)
    }

    pub fn channel_mut(&mut self, id: u32) -> Option<&mut FeedChannel> {
        self.channels.iter_mut().find(|c| c.id == id)
    }

    pub fn channel_by_name(&self, name: &str) -> Option<&FeedChannel> {
        self.channels.iter().find(|c| c.name == name)
    }

    /// `submit_article` without the alerts: the new message's id, `None` without the channel.
    pub fn post(
        &mut self,
        channel: &str,
        body: &str,
        author: &str,
        allow_comments: bool,
        time: &str,
    ) -> Option<u32> {
        let index = self.channels.iter().position(|c| c.name == channel)?;
        self.message_count += 1;
        self.last_action += 1;
        let id = self.message_count;
        self.channels[index].messages.push(FeedMessage {
            id,
            author: author.to_owned(),
            body: body.to_owned(),
            time: time.to_owned(),
            author_censor: false,
            body_censor: false,
            locked: !allow_comments,
            comments: Vec::new(),
        });
        Some(id)
    }

    fn message_mut(&mut self, channel: u32, message: u32) -> Option<&mut FeedMessage> {
        self.channel_mut(channel)?
            .messages
            .iter_mut()
            .find(|m| m.id == message)
    }

    /// `start_create_channel`: an author may own one channel (a censored author counts as
    /// `[REDACTED]`).
    pub fn owns_channel(&self, author: &str) -> bool {
        self.channels.iter().any(|c| c.shown_author() == author)
    }
}

/// A newscaster's own state (tg machine vars).
#[derive(Component, Clone, Debug, Default, PartialEq, Eq)]
pub struct Newscaster {
    /// `current_channel`: its id.
    pub channel: Option<u32>,
    pub viewing_wanted: bool,
    pub paper: u32,
    pub criminal_name: String,
    pub crime_description: String,
}

/// A printed newspaper: the issue's text (`news_content`, the saved wanted issue).
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct Newspaper {
    pub text: String,
}

/// On a prompt: what it asks for, from which newscaster.
#[derive(Component, Clone, Debug)]
enum NewsPrompt {
    Story {
        caster: Entity,
        channel: u32,
    },
    ChannelName {
        caster: Entity,
    },
    ChannelDesc {
        caster: Entity,
        name: String,
    },
    ChannelLock {
        caster: Entity,
        name: String,
        desc: String,
    },
    Comment {
        caster: Entity,
        channel: u32,
        message: u32,
    },
    CriminalName {
        caster: Entity,
    },
    CrimeData {
        caster: Entity,
    },
}

impl NewsPrompt {
    fn caster(&self) -> Entity {
        match self {
            Self::Story { caster, .. }
            | Self::ChannelName { caster }
            | Self::ChannelDesc { caster, .. }
            | Self::ChannelLock { caster, .. }
            | Self::Comment { caster, .. }
            | Self::CriminalName { caster }
            | Self::CrimeData { caster } => *caster,
        }
    }
}

pub struct NewscasterPlugin;

impl Plugin for NewscasterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NewsNetwork>()
            .register_ui::<NewscasterUi>()
            .add_observer(setup_newscaster)
            .add_systems(
                Update,
                (
                    clicks
                        .after(ss13_world::WorldSystems::Movement)
                        .after(ResolveClicks)
                        .before(UiSystems::Open),
                    acts.in_set(UiSystems::Handle),
                    answers
                        .in_set(UiSystems::Handle)
                        .after(PromptSystems::Answer)
                        .after(acts),
                    unpowered_windows.in_set(UiSystems::StatusHook),
                ),
            );
    }
}

fn setup_newscaster(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    if objs
        .get(add.entity)
        .is_ok_and(|o| is_under(&o.path, NEWSCASTER_ROOT))
    {
        commands
            .entity(add.entity)
            .try_insert(Newscaster::default());
    }
}

/// `attack_hand`: the window; `item_interaction` with paper: one more sheet.
fn clicks(world: &mut World, mut cursor: Local<MessageCursor<ClickResolved>>) {
    let clicks: Vec<ClickResolved> = cursor
        .read(world.resource::<Messages<ClickResolved>>())
        .copied()
        .collect();
    for click in clicks {
        if world.get::<Newscaster>(click.target).is_none() {
            continue;
        }
        let held = world
            .get::<Inventory>(click.mob)
            .and_then(Inventory::active_item);
        match held {
            None => {
                if world.get::<Unpowered>(click.target).is_some() {
                    continue;
                }
                let title = world
                    .get::<DmName>(click.target)
                    .map(|n| crate::vending::plain_name(&n.name).to_owned())
                    .unwrap_or_default();
                world.write_message(OpenUi::new::<NewscasterUi>(
                    click.client,
                    click.mob,
                    click.target,
                    title,
                ));
            }
            Some(item)
                if world
                    .get::<DmItem>(item)
                    .is_some_and(|i| is_under(&i.path, PAPER)) =>
            {
                world.despawn(item);
                let paper = {
                    let mut caster = world.get_mut::<Newscaster>(click.target).expect("checked");
                    caster.paper += 1;
                    caster.paper
                };
                tell(
                    world,
                    click.client,
                    Span::Notice,
                    &format!(
                        "Вы вставляете бумагу в новостной терминал! Теперь в нём {paper} лист(ов) \
                         бумаги."
                    ),
                );
            }
            Some(_) => {}
        }
    }
}

/// An unpowered newscaster is update-only.
fn unpowered_windows(
    mut windows: Query<&mut UiWindow>,
    unpowered: Query<(), (With<Newscaster>, With<Unpowered>)>,
) {
    for mut window in &mut windows {
        if window.interface == NewscasterUi::NAME && unpowered.contains(window.source) {
            window.cap_status(UiStatus::Update);
        }
    }
}

/// Who uses a newscaster: tg `ui_data` `user`, the poster's name and the ID's access.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Poster {
    user: NewsUser,
    /// `newscaster_username`: the account holder of the ID, or a silicon's name.
    username: Option<String>,
    access: Vec<String>,
}

impl Poster {
    fn has(&self, access: &str) -> bool {
        self.access.iter().any(|a| a == access)
    }
}

/// The ID's account: `(holder, job, department)`.
fn account_user(economy: Option<&Economy>, account: Option<u32>) -> Option<NewsUser> {
    let account = economy?.account(account?)?;
    Some(NewsUser {
        name: account.holder.clone(),
        job: if account.job.is_empty() {
            "No Job".to_owned()
        } else {
            account.job.clone()
        },
        department: if account.paycheck_department.is_empty() {
            "Unassigned".to_owned()
        } else {
            account.paycheck_department.clone()
        },
    })
}

fn make_poster(
    silicon: bool,
    name: String,
    card: Option<(&IdCard, Option<u32>)>,
    economy: Option<&Economy>,
) -> Poster {
    if silicon {
        return Poster {
            user: NewsUser {
                name: name.clone(),
                job: "N/A".to_owned(),
                department: "N/A".to_owned(),
            },
            username: Some(name),
            access: Vec::new(),
        };
    }
    let access = card.map(|(c, _)| c.access.clone()).unwrap_or_default();
    match card.and_then(|(_, account)| account_user(economy, account)) {
        Some(user) => Poster {
            username: Some(user.name.clone()),
            user,
            access,
        },
        None => Poster {
            user: NewsUser {
                name,
                job: "N/A".to_owned(),
                department: "N/A".to_owned(),
            },
            username: None,
            access,
        },
    }
}

/// [`Poster`] from the world, for the act handlers.
fn poster(world: &World, actor: Entity) -> Poster {
    let silicon = has_silicon_access(world, actor);
    let card = world
        .get::<Inventory>(actor)
        .and_then(|inv| inv.id_card(world));
    let card = card.and_then(|c| {
        let id = world.get::<IdCard>(c)?;
        Some((id, world.get::<BankAccountId>(c).map(|a| a.0)))
    });
    let name = crate::chem::name_of(world, actor);
    make_poster(silicon, name, card, world.get_resource::<Economy>())
}

type NewsParams = (
    Res<'static, NewsNetwork>,
    Query<'static, 'static, &'static Inventory>,
    IdCards<'static, 'static>,
    Query<'static, 'static, &'static BankAccountId>,
    Option<Res<'static, Economy>>,
    SiliconUsers<'static, 'static>,
    Query<'static, 'static, &'static Name>,
);

impl ServerUi for NewscasterUi {
    type Source = &'static Newscaster;
    type UserParam = NewsParams;
    type Act = NewsAct;

    fn static_data(_: &Newscaster) {}

    fn data(
        caster: &Newscaster,
        user: Entity,
        (net, inventories, cards, accounts, economy, silicons, names): &SystemParamItem<
            '_,
            '_,
            NewsParams,
        >,
    ) -> NewscasterData {
        let card = inventories.get(user).ok().and_then(|inv| {
            let card = inv.id_search().find_map(|e| cards.id_of(e))?;
            Some((cards.card(card)?, accounts.get(card).ok().map(|a| a.0)))
        });
        let name = names
            .get(user)
            .map(|n| n.as_str().to_owned())
            .unwrap_or_default();
        let poster = make_poster(silicons.contains(user), name, card, economy.as_deref());
        ui_data(caster, net, &poster)
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<NewsAct, UiReject> {
        let id = |key: &str| {
            params
                .get(key)
                .and_then(|v| v.trim().parse::<u32>().ok())
                .ok_or_else(|| UiReject::BadParams(format!("{key}: an id")))
        };
        Ok(match action {
            SET_CHANNEL => NewsAct::SetChannel(id(CHANNEL)?),
            CREATE_STORY => NewsAct::CreateStory,
            START_CREATE_CHANNEL => NewsAct::StartCreateChannel,
            CANCEL_CREATION => NewsAct::CancelCreation,
            STORY_CENSOR => NewsAct::StoryCensor(id(MESSAGE_ID)?),
            AUTHOR_CENSOR => NewsAct::AuthorCensor(id(MESSAGE_ID)?),
            CHANNEL_D_NOTICE => NewsAct::ChannelDNotice(id(CHANNEL)?),
            START_COMMENT => NewsAct::StartComment(id(MESSAGE_ID)?),
            TOGGLE_WANTED => NewsAct::ToggleWanted,
            SET_CRIMINAL_NAME => NewsAct::SetCriminalName,
            SET_CRIME_DATA => NewsAct::SetCrimeData,
            SUBMIT_WANTED_ISSUE => NewsAct::SubmitWanted,
            CLEAR_WANTED_ISSUE => NewsAct::ClearWanted,
            PRINT_NEWSPAPER => NewsAct::PrintNewspaper,
            // shortcut: togglePhoto, the bounty board and `clear` (see the module).
            _ => return Err(UiReject::UnknownAction),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NewsAct {
    SetChannel(u32),
    CreateStory,
    StartCreateChannel,
    CancelCreation,
    StoryCensor(u32),
    AuthorCensor(u32),
    ChannelDNotice(u32),
    StartComment(u32),
    ToggleWanted,
    SetCriminalName,
    SetCrimeData,
    SubmitWanted,
    ClearWanted,
    PrintNewspaper,
}

/// `ui_data` + `ui_static_data` for `poster`.
fn ui_data(caster: &Newscaster, net: &NewsNetwork, poster: &Poster) -> NewscasterData {
    let channels = net
        .channels
        .iter()
        .map(|c| NewsChannel {
            id: c.id,
            name: c.name.clone(),
            author: c.shown_author().to_owned(),
            censored: c.censored,
            locked: c.locked,
        })
        .collect();
    let current = caster.channel.and_then(|id| net.channel(id));
    let messages = current
        .map(|c| {
            c.messages
                .iter()
                .map(|m| NewsMessage {
                    id: m.id,
                    auth: if m.author_censor {
                        REDACTED.to_owned()
                    } else {
                        m.author.clone()
                    },
                    body: if m.body_censor {
                        REDACTED.to_owned()
                    } else {
                        m.body.clone()
                    },
                    time: m.time.clone(),
                    censored_message: m.body_censor,
                    censored_author: m.author_censor,
                    comments: m
                        .comments
                        .iter()
                        .map(|c| NewsComment {
                            auth: c.author.clone(),
                            body: c.body.clone(),
                            time: c.time.clone(),
                        })
                        .collect(),
                })
                .collect()
        })
        .unwrap_or_default();
    let (channel_author, channel_desc, channel_locked) = match current {
        Some(c) => (c.author.clone(), c.desc.clone(), c.locked),
        None => (
            "Nanotrasen Inc".to_owned(),
            "Welcome to Newscaster Net. Interface & News networks Operational.".to_owned(),
            true,
        ),
    };
    NewscasterData {
        user: poster.user.clone(),
        admin_mode: poster.has(ACCESS_LIBRARY),
        security_mode: poster.has(ACCESS_SECURITY),
        channels,
        viewing_channel: current.map(|c| c.id),
        viewing_wanted: caster.viewing_wanted,
        channel_name: current.map(|c| c.name.clone()).unwrap_or_default(),
        channel_author,
        channel_desc,
        channel_locked,
        channel_censored: current.is_some_and(|c| c.censored),
        messages,
        wanted: net.wanted.active.then(|| NewsWanted {
            criminal: net.wanted.criminal.clone(),
            crime: net.wanted.body.clone(),
            author: net.wanted.scanned_user.clone(),
        }),
        criminal_name: caster.criminal_name.clone(),
        crime_description: caster.crime_description.clone(),
        paper: caster.paper,
    }
}

fn tell(world: &mut World, client: Entity, span: Span, text: &str) {
    world.write_message(SendChat {
        clients: vec![client],
        message: ChatMessage::simple(ChatChannel::System, span, text),
    });
}

fn sound(world: &mut World, at: Entity, file: &str, volume: f32) {
    if let Some(&pos) = world.get::<TilePos>(at) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, file, volume, true));
    }
}

/// `round_timestamp()`: `hh:mm:ss` of the round.
fn round_timestamp(world: &World) -> String {
    let secs = crate::modpc::programs::messenger::round_secs(world);
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

/// The actor still works this newscaster: it is powered and in the actor's reach (a silicon
/// reaches it anywhere it could open it), the actor is conscious.
fn can_use(world: &World, actor: Entity, caster: Entity) -> bool {
    if world.get::<Newscaster>(caster).is_none() || world.get::<Unpowered>(caster).is_some() {
        return false;
    }
    if has_silicon_access(world, actor) {
        return true;
    }
    let conscious =
        world.get::<Health>(actor).is_none() || world.get::<Stat>(actor) == Some(&Stat::Conscious);
    conscious && crate::storage::in_reach(world, actor, caster)
}

fn text_prompt(
    client: Entity,
    actor: Entity,
    title: &str,
    message: &str,
    max_length: u32,
    multiline: bool,
) -> Ask {
    Ask::new(
        client,
        actor,
        title,
        message,
        PromptKind::Text {
            default: String::new(),
            max_length,
            multiline,
        },
    )
}

/// The newscaster says it (`say`).
fn caster_say(world: &mut World, caster: Entity, text: &str) {
    crate::atmos_machinery::machine_say(world, caster, text);
}

/// May `poster` post stories (and comments) on `channel`: not censored, and a locked channel
/// only by its author.
fn may_post(channel: &FeedChannel, poster: &Poster) -> bool {
    !channel.censored
        && !(channel.locked && poster.username.as_deref() != Some(channel.author.as_str()))
}

fn acts(world: &mut World, mut cursor: Local<MessageCursor<UiActed<NewscasterUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, NewsAct)> = cursor
        .read(world.resource::<Messages<UiActed<NewscasterUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, caster, act) in pending {
        if !can_use(world, actor, caster) {
            continue;
        }
        let user = poster(world, actor);
        let state = world.get::<Newscaster>(caster).cloned().expect("checked");
        match act {
            NewsAct::SetChannel(id) => {
                if world.resource::<NewsNetwork>().channel(id).is_some() {
                    let mut c = world.get_mut::<Newscaster>(caster).expect("checked");
                    c.channel = Some(id);
                    c.viewing_wanted = false;
                }
            }
            NewsAct::CreateStory => {
                let Some(channel) = state.channel else {
                    continue;
                };
                let net = world.resource::<NewsNetwork>();
                if !net.channel(channel).is_some_and(|c| may_post(c, &user)) {
                    continue;
                }
                let ask = text_prompt(
                    client,
                    actor,
                    "Network Channel Handler",
                    "Write your Feed story",
                    MAX_MESSAGE_LEN,
                    true,
                );
                news_ask(world, NewsPrompt::Story { caster, channel }, ask);
            }
            NewsAct::StartCreateChannel => {
                let Some(name) = user.username.as_deref() else {
                    tell(
                        world,
                        client,
                        Span::Warning,
                        "ERROR: User cannot be found or already has an owned feed channel.",
                    );
                    continue;
                };
                if world.resource::<NewsNetwork>().owns_channel(name) {
                    tell(
                        world,
                        client,
                        Span::Warning,
                        "ERROR: User cannot be found or already has an owned feed channel.",
                    );
                    continue;
                }
                let ask = text_prompt(
                    client,
                    actor,
                    "Network Channel Handler",
                    "Channel name",
                    MAX_NAME_LEN,
                    false,
                );
                news_ask(world, NewsPrompt::ChannelName { caster }, ask);
            }
            NewsAct::CancelCreation => {
                let mut c = world.get_mut::<Newscaster>(caster).expect("checked");
                c.viewing_wanted = false;
                c.criminal_name.clear();
                c.crime_description.clear();
            }
            NewsAct::StoryCensor(message) | NewsAct::AuthorCensor(message) => {
                let Some(channel) = state.channel else {
                    continue;
                };
                if !user.has(ACCESS_LIBRARY) {
                    caster_say(world, caster, "Clearance not found.");
                    continue;
                }
                let body = matches!(act, NewsAct::StoryCensor(_));
                let mut net = world.resource_mut::<NewsNetwork>();
                if let Some(m) = net.message_mut(channel, message) {
                    if body {
                        m.body_censor = !m.body_censor;
                    } else {
                        m.author_censor = !m.author_censor;
                    }
                    net.last_action += 1;
                }
            }
            NewsAct::ChannelDNotice(id) => {
                if !user.has(ACCESS_LIBRARY) {
                    caster_say(world, caster, "Clearance not found.");
                    continue;
                }
                let mut net = world.resource_mut::<NewsNetwork>();
                let Some(channel) = net.channel_mut(id) else {
                    continue;
                };
                channel.censored = !channel.censored;
                net.last_action += 1;
                world
                    .get_mut::<Newscaster>(caster)
                    .expect("checked")
                    .channel = Some(id);
            }
            NewsAct::StartComment(message) => {
                let Some(channel) = state.channel else {
                    continue;
                };
                if user.username.is_none() || !can_comment(world, channel, message, &user) {
                    continue;
                }
                let ask = text_prompt(
                    client,
                    actor,
                    "Network Channel Handler",
                    "Your comment",
                    MAX_BROADCAST_LEN,
                    true,
                );
                news_ask(
                    world,
                    NewsPrompt::Comment {
                        caster,
                        channel,
                        message,
                    },
                    ask,
                );
            }
            NewsAct::ToggleWanted => {
                world
                    .get_mut::<Newscaster>(caster)
                    .expect("checked")
                    .viewing_wanted = true;
            }
            NewsAct::SetCriminalName | NewsAct::SetCrimeData => {
                if !user.has(ACCESS_SECURITY) {
                    continue;
                }
                let (prompt, ask) = if act == NewsAct::SetCriminalName {
                    (
                        NewsPrompt::CriminalName { caster },
                        text_prompt(
                            client,
                            actor,
                            "Warrent Alert Handler",
                            "Write the Criminal's Name",
                            MAX_NAME_LEN,
                            false,
                        ),
                    )
                } else {
                    (
                        NewsPrompt::CrimeData { caster },
                        text_prompt(
                            client,
                            actor,
                            "Warrent Alert Handler",
                            "Write the Criminal's Crimes",
                            MAX_BROADCAST_LEN,
                            true,
                        ),
                    )
                };
                news_ask(world, prompt, ask);
            }
            NewsAct::SubmitWanted => {
                // The window only offers it to security (`disabled={!security_mode}`).
                if !user.has(ACCESS_SECURITY)
                    || state.criminal_name.is_empty()
                    || state.crime_description.is_empty()
                {
                    continue;
                }
                let author = user.username.clone().unwrap_or_default();
                submit_wanted(
                    world,
                    &state.criminal_name,
                    &state.crime_description,
                    &author,
                );
            }
            NewsAct::ClearWanted => {
                if !user.has(ACCESS_SECURITY) {
                    caster_say(world, caster, "Clearance not found.");
                    continue;
                }
                world.resource_mut::<NewsNetwork>().wanted.active = false;
            }
            NewsAct::PrintNewspaper => print_newspaper(world, client, actor, caster),
        }
    }
}

/// The comment button's checks (`Newscaster.jsx`), on the server.
fn can_comment(world: &World, channel: u32, message: u32, user: &Poster) -> bool {
    let net = world.resource::<NewsNetwork>();
    let Some(c) = net.channel(channel) else {
        return false;
    };
    let Some(m) = c.messages.iter().find(|m| m.id == message) else {
        return false;
    };
    !m.author_censor && !m.body_censor && !m.locked && may_post(c, user)
}

/// Asks `ask`; the answer comes back to [`answers`] with `prompt`.
fn news_ask(world: &mut World, prompt: NewsPrompt, ask: Ask) {
    let entity = ss13_tgui::prompt::ask_world(world, ask);
    world.entity_mut(entity).insert(prompt);
}

/// The prompts' answers: everything is checked again at this moment.
fn answers(world: &mut World, mut cursor: Local<MessageCursor<PromptAnswered>>) {
    let answers: Vec<(Entity, Entity, Entity, Option<PromptAnswer>)> = cursor
        .read(world.resource::<Messages<PromptAnswered>>())
        .map(|a| (a.prompt, a.client, a.actor, a.answer.clone()))
        .collect();
    for (prompt, client, actor, answer) in answers {
        let Some(asked) = world.get::<NewsPrompt>(prompt).cloned() else {
            continue;
        };
        let caster = asked.caster();
        if !can_use(world, actor, caster) {
            continue;
        }
        let user = poster(world, actor);
        let text = match &answer {
            Some(PromptAnswer::Text(t)) => t.trim().to_owned(),
            _ => String::new(),
        };
        match asked {
            NewsPrompt::Story { channel, .. } => {
                // `length(temp_message) <= 1`.
                let Some(author) = user.username.clone() else {
                    continue;
                };
                if text.chars().count() <= 1 {
                    continue;
                }
                let Some(name) = world
                    .resource::<NewsNetwork>()
                    .channel(channel)
                    .filter(|c| may_post(c, &user))
                    .map(|c| c.name.clone())
                else {
                    continue;
                };
                info!("newscaster: {author} posted on {name}: {text:?}");
                submit_article(world, &name, &text, &author, true, true);
            }
            NewsPrompt::ChannelName { caster } => {
                if text.is_empty() {
                    continue;
                }
                if world
                    .resource::<NewsNetwork>()
                    .channel_by_name(&text)
                    .is_some()
                {
                    tell(
                        world,
                        client,
                        Span::Warning,
                        "ERROR: Feed channel with that name already exists on the Network.",
                    );
                    continue;
                }
                let ask = text_prompt(
                    client,
                    actor,
                    "Network Channel Handler",
                    "Channel description",
                    MAX_BROADCAST_LEN,
                    true,
                );
                news_ask(world, NewsPrompt::ChannelDesc { caster, name: text }, ask);
            }
            NewsPrompt::ChannelDesc { caster, name } => {
                if text.is_empty() {
                    continue;
                }
                let ask = Ask::new(
                    client,
                    actor,
                    "Network Channel Handler",
                    "Who may post on the channel?",
                    PromptKind::Alert {
                        buttons: vec!["Public".into(), "Private".into(), "Cancel".into()],
                        autofocus: true,
                    },
                );
                news_ask(
                    world,
                    NewsPrompt::ChannelLock {
                        caster,
                        name,
                        desc: text,
                    },
                    ask,
                );
            }
            NewsPrompt::ChannelLock { name, desc, .. } => {
                let locked = match answer {
                    Some(PromptAnswer::Button(0)) => false,
                    Some(PromptAnswer::Button(1)) => true,
                    _ => continue,
                };
                let Some(author) = user.username.clone() else {
                    continue;
                };
                let mut net = world.resource_mut::<NewsNetwork>();
                if net.owns_channel(&author) || net.channel_by_name(&name).is_some() {
                    tell(
                        world,
                        client,
                        Span::Warning,
                        "ERROR: Feed channel with that name already exists on the Network.",
                    );
                    continue;
                }
                net.create_channel(&name, &author, &desc, locked);
                info!("newscaster: {author} created the channel {name:?}");
            }
            NewsPrompt::Comment {
                channel, message, ..
            } => {
                let Some(author) = user.username.clone() else {
                    continue;
                };
                if text.is_empty() || !can_comment(world, channel, message, &user) {
                    continue;
                }
                let time = round_timestamp(world);
                let mut net = world.resource_mut::<NewsNetwork>();
                net.last_action += 1;
                if let Some(m) = net.message_mut(channel, message) {
                    m.comments.push(FeedComment {
                        author,
                        body: text,
                        time,
                    });
                }
            }
            NewsPrompt::CriminalName { caster } | NewsPrompt::CrimeData { caster } => {
                if text.is_empty() || !user.has(ACCESS_SECURITY) {
                    continue;
                }
                let name = matches!(asked, NewsPrompt::CriminalName { .. });
                let mut c = world.get_mut::<Newscaster>(caster).expect("checked");
                if name {
                    c.criminal_name = text;
                } else {
                    c.crime_description = text;
                }
            }
        }
    }
}

/// `GLOB.news_network.submit_article(msg, author, channel_name, update_alert)`: posts it and,
/// with `alert`, every newscaster announces the channel (`news_alert(channel)`).
pub fn submit_article(
    world: &mut World,
    channel: &str,
    body: &str,
    author: &str,
    allow_comments: bool,
    alert: bool,
) -> Option<u32> {
    let time = round_timestamp(world);
    let id = world.get_resource_mut::<NewsNetwork>()?.post(
        channel,
        body,
        author,
        allow_comments,
        &time,
    )?;
    if alert {
        for caster in powered_casters(world) {
            caster_say(world, caster, &format!("Срочные новости от: {channel}!"));
            sound(world, caster, NEWS_SOUND, 75.0);
        }
    }
    Some(id)
}

/// `submit_wanted(..., newMessage = TRUE)`: the wanted issue, announced by every newscaster.
pub fn submit_wanted(world: &mut World, criminal: &str, body: &str, scanned_user: &str) {
    let Some(mut net) = world.get_resource_mut::<NewsNetwork>() else {
        return;
    };
    net.wanted = Wanted {
        active: true,
        criminal: criminal.to_owned(),
        body: body.to_owned(),
        scanned_user: scanned_user.to_owned(),
    };
    info!("newscaster: wanted issue for {criminal:?} by {scanned_user}");
    for caster in powered_casters(world) {
        caster_say(world, caster, "Внимание! Выдан ордер на арест!");
        sound(world, caster, WANTED_SOUND, 75.0);
    }
}

/// `GLOB.allCasters` that can talk (`say` of an unpowered machine is still heard in tg, but
/// its alert is meant for working screens).
fn powered_casters(world: &mut World) -> Vec<Entity> {
    let mut q = world.query_filtered::<Entity, (With<Newscaster>, Without<Unpowered>)>();
    let mut casters: Vec<Entity> = q.iter(world).collect();
    casters.sort();
    casters
}

/// `print_paper`: a newspaper of the network as it is now, into the user's hands.
fn print_newspaper(world: &mut World, client: Entity, actor: Entity, caster: Entity) {
    let Some(&pos) = world.get::<TilePos>(caster) else {
        return;
    };
    if world.get::<Newscaster>(caster).is_none_or(|c| c.paper == 0) {
        tell(world, client, Span::Warning, "Нет бумаги!");
        return;
    }
    let text = newspaper_text(world.resource::<NewsNetwork>());
    let Some(paper) = crate::construction::spawn_obj(world, NEWSPAPER, pos) else {
        return;
    };
    world.get_mut::<Newscaster>(caster).expect("checked").paper -= 1;
    let name = world
        .get::<DmName>(paper)
        .map(|n| n.name.clone())
        .unwrap_or_else(|| "newspaper".to_owned());
    world.entity_mut(paper).insert((
        Newspaper { text: text.clone() },
        // shortcut: no newspaper window: the issue is its description.
        DmName { name, desc: text },
    ));
    sound(world, caster, PAGE_TURN, 50.0);
    if let Some(inventory) = world.get::<Inventory>(actor).cloned() {
        crate::storage::put_in_hands(world, actor, &inventory, paper);
    }
}

/// The newspaper's pages: every channel that is not censored with its stories (censored ones
/// redacted), then the wanted issue (`saved_wanted_criminal`, `saved_wanted_body`).
pub fn newspaper_text(net: &NewsNetwork) -> String {
    let mut out = String::from("The Griffon");
    for channel in &net.channels {
        out.push_str(&format!(
            "\n\n{} (by {})",
            channel.name,
            channel.shown_author()
        ));
        if channel.censored {
            out.push_str(&format!("\n{CENSOR_MESSAGE}"));
            continue;
        }
        for m in &channel.messages {
            let author = if m.author_censor { REDACTED } else { &m.author };
            let body = if m.body_censor { REDACTED } else { &m.body };
            out.push_str(&format!("\n- {body}\n  [{author}, {}]", m.time));
        }
    }
    if net.wanted.active {
        out.push_str(&format!(
            "\n\nWANTED: {}\n{}",
            net.wanted.criminal, net.wanted.body
        ));
    }
    out
}

/// `SSeconomy.price_update`'s earnings report on the station announcements channel, with the
/// stock market's news (`SSstock_market.news_string`).
pub fn earnings_report(world: &mut World) {
    let money = crate::cargo::MONEY_SYMBOL;
    let inflation = world
        .get_resource::<crate::vending::VendingInflation>()
        .map_or(1.0, |i| i.0);
    let crashing = world
        .get_resource::<crate::cargo::stock_market::StockMarket>()
        .is_some_and(|m| m.crashing);
    let (total, target) = world.get_resource::<Economy>().map_or((0, 1), |e| {
        let accounts: Vec<_> = e.accounts().map(|(_, a)| a).collect();
        let total: i64 = accounts.iter().map(|a| a.balance).sum();
        let paid: i64 = accounts
            .iter()
            .map(|a| i64::from(a.paycheck) * i64::from(crate::cargo::economy::STARTING_PAYCHECKS))
            .sum();
        let target = (paid as f64 / (accounts.len() * 2).max(1) as f64).round() as i64;
        (total, target.max(1))
    });
    let fluff = if crashing {
        ", and company countermeasures are failing to protect YOU from being affected. We're all \
         doomed!"
    } else {
        ", but company countermeasures protect YOU from being affected!"
    };
    let news = world
        .get_resource::<crate::cargo::stock_market::StockMarket>()
        .map(|m| m.news.join("\n"))
        .unwrap_or_default();
    let mut report = format!(
        "Sector Economic Report\n\nSector vendor prices is currently at {}%{fluff}\n\nThe station \
         spending power is currently {total}{money}, and the crew's targeted allowance is at \
         {target}{money}.\n\n",
        (inflation * 100.0).round()
    );
    if !news.is_empty() {
        report.push_str(&news);
        report.push('\n');
    }
    report.push_str("That's all from the Nanotrasen Economist Division.");
    // `update_alert`: only with `STATION_TRAIT_ECONOMY_ALERTS` (no station traits).
    submit_article(
        world,
        STATION_ANNOUNCEMENTS,
        &report,
        "Station Earnings Report",
        true,
        false,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn poster(name: Option<&str>, access: &[&str]) -> Poster {
        Poster {
            user: NewsUser {
                name: name.unwrap_or("Unknown").to_owned(),
                ..default()
            },
            username: name.map(str::to_owned),
            access: access.iter().map(|a| (*a).to_owned()).collect(),
        }
    }

    #[test]
    fn the_network_starts_with_its_channels_and_counts_messages() {
        let mut net = NewsNetwork::default();
        assert_eq!(net.channels.len(), 2);
        assert!(net.channel_by_name(STATION_ANNOUNCEMENTS).unwrap().locked);
        let a = net.post(STATION_ANNOUNCEMENTS, "Первая", "ЦК", true, "00:01:00");
        let b = net.post(SPACE_BETTING, "Вторая", "NtOS", true, "00:02:00");
        assert_eq!((a, b), (Some(1), Some(2)));
        assert_eq!(net.post("Нет такого", "x", "y", true, ""), None);
        assert_eq!(net.last_action, 2);
        let id = net.create_channel("Сплетни", "Алиса", "Обо всём", false);
        assert_eq!(id, 3);
        assert!(net.owns_channel("Алиса") && !net.owns_channel("Боб"));
    }

    #[test]
    fn locks_censorship_and_the_window() {
        let mut net = NewsNetwork::default();
        let open = net.create_channel("Сплетни", "Алиса", "Обо всём", false);
        let locked = net.create_channel("Дневник", "Боб", "Мой", true);
        let alice = poster(Some("Алиса"), &[]);
        let bob = poster(Some("Боб"), &[ACCESS_LIBRARY]);
        assert!(may_post(net.channel(open).unwrap(), &bob));
        assert!(!may_post(net.channel(locked).unwrap(), &alice));
        assert!(may_post(net.channel(locked).unwrap(), &bob));
        net.post("Сплетни", "Секрет", "Алиса", true, "00:00:10");
        net.message_mut(open, 1).unwrap().body_censor = true;
        let caster = Newscaster {
            channel: Some(open),
            ..default()
        };
        let data = ui_data(&caster, &net, &bob);
        assert!(data.admin_mode && !data.security_mode);
        assert_eq!(data.messages[0].body, REDACTED);
        assert!(!data.can_comment(&data.messages[0]));
        assert!(data.can_submit());
        net.channel_mut(open).unwrap().censored = true;
        assert!(!may_post(net.channel(open).unwrap(), &alice));
        let data = ui_data(&caster, &net, &alice);
        assert!(data.channel_censored && !data.can_submit());
        assert!(newspaper_text(&net).contains(CENSOR_MESSAGE));
    }

    #[test]
    fn the_earnings_report_lands_on_the_station_channel() {
        let mut world = World::new();
        world.insert_resource(Time::<()>::default());
        world.init_resource::<NewsNetwork>();
        earnings_report(&mut world);
        let net = world.resource::<NewsNetwork>();
        let channel = net.channel_by_name(STATION_ANNOUNCEMENTS).unwrap();
        let report = &channel.messages[0];
        assert_eq!(report.author, "Station Earnings Report");
        assert!(
            report.body.starts_with("Sector Economic Report"),
            "{}",
            report.body
        );
        assert!(report.body.contains("currently at 100%"), "{}", report.body);
    }
}
