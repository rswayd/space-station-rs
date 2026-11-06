//! The emergency shuttle's bureaucratic events: Shuttle Catastrophe
//! (`code/modules/events/shuttle_catastrophe.dm`) and Shuttle Insurance
//! (`code/modules/events/shuttle_insurance.dm`), over the shuttle catalog
//! (`crate::shuttle_catalog`).
//!
//! - Catastrophe: only while the emergency shuttle is idle, recalled or called and no event has
//!   replaced it yet. Uninsured, a random catalog shuttle replaces it (`SHUTTLEPURCHASE_FORCED`:
//!   no purchases afterwards) and the announcement names it; insured, the cargo budget gets
//!   8000 instead and the shuttle stays.
//! - Insurance: a message to every communications console offers insurance for half the
//!   current shuttle's catalog price (5000 for a shuttle not in the catalog) from a rogue ship;
//!   the first "buy" answer pays it from the cargo budget (a short budget is "fraud": the offer
//!   is lost), unless the shuttle has docked by then.
// shortcut: no admin setup (shuttle choice, docked-shuttle warning); no player-built shuttle
// check (there are none); the ship's announcements are not coloured red (no announcement
// colours).

use bevy::prelude::*;

use super::controller::{EventRng, EventState, RoundEvent};
use crate::cargo::economy::{ACCOUNT_CAR, Economy};
use crate::comms::CommsMessage;
use crate::shuttle::{EmergencyShuttle, ShuttleMode};
use crate::shuttle_catalog::{CATALOG, ShuttleCatalog};

pub const CATASTROPHE: &str = "Shuttle Catastrophe";
pub const INSURANCE: &str = "Shuttle Insurance";

/// What an insured catastrophe pays the cargo budget.
pub const INSURANCE_PAYOUT: i64 = 8000;
/// `insurance_evaluation` of a shuttle not in the catalog ("gee i dunno").
const UNKNOWN_EVALUATION: i64 = 5000;

/// `strings/pirates.json` `rogue_names`.
const ROGUE_NAMES: [&str; 43] = [
    "Титан Бездны",
    "Пепел к Пеплу",
    "Истребитель Астероидов",
    "Перетаскиватель Астероидов",
    "Ревущий Космодракон",
    "Слайдер Солнечных лучей",
    "Кровавые Деньги",
    "Космическая Канонерка",
    "Обслуживание Клиентов",
    "Рубильник Мертвеца",
    "Спорщик Дипломатии",
    "Дрифтбим",
    "Эхо Смеха",
    "Электрическое Буги",
    "Этические Принципы",
    "Толстая Нога",
    "Всадники Небес",
    "Я Стал Смертью",
    "Я Люблю Бомбу",
    "Икити Пикети Стартутс Накл Драгер",
    "Хранитель Революции: Дезертир Эскорта",
    "Корабль Джона",
    "Плавитель Ящериц",
    "Потерянный в Глубоком Космосе",
    "Ракетная Стойка",
    "Разбрызгиватель Мотыльков",
    "Новые Грехи за Старые",
    "Последнее Дело",
    "Красить Астероиды в Красный",
    "Пастафарианство",
    "Яростный Метеор",
    "Обратный Дальний Прыжок",
    "Бич Млечного Пути",
    "Кремниевая Политика",
    "Космическая Смазка",
    "Космическое Правосудие",
    "Космическая Жемчужина",
    "Месть Космической Королевы",
    "Скребок Звёзд",
    "Синдикатный Капер",
    "Мазианни",
    "Война как Бизнес",
    "Ксенофобия",
];

/// `EMERGENCY_AT_LEAST_DOCKED`: the shuttle is past idle, recalled and called.
pub fn at_least_docked(world: &World) -> bool {
    world.get_resource::<EmergencyShuttle>().is_some_and(|s| {
        !matches!(
            s.state.mode,
            ShuttleMode::Idle | ShuttleMode::Recall | ShuttleMode::Call
        )
    })
}

/// The shared `can_spawn_event` checks: an emergency shuttle not yet replaced by an event and
/// not at the station.
fn shuttle_replaceable(world: &World) -> bool {
    world.contains_resource::<EmergencyShuttle>()
        && !at_least_docked(world)
        && world
            .get_resource::<ShuttleCatalog>()
            .is_some_and(|c| !c.forced)
}

fn has_cargo_budget(world: &World) -> bool {
    world
        .get_resource::<Economy>()
        .is_some_and(|e| e.department(ACCOUNT_CAR).is_some())
}

/// Shuttle Catastrophe's `can_spawn_event`, and something to replace the shuttle with (tg's
/// template pick has no empty case).
pub fn catastrophe_available(world: &mut World) -> bool {
    shuttle_replaceable(world)
        && world
            .get_resource::<ShuttleCatalog>()
            .is_some_and(|c| c.insurance || !c.shuttles.is_empty())
}

/// Shuttle Insurance's `can_spawn_event`.
pub fn insurance_available(world: &mut World) -> bool {
    shuttle_replaceable(world) && has_cargo_budget(world)
}

pub fn make_catastrophe(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Catastrophe { new_shuttle: None })
}

struct Catastrophe {
    /// Index into [`CATALOG`] and its template.
    new_shuttle: Option<(usize, std::path::PathBuf)>,
}

fn insured(world: &World) -> bool {
    world
        .get_resource::<ShuttleCatalog>()
        .is_some_and(|c| c.insurance)
}

fn pick_shuttle(world: &mut World) -> Option<(usize, std::path::PathBuf)> {
    let shuttles = world.get_resource::<ShuttleCatalog>()?.shuttles.clone();
    world.resource_mut::<EventRng>().pick(&shuttles).cloned()
}

impl RoundEvent for Catastrophe {
    fn setup(&mut self, world: &mut World, _: &mut EventState) {
        if insured(world) || self.new_shuttle.is_some() {
            return;
        }
        self.new_shuttle = pick_shuttle(world);
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, _: bool) {
        let syndicate =
            crate::antag::nukeop::server::syndicate_name(&mut world.resource_mut::<EventRng>().0);
        let causes = [
            format!("был атакован оперативниками {syndicate}"),
            "странным образом телепортировался в неизвестность".to_owned(),
            "лишился обслуживающего персонала из-за забастовки".to_owned(),
            "лишился двигателей".to_owned(),
            "[ДАННЫЕ УДАЛЕНЫ]".to_owned(),
            "сбился с курса и сгорел вблизи звезды".to_owned(),
            "был оборудовон устройством клонирования".to_owned(),
            "разбился об ангар при посадке".to_owned(),
        ];
        let cause = world
            .resource_mut::<EventRng>()
            .pick(&causes)
            .expect("non-empty")
            .clone();
        let mut message = format!("Ваш эвакуационный шаттл {cause}. ");
        if insured(world) {
            message += "К счастью, страховка вашего шаттла покрыла расходы на ремонт!";
            if has_cargo_budget(world) {
                message += &format!(
                    " Вы получили премию от {} за умение распоряжаться финансами.",
                    super::COMMAND_NAME
                );
            }
        } else {
            // A false alarm runs no `setup` (tg reads a null template there): any shuttle.
            let shuttle = match &self.new_shuttle {
                Some((i, _)) => Some(*i),
                None => pick_shuttle(world).map(|(i, _)| i),
            };
            let Some(i) = shuttle else {
                return;
            };
            message += &format!(
                "На замену будет выслан {} до новых указаний.",
                CATALOG[i].name
            );
        }
        let title = format!(
            "{}: Отдел проектирования космических кораблей",
            super::COMMAND_NAME
        );
        super::priority_announce(world, &title, &message);
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        if insured(world) {
            if let Some(mut economy) = world.get_resource_mut::<Economy>()
                && let Some(account) = economy.department_mut(ACCOUNT_CAR)
            {
                account.adjust_money(INSURANCE_PAYOUT);
            }
            return;
        }
        let Some((i, path)) = self.new_shuttle.clone() else {
            return;
        };
        world.resource_mut::<ShuttleCatalog>().forced = true;
        if crate::shuttle_catalog::replace_emergency(world, CATALOG[i].id, &path) {
            info!("{CATASTROPHE} set a new shuttle, {}.", CATALOG[i].name);
        }
    }
}

/// The running offer, for the answer (tg keeps it on the event datum).
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct InsuranceOffer {
    pub ship_name: String,
    pub evaluation: i64,
}

pub fn make_insurance(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(Insurance)
}

struct Insurance;

/// `insurance_evaluation`: half the catalog price of the shuttle by its name.
pub fn evaluation(shuttle_name: &str) -> i64 {
    CATALOG
        .iter()
        .find(|e| e.name == shuttle_name)
        .map(|e| e.credit_cost / 2)
        .filter(|&v| v != 0)
        .unwrap_or(UNKNOWN_EVALUATION)
}

impl RoundEvent for Insurance {
    fn setup(&mut self, world: &mut World, _: &mut EventState) {
        let ship_name = *world
            .resource_mut::<EventRng>()
            .pick(&ROGUE_NAMES)
            .expect("non-empty");
        let name = world
            .get_resource::<EmergencyShuttle>()
            .map(|s| s.name.clone())
            .unwrap_or_default();
        world.insert_resource(InsuranceOffer {
            ship_name: ship_name.to_owned(),
            evaluation: evaluation(&name),
        });
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, _: bool) {
        let mut announcement = crate::announce::Priority::new(
            "Входящий подпространственный вызов. Защищенный канал открыт на всех \
             коммуникационных консолях.",
            "Входящее сообщение",
        );
        if let Some(sound) = crate::announce::report_sound(world) {
            announcement = announcement.sound(sound);
        }
        announcement.send(world);
    }

    fn start(&mut self, world: &mut World, _: &mut EventState) {
        let Some(offer) = world.get_resource::<InsuranceOffer>().cloned() else {
            return;
        };
        let message = CommsMessage {
            title: "Страховка шаттла".to_owned(),
            content: format!(
                "Привет, приятель, говорит капитан судна {}. Не могу не заметить, что у тебя \
                 там дикий и сумасшедший шаттл БЕЗ СТРАХОВКИ! Безумие! А что, если с ним \
                 что-нибудь случится?! Мы провели быструю оценку тарифов в этом секторе и \
                 возьмем всего {} за страховку вашего шаттла в случае любой катастрофы.",
                offer.ship_name, offer.evaluation
            ),
            possible_answers: vec![
                "Приобрести страховку.".to_owned(),
                "Отклонить предложение.".to_owned(),
            ],
            answered: 0,
        };
        crate::announce::send_unique_message(world, message, answered);
    }
}

/// tg `answered()`.
fn answered(world: &mut World, answer: u32) {
    let Some(offer) = world.get_resource::<InsuranceOffer>().cloned() else {
        return;
    };
    let announce = |world: &mut World, text: &str| {
        crate::announce::Priority::new(text, "")
            .sender(offer.ship_name.clone())
            .send(world);
    };
    if at_least_docked(world) {
        announce(
            world,
            "Друзья, вы опоздали с покупкой страховки. Наши агенты уже покинули ваш сектор.",
        );
        return;
    }
    if answer != 1 {
        return;
    }
    let paid = world
        .get_resource_mut::<Economy>()
        .and_then(|mut e| {
            e.department_mut(ACCOUNT_CAR)
                .map(|a| a.adjust_money(-offer.evaluation))
        })
        .unwrap_or(false);
    if !paid {
        announce(
            world,
            "Вы не прислали нам достаточно денег за страховку шаттла. На языке космических \
             дилетантов это считается мошенничеством. Мы оставим ваши деньги себе, мошенники!",
        );
        return;
    }
    announce(world, "Благодарим вас за покупку страховки для шаттла!");
    if let Some(mut catalog) = world.get_resource_mut::<ShuttleCatalog>() {
        catalog.insurance = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluation_is_half_the_catalog_price() {
        assert_eq!(evaluation("Box Station Emergency Shuttle"), 400);
        assert_eq!(evaluation("Delta Station Emergency Shuttle"), 1500);
        assert_eq!(evaluation("Some Custom Shuttle"), UNKNOWN_EVALUATION);
    }

    fn world(balance: i64) -> World {
        let mut w = World::new();
        let mut economy = Economy::new(1);
        let account = economy.department_mut(ACCOUNT_CAR).expect("cargo budget");
        account.balance = balance;
        w.insert_resource(economy);
        w.insert_resource(ShuttleCatalog::default());
        w.insert_resource(InsuranceOffer {
            ship_name: "Корабль Джона".to_owned(),
            evaluation: 400,
        });
        w
    }

    fn balance(w: &World) -> i64 {
        w.resource::<Economy>()
            .department(ACCOUNT_CAR)
            .expect("cargo budget")
            .balance
    }

    #[test]
    fn buying_insurance_pays_from_the_cargo_budget() {
        let mut w = world(1000);
        answered(&mut w, 2);
        assert!(!w.resource::<ShuttleCatalog>().insurance, "declined");
        assert_eq!(balance(&w), 1000);
        answered(&mut w, 1);
        assert!(w.resource::<ShuttleCatalog>().insurance);
        assert_eq!(balance(&w), 600);

        let mut poor = world(100);
        answered(&mut poor, 1);
        assert!(!poor.resource::<ShuttleCatalog>().insurance, "fraud");
        assert_eq!(balance(&poor), 100);
    }

    #[test]
    fn no_shuttle_no_events() {
        let mut w = world(1000);
        assert!(!at_least_docked(&w));
        assert!(!catastrophe_available(&mut w));
        assert!(!insurance_available(&mut w));
    }
}
