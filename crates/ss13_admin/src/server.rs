//! Server half: admin identity and rights, the verbs, adminhelp tickets, asay and the admin log.
//!
//! Identity: the handshake token's ckey (tg `ckey()`). **There is no real auth yet**: the server
//! accepts any non-empty token, so anyone who can connect can claim an admin's ckey. Admin
//! sessions are therefore refused unless the `TokenValidator` authenticates
//! (`ss13_net::server::TokenValidator::authenticates`, an auth service issues the tokens) or the
//! server explicitly trusts every client (`SS13_INSECURE_ADMINS=1`, [`AdminConfig`]).
//!
//! Admins come from `admin_ranks.txt` and `admins.txt` in the config directory
//! (`SS13_CONFIG_DIR`, default `config`), read at startup ([`Admins`]; tests insert it). A
//! listed admin's connection gets an [`AdminSession`]; ranks without `AUTOADMIN` start deadminned
//! (tg) until `Readmin`. Every request is checked against the asking client's active rights when
//! it is applied, never against what the client claims.
//!
//! Every admin action goes to the admin log (`SS13_ADMIN_LOG`, default `logs/admin.log`; empty:
//! off) and, as tg `message_admins`, to the online admins with `R_ADMIN`.
// shortcut: bans and mutes (by ckey) live in the server `App`: lost on the round reboot (a new `App`) and on
// restart; persist them once a database exists (tg keeps bans in SQL).

use std::collections::{HashMap, HashSet};
use std::io::Write as _;
use std::path::PathBuf;

use bevy::ecs::message::MessageCursor;
use bevy::prelude::*;
use ss13_chat::server::Muted;
use ss13_game::antag::brother::{Brother, add_brother};
use ss13_game::antag::changeling::server::{Changeling, add_changeling};
use ss13_game::antag::traitor::{Traitor, add_traitor};
use ss13_game::antag::vampire::server::{Vampire, add_vampire};
use ss13_game::comms_server::CommsRequest;
use ss13_game::ghost::{Ghost, ghostize_client, return_to_body};
use ss13_game::health::{DamageType, Health, Stat, apply_damage};
use ss13_game::jobs::{Jobs, PlayerJob};
use ss13_game::round::{NewPlayer, REBOOT_EXIT_CODE, Ticker};
use ss13_game::server::Players;
use ss13_mapload::Areas;
use ss13_net::admin::server::{AdminRequestReceived, SendAdminResponse};
use ss13_net::chat::server::SendChat;
use ss13_net::server::{
    ClientConnected, ClientDisconnected, ClientToken, KickClient, TokenValidator,
};
use ss13_proto::Prototypes;
use ss13_shared::TilePos;
use ss13_shared::admin::rights::*;
use ss13_shared::admin::{
    AdminRequest, AdminResponse, AntagRole, MindAct, PlayerEntry, PlayerState, Smite,
};
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, MAX_MESSAGE_LEN, Span};

use crate::rights::{
    Holder, Rank, ckey, greater_rights, has, parse_admins, parse_ranks, rights_text,
};
use crate::tickets::{self, Tickets};

/// The admins' request notification (`comms_notification`).
const SERVER_READY: &str = "sound/misc/server-ready.ogg";

/// Requests per second per client, and the burst. The panel polls the player list once a
/// second; a burst of clicks on its buttons stays under it.
pub const RATE_PER_SEC: f32 = 10.0;
pub const BURST: f32 = 20.0;
/// Prototype search results.
pub const MAX_PROTOTYPE_MATCHES: usize = 50;
/// tg `LIGHTNING_BOLT_DAMAGE`.
pub const LIGHTNING_BOLT_DAMAGE: f32 = 75.0;
const TIME_SCALE_RANGE: (f32, f32) = (0.1, 10.0);
/// Announcement title, kick and ban reasons, in characters.
const MAX_SHORT_TEXT: usize = 200;

/// Where admins and the log come from. Insert before the plugin to override the environment.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct AdminConfig {
    /// `admin_ranks.txt` and `admins.txt`; `None`: no admins from files.
    pub config_dir: Option<PathBuf>,
    /// Appended to; `None`: no log file (the lines still go to the server log).
    pub log_file: Option<PathBuf>,
    /// Grant listed admins their sessions although the token validator does not authenticate
    /// (anyone can claim any ckey): only for servers where every client is trusted.
    pub insecure_admins: bool,
}

impl AdminConfig {
    /// `SS13_CONFIG_DIR` (default `config`), `SS13_ADMIN_LOG` (default `logs/admin.log`, empty:
    /// off), `SS13_INSECURE_ADMINS` (`1`: on).
    pub fn from_env() -> Self {
        let path = |key: &str, default: &str| match std::env::var(key) {
            Ok(v) if v.is_empty() => None,
            Ok(v) => Some(PathBuf::from(v)),
            Err(_) => Some(PathBuf::from(default)),
        };
        Self {
            config_dir: path("SS13_CONFIG_DIR", "config"),
            log_file: path("SS13_ADMIN_LOG", "logs/admin.log"),
            insecure_admins: std::env::var("SS13_INSECURE_ADMINS").is_ok_and(|v| v == "1"),
        }
    }
}

/// tg `GLOB.admin_datums`: listed admins by ckey. Read from [`AdminConfig::config_dir`] at
/// startup unless already inserted.
#[derive(Resource, Clone, Debug, Default)]
pub struct Admins {
    pub holders: HashMap<String, Holder>,
    /// `admin_ranks.txt`, for the permissions panel.
    pub ranks: Vec<Rank>,
}

impl Admins {
    /// Lists `key` (any spelling) as an admin of `rank` with `rights`.
    pub fn grant(&mut self, key: &str, rank: &str, rights: u32) {
        self.holders.insert(
            ckey(key),
            Holder {
                rank: rank.to_owned(),
                rights,
                edit: 0,
            },
        );
    }

    /// Reads `admin_ranks.txt` and `admins.txt` from `dir`; a missing file means none.
    pub fn load(dir: &std::path::Path) -> Self {
        let read = |name: &str| ss13_data::read_to_string(&dir.join(name)).unwrap_or_default();
        let (ranks, mut warnings) = parse_ranks(&read("admin_ranks.txt"));
        let (holders, more) = parse_admins(&read("admins.txt"), &ranks);
        warnings.extend(more);
        for w in warnings {
            warn!("admin config: {w}");
        }
        info!(
            "admin config {}: {} ranks, {} admins",
            dir.display(),
            ranks.len(),
            holders.len()
        );
        Self { holders, ranks }
    }
}

/// On the connection entity of a listed admin (tg `client.holder`).
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct AdminSession {
    pub ckey: String,
    pub rank: String,
    /// The rank's rights.
    pub rights: u32,
    /// tg: not deadminned.
    pub active: bool,
}

impl AdminSession {
    /// What the server checks: nothing while deadminned.
    pub fn active_rights(&self) -> u32 {
        if self.active { self.rights } else { 0 }
    }
}

/// Banned ckeys and the reasons (in memory, see the module docs).
#[derive(Resource, Clone, Debug, Default)]
pub struct Bans(pub HashMap<String, String>);

/// Muted ckeys (in memory, see the module docs): a reconnecting client gets its [`Muted`] back.
#[derive(Resource, Clone, Debug, Default)]
pub struct Mutes(pub HashSet<String>);

/// On an admin ghost: keeps it on the followed mob's tile (tg orbit).
#[derive(Component, Clone, Copy, Debug)]
pub struct Following(pub Entity);

#[derive(Resource, Default)]
struct AdminLogFile(Option<std::io::BufWriter<std::fs::File>>);

#[derive(Resource, Default)]
struct Buckets(HashMap<Entity, (f32, f64)>);

pub struct AdminServerPlugin;

impl Plugin for AdminServerPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<AdminConfig>() {
            app.insert_resource(AdminConfig::from_env());
        }
        let config = app.world().resource::<AdminConfig>().clone();
        if !app.world().contains_resource::<Admins>() {
            let admins = config
                .config_dir
                .as_deref()
                .map(Admins::load)
                .unwrap_or_default();
            app.insert_resource(admins);
        }
        let file = config.log_file.as_ref().and_then(|path| {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| warn!("admin log {}: {e}", path.display()))
                .ok()
                .map(std::io::BufWriter::new)
        });
        crate::logout_report::build(app);
        app.insert_resource(AdminLogFile(file))
            .init_resource::<Bans>()
            .init_resource::<Mutes>()
            .init_resource::<Tickets>()
            .init_resource::<Buckets>()
            .add_systems(PostStartup, warn_unauthenticated)
            .add_systems(
                Update,
                (
                    sessions,
                    handle_requests.run_if(on_message::<AdminRequestReceived>),
                    tickets::autoclose,
                    tickets::push.run_if(|t: Res<Tickets>| t.dirty),
                    follow,
                    comms_requests.run_if(on_message::<CommsRequest>),
                )
                    .chain(),
            );
    }
}

/// Seconds since the server started (ticket times).
pub(crate) fn now_secs(world: &World) -> f64 {
    world.resource::<Time<Real>>().elapsed_secs_f64()
}

/// A client's ckey (empty before the handshake set its token).
pub fn client_ckey(world: &World, client: Entity) -> String {
    world
        .get::<ClientToken>(client)
        .map(|t| ckey(&t.0))
        .unwrap_or_default()
}

/// The connected client with `ckey`.
pub fn client_by_ckey(world: &mut World, wanted: &str) -> Option<Entity> {
    let wanted = ckey(wanted);
    let mut q = world.query::<(Entity, &ClientToken)>();
    q.iter(world)
        .find(|(_, t)| ckey(&t.0) == wanted)
        .map(|(e, _)| e)
}

/// The client's active admin rights (0 for players and deadminned admins).
pub fn rights_of(world: &World, client: Entity) -> u32 {
    world
        .get::<AdminSession>(client)
        .map_or(0, AdminSession::active_rights)
}

/// tg `key_name`: `ckey/(Mob Name)`, or the ckey alone without a mob.
pub fn key_name(world: &World, client: Entity) -> String {
    let key = client_ckey(world, client);
    let name = world
        .resource::<Players>()
        .mob(client)
        .and_then(|mob| world.get::<Name>(mob));
    match name {
        Some(name) => format!("{key}/({name})"),
        None => key,
    }
}

/// Connection entities of the online admins whose active rights have all of `required`.
pub fn admins_with(world: &mut World, required: u32) -> Vec<Entity> {
    let mut q = world.query::<(Entity, &AdminSession)>();
    q.iter(world)
        .filter(|(_, s)| s.active_rights() != 0 && has(s.active_rights(), required))
        .map(|(e, _)| e)
        .collect()
}

pub(crate) fn chat(world: &mut World, clients: Vec<Entity>, message: ChatMessage) {
    if !clients.is_empty() {
        world.write_message(SendChat { clients, message });
    }
}

pub(crate) fn tell(
    world: &mut World,
    client: Entity,
    channel: ChatChannel,
    span: Span,
    text: &str,
) {
    chat(
        world,
        vec![client],
        ChatMessage::simple(channel, span, text),
    );
}

/// tg `log_admin` (and `log_admin_private`, `log_asay`: one file, tagged): the admin log file
/// and the server log.
pub fn log_admin(world: &mut World, tag: &str, text: &str) {
    info!("{tag}: {text}");
    let mut file = world.resource_mut::<AdminLogFile>();
    if let Some(out) = file.0.as_mut() {
        let line = format!("[{}] {tag}: {text}\n", timestamp());
        if out
            .write_all(line.as_bytes())
            .and_then(|()| out.flush())
            .is_err()
        {
            warn!("admin log write failed; logging to the server log only");
            file.0 = None;
        }
    }
}

/// tg `message_admins`: `ADMIN LOG:` to the online admins with `R_ADMIN`.
pub fn message_admins(world: &mut World, text: &str) {
    let to = admins_with(world, R_ADMIN);
    chat(
        world,
        to,
        ChatMessage::simple(
            ChatChannel::AdminLog,
            Span::Admin,
            format!("ADMIN LOG: {text}"),
        ),
    );
}

/// tg `message_centcom`, `message_syndicate`, `nuke_request` and BandaStation's `ert_request`:
/// the request to the admins with `R_ADMIN`, with `server-ready.ogg`.
// shortcut: no reply links or request manager panel; admins answer with the command report.
fn comms_requests(world: &mut World, mut cursor: Local<MessageCursor<CommsRequest>>) {
    let requests: Vec<CommsRequest> = cursor
        .read(world.resource::<Messages<CommsRequest>>())
        .cloned()
        .collect();
    for request in requests {
        let key = client_ckey(world, request.client);
        let text = format!(
            "{}: {} ({key}): {}",
            request.kind.label(),
            request.sender,
            request.text
        );
        log_admin(world, "REQUEST", &text);
        let to = admins_with(world, R_ADMIN);
        for &admin in &to {
            world.write_message(ss13_net::sound::server::SendSound {
                client: admin,
                sound: ss13_shared::protocol::SoundEvent {
                    file: SERVER_READY.to_owned(),
                    volume: 100.0,
                    speed: 1.0,
                    offset: None,
                },
            });
        }
        chat(
            world,
            to,
            ChatMessage::simple(ChatChannel::AdminLog, Span::AdminNotice, text),
        );
    }
}

/// An admin action: logged and announced to the admins.
pub(crate) fn admin_action(world: &mut World, text: &str) {
    log_admin(world, "ADMIN", text);
    message_admins(world, text);
}

/// UTC `YYYY-MM-DD hh:mm:ss`.
fn timestamp() -> String {
    let secs = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Howard Hinnant's civil_from_days.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}",
        rem / 3600,
        rem / 60 % 60,
        rem % 60
    )
}

/// Bidirectional formatting controls reorder the text around them; control characters break
/// lines. Both go (tabs and line breaks become spaces), the rest is trimmed and cut at `max`
/// characters (tg `sanitize(copytext_char(msg, 1, max))`).
pub fn sanitize_text(raw: &str, max: usize) -> String {
    let text: String = raw
        .chars()
        .filter_map(|c| match c {
            '\t' | '\n' | '\r' => Some(' '),
            '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' => None,
            c if c.is_control() => None,
            c => Some(c),
        })
        .take(max)
        .collect();
    text.trim().to_owned()
}

/// Whether a listed ckey may be trusted with its rights: the token proves it, or the server
/// trusts every client.
pub(crate) fn identity_trusted(world: &World) -> bool {
    world.resource::<AdminConfig>().insecure_admins
        || world
            .get_resource::<TokenValidator>()
            .is_some_and(|v| v.authenticates)
}

fn warn_unauthenticated(world: &mut World) {
    if world.resource::<Admins>().holders.is_empty() {
        return;
    }
    if !identity_trusted(world) {
        warn!(
            "ADMINS ARE LISTED BUT TOKENS ARE NOT AUTHENTICATED: anyone could claim an admin's \
             ckey, so no admin sessions will be granted. Plug in an authenticating \
             TokenValidator, or set SS13_INSECURE_ADMINS=1 if every client is trusted."
        );
    } else if world.resource::<AdminConfig>().insecure_admins {
        warn!(
            "SS13_INSECURE_ADMINS=1: admin rights follow the claimed ckey without \
             authentication; anyone who can connect can become an admin"
        );
    }
}

/// Connects and disconnects: bans, mutes, admin sessions, tg's admin login/logout notes.
fn sessions(world: &mut World, mut connected: Local<MessageCursor<ClientConnected>>) {
    let joined: Vec<Entity> = connected
        .read(world.resource::<Messages<ClientConnected>>())
        .map(|c| c.client)
        .collect();
    let left: Vec<(Entity, String, bool)> = {
        let mut q = world.query::<&AdminSession>();
        world
            .resource::<Messages<ClientDisconnected>>()
            .iter_current_update_messages()
            .map(|d| d.client)
            .collect::<Vec<_>>()
            .into_iter()
            .map(|c| {
                let session = q.get(world, c).ok();
                (
                    c,
                    session.map(|s| s.ckey.clone()).unwrap_or_default(),
                    session.is_some(),
                )
            })
            .collect()
    };
    for (client, key, admin) in left {
        world.resource_mut::<Buckets>().0.remove(&client);
        if admin {
            message_admins(world, &format!("Admin logout: {key}"));
        }
    }
    for client in joined {
        let key = client_ckey(world, client);
        if let Some(reason) = world.resource::<Bans>().0.get(&key).cloned() {
            log_admin(world, "ACCESS", &format!("banned {key} refused"));
            world.write_message(KickClient {
                client,
                reason: format!("Вы забанены на этом сервере. Причина: {reason}"),
            });
            continue;
        }
        if world.resource::<Mutes>().0.contains(&key) {
            world.entity_mut(client).insert(Muted);
        }
        let Some(holder) = world.resource::<Admins>().holders.get(&key).cloned() else {
            world.resource_mut::<Tickets>().dirty = true;
            continue;
        };
        if !identity_trusted(world) {
            warn!("admin session for {key} refused: the token is not authenticated");
            log_admin(
                world,
                "ACCESS",
                &format!("admin login refused: {key} (unauthenticated token)"),
            );
            world.resource_mut::<Tickets>().dirty = true;
            continue;
        }
        let session = AdminSession {
            ckey: key.clone(),
            rank: holder.rank.clone(),
            rights: holder.rights,
            active: has(holder.rights, R_AUTOADMIN),
        };
        let rights = session.active_rights();
        world.entity_mut(client).insert(session);
        world.write_message(SendAdminResponse {
            client,
            response: AdminResponse::Rights {
                rank: holder.rank.clone(),
                rights,
            },
        });
        log_admin(
            world,
            "ACCESS",
            &format!(
                "admin login: {key} ({}, {})",
                holder.rank,
                rights_text(holder.rights)
            ),
        );
        if rights != 0 {
            message_admins(world, &format!("Admin login: {key}"));
        }
        world.resource_mut::<Tickets>().dirty = true;
    }
}

fn take_token(buckets: &mut Buckets, client: Entity, now: f64) -> bool {
    let (tokens, last) = buckets.0.entry(client).or_insert((BURST, now));
    *tokens = (*tokens + (now - *last) as f32 * RATE_PER_SEC).min(BURST);
    *last = now;
    if *tokens < 1.0 {
        return false;
    }
    *tokens -= 1.0;
    true
}

/// tg `R_*` a verb needs; `None` for the ones every player has (adminhelp) and the ones checked
/// against the listing itself (readmin, deadmin).
pub fn required_rights(request: &AdminRequest) -> Option<u32> {
    use AdminRequest::*;
    Some(match request {
        Adminhelp(_) | Readmin | Deadmin => return None,
        Asay(_)
        | Players
        | TicketReply { .. }
        | SetTicketState { .. }
        | Pm { .. }
        | JumpTo(_)
        | Get(_)
        | Follow(_)
        | Kick { .. }
        | Mute { .. }
        | Aghost
        | Rejuvenate(_)
        | Announce { .. }
        | MakeAntag { .. }
        | MindPanel(_)
        | MindAct { .. }
        | Dynamic
        | ForceMidround(_)
        | SetSecurityLevel(_)
        | ManUp(_)
        | GlobalManUp => R_ADMIN,
        AdminList | SetAdminRank { .. } | RemoveAdmin(_) => R_PERMISSIONS,
        PlaySound { .. } => R_SOUND,
        AirStatus | SpawnAir(_) => R_DEBUG,
        Ban { .. } | Unban(_) => R_BAN,
        FindPrototypes(_) | Spawn(_) => R_SPAWN,
        Smite { .. } | Emp { .. } => R_ADMIN | R_FUN,
        Events | ForceEvent { .. } | DropBomb(_) => R_FUN,
        StartNow | EndRound | Restart => R_SERVER,
        TimeScale(_) => R_DEBUG,
    })
}

/// Exclusive: verbs move mobs, spawn objects, open windows. Runs only on ticks with requests.
fn handle_requests(world: &mut World) {
    let requests: Vec<AdminRequestReceived> = world
        .resource_mut::<Messages<AdminRequestReceived>>()
        .drain()
        .collect();
    let now = now_secs(world);
    for AdminRequestReceived { client, request } in requests {
        if world.get_entity(client).is_err() {
            continue;
        }
        if !take_token(&mut world.resource_mut::<Buckets>(), client, now) {
            debug!("admin request from {client} dropped: over the rate limit");
            continue;
        }
        let response = match answer(world, client, request) {
            Ok(Some(response)) => response,
            Ok(None) => continue,
            Err(why) => AdminResponse::Refused(why),
        };
        world.write_message(SendAdminResponse { client, response });
    }
}

pub(crate) type AnswerResult = Result<Option<AdminResponse>, String>;

pub(crate) fn done(text: impl Into<String>) -> AnswerResult {
    Ok(Some(AdminResponse::Done(text.into())))
}

fn answer(world: &mut World, client: Entity, request: AdminRequest) -> AnswerResult {
    let rights = rights_of(world, client);
    if let Some(required) = required_rights(&request)
        && !has(rights, required)
    {
        let key = key_name(world, client);
        if world.get::<AdminSession>(client).is_none() {
            // A modified client: tg `alert_to_permissions_elevation_attempt`.
            warn!("admin request {request:?} from non-admin {key}");
            return Err("Вы не администратор.".into());
        }
        return Err(format!(
            "Недостаточно прав: нужно +{}.",
            rights_text(required).replace(' ', " +")
        ));
    }
    let key = key_name(world, client);
    match request {
        AdminRequest::Adminhelp(text) => tickets::adminhelp(world, client, &text),
        AdminRequest::Readmin | AdminRequest::Deadmin => {
            let active = matches!(request, AdminRequest::Readmin);
            let Some(mut session) = world.get_mut::<AdminSession>(client) else {
                return Err("Вы не администратор.".into());
            };
            if session.active == active {
                return Err(if active {
                    "Вы уже администратор.".into()
                } else {
                    "Вы уже деадминились.".into()
                });
            }
            session.active = active;
            let (rank, rights) = (session.rank.clone(), session.active_rights());
            world.write_message(SendAdminResponse {
                client,
                response: AdminResponse::Rights { rank, rights },
            });
            let verb = if active { "re-adminned" } else { "deadminned" };
            admin_action(world, &format!("{key} {verb} themselves."));
            world.resource_mut::<Tickets>().dirty = true;
            done(verb)
        }
        AdminRequest::Asay(text) => {
            let text = sanitize_text(&text, MAX_MESSAGE_LEN);
            if text.is_empty() {
                return Ok(None);
            }
            log_admin(world, "ASAY", &format!("{key}: {text}"));
            let to = admins_with(world, R_ADMIN);
            // tg `emoji_parse` of the asay text.
            let mut segments = vec![ChatSegment::new(Span::AdminSay, format!("ADMIN: {key}: "))];
            segments.extend(ss13_chat::emoji::emoji_parse(
                &text,
                Span::AdminSay,
                world.get_resource::<ss13_chat::emoji::Emojis>(),
            ));
            chat(
                world,
                to,
                ChatMessage {
                    channel: ChatChannel::AdminChat,
                    segments,
                },
            );
            Ok(None)
        }
        AdminRequest::Players => Ok(Some(AdminResponse::Players(players(world)))),
        AdminRequest::TicketReply { ticket, text } => {
            tickets::admin_reply(world, client, ticket, &text)
        }
        AdminRequest::SetTicketState { ticket, state } => {
            tickets::set_state(world, Some(client), ticket, state)
        }
        AdminRequest::Pm { ckey: target, text } => tickets::admin_pm(world, client, &target, &text),
        AdminRequest::JumpTo(target) => {
            let (_, target_mob) = target_mob(world, &target)?;
            let pos = mob_pos(world, target_mob)?;
            let own = own_mob(world, client)?;
            world.entity_mut(own).remove::<Following>().insert(pos);
            let target_key = key_name_of(world, &target);
            admin_action(
                world,
                &format!("{key} jumped to {target_key} at {}", coords(pos)),
            );
            done(format!("jumped to {target_key}"))
        }
        AdminRequest::Get(target) => {
            let (_, target_mob) = target_mob(world, &target)?;
            let own = own_mob(world, client)?;
            let pos = mob_pos(world, own)?;
            world
                .entity_mut(target_mob)
                .remove::<Following>()
                .insert(pos);
            let target_key = key_name_of(world, &target);
            admin_action(
                world,
                &format!("{key} teleported {target_key} to {}", coords(pos)),
            );
            done(format!("got {target_key}"))
        }
        AdminRequest::Follow(target) => {
            let (_, target_mob) = target_mob(world, &target)?;
            let own = own_mob(world, client)?;
            let ghost = if world.get::<Ghost>(own).is_some() {
                own
            } else {
                let ghost =
                    ghostize_client(world, client).ok_or("Нельзя стать админ-призраком отсюда.")?;
                admin_action(world, &format!("{key} admin ghosted."));
                ghost
            };
            if let Ok(pos) = mob_pos(world, target_mob) {
                world.entity_mut(ghost).insert(pos);
            }
            world.entity_mut(ghost).insert(Following(target_mob));
            let target_key = key_name_of(world, &target);
            log_admin(world, "ADMIN", &format!("{key} is following {target_key}"));
            done(format!("following {target_key}"))
        }
        AdminRequest::Kick {
            ckey: target,
            reason,
        } => {
            let target_client = target_client(world, &target)?;
            check_greater(world, client, &client_ckey(world, target_client))?;
            let reason = sanitize_text(&reason, MAX_SHORT_TEXT);
            let target_key = key_name(world, target_client);
            let own_key = client_ckey(world, client);
            let mut text = format!("Вы были кикнуты с сервера администратором {own_key}.");
            if !reason.is_empty() {
                text.push_str(&format!(" Причина: {reason}"));
            }
            tell(
                world,
                target_client,
                ChatChannel::System,
                Span::Danger,
                &text,
            );
            world.write_message(KickClient {
                client: target_client,
                reason: text,
            });
            admin_action(world, &format!("{key} kicked {target_key}. {reason}"));
            done(format!("kicked {target_key}"))
        }
        AdminRequest::Mute { ckey: target, mute } => {
            let target_client = target_client(world, &target)?;
            let target_ckey = client_ckey(world, target_client);
            check_greater(world, client, &target_ckey)?;
            let target_key = key_name(world, target_client);
            if mute {
                world.resource_mut::<Mutes>().0.insert(target_ckey);
                world.entity_mut(target_client).insert(Muted);
            } else {
                world.resource_mut::<Mutes>().0.remove(&target_ckey);
                world.entity_mut(target_client).remove::<Muted>();
            }
            let verb = if mute { "muted" } else { "unmuted" };
            let own_key = client_ckey(world, client);
            let text = if mute {
                format!("Вы были заглушены (всё) администратором {own_key}.")
            } else {
                format!("С вас снято заглушение администратором {own_key}.")
            };
            tell(
                world,
                target_client,
                ChatChannel::System,
                Span::Plain,
                &text,
            );
            admin_action(
                world,
                &format!("{key} has {verb} {target_key} from everything."),
            );
            done(format!("{verb} {target_key}"))
        }
        AdminRequest::Ban {
            ckey: target,
            reason,
        } => {
            let target_ckey = ckey(&target);
            if target_ckey.is_empty() {
                return Err("Пустой ckey.".into());
            }
            // Online or not: a listed admin's rank counts (tg checks the target's holder).
            check_greater(world, client, &target_ckey)?;
            let reason = sanitize_text(&reason, MAX_SHORT_TEXT);
            world
                .resource_mut::<Bans>()
                .0
                .insert(target_ckey.clone(), reason.clone());
            if let Some(target_client) = client_by_ckey(world, &target_ckey) {
                let text = format!(
                    "Вы забанены администратором {}. Причина: {reason}",
                    { client_ckey(world, client) }
                );
                tell(
                    world,
                    target_client,
                    ChatChannel::System,
                    Span::BoldWarning,
                    &text,
                );
                world.write_message(KickClient {
                    client: target_client,
                    reason: text,
                });
            }
            admin_action(
                world,
                &format!("{key} banned {target_ckey} for the session. Reason: {reason}"),
            );
            done(format!("banned {target_ckey}"))
        }
        AdminRequest::Unban(target) => {
            let target_ckey = ckey(&target);
            if world
                .resource_mut::<Bans>()
                .0
                .remove(&target_ckey)
                .is_none()
            {
                return Err(format!("{target_ckey} не забанен."));
            }
            admin_action(world, &format!("{key} unbanned {target_ckey}."));
            done(format!("unbanned {target_ckey}"))
        }
        AdminRequest::Aghost => aghost(world, client, &key),
        AdminRequest::FindPrototypes(query) => {
            let Some(prototypes) = world.get_resource::<Prototypes>() else {
                return Err("Прототипы не загружены.".into());
            };
            let needle = query.to_lowercase();
            let mut ids: Vec<String> = prototypes
                .ids()
                .filter(|id| id.starts_with("/obj") && id.to_lowercase().contains(&needle))
                .map(str::to_owned)
                .collect();
            ids.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
            ids.truncate(MAX_PROTOTYPE_MATCHES);
            Ok(Some(AdminResponse::Prototypes { query, ids }))
        }
        AdminRequest::Spawn(path) => {
            let known = world
                .get_resource::<Prototypes>()
                .is_some_and(|p| p.get(&path).is_some());
            if !known || !path.starts_with("/obj") {
                return Err(format!("{path}: нет такого прототипа /obj."));
            }
            let own = own_mob(world, client)?;
            let pos = mob_pos(world, own)?;
            if !world
                .get_resource::<Areas>()
                .is_some_and(|a| a.area_at(pos).is_some())
            {
                return Err("Вне карты.".into());
            }
            ss13_game::antag::heretic::server::spawn_obj_or_rune(world, &path, pos)
                .ok_or_else(|| format!("{path} не создаётся."))?;
            admin_action(world, &format!("{key} spawned {path} at {}", coords(pos)));
            done(format!("spawned {path}"))
        }
        AdminRequest::Rejuvenate(target) => {
            let (target_client, mob) = target_mob(world, &target)?;
            // A DNR ghost's mind still knows its body (tg `revive(force_grab_ghost = TRUE)`).
            let dnr = world.get::<ss13_game::mind::DnrBody>(mob).map(|d| d.0);
            let body = world
                .get::<Ghost>(mob)
                .map_or(Some(mob), |g| g.body.or(dnr))
                .ok_or("Призрака нельзя вылечить.")?;
            let Some(health) = world.get::<Health>(body).cloned() else {
                return Err("Это не живое существо.".into());
            };
            // shortcut: tg `revive(ADMIN_HEAL_ALL)` also clears status effects; here damage,
            // blood, wounds, organs, limbs, stat and the breath alerts.
            world.entity_mut(body).insert(health.healed());
            ss13_game::medical::heal_all(world, body);
            if world.get::<Stat>(body) == Some(&Stat::Dead) {
                ss13_game::medical::defib::revive(world, body);
            } else {
                world.entity_mut(body).insert(Stat::Conscious);
            }
            if body != mob {
                ss13_game::mind::grab_ghost(world, body, true);
            }
            let target_key = key_name(world, target_client);
            admin_action(
                world,
                &format!("Admin {key} healed / revived {target_key}!"),
            );
            done(format!("healed {target_key}"))
        }
        AdminRequest::Smite {
            ckey: target,
            smite,
        } => {
            let (target_client, mob) = target_mob(world, &target)?;
            check_greater(world, client, &client_ckey(world, target_client))?;
            let (Some(mut health), Some(&stat)) =
                (world.get::<Health>(mob).cloned(), world.get::<Stat>(mob))
            else {
                return Err("Это не живое существо.".into());
            };
            // shortcut: Lightning Bolt only, without the beam, sound and electrocution animation.
            let Smite::Lightning = smite;
            let mut new = stat;
            apply_damage(
                &mut health,
                &mut new,
                DamageType::Burn,
                LIGHTNING_BOLT_DAMAGE,
            );
            world.entity_mut(mob).insert(health);
            if new != stat {
                world.entity_mut(mob).insert(new);
            }
            tell(
                world,
                target_client,
                ChatChannel::Warning,
                Span::BoldWarning,
                "Боги покарали вас за ваши грехи!",
            );
            let target_key = key_name(world, target_client);
            admin_action(
                world,
                &format!("{key} punished {target_key} with Lightning Bolt."),
            );
            done(format!("smote {target_key}"))
        }
        AdminRequest::Events => {
            let names = world
                .get_resource::<ss13_game::events::Events>()
                .map(|e| e.controls.iter().map(|c| c.name.to_owned()).collect())
                .unwrap_or_default();
            Ok(Some(AdminResponse::Events(names)))
        }
        AdminRequest::ForceEvent { name, announce } => {
            ss13_game::events::force_event(world, &name, announce)?;
            admin_action(world, &format!("{key} has triggered an event. ({name})"));
            done(format!("triggered {name}"))
        }
        AdminRequest::StartNow => {
            if world.get_resource::<Ticker>().is_none() {
                return Err("Нет раунда.".into());
            }
            if ss13_game::round::start_now(world)? {
                admin_action(world, &format!("{key} has started the game."));
                done("round starting")
            } else {
                admin_action(
                    world,
                    &format!(
                        "{key} has cancelled immediate game start. Game will start in 3 minutes."
                    ),
                );
                done("start cancelled: 3 minutes")
            }
        }
        AdminRequest::EndRound => {
            let Some(mut ticker) = world.get_resource_mut::<Ticker>() else {
                return Err("Нет раунда.".into());
            };
            ticker.force_ending = true;
            admin_action(world, &format!("{key} has ended the round."));
            done("round ending")
        }
        AdminRequest::Restart => {
            admin_action(world, &format!("{key} initiated a reboot."));
            world.write_message(AppExit::from_code(REBOOT_EXIT_CODE));
            done("rebooting")
        }
        // shortcut: tg has no time scale verb (it is the debug overlay's dev tool); behind
        // `R_DEBUG` until the server has tg's `world.tick_lag` setting.
        AdminRequest::TimeScale(scale) => {
            if !scale.is_finite() || !(TIME_SCALE_RANGE.0..=TIME_SCALE_RANGE.1).contains(&scale) {
                return Err(format!("Скорость должна быть в {TIME_SCALE_RANGE:?}."));
            }
            world
                .resource_mut::<Time<Virtual>>()
                .set_relative_speed(scale);
            admin_action(world, &format!("{key} set the time scale to {scale}."));
            done(format!("time scale {scale}"))
        }
        AdminRequest::Announce { title, text } => {
            let title = sanitize_text(&title, MAX_SHORT_TEXT);
            let text = sanitize_text(&text, MAX_MESSAGE_LEN);
            if text.is_empty() {
                return Err("Пустое объявление.".into());
            }
            ss13_game::events::priority_announce(world, &title, &text);
            admin_action(
                world,
                &format!("{key} has created a command report: {text}"),
            );
            done("announced")
        }
        AdminRequest::MakeAntag { ckey: target, role } => {
            let (target_client, mob) = target_mob(world, &target)?;
            if world.get::<Ghost>(mob).is_some() || world.get::<Health>(mob).is_none() {
                return Err("Антагонистом можно сделать только живое тело.".into());
            }
            let target_key = key_name(world, target_client);
            let (made, what) = match role {
                AntagRole::Traitor => (add_traitor(world, mob), "a traitor"),
                AntagRole::Brother => (add_brother(world, mob, None), "a blood brother"),
                AntagRole::Changeling => (add_changeling(world, mob), "a changeling"),
                AntagRole::Vampire => (add_vampire(world, mob), "a vampire"),
                AntagRole::Cultist => (
                    ss13_game::antag::cult::server::add_cultist(world, mob),
                    "a cultist",
                ),
                AntagRole::Heretic => (
                    ss13_game::antag::heretic::server::add_heretic(world, mob),
                    "a heretic",
                ),
                AntagRole::HeadRev => (
                    ss13_game::antag::revolution::server::add_revolutionary(world, mob, true),
                    "a head revolutionary",
                ),
                AntagRole::Revolutionary => (
                    ss13_game::antag::revolution::server::add_revolutionary(world, mob, false),
                    "a revolutionary",
                ),
            };
            if !made {
                return Err(format!("{target_key} уже антагонист этой роли."));
            }
            admin_action(world, &format!("{key} has made {target_key} {what}."));
            done(format!("{target_key} is {what}"))
        }
        AdminRequest::MindPanel(target) => mind_panel(world, &target),
        AdminRequest::MindAct { ckey: target, act } => {
            let (target_client, mob) = target_mob(world, &target)?;
            let body = mind_mob(world, mob);
            let target_key = key_name(world, target_client);
            match act {
                MindAct::RemoveAntag(role) => {
                    if !ss13_game::mind::remove_antag(world, body, role) {
                        return Err(format!("{target_key} не {role:?}."));
                    }
                    admin_action(
                        world,
                        &format!("{key} has removed {role:?} from {target_key}."),
                    );
                }
                MindAct::SetJob(title) => {
                    let title = sanitize_text(&title, MAX_SHORT_TEXT);
                    if !ss13_game::mind::set_job(world, body, &title) {
                        return Err(format!("Нет должности «{title}»."));
                    }
                    admin_action(
                        world,
                        &format!("{key} set the job of {target_key} to {title}."),
                    );
                }
                MindAct::AddObjective(text) => {
                    let text = sanitize_text(&text, MAX_MESSAGE_LEN);
                    if text.is_empty() {
                        return Err("Пустая цель.".into());
                    }
                    if !ss13_game::mind::add_objective(world, body, text.clone()) {
                        return Err(format!("{target_key} не предатель и не брат."));
                    }
                    admin_action(
                        world,
                        &format!("{key} gave {target_key} an objective: {text}"),
                    );
                }
                MindAct::EditObjective { index, text } => {
                    let text = sanitize_text(&text, MAX_MESSAGE_LEN);
                    if text.is_empty() {
                        return Err("Пустая цель.".into());
                    }
                    if !ss13_game::mind::edit_objective(world, body, index as usize, text.clone()) {
                        return Err("Нет такой цели.".into());
                    }
                    admin_action(
                        world,
                        &format!(
                            "{key} edited objective #{} of {target_key}: {text}",
                            index + 1
                        ),
                    );
                }
                MindAct::RemoveObjective(index) => {
                    if !ss13_game::mind::remove_objective(world, body, index as usize) {
                        return Err("Нет такой цели.".into());
                    }
                    admin_action(
                        world,
                        &format!("{key} removed an objective #{} of {target_key}.", index + 1),
                    );
                }
                MindAct::AnnounceObjectives => {
                    if !ss13_game::antag::traitor::announce_objectives_of(world, body) {
                        return Err(format!("У {target_key} нет целей предателя."));
                    }
                }
            }
            mind_panel(world, &target)
        }
        AdminRequest::Dynamic => Ok(Some(AdminResponse::Dynamic(dynamic_report(world)))),
        AdminRequest::AdminList => Ok(Some(crate::verbs::admin_list(world))),
        AdminRequest::SetAdminRank { ckey: target, rank } => {
            crate::verbs::set_admin_rank(world, client, &key, &target, &rank)
        }
        AdminRequest::RemoveAdmin(target) => {
            crate::verbs::remove_admin(world, client, &key, &target)
        }
        AdminRequest::ManUp(target) => crate::verbs::man_up(world, &key, &target),
        AdminRequest::GlobalManUp => crate::verbs::global_man_up(world, &key),
        AdminRequest::PlaySound {
            file,
            volume,
            scope,
        } => crate::verbs::play_sound(world, client, &key, &file, volume, scope),
        AdminRequest::AirStatus => crate::verbs::air_status(world, client),
        AdminRequest::SpawnAir(text) => crate::verbs::spawn_air(world, client, &key, &text),
        AdminRequest::ForceMidround(tag) => {
            let picked = ss13_game::antag::midround::force_midround(world, &tag)?;
            admin_action(
                world,
                &format!(
                    "{key} forced the midround ruleset {tag}: {} selected.",
                    picked.len()
                ),
            );
            Ok(Some(AdminResponse::Dynamic(dynamic_report(world))))
        }
        AdminRequest::DropBomb(size) => {
            let own = own_mob(world, client)?;
            let pos = mob_pos(world, own)?;
            let (dev, heavy, light, flash) = bomb_ranges(world, size)?;
            let ex = ss13_game::explosions::Explosion {
                devastation: dev,
                heavy,
                light,
                flash: Some(flash),
                ignorecap: true,
                cause: key.clone(),
                ..ss13_game::explosions::Explosion::new(0, 0, 0)
            };
            ss13_game::explosions::explosion(world, pos, ex);
            admin_action(
                world,
                &format!("{key} creating an admin explosion at {}.", coords(pos)),
            );
            done(format!("explosion ({dev}, {heavy}, {light}, {flash})"))
        }
        AdminRequest::Emp { heavy, light } => {
            if heavy == 0 && light == 0 {
                return Err("Пустой импульс.".into());
            }
            if heavy > MAX_ADMIN_BOMB || light > MAX_ADMIN_BOMB {
                return Err(format!("Радиус больше {MAX_ADMIN_BOMB} не разрешён."));
            }
            let own = own_mob(world, client)?;
            let pos = mob_pos(world, own)?;
            world.write_message(ss13_game::emp::Empulse {
                epicenter: pos,
                heavy: i32::from(heavy),
                light: i32::from(light),
            });
            admin_action(
                world,
                &format!(
                    "{key} created an EM Pulse ({heavy},{light}) at {}.",
                    coords(pos)
                ),
            );
            done(format!("EM pulse ({heavy}, {light})"))
        }
        AdminRequest::SetSecurityLevel(name) => {
            let level = ss13_game::security_level::SecLevel::from_name(&name)
                .ok_or_else(|| format!("Нет уровня угрозы {name:?}."))?;
            ss13_game::security_level::set_level(world, level, true);
            admin_action(
                world,
                &format!("{key} changed the security level to {}", level.name()),
            );
            done(format!("security level {}", level.name()))
        }
    }
}

/// Custom "Drop Bomb" ranges beyond this many tiles are refused: tg asks to confirm a bomb over
/// the cap; here a blast walks `(2r + 1)^2` tiles in one tick, so it is bounded.
const MAX_ADMIN_BOMB: u8 = 40;

/// tg "Drop Bomb" choices as `(devastation, heavy, light, flash)`.
fn bomb_ranges(
    world: &World,
    size: ss13_shared::admin::BombSize,
) -> Result<(f32, f32, f32, f32), String> {
    use ss13_shared::admin::BombSize;
    Ok(match size {
        BombSize::Small => (1.0, 2.0, 3.0, 3.0),
        BombSize::Medium => (2.0, 3.0, 4.0, 4.0),
        BombSize::Big => (3.0, 5.0, 7.0, 5.0),
        BombSize::Maxcap => {
            let cap = ss13_game::explosions::wave::BombCap::new(
                world
                    .get_resource::<ss13_game::explosions::ExplosionConfig>()
                    .map_or(14.0, |c| c.bombcap),
            );
            (cap.devastation, cap.heavy, cap.light, cap.flash)
        }
        BombSize::Custom {
            devastation,
            heavy,
            light,
            flash,
        } => {
            if [devastation, heavy, light, flash]
                .iter()
                .any(|&r| r > MAX_ADMIN_BOMB)
            {
                return Err(format!("Не больше {MAX_ADMIN_BOMB} тайлов."));
            }
            (
                f32::from(devastation),
                f32::from(heavy),
                f32::from(light),
                f32::from(flash),
            )
        }
    })
}

/// tg `check_if_greater_rights_than` against the admin listed as `target_ckey` (online or not).
fn check_greater(world: &World, actor: Entity, target_ckey: &str) -> Result<(), String> {
    match world.resource::<Admins>().holders.get(target_ckey) {
        Some(holder) if !greater_rights(rights_of(world, actor), holder.rights) => {
            Err("Ошибка: у цели больше прав, чем у вас.".into())
        }
        _ => Ok(()),
    }
}

pub(crate) fn target_client(world: &mut World, target: &str) -> Result<Entity, String> {
    client_by_ckey(world, target).ok_or_else(|| format!("Игрок {} не в сети.", ckey(target)))
}

/// The mob holding the mind of `mob`: a ghost's body (also a DNR ghost's), else `mob`.
fn mind_mob(world: &World, mob: Entity) -> Entity {
    let dnr = world.get::<ss13_game::mind::DnrBody>(mob).map(|d| d.0);
    world
        .get::<Ghost>(mob)
        .and_then(|g| g.body.or(dnr))
        .filter(|&b| world.get_entity(b).is_ok())
        .unwrap_or(mob)
}

/// tg `traitor_panel` of the player.
fn mind_panel(world: &mut World, target: &str) -> AnswerResult {
    let (_, mob) = target_mob(world, target)?;
    let mut panel = ss13_game::mind::panel(world, mind_mob(world, mob));
    panel.ckey = ckey(target);
    Ok(Some(AdminResponse::Mind(panel)))
}

/// The target's client and its current mob (a ghost included).
pub(crate) fn target_mob(world: &mut World, target: &str) -> Result<(Entity, Entity), String> {
    let client = target_client(world, target)?;
    let mob = world
        .resource::<Players>()
        .mob(client)
        .filter(|&m| world.get_entity(m).is_ok())
        .ok_or_else(|| format!("{} в лобби.", ckey(target)))?;
    Ok((client, mob))
}

pub(crate) fn own_mob(world: &World, client: Entity) -> Result<Entity, String> {
    world
        .resource::<Players>()
        .mob(client)
        .filter(|&m| world.get_entity(m).is_ok())
        .ok_or_else(|| "У вас нет тела (лобби).".into())
}

pub(crate) fn mob_pos(world: &World, mob: Entity) -> Result<TilePos, String> {
    world
        .get::<TilePos>(mob)
        .copied()
        .ok_or_else(|| "Тело вне карты.".into())
}

fn key_name_of(world: &mut World, target: &str) -> String {
    match client_by_ckey(world, target) {
        Some(client) => key_name(world, client),
        None => ckey(target),
    }
}

fn coords(pos: TilePos) -> String {
    format!("({}, {}, {})", pos.x, pos.y, pos.z)
}

/// tg `admin_ghost`: a ghost that can go back, or back into the body.
fn aghost(world: &mut World, client: Entity, key: &str) -> AnswerResult {
    let own = own_mob(world, client)?;
    if let Some(ghost) = world.get::<Ghost>(own).copied() {
        let body = ghost
            .body
            .filter(|&b| world.get_entity(b).is_ok())
            .ok_or("Нет тела, в которое можно вернуться.")?;
        return_to_body(world, body);
        log_admin(world, "ADMIN", &format!("{key} re-entered their body."));
        return done("re-entered body");
    }
    if world.get::<NewPlayer>(client).is_some() {
        return Err("Нельзя стать админ-призраком в лобби.".into());
    }
    ghostize_client(world, client).ok_or("Нельзя стать админ-призраком отсюда.")?;
    admin_action(world, &format!("{key} admin ghosted."));
    done("admin ghosted")
}

/// tg orbit, simplified: an admin ghost stays on the followed mob's tile. Cost: one lookup per
/// following ghost per tick (admins only).
fn follow(
    mut ghosts: Query<(Entity, &Following, &TilePos)>,
    targets: Query<&TilePos, Without<Following>>,
    mut commands: Commands,
) {
    for (ghost, following, pos) in &mut ghosts {
        match targets.get(following.0) {
            Ok(to) if to != pos => {
                commands.entity(ghost).insert(*to);
            }
            Ok(_) => {}
            Err(_) => {
                commands.entity(ghost).remove::<Following>();
            }
        }
    }
}

/// tg player panel rows.
pub fn players(world: &mut World) -> Vec<PlayerEntry> {
    let mut q = world.query::<(Entity, &ClientToken)>();
    let clients: Vec<(Entity, String)> = q.iter(world).map(|(e, t)| (e, ckey(&t.0))).collect();
    let mut rows: Vec<PlayerEntry> = clients
        .into_iter()
        .map(|(client, ckey)| {
            let mob = world
                .resource::<Players>()
                .mob(client)
                .filter(|&m| world.get_entity(m).is_ok());
            let ghost = mob.and_then(|m| world.get::<Ghost>(m).copied());
            // A ghost's job and antagonist roles are its body's.
            let body = match ghost {
                Some(g) => g.body.filter(|&b| world.get_entity(b).is_ok()),
                None => mob,
            };
            let job = body
                .and_then(|b| world.get::<PlayerJob>(b))
                .and_then(|j| {
                    world
                        .get_resource::<Jobs>()
                        .and_then(|jobs| jobs.data.jobs.get(j.0))
                        .map(|job| job.title.clone())
                })
                .unwrap_or_default();
            let mut antag = Vec::new();
            if body.is_some_and(|b| world.get::<Traitor>(b).is_some()) {
                antag.push(ss13_game::antag::ROLE_TRAITOR.to_owned());
            }
            if body.is_some_and(|b| world.get::<Brother>(b).is_some()) {
                antag.push(ss13_game::antag::ROLE_BROTHER.to_owned());
            }
            if body.is_some_and(|b| world.get::<Changeling>(b).is_some()) {
                antag.push(ss13_game::antag::changeling::ROLE_CHANGELING.to_owned());
            }
            if body.is_some_and(|b| world.get::<Vampire>(b).is_some()) {
                antag.push(ss13_game::antag::vampire::ROLE_VAMPIRE.to_owned());
            }
            if body.is_some_and(|b| ss13_game::antag::cult::server::is_cultist(world, b)) {
                antag.push(ss13_game::antag::cult::ROLE_CULTIST.to_owned());
            }
            if body.is_some_and(|b| ss13_game::antag::heretic::server::is_heretic(world, b)) {
                antag.push(ss13_game::antag::heretic::ROLE_HERETIC.to_owned());
            }
            if let Some(rev) = body
                .and_then(|b| world.get::<ss13_game::antag::revolution::server::Revolutionary>(b))
            {
                antag.push(
                    if rev.head {
                        "Head Revolutionary"
                    } else {
                        "Revolutionary"
                    }
                    .into(),
                );
            }
            let state = match mob {
                None => PlayerState::Lobby,
                Some(_) if ghost.is_some() => PlayerState::Ghost,
                Some(m) if world.get::<Stat>(m) == Some(&Stat::Dead) => PlayerState::Dead,
                Some(_) => PlayerState::Alive,
            };
            PlayerEntry {
                name: mob
                    .and_then(|m| world.get::<Name>(m))
                    .map(|n| n.to_string())
                    .unwrap_or_default(),
                job,
                antag,
                state,
                pos: mob.and_then(|m| world.get::<TilePos>(m).copied()),
                muted: world.get::<Muted>(client).is_some(),
                rank: world
                    .get::<AdminSession>(client)
                    .map(|s| s.rank.clone())
                    .unwrap_or_default(),
                ckey,
            }
        })
        .collect();
    rows.sort_by(|a, b| a.ckey.cmp(&b.ckey));
    rows
}

// shortcut: view-only; tg's Dynamic Panel also edits the tier, threat and queued rulesets.
fn dynamic_report(world: &mut World) -> Vec<String> {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let mut lines = Vec::new();
    if let Some(ticker) = world.get_resource::<Ticker>() {
        lines.push(format!("Состояние раунда: {:?}", ticker.state()));
        let secs = ticker.round_duration(now) as u64;
        lines.push(format!(
            "Длительность: {:02}:{:02}:{:02}",
            secs / 3600,
            secs / 60 % 60,
            secs % 60
        ));
    }
    if let Some(config) = world.get_resource::<ss13_game::antag::dynamic::DynamicConfig>() {
        lines.push(match &config.forced_rulesets {
            Some(forced) => format!("Принудительные правила: {}", forced.join(", ")),
            None => "Правила: по броску тира".into(),
        });
    }
    lines.extend(ss13_game::antag::midround::report_lines(world));
    let mut antags: Vec<String> = Vec::new();
    let mut q = world.query::<(
        Entity,
        Has<Traitor>,
        Has<Brother>,
        Has<Changeling>,
        Has<Vampire>,
        Has<ss13_game::antag::cult::server::Cultist>,
        Has<ss13_game::antag::heretic::server::Heretic>,
        Option<&ss13_game::antag::revolution::server::Revolutionary>,
    )>();
    let found: Vec<(Entity, Vec<&str>)> = q
        .iter(world)
        .map(
            |(e, traitor, brother, changeling, vampire, cultist, heretic, rev)| {
                let roles = [
                    (rev.is_some_and(|r| r.head), "Head Revolutionary"),
                    (rev.is_some_and(|r| !r.head), "Revolutionary"),
                    (traitor, ss13_game::antag::ROLE_TRAITOR),
                    (brother, ss13_game::antag::ROLE_BROTHER),
                    (changeling, ss13_game::antag::changeling::ROLE_CHANGELING),
                    (vampire, ss13_game::antag::vampire::ROLE_VAMPIRE),
                    (cultist, ss13_game::antag::cult::ROLE_CULTIST),
                    (heretic, ss13_game::antag::heretic::ROLE_HERETIC),
                ];
                (
                    e,
                    roles
                        .into_iter()
                        .filter(|(on, _)| *on)
                        .map(|(_, r)| r)
                        .collect::<Vec<_>>(),
                )
            },
        )
        .filter(|(_, roles)| !roles.is_empty())
        .collect();
    for (mob, roles) in found {
        let name = world
            .get::<Name>(mob)
            .map_or_else(String::new, |n| n.to_string());
        let dead = world.get::<Stat>(mob) == Some(&Stat::Dead);
        let state = if dead { " (мёртв)" } else { "" };
        antags.extend(
            roles
                .into_iter()
                .map(|role| format!("{role}: {name}{state}")),
        );
    }
    antags.sort();
    lines.push(format!("Антагонисты: {}", antags.len()));
    lines.extend(antags.into_iter().map(|a| format!("  {a}")));
    if let Some(events) = world.get_resource::<ss13_game::events::Events>() {
        let running: Vec<&str> = events.running().collect();
        if !running.is_empty() {
            lines.push(format!("События: {}", running.join(", ")));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_report_lists_vampires() {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.spawn((Name::new("Влад"), Vampire::default(), Stat::Dead));
        let report = dynamic_report(&mut world);
        assert!(report.contains(&"Антагонисты: 1".to_owned()), "{report:?}");
        assert!(report.contains(&format!(
            "  {}: Влад (мёртв)",
            ss13_game::antag::vampire::ROLE_VAMPIRE
        )));
    }

    #[test]
    fn every_admin_verb_needs_rights() {
        use AdminRequest::*;
        assert_eq!(required_rights(&Adminhelp("x".into())), None);
        assert_eq!(required_rights(&Asay("x".into())), Some(R_ADMIN));
        assert_eq!(
            required_rights(&Ban {
                ckey: "a".into(),
                reason: String::new()
            }),
            Some(R_BAN)
        );
        assert_eq!(required_rights(&Spawn("/obj".into())), Some(R_SPAWN));
        assert_eq!(required_rights(&StartNow), Some(R_SERVER));
        assert_eq!(required_rights(&EndRound), Some(R_SERVER));
        assert_eq!(
            required_rights(&Smite {
                ckey: "a".into(),
                smite: ss13_shared::admin::Smite::Lightning
            }),
            Some(R_ADMIN | R_FUN)
        );
        assert_eq!(
            required_rights(&Emp { heavy: 1, light: 2 }),
            Some(R_ADMIN | R_FUN)
        );
        assert_eq!(required_rights(&TimeScale(1.0)), Some(R_DEBUG));
        assert_eq!(
            required_rights(&AdminRequest::MindPanel("a".into())),
            Some(R_ADMIN)
        );
        assert_eq!(
            required_rights(&AdminRequest::MindAct {
                ckey: "a".into(),
                act: ss13_shared::admin::MindAct::AnnounceObjectives
            }),
            Some(R_ADMIN)
        );
    }

    #[test]
    fn deadminned_admins_have_no_rights() {
        let mut session = AdminSession {
            ckey: "a".into(),
            rank: "Game Master".into(),
            rights: R_EVERYTHING,
            active: true,
        };
        assert_eq!(session.active_rights(), R_EVERYTHING);
        session.active = false;
        assert_eq!(session.active_rights(), 0);
    }

    #[test]
    fn text_is_sanitized_and_cut() {
        assert_eq!(sanitize_text("  a\u{202E}b\tc\u{7}  ", 100), "ab c");
        assert_eq!(sanitize_text("абвгд", 3), "абв");
    }

    #[test]
    fn rate_limit_allows_a_burst_then_the_rate() {
        let mut buckets = Buckets::default();
        let client = Entity::from_raw_u32(7).unwrap();
        let allowed = (0..30)
            .filter(|_| take_token(&mut buckets, client, 0.0))
            .count();
        assert_eq!(allowed, BURST as usize);
        let allowed = (0..30)
            .filter(|_| take_token(&mut buckets, client, 1.0))
            .count();
        assert_eq!(allowed, RATE_PER_SEC as usize);
    }

    #[test]
    fn timestamps_are_utc_dates() {
        let t = timestamp();
        assert_eq!(t.len(), 19, "{t}");
        assert!(t.starts_with("20"), "{t}");
    }
}
