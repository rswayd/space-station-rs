//! Client half: the admin panel and the adminhelp window (egui, over the map), in every build.
//!
//! - **F1** (tg/BandaStation `adminhelp`): the player's ticket window: the current ticket's
//!   conversation and a line to write into it, or to open a new ticket.
//! - **F7** (BandaStation "Player Panel New"): the admin panel, only after the server confirmed
//!   admin rights ([`AdminResponse::Rights`]); its buttons follow those rights. The server checks
//!   every request again: the panel only saves admins from clicking what they cannot do.
//! - **F6** (BandaStation "Aghost"): admin ghost / back into the body.
//!
//! While a window is open egui takes the pointer over it and the keyboard while its text field
//! has focus (the same rules as the debug overlay). `SS13_ADMIN_UI=help,players,tickets,game`
//! opens those at start (scripted visual checks: egui gets no injected pointer input).

use bevy::input::keyboard::KeyboardInput;
use bevy::picking::PickingSystems;
use bevy::picking::pointer::PointerInput;
use bevy::prelude::*;
use bevy_egui::input::EguiWantsInput;
use bevy_egui::{
    EguiContexts, EguiGlobalSettings, EguiPlugin, EguiPreUpdateSet, EguiPrimaryContextPass, egui,
};
use ss13_net::admin::client::{AdminResponseReceived, SendAdminRequest};
use ss13_net::client::NetClientConfig;
use ss13_shared::admin::rights::*;
use ss13_shared::admin::{
    AdminEntry, AdminRequest, AdminResponse, AntagRole, MindAct, MindPanel, PlayerEntry,
    PlayerState, Smite, SoundScope, Ticket, TicketState, VvEntity, VvRef, VvValue,
};

/// The player list is asked for this often while it is shown (seconds).
const PLAYERS_POLL_SECS: f64 = 1.0;
/// `SSsecurity_level.available_levels` in BandaStation's order (`ss13_game::security_level`).
const SECURITY_LEVELS: [&str; 6] = ["green", "blue", "red", "gamma", "epsilon", "delta"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Players,
    Tickets,
    Game,
    /// tg "Permissions Panel" (`R_PERMISSIONS`).
    Permissions,
    /// tg "View Variables" (`R_DEBUG`, editing `R_VAREDIT`).
    Variables,
}

/// What the server told this client, and the panel's state.
#[derive(Resource, Debug)]
pub struct AdminUi {
    /// Listed as an admin (a `Rights` arrived), and the active rights.
    pub listed: bool,
    pub rank: String,
    pub rights: u32,
    pub panel: bool,
    pub tab: Tab,
    pub help: bool,
    pub players: Vec<PlayerEntry>,
    pub tickets: Vec<Ticket>,
    pub selected_ticket: Option<u32>,
    pub prototypes: Vec<String>,
    pub events: Vec<String>,
    pub dynamic: Vec<String>,
    /// The last [`AdminResponse::Mind`] (tg `traitor_panel`).
    pub mind: Option<MindPanel>,
    /// The permissions panel's last [`AdminResponse::AdminList`].
    pub admins: Vec<AdminEntry>,
    pub ranks: Vec<String>,
    /// The last [`AdminResponse::Ert`] and the ERT section's inputs.
    pub ert: Option<ss13_shared::admin::ErtPanel>,
    ert_form: ErtForm,
    /// [`AdminResponse::VvTile`] and the last [`AdminResponse::Variables`].
    pub vv_tile: Vec<VvRef>,
    pub vv: Option<VvEntity>,
    /// The field being edited: component, field, typed text.
    vv_edit: Option<(String, String, String)>,
    /// The last `Done`/`Refused`.
    pub last_reply: String,
    players_asked: f64,
    focus_help: bool,
    help_text: String,
    asay_text: String,
    reply_text: String,
    player_text: String,
    search: String,
    announce_title: String,
    announce_text: String,
    time_scale: f32,
    confirm_end: bool,
    emp_heavy: u8,
    emp_light: u8,
    new_admin: String,
    new_rank: String,
    sound_file: String,
    sound_volume: u8,
    gas_text: String,
    confirm_man_up: bool,
    ai_hostile: bool,
}

impl Default for AdminUi {
    fn default() -> Self {
        Self {
            listed: false,
            rank: String::new(),
            rights: 0,
            panel: false,
            tab: Tab::default(),
            help: false,
            players: Vec::new(),
            tickets: Vec::new(),
            selected_ticket: None,
            prototypes: Vec::new(),
            events: Vec::new(),
            dynamic: Vec::new(),
            mind: None,
            admins: Vec::new(),
            ranks: Vec::new(),
            vv_tile: Vec::new(),
            ert: None,
            ert_form: ErtForm::default(),
            vv: None,
            vv_edit: None,
            last_reply: String::new(),
            players_asked: f64::NEG_INFINITY,
            focus_help: false,
            help_text: String::new(),
            asay_text: String::new(),
            reply_text: String::new(),
            player_text: String::new(),
            search: String::new(),
            announce_title: String::new(),
            announce_text: String::new(),
            time_scale: 1.0,
            confirm_end: false,
            emp_heavy: 1,
            emp_light: 3,
            new_admin: String::new(),
            new_rank: String::new(),
            sound_file: String::new(),
            sound_volume: 100,
            gas_text: String::new(),
            confirm_man_up: false,
            ai_hostile: false,
        }
    }
}

impl AdminUi {
    /// `SS13_ADMIN_UI` (see the module docs).
    pub fn from_env() -> Self {
        let mut ui = Self::default();
        if let Ok(value) = std::env::var("SS13_ADMIN_UI") {
            for key in value.split(',').map(str::trim) {
                match key {
                    "help" => ui.help = true,
                    "players" => (ui.panel, ui.tab) = (true, Tab::Players),
                    "tickets" => (ui.panel, ui.tab) = (true, Tab::Tickets),
                    "game" => (ui.panel, ui.tab) = (true, Tab::Game),
                    _ => {}
                }
            }
        }
        ui
    }

    pub fn can(&self, required: u32) -> bool {
        self.rights != 0 && self.rights & required == required
    }

    /// The panel is drawn only for a client the server confirmed as an admin.
    fn panel_shown(&self) -> bool {
        self.panel && self.listed
    }

    fn any_open(&self) -> bool {
        self.help || self.panel_shown()
    }
}

pub struct AdminUiPlugin;

impl Plugin for AdminUiPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<EguiPlugin>() {
            app.insert_resource(EguiGlobalSettings {
                auto_create_primary_context: false,
                enable_absorb_bevy_input_system: false,
                ..default()
            })
            .add_plugins(EguiPlugin::default());
        }
        app.insert_resource(AdminUi::from_env())
            .add_observer(egui_on_ui_camera)
            .add_systems(
                PreUpdate,
                (
                    pointer.after(PickingSystems::Last),
                    absorb_keyboard.after(EguiPreUpdateSet::ProcessInput),
                )
                    .run_if(|ui: Res<AdminUi>| ui.any_open()),
            )
            .add_systems(Update, (hotkeys, collect, poll).chain())
            .add_systems(
                EguiPrimaryContextPass,
                (help_window, admin_panel)
                    .chain()
                    .run_if(|ui: Res<AdminUi>| ui.any_open()),
            );
    }
}

/// egui draws in its camera's pass: on the world camera (order 0) the UI camera's `bevy_ui` (order 1,
/// the full-window lobby background among it) would cover it.
fn egui_on_ui_camera(add: On<Add<IsDefaultUiCamera>>, mut commands: Commands) {
    commands
        .entity(add.entity)
        .insert(bevy_egui::PrimaryEguiContext);
    // Offscreen scripted runs (`SS13_OFFSCREEN=1`, `ss13_devtools`): the camera draws into an
    // image, so no window routes keys to its context; the script's typing goes to it this way.
    if std::env::var("SS13_OFFSCREEN").is_ok_and(|v| v == "1") {
        commands.insert_resource(bevy_egui::input::FocusedNonWindowEguiContext(add.entity));
    }
}

fn hotkeys(
    keys: Res<ButtonInput<KeyCode>>,
    mut ui: ResMut<AdminUi>,
    mut requests: MessageWriter<SendAdminRequest>,
) {
    if keys.just_pressed(KeyCode::F1) {
        ui.help = !ui.help;
        ui.focus_help = ui.help;
    }
    if keys.just_pressed(KeyCode::F7) && ui.listed {
        ui.panel = !ui.panel;
    }
    if keys.just_pressed(KeyCode::F6) && ui.can(R_ADMIN) {
        requests.write(SendAdminRequest(AdminRequest::Aghost));
    }
}

fn collect(mut responses: MessageReader<AdminResponseReceived>, mut ui: ResMut<AdminUi>) {
    for AdminResponseReceived(response) in responses.read() {
        match response {
            AdminResponse::Rights { rank, rights } => {
                // An empty rank: removed from the admins (the permissions panel).
                ui.listed = !rank.is_empty();
                ui.rank.clone_from(rank);
                ui.rights = *rights;
            }
            AdminResponse::Players(players) => ui.players.clone_from(players),
            AdminResponse::Tickets(tickets) => ui.tickets.clone_from(tickets),
            AdminResponse::Prototypes { ids, .. } => ui.prototypes.clone_from(ids),
            AdminResponse::Events(names) => ui.events.clone_from(names),
            AdminResponse::Dynamic(lines) => ui.dynamic.clone_from(lines),
            AdminResponse::Mind(panel) => ui.mind = Some(panel.clone()),
            AdminResponse::Done(text) => ui.last_reply = format!("ok: {text}"),
            AdminResponse::Refused(text) => ui.last_reply = format!("отказано: {text}"),
            AdminResponse::AdminList { admins, ranks } => {
                ui.admins.clone_from(admins);
                ui.ranks.clone_from(ranks);
            }
            AdminResponse::Ert(panel) => ui.ert = Some(panel.clone()),
            AdminResponse::VvTile(refs) => ui.vv_tile.clone_from(refs),
            AdminResponse::Variables(entity) => {
                ui.vv = Some(entity.clone());
                ui.vv_edit = None;
            }
        }
    }
}

fn poll(
    mut ui: ResMut<AdminUi>,
    time: Res<Time<Real>>,
    mut requests: MessageWriter<SendAdminRequest>,
) {
    let now = time.elapsed_secs_f64();
    if ui.panel_shown()
        && ui.tab == Tab::Players
        && ui.can(R_ADMIN)
        && now - ui.players_asked >= PLAYERS_POLL_SECS
    {
        ui.players_asked = now;
        requests.write(SendAdminRequest(AdminRequest::Players));
    }
}

/// egui takes the pointer over its windows: the world under them gets no clicks.
fn pointer(mut input: ResMut<Messages<PointerInput>>, wants: Res<EguiWantsInput>) {
    if wants.wants_any_pointer_input() {
        input.clear();
    }
}

/// While an egui text field has focus the game sees no keys (F1 and F7 still toggle).
fn absorb_keyboard(
    mut contexts: EguiContexts,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut messages: ResMut<Messages<KeyboardInput>>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if !ctx.text_edit_focused() {
        return;
    }
    let kept: Vec<KeyCode> = [KeyCode::F1, KeyCode::F7, KeyCode::F9]
        .into_iter()
        .filter(|k| keys.just_pressed(*k))
        .collect();
    keys.reset_all();
    messages.clear();
    for key in kept {
        keys.press(key);
    }
}

fn ticket_state(state: TicketState) -> &'static str {
    match state {
        TicketState::Open => "открыт",
        TicketState::Closed => "закрыт",
        TicketState::Resolved => "решён",
    }
}

fn messages(ui: &mut egui::Ui, ticket: &Ticket, height: f32) {
    egui::ScrollArea::vertical()
        .max_height(height)
        .stick_to_bottom(true)
        .id_salt(("ticket", ticket.id))
        .show(ui, |ui| {
            for m in &ticket.messages {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!(
                        "{:02}:{:02}:{:02}",
                        m.time / 3600,
                        m.time / 60 % 60,
                        m.time % 60
                    ));
                    ui.strong(format!("{}:", m.sender));
                    ui.label(&m.text);
                });
            }
        });
}

/// BandaStation `TicketCreation` / the player's view of `TicketManager`.
fn help_window(
    mut contexts: EguiContexts,
    config: Res<NetClientConfig>,
    mut ui_state: ResMut<AdminUi>,
    mut requests: MessageWriter<SendAdminRequest>,
) -> Result {
    if !ui_state.help {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let state = &mut *ui_state;
    let mut open = true;
    // An admin's list has everyone's tickets: the newest one of this client's ckey.
    let own = crate::rights::ckey(&config.token);
    let mine = state
        .tickets
        .iter()
        .rev()
        .find(|t| t.initiator == own)
        .cloned();
    egui::Window::new("Админ-помощь")
        .open(&mut open)
        .default_pos([340.0, 200.0])
        .default_size([520.0, 360.0])
        .show(ctx, |ui| {
            let active = mine.as_ref().filter(|t| t.state == TicketState::Open);
            match &mine {
                Some(t) => {
                    ui.strong(format!("Тикет #{} ({})", t.id, ticket_state(t.state)));
                    messages(ui, t, 220.0);
                }
                None => {
                    ui.label("Опишите проблему: администраторы увидят тикет.");
                }
            }
            ui.separator();
            let hint = if active.is_some() {
                "Сообщение в тикет"
            } else {
                "Новый тикет"
            };
            let field = ui.add(
                egui::TextEdit::singleline(&mut state.help_text)
                    .hint_text(hint)
                    .desired_width(f32::INFINITY),
            );
            if state.focus_help {
                field.request_focus();
                state.focus_help = false;
            }
            let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let label = if active.is_some() {
                "Отправить"
            } else {
                "Создать тикет"
            };
            if (ui.button(label).clicked() || enter) && !state.help_text.trim().is_empty() {
                let text = std::mem::take(&mut state.help_text);
                requests.write(SendAdminRequest(AdminRequest::Adminhelp(text)));
            }
            if !state.last_reply.is_empty() && !state.listed {
                ui.small(&state.last_reply);
            }
        });
    ui_state.help = open;
    Ok(())
}

fn admin_panel(
    mut contexts: EguiContexts,
    mut ui_state: ResMut<AdminUi>,
    mut requests: MessageWriter<SendAdminRequest>,
) -> Result {
    if !ui_state.panel_shown() {
        return Ok(());
    }
    let ctx = contexts.ctx_mut()?;
    let state = &mut *ui_state;
    let mut open = true;
    let mut out: Vec<AdminRequest> = Vec::new();
    egui::Window::new("Админ-панель")
        .open(&mut open)
        .default_pos([8.0, 8.0])
        .default_size([760.0, 520.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{} ({})",
                    state.rank,
                    crate::rights::rights_text(state.rights)
                ));
                if state.rights == 0 {
                    if ui.button("Readmin").clicked() {
                        out.push(AdminRequest::Readmin);
                    }
                } else if ui.button("Deadmin").clicked() {
                    out.push(AdminRequest::Deadmin);
                }
                if ui
                    .add_enabled(state.can(R_ADMIN), egui::Button::new("Aghost (F6)"))
                    .clicked()
                {
                    out.push(AdminRequest::Aghost);
                }
            });
            ui.add_enabled_ui(state.can(R_ADMIN), |ui| {
                ui.horizontal(|ui| {
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut state.asay_text)
                            .hint_text("asay")
                            .desired_width(560.0),
                    );
                    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if (ui.button("ASay").clicked() || enter) && !state.asay_text.is_empty() {
                        out.push(AdminRequest::Asay(std::mem::take(&mut state.asay_text)));
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.selectable_value(&mut state.tab, Tab::Players, "Игроки");
                let open_tickets = state
                    .tickets
                    .iter()
                    .filter(|t| t.state == TicketState::Open)
                    .count();
                ui.selectable_value(
                    &mut state.tab,
                    Tab::Tickets,
                    format!("Тикеты ({open_tickets})"),
                );
                ui.selectable_value(&mut state.tab, Tab::Game, "Игра");
                if state.can(R_PERMISSIONS)
                    && ui
                        .selectable_value(&mut state.tab, Tab::Permissions, "Права")
                        .clicked()
                {
                    out.push(AdminRequest::AdminList);
                }
                if state.can(R_DEBUG)
                    && ui
                        .selectable_value(&mut state.tab, Tab::Variables, "Переменные")
                        .clicked()
                {
                    out.push(AdminRequest::VvTile);
                }
            });
            ui.separator();
            match state.tab {
                Tab::Players => players_tab(ui, state, &mut out),
                Tab::Tickets => tickets_tab(ui, state, &mut out),
                Tab::Game => game_tab(ui, state, &mut out),
                Tab::Permissions => permissions_tab(ui, state, &mut out),
                Tab::Variables => variables_tab(ui, state, &mut out),
            }
            if !state.last_reply.is_empty() {
                ui.separator();
                ui.small(&state.last_reply);
            }
        });
    ui_state.panel = open;
    requests.write_batch(out.into_iter().map(SendAdminRequest));
    Ok(())
}

fn players_tab(ui: &mut egui::Ui, state: &mut AdminUi, out: &mut Vec<AdminRequest>) {
    ui.horizontal(|ui| {
        ui.label("Сообщение / причина:");
        ui.add(egui::TextEdit::singleline(&mut state.player_text).desired_width(420.0));
    });
    let text = state.player_text.clone();
    let can = |r| state.can(r);
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("admin players")
            .striped(true)
            .show(ui, |ui| {
                for h in ["ckey", "имя", "должность", "антаг", "статус", "где", ""]
                {
                    ui.strong(h);
                }
                ui.end_row();
                for p in &state.players {
                    let ckey = p.ckey.clone();
                    ui.label(if p.rank.is_empty() {
                        ckey.clone()
                    } else {
                        format!("{ckey} [{}]", p.rank)
                    });
                    ui.label(&p.name);
                    ui.label(&p.job);
                    ui.label(p.antag.join(", "));
                    ui.label(match p.state {
                        PlayerState::Lobby => "лобби",
                        PlayerState::Alive => "жив",
                        PlayerState::Dead => "мёртв",
                        PlayerState::Ghost => "призрак",
                    });
                    ui.label(
                        p.pos
                            .map(|t| format!("{},{},{}", t.x, t.y, t.z))
                            .unwrap_or_default(),
                    );
                    ui.horizontal(|ui| {
                        let mut button = |ui: &mut egui::Ui, label: &str, rights, request| {
                            if ui
                                .add_enabled(can(rights), egui::Button::new(label))
                                .clicked()
                            {
                                out.push(request);
                            }
                        };
                        button(ui, "JMP", R_ADMIN, AdminRequest::JumpTo(ckey.clone()));
                        button(ui, "GET", R_ADMIN, AdminRequest::Get(ckey.clone()));
                        button(ui, "FLW", R_ADMIN, AdminRequest::Follow(ckey.clone()));
                        if !text.is_empty() {
                            button(
                                ui,
                                "PM",
                                R_ADMIN,
                                AdminRequest::Pm {
                                    ckey: ckey.clone(),
                                    text: text.clone(),
                                },
                            );
                        }
                        button(
                            ui,
                            if p.muted { "Unmute" } else { "Mute" },
                            R_ADMIN,
                            AdminRequest::Mute {
                                ckey: ckey.clone(),
                                mute: !p.muted,
                            },
                        );
                        button(
                            ui,
                            "Kick",
                            R_ADMIN,
                            AdminRequest::Kick {
                                ckey: ckey.clone(),
                                reason: text.clone(),
                            },
                        );
                        button(
                            ui,
                            "Ban",
                            R_BAN,
                            AdminRequest::Ban {
                                ckey: ckey.clone(),
                                reason: text.clone(),
                            },
                        );
                        button(ui, "Heal", R_ADMIN, AdminRequest::Rejuvenate(ckey.clone()));
                        button(ui, "Mind", R_ADMIN, AdminRequest::MindPanel(ckey.clone()));
                        button(ui, "Man Up", R_ADMIN, AdminRequest::ManUp(ckey.clone()));
                        button(
                            ui,
                            "Smite",
                            R_ADMIN | R_FUN,
                            AdminRequest::Smite {
                                ckey: ckey.clone(),
                                smite: Smite::Lightning,
                            },
                        );
                        button(
                            ui,
                            "Traitor",
                            R_ADMIN,
                            AdminRequest::MakeAntag {
                                ckey: ckey.clone(),
                                role: AntagRole::Traitor,
                            },
                        );
                        button(
                            ui,
                            "Brother",
                            R_ADMIN,
                            AdminRequest::MakeAntag {
                                ckey: ckey.clone(),
                                role: AntagRole::Brother,
                            },
                        );
                        button(
                            ui,
                            "Changeling",
                            R_ADMIN,
                            AdminRequest::MakeAntag {
                                ckey: ckey.clone(),
                                role: AntagRole::Changeling,
                            },
                        );
                        button(
                            ui,
                            "Vampire",
                            R_ADMIN,
                            AdminRequest::MakeAntag {
                                ckey: ckey.clone(),
                                role: AntagRole::Vampire,
                            },
                        );
                        button(
                            ui,
                            "Cultist",
                            R_ADMIN,
                            AdminRequest::MakeAntag {
                                ckey: ckey.clone(),
                                role: AntagRole::Cultist,
                            },
                        );
                        button(
                            ui,
                            "Heretic",
                            R_ADMIN,
                            AdminRequest::MakeAntag {
                                ckey: ckey.clone(),
                                role: AntagRole::Heretic,
                            },
                        );
                        button(
                            ui,
                            "Head Rev",
                            R_ADMIN,
                            AdminRequest::MakeAntag {
                                ckey: ckey.clone(),
                                role: AntagRole::HeadRev,
                            },
                        );
                        button(
                            ui,
                            "Rev",
                            R_ADMIN,
                            AdminRequest::MakeAntag {
                                ckey: ckey.clone(),
                                role: AntagRole::Revolutionary,
                            },
                        );
                    });
                    ui.end_row();
                }
            });
    });
    mind_panel(ui, state, out);
}

/// tg `traitor_panel`: the mind of the player asked with "Mind", and its `Topic` buttons. The
/// job title and objective texts come from the players tab's text field.
fn mind_panel(ui: &mut egui::Ui, state: &AdminUi, out: &mut Vec<AdminRequest>) {
    let Some(panel) = &state.mind else {
        return;
    };
    let can = state.can(R_ADMIN);
    let text = state.player_text.trim().to_owned();
    let act = |act| AdminRequest::MindAct {
        ckey: panel.ckey.clone(),
        act,
    };
    ui.separator();
    ui.strong(format!("Разум {}", panel.ckey));
    for line in &panel.lines {
        ui.label(line);
    }
    ui.horizontal_wrapped(|ui| {
        for (role, name) in [
            (AntagRole::Traitor, "Traitor"),
            (AntagRole::Brother, "Blood Brother"),
            (AntagRole::Changeling, "Changeling"),
            (AntagRole::Vampire, "Vampire"),
            (AntagRole::Cultist, "Cultist"),
            (AntagRole::Heretic, "Heretic"),
            (AntagRole::HeadRev, "Head Revolutionary"),
            (AntagRole::Revolutionary, "Revolutionary"),
        ] {
            if panel.antag.iter().any(|a| a == name)
                && ui
                    .add_enabled(can, egui::Button::new(format!("Снять: {name}")))
                    .clicked()
            {
                out.push(act(MindAct::RemoveAntag(role)));
            }
        }
        if ui
            .add_enabled(can, egui::Button::new("Объявить цели"))
            .clicked()
        {
            out.push(act(MindAct::AnnounceObjectives));
        }
        let typed = can && !text.is_empty();
        if ui
            .add_enabled(typed, egui::Button::new("Должность = текст"))
            .clicked()
        {
            out.push(act(MindAct::SetJob(text.clone())));
        }
        if ui
            .add_enabled(typed, egui::Button::new("Добавить цель (текст)"))
            .clicked()
        {
            out.push(act(MindAct::AddObjective(text.clone())));
        }
    });
    for (i, objective) in panel.objectives.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("#{} {objective}", i + 1));
            if ui
                .add_enabled(
                    can && !text.is_empty(),
                    egui::Button::new("Заменить текстом"),
                )
                .clicked()
            {
                out.push(act(MindAct::EditObjective {
                    index: i as u32,
                    text: text.clone(),
                }));
            }
            if ui.add_enabled(can, egui::Button::new("Удалить")).clicked() {
                out.push(act(MindAct::RemoveObjective(i as u32)));
            }
        });
    }
}

fn tickets_tab(ui: &mut egui::Ui, state: &mut AdminUi, out: &mut Vec<AdminRequest>) {
    if state.selected_ticket.is_none() {
        state.selected_ticket = state
            .tickets
            .iter()
            .rev()
            .find(|t| t.state == TicketState::Open)
            .map(|t| t.id);
    }
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(220.0);
            egui::ScrollArea::vertical()
                .id_salt("ticket list")
                .max_height(360.0)
                .show(ui, |ui| {
                    for t in state.tickets.iter().rev() {
                        let label = format!(
                            "#{} {} ({}){}",
                            t.id,
                            t.initiator,
                            ticket_state(t.state),
                            if t.pm { " PM" } else { "" }
                        );
                        if ui
                            .selectable_label(state.selected_ticket == Some(t.id), label)
                            .clicked()
                        {
                            state.selected_ticket = Some(t.id);
                        }
                    }
                });
        });
        ui.separator();
        ui.vertical(|ui| {
            let Some(ticket) = state
                .selected_ticket
                .and_then(|id| state.tickets.iter().find(|t| t.id == id))
                .cloned()
            else {
                ui.label("Нет тикетов.");
                return;
            };
            ui.strong(format!(
                "Тикет #{} от {} ({}), админ: {}",
                ticket.id,
                ticket.initiator,
                ticket_state(ticket.state),
                ticket.linked_admin.as_deref().unwrap_or("нет")
            ));
            messages(ui, &ticket, 260.0);
            ui.horizontal(|ui| {
                let field = ui.add(
                    egui::TextEdit::singleline(&mut state.reply_text)
                        .hint_text("Ответ")
                        .desired_width(360.0),
                );
                let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (ui.button("Ответить").clicked() || enter) && !state.reply_text.is_empty()
                {
                    out.push(AdminRequest::TicketReply {
                        ticket: ticket.id,
                        text: std::mem::take(&mut state.reply_text),
                    });
                }
            });
            ui.horizontal(|ui| {
                let mut set = |label: &str, to: TicketState| {
                    if ui
                        .add_enabled(ticket.state != to, egui::Button::new(label))
                        .clicked()
                    {
                        out.push(AdminRequest::SetTicketState {
                            ticket: ticket.id,
                            state: to,
                        });
                    }
                };
                set("Закрыть", TicketState::Closed);
                set("Решить", TicketState::Resolved);
                set("Переоткрыть", TicketState::Open);
            });
        });
    });
}

/// What the admin types in the ERT section (tg's settings menu, BandaStation's manager).
#[derive(Debug)]
struct ErtForm {
    template: String,
    team_size: u8,
    mission: String,
    poll_desc: String,
    enforce_human: bool,
    random_names: bool,
    ert_type: String,
    commander: bool,
    slots: ss13_shared::admin::ErtSlots,
    announce: bool,
    reason: String,
}

impl Default for ErtForm {
    fn default() -> Self {
        Self {
            template: "/datum/ert/amber".to_owned(),
            team_size: 5,
            mission: String::new(),
            poll_desc: String::new(),
            enforce_human: true,
            random_names: true,
            ert_type: "Red".to_owned(),
            commander: true,
            slots: ss13_shared::admin::ErtSlots::default(),
            announce: true,
            reason: String::new(),
        }
    }
}

/// tg "Summon ERT" and BandaStation's ERT Manager; the server checks the rights and the values.
fn ert_section(ui: &mut egui::Ui, state: &mut AdminUi, out: &mut Vec<AdminRequest>) {
    use ss13_shared::admin::ErtSummon as Summon;
    const TYPES: [&str; 4] = ["Amber", "Red", "Gamma", "Inquisition"];
    let can_fun = state.can(R_FUN);
    let can_admin = state.can(R_ADMIN);
    let form = &mut state.ert_form;
    ui.horizontal(|ui| {
        ui.strong("ОБР");
        if ui.small_button("Обновить").clicked() {
            out.push(AdminRequest::ErtPanel);
        }
    });
    if let Some(panel) = &state.ert {
        ui.label(format!(
            "Уровень: {}, запрос отвечен: {}, точек появления: {}",
            panel.security_level,
            if panel.request_answered {
                "да"
            } else {
                "нет"
            },
            panel.spawnpoints
        ));
        for line in panel.requests.iter().take(5) {
            ui.small(line.as_str());
        }
    }
    ui.add_enabled_ui(can_fun, |ui| {
        // tg "Summon ERT".
        ui.horizontal_wrapped(|ui| {
            ui.label("Шаблон");
            ui.add(egui::TextEdit::singleline(&mut form.template).desired_width(180.0));
            if let Some(panel) = &state.ert {
                egui::ComboBox::from_id_salt("ert_templates")
                    .selected_text("выбрать")
                    .show_ui(ui, |ui| {
                        for (path, label) in &panel.templates {
                            ui.selectable_value(&mut form.template, path.clone(), label.as_str());
                        }
                    });
            }
            ui.label("Размер");
            ui.add(egui::DragValue::new(&mut form.team_size).range(1..=30));
            ui.checkbox(&mut form.enforce_human, "только люди");
            ui.checkbox(&mut form.random_names, "случайные имена");
        });
        ui.horizontal(|ui| {
            ui.label("Миссия");
            ui.text_edit_singleline(&mut form.mission);
            ui.label("Опрос");
            ui.text_edit_singleline(&mut form.poll_desc);
        });
        if ui.small_button("Вызвать ОБР").clicked() {
            out.push(AdminRequest::ErtSummon(Summon {
                template: form.template.clone(),
                team_size: form.team_size,
                mission: form.mission.clone(),
                poll_desc: form.poll_desc.clone(),
                enforce_human: form.enforce_human,
                random_names: form.random_names,
            }));
        }
    });
    ui.add_enabled_ui(can_admin, |ui| {
        // BandaStation's ERT Manager.
        ui.horizontal_wrapped(|ui| {
            ui.label("Менеджер:");
            egui::ComboBox::from_id_salt("ert_manager_type")
                .selected_text(form.ert_type.clone())
                .show_ui(ui, |ui| {
                    for name in TYPES {
                        ui.selectable_value(&mut form.ert_type, name.to_owned(), name);
                    }
                });
            ui.checkbox(&mut form.commander, "командир");
            ui.checkbox(&mut form.announce, "объявить");
        });
        ui.horizontal_wrapped(|ui| {
            for (label, value) in [
                ("СБ", &mut form.slots.security),
                ("Мед", &mut form.slots.medical),
                ("Инж", &mut form.slots.engineering),
                ("Уборщики", &mut form.slots.janitor),
                ("Капелланы", &mut form.slots.chaplain),
                ("Клоуны", &mut form.slots.clown),
            ] {
                ui.label(label);
                ui.add(egui::DragValue::new(value).range(0..=30));
            }
        });
        ui.horizontal(|ui| {
            if ui.small_button("Отправить").clicked() {
                out.push(AdminRequest::ErtDispatch {
                    ert_type: form.ert_type.clone(),
                    commander: form.commander,
                    slots: form.slots,
                    announce: form.announce,
                });
            }
            if ui.small_button("Запрос отвечен/не отвечен").clicked() {
                out.push(AdminRequest::ErtToggleAnswered);
            }
            ui.text_edit_singleline(&mut form.reason);
            if ui.small_button("Отклонить запрос").clicked() {
                out.push(AdminRequest::ErtDeny(form.reason.clone()));
            }
        });
    });
    ui.separator();
}

fn game_tab(ui: &mut egui::Ui, state: &mut AdminUi, out: &mut Vec<AdminRequest>) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ert_section(ui, state, out);
        ui.add_enabled_ui(state.can(R_SPAWN), |ui| {
            ui.strong("Спавн (под собой)");
            ui.horizontal(|ui| {
                let field = ui.text_edit_singleline(&mut state.search);
                let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button("Найти").clicked() || enter {
                    out.push(AdminRequest::FindPrototypes(state.search.clone()));
                }
                // tg subtype vendor: the prototype typed in full and everything inheriting it.
                if ui.button("Подтипы").clicked() {
                    out.push(AdminRequest::Subtypes(state.search.trim().to_owned()));
                }
            });
            egui::ScrollArea::vertical()
                .id_salt("prototypes")
                .max_height(120.0)
                .show(ui, |ui| {
                    for path in &state.prototypes {
                        if ui.small_button(path).clicked() {
                            out.push(AdminRequest::Spawn(path.clone()));
                        }
                    }
                });
        });
        ui.separator();
        ui.add_enabled_ui(state.can(R_FUN), |ui| {
            ui.horizontal(|ui| {
                ui.strong("События");
                if ui.button("Обновить").clicked() {
                    out.push(AdminRequest::Events);
                }
            });
            ui.horizontal_wrapped(|ui| {
                for name in &state.events {
                    if ui.small_button(name).clicked() {
                        out.push(AdminRequest::ForceEvent {
                            name: name.clone(),
                            announce: None,
                        });
                    }
                }
            });
        });
        ui.separator();
        ui.add_enabled_ui(state.can(R_FUN), |ui| {
            // tg "Give AI Controller" on the living mobs of the own tile.
            ui.horizontal_wrapped(|ui| {
                ui.strong("ИИ на тайле");
                ui.checkbox(&mut state.ai_hostile, "враждебный");
                for name in ss13_shared::admin::AI_TEMPLATES {
                    if ui.small_button(*name).clicked() {
                        out.push(AdminRequest::GiveAi {
                            template: (*name).to_owned(),
                            make_hostile: state.ai_hostile,
                        });
                    }
                }
            });
        });
        ui.separator();
        ui.add_enabled_ui(state.can(R_FUN), |ui| {
            // tg "Drop Bomb": on the own tile, ignoring the bomb cap.
            use ss13_shared::admin::BombSize;
            ui.horizontal_wrapped(|ui| {
                ui.strong("Бомба под собой");
                for (label, size) in [
                    ("Малая (1, 2, 3)", BombSize::Small),
                    ("Средняя (2, 3, 4)", BombSize::Medium),
                    ("Большая (3, 5, 7)", BombSize::Big),
                    ("Максимальная", BombSize::Maxcap),
                ] {
                    if ui.small_button(label).clicked() {
                        out.push(AdminRequest::DropBomb(size));
                    }
                }
            });
        });
        ui.separator();
        ui.add_enabled_ui(state.can(R_ADMIN | R_FUN), |ui| {
            // tg "EM Pulse": on the own tile.
            ui.horizontal_wrapped(|ui| {
                ui.strong("ЭМИ под собой");
                ui.label("сильный");
                ui.add(egui::DragValue::new(&mut state.emp_heavy).range(0..=40));
                ui.label("слабый");
                ui.add(egui::DragValue::new(&mut state.emp_light).range(0..=40));
                if ui.small_button("Импульс").clicked() {
                    out.push(AdminRequest::Emp {
                        heavy: state.emp_heavy,
                        light: state.emp_light,
                    });
                }
            });
        });
        ui.separator();
        ui.add_enabled_ui(state.can(R_ADMIN), |ui| {
            // tg "Set Security Level" (`SSsecurity_level.available_levels`).
            ui.horizontal_wrapped(|ui| {
                ui.strong("Уровень угрозы");
                for level in SECURITY_LEVELS {
                    if ui.small_button(level).clicked() {
                        out.push(AdminRequest::SetSecurityLevel(level.to_owned()));
                    }
                }
            });
        });
        ui.separator();
        ui.add_enabled_ui(state.can(R_ADMIN), |ui| {
            // BandaStation "Global Man Up" (asks for the confirmation).
            ui.horizontal(|ui| {
                ui.checkbox(&mut state.confirm_man_up, "точно");
                if ui
                    .add_enabled(state.confirm_man_up, egui::Button::new("Global Man Up"))
                    .clicked()
                {
                    state.confirm_man_up = false;
                    out.push(AdminRequest::GlobalManUp);
                }
            });
        });
        ui.add_enabled_ui(state.can(R_SOUND), |ui| {
            // BandaStation "Play Z-level Sound" / "Play Sound in View".
            ui.horizontal_wrapped(|ui| {
                ui.strong("Звук");
                ui.add(
                    egui::TextEdit::singleline(&mut state.sound_file)
                        .hint_text("sound/….ogg")
                        .desired_width(300.0),
                );
                ui.add(egui::DragValue::new(&mut state.sound_volume).range(1..=100));
                for (label, scope) in [
                    ("на z-уровне", SoundScope::ZLevel),
                    ("в поле зрения", SoundScope::View),
                ] {
                    if ui.small_button(label).clicked() && !state.sound_file.trim().is_empty() {
                        out.push(AdminRequest::PlaySound {
                            file: state.sound_file.trim().to_owned(),
                            volume: state.sound_volume,
                            scope,
                        });
                    }
                }
            });
        });
        ui.add_enabled_ui(state.can(R_DEBUG), |ui| {
            // tg "Debug Air Status" and `atmos_spawn_air` on the own tile.
            ui.horizontal_wrapped(|ui| {
                ui.strong("Атмос");
                if ui.small_button("Воздух под собой").clicked() {
                    out.push(AdminRequest::AirStatus);
                }
                ui.add(
                    egui::TextEdit::singleline(&mut state.gas_text)
                        .hint_text("o2=22;n2=82;TEMP=293.15")
                        .desired_width(240.0),
                );
                if ui.small_button("Выпустить газ").clicked() && !state.gas_text.trim().is_empty()
                {
                    out.push(AdminRequest::SpawnAir(state.gas_text.clone()));
                }
            });
        });
        ui.separator();
        ui.add_enabled_ui(state.can(R_ADMIN), |ui| {
            ui.strong("Объявление ЦК");
            ui.add(
                egui::TextEdit::singleline(&mut state.announce_title)
                    .hint_text("заголовок")
                    .desired_width(500.0),
            );
            ui.add(
                egui::TextEdit::multiline(&mut state.announce_text)
                    .hint_text("текст")
                    .desired_rows(2)
                    .desired_width(500.0),
            );
            if ui.button("Объявить").clicked() && !state.announce_text.trim().is_empty() {
                out.push(AdminRequest::Announce {
                    title: std::mem::take(&mut state.announce_title),
                    text: std::mem::take(&mut state.announce_text),
                });
            }
        });
        ui.separator();
        ui.add_enabled_ui(state.can(R_SERVER), |ui| {
            // tg "Start Now": the server refuses it once the round is setting up; pressed again
            // before that it cancels and counts down 3 minutes.
            if ui.button("Начать раунд сейчас").clicked() {
                out.push(AdminRequest::StartNow);
            }
            ui.horizontal(|ui| {
                ui.checkbox(&mut state.confirm_end, "точно");
                if ui
                    .add_enabled(state.confirm_end, egui::Button::new("Закончить раунд"))
                    .clicked()
                {
                    state.confirm_end = false;
                    out.push(AdminRequest::EndRound);
                }
                if ui
                    .add_enabled(state.confirm_end, egui::Button::new("Перезагрузить мир"))
                    .clicked()
                {
                    state.confirm_end = false;
                    out.push(AdminRequest::Restart);
                }
            });
        });
        ui.add_enabled_ui(state.can(R_DEBUG), |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut state.time_scale, 0.1..=10.0).logarithmic(true));
                if ui.button("Скорость времени").clicked() {
                    out.push(AdminRequest::TimeScale(state.time_scale));
                }
            });
        });
        ui.separator();
        ui.add_enabled_ui(state.can(R_ADMIN), |ui| {
            ui.horizontal(|ui| {
                ui.strong("Dynamic");
                if ui.button("Обновить").clicked() {
                    out.push(AdminRequest::Dynamic);
                }
            });
            // tg Dynamic Panel "Force midround" with the implemented midround rulesets
            // (`ss13_game::antag::midround` config tags; the server checks the tag).
            ui.horizontal(|ui| {
                ui.label("Мидраунд сейчас:");
                for tag in ["Midround Traitor", "Mass Traitors", "Midround Nukeops"] {
                    if ui.button(tag).clicked() {
                        out.push(AdminRequest::ForceMidround(tag.to_owned()));
                    }
                }
            });
            for line in &state.dynamic {
                ui.label(line);
            }
        });
    });
}

/// tg "Permissions Panel": the admins, a rank change or a new admin, and removal. Changes last
/// the round (no database yet); the server checks the edit flags and the target's rights.
/// The text of an editable value, `None` for [`VvValue::Other`].
fn vv_text(value: &VvValue) -> Option<String> {
    match value {
        VvValue::Bool(v) => Some(v.to_string()),
        VvValue::Int(v) => Some(v.to_string()),
        VvValue::Float(v) => Some(v.to_string()),
        VvValue::Text(v) => Some(v.clone()),
        VvValue::Other(_) => None,
    }
}

/// Typed text as a value of the same kind as `old`; the server checks it again.
fn vv_parse(old: &VvValue, text: &str) -> Option<VvValue> {
    let text = text.trim();
    Some(match old {
        VvValue::Bool(_) => VvValue::Bool(text.parse().ok()?),
        VvValue::Int(_) => VvValue::Int(text.parse().ok()?),
        VvValue::Float(_) => VvValue::Float(text.parse().ok()?),
        VvValue::Text(_) => VvValue::Text(text.to_owned()),
        VvValue::Other(_) => return None,
    })
}

fn variables_tab(ui: &mut egui::Ui, state: &mut AdminUi, out: &mut Vec<AdminRequest>) {
    ui.horizontal_wrapped(|ui| {
        if ui.button("Мой тайл").clicked() {
            out.push(AdminRequest::VvTile);
        }
        for entry in &state.vv_tile {
            if ui.small_button(&entry.name).clicked() {
                out.push(AdminRequest::ViewVariables(entry.id));
            }
        }
    });
    ui.separator();
    let Some(entity) = &state.vv else {
        ui.label("Выберите объект.");
        return;
    };
    let can_edit = state.can(R_VAREDIT);
    let id = entity.id;
    ui.horizontal(|ui| {
        ui.strong(entity.name.as_str());
        if ui.small_button("Обновить").clicked() {
            out.push(AdminRequest::ViewVariables(id));
        }
    });
    egui::ScrollArea::vertical().id_salt("vv").show(ui, |ui| {
        for component in &entity.components {
            if !component.reflected {
                ui.weak(component.name.as_str());
                continue;
            }
            egui::CollapsingHeader::new(&component.name)
                .id_salt(&component.name)
                .show(ui, |ui| {
                    for field in &component.fields {
                        ui.horizontal(|ui| {
                            ui.label(format!("{} =", field.name));
                            let editing = state
                                .vv_edit
                                .as_ref()
                                .is_some_and(|(c, f, _)| *c == component.name && *f == field.name);
                            let Some(shown) = vv_text(&field.value) else {
                                if let VvValue::Other(text) = &field.value {
                                    ui.weak(text.as_str());
                                }
                                return;
                            };
                            if editing && let Some((_, _, text)) = state.vv_edit.as_mut() {
                                ui.text_edit_singleline(text);
                                if ui.small_button("OK").clicked()
                                    && let Some(value) = vv_parse(&field.value, text)
                                {
                                    out.push(AdminRequest::EditVariable {
                                        id,
                                        component: component.name.clone(),
                                        field: field.name.clone(),
                                        value,
                                    });
                                }
                            } else {
                                ui.monospace(shown.as_str());
                                if can_edit && ui.small_button("✎").clicked() {
                                    state.vv_edit =
                                        Some((component.name.clone(), field.name.clone(), shown));
                                }
                            }
                        });
                    }
                });
        }
    });
}

fn permissions_tab(ui: &mut egui::Ui, state: &mut AdminUi, out: &mut Vec<AdminRequest>) {
    ui.horizontal(|ui| {
        if ui.button("Обновить").clicked() {
            out.push(AdminRequest::AdminList);
        }
        ui.small("изменения действуют до конца раунда");
    });
    ui.horizontal_wrapped(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut state.new_admin)
                .hint_text("ckey")
                .desired_width(160.0),
        );
        ui.add(
            egui::TextEdit::singleline(&mut state.new_rank)
                .hint_text("Ранг или Ранг1+Ранг2")
                .desired_width(220.0),
        );
        if ui.button("Назначить").clicked()
            && !state.new_admin.trim().is_empty()
            && !state.new_rank.trim().is_empty()
        {
            out.push(AdminRequest::SetAdminRank {
                ckey: state.new_admin.trim().to_owned(),
                rank: state.new_rank.trim().to_owned(),
            });
            out.push(AdminRequest::AdminList);
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Ранги:");
        for rank in &state.ranks {
            if ui.small_button(rank).clicked() {
                state.new_rank.clone_from(rank);
            }
        }
    });
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("admin permissions")
            .striped(true)
            .show(ui, |ui| {
                for h in ["ckey", "ранг", "права", ""] {
                    ui.strong(h);
                }
                ui.end_row();
                for a in &state.admins {
                    ui.label(if a.online {
                        format!("{} (в сети)", a.ckey)
                    } else {
                        a.ckey.clone()
                    });
                    ui.label(&a.rank);
                    ui.small(crate::rights::rights_text(a.rights));
                    ui.horizontal(|ui| {
                        if ui.small_button("Сменить ранг").clicked() {
                            state.new_admin.clone_from(&a.ckey);
                        }
                        if ui.small_button("Убрать").clicked() {
                            out.push(AdminRequest::RemoveAdmin(a.ckey.clone()));
                            out.push(AdminRequest::AdminList);
                        }
                    });
                    ui.end_row();
                }
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_panel_needs_server_confirmed_rights() {
        let mut ui = AdminUi {
            panel: true,
            ..default()
        };
        assert!(!ui.panel_shown(), "no Rights from the server yet");
        ui.listed = true;
        ui.rights = R_ADMIN;
        assert!(ui.panel_shown());
        assert!(ui.can(R_ADMIN) && !ui.can(R_BAN));
        ui.rights = 0;
        assert!(!ui.can(0), "deadminned: nothing");
    }

    #[test]
    fn typed_variables_keep_the_field_kind() {
        assert_eq!(vv_parse(&VvValue::Int(1), " 42 "), Some(VvValue::Int(42)));
        assert_eq!(vv_parse(&VvValue::Int(1), "4.5"), None);
        assert_eq!(
            vv_parse(&VvValue::Bool(false), "true"),
            Some(VvValue::Bool(true))
        );
        assert_eq!(vv_parse(&VvValue::Other("[]".into()), "1"), None);
        assert_eq!(vv_text(&VvValue::Float(1.5)).as_deref(), Some("1.5"));
    }
}
