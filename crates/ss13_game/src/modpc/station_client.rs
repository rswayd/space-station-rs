//! Client screens of the station programs (`NtosPay`, `NtosRadar`, `NtosCyborgRemoteMonitor`,
//! `NtosRoboControl`, `NtosRevelation`, `NtosPhysScanner`, `NtosSpectreMeter`, `NtosRobotact`,
//! `SyndicateContractor`).
// shortcut: the radar draws the target as text (grid cell or bearing), not tgui's map with a
// pointer image; NT Pay takes the token and the amount in one input ("TOKEN AMOUNT", sent as
// `transfer`), since an input sends only its own text.

use bevy::scene::{EntityScene, SceneList};
use ss13_tgui::client::ActOnSubmit;
use ss13_ui_kit::ActParams;
use ss13_ui_kit::prelude::*;

use super::station_ui::*;

type Part = Box<dyn SceneList>;

fn button(
    content: String,
    icon: &'static str,
    action: &str,
    params: Vec<(String, String)>,
) -> Part {
    let action = action.to_owned();
    let params = ActParams(params);
    Box::new(EntityScene(bsn! {
        @ss13_ui_kit::Button { @content: content, @icon: icon }
        Act(action)
        params
    }))
}

fn line(text: String) -> Part {
    Box::new(EntityScene(bsn! { Node Children [ Text(text) ] }))
}

fn section(title: String, children: Vec<Part>) -> Part {
    Box::new(EntityScene(bsn! {
        @Section { @title: title, @fill: true, @scrollable: true }
        Children [ {children} ]
    }))
}

fn input(placeholder: String, action: &str, param: &str, params: Vec<(String, String)>) -> Part {
    let (action, param) = (action.to_owned(), param.to_owned());
    Box::new(EntityScene(bsn! {
        Input { placeholder: placeholder, max_length: {Some(1024)}, self_clear: true, fluid: true }
        ActOnSubmit { action: action, param: param, params: params }
    }))
}

pub fn screen(s: &StationScreen) -> Part {
    match s {
        StationScreen::Pay(d) => pay(d),
        StationScreen::Radar(d) => radar(d),
        StationScreen::Borgs(rows) => borgs(rows),
        StationScreen::Bots(rows) => bots(rows),
        StationScreen::Revelation { armed } => {
            let state = if *armed {
                "ВООРУЖЁН"
            } else {
                "не вооружён"
            };
            section(
                "Revelation".to_owned(),
                vec![
                    line(format!("Состояние: {state}")),
                    button(
                        if *armed {
                            "Разоружить"
                        } else {
                            "Вооружить"
                        }
                        .to_owned(),
                        "bomb",
                        REVELATION_ARM,
                        Vec::new(),
                    ),
                    button(
                        "АКТИВИРОВАТЬ".to_owned(),
                        "skull",
                        REVELATION_ACTIVATE,
                        Vec::new(),
                    ),
                ],
            )
        }
        StationScreen::PhysScanner { last_record } => {
            let mut lines: Vec<Part> = last_record.iter().cloned().map(line).collect();
            if lines.is_empty() {
                lines.push(line(
                    "Коснитесь КПК живого существа, чтобы просканировать его.".to_owned(),
                ));
            }
            section("Physical Scanner".to_owned(), lines)
        }
        StationScreen::SpectreMeter {
            spook_value,
            auto_mode,
            on_cooldown,
        } => {
            let mut parts = vec![line(format!(
                "Уровень спектральной активности: {spook_value}"
            ))];
            if !on_cooldown {
                parts.push(button(
                    "Сканировать".to_owned(),
                    "search",
                    SPECTRE_SCAN,
                    Vec::new(),
                ));
            }
            let mode = if *auto_mode {
                "Автоматический режим: вкл"
            } else {
                "Автоматический режим: выкл"
            };
            parts.push(button(mode.to_owned(), "sync", SPECTRE_MODE, Vec::new()));
            section("Spectre-Meter".to_owned(), parts)
        }
        StationScreen::CivBounties(d) => civ_bounties(d),
        StationScreen::BountyBoard(d) => bounty_board(d),
        StationScreen::Restock(rows) => {
            let mut parts: Vec<Part> = rows
                .iter()
                .map(|v| {
                    line(format!(
                        "{} ({}) — {:.0}%, {} кредитов",
                        v.name, v.location, v.percentage, v.credits
                    ))
                })
                .collect();
            if parts.is_empty() {
                parts.push(line("Все автоматы укомплектованы.".to_owned()));
            }
            section("NT Restock Tracker".to_owned(), parts)
        }
        StationScreen::Scipaper(d) => scipaper(d),
        StationScreen::Robotact(d) => robotact(d),
        StationScreen::Contractor(d) => contractor(d),
    }
}

/// tgui `SyndicateContractor`: the login, the hub, the contracts and the extraction.
fn contractor(d: &ContractorData) -> Part {
    const TITLE: &str = "Syndicate Contractor Uplink";
    let mut parts = Vec::new();
    if !d.error.is_empty() {
        parts.push(line(format!("ОШИБКА: {}", d.error)));
        parts.push(button(
            "Закрыть".to_owned(),
            "times",
            CONTRACT_CLEAR_ERROR,
            Vec::new(),
        ));
    }
    if !d.logged_in {
        parts.push(line(format!("SyndTract v2.0 — {}", d.station_name)));
        parts.push(button(
            "Войти".to_owned(),
            "sign-in-alt",
            CONTRACT_LOGIN,
            Vec::new(),
        ));
        return section(TITLE.to_owned(), parts);
    }
    if d.info_screen {
        parts.push(line(
            "Принимайте контракты, доставляйте цели в зону высадки, вызывайте капсулу и \
             помещайте цель внутрь. Бонус в скобках — за живую цель."
                .to_owned(),
        ));
    }
    parts.push(button(
        if d.info_screen {
            "Скрыть справку"
        } else {
            "Справка"
        }
        .to_owned(),
        "info",
        CONTRACT_TOGGLE_INFO,
        Vec::new(),
    ));
    parts.push(line(format!(
        "ТК к выплате: {} | Заработано ТК: {} | Выполнено контрактов: {}",
        d.redeemable_tc, d.earned_tc, d.contracts_completed
    )));
    parts.push(button(
        "Получить ТК".to_owned(),
        "money-bill-wave",
        CONTRACT_REDEEM,
        Vec::new(),
    ));
    if d.ongoing_contract {
        parts.push(line(format!(
            "Текущее местоположение: {} | Зона высадки: {}",
            d.current_location, d.dropoff_direction
        )));
        if d.extraction_enroute {
            parts.push(line("Капсула эвакуации в пути.".to_owned()));
        } else {
            parts.push(button(
                "Вызвать капсулу".to_owned(),
                "parachute-box",
                CONTRACT_EXTRACTION,
                Vec::new(),
            ));
        }
    }
    for c in &d.contracts {
        let status = match c.status {
            1 => "доступен",
            2 => "принят",
            4 => "эвакуация",
            5 => "выполнен",
            6 => "отменён",
            _ => "",
        };
        parts.push(line(format!(
            "#{} {} ({}) — {} ТК (+{} ТК), высадка: {} [{status}]",
            c.id, c.target, c.target_rank, c.payout, c.payout_bonus, c.dropoff
        )));
        if !c.message.is_empty() {
            parts.push(line(c.message.clone()));
        }
        match c.status {
            1 if !d.ongoing_contract => parts.push(button(
                "Принять".to_owned(),
                "check",
                CONTRACT_ACCEPT,
                vec![("contract_id".to_owned(), c.id.to_string())],
            )),
            2 | 4 => parts.push(button(
                "Отменить".to_owned(),
                "ban",
                CONTRACT_ABORT,
                Vec::new(),
            )),
            _ => {}
        }
    }
    section(TITLE.to_owned(), parts)
}

/// tgui `NtosRobotact`'s Status, Logs and Network tabs, one after the other.
fn robotact(d: &RobotactData) -> Part {
    let none = Vec::new;
    let mut parts: Vec<Part> = vec![
        line(format!("Unit: {}", d.borg_name)),
        line(format!("Type: {}", d.designation)),
        line(format!(
            "AI: {}",
            d.master_ai.clone().unwrap_or_else(|| "None".to_owned())
        )),
        line(format!("Charge: {:.0} / {:.0} J", d.charge, d.max_charge)),
    ];
    if d.charge <= 0.0 {
        parts.push(button(
            "Power Alert".to_owned(),
            "bolt",
            ROBOTACT_ALERT_POWER,
            none(),
        ));
    }
    parts.push(line(format!("Chassis Integrity: {:.0}%", d.integrity)));
    let cover = if d.cover_locked { "LOCKED" } else { "UNLOCKED" };
    parts.push(line(format!("Maintenance Cover: {cover}")));
    if d.cover_locked {
        parts.push(button(
            "Unlock".to_owned(),
            "unlock",
            ROBOTACT_COVER_UNLOCK,
            none(),
        ));
    }
    let sensors = if d.sensors { "ACTIVE" } else { "DISABLED" };
    parts.push(button(
        format!("Sensor Overlay: {sensors}"),
        "eye",
        ROBOTACT_SENSORS,
        none(),
    ));
    if d.self_destruct_able {
        parts.push(button(
            "Self Destruct".to_owned(),
            "bomb",
            ROBOTACT_SELF_DESTRUCT,
            none(),
        ));
    }
    parts.push(line("Upgrades:".to_owned()));
    parts.extend(d.upgrades.iter().map(|u| line(format!("  {u}"))));
    for (label, value) in [
        ("AI Connection", &d.wire_ai),
        ("LawSync", &d.wire_law),
        ("Camera", &d.wire_camera),
        ("Module Controller", &d.wire_module),
        ("Motor Controller", &d.locomotion),
    ] {
        parts.push(line(format!("{label}: {value}")));
    }
    parts.push(line("Laws:".to_owned()));
    parts.extend(d.laws.iter().cloned().map(line));
    parts.push(button(
        "State Laws".to_owned(),
        "volume-up",
        ROBOTACT_LAW_STATE,
        none(),
    ));
    parts.push(button(
        "Law Channel".to_owned(),
        "volume-off",
        ROBOTACT_LAW_CHANNEL,
        none(),
    ));
    parts.push(line("Logs:".to_owned()));
    parts.extend(d.borg_log.iter().cloned().map(line));
    parts.push(line(format!(
        "Network: {}",
        d.master_ai
            .clone()
            .unwrap_or_else(|| "NOT CONFIGURED".to_owned())
    )));
    for c in &d.cyborgs {
        let status = if c.shell_discon {
            "DISCONNECTED"
        } else if c.status != 0 {
            "OFFLINE"
        } else if c.locked_down {
            "LOCKED DOWN"
        } else {
            "NOMINAL"
        };
        let charge = c.charge.map_or("—".to_owned(), |c| format!("{c}%"));
        parts.push(line(format!(
            "{} ({}): {status}, condition {}%, charge {charge}",
            c.name, c.module, c.integ
        )));
    }
    section("RoboTact".to_owned(), parts)
}

/// `NtosScipaper`: the tabs as buttons, then the tab.
fn scipaper(d: &ScipaperData) -> Part {
    let p = |k: &str, v: String| vec![(k.to_owned(), v)];
    let mut parts: Vec<Part> = [
        (1, "Публикация"),
        (2, "Статьи"),
        (3, "Эксперименты"),
        (4, "Партнёры"),
    ]
    .into_iter()
    .map(|(n, label)| {
        let label = if d.current_tab == n {
            format!("[{label}]")
        } else {
            label.to_owned()
        };
        button(
            label,
            "folder",
            PAPER_CHANGE_TAB,
            p("new_tab", n.to_string()),
        )
    })
    .collect();
    if !d.has_techweb {
        parts.push(line("Нет подключения к техвебу.".to_owned()));
    }
    let gains = |g: &PaperGist| format!("сотрудничество: {}, финансы: {}", g.gains.0, g.gains.1);
    match d.current_tab {
        1 => {
            let mut form: Vec<Part> = vec![line(format!(
                "Файл: {}",
                d.selected_file.clone().unwrap_or_else(|| "—".to_owned())
            ))];
            for (name, uid) in &d.file_list {
                form.push(button(
                    name.clone(),
                    "file",
                    PAPER_SELECT_FILE,
                    p("selected_uid", uid.to_string()),
                ));
            }
            form.push(line(format!(
                "Эксперимент: {}",
                d.paper
                    .experiment_name
                    .clone()
                    .unwrap_or_else(|| "—".to_owned())
            )));
            for (name, path) in &d.exp_list {
                form.push(button(
                    name.clone(),
                    "flask",
                    PAPER_SELECT_EXPERIMENT,
                    p("selected_expath", path.clone()),
                ));
            }
            form.push(line(format!(
                "Уровень: {}",
                d.paper
                    .tier
                    .map_or_else(|| "—".to_owned(), |t| t.to_string())
            )));
            for t in &d.allowed_tiers {
                form.push(button(
                    t.to_string(),
                    "layer-group",
                    PAPER_SELECT_TIER,
                    p("selected_tier", t.to_string()),
                ));
            }
            form.push(line(format!(
                "Партнёр: {}",
                d.paper.partner.clone().unwrap_or_else(|| "—".to_owned())
            )));
            for (name, path) in &d.allowed_partners {
                form.push(button(
                    name.clone(),
                    "handshake",
                    PAPER_SELECT_PARTNER,
                    p("selected_partner", path.clone()),
                ));
            }
            form.push(line(format!("Название: {}", d.paper.title)));
            form.push(input(
                "Название".to_owned(),
                PAPER_REWRITE,
                "title",
                Vec::new(),
            ));
            let et_al = if d.paper.et_alia { " и др." } else { "" };
            form.push(line(format!("Автор: {}{et_al}", d.paper.author)));
            form.push(input(
                "Автор".to_owned(),
                PAPER_REWRITE,
                "author",
                Vec::new(),
            ));
            form.push(button(
                "Соавторы".to_owned(),
                "users",
                PAPER_ET_ALIA,
                Vec::new(),
            ));
            form.push(line(format!("Аннотация: {}", d.paper.abstract_text)));
            form.push(input(
                "Аннотация".to_owned(),
                PAPER_REWRITE,
                "abstract",
                Vec::new(),
            ));
            form.push(line(format!("Ожидаемая выгода: {}", gains(&d.paper))));
            form.push(button(
                "Опубликовать".to_owned(),
                "paper-plane",
                PAPER_PUBLISH,
                Vec::new(),
            ));
            parts.push(section("Новая статья".to_owned(), form));
        }
        2 => {
            let mut papers: Vec<Part> = Vec::new();
            if d.published.is_empty() {
                papers.push(line("Статей пока нет.".to_owned()));
            }
            for g in &d.published {
                let et_al = if g.et_alia { " и др." } else { "" };
                papers.push(line(format!(
                    "«{}» — {}{et_al}; {}, уровень {}; {}; {}",
                    g.title,
                    g.author,
                    g.experiment_name.clone().unwrap_or_default(),
                    g.tier.unwrap_or(0),
                    g.partner.clone().unwrap_or_default(),
                    gains(g)
                )));
            }
            parts.push(section("Опубликованные статьи".to_owned(), papers));
        }
        3 => {
            let rows: Vec<Part> = d
                .experiments
                .iter()
                .map(|e| {
                    let targets: Vec<String> = e.target.iter().map(|t| format!("{t}")).collect();
                    line(format!(
                        "{}: {} Цели ({}, {}): {}",
                        e.name,
                        e.description,
                        e.prefix,
                        e.suffix,
                        targets.join(" / ")
                    ))
                })
                .collect();
            parts.push(section("Эксперименты".to_owned(), rows));
        }
        _ => {
            for partner in &d.partners {
                let mut rows = vec![
                    line(partner.flufftext.clone()),
                    line(format!(
                        "Отношения: {}. Множители: сотрудничество ×{}, финансы ×{}",
                        partner.relation, partner.multipliers.0, partner.multipliers.1
                    )),
                    line(format!(
                        "Принимает: {}",
                        partner.accepted_experiments.join(", ")
                    )),
                ];
                for (name, discount, node) in &partner.boosted_nodes {
                    let visible = d.visible_nodes.contains(node);
                    let label = format!("{name}: −{discount}");
                    if partner.purchaseable.contains(node) {
                        rows.push(button(
                            label,
                            "cart-shopping",
                            PAPER_PURCHASE_BOOST,
                            vec![
                                ("boost_seller".to_owned(), partner.path.clone()),
                                ("purchased_boost".to_owned(), node.clone()),
                            ],
                        ));
                    } else if visible {
                        rows.push(line(label));
                    }
                }
                parts.push(section(partner.name.clone(), rows));
            }
        }
    }
    section("NT Frontier".to_owned(), parts)
}

/// `NtosCivCargoHoldTerminal`.
fn civ_bounties(d: &crate::cargo::CivBountyData) -> Part {
    if !d.id_inserted {
        return section(
            "Civilian Bounties".to_owned(),
            vec![line("Вставьте ID-карту.".to_owned())],
        );
    }
    let mut parts = vec![line(d.status_report.clone())];
    match &d.id_bounty_info {
        Some(info) => {
            parts.push(line(format!("Заказ: {info}")));
            parts.push(line(format!(
                "Выполнено: {}, награда: {:.0}",
                d.id_bounty_num, d.id_bounty_value
            )));
        }
        None => parts.push(line("Нет активного заказа.".to_owned())),
    }
    if d.picking {
        for (i, c) in d.choices.iter().enumerate() {
            parts.push(line(format!("{} — {} ({:.0})", c.name, c.info, c.value)));
            parts.push(button(
                "Выбрать".to_owned(),
                "check",
                crate::cargo::PICK,
                vec![(crate::cargo::VALUE.to_owned(), (i + 1).to_string())],
            ));
        }
    }
    parts.push(button(
        "Новые заказы".to_owned(),
        "sync",
        crate::cargo::BOUNTY,
        Vec::new(),
    ));
    section("Civilian Bounties".to_owned(), parts)
}

/// `NtosBountyBoard`.
fn bounty_board(d: &BountyBoardData) -> Part {
    let mut parts = Vec::new();
    match &d.user {
        Some((name, job)) => parts.push(line(format!("{name} ({job})"))),
        None => parts.push(line("Вставьте ID-карту с банковским счётом.".to_owned())),
    }
    for r in &d.requests {
        let request = ("request".to_owned(), r.acc_number.to_string());
        parts.push(line(format!(
            "{} — {}: {}",
            r.owner, r.value, r.description
        )));
        parts.push(button(
            "Откликнуться".to_owned(),
            "handshake",
            BOARD_APPLY,
            vec![request.clone()],
        ));
        parts.push(button(
            "Удалить".to_owned(),
            "trash",
            BOARD_DELETE,
            vec![request.clone()],
        ));
        for a in d.applicants.iter().filter(|a| a.request_id == r.acc_number) {
            parts.push(button(
                format!("Оплатить: {}", a.name),
                "money-bill",
                BOARD_PAY,
                vec![
                    request.clone(),
                    ("applicant".to_owned(), a.requestee_id.to_string()),
                ],
            ));
        }
    }
    parts.push(line(
        "Новая заявка: «СУММА ОПИСАНИЕ», Enter — разместить".to_owned(),
    ));
    parts.push(input(
        "СУММА ОПИСАНИЕ".to_owned(),
        BOARD_CREATE,
        "bounty",
        Vec::new(),
    ));
    section("Bounty Board".to_owned(), parts)
}

fn pay(d: &PayData) -> Part {
    let Some(name) = &d.name else {
        return section(
            "Nanotrasen Pay System".to_owned(),
            vec![line("Вставьте ID-карту с банковским счётом.".to_owned())],
        );
    };
    let mut parts = vec![
        line(format!("Владелец: {name}")),
        line(format!("Ваш токен: {}", d.owner_token)),
        line(format!("Баланс: {}", d.money)),
        line("Перевод: «ТОКЕН СУММА», Enter — отправить".to_owned()),
        input(
            "ТОКЕН СУММА".to_owned(),
            PAY_TRANSACTION,
            "transfer",
            Vec::new(),
        ),
        line("Узнать токен по имени владельца:".to_owned()),
        input("Имя".to_owned(), PAY_GET_TOKEN, "wanted_name", Vec::new()),
    ];
    if let Some(wanted) = &d.wanted_token {
        parts.push(line(wanted.clone()));
    }
    parts.push(line("История:".to_owned()));
    parts.extend(
        d.transactions
            .iter()
            .map(|t| line(format!("{:+} — {}", t.adjusted_money, t.reason))),
    );
    section("Nanotrasen Pay System".to_owned(), parts)
}

fn radar(d: &RadarData) -> Part {
    let mut parts = Vec::new();
    if d.scanning {
        parts.push(line("Сканирование...".to_owned()));
    } else {
        parts.push(button(
            "Сканировать".to_owned(),
            "search",
            RADAR_SCAN,
            Vec::new(),
        ));
    }
    for o in &d.objects {
        let marker = if d.selected.as_deref() == Some(o.object_ref.as_str()) {
            "» "
        } else {
            ""
        };
        parts.push(button(
            format!("{marker}{}", o.name),
            "crosshairs",
            RADAR_SELECT,
            vec![("ref".to_owned(), o.object_ref.clone())],
        ));
    }
    match (&d.selected, &d.target) {
        (Some(_), Some(t)) if t.userot => parts.push(line(format!(
            "Цель далеко: азимут {}°, {:.0} м",
            t.rot, t.distance
        ))),
        (Some(_), Some(t)) => parts.push(line(format!(
            "Цель: {} ({:+}, {:+}), {:.0} м",
            t.pointer,
            t.locx - 24,
            24 - t.locy,
            t.distance
        ))),
        (Some(_), None) => parts.push(line("Сигнал цели потерян.".to_owned())),
        _ => {}
    }
    section("Радар".to_owned(), parts)
}

fn borgs(rows: &[BorgRow]) -> Part {
    let mut parts: Vec<Part> = Vec::new();
    if rows.is_empty() {
        parts.push(line("Киборги не обнаружены.".to_owned()));
    }
    for r in rows {
        let charge = r.charge.map_or("нет батареи".to_owned(), |c| {
            format!("заряд {c}%")
        });
        let low = if r.low_power {
            ", НИЗКИЙ ЗАРЯД"
        } else {
            ""
        };
        parts.push(line(format!("{} — {charge}{low}, {}", r.name, r.area)));
        parts.push(input(
            format!("Сообщение для {}", r.name),
            BORG_MESSAGE,
            "message",
            vec![("ref".to_owned(), r.borg_ref.clone())],
        ));
    }
    section("SiliConnect".to_owned(), parts)
}

fn bots(rows: &[BotRow]) -> Part {
    let mut parts: Vec<Part> = Vec::new();
    if rows.is_empty() {
        parts.push(line("Боты не обнаружены.".to_owned()));
    }
    for r in rows {
        parts.push(line(format!("{} — {}, {}", r.name, r.mode, r.area)));
        let (label, interaction) = if r.on {
            ("Выключить", "off")
        } else {
            ("Включить", "on")
        };
        parts.push(button(
            label.to_owned(),
            "power-off",
            BOT_INTERACT,
            vec![
                ("ref".to_owned(), r.bot_ref.clone()),
                ("interaction".to_owned(), interaction.to_owned()),
            ],
        ));
    }
    section("BotKeeper".to_owned(), parts)
}
