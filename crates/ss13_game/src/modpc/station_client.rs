//! Client screens of the station programs (`NtosPay`, `NtosRadar`, `NtosCyborgRemoteMonitor`,
//! `NtosRoboControl`, `NtosRevelation`, `NtosPhysScanner`, `NtosSpectreMeter`).
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
    }
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
