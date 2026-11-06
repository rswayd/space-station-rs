//! The virtual domains, after tg `/datum/lazy_template/virtual_domain`
//! (`code/modules/bitrunning/virtual_domain/virtual_domain.dm`, `domains/*.dm`) and
//! BandaStation's `modular_bandastation/domains/code/domains.dm`: the vars the quantum server
//! and console read. Each loads `_maps/virtual_domains/<map_name>.dmm`.

/// `BITRUNNER_COST_*`.
pub const COST_NONE: i32 = 0;
pub const COST_LOW: i32 = 1;
pub const COST_MEDIUM: i32 = 2;
pub const COST_HIGH: i32 = 3;
pub const COST_EXTREME: i32 = 20;

/// `BITRUNNER_REWARD_*`.
pub const REWARD_MIN: i32 = 1;
pub const REWARD_LOW: i32 = 3;
pub const REWARD_MEDIUM: i32 = 4;
pub const REWARD_HIGH: i32 = 5;
pub const REWARD_EXTREME: i32 = 6;

/// `BITRUNNER_DIFFICULTY_*`.
pub const DIFFICULTY_NONE: u8 = 0;
pub const DIFFICULTY_LOW: u8 = 1;
pub const DIFFICULTY_MEDIUM: u8 = 2;
pub const DIFFICULTY_HIGH: u8 = 3;

/// `DOMAIN_NO_NOHIT_BONUS`.
pub const NO_NOHIT_BONUS: u8 = 1 << 2;
/// `DOMAIN_TEST_ONLY`.
pub const TEST_ONLY: u8 = 1 << 3;

/// `/datum/outfit/job/miner`, the lavaland bosses' forced outfit.
const MINER: Option<&str> = Some("/datum/outfit/job/miner");

/// One `/datum/lazy_template/virtual_domain` subtype.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Domain {
    /// `key`: the console's `id`.
    pub key: &'static str,
    /// `map_name`.
    pub map: &'static str,
    pub name: &'static str,
    pub desc: &'static str,
    pub help_text: &'static str,
    pub cost: i32,
    pub difficulty: u8,
    pub reward_points: i32,
    pub flags: u8,
    /// `forced_outfit`.
    pub forced_outfit: Option<&'static str>,
    /// `completion_loot`: type, amount.
    pub completion_loot: &'static [(&'static str, u32)],
    pub is_modular: bool,
    /// `counterlist_sum(secondary_loot) > 0`.
    pub has_secondary: bool,
    /// `announce_to_ghosts`.
    pub announce_ghosts: bool,
}

impl Domain {
    /// `can_view_name`.
    pub fn can_view_name(&self, points: i32) -> bool {
        self.cost <= points + 5
    }

    /// `can_view_reward`.
    pub fn can_view_reward(&self, scanner_tier: u32, points: i32) -> bool {
        u32::from(self.difficulty) < scanner_tier + 1 && self.cost <= points + 3
    }
}

const BASE: Domain = Domain {
    key: "",
    map: "",
    name: "Virtual Domain",
    desc: "A map.",
    help_text: "",
    cost: COST_NONE,
    difficulty: DIFFICULTY_NONE,
    reward_points: REWARD_MIN,
    flags: 0,
    forced_outfit: None,
    completion_loot: &[],
    is_modular: false,
    has_secondary: false,
    announce_ghosts: false,
};

/// `SSbitrunning.all_domains`.
pub const DOMAINS: &[Domain] = &[
    Domain {
        key: "abductor_ship",
        map: "abductor_ship",
        name: "Корабль абдукторов",
        desc: "Высадитесь на корабль абдукторов и заберите их ценности.",
        help_text: "Материнский корабль абдукторов непреднамеренно вошел во враждебное окружение. \
                    В настоящее время они готовятся покинуть зону, прихватив своё снаряжение и \
                    добычу, включая ящик. Будьте осторожны, так как абдукторы известны своим \
                    продвинутым вооружением.",
        cost: COST_MEDIUM,
        difficulty: DIFFICULTY_MEDIUM,
        reward_points: REWARD_MEDIUM,
        forced_outfit: Some("/datum/outfit/bitductor"),
        completion_loot: &[("/obj/item/toy/plush/abductor/agent", 1)],
        is_modular: true,
        ..BASE
    },
    Domain {
        key: "ash_drake",
        map: "ash_drake",
        name: "Пепельный ад",
        desc: "Здесь обитает пепельный дрейк, могущественный дракон, который бороздит \
               поверхность Лаваланда.",
        cost: COST_MEDIUM,
        difficulty: DIFFICULTY_MEDIUM,
        reward_points: REWARD_MEDIUM,
        forced_outfit: MINER,
        ..BASE
    },
    Domain {
        key: "beach_bar",
        map: "beach_bar",
        name: "Бар на пляже",
        desc: "Веселая пляжная обитель, где дружелюбные скелеты подают напитки. Ребят, а как вы \
               погибли?",
        help_text: "Это место работает на скелетной команде, и они не очень хотят делиться \
                    информацией. Может быть, парочка рюмочек жидкого очарования поднимут им \
                    настроение. Как говорится, если не можешь их победить - присоединись к ним!",
        completion_loot: &[("/obj/item/toy/beach_ball", 1)],
        flags: NO_NOHIT_BONUS,
        ..BASE
    },
    Domain {
        key: "blood_drunk_miner",
        map: "blood_drunk_miner",
        name: "Кровавые раскопки",
        desc: "Немногие покидают поверхность Лаваленда без парочки шрамов. Другие остаются, \
               обезумев от охоты.",
        cost: COST_MEDIUM,
        difficulty: DIFFICULTY_MEDIUM,
        reward_points: REWARD_MEDIUM,
        forced_outfit: MINER,
        ..BASE
    },
    Domain {
        key: "breeze_bay",
        map: "breeze_bay",
        name: "Прохладный залив",
        desc: "Городок на берегу моря с большим лесом на севере.",
        help_text: "Всё просто! Наслаждайтесь лучами, половите рыб и хорошо проведите время! \
                    Только не дайте крабам куснуть вас.",
        reward_points: REWARD_LOW,
        flags: NO_NOHIT_BONUS,
        ..BASE
    },
    Domain {
        key: "bubblegum",
        map: "bubblegum",
        name: "Пропитанное кровью логово",
        desc: "Король демонов резни. Бубльгум - это массивное, громоздкое чудовище, жаждущее \
               насилия.",
        cost: COST_HIGH,
        difficulty: DIFFICULTY_HIGH,
        reward_points: REWARD_HIGH,
        forced_outfit: MINER,
        completion_loot: &[("/obj/item/toy/plush/bubbleplush", 1)],
        ..BASE
    },
    Domain {
        key: "clown_planet",
        map: "clown_planet",
        name: "Планета клоунов",
        desc: "В самых темных уголках космоса есть только Хонк.",
        help_text: "Испытания Хонкости начались. Вдалики раздаются звуки велосипедных гудков. \
                    Этот мир - своего рода головоломка - существует в легендах как последняя \
                    проверка вашей смешнявости.",
        cost: COST_LOW,
        difficulty: DIFFICULTY_LOW,
        reward_points: REWARD_LOW,
        forced_outfit: Some("/datum/outfit/job/clown"),
        completion_loot: &[("/obj/item/bikehorn", 1)],
        ..BASE
    },
    Domain {
        key: "colossus",
        map: "colossus",
        name: "Небесное испытание",
        desc: "Массивное древнее чудовище по имени Колосс. Суд грядёт.",
        cost: COST_HIGH,
        difficulty: DIFFICULTY_HIGH,
        reward_points: REWARD_HIGH,
        forced_outfit: MINER,
        ..BASE
    },
    Domain {
        key: "fredingtonfastingbear",
        map: "fredingtonfastingbear",
        name: "Вечеринка с пиццей",
        desc: "Знаменитая пиццерия, которая разорилась из-за того, что могла предложить.",
        help_text: "Бери фонарик и отправляйся в путь. Твой любимый медведь уже заждался тебя...",
        cost: COST_MEDIUM,
        difficulty: DIFFICULTY_MEDIUM,
        reward_points: REWARD_MEDIUM,
        completion_loot: &[("/obj/item/food/pizzaslice/meat/pizzeria", 1)],
        has_secondary: true,
        ..BASE
    },
    Domain {
        key: "gondola_asteroid",
        map: "gondola_asteroid",
        name: "Астероид с гондолами",
        desc: "Этот астероид является домом для обильного леса с гондолами. Умиротворительно.",
        help_text: "Какой чудесный лес. Нужный нам ящик находится в центре леса. Странно, ящик \
                    не толкнуть. А вот гондолы, похоже, без проблем его передвигают. Наверняка \
                    есть способ сдвинуть его своими силами.",
        flags: NO_NOHIT_BONUS,
        ..BASE
    },
    Domain {
        key: "grasslands_hunt",
        map: "grasslands_hunt",
        name: "Луговая охота",
        desc: "Мирная охота в дикой местности.",
        help_text: "Как охотник, вы должны уметь выслеживать и убивать свою добычу. Проявите \
                    себя.",
        is_modular: true,
        flags: NO_NOHIT_BONUS,
        ..BASE
    },
    Domain {
        key: "heretic_hunt",
        map: "heretic_hunt",
        name: "Еретическая охота",
        desc: "Предай своего ближнего, чтобы получить абсолютную силу.",
        help_text: "Еретики требуют жертвоприношений для своих тёмных ритуалов - приносите трупы \
                    обратно на место ритуала! Трупы членов экипажа более высокого ранга \
                    представляют большую ценность и могут содержать полезное обмундирование.",
        cost: COST_LOW,
        difficulty: DIFFICULTY_LOW,
        reward_points: REWARD_LOW,
        forced_outfit: Some("/datum/outfit/virtual_domain_heretic"),
        ..BASE
    },
    Domain {
        key: "hierophant",
        map: "hierophant",
        name: "Арена ревнителя",
        desc: "Танцуйте, марионетки, танцуйте!",
        cost: COST_HIGH,
        difficulty: DIFFICULTY_HIGH,
        reward_points: REWARD_HIGH,
        forced_outfit: MINER,
        ..BASE
    },
    Domain {
        key: "island_brawl",
        map: "island_brawl",
        name: "Островная потасовка",
        desc: "Мирный остров, спрятанный в Богом забытом месте.. Этот уровень будет \
               автоматически завершен после того, как произойдет некоторое количесво смертей.",
        help_text: "По всему острову могут быть назначены награды, но главная цель - выжить. \
                    Смерти на острове будут засчитаны в итоговый счет.",
        cost: COST_HIGH,
        difficulty: DIFFICULTY_HIGH,
        reward_points: REWARD_HIGH,
        forced_outfit: Some("/datum/outfit/beachbum_combat"),
        has_secondary: true,
        announce_ghosts: true,
        ..BASE
    },
    Domain {
        key: "meta_central",
        map: "meta_central",
        name: "ИСН Цереброн-централ",
        desc: "Время от времени работники требуют соблюдения прав от Нанотрейзен. Это \
               невыгодно.",
        help_text: "Отвечайте на требования работника санкционированным насилием. Соберите \
                    ценные материалы, которые могут быть разбросаны вокруг. Просто помните чему \
                    вас учили: все виновны по умолчанию, свою вину они признают в мед отделе... \
                    ну или что-то типо такого.",
        cost: COST_LOW,
        difficulty: DIFFICULTY_LOW,
        reward_points: REWARD_LOW,
        forced_outfit: Some("/datum/outfit/job/security/mod"),
        is_modular: true,
        announce_ghosts: true,
        ..BASE
    },
    Domain {
        key: "pipedream",
        map: "pipedream",
        name: "Завод труб отправки",
        desc: "Заброшенный и зараженный завод по производству труб отправки.",
        help_text: "Не так давно здесь кипела бурная деятельность. Рабочие, похоже, уходили в \
                    спешке, и производительность теперь на нуле. Что-то, должно быть, разгромило \
                    это место, но что?",
        cost: COST_LOW,
        difficulty: DIFFICULTY_LOW,
        reward_points: REWARD_LOW,
        completion_loot: &[("/obj/item/stack/pipe_cleaner_coil/random/five", 1)],
        is_modular: true,
        ..BASE
    },
    Domain {
        key: "pirates",
        map: "pirates",
        name: "Бухта корсаров",
        desc: "Пробейтесь с боем к спрятанным сокровищам, захватите добычу и поспешите сбежать, \
               прежде чем пираты переломят ситуацию.",
        help_text: "Переодентесь в предоставленную одежду, чтобы слиться с толпой, а затем \
                    сразитесь с враждебными пиратами. Хватайте сокровища и выбирайтесь, пока вас \
                    не одолели!",
        cost: COST_MEDIUM,
        difficulty: DIFFICULTY_MEDIUM,
        reward_points: REWARD_MEDIUM,
        announce_ghosts: true,
        ..BASE
    },
    Domain {
        key: "psyker_shuffle",
        map: "psyker_shuffle",
        name: "Хаос из ящиков",
        desc: "Проберитесь в заброшенный уголок виртуального мира, где хранятся все ящики. \
               Внимание - виртуальный домен не поддерживает визуальное отображение и использует \
               эхолокацию.",
        help_text: "Привыкнуть к эхолокации может быть непросто. Не забывайте переходить на шаг и \
                    внимательно осматривать каждый встреченный ящик.",
        cost: COST_LOW,
        difficulty: DIFFICULTY_MEDIUM,
        reward_points: REWARD_HIGH,
        forced_outfit: Some("/datum/outfit/echolocator"),
        ..BASE
    },
    Domain {
        key: "psyker_zombies",
        map: "psyker_zombies",
        name: "Зараженный домен",
        desc: "Еще один заброшенный уголок виртуального мира. Этот мир был заброшен из-за \
               зомби-вируса. Внимание - виртуальный домен не поддерживает визуальное отображение \
               и использует эхолокацию.",
        help_text: "Этот некогда любимый виртуальный домен был поврежден вирусом, в результате \
                    чего он стал нестабильным, полным дыр и ЗОМБИ! Поблизости где-то должен быть \
                    Загадочный ящик, который поможет вам вооружиться. Вооружитесь и закончите \
                    начатое киберполицией!",
        cost: COST_MEDIUM,
        difficulty: DIFFICULTY_MEDIUM,
        reward_points: REWARD_HIGH,
        forced_outfit: Some("/datum/outfit/echolocator"),
        completion_loot: &[("/obj/item/radio/headset/psyker", 1)],
        ..BASE
    },
    Domain {
        key: "stairs_and_cliffs",
        map: "stairs_and_cliffs",
        name: "Ледниковый откос",
        desc: "Коварный подъем, который осилят немногие. Зато это отличное кардио.",
        help_text: "Слышали когда-нибудь о \"Змеях и Лестницах\"? Так вот, это тоже самое, но \
                    вместо лестниц - подъемы, а вместо змей - крутой спуск с обрыва на жесткие \
                    камни или в жидкую плазму.",
        cost: COST_LOW,
        difficulty: DIFFICULTY_LOW,
        reward_points: REWARD_MEDIUM,
        forced_outfit: Some("/datum/outfit/job/virtual_domain_iceclimber"),
        completion_loot: &[("/obj/item/clothing/suit/costume/snowman", 2)],
        has_secondary: true,
        flags: NO_NOHIT_BONUS,
        ..BASE
    },
    Domain {
        key: "syndicate_assault",
        map: "syndicate_assault",
        name: "Нападение Синдиката",
        desc: "Возьмите на абордаж вражеский корабль и верните украденный груз.",
        help_text: "Группа из оперативников Синдиката похитила со станции ценный груз. Они \
                    поднялись на борт своего корабля и пытаются сбежать. Проникните на их \
                    корабль и верните ящик. Будьте осторожны, они очень хорошо вооружены.",
        cost: COST_MEDIUM,
        difficulty: DIFFICULTY_MEDIUM,
        reward_points: REWARD_MEDIUM,
        completion_loot: &[("/obj/item/toy/plush/nukeplushie", 1)],
        is_modular: true,
        announce_ghosts: true,
        ..BASE
    },
    Domain {
        key: "test_only",
        map: "test_only",
        name: "Test Only",
        flags: TEST_ONLY,
        ..BASE
    },
    Domain {
        key: "test_only_expensive",
        map: "test_only",
        name: "Test Only",
        cost: 3,
        flags: TEST_ONLY,
        ..BASE
    },
    Domain {
        key: "vaporwave",
        map: "vaporwave",
        name: "Космический пережиток",
        desc: "Зависшая в безмолвной пустоте космоса Неоновая реликвия - это призрачное эхо \
               ретрофутуристической эпохи. Отдохните и насладитесь видом.",
        cost: COST_EXTREME,
        reward_points: REWARD_EXTREME,
        completion_loot: &[("/obj/item/stack/spacecash/c500", 4)],
        flags: NO_NOHIT_BONUS,
        ..BASE
    },
    Domain {
        key: "wendigo",
        map: "wendigo",
        name: "Ледяной пожиратель",
        desc: "Легенды гласят о хищных Вендиго, скрытых в глубине пещер Айсмуна.",
        cost: COST_HIGH,
        difficulty: DIFFICULTY_HIGH,
        reward_points: REWARD_HIGH,
        forced_outfit: MINER,
        ..BASE
    },
    Domain {
        key: "xeno_nest",
        map: "xeno_nest",
        name: "Заражение ксеноморфами",
        desc: "Сканеры нашего корабля обнаружили формы жизни неизвестного происхождения. \
               Дружественные попытки связаться с ними не увенчались успехом.",
        help_text: "Вы находитесь на бесплодной планете, наполненной враждебными существами. \
                    Где-то здесь ящик, хоть он и не спрятан, но находится под охраной. Ожидайте \
                    сопротивления.",
        cost: COST_LOW,
        difficulty: DIFFICULTY_LOW,
        reward_points: REWARD_LOW,
        completion_loot: &[("/obj/item/toy/plush/rouny", 1)],
        is_modular: true,
        ..BASE
    },
    // BandaStation `modular_bandastation/domains`.
    Domain {
        key: "inka_base",
        map: "inka_base",
        name: "Зачистка Инки",
        desc: "Зачистите базу Нанотрейзен во время нападения синдиката и заберите секретные \
               данные.",
        help_text: "Группа из оперативников Синдиката нападает на секретную базу Нанотрейзен. \
                    Они только заходят на объект. Проникните на защищённую базу и заберите \
                    секретные документы. Будьте осторожны, защитники и нападающие очень хорошо \
                    вооружены.",
        cost: COST_HIGH,
        difficulty: DIFFICULTY_HIGH,
        reward_points: REWARD_HIGH,
        forced_outfit: Some("/datum/outfit/tsf/bitrun"),
        completion_loot: &[("/obj/item/toy/plush/hampter/ert", 1)],
        is_modular: true,
        announce_ghosts: true,
        ..BASE
    },
    Domain {
        key: "water_grot",
        map: "water_grot",
        name: "Водный сплав",
        desc: "Доплыть до конца.",
        help_text: "Награда ждет тебя в самом конце.",
        cost: COST_LOW,
        difficulty: DIFFICULTY_LOW,
        reward_points: REWARD_LOW,
        ..BASE
    },
];

/// The domain with `key`.
pub fn by_key(key: &str) -> Option<&'static Domain> {
    DOMAINS.iter().find(|d| d.key == key)
}

/// `get_random_domain_id` candidates: not test-only, affordable, not free, not extreme.
pub fn random_candidates(points: i32) -> Vec<&'static Domain> {
    if points < 1 {
        return Vec::new();
    }
    DOMAINS
        .iter()
        .filter(|d| {
            d.flags & TEST_ONLY == 0
                && d.cost <= points
                && d.cost > COST_NONE
                && d.cost < COST_EXTREME
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_and_loot_is_registered() {
        let registry = ss13_mapload::runtime::runtime_types();
        for (i, d) in DOMAINS.iter().enumerate() {
            assert!(DOMAINS[i + 1..].iter().all(|o| o.key != d.key), "{}", d.key);
            for (path, _) in d.completion_loot {
                assert!(registry.contains(path), "{path} not a runtime type");
            }
        }
    }

    #[test]
    fn visibility_and_random_picks_follow_tg() {
        let drake = by_key("ash_drake").unwrap();
        assert!(drake.can_view_name(0));
        assert!(!by_key("vaporwave").unwrap().can_view_name(14));
        // Medium difficulty needs a tier 2 scanner.
        assert!(!drake.can_view_reward(1, 5));
        assert!(drake.can_view_reward(2, 0));
        assert!(random_candidates(0).is_empty());
        let picks = random_candidates(1);
        assert!(!picks.is_empty());
        assert!(picks.iter().all(|d| d.cost == COST_LOW));
        assert!(random_candidates(100).iter().all(|d| d.cost < COST_EXTREME));
    }
}
