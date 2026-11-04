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
    AdminRequest, AdminResponse, AntagRole, PlayerEntry, PlayerState, Smite, Ticket, TicketState,
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
                ui.listed = true;
                ui.rank.clone_from(rank);
                ui.rights = *rights;
            }
            AdminResponse::Players(players) => ui.players.clone_from(players),
            AdminResponse::Tickets(tickets) => ui.tickets.clone_from(tickets),
            AdminResponse::Prototypes { ids, .. } => ui.prototypes.clone_from(ids),
            AdminResponse::Events(names) => ui.events.clone_from(names),
            AdminResponse::Dynamic(lines) => ui.dynamic.clone_from(lines),
            AdminResponse::Done(text) => ui.last_reply = format!("ok: {text}"),
            AdminResponse::Refused(text) => ui.last_reply = format!("отказано: {text}"),
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
            });
            ui.separator();
            match state.tab {
                Tab::Players => players_tab(ui, state, &mut out),
                Tab::Tickets => tickets_tab(ui, state, &mut out),
                Tab::Game => game_tab(ui, state, &mut out),
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

fn game_tab(ui: &mut egui::Ui, state: &mut AdminUi, out: &mut Vec<AdminRequest>) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_enabled_ui(state.can(R_SPAWN), |ui| {
            ui.strong("Спавн (под собой)");
            ui.horizontal(|ui| {
                let field = ui.text_edit_singleline(&mut state.search);
                let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button("Найти").clicked() || enter {
                    out.push(AdminRequest::FindPrototypes(state.search.clone()));
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
}
