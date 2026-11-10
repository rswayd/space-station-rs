//! The records consoles (server) after tg `/obj/machinery/computer/records` (`records.dm`,
//! `security.dm`, `medical.dm`): a click with an empty hand opens `SecurityRecords` or
//! `MedicalRecords`; "Войти" logs the console in when the user's ID has one of its
//! `req_one_access` (security: `ACCESS_SECURITY`/`ACCESS_HOP`; medical: medical, detective,
//! genetics); the records are edited through the acts.
//!
//! Every act but `login` needs the console to be logged in (tg checks only in the UI), and
//! `purge_records` needs armory access (tg only disables the button).
//! `print_record` prints a rapsheet paper or a wanted/missing poster
//! ([`super::posters`]) with the record's photo; a new citation alerts the fined person's PDA
//! (`alert_owner`); fines are paid at the warrant console ([`super::warrant`]).
//! An EMP scrambles the security records (`security/emp_act`).
//! `view_record` keeps the record's front photo as the user's character preview
//! ([`RecordPreviews`]); a photo of one tile used on a security console makes a new record with
//! that picture, named by the user (`insert_new_record`).
// shortcut: the preview has no static and scanline overlays; `purge_records` is
// immediate (tg: a 5 s `do_after`); a print appears at once with a 2 s busy time (tg: after
// 2 s); the UI's `station_z` is always true (the acts check the level).

use bevy::ecs::message::MessageCursor;
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use ss13_mapload::DmName;
use ss13_mapload::construct::DmObj;
use ss13_mapload::fidelity::is_under;
use ss13_shared::chat::Span;
use ss13_tgui::server::{AppTguiExt, OpenUi, ServerUi, UiActed, UiParams, UiReject, UiSystems};

use super::manifest::{Crime, Manifest, MedicalNote, alert_owner, record_photo, round_timestamp};
use super::*;
use crate::click::{ClickResolved, ResolveClicks};
use crate::id_access::ACCESS_ARMORY;
use crate::inventory::{IdCard, IdCards, Inventory, check_access};
use crate::power::Unpowered;

pub const SECURITY_CONSOLE: &str = "/obj/machinery/computer/records/security";
pub const MEDICAL_CONSOLE: &str = "/obj/machinery/computer/records/medical";
/// `MAX_CRIME_NAME_LEN`.
const MAX_CRIME_NAME_LEN: usize = 24;
/// `MAX_MESSAGE_LEN`, `MAX_BROADCAST_LEN`, `MAX_DESC_LEN`.
const MAX_MESSAGE_LEN: usize = 1024;
const MAX_BROADCAST_LEN: usize = 512;
const MAX_DESC_LEN: usize = 4096;
/// `CONFIG_GET(number/maxfine)` default.
pub const MAX_FINE: u32 = 1000;
/// `MAX_NAME_LEN`.
const MAX_NAME_LEN: usize = 42;
/// The print's `addtimer(..., 2 SECONDS)`.
const PRINT_SECS: f64 = 2.0;
const PAPER: &str = "/obj/item/paper";
const SOUND_ERROR: &str = "sound/machines/terminal/terminal_error.ogg";
const SOUND_PRINT: &str = "sound/machines/printer.ogg";

/// A security console's `printing`: game seconds until the printer is free.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq)]
pub struct RecordsPrinter(pub f64);

/// `character_preview_view` per user: the record each one last viewed and its picture.
// shortcut: an entry stays until the user views another record (tg drops the view when the
// last window closes); one per user who ever viewed a record here.
#[derive(Component, Clone, Debug, Default)]
pub struct RecordPreviews(
    pub bevy::ecs::entity::EntityHashMap<(u32, std::sync::Arc<crate::service::Picture>)>,
);

/// The name prompt of `insert_new_record`.
#[derive(Component, Clone, Copy, Debug)]
struct NewRecordPrompt {
    console: Entity,
    mob: Entity,
    photo: Entity,
}

const PHOTO: &str = "/obj/item/photo";
const SOUND_INSERT: &str = "sound/machines/terminal/terminal_insert_disc.ogg";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordsKind {
    Security,
    Medical,
}

/// A records console: which records, its `req_one_access`, and `authenticated`.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct RecordsConsole {
    pub kind: RecordsKind,
    pub req_one_access: Vec<String>,
    pub authenticated: bool,
}

impl RecordsConsole {
    /// The console of DM type `path`, if it is one.
    pub fn of_path(path: &str) -> Option<Self> {
        let kind = if is_under(path, SECURITY_CONSOLE) {
            RecordsKind::Security
        } else if is_under(path, MEDICAL_CONSOLE) {
            RecordsKind::Medical
        } else {
            return None;
        };
        let access: &[&str] = if path.contains("/syndie") {
            &["syndicate"]
        } else if kind == RecordsKind::Security {
            &["security", "hop"]
        } else {
            &["medical", "detective", "genetics"]
        };
        Some(Self {
            kind,
            req_one_access: access.iter().map(|a| (*a).to_owned()).collect(),
            authenticated: false,
        })
    }
}

pub struct RecordsConsolePlugin;

impl Plugin for RecordsConsolePlugin {
    fn build(&self, app: &mut App) {
        use crate::click_handlers::{AddClickHandler, ClickPhase};
        // A photo on a security console makes a new record.
        app.add_click_handler(ClickPhase::HeldOnTarget, 2719, |w, c| {
            c.held
                .is_some_and(|h| photo_interaction(w, c.client, c.mob, h, c.target))
        });
        app.register_ui::<SecurityRecordsUi>()
            .register_ui::<MedicalRecordsUi>()
            .add_observer(attach)
            .add_systems(
                Update,
                (
                    open_console.after(ResolveClicks).before(UiSystems::Open),
                    (security_acts, medical_acts).in_set(UiSystems::Handle),
                    new_record_named
                        .in_set(UiSystems::Handle)
                        .after(ss13_tgui::prompt::PromptSystems::Answer),
                    records_emps.in_set(crate::emp::EmpSystems::Effects),
                ),
            );
    }
}

fn attach(add: On<Add<DmObj>>, objs: Query<&DmObj>, mut commands: Commands) {
    if let Some(console) = objs
        .get(add.entity)
        .ok()
        .and_then(|o| RecordsConsole::of_path(&o.path))
    {
        commands.entity(add.entity).insert(console);
    }
}

/// `attack_hand` -> `ui_interact` of a powered console, with an empty hand.
fn open_console(
    mut clicks: MessageReader<ClickResolved>,
    consoles: Query<(&RecordsConsole, &DmName), Without<Unpowered>>,
    inventories: Query<&Inventory>,
    mut open: MessageWriter<OpenUi>,
) {
    for click in clicks.read() {
        let Ok((console, name)) = consoles.get(click.target) else {
            continue;
        };
        if inventories
            .get(click.mob)
            .is_ok_and(|i| i.active_item().is_some())
        {
            continue;
        }
        let title = crate::vending::plain_name(&name.name).to_owned();
        open.write(match console.kind {
            RecordsKind::Security => {
                OpenUi::new::<SecurityRecordsUi>(click.client, click.mob, click.target, title)
            }
            RecordsKind::Medical => {
                OpenUi::new::<MedicalRecordsUi>(click.client, click.mob, click.target, title)
            }
        });
    }
}

fn terminal_sound(world: &mut World, console: Entity, sound: &str) {
    if let Some(&pos) = world.get::<ss13_shared::TilePos>(console) {
        crate::sound::playsound(world, crate::sound::PlaySound::new(pos, sound, 70.0, true));
    }
}

/// `user.can_perform_action(src, ALLOW_SILICON_REACH)` and `is_operational`, now.
fn can_use(world: &World, user: Entity, console: Entity) -> bool {
    let near = match (
        world.get::<ss13_shared::TilePos>(user),
        world.get::<ss13_shared::TilePos>(console),
    ) {
        (Some(a), Some(b)) => a.chebyshev(*b).is_some_and(|d| d <= 1),
        _ => false,
    };
    near && world.get::<Unpowered>(console).is_none()
        && world
            .get::<crate::health::Stat>(user)
            .is_some_and(|s| *s == crate::health::Stat::Conscious)
}

/// `security/item_interaction` -> `insert_new_record`: a photo on a security console asks for
/// the new record's name. True when the click was the console's.
pub fn photo_interaction(
    world: &mut World,
    client: Entity,
    mob: Entity,
    held: Entity,
    target: Entity,
) -> bool {
    let Some(state) = world.get::<RecordsConsole>(target).cloned() else {
        return false;
    };
    let is_photo = world
        .get::<ss13_mapload::DmItem>(held)
        .is_some_and(|i| is_under(&i.path, PHOTO));
    if state.kind != RecordsKind::Security || !is_photo {
        return false;
    }
    let Some(picture) = world
        .get::<crate::service::Photo>(held)
        .and_then(|p| p.picture.clone())
    else {
        return true;
    };
    if !can_use(world, mob, target) {
        return true;
    }
    if !state.authenticated && !check_access(&[], &state.req_one_access, &user_access(world, mob)) {
        crate::balloon::balloon_alert(world, mob, target, "доступ запрещен");
        terminal_sound(world, target, SOUND_ERROR);
        return true;
    }
    // `psize_x > ICON_SIZE_X`: a mugshot is one tile.
    if picture.width > 1 || picture.height > 1 {
        crate::balloon::balloon_alert(world, mob, target, "фотография слишком большая!");
        terminal_sound(world, target, SOUND_ERROR);
        return true;
    }
    // `copytext(mugshot.name, 9, MAX_NAME_LEN)`: without "photo - ".
    let trimmed: String = name_of_item(world, held)
        .chars()
        .skip(8)
        .take(MAX_NAME_LEN - 9)
        .collect();
    let prompt = ss13_tgui::prompt::ask_world(
        world,
        ss13_tgui::prompt::Ask::new(
            client,
            mob,
            "New Record",
            "Enter the name of the new record.",
            ss13_tgui::prompt::PromptKind::Text {
                default: trimmed,
                max_length: MAX_NAME_LEN as u32,
                multiline: false,
            },
        ),
    );
    world.entity_mut(prompt).insert(NewRecordPrompt {
        console: target,
        mob,
        photo: held,
    });
    true
}

fn name_of_item(world: &World, item: Entity) -> String {
    world
        .get::<DmName>(item)
        .map(|n| crate::vending::plain_name(&n.name).to_owned())
        .unwrap_or_default()
}

/// The name answered: checked again (console working, user able and next to it, the photo still
/// theirs), then the record with the photo's picture, and the photo is used up.
fn new_record_named(
    world: &mut World,
    mut answers: Local<MessageCursor<ss13_tgui::prompt::PromptAnswered>>,
) {
    let pending: Vec<(Entity, Option<ss13_tgui::prompt::PromptAnswer>)> = answers
        .read(world.resource::<Messages<ss13_tgui::prompt::PromptAnswered>>())
        .map(|a| (a.prompt, a.answer.clone()))
        .collect();
    for (prompt, answer) in pending {
        let Some(ask) = world.get::<NewRecordPrompt>(prompt).copied() else {
            continue;
        };
        let Some(ss13_tgui::prompt::PromptAnswer::Text(name)) = answer else {
            continue;
        };
        let name = sanitize(&name, MAX_NAME_LEN);
        let held = world
            .get::<Inventory>(ask.mob)
            .is_some_and(|i| i.hands.contains(&Some(ask.photo)));
        let picture = world
            .get::<crate::service::Photo>(ask.photo)
            .and_then(|p| p.picture.clone());
        let (Some(picture), true) = (picture, held) else {
            continue;
        };
        if name.is_empty() || !can_use(world, ask.mob, ask.console) {
            continue;
        }
        world
            .resource_mut::<Manifest>()
            .add_from_photo(&name, picture);
        crate::balloon::balloon_alert(world, ask.mob, ask.console, "запись создана");
        terminal_sound(world, ask.console, SOUND_INSERT);
        world.despawn(ask.photo);
    }
}

/// What `ui_data` reads about the user and the records.
#[derive(bevy::ecs::system::SystemParam)]
pub struct RecordsParams<'w, 's> {
    manifest: Res<'w, Manifest>,
    names: Query<'w, 's, &'static Name>,
    inventories: Query<'w, 's, &'static Inventory>,
    cards: IdCards<'w, 's>,
}

impl RecordsParams<'_, '_> {
    /// `has_armory_access`: `ACCESS_ARMORY` on `get_idcard(TRUE)`.
    fn armory(&self, user: Entity) -> bool {
        self.inventories
            .get(user)
            .ok()
            .and_then(|i| i.access(&self.cards))
            .is_some_and(|a| a.iter().any(|x| x == ACCESS_ARMORY))
    }
}

fn crime_entry(c: &Crime) -> CrimeEntry {
    CrimeEntry {
        author: c.author.clone(),
        crime_ref: c.id.to_string(),
        details: c.details.clone(),
        fine: c.fine,
        name: c.name.clone(),
        paid: c.paid,
        time: c.time.clone(),
        valid: c.valid,
        voider: c.voider.clone(),
    }
}

impl ServerUi for SecurityRecordsUi {
    type Source = (&'static RecordsConsole, Option<&'static RecordPreviews>);
    type UserParam = RecordsParams<'static, 'static>;
    type Act = RecordsAct;

    fn static_data(_: (&RecordsConsole, Option<&RecordPreviews>)) -> AgeLimits {
        AgeLimits {
            min_age: AGE_MIN,
            max_age: AGE_MAX,
        }
    }

    fn data(
        (console, previews): (&RecordsConsole, Option<&RecordPreviews>),
        user: Entity,
        params: &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> SecurityRecordsData {
        if !console.authenticated {
            return SecurityRecordsData::default();
        }
        let preview = previews
            .and_then(|p| p.0.get(&user))
            .filter(|(id, _)| params.manifest.get(*id).is_some())
            .map(|(id, picture)| RecordPreview {
                crew_ref: id.to_string(),
                picture: crate::service::PhotoData {
                    name: String::new(),
                    width: picture.width,
                    height: picture.height,
                    sprites: picture.sprites.clone(),
                    scribble: String::new(),
                },
            });
        SecurityRecordsData {
            authenticated: true,
            preview,
            station_z: true,
            available_statuses: WANTED_STATUSES.map(str::to_owned).to_vec(),
            current_user: params
                .names
                .get(user)
                .map(|n| n.to_string())
                .unwrap_or_default(),
            higher_access: params.armory(user),
            records: params
                .manifest
                .general
                .iter()
                .map(|r| SecurityRecord {
                    age: r.age,
                    citations: r.citations.iter().map(crime_entry).collect(),
                    crew_ref: r.id.to_string(),
                    crimes: r.crimes.iter().map(crime_entry).collect(),
                    fingerprint: r.fingerprint.clone(),
                    gender: r.gender.clone(),
                    name: r.name.clone(),
                    note: r.security_note.clone(),
                    rank: r.rank.clone(),
                    species: r.species.clone(),
                    trim: r.trim.clone(),
                    wanted_status: r.wanted_status.clone(),
                    voice: r.voice.clone(),
                })
                .collect(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<RecordsAct, UiReject> {
        parse(action, params)
    }
}

impl ServerUi for MedicalRecordsUi {
    type Source = &'static RecordsConsole;
    type UserParam = RecordsParams<'static, 'static>;
    type Act = RecordsAct;

    fn static_data(_: &RecordsConsole) -> MedicalRecordsStatic {
        MedicalRecordsStatic {
            min_age: AGE_MIN,
            max_age: AGE_MAX,
            physical_statuses: PHYSICAL_STATUSES.map(str::to_owned).to_vec(),
            mental_statuses: MENTAL_STATUSES.map(str::to_owned).to_vec(),
            blood_types: BLOOD_TYPES.map(str::to_owned).to_vec(),
        }
    }

    fn data(
        console: &RecordsConsole,
        _: Entity,
        params: &SystemParamItem<'_, '_, Self::UserParam>,
    ) -> MedicalRecordsData {
        if !console.authenticated {
            return MedicalRecordsData::default();
        }
        MedicalRecordsData {
            authenticated: true,
            station_z: true,
            records: params
                .manifest
                .general
                .iter()
                .map(|r| MedicalRecord {
                    age: r.age,
                    blood_type: r.blood_type.clone(),
                    crew_ref: r.id.to_string(),
                    dna: r.dna_string.clone(),
                    gender: r.gender.clone(),
                    major_disabilities: r.major_disabilities_desc.clone(),
                    minor_disabilities: r.minor_disabilities_desc.clone(),
                    physical_status: r.physical_status.clone(),
                    cause_of_death: r.cause_of_death.clone(),
                    mental_status: r.mental_status.clone(),
                    name: r.name.clone(),
                    notes: r
                        .medical_notes
                        .iter()
                        .map(|n| MedicalNoteEntry {
                            author: n.author.clone(),
                            content: n.content.clone(),
                            note_ref: n.id.to_string(),
                            time: n.time.clone(),
                        })
                        .collect(),
                    quirk_notes: r.quirk_notes.clone(),
                    rank: r.rank.clone(),
                    species: r.species.clone(),
                    trim: r.trim.clone(),
                })
                .collect(),
        }
    }

    fn parse_act(action: &str, params: &UiParams) -> Result<RecordsAct, UiReject> {
        parse(action, params)
    }
}

/// A parsed act of either console; which acts a console accepts is checked when applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordsAct {
    Login,
    Logout,
    Purge,
    /// `view_record`: the character preview of a record.
    View(u32),
    EditField {
        record: u32,
        field: String,
        value: String,
    },
    Expunge(u32),
    Delete(u32),
    AddCrime {
        record: u32,
        name: String,
        details: String,
        fine: u32,
    },
    EditCrime {
        record: u32,
        crime: u32,
        name: Option<String>,
        description: Option<String>,
    },
    InvalidateCrime {
        record: u32,
        crime: u32,
    },
    SetNote(u32, String),
    SetWanted(u32, String),
    AddNote(u32, String),
    DeleteNote(u32, u32),
    SetPhysical(u32, String),
    SetMental(u32, String),
    SetCauseOfDeath(u32, String),
    SetBloodType(u32, String),
    /// `print_record`: `kind` is [`PRINT_MISSING`], [`PRINT_WANTED`] or [`PRINT_RAPSHEET`].
    Print {
        record: u32,
        kind: String,
        alias: String,
        desc: String,
        head: String,
    },
}

fn id_param(params: &UiParams, key: &str) -> Result<u32, UiReject> {
    params
        .get(key)
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| UiReject::BadParams(format!("{key}: expected a record ref")))
}

fn text(params: &UiParams, key: &str) -> String {
    params.get(key).unwrap_or_default().to_owned()
}

fn parse(action: &str, params: &UiParams) -> Result<RecordsAct, UiReject> {
    let record = || id_param(params, "crew_ref");
    Ok(match action {
        LOGIN => RecordsAct::Login,
        LOGOUT => RecordsAct::Logout,
        PURGE_RECORDS => RecordsAct::Purge,
        VIEW_RECORD => RecordsAct::View(id_param(params, "crew_ref")?),
        EDIT_FIELD => RecordsAct::EditField {
            record: id_param(params, "ref")?,
            field: text(params, "field"),
            value: text(params, "value"),
        },
        EXPUNGE_RECORD => RecordsAct::Expunge(record()?),
        DELETE_RECORD => RecordsAct::Delete(record()?),
        ADD_CRIME => RecordsAct::AddCrime {
            record: record()?,
            name: text(params, "name"),
            details: text(params, "details"),
            fine: match params.get("fine").filter(|f| !f.is_empty()) {
                None => 0,
                Some(f) => f
                    .parse()
                    .map_err(|_| UiReject::BadParams("fine: expected a number".into()))?,
            },
        },
        EDIT_CRIME => RecordsAct::EditCrime {
            record: record()?,
            crime: id_param(params, "crime_ref")?,
            name: params.get("name").map(str::to_owned),
            description: params.get("description").map(str::to_owned),
        },
        INVALIDATE_CRIME => RecordsAct::InvalidateCrime {
            record: record()?,
            crime: id_param(params, "crime_ref")?,
        },
        SET_NOTE => RecordsAct::SetNote(record()?, text(params, "note")),
        SET_WANTED => RecordsAct::SetWanted(record()?, text(params, "status")),
        ADD_NOTE => RecordsAct::AddNote(record()?, text(params, "content")),
        DELETE_NOTE => RecordsAct::DeleteNote(record()?, id_param(params, "note_ref")?),
        SET_PHYSICAL_STATUS => RecordsAct::SetPhysical(record()?, text(params, "physical_status")),
        SET_MENTAL_STATUS => RecordsAct::SetMental(record()?, text(params, "mental_status")),
        SET_CAUSE_OF_DEATH => RecordsAct::SetCauseOfDeath(record()?, text(params, "cause")),
        SET_BLOOD_TYPE => RecordsAct::SetBloodType(record()?, text(params, "blood_type")),
        PRINT_RECORD => RecordsAct::Print {
            record: record()?,
            kind: text(params, "type"),
            alias: text(params, "alias"),
            desc: text(params, "desc"),
            head: text(params, "head"),
        },
        _ => return Err(UiReject::UnknownAction),
    })
}

/// `strip_html_full(text, max)` / `reject_bad_name`: no markup or control characters, trimmed,
/// at most `max` characters.
// shortcut: `reject_bad_name`'s word rules (no all-caps, no digits-only, ...) are not applied.
pub fn sanitize(text: &str, max: usize) -> String {
    text.chars()
        .filter(|c| !c.is_control() && *c != '<' && *c != '>')
        .take(max)
        .collect::<String>()
        .trim()
        .to_owned()
}

fn security_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<SecurityRecordsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, RecordsAct)> = acts
        .read(world.resource::<Messages<UiActed<SecurityRecordsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, console, act) in pending {
        if let Err(why) = apply(
            world,
            client,
            actor,
            console,
            RecordsKind::Security,
            act.clone(),
        ) {
            debug!("SecurityRecords {act:?} on {console} refused: {why}");
        }
    }
}

fn medical_acts(world: &mut World, mut acts: Local<MessageCursor<UiActed<MedicalRecordsUi>>>) {
    let pending: Vec<(Entity, Entity, Entity, RecordsAct)> = acts
        .read(world.resource::<Messages<UiActed<MedicalRecordsUi>>>())
        .map(|a| (a.client, a.actor, a.source, a.act.clone()))
        .collect();
    for (client, actor, console, act) in pending {
        if let Err(why) = apply(
            world,
            client,
            actor,
            console,
            RecordsKind::Medical,
            act.clone(),
        ) {
            debug!("MedicalRecords {act:?} on {console} refused: {why}");
        }
    }
}

/// The access of the ID `user` holds or wears (`get_idcard(TRUE)`), read now.
fn user_access(world: &World, user: Entity) -> Vec<String> {
    world
        .get::<Inventory>(user)
        .and_then(|i| i.id_card(world))
        .and_then(|c| world.get::<IdCard>(c))
        .map(|c| c.access.clone())
        .unwrap_or_default()
}

/// tg `is_station_level(z)` of the console.
fn on_station(world: &World, console: Entity) -> bool {
    world
        .get::<ss13_shared::TilePos>(console)
        .is_some_and(|p| crate::events::controller::station_levels(world).contains(&p.z))
}

fn armory(world: &World, user: Entity) -> bool {
    user_access(world, user).iter().any(|a| a == ACCESS_ARMORY)
}

fn name_of(world: &World, mob: Entity) -> String {
    world
        .get::<Name>(mob)
        .map(|n| n.to_string())
        .unwrap_or_default()
}

/// `ui_act` of a console of `kind` at the moment of effect.
fn apply(
    world: &mut World,
    client: Entity,
    user: Entity,
    console: Entity,
    kind: RecordsKind,
    act: RecordsAct,
) -> Result<(), &'static str> {
    let state = world
        .get::<RecordsConsole>(console)
        .cloned()
        .ok_or("not a console")?;
    if state.kind != kind {
        return Err("wrong console");
    }
    if world.get::<Unpowered>(console).is_some() {
        return Err("unpowered");
    }
    match act {
        RecordsAct::Login => {
            // `secure_login`: `allowed(user)`.
            let ok = check_access(&[], &state.req_one_access, &user_access(world, user));
            if !ok {
                crate::tools::tell(world, client, Span::Warning, "Доступ запрещен.");
            }
            world
                .get_mut::<RecordsConsole>(console)
                .expect("read above")
                .authenticated = ok;
            return Ok(());
        }
        RecordsAct::Logout => {
            world
                .get_mut::<RecordsConsole>(console)
                .expect("read above")
                .authenticated = false;
            return Ok(());
        }
        _ if !state.authenticated => return Err("not logged in"),
        RecordsAct::View(record) => {
            if kind != RecordsKind::Security {
                return Ok(());
            }
            let picture = record_photo(world, record, true).ok_or("no such record")?;
            let mut previews = world
                .get::<RecordPreviews>(console)
                .cloned()
                .unwrap_or_default();
            previews.0.insert(user, (record, picture));
            world.entity_mut(console).insert(previews);
            return Ok(());
        }
        // "Don't let people off station futz with the station network."
        RecordsAct::Purge | RecordsAct::Expunge(_) if !on_station(world, console) => {
            crate::tools::tell(world, client, Span::Warning, "Слишком далеко!");
            return Err("off station");
        }
        RecordsAct::Print {
            record,
            kind: print_kind,
            alias,
            desc,
            head,
        } => {
            if kind != RecordsKind::Security {
                return Err("security only");
            }
            return print_record(
                world,
                user,
                console,
                record,
                &print_kind,
                &alias,
                &desc,
                &head,
            );
        }
        RecordsAct::Purge => {
            if !armory(world, user) {
                return Err("no armory access");
            }
            let mut manifest = world.resource_mut::<Manifest>();
            for r in &mut manifest.general {
                expunge(r, kind);
            }
            return Ok(());
        }
        _ => {}
    }
    let user_name = name_of(world, user);
    let time = round_timestamp(world);
    let higher = armory(world, user);
    let mut manifest = world.resource_mut::<Manifest>();
    let id = match &act {
        RecordsAct::EditField { record, .. }
        | RecordsAct::Expunge(record)
        | RecordsAct::Delete(record)
        | RecordsAct::AddCrime { record, .. }
        | RecordsAct::EditCrime { record, .. }
        | RecordsAct::InvalidateCrime { record, .. }
        | RecordsAct::SetNote(record, _)
        | RecordsAct::SetWanted(record, _)
        | RecordsAct::AddNote(record, _)
        | RecordsAct::DeleteNote(record, _)
        | RecordsAct::SetPhysical(record, _)
        | RecordsAct::SetMental(record, _)
        | RecordsAct::SetCauseOfDeath(record, _)
        | RecordsAct::SetBloodType(record, _) => *record,
        _ => unreachable!("handled above"),
    };
    if let RecordsAct::Delete(_) = act {
        if kind != RecordsKind::Security {
            return Err("security only");
        }
        manifest.general.retain(|r| r.id != id);
        manifest.photos.retain(|(r, _), _| *r != id);
        return Ok(());
    }
    let next_id = manifest.next_id();
    let record = manifest.get_mut(id).ok_or("no such record")?;
    // `new_citation.alert_owner(...)`, sent once the records are no longer borrowed.
    let mut alert = None;
    match (kind, act) {
        (_, RecordsAct::EditField { field, value, .. }) => edit_field(record, &field, &value)?,
        (_, RecordsAct::Expunge(_)) => expunge(record, kind),
        (
            RecordsKind::Security,
            RecordsAct::AddCrime {
                name,
                details,
                fine,
                ..
            },
        ) => {
            let name = sanitize(&name, MAX_CRIME_NAME_LEN);
            if name.is_empty() {
                return Err("no crime name");
            }
            if fine > MAX_FINE {
                return Err("fine over the maximum");
            }
            let details = sanitize(&details, MAX_MESSAGE_LEN);
            let crime = Crime {
                id: next_id,
                name,
                details: if details.is_empty() {
                    "Детали не указаны.".to_owned()
                } else {
                    details
                },
                author: user_name,
                author_mob: Some(user),
                time,
                valid: true,
                voider: None,
                fine: (fine > 0).then_some(fine),
                paid: 0,
            };
            if fine == 0 {
                record.crimes.push(crime);
                record.wanted_status = WANTED_ARREST.to_owned();
            } else {
                alert = Some((
                    record.name.clone(),
                    format!(
                        "Вам был выставлен штраф в размере {fine}{} за '{}'. Штраф может быть \
                         оплачен у отдела безопасности.",
                        crate::cargo::MONEY_SYMBOL,
                        crime.name
                    ),
                ));
                record.citations.push(crime);
            }
        }
        (
            RecordsKind::Security,
            RecordsAct::EditCrime {
                crime,
                name,
                description,
                ..
            },
        ) => {
            let crime = record
                .crimes
                .iter_mut()
                .find(|c| c.id == crime && c.valid)
                .ok_or("no such valid crime")?;
            if crime.author_mob != Some(user) && !higher {
                return Err("not the author");
            }
            if let Some(name) = name.filter(|n| n.chars().count() > 2 && *n != crime.name) {
                crime.name = sanitize(&name, MAX_CRIME_NAME_LEN);
            } else if let Some(d) = description.filter(|d| d.chars().count() > 2) {
                crime.details = sanitize(&d, MAX_MESSAGE_LEN);
            } else {
                return Err("nothing to change");
            }
        }
        (RecordsKind::Security, RecordsAct::InvalidateCrime { crime, .. }) => {
            let (list, acquitted) = if record.crimes.iter().any(|c| c.id == crime) {
                (&mut record.crimes, true)
            } else {
                (&mut record.citations, false)
            };
            let c = list
                .iter_mut()
                .find(|c| c.id == crime)
                .ok_or("no such crime")?;
            if c.author_mob != Some(user) && !higher {
                return Err("not the author");
            }
            c.valid = false;
            c.voider = Some(user_name);
            if acquitted && !record.crimes.iter().any(|c| c.valid) {
                record.wanted_status = WANTED_DISCHARGED.to_owned();
            }
        }
        (RecordsKind::Security, RecordsAct::SetNote(_, note)) => {
            record.security_note = sanitize(&note, MAX_MESSAGE_LEN);
        }
        (RecordsKind::Security, RecordsAct::SetWanted(_, status)) => {
            if !WANTED_STATUSES.contains(&status.as_str()) {
                return Err("unknown status");
            }
            if status == WANTED_ARREST && record.crimes.is_empty() {
                return Err("arrest needs a crime");
            }
            record.wanted_status = status;
        }
        (RecordsKind::Medical, RecordsAct::AddNote(_, content)) => {
            let content = sanitize(&content, MAX_MESSAGE_LEN);
            if content.is_empty() {
                return Err("empty note");
            }
            while record.medical_notes.len() > 2 {
                record.medical_notes.remove(0);
            }
            record.medical_notes.push(MedicalNote {
                id: next_id,
                author: user_name,
                content,
                time,
            });
        }
        (RecordsKind::Medical, RecordsAct::DeleteNote(_, note)) => {
            let before = record.medical_notes.len();
            record.medical_notes.retain(|n| n.id != note);
            if record.medical_notes.len() == before {
                return Err("no such note");
            }
        }
        (RecordsKind::Medical, RecordsAct::SetPhysical(_, status)) => {
            if !PHYSICAL_STATUSES.contains(&status.as_str()) {
                return Err("unknown status");
            }
            if status != PHYSICAL_DECEASED {
                record.cause_of_death.clear();
            }
            record.physical_status = status;
        }
        (RecordsKind::Medical, RecordsAct::SetMental(_, status)) => {
            if !MENTAL_STATUSES.contains(&status.as_str()) {
                return Err("unknown status");
            }
            record.mental_status = status;
        }
        (RecordsKind::Medical, RecordsAct::SetCauseOfDeath(_, cause)) => {
            let cause = sanitize(&cause, MAX_DESC_LEN);
            if cause.is_empty() {
                return Err("empty cause");
            }
            record.cause_of_death = cause;
        }
        (RecordsKind::Medical, RecordsAct::SetBloodType(_, blood)) => {
            if !BLOOD_TYPES.contains(&blood.as_str()) {
                return Err("unknown blood type");
            }
            record.blood_type = blood;
        }
        _ => return Err("not an act of this console"),
    }
    if let Some((name, message)) = alert {
        alert_owner(world, console, &name, &message);
    }
    Ok(())
}

/// `print_record(user, target, params)` of a logged-in security console.
#[allow(clippy::too_many_arguments)]
fn print_record(
    world: &mut World,
    user: Entity,
    console: Entity,
    record: u32,
    kind: &str,
    alias: &str,
    desc: &str,
    head: &str,
) -> Result<(), &'static str> {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let sound = |world: &mut World, file: &str| {
        if let Some(&pos) = world.get::<ss13_shared::TilePos>(console) {
            crate::sound::playsound(world, crate::sound::PlaySound::new(pos, file, 100.0, true));
        }
    };
    if world
        .get::<RecordsPrinter>(console)
        .is_some_and(|p| now < p.0)
    {
        crate::balloon::balloon_alert(world, user, console, "принтер занят");
        sound(world, SOUND_ERROR);
        return Err("printer busy");
    }
    let target = world
        .resource::<Manifest>()
        .get(record)
        .cloned()
        .ok_or("no such record")?;
    let poster = match kind {
        PRINT_MISSING => Some(super::posters::PosterKind::Missing),
        PRINT_WANTED => Some(super::posters::PosterKind::Wanted),
        PRINT_RAPSHEET => None,
        _ => return Err("unknown print type"),
    };
    if kind != PRINT_MISSING && target.crimes.is_empty() {
        crate::balloon::balloon_alert(world, user, console, "нет преступлений");
        return Err("no crimes");
    }
    let pos = world
        .get::<ss13_shared::TilePos>(console)
        .copied()
        .ok_or("off the map")?;
    let alias = Some(sanitize(alias, MAX_NAME_LEN))
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| target.name.clone());
    let mut description = Some(sanitize(desc, MAX_BROADCAST_LEN))
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| "No further details.".to_owned());
    let header = Some(sanitize(head, 8))
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| ss13_shared::capitalize(kind));
    world
        .entity_mut(console)
        .insert(RecordsPrinter(now + PRINT_SECS));
    crate::balloon::balloon_alert(world, user, console, "распечатывание");
    sound(world, SOUND_PRINT);
    match poster {
        Some(poster) => {
            if poster == super::posters::PosterKind::Wanted {
                description.push_str("\n\nРАЗЫСКИВАЕТСЯ:");
                for incident in &target.crimes {
                    if !incident.valid {
                        description.push_str("--РЕДАКТИРОВАНО--");
                        continue;
                    }
                    description.push_str(&format!(
                        "\nПреступление: {}\nДетали: {}\n",
                        incident.name, incident.details
                    ));
                }
            }
            let photo = record_photo(world, record, true);
            super::posters::print_poster(world, pos, poster, photo, &alias, &description, &header)
                .ok_or("no poster type")?;
        }
        None => {
            let n = {
                let mut count =
                    world.get_resource_or_insert_with(crate::service::RapsheetCount::default);
                count.0 += 1;
                count.0
            };
            let text = format!(
                "<center><b>SR-{n}: {header}</b></center><br>{}",
                super::manifest::rapsheet_text(&target, &alias, &description)
            );
            let paper = crate::construction::spawn_obj(world, PAPER, pos).ok_or("no paper")?;
            world.entity_mut(paper).insert(ss13_mapload::DmName {
                name: format!("SR-{n} '{}'", target.name),
                desc: String::new(),
            });
            crate::service::write_printed(world, paper, text);
        }
    }
    Ok(())
}

/// `security/emp_act` of powered security consoles.
fn records_emps(world: &mut World, mut cursor: Local<MessageCursor<crate::emp::EmpAct>>) {
    for ev in crate::emp::emp_acts(world, &mut cursor) {
        let security = world
            .get::<RecordsConsole>(ev.target)
            .is_some_and(|c| c.kind == RecordsKind::Security);
        if security && ev.hits_self() && world.get::<Unpowered>(ev.target).is_none() {
            scramble_records(world, ev.severity);
        }
    }
}

/// Each record: at `10 / severity` % one field scrambled (a random name, gender, age 5-85,
/// wanted status or species), else at 1 % the record deleted.
pub fn scramble_records(world: &mut World, severity: u8) {
    const GENDERS: [&str; 3] = ["Male", "Female", "Other"];
    let ids: Vec<u32> = world
        .resource::<Manifest>()
        .general
        .iter()
        .map(|r| r.id)
        .collect();
    let species: Vec<&'static str> = crate::species::SpeciesId::roundstart()
        .map(|s| s.def().name)
        .collect();
    world.init_resource::<crate::emp::EmpRng>();
    for id in ids {
        let mut rng = world.resource_mut::<crate::emp::EmpRng>();
        let scrambled = rng.prob(10.0 / f64::from(severity.max(1)));
        if !scrambled {
            if rng.prob(1.0) {
                world
                    .resource_mut::<Manifest>()
                    .general
                    .retain(|r| r.id != id);
            }
            continue;
        }
        let which = rng.range(1, 5);
        let pick = rng.range(0, 1000) as usize;
        let age = rng.range(5, 85) as u32;
        let name = if which == 1 {
            world
                .get_resource_mut::<crate::jobs::Jobs>()
                .and_then(|mut j| j.random_name(&Default::default()))
        } else {
            None
        };
        let mut manifest = world.resource_mut::<Manifest>();
        let Some(r) = manifest.get_mut(id) else {
            continue;
        };
        match which {
            1 => {
                if let Some(name) = name {
                    r.name = name;
                }
            }
            2 => r.gender = GENDERS[pick % GENDERS.len()].to_owned(),
            3 => r.age = age,
            4 => r.wanted_status = WANTED_STATUSES[pick % WANTED_STATUSES.len()].to_owned(),
            _ => {
                if !species.is_empty() {
                    r.species = species[pick % species.len()].to_owned();
                }
            }
        }
    }
}

/// `edit_field`: the fields the windows edit; the value as `reject_bad_name` leaves it, or
/// "Unknown".
pub fn edit_field(
    record: &mut super::manifest::CrewRecord,
    field: &str,
    value: &str,
) -> Result<(), &'static str> {
    let value = sanitize(value, MAX_BROADCAST_LEN);
    let value = if value.is_empty() {
        "Unknown".to_owned()
    } else {
        value
    };
    let slot = match field {
        "age" => {
            let age: u32 = value.parse().map_err(|_| "age: not a number")?;
            if !(AGE_MIN..=AGE_MAX).contains(&age) {
                return Err("age out of range");
            }
            record.age = age;
            return Ok(());
        }
        "name" => &mut record.name,
        "rank" => &mut record.rank,
        "species" => &mut record.species,
        "gender" => &mut record.gender,
        "fingerprint" => &mut record.fingerprint,
        "voice" => &mut record.voice,
        "security_note" => &mut record.security_note,
        "dna_string" => &mut record.dna_string,
        "major_disabilities_desc" => &mut record.major_disabilities_desc,
        "minor_disabilities_desc" => &mut record.minor_disabilities_desc,
        "quirk_notes" => &mut record.quirk_notes,
        _ => return Err("not an editable field"),
    };
    *slot = value;
    Ok(())
}

/// `expunge_record_info` of the console of `kind`.
pub fn expunge(record: &mut super::manifest::CrewRecord, kind: RecordsKind) {
    match kind {
        RecordsKind::Security => {
            record.citations.clear();
            record.crimes.clear();
            record.security_note.clear();
            record.wanted_status = WANTED_NONE.to_owned();
        }
        RecordsKind::Medical => {
            record.age = 18;
            record.blood_type = BLOOD_TYPES[7].to_owned();
            record.dna_string = "Unknown".to_owned();
            record.gender = "Unknown".to_owned();
            record.major_disabilities.clear();
            record.major_disabilities_desc.clear();
            record.medical_notes.clear();
            record.minor_disabilities.clear();
            record.minor_disabilities_desc.clear();
            record.physical_status.clear();
            record.mental_status.clear();
            record.name = "Unknown".to_owned();
            record.quirk_notes.clear();
            record.rank = "Unknown".to_owned();
            record.species = "Unknown".to_owned();
            record.trim = "Unknown".to_owned();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::manifest::Joiner;
    use ss13_net::chat::server::SendChat;

    fn setup() -> (World, Entity, u32) {
        let mut world = World::new();
        world.init_resource::<Time>();
        world.init_resource::<Messages<SendChat>>();
        let mut manifest = Manifest::default();
        let id = manifest.inject(Joiner {
            name: "Боб",
            assignment: "Ассистент",
            job: None,
            mob: Entity::PLACEHOLDER,
        });
        world.insert_resource(manifest);
        // One station level, z 1.
        world.insert_resource(ss13_mapload::MapLoadPlugin {
            source: default(),
            maps: vec!["station.dmm".into()],
            assets: default(),
        });
        let console = world
            .spawn((
                RecordsConsole::of_path(SECURITY_CONSOLE).unwrap(),
                ss13_shared::TilePos::new(1, 1, 1),
            ))
            .id();
        (world, console, id)
    }

    fn officer(world: &mut World, name: &str, access: &[&str]) -> Entity {
        let card = world
            .spawn(IdCard {
                access: access.iter().map(|a| (*a).to_owned()).collect(),
            })
            .id();
        world
            .spawn((
                Name::new(name.to_owned()),
                Inventory {
                    id: Some(card),
                    ..default()
                },
            ))
            .id()
    }

    fn act(
        world: &mut World,
        user: Entity,
        console: Entity,
        act: RecordsAct,
    ) -> Result<(), &'static str> {
        apply(
            world,
            Entity::PLACEHOLDER,
            user,
            console,
            RecordsKind::Security,
            act,
        )
    }

    fn record(world: &World, id: u32) -> super::super::manifest::CrewRecord {
        world.resource::<Manifest>().get(id).unwrap().clone()
    }

    /// `insert_new_record`'s record keeps tg's constructor defaults and the photo's picture,
    /// and `view_record` makes that picture the viewer's preview (only once logged in).
    #[test]
    fn photo_records_and_previews() {
        let (mut world, console, _) = setup();
        let picture = std::sync::Arc::new(crate::service::Picture {
            width: 1,
            height: 1,
            ..default()
        });
        let id = world
            .resource_mut::<Manifest>()
            .add_from_photo("Незнакомец", picture.clone());
        let r = record(&world, id);
        assert_eq!((r.age, r.rank.as_str()), (18, "Unassigned"));
        assert_eq!(r.wanted_status, WANTED_NONE);
        let user = officer(&mut world, "Офицер", &["security"]);
        assert!(act(&mut world, user, console, RecordsAct::View(id)).is_err());
        act(&mut world, user, console, RecordsAct::Login).unwrap();
        act(&mut world, user, console, RecordsAct::View(id)).unwrap();
        let previews = world.get::<RecordPreviews>(console).unwrap();
        assert_eq!(previews.0.get(&user).map(|(r, _)| *r), Some(id));
        assert!(act(&mut world, user, console, RecordsAct::View(9999)).is_err());
    }

    #[test]
    fn security_edits_need_a_login_and_follow_tg() {
        let (mut world, console, id) = setup();
        let clown = officer(&mut world, "Клоун", &["theatre"]);
        let alice = officer(&mut world, "Алиса", &["security"]);
        let warden = officer(&mut world, "Варден", &["security", "armory"]);
        let crime = |name: &str| RecordsAct::AddCrime {
            record: id,
            name: name.into(),
            details: String::new(),
            fine: 0,
        };
        // Nothing before a login; a card without security access cannot log in.
        assert!(act(&mut world, alice, console, crime("x")).is_err());
        act(&mut world, clown, console, RecordsAct::Login).unwrap();
        assert!(!world.get::<RecordsConsole>(console).unwrap().authenticated);
        act(&mut world, alice, console, RecordsAct::Login).unwrap();
        assert!(world.get::<RecordsConsole>(console).unwrap().authenticated);

        // Arrest needs a crime; a crime sets Arrest; a fine is a citation.
        let arrest = RecordsAct::SetWanted(id, WANTED_ARREST.into());
        assert!(act(&mut world, alice, console, arrest.clone()).is_err());
        assert!(
            act(
                &mut world,
                alice,
                console,
                RecordsAct::SetWanted(id, "Обыск".into())
            )
            .is_err()
        );
        act(
            &mut world,
            alice,
            console,
            crime("Кража <b>со взломом</b> и ещё много слов"),
        )
        .unwrap();
        let r = record(&world, id);
        assert_eq!(r.wanted_status, WANTED_ARREST);
        assert_eq!(r.crimes[0].name, "Кража bсо взломом/b и ещ");
        assert_eq!(r.crimes[0].author, "Алиса");
        assert_eq!(r.crimes[0].details, "Детали не указаны.");
        let fine = |fine| RecordsAct::AddCrime {
            record: id,
            name: "Мусор".into(),
            details: "На полу".into(),
            fine,
        };
        assert!(act(&mut world, alice, console, fine(MAX_FINE + 1)).is_err());
        act(&mut world, alice, console, fine(100)).unwrap();
        assert_eq!(record(&world, id).citations[0].fine, Some(100));

        // Only the author or armory access voids a crime; the last valid one discharges.
        let crime_id = record(&world, id).crimes[0].id;
        let void = RecordsAct::InvalidateCrime {
            record: id,
            crime: crime_id,
        };
        assert!(act(&mut world, clown, console, void.clone()).is_err());
        act(&mut world, warden, console, void).unwrap();
        let r = record(&world, id);
        assert!(!r.crimes[0].valid);
        assert_eq!(r.crimes[0].voider.as_deref(), Some("Варден"));
        assert_eq!(r.wanted_status, WANTED_DISCHARGED);

        // Notes and fields; a medical act is no security act.
        act(
            &mut world,
            alice,
            console,
            RecordsAct::SetNote(id, "Опасен".into()),
        )
        .unwrap();
        act(
            &mut world,
            alice,
            console,
            RecordsAct::EditField {
                record: id,
                field: "age".into(),
                value: "40".into(),
            },
        )
        .unwrap();
        assert!(
            act(
                &mut world,
                alice,
                console,
                RecordsAct::EditField {
                    record: id,
                    field: "wanted_status".into(),
                    value: "Нет".into(),
                },
            )
            .is_err()
        );
        assert!(
            act(
                &mut world,
                alice,
                console,
                RecordsAct::AddNote(id, "x".into())
            )
            .is_err()
        );
        let r = record(&world, id);
        assert_eq!((r.security_note.as_str(), r.age), ("Опасен", 40));

        // Purging needs armory access; logging out locks the console again.
        assert!(act(&mut world, alice, console, RecordsAct::Purge).is_err());
        // Off the station's levels neither the purge nor an expunge reach the records.
        let wanted = record(&world, id).wanted_status;
        world
            .entity_mut(console)
            .insert(ss13_shared::TilePos::new(1, 1, 2));
        assert!(act(&mut world, warden, console, RecordsAct::Purge).is_err());
        assert!(act(&mut world, warden, console, RecordsAct::Expunge(id)).is_err());
        assert_eq!(record(&world, id).wanted_status, wanted);
        assert_ne!(wanted, WANTED_NONE);
        world
            .entity_mut(console)
            .insert(ss13_shared::TilePos::new(1, 1, 1));
        act(&mut world, warden, console, RecordsAct::Purge).unwrap();
        assert_eq!(record(&world, id).wanted_status, WANTED_NONE);
        act(&mut world, alice, console, RecordsAct::Logout).unwrap();
        assert!(
            act(
                &mut world,
                alice,
                console,
                RecordsAct::SetNote(id, "y".into())
            )
            .is_err()
        );
    }

    #[test]
    fn a_strong_emp_scrambles_or_deletes_records() {
        let (mut world, _, _) = setup();
        let mut manifest = world.resource_mut::<Manifest>();
        for n in 0..200 {
            manifest.inject(Joiner {
                name: &format!("Член экипажа {n}"),
                assignment: "Ассистент",
                job: None,
                mob: Entity::PLACEHOLDER,
            });
        }
        let before = manifest.general.clone();
        world.insert_resource(crate::emp::EmpRng(ss13_mapload::spawner::Rng::new(3)));
        scramble_records(&mut world, 1);
        let after = &world.resource::<Manifest>().general;
        // About 10 % of 201 changed, about 1 % of the rest gone; ages stay in range.
        let changed = after
            .iter()
            .filter(|r| before.iter().find(|b| b.id == r.id) != Some(*r))
            .count();
        assert!((5..=40).contains(&changed), "{changed}");
        assert!(after.len() <= before.len());
        assert!(
            after
                .iter()
                .all(|r| (5..=85).contains(&r.age) || r.age == 30)
        );
    }

    #[test]
    fn medical_notes_keep_three_and_statuses_are_checked() {
        let (mut world, _, id) = setup();
        let console = world
            .spawn(RecordsConsole::of_path(MEDICAL_CONSOLE).unwrap())
            .id();
        let doc = officer(&mut world, "Док", &["medical"]);
        let med = |w: &mut World, a| {
            apply(
                w,
                Entity::PLACEHOLDER,
                doc,
                console,
                RecordsKind::Medical,
                a,
            )
        };
        med(&mut world, RecordsAct::Login).unwrap();
        for n in 0..4 {
            med(&mut world, RecordsAct::AddNote(id, format!("заметка {n}"))).unwrap();
        }
        let notes = record(&world, id).medical_notes;
        assert_eq!(notes.len(), 3);
        assert_eq!(notes[0].content, "заметка 1");
        assert!(med(&mut world, RecordsAct::SetPhysical(id, "Здоров".into())).is_err());
        med(
            &mut world,
            RecordsAct::SetPhysical(id, PHYSICAL_DECEASED.into()),
        )
        .unwrap();
        med(&mut world, RecordsAct::SetCauseOfDeath(id, "Удушье".into())).unwrap();
        med(
            &mut world,
            RecordsAct::SetPhysical(id, PHYSICAL_ACTIVE.into()),
        )
        .unwrap();
        assert!(record(&world, id).cause_of_death.is_empty());
        assert!(med(&mut world, RecordsAct::SetBloodType(id, "Z+".into())).is_err());
        med(&mut world, RecordsAct::SetBloodType(id, "AB-".into())).unwrap();
        assert!(
            med(
                &mut world,
                RecordsAct::AddCrime {
                    record: id,
                    name: "x".into(),
                    details: String::new(),
                    fine: 0
                }
            )
            .is_err()
        );
        // A security console takes no medical acts either way round.
        let (mut w2, sec, id2) = setup();
        assert!(
            apply(
                &mut w2,
                Entity::PLACEHOLDER,
                doc,
                sec,
                RecordsKind::Medical,
                RecordsAct::Login
            )
            .is_err()
        );
        let _ = id2;
    }
}
