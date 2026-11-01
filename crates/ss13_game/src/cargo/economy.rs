//! The economy (server), after tg `SSeconomy` (`code/controllers/subsystem/economy.dm`) and
//! `/datum/bank_account` (`code/modules/economy/account.dm`).
//!
//! - Department budgets ([`DEPARTMENTS`]): `budget_pool` split over the seven accounts the tg way
//!   (`round(35000 / 7 - 1)`), cargo starts with nothing.
//! - A crew member whose job has a `paycheck_department` gets a bank account when equipped (tg
//!   `on_job_equipping`): a unique six digit id, `STARTING_PAYCHECKS` free paychecks, the id on
//!   the job's ID card ([`BankAccountId`], tg `registered_account`).
//! - Every `wait` (5 minutes, `SS13_PAYDAY_SECONDS`) while the round runs: each department gets
//!   `MAX_GRANT_DPT`, then each account is paid one paycheck from its department
//!   (`payday(1, skippable = TRUE)`: clamped to `PAYCHECK_CREW`), with tg's bank card messages.
//! - The round-end economy report (`market_report`): the crew's total, the average, the richest.
// shortcut: no inflation, mail, civilian bounties, debts, departmental cards, audit log or
// species payday modifiers; vending machines still sell for free.

use std::collections::HashMap;

use bevy::prelude::*;
use ss13_mapload::MapSeed;
use ss13_mapload::jobs::Job;
use ss13_mapload::spawner::Rng;
use ss13_net::chat::server::SendChat;
use ss13_shared::chat::{ChatChannel, ChatMessage, ChatSegment, Span};

use super::MONEY_SYMBOL;
use crate::inventory::CarriedBy;
use crate::lobby::GameState;
use crate::round::Ticker;
use crate::storage::StoredIn;

pub const ACCOUNT_CIV: &str = "CIV";
pub const ACCOUNT_ENG: &str = "ENG";
pub const ACCOUNT_SCI: &str = "SCI";
pub const ACCOUNT_MED: &str = "MED";
pub const ACCOUNT_SRV: &str = "SRV";
pub const ACCOUNT_CAR: &str = "CAR";
pub const ACCOUNT_SEC: &str = "SEC";
/// `SSeconomy.department_accounts`, in tg's order, with BandaStation's names.
pub const DEPARTMENTS: [(&str, &str); 7] = [
    (ACCOUNT_CIV, "Гражданский бюджет"),
    (ACCOUNT_ENG, "Инженерный бюджет"),
    (ACCOUNT_SCI, "Научный бюджет"),
    (ACCOUNT_MED, "Медицинский бюджет"),
    (ACCOUNT_SRV, "Бюджет обслуживания"),
    (ACCOUNT_CAR, "Бюджет снабжения"),
    (ACCOUNT_SEC, "Оборонный бюджет"),
];
/// `SSeconomy.budget_pool`.
pub const BUDGET_POOL: i64 = 35000;
/// `STARTING_PAYCHECKS`.
pub const STARTING_PAYCHECKS: u32 = 5;
/// `PAYCHECK_CREW`: the most a single passive paycheck pays.
pub const PAYCHECK_CREW: i64 = 50;
/// `MAX_GRANT_DPT`.
pub const MAX_GRANT_DPT: i64 = 500;
/// `SSeconomy` `wait = 5 MINUTES`.
pub const PAYDAY_SECS: f64 = 300.0;

/// tg's budget per department at round start: `round(budget_pool / department_accounts.len - 1)`
/// (DM precedence: `35000 / 7 - 1`), cargo `0`.
pub fn starting_budget(department: &str) -> i64 {
    if department == ACCOUNT_CAR {
        0
    } else {
        (BUDGET_POOL as f64 / DEPARTMENTS.len() as f64 - 1.0).round() as i64
    }
}

/// Economy timing. Insert before `GameServerPlugin` to override; otherwise tg's with
/// `SS13_PAYDAY_SECONDS`.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct EconomyConfig {
    pub payday_secs: f64,
}

impl Default for EconomyConfig {
    fn default() -> Self {
        let payday_secs = std::env::var("SS13_PAYDAY_SECONDS")
            .ok()
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite() && *v > 0.0)
            .unwrap_or(PAYDAY_SECS);
        Self { payday_secs }
    }
}

/// A `/datum/bank_account` (department accounts have no job).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BankAccount {
    pub holder: String,
    pub balance: i64,
    /// `account_job.paycheck`.
    pub paycheck: u32,
    /// `account_job.paycheck_department`; empty for department accounts.
    pub paycheck_department: String,
}

impl BankAccount {
    /// tg `has_money`.
    pub fn has_money(&self, amount: i64) -> bool {
        self.balance >= amount
    }

    /// tg `adjust_money`: a withdrawal needs the money; `false` changes nothing.
    pub fn adjust_money(&mut self, amount: i64) -> bool {
        if amount < 0 && !self.has_money(-amount) {
            return false;
        }
        self.balance += amount;
        true
    }
}

/// On an ID card: its `registered_account`.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BankAccountId(pub u32);

/// The account of an ID card, or of the card a PDA holds (tg `GetID()?.registered_account`).
pub fn account_of(world: &World, item: Entity) -> Option<u32> {
    world.get::<BankAccountId>(item).map(|a| a.0).or_else(|| {
        let card = world
            .get::<crate::modpc::pda::ModularComputer>(item)?
            .stored_id?;
        world.get::<BankAccountId>(card).map(|a| a.0)
    })
}

/// `SSeconomy`: department accounts and `bank_accounts_by_id`.
#[derive(Resource)]
pub struct Economy {
    departments: Vec<(String, BankAccount)>,
    accounts: HashMap<u32, BankAccount>,
    rng: Rng,
    last_payday: Option<f64>,
}

/// Why a payday paid nothing (tg's bank card messages).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaydayError {
    NoAccount,
    /// No job, or nothing to pay.
    NoPaycheck,
    NoDepartment,
    DepartmentBroke,
}

impl Economy {
    /// `SSeconomy.Initialize`.
    pub fn new(seed: u64) -> Self {
        Self {
            departments: DEPARTMENTS
                .iter()
                .map(|&(id, name)| {
                    let account = BankAccount {
                        holder: name.to_owned(),
                        balance: starting_budget(id),
                        ..default()
                    };
                    (id.to_owned(), account)
                })
                .collect(),
            accounts: HashMap::new(),
            rng: Rng::new(seed ^ 0x6563_6f6e),
            last_payday: None,
        }
    }

    /// `get_dep_account`.
    pub fn department(&self, id: &str) -> Option<&BankAccount> {
        self.departments
            .iter()
            .find(|(d, _)| d == id)
            .map(|(_, a)| a)
    }

    pub fn department_mut(&mut self, id: &str) -> Option<&mut BankAccount> {
        self.departments
            .iter_mut()
            .find(|(d, _)| d == id)
            .map(|(_, a)| a)
    }

    pub fn departments(&self) -> impl Iterator<Item = (&str, &BankAccount)> {
        self.departments.iter().map(|(d, a)| (d.as_str(), a))
    }

    /// `bank_accounts_by_id[id]`.
    pub fn account(&self, id: u32) -> Option<&BankAccount> {
        self.accounts.get(&id)
    }

    pub fn account_mut(&mut self, id: u32) -> Option<&mut BankAccount> {
        self.accounts.get_mut(&id)
    }

    pub fn accounts(&self) -> impl Iterator<Item = (u32, &BankAccount)> {
        self.accounts.iter().map(|(id, a)| (*id, a))
    }

    /// `new /datum/bank_account(name, job)`: `setup_unique_account_id` picks an unused id in
    /// 111111..=999999.
    pub fn open_account(&mut self, holder: &str, paycheck: u32, department: &str) -> u32 {
        let mut id = 111_111;
        for _ in 0..1000 {
            id = 111_111 + (self.rng.next_u64() % 888_889) as u32;
            if !self.accounts.contains_key(&id) {
                break;
            }
        }
        self.accounts.insert(
            id,
            BankAccount {
                holder: holder.to_owned(),
                balance: 0,
                paycheck,
                paycheck_department: department.to_owned(),
            },
        );
        id
    }

    /// tg `payday(amount_of_paychecks, free)`: the new balance. Paid ones come out of the
    /// department budget (`transfer_money`); a single paycheck is clamped to `PAYCHECK_CREW`.
    pub fn payday(&mut self, id: u32, paychecks: u32, free: bool) -> Result<i64, PaydayError> {
        let account = self.accounts.get(&id).ok_or(PaydayError::NoAccount)?;
        if account.paycheck_department.is_empty() || paychecks == 0 {
            return Err(PaydayError::NoPaycheck);
        }
        let mut amount = i64::from(account.paycheck) * i64::from(paychecks);
        if paychecks == 1 {
            amount = amount.clamp(0, PAYCHECK_CREW);
        }
        if !free {
            let department = account.paycheck_department.clone();
            let budget = self
                .department_mut(&department)
                .ok_or(PaydayError::NoDepartment)?;
            if !budget.adjust_money(-amount) {
                return Err(PaydayError::DepartmentBroke);
            }
        }
        let account = self.accounts.get_mut(&id).expect("checked above");
        account.adjust_money(amount);
        Ok(account.balance)
    }

    /// `departmental_payouts`.
    pub fn departmental_payouts(&mut self) {
        for (_, account) in &mut self.departments {
            account.adjust_money(MAX_GRANT_DPT);
        }
    }
}

pub struct EconomyPlugin;

impl Plugin for EconomyPlugin {
    fn build(&self, app: &mut App) {
        if !app.world().contains_resource::<EconomyConfig>() {
            app.init_resource::<EconomyConfig>();
        }
        app.add_systems(Startup, init).add_systems(Update, fire);
    }
}

fn init(world: &mut World) {
    let seed = world.get_resource::<MapSeed>().map_or(0, |s| s.0);
    world.insert_resource(Economy::new(seed));
}

/// tg `on_job_equipping`: a bank account for a job with a `paycheck_department`, its starting
/// paychecks, and the account on the job's ID card. Called from `crate::jobs::join`.
pub fn open_job_account(world: &mut World, job: &Job, name: &str, card: Option<Entity>) {
    if job.paycheck_department.is_empty() {
        return;
    }
    let Some(mut economy) = world.get_resource_mut::<Economy>() else {
        return;
    };
    let id = economy.open_account(name, job.paycheck, &job.paycheck_department);
    let _ = economy.payday(id, STARTING_PAYCHECKS, true);
    if let Some(card) = card {
        world.entity_mut(card).insert(BankAccountId(id));
    }
}

/// `SSeconomy.fire`, every `payday_secs` of a running round.
fn fire(world: &mut World) {
    let Some(ticker) = world.get_resource::<Ticker>() else {
        return;
    };
    if ticker.state() != GameState::Playing || !world.contains_resource::<Economy>() {
        return;
    }
    let now = world.resource::<Time>().elapsed_secs_f64();
    let wait = world.resource::<EconomyConfig>().payday_secs;
    let mut economy = world.resource_mut::<Economy>();
    let Some(last) = economy.last_payday else {
        economy.last_payday = Some(now);
        return;
    };
    if now - last < wait {
        return;
    }
    economy.last_payday = Some(now);
    economy.departmental_payouts();
    let mut ids: Vec<u32> = economy.accounts.keys().copied().collect();
    ids.sort_unstable();
    let mut talk = Vec::new();
    for id in ids {
        let text = match economy.payday(id, 1, false) {
            Ok(balance) => {
                format!("Выплата зарплаты успешна. На аккаунте теперь {balance}{MONEY_SYMBOL}.")
            }
            Err(PaydayError::NoDepartment) => {
                "ОШИБКА: Выплата зарплаты отменена. Не удалось связаться со счётом отдела.".into()
            }
            Err(PaydayError::DepartmentBroke) => {
                "ОШИБКА: Выплата зарплаты отменена. Средств отдела недостаточно.".into()
            }
            Err(_) => continue,
        };
        talk.push((id, text));
    }
    for (id, text) in talk {
        bank_card_talk(world, id, &text);
    }
}

/// The mob holding `item`, through containers (tg `recursive_loc_check(card, /mob)`).
pub fn holder_of(world: &World, mut item: Entity) -> Option<Entity> {
    for _ in 0..8 {
        if let Some(c) = world.get::<CarriedBy>(item) {
            return Some(c.0);
        }
        item = world.get::<StoredIn>(item)?.0;
    }
    None
}

/// tg `bank_card_talk`: the message to whoever holds a card of the account.
// shortcut: only cards a mob carries talk (tg also beeps to mobs next to a card on the floor).
pub fn bank_card_talk(world: &mut World, account: u32, text: &str) {
    let mut cards = world.query::<(Entity, &BankAccountId)>();
    let cards: Vec<Entity> = cards
        .iter(world)
        .filter(|(_, a)| a.0 == account)
        .map(|(e, _)| e)
        .collect();
    let mut clients = world.query::<(Entity, &ss13_net::server::ClientToken)>();
    let clients: Vec<Entity> = clients.iter(world).map(|(e, _)| e).collect();
    for card in cards {
        let Some(mob) = holder_of(world, card) else {
            continue;
        };
        let players = world.resource::<crate::server::Players>();
        let to: Vec<Entity> = clients
            .iter()
            .copied()
            .filter(|&c| players.mob(c) == Some(mob))
            .collect();
        if to.is_empty() {
            continue;
        }
        world.write_message(SendChat {
            clients: to,
            message: ChatMessage::simple(ChatChannel::System, Span::Notice, text),
        });
    }
}

/// tg `SSticker.market_report` without the venues: the crew's money at round end.
pub fn market_report(world: &World) -> Vec<ChatMessage> {
    let Some(economy) = world.get_resource::<Economy>() else {
        return Vec::new();
    };
    let total = economy.accounts.len() as i64;
    let vault: i64 = economy.accounts.values().map(|a| a.balance).sum();
    let mut accounts: Vec<(u32, &BankAccount)> = economy.accounts().collect();
    accounts.sort_by_key(|(id, _)| *id);
    let richest = accounts
        .iter()
        .fold(None::<&BankAccount>, |best, (_, a)| match best {
            Some(b) if b.balance >= a.balance => Some(b),
            _ => Some(a),
        });
    let line = |span, text: String| ChatMessage::simple(ChatChannel::System, span, text);
    let mut out = vec![
        line(Span::Bold, "Экономическая сводка станции:".into()),
        line(Span::Bold, "Общая статистика:".into()),
        line(
            Span::Plain,
            format!("В эту смену экипаж собрал {vault}{MONEY_SYMBOL}."),
        ),
    ];
    if total > 0 {
        out.push(line(
            Span::Plain,
            format!(
                "В среднем было собрано {}{MONEY_SYMBOL}.",
                dm_number(vault as f64 / total as f64)
            ),
        ));
    }
    out.push(match richest {
        Some(a) => ChatMessage {
            channel: ChatChannel::System,
            segments: vec![
                ChatSegment::new(
                    Span::Plain,
                    "Самым богатым членом экипажа в конце смены был ",
                ),
                ChatSegment::new(Span::Bold, format!("{} с {}", a.holder, a.balance)),
                ChatSegment::new(Span::Plain, format!("{MONEY_SYMBOL}!")),
            ],
        },
        None => line(
            Span::Plain,
            "Каким-то образом, никто не смог заработать кредитов за эту смену...".into(),
        ),
    });
    out
}

/// BYOND's number text: up to 6 significant digits, no trailing zeros.
fn dm_number(n: f64) -> String {
    let s = format!(
        "{:.*}",
        6usize.saturating_sub(n.abs().max(1.0).log10() as usize + 1),
        n
    );
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        s
    };
    if s == "-0" { "0".into() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starting_budgets_follow_tg_precedence() {
        assert_eq!(starting_budget(ACCOUNT_SEC), 4999);
        assert_eq!(starting_budget(ACCOUNT_CAR), 0);
        let economy = Economy::new(1);
        assert_eq!(economy.department(ACCOUNT_CIV).unwrap().balance, 4999);
        assert_eq!(
            economy.department(ACCOUNT_CAR).unwrap().holder,
            "Бюджет снабжения"
        );
        assert!(economy.department("NOPE").is_none());
    }

    #[test]
    fn paydays_move_money_from_the_department() {
        let mut e = Economy::new(7);
        let qm = e.open_account("Квартирмейстер", 100, ACCOUNT_CAR);
        let other = e.open_account("Ассистент", 25, ACCOUNT_CIV);
        assert_ne!(qm, other);
        assert!((111_111..=999_999).contains(&qm));
        // Starting paychecks are free and not clamped.
        assert_eq!(e.payday(qm, STARTING_PAYCHECKS, true), Ok(500));
        assert_eq!(e.department(ACCOUNT_CAR).unwrap().balance, 0);
        // A passive paycheck is clamped to PAYCHECK_CREW and paid by the budget: cargo is broke.
        assert_eq!(e.payday(qm, 1, false), Err(PaydayError::DepartmentBroke));
        e.departmental_payouts();
        assert_eq!(e.department(ACCOUNT_CAR).unwrap().balance, 500);
        assert_eq!(e.department(ACCOUNT_CIV).unwrap().balance, 5499);
        assert_eq!(e.payday(qm, 1, false), Ok(550));
        assert_eq!(e.department(ACCOUNT_CAR).unwrap().balance, 450);
        assert_eq!(e.payday(other, 1, false), Ok(25));
        assert_eq!(e.payday(999, 1, false), Err(PaydayError::NoAccount));
    }

    #[test]
    fn adjust_money_refuses_overdrafts() {
        let mut a = BankAccount {
            balance: 10,
            ..default()
        };
        assert!(!a.adjust_money(-11));
        assert_eq!(a.balance, 10);
        assert!(a.adjust_money(-10));
        assert!(a.adjust_money(5));
        assert_eq!(a.balance, 5);
    }

    #[test]
    fn dm_numbers() {
        assert_eq!(dm_number(250.0), "250");
        assert_eq!(dm_number(83.333333), "83.3333");
        assert_eq!(dm_number(0.5), "0.5");
    }
}
