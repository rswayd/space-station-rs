//! Shuttle Loan (`code/modules/events/shuttle_loan/`): Central Command offers a deal for the
//! cargo shuttle; a supply console's "Loan Shuttle" (`crate::cargo::console`) accepts it while
//! the shuttle idles at CentCom: the bonus goes to the cargo budget, the shuttle is called
//! straight to the station for 5 minutes, and the situation's cargo and threats spawn on its
//! free floor tiles. An offer nobody took lapses when the event ends (500 fires).
//!
//! Only one loan runs at a time; each situation comes once per round until all have
//! (`unavailable_situations`).
// shortcut: of tg's situations "Shuttle with a ticking bomb" (no shuttle loan syndicate bomb),
// "Paperwork shipment" (no paperwork stamping payout) and the mail strike (no "mail blocked"
// station trait, no wall dents) are left out; the infected corpses carry no disease; no
// admin setup, no `STATION_TRAIT_LOANER_SHUTTLE` bonus, no "contents" check of spawn tiles.
// The items spawn as soon as the loan is taken, on the shuttle still docked at CentCom: tg's
// `end()` runs one fire after `loan_shuttle()` too (its `tick()` never gets to wait).

use bevy::prelude::*;
use ss13_shared::TilePos;

use super::controller::{EventRng, EventState, RoundEvent};
use crate::cargo::economy::{ACCOUNT_CAR, Economy};
use crate::cargo::supply::{
    LoanOffer, SupplyMode, SupplyOrders, SupplyPacks, SupplyShuttle, empty_turfs, generate_pack,
};

pub const NAME: &str = "Shuttle Loan";

/// tg `delay_time`.
const DELAY_SECS: f64 = 300.0;
/// `CARGO_CRATE_VALUE`.
const CRATE_VALUE: i64 = 200;
const DEFAULT_THANKS: &str = "Грузовой шаттл должен вернуться через пять минут. За ваши хлопоты \
     будут начислены бонусные очки снабжения.";
const SHORT_THANKS: &str = "Грузовой шаттл должен вернуться через пять минут.";
const CORPSE: &str = "/obj/effect/mob_spawn/corpse/";

/// `/datum/shuttle_loan_situation`.
pub struct Situation {
    pub sender: &'static str,
    pub announcement: &'static str,
    pub transit: &'static str,
    pub bonus: i64,
    pub thanks: &'static str,
    pub logging: &'static str,
    /// `spawn_items`: adds paths to the spawn list or spawns on the free tiles itself.
    spawn: fn(&mut World, &mut Vec<&'static str>, &mut Vec<TilePos>),
}

/// The ported `subtypesof(/datum/shuttle_loan_situation)`.
pub static SITUATIONS: [Situation; 7] = [
    Situation {
        sender: "Центральное Командование: Научно-исследовательский центр",
        announcement: "Ваша станция была выбрана для проведения эпидемиологического \
                       исследования. Пришлите нам свой грузовой шаттл, чтобы получить образцы \
                       для исследования.",
        transit: "Орбразцы вирусов в пути.",
        bonus: CRATE_VALUE * 50,
        thanks: DEFAULT_THANKS,
        logging: "Virus shuttle",
        spawn: antidote,
    },
    Situation {
        sender: "Центральное Командование: Отдел снабжения",
        announcement: "Кажется, в этом месяце мы заказали в два раза больше пакетов пополнения \
                       для нашего отдела. Мы можем отправить их вам?",
        transit: "Пополнение для департаментов в пути.",
        bonus: 0,
        thanks: SHORT_THANKS,
        logging: "Resupply packages",
        spawn: department_resupply,
    },
    Situation {
        sender: "Центральное Командование: Контрразведка",
        announcement: "Синдикат пытается проникнуть на вашу станцию. Если вы позволите им \
                       захватить ваш грузовой шаттл, вы избавите нас от головной боли.",
        transit: "Команда захвата Синдиката в пути.",
        bonus: CRATE_VALUE * 50,
        thanks: DEFAULT_THANKS,
        logging: "Syndicate boarding party",
        spawn: syndie_hijacking,
    },
    Situation {
        sender: "Центральное Командование: Отдел по уборке",
        announcement: "Один из наших грузовых кораблей, перевозивший груз пчел, подвергся \
                       нападению экотеррористов. Вы можете навести порядок?",
        transit: "Зараженный шаттл в пути.",
        bonus: CRATE_VALUE * 100,
        thanks: DEFAULT_THANKS,
        logging: "Shuttle full of bees",
        spawn: lots_of_bees,
    },
    Situation {
        sender: "Центральное Командование: Отдел космопиццы",
        announcement: "Похоже, что соседняя станция случайно доставила вам свою пиццу.",
        transit: "Доставка пиццы!",
        bonus: 0,
        thanks: SHORT_THANKS,
        logging: "Pizza delivery",
        spawn: pizza_delivery,
    },
    Situation {
        sender: "Центральное Командование: Программа по работе с русскими",
        announcement: "Группа злых русских хочет устроить вечеринку. Сможете ли вы отправить им \
                       свой грузовой шаттл, а затем избавиться от них?",
        transit: "Пьяные русские уже в пути.",
        bonus: CRATE_VALUE * 50,
        thanks: DEFAULT_THANKS,
        logging: "Russian party squad",
        spawn: russian_party,
    },
    Situation {
        sender: "Центральное Командование: Дипломатический корпус",
        announcement: "Клан Паука прислал нам таинственный подарок. Можем ли мы отправить его \
                       вам, чтобы вы посмотрели, что внутри?",
        transit: "Подарок от Клана Паука в пути.",
        bonus: CRATE_VALUE * 50,
        thanks: DEFAULT_THANKS,
        logging: "Shuttle full of spiders",
        spawn: spider_gift,
    },
];

/// tg `/datum/round_event_control/shuttle_loan` `unavailable_situations`: the ones already run.
#[derive(Resource, Default)]
pub struct LoanHistory(pub Vec<usize>);

/// tg `can_spawn_event`: no other loan running.
pub fn available(world: &mut World) -> bool {
    !world
        .resource::<super::controller::Events>()
        .running()
        .any(|name| name == NAME)
}

pub fn make(_: &mut World) -> Box<dyn RoundEvent> {
    Box::new(ShuttleLoan { situation: 0 })
}

struct ShuttleLoan {
    situation: usize,
}

/// tg `setup()`'s pick: a situation not run yet, or any once all have.
pub fn pick_situation(world: &mut World) -> usize {
    let used = world.get_resource_or_init::<LoanHistory>().0.clone();
    let mut valid: Vec<usize> = (0..SITUATIONS.len())
        .filter(|i| !used.contains(i))
        .collect();
    if valid.is_empty() {
        world.resource_mut::<LoanHistory>().0.clear();
        valid = (0..SITUATIONS.len()).collect();
    }
    let pick = world
        .resource_mut::<EventRng>()
        .pick(&valid)
        .copied()
        .unwrap_or(0);
    world.resource_mut::<LoanHistory>().0.push(pick);
    pick
}

impl RoundEvent for ShuttleLoan {
    fn setup(&mut self, world: &mut World, ev: &mut EventState) {
        ev.announce_when = 1;
        ev.end_when = 500;
        self.situation = pick_situation(world);
    }

    fn announce(&mut self, world: &mut World, _: &mut EventState, fake: bool) {
        let situation = if fake {
            let all: Vec<usize> = (0..SITUATIONS.len()).collect();
            world
                .resource_mut::<EventRng>()
                .pick(&all)
                .copied()
                .unwrap_or(0)
        } else {
            if let Some(mut shuttle) = world.get_resource_mut::<SupplyShuttle>() {
                shuttle.loan = Some(LoanOffer {
                    situation: self.situation,
                    dispatched: false,
                });
            }
            self.situation
        };
        let s = &SITUATIONS[situation];
        super::priority_announce(
            world,
            s.sender,
            &format!("Отдел снабжения: {}", s.announcement),
        );
    }

    fn end(&mut self, world: &mut World, _: &mut EventState) {
        let Some(mut shuttle) = world.get_resource_mut::<SupplyShuttle>() else {
            return;
        };
        let Some(offer) = shuttle.loan.take() else {
            return;
        };
        if !offer.dispatched {
            return;
        }
        let port = shuttle.port;
        let mut turfs = empty_turfs(world, port);
        if turfs.is_empty() {
            return;
        }
        let mut spawn_list = Vec::new();
        (SITUATIONS[offer.situation].spawn)(world, &mut spawn_list, &mut turfs);
        while !spawn_list.is_empty() && !turfs.is_empty() {
            let (Some(pos), Some(path)) = (take(world, &mut turfs), take(world, &mut spawn_list))
            else {
                break;
            };
            spawn_at(world, path, pos);
        }
    }
}

/// tg `loan_shuttle()`, from a supply console that checked the shuttle idles at CentCom: the
/// thanks, the bonus, the shuttle called to the station, and the event ends at its next fire.
pub fn loan_shuttle(world: &mut World) -> bool {
    let now = world.resource::<Time>().elapsed_secs_f64();
    let Some(mut shuttle) = world.get_resource_mut::<SupplyShuttle>() else {
        return false;
    };
    let Some(offer) = shuttle.loan.filter(|o| !o.dispatched) else {
        return false;
    };
    if shuttle.home.is_none() {
        return false;
    }
    shuttle.loan = Some(LoanOffer {
        dispatched: true,
        ..offer
    });
    let situation = &SITUATIONS[offer.situation];
    shuttle.to_station = true;
    shuttle.mode = SupplyMode::Call;
    shuttle.timer.set(now, DELAY_SECS);
    super::priority_announce(
        world,
        &format!("Грузовой шаттл под командованием {}.", super::COMMAND_NAME),
        situation.thanks,
    );
    if let Some(cargo) = world.resource_mut::<Economy>().department_mut(ACCOUNT_CAR) {
        cargo.adjust_money(situation.bonus);
    }
    if let Some(mut orders) = world.get_resource_mut::<SupplyOrders>() {
        orders.centcom_message.push_str(situation.transit);
    }
    // tg `end_when = activeFor + 1`.
    super::controller::end_next_fire(world, NAME);
    info!(
        "Shuttle loan event firing with type '{}'.",
        situation.logging
    );
    true
}

/// `pick_n_take`.
fn take<T: Copy>(world: &mut World, list: &mut Vec<T>) -> Option<T> {
    if list.is_empty() {
        return None;
    }
    let i = world
        .resource_mut::<EventRng>()
        .0
        .range(0, list.len() as i64 - 1) as usize;
    Some(list.swap_remove(i))
}

fn prob(world: &mut World, percent: f32) -> bool {
    world.resource_mut::<EventRng>().0.prob(percent)
}

fn pick(world: &mut World, paths: &[&'static str]) -> &'static str {
    world
        .resource_mut::<EventRng>()
        .pick(paths)
        .copied()
        .unwrap_or(paths[0])
}

/// `new path(turf)`: a corpse spawner makes its corpse, a mob is a mob.
fn spawn_at(world: &mut World, path: &str, pos: TilePos) {
    let spawned = if path.starts_with(CORPSE) {
        crate::ghost_roles::def(world, path).map(|def| {
            let name = def.mob_name.clone().unwrap_or_else(|| def.name.clone());
            crate::ghost_roles::create_corpse(world, &def, pos, name)
        })
    } else if path.starts_with("/mob/") {
        crate::mobs::spawn::spawn_mob(world, path, pos)
    } else {
        crate::construction::spawn_obj(world, path, pos)
    };
    if spawned.is_none() {
        warn!("shuttle loan: no prototype for {path} (rerun `ss13-mapload convert`)");
    }
}

/// `pack.generate(pick_n_take(empty_shuttle_turfs))`.
fn generate(world: &mut World, pack: &str, turfs: &mut Vec<TilePos>) {
    let Some(pack) = world
        .get_resource::<SupplyPacks>()
        .and_then(|p| p.by_id(pack))
    else {
        warn!("shuttle loan: no supply pack {pack}");
        return;
    };
    if let Some(pos) = take(world, turfs) {
        generate_pack(world, pack, pos);
    }
}

fn spawn_on_free(world: &mut World, path: &str, turfs: &mut Vec<TilePos>) {
    if let Some(pos) = take(world, turfs) {
        spawn_at(world, path, pos);
    }
}

fn antidote(world: &mut World, list: &mut Vec<&'static str>, turfs: &mut Vec<TilePos>) {
    let corpse = pick(
        world,
        &[
            "/obj/effect/mob_spawn/corpse/human/assistant/beesease_infection",
            "/obj/effect/mob_spawn/corpse/human/assistant/brainrot_infection",
            "/obj/effect/mob_spawn/corpse/human/assistant/spanishflu_infection",
        ],
    );
    for _ in 0..10 {
        if prob(world, 15.0) {
            list.push("/obj/item/reagent_containers/cup/bottle");
        } else if prob(world, 15.0) {
            list.push("/obj/item/reagent_containers/syringe");
        } else if prob(world, 25.0) {
            list.push("/obj/item/shard");
        }
        spawn_on_free(world, corpse, turfs);
    }
    list.push("/obj/structure/closet/crate");
    list.push("/obj/item/reagent_containers/cup/bottle/pierrot_throat");
    list.push("/obj/item/reagent_containers/cup/bottle/magnitis");
}

fn department_resupply(world: &mut World, _: &mut Vec<&'static str>, turfs: &mut Vec<TilePos>) {
    for pack in [
        "/datum/supply_pack/emergency/equipment",
        "/datum/supply_pack/security/supplies",
        "/datum/supply_pack/organic/food",
        "/datum/supply_pack/emergency/weedcontrol",
        "/datum/supply_pack/engineering/tools",
        "/datum/supply_pack/engineering/engiequipment",
        "/datum/supply_pack/science/robotics",
        "/datum/supply_pack/science/plasma",
        "/datum/supply_pack/medical/supplies",
    ] {
        generate(world, pack, turfs);
    }
    for _ in 0..5 {
        let decal = pick(
            world,
            &[
                "/obj/effect/decal/cleanable/food/flour",
                "/obj/effect/decal/cleanable/blood/gibs/robot_debris",
                "/obj/effect/decal/cleanable/blood/oil",
            ],
        );
        spawn_on_free(world, decal, turfs);
    }
}

const INFILTRATOR: &str = "/mob/living/basic/trooper/syndicate/ranged/infiltrator";

fn syndie_hijacking(world: &mut World, list: &mut Vec<&'static str>, turfs: &mut Vec<TilePos>) {
    generate(world, "/datum/supply_pack/imports/specialops", turfs);
    list.extend([INFILTRATOR, INFILTRATOR]);
    if prob(world, 75.0) {
        list.push(INFILTRATOR);
    }
    if prob(world, 50.0) {
        list.push(INFILTRATOR);
    }
}

fn lots_of_bees(world: &mut World, list: &mut Vec<&'static str>, turfs: &mut Vec<TilePos>) {
    generate(
        world,
        "/datum/supply_pack/organic/hydroponics/beekeeping_fullkit",
        turfs,
    );
    list.extend([
        "/obj/effect/mob_spawn/corpse/human/bee_terrorist",
        "/obj/effect/mob_spawn/corpse/human/cargo_tech",
        "/obj/effect/mob_spawn/corpse/human/cargo_tech",
        "/obj/effect/mob_spawn/corpse/human/nanotrasensoldier",
        "/obj/item/gun/ballistic/automatic/pistol/no_mag",
        "/obj/item/gun/ballistic/automatic/pistol/m1911/no_mag",
        "/obj/item/honey_frame",
        "/obj/item/honey_frame",
        "/obj/item/honey_frame",
        "/obj/structure/beebox/unwrenched",
        "/obj/item/queen_bee/bought",
        "/obj/structure/closet/crate/hydroponics",
    ]);
    list.extend(["/mob/living/basic/bee/toxin"; 8]);
    for _ in 0..5 {
        let decal = pick(
            world,
            &[
                "/obj/effect/decal/cleanable/blood",
                "/obj/effect/decal/cleanable/insectguts",
            ],
        );
        spawn_on_free(world, decal, turfs);
    }
    for _ in 0..10 {
        spawn_on_free(world, "/obj/item/ammo_casing/spent", turfs);
    }
}

fn pizza_delivery(world: &mut World, list: &mut Vec<&'static str>, _: &mut Vec<TilePos>) {
    const NAUGHTY: [&str; 2] = [
        "/obj/item/pizzabox/bomb",
        "/obj/item/pizzabox/margherita/robo",
    ];
    const NICE: [&str; 4] = [
        "/obj/item/pizzabox/margherita",
        "/obj/item/pizzabox/meat",
        "/obj/item/pizzabox/vegetable",
        "/obj/item/pizzabox/mushroom",
    ];
    for _ in 0..6 {
        let pool: &[&'static str] = if prob(world, 5.0) { &NAUGHTY } else { &NICE };
        list.push(pick(world, pool));
    }
}

fn russian_party(world: &mut World, list: &mut Vec<&'static str>, turfs: &mut Vec<TilePos>) {
    generate(world, "/datum/supply_pack/service/party", turfs);
    list.extend([
        "/mob/living/basic/trooper/russian",
        "/mob/living/basic/trooper/russian/ranged",
        "/mob/living/basic/bear/russian",
    ]);
    if prob(world, 75.0) {
        list.push("/mob/living/basic/trooper/russian");
    }
    if prob(world, 50.0) {
        list.push("/mob/living/basic/bear/russian");
    }
}

fn spider_gift(world: &mut World, list: &mut Vec<&'static str>, turfs: &mut Vec<TilePos>) {
    generate(world, "/datum/supply_pack/imports/specialops", turfs);
    list.extend([
        "/mob/living/basic/spider/giant",
        "/mob/living/basic/spider/giant",
        "/mob/living/basic/spider/giant/nurse",
    ]);
    if prob(world, 50.0) {
        list.push("/mob/living/basic/spider/giant/hunter");
    }
    if let Some(victim) = take(world, turfs) {
        for path in [
            "/obj/effect/decal/remains/human",
            "/obj/item/clothing/shoes/jackboots/fast",
            "/obj/item/clothing/mask/balaclava",
        ] {
            spawn_at(world, path, victim);
        }
    }
    for _ in 0..5 {
        spawn_on_free(world, "/obj/structure/spider/stickyweb", turfs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn situations_have_texts_and_logging() {
        for s in &SITUATIONS {
            assert!(!s.sender.is_empty() && !s.announcement.is_empty());
            assert!(!s.transit.is_empty() && !s.logging.is_empty());
            assert!(s.bonus >= 0);
        }
        assert!(ss13_mapload::runtime::SHUTTLE_LOAN_RUNTIME_TYPES.contains(&INFILTRATOR));
    }

    #[test]
    fn situations_come_once_until_all_ran() {
        let mut world = World::new();
        world.insert_resource(EventRng(ss13_mapload::spawner::Rng::new(7)));
        let mut seen: Vec<usize> = (0..SITUATIONS.len())
            .map(|_| pick_situation(&mut world))
            .collect();
        seen.sort_unstable();
        assert_eq!(seen, (0..SITUATIONS.len()).collect::<Vec<_>>());
        // All ran: the pool refills.
        pick_situation(&mut world);
        assert_eq!(world.resource::<LoanHistory>().0.len(), 1);
    }
}
